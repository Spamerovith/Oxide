//! Базовые элементы в стиле WinUI: шрифты, глифы, фоны наведения, переключатели

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::emath::GuiRounding;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme::{m, Palette};

pub const ICON_NEW: char = '\u{ECC8}';
pub const ICON_CUT: char = '\u{E8C6}';
pub const ICON_COPY: char = '\u{E8C8}';
pub const ICON_PASTE: char = '\u{E77F}';
pub const ICON_RENAME: char = '\u{E8AC}';
pub const ICON_SHARE: char = '\u{E72D}';
pub const ICON_DELETE: char = '\u{E74D}';
pub const ICON_SORT: char = '\u{E8CB}';
pub const ICON_VIEW: char = '\u{E8FD}';
pub const ICON_MORE: char = '\u{E712}';
pub const ICON_DETAILS_PANE: char = '\u{E90C}';
pub const ICON_BACK: char = '\u{E72B}';
pub const ICON_FORWARD: char = '\u{E72A}';
pub const ICON_UP: char = '\u{E74A}';
pub const ICON_REFRESH: char = '\u{E72C}';
pub const ICON_CHEVRON_RIGHT: char = '\u{E76C}';
pub const ICON_CHEVRON_DOWN: char = '\u{E70D}';
pub const ICON_CHEVRON_UP: char = '\u{E70E}';
pub const ICON_SEARCH: char = '\u{E721}';
pub const ICON_ADD: char = '\u{E710}';
pub const ICON_CLOSE: char = '\u{E8BB}';
pub const ICON_MIN: char = '\u{E921}';
pub const ICON_MAX: char = '\u{E922}';
pub const ICON_RESTORE: char = '\u{E923}';
pub const ICON_HOME: char = '\u{E80F}';
pub const ICON_PC: char = '\u{E7F4}';
pub const ICON_NETWORK: char = '\u{EC27}';
pub const ICON_DRIVE: char = '\u{EDA2}';
pub const ICON_CLOUD: char = '\u{E753}';
pub const ICON_GALLERY: char = '\u{E91B}';
pub const ICON_PIN: char = '\u{E718}';
pub const ICON_UNPIN: char = '\u{E77A}';
pub const ICON_SETTINGS: char = '\u{E713}';
pub const ICON_PROPERTIES: char = '\u{E90F}';
pub const ICON_OPEN: char = '\u{E8E5}';
pub const ICON_OPEN_WITH: char = '\u{E7AC}';
pub const ICON_LINK: char = '\u{E71B}';
pub const ICON_NEW_WINDOW: char = '\u{E8A7}';
pub const ICON_TERMINAL: char = '\u{E756}';
pub const ICON_NEW_FOLDER: char = '\u{E8F4}';
pub const ICON_ZIP: char = '\u{F012}';
pub const ICON_RECENT: char = '\u{E81C}';
pub const ICON_CHECK: char = '\u{E73E}';
pub const ICON_SELECT_ALL: char = '\u{E8B3}';
pub const ICON_SELECT_NONE: char = '\u{E8E6}';
pub const ICON_UNDO: char = '\u{E7A7}';
pub const ICON_MORE_OPTIONS: char = '\u{E8A7}';
pub const ICON_FOLDER: char = '\u{E8B7}';
pub const ICON_DOC: char = '\u{E8A5}';
pub const ICON_RUN_ADMIN: char = '\u{E7EF}';
pub const ICON_WALLPAPER: char = '\u{E91B}';
pub const ICON_V_XL: char = '\u{E91B}';
pub const ICON_V_L: char = '\u{E8B9}';
pub const ICON_V_M: char = '\u{E80A}';
pub const ICON_V_S: char = '\u{F0E2}';
pub const ICON_V_LIST: char = '\u{EA37}';
pub const ICON_V_DETAILS: char = '\u{E8FD}';
pub const ICON_V_TILES: char = '\u{ECA5}';
pub const ICON_V_CONTENT: char = '\u{E8A9}';
pub const ICON_INFO: char = '\u{E946}';
pub const ICON_EXTRACT: char = '\u{E8DA}';
pub const ICON_SHOW: char = '\u{E7B3}';
pub const ICON_DOWNLOAD: char = '\u{E896}';
pub const ICON_RESTORE_BIN: char = '\u{E777}';

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn font_sb(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}

pub fn glyph(p: &Painter, center: Pos2, c: char, size: f32, color: Color32) {
    let galley = p.layout_no_wrap(c.to_string(), icon_font(size), color);
    let r = galley.rect;
    // Глифы Fluent рисуются в квадрате em: центрируем по размеру шрифта, а не по строке
    let pos = Pos2::new(center.x - r.width() / 2.0, center.y - galley.size().y / 2.0);
    p.galley(pos.round(), galley, color);
}

/// Текст с обрезкой «…» по ширине; `pos` - левый край, середина по вертикали
pub fn text_trunc(p: &Painter, pos: Pos2, text: &str, f: FontId, color: Color32, max_w: f32) -> f32 {
    if max_w <= 4.0 {
        return 0.0;
    }
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(f, color));
    job.wrap = TextWrapping::truncate_at_width(max_w);
    let g = p.layout_job(job);
    let w = g.size().x;
    let y = pos.y - g.size().y / 2.0;
    p.galley(Pos2::new(pos.x, y).round(), g, color);
    w
}

pub fn text_right(p: &Painter, right: Pos2, text: &str, f: FontId, color: Color32) {
    p.text(right, Align2::RIGHT_CENTER, text, f, color);
}

pub fn text_width(ui: &Ui, text: &str, f: FontId) -> f32 {
    ui.fonts(|fs| fs.layout_no_wrap(text.to_owned(), f, Color32::WHITE).size().x)
}

/// Фон кнопки без рамки: прозрачный → наведение → нажатие (с плавным появлением)
pub fn subtle_bg(ui: &Ui, resp: &Response, rect: Rect, pal: &Palette, r: u8, active: bool) {
    let t = ui.ctx().animate_bool_with_time(resp.id.with("hv"), resp.hovered() || active, m::T_HOVER);
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, CornerRadius::same(r), pal.fill(pal.pressed));
        return;
    }
    if t > 0.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(r), pal.fill(pal.hover.gamma_multiply(t)));
    }
}

/// Кнопка со значком
pub fn icon_button(ui: &mut Ui, rect: Rect, id: egui::Id, c: char, size: f32, pal: &Palette, enabled: bool, tip: &str) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, id, sense);
    if enabled {
        subtle_bg(ui, &resp, rect, pal, m::R_CTRL, false);
    }
    let col = if enabled { pal.text } else { pal.text_disabled };
    let off = if resp.is_pointer_button_down_on() { 0.5 } else { 0.0 };
    glyph(ui.painter(), rect.center() + Vec2::new(0.0, off), c, size, col);
    if !tip.is_empty() {
        resp.on_hover_text(tip)
    } else {
        resp
    }
}

/// Кнопка «значок + текст (+ шеврон)» командной панели. Возвращает ответ и ширину
pub fn cmd_button(
    ui: &mut Ui,
    x: f32,
    cy: f32,
    id: egui::Id,
    icon: Option<(char, Color32)>,
    label: &str,
    dropdown: bool,
    pal: &Palette,
    enabled: bool,
    active: bool,
) -> (Response, Rect) {
    let tw = text_width(ui, label, font(14.0));
    let mut w = 12.0 + tw + 12.0;
    if icon.is_some() {
        w += 16.0 + 8.0;
    }
    if dropdown {
        w += 8.0 + 8.0;
    }
    let rect = Rect::from_min_size(Pos2::new(x, cy - m::BTN_H / 2.0), Vec2::new(w, m::BTN_H));
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    if enabled {
        subtle_bg(ui, &resp, rect, pal, m::R_CTRL, active);
    }
    let col = if enabled { pal.text } else { pal.text_disabled };
    let p = ui.painter();
    let mut cx = rect.left() + 12.0;
    if let Some((c, ic)) = icon {
        glyph(p, Pos2::new(cx + 8.0, cy), c, 16.0, if enabled { ic } else { pal.text_disabled });
        cx += 16.0 + 8.0;
    }
    p.text(Pos2::new(cx, cy), Align2::LEFT_CENTER, label, font(14.0), col);
    if dropdown {
        glyph(p, Pos2::new(rect.right() - 16.0, cy + 1.0), ICON_CHEVRON_DOWN, 8.0, pal.text2);
    }
    (resp, rect)
}

pub fn vsep(p: &Painter, x: f32, cy: f32, h: f32, pal: &Palette) {
    p.line_segment([Pos2::new(x, cy - h / 2.0), Pos2::new(x, cy + h / 2.0)], Stroke::new(1.0, pal.divider));
}

pub fn hline(p: &Painter, y: f32, x0: f32, x1: f32, color: Color32) {
    p.line_segment([Pos2::new(x0, y + 0.5), Pos2::new(x1, y + 0.5)], Stroke::new(1.0, color));
}

/// Переключатель WinUI (ToggleSwitch)
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str, pal: &Palette) -> Response {
    let h = 36.0;
    let w = ui.available_width();
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = ui.painter();
    p.text(Pos2::new(rect.left(), rect.center().y), Align2::LEFT_CENTER, label, font(14.0), pal.text);
    let sw = Rect::from_center_size(Pos2::new(rect.right() - 20.0, rect.center().y), Vec2::new(40.0, 20.0));
    let t = ui.ctx().animate_bool_with_time(resp.id.with("tg"), *on, 0.12);
    let hovered = resp.hovered();
    if *on {
        p.rect_filled(sw, CornerRadius::same(10), pal.accent);
    } else {
        p.rect_filled(sw, CornerRadius::same(10), if hovered { pal.fill(pal.hover) } else { Color32::TRANSPARENT });
        p.rect_stroke(sw, CornerRadius::same(10), Stroke::new(1.0, pal.text2), StrokeKind::Inside);
    }
    let r = if resp.is_pointer_button_down_on() { 7.0 } else if hovered { 7.0 } else { 6.0 };
    let knob_x = egui::lerp(sw.left() + 10.0..=sw.right() - 10.0, t);
    p.circle_filled(Pos2::new(knob_x, sw.center().y), r, if *on { pal.on_accent } else { pal.text2 });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *on, label));
    resp
}

/// Флажок WinUI (CheckBox)
pub fn checkbox(ui: &mut Ui, on: &mut bool, label: &str, pal: &Palette) -> Response {
    let h = 32.0;
    let tw = text_width(ui, label, font(14.0));
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(20.0 + 8.0 + tw + 4.0, h), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let p = ui.painter();
    let b = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 10.0), Vec2::splat(20.0));
    if *on {
        p.rect_filled(b, CornerRadius::same(4), pal.accent);
        glyph(p, b.center(), ICON_CHECK, 12.0, pal.on_accent);
    } else {
        p.rect_filled(b, CornerRadius::same(4), if resp.hovered() { pal.fill(pal.hover) } else { Color32::TRANSPARENT });
        p.rect_stroke(b, CornerRadius::same(4), Stroke::new(1.0, pal.text2), StrokeKind::Inside);
    }
    p.text(Pos2::new(b.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, label, font(14.0), pal.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *on, label));
    resp
}

/// Радиокнопка WinUI
pub fn radio(ui: &mut Ui, on: bool, label: &str, pal: &Palette) -> Response {
    let h = 32.0;
    let tw = text_width(ui, label, font(14.0));
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(20.0 + 8.0 + tw + 4.0, h), Sense::click());
    let p = ui.painter();
    let c = Pos2::new(rect.left() + 10.0, rect.center().y);
    if on {
        p.circle_filled(c, 10.0, pal.accent);
        p.circle_filled(c, if resp.hovered() { 5.0 } else { 4.0 }, pal.on_accent);
    } else {
        p.circle_filled(c, 10.0, if resp.hovered() { pal.fill(pal.hover) } else { Color32::TRANSPARENT });
        p.circle_stroke(c, 9.5, Stroke::new(1.0, pal.text2));
    }
    p.text(Pos2::new(rect.left() + 28.0, rect.center().y), Align2::LEFT_CENTER, label, font(14.0), pal.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label));
    resp
}

/// Обычная кнопка WinUI (стандартная или акцентная)
pub fn button(ui: &mut Ui, label: &str, accent: bool, pal: &Palette, min_w: f32) -> Response {
    let tw = text_width(ui, label, font(14.0));
    let w = (tw + 24.0).max(min_w);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click());
    let p = ui.painter();
    let (fill, text) = if accent {
        let f = if resp.is_pointer_button_down_on() {
            pal.fill(pal.accent.gamma_multiply(0.8))
        } else if resp.hovered() {
            pal.fill(pal.accent.gamma_multiply(0.9))
        } else {
            pal.accent
        };
        (f, pal.on_accent)
    } else {
        let f = if resp.is_pointer_button_down_on() {
            pal.field
        } else if resp.hovered() {
            pal.field_hover
        } else {
            pal.field
        };
        (f, pal.text)
    };
    let off = if resp.is_pointer_button_down_on() { 0.5 } else { 0.0 };
    p.rect_filled(rect, CornerRadius::same(m::R_CTRL), fill);
    if !accent {
        p.rect_stroke(rect, CornerRadius::same(m::R_CTRL), Stroke::new(1.0, pal.field_stroke), StrokeKind::Inside);
    }
    p.text(rect.center() + Vec2::new(0.0, off), Align2::CENTER_CENTER, label, font(14.0), text);
    resp
}

/// Полоса заполнения диска (как в «Этот компьютер»)
pub fn usage_bar(p: &Painter, rect: Rect, frac: f32, pal: &Palette) {
    p.rect_filled(rect, CornerRadius::same(2), pal.bar_track);
    let full = frac >= 0.9;
    let w = rect.width() * frac.clamp(0.0, 1.0);
    if w > 0.5 {
        let r = Rect::from_min_size(rect.min, Vec2::new(w, rect.height()));
        p.rect_filled(r, CornerRadius::same(2), if full { pal.danger } else { pal.accent });
    }
}

pub fn paint_texture(p: &Painter, tex: &egui::TextureHandle, center: Pos2, max: f32) {
    let s = tex.size_vec2() / p.pixels_per_point();
    let k = (max / s.x.max(s.y).max(1.0)).min(1.0);
    let size = s * k;
    let rect = Rect::from_center_size(center, size);
    p.image(tex.id(), rect.round_to_pixels(p.pixels_per_point()), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
}

/// Вертикальная прокрутка со своей полосой вместо полосы egui: у egui ползунок рисуется
/// растянутым до минимальной длины, а перетаскивание считается по настоящему крошечному,
/// поэтому в больших папках ползунок «вылетал» из-под мыши. `key` - постоянный ключ области
pub fn vscroll<R>(ui: &mut Ui, sa: egui::ScrollArea, key: impl std::hash::Hash, pal: &Palette, f: impl FnOnce(&mut Ui, Rect) -> R) -> R {
    let id = egui::Id::new(("oxide-vscroll", key));
    let pending: Option<f32> = ui.data_mut(|d| d.remove_temp::<f32>(id.with("to")));
    let mut sa = sa.scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(o) = pending {
        sa = sa.vertical_scroll_offset(o);
    }
    let out = sa.show_viewport(ui, f);
    if let Some(o) = scrollbar(ui, id, out.inner_rect, out.content_size.y, out.state.offset.y, pal) {
        ui.data_mut(|d| d.insert_temp(id.with("to"), o));
        ui.ctx().request_repaint();
    }
    out.inner
}

/// Сбросить отложенную прокрутку области (при переходе в другую папку)
pub fn vscroll_forget(ui: &Ui, key: impl std::hash::Hash) {
    let id = egui::Id::new(("oxide-vscroll", key));
    ui.data_mut(|d| d.remove_temp::<f32>(id.with("to")));
}

/// Полоса прокрутки как в Windows 11: тонкая видна всегда, при наведении шире.
/// Ползунок не короче SCROLL_HANDLE_MIN, и тянется ровно тот ползунок, что нарисован.
/// Щелчок по полосе выше или ниже ползунка листает на страницу. Возвращает новое смещение
pub fn scrollbar(ui: &mut Ui, id: egui::Id, area: Rect, content: f32, offset: f32, pal: &Palette) -> Option<f32> {
    let view = area.height();
    if content <= view + 0.5 || view < 24.0 {
        return None;
    }
    let max_off = content - view;
    let pad = 2.0;
    let track = Rect::from_min_max(Pos2::new(area.right() - m::SCROLL_W - pad * 2.0, area.top() + pad), Pos2::new(area.right(), area.bottom() - pad));
    let len = (track.height() * view / content).max(m::SCROLL_HANDLE_MIN).min(track.height());
    let travel = (track.height() - len).max(1.0);
    let off = offset.clamp(0.0, max_off);
    let top = track.top() + travel * off / max_off;
    let resp = ui.interact(track, id, Sense::click_and_drag());
    let grab_id = id.with("grab");
    let mut new = None;
    if resp.drag_started() {
        if let Some(p) = resp.interact_pointer_pos() {
            // Схватили ползунок - держим его за то же место; мимо - ползунок прыгает серединой под мышь
            let g = if p.y >= top && p.y <= top + len { p.y - top } else { len / 2.0 };
            ui.data_mut(|d| d.insert_temp(grab_id, g));
        }
    }
    if resp.dragged() || resp.drag_started() {
        if let (Some(p), Some(g)) = (resp.interact_pointer_pos(), ui.data(|d| d.get_temp::<f32>(grab_id))) {
            let t = ((p.y - g - track.top()) / travel).clamp(0.0, 1.0);
            new = Some(t * max_off);
        }
    } else if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            if p.y < top {
                new = Some((off - view).max(0.0));
            } else if p.y > top + len {
                new = Some((off + view).min(max_off));
            }
        }
    }
    let active = resp.hovered() || resp.dragged();
    let t = ui.ctx().animate_bool_with_time(id.with("w"), active, m::T_HOVER);
    let w = m::SCROLL_W_IDLE + (m::SCROLL_W - m::SCROLL_W_IDLE) * t;
    let draw_top = new.map(|n| track.top() + travel * n / max_off).unwrap_or(top);
    let hr = Rect::from_min_size(Pos2::new(track.right() - pad - w, draw_top), Vec2::new(w, len));
    let a = if resp.dragged() {
        0.75
    } else if resp.hovered() {
        0.55
    } else {
        0.35
    };
    ui.painter().rect_filled(hr, CornerRadius::same((w / 2.0) as u8), pal.fill(pal.text.gamma_multiply(a)));
    new
}

/// Ползунок WinUI (Slider): дорожка, заполненная акцентом до бегунка, подпись слева и значение в процентах
pub fn slider(ui: &mut Ui, value: &mut f32, label: &str, pal: &Palette) -> Response {
    let h = 36.0;
    let w = ui.available_width();
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    let track = Rect::from_min_max(Pos2::new(rect.right() - 260.0, rect.center().y - 2.0), Pos2::new(rect.right() - 48.0, rect.center().y + 2.0));
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let v = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            if (v - *value).abs() > f32::EPSILON {
                *value = v;
                resp.mark_changed();
            }
        }
    }
    if resp.has_focus() {
        let step = ui.input(|i| (i.key_pressed(egui::Key::ArrowRight) as i32 - i.key_pressed(egui::Key::ArrowLeft) as i32) as f32 * 0.05);
        if step != 0.0 {
            *value = (*value + step).clamp(0.0, 1.0);
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    p.text(Pos2::new(rect.left(), rect.center().y), Align2::LEFT_CENTER, label, font(14.0), pal.text);
    p.rect_filled(track, CornerRadius::same(2), pal.text3.gamma_multiply(0.6));
    let x = track.left() + track.width() * *value;
    p.rect_filled(Rect::from_min_max(track.min, Pos2::new(x, track.max.y)), CornerRadius::same(2), pal.accent);
    let c = Pos2::new(x, track.center().y);
    p.circle_filled(c, 10.0, pal.flyout);
    p.circle_stroke(c, 10.0, Stroke::new(1.0, pal.flyout_stroke));
    let r = if resp.dragged() { 5.0 } else if resp.hovered() { 7.0 } else { 6.0 };
    p.circle_filled(c, r, pal.accent);
    text_right(p, Pos2::new(rect.right(), rect.center().y), &format!("{} %", (*value * 100.0).round() as i32), font(14.0), pal.text2);
    let v = *value;
    resp.widget_info(|| egui::WidgetInfo::slider(true, v as f64, label));
    resp
}

/// Блик по краю стеклянного элемента, как в «Жидком стекле» iOS 26: свет падает со стороны
/// указателя мыши (без мыши - слева сверху), напротив - слабый отражённый блик, по всему краю
/// тонкий радужный перелив. Рисуется только в теме «Жидкое стекло»
pub fn glass_rim(p: &Painter, r: Rect, radius: f32, pal: &Palette) {
    if !pal.glass || pal.shine <= 0.01 {
        return;
    }
    use egui::epaint::{ColorMode, CornerRadiusF32, PathShape, PathStroke};
    let c = r.center();
    let ptr = p.ctx().pointer_latest_pos();
    let light = ptr
        .map(|q| q - c)
        .filter(|v| v.length() > 1.0)
        .map(|v| v.normalized())
        .unwrap_or(Vec2::new(-0.7071, -0.7071));
    let phase = ptr.map(|q| (q.x + q.y) / 1400.0).unwrap_or(0.0);
    let half = r.size() / 2.0;
    let (shine, dark) = (pal.shine, pal.dark);
    let make = move |strength: f32| {
        std::sync::Arc::new(move |_: Rect, q: Pos2| -> Color32 {
            let d = q - c;
            let n = Vec2::new(d.x / half.x.max(1.0), d.y / half.y.max(1.0));
            let n = if n.length() > 0.001 { n.normalized() } else { Vec2::new(0.0, -1.0) };
            let dot = n.x * light.x + n.y * light.y;
            let spec = (dot.max(0.0).powi(3) + 0.45 * (-dot).max(0.0).powi(4)).min(1.0);
            let hue = (n.y.atan2(n.x) / std::f32::consts::TAU + phase).rem_euclid(1.0);
            let rainbow: Color32 = egui::epaint::Hsva::new(hue, 0.6, 1.0, 1.0).into();
            // В центре блика свет белый, по краям - цветной перелив
            let k = spec * 0.75;
            let mix = |v: u8| (v as f32 * (1.0 - k) + 255.0 * k).round() as u8;
            let rest = if dark { 0.14 } else { 0.3 };
            let a = (shine * strength * (rest + 0.86 * spec)).clamp(0.0, 1.0);
            Color32::from_rgba_unmultiplied(mix(rainbow.r()), mix(rainbow.g()), mix(rainbow.b()), (a * 255.0) as u8)
        }) as std::sync::Arc<dyn Fn(Rect, Pos2) -> Color32 + Send + Sync>
    };
    // Тонкая яркая кромка и мягкое свечение внутри: стекло выглядит толстым
    for (inset, width, strength) in [(0.6, 1.2, 1.0), (2.2, 3.0, 0.22)] {
        let rr = r.shrink(inset);
        if rr.width() < 2.0 || rr.height() < 2.0 {
            continue;
        }
        let mut pts = Vec::new();
        egui::epaint::tessellator::path::rounded_rectangle(&mut pts, rr, CornerRadiusF32::same((radius - inset).max(0.0)));
        p.add(PathShape { points: pts, closed: true, fill: Color32::TRANSPARENT, stroke: PathStroke { width, color: ColorMode::UV(make(strength)), kind: StrokeKind::Middle } });
    }
}
