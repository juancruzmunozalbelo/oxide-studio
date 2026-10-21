//! End-to-End Test Suite for Milestone M12: Dual-Target 76-Tool MCP Server and Ableton Live 12 LOM Bridge.
//!
//! Verifies:
//! - Full 76-tool registration across all 15 Groups with valid MCP schemas
//! - Group 2 Devices & Chains: `device_add_to_track`, `device_get_chain`
//! - Group 3 Synths, Drums & Simpler: `synth_set_parameter`, `drums_configure_pad`, `simpler_slice_to_drum_rack`
//! - Group 4 Browser & Similarity: `browser_search`, `browser_find_similar_sounds`
//! - Group 5 Session Matrix: `clip_launch`, `clip_stop`, `scene_launch`
//! - Group 6 Live 12 MIDI Transforms & Generators: `midi_transform_clip`, `midi_generate_clip`
//! - Group 7 Audio Clips & Warping: `audio_clip_create_warped`, `audio_clip_set_warp_mode`, `audio_clip_add_warp_marker`
//! - Group 8 Groove Pool & Comping: `groove_pool_list`, `groove_pool_apply_to_clip`, `take_lane_create`, `take_lane_comp_region`
//! - Group 9 Mixer, Returns & Global Scale: `return_track_create`, `mixer_set_send`, `scale_set_global`
//! - Group 11 Master FX: `fx_set_limiter`, `fx_set_saturator`
//! - Group 15 Live 12 LOM Dual-Target TCP Bridge:
//!   - Offline fallback (structured fallback to ableton-rust engine state when port 9877 is unbound)
//!   - Online roundtrip (mock TCP server on 127.0.0.1:9877 answering live JSON-RPC 2.0 requests)

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};
use ableton_mcp::McpServer;

fn send_request(server: &mut McpServer, req: Value) -> Value {
    let line = req.to_string();
    let resp_str = server.handle_line(&line).expect("Expected JSON-RPC response");
    serde_json::from_str(&resp_str).expect("Valid JSON response")
}

fn call_tool(server: &mut McpServer, id: u64, name: &str, arguments: Value) -> Value {
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments,
        }
    });
    send_request(server, req)
}

#[test]
fn test_mcp_all_76_tools_registration() {
    let mut server = McpServer::new();

    // Initialize MCP handshake
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "e2e-m12-runner", "version": "1.0.0" }
        }
    });
    let init_resp = send_request(&mut server, init_req);
    assert_eq!(init_resp["jsonrpc"], "2.0");
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "ableton-rust-mcp");

    // List all tools
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    let list_resp = send_request(&mut server, list_req);
    let tools = list_resp["result"]["tools"].as_array().expect("tools array");

    // Must be exactly 76 tools
    assert_eq!(tools.len(), 76, "Expected exactly 76 MCP tools, found {}", tools.len());

    // Validate that every tool has a name, description, and valid inputSchema
    for t in tools {
        let name = t["name"].as_str().expect("tool name string");
        assert!(!name.is_empty(), "Tool name must not be empty");
        let desc = t["description"].as_str().expect("tool description string");
        assert!(!desc.is_empty(), "Tool description must not be empty for {}", name);
        assert_eq!(t["inputSchema"]["type"], "object", "Tool {} schema must be object", name);
    }

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    // Verify key tools across all 15 Groups
    let expected_sample = [
        // Group 1: Project & Transport (8 tools)
        "project_new", "project_load", "project_save", "transport_play", "transport_stop",
        "transport_set_bpm", "transport_set_swing", "transport_get_telemetry",
        // Group 2: Devices & Chains (8 tools)
        "track_create_synth", "track_create_drums", "track_delete",
        "device_add_to_track", "device_remove_from_track", "device_reorder", "device_set_param", "device_get_chain",
        // Group 3: Synths, Drums & Simpler (7 tools)
        "synth_list_presets", "synth_load_preset", "synth_set_parameter", "synth_set_macro",
        "drums_configure_pad", "drums_trigger_pad", "simpler_slice_to_drum_rack",
        // Group 4: Browser & Similarity (2 tools)
        "browser_search", "browser_find_similar_sounds",
        // Group 5: Clips & Matrix (6 tools)
        "clip_create_midi", "clip_create_drums", "clip_create_lilypond",
        "clip_launch", "clip_stop", "scene_launch",
        // Group 6: Live 12 MIDI Transforms & Generators (2 tools)
        "midi_transform_clip", "midi_generate_clip",
        // Group 7: Audio Clips & Warping (3 tools)
        "audio_clip_create_warped", "audio_clip_set_warp_mode", "audio_clip_add_warp_marker",
        // Group 8: Groove Pool & Comping (4 tools)
        "groove_pool_list", "groove_pool_apply_to_clip", "take_lane_create", "take_lane_comp_region",
        // Group 9: Mixer, Returns & Global Scale (4 tools)
        "return_track_create", "mixer_set_channel", "mixer_set_send", "scale_set_global",
        // Group 10: Timeline & Automation (2 tools)
        "timeline_place_clip", "timeline_add_automation",
        // Group 11: Master FX (4 tools)
        "fx_set_saturator", "fx_set_delay", "fx_set_reverb", "fx_set_limiter",
        // Group 12: Plugin Scanner (1 tool)
        "plugin_scan",
        // Group 13: Bounce & Analysis (4 tools)
        "bounce_wav", "render_bounce_wav", "analyze_rendered_audio", "preview_synth_patch",
        // Group 14: Agentic Generators (6 tools)
        "generate_chord_progression_clip", "generate_bassline_clip", "generate_arpeggio_clip",
        "generate_euclidean_drum_clip", "generate_drum_groove_clip", "build_song_arrangement",
        // Group 15: Real Live 12 LOM Bridge (15 tools)
        "live12_status", "live12_get_song_info", "live12_create_midi_track", "live12_create_audio_track",
        "live12_create_return_track", "live12_load_browser_item", "live12_create_clip",
        "live12_add_notes", "live12_fire_clip", "live12_fire_scene", "live12_set_device_param",
        "live12_get_track_devices", "live12_set_tempo", "live12_set_scale", "live12_exec_lom_query"
    ];

    for expected in expected_sample {
        assert!(names.contains(&expected), "Missing required tool: {}", expected);
    }
}

#[test]
fn test_mcp_devices_and_chains() {
    let mut server = McpServer::new();

    // Create a new project
    let res = call_tool(&mut server, 10, "project_new", json!({ "name": "DeviceChainProject" }));
    assert_eq!(res["result"]["content"][0]["type"], "text");

    // Add Wavetable to track 0
    let res1 = call_tool(&mut server, 11, "device_add_to_track", json!({
        "track_id": 0,
        "device_name": "Wavetable"
    }));
    assert!(res1["result"]["content"][0]["text"].as_str().unwrap().contains("device_added"));

    // Add Echo to track 0
    let res2 = call_tool(&mut server, 12, "device_add_to_track", json!({
        "track_id": 0,
        "device_name": "Echo"
    }));
    assert!(res2["result"]["content"][0]["text"].as_str().unwrap().contains("device_added"));

    // Add AutoFilter to track 0
    let res3 = call_tool(&mut server, 13, "device_add_to_track", json!({
        "track_id": 0,
        "device_name": "AutoFilter"
    }));
    assert!(res3["result"]["content"][0]["text"].as_str().unwrap().contains("device_added"));

    // Get device chain
    let res4 = call_tool(&mut server, 14, "device_get_chain", json!({ "track_id": 0 }));
    let text = res4["result"]["content"][0]["text"].as_str().unwrap();
    let data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(data["device_count"], 3);
    let dev_names: Vec<&str> = data["devices"].as_array().unwrap()
        .iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert_eq!(dev_names, vec!["Wavetable", "Echo", "Auto Filter"]);
}

#[test]
fn test_mcp_group3_synths_drums_simpler() {
    let mut server = McpServer::new();

    // Set synth parameter (parameter 0 = Cutoff)
    let res1 = call_tool(&mut server, 20, "synth_set_parameter", json!({
        "parameter": 0,
        "value": 1200.0
    }));
    let data1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data1["parameter_id"], 0);

    // Configure drum pad (pad 0)
    let res2 = call_tool(&mut server, 21, "drums_configure_pad", json!({
        "pad_index": 0,
        "pitch_semitones": 0.0,
        "decay_seconds": 0.5,
        "volume": 0.9,
        "pan": 0.0
    }));
    let data2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data2["pad_index"], 0);

    // Slice Simpler to drum rack
    let res3 = call_tool(&mut server, 22, "simpler_slice_to_drum_rack", json!({
        "track_id": 0
    }));
    let data3: Value = serde_json::from_str(res3["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data3["status"], "sliced_to_drum_rack");
}

#[test]
fn test_mcp_browser_and_similarity() {
    let mut server = McpServer::new();

    // Search browser for "808"
    let res1 = call_tool(&mut server, 30, "browser_search", json!({
        "query": "808"
    }));
    let data1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(data1["count"].as_u64().unwrap() >= 1);

    // Find similar sounds to drum_808_sub_kick
    let res2 = call_tool(&mut server, 31, "browser_find_similar_sounds", json!({
        "target_id": "drum_808_sub_kick",
        "top_k": 3
    }));
    let data2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data2["target_id"], "drum_808_sub_kick");
    let matches = data2["matches"].as_array().unwrap();
    assert!(!matches.is_empty());
    let score = matches[0]["cosine_similarity"].as_f64().unwrap();
    assert!(score > 0.0 && score <= 1.0);
}

#[test]
fn test_mcp_session_matrix() {
    let mut server = McpServer::new();

    // Launch clip (track 0, slot 1)
    let res1 = call_tool(&mut server, 40, "clip_launch", json!({
        "track_id": 0,
        "slot_id": 1,
        "quantize_mode": "1_bar"
    }));
    let data1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data1["status"], "launched");
    assert_eq!(data1["track_id"], 0);
    assert_eq!(data1["slot_id"], 1);

    // Launch scene 0
    let res2 = call_tool(&mut server, 41, "scene_launch", json!({
        "scene_index": 0
    }));
    let data2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data2["status"], "scene_launched");
    assert_eq!(data2["scene_index"], 0);

    // Stop clip (track 0)
    let res3 = call_tool(&mut server, 42, "clip_stop", json!({
        "track_id": 0
    }));
    let data3: Value = serde_json::from_str(res3["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(data3["status"], "stopped");
}

#[test]
fn test_mcp_live12_midi_generators_and_transforms() {
    let mut server = McpServer::new();

    // 1. Generate clip with Seed
    let res1 = call_tool(&mut server, 50, "midi_generate_clip", json!({
        "track_id": 0,
        "slot_id": 0,
        "generator": "seed",
        "params": { "density": 0.6, "seed": 1337 }
    }));
    let d1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d1["status"], "generated");
    assert_eq!(d1["generator"], "seed");
    assert!(d1["notes_count"].as_u64().unwrap() > 0);

    // 2. Generate clip with Shape
    let res2 = call_tool(&mut server, 51, "midi_generate_clip", json!({
        "track_id": 0,
        "slot_id": 1,
        "generator": "shape",
        "params": { "shape": "arch" }
    }));
    let d2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d2["status"], "generated");
    assert_eq!(d2["generator"], "shape");

    // 3. Generate clip with Stacks
    let res3 = call_tool(&mut server, 52, "midi_generate_clip", json!({
        "track_id": 0,
        "slot_id": 2,
        "generator": "stacks",
        "params": { "chord": "maj7" }
    }));
    let d3: Value = serde_json::from_str(res3["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d3["status"], "generated");
    assert_eq!(d3["generator"], "stacks");

    // 4. Generate clip with Rhythm
    let res4 = call_tool(&mut server, 53, "midi_generate_clip", json!({
        "track_id": 0,
        "slot_id": 3,
        "generator": "rhythm",
        "params": { "style": "trap" }
    }));
    let d4: Value = serde_json::from_str(res4["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d4["status"], "generated");
    assert_eq!(d4["generator"], "rhythm");

    // 5. Generate clip with Euclidean
    let res5 = call_tool(&mut server, 54, "midi_generate_clip", json!({
        "track_id": 0,
        "slot_id": 4,
        "generator": "euclidean",
        "params": { "hits": 5, "steps": 16 }
    }));
    let d5: Value = serde_json::from_str(res5["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d5["status"], "generated");
    assert_eq!(d5["generator"], "euclidean");

    // 6. Transforms on slot 0: quantize, arpeggiate, connect, ornament, strum, span, recombine, time_warp
    for transform in ["quantize", "arpeggiate", "ornament", "strum", "span", "recombine", "time_warp"] {
        let res_tf = call_tool(&mut server, 60, "midi_transform_clip", json!({
            "track_id": 0,
            "slot_id": 0,
            "transform": transform
        }));
        let d_tf: Value = serde_json::from_str(res_tf["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(d_tf["status"], "transformed");
        assert_eq!(d_tf["transform"], transform);
    }
}

#[test]
fn test_mcp_audio_clips_and_warping() {
    let mut server = McpServer::new();

    // Create warped audio clip
    let res1 = call_tool(&mut server, 70, "audio_clip_create_warped", json!({
        "track_id": 0,
        "slot_id": 0,
        "sample_path": "/tmp/test_drum_loop.wav",
        "warp_mode": "complex_pro"
    }));
    let d1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d1["status"], "created");
    assert_eq!(d1["warp_mode"], "complex_pro");

    // Set warp mode to Beats
    let res2 = call_tool(&mut server, 71, "audio_clip_set_warp_mode", json!({
        "track_id": 0,
        "slot_id": 0,
        "warp_mode": "beats"
    }));
    let d2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d2["status"], "updated");
    assert_eq!(d2["warp_mode"], "beats");

    // Add warp marker
    let res3 = call_tool(&mut server, 72, "audio_clip_add_warp_marker", json!({
        "track_id": 0,
        "slot_id": 0,
        "sample_time": 0.9375,
        "beat_time": 2.0
    }));
    let d3: Value = serde_json::from_str(res3["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d3["status"], "marker_added");
    assert_eq!(d3["beat_time"], 2.0);
}

#[test]
fn test_mcp_groove_pool_and_comping() {
    let mut server = McpServer::new();

    // List groove pool templates
    let res1 = call_tool(&mut server, 80, "groove_pool_list", json!({}));
    let d1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(d1["template_count"].as_u64().unwrap() >= 6);

    // Apply groove to clip
    let res2 = call_tool(&mut server, 81, "groove_pool_apply_to_clip", json!({
        "track_id": 0,
        "slot_id": 0,
        "groove_name": "MPC 16 Swing-61",
        "amount": 0.8
    }));
    let d2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d2["status"], "groove_applied");

    // Create take lane
    let res3 = call_tool(&mut server, 82, "take_lane_create", json!({
        "track_id": 0,
        "name": "Take 2 Lead Vocal"
    }));
    let d3: Value = serde_json::from_str(res3["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d3["status"], "take_lane_created");

    // Comp region
    let res4 = call_tool(&mut server, 83, "take_lane_comp_region", json!({
        "track_id": 0,
        "take_lane_index": 0,
        "start_beat": 0.0,
        "end_beat": 4.0
    }));
    let d4: Value = serde_json::from_str(res4["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d4["status"], "region_comped");
}

#[test]
fn test_mcp_mixer_returns_and_global_scale() {
    let mut server = McpServer::new();

    // Create return track
    let res1 = call_tool(&mut server, 90, "return_track_create", json!({
        "name": "Space Reverb Return",
        "initial_device": "Reverb"
    }));
    let d1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d1["status"], "return_track_created");
    assert_eq!(d1["name"], "Space Reverb Return");

    // Set send level
    let res2 = call_tool(&mut server, 91, "mixer_set_send", json!({
        "track_id": 0,
        "send_index": 0,
        "level_db": -4.5
    }));
    let d2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d2["status"], "send_updated");

    // Set global scale to F# Dorian
    let res3 = call_tool(&mut server, 92, "scale_set_global", json!({
        "root_note": "F#",
        "scale_name": "dorian"
    }));
    let d3: Value = serde_json::from_str(res3["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d3["status"], "scale_updated");
    assert_eq!(d3["root_note"], "F#");
    assert_eq!(d3["scale_name"], "Dorian");
}

#[test]
fn test_mcp_master_fx() {
    let mut server = McpServer::new();

    // Master Limiter
    let res1 = call_tool(&mut server, 100, "fx_set_limiter", json!({
        "ceiling_db": -0.3
    }));
    let d1: Value = serde_json::from_str(res1["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(d1["ceiling_linear"].as_f64().is_some());
    assert!(d1["ceiling_dbfs"].as_f64().is_some());

    // Master Saturator
    let res2 = call_tool(&mut server, 101, "fx_set_saturator", json!({
        "drive_db": 6.0,
        "wet": 0.8
    }));
    let d2: Value = serde_json::from_str(res2["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(d2["drive_linear"].as_f64().is_some());
    assert!((d2["wet"].as_f64().unwrap() - 0.8).abs() < 1e-4);
}

#[test]
fn test_live12_lom_offline_fallback() {
    let mut server = McpServer::new();
    // Use dedicated unbound port so this test never collides with parallel online mock server
    server.state.lom_client.port = 19876;

    // When no Live 12 Remote Script is running on 19876, live12_* tools must return
    // structured fallback envelopes without throwing or hanging.
    let res_status = call_tool(&mut server, 110, "live12_status", json!({}));
    let d_status: Value = serde_json::from_str(res_status["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_status["connected"], false);
    assert_eq!(d_status["status"], "offline");
    assert_eq!(d_status["target"], "real_ableton_live_12");

    let res_info = call_tool(&mut server, 111, "live12_get_song_info", json!({}));
    let d_info: Value = serde_json::from_str(res_info["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_info["connected"], false);
    assert_eq!(d_info["fallback_executed"], true);
    assert_eq!(d_info["result"]["tempo"], 120.0);

    let res_tempo = call_tool(&mut server, 112, "live12_set_tempo", json!({ "bpm": 138.0 }));
    let d_tempo: Value = serde_json::from_str(res_tempo["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_tempo["connected"], false);
    assert_eq!(d_tempo["fallback_executed"], true);
    assert_eq!(d_tempo["result"]["tempo"], 138.0);

    let res_track = call_tool(&mut server, 113, "live12_create_midi_track", json!({ "index": 0 }));
    let d_track: Value = serde_json::from_str(res_track["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_track["connected"], false);
    assert_eq!(d_track["fallback_executed"], true);

    let res_query = call_tool(&mut server, 114, "live12_exec_lom_query", json!({ "query": "live_set.tracks[0].name" }));
    let d_query: Value = serde_json::from_str(res_query["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_query["connected"], false);
    assert_eq!(d_query["fallback_executed"], true);
}

#[test]
fn test_live12_lom_online_tcp_roundtrip() {
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    // Bind mock TCP server on 127.0.0.1:9877 to simulate active AbletonMCP remote script
    let listener = match TcpListener::bind("127.0.0.1:9877") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Skipping online TCP roundtrip test: 127.0.0.1:9877 unavailable ({})", e);
            return;
        }
    };
    listener.set_nonblocking(true).unwrap();

    let server_handle = thread::spawn(move || {
        while r_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_ok() {
                        if let Ok(req) = serde_json::from_str::<Value>(&line) {
                            let method = req["method"].as_str().unwrap_or("");
                            let id = req["id"].as_u64().unwrap_or(1);
                            let result = match method {
                                "ping" => json!({ "status": "ok", "version": "12.0.1" }),
                                "get_song_info" => json!({ "tempo": 128.0, "is_playing": false, "track_count": 4 }),
                                "set_tempo" => {
                                    let bpm = req["params"]["bpm"].as_f64()
                                        .or_else(|| req["params"]["tempo"].as_f64())
                                        .unwrap_or(120.0);
                                    json!({ "bpm": bpm, "updated": true })
                                },
                                "create_midi_track" => json!({ "index": 0, "status": "created" }),
                                "exec_lom_query" => json!({ "query": req["params"]["query"], "value": "Live 12 Master" }),
                                _ => json!({ "status": "ok" }),
                            };
                            let resp = json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": result,
                            });
                            let mut resp_str = resp.to_string();
                            resp_str.push('\n');
                            let _ = stream.write_all(resp_str.as_bytes());
                            let _ = stream.flush();
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    // Allow mock server to start
    thread::sleep(Duration::from_millis(50));

    let mut server = McpServer::new();

    // 1. live12_status
    let res_status = call_tool(&mut server, 201, "live12_status", json!({}));
    let d_status: Value = serde_json::from_str(res_status["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_status["connected"], true);
    assert_eq!(d_status["status"], "online");
    assert_eq!(d_status["target"], "real_ableton_live_12");

    // 2. live12_get_song_info
    let res_info = call_tool(&mut server, 202, "live12_get_song_info", json!({}));
    let d_info: Value = serde_json::from_str(res_info["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_info["connected"], true);
    assert_eq!(d_info["result"]["tempo"], 128.0);
    assert_eq!(d_info["result"]["track_count"], 4);

    // 3. live12_set_tempo
    let res_tempo = call_tool(&mut server, 203, "live12_set_tempo", json!({ "bpm": 145.0 }));
    let d_tempo: Value = serde_json::from_str(res_tempo["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_tempo["connected"], true);
    assert_eq!(d_tempo["result"]["bpm"], 145.0);

    // 4. live12_create_midi_track
    let res_trk = call_tool(&mut server, 204, "live12_create_midi_track", json!({ "index": 0 }));
    let d_trk: Value = serde_json::from_str(res_trk["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_trk["connected"], true);
    assert_eq!(d_trk["result"]["status"], "created");

    // 5. live12_exec_lom_query
    let res_query = call_tool(&mut server, 205, "live12_exec_lom_query", json!({ "query": "live_set.master_track.name" }));
    let d_query: Value = serde_json::from_str(res_query["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(d_query["connected"], true);
    assert_eq!(d_query["result"]["value"], "Live 12 Master");

    // Cleanup mock server
    running.store(false, Ordering::Relaxed);
    let _ = server_handle.join();
}
