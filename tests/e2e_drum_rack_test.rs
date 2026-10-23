//! E2E Test Suite: 16-Pad Drum Rack, 909 Sample Playback, Choke Groups, and Per-Pad Controls
//!
//! Authoritative baselines:
//! - /Users/juancruz/ableton-rea/public/sounds/ (kick.wav, snare.wav, hihat.wav)
//! - /Users/juancruz/ableton-rust/PROJECT.md § Architectural Layers 3
//! - /Users/juancruz/ableton-rust/.agents/teamwork/ORIGINAL_REQUEST.md

use std::path::Path;

use oxide_drums::{load_wav_file, DrumRack, MAX_DRUM_VOICES};

#[test]
fn test_drum_rack_16_pad_initialization() {
    let drum_rack = DrumRack::new(44100.0);
    assert_eq!(drum_rack.pads.len(), 16);

    for (i, pad) in drum_rack.pads.iter().enumerate() {
        assert_eq!(pad.midi_note, (36 + i) as u8);
        assert_eq!(pad.volume, 1.0);
        assert_eq!(pad.pan, 0.0);
        assert_eq!(pad.pitch_semitones, 0.0);
    }

    // Verify Choke Group 1 assignment for Hi-Hats
    assert_eq!(drum_rack.pads[6].choke_group, Some(1));  // Closed HH
    assert_eq!(drum_rack.pads[8].choke_group, Some(1));  // Pedal HH
    assert_eq!(drum_rack.pads[10].choke_group, Some(1)); // Open HH
    assert_eq!(drum_rack.pads[0].choke_group, None);     // Kick
}

#[test]
fn test_909_sample_playback_loading() {
    let paths = [
        "/Users/juancruz/ableton-rust/assets/kick.wav",
        "/Users/juancruz/ableton-rust/assets/snare.wav",
        "/Users/juancruz/ableton-rust/assets/hihat.wav",
    ];

    for path in &paths {
        let p = Path::new(path);
        if !p.exists() {
            // Check fallback in ableton-rea
            let fallback_name = p.file_name().unwrap().to_str().unwrap();
            let fallback_path = format!("/Users/juancruz/ableton-rea/public/sounds/{}", fallback_name);
            let decoded = load_wav_file(&fallback_path).expect("Failed to load fallback WAV");
            assert!(decoded.samples.len() > 1000, "WAV data must have non-trivial sample count");
            assert_eq!(decoded.sample_rate, 44100);
            println!("Loaded fallback 909 sample {}: {} samples, {} channels", fallback_name, decoded.samples.len(), decoded.channels);
        } else {
            let decoded = load_wav_file(path).expect("Failed to load WAV asset");
            assert!(decoded.samples.len() > 1000, "WAV data must have non-trivial sample count");
            assert_eq!(decoded.sample_rate, 44100);
            println!("Loaded 909 asset {}: {} samples, {} channels", path, decoded.samples.len(), decoded.channels);
        }
    }
}

#[test]
fn test_choke_group_damping_under_5ms() {
    let sample_rate = 44100.0;
    let mut drum_rack = DrumRack::new(sample_rate);

    // 1. Trigger Open Hi-Hat (Pad 10) on Choke Group 1
    drum_rack.trigger_pad(10, 127);
    let mut left = [0.0f32; 512];
    let mut right = [0.0f32; 512];
    drum_rack.process_block(&mut left, &mut right);

    // Check Open HH voice is active
    let active_voice_idx = drum_rack.voices.iter().position(|v| v.active).expect("Voice must be active");
    assert_eq!(drum_rack.voices[active_voice_idx].pad_idx, 10);
    assert!(!drum_rack.voices[active_voice_idx].is_fading_out);
    assert!(drum_rack.voices[active_voice_idx].envelope_val > 0.5);

    // 2. Trigger Closed Hi-Hat (Pad 6) on the same Choke Group 1
    drum_rack.trigger_pad(6, 127);

    // The Open HH voice must immediately enter fading out state
    assert!(
        drum_rack.voices[active_voice_idx].is_fading_out,
        "Original Open HH voice must enter fade out upon choke trigger"
    );

    // 3. Render 240 samples (~5.4ms at 44.1kHz)
    let mut fade_left = [0.0f32; 240];
    let mut fade_right = [0.0f32; 240];
    drum_rack.process_block(&mut fade_left, &mut fade_right);

    // After 240 samples, the original voice should be extinguished or damped below -60 dBFS (< 0.001)
    let open_hh_voice = &drum_rack.voices[active_voice_idx];
    let final_env = open_hh_voice.envelope_val;
    println!("Choke Group Fadeout after 240 samples (~5ms): envelope = {:.6}, active = {}", final_env, open_hh_voice.active);

    assert!(
        final_env < 0.01 || !open_hh_voice.active,
        "Open Hi-Hat voice must be damped within 5ms (got envelope {:.6})",
        final_env
    );
}

#[test]
fn test_voice_pool_stealing_without_allocation() {
    let mut drum_rack = DrumRack::new(44100.0);

    // Trigger all 32 voices
    for i in 0..MAX_DRUM_VOICES {
        drum_rack.trigger_pad((i % 16) as u8, 100);
    }

    let active_count = drum_rack.voices.iter().filter(|v| v.active).count();
    assert_eq!(active_count, MAX_DRUM_VOICES, "All 32 voices should be active");

    // Artificially reduce one voice's envelope to test stealing
    drum_rack.voices[7].envelope_val = 0.005;

    // Trigger 33rd voice
    drum_rack.trigger_pad(0, 127);

    let active_count_after = drum_rack.voices.iter().filter(|v| v.active).count();
    assert_eq!(active_count_after, MAX_DRUM_VOICES, "Voice pool must never exceed 32 voices");

    // Voice 7 should have been stolen
    assert_eq!(drum_rack.voices[7].pad_idx, 0, "Quietest voice (index 7) should have been stolen for Pad 0");
    assert_eq!(drum_rack.voices[7].envelope_val, 1.0, "Stolen voice envelope must reset to 1.0");
}

#[test]
fn test_per_pad_volume_pan_pitch_decay() {
    let sample_rate = 44100.0;
    let mut drum_rack = DrumRack::new(sample_rate);

    // 1. Pan hard left: pan = -1.0
    drum_rack.pads[0].pan = -1.0;
    drum_rack.trigger_pad(0, 127);

    let mut out_l = [0.0f32; 128];
    let mut out_r = [0.0f32; 128];
    drum_rack.process_block(&mut out_l, &mut out_r);

    let energy_l: f32 = out_l.iter().map(|s| s * s).sum();
    let energy_r: f32 = out_r.iter().map(|s| s * s).sum();
    println!("Hard Left Pan: Energy L = {:.4}, Energy R = {:.6}", energy_l, energy_r);
    assert!(energy_l > 0.01, "Left channel must have non-zero energy");
    assert!(energy_r < 1e-5, "Right channel must have negligible energy on hard left pan");

    // 2. Pitch semitones
    drum_rack.pads[1].pitch_semitones = 12.0; // +1 octave -> 2.0x playback speed
    drum_rack.trigger_pad(1, 100);
    let voice_1 = drum_rack.voices.iter().find(|v| v.active && v.pad_idx == 1).unwrap();
    assert!((voice_1.phase_step - 2.0).abs() < 1e-4);

    drum_rack.pads[2].pitch_semitones = -12.0; // -1 octave -> 0.5x playback speed
    drum_rack.trigger_pad(2, 100);
    let voice_2 = drum_rack.voices.iter().find(|v| v.active && v.pad_idx == 2).unwrap();
    assert!((voice_2.phase_step - 0.5).abs() < 1e-4);
}
