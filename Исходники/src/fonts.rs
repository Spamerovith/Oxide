//! Системные шрифты Windows 11: Segoe UI Variable, Segoe UI Semibold, Segoe Fluent Icons.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

fn read(file: &str) -> Option<Vec<u8>> {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    std::fs::read(format!(r"{dir}\Fonts\{file}")).ok()
}

pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut body: Vec<String> = vec![];
    for (key, file) in [("segoe-var", "SegUIVar.ttf"), ("segoe", "segoeui.ttf"), ("segoe-sym", "seguisym.ttf")] {
        if let Some(b) = read(file) {
            defs.font_data.insert(key.into(), Arc::new(FontData::from_owned(b)));
            body.push(key.into());
        }
    }
    let mut semibold: Vec<String> = vec![];
    if let Some(b) = read("seguisb.ttf") {
        defs.font_data.insert("segoe-sb".into(), Arc::new(FontData::from_owned(b)));
        semibold.push("segoe-sb".into());
    }
    let mut icons: Vec<String> = vec![];
    for (key, file) in [("fluent-icons", "SegoeIcons.ttf"), ("mdl2", "segmdl2.ttf")] {
        if let Some(b) = read(file) {
            defs.font_data.insert(key.into(), Arc::new(FontData::from_owned(b)));
            icons.push(key.into());
            break;
        }
    }
    let prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut p = body.clone();
    p.extend(prop.iter().cloned());
    defs.families.insert(FontFamily::Proportional, p.clone());
    let mut sb = semibold;
    sb.extend(p.iter().cloned());
    defs.families.insert(FontFamily::Name("semibold".into()), sb);
    // Моноширинный (область просмотра текста): Cascadia Mono / Consolas из Windows.
    let mut mono: Vec<String> = vec![];
    for (key, file) in [("cascadia", "CascadiaMono.ttf"), ("consolas", "consola.ttf")] {
        if let Some(b) = read(file) {
            defs.font_data.insert(key.into(), Arc::new(FontData::from_owned(b)));
            mono.push(key.into());
            break;
        }
    }
    mono.extend(p.iter().cloned());
    defs.families.insert(FontFamily::Monospace, mono);
    let mut ic = icons;
    ic.extend(p.iter().cloned());
    defs.families.insert(FontFamily::Name("icons".into()), ic);
    ctx.set_fonts(defs);
}
