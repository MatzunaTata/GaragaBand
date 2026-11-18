//! Synthesized drum kit (General MIDI mapping, matching the MPK pads).

use super::fx::Biquad;
use crate::i18n::{tr, Text};
use std::f32::consts::PI;

pub const KIT_NAMES: [Text; 2] = [Text::new("Acoustic kit", "Akustik-Kit"), Text::new("Electro kit (808)", "Elektro-Kit (808)")];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Kick,
    Snare,
    Rim,
    Clap,
    HatClosed,
    HatOpen,
    Tom,
    Crash,
    Ride,
    Cowbell,
    Shaker,
}

/// Display name of a General MIDI drum note (empty if it has none).
pub fn drum_name(note: u8) -> &'static str {
    match note {
        35 | 36 => tr("Kick", "Bassdrum"),
        37 => tr("Rimshot", "Rimshot"),
        38 | 40 => tr("Snare", "Snare"),
        39 => tr("Clap", "Clap"),
        41 | 43 => tr("Low tom", "Tom tief"),
        45 | 47 => tr("Mid tom", "Tom mitte"),
        48 | 50 => tr("High tom", "Tom hoch"),
        42 => tr("Closed hi-hat", "Hi-Hat zu"),
        44 => tr("Pedal hi-hat", "Hi-Hat Pedal"),
        46 => tr("Open hi-hat", "Hi-Hat offen"),
        49 | 57 => tr("Crash", "Crash-Becken"),
        51 | 59 => tr("Ride", "Ride-Becken"),
        56 => tr("Cowbell", "Kuhglocke"),
        54 | 70 | 82 => tr("Shaker", "Shaker"),
        _ => "",
    }
}

/// Maps a MIDI note to a drum sound plus one sound-specific parameter
/// (tom pitch in Hz, hi-hat decay in seconds, snare pitch factor).
fn map(note: u8) -> (Kind, f32) {
    match note {
        35 | 36 => (Kind::Kick, 0.0),
        37 => (Kind::Rim, 0.0),
        38 | 40 => (Kind::Snare, if note == 40 { 1.15 } else { 1.0 }),
        39 => (Kind::Clap, 0.0),
        41 => (Kind::Tom, 82.0),
        43 => (Kind::Tom, 100.0),
        45 => (Kind::Tom, 125.0),
        47 => (Kind::Tom, 150.0),
        48 => (Kind::Tom, 180.0),
        50 => (Kind::Tom, 215.0),
        42 => (Kind::HatClosed, 0.035),
        44 => (Kind::HatClosed, 0.06),
        46 => (Kind::HatOpen, 0.0),
        49 | 57 | 52 | 55 => (Kind::Crash, 0.0),
        51 | 59 | 53 => (Kind::Ride, 0.0),
        56 => (Kind::Cowbell, 0.0),
        54 | 70 | 82 | 69 => (Kind::Shaker, 0.0),
        n => {
            // fold everything else onto the 16 main sounds
            let m = 36 + (n as i32 - 36).rem_euclid(16) as u8;
            if m == n {
                (Kind::Rim, 0.0)
            } else {
                map(m)
            }
        }
    }
}

/// Square-wave partials of the classic metallic hi-hat/cymbal oscillator bank.
const METAL: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

#[derive(Clone, Copy)]
struct DVoice {
    kind: Kind,
    active: bool,
    t: u32,
    vel: f32,
    f: f32,
    ph: [f32; 6],
    e1: f32,
    k1: f32,
    e2: f32,
    k2: f32,
    pe: f32,
    kp: f32,
    f_start: f32,
    f_end: f32,
    bq: Biquad,
    hp: f32,
}

pub struct DrumKit {
    sr: f32,
    voices: Vec<DVoice>,
    rng: u32,
}

impl DrumKit {
    pub fn new(sr: f32) -> Self {
        let v = DVoice {
            kind: Kind::Kick,
            active: false,
            t: 0,
            vel: 0.0,
            f: 0.0,
            ph: [0.0; 6],
            e1: 0.0,
            k1: 0.0,
            e2: 0.0,
            k2: 0.0,
            pe: 0.0,
            kp: 0.0,
            f_start: 0.0,
            f_end: 0.0,
            bq: Biquad::default(),
            hp: 0.0,
        };
        Self { sr, voices: vec![v; 16], rng: 0x1234_5678 }
    }

    pub fn all_off(&mut self) {
        for v in self.voices.iter_mut() {
            if v.active {
                v.k1 = v.k1.min(0.995);
                v.k2 = v.k2.min(0.995);
            }
        }
    }

    pub fn note_on(&mut self, note: u8, vel: u8, kit: u8) {
        let (kind, par) = map(note);
        let sr = self.sr;
        let k = |tau: f32| (-1.0 / (tau * sr)).exp();
        if kind == Kind::HatClosed {
            for v in self.voices.iter_mut() {
                if v.active && v.kind == Kind::HatOpen {
                    v.k1 = k(0.01);
                }
            }
        }
        let mut slot = 0;
        let mut oldest = 0;
        for (i, v) in self.voices.iter().enumerate() {
            if !v.active {
                slot = i;
                break;
            }
            if v.t > oldest {
                oldest = v.t;
                slot = i;
            }
        }
        let e = kit == 1;
        let v = &mut self.voices[slot];
        v.kind = kind;
        v.active = true;
        v.t = 0;
        v.vel = (vel as f32 / 127.0).powf(1.5);
        v.ph = [0.0; 6];
        v.e1 = 1.0;
        v.e2 = 1.0;
        v.pe = 1.0;
        v.hp = 0.0;
        v.f = par;
        v.bq = Biquad::default();
        match kind {
            Kind::Kick => {
                v.f_start = if e { 130.0 } else { 170.0 };
                v.f_end = if e { 43.0 } else { 50.0 };
                v.k1 = k(if e { 0.55 } else { 0.2 });
                v.kp = k(if e { 0.05 } else { 0.03 });
                v.k2 = k(0.004);
            }
            Kind::Snare => {
                v.f = 185.0 * if par > 0.0 { par } else { 1.0 };
                v.k1 = k(0.08);
                v.k2 = k(if e { 0.2 } else { 0.14 });
                v.bq.highpass(sr, if e { 1800.0 } else { 1100.0 }, 0.7);
            }
            Kind::Rim => {
                v.k1 = k(0.012);
                v.k2 = k(0.004);
            }
            Kind::Clap => {
                v.k1 = k(0.009);
                v.k2 = k(0.13);
                v.bq.bandpass(sr, 1150.0, 1.4);
            }
            Kind::HatClosed => {
                v.k1 = k(if par > 0.0 { par } else { 0.035 });
                v.bq.highpass(sr, 7500.0, 0.8);
            }
            Kind::HatOpen => {
                v.k1 = k(if e { 0.4 } else { 0.28 });
                v.bq.highpass(sr, 7000.0, 0.8);
            }
            Kind::Tom => {
                v.f_start = par * 1.7;
                v.f_end = par;
                v.k1 = k(if e { 0.35 } else { 0.24 });
                v.kp = k(0.04);
                v.k2 = k(0.006);
            }
            Kind::Crash => {
                v.k1 = k(1.1);
                v.bq.highpass(sr, 4500.0, 0.7);
            }
            Kind::Ride => {
                v.k1 = k(0.7);
                v.k2 = k(0.05);
                v.bq.highpass(sr, 6000.0, 0.7);
            }
            Kind::Cowbell => {
                v.k1 = k(0.22);
                v.bq.bandpass(sr, 800.0, 2.0);
            }
            Kind::Shaker => {
                v.k1 = k(0.07);
                v.bq.highpass(sr, 6000.0, 0.7);
            }
        }
    }

    /// Adds the rendered signal to `out`.
    pub fn render(&mut self, out: &mut [f32]) {
        let sr = self.sr;
        let mut rng = self.rng;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            for o in out.iter_mut() {
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = (rng >> 8) as f32 / 8388608.0 - 1.0;
                let metal = |ph: &mut [f32; 6], mul: f32| {
                    let mut m = 0.0;
                    for j in 0..6 {
                        ph[j] += METAL[j] * mul / sr;
                        if ph[j] >= 1.0 {
                            ph[j] -= 1.0;
                        }
                        m += if ph[j] < 0.5 { 1.0 } else { -1.0 };
                    }
                    m / 6.0
                };
                let y = match v.kind {
                    Kind::Kick => {
                        let f = v.f_end + (v.f_start - v.f_end) * v.pe;
                        v.pe *= v.kp;
                        v.ph[0] += f / sr;
                        let body = (2.0 * PI * v.ph[0]).sin() * v.e1;
                        let click = noise * v.e2 * 0.35;
                        ((body * 1.4).tanh() + click) * 0.9
                    }
                    Kind::Snare => {
                        v.ph[0] += v.f / sr;
                        v.ph[1] += v.f * 1.78 / sr;
                        let tone = ((2.0 * PI * v.ph[0]).sin() + 0.5 * (2.0 * PI * v.ph[1]).sin()) * v.e1;
                        let nz = v.bq.process(noise) * v.e2;
                        tone * 0.45 + nz * 0.55
                    }
                    Kind::Rim => {
                        v.ph[0] += 1700.0 / sr;
                        v.ph[1] += 480.0 / sr;
                        ((2.0 * PI * v.ph[0]).sin() * 0.5 + (2.0 * PI * v.ph[1]).sin() * 0.4) * v.e1 + noise * v.e2 * 0.3
                    }
                    Kind::Clap => {
                        let ms = v.t as f32 * 1000.0 / sr;
                        let env = if ms < 30.0 {
                            if v.t > 0 && (ms % 10.0) < 1000.0 / sr {
                                v.e1 = 1.0;
                            }
                            v.e1
                        } else {
                            v.e2
                        };
                        if ms < 30.0 {
                            v.e2 = 1.0;
                        }
                        v.bq.process(noise) * env * 1.6
                    }
                    Kind::HatClosed | Kind::HatOpen => {
                        let m = metal(&mut v.ph, 20.0);
                        v.bq.process(m * 0.6 + noise * 0.4) * v.e1 * 0.55
                    }
                    Kind::Tom => {
                        let f = v.f_end + (v.f_start - v.f_end) * v.pe;
                        v.pe *= v.kp;
                        v.ph[0] += f / sr;
                        (2.0 * PI * v.ph[0]).sin() * v.e1 * 0.8 + noise * v.e2 * 0.2
                    }
                    Kind::Crash => {
                        let m = metal(&mut v.ph, 14.0);
                        v.bq.process(m * 0.45 + noise * 0.55) * v.e1 * 0.5
                    }
                    Kind::Ride => {
                        let m = metal(&mut v.ph, 17.0);
                        v.bq.process(m * 0.7 + noise * 0.3) * (v.e1 * 0.3 + v.e2 * 0.3)
                    }
                    Kind::Cowbell => {
                        metal(&mut v.ph, 1.0); // only advances the oscillator phases
                        let sq = (if v.ph[4] < 0.5 { 1.0 } else { -1.0 }) + (if v.ph[5] < 0.5 { 1.0 } else { -1.0 });
                        v.bq.process(sq) * v.e1 * 0.5
                    }
                    Kind::Shaker => v.bq.process(noise) * v.e1 * 0.5,
                };
                v.e1 *= v.k1;
                v.e2 *= v.k2;
                v.t += 1;
                *o += y * v.vel * 0.8;
                if v.e1 < 0.0003 && (v.e2 < 0.0003 || !matches!(v.kind, Kind::Snare | Kind::Clap | Kind::Ride)) {
                    v.active = false;
                    break;
                }
            }
        }
        self.rng = rng;
    }
}

// ------------------------------------------------------------------ Preset beats

pub struct Beat {
    pub name: Text,
    /// (drum note, 16 steps per bar: x = loud, o = soft, - = rest)
    pub rows: &'static [(u8, &'static str)],
}

pub const BEATS: &[Beat] = &[
    Beat { name: Text::new("Rock", "Rock"), rows: &[(36, "x-------x-x-----"), (38, "----x-------x---"), (42, "x-x-x-x-x-x-x-x-")] },
    Beat {
        name: Text::new("Pop", "Pop"),
        rows: &[(36, "x-----x---x-----"), (38, "----x-------x---"), (42, "xoxoxoxoxoxoxo--"), (46, "--------------x-")],
    },
    Beat { name: Text::new("Hip-Hop", "Hip-Hop"), rows: &[(36, "x------x--x-----"), (39, "----x-------x---"), (42, "x-x-x-x-x-x-x-xx")] },
    Beat {
        name: Text::new("House", "House"),
        rows: &[(36, "x---x---x---x---"), (39, "----x-------x---"), (46, "--x---x---x---x-"), (42, "o---o---o---o---")],
    },
    Beat { name: Text::new("Funk", "Funk"), rows: &[(36, "x--x--x---x--x--"), (38, "----x--o-o--x--o"), (42, "xoxoxoxoxoxoxoxo")] },
    Beat { name: Text::new("Ballad", "Ballade"), rows: &[(36, "x---------x-----"), (37, "--------x-------"), (42, "x---x---x---x---")] },
    Beat {
        name: Text::new("Reggaeton", "Reggaeton"),
        rows: &[(36, "x---x---x---x---"), (38, "---x--x----x--x-"), (42, "x-x-x-x-x-x-x-x-")],
    },
    Beat { name: Text::new("Hi-hat + click only", "Nur Hi-Hat + Klick"), rows: &[(37, "x---x---x---x---"), (42, "--x---x---x---x-")] },
];

/// Returns (time in beats, note, velocity) for one bar.
pub fn beat_hits(beat: &Beat) -> Vec<(f64, u8, u8)> {
    let mut v = vec![];
    for (note, row) in beat.rows {
        for (i, c) in row.chars().enumerate() {
            let vel = match c {
                'x' => 105,
                'o' => 60,
                _ => continue,
            };
            v.push((i as f64 * 0.25, *note, vel));
        }
    }
    v
}

/// Renders a beat as a stereo loop (for the loop pads).
pub fn render_beat(beat: &Beat, sr: f32, bar_len: usize, bars: usize, kit: u8) -> Vec<[f32; 2]> {
    let total = bar_len * bars;
    let mut mono = vec![0.0f32; total];
    let mut dk = DrumKit::new(sr);
    let mut hits: Vec<(usize, u8, u8)> = vec![];
    for b in 0..bars {
        for (t, n, v) in beat_hits(beat) {
            hits.push((b * bar_len + (t / 4.0 * bar_len as f64) as usize, n, v));
        }
    }
    hits.sort();
    // two passes: the tail of the first pass ends up at the start of the loop
    for pass in 0..2 {
        let mut pos = 0;
        let mut hi = 0;
        if pass == 1 {
            mono.fill(0.0);
        }
        while pos < total {
            while hi < hits.len() && hits[hi].0 <= pos {
                dk.note_on(hits[hi].1, hits[hi].2, kit);
                hi += 1;
            }
            let next = if hi < hits.len() { hits[hi].0.min(total) } else { total };
            let end = next.max(pos + 1).min(total);
            dk.render(&mut mono[pos..end]);
            pos = end;
        }
    }
    mono.iter().map(|&x| [x, x]).collect()
}
