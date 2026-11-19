//! Буфер обмена в формате Проводника: CF_HDROP + «Preferred DropEffect» (копировать/вырезать).

use std::path::PathBuf;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HGLOBAL, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};
use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};

use crate::util::wide_path;

fn drop_effect_format() -> u32 {
    unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) }
}

unsafe fn open() -> bool {
    for _ in 0..10 {
        if OpenClipboard(super::window::hwnd()).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    false
}

unsafe fn global_from(bytes: &[u8]) -> Option<HGLOBAL> {
    let h = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()).ok()?;
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        return None;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    let _ = GlobalUnlock(h);
    Some(h)
}

pub fn set_files(paths: &[PathBuf], cut: bool) -> bool {
    if paths.is_empty() {
        return false;
    }
    let mut list: Vec<u16> = vec![];
    for p in paths {
        list.extend(wide_path(p));
    }
    list.push(0);
    let header = DROPFILES { pFiles: std::mem::size_of::<DROPFILES>() as u32, pt: POINT::default(), fNC: false.into(), fWide: true.into() };
    let mut bytes = vec![0u8; std::mem::size_of::<DROPFILES>() + list.len() * 2];
    unsafe {
        std::ptr::copy_nonoverlapping(&header as *const _ as *const u8, bytes.as_mut_ptr(), std::mem::size_of::<DROPFILES>());
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, bytes.as_mut_ptr().add(std::mem::size_of::<DROPFILES>()), list.len() * 2);
        if !open() {
            return false;
        }
        let _ = EmptyClipboard();
        let mut ok = false;
        if let Some(h) = global_from(&bytes) {
            ok = SetClipboardData(CF_HDROP.0 as u32, HANDLE(h.0)).is_ok();
        }
        let effect: u32 = if cut { 2 } else { 5 };
        if let Some(h) = global_from(&effect.to_le_bytes()) {
            let _ = SetClipboardData(drop_effect_format(), HANDLE(h.0));
        }
        let _ = CloseClipboard();
        ok
    }
}

/// Файлы из буфера: (пути, вырезаны ли).
pub fn get_files() -> Option<(Vec<PathBuf>, bool)> {
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_err() {
            return None;
        }
        if !open() {
            return None;
        }
        let mut out = vec![];
        let mut cut = false;
        if let Ok(h) = GetClipboardData(CF_HDROP.0 as u32) {
            let hdrop = HDROP(h.0);
            let n = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
            for i in 0..n {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                out.push(PathBuf::from(crate::util::from_wide(&buf)));
            }
        }
        if let Ok(h) = GetClipboardData(drop_effect_format()) {
            let p = GlobalLock(HGLOBAL(h.0)) as *const u32;
            if !p.is_null() {
                cut = *p & 2 != 0 && *p & 1 == 0;
                let _ = GlobalUnlock(HGLOBAL(h.0));
            }
        }
        let _ = CloseClipboard();
        if out.is_empty() {
            None
        } else {
            Some((out, cut))
        }
    }
}

pub fn has_files() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() }
}

pub fn set_text(s: &str) {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes: Vec<u8> = w.iter().flat_map(|c| c.to_le_bytes()).collect();
    unsafe {
        if !open() {
            return;
        }
        let _ = EmptyClipboard();
        if let Some(h) = global_from(&bytes) {
            let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, HANDLE(h.0));
        }
        let _ = CloseClipboard();
    }
}

pub fn clear() {
    unsafe {
        if open() {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
    }
}

#[allow(dead_code)]
const _P: PCWSTR = PCWSTR::null();
