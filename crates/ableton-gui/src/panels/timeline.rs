//! Linear Arrangement Timeline panel with continuous beat header, clips, automation, and playhead.

use egui::{
    pos2, vec2, Button, Color32, FontFamily, FontId, Rect, RichText, Sense, Stroke, Ui,
};
use ableton_timeline::ArrangementTimeline;
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_GREEN, COLOR_LAVENDER, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

pub struct TimelineAction {
    pub seek_beat: Option<f64>,
    pub back_to_arrangement: bool,
}

pub fn render_arrangement_timeline(
    ui: &mut Ui,
    timeline: &mut ArrangementTimeline,
    current_beat: f64,
    has_session_override: bool,
) -> TimelineAction {
    let mut action = TimelineAction {
        seek_beat: None,
        back_to_arrangement: false,
    };

    ui.vertical(|ui| {
        // Top timeline toolbar: Back to Arrangement button & Zoom
        ui.horizontal(|ui| {
            let bta_color = if has_session_override { COLOR_AMBER } else { COLOR_TEXT_MUTED };
            let bta_btn = Button::new(
                RichText::new("Back to Arrangement")
                    .size(11.0)
                    .color(bta_color),
            )
            .fill(COLOR_BG_PANEL_ALT)
            .stroke(Stroke::new(1.0_f32, if has_session_override { COLOR_AMBER } else { COLOR_BORDER }));

            if ui.add(bta_btn).clicked() {
                action.back_to_arrangement = true;
            }

            ui.label(
                RichText::new(format!("Playhead: {:.2} beats", current_beat))
                    .font(FontId::new(11.0, FontFamily::Monospace))
                    .color(COLOR_GREEN),
            );
        });

        ui.add_space(4.0);

        // Timeline Canvas: Header (bars) + Track lanes + Playhead
        let available_size = vec2(ui.available_width().max(400.0), ui.available_height().max(180.0));
        let (rect, response) = ui.allocate_exact_size(available_size, Sense::click_and_drag());

        let total_beats_display = 32.0; // 8 bars (4 beats each)
        let pixels_per_beat = (rect.width() - 100.0) / total_beats_display;
        let track_start_x = rect.left() + 100.0;
        let header_height = 24.0;

        // Seeking by clicking header
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if pos.x >= track_start_x && pos.y <= rect.top() + header_height {
                    let clicked_beat = ((pos.x - track_start_x) / pixels_per_beat).max(0.0) as f64;
                    action.seek_beat = Some(clicked_beat);
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            // 1. Background
            painter.rect_filled(rect, 2.0, COLOR_BG_VOID);
            painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

            // 2. Timeline Header (Bars 1..9)
            let header_rect = Rect::from_min_max(
                pos2(rect.left(), rect.top()),
                pos2(rect.right(), rect.top() + header_height),
            );
            painter.rect_filled(header_rect, 0.0, COLOR_BG_PANEL_ALT);
            painter.line_segment(
                [pos2(rect.left(), rect.top() + header_height), pos2(rect.right(), rect.top() + header_height)],
                Stroke::new(1.0_f32, COLOR_BORDER),
            );

            // Bar tick lines
            for bar in 0..=8 {
                let beat = bar as f32 * 4.0;
                let x = track_start_x + beat * pixels_per_beat;
                if x <= rect.right() {
                    painter.line_segment(
                        [pos2(x, rect.top()), pos2(x, rect.bottom())],
                        Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
                    );

                    let bar_label = format!("{}.1", bar + 1);
                    painter.text(
                        pos2(x + 4.0, rect.top() + 12.0),
                        egui::Align2::LEFT_CENTER,
                        bar_label,
                        FontId::new(10.0, FontFamily::Monospace),
                        COLOR_TEXT_MUTED,
                    );
                }
            }

            // 3. Track Rows & Clips
            let track_height = 42.0;
            let tracks = &timeline.tracks;
            let row_count = tracks.len().max(3);

            for row_idx in 0..row_count {
                let row_top = rect.top() + header_height + row_idx as f32 * track_height;
                let row_bottom = row_top + track_height;

                if row_bottom > rect.bottom() {
                    break;
                }

                // Track row divider
                painter.line_segment(
                    [pos2(rect.left(), row_bottom), pos2(rect.right(), row_bottom)],
                    Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
                );

                // Track Name Label Header on Left
                let track_name = tracks.get(row_idx).map(|t| t.name.as_str()).unwrap_or_else(|| match row_idx {
                    0 => "Synth Lead",
                    1 => "909 Drums",
                    _ => "Sub Bass",
                });
                painter.text(
                    pos2(rect.left() + 8.0, row_top + track_height * 0.5),
                    egui::Align2::LEFT_CENTER,
                    track_name,
                    FontId::new(11.0, FontFamily::Proportional),
                    COLOR_TEXT_PRIMARY,
                );

                // Render Clips
                if let Some(track) = tracks.get(row_idx) {
                    for clip in &track.clips {
                        let clip_start_x = track_start_x + (clip.timeline_start_beat as f32) * pixels_per_beat;
                        let clip_width = (clip.duration_beats as f32) * pixels_per_beat;
                        let clip_rect = Rect::from_min_max(
                            pos2(clip_start_x, row_top + 4.0),
                            pos2((clip_start_x + clip_width).min(rect.right() - 2.0), row_bottom - 4.0),
                        );

                        let clip_color = match row_idx {
                            0 => Color32::from_rgb(0x28, 0x48, 0x64),
                            1 => Color32::from_rgb(0x54, 0x38, 0x28),
                            _ => Color32::from_rgb(0x38, 0x28, 0x54),
                        };
                        painter.rect_filled(clip_rect, 3.0, clip_color);
                        painter.rect_stroke(clip_rect, 3.0, Stroke::new(1.0_f32, COLOR_CYAN));

                        painter.text(
                            pos2(clip_rect.left() + 6.0, clip_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &clip.name,
                            FontId::new(10.0, FontFamily::Proportional),
                            Color32::WHITE,
                        );
                    }
                } else {
                    // Default mockup clips if empty
                    let clip_start_x = track_start_x + (row_idx as f32 * 4.0) * pixels_per_beat;
                    let clip_width = 8.0 * pixels_per_beat;
                    let clip_rect = Rect::from_min_max(
                        pos2(clip_start_x, row_top + 4.0),
                        pos2((clip_start_x + clip_width).min(rect.right() - 2.0), row_bottom - 4.0),
                    );
                    painter.rect_filled(clip_rect, 3.0, Color32::from_rgb(0x1c, 0x28, 0x38));
                    painter.rect_stroke(clip_rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));
                }
            }

            // 4. Automation Lane overlay (Cutoff Automation)
            let auto_top = rect.top() + header_height + track_height * 2.0;
            let auto_h = track_height * 0.8;
            let auto_points = [
                (0.0f32, 0.2f32),
                (4.0, 0.4),
                (8.0, 0.85),
                (12.0, 0.3),
                (16.0, 0.95),
            ];
            let mut line_pts = Vec::new();
            for (beat, val) in auto_points {
                let px = track_start_x + beat * pixels_per_beat;
                let py = auto_top + (1.0 - val) * auto_h;
                line_pts.push(pos2(px, py));
                painter.circle_filled(pos2(px, py), 2.5, COLOR_LAVENDER);
            }
            if line_pts.len() >= 2 {
                painter.add(egui::Shape::line(line_pts, Stroke::new(1.5_f32, COLOR_LAVENDER)));
            }

            // 5. Playhead cursor line (Phosphor Green vertical line)
            let playhead_x = track_start_x + (current_beat as f32) * pixels_per_beat;
            if playhead_x >= track_start_x && playhead_x <= rect.right() {
                // Glow line
                painter.line_segment(
                    [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
                    Stroke::new(3.0_f32, Color32::from_rgba_premultiplied(0x00, 0xe6, 0x76, 0x60)),
                );
                // Core line
                painter.line_segment(
                    [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
                    Stroke::new(1.5_f32, COLOR_GREEN),
                );
                // Head triangle
                let head_pts = vec![
                    pos2(playhead_x - 5.0, rect.top()),
                    pos2(playhead_x + 5.0, rect.top()),
                    pos2(playhead_x, rect.top() + 8.0),
                ];
                painter.add(egui::Shape::convex_polygon(head_pts, COLOR_GREEN, Stroke::NONE));
            }
        }
    });

    action
}
