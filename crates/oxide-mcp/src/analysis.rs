//! Perceptual and spectral audio analysis ("Agent Ears").
//!
//! Computes peak, RMS, crest factor, spectral centroid, zero-crossing rate,
//! and 4-band spectral energy balance across Sub/Bass, Low-Mid, High-Mid, and High frequencies.

use std::f32::consts::PI;
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FourBandEnergy {
    pub sub_bass_energy_pct: f32, // < 250 Hz
    pub low_mid_energy_pct: f32,  // 250 Hz - 1000 Hz
    pub high_mid_energy_pct: f32, // 1000 Hz - 4000 Hz
    pub high_energy_pct: f32,     // > 4000 Hz
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioAnalysisMetrics {
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: usize,
    pub total_samples: usize,
    pub peak_amplitude: f32,
    pub peak_dbfs: f32,
    pub rms_amplitude: f32,
    pub rms_dbfs: f32,
    pub crest_factor_db: f32,
    pub spectral_centroid_hz: f32,
    pub spectral_bands: FourBandEnergy,
    pub zero_crossing_rate: f32,
    pub has_clipping: bool,
    pub is_silent: bool,
}

impl AudioAnalysisMetrics {
    /// Analyze interleaved f32 audio samples.
    pub fn analyze_interleaved(samples: &[f32], sample_rate: u32, channels: usize) -> Self {
        if samples.is_empty() || channels == 0 || sample_rate == 0 {
            return Self::empty();
        }

        let total_frames = samples.len() / channels;
        let duration_seconds = total_frames as f64 / sample_rate as f64;

        // 1. Peak, RMS, Zero-crossings
        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f64;
        let mut zero_crossings = 0usize;
        let mut prev_sign = false;
        let mut has_clipping = false;

        for (i, &s) in samples.iter().enumerate() {
            let abs_s = s.abs();
            if abs_s > peak {
                peak = abs_s;
            }
            if abs_s >= 0.9999 {
                has_clipping = true;
            }
            sum_sq += (s * s) as f64;

            let cur_sign = s >= 0.0;
            if i > 0 && cur_sign != prev_sign {
                zero_crossings += 1;
            }
            prev_sign = cur_sign;
        }

        let mean_sq = sum_sq / samples.len() as f64;
        let rms = mean_sq.sqrt() as f32;

        let peak_dbfs = 20.0 * peak.max(1e-5).log10();
        let rms_dbfs = 20.0 * rms.max(1e-5).log10();
        let crest_factor_db = peak_dbfs - rms_dbfs;

        let zcr = zero_crossings as f32 / samples.len() as f32;
        let is_silent = rms < 1e-4;

        // 2. Mix to mono for Spectral Analysis
        let mut mono = Vec::with_capacity(total_frames);
        for f in 0..total_frames {
            let mut sum = 0.0f32;
            for c in 0..channels {
                sum += samples[f * channels + c];
            }
            mono.push(sum / channels as f32);
        }

        let (centroid, bands) = compute_spectral_metrics(&mono, sample_rate);

        Self {
            duration_seconds,
            sample_rate,
            channels,
            total_samples: samples.len(),
            peak_amplitude: peak,
            peak_dbfs,
            rms_amplitude: rms,
            rms_dbfs,
            crest_factor_db,
            spectral_centroid_hz: centroid,
            spectral_bands: bands,
            zero_crossing_rate: zcr,
            has_clipping,
            is_silent,
        }
    }

    /// Load and analyze WAV file from disk using full RIFF chunk iteration.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn analyze_wav_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(format!("File does not exist: {}", p.display()));
        }

        let bytes = std::fs::read(p).map_err(|e| format!("Failed to open/read WAV: {}", e))?;
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err("Invalid RIFF/WAVE header".to_string());
        }

        let mut format_tag: Option<u16> = None;
        let mut channels: Option<usize> = None;
        let mut sample_rate: Option<u32> = None;
        let mut bits_per_sample: Option<u16> = None;
        let mut data_slice: Option<&[u8]> = None;

        let mut offset: usize = 12;
        while offset + 8 <= bytes.len() {
            let chunk_id = &bytes[offset..offset + 4];
            let chunk_size = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;
            let payload_start = offset + 8;
            if payload_start > bytes.len() {
                break;
            }
            let payload_end = payload_start.saturating_add(chunk_size).min(bytes.len());
            let payload = &bytes[payload_start..payload_end];

            if chunk_id == b"fmt " {
                if payload.len() < 16 {
                    return Err("Malformed WAV 'fmt ' chunk (< 16 bytes)".to_string());
                }
                let mut tag = u16::from_le_bytes([payload[0], payload[1]]);
                if tag == 0xFFFE && payload.len() >= 26 {
                    // WAVE_FORMAT_EXTENSIBLE: actual format code is in SubFormat GUID bytes 24..26
                    tag = u16::from_le_bytes([payload[24], payload[25]]);
                }
                format_tag = Some(tag);
                channels = Some(u16::from_le_bytes([payload[2], payload[3]]) as usize);
                sample_rate = Some(u32::from_le_bytes([
                    payload[4],
                    payload[5],
                    payload[6],
                    payload[7],
                ]));
                bits_per_sample = Some(u16::from_le_bytes([payload[14], payload[15]]));
            } else if chunk_id == b"data" {
                data_slice = Some(payload);
            }

            // RIFF chunks are 2-byte word aligned
            let padded_size = chunk_size.saturating_add(chunk_size & 1);
            let next_offset = payload_start.saturating_add(padded_size);
            if next_offset <= offset {
                break;
            }
            offset = next_offset;
        }

        let format_tag = format_tag.ok_or_else(|| "Missing WAV 'fmt ' chunk".to_string())?;
        let channels = channels.ok_or_else(|| "Missing WAV channel count".to_string())?;
        let sample_rate = sample_rate.ok_or_else(|| "Missing WAV sample rate".to_string())?;
        let bits_per_sample =
            bits_per_sample.ok_or_else(|| "Missing WAV bits per sample".to_string())?;
        let data = data_slice.ok_or_else(|| "Missing WAV 'data' chunk".to_string())?;

        if channels == 0 || sample_rate == 0 {
            return Err(format!(
                "Invalid WAV stream metadata: channels={}, sample_rate={}",
                channels, sample_rate
            ));
        }

        let mut samples = Vec::new();
        match (format_tag, bits_per_sample) {
            (1, 16) => {
                samples.reserve(data.len() / 2);
                for chunk in data.chunks_exact(2) {
                    let s = i16::from_le_bytes([chunk[0], chunk[1]]);
                    samples.push(s as f32 / 32768.0);
                }
            }
            (1, 24) => {
                samples.reserve(data.len() / 3);
                for chunk in data.chunks_exact(3) {
                    let raw = (chunk[0] as i32)
                        | ((chunk[1] as i32) << 8)
                        | ((chunk[2] as i32) << 16);
                    let signed = if (raw & 0x80_0000) != 0 {
                        raw | !0xFF_FFFF
                    } else {
                        raw
                    };
                    samples.push(signed as f32 / 8_388_608.0);
                }
            }
            (1, 32) => {
                samples.reserve(data.len() / 4);
                for chunk in data.chunks_exact(4) {
                    let s = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    samples.push(s as f32 / 2_147_483_648.0);
                }
            }
            (3, 32) => {
                samples.reserve(data.len() / 4);
                for chunk in data.chunks_exact(4) {
                    samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            (3, 64) => {
                samples.reserve(data.len() / 8);
                for chunk in data.chunks_exact(8) {
                    let v = f64::from_le_bytes([
                        chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
                        chunk[7],
                    ]);
                    samples.push(v as f32);
                }
            }
            _ => {
                return Err(format!(
                    "Unsupported WAV format: tag {}, {} bits",
                    format_tag, bits_per_sample
                ));
            }
        }

        Ok(Self::analyze_interleaved(&samples, sample_rate, channels))
    }

    fn empty() -> Self {
        Self {
            duration_seconds: 0.0,
            sample_rate: 44100,
            channels: 2,
            total_samples: 0,
            peak_amplitude: 0.0,
            peak_dbfs: -100.0,
            rms_amplitude: 0.0,
            rms_dbfs: -100.0,
            crest_factor_db: 0.0,
            spectral_centroid_hz: 0.0,
            spectral_bands: FourBandEnergy {
                sub_bass_energy_pct: 0.0,
                low_mid_energy_pct: 0.0,
                high_mid_energy_pct: 0.0,
                high_energy_pct: 0.0,
            },
            zero_crossing_rate: 0.0,
            has_clipping: false,
            is_silent: true,
        }
    }
}

/// Compute spectral centroid and 4-band energy using STFT over mono signal.
fn compute_spectral_metrics(mono: &[f32], sample_rate: u32) -> (f32, FourBandEnergy) {
    if mono.is_empty() || sample_rate == 0 {
        return (
            0.0,
            FourBandEnergy {
                sub_bass_energy_pct: 25.0,
                low_mid_energy_pct: 25.0,
                high_mid_energy_pct: 25.0,
                high_energy_pct: 25.0,
            },
        );
    }

    const FFT_SIZE: usize = 1024;
    const HOP_SIZE: usize = 512;

    let sr = sample_rate as f32;
    let bin_freq_step = sr / FFT_SIZE as f32;

    let mut total_weighted_centroid = 0.0f64;
    let mut total_magnitude_sum = 0.0f64;

    let mut energy_sub_bass = 0.0f64; // < 250 Hz
    let mut energy_low_mid = 0.0f64;  // 250 - 1000 Hz
    let mut energy_high_mid = 0.0f64; // 1000 - 4000 Hz
    let mut energy_high = 0.0f64;     // > 4000 Hz

    // Precalculate Hann window
    let mut window = [0.0f32; FFT_SIZE];
    for (i, w) in window.iter_mut().enumerate() {
        *w = 0.5 * (1.0 - (2.0 * PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos());
    }

    let mut windowed = [0.0f32; FFT_SIZE];
    let num_bins = FFT_SIZE / 2;

    let mut offset = 0;
    while offset + FFT_SIZE <= mono.len() {
        for i in 0..FFT_SIZE {
            windowed[i] = mono[offset + i] * window[i];
        }

        // Discrete Fourier Transform for first num_bins
        // Optimized with step sampling across bins for fast computation
        for k in 1..num_bins {
            let freq = k as f32 * bin_freq_step;
            let omega = 2.0 * PI * (k as f32) / (FFT_SIZE as f32);

            let mut real = 0.0f32;
            let mut imag = 0.0f32;

            for (n, &s) in windowed.iter().enumerate() {
                let angle = omega * n as f32;
                real += s * angle.cos();
                imag -= s * angle.sin();
            }

            let mag = (real * real + imag * imag).sqrt();
            let power = (mag * mag) as f64;

            total_weighted_centroid += (freq as f64) * (mag as f64);
            total_magnitude_sum += mag as f64;

            if freq < 250.0 {
                energy_sub_bass += power;
            } else if freq < 1000.0 {
                energy_low_mid += power;
            } else if freq < 4000.0 {
                energy_high_mid += power;
            } else {
                energy_high += power;
            }
        }

        offset += HOP_SIZE; // 50% Hann window overlap
    }

    let centroid_hz = if total_magnitude_sum > 1e-9 {
        (total_weighted_centroid / total_magnitude_sum) as f32
    } else {
        1000.0
    };

    let total_band_energy = energy_sub_bass + energy_low_mid + energy_high_mid + energy_high;
    let bands = if total_band_energy > 1e-9 {
        FourBandEnergy {
            sub_bass_energy_pct: ((energy_sub_bass / total_band_energy) * 100.0) as f32,
            low_mid_energy_pct: ((energy_low_mid / total_band_energy) * 100.0) as f32,
            high_mid_energy_pct: ((energy_high_mid / total_band_energy) * 100.0) as f32,
            high_energy_pct: ((energy_high / total_band_energy) * 100.0) as f32,
        }
    } else {
        FourBandEnergy {
            sub_bass_energy_pct: 25.0,
            low_mid_energy_pct: 25.0,
            high_mid_energy_pct: 25.0,
            high_energy_pct: 25.0,
        }
    };

    (centroid_hz, bands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_bass_sine_wave_spectral_metrics() {
        let sample_rate = 44100;
        let duration_secs = 1.0;
        let total_samples = (sample_rate as f32 * duration_secs) as usize;
        let freq = 100.0f32;

        let mut samples = Vec::with_capacity(total_samples * 2);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let val = (2.0 * PI * freq * t).sin() * 0.8;
            samples.push(val);
            samples.push(val); // stereo
        }

        let metrics = AudioAnalysisMetrics::analyze_interleaved(&samples, sample_rate, 2);

        assert!(
            metrics.spectral_centroid_hz >= 80.0 && metrics.spectral_centroid_hz <= 140.0,
            "100 Hz sine centroid should be near 100 Hz, got {}",
            metrics.spectral_centroid_hz
        );
        assert!(
            metrics.spectral_bands.sub_bass_energy_pct > 90.0,
            "Sub-bass energy should be > 90%, got {}%",
            metrics.spectral_bands.sub_bass_energy_pct
        );
        assert!(
            metrics.spectral_bands.high_energy_pct < 5.0,
            "High energy should be < 5%, got {}%",
            metrics.spectral_bands.high_energy_pct
        );
        assert!(!metrics.is_silent);
        assert!(!metrics.has_clipping);
    }
}

