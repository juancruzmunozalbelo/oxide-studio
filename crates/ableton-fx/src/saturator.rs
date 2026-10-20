//! Overdrive / Saturator effect using Padé [4/4] rational approximation for tanh.

#[derive(Debug, Clone, Copy)]
pub struct Saturator {
    pub drive_linear: f32,
    pub wet: f32, // [0.0, 1.0]
    pub output_gain: f32,
}

impl Default for Saturator {
    fn default() -> Self {
        Self::new()
    }
}

impl Saturator {
    pub fn new() -> Self {
        Self {
            drive_linear: 1.0,
            wet: 1.0,
            output_gain: 1.0,
        }
    }

    /// Set drive in decibels (e.g. 0.0 dB to 36.0 dB).
    pub fn set_drive_db(&mut self, drive_db: f32) {
        self.drive_linear = 10.0_f32.powf(drive_db.clamp(0.0, 48.0) / 20.0);
    }

    /// Set output gain in decibels.
    pub fn set_output_gain_db(&mut self, gain_db: f32) {
        self.output_gain = 10.0_f32.powf(gain_db.clamp(-48.0, 24.0) / 20.0);
    }

    /// Padé [4/4] rational approximation of tanh(x) / x and tanh(x).
    #[inline(always)]
    pub fn pade_tanh(x: f32) -> f32 {
        let abs_x = x.abs();
        if abs_x > 4.5 {
            return x.signum();
        }
        let a = x * x;
        let num = (a + 105.0) * a + 945.0;
        let denom = (15.0 * a + 420.0) * a + 945.0;
        x * (num / denom)
    }

    /// Process a single mono sample through the saturator.
    #[inline(always)]
    pub fn process_sample(&self, input: f32) -> f32 {
        let driven = input * self.drive_linear;
        let saturated = Self::pade_tanh(driven);
        let mixed = input * (1.0 - self.wet) + saturated * self.wet;
        mixed * self.output_gain
    }

    /// Process stereo planar audio buffers in-place. Zero allocations.
    pub fn process_block(&self, left: &mut [f32], right: &mut [f32]) {
        let len = left.len().min(right.len());
        for i in 0..len {
            left[i] = self.process_sample(left[i]);
            right[i] = self.process_sample(right[i]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pade_tanh_accuracy() {
        for v in [-3.0f32, -1.5, -0.5, 0.0, 0.5, 1.5, 3.0] {
            let actual = v.tanh();
            let approx = Saturator::pade_tanh(v);
            assert!(
                (actual - approx).abs() < 0.005,
                "Padé [4/4] mismatch for v={}: actual={}, approx={}",
                v,
                actual,
                approx
            );
        }
    }

    #[test]
    fn test_saturator_processing() {
        let mut sat = Saturator::new();
        sat.set_drive_db(12.0); // 4x gain
        let out = sat.process_sample(0.5);
        assert!(out > 0.5 && out < 1.0);
    }
}
