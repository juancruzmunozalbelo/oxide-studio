//! Reverb engine: Algorithmic Feedback Delay Network (FDN) and Convolution Reverb.

use std::path::Path;
use std::sync::Arc;
use ableton_drums::load_wav_file;

// ===========================================================================
// 1. Algorithmic Feedback Delay Network (FDN) Reverb
// ===========================================================================

const NUM_FDN_LINES: usize = 8;
const MAX_FDN_DELAY: usize = 4096;

pub struct FdnReverb {
    delay_buffers: [Box<[f32; MAX_FDN_DELAY]>; NUM_FDN_LINES],
    write_indices: [usize; NUM_FDN_LINES],
    delay_lengths: [usize; NUM_FDN_LINES],
    damp_states: [f32; NUM_FDN_LINES],
    // 4 Series All-Pass Diffusers for early reflection density
    ap_buffers: [Box<[f32; 1024]>; 4],
    ap_write_indices: [usize; 4],
    ap_lengths: [usize; 4],
    pub sample_rate: f32,
    pub room_size: f32, // [0.1, 2.0]
    pub decay: f32,     // RT60 feedback gain [0.1, 0.98]
    pub damping: f32,   // High frequency absorption [0.0, 0.95]
    pub wet: f32,       // [0.0, 1.0]
}

impl Default for FdnReverb {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl FdnReverb {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let sr_scale = sr / 44100.0;

        // Mutually prime delay lengths for smooth diffuse tail
        let base_delays = [1087, 1283, 1487, 1721, 1987, 2281, 2593, 2909];
        let mut delay_lengths = [0usize; NUM_FDN_LINES];
        for i in 0..NUM_FDN_LINES {
            delay_lengths[i] = ((base_delays[i] as f32 * sr_scale) as usize).clamp(16, MAX_FDN_DELAY - 1);
        }

        // 4 Series Allpass Diffusers
        let ap_base = [223, 347, 467, 599];
        let mut ap_lengths = [0usize; 4];
        for i in 0..4 {
            ap_lengths[i] = ((ap_base[i] as f32 * sr_scale) as usize).clamp(8, 1023);
        }

        Self {
            delay_buffers: std::array::from_fn(|_| Box::new([0.0; MAX_FDN_DELAY])),
            write_indices: [0; NUM_FDN_LINES],
            delay_lengths,
            damp_states: [0.0; NUM_FDN_LINES],
            ap_buffers: std::array::from_fn(|_| Box::new([0.0; 1024])),
            ap_write_indices: [0; 4],
            ap_lengths,
            sample_rate: sr,
            room_size: 1.0,
            decay: 0.85,
            damping: 0.35,
            wet: 0.25,
        }
    }

    pub fn reset(&mut self) {
        for b in &mut self.delay_buffers {
            b.fill(0.0);
        }
        for b in &mut self.ap_buffers {
            b.fill(0.0);
        }
        self.write_indices.fill(0);
        self.ap_write_indices.fill(0);
        self.damp_states.fill(0.0);
    }

    /// Process a mono sample through 4 series allpass filters.
    #[inline(always)]
    fn diffuse_sample(&mut self, mut input: f32) -> f32 {
        const AP_GAIN: f32 = 0.6;
        for stage in 0..4 {
            let len = self.ap_lengths[stage];
            let w = self.ap_write_indices[stage];
            let r = (w + 1024 - len) % 1024;
            let delayed = self.ap_buffers[stage][r];

            let v = input - AP_GAIN * delayed;
            let output = delayed + AP_GAIN * v;
            self.ap_buffers[stage][w] = v;
            self.ap_write_indices[stage] = (w + 1) % 1024;
            input = output;
        }
        input
    }

    /// Process stereo planar audio buffers in-place. Zero allocations.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        let g = self.decay.clamp(0.1, 0.97) * 0.353553; // Normalized by sqrt(8) = 1 / 2.828427
        let alpha = (1.0 - self.damping.clamp(0.0, 0.95)).clamp(0.05, 1.0);

        let mut read_samples = [0.0f32; NUM_FDN_LINES];

        for i in 0..len {
            let in_l = left[i];
            let in_r = right[i];
            let in_mono = 0.5 * (in_l + in_r);

            // Diffuse input
            let diffused = self.diffuse_sample(in_mono);

            // 1. Read delay lines and apply one-pole lowpass damping
            for ch in 0..NUM_FDN_LINES {
                let d_len = ((self.delay_lengths[ch] as f32 * self.room_size) as usize)
                    .clamp(16, MAX_FDN_DELAY - 1);
                let w = self.write_indices[ch];
                let r = (w + MAX_FDN_DELAY - d_len) % MAX_FDN_DELAY;
                let delayed = self.delay_buffers[ch][r];

                self.damp_states[ch] += alpha * (delayed - self.damp_states[ch]);
                read_samples[ch] = self.damp_states[ch];
            }

            // 2. 8x8 Normalized Hadamard Matrix Mixing
            // Fast Walsh-Hadamard Transform (FWHT) in 3 stages
            let mut h = read_samples;
            // Stage 1
            for k in 0..4 {
                let a = h[k * 2];
                let b = h[k * 2 + 1];
                h[k * 2] = a + b;
                h[k * 2 + 1] = a - b;
            }
            // Stage 2
            for k in 0..2 {
                let offset = k * 4;
                let a0 = h[offset];
                let a1 = h[offset + 1];
                let b0 = h[offset + 2];
                let b1 = h[offset + 3];
                h[offset] = a0 + b0;
                h[offset + 1] = a1 + b1;
                h[offset + 2] = a0 - b0;
                h[offset + 3] = a1 - b1;
            }
            // Stage 3
            for k in 0..4 {
                let a = h[k];
                let b = h[k + 4];
                h[k] = a + b;
                h[k + 4] = a - b;
            }

            // 3. Write feedback + input into delay lines
            for ch in 0..NUM_FDN_LINES {
                let feedback_val = h[ch] * g;
                let w = self.write_indices[ch];
                self.delay_buffers[ch][w] = diffused + feedback_val;
                self.write_indices[ch] = (w + 1) % MAX_FDN_DELAY;
            }

            // 4. Stereo decorrelated output tap summing
            let out_l = (read_samples[0] + read_samples[2] + read_samples[4] + read_samples[6]) * 0.5;
            let out_r = (read_samples[1] + read_samples[3] + read_samples[5] + read_samples[7]) * 0.5;

            // Wet/dry mix
            left[i] = in_l * (1.0 - self.wet) + out_l * self.wet;
            right[i] = in_r * (1.0 - self.wet) + out_r * self.wet;
        }
    }
}

// ===========================================================================
// 2. Convolution Reverb
// ===========================================================================

pub struct ConvolutionReverb {
    ir_samples: Arc<[f32]>,
    history_l: Vec<f32>,
    history_r: Vec<f32>,
    write_pos: usize,
    ir_len: usize,
    pub wet: f32,
}

impl ConvolutionReverb {
    pub fn new(ir_samples: Arc<[f32]>) -> Self {
        let ir_len = ir_samples.len().min(16384); // Up to ~0.37s direct convolution
        Self {
            ir_samples,
            history_l: vec![0.0; ir_len],
            history_r: vec![0.0; ir_len],
            write_pos: 0,
            ir_len,
            wet: 0.3,
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let decoded = load_wav_file(path)?;
        let arc_data = Arc::from(decoded.samples.into_boxed_slice());
        Ok(Self::new(arc_data))
    }

    pub fn reset(&mut self) {
        self.history_l.fill(0.0);
        self.history_r.fill(0.0);
        self.write_pos = 0;
    }

    /// Process a block of stereo planar samples with zero allocations during steady-state.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        if self.ir_len == 0 {
            return;
        }

        for i in 0..len {
            let in_l = left[i];
            let in_r = right[i];

            self.history_l[self.write_pos] = in_l;
            self.history_r[self.write_pos] = in_r;

            // Direct FIR Convolution
            let mut conv_l = 0.0f32;
            let mut conv_r = 0.0f32;
            let mut h_idx = self.write_pos;

            for ir_idx in 0..self.ir_len {
                let h_l = self.history_l[h_idx];
                let h_r = self.history_r[h_idx];
                let ir_val = self.ir_samples[ir_idx];

                conv_l += h_l * ir_val;
                conv_r += h_r * ir_val;

                if h_idx == 0 {
                    h_idx = self.ir_len - 1;
                } else {
                    h_idx -= 1;
                }
            }

            self.write_pos = (self.write_pos + 1) % self.ir_len;

            left[i] = in_l * (1.0 - self.wet) + conv_l * self.wet;
            right[i] = in_r * (1.0 - self.wet) + conv_r * self.wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fdn_reverb_stability() {
        let mut fdn = FdnReverb::new(44100.0);
        fdn.wet = 1.0;
        let mut left = [0.0f32; 512];
        let mut right = [0.0f32; 512];
        left[0] = 1.0;
        right[0] = 1.0;

        fdn.process_block(&mut left, &mut right);

        for &s in left.iter().chain(right.iter()) {
            assert!(s.is_finite());
            assert!(!s.is_nan());
            assert!(s.abs() <= 2.0);
        }
    }

    #[test]
    fn test_convolution_reverb_impulse() {
        // Delta impulse response: [1.0, 0.0, 0.0]
        let ir = Arc::from(vec![1.0, 0.0, 0.0].into_boxed_slice());
        let mut conv = ConvolutionReverb::new(ir);
        conv.wet = 1.0;

        let mut left = [0.5, 0.0, 0.0];
        let mut right = [0.5, 0.0, 0.0];
        conv.process_block(&mut left, &mut right);

        assert!((left[0] - 0.5).abs() < 1e-4);
    }
}
