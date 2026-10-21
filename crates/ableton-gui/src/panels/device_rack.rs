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
    vec2, Button, Layout, RichText, ScrollArea, Stroke, Ui,
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
                            dev.is_enabled = !dev.is_enabled;
                            action.toggle_bypass = Some(idx);
                        }

                        // Title
                        let title_color = if dev.is_enabled { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_MUTED };
                        ui.label(RichText::new(&dev.name).size(11.0).color(title_color).strong());

                        // Header tools on right
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            // Delete button
                            if ui.add(Button::new(RichText::new("✕").size(9.5).color(COLOR_TEXT_MUTED))).clicked() {
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
                    let vis_h = 145.0;
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
                });

                ui.separator();
            }

            // [+ Add Device] Quick-Picker Button at the end of chain
            ui.vertical(|ui| {
                ui.set_width(110.0);
                ui.add_space(20.0);

                let add_btn = Button::new(
                    RichText::new("+ Add Device")
                        .size(11.0)
                        .color(COLOR_AMBER)
                        .strong(),
                )
                .min_size(vec2(100.0, 36.0))
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, COLOR_AMBER));

                if ui.add(add_btn).clicked() {
                    state.quick_picker_open = !state.quick_picker_open;
                }

                if state.quick_picker_open {
                    for &kind in &DeviceKind::ALL {
                        if ui.add(Button::new(RichText::new(kind.name()).size(10.0).color(COLOR_TEXT_PRIMARY))).clicked() {
                            action.insert_device = Some(kind);
                            state.quick_picker_open = false;
                        }
                    }
                }
            });
        });
    });

    // Execute state modifications
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
