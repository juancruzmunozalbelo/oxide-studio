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

/// Dynamic, arbitrary-length Ableton Live 12 style Device Chain.
///
/// Executes in strict ordered sequence:
/// `[MIDI Effects]* -> [Instrument]? -> [Audio Effects]*`
pub struct DeviceChain {
    pub name: String,
    pub enabled: bool,
    pub midi_effects: Vec<Box<dyn MidiEffectDevice>>,
    pub instrument: Option<Box<dyn InstrumentDevice>>,
    pub audio_effects: Vec<Box<dyn AudioEffectDevice>>,
}

impl DeviceChain {
    /// Create a new empty device chain.
    pub fn new() -> Self {
        Self {
            name: "Device Chain".to_string(),
            enabled: true,
            midi_effects: Vec::new(),
            instrument: None,
            audio_effects: Vec::new(),
        }
    }

    /// Create a named device chain.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            midi_effects: Vec::new(),
            instrument: None,
            audio_effects: Vec::new(),
        }
    }

    /// Total count of all devices in this chain.
    pub fn device_count(&self) -> usize {
        self.midi_effects.len()
            + if self.instrument.is_some() { 1 } else { 0 }
            + self.audio_effects.len()
    }

    /// Count of MIDI effects in the chain.
    pub fn midi_effect_count(&self) -> usize {
        self.midi_effects.len()
    }

    /// Count of audio effects in the chain.
    pub fn audio_effect_count(&self) -> usize {
        self.audio_effects.len()
    }

    /// Whether an instrument is present in this chain.
    pub fn has_instrument(&self) -> bool {
        self.instrument.is_some()
    }

    /// Add a MIDI effect to the end of the MIDI processing stage.
    pub fn add_midi_effect(&mut self, effect: Box<dyn MidiEffectDevice>) -> usize {
        let idx = self.midi_effects.len();
        self.midi_effects.push(effect);
        idx
    }

    /// Insert a MIDI effect at a specific index.
    pub fn insert_midi_effect(&mut self, index: usize, effect: Box<dyn MidiEffectDevice>) {
        if index <= self.midi_effects.len() {
            self.midi_effects.insert(index, effect);
        } else {
            self.midi_effects.push(effect);
        }
    }

    /// Remove a MIDI effect at an index.
    pub fn remove_midi_effect(&mut self, index: usize) -> Option<Box<dyn MidiEffectDevice>> {
        if index < self.midi_effects.len() {
            Some(self.midi_effects.remove(index))
        } else {
            None
        }
    }

    /// Reorder a MIDI effect from one index to another.
    pub fn reorder_midi_effects(&mut self, from: usize, to: usize) -> bool {
        if from < self.midi_effects.len() && to < self.midi_effects.len() {
            let effect = self.midi_effects.remove(from);
            self.midi_effects.insert(to, effect);
            true
        } else {
            false
        }
    }

    /// Set or replace the instrument device. Returns the previous instrument if any.
    pub fn set_instrument(
        &mut self,
        instrument: Option<Box<dyn InstrumentDevice>>,
    ) -> Option<Box<dyn InstrumentDevice>> {
        std::mem::replace(&mut self.instrument, instrument)
    }

    /// Add an audio effect to the end of the audio processing stage.
    pub fn add_audio_effect(&mut self, effect: Box<dyn AudioEffectDevice>) -> usize {
        let idx = self.audio_effects.len();
        self.audio_effects.push(effect);
        idx
    }

    /// Insert an audio effect at a specific index.
    pub fn insert_audio_effect(&mut self, index: usize, effect: Box<dyn AudioEffectDevice>) {
        if index <= self.audio_effects.len() {
            self.audio_effects.insert(index, effect);
        } else {
            self.audio_effects.push(effect);
        }
    }

    /// Remove an audio effect at an index.
    pub fn remove_audio_effect(&mut self, index: usize) -> Option<Box<dyn AudioEffectDevice>> {
        if index < self.audio_effects.len() {
            Some(self.audio_effects.remove(index))
        } else {
            None
        }
    }

    /// Reorder an audio effect from one index to another.
    pub fn reorder_audio_effects(&mut self, from: usize, to: usize) -> bool {
        if from < self.audio_effects.len() && to < self.audio_effects.len() {
            let effect = self.audio_effects.remove(from);
            self.audio_effects.insert(to, effect);
            true
        } else {
            false
        }
    }

    /// Bypass or enable a device by category and index.
    pub fn set_bypass(&mut self, category: DeviceCategory, index: usize, bypassed: bool) -> bool {
        let enabled = !bypassed;
        match category {
            DeviceCategory::MidiEffect => {
                if let Some(dev) = self.midi_effects.get_mut(index) {
                    dev.set_enabled(enabled);
                    true
                } else {
                    false
                }
            }
            DeviceCategory::Instrument => {
                if let Some(ref mut inst) = self.instrument {
                    inst.set_enabled(enabled);
                    true
                } else {
                    false
                }
            }
            DeviceCategory::AudioEffect => {
                if let Some(dev) = self.audio_effects.get_mut(index) {
                    dev.set_enabled(enabled);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Total latency in samples across all enabled devices in this chain.
    pub fn total_latency_samples(&self) -> usize {
        let mut total = 0;
        for dev in &self.midi_effects {
            if dev.is_enabled() {
                total += dev.latency_samples();
            }
        }
        if let Some(ref inst) = self.instrument {
            if inst.is_enabled() {
                total += inst.latency_samples();
            }
        }
        for dev in &self.audio_effects {
            if dev.is_enabled() {
                total += dev.latency_samples();
            }
        }
        total
    }

    /// Execute the full device chain:
    /// 1. Route `midi_in` through all enabled `midi_effects` in order.
    /// 2. Route resulting MIDI through `instrument` (if present and enabled) into `audio_buffer`.
    /// 3. Route `audio_buffer` through all enabled `audio_effects` in order.
    pub fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_buffer: &mut AudioBuffer,
        time: &MusicalTime,
    ) {
        if !self.enabled {
            return;
        }

        // Stage 1: MIDI Effects
        let mut current_midi: Vec<MidiEvent> = midi_in.to_vec();
        for mfx in &mut self.midi_effects {
            if mfx.is_enabled() {
                let mut next_midi = Vec::new();
                mfx.process_midi(&current_midi, &mut next_midi, time);
                current_midi = next_midi;
            }
        }

        // Stage 2: Instrument
        if let Some(ref mut inst) = self.instrument {
            if inst.is_enabled() {
                inst.process(&current_midi, audio_buffer, time);
            }
        }

        // Stage 3: Audio Effects
        for afx in &mut self.audio_effects {
            if afx.is_enabled() {
                afx.process_audio(audio_buffer, time);
            }
        }
    }
}

impl Default for DeviceChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for DeviceChain {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> DeviceCategory {
        if self.instrument.is_some() {
            DeviceCategory::Instrument
        } else {
            DeviceCategory::AudioEffect
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self) {
        for mfx in &mut self.midi_effects {
            mfx.reset();
        }
        if let Some(ref mut inst) = self.instrument {
            inst.reset();
        }
        for afx in &mut self.audio_effects {
            afx.reset();
        }
    }

    fn latency_samples(&self) -> usize {
        self.total_latency_samples()
    }
}

impl InstrumentDevice for DeviceChain {
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        time: &MusicalTime,
    ) {
        self.process(midi_in, audio_out, time);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if let Some(ref mut inst) = self.instrument {
            inst.note_on(note, velocity);
        }
    }

    fn note_off(&mut self, note: u8) {
        if let Some(ref mut inst) = self.instrument {
            inst.note_off(note);
        }
    }

    fn all_notes_off(&mut self) {
        if let Some(ref mut inst) = self.instrument {
            inst.all_notes_off();
        }
    }
}

impl AudioEffectDevice for DeviceChain {
    fn process_audio(
        &mut self,
        buffer: &mut AudioBuffer,
        time: &MusicalTime,
    ) {
        self.process(&[], buffer, time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyMidiTransposer {
        enabled: bool,
        shift: i8,
    }

    impl Device for DummyMidiTransposer {
        fn name(&self) -> &str {
            "DummyTransposer"
        }
        fn category(&self) -> DeviceCategory {
            DeviceCategory::MidiEffect
        }
        fn is_enabled(&self) -> bool {
            self.enabled
        }
        fn set_enabled(&mut self, enabled: bool) {
            self.enabled = enabled;
        }
        fn reset(&mut self) {}
    }

    impl MidiEffectDevice for DummyMidiTransposer {
        fn process_midi(
            &mut self,
            events_in: &[MidiEvent],
            events_out: &mut Vec<MidiEvent>,
            _time: &MusicalTime,
        ) {
            for ev in events_in {
                match ev {
                    MidiEvent::NoteOn { channel, note, velocity } => {
                        let new_note = (*note as i16 + self.shift as i16).clamp(0, 127) as u8;
                        events_out.push(MidiEvent::NoteOn {
                            channel: *channel,
                            note: new_note,
                            velocity: *velocity,
                        });
                    }
                    _ => events_out.push(*ev),
                }
            }
        }
    }

    struct DummyOscillator {
        enabled: bool,
        last_note: u8,
    }

    impl Device for DummyOscillator {
        fn name(&self) -> &str {
            "DummyOsc"
        }
        fn category(&self) -> DeviceCategory {
            DeviceCategory::Instrument
        }
        fn is_enabled(&self) -> bool {
            self.enabled
        }
        fn set_enabled(&mut self, enabled: bool) {
            self.enabled = enabled;
        }
        fn reset(&mut self) {}
    }

    impl InstrumentDevice for DummyOscillator {
        fn process(
            &mut self,
            midi_in: &[MidiEvent],
            audio_out: &mut AudioBuffer,
            _time: &MusicalTime,
        ) {
            for ev in midi_in {
                if let MidiEvent::NoteOn { note, .. } = ev {
                    self.last_note = *note;
                }
            }
            let val = self.last_note as f32 / 127.0;
            for i in 0..audio_out.frame_count.min(1024) {
                audio_out.left[i] = val;
                audio_out.right[i] = val;
            }
        }
    }

    struct DummyGain {
        enabled: bool,
        gain: f32,
    }

    impl Device for DummyGain {
        fn name(&self) -> &str {
            "DummyGain"
        }
        fn category(&self) -> DeviceCategory {
            DeviceCategory::AudioEffect
        }
        fn is_enabled(&self) -> bool {
            self.enabled
        }
        fn set_enabled(&mut self, enabled: bool) {
            self.enabled = enabled;
        }
        fn reset(&mut self) {}
    }

    impl AudioEffectDevice for DummyGain {
        fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
            buffer.apply_gain(self.gain);
        }
    }

    #[test]
    fn test_device_chain_ordered_execution() {
        let mut chain = DeviceChain::new();
        // Add MIDI effect shifting by +12 semitones
        chain.add_midi_effect(Box::new(DummyMidiTransposer {
            enabled: true,
            shift: 12,
        }));
        // Add instrument
        chain.set_instrument(Some(Box::new(DummyOscillator {
            enabled: true,
            last_note: 0,
        })));
        // Add Audio Effect scaling gain by 0.5
        chain.add_audio_effect(Box::new(DummyGain {
            enabled: true,
            gain: 0.5,
        }));

        let midi_in = vec![MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        }];
        let mut buf = AudioBuffer::silence(64);
        let time = MusicalTime::default();

        chain.process(&midi_in, &mut buf, &time);

        // Expected: note 60 + 12 = 72. 72 / 127.0 = 0.566929...
        // Scaled by 0.5 = 0.283464...
        let expected = (72.0 / 127.0) * 0.5;
        assert!((buf.left[0] - expected).abs() < 1e-4);
        assert!((buf.right[0] - expected).abs() < 1e-4);
    }

    #[test]
    fn test_device_chain_reorder_and_bypass() {
        let mut chain = DeviceChain::new();
        let idx0 = chain.add_audio_effect(Box::new(DummyGain { enabled: true, gain: 2.0 }));
        let idx1 = chain.add_audio_effect(Box::new(DummyGain { enabled: true, gain: 3.0 }));
        assert_eq!(chain.audio_effect_count(), 2);

        // Bypass second effect
        chain.set_bypass(DeviceCategory::AudioEffect, idx1, true);
        assert!(!chain.audio_effects[idx1].is_enabled());

        let mut buf = AudioBuffer::silence(10);
        buf.left[0] = 1.0;
        buf.right[0] = 1.0;
        let time = MusicalTime::default();

        chain.process(&[], &mut buf, &time);

        // Only first effect (gain 2.0) applied
        assert!((buf.left[0] - 2.0).abs() < 1e-4);

        // Reorder
        assert!(chain.reorder_audio_effects(idx0, idx1));
    }
}

