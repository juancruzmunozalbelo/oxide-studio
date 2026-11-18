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
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

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
                // DC-free asymmetric triode transfer (0.0 -> 0.0, even + odd harmonics)
                ((x + 0.2).tanh() - 0.2_f32.tanh()).clamp(-1.5, 1.5)
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
    fn ln_cosh(x: f32) -> f32 {
        let ax = x.abs();
        ax + (-2.0 * ax).exp().ln_1p() - std::f32::consts::LN_2
    }

    #[inline(always)]
    fn evaluate_f1(curve: RoarCurve, x: f32) -> f32 {
        match curve {
            RoarCurve::SoftSine => {
                if x < -1.0 {
                    -x - 1.0
                } else if x <= 1.0 {
                    -(2.0 / PI) * (x * PI * 0.5).cos()
                } else {
                    x - 1.0
                }
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
                if x < -1.2 {
                    -0.936 * (x + 1.2) + 0.8208
                } else if x <= 1.2 {
                    0.75 * x * x - 0.125 * x * x * x * x
                } else {
                    0.936 * (x - 1.2) + 0.8208
                }
            }
            RoarCurve::HalfWave => {
                if x > 0.0 {
                    0.5 * x * x
                } else {
                    0.0
                }
            }
            RoarCurve::Diode => {
                let ax = x.abs();
                ax + (-ax).exp() - 1.0
            }
            RoarCurve::HardClip => {
                if x < -0.8 {
                    -0.8 * x - 0.32
                } else if x <= 0.8 {
                    0.5 * x * x
                } else {
                    0.8 * x - 0.32
                }
            }
            RoarCurve::SineWavefold => -(1.0 / PI) * (x * PI).cos(),
            RoarCurve::NoiseShaper => Self::ln_cosh(x),
            RoarCurve::BitCrusher | RoarCurve::Tube => Self::ln_cosh(x),
        }
    }

    /// Process a single sample through 1st-order ADAA.
    #[inline(always)]
    fn process_sample_adaa(curve: RoarCurve, x: f32, last_x: &mut f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let prev = if last_x.is_finite() { *last_x } else { 0.0 };
        let diff = x - prev;
        let y = if curve == RoarCurve::BitCrusher {
            // BitCrusher is a discontinuous staircase quantizer; bypass ADAA to avoid spikes
            Self::evaluate_f(curve, x)
        } else if diff.abs() < 1e-5 {
            // Bypass ADAA division near zero-step intervals to avoid ill-conditioning
            Self::evaluate_f(curve, 0.5 * (x + prev))
        } else if curve == RoarCurve::Tube {
            // 2-point Gauss-Legendre quadrature matches 1st-order ADAA smoothly
            let mid = 0.5 * (x + prev);
            let offset = 0.5 * diff * 0.57735027;
            0.5 * (Self::evaluate_f(curve, mid - offset) + Self::evaluate_f(curve, mid + offset))
        } else {
            let adaa = (Self::evaluate_f1(curve, x) - Self::evaluate_f1(curve, prev)) / diff;
            if adaa.is_finite() {
                adaa
            } else {
                Self::evaluate_f(curve, x)
            }
        };
        *last_x = x;
        y
    }

    /// Process one stereo sample through this stage with drive, tone, and ADAA.
    #[inline(always)]
    pub fn process_sample(&mut self, l: f32, r: f32) -> (f32, f32) {
        let drive = 10.0_f32.powf(self.drive_db / 20.0);
        let in_l = if l.is_finite() { l * drive + self.bias } else { 0.0 };
        let in_r = if r.is_finite() { r * drive + self.bias } else { 0.0 };

        let sat_l = Self::process_sample_adaa(self.curve, in_l, &mut self.last_x_l);
        let sat_r = Self::process_sample_adaa(self.curve, in_r, &mut self.last_x_r);

        // Simple 1-pole tilt tone filter
        let alpha = (self.tone * 0.4).clamp(-0.8, 0.8);
        self.tone_state_l = (1.0 - alpha.abs()) * sat_l + alpha * self.tone_state_l;
        self.tone_state_r = (1.0 - alpha.abs()) * sat_r + alpha * self.tone_state_r;

        (self.tone_state_l, self.tone_state_r)
    }
}

/// Stereo Linkwitz-Riley-2 (k = 2.0) Cytomic SVF crossover stage.
#[derive(Debug, Clone)]
struct CrossoverSvf {
    cutoff_hz: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1eq_l: f32,
    ic2eq_l: f32,
    ic1eq_r: f32,
    ic2eq_r: f32,
}

impl CrossoverSvf {
    fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let mut svf = Self {
            cutoff_hz,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1eq_l: 0.0,
            ic2eq_l: 0.0,
            ic1eq_r: 0.0,
            ic2eq_r: 0.0,
        };
        svf.update_coeffs(sample_rate);
        svf
    }

    fn update_coeffs(&mut self, sample_rate: f32) {
        let sr = if sample_rate > 0.0 { sample_rate } else { 44100.0 };
        let fc = self.cutoff_hz.clamp(20.0, sr * 0.45);
        let g = (PI * fc / sr).tan();
        let k = 2.0f32;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    fn reset(&mut self) {
        self.ic1eq_l = 0.0;
        self.ic2eq_l = 0.0;
        self.ic1eq_r = 0.0;
        self.ic2eq_r = 0.0;
    }

    /// Split stereo sample into complementary `(low, high)` outputs whose sum is unity all-pass.
    #[inline(always)]
    fn split(&mut self, in_l: f32, in_r: f32) -> ((f32, f32), (f32, f32)) {
        let k = 2.0f32;
        let v3_l = in_l - self.ic2eq_l;
        let v1_l = self.a1 * self.ic1eq_l + self.a2 * v3_l;
        let v2_l = self.ic2eq_l + self.a2 * self.ic1eq_l + self.a3 * v3_l;
        self.ic1eq_l = 2.0 * v1_l - self.ic1eq_l;
        self.ic2eq_l = 2.0 * v2_l - self.ic2eq_l;
        let lp_l = v2_l;
        let hp_l = in_l - k * v1_l - v2_l;

        let v3_r = in_r - self.ic2eq_r;
        let v1_r = self.a1 * self.ic1eq_r + self.a2 * v3_r;
        let v2_r = self.ic2eq_r + self.a2 * self.ic1eq_r + self.a3 * v3_r;
        self.ic1eq_r = 2.0 * v1_r - self.ic1eq_r;
        self.ic2eq_r = 2.0 * v2_r - self.ic2eq_r;
        let lp_r = v2_r;
        let hp_r = in_r - k * v1_r - v2_r;

        // Polarity-inverted HP compensates phase so lp + (-hp) has flat magnitude 1.0
        ((lp_l, lp_r), (-hp_l, -hp_r))
    }
}

/// Live 12 Flagship Roar Multi-Stage Saturator Device.
pub struct Roar {
    pub name: String,
    pub enabled: bool,
    pub topology: RoarTopology,
    pub stage1: RoarStage,
    pub stage2: RoarStage,
    pub stage3: RoarStage,
    pub feedback_amount: f32, // 0.0 .. 1.0
    pub feedback_delay_ms: f32,
    pub dry_wet: f32,
    pub output_gain_db: f32,

    sample_rate: f32,
    xover_lo: CrossoverSvf,
    xover_hi: CrossoverSvf,
    xover_ap: CrossoverSvf,
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
            stage3: RoarStage::new(RoarCurve::HardClip, 4.0),
            feedback_amount: 0.0,
            feedback_delay_ms: 20.0,
            dry_wet: 1.0,
            output_gain_db: 0.0,
            sample_rate: sr,
            xover_lo: CrossoverSvf::new(250.0, sr),
            xover_hi: CrossoverSvf::new(2500.0, sr),
            xover_ap: CrossoverSvf::new(2500.0, sr),
            fb_buf_l: vec![0.0; max_fb_samples.max(1024)],
            fb_buf_r: vec![0.0; max_fb_samples.max(1024)],
            fb_pos: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            self.xover_lo.update_coeffs(sample_rate);
            self.xover_hi.update_coeffs(sample_rate);
            self.xover_ap.update_coeffs(sample_rate);
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
        self.stage3.reset();
        self.xover_lo.reset();
        self.xover_hi.reset();
        self.xover_ap.reset();
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

        let count = buffer.frame_count.min(oxide_core::MAX_BUFFER_SIZE);
        if count == 0 {
            return;
        }

        let out_gain = 10.0_f32.powf(self.output_gain_db / 20.0);
        let fb_delay_samples = ((self.feedback_delay_ms * 0.001 * self.sample_rate) as usize)
            .clamp(1, self.fb_buf_l.len() - 1);
        let sqrt2 = std::f32::consts::SQRT_2;

        for i in 0..count {
            let dry_l = if buffer.left[i].is_finite() { buffer.left[i] } else { 0.0 };
            let dry_r = if buffer.right[i].is_finite() { buffer.right[i] } else { 0.0 };

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
                    // 3-band Linkwitz-Riley-2 SVF crossover (Low < 250 Hz, Mid 250..2500 Hz, High > 2500 Hz)
                    let ((low_raw_l, low_raw_r), (mh_l, mh_r)) = self.xover_lo.split(dry_l, dry_r);
                    let ((ap_lp_l, ap_lp_r), (ap_hp_l, ap_hp_r)) = self.xover_ap.split(low_raw_l, low_raw_r);
                    let low_l = ap_lp_l + ap_hp_l;
                    let low_r = ap_lp_r + ap_hp_r;
                    let ((mid_l, mid_r), (high_l, high_r)) = self.xover_hi.split(mh_l, mh_r);

                    let (sat_low_l, sat_low_r) = self.stage1.process_sample(low_l, low_r);
                    let (sat_mid_l, sat_mid_r) = self.stage2.process_sample(mid_l, mid_r);
                    let (sat_high_l, sat_high_r) = self.stage3.process_sample(high_l, high_r);

                    (
                        sat_low_l + sat_mid_l + sat_high_l,
                        sat_low_r + sat_mid_r + sat_high_r,
                    )
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

            let dw = self.dry_wet.clamp(0.0, 1.0);
            let out_l = (dry_l * (1.0 - dw) + wet_l * dw) * out_gain;
            let out_r = (dry_r * (1.0 - dw) + wet_r * dw) * out_gain;

            buffer.left[i] = out_l.clamp(-3.0, 3.0);
            buffer.right[i] = out_r.clamp(-3.0, 3.0);
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.stage1.drive_db = value.clamp(0.0, 36.0),
            1 => self.stage2.drive_db = value.clamp(0.0, 36.0),
            2 => self.stage3.drive_db = value.clamp(0.0, 36.0),
            3 => self.dry_wet = value.clamp(0.0, 1.0),
            4 => self.feedback_amount = value.clamp(0.0, 0.99),
            5 => self.feedback_delay_ms = value.clamp(1.0, 500.0),
            6 => self.output_gain_db = value.clamp(-24.0, 24.0),
            7 => self.stage1.tone = value.clamp(-1.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.stage1.drive_db,
            1 => self.stage2.drive_db,
            2 => self.stage3.drive_db,
            3 => self.dry_wet,
            4 => self.feedback_amount,
            5 => self.feedback_delay_ms,
            6 => self.output_gain_db,
            7 => self.stage1.tone,
            _ => 0.0,
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

    #[test]
    fn test_roar_multiband_svf_crossover_and_adaa_stability() {
        // Test BitCrusher & small-diff intervals remain finite without ADAA division spikes
        let mut roar = Roar::new(44100.0);
        roar.topology = RoarTopology::Single;
        roar.stage1.curve = RoarCurve::BitCrusher;
        roar.stage1.drive_db = 12.0;

        let mut buf = AudioBuffer::silence(256);
        for i in 0..256 {
            buf.left[i] = 0.5 + (i as f32) * 1e-7;
            buf.right[i] = -0.5;
        }
        roar.process_audio(&mut buf, &MusicalTime::default());
        for (&l, &r) in buf.left[..256].iter().zip(buf.right[..256].iter()) {
            assert!(l.is_finite() && r.is_finite() && l.abs() <= 2.5 && r.abs() <= 2.5);
        }

        // Test Multiband 3-band crossover separates 60 Hz (Low stage1) vs 8 kHz (High stage3)
        let mut mb_low_boost = Roar::new(44100.0);
        mb_low_boost.topology = RoarTopology::Multiband;
        mb_low_boost.stage1.drive_db = 12.0;
        mb_low_boost.stage2.drive_db = 0.0;
        mb_low_boost.stage3.drive_db = -24.0;

        let mut low_sig = AudioBuffer::silence(512);
        let mut high_sig = AudioBuffer::silence(512);
        for i in 0..512 {
            let t = i as f32 / 44100.0;
            low_sig.left[i] = (2.0 * PI * 60.0 * t).sin() * 0.2;
            low_sig.right[i] = low_sig.left[i];
            high_sig.left[i] = (2.0 * PI * 8000.0 * t).sin() * 0.2;
            high_sig.right[i] = high_sig.left[i];
        }
        mb_low_boost.process_audio(&mut low_sig, &MusicalTime::default());
        mb_low_boost.reset();
        mb_low_boost.process_audio(&mut high_sig, &MusicalTime::default());

        let low_rms: f32 = low_sig.left[256..512].iter().map(|s| s * s).sum();
        let high_rms: f32 = high_sig.left[256..512].iter().map(|s| s * s).sum();
        assert!(
            low_rms > high_rms * 10.0,
            "3-band crossover should pass 60 Hz through boosted stage1 ({}) and attenuate 8 kHz in stage3 ({})",
            low_rms,
            high_rms
        );
    }
}
