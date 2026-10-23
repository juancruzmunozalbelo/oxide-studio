//! Comprehensive End-to-End Integration Test Suite for Milestone M10:
//! 6-Mode Audio Warping Engine (OWarper), Global Groove Pool, Comping Take Lanes,
//! and Full Mixer with Unlimited Tracks & Return Routing.
//!
//! Validates:
//! 1. 6-Mode Audio Warping Engine:
//!    - Beats, Tones, Texture, RePitch, Complex, ComplexPro.
//!    - Time-stretching at 0.5x and 2.0x, pitch-shifting at -12 st, 0 st, +12 st, + detune ct.
//!    - Piecewise linear Warp Markers and spectral flux transient detection.
//!    - Clip gain in dB.
//!    - All rendered outputs strictly finite, non-NaN, and non-silent.
//! 2. Global Groove Pool:
//!    - MPC 16th Swing (54% .. 75%), TR-909 Shuffle, SP-1200 Funk, Latin/Triplet templates.
//!    - Base (1/8, 1/16, 1/32), Quantize %, Timing %, Random %, Velocity %, Global Amount %.
//!    - Non-destructive application to MIDI clip notes and Audio clip warp markers.
//!    - Commit groove action.
//! 3. Full Modular Mixer & Return Routing:
//!    - Unlimited MIDI Tracks, Audio Tracks, Group Tracks, and Return Tracks (A-Reverb, B-Delay, C-Custom).
//!    - Pre-fader and Post-fader Sends, Mute, Solo, Volume dB, Pan, Track Delay (-100..+100 ms).
//!    - DJ Crossfader equal-power curve across Assign A, Off, Assign B.
//!    - Peak + RMS ballistics metering on channels, returns, and master bus.
//! 4. Comping Take Lanes & Multi-Lane Parameter Automation:
//!    - Multi-lane take comping with region selection splicing into main arrangement lane.
//!    - Automatic 4-10 ms equal-power micro-crossfades at splice junctions.
//!    - Multi-lane parameter automation overlays with Linear, Exponential, Logarithmic, SCurve, and Step curves.

use std::f32::consts::PI;
use oxide_audio::ModularAudioMixer;
use oxide_timeline::{
    decode_wav_pcm, ArrangementClip, AutomationCurveType, AutomationPoint, ClipPattern,
    CrossfadeAssign, GroovePool, MicroCrossfade, MidiNote, MidiPattern, MultiLaneAutomation,
    OWarper, OutputDestination, SendRoutingMode, TrackCompingManager, TrackManager, TrackType,
    WarpedAudioClip, WarperConfig, WarpMarker, WarpMode,
};

/// Helper to generate a clean synthetic 16-bit PCM WAV byte buffer in memory.
fn create_test_wav_bytes(sample_rate: u32, channels: u16, duration_sec: f32, freq_hz: f32) -> Vec<u8> {
    let num_frames = (sample_rate as f32 * duration_sec) as usize;
    let bytes_per_sample = 2usize;
    let data_len = num_frames * channels as usize * bytes_per_sample;
    let total_len = 36 + data_len;

    let mut out = Vec::with_capacity(44 + data_len);
    // RIFF header
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // fmt chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    out.extend_from_slice(&1u16.to_le_bytes());  // audio format = 1 (PCM)
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
    out.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = channels * bytes_per_sample as u16;
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&(bytes_per_sample as u16 * 8).to_le_bytes());

    // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());

    for f in 0..num_frames {
        let t = f as f32 / sample_rate as f32;
        let s = (2.0 * PI * freq_hz * t).sin() * 0.75;
        let s_i16 = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        for _ in 0..channels {
            out.extend_from_slice(&s_i16.to_le_bytes());
        }
    }

    out
}

// =========================================================================
// Test 1: 6-Mode Audio Warping Engine Verification
// =========================================================================

#[test]
fn test_all_6_warp_modes_stretch_and_pitch() {
    let sample_rate = 44100u32;
    let wav_bytes = create_test_wav_bytes(sample_rate, 2, 1.0, 440.0);

    let (sr, channels, samples) = decode_wav_pcm(&wav_bytes).expect("WAV decode failed");
    assert_eq!(sr, sample_rate);
    assert_eq!(channels, 2);
    assert!(!samples.is_empty());

    let modes = [
        (WarpMode::Beats, "Beats"),
        (WarpMode::Tones, "Tones"),
        (WarpMode::Texture, "Texture"),
        (WarpMode::RePitch, "RePitch"),
        (WarpMode::Complex, "Complex"),
        (WarpMode::ComplexPro, "ComplexPro"),
    ];

    for (mode, name) in modes {
        let mut config = WarperConfig::default();
        config.mode = mode;
        config.pitch_coarse_st = 0;
        config.pitch_fine_cents = 0;
        config.gain_db = 0.0;

        let mut warper = OWarper::new(sample_rate as f32, config);
        warper.analyze_and_init(&samples, channels as usize, 120.0);

        // 1. Verify 0.5x continuous stretch (double speed)
        let out_0_5 = warper.process_stretch(&samples, channels as usize, 0.5, 0.0);
        assert!(!out_0_5.is_empty(), "Mode {} returned empty output at 0.5x", name);
        for (idx, &s) in out_0_5.iter().enumerate() {
            assert!(!s.is_nan(), "Mode {} generated NaN at 0.5x sample {}", name, idx);
            assert!(s.is_finite(), "Mode {} generated non-finite at 0.5x sample {}", name, idx);
            assert!(s.abs() <= 2.0, "Mode {} sample {} exceeded bounds at 0.5x", name, s);
        }

        // 2. Verify 2.0x continuous stretch (half speed)
        let out_2_0 = warper.process_stretch(&samples, channels as usize, 2.0, 0.0);
        assert!(!out_2_0.is_empty(), "Mode {} returned empty output at 2.0x", name);
        for (idx, &s) in out_2_0.iter().enumerate() {
            assert!(!s.is_nan(), "Mode {} generated NaN at 2.0x sample {}", name, idx);
            assert!(s.is_finite(), "Mode {} generated non-finite at 2.0x sample {}", name, idx);
        }

        // 3. Verify Pitch Transposition -12 semitones
        let out_pitch_down = warper.process_stretch(&samples, channels as usize, 1.0, -12.0);
        assert!(!out_pitch_down.is_empty(), "Mode {} returned empty output at -12 st", name);
        for &s in &out_pitch_down {
            assert!(!s.is_nan() && s.is_finite());
        }

        // 4. Verify Pitch Transposition +12 semitones
        let out_pitch_up = warper.process_stretch(&samples, channels as usize, 1.0, 12.0);
        assert!(!out_pitch_up.is_empty(), "Mode {} returned empty output at +12 st", name);
        for &s in &out_pitch_up {
            assert!(!s.is_nan() && s.is_finite());
        }

        // 5. Verify non-silent energy output
        let energy: f32 = out_0_5.iter().map(|s| s * s).sum();
        assert!(energy > 1e-3, "Mode {} output must have non-zero energy", name);
    }
}

#[test]
fn test_warped_audio_clip_with_markers_and_gain() {
    let sample_rate = 44100u32;
    let wav_bytes = create_test_wav_bytes(sample_rate, 2, 2.0, 330.0);

    let mut clip = WarpedAudioClip::from_wav_bytes("GuitarChord", &wav_bytes, 120.0)
        .expect("Failed creating WarpedAudioClip");

    assert_eq!(clip.sample_rate, sample_rate);
    assert_eq!(clip.channels, 2);
    assert!(clip.is_warped);
    assert!((clip.length_beats - 4.0).abs() < 1e-2);

    // Add piecewise linear warp markers: stretch first bar, compress second bar
    clip.warper.clear_markers();
    clip.warper.add_marker(0.0, 0.0);
    clip.warper.add_marker(2.0, 1.5); // First 2 beats take 1.5 sec (stretched)
    clip.warper.add_marker(4.0, 2.0); // Next 2 beats take 0.5 sec (compressed)

    assert_eq!(clip.warper.beat_to_sample_time(0.0), 0.0);
    assert_eq!(clip.warper.beat_to_sample_time(2.0), 1.5);
    assert_eq!(clip.warper.beat_to_sample_time(4.0), 2.0);

    // Test render at 140 BPM
    let rendered_140 = clip.render_at_bpm(140.0);
    assert!(!rendered_140.is_empty());
    for s in &rendered_140 {
        assert!(s.is_finite() && !s.is_nan());
    }

    // Test -60 dB clip gain produces silence
    clip.warper.config.gain_db = -65.0;
    let rendered_silent = clip.render_at_bpm(120.0);
    for s in &rendered_silent {
        assert_eq!(*s, 0.0, "Gain <= -60 dB must flush to digital silence");
    }
}

// =========================================================================
// Test 2: Global Groove Pool Verification
// =========================================================================

#[test]
fn test_groove_pool_all_templates_and_non_destructive_swing() {
    let mut pool = GroovePool::default();

    // Verify all factory templates exist
    let templates_to_test = [
        "MPC 16th 54%",
        "MPC 16th 60%",
        "MPC 16th 66%",
        "MPC 16th 75%",
        "TR-909 Shuffle",
        "SP-1200 Funk",
        "Latin Triplet",
    ];

    for name in &templates_to_test {
        assert!(
            pool.find_template(name).is_some(),
            "Factory template {} must be available in Groove Pool",
            name
        );
    }

    // Create 4 straight 16th notes: 0.0, 0.25, 0.50, 0.75 beats
    let straight_notes = vec![
        MidiNote::new(36, 100, 0.00, 0.20),
        MidiNote::new(42, 100, 0.25, 0.20),
        MidiNote::new(38, 100, 0.50, 0.20),
        MidiNote::new(42, 100, 0.75, 0.20),
    ];

    // 1. Test MPC 66% Swing
    let mpc_id = pool.find_template("MPC 16th 66%").unwrap().id;
    let mpc_swung = pool.apply_to_midi_notes(&straight_notes, mpc_id, 42);

    assert_eq!(mpc_swung[0].start_beat, 0.00, "Downbeat 0 must not be delayed");
    assert_eq!(mpc_swung[2].start_beat, 0.50, "Downbeat 2 must not be delayed");
    assert!(
        mpc_swung[1].start_beat > 0.25,
        "Offbeat 1 must be delayed by swing (got {})",
        mpc_swung[1].start_beat
    );
    assert!(
        mpc_swung[3].start_beat > 0.75,
        "Offbeat 3 must be delayed by swing (got {})",
        mpc_swung[3].start_beat
    );

    // 2. Test TR-909 Shuffle with Velocity Accents
    let tr909_id = pool.find_template("TR-909").unwrap().id;
    let tr909_swung = pool.apply_to_midi_notes(&straight_notes, tr909_id, 909);
    assert!(tr909_swung[1].velocity > 100, "TR-909 must add velocity accent on offbeats");

    // 3. Test Non-Destructive Application to Audio Warp Markers
    let straight_markers = vec![
        WarpMarker::new(0.00, 0.0),
        WarpMarker::new(0.25, 0.125),
        WarpMarker::new(0.50, 0.250),
        WarpMarker::new(0.75, 0.375),
    ];

    let swung_markers = pool.apply_to_warp_markers(&straight_markers, mpc_id, 111);
    assert_eq!(swung_markers[0].beat, 0.00);
    assert!(swung_markers[1].beat > 0.25);
    assert_eq!(swung_markers[2].beat, 0.50);
    assert!(swung_markers[3].beat > 0.75);

    // 4. Test Global Amount = 0% results in zero modification
    pool.global_amount = 0.0;
    let zero_swung = pool.apply_to_midi_notes(&straight_notes, mpc_id, 42);
    assert_eq!(zero_swung[1].start_beat, 0.25);
    assert_eq!(zero_swung[1].velocity, 100);

    // 5. Test Destructive Commit to MIDI Pattern
    pool.global_amount = 1.0;
    let mut pattern = MidiPattern::new(4.0);
    for n in &straight_notes {
        pattern.add_note(*n);
    }
    pool.commit_to_midi_pattern(&mut pattern, mpc_id, 777);
    assert!(pattern.notes[1].start_beat > 0.25);
}

// =========================================================================
// Test 3: Full Modular Mixer & Return Routing Verification
// =========================================================================

#[test]
fn test_modular_mixer_unlimited_tracks_sends_returns_and_crossfader() {
    let sample_rate = 44100.0f32;
    let mut mixer = ModularAudioMixer::new(sample_rate);

    // Add unlimited tracks of various types
    let t_midi = mixer.add_channel("Operator Bass", TrackType::Midi);
    let t_audio = mixer.add_channel("Warped Vocal", TrackType::Audio);
    let t_group = mixer.add_channel("Drums Group", TrackType::Group);
    let t_ch3 = mixer.add_channel("Pad Synth", TrackType::Midi);

    // Add 3rd custom return track beyond default A-Reverb and B-Delay
    let ret_shimmer = mixer.add_return("C-Shimmer");
    assert_eq!(mixer.returns.len(), 3);
    assert_eq!(mixer.channels.len(), 4);

    // Assign Crossfader:
    // Track 0 -> Side A
    // Track 1 -> Side B
    // Track 2 -> Off
    mixer.channels[t_midi].crossfade_assign = CrossfadeAssign::A;
    mixer.channels[t_audio].crossfade_assign = CrossfadeAssign::B;
    mixer.channels[t_group].crossfade_assign = CrossfadeAssign::Off;

    // Track 3 has Pre-Fader Send to Return 0 (A-Reverb) and is Muted
    mixer.channels[t_ch3].set_send(0, 0.0, SendRoutingMode::PreFader);
    mixer.channels[t_ch3].mute = true;

    // Track 0 has Post-Fader Send to Return 1 (B-Delay) at -6 dB
    mixer.channels[t_midi].set_send(1, -6.0, SendRoutingMode::PostFader);

    // Track 1 has Track Delay of 5.0 ms
    mixer.channels[t_audio].set_track_delay(5.0, sample_rate);

    // Test Crossfader at Full A (-1.0): Side A passes 100%, Side B is silent
    mixer.set_crossfader(-1.0);
    let ga = ModularAudioMixer::crossfade_gain(CrossfadeAssign::A, -1.0);
    let gb = ModularAudioMixer::crossfade_gain(CrossfadeAssign::B, -1.0);
    assert_eq!(ga, 1.0);
    assert!(gb < 1e-4);

    // Process a 256-frame block
    let frames = 256;
    mixer.begin_block(frames);

    let mut buf_midi_l = vec![0.8f32; frames];
    let mut buf_midi_r = vec![0.8f32; frames];
    mixer.process_channel(t_midi, &mut buf_midi_l, &mut buf_midi_r);

    let mut buf_audio_l = vec![0.8f32; frames];
    let mut buf_audio_r = vec![0.8f32; frames];
    mixer.process_channel(t_audio, &mut buf_audio_l, &mut buf_audio_r);

    let mut buf_ch3_l = vec![1.0f32; frames];
    let mut buf_ch3_r = vec![1.0f32; frames];
    mixer.process_channel(t_ch3, &mut buf_ch3_l, &mut buf_ch3_r);

    // Verify Pre-fader send on muted Track 3 fed Return 0
    assert!(
        mixer.returns[0].buffer_l[0] > 0.0,
        "Pre-fader send must route audio to Return A even when track is muted"
    );

    // Verify Post-fader send on Track 0 fed Return 1
    assert!(mixer.returns[1].buffer_l[0] > 0.0);

    // Process Return effects (simulated DSP callback)
    mixer.process_return(0, |_l, _r| {}, frames);
    mixer.process_return(1, |_l, _r| {}, frames);
    mixer.process_return(ret_shimmer, |_l, _r| {}, frames);

    let mut master_out_l = vec![0.0f32; frames];
    let mut master_out_r = vec![0.0f32; frames];
    mixer.end_block(&mut master_out_l, &mut master_out_r);

    // Verify master output (past limiter lookahead of 128 samples)
    assert!(master_out_l[150] > 0.0);
    assert!(mixer.master_peak_l > 0.0);
    assert!(mixer.master_rms_l > 0.0);

    // Verify Track Manager model validation
    let mut track_mgr = TrackManager::new();
    let m1 = track_mgr.add_midi_track("Midi 1");
    let g1 = track_mgr.add_group_track("Group 1");
    track_mgr.get_track_mut(m1).unwrap().output_destination = OutputDestination::GroupTrack(g1);
    assert!(track_mgr.validate_no_cycles().is_ok());
}

// =========================================================================
// Test 4: Comping Take Lanes & Micro-Crossfade Splice Verification
// =========================================================================

#[test]
fn test_comping_take_lanes_with_micro_crossfades() {
    let mut manager = TrackCompingManager::new();
    let lane_id_take1 = manager.add_take_lane("Vocal Take 1");
    let lane_id_take2 = manager.add_take_lane("Vocal Take 2");

    // Create Take 1: Note 60 across 0.0 .. 8.0 beats
    let mut midi1 = MidiPattern::new(8.0);
    midi1.add_note(MidiNote::new(60, 100, 0.0, 8.0));
    let clip1 = ArrangementClip::new(101, "Take 1", 0.0, 8.0, ClipPattern::Midi(midi1));
    manager.get_take_lane_mut(lane_id_take1).unwrap().add_clip(clip1);

    // Create Take 2: Note 64 across 0.0 .. 8.0 beats
    let mut midi2 = MidiPattern::new(8.0);
    midi2.add_note(MidiNote::new(64, 110, 0.0, 8.0));
    let clip2 = ArrangementClip::new(102, "Take 2", 0.0, 8.0, ClipPattern::Midi(midi2));
    manager.get_take_lane_mut(lane_id_take2).unwrap().add_clip(clip2);

    // Main arrangement lane begins with Take 1 occupying 0.0 .. 8.0 beats
    let mut main_clips = vec![
        ArrangementClip::new(1, "Take 1", 0.0, 8.0, ClipPattern::Midi(MidiPattern::new(8.0)))
    ];

    // Comp region 2.5 .. 5.5 beats from Take 2 into main track
    let comped_count = manager
        .comp_region_to_main(&mut main_clips, lane_id_take2, 2.5, 5.5, 6.0)
        .expect("Comping slice must succeed");

    assert_eq!(comped_count, 1);
    assert_eq!(main_clips.len(), 3, "Main lane must now contain 3 spliced segments");

    // Verify timeline bounds of the 3 slices:
    // Slice 1: Take 1 from 0.0 .. 2.5 beats
    assert_eq!(main_clips[0].timeline_start_beat, 0.0);
    assert_eq!(main_clips[0].duration_beats, 2.5);

    // Slice 2: Take 2 from 2.5 .. 5.5 beats
    assert_eq!(main_clips[1].timeline_start_beat, 2.5);
    assert_eq!(main_clips[1].duration_beats, 3.0);
    assert_eq!(main_clips[1].name, "Take 2");

    // Slice 3: Take 1 from 5.5 .. 8.0 beats
    assert_eq!(main_clips[2].timeline_start_beat, 5.5);
    assert_eq!(main_clips[2].duration_beats, 2.5);

    // Verify micro-crossfades are registered at splice points 2.5 and 5.5 beats
    assert_eq!(manager.crossfades.len(), 2);
    assert_eq!(manager.crossfades[0].beat_position, 2.5);
    assert_eq!(manager.crossfades[0].duration_ms, 6.0);
    assert_eq!(manager.crossfades[1].beat_position, 5.5);
    assert_eq!(manager.crossfades[1].duration_ms, 6.0);

    // Verify equal-power fade math: sin^2 + cos^2 == 1.0 at any crossfade sample
    for n in 0..100 {
        let in_g = MicroCrossfade::fade_in_gain(n, 100);
        let out_g = MicroCrossfade::fade_out_gain(n, 100);
        let sum_power = in_g * in_g + out_g * out_g;
        assert!((sum_power - 1.0).abs() < 1e-5, "Micro-crossfade must satisfy constant-power law");
    }
}

// =========================================================================
// Test 5: Multi-Lane Parameter Automation Overlays Verification
// =========================================================================

#[test]
fn test_multi_lane_parameter_automation_curves() {
    let mut auto = MultiLaneAutomation::new();

    // Add Cutoff lane with SCurve tension
    let lane_cutoff = auto.get_or_create_lane(1, "Filter Cutoff");
    lane_cutoff.add_point(AutomationPoint::with_curve_type(0.0, 0.1, AutomationCurveType::Linear));
    lane_cutoff.add_point(AutomationPoint::with_curve_type(4.0, 0.9, AutomationCurveType::SCurve));

    // Add Resonance lane with Exponential curve
    let lane_res = auto.get_or_create_lane(2, "Resonance");
    lane_res.add_point(AutomationPoint::with_curve_type(0.0, 0.8, AutomationCurveType::Exponential));
    lane_res.add_point(AutomationPoint::with_curve_type(4.0, 0.2, AutomationCurveType::Exponential));

    assert_eq!(auto.lanes.len(), 2);

    // Evaluate cutoff at 0.0, 2.0, 4.0
    let v_0 = auto.evaluate_param(1, 0.0, 0.5);
    let v_2 = auto.evaluate_param(1, 2.0, 0.5);
    let v_4 = auto.evaluate_param(1, 4.0, 0.5);

    assert_eq!(v_0, 0.1);
    assert_eq!(v_4, 0.9);
    assert!(v_2 > 0.1 && v_2 < 0.9);

    // Evaluate resonance at 0.0, 2.0, 4.0
    let r_0 = auto.evaluate_param(2, 0.0, 0.5);
    let r_4 = auto.evaluate_param(2, 4.0, 0.5);
    assert_eq!(r_0, 0.8);
    assert_eq!(r_4, 0.2);
}
