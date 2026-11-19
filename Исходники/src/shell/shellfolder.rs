//! Виртуальные папки оболочки (Корзина, Сеть, Быстрый доступ, Галерея) через IEnumShellItems.

use std::path::PathBuf;

use windows::core::{Interface, GUID, PCWSTR};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, IEnumShellItems, IShellItem, IShellItem2, SHCreateItemFromParsingName,
    SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY,
};

use windows::Win32::System::SystemServices::{SFGAO_FILESYSTEM, SFGAO_FOLDER, SFGAO_STREAM};
use super::fs::{ext_of, Entry, ATTR_DIR};
use crate::util::{from_pwstr, wide};

pub const RECYCLE_BIN: &str = "::{645FF040-5081-101B-9F08-00AA002F954E}";
pub const NETWORK: &str = "::{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}";
pub const QUICK_ACCESS: &str = "shell:::{679f85cb-0220-4080-b29b-5540cc05aab6}";
pub const GALLERY: &str = "shell:::{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}";

const FMT_STORAGE: GUID = GUID::from_u128(0xb725f130_47ef_101a_a5f1_02608c9eebac);
const FMT_RECYCLE: GUID = GUID::from_u128(0x9b174b33_40ff_11d2_a27e_00c04fc30871);
const PKEY_SIZE: PROPERTYKEY = PROPERTYKEY { fmtid: FMT_STORAGE, pid: 12 };
const PKEY_MODIFIED: PROPERTYKEY = PROPERTYKEY { fmtid: FMT_STORAGE, pid: 14 };
const PKEY_TYPE_TEXT: PROPERTYKEY = PROPERTYKEY { fmtid: FMT_STORAGE, pid: 4 };
const PKEY_DELETED_FROM: PROPERTYKEY = PROPERTYKEY { fmtid: FMT_RECYCLE, pid: 2 };
const PKEY_DATE_DELETED: PROPERTYKEY = PROPERTYKEY { fmtid: FMT_RECYCLE, pid: 3 };

pub fn display_name_of(parsing: &str) -> Option<String> {
    unsafe {
        let w = wide(parsing);
        let it: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).ok()?;
        Some(from_pwstr(it.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?))
    }
}

pub fn enumerate(parsing: &str, show_ext: bool) -> Result<Vec<Entry>, String> {
    unsafe {
        let w = wide(parsing);
        let folder: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).map_err(|e| e.message())?;
        let en: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems).map_err(|e| e.message())?;
        let is_bin = parsing.eq_ignore_ascii_case(RECYCLE_BIN);
        let mut out = vec![];
        loop {
            let mut items: [Option<IShellItem>; 1] = [None];
            let mut fetched = 0u32;
            if en.Next(&mut items, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(it) = items[0].take() else { break };
            let display = it.GetDisplayName(SIGDN_NORMALDISPLAY).map(|p| from_pwstr(p)).unwrap_or_default();
            let parse = it.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING).map(|p| from_pwstr(p)).unwrap_or_default();
            let fspath = it.GetDisplayName(SIGDN_FILESYSPATH).map(|p| from_pwstr(p)).ok();
            let attrs = it.GetAttributes(SFGAO_FOLDER | SFGAO_FILESYSTEM | SFGAO_STREAM).unwrap_or_default();
            let is_folder = attrs.0 & SFGAO_FOLDER.0 != 0 && attrs.0 & SFGAO_STREAM.0 == 0;
            let mut e = Entry {
                name: display.clone(),
                display: display.clone(),
                path: PathBuf::from(fspath.clone().unwrap_or_else(|| parse.clone())),
                is_dir: is_folder,
                size: 0,
                mtime: 0,
                ctime: 0,
                attrs: if is_folder { ATTR_DIR } else { 0 },
                ext: String::new(),
                shell: Some(parse.clone()),
                location: None,
                extra: None,
                type_name: None,
            };
            if let Some(fp) = &fspath {
                let fname = std::path::Path::new(fp).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if !is_folder {
                    e.ext = ext_of(&fname);
                }
                if !is_bin {
                    e.name = fname;
                }
            }
            if let Ok(i2) = it.cast::<IShellItem2>() {
                if let Ok(s) = i2.GetUInt64(&PKEY_SIZE) {
                    e.size = s;
                }
                if let Ok(t) = i2.GetFileTime(&PKEY_MODIFIED) {
                    e.mtime = ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
                }
                if let Ok(t) = i2.GetString(&PKEY_TYPE_TEXT) {
                    e.type_name = Some(from_pwstr(t));
                }
                if is_bin {
                    if let Ok(s) = i2.GetString(&PKEY_DELETED_FROM) {
                        e.location = Some(from_pwstr(s));
                    }
                    if let Ok(t) = i2.GetFileTime(&PKEY_DATE_DELETED) {
                        e.ctime = ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
                    }
                }
            }
            if !is_bin && !e.is_dir && !show_ext {
                // Отображаемое имя оболочки уже учитывает настройку расширений.
            }
            out.push(e);
        }
        Ok(out)
    }
}

/// Закреплённые и частые папки Быстрого доступа — те же, что в Проводнике.
pub fn quick_access() -> Vec<(String, PathBuf, bool)> {
    use windows::Win32::UI::Shell::PropertiesSystem::PSGetPropertyKeyFromName;
    let mut out: Vec<(String, PathBuf, Option<bool>)> = vec![];
    unsafe {
        let mut key = PROPERTYKEY::default();
        let have_key = PSGetPropertyKeyFromName(windows::core::w!("System.Home.IsPinned"), &mut key).is_ok();
        let w = wide(QUICK_ACCESS);
        let folder: IShellItem = match SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None) { Ok(f) => f, Err(_) => return vec![] };
        let en: IEnumShellItems = match folder.BindToHandler(None, &BHID_EnumItems) { Ok(e) => e, Err(_) => return vec![] };
        loop {
            let mut items: [Option<IShellItem>; 1] = [None];
            let mut fetched = 0u32;
            if en.Next(&mut items, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(it) = items[0].take() else { break };
            let Ok(fsp) = it.GetDisplayName(SIGDN_FILESYSPATH).map(|p| from_pwstr(p)) else { continue };
            let path = PathBuf::from(&fsp);
            if !super::fs::is_dir(&path) {
                continue;
            }
            let display = it.GetDisplayName(SIGDN_NORMALDISPLAY).map(|p| from_pwstr(p)).unwrap_or_else(|_| fsp.clone());
            let mut pinned = None;
            if have_key {
                if let Ok(i2) = it.cast::<IShellItem2>() {
                    if let Ok(b) = i2.GetBool(&key) {
                        pinned = Some(b.as_bool());
                    }
                }
            }
            crate::util::log(&format!("qa item {fsp} pinned={pinned:?} key={have_key}"));
            out.push((display, path, pinned));
        }
    }
    // Если свойство недоступно — закреплёнными считаем то, что Проводник отдаёт с глаголом «открепить».
    out.into_iter()
        .map(|(d, p, pin)| {
            let pinned = pin.unwrap_or_else(|| super::menu::has_verb(&[p.to_string_lossy().to_string()], "unpinfromhome"));
            (d, p, pinned)
        })
        .collect()
}

/// Недавние файлы из папки Recent (ярлыки → цели).
pub fn recent_files(limit: usize) -> Vec<(PathBuf, u64)> {
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let Some(dir) = super::fs::kf::recent() else { return vec![] };
    let mut links: Vec<(PathBuf, u64)> = vec![];
    let _ = super::fs::for_each_child(&dir, |name, attrs, _, mtime, _| {
        if attrs & ATTR_DIR == 0 && name.to_lowercase().ends_with(".lnk") {
            links.push((dir.join(name), mtime));
        }
    });
    links.sort_by(|a, b| b.1.cmp(&a.1));
    let mut out = vec![];
    unsafe {
        let Ok(link): Result<IShellLinkW, _> = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) else { return vec![] };
        let Ok(pf) = link.cast::<IPersistFile>() else { return vec![] };
        for (lnk, t) in links {
            if out.len() >= limit {
                break;
            }
            let w = crate::util::wide_path(&lnk);
            if pf.Load(PCWSTR(w.as_ptr()), STGM_READ).is_err() {
                continue;
            }
            let mut buf = [0u16; 1024];
            if link.GetPath(&mut buf, std::ptr::null_mut(), 0).is_err() {
                continue;
            }
            let target = PathBuf::from(crate::util::from_wide(&buf));
            if target.as_os_str().is_empty() || super::fs::is_dir(&target) || !super::fs::exists(&target) {
                continue;
            }
            out.push((target, t));
        }
    }
    out
}

/// Цель ярлыка, если это папка.
pub fn shortcut_target(lnk: &std::path::Path) -> Option<PathBuf> {
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf = link.cast::<IPersistFile>().ok()?;
        let w = crate::util::wide_path(lnk);
        pf.Load(PCWSTR(w.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let t = PathBuf::from(crate::util::from_wide(&buf));
        if t.as_os_str().is_empty() {
            None
        } else {
            Some(t)
        }
    }
}

pub fn empty_recycle_bin() {
    use windows::Win32::UI::Shell::SHEmptyRecycleBinW;
    unsafe {
        let _ = SHEmptyRecycleBinW(super::window::hwnd(), PCWSTR::null(), 0);
    }
}
