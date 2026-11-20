//! Горячие клавиши - те же, что в Проводнике Windows 11

use std::sync::atomic::Ordering;

use egui::{Event, Key, Modifiers, PointerButton};

use crate::app::{now, OxideApp};
use crate::cmd::{Cmd, PaneMode, ViewMode};
use crate::shell::window;

fn pressed(ctx: &egui::Context, mods: Modifiers, key: Key) -> bool {
    ctx.input_mut(|i| i.consume_key(mods, key))
}

pub fn handle(app: &mut OxideApp, ctx: &egui::Context) {
    let typing = ctx.wants_keyboard_input();
    let popup = app.popup.is_some() || app.dialog.is_some();
    let a = app.active;

    // Кнопки мыши «Назад» / «Вперёд»
    let (back_btn, fwd_btn) = ctx.input(|i| (i.pointer.button_pressed(PointerButton::Extra1), i.pointer.button_pressed(PointerButton::Extra2)));
    if back_btn && !popup {
        app.go_back(ctx);
    }
    if fwd_btn && !popup {
        app.go_forward(ctx);
    }

    // Ctrl+колесо - смена размера значков
    let zoom = ctx.input(|i| if i.modifiers.ctrl { i.raw_scroll_delta.y } else { 0.0 });
    if zoom != 0.0 && !popup && app.tab().loc.is_listing() {
        let order = [ViewMode::Details, ViewMode::List, ViewMode::Small, ViewMode::Medium, ViewMode::Large, ViewMode::ExtraLarge];
        let cur = order.iter().position(|v| *v == app.tab().view.view).unwrap_or(0);
        let next = if zoom > 0.0 { (cur + 1).min(order.len() - 1) } else { cur.saturating_sub(1) };
        if next != cur {
            app.exec(ctx, Cmd::View(order[next]));
        }
    }

    if popup {
        return;
    }

    // Глобальные (работают и при вводе текста)
    let cs = Modifiers::CTRL | Modifiers::SHIFT;
    if pressed(ctx, Modifiers::CTRL, Key::T) {
        app.exec(ctx, Cmd::NewTab);
    }
    if pressed(ctx, Modifiers::CTRL, Key::W) {
        app.exec(ctx, Cmd::CloseTab(a));
        return;
    }
    if !ctx.input(|i| i.modifiers.shift) && pressed(ctx, Modifiers::CTRL, Key::N) {
        let loc = app.tab().loc.arg();
        crate::util::spawn_window(Some(&loc));
    }
    if pressed(ctx, cs, Key::Tab) {
        app.active = (app.active + app.tabs.len() - 1) % app.tabs.len();
    }
    if pressed(ctx, Modifiers::CTRL, Key::Tab) {
        app.active = (app.active + 1) % app.tabs.len();
    }
    for (k, n) in [(Key::Num1, 0), (Key::Num2, 1), (Key::Num3, 2), (Key::Num4, 3), (Key::Num5, 4), (Key::Num6, 5), (Key::Num7, 6), (Key::Num8, 7), (Key::Num9, 8)] {
        if n < 8 && pressed(ctx, cs, k) {
            app.exec(ctx, Cmd::View(ViewMode::ALL[n]));
        }
        if pressed(ctx, Modifiers::CTRL, k) {
            if n == 8 {
                app.active = app.tabs.len() - 1;
            } else if n < app.tabs.len() {
                app.active = n;
            }
        }
    }
    let alt = ctx.input(|i| i.modifiers.alt);
    if pressed(ctx, Modifiers::CTRL, Key::L) || pressed(ctx, Modifiers::ALT, Key::D) || (!alt && pressed(ctx, Modifiers::NONE, Key::F4)) {
        let t = app.tab_mut();
        t.address_edit = Some(match &t.loc {
            crate::app::Location::Path(p) => p.to_string_lossy().to_string(),
            crate::app::Location::Shell(s) => s.clone(),
            l => crate::app::OxideApp::title_of(l),
        });
        t.address_focus = true;
    }
    if pressed(ctx, Modifiers::CTRL, Key::F) || pressed(ctx, Modifiers::CTRL, Key::E) || pressed(ctx, Modifiers::NONE, Key::F3) {
        app.search_focus = true;
    }
    if pressed(ctx, Modifiers::NONE, Key::F5) || pressed(ctx, Modifiers::CTRL, Key::R) {
        app.exec(ctx, Cmd::Refresh);
    }
    if pressed(ctx, Modifiers::ALT | Modifiers::SHIFT, Key::P) {
        app.exec(ctx, Cmd::Pane(PaneMode::Details));
    }
    if pressed(ctx, Modifiers::ALT, Key::P) {
        app.exec(ctx, Cmd::Pane(PaneMode::Preview));
    }
    if pressed(ctx, Modifiers::ALT, Key::ArrowLeft) {
        app.go_back(ctx);
    }
    if pressed(ctx, Modifiers::ALT, Key::ArrowRight) {
        app.go_forward(ctx);
    }
    if pressed(ctx, Modifiers::ALT, Key::ArrowUp) {
        app.go_up(ctx);
    }

    if typing {
        window::PASTE_REQUEST.store(false, Ordering::Relaxed);
        return;
    }
    if !app.tab().loc.is_listing() && !app.tab().searching() {
        // Главная / Этот компьютер: только навигационные клавиши
        if pressed(ctx, Modifiers::NONE, Key::Backspace) {
            app.go_back(ctx);
        }
        window::PASTE_REQUEST.store(false, Ordering::Relaxed);
        return;
    }

    // Буфер обмена: Ctrl+C / Ctrl+X приходят событиями, Ctrl+V - через оконную процедуру
    let (copy, cut) = ctx.input(|i| (i.events.iter().any(|e| matches!(e, Event::Copy)), i.events.iter().any(|e| matches!(e, Event::Cut))));
    let shift_copy = ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift);
    if copy && shift_copy {
        app.exec(ctx, Cmd::CopyPath);
    } else if copy {
        app.exec(ctx, Cmd::Copy);
    }
    // Shift+Del egui превращает в «Вырезать» (старое сочетание Windows), а в Проводнике это
    // удаление без корзины. Ctrl+X при этом приходит без Shift
    let shift_only = ctx.input(|i| i.modifiers.shift && !i.modifiers.ctrl && !i.modifiers.alt);
    if cut && shift_only {
        app.exec(ctx, Cmd::DeletePermanent);
    } else if cut {
        app.exec(ctx, Cmd::Cut);
    }
    if window::PASTE_REQUEST.swap(false, Ordering::Relaxed) {
        app.exec(ctx, Cmd::Paste);
    }
    if pressed(ctx, Modifiers::CTRL, Key::A) {
        app.exec(ctx, Cmd::SelectAll);
    }
    if pressed(ctx, Modifiers::CTRL, Key::Z) {
        app.exec(ctx, Cmd::Undo);
    }
    if pressed(ctx, cs, Key::N) {
        app.exec(ctx, Cmd::NewFolder);
        return;
    }
    if pressed(ctx, Modifiers::NONE, Key::F2) {
        app.exec(ctx, Cmd::Rename);
    }
    if pressed(ctx, Modifiers::SHIFT, Key::Delete) {
        app.exec(ctx, Cmd::DeletePermanent);
    } else if pressed(ctx, Modifiers::NONE, Key::Delete) || pressed(ctx, Modifiers::CTRL, Key::D) {
        app.exec(ctx, Cmd::Delete);
    }
    if pressed(ctx, Modifiers::ALT, Key::Enter) {
        app.exec(ctx, Cmd::Properties);
        return;
    }
    if pressed(ctx, Modifiers::NONE, Key::Backspace) {
        app.go_back(ctx);
    }
    if pressed(ctx, Modifiers::NONE, Key::Escape) {
        if app.tab().searching() {
            app.end_search(ctx, a);
        } else {
            app.tab_mut().clear_sel();
        }
    }
    let shift_f10 = pressed(ctx, Modifiers::SHIFT, Key::F10);
    let ctx_key = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Key { key: Key::F10, pressed: true, modifiers, .. } if modifiers.shift)));
    if shift_f10 || ctx_key {
        // Как в Windows 11: Shift+F10 сразу открывает полное классическое меню
        let pos = focus_pos(app).unwrap_or(app.content_rect.center());
        app.show_system_menu(ctx, pos, false);
    }
    if pressed(ctx, Modifiers::NONE, Key::Enter) || pressed(ctx, Modifiers::CTRL, Key::Enter) {
        app.open_selection(ctx);
        return;
    }

    // Стрелки, Home/End, PageUp/PageDown
    let n = app.tab().order.len();
    if n == 0 {
        return;
    }
    let t = app.tab();
    let cols = t.grid_cols;
    let rows = t.list_rows.max(1);
    let page = t.page_items.max(1);
    let cur = t.focus.unwrap_or(0);
    let mods = ctx.input(|i| i.modifiers);
    let mut target: Option<usize> = None;
    let step = |k: Key| -> Option<isize> {
        match (k, cols) {
            (Key::ArrowDown, 0) => Some(1),
            (Key::ArrowUp, 0) => Some(-1),
            (Key::ArrowRight, 0) => Some(rows as isize),
            (Key::ArrowLeft, 0) => Some(-(rows as isize)),
            (Key::ArrowDown, c) => Some(c as isize),
            (Key::ArrowUp, c) => Some(-(c as isize)),
            (Key::ArrowRight, c) if c > 1 => Some(1),
            (Key::ArrowLeft, c) if c > 1 => Some(-1),
            _ => None,
        }
    };
    for k in [Key::ArrowDown, Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight] {
        let hit = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k) || i.consume_key(Modifiers::SHIFT, k) || i.consume_key(Modifiers::CTRL, k) || i.consume_key(cs, k));
        if hit {
            if let Some(d) = step(k) {
                let base = if t.focus.is_none() { 0 } else { cur as isize + d };
                target = Some(base.clamp(0, n as isize - 1) as usize);
            }
        }
    }
    let home = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Home) || i.consume_key(Modifiers::SHIFT, Key::Home));
    let end = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::End) || i.consume_key(Modifiers::SHIFT, Key::End));
    let pgup = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::PageUp) || i.consume_key(Modifiers::SHIFT, Key::PageUp));
    let pgdn = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::PageDown) || i.consume_key(Modifiers::SHIFT, Key::PageDown));
    if home {
        target = Some(0);
    }
    if end {
        target = Some(n - 1);
    }
    if pgup {
        target = Some(cur.saturating_sub(page));
    }
    if pgdn {
        target = Some((cur + page).min(n - 1));
    }
    if let Some(tg) = target {
        let t = app.tab_mut();
        if mods.shift {
            let anc = t.anchor.unwrap_or(cur);
            let (lo, hi) = if anc <= tg { (anc, tg) } else { (tg, anc) };
            if !mods.ctrl {
                t.clear_sel();
            }
            for k in lo..=hi {
                let idx = t.order[k];
                t.sel[idx] = true;
            }
            if t.anchor.is_none() {
                t.anchor = Some(cur);
            }
        } else if mods.ctrl {
            // Только фокус
        } else {
            let idx = t.order[tg];
            t.select_only(idx);
        }
        t.focus = Some(tg);
        t.scroll_to_focus = true;
        app.kb_focus = true;
    }
    if pressed(ctx, Modifiers::CTRL, Key::Space) {
        let t = app.tab_mut();
        if let Some(f) = t.focus {
            let idx = t.order[f];
            t.sel[idx] = !t.sel[idx];
        }
    }

    // Поиск по первым буквам (как в Проводнике)
    let typed: String = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                Event::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    });
    if !typed.is_empty() && !ctx.input(|i| i.modifiers.ctrl || i.modifiers.alt) {
        let t_now = now(ctx);
        let t = app.tab_mut();
        if t_now - t.type_ahead.1 > 1.0 {
            t.type_ahead.0.clear();
        }
        t.type_ahead.0.push_str(&typed);
        t.type_ahead.1 = t_now;
        let prefix = crate::util::lower(&t.type_ahead.0);
        let start = if t.type_ahead.0.chars().count() == 1 { t.focus.map(|f| f + 1).unwrap_or(0) } else { t.focus.unwrap_or(0) };
        let n = t.order.len();
        for k in 0..n {
            let pos = (start + k) % n;
            let e = &t.entries[t.order[pos]];
            if crate::util::lower(&e.display).starts_with(&prefix) {
                let idx = t.order[pos];
                t.select_only(idx);
                t.scroll_to_focus = true;
                break;
            }
        }
        app.kb_focus = true;
    }
}

fn focus_pos(app: &OxideApp) -> Option<egui::Pos2> {
    let _ = app.tab().focus?;
    Some(app.content_rect.left_top() + egui::vec2(120.0, 60.0))
}
