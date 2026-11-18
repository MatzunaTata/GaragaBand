//! Effects: equalizer, compressor, overdrive, chorus, echo, reverb.

use super::autotune::{Autotune, AutotuneParams};
use crate::i18n::Text;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const REVERB_NAMES: [Text; 4] = [
    Text::new("Ambience (very close)", "Ambience (ganz nah)"),
    Text::new("Room", "Raum"),
    Text::new("Hall", "Halle"),
    Text::new("Plate (studio)", "Platte (Studio)"),
];

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct FxParams {
    pub autotune: AutotuneParams,
    pub eq_on: bool,
    pub lowcut: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub comp_on: bool,
    pub comp: f32,
    pub drive_on: bool,
    pub drive: f32,
    pub chorus_on: bool,
    pub chorus: f32,
    pub delay_on: bool,
    pub delay_time: f32,
    pub delay_fb: f32,
    pub delay_mix: f32,
    pub reverb_on: bool,
    pub reverb_kind: u8,
    pub reverb_mix: f32,
    pub reverb_size: f32,
}

impl Default for FxParams {
    fn default() -> Self {
        Self {
            autotune: AutotuneParams::default(),
            eq_on: false,
            lowcut: 20.0,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            comp_on: false,
            comp: 0.4,
            drive_on: false,
            drive: 0.4,
            chorus_on: false,
            chorus: 0.5,
            delay_on: false,
            delay_time: 350.0,
            delay_fb: 0.35,
            delay_mix: 0.25,
            reverb_on: false,
            reverb_kind: 2,
            reverb_mix: 0.25,
            reverb_size: 0.5,
        }
    }
}

// ------------------------------------------------------------------ Biquad

/// Second-order IIR filter (RBJ cookbook coefficients, transposed direct form II).
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    fn set(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
    pub fn highpass(&mut self, sr: f32, f: f32, q: f32) {
        let w = 2.0 * PI * (f / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q);
        self.set((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al);
    }
    pub fn lowpass(&mut self, sr: f32, f: f32, q: f32) {
        let w = 2.0 * PI * (f / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q);
        self.set((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al);
    }
    pub fn bandpass(&mut self, sr: f32, f: f32, q: f32) {
        let w = 2.0 * PI * (f / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q);
        self.set(al, 0.0, -al, 1.0 + al, -2.0 * c, 1.0 - al);
    }
    pub fn peak(&mut self, sr: f32, f: f32, q: f32, db: f32) {
        let a = 10f32.powf(db / 40.0);
        let w = 2.0 * PI * (f / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q);
        self.set(1.0 + al * a, -2.0 * c, 1.0 - al * a, 1.0 + al / a, -2.0 * c, 1.0 - al / a);
    }
    pub fn lowshelf(&mut self, sr: f32, f: f32, db: f32) {
        let a = 10f32.powf(db / 40.0);
        let w = 2.0 * PI * (f / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let al = s / 2.0 * 2f32.sqrt();
        let sq = 2.0 * a.sqrt() * al;
        self.set(
            a * ((a + 1.0) - (a - 1.0) * c + sq),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - sq),
            (a + 1.0) + (a - 1.0) * c + sq,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - sq,
        );
    }
    pub fn highshelf(&mut self, sr: f32, f: f32, db: f32) {
        let a = 10f32.powf(db / 40.0);
        let w = 2.0 * PI * (f / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let al = s / 2.0 * 2f32.sqrt();
        let sq = 2.0 * a.sqrt() * al;
        self.set(
            a * ((a + 1.0) + (a - 1.0) * c + sq),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - sq),
            (a + 1.0) - (a - 1.0) * c + sq,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - sq,
        );
    }
}

// ------------------------------------------------------------------ Reverb
//
// Freeverb-style network: 8 parallel damped comb filters feeding 4 all-pass
// filters in series, per channel. The right channel is slightly detuned.

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const AP_TUNING: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

struct Comb {
    buf: Vec<f32>,
    len: usize,
    idx: usize,
    store: f32,
    fb: f32,
}

impl Comb {
    fn new(max: usize) -> Self {
        Self { buf: vec![0.0; max + 1], len: max, idx: 0, store: 0.0, fb: 0.5 }
    }
    #[inline]
    fn process(&mut self, x: f32, damp: f32) -> f32 {
        let y = self.buf[self.idx];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.idx] = x + self.store * self.fb;
        self.idx += 1;
        if self.idx >= self.len {
            self.idx = 0;
        }
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    len: usize,
    idx: usize,
}

impl Allpass {
    fn new(max: usize) -> Self {
        Self { buf: vec![0.0; max + 1], len: max, idx: 0 }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.idx];
        self.buf[self.idx] = x + b * 0.5;
        self.idx += 1;
        if self.idx >= self.len {
            self.idx = 0;
        }
        b - x
    }
}

pub struct Reverb {
    sr: f32,
    combs: [Vec<Comb>; 2],
    aps: [Vec<Allpass>; 2],
    pre: Vec<f32>,
    pre_len: usize,
    pre_idx: usize,
    damp: f32,
    kind: u8,
    size: f32,
}

impl Reverb {
    pub fn new(sr: f32) -> Self {
        let k = sr / 44100.0;
        let mk_c = |sp: usize| COMB_TUNING.iter().map(|&l| Comb::new(((l + sp) as f32 * k) as usize + 2)).collect();
        let mk_a = |sp: usize| AP_TUNING.iter().map(|&l| Allpass::new(((l + sp) as f32 * k) as usize + 2)).collect();
        let mut r = Self {
            sr,
            combs: [mk_c(0), mk_c(SPREAD)],
            aps: [mk_a(0), mk_a(SPREAD)],
            pre: vec![0.0; (sr * 0.05) as usize + 2],
            pre_len: 1,
            pre_idx: 0,
            damp: 0.3,
            kind: 255,
            size: -1.0,
        };
        r.configure(2, 0.5);
        r
    }

    fn configure(&mut self, kind: u8, size: f32) {
        let kind = kind.min(3);
        let ki = kind as usize;
        let scale = [0.34, 0.6, 1.0, 0.8][ki];
        let rt60 = [0.3, 0.7, 2.0, 1.4][ki] * (0.5 + 1.6 * size.clamp(0.0, 1.0));
        self.damp = [0.4, 0.4, 0.3, 0.12][ki];
        let k = self.sr / 44100.0;
        let clear = kind != self.kind;
        for ch in 0..2 {
            let sp = if ch == 1 { SPREAD } else { 0 };
            for (i, c) in self.combs[ch].iter_mut().enumerate() {
                c.len = (((COMB_TUNING[i] + sp) as f32 * k * scale) as usize).max(8);
                c.fb = 10f32.powf(-3.0 * c.len as f32 / (self.sr * rt60)).min(0.98);
                if clear {
                    c.buf.fill(0.0);
                    c.idx = 0;
                    c.store = 0.0;
                }
            }
            for (i, a) in self.aps[ch].iter_mut().enumerate() {
                a.len = (((AP_TUNING[i] + sp) as f32 * k * scale.max(0.5)) as usize).max(8);
                if clear {
                    a.buf.fill(0.0);
                    a.idx = 0;
                }
            }
        }
        self.pre_len = ((self.sr * [0.003, 0.008, 0.022, 0.001][ki]) as usize).max(1);
        if clear {
            self.pre.fill(0.0);
            self.pre_idx = 0;
        }
        self.kind = kind;
        self.size = size;
    }

    /// Adds the wet reverb signal to l/r.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], kind: u8, size: f32, mix: f32) {
        if kind != self.kind || (size - self.size).abs() > 1e-4 {
            self.configure(kind, size);
        }
        let wet = mix * 1.6;
        for i in 0..l.len() {
            let x = (l[i] + r[i]) * 0.5 * 0.03;
            let d = self.pre[self.pre_idx];
            self.pre[self.pre_idx] = x;
            self.pre_idx += 1;
            if self.pre_idx >= self.pre_len {
                self.pre_idx = 0;
            }
            let mut ol = 0.0;
            let mut or = 0.0;
            for c in self.combs[0].iter_mut() {
                ol += c.process(d, self.damp);
            }
            for c in self.combs[1].iter_mut() {
                or += c.process(d, self.damp);
            }
            for a in self.aps[0].iter_mut() {
                ol = a.process(ol);
            }
            for a in self.aps[1].iter_mut() {
                or = a.process(or);
            }
            l[i] += ol * wet;
            r[i] += or * wet;
        }
    }
}

// ------------------------------------------------------------------ Chain

pub struct FxChain {
    sr: f32,
    pub autotune: Autotune,
    hp: Biquad,
    ls: Biquad,
    pk: Biquad,
    hs: Biquad,
    eq_key: (f32, f32, f32, f32),
    comp_env: f32,
    dr_hp: Biquad,
    dr_lp: Biquad,
    ch_buf: Vec<f32>,
    ch_idx: usize,
    ch_ph: f32,
    dl_buf: Vec<f32>,
    dl_idx: usize,
    dl_lp: f32,
    dl_time: f32,
    reverb: Reverb,
}

impl FxChain {
    pub fn new(sr: f32) -> Self {
        let mut dr_hp = Biquad::default();
        dr_hp.highpass(sr, 90.0, 0.7);
        let mut dr_lp = Biquad::default();
        dr_lp.lowpass(sr, 5200.0, 0.7);
        Self {
            sr,
            autotune: Autotune::new(sr),
            hp: Biquad::default(),
            ls: Biquad::default(),
            pk: Biquad::default(),
            hs: Biquad::default(),
            eq_key: (-1.0, 0.0, 0.0, 0.0),
            comp_env: 0.0,
            dr_hp,
            dr_lp,
            ch_buf: vec![0.0; 4096],
            ch_idx: 0,
            ch_ph: 0.0,
            dl_buf: vec![0.0; (sr * 2.0) as usize + 4],
            dl_idx: 0,
            dl_lp: 0.0,
            dl_time: 350.0,
            reverb: Reverb::new(sr),
        }
    }

    /// Mono in, stereo out (l/r are overwritten).
    pub fn process(&mut self, mono: &mut [f32], l: &mut [f32], r: &mut [f32], p: &FxParams, audio: bool) {
        let n = mono.len();
        let sr = self.sr;
        if audio && p.autotune.on {
            self.autotune.process(mono, &p.autotune);
        } else {
            self.autotune.disp_midi = 0.0;
        }
        if p.eq_on {
            let key = (p.lowcut, p.bass, p.mid, p.treble);
            if key != self.eq_key {
                self.eq_key = key;
                self.hp.highpass(sr, p.lowcut.clamp(20.0, 400.0), 0.707);
                self.ls.lowshelf(sr, 150.0, p.bass);
                self.pk.peak(sr, 1200.0, 0.8, p.mid);
                self.hs.highshelf(sr, 6000.0, p.treble);
            }
            for x in mono.iter_mut() {
                *x = self.hs.process(self.pk.process(self.ls.process(self.hp.process(*x))));
            }
        }
        // compressor: a single knob drives threshold, ratio and make-up gain
        if p.comp_on {
            let amt = p.comp.clamp(0.0, 1.0);
            let thr = -8.0 - 26.0 * amt;
            let slope = 1.0 - 1.0 / (2.0 + 6.0 * amt);
            let makeup = 10f32.powf(-thr * slope * 0.45 / 20.0);
            let att = 1.0 - (-1.0 / (sr * 0.004)).exp();
            let rel = 1.0 - (-1.0 / (sr * 0.16)).exp();
            for x in mono.iter_mut() {
                let a = x.abs();
                let c = if a > self.comp_env { att } else { rel };
                self.comp_env += (a - self.comp_env) * c;
                let db = 20.0 * (self.comp_env + 1e-6).log10();
                let over = db - thr;
                let g = if over > 0.0 { 10f32.powf(-over * slope / 20.0) } else { 1.0 };
                *x *= g * makeup;
            }
        }
        // overdrive: band-limited, slightly asymmetric tanh waveshaper
        if p.drive_on {
            let d = p.drive.clamp(0.0, 1.0);
            let g = 1.0 + d * d * 40.0;
            let lvl = (1.0 + 0.5 * d) / g.sqrt();
            for x in mono.iter_mut() {
                let v = self.dr_hp.process(*x) * g;
                let y = (v + 0.15 * v * v.abs().min(1.0)).tanh();
                *x = self.dr_lp.process(y) * lvl;
            }
        }
        // stereo from here on
        if p.chorus_on {
            let amt = p.chorus.clamp(0.0, 1.0);
            let base = 0.012 * sr;
            let depth = 0.004 * sr * (0.4 + 0.6 * amt);
            let inc = 0.7 / sr;
            let mix = 0.5 * amt + 0.2;
            for i in 0..n {
                let x = mono[i];
                self.ch_buf[self.ch_idx] = x;
                let tap = |ph: f32, buf: &Vec<f32>, idx: usize| {
                    let d = base + depth * (2.0 * PI * ph).sin();
                    let pos = idx as f32 + 4096.0 - d;
                    let i0 = pos as usize;
                    let fr = pos - i0 as f32;
                    let a = buf[i0 & 4095];
                    let b = buf[(i0 + 1) & 4095];
                    a + (b - a) * fr
                };
                let wl = tap(self.ch_ph, &self.ch_buf, self.ch_idx);
                let wr = tap(self.ch_ph + 0.25, &self.ch_buf, self.ch_idx);
                l[i] = x * (1.0 - mix * 0.5) + wl * mix;
                r[i] = x * (1.0 - mix * 0.5) + wr * mix;
                self.ch_idx = (self.ch_idx + 1) & 4095;
                self.ch_ph += inc;
                if self.ch_ph >= 1.0 {
                    self.ch_ph -= 1.0;
                }
            }
        } else {
            l[..n].copy_from_slice(mono);
            r[..n].copy_from_slice(mono);
        }
        if p.delay_on {
            let len = self.dl_buf.len();
            let fb = p.delay_fb.clamp(0.0, 0.85);
            let mix = p.delay_mix.clamp(0.0, 1.0);
            for i in 0..n {
                // glide the delay time so that turning the knob does not click
                self.dl_time += (p.delay_time - self.dl_time) * 0.0005;
                let d = (self.dl_time.clamp(20.0, 1500.0) * 0.001 * sr) as usize;
                let rd = (self.dl_idx + len - d.min(len - 2)) % len;
                let y = self.dl_buf[rd];
                self.dl_lp += (y - self.dl_lp) * 0.45;
                self.dl_buf[self.dl_idx] = (l[i] + r[i]) * 0.5 + self.dl_lp * fb;
                self.dl_idx = (self.dl_idx + 1) % len;
                l[i] += y * mix;
                r[i] += y * mix;
            }
        }
        if p.reverb_on {
            self.reverb.process(&mut l[..n], &mut r[..n], p.reverb_kind, p.reverb_size, p.reverb_mix);
        }
    }
}

// ------------------------------------------------------------------ Presets

pub struct FxPreset {
    pub name: Text,
    /// Only offered on audio tracks (voice/guitar).
    pub audio: bool,
    pub make: fn() -> FxParams,
}

fn base() -> FxParams {
    FxParams::default()
}

pub const PRESETS: &[FxPreset] = &[
    FxPreset { name: Text::new("No effects (dry)", "Ohne Effekte (trocken)"), audio: false, make: base },
    FxPreset {
        name: Text::new("Voice: natural", "Stimme: natürlich"),
        audio: true,
        make: || FxParams {
            eq_on: true,
            lowcut: 90.0,
            treble: 2.0,
            comp_on: true,
            comp: 0.45,
            reverb_on: true,
            reverb_kind: 1,
            reverb_mix: 0.22,
            ..base()
        },
    },
    FxPreset {
        name: Text::new("Voice: pop with Autotune", "Stimme: Pop mit Autotune"),
        audio: true,
        make: || FxParams {
            autotune: AutotuneParams { on: true, natural: 0.55, ..Default::default() },
            eq_on: true,
            lowcut: 100.0,
            mid: -1.0,
            treble: 3.5,
            comp_on: true,
            comp: 0.6,
            delay_on: true,
            delay_time: 320.0,
            delay_fb: 0.25,
            delay_mix: 0.1,
            reverb_on: true,
            reverb_kind: 3,
            reverb_mix: 0.25,
            ..base()
        },
    },
    FxPreset {
        name: Text::new("Voice: big hall", "Stimme: große Halle"),
        audio: true,
        make: || FxParams {
            eq_on: true,
            lowcut: 100.0,
            comp_on: true,
            comp: 0.45,
            reverb_on: true,
            reverb_kind: 2,
            reverb_mix: 0.45,
            reverb_size: 0.75,
            ..base()
        },
    },
    FxPreset {
        name: Text::new("Voice: robot (hard Autotune)", "Stimme: Roboter (hartes Autotune)"),
        audio: true,
        make: || FxParams {
            autotune: AutotuneParams { on: true, natural: 0.0, ..Default::default() },
            eq_on: true,
            lowcut: 100.0,
            treble: 3.0,
            comp_on: true,
            comp: 0.7,
            reverb_on: true,
            reverb_kind: 3,
            reverb_mix: 0.2,
            ..base()
        },
    },
    FxPreset {
        name: Text::new("Guitar: clean", "Gitarre: clean"),
        audio: true,
        make: || FxParams {
            eq_on: true,
            lowcut: 70.0,
            treble: 2.0,
            comp_on: true,
            comp: 0.35,
            chorus_on: true,
            chorus: 0.3,
            reverb_on: true,
            reverb_kind: 1,
            reverb_mix: 0.2,
            ..base()
        },
    },
    FxPreset {
        name: Text::new("Guitar: crunch", "Gitarre: Crunch"),
        audio: true,
        make: || FxParams {
            eq_on: true,
            lowcut: 80.0,
            mid: 2.0,
            drive_on: true,
            drive: 0.45,
            reverb_on: true,
            reverb_kind: 0,
            reverb_mix: 0.2,
            ..base()
        },
    },
    FxPreset {
        name: Text::new("Guitar: rock solo", "Gitarre: Rock-Solo"),
        audio: true,
        make: || FxParams {
            eq_on: true,
            lowcut: 90.0,
            mid: 3.0,
            drive_on: true,
            drive: 0.8,
            delay_on: true,
            delay_time: 380.0,
            delay_fb: 0.35,
            delay_mix: 0.2,
            reverb_on: true,
            reverb_kind: 2,
            reverb_mix: 0.2,
            ..base()
        },
    },
    FxPreset {
        name: Text::new("A little room", "Etwas Raum"),
        audio: false,
        make: || FxParams { reverb_on: true, reverb_kind: 0, reverb_mix: 0.3, ..base() },
    },
    FxPreset {
        name: Text::new("Lots of reverb", "Viel Hall"),
        audio: false,
        make: || FxParams { reverb_on: true, reverb_kind: 2, reverb_mix: 0.4, reverb_size: 0.7, ..base() },
    },
    FxPreset {
        name: Text::new("Echo + reverb", "Echo + Hall"),
        audio: false,
        make: || FxParams { delay_on: true, delay_mix: 0.3, reverb_on: true, reverb_kind: 2, reverb_mix: 0.25, ..base() },
    },
    FxPreset {
        name: Text::new("Wide and soft (chorus)", "Breit und weich (Chorus)"),
        audio: false,
        make: || FxParams { chorus_on: true, chorus: 0.6, reverb_on: true, reverb_kind: 1, reverb_mix: 0.25, ..base() },
    },
];
