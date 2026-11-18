//! Look and feel: layout constants, colour palette, fonts and the egui style.

use eframe::egui::{self, vec2, Color32, FontFamily, FontId, Stroke};

// ---------------------------------------------------------------- Layout

/// Width of the track header column left of the timeline.
pub(super) const HEADER_W: f32 = 320.0;
/// Height of one track row.
pub(super) const ROW_H: f32 = 72.0;

// ---------------------------------------------------------------- Palette

pub(super) const BG: Color32 = Color32::from_rgb(15, 16, 21);
pub(super) const PANEL: Color32 = Color32::from_rgb(23, 25, 32);
pub(super) const CARD: Color32 = Color32::from_rgb(32, 35, 45);
pub(super) const LANE: Color32 = Color32::from_rgb(21, 23, 29);
pub(super) const LANE_SELECTED: Color32 = Color32::from_rgb(27, 30, 39);
pub(super) const BORDER: Color32 = Color32::from_rgb(46, 50, 64);
pub(super) const WIDGET: Color32 = Color32::from_rgb(45, 49, 63);
pub(super) const WIDGET_HOVER: Color32 = Color32::from_rgb(58, 63, 81);
pub(super) const WIDGET_ACTIVE: Color32 = Color32::from_rgb(70, 76, 98);
pub(super) const TEXT: Color32 = Color32::from_rgb(228, 230, 238);
pub(super) const TEXT_DIM: Color32 = Color32::from_rgb(140, 146, 165);
pub(super) const ACCENT: Color32 = Color32::from_rgb(99, 116, 255);
pub(super) const GREEN: Color32 = Color32::from_rgb(40, 174, 108);
pub(super) const RED: Color32 = Color32::from_rgb(226, 67, 76);
pub(super) const AMBER: Color32 = Color32::from_rgb(240, 180, 62);

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
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0f32, BORDER);
    v.window_rounding = 12.0.into();
    v.menu_rounding = 10.0.into();
    v.extreme_bg_color = Color32::from_rgb(11, 12, 16);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0f32, Color32::WHITE);
    v.slider_trailing_fill = true;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, BORDER);
    for (w, fill) in [
        (&mut v.widgets.inactive, WIDGET),
        (&mut v.widgets.hovered, WIDGET_HOVER),
        (&mut v.widgets.active, WIDGET_ACTIVE),
        (&mut v.widgets.open, WIDGET_HOVER),
    ] {
        w.rounding = 8.0.into();
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::NONE;
        w.expansion = 0.0;
    }
    v
}

/// Installs fonts, colours and spacing. Call once at start-up.
pub(super) fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
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
}
