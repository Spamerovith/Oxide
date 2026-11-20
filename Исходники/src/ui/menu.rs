//! Всплывающие меню в стиле Windows 11: контекстное меню с верхней строкой значков,
//! выпадающие списки командной панели, вложенные меню, управление с клавиатуры

use egui::{Align2, Color32, CornerRadius, Id, Key, Order, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};

use super::widgets::*;
use crate::cmd::Cmd;
use crate::theme::{m, Palette};

#[derive(Clone)]
pub enum MIcon {
    None,
    Glyph(char, Option<Color32>),
    /// Системный значок: (ключ кэша, путь)
    Sys(String, String),
}

#[derive(Clone)]
pub struct MenuItem {
    pub cmd: Option<Cmd>,
    pub label: String,
    pub icon: MIcon,
    pub shortcut: String,
    pub enabled: bool,
    pub check: u8,
    pub sub: Vec<MenuItem>,
    pub sep: bool,
    /// Действие по умолчанию - полужирным, как в Проводнике
    pub bold: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, cmd: Cmd) -> Self {
        MenuItem { cmd: Some(cmd), label: label.into(), icon: MIcon::None, shortcut: String::new(), enabled: true, check: 0, sub: vec![], sep: false, bold: false }
    }
    pub fn submenu(label: impl Into<String>, sub: Vec<MenuItem>) -> Self {
        MenuItem { cmd: None, label: label.into(), icon: MIcon::None, shortcut: String::new(), enabled: true, check: 0, sub, sep: false, bold: false }
    }
    pub fn sep() -> Self {
        MenuItem { cmd: None, label: String::new(), icon: MIcon::None, shortcut: String::new(), enabled: false, check: 0, sub: vec![], sep: true, bold: false }
    }
    pub fn icon(mut self, c: char) -> Self {
        self.icon = MIcon::Glyph(c, None);
        self
    }
    pub fn icon_col(mut self, c: char, col: Color32) -> Self {
        self.icon = MIcon::Glyph(c, Some(col));
        self
    }
    pub fn sys(mut self, key: String, path: String) -> Self {
        self.icon = MIcon::Sys(key, path);
        self
    }
    pub fn key(mut self, s: &str) -> Self {
        self.shortcut = s.into();
        self
    }
    pub fn enabled(mut self, b: bool) -> Self {
        self.enabled = b;
        self
    }
    pub fn checked(mut self, b: bool) -> Self {
        self.check = if b { 1 } else { 3 };
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn radio(mut self, b: bool) -> Self {
        self.check = if b { 2 } else { 3 };
        self
    }
}

pub struct TopButton {
    pub icon: char,
    pub cmd: Cmd,
    pub tip: String,
    pub enabled: bool,
}

pub struct Popup {
    pub id: u64,
    pub pos: Pos2,
    pub items: Vec<MenuItem>,
    pub top: Vec<TopButton>,
    pub opened: f64,
    pub anchor: Option<Rect>,
    pub min_w: f32,
    sub_open: Option<(usize, Rect)>,
    sub_hover_since: Option<(usize, f64)>,
    kb: Option<usize>,
    sub_kb: Option<usize>,
    pub rects: Vec<Rect>,
    pub up: bool,
}

pub enum PopupResult {
    Open,
    Cmd(Cmd),
    Close,
}

impl Popup {
    pub fn new(id: u64, pos: Pos2, items: Vec<MenuItem>, now: f64) -> Self {
        Popup {
            id,
            pos,
            items,
            top: vec![],
            opened: now,
            anchor: None,
            min_w: 0.0,
            sub_open: None,
            sub_hover_since: None,
            kb: None,
            sub_kb: None,
            rects: vec![],
            up: false,
        }
    }
    pub fn below(id: u64, anchor: Rect, items: Vec<MenuItem>, now: f64) -> Self {
        let mut p = Popup::new(id, Pos2::new(anchor.left(), anchor.bottom() + 4.0), items, now);
        p.anchor = Some(anchor);
        p
    }
    pub fn with_top(mut self, top: Vec<TopButton>) -> Self {
        self.top = top;
        self
    }
}

fn item_h(it: &MenuItem) -> f32 {
    if it.sep {
        9.0
    } else {
        m::MENU_ROW_H
    }
}

fn measure(ctx: &egui::Context, items: &[MenuItem], min_w: f32) -> (f32, f32, bool) {
    let has_icon = items.iter().any(|i| !matches!(i.icon, MIcon::None) || i.check != 0);
    let mut w: f32 = 0.0;
    let mut h = 8.0;
    ctx.fonts(|f| {
        for it in items {
            h += item_h(it);
            if it.sep {
                continue;
            }
            let lw = f.layout_no_wrap(it.label.clone(), if it.bold { font_sb(14.0) } else { font(14.0) }, Color32::WHITE).size().x;
            let sw = if it.shortcut.is_empty() { 0.0 } else { f.layout_no_wrap(it.shortcut.clone(), font(12.0), Color32::WHITE).size().x + 32.0 };
            let left = if has_icon { 44.0 } else { 16.0 };
            let right = if !it.sub.is_empty() { 36.0 } else { 16.0 };
            w = w.max(left + lw + sw + right);
        }
    });
    (w.max(min_w).max(180.0).min(520.0), h, has_icon)
}

/// Рисует меню. Возвращает выбранную команду или закрытие
pub fn show(ctx: &egui::Context, pop: &mut Popup, pal: &Palette, icons: &mut crate::shell::sysicons::IconCache, animations: bool) -> PopupResult {
    crate::theme::set_surface(pal.flyout);
    let now = ctx.input(|i| i.time);
    let screen = ctx.screen_rect();
    let (w, mut h, has_icon) = measure(ctx, &pop.items, pop.min_w);
    let top_h = if pop.top.is_empty() { 0.0 } else { 48.0 };
    h += top_h;
    let w = if pop.top.is_empty() { w } else { w.max(pop.top.len() as f32 * 40.0 + 16.0) };
    let mut pos = pop.pos;
    if pos.x + w > screen.right() - 4.0 {
        pos.x = (screen.right() - 4.0 - w).max(screen.left() + 4.0);
    }
    if pos.y + h > screen.bottom() - 4.0 {
        let alt = match pop.anchor {
            Some(a) => a.top() - 4.0 - h,
            None => pos.y - h,
        };
        pos.y = alt.max(screen.top() + 4.0);
        pop.up = true;
    }
    let rect = Rect::from_min_size(pos, Vec2::new(w, h));
    let dur = if animations { m::T_MENU } else { 0.0 };
    let t = if dur > 0.0 { (((now - pop.opened) as f32) / dur).clamp(0.0, 1.0) } else { 1.0 };
    let ease = 1.0 - (1.0 - t).powi(3);
    if t < 1.0 {
        ctx.request_repaint();
    }
    let dy = (1.0 - ease) * if pop.up { 8.0 } else { -8.0 };
    let alpha = ease;

    let mut result = PopupResult::Open;
    let mut rects = vec![rect];

    // Клавиатура
    let (up, down, enter, esc, right, left) = ctx.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::Enter) || i.key_pressed(Key::Space),
            i.key_pressed(Key::Escape),
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::ArrowLeft),
        )
    });
    let selectable: Vec<usize> = pop.items.iter().enumerate().filter(|(_, it)| !it.sep && it.enabled).map(|(i, _)| i).collect();
    if pop.sub_open.is_none() || pop.sub_kb.is_none() {
        if down || up {
            if !selectable.is_empty() {
                let cur = pop.kb.and_then(|k| selectable.iter().position(|&s| s == k));
                let next = match (cur, down) {
                    (None, true) => 0,
                    (None, false) => selectable.len() - 1,
                    (Some(c), true) => (c + 1) % selectable.len(),
                    (Some(c), false) => (c + selectable.len() - 1) % selectable.len(),
                };
                pop.kb = Some(selectable[next]);
            }
        }
        if esc {
            if pop.sub_open.is_some() {
                pop.sub_open = None;
            } else {
                return PopupResult::Close;
            }
        }
        if enter {
            if let Some(k) = pop.kb {
                let it = &pop.items[k];
                if !it.sub.is_empty() {
                    pop.sub_kb = Some(0);
                } else if let Some(c) = &it.cmd {
                    return PopupResult::Cmd(c.clone());
                }
            }
        }
        if right {
            if let Some(k) = pop.kb {
                if !pop.items[k].sub.is_empty() {
                    pop.sub_kb = Some(0);
                }
            }
        }
    }

    let area = egui::Area::new(Id::new(("popup", pop.id)))
        .order(Order::Foreground)
        .fixed_pos(pos + Vec2::new(0.0, dy))
        .interactable(true);
    let mut hovered_item: Option<(usize, Rect)> = None;
    area.show(ctx, |ui| {
        ui.set_opacity(alpha);
        let rect = Rect::from_min_size(ui.max_rect().min, Vec2::new(w, h));
        let painter = ui.painter();
        let shadow = egui::Shadow { offset: [0, 8], blur: 16, spread: 0, color: pal.shadow };
        painter.add(shadow.as_shape(rect, CornerRadius::same(m::R_OVERLAY)));
        painter.rect_filled(rect, CornerRadius::same(m::R_OVERLAY), pal.flyout);
        painter.rect_stroke(rect, CornerRadius::same(m::R_OVERLAY), Stroke::new(1.0, pal.flyout_stroke), StrokeKind::Inside);
        let bg = ui.interact(rect, Id::new(("popup-bg", pop.id)), Sense::click());
        let _ = bg;

        let mut y = rect.top() + 4.0;
        if !pop.top.is_empty() {
            let n = pop.top.len() as f32;
            let bw = 40.0;
            let x0 = rect.left() + 8.0;
            let _ = n;
            for (i, tb) in pop.top.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(x0 + i as f32 * bw, y + 4.0), Vec2::new(bw - 4.0, 36.0));
                let resp = icon_button(ui, r, Id::new(("popup-top", pop.id, i)), tb.icon, 16.0, pal, tb.enabled, &tb.tip);
                if resp.clicked() {
                    result = PopupResult::Cmd(tb.cmd.clone());
                }
            }
            y += top_h - 4.0;
            hline(ui.painter(), y - 1.0, rect.left(), rect.right(), pal.flyout_stroke);
            y += 4.0;
        }
        for (i, it) in pop.items.iter().enumerate() {
            let ih = item_h(it);
            let r = Rect::from_min_size(Pos2::new(rect.left() + 4.0, y), Vec2::new(w - 8.0, ih));
            y += ih;
            if it.sep {
                hline(ui.painter(), r.center().y, rect.left(), rect.right(), pal.flyout_stroke);
                continue;
            }
            let resp = ui.interact(r, Id::new(("popup-item", pop.id, i)), if it.enabled { Sense::click() } else { Sense::hover() });
            if resp.hovered() {
                pop.kb = Some(i);
                hovered_item = Some((i, r));
            }
            let hl = it.enabled && (pop.kb == Some(i) || pop.sub_open.map(|s| s.0) == Some(i));
            if hl {
                let col = if resp.is_pointer_button_down_on() { pal.fill(pal.pressed) } else { pal.fill(pal.hover) };
                ui.painter().rect_filled(r, CornerRadius::same(m::R_CTRL), col);
            }
            let tc = if it.enabled { pal.text } else { pal.text_disabled };
            let cy = r.center().y;
            let ix = r.left() + 20.0;
            match &it.icon {
                MIcon::Glyph(c, col) => glyph(ui.painter(), Pos2::new(ix, cy), *c, 16.0, if it.enabled { col.unwrap_or(pal.text) } else { pal.text_disabled }),
                MIcon::Sys(key, path) => {
                    let px = (16.0 * ui.ctx().pixels_per_point()).round() as u32;
                    if let Some(tex) = icons.get(key, path, px, false) {
                        paint_texture(ui.painter(), tex, Pos2::new(ix, cy), 16.0);
                    }
                }
                MIcon::None => {}
            }
            if it.check == 1 {
                glyph(ui.painter(), Pos2::new(ix, cy), ICON_CHECK, 12.0, tc);
            } else if it.check == 2 {
                ui.painter().circle_filled(Pos2::new(ix, cy), 3.5, tc);
            }
            let tx = if has_icon { r.left() + 40.0 } else { r.left() + 12.0 };
            ui.painter().text(Pos2::new(tx, cy), Align2::LEFT_CENTER, &it.label, if it.bold { font_sb(14.0) } else { font(14.0) }, tc);
            if !it.shortcut.is_empty() {
                let rx = if it.sub.is_empty() { r.right() - 12.0 } else { r.right() - 32.0 };
                ui.painter().text(Pos2::new(rx, cy), Align2::RIGHT_CENTER, &it.shortcut, font(12.0), pal.text3);
            }
            if !it.sub.is_empty() {
                glyph(ui.painter(), Pos2::new(r.right() - 16.0, cy), ICON_CHEVRON_RIGHT, 10.0, pal.text2);
            }
            if resp.clicked() {
                if !it.sub.is_empty() {
                    pop.sub_open = Some((i, r));
                } else if let Some(c) = &it.cmd {
                    result = PopupResult::Cmd(c.clone());
                }
            }
        }
    });

    // Открытие вложенного меню при наведении (с небольшой задержкой, как в Windows)
    if let Some((i, r)) = hovered_item {
        if !pop.items[i].sub.is_empty() {
            match pop.sub_hover_since {
                Some((j, t0)) if j == i => {
                    if now - t0 > 0.25 && pop.sub_open.map(|s| s.0) != Some(i) {
                        pop.sub_open = Some((i, r));
                        pop.sub_kb = None;
                    } else {
                        ctx.request_repaint_after(std::time::Duration::from_millis(60));
                    }
                }
                _ => {
                    pop.sub_hover_since = Some((i, now));
                    ctx.request_repaint_after(std::time::Duration::from_millis(60));
                }
            }
        } else {
            pop.sub_hover_since = None;
            if pop.sub_open.is_some() && now - pop.opened > 0.1 {
                pop.sub_open = None;
            }
        }
    }
    if pop.sub_kb.is_some() && pop.sub_open.is_none() {
        if let Some(k) = pop.kb {
            // Открытие подменю клавиатурой: вычисляем прямоугольник строки
            let mut y = rect.top() + 4.0 + if top_h > 0.0 { top_h } else { 0.0 };
            for it in pop.items.iter().take(k) {
                y += item_h(it);
            }
            pop.sub_open = Some((k, Rect::from_min_size(Pos2::new(rect.left() + 4.0, y), Vec2::new(w - 8.0, m::MENU_ROW_H))));
        }
    }

    // Вложенное меню
    if let Some((si, srow)) = pop.sub_open {
        let sub = pop.items[si].sub.clone();
        let (sw, sh, s_icon) = measure(ctx, &sub, 0.0);
        let mut sp = Pos2::new(rect.right() - 4.0, srow.top() - 4.0);
        if sp.x + sw > screen.right() - 4.0 {
            sp.x = rect.left() - sw + 4.0;
        }
        if sp.y + sh > screen.bottom() - 4.0 {
            sp.y = (screen.bottom() - 4.0 - sh).max(screen.top() + 4.0);
        }
        let srect = Rect::from_min_size(sp, Vec2::new(sw, sh));
        rects.push(srect);
        // Клавиатура в подменю
        if let Some(k) = pop.sub_kb {
            let sel: Vec<usize> = sub.iter().enumerate().filter(|(_, it)| !it.sep && it.enabled).map(|(i, _)| i).collect();
            if !sel.is_empty() {
                let cur = sel.iter().position(|&s| s == k).unwrap_or(0);
                if down {
                    pop.sub_kb = Some(sel[(cur + 1) % sel.len()]);
                } else if up {
                    pop.sub_kb = Some(sel[(cur + sel.len() - 1) % sel.len()]);
                } else if pop.sub_kb == Some(0) && !sel.contains(&0) {
                    pop.sub_kb = Some(sel[0]);
                }
                if enter {
                    if let Some(c) = sub.get(pop.sub_kb.unwrap_or(0)).and_then(|i| i.cmd.clone()) {
                        return PopupResult::Cmd(c);
                    }
                }
            }
            if left || esc {
                pop.sub_kb = None;
                pop.sub_open = None;
            }
        }
        egui::Area::new(Id::new(("popup-sub", pop.id, si))).order(Order::Foreground).fixed_pos(sp).show(ctx, |ui| {
            let painter = ui.painter();
            let shadow = egui::Shadow { offset: [0, 8], blur: 16, spread: 0, color: pal.shadow };
            painter.add(shadow.as_shape(srect, CornerRadius::same(m::R_OVERLAY)));
            painter.rect_filled(srect, CornerRadius::same(m::R_OVERLAY), pal.flyout);
            painter.rect_stroke(srect, CornerRadius::same(m::R_OVERLAY), Stroke::new(1.0, pal.flyout_stroke), StrokeKind::Inside);
            let mut y = srect.top() + 4.0;
            for (i, it) in sub.iter().enumerate() {
                let ih = item_h(it);
                let r = Rect::from_min_size(Pos2::new(srect.left() + 4.0, y), Vec2::new(sw - 8.0, ih));
                y += ih;
                if it.sep {
                    hline(ui.painter(), r.center().y, srect.left(), srect.right(), pal.flyout_stroke);
                    continue;
                }
                let resp = ui.interact(r, Id::new(("popup-subitem", pop.id, si, i)), if it.enabled { Sense::click() } else { Sense::hover() });
                if resp.hovered() {
                    pop.sub_kb = Some(i);
                }
                if it.enabled && pop.sub_kb == Some(i) {
                    ui.painter().rect_filled(r, CornerRadius::same(m::R_CTRL), pal.fill(pal.hover));
                }
                let tc = if it.enabled { pal.text } else { pal.text_disabled };
                let cy = r.center().y;
                let ix = r.left() + 20.0;
                match &it.icon {
                    MIcon::Glyph(c, col) => glyph(ui.painter(), Pos2::new(ix, cy), *c, 16.0, col.unwrap_or(tc)),
                    MIcon::Sys(key, path) => {
                        let px = (16.0 * ui.ctx().pixels_per_point()).round() as u32;
                        if let Some(tex) = icons.get(key, path, px, false) {
                            paint_texture(ui.painter(), tex, Pos2::new(ix, cy), 16.0);
                        }
                    }
                    MIcon::None => {}
                }
                if it.check == 1 {
                    glyph(ui.painter(), Pos2::new(ix, cy), ICON_CHECK, 12.0, tc);
                } else if it.check == 2 {
                    ui.painter().circle_filled(Pos2::new(ix, cy), 3.5, tc);
                }
                let tx = if s_icon { r.left() + 40.0 } else { r.left() + 12.0 };
                ui.painter().text(Pos2::new(tx, cy), Align2::LEFT_CENTER, &it.label, if it.bold { font_sb(14.0) } else { font(14.0) }, tc);
                if !it.shortcut.is_empty() {
                    ui.painter().text(Pos2::new(r.right() - 12.0, cy), Align2::RIGHT_CENTER, &it.shortcut, font(12.0), pal.text3);
                }
                if resp.clicked() {
                    if let Some(c) = &it.cmd {
                        result = PopupResult::Cmd(c.clone());
                    }
                }
            }
        });
    }
    pop.rects = rects.clone();

    // Закрытие по щелчку вне меню
    if matches!(result, PopupResult::Open) {
        let pressed = ctx.input(|i| i.pointer.any_pressed());
        if pressed && now - pop.opened > 0.05 {
            if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
                let inside = rects.iter().any(|r| r.contains(p));
                if !inside {
                    return PopupResult::Close;
                }
            }
        }
        if ctx.input(|i| !i.focused) && now - pop.opened > 0.2 {
            return PopupResult::Close;
        }
    }
    let _ = Shape::Noop;
    result
}
