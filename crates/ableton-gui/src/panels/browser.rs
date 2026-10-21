//! Left collapsible browser panel with category navigation, Live 12 tag pills,
//! instant search, and spectral feature cosine sound similarity ranking.

use egui::{
    vec2, Button, Color32, Layout, Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID,
    COLOR_BORDER, COLOR_CYAN, COLOR_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub const ALL: [BrowserCategory; 10] = [
        BrowserCategory::Sounds,
        BrowserCategory::Drums,
        BrowserCategory::Instruments,
        BrowserCategory::AudioEffects,
        BrowserCategory::MidiEffects,
        BrowserCategory::PlugIns,
        BrowserCategory::Clips,
        BrowserCategory::Samples,
        BrowserCategory::Grooves,
        BrowserCategory::Templates,
    ];

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

pub const ALL_TAGS: [&str; 16] = [
    "Bass", "Lead", "Pad", "Pluck", "Kick", "Snare", "Hi-Hat", "Percussion",
    "Acoustic", "Electronic", "Bright", "Dark", "Warm", "Dirty", "FM", "Analog",
];

/// Item in the browser library.
#[derive(Debug, Clone, PartialEq)]
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

/// Browser state and search/similarity engine.
#[derive(Debug, Clone)]
pub struct BrowserState {
    pub is_collapsed: bool,
    pub width: f32,
    pub selected_category: BrowserCategory,
    pub search_query: String,
    pub active_tags: Vec<String>,
    pub items: Vec<BrowserItem>,
    pub selected_item_id: Option<String>,
    pub similarity_target_id: Option<String>,
    pub similarity_results: Vec<(BrowserItem, f32)>, // top 10 ranked with similarity score
    pub show_similarity_panel: bool,
}

impl Default for BrowserState {
    fn default() -> Self {
        let mut items = Vec::new();

        // Populate comprehensive factory library items
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
            &["Hi-Hat", "Electronic", "Bright"], "Long decay sizzling metallic open hat",
            [0.85, 0.80, 0.92, 0.10, 0.40, 0.95, 0.85, 0.85],
        ));
        items.push(BrowserItem::new(
            "drum_conga", "Afro-Cuban Tumbadora", BrowserCategory::Drums,
            &["Percussion", "Acoustic", "Warm"], "Hand-tuned acoustic conga slap",
            [0.48, 0.60, 0.55, 0.60, 0.80, 0.40, 0.35, 0.88],
        ));

        // Sounds / Presets
        items.push(BrowserItem::new(
            "snd_house_bass", "Deep House Sub Bass", BrowserCategory::Sounds,
            &["Bass", "Electronic", "Warm", "Dark", "Analog"], "Warm saturated analog square/saw sub bass",
            [0.30, 0.45, 0.35, 0.92, 0.45, 0.15, 0.22, 0.60],
        ));
        items.push(BrowserItem::new(
            "snd_fm_slap_bass", "TX81Z Lately Bass", BrowserCategory::Sounds,
            &["Bass", "FM", "Electronic", "Dirty"], "Punchy FM bass staple with percussive transient",
            [0.55, 0.72, 0.65, 0.82, 0.65, 0.45, 0.48, 0.86],
        ));
        items.push(BrowserItem::new(
            "snd_detroit_pad", "Detroit Lush Strings", BrowserCategory::Sounds,
            &["Pad", "Electronic", "Warm", "Analog"], "Lush multi-saw chorus strings with gentle cutoff sweep",
            [0.48, 0.30, 0.52, 0.55, 0.75, 0.50, 0.25, 0.20],
        ));
        items.push(BrowserItem::new(
            "snd_acid_lead", "303 Resonant Screamer", BrowserCategory::Sounds,
            &["Lead", "Dirty", "Analog", "Bright"], "Diode ladder resonant acid lead",
            [0.78, 0.85, 0.84, 0.40, 0.70, 0.88, 0.65, 0.80],
        ));
        items.push(BrowserItem::new(
            "snd_kalimba_pluck", "Crystal Kalimba Pluck", BrowserCategory::Sounds,
            &["Pluck", "Acoustic", "Bright"], "High bell chime with wooden resonance",
            [0.82, 0.65, 0.86, 0.20, 0.50, 0.90, 0.55, 0.90],
        ));

        // Audio Effects
        items.push(BrowserItem::new(
            "fx_eq_eight", "EQ Eight", BrowserCategory::AudioEffects,
            &[], "8-band interactive parametric equalizer",
            [0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50],
        ));
        items.push(BrowserItem::new(
            "fx_glue_comp", "Glue Compressor", BrowserCategory::AudioEffects,
            &["Warm", "Analog"], "SSL-style VCA bus compressor",
            [0.45, 0.60, 0.55, 0.65, 0.60, 0.45, 0.40, 0.70],
        ));
        items.push(BrowserItem::new(
            "fx_roar", "Roar", BrowserCategory::AudioEffects,
            &["Dirty", "Warm"], "Multi-stage non-linear saturation shaper",
            [0.75, 0.88, 0.80, 0.70, 0.85, 0.80, 0.75, 0.85],
        ));
        items.push(BrowserItem::new(
            "fx_auto_filter", "Auto Filter", BrowserCategory::AudioEffects,
            &["Analog", "Warm"], "Interactive ZDF envelope/LFO modulated filter",
            [0.60, 0.70, 0.65, 0.60, 0.70, 0.60, 0.45, 0.55],
        ));
        items.push(BrowserItem::new(
            "fx_ott", "Multiband Dynamics (OTT)", BrowserCategory::AudioEffects,
            &["Bright", "Dirty"], "3-band aggressive upward/downward compressor",
            [0.80, 0.90, 0.88, 0.75, 0.85, 0.92, 0.70, 0.90],
        ));

        // MIDI Effects
        items.push(BrowserItem::new(
            "midi_arpeggiator", "Arpeggiator", BrowserCategory::MidiEffects,
            &[], "Pattern arpeggiator with groove and rate division",
            [0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50],
        ));
        items.push(BrowserItem::new(
            "midi_scale", "Scale", BrowserCategory::MidiEffects,
            &[], "Real-time note quantization to musical scale",
            [0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50],
        ));

        // Plug-Ins
        items.push(BrowserItem::new(
            "plug_vital", "Vital VST3", BrowserCategory::PlugIns,
            &["Lead", "Pad", "Electronic"], "Spectral warping wavetable synthesizer",
            [0.75, 0.65, 0.82, 0.50, 0.60, 0.85, 0.55, 0.72],
        ));

        // Clips
        items.push(BrowserItem::new(
            "clip_house_riff", "Deep House 124 Lead Riff", BrowserCategory::Clips,
            &["Lead", "Electronic", "Warm"], "4-bar quantized MIDI chord progression",
            [0.60, 0.55, 0.65, 0.50, 0.70, 0.60, 0.40, 0.60],
        ));
        items.push(BrowserItem::new(
            "clip_909_groove", "909 Full Beat 124", BrowserCategory::Clips,
            &["Kick", "Snare", "Hi-Hat", "Electronic"], "Classic 909 four-on-the-floor drum loop",
            [0.70, 0.85, 0.80, 0.80, 0.65, 0.85, 0.75, 0.90],
        ));

        // Samples
        items.push(BrowserItem::new(
            "smp_vocal_chop", "Soulful Vocal Chop C3", BrowserCategory::Samples,
            &["Acoustic", "Warm", "Bright"], "Resampled vocal phrase with formant preservation",
            [0.65, 0.60, 0.70, 0.40, 0.75, 0.65, 0.50, 0.65],
        ));
        items.push(BrowserItem::new(
            "smp_analog_sub", "Moog Saw Bass Staccato", BrowserCategory::Samples,
            &["Bass", "Analog", "Dark", "Warm"], "Raw single-cycle analog oscillator waveform",
            [0.35, 0.45, 0.40, 0.90, 0.40, 0.20, 0.25, 0.70],
        ));

        // Grooves
        items.push(BrowserItem::new(
            "grv_swing_16_58", "MPC 16 Swing 58%", BrowserCategory::Grooves,
            &[], "Classic Roger Linn MPC 16th-note swing template",
            [0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50, 0.50],
        ));

        // Templates
        items.push(BrowserItem::new(
            "tmpl_deep_house", "Deep House 124 BPM", BrowserCategory::Templates,
            &["Warm", "Electronic"], "Full club project template with 909 drums & FM bass",
            [0.60, 0.70, 0.65, 0.75, 0.65, 0.60, 0.45, 0.70],
        ));

        Self {
            is_collapsed: false,
            width: 260.0,
            selected_category: BrowserCategory::Sounds,
            search_query: String::new(),
            active_tags: Vec::new(),
            items,
            selected_item_id: Some("snd_house_bass".into()),
            similarity_target_id: None,
            similarity_results: Vec::new(),
            show_similarity_panel: false,
        }
    }
}

impl BrowserState {
    /// Calculate top 10 acoustically similar items using cosine similarity.
    pub fn find_similar_sounds(&mut self, target_id: &str) {
        if let Some(target) = self.items.iter().find(|i| i.id == target_id).cloned() {
            let mut scored: Vec<(BrowserItem, f32)> = self
                .items
                .iter()
                .filter(|i| i.id != target_id)
                .map(|item| {
                    let score = target.cosine_similarity(item);
                    (item.clone(), score)
                })
                .collect();

            // Sort descending by similarity score
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(10);

            self.similarity_target_id = Some(target_id.to_string());
            self.similarity_results = scored;
            self.show_similarity_panel = true;
        }
    }

    /// Toggle tag filter pill.
    pub fn toggle_tag(&mut self, tag: &str) {
        if let Some(pos) = self.active_tags.iter().position(|t| t == tag) {
            self.active_tags.remove(pos);
        } else {
            self.active_tags.push(tag.to_string());
        }
    }

    /// Filtered list of items matching search query, active tags, and selected category.
    pub fn filtered_items(&self) -> Vec<&BrowserItem> {
        let query = self.search_query.trim().to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                // Category match (or if searching, show matches from all categories)
                let cat_match = query.is_empty() && item.category == self.selected_category
                    || !query.is_empty();

                // Search query match
                let query_match = query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item.category.name().to_lowercase().contains(&query)
                    || item.tags.iter().any(|t| t.to_lowercase().contains(&query));

                // Tag match: must contain all active tags if any selected
                let tags_match = self.active_tags.is_empty()
                    || self.active_tags.iter().all(|t| item.tags.contains(t));

                cat_match && query_match && tags_match
            })
            .collect()
    }
}

pub struct BrowserAction {
    pub load_item: Option<BrowserItem>,
    pub find_similar: Option<String>,
}

/// Render the collapsible left-side browser panel.
pub fn render_browser_panel(ui: &mut Ui, state: &mut BrowserState) -> BrowserAction {
    let mut action = BrowserAction {
        load_item: None,
        find_similar: None,
    };

    ui.vertical(|ui| {
        // Top Browser Header & Collapse Button
        ui.horizontal(|ui| {
            let collapse_btn = Button::new(RichText::new("◀").size(11.0).color(COLOR_TEXT_MUTED));
            if ui.add(collapse_btn).clicked() {
                state.is_collapsed = true;
            }

            ui.label(RichText::new("Live 12 Browser").size(12.0).color(COLOR_AMBER).strong());

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if state.show_similarity_panel {
                    let back_btn = Button::new(RichText::new("Library").size(10.0).color(COLOR_CYAN));
                    if ui.add(back_btn).clicked() {
                        state.show_similarity_panel = false;
                    }
                }
            });
        });

        ui.separator();

        // If similarity panel is active, show the top-10 similarity ranked list
        if state.show_similarity_panel {
            ui.label(RichText::new("Similarity Search").size(11.0).color(COLOR_CYAN).strong());
            if let Some(ref target_id) = state.similarity_target_id {
                if let Some(target_item) = state.items.iter().find(|i| &i.id == target_id) {
                    ui.label(RichText::new(format!("Target: {}", target_item.name)).size(10.0).color(COLOR_TEXT_PRIMARY));
                }
            }

            ui.add_space(4.0);

            ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                for (item, score) in &state.similarity_results {
                    let pct = (score * 100.0).round() as u32;
                    let item_clone = item.clone();

                    ui.horizontal(|ui| {
                        // Load button
                        if ui.add(Button::new(RichText::new("+").size(10.0).color(COLOR_GREEN))).clicked() {
                            action.load_item = Some(item_clone.clone());
                        }

                        // Percentage badge
                        let badge_color = if pct >= 90 {
                            COLOR_GREEN
                        } else if pct >= 75 {
                            COLOR_AMBER
                        } else {
                            COLOR_CYAN
                        };
                        ui.label(RichText::new(format!("{:>3}%", pct)).font(egui::FontId::monospace(9.5)).color(badge_color));

                        // Name
                        let name_btn = Button::new(RichText::new(&item.name).size(11.0).color(COLOR_TEXT_PRIMARY))
                            .fill(COLOR_BG_PANEL_ALT);
                        if ui.add(name_btn).double_clicked() {
                            action.load_item = Some(item_clone);
                        }
                    });

                    // Mini percentage bar
                    let (bar_rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(220.0), 3.0), Sense::hover());
                    ui.painter().rect_filled(bar_rect, 1.0, COLOR_BG_VOID);
                    let fill_w = bar_rect.width() * score.clamp(0.0, 1.0);
                    let fill_rect = Rect::from_min_size(bar_rect.min, vec2(fill_w, bar_rect.height()));
                    ui.painter().rect_filled(fill_rect, 1.0, COLOR_AMBER);
                    ui.add_space(2.0);
                }
            });

            return;
        }

        // 1. Instant Search Filter Bar
        ui.horizontal(|ui| {
            ui.label(RichText::new("🔍").size(11.0).color(COLOR_TEXT_MUTED));
            ui.add(
                TextEdit::singleline(&mut state.search_query)
                    .hint_text("Search library...")
                    .desired_width(ui.available_width() - 10.0),
            );
        });

        ui.add_space(4.0);

        // 2. Tag Filter Pills (Live 12 Horizontal Multi-Select Buttons)
        ui.label(RichText::new("Tags:").size(10.0).color(COLOR_TEXT_MUTED));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for &tag in &ALL_TAGS {
                let is_active = state.active_tags.iter().any(|t| t == tag);
                let (fill, text_color, border_color) = if is_active {
                    (COLOR_BG_PANEL_ALT, COLOR_AMBER, COLOR_AMBER)
                } else {
                    (COLOR_BG_SURFACE, COLOR_TEXT_MUTED, COLOR_BORDER)
                };

                let pill = Button::new(RichText::new(tag).size(9.0).color(text_color))
                    .fill(fill)
                    .stroke(Stroke::new(1.0_f32, border_color));

                if ui.add(pill).clicked() {
                    state.toggle_tag(tag);
                }
            }
        });

        ui.add_space(6.0);
        ui.separator();

        // 3. Category Tree Navigation
        ui.label(RichText::new("Categories:").size(10.0).color(COLOR_TEXT_MUTED));
        ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
            for &cat in &BrowserCategory::ALL {
                let is_sel = state.selected_category == cat && state.search_query.is_empty();
                let cat_color = if is_sel { COLOR_AMBER } else { COLOR_TEXT_PRIMARY };
                let cat_btn = Button::new(RichText::new(format!("📁 {}", cat.name())).size(10.5).color(cat_color))
                    .fill(if is_sel { COLOR_BG_PANEL_ALT } else { Color32::TRANSPARENT });

                if ui.add(cat_btn).clicked() {
                    state.selected_category = cat;
                    state.search_query.clear();
                }
            }
        });

        ui.separator();

        // 4. Item List with Double-Click Load & "Find Similar"
        let filtered: Vec<BrowserItem> = state.filtered_items().into_iter().cloned().collect();
        let count_str = format!("Items ({})", filtered.len());
        ui.label(RichText::new(count_str).size(10.5).color(COLOR_TEXT_MUTED));

        ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            if filtered.is_empty() {
                ui.label(RichText::new("No matching items found").size(10.0).color(COLOR_TEXT_MUTED));
            } else {
                for item in filtered {
                    let is_sel = state.selected_item_id.as_deref() == Some(&item.id);
                    let item_clone = item.clone();

                    ui.horizontal(|ui| {
                        // Quick-load button
                        if ui.add(Button::new(RichText::new("+").size(10.0).color(COLOR_GREEN))).clicked() {
                            action.load_item = Some(item_clone.clone());
                        }

                        // Item name button
                        let name_color = if is_sel { COLOR_AMBER } else { COLOR_TEXT_PRIMARY };
                        let name_btn = Button::new(RichText::new(&item.name).size(11.0).color(name_color))
                            .fill(if is_sel { COLOR_BG_PANEL_ALT } else { Color32::TRANSPARENT });

                        let res = ui.add(name_btn);
                        if res.clicked() {
                            state.selected_item_id = Some(item.id.clone());
                        }
                        if res.double_clicked() {
                            action.load_item = Some(item_clone.clone());
                        }

                        // Similarity search icon (≈)
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            let sim_btn = Button::new(RichText::new("≈").size(11.0).color(COLOR_CYAN));
                            if ui.add(sim_btn).on_hover_text("Find Similar Sounds").clicked() {
                                state.find_similar_sounds(&item.id);
                                action.find_similar = Some(item.id.clone());
                            }
                        });
                    });
                }
            }
        });
    });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_similarity_computation() {
        let mut state = BrowserState::default();
        state.find_similar_sounds("snd_house_bass");

        assert_eq!(state.similarity_target_id.as_deref(), Some("snd_house_bass"));
        assert!(!state.similarity_results.is_empty());
        assert!(state.similarity_results.len() <= 10);

        // Verify descending order of similarity
        for i in 1..state.similarity_results.len() {
            assert!(state.similarity_results[i - 1].1 >= state.similarity_results[i].1);
        }

        // Verify cosine similarity value bounds (0.0 .. 1.0)
        for (_, score) in &state.similarity_results {
            assert!(*score >= 0.0 && *score <= 1.0);
        }
    }

    #[test]
    fn test_browser_filtering_and_tags() {
        let mut state = BrowserState::default();
        state.selected_category = BrowserCategory::Drums;
        let drum_items = state.filtered_items();
        assert!(!drum_items.is_empty());

        // Filter by tag "Kick"
        state.toggle_tag("Kick");
        let kicks = state.filtered_items();
        for item in kicks {
            assert!(item.tags.contains(&"Kick".to_string()));
        }

        // Text search
        state.active_tags.clear();
        state.search_query = "Operator".into();
        let ops = state.filtered_items();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].name, "Operator");
    }
}
