//! MCP Resource handling for DAW projects, telemetry, presets, and macro mappings.

use serde_json::json;
use ableton_dsp::presets::{MAPPING_CATALOG_JSON, PRESET_CATALOG_JSON};

use crate::protocol::{Resource, ResourceContent, ResourceReadResult};
use crate::state::DawState;

/// List available MCP resources.
pub fn get_resource_definitions() -> Vec<Resource> {
    vec![
        Resource {
            uri: "project://current".to_string(),
            name: "Current DAW Project Document".to_string(),
            description: Some("Complete JSON serialization of the active project (tracks, clips, transport, presets, timeline).".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        Resource {
            uri: "telemetry://current".to_string(),
            name: "Real-Time Audio Telemetry".to_string(),
            description: Some("Current engine telemetry snapshot: playhead position, master RMS/peak dBFS, track meters.".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        Resource {
            uri: "presets://catalog".to_string(),
            name: "Factory Synth Presets Catalog".to_string(),
            description: Some("Catalog of all 38 factory synth presets imported from Ableton Learning Synths.".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        Resource {
            uri: "mappings://catalog".to_string(),
            name: "Factory XY Macro Mappings Catalog".to_string(),
            description: Some("Catalog of all 17 Performance XY macro piece-wise breakpoint mappings.".to_string()),
            mime_type: Some("application/json".to_string()),
        },
    ]
}

/// Read resource contents by URI.
pub fn read_resource(state: &mut DawState, uri: &str) -> Result<ResourceReadResult, String> {
    match uri {
        "project://current" => {
            state.sync_project_from_state();
            let json_text = state
                .project
                .to_json_string()
                .map_err(|e| format!("Failed to serialize project: {}", e))?;
            Ok(ResourceReadResult {
                contents: vec![ResourceContent {
                    uri: uri.to_string(),
                    mime_type: Some("application/json".to_string()),
                    text: Some(json_text),
                }],
            })
        }
        "telemetry://current" => {
            let tel = state.get_telemetry();
            let tel_json = json!({
                "is_playing": state.is_playing,
                "playhead_beat": tel.playhead_beat,
                "cpu_load": tel.cpu_load,
                "master_peak_l": tel.master_peak_l,
                "master_peak_r": tel.master_peak_r,
                "master_rms_l": tel.master_rms_l,
                "master_rms_r": tel.master_rms_r,
                "track_peaks_l": tel.track_peaks_l,
                "track_peaks_r": tel.track_peaks_r,
            });
            let text = serde_json::to_string_pretty(&tel_json)
                .map_err(|e| format!("Failed to format telemetry: {}", e))?;
            Ok(ResourceReadResult {
                contents: vec![ResourceContent {
                    uri: uri.to_string(),
                    mime_type: Some("application/json".to_string()),
                    text: Some(text),
                }],
            })
        }
        "presets://catalog" => Ok(ResourceReadResult {
            contents: vec![ResourceContent {
                uri: uri.to_string(),
                mime_type: Some("application/json".to_string()),
                text: Some(PRESET_CATALOG_JSON.to_string()),
            }],
        }),
        "mappings://catalog" => Ok(ResourceReadResult {
            contents: vec![ResourceContent {
                uri: uri.to_string(),
                mime_type: Some("application/json".to_string()),
                text: Some(MAPPING_CATALOG_JSON.to_string()),
            }],
        }),
        _ => Err(format!("Resource URI '{}' not found", uri)),
    }
}
