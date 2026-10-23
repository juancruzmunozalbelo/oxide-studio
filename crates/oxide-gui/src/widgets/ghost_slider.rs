//! Studio slider with live modulation ghost indicator.
//!
//! Renders:
//! - Track groove in carbon surface
//! - Filled active region
//! - White base thumb (draggable)
//! - Glowing Cyan ghost indicator showing the instantaneous modulated value

use egui::{
    pos2, vec2, Color32, FontFamily, FontId, Rect, Response, Sense, Stroke, Ui,
};
use crate::theme::{COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN, COLOR_TEXT_MUTED};

pub struct GhostSlider<'a> {
    label: &'a str,
    value: &'a mut f32,
    min: f32,
    max: f32,
    ghost_value: Option<f32>,
    format_fn: Option<Box<dyn Fn(f32) -> String + 'a>>,
}

impl<'a> GhostSlider<'a> {
    pub fn new(label: &'a str, value: &'a mut f32, min: f32, max: f32) -> Self {
        Self {
            label,
            value,
            min,
            max,
            ghost_value: None,
            format_fn: None,
        }
    }

    pub fn with_ghost(mut self, ghost: Option<f32>) -> Self {
        self.ghost_value = ghost;
        self
    }

    pub fn with_format<F: Fn(f32) -> String + 'a>(mut self, f: F) -> Self {
        self.format_fn = Some(Box::new(f));
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let desired_size = vec2(ui.available_width().max(80.0), 22.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::drag());

        let min_val = self.min;
        let max_val = self.max;
        let range = (max_val - min_val).max(1e-6);

        // Interaction
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let track_rect = Rect::from_min_max(
                    pos2(rect.left() + 70.0, rect.top() + 6.0),
                    pos2(rect.right() - 50.0, rect.bottom() - 6.0),
                );
                let norm = ((pos.x - track_rect.left()) / track_rect.width().max(1.0)).clamp(0.0, 1.0);
                *self.value = min_val + norm * range;
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            // 1. Label on left
            painter.text(
                pos2(rect.left(), rect.center().y),
                egui::Align2::LEFT_CENTER,
                self.label,
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_TEXT_MUTED,
            );

            // 2. Track groove
            let track_left = rect.left() + 70.0;
            let track_right = rect.right() - 50.0;
            let track_width = (track_right - track_left).max(10.0);
            let track_y = rect.center().y;

            let groove_rect = Rect::from_min_max(
                pos2(track_left, track_y - 2.5),
                pos2(track_right, track_y + 2.5),
            );
            painter.rect_filled(groove_rect, 2.0, COLOR_BG_VOID);
            painter.rect_stroke(groove_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

            // Filled active region
            let current_norm = ((*self.value - min_val) / range).clamp(0.0, 1.0);
            let active_rect = Rect::from_min_max(
                pos2(track_left, track_y - 2.0),
                pos2(track_left + current_norm * track_width, track_y + 2.0),
            );
            painter.rect_filled(active_rect, 1.5, Color32::from_rgb(0x28, 0x38, 0x54));

            // 3. Live Modulation Ghost Indicator (Cyan glowing marker)
            if let Some(g_val) = self.ghost_value {
                let g_norm = ((g_val - min_val) / range).clamp(0.0, 1.0);
                let ghost_x = track_left + g_norm * track_width;

                painter.circle_stroke(
                    pos2(ghost_x, track_y),
                    6.0,
                    Stroke::new(1.5_f32, Color32::from_rgba_premultiplied(0x00, 0xd2, 0xff, 0x90)),
                );
                painter.circle_filled(pos2(ghost_x, track_y), 3.0, COLOR_CYAN);
            }

            // 4. Base Value Thumb (White draggable circle)
            let thumb_x = track_left + current_norm * track_width;
            let thumb_hovered = response.hovered() || response.dragged();
            let thumb_radius = if thumb_hovered { 6.0 } else { 5.0 };

            painter.circle_filled(pos2(thumb_x, track_y), thumb_radius, Color32::WHITE);
            painter.circle_stroke(
                pos2(thumb_x, track_y),
                thumb_radius,
                Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
            );

            // 5. Readout on right
            let readout = if let Some(ref fmt) = self.format_fn {
                fmt(*self.value)
            } else {
                format!("{:.2}", *self.value)
            };
            painter.text(
                pos2(rect.right(), rect.center().y),
                egui::Align2::RIGHT_CENTER,
                readout,
                FontId::new(11.0, FontFamily::Monospace),
                Color32::from_rgb(0xed, 0xf2, 0xf7),
            );
        }

        response
    }
}

impl<'a> egui::Widget for GhostSlider<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}
