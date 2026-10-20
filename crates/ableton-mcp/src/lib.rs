//! Full-Control Model Context Protocol (MCP) Server for ableton-rust.
//!
//! Exposes complete programmatic control over every subsystem of the electronic music DAW
//! over JSON-RPC 2.0 `stdio`:
//! - Project persistence (load, save, new)
//! - Transport & Engine Telemetry (BPM, swing, play, stop, RMS, peak dBFS)
//! - Tracks, Synthesizer & 909 Drum Rack (parameters, 38 presets, 17 XY macro mappings, choke groups)
//! - Session Clip Matrix & Linear Arrangement Timeline (MIDI, 909 drum steps, LilyPond sequences, automation)
//! - Multi-Track Mixer & Effects Chain (Saturator, Delay, Reverb, Limiter)
//! - Out-of-Process AU/VST3 Plugin Scanner
//! - High-Resolution WAV Bounce & Export
//! - Agentic Music Composition Generators (Chords, Basslines, Arpeggios, Euclidean Drums, 909 Grooves, Arrangements)
//! - "Agent Ears" Perceptual & 4-Band Spectral Audio Analysis (Sub/Bass, Low-Mid, High-Mid, High, Centroid)

pub mod analysis;
pub mod lilypond;
pub mod music_theory;
pub mod protocol;
pub mod resources;
pub mod server;
pub mod state;
pub mod tools;

pub use analysis::{AudioAnalysisMetrics, FourBandEnergy};
pub use lilypond::{parse_lilypond, sequence_to_clip_pattern};
pub use music_theory::{bjorklund, build_chord_notes, generate_drum_groove, get_scale_intervals};
pub use protocol::{
    InitializeResult, JsonRpcError, JsonRpcRequest, JsonRpcResponse, Resource, ResourceContent,
    ResourceReadResult, ServerCapabilities, ServerInfo, Tool, ToolCallResult, ToolContent,
    ToolInputSchema, MCP_PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
};
pub use resources::{get_resource_definitions, read_resource};
pub use server::McpServer;
pub use state::DawState;
pub use tools::{call_tool, get_tool_definitions};

/// Entry point to run line-delimited JSON-RPC 2.0 MCP server over standard I/O.
pub fn run_mcp_server_stdio() {
    let mut server = McpServer::new();
    server.run_stdio();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mcp_initialize_handshake() {
        let mut server = McpServer::new();
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "unit-test-agent", "version": "1.0" }
            }
        });

        let resp_str = server.handle_line(&init_req.to_string()).expect("Expected response");
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp["result"]["serverInfo"]["name"], "ableton-rust-mcp");
        assert_eq!(resp["result"]["serverInfo"]["version"], "0.1.0");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
        assert!(resp["result"]["capabilities"]["resources"].is_object());
    }

    #[test]
    fn test_mcp_tools_list() {
        let mut server = McpServer::new();
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });

        let resp_str = server.handle_line(&list_req.to_string()).expect("Expected response");
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        let tools = resp["result"]["tools"].as_array().expect("tools array");
        assert!(tools.len() >= 25, "Must list all DAW subsystem tools, got {}", tools.len());

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"project_new"));
        assert!(names.contains(&"project_save"));
        assert!(names.contains(&"project_load"));
        assert!(names.contains(&"transport_play"));
        assert!(names.contains(&"synth_set_parameter"));
        assert!(names.contains(&"synth_load_preset"));
        assert!(names.contains(&"drums_configure_pad"));
        assert!(names.contains(&"clip_create_midi"));
        assert!(names.contains(&"clip_create_lilypond"));
        assert!(names.contains(&"generate_chord_progression_clip"));
        assert!(names.contains(&"generate_drum_groove_clip"));
        assert!(names.contains(&"build_song_arrangement"));
        assert!(names.contains(&"analyze_rendered_audio"));
        assert!(names.contains(&"preview_synth_patch"));
    }

    #[test]
    fn test_mcp_resources_list_and_read() {
        let mut server = McpServer::new();

        // 1. resources/list
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "resources/list",
            "params": {}
        });
        let resp_str = server.handle_line(&list_req.to_string()).unwrap();
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        let res_arr = resp["result"]["resources"].as_array().unwrap();
        assert!(res_arr.iter().any(|r| r["uri"] == "project://current"));
        assert!(res_arr.iter().any(|r| r["uri"] == "telemetry://current"));
        assert!(res_arr.iter().any(|r| r["uri"] == "presets://catalog"));
        assert!(res_arr.iter().any(|r| r["uri"] == "mappings://catalog"));

        // 2. resources/read project://current
        let read_req = json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "resources/read",
            "params": { "uri": "project://current" }
        });
        let read_resp_str = server.handle_line(&read_req.to_string()).unwrap();
        let read_resp: serde_json::Value = serde_json::from_str(&read_resp_str).unwrap();
        let contents = read_resp["result"]["contents"].as_array().unwrap();
        assert!(!contents.is_empty());
        assert!(contents[0]["text"].as_str().unwrap().contains("metadata"));
    }
}
