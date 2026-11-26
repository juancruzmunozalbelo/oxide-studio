//! Tool catalogue, schema declarations, and dispatch logic for DAW autonomous control.

use std::fs;
use std::path::PathBuf;
use serde_json::{json, Value};

use oxide_bounce::{render_project_window_to_wav, DemoArrangementRenderer, WavWriter};
use oxide_core::device::{DeviceCategory, DeviceChain};
use oxide_core::scale::{GlobalScale, ScaleMode};
use oxide_dsp::presets::{load_mapping_catalog, load_preset_catalog};
use oxide_dsp::PARAMETER_DESCRIPTORS;
use oxide_drums::DrumPadSource;
use oxide_plugin_scanner::PluginScannerSupervisor;
use oxide_project::{DAWProject, InstrumentConfig, SynthPatchParameters, TrackDefinition};
use oxide_timeline::track::SendRoutingMode;
use oxide_timeline::{
    detect_transients, generate_euclidean, generate_rhythm, generate_seed, generate_shape,
    generate_stacks, transform_arpeggiate, transform_connect, transform_ornament,
    transform_quantize, transform_recombine, transform_span, transform_strum,
    transform_time_warp, ArrangementClip, AudioClipData, AutomationLane, AutomationPoint,
    ClipPattern, ContourShape, DrumStep, DrumStepPattern, MidiNote, MidiPattern, QuantizeMode,
    RhythmStyle, ScaleLock, SessionClip, SpanMode, StacksChordType, StacksVoicing, StrumDirection,
    WarpMode, WarpedAudioClip,
};

use crate::analysis::AudioAnalysisMetrics;
use crate::browser::BrowserCategory;
use crate::devices::{
    add_device_to_chain, dump_device_chain_detailed, instantiate_managed_device,
    reorder_device_chain_global, ManagedDeviceHandle,
};
use crate::lilypond::{parse_lilypond, sequence_to_clip_pattern};
use crate::music_theory::{
    bjorklund, build_chord_notes, generate_drum_groove, get_scale_intervals, parse_pitch_class,
    parse_roman_degree,
};
use crate::protocol::{Tool, ToolCallResult, ToolInputSchema};
use crate::state::DawState;

const MAX_TRACK_ID: usize = 128;
const MAX_SLOT_ID: usize = 256;
const MAX_TOTAL_BARS: usize = 512;

fn validate_track_id(id: usize) -> Result<usize, ToolCallResult> {
    if id > MAX_TRACK_ID {
        Err(ToolCallResult::error(format!(
            "track_id {} exceeds upper bound ({})",
            id, MAX_TRACK_ID
        )))
    } else {
        Ok(id)
    }
}

fn validate_slot_id(id: usize) -> Result<usize, ToolCallResult> {
    if id > MAX_SLOT_ID {
        Err(ToolCallResult::error(format!(
            "slot_id {} exceeds upper bound ({})",
            id, MAX_SLOT_ID
        )))
    } else {
        Ok(id)
    }
}

fn validate_bpm(bpm: f64) -> Result<f64, ToolCallResult> {
    if !bpm.is_finite() || !(20.0..=999.0).contains(&bpm) {
        Err(ToolCallResult::error(format!(
            "bpm {} out of valid range [20.0..=999.0]",
            bpm
        )))
    } else {
        Ok(bpm)
    }
}

fn current_scale_lock(state: &DawState) -> ScaleLock {
    ScaleLock {
        root: state.scale.root,
        degree_mask: state.scale.mode.degree_mask(),
    }
}

fn insert_track_shifting_state(
    state: &mut DawState,
    insert_at: usize,
    trk: TrackDefinition,
    track_name: &str,
    tl_label: &str,
) {
    let is_synth = matches!(trk.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_));
    let is_drum = matches!(trk.instrument, InstrumentConfig::ElectronicDrumRack(_));

    state.project.tracks.insert(insert_at, trk);
    for (i, t) in state.project.tracks.iter_mut().enumerate() {
        t.id = i as u32;
    }

    let m_idx = insert_at.min(state.matrix.tracks.len());
    state.matrix.tracks.insert(
        m_idx,
        oxide_timeline::SessionTrack::new(m_idx as u32, track_name, 8),
    );
    for (i, t) in state.matrix.tracks.iter_mut().enumerate() {
        t.id = i as u32;
    }

    let tl_idx = insert_at.min(state.timeline.tracks.len());
    state.timeline.tracks.insert(
        tl_idx,
        oxide_timeline::ArrangementTrack::new(tl_idx as u32, track_name, tl_label),
    );
    for (i, t) in state.timeline.tracks.iter_mut().enumerate() {
        t.id = i as u32;
    }

    if insert_at <= state.track_manager.tracks.len() {
        let mut new_tm = oxide_timeline::track::Track::new(
            insert_at as u32,
            track_name,
            if tl_label == "Audio" {
                oxide_timeline::track::TrackType::Audio
            } else {
                oxide_timeline::track::TrackType::Midi
            },
        );
        for (s_idx, _) in state.custom_return_ids.iter().enumerate() {
            new_tm.set_send(s_idx as u32, -70.0, SendRoutingMode::PostFader);
        }
        state.track_manager.tracks.insert(insert_at, new_tm);
        for (i, t) in state.track_manager.tracks.iter_mut().enumerate() {
            t.id = i as u32;
        }
    }

    if insert_at < state.mixer.tracks.len() {
        for i in (insert_at + 1..state.mixer.tracks.len()).rev() {
            state.mixer.tracks[i] = state.mixer.tracks[i - 1].clone();
        }
        state.mixer.tracks[insert_at] = oxide_fx::MixerTrack::new(track_name);
    }

    let old_chains = std::mem::take(&mut state.device_chains);
    for (k, v) in old_chains {
        let new_k = if (k as usize) >= insert_at { k + 1 } else { k };
        state.device_chains.insert(new_k, v);
    }

    let old_handles = std::mem::take(&mut state.device_handles);
    for (k, v) in old_handles {
        let new_k = if (k as usize) >= insert_at { k + 1 } else { k };
        state.device_handles.insert(new_k, v);
    }

    let old_clips = std::mem::take(&mut state.audio_clips);
    for ((t, s), v) in old_clips {
        let new_t = if (t as usize) >= insert_at { t + 1 } else { t };
        state.audio_clips.insert((new_t, s), v);
    }

    let _ = (is_synth, is_drum);
    if state.active_synth_track_id >= insert_at {
        state.active_synth_track_id += 1;
    }
    if state.active_drum_track_id >= insert_at {
        state.active_drum_track_id += 1;
    }
}

/// Expand a leading `~` or `~/` path prefix using `HOME`.
fn expand_tilde_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

/// Generate a collision-free fallback path under the OS temp directory.
fn default_temp_file(stem: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{}_{}.{}", stem, std::process::id(), ext))
}

fn parse_scale_mode_name(name: &str) -> ScaleMode {
    match name.trim().to_ascii_lowercase().as_str() {
        "minor" | "aeolian" | "natural_minor" | "natural minor" => ScaleMode::Minor,
        "dorian" => ScaleMode::Dorian,
        "phrygian" => ScaleMode::Phrygian,
        "lydian" => ScaleMode::Lydian,
        "mixolydian" => ScaleMode::Mixolydian,
        "locrian" => ScaleMode::Locrian,
        "harmonic_minor" | "harmonic minor" | "harmonicminor" => ScaleMode::HarmonicMinor,
        "melodic_minor" | "melodic minor" | "melodicminor" => ScaleMode::MelodicMinor,
        "pentatonic" => ScaleMode::Pentatonic,
        "blues" => ScaleMode::Blues,
        "diminished" => ScaleMode::Diminished,
        "whole_tone" | "whole tone" | "wholetone" => ScaleMode::WholeTone,
        _ => ScaleMode::Major,
    }
}

fn set_track_device_param_helper(
    state: &mut DawState,
    track_id: u32,
    device_index: usize,
    param_key: &str,
    value: f32,
) -> Result<(String, String, f32), String> {
    let handles = state
        .device_handles
        .get(&track_id)
        .ok_or_else(|| format!("Track {} has no managed device chain", track_id))?;

    let handle = handles.get_at(device_index).cloned().ok_or_else(|| {
        format!(
            "Device index {} out of range for track {} (count: {})",
            device_index,
            track_id,
            handles.device_count()
        )
    })?;

    let device_name = handle.name();
    let (resolved_param, applied_value) = handle.set_parameter(param_key, value)?;

    // Keep track insert_fx synchronized immediately in project state
    let bpm = state.bpm;
    if let Some(p_trk) = state.project.tracks.get_mut(track_id as usize) {
        if let Some(track_handles) = state.device_handles.get(&track_id) {
            p_trk.insert_fx = track_handles
                .audio_effects
                .iter()
                .filter_map(|h| h.to_insert_effect_config(bpm))
                .collect();
        }
    }

    Ok((device_name, resolved_param, applied_value))
}

/// Return complete catalogue of all registered MCP tools.
///
/// Honors `OXIDE_DISABLE_LEGACY_LIVE12_TOOLS=1` (or `true`/`yes`) to omit Group-15
/// `live12_*` bridge tools when running alongside `ableton-live12-mcp`.
pub fn get_tool_definitions() -> Vec<Tool> {
    let disable_legacy = std::env::var("OXIDE_DISABLE_LEGACY_LIVE12_TOOLS")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    get_tool_definitions_filtered(disable_legacy)
}

/// Return the registered MCP tool catalogue, optionally hiding Group-15 `live12_*` bridge tools.
pub fn get_tool_definitions_filtered(disable_legacy_live12: bool) -> Vec<Tool> {
    let mut tools = vec![
        // --- Project & Transport ---
        Tool {
            name: "project_new".to_string(),
            description: "Create a new DAW project with default lead synth and 909 drum rack tracks.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "name": { "type": "string", "description": "Project title" }
                })),
                required: None,
            },
        },
        Tool {
            name: "project_load".to_string(),
            description: "Load complete DAW project from a JSON project file on disk.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "path": { "type": "string", "description": "Absolute path to the project JSON file" }
                })),
                required: Some(vec!["path".to_string()]),
            },
        },
        Tool {
            name: "project_save".to_string(),
            description: "Save current DAW project (tracks, clips, presets, macros, timeline) to a JSON file.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "path": { "type": "string", "description": "Target JSON output path" }
                })),
                required: None,
            },
        },
        Tool {
            name: "transport_play".to_string(),
            description: "Start real-time audio playback / sequencer transport.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },
        Tool {
            name: "transport_stop".to_string(),
            description: "Stop real-time audio playback and reset transport playhead to zero.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },
        Tool {
            name: "transport_set_bpm".to_string(),
            description: "Set project tempo in beats per minute (BPM).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "bpm": { "type": "number", "description": "Tempo in BPM (e.g. 126.0)" }
                })),
                required: Some(vec!["bpm".to_string()]),
            },
        },
        Tool {
            name: "transport_set_swing".to_string(),
            description: "Set 16th-note swing / shuffle ratio (0.0 = straight, 0.5 = neutral, 0.75 = heavy swing).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "swing": { "type": "number", "description": "Swing ratio 0.0 to 0.75" }
                })),
                required: Some(vec!["swing".to_string()]),
            },
        },
        Tool {
            name: "transport_get_telemetry".to_string(),
            description: "Query real-time DAW audio telemetry (RMS, peak dBFS, playhead beat, CPU load).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },

        // --- Tracks, Synth & 909 Drums ---
        Tool {
            name: "track_create_synth".to_string(),
            description: "Create a new synthesizer track with Mystran ZDF ladder filter instrument.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "name": { "type": "string", "description": "Track name" },
                    "preset": { "type": "string", "description": "Optional initial preset filename" },
                    "preset_name": { "type": "string", "description": "Alias for initial preset filename" }
                })),
                required: Some(vec!["name".to_string()]),
            },
        },
        Tool {
            name: "track_create_drums".to_string(),
            description: "Create a new 16-pad 909 electronic drum rack track.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "name": { "type": "string", "description": "Track name" }
                })),
                required: Some(vec!["name".to_string()]),
            },
        },
        Tool {
            name: "track_delete".to_string(),
            description: "Delete an existing track by index or ID.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Track index [0..N-1]" }
                })),
                required: Some(vec!["track_id".to_string()]),
            },
        },
        Tool {
            name: "synth_list_presets".to_string(),
            description: "List all 38 factory synth presets imported from Ableton Learning Synths.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },
        Tool {
            name: "synth_load_preset".to_string(),
            description: "Load one of the 38 factory synth presets onto the synth engine.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "preset_name": { "type": "string", "description": "Preset filename, e.g. 'wowbass.preset' or 'acid.preset'" },
                    "track_id": { "type": "integer", "description": "Optional target synth track ID" }
                })),
                required: Some(vec!["preset_name".to_string()]),
            },
        },
        Tool {
            name: "synth_set_parameter".to_string(),
            description: "Set any synthesizer parameter by name or ParamId (filter_frequency, filter_resonance, etc.).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "parameter": { "description": "Parameter name (string) or index (integer 0..29)" },
                    "value": { "type": "number", "description": "Target value" },
                    "is_normalized": { "type": "boolean", "description": "Whether value is normalized 0.0..1.0" }
                })),
                required: Some(vec!["parameter".to_string(), "value".to_string()]),
            },
        },
        Tool {
            name: "synth_set_macro".to_string(),
            description: "Set Performance XY Macro coordinates [0.0..1.0] and evaluate the 17 XY macro mappings.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "x": { "type": "number", "description": "X coordinate [0.0..1.0]" },
                    "y": { "type": "number", "description": "Y coordinate [0.0..1.0]" },
                    "mapping_name": { "type": "string", "description": "Optional mapping key e.g. 'wowbass.mapping'" }
                })),
                required: None,
            },
        },
        Tool {
            name: "drums_configure_pad".to_string(),
            description: "Configure 909 drum pad parameters (pitch, decay, level, choke groups, WAV sample path).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "pad_index": { "type": "integer", "description": "Pad index [0..15] (0=Kick, 2=Snare, 6=Closed HH, 10=Open HH)" },
                    "pitch_semitones": { "type": "number", "description": "Pitch offset in semitones [-24.0..24.0]" },
                    "decay_seconds": { "type": "number", "description": "Decay time in seconds [0.01..2.0]" },
                    "volume": { "type": "number", "description": "Gain [0.0..2.0]" },
                    "pan": { "type": "number", "description": "Pan [-1.0..1.0]" },
                    "choke_group": { "type": "integer", "description": "Choke group ID (e.g. 1 for HHs)" },
                    "wav_path": { "type": "string", "description": "Path to custom WAV sample file" }
                })),
                required: Some(vec!["pad_index".to_string()]),
            },
        },
        Tool {
            name: "drums_trigger_pad".to_string(),
            description: "Trigger a drum pad hit with specified velocity [1..127].".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "pad_index": { "type": "integer", "description": "Pad index [0..15]" },
                    "velocity": { "type": "integer", "description": "Velocity [1..127]" }
                })),
                required: Some(vec!["pad_index".to_string()]),
            },
        },

        // --- Session Matrix & Timeline ---
        Tool {
            name: "clip_create_midi".to_string(),
            description: "Create a melodic MIDI clip in a Session Matrix track slot.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track index" },
                    "slot_id": { "type": "integer", "description": "Target slot index (scene row)" },
                    "name": { "type": "string", "description": "Clip name" },
                    "length_beats": { "type": "number", "description": "Loop length in beats (default 4.0)" },
                    "notes": {
                        "type": "array",
                        "description": "List of MIDI notes",
                        "items": {
                            "type": "object",
                            "properties": {
                                "pitch": { "type": "integer" },
                                "velocity": { "type": "integer" },
                                "start_beat": { "type": "number" },
                                "duration_beats": { "type": "number" }
                            },
                            "required": ["pitch", "start_beat", "duration_beats"]
                        }
                    }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string()]),
            },
        },
        Tool {
            name: "clip_create_drums".to_string(),
            description: "Create a 16-step or 32-step 909 drum pattern clip in a Session Matrix slot.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track index" },
                    "slot_id": { "type": "integer", "description": "Target slot index" },
                    "name": { "type": "string", "description": "Clip name" },
                    "step_count": { "type": "integer", "description": "16 or 32 steps (default 16)" },
                    "steps": {
                        "type": "array",
                        "description": "Active drum steps",
                        "items": {
                            "type": "object",
                            "properties": {
                                "pad_index": { "type": "integer" },
                                "step_index": { "type": "integer" },
                                "velocity": { "type": "integer" },
                                "accent": { "type": "boolean" }
                            },
                            "required": ["pad_index", "step_index"]
                        }
                    }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string()]),
            },
        },
        Tool {
            name: "clip_create_lilypond".to_string(),
            description: "Parse LilyPond-style musical notation string (e.g. '\\tempo 4 = 120 c4 d4 e4 g4' or drum triggers 'bd4 sn4 hh8') into a Session clip.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track index" },
                    "slot_id": { "type": "integer", "description": "Target slot index" },
                    "name": { "type": "string", "description": "Clip name" },
                    "notation": { "type": "string", "description": "LilyPond notation text" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "notation".to_string()]),
            },
        },
        Tool {
            name: "clip_launch".to_string(),
            description: "Trigger playback of a Session clip with quantization (1 bar, 1/4 note, or immediate).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Track index" },
                    "slot_id": { "type": "integer", "description": "Slot index" },
                    "quantize_mode": { "type": "string", "description": "'1_bar', '1_beat', or 'immediate'" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string()]),
            },
        },
        Tool {
            name: "clip_stop".to_string(),
            description: "Stop clip playback on a specific track with quantization.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Track index" }
                })),
                required: Some(vec!["track_id".to_string()]),
            },
        },
        Tool {
            name: "scene_launch".to_string(),
            description: "Launch an entire horizontal scene row across all Session Matrix tracks simultaneously.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "scene_index": { "type": "integer", "description": "Scene index [0..N-1]" }
                })),
                required: Some(vec!["scene_index".to_string()]),
            },
        },
        Tool {
            name: "timeline_place_clip".to_string(),
            description: "Place a clip horizontally on the linear Arrangement timeline at a specific beat position.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Arrangement track index" },
                    "slot_id": { "type": "integer", "description": "Source session clip slot index (default 0)" },
                    "clip_id": { "type": "integer", "description": "Clip ID" },
                    "start_beat": { "type": "number", "description": "Start position on timeline in musical beats" },
                    "duration_beats": { "type": "number", "description": "Length in musical beats" },
                    "name": { "type": "string", "description": "Clip name" }
                })),
                required: Some(vec!["track_id".to_string(), "start_beat".to_string(), "duration_beats".to_string()]),
            },
        },
        Tool {
            name: "timeline_add_automation".to_string(),
            description: "Add parameter automation curve breakpoints along the arrangement timeline.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Track index" },
                    "parameter_name": { "type": "string", "description": "Parameter name (e.g. 'filter_frequency')" },
                    "points": {
                        "type": "array",
                        "description": "List of automation points",
                        "items": {
                            "type": "object",
                            "properties": {
                                "beat": { "type": "number" },
                                "value": { "type": "number" }
                            },
                            "required": ["beat", "value"]
                        }
                    }
                })),
                required: Some(vec!["track_id".to_string(), "parameter_name".to_string(), "points".to_string()]),
            },
        },

        // --- Mixer & FX ---
        Tool {
            name: "mixer_set_channel".to_string(),
            description: "Configure track mixer parameters: volume (dB), pan (-1.0..1.0), mute, and solo.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Track index" },
                    "volume_db": { "type": "number", "description": "Fader level in dB (-60.0 to +6.0)" },
                    "pan": { "type": "number", "description": "Pan [-1.0 (L) to +1.0 (R)]" },
                    "mute": { "type": "boolean", "description": "Mute track" },
                    "solo": { "type": "boolean", "description": "Solo track" }
                })),
                required: Some(vec!["track_id".to_string()]),
            },
        },
        Tool {
            name: "fx_set_saturator".to_string(),
            description: "Configure Padé [4/4] tanh overdrive saturator effect.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "drive_db": { "type": "number", "description": "Drive in dB (0.0 to 48.0)" },
                    "wet": { "type": "number", "description": "Dry/Wet mix [0.0..1.0]" },
                    "output_gain_db": { "type": "number", "description": "Output gain in dB" }
                })),
                required: None,
            },
        },
        Tool {
            name: "fx_set_delay".to_string(),
            description: "Configure tempo-synced ping-pong delay with feedback filter.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "subdivision": { "type": "string", "description": "'1/16', '3/16', '1/8', '3/8', '1/4'" },
                    "feedback": { "type": "number", "description": "Feedback [0.0..0.95]" },
                    "wet": { "type": "number", "description": "Dry/Wet [0.0..1.0]" },
                    "ping_pong": { "type": "boolean", "description": "Enable ping-pong stereo bounce" },
                    "damping": { "type": "number", "description": "High frequency damping [0.0..1.0]" }
                })),
                required: None,
            },
        },
        Tool {
            name: "fx_set_reverb".to_string(),
            description: "Configure algorithmic 8-line Feedback Delay Network (FDN) reverb.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "room_size": { "type": "number", "description": "Room size [0.1..2.0]" },
                    "decay": { "type": "number", "description": "RT60 decay time feedback [0.1..0.98]" },
                    "damping": { "type": "number", "description": "High frequency absorption [0.0..0.95]" },
                    "wet": { "type": "number", "description": "Dry/Wet [0.0..1.0]" }
                })),
                required: None,
            },
        },
        Tool {
            name: "fx_set_limiter".to_string(),
            description: "Configure master 128-sample lookahead brickwall limiter (-0.3 dBFS ceiling).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "ceiling_db": { "type": "number", "description": "Limiter ceiling in dBFS (default -0.3)" }
                })),
                required: None,
            },
        },
        Tool {
            name: "plugin_scan".to_string(),
            description: "Trigger out-of-process AU/VST3 plugin scan with SIGSEGV crash isolation and return discovered manifest.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "output_path": { "type": "string", "description": "Optional path to save JSON manifest" }
                })),
                required: None,
            },
        },
        Tool {
            name: "bounce_wav".to_string(),
            description: "Render multi-track arrangement or stem to high-resolution WAV file and return audio analysis metrics.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "start_beat": { "type": "number", "description": "Timeline start beat for render window (default 0.0)" },
                    "duration_seconds": { "type": "number", "description": "Duration in seconds (e.g. 4.5)" },
                    "output_path": { "type": "string", "description": "Target WAV file path" },
                    "bit_depth": { "type": "integer", "description": "16 or 32 (default 16)" }
                })),
                required: None,
            },
        },
        Tool {
            name: "render_bounce_wav".to_string(),
            description: "Alias for bounce_wav: render arrangement to WAV and return audio metrics.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "start_beat": { "type": "number", "description": "Timeline start beat for render window (default 0.0)" },
                    "duration_seconds": { "type": "number", "description": "Duration in seconds" },
                    "output_path": { "type": "string", "description": "Target WAV file path" },
                    "bit_depth": { "type": "integer", "description": "16 or 32" }
                })),
                required: None,
            },
        },

        // --- High-Level Agentic Music Composition Generators ---
        Tool {
            name: "generate_chord_progression_clip".to_string(),
            description: "Generate harmonic chord progression clip with musical voicing (close, open, drop-2, spread) and velocity humanization.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track index" },
                    "slot_id": { "type": "integer", "description": "Target slot index" },
                    "key": { "type": "string", "description": "Root key (e.g. 'C', 'F#', 'Bb')" },
                    "scale": { "type": "string", "description": "Scale: 'Major', 'Minor', 'Dorian', 'Phrygian', 'Lydian', 'Mixolydian', 'Aeolian', 'Harmonic Minor'" },
                    "progression": { "type": "string", "description": "Progression string, e.g. 'ii-V-I', 'I-V-vi-IV', 'i-VI-III-VII'" },
                    "voicing": { "type": "string", "description": "'close', 'open', 'drop-2', or 'spread'" },
                    "humanize": { "type": "number", "description": "Velocity humanization factor [0.0..1.0]" },
                    "bars": { "type": "number", "description": "Length in bars/beats" },
                    "name": { "type": "string", "description": "Clip name" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string()]),
            },
        },
        Tool {
            name: "generate_bassline_clip".to_string(),
            description: "Generate driving electronic bassline clip following chord roots with rhythm patterns (syncopated, driving_8ths, root_fifth, offbeat_house, dub, acid_16ths).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track index" },
                    "slot_id": { "type": "integer", "description": "Target slot index" },
                    "key": { "type": "string", "description": "Root key (e.g. 'F', 'A', 'C')" },
                    "scale": { "type": "string", "description": "Scale" },
                    "progression": { "type": "string", "description": "Chord progression" },
                    "rhythm_pattern": { "type": "string", "description": "'syncopated', 'driving_8ths', 'root_fifth', 'offbeat_house', 'dub', 'acid_16ths'" },
                    "octave": { "type": "integer", "description": "Bass octave (e.g. 1 or 2)" },
                    "bars": { "type": "integer", "description": "Total bar count override" },
                    "humanize": { "type": "number", "description": "Velocity/microtiming humanization amount [0.0..1.0]" },
                    "name": { "type": "string", "description": "Clip name" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string()]),
            },
        },
        Tool {
            name: "generate_arpeggio_clip".to_string(),
            description: "Generate electronic arpeggiator pattern across chord progression with selectable rate, direction, and octave range.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track index" },
                    "slot_id": { "type": "integer", "description": "Target slot index" },
                    "key": { "type": "string", "description": "Key (e.g. 'A')" },
                    "scale": { "type": "string", "description": "Scale" },
                    "progression": { "type": "string", "description": "Progression (e.g. 'i-VI-iv-V')" },
                    "rate": { "type": "string", "description": "'1/8', '1/16', '1/16T', '1/32'" },
                    "direction": { "type": "string", "description": "'up', 'down', 'up_down', 'random'" },
                    "octaves": { "type": "integer", "description": "Octave span [1..3]" },
                    "gate": { "type": "number", "description": "Gate length ratio [0.1..1.0]" },
                    "bars": { "type": "integer", "description": "Total bar count override" },
                    "name": { "type": "string", "description": "Clip name" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string()]),
            },
        },
        Tool {
            name: "generate_euclidean_drum_clip".to_string(),
            description: "Generate Euclidean mathematical rhythm pattern using Bjorklund's algorithm for any drum pad (e.g. 5 hits in 16 steps).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Track index" },
                    "slot_id": { "type": "integer", "description": "Slot index" },
                    "hits": { "type": "integer", "description": "Number of hits (k) [1..N]" },
                    "steps": { "type": "integer", "description": "Total steps (n) (default 16)" },
                    "pad_index": { "type": "integer", "description": "Target pad (0=Kick, 1=Rim, 2=Snare, 6=Closed HH, 10=Open HH)" },
                    "accent_first": { "type": "boolean", "description": "Accent downbeat hit" },
                    "name": { "type": "string", "description": "Clip name" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "hits".to_string()]),
            },
        },
        Tool {
            name: "generate_drum_groove_clip".to_string(),
            description: "Generate full 16-pad authentic 909 drum groove in chosen genre style ('house_909', 'techno_peak', 'electro_funk', 'breakbeat', 'minimal_ambient').".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track index" },
                    "slot_id": { "type": "integer", "description": "Target slot index" },
                    "style": { "type": "string", "description": "'house_909', 'techno_peak', 'electro_funk', 'breakbeat', 'minimal_ambient'" },
                    "tempo": { "type": "number", "description": "Tempo in BPM" },
                    "swing": { "type": "number", "description": "Swing ratio" },
                    "complexity": { "type": "number", "description": "Groove complexity [0.0..1.0]" },
                    "name": { "type": "string", "description": "Clip name" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string()]),
            },
        },
        Tool {
            name: "build_song_arrangement".to_string(),
            description: "Generate complete multi-track electronic song arrangement with Intro, Verse, Build, Drop, Breakdown, Outro and filter automation.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "genre": { "type": "string", "description": "'techno', 'house', 'electro', 'ambient'" },
                    "key": { "type": "string", "description": "Root key (e.g. 'Fm')" },
                    "bpm": { "type": "number", "description": "Tempo (default 126.0)" },
                    "total_bars": { "type": "integer", "description": "Arrangement length in bars (default 32)" }
                })),
                required: None,
            },
        },

        // --- "Agent Ears" Audio Analysis ---
        Tool {
            name: "analyze_rendered_audio".to_string(),
            description: "Perform perceptual and 4-band spectral audio analysis on exported WAV or live audio buffer (Sub/Bass, Low-Mid, High-Mid, High, RMS, peak, centroid).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "wav_path": { "type": "string", "description": "Optional WAV file path to analyze; if omitted, renders current demo" },
                    "duration_seconds": { "type": "number", "description": "Duration if rendering on the fly (default 4.0)" }
                })),
                required: None,
            },
        },
        Tool {
            name: "preview_synth_patch".to_string(),
            description: "Render 1-2 seconds of audio with a synth patch playing a note, write preview WAV, and return spectral/perceptual metrics for agent evaluation.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "preset_name": { "type": "string", "description": "Preset name (e.g. 'wowbass.preset')" },
                    "parameters": { "type": "object", "description": "Key-value parameter overrides" },
                    "note": { "type": "integer", "description": "MIDI note number (default 48 = C2 or 60 = C3)" },
                    "duration_seconds": { "type": "number", "description": "Duration in seconds (default 1.5)" },
                    "output_wav_path": { "type": "string", "description": "Target WAV file" }
                })),
                required: None,
            },
        },

        // --- Group 2 (Extra): Devices & Chains ---
        Tool {
            name: "device_add_to_track".to_string(),
            description: "Insert an Ableton device into a track's DeviceChain (Wavetable, Operator, Drift, Meld, Analog, Collision, Tension, Electric, Simpler, DrumRack, EQEight, GlueCompressor, Roar, AutoFilter, OTT, HybridReverb, SpectralResonator, Echo, BeatRepeat, Corpus, Redux, Utility, Limiter, Arpeggiator, Chord, Scale, Velocity).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "device_type": { "type": "string", "description": "Device name or type identifier" },
                    "device_name": { "type": "string", "description": "Alias for device_type" },
                    "position": { "type": "integer", "description": "Optional 0-based insertion index" }
                })),
                required: Some(vec!["track_id".to_string()]),
            },
        },
        Tool {
            name: "device_remove_from_track".to_string(),
            description: "Remove a device from a track's DeviceChain by its 0-based index.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "device_index": { "type": "integer", "description": "Device index in chain to delete" }
                })),
                required: Some(vec!["track_id".to_string(), "device_index".to_string()]),
            },
        },
        Tool {
            name: "device_reorder".to_string(),
            description: "Reorder devices within a track's DeviceChain from one index to another.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "from_index": { "type": "integer", "description": "Source index" },
                    "to_index": { "type": "integer", "description": "Destination index" }
                })),
                required: Some(vec!["track_id".to_string(), "from_index".to_string(), "to_index".to_string()]),
            },
        },
        Tool {
            name: "device_set_param".to_string(),
            description: "Set an interactive parameter on a device in a track's DeviceChain.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "device_index": { "type": "integer", "description": "Device index in chain" },
                    "param": { "type": ["string", "integer"], "description": "Parameter name, alias, or 0-based index" },
                    "value": { "type": "number", "description": "Target parameter floating-point value" }
                })),
                required: Some(vec!["track_id".to_string(), "device_index".to_string(), "param".to_string(), "value".to_string()]),
            },
        },
        Tool {
            name: "device_get_chain".to_string(),
            description: "Retrieve complete serialized DeviceChain for a track with device types, categories, latency, and parameters.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" }
                })),
                required: Some(vec!["track_id".to_string()]),
            },
        },

        // --- Group 3 (Extra): Synths, Drums & Simpler ---
        Tool {
            name: "simpler_slice_to_drum_rack".to_string(),
            description: "Slice an audio sample in Simpler to a new 16-pad Drum Rack track based on detected transient markers.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Source track containing Simpler" },
                    "simpler_device_index": { "type": "integer", "description": "Optional device index of Simpler in chain" }
                })),
                required: Some(vec!["track_id".to_string()]),
            },
        },

        // --- Group 4: Browser & Neural Sound Similarity ---
        Tool {
            name: "browser_search".to_string(),
            description: "Search the Live 12 library of sounds, drums, instruments, effects, and presets with tag filtering.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "query": { "type": "string", "description": "Search query substring" },
                    "category": { "type": "string", "description": "Optional category (Sounds, Drums, Instruments, AudioEffects, MidiEffects, PlugIns, Clips, Samples, Grooves)" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Optional tags to filter by" }
                })),
                required: Some(vec!["query".to_string()]),
            },
        },
        Tool {
            name: "browser_find_similar_sounds".to_string(),
            description: "Find sonically similar sounds using 8-dimensional spectral cosine similarity vector search.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "target_id": { "type": "string", "description": "Target browser item ID to compare against" },
                    "top_k": { "type": "integer", "description": "Maximum number of similar matches to return (default 10)" }
                })),
                required: Some(vec!["target_id".to_string()]),
            },
        },

        // --- Group 6: Live 12 MIDI Tools ---
        Tool {
            name: "midi_transform_clip".to_string(),
            description: "Apply Live 12 MIDI transformation to a clip (arpeggiate, connect, ornament, quantize, recombine, span, strum, time_warp).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "slot_id": { "type": "integer", "description": "Target clip slot ID" },
                    "transform": { "type": "string", "description": "Transform algorithm: arpeggiate, connect, ornament, quantize, recombine, span, strum, time_warp" },
                    "params": { "type": "object", "description": "Optional algorithm parameters" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "transform".to_string()]),
            },
        },
        Tool {
            name: "midi_generate_clip".to_string(),
            description: "Generate notes into a clip using Live 12 generative algorithms (seed, shape, stacks, rhythm, euclidean).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "slot_id": { "type": "integer", "description": "Target clip slot ID" },
                    "generator": { "type": "string", "description": "Generator algorithm: seed, shape, stacks, rhythm, euclidean" },
                    "params": { "type": "object", "description": "Optional generator parameters" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "generator".to_string()]),
            },
        },

        // --- Group 7: Audio Clips & Warping ---
        Tool {
            name: "audio_clip_create_warped".to_string(),
            description: "Create an audio clip with non-destructive 6-mode warping enabled (beats, tones, texture, repitch, complex, complex_pro).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "slot_id": { "type": "integer", "description": "Target clip slot ID" },
                    "sample_path": { "type": "string", "description": "WAV file path" },
                    "warp_mode": { "type": "string", "description": "Optional warp mode: beats, tones, texture, repitch, complex, complex_pro" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "sample_path".to_string()]),
            },
        },
        Tool {
            name: "audio_clip_set_warp_mode".to_string(),
            description: "Switch warp mode algorithm and granular grain size on a warped audio clip.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "slot_id": { "type": "integer", "description": "Target clip slot ID" },
                    "warp_mode": { "type": "string", "description": "Target warp mode (beats, tones, texture, repitch, complex, complex_pro)" },
                    "grain_size": { "type": "number", "description": "Optional grain size in milliseconds" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "warp_mode".to_string()]),
            },
        },
        Tool {
            name: "audio_clip_add_warp_marker".to_string(),
            description: "Add a piecewise-linear warp marker anchoring sample time in seconds to musical beat time.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "slot_id": { "type": "integer", "description": "Target clip slot ID" },
                    "sample_time": { "type": "number", "description": "Sample timestamp in seconds" },
                    "beat_time": { "type": "number", "description": "Musical beat timestamp" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "sample_time".to_string(), "beat_time".to_string()]),
            },
        },

        // --- Group 8: Groove Pool & Comping ---
        Tool {
            name: "groove_pool_list".to_string(),
            description: "List all loaded Groove Pool templates (MPC 16th, TR-909 Shuffle, SP-1200 Funk, Latin Triplet) and parameters.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },
        Tool {
            name: "groove_pool_apply_to_clip".to_string(),
            description: "Apply a Groove Pool template non-destructively to a MIDI clip's notes or audio clip's warp markers.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "slot_id": { "type": "integer", "description": "Target clip slot ID" },
                    "groove_name": { "type": "string", "description": "Groove template name" },
                    "amount": { "type": "number", "description": "Optional global groove amount multiplier (0.0 to 1.30)" }
                })),
                required: Some(vec!["track_id".to_string(), "slot_id".to_string(), "groove_name".to_string()]),
            },
        },
        Tool {
            name: "take_lane_create".to_string(),
            description: "Add a new recording/comping take lane to an arrangement track.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "name": { "type": "string", "description": "Optional take lane title" }
                })),
                required: Some(vec!["track_id".to_string()]),
            },
        },
        Tool {
            name: "take_lane_comp_region".to_string(),
            description: "Comp a region from a take lane into the main track lane with automatic 4-10ms equal-power micro-crossfades.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Target track ID" },
                    "take_lane_index": { "type": "integer", "description": "Source take lane index" },
                    "start_beat": { "type": "number", "description": "Start beat of comp region" },
                    "end_beat": { "type": "number", "description": "End beat of comp region" }
                })),
                required: Some(vec!["track_id".to_string(), "take_lane_index".to_string(), "start_beat".to_string(), "end_beat".to_string()]),
            },
        },

        // --- Group 9: Mixer, Returns & Scale ---
        Tool {
            name: "return_track_create".to_string(),
            description: "Create an auxiliary return bus track (e.g. Return A Reverb, Return B Delay) with optional initial effect.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "name": { "type": "string", "description": "Return track title" },
                    "initial_device": { "type": "string", "description": "Optional initial device to insert" }
                })),
                required: Some(vec!["name".to_string()]),
            },
        },
        Tool {
            name: "mixer_set_send".to_string(),
            description: "Set auxiliary send level (dB) from a track into a return track bus.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_id": { "type": "integer", "description": "Source track ID" },
                    "send_index": { "type": "integer", "description": "Send index corresponding to return track" },
                    "level_db": { "type": "number", "description": "Send level in decibels (-70.0 to +6.0 dB)" }
                })),
                required: Some(vec!["track_id".to_string(), "send_index".to_string(), "level_db".to_string()]),
            },
        },
        Tool {
            name: "scale_set_global".to_string(),
            description: "Set the song's global scale and root note in Live 12 with optional scale lock.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "root_note": { "type": "string", "description": "Root pitch class (e.g. 'C', 'F#', or integer 0..11)" },
                    "scale_name": { "type": "string", "description": "Scale mode: Major, Minor, Dorian, Phrygian, Lydian, Mixolydian, Aeolian, Locrian, HarmonicMinor, MelodicMinor, Pentatonic, Blues, Diminished, WholeTone" },
                    "scale": { "type": "string", "description": "Alias for scale_name" },
                    "mode": { "type": "string", "description": "Alias for scale_name" },
                    "scale_lock": { "type": "boolean", "description": "Lock notes to scale" }
                })),
                required: Some(vec!["root_note".to_string()]),
            },
        },

        // --- Group 15: Oxide Studio Live GUI & Audio Engine RPC Bridge (15 tools) ---
        Tool {
            name: "live12_status".to_string(),
            description: "Check TCP connectivity to Oxide Studio Live GUI & Audio Engine on 127.0.0.1:9877.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },
        Tool {
            name: "live12_get_song_info".to_string(),
            description: "Query Oxide Studio Live GUI Song info over TCP (127.0.0.1:9877): tempo, is_playing, song_time, scale, root, track count, return track count, scene count.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },
        Tool {
            name: "live12_create_midi_track".to_string(),
            description: "Create a new MIDI track in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "index": { "type": "integer", "description": "Optional track index" }
                })),
                required: None,
            },
        },
        Tool {
            name: "live12_create_audio_track".to_string(),
            description: "Create a new audio track in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "index": { "type": "integer", "description": "Optional track index" }
                })),
                required: None,
            },
        },
        Tool {
            name: "live12_create_return_track".to_string(),
            description: "Create a new return track in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        },
        Tool {
            name: "live12_load_browser_item".to_string(),
            description: "Load an instrument, sample, or effect from the browser into a track in Oxide Studio Live GUI over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_index": { "type": "integer", "description": "Target track index" },
                    "uri": { "type": "string", "description": "Browser item URI or path" }
                })),
                required: Some(vec!["track_index".to_string(), "uri".to_string()]),
            },
        },
        Tool {
            name: "live12_create_clip".to_string(),
            description: "Create an empty clip in a track clip slot in Oxide Studio Live GUI over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_index": { "type": "integer", "description": "Target track index" },
                    "slot_index": { "type": "integer", "description": "Target clip slot index" },
                    "length_beats": { "type": "number", "description": "Clip duration in quarter-note beats (default 4.0)" }
                })),
                required: Some(vec!["track_index".to_string(), "slot_index".to_string()]),
            },
        },
        Tool {
            name: "live12_add_notes".to_string(),
            description: "Add MIDI notes to an existing clip in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_index": { "type": "integer", "description": "Target track index" },
                    "slot_index": { "type": "integer", "description": "Target clip slot index" },
                    "notes": { "type": "array", "description": "List of note dictionaries [{pitch, start_beat, duration_beats, velocity}]" }
                })),
                required: Some(vec!["track_index".to_string(), "slot_index".to_string(), "notes".to_string()]),
            },
        },
        Tool {
            name: "live12_fire_clip".to_string(),
            description: "Trigger/launch a clip in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_index": { "type": "integer", "description": "Target track index" },
                    "slot_index": { "type": "integer", "description": "Target clip slot index" }
                })),
                required: Some(vec!["track_index".to_string(), "slot_index".to_string()]),
            },
        },
        Tool {
            name: "live12_fire_scene".to_string(),
            description: "Trigger/launch a full scene row in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "scene_index": { "type": "integer", "description": "0-based scene row index" }
                })),
                required: Some(vec!["scene_index".to_string()]),
            },
        },
        Tool {
            name: "live12_set_device_param".to_string(),
            description: "Set a device parameter on a track in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_index": { "type": "integer", "description": "Track index" },
                    "device_index": { "type": "integer", "description": "Device index in track chain" },
                    "param_index": { "type": "integer", "description": "Parameter index in device" },
                    "value": { "type": "number", "description": "Target value" }
                })),
                required: Some(vec!["track_index".to_string(), "device_index".to_string(), "param_index".to_string(), "value".to_string()]),
            },
        },
        Tool {
            name: "live12_get_track_devices".to_string(),
            description: "Enumerate all devices and parameter names/ranges for a track in Oxide Studio Live GUI over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "track_index": { "type": "integer", "description": "Track index" }
                })),
                required: Some(vec!["track_index".to_string()]),
            },
        },
        Tool {
            name: "live12_set_tempo".to_string(),
            description: "Set song tempo (BPM) in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "bpm": { "type": "number", "description": "Tempo in BPM (20..999)" }
                })),
                required: Some(vec!["bpm".to_string()]),
            },
        },
        Tool {
            name: "live12_set_scale".to_string(),
            description: "Set song global scale and root note in Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "root_note": { "type": "integer", "description": "Root note pitch class (0..11)" },
                    "scale_name": { "type": "string", "description": "Scale name (Major, Minor, Dorian, etc.)" }
                })),
                required: Some(vec!["root_note".to_string(), "scale_name".to_string()]),
            },
        },
        Tool {
            name: "live12_exec_lom_query".to_string(),
            description: "Execute arbitrary query or expression against Oxide Studio Live GUI & Audio Engine over TCP (127.0.0.1:9877).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "query": { "type": "string", "description": "Safe query expression with 'song' and 'app' in scope" }
                })),
                required: Some(vec!["query".to_string()]),
            },
        },
    ];

    tools.push(Tool {
        name: "project_import_als".to_string(),
        description: "Import Ableton Live 12 .als project file (gzipped XML or uncompressed) into DAW state.".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(json!({
                "path": { "type": "string", "description": "Path to .als file on disk" }
            })),
            required: Some(vec!["path".to_string()]),
        },
    });
    tools.push(Tool {
        name: "project_export_als".to_string(),
        description: "Export current DAW project as a gzip-compressed Ableton Live 12 .als file.".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(json!({
                "path": { "type": "string", "description": "Target output .als file path" }
            })),
            required: Some(vec!["path".to_string()]),
        },
    });
    tools.push(Tool {
        name: "stems_separate_remix".to_string(),
        description: "Separate a mixed WAV audio file into 4 remix stems (drums, bass, vocals, other).".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(json!({
                "input_wav": { "type": "string", "description": "Input WAV file path" },
                "output_dir": { "type": "string", "description": "Output directory for remix stems" }
            })),
            required: Some(vec!["input_wav".to_string(), "output_dir".to_string()]),
        },
    });
    tools.push(Tool {
        name: "bounce_export_stems".to_string(),
        description: "Render all project tracks and 4 separated remix stems to WAV files in the specified directory.".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Some(json!({
                "output_dir": { "type": "string", "description": "Target output directory" }
            })),
            required: Some(vec!["output_dir".to_string()]),
        },
    });

    if disable_legacy_live12 {
        tools.retain(|t| !t.name.starts_with("live12_"));
    }

    tools
}

/// Dispatch a tool call by name with argument payload.
pub fn dispatch_tool_call(state: &mut DawState, name: &str, args: &Value) -> ToolCallResult {
    call_tool(state, name, args)
}

/// Dispatch a tool call by name with argument payload.
pub fn call_tool(state: &mut DawState, name: &str, args: &Value) -> ToolCallResult {
    match name {
        // --- Project & Transport ---
        "project_new" => {
            let title = args.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled Session");
            state.reset_project(title);
            ToolCallResult::json(&json!({
                "status": "created",
                "project_name": title,
                "tracks_count": state.project.tracks.len(),
                "transport": state.project.transport,
            }))
        }
        "project_load" => {
            let path_str = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ToolCallResult::error("Missing 'path' parameter"),
            };
            let resolved_path = expand_tilde_path(path_str);
            match DAWProject::load_from_file(&resolved_path) {
                Ok(proj) => {
                    let tracks_count = proj.tracks.len();
                    let bpm = proj.transport.bpm;
                    state.bpm = bpm;
                    state.project = proj;
                    state.sync_state_from_project();
                    state.delay.set_tempo(bpm);
                    let resolved_str = resolved_path.to_string_lossy().to_string();
                    state.last_saved_path = Some(resolved_str.clone());
                    ToolCallResult::json(&json!({
                        "status": "loaded",
                        "path": resolved_str,
                        "project_name": state.project.metadata.name,
                        "tracks_count": tracks_count,
                        "bpm": bpm,
                    }))
                }
                Err(e) => ToolCallResult::error(format!("Failed to load project: {}", e)),
            }
        }
        "project_save" => {
            let target_path = args
                .get("path")
                .and_then(|v| v.as_str())
                .map(expand_tilde_path)
                .or_else(|| state.last_saved_path.as_deref().map(expand_tilde_path))
                .unwrap_or_else(|| default_temp_file("oxide_session", "json"));

            state.sync_project_from_state();
            match state.project.save_to_file(&target_path) {
                Ok(_) => {
                    let saved_str = target_path.to_string_lossy().to_string();
                    state.last_saved_path = Some(saved_str.clone());
                    ToolCallResult::json(&json!({
                        "status": "saved",
                        "path": saved_str,
                        "tracks": state.project.tracks.len(),
                        "bpm": state.project.transport.bpm,
                    }))
                }
                Err(e) => ToolCallResult::error(format!("Failed to save project: {}", e)),
            }
        }
        "project_import_als" => {
            let path_str = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ToolCallResult::error("Missing 'path' parameter"),
            };
            let resolved_path = expand_tilde_path(path_str);
            match oxide_project::DAWProject::load_als(&resolved_path) {
                Ok(proj) => {
                    state.bpm = proj.transport.bpm;
                    state.project = proj;
                    state.sync_state_from_project();
                    state.delay.set_tempo(state.bpm);
                    ToolCallResult::json(&json!({
                        "imported": true,
                        "path": resolved_path.to_string_lossy(),
                        "bpm": state.bpm,
                        "tracks": state.project.tracks.len()
                    }))
                }
                Err(e) => ToolCallResult::error(format!("Failed importing .als file: {}", e)),
            }
        }
        "project_export_als" => {
            let path_str = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ToolCallResult::error("Missing 'path' parameter"),
            };
            let resolved_path = expand_tilde_path(path_str);
            state.sync_project_from_state();
            match state.project.save_als(&resolved_path) {
                Ok(()) => ToolCallResult::json(&json!({
                    "exported": true,
                    "path": resolved_path.to_string_lossy(),
                    "tracks": state.project.tracks.len()
                })),
                Err(e) => ToolCallResult::error(format!("Failed exporting .als file: {}", e)),
            }
        }
        "transport_play" => {
            state.is_playing = true;
            let _ = state.lom_client.call("set_transport", json!({ "playing": true }));
            ToolCallResult::json(&json!({
                "is_playing": true,
                "playhead_beat": state.playhead_beat,
            }))
        }
        "transport_stop" => {
            state.is_playing = false;
            state.playhead_beat = 0.0;
            state.synth.all_notes_off();
            let _ = state.lom_client.call("set_transport", json!({ "playing": false }));
            ToolCallResult::json(&json!({
                "is_playing": false,
                "playhead_beat": 0.0,
            }))
        }
        "transport_set_bpm" => {
            let bpm = match args.get("bpm").and_then(|v| v.as_f64()) {
                Some(b) => match validate_bpm(b) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'bpm' parameter"),
            };
            state.bpm = bpm;
            state.project.transport.bpm = bpm;
            state.delay.set_tempo(bpm);
            let _ = state.lom_client.call("set_tempo", json!({ "bpm": bpm }));
            ToolCallResult::json(&json!({
                "bpm": bpm,
            }))
        }
        "transport_set_swing" => {
            let swing = args.get("swing").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            state.swing = swing.clamp(0.0, 0.75);
            state.project.transport.swing_16th_ratio = state.swing;
            ToolCallResult::json(&json!({
                "swing_ratio": state.swing,
            }))
        }
        "transport_get_telemetry" => {
            let tel = state.get_telemetry();
            ToolCallResult::json(&json!({
                "is_playing": state.is_playing,
                "playhead_beat": tel.playhead_beat,
                "cpu_load": tel.cpu_load,
                "master_peak_l": tel.master_peak_l,
                "master_peak_r": tel.master_peak_r,
                "master_rms_l": tel.master_rms_l,
                "master_rms_r": tel.master_rms_r,
                "track_peaks_l": tel.track_peaks_l,
                "track_peaks_r": tel.track_peaks_r,
            }))
        }

        // --- Tracks, Synth & Drums ---
        "track_create_synth" => {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("New Synth");
            let id = state.project.tracks.len() as u32;
            let mut track = TrackDefinition::new_synth(id, name);
            if let Some(preset) = args
                .get("preset")
                .or_else(|| args.get("preset_name"))
                .and_then(|v| v.as_str())
            {
                let mut tmp_synth = oxide_dsp::SynthEngine::new(state.sample_rate);
                let mut loaded = tmp_synth.load_preset(preset);
                if !loaded {
                    let fallback = match preset {
                        "analogsaw.preset" | "analogsaw" => Some("osc-shapes.preset"),
                        "aggro.preset" | "aggro" => Some("distortion.preset"),
                        "metallic.preset" | "metallic" => Some("laser.preset"),
                        "darkpad.preset" | "darkpad" => Some("strings.preset"),
                        _ => None,
                    };
                    if let Some(fb) = fallback {
                        loaded = tmp_synth.load_preset(fb);
                    }
                }
                let mut params = SynthPatchParameters::default();
                if loaded {
                    for (i, desc) in PARAMETER_DESCRIPTORS.iter().enumerate() {
                        params.set_parameter(desc.name, tmp_synth.params.values[i]);
                    }
                }
                params.preset_name = Some(preset.to_string());
                track.instrument = InstrumentConfig::BuiltinSubtractiveSynth(params);
            }
            state.project.tracks.push(track);
            while state.matrix.tracks.len() <= id as usize {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(oxide_timeline::SessionTrack::new(tid, name, 8));
            }
            while state.timeline.tracks.len() <= id as usize {
                let tid = state.timeline.tracks.len() as u32;
                state.timeline.tracks.push(oxide_timeline::ArrangementTrack::new(tid, name, "MidiSynth"));
            }
            let _ = state.ensure_timeline_track_mut(id as usize);
            ToolCallResult::json(&json!({
                "status": "created",
                "track_id": id,
                "name": name,
                "type": "MidiSynth"
            }))
        }
        "track_create_drums" => {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("New Drums");
            let id = state.project.tracks.len() as u32;
            let track = TrackDefinition::new_drum_rack(id, name);
            state.project.tracks.push(track);
            while state.matrix.tracks.len() <= id as usize {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(oxide_timeline::SessionTrack::new(tid, name, 8));
            }
            while state.timeline.tracks.len() <= id as usize {
                let tid = state.timeline.tracks.len() as u32;
                state.timeline.tracks.push(oxide_timeline::ArrangementTrack::new(tid, name, "DrumRack"));
            }
            let _ = state.ensure_timeline_track_mut(id as usize);
            ToolCallResult::json(&json!({
                "status": "created",
                "track_id": id,
                "name": name,
                "type": "DrumRack"
            }))
        }
        "track_delete" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            match state.delete_track_at(track_id) {
                Ok(removed) => ToolCallResult::json(&json!({
                    "status": "deleted",
                    "track_id": track_id,
                    "name": removed.name,
                    "remaining_tracks": state.project.tracks.len()
                })),
                Err(e) => ToolCallResult::error(e),
            }
        }
        "synth_list_presets" => {
            let catalog = match load_preset_catalog() {
                Ok(c) => c,
                Err(e) => return ToolCallResult::error(format!("Failed to load presets: {}", e)),
            };
            let mut preset_names: Vec<String> = catalog.keys().cloned().collect();
            preset_names.sort();
            ToolCallResult::json(&json!({
                "count": preset_names.len(),
                "presets": preset_names
            }))
        }
        "synth_load_preset" => {
            let preset_name = match args.get("preset_name").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ToolCallResult::error("Missing 'preset_name' parameter"),
            };
            let explicit_tid = args.get("track_id").and_then(|v| v.as_u64()).map(|t| t as usize);
            let target_idx = if let Some(idx) = explicit_tid {
                match state.project.tracks.get(idx) {
                    Some(trk) if matches!(trk.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_)) => Some(idx),
                    Some(_) => {
                        return ToolCallResult::error(format!(
                            "Track {} is not a synth track",
                            idx
                        ))
                    }
                    None => {
                        return ToolCallResult::error(format!("Track index {} out of range", idx))
                    }
                }
            } else {
                state
                    .project
                    .tracks
                    .iter()
                    .rposition(|t| matches!(t.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_)))
            };

            let mut candidate_synth = oxide_dsp::SynthEngine::new(state.sample_rate);
            let mut success = candidate_synth.load_preset(preset_name);
            if !success {
                let fallback = match preset_name {
                    "analogsaw.preset" | "analogsaw" => Some("osc-shapes.preset"),
                    "aggro.preset" | "aggro" => Some("distortion.preset"),
                    "metallic.preset" | "metallic" => Some("laser.preset"),
                    "darkpad.preset" | "darkpad" => Some("strings.preset"),
                    _ => None,
                };
                if let Some(fb) = fallback {
                    success = candidate_synth.load_preset(fb);
                }
            }
            if success {
                state.synth = candidate_synth;
                if let Some(idx) = target_idx {
                    state.active_synth_track_id = idx;
                    if let InstrumentConfig::BuiltinSubtractiveSynth(ref mut p) =
                        state.project.tracks[idx].instrument
                    {
                        p.preset_name = Some(preset_name.to_string());
                        p.overridden_params.clear();
                    }
                }
                state.sync_synth_params_to_project();
                ToolCallResult::json(&json!({
                    "status": "loaded",
                    "preset": preset_name,
                    "has_mapping": state.synth.current_mapping.is_some()
                }))
            } else {
                ToolCallResult::error(format!("Preset '{}' not found in catalog", preset_name))
            }
        }
        "synth_set_parameter" => {
            let param_val = match args.get("value").and_then(|v| v.as_f64()) {
                Some(v) => v as f32,
                None => return ToolCallResult::error("Missing 'value' parameter"),
            };
            if !param_val.is_finite() {
                return ToolCallResult::error("Parameter 'value' must be finite");
            }
            let is_norm = args.get("is_normalized").and_then(|v| v.as_bool()).unwrap_or(false);

            let mark_active_synth_override = |st: &mut crate::state::DawState, name: &str| {
                let target_idx = st
                    .project
                    .tracks
                    .get(st.active_synth_track_id)
                    .filter(|t| matches!(t.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_)))
                    .map(|_| st.active_synth_track_id)
                    .or_else(|| {
                        st.project
                            .tracks
                            .iter()
                            .position(|t| matches!(t.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_)))
                    });
                if let Some(idx) = target_idx {
                    if let InstrumentConfig::BuiltinSubtractiveSynth(ref mut p) =
                        st.project.tracks[idx].instrument
                    {
                        p.mark_overridden(name);
                    }
                }
            };

            if let Some(param_str) = args.get("parameter").and_then(|v| v.as_str()) {
                if !oxide_dsp::PARAMETER_DESCRIPTORS.iter().any(|d| d.name == param_str) {
                    return ToolCallResult::error(format!("Unknown synth parameter: '{}'", param_str));
                }
                if is_norm {
                    state.synth.set_parameter_normalized(param_str, param_val);
                } else {
                    state.synth.set_parameter(param_str, param_val);
                }
                mark_active_synth_override(state, param_str);
                state.sync_synth_params_to_project();
                ToolCallResult::json(&json!({
                    "parameter": param_str,
                    "value": state.synth.params.get_value(param_str),
                    "normalized": state.synth.params.get_control_value(param_str)
                }))
            } else if let Some(param_idx) = args.get("parameter").and_then(|v| v.as_u64()) {
                if param_idx < 30 {
                    let desc = &oxide_dsp::PARAMETER_DESCRIPTORS[param_idx as usize];
                    if is_norm {
                        state.synth.set_parameter_normalized(desc.name, param_val);
                    } else {
                        state.synth.set_parameter(desc.name, param_val);
                    }
                    mark_active_synth_override(state, desc.name);
                    state.sync_synth_params_to_project();
                    ToolCallResult::json(&json!({
                        "parameter_id": param_idx,
                        "parameter_name": desc.name,
                        "value": state.synth.params.get_value(desc.name)
                    }))
                } else {
                    ToolCallResult::error(format!("ParamId {} out of range [0..29]", param_idx))
                }
            } else {
                ToolCallResult::error("Parameter name or index required")
            }
        }
        "synth_set_macro" => {
            if let Some(x) = args.get("x").and_then(|v| v.as_f64()) {
                state.synth.set_macro_x(x as f32);
            }
            if let Some(y) = args.get("y").and_then(|v| v.as_f64()) {
                state.synth.set_macro_y(y as f32);
            }
            if let Some(map_name) = args.get("mapping_name").and_then(|v| v.as_str()) {
                if let Ok(mappings) = load_mapping_catalog() {
                    let key = if map_name.ends_with(".mapping") {
                        map_name.to_string()
                    } else {
                        format!("{}.mapping", map_name)
                    };
                    if let Some(m) = mappings.get(&key) {
                        state.synth.current_mapping = Some(m.clone());
                        state.synth.set_macro_x(state.synth.macro_x);
                        state.synth.set_macro_y(state.synth.macro_y);
                    }
                }
            }
            state.sync_synth_params_to_project();
            ToolCallResult::json(&json!({
                "macro_x": state.synth.macro_x,
                "macro_y": state.synth.macro_y,
                "has_mapping": state.synth.current_mapping.is_some()
            }))
        }
        "drums_configure_pad" => {
            let pad_idx = match args.get("pad_index").and_then(|v| v.as_u64()) {
                Some(idx) if idx < 16 => idx as usize,
                _ => return ToolCallResult::error("Valid 'pad_index' [0..15] is required"),
            };

            if let Some(pitch) = args.get("pitch_semitones").and_then(|v| v.as_f64()) {
                state.drum_rack.pads[pad_idx].pitch_semitones = pitch as f32;
            }
            if let Some(decay) = args.get("decay_seconds").and_then(|v| v.as_f64()) {
                state.drum_rack.pads[pad_idx].decay_seconds = (decay as f32).max(0.005);
            }
            if let Some(vol) = args.get("volume").and_then(|v| v.as_f64()) {
                state.drum_rack.pads[pad_idx].volume = vol as f32;
            }
            if let Some(pan) = args.get("pan").and_then(|v| v.as_f64()) {
                state.drum_rack.pads[pad_idx].pan = pan as f32;
            }
            if let Some(choke) = args.get("choke_group").and_then(|v| v.as_u64()) {
                state.drum_rack.pads[pad_idx].choke_group = Some(choke as u8);
            }
            if let Some(mute) = args.get("mute").and_then(|v| v.as_bool()) {
                state.drum_rack.pads[pad_idx].mute = mute;
            }
            if let Some(solo) = args.get("solo").and_then(|v| v.as_bool()) {
                state.drum_rack.pads[pad_idx].solo = solo;
            }
            if let Some(wav_path) = args.get("wav_path").and_then(|v| v.as_str()) {
                let resolved_wav = expand_tilde_path(wav_path);
                if let Err(e) = state.drum_rack.load_sample_pad(pad_idx, &resolved_wav) {
                    return ToolCallResult::error(format!("Failed to load WAV into pad {}: {}", pad_idx, e));
                }
                if let Some(trk) = state.project.tracks.get_mut(state.active_drum_track_id) {
                    if let InstrumentConfig::ElectronicDrumRack(ref mut dr_def) = trk.instrument {
                        if let Some(pad_def) = dr_def.pads.get_mut(pad_idx) {
                            pad_def.sample_path = resolved_wav.to_string_lossy().to_string();
                        }
                    }
                }
            }

            state.sync_drum_rack_to_project();
            let pad = &state.drum_rack.pads[pad_idx];
            ToolCallResult::json(&json!({
                "pad_index": pad_idx,
                "name": pad.name,
                "pitch_semitones": pad.pitch_semitones,
                "decay_seconds": pad.decay_seconds,
                "volume": pad.volume,
                "pan": pad.pan,
                "choke_group": pad.choke_group,
                "mute": pad.mute,
                "solo": pad.solo
            }))
        }
        "drums_trigger_pad" => {
            let pad_idx = match args.get("pad_index").and_then(|v| v.as_u64()) {
                Some(idx) if idx < 16 => idx as u8,
                _ => return ToolCallResult::error("Valid 'pad_index' [0..15] is required"),
            };
            let vel = args.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
            state.drum_rack.trigger_pad(pad_idx, vel);
            ToolCallResult::json(&json!({
                "triggered_pad": pad_idx,
                "velocity": vel
            }))
        }

        // --- Session Matrix & Timeline ---
        "clip_create_midi" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("MIDI Clip");
            let len_beats = args.get("length_beats").and_then(|v| v.as_f64()).unwrap_or(4.0);
            if !len_beats.is_finite() || len_beats <= 0.0 || len_beats > (MAX_TOTAL_BARS * 4) as f64 {
                return ToolCallResult::error("length_beats must be finite and in (0.0..=2048.0]");
            }

            let mut notes = Vec::new();
            if let Some(arr) = args.get("notes").and_then(|v| v.as_array()) {
                for item in arr {
                    let pitch = item.get("pitch").and_then(|v| v.as_u64()).unwrap_or(60).min(127) as u8;
                    let vel = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100).clamp(1, 127) as u8;
                    let start = item.get("start_beat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let dur = item.get("duration_beats").and_then(|v| v.as_f64()).unwrap_or(1.0);
                    if !start.is_finite() || start < 0.0 || !dur.is_finite() || dur <= 0.0 {
                        return ToolCallResult::error("MIDI note start_beat must be >= 0.0 and duration_beats must be > 0.0");
                    }
                    notes.push(MidiNote::new(pitch, vel, start, dur));
                }
            }

            let pattern = ClipPattern::Midi(MidiPattern {
                length_beats: len_beats,
                notes: notes.clone(),
            });
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, name, pattern);

            state.set_session_slot_clip(track_id, slot_id, clip);

            let _ = state.lom_client.call("create_clip", json!({
                "track_index": track_id,
                "slot_index": slot_id,
                "length_beats": len_beats,
            }));
            if !notes.is_empty() {
                let notes_json: Vec<Value> = notes.iter().map(|n| json!({
                    "pitch": n.pitch,
                    "velocity": n.velocity,
                    "start_beat": n.start_beat,
                    "duration_beats": n.duration_beats,
                })).collect();
                let _ = state.lom_client.call("add_notes", json!({
                    "track_index": track_id,
                    "slot_index": slot_id,
                    "notes": notes_json,
                }));
            }

            ToolCallResult::json(&json!({
                "status": "created",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_name": name,
                "length_beats": len_beats
            }))
        }
        "clip_create_drums" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Drum Pattern");
            let step_count = args.get("step_count").and_then(|v| v.as_u64()).unwrap_or(16) as usize;

            let mut drum_pat = if step_count == 32 {
                DrumStepPattern::new_32_step()
            } else {
                DrumStepPattern::new_16_step()
            };

            if let Some(arr) = args.get("steps").and_then(|v| v.as_array()) {
                for item in arr {
                    let pad = item.get("pad_index").and_then(|v| v.as_u64()).unwrap_or(0).min(15) as u8;
                    let step = item.get("step_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let vel = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100).clamp(1, 127) as u8;
                    let acc = item.get("accent").and_then(|v| v.as_bool()).unwrap_or(false);
                    drum_pat.set_step(pad, step, DrumStep::new(true, vel, 36 + pad).with_accent(acc));
                }
            }

            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, name, ClipPattern::Drum(drum_pat));

            state.set_session_slot_clip(track_id, slot_id, clip);

            ToolCallResult::json(&json!({
                "status": "created",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_name": name,
                "step_count": step_count
            }))
        }
        "clip_create_lilypond" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let notation = match args.get("notation").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => return ToolCallResult::error("Missing 'notation'"),
            };
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("LilyPond Clip");

            let parsed = parse_lilypond(notation);
            let pattern = sequence_to_clip_pattern(&parsed);
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, name, pattern);

            state.set_session_slot_clip(track_id, slot_id, clip);

            ToolCallResult::json(&json!({
                "status": "created",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_name": name,
                "parsed_tempo": parsed.tempo,
                "voices": parsed.voices.len()
            }))
        }
        "clip_launch" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let q_mode = match args.get("quantize_mode").and_then(|v| v.as_str()) {
                Some("immediate") => QuantizeMode::Immediate,
                Some("1_beat") => QuantizeMode::Quarter,
                _ => QuantizeMode::OneBar,
            };

            if let Some(track) = state.matrix.tracks.get_mut(track_id) {
                let target_beat = track.trigger_slot(slot_id, state.playhead_beat, q_mode);
                ToolCallResult::json(&json!({
                    "status": "launched",
                    "track_id": track_id,
                    "slot_id": slot_id,
                    "target_beat": target_beat
                }))
            } else {
                ToolCallResult::error(format!("Track index {} not found", track_id))
            }
        }
        "clip_stop" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            if let Some(track) = state.matrix.tracks.get_mut(track_id) {
                let target_beat = track.stop(state.playhead_beat, QuantizeMode::OneBar);
                ToolCallResult::json(&json!({
                    "status": "stopped",
                    "track_id": track_id,
                    "target_beat": target_beat
                }))
            } else {
                ToolCallResult::error(format!("Track index {} not found", track_id))
            }
        }
        "scene_launch" => {
            let scene_idx = match args.get("scene_index").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'scene_index'"),
            };
            let launched_count = state.matrix.trigger_scene(scene_idx, state.playhead_beat, QuantizeMode::OneBar);
            ToolCallResult::json(&json!({
                "status": "scene_launched",
                "scene_index": scene_idx,
                "tracks_triggered": launched_count
            }))
        }
        "timeline_place_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let clip_id = args.get("clip_id").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => 0,
            };
            let start = match args.get("start_beat").and_then(|v| v.as_f64()) {
                Some(b) if b.is_finite() && b >= 0.0 => b,
                Some(_) => return ToolCallResult::error("start_beat must be finite and >= 0.0"),
                None => return ToolCallResult::error("Missing 'start_beat'"),
            };
            let dur = match args.get("duration_beats").and_then(|v| v.as_f64()) {
                Some(d) if d.is_finite() && d > 0.0 => d,
                Some(_) => return ToolCallResult::error("duration_beats must be finite and > 0.0"),
                None => return ToolCallResult::error("Missing 'duration_beats'"),
            };
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Arrangement Clip");

            let pattern = if let Some(m_trk) = state.matrix.tracks.get(track_id) {
                m_trk
                    .slots
                    .get(slot_id)
                    .and_then(|s| s.clip.as_ref().map(|c| c.pattern.clone()))
                    .or_else(|| {
                        m_trk
                            .slots
                            .iter()
                            .find_map(|s| s.clip.as_ref().map(|c| c.pattern.clone()))
                    })
            } else {
                None
            }
            .or_else(|| {
                state
                    .audio_clips
                    .get(&(track_id as u32, slot_id as u32))
                    .map(|w| {
                        ClipPattern::Audio(AudioClipData {
                            file_path: w.name.clone(),
                            sample_rate: w.sample_rate,
                            channels: w.channels,
                            length_beats: w.length_beats,
                            gain_db: 0.0,
                            warp_mode: format!("{:?}", w.warper.config.mode),
                            samples: w.raw_samples.clone(),
                        })
                    })
            })
            .or_else(|| {
                state
                    .project
                    .tracks
                    .get(track_id)
                    .and_then(|t| t.session_clips.get(slot_id).and_then(|c| c.as_ref().map(|sc| sc.pattern.clone())))
            })
            .unwrap_or_else(|| ClipPattern::Midi(MidiPattern::new(dur)));

            let clip = ArrangementClip::new(clip_id, name, start, dur, pattern);

            while state.timeline.tracks.len() <= track_id {
                let tid = state.timeline.tracks.len() as u32;
                state.timeline.tracks.push(oxide_timeline::ArrangementTrack::new(tid, format!("Track {}", tid), "MidiSynth"));
            }
            state.timeline.tracks[track_id].add_clip(clip);

            ToolCallResult::json(&json!({
                "status": "placed",
                "track_id": track_id,
                "clip_id": clip_id,
                "start_beat": start,
                "duration_beats": dur
            }))
        }
        "timeline_add_automation" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let param_name = match args.get("parameter_name").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ToolCallResult::error("Missing 'parameter_name'"),
            };

            let mut lane = AutomationLane::new(1, param_name);
            if let Some(arr) = args.get("points").and_then(|v| v.as_array()) {
                for pt in arr {
                    let beat = pt.get("beat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let val = pt.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    if !beat.is_finite() || !val.is_finite() {
                        return ToolCallResult::error("Automation point beat and value must be finite");
                    }
                    lane.add_point(AutomationPoint::linear(beat.max(0.0), val));
                }
            }

            while state.timeline.tracks.len() <= track_id {
                let tid = state.timeline.tracks.len() as u32;
                state.timeline.tracks.push(oxide_timeline::ArrangementTrack::new(tid, format!("Track {}", tid), "MidiSynth"));
            }
            state.timeline.tracks[track_id].automation_lanes.push(lane);

            ToolCallResult::json(&json!({
                "status": "automation_added",
                "track_id": track_id,
                "parameter_name": param_name
            }))
        }

        // --- Mixer & FX ---
        "mixer_set_channel" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            if track_id >= state.mixer.tracks.len() && track_id >= state.project.tracks.len() {
                return ToolCallResult::error(format!("Track {} out of mixer range", track_id));
            }
            let mut vol_out = 0.0f32;
            let mut pan_out = 0.0f32;
            let mut mute_out = false;
            let mut solo_out = false;

            if track_id < state.mixer.tracks.len() {
                let trk = &mut state.mixer.tracks[track_id];
                if let Some(vol) = args.get("volume_db").and_then(|v| v.as_f64()) {
                    trk.set_volume_db(vol as f32);
                }
                if let Some(pan) = args.get("pan").and_then(|v| v.as_f64()) {
                    trk.set_pan(pan as f32);
                }
                if let Some(mute) = args.get("mute").and_then(|v| v.as_bool()) {
                    trk.mute = mute;
                }
                if let Some(solo) = args.get("solo").and_then(|v| v.as_bool()) {
                    trk.solo = solo;
                }
                vol_out = trk.volume_db;
                pan_out = trk.pan;
                mute_out = trk.mute;
                solo_out = trk.solo;
            }
            if let Some(p_trk) = state.project.tracks.get_mut(track_id) {
                if let Some(vol) = args.get("volume_db").and_then(|v| v.as_f64()) {
                    p_trk.mixer.volume_db = vol as f32;
                }
                if let Some(pan) = args.get("pan").and_then(|v| v.as_f64()) {
                    p_trk.mixer.pan = pan as f32;
                }
                if let Some(mute) = args.get("mute").and_then(|v| v.as_bool()) {
                    p_trk.mixer.mute = mute;
                }
                if let Some(solo) = args.get("solo").and_then(|v| v.as_bool()) {
                    p_trk.mixer.solo = solo;
                }
                vol_out = p_trk.mixer.volume_db;
                pan_out = p_trk.mixer.pan;
                mute_out = p_trk.mixer.mute;
                solo_out = p_trk.mixer.solo;
            }
            ToolCallResult::json(&json!({
                "track_id": track_id,
                "volume_db": vol_out,
                "pan": pan_out,
                "mute": mute_out,
                "solo": solo_out
            }))
        }
        "fx_set_saturator" => {
            if let Some(drive) = args.get("drive_db").and_then(|v| v.as_f64()) {
                state.saturator.set_drive_db(drive as f32);
            }
            if let Some(wet) = args.get("wet").and_then(|v| v.as_f64()) {
                state.saturator.wet = (wet as f32).clamp(0.0, 1.0);
            }
            if let Some(gain) = args.get("output_gain_db").and_then(|v| v.as_f64()) {
                state.saturator.set_output_gain_db(gain as f32);
            }
            state.master_saturator_active = true;
            ToolCallResult::json(&json!({
                "drive_linear": state.saturator.drive_linear,
                "wet": state.saturator.wet,
                "output_gain": state.saturator.output_gain
            }))
        }
        "fx_set_delay" => {
            if let Some(sub) = args.get("subdivision").and_then(|v| v.as_str()) {
                let s = match sub {
                    "1/16" => oxide_fx::DelaySubdivision::Sixteenth,
                    "3/16" => oxide_fx::DelaySubdivision::SixteenthDotted,
                    "1/8" => oxide_fx::DelaySubdivision::Eighth,
                    "3/8" => oxide_fx::DelaySubdivision::EighthDotted,
                    "1/4" => oxide_fx::DelaySubdivision::Quarter,
                    _ => oxide_fx::DelaySubdivision::EighthDotted,
                };
                state.delay.set_subdivision(s);
            }
            if let Some(fb) = args.get("feedback").and_then(|v| v.as_f64()) {
                state.delay.feedback = (fb as f32).clamp(0.0, 0.98);
            }
            if let Some(wet) = args.get("wet").and_then(|v| v.as_f64()) {
                state.delay.wet = (wet as f32).clamp(0.0, 1.0);
            }
            if let Some(pp) = args.get("ping_pong").and_then(|v| v.as_bool()) {
                state.delay.ping_pong = pp;
            }
            if let Some(damp) = args.get("damping").and_then(|v| v.as_f64()) {
                state.delay.damping = (damp as f32).clamp(0.0, 1.0);
            }
            state.master_delay_active = state.delay.wet > 0.0;
            ToolCallResult::json(&json!({
                "subdivision_beats": state.delay.subdivision.beats(),
                "feedback": state.delay.feedback,
                "wet": state.delay.wet,
                "ping_pong": state.delay.ping_pong,
                "damping": state.delay.damping
            }))
        }
        "fx_set_reverb" => {
            if let Some(sz) = args.get("room_size").and_then(|v| v.as_f64()) {
                state.reverb.room_size = (sz as f32).clamp(0.1, 2.0);
            }
            if let Some(dec) = args.get("decay").and_then(|v| v.as_f64()) {
                state.reverb.decay = (dec as f32).clamp(0.1, 0.98);
            }
            if let Some(damp) = args.get("damping").and_then(|v| v.as_f64()) {
                state.reverb.damping = (damp as f32).clamp(0.0, 0.95);
            }
            if let Some(wet) = args.get("wet").and_then(|v| v.as_f64()) {
                state.reverb.wet = (wet as f32).clamp(0.0, 1.0);
            }
            state.master_reverb_active = state.reverb.wet > 0.0;
            ToolCallResult::json(&json!({
                "room_size": state.reverb.room_size,
                "decay": state.reverb.decay,
                "damping": state.reverb.damping,
                "wet": state.reverb.wet
            }))
        }
        "fx_set_limiter" => {
            if let Some(ceil) = args.get("ceiling_db").and_then(|v| v.as_f64()) {
                let lin = 10.0_f32.powf((ceil as f32).clamp(-12.0, 0.0) / 20.0);
                state.limiter.ceiling = lin;
            }
            ToolCallResult::json(&json!({
                "ceiling_linear": state.limiter.ceiling,
                "ceiling_dbfs": 20.0 * state.limiter.ceiling.log10()
            }))
        }
        "plugin_scan" => {
            let mut supervisor = PluginScannerSupervisor::new();
            supervisor.manifest_cache_path = None;
            let manifest = supervisor.scan_all();
            if let Some(out_path) = args.get("output_path").and_then(|v| v.as_str()) {
                let resolved = expand_tilde_path(out_path);
                if let Err(e) = manifest.save_to_file(&resolved) {
                    return ToolCallResult::error(format!(
                        "Failed saving plugin scan manifest to {}: {}",
                        resolved.display(),
                        e
                    ));
                }
            }
            ToolCallResult::json(&json!({
                "total_scanned": manifest.plugins.len(),
                "valid_count": manifest.plugins.iter().filter(|p| p.is_valid).count(),
                "quarantined_count": manifest.plugins.iter().filter(|p| p.quarantined).count(),
                "plugins": manifest.plugins
            }))
        }
        "bounce_wav" | "render_bounce_wav" => {
            let dur = args.get("duration_seconds").and_then(|v| v.as_f64()).unwrap_or(4.5);
            if !dur.is_finite() || dur <= 0.0 || dur > 3600.0 {
                return ToolCallResult::error(format!(
                    "duration_seconds {} out of valid range (0.0..=3600.0]",
                    dur
                ));
            }
            let dur = dur.clamp(1.0, 3600.0);
            let start_beat = args.get("start_beat").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if !start_beat.is_finite() || start_beat < 0.0 {
                return ToolCallResult::error(format!("Invalid start_beat: {}", start_beat));
            }
            let out_path = args
                .get("output_path")
                .and_then(|v| v.as_str())
                .map(expand_tilde_path)
                .unwrap_or_else(|| default_temp_file("oxide_bounce", "wav"));
            let out_path_str = out_path.to_string_lossy().to_string();
            let bit_depth = args.get("bit_depth").and_then(|v| v.as_u64()).unwrap_or(16) as u16;

            state.sync_project_from_state();

            let has_real_clips = state.project.tracks.iter().any(|t| {
                !t.arrangement_clips.is_empty()
                    || t.session_clips.iter().any(|c| c.is_some())
            });
            let use_project_renderer = has_real_clips
                || state.last_saved_path.is_some()
                || state.project.metadata.name != "Ableton MCP Session"
                || state.project.tracks.len() != 2;

            if let Some(parent) = out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let interleaved = if use_project_renderer {
                match render_project_window_to_wav(&state.project, &out_path, start_beat, dur, bit_depth) {
                    Ok(samples) => samples,
                    Err(e) => return ToolCallResult::error(format!("Failed rendering project WAV: {}", e)),
                }
            } else {
                let renderer = DemoArrangementRenderer::new(state.sample_rate, state.bpm.max(60.0));
                let samples = renderer.render_demo_window(start_beat, dur);
                let write_res = if bit_depth == 32 {
                    WavWriter::write_wav_32bit_float(&out_path, state.sample_rate as u32, 2, &samples)
                } else {
                    WavWriter::write_wav_16bit(&out_path, state.sample_rate as u32, 2, &samples)
                };
                if let Err(e) = write_res {
                    return ToolCallResult::error(format!("Failed to write WAV: {}", e));
                }
                samples
            };

            let metrics = AudioAnalysisMetrics::analyze_interleaved(&interleaved, state.sample_rate as u32, 2);
            ToolCallResult::json(&json!({
                "status": "rendered",
                "output_path": out_path_str,
                "start_beat": start_beat,
                "duration_seconds": metrics.duration_seconds,
                "peak_dbfs": metrics.peak_dbfs,
                "rms_dbfs": metrics.rms_dbfs,
                "spectral_centroid_hz": metrics.spectral_centroid_hz,
                "spectral_bands": metrics.spectral_bands
            }))
        }
        "stems_separate_remix" => {
            let input_wav = match args.get("input_wav").and_then(|v| v.as_str()) {
                Some(p) => expand_tilde_path(p),
                None => return ToolCallResult::error("Missing 'input_wav' parameter"),
            };
            let output_dir = match args.get("output_dir").and_then(|v| v.as_str()) {
                Some(p) => expand_tilde_path(p),
                None => return ToolCallResult::error("Missing 'output_dir' parameter"),
            };
            match oxide_bounce::stems::export_remix_stems_from_wav(&input_wav, &output_dir) {
                Ok(files) => ToolCallResult::json(&json!({
                    "drums": files.drums.to_string_lossy(),
                    "bass": files.bass.to_string_lossy(),
                    "vocals": files.vocals.to_string_lossy(),
                    "other": files.other.to_string_lossy()
                })),
                Err(e) => ToolCallResult::error(format!("Failed separating remix stems: {}", e)),
            }
        }
        "bounce_export_stems" => {
            let output_dir = match args.get("output_dir").and_then(|v| v.as_str()) {
                Some(p) => expand_tilde_path(p),
                None => return ToolCallResult::error("Missing 'output_dir' parameter"),
            };
            state.sync_project_from_state();
            match oxide_bounce::stems::export_all_tracks_as_stems(&state.project, &output_dir) {
                Ok(paths) => {
                    let files: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                    ToolCallResult::json(&json!({
                        "stems": files,
                        "files": files,
                        "count": files.len()
                    }))
                }
                Err(e) => ToolCallResult::error(format!("Failed exporting track stems: {}", e)),
            }
        }

        // --- High-Level Agentic Music Composition Generators ---
        "generate_chord_progression_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let key_str = args.get("key").and_then(|v| v.as_str()).unwrap_or("C");
            let scale_str = args.get("scale").and_then(|v| v.as_str()).unwrap_or("Minor");
            let prog_str = args.get("progression").and_then(|v| v.as_str()).unwrap_or("i-VI-III-VII");
            let voicing = args.get("voicing").and_then(|v| v.as_str()).unwrap_or("close");
            let bars_override = match args.get("bars") {
                Some(v) => {
                    let b = match v.as_f64() {
                        Some(val) if val.is_finite() && val > 0.0 && val <= MAX_TOTAL_BARS as f64 => val,
                        _ => {
                            return ToolCallResult::error(format!(
                                "bars out of valid range (0.0..={}]",
                                MAX_TOTAL_BARS
                            ))
                        }
                    };
                    Some(b)
                }
                None => None,
            };
            let humanize_raw = args.get("humanize").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if !humanize_raw.is_finite() {
                return ToolCallResult::error("Invalid non-finite 'humanize' parameter");
            }
            let humanize = humanize_raw.clamp(0.0, 1.0);
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Chords");

            let root_pc = parse_pitch_class(key_str).unwrap_or(0);
            let intervals = get_scale_intervals(scale_str);

            let tokens: Vec<&str> = prog_str.split(&['-', ' ', ','][..]).filter(|s| !s.is_empty()).collect();
            let chord_count = tokens.len().max(1);
            let total_beats = bars_override
                .map(|b| (b * 4.0).max(0.25))
                .unwrap_or((chord_count as f64) * 4.0);
            let beats_per_chord = total_beats / (chord_count as f64);

            let mut notes = Vec::new();
            for (i, token) in tokens.iter().enumerate() {
                let start_beat = i as f64 * beats_per_chord;
                let step = parse_roman_degree(token).map(|(s, _)| s).unwrap_or(i % intervals.len());
                let pitches = build_chord_notes(root_pc, intervals, step, voicing, 3);
                for (note_idx, p) in pitches.into_iter().enumerate() {
                    let vel_jitter = if humanize > 0.0 {
                        let phase = ((i * 19 + note_idx * 31) % 25) as i32 - 12;
                        (phase as f64 * humanize).round() as i32
                    } else {
                        0
                    };
                    let vel = (100 + vel_jitter).clamp(1, 127) as u8;
                    let t_offset = if humanize > 0.0 {
                        let phase = ((i * 13 + note_idx * 23) % 100) as f64 / 100.0 - 0.5;
                        phase * 0.025 * humanize
                    } else {
                        0.0
                    };
                    let t = (start_beat + t_offset).clamp(0.0, (total_beats - 0.05).max(0.0));
                    let dur = (beats_per_chord * 0.95).min((total_beats - t).max(0.05));
                    notes.push(MidiNote::new(p, vel, t, dur));
                }
            }

            let pattern = ClipPattern::Midi(MidiPattern {
                length_beats: total_beats,
                notes: notes.clone(),
            });
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, pattern);

            state.set_session_slot_clip(track_id, slot_id, clip);

            let _ = state.lom_client.call("create_clip", json!({
                "track_index": track_id,
                "slot_index": slot_id,
                "length_beats": total_beats,
            }));
            let chord_notes_json: Vec<Value> = notes.iter().map(|n| json!({
                "pitch": n.pitch,
                "velocity": n.velocity,
                "start_beat": n.start_beat,
                "duration_beats": n.duration_beats,
            })).collect();
            let _ = state.lom_client.call("add_notes", json!({
                "track_index": track_id,
                "slot_index": slot_id,
                "notes": chord_notes_json,
            }));

            ToolCallResult::json(&json!({
                "status": "generated",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_name": clip_name,
                "total_beats": total_beats,
                "bars": total_beats / 4.0,
                "humanize": humanize,
                "chord_count": chord_count,
                "note_events": notes.len()
            }))
        }
        "generate_bassline_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let key_str = args.get("key").and_then(|v| v.as_str()).unwrap_or("F");
            let scale_str = args.get("scale").and_then(|v| v.as_str()).unwrap_or("Minor");
            let prog_str = args.get("progression").and_then(|v| v.as_str()).unwrap_or("i-i-VI-VII");
            let rhythm = args.get("rhythm_pattern").and_then(|v| v.as_str()).unwrap_or("driving_8ths");
            let octave = args.get("octave").and_then(|v| v.as_i64()).unwrap_or(2).clamp(0, 9);
            let bars_override = match args.get("bars") {
                Some(v) => match v.as_u64() {
                    Some(b) if b >= 1 && (b as usize) <= MAX_TOTAL_BARS => Some(b as usize),
                    _ => {
                        return ToolCallResult::error(format!(
                            "bars out of valid range [1..={}]",
                            MAX_TOTAL_BARS
                        ))
                    }
                },
                None => None,
            };
            let humanize_raw = args.get("humanize").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if !humanize_raw.is_finite() {
                return ToolCallResult::error("Invalid non-finite 'humanize' parameter");
            }
            let humanize = humanize_raw.clamp(0.0, 1.0);
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Bassline");

            let root_pc = parse_pitch_class(key_str).unwrap_or(5);
            let intervals = get_scale_intervals(scale_str);

            let tokens: Vec<&str> = prog_str.split(&['-', ' ', ','][..]).filter(|s| !s.is_empty()).collect();
            let prog_len = tokens.len().max(1);
            let total_bars = bars_override.unwrap_or(prog_len);
            let beats_per_bar = 4.0;
            let total_beats = (total_bars as f64) * beats_per_bar;

            let mut notes = Vec::new();
            for bar_idx in 0..total_bars {
                let bar_start = bar_idx as f64 * beats_per_bar;
                let token = tokens.get(bar_idx % prog_len).copied().unwrap_or("i");
                let step = parse_roman_degree(token).map(|(s, _)| s).unwrap_or(0);
                let root_midi = ((octave as i32 + 1) * 12) + root_pc as i32 + intervals[step % intervals.len()] as i32;
                let root_note = root_midi.clamp(0, 127) as u8;
                let fifth_note = (root_midi + 7).clamp(0, 127) as u8;
                let oct_up_note = (root_midi + 12).clamp(0, 127) as u8;

                let mut push_note = |pitch: u8, vel: u8, start_offset: f64, dur: f64, idx_seed: usize| {
                    let h_offset = if humanize > 0.0 {
                        let phase = ((bar_idx * 17 + idx_seed * 31) % 100) as f64 / 100.0 - 0.5;
                        phase * 0.03 * humanize
                    } else {
                        0.0
                    };
                    let vel_jitter = if humanize > 0.0 {
                        let phase = ((bar_idx * 13 + idx_seed * 19) % 21) as i32 - 10;
                        (phase as f64 * humanize).round() as i32
                    } else {
                        0
                    };
                    let v = (vel as i32 + vel_jitter).clamp(1, 127) as u8;
                    let t = (bar_start + start_offset + h_offset).max(0.0);
                    notes.push(MidiNote::new(pitch, v, t, dur));
                };

                match rhythm {
                    "offbeat_house" => {
                        for (i, &b) in [0.5, 1.5, 2.5, 3.5].iter().enumerate() {
                            push_note(root_note, 110, b, 0.45, i);
                        }
                    }
                    "syncopated" => {
                        for (i, &b) in [0.0, 0.75, 1.5, 2.25, 3.0].iter().enumerate() {
                            push_note(root_note, 105, b, 0.6, i);
                        }
                    }
                    "root_fifth" => {
                        let pattern = [
                            (root_note, 115, 0.0, 0.85),
                            (fifth_note, 100, 1.0, 0.85),
                            (root_note, 110, 2.0, 0.85),
                            (fifth_note, 98, 3.0, 0.45),
                            (oct_up_note, 104, 3.5, 0.45),
                        ];
                        for (i, &(p, v, off, d)) in pattern.iter().enumerate() {
                            push_note(p, v, off, d, i);
                        }
                    }
                    "dub" => {
                        let pattern = [
                            (root_note, 118, 0.0, 0.75),
                            (root_note, 96, 1.5, 0.5),
                            (fifth_note, 102, 2.75, 0.45),
                            (oct_up_note, 92, 3.5, 0.35),
                        ];
                        for (i, &(p, v, off, d)) in pattern.iter().enumerate() {
                            push_note(p, v, off, d, i);
                        }
                    }
                    "acid_16ths" => {
                        for s in 0..16 {
                            let vel = if s % 4 == 0 { 115 } else { 85 };
                            let note = if s % 8 == 7 { oct_up_note } else { root_note };
                            push_note(note, vel, s as f64 * 0.25, 0.22, s);
                        }
                    }
                    _ => {
                        for eighth in 0..8 {
                            let vel = if eighth % 2 == 0 { 110 } else { 90 };
                            push_note(root_note, vel, eighth as f64 * 0.5, 0.45, eighth);
                        }
                    }
                }
            }

            let pattern = ClipPattern::Midi(MidiPattern {
                length_beats: total_beats,
                notes: notes.clone(),
            });
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, pattern);

            state.set_session_slot_clip(track_id, slot_id, clip);

            ToolCallResult::json(&json!({
                "status": "generated",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_name": clip_name,
                "total_beats": total_beats,
                "rhythm": rhythm,
                "note_events": notes.len()
            }))
        }
        "generate_arpeggio_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let key_str = args.get("key").and_then(|v| v.as_str()).unwrap_or("A");
            let scale_str = args.get("scale").and_then(|v| v.as_str()).unwrap_or("Minor");
            let prog_str = args.get("progression").and_then(|v| v.as_str()).unwrap_or("i-VI");
            let rate_str = args.get("rate").and_then(|v| v.as_str()).unwrap_or("1/16");
            let direction = args.get("direction").and_then(|v| v.as_str()).unwrap_or("up");
            let octaves = args.get("octaves").and_then(|v| v.as_u64()).unwrap_or(2).clamp(1, 3) as usize;
            let gate = args.get("gate").and_then(|v| v.as_f64()).unwrap_or(0.85).clamp(0.1, 1.0);
            let bars_override = match args.get("bars") {
                Some(v) => match v.as_u64() {
                    Some(b) if b >= 1 && (b as usize) <= MAX_TOTAL_BARS => Some(b as usize),
                    _ => {
                        return ToolCallResult::error(format!(
                            "bars out of valid range [1..={}]",
                            MAX_TOTAL_BARS
                        ))
                    }
                },
                None => None,
            };
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Arp");

            let step_dur = match rate_str.trim() {
                "1/8" => 0.5,
                "1/16T" | "1/16t" => 1.0 / 6.0,
                "1/32" => 0.125,
                _ => 0.25, // 1/16
            };

            let root_pc = parse_pitch_class(key_str).unwrap_or(9);
            let intervals = get_scale_intervals(scale_str);

            let tokens: Vec<&str> = prog_str.split(&['-', ' ', ','][..]).filter(|s| !s.is_empty()).collect();
            let prog_len = tokens.len().max(1);
            let total_bars = bars_override.unwrap_or(prog_len);
            let beats_per_chord = 4.0;
            let total_beats = (total_bars as f64) * beats_per_chord;

            let mut notes = Vec::new();
            let mut rng: u64 = 0x5DEECE66D ^ ((track_id as u64) << 16) ^ (slot_id as u64);

            for bar_idx in 0..total_bars {
                let bar_start = bar_idx as f64 * beats_per_chord;
                let token = tokens.get(bar_idx % prog_len).copied().unwrap_or("i");
                let step = parse_roman_degree(token).map(|(s, _)| s).unwrap_or(0);
                let base_pitches = build_chord_notes(root_pc, intervals, step, "close", 3);

                let mut pool: Vec<u8> = Vec::new();
                for oct in 0..octaves {
                    for &p in &base_pitches {
                        let shifted = (p as i32 + (oct as i32 * 12)).clamp(0, 127) as u8;
                        pool.push(shifted);
                    }
                }
                if pool.is_empty() {
                    pool.push(60);
                }

                match direction {
                    "down" => pool.reverse(),
                    "up_down" | "updown" => {
                        if pool.len() > 2 {
                            let rev: Vec<u8> = pool[1..pool.len() - 1].iter().rev().copied().collect();
                            pool.extend(rev);
                        }
                    }
                    _ => {}
                }

                let steps_in_bar = (beats_per_chord / step_dur).round().max(1.0) as usize;
                for s in 0..steps_in_bar {
                    let note_beat = bar_start + (s as f64 * step_dur);
                    let pitch = if direction == "random" {
                        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                        let idx = ((rng >> 33) as usize) % pool.len();
                        pool[idx]
                    } else {
                        pool[s % pool.len()]
                    };
                    notes.push(MidiNote::new(pitch, 100, note_beat, step_dur * gate));
                }
            }

            let pattern = ClipPattern::Midi(MidiPattern {
                length_beats: total_beats,
                notes: notes.clone(),
            });
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, pattern);

            state.set_session_slot_clip(track_id, slot_id, clip);

            ToolCallResult::json(&json!({
                "status": "generated",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_name": clip_name,
                "total_beats": total_beats,
                "direction": direction,
                "rate": rate_str,
                "note_events": notes.len()
            }))
        }
        "generate_euclidean_drum_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let hits = match args.get("hits").and_then(|v| v.as_u64()) {
                Some(k) => k as usize,
                None => return ToolCallResult::error("Missing 'hits'"),
            };
            let steps = args.get("steps").and_then(|v| v.as_u64()).unwrap_or(16) as usize;
            if steps == 0 || steps > 128 {
                return ToolCallResult::error("steps must be in [1..=128]");
            }
            let pad_idx = args.get("pad_index").and_then(|v| v.as_u64()).unwrap_or(6).min(15) as u8;
            let accent_first = args.get("accent_first").and_then(|v| v.as_bool()).unwrap_or(true);
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Euclidean Rhythm");

            let pattern_bools = bjorklund(hits, steps);
            let mut drum_pat = if steps == 32 {
                DrumStepPattern::new_32_step()
            } else {
                DrumStepPattern::new_16_step()
            };

            for (i, &hit) in pattern_bools.iter().enumerate() {
                if hit {
                    let acc = accent_first && i == 0;
                    let vel = if acc { 120 } else { 95 };
                    drum_pat.set_step(pad_idx, i, DrumStep::new(true, vel, 36 + pad_idx).with_accent(acc));
                }
            }

            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, ClipPattern::Drum(drum_pat));

            state.set_session_slot_clip(track_id, slot_id, clip);

            ToolCallResult::json(&json!({
                "status": "generated",
                "track_id": track_id,
                "slot_id": slot_id,
                "pad_index": pad_idx,
                "hits": hits,
                "steps": steps,
                "euclidean_sequence": pattern_bools
            }))
        }
        "generate_drum_groove_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let style = args.get("style").and_then(|v| v.as_str()).unwrap_or("house_909");
            let tempo = match validate_bpm(args.get("tempo").and_then(|v| v.as_f64()).unwrap_or(state.bpm)) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let swing_raw = args.get("swing").and_then(|v| v.as_f64()).unwrap_or(state.swing as f64);
            let complexity_raw = args.get("complexity").and_then(|v| v.as_f64()).unwrap_or(0.5);
            if !swing_raw.is_finite() || !complexity_raw.is_finite() {
                return ToolCallResult::error("swing and complexity must be finite numbers");
            }
            let swing = (swing_raw as f32).clamp(0.0, 1.0);
            let complexity = (complexity_raw as f32).clamp(0.0, 1.0);
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("909 Groove");

            let drum_pat = generate_drum_groove(style, tempo, swing, complexity);
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, ClipPattern::Drum(drum_pat));

            state.set_session_slot_clip(track_id, slot_id, clip);

            ToolCallResult::json(&json!({
                "status": "generated",
                "track_id": track_id,
                "slot_id": slot_id,
                "style": style,
                "clip_name": clip_name
            }))
        }
        "build_song_arrangement" => {
            let genre = args.get("genre").and_then(|v| v.as_str()).unwrap_or("techno");
            let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("Fm");
            let bpm_raw = args.get("bpm").and_then(|v| v.as_f64()).unwrap_or(126.0);
            let bpm = match validate_bpm(bpm_raw) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let raw_bars = args.get("total_bars").and_then(|v| v.as_u64()).unwrap_or(32);
            if raw_bars == 0 || (raw_bars as usize) > MAX_TOTAL_BARS {
                return ToolCallResult::error(format!(
                    "total_bars {} out of valid range [1..={}]",
                    raw_bars, MAX_TOTAL_BARS
                ));
            }
            let total_bars = (raw_bars as usize).max(4);

            state.bpm = bpm;
            state.project.transport.bpm = bpm;
            state.delay.set_tempo(bpm);

            let key_trimmed = key.trim();
            let is_minor = key_trimmed.ends_with('m') || key_trimmed.to_ascii_lowercase().contains("min");
            let root_str = key_trimmed
                .trim_end_matches("min")
                .trim_end_matches("maj")
                .trim_end_matches('m')
                .trim();
            let root_pc = parse_pitch_class(root_str).unwrap_or(5);
            let scale_name = args
                .get("scale")
                .and_then(|v| v.as_str())
                .unwrap_or(if is_minor { "Minor" } else { "Major" });
            let intervals = get_scale_intervals(scale_name);

            let drum_style = match genre.trim().to_ascii_lowercase().as_str() {
                "house" => "house_909",
                "dnb" | "drum_and_bass" | "drum & bass" => "dnb",
                "hiphop" | "hip-hop" | "hip_hop" => "hiphop",
                _ => "techno_peak",
            };

            // Ensure project tracks contain Lead Synth (0), 909 Drums (1), Sub Bass (2)
            while state.project.tracks.len() < 3 {
                let tid = state.project.tracks.len() as u32;
                let name = if tid == 2 { "Sub Bass" } else { "Track" };
                state.project.tracks.push(TrackDefinition::new_synth(tid, name));
            }
            while state.matrix.tracks.len() < 3 {
                let tid = state.matrix.tracks.len() as u32;
                let name = if tid == 2 { "Sub Bass" } else { "Track" };
                state.matrix.tracks.push(oxide_timeline::SessionTrack::new(tid, name, 8));
            }

            // Ensure arrangement tracks match project tracks (>= 3) without discarding extra tracks
            let needed = state.project.tracks.len().max(3);
            while state.timeline.tracks.len() < needed {
                let tid = state.timeline.tracks.len() as u32;
                let (t_name, t_type) = state
                    .project
                    .tracks
                    .get(tid as usize)
                    .map(|p| {
                        let kind = match p.track_type {
                            oxide_project::TrackType::DrumRack => "DrumRack",
                            oxide_project::TrackType::Audio => "Audio",
                            _ => "MidiSynth",
                        };
                        (p.name.as_str(), kind)
                    })
                    .unwrap_or(("Track", "MidiSynth"));
                state
                    .timeline
                    .tracks
                    .push(oxide_timeline::ArrangementTrack::new(tid, t_name, t_type));
            }
            let mut trk_synth = oxide_timeline::ArrangementTrack::new(0, "Lead Synth", "MidiSynth");
            let mut trk_drums = oxide_timeline::ArrangementTrack::new(1, "909 Drums", "DrumRack");
            let mut trk_bass = oxide_timeline::ArrangementTrack::new(2, "Sub Bass", "MidiSynth");

            // Filter sweep automation lane on Synth (15 = filter_frequency)
            let mut filter_auto = AutomationLane::new(15, "filter_frequency");
            filter_auto.add_point(AutomationPoint::linear(0.0, 400.0));
            filter_auto.add_point(AutomationPoint::linear(32.0, 1200.0));
            filter_auto.add_point(AutomationPoint::linear(64.0, 4500.0));
            filter_auto.add_point(AutomationPoint::linear(96.0, 8000.0));
            filter_auto.add_point(AutomationPoint::linear((total_bars * 4) as f64, 15000.0));
            trk_synth.automation_lanes.push(filter_auto);

            let prog_degrees = [0usize, 5, 2, 6];
            let section_bars = (total_bars / 4).max(1);

            for s in 0..4 {
                let start_beat = (s * section_bars * 4) as f64;
                let dur = (section_bars * 4) as f64;

                let (section_name, complexity) = match s {
                    0 => ("Intro", 0.25f32),
                    1 => ("Build", 0.65f32),
                    2 => ("Drop", 0.95f32),
                    _ => ("Outro", 0.40f32),
                };

                // Generate real Synth lead & chord notes across section bars
                let mut synth_notes = Vec::new();
                let mut bass_notes = Vec::new();
                for bar in 0..section_bars {
                    let bar_off = (bar * 4) as f64;
                    let deg = prog_degrees[bar % prog_degrees.len()];
                    let chord = build_chord_notes(root_pc, intervals, deg, "close", 4);
                    let bass_root = ((2 + 1) * 12 + root_pc as i32 + intervals[deg % intervals.len()] as i32).clamp(0, 127) as u8;
                    let bass_oct = (bass_root as i32 + 12).clamp(0, 127) as u8;

                    if s == 2 {
                        // Drop: energetic chord hits + lead motif + driving 16th/8th bass
                        for &p in &chord {
                            synth_notes.push(MidiNote::new(p, 102, bar_off, 1.5));
                            synth_notes.push(MidiNote::new(p, 94, bar_off + 2.0, 1.5));
                        }
                        for (idx, &step_off) in [0.0, 0.75, 1.5, 2.25, 3.0, 3.5].iter().enumerate() {
                            let bp = if idx % 3 == 2 { bass_oct } else { bass_root };
                            bass_notes.push(MidiNote::new(bp, 112, bar_off + step_off, 0.4));
                        }
                    } else if s == 1 {
                        // Build: rising staccato 8th chords + pulsing bass
                        for q in 0..4 {
                            let vel = (80 + q * 10).min(120) as u8;
                            if let Some(&p) = chord.get(q % chord.len()) {
                                synth_notes.push(MidiNote::new(p, vel, bar_off + q as f64, 0.75));
                            }
                            bass_notes.push(MidiNote::new(bass_root, vel, bar_off + q as f64, 0.75));
                        }
                    } else {
                        // Intro / Outro: atmospheric sustained pad root & fifth + sparse bass
                        if let Some(&p0) = chord.first() {
                            synth_notes.push(MidiNote::new(p0, 84, bar_off, 3.5));
                        }
                        if let Some(&p2) = chord.get(2) {
                            synth_notes.push(MidiNote::new(p2, 78, bar_off + 2.0, 1.75));
                        }
                        bass_notes.push(MidiNote::new(bass_root, 96, bar_off, 1.75));
                        bass_notes.push(MidiNote::new(bass_root, 90, bar_off + 2.0, 1.75));
                    }
                }

                let mut drum_pat = generate_drum_groove(drum_style, bpm, state.swing, complexity);
                if s == 0 {
                    // Intro: strip clap density (pad 6) while keeping kick/hat pulse
                    drum_pat.tracks[6].steps.iter_mut().for_each(|st| st.active = false);
                }

                let synth_clip = ArrangementClip::new(
                    s as u32 + 1,
                    format!("Lead - {}", section_name),
                    start_beat,
                    dur,
                    ClipPattern::Midi(MidiPattern {
                        length_beats: dur,
                        notes: synth_notes,
                    }),
                );
                let drum_clip = ArrangementClip::new(
                    s as u32 + 10,
                    format!("Drums - {}", section_name),
                    start_beat,
                    dur,
                    ClipPattern::Drum(drum_pat),
                );
                let bass_clip = ArrangementClip::new(
                    s as u32 + 20,
                    format!("Bass - {}", section_name),
                    start_beat,
                    dur,
                    ClipPattern::Midi(MidiPattern {
                        length_beats: dur,
                        notes: bass_notes,
                    }),
                );

                trk_synth.add_clip(synth_clip);
                trk_drums.add_clip(drum_clip);
                trk_bass.add_clip(bass_clip);
            }

            state.timeline.tracks[0] = trk_synth;
            state.timeline.tracks[1] = trk_drums;
            state.timeline.tracks[2] = trk_bass;

            ToolCallResult::json(&json!({
                "status": "built",
                "genre": genre,
                "key": key,
                "scale": scale_name,
                "bpm": bpm,
                "total_bars": total_bars,
                "total_beats": total_bars * 4,
                "tracks_created": 3,
                "sections": ["Intro", "Build", "Drop", "Outro"]
            }))
        }

        // --- "Agent Ears" Audio Analysis ---
        "analyze_rendered_audio" => {
            if let Some(wav_path_str) = args.get("wav_path").and_then(|v| v.as_str()) {
                let resolved_wav = expand_tilde_path(wav_path_str);
                match AudioAnalysisMetrics::analyze_wav_file(&resolved_wav) {
                    Ok(metrics) => {
                        let mut val = serde_json::to_value(&metrics).unwrap_or_default();
                        if let Some(obj) = val.as_object_mut() {
                            obj.insert("peak_dbfs".to_string(), json!(metrics.peak_dbfs));
                            obj.insert("rms_dbfs".to_string(), json!(metrics.rms_dbfs));
                            obj.insert("crest_factor_db".to_string(), json!(metrics.crest_factor_db));
                            obj.insert("spectral_centroid_hz".to_string(), json!(metrics.spectral_centroid_hz));
                            obj.insert("spectral_bands".to_string(), json!({
                                "sub_bass_energy": metrics.spectral_bands.sub_bass_energy_pct,
                                "low_mid_energy": metrics.spectral_bands.low_mid_energy_pct,
                                "high_mid_energy": metrics.spectral_bands.high_mid_energy_pct,
                                "high_energy": metrics.spectral_bands.high_energy_pct,
                            }));
                        }
                        ToolCallResult::json(&val)
                    }
                    Err(e) => ToolCallResult::error(format!("Failed to analyze WAV: {}", e)),
                }
            } else {
                let dur = args.get("duration_seconds").and_then(|v| v.as_f64()).unwrap_or(4.0).clamp(0.5, 300.0);
                state.sync_project_from_state();
                let has_real_clips = state.project.tracks.iter().any(|t| {
                    !t.arrangement_clips.is_empty()
                        || t.session_clips.iter().any(|c| c.is_some())
                });
                let use_project_renderer = has_real_clips
                    || state.last_saved_path.is_some()
                    || state.project.metadata.name != "Ableton MCP Session"
                    || state.project.tracks.len() != 2;
                let samples = if use_project_renderer {
                    let tmp_wav = default_temp_file("oxide_analyze", "wav");
                    let res = render_project_window_to_wav(&state.project, &tmp_wav, 0.0, dur, 32);
                    let _ = fs::remove_file(&tmp_wav);
                    match res {
                        Ok(s) => s,
                        Err(e) => return ToolCallResult::error(format!("Failed rendering project audio: {}", e)),
                    }
                } else {
                    let renderer = DemoArrangementRenderer::new(state.sample_rate, state.bpm.max(60.0));
                    renderer.render_demo_seconds(dur)
                };
                let metrics = AudioAnalysisMetrics::analyze_interleaved(&samples, state.sample_rate as u32, 2);
                let mut val = serde_json::to_value(&metrics).unwrap_or_default();
                if let Some(obj) = val.as_object_mut() {
                    obj.insert("peak_dbfs".to_string(), json!(metrics.peak_dbfs));
                    obj.insert("rms_dbfs".to_string(), json!(metrics.rms_dbfs));
                    obj.insert("crest_factor_db".to_string(), json!(metrics.crest_factor_db));
                    obj.insert("spectral_centroid_hz".to_string(), json!(metrics.spectral_centroid_hz));
                    obj.insert("spectral_bands".to_string(), json!({
                        "sub_bass_energy": metrics.spectral_bands.sub_bass_energy_pct,
                        "low_mid_energy": metrics.spectral_bands.low_mid_energy_pct,
                        "high_mid_energy": metrics.spectral_bands.high_mid_energy_pct,
                        "high_energy": metrics.spectral_bands.high_energy_pct,
                    }));
                }
                ToolCallResult::json(&val)
            }
        }
        "preview_synth_patch" => {
            let mut preview_synth = state.synth.clone();
            if let Some(preset) = args.get("preset_name").and_then(|v| v.as_str()) {
                preview_synth.load_preset(preset);
            }
            if let Some(params_obj) = args.get("parameters").and_then(|v| v.as_object()) {
                for (k, val) in params_obj {
                    if let Some(num) = val.as_f64() {
                        preview_synth.set_parameter(k, num as f32);
                    }
                }
            }

            let note = args.get("note").and_then(|v| v.as_u64()).unwrap_or(48) as u8;
            let dur = args.get("duration_seconds").and_then(|v| v.as_f64()).unwrap_or(1.5).clamp(0.2, 5.0);
            let out_path = args
                .get("output_wav_path")
                .and_then(|v| v.as_str())
                .map(expand_tilde_path)
                .unwrap_or_else(|| default_temp_file("oxide_synth_preview", "wav"));
            let out_path_str = out_path.to_string_lossy().to_string();

            let total_frames = (dur * preview_synth.sample_rate as f64).ceil() as usize;
            let gate_frames = (total_frames * 3) / 4;

            preview_synth.note_on(note, 100);

            let mut interleaved = Vec::with_capacity(total_frames * 2);
            for f in 0..total_frames {
                if f == gate_frames {
                    preview_synth.note_off(note);
                }
                let s = preview_synth.process_sample();
                interleaved.push(s);
                interleaved.push(s);
            }

            if let Some(parent) = out_path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return ToolCallResult::error(format!("Failed to create output directory: {}", e));
                }
            }

            if let Err(e) = WavWriter::write_wav_16bit(&out_path, preview_synth.sample_rate as u32, 2, &interleaved) {
                return ToolCallResult::error(format!("Failed to write preview WAV: {}", e));
            }
            let metrics = AudioAnalysisMetrics::analyze_interleaved(&interleaved, preview_synth.sample_rate as u32, 2);

            ToolCallResult::json(&json!({
                "status": "preview_ready",
                "output_wav_path": out_path_str,
                "note": note,
                "duration_seconds": metrics.duration_seconds,
                "peak_dbfs": metrics.peak_dbfs,
                "rms_dbfs": metrics.rms_dbfs,
                "crest_factor_db": metrics.crest_factor_db,
                "spectral_centroid_hz": metrics.spectral_centroid_hz,
                "spectral_bands": metrics.spectral_bands
            }))
        }

        // --- Group 2 (Extra): Devices & Chains ---
        "device_add_to_track" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let dev_type = match args.get("device_type").or_else(|| args.get("device_name")).and_then(|v| v.as_str()) {
                Some(d) => d,
                None => return ToolCallResult::error("Missing 'device_type' or 'device_name' parameter"),
            };
            let pos = args.get("position").and_then(|v| v.as_u64()).map(|p| p as usize);
            match instantiate_managed_device(dev_type, state.sample_rate) {
                Ok((dev, handle)) => {
                    let bpm = state.bpm;
                    let cat = handle.category();
                    let chain = state.device_chains.entry(track_id).or_insert_with(|| {
                        let mut c = DeviceChain::new();
                        c.name = format!("Track {} Chain", track_id);
                        c
                    });
                    let inserted_idx = add_device_to_chain(chain, dev, pos);
                    let handles = state.device_handles.entry(track_id).or_default();
                    handles.add_handle(cat, handle, pos);

                    if let Some(p_trk) = state.project.tracks.get_mut(track_id as usize) {
                        p_trk.insert_fx = handles
                            .audio_effects
                            .iter()
                            .filter_map(|h| h.to_insert_effect_config(bpm))
                            .collect();
                    }

                    ToolCallResult::json(&json!({
                        "status": "device_added",
                        "track_id": track_id,
                        "device_type": dev_type,
                        "device_index": inserted_idx,
                        "total_devices": chain.device_count()
                    }))
                }
                Err(e) => ToolCallResult::error(e),
            }
        }
        "device_remove_from_track" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let dev_index = match args.get("device_index").and_then(|v| v.as_u64()) {
                Some(idx) => idx as usize,
                None => return ToolCallResult::error("Missing 'device_index' parameter"),
            };
            let bpm = state.bpm;
            let chain = state.device_chains.entry(track_id).or_default();
            if dev_index >= chain.device_count() {
                return ToolCallResult::error(format!(
                    "Device index {} out of range for track {} (count: {})",
                    dev_index,
                    track_id,
                    chain.device_count()
                ));
            }
            let midi_len = chain.midi_effects.len();
            if dev_index < midi_len {
                chain.remove_midi_effect(dev_index);
            } else if chain.has_instrument() && dev_index == midi_len {
                chain.set_instrument(None);
            } else {
                let inst_offset = if chain.has_instrument() { 1 } else { 0 };
                let audio_idx = dev_index.saturating_sub(midi_len + inst_offset);
                chain.remove_audio_effect(audio_idx);
            }
            if let Some(handles) = state.device_handles.get_mut(&track_id) {
                handles.remove_at(dev_index);
                if let Some(p_trk) = state.project.tracks.get_mut(track_id as usize) {
                    p_trk.insert_fx = handles
                        .audio_effects
                        .iter()
                        .filter_map(|h| h.to_insert_effect_config(bpm))
                        .collect();
                }
            }
            ToolCallResult::json(&json!({
                "status": "device_removed",
                "track_id": track_id,
                "device_index": dev_index,
                "remaining_devices": chain.device_count()
            }))
        }
        "device_reorder" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let from_index = match args.get("from_index").and_then(|v| v.as_u64()) {
                Some(idx) => idx as usize,
                None => return ToolCallResult::error("Missing 'from_index' parameter"),
            };
            let to_index = match args.get("to_index").and_then(|v| v.as_u64()) {
                Some(idx) => idx as usize,
                None => return ToolCallResult::error("Missing 'to_index' parameter"),
            };
            let bpm = state.bpm;
            let chain = state.device_chains.entry(track_id).or_default();
            let handles_opt = state.device_handles.get_mut(&track_id);
            if let Err(err) = reorder_device_chain_global(chain, handles_opt, from_index, to_index) {
                return ToolCallResult::error(err);
            }
            if let Some(handles) = state.device_handles.get(&track_id) {
                if let Some(p_trk) = state.project.tracks.get_mut(track_id as usize) {
                    p_trk.insert_fx = handles
                        .audio_effects
                        .iter()
                        .filter_map(|h| h.to_insert_effect_config(bpm))
                        .collect();
                }
            }
            ToolCallResult::json(&json!({
                "status": "device_reordered",
                "track_id": track_id,
                "from_index": from_index,
                "to_index": to_index,
                "success": true
            }))
        }
        "device_set_param" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let device_index = match args.get("device_index").and_then(|v| v.as_u64()) {
                Some(idx) => idx as usize,
                None => return ToolCallResult::error("Missing 'device_index' parameter"),
            };
            let param_val = args
                .get("param")
                .or_else(|| args.get("param_name"))
                .or_else(|| args.get("param_index"));
            let param_key = match param_val {
                Some(v) if v.is_string() => v.as_str().unwrap_or("0").to_string(),
                Some(v) if v.is_u64() => v.as_u64().unwrap_or(0).to_string(),
                _ => return ToolCallResult::error("Missing 'param' parameter"),
            };
            let value = match args.get("value").and_then(|v| v.as_f64()) {
                Some(val) if val.is_finite() => val as f32,
                Some(_) => return ToolCallResult::error("Non-finite 'value' parameter"),
                None => return ToolCallResult::error("Missing 'value' parameter"),
            };

            match set_track_device_param_helper(state, track_id, device_index, &param_key, value) {
                Ok((device_name, resolved_param, applied_value)) => ToolCallResult::json(&json!({
                    "status": "parameter_set",
                    "track_id": track_id,
                    "device_index": device_index,
                    "device_name": device_name,
                    "param": resolved_param,
                    "value": applied_value
                })),
                Err(e) => ToolCallResult::error(e),
            }
        }
        "device_get_chain" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let chain = state.device_chains.entry(track_id).or_default();
            let handles = state.device_handles.get(&track_id);
            ToolCallResult::json(&dump_device_chain_detailed(chain, handles))
        }

        // --- Group 3 (Extra): Synths, Drums & Simpler ---
        "simpler_slice_to_drum_rack" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let simpler_device_index = args
                .get("simpler_device_index")
                .and_then(|v| v.as_u64())
                .map(|i| i as usize);

            // Inspect any Simpler device on the source track, or fallback to a warped audio clip on the source track
            let mut source_samples: Option<(std::sync::Arc<[f32]>, u32, u8, Vec<usize>, String)> = None;

            if let Some(handles) = state.device_handles.get(&track_id) {
                let simpler_opt = if let Some(dev_idx) = simpler_device_index {
                    match handles.get_at(dev_idx) {
                        Some(ManagedDeviceHandle::Simpler(simpler_arc)) => Some(simpler_arc),
                        Some(other) => {
                            return ToolCallResult::error(format!(
                                "Device index {} on track {} is '{}', expected 'Simpler'",
                                dev_idx,
                                track_id,
                                other.name()
                            ));
                        }
                        None => {
                            return ToolCallResult::error(format!(
                                "Device index {} out of range on track {}",
                                dev_idx, track_id
                            ));
                        }
                    }
                } else if let Some(ManagedDeviceHandle::Simpler(simpler_arc)) = &handles.instrument {
                    Some(simpler_arc)
                } else {
                    None
                };

                if let Some(simpler_arc) = simpler_opt {
                    let mut simpler = simpler_arc.lock().unwrap_or_else(|e| e.into_inner());
                    if simpler.sample_data.is_some() && simpler.slices.is_empty() {
                        simpler.recalculate_slices();
                    }
                    if let Some(ref data) = simpler.sample_data {
                        let mut slices = simpler.slices.clone();
                        if slices.is_empty() {
                            slices = detect_transients(
                                data.as_ref(),
                                simpler.sample_channels.max(1) as usize,
                                simpler.sample_sample_rate.max(8000) as f32,
                            );
                        }
                        if slices.is_empty() {
                            // Even-grid fallback when sample is continuous tone/pad with no sharp transient peaks
                            let n = 16usize.min((data.len() / 256).max(1));
                            let step = (data.len() / n).max(1);
                            slices = (0..n).map(|i| i * step).collect();
                        }
                        source_samples = Some((
                            std::sync::Arc::clone(data),
                            simpler.sample_sample_rate,
                            simpler.sample_channels,
                            slices,
                            "Simpler Slice".to_string(),
                        ));
                    }
                }
            }

            if source_samples.is_none() {
                if let Some((_, w_clip)) = state
                    .audio_clips
                    .iter()
                    .find(|((tid, _), _)| *tid == track_id)
                {
                    if !w_clip.raw_samples.is_empty() {
                        let mut slices = detect_transients(
                            &w_clip.raw_samples,
                            w_clip.channels.max(1) as usize,
                            w_clip.sample_rate.max(8000) as f32,
                        );
                        if slices.is_empty() {
                            let n = 16usize.min((w_clip.raw_samples.len() / 256).max(1));
                            let step = (w_clip.raw_samples.len() / n).max(1);
                            slices = (0..n).map(|i| i * step).collect();
                        }
                        source_samples = Some((
                            std::sync::Arc::from(w_clip.raw_samples.clone().into_boxed_slice()),
                            w_clip.sample_rate,
                            w_clip.channels as u8,
                            slices,
                            w_clip.name.clone(),
                        ));
                    }
                }
            }

            let (sample_arc, sr, ch, mut slices, src_label) = match source_samples {
                Some(s) => s,
                None => {
                    return ToolCallResult::error(format!(
                        "Track {} does not contain a Simpler instrument with loaded audio or an audio clip to slice",
                        track_id
                    ));
                }
            };

            slices.sort_unstable();
            slices.dedup();
            if slices.len() > 16 {
                slices.truncate(16);
            }
            if slices.is_empty() {
                slices.push(0);
            }

            let new_track_id = state.project.tracks.len() as u32;
            let rack_name = format!("Sliced Drum Rack from {}", track_id);
            let mut new_drum_track = TrackDefinition::new_drum_rack(new_track_id, &rack_name);

            let (dev, handle) = match instantiate_managed_device("DrumRack", state.sample_rate) {
                Ok(pair) => pair,
                Err(e) => return ToolCallResult::error(e),
            };

            let slice_count = slices.len();
            if let ManagedDeviceHandle::DrumRack(ref rack_arc) = handle {
                let mut rack = rack_arc.lock().unwrap_or_else(|e| e.into_inner());
                for (i, &start_idx) in slices.iter().enumerate() {
                    let end_idx = slices
                        .get(i + 1)
                        .copied()
                        .unwrap_or(sample_arc.len())
                        .min(sample_arc.len());
                    if end_idx > start_idx {
                        let slice_buf: std::sync::Arc<[f32]> =
                            std::sync::Arc::from(sample_arc[start_idx..end_idx].to_vec().into_boxed_slice());
                        rack.pads[i].name = format!("{} #{}", src_label, i + 1);
                        rack.pads[i].source = DrumPadSource::Sample {
                            sample_data: slice_buf.clone(),
                            channels: ch.max(1),
                            sample_rate: sr.max(8000),
                        };
                        if i < rack.all_pads.len() {
                            rack.all_pads[i].name = rack.pads[i].name.clone();
                            rack.all_pads[i].source = rack.pads[i].source.clone();
                        }
                    }
                }
            }

            if let InstrumentConfig::ElectronicDrumRack(ref mut dr_def) = new_drum_track.instrument {
                for (i, &start_idx) in slices.iter().enumerate() {
                    if let Some(pad_def) = dr_def.pads.get_mut(i) {
                        pad_def.name = format!("{} #{}", src_label, i + 1);
                        pad_def.sample_path = format!("slice:{}:{}", start_idx, slices.get(i + 1).copied().unwrap_or(sample_arc.len()));
                        pad_def.pitch_offset_semitones = 0.0;
                    }
                }
            }

            state.project.tracks.push(new_drum_track);
            while state.matrix.tracks.len() <= new_track_id as usize {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(oxide_timeline::SessionTrack::new(tid, &rack_name, 8));
            }
            while state.timeline.tracks.len() <= new_track_id as usize {
                let tid = state.timeline.tracks.len() as u32;
                state.timeline.tracks.push(oxide_timeline::ArrangementTrack::new(tid, &rack_name, "DrumRack"));
            }
            let _ = state.ensure_timeline_track_mut(new_track_id as usize);

            let chain = state.device_chains.entry(new_track_id).or_default();
            add_device_to_chain(chain, dev, None);
            let handles = state.device_handles.entry(new_track_id).or_default();
            handles.add_handle(DeviceCategory::Instrument, handle, None);

            ToolCallResult::json(&json!({
                "status": "sliced_to_drum_rack",
                "source_track_id": track_id,
                "new_track_id": new_track_id,
                "slices_count": slice_count,
                "slice_offsets": slices,
                "drum_rack_name": rack_name
            }))
        }

        // --- Group 4: Browser & Neural Sound Similarity ---
        "browser_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let category = args.get("category").and_then(|v| v.as_str()).and_then(BrowserCategory::from_str_loose);
            let tags_vec: Option<Vec<String>> = args.get("tags").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect()
            });
            let results = state.browser.search(query, category, tags_vec.as_deref());
            let count = results.len();
            ToolCallResult::json(&json!({
                "query": query,
                "category": category.map(|c| c.name()),
                "count": count,
                "items": results
            }))
        }
        "browser_find_similar_sounds" => {
            let target_id = match args.get("target_id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => return ToolCallResult::error("Missing 'target_id' parameter"),
            };
            let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            let matches = state.browser.find_similar(target_id, top_k);
            let matches_json: Vec<Value> = matches
                .iter()
                .map(|(item, score)| json!({
                    "item": item,
                    "cosine_similarity": score
                }))
                .collect();
            ToolCallResult::json(&json!({
                "target_id": target_id,
                "matches_count": matches_json.len(),
                "matches": matches_json
            }))
        }

        // --- Group 6: Live 12 MIDI Generators & Transformers ---
        "midi_transform_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id' parameter"),
            };
            let transform = match args.get("transform").and_then(|v| v.as_str()) {
                Some(t) => t.to_lowercase(),
                None => return ToolCallResult::error("Missing 'transform' parameter"),
            };
            let params = args.get("params").cloned().unwrap_or(json!({}));
            let lock = current_scale_lock(state);

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(oxide_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(oxide_timeline::SessionSlot::default());
            }
            let slot = &mut track.slots[slot_id];

            let mut pattern = match slot.clip.as_ref() {
                Some(c) => match &c.pattern {
                    ClipPattern::Midi(m) => m.clone(),
                    _ => MidiPattern::new(4.0),
                },
                None => MidiPattern::new(4.0),
            };

            match transform.as_str() {
                "arpeggiate" => {
                    let step = params.get("step_duration_beats").and_then(|v| v.as_f64()).unwrap_or(0.25);
                    let downwards = params.get("downwards").and_then(|v| v.as_bool()).unwrap_or(false);
                    transform_arpeggiate(&mut pattern, step, downwards);
                }
                "connect" => {
                    transform_connect(&mut pattern, lock, 0.5);
                }
                "ornament" => {
                    transform_ornament(&mut pattern, 0.05, 2, 0.8);
                }
                "quantize" => {
                    let grid = params.get("grid_beats").and_then(|v| v.as_f64()).unwrap_or(0.25);
                    let strength = params.get("strength").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    transform_quantize(&mut pattern, grid, strength, 0.0);
                }
                "recombine" => {
                    transform_recombine(&mut pattern, 1, 1, 0);
                }
                "span" => {
                    transform_span(&mut pattern, SpanMode::Legato, 0.25);
                }
                "strum" => {
                    let delay = params.get("strum_delay_beats").and_then(|v| v.as_f64()).unwrap_or(0.02);
                    transform_strum(&mut pattern, delay, StrumDirection::Up);
                }
                "time_warp" => {
                    let exp = params.get("exponent").and_then(|v| v.as_f64()).unwrap_or(1.5);
                    transform_time_warp(&mut pattern, exp);
                }
                _ => return ToolCallResult::error(format!("Transform '{}' not supported", transform)),
            }

            let notes_count = pattern.notes.len();
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            slot.set_clip(Some(SessionClip::new(clip_id, format!("Transformed Clip {}", transform), ClipPattern::Midi(pattern))));
            state.audio_clips.remove(&(track_id as u32, slot_id as u32));

            ToolCallResult::json(&json!({
                "status": "transformed",
                "track_id": track_id,
                "slot_id": slot_id,
                "transform": transform,
                "notes_count": notes_count
            }))
        }
        "midi_generate_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id' parameter"),
            };
            let generator = match args.get("generator").and_then(|v| v.as_str()) {
                Some(g) => g.to_lowercase(),
                None => return ToolCallResult::error("Missing 'generator' parameter"),
            };
            let params = args.get("params").cloned().unwrap_or(json!({}));
            let lock = current_scale_lock(state);

            let pattern = match generator.as_str() {
                "seed" => {
                    let seed = params.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
                    let density = params.get("density").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                    generate_seed(4.0, 0.25, density, 48, 72, 80, 120, 0.2, 0.5, Some(lock), seed)
                }
                "shape" => {
                    let shape = match params.get("shape").and_then(|v| v.as_str()).unwrap_or("arch") {
                        "linear_rise" | "rise" => ContourShape::LinearRise,
                        "linear_fall" | "fall" => ContourShape::LinearFall,
                        "valley" | "cup" => ContourShape::ValleyCup,
                        "scurve" | "s_curve" => ContourShape::SCurve,
                        "triangle" => ContourShape::TriangleWave,
                        _ => ContourShape::BellArch,
                    };
                    generate_shape(4.0, 0.25, shape, 48, 72, 100, Some(lock))
                }
                "stacks" => {
                    let chord_type = match params.get("chord").and_then(|v| v.as_str()).unwrap_or("maj7") {
                        "minor" | "min" => StacksChordType::MinorTriad,
                        "dom7" => StacksChordType::Dom7,
                        "maj7" => StacksChordType::Maj7,
                        "min7" => StacksChordType::Min7,
                        "sus2" => StacksChordType::Sus2,
                        "sus4" => StacksChordType::Sus4,
                        "quartal" => StacksChordType::Quartal,
                        _ => StacksChordType::MajorTriad,
                    };
                    let roots = [60, 65, 67, 60];
                    generate_stacks(&roots, chord_type, StacksVoicing::Closed, 0, 1.0, 100)
                }
                "rhythm" => {
                    let style = match params.get("style").and_then(|v| v.as_str()).unwrap_or("straight") {
                        "syncopated" | "boombap" => RhythmStyle::BoomBap,
                        "trap" | "traphihats" => RhythmStyle::TrapHiHats,
                        "clave" | "clave32" => RhythmStyle::Clave32,
                        "afrobeat" => RhythmStyle::Afrobeat,
                        _ => RhythmStyle::FourOnTheFloor,
                    };
                    generate_rhythm(4.0, style, 60, 100, 127)
                }
                "euclidean" => {
                    let hits = params.get("hits").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                    let steps = params.get("steps").and_then(|v| v.as_u64()).unwrap_or(16) as usize;
                    if steps == 0 || steps > 256 {
                        return ToolCallResult::error("euclidean 'steps' must be in [1..=256]");
                    }
                    generate_euclidean(hits, steps, 0, 60, 0.25, 100)
                }
                _ => return ToolCallResult::error(format!("Generator '{}' not supported", generator)),
            };

            let notes_count = pattern.notes.len();
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            state.set_session_slot_clip(
                track_id,
                slot_id,
                SessionClip::new(clip_id, format!("Generated Clip {}", generator), ClipPattern::Midi(pattern)),
            );

            ToolCallResult::json(&json!({
                "status": "generated",
                "track_id": track_id,
                "slot_id": slot_id,
                "generator": generator,
                "notes_count": notes_count
            }))
        }

        // --- Group 7: Audio Clips & Warping Engine ---
        "audio_clip_create_warped" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_slot_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'slot_id' parameter"),
            };
            let sample_path = match args.get("sample_path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ToolCallResult::error("Missing 'sample_path' parameter"),
            };
            let resolved_sample = expand_tilde_path(sample_path);
            let warp_mode_str = args.get("warp_mode").and_then(|v| v.as_str()).unwrap_or("beats");

            let mode = match warp_mode_str.to_lowercase().as_str() {
                "tones" => WarpMode::Tones,
                "texture" => WarpMode::Texture,
                "repitch" | "re-pitch" => WarpMode::RePitch,
                "complex" => WarpMode::Complex,
                "complex_pro" | "complexpro" => WarpMode::ComplexPro,
                _ => WarpMode::Beats,
            };

            if !resolved_sample.exists() {
                return ToolCallResult::error(format!(
                    "Audio sample file does not exist: {}",
                    resolved_sample.display()
                ));
            }

            let mut clip = match WarpedAudioClip::from_wav_file(&resolved_sample, state.bpm) {
                Ok(c) => c,
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to load WAV audio clip '{}': {}",
                        resolved_sample.display(),
                        e
                    ));
                }
            };

            let resolved_str = resolved_sample.to_string_lossy().to_string();
            clip.name = resolved_str.clone();
            clip.warper.config.mode = mode;
            let len_beats = clip.length_beats;

            // Mirror audio clip into session matrix so groove_pool_apply_to_clip / clip_launch work immediately
            while state.matrix.tracks.len() <= track_id as usize {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(oxide_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let m_trk = &mut state.matrix.tracks[track_id as usize];
            while m_trk.slots.len() <= slot_id as usize {
                m_trk.slots.push(oxide_timeline::SessionSlot::default());
            }
            let clip_name = std::path::Path::new(&resolved_str)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Audio Clip")
                .to_string();
            let audio_data = AudioClipData {
                file_path: resolved_str.clone(),
                sample_rate: clip.sample_rate,
                channels: clip.channels,
                length_beats: len_beats,
                gain_db: 0.0,
                warp_mode: format!("{:?}", clip.warper.config.mode),
                samples: clip.raw_samples.clone(),
            };
            m_trk.slots[slot_id as usize].set_clip(Some(SessionClip::new(
                (track_id * 100) + slot_id + 1,
                clip_name,
                ClipPattern::Audio(audio_data),
            )));

            state.audio_clips.insert((track_id, slot_id), clip);

            ToolCallResult::json(&json!({
                "status": "created",
                "track_id": track_id,
                "slot_id": slot_id,
                "sample_path": resolved_str,
                "warp_mode": warp_mode_str,
                "length_beats": len_beats
            }))
        }
        "audio_clip_set_warp_mode" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as u32,
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as u32,
                None => return ToolCallResult::error("Missing 'slot_id' parameter"),
            };
            let warp_mode_str = match args.get("warp_mode").and_then(|v| v.as_str()) {
                Some(m) => m,
                None => return ToolCallResult::error("Missing 'warp_mode' parameter"),
            };
            let grain_size = args.get("grain_size").and_then(|v| v.as_f64()).map(|g| g as f32);

            let mode = match warp_mode_str.to_lowercase().as_str() {
                "tones" => WarpMode::Tones,
                "texture" => WarpMode::Texture,
                "repitch" | "re-pitch" => WarpMode::RePitch,
                "complex" => WarpMode::Complex,
                "complex_pro" | "complexpro" => WarpMode::ComplexPro,
                _ => WarpMode::Beats,
            };

            let clip = match state.audio_clips.get_mut(&(track_id, slot_id)) {
                Some(c) => c,
                None => {
                    return ToolCallResult::error(format!(
                        "No warped audio clip at track_id={}, slot_id={}",
                        track_id, slot_id
                    ));
                }
            };
            clip.warper.config.mode = mode;
            if let Some(g) = grain_size {
                clip.warper.config.tones_config.grain_size_ms = g;
                clip.warper.config.texture_config.grain_size_ms = g;
            }

            ToolCallResult::json(&json!({
                "status": "updated",
                "track_id": track_id,
                "slot_id": slot_id,
                "warp_mode": warp_mode_str,
                "grain_size": grain_size
            }))
        }
        "audio_clip_add_warp_marker" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as u32,
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as u32,
                None => return ToolCallResult::error("Missing 'slot_id' parameter"),
            };
            let sample_time = match args.get("sample_time").and_then(|v| v.as_f64()) {
                Some(t) => t,
                None => return ToolCallResult::error("Missing 'sample_time' parameter"),
            };
            let beat_time = match args.get("beat_time").and_then(|v| v.as_f64()) {
                Some(b) => b,
                None => return ToolCallResult::error("Missing 'beat_time' parameter"),
            };

            let clip = match state.audio_clips.get_mut(&(track_id, slot_id)) {
                Some(c) => c,
                None => {
                    return ToolCallResult::error(format!(
                        "No warped audio clip at track_id={}, slot_id={}",
                        track_id, slot_id
                    ));
                }
            };
            clip.warper.add_marker(beat_time, sample_time);
            let count = clip.warper.markers.len();

            ToolCallResult::json(&json!({
                "status": "marker_added",
                "track_id": track_id,
                "slot_id": slot_id,
                "sample_time": sample_time,
                "beat_time": beat_time,
                "marker_count": count
            }))
        }

        // --- Group 8: Groove Pool & Comping Take Lanes ---
        "groove_pool_list" => {
            let templates: Vec<Value> = state.groove_pool.templates.iter().map(|t| json!({
                "id": t.id,
                "name": t.name,
                "timing_amount": t.timing_amount,
                "quantize_amount": t.quantize_amount,
                "velocity_amount": t.velocity_amount,
                "steps_count": t.step_offsets.len()
            })).collect();

            ToolCallResult::json(&json!({
                "global_amount": state.groove_pool.global_amount,
                "template_count": templates.len(),
                "templates": templates
            }))
        }
        "groove_pool_apply_to_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id' parameter"),
            };
            let groove_name = match args.get("groove_name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => return ToolCallResult::error("Missing 'groove_name' parameter"),
            };
            let clip_amount = args
                .get("amount")
                .and_then(|v| v.as_f64())
                .map(|a| (a as f32).clamp(0.0, 1.30));

            let template = state
                .groove_pool
                .find_template(groove_name)
                .cloned()
                .or_else(|| {
                    let low = groove_name.to_ascii_lowercase();
                    if low.contains("mpc") {
                        state.groove_pool.find_template("MPC 16th 60%").cloned()
                    } else if low.contains("909") {
                        state.groove_pool.find_template("TR-909").cloned()
                    } else if low.contains("1200") {
                        state.groove_pool.find_template("SP-1200").cloned()
                    } else if low.contains("latin") || low.contains("triplet") {
                        state.groove_pool.find_template("Latin").cloned()
                    } else {
                        None
                    }
                });

            let template = match template {
                Some(t) => t,
                None => {
                    return ToolCallResult::error(format!(
                        "Groove template '{}' not found in Groove Pool",
                        groove_name
                    ));
                }
            };

            let slot_clip = state
                .matrix
                .tracks
                .get_mut(track_id)
                .and_then(|t| t.slots.get_mut(slot_id))
                .and_then(|s| s.clip.as_mut());

            let effective_amount = clip_amount.unwrap_or(state.groove_pool.global_amount);
            let mut local_pool = state.groove_pool.clone();
            local_pool.global_amount = effective_amount;

            let clip_type = if let Some(clip) = slot_clip {
                match clip.pattern {
                    ClipPattern::Midi(ref mut midi) => {
                        midi.notes = local_pool.apply_to_midi_notes(&midi.notes, template.id, 42);
                        "Midi"
                    }
                    ClipPattern::Drum(ref mut drum_pat) => {
                        let g = template.base.step_duration_beats();
                        let vel_amt = template.velocity_amount * effective_amount;
                        let len = drum_pat.step_count.max(1);
                        for (pad_idx, trk) in drum_pat.tracks.iter_mut().enumerate() {
                            for step_idx in 0..len.min(trk.steps.len()) {
                                let step = &mut trk.steps[step_idx];
                                if !step.active {
                                    continue;
                                }
                                let beat = step_idx as f64 * g;
                                let (_, vel_off) = template.evaluate_offsets(beat);
                                let jitter = (((pad_idx * 7 + step_idx * 13) % 9) as f32 - 4.0)
                                    * template.random_amount
                                    * effective_amount;
                                let new_vel = (step.velocity as f32 + vel_off * vel_amt + jitter)
                                    .round()
                                    .clamp(1.0, 127.0) as u8;
                                step.velocity = new_vel;
                            }
                        }
                        "Drums"
                    }
                    ClipPattern::Audio(_) => {
                        if let Some(w_clip) = state.audio_clips.get_mut(&(track_id as u32, slot_id as u32)) {
                            local_pool.commit_to_warper(&mut w_clip.warper, template.id, 42);
                        }
                        "Audio"
                    }
                }
            } else if let Some(w_clip) = state.audio_clips.get_mut(&(track_id as u32, slot_id as u32)) {
                local_pool.commit_to_warper(&mut w_clip.warper, template.id, 42);
                "Audio"
            } else {
                return ToolCallResult::error(format!(
                    "No clip found at track_id={}, slot_id={}",
                    track_id, slot_id
                ));
            };

            ToolCallResult::json(&json!({
                "status": "groove_applied",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_type": clip_type,
                "groove_name": template.name,
                "amount": effective_amount
            }))
        }
        "take_lane_create" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Take Lane");
            let trk = match state.ensure_timeline_track_mut(track_id) {
                Some(t) => t,
                None => return ToolCallResult::error(format!("Track index {} out of range", track_id)),
            };
            let lane_id = trk.comping.add_take_lane(name);
            ToolCallResult::json(&json!({
                "status": "take_lane_created",
                "track_id": track_id,
                "take_lane_id": lane_id,
                "name": name
            }))
        }
        "take_lane_comp_region" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let take_lane_idx = match args.get("take_lane_index").and_then(|v| v.as_u64()) {
                Some(i) => i as u32,
                None => return ToolCallResult::error("Missing 'take_lane_index' parameter"),
            };
            let start_beat = match args.get("start_beat").and_then(|v| v.as_f64()) {
                Some(s) if s.is_finite() => s,
                Some(_) => return ToolCallResult::error("Non-finite 'start_beat' parameter"),
                None => return ToolCallResult::error("Missing 'start_beat' parameter"),
            };
            let end_beat = match args.get("end_beat").and_then(|v| v.as_f64()) {
                Some(e) if e.is_finite() => e,
                Some(_) => return ToolCallResult::error("Non-finite 'end_beat' parameter"),
                None => return ToolCallResult::error("Missing 'end_beat' parameter"),
            };

            let trk = match state.ensure_timeline_track_mut(track_id) {
                Some(t) => t,
                None => return ToolCallResult::error(format!("Track index {} out of range", track_id)),
            };

            // Support either 1-based lane ID or 0-based take_lanes slice index
            let resolved_lane_id = trk
                .comping
                .take_lanes
                .iter()
                .find(|l| l.id == take_lane_idx)
                .or_else(|| trk.comping.take_lanes.get(take_lane_idx as usize))
                .map(|l| l.id);

            let lane_id = match resolved_lane_id {
                Some(id) => id,
                None => {
                    return ToolCallResult::error(format!(
                        "Take lane {} not found on track {}",
                        take_lane_idx, track_id
                    ));
                }
            };

            let mut clips = trk.clips.clone();
            let comp_res = trk
                .comping
                .comp_region_to_main(&mut clips, lane_id, start_beat, end_beat, 5.0);
            let slices_comped = match comp_res {
                Ok(n) => {
                    trk.clips = clips.clone();
                    n
                }
                Err(e) => return ToolCallResult::error(e),
            };
            if let Some(a_trk) = state.timeline.tracks.get_mut(track_id) {
                a_trk.clips = clips;
            }

            ToolCallResult::json(&json!({
                "status": "region_comped",
                "track_id": track_id,
                "take_lane_index": take_lane_idx,
                "take_lane_id": lane_id,
                "start_beat": start_beat,
                "end_beat": end_beat,
                "slices_comped": slices_comped
            }))
        }

        // --- Group 9: Mixer, Returns & Global Scale ---
        "return_track_create" => {
            let name = match args.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => return ToolCallResult::error("Missing 'name' parameter"),
            };
            let init_dev = args.get("initial_device").and_then(|v| v.as_str());
            let ret_id = state.track_manager.add_return_track(name);
            if !state.custom_return_ids.contains(&ret_id) {
                state.custom_return_ids.push(ret_id);
            }
            if let Some(dev_name) = init_dev {
                if let Ok((dev, handle)) = instantiate_managed_device(dev_name, state.sample_rate) {
                    let cat = handle.category();
                    let chain = state.return_device_chains.entry(ret_id).or_default();
                    add_device_to_chain(chain, dev, None);
                    let handles = state.return_device_handles.entry(ret_id).or_default();
                    handles.add_handle(cat, handle, None);
                }
            }
            ToolCallResult::json(&json!({
                "status": "return_track_created",
                "return_track_id": ret_id,
                "name": name,
                "initial_device": init_dev,
                "total_return_tracks": state.track_manager.return_tracks.len()
            }))
        }
        "mixer_set_send" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => match validate_track_id(id as usize) {
                    Ok(v) => v as u32,
                    Err(e) => return e,
                },
                None => return ToolCallResult::error("Missing 'track_id' parameter"),
            };
            let send_index = match args.get("send_index").and_then(|v| v.as_u64()) {
                Some(i) if (i as usize) <= 32 => i as usize,
                Some(i) => return ToolCallResult::error(format!("send_index {} out of range [0..=32]", i)),
                None => return ToolCallResult::error("Missing 'send_index' parameter"),
            };
            let level_db = match args.get("level_db").and_then(|v| v.as_f64()) {
                Some(l) if l.is_finite() => l as f32,
                Some(_) => return ToolCallResult::error("Non-finite 'level_db' parameter"),
                None => return ToolCallResult::error("Missing 'level_db' parameter"),
            };
            let clamped = level_db.clamp(-70.0, 6.0);

            if track_id as usize >= state.project.tracks.len() {
                return ToolCallResult::error(format!("Track {} out of range", track_id));
            }
            let ret_id = state
                .custom_return_ids
                .get(send_index)
                .copied()
                .or_else(|| state.track_manager.return_tracks.get(send_index).map(|r| r.id))
                .unwrap_or(send_index as u32);
            if let Some(trk) = state.ensure_timeline_track_mut(track_id as usize) {
                trk.set_send(ret_id, clamped, SendRoutingMode::PostFader);
            }
            if let Some(p_trk) = state.project.tracks.get_mut(track_id as usize) {
                p_trk.mixer.set_send_db(send_index, clamped);
            }

            ToolCallResult::json(&json!({
                "status": "send_updated",
                "track_id": track_id,
                "send_index": send_index,
                "level_db": clamped
            }))
        }
        "scale_set_global" => {
            let root_val = args.get("root_note");
            let root_num = match root_val {
                Some(v) if v.is_number() => v.as_u64().unwrap_or(0) as u8 % 12,
                Some(v) if v.is_string() => parse_pitch_class(v.as_str().unwrap_or("C")).unwrap_or(0),
                _ => 0,
            };

            let scale_str = match args
                .get("scale_name")
                .or_else(|| args.get("mode"))
                .or_else(|| args.get("scale"))
                .and_then(|v| v.as_str())
            {
                Some(s) => s.to_lowercase(),
                None => return ToolCallResult::error("Missing 'scale_name' parameter"),
            };

            let mode = parse_scale_mode_name(&scale_str);

            let lock = args.get("scale_lock").and_then(|v| v.as_bool()).unwrap_or(false);
            state.scale = GlobalScale::new(root_num, mode).with_lock(lock);
            let _ = state.lom_client.call("set_scale", json!({
                "root_note": root_num,
                "scale_name": state.scale.mode.name(),
                "scale_lock": lock,
            }));

            ToolCallResult::json(&json!({
                "status": "scale_updated",
                "root_note": state.scale.root_name(),
                "scale_name": state.scale.mode.name(),
                "scale_lock": state.scale.scale_lock,
                "display_name": state.scale.display_name()
            }))
        }

        // --- Group 15: Oxide Studio Live GUI RPC Bridge (15 tools) ---
        "live12_status" => {
            let is_online = state.lom_client.is_online();
            ToolCallResult::json(&json!({
                "connected": is_online,
                "target": "oxide_studio_gui_rpc",
                "host": state.lom_client.host,
                "port": state.lom_client.port,
                "status": if is_online { "online" } else { "offline" },
                "native_fallback_ready": true
            }))
        }
        "live12_get_song_info" => {
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "get_song_info",
                json!({}),
                || json!({
                    "tempo": state.bpm,
                    "is_playing": state.is_playing,
                    "current_song_time": state.playhead_beat,
                    "scale_name": state.scale.mode.name(),
                    "root_note": state.scale.root,
                    "track_count": state.project.tracks.len(),
                    "return_track_count": state.track_manager.return_tracks.len(),
                    "scene_count": state.matrix.scenes.len()
                }),
            );
            ToolCallResult::json(&res)
        }
        "live12_create_midi_track" => {
            if state.project.tracks.len() > MAX_TRACK_ID {
                return ToolCallResult::error(format!(
                    "Cannot exceed maximum track count ({})",
                    MAX_TRACK_ID + 1
                ));
            }
            let index = args.get("index").and_then(|v| v.as_u64()).map(|i| i as usize);
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "create_midi_track",
                json!({ "index": index }),
                || {
                    let insert_at = index
                        .map(|i| i.min(state.project.tracks.len()))
                        .unwrap_or(state.project.tracks.len());
                    let track_name = format!("MIDI Track {}", insert_at);
                    let trk = TrackDefinition::new_synth(insert_at as u32, &track_name);
                    insert_track_shifting_state(state, insert_at, trk, &track_name, "MidiSynth");
                    json!({
                        "index": insert_at,
                        "track_type": "MidiSynth",
                        "track_count": state.project.tracks.len()
                    })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_create_audio_track" => {
            if state.project.tracks.len() > MAX_TRACK_ID {
                return ToolCallResult::error(format!(
                    "Cannot exceed maximum track count ({})",
                    MAX_TRACK_ID + 1
                ));
            }
            let index = args.get("index").and_then(|v| v.as_u64()).map(|i| i as usize);
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "create_audio_track",
                json!({ "index": index }),
                || {
                    let insert_at = index
                        .map(|i| i.min(state.project.tracks.len()))
                        .unwrap_or(state.project.tracks.len());
                    let track_name = format!("Audio Track {}", insert_at);
                    let trk = TrackDefinition::new_audio(insert_at as u32, &track_name);
                    insert_track_shifting_state(state, insert_at, trk, &track_name, "Audio");
                    json!({
                        "index": insert_at,
                        "track_type": "Audio",
                        "track_count": state.project.tracks.len()
                    })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_create_return_track" => {
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "create_return_track",
                json!({}),
                || {
                    let ret_id = state.track_manager.add_return_track("New Return");
                    if !state.custom_return_ids.contains(&ret_id) {
                        state.custom_return_ids.push(ret_id);
                    }
                    json!({ "return_track_id": ret_id, "return_track_count": state.track_manager.return_tracks.len() })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_load_browser_item" => {
            let track_index = match validate_track_id(
                args.get("track_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize,
            ) {
                Ok(v) => v as u32,
                Err(e) => return e,
            };
            let uri = args.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "load_browser_item",
                json!({ "track_index": track_index, "uri": uri }),
                || {
                    let dev_hint = {
                        let clean = uri.trim().to_ascii_lowercase();
                        let candidates = [
                            ("wavetable", "Wavetable"),
                            ("operator", "Operator"),
                            ("drift", "Drift"),
                            ("meld", "Meld"),
                            ("analog", "Analog"),
                            ("collision", "Collision"),
                            ("tension", "Tension"),
                            ("electric", "Electric"),
                            ("simpler", "Simpler"),
                            ("drum", "DrumRack"),
                            ("eq", "EQEight"),
                            ("glue", "GlueCompressor"),
                            ("roar", "Roar"),
                            ("autofilter", "AutoFilter"),
                            ("ott", "MultibandDynamics"),
                            ("reverb", "HybridReverb"),
                            ("echo", "Echo"),
                            ("delay", "Echo"),
                            ("synth", "Wavetable"),
                        ];
                        candidates
                            .iter()
                            .find(|(k, _)| clean.contains(k))
                            .map(|(_, v)| v.to_string())
                            .or_else(|| {
                                state
                                    .browser
                                    .search(uri, None, None)
                                    .first()
                                    .map(|item| item.name.clone())
                            })
                    };

                    let mut loaded_device = None;
                    let mut load_error = None;
                    if let Some(hint) = dev_hint {
                        if let Ok((dev, handle)) = instantiate_managed_device(&hint, state.sample_rate) {
                            let cat = handle.category();
                            let dev_name = handle.name();
                            let chain = state.device_chains.entry(track_index).or_default();
                            add_device_to_chain(chain, dev, None);
                            let handles = state.device_handles.entry(track_index).or_default();
                            handles.add_handle(cat, handle, None);
                            loaded_device = Some(dev_name);
                        } else {
                            load_error = Some(format!("Failed to instantiate device '{}'", hint));
                        }
                    } else {
                        load_error = Some(format!("Browser item '{}' not found", uri));
                    }
                    let mut out = json!({
                        "loaded": loaded_device.is_some(),
                        "track_index": track_index,
                        "uri": uri,
                        "device": loaded_device
                    });
                    if let Some(err) = load_error {
                        if let Some(obj) = out.as_object_mut() {
                            obj.insert("error".to_string(), json!(err));
                        }
                    }
                    out
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_create_clip" => {
            let track_index = match validate_track_id(args.get("track_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize) {
                Ok(v) => v as u64,
                Err(e) => return e,
            };
            let slot_index = match validate_slot_id(args.get("slot_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize) {
                Ok(v) => v as u64,
                Err(e) => return e,
            };
            let length_beats = args.get("length_beats").and_then(|v| v.as_f64()).unwrap_or(4.0);
            if !length_beats.is_finite() || length_beats <= 0.0 || length_beats > (MAX_TOTAL_BARS * 4) as f64 {
                return ToolCallResult::error(format!(
                    "length_beats {} out of valid range (0.0..={}]",
                    length_beats,
                    MAX_TOTAL_BARS * 4
                ));
            }
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "create_clip",
                json!({ "track_index": track_index, "slot_index": slot_index, "length_beats": length_beats }),
                || {
                    let clip_id = ((track_index as usize * 100) + slot_index as usize + 1) as u32;
                    state.set_session_slot_clip(
                        track_index as usize,
                        slot_index as usize,
                        SessionClip::new(clip_id, "Live12 Clip", ClipPattern::Midi(MidiPattern::new(length_beats))),
                    );
                    json!({ "created": true, "track_index": track_index, "slot_index": slot_index, "length_beats": length_beats })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_add_notes" => {
            let track_index = match validate_track_id(args.get("track_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize) {
                Ok(v) => v,
                Err(e) => return e,
            };
            let slot_index = match validate_slot_id(args.get("slot_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize) {
                Ok(v) => v,
                Err(e) => return e,
            };
            let notes = args.get("notes").cloned().unwrap_or(json!([]));
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "add_notes",
                json!({ "track_index": track_index, "slot_index": slot_index, "notes": notes }),
                || {
                    let mut parsed_notes = Vec::new();
                    if let Some(arr) = notes.as_array() {
                        for item in arr {
                            let pitch = item.get("pitch").and_then(|v| v.as_u64()).unwrap_or(60).min(127) as u8;
                            let vel = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100).clamp(1, 127) as u8;
                            let raw_start = item.get("start_beat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let raw_dur = item.get("duration_beats").and_then(|v| v.as_f64()).unwrap_or(0.5);
                            let start = if raw_start.is_finite() { raw_start.max(0.0) } else { 0.0 };
                            let dur = if raw_dur.is_finite() { raw_dur.max(0.01) } else { 0.5 };
                            parsed_notes.push(MidiNote::new(pitch, vel, start, dur));
                        }
                    }
                    let count = parsed_notes.len();

                    while state.matrix.tracks.len() <= track_index {
                        let tid = state.matrix.tracks.len() as u32;
                        state.matrix.tracks.push(oxide_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
                    }
                    let trk = &mut state.matrix.tracks[track_index];
                    while trk.slots.len() <= slot_index {
                        trk.slots.push(oxide_timeline::SessionSlot::default());
                    }
                    if trk.slots[slot_index].clip.is_none() {
                        let clip_id = ((track_index * 100) + slot_index + 1) as u32;
                        trk.slots[slot_index].set_clip(Some(SessionClip::new(
                            clip_id,
                            "Live12 Clip",
                            ClipPattern::Midi(MidiPattern::new(4.0)),
                        )));
                        state.audio_clips.remove(&(track_index as u32, slot_index as u32));
                    }
                    if let Some(ref mut clip) = trk.slots[slot_index].clip {
                        if let ClipPattern::Midi(ref mut pat) = clip.pattern {
                            pat.notes.extend(parsed_notes);
                        }
                    }

                    json!({ "notes_added": count, "track_index": track_index, "slot_index": slot_index })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_fire_clip" => {
            let track_index = args.get("track_index").and_then(|v| v.as_u64()).unwrap_or(0);
            let slot_index = args.get("slot_index").and_then(|v| v.as_u64()).unwrap_or(0);
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "fire_clip",
                json!({ "track_index": track_index, "slot_index": slot_index }),
                || {
                    if let Some(track) = state.matrix.tracks.get_mut(track_index as usize) {
                        let _ = track.trigger_slot(slot_index as usize, state.playhead_beat, QuantizeMode::OneBar);
                    }
                    json!({ "fired": true, "track_index": track_index, "slot_index": slot_index })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_fire_scene" => {
            let scene_index = args.get("scene_index").and_then(|v| v.as_u64()).unwrap_or(0);
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "fire_scene",
                json!({ "scene_index": scene_index }),
                || {
                    let _ = state.matrix.trigger_scene(scene_index as usize, state.playhead_beat, QuantizeMode::OneBar);
                    json!({ "fired": true, "scene_index": scene_index })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_set_device_param" => {
            let track_index = match validate_track_id(args.get("track_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize) {
                Ok(v) => v as u32,
                Err(e) => return e,
            };
            let device_index = args.get("device_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let param_val = args
                .get("param_index")
                .or_else(|| args.get("param"))
                .or_else(|| args.get("param_name"));
            let param_key = match param_val {
                Some(v) if v.is_string() => v.as_str().unwrap_or("0").to_string(),
                Some(v) if v.is_u64() => v.as_u64().unwrap_or(0).to_string(),
                _ => "0".to_string(),
            };
            let param_index_json = if let Some(n) = param_val.and_then(|v| v.as_u64()) {
                json!(n)
            } else if let Ok(n) = param_key.trim().parse::<u64>() {
                json!(n)
            } else {
                json!(param_key)
            };
            let value_raw = args.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if !value_raw.is_finite() {
                return ToolCallResult::error("Non-finite 'value' parameter");
            }
            let value = value_raw as f32;
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "set_device_param",
                json!({
                    "track_index": track_index,
                    "device_index": device_index,
                    "param_index": param_index_json,
                    "value": value
                }),
                || match set_track_device_param_helper(state, track_index, device_index, &param_key, value) {
                    Ok((device_name, resolved_param, applied_value)) => json!({
                        "updated": true,
                        "track_index": track_index,
                        "device_index": device_index,
                        "device_name": device_name,
                        "param": resolved_param,
                        "value": applied_value
                    }),
                    Err(err) => json!({
                        "updated": false,
                        "track_index": track_index,
                        "device_index": device_index,
                        "error": err
                    }),
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_get_track_devices" => {
            let track_index = args.get("track_index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "get_track_devices",
                json!({ "track_index": track_index }),
                || {
                    let chain = state.device_chains.entry(track_index).or_default();
                    let handles = state.device_handles.get(&track_index);
                    dump_device_chain_detailed(chain, handles)
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_set_tempo" => {
            let bpm_raw = match args.get("bpm").and_then(|v| v.as_f64()) {
                Some(b) => b,
                None => return ToolCallResult::error("Missing 'bpm' parameter"),
            };
            let bpm = match validate_bpm(bpm_raw) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "set_tempo",
                json!({ "bpm": bpm }),
                || {
                    state.bpm = bpm;
                    state.project.transport.bpm = bpm;
                    state.delay.set_tempo(bpm);
                    json!({ "tempo": state.bpm })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_set_scale" => {
            let root_note = args.get("root_note").and_then(|v| v.as_u64()).unwrap_or(0) as u8 % 12;
            let scale_name = args.get("scale_name").and_then(|v| v.as_str()).unwrap_or("Major");
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "set_scale",
                json!({ "root_note": root_note, "scale_name": scale_name }),
                || {
                    let mode = parse_scale_mode_name(scale_name);
                    state.scale = GlobalScale::new(root_note, mode);
                    json!({ "root_note": root_note, "scale_name": scale_name })
                },
            );
            ToolCallResult::json(&res)
        }
        "live12_exec_lom_query" => {
            let query = match args.get("query").and_then(|v| v.as_str()) {
                Some(q) => q,
                None => return ToolCallResult::error("Missing 'query' parameter"),
            };
            let lom_client = state.lom_client.clone();
            let res = lom_client.execute_with_fallback(
                "exec_lom_query",
                json!({ "query": query }),
                || {
                    let q_lower = query.trim().to_ascii_lowercase();
                    let value = if q_lower == "song.tempo" || q_lower == "tempo" || q_lower == "bpm" {
                        json!(state.bpm)
                    } else if q_lower == "song.is_playing" || q_lower == "is_playing" {
                        json!(state.is_playing)
                    } else if q_lower == "song.tracks" || q_lower == "tracks" {
                        json!(state.project.tracks.iter().map(|t| t.name.clone()).collect::<Vec<_>>())
                    } else if q_lower.contains("tracks[") && q_lower.ends_with(".name") {
                        let idx = q_lower
                            .split("tracks[")
                            .nth(1)
                            .and_then(|s| s.split(']').next())
                            .and_then(|n| n.parse::<usize>().ok());
                        match idx.and_then(|i| state.project.tracks.get(i)) {
                            Some(t) => json!(t.name),
                            None => Value::Null,
                        }
                    } else if q_lower.contains("track_count") {
                        json!(state.project.tracks.len())
                    } else if q_lower.contains("scale") {
                        json!({
                            "root_note": state.scale.root_name(),
                            "scale_name": state.scale.mode.name()
                        })
                    } else if q_lower.contains("return") {
                        json!(state.track_manager.return_tracks.iter().map(|r| r.name.clone()).collect::<Vec<_>>())
                    } else if q_lower.contains("master") {
                        json!(state.project.master_track)
                    } else {
                        json!({
                            "tempo": state.bpm,
                            "is_playing": state.is_playing,
                            "track_count": state.project.tracks.len(),
                            "tracks": state.project.tracks.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
                        })
                    };
                    json!({ "query": query, "result": value })
                },
            );
            ToolCallResult::json(&res)
        }

        _ => ToolCallResult::error(format!("Tool '{}' not recognized", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_als_and_stems_mcp_tools_dispatch() {
        let mut state = DawState::new();
        state.bpm = 124.0;
        state.project.transport.bpm = 124.0;

        let temp_dir = std::env::temp_dir().join(format!("mcp_test_als_stems_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        // 1. project_export_als
        let als_path = temp_dir.join("test_session.als");
        let export_res = call_tool(
            &mut state,
            "project_export_als",
            &json!({ "path": als_path.to_str().unwrap() }),
        );
        assert!(!export_res.is_error, "project_export_als must succeed: {:?}", export_res);
        assert!(als_path.exists(), "ALS file must exist on disk");

        // 2. project_import_als
        state.bpm = 90.0;
        let import_res = call_tool(
            &mut state,
            "project_import_als",
            &json!({ "path": als_path.to_str().unwrap() }),
        );
        assert!(!import_res.is_error, "project_import_als must succeed: {:?}", import_res);
        assert!((state.bpm - 124.0).abs() < 1e-3, "Imported BPM should be 124.0, got {}", state.bpm);

        // 3. bounce_export_stems
        let stems_dir = temp_dir.join("stems_out");
        let bounce_res = call_tool(
            &mut state,
            "bounce_export_stems",
            &json!({ "output_dir": stems_dir.to_str().unwrap() }),
        );
        assert!(!bounce_res.is_error, "bounce_export_stems must succeed: {:?}", bounce_res);
        assert!(stems_dir.exists(), "Stems output directory must exist");

        // 4. stems_separate_remix
        let test_wav = temp_dir.join("source.wav");
        let renderer = DemoArrangementRenderer::new(44100.0, 120.0);
        let audio = renderer.render_demo_seconds(4.0);
        WavWriter::write_wav_16bit(&test_wav, 44100, 2, &audio).unwrap();

        let remix_dir = temp_dir.join("remix_out");
        let remix_res = call_tool(
            &mut state,
            "stems_separate_remix",
            &json!({
                "input_wav": test_wav.to_str().unwrap(),
                "output_dir": remix_dir.to_str().unwrap()
            }),
        );
        assert!(!remix_res.is_error, "stems_separate_remix must succeed: {:?}", remix_res);
        assert!(remix_dir.join("remix_drums.wav").exists(), "remix_drums.wav must exist");
        assert!(remix_dir.join("remix_bass.wav").exists(), "remix_bass.wav must exist");
        assert!(remix_dir.join("remix_vocals.wav").exists(), "remix_vocals.wav must exist");
        assert!(remix_dir.join("remix_other.wav").exists(), "remix_other.wav must exist");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_tools_reject_out_of_bounds_and_oom_inputs() {
        let mut state = DawState::new();
        assert!(call_tool(&mut state, "clip_create_midi", &json!({"track_id": 999999, "slot_id": 0, "notes": []})).is_error);
        assert!(call_tool(&mut state, "clip_create_midi", &json!({"track_id": 0, "slot_id": 999999, "notes": []})).is_error);
        assert!(call_tool(&mut state, "clip_create_midi", &json!({"track_id": 0, "slot_id": 0, "length_beats": 0.0, "notes": []})).is_error);
        assert!(call_tool(&mut state, "clip_create_midi", &json!({"track_id": 0, "slot_id": 0, "notes": [{"pitch": 60, "start_beat": 0.0, "duration_beats": 0.0}]})).is_error);
        assert!(call_tool(&mut state, "generate_chord_progression_clip", &json!({"track_id": 500, "slot_id": 0})).is_error);
        assert!(call_tool(&mut state, "device_add_to_track", &json!({"track_id": 500, "device_type": "Echo"})).is_error);
        assert!(call_tool(&mut state, "build_song_arrangement", &json!({"total_bars": 9999})).is_error);
        assert!(call_tool(&mut state, "transport_set_bpm", &json!({"bpm": 5.0})).is_error);
        assert!(call_tool(&mut state, "live12_set_tempo", &json!({"bpm": 1500.0})).is_error);
    }

    #[test]
    fn test_live12_set_device_param_sends_integer_param_index() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let captured_srv = std::sync::Arc::clone(&captured);

        let handle = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                captured_srv.lock().unwrap().push(req);
                let resp = json!({"ok": true, "result": {"updated": true}}).to_string();
                stream.write_all(format!("{}\n", resp).as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });

        let mut state = DawState::new();
        state.lom_client.host = "127.0.0.1".to_string();
        state.lom_client.port = port;

        let res1 = call_tool(&mut state, "live12_set_device_param", &json!({
            "track_index": 0, "device_index": 0, "param_index": 3, "value": 0.75
        }));
        assert!(!res1.is_error);

        let res2 = call_tool(&mut state, "live12_set_device_param", &json!({
            "track_index": 0, "device_index": 0, "param_index": "2", "value": 0.5
        }));
        assert!(!res2.is_error);

        handle.join().unwrap();
        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0]["params"]["param_index"].is_u64(), "numeric param_index must be JSON integer, got {:?}", reqs[0]["params"]);
        assert_eq!(reqs[0]["params"]["param_index"].as_u64(), Some(3));
        assert!(reqs[1]["params"]["param_index"].is_u64(), "numeric string param_index must be JSON integer, got {:?}", reqs[1]["params"]);
        assert_eq!(reqs[1]["params"]["param_index"].as_u64(), Some(2));
    }

    #[test]
    fn test_live12_create_tracks_shifts_indices_and_active_ids() {
        let mut state = DawState::new();
        state.active_synth_track_id = 0;
        state.active_drum_track_id = 1;
        state.mixer.tracks[0].set_volume_db(-5.5);
        assert!(!call_tool(&mut state, "device_add_to_track", &json!({"track_id": 0, "device_type": "AutoFilter"})).is_error);
        let dummy_clip = WarpedAudioClip::new("a.wav", 44100, 1, vec![0.1; 1024], 128.0);
        state.audio_clips.insert((0, 2), dummy_clip);

        let res = call_tool(&mut state, "live12_create_midi_track", &json!({"index": 0}));
        assert!(!res.is_error);
        assert_eq!(state.active_synth_track_id, 1);
        assert_eq!(state.active_drum_track_id, 2);
        assert!(state.device_handles.get(&0).is_none());
        assert_eq!(state.device_handles.get(&1).map(|h| h.audio_effects.len()), Some(1));
        assert!(state.audio_clips.contains_key(&(1, 2)));
        assert!((state.mixer.tracks[1].volume_db - (-5.5)).abs() < 0.1);
    }

    #[test]
    fn test_build_song_arrangement_uses_techno_peak_and_strips_clap_in_intro() {
        let mut state = DawState::new();
        let res = call_tool(&mut state, "build_song_arrangement", &json!({
            "genre": "techno", "key": "Fm", "bpm": 134.0, "total_bars": 16
        }));
        assert!(!res.is_error, "build_song_arrangement failed: {:?}", res);

        let drum_trk = &state.timeline.tracks[1];
        assert_eq!(drum_trk.clips.len(), 4);
        if let ClipPattern::Drum(ref intro_pat) = drum_trk.clips[0].pattern {
            assert!(
                intro_pat.tracks[6].steps.iter().all(|s| !s.active),
                "Intro should strip Clap on pad 6"
            );
        } else {
            panic!("Expected Drum pattern in intro");
        }
        if let ClipPattern::Drum(ref drop_pat) = drum_trk.clips[2].pattern {
            assert!(drop_pat.tracks[0].steps.iter().any(|s| s.active), "Drop should have Kick on pad 0");
            assert!(drop_pat.tracks[6].steps.iter().any(|s| s.active), "Drop should have Clap on pad 6");
            assert!(drop_pat.tracks[8].steps.iter().any(|s| s.active), "Drop should have Closed HH on pad 8");
        } else {
            panic!("Expected Drum pattern in drop");
        }
    }

    #[test]
    fn test_device_reorder_global_and_cross_category_rejected() {
        let mut state = DawState::new();
        assert!(!call_tool(&mut state, "device_add_to_track", &json!({"track_id": 0, "device_type": "Arpeggiator"})).is_error);
        assert!(!call_tool(&mut state, "device_add_to_track", &json!({"track_id": 0, "device_type": "Wavetable"})).is_error);
        let eq = call_tool(&mut state, "device_add_to_track", &json!({"track_id": 0, "device_type": "EQEight"}));
        let eq_v: Value = serde_json::from_str(&eq.content[0].text).unwrap();
        assert_eq!(eq_v["device_index"], 2);
        let echo = call_tool(&mut state, "device_add_to_track", &json!({"track_id": 0, "device_type": "Echo"}));
        let echo_v: Value = serde_json::from_str(&echo.content[0].text).unwrap();
        assert_eq!(echo_v["device_index"], 3);

        // Cross-category move must fail cleanly
        assert!(call_tool(&mut state, "device_reorder", &json!({"track_id": 0, "from_index": 0, "to_index": 2})).is_error);

        // Same-category audio reorder using global indices 2 <-> 3
        let ok = call_tool(&mut state, "device_reorder", &json!({"track_id": 0, "from_index": 2, "to_index": 3}));
        assert!(!ok.is_error, "valid audio reorder should succeed: {:?}", ok);
        assert_eq!(state.device_handles[&0].audio_effects[0].name(), "Echo");
        assert_eq!(state.device_handles[&0].audio_effects[1].name(), "EQ Eight");
    }

    #[test]
    fn test_synth_load_preset_invalid_track_preserves_state_and_scale_lock() {
        let mut state = DawState::new();
        let before_values = state.synth.params.values;
        let bad = call_tool(&mut state, "synth_load_preset", &json!({"track_id": 999, "preset": "Acid Bass"}));
        assert!(bad.is_error);
        assert_eq!(state.synth.params.values, before_values);

        // Global Major scale lock must produce notes matching C Major mask (never Dorian b3/b7)
        assert!(!call_tool(&mut state, "scale_set_global", &json!({"root_note": "C", "scale_name": "Major", "scale_lock": true})).is_error);
        assert!(!call_tool(&mut state, "midi_generate_clip", &json!({
            "track_id": 0, "slot_id": 0, "generator": "shape", "params": {"shape": "linear_rise"}
        })).is_error);
        let clip = state.matrix.tracks[0].slots[0].clip.as_ref().unwrap();
        let major_mask = ScaleMode::Major.degree_mask();
        if let ClipPattern::Midi(ref mp) = clip.pattern {
            assert!(!mp.notes.is_empty());
            for n in &mp.notes {
                let pc = n.pitch % 12;
                assert_ne!((major_mask >> pc) & 1, 0, "note pitch {} (pc {}) not in C Major", n.pitch, pc);
            }
        }
    }

    #[test]
    fn test_chord_bars_humanize_and_simpler_device_index() {
        let mut state = DawState::new();
        let chords = call_tool(&mut state, "generate_chord_progression_clip", &json!({
            "track_id": 0, "slot_id": 0, "progression": "I-V-vi-IV", "bars": 2, "humanize": 0.9
        }));
        assert!(!chords.is_error, "chords failed: {:?}", chords);
        let clip = state.matrix.tracks[0].slots[0].clip.as_ref().unwrap();
        if let ClipPattern::Midi(ref mp) = clip.pattern {
            assert!((mp.length_beats - 8.0).abs() < 1e-6, "bars=2 must equal 8.0 beats");
            assert!(mp.notes.iter().any(|n| n.velocity != 100), "humanize=0.9 should vary velocities");
        }

        // Invalid simpler_device_index must error cleanly
        assert!(!call_tool(&mut state, "device_add_to_track", &json!({"track_id": 0, "device_type": "Arpeggiator"})).is_error);
        assert!(!call_tool(&mut state, "device_add_to_track", &json!({"track_id": 0, "device_type": "Simpler"})).is_error);
        assert!(call_tool(&mut state, "simpler_slice_to_drum_rack", &json!({"track_id": 0, "simpler_device_index": 99})).is_error);

        // Load audio into Simpler at global index 1 and slice explicitly
        if let Some(ManagedDeviceHandle::Simpler(ref s_arc)) = state.device_handles[&0].get_at(1) {
            let mut s = s_arc.lock().unwrap_or_else(|e| e.into_inner());
            let buf: Vec<f32> = (0..4096).map(|i| if i % 1024 < 64 { 0.8 } else { 0.0 }).collect();
            s.load_sample(buf, 44100, 1);
        }
        let sliced = call_tool(&mut state, "simpler_slice_to_drum_rack", &json!({
            "track_id": 0, "simpler_device_index": 1
        }));
        assert!(!sliced.is_error, "explicit simpler_device_index 1 should slice: {:?}", sliced);
    }

    #[test]
    fn test_mgw2_build_song_arrangement_preserves_extra_tracks() {
        let mut state = DawState::new();
        assert!(!call_tool(&mut state, "track_create_synth", &json!({"name": "Extra Synth"})).is_error);
        assert!(!call_tool(&mut state, "track_create_synth", &json!({"name": "Extra Pad"})).is_error);
        assert_eq!(state.project.tracks.len(), 4);

        let res = call_tool(&mut state, "build_song_arrangement", &json!({
            "genre": "techno", "key": "Am", "bpm": 130.0, "total_bars": 8
        }));
        assert!(!res.is_error, "{:?}", res);
        assert_eq!(
            state.timeline.tracks.len(),
            state.project.tracks.len(),
            "build_song_arrangement must preserve existing tracks beyond 3"
        );
    }

    #[test]
    fn test_mgw3_live12_load_browser_item_rejects_oob_track_index() {
        let mut state = DawState::new();
        state.lom_client = crate::bridge::TcpLomClient::new("127.0.0.1", 1);
        let res = call_tool(&mut state, "live12_load_browser_item", &json!({
            "track_index": 999999,
            "uri": "Roar"
        }));
        assert!(res.is_error, "oob track_index must return error, got {:?}", res);
        assert!(!state.device_chains.contains_key(&999999));

        let wrap_res = call_tool(&mut state, "live12_load_browser_item", &json!({
            "track_index": (1u64 << 32),
            "uri": "Roar"
        }));
        assert!(wrap_res.is_error, "u32-wrapping track_index must error, got {:?}", wrap_res);
    }
}
