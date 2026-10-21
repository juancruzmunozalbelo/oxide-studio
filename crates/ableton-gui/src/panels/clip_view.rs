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
    MidiPattern,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_GREEN, COLOR_TEXT_MUTED,
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
            let fold_text = if state.scale_folded { "🔒 Scale-Folded" } else { "🔓 Chromatic" };
            if ui.add(Button::new(RichText::new(fold_text).size(10.0).color(fold_col))).clicked() {
                state.scale_folded = !state.scale_folded;
            }
        });

        ui.separator();

        match state.sub_tab {
            ClipViewSubTab::Notes => {
                // Interactive Piano Roll + Velocity & Chance lanes
                render_interactive_piano_roll(ui, midi_pattern, state, global_scale, current_beat);
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

pub fn render_interactive_piano_roll(
    ui: &mut Ui,
    midi_pattern: &mut MidiPattern,
    state: &mut ClipViewState,
    scale: &GlobalScale,
    current_beat: f64,
) {
    let available_w = ui.available_width().max(420.0);
    let (roll_rect, response) = ui.allocate_exact_size(vec2(available_w, 130.0), Sense::click_and_drag());

    if ui.is_rect_visible(roll_rect) {
        let painter = ui.painter_at(roll_rect);
        painter.rect_filled(roll_rect, 2.0, COLOR_BG_VOID);
        painter.rect_stroke(roll_rect, 2.0, Stroke::new(1.0_f32, COLOR_BORDER));

        let roll_h = 90.0;
        let lanes_h = 36.0;
        let piano_rect = Rect::from_min_max(roll_rect.min, pos2(roll_rect.right(), roll_rect.top() + roll_h));
        let stalks_rect = Rect::from_min_max(pos2(roll_rect.left(), roll_rect.top() + roll_h), roll_rect.max);

        painter.line_segment([pos2(roll_rect.left(), piano_rect.bottom()), pos2(roll_rect.right(), piano_rect.bottom())], Stroke::new(1.0_f32, COLOR_BORDER));

        let total_beats = midi_pattern.length_beats.max(4.0) as f32;
        let px_per_beat = piano_rect.width() / total_beats;

        // Render Scale-Folded or Chromatic Rows
        let row_count = if state.scale_folded { 14 } else { 24 };
        let row_h = piano_rect.height() / row_count as f32;

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                for (idx, note) in midi_pattern.notes.iter().enumerate() {
                    let nx = piano_rect.left() + note.start_beat as f32 * px_per_beat;
                    let nw = (note.duration_beats as f32 * px_per_beat).max(4.0);
                    let row_idx = (note.pitch % row_count as u8) as f32;
                    let ny = piano_rect.bottom() - (row_idx + 1.0) * row_h;
                    let n_rect = Rect::from_min_size(pos2(nx, ny + 1.0), vec2(nw, row_h - 2.0));
                    if n_rect.contains(pos) {
                        state.selected_note_idx = Some(idx);
                        break;
                    }
                }
            }
        }

        for r in 0..row_count {
            let ry = piano_rect.top() + r as f32 * row_h;
            let pitch = 60 + r as u8;
            let in_scale = scale.is_in_scale(pitch);
            let bg = if in_scale {
                Color32::from_rgb(0x15, 0x22, 0x2e)
            } else if r % 2 == 0 {
                Color32::from_rgb(0x10, 0x14, 0x1e)
            } else {
                Color32::from_rgb(0x14, 0x18, 0x24)
            };
            painter.rect_filled(Rect::from_min_size(pos2(piano_rect.left(), ry), vec2(piano_rect.width(), row_h)), 0.0, bg);
        }

        // Render MIDI Notes
        for (idx, note) in midi_pattern.notes.iter().enumerate() {
            let nx = piano_rect.left() + note.start_beat as f32 * px_per_beat;
            let nw = (note.duration_beats as f32 * px_per_beat).max(4.0);
            let row_idx = (note.pitch % row_count as u8) as f32;
            let ny = piano_rect.bottom() - (row_idx + 1.0) * row_h;

            let is_sel = state.selected_note_idx == Some(idx);
            let n_rect = Rect::from_min_size(pos2(nx, ny + 1.0), vec2(nw, row_h - 2.0));

            painter.rect_filled(n_rect, 1.0, if is_sel { COLOR_AMBER } else { COLOR_CYAN });
            painter.rect_stroke(n_rect, 1.0, Stroke::new(1.0_f32, Color32::WHITE));

            // Velocity & Chance Stalks in bottom lane
            let stalk_x = nx + 2.0;
            let vel_norm = note.velocity as f32 / 127.0;
            let vel_h = vel_norm * (lanes_h * 0.45);
            let prob_h = note.probability * (lanes_h * 0.45);

            // Velocity stalk (Cyan)
            painter.line_segment([pos2(stalk_x, stalks_rect.top() + lanes_h * 0.5), pos2(stalk_x, stalks_rect.top() + lanes_h * 0.5 - vel_h)], Stroke::new(1.5_f32, COLOR_CYAN));
            // Chance stalk (Amber)
            painter.line_segment([pos2(stalk_x + 2.0, stalks_rect.bottom()), pos2(stalk_x + 2.0, stalks_rect.bottom() - prob_h)], Stroke::new(1.5_f32, COLOR_AMBER));
        }

        // Playhead position line
        let ph_x = piano_rect.left() + (current_beat as f32 % total_beats) * px_per_beat;
        painter.line_segment([pos2(ph_x, roll_rect.top()), pos2(ph_x, roll_rect.bottom())], Stroke::new(1.5_f32, COLOR_GREEN));
    }
}
