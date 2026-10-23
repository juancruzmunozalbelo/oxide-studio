//! Band-limited PolyBLEP oscillators and white noise generator.
//!
//! Decompiled & reverse-engineered from Ableton Learning Synths (`src/engine/dsp-core.ts`).
//! PolyBLEP suppresses Nyquist discontinuities without large wavetables or oversampling overhead.

/// PolyBLEP residual function for band-limited step discontinuities.
#[inline(always)]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        let u = t / dt;
        return u + u - u * u - 1.0;
    }
    if t > 1.0 - dt {
        let u = (t - 1.0) / dt;
        return u * u + u + u + 1.0;
    }
    0.0
}

/// 32-bit XorShift pseudo-random number generator for uniform white noise.
///
/// Default seed: 0x13579bdf.
#[derive(Debug, Clone)]
pub struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x13579bdf } else { seed },
        }
    }

    /// Reset with a specific seed.
    pub fn reset(&mut self, seed: u32) {
        self.state = if seed == 0 { 0x13579bdf } else { seed };
    }

    /// Generate next pseudo-random sample uniformly distributed in [-1.0, 1.0].
    #[inline(always)]
    #[allow(clippy::excessive_precision)]
    pub fn next_f32(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as i32 as f32) * 4.6566128730773926e-10
    }
}

impl Default for XorShift32 {
    fn default() -> Self {
        Self::new(0x13579bdf)
    }
}

/// Band-limited PolyBLEP Sawtooth oscillator.
#[derive(Debug, Clone, Default)]
pub struct SawOscillator {
    pub phase: f32,
}

impl SawOscillator {
    pub fn new() -> Self {
        Self { phase: 0.0 }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Render next sawtooth sample with PolyBLEP anti-aliasing.
    #[inline(always)]
    pub fn next_sample(&mut self, freq: f32, sample_rate: f32) -> f32 {
        if sample_rate <= 0.0 {
            return 0.0;
        }
        let dt = (freq / sample_rate).clamp(0.0, 0.49);
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let naive = 2.0 * self.phase - 1.0;
        naive - poly_blep(self.phase, dt)
    }
}

/// Band-limited PolyBLEP Square / PWM oscillator with 5% - 95% duty cycle range.
#[derive(Debug, Clone, Default)]
pub struct PulseOscillator {
    pub phase: f32,
}

impl PulseOscillator {
    pub fn new() -> Self {
        Self { phase: 0.0 }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Render next square/pulse sample with PolyBLEP anti-aliasing on rising and falling edges.
    ///
    /// - `freq`: Frequency in Hz
    /// - `pw_percent`: Pulse width percentage in [5.0, 95.0]
    /// - `sample_rate`: Audio sample rate in Hz
    #[inline(always)]
    pub fn next_sample(&mut self, freq: f32, pw_percent: f32, sample_rate: f32) -> f32 {
        if sample_rate <= 0.0 {
            return 0.0;
        }
        let dt = (freq / sample_rate).clamp(0.0, 0.49);
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let duty = (pw_percent * 0.01).clamp(0.05, 0.95);
        let naive = if self.phase < duty { 1.0 } else { -1.0 };

        let rising = poly_blep(self.phase, dt);
        let wrapped_fall = (self.phase - duty + 1.0) % 1.0;
        let falling = poly_blep(wrapped_fall, dt);

        naive + rising - falling
    }
}

/// Mixer for combining Square/PWM, Sawtooth, and White Noise oscillators.
#[inline(always)]
pub fn mix_oscillators(
    sq_sample: f32,
    sq_gain: f32,
    saw_sample: f32,
    saw_gain: f32,
    noise_sample: f32,
    noise_gain: f32,
) -> f32 {
    0.35 * (sq_gain * sq_sample + saw_gain * saw_sample + noise_gain * noise_sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poly_blep_residual() {
        let dt = 0.1f32;
        assert_eq!(poly_blep(0.5, dt), 0.0);
        assert!((poly_blep(0.0, dt) - (-1.0)).abs() < 1e-5);
        assert!((poly_blep(1.0, dt) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_xorshift32_white_noise() {
        let mut rng = XorShift32::new(0x13579bdf);
        let mut sum = 0.0f32;
        let n = 10000;
        for _ in 0..n {
            let s = rng.next_f32();
            assert!(s >= -1.0 && s <= 1.0, "Sample out of bounds: {}", s);
            sum += s;
        }
        let mean = sum / (n as f32);
        assert!(mean.abs() < 0.05, "Mean of white noise must be close to 0, got {}", mean);
    }

    #[test]
    fn test_saw_oscillator_dc_bias() {
        let mut saw = SawOscillator::new();
        let mut sum = 0.0f32;
        let n = 44100;
        for _ in 0..n {
            sum += saw.next_sample(440.0, 44100.0);
        }
        let mean = sum / (n as f32);
        assert!(mean.abs() < 0.01, "Sawtooth should have minimal DC bias, got {}", mean);
    }

    #[test]
    fn test_pulse_oscillator_duty_cycle() {
        let mut pulse = PulseOscillator::new();
        let mut sum = 0.0f32;
        let n = 44100;
        for _ in 0..n {
            sum += pulse.next_sample(100.0, 75.0, 44100.0);
        }
        let mean = sum / (n as f32);
        // At 75% duty cycle: 75% high (+1), 25% low (-1) => expected mean = 0.75 - 0.25 = 0.50
        assert!((mean - 0.50).abs() < 0.05, "Expected mean 0.50 for 75% pulse, got {}", mean);
    }
}
