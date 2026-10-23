//! Operator FM visualizer: 4-operator algorithm matrix (11 routing topologies) + ADSR envelope curves.

use egui::{
    pos2, vec2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN, COLOR_GREEN,
    COLOR_RUBY, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorId {
    A,
    B,
    C,
    D,
}

#[derive(Debug, Clone)]
pub struct OperatorVoiceParams {
    pub ratio: f32,
    pub level: f32, // 0.0 .. 1.0
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

impl Default for OperatorVoiceParams {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            level: 0.8,
            attack_ms: 5.0,
            decay_ms: 300.0,
            sustain: 0.6,
            release_ms: 250.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperatorVisualizerState {
    pub algorithm_id: usize, // 0 .. 10 (11 topologies)
    pub selected_op: OperatorId,
    pub ops: [OperatorVoiceParams; 4], // A, B, C, D
}

impl Default for OperatorVisualizerState {
    fn default() -> Self {
        Self {
            algorithm_id: 0,
            selected_op: OperatorId::A,
            ops: [
                OperatorVoiceParams { ratio: 1.0, level: 1.0, attack_ms: 2.0, decay_ms: 400.0, sustain: 0.8, release_ms: 300.0 },
                OperatorVoiceParams { ratio: 2.0, level: 0.75, attack_ms: 4.0, decay_ms: 250.0, sustain: 0.4, release_ms: 200.0 },
                OperatorVoiceParams { ratio: 3.0, level: 0.5, attack_ms: 10.0, decay_ms: 150.0, sustain: 0.2, release_ms: 150.0 },
                OperatorVoiceParams { ratio: 0.5, level: 0.3, attack_ms: 1.0, decay_ms: 100.0, sustain: 0.1, release_ms: 100.0 },
            ],
        }
    }
}

pub struct OperatorVisualizerWidget<'a> {
    state: &'a mut OperatorVisualizerState,
    size: egui::Vec2,
}

impl<'a> OperatorVisualizerWidget<'a> {
    pub fn new(state: &'a mut OperatorVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for OperatorVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click());

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Check if clicking operator box on left
                let op_names = [OperatorId::D, OperatorId::C, OperatorId::B, OperatorId::A];
                let op_box_w = 42.0;
                let op_box_h = 24.0;
                let start_x = rect.left() + 10.0;
                let start_y = rect.top() + 12.0;

                for (idx, &op) in op_names.iter().enumerate() {
                    let box_rect = Rect::from_min_size(
                        pos2(start_x, start_y + idx as f32 * 28.0),
                        vec2(op_box_w, op_box_h),
                    );
                    if box_rect.contains(pos) {
                        self.state.selected_op = op;
                        response.mark_changed();
                        break;
                    }
                }

                // Check if clicking algorithm selector button at top
                let algo_rect = Rect::from_min_size(pos2(rect.left() + 60.0, rect.top() + 6.0), vec2(80.0, 20.0));
                if algo_rect.contains(pos) {
                    self.state.algorithm_id = (self.state.algorithm_id + 1) % 11;
                    response.mark_changed();
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_operator_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_operator_canvas(painter: &Painter, rect: Rect, state: &OperatorVisualizerState) {
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let left_w = 150.0;
    let algo_rect = Rect::from_min_max(rect.min, pos2(rect.left() + left_w, rect.bottom()));
    let env_rect = Rect::from_min_max(pos2(rect.left() + left_w, rect.top()), rect.max);

    // 1. FM Algorithm Matrix & Routing Column
    painter.text(
        pos2(algo_rect.left() + 8.0, algo_rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        format!("Algo: #{}/11", state.algorithm_id + 1),
        egui::FontId::monospace(10.5),
        COLOR_AMBER,
    );

    let op_items = [
        (OperatorId::D, 3, "D", COLOR_RUBY),
        (OperatorId::C, 2, "C", COLOR_CYAN),
        (OperatorId::B, 1, "B", COLOR_CYAN),
        (OperatorId::A, 0, "A", COLOR_GREEN), // Carrier
    ];

    let box_w = 36.0;
    let box_h = 22.0;
    for (row, (op_id, idx, label, default_color)) in op_items.iter().enumerate() {
        let is_selected = state.selected_op == *op_id;
        let box_pos = pos2(algo_rect.left() + 14.0, algo_rect.top() + 26.0 + row as f32 * 26.0);
        let b_rect = Rect::from_min_size(box_pos, vec2(box_w, box_h));

        let fill_color = if is_selected {
            COLOR_BG_PANEL_ALT
        } else {
            Color32::from_rgb(0x18, 0x1c, 0x24)
        };
        painter.rect_filled(b_rect, 2.0, fill_color);
        painter.rect_stroke(
            b_rect,
            2.0,
            Stroke::new(if is_selected { 1.8 } else { 1.0_f32 }, if is_selected { COLOR_AMBER } else { *default_color }),
        );

        painter.text(
            b_rect.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::monospace(11.0),
            if is_selected { COLOR_AMBER } else { *default_color },
        );

        // Parameters readout next to box
        let op_data = &state.ops[*idx];
        let info = format!("{}x {:.2}", op_data.ratio, op_data.level);
        painter.text(
            pos2(b_rect.right() + 8.0, b_rect.center().y),
            egui::Align2::LEFT_CENTER,
            info,
            egui::FontId::monospace(9.5),
            COLOR_TEXT_PRIMARY,
        );

        // Routing link arrow down to next operator if algorithm routes serially
        if row < 3 && state.algorithm_id == 0 {
            let arrow_start = pos2(b_rect.center().x, b_rect.bottom());
            let arrow_end = pos2(b_rect.center().x, b_rect.bottom() + 4.0);
            painter.line_segment([arrow_start, arrow_end], Stroke::new(1.0_f32, COLOR_TEXT_MUTED));
        }
    }

    // Separator line
    painter.line_segment(
        [pos2(env_rect.left(), env_rect.top()), pos2(env_rect.left(), env_rect.bottom())],
        Stroke::new(1.0_f32, COLOR_BORDER),
    );

    // 2. Selected Operator ADSR Envelope Curve
    let sel_idx = match state.selected_op {
        OperatorId::A => 0,
        OperatorId::B => 1,
        OperatorId::C => 2,
        OperatorId::D => 3,
    };
    let op_params = &state.ops[sel_idx];

    let header = format!("Op {:?} ADSR Envelope (Attack: {:.0}ms, Decay: {:.0}ms, Sus: {:.2}, Rel: {:.0}ms)",
        state.selected_op, op_params.attack_ms, op_params.decay_ms, op_params.sustain, op_params.release_ms);

    painter.text(
        pos2(env_rect.left() + 8.0, env_rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        header,
        egui::FontId::monospace(10.0),
        COLOR_CYAN,
    );

    let graph_rect = Rect::from_min_max(
        pos2(env_rect.left() + 12.0, env_rect.top() + 24.0),
        pos2(env_rect.right() - 12.0, env_rect.bottom() - 12.0),
    );

    painter.rect_filled(graph_rect, 2.0, Color32::from_rgb(0x10, 0x14, 0x1d));
    painter.rect_stroke(graph_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let gw = graph_rect.width();
    let gh = graph_rect.height();

    // ADSR points dynamically scaled by attack_ms, decay_ms, sustain, release_ms, and level
    let atk_frac = (op_params.attack_ms / 100.0).clamp(0.05, 0.32);
    let dec_frac = (atk_frac + (op_params.decay_ms / 1200.0).clamp(0.10, 0.35)).min(0.68);
    let sus_end_frac = (dec_frac + 0.22).min(0.88);
    let rel_end_frac = (sus_end_frac + (op_params.release_ms / 1500.0).clamp(0.08, 0.25)).min(1.0);

    let p_start = pos2(graph_rect.left(), graph_rect.bottom());
    let p_attack = pos2(graph_rect.left() + gw * atk_frac, graph_rect.top() + gh * (1.0 - op_params.level));
    let p_decay = pos2(graph_rect.left() + gw * dec_frac, graph_rect.top() + gh * (1.0 - op_params.level * op_params.sustain));
    let p_sustain_end = pos2(graph_rect.left() + gw * sus_end_frac, graph_rect.top() + gh * (1.0 - op_params.level * op_params.sustain));
    let p_end = pos2(graph_rect.left() + gw * rel_end_frac, graph_rect.bottom());

    let pts = vec![p_start, p_attack, p_decay, p_sustain_end, p_end];
    painter.add(egui::Shape::line(pts, Stroke::new(1.8_f32, COLOR_CYAN)));

    for &pt in &[p_attack, p_decay, p_sustain_end] {
        painter.circle_filled(pt, 3.0, COLOR_AMBER);
    }
}
