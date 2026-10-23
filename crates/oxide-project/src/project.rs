//! Complete DAWProject schema, JSON serialization, and disk persistence.

use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use oxide_timeline::Scene;
use crate::error::DAWProjectError;
use crate::mixer::MasterTrackDefinition;
use crate::synth::{
    load_all_factory_macros, load_all_factory_presets, MacroMappingDefinition, PresetDefinition,
};
use crate::track::TrackDefinition;

/// Project document metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub name: String,
    pub version: String,
    pub author: String,
    pub created_at_utc: String,
    pub modified_at_utc: String,
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            version: "1.0.0".to_string(),
            author: "Producer".to_string(),
            created_at_utc: "2026-10-20T00:00:00Z".to_string(),
            modified_at_utc: "2026-10-20T00:00:00Z".to_string(),
        }
    }
}

/// Transport and tempo configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportConfig {
    pub bpm: f64,
    pub time_signature_numerator: u8,
    pub time_signature_denominator: u8,
    pub swing_16th_ratio: f32, // 0.0 = straight, 0.5 = neutral, 0.75 = max shuffle
    pub loop_enabled: bool,
    pub loop_start_beat: f64,
    pub loop_end_beat: f64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
            swing_16th_ratio: 0.0,
            loop_enabled: false,
            loop_start_beat: 0.0,
            loop_end_beat: 16.0,
        }
    }
}

/// The universal root DAW project document serialized to JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DAWProject {
    pub metadata: ProjectMetadata,
    pub transport: TransportConfig,
    pub master_track: MasterTrackDefinition,
    pub tracks: Vec<TrackDefinition>,
    pub scenes: Vec<Scene>,
    pub presets: Vec<PresetDefinition>,
    pub macros: Vec<MacroMappingDefinition>,
}

impl Default for DAWProject {
    fn default() -> Self {
        Self::new("Untitled Project")
    }
}

impl DAWProject {
    /// Create a new project initialized with default metadata, transport, master track,
    /// 38 factory synth presets, and 17 XY macro breakpoint mappings.
    pub fn new(name: impl Into<String>) -> Self {
        let metadata = ProjectMetadata {
            name: name.into(),
            ..Default::default()
        };

        Self {
            metadata,
            transport: TransportConfig::default(),
            master_track: MasterTrackDefinition::default(),
            tracks: Vec::new(),
            scenes: Vec::new(),
            presets: load_all_factory_presets(),
            macros: load_all_factory_macros(),
        }
    }

    /// Serialize project to pretty-printed JSON string.
    pub fn to_json_string(&self) -> Result<String, DAWProjectError> {
        serde_json::to_string_pretty(self).map_err(DAWProjectError::Serialization)
    }

    /// Deserialize project from a JSON string.
    pub fn from_json_str(json_str: &str) -> Result<Self, DAWProjectError> {
        let project: Self = serde_json::from_str(json_str).map_err(|e| {
            DAWProjectError::Validation(format!("Failed to parse project JSON: {}", e))
        })?;
        Ok(project)
    }

    /// Save project to disk as a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), DAWProjectError> {
        let json_str = self.to_json_string()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json_str)?;
        Ok(())
    }

    /// Load project from disk from a JSON file.
    pub fn load_from_file(path: &Path) -> Result<Self, DAWProjectError> {
        let content = fs::read_to_string(path)?;
        Self::from_json_str(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_timeline::{
        ArrangementClip, AutomationLane, AutomationPoint, ClipPattern, MidiNote, MidiPattern,
        SessionClip,
    };

    #[test]
    fn test_daw_project_full_roundtrip() {
        let mut project = DAWProject::new("Berlin Demo");
        project.transport.bpm = 128.0;
        project.transport.swing_16th_ratio = 0.16;

        // Add Synth track with session clip and automation
        let mut synth_track = TrackDefinition::new_synth(1, "Acid Bass");
        let mut bass_pattern = MidiPattern::new(4.0);
        bass_pattern.add_note(MidiNote::new(36, 120, 0.0, 0.5));
        bass_pattern.add_note(MidiNote::new(38, 100, 1.0, 0.5));
        synth_track.session_clips.push(Some(SessionClip::new(
            101,
            "Acid Pattern",
            ClipPattern::Midi(bass_pattern),
        )));

        let mut auto_lane = AutomationLane::new(10, "filter_cutoff");
        auto_lane.add_point(AutomationPoint::new(0.0, 0.2, 0.0));
        auto_lane.add_point(AutomationPoint::new(4.0, 0.8, 0.5));
        synth_track.automation_lanes.push(auto_lane);

        project.tracks.push(synth_track);

        // Add 909 Drum track with arrangement clip
        let mut drum_track = TrackDefinition::new_drum_rack(2, "909 Drums");
        let drum_pattern = oxide_timeline::DrumStepPattern::new_16_step();
        drum_track.arrangement_clips.push(ArrangementClip::new(
            201,
            "909 Groove",
            0.0,
            16.0,
            ClipPattern::Drum(drum_pattern),
        ));
        project.tracks.push(drum_track);

        // Add Scenes
        project.scenes.push(Scene::new(1, "Intro"));
        project.scenes.push(Scene::new(2, "Drop"));

        // Verify presets and macros were populated
        assert_eq!(project.presets.len(), 38);
        assert_eq!(project.macros.len(), 17);

        // Roundtrip to JSON string
        let json_str = project.to_json_string().expect("Serialization must succeed");
        let deserialized = DAWProject::from_json_str(&json_str).expect("Deserialization must succeed");

        assert_eq!(project, deserialized, "Full project roundtrip must match 100%");

        // Test disk save & load
        let temp_path = Path::new("/tmp/ableton_worker_roundtrip_test.json");
        project.save_to_file(temp_path).expect("Save to file must succeed");
        let loaded = DAWProject::load_from_file(temp_path).expect("Load from file must succeed");
        assert_eq!(project, loaded, "Disk persistence roundtrip must match 100%");
        let _ = fs::remove_file(temp_path);
    }

    #[test]
    fn test_malformed_json_graceful_handling() {
        let bad_json = "{ \"metadata\": { \"name\": broken ";
        let res = DAWProject::from_json_str(bad_json);
        assert!(res.is_err(), "Malformed JSON must return Err");

        let missing_fields_json = "{}";
        let res2 = DAWProject::from_json_str(missing_fields_json);
        assert!(res2.is_err(), "Missing required fields must return Err");
    }
}
