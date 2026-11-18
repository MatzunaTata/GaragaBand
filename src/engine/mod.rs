//! Audio engine: tracks, transport, recording, loop mode, loop pads, mixing.
//!
//! The engine lives behind a mutex shared by the audio thread (which calls
//! [`Engine::process`]) and the UI (which reads a [`Snapshot`] every frame).
//!
//! - `track`      track parameters, notes, audio clips
//! - `pads`       the eight loop pads
//! - `midi`       routing of incoming MIDI events
//! - `mixer`      the real-time render loop
//! - `recording`  turning raw takes into clips
//! - `snapshot`   read-only copies of the state for the UI

mod midi;
mod mixer;
mod pads;
mod recording;
mod snapshot;
#[cfg(test)]
mod tests;
mod track;

pub use midi::MidiEvent;
pub use pads::{Pad, PadState};
pub use recording::{stop_and_finish, RawRec};
pub use snapshot::{ClipSnap, PadSnap, Snapshot, TrackSnap};
pub use track::{make_clip, AudioClip, Note, Track, TrackKind, TrackParams, TrackPreset, PEAK_STEP};

use crate::dsp::{drums::DrumKit, fx::FxChain, synth::Synth};
use crossbeam_channel::Receiver;
use std::sync::Arc;

/// Largest number of frames processed in one internal block.
pub const MAX_BLOCK: usize = 64;
pub const NUM_PADS: usize = 8;

/// Playback state. Positions are in sample frames.
#[derive(Clone, Debug)]
pub struct Transport {
    pub playing: bool,
    pub recording: bool,
    pub pos: i64,
    pub bpm: f32,
    pub metronome: bool,
    pub count_in: bool,
    pub precount: i64,
    pub precount_total: i64,
    pub loop_on: bool,
    pub loop_start_bar: u32,
    pub loop_bars: u32,
    pub rec_start: i64,
}

/// Per-track clips and notes as stored for one undo step.
type UndoState = Vec<(u64, Vec<AudioClip>, Arc<Vec<Note>>)>;

pub struct Engine {
    pub sr: f32,
    pub tracks: Vec<Track>,
    pub tr: Transport,
    pub selected: u64,
    pub master_vol: f32,
    pub meter: [f32; 2],
    pub pads: Vec<Pad>,
    pub pad_clock: i64,
    pub pad_bars: u32,
    pub pads_from_midi: bool,
    pub latency_comp: i64,
    pub midi_activity: u32,
    midi_rx: Receiver<MidiEvent>,
    next_id: u64,
    click_env: f32,
    click_ph: f32,
    click_f: f32,
    lim: f32,
    undo: Vec<UndoState>,
}

impl Engine {
    pub fn new(sr: f32, midi_rx: Receiver<MidiEvent>) -> Self {
        Self {
            sr,
            tracks: vec![],
            tr: Transport {
                playing: false,
                recording: false,
                pos: 0,
                bpm: 100.0,
                metronome: true,
                count_in: true,
                precount: 0,
                precount_total: 0,
                loop_on: false,
                loop_start_bar: 0,
                loop_bars: 4,
                rec_start: 0,
            },
            selected: 0,
            master_vol: 0.8,
            meter: [0.0; 2],
            pads: (0..NUM_PADS).map(|_| Pad::new()).collect(),
            pad_clock: 0,
            pad_bars: 2,
            pads_from_midi: false,
            latency_comp: 0,
            midi_activity: 0,
            midi_rx,
            next_id: 1,
            click_env: 0.0,
            click_ph: 0.0,
            click_f: 1000.0,
            lim: 1.0,
            undo: vec![],
        }
    }

    pub fn beat_len(&self) -> f64 {
        self.sr as f64 * 60.0 / self.tr.bpm.clamp(30.0, 300.0) as f64
    }
    pub fn bar_len(&self) -> i64 {
        ((self.beat_len() * 4.0).round() as i64).max(64)
    }
    pub fn loop_range(&self) -> (i64, i64) {
        let b = self.bar_len();
        let s = self.tr.loop_start_bar as i64 * b;
        (s, s + self.tr.loop_bars.max(1) as i64 * b)
    }
    pub fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        if (sr - self.sr).abs() < 1.0 {
            return;
        }
        self.sr = sr;
        for t in self.tracks.iter_mut() {
            t.synth = Synth::new(sr);
            t.drums = DrumKit::new(sr);
            t.fx = FxChain::new(sr);
        }
    }

    pub fn add_track(&mut self, params: TrackParams) -> u64 {
        let id = self.new_id();
        self.tracks.push(Track::new(id, params, self.sr));
        self.selected = id;
        id
    }

    pub fn remove_track(&mut self, id: u64) {
        self.tracks.retain(|t| t.id != id);
        if self.selected == id {
            self.selected = self.tracks.first().map(|t| t.id).unwrap_or(0);
        }
    }

    pub fn track_mut(&mut self, id: u64) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn push_undo(&mut self) {
        let st = self.tracks.iter().map(|t| (t.id, t.clips.clone(), t.notes.clone())).collect();
        self.undo.push(st);
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self) {
        if self.tr.recording {
            return;
        }
        if let Some(st) = self.undo.pop() {
            for (id, clips, notes) in st {
                if let Some(t) = self.track_mut(id) {
                    t.clips = clips;
                    t.notes = notes;
                    t.silence();
                }
            }
        }
    }

    pub fn song_end(&self) -> i64 {
        let bl = self.beat_len();
        let mut end = 0i64;
        for t in &self.tracks {
            for c in &t.clips {
                end = end.max(c.start + c.data.len() as i64);
            }
            for n in t.notes.iter() {
                end = end.max(((n.start + n.len) * bl) as i64);
            }
        }
        end
    }

    pub fn play(&mut self) {
        if !self.tr.playing {
            self.tr.playing = true;
            self.tr.precount = 0;
        }
    }

    pub fn record(&mut self) {
        if self.tr.recording {
            return;
        }
        self.push_undo();
        let sel = self.selected;
        let cap = (self.sr as usize) * 600;
        for t in self.tracks.iter_mut() {
            let audio = t.params.kind == TrackKind::Audio;
            t.rec_active = !audio || t.id == sel || t.params.armed;
            t.rec_audio = Vec::with_capacity(if audio && t.rec_active { cap } else { 0 });
            t.rec_segs.clear();
            t.rec_segs.push(0);
            t.rec_notes.clear();
            t.held = [None; 128];
        }
        if self.tr.loop_on {
            let (ls, le) = self.loop_range();
            if self.tr.pos < ls || self.tr.pos >= le {
                self.tr.pos = ls;
            }
        }
        self.tr.rec_start = self.tr.pos;
        self.tr.recording = true;
        if !self.tr.playing {
            self.tr.playing = true;
            if self.tr.count_in {
                self.tr.precount = self.bar_len();
                self.tr.precount_total = self.tr.precount;
            }
        }
    }

    /// Stops playback/recording. Returns the raw takes if anything was recorded.
    pub fn stop(&mut self) -> Vec<RawRec> {
        let mut raw = vec![];
        let bl = self.beat_len();
        let now = self.tr.pos as f64 / bl;
        let (ls, _) = self.loop_range();
        if self.tr.recording {
            for t in self.tracks.iter_mut() {
                if !t.rec_active {
                    continue;
                }
                for p in 0..128 {
                    if let Some((s, v)) = t.held[p].take() {
                        t.rec_notes.push(Note { start: s, len: (now - s).max(0.1), pitch: p as u8, vel: v });
                    }
                }
                if !t.rec_notes.is_empty() {
                    let notes = Arc::make_mut(&mut t.notes);
                    notes.append(&mut t.rec_notes);
                }
                if !t.rec_audio.is_empty() {
                    raw.push(RawRec {
                        track: t.id,
                        data: std::mem::take(&mut t.rec_audio),
                        segs: t.rec_segs.clone(),
                        rec_start: self.tr.rec_start,
                        loop_start: ls,
                        comp: self.latency_comp,
                    });
                }
                t.rec_active = false;
            }
            if raw.is_empty()
                && self.undo.last().is_some_and(|u| {
                    u.iter().all(|(id, _, n)| self.tracks.iter().find(|t| t.id == *id).is_none_or(|t| Arc::ptr_eq(&t.notes, n)))
                })
            {
                // nothing was recorded -> no undo step needed
                self.undo.pop();
            }
        }
        for t in self.tracks.iter_mut() {
            t.silence();
            t.synth.all_off();
            t.drums.all_off();
        }
        for p in self.pads.iter_mut() {
            match p.state {
                PadState::Playing | PadState::QueuedPlay => p.state = PadState::Stopped,
                PadState::QueuedRec | PadState::Recording => {
                    p.state = PadState::Empty;
                    p.buf = vec![];
                }
                _ => {}
            }
        }
        self.tr.playing = false;
        self.tr.recording = false;
        self.tr.precount = 0;
        raw
    }

    pub fn seek(&mut self, pos: i64) {
        if self.tr.recording {
            return;
        }
        self.tr.pos = pos.max(0);
        for t in self.tracks.iter_mut() {
            t.silence();
        }
    }
}
