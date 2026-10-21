//! Live 12 Complete Session Matrix View.
//!
//! Featuring:
//! - Unlimited tracks + Return buses (`A Reverb`, `B Delay`, + custom) + Master bus
//! - 8+ Scenes with scene tempo and meter headers, scene launch triggers
//! - Quantized clip launching (1 bar, 1/4 note, immediate), playing/queued animation
//! - Modular mixer strips: In/Out routing, Pre/Post Send knobs (Send A, Send B),
//!   Volume fader with peak/RMS meter, Pan, Mute, Solo, DJ Crossfader assign (A / Off / B)
//! - Collapsible mixer sections: I/O, Sends, Returns, Crossfader
//! - Dynamic vertical height calculation ensuring Mixer Strips are NEVER clipped

use egui::{
    pos2, vec2, Button, Color32, ComboBox, FontFamily, FontId, Rect, RichText,
    ScrollArea, Sense, Slider, Stroke, Ui,
};
use ableton_core::AudioTelemetry;
use ableton_timeline::{SessionMatrix, SlotState};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID,
    COLOR_BORDER, COLOR_CYAN, COLOR_GREEN, COLOR_LAVENDER, COLOR_RUBY, COLOR_TEXT_MUTED,
    COLOR_TEXT_PRIMARY,
};
use crate::widgets::StereoMeterWidget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchQuantization {
    Bar1,
    Quarter,
    Eighth,
    Sixteenth,
    None,
}

impl LaunchQuantization {
    pub fn name(&self) -> &'static str {
        match self {
            LaunchQuantization::Bar1 => "1 Bar",
            LaunchQuantization::Quarter => "1/4",
            LaunchQuantization::Eighth => "1/8",
            LaunchQuantization::Sixteenth => "1/16",
            LaunchQuantization::None => "None",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossfaderAssign {
    A,
    Off,
    B,
}

#[derive(Debug, Clone)]
pub struct GuiTrackState {
    pub name: String,
    pub volume_linear: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub sends: Vec<f32>, // Send A, Send B, etc.
    pub input_routing: String,
    pub output_routing: String,
    pub monitor_mode: String, // "In", "Auto", "Off"
    pub crossfader_assign: CrossfaderAssign,
}

impl Default for GuiTrackState {
    fn default() -> Self {
        Self {
            name: "Track".into(),
            volume_linear: 0.9,
            pan: 0.0,
            mute: false,
            solo: false,
            sends: vec![0.35, 0.20],
            input_routing: "Ext. 1/2".into(),
            output_routing: "Master".into(),
            monitor_mode: "Auto".into(),
            crossfader_assign: CrossfaderAssign::Off,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GuiReturnTrack {
    pub name: String,
    pub effect_name: String,
    pub volume_linear: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
}

impl Default for GuiReturnTrack {
    fn default() -> Self {
        Self {
            name: "A Reverb".into(),
            effect_name: "Hybrid Reverb".into(),
            volume_linear: 0.85,
            pan: 0.0,
            mute: false,
            solo: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionMixerSections {
    pub show_io: bool,
    pub show_sends: bool,
    pub show_returns: bool,
    pub show_crossfader: bool,
}

impl Default for SessionMixerSections {
    fn default() -> Self {
        Self {
            show_io: false,
            show_sends: false,
            show_returns: true,
            show_crossfader: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SceneHeaderInfo {
    pub name: String,
    pub bpm: f64,
    pub time_sig: (u8, u8),
}

pub struct SessionMatrixAction {
    pub trigger_clip: Option<(usize, usize)>, // (track_idx, slot_idx)
    pub stop_clip: Option<usize>,             // track_idx
    pub stop_all_clips: bool,
    pub trigger_scene: Option<usize>,         // scene_idx
    pub track_volume_changed: Option<(usize, f32)>,
    pub track_pan_changed: Option<(usize, f32)>,
    pub track_mute_toggled: Option<usize>,
    pub track_solo_toggled: Option<usize>,
    pub send_changed: Option<(usize, usize, f32)>, // (track_idx, send_idx, level)
    pub crossfader_changed: Option<f32>,
    pub track_selected: Option<usize>,
}

fn track_category_color(name: &str) -> Color32 {
    let lower = name.to_lowercase();
    if lower.contains("synth") || lower.contains("poly") || lower.contains("pad") || lower.contains("chord") || lower.contains("key") {
        COLOR_CYAN
    } else if lower.contains("drum") || lower.contains("909") || lower.contains("beat") || lower.contains("kick") || lower.contains("perc") || lower.contains("snare") || lower.contains("hihat") {
        COLOR_AMBER
    } else if lower.contains("bass") || lower.contains("sub") || lower.contains("acid") {
        COLOR_LAVENDER
    } else if lower.contains("lead") || lower.contains("arp") || lower.contains("riff") {
        COLOR_GREEN
    } else {
        COLOR_CYAN
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_session_matrix(
    ui: &mut Ui,
    matrix: &mut SessionMatrix,
    tracks_gui: &mut [GuiTrackState],
    return_tracks: &mut [GuiReturnTrack],
    mixer_sections: &mut SessionMixerSections,
    quantization: &mut LaunchQuantization,
    crossfader_pos: &mut f32,
    master_vol: &mut f32,
    telemetry: &AudioTelemetry,
    current_time: f64,
) -> SessionMatrixAction {
    let mut action = SessionMatrixAction {
        trigger_clip: None,
        stop_clip: None,
        stop_all_clips: false,
        trigger_scene: None,
        track_volume_changed: None,
        track_pan_changed: None,
        track_mute_toggled: None,
        track_solo_toggled: None,
        send_changed: None,
        crossfader_changed: None,
        track_selected: None,
    };

    let track_count = matrix.tracks.len();
    let scene_count = matrix.scenes.len().max(8);

    // Track selection state from persistent ui.data
    let selected_track_id = egui::Id::new("aruston_selected_track");
    let mut selected_track: usize = ui.data(|d| d.get_temp(selected_track_id)).unwrap_or(0);

    // Top Session Toolbar: Mixer Sections Toggles, Launch Quantize, & Helpful Studio Hint
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mixer View:").size(11.0).color(COLOR_TEXT_MUTED));

        let io_btn = Button::new(RichText::new("I/O").size(10.0).color(if mixer_sections.show_io { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
            .fill(if mixer_sections.show_io { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(io_btn).clicked() { mixer_sections.show_io = !mixer_sections.show_io; }

        let sends_btn = Button::new(RichText::new("Sends").size(10.0).color(if mixer_sections.show_sends { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
            .fill(if mixer_sections.show_sends { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(sends_btn).clicked() { mixer_sections.show_sends = !mixer_sections.show_sends; }

        let returns_btn = Button::new(RichText::new("Returns").size(10.0).color(if mixer_sections.show_returns { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
            .fill(if mixer_sections.show_returns { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(returns_btn).clicked() { mixer_sections.show_returns = !mixer_sections.show_returns; }

        let xfade_btn = Button::new(RichText::new("X-Fader").size(10.0).color(if mixer_sections.show_crossfader { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
            .fill(if mixer_sections.show_crossfader { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(xfade_btn).clicked() { mixer_sections.show_crossfader = !mixer_sections.show_crossfader; }

        ui.separator();

        // Launch Quantization Dropdown
        ui.label(RichText::new("Quantize:").size(11.0).color(COLOR_TEXT_MUTED));
        ComboBox::from_id_salt("session_quantization_combo")
            .selected_text(RichText::new(quantization.name()).size(10.5).color(COLOR_CYAN))
            .width(60.0)
            .show_ui(ui, |ui| {
                for q in &[
                    LaunchQuantization::Bar1,
                    LaunchQuantization::Quarter,
                    LaunchQuantization::Eighth,
                    LaunchQuantization::Sixteenth,
                    LaunchQuantization::None,
                ] {
                    ui.selectable_value(quantization, *q, q.name());
                }
            });

        ui.separator();

        ui.label(
            RichText::new("💡 Click any clip slot to launch • Select a track header to inspect its Synth / FX Chain below")
                .size(10.0)
                .color(COLOR_TEXT_MUTED),
        );
    });

    ui.separator();

    // Deterministic Session Viewport Budget so the Mixer Strip is ALWAYS pinned and NEVER clipped
    let avail_h = ui.available_height().max(260.0);
    let split = crate::layout::ViewportBudget::session_split_height(avail_h, mixer_sections);

    let header_height = split.header_height;
    let stop_btn_height = split.stop_btn_height;
    let clips_area_height = split.clips_scroll_height;

    // Synchronized scroll offset for clip slots across all columns
    let shared_scroll_id = egui::Id::new("aruston_clips_scroll_y");
    let current_scroll_y: f32 = ui.data(|d| d.get_temp(shared_scroll_id)).unwrap_or(0.0);
    let mut next_scroll_y = current_scroll_y;

    // Horizontal Scrollable Container for Matrix Tracks + Returns + Master
    ScrollArea::horizontal()
        .id_salt("session_matrix_horizontal_scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                // 1. Regular Tracks
                for track_idx in 0..track_count {
                    let track_name = if track_idx < tracks_gui.len() {
                        tracks_gui[track_idx].name.clone()
                    } else {
                        format!("Track {}", track_idx + 1)
                    };

                    let is_selected = selected_track == track_idx;

                    let col_frame = if is_selected {
                        egui::Frame::none()
                            .fill(Color32::from_rgba_unmultiplied(0, 210, 255, 20))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 210, 255, 100)))
                            .rounding(4.0)
                            .inner_margin(egui::Margin::symmetric(2.0, 2.0))
                    } else {
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(2.0, 2.0))
                    };

                    col_frame.show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(136.0);

                        // Track Header (Clickable to select)
                        let (header_rect, header_resp) = ui.allocate_exact_size(vec2(136.0, header_height), Sense::click());
                        if header_resp.clicked() {
                            selected_track = track_idx;
                            ui.data_mut(|d| d.insert_temp(selected_track_id, track_idx));
                            action.track_selected = Some(track_idx);
                        }

                        if ui.is_rect_visible(header_rect) {
                            let painter = ui.painter_at(header_rect);
                            if is_selected {
                                painter.rect_filled(header_rect, 3.0, Color32::from_rgb(0x13, 0x22, 0x33));
                                painter.rect_stroke(header_rect, 3.0, Stroke::new(1.5_f32, COLOR_CYAN));
                                painter.text(
                                    pos2(header_rect.left() + 6.0, header_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &track_name,
                                    FontId::monospace(10.5),
                                    COLOR_CYAN,
                                );
                                painter.text(
                                    pos2(header_rect.right() - 6.0, header_rect.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    "● SELECTED",
                                    FontId::monospace(8.0),
                                    COLOR_AMBER,
                                );
                            } else {
                                let bg = if header_resp.hovered() {
                                    Color32::from_rgb(0x16, 0x1b, 0x24)
                                } else {
                                    COLOR_BG_PANEL_ALT
                                };
                                let border_col = if header_resp.hovered() {
                                    COLOR_TEXT_MUTED
                                } else {
                                    COLOR_BORDER
                                };
                                painter.rect_filled(header_rect, 2.0, bg);
                                painter.rect_stroke(header_rect, 2.0, Stroke::new(1.0_f32, border_col));
                                painter.text(
                                    header_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &track_name,
                                    FontId::monospace(10.5),
                                    COLOR_AMBER,
                                );
                            }
                        }

                        ui.add_space(2.0);

                        // Upper Session Clip Slots Grid inside vertically bounded ScrollArea
                        ui.push_id(track_idx, |ui| {
                            let scroll_out = ScrollArea::vertical()
                                .id_salt("session_clips_scroll")
                                .max_height(clips_area_height)
                                .vertical_scroll_offset(current_scroll_y)
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);

                                    for slot_idx in 0..scene_count {
                                        let slot_state = if let Some(track) = matrix.tracks.get(track_idx) {
                                            if let Some(slot) = track.slots.get(slot_idx) {
                                                slot.state
                                            } else {
                                                SlotState::Empty
                                            }
                                        } else {
                                            SlotState::Empty
                                        };

                                        let clip_name = if let Some(track) = matrix.tracks.get(track_idx) {
                                            track
                                                .slots
                                                .get(slot_idx)
                                                .and_then(|s| s.clip.as_ref())
                                                .map(|c| c.name.as_str())
                                                .unwrap_or("—")
                                        } else {
                                            "—"
                                        };

                                        let (rect, response) = ui.allocate_exact_size(vec2(136.0, 26.0), Sense::click());

                                        if ui.is_rect_visible(rect) {
                                            let painter = ui.painter_at(rect);
                                            render_clip_slot(
                                                &painter,
                                                rect,
                                                slot_state,
                                                clip_name,
                                                response.hovered(),
                                                current_time,
                                            );
                                        }

                                        if response.clicked() {
                                            match slot_state {
                                                SlotState::Empty => {}
                                                SlotState::Playing { .. } | SlotState::QueuedLaunch(_) => {
                                                    action.stop_clip = Some(track_idx);
                                                }
                                                _ => {
                                                    action.trigger_clip = Some((track_idx, slot_idx));
                                                }
                                            }
                                        }
                                    }
                                });

                            if (scroll_out.state.offset.y - current_scroll_y).abs() > 0.5 {
                                next_scroll_y = scroll_out.state.offset.y;
                            }
                        });

                        ui.add_space(2.0);

                        // Stop Clip Button
                        let stop_trk_btn = Button::new(RichText::new("■ Stop").size(9.5).color(COLOR_TEXT_MUTED))
                            .min_size(vec2(136.0, stop_btn_height))
                            .fill(COLOR_BG_SURFACE);
                        if ui.add(stop_trk_btn).clicked() {
                            action.stop_clip = Some(track_idx);
                        }

                        ui.add_space(2.0);
                        ui.separator();

                        // Fixed Bottom Track Mixer Strip (ALWAYS VISIBLE & UNCLIPPED)
                        if track_idx < tracks_gui.len() {
                            egui::ScrollArea::vertical()
                                .id_salt(format!("mixer_strip_scroll_{}", track_idx))
                                .max_height(split.pinned_mixer_footer_height)
                                .show(ui, |ui| {
                                    let t = &mut tracks_gui[track_idx];

                                    // I/O Routing Section (if enabled)
                                    if mixer_sections.show_io {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("In:").size(8.5).color(COLOR_TEXT_MUTED));
                                            ui.label(RichText::new(&t.input_routing).size(9.0).color(COLOR_CYAN));
                                        });
                                        ui.label(RichText::new(format!("Mon: {}", t.monitor_mode)).size(8.5).color(COLOR_TEXT_MUTED));
                                        ui.separator();
                                    }

                                    // Sends Section (if enabled)
                                    if mixer_sections.show_sends {
                                        for (s_idx, s_val) in t.sends.iter_mut().enumerate() {
                                            let label = match s_idx {
                                                0 => "Snd A",
                                                1 => "Snd B",
                                                _ => "Send",
                                            };
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(label).size(8.5).color(COLOR_TEXT_MUTED));
                                                let prev = *s_val;
                                                if ui.add(Slider::new(s_val, 0.0..=1.0).show_value(false)).changed() && *s_val != prev {
                                                    action.send_changed = Some((track_idx, s_idx, *s_val));
                                                }
                                            });
                                        }
                                        ui.separator();
                                    }

                                    // Pan Readout & Horizontal Slider
                                    let pan_display = if t.pan.abs() < 0.02 {
                                        "C".to_string()
                                    } else if t.pan < 0.0 {
                                        format!("{}L", ((-t.pan) * 50.0).round() as i32)
                                    } else {
                                        format!("{}R", ((t.pan) * 50.0).round() as i32)
                                    };

                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("Pan").size(9.0).color(COLOR_TEXT_MUTED));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(pan_display).size(9.0).color(COLOR_CYAN).monospace());
                                        });
                                    });
                                    let prev_pan = t.pan;
                                    if ui.add(Slider::new(&mut t.pan, -1.0..=1.0).show_value(false)).changed() && t.pan != prev_pan {
                                        action.track_pan_changed = Some((track_idx, t.pan));
                                    }

                                    // Vol dB Readout & Horizontal Slider
                                    let vol_db = if t.volume_linear <= 0.001 {
                                        "-∞ dB".to_string()
                                    } else {
                                        let db = 20.0 * t.volume_linear.log10();
                                        if db.abs() < 0.05 {
                                            "0.0 dB".to_string()
                                        } else {
                                            format!("{:+.1} dB", db)
                                        }
                                    };

                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("Vol").size(9.0).color(COLOR_TEXT_MUTED));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(vol_db).size(9.0).color(COLOR_AMBER).monospace());
                                        });
                                    });
                                    let prev_vol = t.volume_linear;
                                    if ui.add(Slider::new(&mut t.volume_linear, 0.0..=1.2).show_value(false)).changed() && t.volume_linear != prev_vol {
                                        action.track_volume_changed = Some((track_idx, t.volume_linear));
                                    }

                                    // Mute, Solo, and Arm row (Never wraps or clips)
                                    let arm_id = egui::Id::new("aruston_track_arm").with(track_idx);
                                    let mut is_armed: bool = ui.data(|d| d.get_temp(arm_id)).unwrap_or(track_idx == 0);

                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                                        let btn_w = 43.0;

                                        // Mute button: amber when active
                                        let mute_fill = if t.mute { COLOR_AMBER } else { COLOR_BG_PANEL_ALT };
                                        let mute_text = if t.mute { COLOR_BG_VOID } else { COLOR_AMBER };
                                        let m_btn = Button::new(RichText::new("M").size(10.0).strong().color(mute_text))
                                            .min_size(vec2(btn_w, 18.0))
                                            .fill(mute_fill)
                                            .stroke(Stroke::new(1.0_f32, if t.mute { COLOR_AMBER } else { COLOR_BORDER }));
                                        if ui.add(m_btn).clicked() {
                                            t.mute = !t.mute;
                                            action.track_mute_toggled = Some(track_idx);
                                        }

                                        // Solo button: cyan when active
                                        let solo_fill = if t.solo { COLOR_CYAN } else { COLOR_BG_PANEL_ALT };
                                        let solo_text = if t.solo { COLOR_BG_VOID } else { COLOR_CYAN };
                                        let s_btn = Button::new(RichText::new("S").size(10.0).strong().color(solo_text))
                                            .min_size(vec2(btn_w, 18.0))
                                            .fill(solo_fill)
                                            .stroke(Stroke::new(1.0_f32, if t.solo { COLOR_CYAN } else { COLOR_BORDER }));
                                        if ui.add(s_btn).clicked() {
                                            t.solo = !t.solo;
                                            action.track_solo_toggled = Some(track_idx);
                                        }

                                        // Arm button: red when active
                                        let arm_fill = if is_armed { COLOR_RUBY } else { COLOR_BG_PANEL_ALT };
                                        let arm_text = if is_armed { Color32::WHITE } else { COLOR_RUBY };
                                        let arm_btn = Button::new(RichText::new("●").size(10.0).strong().color(arm_text))
                                            .min_size(vec2(btn_w, 18.0))
                                            .fill(arm_fill)
                                            .stroke(Stroke::new(1.0_f32, if is_armed { COLOR_RUBY } else { COLOR_BORDER }));
                                        if ui.add(arm_btn).clicked() {
                                            is_armed = !is_armed;
                                            ui.data_mut(|d| d.insert_temp(arm_id, is_armed));
                                        }
                                    });

                                    // Crossfader Assign (A / Off / B) (if enabled)
                                    if mixer_sections.show_crossfader {
                                        let (label, col) = match t.crossfader_assign {
                                            CrossfaderAssign::A => ("A", COLOR_AMBER),
                                            CrossfaderAssign::Off => ("Off", COLOR_TEXT_MUTED),
                                            CrossfaderAssign::B => ("B", COLOR_CYAN),
                                        };
                                        let xf_btn = Button::new(RichText::new(format!("XF: {}", label)).size(9.0).color(col))
                                            .min_size(vec2(136.0, 16.0));
                                        if ui.add(xf_btn).clicked() {
                                            t.crossfader_assign = match t.crossfader_assign {
                                                CrossfaderAssign::Off => CrossfaderAssign::A,
                                                CrossfaderAssign::A => CrossfaderAssign::B,
                                                CrossfaderAssign::B => CrossfaderAssign::Off,
                                            };
                                        }
                                    }

                                    // Peak & RMS Stereo Meter
                                    let pk_l = if track_idx < 16 { telemetry.track_peaks_l[track_idx] } else { 0.0 };
                                    let pk_r = if track_idx < 16 { telemetry.track_peaks_r[track_idx] } else { 0.0 };
                                    ui.add(StereoMeterWidget::new(pk_l, pk_r, pk_l * 0.7, pk_r * 0.7, vec2(136.0, 12.0)));
                                });
                        }
                        });
                    });

                    ui.separator();
                }

                // 2. Return Tracks (A Reverb, B Delay, etc.)
                if mixer_sections.show_returns {
                    for (r_idx, ret) in return_tracks.iter_mut().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(120.0);

                            // Return Header
                            let (r_header_rect, _) = ui.allocate_exact_size(vec2(120.0, header_height), Sense::hover());
                            if ui.is_rect_visible(r_header_rect) {
                                let painter = ui.painter_at(r_header_rect);
                                painter.rect_filled(r_header_rect, 2.0, Color32::from_rgb(0x1a, 0x22, 0x32));
                                painter.rect_stroke(r_header_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));
                                painter.text(
                                    r_header_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &ret.name,
                                    FontId::monospace(10.0),
                                    COLOR_CYAN,
                                );
                            }

                            ui.add_space(2.0);

                            // Return slots placeholder in synchronized scroll area
                            ui.push_id(100 + r_idx, |ui| {
                                let scroll_out = ScrollArea::vertical()
                                    .id_salt("session_clips_scroll")
                                    .max_height(clips_area_height)
                                    .vertical_scroll_offset(current_scroll_y)
                                    .show(ui, |ui| {
                                        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                                        for _ in 0..scene_count {
                                            let (rect, _) = ui.allocate_exact_size(vec2(120.0, 26.0), Sense::hover());
                                            ui.painter().rect_filled(rect, 2.0, COLOR_BG_VOID);
                                            ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0_f32, Color32::from_rgb(0x16, 0x1b, 0x26)));
                                        }
                                    });
                                if (scroll_out.state.offset.y - current_scroll_y).abs() > 0.5 {
                                    next_scroll_y = scroll_out.state.offset.y;
                                }
                            });

                            ui.add_space(2.0);
                            ui.add_space(stop_btn_height);
                            ui.add_space(2.0);
                            ui.separator();

                            // Return FX & Fader with dB readout
                            ui.label(RichText::new(&ret.effect_name).size(9.0).color(COLOR_CYAN));

                            let r_vol_db = if ret.volume_linear <= 0.001 {
                                "-∞ dB".to_string()
                            } else {
                                let db = 20.0 * ret.volume_linear.log10();
                                if db.abs() < 0.05 {
                                    "0.0 dB".to_string()
                                } else {
                                    format!("{:+.1} dB", db)
                                }
                            };

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Vol").size(9.0).color(COLOR_TEXT_MUTED));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(r_vol_db).size(9.0).color(COLOR_AMBER).monospace());
                                });
                            });
                            ui.add(Slider::new(&mut ret.volume_linear, 0.0..=1.2).show_value(false));

                            // Return Mute/Solo
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                                let mute_fill = if ret.mute { COLOR_AMBER } else { COLOR_BG_PANEL_ALT };
                                let mute_text = if ret.mute { COLOR_BG_VOID } else { COLOR_AMBER };
                                if ui.add(Button::new(RichText::new("M").size(10.0).strong().color(mute_text)).min_size(vec2(58.0, 18.0)).fill(mute_fill)).clicked() {
                                    ret.mute = !ret.mute;
                                }
                                let solo_fill = if ret.solo { COLOR_CYAN } else { COLOR_BG_PANEL_ALT };
                                let solo_text = if ret.solo { COLOR_BG_VOID } else { COLOR_CYAN };
                                if ui.add(Button::new(RichText::new("S").size(10.0).strong().color(solo_text)).min_size(vec2(58.0, 18.0)).fill(solo_fill)).clicked() {
                                    ret.solo = !ret.solo;
                                }
                            });

                            // Return Meter
                            let r_pk = telemetry.master_peak_l * 0.6;
                            ui.add(StereoMeterWidget::new(r_pk, r_pk, r_pk * 0.7, r_pk * 0.7, vec2(120.0, 12.0)));
                        });

                        ui.separator();
                    }
                }

                // 3. Master Bus Column & Scene Launch Triggers
                ui.vertical(|ui| {
                    ui.set_width(136.0);

                    let (m_header_rect, _) = ui.allocate_exact_size(vec2(136.0, header_height), Sense::hover());
                    if ui.is_rect_visible(m_header_rect) {
                        let painter = ui.painter_at(m_header_rect);
                        painter.rect_filled(m_header_rect, 2.0, COLOR_BG_PANEL_ALT);
                        painter.rect_stroke(m_header_rect, 2.0, Stroke::new(1.0_f32, COLOR_AMBER));
                        painter.text(
                            m_header_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Master / Scenes",
                            FontId::monospace(10.5),
                            COLOR_AMBER,
                        );
                    }

                    ui.add_space(2.0);

                    // 8+ Scene Launch Buttons in synchronized scroll area
                    ui.push_id(999, |ui| {
                        let scroll_out = ScrollArea::vertical()
                            .id_salt("session_clips_scroll")
                            .max_height(clips_area_height)
                            .vertical_scroll_offset(current_scroll_y)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                                for scene_idx in 0..scene_count {
                                    let scene_label = if let Some(scene) = matrix.scenes.get(scene_idx) {
                                        if !scene.name.is_empty() {
                                            format!("▶ {}", scene.name)
                                        } else {
                                            format!("▶ Scene {}", scene_idx + 1)
                                        }
                                    } else {
                                        format!("▶ Scene {}", scene_idx + 1)
                                    };
                                    let scene_btn = Button::new(RichText::new(scene_label).size(9.5).color(COLOR_AMBER))
                                        .min_size(vec2(136.0, 26.0))
                                        .fill(COLOR_BG_PANEL_ALT)
                                        .stroke(Stroke::new(1.0_f32, COLOR_BORDER));

                                    if ui.add(scene_btn).clicked() {
                                        action.trigger_scene = Some(scene_idx);
                                    }
                                }
                            });
                        if (scroll_out.state.offset.y - current_scroll_y).abs() > 0.5 {
                            next_scroll_y = scroll_out.state.offset.y;
                        }
                    });

                    ui.add_space(2.0);

                    // Stop All Clips Button
                    let stop_all_btn = Button::new(RichText::new("■ Stop All").size(10.0).color(COLOR_RUBY))
                        .min_size(vec2(136.0, stop_btn_height))
                        .fill(COLOR_BG_SURFACE);
                    if ui.add(stop_all_btn).clicked() {
                        action.stop_all_clips = true;
                    }

                    ui.add_space(2.0);
                    ui.separator();

                    // Master Fader with dB Readout
                    let m_vol_db = if *master_vol <= 0.001 {
                        "-∞ dB".to_string()
                    } else {
                        let db = 20.0 * master_vol.log10();
                        if db.abs() < 0.05 {
                            "0.0 dB".to_string()
                        } else {
                            format!("{:+.1} dB", db)
                        }
                    };

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Master Out").size(10.0).color(COLOR_AMBER).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(m_vol_db).size(9.0).color(COLOR_AMBER).monospace());
                        });
                    });
                    ui.add(Slider::new(master_vol, 0.0..=1.2).show_value(false));

                    // DJ Crossfader (A <-> B)
                    if mixer_sections.show_crossfader {
                        ui.label(RichText::new("DJ Crossfader").size(9.0).color(COLOR_TEXT_MUTED));
                        let prev_xf = *crossfader_pos;
                        if ui.add(Slider::new(crossfader_pos, -1.0..=1.0).show_value(false)).changed() && *crossfader_pos != prev_xf {
                            action.crossfader_changed = Some(*crossfader_pos);
                        }
                    }

                    // Master Peak / RMS Ballistics Meter
                    ui.add(StereoMeterWidget::new(
                        telemetry.master_peak_l,
                        telemetry.master_peak_r,
                        telemetry.master_rms_l,
                        telemetry.master_rms_r,
                        vec2(136.0, 14.0),
                    ));
                });
            });
        });

    // Save synchronized scroll position
    if (next_scroll_y - current_scroll_y).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(shared_scroll_id, next_scroll_y));
    }

    action
}

pub fn render_clip_slot(
    painter: &egui::Painter,
    rect: Rect,
    state: SlotState,
    clip_name: &str,
    hovered: bool,
    current_time: f64,
) {
    let base_bg = if hovered {
        Color32::from_rgb(0x1a, 0x22, 0x30)
    } else {
        COLOR_BG_SURFACE
    };
    painter.rect_filled(rect, 3.0, base_bg);

    match state {
        SlotState::Empty => {
            if hovered {
                painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, Color32::from_rgb(0x28, 0x38, 0x4f)));
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "□ + Clip",
                    FontId::monospace(9.0),
                    COLOR_TEXT_MUTED,
                );
            } else {
                painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)));
                painter.circle_filled(rect.center(), 2.0, Color32::from_rgb(0x28, 0x32, 0x48));
            }
        }
        SlotState::Stopped => {
            let cat_col = track_category_color(clip_name);
            // Subtle colored left accent bar matching track category (Synth, 909 Drums, Bass, Lead)
            let bar_rect = Rect::from_min_size(rect.min, vec2(3.5, rect.height()));
            painter.rect_filled(bar_rect, 1.0, cat_col);

            let stroke_col = if hovered { COLOR_CYAN } else { COLOR_BORDER };
            painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, stroke_col));

            let play_col = if hovered { COLOR_GREEN } else { COLOR_TEXT_MUTED };
            painter.text(
                pos2(rect.left() + 9.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "▶",
                FontId::new(9.5, FontFamily::Proportional),
                play_col,
            );
            painter.text(
                pos2(rect.left() + 22.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_TEXT_PRIMARY,
            );
        }
        SlotState::QueuedLaunch(_) => {
            // Pulsing ACCENT_AMBER border with "⏳ " prefix
            let pulse = ((current_time * 8.0).sin().abs() as f32).clamp(0.3, 1.0);
            let stroke_color = Color32::from_rgba_premultiplied(
                255,
                (190.0 * pulse) as u8,
                0,
                (255.0 * pulse) as u8,
            );
            painter.rect_stroke(rect, 3.0, Stroke::new(1.8_f32, stroke_color));
            painter.text(
                pos2(rect.left() + 6.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "⏳",
                FontId::new(10.0, FontFamily::Proportional),
                COLOR_AMBER,
            );
            painter.text(
                pos2(rect.left() + 22.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_AMBER,
            );
        }
        SlotState::Playing { phase, .. } => {
            // Glowing ACCENT_GREEN border, animated "▶ " prefix, bold clip title, and progress bar strip
            painter.rect_filled(rect, 3.0, Color32::from_rgb(0x10, 0x28, 0x1d));
            painter.rect_stroke(rect, 3.0, Stroke::new(1.8_f32, COLOR_GREEN));

            // Horizontal progress bar underneath
            let prog_w = (rect.width() * (phase as f32).fract().clamp(0.0, 1.0)).max(3.0);
            let prog_rect = Rect::from_min_size(
                pos2(rect.left(), rect.bottom() - 3.5),
                vec2(prog_w, 3.5),
            );
            painter.rect_filled(prog_rect, 1.0, COLOR_GREEN);

            // Animated play icon
            let pulse = ((current_time * 6.0).sin().abs() as f32).clamp(0.7, 1.0);
            let icon_col = Color32::from_rgba_premultiplied(0, (230.0 * pulse) as u8, (118.0 * pulse) as u8, 255);
            painter.text(
                pos2(rect.left() + 7.0, rect.center().y - 1.0),
                egui::Align2::LEFT_CENTER,
                "▶",
                FontId::new(10.0, FontFamily::Proportional),
                icon_col,
            );
            // Double-draw text for bold emphasis
            painter.text(
                pos2(rect.left() + 22.0, rect.center().y - 1.0),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_GREEN,
            );
            painter.text(
                pos2(rect.left() + 22.5, rect.center().y - 1.0),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_GREEN,
            );
        }
        SlotState::QueuedStop(_) => {
            // Blinking queued stop animation
            let blink = (current_time * 8.0).fract() > 0.5;
            let stroke_color = if blink { COLOR_RUBY } else { COLOR_BORDER };
            painter.rect_stroke(rect, 3.0, Stroke::new(1.8_f32, stroke_color));
            painter.text(
                pos2(rect.left() + 6.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "■",
                FontId::new(10.0, FontFamily::Proportional),
                COLOR_RUBY,
            );
            painter.text(
                pos2(rect.left() + 22.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_RUBY,
            );
        }
    }
}
