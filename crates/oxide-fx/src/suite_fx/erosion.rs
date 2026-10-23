//! Live 12 Erosion Device (Noise & Sine Carrier Modulator).

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErosionMode {
    Noise,
    WideNoise,
    Sine,
}

pub struct Erosion {
    pub name: String,
    pub enabled: bool,
    pub mode: ErosionMode,
    pub frequency_hz: f32,
    pub width: f32,  // resonance / bandwidth
    pub amount: f32, // 0.0 .. 1.0

    sample_rate: f32,
    sine_phase: f32,
    noise_filter_l: f32,
    noise_filter_r: f32,
}

impl Erosion {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        Self {
            name: "Erosion".to_string(),
            enabled: true,
            mode: ErosionMode::Noise,
            frequency_hz: 3000.0,
            width: 0.5,
            amount: 0.4,
            sample_rate: sr,
            sine_phase: 0.0,
            noise_filter_l: 0.0,
            noise_filter_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }
}

impl Default for Erosion {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for Erosion {
    fn name(&self) -> &str {
        &self.name
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

    fn reset(&mut self) {
        self.sine_phase = 0.0;
        self.noise_filter_l = 0.0;
        self.noise_filter_r = 0.0;
    }
}

impl AudioEffectDevice for Erosion {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let phase_step = 2.0 * PI * self.frequency_hz / self.sample_rate;
        let filter_coeff = (2.0 * PI * self.frequency_hz / self.sample_rate).clamp(0.01, 0.95);

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            let (carrier_l, carrier_r) = match self.mode {
                ErosionMode::Sine => {
                    let s = self.sine_phase.sin();
                    self.sine_phase = (self.sine_phase + phase_step).fract() * 2.0 * PI;
                    (s, s)
                }
                ErosionMode::Noise => {
                    let white = ((i as f32 * 314.159).sin() * 43758.545).fract() * 2.0 - 1.0;
                    self.noise_filter_l = self.noise_filter_l * (1.0 - filter_coeff) + white * filter_coeff;
                    (self.noise_filter_l, self.noise_filter_l)
                }
                ErosionMode::WideNoise => {
                    let white_l = ((i as f32 * 314.159).sin() * 43758.545).fract() * 2.0 - 1.0;
                    let white_r = ((i as f32 * 628.318).sin() * 43758.545).fract() * 2.0 - 1.0;
                    self.noise_filter_l = self.noise_filter_l * (1.0 - filter_coeff) + white_l * filter_coeff;
                    self.noise_filter_r = self.noise_filter_r * (1.0 - filter_coeff) + white_r * filter_coeff;
                    (self.noise_filter_l, self.noise_filter_r)
                }
            };

            let mod_l = dry_l * (1.0 + carrier_l * self.amount);
            let mod_r = dry_r * (1.0 + carrier_r * self.amount);

            buffer.left[i] = mod_l.clamp(-2.0, 2.0);
            buffer.right[i] = mod_r.clamp(-2.0, 2.0);
        }
    }
}
