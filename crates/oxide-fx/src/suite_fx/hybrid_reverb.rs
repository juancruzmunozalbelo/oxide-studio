//! Live 12 Hybrid Reverb Device.
//!
//! Dual-engine architecture combining zero-latency FIR convolution with an algorithmic
//! 4-delay Feedback Delay Network (FDN) matrix.

use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

const NUM_ER_TAPS: usize = 24;
const FDN_BASE_SEC: [f32; 4] = [0.023, 0.029, 0.037, 0.043];

/// Reverb routing blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridReverbMode {
    Parallel,
    Series,
    Blend,
}

/// Ableton Live 12 Hybrid Reverb Device.
pub struct HybridReverb {
    pub name: String,
    pub enabled: bool,
    pub mode: HybridReverbMode,
    pub predelay_ms: f32,
    pub decay_s: f32,
    pub size: f32,      // 0.2 .. 2.0
    pub damping: f32,   // 0.0 .. 1.0 (high frequency loss)
    pub conv_algo_blend: f32, // 0.0 = 100% conv, 1.0 = 100% algo
    pub dry_wet: f32,

    sample_rate: f32,
    // Pre-delay buffer (500ms capacity)
    predelay_buf_l: Vec<f32>,
    predelay_buf_r: Vec<f32>,
    predelay_pos: usize,
    // Sparse Early-Reflection tap bank (<= 32 taps) scaled dynamically by size
    er_delays_sec: [f32; NUM_ER_TAPS],
    er_gains_l: [f32; NUM_ER_TAPS],
    er_gains_r: [f32; NUM_ER_TAPS],
    conv_history_l: Vec<f32>,
    conv_history_r: Vec<f32>,
    conv_pos: usize,
    // FDN matrix lines
    fdn_buffers: [Vec<f32>; 4],
    fdn_indices: [usize; 4],
    fdn_filter_states: [f32; 4],
}

impl HybridReverb {
    fn build_buffers(sample_rate: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, [Vec<f32>; 4]) {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            44100.0
        };
        let max_predelay = ((sr * 0.5).ceil() as usize + 2).max(256); // Full 500ms capacity
        let hist_len = ((sr * 0.14).ceil() as usize + 8).max(512); // Supports ER taps scaled up to size=2.0
        let fdn_buffers = [
            vec![0.0; ((sr * FDN_BASE_SEC[0] * 2.0).ceil() as usize + 4).max(256)],
            vec![0.0; ((sr * FDN_BASE_SEC[1] * 2.0).ceil() as usize + 4).max(256)],
            vec![0.0; ((sr * FDN_BASE_SEC[2] * 2.0).ceil() as usize + 4).max(256)],
            vec![0.0; ((sr * FDN_BASE_SEC[3] * 2.0).ceil() as usize + 4).max(256)],
        ];
        (
            vec![0.0; max_predelay],
            vec![0.0; max_predelay],
            vec![0.0; hist_len],
            vec![0.0; hist_len],
            fdn_buffers,
        )
    }

    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            44100.0
        };
        let (predelay_buf_l, predelay_buf_r, conv_history_l, conv_history_r, fdn_buffers) =
            Self::build_buffers(sr);

        let mut er_delays_sec = [0.0f32; NUM_ER_TAPS];
        let mut er_gains_l = [0.0f32; NUM_ER_TAPS];
        let mut er_gains_r = [0.0f32; NUM_ER_TAPS];
        let mut norm_sq_l = 0.0f32;
        let mut norm_sq_r = 0.0f32;
        for k in 0..NUM_ER_TAPS {
            let frac = (k + 1) as f32 / NUM_ER_TAPS as f32;
            er_delays_sec[k] = 0.002 + 0.053 * frac * frac;
            let env = (-2.6 * frac).exp();
            let sign_l = if ((k * 7 + 3) % 5) < 2 { -1.0 } else { 1.0 };
            let sign_r = if ((k * 11 + 1) % 5) < 2 { -1.0 } else { 1.0 };
            er_gains_l[k] = env * sign_l;
            er_gains_r[k] = env * sign_r;
            norm_sq_l += er_gains_l[k] * er_gains_l[k];
            norm_sq_r += er_gains_r[k] * er_gains_r[k];
        }
        let inv_l = 0.8 / norm_sq_l.sqrt().max(1e-4);
        let inv_r = 0.8 / norm_sq_r.sqrt().max(1e-4);
        for k in 0..NUM_ER_TAPS {
            er_gains_l[k] *= inv_l;
            er_gains_r[k] *= inv_r;
        }

        Self {
            name: "Hybrid Reverb".to_string(),
            enabled: true,
            mode: HybridReverbMode::Parallel,
            predelay_ms: 10.0,
            decay_s: 2.0,
            size: 1.0,
            damping: 0.3,
            conv_algo_blend: 0.5,
            dry_wet: 0.4,
            sample_rate: sr,
            predelay_buf_l,
            predelay_buf_r,
            predelay_pos: 0,
            er_delays_sec,
            er_gains_l,
            er_gains_r,
            conv_history_l,
            conv_history_r,
            conv_pos: 0,
            fdn_buffers,
            fdn_indices: [0; 4],
            fdn_filter_states: [0.0; 4],
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate.is_finite() && sample_rate > 0.0 {
            let (predelay_buf_l, predelay_buf_r, conv_history_l, conv_history_r, fdn_buffers) =
                Self::build_buffers(sample_rate);
            self.sample_rate = sample_rate;
            self.predelay_buf_l = predelay_buf_l;
            self.predelay_buf_r = predelay_buf_r;
            self.predelay_pos = 0;
            self.conv_history_l = conv_history_l;
            self.conv_history_r = conv_history_r;
            self.conv_pos = 0;
            self.fdn_buffers = fdn_buffers;
            self.fdn_indices = [0; 4];
            self.fdn_filter_states = [0.0; 4];
        }
    }
}

impl Default for HybridReverb {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for HybridReverb {
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
        self.predelay_buf_l.fill(0.0);
        self.predelay_buf_r.fill(0.0);
        self.conv_history_l.fill(0.0);
        self.conv_history_r.fill(0.0);
        for buf in &mut self.fdn_buffers {
            buf.fill(0.0);
        }
        self.fdn_filter_states.fill(0.0);
    }
}

impl AudioEffectDevice for HybridReverb {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(oxide_core::MAX_BUFFER_SIZE);
        if count == 0 {
            return;
        }

        let predelay_len = ((self.predelay_ms * 0.001 * self.sample_rate).round() as usize)
            .clamp(0, self.predelay_buf_l.len().saturating_sub(1));
        let hist_len = self.conv_history_l.len();
        let size_clamped = self.size.clamp(0.2, 2.0);
        let decay_s = self.decay_s.clamp(0.1, 20.0);
        let damping = self.damping.clamp(0.0, 0.99);

        let mut fdn_eff_lens = [0usize; 4];
        let mut fdn_gains = [0.0f32; 4];
        for line in 0..4 {
            let buf_len = self.fdn_buffers[line].len();
            let eff_len = ((FDN_BASE_SEC[line] * self.sample_rate * size_clamped).round() as usize)
                .clamp(16, buf_len.saturating_sub(1).max(16));
            fdn_eff_lens[line] = eff_len;
            let delay_sec = eff_len as f32 / self.sample_rate;
            fdn_gains[line] = (-6.907755 * delay_sec / decay_s).exp().clamp(0.0, 0.995);
        }

        for i in 0..count {
            let dry_l = if buffer.left[i].is_finite() { buffer.left[i] } else { 0.0 };
            let dry_r = if buffer.right[i].is_finite() { buffer.right[i] } else { 0.0 };

            // 1. Pre-delay (write first so predelay_len == 0 yields immediate zero-delay tap)
            self.predelay_buf_l[self.predelay_pos] = dry_l;
            self.predelay_buf_r[self.predelay_pos] = dry_r;
            let pd_len = self.predelay_buf_l.len();
            let read_pos = (self.predelay_pos + pd_len - predelay_len) % pd_len;
            let pd_l = self.predelay_buf_l[read_pos];
            let pd_r = self.predelay_buf_r[read_pos];
            self.predelay_pos = (self.predelay_pos + 1) % pd_len;

            // 2. Sparse Early Reflections Convolution (<= 32 taps scaled by room size)
            self.conv_history_l[self.conv_pos] = pd_l;
            self.conv_history_r[self.conv_pos] = pd_r;

            let mut conv_out_l = 0.0f32;
            let mut conv_out_r = 0.0f32;
            for k in 0..NUM_ER_TAPS {
                let tap_l = ((self.er_delays_sec[k] * self.sample_rate * size_clamped).round() as usize)
                    .clamp(1, hist_len.saturating_sub(1).max(1));
                let tap_r = ((self.er_delays_sec[k] * 1.07 * self.sample_rate * size_clamped).round() as usize)
                    .clamp(1, hist_len.saturating_sub(1).max(1));
                let idx_l = (self.conv_pos + hist_len - tap_l) % hist_len;
                let idx_r = (self.conv_pos + hist_len - tap_r) % hist_len;
                conv_out_l += self.conv_history_l[idx_l] * self.er_gains_l[k];
                conv_out_r += self.conv_history_r[idx_r] * self.er_gains_r[k];
            }
            self.conv_pos = (self.conv_pos + 1) % hist_len;

            // 3. Algorithmic FDN Engine (size-aware delay lengths & SR-invariant RT60 decay)
            let fdn_in = match self.mode {
                HybridReverbMode::Series => (conv_out_l + conv_out_r) * 0.5,
                _ => (pd_l + pd_r) * 0.5,
            };

            let mut fdn_reads = [0.0f32; 4];
            for (line, val) in fdn_reads.iter_mut().enumerate() {
                let buf_len = self.fdn_buffers[line].len();
                let w = self.fdn_indices[line];
                let r = (w + buf_len - fdn_eff_lens[line]) % buf_len;
                *val = self.fdn_buffers[line][r];
            }

            // Householder matrix mixing: y = x - 0.5 * sum(x)
            let sum: f32 = fdn_reads.iter().sum();
            for line in 0..4 {
                let mixed = fdn_reads[line] - 0.5 * sum;
                // Damping 1-pole filter
                self.fdn_filter_states[line] =
                    self.fdn_filter_states[line] * damping + mixed * (1.0 - damping);
                let write_val = (fdn_in + self.fdn_filter_states[line] * fdn_gains[line]).clamp(-4.0, 4.0);
                let len = self.fdn_buffers[line].len();
                let pos = self.fdn_indices[line];
                self.fdn_buffers[line][pos] = write_val;
                self.fdn_indices[line] = (pos + 1) % len;
            }

            let algo_out_l = (fdn_reads[0] + fdn_reads[2]) * 0.5;
            let algo_out_r = (fdn_reads[1] + fdn_reads[3]) * 0.5;

            // 4. Combine Engines
            let (wet_l, wet_r) = match self.mode {
                HybridReverbMode::Parallel => {
                    let blend = self.conv_algo_blend;
                    (
                        conv_out_l * (1.0 - blend) + algo_out_l * blend,
                        conv_out_r * (1.0 - blend) + algo_out_r * blend,
                    )
                }
                HybridReverbMode::Series => (algo_out_l, algo_out_r),
                HybridReverbMode::Blend => {
                    let b = self.conv_algo_blend;
                    (
                        conv_out_l * (1.0 - b) + algo_out_l * b,
                        conv_out_r * (1.0 - b) + algo_out_r * b,
                    )
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
            1 => self.decay_s = value.clamp(0.1, 20.0),
            2 => self.predelay_ms = value.clamp(0.0, 500.0),
            3 => self.size = value.clamp(0.2, 2.0),
            4 => self.damping = value.clamp(0.0, 1.0),
            5 => self.conv_algo_blend = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.dry_wet,
            1 => self.decay_s,
            2 => self.predelay_ms,
            3 => self.size,
            4 => self.damping,
            5 => self.conv_algo_blend,
            _ => 0.0,
        }
    }
}
