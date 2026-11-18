//! Small reusable widgets shared by the panels.

use super::theme::*;
use crate::i18n::tr;
use crate::trf;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, Layout, Rect, RichText, Sense, Stroke};

/// Maps a linear level to 0..1 on a 54 dB scale for meters.
pub(super) fn level_frac(level: f32) -> f32 {
    ((20.0 * (level + 1e-6).log10() + 54.0) / 54.0).clamp(0.0, 1.0)
}

/// Meter colour: green, amber when hot, red close to clipping.
pub(super) fn level_color(level: f32) -> Color32 {
    if level > 0.9 {
        RED
    } else if level > 0.5 {
        AMBER
    } else {
        GREEN
    }
}

/// Paints a level meter into `rect`, filling from the left or from the bottom.
pub(super) fn paint_meter(painter: &egui::Painter, rect: Rect, level: f32, vertical: bool) {
    let rounding = rect.width().min(rect.height()) / 2.0;
    painter.rect_filled(rect, rounding, Color32::from_rgb(11, 12, 16));
    let mut fill = rect;
    if vertical {
        fill.set_top(rect.bottom() - rect.height() * level_frac(level));
    } else {
        fill.set_width(rect.width() * level_frac(level));
    }
    painter.rect_filled(fill, rounding, level_color(level));
}

/// Horizontal level meter of the given width.
pub(super) fn meter_h(ui: &mut egui::Ui, level: f32, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, 8.0), Sense::hover());
    paint_meter(ui.painter(), rect, level, false);
}

/// "1 bar" / "4 bars" in the active language.
pub(super) fn bars_label(bars: u32) -> String {
    if bars == 1 {
        tr("1 bar", "1 Takt").to_string()
    } else {
        trf!("{} bars", "{} Takte", bars)
    }
}

/// Rounded square in the track colour with the track icon inside.
pub(super) fn icon_badge(ui: &mut egui::Ui, icon: u8, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, size * 0.3, track_color(icon).linear_multiply(0.9));
    painter.text(rect.center(), Align2::CENTER_CENTER, track_icon(icon), egui::FontId::proportional(size * 0.56), Color32::WHITE);
}

/// Animated on/off switch.
pub(super) fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let radius = rect.height() / 2.0;
    let track = mix(if resp.hovered() { WIDGET_ACTIVE } else { WIDGET_HOVER }, GREEN, t);
    ui.painter().rect_filled(rect, radius, track);
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    ui.painter().circle_filled(pos2(x, rect.center().y), radius - 3.0, Color32::WHITE);
    resp
}

/// Large pill-shaped button for the transport controls and the tabs.
pub(super) fn pill_button(text: &str, fill: Color32, height: f32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).font(semibold(15.0)).color(Color32::WHITE))
        .fill(fill)
        .rounding(height / 2.0)
        .min_size(vec2(height, height))
}

/// A titled card of the effect rack. With `on`, the card gets a switch and its
/// body is disabled while the effect is off.
pub(super) fn card(ui: &mut egui::Ui, title: &str, hint: &str, on: Option<&mut bool>, w: f32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0f32, BORDER)).rounding(12.0).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(w);
        ui.set_min_height(240.0);
        ui.vertical(|ui| {
            ui.spacing_mut().slider_width = w - 16.0;
            let mut enabled = true;
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).font(semibold(15.5))).on_hover_text(hint);
                if let Some(on) = on {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        toggle_switch(ui, on).on_hover_text(tr("Click to switch on/off", "Klicken zum Ein-/Ausschalten"));
                    });
                    enabled = *on;
                }
            });
            ui.label(RichText::new(hint).small().color(TEXT_DIM));
            ui.add_space(6.0);
            ui.add_enabled_ui(enabled, body);
        });
    });
}

/// Labelled slider without a numeric value (the UI avoids technical numbers).
pub(super) fn sl(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.label(RichText::new(label).small());
    ui.add(egui::Slider::new(v, range).show_value(false));
}
