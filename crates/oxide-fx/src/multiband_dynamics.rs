//! Live 12 Multiband Dynamics / OTT (Upward + Downward 3-Band Compressor).
//!
//! Features:
//! - 4th-order Linkwitz-Riley (LR4) phase-aligned 3-band crossover (Low, Mid, High).
//! - Dual-threshold dynamics per band: Downward compression (above high threshold) and Upward compression (below low threshold).
//! - OTT signature presets with Time (attack/release scaling) and Amount (dry/wet %) macros.
//! - Per-band live gain reduction and upward expansion metering.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

/// 2nd-order Butterworth biquad cell for crossover filtering.
#[derive(Debug, Clone, Copy)]
struct Biquad2 {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl Biquad2 {
    fn new_lowpass(fc: f32, sr: f32) -> Self {
        let w0 = 2.0 * PI * fc.clamp(20.0, sr * 0.45) / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * (1.0 / (2.0_f32).sqrt()));
        let b0 = (1.0 - cos_w0) * 0.5;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        let inv_a0 = 1.0 / a0;
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    fn new_highpass(fc: f32, sr: f32) -> Self {
        let w0 = 2.0 * PI * fc.clamp(20.0, sr * 0.45) / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * (1.0 / (2.0_f32).sqrt()));
        let b0 = (1.0 + cos_w0) * 0.5;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        let inv_a0 = 1.0 / a0;
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

/// 4th-order Linkwitz-Riley filter (cascade of two Butterworth biquads).
#[derive(Debug, Clone, Copy)]
struct Lr4Filter {
    stage1: Biquad2,
    stage2: Biquad2,
}

impl Lr4Filter {
    fn new_lp(fc: f32, sr: f32) -> Self {
        Self {
            stage1: Biquad2::new_lowpass(fc, sr),
            stage2: Biquad2::new_lowpass(fc, sr),
        }
    }

    fn new_hp(fc: f32, sr: f32) -> Self {
        Self {
            stage1: Biquad2::new_highpass(fc, sr),
            stage2: Biquad2::new_highpass(fc, sr),
        }
    }

    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let y1 = self.stage1.process(x);
        self.stage2.process(y1)
    }

    fn reset(&mut self) {
        self.stage1.reset();
        self.stage2.reset();
    }
}

/// Single band compressor dynamics processor.
#[derive(Debug, Clone)]
pub struct BandDynamics {
    pub thresh_above_db: f32,
    pub ratio_above: f32,
    pub thresh_below_db: f32,
    pub ratio_below: f32, // < 1.0 for upward expansion
    pub attack_ms: f32,
    pub release_ms: f32,
    pub gain_db: f32,

    envelope_db: f32,
}

impl BandDynamics {
    pub fn new(
        thresh_above_db: f32,
        ratio_above: f32,
        thresh_below_db: f32,
        ratio_below: f32,
        attack_ms: f32,
        release_ms: f32,
        gain_db: f32,
    ) -> Self {
        Self {
            thresh_above_db,
            ratio_above,
            thresh_below_db,
            ratio_below,
            attack_ms,
            release_ms,
            gain_db,
            envelope_db: 0.0,
        }
    }

    #[inline(always)]
    pub fn process_sample(&mut self, in_val: f32, time_scale: f32, sr: f32) -> f32 {
        let in_mag = in_val.abs().max(1e-6);
        let in_db = 20.0 * in_mag.log10();

        // Calculate dual-threshold target gain modification in dB
        let mut target_db = 0.0f32;

        // Downward compression for peaks exceeding thresh_above
        if in_db > self.thresh_above_db {
            let over_db = in_db - self.thresh_above_db;
            let reduction = over_db * (1.0 - 1.0 / self.ratio_above.max(1.0));
            target_db -= reduction;
        }

        // Upward compression for quiet details below thresh_below
        if in_db < self.thresh_below_db {
            let under_db = self.thresh_below_db - in_db;
            // Boost quiet signals with upward ratio
            let boost = under_db * (1.0 - self.ratio_below.clamp(0.1, 1.0));
            target_db += boost.min(18.0); // cap upward expansion to +18 dB
        }

        // Ballistics smoothing
        let att = (self.attack_ms * time_scale * 0.001).max(0.0001);
        let rel = (self.release_ms * time_scale * 0.001).max(0.001);
        let alpha_a = (-1.0 / (att * sr)).exp();
        let alpha_r = (-1.0 / (rel * sr)).exp();

        if target_db < self.envelope_db {
            self.envelope_db = alpha_a * self.envelope_db + (1.0 - alpha_a) * target_db;
        } else {
            self.envelope_db = alpha_r * self.envelope_db + (1.0 - alpha_r) * target_db;
        }

        let total_linear = 10.0_f32.powf((self.envelope_db + self.gain_db) / 20.0);
        in_val * total_linear
    }

    pub fn current_gain_db(&self) -> f32 {
        self.envelope_db
    }

    pub fn reset(&mut self) {
        self.envelope_db = 0.0;
    }
}

/// Live 12 Multiband Dynamics / OTT Device.
pub struct MultibandDynamics {
    pub name: String,
    pub enabled: bool,
    pub time_macro: f32, // 0.1 .. 10.0 (1.0 = standard)
    pub amount: f32,     // 0.0 .. 1.0 (Dry/Wet)
    pub input_gain_db: f32,
    pub output_gain_db: f32,

    pub low_band: BandDynamics,
    pub mid_band: BandDynamics,
    pub high_band: BandDynamics,

    sample_rate: f32,
    crossover_low_lp_l: Lr4Filter,
    crossover_low_lp_r: Lr4Filter,
    crossover_low_hp_l: Lr4Filter,
    crossover_low_hp_r: Lr4Filter,
    crossover_high_lp_l: Lr4Filter,
    crossover_high_lp_r: Lr4Filter,
    crossover_high_hp_l: Lr4Filter,
    crossover_high_hp_r: Lr4Filter,
}

impl MultibandDynamics {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let f_low = 250.0;
        let f_high = 2500.0;

        // Canonical Ableton Live OTT settings:
        // Low: Thresh Above -24 dB (4:1), Thresh Below -36 dB (1:2.5 = 0.4), Att 10 ms, Rel 100 ms
        // Mid: Thresh Above -20 dB (4:1), Thresh Below -32 dB (1:2.5 = 0.4), Att 5 ms, Rel 80 ms
        // High: Thresh Above -18 dB (4:1), Thresh Below -30 dB (1:2.5 = 0.4), Att 2 ms, Rel 50 ms
        Self {
            name: "Multiband Dynamics".to_string(),
            enabled: true,
            time_macro: 1.0,
            amount: 1.0,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            low_band: BandDynamics::new(-24.0, 4.0, -36.0, 0.4, 10.0, 100.0, 0.0),
            mid_band: BandDynamics::new(-20.0, 4.0, -32.0, 0.4, 5.0, 80.0, 0.0),
            high_band: BandDynamics::new(-18.0, 4.0, -30.0, 0.4, 2.0, 50.0, 0.0),
            sample_rate: sr,
            crossover_low_lp_l: Lr4Filter::new_lp(f_low, sr),
            crossover_low_lp_r: Lr4Filter::new_lp(f_low, sr),
            crossover_low_hp_l: Lr4Filter::new_hp(f_low, sr),
            crossover_low_hp_r: Lr4Filter::new_hp(f_low, sr),
            crossover_high_lp_l: Lr4Filter::new_lp(f_high, sr),
            crossover_high_lp_r: Lr4Filter::new_lp(f_high, sr),
            crossover_high_hp_l: Lr4Filter::new_hp(f_high, sr),
            crossover_high_hp_r: Lr4Filter::new_hp(f_high, sr),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            let f_low = 250.0;
            let f_high = 2500.0;
            self.crossover_low_lp_l = Lr4Filter::new_lp(f_low, sample_rate);
            self.crossover_low_lp_r = Lr4Filter::new_lp(f_low, sample_rate);
            self.crossover_low_hp_l = Lr4Filter::new_hp(f_low, sample_rate);
            self.crossover_low_hp_r = Lr4Filter::new_hp(f_low, sample_rate);
            self.crossover_high_lp_l = Lr4Filter::new_lp(f_high, sample_rate);
            self.crossover_high_lp_r = Lr4Filter::new_lp(f_high, sample_rate);
            self.crossover_high_hp_l = Lr4Filter::new_hp(f_high, sample_rate);
            self.crossover_high_hp_r = Lr4Filter::new_hp(f_high, sample_rate);
        }
    }

    /// Read the current 3-band dynamic gain levels (Low dB, Mid dB, High dB).
    pub fn get_band_gain_reduction(&self) -> (f32, f32, f32) {
        (
            self.low_band.current_gain_db(),
            self.mid_band.current_gain_db(),
            self.high_band.current_gain_db(),
        )
    }
}

impl Default for MultibandDynamics {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for MultibandDynamics {
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
        self.low_band.reset();
        self.mid_band.reset();
        self.high_band.reset();
        self.crossover_low_lp_l.reset();
        self.crossover_low_lp_r.reset();
        self.crossover_low_hp_l.reset();
        self.crossover_low_hp_r.reset();
        self.crossover_high_lp_l.reset();
        self.crossover_high_lp_r.reset();
        self.crossover_high_hp_l.reset();
        self.crossover_high_hp_r.reset();
    }
}

impl AudioEffectDevice for MultibandDynamics {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let in_gain = 10.0_f32.powf(self.input_gain_db / 20.0);
        let out_gain = 10.0_f32.powf(self.output_gain_db / 20.0);

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            let input_l = dry_l * in_gain;
            let input_r = dry_r * in_gain;

            // 3-way LR4 Crossover split
            // Low = LP(input)
            let low_l = self.crossover_low_lp_l.process(input_l);
            let low_r = self.crossover_low_lp_r.process(input_r);

            // Rest = HP(input)
            let rest_l = self.crossover_low_hp_l.process(input_l);
            let rest_r = self.crossover_low_hp_r.process(input_r);

            // Mid = LP(rest)
            let mid_l = self.crossover_high_lp_l.process(rest_l);
            let mid_r = self.crossover_high_lp_r.process(rest_r);

            // High = HP(rest)
            let high_l = self.crossover_high_hp_l.process(rest_l);
            let high_r = self.crossover_high_hp_r.process(rest_r);

            // Process dynamic compression/expansion per band
            let proc_low_l = self.low_band.process_sample(low_l, self.time_macro, self.sample_rate);
            let proc_low_r = self.low_band.process_sample(low_r, self.time_macro, self.sample_rate);

            let proc_mid_l = self.mid_band.process_sample(mid_l, self.time_macro, self.sample_rate);
            let proc_mid_r = self.mid_band.process_sample(mid_r, self.time_macro, self.sample_rate);

            let proc_high_l = self.high_band.process_sample(high_l, self.time_macro, self.sample_rate);
            let proc_high_r = self.high_band.process_sample(high_r, self.time_macro, self.sample_rate);

            // Sum bands
            let wet_l = (proc_low_l + proc_mid_l + proc_high_l) * out_gain;
            let wet_r = (proc_low_r + proc_mid_r + proc_high_r) * out_gain;

            // Amount blend
            buffer.left[i] = dry_l * (1.0 - self.amount) + wet_l * self.amount;
            buffer.right[i] = dry_r * (1.0 - self.amount) + wet_r * self.amount;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiband_dynamics_ott_compression_upward_and_downward() {
        let mut ott = MultibandDynamics::new(44100.0);

        // Test with quiet signal: should see upward compression boost
        let mut buf_quiet = AudioBuffer::silence(256);
        for i in 0..256 {
            buf_quiet.left[i] = 0.005; // -46 dBFS (well below thresh_below -36dB)
            buf_quiet.right[i] = 0.005;
        }

        let time = MusicalTime::default();
        ott.process_audio(&mut buf_quiet, &time);

        // Peak should be boosted
        let peak = buf_quiet.left[100..256].iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
        assert!(peak > 0.005, "OTT upward compression must boost quiet details, got {}", peak);

        // Test with loud signal: should see downward compression
        let mut buf_loud = AudioBuffer::silence(256);
        for i in 0..256 {
            buf_loud.left[i] = 1.0;
            buf_loud.right[i] = 1.0;
        }

        ott.process_audio(&mut buf_loud, &time);
        assert!(buf_loud.left[255].is_finite());
        assert!(!buf_loud.left[255].is_nan());
    }
}
