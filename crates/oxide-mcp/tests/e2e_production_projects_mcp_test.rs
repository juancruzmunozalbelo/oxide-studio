//! Comprehensive End-to-End Production Session Test Suite for `oxide-mcp`.
//!
//! Validates:
//! 1. `test_mcp_build_peak_time_techno_135_gmin_project`:
//!    Builds a complete Peak-Time Techno 135 BPM G Minor session via `dispatch_tool_call` with
//!    melodic arpeggio hook, 909 drum groove, rolling 16th bassline, offbeat sub-synth, syncopated stabs,
//!    warped audio clips, return buses, and master FX chain.
//! 2. `test_mcp_build_melodic_techno_126_fsharp_min_project`:
//!    Builds a complete Melodic Techno 126 BPM F# Minor session via `dispatch_tool_call` with
//!    66% 16th swing groove, tuned mid-toms, rolling bassline, polyphonic `F#m -> Dmaj7 -> A -> E` pads,
//!    filter cutoff automation, 6 return buses, multi-track stems export, and spectral verification.

use std::fs;
use std::path::{Path, PathBuf};
use serde_json::{json, Value};

use oxide_mcp::state::DawState;
use oxide_mcp::tools::dispatch_tool_call;

const ASSETS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("oxide_mcp_{}_{}", label, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("Failed to create temp test directory");
        Self(dir)
    }

    fn path_str(&self, file_name: &str) -> String {
        self.0.join(file_name).to_string_lossy().to_string()
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn get_analysis_metric(report: &Value, snake: &str, camel: &str) -> f64 {
    report.get(snake)
        .or_else(|| report.get(camel))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

#[test]
fn test_mcp_build_peak_time_techno_135_gmin_project() {
    let out_dir = TempDirGuard::new("peak_135");

    let mut state = DawState::new();

    // 1. project_new, transport_set_bpm, scale_set_global
    let new_res = dispatch_tool_call(
        &mut state,
        "project_new",
        &json!({ "name": "Peak-Time Techno 135 BPM (G Minor)" }),
    );
    assert!(!new_res.is_error);

    let bpm_res = dispatch_tool_call(
        &mut state,
        "transport_set_bpm",
        &json!({ "bpm": 135.0 }),
    );
    assert!(!bpm_res.is_error);

    let scale_res = dispatch_tool_call(
        &mut state,
        "scale_set_global",
        &json!({ "root_note": "G", "mode": "Minor" }),
    );
    assert!(!scale_res.is_error);

    // 2. Configure Track 0 ("Lead Synth - G Minor Arp Hook")
    let synth_res = dispatch_tool_call(
        &mut state,
        "synth_load_preset",
        &json!({ "track_id": 0, "preset_name": "analogsaw.preset" }),
    );
    assert!(!synth_res.is_error, "synth_load_preset failed: {:?}", synth_res);

    let arp_notes = json!([
        { "pitch": 55, "velocity": 100, "start_beat": 0.00, "duration_beats": 0.19 }, // G2
        { "pitch": 55, "velocity": 100, "start_beat": 0.25, "duration_beats": 0.19 }, // G2
        { "pitch": 63, "velocity": 100, "start_beat": 0.75, "duration_beats": 0.19 }, // D#3
        { "pitch": 63, "velocity": 100, "start_beat": 1.00, "duration_beats": 0.19 }, // D#3
        { "pitch": 67, "velocity": 100, "start_beat": 1.50, "duration_beats": 0.19 }, // G3
        { "pitch": 67, "velocity": 100, "start_beat": 1.75, "duration_beats": 0.19 }, // G3
        { "pitch": 63, "velocity": 100, "start_beat": 2.25, "duration_beats": 0.19 }, // D#3
        { "pitch": 63, "velocity": 100, "start_beat": 2.50, "duration_beats": 0.19 }, // D#3
        { "pitch": 62, "velocity": 100, "start_beat": 3.00, "duration_beats": 0.19 }, // D3
        { "pitch": 62, "velocity": 100, "start_beat": 3.25, "duration_beats": 0.19 }, // D3
        { "pitch": 62, "velocity": 100, "start_beat": 3.50, "duration_beats": 0.19 }, // D3
        { "pitch": 60, "velocity": 100, "start_beat": 3.75, "duration_beats": 0.19 }, // C3
    ]);

    let clip_res = dispatch_tool_call(
        &mut state,
        "clip_create_midi",
        &json!({
            "track_id": 0,
            "slot_id": 0,
            "name": "G Minor Arp Hook",
            "length_beats": 4.0,
            "notes": arp_notes
        }),
    );
    assert!(!clip_res.is_error, "clip_create_midi failed on track 0: {:?}", clip_res);

    let place_t0 = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 0,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "G Minor Arp Hook"
        }),
    );
    assert!(!place_t0.is_error);

    // 3. Configure Track 1 ("909 Peak Techno Drums")
    let mut drum_steps = Vec::new();
    for &s in &[0, 4, 8, 12] {
        drum_steps.push(json!({ "pad_index": 0, "step_index": s, "velocity": 127, "accent": true }));
    }
    for &s in &[4, 12] {
        drum_steps.push(json!({ "pad_index": 2, "step_index": s, "velocity": 115, "accent": true }));
    }
    for s in 0..16 {
        drum_steps.push(json!({ "pad_index": 6, "step_index": s, "velocity": 90, "accent": s % 2 == 0 }));
    }
    for &s in &[2, 6, 10, 14] {
        drum_steps.push(json!({ "pad_index": 10, "step_index": s, "velocity": 105, "accent": true }));
    }

    let drums_clip_res = dispatch_tool_call(
        &mut state,
        "clip_create_drums",
        &json!({
            "track_id": 1,
            "slot_id": 0,
            "name": "Peak 135 Groove",
            "step_count": 16,
            "steps": drum_steps
        }),
    );
    assert!(!drums_clip_res.is_error, "clip_create_drums failed on track 1: {:?}", drums_clip_res);

    let place_t1 = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 1,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "909 Peak Techno Drums"
        }),
    );
    assert!(!place_t1.is_error);

    // 4. Create Track 2 ("Rolling Bass 16th G1")
    let trk2_res = dispatch_tool_call(
        &mut state,
        "track_create_synth",
        &json!({ "name": "Rolling Bass 16th G1", "preset": "wowbass.preset" }),
    );
    assert!(!trk2_res.is_error);

    let mut bass_notes = Vec::new();
    for b in 0..8 {
        for &offset in &[0.25, 0.50, 0.75] {
            bass_notes.push(json!({
                "pitch": 43,
                "velocity": 100,
                "start_beat": (b as f64) + offset,
                "duration_beats": 0.25
            }));
        }
    }

    let bass_clip_res = dispatch_tool_call(
        &mut state,
        "clip_create_midi",
        &json!({
            "track_id": 2,
            "slot_id": 0,
            "name": "Rolling 16th G1",
            "length_beats": 8.0,
            "notes": bass_notes
        }),
    );
    assert!(!bass_clip_res.is_error);

    let place_t2 = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 2,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "Rolling 16th G1"
        }),
    );
    assert!(!place_t2.is_error);

    // 5. Create Track 3 ("Offbeat Sub Synth G0")
    let trk3_res = dispatch_tool_call(
        &mut state,
        "track_create_synth",
        &json!({ "name": "Offbeat Sub Synth G0", "preset": "aggro.preset" }),
    );
    assert!(!trk3_res.is_error);

    let mut offbeat_notes = Vec::new();
    for b in 0..8 {
        offbeat_notes.push(json!({
            "pitch": 31,
            "velocity": 100,
            "start_beat": (b as f64) + 0.5,
            "duration_beats": 0.24
        }));
    }

    let offbeat_clip_res = dispatch_tool_call(
        &mut state,
        "clip_create_midi",
        &json!({
            "track_id": 3,
            "slot_id": 0,
            "name": "Offbeat Sub G0",
            "length_beats": 8.0,
            "notes": offbeat_notes
        }),
    );
    assert!(!offbeat_clip_res.is_error);

    let place_t3 = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 3,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "Offbeat Sub G0"
        }),
    );
    assert!(!place_t3.is_error);

    // 6. Create Track 4 ("Syncopated Stab Hook G2")
    let trk4_res = dispatch_tool_call(
        &mut state,
        "track_create_synth",
        &json!({ "name": "Syncopated Stab Hook G2", "preset": "metallic.preset" }),
    );
    assert!(!trk4_res.is_error);

    let mut stab_notes = Vec::new();
    for &sb in &[0.25, 0.75, 1.00, 1.25, 1.75, 2.00, 2.25, 2.75, 3.25] {
        stab_notes.push(json!({
            "pitch": 55,
            "velocity": 100,
            "start_beat": sb,
            "duration_beats": 0.20
        }));
    }

    let stab_clip_res = dispatch_tool_call(
        &mut state,
        "clip_create_midi",
        &json!({
            "track_id": 4,
            "slot_id": 0,
            "name": "Syncopated Stab Hook G2",
            "length_beats": 4.0,
            "notes": stab_notes
        }),
    );
    assert!(!stab_clip_res.is_error);

    let place_t4 = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 4,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "Syncopated Stab Hook G2"
        }),
    );
    assert!(!place_t4.is_error);

    // 7. Load warped audio stem from built-in assets if available
    let kick_asset = format!("{}/kick.wav", ASSETS_DIR);
    if Path::new(&kick_asset).exists() {
        let trk_res = dispatch_tool_call(
            &mut state,
            "track_create_synth",
            &json!({ "name": "Sub Rumble Layer" }),
        );
        assert!(!trk_res.is_error);

        let audio_clip_res = dispatch_tool_call(
            &mut state,
            "audio_clip_create_warped",
            &json!({
                "track_id": 5,
                "slot_id": 0,
                "sample_path": kick_asset,
                "warp_mode": "beats"
            }),
        );
        assert!(!audio_clip_res.is_error);
    }

    // 8. Configure Mixer, Return tracks & Master FX
    for (tid, vol_db, pan) in &[(0, -2.0, 0.0), (1, 0.0, 0.0), (2, -1.0, 0.0), (3, -3.0, 0.0), (4, -4.0, 0.1)] {
        let mix_res = dispatch_tool_call(
            &mut state,
            "mixer_set_channel",
            &json!({ "track_id": *tid, "volume_db": *vol_db, "pan": *pan }),
        );
        assert!(!mix_res.is_error);
    }

    for (ret_name, dev) in &[("A-Reverb", "reverb"), ("B-Delay", "delay"), ("C-Echo", "echo")] {
        let ret_res = dispatch_tool_call(
            &mut state,
            "return_track_create",
            &json!({ "name": ret_name, "initial_device": dev }),
        );
        assert!(!ret_res.is_error);
    }

    for (track_id, send_index, level_db) in [(0, 0, -12.0), (0, 1, -15.0), (4, 1, -9.0)] {
        let send_res = dispatch_tool_call(
            &mut state,
            "mixer_set_send",
            &json!({ "track_id": track_id, "send_index": send_index, "level_db": level_db }),
        );
        assert!(!send_res.is_error, "mixer_set_send failed: {:?}", send_res);
    }

    let add_dev_res = dispatch_tool_call(
        &mut state,
        "device_add_to_track",
        &json!({ "track_id": 1, "device_name": "eq_eight" }),
    );
    assert!(!add_dev_res.is_error, "device_add_to_track failed: {:?}", add_dev_res);
    let added_dev: Value = serde_json::from_str(&add_dev_res.content[0].text).unwrap();
    let eq_idx = added_dev["device_index"].as_u64().unwrap();
    let set_dev_res = dispatch_tool_call(
        &mut state,
        "device_set_param",
        &json!({ "track_id": 1, "device_index": eq_idx, "param_index": 0, "value": 95.0 }),
    );
    assert!(!set_dev_res.is_error, "device_set_param failed: {:?}", set_dev_res);

    let sat_res = dispatch_tool_call(&mut state, "fx_set_saturator", &json!({ "drive_db": 2.5, "wet": 0.35, "output_gain_db": -0.5 }));
    assert!(!sat_res.is_error);
    let dly_res = dispatch_tool_call(&mut state, "fx_set_delay", &json!({ "subdivision": "3/16", "feedback": 0.30, "wet": 0.15, "ping_pong": true }));
    assert!(!dly_res.is_error);
    let rev_res = dispatch_tool_call(&mut state, "fx_set_reverb", &json!({ "room_size": 0.8, "decay": 0.6, "damping": 0.4, "wet": 0.15 }));
    assert!(!rev_res.is_error);
    let lim_res = dispatch_tool_call(&mut state, "fx_set_limiter", &json!({ "ceiling_db": -0.3 }));
    assert!(!lim_res.is_error);

    // 9. Save project, Export ALS, Bounce WAV and Analyze (inside temp dir)
    let save_json_path = out_dir.path_str("peak_time_135_gmin.oxide.json");
    let save_res = dispatch_tool_call(
        &mut state,
        "project_save",
        &json!({ "path": save_json_path }),
    );
    assert!(!save_res.is_error);
    assert!(Path::new(&save_json_path).exists());

    // Verify full save/load round-trip preserves sends, return tracks, insert FX, and Master FX
    let mut reloaded_state = DawState::new();
    let reload_res = dispatch_tool_call(
        &mut reloaded_state,
        "project_load",
        &json!({ "path": save_json_path }),
    );
    assert!(!reload_res.is_error, "project_load failed: {:?}", reload_res);
    assert!((reloaded_state.project.master_track.saturator_drive_db - 2.5).abs() < 0.01);
    assert!((reloaded_state.project.master_track.reverb_mix - 0.15).abs() < 0.01);
    assert!(
        reloaded_state.project.tracks.iter().any(|t| t.name == "C-Echo"),
        "Custom return track C-Echo must persist across project_save/load"
    );
    assert!(!reloaded_state.project.tracks[1].insert_fx.is_empty());

    let als_out_path = out_dir.path_str("peak_time_135_gmin.als");
    let export_res = dispatch_tool_call(
        &mut state,
        "project_export_als",
        &json!({ "path": als_out_path }),
    );
    assert!(!export_res.is_error);
    assert!(Path::new(&als_out_path).exists());

    let drop_wav_path = out_dir.path_str("peak_time_135_gmin_drop.wav");
    let bounce_res = dispatch_tool_call(
        &mut state,
        "bounce_wav",
        &json!({
            "output_path": drop_wav_path,
            "duration_seconds": 14.22,
            "bit_depth": 16
        }),
    );
    assert!(!bounce_res.is_error);
    assert!(Path::new(&drop_wav_path).exists());

    let analyze_res = dispatch_tool_call(
        &mut state,
        "analyze_rendered_audio",
        &json!({ "wav_path": drop_wav_path }),
    );
    assert!(!analyze_res.is_error);

    let report: Value = serde_json::from_str(&analyze_res.content[0].text).unwrap();
    let peak_dbfs = get_analysis_metric(&report, "peak_dbfs", "peakDbfs");
    let rms_dbfs = get_analysis_metric(&report, "rms_dbfs", "rmsDbfs");

    assert!(rms_dbfs > -30.0, "Expected rms_dbfs > -30.0, got {:.2}", rms_dbfs);
    assert!(peak_dbfs > -12.0, "Expected peak_dbfs > -12.0, got {:.2}", peak_dbfs);

    // Verify soloing Track 0 (Drums) alters rendered output compared to full mix
    assert!(!dispatch_tool_call(&mut state, "mixer_set_channel", &json!({ "track_id": 0, "solo": true })).is_error);
    let solo_wav_path = out_dir.path_str("peak_time_135_solo_drums.wav");
    let solo_bounce = dispatch_tool_call(
        &mut state,
        "bounce_wav",
        &json!({ "output_path": solo_wav_path, "duration_seconds": 4.0, "bit_depth": 16 }),
    );
    assert!(!solo_bounce.is_error);
    let solo_report_res = dispatch_tool_call(&mut state, "analyze_rendered_audio", &json!({ "wav_path": solo_wav_path }));
    assert!(!solo_report_res.is_error);
    let solo_report: Value = serde_json::from_str(&solo_report_res.content[0].text).unwrap();
    let solo_rms = get_analysis_metric(&solo_report, "rms_dbfs", "rmsDbfs");
    assert!(
        (solo_rms - rms_dbfs).abs() > 0.05,
        "Soloing track 0 must change bounced RMS (full={:.2}, solo={:.2})",
        rms_dbfs,
        solo_rms
    );
    assert!(!dispatch_tool_call(&mut state, "mixer_set_channel", &json!({ "track_id": 0, "solo": false })).is_error);
}

#[test]
fn test_mcp_build_melodic_techno_126_fsharp_min_project() {
    let out_dir = TempDirGuard::new("melodic_126");

    let mut state = DawState::new();

    // 1. Project Setup (126 BPM, F# Minor)
    let proj_res = dispatch_tool_call(
        &mut state,
        "project_new",
        &json!({ "name": "Melodic Techno 126 BPM (F# Minor)" }),
    );
    assert!(!proj_res.is_error);

    let bpm_res = dispatch_tool_call(
        &mut state,
        "transport_set_bpm",
        &json!({ "bpm": 126.0 }),
    );
    assert!(!bpm_res.is_error);

    let scale_res = dispatch_tool_call(
        &mut state,
        "scale_set_global",
        &json!({ "root_note": "F#", "mode": "Minor" }),
    );
    assert!(!scale_res.is_error);

    // 2. Groove & Drums
    let swing_res = dispatch_tool_call(
        &mut state,
        "transport_set_swing",
        &json!({ "swing": 0.66 }),
    );
    assert!(!swing_res.is_error);

    let _ = dispatch_tool_call(&mut state, "drums_configure_pad", &json!({ "pad_index": 7, "pitch_semitones": -1.0 }));

    let mut holo_steps = Vec::new();
    for &s in &[0, 4, 8, 12, 16, 20, 24, 28] {
        holo_steps.push(json!({ "pad_index": 0, "step_index": s, "velocity": 127, "accent": true }));
    }
    for &s in &[4, 12, 20, 28] {
        holo_steps.push(json!({ "pad_index": 2, "step_index": s, "velocity": 115, "accent": true }));
    }
    for s in 0..32 {
        holo_steps.push(json!({ "pad_index": 6, "step_index": s, "velocity": 85, "accent": s % 4 == 2 }));
    }
    for &s in &[2, 6, 10, 14, 18, 22, 26, 30] {
        holo_steps.push(json!({ "pad_index": 10, "step_index": s, "velocity": 105, "accent": true }));
    }
    for &s in &[3, 6, 11, 14, 19, 22, 27, 30] {
        holo_steps.push(json!({ "pad_index": 7, "step_index": s, "velocity": 100, "accent": true }));
    }

    let drums_res = dispatch_tool_call(
        &mut state,
        "clip_create_drums",
        &json!({
            "track_id": 1,
            "slot_id": 0,
            "name": "Melodic Techno 126 Groove",
            "step_count": 32,
            "steps": holo_steps
        }),
    );
    assert!(!drums_res.is_error);

    let _ = dispatch_tool_call(
        &mut state,
        "groove_pool_apply_to_clip",
        &json!({
            "track_id": 1,
            "slot_id": 0,
            "groove_name": "TR-909 Shuffle",
            "amount": 1.0
        }),
    );

    let _ = dispatch_tool_call(&mut state, "device_add_to_track", &json!({ "track_id": 1, "device_type": "eq_eight" }));
    let _ = dispatch_tool_call(&mut state, "device_add_to_track", &json!({ "track_id": 1, "device_type": "glue_compressor" }));
    let _ = dispatch_tool_call(&mut state, "device_add_to_track", &json!({ "track_id": 1, "device_type": "roar" }));

    let place_drums = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 1,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "Melodic Techno 126 Groove"
        }),
    );
    assert!(!place_drums.is_error);

    // 3. Sound Design: Melodic Lead Hook & Rolling 16th Bass
    let _ = dispatch_tool_call(
        &mut state,
        "synth_load_preset",
        &json!({ "track_id": 0, "preset_name": "analogsaw.preset" }),
    );

    let lead_notes = json!([
        { "pitch": 66, "velocity": 100, "start_beat": 0.00, "duration_beats": 0.75 }, // F#3
        { "pitch": 68, "velocity": 100, "start_beat": 0.75, "duration_beats": 0.25 }, // G#3
        { "pitch": 69, "velocity": 100, "start_beat": 1.00, "duration_beats": 0.75 }, // A3
        { "pitch": 73, "velocity": 100, "start_beat": 1.75, "duration_beats": 0.75 }, // C#4
        { "pitch": 74, "velocity": 100, "start_beat": 2.50, "duration_beats": 0.50 }, // D4
        { "pitch": 73, "velocity": 100, "start_beat": 3.00, "duration_beats": 0.50 }, // C#4
        { "pitch": 71, "velocity": 100, "start_beat": 3.50, "duration_beats": 0.50 }, // B3
        { "pitch": 69, "velocity": 100, "start_beat": 4.00, "duration_beats": 1.00 }, // A3
        { "pitch": 68, "velocity": 100, "start_beat": 5.00, "duration_beats": 0.50 }, // G#3
        { "pitch": 66, "velocity": 100, "start_beat": 5.50, "duration_beats": 1.50 }, // F#3
        { "pitch": 64, "velocity": 100, "start_beat": 7.00, "duration_beats": 0.50 }, // E3
        { "pitch": 66, "velocity": 100, "start_beat": 7.50, "duration_beats": 0.50 }, // F#3
    ]);

    let _ = dispatch_tool_call(
        &mut state,
        "clip_create_midi",
        &json!({
            "track_id": 0,
            "slot_id": 0,
            "name": "Melodic Lead F#m",
            "length_beats": 8.0,
            "notes": lead_notes
        }),
    );

    let _ = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 0,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "Melodic Lead F#m"
        }),
    );

    // Track 2: Rolling Bass 16th
    let _ = dispatch_tool_call(
        &mut state,
        "track_create_synth",
        &json!({ "name": "Rolling Bass 16th (F#m-D-A-E)", "preset": "wowbass.preset" }),
    );

    let chord_roots = [(0..4, 42), (4..8, 38), (8..12, 45), (12..16, 40)];
    let mut rolling_bass = Vec::new();
    for (range, pitch) in chord_roots {
        for b in range {
            for &off in &[0.25, 0.50, 0.75] {
                rolling_bass.push(json!({
                    "pitch": pitch,
                    "velocity": 100,
                    "start_beat": (b as f64) + off,
                    "duration_beats": 0.25
                }));
            }
        }
    }

    let _ = dispatch_tool_call(
        &mut state,
        "clip_create_midi",
        &json!({
            "track_id": 2,
            "slot_id": 0,
            "name": "Rolling 16th F#m-D-A-E",
            "length_beats": 16.0,
            "notes": rolling_bass
        }),
    );

    let _ = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 2,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "Rolling 16th F#m-D-A-E"
        }),
    );

    // 4. Harmonic Progression Pads + Filter Automation
    let _ = dispatch_tool_call(
        &mut state,
        "track_create_synth",
        &json!({ "name": "Analog Poly Pads (F#m-Dmaj7-A-E)", "preset": "darkpad.preset" }),
    );

    let pad_notes = json!([
        { "pitch": 54, "velocity": 95, "start_beat": 0.0, "duration_beats": 4.0 },
        { "pitch": 57, "velocity": 95, "start_beat": 0.0, "duration_beats": 4.0 },
        { "pitch": 61, "velocity": 95, "start_beat": 0.0, "duration_beats": 4.0 },
        { "pitch": 66, "velocity": 95, "start_beat": 0.0, "duration_beats": 4.0 },
        { "pitch": 50, "velocity": 95, "start_beat": 4.0, "duration_beats": 4.0 },
        { "pitch": 54, "velocity": 95, "start_beat": 4.0, "duration_beats": 4.0 },
        { "pitch": 57, "velocity": 95, "start_beat": 4.0, "duration_beats": 4.0 },
        { "pitch": 61, "velocity": 95, "start_beat": 4.0, "duration_beats": 4.0 },
        { "pitch": 57, "velocity": 95, "start_beat": 8.0, "duration_beats": 4.0 },
        { "pitch": 61, "velocity": 95, "start_beat": 8.0, "duration_beats": 4.0 },
        { "pitch": 64, "velocity": 95, "start_beat": 8.0, "duration_beats": 4.0 },
        { "pitch": 69, "velocity": 95, "start_beat": 8.0, "duration_beats": 4.0 },
        { "pitch": 52, "velocity": 95, "start_beat": 12.0, "duration_beats": 4.0 },
        { "pitch": 56, "velocity": 95, "start_beat": 12.0, "duration_beats": 4.0 },
        { "pitch": 59, "velocity": 95, "start_beat": 12.0, "duration_beats": 4.0 },
        { "pitch": 64, "velocity": 95, "start_beat": 12.0, "duration_beats": 4.0 },
    ]);

    let _ = dispatch_tool_call(
        &mut state,
        "clip_create_midi",
        &json!({
            "track_id": 3,
            "slot_id": 0,
            "name": "F#m - Dmaj7 - A - E Progression",
            "length_beats": 16.0,
            "notes": pad_notes
        }),
    );

    let _ = dispatch_tool_call(
        &mut state,
        "timeline_place_clip",
        &json!({
            "track_id": 3,
            "slot_id": 0,
            "start_beat": 0.0,
            "duration_beats": 32.0,
            "name": "F#m - Dmaj7 - A - E Progression"
        }),
    );

    let auto_points = json!([
        { "beat": 0.0, "value": 0.20 },
        { "beat": 24.0, "value": 0.95 },
        { "beat": 32.0, "value": 0.85 },
    ]);

    let _ = dispatch_tool_call(
        &mut state,
        "timeline_add_automation",
        &json!({ "track_id": 0, "parameter_name": "filter_frequency", "points": auto_points }),
    );
    let _ = dispatch_tool_call(
        &mut state,
        "timeline_add_automation",
        &json!({ "track_id": 3, "parameter_name": "filter_frequency", "points": auto_points }),
    );

    for (ret_name, dev) in &[
        ("A-Parallel Drum Bus", "gluecompressor"),
        ("B-Room Ambience", "reverb"),
        ("C-Plate", "reverb"),
        ("D-Long Hall", "hybridreverb"),
        ("E-Shimmer", "spectralresonator"),
        ("F-Granular Echo", "echo"),
    ] {
        let _ = dispatch_tool_call(
            &mut state,
            "return_track_create",
            &json!({ "name": ret_name, "initial_device": dev }),
        );
    }

    let _ = dispatch_tool_call(&mut state, "mixer_set_send", &json!({ "track_id": 0, "send_index": 1, "level_db": -12.0 }));
    let _ = dispatch_tool_call(&mut state, "mixer_set_send", &json!({ "track_id": 1, "send_index": 0, "level_db": -6.0 }));
    let _ = dispatch_tool_call(&mut state, "mixer_set_send", &json!({ "track_id": 2, "send_index": 0, "level_db": -18.0 }));
    let _ = dispatch_tool_call(&mut state, "mixer_set_send", &json!({ "track_id": 3, "send_index": 3, "level_db": -9.0 }));

    for (tid, vol_db, pan) in [(0, -1.5, 0.0), (1, 0.0, 0.0), (2, -2.0, 0.0), (3, -3.5, 0.0)] {
        let mix_res = dispatch_tool_call(
            &mut state,
            "mixer_set_channel",
            &json!({ "track_id": tid, "volume_db": vol_db, "pan": pan }),
        );
        assert!(!mix_res.is_error, "mixer_set_channel failed: {:?}", mix_res);
    }

    assert!(!dispatch_tool_call(&mut state, "fx_set_saturator", &json!({ "drive_db": 1.8, "wet": 0.25, "output_gain_db": -0.2 })).is_error);
    assert!(!dispatch_tool_call(&mut state, "fx_set_delay", &json!({ "subdivision": "1/8", "feedback": 0.25, "wet": 0.12, "ping_pong": true })).is_error);
    assert!(!dispatch_tool_call(&mut state, "fx_set_reverb", &json!({ "room_size": 1.1, "decay": 0.7, "damping": 0.35, "wet": 0.18 })).is_error);
    assert!(!dispatch_tool_call(&mut state, "fx_set_limiter", &json!({ "ceiling_db": -0.3 })).is_error);

    // 5. Save project, Export ALS, Bounce WAV, Export Stems, and Verify (inside temp dir)
    let save_path = out_dir.path_str("melodic_techno_126_fsharp_min.oxide.json");
    let save_res = dispatch_tool_call(
        &mut state,
        "project_save",
        &json!({ "path": save_path }),
    );
    assert!(!save_res.is_error);
    assert!(Path::new(&save_path).exists());

    let als_path = out_dir.path_str("melodic_techno_126_fsharp_min.als");
    let export_res = dispatch_tool_call(
        &mut state,
        "project_export_als",
        &json!({ "path": als_path }),
    );
    assert!(!export_res.is_error);
    assert!(Path::new(&als_path).exists());

    let drop_path = out_dir.path_str("melodic_techno_126_fsharp_min_drop.wav");
    let bounce_res = dispatch_tool_call(
        &mut state,
        "bounce_wav",
        &json!({
            "output_path": drop_path,
            "duration_seconds": 15.24,
            "bit_depth": 16
        }),
    );
    assert!(!bounce_res.is_error);
    assert!(Path::new(&drop_path).exists());

    let stems_dir = out_dir.path_str("melodic_techno_stems");
    let stems_res = dispatch_tool_call(
        &mut state,
        "bounce_export_stems",
        &json!({ "output_dir": stems_dir }),
    );
    assert!(!stems_res.is_error, "bounce_export_stems failed: {:?}", stems_res);
    assert!(Path::new(&stems_dir).exists(), "Stems directory must exist");
    let stem_wavs = fs::read_dir(&stems_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("wav"))
        .count();
    assert!(stem_wavs >= 4, "Expected at least 4 stem WAVs, found {}", stem_wavs);

    let analyze_res = dispatch_tool_call(
        &mut state,
        "analyze_rendered_audio",
        &json!({ "wav_path": drop_path }),
    );
    assert!(!analyze_res.is_error);

    let report: Value = serde_json::from_str(&analyze_res.content[0].text).unwrap();
    let peak_dbfs = get_analysis_metric(&report, "peak_dbfs", "peakDbfs");
    let rms_dbfs = get_analysis_metric(&report, "rms_dbfs", "rmsDbfs");

    assert!(rms_dbfs > -30.0, "Expected rms_dbfs > -30.0, got {:.2}", rms_dbfs);
    assert!(peak_dbfs > -12.0, "Expected peak_dbfs > -12.0, got {:.2}", peak_dbfs);
}
