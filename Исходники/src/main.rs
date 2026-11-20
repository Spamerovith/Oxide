#![windows_subsystem = "windows"]
//! Oxide - быстрый и лёгкий проводник для Windows в стиле Проводника Windows 11

mod app;
mod cmd;
mod fonts;
mod i18n;
mod search;
mod settings;
mod shell;
mod theme;
mod ui;
mod update;
mod util;

fn main() -> eframe::Result<()> {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    let settings = settings::Settings::load();
    i18n::set(settings.language.unwrap_or_else(i18n::Lang::system));
    update::cleanup();
    // Если Oxide.exe перенесли, а он открывает папки по умолчанию, обновить путь в реестре
    shell::default_app::sync(settings.default_explorer);
    let start = std::env::args().nth(1);

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/oxide.png")).ok();
    let mut vp = egui::ViewportBuilder::default()
        .with_title("Oxide")
        .with_app_id("Oxide.Explorer")
        .with_min_inner_size([560.0, 380.0])
        .with_decorations(true)
        // Прозрачный буфер нужен теме «Жидкое стекло»; остальные темы закрашивают фон сами
        .with_transparent(true)
        .with_drag_and_drop(true);
    match settings.window {
        Some([x, y, w, h]) => {
            vp = vp.with_position([x, y]).with_inner_size([w, h]);
        }
        None => {
            vp = vp.with_inner_size([1100.0, 720.0]);
        }
    }
    if settings.maximized {
        vp = vp.with_maximized(true);
    }
    if let Some(i) = icon {
        vp = vp.with_icon(std::sync::Arc::new(i));
    }
    let opts = eframe::NativeOptions {
        viewport: vp,
        renderer: eframe::Renderer::Glow,
        vsync: true,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        persist_window: false,
        centered: settings.window.is_none(),
        ..Default::default()
    };
    eframe::run_native("Oxide", opts, Box::new(move |cc| Ok(Box::new(app::OxideApp::new(cc, settings, start)))))
}
