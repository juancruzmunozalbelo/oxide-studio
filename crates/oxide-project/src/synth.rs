//! Synthesizer patch states (30 parameters), factory presets, and XY macro breakpoint mappings.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// The complete state of all 30 synthesizer parameters for the native PolyBLEP / Mystran ZDF synth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthPatchParameters {
    pub preset_name: Option<String>,
    
    // Oscillators & Noise (6 parameters)
    pub oscillator_noise_amplitude: f32,
    pub oscillator_pulse_amplitude: f32,
    pub oscillator_pulse_width: f32,
    pub oscillator_saw_amplitude: f32,
    pub oscillator_saw_detune_coarse: f32,
    pub oscillator_saw_detune_fine: f32,

    // Amplitude ADSR Envelope (4 parameters)
    pub amplitude_envelope_attack: f32,
    pub amplitude_envelope_decay: f32,
    pub amplitude_envelope_sustain: f32,
    pub amplitude_envelope_release: f32,

    // Modulation ADSR Envelope (4 parameters)
    pub modulation_envelope_attack: f32,
    pub modulation_envelope_decay: f32,
    pub modulation_envelope_sustain: f32,
    pub modulation_envelope_release: f32,

    // Mystran 2x ZDF 4-Pole Ladder Filter (3 parameters)
    pub filter_envelope_amount: f32,
    pub filter_frequency: f32,
    pub filter_resonance: f32,

    // 4-Shape LFO (3 parameters)
    pub lfo_frequency: f32,
    pub lfo_frequency_envelope_amount: f32,
    pub lfo_shape: u8, // 0: Sine, 1: SawDown, 2: Square, 3: SampleAndHold

    // Modulations (7 parameters)
    pub amplitude_lfo_amount: f32,
    pub oscillator_pulse_width_lfo_amount: f32,
    pub oscillator_pulse_width_envelope_amount: f32,
    pub filter_lfo_amount: f32,
    pub oscillator_pitch_offset_lfo_amount: f32,
    pub oscillator_pitch_offset_envelope_amount: f32,
    pub oscillator_pitch_offset: f32,

    // Global & Voicing (3 parameters)
    pub global_slew_time: f32,
    pub glide_time: f32,
    pub amplitude: f32,

    /// Parameter names explicitly overridden by the user (preserved even if reset to Init defaults).
    #[serde(default)]
    pub overridden_params: Vec<String>,
}

impl Default for SynthPatchParameters {
    fn default() -> Self {
        Self {
            preset_name: Some("Init Patch".to_string()),
            oscillator_noise_amplitude: 0.0,
            oscillator_pulse_amplitude: 1.0,
            oscillator_pulse_width: 0.0,
            oscillator_saw_amplitude: 0.0,
            oscillator_saw_detune_coarse: 0.0,
            oscillator_saw_detune_fine: 0.0,
            amplitude_envelope_attack: 30.0,
            amplitude_envelope_decay: 200.0,
            amplitude_envelope_sustain: 0.5,
            amplitude_envelope_release: 300.0,
            modulation_envelope_attack: 100.0,
            modulation_envelope_decay: 500.0,
            modulation_envelope_sustain: 0.5,
            modulation_envelope_release: 150.0,
            filter_envelope_amount: 0.0,
            filter_frequency: 3000.0,
            filter_resonance: 0.2,
            lfo_frequency: 1.0,
            lfo_frequency_envelope_amount: 0.0,
            lfo_shape: 0,
            amplitude_lfo_amount: 0.0,
            oscillator_pulse_width_lfo_amount: 0.0,
            oscillator_pulse_width_envelope_amount: 0.0,
            filter_lfo_amount: 0.0,
            oscillator_pitch_offset_lfo_amount: 0.0,
            oscillator_pitch_offset_envelope_amount: 0.0,
            oscillator_pitch_offset: 0.0,
            global_slew_time: 50.0,
            glide_time: 0.0,
            amplitude: 1.0,
            overridden_params: Vec::new(),
        }
    }
}

impl SynthPatchParameters {
    /// Get a parameter value by name.
    pub fn get_parameter(&self, name: &str) -> Option<f32> {
        match name {
            "oscillator_noise_amplitude" => Some(self.oscillator_noise_amplitude),
            "oscillator_pulse_amplitude" => Some(self.oscillator_pulse_amplitude),
            "oscillator_pulse_width" => Some(self.oscillator_pulse_width),
            "oscillator_saw_amplitude" => Some(self.oscillator_saw_amplitude),
            "oscillator_saw_detune_coarse" => Some(self.oscillator_saw_detune_coarse),
            "oscillator_saw_detune_fine" => Some(self.oscillator_saw_detune_fine),
            "amplitude_envelope_attack" => Some(self.amplitude_envelope_attack),
            "amplitude_envelope_decay" => Some(self.amplitude_envelope_decay),
            "amplitude_envelope_sustain" => Some(self.amplitude_envelope_sustain),
            "amplitude_envelope_release" => Some(self.amplitude_envelope_release),
            "modulation_envelope_attack" => Some(self.modulation_envelope_attack),
            "modulation_envelope_decay" => Some(self.modulation_envelope_decay),
            "modulation_envelope_sustain" => Some(self.modulation_envelope_sustain),
            "modulation_envelope_release" => Some(self.modulation_envelope_release),
            "filter_envelope_amount" => Some(self.filter_envelope_amount),
            "filter_frequency" => Some(self.filter_frequency),
            "filter_resonance" => Some(self.filter_resonance),
            "lfo_frequency" => Some(self.lfo_frequency),
            "lfo_frequency_envelope_amount" => Some(self.lfo_frequency_envelope_amount),
            "lfo_shape" => Some(self.lfo_shape as f32),
            "amplitude_lfo_amount" => Some(self.amplitude_lfo_amount),
            "oscillator_pulse_width_lfo_amount" => Some(self.oscillator_pulse_width_lfo_amount),
            "oscillator_pulse_width_envelope_amount" => Some(self.oscillator_pulse_width_envelope_amount),
            "filter_lfo_amount" => Some(self.filter_lfo_amount),
            "oscillator_pitch_offset_lfo_amount" => Some(self.oscillator_pitch_offset_lfo_amount),
            "oscillator_pitch_offset_envelope_amount" => Some(self.oscillator_pitch_offset_envelope_amount),
            "oscillator_pitch_offset" => Some(self.oscillator_pitch_offset),
            "global_slew_time" => Some(self.global_slew_time),
            "glide_time" => Some(self.glide_time),
            "amplitude" => Some(self.amplitude),
            _ => None,
        }
    }

    /// Set a parameter value by name. Returns `false` if unknown or non-finite.
    pub fn set_parameter(&mut self, name: &str, val: f32) -> bool {
        if !val.is_finite() {
            return false;
        }
        match name {
            "oscillator_noise_amplitude" => { self.oscillator_noise_amplitude = val; true }
            "oscillator_pulse_amplitude" => { self.oscillator_pulse_amplitude = val; true }
            "oscillator_pulse_width" => { self.oscillator_pulse_width = val; true }
            "oscillator_saw_amplitude" => { self.oscillator_saw_amplitude = val; true }
            "oscillator_saw_detune_coarse" => { self.oscillator_saw_detune_coarse = val; true }
            "oscillator_saw_detune_fine" => { self.oscillator_saw_detune_fine = val; true }
            "amplitude_envelope_attack" => { self.amplitude_envelope_attack = val; true }
            "amplitude_envelope_decay" => { self.amplitude_envelope_decay = val; true }
            "amplitude_envelope_sustain" => { self.amplitude_envelope_sustain = val; true }
            "amplitude_envelope_release" => { self.amplitude_envelope_release = val; true }
            "modulation_envelope_attack" => { self.modulation_envelope_attack = val; true }
            "modulation_envelope_decay" => { self.modulation_envelope_decay = val; true }
            "modulation_envelope_sustain" => { self.modulation_envelope_sustain = val; true }
            "modulation_envelope_release" => { self.modulation_envelope_release = val; true }
            "filter_envelope_amount" => { self.filter_envelope_amount = val; true }
            "filter_frequency" => { self.filter_frequency = val; true }
            "filter_resonance" => { self.filter_resonance = val; true }
            "lfo_frequency" => { self.lfo_frequency = val; true }
            "lfo_frequency_envelope_amount" => { self.lfo_frequency_envelope_amount = val; true }
            "lfo_shape" => { self.lfo_shape = val.round() as u8; true }
            "amplitude_lfo_amount" => { self.amplitude_lfo_amount = val; true }
            "oscillator_pulse_width_lfo_amount" => { self.oscillator_pulse_width_lfo_amount = val; true }
            "oscillator_pulse_width_envelope_amount" => { self.oscillator_pulse_width_envelope_amount = val; true }
            "filter_lfo_amount" => { self.filter_lfo_amount = val; true }
            "oscillator_pitch_offset_lfo_amount" => { self.oscillator_pitch_offset_lfo_amount = val; true }
            "oscillator_pitch_offset_envelope_amount" => { self.oscillator_pitch_offset_envelope_amount = val; true }
            "oscillator_pitch_offset" => { self.oscillator_pitch_offset = val; true }
            "global_slew_time" => { self.global_slew_time = val; true }
            "glide_time" => { self.glide_time = val; true }
            "amplitude" => { self.amplitude = val; true }
            _ => false,
        }
    }

    /// Record `name` as explicitly overridden by the user if not already tracked.
    pub fn mark_overridden(&mut self, name: &str) {
        if self.get_parameter(name).is_some() && !self.overridden_params.iter().any(|p| p == name) {
            self.overridden_params.push(name.to_string());
        }
    }

    /// Check whether `name` was explicitly overridden by the user.
    pub fn is_overridden(&self, name: &str) -> bool {
        self.overridden_params.iter().any(|p| p == name)
    }

    /// Set a parameter value and mark it as explicitly overridden.
    pub fn set_overridden_parameter(&mut self, name: &str, val: f32) -> bool {
        if self.set_parameter(name, val) {
            self.mark_overridden(name);
            true
        } else {
            false
        }
    }

    /// Convert to a parameter map containing all 30 parameters.
    pub fn to_parameter_map(&self) -> HashMap<String, f32> {
        let mut map = HashMap::with_capacity(30);
        let names = [
            "oscillator_noise_amplitude", "oscillator_pulse_amplitude", "oscillator_pulse_width",
            "oscillator_saw_amplitude", "oscillator_saw_detune_coarse", "oscillator_saw_detune_fine",
            "amplitude_envelope_attack", "amplitude_envelope_decay", "amplitude_envelope_sustain",
            "amplitude_envelope_release", "modulation_envelope_attack", "modulation_envelope_decay",
            "modulation_envelope_sustain", "modulation_envelope_release", "filter_envelope_amount",
            "filter_frequency", "filter_resonance", "lfo_frequency", "lfo_frequency_envelope_amount",
            "lfo_shape", "amplitude_lfo_amount", "oscillator_pulse_width_lfo_amount",
            "oscillator_pulse_width_envelope_amount", "filter_lfo_amount",
            "oscillator_pitch_offset_lfo_amount", "oscillator_pitch_offset_envelope_amount",
            "oscillator_pitch_offset", "global_slew_time", "glide_time", "amplitude",
        ];
        for name in names {
            if let Some(val) = self.get_parameter(name) {
                map.insert(name.to_string(), val);
            }
        }
        map
    }

    /// Construct from a parameter map.
    pub fn from_parameter_map(map: &HashMap<String, f32>, preset_name: Option<String>) -> Self {
        let mut patch = Self {
            preset_name,
            ..Default::default()
        };
        for (name, &val) in map {
            patch.set_parameter(name, val);
        }
        patch
    }
}

/// A factory synth preset definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetDefinition {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub mapping_key: Option<String>,
    pub patch: SynthPatchParameters,
}

/// A breakpoint in an XY macro piece-wise curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroBreakpointDefinition {
    pub pct: f32,
    pub control_value: f32,
}

/// A target parameter modulated by an XY macro axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroTargetDefinition {
    pub parameter_name: String,
    pub min_control_value: f32,
    pub max_control_value: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<MacroBreakpointDefinition>,
}

/// An XY macro performance mapping configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroMappingDefinition {
    pub key: String,
    pub version: Option<String>,
    pub macro_x: Vec<MacroTargetDefinition>,
    pub macro_y: Vec<MacroTargetDefinition>,
}

/// Normalize a preset identifier by stripping an optional `.preset` suffix,
/// lowercasing, and normalizing `_` / `-` separators to single spaces.
pub fn normalize_preset_name(name: &str) -> String {
    let trimmed = name.trim();
    let stem = if trimmed.len() > 7 && trimmed[trimmed.len() - 7..].eq_ignore_ascii_case(".preset") {
        &trimmed[..trimmed.len() - 7]
    } else {
        trimmed
    };
    stem.replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

impl PresetDefinition {
    /// Returns `true` if `query` matches this preset's key (`.preset`), stem, or normalized name.
    pub fn matches_name(&self, query: &str) -> bool {
        let q_trim = query.trim();
        if q_trim.is_empty() {
            return false;
        }
        if self.key.eq_ignore_ascii_case(q_trim) || self.name.eq_ignore_ascii_case(q_trim) {
            return true;
        }
        let q_norm = normalize_preset_name(q_trim);
        !q_norm.is_empty()
            && (normalize_preset_name(&self.key) == q_norm
                || normalize_preset_name(&self.name) == q_norm)
    }
}

/// Find a preset by catalog key (with or without `.preset`), stem, or normalized name.
pub fn find_factory_preset<'a>(
    presets: &'a [PresetDefinition],
    query: &str,
) -> Option<&'a PresetDefinition> {
    let q_trim = query.trim();
    if q_trim.is_empty() {
        return None;
    }
    presets
        .iter()
        .find(|p| p.key.eq_ignore_ascii_case(q_trim) || p.name.eq_ignore_ascii_case(q_trim))
        .or_else(|| presets.iter().find(|p| p.matches_name(q_trim)))
}

fn build_presets_from_catalog(
    raw_presets: HashMap<String, oxide_dsp::presets::Preset>,
    raw_mappings: &HashMap<String, oxide_dsp::presets::MacroMapping>,
) -> Vec<PresetDefinition> {
    let mut list = Vec::with_capacity(raw_presets.len());
    for (key, preset) in raw_presets {
        let stem = key.strip_suffix(".preset").unwrap_or(&key).to_string();
        let mut param_map = HashMap::new();
        for entry in &preset.parameters {
            param_map.insert(entry.name.clone(), entry.value);
        }

        let mapping_key = {
            let candidate = format!("{stem}.mapping");
            if raw_mappings.contains_key(&candidate) {
                Some(candidate)
            } else {
                None
            }
        };
        let patch = SynthPatchParameters::from_parameter_map(&param_map, Some(stem.clone()));
        list.push(PresetDefinition {
            key,
            name: stem,
            description: None,
            mapping_key,
            patch,
        });
    }

    list.sort_by(|a, b| a.key.cmp(&b.key));
    list
}

/// Load all 38 factory synth presets from the catalog.
pub fn load_all_factory_presets() -> Vec<PresetDefinition> {
    let raw_presets = match oxide_dsp::presets::load_preset_catalog() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let raw_mappings = oxide_dsp::presets::load_mapping_catalog().unwrap_or_default();
    build_presets_from_catalog(raw_presets, &raw_mappings)
}

/// Load all 17 factory XY macro breakpoint mappings from the catalog.
pub fn load_all_factory_macros() -> Vec<MacroMappingDefinition> {
    let raw_mappings = match oxide_dsp::presets::load_mapping_catalog() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    let mut list = Vec::with_capacity(raw_mappings.len());
    for (key, mapping) in raw_mappings {
        let convert_targets = |targets: &[oxide_dsp::presets::MacroTarget]| -> Vec<MacroTargetDefinition> {
            targets
                .iter()
                .map(|t| MacroTargetDefinition {
                    parameter_name: t.parameter_name.clone(),
                    min_control_value: t.min_control_value,
                    max_control_value: t.max_control_value,
                    min_value: t.min_value,
                    max_value: t.max_value,
                    breakpoints: t
                        .breakpoints
                        .iter()
                        .map(|b| MacroBreakpointDefinition {
                            pct: b.pct,
                            control_value: b.control_value,
                        })
                        .collect(),
                })
                .collect()
        };

        list.push(MacroMappingDefinition {
            key,
            version: mapping.version,
            macro_x: convert_targets(&mapping.macro_x),
            macro_y: convert_targets(&mapping.macro_y),
        });
    }

    list.sort_by(|a, b| a.key.cmp(&b.key));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synth_patch_parameters_30_params() {
        let patch = SynthPatchParameters::default();
        let map = patch.to_parameter_map();
        assert_eq!(map.len(), 30, "Must contain exactly 30 standard synth parameters");

        let mut copy = patch.clone();
        assert!(copy.set_parameter("filter_frequency", 880.0));
        assert_eq!(copy.get_parameter("filter_frequency"), Some(880.0));
        assert!(!copy.set_parameter("filter_frequency", f32::NAN));
        assert_eq!(copy.get_parameter("filter_frequency"), Some(880.0));
    }

    #[test]
    fn test_synth_overridden_params_roundtrip_and_backwards_compat() {
        let mut patch = SynthPatchParameters {
            preset_name: Some("wowbass".to_string()),
            ..Default::default()
        };
        assert!(patch.overridden_params.is_empty());
        assert!(patch.set_overridden_parameter("filter_frequency", 3000.0));
        assert!(patch.is_overridden("filter_frequency"));

        let json = serde_json::to_string(&patch).unwrap();
        let decoded: SynthPatchParameters = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, patch);
        assert_eq!(decoded.overridden_params, vec!["filter_frequency".to_string()]);

        // Legacy JSON without overridden_params deserializes cleanly
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.as_object_mut().unwrap().remove("overridden_params");
        let legacy: SynthPatchParameters = serde_json::from_value(v).unwrap();
        assert!(legacy.overridden_params.is_empty());
    }

    #[test]
    fn test_load_factory_catalogs() {
        let presets = load_all_factory_presets();
        assert_eq!(presets.len(), 38, "Must load all 38 factory presets");

        assert!(find_factory_preset(&presets, "wowbass").is_some());
        assert!(find_factory_preset(&presets, "wowbass.preset").is_some());
        assert!(find_factory_preset(&presets, "two-sounds-in-one").is_some());
        assert!(find_factory_preset(&presets, "two sounds in one").is_some());
        assert!(find_factory_preset(&presets, "two_sounds_in_one.preset").is_some());

        let wow = find_factory_preset(&presets, "wowbass").unwrap();
        assert_eq!(wow.mapping_key.as_deref(), Some("wowbass.mapping"));

        // Synthetic catalog entry with underscores and .preset suffix
        let mut custom = HashMap::new();
        custom.insert(
            "deep_acid_303.preset".to_string(),
            oxide_dsp::presets::Preset {
                instrument_name: Some("learning-synth".to_string()),
                parameters: vec![oxide_dsp::presets::PresetParamEntry {
                    name: "filter_frequency".to_string(),
                    value: 640.0,
                }],
            },
        );
        let built = build_presets_from_catalog(custom, &HashMap::new());
        assert_eq!(built[0].key, "deep_acid_303.preset");
        assert_eq!(built[0].name, "deep_acid_303");
        assert_eq!(built[0].patch.preset_name.as_deref(), Some("deep_acid_303"));
        assert!(find_factory_preset(&built, "deep_acid_303").is_some());
        assert!(find_factory_preset(&built, "deep_acid_303.preset").is_some());
        assert!(find_factory_preset(&built, "deep acid 303").is_some());

        let macros = load_all_factory_macros();
        assert_eq!(macros.len(), 17, "Must load all 17 XY macro mappings");
    }
}
