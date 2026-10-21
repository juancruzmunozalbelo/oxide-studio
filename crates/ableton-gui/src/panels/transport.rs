//! Top control & scale bar (height ~48px).
//!
//! Live 12 Complete Workstation Top Bar featuring:
//! - Global Scale & Root Key selector with Scale-Lock toggle
//! - Tuning system selector (12-TET, Just Intonation, 24-EDO, Scala loader)
//! - Transport controls: Tap Tempo, Draggable BPM, Time Signature, Metronome
//! - Play, Stop, Record (Session/Arrangement record toggle)
//! - CPU load gauge in tabular monospace
//! - Real-time mini oscilloscope and 64-band FFT spectrum
//! - One-click WAV export

use egui::{
    vec2, Button, Color32, ComboBox, DragValue, FontFamily, FontId, Layout, RichText, Sense,
    Stroke, Ui,
};
use ableton_core::{AudioTelemetry, GlobalScale, ScaleMode, TuningSystem, TuningType, ROOT_NAMES};
use crate::theme::{
    format_bpm, format_musical_beat, COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE,
    COLOR_BORDER, COLOR_CYAN, COLOR_GREEN, COLOR_RUBY, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};
use crate::visualizers::{render_oscilloscope, render_spectrum};

pub struct TopControlBarAction {
    pub toggle_play: bool,
    pub stop: bool,
    pub toggle_record: bool,
    pub tap_tempo: bool,
    pub export_wav: bool,
    pub bpm_changed: Option<f64>,
    pub scale_changed: bool,
    pub tuning_changed: bool,
}

pub fn render_top_control_bar(
    ui: &mut Ui,
    is_playing: bool,
    is_recording: bool,
    bpm: &mut f64,
    time_sig: &mut (u8, u8),
    metronome: &mut bool,
    arr_record: &mut bool,
    scale: &mut GlobalScale,
    tuning: &mut TuningSystem,
    current_beat: f64,
    telemetry: &AudioTelemetry,
) -> TopControlBarAction {
    let mut action = TopControlBarAction {
        toggle_play: false,
        stop: false,
        toggle_record: false,
        tap_tempo: false,
        export_wav: false,
        bpm_changed: None,
        scale_changed: false,
        tuning_changed: false,
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

        // 1. Tap Tempo & BPM
        let tap_btn = Button::new(RichText::new("TAP").size(10.5).color(COLOR_AMBER).strong())
            .min_size(vec2(32.0, 26.0))
            .fill(COLOR_BG_PANEL_ALT)
            .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
        if ui.add(tap_btn).clicked() {
            action.tap_tempo = true;
        }

        let prev_bpm = *bpm;
        let drag_bpm = ui.add(
            DragValue::new(bpm)
                .speed(0.2)
                .range(20.0..=999.0)
                .custom_formatter(|val, _| format_bpm(val)),
        );
        if drag_bpm.changed() && *bpm != prev_bpm {
            action.bpm_changed = Some(*bpm);
        }

        // Time Signature (num / den)
        ui.label(RichText::new("/").size(11.0).color(COLOR_TEXT_MUTED));
        let mut num = time_sig.0;
        let mut den = time_sig.1;
        ui.add(DragValue::new(&mut num).range(1..=32));
        ui.label(RichText::new("/").size(11.0).color(COLOR_TEXT_MUTED));
        ui.add(DragValue::new(&mut den).range(1..=32));
        time_sig.0 = num;
        time_sig.1 = den;

        // Metronome Toggle
        let metro_color = if *metronome { COLOR_AMBER } else { COLOR_TEXT_MUTED };
        let metro_btn = Button::new(RichText::new("⏱").size(12.0).color(metro_color))
            .min_size(vec2(24.0, 26.0))
            .fill(if *metronome { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(metro_btn).on_hover_text("Metronome Click").clicked() {
            *metronome = !*metronome;
        }

        ui.separator();

        // 2. Transport Buttons: Play, Stop, Record, Session/Arrangement Record
        let play_color = if is_playing { COLOR_GREEN } else { COLOR_TEXT_PRIMARY };
        let play_btn = Button::new(RichText::new("▶").size(13.0).color(play_color))
            .min_size(vec2(30.0, 26.0));
        if ui.add(play_btn).clicked() {
            action.toggle_play = true;
        }

        let stop_btn = Button::new(RichText::new("■").size(13.0).color(Color32::WHITE))
            .min_size(vec2(30.0, 26.0));
        if ui.add(stop_btn).clicked() {
            action.stop = true;
        }

        let rec_color = if is_recording { COLOR_RUBY } else { COLOR_TEXT_MUTED };
        let rec_btn = Button::new(RichText::new("●").size(13.0).color(rec_color))
            .min_size(vec2(30.0, 26.0));
        if ui.add(rec_btn).clicked() {
            action.toggle_record = true;
        }

        // Arrangement vs Session Record mode toggle
        let rec_mode_text = if *arr_record { "ARR REC" } else { "SES REC" };
        let rec_mode_btn = Button::new(RichText::new(rec_mode_text).size(9.5).color(COLOR_TEXT_MUTED))
            .min_size(vec2(52.0, 24.0));
        if ui.add(rec_mode_btn).clicked() {
            *arr_record = !*arr_record;
        }

        ui.separator();

        // 3. Global Scale & Root Key Bar (Live 12 Flagship Feature)
        ui.label(RichText::new("Scale:").size(11.0).color(COLOR_TEXT_MUTED));

        // Root Note Selector (C, C#, D, D# ... B)
        let cur_root_name = ROOT_NAMES[(scale.root % 12) as usize];
        ComboBox::from_id_salt("top_bar_root_selector")
            .selected_text(RichText::new(cur_root_name).size(11.0).color(COLOR_AMBER).strong())
            .width(42.0)
            .show_ui(ui, |ui| {
                for (idx, &name) in ROOT_NAMES.iter().enumerate() {
                    let is_sel = scale.root == idx as u8;
                    if ui.selectable_label(is_sel, name).clicked() {
                        scale.root = idx as u8;
                        action.scale_changed = true;
                    }
                }
            });

        // Scale Mode Selector (14 modes)
        ComboBox::from_id_salt("top_bar_scale_mode_selector")
            .selected_text(RichText::new(scale.mode.name()).size(11.0).color(COLOR_AMBER))
            .width(110.0)
            .show_ui(ui, |ui| {
                for &m in &ScaleMode::ALL {
                    let is_sel = scale.mode == m;
                    if ui.selectable_label(is_sel, m.name()).clicked() {
                        scale.mode = m;
                        action.scale_changed = true;
                    }
                }
            });

        // Scale-Lock Toggle Button (Lock icon)
        let lock_color = if scale.scale_lock { COLOR_AMBER } else { COLOR_TEXT_MUTED };
        let lock_text = if scale.scale_lock { "🔒 Fold" } else { "🔓 Chromatic" };
        let lock_btn = Button::new(RichText::new(lock_text).size(10.0).color(lock_color))
            .fill(if scale.scale_lock { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE })
            .stroke(Stroke::new(1.0_f32, if scale.scale_lock { COLOR_AMBER } else { COLOR_BORDER }));

        if ui.add(lock_btn).on_hover_text("Scale Lock: Fold piano roll to active scale notes").clicked() {
            scale.scale_lock = !scale.scale_lock;
            action.scale_changed = true;
        }

        // Tuning System Selector
        ComboBox::from_id_salt("top_bar_tuning_system_selector")
            .selected_text(RichText::new(tuning.tuning_type.name()).size(10.0).color(COLOR_CYAN))
            .width(115.0)
            .show_ui(ui, |ui| {
                for &t in &TuningType::ALL {
                    let is_sel = tuning.tuning_type == t;
                    if ui.selectable_label(is_sel, t.name()).clicked() {
                        tuning.tuning_type = t;
                        action.tuning_changed = true;
                    }
                }
            });

        ui.separator();

        // 4. Transport position (Bar.Beat.16th) in Monospace
        let beat_str = format_musical_beat(current_beat);
        ui.label(
            RichText::new(beat_str)
                .font(FontId::new(12.5, FontFamily::Monospace))
                .color(COLOR_AMBER),
        );

        // 5. CPU Load
        let cpu_str = format!("CPU {:>4.1}%", telemetry.cpu_load);
        ui.label(
            RichText::new(cpu_str)
                .font(FontId::new(10.5, FontFamily::Monospace))
                .color(if telemetry.cpu_load > 75.0 { COLOR_RUBY } else { COLOR_TEXT_MUTED }),
        );

        // 6. Mini Oscilloscope & Spectrum
        let osc_size = vec2(70.0, 24.0);
        let (osc_rect, _) = ui.allocate_exact_size(osc_size, Sense::hover());
        if ui.is_rect_visible(osc_rect) {
            let painter = ui.painter_at(osc_rect);
            render_oscilloscope(&painter, osc_rect, &telemetry.oscilloscope_slice);
        }

        let spec_size = vec2(70.0, 24.0);
        let (spec_rect, _) = ui.allocate_exact_size(spec_size, Sense::hover());
        if ui.is_rect_visible(spec_rect) {
            let painter = ui.painter_at(spec_rect);
            render_spectrum(&painter, spec_rect, &telemetry.spectrum_bins);
        }

        // 7. Export WAV button on right
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            let bounce_btn = Button::new(
                RichText::new("Export WAV")
                    .size(10.5)
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

// Keep backwards-compatible legacy wrapper for any existing callers
pub fn render_transport_bar(
    ui: &mut Ui,
    is_playing: bool,
    is_recording: bool,
    bpm: &mut f64,
    _swing: &mut f32,
    current_beat: f64,
    telemetry: &AudioTelemetry,
) -> crate::panels::transport_legacy::LegacyTransportBarAction {
    let mut scale = GlobalScale::default();
    let mut tuning = TuningSystem::default();
    let mut time_sig = (4, 4);
    let mut metronome = false;
    let mut arr_rec = false;

    let action = render_top_control_bar(
        ui,
        is_playing,
        is_recording,
        bpm,
        &mut time_sig,
        &mut metronome,
        &mut arr_rec,
        &mut scale,
        &mut tuning,
        current_beat,
        telemetry,
    );

    crate::panels::transport_legacy::LegacyTransportBarAction {
        toggle_play: action.toggle_play,
        stop: action.stop,
        toggle_record: action.toggle_record,
        export_wav: action.export_wav,
        bpm_changed: action.bpm_changed,
        swing_changed: None,
    }
}
