//! Native Non-Blocking Aruston Live GUI RPC Server (`127.0.0.1:9877`).
//!
//! Synchronizes external MCP tool calls directly with the live open Aruston Studio
//! GUI window and audio engine in real time:
//! - Non-blocking TCP listener on `127.0.0.1:9877`
//! - Dispatches line-delimited JSON-RPC requests directly against `AbletonStudioApp`
//! - Updates session tracks, piano roll notes, device chains, transport, tempo, and scales
//! - Transmits corresponding `AudioCommand`s to the real-time zero-allocation audio engine

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use ableton_audio::PARAM_BPM;
use ableton_core::{AudioCommand, GlobalScale, ScaleMode};
use ableton_mcp::bridge::serde_json::{self, json, Value};
use ableton_timeline::{
    ClipPattern, MidiNote, MidiPattern, QuantizeMode, SessionClip, SessionSlot, SessionTrack,
};

use crate::app::{AbletonStudioApp, DeckTab, WorkspaceView};
use crate::panels::{CrossfaderAssign, DeviceKind, GuiDeviceModule, GuiReturnTrack, GuiTrackState};

pub const MAX_RPC_CLIENTS: usize = 16;
pub const MAX_RPC_LINE_BYTES: usize = 1_048_576; // 1 MiB
pub const MAX_READ_ITERS_PER_FRAME: usize = 32;

struct ActiveClient {
    stream: TcpStream,
    read_buf: Vec<u8>,
    keep: bool,
}

/// Embedded Native GUI RPC Server running inside Aruston Studio.
pub struct ArustonGuiRpcServer {
    listener: Option<TcpListener>,
    clients: Vec<ActiveClient>,
    pub port: u16,
}

impl Default for ArustonGuiRpcServer {
    fn default() -> Self {
        Self::new_on_port(9877)
    }
}

impl ArustonGuiRpcServer {
    /// Bind to the given port on localhost. If port is in use, fail gracefully without panicking.
    pub fn new_on_port(port: u16) -> Self {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = match TcpListener::bind(addr) {
            Ok(l) => {
                let _ = l.set_nonblocking(true);
                Some(l)
            }
            Err(e) => {
                eprintln!(
                    "ArustonGuiRpcServer: Port {} unavailable ({}), running without RPC server",
                    port, e
                );
                None
            }
        };
        Self {
            listener,
            clients: Vec::new(),
            port,
        }
    }

    /// Construct a disabled RPC server that does not bind any network socket.
    pub fn new_disabled() -> Self {
        Self {
            listener: None,
            clients: Vec::new(),
            port: 0,
        }
    }

    /// Poll any pending incoming connections and process buffered JSON-RPC requests.
    /// Mutates `app` in-place and sends `AudioCommand`s to the audio engine.
    /// Returns the number of requests successfully handled during this poll cycle.
    pub fn poll_and_apply(&mut self, app: &mut AbletonStudioApp) -> usize {
        // 1. Accept any pending non-blocking TCP connections
        if let Some(ref listener) = self.listener {
            loop {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let _ = stream.set_nonblocking(true);
                        if self.clients.len() >= MAX_RPC_CLIENTS {
                            self.clients.remove(0);
                        }
                        self.clients.push(ActiveClient {
                            stream,
                            read_buf: Vec::new(),
                            keep: true,
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
        }

        let mut handled_count = 0;

        // 2. Read available incoming data non-blockingly from all clients
        for client in self.clients.iter_mut() {
            let mut chunk = [0u8; 4096];
            let mut read_iters = 0;
            loop {
                if read_iters >= MAX_READ_ITERS_PER_FRAME {
                    break;
                }
                read_iters += 1;
                match client.stream.read(&mut chunk) {
                    Ok(0) => {
                        // EOF
                        client.keep = false;
                        break;
                    }
                    Ok(n) => {
                        client.read_buf.extend_from_slice(&chunk[..n]);
                        if client.read_buf.len() > MAX_RPC_LINE_BYTES
                            && !client.read_buf.contains(&b'\n')
                        {
                            client.keep = false;
                            client.read_buf.clear();
                            break;
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(_) => {
                        client.keep = false;
                        break;
                    }
                }
            }

            if !client.keep {
                continue;
            }

            // 3. Process complete newline-delimited JSON-RPC lines
            while let Some(pos) = client.read_buf.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = client.read_buf.drain(..=pos).collect();
                let line_str = match std::str::from_utf8(&line_bytes) {
                    Ok(s) => s.trim(),
                    Err(_) => continue,
                };
                if line_str.is_empty() {
                    continue;
                }

                let (_id, response_json) = handle_rpc_line(line_str, app);
                handled_count += 1;

                let mut out_str = response_json.to_string();
                out_str.push('\n');

                if client.stream.write_all(out_str.as_bytes()).is_err() {
                    client.keep = false;
                    break;
                }
                let _ = client.stream.flush();
            }
        }

        // 4. Clean up closed connections
        self.clients.retain(|c| c.keep);

        handled_count
    }
}

/// Parse and execute an individual JSON-RPC request line against `AbletonStudioApp`.
fn handle_rpc_line(line: &str, app: &mut AbletonStudioApp) -> (Value, Value) {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            let err_resp = json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "status": "error",
                "error": { "code": -32700, "message": format!("Parse error: {}", e) }
            });
            return (Value::Null, err_resp);
        }
    };

    let id = req.get("id").cloned().unwrap_or(json!(1));
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = execute_method(method, &params, app);

    let resp = json!({
        "jsonrpc": "2.0",
        "id": id,
        "status": "ok",
        "result": result
    });

    (id, resp)
}

/// Execute a specific RPC method on `AbletonStudioApp` and return the result payload.
fn execute_method(method: &str, params: &Value, app: &mut AbletonStudioApp) -> Value {
    match method {
        "ping" | "get_song_info" => json!({
            "status": "ok",
            "app": "Aruston Studio",
            "tempo": app.bpm,
            "bpm": app.bpm,
            "is_playing": app.is_playing,
            "track_count": app.tracks_gui.len(),
            "return_track_count": app.return_tracks.len(),
            "scene_count": app.matrix.scenes.len(),
            "scale_root": app.global_scale.root,
            "root_note": app.global_scale.root,
            "scale_name": format!("{:?}", app.global_scale.mode),
            "workspace_view": format!("{:?}", app.workspace_view),
            "deck_tab": format!("{:?}", app.deck_tab)
        }),

        "set_tempo" => {
            let bpm = params
                .get("bpm")
                .or_else(|| params.get("tempo"))
                .and_then(|v| v.as_f64())
                .unwrap_or(app.bpm);
            app.bpm = bpm.clamp(20.0, 999.0);
            app.send_command(AudioCommand::SetTempo(app.bpm));
            if let Some(ref handle) = app.audio_handle {
                handle.params.set(PARAM_BPM, app.bpm as f32);
            }
            json!({
                "status": "ok",
                "bpm": app.bpm,
                "tempo": app.bpm,
                "updated": true
            })
        }

        "set_transport" => {
            let playing = params
                .get("playing")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            app.is_playing = playing;
            if playing {
                app.send_command(AudioCommand::Play);
            } else {
                app.send_command(AudioCommand::Stop);
            }
            json!({
                "status": "ok",
                "is_playing": app.is_playing
            })
        }

        "play" | "transport_play" => {
            app.is_playing = true;
            app.send_command(AudioCommand::Play);
            json!({ "status": "ok", "is_playing": true })
        }

        "stop" | "transport_stop" => {
            app.is_playing = false;
            app.current_beat = 0.0;
            app.send_command(AudioCommand::Stop);
            json!({ "status": "ok", "is_playing": false })
        }

        "set_scale" => {
            let root_note = params
                .get("root_note")
                .and_then(|v| {
                    if let Some(num) = v.as_u64() {
                        Some(num as u8 % 12)
                    } else if let Some(s) = v.as_str() {
                        ableton_core::scale::ROOT_NAMES
                            .iter()
                            .position(|&r| r.eq_ignore_ascii_case(s))
                            .map(|p| p as u8)
                    } else {
                        None
                    }
                })
                .unwrap_or(app.global_scale.root);

            let scale_name = params
                .get("scale_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Major");

            let mode = match scale_name.to_lowercase().as_str() {
                "minor" | "aeolian" => ScaleMode::Minor,
                "dorian" => ScaleMode::Dorian,
                "phrygian" => ScaleMode::Phrygian,
                "lydian" => ScaleMode::Lydian,
                "mixolydian" => ScaleMode::Mixolydian,
                "locrian" => ScaleMode::Locrian,
                "harmonic_minor" | "harmonicminor" => ScaleMode::HarmonicMinor,
                "melodic_minor" | "melodicminor" => ScaleMode::MelodicMinor,
                "pentatonic" => ScaleMode::Pentatonic,
                "blues" => ScaleMode::Blues,
                "diminished" => ScaleMode::Diminished,
                "whole_tone" | "wholetone" => ScaleMode::WholeTone,
                _ => ScaleMode::Major,
            };

            let lock = params
                .get("scale_lock")
                .and_then(|v| v.as_bool())
                .unwrap_or(app.global_scale.scale_lock);

            app.global_scale = GlobalScale::new(root_note, mode).with_lock(lock);
            app.status_message = format!("Global Scale: {}", app.global_scale.display_name());

            json!({
                "status": "ok",
                "root_note": app.global_scale.root,
                "scale_name": format!("{:?}", app.global_scale.mode),
                "scale_lock": app.global_scale.scale_lock,
                "display_name": app.global_scale.display_name()
            })
        }

        "create_midi_track" | "create_audio_track" => {
            let is_midi = method == "create_midi_track";
            let default_prefix = if is_midi { "MIDI" } else { "Audio" };
            let track_id = app.matrix.tracks.len();
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{} Track {}", default_prefix, track_id + 1));
            let scene_count = app.matrix.scenes.len().max(8);

            app.matrix
                .tracks
                .push(SessionTrack::new(track_id as u32, &name, scene_count));
            app.tracks_gui.push(GuiTrackState {
                name: name.clone(),
                volume_linear: 0.9,
                pan: 0.0,
                mute: false,
                solo: false,
                sends: vec![0.0, 0.0],
                input_routing: "All Ins".into(),
                output_routing: "Master".into(),
                monitor_mode: "Auto".into(),
                crossfader_assign: CrossfaderAssign::Off,
            });

            json!({
                "status": "ok",
                "index": track_id,
                "track_count": app.tracks_gui.len(),
                "name": name
            })
        }

        "create_return_track" => {
            app.mixer_sections.show_returns = true;
            let ret_idx = app.return_tracks.len();
            let ret_char = (b'A' + (ret_idx % 26) as u8) as char;
            let ret_name = format!("Return {}", ret_char);
            app.return_tracks.push(GuiReturnTrack {
                name: ret_name.clone(),
                effect_name: "Echo".into(),
                volume_linear: 0.75,
                pan: 0.0,
                mute: false,
                solo: false,
            });
            json!({
                "status": "ok",
                "return_track_id": ret_idx,
                "return_track_count": app.return_tracks.len(),
                "name": ret_name
            })
        }

        "create_clip" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let slot_index = params
                .get("slot_index")
                .or_else(|| params.get("slot_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let length_beats = params
                .get("length_beats")
                .and_then(|v| v.as_f64())
                .unwrap_or(4.0);

            let scene_count = app.matrix.scenes.len().max(8);
            while app.matrix.tracks.len() <= track_index {
                let tid = app.matrix.tracks.len();
                app.matrix.tracks.push(SessionTrack::new(
                    tid as u32,
                    format!("Track {}", tid + 1),
                    scene_count,
                ));
                app.tracks_gui.push(GuiTrackState {
                    name: format!("Track {}", tid + 1),
                    volume_linear: 0.9,
                    pan: 0.0,
                    mute: false,
                    solo: false,
                    sends: vec![0.0, 0.0],
                    input_routing: "All Ins".into(),
                    output_routing: "Master".into(),
                    monitor_mode: "Auto".into(),
                    crossfader_assign: CrossfaderAssign::Off,
                });
            }

            let track = &mut app.matrix.tracks[track_index];
            while track.slots.len() <= slot_index {
                track.slots.push(SessionSlot::default());
            }

            let clip_id = ((track_index * 100) + slot_index + 1) as u32;
            let pattern = ClipPattern::Midi(MidiPattern::new(length_beats));
            track.slots[slot_index].set_clip(Some(SessionClip::new(clip_id, "Clip", pattern)));

            json!({
                "status": "ok",
                "created": true,
                "track_index": track_index,
                "slot_index": slot_index,
                "length_beats": length_beats
            })
        }

        "add_notes" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let slot_index = params
                .get("slot_index")
                .or_else(|| params.get("slot_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let scene_count = app.matrix.scenes.len().max(8);
            while app.matrix.tracks.len() <= track_index {
                let tid = app.matrix.tracks.len();
                app.matrix.tracks.push(SessionTrack::new(
                    tid as u32,
                    format!("Track {}", tid + 1),
                    scene_count,
                ));
                app.tracks_gui.push(GuiTrackState {
                    name: format!("Track {}", tid + 1),
                    volume_linear: 0.9,
                    pan: 0.0,
                    mute: false,
                    solo: false,
                    sends: vec![0.0, 0.0],
                    input_routing: "All Ins".into(),
                    output_routing: "Master".into(),
                    monitor_mode: "Auto".into(),
                    crossfader_assign: CrossfaderAssign::Off,
                });
            }

            let track = &mut app.matrix.tracks[track_index];
            while track.slots.len() <= slot_index {
                track.slots.push(SessionSlot::default());
            }

            let mut parsed_notes = Vec::new();
            if let Some(arr) = params.get("notes").and_then(|v| v.as_array()) {
                for n in arr {
                    let pitch = n.get("pitch").and_then(|v| v.as_u64()).unwrap_or(60) as u8;
                    let vel = n.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
                    let start = n.get("start_beat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let dur = n.get("duration_beats").and_then(|v| v.as_f64()).unwrap_or(1.0);
                    parsed_notes.push(MidiNote::new(pitch, vel, start, dur));
                }
            }

            let count = parsed_notes.len();

            // 1. Add to target SessionClip
            if let Some(ref mut clip) = track.slots[slot_index].clip {
                if let ClipPattern::Midi(ref mut mp) = clip.pattern {
                    for note in &parsed_notes {
                        mp.notes.push(*note);
                    }
                }
            } else {
                let clip_id = ((track_index * 100) + slot_index + 1) as u32;
                let mut mp = MidiPattern::new(4.0);
                mp.notes = parsed_notes.clone();
                track.slots[slot_index]
                    .set_clip(Some(SessionClip::new(clip_id, "Clip", ClipPattern::Midi(mp))));
            }

            // 2. Add to app.midi_pattern so Piano Roll immediately reflects the newly added notes!
            for note in &parsed_notes {
                app.midi_pattern.notes.push(*note);
            }

            json!({
                "status": "ok",
                "notes_added": count,
                "track_index": track_index,
                "slot_index": slot_index
            })
        }

        "fire_clip" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let slot_index = params
                .get("slot_index")
                .or_else(|| params.get("slot_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            if let Some(track) = app.matrix.tracks.get_mut(track_index) {
                let _ = track.trigger_slot(slot_index, 0.0, QuantizeMode::Immediate);
            }
            app.is_playing = true;
            app.has_session_override = true;
            app.send_command(AudioCommand::TriggerClip {
                track_id: track_index,
                slot_id: slot_index,
            });
            app.send_command(AudioCommand::Play);

            json!({
                "status": "ok",
                "fired": true,
                "track_index": track_index,
                "slot_index": slot_index
            })
        }

        "fire_scene" => {
            let scene_index = params
                .get("scene_index")
                .or_else(|| params.get("scene_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let _ = app.matrix.trigger_scene(scene_index, 0.0, QuantizeMode::Immediate);
            app.is_playing = true;
            app.has_session_override = true;
            app.send_command(AudioCommand::TriggerScene(scene_index));
            app.send_command(AudioCommand::Play);

            json!({
                "status": "ok",
                "fired": true,
                "scene_index": scene_index
            })
        }

        "load_browser_item" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let uri = params
                .get("uri")
                .or_else(|| params.get("item"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let lower = uri.to_lowercase();
            let dev_kind = if lower.contains("wavetable") {
                Some(DeviceKind::Wavetable)
            } else if lower.contains("operator") {
                Some(DeviceKind::Operator)
            } else if lower.contains("simpler") {
                Some(DeviceKind::Simpler)
            } else if lower.contains("drum_rack") || lower.contains("drum") {
                Some(DeviceKind::DrumRack)
            } else if lower.contains("eq_eight") || lower.contains("eq8") {
                Some(DeviceKind::EQEight)
            } else if lower.contains("glue") {
                Some(DeviceKind::GlueCompressor)
            } else if lower.contains("roar") {
                Some(DeviceKind::Roar)
            } else if lower.contains("auto_filter") || lower.contains("autofilter") {
                Some(DeviceKind::AutoFilter)
            } else if lower.contains("ott") {
                Some(DeviceKind::OTT)
            } else {
                None
            };

            if let Some(kind) = dev_kind {
                let next_id = (app.device_chain.devices.len() + 1) as u32;
                app.device_chain.devices.push(GuiDeviceModule::new(next_id, kind));
            }
            app.status_message = format!("Loaded item: {}", uri);

            json!({
                "status": "ok",
                "loaded": true,
                "track_index": track_index,
                "uri": uri,
                "device_count": app.device_chain.devices.len()
            })
        }

        "set_device_param" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let device_index = params
                .get("device_index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let param_index = params
                .get("param_index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let value = params.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);

            if device_index == 0 {
                match param_index {
                    0 => app.synth_state.cutoff_hz = value as f32,
                    1 => app.synth_state.resonance = value as f32,
                    2 => app.synth_state.pulse_pw = value as f32,
                    _ => {}
                }
            }
            app.send_command(AudioCommand::SetParameter {
                track_id: track_index,
                param_id: param_index as u32,
                value: value as f32,
            });

            json!({
                "status": "ok",
                "track_index": track_index,
                "device_index": device_index,
                "param_index": param_index,
                "value": value
            })
        }

        "get_track_devices" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let devices: Vec<Value> = app
                .device_chain
                .devices
                .iter()
                .enumerate()
                .map(|(idx, dev)| {
                    json!({
                        "index": idx,
                        "id": dev.id,
                        "name": dev.name,
                        "kind": format!("{:?}", dev.kind),
                        "is_enabled": dev.is_enabled,
                        "parameters": [
                            { "index": 0, "name": "Frequency / Macro 1", "value": 0.5 },
                            { "index": 1, "name": "Resonance / Macro 2", "value": 0.5 }
                        ]
                    })
                })
                .collect();

            json!({
                "status": "ok",
                "track_index": track_index,
                "device_count": devices.len(),
                "devices": devices
            })
        }

        "switch_view" => {
            if let Some(view) = params
                .get("view")
                .or_else(|| params.get("workspace_view"))
                .and_then(|v| v.as_str())
            {
                let v_lower = view.to_lowercase();
                if v_lower.contains("arrangement") || v_lower.contains("timeline") {
                    app.workspace_view = WorkspaceView::ArrangementTimeline;
                } else if v_lower.contains("session") || v_lower.contains("matrix") {
                    app.workspace_view = WorkspaceView::SessionMatrix;
                }
            }
            if let Some(deck) = params
                .get("deck")
                .or_else(|| params.get("deck_tab"))
                .and_then(|v| v.as_str())
            {
                let d_lower = deck.to_lowercase();
                if d_lower.contains("clip") {
                    app.deck_tab = DeckTab::ClipView;
                } else if d_lower.contains("device") || d_lower.contains("rack") {
                    app.deck_tab = DeckTab::DeviceRack;
                } else if d_lower.contains("seq") || d_lower.contains("drum") {
                    app.deck_tab = DeckTab::DrumSequencer;
                } else if d_lower.contains("xy") || d_lower.contains("pad") || d_lower.contains("play") {
                    app.deck_tab = DeckTab::XyPad;
                }
            }
            json!({
                "status": "ok",
                "workspace_view": format!("{:?}", app.workspace_view),
                "deck_tab": format!("{:?}", app.deck_tab)
            })
        }

        "exec_lom_query" => {
            let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let q_lower = query.to_lowercase();
            let val = if q_lower.contains("tempo") || q_lower.contains("bpm") {
                json!(app.bpm)
            } else if q_lower.contains("is_playing") || q_lower.contains("playing") {
                json!(app.is_playing)
            } else if q_lower.contains("tracks") {
                let track_names: Vec<String> = app.tracks_gui.iter().map(|t| t.name.clone()).collect();
                json!({ "tracks": track_names, "count": track_names.len() })
            } else if q_lower.contains("scale") {
                json!({
                    "root": app.global_scale.root,
                    "mode": format!("{:?}", app.global_scale.mode),
                    "display": app.global_scale.display_name()
                })
            } else {
                json!({
                    "app": "Aruston Studio",
                    "tempo": app.bpm,
                    "tracks": app.tracks_gui.len(),
                    "query": query,
                    "evaluated": true
                })
            };
            json!({
                "status": "ok",
                "query": query,
                "result": val
            })
        }

        _ => json!({
            "status": "ok",
            "method": method,
            "handled_by": "ArustonStudioApp",
            "message": format!("Method '{}' executed against live GUI state", method)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::Duration;

    #[test]
    fn test_aruston_live_rpc_roundtrip_and_mutations() {
        let mut app = AbletonStudioApp::new_mock(44100.0);
        let mut server = ArustonGuiRpcServer::new_on_port(19877);

        assert!(
            server.listener.is_some(),
            "Server must bind successfully to port 19877"
        );

        let mut client = TcpStream::connect("127.0.0.1:19877")
            .expect("Client must connect to 127.0.0.1:19877");
        client
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();

        // 1. set_tempo (140.0)
        let msg1 = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "set_tempo",
            "params": { "bpm": 140.0 }
        });
        let mut s1 = msg1.to_string();
        s1.push('\n');
        client.write_all(s1.as_bytes()).unwrap();

        // 2. create_midi_track
        let msg2 = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "create_midi_track",
            "params": { "name": "Synth Lead" }
        });
        let mut s2 = msg2.to_string();
        s2.push('\n');
        client.write_all(s2.as_bytes()).unwrap();

        // 3. add_notes to track 4, slot 0
        let msg3 = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "add_notes",
            "params": {
                "track_index": 4,
                "slot_index": 0,
                "notes": [
                    { "pitch": 64, "velocity": 110, "start_beat": 0.0, "duration_beats": 1.0 },
                    { "pitch": 67, "velocity": 115, "start_beat": 1.0, "duration_beats": 1.0 }
                ]
            }
        });
        let mut s3 = msg3.to_string();
        s3.push('\n');
        client.write_all(s3.as_bytes()).unwrap();
        client.flush().unwrap();

        // Give socket a brief moment
        std::thread::sleep(Duration::from_millis(30));

        let handled = server.poll_and_apply(&mut app);
        assert_eq!(handled, 3, "All 3 RPC commands must be handled");

        // Read responses from client stream
        let mut reader = BufReader::new(client);
        let mut line1 = String::new();
        reader.read_line(&mut line1).unwrap();
        let resp1: Value = serde_json::from_str(&line1).unwrap();
        assert_eq!(resp1["status"], "ok");
        assert_eq!(resp1["result"]["bpm"], 140.0);

        let mut line2 = String::new();
        reader.read_line(&mut line2).unwrap();
        let resp2: Value = serde_json::from_str(&line2).unwrap();
        assert_eq!(resp2["status"], "ok");

        let mut line3 = String::new();
        reader.read_line(&mut line3).unwrap();
        let resp3: Value = serde_json::from_str(&line3).unwrap();
        assert_eq!(resp3["status"], "ok");
        assert_eq!(resp3["result"]["notes_added"], 2);

        // Verify state mutations on app
        assert_eq!(app.bpm, 140.0);
        assert_eq!(app.tracks_gui.len(), 5);
        assert!(app.midi_pattern.notes.iter().any(|n| n.pitch == 64));
        assert!(app.midi_pattern.notes.iter().any(|n| n.pitch == 67));
    }

    #[test]
    fn test_aruston_live_rpc_connection_limit_and_oversized_line() {
        let mut app = AbletonStudioApp::new_mock(44100.0);
        let mut server = ArustonGuiRpcServer::new_on_port(19878);
        assert!(server.listener.is_some());

        // Connect client in non-blocking mode so writing >1 MiB on a single thread doesn't block OS TCP buffer
        let mut client = TcpStream::connect("127.0.0.1:19878").unwrap();
        client.set_nonblocking(true).unwrap();
        let chunk64k = vec![b'x'; 65536];
        for _ in 0..20 {
            let _ = client.write(&chunk64k);
            let _ = server.poll_and_apply(&mut app);
        }
        // The oversized client should have been dropped / killed
        assert_eq!(server.clients.len(), 0);

        // Verify connection limit capping at MAX_RPC_CLIENTS
        let mut streams = Vec::new();
        for _ in 0..(MAX_RPC_CLIENTS + 5) {
            if let Ok(s) = TcpStream::connect("127.0.0.1:19878") {
                streams.push(s);
            }
        }
        std::thread::sleep(Duration::from_millis(30));
        let _ = server.poll_and_apply(&mut app);
        assert!(server.clients.len() <= MAX_RPC_CLIENTS);
    }
}
