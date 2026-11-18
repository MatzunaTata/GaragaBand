//! Saving/loading projects and exporting the song as a WAV file.
//!
//! A project is a JSON file (`.gbp`) next to a `<name>_audio` folder that holds
//! the recorded clips and loop pads as 32-bit float WAV files.

use crate::engine::{make_clip, Engine, Note, PadState, Track, TrackParams};
use crate::i18n::tr;
use crate::trf;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
struct ClipFile {
    start: i64,
    file: String,
}

#[derive(Serialize, Deserialize)]
struct TrackFile {
    params: TrackParams,
    notes: Vec<Note>,
    clips: Vec<ClipFile>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct PadFile {
    name: String,
    bars: u32,
    volume: f32,
    file: Option<String>,
    beat: Option<u8>,
    bpm: f32,
}

#[derive(Serialize, Deserialize)]
struct ProjFile {
    version: u32,
    sr: f32,
    bpm: f32,
    master_vol: f32,
    loop_on: bool,
    loop_start_bar: u32,
    loop_bars: u32,
    metronome: bool,
    count_in: bool,
    tracks: Vec<TrackFile>,
    #[serde(default)]
    pads: Vec<PadFile>,
}

/// Folder next to the project file that holds its WAV files.
fn audio_dir(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("song".into());
    path.with_file_name(format!("{stem}_audio"))
}

fn write_wav(path: &Path, sr: f32, ch: u16, data: impl Iterator<Item = f32>) -> Result<(), String> {
    let spec = hound::WavSpec { channels: ch, sample_rate: sr as u32, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for s in data {
        w.write_sample(s).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}

fn read_wav(path: &Path) -> Result<(Vec<f32>, u16, f32), String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = r.spec();
    let data: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().filter_map(|s| s.ok()).collect(),
        hound::SampleFormat::Int => {
            let k = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().filter_map(|s| s.ok()).map(|v| v as f32 * k).collect()
        }
    };
    Ok((data, spec.channels, spec.sample_rate as f32))
}

/// Linear-interpolation resampler.
fn resample(data: &[f32], from: f32, to: f32) -> Vec<f32> {
    if (from - to).abs() < 1.0 || data.is_empty() {
        return data.to_vec();
    }
    let n = (data.len() as f64 * to as f64 / from as f64) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * from as f64 / to as f64;
            let i0 = p as usize;
            let fr = (p - i0 as f64) as f32;
            let a = data[i0.min(data.len() - 1)];
            let b = data[(i0 + 1).min(data.len() - 1)];
            a + (b - a) * fr
        })
        .collect()
}

/// Writes the project file and its audio folder. The engine lock is only held
/// while copying the state; the slow file I/O happens afterwards.
pub fn save(engine: &Mutex<Engine>, path: &Path) -> Result<(), String> {
    struct T {
        params: TrackParams,
        notes: Arc<Vec<Note>>,
        clips: Vec<(i64, u64, Arc<Vec<f32>>)>,
    }
    let (sr, tracks, pads, mut pf) = {
        let e = engine.lock();
        let tracks: Vec<T> = e
            .tracks
            .iter()
            .map(|t| T {
                params: t.params.clone(),
                notes: t.notes.clone(),
                clips: t.clips.iter().map(|c| (c.start, c.id, c.data.clone())).collect(),
            })
            .collect();
        let pads: Vec<(PadFile, Vec<[f32; 2]>)> = e
            .pads
            .iter()
            .map(|p| {
                let has = !matches!(p.state, PadState::Empty | PadState::QueuedRec | PadState::Recording);
                (
                    PadFile { name: p.name.clone(), bars: p.bars, volume: p.volume, file: None, beat: p.beat, bpm: p.bpm },
                    if has { p.buf.clone() } else { vec![] },
                )
            })
            .collect();
        let pf = ProjFile {
            version: 1,
            sr: e.sr,
            bpm: e.tr.bpm,
            master_vol: e.master_vol,
            loop_on: e.tr.loop_on,
            loop_start_bar: e.tr.loop_start_bar,
            loop_bars: e.tr.loop_bars,
            metronome: e.tr.metronome,
            count_in: e.tr.count_in,
            tracks: vec![],
            pads: vec![],
        };
        (e.sr, tracks, pads, pf)
    };
    let dir = audio_dir(path);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dir_name = dir.file_name().unwrap().to_string_lossy().to_string();
    for (ti, t) in tracks.iter().enumerate() {
        let mut clips = vec![];
        for (start, id, data) in &t.clips {
            let name = format!("track{}_clip{}.wav", ti + 1, id);
            write_wav(&dir.join(&name), sr, 1, data.iter().copied())?;
            clips.push(ClipFile { start: *start, file: format!("{dir_name}/{name}") });
        }
        pf.tracks.push(TrackFile { params: t.params.clone(), notes: t.notes.to_vec(), clips });
    }
    for (i, (mut p, buf)) in pads.into_iter().enumerate() {
        if !buf.is_empty() {
            let name = format!("pad{}.wav", i + 1);
            write_wav(&dir.join(&name), sr, 2, buf.iter().flat_map(|s| [s[0], s[1]]))?;
            p.file = Some(format!("{dir_name}/{name}"));
        }
        pf.pads.push(p);
    }
    let json = serde_json::to_string_pretty(&pf).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Loads a project, resampling its audio if the device sample rate differs.
pub fn load(engine: &Mutex<Engine>, path: &Path) -> Result<(), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let pf: ProjFile =
        serde_json::from_str(&json).map_err(|e| trf!("Not a valid project file: {}", "Datei ist kein gültiges Projekt: {}", e))?;
    let base = path.parent().unwrap_or(Path::new("."));
    let sr = engine.lock().sr;
    let ratio = sr as f64 / pf.sr as f64;
    let mut tracks = vec![];
    for tf in pf.tracks {
        let mut t = Track::new(0, tf.params, sr);
        t.notes = Arc::new(tf.notes);
        for c in tf.clips {
            let (data, ch, fsr) = read_wav(&base.join(&c.file))?;
            let mono: Vec<f32> = if ch > 1 { data.chunks(ch as usize).map(|f| f[0]).collect() } else { data };
            let mono = resample(&mono, fsr, sr);
            t.clips.push(make_clip(0, (c.start as f64 * ratio) as i64, mono, sr));
        }
        tracks.push(t);
    }
    let mut pads = vec![];
    for p in pf.pads {
        let buf = match &p.file {
            Some(f) => {
                let (data, ch, fsr) = read_wav(&base.join(f))?;
                let l: Vec<f32> = data.chunks(ch as usize).map(|f| f[0]).collect();
                let r: Vec<f32> = data.chunks(ch as usize).map(|f| f[(ch as usize - 1).min(1)]).collect();
                let (l, r) = (resample(&l, fsr, sr), resample(&r, fsr, sr));
                l.iter().zip(r.iter()).map(|(a, b)| [*a, *b]).collect()
            }
            None => vec![],
        };
        pads.push((p, buf));
    }
    let mut e = engine.lock();
    e.stop();
    e.tracks.clear();
    for mut t in tracks {
        t.id = e.new_id();
        for c in t.clips.iter_mut() {
            c.id = e.new_id();
        }
        e.tracks.push(t);
    }
    e.selected = e.tracks.first().map(|t| t.id).unwrap_or(0);
    e.tr.bpm = pf.bpm;
    e.tr.pos = 0;
    e.tr.loop_on = pf.loop_on;
    e.tr.loop_start_bar = pf.loop_start_bar;
    e.tr.loop_bars = pf.loop_bars.max(1);
    e.tr.metronome = pf.metronome;
    e.tr.count_in = pf.count_in;
    e.master_vol = pf.master_vol;
    for i in 0..e.pads.len() {
        e.pad_clear(i);
    }
    for (i, (p, buf)) in pads.into_iter().enumerate() {
        if !buf.is_empty() && i < e.pads.len() {
            e.pad_load(i, buf, p.bars, p.name, p.beat);
            e.pads[i].volume = p.volume;
            e.pads[i].bpm = p.bpm;
        }
    }
    Ok(())
}

/// Renders the whole song offline into a 24-bit WAV file.
pub fn export_wav(engine: &Mutex<Engine>, path: &Path) -> Result<(), String> {
    let mut e = engine.lock().clone_for_render();
    let end = e.song_end();
    if end <= 0 {
        return Err(tr("The song is still empty - record something first.", "Der Song ist noch leer – nimm zuerst etwas auf.").into());
    }
    let sr = e.sr;
    // song length plus a tail so reverb and echo can ring out
    let total = end as usize + (sr * 3.5) as usize;
    let spec = hound::WavSpec { channels: 2, sample_rate: sr as u32, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    e.tr.pos = 0;
    e.tr.loop_on = false;
    e.play();
    const BLOCK: usize = 512; // frames per render call
    let input = [0.0f32; BLOCK * 2];
    let mut out = [0.0f32; BLOCK * 2];
    let mut done = 0;
    while done < total {
        e.process(&input, &mut out);
        for s in out.iter() {
            w.write_sample((s * 8388607.0) as i32).map_err(|e| e.to_string())?;
        }
        done += BLOCK;
    }
    w.finalize().map_err(|e| e.to_string())
}
