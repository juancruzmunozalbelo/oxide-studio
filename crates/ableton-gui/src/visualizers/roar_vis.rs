//! Roar saturation visualizer: Multi-stage non-linear transfer curves + tone & feedback path.

use egui::{
    pos2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_RUBY, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoarStageMode {
    SoftSine,
    Tube,
    DigitalClip,
    DiodeLadder,
    Multiband,
}

#[derive(Debug, Clone)]
pub struct RoarVisualizerState {
    pub mode: RoarStageMode,
    pub drive_db: f32,    // 0.0 .. 36.0 dB
    pub tone: f32,        // -1.0 .. +1.0
    pub feedback: f32,    // 0.0 .. 0.95
    pub feedback_freq_hz: f32,
    pub stages: usize,    // 1, 2, or 3 stages
}

impl Default for RoarVisualizerState {
    fn default() -> Self {
        Self {
            mode: RoarStageMode::SoftSine,
            drive_db: 14.5,
            tone: 0.25,
            feedback: 0.35,
            feedback_freq_hz: 880.0,
            stages: 2,
        }
    }
}

pub struct RoarVisualizerWidget<'a> {
    state: &'a mut RoarVisualizerState,
    size: egui::Vec2,
}

impl<'a> RoarVisualizerWidget<'a> {
    pub fn new(state: &'a mut RoarVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for RoarVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Dragging horizontally adjusts drive dB
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                self.state.drive_db = norm_x * 36.0;
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_roar_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_roar_canvas(painter: &Painter, rect: Rect, state: &RoarVisualizerState) {
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let curve_w = rect.width() * 0.65;
    let curve_rect = Rect::from_min_max(rect.min, pos2(rect.left() + curve_w, rect.bottom()));
    let meta_rect = Rect::from_min_max(pos2(rect.left() + curve_w, rect.top()), rect.max);

    // 1. Non-linear Transfer Curve: y = f(x)
    let grid_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1f, 0x2b));
    let center = curve_rect.center();
    painter.line_segment([pos2(curve_rect.left(), center.y), pos2(curve_rect.right(), center.y)], grid_stroke);
    painter.line_segment([pos2(center.x, curve_rect.top()), pos2(center.x, curve_rect.bottom())], grid_stroke);

    // Diagonal unity line
    painter.line_segment(
        [pos2(curve_rect.left() + 8.0, curve_rect.bottom() - 8.0), pos2(curve_rect.right() - 8.0, curve_rect.top() + 8.0)],
        Stroke::new(1.0_f32, Color32::from_rgb(0x22, 0x28, 0x38)),
    );

    let pts_count = 64;
    let mut pts = Vec::with_capacity(pts_count);
    let drive_linear = 10.0_f32.powf(state.drive_db / 20.0);

    for i in 0..pts_count {
        let x_norm = (i as f32 / (pts_count - 1) as f32) * 2.0 - 1.0; // -1.0 .. +1.0
        let driven_x = x_norm * drive_linear;

        // Transfer curve functions
        let y_val = match state.mode {
            RoarStageMode::SoftSine => (driven_x * 0.6).sin().clamp(-1.0, 1.0),
            RoarStageMode::Tube => {
                if driven_x >= 0.0 {
                    (driven_x / (1.0 + driven_x)).clamp(0.0, 1.0)
                } else {
                    -((-driven_x).tanh().clamp(0.0, 1.0))
                }
            }
            RoarStageMode::DigitalClip => driven_x.clamp(-0.95, 0.95),
            RoarStageMode::DiodeLadder => (driven_x * 0.75).tanh(),
            RoarStageMode::Multiband => (driven_x * 0.5).sin() * 0.8 + (driven_x * 0.25).cos() * 0.2,
        };

        let px = center.x + x_norm * (curve_rect.width() * 0.44);
        let py = center.y - y_val * (curve_rect.height() * 0.42);
        pts.push(pos2(px, py));
    }

    if pts.len() >= 2 {
        painter.add(egui::Shape::line(pts, Stroke::new(2.0_f32, COLOR_AMBER)));
    }

    // Curve Title & Drive info
    painter.text(
        pos2(curve_rect.left() + 8.0, curve_rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        format!("Roar Drive: +{:.1} dB ({:?})", state.drive_db, state.mode),
        egui::FontId::monospace(10.0),
        COLOR_AMBER,
    );

    // Separator line
    painter.line_segment(
        [pos2(meta_rect.left(), meta_rect.top()), pos2(meta_rect.left(), meta_rect.bottom())],
        Stroke::new(1.0_f32, COLOR_BORDER),
    );

    // 2. Feedback Path & Tone Filter info
    painter.text(
        pos2(meta_rect.left() + 8.0, meta_rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        "Feedback & Tone",
        egui::FontId::monospace(10.5),
        COLOR_CYAN,
    );

    let info_lines = [
        format!("Stages: {}x Series", state.stages),
        format!("Tone: {:+.2}", state.tone),
        format!("Feedback: {:.0}%", state.feedback * 100.0),
        format!("FB Freq: {:.0} Hz", state.feedback_freq_hz),
    ];

    for (idx, line) in info_lines.iter().enumerate() {
        painter.text(
            pos2(meta_rect.left() + 8.0, meta_rect.top() + 28.0 + idx as f32 * 16.0),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::monospace(9.5),
            COLOR_TEXT_PRIMARY,
        );
    }

    // Feedback Loop Arc Indicator
    let fb_center = pos2(meta_rect.left() + 32.0, meta_rect.bottom() - 20.0);
    painter.circle_stroke(fb_center, 12.0, Stroke::new(1.2_f32, COLOR_RUBY));
    painter.text(
        fb_center,
        egui::Align2::CENTER_CENTER,
        "FB",
        egui::FontId::monospace(8.0),
        COLOR_RUBY,
    );
}
