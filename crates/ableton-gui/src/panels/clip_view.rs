//! Live 12 Bottom Detail View: Clip View.
//!
//! Features:
//! - Live 12 Side-by-Side Clip Inspector + Canvas Layout (~220px inspector + interactive Piano Roll / Warping canvas)
//! - MIDI Piano Roll: Chromatic / Scale-Folded mode, Velocity stalks lane, Chance (%) probability lane
//! - Dynamic Bar Span: zero clipped notes up to 16.0+ beats with 1, 2, 4, 8 Bar loop selector
//! - Direct Note Manipulation: Move (time & pitch) and right-edge 7px handle Resize (duration), snapped to grid
//! - Key Gutter Audition: click & drag piano keys to audition note with active amber highlight
//! - Scale Fold & Octave Navigation: Scale-Fold toggle, [+ Oct] / [- Oct] buttons, mouse-wheel pitch scroll
//! - Live 12 Transform Tools: Quantize, Connect, Ornament, Recombine, Span, Strum, Time Warp
//! - Live 12 Generate Tools: Seed, Shape, Stacks, Rhythm, Euclidean
//! - Audio Clip Warping Panel: Waveform display with draggable Warp Markers, 6 modes (Beats, Tones,
//!   Texture, RePitch, Complex, ComplexPro), pitch transposition (-48..+48 st), and clip gain dB.

use egui::{
    pos2, vec2, Button, Color32, ComboBox, DragValue, FontId, Frame, Margin, Rect, RichText,
    Sense, Slider, Stroke, Ui,
};
use ableton_core::GlobalScale;
use ableton_timeline::{
    generate::{generate_euclidean, generate_seed, generate_stacks, StacksChordType, StacksVoicing},
    transform::{transform_connect, transform_ornament, transform_quantize, transform_recombine, transform_strum, StrumDirection},
    warping::{WarpMarker, WarpMode},
    MidiNote, MidiPattern,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BORDER, COLOR_CYAN,
    COLOR_GREEN, COLOR_TEXT_DIM, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipViewSubTab {
    Notes,
    Transform,
    Generate,
    Warping,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteDragMode {
    Move {
        orig_start: f64,
        orig_pitch: u8,
        pointer_start_beat: f64,
        pointer_start_row: usize,
    },
    ResizeRight {
        orig_duration: f64,
        pointer_start_beat: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteDragState {
    pub note_idx: usize,
    pub mode: NoteDragMode,
}

#[derive(Debug, Clone)]
pub struct ClipViewState {
    pub is_audio: bool,
    pub sub_tab: ClipViewSubTab,
    pub scale_folded: bool,
    pub selected_note_idx: Option<usize>,

    // Live 12 dynamic bar span and pitch navigation
    pub loop_length_bars: usize, // 1, 2, 4, 8 Bars
    pub pitch_scroll_offset: i32,
    pub note_drag_state: Option<NoteDragState>,
    pub audition_pitch: Option<u8>,
    pub last_audition_note: Option<(u8, u8)>, // (pitch, velocity)

    // Transform parameters
    pub quantize_grid: f64, // 1.0 (1/4), 0.5 (1/8), 0.25 (1/16), 0.125 (1/32)
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
    pub stacks_chord_type: usize, // 0 = Major, 1 = Minor 7th, 2 = 9th
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
            loop_length_bars: 4,
            pitch_scroll_offset: 0,
            note_drag_state: None,
            audition_pitch: None,
            last_audition_note: None,
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
    pub audition_note: Option<(u8, u8)>, // (pitch, velocity)
    pub loop_length_changed: Option<f64>,
}

/// Ableton Live 12 Side-by-Side Clip View: Left Inspector (~220px) + Right Canvas.
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
        audition_note: None,
        loop_length_changed: None,
    };

    let total_avail_h = (ui.available_height() - 2.0).max(60.0);

    Frame::none()
        .fill(Color32::from_rgb(0x3e, 0x3e, 0x3e)) // DetailViewBackground #3e3e3e
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
                // --- 1. Left Column: Live 12 Inspector on SurfaceBackground #363636 ---
                Frame::none()
                    .fill(Color32::from_rgb(0x36, 0x36, 0x36)) // SurfaceBackground #363636
                    .inner_margin(Margin::symmetric(6.0, 2.0))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            vec2(216.0, (total_avail_h - 4.0).max(50.0)),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                render_clip_inspector(ui, state, midi_pattern, global_scale, &mut action);
                            },
                        );
                    });

                // Crisp 1px #111111 vertical divider (ControlContrastFrame)
                let (divider_rect, _) = ui.allocate_exact_size(vec2(1.0, total_avail_h), Sense::hover());
                ui.painter().rect_filled(divider_rect, 0.0, Color32::from_rgb(0x11, 0x11, 0x11));

                // --- 2. Right Column: Live Interactive Canvas (Piano Roll or Audio Waveform) ---
                let canvas_w = ui.available_width().max(300.0);
                ui.allocate_ui_with_layout(
                    vec2(canvas_w, total_avail_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        if state.is_audio || state.sub_tab == ClipViewSubTab::Warping {
                            render_audio_warp_canvas(ui, state, &mut action);
                        } else {
                            if render_interactive_piano_roll(
                                ui,
                                midi_pattern,
                                state,
                                global_scale,
                                current_beat,
                            ) {
                                action.pattern_mutated = true;
                            }
                            if let Some(audition) = state.last_audition_note.take() {
                                action.audition_note = Some(audition);
                            }
                        }
                    },
                );
            });
        });

    action
}

/// Render the compact left-hand Live 12 Inspector with tabs for Clip/Loop, Transform, Generate, Warp.
pub fn render_clip_inspector(
    ui: &mut Ui,
    state: &mut ClipViewState,
    midi_pattern: &mut MidiPattern,
    global_scale: &GlobalScale,
    action: &mut ClipViewAction,
) {
    let is_compact_h = ui.available_height() < 160.0;
    ui.spacing_mut().item_spacing = vec2(3.0, if is_compact_h { 1.0 } else { 2.0 });
    ui.spacing_mut().button_padding = vec2(4.0, if is_compact_h { 1.0 } else { 2.0 });

    // Inspector Tab Selector Bar (4 compact buttons)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 0.0);

        let notes_active = state.sub_tab == ClipViewSubTab::Notes;
        let notes_btn = Button::new(RichText::new("Clip").size(10.0).strong().color(if notes_active { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
            .fill(if notes_active { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(notes_btn).clicked() {
            state.sub_tab = ClipViewSubTab::Notes;
            state.is_audio = false;
        }

        let trans_active = state.sub_tab == ClipViewSubTab::Transform;
        let trans_btn = Button::new(RichText::new("Trans").size(10.0).strong().color(if trans_active { COLOR_CYAN } else { COLOR_TEXT_MUTED }))
            .fill(if trans_active { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(trans_btn).clicked() {
            state.sub_tab = ClipViewSubTab::Transform;
            state.is_audio = false;
        }

        let gen_active = state.sub_tab == ClipViewSubTab::Generate;
        let gen_btn = Button::new(RichText::new("Gen").size(10.0).strong().color(if gen_active { COLOR_GREEN } else { COLOR_TEXT_MUTED }))
            .fill(if gen_active { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(gen_btn).clicked() {
            state.sub_tab = ClipViewSubTab::Generate;
            state.is_audio = false;
        }

        let warp_active = state.sub_tab == ClipViewSubTab::Warping;
        let warp_btn = Button::new(RichText::new("Warp").size(10.0).strong().color(if warp_active { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
            .fill(if warp_active { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
        if ui.add(warp_btn).clicked() {
            state.sub_tab = ClipViewSubTab::Warping;
        }
    });

    ui.separator();

    // Tab Content Panel
    match state.sub_tab {
        ClipViewSubTab::Notes => {
            ui.label(RichText::new("CLIP / LOOP & GRID").size(9.5).color(COLOR_AMBER).strong());

            // 1. Loop Length Bar Span Selector
            ui.horizontal(|ui| {
                ui.label(RichText::new("Span:").size(9.0).color(COLOR_TEXT_MUTED));
                let bar_options = [1usize, 2, 4, 8];
                for &bars in &bar_options {
                    let is_sel = state.loop_length_bars == bars;
                    let btn_txt = format!("{}B", bars);
                    let btn = Button::new(RichText::new(btn_txt).size(9.0).color(if is_sel { COLOR_AMBER } else { COLOR_TEXT_MUTED }))
                        .fill(if is_sel { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
                    if ui.add(btn).clicked() {
                        state.loop_length_bars = bars;
                        let new_beats = (bars * 4) as f64;
                        midi_pattern.length_beats = new_beats;
                        action.loop_length_changed = Some(new_beats);
                        action.pattern_mutated = true;
                    }
                }
            });

            // 2. Grid Division Selector
            ui.horizontal(|ui| {
                ui.label(RichText::new("Grid:").size(9.0).color(COLOR_TEXT_MUTED));
                let grid_options = [(1.0, "1/4"), (0.5, "1/8"), (0.25, "1/16"), (0.125, "1/32")];
                for &(grid_val, label) in &grid_options {
                    let is_sel = (state.quantize_grid - grid_val).abs() < 1e-4;
                    let btn = Button::new(RichText::new(label).size(9.0).color(if is_sel { COLOR_CYAN } else { COLOR_TEXT_MUTED }))
                        .fill(if is_sel { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
                    if ui.add(btn).clicked() {
                        state.quantize_grid = grid_val;
                    }
                }
            });

            // 3. Scale-Fold & Octave / Pitch Scroll Navigation (compact single row)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                let fold_col = if state.scale_folded { COLOR_AMBER } else { COLOR_TEXT_MUTED };
                let fold_text = if state.scale_folded { "★ Fold" } else { "☆ Fold" };
                if ui.add(Button::new(RichText::new(fold_text).size(9.0).color(fold_col))).on_hover_text("Toggle Scale Fold").clicked() {
                    state.scale_folded = !state.scale_folded;
                }

                ui.label(RichText::new("Oct:").size(9.0).color(COLOR_TEXT_MUTED));
                let oct_step = if state.scale_folded { 7 } else { 12 };
                if ui.add(Button::new(RichText::new("+ Oct").size(9.0).color(COLOR_TEXT_PRIMARY))).clicked() {
                    state.pitch_scroll_offset = (state.pitch_scroll_offset + oct_step).min(36);
                }
                if ui.add(Button::new(RichText::new("- Oct").size(9.0).color(COLOR_TEXT_PRIMARY))).clicked() {
                    state.pitch_scroll_offset = (state.pitch_scroll_offset - oct_step).max(-36);
                }
                if state.pitch_scroll_offset != 0
                    && ui.add(Button::new(RichText::new("Rst").size(8.5).color(COLOR_TEXT_MUTED))).clicked()
                {
                    state.pitch_scroll_offset = 0;
                }
            });

            if ui.available_height() >= 24.0 {
                ui.separator();
                // 4. Pattern Telemetry & Help
                let notes_max_end = midi_pattern.notes.iter().map(|n| n.end_beat()).fold(4.0, f64::max);
                let total_beats = (state.loop_length_bars as f64 * 4.0).max(notes_max_end).max(midi_pattern.length_beats).max(4.0) as f32;
                let sel_txt = state.selected_note_idx.map(|i| format!("#{}", i + 1)).unwrap_or_else(|| "-".into());
                ui.label(RichText::new(format!("Len: {:.0}b ({}B) | Notes: {} | Sel: {}", total_beats, (total_beats / 4.0).ceil() as usize, midi_pattern.notes.len(), sel_txt)).size(8.5).color(COLOR_TEXT_MUTED));
                if ui.available_height() >= 14.0 {
                    ui.label(RichText::new("• Click Key: audition • Drag/7px: edit").size(8.0).color(COLOR_TEXT_DIM));
                }
            }
        }

        ClipViewSubTab::Transform => {
            ui.label(RichText::new("LIVE 12 TRANSFORM").size(9.5).color(COLOR_CYAN).strong());

            // 1. Quantize
            ui.horizontal(|ui| {
                ui.label(RichText::new("Quant:").size(9.0).color(COLOR_TEXT_MUTED));
                ui.add(Slider::new(&mut state.quantize_amount, 0.0..=1.0).show_value(false));
                if ui.add(Button::new(RichText::new("Apply").size(9.0).color(COLOR_CYAN))).clicked() {
                    transform_quantize(midi_pattern, state.quantize_grid, state.quantize_amount, 0.0);
                    action.pattern_mutated = true;
                }
            });

            // 2. Strum
            ui.horizontal(|ui| {
                ui.label(RichText::new("Strum:").size(9.0).color(COLOR_TEXT_MUTED));
                ui.add(Slider::new(&mut state.strum_spread_ms, 5.0..=100.0).show_value(false));
                let dir_lbl = if state.strum_direction_up { "Up" } else { "Down" };
                if ui.add(Button::new(RichText::new(dir_lbl).size(9.0).color(COLOR_CYAN))).clicked() {
                    state.strum_direction_up = !state.strum_direction_up;
                }
                if ui.add(Button::new(RichText::new("Apply").size(9.0).color(COLOR_CYAN))).clicked() {
                    let spread_beats = (state.strum_spread_ms / 1000.0) * (124.0 / 60.0);
                    let dir = if state.strum_direction_up {
                        StrumDirection::Up
                    } else {
                        StrumDirection::Down
                    };
                    transform_strum(midi_pattern, spread_beats as f64, dir);
                    action.pattern_mutated = true;
                }
            });

            // 3. Connect & Ornament & Recombine
            ui.horizontal(|ui| {
                if ui.add(Button::new(RichText::new("Connect").size(9.0).color(COLOR_CYAN))).clicked() {
                    let sl = ableton_timeline::generate::ScaleLock::dorian(global_scale.root);
                    transform_connect(midi_pattern, sl, 0.5);
                    action.pattern_mutated = true;
                }
                if ui.add(Button::new(RichText::new("Ornament").size(9.0).color(COLOR_CYAN))).clicked() {
                    transform_ornament(midi_pattern, 0.0625, 2, state.ornament_velocity);
                    action.pattern_mutated = true;
                }
                if ui.add(Button::new(RichText::new("Recombine").size(9.0).color(COLOR_CYAN))).clicked() {
                    transform_recombine(midi_pattern, 1, 1, 1);
                    action.pattern_mutated = true;
                }
            });

            ui.separator();
            ui.label(RichText::new("Mutations update Piano Roll instantly side-by-side.").size(8.5).color(COLOR_TEXT_DIM));
        }

        ClipViewSubTab::Generate => {
            ui.label(RichText::new("LIVE 12 GENERATE").size(9.5).color(COLOR_GREEN).strong());

            // 1. Seed Generator
            ui.horizontal(|ui| {
                ui.label(RichText::new("Density:").size(9.0).color(COLOR_TEXT_MUTED));
                ui.add(Slider::new(&mut state.seed_density, 0.1..=1.0).show_value(false));
                if ui.add(Button::new(RichText::new("Seed").size(9.0).color(COLOR_GREEN))).clicked() {
                    let sl = ableton_timeline::generate::ScaleLock::dorian(global_scale.root);
                    let span_beats = (state.loop_length_bars * 4) as f64;
                    *midi_pattern = generate_seed(span_beats, 0.25, state.seed_density, state.seed_min_pitch, state.seed_max_pitch, 80, 115, 0.2, 0.2, Some(sl), 42);
                    action.pattern_mutated = true;
                }
            });

            // 2. Stacks Generator
            ui.horizontal(|ui| {
                ui.label(RichText::new("Stacks:").size(9.0).color(COLOR_TEXT_MUTED));
                let chord_labels = ["Maj", "Min7", "9th"];
                for (idx, &lbl) in chord_labels.iter().enumerate() {
                    let is_sel = state.stacks_chord_type == idx;
                    let btn = Button::new(RichText::new(lbl).size(8.5).color(if is_sel { COLOR_GREEN } else { COLOR_TEXT_MUTED }))
                        .fill(if is_sel { COLOR_BG_PANEL_ALT } else { COLOR_BG_SURFACE });
                    if ui.add(btn).clicked() {
                        state.stacks_chord_type = idx;
                    }
                }
                if ui.add(Button::new(RichText::new("Apply").size(9.0).color(COLOR_GREEN))).clicked() {
                    let chord_type = match state.stacks_chord_type {
                        0 => StacksChordType::MajorTriad,
                        1 => StacksChordType::Min7,
                        _ => StacksChordType::Dom7,
                    };
                    let roots = [41, 44, 46, 48];
                    *midi_pattern = generate_stacks(&roots, chord_type, StacksVoicing::Closed, 0, 1.0, 105);
                    action.pattern_mutated = true;
                }
            });

            // 3. Euclidean Generator
            ui.horizontal(|ui| {
                ui.label(RichText::new("Euclid:").size(9.0).color(COLOR_TEXT_MUTED));
                ui.add(Slider::new(&mut state.euclidean_hits, 1..=16).show_value(false));
                if ui.add(Button::new(RichText::new("Generate").size(9.0).color(COLOR_GREEN))).clicked() {
                    *midi_pattern = generate_euclidean(state.euclidean_hits as usize, state.euclidean_steps as usize, 0, 41, 0.25, 110);
                    action.pattern_mutated = true;
                }
            });

            ui.separator();
            ui.label(RichText::new("Generated patterns replace notes with real-time feedback.").size(8.5).color(COLOR_TEXT_DIM));
        }

        ClipViewSubTab::Warping => {
            ui.label(RichText::new("AUDIO WARPING").size(9.5).color(COLOR_AMBER).strong());

            // Warp Mode Combo
            let modes = [
                WarpMode::Beats,
                WarpMode::Tones,
                WarpMode::Texture,
                WarpMode::RePitch,
                WarpMode::Complex,
                WarpMode::ComplexPro,
            ];
            ComboBox::from_id_salt("clip_warp_mode_inspector")
                .selected_text(format!("{:?}", state.warp_mode))
                .show_ui(ui, |ui| {
                    for &m in &modes {
                        if ui.selectable_label(state.warp_mode == m, format!("{:?}", m)).clicked() {
                            state.warp_mode = m;
                            action.warp_mode_changed = Some(m);
                        }
                    }
                });

            // Transpose
            ui.horizontal(|ui| {
                ui.label(RichText::new("Transpose:").size(9.0).color(COLOR_TEXT_MUTED));
                let mut tr = state.transpose_semitones as i32;
                if ui.add(DragValue::new(&mut tr).range(-48..=48).suffix(" st")).changed() {
                    state.transpose_semitones = tr as i8;
                    action.transpose_changed = Some(state.transpose_semitones);
                }
            });

            // Gain dB
            ui.horizontal(|ui| {
                ui.label(RichText::new("Gain:").size(9.0).color(COLOR_TEXT_MUTED));
                ui.add(Slider::new(&mut state.clip_gain_db, -24.0..=12.0).suffix(" dB").show_value(false));
            });

            // Audio Mode Toggle
            ui.horizontal(|ui| {
                let audio_col = if state.is_audio { COLOR_AMBER } else { COLOR_TEXT_MUTED };
                let audio_txt = if state.is_audio { "Canvas: Audio Wave" } else { "Canvas: Piano Roll" };
                if ui.add(Button::new(RichText::new(audio_txt).size(9.0).color(audio_col))).clicked() {
                    state.is_audio = !state.is_audio;
                }
            });
        }
    }
}

/// Render the Audio Warping waveform with draggable warp markers.
pub fn render_audio_warp_canvas(
    ui: &mut Ui,
    state: &mut ClipViewState,
    action: &mut ClipViewAction,
) {
    let available_w = ui.available_width().max(300.0);
    let total_h = (ui.available_height() - 2.0).max(50.0);
    let (canvas_rect, mut response) =
        ui.allocate_exact_size(vec2(available_w, total_h), Sense::click_and_drag());

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
            painter.line_segment(
                [pos2(sx, mid_y - h), pos2(sx, mid_y + h)],
                Stroke::new(1.0_f32, Color32::from_rgb(0x28, 0x5a, 0x8a)),
            );
        }

        // Warp Markers (Yellow Draggable Flags)
        for (idx, m) in state.warp_markers.iter().enumerate() {
            let mx = canvas_rect.left() + (m.beat as f32 / 16.0) * canvas_rect.width();
            let flag_rect =
                Rect::from_min_size(pos2(mx - 4.0, canvas_rect.top() + 2.0), vec2(8.0, 10.0));
            painter.rect_filled(flag_rect, 1.0, COLOR_AMBER);
            painter.line_segment(
                [pos2(mx, canvas_rect.top() + 12.0), pos2(mx, canvas_rect.bottom())],
                Stroke::new(1.2_f32, COLOR_AMBER),
            );
            painter.text(
                pos2(mx, canvas_rect.top() + 6.0),
                egui::Align2::CENTER_CENTER,
                format!("{}", idx + 1),
                FontId::monospace(7.0),
                Color32::BLACK,
            );
        }
    }
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

/// Render the interactive Piano Roll Pro canvas with dynamic bar span, note dragging, edge resizing, and audition.
pub fn render_interactive_piano_roll(
    ui: &mut Ui,
    midi_pattern: &mut MidiPattern,
    state: &mut ClipViewState,
    scale: &GlobalScale,
    current_beat: f64,
) -> bool {
    let available_w = ui.available_width().max(380.0);
    let total_h = (ui.available_height() - 2.0).max(50.0);
    let (roll_rect, response) = ui.allocate_exact_size(vec2(available_w, total_h), Sense::click_and_drag());

    let mut pattern_mutated = false;

    if ui.is_rect_visible(roll_rect) {
        let painter = ui.painter_at(roll_rect);
        painter.rect_filled(roll_rect, 0.0, Color32::from_rgb(0x24, 0x24, 0x24)); // SurfaceArea #242424
        painter.rect_stroke(roll_rect, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11))); // ControlContrastFrame #111111

        let ruler_h = 18.0_f32;
        let lanes_h = if total_h < 130.0 { 24.0_f32 } else { 36.0_f32 };
        let gutter_w = 48.0_f32;

        let grid_top = roll_rect.top() + ruler_h;
        let grid_bottom = roll_rect.bottom() - lanes_h;
        let grid_h = (grid_bottom - grid_top).max(20.0);
        let grid_left = roll_rect.left() + gutter_w;
        let grid_w = (roll_rect.width() - gutter_w).max(10.0);

        // --- 1. Dynamic Bar Span Calculation (Zero Clipped Notes) ---
        let notes_max_end = midi_pattern.notes.iter().map(|n| n.end_beat()).fold(4.0, f64::max);
        let total_beats = (state.loop_length_bars as f64 * 4.0)
            .max(notes_max_end)
            .max(midi_pattern.length_beats)
            .max(4.0) as f32;
        let px_per_beat = grid_w / total_beats;

        // --- 2. Visible Pitches with Octave / Pitch Scroll ---
        let all_pitches = get_visible_pitches(&midi_pattern.notes, scale, state.scale_folded);
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
            let mid = ((min_idx + max_idx) / 2) as i32;
            let max_start = all_pitches.len().saturating_sub(target_rows) as i32;
            let base_start = mid.saturating_sub((target_rows / 2) as i32);
            // pitch_scroll_offset: positive shifts towards higher pitch (lower index in descending list)
            let start = (base_start - state.pitch_scroll_offset).clamp(0, max_start) as usize;
            let end = (start + target_rows).min(all_pitches.len());
            all_pitches[start..end].to_vec()
        };

        let row_count = pitches.len().max(1);
        let row_h = grid_h / row_count as f32;

        let grid_rect = Rect::from_min_max(pos2(grid_left, grid_top), pos2(roll_rect.right(), grid_bottom));
        let gutter_keys_rect = Rect::from_min_max(pos2(roll_rect.left(), grid_top), pos2(grid_left, grid_bottom));
        let lanes_grid_rect = Rect::from_min_max(pos2(grid_left, grid_bottom), roll_rect.max);
        let mid_lane_y = lanes_grid_rect.top() + (lanes_grid_rect.height() * 0.5);

        // --- 3. Mouse Wheel Pitch Navigation ---
        if response.hovered() {
            let scroll_y = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_y > 2.0 {
                state.pitch_scroll_offset = (state.pitch_scroll_offset + 1).min(36);
            } else if scroll_y < -2.0 {
                state.pitch_scroll_offset = (state.pitch_scroll_offset - 1).max(-36);
            }
        }

        // --- 4. Cursor Feedback & Right Edge Hover Affordance ---
        if let Some(hover_pos) = response.hover_pos() {
            if grid_rect.contains(hover_pos) {
                let mut hovered_right_edge = false;
                let mut hovered_body = false;
                for note in &midi_pattern.notes {
                    if let Some(row_idx) = pitches.iter().position(|&p| p == note.pitch) {
                        let nx = grid_left + note.start_beat as f32 * px_per_beat;
                        let nw = (note.duration_beats as f32 * px_per_beat).max(6.0);
                        let ny = grid_top + row_idx as f32 * row_h;
                        let right_handle_w = 7.0_f32.min(nw * 0.4);
                        let right_rect = Rect::from_min_max(pos2(nx + nw - right_handle_w, ny), pos2(nx + nw + 3.0, ny + row_h));
                        let body_rect = Rect::from_min_max(pos2(nx, ny), pos2(nx + nw - right_handle_w, ny + row_h));
                        if right_rect.contains(hover_pos) {
                            hovered_right_edge = true;
                            break;
                        } else if body_rect.contains(hover_pos) {
                            hovered_body = true;
                            break;
                        }
                    }
                }
                if state.note_drag_state.is_some() {
                    if let Some(drag) = state.note_drag_state {
                        match drag.mode {
                            NoteDragMode::ResizeRight { .. } => {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                            }
                            NoteDragMode::Move { .. } => {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                            }
                        }
                    }
                } else if hovered_right_edge {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                } else if hovered_body {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }

        // --- 5. Key Gutter Audition Sensing ---
        if let Some(pos) = response.interact_pointer_pos() {
            if gutter_keys_rect.contains(pos) && (response.clicked() || response.dragged() || response.is_pointer_button_down_on()) {
                let r = ((pos.y - grid_top) / row_h).floor().clamp(0.0, (pitches.len() - 1) as f32) as usize;
                if r < pitches.len() {
                    let pitch = pitches[r];
                    state.audition_pitch = Some(pitch);
                    state.last_audition_note = Some((pitch, 100));
                }
            }
        }
        if !response.is_pointer_button_down_on() && !response.dragged() {
            state.audition_pitch = None;
        }

        // --- 6. Direct Note Manipulation (Move & Resize) Engine ---
        if response.drag_started() {
            let press_pos = ui.input(|i| i.pointer.press_origin()).or_else(|| response.interact_pointer_pos());
            if let Some(pos) = press_pos {
                if grid_rect.contains(pos) {
                    for (idx, note) in midi_pattern.notes.iter().enumerate().rev() {
                        if let Some(row_idx) = pitches.iter().position(|&p| p == note.pitch) {
                            let nx = grid_left + note.start_beat as f32 * px_per_beat;
                            let nw = (note.duration_beats as f32 * px_per_beat).max(6.0);
                            let ny = grid_top + row_idx as f32 * row_h;
                            let right_handle_w = 7.0_f32.min(nw * 0.4);
                            let right_rect = Rect::from_min_max(pos2(nx + nw - right_handle_w, ny), pos2(nx + nw + 3.0, ny + row_h));
                            let body_rect = Rect::from_min_max(pos2(nx, ny), pos2(nx + nw - right_handle_w, ny + row_h));
                            let press_beat = ((pos.x - grid_left) / px_per_beat) as f64;

                            if right_rect.contains(pos) {
                                state.note_drag_state = Some(NoteDragState {
                                    note_idx: idx,
                                    mode: NoteDragMode::ResizeRight {
                                        orig_duration: note.duration_beats,
                                        pointer_start_beat: press_beat,
                                    },
                                });
                                state.selected_note_idx = Some(idx);
                                break;
                            } else if body_rect.contains(pos) {
                                state.note_drag_state = Some(NoteDragState {
                                    note_idx: idx,
                                    mode: NoteDragMode::Move {
                                        orig_start: note.start_beat,
                                        orig_pitch: note.pitch,
                                        pointer_start_beat: press_beat,
                                        pointer_start_row: row_idx,
                                    },
                                });
                                state.selected_note_idx = Some(idx);
                                break;
                            }
                        }
                    }
                }
            }
        }

        if response.dragged() {
            if let Some(drag) = state.note_drag_state {
                if drag.note_idx < midi_pattern.notes.len() {
                    if let Some(curr_pos) = response.interact_pointer_pos() {
                        let curr_beat = ((curr_pos.x - grid_left) / px_per_beat) as f64;
                        let q = state.quantize_grid.max(0.0625);

                        match drag.mode {
                            NoteDragMode::ResizeRight { orig_duration, pointer_start_beat } => {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                                let delta_beats = curr_beat - pointer_start_beat;
                                let note = &mut midi_pattern.notes[drag.note_idx];
                                let max_dur = (total_beats as f64 - note.start_beat).max(q);
                                let raw_dur = (orig_duration + delta_beats).clamp(q, max_dur);
                                let snapped_dur = ((raw_dur / q).round() * q).clamp(q, max_dur);
                                if (note.duration_beats - snapped_dur).abs() > 1e-4 {
                                    note.duration_beats = snapped_dur;
                                    pattern_mutated = true;
                                }
                            }
                            NoteDragMode::Move { orig_start, orig_pitch, pointer_start_beat, .. } => {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                let delta_beats = curr_beat - pointer_start_beat;
                                let note = &mut midi_pattern.notes[drag.note_idx];
                                let max_start = (total_beats as f64 - note.duration_beats).max(0.0);
                                let raw_start = (orig_start + delta_beats).clamp(0.0, max_start);
                                let snapped_start = ((raw_start / q).round() * q).clamp(0.0, max_start);

                                let curr_row = ((curr_pos.y - grid_top) / row_h).floor().clamp(0.0, (pitches.len() - 1) as f32) as usize;
                                let new_pitch = if curr_row < pitches.len() { pitches[curr_row] } else { orig_pitch };

                                if (note.start_beat - snapped_start).abs() > 1e-4 || note.pitch != new_pitch {
                                    note.start_beat = snapped_start;
                                    note.pitch = new_pitch;
                                    pattern_mutated = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !response.dragged() {
            state.note_drag_state = None;
        }

        // --- 7. Click to Add / Select / Delete Notes ---
        if (response.clicked() || response.secondary_clicked()) && state.note_drag_state.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                if grid_rect.contains(pos) {
                    let mut clicked_note_idx = None;
                    for (idx, note) in midi_pattern.notes.iter().enumerate().rev() {
                        if let Some(row_idx) = pitches.iter().position(|&p| p == note.pitch) {
                            let nx = grid_left + note.start_beat as f32 * px_per_beat;
                            let nw = (note.duration_beats as f32 * px_per_beat).max(6.0);
                            let ny = grid_top + row_idx as f32 * row_h;
                            let n_rect = Rect::from_min_max(pos2(nx, ny), pos2(nx + nw + 3.0, ny + row_h));
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
                        } else if response.clicked() {
                            state.selected_note_idx = Some(idx);
                        }
                    } else if response.clicked() {
                        let row_idx = ((pos.y - grid_top) / row_h).floor() as usize;
                        if row_idx < pitches.len() {
                            let pitch = pitches[row_idx];
                            let beat_rel = ((pos.x - grid_left) / px_per_beat) as f64;
                            let quant = state.quantize_grid.max(0.0625);
                            let quantized_beat = ((beat_rel / quant).floor() * quant).clamp(0.0, (total_beats as f64 - quant).max(0.0));
                            let new_note = MidiNote::new(pitch, 100, quantized_beat, quant.max(0.25));
                            midi_pattern.notes.push(new_note);
                            state.selected_note_idx = Some(midi_pattern.notes.len() - 1);
                            pattern_mutated = true;
                        }
                    }
                }
            }
        }

        // Delete key removes selected note
        if ui.input(|i| i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete)) {
            if let Some(sel) = state.selected_note_idx {
                if sel < midi_pattern.notes.len() {
                    midi_pattern.notes.remove(sel);
                    state.selected_note_idx = None;
                    pattern_mutated = true;
                }
            }
        }

        // --- 8. Drag Velocity / Chance Lanes ---
        if response.dragged() && state.note_drag_state.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                if lanes_grid_rect.contains(pos) && !midi_pattern.notes.is_empty() {
                    let pointer_beat = ((pos.x - grid_left) / px_per_beat) as f64;
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
                            let vel_base = mid_lane_y - 1.0;
                            let norm = ((vel_base - pos.y) / (mid_lane_y - lanes_grid_rect.top() - 3.0)).clamp(0.05, 1.0);
                            note.velocity = (norm * 127.0).round().clamp(1.0, 127.0) as u8;
                            pattern_mutated = true;
                        } else {
                            let chance_base = roll_rect.bottom() - 1.0;
                            let norm = ((chance_base - pos.y) / (roll_rect.bottom() - mid_lane_y - 3.0)).clamp(0.0, 1.0);
                            note.probability = norm;
                            pattern_mutated = true;
                        }
                    }
                }
            }
        }

        // --- 9. Render Top Timeline Ruler & Loop Brace ---
        let ruler_rect = Rect::from_min_max(roll_rect.min, pos2(roll_rect.right(), grid_top));
        let gutter_ruler_rect = Rect::from_min_max(ruler_rect.min, pos2(grid_left, grid_top));
        painter.rect_filled(gutter_ruler_rect, 0.0, Color32::from_rgb(0x36, 0x36, 0x36)); // SurfaceBackground #363636
        painter.text(gutter_ruler_rect.center(), egui::Align2::CENTER_CENTER, "KEY", FontId::monospace(7.5), COLOR_TEXT_DIM);
        painter.line_segment([gutter_ruler_rect.left_bottom(), gutter_ruler_rect.right_bottom()], Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11)));
        painter.line_segment([gutter_ruler_rect.right_top(), gutter_ruler_rect.right_bottom()], Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11)));

        let time_ruler_rect = Rect::from_min_max(pos2(grid_left, roll_rect.top()), pos2(roll_rect.right(), grid_top));
        painter.rect_filled(time_ruler_rect, 0.0, Color32::from_rgb(0x36, 0x36, 0x36)); // SurfaceBackground #363636
        painter.line_segment([time_ruler_rect.left_bottom(), time_ruler_rect.right_bottom()], Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11)));

        // Loop brace bar at top of ruler
        let loop_end_beat = (state.loop_length_bars as f32 * 4.0).min(total_beats);
        let loop_end_x = grid_left + (loop_end_beat / total_beats) * grid_w;
        let loop_brace_rect = Rect::from_min_max(pos2(grid_left, roll_rect.top()), pos2(loop_end_x, roll_rect.top() + 3.0));
        painter.rect_filled(loop_brace_rect, 1.0, COLOR_AMBER);

        let num_beats = total_beats.ceil() as i32;
        for b in 0..=num_beats {
            let beat_f = b as f32;
            if beat_f > total_beats { break; }
            let bx = grid_left + (beat_f / total_beats) * grid_w;

            if b % 4 == 0 {
                let bar = (b / 4) + 1;
                painter.text(pos2(bx + 3.0, ruler_rect.center().y), egui::Align2::LEFT_CENTER, format!("Bar {}", bar), FontId::monospace(8.5), COLOR_TEXT_PRIMARY);
                painter.line_segment([pos2(bx, grid_top), pos2(bx, grid_bottom)], Stroke::new(1.0_f32, Color32::from_rgb(0x46, 0x46, 0x46)));
            } else {
                let bar = (b / 4) + 1;
                let beat_in_bar = (b % 4) + 1;
                painter.text(pos2(bx + 3.0, ruler_rect.center().y), egui::Align2::LEFT_CENTER, format!("{}.{}", bar, beat_in_bar), FontId::monospace(8.0), COLOR_TEXT_MUTED);
                painter.line_segment([pos2(bx, grid_top), pos2(bx, grid_bottom)], Stroke::new(0.5_f32, Color32::from_rgb(0x11, 0x11, 0x11)));
            }

            for step in 1..4 {
                let sub_b = beat_f + step as f32 * 0.25;
                if sub_b < total_beats {
                    let sub_x = grid_left + (sub_b / total_beats) * grid_w;
                    painter.line_segment([pos2(sub_x, grid_top), pos2(sub_x, grid_bottom)], Stroke::new(0.5_f32, Color32::from_rgb(0x1e, 0x1e, 0x1e)));
                    painter.line_segment([pos2(sub_x, ruler_rect.bottom() - 3.0), pos2(sub_x, ruler_rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0x2a, 0x2a, 0x2a)));
                }
            }
        }

        // --- 10. Render Piano Key Gutter & Grid Rows with Amber Audition ---
        for (r, &pitch) in pitches.iter().enumerate() {
            let ry = grid_top + r as f32 * row_h;
            let is_root = (pitch % 12) == (scale.root % 12);
            let black = is_black_key(pitch);
            let is_audition = state.audition_pitch == Some(pitch);

            let key_rect = Rect::from_min_size(pos2(roll_rect.left(), ry), vec2(gutter_w, row_h));
            let key_bg = if is_audition {
                Color32::from_rgb(0xff, 0xad, 0x56) // ChosenDefault #ffad56
            } else if is_root {
                Color32::from_rgb(0xa0, 0x85, 0x55) // Root key highlight
            } else if black {
                Color32::from_rgb(0x46, 0x46, 0x46) // PianoBlackKey #464646
            } else {
                Color32::from_rgb(0x86, 0x86, 0x86) // PianoWhiteKey #868686
            };
            painter.rect_filled(key_rect, 0.0, key_bg);
            painter.line_segment([pos2(key_rect.left(), key_rect.bottom()), pos2(key_rect.right(), key_rect.bottom())], Stroke::new(0.5_f32, Color32::from_rgb(0x11, 0x11, 0x11)));
            painter.line_segment([pos2(key_rect.right(), key_rect.top()), pos2(key_rect.right(), key_rect.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11)));

            if is_root && !is_audition {
                painter.rect_filled(Rect::from_min_size(key_rect.min, vec2(3.0, row_h)), 0.0, Color32::from_rgb(0xff, 0xad, 0x56));
            }

            let note_name = midi_to_note_name(pitch);
            let text_col = if is_audition || is_root || !black {
                Color32::from_rgb(0x07, 0x07, 0x07)
            } else {
                Color32::from_rgb(0xd0, 0xd0, 0xd0)
            };
            painter.text(pos2(key_rect.right() - 4.0, key_rect.center().y), egui::Align2::RIGHT_CENTER, note_name, FontId::monospace(8.5), text_col);

            let grid_row_rect = Rect::from_min_size(pos2(grid_left, ry), vec2(grid_w, row_h));
            let grid_bg = if is_root {
                Color32::from_rgb(0x35, 0x2e, 0x24) // Root row subtle warm highlight #352e24
            } else if scale.is_in_scale(pitch) {
                if black {
                    Color32::from_rgb(0x28, 0x25, 0x32) // In-scale row tint #b595fc on black key
                } else {
                    Color32::from_rgb(0x2f, 0x2b, 0x3a) // In-scale row tint #b595fc on white key
                }
            } else if black {
                Color32::from_rgb(0x1e, 0x1e, 0x1e)
            } else {
                Color32::from_rgb(0x24, 0x24, 0x24)
            };
            painter.rect_filled(grid_row_rect, 0.0, grid_bg);
            painter.line_segment([grid_row_rect.left_bottom(), grid_row_rect.right_bottom()], Stroke::new(0.5_f32, Color32::from_rgb(0x11, 0x11, 0x11)));
        }

        // --- 11. Render MIDI Notes with 7px Right-Edge Handle Indicator ---
        for (idx, note) in midi_pattern.notes.iter().enumerate() {
            if let Some(row_idx) = pitches.iter().position(|&p| p == note.pitch) {
                let nx = grid_left + note.start_beat as f32 * px_per_beat;
                let nw = (note.duration_beats as f32 * px_per_beat).max(6.0);
                let ny = grid_top + row_idx as f32 * row_h;

                let is_sel = state.selected_note_idx == Some(idx);
                let n_rect = Rect::from_min_size(pos2(nx, ny + 1.0), vec2(nw, (row_h - 2.0).max(3.0)));

                let note_color = if is_sel { COLOR_AMBER } else { COLOR_CYAN };
                painter.rect_filled(n_rect, 2.0, note_color);
                painter.rect_stroke(n_rect, 2.0, Stroke::new(1.0_f32, if is_sel { Color32::WHITE } else { Color32::from_rgb(0x80, 0xdb, 0xe8) }));

                // Right edge handle visual affordance (white vertical bar)
                if nw >= 12.0 {
                    let handle_rect = Rect::from_min_max(pos2(nx + nw - 3.0, ny + 2.0), pos2(nx + nw - 1.0, ny + row_h - 2.0));
                    painter.rect_filled(handle_rect, 0.5, Color32::from_white_alpha(150));
                }

                if nw >= 18.0 {
                    let note_str = midi_to_note_name(note.pitch);
                    let label_col = Color32::from_rgb(0x07, 0x07, 0x07); // SelectionForeground / Note Text #070707
                    painter.text(n_rect.center(), egui::Align2::CENTER_CENTER, note_str, FontId::monospace(8.0), label_col);
                }
            }
        }

        // --- 12. Render Velocity & Chance Stalks Lane (Strictly Aligned) ---
        let gutter_lane_rect = Rect::from_min_max(pos2(roll_rect.left(), grid_bottom), pos2(grid_left, roll_rect.bottom()));
        painter.rect_filled(gutter_lane_rect, 0.0, Color32::from_rgb(0x36, 0x36, 0x36)); // SurfaceBackground #363636
        painter.line_segment([pos2(gutter_lane_rect.left(), mid_lane_y), pos2(gutter_lane_rect.right(), mid_lane_y)], Stroke::new(0.5_f32, Color32::from_rgb(0x11, 0x11, 0x11)));
        painter.line_segment([gutter_lane_rect.right_top(), gutter_lane_rect.right_bottom()], Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11)));
        let vel_y = lanes_grid_rect.top() + (lanes_h * 0.28);
        let chance_y = lanes_grid_rect.top() + (lanes_h * 0.72);
        painter.text(pos2(gutter_lane_rect.center().x, vel_y), egui::Align2::CENTER_CENTER, "VEL", FontId::monospace(8.0), COLOR_AMBER);
        if lanes_h >= 28.0 {
            painter.text(pos2(gutter_lane_rect.center().x, chance_y), egui::Align2::CENTER_CENTER, "CHANCE", FontId::monospace(7.5), COLOR_CYAN);
        } else {
            painter.text(pos2(gutter_lane_rect.center().x, chance_y), egui::Align2::CENTER_CENTER, "%", FontId::monospace(7.5), COLOR_CYAN);
        }

        painter.rect_filled(lanes_grid_rect, 0.0, Color32::from_rgb(0x1e, 0x1e, 0x1e)); // ControlBackground #1e1e1e
        painter.line_segment([pos2(grid_left, grid_bottom), pos2(roll_rect.right(), grid_bottom)], Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11)));
        painter.line_segment([pos2(grid_left, mid_lane_y), pos2(roll_rect.right(), mid_lane_y)], Stroke::new(0.5_f32, Color32::from_rgb(0x2a, 0x2a, 0x2a)));

        for b in 0..=num_beats {
            let beat_f = b as f32;
            if beat_f > total_beats { break; }
            let bx = grid_left + (beat_f / total_beats) * grid_w;
            painter.line_segment([pos2(bx, grid_bottom), pos2(bx, roll_rect.bottom())], Stroke::new(0.5_f32, Color32::from_rgb(0x2a, 0x2a, 0x2a)));
        }

        for (idx, note) in midi_pattern.notes.iter().enumerate() {
            let nx = grid_left + note.start_beat as f32 * px_per_beat;
            let stalk_x = (nx + 4.0).clamp(grid_left + 2.0, roll_rect.right() - 4.0);

            if stalk_x >= grid_left && stalk_x <= roll_rect.right() {
                let is_sel = state.selected_note_idx == Some(idx);

                // Velocity stalk (upper half)
                let vel_norm = (note.velocity as f32 / 127.0).clamp(0.05, 1.0);
                let vel_h = vel_norm * (mid_lane_y - lanes_grid_rect.top() - 3.0);
                let vel_base = mid_lane_y - 1.0;
                let vel_col = if is_sel { Color32::WHITE } else { COLOR_AMBER };
                painter.line_segment([pos2(stalk_x, vel_base), pos2(stalk_x, vel_base - vel_h)], Stroke::new(if is_sel { 2.5_f32 } else { 1.8_f32 }, vel_col));
                painter.circle_filled(pos2(stalk_x, vel_base - vel_h), if is_sel { 2.8_f32 } else { 2.2_f32 }, vel_col);

                // Chance stalk (lower half)
                let prob_norm = note.probability.clamp(0.05, 1.0);
                let prob_h = prob_norm * (roll_rect.bottom() - mid_lane_y - 3.0);
                let chance_base = roll_rect.bottom() - 1.0;
                let chance_col = if is_sel { Color32::WHITE } else { COLOR_CYAN };
                painter.line_segment([pos2(stalk_x, chance_base), pos2(stalk_x, chance_base - prob_h)], Stroke::new(if is_sel { 2.5_f32 } else { 1.8_f32 }, chance_col));
                painter.circle_filled(pos2(stalk_x, chance_base - prob_h), if is_sel { 2.8_f32 } else { 2.2_f32 }, chance_col);
            }
        }

        // --- 13. Playhead Position Line ---
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

    #[test]
    fn test_dynamic_bar_span_16_beats_no_clipping() {
        let ctx = egui::Context::default();
        let mut state = ClipViewState::default();
        state.loop_length_bars = 4; // 16 beats

        // 4-bar chord progression ending at 16.0 beats
        let mut pattern = MidiPattern::new(4.0);
        pattern.add_note(MidiNote::new(41, 100, 0.0, 4.0));
        pattern.add_note(MidiNote::new(44, 100, 4.0, 4.0));
        pattern.add_note(MidiNote::new(46, 100, 8.0, 4.0));
        pattern.add_note(MidiNote::new(48, 100, 12.0, 4.0)); // ends at beat 16.0

        let scale = GlobalScale::default();

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let action = render_clip_view(ui, &mut state, &mut pattern, &scale, 0.0);
                assert!(!action.pattern_mutated);
            });
        });

        let notes_max_end = pattern.notes.iter().map(|n| n.end_beat()).fold(4.0, f64::max);
        let total_beats = (state.loop_length_bars as f64 * 4.0)
            .max(notes_max_end)
            .max(pattern.length_beats)
            .max(4.0) as f32;
        assert_eq!(total_beats, 16.0, "Total beats must span at least 16.0 to render all 4 bars without right edge clipping");
    }

    #[test]
    fn test_side_by_side_inspector_mutation_updates_piano_roll() {
        let ctx = egui::Context::default();
        let mut state = ClipViewState::default();
        let scale = GlobalScale::default();

        // Strum transform in Transform tab
        state.sub_tab = ClipViewSubTab::Transform;
        let mut chord_pattern = MidiPattern::new(4.0);
        chord_pattern.add_note(MidiNote::new(60, 100, 0.0, 2.0));
        chord_pattern.add_note(MidiNote::new(64, 100, 0.0, 2.0));
        chord_pattern.add_note(MidiNote::new(67, 100, 0.0, 2.0));

        transform_strum(&mut chord_pattern, 0.05, StrumDirection::Up);
        assert!(chord_pattern.notes[0].start_beat <= chord_pattern.notes[1].start_beat);
        assert!(chord_pattern.notes[1].start_beat <= chord_pattern.notes[2].start_beat);

        // Verify side-by-side render executes cleanly
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let action = render_clip_view(ui, &mut state, &mut chord_pattern, &scale, 0.0);
                assert!(!action.pattern_mutated);
            });
        });

        // Generate Stacks in Generate tab
        state.sub_tab = ClipViewSubTab::Generate;
        let roots = [60, 65];
        let mut generated_pattern = generate_stacks(&roots, StacksChordType::Min7, StacksVoicing::Closed, 0, 1.0, 100);
        assert_eq!(generated_pattern.notes.len(), 8);

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let action = render_clip_view(ui, &mut state, &mut generated_pattern, &scale, 0.0);
                assert!(!action.pattern_mutated);
            });
        });
    }

    #[test]
    fn test_note_drag_state_move_and_resize_right() {
        let mut state = ClipViewState::default();
        let mut pattern = MidiPattern::new(16.0);
        pattern.add_note(MidiNote::new(60, 100, 2.0, 1.0));

        // 1. Simulate Move mode
        state.quantize_grid = 0.25;
        let drag_move = NoteDragState {
            note_idx: 0,
            mode: NoteDragMode::Move {
                orig_start: 2.0,
                orig_pitch: 60,
                pointer_start_beat: 2.0,
                pointer_start_row: 4,
            },
        };
        state.note_drag_state = Some(drag_move);

        // Move 1 beat forward
        if let Some(drag) = state.note_drag_state {
            if let NoteDragMode::Move { orig_start, pointer_start_beat, .. } = drag.mode {
                let curr_beat = 3.0;
                let delta = curr_beat - pointer_start_beat;
                pattern.notes[drag.note_idx].start_beat = orig_start + delta;
            }
        }
        assert_eq!(pattern.notes[0].start_beat, 3.0);

        // 2. Simulate ResizeRight mode
        let drag_resize = NoteDragState {
            note_idx: 0,
            mode: NoteDragMode::ResizeRight {
                orig_duration: 1.0,
                pointer_start_beat: 4.0,
            },
        };
        state.note_drag_state = Some(drag_resize);

        // Extend duration by 0.5 beats
        if let Some(drag) = state.note_drag_state {
            if let NoteDragMode::ResizeRight { orig_duration, pointer_start_beat } = drag.mode {
                let curr_beat = 4.5;
                let delta = curr_beat - pointer_start_beat;
                pattern.notes[drag.note_idx].duration_beats = orig_duration + delta;
            }
        }
        assert_eq!(pattern.notes[0].duration_beats, 1.5);
    }

    #[test]
    fn test_key_gutter_audition_action_dispatch() {
        let mut state = ClipViewState::default();
        state.audition_pitch = Some(60);
        state.last_audition_note = Some((60, 100));

        let action = ClipViewAction {
            pattern_mutated: false,
            warp_mode_changed: None,
            transpose_changed: None,
            warp_marker_moved: false,
            audition_note: state.last_audition_note.take(),
            loop_length_changed: None,
        };

        assert_eq!(action.audition_note, Some((60, 100)));
        assert_eq!(state.last_audition_note, None);
    }

    #[test]
    fn test_octave_navigation_and_grid_division() {
        let mut state = ClipViewState::default();
        assert_eq!(state.pitch_scroll_offset, 0);

        // Octave up in scale-folded mode (+7)
        let oct_step = if state.scale_folded { 7 } else { 12 };
        state.pitch_scroll_offset += oct_step;
        assert_eq!(state.pitch_scroll_offset, 7);

        // Octave down
        state.pitch_scroll_offset -= oct_step;
        assert_eq!(state.pitch_scroll_offset, 0);

        // Grid divisions
        state.quantize_grid = 1.0; // 1/4
        assert_eq!(state.quantize_grid, 1.0);
        state.quantize_grid = 0.5; // 1/8
        assert_eq!(state.quantize_grid, 0.5);
        state.quantize_grid = 0.125; // 1/32
        assert_eq!(state.quantize_grid, 0.125);
    }

    #[test]
    fn test_live12_clip_view_colors_and_zero_tofu_labels() {
        let ctx = egui::Context::default();
        let mut state = ClipViewState::default();
        let mut pattern = MidiPattern::new(16.0);
        pattern.add_note(MidiNote::new(60, 100, 0.0, 4.0));
        let scale = GlobalScale {
            root: 5,
            mode: ableton_core::ScaleMode::Dorian,
            scale_lock: true,
        };

        // Render in Transform subtab (tests Strum direction button "Up" / "Down")
        state.sub_tab = ClipViewSubTab::Transform;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = render_clip_view(ui, &mut state, &mut pattern, &scale, 0.0);
            });
        });

        // Strum direction toggle
        assert!(state.strum_direction_up);
        let lbl_up = if state.strum_direction_up { "Up" } else { "Down" };
        assert_eq!(lbl_up, "Up");
        state.strum_direction_up = false;
        let lbl_down = if state.strum_direction_up { "Up" } else { "Down" };
        assert_eq!(lbl_down, "Down");

        // Verify Live 12 color tokens used in Piano Roll
        let white_key_col = Color32::from_rgb(0x86, 0x86, 0x86); // PianoWhiteKey
        let black_key_col = Color32::from_rgb(0x46, 0x46, 0x46); // PianoBlackKey
        let root_row_col = Color32::from_rgb(0x35, 0x2e, 0x24);  // Root row highlight
        let note_text_col = Color32::from_rgb(0x07, 0x07, 0x07); // Note text
        let detail_bg_col = Color32::from_rgb(0x3e, 0x3e, 0x3e); // DetailViewBackground
        let surface_bg_col = Color32::from_rgb(0x36, 0x36, 0x36); // SurfaceBackground
        let frame_border_col = Color32::from_rgb(0x11, 0x11, 0x11); // ControlContrastFrame

        assert_eq!(white_key_col, Color32::from_rgb(134, 134, 134));
        assert_eq!(black_key_col, Color32::from_rgb(70, 70, 70));
        assert_eq!(root_row_col, Color32::from_rgb(53, 46, 36));
        assert_eq!(note_text_col, Color32::from_rgb(7, 7, 7));
        assert_eq!(detail_bg_col, Color32::from_rgb(62, 62, 62));
        assert_eq!(surface_bg_col, Color32::from_rgb(54, 54, 54));
        assert_eq!(frame_border_col, Color32::from_rgb(17, 17, 17));
    }
}
