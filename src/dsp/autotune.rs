//! Autotune: YIN pitch detection + formant-preserving PSOLA correction.
//!
//! The correction is computed as a low-pass of (target note - sung pitch).
//! This keeps vibrato and natural movement intact; only the average pitch is
//! pulled gently onto the note. The latency stays below one pitch period.

use crate::i18n::{tr, Text};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AutotuneParams {
    pub on: bool,
    /// Root note 0 = C ... 11 = B
    pub key: u8,
    /// 0 chromatic, 1 major, 2 minor, 3 major pentatonic, 4 minor pentatonic
    pub scale: u8,
    /// 1 = very natural (slow), 0 = robot (instant)
    pub natural: f32,
    /// Correction amount 0..1
    pub amount: f32,
}

impl Default for AutotuneParams {
    fn default() -> Self {
        Self { on: false, key: 0, scale: 0, natural: 0.55, amount: 1.0 }
    }
}

pub const SCALE_NAMES: [Text; 5] = [
    Text::new("All notes (chromatic)", "Alle Töne (chromatisch)"),
    Text::new("Major", "Dur"),
    Text::new("Minor", "Moll"),
    Text::new("Major pentatonic", "Dur-Pentatonik"),
    Text::new("Minor pentatonic", "Moll-Pentatonik"),
];
const KEY_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Name of a pitch class (0 = C). German note naming calls B "H".
pub fn key_name(pitch_class: u8) -> &'static str {
    match pitch_class % 12 {
        11 => tr("B", "H"),
        pc => KEY_NAMES[pc as usize],
    }
}

const SCALES: [&[u8]; 5] =
    [&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], &[0, 2, 4, 5, 7, 9, 11], &[0, 2, 3, 5, 7, 8, 10], &[0, 2, 4, 7, 9], &[0, 3, 5, 7, 10]];

/// Nearest note of the given scale to a (fractional) MIDI pitch.
pub fn nearest_scale_note(midi: f32, key: u8, scale: u8) -> f32 {
    let sc = SCALES[(scale as usize).min(4)];
    let base = midi.floor() as i32;
    let mut best = midi.round();
    let mut best_d = f32::MAX;
    for n in (base - 3)..=(base + 4) {
        let pc = (n - key as i32).rem_euclid(12) as u8;
        if sc.contains(&pc) {
            let d = (midi - n as f32).abs();
            if d < best_d {
                best_d = d;
                best = n as f32;
            }
        }
    }
    best
}

/// Note name with octave, e.g. `A4` for MIDI note 69.
pub fn note_name(midi: f32) -> String {
    let n = midi.round() as i32;
    format!("{}{}", key_name(n.rem_euclid(12) as u8), n.div_euclid(12) - 1)
}

// ---------------------------------------------------------------- Detection

/// YIN pitch detector running on a 2x downsampled copy of the signal.
pub struct PitchDetector {
    ds_sr: f32,
    buf: Vec<f32>,
    pos: usize,
    n: usize,
    w: usize,
    tau_min: usize,
    tau_max: usize,
    hop: usize,
    since: usize,
    filled: usize,
    half: Option<f32>,
    d: Vec<f32>,
}

impl PitchDetector {
    pub fn new(sr: f32) -> Self {
        let ds_sr = sr / 2.0;
        let tau_max = (ds_sr / 70.0) as usize;
        let tau_min = ((ds_sr / 1000.0) as usize).max(2);
        let w = tau_max * 3 / 2;
        let n = w + tau_max + 2;
        Self {
            ds_sr,
            buf: vec![0.0; n * 2],
            pos: n,
            n,
            w,
            tau_min,
            tau_max,
            hop: ((ds_sr * 0.005) as usize).max(16),
            since: 0,
            filled: 0,
            half: None,
            d: vec![0.0; tau_max + 2],
        }
    }

    /// Returns `Some(Some(freq))` (pitch found) or `Some(None)` (unvoiced) on
    /// every analysis step, otherwise `None`.
    #[inline]
    pub fn push(&mut self, x: f32) -> Option<Option<f32>> {
        let h = match self.half.take() {
            None => {
                self.half = Some(x);
                return None;
            }
            Some(h) => h,
        };
        if self.pos == self.n * 2 {
            self.buf.copy_within(self.n.., 0);
            self.pos = self.n;
        }
        self.buf[self.pos] = 0.5 * (h + x);
        self.pos += 1;
        self.filled = (self.filled + 1).min(self.n);
        self.since += 1;
        if self.since >= self.hop && self.filled >= self.n {
            self.since = 0;
            return Some(self.analyze());
        }
        None
    }

    fn analyze(&mut self) -> Option<f32> {
        let x = &self.buf[self.pos - self.n..self.pos];
        let w = self.w;
        let mut e = 0.0f32;
        for v in &x[self.n - w..] {
            e += v * v;
        }
        if (e / w as f32).sqrt() < 0.004 {
            return None;
        }
        let d = &mut self.d;
        d[0] = 1.0;
        let mut running = 0.0f32;
        for tau in 1..=self.tau_max {
            let mut s = 0.0f32;
            let (a, b) = (&x[..w], &x[tau..tau + w]);
            for j in 0..w {
                let t = a[j] - b[j];
                s += t * t;
            }
            running += s;
            d[tau] = s * tau as f32 / running.max(1e-12);
        }
        let mut tau = 0usize;
        let mut t = self.tau_min;
        while t < self.tau_max {
            if d[t] < 0.15 {
                while t + 1 < self.tau_max && d[t + 1] < d[t] {
                    t += 1;
                }
                tau = t;
                break;
            }
            t += 1;
        }
        if tau == 0 {
            let mut m = self.tau_min;
            for t in self.tau_min..self.tau_max {
                if d[t] < d[m] {
                    m = t;
                }
            }
            if d[m] > 0.35 {
                return None;
            }
            tau = m;
        }
        let mut off = 0.0;
        if tau > 1 && tau < self.tau_max {
            let (a, b, c) = (d[tau - 1], d[tau], d[tau + 1]);
            let den = a - 2.0 * b + c;
            if den.abs() > 1e-9 {
                off = (0.5 * (a - c) / den).clamp(-0.5, 0.5);
            }
        }
        Some(self.ds_sr / (tau as f32 + off))
    }
}

// ---------------------------------------------------------------- Correction

const RING: usize = 8192;
const MASK: usize = RING - 1;

#[derive(Clone, Copy, Default)]
struct Grain {
    active: bool,
    src: f64,
    out: f64,
    half: f64,
}

/// Pitch corrector: re-spaces pitch-synchronous grains (PSOLA).
pub struct Autotune {
    sr: f32,
    det: PitchDetector,
    ring: Vec<f32>,
    n: i64,
    grains: [Grain; 8],
    next_c: f64,
    last_src: f64,
    period: f32,
    ratio: f64,
    shift: f32,
    target: f32,
    voiced: bool,
    voiced_frames: u32,
    cur_note: f32,
    hist: [f32; 3],
    /// For the UI: detected pitch (MIDI, 0 = none) and correction in cents
    pub disp_midi: f32,
    pub disp_cents: f32,
}

impl Autotune {
    pub fn new(sr: f32) -> Self {
        let period = sr / 200.0;
        Self {
            sr,
            det: PitchDetector::new(sr),
            ring: vec![0.0; RING],
            n: 0,
            grains: [Grain::default(); 8],
            next_c: 0.0,
            last_src: -(period as f64),
            period,
            ratio: 1.0,
            shift: 0.0,
            target: 0.0,
            voiced: false,
            voiced_frames: 0,
            cur_note: -1.0,
            hist: [0.0; 3],
            disp_midi: 0.0,
            disp_cents: 0.0,
        }
    }

    fn on_frame(&mut self, res: Option<f32>, p: &AutotuneParams) {
        match res {
            Some(f) if f > 60.0 && f < 1100.0 => {
                let raw = 69.0 + 12.0 * (f / 440.0).log2();
                self.hist = [self.hist[1], self.hist[2], raw];
                self.voiced_frames += 1;
                // the median over 3 steps suppresses single misdetections
                let midi = if self.voiced_frames >= 3 {
                    let mut h = self.hist;
                    h.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    h[1]
                } else {
                    raw
                };
                self.voiced = true;
                self.period = (self.sr / (440.0 * 2f32.powf((midi - 69.0) / 12.0))).clamp(self.sr / 1100.0, self.sr / 60.0);
                let cand = nearest_scale_note(midi, p.key, p.scale);
                if self.cur_note < 0.0 || (midi - cand).abs() + 0.2 < (midi - self.cur_note).abs() {
                    self.cur_note = cand;
                }
                self.target = ((self.cur_note - midi) * p.amount.clamp(0.0, 1.0)).clamp(-3.0, 3.0);
                self.disp_midi = midi;
            }
            _ => {
                self.voiced = false;
                self.voiced_frames = 0;
                self.cur_note = -1.0;
                self.target = 0.0;
                self.disp_midi = 0.0;
            }
        }
    }

    pub fn process(&mut self, buf: &mut [f32], p: &AutotuneParams) {
        let nat = p.natural.clamp(0.0, 1.0);
        let speed_s = 0.002 + 0.28 * nat * nat;
        let c_slow = 1.0 - (-1.0 / (self.sr * speed_s)).exp();
        let c_fast = (1.0 - (-1.0 / (self.sr * 0.004)).exp()).max(c_slow);
        let c_rel = 1.0 - (-1.0 / (self.sr * 0.08)).exp();
        for s in buf.iter_mut() {
            let x = *s;
            if let Some(res) = self.det.push(x) {
                self.on_frame(res, p);
            }
            let c = if !self.voiced {
                c_rel
            } else if self.voiced_frames < 6 {
                c_fast
            } else {
                c_slow
            };
            self.shift += (self.target - self.shift) * c;
            if self.n & 7 == 0 {
                self.ratio = (2f64.powf(self.shift as f64 / 12.0)).clamp(0.75, 1.4);
            }
            self.ring[(self.n as usize) & MASK] = x;
            let now = self.n as f64;
            let t = self.period as f64;
            while self.next_c - t <= now {
                let c = self.next_c;
                let k = ((c - self.last_src) / t).floor().max(0.0);
                let mut src = self.last_src + k * t;
                if src > c {
                    src = c;
                }
                if c - src > 4000.0 {
                    src = c - t;
                }
                self.last_src = src;
                let mut slot = 0;
                let mut oldest = f64::MAX;
                for (i, g) in self.grains.iter().enumerate() {
                    if !g.active {
                        slot = i;
                        break;
                    }
                    if g.out < oldest {
                        oldest = g.out;
                        slot = i;
                    }
                }
                self.grains[slot] = Grain { active: true, src, out: c, half: t };
                self.next_c += t / self.ratio;
            }
            let mut y = 0.0f64;
            let mut wsum = 0.0f64;
            for g in self.grains.iter_mut() {
                if !g.active {
                    continue;
                }
                let rel = now - g.out;
                if rel >= g.half {
                    g.active = false;
                    continue;
                }
                if rel <= -g.half {
                    continue;
                }
                let w = 0.5 * (1.0 + (PI * rel / g.half).cos());
                let pos = g.src + rel;
                let fl = pos.floor();
                let fr = (pos - fl) as f32;
                let i = fl as i64;
                let a = self.ring[(i as usize) & MASK];
                let b = if i < self.n { self.ring[((i + 1) as usize) & MASK] } else { a };
                y += w * (a + (b - a) * fr) as f64;
                wsum += w;
            }
            *s = (y / wsum.max(0.5)) as f32;
            self.n += 1;
        }
        self.disp_cents = self.shift * 100.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(freq: f32, sr: f32, n: usize) -> Vec<f32> {
        // harmonically rich, voice-like signal
        (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let mut v = 0.0;
                for h in 1..8 {
                    v += (2.0 * std::f32::consts::PI * freq * h as f32 * t).sin() / h as f32;
                }
                v * 0.2
            })
            .collect()
    }

    fn measure(sig: &[f32], sr: f32) -> f32 {
        let mut det = PitchDetector::new(sr);
        let mut fs = vec![];
        for &x in sig {
            if let Some(Some(f)) = det.push(x) {
                fs.push(f);
            }
        }
        fs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        fs[fs.len() / 2]
    }

    #[test]
    fn detects_pitch() {
        for f in [82.4f32, 110.0, 220.0, 440.0, 660.0] {
            let m = measure(&voice(f, 48000.0, 48000), 48000.0);
            assert!((m / f - 1.0).abs() < 0.004, "{f} -> {m}");
        }
    }

    #[test]
    fn corrects_to_nearest_note() {
        let sr = 48000.0;
        for (fin, want) in [(450.0f32, 440.0f32), (215.0, 220.0), (128.0, 130.81)] {
            let mut sig = voice(fin, sr, 96000);
            let mut at = Autotune::new(sr);
            let p = AutotuneParams { on: true, natural: 0.4, ..Default::default() };
            for ch in sig.chunks_mut(64) {
                at.process(ch, &p);
            }
            let m = measure(&sig[48000..], sr);
            let cents = 1200.0 * (m / want).log2();
            assert!(cents.abs() < 6.0, "{fin} -> {m} ({cents} cents off {want})");
            let rms_out: f32 = (sig[48000..].iter().map(|v| v * v).sum::<f32>() / 48000.0).sqrt();
            assert!(rms_out > 0.05 && rms_out < 0.6, "level {rms_out}");
        }
    }

    #[test]
    fn in_tune_passes_almost_unchanged() {
        let sr = 48000.0;
        let src = voice(440.0, sr, 48000);
        let mut sig = src.clone();
        let mut at = Autotune::new(sr);
        let p = AutotuneParams { on: true, ..Default::default() };
        for ch in sig.chunks_mut(64) {
            at.process(ch, &p);
        }
        let m = measure(&sig[24000..], sr);
        assert!((1200.0 * (m / 440.0).log2()).abs() < 3.0);
        let e_in: f32 = src[24000..].iter().map(|v| v * v).sum();
        let e_out: f32 = sig[24000..].iter().map(|v| v * v).sum();
        assert!((e_out / e_in - 1.0).abs() < 0.1, "{}", e_out / e_in);
    }
}
