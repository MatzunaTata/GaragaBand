use super::*;
use crate::dsp::fx::PRESETS;
use crate::dsp::synth::PATCHES;
use parking_lot::Mutex;

fn mk() -> (Engine, crossbeam_channel::Sender<MidiEvent>) {
    let (tx, rx) = crossbeam_channel::bounded(256);
    let mut e = Engine::new(48000.0, rx);
    e.add_track(TrackParams::preset(TrackPreset::Voice));
    e.add_track(TrackParams::preset(TrackPreset::Guitar));
    e.add_track(TrackParams::preset(TrackPreset::Keys));
    e.add_track(TrackParams::preset(TrackPreset::Drums));
    (e, tx)
}

fn run(e: &mut Engine, frames: usize, tone: f32) -> f32 {
    let mut pk = 0.0f32;
    let mut done = 0;
    let mut ph = 0.0f32;
    while done < frames {
        let n = (frames - done).min(480);
        let mut inp = vec![0.0f32; n * 2];
        for i in 0..n {
            ph += 220.0 / 48000.0;
            let v = (ph * std::f32::consts::TAU).sin() * tone;
            inp[i * 2] = v;
            inp[i * 2 + 1] = v;
        }
        let mut out = vec![0.0f32; n * 2];
        e.process(&inp, &mut out);
        for v in &out {
            assert!(v.is_finite());
            pk = pk.max(v.abs());
        }
        done += n;
    }
    pk
}

#[test]
fn keys_and_pads_play_without_switching() {
    let (mut e, tx) = mk();
    e.selected = e.tracks[2].id;
    assert!(run(&mut e, 4800, 0.0) < 1e-4);
    tx.send(MidiEvent { status: 0x90, d1: 60, d2: 100 }).unwrap();
    assert!(run(&mut e, 4800, 0.0) > 0.01, "keyboard is silent");
    tx.send(MidiEvent { status: 0x80, d1: 60, d2: 0 }).unwrap();
    run(&mut e, 96000, 0.0);
    tx.send(MidiEvent { status: 0x99, d1: 36, d2: 110 }).unwrap();
    assert!(run(&mut e, 4800, 0.0) > 0.01, "drums are silent");
    assert!(e.tracks[3].meter > 0.01 && e.tracks[2].meter < 0.01);
}

#[test]
fn all_sounds_are_finite() {
    let (mut e, tx) = mk();
    e.selected = e.tracks[2].id;
    for i in 0..PATCHES.len() {
        e.tracks[2].params.instrument = i as u8;
        e.tracks[2].params.fx = (PRESETS[i % PRESETS.len()].make)();
        for n in [36u8, 60, 84] {
            tx.send(MidiEvent { status: 0x90, d1: n, d2: 127 }).unwrap();
        }
        let pk = run(&mut e, 24000, 0.0);
        assert!(pk > 0.01 && pk <= 1.0, "Patch {i}: {pk}");
        for n in [36u8, 60, 84] {
            tx.send(MidiEvent { status: 0x80, d1: n, d2: 0 }).unwrap();
        }
        run(&mut e, 24000, 0.0);
    }
    for kit in 0..2 {
        e.tracks[3].params.kit = kit;
        for n in 35..60u8 {
            tx.send(MidiEvent { status: 0x99, d1: n, d2: 120 }).unwrap();
            assert!(run(&mut e, 9600, 0.0) > 0.003, "drum {n} is silent");
        }
    }
    // every effect preset with a live signal
    e.selected = e.tracks[0].id;
    for p in PRESETS {
        e.tracks[0].params.fx = (p.make)();
        let pk = run(&mut e, 48000, 0.3);
        assert!(pk > 0.02 && pk <= 1.0, "{}: {pk}", p.name.en);
    }
}

#[test]
fn records_audio_and_midi_in_loop() {
    let (e, tx) = mk();
    let eng = Mutex::new(e);
    {
        let mut e = eng.lock();
        e.tracks[0].params.monitor = false;
        e.tracks[1].params.monitor = false;
        e.selected = e.tracks[0].id;
        e.tr.bpm = 120.0;
        e.tr.loop_on = true;
        e.tr.loop_bars = 1;
        e.record();
        assert!(e.tr.precount > 0);
        let bar = e.bar_len() as usize;
        run(&mut e, bar, 0.3); // count-in
        assert_eq!(e.tr.precount, 0);
        assert_eq!(e.tr.pos, 0);
        run(&mut e, bar / 4, 0.3);
        tx.send(MidiEvent { status: 0x99, d1: 38, d2: 100 }).unwrap();
        tx.send(MidiEvent { status: 0x90, d1: 64, d2: 100 }).unwrap();
        run(&mut e, bar / 4, 0.3);
        tx.send(MidiEvent { status: 0x80, d1: 64, d2: 0 }).unwrap();
        run(&mut e, bar, 0.3); // past the end of the loop
        assert!(e.tr.pos < bar as i64, "loop does not wrap around");
        run(&mut e, bar, 0.0); // silent pass -> gets discarded
    }
    stop_and_finish(&eng);
    let mut e = eng.lock();
    assert_eq!(e.tracks[3].notes.len(), 1);
    assert_eq!(e.tracks[2].notes.len(), 1);
    assert!((e.tracks[3].notes[0].start - 1.0).abs() < 0.05);
    assert!((e.tracks[2].notes[0].len - 1.0).abs() < 0.05);
    assert_eq!(e.tracks[0].clips.len(), 2, "expected two audible loop passes");
    assert!(e.tracks[1].clips.is_empty());
    // playback contains the recording
    e.tr.loop_on = false;
    e.seek(0);
    e.play();
    assert!(run(&mut e, 48000, 0.0) > 0.02);
    e.stop();
    e.undo();
    assert!(e.tracks[0].clips.is_empty() && e.tracks[3].notes.is_empty());
}

#[test]
fn loop_pad_records_and_repeats() {
    let (mut e, _tx) = mk();
    e.selected = e.tracks[0].id;
    e.tr.bpm = 120.0;
    e.pad_bars = 1;
    e.pad_toggle(0);
    e.tracks[1].params.monitor = false;
    assert_eq!(e.pads[0].state, PadState::QueuedRec);
    let bar = e.bar_len() as usize;
    run(&mut e, bar + 480, 0.3);
    assert_eq!(e.pads[0].state, PadState::Recording);
    run(&mut e, bar, 0.3);
    assert_eq!(e.pads[0].state, PadState::Playing);
    e.tracks[0].params.mute = true;
    assert!(run(&mut e, bar, 0.3) > 0.02, "pad does not play");
    e.pad_toggle(0);
    run(&mut e, 4800, 0.0);
    assert!(run(&mut e, 4800, 0.0) < 1e-3);
    assert_eq!(e.pads[0].state, PadState::Stopped);
}

#[test]
fn save_load_export_roundtrip() {
    let (mut e, _tx) = mk();
    let sr = e.sr;
    let id = e.new_id();
    let data: Vec<f32> = (0..48000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
    e.tracks[0].clips.push(make_clip(id, 24000, data, sr));
    e.tracks[3].notes = Arc::new(vec![Note { start: 0.0, len: 0.25, pitch: 36, vel: 100 }]);
    let eng = Mutex::new(e);
    let dir = std::env::temp_dir().join("garagaband_test");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("t.gbp");
    crate::project::save(&eng, &p).unwrap();
    eng.lock().tracks.clear();
    crate::project::load(&eng, &p).unwrap();
    {
        let e = eng.lock();
        assert_eq!(e.tracks.len(), 4);
        assert_eq!(e.tracks[0].clips[0].data.len(), 48000);
        assert_eq!(e.tracks[0].clips[0].start, 24000);
        assert_eq!(e.tracks[3].notes.len(), 1);
    }
    let w = dir.join("t.wav");
    crate::project::export_wav(&eng, &w).unwrap();
    let r = hound::WavReader::open(&w).unwrap();
    assert!(r.duration() > 72000);
    let pk = r.into_samples::<i32>().map(|s| s.unwrap().abs()).max().unwrap();
    assert!(pk > 100_000, "export is silent: {pk}");
    let _ = std::fs::remove_dir_all(&dir);
}
