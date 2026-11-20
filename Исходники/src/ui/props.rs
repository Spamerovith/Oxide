//! Окно «Свойства» Oxide: вкладка «Общие» со всеми сведениями Проводника. Размер папки виден сразу
//! (из индекса), точный подсчёт идёт в фоне. Остальные вкладки («Доступ», «Безопасность»,
//! «Предыдущие версии», «Настройка», «Подробно»…) открывают системное окно сразу на этой вкладке

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2};

use super::widgets::*;
use crate::app::OxideApp;
use crate::i18n::l;
use crate::search::FolderStats;
use crate::shell::fs;
use crate::shell::ops::Op;
use crate::shell::props::{self as sp, Count, ItemInfo, ATTR_HIDDEN, ATTR_READONLY};
use crate::theme::Palette;
use crate::util::{format_bytes, format_filetime_long, group_digits};

const W: f32 = 520.0;
const LABEL_W: f32 = 128.0;

pub struct Props {
    items: Vec<ItemInfo>,
    name: String,
    orig_name: String,
    /// Флажки атрибутов: None - у выбранных по-разному
    readonly: Option<bool>,
    hidden: Option<bool>,
    orig_readonly: Option<bool>,
    orig_hidden: Option<bool>,
    /// Применять атрибуты и к содержимому папок
    nested: bool,
    /// Содержимое папок по индексу (мгновенно)
    quick: Option<FolderStats>,
    live: Arc<Mutex<Count>>,
    cancel: Arc<AtomicBool>,
    opens_with: Option<String>,
    focus_name: bool,
}

impl Drop for Props {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn tri(v: impl Iterator<Item = bool>) -> Option<bool> {
    let mut it = v.peekable();
    let first = *it.peek()?;
    if it.all(|b| b == first) {
        Some(first)
    } else {
        None
    }
}

impl Props {
    pub fn new(app: &OxideApp, ctx: &egui::Context, paths: Vec<PathBuf>) -> Props {
        let items = sp::items_info(&paths);
        let readonly = tri(items.iter().map(|i| i.attrs & ATTR_READONLY != 0));
        let hidden = tri(items.iter().map(|i| i.attrs & ATTR_HIDDEN != 0));
        let dirs: Vec<PathBuf> = items.iter().filter(|i| i.is_dir).map(|i| i.path.clone()).collect();
        // Мгновенно - из индекса, если там есть все выбранные папки
        let quick = if dirs.is_empty() {
            None
        } else {
            dirs.iter().map(|d| app.engine.folder_stats(d)).try_fold(FolderStats::default(), |acc, s| {
                let s = s?;
                Some(FolderStats { size: acc.size + s.size, alloc: acc.alloc + s.alloc, files: acc.files + s.files, dirs: acc.dirs + s.dirs })
            })
        };
        let live = Arc::new(Mutex::new(Count { done: dirs.is_empty(), ..Default::default() }));
        let cancel = Arc::new(AtomicBool::new(false));
        if !dirs.is_empty() {
            sp::count_tree(dirs, live.clone(), cancel.clone(), ctx.clone());
        }
        let name = if items.len() == 1 { items[0].path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() } else { String::new() };
        let opens_with = if items.len() == 1 && !items[0].is_dir {
            let ext = fs::ext_of(&name);
            if matches!(ext.as_str(), "exe" | "com" | "bat" | "cmd" | "msi" | "lnk" | "scr") {
                None
            } else {
                sp::opens_with(&ext)
            }
        } else {
            None
        };
        Props {
            items,
            orig_name: name.clone(),
            name,
            readonly,
            hidden,
            orig_readonly: readonly,
            orig_hidden: hidden,
            nested: false,
            quick,
            live,
            cancel,
            opens_with,
            focus_name: true,
        }
    }

    fn single(&self) -> Option<&ItemInfo> {
        if self.items.len() == 1 {
            self.items.first()
        } else {
            None
        }
    }

    fn changed(&self) -> bool {
        (self.single().is_some() && self.name.trim() != self.orig_name) || self.readonly != self.orig_readonly || self.hidden != self.orig_hidden
    }

    /// Итог по выбранному: (размер, на диске, файлов, папок, подсчёт ещё идёт)
    fn totals(&self) -> (u64, u64, u64, u64, bool) {
        let live = *self.live.lock().unwrap();
        let (mut size, mut alloc, mut files, mut dirs) = (0u64, 0u64, 0u64, 0u64);
        for i in &self.items {
            if !i.is_dir {
                size += i.size;
                alloc += i.alloc;
            }
        }
        let content = if live.done {
            (live.size, live.alloc, live.files, live.dirs)
        } else if let Some(q) = self.quick {
            (q.size, q.alloc, q.files, q.dirs)
        } else {
            (live.size, live.alloc, live.files, live.dirs)
        };
        size += content.0;
        alloc += content.1;
        files += content.2;
        dirs += content.3;
        if self.items.len() > 1 {
            files += self.items.iter().filter(|i| !i.is_dir).count() as u64;
            dirs += self.items.iter().filter(|i| i.is_dir).count() as u64;
        }
        (size, alloc, files, dirs, !live.done && self.quick.is_none())
    }

    fn apply(&mut self, app: &mut OxideApp, ctx: &egui::Context) -> bool {
        if let Some(it) = self.single().cloned() {
            let new = self.name.trim().to_string();
            if new != self.orig_name {
                if new.is_empty() {
                    return false;
                }
                if crate::util::invalid_name_char(&new).is_some() {
                    app.toast = Some((tr("Имя файла не должно содержать следующих знаков: \\ / : * ? \" < > |").into(), crate::app::now(ctx)));
                    return false;
                }
                app.ops.run(Op::Rename { path: it.path.clone(), new_name: new.clone() });
                let np = it.path.with_file_name(&new);
                self.items[0].path = np;
                self.orig_name = new;
            }
        }
        let mut set = 0;
        let mut clear = 0;
        for (now, was, bit) in [(self.readonly, self.orig_readonly, ATTR_READONLY), (self.hidden, self.orig_hidden, ATTR_HIDDEN)] {
            if now != was {
                match now {
                    Some(true) => set |= bit,
                    Some(false) => clear |= bit,
                    None => {}
                }
            }
        }
        if set | clear != 0 {
            let items: Vec<(PathBuf, u32)> = self.items.iter().map(|i| (i.path.clone(), i.attrs)).collect();
            let nested = self.nested && self.items.iter().any(|i| i.is_dir);
            let c = ctx.clone();
            let tx = app.tx.clone();
            sp::apply_attrs(items, set, clear, nested, move |failed| {
                if failed > 0 {
                    let _ = tx.send(crate::app::Msg::Toast(trf("Не удалось изменить атрибуты у элементов: {}", &[&failed])));
                }
                let _ = tx.send(crate::app::Msg::RefreshAll);
                c.request_repaint();
            });
            for i in self.items.iter_mut() {
                i.attrs = (i.attrs | set) & !clear;
            }
            self.orig_readonly = self.readonly;
            self.orig_hidden = self.hidden;
        }
        app.fs_dirty_all = true;
        true
    }
}

/// Флажок с третьим состоянием «по-разному» (черта), как в Проводнике
fn tri_checkbox(ui: &mut Ui, v: &mut Option<bool>, label: &str, pal: &Palette) {
    // Подпись может быть в две строки («Только для чтения» и пояснение под ним)
    let galley = ui.painter().layout_no_wrap(label.to_string(), font(14.0), pal.text);
    let h = (galley.size().y + 12.0).max(32.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(20.0 + 8.0 + galley.size().x + 4.0, h), Sense::click());
    if resp.clicked() {
        *v = Some(!v.unwrap_or(false));
    }
    let p = ui.painter();
    let b = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + 6.0), Vec2::splat(20.0));
    match v {
        Some(true) => {
            p.rect_filled(b, CornerRadius::same(4), pal.accent);
            glyph(p, b.center(), ICON_CHECK, 12.0, pal.on_accent);
        }
        Some(false) => {
            p.rect_filled(b, CornerRadius::same(4), if resp.hovered() { pal.fill(pal.hover) } else { Color32::TRANSPARENT });
            p.rect_stroke(b, CornerRadius::same(4), Stroke::new(1.0, pal.text2), StrokeKind::Inside);
        }
        None => {
            p.rect_filled(b, CornerRadius::same(4), pal.accent);
            p.line_segment([Pos2::new(b.center().x - 5.0, b.center().y), Pos2::new(b.center().x + 5.0, b.center().y)], Stroke::new(1.5, pal.on_accent));
        }
    }
    let first_line = b.center().y - galley.rows.first().map(|r| r.height() / 2.0).unwrap_or(10.0);
    p.galley(Pos2::new(b.right() + 8.0, first_line), galley, pal.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, v.unwrap_or(false), label));
}

/// Подпись слева в столбце шириной LABEL_W, выровненная по левому краю
fn label_cell(ui: &mut Ui, pal: &Palette, label: &str, h: f32, center: bool) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(LABEL_W, h), Sense::hover());
    if center {
        ui.painter().text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, label, font(14.0), pal.text2);
    } else {
        ui.painter().text(r.left_top(), Align2::LEFT_TOP, label, font(14.0), pal.text2);
    }
}

/// Строка «Подпись: значение»
fn row(ui: &mut Ui, pal: &Palette, label: &str, value: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        label_cell(ui, pal, label, 18.0, false);
        ui.add(egui::Label::new(egui::RichText::new(value).font(font(14.0)).color(pal.text)).wrap());
    });
    ui.add_space(8.0);
}

fn divider(ui: &mut Ui, pal: &Palette) {
    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(W, 1.0), Sense::hover());
    hline(ui.painter(), r.center().y, r.left(), r.right(), pal.divider);
    ui.add_space(12.0);
}

fn size_text(n: u64) -> String {
    trf("{} ({} байт)", &[&format_bytes(n), &group_digits(n)])
}

/// Содержимое окна. `keep` = false закрывает его
pub fn ui(app: &mut OxideApp, ui: &mut Ui, p: &mut Props, keep: &mut bool) {
    let ctx = ui.ctx().clone();
    let pal = app.pal.clone();
    ui.set_width(W);
    let single = p.single().cloned();
    let title = match &single {
        Some(i) => i.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| i.path.to_string_lossy().to_string()),
        None => trf("Файлов: {}, папок: {}", &[&p.items.iter().filter(|i| !i.is_dir).count(), &p.items.iter().filter(|i| i.is_dir).count()]),
    };
    ui.label(egui::RichText::new(trf("Свойства: {}", &[&title])).font(font_sb(20.0)).color(pal.text));
    ui.add_space(12.0);

    // Вкладки: «Общие» - наша, остальные открывают системное окно сразу на нужной вкладке
    let mut pages: Vec<(&str, &str, &str)> = vec![];
    if let Some(i) = &single {
        let ext = fs::ext_of(&i.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        if i.is_dir {
            pages.push((tr("Доступ"), "Доступ", "Sharing"));
            pages.push((tr("Безопасность"), "Безопасность", "Security"));
            pages.push((tr("Предыдущие версии"), "Предыдущие версии", "Previous Versions"));
            pages.push((tr("Настройка"), "Настройка", "Customize"));
        } else {
            if ext == "lnk" {
                pages.push((tr("Ярлык"), "Ярлык", "Shortcut"));
            }
            if matches!(ext.as_str(), "exe" | "lnk" | "msi") {
                pages.push((tr("Совместимость"), "Совместимость", "Compatibility"));
            }
            pages.push((tr("Безопасность"), "Безопасность", "Security"));
            pages.push((tr("Подробно"), "Подробно", "Details"));
            pages.push((tr("Предыдущие версии"), "Предыдущие версии", "Previous Versions"));
        }
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let tab = |ui: &mut Ui, label: &str, on: bool, id: Id| -> egui::Response {
            let tw = text_width(ui, label, if on { font_sb(14.0) } else { font(14.0) });
            let (r, resp) = ui.allocate_exact_size(Vec2::new(tw + 24.0, 32.0), Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            let _ = id;
            if resp.hovered() && !on {
                ui.painter().rect_filled(r, CornerRadius::same(4), pal.fill(pal.hover));
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, label, if on { font_sb(14.0) } else { font(14.0) }, if on { pal.text } else { pal.text2 });
            if on {
                let bar = Rect::from_center_size(Pos2::new(r.center().x, r.bottom() - 1.5), Vec2::new(16.0, 3.0));
                ui.painter().rect_filled(bar, CornerRadius::same(2), pal.accent);
            }
            resp
        };
        tab(ui, tr("Общие"), true, Id::new("pp-general"));
        for (k, (label, ru, en)) in pages.iter().enumerate() {
            if tab(ui, label, false, Id::new(("pp-tab", k))).clicked() {
                if let Some(i) = &single {
                    sp::system_properties(&i.path, Some(sp::system_page_name(ru, en)));
                }
            }
        }
    });
    ui.add_space(12.0);

    // Значок и имя
    ui.horizontal(|ui| {
        let (ir, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
        let ppp = ctx.pixels_per_point();
        let first = p.items.first().map(|i| i.path.to_string_lossy().to_string()).unwrap_or_default();
        if let Some(tex) = app.icons.get(&first, &first, (32.0 * ppp).round() as u32, false) {
            paint_texture(ui.painter(), tex, ir.center(), 32.0);
        }
        ui.add_space(12.0);
        if single.is_some() {
            let (fr, _) = ui.allocate_exact_size(Vec2::new(W - 52.0, 32.0), Sense::hover());
            ui.painter().rect_filled(fr, CornerRadius::same(4), pal.field);
            ui.painter().rect_stroke(fr, CornerRadius::same(4), Stroke::new(1.0, pal.field_stroke), StrokeKind::Inside);
            let inner = fr.shrink2(Vec2::new(10.0, 0.0));
            let out = ui
                .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    TextEdit::singleline(&mut p.name)
                        .id(Id::new("pp-name"))
                        .frame(false)
                        .font(font(14.0))
                        .text_color(pal.text)
                        .vertical_align(egui::Align::Center)
                        .desired_width(inner.width())
                        .show(ui)
                })
                .inner;
            if out.response.has_focus() {
                ui.painter().rect_stroke(fr, CornerRadius::same(4), Stroke::new(1.0, pal.accent), StrokeKind::Inside);
                let b = Rect::from_min_max(Pos2::new(fr.left() + 1.0, fr.bottom() - 2.0), Pos2::new(fr.right() - 1.0, fr.bottom()));
                ui.painter().rect_filled(b, CornerRadius::ZERO, pal.accent);
            }
            if std::mem::take(&mut p.focus_name) {
                out.response.request_focus();
            }
        } else {
            ui.label(egui::RichText::new(&title).font(font(14.0)).color(pal.text));
        }
    });
    ui.add_space(12.0);
    divider(ui, &pal);

    // Тип и расположение
    let any_dir = p.items.iter().any(|i| i.is_dir);
    match &single {
        Some(i) if i.is_dir => row(ui, &pal, tr("Тип:"), &fs::type_name("", true)),
        Some(i) => {
            let ext = fs::ext_of(&i.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
            let t = fs::type_name(&ext, false);
            let t = if ext.is_empty() { t } else { format!("{t} (.{ext})") };
            row(ui, &pal, tr("Тип файла:"), &t);
            if let Some(app_name) = p.opens_with.clone() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    label_cell(ui, &pal, tr("Приложение:"), 32.0, true);
                    ui.add(egui::Label::new(egui::RichText::new(&app_name).font(font(14.0)).color(pal.text)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if button(ui, tr("Изменить..."), false, &pal, 96.0).clicked() {
                            crate::shell::menu::open_with(&i.path.to_string_lossy());
                            p.opens_with = sp::opens_with(&ext);
                        }
                    });
                });
                ui.add_space(8.0);
            }
        }
        None => {
            let types: Vec<String> = p
                .items
                .iter()
                .map(|i| if i.is_dir { fs::type_name("", true) } else { fs::type_name(&fs::ext_of(&i.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), false) })
                .collect();
            let t = if types.iter().all(|t| t == &types[0]) { trf("Все типа {}", &[&types[0]]) } else { tr("Несколько типов").to_string() };
            row(ui, &pal, tr("Тип:"), &t);
        }
    }
    let parents: Vec<Option<PathBuf>> = p.items.iter().map(|i| i.path.parent().map(|x| x.to_path_buf())).collect();
    let loc = match parents.first().cloned().flatten() {
        Some(par) if parents.iter().all(|x| x.as_ref() == Some(&par)) => {
            let s = par.to_string_lossy().to_string();
            if single.is_some() {
                s
            } else {
                trf("Все в {}", &[&s])
            }
        }
        _ => tr("Различные папки").to_string(),
    };
    row(ui, &pal, tr("Расположение:"), &loc);

    // Размеры: сразу из индекса, точное значение - когда закончится подсчёт
    let (size, alloc, files, dirs, counting) = p.totals();
    row(ui, &pal, tr("Размер:"), &size_text(size));
    row(ui, &pal, tr("На диске:"), &size_text(alloc));
    if any_dir || single.is_none() {
        let c = trf("Файлов: {}; папок: {}", &[&group_digits(files), &group_digits(dirs)]);
        row(ui, &pal, tr("Содержит:"), &c);
    }
    if counting {
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }
    divider(ui, &pal);

    // Даты
    if let Some(i) = &single {
        row(ui, &pal, tr("Создан:"), &format_filetime_long(i.created));
        if !i.is_dir {
            row(ui, &pal, tr("Изменён:"), &format_filetime_long(i.modified));
            row(ui, &pal, tr("Открыт:"), &format_filetime_long(i.accessed));
        }
        divider(ui, &pal);
    }

    // Атрибуты
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        label_cell(ui, &pal, tr("Атрибуты:"), 32.0, true);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let ro_label = if any_dir { tr("Только для чтения\n(применимо только к файлам в папке)") } else { tr("Только для чтения") };
            tri_checkbox(ui, &mut p.readonly, ro_label, &pal);
            ui.horizontal(|ui| {
                tri_checkbox(ui, &mut p.hidden, tr("Скрытый"), &pal);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if button(ui, tr("Другие..."), false, &pal, 96.0).clicked() {
                        match &single {
                            Some(i) => sp::system_properties(&i.path, None),
                            None => {
                                let names: Vec<String> = p.items.iter().map(|i| i.path.to_string_lossy().to_string()).collect();
                                crate::shell::menu::invoke_verb(&names, "properties");
                            }
                        }
                    }
                });
            });
        });
    });
    // Для папок, как в Проводнике: только к ней или ко всему содержимому
    if any_dir && (p.readonly != p.orig_readonly || p.hidden != p.orig_hidden) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(LABEL_W);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if radio(ui, !p.nested, tr("Только к этой папке"), &pal).clicked() {
                    p.nested = false;
                }
                if radio(ui, p.nested, tr("К этой папке и ко всем вложенным папкам и файлам"), &pal).clicked() {
                    p.nested = true;
                }
            });
        });
    }
    ui.add_space(24.0);

    // ОК / Отмена / Применить
    let changed = p.changed();
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let ap = ui.add_enabled_ui(changed, |ui| button(ui, tr("Применить"), false, &pal, 96.0)).inner;
        if ap.clicked() && changed {
            p.apply(app, &ctx);
        }
        ui.add_space(8.0);
        if button(ui, tr("Отмена"), false, &pal, 96.0).clicked() {
            *keep = false;
        }
        ui.add_space(8.0);
        if button(ui, tr("ОК"), true, &pal, 96.0).clicked() || enter {
            if !changed || p.apply(app, &ctx) {
                *keep = false;
            }
        }
    });
    let _ = l;
}
