//! "Sound & effects" tab: input/instrument choice, presets and effect cards.

use crate::dsp::autotune::{key_name, note_name, SCALE_NAMES};
use crate::dsp::drums::KIT_NAMES;
use crate::dsp::fx::{PRESETS, REVERB_NAMES};
use crate::dsp::synth::PATCHES;
use crate::engine::{input_ch_for, TrackKind, TrackParams, TrackSnap, INPUT_MIX_1_2, MAX_INPUTS};
use crate::i18n::tr;
use crate::trf;
use crate::ui::theme::*;
use crate::ui::widgets::{card, meter_h, sl};
use crate::ui::App;
use eframe::egui::{self, Align, Layout, RichText};

/// Name of an input choice as stored in `TrackParams::input_ch`.
fn input_label(input_ch: u8) -> String {
    match input_ch {
        INPUT_MIX_1_2 => tr("Input 1 + 2", "Eingang 1 + 2").to_string(),
        0 | 1 => trf!("Input {}", "Eingang {}", input_ch + 1),
        n => trf!("Input {}", "Eingang {}", n),
    }
}

impl App {
    pub(super) fn sound_tab(&mut self, ui: &mut egui::Ui, t: &TrackSnap, p: &mut TrackParams) {
        // inputs of the current audio device (two while no device is open)
        let inputs = (self.status().in_channels as usize).clamp(2, MAX_INPUTS);
        egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let audio = p.kind == TrackKind::Audio;
                match p.kind {
                    TrackKind::Audio => {
                        card(ui, tr("Input", "Eingang"), tr("Where does the sound come from?", "Woher kommt der Ton?"), None, 200.0, |ui| {
                            egui::ComboBox::from_id_salt("in_ch").width(180.0).selected_text(input_label(p.input_ch)).show_ui(ui, |ui| {
                                for input in 0..inputs {
                                    let ch = input_ch_for(input);
                                    ui.selectable_value(&mut p.input_ch, ch, input_label(ch));
                                }
                                ui.selectable_value(&mut p.input_ch, INPUT_MIX_1_2, input_label(INPUT_MIX_1_2));
                            });
                            ui.add_space(4.0);
                            ui.label(RichText::new(tr("Level (volume at the input)", "Pegel (Lautstärke am Eingang)")).small());
                            meter_h(ui, t.in_meter, 180.0);
                            let (txt, c) = if t.in_meter > 0.9 {
                                (
                                    tr(
                                        "Too loud! Turn down the gain knob on the interface",
                                        "Zu laut! Gain-Regler am Interface zurückdrehen",
                                    ),
                                    pal().red,
                                )
                            } else if t.in_meter > 0.05 {
                                (tr("Level is good", "Pegel ist gut"), pal().green)
                            } else {
                                (tr("Sing or play something to test", "Sing oder spiel etwas zum Testen"), pal().text_dim)
                            };
                            ui.label(RichText::new(txt).small().color(c));
                            ui.add_space(2.0);
                            ui.checkbox(&mut p.monitor, tr("🎧 Hear myself", "🎧 Mich selbst hören")).on_hover_text(tr(
                                "You hear yourself with all effects in your headphones",
                                "Du hörst dich mit allen Effekten im Kopfhörer",
                            ));
                        })
                    }
                    TrackKind::Instrument => card(ui, "Instrument", tr("Which sound?", "Welcher Klang?"), None, 200.0, |ui| {
                        egui::ScrollArea::vertical().id_salt("inst").max_height(170.0).show(ui, |ui| {
                            for (i, pa) in PATCHES.iter().enumerate() {
                                ui.selectable_value(&mut p.instrument, i as u8, pa.name.get());
                            }
                        });
                    }),
                    TrackKind::Drums => {
                        card(ui, tr("Drums", "Schlagzeug"), tr("Which drum kit?", "Welches Drum-Kit?"), None, 200.0, |ui| {
                            for (i, k) in KIT_NAMES.iter().enumerate() {
                                ui.selectable_value(&mut p.kit, i as u8, k.get());
                            }
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(tr(
                                    "You can find preset beats under “Edit notes” and in the “Loop pads”.",
                                    "Fertige Beats findest du unter »Noten bearbeiten« und bei den »Loop-Pads«.",
                                ))
                                .small()
                                .weak(),
                            );
                        })
                    }
                }
                card(ui, tr("Sound preset", "Klang-Vorlage"), tr("Ready-made settings", "Fertige Einstellungen"), None, 215.0, |ui| {
                    egui::ScrollArea::vertical().id_salt("presets").max_height(170.0).show(ui, |ui| {
                        for pr in PRESETS.iter().filter(|pr| audio || !pr.audio) {
                            if ui.button(pr.name.get()).clicked() {
                                p.fx = (pr.make)();
                            }
                        }
                    });
                });
                let fx = &mut p.fx;
                if audio {
                    let at = &mut fx.autotune;
                    card(ui, "Autotune", tr("Pulls off-key notes into tune", "Zieht schiefe Töne gerade"), Some(&mut at.on), 235.0, |ui| {
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("at_key").width(52.0).selected_text(key_name(at.key)).show_ui(ui, |ui| {
                                for i in 0..12u8 {
                                    ui.selectable_value(&mut at.key, i, key_name(i));
                                }
                            });
                            egui::ComboBox::from_id_salt("at_scale")
                                .width(150.0)
                                .selected_text(SCALE_NAMES[(at.scale as usize).min(4)].get())
                                .show_ui(ui, |ui| {
                                    for (i, k) in SCALE_NAMES.iter().enumerate() {
                                        ui.selectable_value(&mut at.scale, i as u8, k.get());
                                    }
                                });
                        })
                        .response
                        .on_hover_text(tr(
                            "Key of the song. If you don't know it: leave it on “All notes”.",
                            "Tonart des Songs. Wenn du sie nicht kennst: »Alle Töne« lassen.",
                        ));
                        ui.add(egui::Slider::new(&mut at.natural, 0.0..=1.0).show_value(false));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr("🤖 Robot", "🤖 Roboter")).small());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(tr("natural 🙂", "natürlich 🙂")).small())
                            });
                        });
                        sl(ui, tr("Correction strength", "Stärke der Korrektur"), &mut at.amount, 0.0..=1.0);
                        if t.tune_midi > 1.0 {
                            ui.label(
                                RichText::new(trf!(
                                    "You sing: {}   correction {:+.0} cents",
                                    "Du singst: {}   Korrektur {:+.0} Cent",
                                    note_name(t.tune_midi + t.tune_cents / 100.0),
                                    t.tune_cents
                                ))
                                .small()
                                .color(pal().green),
                            );
                        } else {
                            ui.label(RichText::new(tr("(waiting for a note…)", "(wartet auf einen Ton…)")).small().weak());
                        }
                    });
                }
                card(ui, tr("Reverb", "Hall"), tr("Adds a sense of space", "Raumklang um den Ton"), Some(&mut fx.reverb_on), 200.0, |ui| {
                    egui::ComboBox::from_id_salt("rev")
                        .width(180.0)
                        .selected_text(REVERB_NAMES[(fx.reverb_kind as usize).min(3)].get())
                        .show_ui(ui, |ui| {
                            for (i, k) in REVERB_NAMES.iter().enumerate() {
                                ui.selectable_value(&mut fx.reverb_kind, i as u8, k.get());
                            }
                        });
                    sl(ui, tr("How much reverb", "Wie viel Hall"), &mut fx.reverb_mix, 0.0..=1.0);
                    sl(ui, tr("How long it rings", "Wie lang klingt er nach"), &mut fx.reverb_size, 0.0..=1.0);
                });
                card(ui, "Echo", tr("Repeats the sound", "Wiederholt den Ton"), Some(&mut fx.delay_on), 190.0, |ui| {
                    sl(ui, tr("How loud", "Wie laut"), &mut fx.delay_mix, 0.0..=0.8);
                    sl(ui, tr("Spacing (short ↔ long)", "Abstand (kurz ↔ lang)"), &mut fx.delay_time, 60.0..=900.0);
                    sl(ui, tr("How often", "Wie oft"), &mut fx.delay_fb, 0.0..=0.8);
                });
                card(
                    ui,
                    tr("Equalizer", "Klangregler"),
                    tr("Bass, mids, treble", "Bässe, Mitten, Höhen"),
                    Some(&mut fx.eq_on),
                    190.0,
                    |ui| {
                        sl(ui, tr("Bass", "Bässe"), &mut fx.bass, -12.0..=12.0);
                        sl(ui, tr("Mids", "Mitten"), &mut fx.mid, -12.0..=12.0);
                        sl(ui, tr("Treble", "Höhen"), &mut fx.treble, -12.0..=12.0);
                        sl(ui, tr("Filter out rumble", "Rumpeln wegfiltern"), &mut fx.lowcut, 20.0..=250.0);
                    },
                );
                card(
                    ui,
                    tr("Compressor", "Kompressor"),
                    tr("Evens out loud and quiet", "Gleicht laut und leise aus"),
                    Some(&mut fx.comp_on),
                    180.0,
                    |ui| {
                        sl(ui, tr("Amount", "Stärke"), &mut fx.comp, 0.0..=1.0);
                        ui.label(
                            RichText::new(tr("Makes vocals more even and present.", "Macht Gesang gleichmäßiger und präsenter."))
                                .small()
                                .weak(),
                        );
                    },
                );
                card(
                    ui,
                    tr("Overdrive", "Verzerrer"),
                    tr("For electric guitar: crunch to rock", "Für E-Gitarre: Crunch bis Rock"),
                    Some(&mut fx.drive_on),
                    180.0,
                    |ui| {
                        sl(ui, tr("Amount", "Stärke"), &mut fx.drive, 0.0..=1.0);
                    },
                );
                card(
                    ui,
                    "Chorus",
                    tr("Makes the sound wide and soft", "Macht den Klang breit und weich"),
                    Some(&mut fx.chorus_on),
                    180.0,
                    |ui| {
                        sl(ui, tr("Amount", "Stärke"), &mut fx.chorus, 0.0..=1.0);
                    },
                );
                card(ui, "Position", tr("Left or right in the headphones", "Links oder rechts im Kopfhörer"), None, 160.0, |ui| {
                    ui.add(egui::Slider::new(&mut p.pan, -1.0..=1.0).show_value(false));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("left", "links")).small());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new(tr("right", "rechts")).small()));
                    });
                    if ui.small_button(tr("Center", "Mitte")).clicked() {
                        p.pan = 0.0;
                    }
                });
            });
        });
    }
}
