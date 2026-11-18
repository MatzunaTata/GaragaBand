//! Status bar: audio device, MIDI devices, CPU load and a context hint.

use super::theme::*;
use super::App;
use crate::engine::Snapshot;
use crate::i18n::tr;
use crate::trf;
use eframe::egui::{self, Align, Layout, RichText};

impl App {
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let st = self.status();
        ui.horizontal(|ui| {
            if self.no_audio {
                ui.colored_label(AMBER, tr("Audio is switched off (--no-audio)", "Audio ist ausgeschaltet (--no-audio)"));
            } else if let Some(err) = &st.error {
                ui.colored_label(RED, trf!("⚠ No sound: {}", "⚠ Kein Ton: {}", err));
                if ui.small_button(tr("Reconnect", "Neu verbinden")).clicked() {
                    self.start_audio();
                }
            } else if st.running {
                let c = if st.latency_ms < 15.0 { GREEN } else { AMBER };
                ui.colored_label(c, "●");
                ui.label(format!("🎧 {}", st.out_name));
                if st.in_name.is_empty() {
                    ui.colored_label(AMBER, tr("🎤 no input", "🎤 kein Eingang"));
                } else {
                    ui.label(format!("🎤 {}", st.in_name));
                }
                ui.label(
                    RichText::new(trf!(
                        "{} · latency approx. {:.0} ms · {} Hz",
                        "{} · Verzögerung ca. {:.0} ms · {} Hz",
                        st.mode.label(),
                        st.latency_ms,
                        st.sr
                    ))
                    .weak(),
                )
                .on_hover_text(st.notes.join("\n"));
            } else {
                ui.label(tr("Audio is starting…", "Audio startet…"));
            }
            ui.separator();
            let flash = self.midi_flash.elapsed().as_secs_f32() < 0.15;
            ui.colored_label(if flash { GREEN } else { TEXT_DIM }, "●");
            if self.midi.names.is_empty() {
                ui.label(
                    RichText::new(tr(
                        "🎹 No keyboard connected (the computer keys A–L play too)",
                        "🎹 Kein Keyboard angeschlossen (Computertasten A–L spielen auch)",
                    ))
                    .weak(),
                );
            } else {
                ui.label(format!("🎹 {}", self.midi.names.join(", ")));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(trf!("Load {:.0} %", "Auslastung {:.0} %", st.cpu * 100.0)).weak());
                let hint = if snap.tr.recording {
                    tr("Recording. The space bar or Stop ends it.", "Aufnahme läuft. Leertaste oder Stopp beendet sie.")
                } else if snap.tr.playing {
                    tr("Space bar = stop", "Leertaste = Stopp")
                } else {
                    tr(
                        "Click a track → press Record.  Space bar = play · R = record · Del = delete recording",
                        "Spur anklicken → Aufnehmen drücken.  Leertaste = Abspielen · R = Aufnehmen · Entf = Aufnahme löschen",
                    )
                };
                ui.label(RichText::new(hint).weak());
            });
        });
    }
}
