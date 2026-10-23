//! End-to-end integration tests for 1-Click Remix Stem Extractor & Multi-Stem Bounce.

use std::fs;

use oxide_bounce::stems::{
    export_all_tracks_as_stems, export_remix_stems_from_wav, read_wav_file,
    separate_audio_into_4_stems,
};
use oxide_bounce::WavWriter;
use oxide_core::AudioBuffer;
use oxide_project::{DAWProject, TrackDefinition};
use oxide_timeline::{ArrangementClip, ClipPattern, DrumStep, DrumStepPattern, MidiNote, MidiPattern};

#[test]
fn test_4_stem_separation_on_mixed_audio_buffers() {
    let mut buffer = AudioBuffer::new();
    let frame_count = 1024;
    buffer.frame_count = frame_count;

    let sample_rate = 44100.0f32;
    for i in 0..frame_count {
        let t = i as f32 / sample_rate;

        // 1. Bass: 55 Hz sine wave (< 140 Hz)
        let bass = (2.0 * std::f32::consts::PI * 55.0 * t).sin() * 0.40;

        // 2. Vocals: 1000 Hz center-channel harmonic tone (220 Hz - 4.5 kHz)
        let vocals = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.30;

        // 3. Other: 7500 Hz high frequency stereo pad (> 4.5 kHz)
        let synth_l = (2.0 * std::f32::consts::PI * 7500.0 * t).sin() * 0.20;
        let synth_r = -(2.0 * std::f32::consts::PI * 7500.0 * t).sin() * 0.20;

        // 4. Drums: fast transient impulses at frames 100 and 500
        let click = if (100..105).contains(&i) || (500..505).contains(&i) {
            0.75
        } else {
            0.0
        };

        buffer.left[i] = bass + vocals + synth_l + click;
        buffer.right[i] = bass + vocals + synth_r + click;
    }

    let stems = separate_audio_into_4_stems(&buffer);

    // Verify all 4 stems have matching frame count
    assert_eq!(stems.drums.frame_count, frame_count);
    assert_eq!(stems.bass.frame_count, frame_count);
    assert_eq!(stems.vocals.frame_count, frame_count);
    assert_eq!(stems.other.frame_count, frame_count);

    // Verify all samples are finite and non-NaN
    for i in 0..frame_count {
        assert!(stems.drums.left[i].is_finite(), "Drum L[{}] not finite", i);
        assert!(stems.drums.right[i].is_finite(), "Drum R[{}] not finite", i);
        assert!(stems.bass.left[i].is_finite(), "Bass L[{}] not finite", i);
        assert!(stems.bass.right[i].is_finite(), "Bass R[{}] not finite", i);
        assert!(stems.vocals.left[i].is_finite(), "Vocal L[{}] not finite", i);
        assert!(stems.vocals.right[i].is_finite(), "Vocal R[{}] not finite", i);
        assert!(stems.other.left[i].is_finite(), "Other L[{}] not finite", i);
        assert!(stems.other.right[i].is_finite(), "Other R[{}] not finite", i);
    }

    // Verify each stem has non-zero RMS energy
    let rms_drums = calculate_rms(&stems.drums);
    let rms_bass = calculate_rms(&stems.bass);
    let rms_vocals = calculate_rms(&stems.vocals);
    let rms_other = calculate_rms(&stems.other);

    assert!(rms_drums > 0.005, "Drums should have transient energy, got {}", rms_drums);
    assert!(rms_bass > 0.05, "Bass should have low-frequency energy, got {}", rms_bass);
    assert!(rms_vocals > 0.05, "Vocals should have mid-frequency energy, got {}", rms_vocals);
    assert!(rms_other > 0.02, "Other should have high/stereo energy, got {}", rms_other);
}

#[test]
fn test_stem_energy_conservation_and_exact_sum() {
    let mut buffer = AudioBuffer::new();
    let frame_count = 800;
    buffer.frame_count = frame_count;

    let sample_rate = 44100.0f32;
    for i in 0..frame_count {
        let t = i as f32 / sample_rate;
        let s_low = (2.0 * std::f32::consts::PI * 80.0 * t).sin() * 0.35;
        let s_mid = (2.0 * std::f32::consts::PI * 1200.0 * t).sin() * 0.25;
        let s_hi = (2.0 * std::f32::consts::PI * 9000.0 * t).cos() * 0.20;
        let click = if i % 150 < 3 { 0.6 } else { 0.0 };

        buffer.left[i] = s_low + s_mid + s_hi + click;
        buffer.right[i] = s_low + s_mid * 0.8 - s_hi * 0.9 + click;
    }

    let orig_rms = calculate_rms(&buffer);
    let stems = separate_audio_into_4_stems(&buffer);

    let mut sum_buf = AudioBuffer::new();
    sum_buf.frame_count = frame_count;

    for i in 0..frame_count {
        let sum_l = stems.drums.left[i] + stems.bass.left[i] + stems.vocals.left[i] + stems.other.left[i];
        let sum_r = stems.drums.right[i] + stems.bass.right[i] + stems.vocals.right[i] + stems.other.right[i];

        sum_buf.left[i] = sum_l;
        sum_buf.right[i] = sum_r;

        let diff_l = (sum_l - buffer.left[i]).abs();
        let diff_r = (sum_r - buffer.right[i]).abs();

        assert!(
            diff_l < 1e-4,
            "Reconstruction error on Left[{}] = {} exceeds 1e-4",
            i,
            diff_l
        );
        assert!(
            diff_r < 1e-4,
            "Reconstruction error on Right[{}] = {} exceeds 1e-4",
            i,
            diff_r
        );
    }

    let sum_rms = calculate_rms(&sum_buf);
    let rms_diff = (sum_rms - orig_rms).abs();
    assert!(
        rms_diff < 1e-4,
        "Energy conservation failed: original RMS = {}, sum RMS = {}, diff = {}",
        orig_rms,
        sum_rms,
        rms_diff
    );
}

#[test]
fn test_stem_separation_edge_cases() {
    // 1. Silence buffer
    let silence = AudioBuffer::silence(256);
    let res = separate_audio_into_4_stems(&silence);
    assert_eq!(res.drums.frame_count, 256);
    assert_eq!(calculate_rms(&res.drums), 0.0);
    assert_eq!(calculate_rms(&res.bass), 0.0);
    assert_eq!(calculate_rms(&res.vocals), 0.0);
    assert_eq!(calculate_rms(&res.other), 0.0);

    // 2. Empty buffer
    let empty = AudioBuffer::new();
    let res_empty = separate_audio_into_4_stems(&empty);
    assert_eq!(res_empty.drums.frame_count, 0);

    // 3. DC offset buffer
    let mut dc = AudioBuffer::new();
    dc.frame_count = 512;
    dc.left[..512].fill(0.5);
    dc.right[..512].fill(0.5);
    let res_dc = separate_audio_into_4_stems(&dc);
    for i in 0..512 {
        let sum = res_dc.drums.left[i] + res_dc.bass.left[i] + res_dc.vocals.left[i] + res_dc.other.left[i];
        assert!((sum - 0.5).abs() < 1e-4);
    }
}

#[test]
fn test_multi_stem_export_all_tracks_to_disk() {
    let temp_dir = std::env::temp_dir().join(format!("test_multi_stem_export_{}", std::process::id()));
    let mut project = DAWProject::new("Oxide Studio Stem Session");
    project.transport.bpm = 128.0;

    // Track 1: 909 Kick & Percussion
    let mut kick_trk = TrackDefinition::new_drum_rack(1, "909 Drums");
    kick_trk.mixer.volume_db = -1.5;
    let mut drum_pat = DrumStepPattern::new_16_step();
    drum_pat.set_step(0, 0, DrumStep::new(true, 120, 36));
    drum_pat.set_step(0, 4, DrumStep::new(true, 115, 36));
    drum_pat.set_step(0, 8, DrumStep::new(true, 120, 36));
    drum_pat.set_step(0, 12, DrumStep::new(true, 115, 36));
    drum_pat.set_step(2, 4, DrumStep::new(true, 105, 38));
    drum_pat.set_step(2, 12, DrumStep::new(true, 105, 38));
    let clip1 = ArrangementClip::new(1, "Drum Clip", 0.0, 8.0, ClipPattern::Drum(drum_pat));
    kick_trk.arrangement_clips.push(clip1);

    // Track 2: Sub-bass 303
    let mut bass_trk = TrackDefinition::new_synth(2, "Acid Sub 303");
    bass_trk.mixer.volume_db = -2.0;
    let mut midi_pat = MidiPattern::new(8.0);
    midi_pat.add_note(MidiNote::new(36, 110, 0.0, 1.5));
    midi_pat.add_note(MidiNote::new(38, 105, 2.0, 1.5));
    midi_pat.add_note(MidiNote::new(41, 115, 4.0, 1.5));
    midi_pat.add_note(MidiNote::new(43, 110, 6.0, 1.5));
    let clip2 = ArrangementClip::new(2, "Bass Clip", 0.0, 8.0, ClipPattern::Midi(midi_pat));
    bass_trk.arrangement_clips.push(clip2);

    // Track 3: Lead Synth
    let mut lead_trk = TrackDefinition::new_synth(3, "Lead Melody");
    lead_trk.mixer.volume_db = -4.0;
    let clip3 = ArrangementClip::new(3, "Lead Clip", 0.0, 8.0, ClipPattern::Midi(MidiPattern::new(8.0)));
    lead_trk.arrangement_clips.push(clip3);

    project.tracks.push(kick_trk);
    project.tracks.push(bass_trk);
    project.tracks.push(lead_trk);

    let exported = export_all_tracks_as_stems(&project, &temp_dir)
        .expect("Multi-track stem export must succeed");

    assert_eq!(exported.len(), 7, "Expected 3 track WAVs + 4 remix stems = 7 files");

    let required_files = [
        "track_01_909_drums.wav",
        "track_02_acid_sub_303.wav",
        "track_03_lead_melody.wav",
        "remix_drums.wav",
        "remix_bass.wav",
        "remix_vocals.wav",
        "remix_other.wav",
    ];

    for fname in &required_files {
        let fpath = temp_dir.join(fname);
        assert!(fpath.exists(), "Expected file {:?} does not exist", fpath);

        let (sr, channels, samples) = read_wav_file(&fpath)
            .unwrap_or_else(|e| panic!("Failed reading WAV {:?}: {}", fpath, e));

        assert_eq!(sr, 44100);
        assert_eq!(channels, 2);
        assert!(!samples.is_empty(), "WAV sample data must not be empty");

        // Duration must be at least 4.0s
        let duration = samples.len() as f64 / (2.0 * sr as f64);
        assert!(
            duration >= 3.7,
            "WAV duration {:?} must be >= 3.7s, got {:.2}s",
            fpath,
            duration
        );

        // All samples must be finite
        for &s in &samples {
            assert!(s.is_finite());
            assert!(!s.is_nan());
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_export_remix_stems_from_wav_file() {
    let temp_dir = std::env::temp_dir().join(format!("test_remix_wav_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();

    let input_wav = temp_dir.join("input_master.wav");
    let sample_rate = 44100u32;
    let total_frames = (sample_rate as f64 * 4.0) as usize;
    let mut interleaved = Vec::with_capacity(total_frames * 2);

    for i in 0..total_frames {
        let t = i as f32 / sample_rate as f32;
        let kick = if (i % 22050) < 50 { 0.8 } else { 0.0 };
        let bass = (2.0 * std::f32::consts::PI * 70.0 * t).sin() * 0.35;
        let voc = (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.25;
        let pad = (2.0 * std::f32::consts::PI * 6500.0 * t).cos() * 0.15;

        interleaved.push(kick + bass + voc + pad);
        interleaved.push(kick + bass + voc - pad);
    }

    WavWriter::write_wav_16bit(&input_wav, sample_rate, 2, &interleaved).unwrap();
    assert!(input_wav.exists());

    let stems_out = temp_dir.join("stems_out");
    let files = export_remix_stems_from_wav(&input_wav, &stems_out)
        .expect("Remix separation from WAV must succeed");

    assert!(files.drums.exists(), "remix_drums.wav must exist");
    assert!(files.bass.exists(), "remix_bass.wav must exist");
    assert!(files.vocals.exists(), "remix_vocals.wav must exist");
    assert!(files.other.exists(), "remix_other.wav must exist");

    let (_sr, _ch, drums_samples) = read_wav_file(&files.drums).unwrap();
    let (_sr, _ch, bass_samples) = read_wav_file(&files.bass).unwrap();
    let (_sr, _ch, voc_samples) = read_wav_file(&files.vocals).unwrap();
    let (_sr, _ch, other_samples) = read_wav_file(&files.other).unwrap();

    assert!(!drums_samples.is_empty());
    assert!(!bass_samples.is_empty());
    assert!(!voc_samples.is_empty());
    assert!(!other_samples.is_empty());

    let _ = fs::remove_dir_all(&temp_dir);
}

fn calculate_rms(buf: &AudioBuffer) -> f32 {
    if buf.frame_count == 0 {
        return 0.0;
    }
    let mut sum_sq = 0.0f64;
    for i in 0..buf.frame_count {
        sum_sq += (buf.left[i] * buf.left[i]) as f64;
        sum_sq += (buf.right[i] * buf.right[i]) as f64;
    }
    ((sum_sq / (buf.frame_count * 2) as f64).sqrt()) as f32
}
