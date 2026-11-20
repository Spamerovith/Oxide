//! Общие помощники: строки UTF-16, реестр, форматирование дат/размеров, естественная сортировка

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use std::cmp::Ordering;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::Globalization::{GetDateFormatEx, GetTimeFormatEx, DATE_SHORTDATE, TIME_NOSECONDS};
use windows::Win32::System::Registry::{RegGetValueW, HKEY, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_MULTI_SZ};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
use windows::Win32::UI::Shell::{StrFormatByteSizeEx, StrFormatKBSizeW, SFBS_FLAGS_ROUND_TO_NEAREST_DISPLAYED_DIGIT};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn wide_path(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

pub unsafe fn from_pwstr(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = p.to_string().unwrap_or_default();
    windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
    s
}

fn reg_raw(root: HKEY, sub: &str, name: &str, flags: windows::Win32::System::Registry::REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
    let subw = wide(sub);
    let namew = wide(name);
    unsafe {
        let mut size = 0u32;
        let r = RegGetValueW(root, PCWSTR(subw.as_ptr()), PCWSTR(namew.as_ptr()), flags, None, None, Some(&mut size));
        if r.is_err() || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let r = RegGetValueW(
            root,
            PCWSTR(subw.as_ptr()),
            PCWSTR(namew.as_ptr()),
            flags,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        );
        if r.is_err() {
            return None;
        }
        buf.truncate(size as usize);
        Some(buf)
    }
}

pub fn reg_read_binary(root: HKEY, sub: &str, name: &str) -> Option<Vec<u8>> {
    reg_raw(root, sub, name, RRF_RT_REG_BINARY)
}

pub fn reg_read_dword(root: HKEY, sub: &str, name: &str) -> Option<u32> {
    let b = reg_raw(root, sub, name, RRF_RT_REG_DWORD)?;
    if b.len() >= 4 {
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    } else {
        None
    }
}

fn bytes_to_u16(b: &[u8]) -> Vec<u16> {
    b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

pub fn reg_read_string(root: HKEY, sub: &str, name: &str) -> Option<String> {
    let b = reg_raw(root, sub, name, windows::Win32::System::Registry::REG_ROUTINE_FLAGS(RRF_RT_REG_SZ.0 | RRF_RT_REG_EXPAND_SZ.0))?;
    Some(from_wide(&bytes_to_u16(&b)))
}

pub fn reg_read_multi(root: HKEY, sub: &str, name: &str) -> Vec<String> {
    let Some(b) = reg_raw(root, sub, name, RRF_RT_REG_MULTI_SZ) else { return vec![] };
    let w = bytes_to_u16(&b);
    w.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
}

/// FILETIME (u64) → локальная дата «dd.MM.yyyy H:mm» по формату пользователя
pub fn format_filetime(ft: u64) -> String {
    if ft == 0 {
        return String::new();
    }
    unsafe {
        let f = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
        let mut utc = SYSTEMTIME::default();
        if FileTimeToSystemTime(&f, &mut utc).is_err() {
            return String::new();
        }
        let mut local = SYSTEMTIME::default();
        if SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err() {
            local = utc;
        }
        let mut d = [0u16; 64];
        let mut t = [0u16; 64];
        let dn = GetDateFormatEx(PCWSTR::null(), DATE_SHORTDATE, Some(&local), PCWSTR::null(), Some(&mut d), PCWSTR::null());
        let tn = GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, Some(&local), PCWSTR::null(), Some(&mut t));
        let mut s = from_wide(&d[..dn.max(0) as usize]);
        if tn > 0 {
            s.push(' ');
            s.push_str(&from_wide(&t[..tn as usize]));
        }
        s
    }
}

/// Полная дата со временем, как в свойствах файла («4 октября 2025 г., 19:14:24»)
pub fn format_filetime_long(ft: u64) -> String {
    use windows::Win32::Globalization::DATE_LONGDATE;
    if ft == 0 {
        return String::new();
    }
    unsafe {
        let f = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
        let mut utc = SYSTEMTIME::default();
        if FileTimeToSystemTime(&f, &mut utc).is_err() {
            return String::new();
        }
        let mut local = SYSTEMTIME::default();
        if SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err() {
            local = utc;
        }
        let mut d = [0u16; 96];
        let mut t = [0u16; 64];
        let dn = GetDateFormatEx(PCWSTR::null(), DATE_LONGDATE, Some(&local), PCWSTR::null(), Some(&mut d), PCWSTR::null());
        let tn = GetTimeFormatEx(PCWSTR::null(), Default::default(), Some(&local), PCWSTR::null(), Some(&mut t));
        let mut s = from_wide(&d[..dn.max(0) as usize]);
        if tn > 0 {
            s.push_str(", ");
            s.push_str(&from_wide(&t[..tn as usize]));
        }
        s
    }
}

/// Число с разделением разрядов по правилам языка интерфейса («17 903 871 773» / «17,903,871,773»)
pub fn group_digits(n: u64) -> String {
    let sep = if crate::i18n::is_en() { ',' } else { '\u{00A0}' };
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Размер в КБ, как в столбце «Размер» Проводника («1 234 КБ»)
pub fn format_kb(size: u64) -> String {
    let mut buf = [0u16; 64];
    unsafe {
        let _ = StrFormatKBSizeW(size as i64, &mut buf);
    }
    from_wide(&buf)
}

/// Размер в удобных единицах («1,52 МБ»)
pub fn format_bytes(size: u64) -> String {
    let mut buf = [0u16; 64];
    unsafe {
        if StrFormatByteSizeEx(size, SFBS_FLAGS_ROUND_TO_NEAREST_DISPLAYED_DIGIT, &mut buf).is_err() {
            return trf("{} Б", &[&size]);
        }
    }
    from_wide(&buf)
}

pub fn now_filetime() -> u64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_nanos() / 100) as u64 + 116_444_736_000_000_000
}

/// Естественное сравнение имён, как StrCmpLogicalW: регистр не важен, числа сравниваются по значению
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let mut na = String::new();
                    while let Some(&c) = ai.peek() {
                        if c.is_ascii_digit() {
                            na.push(c);
                            ai.next();
                        } else {
                            break;
                        }
                    }
                    let mut nb = String::new();
                    while let Some(&c) = bi.peek() {
                        if c.is_ascii_digit() {
                            nb.push(c);
                            bi.next();
                        } else {
                            break;
                        }
                    }
                    let ta = na.trim_start_matches('0');
                    let tb = nb.trim_start_matches('0');
                    let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                    if o != Ordering::Equal {
                        return o;
                    }
                    let o = nb.len().cmp(&na.len());
                    if o != Ordering::Equal {
                        return o;
                    }
                } else {
                    let la = fold(ca);
                    let lb = fold(cb);
                    if la != lb {
                        return la.cmp(&lb);
                    }
                    ai.next();
                    bi.next();
                }
            }
        }
    }
}

#[inline]
pub fn fold(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else {
        let mut l = c.to_lowercase();
        let f = l.next().unwrap_or(c);
        if f == 'ё' {
            'е'
        } else {
            f
        }
    }
}

/// Строка в нижнем регистре для поиска
pub fn lower(s: &str) -> String {
    s.chars().map(fold).collect()
}

/// Русская форма числительного: 1 элемент, 2 элемента, 5 элементов
pub fn plural<'a>(n: usize, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let n10 = n % 10;
    let n100 = n % 100;
    if n10 == 1 && n100 != 11 {
        one
    } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
        few
    } else {
        many
    }
}

/// Проверка имени файла на недопустимые символы
pub fn invalid_name_char(name: &str) -> Option<char> {
    name.chars().find(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || (*c as u32) < 32)
}

pub fn exe_path() -> std::path::PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "oxide.exe".into())
}

/// Открыть новое окно Oxide
pub fn spawn_window(arg: Option<&str>) {
    let mut cmd = std::process::Command::new(exe_path());
    if let Some(a) = arg {
        cmd.arg(a);
    }
    let _ = cmd.spawn();
}

/// Отладочный журнал (только если задана переменная OXIDE_DEBUG)
pub fn log(s: &str) {
    if std::env::var_os("OXIDE_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    let p = std::env::temp_dir().join("oxide.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{s}");
    }
}

/// Время двойного щелчка Windows в секундах
pub fn double_click_time() -> f64 {
    let ms = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() };
    (if ms == 0 { 500 } else { ms }) as f64 / 1000.0
}
