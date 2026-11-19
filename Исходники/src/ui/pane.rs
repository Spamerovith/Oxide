//! Область сведений и область просмотра справа.

use egui::{Align2, CornerRadius, Pos2, Rect, Ui, Vec2};

use super::widgets::*;
use crate::app::OxideApp;
use crate::cmd::PaneMode;
use crate::shell::fs;

const TEXT_PREVIEW: &[&str] = &[
    "txt", "md", "log", "ini", "cfg", "conf", "json", "xml", "yaml", "yml", "toml", "csv", "rs", "py", "js", "ts", "c", "h", "cpp", "hpp", "cs", "java",
    "go", "html", "htm", "css", "bat", "cmd", "ps1", "sh", "sql", "lua", "kt", "swift", "php", "rb", "gitignore", "reg",
];

pub fn show(app: &mut OxideApp, ui: &mut Ui) {
    let pal = app.pal.clone();
    let ctx = ui.ctx().clone();
    let rect = ui.max_rect();
    // Тонкая линия-граница слева.
    ui.painter().line_segment([Pos2::new(rect.left() + 0.5, rect.top()), Pos2::new(rect.left() + 0.5, rect.bottom())], egui::Stroke::new(1.0, pal.divider));
    let mode = app.settings.pane;
    let sel: Vec<_> = app.tab().selected_entries().into_iter().cloned().collect();
    let ppp = ctx.pixels_per_point();
    egui::ScrollArea::vertical().id_salt("pane").auto_shrink([false, false]).show(ui, |ui| {
        let w = ui.available_width();
        ui.add_space(16.0);
        if sel.is_empty() {
            let t = app.tab();
            let msg = if mode == PaneMode::Preview { "Выберите файл для предварительного просмотра." } else { "Выберите один файл, чтобы получить сведения о нём." };
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new(msg).font(font(12.0)).color(pal.text2));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new(format!("Элементов: {}", t.entries.len())).font(font(12.0)).color(pal.text2));
            });
            return;
        }
        if sel.len() > 1 {
            let total: u64 = sel.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(format!("Выбрано элементов: {}", sel.len())).font(font_sb(14.0)).color(pal.text));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("Общий размер файлов: {}", crate::util::format_bytes(total))).font(font(12.0)).color(pal.text2));
                });
            });
            return;
        }
        let e = sel[0].clone();
        let size = (w - 48.0).clamp(64.0, 256.0);
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, size + 16.0), egui::Sense::hover());
        let c = Pos2::new(r.center().x, r.top() + size / 2.0 + 8.0);
        let (key, path, _) = super::content::icon_req(&e, true);
        let thumb = !e.is_dir && crate::shell::sysicons::thumbable(&e.ext);
        let px = (size * ppp).round() as u32;
        let key = if thumb { key } else { e.path.to_string_lossy().to_string() };
        if let Some(tex) = app.icons.get(&key, &path, px, thumb) {
            paint_texture(ui.painter(), tex, c, size);
        }

        if mode == PaneMode::Preview && !e.is_dir && TEXT_PREVIEW.contains(&e.ext.as_str()) {
            ui.add_space(8.0);
            let text = read_preview(&e.path);
            egui::Frame::new().fill(pal.field).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.set_width(w - 20.0);
                ui.label(egui::RichText::new(text).font(egui::FontId::monospace(12.0)).color(pal.text));
            });
            return;
        }

        ui.add_space(8.0);
        let rows: Vec<(String, String)> = {
            let mut v = vec![];
            v.push(("Тип".to_string(), e.type_name.clone().unwrap_or_else(|| fs::type_name(&e.ext, e.is_dir))));
            if !e.is_dir {
                v.push(("Размер".into(), format!("{} ({} байт)", crate::util::format_bytes(e.size), group(e.size))));
            }
            if e.mtime != 0 {
                v.push(("Дата изменения".into(), crate::util::format_filetime(e.mtime)));
            }
            if e.ctime != 0 && e.shell.is_none() {
                v.push(("Дата создания".into(), crate::util::format_filetime(e.ctime)));
            }
            let loc = e.location.clone().or_else(|| e.path.parent().map(|p| p.to_string_lossy().to_string())).unwrap_or_default();
            v.push(("Расположение".into(), loc));
            let mut attrs = vec![];
            if e.attrs & fs::ATTR_HIDDEN != 0 {
                attrs.push("скрытый");
            }
            if e.attrs & fs::ATTR_READONLY != 0 && !e.is_dir {
                attrs.push("только чтение");
            }
            if e.attrs & fs::ATTR_SYSTEM != 0 {
                attrs.push("системный");
            }
            if !attrs.is_empty() {
                v.push(("Атрибуты".into(), attrs.join(", ")));
            }
            v
        };
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(w - 32.0);
                let mut job = egui::text::LayoutJob::single_section(e.display.clone(), egui::TextFormat::simple(font_sb(14.0), pal.text));
                job.wrap.max_width = w - 32.0;
                let g = ui.fonts(|f| f.layout_job(job));
                let (r, _) = ui.allocate_exact_size(g.size(), egui::Sense::hover());
                ui.painter().galley(r.min, g, pal.text);
                ui.add_space(12.0);
                ui.label(egui::RichText::new("Свойства").font(font_sb(12.0)).color(pal.text));
                ui.add_space(4.0);
                for (k, v) in rows {
                    ui.label(egui::RichText::new(k).font(font(12.0)).color(pal.text2));
                    ui.add(egui::Label::new(egui::RichText::new(v).font(font(12.0)).color(pal.text)).wrap());
                    ui.add_space(6.0);
                }
            });
        });
        let _ = Rect::NOTHING;
        let _ = Align2::LEFT_TOP;
    });
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{a0}');
        }
        out.push(ch);
    }
    out
}

fn read_preview(p: &std::path::Path) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; 16 * 1024];
    let n = std::fs::File::open(p).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
    buf.truncate(n);
    let s = if buf.len() >= 2 && buf[0] == 0xFF && buf[1] == 0xFE {
        let w: Vec<u16> = buf[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&w)
    } else {
        String::from_utf8_lossy(&buf).to_string()
    };
    s.lines().take(300).collect::<Vec<_>>().join("\n")
}
