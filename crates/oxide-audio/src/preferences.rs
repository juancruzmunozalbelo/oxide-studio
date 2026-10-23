//! Audio Hardware Preferences, Buffer Sizing, Sample Rate Configuration & CPU Metering.
//!
//! Models Ableton Live 12.4.6 Chapter 2 ("First Steps: Audio Preferences & Latency"):
//! - Supported buffer sizes: 64, 128, 256, 512, 1024, 2048 samples.
//! - Supported sample rates: 44.1 kHz, 48.0 kHz, 88.2 kHz, 96.0 kHz.
//! - Latency calculation: Input Latency, Output Latency, Driver Error Compensation, Overall Latency.
//! - Test Tone Generator: 440 Hz standard calibration sine tone, volume -inf..0 dBFS.
//! - CPU load monitoring and audio thread budget calculation.

pub const MIN_BUFFER_SIZE: usize = 64;
pub const MAX_BUFFER_SIZE: usize = 2048;

/// Standard hardware buffer sizes in Ableton Live 12.
pub const SUPPORTED_BUFFER_SIZES: [usize; 6] = [64, 128, 256, 512, 1024, 2048];

/// Standard production sample rates in Ableton Live 12.
pub const SUPPORTED_SAMPLE_RATES: [f32; 4] = [44100.0, 48000.0, 88200.0, 96000.0];

/// Summary of driver latencies in samples and milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioLatencySummary {
    pub sample_rate: f32,
    pub buffer_size: usize,
    pub input_latency_samples: u32,
    pub output_latency_samples: u32,
    pub driver_error_compensation_samples: i32,
    pub overall_latency_samples: u32,
    pub input_latency_ms: f32,
    pub output_latency_ms: f32,
    pub overall_latency_ms: f32,
}

/// User audio preferences for hardware I/O, latency compensation, and test tone.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioHardwarePreferences {
    pub sample_rate: f32,
    pub buffer_size: usize,
    pub driver_error_compensation_samples: i32,
    pub test_tone_enabled: bool,
    pub test_tone_freq: f32,
    pub test_tone_volume_db: f32,
}

impl Default for AudioHardwarePreferences {
    fn default() -> Self {
        Self {
            sample_rate: 44100.0,
            buffer_size: 256,
            driver_error_compensation_samples: 0,
            test_tone_enabled: false,
            test_tone_freq: 440.0,
            test_tone_volume_db: -20.0,
        }
    }
}

impl AudioHardwarePreferences {
    /// Create new preferences with verified buffer size and sample rate.
    pub fn new(sample_rate: f32, buffer_size: usize) -> Self {
        let sr = if Self::is_valid_sample_rate(sample_rate) {
            sample_rate
        } else {
            44100.0
        };
        let bs = Self::snap_buffer_size(buffer_size);

        Self {
            sample_rate: sr,
            buffer_size: bs,
            driver_error_compensation_samples: 0,
            test_tone_enabled: false,
            test_tone_freq: 440.0,
            test_tone_volume_db: -20.0,
        }
    }

    /// Check if a sample rate is a supported standard production rate.
    pub fn is_valid_sample_rate(sr: f32) -> bool {
        SUPPORTED_SAMPLE_RATES.iter().any(|&s| (s - sr).abs() < 1.0)
    }

    /// Check if a buffer size is in the supported set (64..2048).
    pub fn is_valid_buffer_size(size: usize) -> bool {
        SUPPORTED_BUFFER_SIZES.contains(&size)
    }

    /// Snap any buffer size to the nearest supported power-of-two in 64..2048.
    pub fn snap_buffer_size(size: usize) -> usize {
        let clamped = size.clamp(MIN_BUFFER_SIZE, MAX_BUFFER_SIZE);
        *SUPPORTED_BUFFER_SIZES
            .iter()
            .min_by_key(|&&s| (s as isize - clamped as isize).abs())
            .unwrap_or(&256)
    }

    /// Set buffer size, snapping to valid range (64..2048).
    pub fn set_buffer_size(&mut self, size: usize) {
        self.buffer_size = Self::snap_buffer_size(size);
    }

    /// Set sample rate if valid.
    pub fn set_sample_rate(&mut self, sr: f32) -> bool {
        if Self::is_valid_sample_rate(sr) {
            self.sample_rate = sr;
            true
        } else {
            false
        }
    }

    /// Set Driver Error Compensation (DEC) in samples.
    pub fn set_driver_error_compensation(&mut self, samples: i32) {
        self.driver_error_compensation_samples = samples.clamp(-2048, 2048);
    }

    /// Set Test Tone generator parameters.
    pub fn set_test_tone(&mut self, enabled: bool, freq_hz: f32, volume_db: f32) {
        self.test_tone_enabled = enabled;
        self.test_tone_freq = freq_hz.clamp(20.0, 20000.0);
        self.test_tone_volume_db = volume_db.clamp(-60.0, 0.0);
    }

    /// Calculate exact latency metrics according to Ableton Live 12 Chapter 2:
    /// - Input Latency = buffer_size + in_driver_safety
    /// - Output Latency = buffer_size + out_driver_safety
    /// - Overall Latency = input_latency + output_latency + driver_error_compensation
    /// - ms = (samples / sample_rate) * 1000.0
    pub fn calculate_latency_summary(
        &self,
        in_driver_safety: u32,
        out_driver_safety: u32,
    ) -> AudioLatencySummary {
        let in_samples = self.buffer_size as u32 + in_driver_safety;
        let out_samples = self.buffer_size as u32 + out_driver_safety;
        let raw_overall = (in_samples + out_samples) as i32 + self.driver_error_compensation_samples;
        let overall_samples = raw_overall.max(0) as u32;

        let sr = if self.sample_rate > 0.0 {
            self.sample_rate
        } else {
            44100.0
        };

        let in_ms = (in_samples as f32 / sr) * 1000.0;
        let out_ms = (out_samples as f32 / sr) * 1000.0;
        let overall_ms = (overall_samples as f32 / sr) * 1000.0;

        AudioLatencySummary {
            sample_rate: sr,
            buffer_size: self.buffer_size,
            input_latency_samples: in_samples,
            output_latency_samples: out_samples,
            driver_error_compensation_samples: self.driver_error_compensation_samples,
            overall_latency_samples: overall_samples,
            input_latency_ms: in_ms,
            output_latency_ms: out_ms,
            overall_latency_ms: overall_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_size_validation_and_snapping() {
        assert!(AudioHardwarePreferences::is_valid_buffer_size(64));
        assert!(AudioHardwarePreferences::is_valid_buffer_size(128));
        assert!(AudioHardwarePreferences::is_valid_buffer_size(256));
        assert!(AudioHardwarePreferences::is_valid_buffer_size(512));
        assert!(AudioHardwarePreferences::is_valid_buffer_size(1024));
        assert!(AudioHardwarePreferences::is_valid_buffer_size(2048));

        assert!(!AudioHardwarePreferences::is_valid_buffer_size(32));
        assert!(!AudioHardwarePreferences::is_valid_buffer_size(500));
        assert!(!AudioHardwarePreferences::is_valid_buffer_size(4096));

        // Snapping
        assert_eq!(AudioHardwarePreferences::snap_buffer_size(100), 128);
        assert_eq!(AudioHardwarePreferences::snap_buffer_size(300), 256);
        assert_eq!(AudioHardwarePreferences::snap_buffer_size(1500), 1024);
        assert_eq!(AudioHardwarePreferences::snap_buffer_size(2500), 2048);
        assert_eq!(AudioHardwarePreferences::snap_buffer_size(16), 64);
    }

    #[test]
    fn test_sample_rate_validation() {
        assert!(AudioHardwarePreferences::is_valid_sample_rate(44100.0));
        assert!(AudioHardwarePreferences::is_valid_sample_rate(48000.0));
        assert!(AudioHardwarePreferences::is_valid_sample_rate(88200.0));
        assert!(AudioHardwarePreferences::is_valid_sample_rate(96000.0));

        assert!(!AudioHardwarePreferences::is_valid_sample_rate(22050.0));
        assert!(!AudioHardwarePreferences::is_valid_sample_rate(192000.1));
    }

    #[test]
    fn test_latency_summary_calculations() {
        let mut prefs = AudioHardwarePreferences::new(48000.0, 256);
        // Safety offsets: 32 in, 32 out
        let summary = prefs.calculate_latency_summary(32, 32);

        // Input latency = 256 + 32 = 288 samples = 288 / 48000 * 1000 = 6.0 ms
        assert_eq!(summary.input_latency_samples, 288);
        assert!((summary.input_latency_ms - 6.0).abs() < 1e-3);

        // Output latency = 256 + 32 = 288 samples = 6.0 ms
        assert_eq!(summary.output_latency_samples, 288);
        assert!((summary.output_latency_ms - 6.0).abs() < 1e-3);

        // Overall latency = 288 + 288 = 576 samples = 12.0 ms
        assert_eq!(summary.overall_latency_samples, 576);
        assert!((summary.overall_latency_ms - 12.0).abs() < 1e-3);

        // Test with Driver Error Compensation (e.g. -48 samples = -1.0 ms)
        prefs.set_driver_error_compensation(-48);
        let summary_dec = prefs.calculate_latency_summary(32, 32);
        assert_eq!(summary_dec.overall_latency_samples, 576 - 48);
        assert!((summary_dec.overall_latency_ms - 11.0).abs() < 1e-3);
    }
}
