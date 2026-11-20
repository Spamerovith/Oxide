//! Своя рамка окна поверх нативной: вкладки в заголовке, как в Проводнике Windows 11,
//! при этом остаются системная тень, скругление, привязка (Snap Layouts) и изменение размера

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::core::{Interface, PCSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, VK_CONTROL};
use windows::Win32::UI::Shell::{IContextMenu2, IContextMenu3};
use windows::Win32::UI::WindowsAndMessaging::*;

static OLD_PROC: AtomicIsize = AtomicIsize::new(0);
static HWND_RAW: AtomicIsize = AtomicIsize::new(0);
pub static MAX_HOVER: AtomicBool = AtomicBool::new(false);
pub static MAX_PRESSED: AtomicBool = AtomicBool::new(false);
pub static PASTE_REQUEST: AtomicBool = AtomicBool::new(false);
pub static THEME_CHANGED: AtomicBool = AtomicBool::new(false);
/// Окно тащат за заголовок (или меняют размер): идёт системный цикл перемещения
static IN_MOVE: AtomicBool = AtomicBool::new(false);
/// За время перетаскивания окно сдвинулось: в конце сообщим новое положение один раз
static MOVED: AtomicBool = AtomicBool::new(false);
static CTX: OnceLock<egui::Context> = OnceLock::new();

/// Области заголовка в физических пикселях клиентской области
#[derive(Default, Clone)]
pub struct HitRegions {
    pub caption_h: i32,
    pub max_btn: Option<[i32; 4]>,
    pub interactive: Vec<[i32; 4]>,
}

static REGIONS: Mutex<Option<HitRegions>> = Mutex::new(None);

thread_local! {
    static ACTIVE_MENU: RefCell<Option<IContextMenu2>> = const { RefCell::new(None) };
}

pub fn hwnd() -> HWND {
    HWND(HWND_RAW.load(Ordering::Relaxed) as *mut _)
}

pub fn set_regions(r: HitRegions) {
    *REGIONS.lock().unwrap() = Some(r);
}

pub fn set_active_menu(m: Option<IContextMenu2>) {
    ACTIVE_MENU.with(|a| *a.borrow_mut() = m);
}

fn repaint() {
    if let Some(c) = CTX.get() {
        c.request_repaint();
    }
}

pub fn install(hwnd: HWND, ctx: &egui::Context) {
    let _ = CTX.set(ctx.clone());
    HWND_RAW.store(hwnd.0 as isize, Ordering::Relaxed);
    unsafe {
        let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wndproc as usize as isize);
        OLD_PROC.store(old, Ordering::Relaxed);
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const _, 4);
        let m = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 };
        let _ = windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea(hwnd, &m);
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// Размытие того, что за окном. Возвращает false, если Windows не умеет
/// Используется размытие композитора (как у панели задач Windows 10): в отличие от акрила
/// Windows 11 оно не превращается в серую заливку, когда окно неактивно
pub fn set_glass(hwnd: HWND, on: bool, blur: bool) -> bool {
    use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
    #[repr(C)]
    struct AccentPolicy {
        state: u32,
        flags: u32,
        color: u32,
        anim: u32,
    }
    #[repr(C)]
    struct CompAttr {
        attr: u32,
        data: *mut std::ffi::c_void,
        size: usize,
    }
    unsafe {
        let mut ok = true;
        if let Ok(u) = LoadLibraryW(windows::core::w!("user32.dll")) {
            if let Some(f) = GetProcAddress(u, PCSTR(b"SetWindowCompositionAttribute\0".as_ptr())) {
                let set: extern "system" fn(HWND, *mut CompAttr) -> BOOL = std::mem::transmute(f);
                // 3 - размытие за окном, 0 - выключено
                let mut ap = AccentPolicy { state: if on && blur { 3 } else { 0 }, flags: 0, color: 0, anim: 0 };
                let mut d = CompAttr { attr: 19, data: &mut ap as *mut _ as *mut _, size: std::mem::size_of::<AccentPolicy>() };
                ok = set(hwnd, &mut d).as_bool() || !(on && blur);
            } else {
                ok = !blur;
            }
        }
        let m = if on && ok {
            MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 }
        } else {
            MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 1, cyBottomHeight: 0 }
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
        !on || ok
    }
}

/// Ставит окно ровно в сохранённый внешний прямоугольник (логические точки, как его отдаёт
/// `viewport().outer_rect`). Без этого окно росло бы на рамку при каждом запуске
pub fn set_outer_rect(hwnd: HWND, r: [f32; 4]) {
    unsafe {
        let s = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        let _ = SetWindowPos(
            hwnd,
            None,
            (r[0] * s).round() as i32,
            (r[1] * s).round() as i32,
            (r[2] * s).round() as i32,
            (r[3] * s).round() as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Тёмная рамка окна и тёмные системные меню
pub fn set_dark(hwnd: HWND, dark: bool) {
    unsafe {
        let v = BOOL(dark as i32);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &v as *const _ as *const _, 4);
        // Недокументированные функции uxtheme: тёмные классические меню (так делает и Проводник)
        if let Ok(ux) = LoadLibraryW(windows::core::w!("uxtheme.dll")) {
            if let Some(f) = GetProcAddress(ux, PCSTR(135 as *const u8)) {
                let set_mode: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
                set_mode(if dark { 2 } else { 3 });
            }
            if let Some(f) = GetProcAddress(ux, PCSTR(133 as *const u8)) {
                let allow: extern "system" fn(HWND, BOOL) -> BOOL = std::mem::transmute(f);
                allow(hwnd, BOOL(dark as i32));
            }
            if let Some(f) = GetProcAddress(ux, PCSTR(136 as *const u8)) {
                let flush: extern "system" fn() = std::mem::transmute(f);
                flush();
            }
        }
    }
}

pub fn is_maximized() -> bool {
    unsafe { IsZoomed(hwnd()).as_bool() }
}

pub fn toggle_maximize() {
    unsafe {
        let h = hwnd();
        let _ = ShowWindow(h, if IsZoomed(h).as_bool() { SW_RESTORE } else { SW_MAXIMIZE });
    }
}

pub fn minimize() {
    unsafe {
        let _ = ShowWindow(hwnd(), SW_MINIMIZE);
    }
}

fn frame_thickness(hwnd: HWND) -> (i32, i32) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        let pad = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        (GetSystemMetricsForDpi(SM_CXFRAME, dpi) + pad, GetSystemMetricsForDpi(SM_CYFRAME, dpi) + pad)
    }
}

unsafe fn call_old(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let old: WNDPROC = std::mem::transmute(OLD_PROC.load(Ordering::Relaxed));
    CallWindowProcW(old, hwnd, msg, wp, lp)
}

fn inside(r: &[i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Сообщения для подменю системного контекстного меню («Отправить», «Открыть с помощью»)
    if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR | WM_MENUSELECT) {
        let handled = ACTIVE_MENU.with(|a| {
            if let Some(cm) = a.borrow().as_ref() {
                if let Ok(cm3) = cm.cast::<IContextMenu3>() {
                    let mut res = LRESULT(0);
                    if cm3.HandleMenuMsg2(msg, wp, lp, Some(&mut res)).is_ok() {
                        return Some(res);
                    }
                } else if cm.HandleMenuMsg(msg, wp, lp).is_ok() {
                    return Some(LRESULT(0));
                }
            }
            None
        });
        if let Some(r) = handled {
            return r;
        }
    }

    match msg {
        WM_ENTERSIZEMOVE => {
            IN_MOVE.store(true, Ordering::Relaxed);
        }
        WM_EXITSIZEMOVE => {
            IN_MOVE.store(false, Ordering::Relaxed);
            let r = call_old(hwnd, msg, wp, lp);
            if MOVED.swap(false, Ordering::Relaxed) {
                // Одно итоговое «окно сдвинулось», чтобы запомнить положение
                let mut rc = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rc);
                let mut wpos = WINDOWPOS {
                    hwnd,
                    hwndInsertAfter: HWND::default(),
                    x: rc.left,
                    y: rc.top,
                    cx: rc.right - rc.left,
                    cy: rc.bottom - rc.top,
                    flags: SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE,
                };
                call_old(hwnd, WM_WINDOWPOSCHANGED, WPARAM(0), LPARAM(&mut wpos as *mut _ as isize));
                repaint();
            }
            return r;
        }
        // Пока окно тащат, не перерисовываем его на каждый сдвиг: содержимое не меняется, а ожидание
        // кадра тормозило перемещение. Изменение размера проходит как обычно
        WM_WINDOWPOSCHANGED if IN_MOVE.load(Ordering::Relaxed) => {
            let wpos = &*(lp.0 as *const WINDOWPOS);
            if wpos.flags.contains(SWP_NOSIZE) && !wpos.flags.contains(SWP_NOMOVE) {
                MOVED.store(true, Ordering::Relaxed);
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
        }
        WM_NCCALCSIZE if wp.0 != 0 => {
            // Слева, справа и снизу рамка остаётся системной: она невидима и лежит за краем окна,
            // как у Проводника, поэтому не мешает брать полосу прокрутки. Убираем только заголовок
            let params = &mut *(lp.0 as *mut NCCALCSIZE_PARAMS);
            let top = params.rgrc[0].top;
            let r = DefWindowProcW(hwnd, msg, wp, lp);
            let params = &mut *(lp.0 as *mut NCCALCSIZE_PARAMS);
            params.rgrc[0].top = top;
            if IsZoomed(hwnd).as_bool() {
                params.rgrc[0].top += frame_thickness(hwnd).1;
            }
            return r;
        }
        WM_NCHITTEST => {
            let mut pt = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
            let _ = ScreenToClient(hwnd, &mut pt);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            // Левая, правая и нижняя рамки - за краем окна, их обслуживает Windows
            let def = DefWindowProcW(hwnd, msg, wp, lp);
            if def.0 as u32 != HTCLIENT {
                return def;
            }
            let zoomed = IsZoomed(hwnd).as_bool();
            if !zoomed {
                // Верхняя рамка внутри окна, как у Проводника: тонкая полоса над вкладками
                let (fx, fy) = frame_thickness(hwnd);
                if pt.y < fy.max(4) {
                    let hit = if pt.x < fx * 2 {
                        HTTOPLEFT
                    } else if pt.x >= rc.right - fx * 2 {
                        HTTOPRIGHT
                    } else {
                        HTTOP
                    };
                    return LRESULT(hit as isize);
                }
            }
            let regions = REGIONS.lock().unwrap().clone();
            if let Some(r) = regions {
                if let Some(mb) = r.max_btn {
                    if inside(&mb, pt.x, pt.y) {
                        return LRESULT(HTMAXBUTTON as isize);
                    }
                }
                if pt.y < r.caption_h && !r.interactive.iter().any(|ir| inside(ir, pt.x, pt.y)) {
                    return LRESULT(HTCAPTION as isize);
                }
            }
            return LRESULT(HTCLIENT as isize);
        }
        WM_NCMOUSEMOVE => {
            let over = wp.0 as u32 == HTMAXBUTTON;
            if over != MAX_HOVER.swap(over, Ordering::Relaxed) {
                repaint();
            }
            if over {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut t);
            }
        }
        WM_NCMOUSELEAVE => {
            MAX_HOVER.store(false, Ordering::Relaxed);
            MAX_PRESSED.store(false, Ordering::Relaxed);
            repaint();
        }
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wp.0 as u32 == HTMAXBUTTON => {
            MAX_PRESSED.store(true, Ordering::Relaxed);
            repaint();
            return LRESULT(0);
        }
        WM_NCLBUTTONUP if wp.0 as u32 == HTMAXBUTTON => {
            if MAX_PRESSED.swap(false, Ordering::Relaxed) {
                toggle_maximize();
            }
            repaint();
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            if wp.0 == 'V' as usize && GetKeyState(VK_CONTROL.0 as i32) < 0 {
                PASTE_REQUEST.store(true, Ordering::Relaxed);
                repaint();
            }
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE => {
            THEME_CHANGED.store(true, Ordering::Relaxed);
            repaint();
        }
        0x0320 /* WM_DWMCOLORIZATIONCOLORCHANGED */ => {
            THEME_CHANGED.store(true, Ordering::Relaxed);
            repaint();
        }
        _ => {}
    }
    call_old(hwnd, msg, wp, lp)
}

/// Экранные координаты курсора → логические точки egui
pub fn cursor_client_points(ppp: f32) -> Option<egui::Pos2> {
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        let _ = ScreenToClient(hwnd(), &mut pt);
        Some(egui::pos2(pt.x as f32 / ppp, pt.y as f32 / ppp))
    }
}

/// Логические точки клиентской области → экран (для системных меню)
pub fn client_to_screen(p: egui::Pos2, ppp: f32) -> (i32, i32) {
    unsafe {
        let mut pt = POINT { x: (p.x * ppp) as i32, y: (p.y * ppp) as i32 };
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd(), &mut pt);
        (pt.x, pt.y)
    }
}
