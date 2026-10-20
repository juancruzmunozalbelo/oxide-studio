//! 16-step drum sequencer and chromatic piano roll editor.

use egui::{
    pos2, vec2, Button, Color32, FontFamily, FontId, Rect, RichText, Sense, Stroke, Ui,
};
use ableton_timeline::{DrumStepPattern, MidiPattern};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

pub struct SequencerAction {
    pub step_toggled: Option<(usize, usize)>, // (pad_idx, step_idx)
    pub pad_triggered: Option<u8>,           // pad_idx 0..15 (MIDI 36..51)
}

pub fn render_drum_sequencer(
    ui: &mut Ui,
    drum_pattern: &mut DrumStepPattern,
    current_step: usize,
) -> SequencerAction {
    let mut action = SequencerAction {
        step_toggled: None,
        pad_triggered: None,
    };

    let pad_names = [
        "Kick 909", "Rimshot", "Snare 909", "Hand Clap",
        "Low Tom", "Closed HH", "Mid Tom", "Open HH",
        "Hi Tom", "Crash Cym", "Ride Cym", "Perc 1",
        "Perc 2", "Perc 3", "Perc 4", "Perc 5",
    ];

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("16-Step 909 Drum Sequencer").size(12.0).color(COLOR_AMBER).strong());
            ui.label(
                RichText::new(format!("Step: {} / 16", current_step + 1))
                    .font(FontId::new(11.0, FontFamily::Monospace))
                    .color(COLOR_GREEN),
            );
        });

        ui.separator();

        let visible_pads = 4.min(drum_pattern.tracks.len());

        for pad_idx in 0..visible_pads {
            ui.horizontal(|ui| {
                // Pad audition button
                let pad_name = pad_names.get(pad_idx).unwrap_or(&"Pad");
                let pad_btn = Button::new(RichText::new(*pad_name).size(10.0).color(COLOR_TEXT_PRIMARY))
                    .min_size(vec2(80.0, 22.0))
                    .fill(COLOR_BG_PANEL_ALT)
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER));

                if ui.add(pad_btn).clicked() {
                    action.pad_triggered = Some(pad_idx as u8);
                }

                // 16 Step Buttons
                if let Some(track) = drum_pattern.tracks.get_mut(pad_idx) {
                    for step_idx in 0..16 {
                        let step = track.steps.get_mut(step_idx);
                        let is_active = step.as_ref().map(|s| s.active).unwrap_or(false);
                        let is_accent = step.as_ref().map(|s| s.accent).unwrap_or(false);
                        let is_current = step_idx == current_step;

                        let step_size = vec2(20.0, 22.0);
                        let (rect, response) = ui.allocate_exact_size(step_size, Sense::click());

                        if ui.is_rect_visible(rect) {
                            let painter = ui.painter_at(rect);

                            // Background
                            let bg_color = if is_active {
                                if is_accent { COLOR_AMBER } else { COLOR_CYAN }
                            } else if (step_idx / 4) % 2 == 0 {
                                COLOR_BG_SURFACE
                            } else {
                                Color32::from_rgb(0x15, 0x1a, 0x24)
                            };
                            painter.rect_filled(rect, 2.0, bg_color);

                            // Border / current playhead indicator
                            let border_color = if is_current {
                                COLOR_GREEN
                            } else if response.hovered() {
                                Color32::WHITE
                            } else {
                                COLOR_BORDER
                            };
                            let stroke_w = if is_current { 1.5 } else { 1.0_f32 };
                            painter.rect_stroke(rect, 2.0, Stroke::new(stroke_w, border_color));

                            // Step number label if inactive
                            if !is_active {
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    format!("{}", step_idx + 1),
                                    FontId::new(9.0, FontFamily::Monospace),
                                    COLOR_TEXT_MUTED,
                                );
                            }
                        }

                        if response.clicked() {
                            if let Some(s) = step {
                                s.active = !s.active;
                                action.step_toggled = Some((pad_idx, step_idx));
                            }
                        }
                    }
                }
            });
        }
    });

    action
}

pub fn render_piano_roll(
    ui: &mut Ui,
    midi_pattern: &mut MidiPattern,
    current_beat: f64,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Chromatic Piano Roll").size(12.0).color(COLOR_CYAN).strong());
            ui.label(
                RichText::new(format!("Notes: {}", midi_pattern.notes.len()))
                    .font(FontId::new(11.0, FontFamily::Monospace))
                    .color(COLOR_TEXT_MUTED),
            );
        });

        ui.separator();

        let available_size = vec2(ui.available_width().max(400.0), 120.0);
        let (rect, _) = ui.allocate_exact_size(available_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, 2.0, COLOR_BG_VOID);
            painter.rect_stroke(rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

            // Grid lines for 16 beats
            let beats_display = 16.0f32;
            let pixels_per_beat = (rect.width() - 40.0) / beats_display;
            let grid_start_x = rect.left() + 40.0;

            for b in 0..=16 {
                let x = grid_start_x + b as f32 * pixels_per_beat;
                painter.line_segment(
                    [pos2(x, rect.top()), pos2(x, rect.bottom())],
                    Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
                );
            }

            // Draw note rectangles
            for note in &midi_pattern.notes {
                let nx = grid_start_x + (note.start_beat as f32) * pixels_per_beat;
                let nw = ((note.duration_beats as f32) * pixels_per_beat).max(4.0);
                let semitone_y = 127 - note.pitch.clamp(48, 84);
                let ny = rect.top() + (semitone_y as f32 / 36.0) * rect.height();

                let note_rect = Rect::from_min_max(
                    pos2(nx, ny.clamp(rect.top() + 2.0, rect.bottom() - 10.0)),
                    pos2(nx + nw, (ny + 8.0).clamp(rect.top() + 10.0, rect.bottom() - 2.0)),
                );
                painter.rect_filled(note_rect, 2.0, COLOR_CYAN);
                painter.rect_stroke(note_rect, 2.0, Stroke::new(1.0_f32, Color32::WHITE));
            }

            // Playhead
            let playhead_x = grid_start_x + (current_beat as f32) * pixels_per_beat;
            if playhead_x >= grid_start_x && playhead_x <= rect.right() {
                painter.line_segment(
                    [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
                    Stroke::new(1.5_f32, COLOR_GREEN),
                );
            }
        }
    });
}
