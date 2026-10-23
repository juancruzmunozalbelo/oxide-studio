//! Top control & scale bar (height ~48px).
//!
//! Live 12 Complete Workstation Top Bar featuring:
//! - Brand Badge: ARUSTON Studio Header
//! - Transport controls: Play, Stop, Record, Draggable BPM, Tap Tempo, Time Sig, Metronome, Key Mode
//! - Global Scale & Root Key selector with Scale-Lock toggle
//! - Segmented View Switcher: Session vs Arrangement
//! - Pro Studio controls drawer (Swing, Microtonal Tuning, Record Mode)
//! - Telemetry: Real-time mini oscilloscope (64x24), 64-band FFT spectrum (64x24), CPU load gauge
//! - One-click WAV export

use egui::{
    vec2, Button, Color32, ComboBox, DragValue, FontFamily, FontId, Layout, RichText, Sense,
    Stroke, Ui,
};
use oxide_core::{AudioTelemetry, GlobalScale, ScaleMode, TuningSystem, TuningType, ROOT_NAMES};
pub use crate::panels::ai_chat::StudioLocale;
use crate::theme::{
    format_bpm, format_musical_beat, COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE,
    COLOR_BORDER, COLOR_CYAN, COLOR_GREEN, COLOR_RUBY, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};
use crate::visualizers::{render_oscilloscope, render_spectrum};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TopControlBarAction {
    pub toggle_play: bool,
    pub stop: bool,
    pub toggle_record: bool,
    pub tap_tempo: bool,
    pub export_wav: bool,
    pub export_als: bool,
    pub import_als: bool,
    pub split_stems: bool,
    pub bpm_changed: Option<f64>,
    pub scale_changed: bool,
    pub tuning_changed: bool,
    pub view_session: bool,
    pub view_arrangement: bool,
    pub toggle_copilot: bool,
    pub locale_changed: Option<StudioLocale>,
}

#[allow(clippy::too_many_arguments)]
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
    copilot_open: bool,
) -> TopControlBarAction {
    let mut action = TopControlBarAction {
        toggle_play: false,
        stop: false,
        toggle_record: false,
        tap_tempo: false,
        export_wav: false,
        export_als: false,
        import_als: false,
        split_stems: false,
        bpm_changed: None,
        scale_changed: false,
        tuning_changed: false,
        view_session: false,
        view_arrangement: false,
        toggle_copilot: false,
        locale_changed: None,
    };

    let avail_w = ui.available_width();
    let is_compact = avail_w < 1360.0;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(if is_compact { 4.0 } else { 5.0 }, 0.0);

        // 1. Brand Badge: "⚡ OXIDE STUDIO" (or "⚡ OXIDE" when compact) in ACCENT_AMBER bold (13px)
        let brand_label = if is_compact { "⚡ OXIDE" } else { "⚡ OXIDE STUDIO" };
        ui.label(
            RichText::new(brand_label)
                .size(13.0)
                .color(COLOR_AMBER)
                .strong(),
        );

        ui.separator();

        // 2. Transport Group: Prominent "▶ PLAY" / "■ STOP" button, "124.00 BPM" drag value, and "Key: Piano" toggle
        let play_color = if is_playing { COLOR_GREEN } else { COLOR_TEXT_PRIMARY };
        let play_fill = if is_playing { Color32::from_rgb(0x10, 0x36, 0x18) } else { COLOR_BG_PANEL_ALT };
        let play_label = if is_compact { "▶" } else { "▶ PLAY" };
        let play_min_w = if is_compact { 32.0 } else { 54.0 };
        let play_btn = Button::new(RichText::new(play_label).size(11.0).color(play_color).strong())
            .min_size(vec2(play_min_w, 26.0))
            .fill(play_fill)
            .stroke(Stroke::new(1.0_f32, if is_playing { COLOR_GREEN } else { COLOR_BORDER }));
        if ui.add(play_btn).clicked() {
            action.toggle_play = true;
        }

        // Stop Button
        let stop_label = if is_compact { "■" } else { "■ STOP" };
        let stop_min_w = if is_compact { 32.0 } else { 52.0 };
        let stop_btn = Button::new(RichText::new(stop_label).size(11.0).color(COLOR_TEXT_PRIMARY).strong())
            .min_size(vec2(stop_min_w, 26.0))
            .fill(COLOR_BG_PANEL_ALT)
            .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
        if ui.add(stop_btn).clicked() {
            action.stop = true;
        }

        // Record Button
        let rec_color = if is_recording { COLOR_RUBY } else { COLOR_TEXT_MUTED };
        let rec_fill = if is_recording { Color32::from_rgb(0x38, 0x08, 0x10) } else { COLOR_BG_PANEL_ALT };
        let rec_btn = Button::new(RichText::new("REC").size(10.0).color(rec_color).strong())
            .min_size(vec2(32.0, 26.0))
            .fill(rec_fill)
            .stroke(Stroke::new(1.0_f32, if is_recording { COLOR_RUBY } else { COLOR_BORDER }));
        if ui.add(rec_btn).on_hover_text("Toggle Record").clicked() {
            action.toggle_record = true;
        }

        // Tap Tempo
        let tap_btn = Button::new(RichText::new("TAP").size(10.0).color(COLOR_AMBER).strong())
            .min_size(vec2(30.0, 26.0))
            .fill(COLOR_BG_PANEL_ALT)
            .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
        if ui.add(tap_btn).clicked() {
            action.tap_tempo = true;
        }

        // Draggable BPM
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
        let mut num = time_sig.0;
        let mut den = time_sig.1;
        ui.add(DragValue::new(&mut num).range(1..=32));
        ui.label(RichText::new("/").size(11.0).color(COLOR_TEXT_MUTED));
        ui.add(DragValue::new(&mut den).range(1..=32));
        time_sig.0 = num;
        time_sig.1 = den;

        // Metronome Toggle
        let metro_color = if *metronome { COLOR_AMBER } else { COLOR_TEXT_MUTED };
        let metro_btn = Button::new(RichText::new("CLK").size(10.0).color(metro_color).strong())
            .min_size(vec2(30.0, 26.0))
            .fill(if *metronome { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE })
            .stroke(Stroke::new(1.0_f32, if *metronome { COLOR_AMBER } else { COLOR_BORDER }));
        if ui.add(metro_btn).on_hover_text("Metronome Click").clicked() {
            *metronome = !*metronome;
        }

        // Keyboard Mode Toggle: "♪ Piano"
        let key_mode_id = egui::Id::new("aruston_key_piano_mode");
        let mut is_piano_mode = ui.data_mut(|d| *d.get_temp_mut_or(key_mode_id, true));
        let key_label = if is_piano_mode { "♪ Piano" } else { "♪ Iso" };
        let key_btn = Button::new(RichText::new(key_label).size(10.0).color(if is_piano_mode { COLOR_AMBER } else { COLOR_CYAN }))
            .min_size(vec2(52.0, 24.0))
            .fill(COLOR_BG_PANEL_ALT)
            .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
        if ui.add(key_btn).on_hover_text("Toggle Computer Keyboard Input Mode (Piano / Wicki-Hayden Isomorphic)").clicked() {
            is_piano_mode = !is_piano_mode;
            ui.data_mut(|d| *d.get_temp_mut_or(key_mode_id, true) = is_piano_mode);
        }

        ui.separator();

        // 3. Musical Scale Group: Root + Scale selector ("♪ F Dorian") + "★" Scale Lock toggle
        let cur_root_name = ROOT_NAMES[(scale.root % 12) as usize];
        ComboBox::from_id_salt("top_bar_root_selector")
            .selected_text(RichText::new(format!("♪ {}", cur_root_name)).size(10.5).color(COLOR_AMBER).strong())
            .width(38.0)
            .show_ui(ui, |ui| {
                for (idx, &name) in ROOT_NAMES.iter().enumerate() {
                    let is_sel = scale.root == idx as u8;
                    if ui.selectable_label(is_sel, name).clicked() {
                        scale.root = idx as u8;
                        action.scale_changed = true;
                    }
                }
            });

        ComboBox::from_id_salt("top_bar_scale_mode_selector")
            .selected_text(RichText::new(scale.mode.name()).size(10.5).color(COLOR_AMBER))
            .width(66.0)
            .show_ui(ui, |ui| {
                for &m in &ScaleMode::ALL {
                    let is_sel = scale.mode == m;
                    if ui.selectable_label(is_sel, m.name()).clicked() {
                        scale.mode = m;
                        action.scale_changed = true;
                    }
                }
            });

        let lock_color = if scale.scale_lock { COLOR_AMBER } else { COLOR_TEXT_MUTED };
        let lock_text = if is_compact {
            if scale.scale_lock { "★" } else { "☆" }
        } else {
            if scale.scale_lock { "★ Lock" } else { "☆ Lock" }
        };
        let lock_btn = Button::new(RichText::new(lock_text).size(10.0).color(lock_color))
            .fill(if scale.scale_lock { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE })
            .stroke(Stroke::new(1.0_f32, if scale.scale_lock { COLOR_AMBER } else { COLOR_BORDER }));

        if ui.add(lock_btn).on_hover_text("Scale Lock: Fold piano roll and view to active musical scale").clicked() {
            scale.scale_lock = !scale.scale_lock;
            action.scale_changed = true;
        }

        // 4. Pro Controls (Toggle "⚙ Pro", stored in ui.data_mut)
        let pro_mode_id = egui::Id::new("aruston_pro_header_mode");
        let mut pro_mode = ui.data_mut(|d| *d.get_temp_mut_or(pro_mode_id, false));

        let pro_btn = Button::new(
            RichText::new("⚙")
                .size(11.0)
                .color(if pro_mode { COLOR_AMBER } else { COLOR_TEXT_MUTED })
                .strong(),
        )
        .min_size(vec2(24.0, 24.0))
        .fill(if pro_mode { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE })
        .stroke(Stroke::new(1.0_f32, if pro_mode { COLOR_AMBER } else { COLOR_BORDER }));

        if ui.add(pro_btn).on_hover_text("Toggle Pro Studio Controls (Swing, Microtonal Tuning, Record Mode)").clicked() {
            pro_mode = !pro_mode;
            ui.data_mut(|d| *d.get_temp_mut_or(pro_mode_id, false) = pro_mode);
        }

        if pro_mode {
            // Swing control
            let swing_id = egui::Id::new("aruston_transport_swing_pct");
            let mut swing_pct = ui.data_mut(|d| *d.get_temp_mut_or(swing_id, 0.0f32));
            ui.label(RichText::new("Swing:").size(10.0).color(COLOR_TEXT_MUTED));
            let swing_drag = ui.add(
                DragValue::new(&mut swing_pct)
                    .speed(0.5)
                    .range(0.0..=100.0)
                    .suffix("%"),
            );
            if swing_drag.changed() {
                ui.data_mut(|d| *d.get_temp_mut_or(swing_id, 0.0f32) = swing_pct);
            }

            // Tuning System Selector (12-TET, Just Intonation, 24-EDO, Scala loader)
            ComboBox::from_id_salt("top_bar_tuning_system_selector")
                .selected_text(RichText::new(tuning.tuning_type.name()).size(10.0).color(COLOR_CYAN))
                .width(95.0)
                .show_ui(ui, |ui| {
                    for &t in &TuningType::ALL {
                        let is_sel = tuning.tuning_type == t;
                        if ui.selectable_label(is_sel, t.name()).clicked() {
                            tuning.tuning_type = t;
                            action.tuning_changed = true;
                        }
                    }
                });

            // Record destination mode toggle: "• SES REC" / "• ARR REC"
            let rec_mode_text = if *arr_record { "• ARR REC" } else { "• SES REC" };
            let rec_mode_color = if *arr_record { COLOR_CYAN } else { COLOR_AMBER };
            let rec_mode_btn = Button::new(RichText::new(rec_mode_text).size(9.5).color(rec_mode_color))
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
            if ui.add(rec_mode_btn).on_hover_text("Toggle Record Mode: Session Matrix vs Arrangement Timeline").clicked() {
                *arr_record = !*arr_record;
            }
        }

        // 5. Right Telemetry & Export: Live Oscilloscope (46x22), Spectrum (46x22), compact CPU badge, and Export / Copilot buttons
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

            let locale_id = egui::Id::new("oxide_studio_locale");
            let mut locale = ui.data_mut(|d| *d.get_temp_mut_or(locale_id, StudioLocale::En));

            // Prominent ⚡ AI Copilot toggle button
            let copilot_color = if copilot_open { COLOR_CYAN } else { COLOR_TEXT_MUTED };
            let copilot_btn = Button::new(
                RichText::new("⚡ AI Copilot")
                    .size(10.5)
                    .color(copilot_color)
                    .strong(),
            )
            .min_size(vec2(78.0, 25.0))
            .fill(if copilot_open { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE })
            .stroke(Stroke::new(1.0_f32, if copilot_open { COLOR_CYAN } else { COLOR_BORDER }));

            if ui.add(copilot_btn).on_hover_text(if locale == StudioLocale::Es { "Abrir/Cerrar Asistente AI Copilot" } else { "Toggle In-App AI Copilot Studio Assistant (MCP)" }).clicked() {
                action.toggle_copilot = true;
            }

            // Segmented [EN | ES] Studio Locale Toggle (clean 44px total width)
            let is_en = locale == StudioLocale::En;
            let es_btn = Button::new(
                RichText::new("ES")
                    .size(9.0)
                    .color(if !is_en { COLOR_AMBER } else { COLOR_TEXT_MUTED })
                    .strong(),
            )
            .min_size(vec2(22.0, 22.0))
            .fill(if !is_en { COLOR_BG_PANEL_ALT } else { COLOR_BG_PANEL_ALT.gamma_multiply(0.5) })
            .stroke(Stroke::new(1.0_f32, if !is_en { COLOR_AMBER } else { COLOR_BORDER }));
            if ui.add(es_btn).on_hover_text("Cambiar idioma de estudio a Español").clicked() && is_en {
                locale = StudioLocale::Es;
                ui.data_mut(|d| *d.get_temp_mut_or(locale_id, StudioLocale::Es) = StudioLocale::Es);
                action.locale_changed = Some(locale);
            }

            let en_btn = Button::new(
                RichText::new("EN")
                    .size(9.0)
                    .color(if is_en { COLOR_AMBER } else { COLOR_TEXT_MUTED })
                    .strong(),
            )
            .min_size(vec2(22.0, 22.0))
            .fill(if is_en { COLOR_BG_PANEL_ALT } else { COLOR_BG_PANEL_ALT.gamma_multiply(0.5) })
            .stroke(Stroke::new(1.0_f32, if is_en { COLOR_AMBER } else { COLOR_BORDER }));
            if ui.add(en_btn).on_hover_text("Switch studio language to English").clicked() && !is_en {
                locale = StudioLocale::En;
                ui.data_mut(|d| *d.get_temp_mut_or(locale_id, StudioLocale::En) = StudioLocale::En);
                action.locale_changed = Some(locale);
            }

            if is_compact {
                // Responsive overflow: fold export, stems, and ALS buttons into compact "Export" dropdown
                ComboBox::from_id_salt("transport_export_overflow_menu")
                    .selected_text(RichText::new("Export").size(10.0).color(COLOR_AMBER).strong())
                    .width(64.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(false, "WAV (Master Bounce)").clicked() {
                            action.export_wav = true;
                        }
                        if ui.selectable_label(false, "Save .ALS").clicked() {
                            action.export_als = true;
                        }
                        if ui.selectable_label(false, "Open .ALS").clicked() {
                            action.import_als = true;
                        }
                        if ui.selectable_label(false, "Split Stems").clicked() {
                            action.split_stems = true;
                        }
                    });
            } else {
                // One-click WAV export
                let bounce_btn = Button::new(
                    RichText::new("WAV")
                        .size(10.0)
                        .color(COLOR_AMBER)
                        .strong(),
                )
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, COLOR_AMBER));

                if ui.add(bounce_btn).on_hover_text("Export Master Bounce WAV (/tmp/ableton_studio_export.wav)").clicked() {
                    action.export_wav = true;
                }

                // Export .ALS
                let export_als_btn = Button::new(
                    RichText::new("Save .ALS")
                        .size(10.0)
                        .color(COLOR_AMBER)
                        .strong(),
                )
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
                if ui.add(export_als_btn).on_hover_text("Export Ableton Live .als Project (/tmp/oxide_studio_session.als)").clicked() {
                    action.export_als = true;
                }

                // Import .ALS
                let import_als_btn = Button::new(
                    RichText::new("Open .ALS")
                        .size(10.0)
                        .color(COLOR_CYAN)
                        .strong(),
                )
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
                if ui.add(import_als_btn).on_hover_text("Import Ableton Live .als Project (/tmp/oxide_studio_session.als)").clicked() {
                    action.import_als = true;
                }

                // Split Stems
                let stems_btn = Button::new(
                    RichText::new("Stems")
                        .size(10.0)
                        .color(COLOR_AMBER)
                        .strong(),
                )
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
                if ui.add(stems_btn).on_hover_text("Export & Split Remix Stems (/tmp/oxide_stems/)").clicked() {
                    action.split_stems = true;
                }
            }

            // Compact CPU badge
            let cpu_color = if telemetry.cpu_load > 75.0 {
                COLOR_RUBY
            } else if telemetry.cpu_load > 40.0 {
                COLOR_AMBER
            } else {
                COLOR_GREEN
            };
            ui.label(
                RichText::new(format!("CPU {:>4.1}%", telemetry.cpu_load))
                    .font(FontId::new(10.0, FontFamily::Monospace))
                    .color(cpu_color),
            );

            // Mini visualizers only when width >= 1100px
            if !is_compact {
                // Mini Spectrum (44x22)
                let spec_size = vec2(44.0, 22.0);
                let (spec_rect, _) = ui.allocate_exact_size(spec_size, Sense::hover());
                if ui.is_rect_visible(spec_rect) {
                    let painter = ui.painter_at(spec_rect);
                    render_spectrum(&painter, spec_rect, &telemetry.spectrum_bins);
                }

                // Mini Oscilloscope (44x22)
                let osc_size = vec2(44.0, 22.0);
                let (osc_rect, _) = ui.allocate_exact_size(osc_size, Sense::hover());
                if ui.is_rect_visible(osc_rect) {
                    let painter = ui.painter_at(osc_rect);
                    render_oscilloscope(&painter, osc_rect, &telemetry.oscilloscope_slice);
                }
            }

            // Transport position (Bar.Beat.16th) in Monospace
            let beat_str = format_musical_beat(current_beat);
            ui.label(
                RichText::new(beat_str)
                    .font(FontId::new(11.5, FontFamily::Monospace))
                    .color(COLOR_AMBER),
            );
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
    swing: &mut f32,
    current_beat: f64,
    telemetry: &AudioTelemetry,
) -> crate::panels::transport_legacy::LegacyTransportBarAction {
    let mut scale = GlobalScale::default();
    let mut tuning = TuningSystem::default();
    let mut time_sig = (4, 4);
    let mut metronome = false;
    let mut arr_rec = false;

    // Sync swing to data_mut
    let swing_id = egui::Id::new("aruston_transport_swing_pct");
    ui.data_mut(|d| *d.get_temp_mut_or(swing_id, *swing * 100.0) = *swing * 100.0);

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
        false,
    );

    let updated_swing = ui.data_mut(|d| *d.get_temp_mut_or(swing_id, 0.0f32)) / 100.0;
    let swing_changed = if (updated_swing - *swing).abs() > 0.001 {
        *swing = updated_swing;
        Some(updated_swing)
    } else {
        None
    };

    crate::panels::transport_legacy::LegacyTransportBarAction {
        toggle_play: action.toggle_play,
        stop: action.stop,
        toggle_record: action.toggle_record,
        export_wav: action.export_wav,
        bpm_changed: action.bpm_changed,
        swing_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_control_bar_action_defaults() {
        let action = TopControlBarAction {
            toggle_play: false,
            stop: false,
            toggle_record: false,
            tap_tempo: false,
            export_wav: false,
            export_als: false,
            import_als: false,
            split_stems: false,
            bpm_changed: None,
            scale_changed: false,
            tuning_changed: false,
            view_session: false,
            view_arrangement: false,
            toggle_copilot: false,
            locale_changed: None,
        };
        assert!(!action.toggle_play);
        assert!(!action.export_als);
        assert!(!action.import_als);
        assert!(!action.split_stems);
        assert!(!action.view_session);
        assert!(!action.view_arrangement);
        assert!(!action.toggle_copilot);
        assert_eq!(action.locale_changed, None);

        let default_action = TopControlBarAction::default();
        assert_eq!(default_action, action);
    }

    #[test]
    fn test_top_control_bar_bilingual_locale_toggle() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let telemetry = AudioTelemetry::default();
        let mut bpm = 124.0;
        let mut time_sig = (4, 4);
        let mut metronome = false;
        let mut arr_rec = false;
        let mut scale = GlobalScale::default();
        let mut tuning = TuningSystem::default();

        let _ = ctx.run(raw_input, |ctx| {
            egui::TopBottomPanel::top("top_panel_bilingual_test")
                .exact_height(44.0)
                .show(ctx, |ui| {
                    let action = render_top_control_bar(
                        ui,
                        false,
                        false,
                        &mut bpm,
                        &mut time_sig,
                        &mut metronome,
                        &mut arr_rec,
                        &mut scale,
                        &mut tuning,
                        0.0,
                        &telemetry,
                        false,
                    );
                    assert_eq!(action.locale_changed, None);
                });
        });
    }

    #[test]
    fn test_top_control_bar_compact_mode_headless() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let telemetry = AudioTelemetry::default();
        let mut bpm = 124.0;
        let mut time_sig = (4, 4);
        let mut metronome = false;
        let mut arr_rec = false;
        let mut scale = GlobalScale::default();
        let mut tuning = TuningSystem::default();

        // 1. Compact mode at 960px width
        let _ = ctx.run(raw_input.clone(), |ctx| {
            egui::TopBottomPanel::top("top_panel_compact")
                .exact_height(44.0)
                .show(ctx, |ui| {
                    ui.set_width(960.0);
                    let action = render_top_control_bar(
                        ui,
                        false,
                        false,
                        &mut bpm,
                        &mut time_sig,
                        &mut metronome,
                        &mut arr_rec,
                        &mut scale,
                        &mut tuning,
                        0.0,
                        &telemetry,
                        false,
                    );
                    assert!(!action.toggle_play);
                });
        });

        // 2. Full mode at 1440px width
        let _ = ctx.run(raw_input, |ctx| {
            egui::TopBottomPanel::top("top_panel_full")
                .exact_height(44.0)
                .show(ctx, |ui| {
                    ui.set_width(1440.0);
                    let action = render_top_control_bar(
                        ui,
                        true,
                        false,
                        &mut bpm,
                        &mut time_sig,
                        &mut metronome,
                        &mut arr_rec,
                        &mut scale,
                        &mut tuning,
                        4.0,
                        &telemetry,
                        true,
                    );
                    assert!(!action.toggle_play);
                });
        });
    }
}
