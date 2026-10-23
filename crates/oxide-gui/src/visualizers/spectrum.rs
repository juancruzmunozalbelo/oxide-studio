//! 64-band log-frequency FFT spectrum analyzer visualizer.

use egui::{pos2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget};
use crate::theme::{COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN};

pub struct SpectrumWidget<'a> {
    bins: &'a [f32; 64],
    size: egui::Vec2,
}

impl<'a> SpectrumWidget<'a> {
    pub fn new(bins: &'a [f32; 64], size: egui::Vec2) -> Self {
        Self { bins, size }
    }
}

impl<'a> Widget for SpectrumWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_spectrum(&painter, rect, self.bins);
        }

        response
    }
}

pub fn render_spectrum(painter: &Painter, rect: Rect, bins: &[f32; 64]) {
    // Background and border
    painter.rect_filled(rect, 2.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let num_bins = 64;
    let total_width = rect.width();
    let bar_width = (total_width / num_bins as f32).max(1.0);
    let gap = if bar_width > 2.0 { 1.0 } else { 0.0 };
    let usable_bar_w = (bar_width - gap).max(1.0);

    for (i, &mag) in bins.iter().enumerate() {
        let clamped = mag.clamp(0.0, 1.0);
        let bar_h = clamped * rect.height() * 0.95;
        let x0 = rect.left() + i as f32 * bar_width;
        let y0 = rect.bottom() - bar_h;

        let bar_rect = Rect::from_min_max(
            pos2(x0, y0),
            pos2(x0 + usable_bar_w, rect.bottom()),
        );

        // Cyan bar with amber peak indicator
        let bar_color = if clamped > 0.85 {
            COLOR_AMBER
        } else {
            COLOR_CYAN
        };

        painter.rect_filled(bar_rect, 0.5, bar_color);

        // Top glow cap
        if clamped > 0.05 {
            painter.line_segment(
                [pos2(x0, y0), pos2(x0 + usable_bar_w, y0)],
                Stroke::new(1.0_f32, Color32::WHITE),
            );
        }
    }
}
