//! Tool catalogue, schema declarations, and dispatch logic for DAW autonomous control.

use std::fs;
use std::path::Path;
use serde_json::{json, Value};

use ableton_bounce::{DemoArrangementRenderer, WavWriter};
use ableton_dsp::presets::{load_mapping_catalog, load_preset_catalog};
use ableton_plugin_scanner::PluginScannerSupervisor;
use ableton_project::{DAWProject, TrackDefinition};
use ableton_timeline::{
    ArrangementClip, AutomationLane, AutomationPoint, ClipPattern,
    DrumStep, DrumStepPattern, MidiNote, MidiPattern, QuantizeMode, SessionClip,
};

use crate::analysis::AudioAnalysisMetrics;
use crate::lilypond::{parse_lilypond, sequence_to_clip_pattern};
use crate::music_theory::{
    bjorklund, build_chord_notes, generate_drum_groove, get_scale_intervals,
    parse_pitch_class, parse_roman_degree,
};
use crate::protocol::{Tool, ToolCallResult, ToolInputSchema};
use crate::state::DawState;

/// Return complete catalogue of all registered MCP tools.
pub fn get_tool_definitions() -> Vec<Tool> {
    vec![
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
                    "name": { "type": "string", "description": "Track name" }
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
                    "preset_name": { "type": "string", "description": "Preset filename, e.g. 'wowbass.preset' or 'acid.preset'" }
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
    ]
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
            match DAWProject::load_from_file(Path::new(path_str)) {
                Ok(proj) => {
                    let tracks_count = proj.tracks.len();
                    let bpm = proj.transport.bpm;
                    state.bpm = bpm;
                    state.project = proj;
                    state.sync_state_from_project();
                    state.delay.set_tempo(bpm);
                    state.last_saved_path = Some(path_str.to_string());
                    ToolCallResult::json(&json!({
                        "status": "loaded",
                        "path": path_str,
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
                .map(|s| s.to_string())
                .or_else(|| state.last_saved_path.clone())
                .unwrap_or_else(|| "/tmp/ableton_session.json".to_string());

            state.sync_project_from_state();
            match state.project.save_to_file(Path::new(&target_path)) {
                Ok(_) => {
                    state.last_saved_path = Some(target_path.clone());
                    ToolCallResult::json(&json!({
                        "status": "saved",
                        "path": target_path,
                        "tracks": state.project.tracks.len(),
                        "bpm": state.project.transport.bpm,
                    }))
                }
                Err(e) => ToolCallResult::error(format!("Failed to save project: {}", e)),
            }
        }
        "transport_play" => {
            state.is_playing = true;
            ToolCallResult::json(&json!({
                "is_playing": true,
                "playhead_beat": state.playhead_beat,
            }))
        }
        "transport_stop" => {
            state.is_playing = false;
            state.playhead_beat = 0.0;
            state.synth.all_notes_off();
            ToolCallResult::json(&json!({
                "is_playing": false,
                "playhead_beat": 0.0,
            }))
        }
        "transport_set_bpm" => {
            let bpm = match args.get("bpm").and_then(|v| v.as_f64()) {
                Some(b) => b,
                None => return ToolCallResult::error("Missing 'bpm' parameter"),
            };
            state.bpm = bpm;
            state.project.transport.bpm = bpm;
            state.delay.set_tempo(bpm);
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
            let track = TrackDefinition::new_synth(id, name);
            state.project.tracks.push(track);
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
            if track_id < state.project.tracks.len() {
                let removed = state.project.tracks.remove(track_id);
                ToolCallResult::json(&json!({
                    "status": "deleted",
                    "track_id": track_id,
                    "name": removed.name
                }))
            } else {
                ToolCallResult::error(format!("Track index {} out of range", track_id))
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
            let success = state.synth.load_preset(preset_name);
            if success {
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
            let is_norm = args.get("is_normalized").and_then(|v| v.as_bool()).unwrap_or(false);

            if let Some(param_str) = args.get("parameter").and_then(|v| v.as_str()) {
                if !ableton_dsp::PARAMETER_DESCRIPTORS.iter().any(|d| d.name == param_str) {
                    return ToolCallResult::error(format!("Unknown synth parameter: '{}'", param_str));
                }
                if is_norm {
                    state.synth.set_parameter_normalized(param_str, param_val);
                } else {
                    state.synth.set_parameter(param_str, param_val);
                }
                ToolCallResult::json(&json!({
                    "parameter": param_str,
                    "value": state.synth.params.get_value(param_str),
                    "normalized": state.synth.params.get_control_value(param_str)
                }))
            } else if let Some(param_idx) = args.get("parameter").and_then(|v| v.as_u64()) {
                if param_idx < 30 {
                    let desc = &ableton_dsp::PARAMETER_DESCRIPTORS[param_idx as usize];
                    if is_norm {
                        state.synth.set_parameter_normalized(desc.name, param_val);
                    } else {
                        state.synth.set_parameter(desc.name, param_val);
                    }
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
            if let Some(wav_path) = args.get("wav_path").and_then(|v| v.as_str()) {
                if let Err(e) = state.drum_rack.load_sample_pad(pad_idx, wav_path) {
                    return ToolCallResult::error(format!("Failed to load WAV into pad {}: {}", pad_idx, e));
                }
            }

            let pad = &state.drum_rack.pads[pad_idx];
            ToolCallResult::json(&json!({
                "pad_index": pad_idx,
                "name": pad.name,
                "pitch_semitones": pad.pitch_semitones,
                "decay_seconds": pad.decay_seconds,
                "volume": pad.volume,
                "pan": pad.pan,
                "choke_group": pad.choke_group
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
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("MIDI Clip");
            let len_beats = args.get("length_beats").and_then(|v| v.as_f64()).unwrap_or(4.0);

            let mut notes = Vec::new();
            if let Some(arr) = args.get("notes").and_then(|v| v.as_array()) {
                for item in arr {
                    let pitch = item.get("pitch").and_then(|v| v.as_u64()).unwrap_or(60) as u8;
                    let vel = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
                    let start = item.get("start_beat").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let dur = item.get("duration_beats").and_then(|v| v.as_f64()).unwrap_or(1.0);
                    notes.push(MidiNote::new(pitch, vel, start, dur));
                }
            }

            let pattern = ClipPattern::Midi(MidiPattern {
                length_beats: len_beats,
                notes,
            });
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, name, pattern);

            // Attach to SessionMatrix
            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

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
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
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
                    let pad = item.get("pad_index").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                    let step = item.get("step_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let vel = item.get("velocity").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
                    let acc = item.get("accent").and_then(|v| v.as_bool()).unwrap_or(false);
                    drum_pat.set_step(pad, step, DrumStep::new(true, vel, 36 + pad).with_accent(acc));
                }
            }

            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, name, ClipPattern::Drum(drum_pat));

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

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
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
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

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

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
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let clip_id = args.get("clip_id").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let start = match args.get("start_beat").and_then(|v| v.as_f64()) {
                Some(b) => b,
                None => return ToolCallResult::error("Missing 'start_beat'"),
            };
            let dur = match args.get("duration_beats").and_then(|v| v.as_f64()) {
                Some(d) => d,
                None => return ToolCallResult::error("Missing 'duration_beats'"),
            };
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Arrangement Clip");

            let clip = ArrangementClip::new(clip_id, name, start, dur, ClipPattern::Midi(MidiPattern::new(dur)));

            while state.timeline.tracks.len() <= track_id {
                let tid = state.timeline.tracks.len() as u32;
                state.timeline.tracks.push(ableton_timeline::ArrangementTrack::new(tid, format!("Track {}", tid), "MidiSynth"));
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
                Some(id) => id as usize,
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
                    lane.add_point(AutomationPoint::linear(beat, val));
                }
            }

            while state.timeline.tracks.len() <= track_id {
                let tid = state.timeline.tracks.len() as u32;
                state.timeline.tracks.push(ableton_timeline::ArrangementTrack::new(tid, format!("Track {}", tid), "MidiSynth"));
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
            if track_id >= state.mixer.tracks.len() {
                return ToolCallResult::error(format!("Track {} out of mixer range", track_id));
            }
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
            ToolCallResult::json(&json!({
                "track_id": track_id,
                "volume_db": trk.volume_db,
                "pan": trk.pan,
                "mute": trk.mute,
                "solo": trk.solo
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
            ToolCallResult::json(&json!({
                "drive_linear": state.saturator.drive_linear,
                "wet": state.saturator.wet,
                "output_gain": state.saturator.output_gain
            }))
        }
        "fx_set_delay" => {
            if let Some(sub) = args.get("subdivision").and_then(|v| v.as_str()) {
                let s = match sub {
                    "1/16" => ableton_fx::DelaySubdivision::Sixteenth,
                    "3/16" => ableton_fx::DelaySubdivision::SixteenthDotted,
                    "1/8" => ableton_fx::DelaySubdivision::Eighth,
                    "3/8" => ableton_fx::DelaySubdivision::EighthDotted,
                    "1/4" => ableton_fx::DelaySubdivision::Quarter,
                    _ => ableton_fx::DelaySubdivision::EighthDotted,
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
            let supervisor = PluginScannerSupervisor::new();
            let manifest = supervisor.scan_all();
            if let Some(out_path) = args.get("output_path").and_then(|v| v.as_str()) {
                let _ = manifest.save_to_file(Path::new(out_path));
            }
            ToolCallResult::json(&json!({
                "total_scanned": manifest.plugins.len(),
                "valid_count": manifest.plugins.iter().filter(|p| p.is_valid).count(),
                "quarantined_count": manifest.plugins.iter().filter(|p| p.quarantined).count(),
                "plugins": manifest.plugins
            }))
        }
        "bounce_wav" | "render_bounce_wav" => {
            let dur = args.get("duration_seconds").and_then(|v| v.as_f64()).unwrap_or(4.5).clamp(1.0, 300.0);
            let out_path_str = args
                .get("output_path")
                .and_then(|v| v.as_str())
                .unwrap_or("/tmp/ableton_bounce.wav");
            let bit_depth = args.get("bit_depth").and_then(|v| v.as_u64()).unwrap_or(16) as u16;

            let renderer = DemoArrangementRenderer::new(state.sample_rate, state.bpm.max(60.0));
            let interleaved = renderer.render_demo_seconds(dur);

            let out_path = Path::new(out_path_str);
            if let Some(parent) = out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            let write_res = if bit_depth == 32 {
                WavWriter::write_wav_32bit_float(out_path, state.sample_rate as u32, 2, &interleaved)
            } else {
                WavWriter::write_wav_16bit(out_path, state.sample_rate as u32, 2, &interleaved)
            };

            if let Err(e) = write_res {
                return ToolCallResult::error(format!("Failed to write WAV: {}", e));
            }

            let metrics = AudioAnalysisMetrics::analyze_interleaved(&interleaved, state.sample_rate as u32, 2);
            ToolCallResult::json(&json!({
                "status": "rendered",
                "output_path": out_path_str,
                "duration_seconds": metrics.duration_seconds,
                "peak_dbfs": metrics.peak_dbfs,
                "rms_dbfs": metrics.rms_dbfs,
                "spectral_centroid_hz": metrics.spectral_centroid_hz,
                "spectral_bands": metrics.spectral_bands
            }))
        }

        // --- High-Level Agentic Music Composition Generators ---
        "generate_chord_progression_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let key_str = args.get("key").and_then(|v| v.as_str()).unwrap_or("C");
            let scale_str = args.get("scale").and_then(|v| v.as_str()).unwrap_or("Minor");
            let prog_str = args.get("progression").and_then(|v| v.as_str()).unwrap_or("i-VI-III-VII");
            let voicing = args.get("voicing").and_then(|v| v.as_str()).unwrap_or("close");
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Chords");

            let root_pc = parse_pitch_class(key_str).unwrap_or(0);
            let intervals = get_scale_intervals(scale_str);

            let tokens: Vec<&str> = prog_str.split(&['-', ' ', ','][..]).filter(|s| !s.is_empty()).collect();
            let chord_count = tokens.len().max(1);
            let beats_per_chord = 4.0;
            let total_beats = (chord_count as f64) * beats_per_chord;

            let mut notes = Vec::new();
            for (i, token) in tokens.iter().enumerate() {
                let start_beat = i as f64 * beats_per_chord;
                let step = parse_roman_degree(token).map(|(s, _)| s).unwrap_or(i % intervals.len());
                let pitches = build_chord_notes(root_pc, intervals, step, voicing, 3);
                for p in pitches {
                    notes.push(MidiNote::new(p, 100, start_beat, beats_per_chord * 0.95));
                }
            }

            let pattern = ClipPattern::Midi(MidiPattern {
                length_beats: total_beats,
                notes: notes.clone(),
            });
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, pattern);

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

            ToolCallResult::json(&json!({
                "status": "generated",
                "track_id": track_id,
                "slot_id": slot_id,
                "clip_name": clip_name,
                "total_beats": total_beats,
                "chord_count": chord_count,
                "note_events": notes.len()
            }))
        }
        "generate_bassline_clip" => {
            let track_id = match args.get("track_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let key_str = args.get("key").and_then(|v| v.as_str()).unwrap_or("F");
            let scale_str = args.get("scale").and_then(|v| v.as_str()).unwrap_or("Minor");
            let prog_str = args.get("progression").and_then(|v| v.as_str()).unwrap_or("i-i-VI-VII");
            let rhythm = args.get("rhythm_pattern").and_then(|v| v.as_str()).unwrap_or("driving_8ths");
            let octave = args.get("octave").and_then(|v| v.as_i64()).unwrap_or(2).clamp(0, 8) as u8;
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Bassline");

            let root_pc = parse_pitch_class(key_str).unwrap_or(5);
            let intervals = get_scale_intervals(scale_str);

            let tokens: Vec<&str> = prog_str.split(&['-', ' ', ','][..]).filter(|s| !s.is_empty()).collect();
            let beats_per_bar = 4.0;
            let total_beats = (tokens.len().max(1) as f64) * beats_per_bar;

            let mut notes = Vec::new();
            for (bar_idx, token) in tokens.iter().enumerate() {
                let bar_start = bar_idx as f64 * beats_per_bar;
                let step = parse_roman_degree(token).map(|(s, _)| s).unwrap_or(0);
                let root_note = (((octave as i32 + 1) * 12) + root_pc as i32 + intervals[step % intervals.len()] as i32).clamp(0, 127) as u8;

                match rhythm {
                    "offbeat_house" => {
                        // Offbeats on 0.5, 1.5, 2.5, 3.5
                        for b in [0.5, 1.5, 2.5, 3.5] {
                            notes.push(MidiNote::new(root_note, 110, bar_start + b, 0.45));
                        }
                    }
                    "syncopated" => {
                        // 16th syncopation: 0.0, 0.75, 1.5, 2.25, 3.0
                        for b in [0.0, 0.75, 1.5, 2.25, 3.0] {
                            notes.push(MidiNote::new(root_note, 105, bar_start + b, 0.6));
                        }
                    }
                    "acid_16ths" => {
                        // 16th note acid pulses
                        for s in 0..16 {
                            let vel = if s % 4 == 0 { 115 } else { 85 };
                            let note = if s % 8 == 7 { root_note + 12 } else { root_note };
                            notes.push(MidiNote::new(note, vel, bar_start + (s as f64 * 0.25), 0.22));
                        }
                    }
                    _ => {
                        // Default driving_8ths: 8 notes per bar
                        for eighth in 0..8 {
                            let note_time = bar_start + (eighth as f64 * 0.5);
                            let vel = if eighth % 2 == 0 { 110 } else { 90 };
                            notes.push(MidiNote::new(root_note, vel, note_time, 0.45));
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

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

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
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let key_str = args.get("key").and_then(|v| v.as_str()).unwrap_or("A");
            let scale_str = args.get("scale").and_then(|v| v.as_str()).unwrap_or("Minor");
            let prog_str = args.get("progression").and_then(|v| v.as_str()).unwrap_or("i-VI");
            let rate_str = args.get("rate").and_then(|v| v.as_str()).unwrap_or("1/16");
            let direction = args.get("direction").and_then(|v| v.as_str()).unwrap_or("up");
            let octaves = args.get("octaves").and_then(|v| v.as_u64()).unwrap_or(2).clamp(1, 3) as usize;
            let gate = args.get("gate").and_then(|v| v.as_f64()).unwrap_or(0.85).clamp(0.1, 1.0);
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Arp");

            let step_dur = match rate_str {
                "1/8" => 0.5,
                "1/32" => 0.125,
                _ => 0.25, // 1/16
            };

            let root_pc = parse_pitch_class(key_str).unwrap_or(9);
            let intervals = get_scale_intervals(scale_str);

            let tokens: Vec<&str> = prog_str.split(&['-', ' ', ','][..]).filter(|s| !s.is_empty()).collect();
            let beats_per_chord = 4.0;
            let total_beats = (tokens.len().max(1) as f64) * beats_per_chord;

            let mut notes = Vec::new();
            for (bar_idx, token) in tokens.iter().enumerate() {
                let bar_start = bar_idx as f64 * beats_per_chord;
                let step = parse_roman_degree(token).map(|(s, _)| s).unwrap_or(0);
                let base_pitches = build_chord_notes(root_pc, intervals, step, "close", 3);

                let mut pool = Vec::new();
                for oct in 0..octaves {
                    for &p in &base_pitches {
                        pool.push(p + (oct as u8 * 12));
                    }
                }
                if direction == "down" {
                    pool.reverse();
                }

                let steps_in_bar = (beats_per_chord / step_dur).round() as usize;
                for s in 0..steps_in_bar {
                    let note_beat = bar_start + (s as f64 * step_dur);
                    let pitch = pool[s % pool.len()];
                    notes.push(MidiNote::new(pitch, 100, note_beat, step_dur * gate));
                }
            }

            let pattern = ClipPattern::Midi(MidiPattern {
                length_beats: total_beats,
                notes: notes.clone(),
            });
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, pattern);

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

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
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let hits = match args.get("hits").and_then(|v| v.as_u64()) {
                Some(k) => k as usize,
                None => return ToolCallResult::error("Missing 'hits'"),
            };
            let steps = args.get("steps").and_then(|v| v.as_u64()).unwrap_or(16) as usize;
            let pad_idx = args.get("pad_index").and_then(|v| v.as_u64()).unwrap_or(6) as u8;
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

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

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
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'track_id'"),
            };
            let slot_id = match args.get("slot_id").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => return ToolCallResult::error("Missing 'slot_id'"),
            };
            let style = args.get("style").and_then(|v| v.as_str()).unwrap_or("house_909");
            let tempo = args.get("tempo").and_then(|v| v.as_f64()).unwrap_or(state.bpm);
            let swing = args.get("swing").and_then(|v| v.as_f64()).unwrap_or(state.swing as f64) as f32;
            let complexity = args.get("complexity").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
            let clip_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("909 Groove");

            let drum_pat = generate_drum_groove(style, tempo, swing, complexity);
            let clip_id = ((track_id * 100) + slot_id + 1) as u32;
            let clip = SessionClip::new(clip_id, clip_name, ClipPattern::Drum(drum_pat));

            while state.matrix.tracks.len() <= track_id {
                let tid = state.matrix.tracks.len() as u32;
                state.matrix.tracks.push(ableton_timeline::SessionTrack::new(tid, format!("Track {}", tid), 8));
            }
            let track = &mut state.matrix.tracks[track_id];
            while track.slots.len() <= slot_id {
                track.slots.push(ableton_timeline::SessionSlot::default());
            }
            track.slots[slot_id].set_clip(Some(clip));

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
            let bpm = args.get("bpm").and_then(|v| v.as_f64()).unwrap_or(126.0);
            let total_bars = args.get("total_bars").and_then(|v| v.as_u64()).unwrap_or(32) as usize;

            state.bpm = bpm;
            state.project.transport.bpm = bpm;
            state.delay.set_tempo(bpm);

            // Construct 3 arrangement tracks: Synth Lead, 909 Drums, Sub Bass
            state.timeline.tracks.clear();
            let mut trk_synth = ableton_timeline::ArrangementTrack::new(0, "Lead Synth", "MidiSynth");
            let mut trk_drums = ableton_timeline::ArrangementTrack::new(1, "909 Drums", "DrumRack");
            let mut trk_bass = ableton_timeline::ArrangementTrack::new(2, "Sub Bass", "MidiSynth");

            // Filter sweep automation lane on Synth (15 = filter_frequency)
            let mut filter_auto = AutomationLane::new(15, "filter_frequency");
            filter_auto.add_point(AutomationPoint::linear(0.0, 400.0));
            filter_auto.add_point(AutomationPoint::linear(32.0, 1200.0));
            filter_auto.add_point(AutomationPoint::linear(64.0, 4500.0));
            filter_auto.add_point(AutomationPoint::linear(96.0, 8000.0));
            filter_auto.add_point(AutomationPoint::linear((total_bars * 4) as f64, 15000.0));
            trk_synth.automation_lanes.push(filter_auto);

            // Add arrangement clips across sections (Intro, Build, Drop, Outro)
            let section_bars = total_bars / 4;
            for s in 0..4 {
                let start_beat = (s * section_bars * 4) as f64;
                let dur = (section_bars * 4) as f64;

                let section_name = match s {
                    0 => "Intro",
                    1 => "Build",
                    2 => "Drop",
                    _ => "Outro",
                };

                let synth_clip = ArrangementClip::new(s as u32 + 1, format!("Lead - {}", section_name), start_beat, dur, ClipPattern::Midi(MidiPattern::new(dur)));
                let drum_clip = ArrangementClip::new(s as u32 + 10, format!("Drums - {}", section_name), start_beat, dur, ClipPattern::Drum(DrumStepPattern::new_16_step()));
                let bass_clip = ArrangementClip::new(s as u32 + 20, format!("Bass - {}", section_name), start_beat, dur, ClipPattern::Midi(MidiPattern::new(dur)));

                trk_synth.add_clip(synth_clip);
                trk_drums.add_clip(drum_clip);
                trk_bass.add_clip(bass_clip);
            }

            state.timeline.tracks.push(trk_synth);
            state.timeline.tracks.push(trk_drums);
            state.timeline.tracks.push(trk_bass);

            ToolCallResult::json(&json!({
                "status": "built",
                "genre": genre,
                "key": key,
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
                match AudioAnalysisMetrics::analyze_wav_file(wav_path_str) {
                    Ok(metrics) => ToolCallResult::json(&metrics),
                    Err(e) => ToolCallResult::error(format!("Failed to analyze WAV: {}", e)),
                }
            } else {
                let dur = args.get("duration_seconds").and_then(|v| v.as_f64()).unwrap_or(4.0);
                let renderer = DemoArrangementRenderer::new(state.sample_rate, state.bpm.max(60.0));
                let samples = renderer.render_demo_seconds(dur);
                let metrics = AudioAnalysisMetrics::analyze_interleaved(&samples, state.sample_rate as u32, 2);
                ToolCallResult::json(&metrics)
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
            let out_path_str = args
                .get("output_wav_path")
                .and_then(|v| v.as_str())
                .unwrap_or("/tmp/ableton_synth_preview.wav");

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

            let out_path = Path::new(out_path_str);
            if let Some(parent) = out_path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return ToolCallResult::error(format!("Failed to create output directory: {}", e));
                }
            }

            if let Err(e) = WavWriter::write_wav_16bit(out_path, preview_synth.sample_rate as u32, 2, &interleaved) {
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

        _ => ToolCallResult::error(format!("Tool '{}' not recognized", name)),
    }
}
