//! Live 12 Phaser-Flanger Device.
//!
//! Allpass filter cascade (up to 32 poles) with feedback and LFO sweeps.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaserFlangerMode {
    Phaser,
    Flanger,
    Doubler,
}

pub struct PhaserFlanger {
    pub name: String,
    pub enabled: bool,
    pub mode: PhaserFlangerMode,
    pub poles: usize,   // 2 .. 32 (default 8)
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,  // -0.95 .. +0.95
    pub base_freq_hz: f32,
    pub dry_wet: f32,

    sample_rate: f32,
    lfo_phase: f32,
    allpass_states_l: Vec<f32>,
    allpass_states_r: Vec<f32>,
    feedback_state_l: f32,
    feedback_state_r: f32,
    // Flanger delay buffer
    flange_buf_l: Vec<f32>,
    flange_buf_r: Vec<f32>,
    flange_pos: usize,
}

impl PhaserFlanger {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_flange = (sr * 0.05) as usize; // 50ms for flanger/doubler

        Self {
            name: "Phaser-Flanger".to_string(),
            enabled: true,
            mode: PhaserFlangerMode::Phaser,
            poles: 8,
            rate_hz: 0.5,
            depth: 0.7,
            feedback: 0.6,
            base_freq_hz: 800.0,
            dry_wet: 0.5,
            sample_rate: sr,
            lfo_phase: 0.0,
            allpass_states_l: vec![0.0; 32],
            allpass_states_r: vec![0.0; 32],
            feedback_state_l: 0.0,
            feedback_state_r: 0.0,
            flange_buf_l: vec![0.0; max_flange.max(256)],
            flange_buf_r: vec![0.0; max_flange.max(256)],
            flange_pos: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate.is_finite() && sample_rate > 0.0 {
            let max_flange = ((sample_rate * 0.05) as usize).max(256);
            self.sample_rate = sample_rate;
            self.allpass_states_l.fill(0.0);
            self.allpass_states_r.fill(0.0);
            self.feedback_state_l = 0.0;
            self.feedback_state_r = 0.0;
            self.flange_buf_l = vec![0.0; max_flange];
            self.flange_buf_r = vec![0.0; max_flange];
            self.flange_pos = 0;
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

impl Default for PhaserFlanger {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for PhaserFlanger {
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
        self.lfo_phase = 0.0;
        self.allpass_states_l.fill(0.0);
        self.allpass_states_r.fill(0.0);
        self.feedback_state_l = 0.0;
        self.feedback_state_r = 0.0;
        self.flange_buf_l.fill(0.0);
        self.flange_buf_r.fill(0.0);
        self.flange_pos = 0;
    }
}

impl AudioEffectDevice for PhaserFlanger {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(oxide_core::MAX_BUFFER_SIZE);
        if count == 0 {
            return;
        }

        let lfo_step = self.rate_hz / self.sample_rate;
        let num_poles = self.poles.clamp(2, 32);

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            let lfo_l = (self.lfo_phase * 2.0 * PI).sin();
            let lfo_r = ((self.lfo_phase + 0.25) * 2.0 * PI).sin(); // 90 deg stereo phase
            self.lfo_phase = (self.lfo_phase + lfo_step).fract();

            let (wet_l, wet_r) = match self.mode {
                PhaserFlangerMode::Phaser => {
                    // Sweep cutoff frequency
                    let max_fc = (self.sample_rate * 0.45).max(50.0);
                    let mod_l = (self.base_freq_hz * 2.0_f32.powf(lfo_l * self.depth * 2.0)).clamp(50.0, max_fc);
                    let mod_r = (self.base_freq_hz * 2.0_f32.powf(lfo_r * self.depth * 2.0)).clamp(50.0, max_fc);

                    let w_l = PI * mod_l / self.sample_rate;
                    let w_r = PI * mod_r / self.sample_rate;
                    let a1_l = (w_l - 1.0) / (w_l + 1.0);
                    let a1_r = (w_r - 1.0) / (w_r + 1.0);

                    let in_l = dry_l + self.feedback_state_l * self.feedback;
                    let in_r = dry_r + self.feedback_state_r * self.feedback;

                    let mut node_l = in_l;
                    let mut node_r = in_r;

                    for p in 0..num_poles {
                        // 1st order allpass: y[n] = a1 * x[n] + x[n-1] - a1 * y[n-1]
                        let prev_s_l = self.allpass_states_l[p];
                        let y_l = a1_l * node_l + prev_s_l;
                        self.allpass_states_l[p] = node_l - a1_l * y_l;
                        node_l = y_l;

                        let prev_s_r = self.allpass_states_r[p];
                        let y_r = a1_r * node_r + prev_s_r;
                        self.allpass_states_r[p] = node_r - a1_r * y_r;
                        node_r = y_r;
                    }

                    self.feedback_state_l = node_l.clamp(-2.0, 2.0);
                    self.feedback_state_r = node_r.clamp(-2.0, 2.0);

                    (node_l, node_r)
                }
                PhaserFlangerMode::Flanger | PhaserFlangerMode::Doubler => {
                    let max_delay_ms = if self.mode == PhaserFlangerMode::Flanger { 5.0 } else { 25.0 };
                    let base_delay = max_delay_ms * 0.001 * self.sample_rate;
                    let mod_range = base_delay * self.depth * 0.8;

                    let d_l = base_delay + lfo_l * mod_range;
                    let d_r = base_delay + lfo_r * mod_range;

                    let buf_len = self.flange_buf_l.len();
                    let del_l = Self::read_linear(&self.flange_buf_l, self.flange_pos, d_l);
                    let del_r = Self::read_linear(&self.flange_buf_r, self.flange_pos, d_r);

                    self.flange_buf_l[self.flange_pos] = (dry_l + del_l * self.feedback).clamp(-2.0, 2.0);
                    self.flange_buf_r[self.flange_pos] = (dry_r + del_r * self.feedback).clamp(-2.0, 2.0);
                    self.flange_pos = (self.flange_pos + 1) % buf_len;

                    (del_l, del_r)
                }
            };

            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet;
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.dry_wet = value.clamp(0.0, 1.0),
            1 => self.rate_hz = value.clamp(0.01, 20.0),
            2 => self.depth = value.clamp(0.0, 1.0),
            3 => self.feedback = value.clamp(-0.95, 0.95),
            4 => self.base_freq_hz = value.clamp(50.0, 10000.0),
            5 => self.poles = value.round().clamp(2.0, 32.0) as usize,
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.dry_wet,
            1 => self.rate_hz,
            2 => self.depth,
            3 => self.feedback,
            4 => self.base_freq_hz,
            5 => self.poles as f32,
            _ => 0.0,
        }
    }
}
