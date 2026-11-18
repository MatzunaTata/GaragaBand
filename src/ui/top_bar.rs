//! Top bar: transport buttons, position, file menu, tempo and loop settings.

use super::theme::*;
use super::widgets::{bars_label, paint_meter, pill_button};
use super::App;
use crate::audio::list_devices;
use crate::engine::{stop_and_finish, Snapshot};
use crate::i18n::{self, tr, Lang};
use crate::trf;
use eframe::egui::{self, vec2, Align, Color32, Layout, Rect, RichText, Sense};

impl App {
    /// Button that switches between the dark and the light appearance.
    fn theme_toggle(&mut self, ui: &mut egui::Ui) {
        let dark = mode() == ThemeMode::Dark;
        let (icon, hint) = if dark {
            ("☀", tr("Switch to the light theme", "Zum hellen Design wechseln"))
        } else {
            ("🌙", tr("Switch to the dark theme", "Zum dunklen Design wechseln"))
        };
        if ui.button(icon).on_hover_text(hint).clicked() {
            let next = if dark { ThemeMode::Light } else { ThemeMode::Dark };
            set_mode(ui.ctx(), next);
            self.cfg.theme = next;
            self.cfg.save();
        }
    }

    /// Language drop-down; the choice is saved and applies immediately.
    fn language_picker(&mut self, ui: &mut egui::Ui) {
        let mut lang = i18n::lang();
        egui::ComboBox::from_id_salt("lang")
            .width(104.0)
            .selected_text(format!("🌐 {}", lang.label()))
            .show_ui(ui, |ui| {
                for l in Lang::ALL {
                    ui.selectable_value(&mut lang, l, l.label());
                }
            })
            .response
            .on_hover_text("Language / Sprache");
        if lang != i18n::lang() {
            i18n::set_lang(lang);
            self.cfg.lang = lang;
            self.cfg.save();
        }
    }

    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        let tp = &snap.tr;
        let beat_len = snap.sr as f64 * 60.0 / tp.bpm as f64;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().button_padding = vec2(16.0, 5.0);
            let big = |t: &str, fill: Color32| pill_button(t, fill, 40.0);
            if ui.add(big("⏮", pal().widget)).on_hover_text(tr("Back to the start", "Zurück zum Anfang")).clicked() {
                let mut e = self.engine.lock();
                let p = if e.tr.loop_on { e.loop_range().0 } else { 0 };
                e.seek(p);
                self.scroll = 0.0;
            }
            if tp.playing {
                if ui
                    .add(big(tr("⏹  Stop", "⏹  Stopp"), pal().widget_active))
                    .on_hover_text(tr("Stop (space bar)", "Anhalten (Leertaste)"))
                    .clicked()
                {
                    stop_and_finish(&self.engine);
                }
            } else if ui
                .add(big(tr("▶  Play", "▶  Abspielen"), pal().green))
                .on_hover_text(tr("Play the song (space bar)", "Song abspielen (Leertaste)"))
                .clicked()
            {
                self.engine.lock().play();
            }
            if tp.recording {
                let blink = (ui.input(|i| i.time) * 2.0).sin() > 0.0;
                let c = if blink { pal().red } else { mix(pal().red, pal().bg, 0.4) };
                let label = if tp.precount > 0 {
                    trf!("⏺  Count-in… {}", "⏺  Einzähler… {}", (tp.precount as f64 / beat_len).ceil() as i32)
                } else {
                    tr("⏺  Recording – click to stop", "⏺  Aufnahme läuft – klicken zum Beenden").to_string()
                };
                if ui.add(big(&label, c)).clicked() {
                    stop_and_finish(&self.engine);
                }
            } else if ui
                .add(big(tr("⏺  Record", "⏺  Aufnehmen"), pal().red))
                .on_hover_text(tr(
                    "Records on the selected track (R key).\nKeyboard and drum pads are always recorded as well.",
                    "Nimmt auf der ausgewählten Spur auf (Taste R).\nKeyboard und Drum-Pads werden immer mit aufgenommen.",
                ))
                .clicked()
            {
                self.engine.lock().record();
            }
            ui.separator();
            let beats = tp.pos as f64 / beat_len;
            let secs = tp.pos as f64 / snap.sr as f64;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(
                    RichText::new(trf!("Bar {} · {}", "Takt {} · {}", (beats / 4.0) as i64 + 1, (beats as i64) % 4 + 1))
                        .monospace()
                        .size(17.0)
                        .strong(),
                );
                ui.label(RichText::new(format!("{}:{:04.1}", (secs / 60.0) as i64, secs % 60.0)).monospace().weak());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().button_padding = vec2(10.0, 5.0);
                self.language_picker(ui);
                self.theme_toggle(ui);
                if ui.button(tr("❓ Help", "❓ Hilfe")).clicked() {
                    self.show_help = true;
                }
                if ui.button("⚙ Audio").on_hover_text(tr("Audio device settings", "Audiogeräte einstellen")).clicked() {
                    self.devices = list_devices();
                    self.show_settings = true;
                }
                ui.menu_button(tr("📁 File", "📁 Datei"), |ui| {
                    if ui.button(tr("New song", "Neuer Song")).clicked() {
                        self.new_project();
                        ui.close_menu();
                    }
                    if ui.button(tr("Open…", "Öffnen…")).clicked() {
                        ui.close_menu();
                        self.open();
                    }
                    if ui.button(tr("Save   (Ctrl+S)", "Speichern   (Strg+S)")).clicked() {
                        ui.close_menu();
                        self.save(false);
                    }
                    if ui.button(tr("Save as…", "Speichern unter…")).clicked() {
                        ui.close_menu();
                        self.save(true);
                    }
                    ui.separator();
                    if ui.button(tr("Export as audio file (WAV)…", "Als Audiodatei (WAV) exportieren…")).clicked() {
                        ui.close_menu();
                        self.export();
                    }
                });
                if ui
                    .add_enabled(snap.can_undo && !tp.recording, egui::Button::new(tr("↩ Undo", "↩ Rückgängig")))
                    .on_hover_text(tr("Undo the last recording/change (Ctrl+Z)", "Letzte Aufnahme/Änderung zurücknehmen (Strg+Z)"))
                    .clicked()
                {
                    self.engine.lock().undo();
                }
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!tp.recording, |ui| {
                let mut bpm = tp.bpm;
                ui.label("Tempo");
                if ui
                    .add(egui::DragValue::new(&mut bpm).range(40.0..=240.0).speed(0.5).suffix(" BPM"))
                    .on_hover_text(tr("Speed of the song (beats per minute).\nBest set BEFORE the first recording.", "Geschwindigkeit des Songs (Schläge pro Minute).\nAm besten VOR der ersten Aufnahme einstellen."))
                    .changed()
                {
                    self.engine.lock().tr.bpm = bpm.round();
                }
                ui.separator();
                let mut lp = tp.loop_on;
                if ui
                    .toggle_value(&mut lp, RichText::new("🔁 Loop").size(16.0))
                    .on_hover_text(tr("Loop mode: a section repeats endlessly.\nWhile recording you can add something on every pass.", "Loop-Modus: ein Abschnitt wiederholt sich endlos.\nBei der Aufnahme kannst du so Runde für Runde etwas dazuspielen."))
                    .changed()
                {
                    let mut e = self.engine.lock();
                    e.tr.loop_on = lp;
                    if lp {
                        let bar = e.bar_len();
                        e.tr.loop_start_bar = (e.tr.pos / bar) as u32;
                    }
                }
                if tp.loop_on {
                    let mut start = tp.loop_start_bar + 1;
                    ui.label(tr("from bar", "ab Takt"));
                    if ui.add(egui::DragValue::new(&mut start).range(1..=999).speed(0.1)).changed() {
                        self.engine.lock().tr.loop_start_bar = start - 1;
                    }
                    let mut bars = tp.loop_bars;
                    egui::ComboBox::from_id_salt("loop_bars")
                        .width(80.0)
                        .selected_text(bars_label(bars))
                        .show_ui(ui, |ui| {
                            for b in [1u32, 2, 4, 8, 16] {
                                ui.selectable_value(&mut bars, b, bars_label(b));
                            }
                        });
                    if bars != tp.loop_bars {
                        self.engine.lock().tr.loop_bars = bars;
                    }
                }
            });
            ui.separator();
            let (mut met, mut cnt) = (tp.metronome, tp.count_in);
            if ui.checkbox(&mut met, tr("Metronome", "Metronom")).on_hover_text(tr("A click on every beat that keeps you in time", "Klick im Takt, damit du im Tempo bleibst")).changed() {
                self.engine.lock().tr.metronome = met;
            }
            if ui.checkbox(&mut cnt, tr("Count-in", "Einzähler")).on_hover_text(tr("Counts in one bar before the recording starts (1-2-3-4)", "Zählt vor der Aufnahme einen Takt vor (1-2-3-4)")).changed() {
                self.engine.lock().tr.count_in = cnt;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (mr, _) = ui.allocate_exact_size(vec2(120.0, 24.0), Sense::hover());
                for (k, lv) in snap.meter.iter().enumerate() {
                    let r = Rect::from_min_size(mr.min + vec2(0.0, 2.0 + k as f32 * 11.0), vec2(120.0, 7.0));
                    paint_meter(ui.painter(), r, *lv, false);
                }
                let mut mv = snap.master_vol;
                ui.spacing_mut().slider_width = 100.0;
                if ui.add(egui::Slider::new(&mut mv, 0.0..=1.0).show_value(false)).on_hover_text(tr("Master volume", "Gesamtlautstärke")).changed() {
                    self.engine.lock().master_vol = mv;
                }
                ui.label("🔊");
            });
        });
        ui.add_space(6.0);
    }
}
