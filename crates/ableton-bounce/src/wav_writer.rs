//! Lightweight 16-bit and 32-bit stereo WAV file writer with valid 44-byte RIFF headers.

use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct WavWriter;

impl WavWriter {
    /// Write standard 16-bit stereo PCM WAV file with valid 44-byte RIFF header.
    pub fn write_wav_16bit<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        channels: u16,
        interleaved_samples: &[f32],
    ) -> Result<(), String> {
        let mut file = File::create(path.as_ref())
            .map_err(|e| format!("Failed to create WAV at {:?}: {}", path.as_ref(), e))?;

        let num_samples = interleaved_samples.len();
        let bytes_per_sample = 2u16;
        let data_size = (num_samples * bytes_per_sample as usize) as u32;
        let file_size = 36 + data_size;

        // 1. RIFF header
        file.write_all(b"RIFF").map_err(|e| e.to_string())?;
        file.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(b"WAVE").map_err(|e| e.to_string())?;

        // 2. fmt sub-chunk
        file.write_all(b"fmt ").map_err(|e| e.to_string())?;
        let fmt_size = 16u32;
        let audio_format = 1u16; // PCM integer
        let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
        let block_align = channels * bytes_per_sample;
        let bits_per_sample = 16u16;

        file.write_all(&fmt_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&audio_format.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&channels.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&sample_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&byte_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&block_align.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&bits_per_sample.to_le_bytes()).map_err(|e| e.to_string())?;

        // 3. data sub-chunk
        file.write_all(b"data").map_err(|e| e.to_string())?;
        file.write_all(&data_size.to_le_bytes()).map_err(|e| e.to_string())?;

        // 4. Sample data (16-bit PCM clamped with rounding)
        let mut pcm_bytes = Vec::with_capacity(data_size as usize);
        for &s in interleaved_samples {
            let clamped = s.clamp(-1.0, 1.0);
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
        let mut file = File::create(path.as_ref())
            .map_err(|e| format!("Failed to create WAV at {:?}: {}", path.as_ref(), e))?;

        let num_samples = interleaved_samples.len();
        let bytes_per_sample = 2u16;
        let data_size = (num_samples * bytes_per_sample as usize) as u32;
        let file_size = 36 + data_size;

        // 1. RIFF header
        file.write_all(b"RIFF").map_err(|e| e.to_string())?;
        file.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(b"WAVE").map_err(|e| e.to_string())?;

        // 2. fmt sub-chunk
        file.write_all(b"fmt ").map_err(|e| e.to_string())?;
        let fmt_size = 16u32;
        let audio_format = 1u16; // PCM integer
        let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
        let block_align = channels * bytes_per_sample;
        let bits_per_sample = 16u16;

        file.write_all(&fmt_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&audio_format.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&channels.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&sample_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&byte_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&block_align.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&bits_per_sample.to_le_bytes()).map_err(|e| e.to_string())?;

        // 3. data sub-chunk
        file.write_all(b"data").map_err(|e| e.to_string())?;
        file.write_all(&data_size.to_le_bytes()).map_err(|e| e.to_string())?;

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

            let scaled = s * 32767.0 + tpdf_dither;
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
        let mut file = File::create(path.as_ref())
            .map_err(|e| format!("Failed to create WAV at {:?}: {}", path.as_ref(), e))?;

        let num_samples = interleaved_samples.len();
        let bytes_per_sample = 4u16;
        let data_size = (num_samples * bytes_per_sample as usize) as u32;
        let file_size = 36 + data_size;

        // 1. RIFF header
        file.write_all(b"RIFF").map_err(|e| e.to_string())?;
        file.write_all(&file_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(b"WAVE").map_err(|e| e.to_string())?;

        // 2. fmt sub-chunk
        file.write_all(b"fmt ").map_err(|e| e.to_string())?;
        let fmt_size = 16u32;
        let audio_format = 3u16; // IEEE float
        let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
        let block_align = channels * bytes_per_sample;
        let bits_per_sample = 32u16;

        file.write_all(&fmt_size.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&audio_format.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&channels.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&sample_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&byte_rate.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&block_align.to_le_bytes()).map_err(|e| e.to_string())?;
        file.write_all(&bits_per_sample.to_le_bytes()).map_err(|e| e.to_string())?;

        // 3. data sub-chunk
        file.write_all(b"data").map_err(|e| e.to_string())?;
        file.write_all(&data_size.to_le_bytes()).map_err(|e| e.to_string())?;

        // 4. Sample data (32-bit IEEE float)
        let mut float_bytes = Vec::with_capacity(data_size as usize);
        for &s in interleaved_samples {
            float_bytes.extend_from_slice(&s.to_le_bytes());
        }
        file.write_all(&float_bytes).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;

        Ok(())
    }
}
