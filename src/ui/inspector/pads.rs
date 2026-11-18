//! "Loop pads" tab.

use crate::dsp::drums::BEATS;
use crate::engine::{stop_and_finish, PadSnap, PadState, Snapshot};
use crate::i18n::tr;
use crate::ui::theme::*;
use crate::ui::widgets::bars_label;
use crate::ui::App;
use eframe::egui::{self, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke};

/// Name shown on a pad: preset beats follow the UI language, recordings keep their name.
fn pad_title(p: &PadSnap) -> &str {
    p.beat.and_then(|b| BEATS.get(b as usize)).map_or(p.name.as_str(), |b| b.name.get())
}

impl App {
    pub(super) fn pads_tab(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let sel_name =
            snap.tracks.iter().find(|t| t.id == snap.selected).map(|t| format!("{} {}", track_icon(t.params.icon), t.params.name));
        ui.horizontal(|ui| {
            ui.label(tr("Clicking an empty pad records from:", "Leeres Pad anklicken nimmt auf von:"));
            ui.label(RichText::new(sel_name.unwrap_or("–".into())).font(semibold(14.5)));
            ui.separator();
            ui.label(tr("Length:", "Länge:"));
            let mut bars = snap.pad_bars;
            for b in [1u32, 2, 4, 8] {
                ui.selectable_value(&mut bars, b, format!("{b}"));
            }
            ui.label(tr("bars", "Takte"));
            if bars != snap.pad_bars {
                self.engine.lock().pad_bars = bars;
            }
            ui.separator();
            let mut pm = snap.pads_from_midi;
            if ui
                .checkbox(&mut pm, tr("MPK pads control the loop pads", "MPK-Pads steuern die Loop-Pads"))
                .on_hover_text(tr(
                    "On: the 8 pads on the MPK start/stop the loop pads.\nOff: the pads play drums.",
                    "An: Die 8 Pads am MPK starten/stoppen die Loop-Pads.\nAus: Die Pads spielen Schlagzeug.",
                ))
                .changed()
            {
                self.engine.lock().pads_from_midi = pm;
            }
            ui.separator();
            if ui
                .button(tr("🥁 Load preset beats", "🥁 Fertige Beats laden"))
                .on_hover_text(tr("Fills empty pads with drum beats", "Füllt leere Pads mit Schlagzeug-Beats"))
                .clicked()
            {
                let mut b = 0;
                for (i, p) in snap.pads.iter().enumerate() {
                    if p.state == PadState::Empty && b < 4 {
                        self.load_beat(i, b, snap);
                        b += 1;
                    }
                }
            }
            if ui.button(tr("⏹ Stop all", "⏹ Alle stoppen")).clicked() {
                stop_and_finish(&self.engine);
            }
        });
        ui.add_space(4.0);
        for row in 0..2 {
            ui.horizontal(|ui| {
                for c in 0..4 {
                    let i = row * 4 + c;
                    self.pad_widget(ui, i, &snap.pads[i], snap);
                }
                if row == 0 {
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tr("How it works", "So geht's")).font(semibold(14.5)));
                        ui.label(RichText::new(tr("• Click an empty pad → after the count-in the selected track is recorded\n  and then loops automatically.\n• Click a filled pad → starts (on the next bar) or stops it.\n• Right-click → clear the pad or load a preset beat.\n• Several pads always run in sync.", "• Leeres Pad anklicken → nach dem Einzähler wird die ausgewählte Spur aufgenommen\n  und läuft danach automatisch im Kreis.\n• Volles Pad anklicken → startet (beim nächsten Takt) bzw. stoppt.\n• Rechtsklick → leeren oder fertigen Beat laden.\n• Mehrere Pads laufen immer im gleichen Takt.")).small().weak());
                    });
                }
            });
        }
    }

    fn pad_widget(&mut self, ui: &mut egui::Ui, i: usize, p: &PadSnap, snap: &Snapshot) {
        ui.vertical(|ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(178.0, 70.0), Sense::click());
            let blink = (ui.input(|i| i.time) * 3.0).sin() > 0.0;
            let (fill, line) = match p.state {
                PadState::Empty => (CARD, tr("+ empty – click to record", "+ leer – klicken zum Aufnehmen").to_string()),
                PadState::QueuedRec => (
                    if blink { AMBER.linear_multiply(0.8) } else { AMBER.linear_multiply(0.5) },
                    if snap.pad_countin {
                        tr("Count-in… here we go", "Einzähler… gleich geht's los").into()
                    } else {
                        tr("Recording starts on the next bar", "Aufnahme startet beim nächsten Takt").into()
                    },
                ),
                PadState::Recording => (RED, tr("● Recording", "● Aufnahme läuft").into()),
                PadState::Playing => (GREEN, tr("▶ Playing", "▶ läuft").into()),
                PadState::QueuedPlay => (
                    if blink { GREEN.linear_multiply(0.75) } else { GREEN.linear_multiply(0.45) },
                    tr("starts on the next bar", "startet beim nächsten Takt").into(),
                ),
                PadState::Stopped => (mix(CARD, ACCENT, 0.45), tr("■ click to play", "■ klicken zum Abspielen").into()),
            };
            let fill = if resp.hovered() { fill.linear_multiply(1.15) } else { fill };
            let pt = ui.painter();
            pt.rect_filled(rect, 12.0, fill);
            if p.state == PadState::Empty {
                pt.rect_stroke(rect.shrink(0.5), 12.0, Stroke::new(1.0f32, BORDER));
            }
            pt.text(
                rect.min + vec2(8.0, 6.0),
                Align2::LEFT_TOP,
                format!("{}", i + 1),
                FontId::proportional(12.0),
                Color32::from_white_alpha(150),
            );
            if p.state != PadState::Empty {
                let warn = if p.stale_tempo && p.beat.is_none() { tr("  ⚠ different tempo", "  ⚠ anderes Tempo") } else { "" };
                pt.text(
                    rect.min + vec2(24.0, 5.0),
                    Align2::LEFT_TOP,
                    format!("{} ({}){}", pad_title(p), bars_label(p.bars), warn),
                    FontId::proportional(14.5),
                    Color32::WHITE,
                );
            }
            pt.text(
                rect.left_bottom() + vec2(10.0, -20.0),
                Align2::LEFT_CENTER,
                line,
                FontId::proportional(12.0),
                Color32::from_white_alpha(220),
            );
            if matches!(p.state, PadState::Playing | PadState::Recording) {
                let bar = Rect::from_min_size(rect.left_bottom() + vec2(6.0, -9.0), vec2((rect.width() - 12.0) * p.progress, 4.0));
                pt.rect_filled(bar, 2.0, Color32::WHITE);
            }
            if resp.clicked() {
                self.engine.lock().pad_toggle(i);
            }
            resp.context_menu(|ui| {
                if ui.button(tr("🗑 Clear pad", "🗑 Pad leeren")).clicked() {
                    self.engine.lock().pad_clear(i);
                    ui.close_menu();
                }
                ui.menu_button(tr("🥁 Load preset beat", "🥁 Fertigen Beat laden"), |ui| {
                    for (b, beat) in BEATS.iter().enumerate() {
                        if ui.button(beat.name.get()).clicked() {
                            self.load_beat(i, b, snap);
                            ui.close_menu();
                        }
                    }
                });
            });
            let mut v = p.volume;
            ui.spacing_mut().slider_width = 170.0;
            if ui
                .add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false))
                .on_hover_text(tr("Volume of this pad", "Lautstärke dieses Pads"))
                .changed()
            {
                self.engine.lock().pads[i].volume = v;
            }
        });
    }
}
