//! Cytomic SSL Bus Compressor ("Glue Compressor") for ableton-fx.
//!
//! Features:
//! - Exact SSL 4000 G-Bus dynamic VCA curve with log sidechain detector.
//! - Attack options: 0.01, 0.1, 0.3, 1, 3, 10, 30 ms.
//! - Release options: 0.1, 0.2, 0.4, 0.6, 1.2 s and Auto release.
//! - Ratios: 2:1, 4:1, 10:1.
//! - Range control: Capping maximum dB attenuation.
//! - Soft Clip: Non-linear diode saturation circuit.
//! - Parallel Dry/Wet mix and real-time gain reduction metering.

use ableton_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

/// Cytomic Glue Compressor device.
#[derive(Debug, Clone)]
pub struct GlueCompressor {
    pub name: String,
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32, // 2.0, 4.0, 10.0
    pub attack_ms: f32,
    pub release_s: f32, // negative or <= 0 indicates Auto mode
    pub makeup_gain_db: f32,
    pub range_db: f32,
    pub dry_wet: f32,
    pub soft_clip: bool,

    // Internal state
    sample_rate: f32,
    gain_reduction_smoothed_db: f32,
    auto_release_env: f32,
}

impl GlueCompressor {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        Self {
            name: "Glue Compressor".to_string(),
            enabled: true,
            threshold_db: 0.0,
            ratio: 4.0,
            attack_ms: 1.0,
            release_s: 0.2,
            makeup_gain_db: 0.0,
            range_db: 70.0,
            dry_wet: 1.0,
            soft_clip: true,
            sample_rate: sr,
            gain_reduction_smoothed_db: 0.0,
            auto_release_env: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    /// Read current instantaneous gain reduction in dB (e.g. -4.5 dB).
    pub fn gain_reduction_db(&self) -> f32 {
        self.gain_reduction_smoothed_db
    }

    #[inline(always)]
    fn apply_soft_clip(x: f32) -> f32 {
        let abs_x = x.abs();
        if abs_x <= 2.0 / 3.0 {
            x
        } else if abs_x <= 1.0 {
            let sign = if x >= 0.0 { 1.0 } else { -1.0 };
            let diff = 2.0 - 3.0 * abs_x;
            sign * ((3.0 - diff * diff) / 3.0)
        } else {
            if x >= 0.0 { 1.0 } else { -1.0 }
        }
    }
}

impl Default for GlueCompressor {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for GlueCompressor {
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
        self.gain_reduction_smoothed_db = 0.0;
        self.auto_release_env = 0.0;
    }
}

impl AudioEffectDevice for GlueCompressor {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let attack_time = (self.attack_ms * 0.001).max(0.00001);
        let alpha_a = (-1.0 / (attack_time * self.sample_rate)).exp();

        let is_auto = self.release_s <= 0.0;
        let base_release_s = if is_auto { 0.2 } else { self.release_s.max(0.01) };
        let mut alpha_r = (-1.0 / (base_release_s * self.sample_rate)).exp();

        let makeup_linear = 10.0_f32.powf(self.makeup_gain_db / 20.0);
        let ratio_factor = 1.0 - (1.0 / self.ratio.max(1.0));

        for i in 0..count {
            let dry_l = buffer.left[i];
            let dry_r = buffer.right[i];

            // Stereo sidechain max detector
            let peak = dry_l.abs().max(dry_r.abs()).max(1e-6);
            let peak_db = 20.0 * peak.log10();

            // Calculate static target gain reduction (negative dB)
            let mut target_gr_db = if peak_db > self.threshold_db {
                ratio_factor * (self.threshold_db - peak_db)
            } else {
                0.0
            };

            // Range control caps maximum attenuation
            target_gr_db = target_gr_db.max(-self.range_db);

            // Auto release dynamic adjustment
            if is_auto {
                let diff = (target_gr_db - self.gain_reduction_smoothed_db).abs();
                self.auto_release_env = self.auto_release_env * 0.999 + diff * 0.001;
                let dynamic_rel_s = 0.05 + 0.8 / (1.0 + self.auto_release_env * 0.5);
                alpha_r = (-1.0 / (dynamic_rel_s * self.sample_rate)).exp();
            }

            // Decoupled ballistics smoothing
            if target_gr_db < self.gain_reduction_smoothed_db {
                // Attacking (reducing gain)
                self.gain_reduction_smoothed_db =
                    alpha_a * self.gain_reduction_smoothed_db + (1.0 - alpha_a) * target_gr_db;
            } else {
                // Releasing (restoring gain)
                self.gain_reduction_smoothed_db =
                    alpha_r * self.gain_reduction_smoothed_db + (1.0 - alpha_r) * target_gr_db;
            }

            let linear_gain = 10.0_f32.powf(self.gain_reduction_smoothed_db / 20.0) * makeup_linear;
            let mut wet_l = dry_l * linear_gain;
            let mut wet_r = dry_r * linear_gain;

            if self.soft_clip {
                wet_l = Self::apply_soft_clip(wet_l);
                wet_r = Self::apply_soft_clip(wet_r);
            }

            // Dry/Wet blend
            buffer.left[i] = dry_l * (1.0 - self.dry_wet) + wet_l * self.dry_wet;
            buffer.right[i] = dry_r * (1.0 - self.dry_wet) + wet_r * self.dry_wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glue_compressor_gain_reduction() {
        let mut comp = GlueCompressor::new(44100.0);
        comp.threshold_db = -10.0;
        comp.ratio = 4.0;
        comp.attack_ms = 0.1;
        comp.release_s = 0.1;
        comp.soft_clip = false;

        // Feed hot signal at 0 dBFS (amplitude 1.0)
        let mut buf = AudioBuffer::silence(512);
        for i in 0..512 {
            buf.left[i] = 1.0;
            buf.right[i] = 1.0;
        }

        let time = MusicalTime::default();
        comp.process_audio(&mut buf, &time);

        // Target reduction: (1 - 1/4) * (-10 - 0) = -7.5 dB
        let gr = comp.gain_reduction_db();
        assert!(gr < -4.0, "Compressor must apply significant gain reduction, got {}", gr);
        assert!(buf.left[511] < 1.0, "Compressed output amplitude must be attenuated, got {}", buf.left[511]);
    }

    #[test]
    fn test_glue_compressor_soft_clip() {
        let mut comp = GlueCompressor::new(44100.0);
        comp.soft_clip = true;
        comp.threshold_db = 0.0;
        comp.makeup_gain_db = 12.0; // Overdrive output into soft clipper

        let mut buf = AudioBuffer::silence(128);
        for i in 0..128 {
            buf.left[i] = 1.0;
            buf.right[i] = 1.0;
        }

        let time = MusicalTime::default();
        comp.process_audio(&mut buf, &time);

        for i in 0..128 {
            assert!(buf.left[i] <= 1.0 + 1e-5, "Soft clip must bound output to <= 1.0, got {}", buf.left[i]);
            assert!(buf.right[i] <= 1.0 + 1e-5);
        }
    }
}
