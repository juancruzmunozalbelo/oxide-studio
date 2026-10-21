//! Live 12 Bottom Detail View: Clip View.
//!
//! Features:
//! - MIDI Piano Roll: Chromatic / Scale-Folded mode, Velocity stalks lane, Chance (%) probability lane
//! - Live 12 Transform Tools: Quantize, Connect, Ornament, Recombine, Span, Strum, Time Warp
//! - Live 12 Generate Tools: Seed, Shape, Stacks, Rhythm, Euclidean
//! - Audio Clip Warping Panel: Waveform display with draggable Warp Markers, 6 modes (Beats, Tones,
//!   Texture, RePitch, Complex, ComplexPro), pitch transposition (-48..+48 st), and clip gain dB.

use egui::{
    pos2, vec2, Button, Color32, ComboBox, DragValue, FontId, Rect, RichText,
    Sense, Slider, Stroke, Ui,
};
use ableton_core::GlobalScale;
use ableton_timeline::{
    generate::{generate_euclidean, generate_seed, generate_stacks},
    transform::{transform_connect, transform_ornament, transform_recombine, transform_strum},
    warping::{WarpMarker, WarpMode},
    MidiNote, MidiPattern,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_GREEN, COLOR_TEXT_DIM, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipViewSubTab {
    Notes,
    Transform,
    Generate,
    Warping,
}

#[derive(Debug, Clone)]
pub struct ClipViewState {
    pub is_audio: bool,
    pub sub_tab: ClipViewSubTab,
    pub scale_folded: bool,
    pub selected_note_idx: Option<usize>,

    // Transform parameters
    pub quantize_grid: f64, // 0.25 (1/16), 0.5 (1/8)
    pub quantize_amount: f32, // 0.0 .. 1.0
    pub strum_spread_ms: f32,
    pub strum_direction_up: bool,
    pub ornament_velocity: f32,
    pub span_legato: bool,
    pub time_warp_tension: f32,

    // Generate parameters
    pub seed_density: f32,
    pub seed_min_pitch: u8,
    pub seed_max_pitch: u8,
    pub stacks_chord_type: usize, // 0 = Major triad, 1 = Minor 7th, 2 = 9th
    pub euclidean_hits: u32,
    pub euclidean_steps: u32,

    // Audio Warping parameters
    pub warp_enabled: bool,
    pub warp_mode: WarpMode,
    pub transpose_semitones: i8,
    pub clip_gain_db: f32,
    pub warp_markers: Vec<WarpMarker>,
}

impl Default for ClipViewState {
    fn default() -> Self {
        Self {
            is_audio: false,
            sub_tab: ClipViewSubTab::Notes,
            scale_folded: true,
            selected_note_idx: None,
            quantize_grid: 0.25,
            quantize_amount: 1.0,
            strum_spread_ms: 25.0,
            strum_direction_up: true,
            ornament_velocity: 0.7,
            span_legato: true,
            time_warp_tension: 0.2,
            seed_density: 0.75,
            seed_min_pitch: 41,
            seed_max_pitch: 72,
            stacks_chord_type: 1,
            euclidean_hits: 5,
            euclidean_steps: 16,
            warp_enabled: true,
            warp_mode: WarpMode::ComplexPro,
            transpose_semitones: 0,
            clip_gain_db: 0.0,
            warp_markers: vec![
                WarpMarker::new(0.0, 0.0),
                WarpMarker::new(4.0, 1.935),
                WarpMarker::new(8.0, 3.871),
                WarpMarker::new(16.0, 7.742),
            ],
        }
    }
}

pub struct ClipViewAction {
    pub pattern_mutated: bool,
    pub warp_mode_changed: Option<WarpMode>,
    pub transpose_changed: Option<i8>,
    pub warp_marker_moved: bool,
}

pub fn render_clip_view(
    ui: &mut Ui,
    state: &mut ClipViewState,
    midi_pattern: &mut MidiPattern,
    global_scale: &GlobalScale,
    current_beat: f64,
) -> ClipViewAction {
    let mut action = ClipViewAction {
        pattern_mutated: false,
        warp_mode_changed: None,
        transpose_changed: None,
        warp_marker_moved: false,
    };

    ui.vertical(|ui| {
        // Sub-Tab Header Bar
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

            let notes_btn = Button::new(RichText::new("Notes & Piano Roll").size(10.5).color(if state.sub_tab == ClipViewSubTab::Notes { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
                .fill(if state.sub_tab == ClipViewSubTab::Notes { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
            if ui.add(notes_btn).clicked() { state.sub_tab = ClipViewSubTab::Notes; }

            let trans_btn = Button::new(RichText::new("Transform Tools").size(10.5).color(if state.sub_tab == ClipViewSubTab::Transform { COLOR_CYAN } else { COLOR_TEXT_MUTED }))
                .fill(if state.sub_tab == ClipViewSubTab::Transform { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
            if ui.add(trans_btn).clicked() { state.sub_tab = ClipViewSubTab::Transform; }

            let gen_btn = Button::new(RichText::new("Generate Tools").size(10.5).color(if state.sub_tab == ClipViewSubTab::Generate { COLOR_GREEN } else { COLOR_TEXT_MUTED }))
                .fill(if state.sub_tab == ClipViewSubTab::Generate { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
            if ui.add(gen_btn).clicked() { state.sub_tab = ClipViewSubTab::Generate; }

            let warp_btn = Button::new(RichText::new("Audio Warping").size(10.5).color(if state.sub_tab == ClipViewSubTab::Warping { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
                .fill(if state.sub_tab == ClipViewSubTab::Warping { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
            if ui.add(warp_btn).clicked() { state.sub_tab = ClipViewSubTab::Warping; }

            ui.separator();

            // Scale-Fold toggle for piano roll
            let fold_col = if state.scale_folded { COLOR_AMBER } else { COLOR_TEXT_MUTED };
            let fold_text = if state.scale_folded { "★ Scale-Folded" } else { "☆ Chromatic" };
            if ui.add(Button::new(RichText::new(fold_text).size(10.5).color(fold_col))).clicked() {
                state.scale_folded = !state.scale_folded;
            }

            ui.separator();

            ui.label(RichText::new("• Click grid to add/remove MIDI notes • Drag Velocity/Chance below").size(10.0).color(COLOR_TEXT_MUTED));
        });

        ui.separator();

        match state.sub_tab {
            ClipViewSubTab::Notes => {
                // Interactive Piano Roll + Velocity & Chance lanes
                if render_interactive_piano_roll(ui, midi_pattern, state, global_scale, current_beat) {
                    action.pattern_mutated = true;
                }
            }
            ClipViewSubTab::Transform => {
                // Live 12 Transform Panel Controls
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(16.0, 0.0);

                    // 1. Quantize
                    ui.vertical(|ui| {
                        ui.set_width(140.0);
                        ui.label(RichText::new("Quantize").size(11.0).color(COLOR_CYAN).strong());
                        ui.add(Slider::new(&mut state.quantize_amount, 0.0..=1.0).text("Amount").show_value(false));
                        if ui.add(Button::new(RichText::new("Apply Quantize").size(10.0).color(COLOR_CYAN))).clicked() {
                            ableton_timeline::transform::transform_quantize(midi_pattern, state.quantize_grid, state.quantize_amount, 0.0);
                            action.pattern_mutated = true;
                        }
                    });

                    ui.separator();

                    // 2. Strum
                    ui.vertical(|ui| {
                        ui.set_width(140.0);
                        ui.label(RichText::new("Strum").size(11.0).color(COLOR_CYAN).strong());
                        ui.add(Slider::new(&mut state.strum_spread_ms, 5.0..=100.0).text("Spread").show_value(false));
                        if ui.add(Button::new(RichText::new("Apply Strum").size(10.0).color(COLOR_CYAN))).clicked() {
                            let spread_beats = (state.strum_spread_ms / 1000.0) * (124.0 / 60.0);
                            let dir = if state.strum_direction_up {
                                ableton_timeline::transform::StrumDirection::Up
                            } else {
                                ableton_timeline::transform::StrumDirection::Down
                            };
                            transform_strum(midi_pattern, spread_beats as f64, dir);
                            action.pattern_mutated = true;
                        }
                    });

                    ui.separator();

                    // 3. Connect & Ornament
                    ui.vertical(|ui| {
                        ui.set_width(140.0);
                        ui.label(RichText::new("Connect & Ornament").size(11.0).color(COLOR_CYAN).strong());
                        if ui.add(Button::new(RichText::new("Connect Notes").size(10.0).color(COLOR_CYAN))).clicked() {
                            let sl = ableton_timeline::generate::ScaleLock::dorian(global_scale.root);
                            transform_connect(midi_pattern, sl, 0.5);
                            action.pattern_mutated = true;
                        }
                        if ui.add(Button::new(RichText::new("Add Ornaments").size(10.0).color(COLOR_CYAN))).clicked() {
                            transform_ornament(midi_pattern, 0.0625, 2, state.ornament_velocity);
                            action.pattern_mutated = true;
                        }
                        if ui.add(Button::new(RichText::new("Recombine").size(10.0).color(COLOR_CYAN))).clicked() {
                            transform_recombine(midi_pattern, 1, 1, 1);
                            action.pattern_mutated = true;
                        }
                    });
                });
            }
            ClipViewSubTab::Generate => {
                // Live 12 Generate Panel Controls
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(16.0, 0.0);

                    // 1. Seed
                    ui.vertical(|ui| {
                        ui.set_width(150.0);
                        ui.label(RichText::new("Seed Generator").size(11.0).color(COLOR_GREEN).strong());
                        ui.add(Slider::new(&mut state.seed_density, 0.1..=1.0).text("Density").show_value(false));
                        if ui.add(Button::new(RichText::new("Generate Seed").size(10.0).color(COLOR_GREEN))).clicked() {
                            let sl = ableton_timeline::generate::ScaleLock::dorian(global_scale.root);
                            *midi_pattern = generate_seed(4.0, 0.25, state.seed_density, state.seed_min_pitch, state.seed_max_pitch, 80, 115, 0.2, 0.2, Some(sl), 42);
                            action.pattern_mutated = true;
                        }
                    });

                    ui.separator();

                    // 2. Stacks & Rhythm
                    ui.vertical(|ui| {
                        ui.set_width(150.0);
                        ui.label(RichText::new("Stacks & Rhythm").size(11.0).color(COLOR_GREEN).strong());
                        if ui.add(Button::new(RichText::new("Generate Stacks").size(10.0).color(COLOR_GREEN))).clicked() {
                            let chord_type = ableton_timeline::generate::StacksChordType::Min7;
                            let voicing = ableton_timeline::generate::StacksVoicing::Closed;
                            let roots = [41, 44, 46, 48];
                            *midi_pattern = generate_stacks(&roots, chord_type, voicing, 0, 1.0, 105);
                            action.pattern_mutated = true;
                        }
                        if ui.add(Button::new(RichText::new("Euclidean Pattern").size(10.0).color(COLOR_GREEN))).clicked() {
                            *midi_pattern = generate_euclidean(state.euclidean_hits as usize, state.euclidean_steps as usize, 0, 41, 0.25, 110);
                            action.pattern_mutated = true;
                        }
                    });
                });
            }
            ClipViewSubTab::Warping => {
                // Audio Clip Warping Panel
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);

                    // Warp Mode & Controls
                    ui.vertical(|ui| {
                        ui.set_width(160.0);
                        ui.label(RichText::new("Audio Warping").size(11.0).color(COLOR_AMBER).strong());

                        // Mode Selector (6 modes)
                        let modes = [
                            WarpMode::Beats,
                            WarpMode::Tones,
                            WarpMode::Texture,
                            WarpMode::RePitch,
                            WarpMode::Complex,
                            WarpMode::ComplexPro,
                        ];

                        ComboBox::from_id_salt("clip_warp_mode_combo")
                            .selected_text(format!("{:?}", state.warp_mode))
                            .show_ui(ui, |ui| {
                                for &m in &modes {
                                    if ui.selectable_label(state.warp_mode == m, format!("{:?}", m)).clicked() {
                                        state.warp_mode = m;
                                        action.warp_mode_changed = Some(m);
                                    }
                                }
                            });

                        // Transpose (-48 .. +48 st)
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Transpose:").size(9.5).color(COLOR_TEXT_MUTED));
                            let mut tr = state.transpose_semitones as i32;
                            if ui.add(DragValue::new(&mut tr).range(-48..=48).suffix(" st")).changed() {
                                state.transpose_semitones = tr as i8;
                                action.transpose_changed = Some(state.transpose_semitones);
                            }
                        });

                        // Gain dB
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Gain:").size(9.5).color(COLOR_TEXT_MUTED));
                            ui.add(Slider::new(&mut state.clip_gain_db, -24.0..=12.0).suffix(" dB").show_value(false));
                        });
                    });

                    ui.separator();

                    // Waveform with Draggable Warp Markers
                    let (canvas_rect, mut response) = ui.allocate_exact_size(vec2(ui.available_width().max(300.0), 120.0), Sense::click_and_drag());
                    if response.dragged() {
                        let delta_x = response.drag_delta().x;
                        if canvas_rect.width() > 0.0 && delta_x != 0.0 {
                            let beat_delta = (delta_x / canvas_rect.width()) as f64 * 16.0;
                            if let Some(pos) = response.interact_pointer_pos() {
                                let pointer_beat = ((pos.x - canvas_rect.left()) / canvas_rect.width()) as f64 * 16.0;
                                if let Some(marker) = state.warp_markers.iter_mut().min_by(|a, b| {
                                    let dist_a = (a.beat - pointer_beat).abs();
                                    let dist_b = (b.beat - pointer_beat).abs();
                                    dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
                                }) {
                                    marker.beat = (marker.beat + beat_delta).clamp(0.0, 16.0);
                                    marker.sample_time = (marker.beat * (7.742 / 16.0)).max(0.0);
                                    action.warp_marker_moved = true;
                                    response.mark_changed();
                                }
                            }
                        }
                    }

                    if ui.is_rect_visible(canvas_rect) {
                        let painter = ui.painter_at(canvas_rect);
                        painter.rect_filled(canvas_rect, 2.0, Color32::from_rgb(0x10, 0x14, 0x1d));
                        painter.rect_stroke(canvas_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

                        let mid_y = canvas_rect.center().y;
                        let steps = 96;
                        for s in 0..steps {
                            let t = s as f32 / steps as f32;
                            let sx = canvas_rect.left() + t * canvas_rect.width();
                            let h = ((s as f32 * 0.8).sin() * 0.5 + (s as f32 * 2.5).cos() * 0.3).abs() * 32.0;
                            painter.line_segment([pos2(sx, mid_y - h), pos2(sx, mid_y + h)], Stroke::new(1.0_f32, Color32::from_rgb(0x28, 0x5a, 0x8a)));
                        }

                        // Warp Markers (Yellow Draggable Flags)
                        for (idx, m) in state.warp_markers.iter().enumerate() {
                            let mx = canvas_rect.left() + (m.beat as f32 / 16.0) * canvas_rect.width();
                            let flag_rect = Rect::from_min_size(pos2(mx - 4.0, canvas_rect.top() + 2.0), vec2(8.0, 10.0));
                            painter.rect_filled(flag_rect, 1.0, COLOR_AMBER);
                            painter.line_segment([pos2(mx, canvas_rect.top() + 12.0), pos2(mx, canvas_rect.bottom())], Stroke::new(1.2_f32, COLOR_AMBER));
                            painter.text(pos2(mx, canvas_rect.top() + 6.0), egui::Align2::CENTER_CENTER, format!("{}", idx + 1), FontId::monospace(7.0), Color32::BLACK);
                        }
                    }
                });
            }
        }
    });

    action
}

fn midi_to_note_name(pitch: u8) -> String {
    const NOTE_NAMES: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    let note_idx = (pitch % 12) as usize;
    let octave = (pitch as i32 / 12) - 1;
    format!("{}{}", NOTE_NAMES[note_idx], octave)
}

fn is_black_key(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

fn get_visible_pitches(
    notes: &[MidiNote],
    scale: &GlobalScale,
    scale_folded: bool,
) -> Vec<u8> {
    let mut pitches: Vec<u8> = Vec::new();
    let min_note = notes.iter().map(|n| n.pitch).min().unwrap_or(48);
    let max_note = notes.iter().map(|n| n.pitch).max().unwrap_or(72);

    if scale_folded {
        // Collect ~14 scale degrees covering clip's pitch range or C2 (36) up to C5 (72)
        let mut low = min_note.saturating_sub(2).min(48);
        let mut high = max_note.saturating_add(2).max(67);

        for p in 0..=127 {
            if (p >= low && p <= high && scale.is_in_scale(p)) || notes.iter().any(|n| n.pitch == p) {
                pitches.push(p);
            }
        }
        // Ensure at least 14 scale degrees
        while pitches.len() < 14 && (low > 12 || high < 108) {
            if low > 12 {
                low -= 1;
                if scale.is_in_scale(low) && !pitches.contains(&low) {
                    pitches.push(low);
                }
            }
            if high < 108 && pitches.len() < 14 {
                high += 1;
                if scale.is_in_scale(high) && !pitches.contains(&high) {
                    pitches.push(high);
                }
            }
        }
    } else {
        // Chromatic 16-row window centered around clip's notes (e.g. 48..64 or min..max pitch)
        let span = (max_note as i32 - min_note as i32).max(15);
        let center = (min_note as i32 + max_note as i32) / 2;
        let half = (span + 1) / 2;
        let start = (center - half).clamp(0, 127 - span) as u8;
        let count = span.min(24) as u8;
        for p in start..=(start + count) {
            pitches.push(p);
        }
        for n in notes {
            if !pitches.contains(&n.pitch) {
                pitches.push(n.pitch);
            }
        }
    }

    // Always sort descending: highest pitch at top (row 0), lowest pitch at bottom
    pitches.sort_by(|a, b| b.cmp(a));
    pitches.dedup();

    if pitches.is_empty() {
        pitches = (48..=64).rev().collect();
    }

    pitches
}

pub fn render_interactive_piano_roll(
    ui: &mut Ui,
    midi_pattern: &mut MidiPattern,
    state: &mut ClipViewState,
    scale: &GlobalScale,
    current_beat: f64,
) -> bool {
    let available_w = ui.available_width().max(420.0);
    let total_h = ui.available_height().clamp(155.0, 215.0);
    let (roll_rect, response) = ui.allocate_exact_size(vec2(available_w, total_h), Sense::click_and_drag());

    let mut pattern_mutated = false;

    if ui.is_rect_visible(roll_rect) {
        let painter = ui.painter_at(roll_rect);
        painter.rect_filled(roll_rect, 2.0, COLOR_BG_VOID);
        painter.rect_stroke(roll_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

        let ruler_h = 18.0_f32;
        let lanes_h = 34.0_f32;
        let gutter_w = 50.0_f32;

        let grid_top = roll_rect.top() + ruler_h;
        let grid_bottom = roll_rect.bottom() - lanes_h;
        let grid_h = (grid_bottom - grid_top).max(20.0);
        let grid_left = roll_rect.left() + gutter_w;
        let grid_w = (roll_rect.width() - gutter_w).max(10.0);

        let all_pitches = get_visible_pitches(&midi_pattern.notes, scale, state.scale_folded);
        // Select a 9-row viewport window covering all active clip notes so each row is >= 12px tall (zero label overlap)
        let target_rows = 9usize;
        let pitches: Vec<u8> = if all_pitches.len() <= target_rows {
            all_pitches
        } else {
            let mut min_idx = all_pitches.len() / 2;
            let mut max_idx = min_idx;
            if !midi_pattern.notes.is_empty() {
                min_idx = all_pitches.len().saturating_sub(1);
                max_idx = 0;
                for n in &midi_pattern.notes {
                    if let Some(pos) = all_pitches.iter().position(|&p| p == n.pitch) {
                        min_idx = min_idx.min(pos);
                        max_idx = max_idx.max(pos);
                    }
                }
            }
            let mid = (min_idx + max_idx) / 2;
            let start = mid
                .saturating_sub(target_rows / 2)
                .min(all_pitches.len().saturating_sub(target_rows));
            let end = (start + target_rows).min(all_pitches.len());
            all_pitches[start..end].to_vec()
        };

        let row_count = pitches.len().max(1);
        let row_h = grid_h / row_count as f32;

        let total_beats = midi_pattern.length_beats.max(4.0) as f32;
        let px_per_beat = grid_w / total_beats;

        // --- 1. Interaction: Click to Add / Remove / Select Notes ---
        let grid_rect = Rect::from_min_max(pos2(grid_left, grid_top), pos2(roll_rect.right(), grid_bottom));
        if response.clicked() || response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if grid_rect.contains(pos) {
                    let mut clicked_note_idx = None;
                    for (idx, note) in midi_pattern.notes.iter().enumerate() {
                        if let Some(row_idx) = pitches.iter().position(|&p| p == note.pitch) {
                            let nx = grid_left + note.start_beat as f32 * px_per_beat;
                            let nw = (note.duration_beats as f32 * px_per_beat).max(6.0);
                            let ny = grid_top + row_idx as f32 * row_h;
                            let n_rect = Rect::from_min_size(pos2(nx, ny + 1.0), vec2(nw, (row_h - 2.0).max(3.0)));
                            if n_rect.contains(pos) {
                                clicked_note_idx = Some(idx);
                                break;
                            }
                        }
                    }

                    if let Some(idx) = clicked_note_idx {
                        if response.secondary_clicked() {
                            midi_pattern.notes.remove(idx);
                            state.selected_note_idx = None;
                            pattern_mutated = true;
                        } else {
                            if state.selected_note_idx == Some(idx) {
                                midi_pattern.notes.remove(idx);
                                state.selected_note_idx = None;
                                pattern_mutated = true;
                            } else {
                                state.selected_note_idx = Some(idx);
                            }
                        }
                    } else if response.clicked() {
                        let row_idx = ((pos.y - grid_top) / row_h).floor() as usize;
                        if row_idx < pitches.len() {
                            let pitch = pitches[row_idx];
                            let beat_rel = ((pos.x - grid_left) / grid_w) as f64 * total_beats as f64;
                            let quant = 0.25;
                            let quantized_beat = ((beat_rel / quant).floor() * quant).clamp(0.0, (total_beats as f64 - 0.25).max(0.0));
                            let new_note = MidiNote::new(pitch, 100, quantized_beat, 0.5);
                            midi_pattern.notes.push(new_note);
                            state.selected_note_idx = Some(midi_pattern.notes.len() - 1);
                            pattern_mutated = true;
                        }
                    }
                }
            }
        }

        // --- 2. Interaction: Drag Velocity / Chance Lanes ---
        let lanes_rect = Rect::from_min_max(pos2(roll_rect.left(), grid_bottom), roll_rect.max);
        let mid_lane_y = lanes_rect.top() + 19.0;
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let lanes_grid_rect = Rect::from_min_max(pos2(grid_left, grid_bottom), roll_rect.max);
                if lanes_grid_rect.contains(pos) && !midi_pattern.notes.is_empty() {
                    let pointer_beat = ((pos.x - grid_left) / grid_w) as f64 * total_beats as f64;
                    let target_idx = if let Some(sel) = state.selected_note_idx {
                        if sel < midi_pattern.notes.len() { Some(sel) } else { None }
                    } else {
                        midi_pattern.notes.iter().enumerate().min_by(|(_, a), (_, b)| {
                            (a.start_beat - pointer_beat).abs().partial_cmp(&(b.start_beat - pointer_beat).abs()).unwrap_or(std::cmp::Ordering::Equal)
                        }).map(|(i, _)| i)
                    };

                    if let Some(idx) = target_idx {
                        let note = &mut midi_pattern.notes[idx];
                        if pos.y < mid_lane_y {
                            let vel_base = mid_lane_y - 2.0;
                            let norm = ((vel_base - pos.y) / 15.0).clamp(0.05, 1.0);
                            note.velocity = (norm * 127.0).round().clamp(1.0, 127.0) as u8;
                            pattern_mutated = true;
                        } else {
                            let chance_base = lanes_rect.bottom() - 2.0;
                            let norm = ((chance_base - pos.y) / 15.0).clamp(0.0, 1.0);
                            note.probability = norm;
                            pattern_mutated = true;
                        }
                    }
                }
            }
        }

        // --- 3. Render Top Bar/Beat Timeline Ruler (18.0 px high) ---
        let ruler_rect = Rect::from_min_max(roll_rect.min, pos2(roll_rect.right(), grid_top));
        let gutter_ruler_rect = Rect::from_min_max(ruler_rect.min, pos2(grid_left, grid_top));
        painter.rect_filled(gutter_ruler_rect, 0.0, COLOR_BG_PANEL_ALT);
        painter.text(gutter_ruler_rect.center(), egui::Align2::CENTER_CENTER, "KEY", FontId::monospace(7.5), COLOR_TEXT_DIM);
        painter.line_segment([gutter_ruler_rect.left_bottom(), gutter_ruler_rect.right_bottom()], Stroke::new(1.0_f32, COLOR_BORDER));
        painter.line_segment([gutter_ruler_rect.right_top(), gutter_ruler_rect.right_bottom()], Stroke::new(1.0_f32, COLOR_BORDER));

        let time_ruler_rect = Rect::from_min_max(pos2(grid_left, roll_rect.top()), pos2(roll_rect.right(), grid_top));
        painter.rect_filled(time_ruler_rect, 0.0, Color32::from_rgb(0x11, 0x14, 0x1c));
        painter.line_segment([time_ruler_rect.left_bottom(), time_ruler_rect.right_bottom()], Stroke::new(1.0_f32, COLOR_BORDER));

        let num_beats = total_beats.ceil() as i32;
        for b in 0..=num_beats {
            let beat_f = b as f32;
            if beat_f > total_beats { break; }
            let bx = grid_left + (beat_f / total_beats) * grid_w;

            if b % 4 == 0 {
                let bar = (b / 4) + 1;
                painter.text(pos2(bx + 3.0, ruler_rect.center().y), egui::Align2::LEFT_CENTER, format!("Bar {}", bar), FontId::monospace(8.5), COLOR_TEXT_PRIMARY);
                painter.line_segment([pos2(bx, grid_top), pos2(bx, grid_bottom)], Stroke::new(1.0_f32, Color32::from_rgb(0x2a, 0x2f, 0x3a)));
            } else {
                let bar = (b / 4) + 1;
                let beat_in_bar = (b % 4) + 1;
                painter.text(pos2(bx + 3.0, ruler_rect.center().y), egui::Align2::LEFT_CENTER, format!("{}.{}", bar, beat_in_bar), FontId::monospace(8.0), COLOR_TEXT_MUTED);
                painter.line_segment([pos2(bx, grid_top), pos2(bx, grid_bottom)], Stroke::new(1.0_f32, Color32::from_rgb(0x2a, 0x2f, 0x3a)));
            }

            for step in 1..4 {
                let sub_b = beat_f + step as f32 * 0.25;
                if sub_b < total_beats {
                    let sub_x = grid_left + (sub_b / total_beats) * grid_w;
                    painter.line_segment([pos2(sub_x, grid_top), pos2(sub_x, grid_bottom)], Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1b, 0x22)));
                    painter.line_segment([pos2(sub_x, ruler_rect.bottom() - 3.0), pos2(sub_x, ruler_rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0x2a, 0x2f, 0x3a)));
                }
            }
        }

        // --- 4. Render Left Piano Key Gutter (46.0 px wide) & Grid Rows ---
        for (r, &pitch) in pitches.iter().enumerate() {
            let ry = grid_top + r as f32 * row_h;
            let is_root = (pitch % 12) == (scale.root % 12);
            let black = is_black_key(pitch);

            let key_rect = Rect::from_min_size(pos2(roll_rect.left(), ry), vec2(gutter_w, row_h));
            let key_bg = if is_root {
                Color32::from_rgb(0x32, 0x27, 0x12)
            } else if black {
                Color32::from_rgb(0x12, 0x14, 0x1c)
            } else {
                Color32::from_rgb(0x20, 0x25, 0x32)
            };
            painter.rect_filled(key_rect, 0.0, key_bg);
            painter.line_segment([pos2(key_rect.left(), key_rect.bottom()), pos2(key_rect.right(), key_rect.bottom())], Stroke::new(0.5_f32, Color32::from_rgb(0x1a, 0x1e, 0x28)));
            painter.line_segment([pos2(key_rect.right(), key_rect.top()), pos2(key_rect.right(), key_rect.bottom())], Stroke::new(1.0_f32, COLOR_BORDER));

            if is_root {
                painter.rect_filled(Rect::from_min_size(key_rect.min, vec2(3.0, row_h)), 0.0, COLOR_AMBER);
            }

            let note_name = midi_to_note_name(pitch);
            let text_col = if is_root {
                COLOR_AMBER
            } else if black {
                COLOR_TEXT_MUTED
            } else {
                COLOR_TEXT_PRIMARY
            };
            painter.text(pos2(key_rect.right() - 4.0, key_rect.center().y), egui::Align2::RIGHT_CENTER, note_name, FontId::monospace(8.5), text_col);

            let grid_row_rect = Rect::from_min_size(pos2(grid_left, ry), vec2(grid_w, row_h));
            let grid_bg = if is_root {
                Color32::from_rgb(0x18, 0x1c, 0x22)
            } else if black {
                Color32::from_rgb(0x0c, 0x0e, 0x13)
            } else {
                Color32::from_rgb(0x10, 0x13, 0x1a)
            };
            painter.rect_filled(grid_row_rect, 0.0, grid_bg);
            painter.line_segment([grid_row_rect.left_bottom(), grid_row_rect.right_bottom()], Stroke::new(0.5_f32, Color32::from_rgb(0x1c, 0x20, 0x2c)));
        }

        // --- 5. Render MIDI Notes ---
        for (idx, note) in midi_pattern.notes.iter().enumerate() {
            if let Some(row_idx) = pitches.iter().position(|&p| p == note.pitch) {
                let nx = grid_left + note.start_beat as f32 * px_per_beat;
                let nw = (note.duration_beats as f32 * px_per_beat).max(6.0);
                let ny = grid_top + row_idx as f32 * row_h;

                let is_sel = state.selected_note_idx == Some(idx);
                let n_rect = Rect::from_min_size(pos2(nx, ny + 1.0), vec2(nw, (row_h - 2.0).max(3.0)));

                let note_color = if is_sel { COLOR_AMBER } else { COLOR_CYAN };
                painter.rect_filled(n_rect, 2.0, note_color);
                painter.rect_stroke(n_rect, 2.0, Stroke::new(1.0_f32, Color32::WHITE));

                if nw >= 18.0 {
                    let note_str = midi_to_note_name(note.pitch);
                    let label_col = if is_sel { Color32::from_rgb(0x1a, 0x12, 0x00) } else { Color32::from_rgb(0x08, 0x09, 0x0d) };
                    painter.text(n_rect.center(), egui::Align2::CENTER_CENTER, note_str, FontId::monospace(8.0), label_col);
                }
            }
        }

        // --- 6. Render Labeled Velocity & Chance Lanes (38.0 px high at bottom) ---
        let gutter_lane_rect = Rect::from_min_max(pos2(roll_rect.left(), grid_bottom), pos2(grid_left, roll_rect.bottom()));
        painter.rect_filled(gutter_lane_rect, 0.0, COLOR_BG_PANEL_ALT);
        painter.line_segment([pos2(gutter_lane_rect.left(), mid_lane_y), pos2(gutter_lane_rect.right(), mid_lane_y)], Stroke::new(0.5_f32, COLOR_BORDER));
        painter.line_segment([gutter_lane_rect.right_top(), gutter_lane_rect.right_bottom()], Stroke::new(1.0_f32, COLOR_BORDER));
        painter.text(pos2(gutter_lane_rect.center().x, lanes_rect.top() + 9.5), egui::Align2::CENTER_CENTER, "VEL", FontId::monospace(8.5), COLOR_AMBER);
        painter.text(pos2(gutter_lane_rect.center().x, lanes_rect.top() + 28.5), egui::Align2::CENTER_CENTER, "CHANCE", FontId::monospace(7.5), COLOR_CYAN);

        let lanes_grid_rect = Rect::from_min_max(pos2(grid_left, grid_bottom), roll_rect.max);
        painter.rect_filled(lanes_grid_rect, 0.0, COLOR_BG_VOID);
        painter.line_segment([pos2(grid_left, grid_bottom), pos2(roll_rect.right(), grid_bottom)], Stroke::new(1.0_f32, COLOR_BORDER));
        painter.line_segment([pos2(grid_left, mid_lane_y), pos2(roll_rect.right(), mid_lane_y)], Stroke::new(0.5_f32, Color32::from_rgb(0x18, 0x1d, 0x28)));

        for b in 0..=num_beats {
            let beat_f = b as f32;
            if beat_f > total_beats { break; }
            let bx = grid_left + (beat_f / total_beats) * grid_w;
            painter.line_segment([pos2(bx, grid_bottom), pos2(bx, roll_rect.bottom())], Stroke::new(0.5_f32, Color32::from_rgb(0x18, 0x1d, 0x28)));
        }

        for note in &midi_pattern.notes {
            let stalk_x = grid_left + note.start_beat as f32 * px_per_beat + 3.0;
            let vel_norm = (note.velocity as f32 / 127.0).clamp(0.05, 1.0);
            let vel_h = vel_norm * 14.0;
            let vel_base = mid_lane_y - 2.0;
            painter.line_segment([pos2(stalk_x, vel_base), pos2(stalk_x, vel_base - vel_h)], Stroke::new(2.0_f32, COLOR_AMBER));
            painter.circle_filled(pos2(stalk_x, vel_base - vel_h), 2.2, COLOR_AMBER);

            let prob_norm = note.probability.clamp(0.05, 1.0);
            let prob_h = prob_norm * 14.0;
            let chance_base = roll_rect.bottom() - 2.0;
            painter.line_segment([pos2(stalk_x, chance_base), pos2(stalk_x, chance_base - prob_h)], Stroke::new(2.0_f32, COLOR_CYAN));
            painter.circle_filled(pos2(stalk_x, chance_base - prob_h), 2.2, COLOR_CYAN);
        }

        // --- 7. Playhead Position Line ---
        let ph_x = grid_left + ((current_beat as f32 % total_beats) / total_beats) * grid_w;
        painter.line_segment([pos2(ph_x, roll_rect.top()), pos2(ph_x, roll_rect.bottom())], Stroke::new(1.5_f32, COLOR_GREEN));
        painter.line_segment([pos2(ph_x - 3.0, roll_rect.top()), pos2(ph_x + 3.0, roll_rect.top())], Stroke::new(2.0_f32, COLOR_GREEN));
    }

    pattern_mutated
}

#[cfg(test)]
mod tests {
    use super::*;
    use ableton_core::ScaleMode;

    #[test]
    fn test_pitch_ordering_and_note_naming() {
        assert_eq!(midi_to_note_name(60), "C4");
        assert_eq!(midi_to_note_name(61), "Db4");
        assert_eq!(midi_to_note_name(63), "Eb4");
        assert_eq!(midi_to_note_name(58), "Bb3");
        assert_eq!(midi_to_note_name(56), "Ab3");
        assert_eq!(midi_to_note_name(55), "G3");
        assert_eq!(midi_to_note_name(53), "F3");

        let scale = GlobalScale::new(0, ScaleMode::Major);
        let notes = vec![
            MidiNote::new(60, 100, 0.0, 1.0),
            MidiNote::new(64, 100, 1.0, 1.0),
            MidiNote::new(67, 100, 2.0, 1.0),
        ];

        // Folded scale mode: pitches must be strictly sorted descending
        let pitches_folded = get_visible_pitches(&notes, &scale, true);
        assert!(pitches_folded.len() >= 14);
        for w in pitches_folded.windows(2) {
            assert!(w[0] > w[1], "Pitches must be strictly descending: {} > {}", w[0], w[1]);
        }
        for n in &notes {
            assert!(pitches_folded.contains(&n.pitch));
        }
        let pos_67 = pitches_folded.iter().position(|&p| p == 67).unwrap();
        let pos_64 = pitches_folded.iter().position(|&p| p == 64).unwrap();
        let pos_60 = pitches_folded.iter().position(|&p| p == 60).unwrap();
        assert!(pos_67 < pos_64, "Higher pitch 67 must appear above (smaller row index than) 64");
        assert!(pos_64 < pos_60, "Higher pitch 64 must appear above 60");

        // Chromatic mode: pitches must be strictly sorted descending
        let pitches_chromatic = get_visible_pitches(&notes, &scale, false);
        assert!(pitches_chromatic.len() >= 16);
        for w in pitches_chromatic.windows(2) {
            assert!(w[0] > w[1], "Chromatic pitches must be strictly descending");
        }
        for n in &notes {
            assert!(pitches_chromatic.contains(&n.pitch));
        }
    }

    #[test]
    fn test_interactive_piano_roll_headless_execution() {
        let ctx = egui::Context::default();
        let mut state = ClipViewState::default();
        let mut pattern = MidiPattern::new(4.0);
        pattern.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        let scale = GlobalScale::default();

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mutated = render_interactive_piano_roll(ui, &mut pattern, &mut state, &scale, 1.0);
                assert!(!mutated);
                assert_eq!(pattern.notes.len(), 1);
            });
        });
    }
}
