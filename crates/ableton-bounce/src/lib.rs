//! Offline bounce and WAV rendering engine for ableton-rust.
//!
//! Provides:
//! - Offline multi-track rendering combining synthesizer and 909 drum sequences
//! - Lightweight 16-bit PCM (TPDF dithered) and 32-bit float stereo WAV writer with 44-byte RIFF headers
//! - Verification of exported audio duration (>= 4.0s), RMS energy (> 0.02), and zero NaN/Inf

pub mod audio_to_midi;
pub mod pack_formats;
pub mod renderer;
pub mod stems;
pub mod wav_writer;

pub use audio_to_midi::{AudioToMidiConverter, GrooveTemplate, TranscribedMidiNote};
pub use pack_formats::{
    calculate_crc32, AlpError, AlpPackArchive, AlpPackEntry, DawClip, DawNote, DawProject,
    DawProjectError, DawProjectExchange, DawTrack, FreezeCodecError, LosslessFreezeCodec,
};
pub use renderer::DemoArrangementRenderer;
pub use stems::{
    export_all_tracks_as_stems, export_remix_stems, export_remix_stems_from_wav,
    separate_audio_into_4_stems, separate_audio_streams, separate_interleaved_samples,
    BounceError, RemixStemsFiles, StemSeparationResult,
};
pub use wav_writer::WavWriter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Read;
    use std::path::Path;

    #[test]
    fn test_render_and_export_16bit_wav() {
        let renderer = DemoArrangementRenderer::new(44100.0, 126.0);
        let duration = 4.2; // > 4.0s
        let interleaved = renderer.render_demo_seconds(duration);

        assert_eq!(interleaved.len(), (4.2 * 44100.0 * 2.0) as usize);

        let out_path = Path::new("/tmp/test_ableton_bounce_16bit.wav");
        WavWriter::write_wav_16bit(out_path, 44100, 2, &interleaved)
            .expect("Failed to write 16-bit WAV");

        assert!(out_path.exists());
        let meta = fs::metadata(out_path).unwrap();
        assert_eq!(meta.len(), 44 + (interleaved.len() * 2) as u64);

        // Verify header
        let mut f = File::open(out_path).unwrap();
        let mut header = [0u8; 44];
        f.read_exact(&mut header).unwrap();

        assert_eq!(&header[0..4], b"RIFF");
        assert_eq!(&header[8..12], b"WAVE");
        assert_eq!(&header[12..16], b"fmt ");
        assert_eq!(&header[36..40], b"data");

        // Verify RMS and finiteness
        let mut sum_sq = 0.0f64;
        for &s in &interleaved {
            assert!(s.is_finite());
            assert!(!s.is_nan());
            assert!(s.abs() <= 1.0);
            sum_sq += (s * s) as f64;
        }
        let rms = (sum_sq / interleaved.len() as f64).sqrt();
        assert!(rms > 0.02, "RMS energy must be > 0.02, got {}", rms);

        let _ = fs::remove_file(out_path);
    }

    #[test]
    fn test_render_and_export_32bit_float_wav() {
        let renderer = DemoArrangementRenderer::new(44100.0, 126.0);
        let interleaved = renderer.render_demo_seconds(4.0);

        let out_path = Path::new("/tmp/test_ableton_bounce_32bit.wav");
        WavWriter::write_wav_32bit_float(out_path, 44100, 2, &interleaved)
            .expect("Failed to write 32-bit float WAV");

        assert!(out_path.exists());
        let meta = fs::metadata(out_path).unwrap();
        assert_eq!(meta.len(), 44 + (interleaved.len() * 4) as u64);

        let mut f = File::open(out_path).unwrap();
        let mut header = [0u8; 44];
        f.read_exact(&mut header).unwrap();
        let audio_format = u16::from_le_bytes([header[20], header[21]]);
        assert_eq!(audio_format, 3, "Audio format must be 3 for IEEE float");

        let _ = fs::remove_file(out_path);
    }

    #[test]
    fn test_tpdf_dither_export() {
        let renderer = DemoArrangementRenderer::new(44100.0, 120.0);
        let interleaved = renderer.render_demo_seconds(4.0);

        let out_path = Path::new("/tmp/test_ableton_bounce_tpdf.wav");
        WavWriter::write_wav_16bit_tpdf(out_path, 44100, 2, &interleaved)
            .expect("Failed to write TPDF dithered WAV");

        assert!(out_path.exists());
        let _ = fs::remove_file(out_path);
    }
}
