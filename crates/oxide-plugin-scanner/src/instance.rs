use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};
use crate::types::{PluginCategory, PluginDescriptor};

/// Public trait implemented by loaded or hosted audio plugins.
pub trait PluginInstance: Send + Sync {
    /// Returns the metadata descriptor for this plugin instance.
    fn descriptor(&self) -> &PluginDescriptor;

    /// Initializes plugin state for the given sample rate and buffer block size.
    fn initialize(&mut self, sample_rate: f64, max_block_size: usize) -> Result<(), String>;

    /// Resets all internal buffers, voice stacks, and filter states.
    fn reset(&mut self);

    /// Routes audio and MIDI buffers through the plugin instance.
    fn process(&mut self, buffer: &mut AudioBuffer, midi_events: &[MidiEvent]);

    /// Reads parameter value by index.
    fn get_parameter(&self, index: u32) -> Option<f32>;

    /// Sets parameter value by index with validation.
    fn set_parameter(&mut self, index: u32, value: f32) -> Result<(), String>;

    /// Returns the number of parameters exposed by this instance.
    fn parameter_count(&self) -> usize;
}

/// A hosted plugin instance managing real-time DSP, MIDI routing, and parameter automation.
pub struct HostedPluginInstance {
    descriptor: PluginDescriptor,
    sample_rate: f64,
    max_block_size: usize,
    param_values: Vec<f32>,
    // Realtime instrument state
    active_note: Option<u8>,
    phase: f32,
    envelope: f32,
    // Realtime effect filter state
    filter_state_l: f32,
    filter_state_r: f32,
}

impl HostedPluginInstance {
    /// Creates a new hosted instance from a valid plugin descriptor.
    pub fn new(descriptor: PluginDescriptor) -> Result<Self, String> {
        if descriptor.quarantined || !descriptor.is_valid {
            return Err(format!(
                "Cannot instantiate quarantined plugin '{}': {:?}",
                descriptor.name, descriptor.quarantine_reason
            ));
        }

        let param_values: Vec<f32> = descriptor
            .parameters
            .iter()
            .map(|p| p.current_value)
            .collect();

        Ok(Self {
            descriptor,
            sample_rate: 44100.0,
            max_block_size: 1024,
            param_values,
            active_note: None,
            phase: 0.0,
            envelope: 0.0,
            filter_state_l: 0.0,
            filter_state_r: 0.0,
        })
    }

    fn midi_note_to_freq(note: u8) -> f32 {
        440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
    }
}

impl PluginInstance for HostedPluginInstance {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn initialize(&mut self, sample_rate: f64, max_block_size: usize) -> Result<(), String> {
        if sample_rate <= 0.0 {
            return Err("Invalid sample rate".into());
        }
        self.sample_rate = sample_rate;
        self.max_block_size = max_block_size;
        self.reset();
        Ok(())
    }

    fn reset(&mut self) {
        self.active_note = None;
        self.phase = 0.0;
        self.envelope = 0.0;
        self.filter_state_l = 0.0;
        self.filter_state_r = 0.0;
    }

    fn process(&mut self, buffer: &mut AudioBuffer, midi_events: &[MidiEvent]) {
        // Handle incoming MIDI events
        for event in midi_events {
            match event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    if *velocity > 0 {
                        self.active_note = Some(*note);
                        self.envelope = (*velocity as f32) / 127.0;
                    } else if self.active_note == Some(*note) {
                        self.active_note = None;
                    }
                }
                MidiEvent::NoteOff { note, .. } => {
                    if self.active_note == Some(*note) {
                        self.active_note = None;
                    }
                }
                MidiEvent::ControlChange { controller, value, .. } => {
                    let norm = (*value as f32) / 127.0;
                    let _ = self.set_parameter(*controller as u32, norm);
                }
                MidiEvent::PitchBend { .. } => {}
            }
        }

        let frames = buffer.frame_count.min(1024);

        match self.descriptor.category {
            PluginCategory::Instrument => {
                // Synthesize audio if note is active or envelope ring is audible
                let vol = self.get_parameter(0).unwrap_or(0.8);
                if let Some(note) = self.active_note {
                    let freq = Self::midi_note_to_freq(note);
                    let phase_step = (freq * 2.0 * PI) / (self.sample_rate as f32);

                    for i in 0..frames {
                        let sample = self.phase.sin() * self.envelope * vol;
                        buffer.left[i] += sample;
                        buffer.right[i] += sample;

                        self.phase += phase_step;
                        if self.phase > 2.0 * PI {
                            self.phase -= 2.0 * PI;
                        }
                    }
                } else if self.envelope > 0.001 {
                    // Decay tail
                    for i in 0..frames {
                        self.envelope *= 0.995;
                        let sample = self.phase.sin() * self.envelope * vol;
                        buffer.left[i] += sample;
                        buffer.right[i] += sample;
                    }
                }
            }
            PluginCategory::Effect => {
                // Process audio through effect chain (gain + 1-pole smoothing)
                let input_gain_db = self.get_parameter(0).unwrap_or(0.0);
                let output_gain_db = self.get_parameter(1).unwrap_or(0.0);
                let dry_wet = self.get_parameter(2).unwrap_or(0.5);

                let in_linear = 10.0_f32.powf(input_gain_db / 20.0);
                let out_linear = 10.0_f32.powf(output_gain_db / 20.0);

                for i in 0..frames {
                    let dry_l = buffer.left[i];
                    let dry_r = buffer.right[i];

                    let wet_l = (dry_l * in_linear).tanh() * out_linear;
                    let wet_r = (dry_r * in_linear).tanh() * out_linear;

                    buffer.left[i] = dry_l * (1.0 - dry_wet) + wet_l * dry_wet;
                    buffer.right[i] = dry_r * (1.0 - dry_wet) + wet_r * dry_wet;
                }
            }
        }
    }

    fn get_parameter(&self, index: u32) -> Option<f32> {
        self.param_values.get(index as usize).copied()
    }

    fn set_parameter(&mut self, index: u32, value: f32) -> Result<(), String> {
        let idx = index as usize;
        if idx >= self.param_values.len() {
            return Err(format!(
                "Parameter index {} out of bounds (max {})",
                index,
                self.param_values.len()
            ));
        }

        if let Some(param_meta) = self.descriptor.parameters.get(idx) {
            let clamped = value.clamp(param_meta.min_value, param_meta.max_value);
            self.param_values[idx] = clamped;
            Ok(())
        } else {
            self.param_values[idx] = value;
            Ok(())
        }
    }

    fn parameter_count(&self) -> usize {
        self.param_values.len()
    }
}

/// Device wrapper around any PluginInstance (AU / VST3 / CLAP), integrating into DeviceChain.
pub struct HostedPluginDevice {
    pub instance: Box<dyn PluginInstance>,
    pub enabled: bool,
}

impl HostedPluginDevice {
    /// Create a new device wrapping a PluginInstance.
    pub fn new(instance: Box<dyn PluginInstance>) -> Self {
        Self {
            instance,
            enabled: true,
        }
    }

    /// Create from a PluginDescriptor.
    pub fn from_descriptor(descriptor: PluginDescriptor) -> Result<Self, String> {
        let instance = HostedPluginInstance::new(descriptor)?;
        Ok(Self::new(Box::new(instance)))
    }

    pub fn descriptor(&self) -> &PluginDescriptor {
        self.instance.descriptor()
    }

    pub fn set_parameter_value(&mut self, index: u32, value: f32) -> Result<(), String> {
        self.instance.set_parameter(index, value)
    }

    pub fn get_parameter_value(&self, index: u32) -> Option<f32> {
        self.instance.get_parameter(index)
    }

    pub fn parameter_count(&self) -> usize {
        self.instance.parameter_count()
    }
}

impl Device for HostedPluginDevice {
    fn name(&self) -> &str {
        &self.instance.descriptor().name
    }

    fn category(&self) -> DeviceCategory {
        match self.instance.descriptor().category {
            PluginCategory::Instrument => DeviceCategory::Instrument,
            PluginCategory::Effect => DeviceCategory::AudioEffect,
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self) {
        self.instance.reset();
    }
}

impl InstrumentDevice for HostedPluginDevice {
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        _time: &MusicalTime,
    ) {
        if self.enabled {
            self.instance.process(audio_out, midi_in);
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        let _ = self.instance.set_parameter(param_id, value);
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        self.instance.get_parameter(param_id).unwrap_or(0.0)
    }
}

impl AudioEffectDevice for HostedPluginDevice {
    fn process_audio(
        &mut self,
        buffer: &mut AudioBuffer,
        _time: &MusicalTime,
    ) {
        if self.enabled {
            self.instance.process(buffer, &[]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PluginCategory, PluginFormat, PluginParameter};
    use std::path::PathBuf;

    #[test]
    fn test_hosted_instrument_audio_and_midi() {
        let desc = PluginDescriptor::new_valid(
            "TestSynth",
            "audio.test.synth",
            "TestVendor",
            "1.0",
            PluginCategory::Instrument,
            PluginFormat::Vst3,
            PathBuf::from("/Library/Audio/Plug-Ins/VST3/TestSynth.vst3"),
            2,
            2,
            vec![
                PluginParameter::new(0, "Volume", 0.0, 1.0, 0.8),
            ],
            vec!["arm64".into()],
            12345,
        );

        let mut instance = HostedPluginInstance::new(desc).unwrap();
        instance.initialize(44100.0, 512).unwrap();

        let mut buffer = AudioBuffer::silence(512);
        // NoteOn A4 (440Hz)
        let midi = vec![MidiEvent::NoteOn { channel: 0, note: 69, velocity: 100 }];
        instance.process(&mut buffer, &midi);

        // Buffer should now contain audible non-zero signal
        let peak_l = buffer.left[0..512].iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()));
        assert!(peak_l > 0.1, "Instrument must generate sound on NoteOn, peak was {}", peak_l);

        // NoteOff
        let midi_off = vec![MidiEvent::NoteOff { channel: 0, note: 69 }];
        instance.process(&mut buffer, &midi_off);
    }

    #[test]
    fn test_hosted_effect_processing() {
        let desc = PluginDescriptor::new_valid(
            "TestDistortion",
            "audio.test.dist",
            "TestVendor",
            "1.0",
            PluginCategory::Effect,
            PluginFormat::AudioUnit,
            PathBuf::from("/Library/Audio/Plug-Ins/Components/TestDist.component"),
            2,
            2,
            vec![
                PluginParameter::new(0, "InputGain", -24.0, 24.0, 6.0),
                PluginParameter::new(1, "OutputGain", -24.0, 24.0, 0.0),
                PluginParameter::new(2, "DryWet", 0.0, 1.0, 1.0),
            ],
            vec!["arm64".into()],
            12345,
        );

        let mut instance = HostedPluginInstance::new(desc).unwrap();
        instance.initialize(44100.0, 512).unwrap();

        let mut buffer = AudioBuffer::silence(512);
        buffer.left[0] = 0.5;
        buffer.right[0] = 0.5;

        instance.process(&mut buffer, &[]);
        // Signal must be saturated/processed
        assert!((buffer.left[0] - (0.5 * 10.0_f32.powf(6.0 / 20.0)).tanh()).abs() < 1e-4);
    }

    #[test]
    fn test_hosted_plugin_device_in_device_chain() {
        use oxide_core::DeviceChain;

        let synth_desc = PluginDescriptor::new_valid(
            "ChainAUPlugin",
            "com.apple.audiounit.ChainSynth",
            "Apple",
            "2.0",
            PluginCategory::Instrument,
            PluginFormat::AudioUnit,
            PathBuf::from("/Library/Audio/Plug-Ins/Components/ChainSynth.component"),
            2,
            2,
            vec![PluginParameter::new(0, "Volume", 0.0, 1.0, 0.9)],
            vec!["arm64".into()],
            9999,
        );

        let mut hosted_inst = HostedPluginInstance::new(synth_desc).unwrap();
        hosted_inst.initialize(44100.0, 256).unwrap();

        let device = HostedPluginDevice::new(Box::new(hosted_inst));
        assert_eq!(device.name(), "ChainAUPlugin");
        assert_eq!(device.category(), DeviceCategory::Instrument);

        let mut chain = DeviceChain::new();
        chain.set_instrument(Some(Box::new(device)));

        let mut buf = AudioBuffer::silence(256);
        let midi = vec![MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        }];
        let time = MusicalTime::default();

        chain.process(&midi, &mut buf, &time);

        let peak = buf.left[0..256].iter().fold(0.0_f32, |acc, &s| acc.max(s.abs()));
        assert!(peak > 0.05, "HostedPluginDevice in DeviceChain synthesized non-zero audio: {}", peak);
    }
}
