//! Master 128-sample lookahead brickwall limiter with -0.3 dBFS ceiling.

use ableton_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

pub const LOOKAHEAD_SAMPLES: usize = 128;
pub const CEILING_LINEAR: f32 = 0.9660509; // -0.3 dBFS

#[derive(Debug, Clone)]
pub struct MasterLimiter {
    lookahead_buffer_l: [f32; LOOKAHEAD_SAMPLES],
    lookahead_buffer_r: [f32; LOOKAHEAD_SAMPLES],
    write_pos: usize,
    envelope: f32,
    release_coeff: f32,
    pub ceiling: f32,
    sample_rate: f32,
}

impl Default for MasterLimiter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl MasterLimiter {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        // ~60ms exponential release
        let release_samples = 0.06 * sr;
        let release_coeff = (-1.0 / release_samples).exp();

        Self {
            lookahead_buffer_l: [0.0; LOOKAHEAD_SAMPLES],
            lookahead_buffer_r: [0.0; LOOKAHEAD_SAMPLES],
            write_pos: 0,
            envelope: 0.0,
            release_coeff,
            ceiling: CEILING_LINEAR,
            sample_rate: sr,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            let release_samples = 0.06 * sample_rate;
            self.release_coeff = (-1.0 / release_samples).exp();
        }
    }

    pub fn reset(&mut self) {
        self.lookahead_buffer_l.fill(0.0);
        self.lookahead_buffer_r.fill(0.0);
        self.write_pos = 0;
        self.envelope = 0.0;
    }

    /// Process a block of stereo planar samples with zero allocations.
    /// Guarantees that peak amplitude never exceeds `ceiling` (-0.3 dBFS) even under extreme overdrive.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());

        for i in 0..len {
            let in_l = left[i];
            let in_r = right[i];
            let peak_in = in_l.abs().max(in_r.abs());

            // Instant attack: envelope snaps up immediately to input peak
            if peak_in > self.envelope {
                self.envelope = peak_in;
            } else {
                // Exponential release decay
                self.envelope = peak_in + (self.envelope - peak_in) * self.release_coeff;
            }

            // Calculate gain reduction
            let gain = if self.envelope > self.ceiling {
                self.ceiling / self.envelope
            } else {
                1.0
            };

            // Read oldest delayed sample from lookahead ring buffer
            let read_idx = self.write_pos;
            let delayed_l = self.lookahead_buffer_l[read_idx];
            let delayed_r = self.lookahead_buffer_r[read_idx];

            // Store new input sample into lookahead buffer
            self.lookahead_buffer_l[self.write_pos] = in_l;
            self.lookahead_buffer_r[self.write_pos] = in_r;
            self.write_pos = (self.write_pos + 1) % LOOKAHEAD_SAMPLES;

            // Apply calculated gain to the delayed audio sample
            let mut out_l = delayed_l * gain;
            let mut out_r = delayed_r * gain;

            // Hard safety brickwall clamping as secondary guard
            if out_l.abs() > self.ceiling {
                out_l = out_l.signum() * self.ceiling;
            }
            if out_r.abs() > self.ceiling {
                out_r = out_r.signum() * self.ceiling;
            }

            left[i] = out_l;
            right[i] = out_r;
        }
    }
}

impl Device for MasterLimiter {
    fn name(&self) -> &str {
        "Limiter"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::AudioEffect
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn set_enabled(&mut self, _enabled: bool) {}

    fn reset(&mut self) {
        self.reset();
    }

    fn latency_samples(&self) -> usize {
        LOOKAHEAD_SAMPLES
    }
}

impl AudioEffectDevice for MasterLimiter {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        let count = buffer.frame_count.min(1024);
        self.process_block(&mut buffer.left[0..count], &mut buffer.right[0..count]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_brickwall_ceiling_under_12db_overdrive() {
        let mut limiter = MasterLimiter::new(44100.0);
        // +12 dB overdrive is ~4.0 amplitude
        let mut left = vec![4.0f32; 1024];
        let mut right = vec![4.0f32; 1024];

        limiter.process_block(&mut left, &mut right);

        for (i, (&l, &r)) in left.iter().zip(right.iter()).enumerate() {
            // First 128 samples are latency flush
            if i >= 128 {
                assert!(
                    l.abs() <= CEILING_LINEAR + 1e-6,
                    "Left sample {} exceeded ceiling: {}",
                    i,
                    l
                );
                assert!(
                    r.abs() <= CEILING_LINEAR + 1e-6,
                    "Right sample {} exceeded ceiling: {}",
                    i,
                    r
                );
            }
            assert!(!l.is_nan() && l.is_finite());
            assert!(!r.is_nan() && r.is_finite());
        }
    }
}
