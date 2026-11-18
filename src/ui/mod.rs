//! User interface (egui).
//!
//! The UI never holds the audio lock for long: every frame it takes a
//! [`Snapshot`] of the engine, draws from it and writes changes back under a
//! short lock.
//!
//! - `theme`       colours, fonts, widget style
//! - `widgets`     small reusable widgets
//! - `config`      persisted user settings
//! - `input`       keyboard shortcuts and the computer keyboard as an instrument
//! - `file_ops`    new / open / save / export
//! - `top_bar`, `arrange`, `inspector`, `status_bar`, `dialogs`: the panels

mod arrange;
mod config;
mod dialogs;
mod file_ops;
mod input;
mod inspector;
mod status_bar;
mod theme;
mod top_bar;
mod widgets;

use crate::audio::{AudioConfig, AudioHandle, AudioStatus, MidiManager};
use crate::dsp::drums::{beat_hits, render_beat, BEATS};
use crate::engine::{make_clip, Engine, MidiEvent, Note, PadState, Snapshot, TrackParams, TrackPreset};
use crate::i18n::{self, Lang};
use config::AppConfig;
use crossbeam_channel::Sender;
use eframe::egui::{self, Key};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
pub use theme::ThemeMode;

/// Command line options that influence the UI.
#[derive(Default)]
pub struct StartOptions {
    /// Do not open an audio device.
    pub no_audio: bool,
    /// Do not show the first-run help window.
    pub no_help: bool,
    /// Fill the song with demo content (for screenshots).
    pub demo: bool,
    /// Tab of the bottom panel to open: `sound`, `pads` or `notes`.
    pub tab: Option<String>,
    /// Language for this run instead of the saved one.
    pub lang: Option<Lang>,
    /// Appearance for this run instead of the saved one.
    pub theme: Option<ThemeMode>,
}

/// Tabs of the bottom panel.
#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Sound,
    Pads,
    Notes,
}

/// A clip being dragged along the timeline.
struct Drag {
    track: u64,
    clip: u64,
    /// Clip start (frames) when the drag began.
    orig: i64,
    /// Pointer x position when the drag began.
    x0: f32,
}

pub struct App {
    engine: Arc<Mutex<Engine>>,
    audio: Option<AudioHandle>,
    last_audio_try: Instant,
    midi: MidiManager,
    midi_tx: Sender<MidiEvent>,
    cfg: AppConfig,
    /// Device names (outputs, inputs) for the settings window.
    devices: (Vec<String>, Vec<String>),
    show_settings: bool,
    show_help: bool,
    tab: Tab,
    /// Timeline zoom in pixels per beat.
    zoom: f32,
    /// Timeline scroll position in beats.
    scroll: f32,
    /// Selected clip as (track id, clip id).
    sel_clip: Option<(u64, u64)>,
    drag: Option<Drag>,
    /// Path of the current project file, if it has been saved.
    path: Option<PathBuf>,
    /// Message shown briefly at the bottom of the window.
    toast: Option<(String, Instant)>,
    /// Computer keys currently held, with the note each one triggered.
    keys_down: Vec<(Key, u8)>,
    octave: i32,
    /// Track whose name is being edited.
    rename: Option<u64>,
    /// Track waiting for a delete confirmation.
    confirm_delete: Option<u64>,
    /// Pending note-offs of notes previewed in the note editor.
    preview_off: Vec<(Instant, u8)>,
    /// Result message of the background export thread.
    export_result: Arc<Mutex<Option<String>>>,
    no_audio: bool,
    midi_seen: u32,
    midi_flash: Instant,
}

/// The four tracks every new song starts with.
fn default_tracks(e: &mut Engine) {
    e.add_track(TrackParams::preset(TrackPreset::Voice));
    e.add_track(TrackParams::preset(TrackPreset::Guitar));
    e.add_track(TrackParams::preset(TrackPreset::Keys));
    e.add_track(TrackParams::preset(TrackPreset::Drums));
    e.selected = e.tracks[0].id;
}

/// Demo content for checking the rendering and for screenshots (`--demo`).
fn load_demo(e: &mut Engine) {
    let sr = e.sr;
    let id = e.new_id();
    let wave: Vec<f32> = (0..(sr as usize * 6))
        .map(|i| {
            let t = i as f32 / sr;
            (t * 220.0 * std::f32::consts::TAU).sin() * (0.5 + 0.5 * (t * 3.0).sin()) * 0.5
        })
        .collect();
    e.tracks[0].clips.push(make_clip(id, (sr * 1.2) as i64, wave, sr));
    let melody = [60u8, 64, 67, 72, 67, 64, 62, 65];
    e.tracks[2].notes = Arc::new(melody.iter().enumerate().map(|(i, p)| Note { start: i as f64, len: 0.9, pitch: *p, vel: 90 }).collect());
    let mut drums = vec![];
    for bar in 0..4 {
        for (time, pitch, vel) in beat_hits(&BEATS[0]) {
            drums.push(Note { start: bar as f64 * 4.0 + time, len: 0.25, pitch, vel });
        }
    }
    e.tracks[3].notes = Arc::new(drums);
    e.tr.loop_on = true;
    e.selected = e.tracks[3].id;
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: StartOptions) -> Self {
        let cfg = AppConfig::load();
        // Must happen before the default tracks get their (translated) names.
        i18n::set_lang(opts.lang.unwrap_or(cfg.lang));
        theme::apply(&cc.egui_ctx, opts.theme.unwrap_or(cfg.theme));

        let (tx, rx) = crossbeam_channel::bounded(2048);
        let mut e = Engine::new(48000.0, rx);
        default_tracks(&mut e);
        if opts.demo {
            load_demo(&mut e);
        }
        let mut app = Self {
            engine: Arc::new(Mutex::new(e)),
            audio: None,
            last_audio_try: Instant::now(),
            midi: MidiManager::new(tx.clone()),
            midi_tx: tx,
            show_help: !cfg.help_shown && !opts.no_help,
            cfg,
            devices: (vec![], vec![]),
            show_settings: false,
            tab: match opts.tab.as_deref() {
                Some("pads") => Tab::Pads,
                Some("notes") => Tab::Notes,
                _ => Tab::Sound,
            },
            zoom: 34.0,
            scroll: 0.0,
            sel_clip: None,
            drag: None,
            path: None,
            toast: None,
            keys_down: vec![],
            octave: 0,
            rename: None,
            confirm_delete: None,
            preview_off: vec![],
            export_result: Arc::new(Mutex::new(None)),
            no_audio: opts.no_audio,
            midi_seen: 0,
            midi_flash: Instant::now() - Duration::from_secs(10),
        };
        app.start_audio();
        app
    }

    /// (Re)starts the audio thread with the current settings.
    fn start_audio(&mut self) {
        self.audio = None;
        self.last_audio_try = Instant::now();
        if self.no_audio {
            return;
        }
        let cfg = AudioConfig {
            out_name: self.cfg.out_dev.clone(),
            in_name: self.cfg.in_dev.clone(),
            low_latency: self.cfg.low_latency,
            period_ms: self.cfg.period_ms,
        };
        self.audio = Some(AudioHandle::start(cfg, self.engine.clone()));
    }

    fn status(&self) -> AudioStatus {
        self.audio.as_ref().map(|a| a.status.lock().clone()).unwrap_or_default()
    }

    /// Shows a short message at the bottom of the window.
    fn say(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    /// Sends a MIDI event to the engine as if it came from a keyboard.
    fn send_midi(&self, status: u8, d1: u8, d2: u8) {
        let _ = self.midi_tx.try_send(MidiEvent { status, d1, d2 });
    }

    /// Renders a preset beat at the current tempo into a loop pad.
    fn load_beat(&mut self, pad: usize, beat: usize, snap: &Snapshot) {
        let bar = ((snap.sr as f64 * 60.0 / snap.tr.bpm as f64 * 4.0).round() as usize).max(64);
        let buf = render_beat(&BEATS[beat], snap.sr, bar, 1, 0);
        self.engine.lock().pad_load(pad, buf, 1, BEATS[beat].name.en.to_string(), Some(beat as u8));
    }

    /// Per-frame background chores that are not tied to a panel.
    fn housekeeping(&mut self, snap: &Snapshot) {
        self.midi.poll();
        // retry a failed audio device every few seconds (e.g. after re-plugging)
        let st = self.status();
        if !self.no_audio && !st.running && st.error.is_some() && self.last_audio_try.elapsed().as_secs_f32() > 4.0 {
            self.start_audio();
        }
        // keep the recording offset in sync with the device latency
        let comp_ms = self.cfg.latency_comp_ms.unwrap_or(st.latency_ms);
        let comp = (comp_ms * snap.sr / 1000.0) as i64;
        {
            let mut e = self.engine.lock();
            if e.latency_comp != comp {
                e.latency_comp = comp;
            }
        }
        // release notes previewed in the note editor
        let now = Instant::now();
        let (due, pending): (Vec<_>, Vec<_>) = self.preview_off.drain(..).partition(|(at, _)| *at <= now);
        self.preview_off = pending;
        for (_, note) in due {
            self.send_midi(0x80, note, 0);
        }
        let done = self.export_result.lock().take();
        if let Some(msg) = done {
            self.say(msg);
        }
        if snap.midi_activity != self.midi_seen {
            self.midi_seen = snap.midi_activity;
            self.midi_flash = now;
        }
        // preset beats follow tempo changes: re-render them at the new tempo
        for (i, p) in snap.pads.iter().enumerate() {
            if p.stale_tempo && !matches!(p.state, PadState::Recording | PadState::QueuedRec) {
                if let Some(b) = p.beat {
                    self.load_beat(i, b as usize, snap);
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snap = self.engine.lock().snapshot();
        self.housekeeping(&snap);
        // The panels edit a copy of the track parameters; changes are written
        // back to the engine in one go at the end of the frame.
        let mut edit: Vec<TrackParams> = snap.tracks.iter().map(|t| t.params.clone()).collect();
        self.shortcuts(ctx, &snap);

        let panel = egui::Frame::none().fill(theme::pal().panel);
        egui::TopBottomPanel::top("top")
            .frame(panel.inner_margin(egui::Margin::symmetric(12.0, 4.0)))
            .show(ctx, |ui| self.top_bar(ui, &snap));
        egui::TopBottomPanel::bottom("status")
            .frame(panel.inner_margin(egui::Margin::symmetric(12.0, 5.0)))
            .show(ctx, |ui| self.status_bar(ui, &snap));
        egui::TopBottomPanel::bottom("inspector")
            .frame(panel.inner_margin(egui::Margin::symmetric(12.0, 8.0)))
            .resizable(true)
            .default_height(352.0)
            .min_height(240.0)
            .show(ctx, |ui| self.inspector(ui, &snap, &mut edit));
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::pal().bg).inner_margin(egui::Margin::symmetric(10.0, 8.0)))
            .show(ctx, |ui| self.arrange(ui, &snap, &mut edit));
        self.windows(ctx, &snap);

        {
            let mut e = self.engine.lock();
            for (track, params) in snap.tracks.iter().zip(edit) {
                if params != track.params {
                    if let Some(t) = e.track_mut(track.id) {
                        t.params = params;
                    }
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn on_exit(&mut self) {
        self.audio = None;
        self.cfg.save();
    }
}
