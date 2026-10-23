//! EQ Eight visualizer: 8-band interactive draggable parametric nodes over real-time FFT spectrum background.

use egui::{
    pos2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqFilterType {
    LowCut,
    LowShelf,
    Bell,
    Notch,
    HighShelf,
    HighCut,
}

#[derive(Debug, Clone)]
pub struct EqBand {
    pub enabled: bool,
    pub filter_type: EqFilterType,
    pub freq_hz: f32, // 20.0 .. 20000.0
    pub gain_db: f32, // -15.0 .. +15.0
    pub q: f32,       // 0.1 .. 18.0
}

#[derive(Debug, Clone)]
pub struct EqEightVisualizerState {
    pub bands: [EqBand; 8],
    pub selected_band: usize,
}

impl Default for EqEightVisualizerState {
    fn default() -> Self {
        Self {
            bands: [
                EqBand { enabled: true, filter_type: EqFilterType::LowCut, freq_hz: 35.0, gain_db: 0.0, q: 0.71 },
                EqBand { enabled: true, filter_type: EqFilterType::Bell, freq_hz: 120.0, gain_db: 2.5, q: 1.2 },
                EqBand { enabled: true, filter_type: EqFilterType::Bell, freq_hz: 350.0, gain_db: -1.5, q: 1.5 },
                EqBand { enabled: true, filter_type: EqFilterType::Bell, freq_hz: 850.0, gain_db: 0.0, q: 1.0 },
                EqBand { enabled: true, filter_type: EqFilterType::Bell, freq_hz: 2200.0, gain_db: 1.8, q: 1.4 },
                EqBand { enabled: true, filter_type: EqFilterType::Bell, freq_hz: 5000.0, gain_db: -2.0, q: 1.8 },
                EqBand { enabled: true, filter_type: EqFilterType::HighShelf, freq_hz: 8500.0, gain_db: 3.0, q: 0.71 },
                EqBand { enabled: true, filter_type: EqFilterType::HighCut, freq_hz: 18000.0, gain_db: 0.0, q: 0.71 },
            ],
            selected_band: 1,
        }
    }
}

pub struct EqEightVisualizerWidget<'a> {
    state: &'a mut EqEightVisualizerState,
    spectrum_bins: &'a [f32; 64],
    size: egui::Vec2,
}

impl<'a> EqEightVisualizerWidget<'a> {
    pub fn new(state: &'a mut EqEightVisualizerState, spectrum_bins: &'a [f32; 64], size: egui::Vec2) -> Self {
        Self { state, spectrum_bins, size }
    }
}

impl<'a> Widget for EqEightVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());

        let log_min = (20.0_f32).ln();
        let log_max = (20000.0_f32).ln();

        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // If dragging, update the selected band's freq and gain
                let sel = self.state.selected_band.min(self.state.bands.len().saturating_sub(1));
                let norm_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let new_freq = (log_min + norm_x * (log_max - log_min)).exp();
                let norm_y = ((pos.y - rect.center().y) / (rect.height() * 0.45)).clamp(-1.0, 1.0);
                let new_gain = -norm_y * 15.0;

                self.state.bands[sel].freq_hz = new_freq.clamp(20.0, 20000.0);
                if self.state.bands[sel].filter_type != EqFilterType::LowCut && self.state.bands[sel].filter_type != EqFilterType::HighCut {
                    self.state.bands[sel].gain_db = new_gain.clamp(-15.0, 15.0);
                }
                response.mark_changed();
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_eq8_canvas(&painter, rect, self.state, self.spectrum_bins);
        }

        response
    }
}

pub fn render_eq8_canvas(painter: &Painter, rect: Rect, state: &EqEightVisualizerState, spectrum_bins: &[f32; 64]) {
    if rect.width() < 12.0 || rect.height() < 12.0 {
        return;
    }

    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    let log_min = (20.0_f32).ln();
    let log_max = (20000.0_f32).ln();

    // 1. Grid Lines: 100Hz, 1kHz, 10kHz & 0dB line
    let center_y = rect.center().y;
    painter.line_segment([pos2(rect.left(), center_y), pos2(rect.right(), center_y)], Stroke::new(1.0_f32, Color32::from_rgb(0x22, 0x28, 0x38)));

    for f in [100.0_f32, 1000.0_f32, 10000.0_f32] {
        let norm_f = (f.ln() - log_min) / (log_max - log_min);
        let x = rect.left() + norm_f * rect.width();
        painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1f, 0x2b)));
    }

    // 2. Real-Time 64-Band FFT Spectrum Analyzer Background Bars
    let bar_count = spectrum_bins.len();
    let bar_w = rect.width() / bar_count as f32;
    for (i, &mag) in spectrum_bins.iter().enumerate() {
        let x = rect.left() + i as f32 * bar_w;
        let h = (mag * (rect.height() * 0.75)).min(rect.height() - 4.0);
        let bar_rect = Rect::from_min_max(
            pos2(x + 1.0, rect.bottom() - 2.0 - h),
            pos2(x + bar_w - 1.0, rect.bottom() - 2.0),
        );
        painter.rect_filled(bar_rect, 1.0, Color32::from_rgba_unmultiplied(0, 210, 255, 35));
    }

    // 3. Composite Parametric EQ Response Curve
    let pts_count = 64;
    let mut curve_pts = Vec::with_capacity(pts_count);

    for i in 0..pts_count {
        let t = i as f32 / (pts_count - 1) as f32;
        let freq = (log_min + t * (log_max - log_min)).exp();

        let mut total_db = 0.0f32;
        for band in &state.bands {
            if !band.enabled { continue; }
            let f0 = band.freq_hz;
            let ratio = (freq / f0).max(1e-4);
            let log_ratio = ratio.ln();

            match band.filter_type {
                EqFilterType::Bell => {
                    let band_width = 1.0 / band.q;
                    let dist = log_ratio / band_width;
                    let factor = (-0.5 * dist * dist).exp();
                    total_db += band.gain_db * factor;
                }
                EqFilterType::LowShelf => {
                    if freq <= f0 {
                        total_db += band.gain_db;
                    } else {
                        let roll = (-(freq - f0) / (f0 * 0.5)).exp();
                        total_db += band.gain_db * roll;
                    }
                }
                EqFilterType::HighShelf => {
                    if freq >= f0 {
                        total_db += band.gain_db;
                    } else {
                        let roll = (-(f0 - freq) / (f0 * 0.5)).exp();
                        total_db += band.gain_db * roll;
                    }
                }
                EqFilterType::LowCut => {
                    if freq < f0 {
                        let oct = (f0 / freq).log2();
                        total_db -= oct * 12.0;
                    }
                }
                EqFilterType::HighCut => {
                    if freq > f0 {
                        let oct = (freq / f0).log2();
                        total_db -= oct * 12.0;
                    }
                }
                EqFilterType::Notch => {
                    let band_width = 0.5 / band.q;
                    let dist = log_ratio / band_width;
                    let factor = (-0.5 * dist * dist).exp();
                    total_db -= 24.0 * factor;
                }
            }
        }

        let clamped_db = total_db.clamp(-18.0, 18.0);
        let px = rect.left() + t * rect.width();
        let py = center_y - (clamped_db / 18.0) * (rect.height() * 0.42);
        curve_pts.push(pos2(px, py));
    }

    if curve_pts.len() >= 2 {
        painter.add(egui::Shape::line(curve_pts, Stroke::new(2.0_f32, COLOR_AMBER)));
    }

    // 4. Interactive 8 Draggable Nodes
    for (idx, band) in state.bands.iter().enumerate() {
        if !band.enabled { continue; }
        let norm_f = (band.freq_hz.max(20.0).ln() - log_min) / (log_max - log_min);
        let px = rect.left() + norm_f * rect.width();
        let py = center_y - (band.gain_db / 18.0) * (rect.height() * 0.42);
        let min_x = (rect.left() + 6.0).min(rect.right());
        let max_x = (rect.right() - 6.0).max(min_x);
        let min_y = (rect.top() + 6.0).min(rect.bottom());
        let max_y = (rect.bottom() - 6.0).max(min_y);
        let node_pos = pos2(px.clamp(min_x, max_x), py.clamp(min_y, max_y));

        let is_selected = idx == state.selected_band;
        painter.circle_filled(node_pos, if is_selected { 5.5 } else { 4.0 }, if is_selected { COLOR_AMBER } else { COLOR_CYAN });
        painter.circle_stroke(node_pos, if is_selected { 6.5 } else { 4.5 }, Stroke::new(1.0_f32, Color32::WHITE));

        // Node number label
        painter.text(
            node_pos,
            egui::Align2::CENTER_CENTER,
            format!("{}", idx + 1),
            egui::FontId::monospace(8.0),
            Color32::BLACK,
        );
    }

    // Header readout for active band
    let sel_band = state.bands.get(state.selected_band).unwrap_or(&state.bands[0]);
    let sel_display_idx = if state.selected_band < state.bands.len() { state.selected_band + 1 } else { 1 };
    painter.text(
        pos2(rect.left() + 8.0, rect.top() + 6.0),
        egui::Align2::LEFT_TOP,
        format!("EQ Eight Band {}: {:.0} Hz, {:+.1} dB, Q: {:.2}",
            sel_display_idx, sel_band.freq_hz, sel_band.gain_db, sel_band.q),
        egui::FontId::monospace(10.0),
        COLOR_AMBER,
    );
}
