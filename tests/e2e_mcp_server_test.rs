//! End-to-End Test Suite for Model Context Protocol (MCP) Server (`crates/ableton-mcp`).
//!
//! Verifies:
//! - JSON-RPC 2.0 stdio initialization handshake (protocolVersion "2024-11-05")
//! - `tools/list` enumerates all DAW subsystem tools and agentic generators
//! - `tools/call` for granular control:
//!   - Project persistence: `project_new`, `project_save`, `project_load`
//!   - Transport & Telemetry: `transport_set_bpm`, `transport_set_swing`, `transport_play`, `transport_stop`, `transport_get_telemetry`
//!   - Tracks, Synth & 909 Drums: `track_create_synth`, `track_create_drums`, `track_delete`, `synth_list_presets`, `synth_load_preset`, `synth_set_parameter`, `synth_set_macro`, `drums_configure_pad`, `drums_trigger_pad`
//!   - Session & Timeline: `clip_create_midi`, `clip_create_drums`, `clip_create_lilypond`, `clip_launch`, `clip_stop`, `scene_launch`, `timeline_place_clip`, `timeline_add_automation`
//!   - Mixer, FX, Plugins & Bounce: `mixer_set_channel`, `fx_set_saturator`, `fx_set_delay`, `fx_set_reverb`, `fx_set_limiter`, `plugin_scan`, `bounce_wav`
//!   - Agentic Music Generators: `generate_chord_progression_clip`, `generate_bassline_clip`, `generate_arpeggio_clip`, `generate_euclidean_drum_clip`, `generate_drum_groove_clip`, `build_song_arrangement`
//!   - "Agent Ears" Perceptual & 4-Band Spectral Audio Analysis: `analyze_rendered_audio`, `preview_synth_patch`
//! - `resources/list` and `resources/read` for `project://current`, `telemetry://current`, `presets://catalog`, `mappings://catalog`

use std::fs;
use std::path::Path;
use serde_json::{json, Value};

use ableton_mcp::McpServer;

fn send_request(server: &mut McpServer, req: Value) -> Value {
    let line = req.to_string();
    let resp_str = server.handle_line(&line).expect("Expected JSON-RPC response");
    serde_json::from_str(&resp_str).expect("Valid JSON response")
}

#[test]
fn test_mcp_initialize_and_ping() {
    let mut server = McpServer::new();

    // 1. initialize handshake
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "e2e-test-runner", "version": "1.0.0" }
        }
    });
    let resp = send_request(&mut server, init_req);
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);

    let result = &resp["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["serverInfo"]["name"], "ableton-rust-mcp");
    assert_eq!(result["serverInfo"]["version"], "0.1.0");
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());

    // 2. notifications/initialized (notification returns None)
    let notif = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    let opt_resp = server.handle_line(&notif.to_string());
    assert!(opt_resp.is_none(), "Notifications must not produce a response");

    // 3. ping
    let ping_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "ping",
        "params": {}
    });
    let ping_resp = send_request(&mut server, ping_req);
    assert_eq!(ping_resp["id"], 2);
    assert!(ping_resp["result"].is_object());
}

#[test]
fn test_mcp_tools_list_full_catalogue() {
    let mut server = McpServer::new();
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "tools/list",
        "params": {}
    });
    let resp = send_request(&mut server, list_req);
    let tools = resp["result"]["tools"].as_array().expect("tools array");

    let required_tools = [
        "project_new",
        "project_load",
        "project_save",
        "transport_play",
        "transport_stop",
        "transport_set_bpm",
        "transport_set_swing",
        "transport_get_telemetry",
        "track_create_synth",
        "track_create_drums",
        "track_delete",
        "synth_list_presets",
        "synth_load_preset",
        "synth_set_parameter",
        "synth_set_macro",
        "drums_configure_pad",
        "drums_trigger_pad",
        "clip_create_midi",
        "clip_create_drums",
        "clip_create_lilypond",
        "clip_launch",
        "clip_stop",
        "scene_launch",
        "timeline_place_clip",
        "timeline_add_automation",
        "mixer_set_channel",
        "fx_set_saturator",
        "fx_set_delay",
        "fx_set_reverb",
        "fx_set_limiter",
        "plugin_scan",
        "bounce_wav",
        "render_bounce_wav",
        "generate_chord_progression_clip",
        "generate_bassline_clip",
        "generate_arpeggio_clip",
        "generate_euclidean_drum_clip",
        "generate_drum_groove_clip",
        "build_song_arrangement",
        "analyze_rendered_audio",
        "preview_synth_patch",
    ];

    let registered_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    for req in &required_tools {
        assert!(
            registered_names.contains(req),
            "MCP tools catalogue is missing required tool: '{}'",
            req
        );
    }
}

#[test]
fn test_mcp_project_lifecycle_and_transport() {
    let mut server = McpServer::new();

    // 1. project_new
    let new_req = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "project_new",
            "arguments": { "name": "E2E Techno Session" }
        }
    });
    let resp = send_request(&mut server, new_req);
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(false));

    // 2. transport_set_bpm
    let bpm_req = json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "transport_set_bpm",
            "arguments": { "bpm": 128.5 }
        }
    });
    let resp = send_request(&mut server, bpm_req);
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(false));
    assert_eq!(server.state.bpm, 128.5);

    // 3. transport_set_swing
    let swing_req = json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "transport_set_swing",
            "arguments": { "swing": 0.55 }
        }
    });
    send_request(&mut server, swing_req);
    assert_eq!(server.state.swing, 0.55);

    // 4. transport_play & transport_stop
    let play_req = json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": { "name": "transport_play", "arguments": {} }
    });
    send_request(&mut server, play_req);
    assert!(server.state.is_playing);

    let stop_req = json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": { "name": "transport_stop", "arguments": {} }
    });
    send_request(&mut server, stop_req);
    assert!(!server.state.is_playing);

    // 5. transport_get_telemetry
    let tel_req = json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": { "name": "transport_get_telemetry", "arguments": {} }
    });
    let tel_resp = send_request(&mut server, tel_req);
    let tel_text = tel_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(tel_text.contains("master_peak_l"));
    assert!(tel_text.contains("cpu_load"));

    // 6. project_save to disk
    let test_proj_path = format!("/tmp/ableton_mcp_e2e_test_proj_{}.json", std::process::id());
    let save_req = json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": {
            "name": "project_save",
            "arguments": { "path": &test_proj_path }
        }
    });
    send_request(&mut server, save_req);
    assert!(Path::new(&test_proj_path).exists(), "Saved project must exist on disk");

    // 7. project_load from disk
    let load_req = json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": {
            "name": "project_load",
            "arguments": { "path": &test_proj_path }
        }
    });
    let load_resp = send_request(&mut server, load_req);
    let load_text = load_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(load_text.contains("E2E Techno Session"));

    let _ = fs::remove_file(&test_proj_path);
}

#[test]
fn test_mcp_synth_and_drum_rack_control() {
    let mut server = McpServer::new();

    // 1. synth_list_presets
    let list_presets_req = json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": { "name": "synth_list_presets", "arguments": {} }
    });
    let resp = send_request(&mut server, list_presets_req);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("38"), "Must report 38 presets");
    assert!(text.contains("wowbass.preset"));

    // 2. synth_load_preset
    let load_preset_req = json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": {
            "name": "synth_load_preset",
            "arguments": { "preset_name": "wowbass.preset" }
        }
    });
    let resp = send_request(&mut server, load_preset_req);
    assert!(!resp["result"]["isError"].as_bool().unwrap_or(false));

    // 3. synth_set_parameter
    let set_param_req = json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": {
            "name": "synth_set_parameter",
            "arguments": {
                "parameter": "filter_frequency",
                "value": 1450.0
            }
        }
    });
    send_request(&mut server, set_param_req);
    assert_eq!(server.state.synth.params.get_value("filter_frequency"), 1450.0);

    // 4. synth_set_macro
    let macro_req = json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": {
            "name": "synth_set_macro",
            "arguments": {
                "x": 0.85,
                "y": 0.45,
                "mapping_name": "wowbass.mapping"
            }
        }
    });
    send_request(&mut server, macro_req);
    assert!((server.state.synth.macro_x - 0.85).abs() < 1e-4);
    assert!((server.state.synth.macro_y - 0.45).abs() < 1e-4);

    // 5. drums_configure_pad
    let drum_cfg_req = json!({
        "jsonrpc": "2.0",
        "id": 24,
        "method": "tools/call",
        "params": {
            "name": "drums_configure_pad",
            "arguments": {
                "pad_index": 0,
                "pitch_semitones": 3.0,
                "decay_seconds": 0.35,
                "volume": 1.2
            }
        }
    });
    send_request(&mut server, drum_cfg_req);
    assert_eq!(server.state.drum_rack.pads[0].pitch_semitones, 3.0);
    assert_eq!(server.state.drum_rack.pads[0].decay_seconds, 0.35);

    // 6. drums_trigger_pad
    let trigger_req = json!({
        "jsonrpc": "2.0",
        "id": 25,
        "method": "tools/call",
        "params": {
            "name": "drums_trigger_pad",
            "arguments": {
                "pad_index": 0,
                "velocity": 120
            }
        }
    });
    let trig_resp = send_request(&mut server, trigger_req);
    assert!(!trig_resp["result"]["isError"].as_bool().unwrap_or(false));
}

#[test]
fn test_mcp_session_matrix_and_lilypond() {
    let mut server = McpServer::new();

    // 1. clip_create_midi
    let midi_req = json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": {
            "name": "clip_create_midi",
            "arguments": {
                "track_id": 0,
                "slot_id": 0,
                "name": "Acid Baseline",
                "length_beats": 4.0,
                "notes": [
                    { "pitch": 36, "velocity": 110, "start_beat": 0.0, "duration_beats": 0.5 },
                    { "pitch": 38, "velocity": 95, "start_beat": 1.0, "duration_beats": 0.5 },
                    { "pitch": 43, "velocity": 120, "start_beat": 2.0, "duration_beats": 0.75 }
                ]
            }
        }
    });
    send_request(&mut server, midi_req);
    assert!(server.state.matrix.tracks[0].slots[0].clip.is_some());

    // 2. clip_create_lilypond
    let lily_req = json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": {
            "name": "clip_create_lilypond",
            "arguments": {
                "track_id": 0,
                "slot_id": 1,
                "name": "Lily Lead",
                "notation": "\\tempo 4 = 126 c'4 e'4 g'4 b'4"
            }
        }
    });
    let lily_resp = send_request(&mut server, lily_req);
    assert!(!lily_resp["result"]["isError"].as_bool().unwrap_or(false));
    assert!(server.state.matrix.tracks[0].slots[1].clip.is_some());

    // 3. clip_launch & clip_stop
    let launch_req = json!({
        "jsonrpc": "2.0",
        "id": 32,
        "method": "tools/call",
        "params": {
            "name": "clip_launch",
            "arguments": {
                "track_id": 0,
                "slot_id": 0,
                "quantize_mode": "immediate"
            }
        }
    });
    let launch_resp = send_request(&mut server, launch_req);
    assert!(!launch_resp["result"]["isError"].as_bool().unwrap_or(false));

    let stop_req = json!({
        "jsonrpc": "2.0",
        "id": 33,
        "method": "tools/call",
        "params": {
            "name": "clip_stop",
            "arguments": { "track_id": 0 }
        }
    });
    send_request(&mut server, stop_req);
}

#[test]
fn test_mcp_mixer_fx_and_bounce_wav() {
    let mut server = McpServer::new();

    // 1. mixer_set_channel
    let mixer_req = json!({
        "jsonrpc": "2.0",
        "id": 40,
        "method": "tools/call",
        "params": {
            "name": "mixer_set_channel",
            "arguments": {
                "track_id": 0,
                "volume_db": -4.5,
                "pan": -0.3,
                "mute": false,
                "solo": false
            }
        }
    });
    send_request(&mut server, mixer_req);
    assert_eq!(server.state.mixer.tracks[0].volume_db, -4.5);
    assert_eq!(server.state.mixer.tracks[0].pan, -0.3);

    // 2. fx_set_saturator & delay & reverb & limiter
    let fx_req = json!({
        "jsonrpc": "2.0",
        "id": 41,
        "method": "tools/call",
        "params": {
            "name": "fx_set_saturator",
            "arguments": { "drive_db": 12.0, "wet": 0.8 }
        }
    });
    send_request(&mut server, fx_req);
    assert!(server.state.saturator.drive_linear > 3.0);

    let delay_req = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "fx_set_delay",
            "arguments": { "subdivision": "3/16", "feedback": 0.65, "wet": 0.4 }
        }
    });
    send_request(&mut server, delay_req);
    assert_eq!(server.state.delay.subdivision.beats(), 0.375);

    // 3. bounce_wav / render_bounce_wav
    let out_wav = format!("/tmp/ableton_mcp_e2e_bounce_{}.wav", std::process::id());
    let bounce_req = json!({
        "jsonrpc": "2.0",
        "id": 43,
        "method": "tools/call",
        "params": {
            "name": "bounce_wav",
            "arguments": {
                "duration_seconds": 4.2,
                "output_path": &out_wav,
                "bit_depth": 16
            }
        }
    });
    let bounce_resp = send_request(&mut server, bounce_req);
    let bounce_text = bounce_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(bounce_text.contains("rendered"));
    assert!(Path::new(&out_wav).exists(), "Rendered WAV must exist");

    // 4. analyze_rendered_audio on the bounced file
    let analyze_req = json!({
        "jsonrpc": "2.0",
        "id": 44,
        "method": "tools/call",
        "params": {
            "name": "analyze_rendered_audio",
            "arguments": { "wav_path": &out_wav }
        }
    });
    let analyze_resp = send_request(&mut server, analyze_req);
    let metrics_text = analyze_resp["result"]["content"][0]["text"].as_str().unwrap();
    let metrics_json: serde_json::Value = serde_json::from_str(metrics_text).unwrap();

    // Verify key bounds and non-trivial spectral metrics
    let centroid = metrics_json["spectralCentroidHz"].as_f64().unwrap();
    let peak_dbfs = metrics_json["peakDbfs"].as_f64().unwrap();
    let is_silent = metrics_json["isSilent"].as_bool().unwrap();
    assert!(centroid > 50.0, "Centroid must be positive and non-zero: got {}", centroid);
    assert!(peak_dbfs <= 0.0, "Peak dBFS must be <= 0.0: got {}", peak_dbfs);
    assert!(!is_silent, "Rendered arrangement must not be silent");

    let bands = &metrics_json["spectralBands"];
    let sub_pct = bands["subBassEnergyPct"].as_f64().unwrap();
    let low_mid_pct = bands["lowMidEnergyPct"].as_f64().unwrap();
    let high_mid_pct = bands["highMidEnergyPct"].as_f64().unwrap();
    let high_pct = bands["highEnergyPct"].as_f64().unwrap();
    let total_band_pct = sub_pct + low_mid_pct + high_mid_pct + high_pct;
    assert!((total_band_pct - 100.0).abs() < 1.0, "Total band energy must sum to ~100%, got {}", total_band_pct);

    let _ = fs::remove_file(&out_wav);
}

#[test]
fn test_mcp_agentic_generators_and_preview() {
    let mut server = McpServer::new();

    // 1. generate_chord_progression_clip
    let chord_req = json!({
        "jsonrpc": "2.0",
        "id": 50,
        "method": "tools/call",
        "params": {
            "name": "generate_chord_progression_clip",
            "arguments": {
                "track_id": 0,
                "slot_id": 0,
                "key": "F",
                "scale": "Minor",
                "progression": "i-VI-III-VII",
                "voicing": "drop-2"
            }
        }
    });
    let chord_resp = send_request(&mut server, chord_req);
    assert!(!chord_resp["result"]["isError"].as_bool().unwrap_or(false));
    assert!(server.state.matrix.tracks[0].slots[0].clip.is_some());

    // 2. generate_bassline_clip
    let bass_req = json!({
        "jsonrpc": "2.0",
        "id": 51,
        "method": "tools/call",
        "params": {
            "name": "generate_bassline_clip",
            "arguments": {
                "track_id": 0,
                "slot_id": 1,
                "key": "F",
                "scale": "Minor",
                "progression": "i-VI-III-VII",
                "rhythm_pattern": "driving_8ths"
            }
        }
    });
    send_request(&mut server, bass_req);
    assert!(server.state.matrix.tracks[0].slots[1].clip.is_some());

    // 3. generate_arpeggio_clip
    let arp_req = json!({
        "jsonrpc": "2.0",
        "id": 52,
        "method": "tools/call",
        "params": {
            "name": "generate_arpeggio_clip",
            "arguments": {
                "track_id": 0,
                "slot_id": 2,
                "key": "A",
                "scale": "Minor",
                "progression": "i-VI",
                "rate": "1/16",
                "direction": "up",
                "octaves": 2
            }
        }
    });
    send_request(&mut server, arp_req);
    assert!(server.state.matrix.tracks[0].slots[2].clip.is_some());

    // 4. generate_euclidean_drum_clip (Bjorklund k=5, n=16)
    let euc_req = json!({
        "jsonrpc": "2.0",
        "id": 53,
        "method": "tools/call",
        "params": {
            "name": "generate_euclidean_drum_clip",
            "arguments": {
                "track_id": 1,
                "slot_id": 0,
                "hits": 5,
                "steps": 16,
                "pad_index": 6
            }
        }
    });
    send_request(&mut server, euc_req);
    assert!(server.state.matrix.tracks[1].slots[0].clip.is_some());

    // 5. generate_drum_groove_clip
    let groove_req = json!({
        "jsonrpc": "2.0",
        "id": 54,
        "method": "tools/call",
        "params": {
            "name": "generate_drum_groove_clip",
            "arguments": {
                "track_id": 1,
                "slot_id": 1,
                "style": "house_909",
                "complexity": 0.6
            }
        }
    });
    send_request(&mut server, groove_req);
    assert!(server.state.matrix.tracks[1].slots[1].clip.is_some());

    // 6. build_song_arrangement
    let arr_req = json!({
        "jsonrpc": "2.0",
        "id": 55,
        "method": "tools/call",
        "params": {
            "name": "build_song_arrangement",
            "arguments": {
                "genre": "techno",
                "bpm": 128.0,
                "total_bars": 32
            }
        }
    });
    let arr_resp = send_request(&mut server, arr_req);
    assert!(!arr_resp["result"]["isError"].as_bool().unwrap_or(false));
    assert_eq!(server.state.timeline.tracks.len(), 3);
    assert_eq!(server.state.timeline.tracks[0].clips.len(), 4);

    // 7. preview_synth_patch
    let preview_wav = format!("/tmp/ableton_mcp_preview_test_{}.wav", std::process::id());
    let prev_req = json!({
        "jsonrpc": "2.0",
        "id": 56,
        "method": "tools/call",
        "params": {
            "name": "preview_synth_patch",
            "arguments": {
                "preset_name": "wowbass.preset",
                "note": 48,
                "duration_seconds": 1.2,
                "output_wav_path": &preview_wav
            }
        }
    });
    let prev_resp = send_request(&mut server, prev_req);
    let prev_text = prev_resp["result"]["content"][0]["text"].as_str().unwrap();
    let prev_json: serde_json::Value = serde_json::from_str(prev_text).unwrap();
    assert_eq!(prev_json["status"], "preview_ready");
    assert!(Path::new(&preview_wav).exists());
    let centroid = prev_json.get("spectral_centroid_hz").or_else(|| prev_json.get("spectralCentroidHz")).and_then(|v| v.as_f64()).unwrap();
    assert!(centroid > 40.0);
    assert!(prev_json["peak_dbfs"].as_f64().unwrap() <= 0.0);
    let _ = fs::remove_file(&preview_wav);
}

#[test]
fn test_mcp_resources_suite() {
    let mut server = McpServer::new();

    // 1. resources/list
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 60,
        "method": "resources/list",
        "params": {}
    });
    let resp = send_request(&mut server, list_req);
    let resources = resp["result"]["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 4);

    // 2. Read each resource URI
    for uri in ["project://current", "telemetry://current", "presets://catalog", "mappings://catalog"] {
        let read_req = json!({
            "jsonrpc": "2.0",
            "id": 61,
            "method": "resources/read",
            "params": { "uri": uri }
        });
        let read_resp = send_request(&mut server, read_req);
        assert!(!read_resp["result"]["isError"].as_bool().unwrap_or(false));
        let content = read_resp["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(!content.is_empty(), "Resource '{}' must return non-empty content", uri);
    }
}

#[test]
fn test_mcp_project_save_load_roundtrip_preserves_swing_and_mixer() {
    let mut server = McpServer::new();

    // 1. Initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "roundtrip-test", "version": "1.0" }
        }
    });
    send_request(&mut server, init_req);

    // 2. New project
    let new_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "project_new",
            "arguments": { "name": "SwingMixerProject" }
        }
    });
    send_request(&mut server, new_req);

    // 3. Set custom swing and mixer settings
    let swing_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "transport_set_swing",
            "arguments": { "swing": 0.42 }
        }
    });
    send_request(&mut server, swing_req);

    let mixer_req = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "mixer_set_channel",
            "arguments": {
                "track_id": 0,
                "volume_db": -12.0,
                "pan": -0.5,
                "mute": true,
                "solo": false
            }
        }
    });
    send_request(&mut server, mixer_req);

    // 4. Save project to disk
    let save_path = format!("/tmp/mcp_roundtrip_swing_mixer_{}.json", std::process::id());
    let save_req = json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "project_save",
            "arguments": { "path": &save_path }
        }
    });
    send_request(&mut server, save_req);

    // 5. Reset session with project_new ("Blank")
    let blank_req = json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "project_new",
            "arguments": { "name": "Blank" }
        }
    });
    send_request(&mut server, blank_req);
    assert_eq!(server.state.swing, 0.0);
    assert_eq!(server.state.mixer.tracks[0].volume_db, 0.0);

    // 6. Load saved project
    let load_req = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "project_load",
            "arguments": { "path": &save_path }
        }
    });
    let load_resp = send_request(&mut server, load_req);
    assert!(!load_resp["result"]["isError"].as_bool().unwrap_or(false));

    // Verify state was restored
    assert_eq!(server.state.swing, 0.42);
    assert_eq!(server.state.mixer.tracks[0].volume_db, -12.0);
    assert_eq!(server.state.mixer.tracks[0].pan, -0.5);
    assert!(server.state.mixer.tracks[0].mute);

    // 7. Resave project
    let resave_req = json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": {
            "name": "project_save",
            "arguments": { "path": &save_path }
        }
    });
    send_request(&mut server, resave_req);

    // 8. Verify the saved file on disk retains swing and mixer parameters
    let content = fs::read_to_string(&save_path).unwrap();
    let saved_json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(saved_json["transport"]["swing_16th_ratio"].as_f64().unwrap(), 0.42);
    assert_eq!(saved_json["tracks"][0]["mixer"]["volume_db"].as_f64().unwrap(), -12.0);
    assert_eq!(saved_json["tracks"][0]["mixer"]["pan"].as_f64().unwrap(), -0.5);
    assert_eq!(saved_json["tracks"][0]["mixer"]["mute"].as_bool().unwrap(), true);

    // 9. Verify resources/read project://current returns correct state
    let read_res_req = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "resources/read",
        "params": { "uri": "project://current" }
    });
    let read_res_resp = send_request(&mut server, read_res_req);
    let res_text = read_res_resp["result"]["contents"][0]["text"].as_str().unwrap();
    let current_json: serde_json::Value = serde_json::from_str(res_text).unwrap();
    assert_eq!(current_json["transport"]["swing_16th_ratio"].as_f64().unwrap(), 0.42);
    assert_eq!(current_json["tracks"][0]["mixer"]["volume_db"].as_f64().unwrap(), -12.0);

    let _ = fs::remove_file(&save_path);
}

