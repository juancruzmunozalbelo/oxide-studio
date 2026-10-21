//! Multiband Dynamics (OTT) visualizer: 3-band vertical dual-threshold compression/expansion meters.

use egui::{
    pos2, vec2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone)]
pub struct BandDynamicsState {
    pub name: &'static str,
    pub crossover_text: &'static str,
    pub above_thresh_db: f32, // -24.0 .. 0.0
    pub below_thresh_db: f32, // -48.0 .. -12.0
    pub gr_meter: f32,        // -1.0 .. +1.0 (negative = compression, positive = expansion)
}

#[derive(Debug, Clone)]
pub struct OttVisualizerState {
    pub low: BandDynamicsState,
    pub mid: BandDynamicsState,
    pub high: BandDynamicsState,
    pub depth: f32, // 0.0 .. 1.0 (default 100%)
    pub time_scale: f32,
    pub output_gain_db: f32,
}

impl Default for OttVisualizerState {
    fn default() -> Self {
        Self {
            low: BandDynamicsState {
                name: "LOW",
                crossover_text: "< 120 Hz",
                above_thresh_db: -14.0,
                below_thresh_db: -32.0,
                gr_meter: -0.45,
            },
            mid: BandDynamicsState {
                name: "MID",
                crossover_text: "120 - 2.5k",
                above_thresh_db: -18.0,
                below_thresh_db: -36.0,
                gr_meter: -0.65,
            },
            high: BandDynamicsState {
                name: "HIGH",
                crossover_text: "> 2.5 kHz",
                above_thresh_db: -12.0,
                below_thresh_db: -28.0,
                gr_meter: 0.35, // upward expansion
            },
            depth: 1.0,
            time_scale: 1.0,
            output_gain_db: 0.0,
        }
    }
}

pub struct OttVisualizerWidget<'a> {
    state: &'a mut OttVisualizerState,
    size: egui::Vec2,
}

impl<'a> OttVisualizerWidget<'a> {
    pub fn new(state: &'a mut OttVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for OttVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_ott_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_ott_canvas(painter: &Painter, rect: Rect, state: &OttVisualizerState) {
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let bands_w = rect.width() * 0.72;
    let bands_rect = Rect::from_min_max(rect.min, pos2(rect.left() + bands_w, rect.bottom()));
    let meta_rect = Rect::from_min_max(pos2(rect.left() + bands_w, rect.top()), rect.max);

    // 1. 3-Band Vertical Dynamics Meters
    let col_w = (bands_rect.width() - 16.0) / 3.0;
    let bands = [&state.low, &state.mid, &state.high];

    for (idx, b) in bands.iter().enumerate() {
        let col_x = bands_rect.left() + 8.0 + idx as f32 * col_w;
        let col_rect = Rect::from_min_size(pos2(col_x, bands_rect.top() + 24.0), vec2(col_w - 6.0, bands_rect.height() - 32.0));

        // Column background
        painter.rect_filled(col_rect, 2.0, Color32::from_rgb(0x13, 0x17, 0x22));
        painter.rect_stroke(col_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

        // Center unity line (0 dB reduction)
        let mid_y = col_rect.center().y;
        painter.line_segment([pos2(col_rect.left(), mid_y), pos2(col_rect.right(), mid_y)], Stroke::new(1.0_f32, COLOR_TEXT_MUTED));

        // Meter fill: upward (cyan expansion) vs downward (amber compression)
        let meter_val = b.gr_meter.clamp(-1.0, 1.0);
        let bar_h = meter_val.abs() * (col_rect.height() * 0.44);

        let (fill_rect, fill_color) = if meter_val <= 0.0 {
            // Downward compression (Amber)
            (
                Rect::from_min_max(pos2(col_rect.left() + 4.0, mid_y), pos2(col_rect.right() - 4.0, mid_y + bar_h)),
                COLOR_AMBER,
            )
        } else {
            // Upward expansion (Cyan)
            (
                Rect::from_min_max(pos2(col_rect.left() + 4.0, mid_y - bar_h), pos2(col_rect.right() - 4.0, mid_y)),
                COLOR_CYAN,
            )
        };
        painter.rect_filled(fill_rect, 1.0, fill_color);

        // Header label
        painter.text(
            pos2(col_rect.center().x, col_rect.top() - 14.0),
            egui::Align2::CENTER_TOP,
            format!("{} ({})", b.name, b.crossover_text),
            egui::FontId::monospace(8.5),
            COLOR_TEXT_PRIMARY,
        );
    }

    // Separator line
    painter.line_segment(
        [pos2(meta_rect.left(), meta_rect.top()), pos2(meta_rect.left(), meta_rect.bottom())],
        Stroke::new(1.0_f32, COLOR_BORDER),
    );

    // 2. OTT Global Controls
    painter.text(
        pos2(meta_rect.left() + 8.0, meta_rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        "OTT Dynamics",
        egui::FontId::monospace(10.5),
        COLOR_AMBER,
    );

    let info_lines = [
        format!("Depth: {:.0}%", state.depth * 100.0),
        format!("Time: {:.1}x", state.time_scale),
        format!("Out: {:+.1} dB", state.output_gain_db),
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
}
