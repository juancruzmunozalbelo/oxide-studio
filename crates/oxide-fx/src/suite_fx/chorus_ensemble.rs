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
        if sample_rate.is_finite() && sample_rate > 0.0 {
            let max_samples = ((sample_rate * 0.05) as usize).max(256);
            self.sample_rate = sample_rate;
            self.buf_l = vec![0.0; max_samples];
            self.buf_r = vec![0.0; max_samples];
            self.write_pos = 0;
            self.warm_filter_l = 0.0;
            self.warm_filter_r = 0.0;
        }
    }

    #[inline(always)]
    fn read_linear(buf: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
        let len = buf.len();
        let d = delay_samples.clamp(1.0, (len - 2) as f32);
        let idx_int = d.floor() as usize;
        let frac = d - idx_int as f32;
        let i0 = (write_pos + len - idx_int) % len;
        let i1 = (write_pos + len - idx_int - 1) % len;
        buf[i0] + frac * (buf[i1] - buf[i0])
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

        let count = buffer.frame_count.min(oxide_core::MAX_BUFFER_SIZE);
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
                    let d1 = base_delay_samples + mod1 * mod_range_samples;
                    let d2 = base_delay_samples - mod2 * mod_range_samples;

                    (
                        Self::read_linear(&self.buf_l, self.write_pos, d1),
                        Self::read_linear(&self.buf_r, self.write_pos, d2),
                    )
                }
                ChorusMode::Ensemble => {
                    let d1 = base_delay_samples + mod1 * mod_range_samples;
                    let d2 = base_delay_samples + mod2 * mod_range_samples;
                    let d3 = base_delay_samples + mod3 * mod_range_samples;

                    let l1 = Self::read_linear(&self.buf_l, self.write_pos, d1);
                    let l2 = Self::read_linear(&self.buf_l, self.write_pos, d2);
                    let r2 = Self::read_linear(&self.buf_r, self.write_pos, d2);
                    let r3 = Self::read_linear(&self.buf_r, self.write_pos, d3);

                    ((l1 + l2 * 0.5) * 0.67, (r3 + r2 * 0.5) * 0.67)
                }
                ChorusMode::Vibrato => {
                    let d = base_delay_samples + mod1 * mod_range_samples * 1.5;
                    (
                        Self::read_linear(&self.buf_l, self.write_pos, d),
                        Self::read_linear(&self.buf_r, self.write_pos, d),
                    )
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

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.dry_wet = value.clamp(0.0, 1.0),
            1 => self.rate_hz = value.clamp(0.05, 10.0),
            2 => self.depth = value.clamp(0.0, 1.0),
            3 => self.warmth = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.dry_wet,
            1 => self.rate_hz,
            2 => self.depth,
            3 => self.warmth,
            _ => 0.0,
        }
    }
}
