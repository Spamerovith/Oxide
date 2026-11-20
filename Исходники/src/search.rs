//! Мгновенный поиск по всем локальным дискам и мгновенный размер папок
//!
//! Индекс имён строится в фоне с низким приоритетом и пишется потоком прямо на диск
//! (%LOCALAPPDATA%\Oxide\index). Для поиска файлы индекса отображаются в память (memory-mapped):
//! страницы общие и сбрасываются системой, поэтому личная память процесса почти не растёт
//!
//! Для каждой папки индекс помнит время изменения и сколько занимает всё её содержимое. Обновление
//! не начинается с нуля: папки, которые не менялись, переносятся из готового индекса, заново читаются
//! только изменённые. Изменения, пойманные наблюдателем ReadDirectoryChangesW, сохраняются на диск
//! и переживают перезапуск. Служебные папки (Windows, $Recycle.Bin…) не индексируются - поиск в них
//! идёт напрямую по диску

use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetFileSizeEx, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, OPEN_EXISTING,
};
use windows::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READONLY};

use crate::shell::fs::{dir_mtime, for_each_child, for_each_child_full, ATTR_DIR, ATTR_HIDDEN, ATTR_REPARSE};
use crate::util::{fold, lower};

const NONE: u32 = u32::MAX;
/// Запись имени: смещение имени, родитель, длина имени, флаги, номер папки в dirs.bin
const REC: usize = 16;
/// Запись папки в dirs.bin
const DREC: usize = 64;
const F_DIR: u8 = 1;
const F_HIDDEN: u8 = 2;
const F_REPARSE: u8 = 4;
const FORMAT: u32 = 3;
/// Если индекс старше этого, при запуске проверяем, что изменилось, пока Oxide был закрыт
const CHECK_AFTER_SECS: u64 = 3600;

/// Файл, отображённый в память только для чтения
struct Mapped {
    ptr: *const u8,
    len: usize,
    file: HANDLE,
    map: HANDLE,
}
unsafe impl Send for Mapped {}
unsafe impl Sync for Mapped {}

impl Mapped {
    fn open(p: &Path) -> Option<Mapped> {
        unsafe {
            let w = crate::util::wide_path(p);
            let file = CreateFileW(PCWSTR(w.as_ptr()), GENERIC_READ.0, FILE_SHARE_READ | FILE_SHARE_DELETE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None).ok()?;
            let mut size = 0i64;
            if GetFileSizeEx(file, &mut size).is_err() || size == 0 {
                let _ = CloseHandle(file);
                return if size == 0 { Some(Mapped { ptr: std::ptr::null(), len: 0, file: HANDLE::default(), map: HANDLE::default() }) } else { None };
            }
            let map = match CreateFileMappingW(file, None, PAGE_READONLY, 0, 0, PCWSTR::null()) {
                Ok(m) => m,
                Err(_) => {
                    let _ = CloseHandle(file);
                    return None;
                }
            };
            let view = MapViewOfFile(map, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                let _ = CloseHandle(map);
                let _ = CloseHandle(file);
                return None;
            }
            Some(Mapped { ptr: view.Value as *const u8, len: size as usize, file, map })
        }
    }
    fn bytes(&self) -> &[u8] {
        if self.ptr.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl Drop for Mapped {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.ptr as *mut _ });
            }
            if !self.map.is_invalid() && self.map != HANDLE::default() {
                let _ = CloseHandle(self.map);
            }
            if !self.file.is_invalid() && self.file != HANDLE::default() {
                let _ = CloseHandle(self.file);
            }
        }
    }
}

pub struct Index {
    names: Mapped,
    recs: Mapped,
    dirs: Mapped,
    n: usize,
    nd: usize,
    /// Первые записи - корни дисков
    nroots: usize,
    dir: PathBuf,
    pub built: u64,
}

#[derive(Clone, Copy)]
struct Rec {
    off: u32,
    parent: u32,
    len: u16,
    flags: u8,
    dir: u32,
}

#[derive(Clone, Copy, Default)]
struct DirRec {
    first: u32,
    nkids: u32,
    own_files: u32,
    mtime: u64,
    own_size: u64,
    own_alloc: u64,
    tot_size: u64,
    tot_alloc: u64,
    tot_files: u32,
    tot_dirs: u32,
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

impl Index {
    fn rec(&self, i: u32) -> Rec {
        let b = &self.recs.bytes()[i as usize * REC..i as usize * REC + REC];
        Rec { off: u32_at(b, 0), parent: u32_at(b, 4), len: u16::from_le_bytes([b[8], b[9]]), flags: b[10], dir: u32_at(b, 12) }
    }
    fn drec(&self, d: u32) -> DirRec {
        let b = &self.dirs.bytes()[d as usize * DREC..d as usize * DREC + DREC];
        DirRec {
            first: u32_at(b, 4),
            nkids: u32_at(b, 8),
            own_files: u32_at(b, 12),
            mtime: u64_at(b, 16),
            own_size: u64_at(b, 24),
            own_alloc: u64_at(b, 32),
            tot_size: u64_at(b, 40),
            tot_alloc: u64_at(b, 48),
            tot_files: u32_at(b, 56),
            tot_dirs: u32_at(b, 60),
        }
    }
    fn name_of(&self, r: &Rec) -> &str {
        let b = &self.names.bytes()[r.off as usize..r.off as usize + r.len as usize];
        // Записываем только корректный UTF-8
        unsafe { std::str::from_utf8_unchecked(b) }
    }
    fn path(&self, mut i: u32) -> String {
        let mut parts = vec![];
        while i != NONE {
            let r = self.rec(i);
            parts.push(self.name_of(&r));
            i = r.parent;
        }
        parts.reverse();
        let mut s = String::new();
        for (k, p) in parts.iter().enumerate() {
            if k == 0 {
                s.push_str(p.trim_end_matches('\\'));
            }
            if k > 0 {
                s.push('\\');
                s.push_str(p);
            }
        }
        if parts.len() == 1 {
            s.push('\\');
        }
        s
    }
    /// Номер записи папки по пути (без учёта регистра)
    fn find_dir(&self, path: &Path) -> Option<u32> {
        let s = path.to_string_lossy();
        let mut comps = s.trim_end_matches('\\').split('\\').filter(|c| !c.is_empty());
        let drive = lower(comps.next()?);
        let mut cur = (0..self.nroots as u32).find(|&i| lower(self.name_of(&self.rec(i)).trim_end_matches('\\')) == drive)?;
        for c in comps {
            let r = self.rec(cur);
            if r.dir == NONE {
                return None;
            }
            let d = self.drec(r.dir);
            let want = lower(c);
            cur = (d.first..d.first + d.nkids).find(|&k| {
                let kr = self.rec(k);
                kr.flags & F_DIR != 0 && lower(self.name_of(&kr)) == want
            })?;
        }
        Some(cur)
    }
    pub fn len(&self) -> usize {
        self.n
    }
    /// Размер файлов индекса на диске
    pub fn disk_bytes(&self) -> usize {
        self.names.len + self.recs.len + self.dirs.len
    }
}

/// Содержимое папки по индексу
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FolderStats {
    pub size: u64,
    pub alloc: u64,
    pub files: u64,
    pub dirs: u64,
}

fn index_root() -> Option<PathBuf> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    Some(PathBuf::from(base).join("Oxide").join("index"))
}

/// Папки верхнего уровня диска, которые не индексируются (поиск в них идёт напрямую)
fn excluded_top(name: &str, system_drive: bool) -> bool {
    let l = name.to_ascii_lowercase();
    matches!(l.as_str(), "$recycle.bin" | "system volume information" | "$winreagent" | "config.msi" | "$windows.~bt" | "$windows.~ws" | "$sysreset" | "recovery")
        || (system_drive && l == "windows")
}

fn system_drive_letter() -> char {
    std::env::var("SystemDrive").ok().and_then(|s| s.chars().next()).unwrap_or('C').to_ascii_uppercase()
}

/// Изменения после построения индекса: учитываются в поиске и сохраняются на диск
#[derive(Default)]
struct Changes {
    /// Появившиеся: путь, папка?, номер изменения
    added: Vec<(String, bool, u64)>,
    /// Исчезнувшие (нижний регистр)
    removed: HashMap<String, u64>,
    /// Папки, содержимое которых менялось (нижний регистр): при обновлении читаются заново
    dirty: HashMap<String, u64>,
    seq: u64,
    unsaved: bool,
}

const MAX_CHANGES: usize = 200_000;

impl Changes {
    fn touch_dir(&mut self, p: &Path) {
        if let Some(d) = p.parent() {
            if self.dirty.len() < MAX_CHANGES {
                self.seq += 1;
                self.dirty.insert(lower(&d.to_string_lossy()), self.seq);
            }
        }
    }
    fn add(&mut self, s: String, d: bool) {
        self.seq += 1;
        self.removed.remove(&lower(&s));
        if self.added.len() < MAX_CHANGES {
            self.added.push((s, d, self.seq));
        }
    }
    fn remove(&mut self, s: &str) {
        self.seq += 1;
        if let Some(pos) = self.added.iter().rposition(|(a, _, _)| a == s) {
            self.added.swap_remove(pos);
        } else if self.removed.len() < MAX_CHANGES {
            self.removed.insert(lower(s), self.seq);
        }
    }
    /// Оставить только то, что случилось после начала обновления
    fn keep_after(&mut self, seq: u64) {
        self.added.retain(|a| a.2 > seq);
        self.removed.retain(|_, s| *s > seq);
        self.dirty.retain(|_, s| *s > seq);
        self.unsaved = true;
    }
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.dirty.is_empty()
    }
    fn file() -> Option<PathBuf> {
        Some(index_root()?.join("changes.txt"))
    }
    fn save(&mut self) {
        let Some(f) = Self::file() else { return };
        let mut s = String::new();
        for (p, d, _) in &self.added {
            s.push_str(if *d { "A\t1\t" } else { "A\t0\t" });
            s.push_str(p);
            s.push('\n');
        }
        for p in self.removed.keys() {
            s.push_str("R\t");
            s.push_str(p);
            s.push('\n');
        }
        for p in self.dirty.keys() {
            s.push_str("D\t");
            s.push_str(p);
            s.push('\n');
        }
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, s).is_ok() && std::fs::rename(&tmp, &f).is_ok() {
            self.unsaved = false;
        }
    }
    fn load() -> Changes {
        let mut c = Changes::default();
        let Some(f) = Self::file() else { return c };
        let Ok(s) = std::fs::read_to_string(f) else { return c };
        for line in s.lines() {
            let mut it = line.splitn(3, '\t');
            match it.next() {
                Some("A") => {
                    let d = it.next() == Some("1");
                    if let Some(p) = it.next() {
                        c.added.push((p.to_string(), d, 0));
                    }
                }
                Some("R") => {
                    if let Some(p) = it.next() {
                        c.removed.insert(p.to_string(), 0);
                    }
                }
                Some("D") => {
                    if let Some(p) = it.next() {
                        c.dirty.insert(p.to_string(), 0);
                    }
                }
                _ => {}
            }
        }
        c
    }
}

pub struct SearchEngine {
    pub index: Arc<RwLock<Option<Index>>>,
    pub building: Arc<AtomicBool>,
    /// Сколько папок уже просмотрено при построении или проверке
    pub progress: Arc<AtomicUsize>,
    /// Сколько папок в прежнем индексе (0 - строится с нуля)
    pub progress_total: Arc<AtomicUsize>,
    changes: Mutex<Changes>,
    watchers: Mutex<Vec<notify::RecommendedWatcher>>,
    started: AtomicBool,
}

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine {
            index: Arc::new(RwLock::new(None)),
            building: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicUsize::new(0)),
            progress_total: Arc::new(AtomicUsize::new(0)),
            changes: Mutex::new(Changes::default()),
            watchers: Mutex::new(vec![]),
            started: AtomicBool::new(false),
        }
    }

    pub fn ready(&self) -> bool {
        self.index.read().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Объектов в индексе с учётом изменений, размер на диске, время построения
    pub fn info(&self) -> Option<(usize, usize, u64)> {
        let g = self.index.read().ok()?;
        let ix = g.as_ref()?;
        let c = self.changes.lock().ok()?;
        Some(((ix.len() + c.added.len()).saturating_sub(c.removed.len()), ix.disk_bytes(), ix.built))
    }

    /// Размер содержимого папки по индексу (мгновенно). None, если папки нет в индексе
    pub fn folder_stats(&self, path: &Path) -> Option<FolderStats> {
        if !self.covers(path) {
            return None;
        }
        let g = self.index.read().ok()?;
        let ix = g.as_ref()?;
        let r = ix.rec(ix.find_dir(path)?);
        if r.dir == NONE || r.flags & F_REPARSE != 0 {
            return None;
        }
        let d = ix.drec(r.dir);
        Some(FolderStats { size: d.tot_size, alloc: d.tot_alloc, files: d.tot_files as u64, dirs: d.tot_dirs as u64 })
    }

    /// Загружает готовый индекс с диска (мгновенно) и при необходимости обновляет его в фоне
    pub fn start(self: &Arc<Self>, ctx: egui::Context) {
        if self.started.swap(true, Ordering::SeqCst) {
            if !self.ready() {
                self.build(ctx, false);
            }
            return;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("index-load".into())
            .spawn(move || {
                let drives = fixed_drives();
                let mut check = true;
                if let Some(ix) = load_latest() {
                    let age = crate::util::now_filetime().saturating_sub(ix.built) / 10_000_000;
                    check = age > CHECK_AFTER_SECS;
                    *me.index.write().unwrap() = Some(ix);
                    *me.changes.lock().unwrap() = Changes::load();
                    ctx.request_repaint();
                } else if let Some(f) = Changes::file() {
                    let _ = std::fs::remove_file(f);
                }
                me.start_watchers(&drives);
                me.start_saver();
                if check {
                    // Даём приложению спокойно запуститься
                    std::thread::sleep(std::time::Duration::from_secs(if me.ready() { 30 } else { 3 }));
                    me.build(ctx.clone(), false);
                }
                // Раз в 6 часов переносим накопленные изменения в сам индекс
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
                    let pending = me.changes.lock().map(|c| !c.is_empty()).unwrap_or(false);
                    if pending {
                        me.build(ctx.clone(), false);
                    }
                }
            })
            .ok();
    }

    /// Сохраняет изменения на диск раз в 20 секунд, если они есть
    fn start_saver(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::Builder::new()
            .name("index-save".into())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(20));
                if me.ready() {
                    if let Ok(mut c) = me.changes.lock() {
                        if c.unsaved {
                            c.save();
                        }
                    }
                }
            })
            .ok();
    }

    /// Обновление индекса в фоне (низкий приоритет, запись потоком на диск)
    /// `full` - построить с нуля («Перестроить индекс»), иначе взять неизменившиеся папки из готового
    pub fn build(self: &Arc<Self>, ctx: egui::Context, full: bool) {
        if self.building.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("indexer".into())
            .spawn(move || {
                unsafe {
                    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
                }
                let t0 = std::time::Instant::now();
                let drives = fixed_drives();
                // Прежний индекс открываем отдельно: поиск продолжает работать по нему, пока идёт обновление
                let old = if full { None } else { me.index.read().ok().and_then(|g| g.as_ref().map(|ix| ix.dir.clone())).and_then(|d| open_index(&d)) };
                let (seq, dirty) = {
                    let c = me.changes.lock().unwrap();
                    (c.seq, c.dirty.keys().cloned().collect::<HashSet<String>>())
                };
                me.progress.store(0, Ordering::Relaxed);
                me.progress_total.store(old.as_ref().map(|o| o.nd).unwrap_or(0), Ordering::Relaxed);
                let res = build_to_disk(&drives, old.as_ref(), &dirty, &me.progress);
                drop(old);
                match res {
                    Some((dir, reused)) => {
                        if let Some(ix) = open_index(&dir) {
                            crate::util::log(&format!(
                                "index built: {} recs, {} dirs ({} reused), {} MB on disk, {:?}",
                                ix.n,
                                ix.nd,
                                reused,
                                ix.disk_bytes() / 1_048_576,
                                t0.elapsed()
                            ));
                            let old = me.index.write().unwrap().replace(ix);
                            if let Ok(mut c) = me.changes.lock() {
                                c.keep_after(seq);
                                c.save();
                            }
                            if let Some(o) = old {
                                let od = o.dir.clone();
                                drop(o);
                                let _ = std::fs::remove_dir_all(od);
                            }
                            me.start_watchers(&drives);
                        }
                    }
                    None => crate::util::log("index build failed"),
                }
                me.building.store(false, Ordering::SeqCst);
                me.progress.store(0, Ordering::Relaxed);
                ctx.request_repaint();
            })
            .ok();
    }

    fn start_watchers(self: &Arc<Self>, drives: &[PathBuf]) {
        use notify::event::{ModifyKind, RenameMode};
        use notify::{EventKind, RecursiveMode, Watcher};
        let mut ws = self.watchers.lock().unwrap();
        ws.clear();
        for root in drives {
            let me = self.clone();
            let w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(ev) = res else { return };
                // Свои файлы индекса не считаем изменениями на диске
                if ev.paths.iter().any(|p| lower(&p.to_string_lossy()).contains(r"\oxide\index")) {
                    return;
                }
                let Ok(mut c) = me.changes.lock() else { return };
                for p in &ev.paths {
                    c.touch_dir(p);
                }
                match ev.kind {
                    EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                        for p in &ev.paths {
                            let d = p.is_dir();
                            if d {
                                // В новой папке тоже есть что прочитать
                                c.seq += 1;
                                let s = c.seq;
                                c.dirty.insert(lower(&p.to_string_lossy()), s);
                            }
                            c.add(p.to_string_lossy().to_string(), d);
                        }
                    }
                    EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                        for p in &ev.paths {
                            c.remove(&p.to_string_lossy());
                        }
                    }
                    EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if ev.paths.len() == 2 => {
                        c.remove(&ev.paths[0].to_string_lossy());
                        let d = ev.paths[1].is_dir();
                        c.add(ev.paths[1].to_string_lossy().to_string(), d);
                    }
                    _ => {}
                }
                c.unsaved = true;
            });
            if let Ok(mut w) = w {
                if w.watch(root, RecursiveMode::Recursive).is_ok() {
                    ws.push(w);
                }
            }
        }
    }

    /// Покрывает ли индекс эту папку (служебные папки не индексируются)
    pub fn covers(&self, root: &Path) -> bool {
        let s = root.to_string_lossy();
        let mut comps = s.trim_end_matches('\\').split('\\');
        let drive = comps.next().unwrap_or("");
        if drive.len() != 2 || !drive.ends_with(':') {
            return false;
        }
        let sys = drive.chars().next().map(|c| c.to_ascii_uppercase()) == Some(system_drive_letter());
        match comps.next() {
            Some(top) => !excluded_top(top, sys),
            None => true,
        }
    }

    /// Поиск в индексе: все слова (или маска с * ?) в имени, только внутри `root`
    pub fn query(&self, root: &Path, q: &Query, show_hidden: bool, limit: usize) -> Option<Vec<(String, bool)>> {
        if !self.covers(root) {
            return None;
        }
        let g = self.index.read().ok()?;
        let idx = g.as_ref()?;
        let root_s = lower(root.to_string_lossy().trim_end_matches('\\'));
        let root_prefix = format!("{root_s}\\");
        let n = idx.n;
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
        let chunk = n.div_ceil(threads).max(1);
        let mut hits: Vec<u32> = vec![];
        std::thread::scope(|s| {
            let mut hs = vec![];
            for t in 0..threads {
                let lo = t * chunk;
                let hi = ((t + 1) * chunk).min(n);
                if lo >= hi {
                    continue;
                }
                hs.push(s.spawn(move || {
                    let mut buf = String::new();
                    let mut out = vec![];
                    for i in lo..hi {
                        let r = idx.rec(i as u32);
                        if r.parent == NONE || (r.flags & F_HIDDEN != 0 && !show_hidden) {
                            continue;
                        }
                        let name = idx.name_of(&r);
                        buf.clear();
                        buf.extend(name.chars().map(fold));
                        if q.matches_lower(&buf) {
                            out.push(i as u32);
                            if out.len() > limit * 4 {
                                break;
                            }
                        }
                    }
                    out
                }));
            }
            for h in hs {
                if let Ok(v) = h.join() {
                    hits.extend(v);
                }
            }
        });
        let c = self.changes.lock().ok()?;
        let mut out = vec![];
        for i in hits {
            let p = idx.path(i);
            let pl = lower(&p);
            if !pl.starts_with(&root_prefix) || c.removed.contains_key(&pl) {
                continue;
            }
            out.push((p, idx.rec(i).flags & F_DIR != 0));
            if out.len() >= limit {
                break;
            }
        }
        for (p, d, _) in &c.added {
            if out.len() >= limit {
                break;
            }
            let pl = lower(p);
            if !pl.starts_with(&root_prefix) {
                continue;
            }
            let name = Path::new(p).file_name().map(|s| lower(&s.to_string_lossy())).unwrap_or_default();
            if q.matches_lower(&name) {
                out.push((p.clone(), *d));
            }
        }
        Some(out)
    }
}

fn fixed_drives() -> Vec<PathBuf> {
    crate::shell::fs::drives(false).into_iter().filter(|d| d.kind == crate::shell::fs::DRIVE_FIXED).map(|d| d.root).collect()
}

fn open_index(dir: &Path) -> Option<Index> {
    let meta = std::fs::read_to_string(dir.join("ok")).ok()?;
    let mut it = meta.split_whitespace();
    let fmt: u32 = it.next()?.parse().ok()?;
    let built: u64 = it.next()?.parse().ok()?;
    let nroots: usize = it.next()?.parse().ok()?;
    if fmt != FORMAT {
        return None;
    }
    let names = Mapped::open(&dir.join("names.bin"))?;
    let recs = Mapped::open(&dir.join("recs.bin"))?;
    let dirs = Mapped::open(&dir.join("dirs.bin"))?;
    let n = recs.len / REC;
    let nd = dirs.len / DREC;
    Some(Index { names, recs, dirs, n, nd, nroots: nroots.min(n), dir: dir.to_path_buf(), built })
}

/// Самый свежий готовый индекс; старые и недостроенные удаляются
fn load_latest() -> Option<Index> {
    let root = index_root()?;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let mut found = None;
    while let Some(d) = dirs.pop() {
        if found.is_none() {
            if let Some(ix) = open_index(&d) {
                found = Some(ix);
                continue;
            }
        }
        let _ = std::fs::remove_dir_all(&d);
    }
    found
}

struct Writer {
    names: BufWriter<std::fs::File>,
    recs: BufWriter<std::fs::File>,
    name_off: u64,
    count: u32,
    ok: bool,
}

impl Writer {
    fn push(&mut self, name: &str, flags: u8, parent: u32, dir: u32) -> u32 {
        let mut n = name;
        if n.len() > u16::MAX as usize {
            n = &n[..n.char_indices().take_while(|(i, _)| *i < 60000).last().map(|(i, _)| i).unwrap_or(0)];
        }
        if self.name_off + n.len() as u64 > u32::MAX as u64 {
            self.ok = false;
            return NONE;
        }
        let mut rec = [0u8; REC];
        rec[0..4].copy_from_slice(&(self.name_off as u32).to_le_bytes());
        rec[4..8].copy_from_slice(&parent.to_le_bytes());
        rec[8..10].copy_from_slice(&(n.len() as u16).to_le_bytes());
        rec[10] = flags;
        rec[12..16].copy_from_slice(&dir.to_le_bytes());
        self.ok &= self.names.write_all(n.as_bytes()).is_ok() && self.recs.write_all(&rec).is_ok();
        self.name_off += n.len() as u64;
        self.count += 1;
        self.count - 1
    }
}

fn flags_of(attrs: u32) -> u8 {
    let mut f = 0;
    if attrs & ATTR_DIR != 0 {
        f |= F_DIR;
    }
    if attrs & ATTR_HIDDEN != 0 {
        f |= F_HIDDEN;
    }
    if attrs & ATTR_REPARSE != 0 {
        f |= F_REPARSE;
    }
    f
}

/// Папка нового индекса, пока он строится
#[derive(Default, Clone, Copy)]
struct DirMem {
    rec: u32,
    parent: u32,
    first: u32,
    nkids: u32,
    own_files: u32,
    mtime: u64,
    own_size: u64,
    own_alloc: u64,
}

struct Job {
    path: PathBuf,
    /// Номер папки в новом индексе
    d: u32,
    /// Номер этой же папки в прежнем индексе
    old: Option<u32>,
    root: bool,
}

struct Shared {
    w: Writer,
    dirs: Vec<DirMem>,
    jobs: Vec<Job>,
    reused: usize,
}

impl Shared {
    fn new_dir(&mut self, rec: u32, parent: u32) -> u32 {
        self.dirs.push(DirMem { rec, parent, ..Default::default() });
        (self.dirs.len() - 1) as u32
    }
    /// Записывает ребёнка; для папки заводит запись и, если в неё можно зайти, задание
    fn push_child(&mut self, base: &Path, parent_d: u32, name: &str, flags: u8, old: Option<u32>) {
        let parent_rec = self.dirs[parent_d as usize].rec;
        if flags & F_DIR != 0 {
            let rec = self.w.count;
            let d = self.new_dir(rec, parent_d);
            if self.w.push(name, flags, parent_rec, d) == NONE {
                self.dirs.pop();
                return;
            }
            if flags & F_REPARSE == 0 {
                self.jobs.push(Job { path: base.join(name), d, old, root: false });
            }
        } else {
            self.w.push(name, flags, parent_rec, NONE);
        }
    }
}

/// Параллельный обход дисков с записью индекса прямо в файлы. Папки, которые не менялись
/// с прошлого раза (то же время изменения и наблюдатель ничего в них не видел), не читаются
/// с диска, а переносятся из `old`. Возвращает папку индекса и число перенесённых папок
fn build_to_disk(drives: &[PathBuf], old: Option<&Index>, dirty: &HashSet<String>, progress: &AtomicUsize) -> Option<(PathBuf, usize)> {
    let root = index_root()?;
    let dir = root.join(format!("{:020}", crate::util::now_filetime()));
    std::fs::create_dir_all(&dir).ok()?;
    let w = Writer {
        names: BufWriter::with_capacity(1 << 20, std::fs::File::create(dir.join("names.bin")).ok()?),
        recs: BufWriter::with_capacity(1 << 20, std::fs::File::create(dir.join("recs.bin")).ok()?),
        name_off: 0,
        count: 0,
        ok: true,
    };
    let sys = system_drive_letter();
    let shared = Mutex::new(Shared { w, dirs: Vec::with_capacity(old.map(|o| o.nd + 1024).unwrap_or(1 << 16)), jobs: vec![], reused: 0 });
    {
        let mut g = shared.lock().unwrap();
        for d in drives {
            let name = d.to_string_lossy().to_string();
            let rec = g.w.count;
            let di = g.new_dir(rec, NONE);
            g.w.push(&name, F_DIR, NONE, di);
            let old_d = old.and_then(|o| {
                let want = lower(name.trim_end_matches('\\'));
                (0..o.nroots as u32).map(|i| o.rec(i)).find(|r| lower(o.name_of(r).trim_end_matches('\\')) == want).map(|r| r.dir).filter(|&d| d != NONE)
            });
            g.jobs.push(Job { path: d.clone(), d: di, old: old_d, root: true });
        }
    }
    let nroots = drives.len();
    let active = AtomicUsize::new(0);
    // Не более 4 потоков, чтобы система оставалась отзывчивой
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 4);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                unsafe {
                    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
                }
                loop {
                    let job = {
                        let mut g = shared.lock().unwrap();
                        let j = g.jobs.pop();
                        if j.is_some() {
                            active.fetch_add(1, Ordering::SeqCst);
                        }
                        j
                    };
                    let Some(job) = job else {
                        if active.load(Ordering::SeqCst) == 0 {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    };
                    // Время из списка родителя NTFS обновляет с запозданием, поэтому для сравнения
                    // с прежним индексом спрашиваем саму папку. Читаем его до списка содержимого:
                    // если папка изменится во время чтения, в следующий раз она перечитается
                    let mtime = dir_mtime(&job.path).unwrap_or(0);
                    // Не менялась ли папка с прошлого раза
                    let same = match (old, job.old) {
                        (Some(o), Some(od)) if !job.root && mtime != 0 => {
                            let r = o.drec(od);
                            (r.mtime == mtime && !dirty.contains(&lower(&job.path.to_string_lossy()))).then_some((o, r))
                        }
                        _ => None,
                    };
                    if let Some((o, r)) = same {
                        let mut g = shared.lock().unwrap();
                        let first = g.w.count;
                        for k in r.first..r.first + r.nkids {
                            let c = o.rec(k);
                            let od = (c.dir != NONE).then_some(c.dir);
                            g.push_child(&job.path, job.d, o.name_of(&c), c.flags, od);
                        }
                        let dm = &mut g.dirs[job.d as usize];
                        dm.first = first;
                        dm.nkids = r.nkids;
                        dm.own_files = r.own_files;
                        dm.own_size = r.own_size;
                        dm.own_alloc = r.own_alloc;
                        dm.mtime = mtime;
                        g.reused += 1;
                    } else {
                        let sys_root = job.root && job.path.to_string_lossy().chars().next().map(|c| c.to_ascii_uppercase()) == Some(sys);
                        let mut kids: Vec<(String, u8)> = vec![];
                        let (mut files, mut size, mut alloc) = (0u32, 0u64, 0u64);
                        let _ = for_each_child_full(&job.path, |e| {
                            if e.attrs & ATTR_DIR != 0 {
                                if (job.root && excluded_top(e.name, sys_root)) || e.name == ".git" {
                                    return;
                                }
                            } else {
                                files += 1;
                                size += e.size;
                                alloc += e.alloc;
                            }
                            kids.push((e.name.to_string(), flags_of(e.attrs)));
                        });
                        // Папки-дети прежнего индекса: по ним узнаем, какие из них можно перенести
                        let old_kids: HashMap<String, u32> = match (old, job.old) {
                            (Some(o), Some(od)) => {
                                let r = o.drec(od);
                                (r.first..r.first + r.nkids)
                                    .map(|k| o.rec(k))
                                    .filter(|c| c.dir != NONE)
                                    .map(|c| (lower(o.name_of(&c)), c.dir))
                                    .collect()
                            }
                            _ => HashMap::new(),
                        };
                        let mut g = shared.lock().unwrap();
                        let first = g.w.count;
                        let n = kids.len() as u32;
                        for (name, flags) in &kids {
                            let od = if flags & F_DIR != 0 { old_kids.get(&lower(name)).copied() } else { None };
                            g.push_child(&job.path, job.d, name, *flags, od);
                        }
                        let dm = &mut g.dirs[job.d as usize];
                        dm.first = first;
                        dm.nkids = n;
                        dm.own_files = files;
                        dm.own_size = size;
                        dm.own_alloc = alloc;
                        dm.mtime = mtime;
                    }
                    progress.fetch_add(1, Ordering::Relaxed);
                    active.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }
    });
    let Shared { mut w, dirs, reused, .. } = shared.into_inner().ok()?;
    let ok = w.ok && w.names.flush().is_ok() && w.recs.flush().is_ok();
    drop(w);
    if !ok {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    // Итоги по папкам снизу вверх: дети всегда заведены позже родителя
    let mut tot: Vec<(u64, u64, u32, u32)> = dirs.iter().map(|d| (d.own_size, d.own_alloc, d.own_files, 0)).collect();
    for i in (0..dirs.len()).rev() {
        let p = dirs[i].parent;
        if p != NONE {
            let t = tot[i];
            let pt = &mut tot[p as usize];
            pt.0 += t.0;
            pt.1 += t.1;
            pt.2 = pt.2.saturating_add(t.2);
            pt.3 = pt.3.saturating_add(t.3 + 1);
        }
    }
    let mut dw = BufWriter::with_capacity(1 << 20, std::fs::File::create(dir.join("dirs.bin")).ok()?);
    let mut b = [0u8; DREC];
    let mut ok = true;
    for (d, t) in dirs.iter().zip(&tot) {
        b[0..4].copy_from_slice(&d.rec.to_le_bytes());
        b[4..8].copy_from_slice(&d.first.to_le_bytes());
        b[8..12].copy_from_slice(&d.nkids.to_le_bytes());
        b[12..16].copy_from_slice(&d.own_files.to_le_bytes());
        b[16..24].copy_from_slice(&d.mtime.to_le_bytes());
        b[24..32].copy_from_slice(&d.own_size.to_le_bytes());
        b[32..40].copy_from_slice(&d.own_alloc.to_le_bytes());
        b[40..48].copy_from_slice(&t.0.to_le_bytes());
        b[48..56].copy_from_slice(&t.1.to_le_bytes());
        b[56..60].copy_from_slice(&t.2.to_le_bytes());
        b[60..64].copy_from_slice(&t.3.to_le_bytes());
        ok &= dw.write_all(&b).is_ok();
    }
    ok &= dw.flush().is_ok();
    drop(dw);
    if !ok {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    std::fs::write(dir.join("ok"), format!("{} {} {}", FORMAT, crate::util::now_filetime(), nroots)).ok()?;
    Some((dir, reused))
}

/// Разобранный поисковый запрос
#[derive(Clone, Debug)]
pub struct Query {
    terms: Vec<String>,
    glob: Option<Vec<char>>,
}

impl Query {
    pub fn parse(s: &str) -> Self {
        let l = crate::util::lower(s.trim());
        if l.contains('*') || l.contains('?') {
            Query { terms: vec![], glob: Some(l.chars().collect()) }
        } else {
            Query { terms: l.split_whitespace().map(|t| t.trim_matches('"').to_string()).filter(|t| !t.is_empty()).collect(), glob: None }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.glob.is_none()
    }
    pub fn matches_lower(&self, name: &str) -> bool {
        if let Some(g) = &self.glob {
            let n: Vec<char> = name.chars().collect();
            return glob_match(g, &n);
        }
        self.terms.iter().all(|t| name.contains(t.as_str()))
    }
}

fn glob_match(p: &[char], s: &[char]) -> bool {
    let (mut pi, mut si) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = si;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Живой рекурсивный поиск (если папка не в индексе): отдаёт пачки результатов
pub fn live_search(root: PathBuf, q: Query, show_hidden: bool, cancel: Arc<AtomicBool>, mut emit: impl FnMut(Vec<(String, bool)>) + Send) {
    let stack = Mutex::new(vec![root]);
    let results = Mutex::new(Vec::<(String, bool)>::new());
    let active = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
    let emit = Mutex::new(&mut emit);
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
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                };
                let mut found = vec![];
                let mut sub = vec![];
                let mut buf = String::new();
                let _ = for_each_child(&dir, |name, attrs, _, _, _| {
                    if attrs & ATTR_HIDDEN != 0 && !show_hidden {
                        return;
                    }
                    buf.clear();
                    buf.extend(name.chars().map(fold));
                    let p = dir.join(name);
                    if q.matches_lower(&buf) {
                        found.push((p.to_string_lossy().to_string(), attrs & ATTR_DIR != 0));
                    }
                    if attrs & ATTR_DIR != 0 && attrs & ATTR_REPARSE == 0 {
                        sub.push(p);
                    }
                });
                stack.lock().unwrap().extend(sub);
                if !found.is_empty() {
                    let mut r = results.lock().unwrap();
                    r.extend(found);
                    if r.len() >= 64 {
                        let batch = std::mem::take(&mut *r);
                        drop(r);
                        (emit.lock().unwrap())(batch);
                    }
                }
                active.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    let rest = std::mem::take(&mut *results.lock().unwrap());
    if !rest.is_empty() && !cancel.load(Ordering::Relaxed) {
        (emit.lock().unwrap())(rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(ix: &Index, i: u32) -> (u64, u32, u32) {
        let d = ix.drec(ix.rec(i).dir);
        (d.tot_size, d.tot_files, d.tot_dirs)
    }

    fn child(ix: &Index, parent: u32, name: &str) -> u32 {
        let d = ix.drec(ix.rec(parent).dir);
        (d.first..d.first + d.nkids).find(|&k| ix.name_of(&ix.rec(k)) == name).unwrap()
    }

    /// Замер на настоящем индексе (ручной): cargo test --release -- --ignored real_refresh --nocapture
    #[test]
    #[ignore]
    fn real_refresh() {
        let old = load_latest().expect("index");
        let prog = AtomicUsize::new(0);
        let t = std::time::Instant::now();
        let (d, reused) = build_to_disk(&fixed_drives(), Some(&old), &HashSet::new(), &prog).unwrap();
        let el = t.elapsed();
        let ix = open_index(&d).unwrap();
        println!("old: {} recs {} dirs; new: {} recs {} dirs, reused {} dirs, {:?}", old.n, old.nd, ix.n, ix.nd, reused, el);
        for p in [r"E:\Claude", r"C:\Users"] {
            let t = std::time::Instant::now();
            let r = ix.find_dir(std::path::Path::new(p)).map(|i| ix.drec(ix.rec(i).dir));
            println!("{p}: lookup {:?}: {:?}", t.elapsed(), r.map(|d| (d.tot_size, d.tot_files, d.tot_dirs)));
        }
        drop(ix);
        drop(old);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn incremental_index() {
        let base = std::env::temp_dir().join(format!("oxide-ix-test-{}", std::process::id()));
        let root = base.join("root");
        std::fs::create_dir_all(root.join("a").join("b")).unwrap();
        std::fs::create_dir_all(root.join("c")).unwrap();
        std::fs::create_dir_all(root.join("x").join(".git")).unwrap();
        std::fs::write(root.join("a").join("f1"), vec![1u8; 100]).unwrap();
        std::fs::write(root.join("a").join("f2"), vec![1u8; 50]).unwrap();
        std::fs::write(root.join("a").join("b").join("f3"), vec![1u8; 7]).unwrap();
        std::fs::write(root.join("x").join(".git").join("big"), vec![1u8; 1000]).unwrap();
        std::env::set_var("LOCALAPPDATA", base.join("appdata"));
        let prog = AtomicUsize::new(0);

        let (d1, reused) = build_to_disk(&[root.clone()], None, &HashSet::new(), &prog).unwrap();
        assert_eq!(reused, 0);
        let ix = open_index(&d1).unwrap();
        // .git не индексируется
        assert_eq!(stats(&ix, 0), (157, 3, 4));
        let a = child(&ix, 0, "a");
        assert_eq!(stats(&ix, a), (157, 3, 1));

        // Новый файл в a\b: перечитываются корень и a\b, остальные переносятся
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(root.join("a").join("b").join("f4"), vec![1u8; 3]).unwrap();
        let (d2, reused) = build_to_disk(&[root.clone()], Some(&ix), &HashSet::new(), &prog).unwrap();
        let ix2 = open_index(&d2).unwrap();
        assert_eq!(reused, 3, "a, c, x");
        assert_eq!(stats(&ix2, 0), (160, 4, 4));
        let b = child(&ix2, child(&ix2, 0, "a"), "b");
        assert_eq!(stats(&ix2, b), (10, 2, 0));

        // Файл изменён без смены состава папки: наблюдатель пометил папку, она перечитывается
        std::fs::write(root.join("a").join("f1"), vec![1u8; 200]).unwrap();
        let mut dirty = HashSet::new();
        dirty.insert(lower(&root.join("a").to_string_lossy()));
        let (d3, _) = build_to_disk(&[root.clone()], Some(&ix2), &dirty, &prog).unwrap();
        let ix3 = open_index(&d3).unwrap();
        assert_eq!(stats(&ix3, 0), (260, 4, 4));
        drop((ix, ix2, ix3));
        let _ = std::fs::remove_dir_all(&base);
    }
}
