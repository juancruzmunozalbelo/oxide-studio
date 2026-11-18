//! Lightweight WAV file decoder for loading drum samples without external runtime dependencies.

use std::convert::TryInto;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DecodedWav {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub samples: Vec<f32>, // Interleaved normalized f32 audio samples
}

pub fn load_wav_file<P: AsRef<Path>>(path: P) -> Result<DecodedWav, String> {
    let mut file = File::open(path.as_ref())
        .map_err(|e| format!("Failed to open WAV at {:?}: {}", path.as_ref(), e))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read WAV bytes: {}", e))?;

    if bytes.len() < 44 {
        return Err("WAV file too short for standard RIFF header".into());
    }

    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Invalid RIFF/WAVE header magic bytes".into());
    }

    let mut pos = 12;
    let mut sample_rate = 44100u32;
    let mut channels = 2u16;
    let mut bits_per_sample = 16u16;
    let mut data_offset = 0;
    let mut data_len = 0;
    let mut is_float_pcm = false;

    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(
            bytes[pos + 4..pos + 8]
                .try_into()
                .map_err(|_| "Failed reading chunk size")?,
        ) as usize;
        pos += 8;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || pos + chunk_size > bytes.len() {
                return Err("Corrupt fmt chunk in WAV".into());
            }
            let audio_format = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            channels = u16::from_le_bytes(bytes[pos + 2..pos + 4].try_into().unwrap());
            if channels == 0 {
                return Err("Invalid channel count (0) in WAV".into());
            }
            sample_rate = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(bytes[pos + 14..pos + 16].try_into().unwrap());

            if audio_format == 3 {
                is_float_pcm = true;
            } else if audio_format == 0xFFFE {
                if chunk_size < 40 {
                    return Err("Truncated WAVE_FORMAT_EXTENSIBLE fmt chunk".into());
                }
                let sub_format = u16::from_le_bytes(bytes[pos + 24..pos + 26].try_into().unwrap());
                const MS_SUBFORMAT_TAIL: [u8; 14] = [
                    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
                ];
                if bytes[pos + 26..pos + 40] != MS_SUBFORMAT_TAIL {
                    return Err("Unsupported WAVE_FORMAT_EXTENSIBLE subformat GUID".into());
                }
                match sub_format {
                    1 => is_float_pcm = false,
                    3 => is_float_pcm = true,
                    other => {
                        return Err(format!(
                            "Unsupported WAVE_FORMAT_EXTENSIBLE subformat code: {}",
                            other
                        ));
                    }
                }
            } else if audio_format != 1 {
                return Err(format!("Unsupported WAV audio_format: {}", audio_format));
            }
            pos = pos.saturating_add(chunk_size + (chunk_size & 1));
        } else if chunk_id == b"data" {
            data_offset = pos;
            data_len = chunk_size.min(bytes.len().saturating_sub(pos));
            break;
        } else {
            pos = pos.saturating_add(chunk_size + (chunk_size & 1));
        }
    }

    if data_offset == 0 || data_len == 0 {
        return Err("No data chunk found in WAV file".into());
    }

    let bytes_per_sample = (bits_per_sample / 8) as usize;
    if bytes_per_sample == 0 {
        return Err(format!("Invalid bits_per_sample: {}", bits_per_sample));
    }

    let data_slice = &bytes[data_offset..data_offset + data_len];
    let mut samples = Vec::with_capacity(data_len / bytes_per_sample);

    if bytes_per_sample == 2 {
        // 16-bit signed PCM
        for chunk in data_slice.chunks_exact(2) {
            let s_i16 = i16::from_le_bytes(chunk.try_into().unwrap());
            samples.push(s_i16 as f32 / 32768.0);
        }
    } else if bytes_per_sample == 3 {
        // 24-bit signed PCM
        for chunk in data_slice.chunks_exact(3) {
            let s_i32 = (chunk[0] as i32) | ((chunk[1] as i32) << 8) | ((chunk[2] as i8 as i32) << 16);
            samples.push(s_i32 as f32 / 8388608.0);
        }
    } else if bytes_per_sample == 4 {
        if is_float_pcm {
            // 32-bit float IEEE-754
            for chunk in data_slice.chunks_exact(4) {
                let s_f32 = f32::from_le_bytes(chunk.try_into().unwrap());
                samples.push(s_f32);
            }
        } else {
            // 32-bit signed integer PCM
            for chunk in data_slice.chunks_exact(4) {
                let s_i32 = i32::from_le_bytes(chunk.try_into().unwrap());
                samples.push(s_i32 as f32 / 2147483648.0);
            }
        }
    } else {
        return Err(format!("Unsupported bits_per_sample: {}", bits_per_sample));
    }

    Ok(DecodedWav {
        sample_rate,
        channels,
        bits_per_sample,
        samples,
    })
}
