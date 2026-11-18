//! The real-time render loop: tracks, loop pads, metronome and master bus.

// The DSP loops index several fixed-size buffers in lockstep; plain index
// loops are the clearest way to write them.
#![allow(clippy::needless_range_loop)]

use super::{Engine, Note, PadState, TrackKind, MAX_BLOCK};

/// Fader position (0..1) to linear gain: quadratic taper, unity gain at 0.8.
#[inline]
fn fader_gain(v: f32) -> f32 {
    v * v * 1.5625
}

impl Engine {
    /// `input` and `out`: stereo, interleaved, same length.
    pub fn process(&mut self, input: &[f32], out: &mut [f32]) {
        let frames = out.len() / 2;
        self.handle_midi();
        let mut done = 0;
        while done < frames {
            let mut n = (frames - done).min(MAX_BLOCK);
            let running = self.tr.playing && self.tr.precount == 0;
            if running && self.tr.loop_on {
                let (_, le) = self.loop_range();
                if self.tr.pos < le {
                    n = n.min((le - self.tr.pos) as usize);
                }
            }
            if self.tr.precount > 0 {
                n = n.min(self.tr.precount as usize);
            }
            let clock = if running { self.tr.pos } else { self.pad_clock };
            let bar = self.bar_len();
            let ph = clock.rem_euclid(bar);
            n = n.min((bar - ph) as usize).max(1);
            if ph == 0 && clock >= 0 {
                self.pad_boundary(clock);
            }
            self.block(&input[done * 2..(done + n) * 2], &mut out[done * 2..(done + n) * 2], n, clock);
            done += n;
        }
    }

    /// Triggers the metronome click if a beat falls into the next `n` frames.
    fn click_check(&mut self, c: i64, n: usize) {
        let bl = self.beat_len();
        let k = (c as f64 / bl).ceil();
        let t = (k * bl) as i64;
        if t >= c && t < c + n as i64 {
            self.click_env = 1.0;
            self.click_ph = 0.0;
            self.click_f = if (k as i64).rem_euclid(4) == 0 { 1500.0 } else { 1000.0 };
        }
    }

    /// Renders one block of at most `MAX_BLOCK` frames.
    fn block(&mut self, input: &[f32], out: &mut [f32], n: usize, clock: i64) {
        let sr = self.sr;
        let running = self.tr.playing && self.tr.precount == 0;
        let recording = running && self.tr.recording;
        let pos = self.tr.pos;
        let bl = self.beat_len();
        let (b0, b1) = (pos as f64 / bl, (pos + n as i64) as f64 / bl);
        let any_solo = self.tracks.iter().any(|t| t.params.solo);
        let selected = self.selected;
        let comp = self.latency_comp;
        let (loop_s, loop_e) = self.loop_range();
        let rec_start = self.tr.rec_start;
        let meter_k = (-(n as f32) / (sr * 0.25)).exp();

        let mut ml = [0.0f32; MAX_BLOCK];
        let mut mr = [0.0f32; MAX_BLOCK];
        let mut cl = [0.0f32; MAX_BLOCK];
        let mut cr = [0.0f32; MAX_BLOCK];

        for t in self.tracks.iter_mut() {
            let mut mono_buf = [0.0f32; MAX_BLOCK];
            let mono = &mut mono_buf[..n];
            let audio = t.params.kind == TrackKind::Audio;
            if audio {
                let mut live = [0.0f32; MAX_BLOCK];
                let mut pk = 0.0f32;
                for i in 0..n {
                    let v = match t.params.input_ch {
                        0 => input[i * 2],
                        1 => input[i * 2 + 1],
                        _ => 0.5 * (input[i * 2] + input[i * 2 + 1]),
                    };
                    live[i] = v;
                    pk = pk.max(v.abs());
                }
                t.in_meter = (t.in_meter * meter_k).max(pk);
                if recording && t.rec_active {
                    t.rec_audio.extend_from_slice(&live[..n]);
                }
                if t.params.monitor {
                    mono.copy_from_slice(&live[..n]);
                }
                if running {
                    for c in &t.clips {
                        let rel = pos - c.start;
                        let len = c.data.len() as i64;
                        if rel + n as i64 <= 0 || rel >= len {
                            continue;
                        }
                        for i in 0..n {
                            let idx = rel + i as i64;
                            if idx >= 0 && idx < len {
                                mono[i] += c.data[idx as usize];
                            }
                        }
                    }
                    // monitor the earlier passes of the running loop recording
                    if recording && t.rec_active && t.rec_segs.len() > 1 {
                        for k in 0..t.rec_segs.len() - 1 {
                            let (s0, s1) = (t.rec_segs[k] as i64, t.rec_segs[k + 1] as i64);
                            let p0 = if k == 0 { rec_start } else { loop_s };
                            for i in 0..n {
                                let idx = s0 + (pos + i as i64 - p0) + comp;
                                if idx >= s0 && idx < s1 {
                                    mono[i] += t.rec_audio[idx as usize];
                                }
                            }
                        }
                    }
                }
            } else {
                if running {
                    let mut ons: [(u8, u8, f64); 32] = [(0, 0, 0.0); 32];
                    let mut cnt = 0;
                    for nt in t.notes.iter().chain(t.rec_notes.iter()) {
                        if nt.start >= b0 && nt.start < b1 && cnt < 32 {
                            ons[cnt] = (nt.pitch, nt.vel, nt.start + nt.len);
                            cnt += 1;
                        }
                    }
                    let mut j = 0;
                    while j < t.sounding.len() {
                        if t.sounding[j].0 < b1 {
                            let p = t.sounding.swap_remove(j).1;
                            t.note_off(p);
                        } else {
                            j += 1;
                        }
                    }
                    for &(p, v, end) in &ons[..cnt] {
                        t.note_on(p, v);
                        if t.sounding.len() < t.sounding.capacity() {
                            t.sounding.push((end, p));
                        }
                    }
                }
                match t.params.kind {
                    TrackKind::Instrument => t.synth.render(mono, t.params.instrument as usize),
                    _ => t.drums.render(mono),
                }
            }
            let quiet = mono.iter().all(|v| v.abs() < 1e-7);
            t.idle = if quiet { t.idle + n } else { 0 };
            if t.idle > (sr as usize) * 8 {
                t.meter *= meter_k;
                t.fx.autotune.disp_midi = 0.0;
                continue;
            }
            let mut l = [0.0f32; MAX_BLOCK];
            let mut r = [0.0f32; MAX_BLOCK];
            t.fx.process(mono, &mut l[..n], &mut r[..n], &t.params.fx, audio);
            let g = fader_gain(t.params.volume);
            let gl = g * (1.0 - t.params.pan.max(0.0));
            let gr = g * (1.0 + t.params.pan.min(0.0));
            let audible = !t.params.mute && (!any_solo || t.params.solo);
            let mut pk = 0.0f32;
            for i in 0..n {
                let (a, b) = (l[i] * gl, r[i] * gr);
                pk = pk.max(a.abs()).max(b.abs());
                if audible {
                    ml[i] += a;
                    mr[i] += b;
                    if t.id == selected {
                        cl[i] = a;
                        cr[i] = b;
                    }
                }
            }
            t.meter = (t.meter * meter_k).max(if audible { pk } else { 0.0 });
        }

        // Loop-Pads
        let fade_len = ((sr * 0.004) as usize).max(1);
        let slew = 1.0 / (sr * 0.006);
        for p in self.pads.iter_mut() {
            match p.state {
                PadState::Recording => {
                    for i in 0..n {
                        if p.rec_pos < p.len {
                            p.buf[p.rec_pos] = [cl[i], cr[i]];
                            p.rec_pos += 1;
                        }
                    }
                    if p.rec_pos >= p.len {
                        let f = fade_len.min(p.len / 2);
                        for i in 0..f {
                            let g = i as f32 / f as f32;
                            let e = p.len - 1 - i;
                            p.buf[i] = [p.buf[i][0] * g, p.buf[i][1] * g];
                            p.buf[e] = [p.buf[e][0] * g, p.buf[e][1] * g];
                        }
                        p.state = PadState::Playing;
                        p.gain = 1.0;
                    }
                }
                PadState::Playing | PadState::Stopped | PadState::QueuedPlay if p.len > 0 => {
                    let target = if p.state == PadState::Playing { 1.0 } else { 0.0 };
                    if target == 0.0 && p.gain <= 0.0 {
                        continue;
                    }
                    let mut ph = (clock - p.origin).rem_euclid(p.len as i64) as usize;
                    for i in 0..n {
                        p.gain = if target > p.gain { (p.gain + slew).min(1.0) } else { (p.gain - slew).max(0.0) };
                        let g = p.gain * fader_gain(p.volume);
                        let s = p.buf[ph];
                        ml[i] += s[0] * g;
                        mr[i] += s[1] * g;
                        ph += 1;
                        if ph >= p.len {
                            ph = 0;
                        }
                    }
                }
                _ => {}
            }
        }

        // Metronome
        let pads_active = self.pads_active();
        let pad_rec = self.pads.iter().any(|p| matches!(p.state, PadState::Recording | PadState::QueuedRec));
        if self.tr.precount > 0 {
            self.click_check(self.tr.precount_total - self.tr.precount, n);
        } else if running {
            if self.tr.metronome {
                self.click_check(pos, n);
            }
        } else if pads_active && (self.pad_clock < 0 || (pad_rec && self.tr.metronome)) {
            self.click_check(self.pad_clock, n);
        }
        if self.click_env > 0.001 {
            let k = (-1.0 / (sr * 0.012)).exp();
            let inc = self.click_f / sr;
            for i in 0..n {
                let v = (self.click_ph * std::f32::consts::TAU).sin() * self.click_env * 0.35;
                self.click_ph += inc;
                self.click_env *= k;
                ml[i] += v;
                mr[i] += v;
            }
        }

        // Master
        let mg = fader_gain(self.master_vol);
        let rel = 1.0 - (-1.0 / (sr * 0.2)).exp();
        let mut pk = [0.0f32; 2];
        for i in 0..n {
            let (mut a, mut b) = (ml[i] * mg, mr[i] * mg);
            let peak = a.abs().max(b.abs());
            let want = if peak > 0.95 { 0.95 / peak } else { 1.0 };
            if want < self.lim {
                self.lim = want;
            } else {
                self.lim += (want - self.lim) * rel;
            }
            a *= self.lim;
            b *= self.lim;
            pk[0] = pk[0].max(a.abs());
            pk[1] = pk[1].max(b.abs());
            out[i * 2] = a.clamp(-1.0, 1.0);
            out[i * 2 + 1] = b.clamp(-1.0, 1.0);
        }
        self.meter[0] = (self.meter[0] * meter_k).max(pk[0]);
        self.meter[1] = (self.meter[1] * meter_k).max(pk[1]);

        // advance time
        if running {
            self.tr.pos += n as i64;
            if self.tr.loop_on && self.tr.pos >= loop_e && pos < loop_e {
                self.tr.pos = loop_s;
                let end_beat = loop_e as f64 / bl;
                let rec = self.tr.recording;
                for t in self.tracks.iter_mut() {
                    t.silence();
                    if rec && t.rec_active {
                        if t.params.kind == TrackKind::Audio {
                            if t.rec_segs.len() < t.rec_segs.capacity() {
                                t.rec_segs.push(t.rec_audio.len());
                            }
                        } else {
                            for pch in 0..128 {
                                if let Some((s, v)) = t.held[pch].take() {
                                    if t.rec_notes.len() < t.rec_notes.capacity() {
                                        t.rec_notes.push(Note { start: s, len: (end_beat - s).max(0.05), pitch: pch as u8, vel: v });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            if self.tr.precount > 0 {
                self.tr.precount -= n as i64;
            }
            if pads_active {
                self.pad_clock += n as i64;
            } else {
                self.pad_clock = 0;
            }
        }
    }
}
