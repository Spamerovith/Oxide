//! Обновление из релизов GitHub
//!
//! Проверка: `releases/latest` через WinHTTP (TLS и прокси берутся из Windows, лишних библиотек нет).
//! Установка: новый exe скачивается целиком в память, сверяется размер и SHA-256 из релиза,
//! затем запущенный exe переименовывается в `.exe.old`, а новый встаёт на его место.
//! Настройки (%APPDATA%\Oxide), закреплённые папки и индекс поиска (%LOCALAPPDATA%\Oxide)
//! лежат отдельно от exe, поэтому обновление их не трогает

use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

use crate::i18n::{is_en, l};

const LATEST: &str = "https://api.github.com/repos/Spamerovith/Oxide/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/Spamerovith/Oxide/releases";
pub const AFTER_UPDATE_ARG: &str = "--after-update";
/// Как часто проверять, если Oxide долго открыт
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// Первая проверка - через несколько секунд после запуска, чтобы не мешать старту
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(4);

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub page: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking { manual: bool },
    UpToDate,
    Available(Release),
    Downloading { rel: Release, got: u64 },
    /// Новый exe на месте, осталось перезапуститься
    Installed(String),
    Failed { text: String, rel: Option<Release> },
}

pub struct Updater {
    state: Arc<Mutex<State>>,
    started: Instant,
    last_check: Option<Instant>,
    /// Предложение обновиться уже показано в этом сеансе
    pub offered: bool,
    /// Перезапуск уже запущен: новая версия стартует ровно один раз
    restarting: bool,
}

impl Updater {
    pub fn new() -> Self {
        Updater { state: Arc::new(Mutex::new(State::Idle)), started: Instant::now(), last_check: None, offered: false, restarting: false }
    }

    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or(State::Idle)
    }

    fn set(&self, s: State) {
        if let Ok(mut g) = self.state.lock() {
            *g = s;
        }
    }

    fn busy(&self) -> bool {
        matches!(self.state(), State::Checking { .. } | State::Downloading { .. } | State::Installed(_))
    }

    /// Плановая проверка: вызывается каждый кадр, сама решает, пора ли
    pub fn tick(&mut self, ctx: &egui::Context, enabled: bool) {
        if !enabled || self.busy() {
            return;
        }
        let due = match self.last_check {
            None => self.started.elapsed() >= FIRST_CHECK_AFTER,
            Some(t) => t.elapsed() >= CHECK_EVERY,
        };
        if due {
            self.check(ctx, false);
        } else if self.last_check.is_none() {
            ctx.request_repaint_after(FIRST_CHECK_AFTER.saturating_sub(self.started.elapsed()) + Duration::from_millis(50));
        }
    }

    /// Проверить наличие новой версии. `manual` - нажата кнопка: тогда ошибка сети показывается
    pub fn check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.busy() {
            return;
        }
        self.last_check = Some(Instant::now());
        // Уже найденную версию не теряем, пока идёт повторная проверка
        let prev = self.state();
        self.set(State::Checking { manual });
        let st = self.state.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = fetch_latest();
            let next = match res {
                Ok(Some(rel)) => State::Available(rel),
                Ok(None) => State::UpToDate,
                Err(e) if manual => State::Failed { text: e, rel: None },
                Err(_) => match prev {
                    State::Available(r) => State::Available(r),
                    _ => State::Idle,
                },
            };
            if let Ok(mut g) = st.lock() {
                *g = next;
            }
            ctx.request_repaint();
        });
    }

    /// Скачать и установить версию
    pub fn install(&mut self, ctx: &egui::Context, rel: Release) {
        if self.busy() {
            return;
        }
        self.set(State::Downloading { rel: rel.clone(), got: 0 });
        let st = self.state.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut last = Instant::now();
            let res = http_get(&rel.url, "application/octet-stream", &mut |got| {
                if last.elapsed() >= Duration::from_millis(80) {
                    last = Instant::now();
                    if let Ok(mut g) = st.lock() {
                        if let State::Downloading { got: g2, .. } = &mut *g {
                            *g2 = got;
                        }
                    }
                    ctx.request_repaint();
                }
            })
            .and_then(|data| verify(&rel, &data).map(|_| data))
            .and_then(|data| replace_exe(&data));
            let next = match res {
                Ok(()) => State::Installed(rel.version.clone()),
                Err(e) => State::Failed { text: e, rel: Some(rel.clone()) },
            };
            if let Ok(mut g) = st.lock() {
                *g = next;
            }
            ctx.request_repaint();
        });
    }

    /// Новая версия установлена и перезапуск ещё не начат. Возвращает true один раз
    pub fn take_restart(&mut self) -> bool {
        if self.restarting || !matches!(self.state(), State::Installed(_)) {
            return false;
        }
        self.restarting = true;
        true
    }

    /// Убрать сообщение об ошибке
    pub fn dismiss_error(&mut self) {
        if let State::Failed { rel, .. } = self.state() {
            self.set(match rel {
                Some(r) => State::Available(r),
                None => State::Idle,
            });
        }
    }
}

/// Перезапуск после установки: та же программа, но уже новая версия
pub fn restart() {
    let _ = std::process::Command::new(crate::util::exe_path()).arg(AFTER_UPDATE_ARG).spawn();
}

/// Удалить остатки прошлого обновления (старый exe освобождается, когда старый процесс завершится)
pub fn cleanup() {
    let exe = crate::util::exe_path();
    let files = [exe.with_extension("exe.old"), exe.with_extension("exe.new")];
    if !files.iter().any(|f| f.exists()) {
        return;
    }
    std::thread::spawn(move || {
        for _ in 0..40 {
            let mut left = false;
            for f in &files {
                if f.exists() && std::fs::remove_file(f).is_err() {
                    left = true;
                }
            }
            if !left {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let mut it = s.split(['.', '-', '+']).map(|p| p.parse::<u32>());
    let a = it.next()?.ok()?;
    let b = it.next().map(|r| r.ok()).unwrap_or(Some(0))?;
    let c = it.next().map(|r| r.ok()).unwrap_or(Some(0))?;
    Some((a, b, c))
}

fn is_newer(remote: &str, local: &str) -> bool {
    match (parse_version(remote), parse_version(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

/// Последний релиз, если он новее запущенной версии
fn fetch_latest() -> Result<Option<Release>, String> {
    let body = http_get(LATEST, "application/vnd.github+json", &mut |_| {})?;
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|_| l("GitHub вернул непонятный ответ", "GitHub returned an unexpected response").to_string())?;
    let tag = v["tag_name"].as_str().unwrap_or_default();
    if !is_newer(tag, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let asset = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str().map(|n| n.eq_ignore_ascii_case("Oxide.exe")).unwrap_or(false)));
    let Some(asset) = asset else {
        // Релиз без exe (например, только архив) - предлагать нечего
        return Ok(None);
    };
    let url = asset["browser_download_url"].as_str().unwrap_or_default().to_string();
    if !url.starts_with("https://github.com/") {
        return Ok(None);
    }
    let sha256 = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(|s| s.to_ascii_lowercase());
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url,
        size: asset["size"].as_u64().unwrap_or(0),
        sha256,
        page: v["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string(),
    }))
}

fn verify(rel: &Release, data: &[u8]) -> Result<(), String> {
    let bad = || l("Загруженный файл повреждён, попробуйте ещё раз", "The downloaded file is damaged, please try again").to_string();
    if data.len() < 2 || &data[..2] != b"MZ" || (rel.size > 0 && data.len() as u64 != rel.size) {
        return Err(bad());
    }
    if let Some(want) = &rel.sha256 {
        if sha256_hex(data).as_deref() != Some(want.as_str()) {
            return Err(bad());
        }
    }
    Ok(())
}

fn sha256_hex(data: &[u8]) -> Option<String> {
    use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
    let mut out = [0u8; 32];
    let st = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out) };
    if st.is_err() {
        return None;
    }
    Some(out.iter().map(|b| format!("{b:02x}")).collect())
}

/// Подменить запущенный exe новым. Windows не даёт перезаписать работающий exe, но даёт его переименовать
fn replace_exe(data: &[u8]) -> Result<(), String> {
    let cur = crate::util::exe_path();
    let new = cur.with_extension("exe.new");
    let old = cur.with_extension("exe.old");
    let no_access = || {
        let dir = cur.parent().map(|d| d.display().to_string()).unwrap_or_default();
        if is_en() {
            format!("No permission to write to \"{dir}\". Move Oxide.exe to a folder you can write to or download the update from GitHub")
        } else {
            format!("Нет прав на запись в папку «{dir}». Перенесите Oxide.exe в доступную папку или скачайте обновление с GitHub")
        }
    };
    std::fs::write(&new, data).map_err(|_| no_access())?;
    let _ = std::fs::remove_file(&old);
    if std::fs::rename(&cur, &old).is_err() {
        let _ = std::fs::remove_file(&new);
        return Err(no_access());
    }
    if std::fs::rename(&new, &cur).is_err() {
        let _ = std::fs::rename(&old, &cur);
        let _ = std::fs::remove_file(&new);
        return Err(no_access());
    }
    Ok(())
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// HTTPS GET целиком в память. Переадресации (GitHub отдаёт файлы через свой CDN) WinHTTP проходит сам
fn http_get(url: &str, accept: &str, progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String> {
    let net = || l("Не удалось подключиться к GitHub. Проверьте подключение к интернету", "Couldn't connect to GitHub. Check your internet connection").to_string();
    let rest = url.strip_prefix("https://").ok_or_else(net)?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let host_w = crate::util::wide(host);
    let path_w = crate::util::wide(path);
    let agent = crate::util::wide(&format!("Oxide/{}", env!("CARGO_PKG_VERSION")));
    let mut headers: Vec<u16> = format!("Accept: {accept}\r\n").encode_utf16().collect();
    if url.contains("api.github.com") {
        headers.extend("X-GitHub-Api-Version: 2022-11-28\r\n".encode_utf16());
    }
    unsafe {
        let session = Handle(WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err(net());
        }
        let _ = WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 30_000);
        let conn = Handle(WinHttpConnect(session.0, PCWSTR(host_w.as_ptr()), 443, 0));
        if conn.0.is_null() {
            return Err(net());
        }
        let req = Handle(WinHttpOpenRequest(conn.0, w!("GET"), PCWSTR(path_w.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE));
        if req.0.is_null() {
            return Err(net());
        }
        WinHttpSendRequest(req.0, Some(headers.as_slice()), None, 0, 0, 0).map_err(|_| net())?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|_| net())?;
        let mut code: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut code as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(|_| net())?;
        if code != 200 {
            return Err(if code == 403 || code == 429 {
                l("GitHub временно ограничил запросы, попробуйте позже", "GitHub is limiting requests right now, please try again later").to_string()
            } else if is_en() {
                format!("GitHub replied with error {code}")
            } else {
                format!("GitHub ответил ошибкой {code}")
            });
        }
        let mut out: Vec<u8> = Vec::new();
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(req.0, &mut avail).map_err(|_| net())?;
            if avail == 0 {
                break;
            }
            let at = out.len();
            out.resize(at + avail as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(req.0, out[at..].as_mut_ptr() as *mut c_void, avail, &mut read).map_err(|_| net())?;
            out.truncate(at + read as usize);
            progress(out.len() as u64);
            if out.len() > 200 << 20 {
                return Err(net());
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.0"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.10"), Some((1, 10, 0)));
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(!is_newer("v1.1.0", "1.1.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }

    #[test]
    fn sha() {
        assert_eq!(sha256_hex(b"abc").unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
