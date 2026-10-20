//! 4-shape Low Frequency Oscillator (LFO) with envelope modulation and sample & hold.
//!
//! Decompiled & reverse-engineered from Ableton Learning Synths (`src/engine/dsp-core.ts`, lines 444-470).

use crate::oscillator::XorShift32;
use std::f32::consts::PI;

/// 4 LFO shapes supported by the synth engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShape {
    Sine = 0,
    SawDown = 1,
    Square = 2,
    SampleAndHold = 3,
}

impl From<usize> for LfoShape {
    fn from(idx: usize) -> Self {
        match idx {
            1 => LfoShape::SawDown,
            2 => LfoShape::Square,
            3 => LfoShape::SampleAndHold,
            _ => LfoShape::Sine,
        }
    }
}

/// Low Frequency Oscillator.
#[derive(Debug, Clone)]
pub struct Lfo {
    pub phase: f32,
    pub sample_hold_value: f32,
    pub rng: XorShift32,
}

impl Lfo {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            sample_hold_value: 0.0,
            rng: XorShift32::new(0x13579bdf),
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.sample_hold_value = 0.0;
        self.rng.reset(0x13579bdf);
    }

    /// Advance LFO and compute the current modulation sample.
    ///
    /// - `base_freq`: Base frequency in Hz (0.05 to 100 Hz)
    /// - `env_mod`: Envelope modulation depth in [-1.0, 1.0]
    /// - `mod_env_val`: Current value of the modulation envelope [0.0, 1.0]
    /// - `shape`: Waveform shape
    /// - `sample_rate`: Audio sample rate in Hz
    ///
    /// Returns `(signal, modulated_freq)` where signal is in `[-1.0, 1.0]`.
    pub fn next_sample(
        &mut self,
        base_freq: f32,
        env_mod: f32,
        mod_env_val: f32,
        shape: LfoShape,
        sample_rate: f32,
    ) -> (f32, f32) {
        if sample_rate <= 0.0 {
            return (0.0, base_freq);
        }

        // Modulated frequency: base_freq * 2^(env_mod * mod_env_val * 4.0), clamped to [0.05, 100.0]
        let freq = (base_freq * 2.0f32.powf(env_mod * mod_env_val * 4.0)).clamp(0.05, 100.0);
        let prev_phase = self.phase;
        self.phase += freq / sample_rate;

        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.sample_hold_value = self.rng.next_f32();
        } else if prev_phase == 0.0 && self.sample_hold_value == 0.0 {
            self.sample_hold_value = self.rng.next_f32();
        }

        let signal = match shape {
            LfoShape::Sine => (self.phase * 2.0 * PI).sin(),
            LfoShape::SawDown => 1.0 - 2.0 * self.phase,
            LfoShape::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => self.sample_hold_value,
        };

        (signal, freq)
    }

    pub fn get_phase(&self) -> f32 {
        self.phase
    }
}

impl Default for Lfo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lfo_shapes() {
        let mut lfo = Lfo::new();
        // At phase 0: Sine is 0.0, SawDown is 1.0, Square is 1.0
        let (s_sin, _) = lfo.next_sample(1.0, 0.0, 0.0, LfoShape::Sine, 44100.0);
        assert!(s_sin.abs() < 0.01);

        lfo.reset();
        let (s_saw, _) = lfo.next_sample(1.0, 0.0, 0.0, LfoShape::SawDown, 44100.0);
        assert!((s_saw - 1.0).abs() < 0.01);

        lfo.reset();
        let (s_sq, _) = lfo.next_sample(1.0, 0.0, 0.0, LfoShape::Square, 44100.0);
        assert!((s_sq - 1.0).abs() < 0.01);
    }
}
