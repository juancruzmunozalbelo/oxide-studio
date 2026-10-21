//! Adversarial Empirical Stress Test Suite for Milestone M12:
//! Dual-Target 76-Tool MCP Server and Ableton Live 12 LOM Bridge.
//!
//! Stress-tests:
//! 1. Tool dispatch with empty parameters across all 76 tools.
//! 2. Inverted and hostile JSON parameter types across all 76 tools.
//! 3. Unknown and malformed tool names.
//! 4. MCP Server protocol parser fault injection (malformed JSON, broken RPC).
//! 5. `TcpLomClient` offline unbound port behavior & connect timeout.
//! 6. `TcpLomClient` dropped connection / immediate RST without response.
//! 7. `TcpLomClient` malformed / non-JSON response from server.
//! 8. `TcpLomClient` JSON-RPC error response forwarding.
//! 9. `TcpLomClient` high-latency / slow server read timeout.
//! 10. `TcpLomClient` concurrent request stress (online & offline).
//! 11. Full matrix verification of all 15 `live12_*` tools in offline fallback mode.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use ableton_mcp::bridge::TcpLomClient;
use ableton_mcp::protocol::*;
use ableton_mcp::state::DawState;
use ableton_mcp::tools::{call_tool, get_tool_definitions};
use ableton_mcp::McpServer;

// ============================================================================
// 1. TOOL DISPATCH STRESS: EMPTY PARAMETERS ACROSS ALL 76 TOOLS
// ============================================================================

#[test]
fn test_adv_all_76_tools_empty_parameters() {
    let tools = get_tool_definitions();
    assert_eq!(tools.len(), 76, "Tool catalog must contain exactly 76 tools");

    let mut state = DawState::new();
    // Use unbound port for LOM client during empty param tests
    state.lom_client.port = 28901;

    let empty_args = json!({});

    for tool in &tools {
        let tool_name = tool.name.clone();
        let unwind_res = panic::catch_unwind(AssertUnwindSafe(|| {
            call_tool(&mut state, &tool_name, &empty_args)
        }));

        assert!(
            unwind_res.is_ok(),
            "Tool '{}' PANICKED when called with empty parameters {{}}",
            tool.name
        );

        let call_res = unwind_res.unwrap();
        // The call must produce valid content and not crash
        assert!(
            !call_res.content.is_empty(),
            "Tool '{}' returned empty content on empty parameters",
            tool.name
        );
    }
}

// ============================================================================
// 2. TOOL DISPATCH STRESS: HOSTILE & INVERTED PARAMETER TYPES
// ============================================================================

#[test]
fn test_adv_all_76_tools_hostile_and_inverted_parameters() {
    let tools = get_tool_definitions();
    let mut state = DawState::new();
    state.lom_client.port = 28902;

    let hostile_payloads = vec![
        json!(null),
        json!(12345),
        json!("unexpected_string"),
        json!([1, 2, 3]),
        json!(false),
        json!({
            "path": 999999,
            "bpm": "super_fast",
            "track_id": -50,
            "slot_id": 99999999999999i64,
            "parameter": -1,
            "value": "NaN",
            "preset_name": false,
            "pad_index": 999,
            "notation": 0,
            "query": true,
            "target_id": 12345,
            "top_k": -10,
            "device_name": 42,
            "device_type": null,
            "position": -5,
            "from_index": -1,
            "to_index": -2,
            "ceiling_dbfs": "inf",
            "drive_db": [1, 2],
            "wet": 999999.0,
            "scale_name": 123,
            "root_note": "C_flat"
        }),
    ];

    for tool in &tools {
        for payload in &hostile_payloads {
            let tool_name = tool.name.clone();
            let p_clone = payload.clone();
            let unwind_res = panic::catch_unwind(AssertUnwindSafe(|| {
                call_tool(&mut state, &tool_name, &p_clone)
            }));

            assert!(
                unwind_res.is_ok(),
                "Tool '{}' PANICKED on hostile payload {:?}",
                tool.name,
                payload
            );
        }
    }
}

// ============================================================================
// 3. UNKNOWN AND MALFORMED TOOL NAMES
// ============================================================================

#[test]
fn test_adv_unknown_and_malformed_tool_names() {
    let mut state = DawState::new();

    let invalid_names = [
        "",
        "   ",
        "unknown_tool_xyz",
        "drop table users;",
        "tool\0with\0nulls",
        "LIVE12_STATUS", // Case sensitivity check
        "PROJECT_NEW",
    ];

    for &name in &invalid_names {
        let res = call_tool(&mut state, name, &json!({}));
        assert!(
            res.is_error,
            "Expected is_error=true for unrecognized tool name '{}'",
            name
        );
        let text = &res.content[0].text;
        assert!(
            text.contains("not recognized"),
            "Expected 'not recognized' in error message for '{}', got: {}",
            name,
            text
        );
    }

    // Huge tool name
    let huge_name = "a".repeat(50_000);
    let res_huge = call_tool(&mut state, &huge_name, &json!({}));
    assert!(res_huge.is_error);
}

// ============================================================================
// 4. MCP SERVER PROTOCOL PARSER FAULT INJECTION
// ============================================================================

#[test]
fn test_adv_mcp_server_protocol_parser_fault_injection() {
    let mut server = McpServer::new();

    // 1. Empty and whitespace lines
    assert_eq!(server.handle_line(""), None);
    assert_eq!(server.handle_line("   \r\n\t  "), None);

    // 2. Truncated and malformed JSON
    let err_resp1 = server.handle_line("{\"jsonrpc\": \"2.0\", \"method\":").expect("error response");
    let val1: Value = serde_json::from_str(&err_resp1).unwrap();
    assert_eq!(val1["error"]["code"], PARSE_ERROR);

    // 3. JSON array instead of request object
    let err_resp2 = server.handle_line("[1, 2, 3]").expect("error response");
    let val2: Value = serde_json::from_str(&err_resp2).unwrap();
    assert_eq!(val2["error"]["code"], PARSE_ERROR);

    // 4. Request without id (Notification)
    let notif = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    assert_eq!(server.handle_line(&notif.to_string()), None);
    assert!(server.is_initialized);

    // 5. Unknown method
    let unk_method = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "non_existent_method_xyz",
        "params": {}
    });
    let err_resp3 = server.handle_line(&unk_method.to_string()).expect("error response");
    let val3: Value = serde_json::from_str(&err_resp3).unwrap();
    assert_eq!(val3["error"]["code"], METHOD_NOT_FOUND);

    // 6. tools/call missing params
    let call_no_params = json!({
        "jsonrpc": "2.0",
        "id": 43,
        "method": "tools/call"
    });
    let err_resp4 = server.handle_line(&call_no_params.to_string()).expect("error response");
    let val4: Value = serde_json::from_str(&err_resp4).unwrap();
    assert_eq!(val4["error"]["code"], INVALID_PARAMS);

    // 7. tools/call missing 'name'
    let call_no_name = json!({
        "jsonrpc": "2.0",
        "id": 44,
        "method": "tools/call",
        "params": {}
    });
    let err_resp5 = server.handle_line(&call_no_name.to_string()).expect("error response");
    let val5: Value = serde_json::from_str(&err_resp5).unwrap();
    assert_eq!(val5["error"]["code"], INVALID_PARAMS);

    // 8. resources/read missing uri
    let read_no_uri = json!({
        "jsonrpc": "2.0",
        "id": 45,
        "method": "resources/read",
        "params": {}
    });
    let err_resp6 = server.handle_line(&read_no_uri.to_string()).expect("error response");
    let val6: Value = serde_json::from_str(&err_resp6).unwrap();
    assert_eq!(val6["error"]["code"], INVALID_PARAMS);

    // 9. resources/read unknown URI
    let read_unk_uri = json!({
        "jsonrpc": "2.0",
        "id": 46,
        "method": "resources/read",
        "params": { "uri": "invalid://resource_foo" }
    });
    let err_resp7 = server.handle_line(&read_unk_uri.to_string()).expect("error response");
    let val7: Value = serde_json::from_str(&err_resp7).unwrap();
    assert_eq!(val7["error"]["code"], INTERNAL_ERROR);
}

// ============================================================================
// 5. TCPLOMCLIENT: OFFLINE UNBOUND PORT BEHAVIOR & TIMEOUT
// ============================================================================

#[test]
fn test_adv_tcp_lom_client_offline_unbound_port() {
    let client = TcpLomClient::new("127.0.0.1", 28905);

    let start = Instant::now();
    let online = client.is_online();
    let elapsed = start.elapsed();

    assert!(!online, "is_online must return false for unbound port");
    assert!(
        elapsed < Duration::from_millis(500),
        "is_online took {:?} which exceeds expected connect timeout envelope",
        elapsed
    );

    let req_res = client.send_request("ping", json!({}));
    assert!(req_res.is_err(), "send_request must error on unbound port");

    let fallback_res = client.execute_with_fallback("ping", json!({}), || json!({ "mock": true }));
    assert_eq!(fallback_res["connected"], false);
    assert_eq!(fallback_res["fallback_executed"], true);
    assert_eq!(fallback_res["result"]["mock"], true);
}

// ============================================================================
// 6. TCPLOMCLIENT: DROPPED CONNECTION / IMMEDIATE RST
// ============================================================================

#[test]
fn test_adv_tcp_lom_client_immediate_drop_connection() {
    let listener = TcpListener::bind("127.0.0.1:28906").unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    let handle = thread::spawn(move || {
        while r_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    // Close immediately without sending any data
                    drop(stream);
                }
                Err(_) => break,
            }
        }
    });

    let client = TcpLomClient::new("127.0.0.1", 28906);

    let res = client.send_request("ping", json!({}));
    assert!(res.is_err(), "Expected error when server drops connection immediately");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("Empty response") || err_msg.contains("Failed to read"),
        "Unexpected error message: {}",
        err_msg
    );

    let fallback = client.execute_with_fallback("ping", json!({}), || json!({ "safe": true }));
    assert_eq!(fallback["connected"], false);
    assert_eq!(fallback["fallback_executed"], true);
    assert_eq!(fallback["result"]["safe"], true);

    running.store(false, Ordering::Relaxed);
    // Unblock accept by connecting once
    let _ = std::net::TcpStream::connect("127.0.0.1:28906");
    let _ = handle.join();
}

// ============================================================================
// 7. TCPLOMCLIENT: MALFORMED / NON-JSON RESPONSE FROM SERVER
// ============================================================================

#[test]
fn test_adv_tcp_lom_client_malformed_json_response() {
    let listener = TcpListener::bind("127.0.0.1:28907").unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    let handle = thread::spawn(move || {
        while r_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    let _ = reader.read_line(&mut line);
                    // Send corrupt payload
                    let _ = stream.write_all(b"NOT_A_VALID_JSON_OBJECT\n");
                    let _ = stream.flush();
                }
                Err(_) => break,
            }
        }
    });

    let client = TcpLomClient::new("127.0.0.1", 28907);

    let res = client.send_request("ping", json!({}));
    assert!(res.is_err(), "Expected error when server returns corrupt JSON");
    let err = res.unwrap_err();
    assert!(err.contains("Failed to parse JSON-RPC response"), "Got: {}", err);

    let fallback = client.execute_with_fallback("ping", json!({}), || json!({ "recovered": true }));
    assert_eq!(fallback["connected"], false);
    assert_eq!(fallback["fallback_executed"], true);
    assert_eq!(fallback["result"]["recovered"], true);

    running.store(false, Ordering::Relaxed);
    let _ = std::net::TcpStream::connect("127.0.0.1:28907");
    let _ = handle.join();
}

// ============================================================================
// 8. TCPLOMCLIENT: JSON-RPC ERROR RESPONSE HANDLING
// ============================================================================

#[test]
fn test_adv_tcp_lom_client_jsonrpc_error_handling() {
    let listener = TcpListener::bind("127.0.0.1:28908").unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    let handle = thread::spawn(move || {
        while r_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    let _ = reader.read_line(&mut line);
                    let resp = json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "error": { "code": -32603, "message": "Live 12 Device Locked" }
                    });
                    let mut s = resp.to_string();
                    s.push('\n');
                    let _ = stream.write_all(s.as_bytes());
                    let _ = stream.flush();
                }
                Err(_) => break,
            }
        }
    });

    let client = TcpLomClient::new("127.0.0.1", 28908);

    let res = client.send_request("get_track_devices", json!({ "track_index": 0 }));
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("LOM Error: Live 12 Device Locked"), "Got: {}", err);

    // execute_with_fallback catches LOM error and executes native fallback
    let fallback = client.execute_with_fallback("get_track_devices", json!({ "track_index": 0 }), || {
        json!({ "native_device_count": 2 })
    });
    assert_eq!(fallback["connected"], false);
    assert_eq!(fallback["fallback_executed"], true);
    assert_eq!(fallback["result"]["native_device_count"], 2);

    running.store(false, Ordering::Relaxed);
    let _ = std::net::TcpStream::connect("127.0.0.1:28908");
    let _ = handle.join();
}

// ============================================================================
// 9. TCPLOMCLIENT: HIGH LATENCY & TIMEOUT VERIFICATION
// ============================================================================

#[test]
fn test_adv_tcp_lom_client_read_timeout() {
    let listener = TcpListener::bind("127.0.0.1:28909").unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    let handle = thread::spawn(move || {
        while r_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((_stream, _)) => {
                    // Sleep longer than the client's timeout without writing response
                    thread::sleep(Duration::from_millis(800));
                }
                Err(_) => break,
            }
        }
    });

    let mut client = TcpLomClient::new("127.0.0.1", 28909);
    // Set a 200ms read timeout
    client.timeout = Duration::from_millis(200);

    let start = Instant::now();
    let res = client.send_request("ping", json!({}));
    let elapsed = start.elapsed();

    assert!(res.is_err(), "Expected timeout error from stalled server");
    assert!(
        elapsed >= Duration::from_millis(180) && elapsed < Duration::from_millis(600),
        "Timeout must fire within the configured envelope (~200ms), took {:?}",
        elapsed
    );

    running.store(false, Ordering::Relaxed);
    let _ = std::net::TcpStream::connect("127.0.0.1:28909");
    let _ = handle.join();
}

// ============================================================================
// 10. TCPLOMCLIENT: CONCURRENCY STRESS (ONLINE & OFFLINE)
// ============================================================================

#[test]
fn test_adv_tcp_lom_client_concurrency_stress() {
    // 1. Offline concurrency: 10 threads hammering an offline port
    let offline_client = Arc::new(TcpLomClient::new("127.0.0.1", 28910));
    let mut handles = Vec::new();

    for thread_idx in 0..10 {
        let c = offline_client.clone();
        handles.push(thread::spawn(move || {
            for i in 0..5 {
                let res = c.execute_with_fallback("ping", json!({ "iter": i }), || {
                    json!({ "thread": thread_idx, "iter": i })
                });
                assert_eq!(res["connected"], false);
                assert_eq!(res["fallback_executed"], true);
                assert_eq!(res["result"]["thread"], thread_idx);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // 2. Online concurrency: 8 threads hammering an online mock TCP server
    let listener = TcpListener::bind("127.0.0.1:28911").unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    let server_handle = thread::spawn(move || {
        while r_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_ok() {
                        if let Ok(req) = serde_json::from_str::<Value>(&line) {
                            let id = req["id"].clone();
                            let resp = json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": { "status": "ok", "echo": req["params"] }
                            });
                            let mut resp_str = resp.to_string();
                            resp_str.push('\n');
                            let _ = stream.write_all(resp_str.as_bytes());
                            let _ = stream.flush();
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    let online_client = Arc::new(TcpLomClient::new("127.0.0.1", 28911));
    let mut online_handles = Vec::new();

    for thread_idx in 0..8 {
        let c = online_client.clone();
        online_handles.push(thread::spawn(move || {
            for i in 0..5 {
                let res = c.execute_with_fallback("ping", json!({ "idx": thread_idx, "i": i }), || {
                    json!({ "fallback": true })
                });
                assert_eq!(res["connected"], true);
                assert_eq!(res["result"]["status"], "ok");
                assert_eq!(res["result"]["echo"]["idx"], thread_idx);
            }
        }));
    }

    for h in online_handles {
        h.join().unwrap();
    }

    running.store(false, Ordering::Relaxed);
    let _ = std::net::TcpStream::connect("127.0.0.1:28911");
    let _ = server_handle.join();
}

// ============================================================================
// 11. FULL MATRIX: ALL 15 LIVE12_* TOOLS IN OFFLINE FALLBACK MODE
// ============================================================================

#[test]
fn test_adv_all_15_live12_tools_offline_matrix() {
    let mut server = McpServer::new();
    server.state.lom_client.port = 28912; // Unbound port

    let live12_tools = [
        ("live12_status", json!({})),
        ("live12_get_song_info", json!({})),
        ("live12_create_midi_track", json!({ "index": 0 })),
        ("live12_create_audio_track", json!({ "index": 1 })),
        ("live12_create_return_track", json!({})),
        ("live12_load_browser_item", json!({ "track_index": 0, "uri": "query:synths" })),
        ("live12_create_clip", json!({ "track_index": 0, "slot_index": 0, "length_beats": 4.0 })),
        ("live12_add_notes", json!({ "track_index": 0, "slot_index": 0, "notes": [{"pitch": 60, "start_beat": 0.0, "duration_beats": 1.0}] })),
        ("live12_fire_clip", json!({ "track_index": 0, "slot_index": 0 })),
        ("live12_fire_scene", json!({ "scene_index": 0 })),
        ("live12_set_device_param", json!({ "track_index": 0, "device_index": 0, "param_index": 0, "value": 0.5 })),
        ("live12_get_track_devices", json!({ "track_index": 0 })),
        ("live12_set_tempo", json!({ "bpm": 132.0 })),
        ("live12_set_scale", json!({ "root_note": 7, "scale_name": "Dorian" })),
        ("live12_exec_lom_query", json!({ "query": "song.tempo" })),
    ];

    assert_eq!(live12_tools.len(), 15, "Must test all 15 Live 12 LOM tools");

    for (name, args) in &live12_tools {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 100,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": args
            }
        });

        let line = req.to_string();
        let resp_str = server.handle_line(&line).expect("Expected valid JSON-RPC response");
        let resp: Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 100);

        let content_text = resp["result"]["content"][0]["text"].as_str().expect("text content");
        let envelope: Value = serde_json::from_str(content_text).unwrap();

        assert_eq!(
            envelope["connected"], false,
            "Tool {} must report connected=false when port is unbound",
            name
        );
        assert_eq!(
            envelope["target"], "real_ableton_live_12",
            "Tool {} must specify target real_ableton_live_12",
            name
        );

        if *name == "live12_status" {
            assert_eq!(envelope["status"], "offline");
        } else {
            assert_eq!(
                envelope["fallback_executed"], true,
                "Tool {} must report fallback_executed=true",
                name
            );
            assert!(
                envelope["result"].is_object(),
                "Tool {} result must contain valid native fallback JSON object",
                name
            );
        }
    }
}
