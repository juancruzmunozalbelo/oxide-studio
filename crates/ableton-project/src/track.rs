//! Track definitions, instrument routing, and clip collections.

use serde::{Deserialize, Serialize};
use ableton_timeline::{ArrangementClip, AutomationLane, SessionClip};
use crate::drums::DrumRackDefinition;
use crate::mixer::{InsertEffectConfig, MixerState};
use crate::synth::SynthPatchParameters;

/// Classification of tracks in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackType {
    MidiSynth,
    DrumRack,
    Audio,
}

/// Instrument engine loaded on a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InstrumentConfig {
    BuiltinSubtractiveSynth(SynthPatchParameters),
    ElectronicDrumRack(DrumRackDefinition),
    ExternalPlugin {
        plugin_id: String,
        plugin_name: String,
        format: String, // "AU" or "VST3"
        state_base64: String,
    },
    None,
}

/// Complete definition of an audio or MIDI track in the DAW.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackDefinition {
    pub id: u32,
    pub name: String,
    pub color_hex: String,
    pub track_type: TrackType,
    pub mixer: MixerState,
    pub instrument: InstrumentConfig,
    pub insert_fx: Vec<InsertEffectConfig>,
    pub session_clips: Vec<Option<SessionClip>>,
    pub arrangement_clips: Vec<ArrangementClip>,
    pub automation_lanes: Vec<AutomationLane>,
}

impl TrackDefinition {
    pub fn new_synth(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color_hex: "#ffbe00".to_string(),
            track_type: TrackType::MidiSynth,
            mixer: MixerState::default(),
            instrument: InstrumentConfig::BuiltinSubtractiveSynth(SynthPatchParameters::default()),
            insert_fx: Vec::new(),
            session_clips: Vec::new(),
            arrangement_clips: Vec::new(),
            automation_lanes: Vec::new(),
        }
    }

    pub fn new_drum_rack(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color_hex: "#00d2ff".to_string(),
            track_type: TrackType::DrumRack,
            mixer: MixerState::default(),
            instrument: InstrumentConfig::ElectronicDrumRack(DrumRackDefinition::default_909()),
            insert_fx: Vec::new(),
            session_clips: Vec::new(),
            arrangement_clips: Vec::new(),
            automation_lanes: Vec::new(),
        }
    }

    pub fn new_audio(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color_hex: "#00e676".to_string(),
            track_type: TrackType::Audio,
            mixer: MixerState::default(),
            instrument: InstrumentConfig::None,
            insert_fx: Vec::new(),
            session_clips: Vec::new(),
            arrangement_clips: Vec::new(),
            automation_lanes: Vec::new(),
        }
    }
}
