//! Область навигации: Главная, Галерея, закреплённые папки, Этот компьютер, Сеть

use std::path::PathBuf;

use egui::{Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::menu::{MenuItem, Popup};
use super::widgets::*;
use crate::app::{now, Location, OxideApp};
use crate::cmd::Cmd;
use crate::shell::{fs, menu as smenu, shellfolder};
use crate::theme::m;

enum NIcon {
    Glyph(char, Color32),
    Sys(String),
}

struct Row {
    key: String,
    label: String,
    depth: usize,
    loc: Location,
    icon: NIcon,
    expandable: bool,
    pinned: bool,
    sep: bool,
}

fn sep() -> Row {
    Row { key: String::new(), label: String::new(), depth: 0, loc: Location::Home, icon: NIcon::Glyph(' ', Color32::TRANSPARENT), expandable: false, pinned: false, sep: true }
}

fn push_children(app: &mut OxideApp, rows: &mut Vec<Row>, key: &str, path: &PathBuf, depth: usize) {
    if !app.nav.expanded.contains(key) {
        return;
    }
    if !app.nav.children.contains_key(key) {
        let kids = fs::subdirs(path, app.settings.show_hidden);
        app.nav.children.insert(key.to_string(), kids);
    }
    let kids = app.nav.children.get(key).cloned().unwrap_or_default();
    for (label, p) in kids {
        let k = format!("p:{}", crate::util::lower(&p.to_string_lossy()));
        rows.push(Row {
            key: k.clone(),
            label,
            depth,
            loc: Location::Path(p.clone()),
            icon: NIcon::Sys(p.to_string_lossy().to_string()),
            expandable: true,
            pinned: false,
            sep: false,
        });
        push_children(app, rows, &k, &p, depth + 1);
    }
}

fn build(app: &mut OxideApp) -> Vec<Row> {
    let pal = app.pal.clone();
    let mut rows = vec![
        Row { key: "home".into(), label: "Главная".into(), depth: 0, loc: Location::Home, icon: NIcon::Glyph(ICON_HOME, pal.accent), expandable: false, pinned: false, sep: false },
        Row {
            key: "gallery".into(),
            label: "Галерея".into(),
            depth: 0,
            loc: Location::Shell(shellfolder::GALLERY.into()),
            icon: NIcon::Glyph(ICON_GALLERY, pal.accent),
            expandable: false,
            pinned: false,
            sep: false,
        },
    ];
    if let Ok(od) = std::env::var("OneDrive") {
        let p = PathBuf::from(&od);
        if p.exists() {
            let k = "onedrive".to_string();
            rows.push(Row { key: k.clone(), label: "OneDrive".into(), depth: 0, loc: Location::Path(p.clone()), icon: NIcon::Glyph(ICON_CLOUD, pal.accent), expandable: true, pinned: false, sep: false });
            push_children(app, &mut rows, &k, &p, 1);
        }
    }
    rows.push(sep());
    let pinned: Vec<(String, PathBuf)> = app.quick.iter().filter(|q| q.2).map(|q| (q.0.clone(), q.1.clone())).collect();
    for (label, p) in pinned {
        let k = format!("qa:{}", crate::util::lower(&p.to_string_lossy()));
        rows.push(Row { key: k.clone(), label, depth: 0, loc: Location::Path(p.clone()), icon: NIcon::Sys(p.to_string_lossy().to_string()), expandable: true, pinned: true, sep: false });
        push_children(app, &mut rows, &k, &p, 1);
    }
    rows.push(sep());
    rows.push(Row { key: "thispc".into(), label: "Этот компьютер".into(), depth: 0, loc: Location::ThisPc, icon: NIcon::Glyph(ICON_PC, pal.accent), expandable: true, pinned: false, sep: false });
    if app.nav.expanded.contains("thispc") {
        let drives = app.drives.clone();
        for d in drives {
            let k = format!("drv:{}", d.root.to_string_lossy().to_lowercase());
            rows.push(Row {
                key: k.clone(),
                label: d.label.clone(),
                depth: 1,
                loc: Location::Path(d.root.clone()),
                icon: NIcon::Sys(d.root.to_string_lossy().to_string()),
                expandable: true,
                pinned: false,
                sep: false,
            });
            push_children(app, &mut rows, &k, &d.root, 2);
        }
    }
    rows.push(Row {
        key: "network".into(),
        label: "Сеть".into(),
        depth: 0,
        loc: Location::Shell(shellfolder::NETWORK.into()),
        icon: NIcon::Sys(shellfolder::NETWORK.into()),
        expandable: false,
        pinned: false,
        sep: false,
    });
    rows.push(Row {
        key: "bin".into(),
        label: "Корзина".into(),
        depth: 0,
        loc: Location::Shell(shellfolder::RECYCLE_BIN.into()),
        icon: NIcon::Sys(shellfolder::RECYCLE_BIN.into()),
        expandable: false,
        pinned: false,
        sep: false,
    });
    rows
}

pub fn show(app: &mut OxideApp, ui: &mut Ui) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let rows = build(app);
    let row_h = if app.settings.compact { m::NAV_ROW_H_COMPACT } else { m::NAV_ROW_H };
    let pane_rect = ui.max_rect();
    let pane_hovered = ui.rect_contains_pointer(pane_rect);
    let cur = app.tab().loc.clone();
    let ppp = ctx.pixels_per_point();
    let mut nav_to: Option<(Location, bool)> = None;
    let mut toggle: Option<String> = None;
    let mut menu_for: Option<(Location, bool, Pos2)> = None;
    let scroll_to = std::mem::take(&mut app.nav_scroll_to);
    let drag_hover = app.drag_hover.clone();

    egui::ScrollArea::vertical().id_salt("nav").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(4.0);
        let w = ui.available_width();
        for (i, row) in rows.iter().enumerate() {
            if row.sep {
                let (r, _) = ui.allocate_exact_size(Vec2::new(w, 9.0), Sense::hover());
                hline(ui.painter(), r.center().y, r.left() + 12.0, r.right() - 12.0, pal.divider);
                continue;
            }
            let (r, resp) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::click());
            let hl = Rect::from_min_max(Pos2::new(r.left() + 4.0, r.top() + 2.0), Pos2::new(r.right() - 4.0, r.bottom() - 2.0));
            let selected = row.loc == cur && !matches!(row.loc, Location::Home) || (matches!(row.loc, Location::Home) && cur == Location::Home);
            let selected = selected && !(row.pinned && false);
            if let Location::Path(p) = &row.loc {
                app.drop_targets.items.push((r, p.clone()));
            }
            let t = ctx.animate_bool_with_time(resp.id.with("hv"), resp.hovered(), m::T_HOVER);
            let p = ui.painter();
            if selected {
                p.rect_filled(hl, CornerRadius::same(m::R_CTRL), if resp.hovered() { pal.fill(pal.selected_hover) } else { pal.fill(pal.nav_selected) });
            } else if t > 0.0 {
                p.rect_filled(hl, CornerRadius::same(m::R_CTRL), pal.fill(pal.hover.gamma_multiply(t)));
            }
            if let (Some(dh), Location::Path(rp)) = (&drag_hover, &row.loc) {
                if dh == rp {
                    p.rect_stroke(hl, CornerRadius::same(m::R_CTRL), Stroke::new(1.0, pal.accent), StrokeKind::Inside);
                }
            }
            if selected && scroll_to {
                ui.scroll_to_rect(r, None);
            }
            let x0 = r.left() + 8.0 + row.depth as f32 * m::NAV_INDENT;
            let cy = r.center().y;
            let expanded = app.nav.expanded.contains(&row.key);
            let known_empty = app.nav.children.get(&row.key).map(|c| c.is_empty()).unwrap_or(false);
            if row.expandable && !known_empty && (pane_hovered || expanded) {
                let cr = Rect::from_center_size(Pos2::new(x0 + 8.0, cy), Vec2::new(16.0, 20.0));
                let cresp = ui.interact(cr, Id::new(("nav-chev", i, &row.key)), Sense::click());
                let ang = ctx.animate_bool_with_time(Id::new(("nav-rot", &row.key)), expanded, if app.animations { m::T_MENU } else { 0.0 });
                let col = if cresp.hovered() { pal.text } else { pal.text3 };
                // Шеврон поворачивается при раскрытии
                let c = cr.center();
                let s = 3.5;
                let rot = |v: Vec2| {
                    let a = ang * std::f32::consts::FRAC_PI_2;
                    Vec2::new(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos())
                };
                let pts = [c + rot(Vec2::new(-s / 2.0, -s)), c + rot(Vec2::new(s / 2.0, 0.0)), c + rot(Vec2::new(-s / 2.0, s))];
                ui.painter().line_segment([pts[0], pts[1]], Stroke::new(1.2, col));
                ui.painter().line_segment([pts[1], pts[2]], Stroke::new(1.2, col));
                if cresp.clicked() {
                    toggle = Some(row.key.clone());
                }
            }
            let ic = Pos2::new(x0 + 28.0, cy);
            match &row.icon {
                NIcon::Glyph(c, col) => glyph(ui.painter(), ic, *c, 16.0, *col),
                NIcon::Sys(path) => {
                    let px = (16.0 * ppp).round() as u32;
                    let key = if path.len() > 3 && fs::is_dir(std::path::Path::new(path)) && !std::path::Path::new(path).join("desktop.ini").exists() {
                        "<dir>".to_string()
                    } else {
                        path.clone()
                    };
                    if let Some(tex) = app.icons.get(&key, path, px, false) {
                        paint_texture(ui.painter(), tex, ic, 16.0);
                    } else if app.icons.failed(&key, px) {
                        let g = match row.key.as_str() {
                            "network" => ICON_NETWORK,
                            "bin" => ICON_DELETE,
                            _ => ICON_FOLDER,
                        };
                        glyph(ui.painter(), ic, g, 16.0, pal.accent);
                    }
                }
            }
            let tx = x0 + 44.0;
            let pin_w = if row.pinned { 22.0 } else { 0.0 };
            text_trunc(ui.painter(), Pos2::new(tx, cy), &row.label, font(12.0), pal.text, r.right() - 8.0 - pin_w - tx);
            if row.pinned {
                glyph(ui.painter(), Pos2::new(r.right() - 18.0, cy), ICON_PIN, 12.0, pal.text3);
            }
            if resp.clicked() {
                nav_to = Some((row.loc.clone(), false));
            }
            if resp.double_clicked() && row.expandable {
                toggle = Some(row.key.clone());
            }
            if resp.middle_clicked() {
                nav_to = Some((row.loc.clone(), true));
            }
            if resp.secondary_clicked() {
                menu_for = Some((row.loc.clone(), row.pinned, resp.interact_pointer_pos().unwrap_or(r.center())));
            }
            let _ = resp.on_hover_text_at_pointer(row.label.clone());
        }
        ui.add_space(8.0);
    });

    if let Some(k) = toggle {
        if !app.nav.expanded.remove(&k) {
            app.nav.expanded.insert(k.clone());
            app.nav.children.remove(&k);
        }
        app.mark_settings(&ctx);
    }
    if let Some((loc, new_tab)) = nav_to {
        if new_tab {
            app.open_tab(&ctx, loc, false);
        } else if loc != app.tab().loc || app.tab().searching() {
            app.navigate(&ctx, loc);
        }
    }
    if let Some((loc, pinned, pos)) = menu_for {
        let items = match &loc {
            Location::Path(p) => {
                let mut v = vec![
                    MenuItem::new("Открыть в новой вкладке", Cmd::OpenInNewTabPath(p.clone())).icon(ICON_ADD),
                    MenuItem::new("Открыть в новом окне", Cmd::OpenInNewWindowPath(p.clone())).icon(ICON_NEW_WINDOW),
                    MenuItem::sep(),
                ];
                if pinned {
                    v.push(MenuItem::new("Открепить от панели быстрого доступа", Cmd::UnpinPath(p.clone())).icon(ICON_UNPIN));
                } else {
                    v.push(MenuItem::new("Закрепить на панели быстрого доступа", Cmd::PinPath(p.clone())).icon(ICON_PIN));
                }
                v.push(MenuItem::new("Копировать как путь", Cmd::CopyPathOf(p.clone())).icon(ICON_LINK));
                v.push(MenuItem::new("Свойства", Cmd::PropertiesOf(p.clone())).icon(ICON_PROPERTIES));
                v
            }
            Location::Shell(s) if s.eq_ignore_ascii_case(shellfolder::RECYCLE_BIN) => vec![
                MenuItem::new("Открыть", Cmd::NavigateShell(s.clone())).icon(ICON_OPEN),
                MenuItem::new("Очистить корзину", Cmd::EmptyRecycleBin).icon(ICON_DELETE),
            ],
            Location::Home => vec![MenuItem::new("Открыть в новой вкладке", Cmd::NewTab).icon(ICON_ADD)],
            _ => vec![],
        };
        if !items.is_empty() {
            let id = app.new_popup_id();
            app.open_popup(Popup::new(id, pos, items, now(&ctx)));
        }
    }
    let _ = smenu::open;
}
