//! Native Non-Blocking Oxide Studio Live GUI RPC Server (`127.0.0.1:9877`).
//!
//! Synchronizes external MCP tool calls directly with the live open Oxide Studio
//! GUI window and audio engine in real time:
//! - Non-blocking TCP listener on `127.0.0.1:9877`
//! - Dispatches line-delimited JSON-RPC requests directly against `OxideStudioApp`
//! - Updates session tracks, piano roll notes, device chains, transport, tempo, and scales
//! - Transmits corresponding `AudioCommand`s to the real-time zero-allocation audio engine

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use oxide_audio::{
    PARAM_BPM, PARAM_FX_DRIVE, PARAM_ID_SYNTH_CUTOFF, PARAM_ID_SYNTH_ENV_MOD,
    PARAM_ID_SYNTH_RESONANCE,
};
use oxide_core::{AudioCommand, GlobalScale, ScaleMode};
use oxide_mcp::bridge::serde_json::{self, json, Value};
use oxide_timeline::{
    ArrangementTrack, ClipPattern, MidiNote, MidiPattern, QuantizeMode, SessionClip, SessionSlot,
    SessionTrack,
};

use crate::app::{OxideStudioApp, DeckTab, WorkspaceView};
use crate::panels::{
    CrossfaderAssign, DeviceChainViewState, DeviceKind, GuiDeviceModule, GuiReturnTrack,
    GuiTrackState,
};

pub const MAX_RPC_CLIENTS: usize = 16;
pub const MAX_RPC_LINE_BYTES: usize = 1_048_576; // 1 MiB
pub const MAX_RPC_WRITE_BYTES: usize = 4_194_304; // 4 MiB
pub const MAX_READ_ITERS_PER_FRAME: usize = 32;
pub const MAX_REQUESTS_PER_FRAME: usize = 16;
pub const MAX_RPC_TRACK_INDEX: usize = 128;
pub const MAX_RPC_SLOT_INDEX: usize = 256;

struct ActiveClient {
    stream: TcpStream,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
    write_offset: usize,
    keep: bool,
}

impl ActiveClient {
    fn enqueue_response(&mut self, response_json: &Value) {
        let mut out_str = response_json.to_string();
        out_str.push('\n');
        if self.write_buf.len().saturating_sub(self.write_offset) + out_str.len()
            > MAX_RPC_WRITE_BYTES
        {
            self.keep = false;
            return;
        }
        self.write_buf.extend_from_slice(out_str.as_bytes());
    }

    fn flush_writes(&mut self) {
        while self.write_offset < self.write_buf.len() {
            match self.stream.write(&self.write_buf[self.write_offset..]) {
                Ok(0) => {
                    self.keep = false;
                    break;
                }
                Ok(n) => {
                    self.write_offset += n;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.keep = false;
                    break;
                }
            }
        }
        if self.write_offset >= self.write_buf.len() {
            self.write_buf.clear();
            self.write_offset = 0;
        } else if self.write_offset > 65_536 {
            self.write_buf.drain(..self.write_offset);
            self.write_offset = 0;
        }
    }
}

/// Embedded Native GUI RPC Server running inside Oxide Studio.
pub struct OxideGuiRpcServer {
    listener: Option<TcpListener>,
    clients: Vec<ActiveClient>,
    pub port: u16,
}

impl Default for OxideGuiRpcServer {
    fn default() -> Self {
        Self::new_on_port(9877)
    }
}

impl OxideGuiRpcServer {
    /// Bind to the given port on localhost. If port is in use, fail gracefully without panicking.
    pub fn new_on_port(port: u16) -> Self {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let (listener, actual_port) = match TcpListener::bind(addr) {
            Ok(l) => {
                let _ = l.set_nonblocking(true);
                let bound_port = l.local_addr().map(|a| a.port()).unwrap_or(port);
                (Some(l), bound_port)
            }
            Err(e) => {
                eprintln!(
                    "OxideGuiRpcServer: Port {} unavailable ({}), running without RPC server",
                    port, e
                );
                (None, port)
            }
        };
        Self {
            listener,
            clients: Vec::new(),
            port: actual_port,
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
    pub fn poll_and_apply(&mut self, app: &mut OxideStudioApp) -> usize {
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
                            write_buf: Vec::new(),
                            write_offset: 0,
                            keep: true,
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
        }

        let mut handled_count = 0;

        // 2. Read available incoming data non-blockingly and flush outgoing buffers across ticks
        for client in self.clients.iter_mut() {
            client.flush_writes();
            if !client.keep {
                continue;
            }

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

            // 3. Process complete newline-delimited JSON-RPC lines (capped per frame)
            while handled_count < MAX_REQUESTS_PER_FRAME {
                let Some(pos) = client.read_buf.iter().position(|&b| b == b'\n') else {
                    break;
                };
                let line_bytes: Vec<u8> = client.read_buf.drain(..=pos).collect();
                let line_str = match std::str::from_utf8(&line_bytes) {
                    Ok(s) => s.trim(),
                    Err(_) => {
                        handled_count += 1;
                        let err_resp = json!({
                            "jsonrpc": "2.0",
                            "id": Value::Null,
                            "status": "error",
                            "error": { "code": -32700, "message": "Parse error: invalid UTF-8" }
                        });
                        client.enqueue_response(&err_resp);
                        client.flush_writes();
                        if !client.keep {
                            break;
                        }
                        continue;
                    }
                };
                if line_str.is_empty() {
                    continue;
                }

                let (_id, response_json) = handle_rpc_line(line_str, app);
                handled_count += 1;
                client.enqueue_response(&response_json);
                client.flush_writes();
                if !client.keep {
                    break;
                }
            }
        }

        // 4. Clean up closed connections
        self.clients.retain(|c| c.keep);

        handled_count
    }
}

/// Parse and execute an individual JSON-RPC request line against `OxideStudioApp`.
fn handle_rpc_line(line: &str, app: &mut OxideStudioApp) -> (Value, Value) {
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

    let resp = match execute_method(method, &params, app) {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "status": "ok",
            "result": result
        }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "status": "error",
            "error": { "code": code, "message": message }
        }),
    };

    (id, resp)
}

fn validate_track_index(track_index: usize) -> Result<(), (i32, String)> {
    if track_index > MAX_RPC_TRACK_INDEX {
        return Err((
            -32602,
            format!(
                "Invalid params: track_index {} exceeds maximum {}",
                track_index, MAX_RPC_TRACK_INDEX
            ),
        ));
    }
    Ok(())
}

fn validate_slot_index(slot_index: usize) -> Result<(), (i32, String)> {
    if slot_index > MAX_RPC_SLOT_INDEX {
        return Err((
            -32602,
            format!(
                "Invalid params: slot_index {} exceeds maximum {}",
                slot_index, MAX_RPC_SLOT_INDEX
            ),
        ));
    }
    Ok(())
}

fn ensure_rpc_tracks_up_to(app: &mut OxideStudioApp, target_track_index: usize, is_midi: bool) {
    let scene_count = app.matrix.scenes.len().max(8);
    let track_kind = if is_midi { "MidiSynth" } else { "Audio" };
    while app.matrix.tracks.len() <= target_track_index {
        let tid = app.matrix.tracks.len();
        let name = format!("Track {}", tid + 1);
        app.matrix
            .tracks
            .push(SessionTrack::new(tid as u32, &name, scene_count));
        if app.tracks_gui.len() <= tid {
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
        }
        if app.timeline.tracks.len() <= tid {
            app.timeline
                .tracks
                .push(ArrangementTrack::new((tid + 1) as u32, &name, track_kind));
        }
        app.ensure_track_device_chain(tid);
    }
    app.ensure_track_device_chain(target_track_index);
    while app.timeline.tracks.len() <= target_track_index {
        let tid = app.timeline.tracks.len();
        let name = app
            .tracks_gui
            .get(tid)
            .map(|t| t.name.clone())
            .unwrap_or_else(|| format!("Track {}", tid + 1));
        app.timeline
            .tracks
            .push(ArrangementTrack::new((tid + 1) as u32, &name, track_kind));
    }
}

fn stop_and_reset_transport(app: &mut OxideStudioApp) {
    app.is_playing = false;
    app.current_beat = 0.0;
    app.last_triggered_step = None;
    let notes: Vec<u8> = app.active_synth_notes.drain(..).map(|(p, _)| p).collect();
    for pitch in notes {
        app.send_command(AudioCommand::NoteOff {
            track_id: 0,
            note: pitch,
        });
    }
    app.active_synth_note = None;
    app.send_command(AudioCommand::Stop);
}

/// Execute a specific RPC method on `OxideStudioApp` and return the result payload.
fn execute_method(
    method: &str,
    params: &Value,
    app: &mut OxideStudioApp,
) -> Result<Value, (i32, String)> {
    match method {
        "ping" | "get_song_info" => Ok(json!({
            "status": "ok",
            "app": "Oxide Studio",
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
        })),

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
            Ok(json!({
                "status": "ok",
                "bpm": app.bpm,
                "tempo": app.bpm,
                "updated": true
            }))
        }

        "set_transport" => {
            let playing = params
                .get("playing")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if playing {
                app.is_playing = true;
                app.last_triggered_step = None;
                app.send_command(AudioCommand::Play);
            } else {
                stop_and_reset_transport(app);
            }
            Ok(json!({
                "status": "ok",
                "is_playing": app.is_playing
            }))
        }

        "play" | "transport_play" => {
            app.is_playing = true;
            app.last_triggered_step = None;
            app.send_command(AudioCommand::Play);
            Ok(json!({ "status": "ok", "is_playing": true }))
        }

        "stop" | "transport_stop" => {
            stop_and_reset_transport(app);
            Ok(json!({ "status": "ok", "is_playing": false }))
        }

        "set_scale" => {
            let root_note = params
                .get("root_note")
                .and_then(|v| {
                    if let Some(num) = v.as_u64() {
                        Some((num % 12) as u8)
                    } else if let Some(s) = v.as_str() {
                        oxide_core::scale::ROOT_NAMES
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

            let normalized_scale: String = scale_name
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect();

            let mode = match normalized_scale.as_str() {
                "minor" | "aeolian" | "naturalminor" => ScaleMode::Minor,
                "dorian" => ScaleMode::Dorian,
                "phrygian" => ScaleMode::Phrygian,
                "lydian" => ScaleMode::Lydian,
                "mixolydian" => ScaleMode::Mixolydian,
                "locrian" => ScaleMode::Locrian,
                "harmonicminor" => ScaleMode::HarmonicMinor,
                "melodicminor" => ScaleMode::MelodicMinor,
                "pentatonic" | "majorpentatonic" => ScaleMode::Pentatonic,
                "blues" => ScaleMode::Blues,
                "diminished" => ScaleMode::Diminished,
                "wholetone" => ScaleMode::WholeTone,
                _ => ScaleMode::Major,
            };

            let lock = params
                .get("scale_lock")
                .and_then(|v| v.as_bool())
                .unwrap_or(app.global_scale.scale_lock);

            app.global_scale = GlobalScale::new(root_note, mode).with_lock(lock);
            app.status_message = format!("Global Scale: {}", app.global_scale.display_name());

            Ok(json!({
                "status": "ok",
                "root_note": app.global_scale.root,
                "scale_name": format!("{:?}", app.global_scale.mode),
                "scale_lock": app.global_scale.scale_lock,
                "display_name": app.global_scale.display_name()
            }))
        }

        "create_midi_track" | "create_audio_track" => {
            let is_midi = method == "create_midi_track";
            let track_id = app.matrix.tracks.len();
            validate_track_index(track_id)?;

            let default_prefix = if is_midi { "MIDI" } else { "Audio" };
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
            app.track_device_chains
                .push(DeviceChainViewState::default_for_track(track_id, false));
            app.timeline.tracks.push(ArrangementTrack::new(
                (track_id + 1) as u32,
                &name,
                if is_midi { "MidiSynth" } else { "Audio" },
            ));

            Ok(json!({
                "status": "ok",
                "index": track_id,
                "track_count": app.tracks_gui.len(),
                "name": name
            }))
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
            Ok(json!({
                "status": "ok",
                "return_track_id": ret_idx,
                "return_track_count": app.return_tracks.len(),
                "name": ret_name
            }))
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
            validate_track_index(track_index)?;
            validate_slot_index(slot_index)?;

            let length_beats = params
                .get("length_beats")
                .and_then(|v| v.as_f64())
                .unwrap_or(4.0);

            ensure_rpc_tracks_up_to(app, track_index, true);

            let track = &mut app.matrix.tracks[track_index];
            while track.slots.len() <= slot_index {
                track.slots.push(SessionSlot::default());
            }

            let clip_id = ((track_index * 100) + slot_index + 1) as u32;
            let pattern = ClipPattern::Midi(MidiPattern::new(length_beats));
            track.slots[slot_index].set_clip(Some(SessionClip::new(clip_id, "Clip", pattern)));

            Ok(json!({
                "status": "ok",
                "created": true,
                "track_index": track_index,
                "slot_index": slot_index,
                "length_beats": length_beats
            }))
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
            validate_track_index(track_index)?;
            validate_slot_index(slot_index)?;

            ensure_rpc_tracks_up_to(app, track_index, true);

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

            // 2. Sync to app.midi_pattern only when editing the active selected clip
            if track_index == app.selected_track
                && app.selected_clip_slot == Some((track_index, slot_index))
            {
                for note in &parsed_notes {
                    app.midi_pattern.notes.push(*note);
                }
            }

            Ok(json!({
                "status": "ok",
                "notes_added": count,
                "track_index": track_index,
                "slot_index": slot_index
            }))
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
            validate_track_index(track_index)?;
            validate_slot_index(slot_index)?;

            if let Some(track) = app.matrix.tracks.get_mut(track_index) {
                let _ = track.trigger_slot(slot_index, 0.0, QuantizeMode::Immediate);
            }
            app.is_playing = true;
            app.has_session_override = true;
            app.last_triggered_step = None;
            app.send_command(AudioCommand::TriggerClip {
                track_id: track_index,
                slot_id: slot_index,
            });
            app.send_command(AudioCommand::Play);

            Ok(json!({
                "status": "ok",
                "fired": true,
                "track_index": track_index,
                "slot_index": slot_index
            }))
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
            app.last_triggered_step = None;
            app.send_command(AudioCommand::TriggerScene(scene_index));
            app.send_command(AudioCommand::Play);

            Ok(json!({
                "status": "ok",
                "fired": true,
                "scene_index": scene_index
            }))
        }

        "load_browser_item" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            validate_track_index(track_index)?;

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

            app.ensure_track_device_chain(track_index);
            let Some(kind) = dev_kind else {
                let device_count = app
                    .track_device_chains
                    .get(track_index)
                    .map(|c| c.devices.len())
                    .unwrap_or(0);
                app.status_message = format!("Unknown browser item: {}", uri);
                return Ok(json!({
                    "status": "ok",
                    "loaded": false,
                    "track_index": track_index,
                    "uri": uri,
                    "device_count": device_count,
                    "error": format!("Unknown browser item URI: {}", uri)
                }));
            };

            let chain = &mut app.track_device_chains[track_index];
            let next_id = (chain.devices.len() + 1) as u32;
            chain.devices.push(GuiDeviceModule::new(next_id, kind));
            let device_count = chain.devices.len();
            if track_index == app.selected_track {
                app.device_chain = chain.clone();
            }
            app.status_message = format!("Loaded item: {} (Track {})", uri, track_index + 1);

            Ok(json!({
                "status": "ok",
                "loaded": true,
                "track_index": track_index,
                "uri": uri,
                "device_count": device_count
            }))
        }

        "set_device_param" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            validate_track_index(track_index)?;

            let device_index = params
                .get("device_index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let param_index = params
                .get("param_index")
                .or_else(|| params.get("param"))
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
                })
                .unwrap_or(0) as usize;
            let value = params.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let fval = value as f32;

            app.ensure_track_device_chain(track_index);
            let dev_kind = app
                .track_device_chains
                .get(track_index)
                .and_then(|chain| chain.devices.get(device_index))
                .map(|dev| dev.kind);

            if let Some(chain) = app.track_device_chains.get_mut(track_index) {
                if let Some(dev) = chain.devices.get_mut(device_index) {
                    apply_gui_device_param(dev, param_index, fval);
                }
            }
            if track_index == app.selected_track {
                if let Some(dev) = app.device_chain.devices.get_mut(device_index) {
                    apply_gui_device_param(dev, param_index, fval);
                }
            }

            let mapped_command: Option<(u32, f32)> = match (device_index, dev_kind) {
                (0, Some(DeviceKind::Wavetable)) => match param_index {
                    0 => {
                        if track_index == 0 {
                            app.synth_state.cutoff_hz = fval.clamp(20.0, 20000.0);
                        }
                        Some((PARAM_ID_SYNTH_CUTOFF, fval))
                    }
                    1 => {
                        if track_index == 0 {
                            app.synth_state.resonance = fval.clamp(0.0, 1.0);
                        }
                        Some((PARAM_ID_SYNTH_RESONANCE, fval))
                    }
                    // Keep Wavetable table_pos separate from oscillator pulse width (`PARAM_ID_SYNTH_PULSE_PW`)
                    2 => Some((100, fval.clamp(0.0, 1.0))),
                    id if id >= 10 => Some((id as u32, fval)),
                    _ => Some((3000 + (param_index as u32).min(31), fval)),
                },
                (0, None) if track_index == 0 => match param_index {
                    0 => {
                        app.synth_state.cutoff_hz = fval.clamp(20.0, 20000.0);
                        Some((PARAM_ID_SYNTH_CUTOFF, fval))
                    }
                    1 => {
                        app.synth_state.resonance = fval.clamp(0.0, 1.0);
                        Some((PARAM_ID_SYNTH_RESONANCE, fval))
                    }
                    2 => Some((100, fval.clamp(0.0, 1.0))),
                    id if id >= 10 => Some((id as u32, fval)),
                    _ => None,
                },
                (_, Some(DeviceKind::Roar)) if param_index == 0 => {
                    Some((PARAM_FX_DRIVE as u32, (fval / 12.0).clamp(0.0, 2.0)))
                }
                (_, Some(DeviceKind::AutoFilter)) => match param_index {
                    0 => Some((PARAM_ID_SYNTH_CUTOFF, fval.clamp(20.0, 20000.0))),
                    1 => Some((PARAM_ID_SYNTH_RESONANCE, fval.clamp(0.0, 1.0))),
                    2 => Some((PARAM_ID_SYNTH_ENV_MOD, fval.clamp(-1.0, 1.0))),
                    _ => Some((
                        3000 + (device_index as u32) * 32 + (param_index as u32).min(31),
                        fval,
                    )),
                },
                _ if dev_kind.is_some() => Some((
                    3000 + (device_index as u32).min(127) * 32 + (param_index as u32).min(31),
                    fval,
                )),
                _ => None,
            };

            if let Some((audio_pid, audio_val)) = mapped_command {
                app.send_command(AudioCommand::SetParameter {
                    track_id: track_index,
                    param_id: audio_pid,
                    value: audio_val,
                });
            }

            Ok(json!({
                "status": "ok",
                "track_index": track_index,
                "device_index": device_index,
                "param_index": param_index,
                "value": value
            }))
        }

        "get_track_devices" => {
            let track_index = params
                .get("track_index")
                .or_else(|| params.get("track_id"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            validate_track_index(track_index)?;

            let chain_ref = if track_index == app.selected_track {
                Some(&app.device_chain)
            } else {
                app.track_device_chains.get(track_index)
            };

            let devices: Vec<Value> = chain_ref
                .map(|chain| {
                    chain
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
                                "parameters": serialize_gui_device_params(dev)
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(json!({
                "status": "ok",
                "track_index": track_index,
                "device_count": devices.len(),
                "devices": devices
            }))
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
            Ok(json!({
                "status": "ok",
                "workspace_view": format!("{:?}", app.workspace_view),
                "deck_tab": format!("{:?}", app.deck_tab)
            }))
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
                    "app": "Oxide Studio",
                    "tempo": app.bpm,
                    "tracks": app.tracks_gui.len(),
                    "query": query,
                    "evaluated": true
                })
            };
            Ok(json!({
                "status": "ok",
                "query": query,
                "result": val
            }))
        }

        _ => Err((-32601, format!("Method not found: '{}'", method))),
    }
}

fn apply_gui_device_param(dev: &mut GuiDeviceModule, param_index: usize, value: f32) {
    match dev.kind {
        DeviceKind::Wavetable => match param_index {
            0 => dev.wavetable.filter1_cutoff = value.clamp(20.0, 20000.0),
            1 => dev.wavetable.filter1_res = value.clamp(0.0, 1.0),
            2 => dev.wavetable.table_pos = value.clamp(0.0, 1.0),
            3 => dev.wavetable.filter2_cutoff = value.clamp(20.0, 20000.0),
            4 => dev.wavetable.filter2_res = value.clamp(0.0, 1.0),
            _ => {}
        },
        DeviceKind::Operator => match param_index {
            0 => dev.operator.ops[0].level = value.clamp(0.0, 2.0),
            1 => dev.operator.ops[0].ratio = value.clamp(0.5, 32.0),
            2 => dev.operator.algorithm_id = (value.round() as usize).min(10),
            3 => dev.operator.ops[1].level = value.clamp(0.0, 2.0),
            _ => {}
        },
        DeviceKind::Roar => match param_index {
            0 => dev.roar.drive_db = value.clamp(0.0, 36.0),
            1 => dev.roar.tone = value.clamp(-1.0, 1.0),
            2 => dev.roar.feedback = value.clamp(0.0, 0.95),
            3 => dev.roar.feedback_freq_hz = value.clamp(20.0, 20000.0),
            _ => {}
        },
        DeviceKind::EQEight => match param_index {
            0 => dev.eq8.bands[0].freq_hz = value.clamp(10.0, 20000.0),
            1 => dev.eq8.bands[0].gain_db = value.clamp(-15.0, 15.0),
            2 => dev.eq8.bands[0].q = value.clamp(0.1, 18.0),
            3 => dev.eq8.bands[1].freq_hz = value.clamp(10.0, 20000.0),
            4 => dev.eq8.bands[1].gain_db = value.clamp(-15.0, 15.0),
            _ => {}
        },
        DeviceKind::GlueCompressor => match param_index {
            0 => {
                dev.glue.threshold_db = value.clamp(-40.0, 0.0);
                dev.glue.recompute_gain_reduction();
            }
            1 => {
                dev.glue.ratio = value.clamp(1.0, 20.0);
                dev.glue.recompute_gain_reduction();
            }
            2 => dev.glue.attack_ms = value.clamp(0.01, 100.0),
            3 => dev.glue.release_ms = value.clamp(50.0, 2000.0),
            _ => {}
        },
        DeviceKind::AutoFilter => match param_index {
            0 => dev.auto_filter.cutoff_hz = value.clamp(20.0, 20000.0),
            1 => dev.auto_filter.resonance = value.clamp(0.0, 1.0),
            2 => dev.auto_filter.env_amount = value.clamp(-1.0, 1.0),
            3 => dev.auto_filter.lfo_amount = value.clamp(0.0, 1.0),
            4 => dev.auto_filter.lfo_rate_hz = value.clamp(0.05, 50.0),
            _ => {}
        },
        DeviceKind::OTT => match param_index {
            0 => dev.ott.depth = value.clamp(0.0, 1.0),
            1 => dev.ott.time_scale = value.clamp(0.1, 10.0),
            2 => dev.ott.output_gain_db = value.clamp(-24.0, 24.0),
            _ => {}
        },
        DeviceKind::Simpler => match param_index {
            0 => dev.simpler.start_pos = value.clamp(0.0, 1.0),
            1 => dev.simpler.end_pos = value.clamp(0.0, 1.0),
            _ => {}
        },
        DeviceKind::DrumRack => match param_index {
            0 => dev.drum_rack.bank = (value.round() as usize).min(7),
            1 => dev.drum_rack.selected_pad = (value.round() as usize).min(15),
            _ => {}
        },
    }
}

fn serialize_gui_device_params(dev: &GuiDeviceModule) -> Vec<Value> {
    match dev.kind {
        DeviceKind::Wavetable => vec![
            json!({ "index": 0, "name": "filter1_cutoff", "value": dev.wavetable.filter1_cutoff }),
            json!({ "index": 1, "name": "filter1_res", "value": dev.wavetable.filter1_res }),
            json!({ "index": 2, "name": "table_pos", "value": dev.wavetable.table_pos }),
            json!({ "index": 3, "name": "filter2_cutoff", "value": dev.wavetable.filter2_cutoff }),
            json!({ "index": 4, "name": "filter2_res", "value": dev.wavetable.filter2_res }),
        ],
        DeviceKind::Operator => vec![
            json!({ "index": 0, "name": "op_a_level", "value": dev.operator.ops[0].level }),
            json!({ "index": 1, "name": "op_a_ratio", "value": dev.operator.ops[0].ratio }),
            json!({ "index": 2, "name": "algorithm_id", "value": dev.operator.algorithm_id }),
            json!({ "index": 3, "name": "op_b_level", "value": dev.operator.ops[1].level }),
        ],
        DeviceKind::Roar => vec![
            json!({ "index": 0, "name": "drive_db", "value": dev.roar.drive_db }),
            json!({ "index": 1, "name": "tone", "value": dev.roar.tone }),
            json!({ "index": 2, "name": "feedback", "value": dev.roar.feedback }),
            json!({ "index": 3, "name": "feedback_freq_hz", "value": dev.roar.feedback_freq_hz }),
        ],
        DeviceKind::EQEight => vec![
            json!({ "index": 0, "name": "band1_freq_hz", "value": dev.eq8.bands[0].freq_hz }),
            json!({ "index": 1, "name": "band1_gain_db", "value": dev.eq8.bands[0].gain_db }),
            json!({ "index": 2, "name": "band1_q", "value": dev.eq8.bands[0].q }),
            json!({ "index": 3, "name": "band2_freq_hz", "value": dev.eq8.bands[1].freq_hz }),
            json!({ "index": 4, "name": "band2_gain_db", "value": dev.eq8.bands[1].gain_db }),
        ],
        DeviceKind::GlueCompressor => vec![
            json!({ "index": 0, "name": "threshold_db", "value": dev.glue.threshold_db }),
            json!({ "index": 1, "name": "ratio", "value": dev.glue.ratio }),
            json!({ "index": 2, "name": "attack_ms", "value": dev.glue.attack_ms }),
            json!({ "index": 3, "name": "release_ms", "value": dev.glue.release_ms }),
            json!({ "index": 4, "name": "gain_reduction_db", "value": dev.glue.gain_reduction_db }),
        ],
        DeviceKind::AutoFilter => vec![
            json!({ "index": 0, "name": "cutoff_hz", "value": dev.auto_filter.cutoff_hz }),
            json!({ "index": 1, "name": "resonance", "value": dev.auto_filter.resonance }),
            json!({ "index": 2, "name": "env_amount", "value": dev.auto_filter.env_amount }),
            json!({ "index": 3, "name": "lfo_amount", "value": dev.auto_filter.lfo_amount }),
            json!({ "index": 4, "name": "lfo_rate_hz", "value": dev.auto_filter.lfo_rate_hz }),
        ],
        DeviceKind::OTT => vec![
            json!({ "index": 0, "name": "depth", "value": dev.ott.depth }),
            json!({ "index": 1, "name": "time_scale", "value": dev.ott.time_scale }),
            json!({ "index": 2, "name": "output_gain_db", "value": dev.ott.output_gain_db }),
        ],
        DeviceKind::Simpler => vec![
            json!({ "index": 0, "name": "start_pos", "value": dev.simpler.start_pos }),
            json!({ "index": 1, "name": "end_pos", "value": dev.simpler.end_pos }),
        ],
        DeviceKind::DrumRack => vec![
            json!({ "index": 0, "name": "bank", "value": dev.drum_rack.bank }),
            json!({ "index": 1, "name": "selected_pad", "value": dev.drum_rack.selected_pad }),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::Duration;

    #[test]
    fn test_oxide_live_rpc_roundtrip_and_mutations() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        let mut server = OxideGuiRpcServer::new_on_port(19877);

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

        // Verify state mutations on app (Track 4 slot 0 updated, Track 0 active midi_pattern NOT polluted)
        assert_eq!(app.bpm, 140.0);
        assert_eq!(app.tracks_gui.len(), 5);
        assert_eq!(app.track_device_chains.len(), 5);
        assert_eq!(app.timeline.tracks.len(), 5);
        let slot_clip = app.matrix.tracks[4].slots[0].clip.as_ref().unwrap();
        if let ClipPattern::Midi(ref mp) = slot_clip.pattern {
            assert!(mp.notes.iter().any(|n| n.pitch == 64));
            assert!(mp.notes.iter().any(|n| n.pitch == 67));
        } else {
            panic!("Expected Midi clip in Track 4 slot 0");
        }
        assert!(
            !app.midi_pattern.notes.iter().any(|n| n.pitch == 64),
            "Track 0 active midi_pattern must not be polluted by Track 4 add_notes"
        );
    }

    #[test]
    fn test_oxide_live_rpc_r2_regression_suite() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        let mut server = OxideGuiRpcServer::new_on_port(0);
        assert!(server.listener.is_some());
        let port = server.port;
        assert_ne!(port, 0);

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();

        // 1. String param_index "1" and Wavetable table_pos param_index "2" (must not clobber pulse_pw)
        let orig_pw = app.synth_state.pulse_pw;
        let req1 = json!({
            "jsonrpc": "2.0", "id": 10, "method": "set_device_param",
            "params": { "track_index": 0, "device_index": 0, "param_index": "1", "value": 0.77 }
        });
        let req2 = json!({
            "jsonrpc": "2.0", "id": 11, "method": "set_device_param",
            "params": { "track_index": 0, "device_index": 0, "param_index": "2", "value": 0.65 }
        });
        // 2. Scale normalization with spaces
        let req3 = json!({
            "jsonrpc": "2.0", "id": 12, "method": "set_scale",
            "params": { "root_note": "A", "scale_name": "Harmonic Minor" }
        });
        // 3. Unknown method -> JSON-RPC -32601
        let req4 = json!({
            "jsonrpc": "2.0", "id": 13, "method": "nonexistent_method", "params": {}
        });
        // 4. Out-of-bounds track_index -> JSON-RPC -32602 (no OOM loop)
        let req5 = json!({
            "jsonrpc": "2.0", "id": 14, "method": "create_clip",
            "params": { "track_index": 999999999u64, "slot_index": 0 }
        });
        // 5. Unknown browser URI -> loaded: false
        let req6 = json!({
            "jsonrpc": "2.0", "id": 15, "method": "load_browser_item",
            "params": { "track_index": 1, "uri": "oxide://unknown/nope" }
        });
        // 6. Track 1+ & device_index >= 1 set_device_param emits AudioCommand
        let req7 = json!({
            "jsonrpc": "2.0", "id": 16, "method": "set_device_param",
            "params": { "track_index": 0, "device_index": 1, "param_index": "0", "value": 18.0 }
        });

        let mut wire = Vec::new();
        for r in [req1, req2, req3, req4, req5, req6, req7] {
            wire.extend_from_slice(r.to_string().as_bytes());
            wire.push(b'\n');
        }
        // 7. Invalid UTF-8 line -> JSON-RPC -32700 Parse error
        wire.extend_from_slice(b"\xff\xfe\xfd\n");

        client.write_all(&wire).unwrap();
        client.flush().unwrap();
        std::thread::sleep(Duration::from_millis(30));

        let handled = server.poll_and_apply(&mut app);
        assert_eq!(handled, 8);
        assert!((app.device_chain.devices[0].wavetable.filter1_res - 0.77).abs() < 1e-4);
        assert!((app.device_chain.devices[0].wavetable.table_pos - 0.65).abs() < 1e-4);
        assert_eq!(app.synth_state.pulse_pw, orig_pw);
        assert_eq!(app.global_scale.mode, ScaleMode::HarmonicMinor);

        let mut reader = BufReader::new(client);
        let mut lines = Vec::new();
        for _ in 0..8 {
            let mut s = String::new();
            reader.read_line(&mut s).unwrap();
            lines.push(serde_json::from_str::<Value>(&s).unwrap());
        }
        assert_eq!(lines[0]["status"], "ok");
        assert_eq!(lines[2]["result"]["scale_name"], "HarmonicMinor");
        assert_eq!(lines[3]["error"]["code"], -32601);
        assert_eq!(lines[4]["error"]["code"], -32602);
        assert_eq!(lines[5]["result"]["loaded"], false);
        assert_eq!(lines[6]["status"], "ok");
        assert_eq!(lines[7]["error"]["code"], -32700);
    }

    #[test]
    fn test_oxide_live_rpc_connection_limit_and_oversized_line() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        let mut server = OxideGuiRpcServer::new_on_port(19878);
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
