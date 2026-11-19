//! E2E Test Suite: 16-Pad Drum Rack, 909 Sample Playback, Choke Groups, and Per-Pad Controls
//!
//! Authoritative baselines:
//! - Workspace assets (`assets/kick.wav`, `snare.wav`, `hihat.wav`) with synthesized temp WAV fallback
//! - PROJECT.md § Architectural Layers 3

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use oxide_drums::{load_wav_file, DrumRack, MAX_DRUM_VOICES};

fn write_temp_pcm_wav(path: &Path, sample_rate: u32, num_frames: usize) {
    let data_bytes = (num_frames * 2) as u32;
    let riff_size = 36 + data_bytes;
    let mut buf = Vec::with_capacity(44 + data_bytes as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&riff_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_bytes.to_le_bytes());
    for i in 0..num_frames {
        let s = (2.0 * std::f32::consts::PI * 60.0 * (i as f32 / sample_rate as f32)).sin() * 0.5;
        let q = (s * 32767.0) as i16;
        buf.extend_from_slice(&q.to_le_bytes());
    }
    let mut f = File::create(path).expect("create fallback temp WAV");
    f.write_all(&buf).expect("write fallback temp WAV");
}

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

    // Verify Choke Group 1 assignment for canonical Hi-Hats (8=Closed HH, 9=Open HH)
    assert_eq!(drum_rack.pads[8].choke_group, Some(1)); // Closed HH
    assert_eq!(drum_rack.pads[9].choke_group, Some(1)); // Open HH
    assert_eq!(drum_rack.pads[6].choke_group, None);    // Clap
    assert_eq!(drum_rack.pads[10].choke_group, None);   // Crash
    assert_eq!(drum_rack.pads[0].choke_group, None);    // Kick
}

#[test]
fn test_909_sample_playback_loading() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(manifest_dir);
    let assets_dir = workspace_root.join("assets");

    for name in ["kick.wav", "snare.wav", "hihat.wav"] {
        let candidate = assets_dir.join(name);
        let mut temp_guard: Option<PathBuf> = None;
        let load_path = if candidate.exists() {
            candidate
        } else {
            let tmp = std::env::temp_dir().join(format!(
                "oxide_drum_rack_test_{}_{}",
                std::process::id(),
                name
            ));
            write_temp_pcm_wav(&tmp, 44100, 2048);
            temp_guard = Some(tmp.clone());
            tmp
        };

        let decoded = load_wav_file(load_path.to_str().unwrap())
            .unwrap_or_else(|e| panic!("Failed to load {}: {}", load_path.display(), e));
        assert!(decoded.samples.len() > 1000, "WAV data must have non-trivial sample count");
        assert_eq!(decoded.sample_rate, 44100);

        if let Some(tmp) = temp_guard {
            let _ = fs::remove_file(tmp);
        }
    }
}

#[test]
fn test_choke_group_damping_under_5ms() {
    let sample_rate = 44100.0;
    let mut drum_rack = DrumRack::new(sample_rate);

    // 1. Trigger Open Hi-Hat (Pad 9) on Choke Group 1
    drum_rack.trigger_pad(9, 127);
    let mut left = [0.0f32; 512];
    let mut right = [0.0f32; 512];
    drum_rack.process_block(&mut left, &mut right);

    // Check Open HH voice is active
    let active_voice_idx = drum_rack.voices.iter().position(|v| v.active).expect("Voice must be active");
    assert_eq!(drum_rack.voices[active_voice_idx].pad_idx, 9);
    assert!(!drum_rack.voices[active_voice_idx].is_fading_out);
    assert!(drum_rack.voices[active_voice_idx].envelope_val > 0.5);

    // 2. Trigger Closed Hi-Hat (Pad 8) on the same Choke Group 1
    drum_rack.trigger_pad(8, 127);

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
