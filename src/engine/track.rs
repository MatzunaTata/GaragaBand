//! Tracks and what they contain: parameters, MIDI notes and audio clips.

use crate::dsp::drums::DrumKit;
use crate::dsp::fx::{FxChain, FxParams, PRESETS};
use crate::dsp::synth::Synth;
use crate::i18n::tr;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum TrackKind {
    Audio,
    Instrument,
    Drums,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TrackPreset {
    Voice,
    Guitar,
    Keys,
    Drums,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct TrackParams {
    pub name: String,
    pub kind: TrackKind,
    pub icon: u8,
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub armed: bool,
    pub monitor: bool,
    pub input_ch: u8,
    pub instrument: u8,
    pub kit: u8,
    pub fx: FxParams,
}

impl Default for TrackParams {
    fn default() -> Self {
        Self {
            name: tr("Track", "Spur").into(),
            kind: TrackKind::Audio,
            icon: 0,
            volume: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            armed: false,
            monitor: true,
            input_ch: 0,
            instrument: 0,
            kit: 0,
            fx: FxParams::default(),
        }
    }
}

impl TrackParams {
    /// Ready-made track settings. The indices refer to `fx::PRESETS`.
    pub fn preset(p: TrackPreset) -> Self {
        match p {
            TrackPreset::Voice => Self {
                name: tr("Voice", "Stimme").into(),
                kind: TrackKind::Audio,
                icon: 0,
                input_ch: 0,
                fx: (PRESETS[2].make)(),
                ..Default::default()
            },
            TrackPreset::Guitar => Self {
                name: tr("Guitar", "Gitarre").into(),
                kind: TrackKind::Audio,
                icon: 1,
                input_ch: 1,
                fx: (PRESETS[5].make)(),
                ..Default::default()
            },
            TrackPreset::Keys => {
                Self { name: "Keyboard".into(), kind: TrackKind::Instrument, icon: 2, fx: (PRESETS[8].make)(), ..Default::default() }
            }
            TrackPreset::Drums => Self {
                name: tr("Drums", "Schlagzeug").into(),
                kind: TrackKind::Drums,
                icon: 3,
                fx: (PRESETS[8].make)(),
                ..Default::default()
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub struct Note {
    pub start: f64,
    pub len: f64,
    pub pitch: u8,
    pub vel: u8,
}

#[derive(Clone)]
pub struct AudioClip {
    pub id: u64,
    pub start: i64,
    pub data: Arc<Vec<f32>>,
    pub peaks: Arc<Vec<f32>>,
}

/// Samples per waveform peak value (used for drawing clips).
pub const PEAK_STEP: usize = 256;

/// Builds a clip from raw samples: short fades at both ends plus waveform peaks.
pub fn make_clip(id: u64, start: i64, mut data: Vec<f32>, sr: f32) -> AudioClip {
    let fade = ((sr * 0.004) as usize).min(data.len() / 2);
    let len = data.len();
    for i in 0..fade {
        let g = i as f32 / fade as f32;
        data[i] *= g;
        data[len - 1 - i] *= g;
    }
    let peaks = data.chunks(PEAK_STEP).map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs()))).collect();
    AudioClip { id, start, data: Arc::new(data), peaks: Arc::new(peaks) }
}

pub struct Track {
    pub id: u64,
    pub params: TrackParams,
    pub clips: Vec<AudioClip>,
    pub notes: Arc<Vec<Note>>,
    pub(super) synth: Synth,
    pub(super) drums: DrumKit,
    pub(super) fx: FxChain,
    pub(super) rec_active: bool,
    pub(super) rec_audio: Vec<f32>,
    pub(super) rec_segs: Vec<usize>,
    pub(super) rec_notes: Vec<Note>,
    pub(super) held: [Option<(f64, u8)>; 128],
    pub(super) sounding: Vec<(f64, u8)>,
    pub(super) idle: usize,
    pub meter: f32,
    pub in_meter: f32,
}

impl Track {
    pub fn new(id: u64, params: TrackParams, sr: f32) -> Self {
        Self {
            id,
            params,
            clips: vec![],
            notes: Arc::new(vec![]),
            synth: Synth::new(sr),
            drums: DrumKit::new(sr),
            fx: FxChain::new(sr),
            rec_active: false,
            rec_audio: vec![],
            rec_segs: Vec::with_capacity(1024),
            rec_notes: Vec::with_capacity(8192),
            held: [None; 128],
            sounding: Vec::with_capacity(256),
            idle: 0,
            meter: 0.0,
            in_meter: 0.0,
        }
    }

    pub(super) fn note_on(&mut self, n: u8, v: u8) {
        self.idle = 0;
        match self.params.kind {
            TrackKind::Instrument => self.synth.note_on(n, v),
            TrackKind::Drums => self.drums.note_on(n, v, self.params.kit),
            TrackKind::Audio => {}
        }
    }

    pub(super) fn note_off(&mut self, n: u8) {
        if self.params.kind == TrackKind::Instrument {
            self.synth.note_off(n);
        }
    }

    pub(super) fn silence(&mut self) {
        for (_, p) in self.sounding.drain(..) {
            self.synth.note_off(p);
        }
    }
}
