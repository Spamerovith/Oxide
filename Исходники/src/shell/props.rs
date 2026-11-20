//! Windows-часть окна «Свойства»: сведения о файлах, подсчёт содержимого папок, атрибуты,
//! открытие системных вкладок свойств на нужной странице

use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_FLAGS_AND_ATTRIBUTES};
use windows::Win32::UI::Shell::{AssocQueryStringW, SHObjectProperties, ASSOCF_NONE, ASSOCSTR_FRIENDLYAPPNAME, SHOP_FILEPATH};

use super::fs::{for_each_child_full, ATTR_DIR, ATTR_REPARSE};
use crate::util::{from_wide, wide, wide_path};

pub const ATTR_READONLY: u32 = 0x1;
pub const ATTR_HIDDEN: u32 = 0x2;

/// Сведения об одном выбранном элементе
#[derive(Clone, Debug, Default)]
pub struct ItemInfo {
    pub path: PathBuf,
    pub is_dir: bool,
    pub attrs: u32,
    pub size: u64,
    pub alloc: u64,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
}

/// Сведения о выбранных элементах. Размер на диске берётся из списка их папки одним вызовом
pub fn items_info(paths: &[PathBuf]) -> Vec<ItemInfo> {
    let mut out: Vec<ItemInfo> = paths
        .iter()
        .map(|p| {
            let mut i = ItemInfo { path: p.clone(), ..Default::default() };
            if let Ok(m) = std::fs::symlink_metadata(super::fs::long_path(p)) {
                i.attrs = m.file_attributes();
                i.is_dir = i.attrs & ATTR_DIR != 0;
                i.size = if i.is_dir { 0 } else { m.file_size() };
                i.alloc = i.size;
                i.created = m.creation_time();
                i.modified = m.last_write_time();
                i.accessed = m.last_access_time();
            }
            i
        })
        .collect();
    // Размер на диске файлов: по одной выборке на каждую родительскую папку
    let mut parents: Vec<PathBuf> = out.iter().filter(|i| !i.is_dir).filter_map(|i| i.path.parent().map(|p| p.to_path_buf())).collect();
    parents.sort();
    parents.dedup();
    for dir in parents {
        let _ = for_each_child_full(&dir, |e| {
            if e.attrs & ATTR_DIR != 0 {
                return;
            }
            for i in out.iter_mut() {
                if !i.is_dir && i.path.parent() == Some(dir.as_path()) && i.path.file_name().map(|n| n.to_string_lossy() == e.name).unwrap_or(false) {
                    i.alloc = e.alloc;
                }
            }
        });
    }
    out
}

/// Содержимое папок: размер, размер на диске, число файлов и папок
#[derive(Clone, Copy, Debug, Default)]
pub struct Count {
    pub size: u64,
    pub alloc: u64,
    pub files: u64,
    pub dirs: u64,
    pub done: bool,
}

/// Точный подсчёт содержимого папок в фоне (несколько потоков, крупные пачки).
/// Результат копится в `out`, `cancel` останавливает подсчёт, когда окно закрыли
pub fn count_tree(roots: Vec<PathBuf>, out: Arc<Mutex<Count>>, cancel: Arc<AtomicBool>, ctx: egui::Context) {
    std::thread::Builder::new()
        .name("props-count".into())
        .spawn(move || {
            let stack = Mutex::new(roots);
            let active = AtomicUsize::new(0);
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
            let last_paint = Mutex::new(std::time::Instant::now());
            std::thread::scope(|s| {
                for _ in 0..threads {
                    s.spawn(|| loop {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let job = {
                            let mut g = stack.lock().unwrap();
                            let j = g.pop();
                            if j.is_some() {
                                active.fetch_add(1, Ordering::SeqCst);
                            }
                            j
                        };
                        let Some(dir) = job else {
                            if active.load(Ordering::SeqCst) == 0 {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(1));
                            continue;
                        };
                        let mut c = Count::default();
                        let mut sub = vec![];
                        let _ = for_each_child_full(&dir, |e| {
                            if e.attrs & ATTR_DIR != 0 {
                                c.dirs += 1;
                                if e.attrs & ATTR_REPARSE == 0 {
                                    sub.push(dir.join(e.name));
                                }
                            } else {
                                c.files += 1;
                                c.size += e.size;
                                c.alloc += e.alloc;
                            }
                        });
                        stack.lock().unwrap().extend(sub);
                        {
                            let mut o = out.lock().unwrap();
                            o.size += c.size;
                            o.alloc += c.alloc;
                            o.files += c.files;
                            o.dirs += c.dirs;
                        }
                        let mut lp = last_paint.lock().unwrap();
                        if lp.elapsed().as_millis() > 100 {
                            *lp = std::time::Instant::now();
                            ctx.request_repaint();
                        }
                        active.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            });
            if !cancel.load(Ordering::Relaxed) {
                out.lock().unwrap().done = true;
            }
            ctx.request_repaint();
        })
        .ok();
}

/// Программа, которой открываются файлы с таким расширением («Блокнот»)
pub fn opens_with(ext: &str) -> Option<String> {
    if ext.is_empty() {
        return None;
    }
    let e = wide(&format!(".{ext}"));
    let mut buf = [0u16; 260];
    let mut n = buf.len() as u32;
    unsafe {
        AssocQueryStringW(ASSOCF_NONE, ASSOCSTR_FRIENDLYAPPNAME, PCWSTR(e.as_ptr()), PCWSTR::null(), PWSTR(buf.as_mut_ptr()), &mut n).ok().ok()?;
    }
    let s = from_wide(&buf);
    (!s.is_empty()).then_some(s)
}

/// Названия вкладок системного окна свойств на языке Windows (по ним открывается нужная вкладка)
pub fn system_page_name(ru: &'static str, en: &'static str) -> &'static str {
    let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() } & 0x3FF;
    if lang == 0x19 {
        ru
    } else {
        en
    }
}

/// Системное окно свойств, открытое на вкладке `page` (None - «Общие»)
pub fn system_properties(path: &Path, page: Option<&str>) {
    let p = wide_path(path);
    let pg = page.map(wide);
    unsafe {
        let _ = SHObjectProperties(
            super::window::hwnd(),
            SHOP_FILEPATH,
            PCWSTR(p.as_ptr()),
            pg.as_ref().map(|w| PCWSTR(w.as_ptr())).unwrap_or(PCWSTR::null()),
        );
    }
}

fn set_attr(p: &Path, attrs: u32, set: u32, clear: u32) -> bool {
    let new = (attrs | set) & !clear;
    if new == attrs {
        return true;
    }
    // FILE_ATTRIBUTE_NORMAL, если других атрибутов не осталось
    let v = new & !ATTR_DIR;
    let v = if v == 0 { 0x80 } else { v };
    let w = wide_path(&super::fs::long_path(p));
    unsafe { SetFileAttributesW(PCWSTR(w.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(v)).is_ok() }
}

/// Ставит и снимает атрибуты. `nested` - и у всего содержимого папок (в фоне)
pub fn apply_attrs(items: Vec<(PathBuf, u32)>, set: u32, clear: u32, nested: bool, done: impl FnOnce(usize) + Send + 'static) {
    std::thread::Builder::new()
        .name("props-attrs".into())
        .spawn(move || {
            let mut failed = 0usize;
            let mut stack = vec![];
            for (p, a) in &items {
                if !set_attr(p, *a, set, clear) {
                    failed += 1;
                }
                if nested && a & ATTR_DIR != 0 && a & ATTR_REPARSE == 0 {
                    stack.push(p.clone());
                }
            }
            while let Some(dir) = stack.pop() {
                let mut kids = vec![];
                let _ = for_each_child_full(&dir, |e| kids.push((dir.join(e.name), e.attrs)));
                for (p, a) in kids {
                    if !set_attr(&p, a, set, clear) {
                        failed += 1;
                    }
                    if a & ATTR_DIR != 0 && a & ATTR_REPARSE == 0 {
                        stack.push(p);
                    }
                }
            }
            done(failed);
        })
        .ok();
}
