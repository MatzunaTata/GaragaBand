//! Floating windows: delete confirmation, audio settings, help and toasts.

use super::theme::*;
use super::App;
use crate::engine::{stop_and_finish, Snapshot};
use crate::i18n::tr;
use crate::trf;
use eframe::egui::{self, Align2, Color32, RichText};

impl App {
    pub(super) fn windows(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        if let Some(id) = self.confirm_delete {
            let name = snap.tracks.iter().find(|t| t.id == id).map(|t| t.params.name.clone()).unwrap_or_default();
            egui::Window::new(tr("Delete track?", "Spur löschen?"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(trf!(
                        "Really delete the track “{}” with all its recordings?",
                        "Die Spur »{}« mit allen Aufnahmen wirklich löschen?",
                        name
                    ));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Yes, delete", "Ja, löschen")).clicked() {
                            stop_and_finish(&self.engine);
                            self.engine.lock().remove_track(id);
                            self.confirm_delete = None;
                        }
                        if ui.button(tr("Cancel", "Abbrechen")).clicked() {
                            self.confirm_delete = None;
                        }
                    });
                });
        }

        let mut open = self.show_settings;
        egui::Window::new(tr("⚙ Audio settings", "⚙ Audio-Einstellungen")).id(egui::Id::new("audio_settings")).open(&mut open).collapsible(false).resizable(false).default_width(470.0).show(ctx, |ui| {
            let st = self.status();
            let mut changed = false;
            egui::Grid::new("devs").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(tr("Headphones / output:", "Kopfhörer / Ausgang:"));
                let cur = self.cfg.out_dev.clone().unwrap_or(tr("Automatic (audio interface preferred)", "Automatisch (Audio-Interface bevorzugt)").into());
                egui::ComboBox::from_id_salt("outdev").width(300.0).selected_text(cur).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut self.cfg.out_dev, None, tr("Automatic (audio interface preferred)", "Automatisch (Audio-Interface bevorzugt)")).changed();
                    for d in self.devices.0.clone() {
                        changed |= ui.selectable_value(&mut self.cfg.out_dev, Some(d.clone()), d).changed();
                    }
                });
                ui.end_row();
                ui.label(tr("Microphone / input:", "Mikrofon / Eingang:"));
                let cur = self.cfg.in_dev.clone().unwrap_or(tr("Automatic (audio interface preferred)", "Automatisch (Audio-Interface bevorzugt)").into());
                egui::ComboBox::from_id_salt("indev").width(300.0).selected_text(cur).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut self.cfg.in_dev, None, tr("Automatic (audio interface preferred)", "Automatisch (Audio-Interface bevorzugt)")).changed();
                    for d in self.devices.1.clone() {
                        changed |= ui.selectable_value(&mut self.cfg.in_dev, Some(d.clone()), d).changed();
                    }
                });
                ui.end_row();
            });
            ui.add_space(6.0);
            changed |= ui
                .checkbox(&mut self.cfg.low_latency, tr("Fastest mode (no audible delay while monitoring)", "Schnellster Modus (keine hörbare Verzögerung beim Mithören)"))
                .on_hover_text(tr("The app gets the audio device all to itself.\nDownside: other programs (browser, YouTube) cannot play sound\nthrough this device in the meantime. Switch it off here if needed.", "Die App bekommt das Audiogerät für sich allein.\nNachteil: Andere Programme (Browser, YouTube) können währenddessen\nkeinen Ton über dieses Gerät abspielen. Dann hier ausschalten."))
                .changed();
            ui.add_enabled_ui(self.cfg.low_latency, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Buffer:", "Puffer:"));
                    for (ms, label) in [(2.0, tr("Very fast", "Sehr schnell")), (3.0, tr("Fast", "Schnell")), (5.0, tr("Balanced", "Ausgewogen")), (10.0, tr("Safe (if it crackles)", "Sicher (bei Knacksern)"))] {
                        changed |= ui.selectable_value(&mut self.cfg.period_ms, ms, label).changed();
                    }
                });
            });
            ui.add_space(6.0);
            let mut auto = self.cfg.latency_comp_ms.is_none();
            if ui.checkbox(&mut auto, tr("Compensate the recording offset automatically", "Aufnahme-Versatz automatisch ausgleichen")).on_hover_text(tr("Shifts recordings by the latency of the audio device so that they sit exactly in time.", "Verschiebt Aufnahmen um die Verzögerung des Audiogeräts, damit sie genau im Takt liegen.")).changed() {
                self.cfg.latency_comp_ms = if auto { None } else { Some(st.latency_ms) };
                self.cfg.save();
            }
            if let Some(ms) = self.cfg.latency_comp_ms.as_mut() {
                if ui.add(egui::Slider::new(ms, 0.0..=120.0).suffix(" ms").text(tr("Offset", "Versatz"))).changed() {
                    self.cfg.save();
                }
            }
            ui.separator();
            if let Some(e) = &st.error {
                ui.colored_label(pal().red, trf!("Error: {}", "Fehler: {}", e));
            } else if st.running {
                ui.label(trf!(
                    "Running: {} · {} Hz · buffer {} samples · latency approx. {:.0} ms",
                    "Läuft: {} · {} Hz · Puffer {} Samples · Verzögerung ca. {:.0} ms",
                    st.mode.label(),
                    st.sr,
                    st.period_frames,
                    st.latency_ms
                ));
            }
            for n in &st.notes {
                ui.label(RichText::new(n).small().color(pal().amber));
            }
            ui.label(RichText::new(tr("Tip: if your audio interface has a “Direct Monitor” switch, turn it off – otherwise you hear yourself twice.", "Tipp: Hat dein Audio-Interface einen Schalter »Direct Monitor«, schalte ihn aus – sonst hörst du dich doppelt.")).small().weak());
            if changed {
                self.cfg.save();
                self.start_audio();
            }
        });
        self.show_settings = open;

        let mut open = self.show_help;
        egui::Window::new(tr("❓ How it works", "❓ So funktioniert's")).id(egui::Id::new("help")).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                let h = |ui: &mut egui::Ui, t: &str| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(t).font(semibold(16.0)));
                };
                h(ui, tr("1. Getting ready", "1. Vorbereiten"));
                ui.label(tr("• Plug headphones and microphone/guitar into your audio interface and connect a MIDI keyboard via USB if you have one.\n• The status bar at the bottom shows whether everything was detected (green dot).\n• Switch off “Direct Monitor” on the interface (if it has one) – the app lets you hear yourself with effects.", "• Kopfhörer und Mikrofon/Gitarre ans Audio-Interface anschließen, ein MIDI-Keyboard (falls vorhanden) per USB anstecken.\n• Unten in der Statuszeile siehst du, ob alles erkannt wurde (grüner Punkt).\n• Am Interface »Direct Monitor« ausschalten (falls vorhanden) – die App lässt dich selbst mit Effekten hören."));
                h(ui, tr("2. Recording", "2. Aufnehmen"));
                ui.label(tr("• On the left, click the track you want to record on (🎤 voice, 🎸 guitar, 🎹 keyboard, 🥁 drums).\n• Press ⏺ Record. It starts after the count-in (1-2-3-4).\n• ⏹ Stop or the space bar ends the recording.\n• Not happy with it? Use “↩ Undo”, or click the recording and press Del.", "• Links die Spur anklicken, auf der du aufnehmen willst (🎤 Stimme, 🎸 Gitarre, 🎹 Keyboard, 🥁 Schlagzeug).\n• ⏺ Aufnehmen drücken. Nach dem Einzähler (1-2-3-4) geht es los.\n• ⏹ Stopp oder die Leertaste beendet die Aufnahme.\n• Nicht gut geworden? »↩ Rückgängig« oder Aufnahme anklicken und Entf drücken."));
                h(ui, tr("3. Keyboard and drum pads", "3. Keyboard und Drum-Pads"));
                ui.label(tr("• The keys always play the selected keyboard track.\n• Drum pads (MIDI channel 10) always play the drums – there is nothing to switch.\n• No MIDI keyboard? The computer keys A S D F G H J K (and W E T Y U) play too.", "• Die Tasten spielen immer die ausgewählte Keyboard-Spur.\n• Drum-Pads (MIDI-Kanal 10) spielen immer das Schlagzeug – du musst nichts umschalten.\n• Ohne MIDI-Keyboard: Die Computertasten A S D F G H J K (und W E T Z U) spielen auch."));
                h(ui, tr("4. Sound & effects", "4. Klang & Effekte"));
                ui.label(tr("• Under “Sound & effects” at the bottom, pick a sound preset or switch the cards on and off one by one.\n• Autotune: slider towards “natural” = inaudible correction, towards “robot” = the famous effect.\n• Reverb: “Ambience” is very subtle, “Hall” is big.", "• Unten bei »Klang & Effekte« eine Klang-Vorlage wählen oder die Karten einzeln an- und ausschalten.\n• Autotune: Schieberegler Richtung »natürlich« = unhörbare Korrektur, Richtung »Roboter« = der bekannte Effekt.\n• Hall: »Ambience« ist ganz dezent, »Halle« ist groß."));
                h(ui, tr("5. Loop mode", "5. Loop-Modus"));
                ui.label(tr("• Switch on “🔁 Loop” at the top: the highlighted region repeats endlessly.\n• Recording in a loop: every pass is layered on top – first the kick, then the snare, then vocals …", "• »🔁 Loop« oben einschalten: Der gelbe Bereich wiederholt sich endlos.\n• Im Loop aufnehmen: Jede Runde wird dazugelegt – erst Bassdrum, dann Snare, dann Gesang …"));
                h(ui, tr("6. Loop pads", "6. Loop-Pads"));
                ui.label(tr("• “Loop pads” at the bottom: 8 slots for short loops that run in time.\n• Click an empty pad = record the selected track. Filled pad = start/stop.\n• “Load preset beats” gives you an instant drum backing.", "• Unten »Loop-Pads«: 8 Felder für kurze Schleifen, die im Takt laufen.\n• Leeres Pad anklicken = die ausgewählte Spur aufnehmen. Volles Pad = Start/Stopp.\n• »Fertige Beats laden« gibt dir sofort Schlagzeug-Begleitung."));
                h(ui, tr("7. Done?", "7. Fertig?"));
                ui.label(tr("• “📁 File → Save” stores the project.\n• “Export as audio file” creates a WAV file you can share.", "• »📁 Datei → Speichern« sichert das Projekt.\n• »Als Audiodatei exportieren« macht eine WAV-Datei zum Verschicken."));
            });
        });
        if self.show_help && !open && !self.cfg.help_shown {
            self.cfg.help_shown = true;
            self.cfg.save();
        }
        self.show_help = open;

        if let Some((msg, at)) = &self.toast {
            if at.elapsed().as_secs_f32() < 5.0 {
                egui::Area::new(egui::Id::new("toast")).anchor(Align2::CENTER_BOTTOM, [0.0, -60.0]).show(ctx, |ui| {
                    egui::Frame::none().fill(pal().accent).rounding(22.0).inner_margin(egui::Margin::symmetric(18.0, 11.0)).show(
                        ui,
                        |ui| {
                            ui.label(RichText::new(msg).size(15.0).color(Color32::WHITE));
                        },
                    );
                });
            } else {
                self.toast = None;
            }
        }
    }
}
