//! Simpler sample visualizer: Interactive waveform zoom with slice markers and start/end handles.

use egui::{
    pos2, vec2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN, COLOR_GREEN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimplerPlayMode {
    Classic,
    OneShot,
    Slicing,
}

#[derive(Debug, Clone)]
pub struct SimplerVisualizerState {
    pub mode: SimplerPlayMode,
    pub sample_name: String,
    pub start_pos: f32, // 0.0 .. 1.0
    pub end_pos: f32,   // 0.0 .. 1.0
    pub loop_enabled: bool,
    pub slice_markers: Vec<f32>, // normalized slice times
    pub active_slice: usize,
    pub playhead_pos: Option<f32>,
}

impl Default for SimplerVisualizerState {
    fn default() -> Self {
        Self {
            mode: SimplerPlayMode::Slicing,
            sample_name: "909_House_Loop_124bpm.wav".to_string(),
            start_pos: 0.0,
            end_pos: 1.0,
            loop_enabled: true,
            slice_markers: vec![0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875],
            active_slice: 2,
            playhead_pos: Some(0.28),
        }
    }
}

pub struct SimplerVisualizerWidget<'a> {
    state: &'a mut SimplerVisualizerState,
    size: egui::Vec2,
}

impl<'a> SimplerVisualizerWidget<'a> {
    pub fn new(state: &'a mut SimplerVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for SimplerVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Clicking waveform selects closest slice or adds slice marker
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let mut closest_idx = 0;
                let mut min_dist = 1.0f32;
                for (idx, &s) in self.state.slice_markers.iter().enumerate() {
                    let d = (s - norm_x).abs();
                    if d < min_dist {
                        min_dist = d;
                        closest_idx = idx;
                    }
                }
                self.state.active_slice = closest_idx;
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_simpler_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_simpler_canvas(painter: &Painter, rect: Rect, state: &SimplerVisualizerState) {
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let wave_rect = Rect::from_min_max(
        pos2(rect.left() + 6.0, rect.top() + 24.0),
        pos2(rect.right() - 6.0, rect.bottom() - 10.0),
    );

    painter.rect_filled(wave_rect, 2.0, Color32::from_rgb(0x10, 0x14, 0x1e));
    painter.rect_stroke(wave_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

    // 1. Synthetic Sample Audio Waveform Peaks
    let pts_count = 120;
    let mid_y = wave_rect.center().y;
    let hw = wave_rect.height() * 0.42;

    for i in 0..pts_count {
        let t = i as f32 / pts_count as f32;
        let x = wave_rect.left() + t * wave_rect.width();

        // Waveform profile with rhythmic drum transient peaks
        let env = (-(t * 8.0).fract() * 4.0).exp();
        let amp = 0.15 + env * 0.75 * (1.0 + (t * 12.0).sin() * 0.2);

        painter.line_segment(
            [pos2(x, mid_y - amp * hw), pos2(x, mid_y + amp * hw)],
            Stroke::new(1.2_f32, Color32::from_rgb(0x28, 0x5a, 0x8a)),
        );
    }

    // 2. Start / End Sample Crop Curtains
    let start_x = wave_rect.left() + state.start_pos.clamp(0.0, 1.0) * wave_rect.width();
    let end_x = wave_rect.left() + state.end_pos.clamp(0.0, 1.0).max(state.start_pos + 0.02) * wave_rect.width();
    if start_x > wave_rect.left() + 1.0 {
        let left_curtain = Rect::from_min_max(wave_rect.min, pos2(start_x, wave_rect.bottom()));
        painter.rect_filled(left_curtain, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 150));
        painter.line_segment([pos2(start_x, wave_rect.top()), pos2(start_x, wave_rect.bottom())], Stroke::new(1.5_f32, COLOR_GREEN));
    }
    if end_x < wave_rect.right() - 1.0 {
        let right_curtain = Rect::from_min_max(pos2(end_x, wave_rect.top()), wave_rect.max);
        painter.rect_filled(right_curtain, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 150));
        painter.line_segment([pos2(end_x, wave_rect.top()), pos2(end_x, wave_rect.bottom())], Stroke::new(1.5_f32, COLOR_GREEN));
    }

    // 3. Slice Markers & Active Region Highlight
    for (idx, &slice_t) in state.slice_markers.iter().enumerate() {
        let sx = wave_rect.left() + slice_t * wave_rect.width();
        let is_active = idx == state.active_slice;
        let marker_color = if is_active { COLOR_AMBER } else { COLOR_CYAN };

        // Slice line
        painter.line_segment(
            [pos2(sx, wave_rect.top()), pos2(sx, wave_rect.bottom())],
            Stroke::new(if is_active { 2.0 } else { 1.0_f32 }, marker_color),
        );

        // Slice flag handle at top
        let flag_rect = Rect::from_min_size(pos2(sx - 3.0, wave_rect.top() - 6.0), vec2(7.0, 6.0));
        painter.rect_filled(flag_rect, 1.0, marker_color);

        painter.text(
            pos2(sx + 3.0, wave_rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            format!("{}", idx + 1),
            egui::FontId::monospace(8.0),
            marker_color,
        );
    }

    // 3. Playhead Position Cursor
    if let Some(ph) = state.playhead_pos {
        let px = wave_rect.left() + ph * wave_rect.width();
        painter.line_segment(
            [pos2(px, wave_rect.top()), pos2(px, wave_rect.bottom())],
            Stroke::new(1.8_f32, COLOR_GREEN),
        );
    }

    // 4. Header Readout
    let mode_str = match state.mode {
        SimplerPlayMode::Classic => "Classic",
        SimplerPlayMode::OneShot => "1-Shot",
        SimplerPlayMode::Slicing => "Slicing",
    };

    painter.text(
        pos2(rect.left() + 8.0, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        format!("Simpler [{}] {} (Slice: {}/{})",
            mode_str, state.sample_name, state.active_slice + 1, state.slice_markers.len()),
        egui::FontId::monospace(10.0),
        COLOR_AMBER,
    );
}
