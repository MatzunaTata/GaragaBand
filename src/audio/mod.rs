//! Talking to the outside world: audio devices (WASAPI) and MIDI inputs.

mod device;
mod midi;

pub use device::{list_devices, AudioConfig, AudioHandle, AudioStatus};
pub use midi::MidiManager;
