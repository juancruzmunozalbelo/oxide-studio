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
    rng_state: u32,
    bp_s1_l: f32,
    bp_s2_l: f32,
    bp_s1_r: f32,
    bp_s2_r: f32,
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
            rng_state: 0x4552_4f53,
            bp_s1_l: 0.0,
            bp_s2_l: 0.0,
            bp_s1_r: 0.0,
            bp_s2_r: 0.0,
        }
    }

    #[inline(always)]
    fn next_noise(&mut self) -> f32 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        (x as i32 as f32) * 4.6566128730773926e-10
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate.is_finite() && sample_rate > 0.0 {
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
        self.rng_state = 0x4552_4f53;
        self.bp_s1_l = 0.0;
        self.bp_s2_l = 0.0;
        self.bp_s1_r = 0.0;
        self.bp_s2_r = 0.0;
    }
}

impl AudioEffectDevice for Erosion {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(oxide_core::MAX_BUFFER_SIZE);
        if count == 0 {
            return;
        }

        let phase_step = 2.0 * PI * self.frequency_hz / self.sample_rate;
        let fc = self.frequency_hz.clamp(20.0, (self.sample_rate * 0.45).max(20.0));
        let g = (PI * fc / self.sample_rate).tan().clamp(0.001, 10.0);
        // Bandpass width damping: narrow Q when width near 0, wide bandwidth when width near 1
        let k = 0.1 + self.width.clamp(0.0, 1.0) * 1.9;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let bp_scale = k.sqrt().clamp(0.3, 1.4);

        for i in 0..count {
            let dry_l = if buffer.left[i].is_finite() { buffer.left[i] } else { 0.0 };
            let dry_r = if buffer.right[i].is_finite() { buffer.right[i] } else { 0.0 };

            let (carrier_l, carrier_r) = match self.mode {
                ErosionMode::Sine => {
                    let s = self.sine_phase.sin();
                    self.sine_phase = (self.sine_phase + phase_step).rem_euclid(2.0 * PI);
                    (s, s)
                }
                ErosionMode::Noise => {
                    let white = self.next_noise();
                    let v3 = white - self.bp_s2_l;
                    let v1 = a1 * self.bp_s1_l + a2 * v3;
                    let v2 = self.bp_s2_l + a2 * self.bp_s1_l + a3 * v3;
                    self.bp_s1_l = (2.0 * v1 - self.bp_s1_l).clamp(-4.0, 4.0);
                    self.bp_s2_l = (2.0 * v2 - self.bp_s2_l).clamp(-4.0, 4.0);
                    let bp = (v1 * bp_scale).clamp(-1.0, 1.0);
                    (bp, bp)
                }
                ErosionMode::WideNoise => {
                    let white_l = self.next_noise();
                    let white_r = self.next_noise();
                    let v3_l = white_l - self.bp_s2_l;
                    let v1_l = a1 * self.bp_s1_l + a2 * v3_l;
                    let v2_l = self.bp_s2_l + a2 * self.bp_s1_l + a3 * v3_l;
                    self.bp_s1_l = (2.0 * v1_l - self.bp_s1_l).clamp(-4.0, 4.0);
                    self.bp_s2_l = (2.0 * v2_l - self.bp_s2_l).clamp(-4.0, 4.0);

                    let v3_r = white_r - self.bp_s2_r;
                    let v1_r = a1 * self.bp_s1_r + a2 * v3_r;
                    let v2_r = self.bp_s2_r + a2 * self.bp_s1_r + a3 * v3_r;
                    self.bp_s1_r = (2.0 * v1_r - self.bp_s1_r).clamp(-4.0, 4.0);
                    self.bp_s2_r = (2.0 * v2_r - self.bp_s2_r).clamp(-4.0, 4.0);

                    (
                        (v1_l * bp_scale).clamp(-1.0, 1.0),
                        (v1_r * bp_scale).clamp(-1.0, 1.0),
                    )
                }
            };

            let mod_l = dry_l * (1.0 + carrier_l * self.amount);
            let mod_r = dry_r * (1.0 + carrier_r * self.amount);

            buffer.left[i] = mod_l.clamp(-2.0, 2.0);
            buffer.right[i] = mod_r.clamp(-2.0, 2.0);
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.amount = value.clamp(0.0, 1.0),
            1 => self.frequency_hz = value.clamp(20.0, 20000.0),
            2 => self.width = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.amount,
            1 => self.frequency_hz,
            2 => self.width,
            _ => 0.0,
        }
    }
}
