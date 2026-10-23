//! Live 12 Hybrid Reverb Device.
//!
//! Dual-engine architecture combining zero-latency FIR convolution with an algorithmic
//! 4-delay Feedback Delay Network (FDN) matrix.

use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

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
    // Pre-delay buffer
    predelay_buf_l: Vec<f32>,
    predelay_buf_r: Vec<f32>,
    predelay_pos: usize,
    // Convolution FIR buffer
    ir_l: Vec<f32>,
    ir_r: Vec<f32>,
    conv_history_l: Vec<f32>,
    conv_history_r: Vec<f32>,
    conv_pos: usize,
    // FDN matrix lines
    fdn_buffers: [Vec<f32>; 4],
    fdn_indices: [usize; 4],
    fdn_filter_states: [f32; 4],
}

impl HybridReverb {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_predelay = (sr * 0.2) as usize; // 200 ms
        let ir_len = 256; // Fast partitioned synthetic early reflection IR

        // Generate synthetic early reflection IR
        let mut ir_l = vec![0.0; ir_len];
        let mut ir_r = vec![0.0; ir_len];
        for i in 0..ir_len {
            let t = i as f32 / ir_len as f32;
            let decay = (-3.0 * t).exp();
            let noise_l = ((i as f32 * 123.456).sin() * 43_758.547).fract() * 2.0 - 1.0;
            let noise_r = ((i as f32 * 654.321).sin() * 43_758.547).fract() * 2.0 - 1.0;
            ir_l[i] = noise_l * decay * (1.0 / (ir_len as f32).sqrt());
            ir_r[i] = noise_r * decay * (1.0 / (ir_len as f32).sqrt());
        }

        // Prime numbers for FDN delay lines (approx 20..45 ms)
        let lengths = [
            (sr * 0.023) as usize,
            (sr * 0.029) as usize,
            (sr * 0.037) as usize,
            (sr * 0.043) as usize,
        ];
        let fdn_buffers = [
            vec![0.0; lengths[0].max(128)],
            vec![0.0; lengths[1].max(128)],
            vec![0.0; lengths[2].max(128)],
            vec![0.0; lengths[3].max(128)],
        ];

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
            predelay_buf_l: vec![0.0; max_predelay.max(256)],
            predelay_buf_r: vec![0.0; max_predelay.max(256)],
            predelay_pos: 0,
            ir_l,
            ir_r,
            conv_history_l: vec![0.0; ir_len],
            conv_history_r: vec![0.0; ir_len],
            conv_pos: 0,
            fdn_buffers,
            fdn_indices: [0; 4],
            fdn_filter_states: [0.0; 4],
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            *self = Self::new(sample_rate);
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

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let predelay_len = ((self.predelay_ms * 0.001 * self.sample_rate) as usize)
            .clamp(1, self.predelay_buf_l.len() - 1);
        let ir_len = self.ir_l.len();
        let feedback_gain = (-3.0 / (self.decay_s.max(0.1) * self.sample_rate / 1000.0)).exp();

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            // 1. Pre-delay
            let read_pos = if self.predelay_pos >= predelay_len {
                self.predelay_pos - predelay_len
            } else {
                self.predelay_pos + self.predelay_buf_l.len() - predelay_len
            };
            let pd_l = self.predelay_buf_l[read_pos];
            let pd_r = self.predelay_buf_r[read_pos];

            self.predelay_buf_l[self.predelay_pos] = dry_l;
            self.predelay_buf_r[self.predelay_pos] = dry_r;
            self.predelay_pos = (self.predelay_pos + 1) % self.predelay_buf_l.len();

            // 2. Convolution Engine
            self.conv_history_l[self.conv_pos] = pd_l;
            self.conv_history_r[self.conv_pos] = pd_r;

            let mut conv_out_l = 0.0f32;
            let mut conv_out_r = 0.0f32;
            for k in 0..ir_len {
                let h_idx = (self.conv_pos + ir_len - k) % ir_len;
                conv_out_l += self.conv_history_l[h_idx] * self.ir_l[k];
                conv_out_r += self.conv_history_r[h_idx] * self.ir_r[k];
            }
            self.conv_pos = (self.conv_pos + 1) % ir_len;

            // 3. Algorithmic FDN Engine
            let fdn_in = match self.mode {
                HybridReverbMode::Series => (conv_out_l + conv_out_r) * 0.5,
                _ => (pd_l + pd_r) * 0.5,
            };

            let mut fdn_reads = [0.0f32; 4];
            for (line, val) in fdn_reads.iter_mut().enumerate() {
                let pos = self.fdn_indices[line];
                *val = self.fdn_buffers[line][pos];
            }

            // Householder matrix mixing: y = x - 0.5 * sum(x)
            let sum: f32 = fdn_reads.iter().sum();
            let mut fdn_mixed = [0.0f32; 4];
            for line in 0..4 {
                fdn_mixed[line] = fdn_reads[line] - 0.5 * sum;
                // Damping 1-pole filter
                self.fdn_filter_states[line] =
                    self.fdn_filter_states[line] * self.damping + fdn_mixed[line] * (1.0 - self.damping);
                let write_val = fdn_in + self.fdn_filter_states[line] * feedback_gain;
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
}
