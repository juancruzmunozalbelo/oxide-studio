//! Live 12 Chorus-Ensemble Device (3-line BBD Chorus with Warmth).

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChorusMode {
    Classic,
    Ensemble,
    Vibrato,
}

pub struct ChorusEnsemble {
    pub name: String,
    pub enabled: bool,
    pub mode: ChorusMode,
    pub rate_hz: f32,
    pub depth: f32,   // 0.0 .. 1.0
    pub warmth: f32,  // 0.0 .. 1.0 (BBD saturation and lowpass)
    pub dry_wet: f32,

    sample_rate: f32,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    lfo_phase1: f32,
    lfo_phase2: f32,
    lfo_phase3: f32,
    warm_filter_l: f32,
    warm_filter_r: f32,
}

impl ChorusEnsemble {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_samples = (sr * 0.05) as usize; // 50ms delay lines

        Self {
            name: "Chorus-Ensemble".to_string(),
            enabled: true,
            mode: ChorusMode::Classic,
            rate_hz: 1.2,
            depth: 0.5,
            warmth: 0.3,
            dry_wet: 0.5,
            sample_rate: sr,
            buf_l: vec![0.0; max_samples.max(256)],
            buf_r: vec![0.0; max_samples.max(256)],
            write_pos: 0,
            lfo_phase1: 0.0,
            lfo_phase2: 0.333,
            lfo_phase3: 0.666,
            warm_filter_l: 0.0,
            warm_filter_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            *self = Self::new(sample_rate);
        }
    }
}

impl Default for ChorusEnsemble {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for ChorusEnsemble {
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
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos = 0;
        self.warm_filter_l = 0.0;
        self.warm_filter_r = 0.0;
    }
}

impl AudioEffectDevice for ChorusEnsemble {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let buf_len = self.buf_l.len();
        let lfo_step = self.rate_hz / self.sample_rate;
        let base_delay_samples = 0.003 * self.sample_rate; // 3ms base BBD delay (~132 samples)
        let mod_range_samples = self.depth * 0.002 * self.sample_rate;

        let warmth_alpha = (0.2 + self.warmth * 0.6).clamp(0.0, 0.95);

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            self.buf_l[self.write_pos] = dry_l;
            self.buf_r[self.write_pos] = dry_r;

            let mod1 = (self.lfo_phase1 * 2.0 * PI).sin();
            let mod2 = (self.lfo_phase2 * 2.0 * PI).sin();
            let mod3 = (self.lfo_phase3 * 2.0 * PI).sin();

            self.lfo_phase1 = (self.lfo_phase1 + lfo_step).fract();
            self.lfo_phase2 = (self.lfo_phase2 + lfo_step).fract();
            self.lfo_phase3 = (self.lfo_phase3 + lfo_step).fract();

            let (wet_l, wet_r) = match self.mode {
                ChorusMode::Classic => {
                    let d1 = (base_delay_samples + mod1 * mod_range_samples).clamp(1.0, (buf_len - 2) as f32);
                    let d2 = (base_delay_samples - mod2 * mod_range_samples).clamp(1.0, (buf_len - 2) as f32);

                    let idx1 = ((self.write_pos as f32 - d1 + buf_len as f32) as usize) % buf_len;
                    let idx2 = ((self.write_pos as f32 - d2 + buf_len as f32) as usize) % buf_len;

                    (self.buf_l[idx1], self.buf_r[idx2])
                }
                ChorusMode::Ensemble => {
                    let d1 = (base_delay_samples + mod1 * mod_range_samples).clamp(1.0, (buf_len - 2) as f32);
                    let d2 = (base_delay_samples + mod2 * mod_range_samples).clamp(1.0, (buf_len - 2) as f32);
                    let d3 = (base_delay_samples + mod3 * mod_range_samples).clamp(1.0, (buf_len - 2) as f32);

                    let idx1 = ((self.write_pos as f32 - d1 + buf_len as f32) as usize) % buf_len;
                    let idx2 = ((self.write_pos as f32 - d2 + buf_len as f32) as usize) % buf_len;
                    let idx3 = ((self.write_pos as f32 - d3 + buf_len as f32) as usize) % buf_len;

                    (
                        (self.buf_l[idx1] + self.buf_l[idx2] * 0.5) * 0.67,
                        (self.buf_r[idx3] + self.buf_r[idx2] * 0.5) * 0.67,
                    )
                }
                ChorusMode::Vibrato => {
                    let d = (base_delay_samples + mod1 * mod_range_samples * 1.5).clamp(1.0, (buf_len - 2) as f32);
                    let idx = ((self.write_pos as f32 - d + buf_len as f32) as usize) % buf_len;
                    (self.buf_l[idx], self.buf_r[idx])
                }
            };

            self.write_pos = (self.write_pos + 1) % buf_len;

            // Warmth BBD emulation
            self.warm_filter_l = self.warm_filter_l * warmth_alpha + (wet_l * 1.1).tanh() * (1.0 - warmth_alpha);
            self.warm_filter_r = self.warm_filter_r * warmth_alpha + (wet_r * 1.1).tanh() * (1.0 - warmth_alpha);

            let eff_dw = if self.mode == ChorusMode::Vibrato { 1.0 } else { self.dry_wet };

            buffer.left[i] = dry_l * (1.0 - eff_dw) + self.warm_filter_l * eff_dw;
            buffer.right[i] = dry_r * (1.0 - eff_dw) + self.warm_filter_r * eff_dw;
        }
    }
}
