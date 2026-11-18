//! Loop pads: short stereo loops that are recorded and played in sync with the bar.

use super::Engine;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PadState {
    Empty,
    Stopped,
    QueuedPlay,
    Playing,
    QueuedRec,
    Recording,
}

pub struct Pad {
    pub state: PadState,
    pub buf: Vec<[f32; 2]>,
    pub len: usize,
    pub bars: u32,
    pub origin: i64,
    pub rec_pos: usize,
    pub volume: f32,
    pub name: String,
    pub beat: Option<u8>,
    pub bpm: f32,
    pub(super) gain: f32,
}

impl Pad {
    pub(super) fn new() -> Self {
        Self {
            state: PadState::Empty,
            buf: vec![],
            len: 0,
            bars: 0,
            origin: 0,
            rec_pos: 0,
            volume: 0.8,
            name: String::new(),
            beat: None,
            bpm: 0.0,
            gain: 0.0,
        }
    }
    pub(super) fn active(&self) -> bool {
        !matches!(self.state, PadState::Empty | PadState::Stopped)
    }
}

impl Engine {
    pub(super) fn pads_active(&self) -> bool {
        self.pads.iter().any(|p| p.active())
    }

    pub fn pad_toggle(&mut self, i: usize) {
        if i >= self.pads.len() {
            return;
        }
        let idle = !(self.tr.playing && self.tr.precount == 0) && !self.pads_active();
        let bar = self.bar_len();
        let bars = self.pad_bars.clamp(1, 8);
        let count_in = self.tr.count_in;
        let bpm = self.tr.bpm;
        let p = &mut self.pads[i];
        match p.state {
            PadState::Empty => {
                p.len = (bar * bars as i64) as usize;
                p.bars = bars;
                p.buf = vec![[0.0; 2]; p.len];
                p.rec_pos = 0;
                p.beat = None;
                p.bpm = bpm;
                p.name = format!("Loop {}", i + 1);
                p.state = PadState::QueuedRec;
                if idle {
                    self.pad_clock = if count_in { -bar } else { 0 };
                }
            }
            PadState::Stopped => {
                p.state = PadState::QueuedPlay;
                if idle {
                    self.pad_clock = 0;
                }
            }
            PadState::Playing => p.state = PadState::Stopped,
            PadState::QueuedPlay => p.state = PadState::Stopped,
            PadState::QueuedRec | PadState::Recording => {
                p.state = PadState::Empty;
                p.buf = vec![];
            }
        }
    }

    pub fn pad_clear(&mut self, i: usize) {
        if let Some(p) = self.pads.get_mut(i) {
            *p = Pad::new();
        }
    }

    pub fn pad_load(&mut self, i: usize, buf: Vec<[f32; 2]>, bars: u32, name: String, beat: Option<u8>) {
        let bpm = self.tr.bpm;
        if let Some(p) = self.pads.get_mut(i) {
            let was_playing = matches!(p.state, PadState::Playing | PadState::QueuedPlay);
            p.len = buf.len();
            p.buf = buf;
            p.bars = bars;
            p.name = name;
            p.beat = beat;
            p.bpm = bpm;
            if !was_playing {
                p.state = PadState::Stopped;
            }
        }
    }

    pub(super) fn pad_boundary(&mut self, clock: i64) {
        for p in self.pads.iter_mut() {
            match p.state {
                PadState::QueuedPlay => {
                    p.state = PadState::Playing;
                    p.origin = clock;
                }
                PadState::QueuedRec => {
                    p.state = PadState::Recording;
                    p.origin = clock;
                    p.rec_pos = 0;
                }
                _ => {}
            }
        }
    }
}
