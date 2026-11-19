//! Все команды приложения: из меню, кнопок и горячих клавиш.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum ViewMode {
    ExtraLarge,
    Large,
    Medium,
    Small,
    List,
    Details,
    Tiles,
    Content,
}

impl ViewMode {
    pub const ALL: [ViewMode; 8] = [
        ViewMode::ExtraLarge,
        ViewMode::Large,
        ViewMode::Medium,
        ViewMode::Small,
        ViewMode::List,
        ViewMode::Details,
        ViewMode::Tiles,
        ViewMode::Content,
    ];
    pub fn title(self) -> &'static str {
        match self {
            ViewMode::ExtraLarge => "Огромные значки",
            ViewMode::Large => "Крупные значки",
            ViewMode::Medium => "Обычные значки",
            ViewMode::Small => "Мелкие значки",
            ViewMode::List => "Список",
            ViewMode::Details => "Таблица",
            ViewMode::Tiles => "Плитка",
            ViewMode::Content => "Содержимое",
        }
    }
    pub fn shortcut(self) -> &'static str {
        match self {
            ViewMode::ExtraLarge => "Ctrl+Shift+1",
            ViewMode::Large => "Ctrl+Shift+2",
            ViewMode::Medium => "Ctrl+Shift+3",
            ViewMode::Small => "Ctrl+Shift+4",
            ViewMode::List => "Ctrl+Shift+5",
            ViewMode::Details => "Ctrl+Shift+6",
            ViewMode::Tiles => "Ctrl+Shift+7",
            ViewMode::Content => "Ctrl+Shift+8",
        }
    }
    pub fn icon(self) -> char {
        use crate::ui::widgets::*;
        match self {
            ViewMode::ExtraLarge => ICON_V_XL,
            ViewMode::Large => ICON_V_L,
            ViewMode::Medium => ICON_V_M,
            ViewMode::Small => ICON_V_S,
            ViewMode::List => ICON_V_LIST,
            ViewMode::Details => ICON_V_DETAILS,
            ViewMode::Tiles => ICON_V_TILES,
            ViewMode::Content => ICON_V_CONTENT,
        }
    }
    /// Размер значка в логических пикселях.
    pub fn icon_size(self) -> f32 {
        match self {
            ViewMode::ExtraLarge => 256.0,
            ViewMode::Large => 96.0,
            ViewMode::Medium => 48.0,
            ViewMode::Small | ViewMode::List | ViewMode::Details => 16.0,
            ViewMode::Tiles => 48.0,
            ViewMode::Content => 32.0,
        }
    }
    pub fn thumbs(self) -> bool {
        matches!(self, ViewMode::ExtraLarge | ViewMode::Large | ViewMode::Medium | ViewMode::Tiles | ViewMode::Content)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum SortCol {
    Name,
    Modified,
    Type,
    Size,
    Location,
    Extra,
}

impl SortCol {
    pub fn title(self) -> &'static str {
        match self {
            SortCol::Name => "Имя",
            SortCol::Modified => "Дата изменения",
            SortCol::Type => "Тип",
            SortCol::Size => "Размер",
            SortCol::Location => "Папка",
            SortCol::Extra => "Дата",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PaneMode {
    None,
    Details,
    Preview,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Open,
    OpenNewTab,
    OpenNewWindow,
    OpenWith,
    RunAsAdmin,
    Cut,
    Copy,
    Paste,
    Rename,
    Delete,
    DeletePermanent,
    Share,
    Properties,
    CopyPath,
    PinHome,
    UnpinHome,
    CompressZip,
    ExtractAll,
    OpenTerminal,
    MoreOptions,
    NewFolder,
    NewItem(usize),
    SortBy(SortCol),
    SortAsc,
    SortDesc,
    View(ViewMode),
    ToggleCompact,
    Pane(PaneMode),
    ToggleCheckboxes,
    ToggleExt,
    ToggleHidden,
    SelectAll,
    SelectNone,
    InvertSelection,
    Undo,
    Refresh,
    Options,
    SetWallpaper,
    EmptyRecycleBin,
    Navigate(PathBuf),
    NavigateShell(String),
    NavigateHome,
    NavigateThisPc,
    NewTab,
    CloseTab(usize),
    CloseOtherTabs(usize),
    CloseTabsRight(usize),
    DuplicateTab(usize),
    TabToWindow(usize),
    HistoryJump(isize),
    OpenParsing(String),
    CopyPathOf(PathBuf),
    PropertiesOf(PathBuf),
    OpenInNewTabPath(PathBuf),
    OpenInNewWindowPath(PathBuf),
    PinPath(PathBuf),
    UnpinPath(PathBuf),
    About,
}
