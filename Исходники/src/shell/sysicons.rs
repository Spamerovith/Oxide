//! Системные значки и эскизы через IShellItemImageFactory в фоновых потоках

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};

use egui::{ColorImage, TextureHandle, TextureOptions};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT,
};

use crate::util::wide;

#[derive(Clone)]
struct Req {
    key: (String, u32),
    path: String,
    thumb: bool,
}

struct Shared {
    queue: Mutex<Vec<Req>>,
    cv: Condvar,
    done: Mutex<Vec<((String, u32), Option<ColorImage>)>>,
    ctx: egui::Context,
}

enum Slot {
    Pending,
    Ready(TextureHandle),
    Failed,
}

pub struct IconCache {
    shared: Arc<Shared>,
    map: HashMap<(String, u32), (Slot, u64)>,
    queued: HashSet<(String, u32)>,
    frame: u64,
}

/// Расширения, для которых в крупных видах показывается эскиз
pub fn thumbable(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "jfif" | "png" | "gif" | "bmp" | "webp" | "heic" | "heif" | "avif" | "tif" | "tiff" | "ico" | "dds"
            | "psd" | "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "m4v" | "mpg" | "mpeg" | "3gp" | "flv" | "mp3"
            | "flac" | "m4a" | "pdf" | "svg" | "raw" | "cr2" | "nef" | "arw" | "dng"
    )
}

/// Расширения, у которых свой значок у каждого файла
pub fn per_file_icon(ext: &str) -> bool {
    matches!(ext, "exe" | "ico" | "lnk" | "url" | "cur" | "ani" | "scr" | "msc" | "appref-ms" | "cpl" | "library-ms")
}

impl IconCache {
    pub fn new(ctx: &egui::Context) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Vec::new()),
            cv: Condvar::new(),
            done: Mutex::new(Vec::new()),
            ctx: ctx.clone(),
        });
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 4);
        for i in 0..threads {
            let s = shared.clone();
            std::thread::Builder::new()
                .name(format!("icons-{i}"))
                .spawn(move || worker(s))
                .ok();
        }
        IconCache { shared, map: HashMap::new(), queued: HashSet::new(), frame: 0 }
    }

    /// Вызывать раз в кадр: забирает готовые картинки и создаёт текстуры
    pub fn pump(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        let done: Vec<_> = std::mem::take(&mut *self.shared.done.lock().unwrap());
        for (key, img) in done {
            self.queued.remove(&key);
            let slot = match img {
                Some(img) => Slot::Ready(ctx.load_texture(format!("ic:{}:{}", key.0, key.1), img, TextureOptions::LINEAR)),
                None => Slot::Failed,
            };
            self.map.insert(key, (slot, self.frame));
        }
        // Освобождаем давно не показанные эскизы
        if self.map.len() > 1500 && self.frame % 120 == 0 {
            let f = self.frame;
            self.map.retain(|k, (_, last)| !k.0.contains('\\') || f - *last < 600);
        }
    }

    /// Возвращает текстуру значка; если её ещё нет - ставит в очередь
    pub fn get(&mut self, key: &str, path: &str, px: u32, thumb: bool) -> Option<&TextureHandle> {
        let k = (key.to_string(), px);
        let frame = self.frame;
        match self.map.get_mut(&k) {
            Some((Slot::Ready(_), last)) => {
                *last = frame;
            }
            Some((Slot::Pending, _)) | Some((Slot::Failed, _)) => return None,
            None => {
                self.map.insert(k.clone(), (Slot::Pending, frame));
                if self.queued.insert(k.clone()) {
                    let mut q = self.shared.queue.lock().unwrap();
                    q.push(Req { key: k.clone(), path: path.to_string(), thumb });
                    self.shared.cv.notify_one();
                }
                return None;
            }
        }
        match self.map.get(&k) {
            Some((Slot::Ready(t), _)) => Some(t),
            _ => None,
        }
    }

    pub fn failed(&self, key: &str, px: u32) -> bool {
        matches!(self.map.get(&(key.to_string(), px)), Some((Slot::Failed, _)))
    }

    /// Сбрасывает значки конкретных файлов (после изменения)
    pub fn invalidate_path(&mut self, path: &str) {
        self.map.retain(|k, _| k.0 != path);
    }

    pub fn clear_thumbs(&mut self) {
        self.map.retain(|k, _| !k.0.contains('\\'));
    }
}

fn worker(s: Arc<Shared>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    loop {
        let req = {
            let mut q = s.queue.lock().unwrap();
            loop {
                if let Some(r) = q.pop() {
                    break r;
                }
                q = s.cv.wait(q).unwrap();
            }
        };
        let img = load_image(&req.path, req.key.1, req.thumb);
        s.done.lock().unwrap().push((req.key, img));
        s.ctx.request_repaint();
    }
}

pub fn load_image(path: &str, px: u32, thumb: bool) -> Option<ColorImage> {
    unsafe {
        let w = wide(path);
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).ok()?;
        let fac: IShellItemImageFactory = item.cast().ok()?;
        let flags = if thumb { SIIGBF_RESIZETOFIT } else { SIIGBF(SIIGBF_ICONONLY.0 | SIIGBF_BIGGERSIZEOK.0) };
        let hbmp = fac.GetImage(SIZE { cx: px as i32, cy: px as i32 }, flags).ok()?;
        let img = hbitmap_to_image(hbmp);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        img
    }
}

pub unsafe fn hbitmap_to_image(hbmp: HBITMAP) -> Option<ColorImage> {
    let mut bm = BITMAP::default();
    if GetObjectW(HGDIOBJ(hbmp.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let dc = GetDC(None);
    let n = GetDIBits(dc, hbmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
    ReleaseDC(None, dc);
    if n == 0 {
        return None;
    }
    let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
    for p in buf.chunks_exact_mut(4) {
        p.swap(0, 2); // BGRA → RGBA
        if !has_alpha {
            p[3] = 255;
        }
    }
    // Изображение уже с предумноженной альфой (DIB для AlphaBlend)
    Some(ColorImage::from_rgba_premultiplied([w as usize, h as usize], &buf))
}
