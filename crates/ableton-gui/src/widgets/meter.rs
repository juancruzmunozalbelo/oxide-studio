//! Peak and RMS ballistics audio meter with clip LED.

use egui::{pos2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_GREEN, COLOR_RUBY,
};

pub struct StereoMeterWidget {
    peak_l: f32, // linear or dB
    peak_r: f32,
    rms_l: f32,
    rms_r: f32,
    is_clipped: bool,
    size: egui::Vec2,
}

impl StereoMeterWidget {
    pub fn new(peak_l: f32, peak_r: f32, rms_l: f32, rms_r: f32, size: egui::Vec2) -> Self {
        let is_clipped = peak_l >= 0.999 || peak_r >= 0.999;
        Self {
            peak_l,
            peak_r,
            rms_l,
            rms_r,
            is_clipped,
            size,
        }
    }
}

impl Widget for StereoMeterWidget {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_stereo_meter(
                &painter,
                rect,
                self.peak_l,
                self.peak_r,
                self.rms_l,
                self.rms_r,
                self.is_clipped,
            );
        }

        response
    }
}

pub fn render_stereo_meter(
    painter: &Painter,
    rect: Rect,
    peak_l: f32,
    peak_r: f32,
    rms_l: f32,
    rms_r: f32,
    is_clipped: bool,
) {
    painter.rect_filled(rect, 2.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let bar_width = (rect.width() * 0.4).max(3.0);
    let gap = 2.0;
    let x_l = rect.left() + 2.0;
    let x_r = x_l + bar_width + gap;
    let meter_h = rect.height() - 4.0;

    // Helper to draw a single channel bar
    let draw_channel = |x: f32, peak: f32, rms: f32| {
        let p_norm = peak.clamp(0.0, 1.0);
        let r_norm = rms.clamp(0.0, 1.0);

        let p_h = p_norm * meter_h;
        let r_h = r_norm * meter_h;

        // RMS solid fill
        let rms_rect = Rect::from_min_max(
            pos2(x, rect.bottom() - 2.0 - r_h),
            pos2(x + bar_width, rect.bottom() - 2.0),
        );
        let rms_color = if r_norm > 0.85 { COLOR_AMBER } else { COLOR_GREEN };
        painter.rect_filled(rms_rect, 1.0, rms_color);

        // Peak line
        let peak_y = rect.bottom() - 2.0 - p_h;
        let peak_color = if p_norm >= 0.98 { COLOR_RUBY } else { Color32::WHITE };
        painter.line_segment(
            [pos2(x, peak_y), pos2(x + bar_width, peak_y)],
            Stroke::new(1.5_f32, peak_color),
        );
    };

    draw_channel(x_l, peak_l, rms_l);
    draw_channel(x_r, peak_r, rms_r);

    // Clip LED at the top
    let clip_size = 4.0;
    let clip_pos = pos2(rect.right() - clip_size - 2.0, rect.top() + 3.0);
    let clip_color = if is_clipped {
        COLOR_RUBY
    } else {
        Color32::from_rgb(0x38, 0x14, 0x1a)
    };
    painter.circle_filled(clip_pos, clip_size * 0.5, clip_color);
}
