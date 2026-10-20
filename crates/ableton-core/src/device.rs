//! Unified device abstractions for Ableton Live 12 style device chains.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{AudioBuffer, MidiEvent, MusicalTime};

/// Device category distinguishing MIDI processors, audio generators, and audio effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DeviceCategory {
    MidiEffect,
    Instrument,
    AudioEffect,
}

/// Base device trait common to all devices in an Ableton device chain.
pub trait Device: Send + Sync {
    /// Human-readable device name (e.g. "Wavetable", "Operator", "EQ Eight").
    fn name(&self) -> &str;

    /// Device classification category.
    fn category(&self) -> DeviceCategory;

    /// Whether this device is active (unbypassed).
    fn is_enabled(&self) -> bool;

    /// Enable or bypass this device.
    fn set_enabled(&mut self, enabled: bool);

    /// Reset internal state, delay lines, and active voices.
    fn reset(&mut self);

    /// Internal processing latency in samples (default: 0).
    fn latency_samples(&self) -> usize {
        0
    }
}

/// MIDI processing device (Arpeggiator, Chord, Scale, Note Length, etc.).
pub trait MidiEffectDevice: Device {
    fn process_midi(
        &mut self,
        events_in: &[MidiEvent],
        events_out: &mut Vec<MidiEvent>,
        time: &MusicalTime,
    );
}

/// Sound-generating instrument device (Wavetable, Operator, Drift, Meld, Analog, Simpler, etc.).
pub trait InstrumentDevice: Device {
    /// Render stereo audio from incoming MIDI events.
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        time: &MusicalTime,
    );

    /// Helper for direct NoteOn event.
    fn note_on(&mut self, _note: u8, _velocity: u8) {}

    /// Helper for direct NoteOff event.
    fn note_off(&mut self, _note: u8) {}

    /// Helper to silence all active voices.
    fn all_notes_off(&mut self) {}

    /// Set a device parameter by integer ID.
    fn set_parameter(&mut self, _param_id: u32, _value: f32) {}

    /// Query a device parameter value by integer ID.
    fn get_parameter(&self, _param_id: u32) -> f32 {
        0.0
    }
}

/// Audio effect processing device (EQ Eight, Roar, Glue Compressor, Reverb, etc.).
pub trait AudioEffectDevice: Device {
    fn process_audio(
        &mut self,
        buffer: &mut AudioBuffer,
        time: &MusicalTime,
    );
}
