//! Live 12 Utility Device.
//!
//! Master utility tool providing Gain dB, Pan, Stereo Width (0-400%), Bass Mono crossover,
//! Phase Invert L/R, and DC offset blocking.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, AudioEffectDevice, Device, DeviceCategory, MusicalTime};

pub struct Utility {
    pub name: String,
    pub enabled: bool,
    pub gain_db: f32,          // -inf .. +35.0 dB
    pub pan: f32,              // -1.0 (Left) .. +1.0 (Right)
    pub width: f32,            // 0.0 (Mono) .. 1.0 (Normal) .. 4.0 (Super Wide)
    pub bass_mono: bool,
    pub bass_mono_freq_hz: f32, // typically 120 Hz
    pub phase_invert_l: bool,
    pub phase_invert_r: bool,
    pub mute: bool,

    sample_rate: f32,
    dc_x1_l: f32,
    dc_y1_l: f32,
    dc_x1_r: f32,
    dc_y1_r: f32,
    bass_lp_l: f32,
    bass_lp_r: f32,
}

impl Utility {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        Self {
            name: "Utility".to_string(),
            enabled: true,
            gain_db: 0.0,
            pan: 0.0,
            width: 1.0,
            bass_mono: false,
            bass_mono_freq_hz: 120.0,
            phase_invert_l: false,
            phase_invert_r: false,
            mute: false,
            sample_rate: sr,
            dc_x1_l: 0.0,
            dc_y1_l: 0.0,
            dc_x1_r: 0.0,
            dc_y1_r: 0.0,
            bass_lp_l: 0.0,
            bass_lp_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }
}

impl Default for Utility {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl Device for Utility {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::AudioEffect
    }

    fn is_enabled(&self) -> bool {
        self.enabled && !self.mute
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self) {
        self.dc_x1_l = 0.0;
        self.dc_y1_l = 0.0;
        self.dc_x1_r = 0.0;
        self.dc_y1_r = 0.0;
        self.bass_lp_l = 0.0;
        self.bass_lp_r = 0.0;
    }
}

impl AudioEffectDevice for Utility {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, _time: &MusicalTime) {
        if !self.enabled {
            return;
        }
        if self.mute {
            buffer.clear();
            return;
        }

        let count = buffer.frame_count.min(1024);
        if count == 0 {
            return;
        }

        let linear_gain = 10.0_f32.powf(self.gain_db / 20.0);
        let pan_angle = (self.pan.clamp(-1.0, 1.0) + 1.0) * (PI * 0.25);
        let pan_l = pan_angle.cos();
        let pan_r = pan_angle.sin();

        let sqrt2 = std::f32::consts::SQRT_2;
        let bass_coeff = (2.0 * PI * self.bass_mono_freq_hz / self.sample_rate).clamp(0.001, 0.5);

        for i in 0..count {
            let mut l = buffer.left[i];
            let mut r = buffer.right[i];

            // 1. DC Blocking filter (R = 0.995)
            let dc_out_l = l - self.dc_x1_l + 0.995 * self.dc_y1_l;
            self.dc_x1_l = l;
            self.dc_y1_l = dc_out_l;
            l = dc_out_l;

            let dc_out_r = r - self.dc_x1_r + 0.995 * self.dc_y1_r;
            self.dc_x1_r = r;
            self.dc_y1_r = dc_out_r;
            r = dc_out_r;

            // 2. Phase Inversion
            if self.phase_invert_l {
                l = -l;
            }
            if self.phase_invert_r {
                r = -r;
            }

            // 3. Stereo Width via Mid/Side matrixing
            let mid = (l + r) / sqrt2;
            let side = (l - r) / sqrt2 * self.width;

            l = (mid + side) / sqrt2;
            r = (mid - side) / sqrt2;

            // 4. Bass Mono
            if self.bass_mono {
                self.bass_lp_l = self.bass_lp_l * (1.0 - bass_coeff) + l * bass_coeff;
                self.bass_lp_r = self.bass_lp_r * (1.0 - bass_coeff) + r * bass_coeff;

                let mono_bass = (self.bass_lp_l + self.bass_lp_r) * 0.5;
                let high_l = l - self.bass_lp_l;
                let high_r = r - self.bass_lp_r;

                l = high_l + mono_bass;
                r = high_r + mono_bass;
            }

            // 5. Gain & Pan
            buffer.left[i] = l * linear_gain * pan_l;
            buffer.right[i] = r * linear_gain * pan_r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utility_stereo_width_mono_and_gain() {
        let mut u = Utility::new(44100.0);
        u.gain_db = 6.0; // ~2.0x gain
        u.width = 0.0;   // Collapse to mono

        let mut buf = AudioBuffer::silence(64);
        buf.left[0] = 1.0;
        buf.right[0] = 0.0;

        let time = MusicalTime::default();
        u.process_audio(&mut buf, &time);

        // Mono width: (1.0 + 0.0)/2 = 0.5. With +6dB (~1.995x) and centered pan (cos(pi/4) = 0.7071)
        // L and R should be identical!
        assert!((buf.left[0] - buf.right[0]).abs() < 1e-4);
        assert!(buf.left[0] > 0.5);
    }
}
