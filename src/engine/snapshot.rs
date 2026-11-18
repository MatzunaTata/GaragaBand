//! Read-only copies of the engine state for the UI and for offline rendering.

use super::{Engine, Note, PadState, Track, TrackParams, Transport};
use std::sync::Arc;

pub struct ClipSnap {
    pub id: u64,
    pub start: i64,
    pub len: usize,
    pub peaks: Arc<Vec<f32>>,
}

pub struct TrackSnap {
    pub id: u64,
    pub params: TrackParams,
    pub clips: Vec<ClipSnap>,
    pub notes: Arc<Vec<Note>>,
    pub meter: f32,
    pub in_meter: f32,
    pub rec_active: bool,
    pub tune_midi: f32,
    pub tune_cents: f32,
}

pub struct PadSnap {
    pub state: PadState,
    pub name: String,
    pub progress: f32,
    pub bars: u32,
    pub volume: f32,
    pub stale_tempo: bool,
    pub beat: Option<u8>,
}

pub struct Snapshot {
    pub sr: f32,
    pub tracks: Vec<TrackSnap>,
    pub tr: Transport,
    pub selected: u64,
    pub master_vol: f32,
    pub meter: [f32; 2],
    pub pads: Vec<PadSnap>,
    pub pad_bars: u32,
    pub pads_from_midi: bool,
    pub pad_countin: bool,
    pub midi_activity: u32,
    pub can_undo: bool,
}

impl Engine {
    pub fn snapshot(&self) -> Snapshot {
        let running = self.tr.playing && self.tr.precount == 0;
        let clock = if running { self.tr.pos } else { self.pad_clock };
        Snapshot {
            sr: self.sr,
            tracks: self
                .tracks
                .iter()
                .map(|t| TrackSnap {
                    id: t.id,
                    params: t.params.clone(),
                    clips: t
                        .clips
                        .iter()
                        .map(|c| ClipSnap { id: c.id, start: c.start, len: c.data.len(), peaks: c.peaks.clone() })
                        .collect(),
                    notes: t.notes.clone(),
                    meter: t.meter,
                    in_meter: t.in_meter,
                    rec_active: t.rec_active && self.tr.recording,
                    tune_midi: t.fx.autotune.disp_midi,
                    tune_cents: t.fx.autotune.disp_cents,
                })
                .collect(),
            tr: self.tr.clone(),
            selected: self.selected,
            master_vol: self.master_vol,
            meter: self.meter,
            pads: self
                .pads
                .iter()
                .map(|p| PadSnap {
                    state: p.state,
                    name: p.name.clone(),
                    progress: match p.state {
                        PadState::Recording => p.rec_pos as f32 / p.len.max(1) as f32,
                        PadState::Playing => (clock - p.origin).rem_euclid(p.len.max(1) as i64) as f32 / p.len.max(1) as f32,
                        _ => 0.0,
                    },
                    bars: p.bars,
                    volume: p.volume,
                    stale_tempo: p.state != PadState::Empty && (p.bpm - self.tr.bpm).abs() > 0.01,
                    beat: p.beat,
                })
                .collect(),
            pad_bars: self.pad_bars,
            pads_from_midi: self.pads_from_midi,
            pad_countin: !running && self.pad_clock < 0,
            midi_activity: self.midi_activity,
            can_undo: !self.undo.is_empty(),
        }
    }

    /// Copy of the song with fresh effects, for exporting (no live input).
    pub fn clone_for_render(&self) -> Engine {
        let (_tx, rx) = crossbeam_channel::bounded(1);
        let mut e = Engine::new(self.sr, rx);
        e.tr.bpm = self.tr.bpm;
        e.tr.metronome = false;
        e.master_vol = self.master_vol;
        for t in &self.tracks {
            let mut p = t.params.clone();
            p.monitor = false;
            let mut nt = Track::new(t.id, p, self.sr);
            nt.clips = t.clips.clone();
            nt.notes = t.notes.clone();
            e.tracks.push(nt);
        }
        e
    }
}
