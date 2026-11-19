//! Кнопки навигации, адресная строка с «хлебными крошками» и поле поиска.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Align2, CornerRadius, Id, Key, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2};

use super::menu::{MenuItem, Popup};
use super::widgets::*;
use crate::app::{now, Location, OxideApp};
use crate::cmd::Cmd;
use crate::shell::{fs, shellfolder};
use crate::theme::m;

thread_local! {
    static CRUMBS: RefCell<HashMap<String, Vec<(String, Location)>>> = RefCell::new(HashMap::new());
}

pub fn crumbs(loc: &Location) -> Vec<(String, Location)> {
    let key = loc.key();
    if let Some(v) = CRUMBS.with(|c| c.borrow().get(&key).cloned()) {
        return v;
    }
    let v = build_crumbs(loc);
    CRUMBS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 300 {
            c.clear();
        }
        c.insert(key, v.clone());
    });
    v
}

fn path_crumbs(p: &Path) -> Vec<(String, Location)> {
    let s = p.to_string_lossy().to_string();
    let mut out = vec![];
    if s.starts_with(r"\\") {
        out.push(("Сеть".to_string(), Location::Shell(shellfolder::NETWORK.into())));
        let parts: Vec<&str> = s.trim_start_matches('\\').split('\\').filter(|x| !x.is_empty()).collect();
        let mut acc = String::from(r"\\");
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                acc.push('\\');
            }
            acc.push_str(part);
            out.push((part.to_string(), Location::Path(PathBuf::from(&acc))));
        }
        return out;
    }
    out.push(("Этот компьютер".to_string(), Location::ThisPc));
    let mut acc = PathBuf::new();
    for (i, comp) in p.components().enumerate() {
        let c = comp.as_os_str().to_string_lossy().to_string();
        if i == 0 {
            acc = PathBuf::from(format!("{}\\", c.trim_end_matches('\\')));
            let label = fs::shell_display_name(&acc).unwrap_or(c);
            out.push((label, Location::Path(acc.clone())));
            continue;
        }
        if c == "\\" {
            continue;
        }
        acc.push(&c);
        let mut label = c.clone();
        if acc.join("desktop.ini").exists() {
            if let Some(d) = fs::shell_display_name(&acc) {
                label = d;
            }
        }
        out.push((label, Location::Path(acc.clone())));
    }
    out
}

fn build_crumbs(loc: &Location) -> Vec<(String, Location)> {
    match loc {
        Location::Home => vec![("Главная".into(), Location::Home)],
        Location::ThisPc => vec![("Этот компьютер".into(), Location::ThisPc)],
        Location::Path(p) => path_crumbs(p),
        Location::Shell(s) => {
            let p = Path::new(s);
            if p.is_absolute() && p.exists() {
                let mut v = path_crumbs(p.parent().unwrap_or(p));
                v.push((p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), loc.clone()));
                v
            } else {
                vec![(OxideApp::title_of(loc), loc.clone())]
            }
        }
    }
}

fn address_text(loc: &Location) -> String {
    match loc {
        Location::Home => "Главная".into(),
        Location::ThisPc => "Этот компьютер".into(),
        Location::Path(p) => p.to_string_lossy().to_string(),
        Location::Shell(s) => s.clone(),
    }
}

pub fn show(app: &mut OxideApp, ui: &mut Ui) {
    let rect = ui.max_rect();
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let cy = rect.center().y;
    let a = app.active;

    // Назад / Вперёд / Вверх / Обновить.
    let mut x = rect.left() + 14.0;
    let btn = |x: f32| Rect::from_min_size(Pos2::new(x, cy - m::BTN_H / 2.0), Vec2::new(m::NAV_BTN_W, m::BTN_H));
    let can_back = !app.tabs[a].back.is_empty() || app.tabs[a].searching();
    let can_fwd = !app.tabs[a].fwd.is_empty();
    let can_up = OxideApp::parent_of(&app.tabs[a].loc).is_some();
    let back_title = app.tabs[a].back.last().map(OxideApp::title_of).unwrap_or_default();
    let fwd_title = app.tabs[a].fwd.last().map(OxideApp::title_of).unwrap_or_default();
    let up_title = OxideApp::parent_of(&app.tabs[a].loc).map(|l| OxideApp::title_of(&l)).unwrap_or_default();
    let r = icon_button(ui, btn(x), Id::new("nav-back"), ICON_BACK, 16.0, &pal, can_back, &format!("Назад к «{back_title}» (Alt+Стрелка влево)"));
    if r.clicked() {
        app.go_back(&ctx);
    }
    if r.secondary_clicked() && can_back {
        history_menu(app, &ctx, r.rect, true);
    }
    x += 48.0;
    let r = icon_button(ui, btn(x), Id::new("nav-fwd"), ICON_FORWARD, 16.0, &pal, can_fwd, &format!("Вперёд к «{fwd_title}» (Alt+Стрелка вправо)"));
    if r.clicked() {
        app.go_forward(&ctx);
    }
    if r.secondary_clicked() && can_fwd {
        history_menu(app, &ctx, r.rect, false);
    }
    x += 48.0;
    let r = icon_button(ui, btn(x), Id::new("nav-up"), ICON_UP, 16.0, &pal, can_up, &format!("Вверх к «{up_title}» (Alt+Стрелка вверх)"));
    if r.clicked() {
        app.go_up(&ctx);
    }
    x += 48.0;
    let r = icon_button(ui, btn(x), Id::new("nav-refresh"), ICON_REFRESH, 16.0, &pal, true, &format!("Обновить «{}» (F5)", app.tabs[a].title));
    if r.clicked() {
        app.exec(&ctx, Cmd::Refresh);
    }
    x += m::NAV_BTN_W + 12.0;

    // Поле поиска справа.
    let sw = app.settings.search_width;
    let search_r = Rect::from_min_size(Pos2::new(rect.right() - 16.0 - sw, cy - m::FIELD_H / 2.0), Vec2::new(sw, m::FIELD_H));
    let addr_r = Rect::from_min_max(Pos2::new(x, cy - m::FIELD_H / 2.0), Pos2::new(search_r.left() - 8.0, cy + m::FIELD_H / 2.0));

    address(app, ui, addr_r);
    search(app, ui, search_r);
}

fn history_menu(app: &mut OxideApp, ctx: &egui::Context, anchor: Rect, back: bool) {
    let t = app.tab();
    let list: Vec<Location> = if back { t.back.iter().rev().take(12).cloned().collect() } else { t.fwd.iter().rev().take(12).cloned().collect() };
    let items: Vec<MenuItem> = list
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let n = (i + 1) as isize;
            MenuItem::new(OxideApp::title_of(l), Cmd::HistoryJump(if back { -n } else { n }))
        })
        .collect();
    let id = app.new_popup_id();
    app.open_popup(Popup::below(id, anchor, items, now(ctx)));
}

fn field_frame(ui: &Ui, r: Rect, pal: &crate::theme::Palette, hovered: bool, focused: bool) {
    let p = ui.painter();
    let fill = if focused { pal.field } else if hovered { pal.field_hover } else { pal.field };
    p.rect_filled(r, CornerRadius::same(m::R_CTRL), fill);
    if !pal.dark {
        p.rect_stroke(r, CornerRadius::same(m::R_CTRL), Stroke::new(1.0, pal.field_stroke), StrokeKind::Inside);
    }
    if focused {
        let b = Rect::from_min_max(Pos2::new(r.left() + 1.0, r.bottom() - 2.0), Pos2::new(r.right() - 1.0, r.bottom()));
        p.rect_filled(b, CornerRadius { nw: 0, ne: 0, sw: m::R_CTRL, se: m::R_CTRL }, pal.accent);
    }
}

fn address(app: &mut OxideApp, ui: &mut Ui, r: Rect) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let a = app.active;
    let editing = app.tabs[a].address_edit.is_some();
    let bg = ui.interact(r, Id::new("addr-bg"), Sense::click());
    field_frame(ui, r, &pal, bg.hovered(), editing);
    crate::theme::set_surface(if bg.hovered() && !editing { pal.field_hover } else { pal.field });

    if editing {
        let id = Id::new(("addr-edit", app.tabs[a].id));
        let mut text = app.tabs[a].address_edit.clone().unwrap_or_default();
        let inner = Rect::from_min_max(Pos2::new(r.left() + 10.0, r.top() + 4.0), Pos2::new(r.right() - 10.0, r.bottom() - 4.0));
        let out = ui
            .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                TextEdit::singleline(&mut text)
                    .id(id)
                    .frame(false)
                    .font(font(14.0))
                    .text_color(pal.text)
                    .vertical_align(egui::Align::Center)
                    .desired_width(inner.width())
                    .show(ui)
            })
            .inner;
        if app.tabs[a].address_focus {
            app.tabs[a].address_focus = false;
            out.response.request_focus();
            let mut st = out.state.clone();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()))));
            st.store(&ctx, id);
        }
        let enter = out.response.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        app.tabs[a].address_edit = Some(text.clone());
        if enter {
            app.tabs[a].address_edit = None;
            go_address(app, &ctx, text.trim());
        } else if esc || (out.response.lost_focus() && !enter) {
            app.tabs[a].address_edit = None;
        }
        return;
    }

    // Режим «хлебных крошек».
    let cy = r.center().y;
    let loc = app.tabs[a].loc.clone();
    super::titlebar::paint_loc_icon(app, ui, &loc, Pos2::new(r.left() + 20.0, cy));
    let mut cr = crumbs(&loc);
    if app.tabs[a].searching() {
        let t = app.tabs[a].title.clone();
        cr.push((format!("Результаты поиска в «{t}»"), loc.clone()));
    }
    let start_x = r.left() + 36.0;
    let avail = r.right() - 8.0 - start_x;
    let widths: Vec<f32> = cr.iter().map(|(l, _)| text_width(ui, l, font(14.0)) + 12.0 + 24.0).collect();
    let mut first = 0usize;
    let mut total: f32 = widths.iter().sum();
    while first + 1 < cr.len() && total > avail - 32.0 {
        total -= widths[first];
        first += 1;
    }
    let mut x = start_x;
    let mut nav_to: Option<Location> = None;
    let mut dropdown: Option<(Rect, Option<Location>, Vec<(String, Location)>)> = None;
    if first > 0 {
        let br = Rect::from_min_size(Pos2::new(x, cy - 12.0), Vec2::new(28.0, 24.0));
        let resp = ui.interact(br, Id::new("crumb-overflow"), Sense::click());
        subtle_bg(ui, &resp, br, &pal, m::R_CTRL, false);
        glyph(ui.painter(), br.center(), '\u{E76B}', 10.0, pal.text2);
        glyph(ui.painter(), br.center() + Vec2::new(5.0, 0.0), '\u{E76B}', 10.0, pal.text2);
        if resp.clicked() {
            dropdown = Some((br, None, cr[..first].to_vec()));
        }
        x += 28.0;
    }
    let last_i = cr.len().saturating_sub(1);
    for (i, (label, l)) in cr.iter().enumerate().skip(first) {
        // Шеврон перед элементом — список подпапок родителя.
        let chev = Rect::from_min_size(Pos2::new(x, cy - 12.0), Vec2::new(20.0, 24.0));
        let resp = ui.interact(chev, Id::new(("crumb-chev", i)), Sense::click());
        subtle_bg(ui, &resp, chev, &pal, m::R_CTRL, false);
        glyph(ui.painter(), chev.center(), ICON_CHEVRON_RIGHT, 10.0, pal.text2);
        if resp.clicked() {
            let parent = if i == 0 { None } else { Some(cr[i - 1].1.clone()) };
            dropdown = Some((chev, parent, vec![]));
        }
        x += 24.0;
        let tw = text_width(ui, label, font(14.0));
        let lr = Rect::from_min_size(Pos2::new(x, cy - 12.0), Vec2::new((tw + 12.0).min((r.right() - 8.0 - x).max(20.0)), 24.0));
        let resp = ui.interact(lr, Id::new(("crumb", i)), Sense::click());
        subtle_bg(ui, &resp, lr, &pal, m::R_CTRL, false);
        text_trunc(ui.painter(), Pos2::new(lr.left() + 6.0, cy), label, font(14.0), pal.text, lr.width() - 10.0);
        if resp.clicked() && (i != last_i || app.tabs[a].searching()) {
            nav_to = Some(l.clone());
        }
        if resp.middle_clicked() {
            app.open_tab(&ctx, l.clone(), false);
        }
        x += lr.width();
        if x > r.right() - 8.0 {
            break;
        }
    }
    // Последний шеврон — подпапки текущей папки.
    if x + 24.0 < r.right() - 4.0 && app.tabs[a].loc.is_listing() && !app.tabs[a].searching() {
        let chev = Rect::from_min_size(Pos2::new(x, cy - 12.0), Vec2::new(20.0, 24.0));
        let resp = ui.interact(chev, Id::new("crumb-chev-last"), Sense::click());
        subtle_bg(ui, &resp, chev, &pal, m::R_CTRL, false);
        glyph(ui.painter(), chev.center(), ICON_CHEVRON_RIGHT, 10.0, pal.text2);
        if resp.clicked() {
            dropdown = Some((chev, Some(loc.clone()), vec![]));
        }
    }
    if bg.clicked() {
        app.tabs[a].address_edit = Some(address_text(&loc));
        app.tabs[a].address_focus = true;
    }
    if let Some(l) = nav_to {
        app.navigate(&ctx, l);
    }
    if let Some((anchor, parent, extra)) = dropdown {
        let items: Vec<MenuItem> = if !extra.is_empty() {
            extra.into_iter().rev().map(|(n, l)| MenuItem::new(n, loc_cmd(&l))).collect()
        } else {
            match parent {
                None => vec![
                    MenuItem::new("Главная", Cmd::NavigateHome).icon(ICON_HOME),
                    MenuItem::new("Этот компьютер", Cmd::NavigateThisPc).icon(ICON_PC),
                    MenuItem::new("Сеть", Cmd::NavigateShell(shellfolder::NETWORK.into())).icon(ICON_NETWORK),
                    MenuItem::new("Корзина", Cmd::NavigateShell(shellfolder::RECYCLE_BIN.into())).icon(ICON_DELETE),
                ],
                Some(Location::ThisPc) => app
                    .drives
                    .iter()
                    .map(|d| MenuItem::new(d.label.clone(), Cmd::Navigate(d.root.clone())).sys(d.root.to_string_lossy().to_string(), d.root.to_string_lossy().to_string()))
                    .collect(),
                Some(Location::Path(p)) => fs::subdirs(&p, app.settings.show_hidden)
                    .into_iter()
                    .take(400)
                    .map(|(n, pp)| MenuItem::new(n, Cmd::Navigate(pp)).sys("<dir>".into(), p.to_string_lossy().to_string()))
                    .collect(),
                _ => vec![],
            }
        };
        if !items.is_empty() {
            let id = app.new_popup_id();
            let mut pop = Popup::below(id, anchor, items, now(&ctx));
            pop.min_w = 200.0;
            app.open_popup(pop);
        }
    }
}

fn loc_cmd(l: &Location) -> Cmd {
    match l {
        Location::Home => Cmd::NavigateHome,
        Location::ThisPc => Cmd::NavigateThisPc,
        Location::Path(p) => Cmd::Navigate(p.clone()),
        Location::Shell(s) => Cmd::NavigateShell(s.clone()),
    }
}

/// Переход по введённому адресу (путь, shell:, переменные окружения, программа).
fn go_address(app: &mut OxideApp, ctx: &egui::Context, text: &str) {
    if text.is_empty() {
        return;
    }
    let low = text.to_lowercase();
    if low == "главная" || low == "home" {
        app.navigate(ctx, Location::Home);
        return;
    }
    if low == "этот компьютер" || low == "this pc" {
        app.navigate(ctx, Location::ThisPc);
        return;
    }
    let expanded = expand_env(text);
    let p = PathBuf::from(&expanded);
    if p.is_absolute() && fs::is_dir(&p) {
        app.navigate(ctx, Location::Path(p));
        return;
    }
    if p.is_absolute() && fs::exists(&p) {
        crate::shell::menu::open(&expanded);
        return;
    }
    if low.starts_with("shell:") || low.starts_with("::") {
        // shell:downloads и т.п. — пробуем получить путь файловой системы.
        if let Some(fsp) = shell_fs_path(&expanded) {
            app.navigate(ctx, Location::Path(fsp));
        } else {
            app.navigate(ctx, Location::Shell(expanded));
        }
        return;
    }
    if let Some(dir) = app.tab().fs_dir() {
        let rel = dir.join(&expanded);
        if fs::is_dir(&rel) {
            app.navigate(ctx, Location::Path(rel));
            return;
        }
    }
    // Как в Проводнике: команда (cmd, notepad, https://...) запускается.
    if !crate::shell::menu::shell_execute(&expanded, None, None) {
        app.dialog = Some(crate::app::Dialog::Message {
            title: "Проводник Oxide".into(),
            text: format!("Не удаётся найти «{text}». Проверьте правильность написания и повторите попытку."),
        });
    }
}

fn shell_fs_path(name: &str) -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH};
    unsafe {
        let w = crate::util::wide(name);
        let it: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).ok()?;
        let p = crate::util::from_pwstr(it.GetDisplayName(SIGDN_FILESYSPATH).ok()?);
        if p.is_empty() {
            None
        } else {
            Some(PathBuf::from(p))
        }
    }
}

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        if let Some(j) = after.find('%') {
            let var = &after[..j];
            match std::env::var(var) {
                Ok(v) => out.push_str(&v),
                Err(_) => {
                    out.push('%');
                    out.push_str(var);
                    out.push('%');
                }
            }
            rest = &after[j + 1..];
        } else {
            out.push_str(&rest[i..]);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn search(app: &mut OxideApp, ui: &mut Ui, r: Rect) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let a = app.active;
    let id = Id::new(("search", app.tabs[a].id));
    let focused = ctx.memory(|m| m.has_focus(id));
    let bg = ui.interact(r, Id::new("search-bg"), Sense::click());
    field_frame(ui, r, &pal, bg.hovered(), focused);
    crate::theme::set_surface(pal.field);
    let has_text = !app.tabs[a].search_text.is_empty();
    let icon_r = Rect::from_center_size(Pos2::new(r.right() - 18.0, r.center().y), Vec2::new(28.0, 24.0));
    let inner = Rect::from_min_max(Pos2::new(r.left() + 10.0, r.top() + 4.0), Pos2::new(icon_r.left() - if has_text { 28.0 } else { 2.0 }, r.bottom() - 4.0));
    let mut text = app.tabs[a].search_text.clone();
    let hint = format!("Поиск в: {}", app.tabs[a].title);
    let out = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            TextEdit::singleline(&mut text)
                .id(id)
                .frame(false)
                .font(font(14.0))
                .text_color(pal.text)
                .hint_text(egui::RichText::new(hint).color(pal.text3).font(font(14.0)))
                .vertical_align(egui::Align::Center)
                .desired_width(inner.width())
                .show(ui)
        })
        .inner;
    if app.search_focus {
        app.search_focus = false;
        out.response.request_focus();
    }
    if bg.clicked() {
        out.response.request_focus();
    }
    if out.response.changed() {
        app.tabs[a].search_text = text.clone();
        app.tabs[a].search_edit_at = Some(now(&ctx));
        if text.is_empty() {
            app.tabs[a].search_edit_at = None;
            app.end_search(&ctx, a);
        }
    }
    if out.response.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter)) {
        app.tabs[a].search_edit_at = None;
        app.start_search(&ctx, a);
    }
    if focused && ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.end_search(&ctx, a);
        out.response.surrender_focus();
    }
    if has_text {
        let cr = Rect::from_center_size(Pos2::new(icon_r.left() - 12.0, r.center().y), Vec2::new(24.0, 24.0));
        let resp = icon_button(ui, cr, Id::new("search-clear"), ICON_CLOSE, 10.0, &pal, true, "Очистить");
        if resp.clicked() {
            app.end_search(&ctx, a);
        }
    }
    let running = app.tabs[a].search.as_ref().map(|s| s.running).unwrap_or(false);
    if running {
        // Вращающийся индикатор вместо лупы, пока идёт поиск.
        let t = ctx.input(|i| i.time) as f32;
        let c = icon_r.center();
        let start = t * 6.0;
        let pts: Vec<Pos2> = (0..=20).map(|k| {
            let ang = start + k as f32 / 20.0 * 4.2;
            c + Vec2::new(ang.cos(), ang.sin()) * 6.5
        }).collect();
        ui.painter().add(egui::Shape::line(pts, Stroke::new(1.6, pal.accent)));
        ctx.request_repaint();
    } else {
        let resp = ui.interact(icon_r, Id::new("search-go"), Sense::click());
        subtle_bg(ui, &resp, icon_r, &pal, m::R_CTRL, false);
        glyph(ui.painter(), icon_r.center(), ICON_SEARCH, 14.0, pal.text2);
        if resp.clicked() {
            app.start_search(&ctx, a);
        }
    }
    let _ = Align2::LEFT_CENTER;
}
