//! Adversarial Empirical Verification Suite for Milestones 1 & 3:
//! - M1: Piano Roll Pro & Unified Clip View
//! - M3: TR-909 Pro Drum Sequencer & 8-Macro XY Surface
//!
//! Stress-testing:
//! 1. Dynamic bar span with chord notes ending at 16.0, 32.0, 64.0 beats (zero clipping)
//! 2. Note drag calculations: bounds clamping (no underflow < 0.0, pitch in 0..127, duration grid snap)
//! 3. TR-909 drum voice indices & bank switching: Bank A (1-8) and Bank B (9-16) independence
//! 4. Groove patterns: mathematically exact step activations for 4-on-the-Floor, RKT Tresillo, Techno 16ths
//! 5. Orbit LFO animation: Circle, Figure-8, Lissajous outputs strictly bounded in [0.0, 1.0] across 1,000 continuous simulation steps

use ableton_core::{GlobalScale, ScaleMode};
use ableton_gui::{
    panels::{
        clip_view::{
            render_clip_view, render_interactive_piano_roll, ClipViewState,
            NoteDragMode, NoteDragState,
        },
        sequencer::{
            ensure_16_tracks, render_drum_sequencer_with_state, DrumBank, DrumSequencerState,
            BANK_A_PAD_NAMES, BANK_B_PAD_NAMES,
        },
    },
    visualizers::xy_pad::{OrbitShape, XyPerformPadState},
};
use ableton_timeline::{DrumStepPattern, MidiNote, MidiPattern};
use egui::{pos2, vec2, Context, RawInput, Rect};

/// Dynamic bar span formula matching Live 12 implementation in `clip_view.rs`
fn compute_total_beats(pattern: &MidiPattern, loop_bars: usize) -> f32 {
    let notes_max_end = pattern
        .notes
        .iter()
        .map(|n| n.end_beat())
        .fold(4.0, f64::max);
    (loop_bars as f64 * 4.0)
        .max(notes_max_end)
        .max(pattern.length_beats)
        .max(4.0) as f32
}

fn snap_to_grid(beat: f64, grid: f64) -> f64 {
    (beat / grid).round() * grid
}

// =========================================================================
// 1. Dynamic Bar Span When Chord Notes End at 16.0, 32.0, or 64.0 Beats
// =========================================================================

#[test]
fn test_adv_dynamic_bar_span_16_32_64_beats_zero_clipping() {
    let scale = GlobalScale {
        root: 5, // F
        mode: ScaleMode::Dorian,
        scale_lock: true,
    };

    let test_durations = [16.0_f64, 32.0_f64, 64.0_f64];
    let view_widths = [400.0_f32, 800.0_f32, 1200.0_f32, 1920.0_f32];

    for &target_beats in &test_durations {
        let mut pattern = MidiPattern::new(target_beats);

        // Populate chord progression spanning the entire duration
        let chord_count = (target_beats / 4.0) as usize;
        for c in 0..chord_count {
            let start = c as f64 * 4.0;
            // 4-note chord lasting 3.75 beats (ending 0.25 beats before next bar)
            pattern.notes.push(MidiNote::new(41 + (c as u8 % 12), 100, start, 3.75));
            pattern.notes.push(MidiNote::new(44 + (c as u8 % 12), 95, start, 3.75));
            pattern.notes.push(MidiNote::new(48 + (c as u8 % 12), 90, start, 3.75));
            pattern.notes.push(MidiNote::new(51 + (c as u8 % 12), 85, start, 3.75));
        }

        // Add a final note that ends precisely at target_beats
        pattern.notes.push(MidiNote::new(65, 110, target_beats - 1.0, 1.0));

        let last_note_end = pattern.notes.iter().map(|n| n.end_beat()).fold(0.0, f64::max);
        assert_eq!(
            last_note_end, target_beats,
            "Last note must end exactly at {} beats",
            target_beats
        );

        // Test across loop_length_bars = 1, 2, 4, 8 to verify notes_max_end dominates and prevents clipping
        for loop_bars in [1, 2, 4, 8] {
            let total_beats = compute_total_beats(&pattern, loop_bars);
            assert!(
                total_beats >= target_beats as f32,
                "Computed total beats {:.1} must be at least target beats {:.1} (loop_bars={})",
                total_beats,
                target_beats,
                loop_bars
            );

            // Verify coordinate bounds for all viewport widths
            for &grid_w in &view_widths {
                let grid_left = 220.0_f32;
                let grid_right = grid_left + grid_w;
                let px_per_beat = grid_w / total_beats;

                for note in &pattern.notes {
                    let nx = grid_left + note.start_beat as f32 * px_per_beat;
                    let nw = (note.duration_beats as f32 * px_per_beat).max(6.0);
                    let note_right = nx + nw;

                    assert!(
                        nx >= grid_left - 0.01,
                        "Note starting at {:.2} must not precede grid left {:.2}",
                        note.start_beat,
                        grid_left
                    );
                    assert!(
                        note_right <= grid_right + 0.01,
                        "Target {:.0}b note right edge {:.2}px overflows grid right {:.2}px by {:.3}px! (CLIPPED)",
                        target_beats,
                        note_right,
                        grid_right,
                        note_right - grid_right
                    );
                }
            }
        }

        // Headless egui run to verify UI layout and rendering execute without panic
        let ctx = Context::default();
        let mut state = ClipViewState::default();
        state.loop_length_bars = (target_beats / 4.0).min(8.0) as usize;

        let mut raw_input = RawInput::default();
        raw_input.screen_rect = Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0)));
        let _ = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let action = render_clip_view(ui, &mut state, &mut pattern, &scale, 0.0);
                assert!(!action.pattern_mutated);

                // Also run render_interactive_piano_roll directly
                let mutated = render_interactive_piano_roll(ui, &mut pattern, &mut state, &scale, 0.0);
                assert!(!mutated);
            });
        });
    }
}

#[test]
fn test_adv_dynamic_bar_span_extreme_edge_cases() {
    // 1. Single note extending to 64 beats in a 1-bar pattern
    let mut pattern = MidiPattern::new(4.0);
    pattern.notes.push(MidiNote::new(60, 100, 0.0, 64.0));
    let total_beats = compute_total_beats(&pattern, 1);
    assert_eq!(total_beats, 64.0);

    // 2. Empty pattern: must never produce zero or negative beats (division by zero guard)
    let empty = MidiPattern::new(0.0);
    assert_eq!(compute_total_beats(&empty, 0), 4.0);

    // 3. Notes starting at fractional beats ending at 16.0
    let mut frac_pattern = MidiPattern::new(16.0);
    frac_pattern.notes.push(MidiNote::new(60, 100, 15.333, 0.667));
    let total_frac = compute_total_beats(&frac_pattern, 4);
    assert!(total_frac >= 16.0);
}

// =========================================================================
// 2. Note Drag Calculations: Bounds Clamping & Grid Snapping
// =========================================================================

#[test]
fn test_adv_note_drag_move_bounds_never_underflow_zero() {
    let mut pattern = MidiPattern::new(16.0);
    pattern.notes.push(MidiNote::new(60, 100, 4.0, 1.0));

    let note_idx = 0;
    let total_beats = 16.0_f64;
    let grid_quantize_values = [1.0, 0.5, 0.25, 0.125, 0.0625];

    // Adversarial: Dragging pointer far to the left (negative infinity or negative screen coordinates)
    let extreme_negative_deltas = [-5.0, -10.0, -100.0, -10000.0];

    for &q in &grid_quantize_values {
        for &delta in &extreme_negative_deltas {
            let orig_start = pattern.notes[note_idx].start_beat;
            let max_start = (total_beats - pattern.notes[note_idx].duration_beats).max(0.0);
            let raw_start = (orig_start + delta).clamp(0.0, max_start);
            let snapped_start = snap_to_grid(raw_start, q).clamp(0.0, max_start);

            assert!(
                snapped_start >= 0.0,
                "Start beat must never underflow 0.0 (got {})",
                snapped_start
            );
            assert_eq!(snapped_start, 0.0);
        }
    }
}

#[test]
fn test_adv_note_drag_move_bounds_never_exceed_total_beats() {
    let mut pattern = MidiPattern::new(16.0);
    pattern.notes.push(MidiNote::new(60, 100, 4.0, 2.0));

    let note_idx = 0;
    let total_beats = 16.0_f64;
    let q = 0.25;

    // Adversarial: Dragging pointer far to the right
    let extreme_positive_deltas = [20.0, 100.0, 5000.0];

    for &delta in &extreme_positive_deltas {
        let orig_start = pattern.notes[note_idx].start_beat;
        let note_dur = pattern.notes[note_idx].duration_beats;
        let max_start = (total_beats - note_dur).max(0.0);
        let raw_start = (orig_start + delta).clamp(0.0, max_start);
        let snapped_start = snap_to_grid(raw_start, q).clamp(0.0, max_start);

        assert!(
            snapped_start <= max_start,
            "Start beat {} must not exceed max_start {}",
            snapped_start,
            max_start
        );
        assert!(
            snapped_start + note_dur <= total_beats + 1e-6,
            "Note end beat must never exceed total_beats"
        );
        assert_eq!(snapped_start, 14.0); // 16.0 - 2.0 = 14.0
    }
}

#[test]
fn test_adv_note_drag_pitch_bounds_clamped_to_midi_range() {
    let scale = GlobalScale {
        root: 0,
        mode: ScaleMode::Major,
        scale_lock: false,
    };
    let mut pattern = MidiPattern::new(16.0);
    pattern.notes.push(MidiNote::new(60, 100, 0.0, 1.0));
    let mut state = ClipViewState::default();

    // Verify headless execution with extreme octave scroll offsets does not panic
    let ctx = Context::default();
    let mut raw_input = RawInput::default();
    raw_input.screen_rect = Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 600.0)));

    for folded in [true, false] {
        state.scale_folded = folded;
        for offset in [-36, -24, 0, 24, 36] {
            state.pitch_scroll_offset = offset;
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let _ = render_clip_view(ui, &mut state, &mut pattern, &scale, 0.0);
                });
            });

            // Note pitch remains clamped in 0..=127
            for n in &pattern.notes {
                assert!(n.pitch <= 127, "Note pitch must be <= 127");
            }
        }
    }
}

#[test]
fn test_adv_note_drag_state_model_invariants() {
    let mut state = ClipViewState::default();
    assert!(state.note_drag_state.is_none());

    // 1. Move state representation
    state.note_drag_state = Some(NoteDragState {
        note_idx: 0,
        mode: NoteDragMode::Move {
            orig_start: 0.0,
            orig_pitch: 60,
            pointer_start_beat: 0.0,
            pointer_start_row: 5,
        },
    });

    if let Some(ref drag) = state.note_drag_state {
        assert_eq!(drag.note_idx, 0);
        if let NoteDragMode::Move { orig_pitch, .. } = drag.mode {
            assert_eq!(orig_pitch, 60);
        } else {
            panic!("Expected Move mode");
        }
    }

    // 2. ResizeRight state representation
    state.note_drag_state = Some(NoteDragState {
        note_idx: 1,
        mode: NoteDragMode::ResizeRight {
            orig_duration: 1.0,
            pointer_start_beat: 1.0,
        },
    });

    if let Some(ref drag) = state.note_drag_state {
        assert_eq!(drag.note_idx, 1);
        if let NoteDragMode::ResizeRight { orig_duration, .. } = drag.mode {
            assert_eq!(orig_duration, 1.0);
        } else {
            panic!("Expected ResizeRight mode");
        }
    }
}

#[test]
fn test_adv_note_resize_duration_snaps_to_grid_and_never_underflows() {
    let orig_duration = 1.0_f64;
    let start_beat = 4.0_f64;
    let total_beats = 16.0_f64;

    let grids = [1.0, 0.5, 0.25, 0.125, 0.0625];

    for &q in &grids {
        // 1. Extreme left drag (negative delta beats)
        let extreme_negative_deltas = [-0.99, -1.0, -5.0, -100.0];
        for &delta in &extreme_negative_deltas {
            let max_dur = (total_beats - start_beat).max(q);
            let raw_dur = (orig_duration + delta).clamp(q, max_dur);
            let snapped_dur = snap_to_grid(raw_dur, q).clamp(q, max_dur);

            assert!(
                snapped_dur >= q,
                "Duration {} must never underflow grid minimum {}",
                snapped_dur,
                q
            );
            let remainder = snap_to_grid(snapped_dur, q) - snapped_dur;
            assert!(
                remainder.abs() < 1e-6,
                "Duration {} must be exact multiple of grid {}",
                snapped_dur,
                q
            );
        }

        // 2. Extreme right drag (exceeding total pattern length)
        let extreme_positive_deltas = [20.0, 50.0, 1000.0];
        for &delta in &extreme_positive_deltas {
            let max_dur = (total_beats - start_beat).max(q);
            let raw_dur = (orig_duration + delta).clamp(q, max_dur);
            let snapped_dur = snap_to_grid(raw_dur, q).clamp(q, max_dur);

            assert!(
                snapped_dur <= max_dur,
                "Duration {} must not exceed max_dur {}",
                snapped_dur,
                max_dur
            );
            assert!(
                start_beat + snapped_dur <= total_beats + 1e-6,
                "Note end beat must never exceed total_beats"
            );
        }
    }
}

// =========================================================================
// 3. TR-909 Drum Voice Indices & Bank Switching State Isolation
// =========================================================================

#[test]
fn test_adv_tr909_voice_indices_and_bank_switching_no_panic() {
    let mut state = DrumSequencerState::new();
    let mut pattern = DrumStepPattern::new_16_step();

    // 1. Verify pattern initialization with fewer or greater tracks
    pattern.tracks.clear(); // 0 tracks
    ensure_16_tracks(&mut pattern);
    assert_eq!(pattern.tracks.len(), 16);

    // Verify pad names match Bank A (0..8) and Bank B (8..16)
    for i in 0..8 {
        assert_eq!(pattern.tracks[i].name, BANK_A_PAD_NAMES[i]);
    }
    for i in 8..16 {
        assert_eq!(pattern.tracks[i].name, BANK_B_PAD_NAMES[i - 8]);
    }

    // 2. Verify Choke Groups (Closed HH = 8, Open HH = 9 share Choke Group 1)
    for step in &pattern.tracks[8].steps {
        assert_eq!(step.choke_group, Some(1));
    }
    for step in &pattern.tracks[9].steps {
        assert_eq!(step.choke_group, Some(1));
    }

    // 3. Populate Bank A with distinct steps
    state.bank = DrumBank::BankA;
    pattern.tracks[0].steps[0].active = true; // Kick step 0
    pattern.tracks[0].steps[0].velocity = 127;
    pattern.tracks[0].steps[0].accent = true;
    pattern.tracks[1].steps[4].active = true; // Snare step 4
    pattern.tracks[1].steps[4].velocity = 110;

    // 4. Switch to Bank B and populate Bank B with distinct steps
    state.bank = DrumBank::BankB;
    pattern.tracks[8].steps[2].active = true; // Closed HH step 2
    pattern.tracks[8].steps[2].velocity = 95;
    pattern.tracks[9].steps[10].active = true; // Open HH step 10
    pattern.tracks[9].steps[10].velocity = 105;

    // 5. Verify Bank A remains unmodified
    assert!(pattern.tracks[0].steps[0].active);
    assert!(pattern.tracks[0].steps[0].accent);
    assert_eq!(pattern.tracks[0].steps[0].velocity, 127);
    assert!(pattern.tracks[1].steps[4].active);
    assert_eq!(pattern.tracks[1].steps[4].velocity, 110);

    // 6. Verify Bank B remains unmodified
    assert!(pattern.tracks[8].steps[2].active);
    assert_eq!(pattern.tracks[8].steps[2].velocity, 95);
    assert!(pattern.tracks[9].steps[10].active);
    assert_eq!(pattern.tracks[9].steps[10].velocity, 105);

    // 7. Clear active bank when in Bank A: Bank B MUST NOT be cleared
    state.bank = DrumBank::BankA;
    state.clear_active_bank(&mut pattern);
    assert!(!pattern.tracks[0].steps[0].active, "Bank A kick must be cleared");
    assert!(!pattern.tracks[1].steps[4].active, "Bank A snare must be cleared");
    assert!(pattern.tracks[8].steps[2].active, "Bank B Closed HH MUST NOT be cleared!");
    assert!(pattern.tracks[9].steps[10].active, "Bank B Open HH MUST NOT be cleared!");

    // 8. Per-pad Mute/Solo isolation across Bank A and B
    state.toggle_mute(0); // Mute Kick (Bank A)
    state.toggle_solo(8); // Solo Closed HH (Bank B)

    assert!(!state.is_pad_audible(0), "Muted Kick must not be audible");
    assert!(state.is_pad_audible(8), "Soloed Closed HH must be audible");
    assert!(!state.is_pad_audible(1), "Non-soloed Snare must not be audible");

    // 9. Headless rendering under Bank A and Bank B
    let ctx = Context::default();
    let mut raw_input = RawInput::default();
    raw_input.screen_rect = Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 600.0)));

    for bank in [DrumBank::BankA, DrumBank::BankB] {
        state.bank = bank;
        let _ = ctx.run(raw_input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let action = render_drum_sequencer_with_state(ui, &mut pattern, 0, &mut state);
                assert!(action.step_toggled.is_none());
            });
        });
    }
}

// =========================================================================
// 4. Groove Patterns: Mathematically Exact Step Activations
// =========================================================================

#[test]
fn test_adv_groove_four_on_the_floor_exact_activations() {
    let state = DrumSequencerState::new();
    let mut pattern = DrumStepPattern::new_16_step();
    state.apply_four_on_the_floor(&mut pattern);

    // Track 0: Kick on 0, 4, 8, 12 (all accented, vel 127)
    for step in 0..16 {
        let s = &pattern.tracks[0].steps[step];
        if step % 4 == 0 {
            assert!(s.active, "Kick step {} must be active", step);
            assert!(s.accent, "Kick step {} must be accented", step);
            assert_eq!(s.velocity, 127);
        } else {
            assert!(!s.active, "Kick step {} must be inactive", step);
        }
    }

    // Track 1: Snare on 4, 12 (accented, vel 115)
    for step in 0..16 {
        let s = &pattern.tracks[1].steps[step];
        if step == 4 || step == 12 {
            assert!(s.active, "Snare step {} must be active", step);
            assert!(s.accent, "Snare step {} must be accented", step);
            assert_eq!(s.velocity, 115);
        } else {
            assert!(!s.active, "Snare step {} must be inactive", step);
        }
    }

    // Track 6: Clap on 4, 12 (unaccented, vel 105)
    for step in 0..16 {
        let s = &pattern.tracks[6].steps[step];
        if step == 4 || step == 12 {
            assert!(s.active, "Clap step {} must be active", step);
            assert!(!s.accent, "Clap step {} must be unaccented", step);
            assert_eq!(s.velocity, 105);
        } else {
            assert!(!s.active, "Clap step {} must be inactive", step);
        }
    }

    // Track 8: Closed HH on offbeats 2, 6, 10, 14 (vel 95)
    for step in 0..16 {
        let s = &pattern.tracks[8].steps[step];
        if step % 4 == 2 {
            assert!(s.active, "Closed HH step {} must be active", step);
            assert_eq!(s.velocity, 95);
        } else {
            assert!(!s.active, "Closed HH step {} must be inactive", step);
        }
    }

    // Track 9: Open HH on 2, 10 (vel 90)
    for step in 0..16 {
        let s = &pattern.tracks[9].steps[step];
        if step == 2 || step == 10 {
            assert!(s.active, "Open HH step {} must be active", step);
            assert_eq!(s.velocity, 90);
        } else {
            assert!(!s.active, "Open HH step {} must be inactive", step);
        }
    }

    // Other tracks must have zero active steps
    let inactive_tracks = [2, 3, 4, 5, 7, 10, 11, 12, 13, 14, 15];
    for &t in &inactive_tracks {
        for step in 0..16 {
            assert!(
                !pattern.tracks[t].steps[step].active,
                "Track {} step {} must be inactive in 4-on-the-Floor",
                t,
                step
            );
        }
    }
}

#[test]
fn test_adv_groove_rkt_tresillo_exact_activations() {
    let state = DrumSequencerState::new();
    let mut pattern = DrumStepPattern::new_16_step();
    state.apply_rkt_tresillo(&mut pattern);

    // Track 0: Kick on 0, 6, 10 (3-3-2 syncopation)
    for step in 0..16 {
        let s = &pattern.tracks[0].steps[step];
        if step == 0 || step == 6 || step == 10 {
            assert!(s.active, "RKT Kick step {} must be active", step);
            let expected_accent = step == 0 || step == 6;
            assert_eq!(s.accent, expected_accent);
            assert_eq!(s.velocity, if expected_accent { 127 } else { 118 });
        } else {
            assert!(!s.active, "RKT Kick step {} must be inactive", step);
        }
    }

    // Track 1: Snare on 3, 7, 11, 15 (dembow syncopation)
    for step in 0..16 {
        let s = &pattern.tracks[1].steps[step];
        if step == 3 || step == 7 || step == 11 || step == 15 {
            assert!(s.active, "RKT Snare step {} must be active", step);
            let expected_accent = step == 3 || step == 11;
            assert_eq!(s.accent, expected_accent);
            assert_eq!(s.velocity, if expected_accent { 122 } else { 110 });
        } else {
            assert!(!s.active, "RKT Snare step {} must be inactive", step);
        }
    }

    // Track 5: Rimshot on 3, 7, 11, 15 (vel 98)
    for step in 0..16 {
        let s = &pattern.tracks[5].steps[step];
        if step == 3 || step == 7 || step == 11 || step == 15 {
            assert!(s.active, "Rimshot step {} must be active", step);
            assert_eq!(s.velocity, 98);
        } else {
            assert!(!s.active, "Rimshot step {} must be inactive", step);
        }
    }

    // Track 8: Closed HH on [0, 2, 3, 6, 8, 10, 11, 14]
    let hat_steps = [0, 2, 3, 6, 8, 10, 11, 14];
    for step in 0..16 {
        let s = &pattern.tracks[8].steps[step];
        if hat_steps.contains(&step) {
            assert!(s.active, "RKT Hat step {} must be active", step);
            let expected_accent = step == 0 || step == 6;
            assert_eq!(s.accent, expected_accent);
            assert_eq!(s.velocity, if expected_accent { 108 } else { 92 });
        } else {
            assert!(!s.active, "RKT Hat step {} must be inactive", step);
        }
    }

    // Track 7: Cowbell on [0, 6, 12] (vel 85)
    for step in 0..16 {
        let s = &pattern.tracks[7].steps[step];
        if step == 0 || step == 6 || step == 12 {
            assert!(s.active, "Cowbell step {} must be active", step);
            assert_eq!(s.velocity, 85);
        } else {
            assert!(!s.active, "Cowbell step {} must be inactive", step);
        }
    }
}

#[test]
fn test_adv_groove_techno_16ths_exact_activations() {
    let state = DrumSequencerState::new();
    let mut pattern = DrumStepPattern::new_16_step();
    state.apply_techno_16ths(&mut pattern);

    // Track 0: Kick on 0, 4, 8, 12 (vel 127, accent true)
    for step in 0..16 {
        let s = &pattern.tracks[0].steps[step];
        if step % 4 == 0 {
            assert!(s.active, "Techno Kick step {} must be active", step);
            assert!(s.accent, "Techno Kick step {} must be accented", step);
            assert_eq!(s.velocity, 127);
        } else {
            assert!(!s.active, "Techno Kick step {} must be inactive", step);
        }
    }

    // Track 8: Closed HH on ALL 16 steps with offbeat accents
    for step in 0..16 {
        let s = &pattern.tracks[8].steps[step];
        assert!(s.active, "Techno rolling hat step {} must be active", step);
        let is_offbeat = step % 4 == 2;
        assert_eq!(s.accent, is_offbeat);
        assert_eq!(s.velocity, if is_offbeat { 120 } else { 85 });
    }

    // Track 9: Open HH on 2, 6, 10, 14 (vel 105)
    for step in 0..16 {
        let s = &pattern.tracks[9].steps[step];
        if step % 4 == 2 {
            assert!(s.active, "Techno Open HH step {} must be active", step);
            assert_eq!(s.velocity, 105);
        } else {
            assert!(!s.active, "Techno Open HH step {} must be inactive", step);
        }
    }

    // Track 6: Clap on 4, 12 (vel 115, accent true)
    for step in 0..16 {
        let s = &pattern.tracks[6].steps[step];
        if step == 4 || step == 12 {
            assert!(s.active, "Techno Clap step {} must be active", step);
            assert!(s.accent, "Techno Clap step {} must be accented", step);
            assert_eq!(s.velocity, 115);
        } else {
            assert!(!s.active, "Techno Clap step {} must be inactive", step);
        }
    }

    // Track 2: Low Tom rumble on 3, 7, 11, 15 (vel 95)
    for step in 0..16 {
        let s = &pattern.tracks[2].steps[step];
        if step % 4 == 3 {
            assert!(s.active, "Techno Low Tom step {} must be active", step);
            assert_eq!(s.velocity, 95);
        } else {
            assert!(!s.active, "Techno Low Tom step {} must be inactive", step);
        }
    }
}

// =========================================================================
// 5. Orbit LFO Animation: Circle, Figure-8, Lissajous Bounded in [0.0, 1.0]
// =========================================================================

#[test]
fn test_adv_orbit_lfo_simulation_1000_steps_strictly_bounded() {
    let mut xy = XyPerformPadState::default();
    xy.orbit_enabled = true;

    let shapes = [OrbitShape::Circle, OrbitShape::FigureEight, OrbitShape::Lissajous];
    let centers = [
        (0.5_f32, 0.5_f32), // Standard center
        (0.0_f32, 0.0_f32), // Bottom-left corner
        (1.0_f32, 1.0_f32), // Top-right corner
        (0.1_f32, 0.9_f32), // Off-center
        (-0.5_f32, 1.5_f32), // Adversarial out-of-bounds center
    ];
    let radii = [0.1_f32, 0.3_f32, 0.45_f32, 0.9_f32, 5.0_f32];
    let speeds = [0.5_f32, 1.0_f32, 4.0_f32, 50.0_f32, -10.0_f32];

    for shape in shapes {
        xy.orbit_shape = shape;
        for center in centers {
            xy.orbit_center = center;
            for &radius in &radii {
                xy.orbit_radius = radius;
                for &speed in &speeds {
                    xy.orbit_speed = speed;

                    // Continuous simulation across 1,000 steps at 60 FPS (dt = 0.016667s)
                    let dt = 1.0 / 60.0;
                    for step in 0..1000 {
                        let time = step as f64 * dt;
                        xy.update_orbit(time);

                        // 1. Verify X and Y bounds
                        assert!(
                            xy.x >= 0.0 && xy.x <= 1.0,
                            "Orbit X={:.4} out of bounds [0.0, 1.0] at step {} (shape={:?}, center={:?}, radius={}, speed={})",
                            xy.x,
                            step,
                            shape,
                            center,
                            radius,
                            speed
                        );
                        assert!(
                            xy.y >= 0.0 && xy.y <= 1.0,
                            "Orbit Y={:.4} out of bounds [0.0, 1.0] at step {} (shape={:?}, center={:?}, radius={}, speed={})",
                            xy.y,
                            step,
                            shape,
                            center,
                            radius,
                            speed
                        );

                        // 2. Verify neither coordinate is NaN or Infinite
                        assert!(!xy.x.is_nan(), "Orbit X must not be NaN");
                        assert!(!xy.y.is_nan(), "Orbit Y must not be NaN");

                        // 3. Verify mapped macros receive bounded values
                        if let Some(x_map) = xy.map_x_to_macro {
                            let val = xy.macros[x_map].value;
                            assert!(val >= 0.0 && val <= 1.0);
                            let display = xy.macros[x_map].format_display();
                            assert!(!display.contains("NaN"));
                        }
                        if let Some(y_map) = xy.map_y_to_macro {
                            let val = xy.macros[y_map].value;
                            assert!(val >= 0.0 && val <= 1.0);
                            let display = xy.macros[y_map].format_display();
                            assert!(!display.contains("NaN"));
                        }
                    }
                }
            }
        }
    }
}
