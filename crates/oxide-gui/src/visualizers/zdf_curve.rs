//! Interactive 4-pole ZDF frequency-response curve widget.
//!
//! Evaluates the 4-pole ladder magnitude equation:
//! |H(f)| = 1 / sqrt(1 + (f / fc)^8) + 3.6 * resonance^0.9 * exp(-((log2(f / fc) / 0.35)^2))
//! across 20 Hz .. 20 kHz with -48 dB .. +12 dB vertical range.
//! Supports interactive puck dragging for cutoff and resonance.

use egui::{
    pos2, Color32, FontFamily, FontId, Painter, Rect, Response, Sense, Stroke, Ui,
};
use crate::theme::{COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN, COLOR_TEXT_MUTED};

pub struct ZdfCurveWidget<'a> {
    cutoff_hz: &'a mut f32,
    resonance: &'a mut f32,
    live_cutoff_hz: Option<f32>,
    size: egui::Vec2,
}

impl<'a> ZdfCurveWidget<'a> {
    pub fn new(
        cutoff_hz: &'a mut f32,
        resonance: &'a mut f32,
        live_cutoff_hz: Option<f32>,
        size: egui::Vec2,
    ) -> Self {
        Self {
            cutoff_hz,
            resonance,
            live_cutoff_hz,
            size,
        }
    }

    /// Calculate magnitude in dB at frequency f.
    pub fn magnitude_db(f: f32, fc: f32, res: f32) -> f32 {
        let f_safe = f.max(1.0);
        let fc_safe = fc.clamp(20.0, 20000.0);
        let ratio = f_safe / fc_safe;
        let base = 1.0 / (1.0 + ratio.powi(8)).sqrt();

        let log_ratio = (ratio).log2();
        let peak_term = 3.6 * res.clamp(0.0, 1.0).powf(0.9) * (-((log_ratio / 0.35).powi(2))).exp();

        let mag_linear = (base + peak_term).max(1e-4);
        20.0 * mag_linear.log10()
    }

    /// Frequency (20Hz..20kHz) to normalized X (0.0..1.0).
    pub fn freq_to_norm_x(f: f32) -> f32 {
        let min_log = 20.0f32.log10();
        let max_log = 20000.0f32.log10();
        let f_log = f.clamp(20.0, 20000.0).log10();
        ((f_log - min_log) / (max_log - min_log)).clamp(0.0, 1.0)
    }

    /// Normalized X (0.0..1.0) to frequency (20Hz..20kHz).
    pub fn norm_x_to_freq(norm_x: f32) -> f32 {
        let min_log = 20.0f32.log10();
        let max_log = 20000.0f32.log10();
        let log_val = min_log + norm_x.clamp(0.0, 1.0) * (max_log - min_log);
        10.0f32.powf(log_val)
    }

    /// dB (-48dB..+12dB) to normalized Y (0.0=bottom, 1.0=top).
    pub fn db_to_norm_y(db: f32) -> f32 {
        let min_db = -48.0;
        let max_db = 12.0;
        ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0)
    }
}

impl<'a> ZdfCurveWidget<'a> {
    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::drag());

        // Handle interactive drag on cutoff/resonance
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let norm_y = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);

                *self.cutoff_hz = Self::norm_x_to_freq(norm_x);
                *self.resonance = norm_y;
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_zdf_curve(
                &painter,
                rect,
                *self.cutoff_hz,
                *self.resonance,
                self.live_cutoff_hz,
            );
        }

        response
    }
}

impl<'a> egui::Widget for ZdfCurveWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

pub fn render_zdf_curve(
    painter: &Painter,
    rect: Rect,
    cutoff_hz: f32,
    resonance: f32,
    live_cutoff_hz: Option<f32>,
) {
    // 1. Background and border
    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    // 2. Frequency grid lines: 100 Hz, 1 kHz, 10 kHz
    for &f in &[100.0, 1000.0, 10000.0] {
        let norm_x = ZdfCurveWidget::freq_to_norm_x(f);
        let x = rect.left() + norm_x * rect.width();
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
        );
        let label = if f >= 1000.0 {
            format!("{}k", (f / 1000.0) as u32)
        } else {
            format!("{}", f as u32)
        };
        painter.text(
            pos2(x + 2.0, rect.bottom() - 12.0),
            egui::Align2::LEFT_BOTTOM,
            label,
            FontId::new(9.0, FontFamily::Monospace),
            COLOR_TEXT_MUTED,
        );
    }

    // 0 dB reference line
    let norm_0db = ZdfCurveWidget::db_to_norm_y(0.0);
    let y_0db = rect.bottom() - norm_0db * rect.height();
    painter.line_segment(
        [pos2(rect.left(), y_0db), pos2(rect.right(), y_0db)],
        Stroke::new(1.0_f32, Color32::from_rgb(0x23, 0x2a, 0x3b)),
    );

    // 3. Draw curve polyline (128 points)
    let num_points = 128;
    let mut curve_points = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let norm_x = i as f32 / (num_points - 1) as f32;
        let f = ZdfCurveWidget::norm_x_to_freq(norm_x);
        let db = ZdfCurveWidget::magnitude_db(f, cutoff_hz, resonance);
        let norm_y = ZdfCurveWidget::db_to_norm_y(db);

        let px = rect.left() + norm_x * rect.width();
        let py = rect.bottom() - norm_y * rect.height();
        curve_points.push(pos2(px, py));
    }

    // Filled background under curve
    if curve_points.len() >= 2 {
        let mut filled_mesh = curve_points.clone();
        filled_mesh.push(pos2(rect.right(), rect.bottom()));
        filled_mesh.push(pos2(rect.left(), rect.bottom()));

        // Glow stroke
        painter.add(egui::Shape::line(
            curve_points.clone(),
            Stroke::new(3.0_f32, Color32::from_rgba_premultiplied(0xff, 0xbe, 0x00, 0x40)),
        ));

        // Amber curve line
        painter.add(egui::Shape::line(
            curve_points,
            Stroke::new(2.0_f32, COLOR_AMBER),
        ));
    }

    // 4. Draggable Cutoff & Resonance Puck
    let puck_norm_x = ZdfCurveWidget::freq_to_norm_x(cutoff_hz);
    let puck_norm_y = resonance.clamp(0.0, 1.0);
    let puck_x = rect.left() + puck_norm_x * rect.width();
    let puck_y = rect.bottom() - puck_norm_y * rect.height();

    // Outer glow ring
    painter.circle_stroke(
        pos2(puck_x, puck_y),
        8.0,
        Stroke::new(1.5_f32, Color32::from_rgba_premultiplied(0xff, 0xbe, 0x00, 0x60)),
    );
    // Core amber puck
    painter.circle_filled(pos2(puck_x, puck_y), 5.0, COLOR_AMBER);
    painter.circle_filled(pos2(puck_x, puck_y), 2.0, Color32::WHITE);

    // 5. Live Modulated Cutoff Marker (Cyan ghost line)
    if let Some(live_fc) = live_cutoff_hz {
        let live_norm_x = ZdfCurveWidget::freq_to_norm_x(live_fc);
        let live_x = rect.left() + live_norm_x * rect.width();
        painter.line_segment(
            [pos2(live_x, rect.top()), pos2(live_x, rect.bottom())],
            Stroke::new(1.5_f32, COLOR_CYAN),
        );
        painter.circle_filled(pos2(live_x, rect.top() + 6.0), 3.0, COLOR_CYAN);
    }

    // Readout label: "1250 Hz | Q 0.75"
    let readout = format!("{:.0} Hz | Q {:.2}", cutoff_hz, resonance);
    painter.text(
        pos2(rect.right() - 6.0, rect.top() + 6.0),
        egui::Align2::RIGHT_TOP,
        readout,
        FontId::new(10.0, FontFamily::Monospace),
        COLOR_AMBER,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zdf_curve_frequency_log_mapping_roundtrip() {
        for freq in [20.0, 100.0, 440.0, 1000.0, 5000.0, 20000.0] {
            let norm = ZdfCurveWidget::freq_to_norm_x(freq);
            assert!((0.0..=1.0).contains(&norm));
            let reconstructed = ZdfCurveWidget::norm_x_to_freq(norm);
            assert!(
                (freq - reconstructed).abs() / freq < 0.001,
                "Frequency mapping must roundtrip within 0.1%: {} vs {}",
                freq,
                reconstructed
            );
        }
    }

    #[test]
    fn test_zdf_curve_magnitude_attenuation() {
        // At 1000 Hz cutoff:
        // Passband (100 Hz): close to 0 dB
        let passband_db = ZdfCurveWidget::magnitude_db(100.0, 1000.0, 0.0);
        assert!((-1.0..=1.0).contains(&passband_db));

        // Stopband (10000 Hz): should show steep attenuation (> 20 dB)
        let stopband_db = ZdfCurveWidget::magnitude_db(10000.0, 1000.0, 0.0);
        assert!(stopband_db < -20.0, "Stopband must show roll-off attenuation");

        // Resonance peak check
        let peak_db = ZdfCurveWidget::magnitude_db(1000.0, 1000.0, 0.9);
        assert!(peak_db > passband_db, "Resonance must boost cutoff frequency");
    }
}
