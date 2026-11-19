//! Мгновенный поиск по всем локальным дискам
//!
//! Индекс имён строится в фоне с низким приоритетом и пишется потоком прямо на диск
//! (%LOCALAPPDATA%\Oxide\index). Для поиска файлы индекса отображаются в память (memory-mapped):
//! страницы общие и сбрасываются системой, поэтому личная память процесса почти не растёт
//! При следующем запуске готовый индекс подхватывается сразу. Изменения ловит наблюдатель
//! ReadDirectoryChangesW. Служебные папки (Windows, $Recycle.Bin…) не индексируются -
//! поиск в них идёт напрямую по диску

use std::collections::HashSet;
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

use crate::shell::fs::{for_each_child, ATTR_DIR, ATTR_HIDDEN, ATTR_REPARSE, ATTR_SYSTEM};
use crate::util::fold;

const NONE: u32 = u32::MAX;
const REC: usize = 12;
const F_DIR: u8 = 1;
const F_HIDDEN: u8 = 2;
const FORMAT: u32 = 2;

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
    n: usize,
    dir: PathBuf,
    pub built: u64,
    /// Добавленные после построения: (путь, папка?)
    added: Vec<(String, bool)>,
    /// Удалённые после построения (нижний регистр)
    removed: HashSet<String>,
}

#[derive(Clone, Copy)]
struct Rec {
    off: u32,
    parent: u32,
    len: u16,
    flags: u8,
}

impl Index {
    fn rec(&self, i: u32) -> Rec {
        let b = &self.recs.bytes()[i as usize * REC..i as usize * REC + REC];
        Rec {
            off: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            parent: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            len: u16::from_le_bytes([b[8], b[9]]),
            flags: b[10],
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
    pub fn len(&self) -> usize {
        self.n + self.added.len()
    }
    /// Размер файлов индекса на диске
    pub fn disk_bytes(&self) -> usize {
        self.names.len + self.recs.len
    }
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

pub struct SearchEngine {
    pub index: Arc<RwLock<Option<Index>>>,
    pub building: Arc<AtomicBool>,
    pub progress: Arc<AtomicUsize>,
    watchers: Mutex<Vec<notify::RecommendedWatcher>>,
}

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine { index: Arc::new(RwLock::new(None)), building: Arc::new(AtomicBool::new(false)), progress: Arc::new(AtomicUsize::new(0)), watchers: Mutex::new(vec![]) }
    }

    pub fn ready(&self) -> bool {
        self.index.read().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Загружает готовый индекс с диска (мгновенно) и при необходимости обновляет его в фоне
    pub fn start(self: &Arc<Self>, ctx: egui::Context) {
        let me = self.clone();
        std::thread::Builder::new()
            .name("index-load".into())
            .spawn(move || {
                let drives = fixed_drives();
                let mut fresh = false;
                if let Some(ix) = load_latest() {
                    let age = crate::util::now_filetime().saturating_sub(ix.built) / 10_000_000;
                    fresh = age < 12 * 3600;
                    *me.index.write().unwrap() = Some(ix);
                    me.start_watchers(&drives);
                    ctx.request_repaint();
                }
                if !fresh {
                    // Даём приложению спокойно запуститься
                    std::thread::sleep(std::time::Duration::from_secs(if me.ready() { 60 } else { 3 }));
                    me.build(ctx);
                }
            })
            .ok();
    }

    /// Построение индекса в фоне (низкий приоритет, запись потоком на диск)
    pub fn build(self: &Arc<Self>, ctx: egui::Context) {
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
                let res = build_to_disk(&drives, &me.progress);
                match res {
                    Some(dir) => {
                        if let Some(ix) = open_index(&dir) {
                            crate::util::log(&format!("index built: {} recs, {} MB on disk, {:?}", ix.n, ix.disk_bytes() / 1_048_576, t0.elapsed()));
                            let old = me.index.write().unwrap().replace(ix);
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
        use notify::{EventKind, RecursiveMode, Watcher};
        let mut ws = self.watchers.lock().unwrap();
        ws.clear();
        for root in drives {
            let me = self.clone();
            let w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(ev) = res else { return };
                let Ok(mut g) = me.index.write() else { return };
                let Some(idx) = g.as_mut() else { return };
                match ev.kind {
                    EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::To)) => {
                        for p in ev.paths {
                            let s = p.to_string_lossy().to_string();
                            idx.removed.remove(&crate::util::lower(&s));
                            if idx.added.len() < 200_000 {
                                let d = p.is_dir();
                                idx.added.push((s, d));
                            }
                        }
                    }
                    EventKind::Remove(_) | EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::From)) => {
                        for p in ev.paths {
                            let s = p.to_string_lossy().to_string();
                            if let Some(pos) = idx.added.iter().rposition(|(a, _)| a == &s) {
                                idx.added.swap_remove(pos);
                            } else if idx.removed.len() < 200_000 {
                                idx.removed.insert(crate::util::lower(&s));
                            }
                        }
                    }
                    EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::Both)) if ev.paths.len() == 2 => {
                        let from = ev.paths[0].to_string_lossy().to_string();
                        let to = ev.paths[1].to_string_lossy().to_string();
                        idx.removed.insert(crate::util::lower(&from));
                        idx.removed.remove(&crate::util::lower(&to));
                        idx.added.push((to, ev.paths[1].is_dir()));
                    }
                    _ => {}
                }
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
        let root_s = crate::util::lower(root.to_string_lossy().trim_end_matches('\\'));
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
        let mut out = vec![];
        for i in hits {
            let p = idx.path(i);
            let pl = crate::util::lower(&p);
            if !pl.starts_with(&root_prefix) || idx.removed.contains(&pl) {
                continue;
            }
            out.push((p, idx.rec(i).flags & F_DIR != 0));
            if out.len() >= limit {
                break;
            }
        }
        for (p, d) in &idx.added {
            if out.len() >= limit {
                break;
            }
            let pl = crate::util::lower(p);
            if !pl.starts_with(&root_prefix) {
                continue;
            }
            let name = Path::new(p).file_name().map(|s| crate::util::lower(&s.to_string_lossy())).unwrap_or_default();
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
    if fmt != FORMAT {
        return None;
    }
    let names = Mapped::open(&dir.join("names.bin"))?;
    let recs = Mapped::open(&dir.join("recs.bin"))?;
    let n = recs.len / REC;
    Some(Index { names, recs, n, dir: dir.to_path_buf(), built, added: vec![], removed: HashSet::new() })
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
    fn push(&mut self, name: &str, flags: u8, parent: u32) -> u32 {
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
    f
}

/// Параллельный обход дисков с записью индекса прямо в файлы
fn build_to_disk(drives: &[PathBuf], progress: &AtomicUsize) -> Option<PathBuf> {
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
    let shared = Mutex::new((w, Vec::<(PathBuf, u32, bool)>::new()));
    {
        let mut g = shared.lock().unwrap();
        for d in drives {
            let i = g.0.push(&d.to_string_lossy(), F_DIR, NONE);
            g.1.push((d.clone(), i, true));
        }
    }
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
                        let j = g.1.pop();
                        if j.is_some() {
                            active.fetch_add(1, Ordering::SeqCst);
                        }
                        j
                    };
                    let Some((dir_p, di, is_root)) = job else {
                        if active.load(Ordering::SeqCst) == 0 {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue;
                    };
                    let sys_root = is_root && dir_p.to_string_lossy().chars().next().map(|c| c.to_ascii_uppercase()) == Some(sys);
                    let mut kids: Vec<(String, u32)> = vec![];
                    let _ = for_each_child(&dir_p, |name, attrs, _, _, _| {
                        if is_root && attrs & ATTR_DIR != 0 && excluded_top(name, sys_root) {
                            return;
                        }
                        if attrs & ATTR_DIR != 0 && name == ".git" {
                            return;
                        }
                        kids.push((name.to_string(), attrs));
                    });
                    {
                        let mut g = shared.lock().unwrap();
                        for (name, attrs) in kids {
                            let hidden_sys = attrs & ATTR_HIDDEN != 0 && attrs & ATTR_SYSTEM != 0;
                            let i = g.0.push(&name, flags_of(attrs) | if hidden_sys { F_HIDDEN } else { 0 }, di);
                            if i != NONE && attrs & ATTR_DIR != 0 && attrs & ATTR_REPARSE == 0 {
                                g.1.push((dir_p.join(&name), i, false));
                            }
                        }
                        progress.store(g.0.count as usize, Ordering::Relaxed);
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }
    });
    let (mut w, _) = shared.into_inner().ok()?;
    let ok = w.ok && w.names.flush().is_ok() && w.recs.flush().is_ok();
    drop(w);
    if !ok {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    std::fs::write(dir.join("ok"), format!("{} {}", FORMAT, crate::util::now_filetime())).ok()?;
    Some(dir)
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
