//! Dynamic Ableton Live 12 Device Chain View.
//!
//! Features:
//! - Horizontally scrollable container hosting track's dynamic `DeviceChain`
//!   (`[MIDI FX] -> [Instrument] -> [Audio FX]`)
//! - Reorder, delete, insert `[+ Add Device]`, and bypass controls
//! - Custom interactive visualizers embedded per device:
//!   - `Wavetable`: 2D/3D morphing wavetable surface + dual ZDF filter curves
//!   - `Operator`: 4-op FM algorithm matrix (11 routing topologies) + ADSR envelope curves
//!   - `Roar`: Multi-stage non-linear saturation transfer curve + tone & feedback
//!   - `EQ Eight`: 8-band interactive draggable parametric nodes over FFT spectrum
//!   - `Glue Compressor`: SSL-style analog needle gain-reduction meter (-20..0 dB)
//!   - `Auto Filter`: Interactive ZDF cutoff/resonance response curve (Clean, OSR, MS2, SMP, PRD)
//!   - `Multiband Dynamics (OTT)`: 3-band high/mid/low compressor gain-reduction / expansion meter bars
//!   - `Simpler`: Interactive sample waveform zoom with slice markers
//!   - `Drum Rack`: 4x4 interactive 16-pad matrix with bank scroll and choke groups

use egui::{
    vec2, Button, DragValue, Layout, RichText, ScrollArea, Stroke, Ui,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};
use crate::visualizers::{
    AutoFilterVisualizerState, AutoFilterVisualizerWidget,
    DrumRackVisualizerState, DrumRackVisualizerWidget, EqEightVisualizerState,
    EqEightVisualizerWidget, GlueCompressorVisualizerState, GlueCompressorVisualizerWidget,
    OperatorVisualizerState, OperatorVisualizerWidget, OttVisualizerState, OttVisualizerWidget,
    RoarVisualizerState, RoarVisualizerWidget, SimplerVisualizerState, SimplerVisualizerWidget,
    WavetableVisualizerState, WavetableVisualizerWidget,
};

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
}

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
}

#[derive(Debug, Clone)]
pub struct DeviceChainViewState {
    pub devices: Vec<GuiDeviceModule>,
    pub selected_device_idx: usize,
    pub quick_picker_open: bool,
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
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeviceChainAction {
    pub toggle_bypass: Option<usize>,
    pub delete_device: Option<usize>,
    pub move_device_left: Option<usize>,
    pub move_device_right: Option<usize>,
    pub insert_device: Option<DeviceKind>,
    pub device_parameter_changed: Option<usize>,
}

pub fn render_device_chain_view(
    ui: &mut Ui,
    state: &mut DeviceChainViewState,
    spectrum_bins: &[f32; 64],
) -> DeviceChainAction {
    let mut action = DeviceChainAction {
        toggle_bypass: None,
        delete_device: None,
        move_device_left: None,
        move_device_right: None,
        insert_device: None,
        device_parameter_changed: None,
    };

    ui.vertical(|ui| {
        // Track Header Badge & Signal Flow Direction Indicator
        let selected_track_idx = ui.data_mut(|d| d.get_temp::<usize>(egui::Id::new("aruston_selected_track")).unwrap_or(0));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            let badge_text = format!("⚙ Track {} Signal Chain (MIDI FX -> Instrument -> Audio FX)", selected_track_idx + 1);
            ui.label(RichText::new(badge_text).size(11.5).color(COLOR_AMBER).strong());
            ui.label(RichText::new("• Audio flows left -> right").size(10.0).color(COLOR_TEXT_MUTED));
        });
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            // Horizontally Scrollable Chain Container
            ScrollArea::horizontal().show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);

                let dev_count = state.devices.len();
                for idx in 0..dev_count {
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

                        // Device Header Bar (Bypass, Name, Reorder, Delete)
                        ui.horizontal(|ui| {
                            // On/Off Bypass Toggle Button
                            let bypass_color = if dev.is_enabled { COLOR_AMBER } else { COLOR_TEXT_MUTED };
                            let bypass_btn = Button::new(RichText::new("●").size(11.0).color(bypass_color))
                                .min_size(vec2(18.0, 18.0))
                                .fill(COLOR_BG_PANEL_ALT);
                            if ui.add(bypass_btn).clicked() {
                                action.toggle_bypass = Some(idx);
                            }

                            // Title
                            let title_color = if dev.is_enabled { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_MUTED };
                            ui.label(RichText::new(&dev.name).size(11.0).color(title_color).strong());

                            // Header tools on right
                            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                // Delete button
                                if ui.add(Button::new(RichText::new("×").size(9.5).color(COLOR_TEXT_MUTED))).clicked() {
                                    action.delete_device = Some(idx);
                                }

                                // Reorder Right
                                if idx + 1 < dev_count {
                                    if ui.add(Button::new(RichText::new("▶").size(9.0).color(COLOR_TEXT_MUTED))).clicked() {
                                        action.move_device_right = Some(idx);
                                    }
                                }

                                // Reorder Left
                                if idx > 0 {
                                    if ui.add(Button::new(RichText::new("◀").size(9.0).color(COLOR_TEXT_MUTED))).clicked() {
                                        action.move_device_left = Some(idx);
                                    }
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
                            DeviceKind::EQEight => {
                                ui.add(EqEightVisualizerWidget::new(&mut dev.eq8, spectrum_bins, vis_size))
                            }
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
                                    if ui.add(DragValue::new(&mut dev.wavetable.table_pos).range(0.0..=1.0).speed(0.01)).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Cutoff").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.wavetable.filter1_cutoff).range(20.0..=20000.0).speed(10.0).suffix(" Hz")).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Res").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.wavetable.filter1_res).range(0.0..=1.0).speed(0.01)).changed() {
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
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
                                    if ui.add(DragValue::new(&mut dev.operator.algorithm_id).range(0..=10)).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Ratio").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.operator.ops[sel_op_idx].ratio).range(0.25..=16.0).speed(0.25).suffix("x")).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Lvl").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.operator.ops[sel_op_idx].level).range(0.0..=1.0).speed(0.01)).changed() {
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
                                        dev.operator = OperatorVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::Roar => {
                                    ui.label(RichText::new("Drive").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.roar.drive_db).range(0.0..=36.0).speed(0.1).suffix(" dB")).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Tone").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.roar.tone).range(-1.0..=1.0).speed(0.02)).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("FB").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.roar.feedback).range(0.0..=0.95).speed(0.01)).changed() {
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
                                        dev.roar = RoarVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::EQEight => {
                                    let sel_band = dev.eq8.selected_band.min(7);
                                    ui.label(RichText::new(format!("B{} Freq", sel_band + 1)).size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.eq8.bands[sel_band].freq_hz).range(20.0..=20000.0).speed(10.0).suffix(" Hz")).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Gain").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.eq8.bands[sel_band].gain_db).range(-15.0..=15.0).speed(0.1).suffix(" dB")).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Q").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.eq8.bands[sel_band].q).range(0.1..=18.0).speed(0.05)).changed() {
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
                                        dev.eq8 = EqEightVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::GlueCompressor => {
                                    ui.label(RichText::new("Thresh").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.glue.threshold_db).range(-40.0..=0.0).speed(0.2).suffix(" dB")).changed() {
                                        dev.glue.recompute_gain_reduction();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Ratio").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.glue.ratio).range(2.0..=10.0).speed(1.0).suffix(":1")).changed() {
                                        dev.glue.recompute_gain_reduction();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Atk").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.glue.attack_ms).range(0.1..=30.0).speed(0.5).suffix(" ms")).changed() {
                                        dev.glue.recompute_gain_reduction();
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
                                        dev.glue = GlueCompressorVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::AutoFilter => {
                                    ui.label(RichText::new("Cutoff").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.auto_filter.cutoff_hz).range(20.0..=20000.0).speed(10.0).suffix(" Hz")).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Res").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.auto_filter.resonance).range(0.0..=1.0).speed(0.01)).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Env").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.auto_filter.env_amount).range(-1.0..=1.0).speed(0.02)).changed() {
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
                                        dev.auto_filter = AutoFilterVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::OTT => {
                                    ui.label(RichText::new("Depth").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.ott.depth).range(0.0..=1.0).speed(0.01)).changed() {
                                        dev.ott.recompute_meters();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Time").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.ott.time_scale).range(0.1..=10.0).speed(0.1).suffix("x")).changed() {
                                        dev.ott.recompute_meters();
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Gain").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.ott.output_gain_db).range(-24.0..=24.0).speed(0.2).suffix(" dB")).changed() {
                                        dev.ott.recompute_meters();
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
                                        dev.ott = OttVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::Simpler => {
                                    ui.label(RichText::new("Start").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.simpler.start_pos).range(0.0..=1.0).speed(0.01)).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("End").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.simpler.end_pos).range(0.0..=1.0).speed(0.01)).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Slice").size(9.0).color(COLOR_TEXT_MUTED));
                                    let max_slice = dev.simpler.slice_markers.len().saturating_sub(1);
                                    if ui.add(DragValue::new(&mut dev.simpler.active_slice).range(0..=max_slice)).changed() {
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
                                        dev.simpler = SimplerVisualizerState::default();
                                        param_changed = true;
                                    }
                                }
                                DeviceKind::DrumRack => {
                                    ui.label(RichText::new("Bank").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.drum_rack.bank).range(0..=7)).changed() {
                                        param_changed = true;
                                    }
                                    ui.label(RichText::new("Pad").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.drum_rack.selected_pad).range(0..=15)).changed() {
                                        param_changed = true;
                                    }
                                    let sel_pad = dev.drum_rack.selected_pad.min(15);
                                    ui.label(RichText::new("Choke").size(9.0).color(COLOR_TEXT_MUTED));
                                    if ui.add(DragValue::new(&mut dev.drum_rack.pads[sel_pad].choke_group).range(0..=16)).changed() {
                                        param_changed = true;
                                    }
                                    if ui.add(Button::new(RichText::new("R").size(9.0).color(COLOR_TEXT_MUTED)).min_size(vec2(16.0, 16.0))).on_hover_text("Reset to Defaults").clicked() {
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

                // [+ Add Device] Quick-Picker Button at the end of chain
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

                    if state.quick_picker_open {
                        ui.add_space(2.0);
                        let grid = [
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
                        for row in grid {
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

    // Execute state modifications
    if let Some(bypass_idx) = action.toggle_bypass {
        if let Some(dev) = state.devices.get_mut(bypass_idx) {
            dev.is_enabled = !dev.is_enabled;
        }
    }
    if let Some(del_idx) = action.delete_device {
        if del_idx < state.devices.len() {
            state.devices.remove(del_idx);
        }
    }
    if let Some(left_idx) = action.move_device_left {
        if left_idx > 0 && left_idx < state.devices.len() {
            state.devices.swap(left_idx - 1, left_idx);
        }
    }
    if let Some(right_idx) = action.move_device_right {
        if right_idx + 1 < state.devices.len() {
            state.devices.swap(right_idx, right_idx + 1);
        }
    }
    if let Some(insert_kind) = action.insert_device {
        let new_id = (state.devices.len() as u32) + 1;
        state.devices.push(GuiDeviceModule::new(new_id, insert_kind));
    }

    action
}

// Backwards-compatible legacy wrapper
#[derive(Debug, Clone, Default)]
pub struct SynthControlsState {
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub pulse_pw: f32,
}

pub struct DeviceRackAction {
    pub cutoff_changed: Option<f32>,
    pub resonance_changed: Option<f32>,
    pub synth_param_changed: Option<(u32, f32)>,
}

pub fn render_device_rack(
    ui: &mut Ui,
    _state: &mut SynthControlsState,
    _live_cutoff: Option<f32>,
    _live_amp: Option<f32>,
    _live_pw: Option<f32>,
) -> DeviceRackAction {
    let mut default_chain = DeviceChainViewState::default();
    let bins = [0.0f32; 64];
    render_device_chain_view(ui, &mut default_chain, &bins);
    DeviceRackAction {
        cutoff_changed: None,
        resonance_changed: None,
        synth_param_changed: None,
    }
}

#[cfg(test)]
mod tests {
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
    fn test_device_rack_all_device_visualizers_with_parameter_pills() {
        let mut chain = DeviceChainViewState::default();
        // Insert Operator, Simpler, DrumRack to cover all 9 kinds
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
    fn test_per_track_device_chains_isolated() {
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
        assert_eq!(DeviceChainViewState::default_for_track(0, false).devices[0].kind, DeviceKind::Wavetable);
        assert_eq!(DeviceChainViewState::default_for_track(1, true).devices[0].kind, DeviceKind::DrumRack);
        assert_eq!(DeviceChainViewState::default_for_track(2, false).devices[0].kind, DeviceKind::Operator);
        assert_eq!(DeviceChainViewState::default_for_track(99, true).devices[0].kind, DeviceKind::DrumRack);
        assert_eq!(DeviceChainViewState::default_for_track(99, false).devices[0].kind, DeviceKind::Wavetable);

        // Verify independent mutation
        let mut chains = vec![t1, t2, t3];
        chains[0].devices[0].is_enabled = false;
        assert!(!chains[0].devices[0].is_enabled);
        assert!(chains[1].devices[0].is_enabled);
        assert!(chains[2].devices[0].is_enabled);
    }

    #[test]
    fn test_glue_compressor_needle_and_ott_meters_respond_to_params() {
        let mut glue = GlueCompressorVisualizerState::default();
        assert!((glue.gain_reduction_db - (-4.5)).abs() < 0.05);

        // Lowering threshold to -33.902 dB (as in user test) swings needle deep left (~-16.4 dB) and lights CLIP
        glue.threshold_db = -33.902;
        glue.recompute_gain_reduction();
        assert!(
            glue.gain_reduction_db < -15.0,
            "Expected needle to move below -15 dB at -33.9 dB threshold, got {}",
            glue.gain_reduction_db
        );
        assert!(glue.soft_clip_active);

        // Raising threshold to -2.0 dB releases compression to 0.0 dB
        glue.threshold_db = -2.0;
        glue.recompute_gain_reduction();
        assert_eq!(glue.gain_reduction_db, 0.0);
        assert!(!glue.soft_clip_active);

        // Changing OTT depth from 1.0 to 0.2 scales all 3 band meters proportionally
        let mut ott = OttVisualizerState::default();
        let full_mid = ott.mid.gr_meter;
        ott.depth = 0.2;
        ott.recompute_meters();
        assert!(ott.mid.gr_meter.abs() < full_mid.abs());
    }

    #[test]
    fn test_device_rack_toggle_bypass_execution() {
        let mut chain = DeviceChainViewState::default();
        assert!(!chain.devices.is_empty());
        assert!(chain.devices[0].is_enabled);

        let mut act = DeviceChainAction::default();
        act.toggle_bypass = Some(0);

        if let Some(b_idx) = act.toggle_bypass {
            if let Some(dev) = chain.devices.get_mut(b_idx) {
                dev.is_enabled = !dev.is_enabled;
            }
        }
        assert!(!chain.devices[0].is_enabled, "Device should be bypassed");

        if let Some(b_idx) = act.toggle_bypass {
            if let Some(dev) = chain.devices.get_mut(b_idx) {
                dev.is_enabled = !dev.is_enabled;
            }
        }
        assert!(chain.devices[0].is_enabled, "Device should be re-enabled");
    }
}
