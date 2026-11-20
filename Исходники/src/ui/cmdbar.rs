//! Командная панель, как в Проводнике Windows 11

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use egui::{Id, Pos2, Rect, Ui, Vec2};

use super::menu::Popup;
use super::widgets::*;
use crate::app::{now, Location, OxideApp};
use crate::cmd::{Cmd, PaneMode};
use crate::shell::{clipboard, shellfolder};
use crate::theme::m;

pub fn show(app: &mut OxideApp, ui: &mut Ui) {
    let rect = ui.max_rect();
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    // Линии сверху и снизу командной панели
    hline(ui.painter(), rect.top() - 0.5, rect.left(), rect.right(), pal.divider);
    hline(ui.painter(), rect.bottom() - 1.0, rect.left(), rect.right(), pal.divider);
    let cy = rect.center().y;
    let t = app.tab();
    let listing = t.loc.is_listing();
    let virt = t.is_virtual();
    let writable = listing && !virt && t.fs_dir().is_some() && !t.searching();
    let has_sel = t.sel_count() > 0;
    let single = t.sel_count() == 1;
    let is_bin = matches!(&t.loc, Location::Shell(s) if s.eq_ignore_ascii_case(shellfolder::RECYCLE_BIN));
    let bin_some = !t.entries.is_empty();
    let mut cmd: Option<Cmd> = None;
    let mut x = rect.left() + 12.0;

    // «Создать»
    let (r, br) = cmd_button(ui, x, cy, Id::new("cb-new"), Some((ICON_NEW, pal.text)), tr("Создать"), true, &pal, writable, false);
    if r.clicked() {
        let items = app.new_menu_items();
        let id = app.new_popup_id();
        app.open_popup(Popup::below(id, br, items, now(&ctx)));
    }
    x = br.right() + 12.0;
    vsep(ui.painter(), x, cy, 24.0, &pal);
    x += 12.0;

    let paste_ok = writable && clipboard::has_files();
    let icons: [(char, Cmd, &str, bool); 6] = [
        (ICON_CUT, Cmd::Cut, tr("Вырезать (Ctrl+X)"), has_sel && listing),
        (ICON_COPY, Cmd::Copy, tr("Копировать (Ctrl+C)"), has_sel && listing && !is_bin),
        (ICON_PASTE, Cmd::Paste, tr("Вставить (Ctrl+V)"), paste_ok),
        (ICON_RENAME, Cmd::Rename, tr("Переименовать (F2)"), single && !virt),
        (ICON_SHARE, Cmd::Share, tr("Поделиться"), has_sel && !virt),
        (ICON_DELETE, Cmd::Delete, tr("Удалить (Delete)"), has_sel && listing),
    ];
    for (i, (ic, c, tip, en)) in icons.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(x, cy - m::BTN_H / 2.0), Vec2::new(m::NAV_BTN_W, m::BTN_H));
        let resp = icon_button(ui, r, Id::new(("cb-icon", i)), *ic, 16.0, &pal, *en, tip);
        if resp.clicked() {
            cmd = Some(c.clone());
        }
        x += m::NAV_BTN_W + 8.0;
    }
    x += 4.0;
    vsep(ui.painter(), x, cy, 24.0, &pal);
    x += 12.0;

    let (r, br) = cmd_button(ui, x, cy, Id::new("cb-sort"), Some((ICON_SORT, pal.accent)), tr("Сортировать"), true, &pal, listing, false);
    if r.clicked() {
        let items = app.sort_menu_items();
        let id = app.new_popup_id();
        app.open_popup(Popup::below(id, br, items, now(&ctx)));
    }
    x = br.right() + 8.0;
    let (r, br) = cmd_button(ui, x, cy, Id::new("cb-view"), Some((ICON_VIEW, pal.text)), tr("Просмотреть"), true, &pal, true, false);
    if r.clicked() {
        let items = app.view_menu_items();
        let id = app.new_popup_id();
        app.open_popup(Popup::below(id, br, items, now(&ctx)));
    }
    x = br.right() + 12.0;
    if is_bin {
        vsep(ui.painter(), x, cy, 24.0, &pal);
        x += 12.0;
        let some = bin_some;
        let (r, br) = cmd_button(ui, x, cy, Id::new("cb-emptybin"), Some((ICON_DELETE, pal.text)), tr("Очистить корзину"), false, &pal, some, false);
        if r.clicked() {
            cmd = Some(Cmd::EmptyRecycleBin);
        }
        x = br.right() + 8.0;
        let (label, c) = if has_sel { (tr("Восстановить выделенные объекты"), Cmd::RestoreSelected) } else { (tr("Восстановить все объекты"), Cmd::RestoreAll) };
        let (r, br) = cmd_button(ui, x, cy, Id::new("cb-restorebin"), Some((ICON_RESTORE_BIN, pal.text)), label, false, &pal, some, false);
        if r.clicked() {
            cmd = Some(c);
        }
        x = br.right() + 12.0;
    }
    vsep(ui.painter(), x, cy, 24.0, &pal);
    x += 12.0;
    let r = Rect::from_min_size(Pos2::new(x, cy - m::BTN_H / 2.0), Vec2::new(m::NAV_BTN_W, m::BTN_H));
    let resp = icon_button(ui, r, Id::new("cb-more"), ICON_MORE, 16.0, &pal, true, tr("Подробнее"));
    if resp.clicked() {
        let items = app.more_menu_items();
        let id = app.new_popup_id();
        app.open_popup(Popup::below(id, r, items, now(&ctx)));
    }

    // «Сведения» справа
    let label = tr("Сведения");
    let w = 12.0 + 16.0 + 8.0 + text_width(ui, label, font(14.0)) + 12.0;
    let sx = rect.right() - 16.0 - w;
    if sx > x + m::NAV_BTN_W + 8.0 {
        let on = app.settings.pane == PaneMode::Details;
        let (r, _) = cmd_button(ui, sx, cy, Id::new("cb-details"), Some((ICON_DETAILS_PANE, pal.text)), label, false, &pal, true, on);
        if r.clicked() {
            cmd = Some(Cmd::Pane(PaneMode::Details));
        }
        let _ = r.on_hover_text(tr("Область сведений (Alt+Shift+P)"));
    }
    if let Some(c) = cmd {
        app.exec(&ctx, c);
    }
}
