//! Auto Filter visualizer: Interactive ZDF cutoff/resonance curve with filter circuit types (Clean, OSR, MS2, SMP, PRD).

use egui::{
    pos2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoFilterCircuit {
    Clean,
    Osr,
    Ms2,
    Smp,
    Prd,
}

impl AutoFilterCircuit {
    pub const ALL: [AutoFilterCircuit; 5] = [
        AutoFilterCircuit::Clean,
        AutoFilterCircuit::Osr,
        AutoFilterCircuit::Ms2,
        AutoFilterCircuit::Smp,
        AutoFilterCircuit::Prd,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            AutoFilterCircuit::Clean => "Clean (Cytomic SVF)",
            AutoFilterCircuit::Osr => "OSR (Classic British 24dB)",
            AutoFilterCircuit::Ms2 => "MS2 (Korg 35 Sallen-Key)",
            AutoFilterCircuit::Smp => "SMP (Russian Polivoks)",
            AutoFilterCircuit::Prd => "PRD (Ladder Diode)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutoFilterVisualizerState {
    pub circuit: AutoFilterCircuit,
    pub cutoff_hz: f32, // 20.0 .. 20000.0
    pub resonance: f32, // 0.0 .. 1.0
    pub env_amount: f32, // -1.0 .. +1.0
    pub lfo_amount: f32, // 0.0 .. 1.0
    pub lfo_rate_hz: f32,
    pub live_envelope: f32, // animated mod follower
}

impl Default for AutoFilterVisualizerState {
    fn default() -> Self {
        Self {
            circuit: AutoFilterCircuit::Osr,
            cutoff_hz: 1850.0,
            resonance: 0.65,
            env_amount: 0.45,
            lfo_amount: 0.30,
            lfo_rate_hz: 1.5,
            live_envelope: 0.35,
        }
    }
}

pub struct AutoFilterVisualizerWidget<'a> {
    state: &'a mut AutoFilterVisualizerState,
    size: egui::Vec2,
}

impl<'a> AutoFilterVisualizerWidget<'a> {
    pub fn new(state: &'a mut AutoFilterVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for AutoFilterVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        let log_min = (20.0_f32).ln();
        let log_max = (20000.0_f32).ln();

        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Dragging X adjusts cutoff, Y adjusts resonance
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let new_fc = (log_min + norm_x * (log_max - log_min)).exp();
                let norm_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);

                self.state.cutoff_hz = new_fc.clamp(20.0, 20000.0);
                self.state.resonance = norm_y;
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_auto_filter_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_auto_filter_canvas(painter: &Painter, rect: Rect, state: &AutoFilterVisualizerState) {
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let log_min = (20.0_f32).ln();
    let log_max = (20000.0_f32).ln();

    // 1. Grid lines
    for f in [100.0_f32, 1000.0_f32, 10000.0_f32] {
        let norm_f = (f.ln() - log_min) / (log_max - log_min);
        let x = rect.left() + norm_f * rect.width();
        painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1f, 0x2b)));
    }

    // 2. Interactive ZDF Response Curve
    let effective_fc = (state.cutoff_hz * 2.0_f32.powf(state.env_amount * state.live_envelope * 2.5)).clamp(20.0, 20000.0);
    let fc_norm = (effective_fc.ln() - log_min) / (log_max - log_min);

    let pts_count = 64;
    let mut pts = Vec::with_capacity(pts_count);

    for i in 0..pts_count {
        let t = i as f32 / (pts_count - 1) as f32;
        let delta = t - fc_norm;

        // 4-pole ZDF shape with circuit non-linearity
        let slope = match state.circuit {
            AutoFilterCircuit::Clean => 24.0,
            AutoFilterCircuit::Osr => 26.0,
            AutoFilterCircuit::Ms2 => 28.0,
            AutoFilterCircuit::Smp => 22.0,
            AutoFilterCircuit::Prd => 24.0,
        };

        let response_db = if delta <= 0.0 {
            (state.resonance * 16.0) * (1.0 - delta.abs() * 3.5).max(0.0)
        } else {
            -slope * delta * 4.5 + (state.resonance * 16.0) * (1.0 - delta.abs() * 4.0).max(0.0)
        };

        let clamped_db = response_db.clamp(-36.0, 18.0);
        let px = rect.left() + t * rect.width();
        let py = rect.center().y - (clamped_db / 36.0) * (rect.height() * 0.42);
        pts.push(pos2(px, py));
    }

    if pts.len() >= 2 {
        painter.add(egui::Shape::line(pts, Stroke::new(2.0_f32, COLOR_CYAN)));
    }

    // Dynamic Cutoff Follower Marker
    let marker_x = rect.left() + fc_norm * rect.width();
    let marker_y = rect.center().y - (state.resonance * 16.0 / 36.0) * (rect.height() * 0.42);
    let marker_pos = pos2(marker_x, marker_y);

    painter.circle_filled(marker_pos, 5.0, COLOR_AMBER);
    painter.circle_stroke(marker_pos, 7.0, Stroke::new(1.0_f32, Color32::WHITE));

    // Readout Header
    painter.text(
        pos2(rect.left() + 8.0, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        format!("Auto Filter: {} (Fc: {:.0} Hz, Res: {:.2})",
            state.circuit.name(), effective_fc, state.resonance),
        egui::FontId::monospace(10.0),
        COLOR_AMBER,
    );
}
