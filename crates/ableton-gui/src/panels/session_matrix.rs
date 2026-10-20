//! Session Clip Matrix panel with quantized clip slots, scenes, and channel strips.

use egui::{
    pos2, vec2, Button, Color32, FontFamily, FontId, Rect, RichText, Sense, Slider, Stroke, Ui,
};
use ableton_core::AudioTelemetry;
use ableton_timeline::{SessionMatrix, SlotState};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BORDER, COLOR_GREEN,
    COLOR_RUBY, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};
use crate::widgets::StereoMeterWidget;

pub struct SessionMatrixAction {
    pub trigger_clip: Option<(usize, usize)>, // (track_idx, slot_idx)
    pub stop_clip: Option<usize>,             // track_idx
    pub trigger_scene: Option<usize>,         // scene_idx
    pub track_volume_changed: Option<(usize, f32)>,
    pub track_pan_changed: Option<(usize, f32)>,
    pub track_mute_toggled: Option<usize>,
    pub track_solo_toggled: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GuiTrackState {
    pub name: String,
    pub volume_linear: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
}

pub fn render_session_matrix(
    ui: &mut Ui,
    matrix: &mut SessionMatrix,
    tracks_gui: &mut [GuiTrackState],
    telemetry: &AudioTelemetry,
    current_time: f64,
) -> SessionMatrixAction {
    let mut action = SessionMatrixAction {
        trigger_clip: None,
        stop_clip: None,
        trigger_scene: None,
        track_volume_changed: None,
        track_pan_changed: None,
        track_mute_toggled: None,
        track_solo_toggled: None,
    };

    let track_count = matrix.tracks.len();
    let scene_count = matrix.scenes.len();

    ui.horizontal(|ui| {
        // Track columns
        for track_idx in 0..track_count {
            let track_name = if track_idx < tracks_gui.len() {
                tracks_gui[track_idx].name.clone()
            } else {
                format!("Track {}", track_idx + 1)
            };

            ui.vertical(|ui| {
                ui.set_width(120.0);

                // Track Header
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&track_name)
                            .size(12.0)
                            .color(COLOR_TEXT_PRIMARY)
                            .strong(),
                    );
                });

                ui.separator();

                // Clip Slots (1..scene_count)
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

                    let slot_size = vec2(110.0, 36.0);
                    let (rect, response) = ui.allocate_exact_size(slot_size, Sense::click());

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

                ui.add_space(8.0);
                ui.separator();

                // Channel Strip Controls (Volume fader, Pan, Mute, Solo, Meters)
                if track_idx < tracks_gui.len() {
                    let t = &mut tracks_gui[track_idx];

                    // Volume fader
                    let prev_vol = t.volume_linear;
                    let v_slider = ui.add(
                        Slider::new(&mut t.volume_linear, 0.0..=1.2)
                            .text("Vol")
                            .show_value(false),
                    );
                    if v_slider.changed() && t.volume_linear != prev_vol {
                        action.track_volume_changed = Some((track_idx, t.volume_linear));
                    }

                    // Pan slider
                    let prev_pan = t.pan;
                    let p_slider = ui.add(
                        Slider::new(&mut t.pan, -1.0..=1.0)
                            .text("Pan")
                            .show_value(false),
                    );
                    if p_slider.changed() && t.pan != prev_pan {
                        action.track_pan_changed = Some((track_idx, t.pan));
                    }

                    // Mute / Solo buttons
                    ui.horizontal(|ui| {
                        let mute_color = if t.mute { COLOR_RUBY } else { COLOR_TEXT_MUTED };
                        let mute_btn = Button::new(RichText::new("M").size(11.0).color(mute_color))
                            .min_size(vec2(24.0, 20.0));
                        if ui.add(mute_btn).clicked() {
                            t.mute = !t.mute;
                            action.track_mute_toggled = Some(track_idx);
                        }

                        let solo_color = if t.solo { COLOR_GREEN } else { COLOR_TEXT_MUTED };
                        let solo_btn = Button::new(RichText::new("S").size(11.0).color(solo_color))
                            .min_size(vec2(24.0, 20.0));
                        if ui.add(solo_btn).clicked() {
                            t.solo = !t.solo;
                            action.track_solo_toggled = Some(track_idx);
                        }
                    });

                    // Track Stereo Meter
                    let pk_l = if track_idx < 16 { telemetry.track_peaks_l[track_idx] } else { 0.0 };
                    let pk_r = if track_idx < 16 { telemetry.track_peaks_r[track_idx] } else { 0.0 };
                    ui.add(StereoMeterWidget::new(
                        pk_l,
                        pk_r,
                        pk_l * 0.7,
                        pk_r * 0.7,
                        vec2(100.0, 14.0),
                    ));
                }
            });

            ui.separator();
        }

        // Scene trigger buttons column on right
        ui.vertical(|ui| {
            ui.set_width(90.0);
            ui.label(RichText::new("Master").size(12.0).color(COLOR_TEXT_PRIMARY).strong());
            ui.separator();

            for scene_idx in 0..scene_count {
                let scene_btn = Button::new(
                    RichText::new(format!("▶ Scene {}", scene_idx + 1))
                        .size(11.0)
                        .color(COLOR_AMBER),
                )
                .min_size(vec2(80.0, 36.0))
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, COLOR_BORDER));

                if ui.add(scene_btn).clicked() {
                    action.trigger_scene = Some(scene_idx);
                }
            }
        });
    });

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
            painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)));
            painter.circle_filled(rect.center(), 2.0, Color32::from_rgb(0x28, 0x32, 0x48));
        }
        SlotState::Stopped => {
            painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));
            // Stopped play triangle
            painter.text(
                pos2(rect.left() + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "▶",
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_TEXT_MUTED,
            );
            painter.text(
                pos2(rect.left() + 24.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_TEXT_PRIMARY,
            );
        }
        SlotState::QueuedLaunch(_) => {
            // Amber pulsating border
            let pulse = ((current_time * 8.0).sin() * 0.5 + 0.5) as f32;
            let border_color = Color32::from_rgba_premultiplied(
                0xff,
                0xbe,
                0x00,
                (128.0 + pulse * 127.0) as u8,
            );
            painter.rect_stroke(rect, 3.0, Stroke::new(1.5_f32, border_color));

            painter.text(
                pos2(rect.left() + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "⏳",
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_AMBER,
            );
            painter.text(
                pos2(rect.left() + 24.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_AMBER,
            );
        }
        SlotState::Playing { phase, .. } => {
            // Bright green border
            painter.rect_stroke(rect, 3.0, Stroke::new(1.5_f32, COLOR_GREEN));

            // Green progress sweep across the bottom of the slot
            let sweep_w = (rect.width() * (phase.clamp(0.0, 1.0) as f32)).max(2.0);
            let sweep_rect = Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - 3.0),
                pos2(rect.left() + sweep_w, rect.bottom()),
            );
            painter.rect_filled(sweep_rect, 1.0, COLOR_GREEN);

            // Circular progress ring inside icon
            let icon_pos = pos2(rect.left() + 14.0, rect.center().y);
            painter.circle_filled(icon_pos, 5.0, COLOR_GREEN);
            painter.text(
                pos2(rect.left() + 24.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_GREEN,
            );
        }
        SlotState::QueuedStop(_) => {
            painter.rect_stroke(rect, 3.0, Stroke::new(1.5_f32, COLOR_RUBY));
            painter.text(
                pos2(rect.left() + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "■",
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_RUBY,
            );
            painter.text(
                pos2(rect.left() + 24.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                clip_name,
                FontId::new(11.0, FontFamily::Proportional),
                COLOR_TEXT_PRIMARY,
            );
        }
    }
}
