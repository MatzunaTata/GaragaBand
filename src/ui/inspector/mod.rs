//! Bottom panel with three tabs: sound & effects, loop pads, note editor.

mod notes;
mod pads;
mod sound;

use super::theme::*;
use super::widgets::pill_button;
use super::{App, Tab};
use crate::engine::{Snapshot, TrackParams};
use crate::i18n::tr;
use crate::trf;
use eframe::egui::{self, vec2, RichText};

impl App {
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui, snap: &Snapshot, edit: &mut [TrackParams]) {
        let idx = snap.tracks.iter().position(|t| t.id == snap.selected);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let tab = |ui: &mut egui::Ui, cur: &mut Tab, t: Tab, label: &str| {
                let sel = *cur == t;
                let b = pill_button(label, if sel { ACCENT } else { WIDGET }, 32.0).min_size(vec2(160.0, 32.0));
                if ui.add(b).clicked() {
                    *cur = t;
                }
            };
            tab(ui, &mut self.tab, Tab::Sound, tr("🎚 Sound & effects", "🎚 Klang & Effekte"));
            tab(ui, &mut self.tab, Tab::Pads, tr("🔁 Loop pads", "🔁 Loop-Pads"));
            tab(ui, &mut self.tab, Tab::Notes, tr("🎼 Edit notes", "🎼 Noten bearbeiten"));
            if let Some(i) = idx {
                ui.separator();
                ui.label(
                    RichText::new(trf!("Selected track:  {} {}", "Ausgewählte Spur:  {} {}", track_icon(edit[i].icon), edit[i].name))
                        .color(TEXT_DIM),
                );
            }
        });
        ui.add_space(4.0);
        match (self.tab, idx) {
            (Tab::Pads, _) => self.pads_tab(ui, snap),
            (_, None) => {
                ui.label(tr(
                    "No track selected. Create one above with “New track”.",
                    "Keine Spur ausgewählt. Lege oben mit »Neue Spur« eine an.",
                ));
            }
            (Tab::Sound, Some(i)) => self.sound_tab(ui, &snap.tracks[i], &mut edit[i]),
            (Tab::Notes, Some(i)) => self.notes_tab(ui, snap, &snap.tracks[i]),
        }
    }
}
