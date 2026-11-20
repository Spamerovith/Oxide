//! Полоса вкладок в заголовке окна и кнопки «Свернуть / Развернуть / Закрыть»

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use std::sync::atomic::Ordering;

use egui::{Color32, CornerRadius, Id, Pos2, Rect, Sense, Ui, Vec2};

use super::menu::{MenuItem, Popup};
use super::widgets::*;
use crate::app::{now, Location, OxideApp};
use crate::cmd::Cmd;
use crate::shell::window;
use crate::theme::m;

pub fn location_icon_key(loc: &Location) -> Option<(String, String)> {
    match loc {
        Location::Path(p) => {
            let s = p.to_string_lossy().to_string();
            Some((s.clone(), s))
        }
        Location::Shell(s) => Some((s.clone(), s.clone())),
        _ => None,
    }
}

pub fn location_glyph(loc: &Location) -> char {
    match loc {
        Location::Home => ICON_HOME,
        Location::ThisPc => ICON_PC,
        _ => ICON_FOLDER,
    }
}

/// Значок расположения 16×16 (системный, если есть)
pub fn paint_loc_icon(app: &mut OxideApp, ui: &Ui, loc: &Location, center: Pos2) {
    let ppp = ui.ctx().pixels_per_point();
    let px = (16.0 * ppp).round() as u32;
    if let Some((k, p)) = location_icon_key(loc) {
        if let Some(t) = app.icons.get(&k, &p, px, false) {
            paint_texture(ui.painter(), t, center, 16.0);
            return;
        }
    }
    let col = match loc {
        Location::Home => app.pal.accent,
        _ => app.pal.text2,
    };
    glyph(ui.painter(), center, location_glyph(loc), 16.0, col);
}

pub fn show(app: &mut OxideApp, ui: &mut Ui) {
    let rect = ui.max_rect();
    let ppp = ui.ctx().pixels_per_point();
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();

    // Кнопки окна
    let close_r = Rect::from_min_size(Pos2::new(rect.right() - m::CAPTION_W, rect.top()), Vec2::new(m::CAPTION_W, m::CAPTION_H));
    let max_r = close_r.translate(Vec2::new(-m::CAPTION_W, 0.0));
    let min_r = max_r.translate(Vec2::new(-m::CAPTION_W, 0.0));

    let mut interactive: Vec<[i32; 4]> = vec![];
    let phys = |r: Rect| [(r.left() * ppp) as i32, (r.top() * ppp) as i32, (r.right() * ppp).ceil() as i32, (r.bottom() * ppp).ceil() as i32];

    let min_resp = ui.interact(min_r, Id::new("cap-min"), Sense::click());
    caption_bg(ui, &min_resp, min_r, &pal, false);
    glyph(ui.painter(), min_r.center(), ICON_MIN, 10.0, pal.text);
    if min_resp.clicked() {
        window::minimize();
    }
    interactive.push(phys(min_r));

    // Кнопка «Развернуть» - системная зона HTMAXBUTTON, чтобы работали Snap Layouts
    let max_hover = window::MAX_HOVER.load(Ordering::Relaxed);
    let max_pressed = window::MAX_PRESSED.load(Ordering::Relaxed);
    let max_resp = ui.interact(max_r, Id::new("cap-max"), Sense::click());
    if max_pressed {
        ui.painter().rect_filled(max_r, CornerRadius::ZERO, pal.fill(pal.pressed));
    } else if max_hover || max_resp.hovered() {
        ui.painter().rect_filled(max_r, CornerRadius::ZERO, pal.fill(pal.hover));
    }
    if max_resp.clicked() {
        window::toggle_maximize();
    }
    glyph(ui.painter(), max_r.center(), if window::is_maximized() { ICON_RESTORE } else { ICON_MAX }, 10.0, pal.text);

    let close_resp = ui.interact(close_r, Id::new("cap-close"), Sense::click());
    let close_t = ctx.animate_bool_with_time(close_resp.id.with("hv"), close_resp.hovered(), m::T_HOVER);
    if close_t > 0.0 {
        let c = if close_resp.is_pointer_button_down_on() { pal.close_hover.gamma_multiply(0.85) } else { pal.close_hover };
        ui.painter().rect_filled(close_r, CornerRadius::ZERO, c.gamma_multiply(close_t));
    }
    let close_fg = if close_t > 0.5 { Color32::WHITE } else { pal.text };
    glyph(ui.painter(), close_r.center(), ICON_CLOSE, 10.0, close_fg);
    if close_resp.clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    interactive.push(phys(close_r));

    // Кнопка «Обновить» слева, пока есть новая версия, затем вкладки
    let top = rect.top() + (m::TITLE_H - m::TAB_H);
    let left = update_button(app, ui, &pal, rect.left() + 8.0, top, &mut interactive, &phys);
    let right_limit = min_r.left() - 48.0;
    let n = app.tabs.len().max(1) as f32;
    let tab_w = ((right_limit - left - 40.0) / n).clamp(m::TAB_MIN_W, m::TAB_MAX_W);
    let mut x = left;
    let mut action: Option<Cmd> = None;
    let mut activate: Option<usize> = None;
    let mut ctx_menu: Option<(usize, Pos2)> = None;
    let active = app.active;
    let hover_idx = (0..app.tabs.len()).find(|&i| {
        let r = Rect::from_min_size(Pos2::new(left + i as f32 * tab_w, top), Vec2::new(tab_w, m::TAB_H));
        ui.rect_contains_pointer(r)
    });
    let ntabs = app.tabs.len();
    let drag_off = app.tab_drag.map(|(_, dx)| dx).unwrap_or(0.0);
    for i in 0..ntabs {
        let mut r = Rect::from_min_size(Pos2::new(x, top), Vec2::new(tab_w, m::TAB_H));
        x += tab_w;
        if app.tab_drag.map(|d| d.0) == Some(i) {
            r = r.translate(Vec2::new(drag_off, 0.0));
        }
        let is_active = i == active;
        let resp = ui.interact(r, Id::new(("tab", app.tabs[i].id)), Sense::click_and_drag());
        // По прямоугольнику, а не по виджету: над крестиком вкладка тоже считается наведённой
        let tab_hovered = app.tab_drag.is_none() && ui.rect_contains_pointer(r);
        interactive.push(phys(r));
        let p = ui.painter();
        if is_active {
            // Активная вкладка сливается со строкой адреса; «ушки» внизу
            let body = Rect::from_min_max(r.min, Pos2::new(r.right(), rect.bottom()));
            p.rect_filled(body, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, pal.layer);
            let ear = 8.0;
            if pal.glass {
                // Нижний край вкладки сливается со строкой адреса, его блик не нужен
                let clip = p.with_clip_rect(Rect::from_min_max(r.min - Vec2::new(1.0, 1.0), Pos2::new(r.right() + 1.0, rect.bottom())));
                glass_rim(&clip, Rect::from_min_max(r.min, Pos2::new(r.right(), rect.bottom() + 8.0)), 8.0, &pal);
                // Прозрачный слой нельзя «вырезать» кругом цвета фона: ушки рисуем сразу вогнутыми
                p.add(ear_shape(Pos2::new(r.left(), rect.bottom()), -ear, pal.layer));
                p.add(ear_shape(Pos2::new(r.right(), rect.bottom()), ear, pal.layer));
            } else {
                let bl = Rect::from_min_size(Pos2::new(r.left() - ear, rect.bottom() - ear), Vec2::splat(ear));
                p.rect_filled(bl, CornerRadius::ZERO, pal.layer);
                p.circle_filled(Pos2::new(r.left() - ear, rect.bottom() - ear), ear, pal.base);
                let br = Rect::from_min_size(Pos2::new(r.right(), rect.bottom() - ear), Vec2::splat(ear));
                p.rect_filled(br, CornerRadius::ZERO, pal.layer);
                p.circle_filled(Pos2::new(r.right() + ear, rect.bottom() - ear), ear, pal.base);
            }
        } else {
            let t = ctx.animate_bool_with_time(resp.id.with("hv"), tab_hovered, m::T_HOVER);
            if t > 0.0 {
                let hr = Rect::from_min_max(Pos2::new(r.left() + 2.0, r.top() + 2.0), Pos2::new(r.right() - 2.0, r.bottom() - 4.0));
                p.rect_filled(hr, CornerRadius::same(m::R_CTRL), pal.fill(pal.hover.gamma_multiply(t)));
            }
            // Разделитель между неактивными вкладками
            let next_active = i + 1 == active;
            let near_hover = hover_idx == Some(i) || hover_idx == Some(i + 1);
            if i + 1 < ntabs && !next_active && !near_hover {
                vsep(p, r.right(), r.center().y - 2.0, 16.0, &pal);
            }
        }
        let cy = r.center().y - if is_active { 0.0 } else { 2.0 };
        let loc = app.tabs[i].loc.clone();
        paint_loc_icon(app, ui, &loc, Pos2::new(r.left() + 20.0, cy));
        let title = if app.tabs[i].searching() { trf("Поиск: {}", &[&app.tabs[i].search_text]) } else { app.tabs[i].title.clone() };
        let show_close = is_active || tab_hovered;
        let tw = r.width() - 36.0 - if show_close { 32.0 } else { 8.0 };
        text_trunc(ui.painter(), Pos2::new(r.left() + 36.0, cy), &title, font(12.0), if is_active { pal.text } else { pal.text2 }, tw);
        let mut close_hit = false;
        if show_close && r.width() > 90.0 {
            let cr = Rect::from_center_size(Pos2::new(r.right() - 20.0, cy), Vec2::new(24.0, 24.0));
            let cresp = ui.interact(cr, Id::new(("tabclose", app.tabs[i].id)), Sense::click());
            subtle_bg(ui, &cresp, cr, &pal, m::R_CTRL, false);
            glyph(ui.painter(), cr.center(), ICON_CLOSE, 10.0, pal.text2);
            if cresp.clicked() {
                action = Some(Cmd::CloseTab(i));
                close_hit = true;
            }
            let _ = cresp.on_hover_text(tr("Закрыть вкладку (Ctrl+W)"));
        }
        if !close_hit {
            if resp.clicked() || resp.drag_started() {
                activate = Some(i);
            }
            if resp.middle_clicked() {
                action = Some(Cmd::CloseTab(i));
            }
            if resp.secondary_clicked() {
                ctx_menu = Some((i, resp.interact_pointer_pos().unwrap_or(r.center())));
            }
            if resp.dragged() {
                let dx = resp.drag_delta().x;
                let cur = app.tab_drag.map(|d| d.1).unwrap_or(0.0) + dx;
                app.tab_drag = Some((i, cur));
                // Перестановка с соседями
                if cur > tab_w / 2.0 && i + 1 < ntabs {
                    app.tabs.swap(i, i + 1);
                    if app.active == i {
                        app.active = i + 1;
                    } else if app.active == i + 1 {
                        app.active = i;
                    }
                    app.tab_drag = Some((i + 1, cur - tab_w));
                } else if cur < -tab_w / 2.0 && i > 0 {
                    app.tabs.swap(i, i - 1);
                    if app.active == i {
                        app.active = i - 1;
                    } else if app.active == i - 1 {
                        app.active = i;
                    }
                    app.tab_drag = Some((i - 1, cur + tab_w));
                }
            }
            if resp.drag_stopped() {
                // Вытащили вкладку вниз за пределы полосы - в новое окно
                if let Some(p) = resp.interact_pointer_pos() {
                    if p.y > rect.bottom() + 60.0 && ntabs > 1 {
                        action = Some(Cmd::TabToWindow(app.tab_drag.map(|d| d.0).unwrap_or(i)));
                    }
                }
                app.tab_drag = None;
            }
        }
        if i >= app.tabs.len() {
            break;
        }
    }

    // «+» - новая вкладка
    let plus = Rect::from_center_size(Pos2::new(x + 24.0, top + m::TAB_H / 2.0 - 2.0), Vec2::new(32.0, 32.0));
    let presp = icon_button(ui, plus, Id::new("newtab"), ICON_ADD, 12.0, &pal, true, tr("Новая вкладка (Ctrl+T)"));
    interactive.push(phys(plus));
    if presp.clicked() {
        action = Some(Cmd::NewTab);
    }

    window::set_regions(window::HitRegions { caption_h: (m::TITLE_H * ppp) as i32, max_btn: Some(phys(max_r)), interactive });

    if let Some(i) = activate {
        if i < app.tabs.len() {
            app.active = i;
        }
    }
    if let Some((i, pos)) = ctx_menu {
        let id = app.new_popup_id();
        let n = app.tabs.len();
        let items = vec![
            MenuItem::new(tr("Новая вкладка"), Cmd::NewTab).icon(ICON_ADD).key("Ctrl+T"),
            MenuItem::new(tr("Дублировать вкладку"), Cmd::DuplicateTab(i)),
            MenuItem::new(tr("Переместить в новое окно"), Cmd::TabToWindow(i)).icon(ICON_NEW_WINDOW).enabled(n > 1),
            MenuItem::sep(),
            MenuItem::new(tr("Закрыть вкладку"), Cmd::CloseTab(i)).icon(ICON_CLOSE).key("Ctrl+W"),
            MenuItem::new(tr("Закрыть другие вкладки"), Cmd::CloseOtherTabs(i)).enabled(n > 1),
            MenuItem::new(tr("Закрыть вкладки справа"), Cmd::CloseTabsRight(i)).enabled(i + 1 < n),
        ];
        app.open_popup(Popup::new(id, pos, items, now(&ctx)));
    }
    if let Some(c) = action {
        app.exec(&ctx, c);
    }
}

/// Акцентная кнопка обновления в левом верхнем углу. Возвращает, откуда начинать вкладки
fn update_button(app: &mut OxideApp, ui: &mut Ui, pal: &crate::theme::Palette, x: f32, top: f32, interactive: &mut Vec<[i32; 4]>, phys: &dyn Fn(Rect) -> [i32; 4]) -> f32 {
    use crate::i18n::{is_en, l};
    use crate::update::State;
    let st = app.updater.state();
    let (label, progress, tip) = match &st {
        State::Available(r) => (
            l("Обновить", "Update").to_string(),
            None,
            if is_en() { format!("Update Oxide to version {}", r.version) } else { format!("Обновить Oxide до версии {}", r.version) },
        ),
        State::Downloading { rel, got } => {
            let f = if rel.size > 0 { (*got as f32 / rel.size as f32).clamp(0.0, 1.0) } else { 0.0 };
            (format!("{} %", (f * 100.0) as u32), Some(f), l("Загрузка обновления", "Downloading the update").to_string())
        }
        State::Installed(_) => (l("Перезапуск…", "Restarting…").to_string(), Some(1.0), String::new()),
        _ => return x,
    };
    let ctx = ui.ctx().clone();
    let h = 28.0;
    let tw = text_width(ui, &label, font(12.0));
    let w = (12.0 + 16.0 + 8.0 + tw + 12.0).max(96.0);
    let cy = top + m::TAB_H / 2.0 - 2.0;
    let r = Rect::from_min_size(Pos2::new(x, cy - h / 2.0), Vec2::new(w, h));
    let clickable = progress.is_none();
    let resp = ui.interact(r, Id::new("update-btn"), if clickable { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let radius = CornerRadius::same(m::R_CTRL);
    match progress {
        None => {
            let t = ctx.animate_bool_with_time(resp.id.with("hv"), resp.hovered(), m::T_HOVER);
            let fill = if resp.is_pointer_button_down_on() {
                pal.fill(pal.accent.gamma_multiply(0.8))
            } else {
                pal.fill(pal.accent.gamma_multiply(1.0 - 0.1 * t))
            };
            p.rect_filled(r, radius, fill);
        }
        Some(f) => {
            p.rect_filled(r, radius, pal.fill(pal.accent.gamma_multiply(0.45)));
            let done = Rect::from_min_size(r.min, Vec2::new(r.width() * f, r.height()));
            p.with_clip_rect(done).rect_filled(r, radius, pal.accent);
        }
    }
    let off = if resp.is_pointer_button_down_on() { 0.5 } else { 0.0 };
    glyph(p, Pos2::new(r.left() + 12.0 + 8.0, cy + off), ICON_DOWNLOAD, 16.0, pal.on_accent);
    p.text(Pos2::new(r.left() + 12.0 + 16.0 + 8.0, cy + off), egui::Align2::LEFT_CENTER, &label, font(12.0), pal.on_accent);
    interactive.push(phys(r));
    if !tip.is_empty() {
        let resp = resp.on_hover_text(tip);
        if resp.clicked() {
            if let State::Available(rel) = st {
                app.updater.install(&ctx, rel);
            }
        }
    }
    // Отступ больше скругления активной вкладки, чтобы её «ушко» не наезжало на кнопку
    r.right() + 16.0
}

fn caption_bg(ui: &Ui, resp: &egui::Response, r: Rect, pal: &crate::theme::Palette, _close: bool) {
    let t = ui.ctx().animate_bool_with_time(resp.id.with("hv"), resp.hovered(), m::T_HOVER);
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(r, CornerRadius::ZERO, pal.fill(pal.pressed));
    } else if t > 0.0 {
        ui.painter().rect_filled(r, CornerRadius::ZERO, pal.fill(pal.hover.gamma_multiply(t)));
    }
}

/// Вогнутое «ушко» у низа активной вкладки: угол (x, низ), ear < 0 - ушко слева
fn ear_shape(corner: Pos2, ear: f32, color: egui::Color32) -> egui::Shape {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(corner, color);
    let center = Pos2::new(corner.x + ear, corner.y - ear.abs());
    let steps = 8;
    for i in 0..=steps {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
        // От точки над углом (x, низ - ear) по дуге к точке на линии низа (x + ear, низ)
        let pt = Pos2::new(center.x - ear * a.cos(), center.y + ear.abs() * a.sin());
        mesh.colored_vertex(pt, color);
    }
    for i in 1..=steps as u32 {
        mesh.add_triangle(0, i, i + 1);
    }
    egui::Shape::mesh(mesh)
}
