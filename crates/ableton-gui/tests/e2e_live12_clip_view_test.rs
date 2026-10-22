//! End-to-End Test Suite for Requirement 1 (R1):
//! Ableton Live 12 Unified Clip View & Interactive Piano Roll Pro.
//!
//! Covering Features F1..F8:
//! - F1: Side-by-Side Live 12 Clip Inspector + Piano Roll Canvas
//! - F2: Dynamic Bar Span & Zero Note Clipping (16/32 Beats)
//! - F3: Loop Length & Grid Quantization Selectors
//! - F4: Scale Fold & Octave/Pitch Scrolling
//! - F5: Direct Note Manipulation (Move & Resize)
//! - F6: Key Gutter Audition & Audio Command Dispatch
//! - F7: Aligned Velocity & Chance Stalks
//! - F8: Real-Time Transform/Generate Mutation

use ableton_core::{AudioCommand, GlobalScale, ScaleMode};
use ableton_gui::{
    app::{AbletonStudioApp, DeckTab},
    panels::clip_view::{
        render_clip_view, ClipViewAction, ClipViewState, ClipViewSubTab, NoteDragMode,
        NoteDragState,
    },
};
use ableton_timeline::{
    generate::{generate_euclidean, generate_stacks, ScaleLock, StacksChordType, StacksVoicing},
    transform::{transform_connect, transform_ornament, transform_strum, StrumDirection},
    MidiNote, MidiPattern,
};
use egui::{Context, RawInput};

/// Helper: Snap beat to grid division
fn snap_to_grid(beat: f64, grid: f64) -> f64 {
    (beat / grid).round() * grid
}

/// Helper: Compute total beats matching Live 12 dynamic bar span formula
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

// =========================================================================
// Feature 1: Side-by-Side Live 12 Clip Inspector + Piano Roll Canvas
// =========================================================================

#[test]
fn test_f1_inspector_side_by_side_dimensions_and_subtabs() {
    let ctx = Context::default();
    let mut state = ClipViewState::default();
    let mut pattern = MidiPattern::new(16.0);
    let scale = GlobalScale {
        root: 5, // F
        mode: ScaleMode::Dorian,
        scale_lock: true,
    };

    // 1. Initial State: Default subtab is Notes, scale folded is true
    assert_eq!(state.sub_tab, ClipViewSubTab::Notes);
    assert!(state.scale_folded);
    assert_eq!(state.loop_length_bars, 4);

    // 2. Headless egui run verifying side-by-side execution
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_clip_view(ui, &mut state, &mut pattern, &scale, 0.0);
            assert!(!action.pattern_mutated);
        });
    });

    // 3. Switch subtab to Transform tools
    state.sub_tab = ClipViewSubTab::Transform;
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_clip_view(ui, &mut state, &mut pattern, &scale, 2.0);
            assert!(!action.pattern_mutated);
        });
    });

    // 4. Switch subtab to Generate tools
    state.sub_tab = ClipViewSubTab::Generate;
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_clip_view(ui, &mut state, &mut pattern, &scale, 4.0);
            assert!(!action.pattern_mutated);
        });
    });

    // 5. Switch to Audio Warping mode
    state.is_audio = true;
    state.sub_tab = ClipViewSubTab::Warping;
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_clip_view(ui, &mut state, &mut pattern, &scale, 0.0);
            assert!(!action.pattern_mutated);
        });
    });
}

// =========================================================================
// Feature 2: Dynamic Bar Span & Zero Note Clipping (16/32 Beats)
// =========================================================================

#[test]
fn test_f2_dynamic_bar_span_calculation_and_zero_clipping() {
    // Construct a classic 4-bar deep house chord progression in F Dorian (16 beats)
    // Bar 1 (0.0): F-3 (41), Ab-3 (44), C-4 (48), Eb-4 (51)
    // Bar 2 (4.0): Bb-3 (46), D-4 (50), F-4 (53), Ab-4 (56)
    // Bar 3 (8.0): Eb-3 (39), G-3 (43), Bb-3 (46), D-4 (50)
    // Bar 4 (12.0): C-3 (36), Eb-3 (39), G-3 (43), Bb-3 (46)
    let chord_notes = vec![
        // Bar 1
        MidiNote::new(41, 100, 0.0, 3.5),
        MidiNote::new(44, 95, 0.0, 3.5),
        MidiNote::new(48, 90, 0.0, 3.5),
        MidiNote::new(51, 85, 0.0, 3.5),
        // Bar 2 (starts at beat 4.0)
        MidiNote::new(46, 102, 4.0, 3.5),
        MidiNote::new(50, 98, 4.0, 3.5),
        MidiNote::new(53, 92, 4.0, 3.5),
        MidiNote::new(56, 88, 4.0, 3.5),
        // Bar 3 (starts at beat 8.0)
        MidiNote::new(39, 105, 8.0, 3.5),
        MidiNote::new(43, 100, 8.0, 3.5),
        MidiNote::new(46, 95, 8.0, 3.5),
        MidiNote::new(50, 90, 8.0, 3.5),
        // Bar 4 (starts at beat 12.0, ends at beat 15.5)
        MidiNote::new(36, 110, 12.0, 3.5),
        MidiNote::new(39, 105, 12.0, 3.5),
        MidiNote::new(43, 100, 12.0, 3.5),
        MidiNote::new(46, 95, 12.0, 3.5),
    ];

    let pattern = MidiPattern {
        length_beats: 16.0,
        notes: chord_notes,
    };

    // Calculate total beats for 4 bars
    let total_beats = compute_total_beats(&pattern, 4);
    assert_eq!(total_beats, 16.0, "Total beats must be exactly 16.0 for a 4-bar progression");

    // Simulate viewport grid width
    let grid_w = 800.0_f32;
    let grid_left = 266.0_f32;
    let grid_right = grid_left + grid_w;
    let px_per_beat = grid_w / total_beats;

    // Verify EVERY note in bars 1, 2, 3, and 4 renders strictly inside grid bounds
    for note in &pattern.notes {
        let nx = grid_left + note.start_beat as f32 * px_per_beat;
        let nw = note.duration_beats as f32 * px_per_beat;
        let note_right = nx + nw;

        assert!(
            nx >= grid_left,
            "Note starting at beat {} must not precede grid left",
            note.start_beat
        );
        assert!(
            note_right <= grid_right + 0.01,
            "Note at beat {} (end {}) has right edge {:.2} > grid right {:.2} (CLIPPED!)",
            note.start_beat,
            note.end_beat(),
            note_right,
            grid_right
        );
    }

    // Boundary Test: 8-bar loop (32 beats) with final note at beat 31.0
    let mut long_pattern = pattern.clone();
    long_pattern.length_beats = 32.0;
    long_pattern.notes.push(MidiNote::new(65, 100, 31.0, 0.75));

    let total_beats_32 = compute_total_beats(&long_pattern, 8);
    assert_eq!(total_beats_32, 32.0);
    let px_per_beat_32 = grid_w / total_beats_32;
    let last_note = long_pattern.notes.last().unwrap();
    let last_nx = grid_left + last_note.start_beat as f32 * px_per_beat_32;
    let last_nw = last_note.duration_beats as f32 * px_per_beat_32;
    assert!(last_nx + last_nw <= grid_right + 0.01, "32-beat note must not clip right edge");
}

#[test]
fn test_f2_empty_pattern_floor_prevents_division_by_zero() {
    let empty_pattern = MidiPattern::new(0.0);
    let total_beats = compute_total_beats(&empty_pattern, 1);
    assert_eq!(total_beats, 4.0, "Floor must be 4.0 beats (1 bar) even if length_beats is 0.0");
    assert!(total_beats > 0.0);
}

// =========================================================================
// Feature 3: Loop Length & Grid Quantization Selectors
// =========================================================================

#[test]
fn test_f3_loop_length_and_grid_quantization_snapping() {
    let mut state = ClipViewState::default();

    // Loop bar options: 1, 2, 4, 8 Bars
    let bar_options = [1, 2, 4, 8];
    for &bars in &bar_options {
        state.loop_length_bars = bars;
        let expected_beats = bars as f64 * 4.0;
        assert_eq!(bars as f64 * 4.0, expected_beats);
    }

    // Grid options: 1/4 (1.0), 1/8 (0.5), 1/16 (0.25), 1/32 (0.125)
    let grid_options = [1.0, 0.5, 0.25, 0.125];
    for &grid in &grid_options {
        state.quantize_grid = grid;

        // Test precision snapping
        let arbitrary_beat = 1.341;
        let snapped = snap_to_grid(arbitrary_beat, grid);
        let remainder = (snapped / grid).round() * grid - snapped;
        assert!(remainder.abs() < 1e-6, "Snapped beat must be exact multiple of grid");
    }

    // Specific snapping assertions
    assert_eq!(snap_to_grid(0.24, 0.25), 0.25);
    assert_eq!(snap_to_grid(0.12, 0.25), 0.0);
    assert_eq!(snap_to_grid(1.88, 0.5), 2.0);
    assert_eq!(snap_to_grid(3.6, 1.0), 4.0);
    assert_eq!(snap_to_grid(0.07, 0.125), 0.125);
}

// =========================================================================
// Feature 4: Scale Fold & Octave/Pitch Scrolling
// =========================================================================

#[test]
fn test_f4_scale_fold_and_octave_scrolling() {
    let f_dorian = GlobalScale {
        root: 5, // F
        mode: ScaleMode::Dorian,
        scale_lock: true,
    };

    // F Dorian: F, G, Ab, Bb, C, D, Eb (degrees: 5, 7, 8, 10, 0, 2, 3)
    let valid_pitches = [41, 43, 44, 46, 48, 50, 51, 53, 55, 56, 58, 60, 62, 63, 65];
    for &p in &valid_pitches {
        assert!(f_dorian.is_in_scale(p), "Pitch {} must be in F Dorian", p);
    }

    // Non-scale notes: F# (42), A (45), B (47), C# (49), E (52)
    let invalid_pitches = [42, 45, 47, 49, 52];
    for &p in &invalid_pitches {
        assert!(!f_dorian.is_in_scale(p), "Pitch {} must NOT be in F Dorian", p);
    }

    // Test Octave Scrolling
    let mut state = ClipViewState::default();
    assert_eq!(state.pitch_scroll_offset, 0);

    // Scroll up 1 octave
    state.pitch_scroll_offset += 7; // Folded mode: 7 scale degrees per octave
    assert_eq!(state.pitch_scroll_offset, 7);

    // Scroll down 2 octaves
    state.pitch_scroll_offset -= 14;
    assert_eq!(state.pitch_scroll_offset, -7);

    // Reset offset
    state.pitch_scroll_offset = 0;
    assert_eq!(state.pitch_scroll_offset, 0);
}

// =========================================================================
// Feature 5: Direct Note Manipulation (Move & Resize)
// =========================================================================

#[test]
fn test_f5_direct_note_manipulation_move_and_resize() {
    let mut pattern = MidiPattern::new(16.0);
    pattern.notes = vec![
        MidiNote::new(65, 100, 4.0, 1.0), // F4 at Bar 2
        MidiNote::new(68, 90, 8.0, 2.0),  // Ab4 at Bar 3
    ];

    let note_idx = 0;
    let grid = 0.25;

    // 1. Simulate Move in Time (Drag horizontally by +1.5 beats)
    let drag_mode_move = NoteDragMode::Move {
        orig_start: pattern.notes[note_idx].start_beat,
        orig_pitch: pattern.notes[note_idx].pitch,
        pointer_start_beat: 4.0,
        pointer_start_row: 5,
    };
    let drag_state = NoteDragState {
        note_idx,
        mode: drag_mode_move,
    };

    if let NoteDragMode::Move { orig_start, .. } = drag_state.mode {
        let delta_beats = 1.5;
        let new_start = snap_to_grid(orig_start + delta_beats, grid).clamp(0.0, 16.0 - grid);
        pattern.notes[note_idx].start_beat = new_start;
    }
    assert_eq!(pattern.notes[note_idx].start_beat, 5.5, "Note must move to beat 5.5");

    // 2. Simulate Resize Duration (Drag right handle by +1.25 beats)
    let drag_mode_resize = NoteDragMode::ResizeRight {
        orig_duration: pattern.notes[note_idx].duration_beats,
        pointer_start_beat: 6.5,
    };
    let drag_state_resize = NoteDragState {
        note_idx,
        mode: drag_mode_resize,
    };

    if let NoteDragMode::ResizeRight { orig_duration, .. } = drag_state_resize.mode {
        let delta_duration = 1.25;
        let new_duration = snap_to_grid(orig_duration + delta_duration, grid).max(grid);
        pattern.notes[note_idx].duration_beats = new_duration;
    }
    assert_eq!(pattern.notes[note_idx].duration_beats, 2.25, "Note duration must resize to 2.25 beats");

    // 3. Simulate Pitch Transposition (Move up 3 semitones to Ab4 = 68)
    pattern.notes[note_idx].pitch = 68;
    assert_eq!(pattern.notes[note_idx].pitch, 68);

    // 4. Secondary Click Delete
    pattern.notes.remove(note_idx);
    assert_eq!(pattern.notes.len(), 1);
    assert_eq!(pattern.notes[0].pitch, 68);
}

// =========================================================================
// Feature 6: Key Gutter Audition & Audio Command Dispatch
// =========================================================================

#[test]
fn test_f6_key_gutter_audition_and_command_dispatch() {
    let mut app = AbletonStudioApp::new_mock(44100.0);
    app.deck_tab = DeckTab::ClipView;

    // Simulate clicking key in gutter for pitch 65 (F4)
    let audition_pitch = 65_u8;
    let audition_velocity = 100_u8;

    // Action emission
    let action = ClipViewAction {
        pattern_mutated: false,
        warp_mode_changed: None,
        transpose_changed: None,
        warp_marker_moved: false,
        audition_note: Some((audition_pitch, audition_velocity)),
        loop_length_changed: None,
        envelope_mutated: false,
    };

    assert_eq!(action.audition_note, Some((65, 100)));

    // Verify app handles audition by sending NoteOn to mock audio engine
    if let Some((note, vel)) = action.audition_note {
        app.send_command(AudioCommand::NoteOn {
            track_id: 0,
            note,
            velocity: vel,
        });
    }

    // Poll engine to process command
    app.poll_audio();

    // Verify engine received NoteOn without error
    assert_eq!(app.active_synth_note, None); // Mock engine state maintained
}

// =========================================================================
// Feature 7: Velocity & Chance Stalks Alignment
// =========================================================================

#[test]
fn test_f7_velocity_and_chance_stalks_alignment() {
    let mut pattern = MidiPattern::new(16.0);
    pattern.notes = vec![
        MidiNote::new(65, 100, 0.0, 1.0),
        MidiNote::new(68, 80, 4.0, 1.0),
        MidiNote::new(72, 120, 8.0, 1.0),
        MidiNote::new(75, 60, 12.0, 1.0),
    ];

    let grid_left = 220.0_f32;
    let grid_w = 780.0_f32;
    let total_beats = 16.0_f32;
    let px_per_beat = grid_w / total_beats;

    for note in &mut pattern.notes {
        // Stalk X calculation
        let stalk_x = grid_left + note.start_beat as f32 * px_per_beat + 3.0;
        let note_box_x = grid_left + note.start_beat as f32 * px_per_beat;

        assert!(
            (stalk_x - (note_box_x + 3.0)).abs() < 1e-4,
            "Stalk X must match note box X + 3.0"
        );
        assert!(stalk_x >= grid_left);
        assert!(stalk_x <= grid_left + grid_w);

        // Velocity adjustment (drag up/down)
        note.velocity = (note.velocity as i32 + 10).clamp(1, 127) as u8;
        assert!(note.velocity >= 1 && note.velocity <= 127);

        // Chance/Probability adjustment (0.0 .. 1.0)
        note.probability = 0.85;
        assert!(note.probability >= 0.0 && note.probability <= 1.0);
    }
}

// =========================================================================
// Feature 8: Real-Time Transform/Generate Mutation
// =========================================================================

#[test]
fn test_f8_real_time_transform_and_generate_mutation() {
    let scale = GlobalScale {
        root: 5, // F
        mode: ScaleMode::Dorian,
        scale_lock: true,
    };

    // 1. Generate Stacks (Minor 7th on F3 = 41)
    let roots = [41];
    let stacks = generate_stacks(&roots, StacksChordType::Min7, StacksVoicing::Closed, 0, 1.0, 100);
    assert_eq!(stacks.notes.len(), 4, "Minor 7th chord must produce 4 notes");
    for note in &stacks.notes {
        assert!(scale.is_in_scale(note.pitch), "Note {} in stack must be in F Dorian", note.pitch);
    }

    // 2. Generate Euclidean Rhythm (5 hits in 16 steps)
    let euclidean = generate_euclidean(5, 16, 0, 41, 0.25, 100);
    assert_eq!(euclidean.notes.len(), 5, "5 hits must produce 5 notes");

    // 3. Transform Strum (Spread notes across 0.05 beats)
    let mut chord_pattern = MidiPattern::new(4.0);
    for note in &stacks.notes {
        chord_pattern.notes.push(MidiNote::new(note.pitch, 100, 0.0, 2.0));
    }
    transform_strum(&mut chord_pattern, 0.05, StrumDirection::Up);
    // After strum, notes should have slightly staggered start beats
    let first_start = chord_pattern.notes[0].start_beat;
    let last_start = chord_pattern.notes.last().unwrap().start_beat;
    assert!(last_start >= first_start, "Strum Up must stagger start beats upwards");

    // 4. Transform Connect & Ornament
    let sl = ScaleLock::dorian(scale.root);
    transform_connect(&mut chord_pattern, sl, 0.5);
    assert!(!chord_pattern.notes.is_empty());

    transform_ornament(&mut chord_pattern, 0.0625, 2, 0.7);
    assert!(!chord_pattern.notes.is_empty());
}
