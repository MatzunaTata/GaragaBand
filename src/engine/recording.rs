//! Finishing a recording: raw takes become clips on their tracks.

use super::{make_clip, AudioClip, Engine};
use parking_lot::Mutex;

/// Raw data of a finished recording (turned into clips outside the audio lock).
pub struct RawRec {
    pub track: u64,
    pub data: Vec<f32>,
    pub segs: Vec<usize>,
    pub rec_start: i64,
    pub loop_start: i64,
    pub comp: i64,
}

/// Stops the transport and turns raw takes into finished clips.
/// The expensive work happens outside the audio lock.
pub fn stop_and_finish(engine: &Mutex<Engine>) {
    let (raw, sr, mut id) = {
        let mut e = engine.lock();
        let raw = e.stop();
        let id = e.next_id;
        e.next_id += 10_000;
        (raw, e.sr, id)
    };
    if raw.is_empty() {
        return;
    }
    let mut clips: Vec<(u64, AudioClip)> = vec![];
    for r in raw {
        let looped = r.segs.len() > 1;
        for k in 0..r.segs.len() {
            let s0 = r.segs[k];
            let s1 = if k + 1 < r.segs.len() { r.segs[k + 1] } else { r.data.len() };
            if s1 <= s0 {
                continue;
            }
            let mut start = if k == 0 { r.rec_start } else { r.loop_start } - r.comp;
            let mut slice = &r.data[s0..s1];
            if start < 0 {
                let cut = ((-start) as usize).min(slice.len());
                slice = &slice[cut..];
                start = 0;
            }
            if slice.len() < (sr * 0.1) as usize {
                continue;
            }
            let peak = slice.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            if looped && peak < 0.004 {
                continue;
            }
            id += 1;
            clips.push((r.track, make_clip(id, start, slice.to_vec(), sr)));
        }
    }
    let mut e = engine.lock();
    for (tid, c) in clips {
        if let Some(t) = e.track_mut(tid) {
            t.clips.push(c);
        }
    }
}
