//! 38 Factory Synth Presets and 17 XY Macro Breakpoint Mappings.
//!
//! Decompiled & reverse-engineered from Ableton Learning Synths
//! (`src/data/preset-catalog.json` and `src/data/mapping-catalog.json`).

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::parameters::ParameterBank;

pub const PRESET_CATALOG_JSON: &str = include_str!("../data/preset-catalog.json");
pub const MAPPING_CATALOG_JSON: &str = include_str!("../data/mapping-catalog.json");

/// Breakpoint in an XY macro piece-wise curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroBreakpoint {
    pub pct: f32,
    #[serde(rename = "controlValue")]
    pub control_value: f32,
}

/// Target parameter controlled by an XY macro axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroTarget {
    #[serde(rename = "parameterName")]
    pub parameter_name: String,
    #[serde(rename = "minControlValue")]
    pub min_control_value: f32,
    #[serde(rename = "maxControlValue")]
    pub max_control_value: f32,
    #[serde(rename = "minValue", default)]
    pub min_value: Option<f32>,
    #[serde(rename = "maxValue", default)]
    pub max_value: Option<f32>,
    #[serde(default)]
    pub breakpoints: Vec<MacroBreakpoint>,
}

/// XY Macro performance modulation mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroMapping {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(rename = "macroX", default)]
    pub macro_x: Vec<MacroTarget>,
    #[serde(rename = "macroY", default)]
    pub macro_y: Vec<MacroTarget>,
}

/// Individual parameter setting inside a preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetParamEntry {
    pub name: String,
    pub value: f32,
}

/// Factory synth preset definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    #[serde(default)]
    pub instrument_name: Option<String>,
    #[serde(default)]
    pub parameters: Vec<PresetParamEntry>,
}

/// Evaluate piece-wise linear breakpoint interpolation for an XY macro target.
pub fn evaluate_macro_target(norm: f32, target: &MacroTarget) -> f32 {
    let t = norm.clamp(0.0, 1.0);
    if target.breakpoints.is_empty() {
        return target.min_control_value + (target.max_control_value - target.min_control_value) * t;
    }
    let mut low_ctrl = 0.0f32;
    let mut low_pct = 0.0f32;
    let mut high_ctrl = 1.0f32;
    let mut high_pct = 1.0f32;

    for bp in &target.breakpoints {
        if bp.pct <= t {
            low_ctrl = bp.control_value;
            low_pct = bp.pct;
        }
        if bp.pct >= t {
            high_ctrl = bp.control_value;
            high_pct = bp.pct;
            break;
        }
    }

    if (high_pct - low_pct).abs() < 1e-6 {
        high_ctrl
    } else {
        low_ctrl + ((t - low_pct) / (high_pct - low_pct)) * (high_ctrl - low_ctrl)
    }
}

/// Load and parse all 38 factory presets from the embedded catalog.
pub fn load_preset_catalog() -> Result<HashMap<String, Preset>, serde_json::Error> {
    serde_json::from_str(PRESET_CATALOG_JSON)
}

/// Load and parse all 17 XY macro mappings from the embedded catalog.
pub fn load_mapping_catalog() -> Result<HashMap<String, MacroMapping>, serde_json::Error> {
    serde_json::from_str(MAPPING_CATALOG_JSON)
}

/// Apply a preset's parameters to a `ParameterBank`.
pub fn apply_preset(bank: &mut ParameterBank, preset: &Preset) {
    for param in &preset.parameters {
        bank.set_value(&param.name, param.value);
    }
}

/// Apply Macro X normalized coordinate to a `ParameterBank` using the given mapping.
pub fn apply_macro_x(bank: &mut ParameterBank, mapping: &MacroMapping, norm: f32) {
    for target in &mapping.macro_x {
        let ctrl = evaluate_macro_target(norm, target);
        bank.set_control_value(&target.parameter_name, ctrl);
    }
}

/// Apply Macro Y normalized coordinate to a `ParameterBank` using the given mapping.
pub fn apply_macro_y(bank: &mut ParameterBank, mapping: &MacroMapping, norm: f32) {
    for target in &mapping.macro_y {
        let ctrl = evaluate_macro_target(norm, target);
        bank.set_control_value(&target.parameter_name, ctrl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_all_38_presets() {
        let catalog = load_preset_catalog().expect("Must parse preset-catalog.json");
        assert_eq!(catalog.len(), 38, "Must contain exactly 38 presets");

        assert!(catalog.contains_key("wowbass.preset"));
        assert!(catalog.contains_key("laser.preset"));
        assert!(catalog.contains_key("bouncingball.preset"));
        assert!(catalog.contains_key("synthkick.preset"));

        let wowbass = &catalog["wowbass.preset"];
        assert!(wowbass.parameters.len() >= 27);
    }

    #[test]
    fn test_load_all_17_mappings() {
        let mappings = load_mapping_catalog().expect("Must parse mapping-catalog.json");
        assert_eq!(mappings.len(), 17, "Must contain exactly 17 mappings");

        assert!(mappings.contains_key("wowbass.mapping"));
        assert!(mappings.contains_key("laser.mapping"));

        let wowbass = &mappings["wowbass.mapping"];
        assert_eq!(wowbass.macro_x.len(), 1);
        assert_eq!(wowbass.macro_x[0].parameter_name, "filter_frequency");
        assert_eq!(wowbass.macro_y.len(), 1);
        assert_eq!(wowbass.macro_y[0].parameter_name, "filter_resonance");
    }

    #[test]
    fn test_macro_evaluation_and_frequency_sweep() {
        let mappings = load_mapping_catalog().unwrap();
        let wowbass = &mappings["wowbass.mapping"];

        let mut bank = ParameterBank::new();
        // At norm = 0.0
        apply_macro_x(&mut bank, wowbass, 0.0);
        let freq_low = bank.get_value("filter_frequency");
        // wowbass X minControlValue = 0.293425 -> 20 * 2^(10 * 0.293425) = 152.87 Hz
        assert!((freq_low - 152.87).abs() < 1.0, "Expected ~152.87 Hz, got {}", freq_low);

        // At norm = 1.0
        apply_macro_x(&mut bank, wowbass, 1.0);
        let freq_high = bank.get_value("filter_frequency");
        // wowbass X maxControlValue = 0.696022 -> 20 * 2^(10 * 0.696022) = 2490.39 Hz
        assert!((freq_high - 2490.39).abs() < 2.0, "Expected ~2490.39 Hz, got {}", freq_high);
    }
}
