//! Настройки Oxide. При первом запуске берутся из параметров Проводника пользователя,
//! чтобы всё выглядело так же (скрытые файлы, расширения, куда открывать)

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use windows::Win32::System::Registry::HKEY_CURRENT_USER;

use crate::cmd::{PaneMode, SortCol, ViewMode};
use crate::i18n::Lang;
use crate::theme::ThemeChoice;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum OpenTo {
    Home,
    ThisPc,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct FolderView {
    pub view: ViewMode,
    pub sort: SortCol,
    pub desc: bool,
}

impl Default for FolderView {
    fn default() -> Self {
        FolderView { view: ViewMode::Details, sort: SortCol::Name, desc: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub show_hidden: bool,
    pub show_super_hidden: bool,
    pub show_ext: bool,
    pub checkboxes: bool,
    pub compact: bool,
    pub open_to: OpenTo,
    pub nav_width: f32,
    pub pane_width: f32,
    pub search_width: f32,
    pub pane: PaneMode,
    pub window: Option<[f32; 4]>,
    pub maximized: bool,
    pub default_view: FolderView,
    pub folder_views: HashMap<String, FolderView>,
    pub col_widths: [f32; 6],
    pub index_enabled: bool,
    pub nav_expanded: Vec<String>,
    pub show_recent: bool,
    pub show_frequent: bool,
    pub restore_tabs: bool,
    pub last_tabs: Vec<String>,
    pub show_about_on_start: bool,
    /// None - язык ещё не выбран, при запуске будет вопрос
    pub language: Option<Lang>,
    pub check_updates: bool,
    /// Папки, диски, Win+E и «Показать в папке» открываются в Oxide
    pub default_explorer: bool,
    /// «Жидкое стекло»: насколько видно то, что за окном (0 - матовое, 1 - почти прозрачное)
    pub glass_clarity: f32,
    /// «Жидкое стекло»: размывать то, что за окном
    pub glass_blur: bool,
    /// «Жидкое стекло»: яркость бликов и переливов по краям
    pub glass_shine: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeChoice::System,
            show_hidden: false,
            show_super_hidden: false,
            show_ext: false,
            checkboxes: false,
            compact: false,
            open_to: OpenTo::Home,
            nav_width: 200.0,
            pane_width: 300.0,
            search_width: crate::theme::m::SEARCH_W,
            pane: PaneMode::None,
            window: None,
            maximized: false,
            default_view: FolderView::default(),
            folder_views: HashMap::new(),
            col_widths: [260.0, 150.0, 140.0, 100.0, 260.0, 150.0],
            index_enabled: true,
            nav_expanded: vec!["thispc".into()],
            show_recent: true,
            show_frequent: true,
            restore_tabs: false,
            last_tabs: vec![],
            show_about_on_start: true,
            language: None,
            check_updates: true,
            default_explorer: false,
            glass_clarity: 0.55,
            glass_blur: true,
            glass_shine: 0.7,
        }
    }
}

fn file() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(base).join("Oxide").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        if let Some(f) = file() {
            if let Ok(s) = std::fs::read_to_string(&f) {
                if let Ok(v) = serde_json::from_str::<Settings>(&s) {
                    return v;
                }
                if let Some(v) = Self::salvage(&s) {
                    return v;
                }
            }
        }
        Self::from_explorer()
    }

    /// Файл не читается целиком (например, его записала другая версия Oxide):
    /// берём каждое поле, которое читается, остальные - по умолчанию. Так настройки
    /// пользователя не сбрасываются из-за одного незнакомого значения
    fn salvage(text: &str) -> Option<Self> {
        let serde_json::Value::Object(saved) = serde_json::from_str::<serde_json::Value>(text).ok()? else { return None };
        let serde_json::Value::Object(mut acc) = serde_json::to_value(Self::from_explorer()).ok()? else { return None };
        for (k, v) in saved {
            if !acc.contains_key(&k) {
                continue;
            }
            let mut probe = acc.clone();
            probe.insert(k.clone(), v.clone());
            if serde_json::from_value::<Settings>(serde_json::Value::Object(probe)).is_ok() {
                acc.insert(k, v);
            }
        }
        let f = file()?;
        let _ = std::fs::copy(&f, f.with_extension("json.bak"));
        serde_json::from_value(serde_json::Value::Object(acc)).ok()
    }

    /// Начальные значения из настроек Проводника Windows
    pub fn from_explorer() -> Self {
        let mut s = Settings::default();
        let adv = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        let get = |n: &str| crate::util::reg_read_dword(HKEY_CURRENT_USER, adv, n);
        if let Some(v) = get("Hidden") {
            s.show_hidden = v == 1;
        }
        if let Some(v) = get("HideFileExt") {
            s.show_ext = v == 0;
        }
        if let Some(v) = get("ShowSuperHidden") {
            s.show_super_hidden = v == 1;
        }
        if let Some(v) = get("AutoCheckSelect") {
            s.checkboxes = v == 1;
        }
        if let Some(v) = get("UseCompactMode") {
            s.compact = v == 1;
        }
        if let Some(v) = get("LaunchTo") {
            s.open_to = if v == 1 { OpenTo::ThisPc } else { OpenTo::Home };
        }
        let ex = r"Software\Microsoft\Windows\CurrentVersion\Explorer";
        if let Some(v) = crate::util::reg_read_dword(HKEY_CURRENT_USER, ex, "ShowRecent") {
            s.show_recent = v != 0;
        }
        if let Some(v) = crate::util::reg_read_dword(HKEY_CURRENT_USER, ex, "ShowFrequent") {
            s.show_frequent = v != 0;
        }
        s
    }

    pub fn save(&self) {
        let Some(f) = file() else { return };
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let tmp = f.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, &f);
            }
        }
    }

    pub fn view_for(&self, key: &str) -> FolderView {
        self.folder_views.get(key).copied().unwrap_or(self.default_view)
    }

    pub fn set_view_for(&mut self, key: &str, v: FolderView) {
        if self.folder_views.len() > 2000 {
            self.folder_views.clear();
        }
        self.folder_views.insert(key.to_string(), v);
    }
}
