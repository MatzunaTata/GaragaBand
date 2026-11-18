//! Keyboard shortcuts and the computer keyboard as a piano / drum kit.

use super::App;
use crate::engine::{stop_and_finish, Snapshot, TrackKind};
use eframe::egui::{self, Key};

/// Computer keys mapped to semitone offsets from C, laid out like a piano.
const KEYMAP: [(Key, u8); 16] = [
    (Key::A, 0),
    (Key::W, 1),
    (Key::S, 2),
    (Key::E, 3),
    (Key::D, 4),
    (Key::F, 5),
    (Key::T, 6),
    (Key::G, 7),
    (Key::Z, 8),
    (Key::Y, 8),
    (Key::H, 9),
    (Key::U, 10),
    (Key::J, 11),
    (Key::K, 12),
    (Key::O, 13),
    (Key::L, 14),
];
/// Drum notes for the same keys while a drum track is selected.
const DRUM_KEYS: [u8; 15] = [36, 37, 38, 44, 42, 46, 51, 39, 56, 41, 54, 45, 48, 50, 49];

impl App {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        let typing = ctx.wants_keyboard_input();
        let (space, r, del, undo, save, ctrl) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::R),
                i.key_pressed(Key::Delete),
                i.modifiers.command && i.key_pressed(Key::Z),
                i.modifiers.command && i.key_pressed(Key::S),
                i.modifiers.command,
            )
        });
        if !typing {
            if space {
                if snap.tr.playing {
                    stop_and_finish(&self.engine);
                } else {
                    self.engine.lock().play();
                }
            }
            if r && !ctrl {
                if snap.tr.recording {
                    stop_and_finish(&self.engine);
                } else {
                    self.engine.lock().record();
                }
            }
            if del {
                if let Some((tid, cid)) = self.sel_clip.take() {
                    let mut e = self.engine.lock();
                    e.push_undo();
                    if let Some(t) = e.track_mut(tid) {
                        t.clips.retain(|c| c.id != cid);
                    }
                }
            }
            if undo {
                self.engine.lock().undo();
            }
        }
        if save {
            self.save(false);
        }
        // computer keyboard as piano / drums
        let drums = snap.tracks.iter().find(|t| t.id == snap.selected).is_some_and(|t| t.params.kind == TrackKind::Drums);
        let active = !typing && !ctrl;
        for (idx, (key, off)) in KEYMAP.iter().enumerate() {
            let down = active && ctx.input(|i| i.key_down(*key));
            let was = self.keys_down.iter().position(|k| k.0 == *key);
            if down && was.is_none() {
                let note =
                    if drums { DRUM_KEYS[idx.min(DRUM_KEYS.len() - 1)] } else { (60 + self.octave * 12 + *off as i32).clamp(0, 127) as u8 };
                self.send_midi(0x90, note, 100);
                self.keys_down.push((*key, note));
            } else if !down {
                if let Some(p) = was {
                    let n = self.keys_down.remove(p).1;
                    self.send_midi(0x80, n, 0);
                }
            }
        }
    }
}
