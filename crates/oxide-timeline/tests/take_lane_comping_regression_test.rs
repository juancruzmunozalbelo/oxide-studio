//! Chapter 20 Regression Test: Comping & Take Lanes: Blade Splicing & Micro-Crossfades (Ableton Live 12 / Epic #25)
//!
//! Validates:
//! 1. Multi-lane recording management, take lane creation, and non-destructive auditioning.
//! 2. Blade tool splicing at exact musical beats on individual take lanes.
//! 3. Region comping into main track arrangement clips with automatic equal-power 4ms micro-crossfades.
//! 4. Mathematical equal-power law (sin^2 + cos^2 = 1.0) and real audio buffer crossfade processing at 44.1, 48, and 96 kHz.
//! 5. Track-level convenience APIs for seamless DAW timeline integration.

use oxide_timeline::{
    ArrangementClip, ClipPattern, MicroCrossfade, MidiNote, MidiPattern,
    Track, TrackCompingManager, TrackType,
};

#[test]
fn test_take_lane_creation_and_auditioning_lifecycle() {
    let mut manager = TrackCompingManager::new();
    assert_eq!(manager.take_lanes.len(), 0);
    assert!(!manager.is_lanes_visible);
    assert!(manager.active_take_id.is_none());

    // 1. Create 3 vocal take lanes
    let t1_id = manager.add_take_lane("Vocal Take 1 - Lead");
    let t2_id = manager.add_take_lane("Vocal Take 2 - Alt Harmonies");
    let t3_id = manager.add_take_lane("Vocal Take 3 - Double");

    assert_eq!(t1_id, 1);
    assert_eq!(t2_id, 2);
    assert_eq!(t3_id, 3);
    assert_eq!(manager.take_lanes.len(), 3);

    // 2. Populate each take lane with distinct takes
    let mut pat1 = MidiPattern::new(8.0);
    pat1.add_note(MidiNote::new(60, 100, 0.0, 2.0)); // C4
    let clip1 = ArrangementClip::new(101, "Take 1 Clip", 0.0, 8.0, ClipPattern::Midi(pat1));
    manager.get_take_lane_mut(t1_id).unwrap().add_clip(clip1);

    let mut pat2 = MidiPattern::new(8.0);
    pat2.add_note(MidiNote::new(64, 110, 0.0, 2.0)); // E4
    let clip2 = ArrangementClip::new(102, "Take 2 Clip", 0.0, 8.0, ClipPattern::Midi(pat2));
    manager.get_take_lane_mut(t2_id).unwrap().add_clip(clip2);

    let mut pat3 = MidiPattern::new(8.0);
    pat3.add_note(MidiNote::new(67, 120, 0.0, 2.0)); // G4
    let clip3 = ArrangementClip::new(103, "Take 3 Clip", 0.0, 8.0, ClipPattern::Midi(pat3));
    manager.get_take_lane_mut(t3_id).unwrap().add_clip(clip3);

    // Main track has baseline clip
    let mut main_pat = MidiPattern::new(8.0);
    main_pat.add_note(MidiNote::new(72, 90, 0.0, 2.0)); // C5
    let main_clip = ArrangementClip::new(10, "Main Baseline", 0.0, 8.0, ClipPattern::Midi(main_pat));
    let main_clips = vec![main_clip];

    // 3. Verify auditioning states
    // Default: no audition -> effective clips are from main track
    assert!(!manager.is_lane_auditioning(t1_id));
    assert!(!manager.is_lane_auditioning(t2_id));
    assert!(!manager.is_lane_auditioning(t3_id));
    let eff0 = manager.get_effective_clips(&main_clips);
    assert_eq!(eff0.len(), 1);
    assert_eq!(eff0[0].id, 10);
    assert_eq!(eff0[0].name, "Main Baseline");

    // Audition Take 1
    manager.set_audition_take(Some(t1_id));
    assert!(manager.is_lane_auditioning(t1_id));
    assert!(!manager.is_lane_auditioning(t2_id));
    assert!(manager.get_take_lane(t1_id).unwrap().is_auditioning);
    assert!(!manager.get_take_lane(t2_id).unwrap().is_auditioning);
    let eff1 = manager.get_effective_clips(&main_clips);
    assert_eq!(eff1.len(), 1);
    assert_eq!(eff1[0].id, 101);
    assert_eq!(eff1[0].name, "Take 1 Clip");

    // Audition Take 2 via toggle
    manager.toggle_audition_take(t2_id);
    assert!(!manager.is_lane_auditioning(t1_id));
    assert!(manager.is_lane_auditioning(t2_id));
    assert_eq!(manager.get_effective_clips(&main_clips)[0].id, 102);

    // Toggle Take 2 off -> returns to main track clips
    manager.toggle_audition_take(t2_id);
    assert!(!manager.is_lane_auditioning(t2_id));
    assert_eq!(manager.get_effective_clips(&main_clips)[0].id, 10);

    // Non-destructive check: all take lane clips still exist unaltered
    assert_eq!(manager.get_take_lane(t1_id).unwrap().clips.len(), 1);
    assert_eq!(manager.get_take_lane(t2_id).unwrap().clips.len(), 1);
    assert_eq!(manager.get_take_lane(t3_id).unwrap().clips.len(), 1);
}

#[test]
fn test_take_lane_blade_splicing() {
    let mut manager = TrackCompingManager::new();
    let lane_id = manager.add_take_lane("Synth Take");

    let pat = ClipPattern::Midi(MidiPattern::new(16.0));
    let full_clip = ArrangementClip::new(200, "Continuous Synth Solo", 0.0, 16.0, pat);
    manager.get_take_lane_mut(lane_id).unwrap().add_clip(full_clip);

    // 1. Blade slice at beat 4.0
    let split1 = manager.blade_split_take_lane(lane_id, 4.0).expect("Blade slice at 4.0 must succeed");
    assert!(split1, "Splitting at 4.0 should split the 0..16 clip into 2 pieces");

    let lane = manager.get_take_lane(lane_id).unwrap();
    assert_eq!(lane.clips.len(), 2);
    assert_eq!(lane.clips[0].timeline_start_beat, 0.0);
    assert_eq!(lane.clips[0].duration_beats, 4.0);
    assert_eq!(lane.clips[1].timeline_start_beat, 4.0);
    assert_eq!(lane.clips[1].duration_beats, 12.0);
    assert_eq!(lane.clips[1].pattern_offset_beats, 4.0);

    // 2. Blade slice at beat 12.0
    let split2 = manager.blade_split_take_lane(lane_id, 12.0).expect("Blade slice at 12.0 must succeed");
    assert!(split2, "Splitting at 12.0 should split the 4..16 clip into 2 pieces");

    let lane = manager.get_take_lane(lane_id).unwrap();
    assert_eq!(lane.clips.len(), 3);
    assert_eq!(lane.clips[0].timeline_start_beat, 0.0);
    assert_eq!(lane.clips[0].duration_beats, 4.0);

    assert_eq!(lane.clips[1].timeline_start_beat, 4.0);
    assert_eq!(lane.clips[1].duration_beats, 8.0);
    assert_eq!(lane.clips[1].pattern_offset_beats, 4.0);

    assert_eq!(lane.clips[2].timeline_start_beat, 12.0);
    assert_eq!(lane.clips[2].duration_beats, 4.0);
    assert_eq!(lane.clips[2].pattern_offset_beats, 12.0);

    // 3. Slicing at beat outside any clip (e.g. 20.0) does nothing and returns false
    let split_outside = manager.blade_split_take_lane(lane_id, 20.0).unwrap();
    assert!(!split_outside);

    // 4. Slicing on invalid lane returns error
    assert!(manager.blade_split_take_lane(999, 5.0).is_err());
}

#[test]
fn test_take_lane_multi_take_comping_with_4ms_micro_crossfades() {
    let mut manager = TrackCompingManager::new();
    let t1_id = manager.add_take_lane("Acoustic Guitar - Take 1");
    let t2_id = manager.add_take_lane("Acoustic Guitar - Take 2");

    let pat1 = ClipPattern::Midi(MidiPattern::new(8.0));
    let clip1 = ArrangementClip::new(1, "Take 1 Verse", 0.0, 8.0, pat1);
    manager.get_take_lane_mut(t1_id).unwrap().add_clip(clip1);

    let pat2 = ClipPattern::Midi(MidiPattern::new(8.0));
    let clip2 = ArrangementClip::new(2, "Take 2 Verse", 0.0, 8.0, pat2);
    manager.get_take_lane_mut(t2_id).unwrap().add_clip(clip2);

    // Initially main track has Take 1 spanning [0.0, 8.0)
    let pat_main = ClipPattern::Midi(MidiPattern::new(8.0));
    let mut main_clips = vec![ArrangementClip::new(10, "Take 1 Baseline", 0.0, 8.0, pat_main)];

    // Comp best bar [2.0, 5.0) from Take 2 into main track with 4.0 ms micro-crossfade
    let comped_count = manager
        .comp_region_to_main(&mut main_clips, t2_id, 2.0, 5.0, 4.0)
        .expect("Comping Take 2 [2..5) must succeed");

    assert_eq!(comped_count, 1);
    assert_eq!(main_clips.len(), 3, "Main track must now have 3 segments: [0..2], [2..5], [5..8]");

    // Verify main track slice boundaries
    assert_eq!(main_clips[0].timeline_start_beat, 0.0);
    assert_eq!(main_clips[0].duration_beats, 2.0);

    assert_eq!(main_clips[1].timeline_start_beat, 2.0);
    assert_eq!(main_clips[1].duration_beats, 3.0);
    assert_eq!(main_clips[1].name, "Take 2 Verse");

    assert_eq!(main_clips[2].timeline_start_beat, 5.0);
    assert_eq!(main_clips[2].duration_beats, 3.0);

    // Verify micro-crossfades registered at boundaries 2.0 and 5.0
    assert_eq!(manager.crossfades.len(), 2);
    assert_eq!(manager.crossfades[0].beat_position, 2.0);
    assert_eq!(manager.crossfades[0].duration_ms, 4.0);
    assert_eq!(manager.crossfades[1].beat_position, 5.0);
    assert_eq!(manager.crossfades[1].duration_ms, 4.0);

    // Verify boundary validation error when start_beat >= end_beat
    let invalid = manager.comp_region_to_main(&mut main_clips, t2_id, 5.0, 2.0, 4.0);
    assert!(invalid.is_err());
}

#[test]
fn test_take_lane_4ms_equal_power_audio_crossfade_processing() {
    let xfade = MicroCrossfade::new(4.0, 4.0, 100, 101);
    assert_eq!(xfade.duration_ms, 4.0);
    assert_eq!(xfade.beat_position, 4.0);

    // Sample-rate specific frame counts for 4ms duration
    let frames_44k = xfade.frames_at_sample_rate(44100.0);
    let frames_48k = xfade.frames_at_sample_rate(48000.0);
    let frames_96k = xfade.frames_at_sample_rate(96000.0);

    assert_eq!(frames_44k, 176); // 4.0 * 44100 / 1000 = 176.4 -> 176
    assert_eq!(frames_48k, 192); // 4.0 * 48000 / 1000 = 192.0 -> 192
    assert_eq!(frames_96k, 384); // 4.0 * 96000 / 1000 = 384.0 -> 384

    // Check mathematical equal-power law across all frames: sin^2(x) + cos^2(x) == 1.0
    for n in 0..frames_44k {
        let in_gain = MicroCrossfade::fade_in_gain(n, frames_44k);
        let out_gain = MicroCrossfade::fade_out_gain(n, frames_44k);
        let power_sum = in_gain * in_gain + out_gain * out_gain;
        assert!(
            (power_sum - 1.0).abs() < 1e-4,
            "Frame {} power sum {} deviated from 1.0",
            n,
            power_sum
        );
    }

    // Audio buffer DSP crossfade test: crossfade between a constant 0.8 signal and a constant -0.8 signal
    let out_audio = vec![0.8f32; frames_44k];
    let in_audio = vec![-0.8f32; frames_44k];
    let mut blended = vec![0.0f32; frames_44k];

    xfade.apply_crossfade(&out_audio, &in_audio, &mut blended);

    // Frame 0: pure out_audio = 0.8
    assert!((blended[0] - 0.8).abs() < 1e-4);

    // Midpoint: (0.8 * cos(pi/4)) + (-0.8 * sin(pi/4)) = 0.8 * 0.7071 - 0.8 * 0.7071 = 0.0 (anti-phase cancellation)
    let mid = frames_44k / 2;
    assert!(blended[mid].abs() < 1e-3);

    // Final frame: pure in_audio = -0.8
    assert!((blended[frames_44k - 1] - (-0.8)).abs() < 1e-2);
}

#[test]
fn test_take_lane_track_convenience_methods_comping_and_audition() {
    let mut track = Track::new(1, "Lead Vocal", TrackType::Audio);
    assert_eq!(track.comping.take_lanes.len(), 0);

    let t1_id = track.comping.add_take_lane("Mic A - Warm");
    let t2_id = track.comping.add_take_lane("Mic B - Bright");

    let pat_a = ClipPattern::Midi(MidiPattern::new(8.0));
    track.comping.get_take_lane_mut(t1_id).unwrap().add_clip(
        ArrangementClip::new(1, "Take Mic A", 0.0, 8.0, pat_a),
    );

    let pat_b = ClipPattern::Midi(MidiPattern::new(8.0));
    track.comping.get_take_lane_mut(t2_id).unwrap().add_clip(
        ArrangementClip::new(2, "Take Mic B", 0.0, 8.0, pat_b),
    );

    // Initial state: main clips empty
    assert!(track.effective_clips().is_empty());

    // Comp region 0.0..4.0 from Mic A
    let c1 = track.comp_take_region(t1_id, 0.0, 4.0, 4.0).expect("Comp Mic A must succeed");
    assert_eq!(c1, 1);
    assert_eq!(track.clips.len(), 1);
    assert_eq!(track.clips[0].name, "Take Mic A");

    // Comp region 4.0..8.0 from Mic B
    let c2 = track.comp_take_region(t2_id, 4.0, 8.0, 4.0).expect("Comp Mic B must succeed");
    assert_eq!(c2, 1);
    assert_eq!(track.clips.len(), 2);
    assert_eq!(track.clips[1].name, "Take Mic B");

    // Effective clips without audition is the comped main clips (2 clips)
    assert_eq!(track.effective_clips().len(), 2);

    // Engage audition on Take Mic B: effective clips switches to Take Mic B clip (1 clip)
    track.set_audition_take(Some(t2_id));
    assert_eq!(track.effective_clips().len(), 1);
    assert_eq!(track.effective_clips()[0].name, "Take Mic B");

    // Clear audition: switches back to main track clips
    track.set_audition_take(None);
    assert_eq!(track.effective_clips().len(), 2);
}
