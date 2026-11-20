//! Oxide как проводник по умолчанию (только для текущего пользователя, HKCU)
//!
//! - `Directory` и `Drive`: своя команда по умолчанию. Папки и диски с рабочего стола, из ярлыков,
//!   из других программ и «Показать в папке» (SHOpenFolderAndSelectItems) открываются в Oxide.
//!   Виртуальные папки («Панель управления», «Корзина») не трогаются: они остаются за Windows
//!
//! Win+E так не перехватить: ключ `opennewwindow` у CLSID Проводника Windows 11 больше не учитывает
//!
//! Выключение удаляет только свои ключи, и Windows возвращается к Проводнику. Ключи HKLM не меняются

use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegDeleteTreeW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

use crate::util::{reg_read_string, wide};

const VERB: &str = "oxide";
const CLASSES: [&str; 2] = [r"Software\Classes\Directory\shell", r"Software\Classes\Drive\shell"];

fn exe() -> String {
    crate::util::exe_path().to_string_lossy().to_string()
}

fn set_str(sub: &str, name: Option<&str>, value: &str) -> bool {
    let subw = wide(sub);
    let namew = name.map(wide);
    let data = wide(value);
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subw.as_ptr()),
            namew.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()),
            REG_SZ.0,
            Some(data.as_ptr() as *const _),
            (data.len() * 2) as u32,
        )
    };
    r == ERROR_SUCCESS
}

fn delete_tree(sub: &str) {
    let w = wide(sub);
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(w.as_ptr()));
    }
}

fn notify() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}

/// Папки сейчас открываются в этом самом Oxide.exe
pub fn is_enabled() -> bool {
    let want = format!("\"{}\"", exe()).to_lowercase();
    CLASSES.iter().all(|c| {
        reg_read_string(HKEY_CURRENT_USER, c, "").as_deref() == Some(VERB)
            && reg_read_string(HKEY_CURRENT_USER, &format!(r"{c}\{VERB}\command"), "").map(|s| s.to_lowercase().starts_with(&want)).unwrap_or(false)
    })
}

/// Сделать Oxide проводником по умолчанию или вернуть Проводник
pub fn set(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = exe();
        let open = format!("\"{exe}\" \"%1\"");
        let label = crate::i18n::l("Открыть в Oxide", "Open in Oxide");
        let mut ok = true;
        for c in CLASSES {
            let verb = format!(r"{c}\{VERB}");
            ok &= set_str(&verb, None, label);
            ok &= set_str(&verb, Some("Icon"), &exe);
            ok &= set_str(&format!(r"{verb}\command"), None, &open);
            // Свой пункт - действие по умолчанию: двойной щелчок и «Открыть» идут в Oxide
            ok &= set_str(c, None, VERB);
        }
        if !ok {
            set(false).ok();
            return Err(crate::i18n::l("Windows не дала изменить настройку открытия папок", "Windows didn't allow changing how folders open").into());
        }
    } else {
        for c in CLASSES {
            delete_tree(&format!(r"{c}\{VERB}"));
            if reg_read_string(HKEY_CURRENT_USER, c, "").as_deref() == Some(VERB) {
                let w = wide(c);
                unsafe {
                    let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(w.as_ptr()), PCWSTR::null());
                }
            }
        }
    }
    notify();
    Ok(())
}

/// При запуске: если галочка стоит, но exe перенесли, обновить путь в реестре
pub fn sync(enabled: bool) {
    if enabled && !is_enabled() {
        let _ = set(true);
    }
}
