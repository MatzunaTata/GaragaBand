//! Routing of incoming MIDI events to tracks and loop pads.

use super::{Engine, Note, TrackKind, NUM_PADS};

#[derive(Clone, Copy)]
pub struct MidiEvent {
    pub status: u8,
    pub d1: u8,
    pub d2: u8,
}

impl Engine {
    fn midi_target(&self, drum: bool) -> Option<usize> {
        let sel = self.tracks.iter().position(|t| t.id == self.selected);
        let kind = |i: usize| self.tracks[i].params.kind;
        let first = |k: TrackKind| self.tracks.iter().position(|t| t.params.kind == k);
        if drum {
            if let Some(i) = sel {
                if kind(i) == TrackKind::Drums {
                    return Some(i);
                }
            }
            first(TrackKind::Drums).or_else(|| first(TrackKind::Instrument))
        } else {
            if let Some(i) = sel {
                if kind(i) != TrackKind::Audio {
                    return Some(i);
                }
            }
            first(TrackKind::Instrument).or_else(|| first(TrackKind::Drums))
        }
    }

    pub(super) fn handle_midi(&mut self) {
        while let Ok(ev) = self.midi_rx.try_recv() {
            self.midi_activity = self.midi_activity.wrapping_add(1);
            let ch = ev.status & 0x0F;
            let kind = ev.status & 0xF0;
            let drum = ch == 9;
            if drum && self.pads_from_midi {
                if kind == 0x90 && ev.d2 > 0 {
                    self.pad_toggle((ev.d1 as i32 - 36).rem_euclid(NUM_PADS as i32) as usize);
                }
                continue;
            }
            let Some(ti) = self.midi_target(drum) else { continue };
            let rec = self.tr.recording && self.tr.playing && self.tr.precount == 0;
            let bl = self.beat_len();
            let beat = ((self.tr.pos - self.latency_comp / 2).max(0)) as f64 / bl;
            let t = &mut self.tracks[ti];
            let n = (ev.d1 & 127) as usize;
            match kind {
                0x90 if ev.d2 > 0 => {
                    t.note_on(ev.d1, ev.d2);
                    if rec && t.rec_active {
                        t.held[n] = Some((beat, ev.d2));
                    }
                }
                0x80 | 0x90 => {
                    t.note_off(ev.d1);
                    if let Some((s, v)) = t.held[n].take() {
                        let len = if beat > s { beat - s } else { 0.25 };
                        if t.rec_notes.len() < t.rec_notes.capacity() {
                            t.rec_notes.push(Note { start: s, len: len.max(0.05), pitch: ev.d1, vel: v });
                        }
                    }
                }
                0xE0 => {
                    let v = ((ev.d2 as i32) << 7 | ev.d1 as i32) - 8192;
                    t.synth.bend = v as f32 / 8192.0 * 2.0;
                }
                0xB0 if ev.d1 == 64 => t.synth.set_sustain(ev.d2 >= 64),
                _ => {}
            }
        }
    }
}
