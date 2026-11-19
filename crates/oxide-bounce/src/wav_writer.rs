//! Lightweight 16-bit and 32-bit stereo WAV file writer with valid 44-byte RIFF headers.

use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct WavWriter;

impl WavWriter {
    fn compute_riff_sizes(num_samples: usize, bytes_per_sample: u16) -> Result<(u32, u32), String> {
        let data_bytes = num_samples
            .checked_mul(bytes_per_sample as usize)
            .ok_or_else(|| "WAV data size overflows usize".to_string())?;
        let data_size = u32::try_from(data_bytes)
            .map_err(|_| "WAV data size exceeds 4 GiB RIFF limit".to_string())?;
        let file_size = 36u32
            .checked_add(data_size)
            .ok_or_else(|| "WAV RIFF chunk size exceeds 4 GiB limit".to_string())?;
        Ok((data_size, file_size))
    }

    fn create_wav_with_header<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        channels: u16,
        num_samples: usize,
        bytes_per_sample: u16,
        audio_format: u16,
    ) -> Result<(File, u32), String> {
        if channels == 0 {
            return Err("WAV channels must be > 0".to_string());
        }
        if sample_rate == 0 {
            return Err("WAV sample_rate must be > 0".to_string());
        }

        let (data_size, file_size) = Self::compute_riff_sizes(num_samples, bytes_per_sample)?;
        let byte_rate = sample_rate
            .checked_mul(channels as u32)
            .and_then(|v| v.checked_mul(bytes_per_sample as u32))
            .ok_or_else(|| "WAV byte_rate overflows u32".to_string())?;
        let block_align = channels
            .checked_mul(bytes_per_sample)
            .ok_or_else(|| "WAV block_align overflows u16".to_string())?;
        let bits_per_sample = bytes_per_sample * 8;

        let mut file = File::create(path.as_ref())
            .map_err(|e| format!("Failed to create WAV at {:?}: {}", path.as_ref(), e))?;

        // 1. RIFF header
        file.write_all(b"RIFF").map_err(|e| e.to_string())?;
        file.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(b"WAVE").map_err(|e| e.to_string())?;

        // 2. fmt sub-chunk
        file.write_all(b"fmt ").map_err(|e| e.to_string())?;
        let fmt_size = 16u32;
        file.write_all(&fmt_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&audio_format.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&channels.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&sample_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&byte_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&block_align.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&bits_per_sample.to_le_bytes()).map_err(|e| e.to_string())?;

        // 3. data sub-chunk header
        file.write_all(b"data").map_err(|e| e.to_string())?;
        file.write_all(&data_size.to_le_bytes()).map_err(|e| e.to_string())?;

        Ok((file, data_size))
    }

    /// Write standard 16-bit stereo PCM WAV file with valid 44-byte RIFF header.
    pub fn write_wav_16bit<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        channels: u16,
        interleaved_samples: &[f32],
    ) -> Result<(), String> {
        let (mut file, data_size) = Self::create_wav_with_header(
            path,
            sample_rate,
            channels,
            interleaved_samples.len(),
            2,
            1, // PCM integer
        )?;

        // 4. Sample data (16-bit PCM clamped with rounding)
        let mut pcm_bytes = Vec::with_capacity(data_size as usize);
        for &s in interleaved_samples {
            let safe = if s.is_finite() { s } else { 0.0 };
            let clamped = safe.clamp(-1.0, 1.0);
            let val = (clamped * 32767.0).round() as i16;
            pcm_bytes.extend_from_slice(&val.to_le_bytes());
        }
        file.write_all(&pcm_bytes).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Write 16-bit stereo PCM WAV file with Triangular Probability Density Function (TPDF) dither.
    pub fn write_wav_16bit_tpdf<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        channels: u16,
        interleaved_samples: &[f32],
    ) -> Result<(), String> {
        let (mut file, data_size) = Self::create_wav_with_header(
            path,
            sample_rate,
            channels,
            interleaved_samples.len(),
            2,
            1, // PCM integer
        )?;

        // 4. Sample data with TPDF dither
        let mut rng: u32 = 0x5a1b3c7d;
        let mut pcm_bytes = Vec::with_capacity(data_size as usize);
        for &s in interleaved_samples {
            // XorShift 32 for two independent uniform random variables
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let r1 = (rng as f32) / 4294967296.0;

            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let r2 = (rng as f32) / 4294967296.0;

            // TPDF noise: triangular distribution [-1.0, 1.0] LSB
            let tpdf_dither = r1 - r2;

            let safe = if s.is_finite() { s } else { 0.0 };
            let scaled = safe * 32767.0 + tpdf_dither;
            let clamped = scaled.clamp(-32768.0, 32767.0);
            let val = clamped.round() as i16;
            pcm_bytes.extend_from_slice(&val.to_le_bytes());
        }
        file.write_all(&pcm_bytes).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Write 32-bit float stereo WAV file (IEEE float) with valid 44-byte RIFF header.
    pub fn write_wav_32bit_float<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        channels: u16,
        interleaved_samples: &[f32],
    ) -> Result<(), String> {
        let (mut file, data_size) = Self::create_wav_with_header(
            path,
            sample_rate,
            channels,
            interleaved_samples.len(),
            4,
            3, // IEEE float
        )?;

        // 4. Sample data (32-bit IEEE float)
        let mut float_bytes = Vec::with_capacity(data_size as usize);
        for &s in interleaved_samples {
            let safe = if s.is_finite() { s } else { 0.0 };
            float_bytes.extend_from_slice(&safe.to_le_bytes());
        }
        file.write_all(&float_bytes).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejects_zero_channels_or_sample_rate_without_creating_file() {
        let dir = std::env::temp_dir().join("oxide_wav_writer_invalid_header");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let p1 = dir.join("zero_ch.wav");
        let p2 = dir.join("zero_sr.wav");
        assert!(WavWriter::write_wav_16bit(&p1, 44100, 0, &[0.0, 0.0]).is_err());
        assert!(WavWriter::write_wav_32bit_float(&p2, 0, 2, &[0.0, 0.0]).is_err());
        assert!(!p1.exists());
        assert!(!p2.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_riff_overflow_rejected() {
        // Exactly at 4 GiB RIFF boundary: 36 + data_size overflows u32
        assert!(WavWriter::compute_riff_sizes(1_073_741_824, 4).is_err());
        assert!(WavWriter::compute_riff_sizes(1_073_741_820, 4).is_err());
        assert!(WavWriter::compute_riff_sizes(1_073_741_814, 4).is_ok());
    }

    #[test]
    fn test_write_wav_32bit_float_sanitizes_non_finite_samples() {
        let dir = std::env::temp_dir().join("oxide_wav_writer_sanitize");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sanitized.wav");

        WavWriter::write_wav_32bit_float(
            &path,
            44100,
            2,
            &[0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
        )
        .unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 44 + 16);
        for chunk in bytes[44..].chunks_exact(4) {
            let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            assert!(val.is_finite(), "Non-finite float leaked into WAV: {}", val);
        }
        assert_eq!(f32::from_le_bytes(bytes[44..48].try_into().unwrap()), 0.5);
        assert_eq!(f32::from_le_bytes(bytes[48..52].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(bytes[52..56].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(bytes[56..60].try_into().unwrap()), 0.0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
