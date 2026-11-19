//! Строка состояния: «Элементов: N | Выбрано …» и переключатели вида справа.

use egui::{Align2, Id, Pos2, Rect, Ui, Vec2};

use super::widgets::*;
use crate::app::{Location, OxideApp};
use crate::cmd::{Cmd, ViewMode};
use crate::util::plural;

pub fn show(app: &mut OxideApp, ui: &mut Ui) {
    let rect = ui.max_rect();
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let cy = rect.center().y;
    let t = app.tab();
    let mut text = String::new();
    match &t.loc {
        Location::Home if !t.searching() => {
            text = format!("Элементов: {}", app.quick.len() + if app.settings.show_recent { app.recent.len() } else { 0 });
        }
        Location::ThisPc if !t.searching() => {
            text = format!("Элементов: {}", app.drives.len());
        }
        _ => {
            if t.loading && t.entries.is_empty() {
                text = if t.searching() { "Поиск…".into() } else { "Загрузка…".into() };
            } else {
                text = format!("Элементов: {}", t.entries.len());
                if t.search.as_ref().map(|s| s.running).unwrap_or(false) {
                    text.push_str("   |   Поиск продолжается…");
                }
                let sel = t.selected_entries();
                if !sel.is_empty() {
                    let n = sel.len();
                    text.push_str(&format!("   |   {} {} {}", plural(n, "Выбран", "Выбрано", "Выбрано"), n, plural(n, "элемент", "элемента", "элементов")));
                    let files: Vec<_> = sel.iter().filter(|e| !e.is_dir).collect();
                    if !files.is_empty() && files.len() == n {
                        let total: u64 = files.iter().map(|e| e.size).sum();
                        text.push_str(&format!("  {}", crate::util::format_bytes(total)));
                    }
                    text.push_str("   |");
                }
            }
        }
    }
    if app.engine.building.load(std::sync::atomic::Ordering::Relaxed) && !app.engine.ready() {
        let n = app.engine.progress.load(std::sync::atomic::Ordering::Relaxed);
        if n > 0 {
            text.push_str(&format!("      Индексация для быстрого поиска: {n}"));
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
    ui.painter().text(Pos2::new(rect.left() + 16.0, cy), Align2::LEFT_CENTER, text, font(12.0), pal.text);

    // Таблица / Крупные значки — как две кнопки в правом нижнем углу Проводника.
    let view = app.tab().view.view;
    let r2 = Rect::from_center_size(Pos2::new(rect.right() - 20.0, cy), Vec2::new(24.0, 22.0));
    let r1 = r2.translate(Vec2::new(-28.0, 0.0));
    let listing = app.tab().loc.is_listing() || app.tab().searching();
    for (r, mode, icon, tip) in [
        (r1, ViewMode::Details, ICON_V_DETAILS, "Отображает сведения о каждом элементе в окне (Ctrl+Shift+6)"),
        (r2, ViewMode::Large, ICON_V_L, "Отображение элементов в виде крупных эскизов (Ctrl+Shift+2)"),
    ] {
        let on = view == mode && listing;
        if on {
            ui.painter().rect_filled(r, egui::CornerRadius::same(4), pal.fill(pal.selected));
        }
        let resp = icon_button(ui, r, Id::new(("status-view", mode as u8)), icon, 14.0, &pal, listing, tip);
        if resp.clicked() {
            app.exec(&ctx, Cmd::View(mode));
        }
    }
}
