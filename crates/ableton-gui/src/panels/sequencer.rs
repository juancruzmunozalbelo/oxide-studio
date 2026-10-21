//! TR-909 Pro Drum Sequencer & Chromatic Piano Roll editor.
//!
//! Provides:
//! - 8 visible drum voices simultaneously with Bank A (Pads 1-8) and Bank B (Pads 9-16) switching
//! - Responsive full-width step buttons dynamically scaling across available panel width
//! - Per-step velocity fill gauge + Right-click / Shift-click Accent toggle (Amber Accent glow)
//! - Per-voice Mute (M), Solo (S), and Audition (►) controls on each pad strip
//! - Right-hand Groove & Pattern bar: Swing % slider, Velocity Humanize slider,
//!   quick-action pattern buttons (4-on-the-Floor, RKT Tresillo, Techno 16ths, Clear, Randomize)
//! - Backward-compatible `render_drum_sequencer` and `render_piano_roll`

use egui::{
    pos2, vec2, Align2, Button, Color32, FontFamily, FontId, Rect, RichText, Sense, Slider,
    Stroke, Ui,
};
use ableton_timeline::{DrumStepPattern, DrumStepTrack, MidiPattern};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_DIVIDER, COLOR_GREEN, COLOR_RUBY, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

/// Drum bank selection: Bank A (Pads 1-8) vs Bank B (Pads 9-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumBank {
    BankA, // Pads 1..8
    BankB, // Pads 9..16
}

/// Canonical Bank A pad names (Pads 1-8).
pub const BANK_A_PAD_NAMES: [&str; 8] = [
    "Kick", "Snare", "Low Tom", "Mid Tom",
    "Hi Tom", "Rimshot", "Clap", "Cowbell",
];

/// Canonical Bank B pad names (Pads 9-16).
pub const BANK_B_PAD_NAMES: [&str; 8] = [
    "Closed HH", "Open HH", "Crash", "Ride",
    "Perc 1", "Perc 2", "Shaker", "Tambourine",
];

/// Persistent UI & Groove state for the TR-909 Pro Drum Sequencer.
#[derive(Debug, Clone)]
pub struct DrumSequencerState {
    pub bank: DrumBank,
    pub muted_pads: [bool; 16],
    pub soloed_pads: [bool; 16],
    pub swing_percent: f32,    // 0.0 .. 75.0%
    pub humanize_percent: f32, // 0.0 .. 100.0%
}

impl Default for DrumSequencerState {
    fn default() -> Self {
        Self {
            bank: DrumBank::BankA,
            muted_pads: [false; 16],
            soloed_pads: [false; 16],
            swing_percent: 18.0,
            humanize_percent: 10.0,
        }
    }
}

impl DrumSequencerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_bank(&mut self) {
        self.bank = match self.bank {
            DrumBank::BankA => DrumBank::BankB,
            DrumBank::BankB => DrumBank::BankA,
        };
    }

    pub fn toggle_mute(&mut self, pad_idx: usize) {
        if pad_idx < 16 {
            self.muted_pads[pad_idx] = !self.muted_pads[pad_idx];
        }
    }

    pub fn toggle_solo(&mut self, pad_idx: usize) {
        if pad_idx < 16 {
            self.soloed_pads[pad_idx] = !self.soloed_pads[pad_idx];
        }
    }

    pub fn is_pad_audible(&self, pad_idx: usize) -> bool {
        let any_solo = self.soloed_pads.iter().any(|&s| s);
        if any_solo {
            self.soloed_pads.get(pad_idx).copied().unwrap_or(false)
        } else {
            !self.muted_pads.get(pad_idx).copied().unwrap_or(false)
        }
    }

    /// Clear all active steps for the 8 pads in the active bank.
    pub fn clear_active_bank(&self, pattern: &mut DrumStepPattern) {
        let (start, end) = match self.bank {
            DrumBank::BankA => (0, 8),
            DrumBank::BankB => (8, 16),
        };
        for pad_idx in start..end {
            if let Some(track) = pattern.tracks.get_mut(pad_idx) {
                for step in &mut track.steps {
                    step.active = false;
                    step.accent = false;
                }
            }
        }
    }

    /// Clear all active steps across all 16 pads.
    pub fn clear_all(&self, pattern: &mut DrumStepPattern) {
        for track in &mut pattern.tracks {
            for step in &mut track.steps {
                step.active = false;
                step.accent = false;
            }
        }
    }

    /// Applies classic 4-on-the-Floor house/techno groove.
    pub fn apply_four_on_the_floor(&self, pattern: &mut DrumStepPattern) {
        ensure_16_tracks(pattern);

        // Kick on 0, 4, 8, 12 (accented)
        if let Some(trk) = pattern.tracks.get_mut(0) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx % 4 == 0 {
                    step.active = true;
                    step.accent = true;
                    step.velocity = 127;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Snare on 4, 12 (backbeat)
        if let Some(trk) = pattern.tracks.get_mut(1) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 4 || idx == 12 {
                    step.active = true;
                    step.accent = true;
                    step.velocity = 115;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Clap on 4, 12 (layer)
        if let Some(trk) = pattern.tracks.get_mut(6) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 4 || idx == 12 {
                    step.active = true;
                    step.accent = false;
                    step.velocity = 105;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Closed HH on offbeats 2, 6, 10, 14
        if let Some(trk) = pattern.tracks.get_mut(8) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx % 4 == 2 {
                    step.active = true;
                    step.accent = false;
                    step.velocity = 95;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Open HH on 2, 10
        if let Some(trk) = pattern.tracks.get_mut(9) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 2 || idx == 10 {
                    step.active = true;
                    step.accent = false;
                    step.velocity = 90;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }
    }

    /// Applies Argentine RKT / Turreo Dembow Tresillo rhythm (3-3-2 syncopation).
    pub fn apply_rkt_tresillo(&self, pattern: &mut DrumStepPattern) {
        ensure_16_tracks(pattern);

        // Kick on 0, 6, 10 (3-3-2 tresillo syncopation)
        if let Some(trk) = pattern.tracks.get_mut(0) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 0 || idx == 6 || idx == 10 {
                    step.active = true;
                    step.accent = idx == 0 || idx == 6;
                    step.velocity = if step.accent { 127 } else { 118 };
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Snare on 3, 7, 11, 15 (defining dembow syncopation)
        if let Some(trk) = pattern.tracks.get_mut(1) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 3 || idx == 7 || idx == 11 || idx == 15 {
                    step.active = true;
                    step.accent = idx == 3 || idx == 11;
                    step.velocity = if step.accent { 122 } else { 110 };
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Rimshot on 3, 7, 11, 15
        if let Some(trk) = pattern.tracks.get_mut(5) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 3 || idx == 7 || idx == 11 || idx == 15 {
                    step.active = true;
                    step.accent = false;
                    step.velocity = 98;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Closed HH on 0, 2, 3, 6, 8, 10, 11, 14
        if let Some(trk) = pattern.tracks.get_mut(8) {
            let hat_steps = [0, 2, 3, 6, 8, 10, 11, 14];
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if hat_steps.contains(&idx) {
                    step.active = true;
                    step.accent = idx == 0 || idx == 6;
                    step.velocity = if step.accent { 108 } else { 92 };
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Cowbell on 0, 6, 12
        if let Some(trk) = pattern.tracks.get_mut(7) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 0 || idx == 6 || idx == 12 {
                    step.active = true;
                    step.accent = false;
                    step.velocity = 85;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }
    }

    /// Applies driving Berlin Techno 16th rolling groove.
    pub fn apply_techno_16ths(&self, pattern: &mut DrumStepPattern) {
        ensure_16_tracks(pattern);

        // Kick on 0, 4, 8, 12 (heavy accent)
        if let Some(trk) = pattern.tracks.get_mut(0) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx % 4 == 0 {
                    step.active = true;
                    step.accent = true;
                    step.velocity = 127;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Rolling 16th Closed Hats on all 16 steps with offbeat accents
        if let Some(trk) = pattern.tracks.get_mut(8) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                step.active = true;
                let is_offbeat = idx % 4 == 2;
                step.accent = is_offbeat;
                step.velocity = if is_offbeat { 120 } else { 85 };
            }
        }

        // Open HH on 2, 6, 10, 14
        if let Some(trk) = pattern.tracks.get_mut(9) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx % 4 == 2 {
                    step.active = true;
                    step.accent = false;
                    step.velocity = 105;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Clap on 4, 12
        if let Some(trk) = pattern.tracks.get_mut(6) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx == 4 || idx == 12 {
                    step.active = true;
                    step.accent = true;
                    step.velocity = 115;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }

        // Low Tom rumble on 3, 7, 11, 15
        if let Some(trk) = pattern.tracks.get_mut(2) {
            for (idx, step) in trk.steps.iter_mut().enumerate() {
                if idx % 4 == 3 {
                    step.active = true;
                    step.accent = false;
                    step.velocity = 95;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }
    }

    /// Randomize pattern using musically informed density weights and humanization.
    pub fn randomize_pattern(&self, pattern: &mut DrumStepPattern) {
        ensure_16_tracks(pattern);
        let mut seed: u64 = 0x909_1234_5678 ^ (self.humanize_percent as u64 * 31);
        let mut next_rand = || -> f32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 32) as u32 as f32) / (u32::MAX as f32)
        };

        for (pad_idx, track) in pattern.tracks.iter_mut().enumerate() {
            let density = match pad_idx {
                0 => 0.40, // Kick
                1 => 0.25, // Snare
                2..=4 => 0.20, // Toms
                5 => 0.25, // Rimshot
                6 => 0.20, // Clap
                7 => 0.15, // Cowbell
                8 => 0.70, // Closed HH
                9 => 0.25, // Open HH
                _ => 0.18,
            };

            for (step_idx, step) in track.steps.iter_mut().enumerate() {
                let r = next_rand();
                let is_downbeat = step_idx % 4 == 0;
                let is_offbeat = step_idx % 4 == 2;
                let boost = if pad_idx == 0 && is_downbeat {
                    0.45
                } else if (pad_idx == 1 || pad_idx == 6) && (step_idx == 4 || step_idx == 12) {
                    0.55
                } else if pad_idx == 8 && is_offbeat {
                    0.35
                } else {
                    0.0
                };

                if r < (density + boost) {
                    step.active = true;
                    let is_acc = next_rand() < 0.25 || (pad_idx == 0 && is_downbeat);
                    step.accent = is_acc;
                    let base_vel = if is_acc { 120.0 } else { 95.0 };
                    let jitter = (next_rand() - 0.5) * (self.humanize_percent * 0.5);
                    step.velocity = (base_vel + jitter).clamp(20.0, 127.0) as u8;
                } else {
                    step.active = false;
                    step.accent = false;
                }
            }
        }
    }
}

/// Ensures the pattern contains at least 16 tracks with standard pads and choke groups.
pub fn ensure_16_tracks(drum_pattern: &mut DrumStepPattern) {
    let step_count = if drum_pattern.step_count == 0 { 16 } else { drum_pattern.step_count };
    while drum_pattern.tracks.len() < 16 {
        let idx = drum_pattern.tracks.len() as u8;
        let name = if (idx as usize) < 8 {
            BANK_A_PAD_NAMES[idx as usize]
        } else {
            BANK_B_PAD_NAMES[(idx as usize) - 8]
        };
        let pitch = 36 + idx;
        let mut trk = DrumStepTrack::new(idx, name, step_count, pitch);
        if idx == 8 || idx == 9 {
            // Closed & Open Hi-Hats share choke group 1
            for s in &mut trk.steps {
                s.choke_group = Some(1);
            }
        }
        drum_pattern.tracks.push(trk);
    }
}

/// Action produced by sequencer user interactions.
#[derive(Debug, Clone, Default)]
pub struct SequencerAction {
    pub step_toggled: Option<(usize, usize)>, // (pad_idx, step_idx)
    pub pad_triggered: Option<u8>,           // pad_idx 0..15 (MIDI 36..51)
    pub pattern_mutated: bool,
    pub swing_changed: Option<f32>,
    pub humanize_changed: Option<f32>,
}

/// Main entry point: renders the TR-909 Pro Drum Sequencer with automatic UI state persistence.
pub fn render_drum_sequencer(
    ui: &mut Ui,
    drum_pattern: &mut DrumStepPattern,
    current_step: usize,
) -> SequencerAction {
    let state_id = egui::Id::new("aruston_909_sequencer_state");
    let mut state: DrumSequencerState = ui.data_mut(|d| d.get_temp(state_id).unwrap_or_default());
    let action = render_drum_sequencer_with_state(ui, drum_pattern, current_step, &mut state);
    ui.data_mut(|d| d.insert_temp(state_id, state));
    action
}

/// Explicit-state entry point: renders the TR-909 Pro Drum Sequencer with the provided state.
pub fn render_drum_sequencer_with_state(
    ui: &mut Ui,
    drum_pattern: &mut DrumStepPattern,
    current_step: usize,
    state: &mut DrumSequencerState,
) -> SequencerAction {
    ensure_16_tracks(drum_pattern);

    let mut action = SequencerAction::default();

    let total_width = ui.available_width();
    let total_height = ui.available_height().max(136.0);
    let groove_bar_width = 188.0_f32;
    let main_width = (total_width - groove_bar_width - 14.0).max(360.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
        // --- Left Main Column: 8 Visible Voice Rows & Header ---
        ui.allocate_ui_with_layout(
            vec2(main_width, total_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 2.0);
                // Header Toolbar: Title, Bank Selectors, Step counter
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("TR-909 PRO SEQUENCER")
                            .size(11.0)
                            .color(COLOR_AMBER)
                            .strong(),
                    );

                    ui.add_space(6.0);

                    // Bank A Toggle
                    let bank_a_active = state.bank == DrumBank::BankA;
                    let bank_a_text = RichText::new("Bank A (1-8)").size(9.5).color(
                        if bank_a_active {
                            COLOR_AMBER
                        } else {
                            COLOR_TEXT_MUTED
                        },
                    );
                    let bank_a_btn = Button::new(bank_a_text)
                        .min_size(vec2(72.0, 18.0))
                        .fill(if bank_a_active {
                            Color32::from_rgb(0x22, 0x1c, 0x10)
                        } else {
                            COLOR_BG_PANEL_ALT
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if bank_a_active {
                                COLOR_AMBER
                            } else {
                                COLOR_BORDER
                            },
                        ));
                    if ui.add(bank_a_btn).clicked() {
                        state.bank = DrumBank::BankA;
                    }

                    // Bank B Toggle
                    let bank_b_active = state.bank == DrumBank::BankB;
                    let bank_b_text = RichText::new("Bank B (9-16)").size(9.5).color(
                        if bank_b_active {
                            COLOR_AMBER
                        } else {
                            COLOR_TEXT_MUTED
                        },
                    );
                    let bank_b_btn = Button::new(bank_b_text)
                        .min_size(vec2(72.0, 18.0))
                        .fill(if bank_b_active {
                            Color32::from_rgb(0x22, 0x1c, 0x10)
                        } else {
                            COLOR_BG_PANEL_ALT
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if bank_b_active {
                                COLOR_AMBER
                            } else {
                                COLOR_BORDER
                            },
                        ));
                    if ui.add(bank_b_btn).clicked() {
                        state.bank = DrumBank::BankB;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("Step: {:02} / 16", current_step + 1))
                                .font(FontId::new(10.5, FontFamily::Monospace))
                                .color(COLOR_GREEN),
                        );
                    });
                });

                // 8 Visible Voice Rows — Proportional Anti-Breakage Auto-Fit
                ui.spacing_mut().item_spacing.y = 1.5;
                let (pad_start, pad_end) = match state.bank {
                    DrumBank::BankA => (0, 8),
                    DrumBank::BankB => (8, 16),
                };

                let rem_rows_h = ui.available_height().max(90.0);
                let step_h = crate::layout_guard::FlexFitGrid::compute_row_height(
                    rem_rows_h,
                    8,
                    1.5,
                    2.0,
                    11.0,
                    20.0,
                );

                // Responsive step button width calculation
                let left_controls_w = 124.0_f32; // Audition (76px) + M (18px) + S (18px) + spacing
                let avail_step_w = (main_width - left_controls_w - 8.0).max(272.0);
                let step_w = crate::layout_guard::FlexFitGrid::compute_col_width(
                    avail_step_w,
                    16,
                    3.0,
                    2.0,
                    16.0,
                    56.0,
                );

                for pad_idx in pad_start..pad_end {
                    let pad_name = if pad_idx < 8 {
                        BANK_A_PAD_NAMES[pad_idx]
                    } else {
                        BANK_B_PAD_NAMES[pad_idx - 8]
                    };

                    let is_audible = state.is_pad_audible(pad_idx);
                    let is_muted = state.muted_pads[pad_idx];
                    let is_soloed = state.soloed_pads[pad_idx];

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                        // Pad Audition Button (▶ Name — verified egui glyph U+25B6)
                        let audition_label = format!("▶ {}. {}", pad_idx + 1, pad_name);
                        let pad_btn = Button::new(
                            RichText::new(audition_label)
                                .size(8.5)
                                .color(if is_audible {
                                    COLOR_TEXT_PRIMARY
                                } else {
                                    COLOR_TEXT_MUTED
                                }),
                        )
                        .min_size(vec2(76.0, step_h))
                        .fill(if is_soloed {
                            Color32::from_rgb(0x0e, 0x25, 0x33)
                        } else {
                            COLOR_BG_PANEL_ALT
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if is_soloed {
                                COLOR_CYAN
                            } else {
                                COLOR_BORDER
                            },
                        ));

                        if ui.add(pad_btn).clicked() {
                            action.pad_triggered = Some(pad_idx as u8);
                        }

                        // Mute [M] Button
                        let mute_btn = Button::new(
                            RichText::new("M")
                                .size(8.5)
                                .strong()
                                .color(if is_muted {
                                    COLOR_RUBY
                                } else {
                                    COLOR_TEXT_MUTED
                                }),
                        )
                        .min_size(vec2(18.0, step_h))
                        .fill(if is_muted {
                            Color32::from_rgb(0x38, 0x10, 0x16)
                        } else {
                            COLOR_BG_PANEL_ALT
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if is_muted {
                                COLOR_RUBY
                            } else {
                                COLOR_BORDER
                            },
                        ));

                        if ui.add(mute_btn).clicked() {
                            state.toggle_mute(pad_idx);
                            action.pattern_mutated = true;
                        }

                        // Solo [S] Button
                        let solo_btn = Button::new(
                            RichText::new("S")
                                .size(8.5)
                                .strong()
                                .color(if is_soloed {
                                    COLOR_CYAN
                                } else {
                                    COLOR_TEXT_MUTED
                                }),
                        )
                        .min_size(vec2(18.0, step_h))
                        .fill(if is_soloed {
                            Color32::from_rgb(0x0a, 0x26, 0x36)
                        } else {
                            COLOR_BG_PANEL_ALT
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if is_soloed {
                                COLOR_CYAN
                            } else {
                                COLOR_BORDER
                            },
                        ));

                        if ui.add(solo_btn).clicked() {
                            state.toggle_solo(pad_idx);
                            action.pattern_mutated = true;
                        }

                        // 16 Step Buttons
                        if let Some(track) = drum_pattern.tracks.get_mut(pad_idx) {
                            for step_idx in 0..16 {
                                let step = track.steps.get_mut(step_idx);
                                let is_active = step.as_ref().map(|s| s.active).unwrap_or(false);
                                let is_accent = step.as_ref().map(|s| s.accent).unwrap_or(false);
                                let velocity = step.as_ref().map(|s| s.velocity).unwrap_or(100);
                                let is_current = step_idx == current_step;

                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(step_w, step_h),
                                    Sense::click(),
                                );

                                if ui.is_rect_visible(rect) {
                                    let painter = ui.painter_at(rect);

                                    // Render Background
                                    let bg_color = if is_active {
                                        if is_accent {
                                            COLOR_AMBER
                                        } else {
                                            let vel_factor = (velocity as f32 / 127.0).clamp(0.2, 1.0);
                                            Color32::from_rgba_premultiplied(
                                                0x00,
                                                (0xd2 as f32 * vel_factor) as u8,
                                                (0xff as f32 * vel_factor) as u8,
                                                (140.0 + 115.0 * vel_factor) as u8,
                                            )
                                        }
                                    } else if (step_idx / 4) % 2 == 0 {
                                        COLOR_BG_SURFACE
                                    } else {
                                        Color32::from_rgb(0x2e, 0x2e, 0x2e)
                                    };

                                    let final_bg = if is_audible {
                                        bg_color
                                    } else {
                                        Color32::from_rgba_premultiplied(
                                            (bg_color.r() as f32 * 0.35) as u8,
                                            (bg_color.g() as f32 * 0.35) as u8,
                                            (bg_color.b() as f32 * 0.35) as u8,
                                            100,
                                        )
                                    };

                                    painter.rect_filled(rect, 1.5, final_bg);

                                    // Amber Accent Geometric Marker (Vector triangle, zero font dependency)
                                    if is_active && is_accent {
                                        let tri_pts = vec![
                                            pos2(rect.right() - 2.0, rect.top() + 2.0),
                                            pos2(rect.right() - 7.0, rect.top() + 2.0),
                                            pos2(rect.right() - 2.0, rect.top() + 7.0),
                                        ];
                                        painter.add(egui::Shape::convex_polygon(
                                            tri_pts,
                                            Color32::from_rgb(0x07, 0x07, 0x07),
                                            Stroke::NONE,
                                        ));
                                    }

                                    // Velocity Gauge (Level bar at bottom of button)
                                    if is_active {
                                        let vel_norm = (velocity as f32 / 127.0).clamp(0.05, 1.0);
                                        let gauge_w = (rect.width() - 4.0) * vel_norm;
                                        let gauge_rect = Rect::from_min_size(
                                            pos2(rect.left() + 2.0, rect.bottom() - 3.0),
                                            vec2(gauge_w, 2.0),
                                        );
                                        let gauge_color = if is_accent {
                                            Color32::from_rgb(0x07, 0x07, 0x07)
                                        } else {
                                            COLOR_AMBER
                                        };
                                        painter.rect_filled(gauge_rect, 1.0, gauge_color);
                                    }

                                    // Border & Playhead indication
                                    let border_color = if is_current {
                                        COLOR_GREEN
                                    } else if response.hovered() {
                                        Color32::WHITE
                                    } else if is_active && is_accent {
                                        Color32::from_rgb(0xff, 0xee, 0x88)
                                    } else {
                                        COLOR_BORDER
                                    };
                                    let stroke_w = if is_current {
                                        2.0_f32
                                    } else if is_active && is_accent {
                                        1.5_f32
                                    } else {
                                        1.0_f32
                                    };
                                    painter.rect_stroke(rect, 1.5, Stroke::new(stroke_w, border_color));

                                    // Top playhead marker on current step
                                    if is_current {
                                        let marker_rect = Rect::from_min_size(
                                            pos2(rect.left(), rect.top()),
                                            vec2(rect.width(), 2.0),
                                        );
                                        painter.rect_filled(marker_rect, 1.0, COLOR_GREEN);
                                    }

                                    // Inactive step index label
                                    if !is_active {
                                        painter.text(
                                            rect.center(),
                                            Align2::CENTER_CENTER,
                                            format!("{}", step_idx + 1),
                                            FontId::new(8.0, FontFamily::Monospace),
                                            COLOR_TEXT_MUTED,
                                        );
                                    }
                                }

                                // Handle Click & Accent Interactions
                                let is_accent_click = response.secondary_clicked()
                                    || (response.clicked() && ui.input(|i| i.modifiers.shift));

                                if is_accent_click {
                                    if let Some(s) = step {
                                        if !s.active {
                                            s.active = true;
                                            s.accent = true;
                                            s.velocity = 127;
                                        } else {
                                            s.accent = !s.accent;
                                            s.velocity = if s.accent { 127 } else { 100 };
                                        }
                                        action.pattern_mutated = true;
                                        action.step_toggled = Some((pad_idx, step_idx));
                                    }
                                } else if response.clicked() {
                                    if let Some(s) = step {
                                        s.active = !s.active;
                                        if s.active {
                                            s.accent = false;
                                            s.velocity = 100;
                                        }
                                        action.pattern_mutated = true;
                                        action.step_toggled = Some((pad_idx, step_idx));
                                    }
                                }
                            }
                        }
                    });
                }
            },
        );

        ui.separator();

        // --- Right Column: Groove & Pattern Quick Bar (Compact 2x2 Grid, Zero Overflow) ---
        ui.allocate_ui_with_layout(
            vec2(groove_bar_width, total_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 2.0);
                ui.spacing_mut().slider_width = 44.0;

                ui.label(
                    RichText::new("GROOVE & PATTERN")
                        .size(9.5)
                        .color(COLOR_AMBER)
                        .strong(),
                );

                // Combined Swing & Humanize Sliders
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Swg").size(8.5).color(COLOR_TEXT_PRIMARY));
                    let slider = Slider::new(&mut state.swing_percent, 0.0..=75.0)
                        .suffix("%")
                        .max_decimals(0)
                        .show_value(true);
                    if ui.add(slider).changed() {
                        action.swing_changed = Some(state.swing_percent);
                        action.pattern_mutated = true;
                    }

                    ui.label(RichText::new("Hum").size(8.5).color(COLOR_TEXT_PRIMARY));
                    let slider = Slider::new(&mut state.humanize_percent, 0.0..=100.0)
                        .suffix("%")
                        .max_decimals(0)
                        .show_value(true);
                    if ui.add(slider).changed() {
                        action.humanize_changed = Some(state.humanize_percent);
                        action.pattern_mutated = true;
                    }
                });

                // Compact 2x2 Quick-Action Pattern Grid + Clear Row
                let half_w = ((groove_bar_width - 10.0) * 0.5).floor();
                let full_w = groove_bar_width - 6.0;

                ui.horizontal(|ui| {
                    // 4-on-the-Floor
                    let btn_4floor = Button::new(
                        RichText::new("4-Floor")
                            .size(8.5)
                            .color(COLOR_TEXT_PRIMARY),
                    )
                    .min_size(vec2(half_w, 16.0))
                    .fill(COLOR_BG_PANEL_ALT)
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
                    if ui.add(btn_4floor).clicked() {
                        state.apply_four_on_the_floor(drum_pattern);
                        action.pattern_mutated = true;
                    }

                    // RKT Tresillo
                    let btn_rkt = Button::new(
                        RichText::new("RKT Tresillo")
                            .size(8.5)
                            .color(COLOR_AMBER)
                            .strong(),
                    )
                    .min_size(vec2(half_w, 16.0))
                    .fill(Color32::from_rgb(0x20, 0x18, 0x0a))
                    .stroke(Stroke::new(1.0_f32, COLOR_AMBER));
                    if ui.add(btn_rkt).clicked() {
                        state.apply_rkt_tresillo(drum_pattern);
                        action.pattern_mutated = true;
                    }
                });

                ui.horizontal(|ui| {
                    // Techno 16ths
                    let btn_techno = Button::new(
                        RichText::new("Techno 16th")
                            .size(8.5)
                            .color(COLOR_CYAN)
                            .strong(),
                    )
                    .min_size(vec2(half_w, 16.0))
                    .fill(Color32::from_rgb(0x0a, 0x1e, 0x28))
                    .stroke(Stroke::new(1.0_f32, COLOR_CYAN));
                    if ui.add(btn_techno).clicked() {
                        state.apply_techno_16ths(drum_pattern);
                        action.pattern_mutated = true;
                    }

                    // Randomize (Verified ★ glyph, zero tofu)
                    let btn_rand = Button::new(
                        RichText::new("★ Random")
                            .size(8.5)
                            .color(COLOR_GREEN),
                    )
                    .min_size(vec2(half_w, 16.0))
                    .fill(COLOR_BG_PANEL_ALT)
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER));
                    if ui.add(btn_rand).clicked() {
                        state.randomize_pattern(drum_pattern);
                        action.pattern_mutated = true;
                    }
                });

                // Clear Bank
                let btn_clear = Button::new(
                    RichText::new("Clear Active Bank")
                        .size(8.5)
                        .color(COLOR_TEXT_MUTED),
                )
                .min_size(vec2(full_w, 15.0))
                .fill(COLOR_BG_VOID)
                .stroke(Stroke::new(1.0_f32, COLOR_DIVIDER));
                if ui.add(btn_clear).clicked() {
                    state.clear_active_bank(drum_pattern);
                    action.pattern_mutated = true;
                }
            },
        );
    });

    action
}

/// Chromatic Piano Roll editor retained for backward compatibility.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drum_bank_toggle() {
        let mut state = DrumSequencerState::default();
        assert_eq!(state.bank, DrumBank::BankA);
        state.toggle_bank();
        assert_eq!(state.bank, DrumBank::BankB);
        state.toggle_bank();
        assert_eq!(state.bank, DrumBank::BankA);
    }

    #[test]
    fn test_pad_audibility_mute_solo() {
        let mut state = DrumSequencerState::default();
        assert!(state.is_pad_audible(0));
        assert!(state.is_pad_audible(1));

        // Mute Pad 0
        state.toggle_mute(0);
        assert!(!state.is_pad_audible(0));
        assert!(state.is_pad_audible(1));

        // Solo Pad 1: Pad 1 should be audible, all non-soloed pads muted
        state.toggle_solo(1);
        assert!(!state.is_pad_audible(0));
        assert!(state.is_pad_audible(1));
        assert!(!state.is_pad_audible(2));

        // Unsolo Pad 1: Returns to regular mute behavior
        state.toggle_solo(1);
        assert!(!state.is_pad_audible(0));
        assert!(state.is_pad_audible(1));
        assert!(state.is_pad_audible(2));
    }

    #[test]
    fn test_ensure_16_tracks() {
        let mut pattern = DrumStepPattern {
            step_count: 16,
            step_duration_beats: 0.25,
            tracks: Vec::new(),
        };
        assert_eq!(pattern.tracks.len(), 0);

        ensure_16_tracks(&mut pattern);
        assert_eq!(pattern.tracks.len(), 16);
        assert_eq!(pattern.tracks[0].name, "Kick");
        assert_eq!(pattern.tracks[1].name, "Snare");
        assert_eq!(pattern.tracks[8].name, "Closed HH");
        assert_eq!(pattern.tracks[9].name, "Open HH");
        // Verify choke group on hats
        assert_eq!(pattern.tracks[8].steps[0].choke_group, Some(1));
        assert_eq!(pattern.tracks[9].steps[0].choke_group, Some(1));
    }

    #[test]
    fn test_groove_four_on_the_floor() {
        let mut pattern = DrumStepPattern::new_16_step();
        let state = DrumSequencerState::default();

        state.apply_four_on_the_floor(&mut pattern);

        // Kick on 0, 4, 8, 12 accented
        for step_idx in 0..16 {
            let kick_step = &pattern.tracks[0].steps[step_idx];
            if step_idx % 4 == 0 {
                assert!(kick_step.active, "Kick should be active on step {}", step_idx);
                assert!(kick_step.accent, "Kick should be accented on step {}", step_idx);
                assert_eq!(kick_step.velocity, 127);
            } else {
                assert!(!kick_step.active, "Kick should be inactive on step {}", step_idx);
            }
        }

        // Snare on 4, 12
        assert!(pattern.tracks[1].steps[4].active);
        assert!(pattern.tracks[1].steps[12].active);
        assert!(!pattern.tracks[1].steps[0].active);

        // Closed HH on offbeats 2, 6, 10, 14
        assert!(pattern.tracks[8].steps[2].active);
        assert!(pattern.tracks[8].steps[6].active);
        assert!(pattern.tracks[8].steps[10].active);
        assert!(pattern.tracks[8].steps[14].active);
    }

    #[test]
    fn test_groove_rkt_tresillo() {
        let mut pattern = DrumStepPattern::new_16_step();
        let state = DrumSequencerState::default();

        state.apply_rkt_tresillo(&mut pattern);

        // Kick: 3-3-2 Dembow pattern on 0, 6, 10
        let kick_steps = &pattern.tracks[0].steps;
        assert!(kick_steps[0].active && kick_steps[0].accent);
        assert!(kick_steps[6].active && kick_steps[6].accent);
        assert!(kick_steps[10].active);
        assert!(!kick_steps[4].active);

        // Snare on 3, 7, 11, 15
        let snare_steps = &pattern.tracks[1].steps;
        assert!(snare_steps[3].active);
        assert!(snare_steps[7].active);
        assert!(snare_steps[11].active);
        assert!(snare_steps[15].active);
        assert!(!snare_steps[4].active);
    }

    #[test]
    fn test_groove_techno_16ths() {
        let mut pattern = DrumStepPattern::new_16_step();
        let state = DrumSequencerState::default();

        state.apply_techno_16ths(&mut pattern);

        // Closed HH active on all 16 steps with offbeat accents
        let hh_steps = &pattern.tracks[8].steps;
        for (idx, step) in hh_steps.iter().enumerate() {
            assert!(step.active, "Hat should be active on step {}", idx);
            if idx % 4 == 2 {
                assert!(step.accent, "Hat should be accented on offbeat step {}", idx);
                assert_eq!(step.velocity, 120);
            }
        }
    }

    #[test]
    fn test_clear_and_randomize() {
        let mut pattern = DrumStepPattern::new_16_step();
        let state = DrumSequencerState::default();

        state.apply_four_on_the_floor(&mut pattern);
        assert!(pattern.tracks[0].steps[0].active);

        state.clear_active_bank(&mut pattern);
        // Bank A pads 0..8 should now be clear
        for pad_idx in 0..8 {
            for step in &pattern.tracks[pad_idx].steps {
                assert!(!step.active);
            }
        }

        // Randomize
        state.randomize_pattern(&mut pattern);
        let any_active = pattern.tracks.iter().any(|trk| trk.steps.iter().any(|s| s.active));
        assert!(any_active, "Randomize should populate active drum steps");
    }
}
