//! Диалоги: «Параметры» (с выбором интерфейса), сообщения, «О программе».

use egui::{Align2, CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::widgets::*;
use crate::app::{Dialog, OxideApp};
use crate::settings::OpenTo;
use crate::theme::{m, Palette, ThemeChoice};

pub const DISCLAIMER: &str = "Oxide — независимый проект с открытым исходным кодом. Он не связан с Microsoft, не одобрен и не поддерживается ею. Windows и Windows 11 — товарные знаки Microsoft Corporation. Сторонние библиотеки и их лицензии перечислены в файле THIRD_PARTY_LICENSES.md.";

pub fn show(app: &mut OxideApp, ctx: &egui::Context) {
    let Some(d) = app.dialog.take() else { return };
    let pal = app.pal.clone();
    crate::theme::set_surface(pal.flyout);
    let frame = egui::Frame::new()
        .fill(pal.flyout)
        .stroke(Stroke::new(1.0, pal.flyout_stroke))
        .corner_radius(CornerRadius::same(m::R_OVERLAY))
        .inner_margin(egui::Margin::same(24))
        .shadow(egui::Shadow { offset: [0, 16], blur: 32, spread: 0, color: pal.shadow });
    let mut keep = true;
    let modal = egui::Modal::new(Id::new("dialog")).frame(frame).backdrop_color(egui::Color32::from_black_alpha(if pal.dark { 110 } else { 70 }));
    let resp = modal.show(ctx, |ui| match &d {
        Dialog::Settings => settings(app, ui, &mut keep),
        Dialog::Message { title, text } => {
            ui.set_width(420.0);
            ui.label(egui::RichText::new(title).font(font_sb(20.0)).color(pal.text));
            ui.add_space(12.0);
            ui.add(egui::Label::new(egui::RichText::new(text).font(font(14.0)).color(pal.text)).wrap());
            ui.add_space(24.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if button(ui, "ОК", true, &pal, 120.0).clicked() {
                    keep = false;
                }
            });
        }
        Dialog::About => {
            ui.set_width(420.0);
            ui.label(egui::RichText::new("Oxide").font(font_sb(20.0)).color(pal.text));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("Версия {}", env!("CARGO_PKG_VERSION"))).font(font(14.0)).color(pal.text2));
            ui.add_space(12.0);
            ui.add(egui::Label::new(egui::RichText::new("Быстрый проводник для Windows на Rust. Интерфейс повторяет Проводник Windows 11, а файловые операции выполняет сам движок Windows.").font(font(14.0)).color(pal.text)).wrap());
            ui.add_space(12.0);
            ui.add(egui::Label::new(egui::RichText::new(DISCLAIMER).font(font(12.0)).color(pal.text2)).wrap());
            ui.add_space(24.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if button(ui, "Закрыть", true, &pal, 120.0).clicked() {
                    keep = false;
                }
            });
        }
    });
    if resp.should_close() {
        keep = false;
    }
    if keep {
        app.dialog = Some(d);
    }
}

fn group_title(ui: &mut Ui, pal: &Palette, t: &str) {
    ui.add_space(16.0);
    ui.label(egui::RichText::new(t).font(font_sb(14.0)).color(pal.text));
    ui.add_space(6.0);
}

fn settings(app: &mut OxideApp, ui: &mut Ui, keep: &mut bool) {
    let ctx = ui.ctx().clone();
    let pal = app.pal.clone();
    ui.set_width(680.0);
    ui.label(egui::RichText::new("Параметры").font(font_sb(20.0)).color(pal.text));
    egui::ScrollArea::vertical().max_height((ctx.screen_rect().height() - 200.0).max(240.0)).auto_shrink([false, true]).show(ui, |ui| {
        ui.set_width(660.0);
        group_title(ui, &pal, "Интерфейс");
        let card = Vec2::new(152.0, 128.0);
        let cols = 4;
        let n = ThemeChoice::ALL.len();
        let rows = n.div_ceil(cols);
        let (area, _) = ui.allocate_exact_size(Vec2::new(660.0, rows as f32 * (card.y + 12.0)), Sense::hover());
        let mut pick = None;
        for (i, t) in ThemeChoice::ALL.iter().enumerate() {
            let r = Rect::from_min_size(area.min + Vec2::new((i % cols) as f32 * (card.x + 12.0), (i / cols) as f32 * (card.y + 12.0)), card);
            let resp = ui.interact(r, Id::new(("theme", i)), Sense::click());
            let tp = Palette::resolve(*t, app.system_dark, app.accent);
            let prev = Rect::from_min_size(r.min, Vec2::new(card.x, 88.0));
            mini_window(ui, prev, &tp);
            let on = app.settings.theme == *t;
            let stroke = if on { Stroke::new(2.0, pal.accent) } else if resp.hovered() { Stroke::new(1.0, pal.text3) } else { Stroke::new(1.0, pal.flyout_stroke) };
            ui.painter().rect_stroke(prev, CornerRadius::same(m::R_OVERLAY), stroke, StrokeKind::Outside);
            ui.painter().text(Pos2::new(r.left() + 2.0, r.top() + 106.0), Align2::LEFT_CENTER, t.title(), font(12.0), if on { pal.text } else { pal.text2 });
            if on {
                let c = Pos2::new(prev.right() - 14.0, prev.top() + 14.0);
                ui.painter().circle_filled(c, 9.0, pal.accent);
                glyph(ui.painter(), c, ICON_CHECK, 10.0, pal.on_accent);
            }
            if resp.clicked() {
                pick = Some(*t);
            }
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, t.title()));
        }
        if let Some(t) = pick {
            app.set_theme(&ctx, t);
        }

        group_title(ui, &pal, "Открывать проводник для");
        ui.horizontal(|ui| {
            if radio(ui, app.settings.open_to == OpenTo::Home, "Главная", &pal).clicked() {
                app.settings.open_to = OpenTo::Home;
                app.mark_settings(&ctx);
            }
            ui.add_space(16.0);
            if radio(ui, app.settings.open_to == OpenTo::ThisPc, "Этот компьютер", &pal).clicked() {
                app.settings.open_to = OpenTo::ThisPc;
                app.mark_settings(&ctx);
            }
        });

        group_title(ui, &pal, "Вид");
        let s = &mut app.settings;
        let mut changed = false;
        let mut reload = false;
        changed |= toggle(ui, &mut s.compact, "Компактный вид", &pal).changed();
        let r = toggle(ui, &mut s.show_hidden, "Показывать скрытые файлы, папки и диски", &pal).changed();
        reload |= r;
        let r = toggle(ui, &mut s.show_ext, "Показывать расширения имён файлов", &pal).changed();
        reload |= r;
        let r = toggle(ui, &mut s.show_super_hidden, "Показывать защищённые системные файлы", &pal).changed();
        reload |= r;
        changed |= toggle(ui, &mut s.checkboxes, "Использовать флажки для выбора элементов", &pal).changed();
        changed |= toggle(ui, &mut s.restore_tabs, "Восстанавливать вкладки при следующем запуске", &pal).changed();

        group_title(ui, &pal, "Конфиденциальность");
        let r = toggle(ui, &mut s.show_recent, "Показывать недавно использовавшиеся файлы на главной", &pal).changed();
        changed |= r;
        let quick_reload = r;

        group_title(ui, &pal, "Поиск");
        let mut idx = s.index_enabled;
        let idx_changed = toggle(ui, &mut idx, "Индексировать диски для мгновенного поиска", &pal).changed();
        s.index_enabled = idx;
        changed |= idx_changed;
        let status = {
            let building = app.engine.building.load(std::sync::atomic::Ordering::Relaxed);
            let prog = app.engine.progress.load(std::sync::atomic::Ordering::Relaxed);
            let info = app.engine.index.read().ok().and_then(|g| g.as_ref().map(|ix| (ix.len(), ix.disk_bytes(), ix.built)));
            let mut t = match info {
                Some((n, bytes, built)) => format!(
                    "В индексе {} объектов (на диске {}, обновлён {}). Индекс читается с диска по мере надобности и почти не занимает оперативную память.",
                    n,
                    crate::util::format_bytes(bytes as u64),
                    crate::util::format_filetime(built)
                ),
                None => "Индекс ещё не построен: поиск идёт по папкам напрямую.".into(),
            };
            if building {
                t.push_str(&format!(" Обновление… обработано объектов: {prog}."));
            }
            t
        };
        ui.add(egui::Label::new(egui::RichText::new(status).font(font(12.0)).color(pal.text2)).wrap());
        if idx_changed && app.settings.index_enabled {
            app.engine.start(ctx.clone());
        }
        ui.add_space(4.0);
        if !app.engine.building.load(std::sync::atomic::Ordering::Relaxed) && app.settings.index_enabled && button(ui, "Перестроить индекс", false, &pal, 160.0).clicked() {
            app.engine.build(ctx.clone());
        }
        if changed || reload {
            app.mark_settings(&ctx);
        }
        if reload {
            app.fs_dirty_all = true;
            app.nav.children.clear();
        }
        if quick_reload {
            app.load_quick(&ctx);
        }
        ui.add_space(8.0);
    });
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Oxide {}", env!("CARGO_PKG_VERSION"))).font(font(12.0)).color(pal.text3));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button(ui, "Закрыть", true, &pal, 120.0).clicked() {
                *keep = false;
            }
        });
    });
}

/// Миниатюра окна в выбранной теме.
fn mini_window(ui: &Ui, r: Rect, p: &Palette) {
    let pt = ui.painter();
    pt.rect_filled(r, CornerRadius::same(m::R_OVERLAY), p.base);
    let tab = Rect::from_min_size(r.min + Vec2::new(8.0, 6.0), Vec2::new(48.0, 10.0));
    pt.rect_filled(tab, CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, p.layer);
    let addr = Rect::from_min_size(Pos2::new(r.left(), r.top() + 16.0), Vec2::new(r.width(), 14.0));
    pt.rect_filled(addr, CornerRadius::ZERO, p.layer);
    pt.rect_filled(Rect::from_min_size(addr.min + Vec2::new(34.0, 3.0), Vec2::new(r.width() - 70.0, 8.0)), CornerRadius::same(2), p.field);
    let cmd = Rect::from_min_size(Pos2::new(r.left(), addr.bottom()), Vec2::new(r.width(), 12.0));
    pt.rect_filled(cmd, CornerRadius::ZERO, p.cmdbar);
    for i in 0..4 {
        pt.rect_filled(Rect::from_min_size(Pos2::new(r.left() + 8.0, cmd.bottom() + 6.0 + i as f32 * 9.0), Vec2::new(30.0, 4.0)), CornerRadius::same(1), p.text3.gamma_multiply(0.7));
    }
    let sel = Rect::from_min_size(Pos2::new(r.left() + 46.0, cmd.bottom() + 13.0), Vec2::new(r.width() - 54.0, 9.0));
    pt.rect_filled(sel, CornerRadius::same(2), p.selected);
    for i in 0..4 {
        let y = cmd.bottom() + 6.0 + i as f32 * 9.0;
        pt.rect_filled(Rect::from_min_size(Pos2::new(r.left() + 50.0, y), Vec2::new(4.0, 4.0)), CornerRadius::same(1), egui::Color32::from_rgb(0xFF, 0xC8, 0x3D));
        pt.rect_filled(Rect::from_min_size(Pos2::new(r.left() + 58.0, y), Vec2::new(40.0, 4.0)), CornerRadius::same(1), p.text.gamma_multiply(0.75));
    }
    pt.circle_filled(Pos2::new(r.right() - 12.0, r.bottom() - 10.0), 4.0, p.accent);
}
