//! Real-time stereo mix oscilloscope widget with phosphor bloom.

use egui::{pos2, Color32, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Widget};
use crate::theme::{COLOR_BG_VOID, COLOR_BORDER, COLOR_GREEN};

pub struct OscilloscopeWidget<'a> {
    samples: &'a [f32; 256],
    size: egui::Vec2,
}

impl<'a> OscilloscopeWidget<'a> {
    pub fn new(samples: &'a [f32; 256], size: egui::Vec2) -> Self {
        Self { samples, size }
    }
}

impl<'a> Widget for OscilloscopeWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_oscilloscope(&painter, rect, self.samples);
        }

        response
    }
}

pub fn render_oscilloscope(painter: &Painter, rect: Rect, samples: &[f32; 256]) {
    // 1. Background & border
    painter.rect_filled(rect, 2.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

    // 2. Center zero-line grid
    let mid_y = rect.center().y;
    painter.line_segment(
        [pos2(rect.left(), mid_y), pos2(rect.right(), mid_y)],
        Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x22, 0x24)),
    );

    // 3. Phosphor glow pass & primary waveform pass
    let step_x = rect.width() / 255.0;
    let amp_scale = (rect.height() * 0.45).clamp(10.0, 500.0);

    let mut points = Vec::with_capacity(256);
    for (i, &s) in samples.iter().enumerate() {
        let x = rect.left() + i as f32 * step_x;
        let y = mid_y - s.clamp(-1.0, 1.0) * amp_scale;
        points.push(Pos2::new(x, y));
    }

    if points.len() >= 2 {
        // Soft phosphor glow pass
        painter.add(egui::Shape::line(
            points.clone(),
            Stroke::new(3.0_f32, Color32::from_rgba_premultiplied(0x00, 0xe6, 0x76, 0x40)),
        ));

        // Crisp core trace pass
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.5_f32, COLOR_GREEN),
        ));
    }
}
