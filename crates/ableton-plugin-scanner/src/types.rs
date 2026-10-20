use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Supported audio plugin bundle formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginFormat {
    AudioUnit,
    Vst3,
}

impl PluginFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginFormat::AudioUnit => "AudioUnit",
            PluginFormat::Vst3 => "VST3",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            PluginFormat::AudioUnit => "component",
            PluginFormat::Vst3 => "vst3",
        }
    }
}

impl std::fmt::Display for PluginFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// High-level categorization of plugin role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCategory {
    Instrument,
    Effect,
}

impl PluginCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginCategory::Instrument => "Instrument",
            PluginCategory::Effect => "Effect",
        }
    }

    pub fn is_instrument(&self) -> bool {
        matches!(self, PluginCategory::Instrument)
    }
}

impl std::fmt::Display for PluginCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Parameter exposed by a plugin for automation and UI control.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginParameter {
    pub index: u32,
    pub name: String,
    pub min_value: f32,
    pub max_value: f32,
    pub default_value: f32,
    pub current_value: f32,
}

impl PluginParameter {
    pub fn new(index: u32, name: impl Into<String>, min_value: f32, max_value: f32, default_value: f32) -> Self {
        let default_clamped = default_value.clamp(min_value, max_value);
        Self {
            index,
            name: name.into(),
            min_value,
            max_value,
            default_value: default_clamped,
            current_value: default_clamped,
        }
    }

    pub fn set_value(&mut self, value: f32) {
        self.current_value = value.clamp(self.min_value, self.max_value);
    }
}

/// Complete metadata descriptor for a discovered or scanned plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub name: String,
    pub identifier: String,
    pub vendor: String,
    pub version: String,
    pub category: PluginCategory,
    pub format: PluginFormat,
    pub path: PathBuf,
    pub is_instrument: bool,
    pub is_valid: bool,
    pub quarantined: bool,
    pub quarantine_reason: Option<String>,
    pub error_message: Option<String>,
    pub num_inputs: u32,
    pub num_outputs: u32,
    pub num_parameters: u32,
    pub parameters: Vec<PluginParameter>,
    pub architectures: Vec<String>,
    pub mtime_secs: u64,
}

impl PluginDescriptor {
    /// Create a valid, verified plugin descriptor.
    #[allow(clippy::too_many_arguments)]
    pub fn new_valid(
        name: impl Into<String>,
        identifier: impl Into<String>,
        vendor: impl Into<String>,
        version: impl Into<String>,
        category: PluginCategory,
        format: PluginFormat,
        path: PathBuf,
        num_inputs: u32,
        num_outputs: u32,
        parameters: Vec<PluginParameter>,
        architectures: Vec<String>,
        mtime_secs: u64,
    ) -> Self {
        let is_inst = category.is_instrument();
        let param_count = parameters.len() as u32;
        Self {
            name: name.into(),
            identifier: identifier.into(),
            vendor: vendor.into(),
            version: version.into(),
            category,
            format,
            path,
            is_instrument: is_inst,
            is_valid: true,
            quarantined: false,
            quarantine_reason: None,
            error_message: None,
            num_inputs,
            num_outputs,
            num_parameters: param_count,
            parameters,
            architectures,
            mtime_secs,
        }
    }

    /// Create a quarantined plugin descriptor for a failed or crashing plugin.
    pub fn new_quarantined(
        name: impl Into<String>,
        identifier: impl Into<String>,
        format: PluginFormat,
        path: PathBuf,
        reason: impl Into<String>,
        mtime_secs: u64,
    ) -> Self {
        let reason_str = reason.into();
        Self {
            name: name.into(),
            identifier: identifier.into(),
            vendor: "Unknown".into(),
            version: "0.0".into(),
            category: PluginCategory::Effect,
            format,
            path,
            is_instrument: false,
            is_valid: false,
            quarantined: true,
            quarantine_reason: Some(reason_str.clone()),
            error_message: Some(reason_str),
            num_inputs: 0,
            num_outputs: 0,
            num_parameters: 0,
            parameters: Vec::new(),
            architectures: Vec::new(),
            mtime_secs,
        }
    }

    pub fn is_instrument(&self) -> bool {
        self.is_instrument
    }

    pub fn find_parameter(&self, name: &str) -> Option<&PluginParameter> {
        self.parameters.iter().find(|p| p.name == name)
    }

    pub fn find_parameter_by_index(&self, index: u32) -> Option<&PluginParameter> {
        self.parameters.iter().find(|p| p.index == index)
    }
}

/// Persistent manifest catalog of all discovered plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugins: Vec<PluginDescriptor>,
    pub scan_timestamp: u64,
}

impl PluginManifest {
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            plugins: Vec::new(),
            scan_timestamp: now,
        }
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)
    }

    pub fn load_from_file(path: &Path) -> Result<Self, std::io::Error> {
        let data = fs::read(path)?;
        serde_json::from_slice(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn add_or_update(&mut self, desc: PluginDescriptor) {
        if let Some(pos) = self.plugins.iter().position(|p| p.path == desc.path) {
            self.plugins[pos] = desc;
        } else {
            self.plugins.push(desc);
        }
    }

    pub fn find_by_path(&self, path: &Path) -> Option<&PluginDescriptor> {
        self.plugins.iter().find(|p| p.path == path)
    }

    pub fn find_by_identifier(&self, id: &str) -> Option<&PluginDescriptor> {
        self.plugins.iter().find(|p| p.identifier == id)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&PluginDescriptor> {
        self.plugins.iter().find(|p| p.name == name)
    }

    pub fn valid_plugins(&self) -> impl Iterator<Item = &PluginDescriptor> {
        self.plugins.iter().filter(|p| p.is_valid && !p.quarantined)
    }

    pub fn quarantined_plugins(&self) -> impl Iterator<Item = &PluginDescriptor> {
        self.plugins.iter().filter(|p| p.quarantined || !p.is_valid)
    }
}

impl Default for PluginManifest {
    fn default() -> Self {
        Self::new()
    }
}
