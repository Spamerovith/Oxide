//! Цветовые темы и метрики. Значения описаны в DESIGN.md - меняются только вместе

#[allow(unused_imports)]
use crate::i18n::{tr, trf};

use egui::{Color32, CornerRadius, Stroke, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
    Oxide,
    Graphite,
    Nordic,
    Paper,
    Glass,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 8] = [
        ThemeChoice::System,
        ThemeChoice::Light,
        ThemeChoice::Dark,
        ThemeChoice::Oxide,
        ThemeChoice::Graphite,
        ThemeChoice::Nordic,
        ThemeChoice::Paper,
        ThemeChoice::Glass,
    ];
    pub fn title(self) -> &'static str {
        match self {
            ThemeChoice::System => tr("Как в Windows"),
            ThemeChoice::Light => tr("Windows 11 - светлая"),
            ThemeChoice::Dark => tr("Windows 11 - тёмная"),
            ThemeChoice::Oxide => "Oxide",
            ThemeChoice::Graphite => tr("Графит"),
            ThemeChoice::Nordic => tr("Северная ночь"),
            ThemeChoice::Paper => tr("Бумага"),
            ThemeChoice::Glass => tr("Жидкое стекло"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Полупрозрачные слои поверх размытия Windows (тема «Жидкое стекло»)
    pub glass: bool,
    /// Яркость бликов по краям стеклянных элементов (0 - нет)
    pub shine: f32,
    pub base: Color32,
    pub layer: Color32,
    pub cmdbar: Color32,
    pub divider: Color32,
    pub field: Color32,
    pub field_hover: Color32,
    pub field_stroke: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub text_disabled: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    pub selected_hover: Color32,
    pub nav_selected: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub flyout: Color32,
    pub flyout_stroke: Color32,
    pub shadow: Color32,
    pub close_hover: Color32,
    pub danger: Color32,
    pub focus: Color32,
    pub bar_track: Color32,
    /// Рамка выделения мышью (базовый системный акцент)
    pub marquee: Color32,
}

fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
fn rgba(v: u32) -> Color32 {
    // 0xRRGGBBAA, не premultiplied
    Color32::from_rgba_unmultiplied((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8)
}

impl Palette {
    pub fn light(accent: Option<[Color32; 8]>) -> Self {
        Palette {
            dark: false,
            glass: false,
            shine: 0.0,
            base: hex(0xF3F3F3),
            layer: hex(0xFBFBFB),
            cmdbar: hex(0xF9F9F9),
            divider: hex(0xE5E5E5),
            field: hex(0xFFFFFF),
            field_hover: hex(0xF9F9F9),
            field_stroke: hex(0xE5E5E5),
            text: hex(0x1B1B1B),
            text2: hex(0x616161),
            text3: hex(0x8D8D8D),
            text_disabled: hex(0xA0A0A0),
            hover: rgba(0x0000000A),
            pressed: rgba(0x00000006),
            selected: hex(0xCCE8FF),
            selected_hover: hex(0xB9DFFF),
            nav_selected: rgba(0x0000000F),
            accent: accent.map(|a| a[4]).unwrap_or(hex(0x005FB8)),
            on_accent: hex(0xFFFFFF),
            flyout: hex(0xF9F9F9),
            flyout_stroke: hex(0xE0E0E0),
            shadow: rgba(0x00000024),
            close_hover: hex(0xC42B1C),
            danger: hex(0xC42B1C),
            focus: hex(0x1B1B1B),
            bar_track: hex(0xE6E6E6),
            marquee: accent.map(|a| a[3]).unwrap_or(hex(0x0078D4)),
        }
    }

    pub fn dark(accent: Option<[Color32; 8]>) -> Self {
        Palette {
            dark: true,
            glass: false,
            shine: 0.0,
            base: hex(0x202020),
            layer: hex(0x2C2C2C),
            cmdbar: hex(0x282828),
            divider: hex(0x3A3A3A),
            field: hex(0x383838),
            field_hover: hex(0x3D3D3D),
            field_stroke: hex(0x444444),
            text: hex(0xFFFFFF),
            text2: hex(0xCFCFCF),
            text3: hex(0x8B8B8B),
            text_disabled: hex(0x5D5D5D),
            hover: rgba(0xFFFFFF0F),
            pressed: rgba(0xFFFFFF0A),
            selected: rgba(0xFFFFFF2B),
            selected_hover: rgba(0xFFFFFF36),
            nav_selected: rgba(0xFFFFFF1A),
            accent: accent.map(|a| a[1]).unwrap_or(hex(0x60CDFF)),
            on_accent: hex(0x000000),
            flyout: hex(0x2C2C2C),
            flyout_stroke: hex(0x3F3F3F),
            shadow: rgba(0x00000066),
            close_hover: hex(0xC42B1C),
            danger: hex(0xFF99A4),
            focus: hex(0xFFFFFF),
            bar_track: hex(0x3A3A3A),
            marquee: accent.map(|a| a[3]).unwrap_or(hex(0x0078D4)),
        }
    }

    /// Oxide - тёплый графит, ржаво-медный акцент
    pub fn oxide() -> Self {
        let mut p = Palette::dark(None);
        p.base = hex(0x1E1B19);
        p.layer = hex(0x2B2623);
        p.cmdbar = hex(0x25211F);
        p.divider = hex(0x3B3430);
        p.field = hex(0x36302C);
        p.field_hover = hex(0x3C3531);
        p.field_stroke = hex(0x463E39);
        p.text = hex(0xF3EDE8);
        p.text2 = hex(0xCBC1B9);
        p.text3 = hex(0x8E847C);
        p.text_disabled = hex(0x5E5650);
        p.hover = rgba(0xFFE6D20F);
        p.pressed = rgba(0xFFE6D20A);
        p.selected = rgba(0xE8743B38);
        p.selected_hover = rgba(0xE8743B48);
        p.nav_selected = rgba(0xE8743B2A);
        p.accent = hex(0xE8743B);
        p.marquee = p.accent;
        p.on_accent = hex(0x1E1B19);
        p.flyout = hex(0x2B2623);
        p.flyout_stroke = hex(0x40372F);
        p.bar_track = hex(0x3B3430);
        p.focus = hex(0xF3EDE8);
        p
    }

    /// Графит - чёрный холст для OLED
    pub fn graphite(accent: Option<[Color32; 8]>) -> Self {
        let mut p = Palette::dark(accent);
        p.base = hex(0x000000);
        p.layer = hex(0x141414);
        p.cmdbar = hex(0x0C0C0C);
        p.divider = hex(0x262626);
        p.field = hex(0x1C1C1C);
        p.field_hover = hex(0x222222);
        p.field_stroke = hex(0x2E2E2E);
        p.text = hex(0xF2F2F2);
        p.text2 = hex(0xB8B8B8);
        p.text3 = hex(0x7A7A7A);
        p.flyout = hex(0x161616);
        p.flyout_stroke = hex(0x2C2C2C);
        p.bar_track = hex(0x262626);
        p
    }

    /// Северная ночь - сине-сланцевый тёмный
    pub fn nordic() -> Self {
        let mut p = Palette::dark(None);
        p.base = hex(0x1B2029);
        p.layer = hex(0x252C37);
        p.cmdbar = hex(0x212731);
        p.divider = hex(0x333C49);
        p.field = hex(0x2C3441);
        p.field_hover = hex(0x323B49);
        p.field_stroke = hex(0x3B4554);
        p.text = hex(0xE6EBF2);
        p.text2 = hex(0xB2BDCB);
        p.text3 = hex(0x7D8898);
        p.text_disabled = hex(0x515B69);
        p.hover = rgba(0xC8DCFF12);
        p.pressed = rgba(0xC8DCFF0C);
        p.selected = rgba(0x88B4F035);
        p.selected_hover = rgba(0x88B4F045);
        p.nav_selected = rgba(0x88B4F028);
        p.accent = hex(0x88B4F0);
        p.marquee = p.accent;
        p.on_accent = hex(0x10151C);
        p.flyout = hex(0x252C37);
        p.flyout_stroke = hex(0x384252);
        p.bar_track = hex(0x333C49);
        p.focus = hex(0xE6EBF2);
        p
    }

    /// Бумага - тёплый светлый
    pub fn paper() -> Self {
        let mut p = Palette::light(None);
        p.base = hex(0xF4F1EA);
        p.layer = hex(0xFBF9F4);
        p.cmdbar = hex(0xF8F5EF);
        p.divider = hex(0xE4DDD0);
        p.field = hex(0xFFFDF9);
        p.field_hover = hex(0xFBF8F2);
        p.field_stroke = hex(0xE2DACB);
        p.text = hex(0x2B2722);
        p.text2 = hex(0x6B645A);
        p.text3 = hex(0x958C80);
        p.text_disabled = hex(0xB3AA9D);
        p.hover = rgba(0x5A3C140C);
        p.pressed = rgba(0x5A3C1408);
        p.selected = hex(0xEFDFC4);
        p.selected_hover = hex(0xE8D3B0);
        p.nav_selected = rgba(0x5A3C1414);
        p.accent = hex(0xA8541E);
        p.marquee = p.accent;
        p.on_accent = hex(0xFFFFFF);
        p.flyout = hex(0xFBF9F4);
        p.flyout_stroke = hex(0xE2DACB);
        p.bar_track = hex(0xE4DDD0);
        p.focus = hex(0x2B2722);
        p
    }

    /// Жидкое стекло - прозрачные слои над размытым рабочим столом, светлые блики по краям,
    /// синий акцент. Светлое или тёмное, как Windows
    pub fn glass(dark: bool) -> Self {
        // Каждая панель залита целиком своим цветом (панели не перекрываются), поэтому
        // у строки адреса и командной панели свой тон, а не надбавка к фону
        let mut p = if dark { Palette::dark(None) } else { Palette::light(None) };
        p.glass = true;
        if dark {
            p.base = rgba(0x14181F8C);
            p.layer = rgba(0x2A2F39A6);
            p.cmdbar = rgba(0x1E222A99);
            p.divider = rgba(0xFFFFFF1F);
            // Поля темнее фона: на светлых обоях белый текст остаётся читаемым и верх не слепит
            p.field = rgba(0x0B0E1466);
            p.field_hover = rgba(0x1A1F2873);
            p.field_stroke = rgba(0xFFFFFF26);
            p.text = hex(0xFFFFFF);
            p.text2 = hex(0xDCE0E8);
            p.text3 = hex(0xA9AFBA);
            p.text_disabled = hex(0x6E7480);
            p.hover = rgba(0xFFFFFF1A);
            p.pressed = rgba(0xFFFFFF12);
            p.selected = rgba(0xFFFFFF33);
            p.selected_hover = rgba(0xFFFFFF40);
            p.nav_selected = rgba(0xFFFFFF26);
            p.accent = hex(0x0A84FF);
            p.on_accent = hex(0xFFFFFF);
            p.flyout = rgba(0x262A33F2);
            p.flyout_stroke = rgba(0xFFFFFF38);
            p.shadow = rgba(0x00000066);
            p.bar_track = rgba(0xFFFFFF1F);
            p.focus = hex(0xFFFFFF);
        } else {
            p.base = rgba(0xF2F4F899);
            p.layer = rgba(0xFCFDFFB8);
            p.cmdbar = rgba(0xF7F8FBA6);
            p.divider = rgba(0x0000001A);
            p.field = rgba(0xFFFFFFB3);
            p.field_hover = rgba(0xFFFFFFD9);
            p.field_stroke = rgba(0x0000001F);
            p.text = hex(0x1C1C1E);
            p.text2 = hex(0x3C3C43);
            p.text3 = hex(0x66666D);
            p.text_disabled = hex(0x9E9EA4);
            p.hover = rgba(0xFFFFFF80);
            p.pressed = rgba(0xFFFFFF59);
            p.selected = rgba(0x007AFF2E);
            p.selected_hover = rgba(0x007AFF40);
            p.nav_selected = rgba(0xFFFFFF99);
            p.accent = hex(0x007AFF);
            p.on_accent = hex(0xFFFFFF);
            p.flyout = rgba(0xF9FAFCF2);
            p.flyout_stroke = rgba(0x00000024);
            p.shadow = rgba(0x00000033);
            p.bar_track = rgba(0x0000001A);
            p.focus = hex(0x1C1C1E);
        }
        p.marquee = p.accent;
        p.shine = 0.7;
        p
    }

    /// Настройки стекла пользователя: прозрачность слоёв и яркость бликов
    pub fn tune_glass(mut self, clarity: f32, shine: f32) -> Self {
        if !self.glass {
            return self;
        }
        let clarity = clarity.clamp(0.0, 1.0);
        // Уже на середине ползунка стекло заметно прозрачное, на максимуме - почти чистое
        let base_a = 0.04 + 0.6 * (1.0 - clarity).powf(1.3);
        let with_a = |c: Color32, a: f32| {
            let [r, g, b, _] = c.to_srgba_unmultiplied();
            Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
        };
        self.base = with_a(self.base, base_a);
        self.layer = with_a(self.layer, base_a + 0.10);
        self.cmdbar = with_a(self.cmdbar, base_a + 0.05);
        self.shine = shine.clamp(0.0, 1.0);
        self
    }

    /// Если Windows не умеет размытие под окном, стекло становится непрозрачным: слои
    /// смешиваются с подложкой заранее
    pub fn solid(mut self) -> Self {
        if !self.glass {
            return self;
        }
        let under = if self.dark { hex(0x1C2028) } else { hex(0xE9ECF2) };
        let flat = |c: Color32, under: Color32| {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            let k = a as f32 / 255.0;
            let mix = |o: u8, u: u8| (u as f32 * (1.0 - k) + o as f32 * k).round() as u8;
            Color32::from_rgb(mix(r, under.r()), mix(g, under.g()), mix(b, under.b()))
        };
        self.base = flat(self.base, under);
        let base = self.base;
        for c in [&mut self.layer, &mut self.cmdbar, &mut self.field, &mut self.field_hover, &mut self.flyout] {
            *c = flat(*c, base);
        }
        self.glass = false;
        self.shine = 0.0;
        self
    }

    pub fn resolve(choice: ThemeChoice, system_dark: bool, accent: Option<[Color32; 8]>) -> Self {
        match choice {
            ThemeChoice::System => {
                if system_dark {
                    Palette::dark(accent)
                } else {
                    Palette::light(accent)
                }
            }
            ThemeChoice::Light => Palette::light(accent),
            ThemeChoice::Dark => Palette::dark(accent),
            ThemeChoice::Oxide => Palette::oxide(),
            ThemeChoice::Graphite => Palette::graphite(accent),
            ThemeChoice::Nordic => Palette::nordic(),
            ThemeChoice::Paper => Palette::paper(),
            ThemeChoice::Glass => Palette::glass(system_dark),
        }
    }

    /// Полупрозрачный слой поверх текущей подложки → непрозрачный цвет
    /// Смешивание в гамма-пространстве, как в WinUI (egui на sRGB-буфере смешивает линейно, слишком светло)
    pub fn fill(&self, overlay: Color32) -> Color32 {
        let [r, g, b, a] = overlay.to_srgba_unmultiplied();
        if a == 255 {
            return overlay;
        }
        let bg = SURFACE.with(|s| s.get());
        if bg.a() < 255 {
            // Подложка прозрачная (стекло): смешает видеокарта
            return overlay;
        }
        let k = a as f32 / 255.0;
        let mix = |o: u8, u: u8| (u as f32 * (1.0 - k) + o as f32 * k).round() as u8;
        Color32::from_rgb(mix(r, bg.r()), mix(g, bg.g()), mix(b, bg.b()))
    }

    /// Полупрозрачная заливка рамки выделения: файлы под ней остаются видны
    pub fn marquee_fill(&self) -> Color32 {
        let c = self.marquee;
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), if self.dark { 40 } else { 45 })
    }

    /// Цвет акцента, читаемый как текст на подложке (ссылки, выбранные пункты)
    pub fn accent_text(&self) -> Color32 {
        self.accent
    }
}

thread_local! {
    static SURFACE: std::cell::Cell<Color32> = const { std::cell::Cell::new(Color32::from_rgb(0x20, 0x20, 0x20)) };
}

/// Текущая подложка, над которой рисуются подсветки
pub fn set_surface(c: Color32) {
    SURFACE.with(|s| s.set(c));
}

/// Метрики при 100 % (логические пиксели = DIP)
pub mod m {
    pub const TITLE_H: f32 = 40.0;
    pub const TAB_H: f32 = 32.0;
    pub const TAB_MAX_W: f32 = 240.0;
    pub const TAB_MIN_W: f32 = 80.0;
    pub const CAPTION_W: f32 = 46.0;
    pub const CAPTION_H: f32 = 32.0;
    pub const ADDR_H: f32 = 48.0;
    pub const FIELD_H: f32 = 32.0;
    pub const NAV_BTN_W: f32 = 40.0;
    pub const CMD_H: f32 = 48.0;
    pub const BTN_H: f32 = 32.0;
    pub const STATUS_H: f32 = 28.0;
    pub const HEADER_H: f32 = 32.0;
    pub const ROW_H: f32 = 28.0;
    pub const ROW_H_COMPACT: f32 = 22.0;
    pub const NAV_ROW_H: f32 = 32.0;
    pub const NAV_ROW_H_COMPACT: f32 = 24.0;
    pub const NAV_INDENT: f32 = 16.0;
    pub const MENU_ROW_H: f32 = 32.0;
    pub const R_CTRL: u8 = 4;
    pub const R_OVERLAY: u8 = 8;
    pub const T_HOVER: f32 = 0.083;
    pub const T_MENU: f32 = 0.167;
    pub const SEARCH_W: f32 = 220.0;
    /// Полоса прокрутки: тонкая всегда видна, при наведении шире; ползунок не короче SCROLL_HANDLE_MIN
    pub const SCROLL_W_IDLE: f32 = 6.0;
    pub const SCROLL_W: f32 = 10.0;
    pub const SCROLL_HANDLE_MIN: f32 = 48.0;
}

/// Читает системную палитру акцента (8 цветов: Light3..Dark3)
pub fn system_accent_palette() -> Option<[Color32; 8]> {
    let data = crate::util::reg_read_binary(
        windows::Win32::System::Registry::HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent",
        "AccentPalette",
    )?;
    if data.len() < 32 {
        return None;
    }
    let mut out = [Color32::BLACK; 8];
    for (i, c) in out.iter_mut().enumerate() {
        *c = Color32::from_rgb(data[i * 4], data[i * 4 + 1], data[i * 4 + 2]);
    }
    Some(out)
}

pub fn system_is_dark() -> bool {
    crate::util::reg_read_dword(
        windows::Win32::System::Registry::HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    )
    .map(|v| v == 0)
    .unwrap_or(false)
}

/// Анимации Windows включены?
pub fn system_animations_enabled() -> bool {
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut v = BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut v as *mut BOOL as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    v.as_bool()
}

pub fn apply(ctx: &egui::Context, p: &Palette, animations: bool) {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.base;
    v.window_fill = p.flyout;
    v.extreme_bg_color = p.field;
    v.faint_bg_color = p.layer;
    v.code_bg_color = p.field;
    v.window_stroke = Stroke::new(1.0, p.flyout_stroke);
    v.window_corner_radius = CornerRadius::same(m::R_OVERLAY);
    v.menu_corner_radius = CornerRadius::same(m::R_OVERLAY);
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 16, spread: 0, color: p.shadow };
    v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 16, spread: 0, color: p.shadow };
    v.selection.bg_fill = p.accent.gamma_multiply(if p.dark { 0.55 } else { 0.35 });
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.hyperlink_color = p.accent;
    v.override_text_color = Some(p.text);
    v.text_cursor.stroke = Stroke::new(1.0, p.text);
    v.striped = false;
    v.resize_corner_size = 0.0;

    let r = CornerRadius::same(m::R_CTRL);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
        w.fg_stroke = Stroke::new(1.0, p.text);
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_fill = p.base;
    v.widgets.noninteractive.weak_bg_fill = p.base;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.divider);
    v.widgets.inactive.bg_fill = p.field;
    v.widgets.inactive.weak_bg_fill = p.field;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.field_stroke);
    v.widgets.hovered.bg_fill = p.field_hover;
    v.widgets.hovered.weak_bg_fill = p.field_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.field_stroke);
    v.widgets.active.bg_fill = p.field;
    v.widgets.active.weak_bg_fill = p.field;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.open = v.widgets.active;

    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.animation_time = if animations { m::T_HOVER } else { 0.0 };
        s.spacing.item_spacing = egui::vec2(8.0, 4.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = m::BTN_H;
        // Полоса прокрутки как в Windows 11: тонкая видна всегда, при наведении расширяется,
        // ползунок не короче m::SCROLL_HANDLE_MIN даже в папке с тысячами файлов
        let mut sc = egui::style::ScrollStyle::floating();
        sc.floating_width = m::SCROLL_W_IDLE;
        sc.bar_width = m::SCROLL_W;
        sc.floating_allocated_width = 0.0;
        sc.bar_inner_margin = 2.0;
        sc.bar_outer_margin = 2.0;
        sc.foreground_color = true;
        sc.dormant_background_opacity = 0.0;
        sc.active_background_opacity = 0.0;
        sc.interact_background_opacity = 0.15;
        sc.dormant_handle_opacity = 0.35;
        sc.active_handle_opacity = 0.55;
        sc.interact_handle_opacity = 0.75;
        s.spacing.scroll = sc;
        s.interaction.tooltip_delay = 0.6;
        s.interaction.selectable_labels = false;
        s.url_in_tooltip = false;
    });
}

/// Включены ли в Windows эффекты прозрачности. Если они выключены (или включена экономия заряда),
/// Windows рисует размытие за окном непрозрачным
pub fn system_transparency() -> bool {
    let on = crate::util::reg_read_dword(
        windows::Win32::System::Registry::HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "EnableTransparency",
    )
    .map(|v| v != 0)
    .unwrap_or(true);
    let mut ps = windows::Win32::System::Power::SYSTEM_POWER_STATUS::default();
    let saver = unsafe { windows::Win32::System::Power::GetSystemPowerStatus(&mut ps).is_ok() } && ps.SystemStatusFlag == 1;
    on && !saver
}

/// Сколько точек прокручивает один щелчок колёсика: как в Проводнике, число строк из параметров
/// мыши Windows (обычно 3) на высоту строки. «На один экран» - примерно десять строк
pub fn wheel_step() -> f32 {
    use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    let mut lines: u32 = 3;
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, Some(&mut lines as *mut u32 as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    let lines = if lines == u32::MAX { 10 } else { lines.clamp(1, 30) };
    lines as f32 * 36.0
}
