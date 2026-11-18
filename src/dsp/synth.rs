//! Polyphonic synthesizer for the keyboard tracks.

use crate::i18n::Text;
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq)]
enum Wave {
    Sine,
    Saw,
    Square,
    Tri,
    Organ,
}

pub struct Patch {
    pub name: Text,
    w1: Wave,
    w2: Wave,
    ratio2: f32,
    detune: f32,
    mix2: f32,
    fm: f32,
    fm_decay: f32,
    a: f32,
    d: f32,
    s: f32,
    r: f32,
    cutoff: f32,
    fenv: f32,
    fdecay: f32,
    q: f32,
    gain: f32,
    key_decay: bool,
}

/// Default patch; the entries in `PATCHES` override individual fields.
const fn patch(name: Text) -> Patch {
    Patch {
        name,
        w1: Wave::Saw,
        w2: Wave::Saw,
        ratio2: 1.0,
        detune: 0.0,
        mix2: 0.0,
        fm: 0.0,
        fm_decay: 0.0,
        a: 0.003,
        d: 0.3,
        s: 0.7,
        r: 0.2,
        cutoff: 18000.0,
        fenv: 0.0,
        fdecay: 0.3,
        q: 0.7,
        gain: 0.5,
        key_decay: false,
    }
}

pub const PATCHES: &[Patch] = &[
    Patch {
        w1: Wave::Saw,
        w2: Wave::Tri,
        mix2: 0.45,
        detune: 2.5,
        a: 0.002,
        d: 2.2,
        s: 0.0,
        r: 0.25,
        cutoff: 700.0,
        fenv: 3.2,
        fdecay: 0.35,
        q: 0.6,
        gain: 0.75,
        key_decay: true,
        ..patch(Text::new("Piano", "Klavier"))
    },
    Patch {
        w1: Wave::Sine,
        w2: Wave::Sine,
        ratio2: 1.0,
        fm: 1.7,
        fm_decay: 0.45,
        a: 0.002,
        d: 2.8,
        s: 0.0,
        r: 0.3,
        gain: 0.6,
        key_decay: true,
        ..patch(Text::new("E-Piano", "E-Piano"))
    },
    Patch {
        w1: Wave::Organ,
        w2: Wave::Sine,
        ratio2: 0.5,
        mix2: 0.25,
        a: 0.006,
        d: 0.1,
        s: 1.0,
        r: 0.06,
        gain: 0.4,
        ..patch(Text::new("Organ", "Orgel"))
    },
    Patch {
        w1: Wave::Saw,
        w2: Wave::Saw,
        ratio2: 1.0,
        mix2: 0.35,
        detune: 9.0,
        a: 0.22,
        d: 0.4,
        s: 0.85,
        r: 0.45,
        cutoff: 3200.0,
        gain: 0.4,
        ..patch(Text::new("Strings", "Streicher"))
    },
    Patch {
        w1: Wave::Saw,
        w2: Wave::Square,
        ratio2: 0.5,
        mix2: 0.3,
        detune: 12.0,
        a: 0.6,
        d: 1.0,
        s: 0.8,
        r: 1.3,
        cutoff: 900.0,
        fenv: 1.5,
        fdecay: 1.5,
        gain: 0.4,
        ..patch(Text::new("Synth Pad", "Synth-Fläche (Pad)"))
    },
    Patch {
        w1: Wave::Saw,
        w2: Wave::Square,
        ratio2: 1.0,
        mix2: 0.4,
        detune: 5.0,
        a: 0.004,
        d: 0.3,
        s: 0.75,
        r: 0.15,
        cutoff: 1500.0,
        fenv: 2.5,
        fdecay: 0.25,
        q: 1.6,
        gain: 0.4,
        ..patch(Text::new("Synth Lead", "Synth-Lead"))
    },
    Patch {
        w1: Wave::Saw,
        w2: Wave::Square,
        ratio2: 0.5,
        mix2: 0.5,
        a: 0.003,
        d: 0.35,
        s: 0.6,
        r: 0.1,
        cutoff: 260.0,
        fenv: 3.5,
        fdecay: 0.18,
        q: 1.8,
        gain: 0.6,
        ..patch(Text::new("Synth Bass", "Synth-Bass"))
    },
    Patch {
        w1: Wave::Sine,
        w2: Wave::Sine,
        ratio2: 3.5,
        fm: 2.4,
        fm_decay: 0.6,
        a: 0.001,
        d: 2.2,
        s: 0.0,
        r: 0.8,
        gain: 0.5,
        ..patch(Text::new("Bells", "Glocken"))
    },
    Patch {
        w1: Wave::Tri,
        w2: Wave::Sine,
        ratio2: 4.0,
        mix2: 0.15,
        a: 0.001,
        d: 0.45,
        s: 0.0,
        r: 0.2,
        cutoff: 2500.0,
        fenv: 2.0,
        fdecay: 0.08,
        gain: 0.7,
        ..patch(Text::new("Marimba", "Marimba"))
    },
    Patch {
        w1: Wave::Saw,
        w2: Wave::Saw,
        mix2: 0.3,
        detune: 4.0,
        a: 0.002,
        d: 0.9,
        s: 0.0,
        r: 0.3,
        cutoff: 500.0,
        fenv: 4.0,
        fdecay: 0.12,
        q: 1.0,
        gain: 0.6,
        key_decay: true,
        ..patch(Text::new("Pluck Synth", "Zupf-Synth (Pluck)"))
    },
];

#[derive(Clone, Copy, Default)]
struct Voice {
    active: bool,
    held: bool,
    sustained: bool,
    note: u8,
    vel: f32,
    freq: f32,
    pa: f32,
    pb: f32,
    pc: f32,
    stage: u8,
    env: f32,
    fenv: f32,
    fmenv: f32,
    ic1: f32,
    ic2: f32,
    g: f32,
    age: u64,
}

/// PolyBLEP correction that smooths the discontinuities of saw/square waves.
#[inline]
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
fn osc(w: Wave, ph: f32, dt: f32, pm: f32) -> f32 {
    match w {
        Wave::Sine => (2.0 * PI * ph + pm).sin(),
        Wave::Saw => 2.0 * ph - 1.0 - blep(ph, dt),
        Wave::Square => {
            let mut v = if ph < 0.5 { 1.0 } else { -1.0 };
            v += blep(ph, dt);
            v -= blep((ph + 0.5).fract(), dt);
            v
        }
        Wave::Tri => 4.0 * (ph - 0.5).abs() - 1.0,
        Wave::Organ => {
            let p = 2.0 * PI * ph;
            (p.sin() + 0.6 * (2.0 * p).sin() + 0.4 * (3.0 * p).sin() + 0.25 * (4.0 * p).sin() + 0.15 * (6.0 * p).sin()) * 0.5
        }
    }
}

pub struct Synth {
    sr: f32,
    voices: [Voice; 16],
    counter: u64,
    sustain: bool,
    pub bend: f32,
}

impl Synth {
    pub fn new(sr: f32) -> Self {
        Self { sr, voices: [Voice::default(); 16], counter: 0, sustain: false, bend: 0.0 }
    }

    pub fn note_on(&mut self, note: u8, vel: u8) {
        self.counter += 1;
        // voice allocation: reuse the same note, else a free voice,
        // else the oldest released voice, else the oldest held voice
        let mut slot = 0;
        let mut best = u64::MAX;
        for (i, v) in self.voices.iter().enumerate() {
            if v.active && v.note == note {
                slot = i;
                break;
            }
            let score = if !v.active {
                0
            } else if !v.held {
                1 + v.age / 2
            } else {
                u64::MAX / 2 + v.age / 2
            };
            if score < best {
                best = score;
                slot = i;
            }
        }
        let old = self.voices[slot];
        self.voices[slot] = Voice {
            active: true,
            held: true,
            sustained: false,
            note,
            vel: (vel as f32 / 127.0).powf(1.3),
            freq: 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0),
            pa: 0.0,
            pb: 0.37,
            pc: 0.0,
            stage: 0,
            env: if old.active { old.env * 0.5 } else { 0.0 },
            fenv: 1.0,
            fmenv: 1.0,
            ic1: 0.0,
            ic2: 0.0,
            g: 0.1,
            age: self.counter,
        };
    }

    pub fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.held && v.note == note {
                v.held = false;
                if self.sustain {
                    v.sustained = true;
                } else {
                    v.stage = 3;
                }
            }
        }
    }

    pub fn set_sustain(&mut self, on: bool) {
        self.sustain = on;
        if !on {
            for v in self.voices.iter_mut() {
                if v.sustained {
                    v.sustained = false;
                    v.stage = 3;
                }
            }
        }
    }

    pub fn all_off(&mut self) {
        self.sustain = false;
        for v in self.voices.iter_mut() {
            if v.active {
                v.held = false;
                v.sustained = false;
                v.stage = 3;
            }
        }
    }

    /// Adds the rendered signal to `out`.
    pub fn render(&mut self, out: &mut [f32], patch_idx: usize) {
        let p = &PATCHES[patch_idx.min(PATCHES.len() - 1)];
        let sr = self.sr;
        let bend = 2f32.powf(self.bend / 12.0);
        let det = 2f32.powf(p.detune / 1200.0);
        let a_inc = 1.0 / (p.a * sr).max(1.0);
        let r_k = (-1.0 / (p.r * sr)).exp();
        let f_k = (-1.0 / (p.fdecay * sr)).exp();
        let fm_k = if p.fm_decay > 0.0 { (-1.0 / (p.fm_decay * sr)).exp() } else { 1.0 };
        let kq = 1.0 / p.q;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            let dmul = if p.key_decay { 2f32.powf((60.0 - v.note as f32) / 20.0) } else { 1.0 };
            let d_k = (-1.0 / (p.d * dmul * sr)).exp();
            let f = v.freq * bend;
            let (da, db, dc) = (f * det / sr, f / det / sr, f * p.ratio2 / sr);
            let track = (f / 261.6).powf(0.5);
            for (i, o) in out.iter_mut().enumerate() {
                match v.stage {
                    0 => {
                        v.env += a_inc;
                        if v.env >= 1.0 {
                            v.env = 1.0;
                            v.stage = 1;
                        }
                    }
                    1 => v.env = p.s + (v.env - p.s) * d_k,
                    _ => {
                        v.env *= r_k;
                        if v.env < 0.0005 {
                            v.active = false;
                            break;
                        }
                    }
                }
                if v.stage == 1 && p.s == 0.0 && v.env < 0.0005 {
                    v.active = false;
                    break;
                }
                v.fenv *= f_k;
                v.fmenv *= fm_k;
                let m = osc(p.w2, v.pc, dc, 0.0);
                let pm = if p.fm > 0.0 { p.fm * v.fmenv * (0.5 + 0.5 * v.vel) * m } else { 0.0 };
                let mut x = osc(p.w1, v.pa, da, pm);
                if p.detune > 0.0 {
                    x = 0.5 * (x + osc(p.w1, v.pb, db, pm));
                }
                if p.mix2 > 0.0 {
                    x = x * (1.0 - p.mix2) + m * p.mix2;
                }
                v.pa += da;
                if v.pa >= 1.0 {
                    v.pa -= 1.0;
                }
                v.pb += db;
                if v.pb >= 1.0 {
                    v.pb -= 1.0;
                }
                v.pc += dc;
                if v.pc >= 1.0 {
                    v.pc -= 1.0;
                }
                // state-variable low-pass (TPT form); cutoff is updated every 8 samples
                if p.cutoff < 17000.0 {
                    if i & 7 == 0 {
                        let fc = (p.cutoff * track * 2f32.powf(p.fenv * v.fenv * (0.4 + 0.6 * v.vel))).clamp(40.0, sr * 0.45);
                        v.g = (PI * fc / sr).tan();
                    }
                    let g = v.g;
                    let a1 = 1.0 / (1.0 + g * (g + kq));
                    let a2 = g * a1;
                    let a3 = g * a2;
                    let v3 = x - v.ic2;
                    let v1 = a1 * v.ic1 + a2 * v3;
                    let v2 = v.ic2 + a2 * v.ic1 + a3 * v3;
                    v.ic1 = 2.0 * v1 - v.ic1;
                    v.ic2 = 2.0 * v2 - v.ic2;
                    x = v2;
                }
                *o += x * v.env * v.vel * p.gain * 0.5;
            }
        }
    }
}
