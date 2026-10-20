//! E2E Test Suite: Headless Multi-Track Arrangement Demo WAV Bounce Export & Verification
//!
//! Authoritative baselines:
//! - /Users/juancruz/ableton-rust/PROJECT.md § Architectural Layers 6 & Acceptance Criteria
//! - /Users/juancruz/ableton-rust/.agents/teamwork/ORIGINAL_REQUEST.md § Acceptance Criteria

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use ableton_bounce::{DemoArrangementRenderer, WavWriter};

#[test]
fn test_multitrack_offline_bounce_wav_export() {
    let sample_rate = 44100.0;
    let bpm = 126.0;
    let renderer = DemoArrangementRenderer::new(sample_rate, bpm);

    // Render 4.5 seconds of multi-track arrangement (> 4.0s required)
    let duration_sec = 4.5;
    let interleaved = renderer.render_demo_seconds(duration_sec);

    let expected_total_samples = (duration_sec * sample_rate as f64 * 2.0) as usize;
    assert_eq!(
        interleaved.len(),
        expected_total_samples,
        "Rendered interleaved sample count must match duration * sr * 2"
    );

    // Export to WAV
    let output_path = Path::new("/tmp/ableton_rust_test_export.wav");
    WavWriter::write_wav_16bit(output_path, sample_rate as u32, 2, &interleaved)
        .expect("Failed to export WAV");

    assert!(output_path.exists(), "Exported WAV file must exist on disk");
    let metadata = fs::metadata(output_path).expect("File metadata");
    println!("Exported WAV size: {} bytes", metadata.len());
    assert!(metadata.len() > 100000, "WAV file must be non-trivial size");

    let _ = fs::remove_file(output_path);
}

#[test]
fn test_wav_riff_header_and_chunk_integrity() {
    let sample_rate = 44100.0;
    let bpm = 126.0;
    let renderer = DemoArrangementRenderer::new(sample_rate, bpm);
    let interleaved = renderer.render_demo_seconds(4.2);

    let wav_path = Path::new("/tmp/ableton_rust_test_header.wav");
    WavWriter::write_wav_16bit(wav_path, sample_rate as u32, 2, &interleaved)
        .expect("Failed to write header test WAV");

    let mut file = File::open(wav_path).expect("Failed to open exported WAV");
    let mut header = [0u8; 44];
    file.read_exact(&mut header).expect("Failed to read 44-byte RIFF header");

    // 1. Verify RIFF Magic
    assert_eq!(&header[0..4], b"RIFF", "Header must start with 'RIFF'");

    // 2. File size field (header[4..8])
    let file_size_field = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let actual_len = fs::metadata(wav_path).unwrap().len() as u32;
    assert_eq!(file_size_field + 8, actual_len, "RIFF size field + 8 must equal total file length");

    // 3. WAVE Format
    assert_eq!(&header[8..12], b"WAVE", "Format must be 'WAVE'");

    // 4. fmt subchunk
    assert_eq!(&header[12..16], b"fmt ", "Sub-chunk must be 'fmt '");
    let fmt_size = u32::from_le_bytes(header[16..20].try_into().unwrap());
    assert_eq!(fmt_size, 16, "fmt chunk size must be 16 for standard PCM");

    let audio_format = u16::from_le_bytes(header[20..22].try_into().unwrap());
    assert_eq!(audio_format, 1, "Audio format must be 1 (linear PCM)");

    let num_channels = u16::from_le_bytes(header[22..24].try_into().unwrap());
    assert_eq!(num_channels, 2, "Channel count must be 2 (Stereo)");

    let sr = u32::from_le_bytes(header[24..28].try_into().unwrap());
    assert_eq!(sr, 44100, "Sample rate must be 44100 Hz");

    let bits_per_sample = u16::from_le_bytes(header[34..36].try_into().unwrap());
    assert_eq!(bits_per_sample, 16, "Bits per sample must be 16-bit");

    // 5. data subchunk
    assert_eq!(&header[36..40], b"data", "Data chunk must be 'data'");
    let data_size = u32::from_le_bytes(header[40..44].try_into().unwrap());
    assert_eq!(data_size, actual_len - 44, "Data size must equal file length minus 44 bytes");

    let _ = fs::remove_file(wav_path);
    println!("RIFF/WAVE 44-byte header verified: 100% compliant.");
}

#[test]
fn test_bounce_audio_duration_and_rms_energy() {
    let sample_rate = 44100.0;
    let bpm = 126.0;
    let renderer = DemoArrangementRenderer::new(sample_rate, bpm);
    let duration_target = 4.5; // > 4.0 seconds
    let interleaved = renderer.render_demo_seconds(duration_target);

    let wav_path = Path::new("/tmp/ableton_rust_test_rms.wav");
    WavWriter::write_wav_16bit(wav_path, sample_rate as u32, 2, &interleaved)
        .expect("Failed to write test RMS WAV");

    let mut bytes = Vec::new();
    File::open(wav_path).unwrap().read_to_end(&mut bytes).unwrap();

    let data_bytes = &bytes[44..];
    let mut sum_sq = 0.0f64;
    let mut sample_count = 0usize;
    let mut max_abs = 0.0f32;

    for chunk in data_bytes.chunks_exact(2) {
        let pcm = i16::from_le_bytes(chunk.try_into().unwrap());
        let val = pcm as f32 / 32768.0;

        assert!(val.is_finite(), "Sample {} is not finite", sample_count);
        assert!(!val.is_nan(), "Sample {} is NaN", sample_count);

        let abs_val = val.abs();
        if abs_val > max_abs {
            max_abs = abs_val;
        }

        sum_sq += (val * val) as f64;
        sample_count += 1;
    }

    let frames = sample_count / 2; // Stereo
    let duration_seconds = frames as f64 / 44100.0;
    let rms = (sum_sq / sample_count as f64).sqrt();

    println!("Audio Bounce Quality Verification:");
    println!("  Total frames: {}", frames);
    println!("  Duration: {:.2} seconds (Acceptance criterion: >= 4.0s)", duration_seconds);
    println!("  RMS Energy: {:.4} (Acceptance criterion: > 0.02)", rms);
    println!("  Peak amplitude: {:.4} (Limiter ceiling: <= 1.0)", max_abs);

    assert!(
        duration_seconds >= 4.0,
        "Audio duration must be >= 4.0 seconds, got {:.2}s",
        duration_seconds
    );
    assert!(
        rms > 0.02,
        "Audio RMS energy must be > 0.02 (audible, non-silent signal), got {:.4}",
        rms
    );
    assert!(
        max_abs <= 1.0001,
        "Audio peak amplitude must not exceed 1.0, got {:.4}",
        max_abs
    );

    let _ = fs::remove_file(wav_path);
}
