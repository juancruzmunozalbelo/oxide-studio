//! Browser catalogue and neural sound similarity vector search for MCP.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserCategory {
    Sounds,
    Drums,
    Instruments,
    AudioEffects,
    MidiEffects,
    PlugIns,
    Clips,
    Samples,
    Grooves,
    Templates,
}

impl BrowserCategory {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sounds" | "sound" => Some(BrowserCategory::Sounds),
            "drums" | "drum" => Some(BrowserCategory::Drums),
            "instruments" | "instrument" | "synth" => Some(BrowserCategory::Instruments),
            "audioeffects" | "audio_effects" | "audio_effect" | "fx" => Some(BrowserCategory::AudioEffects),
            "midieffects" | "midi_effects" | "midi_effect" => Some(BrowserCategory::MidiEffects),
            "plugins" | "plug-ins" | "vst" | "au" => Some(BrowserCategory::PlugIns),
            "clips" | "clip" => Some(BrowserCategory::Clips),
            "samples" | "sample" => Some(BrowserCategory::Samples),
            "grooves" | "groove" => Some(BrowserCategory::Grooves),
            "templates" | "template" => Some(BrowserCategory::Templates),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            BrowserCategory::Sounds => "Sounds",
            BrowserCategory::Drums => "Drums",
            BrowserCategory::Instruments => "Instruments",
            BrowserCategory::AudioEffects => "Audio Effects",
            BrowserCategory::MidiEffects => "MIDI Effects",
            BrowserCategory::PlugIns => "Plug-Ins",
            BrowserCategory::Clips => "Clips",
            BrowserCategory::Samples => "Samples",
            BrowserCategory::Grooves => "Grooves",
            BrowserCategory::Templates => "Templates",
        }
    }
}

/// Item in the browser library with 8-dimensional spectral features.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserItem {
    pub id: String,
    pub name: String,
    pub category: BrowserCategory,
    pub tags: Vec<String>,
    pub detail: String,
    /// 8-dimensional normalized spectral feature vector:
    /// [centroid, flux, rolloff, low_band, mid_band, high_band, zcr, attack_sharpness]
    pub spectral_features: [f32; 8],
}

impl BrowserItem {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        category: BrowserCategory,
        tags: &[&str],
        detail: impl Into<String>,
        features: [f32; 8],
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category,
            tags: tags.iter().map(|&s| s.to_string()).collect(),
            detail: detail.into(),
            spectral_features: features,
        }
    }

    /// Compute cosine similarity against another browser item's spectral feature vector.
    pub fn cosine_similarity(&self, other: &BrowserItem) -> f32 {
        let u = &self.spectral_features;
        let v = &other.spectral_features;
        let mut dot = 0.0f32;
        let mut norm_u = 0.0f32;
        let mut norm_v = 0.0f32;

        for i in 0..8 {
            dot += u[i] * v[i];
            norm_u += u[i] * u[i];
            norm_v += v[i] * v[i];
        }

        let denom = norm_u.sqrt() * norm_v.sqrt();
        if denom > 1e-8 {
            (dot / denom).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// In-memory library and similarity ranking engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserLibrary {
    pub items: Vec<BrowserItem>,
}

impl Default for BrowserLibrary {
    fn default() -> Self {
        Self::new_factory()
    }
}

impl BrowserLibrary {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn new_factory() -> Self {
        let mut items = Vec::new();

        // Instruments
        items.push(BrowserItem::new(
            "inst_wavetable", "Wavetable", BrowserCategory::Instruments,
            &["Lead", "Pad", "Electronic", "Bright", "Warm"], "Dual-morphing wavetable synth with 2x ZDF filters",
            [0.72, 0.65, 0.80, 0.45, 0.60, 0.85, 0.50, 0.70],
        ));
        items.push(BrowserItem::new(
            "inst_operator", "Operator", BrowserCategory::Instruments,
            &["Bass", "Lead", "FM", "Electronic", "Bright", "Dirty"], "4-operator FM synthesis workstation",
            [0.68, 0.75, 0.78, 0.60, 0.55, 0.82, 0.62, 0.85],
        ));
        items.push(BrowserItem::new(
            "inst_drift", "Drift", BrowserCategory::Instruments,
            &["Bass", "Lead", "Pad", "Analog", "Warm"], "Subtractive analog synth with thermal oscillator drift",
            [0.45, 0.35, 0.50, 0.75, 0.65, 0.40, 0.30, 0.45],
        ));
        items.push(BrowserItem::new(
            "inst_meld", "Meld", BrowserCategory::Instruments,
            &["Pad", "Pluck", "Electronic", "Bright"], "Bi-timbral macro synth with harmonic quantizers",
            [0.80, 0.50, 0.85, 0.30, 0.70, 0.90, 0.45, 0.60],
        ));
        items.push(BrowserItem::new(
            "inst_analog", "Analog", BrowserCategory::Instruments,
            &["Bass", "Lead", "Analog", "Warm"], "Vintage dual-VCO analog synthesizer emulation",
            [0.40, 0.30, 0.45, 0.80, 0.70, 0.35, 0.25, 0.50],
        ));
        items.push(BrowserItem::new(
            "inst_simpler", "Simpler", BrowserCategory::Instruments,
            &["Acoustic", "Electronic"], "Dynamic multi-mode sample playback and transient slicer",
            [0.55, 0.60, 0.60, 0.50, 0.50, 0.50, 0.40, 0.65],
        ));
        items.push(BrowserItem::new(
            "inst_drum_rack", "Drum Rack", BrowserCategory::Instruments,
            &["Percussion", "Electronic"], "16-pad matrix with choke groups and nested chains",
            [0.65, 0.85, 0.70, 0.85, 0.60, 0.75, 0.70, 0.95],
        ));

        // Drums
        items.push(BrowserItem::new(
            "drum_909_kick", "909 Punch Kick", BrowserCategory::Drums,
            &["Kick", "Electronic", "Warm", "Dirty"], "Classic analog punchy 909 bass drum",
            [0.25, 0.80, 0.30, 0.95, 0.40, 0.15, 0.20, 0.92],
        ));
        items.push(BrowserItem::new(
            "drum_808_sub_kick", "808 Sub Kick", BrowserCategory::Drums,
            &["Kick", "Bass", "Electronic", "Dark", "Warm"], "Deep sine sub-boom kick",
            [0.15, 0.40, 0.20, 0.99, 0.20, 0.05, 0.10, 0.75],
        ));
        items.push(BrowserItem::new(
            "drum_909_snare", "909 Crisp Snare", BrowserCategory::Drums,
            &["Snare", "Electronic", "Bright"], "909 tuned snare with snappy noise wire",
            [0.70, 0.88, 0.82, 0.35, 0.80, 0.85, 0.78, 0.90],
        ));
        items.push(BrowserItem::new(
            "drum_acoustic_snare", "Studio Birch Snare", BrowserCategory::Drums,
            &["Snare", "Acoustic", "Warm"], "Multi-velocity acoustic birch snare drum",
            [0.62, 0.70, 0.72, 0.45, 0.75, 0.70, 0.60, 0.82],
        ));
        items.push(BrowserItem::new(
            "drum_909_hihat", "909 Metallic Closed Hat", BrowserCategory::Drums,
            &["Hi-Hat", "Electronic", "Bright"], "6-oscillator transistorized 909 closed hat",
            [0.88, 0.92, 0.95, 0.10, 0.35, 0.98, 0.90, 0.94],
        ));
        items.push(BrowserItem::new(
            "drum_open_hihat", "909 Sizzling Open Hat", BrowserCategory::Drums,
            &["Hi-Hat", "Electronic", "Bright"], "Sustained shimmering 909 open hi-hat",
            [0.85, 0.75, 0.90, 0.12, 0.40, 0.95, 0.82, 0.78],
        ));

        // Audio Effects
        items.push(BrowserItem::new(
            "fx_roar", "Roar", BrowserCategory::AudioEffects,
            &["Dirty", "Warm", "Electronic"], "Dynamic multi-stage saturation and wave-shaping processor",
            [0.60, 0.80, 0.75, 0.70, 0.75, 0.80, 0.65, 0.75],
        ));
        items.push(BrowserItem::new(
            "fx_eq_eight", "EQ Eight", BrowserCategory::AudioEffects,
            &["Clean"], "8-band precision parametric equalizer with real-time spectrum",
            [0.50, 0.20, 0.50, 0.50, 0.50, 0.50, 0.20, 0.20],
        ));
        items.push(BrowserItem::new(
            "fx_glue_comp", "Glue Compressor", BrowserCategory::AudioEffects,
            &["Warm", "Analog"], "Analog SSL-style VCA bus compressor",
            [0.48, 0.45, 0.55, 0.60, 0.65, 0.50, 0.35, 0.60],
        ));
        items.push(BrowserItem::new(
            "fx_auto_filter", "Auto Filter", BrowserCategory::AudioEffects,
            &["Analog", "Warm"], "Zero-delay feedback filter with envelope follower and LFO",
            [0.52, 0.60, 0.65, 0.55, 0.60, 0.65, 0.45, 0.50],
        ));
        items.push(BrowserItem::new(
            "fx_ott", "Multiband Dynamics (OTT)", BrowserCategory::AudioEffects,
            &["Bright", "Electronic"], "3-band aggressive upward/downward compressor",
            [0.78, 0.85, 0.82, 0.70, 0.80, 0.88, 0.60, 0.80],
        ));
        items.push(BrowserItem::new(
            "fx_hybrid_reverb", "Hybrid Reverb", BrowserCategory::AudioEffects,
            &["Warm", "Dark"], "Convolution impulse response combined with algorithmic reverb",
            [0.40, 0.25, 0.45, 0.50, 0.60, 0.55, 0.20, 0.15],
        ));
        items.push(BrowserItem::new(
            "fx_echo", "Echo", BrowserCategory::AudioEffects,
            &["Warm", "Electronic"], "Modulated dual delay with tape tube warmth and ducking",
            [0.55, 0.50, 0.60, 0.55, 0.60, 0.65, 0.35, 0.40],
        ));

        // Grooves
        items.push(BrowserItem::new(
            "groove_mpc_16_60", "MPC 16th 60%", BrowserCategory::Grooves,
            &["Electronic", "Warm"], "Classic Roger Linn MPC 60 swing profile",
            [0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50],
        ));
        items.push(BrowserItem::new(
            "groove_tr909_shuffle", "TR-909 Shuffle", BrowserCategory::Grooves,
            &["Electronic"], "Punchy Roland TR-909 internal shuffle clock swing",
            [0.55, 0.55, 0.55, 0.55, 0.55, 0.55, 0.55, 0.55],
        ));

        Self { items }
    }

    /// Query library by substring, category, and tags.
    pub fn search(
        &self,
        query: &str,
        category: Option<BrowserCategory>,
        tags: Option<&[String]>,
    ) -> Vec<&BrowserItem> {
        let q = query.trim().to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                if let Some(cat) = category {
                    if item.category != cat {
                        return false;
                    }
                }
                if let Some(tag_list) = tags {
                    for req_tag in tag_list {
                        if !item.tags.iter().any(|t| t.eq_ignore_ascii_case(req_tag)) {
                            return false;
                        }
                    }
                }
                if !q.is_empty() {
                    let matches_name = item.name.to_lowercase().contains(&q);
                    let matches_detail = item.detail.to_lowercase().contains(&q);
                    let matches_tag = item.tags.iter().any(|t| t.to_lowercase().contains(&q));
                    if !matches_name && !matches_detail && !matches_tag {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    /// Compute cosine similarity ranking against a target item.
    pub fn find_similar(&self, target_id: &str, top_k: usize) -> Vec<(&BrowserItem, f32)> {
        let target = match self.items.iter().find(|i| i.id == target_id) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut ranked: Vec<(&BrowserItem, f32)> = self
            .items
            .iter()
            .filter(|i| i.id != target_id)
            .map(|i| (i, target.cosine_similarity(i)))
            .collect();

        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(top_k);
        ranked
    }
}
