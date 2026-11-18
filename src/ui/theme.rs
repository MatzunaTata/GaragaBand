//! Look and feel: layout constants, colour palette, fonts and the egui style.

use eframe::egui::{self, vec2, Color32, FontFamily, FontId, Stroke};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

// ---------------------------------------------------------------- Layout

/// Width of the track header column left of the timeline.
pub(super) const HEADER_W: f32 = 320.0;
/// Height of one track row.
pub(super) const ROW_H: f32 = 72.0;

// ---------------------------------------------------------------- Palette

/// Appearance of the user interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    /// Parses `dark` or `light` (used by `--theme`).
    pub fn from_code(code: &str) -> Option<ThemeMode> {
        match code.to_ascii_lowercase().as_str() {
            "dark" => Some(ThemeMode::Dark),
            "light" => Some(ThemeMode::Light),
            _ => None,
        }
    }
}

/// The colours of one appearance. Panels read them through [`pal`], so
/// switching the theme takes effect on the next frame.
pub(super) struct Palette {
    pub dark: bool,
    /// Window background behind the timeline.
    pub bg: Color32,
    /// Top bar, bottom panel and floating windows.
    pub panel: Color32,
    /// Raised surfaces: effect cards, track headers, empty pads.
    pub card: Color32,
    pub lane: Color32,
    pub lane_selected: Color32,
    pub border: Color32,
    pub widget: Color32,
    pub widget_hover: Color32,
    pub widget_active: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub green: Color32,
    pub red: Color32,
    pub amber: Color32,
    /// Sunken areas: meter backgrounds, slider rails, text fields.
    pub well: Color32,
}

static DARK: Palette = Palette {
    dark: true,
    bg: Color32::from_rgb(15, 16, 21),
    panel: Color32::from_rgb(23, 25, 32),
    card: Color32::from_rgb(32, 35, 45),
    lane: Color32::from_rgb(21, 23, 29),
    lane_selected: Color32::from_rgb(27, 30, 39),
    border: Color32::from_rgb(46, 50, 64),
    widget: Color32::from_rgb(45, 49, 63),
    widget_hover: Color32::from_rgb(58, 63, 81),
    widget_active: Color32::from_rgb(70, 76, 98),
    text: Color32::from_rgb(228, 230, 238),
    text_dim: Color32::from_rgb(140, 146, 165),
    accent: Color32::from_rgb(99, 116, 255),
    green: Color32::from_rgb(40, 174, 108),
    red: Color32::from_rgb(226, 67, 76),
    amber: Color32::from_rgb(240, 180, 62),
    well: Color32::from_rgb(11, 12, 16),
};

static LIGHT: Palette = Palette {
    dark: false,
    bg: Color32::from_rgb(236, 238, 244),
    panel: Color32::from_rgb(247, 248, 251),
    card: Color32::from_rgb(255, 255, 255),
    lane: Color32::from_rgb(226, 229, 237),
    lane_selected: Color32::from_rgb(214, 219, 231),
    border: Color32::from_rgb(206, 211, 224),
    widget: Color32::from_rgb(225, 228, 238),
    widget_hover: Color32::from_rgb(210, 215, 229),
    widget_active: Color32::from_rgb(192, 198, 217),
    text: Color32::from_rgb(28, 31, 42),
    text_dim: Color32::from_rgb(98, 105, 126),
    accent: Color32::from_rgb(82, 99, 240),
    green: Color32::from_rgb(28, 156, 94),
    red: Color32::from_rgb(214, 58, 68),
    amber: Color32::from_rgb(204, 136, 20),
    well: Color32::from_rgb(203, 208, 221),
};

static LIGHT_MODE: AtomicBool = AtomicBool::new(false);

/// The active colour palette.
pub(super) fn pal() -> &'static Palette {
    if LIGHT_MODE.load(Ordering::Relaxed) {
        &LIGHT
    } else {
        &DARK
    }
}

pub(super) fn mode() -> ThemeMode {
    if pal().dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    }
}

/// Text colour that stays readable on the given fill (white or near-black).
pub(super) fn on_color(fill: Color32) -> Color32 {
    let luminance = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luminance > 165.0 {
        Color32::from_rgb(28, 31, 42)
    } else {
        Color32::WHITE
    }
}

/// Icons of the four track types (voice, guitar, keyboard, drums).
pub(super) const ICONS: [&str; 4] = ["🎤", "🎸", "🎹", "🥁"];

/// Icon of a track type; unknown values fall back to the drum icon.
pub(super) fn track_icon(icon: u8) -> &'static str {
    ICONS[(icon as usize).min(ICONS.len() - 1)]
}

/// Signature colour of a track type.
pub(super) fn track_color(icon: u8) -> Color32 {
    match icon {
        0 => Color32::from_rgb(84, 152, 255),
        1 => Color32::from_rgb(255, 158, 68),
        2 => Color32::from_rgb(58, 199, 138),
        _ => Color32::from_rgb(176, 124, 255),
    }
}

/// Linear blend between two colours (`t` = 0 gives `a`, 1 gives `b`).
pub(super) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

// ---------------------------------------------------------------- Fonts

const SEMIBOLD: &str = "semibold";

/// Semi-bold font for headings, track names and button labels.
pub(super) fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Uses the Windows system fonts where available: Segoe UI for text and the
/// Segoe symbol/emoji fonts as fallback so that every icon (🥁 🎧 …) can be shown.
/// Without them egui's bundled fonts are used.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::path::Path::new(&std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
    let mut load = |name: &str, file: &str| match std::fs::read(dir.join(file)) {
        Ok(bytes) => {
            fonts.font_data.insert(name.into(), egui::FontData::from_owned(bytes));
            true
        }
        Err(_) => false,
    };
    let has_text = load("segoe_ui", "segoeui.ttf");
    let has_semibold = load("segoe_ui_semibold", "seguisb.ttf");
    let fallbacks: Vec<&str> =
        [("win_symbol", "seguisym.ttf"), ("win_emoji", "seguiemj.ttf")].into_iter().filter(|(n, f)| load(n, f)).map(|(n, _)| n).collect();

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    if has_text {
        proportional.insert(0, "segoe_ui".into());
    }
    proportional.extend(fallbacks.iter().map(|n| n.to_string()));
    // The semi-bold family shares all fallbacks of the regular text font.
    let mut heading = proportional.clone();
    if has_semibold {
        heading.insert(0, "segoe_ui_semibold".into());
    }
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), heading);
    fonts.families.entry(FontFamily::Monospace).or_default().extend(fallbacks.iter().map(|n| n.to_string()));
    fonts
}

// ---------------------------------------------------------------- Style

fn visuals() -> egui::Visuals {
    let p = pal();
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0f32, p.border);
    v.window_rounding = 12.0.into();
    v.menu_rounding = 10.0.into();
    v.extreme_bg_color = p.well;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0f32, Color32::WHITE);
    v.slider_trailing_fill = true;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, p.border);
    for (w, fill) in [
        (&mut v.widgets.inactive, p.widget),
        (&mut v.widgets.hovered, p.widget_hover),
        (&mut v.widgets.active, p.widget_active),
        (&mut v.widgets.open, p.widget_hover),
    ] {
        w.rounding = 8.0.into();
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke.color = p.text;
        w.expansion = 0.0;
    }
    v
}

/// Switches between the dark and the light appearance.
pub(super) fn set_mode(ctx: &egui::Context, mode: ThemeMode) {
    LIGHT_MODE.store(mode == ThemeMode::Light, Ordering::Relaxed);
    let (preference, theme) = match mode {
        ThemeMode::Dark => (egui::ThemePreference::Dark, egui::Theme::Dark),
        ThemeMode::Light => (egui::ThemePreference::Light, egui::Theme::Light),
    };
    ctx.set_theme(preference);
    ctx.set_visuals_of(theme, visuals());
}

/// Installs fonts, spacing and the colours of `mode`. Call once at start-up.
pub(super) fn apply(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_fonts(fonts());
    ctx.all_styles_mut(|s| {
        for (style, font) in s.text_styles.iter_mut() {
            font.size = match style {
                egui::TextStyle::Heading => 20.0,
                egui::TextStyle::Small => 11.5,
                _ => 14.0,
            };
        }
        s.spacing.button_padding = vec2(10.0, 5.0);
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.menu_margin = egui::Margin::same(8.0);
        s.spacing.window_margin = egui::Margin::same(14.0);
    });
    set_mode(ctx, mode);
}
