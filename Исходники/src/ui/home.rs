//! Страницы «Главная» (Быстрый доступ + Последние) и «Этот компьютер» (диски)

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use std::cell::RefCell;
use std::path::PathBuf;

use egui::{Align2, CornerRadius, Id, Pos2, Rect, Sense, Ui, Vec2};

use super::menu::{MenuItem, Popup};
use super::widgets::*;
use crate::app::{now, Location, OxideApp};
use crate::cmd::Cmd;
use crate::shell::fs;
use crate::theme::m;

thread_local! {
    static SEL: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
    static MQ: RefCell<Option<(Pos2, Vec<String>)>> = const { RefCell::new(None) };
    static TILES: RefCell<Vec<(String, Rect)>> = const { RefCell::new(Vec::new()) };
    static COLLAPSED: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn selected(key: &str) -> bool {
    SEL.with(|s| s.borrow().contains(key))
}
/// Щелчок по плитке: Ctrl - добавить/убрать, иначе выбрать только её
fn select(key: &str) {
    let ctrl = CTRL.with(|c| c.get());
    SEL.with(|s| {
        let mut s = s.borrow_mut();
        if ctrl {
            if !s.remove(key) {
                s.insert(key.to_string());
            }
        } else {
            s.clear();
            s.insert(key.to_string());
        }
    });
}
/// Правый щелчок: если плитка не выбрана - выбрать только её
fn select_for_menu(key: &str) {
    SEL.with(|s| {
        let mut s = s.borrow_mut();
        if !s.contains(key) {
            s.clear();
            s.insert(key.to_string());
        }
    });
}
fn reg_tile(key: &str, r: Rect) {
    TILES.with(|t| t.borrow_mut().push((key.to_string(), r)));
}

thread_local! {
    static CTRL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Фон страницы: щелчок по пустому месту снимает выделение, протягивание - рамка выделения
fn page_begin(ui: &mut Ui, id: &str) -> egui::Response {
    TILES.with(|t| t.borrow_mut().clear());
    CTRL.with(|c| c.set(ui.input(|i| i.modifiers.ctrl)));
    let r = ui.clip_rect();
    ui.interact(r, Id::new(("page-bg", id)), Sense::click_and_drag())
}

fn page_end(ui: &mut Ui, bg: &egui::Response, pal: &crate::theme::Palette) {
    let origin = ui.min_rect().min;
    let (pressed, down, ctrl, pos) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers.ctrl, i.pointer.latest_pos()));
    if bg.hovered() && pressed {
        let base: Vec<String> = if ctrl { SEL.with(|s| s.borrow().iter().cloned().collect()) } else { vec![] };
        if !ctrl {
            SEL.with(|s| s.borrow_mut().clear());
        }
        if let Some(p) = pos {
            MQ.with(|m| *m.borrow_mut() = Some((p - origin.to_vec2(), base)));
        }
    }
    if bg.secondary_clicked() {
        SEL.with(|s| s.borrow_mut().clear());
    }
    let mq = MQ.with(|m| m.borrow().clone());
    if let Some((start, base)) = mq {
        if !down {
            MQ.with(|m| *m.borrow_mut() = None);
            return;
        }
        let Some(p) = pos else { return };
        let sel = Rect::from_two_pos(start + origin.to_vec2(), p);
        if sel.width() < 3.0 && sel.height() < 3.0 {
            return;
        }
        let clip = ui.clip_rect();
        if p.y > clip.bottom() - 20.0 {
            ui.scroll_with_delta(Vec2::new(0.0, -12.0));
            ui.ctx().request_repaint();
        } else if p.y < clip.top() + 20.0 {
            ui.scroll_with_delta(Vec2::new(0.0, 12.0));
            ui.ctx().request_repaint();
        }
        let tiles = TILES.with(|t| t.borrow().clone());
        SEL.with(|s| {
            let mut s = s.borrow_mut();
            s.clear();
            s.extend(base.iter().cloned());
            for (k, r) in tiles {
                if r.intersects(sel) {
                    s.insert(k);
                }
            }
        });
        let pt = ui.painter();
        pt.rect_filled(sel, CornerRadius::ZERO, pal.marquee_fill());
        pt.rect_stroke(sel, CornerRadius::ZERO, egui::Stroke::new(1.0, pal.marquee), egui::StrokeKind::Inside);
    }
}

/// Заголовок раздела со сворачиванием. Возвращает true, если раздел развёрнут
fn section(app: &OxideApp, ui: &mut Ui, title: &'static str) -> bool {
    let pal = &app.pal;
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 40.0), Sense::click());
    let collapsed = COLLAPSED.with(|c| c.borrow().contains(&title));
    let hr = Rect::from_min_size(Pos2::new(r.left(), r.top() + 6.0), Vec2::new(text_width(ui, title, font_sb(14.0)) + 44.0, 28.0));
    subtle_bg(ui, &resp, hr, pal, m::R_CTRL, false);
    glyph(ui.painter(), Pos2::new(hr.left() + 14.0, hr.center().y), if collapsed { ICON_CHEVRON_RIGHT } else { ICON_CHEVRON_DOWN }, 10.0, pal.text2);
    ui.painter().text(Pos2::new(hr.left() + 30.0, hr.center().y), Align2::LEFT_CENTER, title, font_sb(14.0), pal.text);
    if resp.clicked() {
        COLLAPSED.with(|c| {
            let mut c = c.borrow_mut();
            if collapsed {
                c.retain(|t| *t != title);
            } else {
                c.push(title);
            }
        });
    }
    !collapsed
}

fn folder_menu(app: &mut OxideApp, ctx: &egui::Context, p: PathBuf, pinned: bool, pos: Pos2) {
    let mut items = vec![
        MenuItem::new(tr("Открыть"), Cmd::Navigate(p.clone())).icon(ICON_OPEN),
        MenuItem::new(tr("Открыть в новой вкладке"), Cmd::OpenInNewTabPath(p.clone())).icon(ICON_ADD),
        MenuItem::new(tr("Открыть в новом окне"), Cmd::OpenInNewWindowPath(p.clone())).icon(ICON_NEW_WINDOW),
        MenuItem::sep(),
    ];
    if pinned {
        items.push(MenuItem::new(tr("Открепить от панели быстрого доступа"), Cmd::UnpinPath(p.clone())).icon(ICON_UNPIN));
    } else {
        items.push(MenuItem::new(tr("Закрепить на панели быстрого доступа"), Cmd::PinPath(p.clone())).icon(ICON_PIN));
    }
    items.push(MenuItem::new(tr("Копировать как путь"), Cmd::CopyPathOf(p.clone())).icon(ICON_LINK));
    items.push(MenuItem::new(tr("Свойства"), Cmd::PropertiesOf(p)).icon(ICON_PROPERTIES));
    let id = app.new_popup_id();
    app.open_popup(Popup::new(id, pos, items, now(ctx)));
}

pub fn home(app: &mut OxideApp, ui: &mut Ui) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let mut go: Option<Location> = None;
    let mut open_file: Option<String> = None;
    let mut menu: Option<(PathBuf, bool, Pos2)> = None;
    let mut file_menu: Option<(PathBuf, Pos2)> = None;
    vscroll(ui, egui::ScrollArea::vertical().id_salt("home").auto_shrink([false, false]), "home", &pal, |ui, _| {
        let bg = page_begin(ui, "home");
        ui.add_space(8.0);
        let margin = 20.0;
        ui.horizontal(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - margin);
                if section(app, ui, tr("Быстрый доступ")) {
                    let quick = app.quick.clone();
                    let tile = Vec2::new(236.0, 64.0);
                    let w = ui.available_width();
                    let cols = ((w / (tile.x + 8.0)).floor() as usize).max(1);
                    let rows = quick.len().div_ceil(cols);
                    let (area, _) = ui.allocate_exact_size(Vec2::new(w, rows as f32 * (tile.y + 8.0)), Sense::hover());
                    for (i, (name, p, pinned)) in quick.iter().enumerate() {
                        let r = Rect::from_min_size(area.min + Vec2::new((i % cols) as f32 * (tile.x + 8.0), (i / cols) as f32 * (tile.y + 8.0)), tile);
                        let key = format!("qa:{}", p.display());
                        let resp = ui.interact(r, Id::new(&key), Sense::click());
                        app.drop_targets.items.push((r, p.clone()));
                        reg_tile(&key, r);
                        let sel = selected(&key);
                        let ht = ctx.animate_bool_with_time(resp.id.with("hv"), resp.hovered(), m::T_HOVER);
                        if sel {
                            ui.painter().rect_filled(r, CornerRadius::same(m::R_CTRL), if resp.hovered() { pal.fill(pal.selected_hover) } else { pal.fill(pal.selected) });
                        } else if ht > 0.0 {
                            ui.painter().rect_filled(r, CornerRadius::same(m::R_CTRL), pal.fill(pal.hover.gamma_multiply(ht)));
                        }
                        let px = (40.0 * ppp).round() as u32;
                        let ps = p.to_string_lossy().to_string();
                        if let Some(t) = app.icons.get(&ps, &ps, px, false) {
                            paint_texture(ui.painter(), t, Pos2::new(r.left() + 30.0, r.center().y), 40.0);
                        }
                        let tx = r.left() + 60.0;
                        text_trunc(ui.painter(), Pos2::new(tx, r.center().y - 9.0), name, font(14.0), pal.text, r.right() - tx - 8.0);
                        let sub = p.parent().map(|pp| fs::shell_display_name(pp).unwrap_or_else(|| pp.to_string_lossy().to_string())).unwrap_or_default();
                        text_trunc(ui.painter(), Pos2::new(tx, r.center().y + 10.0), &sub, font(12.0), pal.text2, r.right() - tx - 26.0);
                        if *pinned {
                            glyph(ui.painter(), Pos2::new(r.right() - 14.0, r.center().y + 10.0), ICON_PIN, 12.0, pal.text3);
                        }
                        if resp.clicked() {
                            select(&key);
                        }
                        if resp.double_clicked() {
                            go = Some(Location::Path(p.clone()));
                        }
                        if resp.middle_clicked() {
                            app.open_tab(&ctx, Location::Path(p.clone()), false);
                        }
                        if resp.secondary_clicked() {
                            select_for_menu(&key);
                            menu = Some((p.clone(), *pinned, resp.interact_pointer_pos().unwrap_or(r.center())));
                        }
                    }
                    ui.add_space(12.0);
                }
                if app.settings.show_recent && section(app, ui, tr("Последние")) {
                    let recent = app.recent.clone();
                    let w = ui.available_width();
                    let c_name = (w * 0.45).max(220.0);
                    let c_date = 160.0;
                    // Заголовки
                    let (hr, _) = ui.allocate_exact_size(Vec2::new(w, m::HEADER_H), Sense::hover());
                    let cy = hr.center().y;
                    ui.painter().text(Pos2::new(hr.left() + 12.0, cy), Align2::LEFT_CENTER, tr("Имя"), font(12.0), pal.text2);
                    ui.painter().text(Pos2::new(hr.left() + c_name, cy), Align2::LEFT_CENTER, tr("Дата открытия"), font(12.0), pal.text2);
                    ui.painter().text(Pos2::new(hr.left() + c_name + c_date, cy), Align2::LEFT_CENTER, tr("Расположение"), font(12.0), pal.text2);
                    if recent.is_empty() {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(tr("Здесь появятся файлы, которые вы недавно открывали")).font(font(12.0)).color(pal.text2));
                    }
                    let row_h = if app.settings.compact { m::ROW_H_COMPACT } else { m::ROW_H };
                    for e in recent.iter() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::click());
                        let key = format!("rc:{}", e.path.display());
                        let hl = r.shrink2(Vec2::new(0.0, 2.0));
                        reg_tile(&key, hl);
                        let sel = selected(&key);
                        let ht = ctx.animate_bool_with_time(resp.id.with("hv"), resp.hovered(), m::T_HOVER);
                        if sel {
                            ui.painter().rect_filled(hl, CornerRadius::same(m::R_CTRL), if resp.hovered() { pal.fill(pal.selected_hover) } else { pal.fill(pal.selected) });
                        } else if ht > 0.0 {
                            ui.painter().rect_filled(hl, CornerRadius::same(m::R_CTRL), pal.fill(pal.hover.gamma_multiply(ht)));
                        }
                        let cy = r.center().y;
                        let (k, path, _) = super::content::icon_req(e, false);
                        let px = (16.0 * ppp).round() as u32;
                        if let Some(t) = app.icons.get(&k, &path, px, false) {
                            paint_texture(ui.painter(), t, Pos2::new(r.left() + 20.0, cy), 16.0);
                        }
                        text_trunc(ui.painter(), Pos2::new(r.left() + 36.0, cy), &e.display, font(12.0), pal.text, c_name - 44.0);
                        text_trunc(ui.painter(), Pos2::new(r.left() + c_name, cy), &crate::util::format_filetime(e.ctime), font(12.0), pal.text2, c_date - 8.0);
                        let loc = e.location.clone().unwrap_or_default();
                        text_trunc(ui.painter(), Pos2::new(r.left() + c_name + c_date, cy), &loc, font(12.0), pal.text2, w - c_name - c_date - 8.0);
                        if resp.clicked() {
                            select(&key);
                        }
                        if resp.double_clicked() {
                            open_file = Some(e.path.to_string_lossy().to_string());
                        }
                        if resp.secondary_clicked() {
                            select_for_menu(&key);
                            file_menu = Some((e.path.clone(), resp.interact_pointer_pos().unwrap_or(r.center())));
                        }
                    }
                }
                ui.add_space(24.0);
            });
        });
        page_end(ui, &bg, &pal);
    });
    if let Some(l) = go {
        app.navigate(&ctx, l);
    }
    if let Some(f) = open_file {
        crate::shell::menu::open(&f);
    }
    if let Some((p, pinned, pos)) = menu {
        folder_menu(app, &ctx, p, pinned, pos);
    }
    if let Some((p, pos)) = file_menu {
        let items = vec![
            MenuItem::new(tr("Открыть"), Cmd::OpenParsing(p.to_string_lossy().to_string())).icon(ICON_OPEN),
            MenuItem::new(tr("Открыть расположение файла"), Cmd::Navigate(p.clone())).icon(ICON_FOLDER),
            MenuItem::sep(),
            MenuItem::new(tr("Копировать как путь"), Cmd::CopyPathOf(p.clone())).icon(ICON_LINK),
            MenuItem::new(tr("Свойства"), Cmd::PropertiesOf(p)).icon(ICON_PROPERTIES),
        ];
        let id = app.new_popup_id();
        app.open_popup(Popup::new(id, pos, items, now(&ctx)));
    }
}

pub fn this_pc(app: &mut OxideApp, ui: &mut Ui) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let mut go: Option<Location> = None;
    let mut menu: Option<(PathBuf, Pos2)> = None;
    vscroll(ui, egui::ScrollArea::vertical().id_salt("thispc").auto_shrink([false, false]), "thispc", &pal, |ui, _| {
        let bg = page_begin(ui, "thispc");
        ui.add_space(8.0);
        let margin = 20.0;
        ui.horizontal(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - margin);
                let drives = app.drives.clone();
                let local: Vec<_> = drives.iter().filter(|d| d.kind != fs::DRIVE_REMOTE).cloned().collect();
                let remote: Vec<_> = drives.iter().filter(|d| d.kind == fs::DRIVE_REMOTE).cloned().collect();
                for (title, list) in [(tr("Устройства и диски"), local), (tr("Сетевые расположения"), remote)] {
                    if list.is_empty() {
                        continue;
                    }
                    if !section(app, ui, title) {
                        continue;
                    }
                    let tile = Vec2::new(272.0, 72.0);
                    let w = ui.available_width();
                    let cols = ((w / (tile.x + 8.0)).floor() as usize).max(1);
                    let rows = list.len().div_ceil(cols);
                    let (area, _) = ui.allocate_exact_size(Vec2::new(w, rows as f32 * (tile.y + 8.0)), Sense::hover());
                    for (i, d) in list.iter().enumerate() {
                        let r = Rect::from_min_size(area.min + Vec2::new((i % cols) as f32 * (tile.x + 8.0), (i / cols) as f32 * (tile.y + 8.0)), tile);
                        let key = format!("drv:{}", d.root.display());
                        let resp = ui.interact(r, Id::new(&key), Sense::click());
                        app.drop_targets.items.push((r, d.root.clone()));
                        reg_tile(&key, r);
                        let sel = selected(&key);
                        let ht = ctx.animate_bool_with_time(resp.id.with("hv"), resp.hovered(), m::T_HOVER);
                        if sel {
                            ui.painter().rect_filled(r, CornerRadius::same(m::R_CTRL), if resp.hovered() { pal.fill(pal.selected_hover) } else { pal.fill(pal.selected) });
                        } else if ht > 0.0 {
                            ui.painter().rect_filled(r, CornerRadius::same(m::R_CTRL), pal.fill(pal.hover.gamma_multiply(ht)));
                        }
                        let ps = d.root.to_string_lossy().to_string();
                        let px = (48.0 * ppp).round() as u32;
                        if let Some(t) = app.icons.get(&ps, &ps, px, false) {
                            paint_texture(ui.painter(), t, Pos2::new(r.left() + 34.0, r.center().y), 48.0);
                        } else {
                            glyph(ui.painter(), Pos2::new(r.left() + 34.0, r.center().y), ICON_DRIVE, 32.0, pal.text2);
                        }
                        let tx = r.left() + 68.0;
                        let tw = r.right() - tx - 12.0;
                        text_trunc(ui.painter(), Pos2::new(tx, r.top() + 18.0), &d.label, font(14.0), pal.text, tw);
                        if d.total > 0 {
                            let frac = 1.0 - d.free as f32 / d.total as f32;
                            let bar = Rect::from_min_size(Pos2::new(tx, r.top() + 34.0), Vec2::new(tw, 4.0));
                            usage_bar(ui.painter(), bar, frac, &pal);
                            let txt = trf("{} свободно из {}", &[&crate::util::format_bytes(d.free), &crate::util::format_bytes(d.total)]);
                            text_trunc(ui.painter(), Pos2::new(tx, r.top() + 54.0), &txt, font(12.0), pal.text2, tw);
                        }
                        if resp.clicked() {
                            select(&key);
                        }
                        if resp.double_clicked() {
                            go = Some(Location::Path(d.root.clone()));
                        }
                        if resp.middle_clicked() {
                            app.open_tab(&ctx, Location::Path(d.root.clone()), false);
                        }
                        if resp.secondary_clicked() {
                            select_for_menu(&key);
                            menu = Some((d.root.clone(), resp.interact_pointer_pos().unwrap_or(r.center())));
                        }
                    }
                    ui.add_space(12.0);
                }
            });
        });
        page_end(ui, &bg, &pal);
    });
    if let Some(l) = go {
        app.navigate(&ctx, l);
    }
    if let Some((p, pos)) = menu {
        let items = vec![
            MenuItem::new(tr("Открыть"), Cmd::Navigate(p.clone())).icon(ICON_OPEN),
            MenuItem::new(tr("Открыть в новой вкладке"), Cmd::OpenInNewTabPath(p.clone())).icon(ICON_ADD),
            MenuItem::new(tr("Открыть в новом окне"), Cmd::OpenInNewWindowPath(p.clone())).icon(ICON_NEW_WINDOW),
            MenuItem::sep(),
            MenuItem::new(tr("Закрепить на панели быстрого доступа"), Cmd::PinPath(p.clone())).icon(ICON_PIN),
            MenuItem::new(tr("Копировать как путь"), Cmd::CopyPathOf(p.clone())).icon(ICON_LINK),
            MenuItem::new(tr("Свойства"), Cmd::PropertiesOf(p)).icon(ICON_PROPERTIES),
        ];
        let id = app.new_popup_id();
        app.open_popup(Popup::new(id, pos, items, now(&ctx)));
    }
}
