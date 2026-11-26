//! 16-pad Electronic Drum Rack and 909 percussion engine for Oxide Studio.

pub mod drum_rack;
pub mod simpler;
pub mod suite_instruments;
pub mod synth_models;
pub mod voice;
pub mod wav_loader;

pub use drum_rack::{
    DrumPad, DrumPadSource, DrumRack, DrumRackMacroMapping, DrumRackMacroTarget, MAX_DRUM_VOICES,
    NUM_BANKS, NUM_DRUM_PADS, PADS_PER_BANK, TOTAL_DRUM_PADS,
};
pub use simpler::{
    OneShotRetrigger, SimplerDevice, SimplerMode, SimplerVoice, SimplerWarpMode, SlicingMode,
};
pub use suite_instruments::{GranulatorIIIEngine, SamplerEngine, SuiteInstrumentEngine, SuiteInstrumentKind};
pub use synth_models::{SynthDrumType, SynthPercussion};
pub use voice::DrumVoice;
pub use wav_loader::{load_wav_file, DecodedWav};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_drum_rack_init() {
        let rack = DrumRack::new(44100.0);
        assert_eq!(rack.pads.len(), 16);
        assert_eq!(rack.voices.len(), 32);
        assert_eq!(rack.pads[0].name, "Kick");
        assert_eq!(rack.pads[6].name, "Clap");
        assert_eq!(rack.pads[6].choke_group, None);
        assert_eq!(rack.pads[8].name, "Closed HH");
        assert_eq!(rack.pads[8].choke_group, Some(1));
        assert_eq!(rack.pads[9].name, "Open HH");
        assert_eq!(rack.pads[9].choke_group, Some(1));
        assert_eq!(rack.pads[10].name, "Crash");
        assert_eq!(rack.pads[10].choke_group, None);
    }

    #[test]
    fn test_choke_group_damping() {
        let mut rack = DrumRack::new(44100.0);
        // Trigger Open HH (pad 9)
        rack.trigger_pad(9, 127);
        let mut left = [0.0; 256];
        let mut right = [0.0; 256];
        rack.process_block(&mut left, &mut right);

        let active_idx = rack.voices.iter().position(|v| v.active).unwrap();
        assert_eq!(rack.voices[active_idx].pad_idx, 9);
        assert!(!rack.voices[active_idx].is_fading_out);

        // Trigger Closed HH (pad 8) on same choke group
        rack.trigger_pad(8, 127);
        assert!(rack.voices[active_idx].is_fading_out);

        // Process 240 samples (< 5.5ms)
        let mut f_l = [0.0; 240];
        let mut f_r = [0.0; 240];
        rack.process_block(&mut f_l, &mut f_r);

        let env = rack.voices[active_idx].envelope_val;
        assert!(
            env < 0.001 || !rack.voices[active_idx].active,
            "Choke group must damp voice below -60 dBFS (<0.001) in <5ms, got {}",
            env
        );
    }

    #[test]
    fn test_clap_and_crash_are_not_choked_by_hihats() {
        let mut rack = DrumRack::new(44100.0);
        rack.trigger_pad(6, 127); // Clap
        rack.trigger_pad(10, 127); // Crash
        rack.trigger_pad(8, 127); // Closed HH (choke group 1)

        let clap_voice = rack.voices.iter().find(|v| v.active && v.pad_idx == 6).unwrap();
        let crash_voice = rack.voices.iter().find(|v| v.active && v.pad_idx == 10).unwrap();
        assert!(!clap_voice.is_fading_out, "Clap must not be choked by Hi-Hat");
        assert!(!crash_voice.is_fading_out, "Crash must not be choked by Hi-Hat");
    }

    #[test]
    fn test_constant_power_pan() {
        let mut rack = DrumRack::new(44100.0);
        // Hard left
        rack.pads[0].pan = -1.0;
        rack.trigger_pad(0, 127);
        let mut l = [0.0; 128];
        let mut r = [0.0; 128];
        rack.process_block(&mut l, &mut r);

        let el: f32 = l.iter().map(|s| s * s).sum();
        let er: f32 = r.iter().map(|s| s * s).sum();
        assert!(el > 0.01);
        assert!(er < 1e-6);
    }

    #[test]
    fn test_voice_stealing() {
        let mut rack = DrumRack::new(44100.0);
        for i in 0..32 {
            rack.trigger_pad((i % 16) as u8, 100);
        }
        let active = rack.voices.iter().filter(|v| v.active).count();
        assert_eq!(active, 32);

        // Reduce one voice envelope
        rack.voices[5].envelope_val = 0.001;
        // Trigger 33rd
        rack.trigger_pad(0, 127);
        assert_eq!(rack.voices[5].pad_idx, 0);
        assert_eq!(rack.voices[5].envelope_val, 1.0);
    }

    #[test]
    fn test_sample_loading_if_present() {
        let asset_kick = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/kick.wav");
        let (wav_path, cleanup) = if asset_kick.exists() {
            (asset_kick, false)
        } else {
            let temp_wav = std::env::temp_dir().join("oxide_drums_synthetic_kick.wav");
            let num_frames: u32 = 2048;
            let data_bytes = num_frames * 2;
            let riff_size = 36 + data_bytes;
            let mut bytes = Vec::with_capacity(44 + data_bytes as usize);
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&riff_size.to_le_bytes());
            bytes.extend_from_slice(b"WAVE");
            bytes.extend_from_slice(b"fmt ");
            bytes.extend_from_slice(&16u32.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
            bytes.extend_from_slice(&1u16.to_le_bytes()); // Mono
            bytes.extend_from_slice(&44100u32.to_le_bytes());
            bytes.extend_from_slice(&(44100u32 * 2).to_le_bytes());
            bytes.extend_from_slice(&2u16.to_le_bytes());
            bytes.extend_from_slice(&16u16.to_le_bytes());
            bytes.extend_from_slice(b"data");
            bytes.extend_from_slice(&data_bytes.to_le_bytes());
            for i in 0..num_frames {
                let sample = (((i as f32 / 44100.0) * 60.0 * std::f32::consts::TAU).sin() * 16000.0) as i16;
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            std::fs::write(&temp_wav, &bytes).unwrap();
            (temp_wav, true)
        };

        let res = load_wav_file(&wav_path);
        assert!(res.is_ok(), "WAV loading failed: {:?}", res.err());
        let decoded = res.unwrap();
        assert_eq!(decoded.sample_rate, 44100);
        assert!(decoded.samples.len() > 1000);

        if cleanup {
            let _ = std::fs::remove_file(&wav_path);
        }
    }

    #[test]
    fn test_dsp13_wav_loader_rejects_zero_sr_and_sanitizes_nan() {
        // 1. WAV with sample_rate == 0 must return Err
        let zero_sr_path = std::env::temp_dir().join("oxide_drums_dsp13_zero_sr.wav");
        let mut zbytes = Vec::new();
        zbytes.extend_from_slice(b"RIFF");
        zbytes.extend_from_slice(&40u32.to_le_bytes());
        zbytes.extend_from_slice(b"WAVE");
        zbytes.extend_from_slice(b"fmt ");
        zbytes.extend_from_slice(&16u32.to_le_bytes());
        zbytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        zbytes.extend_from_slice(&1u16.to_le_bytes()); // Mono
        zbytes.extend_from_slice(&0u32.to_le_bytes()); // sample_rate = 0
        zbytes.extend_from_slice(&0u32.to_le_bytes());
        zbytes.extend_from_slice(&2u16.to_le_bytes());
        zbytes.extend_from_slice(&16u16.to_le_bytes());
        zbytes.extend_from_slice(b"data");
        zbytes.extend_from_slice(&4u32.to_le_bytes());
        zbytes.extend_from_slice(&[0u8; 4]);
        std::fs::write(&zero_sr_path, &zbytes).unwrap();
        let res_zero = load_wav_file(&zero_sr_path);
        let _ = std::fs::remove_file(&zero_sr_path);
        assert!(res_zero.is_err(), "WAV with sample_rate == 0 must be rejected");

        // 2. 32-bit float WAV with NaN/Inf must sanitize to finite 0.0
        let nan_path = std::env::temp_dir().join("oxide_drums_dsp13_nan_f32.wav");
        let mut fbytes = Vec::new();
        fbytes.extend_from_slice(b"RIFF");
        fbytes.extend_from_slice(&48u32.to_le_bytes());
        fbytes.extend_from_slice(b"WAVE");
        fbytes.extend_from_slice(b"fmt ");
        fbytes.extend_from_slice(&16u32.to_le_bytes());
        fbytes.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
        fbytes.extend_from_slice(&1u16.to_le_bytes());
        fbytes.extend_from_slice(&44100u32.to_le_bytes());
        fbytes.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        fbytes.extend_from_slice(&4u16.to_le_bytes());
        fbytes.extend_from_slice(&32u16.to_le_bytes());
        fbytes.extend_from_slice(b"data");
        fbytes.extend_from_slice(&12u32.to_le_bytes());
        fbytes.extend_from_slice(&0.5f32.to_le_bytes());
        fbytes.extend_from_slice(&f32::NAN.to_le_bytes());
        fbytes.extend_from_slice(&f32::INFINITY.to_le_bytes());
        std::fs::write(&nan_path, &fbytes).unwrap();
        let decoded = load_wav_file(&nan_path).expect("Valid float WAV should load");
        let _ = std::fs::remove_file(&nan_path);
        assert_eq!(decoded.samples, vec![0.5, 0.0, 0.0]);
    }
}
