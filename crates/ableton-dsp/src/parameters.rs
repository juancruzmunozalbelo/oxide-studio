//! Synth parameter descriptors, scaling curves, and parameter bank.
//!
//! Decompiled & reverse-engineered from Ableton Learning Synths (`src/engine/parameter-bank.ts`).

use std::collections::HashMap;

/// Mathematical scaling curves mapping normalized [0.0, 1.0] controls to domain units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScalingCurve {
    Linear,
    Exponential { exponent: f32, rnbo_max: Option<f32> },
    Log2Freq { base_hz: f32, octaves: f32 },
    Steps { count: usize },
}

/// Metadata descriptor for a single synthesizer parameter.
#[derive(Debug, Clone)]
pub struct ParameterDescriptor {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default_value: f32,
    pub scaling: ScalingCurve,
    pub is_enum: bool,
}

impl ParameterDescriptor {
    pub const fn number(
        name: &'static str,
        min: f32,
        max: f32,
        default_value: f32,
        scaling: ScalingCurve,
    ) -> Self {
        Self {
            name,
            min,
            max,
            default_value,
            scaling,
            is_enum: false,
        }
    }

    pub const fn enumeration(
        name: &'static str,
        min: f32,
        max: f32,
        default_value: f32,
        count: usize,
    ) -> Self {
        Self {
            name,
            min,
            max,
            default_value,
            scaling: ScalingCurve::Steps { count },
            is_enum: true,
        }
    }

    /// Convert a domain value to normalized [0.0, 1.0].
    pub fn to_normalized(&self, domain_val: f32) -> f32 {
        match self.scaling {
            ScalingCurve::Linear => {
                let clamped = domain_val.clamp(self.min, self.max);
                if (self.max - self.min).abs() < 1e-6 {
                    0.0
                } else {
                    (clamped - self.min) / (self.max - self.min)
                }
            }
            ScalingCurve::Exponential { exponent, rnbo_max } => {
                let max_bound = rnbo_max.unwrap_or(self.max);
                let clamped = domain_val.clamp(self.min, max_bound);
                let span = max_bound - self.min;
                if span <= 0.0 {
                    return 0.0;
                }
                let ratio = (clamped - self.min) / span;
                if ratio <= 0.0 {
                    0.0
                } else {
                    ratio.powf(1.0 / exponent)
                }
            }
            ScalingCurve::Log2Freq { base_hz, octaves } => {
                let clamped = domain_val.max(base_hz);
                ((clamped / base_hz).log2() / octaves).clamp(0.0, 1.0)
            }
            ScalingCurve::Steps { count } => {
                if count <= 1 {
                    return 0.0;
                }
                let clamped = domain_val.clamp(self.min, self.max);
                let raw = (clamped - self.min) / (self.max - self.min);
                ((raw * count as f32).floor() / ((count - 1) as f32)).clamp(0.0, 1.0)
            }
        }
    }

    /// Convert a normalized [0.0, 1.0] control value to domain value.
    pub fn from_normalized(&self, norm: f32) -> f32 {
        let t = norm.clamp(0.0, 1.0);
        match self.scaling {
            ScalingCurve::Linear => self.min + (self.max - self.min) * t,
            ScalingCurve::Exponential { exponent, rnbo_max } => {
                let max_bound = rnbo_max.unwrap_or(self.max);
                if t <= 0.0 {
                    self.min
                } else {
                    self.min + (max_bound - self.min) * t.powf(exponent)
                }
            }
            ScalingCurve::Log2Freq { base_hz, octaves } => {
                base_hz * 2.0f32.powf(octaves * t)
            }
            ScalingCurve::Steps { count } => {
                if count <= 1 {
                    return self.min;
                }
                let stepped = ((t * count as f32).floor()).min((count - 1) as f32) / ((count - 1) as f32);
                (self.min + (self.max - self.min) * stepped).round()
            }
        }
    }
}

/// The 30 standard synth parameters in Ableton Learning Synths.
pub const PARAMETER_COUNT: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum ParamId {
    OscillatorNoiseAmplitude = 0,
    OscillatorPulseAmplitude = 1,
    OscillatorPulseWidth = 2,
    OscillatorSawAmplitude = 3,
    OscillatorSawDetuneCoarse = 4,
    OscillatorSawDetuneFine = 5,
    AmplitudeEnvelopeAttack = 6,
    AmplitudeEnvelopeDecay = 7,
    AmplitudeEnvelopeSustain = 8,
    AmplitudeEnvelopeRelease = 9,
    ModulationEnvelopeAttack = 10,
    ModulationEnvelopeDecay = 11,
    ModulationEnvelopeSustain = 12,
    ModulationEnvelopeRelease = 13,
    FilterEnvelopeAmount = 14,
    FilterFrequency = 15,
    FilterResonance = 16,
    LfoFrequency = 17,
    LfoFrequencyEnvelopeAmount = 18,
    LfoShape = 19,
    AmplitudeLfoAmount = 20,
    OscillatorPulseWidthLfoAmount = 21,
    OscillatorPulseWidthEnvelopeAmount = 22,
    FilterLfoAmount = 23,
    OscillatorPitchOffsetLfoAmount = 24,
    OscillatorPitchOffsetEnvelopeAmount = 25,
    OscillatorPitchOffset = 26,
    GlobalSlewTime = 27,
    GlideTime = 28,
    Amplitude = 29,
}

pub static PARAMETER_DESCRIPTORS: &[ParameterDescriptor] = &[
    ParameterDescriptor::number("oscillator_noise_amplitude", 0.0, 1.0, 0.0, ScalingCurve::Exponential { exponent: 0.8, rnbo_max: None }),
    ParameterDescriptor::number("oscillator_pulse_amplitude", 0.0, 1.0, 1.0, ScalingCurve::Exponential { exponent: 0.8, rnbo_max: None }),
    ParameterDescriptor::number("oscillator_pulse_width", -1.0, 1.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("oscillator_saw_amplitude", 0.0, 1.0, 0.0, ScalingCurve::Exponential { exponent: 0.8, rnbo_max: None }),
    ParameterDescriptor::number("oscillator_saw_detune_coarse", -48.0, 48.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("oscillator_saw_detune_fine", -100.0, 100.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("amplitude_envelope_attack", 0.0, 5000.0, 30.0, ScalingCurve::Exponential { exponent: 3.0, rnbo_max: None }),
    ParameterDescriptor::number("amplitude_envelope_decay", 1.0, 5000.0, 200.0, ScalingCurve::Exponential { exponent: 3.0, rnbo_max: None }),
    ParameterDescriptor::number("amplitude_envelope_sustain", 0.0, 1.0, 0.5, ScalingCurve::Exponential { exponent: 0.8, rnbo_max: None }),
    ParameterDescriptor::number("amplitude_envelope_release", 1.0, 20000.0, 300.0, ScalingCurve::Exponential { exponent: 5.0, rnbo_max: Some(90000.0) }),
    ParameterDescriptor::number("modulation_envelope_attack", 0.0, 5000.0, 100.0, ScalingCurve::Exponential { exponent: 3.0, rnbo_max: None }),
    ParameterDescriptor::number("modulation_envelope_decay", 1.0, 5000.0, 500.0, ScalingCurve::Exponential { exponent: 3.0, rnbo_max: None }),
    ParameterDescriptor::number("modulation_envelope_sustain", 0.0, 1.0, 0.5, ScalingCurve::Exponential { exponent: 0.8, rnbo_max: None }),
    ParameterDescriptor::number("modulation_envelope_release", 1.0, 20000.0, 150.0, ScalingCurve::Exponential { exponent: 5.0, rnbo_max: Some(90000.0) }),
    ParameterDescriptor::number("filter_envelope_amount", -1.0, 1.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("filter_frequency", 20.0, 20000.0, 3000.0, ScalingCurve::Log2Freq { base_hz: 20.0, octaves: 10.0 }),
    ParameterDescriptor::number("filter_resonance", 0.0, 1.0, 0.2, ScalingCurve::Linear),
    ParameterDescriptor::number("lfo_frequency", 0.1, 1000.0, 1.0, ScalingCurve::Exponential { exponent: 8.0, rnbo_max: None }),
    ParameterDescriptor::number("lfo_frequency_envelope_amount", -1.0, 1.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::enumeration("lfo_shape", 0.0, 3.0, 0.0, 4),
    ParameterDescriptor::number("amplitude_lfo_amount", -1.0, 1.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("oscillator_pulse_width_lfo_amount", -1.0, 1.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("oscillator_pulse_width_envelope_amount", -1.0, 1.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("filter_lfo_amount", -1.0, 1.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("oscillator_pitch_offset_lfo_amount", -48.0, 48.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("oscillator_pitch_offset_envelope_amount", -48.0, 48.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("oscillator_pitch_offset", -48.0, 48.0, 0.0, ScalingCurve::Linear),
    ParameterDescriptor::number("global_slew_time", 0.0, 5000.0, 50.0, ScalingCurve::Linear),
    ParameterDescriptor::number("glide_time", 0.0, 5000.0, 0.0, ScalingCurve::Exponential { exponent: 3.0, rnbo_max: None }),
    ParameterDescriptor::number("amplitude", 0.0, 1.0, 1.0, ScalingCurve::Linear),
];

/// Parameter bank holding current runtime values for the synth.
///
/// Uses a fixed-size `[f32; 32]` array with zero heap allocations during steady-state rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterBank {
    pub values: [f32; 32],
}

impl ParameterBank {
    pub fn new() -> Self {
        let mut values = [0.0f32; 32];
        for (i, desc) in PARAMETER_DESCRIPTORS.iter().enumerate() {
            values[i] = desc.default_value;
        }
        Self { values }
    }

    pub fn reset_to_defaults(&mut self) {
        for (i, desc) in PARAMETER_DESCRIPTORS.iter().enumerate() {
            self.values[i] = desc.default_value;
        }
    }

    pub fn get_descriptor(name: &str) -> Option<&'static ParameterDescriptor> {
        PARAMETER_DESCRIPTORS.iter().find(|d| d.name == name)
    }

    #[inline(always)]
    pub fn get_index_by_name(name: &str) -> Option<usize> {
        match name {
            "oscillator_noise_amplitude" => Some(0),
            "oscillator_pulse_amplitude" => Some(1),
            "oscillator_pulse_width" => Some(2),
            "oscillator_saw_amplitude" => Some(3),
            "oscillator_saw_detune_coarse" => Some(4),
            "oscillator_saw_detune_fine" => Some(5),
            "amplitude_envelope_attack" => Some(6),
            "amplitude_envelope_decay" => Some(7),
            "amplitude_envelope_sustain" => Some(8),
            "amplitude_envelope_release" => Some(9),
            "modulation_envelope_attack" => Some(10),
            "modulation_envelope_decay" => Some(11),
            "modulation_envelope_sustain" => Some(12),
            "modulation_envelope_release" => Some(13),
            "filter_envelope_amount" => Some(14),
            "filter_frequency" => Some(15),
            "filter_resonance" => Some(16),
            "lfo_frequency" => Some(17),
            "lfo_frequency_envelope_amount" => Some(18),
            "lfo_shape" => Some(19),
            "amplitude_lfo_amount" => Some(20),
            "oscillator_pulse_width_lfo_amount" => Some(21),
            "oscillator_pulse_width_envelope_amount" => Some(22),
            "filter_lfo_amount" => Some(23),
            "oscillator_pitch_offset_lfo_amount" => Some(24),
            "oscillator_pitch_offset_envelope_amount" => Some(25),
            "oscillator_pitch_offset" => Some(26),
            "global_slew_time" => Some(27),
            "glide_time" => Some(28),
            "amplitude" => Some(29),
            _ => None,
        }
    }

    #[inline]
    pub fn get_value(&self, name: &str) -> f32 {
        if let Some(idx) = Self::get_index_by_name(name) {
            self.values[idx]
        } else {
            Self::get_descriptor(name).map(|d| d.default_value).unwrap_or(0.0)
        }
    }

    #[inline]
    pub fn set_value(&mut self, name: &str, value: f32) {
        if let Some(idx) = Self::get_index_by_name(name) {
            let desc = &PARAMETER_DESCRIPTORS[idx];
            let clamped = if desc.is_enum {
                value.round().clamp(desc.min, desc.max)
            } else {
                value.clamp(desc.min, desc.max)
            };
            self.values[idx] = clamped;
        }
    }

    #[inline(always)]
    pub fn get(&self, param: ParamId) -> f32 {
        self.values[param as usize]
    }

    #[inline(always)]
    pub fn set(&mut self, param: ParamId, value: f32) {
        self.set_by_index(param as usize, value);
    }

    #[inline(always)]
    pub fn get_by_index(&self, idx: usize) -> f32 {
        if idx < self.values.len() {
            self.values[idx]
        } else {
            0.0
        }
    }

    #[inline(always)]
    pub fn set_by_index(&mut self, idx: usize, value: f32) {
        if idx < PARAMETER_DESCRIPTORS.len() {
            let desc = &PARAMETER_DESCRIPTORS[idx];
            let clamped = if desc.is_enum {
                value.round().clamp(desc.min, desc.max)
            } else {
                value.clamp(desc.min, desc.max)
            };
            self.values[idx] = clamped;
        }
    }

    pub fn get_control_value(&self, name: &str) -> f32 {
        if let Some(desc) = Self::get_descriptor(name) {
            let val = self.get_value(name);
            desc.to_normalized(val)
        } else {
            0.0
        }
    }

    pub fn set_control_value(&mut self, name: &str, norm: f32) {
        if let Some(desc) = Self::get_descriptor(name) {
            let domain_val = desc.from_normalized(norm);
            self.set_value(name, domain_val);
        }
    }

    pub fn all_values(&self) -> HashMap<String, f32> {
        let mut map = HashMap::with_capacity(PARAMETER_COUNT);
        for (i, desc) in PARAMETER_DESCRIPTORS.iter().enumerate() {
            map.insert(desc.name.to_string(), self.values[i]);
        }
        map
    }
}

impl Default for ParameterBank {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_30_descriptors_present() {
        assert_eq!(PARAMETER_DESCRIPTORS.len(), 30);
        let bank = ParameterBank::new();
        for desc in PARAMETER_DESCRIPTORS {
            assert_eq!(bank.get_value(desc.name), desc.default_value);
        }
    }

    #[test]
    fn test_log2_freq_scaling() {
        let desc = ParameterBank::get_descriptor("filter_frequency").unwrap();
        // At norm 0.0: 20 Hz
        assert!((desc.from_normalized(0.0) - 20.0).abs() < 1e-3);
        // At norm 1.0: 20 * 2^10 = 20480 Hz
        assert!((desc.from_normalized(1.0) - 20480.0).abs() < 1e-1);

        // Roundtrip check at 1000 Hz
        let norm = desc.to_normalized(1000.0);
        let back = desc.from_normalized(norm);
        assert!((back - 1000.0).abs() < 1e-2);
    }
}
