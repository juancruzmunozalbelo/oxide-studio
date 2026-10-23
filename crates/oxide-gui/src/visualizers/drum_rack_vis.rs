//! Drum Rack visualizer: 4x4 interactive 16-pad matrix with pad names, bank scroll (0..7), choke groups, and pad sub-device chain.

use egui::{
    pos2, vec2, Color32, Painter, Rect, Response, Sense, Stroke, Ui, Widget,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone)]
pub struct DrumPadState {
    pub name: String,
    pub note_number: u8, // MIDI 0 .. 127
    pub choke_group: u8, // 0 = none, 1..16
    pub is_muted: bool,
    pub is_solo: bool,
    pub level_meter: f32, // 0.0 .. 1.0
}

#[derive(Debug, Clone)]
pub struct DrumRackVisualizerState {
    pub bank: usize, // 0 .. 7 (Bank C1, C2, etc.)
    pub pads: [DrumPadState; 16],
    pub selected_pad: usize, // 0 .. 15
    pub last_triggered_pad: Option<usize>,
}

impl Default for DrumRackVisualizerState {
    fn default() -> Self {
        let default_names = [
            ("Kick 909", 36, 0),
            ("Rimshot", 37, 0),
            ("Snare 909", 38, 0),
            ("Hand Clap", 39, 0),
            ("Low Tom", 41, 0),
            ("Closed HH", 42, 1), // Choke 1
            ("Mid Tom", 45, 0),
            ("Open HH", 46, 1),   // Choke 1
            ("Hi Tom", 48, 0),
            ("Crash Cym", 49, 0),
            ("Ride Cym", 51, 0),
            ("Tambourine", 54, 0),
            ("Cowbell", 56, 0),
            ("Vibraslap", 58, 0),
            ("High Bongo", 60, 0),
            ("Low Conga", 64, 0),
        ];

        let pads = std::array::from_fn(|i| {
            let (name, note, choke) = default_names[i];
            DrumPadState {
                name: name.to_string(),
                note_number: note,
                choke_group: choke,
                is_muted: false,
                is_solo: false,
                level_meter: if i == 0 || i == 5 { 0.85 } else { 0.0 },
            }
        });

        Self {
            bank: 1, // Bank C1 (36..51)
            pads,
            selected_pad: 0,
            last_triggered_pad: None,
        }
    }
}

pub struct DrumRackVisualizerWidget<'a> {
    state: &'a mut DrumRackVisualizerState,
    size: egui::Vec2,
}

impl<'a> DrumRackVisualizerWidget<'a> {
    pub fn new(state: &'a mut DrumRackVisualizerState, size: egui::Vec2) -> Self {
        Self { state, size }
    }
}

impl<'a> Widget for DrumRackVisualizerWidget<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(self.size, Sense::click());

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                // Check if clicking bank buttons on left
                let bank_w = 40.0;
                if pos.x < rect.left() + bank_w {
                    let bank_h = rect.height() / 8.0;
                    let bank_idx = (((pos.y - rect.top()) / bank_h).floor() as usize).min(7);
                    self.state.bank = bank_idx;
                    // Update pad notes for the bank
                    let base_note = (bank_idx * 16) as u8;
                    for (i, p) in self.state.pads.iter_mut().enumerate() {
                        p.note_number = base_note + i as u8;
                    }
                    response.mark_changed();
                } else {
                    // Check 4x4 pad grid
                    let grid_left = rect.left() + bank_w + 6.0;
                    let grid_w = rect.width() - bank_w - 12.0;
                    let pad_w = grid_w / 4.0;
                    let pad_h = (rect.height() - 20.0) / 4.0;
                    let grid_top = rect.top() + 18.0;

                    if pos.x >= grid_left && pos.y >= grid_top {
                        let col = (((pos.x - grid_left) / pad_w).floor() as usize).min(3);
                        // Bottom row in Ableton Drum Rack is row 0 (pads 0..3)
                        let raw_row = (((pos.y - grid_top) / pad_h).floor() as usize).min(3);
                        let row = 3 - raw_row;
                        let pad_idx = row * 4 + col;
                        if pad_idx < 16 {
                            self.state.selected_pad = pad_idx;
                            self.state.last_triggered_pad = Some(pad_idx);
                            response.mark_changed();
                        }
                    }
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            render_drum_rack_canvas(&painter, rect, self.state);
        }

        response
    }
}

pub fn render_drum_rack_canvas(painter: &Painter, rect: Rect, state: &DrumRackVisualizerState) {
    if rect.width() < 12.0 || rect.height() < 12.0 {
        return;
    }

    painter.rect_filled(rect, 3.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, COLOR_BORDER));

    // 1. Bank Selector Column (8 Banks: C-2 through C5)
    let bank_w = 36.0;
    let bank_rect = Rect::from_min_max(rect.min, pos2(rect.left() + bank_w, rect.bottom()));
    painter.rect_filled(bank_rect, 2.0, Color32::from_rgb(0x11, 0x14, 0x1d));
    painter.line_segment([pos2(bank_rect.right(), rect.top()), pos2(bank_rect.right(), rect.bottom())], Stroke::new(1.0_f32, COLOR_BORDER));

    let bank_names = ["C-2", "C-1", "C0", "C1", "C2", "C3", "C4", "C5"];
    let bh = rect.height() / 8.0;
    for (b_idx, &name) in bank_names.iter().enumerate() {
        let b_box = Rect::from_min_size(pos2(rect.left() + 2.0, rect.top() + b_idx as f32 * bh), vec2(bank_w - 4.0, bh - 1.0));
        let is_active = b_idx == state.bank;
        if is_active {
            painter.rect_filled(b_box, 1.0, COLOR_BG_PANEL_ALT);
            painter.rect_stroke(b_box, 1.0, Stroke::new(1.0_f32, COLOR_AMBER));
        }
        painter.text(
            b_box.center(),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::monospace(8.5),
            if is_active { COLOR_AMBER } else { COLOR_TEXT_MUTED },
        );
    }

    // 2. 4x4 Drum Pad Matrix (Rows 3..0 from top to bottom)
    let grid_left = rect.left() + bank_w + 6.0;
    let grid_top = rect.top() + 20.0;
    let grid_w = rect.width() - bank_w - 12.0;
    let pad_w = grid_w / 4.0;
    let pad_h = (rect.height() - 26.0) / 4.0;

    // Header label
    let sel_p = state.pads.get(state.selected_pad).unwrap_or(&state.pads[0]);
    let sel_pad_idx = if state.selected_pad < state.pads.len() { state.selected_pad + 1 } else { 1 };
    painter.text(
        pos2(grid_left, rect.top() + 4.0),
        egui::Align2::LEFT_TOP,
        format!("Drum Rack Pad #{}: {} (Note: {}, Choke: {})",
            sel_pad_idx, sel_p.name, sel_p.note_number,
            if sel_p.choke_group > 0 { format!("{}", sel_p.choke_group) } else { "None".into() }),
        egui::FontId::monospace(9.5),
        COLOR_AMBER,
    );

    for row in 0..4 {
        // Ableton displays pad 12..15 on top row, 0..3 on bottom row
        let pad_row = 3 - row;
        for col in 0..4 {
            let pad_idx = pad_row * 4 + col;
            let pad = &state.pads[pad_idx];
            let p_rect = Rect::from_min_size(
                pos2(grid_left + col as f32 * pad_w + 2.0, grid_top + row as f32 * pad_h + 2.0),
                vec2(pad_w - 4.0, pad_h - 4.0),
            );

            let is_sel = pad_idx == state.selected_pad;
            let pad_bg = if is_sel {
                COLOR_BG_PANEL_ALT
            } else if pad.choke_group > 0 {
                Color32::from_rgb(0x19, 0x1f, 0x2b)
            } else {
                COLOR_BG_SURFACE
            };

            painter.rect_filled(p_rect, 2.0, pad_bg);
            painter.rect_stroke(
                p_rect,
                2.0,
                Stroke::new(if is_sel { 1.8 } else { 1.0_f32 }, if is_sel { COLOR_AMBER } else { COLOR_BORDER }),
            );

            // Pad Name text
            painter.text(
                pos2(p_rect.left() + 4.0, p_rect.top() + 3.0),
                egui::Align2::LEFT_TOP,
                &pad.name,
                egui::FontId::monospace(8.0),
                if is_sel { COLOR_AMBER } else { COLOR_TEXT_PRIMARY },
            );

            // Choke group badge if any
            if pad.choke_group > 0 {
                painter.text(
                    pos2(p_rect.right() - 4.0, p_rect.top() + 3.0),
                    egui::Align2::RIGHT_TOP,
                    format!("G{}", pad.choke_group),
                    egui::FontId::monospace(7.5),
                    COLOR_CYAN,
                );
            }

            // Note number at bottom right
            painter.text(
                pos2(p_rect.right() - 4.0, p_rect.bottom() - 3.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("{}", pad.note_number),
                egui::FontId::monospace(7.5),
                COLOR_TEXT_MUTED,
            );

            // Pad activity level meter
            if pad.level_meter > 0.05 {
                let meter_w = (p_rect.width() - 8.0) * pad.level_meter.clamp(0.0, 1.0);
                let m_rect = Rect::from_min_size(pos2(p_rect.left() + 4.0, p_rect.bottom() - 5.0), vec2(meter_w, 2.0));
                painter.rect_filled(m_rect, 1.0, COLOR_GREEN);
            }
        }
    }
}
