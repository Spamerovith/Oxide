//! Область содержимого: таблица, значки (огромные…мелкие), список, плитка, содержимое
//! Виртуализация: рисуются только видимые элементы, поэтому папки на 100 000 файлов не тормозят

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::emath::GuiRounding;
use egui::{Align2, Color32, CornerRadius, Id, Key, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2};

use super::widgets::*;
use crate::app::{now, Marquee, OxideApp};
use crate::cmd::{Cmd, SortCol, ViewMode};
use crate::shell::fs::{self, Entry};
use crate::shell::{shellfolder, sysicons};
use crate::theme::{m, Palette};

#[derive(Clone, Copy, PartialEq)]
enum Flow {
    Rows,
    Grid,
    Columns,
}

#[derive(Clone, Copy)]
pub struct Lay {
    flow: Flow,
    cell: Vec2,
    cols: usize,
    rows: usize,
    pad: Vec2,
    /// Шаг столбцов сетки: как в Проводнике, свободное место по ширине делится между столбцами поровну
    step_x: f32,
    pub total: Vec2,
}

impl Lay {
    fn rect(&self, pos: usize) -> Rect {
        let (c, r) = match self.flow {
            Flow::Rows => (0, pos),
            Flow::Grid => (pos % self.cols.max(1), pos / self.cols.max(1)),
            Flow::Columns => (pos / self.rows.max(1), pos % self.rows.max(1)),
        };
        let x = self.pad.x + c as f32 * self.step_x + (self.step_x - self.cell.x) / 2.0;
        Rect::from_min_size(Pos2::new(x.round(), self.pad.y + r as f32 * self.cell.y), self.cell)
    }
    fn visible(&self, clip: Rect, n: usize) -> std::ops::Range<usize> {
        match self.flow {
            Flow::Rows => {
                let a = ((clip.top() - self.pad.y) / self.cell.y).floor().max(0.0) as usize;
                let b = ((clip.bottom() - self.pad.y) / self.cell.y).ceil().max(0.0) as usize + 1;
                a.min(n)..b.min(n)
            }
            Flow::Grid => {
                let a = ((clip.top() - self.pad.y) / self.cell.y).floor().max(0.0) as usize * self.cols;
                let b = (((clip.bottom() - self.pad.y) / self.cell.y).ceil().max(0.0) as usize + 1) * self.cols;
                a.min(n)..b.min(n)
            }
            Flow::Columns => {
                let a = ((clip.left() - self.pad.x) / self.cell.x).floor().max(0.0) as usize * self.rows;
                let b = (((clip.right() - self.pad.x) / self.cell.x).ceil().max(0.0) as usize + 1) * self.rows;
                a.min(n)..b.min(n)
            }
        }
    }
}

pub struct Col {
    pub id: SortCol,
    pub title: &'static str,
    pub w: f32,
    pub slot: usize,
    pub right: bool,
}

pub fn columns(app: &OxideApp) -> Vec<Col> {
    let w = app.settings.col_widths;
    let t = app.tab();
    let bin = matches!(&t.loc, crate::app::Location::Shell(s) if s.eq_ignore_ascii_case(shellfolder::RECYCLE_BIN));
    let mut v = vec![Col { id: SortCol::Name, title: tr("Имя"), w: w[0], slot: 0, right: false }];
    if bin {
        v.push(Col { id: SortCol::Location, title: tr("Исходное расположение"), w: w[4], slot: 4, right: false });
        v.push(Col { id: SortCol::Extra, title: tr("Дата удаления"), w: w[5], slot: 5, right: false });
        v.push(Col { id: SortCol::Size, title: tr("Размер"), w: w[3], slot: 3, right: true });
        v.push(Col { id: SortCol::Type, title: tr("Тип элемента"), w: w[2], slot: 2, right: false });
        return v;
    }
    if t.searching() {
        v.push(Col { id: SortCol::Location, title: tr("Папка"), w: w[4], slot: 4, right: false });
    }
    v.push(Col { id: SortCol::Modified, title: tr("Дата изменения"), w: w[1], slot: 1, right: false });
    v.push(Col { id: SortCol::Type, title: tr("Тип"), w: w[2], slot: 2, right: false });
    v.push(Col { id: SortCol::Size, title: tr("Размер"), w: w[3], slot: 3, right: true });
    v
}

pub fn layout(view: ViewMode, n: usize, avail: Vec2, compact: bool, cols_w: f32) -> Lay {
    let row_h = if compact { m::ROW_H_COMPACT } else { m::ROW_H };
    let (flow, cell) = match view {
        ViewMode::Details => (Flow::Rows, Vec2::new(cols_w.max(avail.x - 8.0), row_h)),
        ViewMode::Content => (Flow::Rows, Vec2::new(avail.x - 8.0, if compact { 52.0 } else { 60.0 })),
        ViewMode::Tiles => (Flow::Grid, Vec2::new(272.0, if compact { 68.0 } else { 76.0 })),
        ViewMode::ExtraLarge => (Flow::Grid, Vec2::new(272.0, 256.0 + 56.0)),
        ViewMode::Large => (Flow::Grid, Vec2::new(112.0, 96.0 + 56.0)),
        ViewMode::Medium => (Flow::Grid, Vec2::new(88.0, 48.0 + 52.0)),
        ViewMode::Small => (Flow::Grid, Vec2::new(232.0, row_h)),
        ViewMode::List => (Flow::Columns, Vec2::new(260.0, row_h)),
    };
    let pad = match flow {
        Flow::Rows => Vec2::new(4.0, 4.0),
        _ => Vec2::new(12.0, 8.0),
    };
    let (cols, rows) = match flow {
        Flow::Rows => (1, n),
        Flow::Grid => {
            let c = (((avail.x - pad.x * 2.0) / cell.x).floor() as usize).max(1);
            (c, n.div_ceil(c))
        }
        Flow::Columns => {
            let r = (((avail.y - pad.y * 2.0) / cell.y).floor() as usize).max(1);
            (n.div_ceil(r), r)
        }
    };
    let total = match flow {
        Flow::Rows => Vec2::new(cell.x + pad.x * 2.0, rows as f32 * cell.y + pad.y * 2.0 + 24.0),
        Flow::Grid => Vec2::new(avail.x, rows as f32 * cell.y + pad.y * 2.0 + 24.0),
        Flow::Columns => Vec2::new(cols as f32 * cell.x + pad.x * 2.0, avail.y),
    };
    let step_x = match flow {
        Flow::Grid => ((avail.x - pad.x * 2.0) / cols as f32).max(cell.x),
        _ => cell.x,
    };
    Lay { flow, cell, cols, rows, pad, step_x, total }
}

/// Ключ и путь системного значка для элемента
pub fn icon_req(e: &Entry, thumbs: bool) -> (String, String, bool) {
    let path = e.shell.clone().unwrap_or_else(|| e.path.to_string_lossy().to_string());
    if e.shell.is_some() {
        return (path.clone(), path, false);
    }
    if e.is_dir {
        if thumbs || e.customized_dir() {
            return (path.clone(), path, thumbs);
        }
        return ("<dir>".into(), path, false);
    }
    if thumbs && sysicons::thumbable(&e.ext) {
        return (path.clone(), path, true);
    }
    if sysicons::per_file_icon(&e.ext) {
        return (path.clone(), path, false);
    }
    (format!(".{}", e.ext), path, false)
}

fn paint_icon(app: &mut OxideApp, ui: &Ui, e: &Entry, center: Pos2, size: f32, thumbs: bool, dim: bool) {
    let ppp = ui.ctx().pixels_per_point();
    let (key, path, thumb) = icon_req(e, thumbs);
    let px = (size * ppp).round() as u32;
    let tint = if dim { Color32::from_white_alpha(128) } else { Color32::WHITE };
    if let Some(tex) = app.icons.get(&key, &path, px, thumb) {
        let s = tex.size_vec2() / ppp;
        let k = (size / s.x.max(s.y).max(1.0)).min(1.0);
        let r = Rect::from_center_size(center, s * k).round_to_pixels(ppp);
        ui.painter().image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
        if thumb && !e.is_dir && size >= 48.0 && s.x.max(s.y) * k > size * 0.6 {
            // Тонкая рамка вокруг эскизов изображений, как в Проводнике
            ui.painter().rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_black_alpha(30)), StrokeKind::Outside);
        }
    } else {
        // Пока значок грузится - лёгкий значок-заглушка того же размера (без «прыжка» раскладки)
        let g = if e.is_dir { ICON_FOLDER } else { ICON_DOC };
        glyph(ui.painter(), center, g, (size * 0.8).min(48.0), app.pal.text3.gamma_multiply(0.6));
    }
}

fn header(app: &mut OxideApp, ui: &mut Ui, r: Rect) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let cols = columns(app);
    let sort = app.tab().view.sort;
    let desc = app.tab().view.desc;
    let mut x = r.left() + 4.0;
    let mut sort_click: Option<SortCol> = None;
    let mut resize: Option<(usize, f32)> = None;
    for c in cols.iter() {
        let cr = Rect::from_min_size(Pos2::new(x, r.top()), Vec2::new(c.w, r.height()));
        let resp = ui.interact(cr.shrink2(Vec2::new(4.0, 0.0)), Id::new(("hdr", c.slot)), Sense::click());
        subtle_bg(ui, &resp, cr.shrink2(Vec2::new(1.0, 4.0)), &pal, m::R_CTRL, false);
        let cy = cr.center().y + 2.0;
        if c.right {
            text_right(ui.painter(), Pos2::new(cr.right() - 12.0, cy), c.title, font(12.0), pal.text2);
        } else {
            text_trunc(ui.painter(), Pos2::new(cr.left() + 10.0, cy), c.title, font(12.0), pal.text2, cr.width() - 16.0);
        }
        if sort == c.id {
            glyph(ui.painter(), Pos2::new(cr.center().x, cr.top() + 6.0), if desc { ICON_CHEVRON_DOWN } else { ICON_CHEVRON_UP }, 8.0, pal.text3);
        }
        if resp.clicked() {
            sort_click = Some(c.id);
        }
        // Граница столбца - перетаскивание меняет ширину
        let edge = Rect::from_center_size(Pos2::new(cr.right(), cr.center().y), Vec2::new(8.0, r.height()));
        let er = ui.interact(edge, Id::new(("hdr-edge", c.slot)), Sense::drag());
        if er.hovered() || er.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        vsep(ui.painter(), cr.right(), cr.center().y, if er.hovered() { r.height() - 8.0 } else { 16.0 }, &pal);
        if er.dragged() {
            resize = Some((c.slot, (c.w + er.drag_delta().x).clamp(50.0, 900.0)));
        }
        if er.double_clicked() {
            resize = Some((c.slot, if c.slot == 0 { 320.0 } else { 150.0 }));
        }
        x += c.w;
    }
    if let Some(c) = sort_click {
        app.exec(&ctx, Cmd::SortBy(c));
    }
    if let Some((slot, w)) = resize {
        app.settings.col_widths[slot] = w;
        app.mark_settings(&ctx);
    }
}

pub fn show(app: &mut OxideApp, ui: &mut Ui) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let full = ui.max_rect();
    let a = app.active;
    if let Some(d) = app.tab().fs_dir() {
        if !app.tab().is_virtual() && !app.tab().searching() {
            app.drop_targets.areas.push((full, d));
        }
    }
    let view = app.tab().view.view;
    let details = view == ViewMode::Details;
    let mut area = full;
    if details {
        let hr = Rect::from_min_size(full.min, Vec2::new(full.width(), m::HEADER_H));
        header(app, ui, hr);
        area.min.y += m::HEADER_H;
    }

    let t = app.tab();
    let t_now = now(&ctx);
    // Состояния: загрузка, ошибка, пусто
    if t.entries.is_empty() {
        if let Some(err) = t.error.clone() {
            let c = Pos2::new(area.center().x, area.top() + 80.0);
            ui.painter().text(c, Align2::CENTER_CENTER, tr("Расположение недоступно"), font_sb(14.0), pal.text);
            ui.painter().text(c + Vec2::new(0.0, 26.0), Align2::CENTER_CENTER, err, font(12.0), pal.text2);
            let br = Rect::from_center_size(c + Vec2::new(0.0, 70.0), Vec2::new(120.0, 32.0));
            let resp = ui.scope_builder(egui::UiBuilder::new().max_rect(br), |ui| button(ui, tr("Повторить"), true, &pal, 120.0)).inner;
            if resp.clicked() {
                app.exec(&ctx, Cmd::Refresh);
            }
            return;
        }
        if t.loading {
            if t_now - t.load_started > 0.12 {
                skeleton(ui, area, &pal, details);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        let msg = if t.searching() { tr("Нет элементов, удовлетворяющих условиям поиска") } else { tr("Эта папка пуста") };
        ui.painter().text(Pos2::new(area.center().x, area.top() + 28.0), Align2::CENTER_CENTER, msg, font(12.0), pal.text2);
        let bg = ui.interact(area, Id::new(("empty-bg", t.id)), Sense::click());
        if bg.secondary_clicked() {
            let pos = bg.interact_pointer_pos().unwrap_or(area.center());
            let p = app.background_menu(&ctx, pos);
            app.open_popup(p);
        }
        return;
    }

    let cols_w: f32 = columns(app).iter().map(|c| c.w).sum::<f32>() + 8.0;
    let n = app.tab().order.len();
    let compact = app.settings.compact;
    let reset = std::mem::take(&mut app.tabs[a].reset_scroll);
    let horizontal = view == ViewMode::List;
    let mut sa = if horizontal { egui::ScrollArea::horizontal() } else { egui::ScrollArea::vertical() };
    sa = sa.id_salt(("items", app.tabs[a].id, view as u8)).auto_shrink([false, false]);
    let key = ("items", app.tabs[a].id, view as u8);
    if reset {
        sa = if horizontal { sa.horizontal_scroll_offset(0.0) } else { sa.vertical_scroll_offset(0.0) };
        vscroll_forget(ui, key);
    }
    let pal = app.pal.clone();
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        ui.set_clip_rect(area);
        let show = |ui: &mut Ui, _vp: Rect| {
            // Без ширины полосы прокрутки, чтобы строки не заходили под неё
            let avail = Vec2::new(ui.available_width().min(area.width()), area.height());
            let lay = layout(view, n, avail, compact, cols_w);
            body(app, ui, lay, view);
        };
        // Список листается вбок - там полоса egui; остальные виды - своя вертикальная полоса
        if horizontal {
            sa.show_viewport(ui, show);
        } else {
            vscroll(ui, sa, key, &pal, show);
        }
    });
}

fn skeleton(ui: &Ui, area: Rect, pal: &Palette, details: bool) {
    let p = ui.painter();
    let c = pal.fill(pal.hover.gamma_multiply(1.6));
    for i in 0..10 {
        let y = area.top() + 8.0 + i as f32 * m::ROW_H;
        if details {
            p.rect_filled(Rect::from_min_size(Pos2::new(area.left() + 16.0, y + 6.0), Vec2::new(16.0, 16.0)), CornerRadius::same(3), c);
            let w = 120.0 + ((i * 37) % 90) as f32;
            p.rect_filled(Rect::from_min_size(Pos2::new(area.left() + 40.0, y + 9.0), Vec2::new(w, 10.0)), CornerRadius::same(3), c);
            p.rect_filled(Rect::from_min_size(Pos2::new(area.left() + 270.0, y + 9.0), Vec2::new(100.0, 10.0)), CornerRadius::same(3), c);
        } else {
            let x = area.left() + 16.0 + (i % 5) as f32 * 100.0;
            let y = area.top() + 12.0 + (i / 5) as f32 * 110.0;
            p.rect_filled(Rect::from_min_size(Pos2::new(x, y), Vec2::new(64.0, 64.0)), CornerRadius::same(4), c);
            p.rect_filled(Rect::from_min_size(Pos2::new(x, y + 72.0), Vec2::new(64.0, 10.0)), CornerRadius::same(3), c);
        }
    }
}

fn body(app: &mut OxideApp, ui: &mut Ui, lay: Lay, view: ViewMode) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let a = app.active;
    let origin = ui.max_rect().min;
    let clip = ui.clip_rect();
    let total = Rect::from_min_size(origin, lay.total);
    ui.allocate_rect(total, Sense::hover());
    // Фон ловит щелчки по всей видимой области, а не только под элементами
    let bg = ui.interact(total.union(clip), Id::new(("items-bg", app.tabs[app.active].id)), Sense::click_and_drag());
    let n = app.tabs[a].order.len();
    let local_clip = clip.translate(-origin.to_vec2());
    let range = lay.visible(local_clip, n);
    let mods = ctx.input(|i| i.modifiers);
    let thumbs = view.thumbs();
    let isz = view.icon_size();
    let cols = columns(app);
    let checkboxes = app.settings.checkboxes;
    let drag_hover = app.drag_hover.clone();
    let kb_focus = app.kb_focus;
    let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
    let secondary_pressed = ctx.input(|i| i.pointer.secondary_pressed());

    // Сообщаем размеры раскладки для навигации клавишами
    {
        let t = &mut app.tabs[a];
        t.grid_cols = match lay.flow {
            Flow::Grid => lay.cols,
            Flow::Columns => 0,
            Flow::Rows => 1,
        };
        t.list_rows = lay.rows;
        t.page_items = match lay.flow {
            Flow::Rows => ((clip.height() / lay.cell.y).floor() as usize).max(1),
            Flow::Grid => ((clip.height() / lay.cell.y).floor() as usize).max(1) * lay.cols,
            Flow::Columns => lay.rows * ((clip.width() / lay.cell.x).floor() as usize).max(1),
        };
    }

    if app.tabs[a].scroll_to_focus {
        app.tabs[a].scroll_to_focus = false;
        if let Some(f) = app.tabs[a].focus {
            let r = lay.rect(f).translate(origin.to_vec2());
            ui.scroll_to_rect(r, None);
        }
    }

    let mut open_idx: Option<(usize, bool)> = None;
    let mut menu_at: Option<Pos2> = None;
    // Some(true) - тянут правой кнопкой
    let mut start_drag: Option<bool> = None;
    let mut item_hovered = false;
    let rename_idx = app.tabs[a].rename.as_ref().map(|r| r.idx);

    for pos in range {
        let ei = app.tabs[a].order[pos];
        let r = lay.rect(pos).translate(origin.to_vec2());
        let hit = match view {
            ViewMode::Details => Rect::from_min_max(Pos2::new(r.left(), r.top() + 1.0), Pos2::new(r.left() + cols.iter().map(|c| c.w).sum::<f32>().min(r.width()), r.bottom() - 1.0)),
            _ => r.shrink(2.0),
        };
        let id = Id::new(("item", app.tabs[a].id, ei));
        let resp = ui.interact(hit, id, Sense::click_and_drag());
        let e = app.tabs[a].entries[ei].clone();
        if e.is_dir && e.shell.is_none() {
            app.drop_targets.items.push((hit, e.path.clone()));
        }
        let selected = app.tabs[a].sel[ei];
        let hovered = resp.hovered();
        if hovered {
            item_hovered = true;
        }
        // Фон
        let p = ui.painter();
        let hl = match view {
            ViewMode::Details | ViewMode::Content | ViewMode::Small | ViewMode::List => Rect::from_min_max(Pos2::new(hit.left(), r.top() + 2.0), Pos2::new(hit.right(), r.bottom() - 2.0)),
            _ => r.shrink(2.0),
        };
        let ht = ctx.animate_bool_with_time(id.with("hv"), hovered, m::T_HOVER);
        if selected {
            p.rect_filled(hl, CornerRadius::same(m::R_CTRL), if hovered { pal.fill(pal.selected_hover) } else { pal.fill(pal.selected) });
        } else if ht > 0.0 {
            p.rect_filled(hl, CornerRadius::same(m::R_CTRL), pal.fill(pal.hover.gamma_multiply(ht)));
        }
        if drag_hover.as_ref() == Some(&e.path) && e.is_dir {
            p.rect_stroke(hl, CornerRadius::same(m::R_CTRL), Stroke::new(1.5, pal.accent), StrokeKind::Inside);
        }
        if kb_focus && app.tabs[a].focus == Some(pos) {
            p.rect_stroke(hl, CornerRadius::same(m::R_CTRL), Stroke::new(1.0, pal.focus.gamma_multiply(0.7)), StrokeKind::Inside);
        }
        let dim = e.is_hidden() || app.cut_set.contains(&e.path);
        let mut check_rect: Option<Rect> = None;
        let name_rect = paint_item(app, ui, &e, r, hl, view, &cols, thumbs, isz, dim, checkboxes && (hovered || selected), selected, &mut check_rect, rename_idx == Some(ei));

        // Переименование на месте
        if rename_idx == Some(ei) {
            rename_editor(app, ui, name_rect, view);
        }

        // Выделение мышью (по нажатию, как в Проводнике)
        // В таблице «тело» элемента - значок и имя; с остальной части строки тянется рамка, как в Проводнике
        let ptr = ctx.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        let on_body = view != ViewMode::Details || ptr.x < r.left() + cols.first().map(|c| c.w).unwrap_or(260.0);
        if hovered && primary_pressed && !on_body {
            let t = &mut app.tabs[a];
            let base = if mods.ctrl { t.sel.clone() } else { vec![false; t.sel.len()] };
            t.marquee = Some(Marquee { start: ptr - origin.to_vec2(), cur: ptr - origin.to_vec2(), base });
        }
        if hovered && primary_pressed {
            let t = &mut app.tabs[a];
            t.press_item = None;
            let on_check = check_rect.map(|c| c.contains(ctx.input(|i| i.pointer.interact_pos().unwrap_or_default()))).unwrap_or(false);
            if mods.ctrl || on_check {
                t.sel[ei] = !t.sel[ei];
                t.focus = Some(pos);
                t.anchor = Some(pos);
            } else if mods.shift {
                let anc = t.anchor.unwrap_or(pos);
                let (lo, hi) = if anc <= pos { (anc, pos) } else { (pos, anc) };
                t.clear_sel();
                for k in lo..=hi {
                    let idx = t.order[k];
                    t.sel[idx] = true;
                }
                t.focus = Some(pos);
            } else if !selected {
                t.select_only(ei);
                t.rename_click = None;
            } else {
                // Нажали на имя единственного выделенного элемента - возможно, это щелчок для переименования
                let only = t.sel.iter().filter(|s| **s).count() == 1;
                let armed = only && name_rect.contains(ptr) && !t.is_virtual() && t.rename.is_none() && e.shell.is_none();
                t.rename_click = if armed { Some((ei, f64::INFINITY)) } else { None };
                t.press_item = Some(ei);
                t.focus = Some(pos);
            }
            app.kb_focus = false;
            if app.tabs[a].rename.as_ref().map(|r| r.idx) != Some(ei) && app.tabs[a].rename.is_some() {
                app.commit_rename(&ctx);
            }
        }
        if resp.clicked() && !mods.ctrl && !mods.shift && app.tabs[a].press_item == Some(ei) {
            app.tabs[a].select_only(ei);
            app.tabs[a].press_item = None;
            // Щелчок отпущен на месте: ждём, не будет ли второго
            if let Some((ri, at)) = app.tabs[a].rename_click.as_mut() {
                if *ri == ei && at.is_infinite() {
                    *at = ctx.input(|i| i.time);
                }
            }
        }
        if resp.double_clicked() && !mods.ctrl {
            app.tabs[a].rename_click = None;
            open_idx = Some((ei, false));
        }
        if resp.drag_started() {
            app.tabs[a].rename_click = None;
        }
        if resp.middle_clicked() && e.is_dir {
            open_idx = Some((ei, true));
        }
        if hovered && secondary_pressed {
            if !app.tabs[a].sel[ei] {
                app.tabs[a].select_only(ei);
            }
        }
        if resp.secondary_clicked() {
            menu_at = Some(resp.interact_pointer_pos().unwrap_or(r.center()));
        }
        let right_drag = resp.drag_started_by(egui::PointerButton::Secondary);
        if (resp.drag_started_by(egui::PointerButton::Primary) || right_drag) && !app.tabs[a].is_virtual() && app.tabs[a].marquee.is_none() {
            if !app.tabs[a].sel[ei] {
                app.tabs[a].select_only(ei);
            }
            start_drag = Some(right_drag);
        }
        if hovered && !e.is_dir {
            let tip_type = e.type_name.clone().unwrap_or_else(|| fs::type_name(&e.ext, e.is_dir));
            let mut tip = trf("Тип: {}\nРазмер: {}", &[&tip_type, &crate::util::format_bytes(e.size)]);
            if e.mtime != 0 {
                tip.push_str(&trf("\nДата изменения: {}", &[&crate::util::format_filetime(e.mtime)]));
            }
            resp.on_hover_text(tip);
        }
    }

    // Отложенное переименование по щелчку
    if let Some((ri, at)) = app.tabs[a].rename_click {
        let wait = crate::util::double_click_time();
        let now = ctx.input(|i| i.time);
        let still = app.tabs[a].sel.get(ri).copied().unwrap_or(false) && app.tabs[a].sel.iter().filter(|s| **s).count() == 1;
        // Отпустили не щелчком (увели мышь, потянули) - отмена
        if !still || (at.is_infinite() && !ctx.input(|i| i.pointer.primary_down())) {
            app.tabs[a].rename_click = None;
        } else if at.is_finite() {
            if now - at >= wait {
                app.tabs[a].rename_click = None;
                if open_idx.is_none() {
                    app.start_rename();
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((wait - (now - at)).max(0.01)));
            }
        }
    }

    // Пустое место: снять выделение, рамка выделения, меню фона
    if bg.hovered() && !item_hovered && primary_pressed {
        if app.tabs[a].rename.is_some() {
            app.commit_rename(&ctx);
        }
        let t = &mut app.tabs[a];
        if !mods.ctrl && !mods.shift {
            t.clear_sel();
        }
        if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
            let local = p - origin.to_vec2();
            t.marquee = Some(Marquee { start: local, cur: local, base: t.sel.clone() });
        }
        app.kb_focus = false;
    }
    if bg.hovered() && !item_hovered && secondary_pressed {
        app.tabs[a].clear_sel();
    }
    if let Some(mq) = app.tabs[a].marquee.as_mut() {
        if ctx.input(|i| i.pointer.primary_down()) {
            if let Some(p) = ctx.input(|i| i.pointer.latest_pos()) {
                mq.cur = p - origin.to_vec2();
                // Прокрутка у краёв
                let edge = 24.0;
                let mut d = Vec2::ZERO;
                if p.y < clip.top() + edge {
                    d.y = 12.0;
                } else if p.y > clip.bottom() - edge {
                    d.y = -12.0;
                }
                if lay.flow == Flow::Columns {
                    if p.x < clip.left() + edge {
                        d.x = 12.0;
                    } else if p.x > clip.right() - edge {
                        d.x = -12.0;
                    }
                }
                if d != Vec2::ZERO {
                    ui.scroll_with_delta(d);
                    ctx.request_repaint();
                }
            }
            let sel_rect = Rect::from_two_pos(mq.start, mq.cur);
            if sel_rect.width() > 2.0 || sel_rect.height() > 2.0 {
                let t = &mut app.tabs[a];
                let mq = t.marquee.as_ref().unwrap();
                let base = mq.base.clone();
                let n = t.order.len();
                for pos in 0..n {
                    let idx = t.order[pos];
                    let mut r = lay.rect(pos);
                    if view == ViewMode::Details {
                        r.max.x = r.min.x + cols.first().map(|c| c.w).unwrap_or(200.0);
                    }
                    let inside = r.shrink(2.0).intersects(sel_rect);
                    t.sel[idx] = if mods.ctrl { base[idx] ^ inside } else { base[idx] || inside };
                }
                let screen_rect = sel_rect.translate(origin.to_vec2());
                let p = ui.painter();
                p.rect_filled(screen_rect, CornerRadius::ZERO, pal.marquee_fill());
                p.rect_stroke(screen_rect, CornerRadius::ZERO, Stroke::new(1.0, pal.marquee), StrokeKind::Inside);
            }
        } else {
            app.tabs[a].marquee = None;
        }
    }
    if bg.secondary_clicked() && !item_hovered {
        let pos = bg.interact_pointer_pos().unwrap_or(total.center());
        let p = app.background_menu(&ctx, pos);
        app.open_popup(p);
    }
    if let Some(pos) = menu_at {
        let p = app.item_menu(&ctx, pos);
        app.open_popup(p);
    }
    if let Some(right) = start_drag {
        app.pending_drag = Some((app.tabs[a].selected_paths(), right));
    }
    if let Some((ei, new_tab)) = open_idx {
        app.open_entry(&ctx, ei, new_tab);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_item(
    app: &mut OxideApp,
    ui: &Ui,
    e: &Entry,
    r: Rect,
    hl: Rect,
    view: ViewMode,
    cols: &[Col],
    thumbs: bool,
    isz: f32,
    dim: bool,
    show_check: bool,
    selected: bool,
    check_rect: &mut Option<Rect>,
    renaming: bool,
) -> Rect {
    let pal = app.pal.clone();
    let p = ui.painter().clone();
    let name_col = if dim { pal.text2 } else { pal.text };
    let tname = || e.type_name.clone().unwrap_or_else(|| fs::type_name(&e.ext, e.is_dir));
    let mut paint_check = |pos: Pos2| {
        let b = Rect::from_center_size(pos, Vec2::splat(16.0));
        *check_rect = Some(b.expand(4.0));
        if selected {
            p.rect_filled(b, CornerRadius::same(3), pal.accent);
            glyph(&p, b.center(), ICON_CHECK, 10.0, pal.on_accent);
        } else {
            p.rect_filled(b, CornerRadius::same(3), pal.field);
            p.rect_stroke(b, CornerRadius::same(3), Stroke::new(1.0, pal.text2), StrokeKind::Inside);
        }
    };
    match view {
        ViewMode::Details => {
            let cy = r.center().y;
            let mut x = r.left();
            let mut name_rect = Rect::NOTHING;
            for c in cols {
                let cr = Rect::from_min_size(Pos2::new(x, r.top()), Vec2::new(c.w, r.height()));
                match c.id {
                    SortCol::Name => {
                        let mut ix = cr.left() + 20.0;
                        if show_check || app.settings.checkboxes {
                            if show_check {
                                paint_check(Pos2::new(cr.left() + 16.0, cy));
                            }
                            ix += 22.0;
                        }
                        paint_icon(app, ui, e, Pos2::new(ix, cy), 16.0, false, dim);
                        let tx = ix + 14.0;
                        name_rect = Rect::from_min_max(Pos2::new(tx - 4.0, r.top() + 2.0), Pos2::new(cr.right() - 4.0, r.bottom() - 2.0));
                        if !renaming {
                            text_trunc(&p, Pos2::new(tx, cy), &e.display, font(12.0), name_col, cr.right() - tx - 8.0);
                        }
                    }
                    SortCol::Modified => {
                        text_trunc(&p, Pos2::new(cr.left() + 10.0, cy), &crate::util::format_filetime(e.mtime), font(12.0), pal.text2, cr.width() - 14.0);
                    }
                    SortCol::Type => {
                        text_trunc(&p, Pos2::new(cr.left() + 10.0, cy), &tname(), font(12.0), pal.text2, cr.width() - 14.0);
                    }
                    SortCol::Size => {
                        if !e.is_dir {
                            text_right(&p, Pos2::new(cr.right() - 12.0, cy), &crate::util::format_kb(e.size), font(12.0), pal.text2);
                        }
                    }
                    SortCol::Location => {
                        let s = e.location.clone().unwrap_or_default();
                        text_trunc(&p, Pos2::new(cr.left() + 10.0, cy), &s, font(12.0), pal.text2, cr.width() - 14.0);
                    }
                    SortCol::Extra => {
                        text_trunc(&p, Pos2::new(cr.left() + 10.0, cy), &crate::util::format_filetime(e.ctime), font(12.0), pal.text2, cr.width() - 14.0);
                    }
                }
                x += c.w;
            }
            name_rect
        }
        ViewMode::Small | ViewMode::List => {
            let cy = r.center().y;
            let mut ix = hl.left() + 14.0;
            if show_check {
                paint_check(Pos2::new(ix, cy));
                ix += 22.0;
            }
            paint_icon(app, ui, e, Pos2::new(ix, cy), 16.0, false, dim);
            let tx = ix + 14.0;
            if !renaming {
                text_trunc(&p, Pos2::new(tx, cy), &e.display, font(12.0), name_col, hl.right() - tx - 6.0);
            }
            Rect::from_min_max(Pos2::new(tx - 4.0, r.top() + 2.0), Pos2::new(hl.right() - 2.0, r.bottom() - 2.0))
        }
        ViewMode::Content => {
            let cy = r.center().y;
            paint_icon(app, ui, e, Pos2::new(hl.left() + 32.0, cy), 32.0, thumbs, dim);
            if show_check {
                paint_check(Pos2::new(hl.left() + 12.0, hl.top() + 12.0));
            }
            let tx = hl.left() + 60.0;
            if !renaming {
                text_trunc(&p, Pos2::new(tx, cy - 9.0), &e.display, font(14.0), name_col, hl.width() * 0.5);
            }
            let sub = if let Some(l) = &e.location { l.clone() } else { tname() };
            text_trunc(&p, Pos2::new(tx, cy + 11.0), &sub, font(12.0), pal.text2, hl.width() * 0.5);
            let rx = hl.right() - 16.0;
            if e.mtime != 0 {
                text_right(&p, Pos2::new(rx, cy - 9.0), &trf("Дата изменения: {}", &[&crate::util::format_filetime(e.mtime)]), font(12.0), pal.text2);
            }
            if !e.is_dir {
                text_right(&p, Pos2::new(rx, cy + 11.0), &trf("Размер: {}", &[&crate::util::format_bytes(e.size)]), font(12.0), pal.text2);
            }
            Rect::from_min_max(Pos2::new(tx - 4.0, cy - 21.0), Pos2::new(tx + hl.width() * 0.5, cy + 3.0))
        }
        ViewMode::Tiles => {
            let cy = r.center().y;
            paint_icon(app, ui, e, Pos2::new(hl.left() + 36.0, cy), 48.0, thumbs, dim);
            if show_check {
                paint_check(Pos2::new(hl.left() + 12.0, hl.top() + 12.0));
            }
            let tx = hl.left() + 70.0;
            let w = hl.right() - tx - 8.0;
            if !renaming {
                text_trunc(&p, Pos2::new(tx, cy - 16.0), &e.display, font(12.0), name_col, w);
            }
            text_trunc(&p, Pos2::new(tx, cy), &tname(), font(12.0), pal.text2, w);
            if !e.is_dir {
                text_trunc(&p, Pos2::new(tx, cy + 16.0), &crate::util::format_bytes(e.size), font(12.0), pal.text2, w);
            }
            Rect::from_min_max(Pos2::new(tx - 4.0, cy - 28.0), Pos2::new(hl.right() - 4.0, cy - 4.0))
        }
        ViewMode::Medium | ViewMode::Large | ViewMode::ExtraLarge => {
            let icon_c = Pos2::new(r.center().x, r.top() + 6.0 + isz / 2.0);
            paint_icon(app, ui, e, icon_c, isz, thumbs, dim);
            if show_check {
                paint_check(Pos2::new(hl.left() + 12.0, hl.top() + 12.0));
            }
            let ty = r.top() + 6.0 + isz + 6.0;
            let w = r.width() - 12.0;
            if !renaming {
                let mut job = LayoutJob::single_section(e.display.clone(), TextFormat::simple(font(12.0), name_col));
                job.wrap = TextWrapping { max_width: w, max_rows: if selected { 4 } else { 2 }, break_anywhere: false, overflow_character: Some('…') };
                job.halign = egui::Align::Center;
                let g = ui.fonts(|f| f.layout_job(job));
                p.galley(Pos2::new(r.center().x, ty).round(), g, name_col);
            }
            Rect::from_min_max(Pos2::new(r.left() + 4.0, ty - 2.0), Pos2::new(r.right() - 4.0, ty + 22.0))
        }
    }
}

fn rename_editor(app: &mut OxideApp, ui: &mut Ui, rect: Rect, _view: ViewMode) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let a = app.active;
    let rn_is_dir = app.tabs[a].rename.as_ref().and_then(|r| app.tabs[a].entries.get(r.idx)).map(|e| e.is_dir).unwrap_or(false);
    let Some(rn) = app.tabs[a].rename.as_mut() else { return };
    let id = Id::new(("rename", rn.idx));
    let mut text = rn.text.clone();
    // Как в Проводнике: рамка по ширине имени и растёт при наборе, текст на месте подписи,
    // выделение системным синим
    let tw = text_width(ui, &text, font(12.0));
    let w = (tw + 12.0).clamp(48.0, rect.width().max(48.0));
    // Рамка на 1 правее начала подписи, текст внутри ровно на месте подписи
    let rect = Rect::from_min_size(Pos2::new(rect.left() + 1.0, rect.center().y - 10.0), Vec2::new(w, 20.0)).round_ui();
    let bg = if pal.dark { egui::Color32::BLACK } else { egui::Color32::WHITE };
    ui.painter().rect_filled(rect, CornerRadius::ZERO, bg);
    ui.painter().rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, pal.text3), StrokeKind::Inside);
    let inner = rect.shrink2(Vec2::new(3.0, 2.0));
    let out = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            // Высота поля - ровно строка: иначе выделение вылезает за рамку
            ui.spacing_mut().interact_size.y = inner.height();
            ui.set_clip_rect(inner.expand(1.0));
            ui.visuals_mut().selection.bg_fill = pal.marquee;
            ui.visuals_mut().selection.stroke = Stroke::new(1.0, egui::Color32::WHITE);
            TextEdit::singleline(&mut text).id(id).frame(false).margin(egui::Margin::ZERO).font(font(12.0)).text_color(pal.text).desired_width(inner.width()).vertical_align(egui::Align::Center).show(ui)
        })
        .inner;
    if rn.init {
        rn.init = false;
        out.response.request_focus();
        let is_dir = rn_is_dir;
        let chars = text.chars().count();
        let stem = if is_dir {
            chars
        } else {
            match text.rfind('.') {
                Some(i) if i > 0 => text[..i].chars().count(),
                _ => chars,
            }
        };
        let mut st = out.state.clone();
        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(stem))));
        st.store(&ctx, id);
    }
    if let Some(err) = &rn.error {
        let tip = Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() + 4.0), Vec2::new(320.0, 40.0));
        egui::Area::new(Id::new("rename-err")).order(egui::Order::Tooltip).fixed_pos(tip.min).show(&ctx, |ui| {
            let p = ui.painter();
            p.rect_filled(tip, CornerRadius::same(m::R_CTRL), pal.flyout);
            p.rect_stroke(tip, CornerRadius::same(m::R_CTRL), Stroke::new(1.0, pal.danger), StrokeKind::Inside);
            let mut job = LayoutJob::single_section(err.clone(), TextFormat::simple(font(12.0), pal.text));
            job.wrap.max_width = 300.0;
            let g = ui.fonts(|f| f.layout_job(job));
            p.galley(tip.min + Vec2::new(10.0, 6.0), g, pal.text);
        });
    }
    if out.response.changed() {
        rn.error = None;
    }
    rn.text = text;
    let enter = ctx.input(|i| i.key_pressed(Key::Enter));
    let esc = ctx.input(|i| i.key_pressed(Key::Escape));
    if esc {
        app.tabs[a].rename = None;
    } else if (out.response.lost_focus() && enter) || (out.response.lost_focus() && !esc) {
        app.commit_rename(&ctx);
    }
}
