//! Top transport & telemetry bar (height ~48px).
//!
//! Features:
//! - Play, Stop, Record buttons
//! - Draggable BPM value & 16th swing ratio
//! - Current bar.beat.16th transport position
//! - CPU load gauge in tabular monospace
//! - Real-time oscilloscope widget
//! - 64-band FFT log spectrum widget
//! - One-click WAV bounce button

use egui::{
    vec2, Button, Color32, DragValue, FontFamily, FontId, Layout, RichText, Sense, Stroke, Ui,
};
use ableton_core::AudioTelemetry;
use crate::theme::{
    format_bpm, format_musical_beat, COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_GREEN, COLOR_RUBY, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};
use crate::visualizers::{render_oscilloscope, render_spectrum};

pub struct TransportBarAction {
    pub toggle_play: bool,
    pub stop: bool,
    pub toggle_record: bool,
    pub export_wav: bool,
    pub bpm_changed: Option<f64>,
    pub swing_changed: Option<f32>,
}

pub fn render_transport_bar(
    ui: &mut Ui,
    is_playing: bool,
    is_recording: bool,
    bpm: &mut f64,
    swing: &mut f32,
    current_beat: f64,
    telemetry: &AudioTelemetry,
) -> TransportBarAction {
    let mut action = TransportBarAction {
        toggle_play: false,
        stop: false,
        toggle_record: false,
        export_wav: false,
        bpm_changed: None,
        swing_changed: None,
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);

        // 1. Transport Buttons: Play, Stop, Record
        let play_color = if is_playing { COLOR_GREEN } else { COLOR_TEXT_PRIMARY };
        let play_btn = Button::new(RichText::new("▶").size(14.0).color(play_color))
            .min_size(vec2(32.0, 28.0));
        if ui.add(play_btn).clicked() {
            action.toggle_play = true;
        }

        let stop_btn = Button::new(RichText::new("■").size(14.0).color(Color32::WHITE))
            .min_size(vec2(32.0, 28.0));
        if ui.add(stop_btn).clicked() {
            action.stop = true;
        }

        let rec_color = if is_recording { COLOR_RUBY } else { COLOR_TEXT_MUTED };
        let rec_btn = Button::new(RichText::new("●").size(14.0).color(rec_color))
            .min_size(vec2(32.0, 28.0));
        if ui.add(rec_btn).clicked() {
            action.toggle_record = true;
        }

        ui.separator();

        // 2. BPM readout and drag value
        ui.label(RichText::new("BPM:").size(11.0).color(COLOR_TEXT_MUTED));
        let prev_bpm = *bpm;
        let drag_bpm = ui.add(
            DragValue::new(bpm)
                .speed(0.2)
                .range(40.0..=240.0)
                .custom_formatter(|val, _| format_bpm(val)),
        );
        if drag_bpm.changed() && *bpm != prev_bpm {
            action.bpm_changed = Some(*bpm);
        }

        // 3. 16th Swing Slider
        ui.label(RichText::new("Swing:").size(11.0).color(COLOR_TEXT_MUTED));
        let prev_swing = *swing;
        let drag_swing = ui.add(
            DragValue::new(swing)
                .speed(0.01)
                .range(0.0..=0.75)
                .custom_formatter(|val, _| format!("{:.0}%", val * 100.0)),
        );
        if drag_swing.changed() && *swing != prev_swing {
            action.swing_changed = Some(*swing);
        }

        // 4. Transport position (Bar.Beat.16th) in Tabular Monospace
        let beat_str = format_musical_beat(current_beat);
        ui.label(
            RichText::new(beat_str)
                .font(FontId::new(13.0, FontFamily::Monospace))
                .color(COLOR_AMBER),
        );

        ui.separator();

        // 5. CPU load readout
        let cpu_str = format!("CPU {:>4.1}%", telemetry.cpu_load);
        ui.label(
            RichText::new(cpu_str)
                .font(FontId::new(11.0, FontFamily::Monospace))
                .color(if telemetry.cpu_load > 75.0 { COLOR_RUBY } else { COLOR_TEXT_MUTED }),
        );

        ui.separator();

        // 6. Embedded Mini Oscilloscope
        let osc_size = vec2(90.0, 28.0);
        let (osc_rect, _) = ui.allocate_exact_size(osc_size, Sense::hover());
        if ui.is_rect_visible(osc_rect) {
            let painter = ui.painter_at(osc_rect);
            render_oscilloscope(&painter, osc_rect, &telemetry.oscilloscope_slice);
        }

        // 7. Embedded Mini Spectrum
        let spec_size = vec2(90.0, 28.0);
        let (spec_rect, _) = ui.allocate_exact_size(spec_size, Sense::hover());
        if ui.is_rect_visible(spec_rect) {
            let painter = ui.painter_at(spec_rect);
            render_spectrum(&painter, spec_rect, &telemetry.spectrum_bins);
        }

        // 8. One-click WAV bounce button
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            let bounce_btn = Button::new(
                RichText::new("Export WAV")
                    .size(11.0)
                    .color(COLOR_AMBER)
                    .strong(),
            )
            .fill(COLOR_BG_PANEL_ALT)
            .stroke(Stroke::new(1.0_f32, COLOR_AMBER));

            if ui.add(bounce_btn).clicked() {
                action.export_wav = true;
            }
        });
    });

    action
}
