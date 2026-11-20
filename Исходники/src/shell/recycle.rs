//! Корзина: восстановление, удаление навсегда, свойства и очистка
//!
//! Элементы корзины берутся из самой папки оболочки «Корзина», а не по путям скрытых файлов `$R…`:
//! так «Удалить» стирает элемент навсегда (а не кладёт его в корзину ещё раз), «Восстановить»
//! возвращает его на старое место, а «Свойства» показывают сведения об удалённом элементе

use std::path::{Path, PathBuf};

use windows::core::{PCSTR, PCWSTR};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, BHID_SFUIObject, IContextMenu, IEnumShellItems, IShellItem, IShellItemArray, SHCreateItemFromParsingName,
    SHCreateShellItemArrayFromIDLists, SHEmptyRecycleBinW, SHGetIDListFromObject, SHQueryRecycleBinW, CMF_NORMAL,
    CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX, SHQUERYRBINFO, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{CreatePopupMenu, DestroyMenu, SW_SHOWNORMAL};

use super::menu::Pidl;
use super::shellfolder::RECYCLE_BIN;
use crate::util::{from_pwstr, wide};

const CMIC_MASK_FLAG_NO_UI: u32 = 0x0000_0400;
const CMIC_MASK_UNICODE: u32 = 0x0000_4000;
const SHERB_NOCONFIRMATION: u32 = 0x1;

struct Item {
    /// Путь скрытого файла `$R…` (так элементы корзины хранятся в списке Oxide)
    path: PathBuf,
    pidl: Pidl,
}

fn same(a: &Path, b: &Path) -> bool {
    crate::util::lower(&a.to_string_lossy()) == crate::util::lower(&b.to_string_lossy())
}

/// Все элементы корзины со всех дисков
fn items() -> Vec<Item> {
    let mut out = vec![];
    unsafe {
        let w = wide(RECYCLE_BIN);
        let bin: IShellItem = match SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None) { Ok(b) => b, Err(_) => return out };
        let en: IEnumShellItems = match bin.BindToHandler(None, &BHID_EnumItems) { Ok(e) => e, Err(_) => return out };
        loop {
            let mut got: [Option<IShellItem>; 1] = [None];
            let mut fetched = 0u32;
            if en.Next(&mut got, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(it) = got[0].take() else { break };
            let Ok(p) = it.GetDisplayName(SIGDN_FILESYSPATH) else { continue };
            let path = PathBuf::from(from_pwstr(p));
            let Ok(raw) = SHGetIDListFromObject(&it) else { continue };
            if raw.is_null() {
                continue;
            }
            out.push(Item { path, pidl: Pidl(raw) });
        }
    }
    out
}

/// Меню оболочки для выбранных элементов корзины (то же, что у Проводника внутри корзины)
unsafe fn menu_of(sel: &[&Item]) -> Option<IContextMenu> {
    if sel.is_empty() {
        return None;
    }
    let raw: Vec<*const ITEMIDLIST> = sel.iter().map(|i| i.pidl.0 as *const _).collect();
    let arr: IShellItemArray = SHCreateShellItemArrayFromIDLists(&raw).ok()?;
    arr.BindToHandler(None, &BHID_SFUIObject).ok()
}

unsafe fn invoke(cm: &IContextMenu, verb: &str, no_ui: bool) -> bool {
    let Ok(menu) = CreatePopupMenu() else { return false };
    let _ = cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL);
    let a: Vec<u8> = verb.bytes().chain(std::iter::once(0)).collect();
    let w = wide(verb);
    let mut mask = CMIC_MASK_UNICODE;
    if no_ui {
        mask |= CMIC_MASK_FLAG_NO_UI;
    }
    let info = CMINVOKECOMMANDINFOEX {
        cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        fMask: mask,
        // Корзина спрашивает подтверждение, если ей дать окно, даже с CMIC_MASK_FLAG_NO_UI.
        // Подтверждение уже спросил Oxide, поэтому без окна - иначе вопрос задаётся дважды
        hwnd: if no_ui { Default::default() } else { super::window::hwnd() },
        lpVerb: PCSTR(a.as_ptr()),
        lpVerbW: PCWSTR(w.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    let ok = cm.InvokeCommand(&info as *const _ as *const CMINVOKECOMMANDINFO).is_ok();
    let _ = DestroyMenu(menu);
    ok
}

/// Глагол оболочки для элементов корзины, заданных путями `$R…`
fn run(paths: &[PathBuf], verb: &str, no_ui: bool) -> bool {
    let all = items();
    let sel: Vec<&Item> = all.iter().filter(|i| paths.iter().any(|p| same(p, &i.path))).collect();
    unsafe {
        match menu_of(&sel) {
            Some(cm) => invoke(&cm, verb, no_ui),
            None => false,
        }
    }
}

/// Парный файл сведений `$I…` для файла `$R…`
fn info_file(r: &Path) -> Option<PathBuf> {
    let name = r.file_name()?.to_string_lossy().to_string();
    let rest = name.strip_prefix("$R").or_else(|| name.strip_prefix("$r"))?;
    Some(r.with_file_name(format!("$I{rest}")))
}

/// Убирает `$I…`, у которых больше нет `$R…`: элемента в корзине уже нет, а сведения
/// о нём оболочка иногда оставляет. Возвращает, все ли элементы ушли из корзины
fn tidy_now(paths: &[PathBuf]) -> bool {
    let mut all_gone = true;
    for r in paths {
        if r.exists() {
            all_gone = false;
            continue;
        }
        if let Some(i) = info_file(r) {
            let _ = std::fs::remove_file(i);
        }
    }
    all_gone
}

/// То же в фоне: восстановление оболочка может заканчивать не сразу
fn tidy_later(paths: Vec<PathBuf>) {
    std::thread::Builder::new()
        .name("bin-tidy".into())
        .spawn(move || {
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if tidy_now(&paths) {
                    break;
                }
            }
        })
        .ok();
}

/// «Восстановить»: вернуть элементы туда, откуда их удалили
pub fn restore(paths: &[PathBuf]) -> bool {
    let ok = run(paths, "undelete", false);
    tidy_later(paths.to_vec());
    ok
}

/// «Восстановить все объекты»
pub fn restore_all() -> bool {
    let all = items();
    let sel: Vec<&Item> = all.iter().collect();
    let ok = unsafe {
        match menu_of(&sel) {
            Some(cm) => invoke(&cm, "undelete", false),
            None => false,
        }
    };
    tidy_later(all.iter().map(|i| i.path.clone()).collect());
    ok
}

/// Удалить навсегда. Подтверждение уже спросил Oxide: глагол «delete» корзины спрашивает
/// ещё раз в любом случае, поэтому удаляем через IFileOperation без вопросов (прогресс Windows остаётся)
pub fn delete_forever_sync(paths: &[PathBuf]) -> bool {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    use windows::Win32::UI::Shell::{FileOperation, IFileOperation, FILEOPERATION_FLAGS};
    const FOF_NOCONFIRMATION: u32 = 0x0010;
    const FOF_NOCONFIRMMKDIR: u32 = 0x0200;
    const FOFX_SHOWELEVATIONPROMPT: u32 = 0x0004_0000;
    let all = items();
    let sel: Vec<&Item> = all.iter().filter(|i| paths.iter().any(|p| same(p, &i.path))).collect();
    if sel.is_empty() {
        return false;
    }
    unsafe {
        let raw: Vec<*const ITEMIDLIST> = sel.iter().map(|i| i.pidl.0 as *const _).collect();
        let Ok(arr) = SHCreateShellItemArrayFromIDLists(&raw) else { return false };
        let Ok(fo) = CoCreateInstance::<_, IFileOperation>(&FileOperation, None, CLSCTX_ALL) else { return false };
        let owner = super::window::hwnd();
        if owner != Default::default() {
            let _ = fo.SetOwnerWindow(owner);
        }
        if fo.SetOperationFlags(FILEOPERATION_FLAGS(FOF_NOCONFIRMATION | FOF_NOCONFIRMMKDIR | FOFX_SHOWELEVATIONPROMPT)).is_err() {
            return false;
        }
        if fo.DeleteItems(&arr).is_err() {
            return false;
        }
        let ok = fo.PerformOperations().is_ok();
        tidy_now(&sel.iter().map(|i| i.path.clone()).collect::<Vec<_>>());
        ok
    }
}

/// То же в отдельном потоке, чтобы окно не замирало на больших папках
pub fn delete_forever(paths: Vec<PathBuf>, done: impl FnOnce(bool) + Send + 'static) {
    std::thread::Builder::new()
        .name("bin-delete".into())
        .spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            }
            done(delete_forever_sync(&paths));
        })
        .ok();
}

/// Свойства удалённых элементов (исходное расположение, дата удаления)
pub fn properties(paths: &[PathBuf]) -> bool {
    run(paths, "properties", false)
}

/// «Вырезать» из корзины: при вставке элемент восстанавливается в новое место
pub fn cut(paths: &[PathBuf]) -> bool {
    run(paths, "cut", false)
}

/// «Показать дополнительные параметры» для элементов корзины
pub fn show_menu(paths: &[PathBuf], screen: (i32, i32), extended: bool) -> super::menu::MenuOutcome {
    let all = items();
    let sel: Vec<&Item> = all.iter().filter(|i| paths.iter().any(|p| same(p, &i.path))).collect();
    unsafe {
        match menu_of(&sel) {
            Some(cm) => super::menu::track_menu(&cm, &[], None, screen, extended),
            None => super::menu::MenuOutcome::None,
        }
    }
}

/// Сколько элементов в корзине и сколько они занимают
pub fn info() -> (u64, u64) {
    let mut q = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    unsafe {
        if SHQueryRecycleBinW(PCWSTR::null(), &mut q).is_ok() {
            return (q.i64NumItems.max(0) as u64, q.i64Size.max(0) as u64);
        }
    }
    (0, 0)
}

/// Очистить корзину. Подтверждение уже спросил Oxide
pub fn empty_now() {
    unsafe {
        let _ = SHEmptyRecycleBinW(super::window::hwnd(), PCWSTR::null(), SHERB_NOCONFIRMATION);
    }
}

/// Откуда удалён элемент и когда: читается из парного файла `$I…`
fn origin_of(r: &Path) -> Option<(PathBuf, u64)> {
    let b = std::fs::read(info_file(r)?).ok()?;
    if b.len() < 28 {
        return None;
    }
    let ver = u64::from_le_bytes(b[0..8].try_into().ok()?);
    let time = u64::from_le_bytes(b[16..24].try_into().ok()?);
    let units: Vec<u16> = match ver {
        2 => {
            let n = u32::from_le_bytes(b[24..28].try_into().ok()?) as usize;
            b.get(28..28 + n * 2)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
        }
        1 => b.get(24..24 + 520)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
        _ => return None,
    };
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    Some((PathBuf::from(String::from_utf16_lossy(&units[..end])), time))
}

/// Отмена удаления (Ctrl+Z): восстановить последние удалённые копии этих путей
pub fn restore_originals(originals: &[PathBuf]) -> bool {
    let all = items();
    let mut pick: Vec<(usize, u64)> = vec![];
    for o in originals {
        let mut best: Option<(usize, u64)> = None;
        for (i, it) in all.iter().enumerate() {
            if let Some((orig, t)) = origin_of(&it.path) {
                if same(&orig, o) && best.map(|b| t > b.1).unwrap_or(true) {
                    best = Some((i, t));
                }
            }
        }
        if let Some(b) = best {
            pick.push(b);
        }
    }
    if pick.is_empty() {
        return false;
    }
    let sel: Vec<&Item> = pick.iter().map(|(i, _)| &all[*i]).collect();
    let ok = unsafe {
        match menu_of(&sel) {
            Some(cm) => invoke(&cm, "undelete", false),
            None => false,
        }
    };
    tidy_later(sel.iter().map(|i| i.path.clone()).collect());
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recycle(p: &Path) {
        let mut ops = crate::shell::ops::Op::Delete { paths: vec![p.to_path_buf()], permanent: false };
        unsafe { crate::shell::ops::perform_for_test(&mut ops) };
    }

    fn find(orig: &Path) -> Option<PathBuf> {
        items().into_iter().find(|i| origin_of(&i.path).map(|(o, _)| same(&o, orig)).unwrap_or(false)).map(|i| i.path)
    }

    /// Ручная проверка на настоящей корзине (трогает только свой временный файл):
    /// cargo test --release -- --ignored bin_roundtrip
    #[test]
    #[ignore]
    fn bin_roundtrip() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        let dir = std::env::temp_dir().join(format!("oxide-bin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("oxide-bin-test.txt");
        std::fs::write(&f, "hello").unwrap();

        // В корзину и обратно через «Восстановить»
        recycle(&f);
        assert!(!f.exists());
        let r = find(&f).expect("элемент в корзине");
        assert!(restore(&[r]));
        assert!(f.exists(), "восстановлен");

        // Отмена удаления по исходному пути
        recycle(&f);
        assert!(restore_originals(&[f.clone()]));
        assert!(f.exists(), "восстановлен по исходному пути");

        // Удаление навсегда без второго окна
        recycle(&f);
        let r = find(&f).expect("элемент в корзине");
        assert!(delete_forever_sync(&[r.clone()]));
        let mut gone = false;
        for _ in 0..50 {
            if find(&f).is_none() {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(gone, "удалён из корзины");
        assert!(!r.exists());
        // Сведений `$I…` о тестовом файле в корзине не осталось
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let bin_dir = r.parent().unwrap().to_path_buf();
        let left = std::fs::read_dir(&bin_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("$I"))
            .filter(|e| {
                let b = std::fs::read(e.path()).unwrap_or_default();
                let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                String::from_utf16_lossy(&u).contains("oxide-bin-test")
            })
            .count();
        assert_eq!(left, 0, "остались $I");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
