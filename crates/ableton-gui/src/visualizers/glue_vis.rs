//! Glue Compressor visualizer: SSL-style analog needle gain-reduction meter (-20..0 dB).

use egui::{
    pos2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_RUBY, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone)]
pub struct GlueCompressorVisualizerState {
    pub gain_reduction_db: f32, // 0.0 .. -20.0 dB
    pub threshold_db: f32,      // -40.0 .. 0.0 dB
    pub ratio: f32,             // 2.0, 4.0, 10.0
    pub attack_ms: f32,
    pub release_ms: f32,
    pub soft_clip: bool,
    pub soft_clip_active: bool,
}

impl Default for GlueCompressorVisualizerState {
    fn default() -> Self {
        Self {
            gain_reduction_db: -4.5,
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 200.0,
            soft_clip: true,
            soft_clip_active: false,
        }
    }
}

pub struct GlueCompressorVisualizerWidget<'a> {
    state: &'a mut GlueCompressorVisualizerState,
    size: egui::Vec2,
}

impl<'a> GlueCompressorVisualizerWidget<'a> {
    pub fn new(state: &'a mut GlueCompressorVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for GlueCompressorVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Dragging horizontally adjusts threshold
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                self.state.threshold_db = -40.0 + norm_x * 40.0;
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_glue_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_glue_canvas(painter: &Painter, rect: Rect, state: &GlueCompressorVisualizerState) {
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let meter_w = rect.width() * 0.62;
    let meter_rect = Rect::from_min_max(rect.min, pos2(rect.left() + meter_w, rect.bottom()));
    let params_rect = Rect::from_min_max(pos2(rect.left() + meter_w, rect.top()), rect.max);

    // 1. SSL Analog GR Needle Gauge (-20 .. 0 dB)
    // Arc background
    let pivot = pos2(meter_rect.center().x, meter_rect.bottom() + 10.0);
    let radius = (meter_rect.height() * 0.85).max(40.0);

    // Arc from angle -135 deg to -45 deg (0 dB is at -45 deg, -20 dB is at -135 deg)
    let min_angle = -std::f32::consts::PI * 0.75; // -135 deg -> -20 dB
    let max_angle = -std::f32::consts::PI * 0.25; // -45 deg -> 0 dB

    // Draw arc ticks
    let ticks = [
        (-20.0, "-20"),
        (-16.0, "-16"),
        (-12.0, "-12"),
        (-8.0, "-8"),
        (-4.0, "-4"),
        (0.0, "0"),
    ];

    for (db, label) in ticks {
        let norm = (db + 20.0) / 20.0; // 0.0 .. 1.0
        let angle = min_angle + norm * (max_angle - min_angle);
        let p_inner = pos2(pivot.x + (radius - 8.0) * angle.cos(), pivot.y + (radius - 8.0) * angle.sin());
        let p_outer = pos2(pivot.x + radius * angle.cos(), pivot.y + radius * angle.sin());

        painter.line_segment([p_inner, p_outer], Stroke::new(1.0_f32, COLOR_TEXT_MUTED));

        let p_text = pos2(pivot.x + (radius - 16.0) * angle.cos(), pivot.y + (radius - 16.0) * angle.sin());
        painter.text(p_text, egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(8.0), COLOR_TEXT_MUTED);
    }

    // Pivoting Analog Needle
    let clamped_gr = state.gain_reduction_db.clamp(-20.0, 0.0);
    let gr_norm = (clamped_gr + 20.0) / 20.0;
    let needle_angle = min_angle + gr_norm * (max_angle - min_angle);
    let needle_tip = pos2(pivot.x + (radius - 2.0) * needle_angle.cos(), pivot.y + (radius - 2.0) * needle_angle.sin());

    painter.line_segment([pivot, needle_tip], Stroke::new(1.8_f32, COLOR_AMBER));
    painter.circle_filled(pivot, 5.0, COLOR_BG_PANEL_ALT);
    painter.circle_stroke(pivot, 5.0, Stroke::new(1.0_f32, COLOR_BORDER));

    // Meter Label
    painter.text(
        pos2(meter_rect.left() + 8.0, meter_rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        format!("Glue GR: {:.1} dB", state.gain_reduction_db),
        egui::FontId::monospace(10.0),
        COLOR_AMBER,
    );

    // Separator line
    painter.line_segment(
        [pos2(params_rect.left(), params_rect.top()), pos2(params_rect.left(), params_rect.bottom())],
        Stroke::new(1.0_f32, COLOR_BORDER),
    );

    // 2. Parameters & Soft Clip LED
    painter.text(
        pos2(params_rect.left() + 8.0, params_rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        "Cytomic SSL",
        egui::FontId::monospace(10.5),
        COLOR_CYAN,
    );

    let params_info = [
        format!("Thresh: {:+.1} dB", state.threshold_db),
        format!("Ratio: {:.0}:1", state.ratio),
        format!("Att: {:.0} ms", state.attack_ms),
        format!("Rel: {:.0} ms", state.release_ms),
    ];

    for (idx, line) in params_info.iter().enumerate() {
        painter.text(
            pos2(params_rect.left() + 8.0, params_rect.top() + 24.0 + idx as f32 * 14.0),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::monospace(9.0),
            COLOR_TEXT_PRIMARY,
        );
    }

    // Soft Clip Indicator LED
    let led_pos = pos2(params_rect.right() - 20.0, params_rect.top() + 12.0);
    let led_color = if state.soft_clip_active {
        COLOR_RUBY
    } else if state.soft_clip {
        Color32::from_rgb(0x30, 0x80, 0x40)
    } else {
        COLOR_BORDER
    };
    painter.circle_filled(led_pos, 4.0, led_color);
    painter.text(
        pos2(led_pos.x - 6.0, led_pos.y),
        egui::Align2::RIGHT_CENTER,
        "CLIP",
        egui::FontId::monospace(8.0),
        COLOR_TEXT_MUTED,
    );
}
