//! Системные действия оболочки: полное контекстное меню Windows, глаголы (свойства, закрепить...),
//! открытие файлов, «Открыть с помощью», перетаскивание наружу

use std::path::{Path, PathBuf};

use windows::core::{Interface, PCSTR, PCWSTR};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{CoTaskMemFree, IDataObject};
use windows::Win32::System::Ole::{DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IContextMenu, IContextMenu2, IShellFolder, IShellItem, IShellItemArray, SHCreateDataObject, SHCreateItemFromParsingName,
    SHCreateShellItemArrayFromIDLists, SHDoDragDrop, SHOpenWithDialog, SHParseDisplayName, ShellExecuteExW, BHID_SFObject, BHID_SFUIObject,
    CMF_EXPLORE, CMF_EXTENDEDVERBS, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    GCS_VERBW, OAIF_EXEC, OAIF_ALLOW_REGISTRATION, OPENASINFO, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{CreatePopupMenu, DestroyMenu, TrackPopupMenuEx, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON};

use crate::util::{wide, wide_path};

const CMIC_MASK_UNICODE: u32 = 0x4000;
use windows::core::PSTR;

pub struct Pidl(*mut ITEMIDLIST);
impl Drop for Pidl {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0 as *const _)) }
    }
}

pub fn parse(name: &str) -> Option<Pidl> {
    let w = wide(name);
    unsafe {
        let mut p: *mut ITEMIDLIST = std::ptr::null_mut();
        SHParseDisplayName(PCWSTR(w.as_ptr()), None, &mut p, 0, None).ok()?;
        if p.is_null() {
            None
        } else {
            Some(Pidl(p))
        }
    }
}

unsafe fn item_array(names: &[String]) -> Option<IShellItemArray> {
    let pidls: Vec<Pidl> = names.iter().filter_map(|n| parse(n)).collect();
    if pidls.is_empty() {
        return None;
    }
    let raw: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| p.0 as *const _).collect();
    SHCreateShellItemArrayFromIDLists(&raw).ok()
}

unsafe fn items_menu(names: &[String]) -> Option<IContextMenu> {
    let arr = item_array(names)?;
    arr.BindToHandler(None, &BHID_SFUIObject).ok()
}

unsafe fn background_menu(folder: &str, hwnd: HWND) -> Option<IContextMenu> {
    let w = wide(folder);
    let it: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).ok()?;
    let sf: IShellFolder = it.BindToHandler(None, &BHID_SFObject).ok()?;
    sf.CreateViewObject::<_, IContextMenu>(hwnd).ok()
}

/// Что выбрал пользователь в системном меню, если это нужно обработать нам
pub enum MenuOutcome {
    None,
    Rename,
    Open,
    Invoked(String),
}

/// Показывает полное системное контекстное меню («Показать дополнительные параметры»)
/// `names` - пути или строки разбора; пустой список + `folder` - меню фона папки
pub fn show_shell_menu(names: &[String], folder: Option<&str>, screen: (i32, i32), extended: bool) -> MenuOutcome {
    unsafe {
        let hwnd = super::window::hwnd();
        let cm = if names.is_empty() {
            match folder {
                Some(f) => background_menu(f, hwnd),
                None => None,
            }
        } else {
            items_menu(names)
        };
        let Some(cm) = cm else { return MenuOutcome::None };
        let Ok(menu) = CreatePopupMenu() else { return MenuOutcome::None };
        let mut flags = CMF_NORMAL | CMF_EXPLORE;
        if extended {
            flags |= CMF_EXTENDEDVERBS;
        }
        if cm.QueryContextMenu(menu, 0, 1, 0x7FFF, flags).is_err() {
            let _ = DestroyMenu(menu);
            return MenuOutcome::None;
        }
        super::window::set_active_menu(cm.cast::<IContextMenu2>().ok());
        let cmd = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, screen.0, screen.1, hwnd, None);
        super::window::set_active_menu(None);
        let mut outcome = MenuOutcome::None;
        if cmd.0 > 0 {
            let id = (cmd.0 - 1) as usize;
            let mut verb = [0u16; 128];
            let _ = cm.GetCommandString(id, GCS_VERBW, None, PSTR(verb.as_mut_ptr() as *mut u8), verb.len() as u32);
            let verb = crate::util::from_wide(&verb).to_lowercase();
            if verb == "rename" {
                outcome = MenuOutcome::Rename;
            } else if (verb == "open" || verb == "explore") && names.len() == 1 && super::fs::is_dir(Path::new(&names[0])) {
                outcome = MenuOutcome::Open;
            } else {
                invoke_id(&cm, id, folder, screen);
                outcome = MenuOutcome::Invoked(verb);
            }
        }
        let _ = DestroyMenu(menu);
        outcome
    }
}

unsafe fn invoke_id(cm: &IContextMenu, id: usize, folder: Option<&str>, screen: (i32, i32)) {
    let dir = folder.map(wide);
    let mut info = CMINVOKECOMMANDINFOEX {
        cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
        hwnd: super::window::hwnd(),
        lpVerb: PCSTR(id as *const u8),
        lpVerbW: PCWSTR(id as *const u16),
        nShow: SW_SHOWNORMAL.0,
        ptInvoke: POINT { x: screen.0, y: screen.1 },
        ..Default::default()
    };
    if let Some(d) = &dir {
        info.lpDirectoryW = PCWSTR(d.as_ptr());
    }
    let _ = cm.InvokeCommand(&info as *const _ as *const CMINVOKECOMMANDINFO);
}

/// Вызов глагола оболочки по имени («properties», «pintohome», «Windows.ModernShare»...)
pub fn invoke_verb(names: &[String], verb: &str) -> bool {
    unsafe {
        let Some(cm) = items_menu(names) else { return false };
        let Ok(menu) = CreatePopupMenu() else { return false };
        let _ = cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL | CMF_EXTENDEDVERBS);
        let a: Vec<u8> = verb.bytes().chain(std::iter::once(0)).collect();
        let w = wide(verb);
        let info = CMINVOKECOMMANDINFOEX {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
            fMask: CMIC_MASK_UNICODE,
            hwnd: super::window::hwnd(),
            lpVerb: PCSTR(a.as_ptr()),
            lpVerbW: PCWSTR(w.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        let ok = cm.InvokeCommand(&info as *const _ as *const CMINVOKECOMMANDINFO).is_ok();
        let _ = DestroyMenu(menu);
        ok
    }
}

/// Есть ли у элементов глагол (например, «pintohome» или «unpinfromhome»)
pub fn has_verb(names: &[String], verb: &str) -> bool {
    unsafe {
        let Some(cm) = items_menu(names) else { return false };
        let Ok(menu) = CreatePopupMenu() else { return false };
        let _ = cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL);
        let n = windows::Win32::UI::WindowsAndMessaging::GetMenuItemCount(menu);
        let mut found = false;
        for i in 0..n.max(0) {
            let id = windows::Win32::UI::WindowsAndMessaging::GetMenuItemID(menu, i);
            if id == u32::MAX || id == 0 {
                continue;
            }
            let mut buf = [0u16; 128];
            if cm.GetCommandString((id - 1) as usize, GCS_VERBW, None, PSTR(buf.as_mut_ptr() as *mut u8), 128).is_ok()
                && crate::util::from_wide(&buf).eq_ignore_ascii_case(verb)
            {
                found = true;
                break;
            }
        }
        let _ = DestroyMenu(menu);
        found
    }
}

/// Открыть файл программой по умолчанию
pub fn open(path: &str) -> bool {
    shell_execute(path, None, None)
}

pub fn shell_execute(file: &str, verb: Option<&str>, params: Option<&str>) -> bool {
    let f = wide(file);
    let v = verb.map(wide);
    let p = params.map(wide);
    let dir = Path::new(file).parent().map(|d| wide_path(d));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_INVOKEIDLIST | SEE_MASK_NOASYNC,
        hwnd: super::window::hwnd(),
        lpFile: PCWSTR(f.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    if let Some(v) = &v {
        info.lpVerb = PCWSTR(v.as_ptr());
    }
    if let Some(p) = &p {
        info.lpParameters = PCWSTR(p.as_ptr());
    }
    if let Some(d) = &dir {
        info.lpDirectory = PCWSTR(d.as_ptr());
    }
    unsafe { ShellExecuteExW(&mut info).is_ok() }
}

pub fn open_with(path: &str) {
    let w = wide(path);
    let info = OPENASINFO { pcszFile: PCWSTR(w.as_ptr()), pcszClass: PCWSTR::null(), oaifInFlags: OAIF_EXEC | OAIF_ALLOW_REGISTRATION };
    unsafe {
        let _ = SHOpenWithDialog(super::window::hwnd(), &info);
    }
}

pub fn properties(names: &[String]) {
    if names.len() == 1 {
        shell_execute(&names[0], Some("properties"), None);
    } else {
        invoke_verb(names, "properties");
    }
}

/// «Открыть в Терминале»: Windows Terminal, иначе PowerShell
pub fn open_terminal(dir: &Path) {
    let d = dir.to_string_lossy().to_string();
    let ok = std::process::Command::new("wt.exe").arg("-d").arg(&d).spawn().is_ok();
    if !ok {
        let _ = std::process::Command::new("powershell.exe").current_dir(dir).arg("-NoExit").spawn();
    }
}

/// Перетаскивание файлов в другие программы (и в другие окна). Возвращает эффект
pub fn drag_out(paths: &[PathBuf]) -> Option<DROPEFFECT> {
    unsafe {
        let names: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
        let pidls: Vec<Pidl> = names.iter().filter_map(|n| parse(n)).collect();
        if pidls.is_empty() {
            return None;
        }
        // Общий родитель - относительные PIDL от него
        let arr = item_array(&names)?;
        let data: IDataObject = arr.BindToHandler(None, &windows::Win32::UI::Shell::BHID_DataObject).ok().or_else(|| {
            let raw: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| p.0 as *const _).collect();
            SHCreateDataObject(None, Some(&raw), None).ok()
        })?;
        let r = SHDoDragDrop(super::window::hwnd(), &data, None, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK);
        r.ok()
    }
}
