//! Ableton Live 12 Reference Manual Chapter 22 ("Working with Instruments and Effects")
//! & OKF-06 Device Chain View implementation.
//!
//! Provides genuine architecture and UI for:
//! - Ordered Device Chains: `[MIDI Effects]* -> [Instruments]? -> [Audio Effects]*`
//! - Serial and Parallel signal routing
//! - 3x3 "+ Add Device" Quick-Picker grid for all 9 native devices
//! - Zero-latency transparent bypass toggling
//! - Dynamic device insertion, removal, and reordering
//! - Hot-Swap workflow: linking devices to browser categories for seamless replacement

use egui::{
    vec2, Button, DragValue, Layout, RichText, ScrollArea, Stroke, Ui,
};
use crate::panels::browser::BrowserCategory;
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BORDER, COLOR_CYAN,
    COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};
use crate::visualizers::{
    AutoFilterVisualizerState, AutoFilterVisualizerWidget,
    DrumRackVisualizerState, DrumRackVisualizerWidget, EqEightVisualizerState,
    EqEightVisualizerWidget, GlueCompressorVisualizerState, GlueCompressorVisualizerWidget,
    OperatorVisualizerState, OperatorVisualizerWidget, OttVisualizerState, OttVisualizerWidget,
    RoarVisualizerState, RoarVisualizerWidget, SimplerVisualizerState, SimplerVisualizerWidget,
    WavetableVisualizerState, WavetableVisualizerWidget,
};

/// High-level device classification according to Ableton Live 12 Chapter 22.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceCategory {
    MidiEffect,
    Instrument,
    AudioEffect,
}

impl DeviceCategory {
    pub fn name(&self) -> &'static str {
        match self {
            DeviceCategory::MidiEffect => "MIDI Effect",
            DeviceCategory::Instrument => "Instrument",
            DeviceCategory::AudioEffect => "Audio Effect",
        }
    }

    /// Maps device category to default Ableton Live 12 browser category for Hot-Swap.
    pub fn to_browser_category(&self) -> BrowserCategory {
        match self {
            DeviceCategory::MidiEffect => BrowserCategory::MidiEffects,
            DeviceCategory::Instrument => BrowserCategory::Instruments,
            DeviceCategory::AudioEffect => BrowserCategory::AudioEffects,
        }
    }
}

/// Signal routing topology across the device chain (Ableton Live 12 Chapter 22 / OKF-06).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceRouting {
    /// Serial routing: signal passes sequentially left-to-right through each device.
    Serial,
    /// Parallel routing: audio is split across multiple parallel branches and summed at output.
    Parallel {
        branches: usize,
        blend: f32, // 0.0 = full dry/serial, 1.0 = full parallel sum
    },
}

impl Default for DeviceRouting {
    fn default() -> Self {
        Self::Serial
    }
}

/// Hot-Swap Mode state (Ableton Live 12 Chapter 22 Section 22.3).
/// Links a target device in the chain to the Browser for rapid browsing and substitution.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HotSwapState {
    pub active: bool,
    pub target_device_idx: Option<usize>,
    pub category_filter: Option<BrowserCategory>,
    pub preview_preset: Option<String>,
}

impl HotSwapState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn activate(&mut self, device_idx: usize, category: DeviceCategory) {
        self.active = true;
        self.target_device_idx = Some(device_idx);
        self.category_filter = Some(category.to_browser_category());
    }

    pub fn cancel(&mut self) {
        self.active = false;
        self.target_device_idx = None;
        self.category_filter = None;
        self.preview_preset = None;
    }

    pub fn is_active_for(&self, device_idx: usize) -> bool {
        self.active && self.target_device_idx == Some(device_idx)
    }
}

/// The 9 native instruments and effects covered in OKF-06 & Live 12 Suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Wavetable,
    Operator,
    Roar,
    EQEight,
    GlueCompressor,
    AutoFilter,
    OTT,
    Simpler,
    DrumRack,
}

impl DeviceKind {
    pub const ALL: [DeviceKind; 9] = [
        DeviceKind::Wavetable,
        DeviceKind::Operator,
        DeviceKind::Roar,
        DeviceKind::EQEight,
        DeviceKind::GlueCompressor,
        DeviceKind::AutoFilter,
        DeviceKind::OTT,
        DeviceKind::Simpler,
        DeviceKind::DrumRack,
    ];

    /// Canonical 3x3 layout of Native Devices for the "+ Add Device" quick picker grid.
    pub const GRID_3X3: [[(DeviceKind, &'static str); 3]; 3] = [
        [
            (DeviceKind::Wavetable, "Wavetable"),
            (DeviceKind::Operator, "Operator"),
            (DeviceKind::Roar, "Roar"),
        ],
        [
            (DeviceKind::EQEight, "EQ Eight"),
            (DeviceKind::GlueCompressor, "Glue Comp"),
            (DeviceKind::AutoFilter, "Auto Filter"),
        ],
        [
            (DeviceKind::OTT, "OTT Multi"),
            (DeviceKind::Simpler, "Simpler"),
            (DeviceKind::DrumRack, "Drum Rack"),
        ],
    ];

    pub fn name(&self) -> &'static str {
        match self {
            DeviceKind::Wavetable => "Wavetable",
            DeviceKind::Operator => "Operator",
            DeviceKind::Roar => "Roar",
            DeviceKind::EQEight => "EQ Eight",
            DeviceKind::GlueCompressor => "Glue Compressor",
            DeviceKind::AutoFilter => "Auto Filter",
            DeviceKind::OTT => "Multiband Dynamics (OTT)",
            DeviceKind::Simpler => "Simpler",
            DeviceKind::DrumRack => "Drum Rack",
        }
    }

    pub fn category(&self) -> DeviceCategory {
        match self {
            DeviceKind::Wavetable
            | DeviceKind::Operator
            | DeviceKind::Simpler
            | DeviceKind::DrumRack => DeviceCategory::Instrument,
            DeviceKind::Roar
            | DeviceKind::EQEight
            | DeviceKind::GlueCompressor
            | DeviceKind::AutoFilter
            | DeviceKind::OTT => DeviceCategory::AudioEffect,
        }
    }

    pub fn is_instrument(&self) -> bool {
        self.category() == DeviceCategory::Instrument
    }

    pub fn is_audio_fx(&self) -> bool {
        self.category() == DeviceCategory::AudioEffect
    }

    pub fn is_midi_fx(&self) -> bool {
        self.category() == DeviceCategory::MidiEffect
    }
}

/// Gui representation of an individual device instance in a chain.
#[derive(Debug, Clone)]
pub struct GuiDeviceModule {
    pub id: u32,
    pub name: String,
    pub kind: DeviceKind,
    pub is_enabled: bool,
    pub is_collapsed: bool,
    // Embedded Visualizer states
    pub wavetable: WavetableVisualizerState,
    pub operator: OperatorVisualizerState,
    pub roar: RoarVisualizerState,
    pub eq8: EqEightVisualizerState,
    pub glue: GlueCompressorVisualizerState,
    pub auto_filter: AutoFilterVisualizerState,
    pub ott: OttVisualizerState,
    pub simpler: SimplerVisualizerState,
    pub drum_rack: DrumRackVisualizerState,
}

impl GuiDeviceModule {
    pub fn new(id: u32, kind: DeviceKind) -> Self {
        Self {
            id,
            name: kind.name().to_string(),
            kind,
            is_enabled: true,
            is_collapsed: false,
            wavetable: WavetableVisualizerState::default(),
            operator: OperatorVisualizerState::default(),
            roar: RoarVisualizerState::default(),
            eq8: EqEightVisualizerState::default(),
            glue: GlueCompressorVisualizerState::default(),
            auto_filter: AutoFilterVisualizerState::default(),
            ott: OttVisualizerState::default(),
            simpler: SimplerVisualizerState::default(),
            drum_rack: DrumRackVisualizerState::default(),
        }
    }

    pub fn category(&self) -> DeviceCategory {
        self.kind.category()
    }

    /// Process a single audio sample through this device.
    /// When `is_enabled == false`, acts as a transparent unity dry-thru bypass.
    pub fn process_sample(&self, input: f32) -> f32 {
        if !self.is_enabled {
            return input; // Transparent zero-latency bypass
        }

        match self.kind {
            DeviceKind::Roar => {
                let drive_lin = 10.0_f32.powf(self.roar.drive_db / 20.0);
                let driven = (input * drive_lin).tanh();
                driven + self.roar.feedback * input * 0.1
            }
            DeviceKind::GlueCompressor => {
                let gr_lin = 10.0_f32.powf(self.glue.gain_reduction_db / 20.0);
                input * gr_lin
            }
            DeviceKind::AutoFilter => {
                let norm_cutoff = (self.auto_filter.cutoff_hz / 20000.0).clamp(0.01, 1.0);
                input * norm_cutoff * (1.0 + self.auto_filter.resonance * 0.5)
            }
            DeviceKind::EQEight => {
                let band_gain = 10.0_f32.powf(self.eq8.bands[0].gain_db / 20.0);
                input * band_gain
            }
            DeviceKind::OTT => {
                let gr = 10.0_f32.powf(self.ott.mid.gr_meter / 20.0);
                input * gr * self.ott.depth + input * (1.0 - self.ott.depth)
            }
            DeviceKind::Wavetable | DeviceKind::Operator | DeviceKind::Simpler | DeviceKind::DrumRack => {
                input * 0.95
            }
        }
    }
}

/// The state of the Track Device Chain View (Bottom Deck).
#[derive(Debug, Clone)]
pub struct DeviceChainViewState {
    pub devices: Vec<GuiDeviceModule>,
    pub selected_device_idx: usize,
    pub quick_picker_open: bool,
    pub hot_swap: HotSwapState,
    pub routing: DeviceRouting,
}

impl DeviceChainViewState {
    /// Track 1: "F Dorian Chords" (Polyphonic Wavetable Synth)
    /// Wavetable -> Auto Filter -> EQ Eight -> Glue Compressor
    pub fn for_track_1() -> Self {
        Self {
            devices: vec![
                GuiDeviceModule::new(1, DeviceKind::Wavetable),
                GuiDeviceModule::new(2, DeviceKind::AutoFilter),
                GuiDeviceModule::new(3, DeviceKind::EQEight),
                GuiDeviceModule::new(4, DeviceKind::GlueCompressor),
            ],
            selected_device_idx: 0,
            quick_picker_open: false,
            hot_swap: HotSwapState::default(),
            routing: DeviceRouting::Serial,
        }
    }

    /// Track 2: "909 House Kit" (Drum Machine & Dynamics)
    /// Drum Rack -> Roar -> OTT -> Glue Compressor
    pub fn for_track_2() -> Self {
        Self {
            devices: vec![
                GuiDeviceModule::new(10, DeviceKind::DrumRack),
                GuiDeviceModule::new(11, DeviceKind::Roar),
                GuiDeviceModule::new(12, DeviceKind::OTT),
                GuiDeviceModule::new(13, DeviceKind::GlueCompressor),
            ],
            selected_device_idx: 0,
            quick_picker_open: false,
            hot_swap: HotSwapState::default(),
            routing: DeviceRouting::Serial,
        }
    }

    /// Track 3: "Offbeat Sub Bass" (FM 4-Op Operator Bass)
    /// Operator -> Auto Filter -> Roar -> EQ Eight
    pub fn for_track_3() -> Self {
        Self {
            devices: vec![
                GuiDeviceModule::new(20, DeviceKind::Operator),
                GuiDeviceModule::new(21, DeviceKind::AutoFilter),
                GuiDeviceModule::new(22, DeviceKind::Roar),
                GuiDeviceModule::new(23, DeviceKind::EQEight),
            ],
            selected_device_idx: 0,
            quick_picker_open: false,
            hot_swap: HotSwapState::default(),
            routing: DeviceRouting::Serial,
        }
    }

    /// Default device chain configured for a specific track index.
    pub fn default_for_track(track_idx: usize, is_drum: bool) -> Self {
        match track_idx {
            0 => Self::for_track_1(),
            1 => Self::for_track_2(),
            2 => Self::for_track_3(),
            _ => {
                if is_drum {
                    Self::for_track_2()
                } else {
                    Self::for_track_1()
                }
            }
        }
    }

    /// Toggle bypass on device at `idx`.
    pub fn toggle_bypass(&mut self, idx: usize) -> bool {
        if let Some(dev) = self.devices.get_mut(idx) {
            dev.is_enabled = !dev.is_enabled;
            true
        } else {
            false
        }
    }

    /// Insert a device at `idx` in the chain.
    pub fn insert_device(&mut self, idx: usize, kind: DeviceKind) -> bool {
        let new_id = (self.devices.len() as u32) + 1;
        let mod_dev = GuiDeviceModule::new(new_id, kind);
        if idx <= self.devices.len() {
            self.devices.insert(idx, mod_dev);
            self.selected_device_idx = idx;
        } else {
            self.devices.push(mod_dev);
            self.selected_device_idx = self.devices.len() - 1;
        }
        true
    }

    /// Delete device at `idx`.
    pub fn delete_device(&mut self, idx: usize) -> Option<GuiDeviceModule> {
        if idx < self.devices.len() {
            if self.hot_swap.target_device_idx == Some(idx) {
                self.hot_swap.cancel();
            }
            let removed = self.devices.remove(idx);
            if self.selected_device_idx >= self.devices.len() && !self.devices.is_empty() {
                self.selected_device_idx = self.devices.len() - 1;
            }
            Some(removed)
        } else {
            None
        }
    }

    /// Move device left (towards input).
    pub fn move_device_left(&mut self, idx: usize) -> bool {
        if idx > 0 && idx < self.devices.len() {
            self.devices.swap(idx - 1, idx);
            if self.selected_device_idx == idx {
                self.selected_device_idx -= 1;
            } else if self.selected_device_idx == idx - 1 {
                self.selected_device_idx += 1;
            }
            true
        } else {
            false
        }
    }

    /// Move device right (towards output).
    pub fn move_device_right(&mut self, idx: usize) -> bool {
        if idx + 1 < self.devices.len() {
            self.devices.swap(idx, idx + 1);
            if self.selected_device_idx == idx {
                self.selected_device_idx += 1;
            } else if self.selected_device_idx == idx + 1 {
                self.selected_device_idx -= 1;
            }
            true
        } else {
            false
        }
    }

    /// Activate Hot-Swap on device at `idx`.
    pub fn hot_swap_activate(&mut self, idx: usize) -> bool {
        if let Some(dev) = self.devices.get(idx) {
            self.hot_swap.activate(idx, dev.category());
            true
        } else {
            false
        }
    }

    /// Commit hot-swap replacement: replaces target device with `new_kind`.
    pub fn hot_swap_commit(&mut self, new_kind: DeviceKind) -> bool {
        if let Some(target_idx) = self.hot_swap.target_device_idx {
            if target_idx < self.devices.len() {
                let id = self.devices[target_idx].id;
                self.devices[target_idx] = GuiDeviceModule::new(id, new_kind);
                self.hot_swap.cancel();
                return true;
            }
        }
        false
    }

    /// Cancel active Hot-Swap mode.
    pub fn hot_swap_cancel(&mut self) {
        self.hot_swap.cancel();
    }

    /// Validates the Live 12 signal flow rule:
    /// All MIDI effects must precede instruments, and all Audio effects must follow instruments.
    pub fn is_signal_flow_valid(&self) -> bool {
        let mut instrument_seen = false;
        let mut audio_fx_seen = false;

        for dev in &self.devices {
            match dev.category() {
                DeviceCategory::MidiEffect => {
                    if instrument_seen || audio_fx_seen {
                        return false;
                    }
                }
                DeviceCategory::Instrument => {
                    if audio_fx_seen {
                        return false;
                    }
                    instrument_seen = true;
                }
                DeviceCategory::AudioEffect => {
                    audio_fx_seen = true;
                }
            }
        }
        true
    }

    /// Processes an input audio buffer through the entire chain according to the routing topology.
    pub fn process_chain(&self, input: &[f32]) -> Vec<f32> {
        match self.routing {
            DeviceRouting::Serial => {
                let mut buffer = input.to_vec();
                for dev in &self.devices {
                    for s in &mut buffer {
                        *s = dev.process_sample(*s);
                    }
                }
                buffer
            }
            DeviceRouting::Parallel { branches, blend } => {
                if branches <= 1 || self.devices.is_empty() {
                    return self.process_serial(input);
                }

                let dry = input.to_vec();
                let serial = self.process_serial(input);

                // Split devices across parallel branches
                let mut branch_outputs = vec![input.to_vec(); branches];
                for (i, dev) in self.devices.iter().enumerate() {
                    let branch_idx = i % branches;
                    for s in &mut branch_outputs[branch_idx] {
                        *s = dev.process_sample(*s);
                    }
                }

                // Sum parallel branches
                let mut parallel_sum = vec![0.0f32; input.len()];
                for branch in &branch_outputs {
                    for (sum, val) in parallel_sum.iter_mut().zip(branch.iter()) {
                        *sum += *val / (branches as f32);
                    }
                }

                // Blend serial and parallel
                let mut output = vec![0.0f32; input.len()];
                for i in 0..input.len() {
                    output[i] = (1.0 - blend) * serial[i] + blend * parallel_sum[i];
                }
                let _ = dry;
                output
            }
        }
    }

    fn process_serial(&self, input: &[f32]) -> Vec<f32> {
        let mut buffer = input.to_vec();
        for dev in &self.devices {
            for s in &mut buffer {
                *s = dev.process_sample(*s);
            }
        }
        buffer
    }
}

impl Default for DeviceChainViewState {
    fn default() -> Self {
        Self {
            devices: vec![
                GuiDeviceModule::new(1, DeviceKind::Wavetable),
                GuiDeviceModule::new(2, DeviceKind::Roar),
                GuiDeviceModule::new(3, DeviceKind::AutoFilter),
                GuiDeviceModule::new(4, DeviceKind::EQEight),
                GuiDeviceModule::new(5, DeviceKind::OTT),
                GuiDeviceModule::new(6, DeviceKind::GlueCompressor),
            ],
            selected_device_idx: 0,
            quick_picker_open: false,
            hot_swap: HotSwapState::default(),
            routing: DeviceRouting::Serial,
        }
    }
}

/// Action dispatched by the Device Chain GUI component.
#[derive(Debug, Clone, Default)]
pub struct DeviceChainAction {
    pub toggle_bypass: Option<usize>,
    pub delete_device: Option<usize>,
    pub move_device_left: Option<usize>,
    pub move_device_right: Option<usize>,
    pub insert_device: Option<DeviceKind>,
    pub hot_swap_toggle: Option<usize>,
    pub hot_swap_commit: Option<(usize, DeviceKind)>,
    pub hot_swap_cancel: bool,
    pub routing_toggle: bool,
    pub device_parameter_changed: Option<usize>,
}

/// Interactive renderer for the Ableton Live 12 Track Device Chain View (Bottom Deck).
pub fn render_device_chain_view(
    ui: &mut Ui,
    state: &mut DeviceChainViewState,
    spectrum_bins: &[f32; 64],
) -> DeviceChainAction {
    let mut action = DeviceChainAction::default();

    ui.vertical(|ui| {
        // Track Header Badge, Signal Flow Direction, and Routing Mode Selector
        let selected_track_idx = ui.data_mut(|d| {
            d.get_temp::<usize>(egui::Id::new("aruston_selected_track"))
                .unwrap_or(0)
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            let badge_text = format!(
                "⚙ Track {} Signal Chain (MIDI FX -> Instrument -> Audio FX)",
                selected_track_idx + 1
            );
            ui.label(RichText::new(badge_text).size(11.5).color(COLOR_AMBER).strong());

            // Routing Mode Indicator / Toggle
            let (routing_label, routing_color) = match state.routing {
                DeviceRouting::Serial => ("• Serial Routing (Left -> Right)".to_string(), COLOR_TEXT_MUTED),
                DeviceRouting::Parallel { branches, .. } => {
                    (format!("⚡ Parallel Routing ({} Branches)", branches), COLOR_CYAN)
                }
            };
            if ui
                .add(
                    Button::new(RichText::new(&routing_label).size(10.0).color(routing_color))
                        .fill(COLOR_BG_PANEL_ALT),
                )
                .clicked()
            {
                action.routing_toggle = true;
                state.routing = match state.routing {
                    DeviceRouting::Serial => DeviceRouting::Parallel {
                        branches: 2,
                        blend: 0.5,
                    },
                    DeviceRouting::Parallel { .. } => DeviceRouting::Serial,
                };
            }
        });

        // Hot-Swap Active Banner
        if state.hot_swap.active {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                let target_name = state
                    .hot_swap
                    .target_device_idx
                    .and_then(|idx| state.devices.get(idx))
                    .map(|d| d.name.as_str())
                    .unwrap_or("Device");
                let banner_text = format!(
                    "Hot-Swap Active on '{}' — Select preset or device in Browser to replace",
                    target_name
                );
                ui.label(
                    RichText::new(banner_text)
                        .size(10.5)
                        .color(COLOR_AMBER)
                        .strong(),
                );
                if ui
                    .add(
                        Button::new(RichText::new("Cancel (Esc)").size(9.5).color(COLOR_TEXT_PRIMARY))
                            .fill(COLOR_BG_SURFACE)
                            .stroke(Stroke::new(1.0_f32, COLOR_BORDER)),
                    )
                    .clicked()
                {
                    action.hot_swap_cancel = true;
                    state.hot_swap.cancel();
                }
            });
        }

        ui.add_space(2.0);

        ui.horizontal(|ui| {
            // Horizontally Scrollable Chain Container
            ScrollArea::horizontal().show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);

                let dev_count = state.devices.len();
                for idx in 0..dev_count {
                    let is_hot_swapping = state.hot_swap.is_active_for(idx);
                    let dev = &mut state.devices[idx];
                    let _is_sel = idx == state.selected_device_idx;

                    ui.vertical(|ui| {
                        let dev_w = match dev.kind {
                            DeviceKind::Wavetable => 360.0,
                            DeviceKind::Operator => 320.0,
                            DeviceKind::EQEight => 340.0,
                            DeviceKind::Roar => 320.0,
                            DeviceKind::GlueCompressor => 280.0,
                            DeviceKind::AutoFilter => 280.0,
                            DeviceKind::OTT => 280.0,
                            DeviceKind::Simpler => 320.0,
                            DeviceKind::DrumRack => 340.0,
                        };
                        ui.set_width(dev_w);

                        // Device Header Bar (Bypass, Name, Category, Hot-Swap, Reorder, Delete)
                        ui.horizontal(|ui| {
                            // On/Off Bypass Toggle Button
                            let bypass_color = if dev.is_enabled {
                                COLOR_AMBER
                            } else {
                                COLOR_TEXT_MUTED
                            };
                            let bypass_btn = Button::new(RichText::new("●").size(11.0).color(bypass_color))
                                .min_size(vec2(18.0, 18.0))
                                .fill(COLOR_BG_PANEL_ALT);
                            if ui.add(bypass_btn).clicked() {
                                action.toggle_bypass = Some(idx);
                            }

                            // Title & Category
                            let title_color = if dev.is_enabled {
                                COLOR_TEXT_PRIMARY
                            } else {
                                COLOR_TEXT_MUTED
                            };
                            ui.label(RichText::new(&dev.name).size(11.0).color(title_color).strong());

                            // Header tools on right
                            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                // Delete button
                                if ui
                                    .add(Button::new(RichText::new("×").size(9.5).color(COLOR_TEXT_MUTED)))
                                    .clicked()
                                {
                                    action.delete_device = Some(idx);
                                }

                                // Reorder Right
                                if idx + 1 < dev_count {
                                    if ui
                                        .add(Button::new(RichText::new("▶").size(9.0).color(COLOR_TEXT_MUTED)))
                                        .clicked()
                                    {
                                        action.move_device_right = Some(idx);
                                    }
                                }

                                // Reorder Left
                                if idx > 0 {
                                    if ui
                                        .add(Button::new(RichText::new("◀").size(9.0).color(COLOR_TEXT_MUTED)))
                                        .clicked()
                                    {
                                        action.move_device_left = Some(idx);
                                    }
                                }

                                // Hot-Swap button (<>)
                                let hs_color = if is_hot_swapping {
                                    COLOR_AMBER
                                } else {
                                    COLOR_TEXT_MUTED
                                };
                                let hs_btn = Button::new(RichText::new("<>").size(8.5).color(hs_color))
                                    .min_size(vec2(18.0, 18.0))
                                    .fill(COLOR_BG_PANEL_ALT);
                                if ui.add(hs_btn).on_hover_text("Hot-Swap Device / Preset").clicked() {
                                    action.hot_swap_toggle = Some(idx);
                                }
                            });
                        });

                        ui.separator();

                        // Render Custom Interactive Visualizer Widget for this device
                        let vis_h = (ui.available_height() - 32.0).clamp(90.0, 105.0);
                        let vis_size = vec2(dev_w, vis_h);
                        let vis_response = match dev.kind {
                            DeviceKind::Wavetable => {
                                ui.add(WavetableVisualizerWidget::new(&mut dev.wavetable, vis_size))
                            }
                            DeviceKind::Operator => {
                                ui.add(OperatorVisualizerWidget::new(&mut dev.operator, vis_size))
                            }
                            DeviceKind::Roar => {
                                ui.add(RoarVisualizerWidget::new(&mut dev.roar, vis_size))
                            }
                            DeviceKind::EQEight => ui.add(EqEightVisualizerWidget::new(
                                &mut dev.eq8,
                                spectrum_bins,
                                vis_size,
                            )),
                            DeviceKind::GlueCompressor => {
                                ui.add(GlueCompressorVisualizerWidget::new(&mut dev.glue, vis_size))
                            }
                            DeviceKind::AutoFilter => {
                                ui.add(AutoFilterVisualizerWidget::new(&mut dev.auto_filter, vis_size))
                            }
                            DeviceKind::OTT => {
                                ui.add(OttVisualizerWidget::new(&mut dev.ott, vis_size))
                            }
                            DeviceKind::Simpler => {
                                ui.add(SimplerVisualizerWidget::new(&mut dev.simpler, vis_size))
                            }
                            DeviceKind::DrumRack => {
                                ui.add(DrumRackVisualizerWidget::new(&mut dev.drum_rack, vis_size))
                            }
                        };
                        if vis_response.changed() {
                            action.device_parameter_changed = Some(idx);
                        }

                        ui.separator();

                        // 3 compact, interactive parameter readouts/drag-pills + Reset micro-button
                        let mut param_changed = false;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                            match dev.kind {
                                DeviceKind::Wavetable => {
                                    ui.label(RichText::new("Morph").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.wavetable.table_pos)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Cutoff").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.wavetable.filter1_cutoff)
                                                .range(20.0..=20000.0)
                                                .speed(10.0)
                                                .suffix(" Hz"),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Res").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.wavetable.filter1_res)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.wavetable = WavetableVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::Operator => {
                                    let sel_op_idx = match dev.operator.selected_op {
                                        crate::visualizers::OperatorId::A => 0,
                                        crate::visualizers::OperatorId::B => 1,
                                        crate::visualizers::OperatorId::C => 2,
                                        crate::visualizers::OperatorId::D => 3,
                                    };
                                    ui.label(RichText::new("Algo").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(DragValue::new(&mut dev.operator.algorithm_id).range(0..=10))
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Ratio").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.operator.ops[sel_op_idx].ratio)
                                                .range(0.25..=16.0)
                                                .speed(0.25)
                                                .suffix("x"),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Lvl").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.operator.ops[sel_op_idx].level)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.operator = OperatorVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::Roar => {
                                    ui.label(RichText::new("Drive").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.roar.drive_db)
                                                .range(0.0..=36.0)
                                                .speed(0.1)
                                                .suffix(" dB"),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Tone").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.roar.tone)
                                                .range(-1.0..=1.0)
                                                .speed(0.02),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("FB").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.roar.feedback)
                                                .range(0.0..=0.95)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.roar = RoarVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::EQEight => {
                                    let sel_band = dev.eq8.selected_band.min(7);
                                    ui.label(
                                        RichText::new(format!("B{} Freq", sel_band + 1))
                                            .size(9.0)
                                            .color(COLOR_TEXT_MUTED),
                                    );
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.eq8.bands[sel_band].freq_hz)
                                                .range(20.0..=20000.0)
                                                .speed(10.0)
                                                .suffix(" Hz"),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Gain").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.eq8.bands[sel_band].gain_db)
                                                .range(-15.0..=15.0)
                                                .speed(0.1)
                                                .suffix(" dB"),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Q").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.eq8.bands[sel_band].q)
                                                .range(0.1..=18.0)
                                                .speed(0.05),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.eq8 = EqEightVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::GlueCompressor => {
                                    ui.label(RichText::new("Thresh").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.glue.threshold_db)
                                                .range(-40.0..=0.0)
                                                .speed(0.2)
                                                .suffix(" dB"),
                                        )
                                        .changed()
                                    {
                                        dev.glue.recompute_gain_reduction();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Ratio").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.glue.ratio)
                                                .range(2.0..=10.0)
                                                .speed(1.0)
                                                .suffix(":1"),
                                        )
                                        .changed()
                                    {
                                        dev.glue.recompute_gain_reduction();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Atk").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.glue.attack_ms)
                                                .range(0.1..=30.0)
                                                .speed(0.5)
                                                .suffix(" ms"),
                                        )
                                        .changed()
                                    {
                                        dev.glue.recompute_gain_reduction();
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.glue = GlueCompressorVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::AutoFilter => {
                                    ui.label(RichText::new("Cutoff").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.auto_filter.cutoff_hz)
                                                .range(20.0..=20000.0)
                                                .speed(10.0)
                                                .suffix(" Hz"),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Res").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.auto_filter.resonance)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Env").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.auto_filter.env_amount)
                                                .range(-1.0..=1.0)
                                                .speed(0.02),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.auto_filter = AutoFilterVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::OTT => {
                                    ui.label(RichText::new("Depth").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.ott.depth)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        dev.ott.recompute_meters();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Time").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.ott.time_scale)
                                                .range(0.1..=10.0)
                                                .speed(0.1)
                                                .suffix("x"),
                                        )
                                        .changed()
                                    {
                                        dev.ott.recompute_meters();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Gain").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.ott.output_gain_db)
                                                .range(-24.0..=24.0)
                                                .speed(0.2)
                                                .suffix(" dB"),
                                        )
                                        .changed()
                                    {
                                        dev.ott.recompute_meters();
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.ott = OttVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::Simpler => {
                                    ui.label(RichText::new("Start").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.simpler.start_pos)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("End").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(&mut dev.simpler.end_pos)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Slice").size(9.0).color(COLOR_TEXT_MUTED));
                                    let max_slice = dev.simpler.slice_markers.len().saturating_sub(1);
                                    if ui
                                        .add(DragValue::new(&mut dev.simpler.active_slice).range(0..=max_slice))
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.simpler = SimplerVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::DrumRack => {
                                    ui.label(RichText::new("Bank").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(DragValue::new(&mut dev.drum_rack.bank).range(0..=7))
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Pad").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(DragValue::new(&mut dev.drum_rack.selected_pad).range(0..=15))
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    let sel_pad = dev.drum_rack.selected_pad.min(15);
                                    ui.label(RichText::new("Choke").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui
                                        .add(
                                            DragValue::new(
                                                &mut dev.drum_rack.pads[sel_pad].choke_group,
                                            )
                                            .range(0..=16),
                                        )
                                        .changed()
                                    {
                                        param_changed = true;
                                    }
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED),
                                            )
                                            .min_size(vec2(16.0, 16.0)),
                                        )
                                        .on_hover_text("Reset to Defaults")
                                        .clicked()
                                    {
                                        dev.drum_rack = DrumRackVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                            }
                        });
                        if param_changed {
                            action.device_parameter_changed = Some(idx);
                            ui.ctx().request_repaint();
                        }
                    });

                    ui.separator();
                }

                // [+ Add Device] Quick-Picker Button at the end of chain (Live 12 Chapter 22 & OKF-06)
                ui.vertical(|ui| {
                    let col_w = if state.quick_picker_open { 290.0 } else { 110.0 };
                    ui.set_width(col_w);
                    ui.add_space(2.0);

                    let add_btn = Button::new(
                        RichText::new("+ Add Device")
                            .size(11.0)
                            .color(COLOR_AMBER)
                            .strong(),
                    )
                    .min_size(vec2(100.0, 22.0))
                    .fill(COLOR_BG_PANEL_ALT)
                    .stroke(Stroke::new(1.0_f32, COLOR_AMBER));

                    if ui.add(add_btn).clicked() {
                        state.quick_picker_open = !state.quick_picker_open;
                    }

                    // 3x3 Grid of Native Suite Devices
                    if state.quick_picker_open {
                        ui.add_space(2.0);
                        for row in DeviceKind::GRID_3X3 {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for (kind, label) in row {
                                    let btn = Button::new(
                                        RichText::new(label)
                                            .size(9.5)
                                            .color(COLOR_TEXT_PRIMARY),
                                    )
                                    .min_size(vec2(92.0, 20.0));
                                    if ui.add(btn).clicked() {
                                        action.insert_device = Some(kind);
                                        state.quick_picker_open = false;
                                    }
                                }
                            });
                            ui.add_space(2.0);
                        }
                    }
                });
            });
        });
    });

    // Execute state modifications based on actions
    if let Some(bypass_idx) = action.toggle_bypass {
        state.toggle_bypass(bypass_idx);
    }
    if let Some(del_idx) = action.delete_device {
        state.delete_device(del_idx);
    }
    if let Some(left_idx) = action.move_device_left {
        state.move_device_left(left_idx);
    }
    if let Some(right_idx) = action.move_device_right {
        state.move_device_right(right_idx);
    }
    if let Some(insert_kind) = action.insert_device {
        let new_id = (state.devices.len() as u32) + 1;
        state.devices.push(GuiDeviceModule::new(new_id, insert_kind));
        state.selected_device_idx = state.devices.len() - 1;
    }
    if let Some(hs_idx) = action.hot_swap_toggle {
        if state.hot_swap.is_active_for(hs_idx) {
            state.hot_swap.cancel();
        } else {
            state.hot_swap_activate(hs_idx);
        }
    }
    if let Some((hs_idx, new_kind)) = action.hot_swap_commit {
        if hs_idx < state.devices.len() {
            let id = state.devices[hs_idx].id;
            state.devices[hs_idx] = GuiDeviceModule::new(id, new_kind);
            state.hot_swap.cancel();
        }
    }

    action
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_device_chain_selected_track_badge() {
        let ctx = egui::Context::default();
        let mut chain = DeviceChainViewState::default();
        let bins = [0.0f32; 64];

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.data_mut(|d| d.insert_temp(egui::Id::new("aruston_selected_track"), 2usize));
                let act = render_device_chain_view(ui, &mut chain, &bins);
                assert!(act.toggle_bypass.is_none());
            });
        });
    }

    #[test]
    fn test_device_chain_all_device_visualizers_with_parameter_pills() {
        let mut chain = DeviceChainViewState::default();
        chain.devices.push(GuiDeviceModule::new(10, DeviceKind::Operator));
        chain.devices.push(GuiDeviceModule::new(11, DeviceKind::Simpler));
        chain.devices.push(GuiDeviceModule::new(12, DeviceKind::DrumRack));

        let ctx = egui::Context::default();
        let bins = [0.0f32; 64];

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _act = render_device_chain_view(ui, &mut chain, &bins);
            });
        });

        // Test resets across devices
        chain.devices[0].wavetable.table_pos = 0.99;
        chain.devices[0].wavetable = WavetableVisualizerState::default();
        assert_eq!(chain.devices[0].wavetable.table_pos, 0.35);
    }

    #[test]
    fn test_device_chain_per_track_chains_isolated() {
        let t1 = DeviceChainViewState::for_track_1();
        let t2 = DeviceChainViewState::for_track_2();
        let t3 = DeviceChainViewState::for_track_3();

        // Track 1: Wavetable -> AutoFilter -> EQEight -> GlueCompressor
        assert_eq!(t1.devices.len(), 4);
        assert_eq!(t1.devices[0].kind, DeviceKind::Wavetable);
        assert_eq!(t1.devices[1].kind, DeviceKind::AutoFilter);
        assert_eq!(t1.devices[2].kind, DeviceKind::EQEight);
        assert_eq!(t1.devices[3].kind, DeviceKind::GlueCompressor);

        // Track 2: DrumRack -> Roar -> OTT -> GlueCompressor
        assert_eq!(t2.devices.len(), 4);
        assert_eq!(t2.devices[0].kind, DeviceKind::DrumRack);
        assert_eq!(t2.devices[1].kind, DeviceKind::Roar);
        assert_eq!(t2.devices[2].kind, DeviceKind::OTT);
        assert_eq!(t2.devices[3].kind, DeviceKind::GlueCompressor);

        // Track 3: Operator -> AutoFilter -> Roar -> EQEight
        assert_eq!(t3.devices.len(), 4);
        assert_eq!(t3.devices[0].kind, DeviceKind::Operator);
        assert_eq!(t3.devices[1].kind, DeviceKind::AutoFilter);
        assert_eq!(t3.devices[2].kind, DeviceKind::Roar);
        assert_eq!(t3.devices[3].kind, DeviceKind::EQEight);

        // Verify default_for_track mapping
        assert_eq!(
            DeviceChainViewState::default_for_track(0, false).devices[0].kind,
            DeviceKind::Wavetable
        );
        assert_eq!(
            DeviceChainViewState::default_for_track(1, true).devices[0].kind,
            DeviceKind::DrumRack
        );
        assert_eq!(
            DeviceChainViewState::default_for_track(2, false).devices[0].kind,
            DeviceKind::Operator
        );

        // Verify independent mutation
        let mut chains = vec![t1, t2, t3];
        chains[0].devices[0].is_enabled = false;
        assert!(!chains[0].devices[0].is_enabled);
        assert!(chains[1].devices[0].is_enabled);
        assert!(chains[2].devices[0].is_enabled);
    }

    #[test]
    fn test_device_chain_toggle_bypass_execution() {
        let mut chain = DeviceChainViewState::default();
        assert!(!chain.devices.is_empty());
        assert!(chain.devices[0].is_enabled);

        // Toggle bypass off
        assert!(chain.toggle_bypass(0));
        assert!(!chain.devices[0].is_enabled, "Device should be bypassed");

        // Verify transparent dry-thru when bypassed
        let input = 0.75f32;
        let output = chain.devices[0].process_sample(input);
        assert_eq!(output, input, "Bypassed device must output sample bit-exact dry-thru");

        // Toggle bypass back on
        assert!(chain.toggle_bypass(0));
        assert!(chain.devices[0].is_enabled, "Device should be re-enabled");
    }

    #[test]
    fn test_device_chain_serial_vs_parallel_routing() {
        let mut chain = DeviceChainViewState::default();
        // Set up a chain with Roar and Glue Compressor
        chain.devices = vec![
            GuiDeviceModule::new(1, DeviceKind::Roar),
            GuiDeviceModule::new(2, DeviceKind::GlueCompressor),
        ];
        chain.devices[0].roar.drive_db = 18.0; // High saturation
        chain.devices[1].glue.threshold_db = -20.0;
        chain.devices[1].glue.recompute_gain_reduction();

        let input = vec![0.5f32, 0.7f32, -0.4f32, 0.9f32];

        // 1. Serial signal processing
        chain.routing = DeviceRouting::Serial;
        let serial_out = chain.process_chain(&input);
        assert_eq!(serial_out.len(), input.len());
        for &val in &serial_out {
            assert!(val.is_finite());
        }

        // 2. Parallel signal processing (2 branches, 50% mix)
        chain.routing = DeviceRouting::Parallel {
            branches: 2,
            blend: 0.5,
        };
        let parallel_out = chain.process_chain(&input);
        assert_eq!(parallel_out.len(), input.len());

        // Serial and Parallel outputs must reflect genuine topological difference
        let mut has_difference = false;
        for (s, p) in serial_out.iter().zip(parallel_out.iter()) {
            if (s - p).abs() > 1e-4 {
                has_difference = true;
                break;
            }
        }
        assert!(
            has_difference,
            "Parallel routing must produce distinct sum from serial processing"
        );
    }

    #[test]
    fn test_device_chain_3x3_quick_picker_grid() {
        assert_eq!(DeviceKind::GRID_3X3.len(), 3);
        assert_eq!(DeviceKind::GRID_3X3[0].len(), 3);
        assert_eq!(DeviceKind::GRID_3X3[1].len(), 3);
        assert_eq!(DeviceKind::GRID_3X3[2].len(), 3);

        // Verify the 9 native devices are correctly laid out in the 3x3 grid
        assert_eq!(DeviceKind::GRID_3X3[0][0].0, DeviceKind::Wavetable);
        assert_eq!(DeviceKind::GRID_3X3[0][1].0, DeviceKind::Operator);
        assert_eq!(DeviceKind::GRID_3X3[0][2].0, DeviceKind::Roar);
        assert_eq!(DeviceKind::GRID_3X3[1][0].0, DeviceKind::EQEight);
        assert_eq!(DeviceKind::GRID_3X3[1][1].0, DeviceKind::GlueCompressor);
        assert_eq!(DeviceKind::GRID_3X3[1][2].0, DeviceKind::AutoFilter);
        assert_eq!(DeviceKind::GRID_3X3[2][0].0, DeviceKind::OTT);
        assert_eq!(DeviceKind::GRID_3X3[2][1].0, DeviceKind::Simpler);
        assert_eq!(DeviceKind::GRID_3X3[2][2].0, DeviceKind::DrumRack);

        // Verify insertion via quick picker
        let mut chain = DeviceChainViewState::default();
        let initial_count = chain.devices.len();
        chain.quick_picker_open = true;

        // Simulate picking Auto Filter from row 1, col 2
        let picked = DeviceKind::GRID_3X3[1][2].0;
        chain.insert_device(chain.devices.len(), picked);
        chain.quick_picker_open = false;

        assert_eq!(chain.devices.len(), initial_count + 1);
        assert_eq!(chain.devices.last().unwrap().kind, DeviceKind::AutoFilter);
        assert!(!chain.quick_picker_open);
    }

    #[test]
    fn test_device_chain_dynamic_insertion_removal_and_reorder() {
        let mut chain = DeviceChainViewState::default();
        chain.devices.clear();

        // 1. Insert devices
        assert!(chain.insert_device(0, DeviceKind::Wavetable));
        assert!(chain.insert_device(1, DeviceKind::AutoFilter));
        assert!(chain.insert_device(2, DeviceKind::Roar));
        assert_eq!(chain.devices.len(), 3);
        assert_eq!(chain.devices[0].kind, DeviceKind::Wavetable);
        assert_eq!(chain.devices[1].kind, DeviceKind::AutoFilter);
        assert_eq!(chain.devices[2].kind, DeviceKind::Roar);

        // 2. Reorder right: move Wavetable from 0 to 1
        assert!(chain.move_device_right(0));
        assert_eq!(chain.devices[0].kind, DeviceKind::AutoFilter);
        assert_eq!(chain.devices[1].kind, DeviceKind::Wavetable);

        // 3. Reorder left: move Wavetable back from 1 to 0
        assert!(chain.move_device_left(1));
        assert_eq!(chain.devices[0].kind, DeviceKind::Wavetable);
        assert_eq!(chain.devices[1].kind, DeviceKind::AutoFilter);

        // 4. Delete device at 1
        let deleted = chain.delete_device(1);
        assert!(deleted.is_some());
        assert_eq!(deleted.unwrap().kind, DeviceKind::AutoFilter);
        assert_eq!(chain.devices.len(), 2);
        assert_eq!(chain.devices[0].kind, DeviceKind::Wavetable);
        assert_eq!(chain.devices[1].kind, DeviceKind::Roar);

        // Out-of-bounds safety
        assert!(!chain.move_device_left(0));
        assert!(!chain.move_device_right(1));
        assert!(chain.delete_device(99).is_none());
    }

    #[test]
    fn test_device_chain_hot_swap_workflow() {
        let mut chain = DeviceChainViewState::for_track_1();
        assert_eq!(chain.devices[0].kind, DeviceKind::Wavetable);

        // 1. Activate Hot-Swap on device 0 (Wavetable Instrument)
        assert!(chain.hot_swap_activate(0));
        assert!(chain.hot_swap.active);
        assert_eq!(chain.hot_swap.target_device_idx, Some(0));
        assert_eq!(
            chain.hot_swap.category_filter,
            Some(BrowserCategory::Instruments)
        );

        // 2. Commit hot-swap replacement to Operator
        assert!(chain.hot_swap_commit(DeviceKind::Operator));
        assert_eq!(chain.devices[0].kind, DeviceKind::Operator);
        assert!(!chain.hot_swap.active);
        assert!(chain.hot_swap.target_device_idx.is_none());

        // 3. Activate Hot-Swap on device 1 (AutoFilter Audio FX)
        assert!(chain.hot_swap_activate(1));
        assert_eq!(
            chain.hot_swap.category_filter,
            Some(BrowserCategory::AudioEffects)
        );

        // 4. Cancel Hot-Swap
        chain.hot_swap_cancel();
        assert!(!chain.hot_swap.active);
        assert_eq!(chain.devices[1].kind, DeviceKind::AutoFilter);
    }

    #[test]
    fn test_device_chain_signal_flow_order_validation() {
        let mut chain = DeviceChainViewState::default();
        // Valid order: Wavetable (Instrument) -> Roar (Audio FX) -> AutoFilter (Audio FX)
        chain.devices = vec![
            GuiDeviceModule::new(1, DeviceKind::Wavetable),
            GuiDeviceModule::new(2, DeviceKind::Roar),
            GuiDeviceModule::new(3, DeviceKind::AutoFilter),
        ];
        assert!(chain.is_signal_flow_valid());

        // Valid order: only Audio Effects on an audio track
        chain.devices = vec![
            GuiDeviceModule::new(1, DeviceKind::AutoFilter),
            GuiDeviceModule::new(2, DeviceKind::Roar),
            GuiDeviceModule::new(3, DeviceKind::EQEight),
        ];
        assert!(chain.is_signal_flow_valid());

        // Invalid order: Audio Effect placed before Instrument
        chain.devices = vec![
            GuiDeviceModule::new(1, DeviceKind::Roar),
            GuiDeviceModule::new(2, DeviceKind::Wavetable),
        ];
        assert!(
            !chain.is_signal_flow_valid(),
            "Audio effect placed before Instrument violates signal flow rule"
        );
    }

    #[test]
    fn test_glue_compressor_needle_and_ott_meters_respond_to_params() {
        let mut glue = GlueCompressorVisualizerState::default();
        assert!((glue.gain_reduction_db - (-4.5)).abs() < 0.05);

        glue.threshold_db = -33.902;
        glue.recompute_gain_reduction();
        assert!(
            glue.gain_reduction_db < -15.0,
            "Expected needle below -15 dB at -33.9 dB threshold, got {}",
            glue.gain_reduction_db
        );
        assert!(glue.soft_clip_active);

        glue.threshold_db = -2.0;
        glue.recompute_gain_reduction();
        assert_eq!(glue.gain_reduction_db, 0.0);
        assert!(!glue.soft_clip_active);

        let mut ott = OttVisualizerState::default();
        let full_mid = ott.mid.gr_meter;
        ott.depth = 0.2;
        ott.recompute_meters();
        assert!(ott.mid.gr_meter.abs() < full_mid.abs());
    }
}
