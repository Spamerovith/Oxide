//! Файловые операции через IFileOperation (родные диалоги прогресса, конфликтов, UAC, Корзина)
//! в отдельном STA-потоке, чтобы окно не зависало

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, FILEOPERATION_FLAGS,
};

use crate::util::{wide, wide_path};

const FOF_RENAMEONCOLLISION: u32 = 0x0008;
const FOF_NOCONFIRMATION: u32 = 0x0010;
const FOF_ALLOWUNDO: u32 = 0x0040;
const FOF_NOCONFIRMMKDIR: u32 = 0x0200;
const FOF_WANTNUKEWARNING: u32 = 0x4000;
const FOFX_SHOWELEVATIONPROMPT: u32 = 0x0004_0000;
const FOFX_RECYCLEONDELETE: u32 = 0x0008_0000;
const FOFX_ADDUNDORECORD: u32 = 0x2000_0000;

#[derive(Clone, Debug)]
pub enum Op {
    Copy { src: Vec<PathBuf>, dst: PathBuf },
    Move { src: Vec<PathBuf>, dst: PathBuf },
    Delete { paths: Vec<PathBuf>, permanent: bool },
    Rename { path: PathBuf, new_name: String },
}

#[derive(Debug)]
pub struct OpDone {
    pub op: Op,
    pub ok: bool,
    pub aborted: bool,
    pub error: Option<String>,
}

pub struct Ops {
    tx: Sender<Op>,
    pub rx: Receiver<OpDone>,
}

impl Ops {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx_ops) = channel::<Op>();
        let (tx_done, rx) = channel::<OpDone>();
        std::thread::Builder::new()
            .name("fileops".into())
            .spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                }
                while let Ok(op) = rx_ops.recv() {
                    let res = unsafe { perform(&op) };
                    let done = match res {
                        Ok(aborted) => OpDone { op, ok: !aborted, aborted, error: None },
                        Err(e) => OpDone { op, ok: false, aborted: false, error: Some(e) },
                    };
                    let _ = tx_done.send(done);
                    ctx.request_repaint();
                }
            })
            .ok();
        Ops { tx, rx }
    }

    pub fn run(&self, op: Op) {
        let _ = self.tx.send(op);
    }
}

unsafe fn item(p: &Path) -> Result<IShellItem, String> {
    let w = wide_path(p);
    SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).map_err(|e| format!("{}: {}", p.display(), e.message()))
}

unsafe fn perform(op: &Op) -> Result<bool, String> {
    let fo: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.message())?;
    let owner = super::window::hwnd();
    if owner != HWND::default() {
        let _ = fo.SetOwnerWindow(owner);
    }
    let mut flags = FOF_ALLOWUNDO | FOF_NOCONFIRMMKDIR | FOFX_ADDUNDORECORD | FOFX_SHOWELEVATIONPROMPT;
    match op {
        Op::Copy { src, dst } => {
            if src.iter().any(|s| s.parent() == Some(dst.as_path())) {
                flags |= FOF_RENAMEONCOLLISION;
            }
            fo.SetOperationFlags(FILEOPERATION_FLAGS(flags)).map_err(|e| e.message())?;
            let d = item(dst)?;
            for s in src {
                fo.CopyItem(&item(s)?, &d, PCWSTR::null(), None).map_err(|e| e.message())?;
            }
        }
        Op::Move { src, dst } => {
            fo.SetOperationFlags(FILEOPERATION_FLAGS(flags)).map_err(|e| e.message())?;
            let d = item(dst)?;
            for s in src {
                if s.parent() == Some(dst.as_path()) {
                    continue;
                }
                fo.MoveItem(&item(s)?, &d, PCWSTR::null(), None).map_err(|e| e.message())?;
            }
        }
        Op::Delete { paths, permanent } => {
            if *permanent {
                flags = (flags & !FOF_ALLOWUNDO) | FOF_WANTNUKEWARNING;
            } else {
                flags |= FOFX_RECYCLEONDELETE | FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING;
            }
            fo.SetOperationFlags(FILEOPERATION_FLAGS(flags)).map_err(|e| e.message())?;
            for p in paths {
                fo.DeleteItem(&item(p)?, None).map_err(|e| e.message())?;
            }
        }
        Op::Rename { path, new_name } => {
            fo.SetOperationFlags(FILEOPERATION_FLAGS(flags)).map_err(|e| e.message())?;
            let n = wide(new_name);
            fo.RenameItem(&item(path)?, PCWSTR(n.as_ptr()), None).map_err(|e| e.message())?;
        }
    }
    match fo.PerformOperations() {
        Ok(()) => {}
        Err(e) => {
            // ERROR_CANCELLED / COPYENGINE_E_USER_CANCELLED - пользователь отменил
            let code = e.code().0 as u32;
            if code == 0x800704C7 || code == 0x80270000 {
                return Ok(true);
            }
            return Err(e.message());
        }
    }
    let aborted = fo.GetAnyOperationsAborted().map(|b| b.as_bool()).unwrap_or(false);
    Ok(aborted)
}

/// Создание папки (синхронно - мгновенная операция)
pub fn create_folder(dir: &Path) -> Result<PathBuf, String> {
    let p = super::fs::unique_name(dir, tr("Новая папка"), "");
    std::fs::create_dir(super::fs::long_path(&p)).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Позиция в списке «Создать» из реестра ShellNew
#[derive(Clone, Debug)]
pub struct NewItem {
    pub ext: String,
    pub title: String,
    pub kind: NewKind,
}

#[derive(Clone, Debug)]
pub enum NewKind {
    Null,
    File(PathBuf),
    Data(Vec<u8>),
}

/// Читает список шаблонов «Создать» так же, как Проводник
pub fn shellnew_items() -> Vec<NewItem> {
    use crate::util::{reg_read_binary, reg_read_multi, reg_read_string};
    use windows::Win32::System::Registry::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};
    let mut exts = reg_read_multi(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Discardable\PostSetup\ShellNew",
        "Classes",
    );
    if exts.is_empty() {
        exts = vec![".txt".into(), ".rtf".into(), ".zip".into(), ".bmp".into()];
    }
    let mut out = vec![];
    for ext in exts {
        if !ext.starts_with('.') || ext.eq_ignore_ascii_case(".lnk") || ext.eq_ignore_ascii_case(".library-ms") {
            continue;
        }
        let progid = reg_read_string(HKEY_CLASSES_ROOT, &ext, "").unwrap_or_default();
        let mut keys = vec![format!(r"{ext}\ShellNew")];
        if !progid.is_empty() {
            keys.push(format!(r"{ext}\{progid}\ShellNew"));
        }
        let mut kind = None;
        for k in &keys {
            if reg_read_string(HKEY_CLASSES_ROOT, k, "NullFile").is_some() || reg_has_value(k, "NullFile") {
                kind = Some(NewKind::Null);
                break;
            }
            if let Some(f) = reg_read_string(HKEY_CLASSES_ROOT, k, "FileName") {
                let p = PathBuf::from(&f);
                let p = if p.is_absolute() {
                    p
                } else {
                    let win = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
                    let a = PathBuf::from(&win).join("ShellNew").join(&f);
                    if a.exists() {
                        a
                    } else {
                        p
                    }
                };
                kind = Some(NewKind::File(p));
                break;
            }
            if let Some(d) = reg_read_binary(HKEY_CLASSES_ROOT, k, "Data") {
                kind = Some(NewKind::Data(d));
                break;
            }
        }
        let Some(kind) = kind else { continue };
        let title = super::fs::type_name(&ext[1..].to_lowercase(), false);
        out.push(NewItem { ext: ext.to_lowercase(), title, kind });
    }
    out.sort_by(|a, b| crate::util::natural_cmp(&a.title, &b.title));
    out.dedup_by(|a, b| a.title == b.title);
    out
}

fn reg_has_value(sub: &str, name: &str) -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CLASSES_ROOT, RRF_RT_ANY};
    let s = wide(sub);
    let n = wide(name);
    unsafe { RegGetValueW(HKEY_CLASSES_ROOT, PCWSTR(s.as_ptr()), PCWSTR(n.as_ptr()), RRF_RT_ANY, None, None, None).is_ok() }
}

pub fn create_new_item(dir: &Path, it: &NewItem) -> Result<PathBuf, String> {
    let base = format!("{}", it.title);
    let p = super::fs::unique_name(dir, &base, &it.ext);
    match &it.kind {
        NewKind::Null => std::fs::write(&p, []).map_err(|e| e.to_string())?,
        NewKind::Data(d) => std::fs::write(&p, d).map_err(|e| e.to_string())?,
        NewKind::File(src) => {
            if std::fs::copy(src, &p).is_err() {
                std::fs::write(&p, []).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(p)
}

pub fn create_text_file(dir: &Path) -> Result<PathBuf, String> {
    let p = super::fs::unique_name(dir, tr("Текстовый документ"), ".txt");
    std::fs::write(&p, []).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Сжатие в ZIP (фоново)
pub fn compress_zip(items: &[PathBuf], out: &Path) -> Result<(), String> {
    use std::io::Write;
    let f = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut z = zip::ZipWriter::new(std::io::BufWriter::new(f));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    fn add(z: &mut zip::ZipWriter<std::io::BufWriter<std::fs::File>>, p: &Path, rel: &str, opts: zip::write::SimpleFileOptions) -> Result<(), String> {
        if p.is_dir() {
            z.add_directory(format!("{rel}/"), opts).map_err(|e| e.to_string())?;
            for e in std::fs::read_dir(p).map_err(|e| e.to_string())?.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                add(z, &e.path(), &format!("{rel}/{name}"), opts)?;
            }
        } else {
            z.start_file(rel, opts).map_err(|e| e.to_string())?;
            let mut src = std::fs::File::open(p).map_err(|e| e.to_string())?;
            std::io::copy(&mut src, z).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    for p in items {
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        add(&mut z, p, &name, opts)?;
    }
    let mut w = z.finish().map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn extract_zip(zipf: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(zipf).map_err(|e| e.to_string())?;
    let mut a = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    a.extract(dest).map_err(|e| e.to_string())
}

/// «Создать ярлыки»: ярлык на каждый файл в папке dst («Имя - Ярлык.lnk», как в Проводнике)
pub fn create_shortcuts(src: &[PathBuf], dst: &Path) -> Result<(), String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::IPersistFile;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    for f in src {
        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.to_string_lossy().replace([':', '\\'], ""));
        let base = format!("{name}{}", crate::i18n::l(" - Ярлык", " - Shortcut"));
        let lnk = super::fs::unique_name(dst, &base, ".lnk");
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_ALL).map_err(|e| e.message())?;
            let target = wide_path(f);
            link.SetPath(PCWSTR(target.as_ptr())).map_err(|e| e.message())?;
            if let Some(dir) = f.parent() {
                let d = wide_path(dir);
                let _ = link.SetWorkingDirectory(PCWSTR(d.as_ptr()));
            }
            let pf: IPersistFile = link.cast().map_err(|e| e.message())?;
            let out = wide_path(&lnk);
            pf.Save(PCWSTR(out.as_ptr()), true).map_err(|e| e.message())?;
        }
    }
    Ok(())
}
