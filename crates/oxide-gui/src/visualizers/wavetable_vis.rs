//! Wavetable visualizer: 2D/3D morphing wavetable surface + dual ZDF filter curves.

use egui::{
    pos2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
};

#[derive(Debug, Clone)]
pub struct WavetableVisualizerState {
    pub table_pos: f32, // 0.0 .. 1.0 morph position
    pub filter1_cutoff: f32, // 20.0 .. 20000.0 Hz
    pub filter1_res: f32,    // 0.0 .. 1.0
    pub filter2_cutoff: f32,
    pub filter2_res: f32,
    pub mod_matrix: [[f32; 4]; 4], // 4 sources x 4 destinations
}

impl Default for WavetableVisualizerState {
    fn default() -> Self {
        Self {
            table_pos: 0.35,
            filter1_cutoff: 2400.0,
            filter1_res: 0.55,
            filter2_cutoff: 6200.0,
            filter2_res: 0.30,
            mod_matrix: [
                [0.75, 0.0, 0.2, 0.0],
                [0.0, 0.5, 0.0, 0.3],
                [0.4, 0.0, 0.8, 0.0],
                [0.0, 0.0, 0.0, 0.6],
            ],
        }
    }
}

pub struct WavetableVisualizerWidget<'a> {
    state: &'a mut WavetableVisualizerState,
    size: egui::Vec2,
}

impl<'a> WavetableVisualizerWidget<'a> {
    pub fn new(state: &'a mut WavetableVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for WavetableVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let wave_rect_width = rect.width() * 0.55;
                if pos.x >= rect.left() && pos.x <= rect.left() + wave_rect_width {
                    let norm = ((pos.x - rect.left()) / wave_rect_width).clamp(0.0, 1.0);
                    self.state.table_pos = norm;
                    response.mark_changed();
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_wavetable_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_wavetable_canvas(painter: &Painter, rect: Rect, state: &WavetableVisualizerState) {
    if rect.width() < 16.0 || rect.height() < 8.0 {
        return;
    }

    // Background frame
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let half_w = rect.width() * 0.58;
    let wave_rect = Rect::from_min_max(rect.min, pos2(rect.left() + half_w, rect.bottom()));
    let filter_rect = Rect::from_min_max(pos2(rect.left() + half_w, rect.top()), rect.max);

    // 1. Isometric 2D/3D Morphing Wavetable Surface
    let slices = 6;
    for s in (0..slices).rev() {
        let depth_factor = s as f32 / slices as f32;
        let slice_pos = (state.table_pos + depth_factor * 0.4).fract();

        // 3D perspective offsets
        let offset_x = (1.0 - depth_factor) * 14.0;
        let offset_y = (1.0 - depth_factor) * 18.0;

        let is_active = s == 0;
        let alpha = if is_active { 255 } else { (80.0 + (1.0 - depth_factor) * 90.0) as u8 };
        let stroke_color = if is_active {
            COLOR_AMBER
        } else {
            Color32::from_rgba_unmultiplied(0, 210, 255, alpha)
        };

        let pts_count = 48;
        let mut pts = Vec::with_capacity(pts_count);
        for i in 0..pts_count {
            let t = i as f32 / (pts_count - 1) as f32;
            let phase = t * std::f32::consts::TAU;

            // Synthesize morphing waveform: sine -> saw -> square based on slice_pos
            let sine = phase.sin();
            let saw = 2.0 * (t - (t + 0.5).floor());
            let square = if phase.sin() >= 0.0 { 0.85 } else { -0.85 };

            let y_sample = if slice_pos < 0.5 {
                let morph = slice_pos * 2.0;
                sine * (1.0 - morph) + saw * morph
            } else {
                let morph = (slice_pos - 0.5) * 2.0;
                saw * (1.0 - morph) + square * morph
            };

            let px = wave_rect.left() + 8.0 + offset_x + t * (wave_rect.width() - 28.0);
            let py = wave_rect.center().y - offset_y - (y_sample * (wave_rect.height() * 0.28));
            pts.push(pos2(px, py));
        }

        if pts.len() >= 2 {
            painter.add(egui::Shape::line(
                pts,
                Stroke::new(if is_active { 1.8 } else { 1.0_f32 }, stroke_color),
            ));
        }
    }

    // Active position marker text
    painter.text(
        pos2(wave_rect.left() + 8.0, wave_rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        format!("Table Pos: {:.0}%", state.table_pos * 100.0),
        egui::FontId::monospace(10.0),
        COLOR_AMBER,
    );

    // Separator between wave and filter curves
    painter.line_segment(
        [pos2(filter_rect.left(), filter_rect.top()), pos2(filter_rect.left(), filter_rect.bottom())],
        Stroke::new(1.0_f32, COLOR_BORDER),
    );

    // 2. Dual Cytomic ZDF Filter Response Curves (Filter 1: Amber, Filter 2: Cyan)
    if filter_rect.height() < 8.0 || filter_rect.width() < 16.0 {
        return;
    }

    let f_pts_count = 40;
    for (f_idx, (cutoff, res, color, label)) in [
        (state.filter1_cutoff, state.filter1_res, COLOR_AMBER, "ZDF LowPass"),
        (state.filter2_cutoff, state.filter2_res, COLOR_CYAN, "ZDF HighPass/Peak"),
    ]
    .iter()
    .enumerate()
    {
        let mut f_pts = Vec::with_capacity(f_pts_count);
        let log_min = (20.0_f32).ln();
        let log_max = (20000.0_f32).ln();
        let safe_cutoff = cutoff.max(20.0);
        let fc_norm = (safe_cutoff.ln() - log_min) / (log_max - log_min);

        for i in 0..f_pts_count {
            let t = i as f32 / (f_pts_count - 1) as f32;
            let x = filter_rect.left() + 8.0 + t * (filter_rect.width() - 16.0);

            // Approximate 4-pole ZDF magnitude curve
            let delta = t - fc_norm;
            let response_db = if f_idx == 0 {
                // Lowpass
                if delta <= 0.0 {
                    (res * 12.0) * (1.0 - delta.abs() * 3.0).max(0.0)
                } else {
                    -24.0 * delta * 4.0 + (res * 12.0) * (1.0 - delta.abs() * 4.0).max(0.0)
                }
            } else {
                // Highpass / Peak
                if delta >= 0.0 {
                    (res * 10.0) * (1.0 - delta.abs() * 3.0).max(0.0)
                } else {
                    -24.0 * delta.abs() * 4.0 + (res * 10.0) * (1.0 - delta.abs() * 4.0).max(0.0)
                }
            };

            let norm_y = (-response_db / 36.0).clamp(-1.0, 1.0);
            let y = filter_rect.center().y + norm_y * (filter_rect.height() * 0.35);
            let min_y = (filter_rect.top() + 4.0).min(filter_rect.bottom());
            let max_y = (filter_rect.bottom() - 4.0).max(min_y);
            f_pts.push(pos2(x, y.clamp(min_y, max_y)));
        }

        if f_pts.len() >= 2 {
            painter.add(egui::Shape::line(f_pts, Stroke::new(1.4_f32, *color)));
        }

        painter.text(
            pos2(filter_rect.left() + 8.0, filter_rect.top() + 6.0 + f_idx as f32 * 14.0),
            egui::Align2::LEFT_TOP,
            format!("F{}: {} ({:.0} Hz)", f_idx + 1, label, cutoff),
            egui::FontId::monospace(9.5),
            *color,
        );
    }
}
