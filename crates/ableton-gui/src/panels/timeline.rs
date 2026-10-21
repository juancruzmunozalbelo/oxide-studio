//! Linear Arrangement Timeline panel with continuous beat header, zoom, locators,
//! loop brace, waveform & MIDI clip rendering, collapsible automation lanes with bezier curves,
//! and collapsible comping take lanes with micro-crossfades.

use egui::{
    pos2, vec2, Button, Color32, FontFamily, FontId, Rect, RichText, Sense, Slider, Stroke, Ui,
};
use ableton_timeline::{
    ArrangementTimeline, AutomationPoint, ClipPattern,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone)]
pub struct TimelineGuiState {
    pub zoom_pixels_per_beat: f32, // default ~24.0
    pub loop_enabled: bool,
    pub loop_start_beat: f64,
    pub loop_length_beats: f64,
    pub show_automation: bool,
    pub show_comping: bool,
    pub locators: Vec<(f64, String)>, // (beat, name)
    pub automation_points: Vec<AutomationPoint>,
    pub selected_take_region: Option<(usize, f64, f64)>, // (take_idx, start_beat, end_beat)
    pub selected_track: usize,
    pub selected_clip: Option<(usize, usize)>, // (track_idx, clip_idx)
}

impl Default for TimelineGuiState {
    fn default() -> Self {
        Self {
            zoom_pixels_per_beat: 24.0,
            loop_enabled: true,
            loop_start_beat: 0.0,
            loop_length_beats: 16.0,
            show_automation: true,
            show_comping: true,
            locators: vec![
                (0.0, "1. Intro".into()),
                (16.0, "2. Groove".into()),
                (32.0, "3. Build".into()),
                (48.0, "4. Drop".into()),
            ],
            automation_points: vec![
                AutomationPoint::new(0.0, 0.15, 0.0),
                AutomationPoint::new(8.0, 0.35, 0.25),
                AutomationPoint::new(16.0, 0.85, -0.2),
                AutomationPoint::new(24.0, 0.45, 0.1),
                AutomationPoint::new(32.0, 0.95, 0.3),
            ],
            selected_take_region: Some((0, 4.0, 12.0)),
            selected_track: 0,
            selected_clip: None,
        }
    }
}

pub struct TimelineAction {
    pub seek_beat: Option<f64>,
    pub back_to_arrangement: bool,
    pub loop_toggled: bool,
    pub track_selected: Option<usize>,
    pub clip_selected: Option<(usize, usize)>, // (track_idx, clip_idx)
}

pub fn render_arrangement_timeline(
    ui: &mut Ui,
    timeline: &mut ArrangementTimeline,
    gui_state: &mut TimelineGuiState,
    current_beat: f64,
    has_session_override: bool,
) -> TimelineAction {
    let mut action = TimelineAction {
        seek_beat: None,
        back_to_arrangement: false,
        loop_toggled: false,
        track_selected: None,
        clip_selected: None,
    };

    ui.vertical(|ui| {
        // 1. Top timeline toolbar: Back to Arrangement button, Loop brace, Zoom, Lane Toggles
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

            let bta_color = if has_session_override { COLOR_AMBER } else { COLOR_TEXT_MUTED };
            let bta_btn = Button::new(RichText::new("Back to Arrangement").size(10.5).color(bta_color))
                .fill(COLOR_BG_PANEL_ALT)
                .stroke(Stroke::new(1.0_f32, if has_session_override { COLOR_AMBER } else { COLOR_BORDER }));

            if ui.add(bta_btn).clicked() {
                action.back_to_arrangement = true;
            }

            ui.separator();

            // Loop Toggle & Controls
            let loop_col = if gui_state.loop_enabled { COLOR_AMBER } else { COLOR_TEXT_MUTED };
            let loop_btn = Button::new(RichText::new("⇄ Loop").size(10.5).color(loop_col))
                .fill(if gui_state.loop_enabled { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
            if ui.add(loop_btn).clicked() {
                gui_state.loop_enabled = !gui_state.loop_enabled;
                action.loop_toggled = true;
            }

            ui.label(RichText::new(format!("Len: {:.0}b", gui_state.loop_length_beats)).size(10.0).color(COLOR_TEXT_MUTED));

            ui.separator();

            // Automation & Comping Lane Toggles
            let auto_btn = Button::new(RichText::new("Auto Lanes").size(10.0).color(if gui_state.show_automation { COLOR_CYAN } else { COLOR_TEXT_MUTED }))
                .fill(if gui_state.show_automation { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
            if ui.add(auto_btn).clicked() {
                gui_state.show_automation = !gui_state.show_automation;
            }

            let comp_btn = Button::new(RichText::new("Take Lanes").size(10.0).color(if gui_state.show_comping { COLOR_GREEN } else { COLOR_TEXT_MUTED }))
                .fill(if gui_state.show_comping { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
            if ui.add(comp_btn).clicked() {
                gui_state.show_comping = !gui_state.show_comping;
            }

            ui.separator();

            // Zoom slider
            ui.label(RichText::new("Zoom:").size(10.0).color(COLOR_TEXT_MUTED));
            ui.add(Slider::new(&mut gui_state.zoom_pixels_per_beat, 10.0..=60.0).show_value(false));

            // Playhead readout
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Playhead: {:.2} beats", current_beat))
                        .font(FontId::new(11.0, FontFamily::Monospace))
                        .color(COLOR_GREEN),
                );
            });
        });

        ui.add_space(4.0);

        // 2. Timeline Canvas: Ruler + Tracks + Automation Lanes + Take Lanes
        let available_size = vec2(ui.available_width().max(600.0), ui.available_height().max(320.0));
        let (rect, response) = ui.allocate_exact_size(available_size, Sense::click_and_drag());

        let pixels_per_beat = gui_state.zoom_pixels_per_beat;
        let track_header_w = 120.0;
        let track_start_x = rect.left() + track_header_w;
        let ruler_height = 24.0;

        let track_h = 44.0;

        // Scrubbing by clicking ruler
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x >= track_start_x && pos.y <= rect.top() + ruler_height {
                    let clicked_beat = ((pos.x - track_start_x) / pixels_per_beat).max(0.0) as f64;
                    action.seek_beat = Some(clicked_beat);
                }
            }
        }

        // Selection by clicking track lane headers or clips
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.y > rect.top() + ruler_height {
                    let mut check_y = rect.top() + ruler_height;
                    for (t_idx, track) in timeline.tracks.iter().enumerate() {
                        let lane_bottom = check_y + track_h;
                        if pos.y >= check_y && pos.y < lane_bottom {
                            gui_state.selected_track = t_idx;
                            action.track_selected = Some(t_idx);
                            ui.data_mut(|d| d.insert_temp(egui::Id::new("aruston_selected_track"), t_idx));

                            if pos.x >= track_start_x {
                                for (c_idx, clip) in track.clips.iter().enumerate() {
                                    let cx1 = track_start_x + clip.timeline_start_beat as f32 * pixels_per_beat;
                                    let cx2 = track_start_x + (clip.timeline_start_beat + clip.duration_beats) as f32 * pixels_per_beat;
                                    if pos.x >= cx1 && pos.x <= cx2 {
                                        gui_state.selected_clip = Some((t_idx, c_idx));
                                        action.clip_selected = Some((t_idx, c_idx));
                                        break;
                                    }
                                }
                            }
                            break;
                        }
                        check_y += track_h;
                        if gui_state.show_automation && t_idx == 0 {
                            check_y += 32.0;
                        }
                        if gui_state.show_comping && t_idx == 1 {
                            check_y += 56.0;
                        }
                    }
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            // Canvas background
            painter.rect_filled(rect, 2.0, COLOR_BG_VOID);
            painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

            // Ruler Header Background
            let ruler_rect = Rect::from_min_max(
                pos2(rect.left(), rect.top()),
                pos2(rect.right(), rect.top() + ruler_height),
            );
            painter.rect_filled(ruler_rect, 0.0, COLOR_BG_PANEL_ALT);
            painter.line_segment(
                [pos2(rect.left(), rect.top() + ruler_height), pos2(rect.right(), rect.top() + ruler_height)],
                Stroke::new(1.0_f32, COLOR_BORDER),
            );

            // Bar tick lines & Locators across timeline
            let total_bars = ((rect.width() - track_header_w) / (pixels_per_beat * 4.0)).ceil() as usize;
            for bar in 0..=total_bars {
                let beat = bar as f32 * 4.0;
                let x = track_start_x + beat * pixels_per_beat;
                if x <= rect.right() {
                    // Bar tick in ruler
                    painter.line_segment(
                        [pos2(x, rect.top()), pos2(x, rect.bottom())],
                        Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
                    );

                    let bar_label = format!("{}.1", bar + 1);
                    painter.text(
                        pos2(x + 4.0, rect.top() + 6.0),
                        egui::Align2::LEFT_TOP,
                        bar_label,
                        FontId::new(9.5, FontFamily::Monospace),
                        COLOR_TEXT_MUTED,
                    );
                }
            }

            // Locators (e.g. Intro, Groove, Drop)
            for (loc_beat, name) in &gui_state.locators {
                let lx = track_start_x + *loc_beat as f32 * pixels_per_beat;
                if lx <= rect.right() {
                    let flag_rect = Rect::from_min_size(pos2(lx, rect.top() + 2.0), vec2(40.0, 10.0));
                    painter.rect_filled(flag_rect, 1.0, Color32::from_rgb(0x28, 0x32, 0x48));
                    painter.text(
                        flag_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        name,
                        FontId::monospace(7.5),
                        COLOR_CYAN,
                    );
                }
            }

            // Loop Brace in Ruler
            if gui_state.loop_enabled {
                let loop_x1 = track_start_x + gui_state.loop_start_beat as f32 * pixels_per_beat;
                let loop_x2 = track_start_x + (gui_state.loop_start_beat + gui_state.loop_length_beats) as f32 * pixels_per_beat;
                let brace_rect = Rect::from_min_max(
                    pos2(loop_x1, rect.top() + ruler_height - 4.0),
                    pos2(loop_x2.min(rect.right()), rect.top() + ruler_height),
                );
                painter.rect_filled(brace_rect, 0.0, COLOR_AMBER);
            }

            // Track Lanes
            let track_h = 44.0;
            let mut current_y = rect.top() + ruler_height;

            for (t_idx, track) in timeline.tracks.iter().enumerate() {
                if current_y + track_h > rect.bottom() { break; }

                let lane_rect = Rect::from_min_max(
                    pos2(rect.left(), current_y),
                    pos2(rect.right(), current_y + track_h),
                );

                // Track Header (left)
                let header_box = Rect::from_min_max(
                    pos2(lane_rect.left(), lane_rect.top()),
                    pos2(track_start_x, lane_rect.bottom()),
                );
                let is_track_selected = gui_state.selected_track == t_idx;
                if is_track_selected {
                    painter.rect_filled(header_box, 1.0, Color32::from_rgb(0x13, 0x24, 0x36));
                    painter.rect_stroke(header_box, 1.0, Stroke::new(1.8_f32, COLOR_CYAN));
                    let left_bar = Rect::from_min_size(header_box.min, vec2(3.0, header_box.height()));
                    painter.rect_filled(left_bar, 1.0, COLOR_CYAN);
                    painter.text(
                        pos2(header_box.left() + 8.0, header_box.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("● {}", track.name),
                        FontId::monospace(10.0),
                        COLOR_CYAN,
                    );
                } else {
                    painter.rect_filled(header_box, 1.0, COLOR_BG_PANEL_ALT);
                    painter.rect_stroke(header_box, 1.0, Stroke::new(1.0_f32, COLOR_BORDER));
                    painter.text(
                        pos2(header_box.left() + 6.0, header_box.center().y),
                        egui::Align2::LEFT_CENTER,
                        &track.name,
                        FontId::monospace(10.0),
                        COLOR_TEXT_PRIMARY,
                    );
                }

                // Track lane background
                painter.line_segment(
                    [pos2(rect.left(), lane_rect.bottom()), pos2(rect.right(), lane_rect.bottom())],
                    Stroke::new(1.0_f32, COLOR_BORDER),
                );

                // Render Clips on Track
                for (clip_idx, clip) in track.clips.iter().enumerate() {
                    let cx1 = track_start_x + clip.timeline_start_beat as f32 * pixels_per_beat;
                    let cx2 = track_start_x + (clip.timeline_start_beat + clip.duration_beats) as f32 * pixels_per_beat;
                    let c_rect = Rect::from_min_max(
                        pos2(cx1, lane_rect.top() + 3.0),
                        pos2(cx2.min(rect.right()), lane_rect.bottom() - 3.0),
                    );

                    let clip_bg = match &clip.pattern {
                        ClipPattern::Midi(_) => Color32::from_rgb(0x18, 0x2d, 0x3e),
                        ClipPattern::Drum(_) => Color32::from_rgb(0x32, 0x25, 0x18),
                        ClipPattern::Audio(_) => Color32::from_rgb(0x28, 0x1d, 0x36),
                    };

                    painter.rect_filled(c_rect, 2.0, clip_bg);

                    let is_clip_selected = gui_state.selected_clip == Some((t_idx, clip_idx));
                    if is_clip_selected {
                        painter.rect_stroke(c_rect, 2.0, Stroke::new(2.0_f32, COLOR_AMBER));
                        painter.circle_filled(pos2(c_rect.right() - 5.0, c_rect.top() + 5.0), 2.5, COLOR_AMBER);
                    } else {
                        painter.rect_stroke(c_rect, 2.0, Stroke::new(1.0_f32, COLOR_CYAN));
                    }

                    // Clip Name label
                    painter.text(
                        pos2(c_rect.left() + 4.0, c_rect.top() + 3.0),
                        egui::Align2::LEFT_TOP,
                        &clip.name,
                        FontId::monospace(8.5),
                        COLOR_AMBER,
                    );

                    // Waveform / MIDI note preview inside clip
                    match &clip.pattern {
                        ClipPattern::Midi(pat) => {
                            for n in &pat.notes {
                                let nx = c_rect.left() + (n.start_beat as f32 / clip.duration_beats as f32) * c_rect.width();
                                let nw = (n.duration_beats as f32 / clip.duration_beats as f32) * c_rect.width();
                                let ny = c_rect.bottom() - 6.0 - ((n.pitch % 24) as f32 / 24.0) * (c_rect.height() - 14.0);
                                let note_rect = Rect::from_min_size(pos2(nx, ny), vec2(nw.max(3.0), 3.0));
                                painter.rect_filled(note_rect, 1.0, COLOR_CYAN);
                            }
                        }
                        _ => {
                            // Audio waveform peaks representation
                            let mid = c_rect.center().y;
                            let steps = 16;
                            for s in 0..steps {
                                let sx = c_rect.left() + (s as f32 / steps as f32) * c_rect.width();
                                let h = (s as f32 * 1.5).sin().abs() * 8.0;
                                painter.line_segment([pos2(sx, mid - h), pos2(sx, mid + h)], Stroke::new(1.0_f32, COLOR_AMBER));
                            }
                        }
                    }
                }

                current_y += track_h;

                // Collapsible Automation Sub-Lane
                if gui_state.show_automation && t_idx == 0 {
                    let auto_h = 32.0;
                    let a_rect = Rect::from_min_max(pos2(rect.left(), current_y), pos2(rect.right(), current_y + auto_h));
                    painter.rect_filled(a_rect, 0.0, Color32::from_rgb(0x0e, 0x12, 0x1a));
                    painter.line_segment([pos2(rect.left(), a_rect.bottom()), pos2(rect.right(), a_rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0x1a, 0x20, 0x2c)));

                    // Header on left
                    painter.text(
                        pos2(a_rect.left() + 8.0, a_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "Cutoff [Auto]",
                        FontId::monospace(8.5),
                        COLOR_CYAN,
                    );

                    // Continuous Bezier / Power Curve Breakpoints
                    let pts = &gui_state.automation_points;
                    let mut curve_lines = Vec::new();
                    for p in pts {
                        let px = track_start_x + p.beat as f32 * pixels_per_beat;
                        let py = a_rect.bottom() - 4.0 - p.value_norm * (auto_h - 8.0);
                        let pt_pos = pos2(px, py);
                        curve_lines.push(pt_pos);

                        // Draggable Breakpoint Node
                        painter.circle_filled(pt_pos, 3.0, COLOR_AMBER);
                        painter.circle_stroke(pt_pos, 4.0, Stroke::new(1.0_f32, Color32::WHITE));
                    }
                    if curve_lines.len() >= 2 {
                        painter.add(egui::Shape::line(curve_lines, Stroke::new(1.5_f32, COLOR_CYAN)));
                    }

                    current_y += auto_h;
                }

                // Collapsible Comping Take Lanes
                if gui_state.show_comping && t_idx == 1 {
                    let take_h = 24.0;
                    for take_idx in 0..2 {
                        let take_rect = Rect::from_min_max(pos2(rect.left(), current_y), pos2(rect.right(), current_y + take_h));
                        painter.rect_filled(take_rect, 0.0, Color32::from_rgb(0x10, 0x14, 0x1e));
                        painter.line_segment([pos2(rect.left(), take_rect.bottom()), pos2(rect.right(), take_rect.bottom())], Stroke::new(1.0_f32, COLOR_BORDER));

                        painter.text(
                            pos2(take_rect.left() + 8.0, take_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("Take {}", take_idx + 1),
                            FontId::monospace(8.0),
                            COLOR_GREEN,
                        );

                        // Region comp selection highlight & micro-crossfade markers
                        if let Some((sel_take, s_beat, e_beat)) = gui_state.selected_take_region {
                            if sel_take == take_idx {
                                let rx1 = track_start_x + s_beat as f32 * pixels_per_beat;
                                let rx2 = track_start_x + e_beat as f32 * pixels_per_beat;
                                let comp_box = Rect::from_min_max(pos2(rx1, take_rect.top() + 2.0), pos2(rx2, take_rect.bottom() - 2.0));
                                painter.rect_filled(comp_box, 1.0, Color32::from_rgba_unmultiplied(0, 230, 118, 40));
                                painter.rect_stroke(comp_box, 1.0, Stroke::new(1.0_f32, COLOR_GREEN));

                                // Micro-crossfade tick markers at splice bounds
                                painter.line_segment([pos2(rx1, take_rect.top()), pos2(rx1, take_rect.bottom())], Stroke::new(1.5_f32, COLOR_AMBER));
                                painter.line_segment([pos2(rx2, take_rect.top()), pos2(rx2, take_rect.bottom())], Stroke::new(1.5_f32, COLOR_AMBER));
                            }
                        }

                        current_y += take_h;
                    }
                }
            }

            // Real-Time Playhead Line across all tracks
            let playhead_x = track_start_x + current_beat as f32 * pixels_per_beat;
            if playhead_x >= track_start_x && playhead_x <= rect.right() {
                painter.line_segment(
                    [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
                    Stroke::new(2.0_f32, COLOR_GREEN),
                );
            }
        }
    });

    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use ableton_timeline::{ArrangementClip, ArrangementTrack, ClipPattern, MidiPattern};

    #[test]
    fn test_timeline_gui_state_selection_defaults() {
        let state = TimelineGuiState::default();
        assert_eq!(state.selected_track, 0);
        assert!(state.selected_clip.is_none());
    }

    #[test]
    fn test_timeline_render_with_track_and_clip_selection() {
        let ctx = egui::Context::default();
        let mut timeline = ArrangementTimeline::new();
        let mut trk = ArrangementTrack::new(1, "Test Track", "MidiSynth");
        trk.add_clip(ArrangementClip::new(1, "Clip 1", 0.0, 16.0, ClipPattern::Midi(MidiPattern::new(16.0))));
        timeline.tracks.push(trk);

        let mut gui_state = TimelineGuiState::default();
        gui_state.selected_track = 0;
        gui_state.selected_clip = Some((0, 0));

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let action = render_arrangement_timeline(
                    ui,
                    &mut timeline,
                    &mut gui_state,
                    0.0,
                    false,
                );
                assert!(!action.back_to_arrangement);
            });
        });
    }
}
