//! Файловая система: быстрый список папки, диски, типы файлов, отображаемые имена.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, ERROR_PATH_NOT_FOUND, HANDLE};
use windows::Win32::Storage::FileSystem::{
    FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, GetDiskFreeSpaceExW, GetDriveTypeW,
    GetFileAttributesExW, GetLogicalDrives, GetFileExInfoStandard, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
    FIND_FIRST_EX_LARGE_FETCH, WIN32_FILE_ATTRIBUTE_DATA, WIN32_FIND_DATAW,
};
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHGetKnownFolderPath, KF_FLAG_DEFAULT, SHFILEINFOW, SHGFI_DISPLAYNAME, SHGFI_TYPENAME, SHGFI_USEFILEATTRIBUTES,
};

use crate::util::{from_wide, wide, wide_path};

pub const ATTR_DIR: u32 = FILE_ATTRIBUTE_DIRECTORY.0;
pub const ATTR_HIDDEN: u32 = FILE_ATTRIBUTE_HIDDEN.0;
pub const ATTR_SYSTEM: u32 = FILE_ATTRIBUTE_SYSTEM.0;
pub const ATTR_READONLY: u32 = FILE_ATTRIBUTE_READONLY.0;
pub const ATTR_REPARSE: u32 = FILE_ATTRIBUTE_REPARSE_POINT.0;

#[derive(Clone, Debug)]
pub struct Entry {
    /// Имя на диске.
    pub name: String,
    /// Имя для показа (локализованное, без расширения при необходимости).
    pub display: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub attrs: u32,
    /// Расширение в нижнем регистре без точки.
    pub ext: String,
    /// Для элементов оболочки (Корзина, Сеть): строка разбора.
    pub shell: Option<String>,
    /// Для результатов поиска и «Последних»: папка, где лежит элемент.
    pub location: Option<String>,
    /// Доп. столбец (Корзина: исходное расположение; Последние: дата открытия).
    pub extra: Option<String>,
    pub type_name: Option<String>,
}

impl Entry {
    pub fn is_hidden(&self) -> bool {
        self.attrs & ATTR_HIDDEN != 0
    }
    pub fn customized_dir(&self) -> bool {
        self.is_dir && self.attrs & (ATTR_READONLY | ATTR_SYSTEM) != 0
    }
}

pub struct ListOptions {
    pub show_hidden: bool,
    pub show_super_hidden: bool,
    pub show_ext: bool,
}

pub fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => crate::util::lower(&name[i + 1..]),
        _ => String::new(),
    }
}

/// Расширения, которые Проводник не показывает никогда (NeverShowExt).
pub fn never_show_ext(ext: &str) -> bool {
    matches!(ext, "lnk" | "url" | "pif" | "appref-ms" | "library-ms" | "searchconnector-ms")
}

pub fn display_name_for(name: &str, is_dir: bool, ext: &str, show_ext: bool) -> String {
    if is_dir || ext.is_empty() || (show_ext && !never_show_ext(ext)) {
        return name.to_string();
    }
    let cut = name.len() - ext.len() - 1;
    if cut == 0 {
        name.to_string()
    } else {
        name[..cut].to_string()
    }
}

pub fn long_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s.len() > 240 && !s.starts_with(r"\\") {
        PathBuf::from(format!(r"\\?\{s}"))
    } else {
        p.to_path_buf()
    }
}

#[inline]
fn ft(f: &windows::Win32::Foundation::FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// Перебирает содержимое папки системным вызовом (быстро, без лишних stat).
pub fn for_each_child(dir: &Path, mut f: impl FnMut(&str, u32, u64, u64, u64)) -> Result<(), String> {
    let pat = long_path(&dir.join("*"));
    let w = wide_path(&pat);
    unsafe {
        let mut data = WIN32_FIND_DATAW::default();
        let h = FindFirstFileExW(
            PCWSTR(w.as_ptr()),
            FindExInfoBasic,
            &mut data as *mut _ as *mut _,
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        );
        let h: HANDLE = match h {
            Ok(h) => h,
            Err(e) => {
                let code = e.code().0 as u32 & 0xFFFF;
                if code == ERROR_FILE_NOT_FOUND.0 || code == ERROR_NO_MORE_FILES.0 {
                    return Ok(());
                }
                return Err(error_text(code));
            }
        };
        loop {
            let name = from_wide(&data.cFileName);
            if name != "." && name != ".." {
                let size = ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64;
                f(&name, data.dwFileAttributes, size, ft(&data.ftLastWriteTime), ft(&data.ftCreationTime));
            }
            if FindNextFileW(h, &mut data).is_err() {
                break;
            }
        }
        let _ = FindClose(h);
    }
    Ok(())
}

pub fn error_text(code: u32) -> String {
    if code == ERROR_ACCESS_DENIED.0 {
        "Отказано в доступе.".into()
    } else if code == ERROR_PATH_NOT_FOUND.0 || code == ERROR_FILE_NOT_FOUND.0 {
        "Расположение недоступно: путь не найден.".into()
    } else {
        let msg = windows::core::Error::from(windows::core::HRESULT::from_win32(code)).message();
        if msg.is_empty() {
            format!("Ошибка {code}.")
        } else {
            msg
        }
    }
}

pub fn list_dir(dir: &Path, o: &ListOptions) -> Result<Vec<Entry>, String> {
    let mut out = Vec::with_capacity(256);
    for_each_child(dir, |name, attrs, size, mtime, ctime| {
        if attrs & ATTR_HIDDEN != 0 && !o.show_hidden {
            return;
        }
        if attrs & ATTR_HIDDEN != 0 && attrs & ATTR_SYSTEM != 0 && !o.show_super_hidden {
            return;
        }
        let is_dir = attrs & ATTR_DIR != 0;
        let ext = if is_dir { String::new() } else { ext_of(name) };
        let display = display_name_for(name, is_dir, &ext, o.show_ext);
        out.push(Entry {
            name: name.to_string(),
            display,
            path: dir.join(name),
            is_dir,
            size: if is_dir { 0 } else { size },
            mtime,
            ctime,
            attrs,
            ext,
            shell: None,
            location: None,
            extra: None,
            type_name: None,
        });
    })?;
    // Локализованные имена особых папок (Загрузки, Документы...).
    for e in out.iter_mut() {
        if e.customized_dir() && e.path.join("desktop.ini").exists() {
            if let Some(d) = shell_display_name(&e.path) {
                if !d.is_empty() {
                    e.display = d;
                }
            }
        }
    }
    Ok(out)
}

/// Сведения об одном файле (для результатов поиска).
pub fn stat_entry(path: &Path, o: &ListOptions) -> Option<Entry> {
    let w = wide_path(&long_path(path));
    let mut d = WIN32_FILE_ATTRIBUTE_DATA::default();
    unsafe {
        GetFileAttributesExW(PCWSTR(w.as_ptr()), GetFileExInfoStandard, &mut d as *mut _ as *mut _).ok()?;
    }
    let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
    let attrs = d.dwFileAttributes;
    let is_dir = attrs & ATTR_DIR != 0;
    let ext = if is_dir { String::new() } else { ext_of(&name) };
    let mut display = display_name_for(&name, is_dir, &ext, o.show_ext);
    if is_dir && attrs & (ATTR_READONLY | ATTR_SYSTEM) != 0 && path.join("desktop.ini").exists() {
        if let Some(dn) = shell_display_name(path) {
            display = dn;
        }
    }
    Some(Entry {
        name,
        display,
        path: path.to_path_buf(),
        is_dir,
        size: if is_dir { 0 } else { ((d.nFileSizeHigh as u64) << 32) | d.nFileSizeLow as u64 },
        mtime: ft(&d.ftLastWriteTime),
        ctime: ft(&d.ftCreationTime),
        attrs,
        ext,
        shell: None,
        location: path.parent().map(|p| p.to_string_lossy().to_string()),
        extra: None,
        type_name: None,
    })
}

pub fn shell_display_name(path: &Path) -> Option<String> {
    let w = wide_path(path);
    let mut info = SHFILEINFOW::default();
    unsafe {
        let r = SHGetFileInfoW(
            PCWSTR(w.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_DISPLAYNAME,
        );
        if r == 0 {
            return None;
        }
    }
    let s = from_wide(&info.szDisplayName);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

static TYPE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Название типа («Текстовый документ», «Папка с файлами»), кэш по расширению.
pub fn type_name(ext: &str, is_dir: bool) -> String {
    let key = if is_dir { "<dir>".to_string() } else { ext.to_string() };
    let cache = TYPE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().unwrap().get(&key) {
        return v.clone();
    }
    let probe = if is_dir { "folder".to_string() } else if ext.is_empty() { "file".to_string() } else { format!("file.{ext}") };
    let w = wide(&probe);
    let mut info = SHFILEINFOW::default();
    let attrs = if is_dir { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
    let r = unsafe {
        SHGetFileInfoW(
            PCWSTR(w.as_ptr()),
            attrs,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_TYPENAME | SHGFI_USEFILEATTRIBUTES,
        )
    };
    let mut s = if r != 0 { from_wide(&info.szTypeName) } else { String::new() };
    if s.is_empty() {
        s = if is_dir {
            "Папка с файлами".into()
        } else if ext.is_empty() {
            "Файл".into()
        } else {
            format!("Файл \"{}\"", ext.to_uppercase())
        };
    }
    cache.lock().unwrap().insert(key, s.clone());
    s
}

#[derive(Clone, Debug)]
pub struct Drive {
    pub root: PathBuf,
    pub letter: char,
    pub label: String,
    pub kind: u32,
    pub total: u64,
    pub free: u64,
}

pub const DRIVE_REMOVABLE: u32 = 2;
pub const DRIVE_FIXED: u32 = 3;
pub const DRIVE_REMOTE: u32 = 4;
pub const DRIVE_CDROM: u32 = 5;

/// Список дисков; `with_space` — опрашивать свободное место (может ждать сетевые диски).
pub fn drives(with_space: bool) -> Vec<Drive> {
    let mask = unsafe { GetLogicalDrives() };
    let mut out = vec![];
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let w = wide(&root);
        let kind = unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) };
        let (mut total, mut free) = (0u64, 0u64);
        if with_space {
            unsafe {
                let mut avail = 0u64;
                let _ = GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut avail), Some(&mut total), Some(&mut free));
                free = avail;
            }
        }
        let label = shell_display_name(Path::new(&root)).unwrap_or_else(|| format!("Локальный диск ({letter}:)"));
        out.push(Drive { root: PathBuf::from(&root), letter, label, kind, total, free });
    }
    out
}

/// Путь известной папки по GUID.
pub fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = crate::util::from_pwstr(p);
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    }
}

pub mod kf {
    use windows::Win32::UI::Shell::*;
    pub fn desktop() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Desktop)
    }
    pub fn downloads() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Downloads)
    }
    pub fn documents() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Documents)
    }
    pub fn pictures() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Pictures)
    }
    pub fn music() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Music)
    }
    pub fn videos() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Videos)
    }
    pub fn profile() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Profile)
    }
    pub fn recent() -> Option<std::path::PathBuf> {
        super::known_folder(&FOLDERID_Recent)
    }
}

pub fn exists(p: &Path) -> bool {
    let w = wide_path(&long_path(p));
    let mut d = WIN32_FILE_ATTRIBUTE_DATA::default();
    unsafe { GetFileAttributesExW(PCWSTR(w.as_ptr()), GetFileExInfoStandard, &mut d as *mut _ as *mut _).is_ok() }
}

pub fn is_dir(p: &Path) -> bool {
    let w = wide_path(&long_path(p));
    let mut d = WIN32_FILE_ATTRIBUTE_DATA::default();
    unsafe {
        GetFileAttributesExW(PCWSTR(w.as_ptr()), GetFileExInfoStandard, &mut d as *mut _ as *mut _).is_ok()
            && d.dwFileAttributes & ATTR_DIR != 0
    }
}

/// Только подпапки (для дерева навигации).
pub fn subdirs(dir: &Path, show_hidden: bool) -> Vec<(String, PathBuf)> {
    let mut v = vec![];
    let _ = for_each_child(dir, |name, attrs, _, _, _| {
        if attrs & ATTR_DIR == 0 {
            return;
        }
        if attrs & ATTR_HIDDEN != 0 && !show_hidden {
            return;
        }
        if attrs & ATTR_HIDDEN != 0 && attrs & ATTR_SYSTEM != 0 {
            return;
        }
        let p = dir.join(name);
        let mut disp = name.to_string();
        if attrs & (ATTR_READONLY | ATTR_SYSTEM) != 0 && p.join("desktop.ini").exists() {
            if let Some(d) = shell_display_name(&p) {
                disp = d;
            }
        }
        v.push((disp, p));
    });
    v.sort_by(|a, b| crate::util::natural_cmp(&a.0, &b.0));
    v
}

pub fn has_subdirs(dir: &Path) -> bool {
    let mut found = false;
    let _ = for_each_child(dir, |_, attrs, _, _, _| {
        if attrs & ATTR_DIR != 0 && attrs & ATTR_HIDDEN == 0 {
            found = true;
        }
    });
    found
}

pub fn last_error() -> u32 {
    unsafe { GetLastError().0 }
}

/// Уникальное имя в папке: «Новая папка», «Новая папка (2)»...
pub fn unique_name(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let mk = |n: usize| {
        if n == 1 {
            format!("{base}{ext}")
        } else {
            format!("{base} ({n}){ext}")
        }
    };
    let mut n = 1;
    loop {
        let p = dir.join(mk(n));
        if !exists(&p) {
            return p;
        }
        n += 1;
    }
}
