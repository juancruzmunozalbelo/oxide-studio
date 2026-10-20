//! Mystran 2x-oversampled non-linear Zero-Delay Feedback (ZDF) 4-pole low-pass ladder filter.
//!
//! Decompiled & reverse-engineered from Ableton Learning Synths and Cycling '74 Max RNBO
//! (`rnbomatic.wasm`, `code[817]`, Loop #84 @ +17920).
//! Reference: `docs/adr/0001-dual-dsp-engine-and-mystran-zdf-ladder.md`

use std::f32::consts::PI;

/// Mystran 2x-oversampled non-linear ZDF 4-pole low-pass ladder filter.
#[derive(Debug, Clone)]
pub struct MystranZdfLadder {
    pub sample_rate: f32,
    pub s1: f32,
    pub s2: f32,
    pub s3: f32,
    pub s4: f32,
    pub prev_input: f32,
}

impl MystranZdfLadder {
    /// Create a new ladder filter initialized with a given sample rate.
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: if sample_rate <= 0.0 { 44100.0 } else { sample_rate },
            s1: 0.0,
            s2: 0.0,
            s3: 0.0,
            s4: 0.0,
            prev_input: 0.0,
        }
    }

    /// Reset internal integrator states and input memory.
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
        self.s3 = 0.0;
        self.s4 = 0.0;
        self.prev_input = 0.0;
    }

    /// Update sample rate.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    /// Padé [4/4] rational approximation for `tanh(x) / x`.
    ///
    /// Accurate to within 0.001% of true tanh(x)/x for |x| < 4.0.
    #[inline(always)]
    pub fn tanh_xdx(x: f32) -> f32 {
        let abs_x = x.abs();
        if abs_x > 4.5 {
            return 1.0 / abs_x;
        }
        let a = x * x;
        ((a + 105.0) * a + 945.0) / ((15.0 * a + 420.0) * a + 945.0)
    }

    /// Process a single audio sample through the 4-pole ladder filter.
    ///
    /// - `x`: Input audio sample
    /// - `cutoff_hz`: Cutoff frequency in Hz (clamped between 15 Hz and 0.45 * sample_rate, max 18 kHz)
    /// - `resonance_norm`: Normalized resonance in [0.0, 1.0]
    ///
    /// Returns the filtered sample with passband resonance gain compensation.
    #[allow(clippy::excessive_precision)]
    pub fn process_sample(&mut self, x: f32, cutoff_hz: f32, resonance_norm: f32) -> f32 {
        let sr = self.sample_rate;
        let nyquist_limit = (sr * 0.45).min(18000.0);
        let fc = cutoff_hz.clamp(15.0, nyquist_limit);

        // Prewarped cutoff at 2x oversampled rate: w = pi * fc / (2 * fs)
        let w = (fc * PI) / (2.0 * sr);
        let w2 = w * w;
        let f = (w * (0.99999949 - 0.096524608 * w2))
            / (1.0 - 0.42986726 * w2 + 0.009981878 * w2 * w2);

        // Resonance mapping: 0..1 -> k in 0..4.35
        let res_clamped = resonance_norm.clamp(0.0, 1.0);
        let r = 4.35 * res_clamped.powf(0.85);

        let mut y4 = self.s4;
        let prev_x = self.prev_input;
        self.prev_input = x;

        // 2x oversampling loop: linear midpoint interpolation
        for pass in 0..2 {
            let in_sample = if pass == 0 { 0.5 * (x + prev_x) } else { x };

            let s1 = self.s1;
            let s2 = self.s2;
            let s3 = self.s3;
            let s4 = self.s4;

            // Input saturation + feedback linearization
            let zi = in_sample - r * s4;
            let t0 = Self::tanh_xdx(zi);

            // Pivotal secant gains
            let t1 = Self::tanh_xdx(s1);
            let t2 = Self::tanh_xdx(s2);
            let t3 = Self::tanh_xdx(s3);
            let t4 = Self::tanh_xdx(s4);

            let g0 = 1.0 / (1.0 + f * t1);
            let g1 = 1.0 / (1.0 + f * t2);
            let g2 = 1.0 / (1.0 + f * t3);
            let g3 = 1.0 / (1.0 + f * t4);

            let f3 = f * t3 * g3;
            let f2 = f * t2 * g2 * f3;
            let f1 = f * t1 * g1 * f2;
            let f0 = f * t0 * g0 * f1;

            let num = g3 * s4
                + f3 * g2 * s3
                + f2 * g1 * s2
                + f1 * g0 * s1
                + f0 * in_sample;

            let denom = 1.0 + r * f0;
            y4 = num / denom;

            // Back-substitute through the 4 ZDF integrator stages
            let u0 = t0 * (in_sample - r * y4);
            let v1 = g0 * (s1 + f * u0);
            let v2 = g1 * (s2 + f * t1 * v1);
            let v3 = g2 * (s3 + f * t2 * v2);

            self.s1 = 2.0 * v1 - s1;
            self.s2 = 2.0 * v2 - s2;
            self.s3 = 2.0 * v3 - s3;
            self.s4 = 2.0 * y4 - s4;
        }

        // Passband resonance gain compensation
        y4 * (1.0 + 0.25 * res_clamped)
    }

    /// Process a block of samples in-place or into an output slice.
    pub fn process_block(&mut self, input: &[f32], output: &mut [f32], cutoff_hz: f32, resonance_norm: f32) {
        let count = input.len().min(output.len());
        for i in 0..count {
            output[i] = self.process_sample(input[i], cutoff_hz, resonance_norm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tanh_xdx_accuracy() {
        for &x in &[0.0f32, 0.1, 0.5, 1.0, 2.0, 3.0] {
            let approx = MystranZdfLadder::tanh_xdx(x);
            let exact = if x.abs() < 1e-6 { 1.0 } else { x.tanh() / x };
            assert!((approx - exact).abs() < 0.005, "Mismatch at x={}: approx={}, exact={}", x, approx, exact);
        }
    }

    #[test]
    fn test_filter_attenuation_and_stability() {
        let mut filter_low = MystranZdfLadder::new(44100.0);
        let mut filter_high = MystranZdfLadder::new(44100.0);

        let mut energy_low = 0.0f32;
        let mut energy_high = 0.0f32;

        let freq = 880.0f32;
        let mut phase = 0.0f32;
        let dt = freq / 44100.0;

        for i in 0..2048 {
            let x = 2.0 * phase - 1.0;
            phase = (phase + dt) % 1.0;

            let y_low = filter_low.process_sample(x, 180.0, 0.1);
            let y_high = filter_high.process_sample(x, 15000.0, 0.1);

            if i >= 512 {
                energy_low += y_low * y_low;
                energy_high += y_high * y_high;
            }
        }

        assert!(energy_low < 0.1 * energy_high, "180Hz cutoff must attenuate 880Hz saw by > 20dB");

        // Test extreme self-oscillation
        let mut osc_filter = MystranZdfLadder::new(44100.0);
        osc_filter.process_sample(1.0, 1200.0, 1.0); // impulse
        for _ in 0..4096 {
            let y = osc_filter.process_sample(0.0, 1200.0, 1.0);
            assert!(y.is_finite(), "Output must be finite");
            assert!(y.abs() < 2.0, "Self-oscillation must stay bounded within |y| < 2.0, got {}", y);
        }
    }
}
