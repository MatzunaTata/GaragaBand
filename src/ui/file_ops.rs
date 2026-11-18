//! Project file actions: new, open, save, export.

use super::{default_tracks, App};
use crate::engine::{stop_and_finish, NUM_PADS};
use crate::i18n::tr;
use crate::{project, trf};

impl App {
    pub(super) fn new_project(&mut self) {
        stop_and_finish(&self.engine);
        let mut e = self.engine.lock();
        e.tracks.clear();
        for i in 0..NUM_PADS {
            e.pad_clear(i);
        }
        e.tr.pos = 0;
        default_tracks(&mut e);
        drop(e);
        self.path = None;
        self.sel_clip = None;
        self.scroll = 0.0;
    }

    pub(super) fn save(&mut self, save_as: bool) {
        if save_as || self.path.is_none() {
            let p = rfd::FileDialog::new()
                .set_title(tr("Save project", "Projekt speichern"))
                .add_filter(tr("GaragaBand project", "GaragaBand-Projekt"), &["gbp"])
                .set_file_name(tr("My Song.gbp", "Mein Song.gbp"))
                .save_file();
            match p {
                Some(p) => self.path = Some(p),
                None => return,
            }
        }
        let path = self.path.clone().unwrap();
        match project::save(&self.engine, &path) {
            Ok(()) => self.say(trf!("Saved: {}", "Gespeichert: {}", path.display())),
            Err(e) => self.say(trf!("Saving failed: {}", "Speichern fehlgeschlagen: {}", e)),
        }
    }

    pub(super) fn open(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .set_title(tr("Open project", "Projekt öffnen"))
            .add_filter(tr("GaragaBand project", "GaragaBand-Projekt"), &["gbp"])
            .pick_file()
        else {
            return;
        };
        stop_and_finish(&self.engine);
        match project::load(&self.engine, &p) {
            Ok(()) => {
                self.path = Some(p);
                self.sel_clip = None;
                self.scroll = 0.0;
                self.say(tr("Project loaded", "Projekt geladen"));
            }
            Err(e) => self.say(trf!("Opening failed: {}", "Öffnen fehlgeschlagen: {}", e)),
        }
    }

    pub(super) fn export(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .set_title(tr("Export song as audio file", "Song als Audiodatei exportieren"))
            .add_filter(tr("WAV audio", "WAV-Audio"), &["wav"])
            .set_file_name(tr("My Song.wav", "Mein Song.wav"))
            .save_file()
        else {
            return;
        };
        stop_and_finish(&self.engine);
        self.say(tr("Exporting… one moment", "Exportiere… einen Moment"));
        let (eng, res) = (self.engine.clone(), self.export_result.clone());
        std::thread::spawn(move || {
            let msg = match project::export_wav(&eng, &p) {
                Ok(()) => trf!("Export finished: {}", "Fertig exportiert: {}", p.display()),
                Err(e) => trf!("Export failed: {}", "Export fehlgeschlagen: {}", e),
            };
            *res.lock() = Some(msg);
        });
    }
}
