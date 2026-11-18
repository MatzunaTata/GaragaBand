//! MIDI input: connects automatically to every attached device
//! (e.g. Akai MPK mini) and notices plugging/unplugging on its own.

use crate::engine::MidiEvent;
use crossbeam_channel::Sender;
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::time::Instant;

pub struct MidiManager {
    tx: Sender<MidiEvent>,
    conns: Vec<MidiInputConnection<()>>,
    pub names: Vec<String>,
    seen: Vec<String>,
    last_scan: Option<Instant>,
}

impl MidiManager {
    pub fn new(tx: Sender<MidiEvent>) -> Self {
        Self { tx, conns: vec![], names: vec![], seen: vec![], last_scan: None }
    }

    /// Call regularly; rescans for added/removed devices every 2 seconds.
    pub fn poll(&mut self) {
        if self.last_scan.is_some_and(|t| t.elapsed().as_secs_f32() < 2.0) {
            return;
        }
        self.last_scan = Some(Instant::now());
        let Ok(mut probe) = MidiInput::new("GaragaBand probe") else { return };
        probe.ignore(Ignore::All);
        let names: Vec<String> = probe.ports().iter().filter_map(|p| probe.port_name(p).ok()).collect();
        if names == self.seen {
            return;
        }
        self.seen = names.clone();
        self.conns.clear();
        self.names.clear();
        for name in names {
            let Ok(mut inp) = MidiInput::new("GaragaBand") else { continue };
            inp.ignore(Ignore::All);
            let port = inp.ports().into_iter().find(|p| inp.port_name(p).ok().as_deref() == Some(&name));
            let Some(port) = port else { continue };
            let tx = self.tx.clone();
            if let Ok(c) = inp.connect(
                &port,
                "garagaband-in",
                move |_, msg, _| {
                    // forward channel messages only (no SysEx / real-time)
                    if msg.len() >= 2 && msg[0] >= 0x80 && msg[0] < 0xF0 {
                        let _ = tx.try_send(MidiEvent { status: msg[0], d1: msg[1], d2: if msg.len() > 2 { msg[2] } else { 0 } });
                    }
                },
                (),
            ) {
                self.conns.push(c);
                self.names.push(name);
            }
        }
    }
}
