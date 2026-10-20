//! 16-pad Electronic Drum Rack and 909 percussion engine for ableton-rust.

pub mod drum_rack;
pub mod simpler;
pub mod synth_models;
pub mod voice;
pub mod wav_loader;

pub use drum_rack::{
    DrumPad, DrumPadSource, DrumRack, MAX_DRUM_VOICES, NUM_BANKS, NUM_DRUM_PADS, PADS_PER_BANK,
    TOTAL_DRUM_PADS,
};
pub use simpler::{
    OneShotRetrigger, SimplerDevice, SimplerMode, SimplerVoice, SimplerWarpMode, SlicingMode,
};
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
        assert_eq!(rack.pads[6].name, "Closed HH");
        assert_eq!(rack.pads[6].choke_group, Some(1));
        assert_eq!(rack.pads[10].choke_group, Some(1));
    }

    #[test]
    fn test_choke_group_damping() {
        let mut rack = DrumRack::new(44100.0);
        // Trigger Open HH
        rack.trigger_pad(10, 127);
        let mut left = [0.0; 256];
        let mut right = [0.0; 256];
        rack.process_block(&mut left, &mut right);

        let active_idx = rack.voices.iter().position(|v| v.active).unwrap();
        assert_eq!(rack.voices[active_idx].pad_idx, 10);
        assert!(!rack.voices[active_idx].is_fading_out);

        // Trigger Closed HH on same choke group
        rack.trigger_pad(6, 127);
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
        let path = "/Users/juancruz/ableton-rust/assets/kick.wav";
        if Path::new(path).exists() {
            let res = load_wav_file(path);
            assert!(res.is_ok());
            let decoded = res.unwrap();
            assert_eq!(decoded.sample_rate, 44100);
            assert!(decoded.samples.len() > 1000);
        }
    }
}
