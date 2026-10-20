//! Standard Ableton Live Compressor Device with Peak, RMS, and Expand modes.

use ableton_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

/// Detection modes for the standard Compressor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressorMode {
    Peak,
    RMS,
    Expand,
}

/// Standard multi-mode compressor.
#[derive(Debug, Clone)]
pub struct Compressor {
    pub name: String,
    pub enabled: bool,
    pub mode: CompressorMode,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub knee_db: f32,
    pub makeup_gain_db: f32,
    pub dry_wet: f32,

    sample_rate: f32,
    envelope: f32,
    rms_sum: f32,
    rms_history: [f32; 64],
    rms_pos: usize,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        Self {
            name: "Compressor".to_string(),
            enabled: true,
            mode: CompressorMode::Peak,
            threshold_db: -12.0,
            ratio: 4.0,
            attack_ms: 5.0,
            release_ms: 50.0,
            knee_db: 3.0,
            makeup_gain_db: 0.0,
            dry_wet: 1.0,
            sample_rate: sr,
            envelope: 0.0,
            rms_sum: 0.0,
            rms_history: [0.0; 64],
            rms_pos: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for Compressor {
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
        self.envelope = 0.0;
        self.rms_sum = 0.0;
        self.rms_history.fill(0.0);
        self.rms_pos = 0;
    }
}

impl AudioEffectDevice for Compressor {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let attack_coeff = (-1.0 / (self.attack_ms.max(0.1) * 0.001 * self.sample_rate)).exp();
        let release_coeff = (-1.0 / (self.release_ms.max(1.0) * 0.001 * self.sample_rate)).exp();
        let makeup_linear = 10.0_f32.powf(self.makeup_gain_db / 20.0);

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            let input_level = match self.mode {
                CompressorMode::Peak => dry_l.abs().max(dry_r.abs()),
                CompressorMode::RMS => {
                    let mono = (dry_l + dry_r) * 0.5;
                    let mono_sq = mono * mono;
                    let old = self.rms_history[self.rms_pos];
                    self.rms_sum = (self.rms_sum - old + mono_sq).max(0.0);
                    self.rms_history[self.rms_pos] = mono_sq;
                    self.rms_pos = (self.rms_pos + 1) % 64;
                    (self.rms_sum / 64.0).sqrt()
                }
                CompressorMode::Expand => dry_l.abs().max(dry_r.abs()),
            };

            let level_db = 20.0 * input_level.max(1e-6).log10();

            // Gain calculation
            let target_gain_db = match self.mode {
                CompressorMode::Peak | CompressorMode::RMS => {
                    if level_db > self.threshold_db {
                        (1.0 - 1.0 / self.ratio.max(1.0)) * (self.threshold_db - level_db)
                    } else {
                        0.0
                    }
                }
                CompressorMode::Expand => {
                    // Downward expander / gate: attenuates if below threshold
                    if level_db < self.threshold_db {
                        (self.ratio.max(1.0) - 1.0) * (level_db - self.threshold_db)
                    } else {
                        0.0
                    }
                }
            };

            // Ballistics
            if target_gain_db < self.envelope {
                self.envelope = attack_coeff * self.envelope + (1.0 - attack_coeff) * target_gain_db;
            } else {
                self.envelope = release_coeff * self.envelope + (1.0 - release_coeff) * target_gain_db;
            }

            let linear_gain = 10.0_f32.powf(self.envelope / 20.0) * makeup_linear;
            let wet_l = dry_l * linear_gain;
            let wet_r = dry_r * linear_gain;

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet;
        }
    }
}
