//! GaragaBand - a simple music studio for Windows.
//!
//! Command line flags (all optional, mainly for testing):
//! - `--audio-test <file>`  silent device test, writes a report and exits
//! - `--no-audio`           start without opening an audio device
//! - `--no-help`            do not show the first-run help window
//! - `--demo`               start with demo content (used for screenshots)
//! - `--tab <sound|pads|notes>`  bottom tab to open at start
//! - `--lang <en|de>`       override the UI language for this run

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod dsp;
mod engine;
mod i18n;
mod project;
mod ui;

use eframe::egui;

/// Silent device test: opens input and output with several buffer settings,
/// plays only silence and writes the achieved values to a text file.
fn audio_test(out: &str) {
    use std::fmt::Write;
    use std::sync::Arc;
    let mut rep = String::new();
    let (outs, ins) = audio::list_devices();
    let _ = writeln!(rep, "Outputs: {outs:?}\nInputs: {ins:?}");
    for (low, ms) in [(true, 3.0f32), (true, 5.0), (true, 10.0), (false, 10.0)] {
        let (_tx, rx) = crossbeam_channel::bounded(4);
        let mut e = engine::Engine::new(48000.0, rx);
        let mut p = engine::TrackParams::preset(engine::TrackPreset::Voice);
        p.monitor = true; // run the full effect chain ...
        p.volume = 0.0; // ... but keep it inaudible
        e.add_track(p);
        e.master_vol = 0.0;
        let eng = Arc::new(parking_lot::Mutex::new(e));
        let cfg = audio::AudioConfig { out_name: None, in_name: None, low_latency: low, period_ms: ms };
        let h = audio::AudioHandle::start(cfg, eng.clone());
        let mut peak = 0.0f32;
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            peak = peak.max(eng.lock().tracks[0].in_meter);
        }
        let st = h.status.lock().clone();
        let _ = writeln!(rep, "--- low_latency={low} period_ms={ms}\n{st:#?}\nInput level (peak): {peak:.5}");
        drop(h);
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let _ = std::fs::write(out, rep);
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let value_of = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    if args.iter().any(|a| a == "--audio-test") {
        audio_test(value_of("--audio-test").map_or("audio-test.txt", |s| s.as_str()));
        return Ok(());
    }
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let opts = ui::StartOptions {
        no_audio: has("--no-audio"),
        no_help: has("--no-help"),
        demo: has("--demo"),
        tab: value_of("--tab").cloned(),
        lang: value_of("--lang").and_then(|code| i18n::Lang::from_code(code)),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GaragaBand")
            .with_inner_size([1500.0, 900.0])
            .with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native("GaragaBand", options, Box::new(move |cc| Ok(Box::new(ui::App::new(cc, opts)))))
}
