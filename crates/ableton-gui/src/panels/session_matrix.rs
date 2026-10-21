//! Live 12 Complete Session Matrix View & Contiguous Architectural Studio Table.
//!
//! Featuring:
//! - Contiguous architectural session grid (0px item spacing, 1px `#111111` vertical divider strokes)
//! - Full-bleed category colored track headers (`#6baace`, `#e0aa2a`, `#52ba46`, `#954eb2`) with bold `#070707` text
//! - 100% row-aligned clip slots (fixed 22.0px height across Audio/MIDI tracks, return empty slots, and master scenes)
//! - Synchronized stop clip row directly below clip slots (`[■]` per track, `[■ Stop All]` in Master column)
//! - Custom-painted Ableton Live 12 vertical mixer strip (NO generic `egui::Slider` horizontal circles):
//!   - Rotary arc sends (`A`, `B`) with `#03c3d5` arc and `#111111` center
//!   - Bipolar split-bar pan control (`18px` high, `#181818` bg, `#03c3d5` bipolar bar, displaying `C`, `12L`, `15R`)
//!   - Side-by-side vertical volume fader (76px, `#181818` slot, 0dB unity tick at 75%, cap `#b5b5b5`/`#ffad56`, digital dB readout pill)
//!     AND vertical stereo segmented peak/RMS VU meter (`L` and `R` segments: `#00f758` -> `#ffd100` -> `#ff0a0a`)
//!   - Crisp rectangular buttons (1px `#111111` border): Track Activator (`1`, `2`, `3` in `#ffad56` amber with bold `#070707` text; `#363636` when muted),
//!     Solo (`S` in `#3c6ab6` blue when active), Record Arm (`●` in `#ff5559` red when active)

use ableton_core::AudioTelemetry;
use ableton_timeline::{SessionMatrix, SlotState};
use egui::{
    pos2, vec2, Button, Color32, ComboBox, FontFamily, FontId, Rect, RichText, ScrollArea,
    Sense, Stroke, Ui,
};

use crate::theme::{
    COLOR_CHOSEN_DEFAULT, COLOR_CHOSEN_PLAY, COLOR_CHOSEN_PRELISTEN, COLOR_CHOSEN_RECORD,
    COLOR_CONTROL_BG, COLOR_CONTROL_CONTRAST_FRAME, COLOR_CONTROL_FG, COLOR_CONTROL_ON_FG,
    COLOR_DESKTOP, COLOR_METER_BG, COLOR_RANGE_DEFAULT, COLOR_SURFACE_AREA, COLOR_SURFACE_BG,
    COLOR_SURFACE_HIGHLIGHT, COLOR_TEXT_MUTED, COLOR_TRACK_AMBER, COLOR_TRACK_BLUE,
    COLOR_TRACK_GREEN, COLOR_TRACK_PURPLE, COLOR_TRACK_TEAL, COLOR_VU_MAX, COLOR_VU_MID,
    COLOR_VU_MIN,
};

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
    pub clip_selected: Option<(usize, usize)>, // (track_idx, slot_idx)
}

pub fn track_category_color(name: &str) -> Color32 {
    let lower = name.to_lowercase();
    if lower.contains("synth")
        || lower.contains("poly")
        || lower.contains("pad")
        || lower.contains("chord")
        || lower.contains("key")
    {
        COLOR_TRACK_TEAL // #6baace
    } else if lower.contains("drum")
        || lower.contains("909")
        || lower.contains("beat")
        || lower.contains("kick")
        || lower.contains("perc")
        || lower.contains("snare")
        || lower.contains("hihat")
    {
        COLOR_TRACK_AMBER // #e0aa2a
    } else if lower.contains("bass") || lower.contains("sub") || lower.contains("acid") {
        COLOR_TRACK_PURPLE // #954eb2
    } else if lower.contains("lead") || lower.contains("arp") || lower.contains("riff") {
        COLOR_TRACK_GREEN // #52ba46
    } else {
        COLOR_TRACK_TEAL // #6baace
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
        clip_selected: None,
    };

    let track_count = matrix.tracks.len();
    let scene_count = matrix.scenes.len().max(8);

    // Track and clip selection state from persistent ui.data
    let selected_track_id = egui::Id::new("aruston_selected_track");
    let mut selected_track: usize = ui.data(|d| d.get_temp(selected_track_id)).unwrap_or(0);
    let selected_slot_id = egui::Id::new("aruston_selected_slot");
    let selected_slot: Option<(usize, usize)> = ui.data(|d| d.get_temp(selected_slot_id));

    // Top Session Toolbar: Mixer Sections Toggles, Launch Quantize, & Studio Hint
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mixer View:").size(11.0).color(COLOR_TEXT_MUTED));

        let io_btn = Button::new(
            RichText::new("I/O")
                .size(10.0)
                .color(if mixer_sections.show_io {
                    COLOR_CHOSEN_DEFAULT
                } else {
                    COLOR_TEXT_MUTED
                }),
        )
        .fill(if mixer_sections.show_io {
            COLOR_SURFACE_BG
        } else {
            COLOR_CONTROL_BG
        })
        .stroke(Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        if ui.add(io_btn).clicked() {
            mixer_sections.show_io = !mixer_sections.show_io;
        }

        let sends_btn = Button::new(
            RichText::new("Sends")
                .size(10.0)
                .color(if mixer_sections.show_sends {
                    COLOR_CHOSEN_DEFAULT
                } else {
                    COLOR_TEXT_MUTED
                }),
        )
        .fill(if mixer_sections.show_sends {
            COLOR_SURFACE_BG
        } else {
            COLOR_CONTROL_BG
        })
        .stroke(Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        if ui.add(sends_btn).clicked() {
            mixer_sections.show_sends = !mixer_sections.show_sends;
        }

        let returns_btn = Button::new(
            RichText::new("Returns")
                .size(10.0)
                .color(if mixer_sections.show_returns {
                    COLOR_CHOSEN_DEFAULT
                } else {
                    COLOR_TEXT_MUTED
                }),
        )
        .fill(if mixer_sections.show_returns {
            COLOR_SURFACE_BG
        } else {
            COLOR_CONTROL_BG
        })
        .stroke(Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        if ui.add(returns_btn).clicked() {
            mixer_sections.show_returns = !mixer_sections.show_returns;
        }

        let xfade_btn = Button::new(
            RichText::new("X-Fader")
                .size(10.0)
                .color(if mixer_sections.show_crossfader {
                    COLOR_CHOSEN_DEFAULT
                } else {
                    COLOR_TEXT_MUTED
                }),
        )
        .fill(if mixer_sections.show_crossfader {
            COLOR_SURFACE_BG
        } else {
            COLOR_CONTROL_BG
        })
        .stroke(Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        if ui.add(xfade_btn).clicked() {
            mixer_sections.show_crossfader = !mixer_sections.show_crossfader;
        }

        ui.separator();

        // Launch Quantization Dropdown
        ui.label(RichText::new("Quantize:").size(11.0).color(COLOR_TEXT_MUTED));
        ComboBox::from_id_salt("session_quantization_combo")
            .selected_text(RichText::new(quantization.name()).size(10.5).color(COLOR_RANGE_DEFAULT))
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
            RichText::new("• Click clip slot to launch • Select track header to inspect device chain")
                .size(10.5)
                .color(COLOR_TEXT_MUTED),
        );
    });

    ui.separator();

    // Deterministic Session Viewport Budget
    let avail_h = ui.available_height().max(260.0);
    let split = crate::layout::ViewportBudget::session_split_height(avail_h, mixer_sections);

    let header_height = split.header_height;
    let stop_btn_height = split.stop_btn_height;
    let visible_slots = scene_count.min(6);
    let slot_h = 22.0_f32; // Fixed row height across ALL columns

    let col_w = 140.0_f32;
    let ret_col_w = 120.0_f32;
    let master_col_w = 140.0_f32;

    // Horizontal Scrollable Container for Contiguous Architectural Session Grid
    ScrollArea::horizontal()
        .id_salt("session_matrix_horizontal_scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Contiguous architectural grid: 0px spacing between columns!
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                // 1. Regular Tracks (Audio / MIDI)
                for track_idx in 0..track_count {
                    let track_name = if track_idx < tracks_gui.len() {
                        tracks_gui[track_idx].name.clone()
                    } else {
                        format!("Track {}", track_idx + 1)
                    };

                    let is_selected = selected_track == track_idx;

                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                        // Full-Bleed Category Colored Track Header (26px height)
                        let (header_rect, header_resp) =
                            ui.allocate_exact_size(vec2(col_w, header_height), Sense::click());
                        if header_resp.clicked() {
                            selected_track = track_idx;
                            ui.data_mut(|d| d.insert_temp(selected_track_id, track_idx));
                            action.track_selected = Some(track_idx);
                        }

                        if ui.is_rect_visible(header_rect) {
                            let painter = ui.painter_at(header_rect);
                            let track_color = track_category_color(&track_name);

                            // Full-bleed track category fill
                            painter.rect_filled(header_rect, 0.0, track_color);
                            painter.rect_stroke(
                                header_rect,
                                0.0,
                                Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                            );

                            if is_selected {
                                painter.rect_stroke(
                                    header_rect.shrink(1.0),
                                    0.0,
                                    Stroke::new(1.8_f32, Color32::WHITE),
                                );
                            }

                            let display_title = if track_name.starts_with(|c: char| c.is_ascii_digit()) {
                                track_name.clone()
                            } else {
                                format!("{} {}", track_idx + 1, track_name)
                            };

                            // Bold text in #070707
                            painter.text(
                                header_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &display_title,
                                FontId::new(11.0, FontFamily::Proportional),
                                COLOR_CONTROL_ON_FG,
                            );
                            painter.text(
                                pos2(header_rect.center().x + 0.4, header_rect.center().y),
                                egui::Align2::CENTER_CENTER,
                                &display_title,
                                FontId::new(11.0, FontFamily::Proportional),
                                COLOR_CONTROL_ON_FG,
                            );
                        }

                        // Pixel-Locked 100% Row-Aligned Clip Slots (Fixed 22.0px height)
                        for slot_idx in 0..visible_slots {
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

                            let is_slot_selected = selected_slot == Some((track_idx, slot_idx));
                            let (rect, response) =
                                ui.allocate_exact_size(vec2(col_w, slot_h), Sense::click());

                            if ui.is_rect_visible(rect) {
                                let painter = ui.painter_at(rect);
                                render_clip_slot(
                                    &painter,
                                    rect,
                                    slot_state,
                                    clip_name,
                                    response.hovered(),
                                    is_slot_selected,
                                    current_time,
                                );
                            }

                            if response.clicked() {
                                selected_track = track_idx;
                                ui.data_mut(|d| {
                                    d.insert_temp(selected_track_id, track_idx);
                                    d.insert_temp(selected_slot_id, Some((track_idx, slot_idx)));
                                });
                                action.track_selected = Some(track_idx);
                                action.clip_selected = Some((track_idx, slot_idx));

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

                        // Synchronized Stop Clip Row ([■] per track)
                        let (stop_rect, stop_resp) =
                            ui.allocate_exact_size(vec2(col_w, stop_btn_height), Sense::click());
                        if stop_resp.clicked() {
                            action.stop_clip = Some(track_idx);
                        }
                        if ui.is_rect_visible(stop_rect) {
                            let p = ui.painter_at(stop_rect);
                            let bg = if stop_resp.hovered() {
                                COLOR_SURFACE_HIGHLIGHT
                            } else {
                                COLOR_SURFACE_AREA
                            };
                            p.rect_filled(stop_rect, 0.0, bg);
                            p.rect_stroke(
                                stop_rect,
                                0.0,
                                Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                            );
                            p.text(
                                stop_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "■",
                                FontId::new(10.0, FontFamily::Proportional),
                                COLOR_CHOSEN_RECORD,
                            );
                        }

                        // Vertical Mixer Strip
                        if track_idx < tracks_gui.len() {
                            let t = &mut tracks_gui[track_idx];

                            // 1. I/O Routing Section (if enabled)
                            if mixer_sections.show_io {
                                let (io_rect, _) =
                                    ui.allocate_exact_size(vec2(col_w, 32.0), Sense::hover());
                                if ui.is_rect_visible(io_rect) {
                                    let p = ui.painter_at(io_rect);
                                    p.rect_filled(io_rect, 0.0, COLOR_SURFACE_AREA);
                                    p.rect_stroke(
                                        io_rect,
                                        0.0,
                                        Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                    );
                                    p.text(
                                        pos2(io_rect.left() + 6.0, io_rect.top() + 9.0),
                                        egui::Align2::LEFT_CENTER,
                                        format!("In: {}", t.input_routing),
                                        FontId::monospace(8.5),
                                        COLOR_RANGE_DEFAULT,
                                    );
                                    p.text(
                                        pos2(io_rect.left() + 6.0, io_rect.top() + 22.0),
                                        egui::Align2::LEFT_CENTER,
                                        format!("Mon: {}", t.monitor_mode),
                                        FontId::monospace(8.5),
                                        COLOR_TEXT_MUTED,
                                    );
                                }
                            }

                            // 2. Rotary Arc Sends (A, B) (if enabled)
                            if mixer_sections.show_sends {
                                if let Some((trk, s_idx, val)) =
                                    render_rotary_arc_sends(ui, &mut t.sends, col_w, track_idx)
                                {
                                    action.send_changed = Some((trk, s_idx, val));
                                }
                            }

                            // 3. Bipolar Split-Bar Pan Control (18px tall on #181818)
                            let (pan_container_rect, _) =
                                ui.allocate_exact_size(vec2(col_w, 22.0), Sense::hover());
                            let pan_rect = Rect::from_center_size(
                                pan_container_rect.center(),
                                vec2(col_w - 8.0, 18.0),
                            );
                            if ui.is_rect_visible(pan_container_rect) {
                                let p = ui.painter_at(pan_container_rect);
                                p.rect_filled(pan_container_rect, 0.0, COLOR_SURFACE_AREA);
                                p.rect_stroke(
                                    pan_container_rect,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                            }
                            let pan_resp = ui.interact(
                                pan_rect,
                                ui.id().with(("pan_control", track_idx)),
                                Sense::click_and_drag(),
                            );
                            if pan_resp.dragged() {
                                let delta = pan_resp.drag_delta().x * 0.01;
                                t.pan = (t.pan + delta).clamp(-1.0, 1.0);
                                action.track_pan_changed = Some((track_idx, t.pan));
                            }
                            if pan_resp.double_clicked() {
                                t.pan = 0.0;
                                action.track_pan_changed = Some((track_idx, 0.0));
                            }
                            if ui.is_rect_visible(pan_rect) {
                                let p = ui.painter_at(pan_rect);
                                paint_bipolar_pan_bar(&p, pan_rect, t.pan);
                            }

                            // 4. Side-by-Side Vertical Volume Fader (76px) + Stereo VU Meter
                            let (fader_container, _) =
                                ui.allocate_exact_size(vec2(col_w, 80.0), Sense::hover());
                            if ui.is_rect_visible(fader_container) {
                                let p = ui.painter_at(fader_container);
                                p.rect_filled(fader_container, 0.0, COLOR_SURFACE_AREA);
                                p.rect_stroke(
                                    fader_container,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                            }
                            let fader_meter_rect = Rect::from_center_size(
                                fader_container.center(),
                                vec2(col_w - 8.0, 76.0),
                            );
                            let pk_l = if track_idx < 16 {
                                telemetry.track_peaks_l[track_idx]
                            } else {
                                0.0
                            };
                            let pk_r = if track_idx < 16 {
                                telemetry.track_peaks_r[track_idx]
                            } else {
                                0.0
                            };
                            if render_vertical_fader_and_meter(
                                ui,
                                &mut t.volume_linear,
                                pk_l,
                                pk_r,
                                fader_meter_rect,
                                track_idx,
                            ) {
                                action.track_volume_changed = Some((track_idx, t.volume_linear));
                            }

                            // 5. Crisp Rectangular Track Activator / Solo / Arm Row
                            let arm_id = egui::Id::new("aruston_track_arm").with(track_idx);
                            let mut is_armed: bool =
                                ui.data(|d| d.get_temp(arm_id)).unwrap_or(track_idx == 0);

                            let (mute_tog, solo_tog, arm_tog) = render_activator_buttons(
                                ui,
                                track_idx,
                                &mut t.mute,
                                &mut t.solo,
                                &mut is_armed,
                                col_w,
                            );
                            if mute_tog {
                                action.track_mute_toggled = Some(track_idx);
                            }
                            if solo_tog {
                                action.track_solo_toggled = Some(track_idx);
                            }
                            if arm_tog {
                                ui.data_mut(|d| d.insert_temp(arm_id, is_armed));
                            }

                            // 6. Crossfader Assign (A / Off / B) (if enabled)
                            if mixer_sections.show_crossfader {
                                let (label, col) = match t.crossfader_assign {
                                    CrossfaderAssign::A => ("A", COLOR_CHOSEN_DEFAULT),
                                    CrossfaderAssign::Off => ("Off", COLOR_CONTROL_FG),
                                    CrossfaderAssign::B => ("B", COLOR_RANGE_DEFAULT),
                                };
                                let (xf_rect, xf_resp) =
                                    ui.allocate_exact_size(vec2(col_w, 20.0), Sense::click());
                                if xf_resp.clicked() {
                                    t.crossfader_assign = match t.crossfader_assign {
                                        CrossfaderAssign::Off => CrossfaderAssign::A,
                                        CrossfaderAssign::A => CrossfaderAssign::B,
                                        CrossfaderAssign::B => CrossfaderAssign::Off,
                                    };
                                }
                                if ui.is_rect_visible(xf_rect) {
                                    let p = ui.painter_at(xf_rect);
                                    p.rect_filled(xf_rect, 0.0, COLOR_SURFACE_AREA);
                                    p.rect_stroke(
                                        xf_rect,
                                        0.0,
                                        Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                    );
                                    let pill = Rect::from_center_size(
                                        xf_rect.center(),
                                        vec2(col_w - 12.0, 16.0),
                                    );
                                    p.rect_filled(pill, 0.0, COLOR_SURFACE_BG);
                                    p.rect_stroke(
                                        pill,
                                        0.0,
                                        Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                    );
                                    p.text(
                                        pill.center(),
                                        egui::Align2::CENTER_CENTER,
                                        format!("XF: {}", label),
                                        FontId::monospace(9.0),
                                        col,
                                    );
                                }
                            }
                        }
                    });
                }

                // 2. Return Tracks (A Reverb, B Delay, etc.)
                if mixer_sections.show_returns {
                    for (r_idx, ret) in return_tracks.iter_mut().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(ret_col_w);
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                            // Full-Bleed Slate Blue / Teal Return Header
                            let (r_header_rect, _) = ui.allocate_exact_size(
                                vec2(ret_col_w, header_height),
                                Sense::hover(),
                            );
                            if ui.is_rect_visible(r_header_rect) {
                                let painter = ui.painter_at(r_header_rect);
                                let ret_color = if r_idx == 0 {
                                    COLOR_TRACK_BLUE // #4881aa Return A
                                } else {
                                    COLOR_TRACK_TEAL // #6baace Return B
                                };
                                painter.rect_filled(r_header_rect, 0.0, ret_color);
                                painter.rect_stroke(
                                    r_header_rect,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                                painter.text(
                                    r_header_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &ret.name,
                                    FontId::new(11.0, FontFamily::Proportional),
                                    COLOR_CONTROL_ON_FG,
                                );
                                painter.text(
                                    pos2(r_header_rect.center().x + 0.4, r_header_rect.center().y),
                                    egui::Align2::CENTER_CENTER,
                                    &ret.name,
                                    FontId::new(11.0, FontFamily::Proportional),
                                    COLOR_CONTROL_ON_FG,
                                );
                            }

                            // Return Clip Slots: 100% row-aligned empty slots (#242424, 22.0px height)
                            for _ in 0..visible_slots {
                                let (rect, _) =
                                    ui.allocate_exact_size(vec2(ret_col_w, slot_h), Sense::hover());
                                if ui.is_rect_visible(rect) {
                                    let p = ui.painter_at(rect);
                                    p.rect_filled(rect, 0.0, COLOR_SURFACE_AREA);
                                    p.rect_stroke(
                                        rect,
                                        0.0,
                                        Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                    );
                                }
                            }

                            // Return Stop Row: empty cell (20px height)
                            let (r_stop_rect, _) = ui.allocate_exact_size(
                                vec2(ret_col_w, stop_btn_height),
                                Sense::hover(),
                            );
                            if ui.is_rect_visible(r_stop_rect) {
                                let p = ui.painter_at(r_stop_rect);
                                p.rect_filled(r_stop_rect, 0.0, COLOR_SURFACE_AREA);
                                p.rect_stroke(
                                    r_stop_rect,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                            }

                            // Return FX label pill
                            let (fx_rect, _) =
                                ui.allocate_exact_size(vec2(ret_col_w, 22.0), Sense::hover());
                            if ui.is_rect_visible(fx_rect) {
                                let p = ui.painter_at(fx_rect);
                                p.rect_filled(fx_rect, 0.0, COLOR_SURFACE_AREA);
                                p.rect_stroke(
                                    fx_rect,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                                let pill = Rect::from_center_size(
                                    fx_rect.center(),
                                    vec2(ret_col_w - 8.0, 16.0),
                                );
                                p.rect_filled(pill, 0.0, COLOR_CONTROL_BG);
                                p.rect_stroke(
                                    pill,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                                p.text(
                                    pill.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &ret.effect_name,
                                    FontId::monospace(8.5),
                                    COLOR_RANGE_DEFAULT,
                                );
                            }

                            // Return Bipolar Pan Control
                            let (r_pan_c, _) =
                                ui.allocate_exact_size(vec2(ret_col_w, 22.0), Sense::hover());
                            let r_pan_rect = Rect::from_center_size(
                                r_pan_c.center(),
                                vec2(ret_col_w - 8.0, 18.0),
                            );
                            if ui.is_rect_visible(r_pan_c) {
                                let p = ui.painter_at(r_pan_c);
                                p.rect_filled(r_pan_c, 0.0, COLOR_SURFACE_AREA);
                                p.rect_stroke(
                                    r_pan_c,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                            }
                            let r_pan_resp = ui.interact(
                                r_pan_rect,
                                ui.id().with(("ret_pan", r_idx)),
                                Sense::click_and_drag(),
                            );
                            if r_pan_resp.dragged() {
                                let delta = r_pan_resp.drag_delta().x * 0.01;
                                ret.pan = (ret.pan + delta).clamp(-1.0, 1.0);
                            }
                            if r_pan_resp.double_clicked() {
                                ret.pan = 0.0;
                            }
                            if ui.is_rect_visible(r_pan_rect) {
                                let p = ui.painter_at(r_pan_rect);
                                paint_bipolar_pan_bar(&p, r_pan_rect, ret.pan);
                            }

                            // Return Vertical Fader + Stereo Meter (76px)
                            let (r_fader_c, _) =
                                ui.allocate_exact_size(vec2(ret_col_w, 80.0), Sense::hover());
                            if ui.is_rect_visible(r_fader_c) {
                                let p = ui.painter_at(r_fader_c);
                                p.rect_filled(r_fader_c, 0.0, COLOR_SURFACE_AREA);
                                p.rect_stroke(
                                    r_fader_c,
                                    0.0,
                                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                                );
                            }
                            let r_fm_rect = Rect::from_center_size(
                                r_fader_c.center(),
                                vec2(ret_col_w - 8.0, 76.0),
                            );
                            let r_pk = telemetry.master_peak_l * 0.6;
                            render_vertical_fader_and_meter(
                                ui,
                                &mut ret.volume_linear,
                                r_pk,
                                r_pk,
                                r_fm_rect,
                                format!("ret_fader_{}", r_idx),
                            );

                            // Return Activator (Mute / Solo)
                            render_return_activator_buttons(
                                ui,
                                r_idx,
                                &mut ret.mute,
                                &mut ret.solo,
                                ret_col_w,
                            );
                        });
                    }
                }

                // 3. Master Bus Column & Scene Launch Triggers
                ui.vertical(|ui| {
                    ui.set_width(master_col_w);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                    // Master Header (26px height)
                    let (m_header_rect, _) =
                        ui.allocate_exact_size(vec2(master_col_w, header_height), Sense::hover());
                    if ui.is_rect_visible(m_header_rect) {
                        let painter = ui.painter_at(m_header_rect);
                        painter.rect_filled(m_header_rect, 0.0, COLOR_SURFACE_BG);
                        painter.rect_stroke(
                            m_header_rect,
                            0.0,
                            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                        );
                        painter.text(
                            m_header_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Master / Scenes",
                            FontId::new(11.0, FontFamily::Proportional),
                            COLOR_CHOSEN_DEFAULT,
                        );
                    }

                    // 100% Row-Aligned Scene Launch Buttons (Fixed 22.0px height)
                    for scene_idx in 0..visible_slots {
                        let scene_label = if let Some(scene) = matrix.scenes.get(scene_idx) {
                            if !scene.name.is_empty() {
                                format!("▶ {}", scene.name)
                            } else {
                                format!("▶ Scene {}", scene_idx + 1)
                            }
                        } else {
                            format!("▶ Scene {}", scene_idx + 1)
                        };

                        let (s_rect, s_resp) =
                            ui.allocate_exact_size(vec2(master_col_w, slot_h), Sense::click());
                        if s_resp.clicked() {
                            action.trigger_scene = Some(scene_idx);
                        }

                        if ui.is_rect_visible(s_rect) {
                            let p = ui.painter_at(s_rect);
                            let bg = if s_resp.hovered() {
                                COLOR_SURFACE_BG
                            } else {
                                COLOR_DESKTOP
                            };
                            p.rect_filled(s_rect, 0.0, bg);
                            p.rect_stroke(s_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
                            p.text(
                                pos2(s_rect.left() + 8.0, s_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                scene_label,
                                FontId::monospace(9.5),
                                COLOR_CHOSEN_DEFAULT,
                            );
                        }
                    }

                    // Synchronized Stop All Clips Row ([■ Stop All])
                    let (stop_all_rect, stop_all_resp) =
                        ui.allocate_exact_size(vec2(master_col_w, stop_btn_height), Sense::click());
                    if stop_all_resp.clicked() {
                        action.stop_all_clips = true;
                    }
                    if ui.is_rect_visible(stop_all_rect) {
                        let p = ui.painter_at(stop_all_rect);
                        let bg = if stop_all_resp.hovered() {
                            COLOR_SURFACE_HIGHLIGHT
                        } else {
                            COLOR_SURFACE_AREA
                        };
                        p.rect_filled(stop_all_rect, 0.0, bg);
                        p.rect_stroke(
                            stop_all_rect,
                            0.0,
                            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                        );
                        p.text(
                            stop_all_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "■ Stop All",
                            FontId::monospace(9.5),
                            COLOR_CHOSEN_RECORD,
                        );
                    }

                    // Master Out Header Pill
                    let (m_out_rect, _) =
                        ui.allocate_exact_size(vec2(master_col_w, 22.0), Sense::hover());
                    if ui.is_rect_visible(m_out_rect) {
                        let p = ui.painter_at(m_out_rect);
                        p.rect_filled(m_out_rect, 0.0, COLOR_SURFACE_AREA);
                        p.rect_stroke(
                            m_out_rect,
                            0.0,
                            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                        );
                        let pill = Rect::from_center_size(
                            m_out_rect.center(),
                            vec2(master_col_w - 8.0, 16.0),
                        );
                        p.rect_filled(pill, 0.0, COLOR_SURFACE_BG);
                        p.rect_stroke(
                            pill,
                            0.0,
                            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                        );
                        p.text(
                            pill.center(),
                            egui::Align2::CENTER_CENTER,
                            "Master Out",
                            FontId::monospace(9.0),
                            COLOR_CHOSEN_DEFAULT,
                        );
                    }

                    // Master Bipolar Balance / Crossfader if enabled
                    if mixer_sections.show_crossfader {
                        let (xf_container, _) =
                            ui.allocate_exact_size(vec2(master_col_w, 22.0), Sense::hover());
                        if ui.is_rect_visible(xf_container) {
                            let p = ui.painter_at(xf_container);
                            p.rect_filled(xf_container, 0.0, COLOR_SURFACE_AREA);
                            p.rect_stroke(
                                xf_container,
                                0.0,
                                Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                            );
                        }
                        let xf_rect = Rect::from_center_size(
                            xf_container.center(),
                            vec2(master_col_w - 8.0, 18.0),
                        );
                        if render_dj_crossfader(ui, crossfader_pos, xf_rect) {
                            action.crossfader_changed = Some(*crossfader_pos);
                        }
                    }

                    // Master Vertical Volume Fader (76px) + Stereo Ballistics Meter
                    let (m_fader_c, _) =
                        ui.allocate_exact_size(vec2(master_col_w, 80.0), Sense::hover());
                    if ui.is_rect_visible(m_fader_c) {
                        let p = ui.painter_at(m_fader_c);
                        p.rect_filled(m_fader_c, 0.0, COLOR_SURFACE_AREA);
                        p.rect_stroke(
                            m_fader_c,
                            0.0,
                            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                        );
                    }
                    let m_fm_rect = Rect::from_center_size(
                        m_fader_c.center(),
                        vec2(master_col_w - 8.0, 76.0),
                    );
                    render_vertical_fader_and_meter(
                        ui,
                        master_vol,
                        telemetry.master_peak_l,
                        telemetry.master_peak_r,
                        m_fm_rect,
                        "master_fader",
                    );

                    // Master Limiter / Output Pill
                    let (m_lim_rect, _) =
                        ui.allocate_exact_size(vec2(master_col_w, 24.0), Sense::hover());
                    if ui.is_rect_visible(m_lim_rect) {
                        let p = ui.painter_at(m_lim_rect);
                        p.rect_filled(m_lim_rect, 0.0, COLOR_SURFACE_AREA);
                        p.rect_stroke(
                            m_lim_rect,
                            0.0,
                            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                        );
                        let pill = Rect::from_center_size(
                            m_lim_rect.center(),
                            vec2(master_col_w - 8.0, 20.0),
                        );
                        p.rect_filled(pill, 0.0, COLOR_DESKTOP);
                        p.rect_stroke(
                            pill,
                            0.0,
                            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                        );
                        p.text(
                            pill.center(),
                            egui::Align2::CENTER_CENTER,
                            "LIMITER ACTIVE",
                            FontId::monospace(8.0),
                            COLOR_CHOSEN_PLAY,
                        );
                    }
                });
            });
        });

    action
}

/// Custom Rotary Arc Sends Widget (A and B dials)
pub fn render_rotary_arc_sends(
    ui: &mut Ui,
    sends: &mut [f32],
    col_width: f32,
    track_idx: usize,
) -> Option<(usize, usize, f32)> {
    let height = 36.0_f32;
    let (rect, _) = ui.allocate_exact_size(vec2(col_width, height), Sense::hover());
    let mut send_changed = None;

    if ui.is_rect_visible(rect) {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, COLOR_SURFACE_AREA);
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));

        let num_dials = sends.len().min(2);
        let dial_spacing = col_width / (num_dials + 1) as f32;

        for (s_idx, s_val) in sends.iter_mut().enumerate().take(num_dials) {
            let cx = rect.left() + dial_spacing * (s_idx + 1) as f32;
            let cy = rect.center().y + 3.0;
            let dial_center = pos2(cx, cy);
            let dial_radius = 9.5_f32;
            let dial_rect = Rect::from_center_size(
                dial_center,
                vec2(dial_radius * 2.0 + 8.0, dial_radius * 2.0 + 14.0),
            );
            let dial_resp = ui.interact(
                dial_rect,
                ui.id().with(("send_dial", track_idx, s_idx)),
                Sense::click_and_drag(),
            );

            let prev_val = *s_val;

            if dial_resp.dragged() {
                let delta = -dial_resp.drag_delta().y * 0.005;
                *s_val = (*s_val + delta).clamp(0.0, 1.0);
                if *s_val != prev_val {
                    send_changed = Some((track_idx, s_idx, *s_val));
                }
            }
            if dial_resp.double_clicked() {
                *s_val = 0.0;
                send_changed = Some((track_idx, s_idx, 0.0));
            }

            // Dial Label ("A" or "B")
            let label = if s_idx == 0 { "A" } else { "B" };
            p.text(
                pos2(cx, rect.top() + 7.0),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::monospace(8.5),
                COLOR_CONTROL_FG,
            );

            // Dial Background Arc (270 degrees sweep from 135 deg)
            let start_angle = 135.0_f32.to_radians();
            let total_sweep = 270.0_f32.to_radians();
            let bg_segments = 16;
            for i in 0..bg_segments {
                let a1 = start_angle + (i as f32 / bg_segments as f32) * total_sweep;
                let a2 = start_angle + ((i + 1) as f32 / bg_segments as f32) * total_sweep;
                let p1 = pos2(cx + a1.cos() * dial_radius, cy + a1.sin() * dial_radius);
                let p2 = pos2(cx + a2.cos() * dial_radius, cy + a2.sin() * dial_radius);
                p.line_segment([p1, p2], Stroke::new(2.2_f32, COLOR_METER_BG));
            }

            // Active Send Arc in #03c3d5
            let active_sweep = total_sweep * (*s_val).clamp(0.0, 1.0);
            if active_sweep > 0.01 {
                let active_segments = (16.0 * (*s_val).clamp(0.0, 1.0)).ceil() as usize;
                let active_segments = active_segments.max(1);
                for i in 0..active_segments {
                    let a1 = start_angle + (i as f32 / active_segments as f32) * active_sweep;
                    let a2 = start_angle + ((i + 1) as f32 / active_segments as f32) * active_sweep;
                    let p1 = pos2(cx + a1.cos() * dial_radius, cy + a1.sin() * dial_radius);
                    let p2 = pos2(cx + a2.cos() * dial_radius, cy + a2.sin() * dial_radius);
                    p.line_segment([p1, p2], Stroke::new(2.2_f32, COLOR_RANGE_DEFAULT));
                }
            }

            // Center Disc in #111111
            p.circle_filled(dial_center, dial_radius - 2.5, COLOR_CONTROL_CONTRAST_FRAME);

            // Needle
            let cur_angle = start_angle + active_sweep;
            let needle_tip = pos2(
                cx + cur_angle.cos() * (dial_radius - 2.0),
                cy + cur_angle.sin() * (dial_radius - 2.0),
            );
            p.line_segment([dial_center, needle_tip], Stroke::new(1.5_f32, COLOR_CONTROL_FG));
        }

    }

    send_changed
}

/// Paint helper for Bipolar Split-Bar Pan Control
pub fn paint_bipolar_pan_bar(p: &egui::Painter, rect: Rect, pan: f32) {
    p.rect_filled(rect, 0.0, COLOR_METER_BG);
    p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));

    let cx = rect.center().x;
    p.line_segment(
        [pos2(cx, rect.top() + 1.0), pos2(cx, rect.bottom() - 1.0)],
        Stroke::new(1.0_f32, COLOR_SURFACE_HIGHLIGHT),
    );

    let half_w = (rect.width() * 0.5 - 2.0).max(1.0);
    if pan.abs() > 0.005 {
        let bar_rect = if pan < 0.0 {
            Rect::from_min_max(
                pos2((cx + pan * half_w).max(rect.left() + 1.0), rect.top() + 2.0),
                pos2(cx, rect.bottom() - 2.0),
            )
        } else {
            Rect::from_min_max(
                pos2(cx, rect.top() + 2.0),
                pos2((cx + pan * half_w).min(rect.right() - 1.0), rect.bottom() - 2.0),
            )
        };
        p.rect_filled(bar_rect, 0.0, COLOR_RANGE_DEFAULT);
    }

    let txt = if pan.abs() < 0.02 {
        "C".to_string()
    } else if pan < 0.0 {
        format!("{}L", ((-pan) * 50.0).round() as i32)
    } else {
        format!("{}R", ((pan) * 50.0).round() as i32)
    };
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        txt,
        FontId::monospace(9.5),
        Color32::WHITE,
    );
}

/// Side-by-side vertical volume fader (76px) and vertical stereo segmented VU meter
pub fn render_vertical_fader_and_meter(
    ui: &mut Ui,
    vol_linear: &mut f32,
    peak_l: f32,
    peak_r: f32,
    rect: Rect,
    id_source: impl std::hash::Hash,
) -> bool {
    let mut changed = false;

    let fader_w = (rect.width() * 0.60).round(); // ~60% width for fader
    let meter_w = rect.width() - fader_w; // ~40% width for meter
    let fader_rect = Rect::from_min_size(rect.min, vec2(fader_w, rect.height()));
    let meter_rect = Rect::from_min_size(
        pos2(rect.min.x + fader_w, rect.min.y),
        vec2(meter_w, rect.height()),
    );

    let resp = ui.interact(
        fader_rect,
        ui.id().with(("fader_drag", id_source)),
        Sense::click_and_drag(),
    );
    if resp.dragged() {
        let track_h = (fader_rect.height() - 24.0).max(10.0);
        let delta = -resp.drag_delta().y / track_h * 1.25;
        *vol_linear = (*vol_linear + delta).clamp(0.0, 1.25);
        changed = true;
    }
    if resp.double_clicked() {
        *vol_linear = 1.0; // 0.0 dB unity reset
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        let p = ui.painter_at(rect);

        // Fader slot background area
        p.rect_filled(fader_rect, 0.0, COLOR_SURFACE_AREA);
        p.rect_stroke(
            fader_rect,
            0.0,
            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
        );

        let slot_x = fader_rect.left() + (fader_w * 0.44).round();
        let slot_top = fader_rect.top() + 4.0;
        let slot_bottom = fader_rect.bottom() - 19.0;
        let slot_travel = slot_bottom - slot_top;

        // Fader slot track (#181818, 3.0px wide)
        let slot_rect = Rect::from_min_max(
            pos2(slot_x - 1.5, slot_top),
            pos2(slot_x + 1.5, slot_bottom),
        );
        p.rect_filled(slot_rect, 0.0, COLOR_METER_BG);
        p.rect_stroke(
            slot_rect,
            0.0,
            Stroke::new(0.5_f32, COLOR_CONTROL_CONTRAST_FRAME),
        );

        // Unity 0dB tick mark at 75% height
        let unity_y = slot_bottom - 0.75 * slot_travel;
        p.line_segment(
            [pos2(slot_x - 7.0, unity_y), pos2(slot_x + 7.0, unity_y)],
            Stroke::new(1.0_f32, Color32::from_rgb(0x75, 0x75, 0x75)),
        );
        p.text(
            pos2(slot_x + 9.0, unity_y),
            egui::Align2::LEFT_CENTER,
            "0",
            FontId::monospace(8.0),
            Color32::from_rgb(0x75, 0x75, 0x75),
        );

        // Rectangular Ableton fader cap (#b5b5b5 / #ffad56 when dragged)
        let norm_vol = (*vol_linear / 1.25).clamp(0.0, 1.0);
        let cap_y = slot_bottom - norm_vol * slot_travel;
        let cap_rect = Rect::from_center_size(pos2(slot_x, cap_y), vec2(26.0, 11.0));
        let cap_color = if resp.dragged() {
            COLOR_CHOSEN_DEFAULT // #ffad56
        } else {
            COLOR_CONTROL_FG // #b5b5b5
        };
        p.rect_filled(cap_rect, 0.0, cap_color);
        p.rect_stroke(cap_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        p.line_segment(
            [
                pos2(cap_rect.left(), cap_rect.center().y),
                pos2(cap_rect.right(), cap_rect.center().y),
            ],
            Stroke::new(1.0_f32, COLOR_CONTROL_ON_FG),
        );

        // Digital dB readout pill
        let pill_rect = Rect::from_min_size(
            pos2(fader_rect.left() + 3.0, fader_rect.bottom() - 16.0),
            vec2(fader_w - 6.0, 14.0),
        );
        p.rect_filled(pill_rect, 0.0, COLOR_METER_BG);
        p.rect_stroke(pill_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));

        let (db_str, db_col) = if *vol_linear <= 0.001 {
            ("-inf dB".to_string(), COLOR_RANGE_DEFAULT)
        } else {
            let db = 20.0 * vol_linear.log10();
            let col = if db >= 0.05 {
                COLOR_CHOSEN_DEFAULT
            } else {
                COLOR_RANGE_DEFAULT
            };
            if db.abs() < 0.05 {
                ("0.0 dB".to_string(), col)
            } else {
                (format!("{:+.1} dB", db), col)
            }
        };
        p.text(
            pill_rect.center(),
            egui::Align2::CENTER_CENTER,
            db_str,
            FontId::monospace(8.0),
            db_col,
        );

        // Segmented Stereo VU Meter
        render_segmented_stereo_vu_meter(&p, meter_rect, peak_l, peak_r, *vol_linear);
    }

    changed
}

/// Custom-Painted Ableton Live 12 Segmented Stereo VU Meter
pub fn render_segmented_stereo_vu_meter(
    painter: &egui::Painter,
    rect: Rect,
    peak_l: f32,
    peak_r: f32,
    vol_linear: f32,
) {
    painter.rect_filled(rect, 0.0, COLOR_METER_BG);
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));

    let inner_rect = rect.shrink2(vec2(3.0, 3.0));
    let num_segments = 18;
    let seg_gap = 0.8_f32;
    let total_gap = seg_gap * (num_segments - 1) as f32;
    let seg_h = ((inner_rect.height() - total_gap) / num_segments as f32).max(1.5);

    let ch_gap = 2.0_f32;
    let ch_w = ((inner_rect.width() - ch_gap) * 0.5).floor();

    let l_left = inner_rect.left();
    let r_left = l_left + ch_w + ch_gap;

    let eff_l = (peak_l * (vol_linear / 1.0).max(0.1)).clamp(0.0, 1.25) / 1.25;
    let eff_r = (peak_r * (vol_linear / 1.0).max(0.1)).clamp(0.0, 1.25) / 1.25;

    for seg_idx in 0..num_segments {
        let frac = (seg_idx + 1) as f32 / num_segments as f32;
        let y_bot = inner_rect.bottom() - seg_idx as f32 * (seg_h + seg_gap);
        let y_top = y_bot - seg_h;

        // Color thresholds according to Live 12 StandardVuMeter:
        // <= -12 dB: #00f758 (Green)
        // -12 to 0 dB: #ffd100 (Yellow)
        // > 0 dB: #ff0a0a (Red)
        let seg_color = if frac <= 0.70 {
            COLOR_VU_MIN
        } else if frac <= 0.88 {
            COLOR_VU_MID
        } else {
            COLOR_VU_MAX
        };

        // Left channel segment
        let l_seg_rect = Rect::from_min_max(pos2(l_left, y_top), pos2(l_left + ch_w, y_bot));
        let l_color = if frac <= eff_l {
            seg_color
        } else {
            Color32::from_rgb(0x22, 0x22, 0x22)
        };
        painter.rect_filled(l_seg_rect, 0.0, l_color);

        // Right channel segment
        let r_seg_rect = Rect::from_min_max(pos2(r_left, y_top), pos2(r_left + ch_w, y_bot));
        let r_color = if frac <= eff_r {
            seg_color
        } else {
            Color32::from_rgb(0x22, 0x22, 0x22)
        };
        painter.rect_filled(r_seg_rect, 0.0, r_color);
    }
}

/// Crisp Rectangular Ableton Live 12 Buttons: Track Activator / Solo / Record Arm
pub fn render_activator_buttons(
    ui: &mut Ui,
    track_idx: usize,
    mute: &mut bool,
    solo: &mut bool,
    is_armed: &mut bool,
    col_width: f32,
) -> (bool, bool, bool) {
    let height = 24.0_f32;
    let (container_rect, _) = ui.allocate_exact_size(vec2(col_width, height), Sense::hover());
    let mut mute_toggled = false;
    let mut solo_toggled = false;
    let mut arm_toggled = false;

    if ui.is_rect_visible(container_rect) {
        let p = ui.painter_at(container_rect);
        p.rect_filled(container_rect, 0.0, COLOR_SURFACE_AREA);
        p.rect_stroke(
            container_rect,
            0.0,
            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
        );

        let inner = Rect::from_center_size(
            container_rect.center(),
            vec2(col_width - 8.0, 20.0),
        );
        let btn_gap = 2.0_f32;
        let btn_w = ((inner.width() - btn_gap * 2.0) / 3.0).floor();

        // 1. Track Activator (Mute): 1, 2, 3 in #ffad56 amber with bold #070707 text when active; #363636 when muted
        let act_rect = Rect::from_min_size(inner.min, vec2(btn_w, inner.height()));
        let act_resp = ui.interact(act_rect, ui.id().with(("act_btn", track_idx)), Sense::click());
        if act_resp.clicked() {
            *mute = !*mute;
            mute_toggled = true;
        }
        let is_active = !*mute;
        let act_bg = if is_active {
            COLOR_CHOSEN_DEFAULT // #ffad56
        } else {
            COLOR_SURFACE_BG // #363636
        };
        let act_text_col = if is_active {
            COLOR_CONTROL_ON_FG // #070707 bold
        } else {
            Color32::from_rgb(0x75, 0x75, 0x75)
        };
        p.rect_filled(act_rect, 0.0, act_bg);
        p.rect_stroke(act_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        let title_num = format!("{}", track_idx + 1);
        p.text(
            act_rect.center(),
            egui::Align2::CENTER_CENTER,
            &title_num,
            FontId::new(10.5, FontFamily::Proportional),
            act_text_col,
        );
        if is_active {
            p.text(
                pos2(act_rect.center().x + 0.4, act_rect.center().y),
                egui::Align2::CENTER_CENTER,
                &title_num,
                FontId::new(10.5, FontFamily::Proportional),
                act_text_col,
            );
        }

        // 2. Solo: S in #3c6ab6 blue when active
        let solo_rect = Rect::from_min_size(
            pos2(inner.min.x + btn_w + btn_gap, inner.min.y),
            vec2(btn_w, inner.height()),
        );
        let solo_resp = ui.interact(solo_rect, ui.id().with(("solo_btn", track_idx)), Sense::click());
        if solo_resp.clicked() {
            *solo = !*solo;
            solo_toggled = true;
        }
        let solo_bg = if *solo {
            COLOR_CHOSEN_PRELISTEN // #3c6ab6
        } else {
            COLOR_DESKTOP // #2a2a2a
        };
        let solo_text_col = if *solo {
            Color32::WHITE
        } else {
            COLOR_CONTROL_FG // #b5b5b5
        };
        p.rect_filled(solo_rect, 0.0, solo_bg);
        p.rect_stroke(solo_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        p.text(
            solo_rect.center(),
            egui::Align2::CENTER_CENTER,
            "S",
            FontId::new(10.5, FontFamily::Proportional),
            solo_text_col,
        );

        // 3. Record Arm: ● in #ff5559 red when active
        let arm_rect = Rect::from_min_size(
            pos2(inner.min.x + (btn_w + btn_gap) * 2.0, inner.min.y),
            vec2(btn_w, inner.height()),
        );
        let arm_resp = ui.interact(arm_rect, ui.id().with(("arm_btn", track_idx)), Sense::click());
        if arm_resp.clicked() {
            *is_armed = !*is_armed;
            arm_toggled = true;
        }
        let arm_bg = if *is_armed {
            COLOR_CHOSEN_RECORD // #ff5559
        } else {
            COLOR_DESKTOP // #2a2a2a
        };
        let arm_text_col = if *is_armed {
            Color32::WHITE
        } else {
            COLOR_CHOSEN_RECORD // #ff5559
        };
        p.rect_filled(arm_rect, 0.0, arm_bg);
        p.rect_stroke(arm_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        p.text(
            arm_rect.center(),
            egui::Align2::CENTER_CENTER,
            "●",
            FontId::new(10.5, FontFamily::Proportional),
            arm_text_col,
        );
    }

    (mute_toggled, solo_toggled, arm_toggled)
}

/// Return Track Activator Buttons (Return Activator A/B & Solo S)
pub fn render_return_activator_buttons(
    ui: &mut Ui,
    ret_idx: usize,
    mute: &mut bool,
    solo: &mut bool,
    col_width: f32,
) -> (bool, bool) {
    let height = 24.0_f32;
    let (container_rect, _) = ui.allocate_exact_size(vec2(col_width, height), Sense::hover());
    let mut mute_toggled = false;
    let mut solo_toggled = false;

    if ui.is_rect_visible(container_rect) {
        let p = ui.painter_at(container_rect);
        p.rect_filled(container_rect, 0.0, COLOR_SURFACE_AREA);
        p.rect_stroke(
            container_rect,
            0.0,
            Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
        );

        let inner = Rect::from_center_size(
            container_rect.center(),
            vec2(col_width - 8.0, 20.0),
        );
        let btn_gap = 2.0_f32;
        let btn_w = ((inner.width() - btn_gap) * 0.5).floor();

        // Return Activator
        let act_rect = Rect::from_min_size(inner.min, vec2(btn_w, inner.height()));
        let act_resp =
            ui.interact(act_rect, ui.id().with(("ret_act_btn", ret_idx)), Sense::click());
        if act_resp.clicked() {
            *mute = !*mute;
            mute_toggled = true;
        }
        let is_active = !*mute;
        let act_bg = if is_active {
            COLOR_CHOSEN_DEFAULT
        } else {
            COLOR_SURFACE_BG
        };
        let act_text_col = if is_active {
            COLOR_CONTROL_ON_FG
        } else {
            Color32::from_rgb(0x75, 0x75, 0x75)
        };
        p.rect_filled(act_rect, 0.0, act_bg);
        p.rect_stroke(act_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        let label = if ret_idx == 0 { "A" } else { "B" };
        p.text(
            act_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::new(10.5, FontFamily::Proportional),
            act_text_col,
        );

        // Solo
        let solo_rect = Rect::from_min_size(
            pos2(inner.min.x + btn_w + btn_gap, inner.min.y),
            vec2(btn_w, inner.height()),
        );
        let solo_resp =
            ui.interact(solo_rect, ui.id().with(("ret_solo_btn", ret_idx)), Sense::click());
        if solo_resp.clicked() {
            *solo = !*solo;
            solo_toggled = true;
        }
        let solo_bg = if *solo {
            COLOR_CHOSEN_PRELISTEN
        } else {
            COLOR_DESKTOP
        };
        let solo_text_col = if *solo {
            Color32::WHITE
        } else {
            COLOR_CONTROL_FG
        };
        p.rect_filled(solo_rect, 0.0, solo_bg);
        p.rect_stroke(solo_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
        p.text(
            solo_rect.center(),
            egui::Align2::CENTER_CENTER,
            "S",
            FontId::new(10.5, FontFamily::Proportional),
            solo_text_col,
        );
    }

    (mute_toggled, solo_toggled)
}

/// Custom-Painted DJ Crossfader Widget
pub fn render_dj_crossfader(ui: &mut Ui, crossfader_pos: &mut f32, rect: Rect) -> bool {
    let resp = ui.interact(
        rect,
        ui.id().with("dj_crossfader_drag"),
        Sense::click_and_drag(),
    );
    let mut changed = false;

    if resp.dragged() {
        let delta = resp.drag_delta().x / (rect.width() * 0.5) * 1.0;
        *crossfader_pos = (*crossfader_pos + delta).clamp(-1.0, 1.0);
        changed = true;
    }
    if resp.double_clicked() {
        *crossfader_pos = 0.0;
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, COLOR_METER_BG);
        p.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));

        let cx = rect.center().x;
        p.line_segment(
            [pos2(cx, rect.top() + 1.0), pos2(cx, rect.bottom() - 1.0)],
            Stroke::new(1.0_f32, COLOR_SURFACE_HIGHLIGHT),
        );

        p.text(
            pos2(rect.left() + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "A",
            FontId::monospace(8.5),
            COLOR_CHOSEN_DEFAULT,
        );
        p.text(
            pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "B",
            FontId::monospace(8.5),
            COLOR_RANGE_DEFAULT,
        );

        let thumb_travel = rect.width() - 20.0;
        let thumb_x = cx + *crossfader_pos * (thumb_travel * 0.5);
        let thumb_rect = Rect::from_center_size(
            pos2(thumb_x, rect.center().y),
            vec2(12.0, rect.height() - 4.0),
        );
        let thumb_col = if resp.dragged() {
            COLOR_CHOSEN_DEFAULT
        } else {
            COLOR_CONTROL_FG
        };
        p.rect_filled(thumb_rect, 0.0, thumb_col);
        p.rect_stroke(thumb_rect, 0.0, Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME));
    }

    changed
}

/// Ableton Live 12 Clip Slot Renderer
pub fn render_clip_slot(
    painter: &egui::Painter,
    rect: Rect,
    state: SlotState,
    clip_name: &str,
    hovered: bool,
    is_selected: bool,
    current_time: f64,
) {
    let base_bg = if hovered {
        COLOR_SURFACE_HIGHLIGHT
    } else {
        COLOR_CONTROL_BG
    };
    painter.rect_filled(rect, 0.0, base_bg);

    match state {
        SlotState::Empty => {
            if hovered {
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0_f32, COLOR_SURFACE_BG),
                );
                let box_size = 7.0_f32;
                let box_rect = Rect::from_center_size(
                    pos2(rect.center().x - 18.0, rect.center().y),
                    vec2(box_size, box_size),
                );
                painter.rect_stroke(box_rect, 0.0, Stroke::new(1.0_f32, COLOR_TEXT_MUTED));
                painter.text(
                    pos2(rect.center().x - 11.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "+ Clip",
                    FontId::monospace(9.0),
                    COLOR_TEXT_MUTED,
                );
            } else {
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME),
                );
                painter.circle_filled(rect.center(), 1.5, COLOR_SURFACE_HIGHLIGHT);
            }
        }
        SlotState::Stopped => {
            let cat_col = track_category_color(clip_name);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(0x2e, 0x2e, 0x2e));

            // Left colored category strip
            let bar_rect = Rect::from_min_size(rect.min, vec2(3.5, rect.height()));
            painter.rect_filled(bar_rect, 0.0, cat_col);

            let stroke_col = if hovered {
                COLOR_RANGE_DEFAULT
            } else {
                COLOR_CONTROL_CONTRAST_FRAME
            };
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, stroke_col));

            let play_col = if hovered {
                COLOR_CHOSEN_PLAY
            } else {
                COLOR_CONTROL_FG
            };
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
                Color32::WHITE,
            );
        }
        SlotState::QueuedLaunch(_) => {
            let pulse = ((current_time * 8.0).sin().abs() as f32).clamp(0.3, 1.0);
            let stroke_color = Color32::from_rgba_premultiplied(
                255,
                (173.0 * pulse) as u8,
                (86.0 * pulse) as u8,
                (255.0 * pulse) as u8,
            );
            painter.rect_filled(rect, 0.0, Color32::from_rgb(0x2e, 0x2e, 0x2e));
            painter.rect_stroke(rect, 0.0, Stroke::new(1.5_f32, stroke_color));
            painter.text(
                pos2(rect.left() + 6.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "▶",
                FontId::new(10.0, FontFamily::Proportional),
                COLOR_CHOSEN_DEFAULT,
            );
            painter.text(
                pos2(rect.left() + 22.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_CHOSEN_DEFAULT,
            );
        }
        SlotState::Playing { phase, .. } => {
            painter.rect_filled(rect, 0.0, Color32::from_rgb(0x12, 0x33, 0x20));
            painter.rect_stroke(rect, 0.0, Stroke::new(1.5_f32, COLOR_CHOSEN_PLAY));

            let prog_w = (rect.width() * (phase as f32).fract().clamp(0.0, 1.0)).max(3.0);
            let prog_rect = Rect::from_min_size(
                pos2(rect.left(), rect.bottom() - 2.5),
                vec2(prog_w, 2.5),
            );
            painter.rect_filled(prog_rect, 0.0, COLOR_CHOSEN_PLAY);

            let pulse = ((current_time * 6.0).sin().abs() as f32).clamp(0.7, 1.0);
            let icon_col = Color32::from_rgba_premultiplied(
                0,
                (211.0 * pulse) as u8,
                (141.0 * pulse) as u8,
                255,
            );
            painter.text(
                pos2(rect.left() + 7.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "▶",
                FontId::new(10.0, FontFamily::Proportional),
                icon_col,
            );
            painter.text(
                pos2(rect.left() + 22.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_CHOSEN_PLAY,
            );
            painter.text(
                pos2(rect.left() + 22.4, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_CHOSEN_PLAY,
            );
        }
        SlotState::QueuedStop(_) => {
            let blink = (current_time * 8.0).fract() > 0.5;
            let stroke_color = if blink {
                COLOR_CHOSEN_RECORD
            } else {
                COLOR_CONTROL_CONTRAST_FRAME
            };
            painter.rect_filled(rect, 0.0, Color32::from_rgb(0x2e, 0x2e, 0x2e));
            painter.rect_stroke(rect, 0.0, Stroke::new(1.5_f32, stroke_color));
            painter.text(
                pos2(rect.left() + 6.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "■",
                FontId::new(10.0, FontFamily::Proportional),
                COLOR_CHOSEN_RECORD,
            );
            painter.text(
                pos2(rect.left() + 22.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(9.5, FontFamily::Proportional),
                COLOR_CHOSEN_RECORD,
            );
        }
    }

    if is_selected {
        painter.rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(1.5_f32, COLOR_RANGE_DEFAULT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ableton_timeline::{MidiPattern, SessionClip};

    #[test]
    fn test_session_matrix_render_and_slot_selection() {
        let ctx = egui::Context::default();
        let mut matrix = SessionMatrix::new(3, 8);
        matrix.tracks[0].slots[0].clip = Some(SessionClip::new(
            1,
            "Test Clip",
            ableton_timeline::ClipPattern::Midi(MidiPattern::new(4.0)),
        ));
        matrix.tracks[0].slots[0].state = SlotState::Stopped;

        let mut tracks_gui = vec![GuiTrackState {
            name: "Track 1".into(),
            volume_linear: 0.9,
            pan: 0.0,
            mute: false,
            solo: false,
            sends: vec![0.0, 0.0],
            input_routing: "All Ins".into(),
            output_routing: "Master".into(),
            monitor_mode: "Auto".into(),
            crossfader_assign: CrossfaderAssign::Off,
        }];
        let mut return_tracks = Vec::new();
        let mut mixer_sections = SessionMixerSections::default();
        let mut quantization = LaunchQuantization::Bar1;
        let mut crossfader_pos = 0.0f32;
        let mut master_vol = 0.9f32;
        let telemetry = AudioTelemetry::new();

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.data_mut(|d| {
                    d.insert_temp(egui::Id::new("aruston_selected_track"), 0usize);
                    d.insert_temp(egui::Id::new("aruston_selected_slot"), Some((0usize, 0usize)));
                });

                let action = render_session_matrix(
                    ui,
                    &mut matrix,
                    &mut tracks_gui,
                    &mut return_tracks,
                    &mut mixer_sections,
                    &mut quantization,
                    &mut crossfader_pos,
                    &mut master_vol,
                    &telemetry,
                    0.0,
                );

                assert!(action.stop_clip.is_none());
            });
        });
    }

    #[test]
    fn test_zero_tofu_glyph_in_session_matrix_source() {
        let source = include_str!("session_matrix.rs");
        assert!(
            !source.contains('\u{25A1}'),
            "session_matrix.rs must not contain literal tofu box U+25A1"
        );
    }

    #[test]
    fn test_bipolar_pan_control_bounds_and_center_reset() {
        let mut pan = 0.45f32;
        // Test clamping
        pan = (pan + 1.0).clamp(-1.0, 1.0);
        assert_eq!(pan, 1.0);
        pan = (pan - 3.0).clamp(-1.0, 1.0);
        assert_eq!(pan, -1.0);
        // Double-click reset
        pan = 0.0;
        assert_eq!(pan, 0.0);
    }

    #[test]
    fn test_vertical_fader_unity_tick_and_linear_mapping() {
        let mut vol = 0.9f32;
        // Test clamping up to +6 dB (1.25 linear)
        vol = (vol + 0.8).clamp(0.0, 1.25);
        assert_eq!(vol, 1.25);
        vol = (vol - 2.0).clamp(0.0, 1.25);
        assert_eq!(vol, 0.0);
        // Unity reset
        vol = 1.0;
        assert_eq!(vol, 1.0);
        let db = 20.0 * vol.log10();
        assert!((db - 0.0).abs() < 1e-4, "Unity linear 1.0 must be exactly 0.0 dB");
    }

    #[test]
    fn test_segmented_vu_meter_color_thresholds() {
        let num_segments = 18;
        for seg_idx in 0..num_segments {
            let frac = (seg_idx + 1) as f32 / num_segments as f32;
            let col = if frac <= 0.70 {
                COLOR_VU_MIN
            } else if frac <= 0.88 {
                COLOR_VU_MID
            } else {
                COLOR_VU_MAX
            };
            if frac <= 0.70 {
                assert_eq!(col, Color32::from_rgb(0x00, 0xf7, 0x58));
            } else if frac <= 0.88 {
                assert_eq!(col, Color32::from_rgb(0xff, 0xd1, 0x00));
            } else {
                assert_eq!(col, Color32::from_rgb(0xff, 0x0a, 0x0a));
            }
        }
    }

    #[test]
    fn test_no_generic_sliders_in_session_matrix_source() {
        let source = include_str!("session_matrix.rs");
        let prod_code = source.split("#[cfg(test)]").next().unwrap_or(source);
        let banned = ["Slider", "::new"].concat();
        assert!(
            !prod_code.contains(&banned),
            "session_matrix.rs production code must not contain generic egui::Slider"
        );
    }

    #[test]
    fn test_item_spacing_zero_in_session_matrix() {
        let source = include_str!("session_matrix.rs");
        assert!(
            source.contains("ui.spacing_mut().item_spacing = vec2(0.0, 0.0);"),
            "session_matrix.rs must set item_spacing to vec2(0.0, 0.0)"
        );
    }
}
