//! Состояние приложения, вкладки, навигация, фоновые задачи и выполнение команд

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use egui::{Pos2, Rect};

use crate::cmd::{Cmd, PaneMode, SortCol, ViewMode};
use crate::search::{Query, SearchEngine};
use crate::settings::{FolderView, OpenTo, Settings};
use crate::shell::fs::{self, Drive, Entry, ListOptions};
use crate::shell::ops::{NewItem, Op, Ops};
use crate::shell::shellfolder;
use crate::shell::sysicons::IconCache;
use crate::shell::{clipboard, menu as smenu, recycle, window};
use crate::theme::{self, Palette, ThemeChoice};
use crate::ui;
use crate::ui::menu::{MenuItem, Popup, PopupResult, TopButton};
use crate::ui::widgets::*;

#[derive(Clone, PartialEq, Debug)]
pub enum Location {
    Home,
    ThisPc,
    Path(PathBuf),
    Shell(String),
}

impl Location {
    pub fn key(&self) -> String {
        match self {
            Location::Home => "::home".into(),
            Location::ThisPc => "::thispc".into(),
            Location::Path(p) => crate::util::lower(&p.to_string_lossy()),
            Location::Shell(s) => crate::util::lower(s),
        }
    }
    pub fn path(&self) -> Option<&Path> {
        match self {
            Location::Path(p) => Some(p),
            _ => None,
        }
    }
    pub fn arg(&self) -> String {
        match self {
            Location::Home => "::home".into(),
            Location::ThisPc => "::thispc".into(),
            Location::Path(p) => p.to_string_lossy().to_string(),
            Location::Shell(s) => s.clone(),
        }
    }
    pub fn from_arg(s: &str) -> Location {
        match s {
            "::home" => Location::Home,
            "::thispc" => Location::ThisPc,
            _ if s.starts_with("::") || s.to_lowercase().starts_with("shell:") => Location::Shell(s.to_string()),
            _ => Location::Path(PathBuf::from(s)),
        }
    }
    pub fn is_listing(&self) -> bool {
        matches!(self, Location::Path(_) | Location::Shell(_))
    }
}

pub struct Rename {
    pub idx: usize,
    pub text: String,
    pub init: bool,
    pub error: Option<String>,
}

pub struct Marquee {
    pub start: Pos2,
    pub cur: Pos2,
    pub base: Vec<bool>,
}

pub struct SearchState {
    pub query: String,
    pub gen: u64,
    pub running: bool,
    pub cancel: Arc<AtomicBool>,
    pub seen: HashSet<String>,
}

pub struct Tab {
    pub id: u64,
    pub loc: Location,
    pub back: Vec<Location>,
    pub fwd: Vec<Location>,
    pub title: String,
    pub entries: Vec<Entry>,
    pub order: Vec<usize>,
    pub sel: Vec<bool>,
    pub focus: Option<usize>,
    pub anchor: Option<usize>,
    pub loading: bool,
    pub load_started: f64,
    pub gen: u64,
    pub error: Option<String>,
    pub view: FolderView,
    pub rename: Option<Rename>,
    pub search_text: String,
    pub search_edit_at: Option<f64>,
    pub search: Option<SearchState>,
    pub address_edit: Option<String>,
    pub address_focus: bool,
    pub scroll_to_focus: bool,
    pub reset_scroll: bool,
    pub reselect: Option<Vec<PathBuf>>,
    pub rename_after_load: Option<PathBuf>,
    pub dirty_at: Option<f64>,
    pub marquee: Option<Marquee>,
    pub type_ahead: (String, f64),
    pub watcher: Option<notify::RecommendedWatcher>,
    pub press_item: Option<usize>,
    /// Щелчок по имени уже выделенного элемента: если за время двойного щелчка второго не было,
    /// начинается переименование, как в Проводнике. (элемент, время щелчка)
    pub rename_click: Option<(usize, f64)>,
    pub grid_cols: usize,
    pub list_rows: usize,
    pub page_items: usize,
}

impl Tab {
    fn new(id: u64, loc: Location) -> Self {
        Tab {
            id,
            loc,
            back: vec![],
            fwd: vec![],
            title: String::new(),
            entries: vec![],
            order: vec![],
            sel: vec![],
            focus: None,
            anchor: None,
            loading: false,
            load_started: 0.0,
            gen: 0,
            error: None,
            view: FolderView::default(),
            rename: None,
            search_text: String::new(),
            search_edit_at: None,
            search: None,
            address_edit: None,
            address_focus: false,
            scroll_to_focus: false,
            reset_scroll: false,
            reselect: None,
            rename_after_load: None,
            dirty_at: None,
            marquee: None,
            type_ahead: (String::new(), 0.0),
            watcher: None,
            press_item: None,
            rename_click: None,
            grid_cols: 1,
            list_rows: 1,
            page_items: 10,
        }
    }

    pub fn selected(&self) -> Vec<usize> {
        self.order.iter().copied().filter(|&i| self.sel.get(i).copied().unwrap_or(false)).collect()
    }
    pub fn selected_entries(&self) -> Vec<&Entry> {
        self.selected().into_iter().map(|i| &self.entries[i]).collect()
    }
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_entries().iter().map(|e| e.path.clone()).collect()
    }
    pub fn selected_names(&self) -> Vec<String> {
        self.selected_entries().iter().map(|e| e.shell.clone().unwrap_or_else(|| e.path.to_string_lossy().to_string())).collect()
    }
    pub fn sel_count(&self) -> usize {
        self.sel.iter().filter(|&&b| b).count()
    }
    pub fn clear_sel(&mut self) {
        self.sel.iter_mut().for_each(|b| *b = false);
    }
    pub fn select_only(&mut self, entry_idx: usize) {
        self.clear_sel();
        if let Some(b) = self.sel.get_mut(entry_idx) {
            *b = true;
        }
        self.focus = self.order.iter().position(|&i| i == entry_idx);
        self.anchor = self.focus;
    }
    pub fn is_bin(&self) -> bool {
        matches!(&self.loc, Location::Shell(s) if s.eq_ignore_ascii_case(shellfolder::RECYCLE_BIN))
    }
    pub fn is_virtual(&self) -> bool {
        matches!(self.loc, Location::Shell(_))
    }
    pub fn fs_dir(&self) -> Option<PathBuf> {
        match &self.loc {
            Location::Path(p) => Some(p.clone()),
            Location::Shell(s) => {
                let p = Path::new(s);
                if p.is_absolute() && fs::is_dir(p) {
                    Some(p.to_path_buf())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    pub fn resort(&mut self) {
        let col = self.view.sort;
        let desc = self.view.desc;
        let e = &self.entries;
        let mut order: Vec<usize> = (0..e.len()).collect();
        // Как в Проводнике: папки всегда выше файлов, в любом столбце и в обе стороны,
        // Направление меняет только порядок внутри папок и внутри файлов
        let folders_first = !matches!(col, SortCol::Location | SortCol::Extra);
        order.sort_by(|&a, &b| {
            let (x, y) = (&e[a], &e[b]);
            if folders_first && x.is_dir != y.is_dir {
                return y.is_dir.cmp(&x.is_dir);
            }
            let o = match col {
                SortCol::Name => crate::util::natural_cmp(&x.display, &y.display),
                SortCol::Modified => x.mtime.cmp(&y.mtime),
                SortCol::Type => {
                    let tx = x.type_name.clone().unwrap_or_else(|| fs::type_name(&x.ext, x.is_dir));
                    let ty = y.type_name.clone().unwrap_or_else(|| fs::type_name(&y.ext, y.is_dir));
                    crate::util::natural_cmp(&tx, &ty)
                }
                SortCol::Size => x.size.cmp(&y.size),
                SortCol::Location => crate::util::natural_cmp(x.location.as_deref().unwrap_or(""), y.location.as_deref().unwrap_or("")),
                SortCol::Extra => x.ctime.cmp(&y.ctime),
            };
            let o = o.then_with(|| crate::util::natural_cmp(&x.display, &y.display));
            if desc {
                o.reverse()
            } else {
                o
            }
        });
        let focused = self.focus.and_then(|f| self.order.get(f).copied());
        self.order = order;
        self.focus = focused.and_then(|fi| self.order.iter().position(|&i| i == fi));
        self.anchor = self.focus;
    }
}

pub enum Msg {
    Listing { tab: u64, gen: u64, res: Result<Vec<Entry>, String> },
    SearchBatch { tab: u64, gen: u64, items: Vec<Entry> },
    SearchDone { tab: u64, gen: u64 },
    FsChanged { tab: u64 },
    Drives(Vec<Drive>),
    Quick(Vec<(String, PathBuf, bool)>),
    Recent(Vec<Entry>),
    Toast(String),
    RefreshAll,
}

pub enum Dialog {
    Settings,
    Message { title: String, text: String },
    About,
    /// Выбор языка при первом запуске
    Language,
    /// Предложение установить новую версию
    Update(crate::update::Release),
    /// Вопрос «Да / Нет», как в Проводнике. «Да» выполняет команду
    Confirm { title: String, text: String, cmd: Cmd },
    /// Окно «Свойства» Oxide
    Properties(Box<crate::ui::props::Props>),
}

pub enum UndoRec {
    Rename { from: PathBuf, to: PathBuf },
    Move { items: Vec<(PathBuf, PathBuf)> },
    Created { paths: Vec<PathBuf> },
    /// Удалённые в корзину: отмена восстанавливает их
    Recycled { paths: Vec<PathBuf> },
}

#[derive(Default, Clone)]
pub struct DropTargets {
    pub items: Vec<(Rect, PathBuf)>,
    pub areas: Vec<(Rect, PathBuf)>,
}

pub struct NavState {
    pub expanded: HashSet<String>,
    pub children: HashMap<String, Vec<(String, PathBuf)>>,
}

pub struct OxideApp {
    pub settings: Settings,
    pub updater: crate::update::Updater,
    pub pal: Palette,
    pub system_dark: bool,
    pub accent: Option<[egui::Color32; 8]>,
    pub animations: bool,
    pub tabs: Vec<Tab>,
    pub active: usize,
    next_id: u64,
    pub icons: IconCache,
    pub ops: Ops,
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub engine: Arc<SearchEngine>,
    pub drives: Vec<Drive>,
    drive_mask: u32,
    drive_check: f64,
    /// Число и объём элементов корзины при последней проверке: открытая корзина обновляется сама
    bin_seen: (u64, u64),
    bin_check: f64,
    pub quick: Vec<(String, PathBuf, bool)>,
    pub recent: Vec<Entry>,
    pub nav: NavState,
    pub popup: Option<Popup>,
    pub popup_seq: u64,
    pub dialog: Option<Dialog>,
    pub toast: Option<(String, f64)>,
    pub undo: Vec<UndoRec>,
    pub new_items: Option<Vec<NewItem>>,
    pub hwnd_ok: bool,
    last_title: String,
    pub kb_focus: bool,
    pub cut_set: HashSet<PathBuf>,
    pub settings_dirty: Option<f64>,
    pub drop_targets: DropTargets,
    /// Файлы, которые начали перетаскивать, и правой ли кнопкой
    pub pending_drag: Option<(Vec<PathBuf>, bool)>,
    ignore_drops_until: f64,
    pub drag_hover: Option<PathBuf>,
    pub content_rect: Rect,
    pub tab_drag: Option<(usize, f32)>,
    pub search_focus: bool,
    last_dark_applied: Option<bool>,
    pub nav_scroll_to: bool,
    pub fs_dirty_all: bool,
    pub last_menu_pos: Option<Pos2>,
    pub undo_pop_after: bool,
}

pub fn now(ctx: &egui::Context) -> f64 {
    ctx.input(|i| i.time)
}

impl OxideApp {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings, start: Option<String>) -> Self {
        let ctx = cc.egui_ctx.clone();
        crate::fonts::install(&ctx);
        ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
        });
        let system_dark = theme::system_is_dark();
        let accent = theme::system_accent_palette();
        let animations = theme::system_animations_enabled();
        let mut pal = Palette::resolve(settings.theme, system_dark, accent).tune_glass(settings.glass_clarity, settings.glass_shine);

        let mut hwnd_ok = false;
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = cc.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    let hwnd = windows::Win32::Foundation::HWND(w.hwnd.get() as *mut _);
                    window::install(hwnd, &ctx);
                    window::set_dark(hwnd, pal.dark);
                    if !window::set_glass(hwnd, pal.glass, settings.glass_blur) {
                        pal = pal.solid();
                    }
                    hwnd_ok = true;
                }
            }
        }
        if !hwnd_ok {
            pal = pal.solid();
        }
        theme::apply(&ctx, &pal, animations);

        let (tx, rx) = channel();
        let engine = Arc::new(SearchEngine::new());
        if settings.index_enabled {
            engine.start(ctx.clone());
        }
        let mut app = OxideApp {
            icons: IconCache::new(&ctx),
            ops: Ops::new(ctx.clone()),
            pal,
            system_dark,
            accent,
            animations,
            tabs: vec![],
            active: 0,
            next_id: 1,
            tx,
            rx,
            engine,
            drives: vec![],
            drive_mask: 0,
            drive_check: 0.0,
            bin_seen: (u64::MAX, 0),
            bin_check: 0.0,
            quick: vec![],
            recent: vec![],
            nav: NavState { expanded: settings.nav_expanded.iter().cloned().collect(), children: HashMap::new() },
            popup: None,
            popup_seq: 1,
            dialog: None,
            toast: None,
            undo: vec![],
            new_items: None,
            hwnd_ok,
            last_title: String::new(),
            kb_focus: false,
            cut_set: HashSet::new(),
            settings_dirty: None,
            drop_targets: DropTargets::default(),
            pending_drag: None,
            ignore_drops_until: 0.0,
            drag_hover: None,
            content_rect: Rect::NOTHING,
            tab_drag: None,
            search_focus: false,
            last_dark_applied: None,
            nav_scroll_to: false,
            fs_dirty_all: false,
            last_menu_pos: None,
            undo_pop_after: false,
            updater: crate::update::Updater::new(),
            settings,
        };
        app.refresh_drives(&ctx);
        app.load_quick(&ctx);

        // Стартовое расположение
        let mut select_file: Option<PathBuf> = None;
        // Перезапуск после обновления: вернуть все вкладки, как были
        let after_update = start.as_deref() == Some(crate::update::AFTER_UPDATE_ARG);
        let start = if after_update { None } else { start };
        let loc = match start {
            Some(a) if !a.is_empty() => {
                let mut a = a.trim_matches('"').to_string();
                // Корень диска приходит как «D:» (кавычка после «D:\» съедает обратную косую черту)
                if a.len() == 2 && a.ends_with(':') {
                    a.push(std::path::MAIN_SEPARATOR);
                }
                let p = PathBuf::from(&a);
                if a.starts_with("::") || a.to_lowercase().starts_with("shell:") {
                    Location::from_arg(&a)
                } else if fs::is_dir(&p) {
                    Location::Path(p)
                } else if fs::exists(&p) {
                    select_file = Some(p.clone());
                    Location::Path(p.parent().map(|x| x.to_path_buf()).unwrap_or(p))
                } else {
                    app.default_location()
                }
            }
            _ => app.default_location(),
        };
        if (app.settings.restore_tabs || after_update) && select_file.is_none() && !app.settings.last_tabs.is_empty() {
            let tabs = app.settings.last_tabs.clone();
            // Каждую следующую вкладку - правее предыдущей, порядок как был
            for t in tabs {
                app.open_tab(&ctx, Location::from_arg(&t), false);
                app.active = app.tabs.len() - 1;
            }
            app.active = 0;
        } else {
            app.open_tab(&ctx, loc, true);
        }
        if app.settings.language.is_none() {
            app.dialog = Some(Dialog::Language);
        } else if app.settings.show_about_on_start && !after_update {
            app.dialog = Some(Dialog::About);
        }
        if after_update {
            let v = env!("CARGO_PKG_VERSION");
            let text = if crate::i18n::is_en() { format!("Oxide has been updated to version {v}") } else { format!("Oxide обновлён до версии {v}") };
            app.toast = Some((text, now(&ctx)));
        }
        if let Some(f) = select_file {
            app.tabs[app.active].reselect = Some(vec![f]);
        }
        app
    }

    pub fn default_location(&self) -> Location {
        match self.settings.open_to {
            OpenTo::Home => Location::Home,
            OpenTo::ThisPc => Location::ThisPc,
        }
    }

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }
    pub fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn list_opts(&self) -> ListOptions {
        ListOptions { show_hidden: self.settings.show_hidden, show_super_hidden: self.settings.show_super_hidden, show_ext: self.settings.show_ext }
    }

    pub fn mark_settings(&mut self, ctx: &egui::Context) {
        self.settings_dirty = Some(now(ctx));
    }

    /// Сменить язык интерфейса сразу, без перезапуска
    pub fn set_language(&mut self, ctx: &egui::Context, lang: crate::i18n::Lang) {
        self.settings.language = Some(lang);
        crate::i18n::set(lang);
        for t in &mut self.tabs {
            t.title = Self::title_of(&t.loc);
        }
        self.refresh_drives(ctx);
        self.mark_settings(ctx);
        ctx.request_repaint();
    }

    /// Обновление скачано и установлено: сохранить всё и запустить новую версию
    fn finish_update(&mut self, ctx: &egui::Context) {
        self.save_window(ctx);
        self.settings.save();
        self.settings_dirty = None;
        crate::update::restart();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // ---------- Вкладки ----------

    pub fn open_tab(&mut self, ctx: &egui::Context, loc: Location, activate: bool) {
        let id = self.next_id;
        self.next_id += 1;
        let t = Tab::new(id, Location::Home);
        let at = if self.tabs.is_empty() { 0 } else { self.active + 1 };
        let at = at.min(self.tabs.len());
        self.tabs.insert(at, t);
        if activate || self.tabs.len() == 1 {
            self.active = at;
        } else if at <= self.active && self.tabs.len() > 1 {
            // активная осталась прежней
        }
        self.navigate_tab(ctx, at, loc, false);
    }

    pub fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        if self.tabs.len() <= 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if let Some(s) = &self.tabs[i].search {
            s.cancel.store(true, Ordering::Relaxed);
        }
        self.tabs.remove(i);
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    // ---------- Навигация ----------

    pub fn navigate(&mut self, ctx: &egui::Context, loc: Location) {
        let a = self.active;
        self.navigate_tab(ctx, a, loc, true);
    }

    pub fn navigate_tab(&mut self, ctx: &egui::Context, ti: usize, loc: Location, push: bool) {
        // Ярлык на папку → сама папка; путь к файлу → открыть папку и выделить файл
        let mut loc = loc;
        let mut select: Option<PathBuf> = None;
        if let Location::Path(p) = &loc {
            if !fs::is_dir(p) && fs::exists(p) {
                select = Some(p.clone());
                loc = Location::Path(p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| p.clone()));
            }
        }
        let key = loc.key();
        let view = self.settings.view_for(&key);
        let t = &mut self.tabs[ti];
        if let Some(s) = t.search.take() {
            s.cancel.store(true, Ordering::Relaxed);
        }
        if push && t.loc != loc {
            let old = std::mem::replace(&mut t.loc, loc.clone());
            if t.back.last() != Some(&old) {
                t.back.push(old);
            }
            if t.back.len() > 100 {
                t.back.remove(0);
            }
            t.fwd.clear();
        } else {
            t.loc = loc.clone();
        }
        t.view = view;
        t.search_text.clear();
        t.search_edit_at = None;
        t.address_edit = None;
        t.rename = None;
        t.marquee = None;
        t.reset_scroll = true;
        t.reselect = select.map(|s| vec![s]);
        t.error = None;
        t.watcher = None;
        t.title = Self::title_of(&loc);
        self.load_tab(ctx, ti);
        self.nav_scroll_to = true;
        if let Location::Path(p) = &loc {
            self.watch_tab(ctx, ti, p.clone());
        }
    }

    pub fn title_of(loc: &Location) -> String {
        match loc {
            Location::Home => tr("Главная").into(),
            Location::ThisPc => tr("Этот компьютер").into(),
            Location::Path(p) => {
                if p.parent().is_none() || p.to_string_lossy().ends_with(":\\") || p.to_string_lossy().len() <= 3 {
                    fs::shell_display_name(p).unwrap_or_else(|| p.to_string_lossy().to_string())
                } else {
                    let custom = p.join("desktop.ini").exists();
                    if custom {
                        if let Some(d) = fs::shell_display_name(p) {
                            return d;
                        }
                    }
                    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string_lossy().to_string())
                }
            }
            Location::Shell(s) => shellfolder::display_name_of(s).unwrap_or_else(|| s.clone()),
        }
    }

    fn watch_tab(&mut self, ctx: &egui::Context, ti: usize, p: PathBuf) {
        use notify::{RecursiveMode, Watcher};
        let tx = self.tx.clone();
        let id = self.tabs[ti].id;
        let c = ctx.clone();
        let w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if !matches!(ev.kind, notify::EventKind::Access(_)) {
                    let _ = tx.send(Msg::FsChanged { tab: id });
                    c.request_repaint_after(std::time::Duration::from_millis(300));
                }
            }
        });
        if let Ok(mut w) = w {
            if w.watch(&p, RecursiveMode::NonRecursive).is_ok() {
                self.tabs[ti].watcher = Some(w);
            }
        }
    }

    pub fn load_tab(&mut self, ctx: &egui::Context, ti: usize) {
        let opts = self.list_opts();
        let t = &mut self.tabs[ti];
        t.gen += 1;
        let gen = t.gen;
        let id = t.id;
        t.loading = t.loc.is_listing();
        t.load_started = now(ctx);
        if !t.loading {
            t.entries.clear();
            t.order.clear();
            t.sel.clear();
            return;
        }
        let loc = t.loc.clone();
        let tx = self.tx.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            }
            let res = match &loc {
                Location::Path(p) => fs::list_dir(p, &opts),
                Location::Shell(s) => shellfolder::enumerate(s, opts.show_ext),
                _ => Ok(vec![]),
            };
            let _ = tx.send(Msg::Listing { tab: id, gen, res });
            c.request_repaint();
        });
    }

    pub fn reload_keep(&mut self, ctx: &egui::Context, ti: usize) {
        let t = &mut self.tabs[ti];
        if t.searching() {
            return;
        }
        if t.reselect.is_none() {
            t.reselect = Some(t.selected_paths());
        }
        self.load_tab(ctx, ti);
    }

    pub fn go_back(&mut self, ctx: &egui::Context) {
        let a = self.active;
        if self.tabs[a].searching() {
            self.end_search(ctx, a);
            return;
        }
        if let Some(prev) = self.tabs[a].back.pop() {
            let cur = self.tabs[a].loc.clone();
            self.tabs[a].fwd.push(cur.clone());
            let came_from = cur.path().map(|p| p.to_path_buf());
            self.navigate_tab(ctx, a, prev, false);
            if let Some(cf) = came_from {
                self.tabs[a].reselect = Some(vec![cf]);
            }
        }
    }

    pub fn go_forward(&mut self, ctx: &egui::Context) {
        let a = self.active;
        if let Some(next) = self.tabs[a].fwd.pop() {
            let cur = self.tabs[a].loc.clone();
            self.tabs[a].back.push(cur);
            self.navigate_tab(ctx, a, next, false);
        }
    }

    pub fn parent_of(loc: &Location) -> Option<Location> {
        match loc {
            Location::Path(p) => match p.parent() {
                Some(pp) => Some(Location::Path(pp.to_path_buf())),
                None => Some(Location::ThisPc),
            },
            Location::Shell(s) => {
                let p = Path::new(s);
                if p.is_absolute() {
                    p.parent().map(|x| Location::Path(x.to_path_buf()))
                } else {
                    Some(Location::Home)
                }
            }
            Location::ThisPc => Some(Location::Home),
            Location::Home => None,
        }
    }

    pub fn go_up(&mut self, ctx: &egui::Context) {
        let cur = self.tab().loc.clone();
        if let Some(p) = Self::parent_of(&cur) {
            self.navigate(ctx, p);
            if let Location::Path(c) = cur {
                self.tab_mut().reselect = Some(vec![c]);
            }
        }
    }

    // ---------- Фоновые данные ----------

    pub fn refresh_drives(&mut self, ctx: &egui::Context) {
        self.drive_mask = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
        let tx = self.tx.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            }
            let d = fs::drives(true);
            let _ = tx.send(Msg::Drives(d));
            c.request_repaint();
        });
    }

    pub fn load_quick(&mut self, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let c = ctx.clone();
        let opts = self.list_opts();
        let want_recent = self.settings.show_recent;
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            }
            let mut q = shellfolder::quick_access();
            if q.is_empty() {
                use crate::shell::fs::kf;
                for p in [kf::desktop(), kf::downloads(), kf::documents(), kf::pictures(), kf::music(), kf::videos()].into_iter().flatten() {
                    let n = fs::shell_display_name(&p).unwrap_or_else(|| p.to_string_lossy().to_string());
                    q.push((n, p, true));
                }
            }
            let _ = tx.send(Msg::Quick(q));
            c.request_repaint();
            if want_recent {
                let r = shellfolder::recent_files(40);
                let mut ents = vec![];
                for (p, t) in r {
                    if let Some(mut e) = fs::stat_entry(&p, &opts) {
                        e.ctime = t;
                        ents.push(e);
                    }
                }
                let _ = tx.send(Msg::Recent(ents));
                c.request_repaint();
            }
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let t_now = now(ctx);
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Listing { tab, gen, res } => {
                    let Some(ti) = self.tabs.iter().position(|t| t.id == tab) else { continue };
                    let t = &mut self.tabs[ti];
                    if t.gen != gen || t.searching() {
                        continue;
                    }
                    t.loading = false;
                    match res {
                        Ok(entries) => {
                            let keep = t.reselect.take().unwrap_or_default();
                            t.error = None;
                            t.sel = vec![false; entries.len()];
                            t.entries = entries;
                            t.resort();
                            t.focus = None;
                            if !keep.is_empty() {
                                let set: HashSet<&PathBuf> = keep.iter().collect();
                                let mut first = None;
                                for (pos, &i) in t.order.iter().enumerate() {
                                    if set.contains(&t.entries[i].path) {
                                        t.sel[i] = true;
                                        if first.is_none() {
                                            first = Some(pos);
                                        }
                                    }
                                }
                                t.focus = first;
                                t.anchor = first;
                                t.scroll_to_focus = first.is_some();
                            }
                            if let Some(rp) = t.rename_after_load.take() {
                                if let Some(i) = t.entries.iter().position(|e| e.path == rp) {
                                    t.select_only(i);
                                    t.scroll_to_focus = true;
                                    let text = t.entries[i].name.clone();
                                    t.rename = Some(Rename { idx: i, text, init: true, error: None });
                                }
                            }
                        }
                        Err(e) => {
                            t.error = Some(e);
                            t.entries.clear();
                            t.order.clear();
                            t.sel.clear();
                        }
                    }
                }
                Msg::SearchBatch { tab, gen, items } => {
                    let Some(ti) = self.tabs.iter().position(|t| t.id == tab) else { continue };
                    let t = &mut self.tabs[ti];
                    let Some(s) = &mut t.search else { continue };
                    if s.gen != gen {
                        continue;
                    }
                    for e in items {
                        if s.seen.insert(e.path.to_string_lossy().to_string()) {
                            t.entries.push(e);
                            t.sel.push(false);
                        }
                    }
                    t.resort();
                }
                Msg::SearchDone { tab, gen } => {
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) {
                        if let Some(s) = &mut t.search {
                            if s.gen == gen {
                                s.running = false;
                            }
                        }
                        t.loading = false;
                    }
                }
                Msg::FsChanged { tab } => {
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) {
                        t.dirty_at = Some(t_now);
                    }
                }
                Msg::Drives(d) => self.drives = d,
                Msg::Quick(q) => self.quick = q,
                Msg::Recent(r) => self.recent = r,
                Msg::Toast(s) => self.toast = Some((s, t_now)),
                Msg::RefreshAll => self.fs_dirty_all = true,
            }
        }
        // Отложенное обновление после изменений на диске
        for ti in 0..self.tabs.len() {
            let due = self.tabs[ti].dirty_at.map(|d| t_now - d > 0.25).unwrap_or(false) || self.fs_dirty_all;
            if due && self.tabs[ti].rename.is_none() && self.tabs[ti].marquee.is_none() && !self.tabs[ti].searching() {
                self.tabs[ti].dirty_at = None;
                if self.tabs[ti].loc.is_listing() {
                    self.reload_keep(ctx, ti);
                }
            } else if self.tabs[ti].dirty_at.is_some() {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
        }
        self.fs_dirty_all = false;
        // Открытая корзина следит за изменениями сама: удаление в Проводнике или в другом окне сразу видно
        if self.tabs.iter().any(|t| t.is_bin()) && t_now - self.bin_check > 1.0 {
            self.bin_check = t_now;
            let info = recycle::info();
            if info != self.bin_seen {
                if self.bin_seen.0 != u64::MAX {
                    for t in self.tabs.iter_mut().filter(|t| t.is_bin()) {
                        t.dirty_at = Some(t_now - 1.0);
                    }
                }
                self.bin_seen = info;
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        // Подключение/отключение дисков
        if t_now - self.drive_check > 2.0 {
            self.drive_check = t_now;
            let m = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
            if m != self.drive_mask {
                self.refresh_drives(ctx);
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
        // Завершённые файловые операции
        while let Ok(done) = self.ops.rx.try_recv() {
            if let Some(err) = done.error {
                self.dialog = Some(Dialog::Message { title: tr("Не удалось выполнить операцию").into(), text: err });
            } else if done.ok && std::mem::take(&mut self.undo_pop_after) {
                // Это была отмена - новую запись отмены не создаём
            } else if done.ok {
                match &done.op {
                    Op::Rename { path, new_name } => {
                        let to = path.with_file_name(new_name);
                        self.undo.push(UndoRec::Rename { from: path.clone(), to: to.clone() });
                        let a = self.active;
                        self.tabs[a].reselect = Some(vec![to]);
                    }
                    Op::Move { src, dst } => {
                        let items: Vec<(PathBuf, PathBuf)> = src
                            .iter()
                            .filter(|s| s.parent() != Some(dst.as_path()))
                            .map(|s| (s.clone(), dst.join(s.file_name().unwrap_or_default())))
                            .collect();
                        if !items.is_empty() {
                            self.undo.push(UndoRec::Move { items });
                        }
                    }
                    Op::Copy { src, dst } => {
                        if !src.iter().any(|s| s.parent() == Some(dst.as_path())) {
                            let created: Vec<PathBuf> = src.iter().map(|s| dst.join(s.file_name().unwrap_or_default())).collect();
                            self.undo.push(UndoRec::Created { paths: created });
                        }
                    }
                    Op::Delete { paths, permanent: false } => self.undo.push(UndoRec::Recycled { paths: paths.clone() }),
                    Op::Delete { .. } => {}
                }
            }
            self.fs_dirty_all = true;
            self.load_quick(ctx);
            if let Some(dir) = self.tab().fs_dir() {
                if dir.parent().is_none() || dir.to_string_lossy().len() <= 3 {
                    self.refresh_drives(ctx);
                }
            }
        }
    }

    // ---------- Поиск ----------

    pub fn start_search(&mut self, ctx: &egui::Context, ti: usize) {
        let q_text = self.tabs[ti].search_text.trim().to_string();
        if q_text.is_empty() {
            self.end_search(ctx, ti);
            return;
        }
        let q = Query::parse(&q_text);
        if q.is_empty() {
            return;
        }
        let show_hidden = self.settings.show_hidden;
        let opts = self.list_opts();
        let t = &mut self.tabs[ti];
        if let Some(s) = &t.search {
            s.cancel.store(true, Ordering::Relaxed);
        }
        t.gen += 1;
        let gen = t.gen;
        let cancel = Arc::new(AtomicBool::new(false));
        t.search = Some(SearchState { query: q_text.clone(), gen, running: true, cancel: cancel.clone(), seen: HashSet::new() });
        t.entries.clear();
        t.order.clear();
        t.sel.clear();
        t.focus = None;
        t.error = None;
        t.loading = true;
        t.load_started = now(ctx);
        t.reset_scroll = true;
        let roots: Vec<PathBuf> = match &t.loc {
            Location::Path(p) => vec![p.clone()],
            _ => self.drives.iter().filter(|d| d.kind == fs::DRIVE_FIXED).map(|d| d.root.clone()).collect(),
        };
        let id = t.id;
        let tx = self.tx.clone();
        let engine = self.engine.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            }
            let to_entries = |v: Vec<(String, bool)>| -> Vec<Entry> {
                v.into_iter()
                    .filter_map(|(p, _)| fs::stat_entry(Path::new(&p), &opts))
                    .collect()
            };
            let mut not_indexed = vec![];
            for r in roots {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                match engine.query(&r, &q, show_hidden, 20000) {
                    Some(res) => {
                        for chunk in res.chunks(500) {
                            if cancel.load(Ordering::Relaxed) {
                                break;
                            }
                            let items = to_entries(chunk.to_vec());
                            let _ = tx.send(Msg::SearchBatch { tab: id, gen, items });
                            c.request_repaint();
                        }
                    }
                    None => not_indexed.push(r),
                }
            }
            {
                for r in not_indexed {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let tx2 = tx.clone();
                    let c2 = c.clone();
                    let opts2 = ListOptions { show_hidden: opts.show_hidden, show_super_hidden: opts.show_super_hidden, show_ext: opts.show_ext };
                    crate::search::live_search(r, q.clone(), show_hidden, cancel.clone(), move |batch| {
                        let items: Vec<Entry> = batch.into_iter().filter_map(|(p, _)| fs::stat_entry(Path::new(&p), &opts2)).collect();
                        let _ = tx2.send(Msg::SearchBatch { tab: id, gen, items });
                        c2.request_repaint();
                    });
                }
            }
            let _ = tx.send(Msg::SearchDone { tab: id, gen });
            c.request_repaint();
        });
    }

    pub fn end_search(&mut self, ctx: &egui::Context, ti: usize) {
        let t = &mut self.tabs[ti];
        t.search_text.clear();
        t.search_edit_at = None;
        if let Some(s) = t.search.take() {
            s.cancel.store(true, Ordering::Relaxed);
            t.reset_scroll = true;
            self.load_tab(ctx, ti);
        }
    }

    // ---------- Открытие ----------

    pub fn open_entry(&mut self, ctx: &egui::Context, idx: usize, new_tab: bool) {
        let e = self.tab().entries[idx].clone();
        if self.tab().is_bin() {
            // Удалённые элементы не открываются: Проводник показывает их свойства
            recycle::properties(&[e.path]);
            return;
        }
        if e.is_dir {
            let loc = if let Some(sh) = &e.shell {
                let p = Path::new(&e.path);
                if p.is_absolute() && fs::is_dir(p) {
                    Location::Path(p.to_path_buf())
                } else {
                    Location::Shell(sh.clone())
                }
            } else {
                Location::Path(e.path.clone())
            };
            if new_tab {
                self.open_tab(ctx, loc, false);
            } else {
                self.navigate(ctx, loc);
            }
            return;
        }
        if e.ext == "lnk" {
            if let Some(t) = shellfolder::shortcut_target(&e.path) {
                if fs::is_dir(&t) {
                    if new_tab {
                        self.open_tab(ctx, Location::Path(t), false);
                    } else {
                        self.navigate(ctx, Location::Path(t));
                    }
                    return;
                }
            }
        }
        if e.ext == "zip" && e.shell.is_none() {
            self.navigate(ctx, Location::Shell(e.path.to_string_lossy().to_string()));
            return;
        }
        let target = e.shell.clone().unwrap_or_else(|| e.path.to_string_lossy().to_string());
        if !smenu::open(&target) {
            self.toast = Some((trf("Не удаётся открыть «{}»", &[&e.display]), now(ctx)));
        }
    }

    pub fn open_selection(&mut self, ctx: &egui::Context) {
        let sel = self.tab().selected();
        if sel.len() == 1 {
            self.open_entry(ctx, sel[0], false);
        } else {
            for i in sel {
                let e = &self.tab().entries[i];
                if e.is_dir {
                    let loc = Location::Path(e.path.clone());
                    self.open_tab(ctx, loc, false);
                } else {
                    let target = e.shell.clone().unwrap_or_else(|| e.path.to_string_lossy().to_string());
                    smenu::open(&target);
                }
            }
        }
    }

    // ---------- Команды ----------

    pub fn paste_into(&mut self, ctx: &egui::Context, dir: PathBuf) {
        if let Some((files, cut)) = clipboard::get_files() {
            if cut {
                self.ops.run(Op::Move { src: files.clone(), dst: dir });
                clipboard::clear();
                self.cut_set.clear();
            } else {
                self.ops.run(Op::Copy { src: files, dst: dir });
            }
        } else {
            self.toast = Some((tr("В буфере обмена нет файлов").into(), now(ctx)));
        }
    }

    pub fn start_rename(&mut self) {
        let t = self.tab_mut();
        if t.is_virtual() {
            return;
        }
        let Some(&first) = t.selected().first() else { return };
        let text = t.entries[first].name.clone();
        t.rename = Some(Rename { idx: first, text, init: true, error: None });
        t.focus = t.order.iter().position(|&i| i == first);
        t.scroll_to_focus = true;
    }

    pub fn commit_rename(&mut self, ctx: &egui::Context) {
        let a = self.active;
        let Some(r) = self.tabs[a].rename.take() else { return };
        let Some(e) = self.tabs[a].entries.get(r.idx).cloned() else { return };
        let new = r.text.trim().trim_end_matches('.').to_string();
        if new.is_empty() || new == e.name {
            return;
        }
        if let Some(c) = crate::util::invalid_name_char(&new) {
            let _ = c;
            self.tabs[a].rename = Some(Rename { error: Some(tr("Имя файла не должно содержать следующих знаков: \\ / : * ? \" < > |").into()), ..r });
            return;
        }
        self.ops.run(Op::Rename { path: e.path.clone(), new_name: new.clone() });
        // Мгновенно показываем новое имя, не дожидаясь диска
        let ext = if e.is_dir { String::new() } else { fs::ext_of(&new) };
        let show_ext = self.settings.show_ext;
        let ent = &mut self.tabs[a].entries[r.idx];
        ent.display = fs::display_name_for(&new, e.is_dir, &ext, show_ext);
        ent.name = new.clone();
        ent.ext = ext;
        self.tabs[a].reselect = Some(vec![e.path.with_file_name(&new)]);
        let _ = ctx;
    }

    /// Своё окно «Свойства» для обычных файлов и папок. Диски и виртуальные папки
    /// показывает Windows. Возвращает false, если окно не подходит
    fn open_own_properties(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) -> bool {
        let ok = !paths.is_empty()
            && paths.iter().all(|p| {
                let s = p.to_string_lossy();
                p.is_absolute() && p.parent().is_some() && !s.starts_with(r"\\?\") && fs::exists(p)
            });
        if ok {
            self.dialog = Some(Dialog::Properties(Box::new(crate::ui::props::Props::new(self, ctx, paths))));
        }
        ok
    }

    pub fn delete_selection(&mut self, permanent: bool) {
        let t = self.tab();
        if t.is_bin() {
            // В корзине «Удалить» стирает навсегда, как в Проводнике
            let ents: Vec<Entry> = t.selected_entries().into_iter().cloned().collect();
            if ents.is_empty() {
                return;
            }
            let (title, text) = if ents.len() == 1 {
                let e = &ents[0];
                let (title, mut text) = if e.is_dir {
                    (tr("Удаление папки").to_string(), trf("Вы действительно хотите безвозвратно удалить папку «{}»?", &[&e.display]))
                } else {
                    (tr("Удаление файла").to_string(), trf("Вы действительно хотите безвозвратно удалить файл «{}»?", &[&e.display]))
                };
                // Сведения об элементе, как в окне Проводника
                text.push('\n');
                if let Some(t) = &e.type_name {
                    text.push_str(&format!("\n{} {t}", tr("Тип:")));
                }
                if !e.is_dir {
                    text.push_str(&format!("\n{} {}", tr("Размер:"), crate::util::format_bytes(e.size)));
                }
                if e.ctime != 0 {
                    text.push_str(&format!("\n{} {}", tr("Дата удаления:"), crate::util::format_filetime(e.ctime)));
                }
                if let Some(l) = &e.location {
                    text.push_str(&format!("\n{} {l}", tr("Исходное расположение:")));
                }
                (title, text)
            } else {
                (tr("Удаление нескольких элементов").to_string(), trf("Вы действительно хотите безвозвратно удалить эти элементы ({})?", &[&ents.len()]))
            };
            let paths = ents.into_iter().map(|e| e.path).collect();
            self.dialog = Some(Dialog::Confirm { title, text, cmd: Cmd::DeleteForeverNow(paths) });
            return;
        }
        if t.is_virtual() {
            let names = t.selected_names();
            if !names.is_empty() {
                smenu::invoke_verb(&names, "delete");
            }
            return;
        }
        let paths = t.selected_paths();
        if !paths.is_empty() {
            self.ops.run(Op::Delete { paths, permanent });
        }
    }

    pub fn new_folder(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.tab().fs_dir() else { return };
        if self.tab().is_virtual() {
            return;
        }
        match crate::shell::ops::create_folder(&dir) {
            Ok(p) => {
                self.undo.push(UndoRec::Created { paths: vec![p.clone()] });
                let a = self.active;
                self.tabs[a].rename_after_load = Some(p);
                self.reload_keep(ctx, a);
            }
            Err(e) => self.dialog = Some(Dialog::Message { title: tr("Не удалось создать папку").into(), text: e }),
        }
    }

    pub fn undo_last(&mut self, ctx: &egui::Context) {
        let Some(u) = self.undo.pop() else {
            self.toast = Some((tr("Нечего отменять").into(), now(ctx)));
            return;
        };
        match u {
            UndoRec::Rename { from, to } => {
                let name = from.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.ops.run(Op::Rename { path: to, new_name: name });
                self.undo_pop_after = true;
            }
            UndoRec::Move { items } => {
                let mut by_dir: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
                for (orig, now_p) in items {
                    if let Some(d) = orig.parent() {
                        by_dir.entry(d.to_path_buf()).or_default().push(now_p);
                    }
                }
                for (d, src) in by_dir {
                    self.ops.run(Op::Move { src, dst: d });
                }
                self.undo_pop_after = true;
            }
            UndoRec::Created { paths } => {
                self.ops.run(Op::Delete { paths, permanent: false });
            }
            UndoRec::Recycled { paths } => {
                if recycle::restore_originals(&paths) {
                    self.fs_dirty_all = true;
                } else {
                    self.toast = Some((tr("Не удалось восстановить из корзины").into(), now(ctx)));
                }
            }
        }
    }

    pub fn exec(&mut self, ctx: &egui::Context, cmd: Cmd) {
        let a = self.active;
        match cmd {
            Cmd::Open => self.open_selection(ctx),
            Cmd::OpenNewTab => {
                for i in self.tab().selected() {
                    let e = self.tab().entries[i].clone();
                    if e.is_dir {
                        self.open_tab(ctx, Location::Path(e.path), false);
                    }
                }
            }
            Cmd::OpenNewWindow => {
                for p in self.tab().selected_paths() {
                    crate::util::spawn_window(Some(&p.to_string_lossy()));
                }
            }
            Cmd::OpenWith => {
                if let Some(p) = self.tab().selected_paths().first() {
                    smenu::open_with(&p.to_string_lossy());
                }
            }
            Cmd::RunAsAdmin => {
                if let Some(p) = self.tab().selected_paths().first() {
                    smenu::shell_execute(&p.to_string_lossy(), Some("runas"), None);
                }
            }
            Cmd::Cut | Cmd::Copy => {
                let cut = cmd == Cmd::Cut;
                if self.tab().is_bin() {
                    // Из корзины можно только вырезать: вставка восстанавливает элемент в новое место
                    if cut {
                        recycle::cut(&self.tab().selected_paths());
                    }
                    return;
                }
                if self.tab().is_virtual() {
                    let names = self.tab().selected_names();
                    smenu::invoke_verb(&names, if cut { "cut" } else { "copy" });
                    return;
                }
                let paths = self.tab().selected_paths();
                if clipboard::set_files(&paths, cut) {
                    self.cut_set = if cut { paths.into_iter().collect() } else { HashSet::new() };
                }
            }
            Cmd::Paste => {
                if let Some(d) = self.tab().fs_dir() {
                    if !self.tab().is_virtual() {
                        self.paste_into(ctx, d);
                    }
                }
            }
            Cmd::Rename => self.start_rename(),
            Cmd::Delete => self.delete_selection(false),
            Cmd::DeletePermanent => self.delete_selection(true),
            Cmd::Share => {
                let names = self.tab().selected_names();
                if !names.is_empty() && !smenu::invoke_verb(&names, "Windows.ModernShare") {
                    self.toast = Some((tr("Общий доступ для этих элементов недоступен").into(), now(ctx)));
                }
            }
            Cmd::Properties if self.tab().is_bin() => {
                let paths = self.tab().selected_paths();
                if paths.is_empty() {
                    smenu::invoke_verb(&[shellfolder::RECYCLE_BIN.to_string()], "properties");
                } else {
                    recycle::properties(&paths);
                }
            }
            Cmd::Properties => {
                let names = self.tab().selected_names();
                let own: Vec<PathBuf> = if names.is_empty() {
                    self.tab().fs_dir().into_iter().collect()
                } else if self.tab().is_virtual() {
                    vec![]
                } else {
                    self.tab().selected_paths()
                };
                if self.open_own_properties(ctx, own) {
                    return;
                }
                if names.is_empty() {
                    match &self.tab().loc {
                        Location::Path(p) => smenu::properties(&[p.to_string_lossy().to_string()]),
                        Location::Shell(s) => smenu::properties(&[s.clone()]),
                        _ => {}
                    }
                } else {
                    smenu::properties(&names);
                }
            }
            Cmd::CopyPath => {
                let paths = self.tab().selected_paths();
                let s: Vec<String> = if paths.is_empty() {
                    self.tab().fs_dir().map(|p| vec![format!("\"{}\"", p.display())]).unwrap_or_default()
                } else {
                    paths.iter().map(|p| format!("\"{}\"", p.display())).collect()
                };
                clipboard::set_text(&s.join("\r\n"));
            }
            Cmd::CopyPathOf(p) => clipboard::set_text(&format!("\"{}\"", p.display())),
            Cmd::Nothing => {}
            Cmd::DropAs { src, dst, how } => match how {
                crate::cmd::DropHow::Copy => self.ops.run(Op::Copy { src, dst }),
                crate::cmd::DropHow::Move => self.ops.run(Op::Move { src, dst }),
                crate::cmd::DropHow::Link => {
                    if let Err(e) = crate::shell::ops::create_shortcuts(&src, &dst) {
                        self.dialog = Some(Dialog::Message { title: tr("Не удалось выполнить операцию").into(), text: e });
                    }
                }
            },
            Cmd::PinHome => {
                let names = self.tab().selected_names();
                let names = if names.is_empty() { self.tab().fs_dir().map(|p| vec![p.to_string_lossy().to_string()]).unwrap_or_default() } else { names };
                smenu::invoke_verb(&names, "pintohome");
                self.load_quick(ctx);
            }
            Cmd::UnpinHome => {
                let names = self.tab().selected_names();
                smenu::invoke_verb(&names, "unpinfromhome");
                self.load_quick(ctx);
            }
            Cmd::PinPath(p) => {
                smenu::invoke_verb(&[p.to_string_lossy().to_string()], "pintohome");
                self.load_quick(ctx);
            }
            Cmd::UnpinPath(p) => {
                smenu::invoke_verb(&[p.to_string_lossy().to_string()], "unpinfromhome");
                self.load_quick(ctx);
            }
            Cmd::CompressZip => {
                let paths = self.tab().selected_paths();
                let Some(dir) = self.tab().fs_dir() else { return };
                if paths.is_empty() {
                    return;
                }
                let base = if paths.len() == 1 {
                    paths[0].file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| tr("Архив").into())
                } else {
                    dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| tr("Архив").into())
                };
                let out = fs::unique_name(&dir, &base, ".zip");
                let tx = self.tx.clone();
                let c = ctx.clone();
                self.tabs[a].rename_after_load = Some(out.clone());
                std::thread::spawn(move || {
                    let r = crate::shell::ops::compress_zip(&paths, &out);
                    if let Err(e) = r {
                        let _ = std::fs::remove_file(&out);
                        let _ = tx.send(Msg::Toast(trf("Не удалось создать ZIP: {}", &[&e])));
                    }
                    let _ = tx.send(Msg::RefreshAll);
                    c.request_repaint();
                });
            }
            Cmd::ExtractAll => {
                let paths = self.tab().selected_paths();
                let tx = self.tx.clone();
                let c = ctx.clone();
                std::thread::spawn(move || {
                    for z in paths {
                        let dir = z.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                        let stem = z.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| tr("Архив").into());
                        let dest = fs::unique_name(&dir, &stem, "");
                        if let Err(e) = crate::shell::ops::extract_zip(&z, &dest) {
                            let _ = tx.send(Msg::Toast(trf("Не удалось извлечь: {}", &[&e])));
                        }
                    }
                    let _ = tx.send(Msg::RefreshAll);
                    c.request_repaint();
                });
            }
            Cmd::OpenTerminal => {
                let sel = self.tab().selected_entries().first().filter(|e| e.is_dir).map(|e| e.path.clone());
                if let Some(d) = sel.or_else(|| self.tab().fs_dir()) {
                    smenu::open_terminal(&d);
                }
            }
            Cmd::MoreOptions => {
                let pos = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(Pos2::new(200.0, 200.0));
                let pos = self.last_menu_pos.unwrap_or(pos);
                self.show_system_menu(ctx, pos, false);
            }
            Cmd::NewFolder => self.new_folder(ctx),
            Cmd::NewItem(i) => {
                let Some(dir) = self.tab().fs_dir() else { return };
                let items = self.new_items.clone().unwrap_or_default();
                if let Some(it) = items.get(i) {
                    match crate::shell::ops::create_new_item(&dir, it) {
                        Ok(p) => {
                            self.undo.push(UndoRec::Created { paths: vec![p.clone()] });
                            self.tabs[a].rename_after_load = Some(p);
                            self.reload_keep(ctx, a);
                        }
                        Err(e) => self.dialog = Some(Dialog::Message { title: tr("Не удалось создать файл").into(), text: e }),
                    }
                }
            }
            Cmd::SortBy(c) => {
                let t = &mut self.tabs[a];
                if t.view.sort == c {
                    t.view.desc = !t.view.desc;
                } else {
                    t.view.sort = c;
                    t.view.desc = matches!(c, SortCol::Modified | SortCol::Size | SortCol::Extra);
                }
                t.resort();
                self.save_view(ctx);
            }
            Cmd::SortAsc | Cmd::SortDesc => {
                let t = &mut self.tabs[a];
                t.view.desc = cmd == Cmd::SortDesc;
                t.resort();
                self.save_view(ctx);
            }
            Cmd::View(v) => {
                self.tabs[a].view.view = v;
                self.tabs[a].scroll_to_focus = true;
                self.save_view(ctx);
            }
            Cmd::ToggleCompact => {
                self.settings.compact = !self.settings.compact;
                self.mark_settings(ctx);
            }
            Cmd::Pane(p) => {
                self.settings.pane = if self.settings.pane == p { PaneMode::None } else { p };
                self.mark_settings(ctx);
            }
            Cmd::ToggleCheckboxes => {
                self.settings.checkboxes = !self.settings.checkboxes;
                self.mark_settings(ctx);
            }
            Cmd::ToggleExt => {
                self.settings.show_ext = !self.settings.show_ext;
                self.mark_settings(ctx);
                self.fs_dirty_all = true;
            }
            Cmd::ToggleHidden => {
                self.settings.show_hidden = !self.settings.show_hidden;
                self.mark_settings(ctx);
                self.fs_dirty_all = true;
                self.nav.children.clear();
            }
            Cmd::SelectAll => {
                let t = &mut self.tabs[a];
                t.sel.iter_mut().for_each(|b| *b = true);
            }
            Cmd::SelectNone => self.tabs[a].clear_sel(),
            Cmd::InvertSelection => {
                let t = &mut self.tabs[a];
                t.sel.iter_mut().for_each(|b| *b = !*b);
            }
            Cmd::Undo => self.undo_last(ctx),
            Cmd::Refresh => {
                self.icons.clear_thumbs();
                match self.tab().loc {
                    Location::Home => self.load_quick(ctx),
                    Location::ThisPc => self.refresh_drives(ctx),
                    _ => {
                        if self.tab().searching() {
                            self.start_search(ctx, a);
                        } else {
                            self.reload_keep(ctx, a);
                        }
                    }
                }
                self.nav.children.clear();
            }
            Cmd::Options => self.dialog = Some(Dialog::Settings),
            Cmd::About => self.dialog = Some(Dialog::About),
            Cmd::SetWallpaper => {
                let names = self.tab().selected_names();
                smenu::invoke_verb(&names, "setdesktopwallpaper");
            }
            Cmd::EmptyRecycleBin => {
                let (n, _) = recycle::info();
                if n == 0 {
                    self.toast = Some((tr("Корзина пуста").into(), now(ctx)));
                } else {
                    self.dialog = Some(Dialog::Confirm {
                        title: tr("Удаление нескольких элементов").into(),
                        text: trf("Вы действительно хотите безвозвратно удалить все элементы из корзины ({})?", &[&n]),
                        cmd: Cmd::EmptyRecycleBinNow,
                    });
                }
            }
            Cmd::EmptyRecycleBinNow => {
                recycle::empty_now();
                self.fs_dirty_all = true;
            }
            Cmd::RestoreSelected => {
                let paths = self.tab().selected_paths();
                if !paths.is_empty() && recycle::restore(&paths) {
                    self.fs_dirty_all = true;
                }
            }
            Cmd::RestoreAll => {
                if recycle::restore_all() {
                    self.fs_dirty_all = true;
                }
            }
            Cmd::DeleteForeverNow(paths) => {
                let tx = self.tx.clone();
                let c = ctx.clone();
                recycle::delete_forever(paths, move |ok| {
                    if !ok {
                        let _ = tx.send(Msg::Toast(tr("Не удалось удалить элементы из корзины").into()));
                    }
                    let _ = tx.send(Msg::RefreshAll);
                    c.request_repaint();
                });
            }
            Cmd::Navigate(p) => self.navigate(ctx, Location::Path(p)),
            Cmd::NavigateShell(s) => self.navigate(ctx, Location::Shell(s)),
            Cmd::NavigateHome => self.navigate(ctx, Location::Home),
            Cmd::NavigateThisPc => self.navigate(ctx, Location::ThisPc),
            Cmd::NewTab => {
                let l = self.default_location();
                self.open_tab(ctx, l, true);
            }
            Cmd::CloseTab(i) => self.close_tab(ctx, i),
            Cmd::CloseOtherTabs(i) => {
                let keep = self.tabs.remove(i);
                self.tabs.clear();
                self.tabs.push(keep);
                self.active = 0;
            }
            Cmd::CloseTabsRight(i) => {
                self.tabs.truncate(i + 1);
                self.active = self.active.min(self.tabs.len() - 1);
            }
            Cmd::DuplicateTab(i) => {
                let l = self.tabs[i].loc.clone();
                self.active = i;
                self.open_tab(ctx, l, true);
            }
            Cmd::TabToWindow(i) => {
                let l = self.tabs[i].loc.arg();
                crate::util::spawn_window(Some(&l));
                self.close_tab(ctx, i);
            }
            Cmd::HistoryJump(n) => {
                if n < 0 {
                    for _ in 0..(-n) {
                        self.go_back(ctx);
                    }
                } else {
                    for _ in 0..n {
                        self.go_forward(ctx);
                    }
                }
            }
            Cmd::OpenParsing(s) => {
                smenu::open(&s);
            }
            Cmd::PropertiesOf(p) => {
                let s = p.to_string_lossy().to_string();
                if self.open_own_properties(ctx, vec![p.clone()]) {
                    return;
                }
                if s.starts_with("::") {
                    smenu::invoke_verb(&[s], "properties");
                } else {
                    smenu::properties(&[s]);
                }
            }
            Cmd::OpenInNewTabPath(p) => self.open_tab(ctx, Location::Path(p), false),
            Cmd::OpenInNewWindowPath(p) => crate::util::spawn_window(Some(&p.to_string_lossy())),
        }
    }

    pub fn save_view(&mut self, ctx: &egui::Context) {
        let key = self.tab().loc.key();
        let v = self.tab().view;
        self.settings.set_view_for(&key, v);
        self.settings.default_view.view = if self.tab().loc.is_listing() { self.settings.default_view.view } else { self.settings.default_view.view };
        self.mark_settings(ctx);
    }

    /// Полное системное меню («Показать дополнительные параметры»)
    pub fn show_system_menu(&mut self, ctx: &egui::Context, pos: Pos2, extended: bool) {
        let ppp = ctx.pixels_per_point();
        let screen = window::client_to_screen(pos, ppp);
        if self.tab().is_bin() && self.tab().sel_count() > 0 {
            if let smenu::MenuOutcome::Invoked(_) = recycle::show_menu(&self.tab().selected_paths(), screen, extended) {
                self.fs_dirty_all = true;
            }
            return;
        }
        let names = self.tab().selected_names();
        let folder = match &self.tab().loc {
            Location::Path(p) => Some(p.to_string_lossy().to_string()),
            Location::Shell(s) => Some(s.clone()),
            _ => None,
        };
        match smenu::show_shell_menu(&names, folder.as_deref(), screen, extended) {
            smenu::MenuOutcome::Rename => self.start_rename(),
            smenu::MenuOutcome::Open => self.open_selection(ctx),
            smenu::MenuOutcome::Invoked(v) => {
                if v == "pintohome" || v == "unpinfromhome" {
                    self.load_quick(ctx);
                }
                self.fs_dirty_all = true;
            }
            smenu::MenuOutcome::None => {}
        }
    }

    // ---------- Меню ----------

    pub fn open_popup(&mut self, p: Popup) {
        self.popup = Some(p);
        self.popup_seq += 1;
    }

    pub fn new_popup_id(&mut self) -> u64 {
        self.popup_seq += 1;
        self.popup_seq
    }

    pub fn ensure_new_items(&mut self) {
        if self.new_items.is_none() {
            self.new_items = Some(crate::shell::ops::shellnew_items());
        }
    }

    pub fn new_menu_items(&mut self) -> Vec<MenuItem> {
        self.ensure_new_items();
        let mut v = vec![MenuItem::new(tr("Папку"), Cmd::NewFolder).sys("<dir>".into(), std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).key("Ctrl+Shift+N")];
        let items = self.new_items.clone().unwrap_or_default();
        if !items.is_empty() {
            v.push(MenuItem::sep());
        }
        for (i, it) in items.iter().enumerate() {
            let probe = format!("probe{}", it.ext);
            v.push(MenuItem::new(it.title.clone(), Cmd::NewItem(i)).sys(it.ext.clone(), probe));
        }
        v
    }

    pub fn sort_menu_items(&self) -> Vec<MenuItem> {
        let t = self.tab();
        let mut cols = vec![SortCol::Name, SortCol::Modified, SortCol::Type, SortCol::Size];
        if t.searching() {
            cols.push(SortCol::Location);
        }
        let mut v: Vec<MenuItem> = cols.into_iter().map(|c| MenuItem::new(c.title(), Cmd::SortBy(c)).radio(t.view.sort == c)).collect();
        v.push(MenuItem::sep());
        v.push(MenuItem::new(tr("По возрастанию"), Cmd::SortAsc).radio(!t.view.desc));
        v.push(MenuItem::new(tr("По убыванию"), Cmd::SortDesc).radio(t.view.desc));
        v
    }

    pub fn view_menu_items(&self) -> Vec<MenuItem> {
        let t = self.tab();
        let mut v: Vec<MenuItem> = ViewMode::ALL
            .iter()
            .map(|&m| MenuItem::new(m.title(), Cmd::View(m)).icon(m.icon()).key(m.shortcut()).radio(t.view.view == m))
            .collect();
        v.push(MenuItem::sep());
        v.push(MenuItem::new(tr("Компактный вид"), Cmd::ToggleCompact).checked(self.settings.compact));
        v.push(MenuItem::submenu(
            tr("Показать"),
            vec![
                MenuItem::new(tr("Область сведений"), Cmd::Pane(PaneMode::Details)).key("Alt+Shift+P").checked(self.settings.pane == PaneMode::Details),
                MenuItem::new(tr("Область просмотра"), Cmd::Pane(PaneMode::Preview)).key("Alt+P").checked(self.settings.pane == PaneMode::Preview),
                MenuItem::sep(),
                MenuItem::new(tr("Флажки элементов"), Cmd::ToggleCheckboxes).checked(self.settings.checkboxes),
                MenuItem::new(tr("Расширения имён файлов"), Cmd::ToggleExt).checked(self.settings.show_ext),
                MenuItem::new(tr("Скрытые элементы"), Cmd::ToggleHidden).checked(self.settings.show_hidden),
            ],
        ).icon(ICON_SHOW));
        v
    }

    pub fn more_menu_items(&self) -> Vec<MenuItem> {
        let t = self.tab();
        let has_sel = t.sel_count() > 0;
        let mut v = vec![
            MenuItem::new(tr("Отменить"), Cmd::Undo).icon(ICON_UNDO).key("Ctrl+Z").enabled(!self.undo.is_empty()),
            MenuItem::new(tr("Закрепить на панели быстрого доступа"), Cmd::PinHome).icon(ICON_PIN).enabled(t.fs_dir().is_some()),
            MenuItem::sep(),
            MenuItem::new(tr("Выделить все"), Cmd::SelectAll).icon(ICON_SELECT_ALL).key("Ctrl+A").enabled(!t.entries.is_empty()),
            MenuItem::new(tr("Снять выделение"), Cmd::SelectNone).icon(ICON_SELECT_NONE).enabled(has_sel),
            MenuItem::new(tr("Обратить выделение"), Cmd::InvertSelection).icon(ICON_SELECT_NONE).enabled(!t.entries.is_empty()),
            MenuItem::sep(),
        ];
        if t.is_bin() {
            v.push(MenuItem::new(tr("Очистить корзину"), Cmd::EmptyRecycleBin).icon(ICON_DELETE).enabled(!t.entries.is_empty()));
            v.push(MenuItem::new(tr("Восстановить все объекты"), Cmd::RestoreAll).icon(ICON_RESTORE_BIN).enabled(!t.entries.is_empty()));
        }
        v.push(MenuItem::new(tr("Открыть в Терминале"), Cmd::OpenTerminal).icon(ICON_TERMINAL).enabled(t.fs_dir().is_some()));
        v.push(MenuItem::new(tr("Свойства"), Cmd::Properties).icon(ICON_PROPERTIES).key("Alt+Enter"));
        v.push(MenuItem::new(tr("Параметры"), Cmd::Options).icon(ICON_SETTINGS));
        v.push(MenuItem::new(tr("О программе Oxide"), Cmd::About).icon(ICON_INFO));
        v
    }

    /// Контекстное меню элементов (как в Windows 11)
    pub fn item_menu(&mut self, ctx: &egui::Context, pos: Pos2) -> Popup {
        let t = self.tab();
        let ents: Vec<Entry> = t.selected_entries().into_iter().cloned().collect();
        let virt = t.is_virtual();
        let single = ents.len() == 1;
        let all_dirs = ents.iter().all(|e| e.is_dir);
        let first = ents.first().cloned();
        if t.is_bin() {
            let items = vec![
                MenuItem::new(tr("Восстановить"), Cmd::RestoreSelected).icon(ICON_RESTORE_BIN),
                MenuItem::new(tr("Свойства"), Cmd::Properties).icon(ICON_PROPERTIES).key("Alt+Enter"),
                MenuItem::sep(),
                MenuItem::new(tr("Показать дополнительные параметры"), Cmd::MoreOptions).icon(ICON_MORE_OPTIONS).key("Shift+F10"),
            ];
            let top = vec![
                TopButton { icon: ICON_CUT, cmd: Cmd::Cut, tip: tr("Вырезать (Ctrl+X)").into(), enabled: true },
                TopButton { icon: ICON_DELETE, cmd: Cmd::Delete, tip: tr("Удалить (Delete)").into(), enabled: true },
            ];
            let id = self.new_popup_id();
            self.last_menu_pos = Some(pos);
            return Popup::new(id, pos, items, now(ctx)).with_top(top);
        }
        let mut items = vec![];
        items.push(MenuItem::new(tr("Открыть"), Cmd::Open).icon(ICON_OPEN).key("Enter"));
        if all_dirs && !virt {
            items.push(MenuItem::new(tr("Открыть в новой вкладке"), Cmd::OpenNewTab).icon(ICON_ADD));
            items.push(MenuItem::new(tr("Открыть в новом окне"), Cmd::OpenNewWindow).icon(ICON_NEW_WINDOW));
        }
        if let Some(e) = &first {
            if single && !e.is_dir && !virt {
                items.push(MenuItem::new(tr("Открыть с помощью"), Cmd::OpenWith).icon(ICON_OPEN_WITH));
                if matches!(e.ext.as_str(), "exe" | "bat" | "cmd" | "msi" | "ps1" | "lnk") {
                    items.push(MenuItem::new(tr("Запуск от имени администратора"), Cmd::RunAsAdmin).icon(ICON_RUN_ADMIN));
                }
            }
        }
        if !virt {
            items.push(MenuItem::sep());
            if all_dirs {
                let pinned = first.as_ref().map(|e| self.quick.iter().any(|q| q.2 && q.1 == e.path)).unwrap_or(false);
                if pinned && single {
                    items.push(MenuItem::new(tr("Открепить от панели быстрого доступа"), Cmd::UnpinHome).icon(ICON_UNPIN));
                } else {
                    items.push(MenuItem::new(tr("Закрепить на панели быстрого доступа"), Cmd::PinHome).icon(ICON_PIN));
                }
            }
            if let Some(e) = &first {
                if single && e.ext == "zip" {
                    items.push(MenuItem::new(tr("Извлечь все..."), Cmd::ExtractAll).icon(ICON_EXTRACT));
                }
                if single && matches!(e.ext.as_str(), "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp" | "jfif") {
                    items.push(MenuItem::new(tr("Сделать фоновым изображением рабочего стола"), Cmd::SetWallpaper).icon(ICON_WALLPAPER));
                }
            }
            items.push(MenuItem::new(tr("Сжать в ZIP-файл"), Cmd::CompressZip).icon(ICON_ZIP));
            items.push(MenuItem::new(tr("Копировать как путь"), Cmd::CopyPath).icon(ICON_LINK).key("Ctrl+Shift+C"));
            if all_dirs && single {
                items.push(MenuItem::new(tr("Открыть в Терминале"), Cmd::OpenTerminal).icon(ICON_TERMINAL));
            }
        }
        items.push(MenuItem::new(tr("Свойства"), Cmd::Properties).icon(ICON_PROPERTIES).key("Alt+Enter"));
        items.push(MenuItem::sep());
        items.push(MenuItem::new(tr("Показать дополнительные параметры"), Cmd::MoreOptions).icon(ICON_MORE_OPTIONS).key("Shift+F10"));
        let top = vec![
            TopButton { icon: ICON_CUT, cmd: Cmd::Cut, tip: tr("Вырезать (Ctrl+X)").into(), enabled: true },
            TopButton { icon: ICON_COPY, cmd: Cmd::Copy, tip: tr("Копировать (Ctrl+C)").into(), enabled: true },
            TopButton { icon: ICON_RENAME, cmd: Cmd::Rename, tip: tr("Переименовать (F2)").into(), enabled: single && !virt },
            TopButton { icon: ICON_SHARE, cmd: Cmd::Share, tip: tr("Поделиться").into(), enabled: !all_dirs },
            TopButton { icon: ICON_DELETE, cmd: Cmd::Delete, tip: tr("Удалить (Delete)").into(), enabled: true },
        ];
        let id = self.new_popup_id();
        self.last_menu_pos = Some(pos);
        Popup::new(id, pos, items, now(ctx)).with_top(top)
    }

    /// Контекстное меню пустого места папки
    pub fn background_menu(&mut self, ctx: &egui::Context, pos: Pos2) -> Popup {
        let virt = self.tab().is_virtual();
        let has_dir = self.tab().fs_dir().is_some() && !virt;
        let mut items = vec![
            MenuItem::submenu(tr("Вид"), self.view_menu_items()).icon(ICON_V_M),
            MenuItem::submenu(tr("Сортировка"), self.sort_menu_items()).icon(ICON_SORT),
            MenuItem::sep(),
            MenuItem::new(tr("Обновить"), Cmd::Refresh).icon(ICON_REFRESH).key("F5"),
        ];
        if has_dir {
            items.push(MenuItem::sep());
            items.push(MenuItem::new(tr("Вставить"), Cmd::Paste).icon(ICON_PASTE).key("Ctrl+V").enabled(clipboard::has_files()));
            items.push(MenuItem::new(tr("Отменить"), Cmd::Undo).icon(ICON_UNDO).key("Ctrl+Z").enabled(!self.undo.is_empty()));
            items.push(MenuItem::sep());
            let nm = self.new_menu_items();
            items.push(MenuItem::submenu(tr("Создать"), nm).icon(ICON_NEW));
            items.push(MenuItem::sep());
            items.push(MenuItem::new(tr("Открыть в Терминале"), Cmd::OpenTerminal).icon(ICON_TERMINAL));
        }
        if self.tab().is_bin() {
            let some = !self.tab().entries.is_empty();
            items.push(MenuItem::sep());
            items.push(MenuItem::new(tr("Очистить корзину"), Cmd::EmptyRecycleBin).icon(ICON_DELETE).enabled(some));
            items.push(MenuItem::new(tr("Восстановить все объекты"), Cmd::RestoreAll).icon(ICON_RESTORE_BIN).enabled(some));
        }
        items.push(MenuItem::new(tr("Свойства"), Cmd::Properties).icon(ICON_PROPERTIES).key("Alt+Enter"));
        items.push(MenuItem::sep());
        items.push(MenuItem::new(tr("Показать дополнительные параметры"), Cmd::MoreOptions).icon(ICON_MORE_OPTIONS).key("Shift+F10"));
        let id = self.new_popup_id();
        self.last_menu_pos = Some(pos);
        Popup::new(id, pos, items, now(ctx))
    }

    // ---------- Перетаскивание ----------

    fn drop_target_at(&self, p: Pos2) -> Option<PathBuf> {
        for (r, path) in &self.drop_targets.items {
            if r.contains(p) {
                return Some(path.clone());
            }
        }
        for (r, path) in &self.drop_targets.areas {
            if r.contains(p) {
                return Some(path.clone());
            }
        }
        None
    }

    fn drop_files(&mut self, files: Vec<PathBuf>, target: PathBuf) {
        let files: Vec<PathBuf> = files.into_iter().filter(|f| f != &target && f.parent() != Some(target.as_path()) && !target.starts_with(f)).collect();
        if files.is_empty() {
            return;
        }
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        let same_vol = |a: &Path, b: &Path| {
            let ra = a.to_string_lossy().chars().take(2).collect::<String>().to_lowercase();
            let rb = b.to_string_lossy().chars().take(2).collect::<String>().to_lowercase();
            ra == rb
        };
        let mv = if ctrl {
            false
        } else if shift {
            true
        } else {
            files.iter().all(|f| same_vol(f, &target))
        };
        if mv {
            self.ops.run(Op::Move { src: files, dst: target });
        } else {
            self.ops.run(Op::Copy { src: files, dst: target });
        }
    }

    /// Меню после перетаскивания правой кнопкой. Жирным - что сделалось бы левой кнопкой
    fn drop_menu(&mut self, ctx: &egui::Context, files: Vec<PathBuf>, target: PathBuf, pos: Pos2) {
        use crate::cmd::DropHow;
        let files: Vec<PathBuf> = files.into_iter().filter(|f| f != &target && !target.starts_with(f)).collect();
        if files.is_empty() {
            return;
        }
        let same_parent = files.iter().all(|f| f.parent() == Some(target.as_path()));
        let same_vol = files.iter().all(|f| {
            let a = f.to_string_lossy().chars().take(2).collect::<String>().to_lowercase();
            let b = target.to_string_lossy().chars().take(2).collect::<String>().to_lowercase();
            a == b
        });
        let mk = |label: &'static str, how: DropHow| MenuItem::new(tr(label), Cmd::DropAs { src: files.clone(), dst: target.clone(), how });
        let mut copy = mk("Копировать", DropHow::Copy);
        let mut mv = mk("Переместить", DropHow::Move);
        if same_vol {
            mv = mv.bold();
        } else {
            copy = copy.bold();
        }
        let mut items = vec![copy];
        // В ту же папку перемещать некуда: только копия и ярлыки
        if !same_parent {
            items.push(mv);
        }
        items.push(mk("Создать ярлыки", DropHow::Link));
        items.push(MenuItem::sep());
        items.push(MenuItem::new(tr("Отмена"), Cmd::Nothing));
        let id = self.new_popup_id();
        self.open_popup(Popup::new(id, pos, items, now(ctx)));
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let t_now = now(ctx);
        let ppp = ctx.pixels_per_point();
        // Наши файлы перетаскиваются наружу/внутрь (OLE). Выполняется в конце кадра
        if let Some((paths, right)) = self.pending_drag.take() {
            let _ = smenu::drag_out(&paths);
            self.ignore_drops_until = t_now + 1.5;
            // Если отпустили над нашим окном - выполняем сами. Правой кнопкой - сначала меню, как в Проводнике
            if let Some(p) = window::cursor_client_points(ppp) {
                if ctx.screen_rect().contains(p) && self.over_own_window() {
                    if let Some(target) = self.drop_target_at(p) {
                        if right {
                            self.drop_menu(ctx, paths, target, p);
                        } else {
                            self.drop_files(paths, target);
                        }
                    }
                }
            }
        }
        let (hovered, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
        if hovered {
            self.drag_hover = window::cursor_client_points(ppp).and_then(|p| self.drop_target_at(p));
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            self.drag_hover = None;
        }
        if !dropped.is_empty() {
            if t_now < self.ignore_drops_until {
                return;
            }
            let files: Vec<PathBuf> = dropped.into_iter().filter_map(|f| f.path).collect();
            let p = window::cursor_client_points(ppp);
            let target = p.and_then(|p| self.drop_target_at(p)).or_else(|| self.tab().fs_dir());
            if let Some(t) = target {
                self.drop_files(files, t);
            }
        }
    }

    fn over_own_window(&self) -> bool {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GetCursorPos, WindowFromPoint, GA_ROOT};
        unsafe {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_err() {
                return false;
            }
            let w = WindowFromPoint(pt);
            GetAncestor(w, GA_ROOT) == window::hwnd()
        }
    }

    // ---------- Тема ----------

    pub fn apply_theme(&mut self, ctx: &egui::Context) {
        self.system_dark = theme::system_is_dark();
        self.accent = theme::system_accent_palette();
        self.animations = theme::system_animations_enabled();
        self.pal = Palette::resolve(self.settings.theme, self.system_dark, self.accent).tune_glass(self.settings.glass_clarity, self.settings.glass_shine);
        if !self.hwnd_ok || !window::set_glass(window::hwnd(), self.pal.glass, self.settings.glass_blur) {
            self.pal = std::mem::replace(&mut self.pal, Palette::light(None)).solid();
        }
        theme::apply(ctx, &self.pal, self.animations);
        if self.hwnd_ok && self.last_dark_applied != Some(self.pal.dark) {
            window::set_dark(window::hwnd(), self.pal.dark);
            self.last_dark_applied = Some(self.pal.dark);
        }
    }

    pub fn set_theme(&mut self, ctx: &egui::Context, t: ThemeChoice) {
        self.settings.theme = t;
        self.apply_theme(ctx);
        self.mark_settings(ctx);
    }

    fn save_window(&mut self, ctx: &egui::Context) {
        let (outer, maxed) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().maximized.unwrap_or(false)));
        self.settings.maximized = maxed;
        if !maxed {
            if let Some(r) = outer {
                if r.width() > 200.0 && r.height() > 200.0 && r.left() > -10000.0 {
                    self.settings.window = Some([r.left(), r.top(), r.width(), r.height()]);
                }
            }
        }
        self.settings.nav_expanded = self.nav.expanded.iter().cloned().collect();
        self.settings.last_tabs = self.tabs.iter().map(|t| t.loc.arg()).collect();
    }
}

impl eframe::App for OxideApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if window::THEME_CHANGED.swap(false, Ordering::Relaxed) {
            self.apply_theme(ctx);
        }
        if self.last_dark_applied.is_none() && self.hwnd_ok {
            window::set_dark(window::hwnd(), self.pal.dark);
            self.last_dark_applied = Some(self.pal.dark);
        }
        self.icons.pump(ctx);
        self.poll(ctx);
        self.drop_targets = DropTargets::default();

        // Отложенный поиск при наборе
        let t_now = now(ctx);
        let a = self.active;
        if let Some(at) = self.tabs[a].search_edit_at {
            if t_now - at > 0.35 {
                self.tabs[a].search_edit_at = None;
                self.start_search(ctx, a);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }

        // Обновления: плановая проверка, предложение установить, перезапуск
        self.updater.tick(ctx, self.settings.check_updates);
        match self.updater.state() {
            crate::update::State::Available(rel) if !self.updater.offered && self.dialog.is_none() && self.popup.is_none() => {
                self.updater.offered = true;
                self.dialog = Some(Dialog::Update(rel));
            }
            crate::update::State::Installed(_) => {
                if self.updater.take_restart() {
                    self.finish_update(ctx);
                }
            }
            crate::update::State::Failed { text, rel: Some(_) } if self.dialog.is_none() => {
                self.updater.dismiss_error();
                self.dialog = Some(Dialog::Message { title: crate::i18n::l("Не удалось обновить Oxide", "Couldn't update Oxide").into(), text });
            }
            _ => {}
        }

        ui::keys::handle(self, ctx);
        ui::frame(self, ctx);

        // Меню поверх всего
        if let Some(mut pop) = self.popup.take() {
            // Слой-заглушка: щелчок мимо меню не срабатывает на элементах под ним
            egui::Area::new(egui::Id::new("popup-blocker"))
                .order(egui::Order::Middle)
                .fixed_pos(Pos2::ZERO)
                .show(ctx, |ui| {
                    let r = ctx.screen_rect();
                    ui.allocate_rect(r, egui::Sense::click_and_drag());
                });
            let pal = self.pal.clone();
            match crate::ui::menu::show(ctx, &mut pop, &pal, &mut self.icons, self.animations) {
                PopupResult::Open => self.popup = Some(pop),
                PopupResult::Close => {}
                PopupResult::Cmd(c) => self.exec(ctx, c),
            }
        }
        ui::dialogs::show(self, ctx);
        ui::toast(self, ctx);

        self.handle_drops(ctx);

        // Заголовок окна = имя текущей папки (как в Проводнике)
        let title = if self.tab().searching() { trf("Результаты поиска в «{}»", &[&self.tab().title]) } else { self.tab().title.clone() };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{title} - Oxide")));
            self.last_title = title;
        }
        if let Some(t) = self.settings_dirty {
            if t_now - t > 1.0 {
                self.settings_dirty = None;
                self.save_window(ctx);
                self.settings.save();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(1100));
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_window(ctx);
            self.settings.save();
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.pal.glass {
            // Под окном размытие Windows: фон не закрашиваем
            return [0.0; 4];
        }
        let c = self.pal.base;
        [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0]
    }
}
