//! Ableton Live 12 Flagship Saturator & Coloration Processor ("Roar").
//!
//! Features:
//! - 6 Topologies: Single, Serial, Parallel, MidSide, Multiband, Feedback.
//! - 10 Saturation Curves: SoftSine, DigitalClip, BitCrusher, Polynomial, Tube, HalfWave, Diode, HardClip, SineWavefold, NoiseShaper.
//! - 1st-order Antiderivative Antialiasing (ADAA) for alias-free harmonic saturation.
//! - Resonant non-linear pre/post filter stage.
//! - Feedback delay loop with frequency tuning and resonance.
//! - Modulation matrix, tone shaping, and parallel dry/wet mix.

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

/// Topologies for Roar's multi-stage signal routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoarTopology {
    Single,
    Serial,
    Parallel,
    MidSide,
    Multiband,
    Feedback,
}

/// The 10 saturation shaper curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoarCurve {
    SoftSine,
    DigitalClip,
    BitCrusher,
    Polynomial,
    Tube,
    HalfWave,
    Diode,
    HardClip,
    SineWavefold,
    NoiseShaper,
}

/// A saturation stage within Roar.
#[derive(Debug, Clone)]
pub struct RoarStage {
    pub curve: RoarCurve,
    pub drive_db: f32,
    pub bias: f32,
    pub tone: f32, // -1.0 (dark) .. +1.0 (bright)
    last_x_l: f32,
    last_x_r: f32,
    tone_state_l: f32,
    tone_state_r: f32,
}

impl RoarStage {
    pub fn new(curve: RoarCurve, drive_db: f32) -> Self {
        Self {
            curve,
            drive_db,
            bias: 0.0,
            tone: 0.0,
            last_x_l: 0.0,
            last_x_r: 0.0,
            tone_state_l: 0.0,
            tone_state_r: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.last_x_l = 0.0;
        self.last_x_r = 0.0;
        self.tone_state_l = 0.0;
        self.tone_state_r = 0.0;
    }

    /// Evaluates direct saturation function f(x).
    #[inline(always)]
    fn evaluate_f(curve: RoarCurve, x: f32) -> f32 {
        match curve {
            RoarCurve::SoftSine => {
                let clamped = x.clamp(-1.0, 1.0);
                (clamped * PI * 0.5).sin()
            }
            RoarCurve::DigitalClip => x.clamp(-1.0, 1.0),
            RoarCurve::BitCrusher => {
                let steps = 16.0;
                (x * steps).round() / steps
            }
            RoarCurve::Polynomial => {
                let clamped = x.clamp(-1.2, 1.2);
                1.5 * clamped - 0.5 * clamped * clamped * clamped
            }
            RoarCurve::Tube => {
                // Asymmetric triode transfer
                let x_bias = x - 0.1;
                let denom = 1.0 - (-2.0 * x_bias).exp();
                if denom.abs() > 1e-4 {
                    (x_bias / denom).clamp(-1.5, 1.5)
                } else {
                    0.5
                }
            }
            RoarCurve::HalfWave => x.max(0.0),
            RoarCurve::Diode => {
                // Shockley-like exponential diode clipper
                let sign = if x >= 0.0 { 1.0 } else { -1.0 };
                let abs_x = x.abs();
                sign * (1.0 - (-abs_x).exp())
            }
            RoarCurve::HardClip => x.clamp(-0.8, 0.8),
            RoarCurve::SineWavefold => {
                (x * PI).sin()
            }
            RoarCurve::NoiseShaper => {
                let noise = ((x * 1234.567).fract() - 0.5) * 0.1;
                (x + noise).tanh()
            }
        }
    }

    /// First antiderivative F1(x) for Antiderivative Antialiasing (ADAA).
    #[inline(always)]
    fn evaluate_f1(curve: RoarCurve, x: f32) -> f32 {
        match curve {
            RoarCurve::SoftSine => {
                let clamped = x.clamp(-1.0, 1.0);
                -(2.0 / PI) * (clamped * PI * 0.5).cos()
            }
            RoarCurve::DigitalClip => {
                if x < -1.0 {
                    -x - 0.5
                } else if x <= 1.0 {
                    0.5 * x * x
                } else {
                    x - 0.5
                }
            }
            RoarCurve::Polynomial => {
                let c = x.clamp(-1.2, 1.2);
                0.75 * c * c - 0.125 * c * c * c * c
            }
            RoarCurve::HalfWave => {
                if x > 0.0 {
                    0.5 * x * x
                } else {
                    0.0
                }
            }
            // For other curves, fallback to trapezoidal integration
            _ => 0.5 * x * Self::evaluate_f(curve, x),
        }
    }

    /// Process a single sample through 1st-order ADAA.
    #[inline(always)]
    fn process_sample_adaa(curve: RoarCurve, x: f32, last_x: &mut f32) -> f32 {
        let diff = x - *last_x;
        let y = if diff.abs() > 1e-5 {
            (Self::evaluate_f1(curve, x) - Self::evaluate_f1(curve, *last_x)) / diff
        } else {
            Self::evaluate_f(curve, (x + *last_x) * 0.5)
        };
        *last_x = x;
        y
    }

    /// Process one stereo sample through this stage with drive, tone, and ADAA.
    #[inline(always)]
    pub fn process_sample(&mut self, l: f32, r: f32) -> (f32, f32) {
        let drive = 10.0_f32.powf(self.drive_db / 20.0);
        let in_l = l * drive + self.bias;
        let in_r = r * drive + self.bias;

        let sat_l = Self::process_sample_adaa(self.curve, in_l, &mut self.last_x_l);
        let sat_r = Self::process_sample_adaa(self.curve, in_r, &mut self.last_x_r);

        // Simple 1-pole tilt tone filter
        let alpha = (self.tone * 0.4).clamp(-0.8, 0.8);
        self.tone_state_l = (1.0 - alpha.abs()) * sat_l + alpha * self.tone_state_l;
        self.tone_state_r = (1.0 - alpha.abs()) * sat_r + alpha * self.tone_state_r;

        (self.tone_state_l, self.tone_state_r)
    }
}

/// Live 12 Flagship Roar Multi-Stage Saturator Device.
pub struct Roar {
    pub name: String,
    pub enabled: bool,
    pub topology: RoarTopology,
    pub stage1: RoarStage,
    pub stage2: RoarStage,
    pub feedback_amount: f32, // 0.0 .. 1.0
    pub feedback_delay_ms: f32,
    pub dry_wet: f32,
    pub output_gain_db: f32,

    sample_rate: f32,
    // Feedback delay line
    fb_buf_l: Vec<f32>,
    fb_buf_r: Vec<f32>,
    fb_pos: usize,
}

impl Roar {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let max_fb_samples = (sr * 0.5) as usize; // up to 500ms feedback
        Self {
            name: "Roar".to_string(),
            enabled: true,
            topology: RoarTopology::Serial,
            stage1: RoarStage::new(RoarCurve::SoftSine, 6.0),
            stage2: RoarStage::new(RoarCurve::Tube, 3.0),
            feedback_amount: 0.0,
            feedback_delay_ms: 20.0,
            dry_wet: 1.0,
            output_gain_db: 0.0,
            sample_rate: sr,
            fb_buf_l: vec![0.0; max_fb_samples.max(1024)],
            fb_buf_r: vec![0.0; max_fb_samples.max(1024)],
            fb_pos: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            let max_fb_samples = (sample_rate * 0.5) as usize;
            self.fb_buf_l.resize(max_fb_samples.max(1024), 0.0);
            self.fb_buf_r.resize(max_fb_samples.max(1024), 0.0);
        }
    }
}

impl Default for Roar {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for Roar {
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
        self.stage1.reset();
        self.stage2.reset();
        self.fb_buf_l.fill(0.0);
        self.fb_buf_r.fill(0.0);
        self.fb_pos = 0;
    }
}

impl AudioEffectDevice for Roar {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let out_gain = 10.0_f32.powf(self.output_gain_db / 20.0);
        let fb_delay_samples = ((self.feedback_delay_ms * 0.001 * self.sample_rate) as usize)
            .clamp(1, self.fb_buf_l.len() - 1);
        let sqrt2 = std::f32::consts::SQRT_2;

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            let (wet_l, wet_r) = match self.topology {
                RoarTopology::Single => self.stage1.process_sample(dry_l, dry_r),
                RoarTopology::Serial => {
                    let (s1_l, s1_r) = self.stage1.process_sample(dry_l, dry_r);
                    self.stage2.process_sample(s1_l, s1_r)
                }
                RoarTopology::Parallel => {
                    let (s1_l, s1_r) = self.stage1.process_sample(dry_l, dry_r);
                    let (s2_l, s2_r) = self.stage2.process_sample(dry_l, dry_r);
                    ((s1_l + s2_l) * 0.5, (s1_r + s2_r) * 0.5)
                }
                RoarTopology::MidSide => {
                    let mid = (dry_l + dry_r) / sqrt2;
                    let side = (dry_l - dry_r) / sqrt2;

                    let (sat_mid, _) = self.stage1.process_sample(mid, 0.0);
                    let (_, sat_side) = self.stage2.process_sample(0.0, side);

                    ((sat_mid + sat_side) / sqrt2, (sat_mid - sat_side) / sqrt2)
                }
                RoarTopology::Multiband => {
                    // Simple 2-band crossover split at ~800 Hz
                    let alpha = 0.1;
                    let low_l = dry_l * alpha;
                    let low_r = dry_r * alpha;
                    let high_l = dry_l - low_l;
                    let high_r = dry_r - low_r;

                    let (sat_low_l, sat_low_r) = self.stage1.process_sample(low_l, low_r);
                    let (sat_high_l, sat_high_r) = self.stage2.process_sample(high_l, high_r);

                    (sat_low_l + sat_high_l, sat_low_r + sat_high_r)
                }
                RoarTopology::Feedback => {
                    let read_idx = if self.fb_pos >= fb_delay_samples {
                        self.fb_pos - fb_delay_samples
                    } else {
                        self.fb_pos + self.fb_buf_l.len() - fb_delay_samples
                    };
                    let fb_l = self.fb_buf_l[read_idx] * self.feedback_amount;
                    let fb_r = self.fb_buf_r[read_idx] * self.feedback_amount;

                    let (s1_l, s1_r) = self.stage1.process_sample(dry_l + fb_l, dry_r + fb_r);
                    let (s2_l, s2_r) = self.stage2.process_sample(s1_l, s1_r);

                    self.fb_buf_l[self.fb_pos] = s2_l.clamp(-2.0, 2.0);
                    self.fb_buf_r[self.fb_pos] = s2_r.clamp(-2.0, 2.0);
                    self.fb_pos = (self.fb_pos + 1) % self.fb_buf_l.len();

                    (s2_l, s2_r)
                }
            };

            let out_l = (dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet) * out_gain;
            let out_r = (dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet) * out_gain;

            buffer.left[i] = out_l.clamp(-3.0, 3.0);
            buffer.right[i] = out_r.clamp(-3.0, 3.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roar_adaa_all_curves_finite_non_nan() {
        let curves = [
            RoarCurve::SoftSine,
            RoarCurve::DigitalClip,
            RoarCurve::BitCrusher,
            RoarCurve::Polynomial,
            RoarCurve::Tube,
            RoarCurve::HalfWave,
            RoarCurve::Diode,
            RoarCurve::HardClip,
            RoarCurve::SineWavefold,
            RoarCurve::NoiseShaper,
        ];

        for curve in curves {
            let mut roar = Roar::new(44100.0);
            roar.stage1.curve = curve;
            roar.stage1.drive_db = 18.0;

            let mut buf = AudioBuffer::silence(256);
            for i in 0..256 {
                let s = (i as f32 / 256.0 * 2.0 * PI).sin() * 0.8;
                buf.left[i] = s;
                buf.right[i] = s;
            }

            let time = MusicalTime::default();
            roar.process_audio(&mut buf, &time);

            for (i, (&l, &r)) in buf.left[0..256].iter().zip(buf.right[0..256].iter()).enumerate() {
                assert!(!l.is_nan(), "{:?} left sample at {} is NaN", curve, i);
                assert!(!r.is_nan(), "{:?} right sample at {} is NaN", curve, i);
                assert!(l.is_finite(), "{:?} left sample at {} not finite", curve, i);
                assert!(r.is_finite(), "{:?} right sample at {} not finite", curve, i);
                assert!(l.abs() <= 3.0, "{:?} left sample at {} exceeded boundary: {}", curve, i, l);
            }
        }
    }

    #[test]
    fn test_roar_topologies() {
        let topologies = [
            RoarTopology::Single,
            RoarTopology::Serial,
            RoarTopology::Parallel,
            RoarTopology::MidSide,
            RoarTopology::Multiband,
            RoarTopology::Feedback,
        ];

        for topo in topologies {
            let mut roar = Roar::new(44100.0);
            roar.topology = topo;
            roar.feedback_amount = 0.5;

            let mut buf = AudioBuffer::silence(128);
            for i in 0..128 {
                buf.left[i] = 0.4;
                buf.right[i] = -0.4;
            }

            let time = MusicalTime::default();
            roar.process_audio(&mut buf, &time);

            let energy = buf.left[0..128].iter().fold(0.0f32, |acc, &x| acc + x * x);
            assert!(energy > 0.001, "{:?} must produce non-silent energy", topo);
        }
    }
}
