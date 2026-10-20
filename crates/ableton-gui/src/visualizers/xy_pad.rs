//! Interactive XY Perform Pad with phosphor decay trail and audio-reactive RMS bloom.

use egui::{
    pos2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, Ui,
};
use crate::theme::{COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_TEXT_MUTED};

#[derive(Debug, Clone)]
pub struct PhosphorPoint {
    pub pos: Pos2,
    pub timestamp: f64,
}

#[derive(Debug, Clone)]
pub struct XyPerformPadState {
    pub x: f32, // 0.0 .. 1.0
    pub y: f32, // 0.0 .. 1.0
    pub trail: Vec<PhosphorPoint>,
}

impl Default for XyPerformPadState {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            trail: Vec::with_capacity(64),
        }
    }
}

pub struct XyPerformPadWidget<'a> {
    state: &'a mut XyPerformPadState,
    current_time: f64,
    rms_bloom: f32, // 0.0 .. 1.0
    size: egui::Vec2,
}

impl<'a> XyPerformPadWidget<'a> {
    pub fn new(
        state: &'a mut XyPerformPadState,
        current_time: f64,
        rms_bloom: f32,
        size: egui::Vec2,
    ) -> Self {
        Self {
            state,
            current_time,
            rms_bloom,
            size,
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::drag());

        // Handle pointer interaction
        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let norm_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);

                self.state.x = norm_x;
                self.state.y = norm_y;
                response.mark_changed();

                // Add to phosphor trail
                self.state.trail.push(PhosphorPoint {
                    pos: pos2(
                        rect.left() + norm_x * rect.width(),
                        rect.bottom() - norm_y * rect.height(),
                    ),
                    timestamp: self.current_time,
                });
            }
        }

        // Age phosphor trail (decay over 1.2s)
        let now = self.current_time;
        self.state.trail.retain(|pt| now - pt.timestamp < 1.2);

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_xy_pad(
                &painter,
                rect,
                self.state.x,
                self.state.y,
                &self.state.trail,
                now,
                self.rms_bloom,
            );
        }

        response
    }
}

pub fn render_xy_pad(
    painter: &Painter,
    rect: Rect,
    norm_x: f32,
    norm_y: f32,
    trail: &[PhosphorPoint],
    current_time: f64,
    rms_bloom: f32,
) {
    // 1. Background and border
    painter.rect_filled(rect, 4.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, COLOR_BORDER));

    // 2. Crosshair grid
    let mid_x = rect.center().x;
    let mid_y = rect.center().y;
    painter.line_segment(
        [pos2(mid_x, rect.top()), pos2(mid_x, rect.bottom())],
        Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
    );
    painter.line_segment(
        [pos2(rect.left(), mid_y), pos2(rect.right(), mid_y)],
        Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
    );

    // Axis Labels
    painter.text(
        pos2(rect.right() - 8.0, rect.bottom() - 8.0),
        egui::Align2::RIGHT_BOTTOM,
        "X: CUTOFF",
        FontId::new(9.0, FontFamily::Monospace),
        COLOR_TEXT_MUTED,
    );
    painter.text(
        pos2(rect.left() + 8.0, rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        "Y: RESONANCE",
        FontId::new(9.0, FontFamily::Monospace),
        COLOR_TEXT_MUTED,
    );

    // 3. Phosphor trail with alpha decay
    for pt in trail {
        let age = (current_time - pt.timestamp).max(0.0) as f32;
        let alpha = (1.0 - (age / 1.2)).clamp(0.0, 1.0);
        let radius = 6.0 * (1.0 + age * 1.5);
        let alpha_u8 = (alpha * 120.0) as u8;

        painter.circle_stroke(
            pt.pos,
            radius,
            Stroke::new(
                1.5 * alpha,
                Color32::from_rgba_premultiplied(0x00, 0xd2, 0xff, alpha_u8),
            ),
        );
    }

    // 4. Current Puck Position
    let puck_pos = pos2(
        rect.left() + norm_x.clamp(0.0, 1.0) * rect.width(),
        rect.bottom() - norm_y.clamp(0.0, 1.0) * rect.height(),
    );

    // 5. Audio-Reactive RMS Bloom Ring
    let bloom_factor = rms_bloom.clamp(0.0, 1.0);
    let bloom_radius = 12.0 + bloom_factor * 28.0;
    let bloom_alpha = (0.2 + bloom_factor * 0.6).min(1.0);
    painter.circle_stroke(
        puck_pos,
        bloom_radius,
        Stroke::new(
            1.5_f32,
            Color32::from_rgba_premultiplied(
                0xff,
                0xbe,
                0x00,
                (bloom_alpha * 200.0) as u8,
            ),
        ),
    );

    // Core puck
    painter.circle_filled(puck_pos, 7.0, COLOR_AMBER);
    painter.circle_stroke(puck_pos, 7.0, Stroke::new(1.5_f32, Color32::WHITE));
    painter.circle_filled(puck_pos, 2.5, Color32::WHITE);

    // Readout
    let readout = format!("X: {:.2}  Y: {:.2}", norm_x, norm_y);
    painter.text(
        pos2(rect.left() + 8.0, rect.bottom() - 8.0),
        egui::Align2::LEFT_BOTTOM,
        readout,
        FontId::new(10.0, FontFamily::Monospace),
        COLOR_AMBER,
    );
}
