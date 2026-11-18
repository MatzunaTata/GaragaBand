//! Audio input/output via WASAPI.
//!
//! Modes are tried in this order (fastest first):
//! 1. Exclusive mode (smallest buffer, direct access to the device)
//! 2. Shared mode with a small buffer (IAudioClient3)
//! 3. Regular shared mode (always works, but ~20-30 ms latency)

use crate::engine::{Engine, MAX_INPUTS};
use crate::i18n::tr;
use crate::trf;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use wasapi::*;

#[derive(Clone, Debug, PartialEq)]
pub struct AudioConfig {
    pub out_name: Option<String>,
    pub in_name: Option<String>,
    pub low_latency: bool,
    pub period_ms: f32,
}

/// How the audio device was opened.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub enum AudioMode {
    Exclusive,
    SharedLowLatency,
    #[default]
    Shared,
}

impl AudioMode {
    pub fn label(self) -> &'static str {
        match self {
            AudioMode::Exclusive => tr("Exclusive (fast)", "Exklusiv (schnell)"),
            AudioMode::SharedLowLatency => tr("Shared, small buffer", "Geteilt, kleiner Puffer"),
            AudioMode::Shared => tr("Shared (standard)", "Geteilt (Standard)"),
        }
    }
}

/// State of the audio thread as shown in the UI.
#[derive(Clone, Default, Debug)]
pub struct AudioStatus {
    pub running: bool,
    pub error: Option<String>,
    pub mode: AudioMode,
    pub out_name: String,
    pub in_name: String,
    pub sr: u32,
    /// Number of input channels tracks can choose from (0 without an input).
    pub in_channels: u32,
    pub period_frames: u32,
    pub latency_ms: f32,
    pub cpu: f32,
    pub notes: Vec<String>,
}

pub struct AudioHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub status: Arc<Mutex<AudioStatus>>,
}

impl AudioHandle {
    pub fn start(cfg: AudioConfig, engine: Arc<Mutex<Engine>>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(AudioStatus::default()));
        let (s2, st2) = (stop.clone(), status.clone());
        let thread = std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let _ = initialize_mta();
                if let Err(e) = run(&cfg, &engine, &st2, &s2) {
                    let mut st = st2.lock();
                    st.running = false;
                    st.error = Some(e);
                }
                st2.lock().running = false;
            })
            .ok();
        Self { stop, thread, status }
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Lists device names (outputs, inputs).
pub fn list_devices() -> (Vec<String>, Vec<String>) {
    let _ = initialize_mta();
    let list = |dir: Direction| -> Vec<String> {
        let mut v = vec![];
        if let Ok(en) = DeviceEnumerator::new() {
            if let Ok(col) = en.get_device_collection(dir) {
                for d in col.into_iter().flatten() {
                    if let Ok(n) = d.get_friendlyname() {
                        v.push(n);
                    }
                }
            }
        }
        v
    };
    (list(Direction::Render), list(Direction::Capture))
}

/// Name fragments of common USB audio interfaces. With automatic device
/// selection such a device is preferred over the Windows default device (which
/// is often the on-board sound card or a monitor).
#[rustfmt::skip]
const INTERFACE_HINTS: [&str; 34] = [
    "focusrite", "scarlett", "clarett", "behringer", "umc", "u-phoria", "usb audio codec", "presonus", "audiobox",
    "studio 24", "steinberg", "ur22", "ur12", "motu", "m-audio", "m-track", "audient", "evo", "solid state logic",
    "ssl 2", "universal audio", "volt", "rme", "fireface", "babyface", "tascam", "zoom", "roland", "rubix",
    "komplete audio", "minifuse", "mackie", "onyx", "audio interface",
];

fn is_interface(name: &str) -> bool {
    let n = name.to_lowercase();
    INTERFACE_HINTS.iter().any(|hint| n.contains(hint))
}

/// Picks the requested device, else a known audio interface, else the Windows default.
fn pick(en: &DeviceEnumerator, dir: Direction, want: &Option<String>) -> Result<Device, String> {
    let col = en.get_device_collection(dir).map_err(|e| e.to_string())?;
    if let Some(w) = want {
        if let Ok(d) = col.get_device_with_name(w) {
            return Ok(d);
        }
    } else {
        for d in col.into_iter().flatten() {
            if d.get_friendlyname().map(|n| is_interface(&n)).unwrap_or(false) {
                return Ok(d);
            }
        }
    }
    en.get_default_device(dir).map_err(|e| e.to_string())
}

/// Sample layout of a device stream.
#[derive(Clone, Copy, Debug)]
struct Fmt {
    bytes: usize,
    float: bool,
    ch: usize,
}

impl Fmt {
    fn of(w: &WaveFormat) -> Self {
        Self {
            bytes: (w.get_bitspersample() / 8) as usize,
            float: matches!(w.get_subformat(), Ok(SampleType::Float)),
            ch: w.get_nchannels() as usize,
        }
    }
    fn block(&self) -> usize {
        self.bytes * self.ch
    }
}

struct Out {
    client: AudioClient,
    render: AudioRenderClient,
    event: Handle,
    fmt: Fmt,
}

struct In {
    client: AudioClient,
    capture: AudioCaptureClient,
    _event: Handle,
    fmt: Fmt,
}

struct Streams {
    out: Out,
    inp: Option<In>,
    sr: u32,
    exclusive: bool,
    mode: AudioMode,
}

/// Sample rate and channel count Windows uses for the device in shared mode.
fn mix_layout(client: &AudioClient) -> Option<(usize, usize)> {
    client.get_mixformat().ok().map(|f| (f.get_samplespersec() as usize, f.get_nchannels() as usize))
}

/// First exclusive-mode format the device accepts for one of the channel counts.
fn excl_format_any(client: &AudioClient, sr: usize, channels: &[usize]) -> Option<WaveFormat> {
    channels.iter().find_map(|&ch| excl_format(client, sr, ch))
}

fn excl_format(client: &AudioClient, sr: usize, ch: usize) -> Option<WaveFormat> {
    for (store, valid, ty) in [
        (32, 32, SampleType::Float),
        (32, 24, SampleType::Int),
        (24, 24, SampleType::Int),
        (32, 32, SampleType::Int),
        (16, 16, SampleType::Int),
    ] {
        let f = WaveFormat::new(store, valid, ty, sr, ch, None);
        if let Ok(ok) = client.is_supported_exclusive_with_quirks(&f) {
            return Some(ok);
        }
    }
    None
}

fn init_excl(dev: &Device, fmt: &WaveFormat, dir: Direction, want_hns: i64) -> Result<AudioClient, String> {
    let mut client = dev.get_iaudioclient().map_err(|e| e.to_string())?;
    let period = client.calculate_aligned_period_near(want_hns, Some(128), fmt).map_err(|e| e.to_string())?;
    let mode = StreamMode::EventsExclusive { period_hns: period };
    if client.initialize_client(fmt, dir, mode, None).is_ok() {
        return Ok(client);
    }
    // Some drivers require a different alignment: retry with the buffer size they suggest
    let frames = client.get_buffer_size().unwrap_or(0) as i64;
    let mut client = dev.get_iaudioclient().map_err(|e| e.to_string())?;
    let period2 = if frames > 0 { calculate_period_100ns(frames, fmt.get_samplespersec() as i64) } else { period * 2 };
    client.initialize_client(fmt, dir, StreamMode::EventsExclusive { period_hns: period2 }, None).map_err(|e| e.to_string())?;
    Ok(client)
}

fn open_exclusive(out_dev: &Device, in_dev: Option<&Device>, period_ms: f32) -> Result<Streams, String> {
    let want = (period_ms.clamp(1.0, 20.0) * 10_000.0) as i64;
    let probe_out = out_dev.get_iaudioclient().map_err(|e| e.to_string())?;
    let probe_in = match in_dev {
        Some(d) => Some(d.get_iaudioclient().map_err(|e| e.to_string())?),
        None => None,
    };
    let mut last = tr("no matching format", "kein passendes Format").to_string();
    let out_mix = mix_layout(&probe_out);
    let in_mix = probe_in.as_ref().and_then(mix_layout);
    // The rate the device is set to first, then the common ones.
    let mut rates: Vec<usize> = vec![];
    for sr in out_mix.map(|m| m.0).into_iter().chain([48000, 44100, 96000, 88200]) {
        if !rates.contains(&sr) {
            rates.push(sr);
        }
    }
    // Output: stereo, or the full channel count if the driver insists on it.
    let out_channels: Vec<usize> = [2].into_iter().chain(out_mix.map(|m| m.1)).collect();
    // Input: all channels of a multi-input interface, else stereo or mono.
    let in_channels: Vec<usize> = in_mix.map(|m| m.1).filter(|&ch| ch > 2).into_iter().chain([2, 1]).collect();
    for sr in rates {
        let Some(of) = excl_format_any(&probe_out, sr, &out_channels) else { continue };
        let inf = match &probe_in {
            Some(p) => match excl_format_any(p, sr, &in_channels) {
                Some(f) => Some(f),
                None => continue,
            },
            None => None,
        };
        let oc = match init_excl(out_dev, &of, Direction::Render, want) {
            Ok(c) => c,
            Err(e) => {
                last = e;
                continue;
            }
        };
        let inp = match (in_dev, inf) {
            (Some(d), Some(f)) => match init_excl(d, &f, Direction::Capture, want) {
                Ok(c) => {
                    let ev = c.set_get_eventhandle().map_err(|e| e.to_string())?;
                    let cap = c.get_audiocaptureclient().map_err(|e| e.to_string())?;
                    Some(In { client: c, capture: cap, _event: ev, fmt: Fmt::of(&f) })
                }
                Err(e) => {
                    last = trf!("Input: {}", "Eingang: {}", e);
                    continue;
                }
            },
            _ => None,
        };
        let event = oc.set_get_eventhandle().map_err(|e| e.to_string())?;
        let render = oc.get_audiorenderclient().map_err(|e| e.to_string())?;
        return Ok(Streams {
            out: Out { client: oc, render, event, fmt: Fmt::of(&of) },
            inp,
            sr: sr as u32,
            exclusive: true,
            mode: AudioMode::Exclusive,
        });
    }
    Err(last)
}

fn open_shared(out_dev: &Device, in_dev: Option<&Device>, low: bool) -> Result<Streams, String> {
    let es = |e: WasapiError| e.to_string();
    let mut oc = out_dev.get_iaudioclient().map_err(es)?;
    let mix = oc.get_mixformat().map_err(es)?;
    let sr = mix.get_samplespersec();
    let mut ofmt = mix.clone();
    if low {
        let p = oc.get_shared_mode_engine_period(&mix).map_err(es)?;
        if p.min >= p.default {
            return Err(
                tr("Device offers no smaller buffer in shared mode", "Gerät bietet im geteilten Modus keinen kleineren Puffer").into()
            );
        }
        oc.initialize_shared_audio_stream(&mix, TimingMode::Events, p.min).map_err(es)?;
    } else {
        ofmt = WaveFormat::new(32, 32, SampleType::Float, sr as usize, 2, None);
        let (def, _) = oc.get_device_period().map_err(es)?;
        let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: def };
        oc.initialize_client(&ofmt, Direction::Render, mode, None).map_err(es)?;
    }
    let inp = match in_dev {
        None => None,
        Some(d) => {
            let open = || -> Result<In, String> {
                let mut ic = d.get_iaudioclient().map_err(es)?;
                let imix = ic.get_mixformat().map_err(es)?;
                let ifmt;
                if low {
                    if imix.get_samplespersec() != sr {
                        return Err(
                            tr("Input and output use different sample rates", "Ein- und Ausgang haben verschiedene Abtastraten").into()
                        );
                    }
                    let p = ic.get_shared_mode_engine_period(&imix).map_err(es)?;
                    ic.initialize_shared_audio_stream(&imix, TimingMode::Events, p.min).map_err(es)?;
                    ifmt = imix;
                } else {
                    // keep every input channel of the device
                    ifmt = WaveFormat::new(32, 32, SampleType::Float, sr as usize, imix.get_nchannels() as usize, None);
                    let (def, _) = ic.get_device_period().map_err(es)?;
                    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: def };
                    ic.initialize_client(&ifmt, Direction::Capture, mode, None).map_err(es)?;
                }
                let ev = ic.set_get_eventhandle().map_err(es)?;
                let cap = ic.get_audiocaptureclient().map_err(es)?;
                Ok(In { client: ic, capture: cap, _event: ev, fmt: Fmt::of(&ifmt) })
            };
            match open() {
                Ok(i) => Some(i),
                Err(e) if low => return Err(e),
                Err(_) => None,
            }
        }
    };
    let event = oc.set_get_eventhandle().map_err(es)?;
    let render = oc.get_audiorenderclient().map_err(es)?;
    Ok(Streams {
        out: Out { client: oc, render, event, fmt: Fmt::of(&ofmt) },
        inp,
        sr,
        exclusive: false,
        mode: if low { AudioMode::SharedLowLatency } else { AudioMode::Shared },
    })
}

/// Converts one little-endian device sample to f32.
#[inline]
fn decode(b: &[u8], f: &Fmt) -> f32 {
    match (f.float, f.bytes) {
        (true, 4) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        (false, 2) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
        (false, 3) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8388608.0,
        (false, 4) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2147483648.0,
        _ => 0.0,
    }
}

/// Converts f32 to one little-endian device sample.
#[inline]
fn encode(v: f32, b: &mut [u8], f: &Fmt) {
    let v = v.clamp(-1.0, 1.0);
    match (f.float, f.bytes) {
        (true, 4) => b[..4].copy_from_slice(&v.to_le_bytes()),
        (false, 2) => b[..2].copy_from_slice(&((v * 32767.0) as i16).to_le_bytes()),
        (false, 3) => {
            let i = (v * 8388607.0) as i32;
            b[..3].copy_from_slice(&i.to_le_bytes()[..3]);
        }
        (false, 4) => b[..4].copy_from_slice(&((v as f64 * 2147483647.0) as i32).to_le_bytes()),
        _ => {}
    }
}

#[link(name = "avrt")]
extern "system" {
    fn AvSetMmThreadCharacteristicsW(task: *const u16, index: *mut u32) -> isize;
}

/// Body of the audio thread: opens the streams and runs the callback loop.
fn run(cfg: &AudioConfig, engine: &Arc<Mutex<Engine>>, status: &Arc<Mutex<AudioStatus>>, stop: &Arc<AtomicBool>) -> Result<(), String> {
    let en = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let out_dev = pick(&en, Direction::Render, &cfg.out_name)
        .map_err(|e| trf!("No output device found ({})", "Kein Ausgabegerät gefunden ({})", e))?;
    let in_dev = pick(&en, Direction::Capture, &cfg.in_name).ok();
    let out_name = out_dev.get_friendlyname().unwrap_or_default();
    let in_name = in_dev.as_ref().and_then(|d| d.get_friendlyname().ok()).unwrap_or_default();

    let mut notes = vec![];
    let mut streams = None;
    if cfg.low_latency {
        match open_exclusive(&out_dev, in_dev.as_ref(), cfg.period_ms) {
            Ok(s) => streams = Some(s),
            Err(e) => notes.push(trf!("Exclusive mode not available: {}", "Exklusiv-Modus nicht möglich: {}", e)),
        }
        if streams.is_none() {
            match open_shared(&out_dev, in_dev.as_ref(), true) {
                Ok(s) => streams = Some(s),
                Err(e) => notes.push(trf!("Small buffer not available: {}", "Kleiner Puffer nicht möglich: {}", e)),
            }
        }
    }
    let s = match streams {
        Some(s) => s,
        None => open_shared(&out_dev, in_dev.as_ref(), false)
            .map_err(|e| trf!("Could not open the audio device: {}", "Audiogerät konnte nicht geöffnet werden: {}", e))?,
    };
    if in_dev.is_some() && s.inp.is_none() {
        notes.push(
            tr(
                "Could not open the input - recording voice/guitar is not possible.",
                "Eingang konnte nicht geöffnet werden – Aufnahme von Stimme/Gitarre nicht möglich.",
            )
            .into(),
        );
    }
    if in_dev.is_none() {
        notes.push(tr("No recording device found.", "Kein Aufnahmegerät gefunden.").into());
    }

    let out_buf = s.out.client.get_buffer_size().map_err(|e| e.to_string())? as usize;
    let in_buf = s.inp.as_ref().and_then(|i| i.client.get_buffer_size().ok()).unwrap_or(0) as usize;
    let period = if s.exclusive {
        out_buf
    } else {
        s.out.client.get_current_shared_mode_engine_period().map(|p| p.1 as usize).unwrap_or(s.sr as usize / 100)
    };
    let in_period = if s.exclusive { in_buf } else { period };
    // Channels handed to the engine per frame: at least two (a mono
    // microphone shows up on both), at most `MAX_INPUTS`.
    let ic = s.inp.as_ref().map_or(2, |i| i.fmt.ch.clamp(2, MAX_INPUTS));
    {
        let mut e = engine.lock();
        e.set_sample_rate(s.sr as f32);
        e.in_channels = ic;
    }
    {
        let mut st = status.lock();
        st.running = true;
        st.error = None;
        st.mode = s.mode;
        st.out_name = out_name;
        st.in_name = if s.inp.is_some() { in_name } else { String::new() };
        st.sr = s.sr;
        st.in_channels = if s.inp.is_some() { ic as u32 } else { 0 };
        st.period_frames = period as u32;
        // output buffer + input buffer + some headroom for converters/USB
        let base = if s.exclusive { 2 * period + in_period } else { 3 * period + in_period };
        st.latency_ms = base as f32 * 1000.0 / s.sr as f32 + 2.0;
        st.notes = notes;
    }

    // Ask Windows to schedule this thread with pro-audio priority (MMCSS).
    // SAFETY: `name` is a valid NUL-terminated UTF-16 string that outlives the call.
    unsafe {
        let name: Vec<u16> = "Pro Audio\0".encode_utf16().collect();
        let mut idx = 0u32;
        AvSetMmThreadCharacteristicsW(name.as_ptr(), &mut idx);
    }

    let max_frames = out_buf.max(period).max(4096);
    let of = s.out.fmt;
    let mut in_bytes = vec![0u8; s.inp.as_ref().map(|i| (in_buf.max(8192)) * i.fmt.block()).unwrap_or(0)];
    let mut fifo: VecDeque<f32> = VecDeque::with_capacity(s.sr as usize * ic);
    let mut in_f = vec![0.0f32; max_frames * ic];
    let mut out_f = vec![0.0f32; max_frames * 2];
    let mut out_bytes = vec![0u8; max_frames * of.block()];

    if let Some(i) = &s.inp {
        i.client.start_stream().map_err(|e| e.to_string())?;
    }
    if s.exclusive {
        // the first buffer must be filled before the stream starts
        s.out.render.write_to_device(&out_bytes[..out_buf * of.block()], None).map_err(|e| e.to_string())?;
    }
    s.out.client.start_stream().map_err(|e| e.to_string())?;

    let mut timeouts = 0;
    let mut cpu = 0.0f32;
    let mut tick = 0u32;
    while !stop.load(Ordering::Relaxed) {
        if s.out.event.wait_for_event(300).is_err() {
            timeouts += 1;
            if timeouts > 10 {
                return Err(
                    tr("The audio device stopped responding (unplugged?).", "Das Audiogerät antwortet nicht mehr (abgezogen?).").into()
                );
            }
            continue;
        }
        timeouts = 0;
        let frames = if s.exclusive { out_buf } else { s.out.client.get_available_space_in_frames().map_err(|e| e.to_string())? as usize }
            .min(max_frames);
        if frames == 0 {
            continue;
        }
        let t0 = Instant::now();
        if let Some(i) = &s.inp {
            let f = i.fmt;
            loop {
                match i.capture.read_from_device(&mut in_bytes) {
                    Ok((0, _)) | Err(_) => break,
                    Ok((n, _)) => {
                        for fr in in_bytes[..n as usize * f.block()].chunks_exact(f.block()) {
                            for c in 0..ic {
                                let src = c.min(f.ch - 1);
                                fifo.push_back(decode(&fr[src * f.bytes..(src + 1) * f.bytes], &f));
                            }
                        }
                    }
                }
            }
        }
        // keep the input FIFO short so that the latency does not grow
        let have = fifo.len() / ic;
        if have > frames + 2 * in_period.max(frames) {
            let drop = have - frames;
            fifo.drain(..drop * ic);
        }
        let have = (fifo.len() / ic).min(frames);
        let pad = frames - have;
        in_f[..pad * ic].fill(0.0);
        for v in in_f[pad * ic..frames * ic].iter_mut() {
            *v = fifo.pop_front().unwrap_or(0.0);
        }
        engine.lock().process(&in_f[..frames * ic], &mut out_f[..frames * 2]);
        let blk = of.block();
        for (fr, o) in out_bytes[..frames * blk].chunks_exact_mut(blk).zip(out_f.as_chunks::<2>().0) {
            encode(o[0], &mut fr[..of.bytes], &of);
            if of.ch > 1 {
                encode(o[1], &mut fr[of.bytes..2 * of.bytes], &of);
            }
            for c in 2..of.ch {
                fr[c * of.bytes..(c + 1) * of.bytes].fill(0);
            }
        }
        s.out
            .render
            .write_to_device(&out_bytes[..frames * blk], None)
            .map_err(|e| trf!("Audio output interrupted: {}", "Audio-Ausgabe unterbrochen: {}", e))?;
        let used = t0.elapsed().as_secs_f32() / (frames as f32 / s.sr as f32);
        cpu = cpu.max(used) * 0.999;
        tick += 1;
        if tick.is_multiple_of(32) {
            if let Some(mut st) = status.try_lock() {
                st.cpu = cpu;
            }
        }
    }
    let _ = s.out.client.stop_stream();
    if let Some(i) = &s.inp {
        let _ = i.client.stop_stream();
    }
    Ok(())
}
