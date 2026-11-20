//! Сборка окна: полоса вкладок, строка адреса, командная панель, навигация, содержимое, строка состояния

pub mod addressbar;
pub mod cmdbar;
pub mod content;
pub mod dialogs;
pub mod home;
pub mod keys;
pub mod menu;
pub mod nav;
pub mod pane;
pub mod props;
pub mod status;
pub mod titlebar;
pub mod widgets;

use egui::{Align2, CornerRadius, Frame, Pos2, Vec2};

use crate::app::{Location, OxideApp};
use crate::cmd::PaneMode;
use crate::theme::m;
use widgets::*;

pub fn frame(app: &mut OxideApp, ctx: &egui::Context) {
    let pal = app.pal.clone();
    egui::TopBottomPanel::top("titlebar")
        .exact_height(m::TITLE_H)
        .frame(Frame::NONE.fill(pal.base))
        .show_separator_line(false)
        .show(ctx, |ui| { crate::theme::set_surface(pal.base); titlebar::show(app, ui) });
    egui::TopBottomPanel::top("addressbar")
        .exact_height(m::ADDR_H)
        .frame(Frame::NONE.fill(pal.layer))
        .show_separator_line(false)
        .show(ctx, |ui| { crate::theme::set_surface(pal.layer); addressbar::show(app, ui) });
    egui::TopBottomPanel::top("cmdbar")
        .exact_height(m::CMD_H)
        .frame(Frame::NONE.fill(pal.cmdbar))
        .show_separator_line(false)
        .show(ctx, |ui| { crate::theme::set_surface(pal.cmdbar); cmdbar::show(app, ui) });
    egui::TopBottomPanel::bottom("status")
        .exact_height(m::STATUS_H)
        .frame(Frame::NONE.fill(pal.base))
        .show_separator_line(false)
        .show(ctx, |ui| { crate::theme::set_surface(pal.base); status::show(app, ui) });

    let nav = egui::SidePanel::left("nav")
        .resizable(true)
        .default_width(app.settings.nav_width)
        .width_range(120.0..=480.0)
        .frame(Frame::NONE.fill(pal.base))
        .show_separator_line(false)
        .show(ctx, |ui| { crate::theme::set_surface(pal.base); nav::show(app, ui) });
    let w = nav.response.rect.width();
    if (w - app.settings.nav_width).abs() > 0.5 {
        app.settings.nav_width = w;
        app.mark_settings(ctx);
    }

    if app.settings.pane != PaneMode::None && app.tab().loc.is_listing() {
        let pane = egui::SidePanel::right("pane")
            .resizable(true)
            .default_width(app.settings.pane_width)
            .width_range(200.0..=600.0)
            .frame(Frame::NONE.fill(pal.base))
            .show_separator_line(false)
            .show(ctx, |ui| { crate::theme::set_surface(pal.base); pane::show(app, ui) });
        let w = pane.response.rect.width();
        if (w - app.settings.pane_width).abs() > 0.5 {
            app.settings.pane_width = w;
            app.mark_settings(ctx);
        }
    }

    egui::CentralPanel::default().frame(Frame::NONE.fill(pal.base)).show(ctx, |ui| {
        crate::theme::set_surface(pal.base);
        app.content_rect = ui.max_rect();
        match app.tab().loc.clone() {
            Location::Home if !app.tab().searching() => home::home(app, ui),
            Location::ThisPc if !app.tab().searching() => home::this_pc(app, ui),
            _ => content::show(app, ui),
        }
    });
}

/// Короткое уведомление внизу окна
pub fn toast(app: &mut OxideApp, ctx: &egui::Context) {
    let Some((text, t0)) = app.toast.clone() else { return };
    let now = ctx.input(|i| i.time);
    let age = (now - t0) as f32;
    if age > 4.0 {
        app.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    let alpha = if age < 0.15 { age / 0.15 } else if age > 3.6 { (4.0 - age) / 0.4 } else { 1.0 };
    let pal = &app.pal;
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("toast")).order(egui::Order::Tooltip).fixed_pos(Pos2::ZERO).interactable(false).show(ctx, |ui| {
        let g = ui.painter().layout_no_wrap(text.clone(), font(14.0), pal.text);
        let size = g.size() + Vec2::new(32.0, 20.0);
        let rect = egui::Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - m::STATUS_H - 24.0 - size.y / 2.0), size);
        let p = ui.painter();
        let shadow = egui::Shadow { offset: [0, 8], blur: 16, spread: 0, color: pal.shadow.gamma_multiply(alpha) };
        p.add(shadow.as_shape(rect, CornerRadius::same(m::R_OVERLAY)));
        p.rect_filled(rect, CornerRadius::same(m::R_OVERLAY), pal.flyout.gamma_multiply(alpha));
        p.rect_stroke(rect, CornerRadius::same(m::R_OVERLAY), egui::Stroke::new(1.0, pal.flyout_stroke.gamma_multiply(alpha)), egui::StrokeKind::Inside);
        p.text(rect.center(), Align2::CENTER_CENTER, text, font(14.0), pal.text.gamma_multiply(alpha));
    });
}
