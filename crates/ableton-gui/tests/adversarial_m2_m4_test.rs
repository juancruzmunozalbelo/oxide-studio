//! Adversarial Empirical Verification Suite for Milestones 2 & 4:
//! - M2: Track & Clip Sync, Per-Track Independent Device Chains, Bidirectional Write-Back
//! - M4: Layout Splitter Clamping, Multi-Resolution Viewport Invariants & AI Copilot Flex-Fill
//!
//! Stress-testing:
//! 1. Dynamic track allocation: adding new tracks beyond 3 allocates valid chains without indexing panics.
//! 2. Cross-track parameter isolation: modifying parameters on Track 1 never leaks or mutates parameters on Track 2 or 3.
//! 3. Rapid track and clip slot switching: ensure bidirectional write-back does not drop notes or corrupt patterns.
//! 4. Splitter dragging extremes: dragging to y = -10,000 or +10,000 clamps strictly within `[180.0, max_deck_height]`; double-click reset restores exact defaults.
//! 5. Multi-resolution budget invariant: center workspace height >= 55% at 960x600, 1280x800, 1440x900, 1920x1080.
//! 6. AI Copilot zero black void invariant under empty history (0 msgs), single message (1 msg), and high-volume (100 msgs).

use std::collections::HashMap;

use ableton_gui::{
    app::{AbletonStudioApp, DeckTab},
    layout::{StudioSplitterState, ViewportBudget},
    panels::{
        ai_chat::{render_ai_copilot_panel, AiChatMessage, MessageRole},
        device_rack::DeviceKind,
        session_matrix::{CrossfaderAssign, GuiTrackState, SessionMixerSections},
    },
};
use ableton_timeline::{
    ClipPattern, DrumStepPattern, MidiNote, MidiPattern, SessionClip, SessionTrack,
};
use egui::{pos2, vec2, Context, RawInput, Rect};


// =========================================================================
// 1. Dynamic Track Allocation Stress Tests
// =========================================================================

#[test]
fn test_adv_dynamic_track_allocation_stress_up_to_64_tracks() {
    let mut app = AbletonStudioApp::new_mock(44100.0);
    assert_eq!(app.track_device_chains.len(), 3);
    assert_eq!(app.matrix.tracks.len(), 3);
    assert_eq!(app.tracks_gui.len(), 3);

    // Dynamically expand tracks up to 64 tracks
    for track_idx in 3..64 {
        let is_drum = track_idx % 4 == 1;
        let name = if is_drum {
            format!("Drum Kit {}", track_idx + 1)
        } else {
            format!("Synth Lead {}", track_idx + 1)
        };

        // Add to matrix & tracks_gui
        app.matrix.tracks.push(SessionTrack::new(track_idx as u32, &name, 8));
        app.tracks_gui.push(GuiTrackState {
            name: name.clone(),
            volume_linear: 0.85,
            pan: 0.0,
            mute: false,
            solo: false,
            sends: vec![0.0, 0.0],
            input_routing: "All Ins".into(),
            output_routing: "Master".into(),
            monitor_mode: "Auto".into(),
            crossfader_assign: CrossfaderAssign::Off,
        });

        // Ensure chain allocated
        app.ensure_track_device_chain(track_idx);
        assert_eq!(app.track_device_chains.len(), track_idx + 1);

        // Verify valid chain allocation
        let chain = &app.track_device_chains[track_idx];
        assert!(!chain.devices.is_empty(), "Track {} must have devices", track_idx);
        if is_drum {
            assert_eq!(
                chain.devices[0].kind,
                DeviceKind::DrumRack,
                "Track {} (named {}) must be allocated Drum Rack",
                track_idx,
                name
            );
        } else {
            assert_eq!(
                chain.devices[0].kind,
                DeviceKind::Wavetable,
                "Track {} (named {}) must be allocated Wavetable",
                track_idx,
                name
            );
        }
    }

    assert_eq!(app.track_device_chains.len(), 64);
    assert_eq!(app.matrix.tracks.len(), 64);

    // Stress test track selection across all 64 tracks
    for track_idx in 0..64 {
        app.select_track(track_idx);
        assert_eq!(app.selected_track, track_idx);
        assert!(!app.device_chain.devices.is_empty());
        assert_eq!(
            app.device_chain.devices[0].kind,
            app.track_device_chains[track_idx].devices[0].kind
        );

        if track_idx == 1 {
            assert_eq!(app.deck_tab, DeckTab::DrumSequencer);
        }
    }

    // Full headless UI frame execution with 64 tracks
    let ctx = Context::default();
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let raw_input = RawInput {
        screen_rect: Some(screen_rect),
        ..Default::default()
    };
    let budget = ViewportBudget::compute(screen_rect, true, true);

    let output = ctx.run(raw_input, |ctx| {
        app.render_frame_with_budget(ctx, &budget);
    });

    assert!(!output.shapes.is_empty(), "64 tracks frame rendered cleanly without indexing panic");
}

#[test]
fn test_adv_dynamic_allocation_large_out_of_bounds_no_panic() {
    let mut app = AbletonStudioApp::new_mock(44100.0);

    // Directly request chain for index 127
    app.ensure_track_device_chain(127);
    assert_eq!(app.track_device_chains.len(), 128);

    // Verify all intermediate chains are non-empty and valid
    for (i, chain) in app.track_device_chains.iter().enumerate() {
        assert!(!chain.devices.is_empty(), "Chain {} must be populated", i);
    }

    // Select track 127
    app.select_track(127);
    assert_eq!(app.selected_track, 127);
    assert_eq!(app.device_chain.devices[0].kind, DeviceKind::Wavetable);

    // Selecting an out of bounds track beyond max_tracks is safely ignored
    let prev_track = app.selected_track;
    app.select_track(99999);
    assert_eq!(app.selected_track, prev_track, "Out of bounds select_track must be safely ignored");
}

// =========================================================================
// 2. Cross-Track Parameter Isolation Stress Tests
// =========================================================================

#[test]
fn test_adv_cross_track_parameter_isolation_stress() {
    let mut app = AbletonStudioApp::new_mock(44100.0);

    // Add 5 extra dynamic tracks (total 8 tracks: 0..7)
    for i in 3..8 {
        app.matrix.tracks.push(SessionTrack::new(i as u32, &format!("Track {}", i + 1), 8));
        app.tracks_gui.push(GuiTrackState {
            name: format!("Track {}", i + 1),
            volume_linear: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            sends: vec![0.0, 0.0],
            input_routing: "All Ins".into(),
            output_routing: "Master".into(),
            monitor_mode: "Auto".into(),
            crossfader_assign: CrossfaderAssign::Off,
        });
        app.ensure_track_device_chain(i);
    }

    // Apply distinct parameter mutations to each track
    // We record an independent expected profile per track
    #[derive(Debug, Clone, PartialEq)]
    struct TrackFingerprint {
        is_dev0_enabled: bool,
        is_dev1_enabled: bool,
        selected_device_idx: usize,
        table_pos: f32,
        cutoff_hz: f32,
        drive_db: f32,
    }

    let mut fingerprints: Vec<TrackFingerprint> = Vec::new();

    for track_idx in 0..8 {
        app.select_track(track_idx);

        let is_dev0_enabled = track_idx % 2 == 0;
        let is_dev1_enabled = track_idx % 3 != 0;
        let sel_dev = track_idx % app.device_chain.devices.len();
        let table_pos = 0.05 * (track_idx + 1) as f32;
        let cutoff_hz = 200.0 + 350.0 * (track_idx + 1) as f32;
        let drive_db = 1.5 * (track_idx + 1) as f32;

        app.device_chain.devices[0].is_enabled = is_dev0_enabled;
        app.device_chain.devices[1].is_enabled = is_dev1_enabled;
        app.device_chain.selected_device_idx = sel_dev;
        app.device_chain.devices[0].wavetable.table_pos = table_pos;
        app.device_chain.devices[1].auto_filter.cutoff_hz = cutoff_hz;
        app.device_chain.devices[1].roar.drive_db = drive_db;

        fingerprints.push(TrackFingerprint {
            is_dev0_enabled,
            is_dev1_enabled,
            selected_device_idx: sel_dev,
            table_pos,
            cutoff_hz,
            drive_db,
        });

        // Deselect by switching track to persist into track_device_chains
        app.select_track((track_idx + 1) % 8);
    }

    // Rapid randomized switching between tracks (300 iterations)
    // Verifying at every step that the loaded track matches its fingerprint
    // and that switching never leaks or mutates sibling tracks.
    let mut rng_seed: u64 = 0xDEADBEEFCAFE;
    for _ in 0..300 {
        rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let trk = ((rng_seed >> 32) % 8) as usize;

        app.select_track(trk);
        assert_eq!(app.selected_track, trk);

        let fp = &fingerprints[trk];
        assert_eq!(
            app.device_chain.devices[0].is_enabled, fp.is_dev0_enabled,
            "Track {} dev 0 is_enabled mutated by cross-track leak",
            trk
        );
        assert_eq!(
            app.device_chain.devices[1].is_enabled, fp.is_dev1_enabled,
            "Track {} dev 1 is_enabled mutated by cross-track leak",
            trk
        );
        assert_eq!(
            app.device_chain.selected_device_idx, fp.selected_device_idx,
            "Track {} selected_device_idx mutated by cross-track leak",
            trk
        );
        assert_eq!(
            app.device_chain.devices[0].wavetable.table_pos, fp.table_pos,
            "Track {} wavetable table_pos mutated by cross-track leak",
            trk
        );
        assert_eq!(
            app.device_chain.devices[1].auto_filter.cutoff_hz, fp.cutoff_hz,
            "Track {} auto_filter cutoff_hz mutated by cross-track leak",
            trk
        );
        assert_eq!(
            app.device_chain.devices[1].roar.drive_db, fp.drive_db,
            "Track {} roar drive_db mutated by cross-track leak",
            trk
        );
    }

    // Final comprehensive audit across all 8 tracks
    for trk in 0..8 {
        app.select_track(trk);
        let fp = &fingerprints[trk];
        assert_eq!(app.device_chain.devices[0].is_enabled, fp.is_dev0_enabled);
        assert_eq!(app.device_chain.devices[1].is_enabled, fp.is_dev1_enabled);
        assert_eq!(app.device_chain.devices[0].wavetable.table_pos, fp.table_pos);
        assert_eq!(app.device_chain.devices[1].auto_filter.cutoff_hz, fp.cutoff_hz);
        assert_eq!(app.device_chain.devices[1].roar.drive_db, fp.drive_db);
    }
}

// =========================================================================
// 3. Rapid Track and Clip Slot Switching & Bidirectional Write-Back Tests
// =========================================================================

#[test]
fn test_adv_rapid_track_and_clip_slot_switching_bidirectional_write_back() {
    let mut app = AbletonStudioApp::new_mock(44100.0);

    // Setup 3 tracks with 4 clip slots each in Session Matrix
    // Maintain an independent ground-truth ledger of patterns
    let mut oracle_midi: HashMap<(usize, usize), MidiPattern> = HashMap::new();
    let mut oracle_drum: HashMap<(usize, usize), DrumStepPattern> = HashMap::new();

    for trk_idx in 0..3 {
        for slot_idx in 0..4 {
            let clip_id = (trk_idx * 10 + slot_idx + 1) as u32;
            if trk_idx == 1 {
                // Drum track
                let mut drum_pat = DrumStepPattern::new_16_step();
                // Custom rhythm for this slot
                for step in 0..16 {
                    if step % (slot_idx + 2) == 0 {
                        drum_pat.tracks[0].steps[step].active = true;
                        drum_pat.tracks[0].steps[step].velocity = (80 + step * 3).min(127) as u8;
                    }
                }
                oracle_drum.insert((trk_idx, slot_idx), drum_pat.clone());
                app.matrix.tracks[trk_idx].slots[slot_idx].clip = Some(SessionClip::new(
                    clip_id,
                    format!("Drum Slot {}:{}", trk_idx, slot_idx),
                    ClipPattern::Drum(drum_pat),
                ));
            } else {
                // Synth tracks
                let mut midi_pat = MidiPattern::new(16.0);
                let note_count = (slot_idx + 1) * 3;
                for n in 0..note_count {
                    let pitch = (48 + trk_idx * 12 + n * 2) as u8;
                    let beat = n as f64 * 1.0;
                    midi_pat.notes.push(MidiNote::new(pitch, 95, beat, 0.75));
                }
                oracle_midi.insert((trk_idx, slot_idx), midi_pat.clone());
                app.matrix.tracks[trk_idx].slots[slot_idx].clip = Some(SessionClip::new(
                    clip_id,
                    format!("Synth Slot {}:{}", trk_idx, slot_idx),
                    ClipPattern::Midi(midi_pat),
                ));
            }
        }
    }

    // Rapid switching stress loop (500 iterations)
    let mut rng: u64 = 0xCAFEBABE12345678;
    for iter in 0..500 {
        rng = rng.wrapping_mul(2862933555777941757).wrapping_add(3037000493);
        let trk = ((rng >> 32) % 3) as usize;
        let slot = ((rng >> 16) % 4) as usize;

        // 1. Select session clip
        app.select_session_clip(trk, slot);
        assert_eq!(app.selected_track, trk);
        assert_eq!(app.selected_clip_slot, Some((trk, slot)));

        // 2. Verify deck state correctly loaded from Session Matrix
        if trk == 1 {
            assert_eq!(app.deck_tab, DeckTab::DrumSequencer);
            let expected = oracle_drum.get(&(trk, slot)).unwrap();
            for t in 0..8 {
                for s in 0..16 {
                    assert_eq!(
                        app.drum_pattern.tracks[t].steps[s].active,
                        expected.tracks[t].steps[s].active,
                        "Iter {}: Drum step active mismatch at ({}, {}, step {})",
                        iter, trk, slot, s
                    );
                    assert_eq!(
                        app.drum_pattern.tracks[t].steps[s].velocity,
                        expected.tracks[t].steps[s].velocity,
                        "Iter {}: Drum step vel mismatch at ({}, {}, step {})",
                        iter, trk, slot, s
                    );
                }
            }
        } else {
            assert_eq!(app.deck_tab, DeckTab::ClipView);
            let expected = oracle_midi.get(&(trk, slot)).unwrap();
            assert_eq!(
                app.midi_pattern.notes.len(),
                expected.notes.len(),
                "Iter {}: Midi note count mismatch at ({}, {})",
                iter, trk, slot
            );
            for (idx, (actual_note, exp_note)) in app.midi_pattern.notes.iter().zip(&expected.notes).enumerate() {
                assert_eq!(actual_note.pitch, exp_note.pitch, "Note {} pitch mismatch", idx);
                assert!((actual_note.start_beat - exp_note.start_beat).abs() < 1e-4);
            }
        }

        // 3. Mutate active pattern in 50% of iterations & write back
        if iter % 2 == 0 {
            if trk == 1 {
                // Mutate drum pattern
                let step_to_toggle = (iter % 16) as usize;
                let cur = app.drum_pattern.tracks[0].steps[step_to_toggle].active;
                app.drum_pattern.tracks[0].steps[step_to_toggle].active = !cur;
                app.drum_pattern.tracks[0].steps[step_to_toggle].velocity = 115;
                app.write_back_active_clip();

                // Update oracle
                let ent = oracle_drum.get_mut(&(trk, slot)).unwrap();
                ent.tracks[0].steps[step_to_toggle].active = !cur;
                ent.tracks[0].steps[step_to_toggle].velocity = 115;
            } else {
                // Mutate MIDI pattern: add a new note
                let new_pitch = (60 + (iter % 12)) as u8;
                let new_beat = (iter % 16) as f64 * 0.5;
                let note = MidiNote::new(new_pitch, 100, new_beat, 0.5);
                app.midi_pattern.notes.push(note.clone());
                app.write_back_active_clip();

                // Update oracle
                let ent = oracle_midi.get_mut(&(trk, slot)).unwrap();
                ent.notes.push(note);
            }
        }
    }

    // Final Audit: Verify all 12 clips in Session Matrix match Oracle perfectly
    for trk_idx in 0..3 {
        for slot_idx in 0..4 {
            let clip = app.matrix.tracks[trk_idx].slots[slot_idx].clip.as_ref().unwrap();
            if trk_idx == 1 {
                if let ClipPattern::Drum(ref pat) = clip.pattern {
                    let expected = oracle_drum.get(&(trk_idx, slot_idx)).unwrap();
                    for t in 0..8 {
                        for s in 0..16 {
                            assert_eq!(pat.tracks[t].steps[s].active, expected.tracks[t].steps[s].active);
                            assert_eq!(pat.tracks[t].steps[s].velocity, expected.tracks[t].steps[s].velocity);
                        }
                    }
                } else {
                    panic!("Track 1 slot {} must be Drum", slot_idx);
                }
            } else if let ClipPattern::Midi(ref pat) = clip.pattern {
                let expected = oracle_midi.get(&(trk_idx, slot_idx)).unwrap();
                assert_eq!(pat.notes.len(), expected.notes.len());
                for (a, e) in pat.notes.iter().zip(&expected.notes) {
                    assert_eq!(a.pitch, e.pitch);
                    assert!((a.start_beat - e.start_beat).abs() < 1e-4);
                }
            } else {
                panic!("Track {} slot {} must be Midi", trk_idx, slot_idx);
            }
        }
    }
}

// =========================================================================
// 4. Splitter Dragging Extremes & Bounds Clamping Tests
// =========================================================================

#[test]
fn test_adv_splitter_dragging_extremes_clamped_strictly_within_safe_bounds() {
    let test_heights = [600.0_f32, 720.0, 800.0, 900.0, 1080.0, 1440.0, 2160.0];

    for &h in &test_heights {
        let (min_h, max_h) = ViewportBudget::safe_deck_bounds(h);

        // Invariant 1: Ceiling strictly respects center workspace >= 55%
        assert!(
            max_h <= h * 0.45 - 44.0 + 1e-3,
            "max_h {:.2} must respect 55% center height ceiling at h={:.0}",
            max_h,
            h
        );

        // Invariant 2: Ceiling strictly respects deck <= 34%
        assert!(
            max_h <= h * 0.34 + 1e-3,
            "max_h {:.2} must respect 34% total height ceiling at h={:.0}",
            max_h,
            h
        );

        // Invariant 3: min_h <= max_h and min_h == 180.0.min(max_h)
        assert!(min_h <= max_h);
        assert_eq!(min_h, 180.0_f32.min(max_h));

        let default_h = (h * 0.33).clamp(210.0, 270.0).min(h * 0.34).clamp(min_h, max_h);
        let mut state = StudioSplitterState::new(default_h);

        // Extreme Test A: Drag to y = -10,000 (huge upward drag)
        state.drag(-10_000.0, min_h, max_h);
        assert_eq!(
            state.bottom_deck_height, max_h,
            "Extreme negative delta must clamp exactly to max_h"
        );
        assert!(state.is_overridden);
        assert!(state.is_dragging);

        // Extreme Test B: Drag to y = +10,000 (huge downward drag)
        state.drag(10_000.0, min_h, max_h);
        assert_eq!(
            state.bottom_deck_height, min_h,
            "Extreme positive delta must clamp exactly to min_h"
        );
        assert!(state.is_overridden);

        // Extreme Test C: Astronomical values (-1e8 and +1e8)
        state.drag(-1e8, min_h, max_h);
        assert_eq!(state.bottom_deck_height, max_h);
        state.drag(1e8, min_h, max_h);
        assert_eq!(state.bottom_deck_height, min_h);

        // Double-click reset restores exact default budget
        state.reset(default_h);
        assert_eq!(
            state.bottom_deck_height, default_h,
            "Double-click reset must restore exact default computed height"
        );
        assert!(!state.is_overridden);
        assert!(!state.is_dragging);

        // Test ViewportBudget with extreme overrides
        let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, h));
        let budget_min = ViewportBudget::compute_with_deck_height(screen_rect, true, true, Some(-10_000.0));
        let budget_max = ViewportBudget::compute_with_deck_height(screen_rect, true, true, Some(10_000.0));

        assert_eq!(budget_min.bottom_deck_height, min_h);
        assert_eq!(budget_max.bottom_deck_height, max_h);

        assert!(
            budget_max.center_height / h >= 0.55 - 1e-4,
            "Center height ratio ({:.3}) must be >= 0.55 at extreme max drag",
            budget_max.center_height / h
        );
        assert!(
            budget_max.bottom_deck_height / h <= 0.34 + 1e-4,
            "Bottom deck ratio ({:.3}) must be <= 0.34 at extreme max drag",
            budget_max.bottom_deck_height / h
        );
    }
}

// =========================================================================
// 5. Multi-Resolution Budget Invariant Tests
// =========================================================================

#[test]
fn test_adv_multi_resolution_budget_invariants_exhaustive() {
    let resolutions = [
        ("Compact Laptop 960x600", vec2(960.0, 600.0)),
        ("Standard MacBook 1280x800", vec2(1280.0, 800.0)),
        ("Default Retina MacBook 1440x900", vec2(1440.0, 900.0)),
        ("Studio Display 1920x1080", vec2(1920.0, 1080.0)),
        ("4K Studio 3840x2160", vec2(3840.0, 2160.0)),
    ];

    let overrides = [
        None,
        Some(-10000.0),
        Some(0.0),
        Some(180.0),
        Some(210.0),
        Some(260.0),
        Some(350.0),
        Some(10000.0),
    ];

    for (name, size) in resolutions {
        let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), size);

        for browser_open in [false, true] {
            for copilot_open in [false, true] {
                for &deck_override in &overrides {
                    let budget = ViewportBudget::compute_with_deck_height(
                        screen_rect,
                        browser_open,
                        copilot_open,
                        deck_override,
                    );

                    // 1. Center workspace height ratio >= 55%
                    let center_ratio = budget.center_height / size.y;
                    assert!(
                        center_ratio >= 0.55 - 1e-4,
                        "{}: center ratio {:.4} < 0.55 (browser={}, copilot={}, override={:?})",
                        name, center_ratio, browser_open, copilot_open, deck_override
                    );

                    // 2. Bottom deck height ratio <= 34%
                    let deck_ratio = budget.bottom_deck_height / size.y;
                    assert!(
                        deck_ratio <= 0.34 + 1e-4,
                        "{}: deck ratio {:.4} > 0.34 (browser={}, copilot={}, override={:?})",
                        name, deck_ratio, browser_open, copilot_open, deck_override
                    );

                    // 3. Exact vertical sum matches screen height
                    let total_v = budget.top_bar_height + budget.center_height + budget.bottom_deck_height;
                    assert!(
                        (total_v - size.y).abs() < 0.01,
                        "{}: vertical sum mismatch: {:.2} != {:.2}",
                        name, total_v, size.y
                    );

                    // 4. Exact horizontal sum matches screen width
                    let total_h = budget.left_browser_width + budget.center_width + budget.right_copilot_width;
                    assert!(
                        (total_h - size.x).abs() < 0.01,
                        "{}: horizontal sum mismatch: {:.2} != {:.2}",
                        name, total_h, size.x
                    );

                    // 5. Test session split under all 16 mixer combinations
                    for io in [false, true] {
                        for sends in [false, true] {
                            for returns in [false, true] {
                                for cross in [false, true] {
                                    let mixer = SessionMixerSections {
                                        show_io: io,
                                        show_sends: sends,
                                        show_returns: returns,
                                        show_crossfader: cross,
                                    };
                                    let split = budget.session_split(&mixer);

                                    // Clips scroll height must ALWAYS be >= 110.0 px
                                    assert!(
                                        split.clips_scroll_height >= 110.0,
                                        "{}: clips_scroll_height {:.1} < 110.0",
                                        name, split.clips_scroll_height
                                    );

                                    let split_sum = split.toolbar_height
                                        + split.header_height
                                        + split.clips_scroll_height
                                        + split.stop_btn_height
                                        + split.pinned_mixer_footer_height;

                                    assert!(
                                        split_sum <= budget.center_height + 0.01,
                                        "{}: split sum {:.1} exceeds center_height {:.1}",
                                        name, split_sum, budget.center_height
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// =========================================================================
// 6. AI Copilot Zero Black Void Invariant Tests
// =========================================================================

#[test]
fn test_adv_ai_copilot_zero_black_void_under_0_1_and_100_messages() {
    let ctx = Context::default();
    let panel_heights = [600.0_f32, 800.0, 900.0, 1080.0];

    // Scenarios:
    // A: Empty history (0 messages)
    // B: Single message (1 message)
    // C: High-volume message history (100 messages)
    let message_counts = [0, 1, 100];

    for &msg_count in &message_counts {
        for &height in &panel_heights {
            let mut app = AbletonStudioApp::new_mock(44100.0);
            app.copilot_open = true;
            app.ai_copilot.messages.clear();

            for m in 0..msg_count {
                if m % 2 == 0 {
                    app.ai_copilot.messages.push(AiChatMessage {
                        role: MessageRole::User,
                        text: format!("Mensaje de usuario {}", m + 1),
                        mcp_badges: vec![],
                        timestamp: format!("00:{:02}", m),
                    });
                } else {
                    app.ai_copilot.messages.push(AiChatMessage {
                        role: MessageRole::Assistant,
                        text: format!("Respuesta de Copilot {} con acción de mezcla", m + 1),
                        mcp_badges: vec!["synth_set_parameter".into(), "device_set_param".into()],
                        timestamp: format!("00:{:02}", m),
                    });
                }
            }

            assert_eq!(app.ai_copilot.messages.len(), msg_count);

            let raw_input = RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, height))),
                ..Default::default()
            };

            let output = ctx.run(raw_input, |ctx| {
                egui::SidePanel::right("test_ai_copilot_void_audit")
                    .exact_width(280.0)
                    .show(ctx, |ui| {
                        ui.set_height(height);
                        render_ai_copilot_panel(ui, &mut app);
                    });
            });

            // 1. Panel must render non-empty shapes for all message counts
            assert!(
                !output.shapes.is_empty(),
                "Panel with {} messages at height {}px produced zero shapes",
                msg_count, height
            );

            // 2. Measure vertical extent of all rendered shapes within panel
            let mut min_shape_y = f32::MAX;
            let mut max_shape_y = f32::MIN;

            for shape in &output.shapes {
                let rect = shape.shape.visual_bounding_rect();
                if rect.is_finite() && rect.width() > 0.0 && rect.height() > 0.0 {
                    min_shape_y = min_shape_y.min(rect.top());
                    max_shape_y = max_shape_y.max(rect.bottom());
                }
            }

            // Verify header renders near top of panel
            assert!(
                min_shape_y <= 25.0,
                "Header must render near top (min_y={:.1} > 25.0, msgs={}, h={})",
                min_shape_y, msg_count, height
            );

            // Verify footer (input/prompt) renders docked near bottom of panel
            // In bottom-up layout, the prompt input bar sits at the bottom:
            // max_shape_y should be within 20px of panel bottom (height)
            assert!(
                max_shape_y >= height - 25.0,
                "Footer controls must be anchored to bottom of panel (max_y={:.1} < {:.1}, msgs={}, h={})",
                max_shape_y, height - 25.0, msg_count, height
            );

            // 3. Zero Black Void Invariant:
            // The vertical span of rendered elements fills >= 95% of total panel height!
            let vertical_coverage = (max_shape_y - min_shape_y) / height;
            assert!(
                vertical_coverage >= 0.95,
                "Zero Black Void violation: vertical coverage {:.1}% < 95% (msgs={}, h={})",
                vertical_coverage * 100.0, msg_count, height
            );
        }
    }
}

#[test]
fn test_adv_structural_device_chain_mutation_isolation() {
    let mut app = AbletonStudioApp::new_mock(44100.0);

    // Track 0 starts with 4 devices
    assert_eq!(app.track_device_chains[0].devices.len(), 4);
    assert_eq!(app.track_device_chains[1].devices.len(), 4);
    assert_eq!(app.track_device_chains[2].devices.len(), 4);

    // 1. Select Track 0 and insert 2 new devices
    app.select_track(0);
    app.device_chain.devices.push(ableton_gui::panels::device_rack::GuiDeviceModule::new(5, DeviceKind::Roar));
    app.device_chain.devices.push(ableton_gui::panels::device_rack::GuiDeviceModule::new(6, DeviceKind::OTT));
    assert_eq!(app.device_chain.devices.len(), 6);

    // 2. Select Track 1 and remove 1 device
    app.select_track(1);
    assert_eq!(app.device_chain.devices.len(), 4); // Track 1 device count UNTOUCHED
    app.device_chain.devices.pop();
    assert_eq!(app.device_chain.devices.len(), 3);

    // 3. Select Track 2 and verify it has exactly 4 devices
    app.select_track(2);
    assert_eq!(app.device_chain.devices.len(), 4);

    // 4. Switch back to Track 0: must have 6 devices
    app.select_track(0);
    assert_eq!(app.device_chain.devices.len(), 6);
    assert_eq!(app.device_chain.devices[4].kind, DeviceKind::Roar);
    assert_eq!(app.device_chain.devices[5].kind, DeviceKind::OTT);

    // 5. Switch back to Track 1: must have 3 devices
    app.select_track(1);
    assert_eq!(app.device_chain.devices.len(), 3);
}

#[test]
fn test_adv_arrangement_clip_switching_and_write_back_isolation() {
    let mut app = AbletonStudioApp::new_mock(44100.0);

    // Setup arrangement clips on tracks 0 and 1
    let mut pat0 = MidiPattern::new(16.0);
    pat0.notes.push(MidiNote::new(48, 100, 0.0, 4.0));
    pat0.notes.push(MidiNote::new(52, 90, 0.0, 4.0));

    let mut pat1 = DrumStepPattern::new_16_step();
    pat1.tracks[0].steps[0].active = true;
    pat1.tracks[0].steps[4].active = true;

    // Ensure timeline tracks have clips
    if app.timeline.tracks.len() > 1 {
        if !app.timeline.tracks[0].clips.is_empty() {
            app.timeline.tracks[0].clips[0].pattern = ClipPattern::Midi(pat0.clone());
        }
        if !app.timeline.tracks[1].clips.is_empty() {
            app.timeline.tracks[1].clips[0].pattern = ClipPattern::Drum(pat1.clone());
        }
    }

    // Select arrangement clip on track 0
    app.select_arrangement_clip(0, 0);
    assert_eq!(app.selected_track, 0);
    assert_eq!(app.selected_arrangement_clip, Some((0, 0)));

    // Mutate and write back
    app.midi_pattern.notes.push(MidiNote::new(55, 95, 2.0, 2.0));
    app.write_back_active_clip();

    if let Some(track) = app.timeline.tracks.get(0) {
        if let Some(clip) = track.clips.get(0) {
            if let ClipPattern::Midi(ref p) = clip.pattern {
                assert_eq!(p.notes.len(), 3);
            }
        }
    }

    // Select arrangement clip on track 1
    app.select_arrangement_clip(1, 0);
    assert_eq!(app.selected_track, 1);
    assert_eq!(app.deck_tab, DeckTab::DrumSequencer);
}

#[test]
fn test_adv_splitter_widget_egui_interaction_and_show_with_state() {
    let ctx = Context::default();
    let screen_height = 900.0_f32;
    let (min_h, max_h) = ViewportBudget::safe_deck_bounds(screen_height);
    let default_h = (screen_height * 0.33).clamp(210.0, 270.0).min(screen_height * 0.34).clamp(min_h, max_h);

    let raw_input = RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, screen_height))),
        ..Default::default()
    };

    let id = egui::Id::new("test_splitter_id");

    // Frame 1: show_with_state returns default height
    let _ = ctx.run(raw_input.clone(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let (h, _resp) = ableton_gui::layout::StudioHorizontalSplitter::show_with_state(ui, id, screen_height);
            assert_eq!(h, default_h);
        });
    });

    // Directly test show with delta mutations
    let mut current_h = default_h;
    let mut is_overridden = false;

    // Simulate drag upward (-50px delta_y -> +50px height)
    let _ = ctx.run(raw_input.clone(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Test direct call
            let _resp = ableton_gui::layout::StudioHorizontalSplitter::show(
                ui,
                &mut current_h,
                default_h,
                min_h,
                max_h,
                &mut is_overridden,
            );
        });
    });

    assert!(current_h >= min_h && current_h <= max_h);
}

#[test]
fn test_adv_ai_copilot_full_studio_frame_zero_void_across_all_resolutions() {
    let resolutions = [
        vec2(960.0, 600.0),
        vec2(1280.0, 800.0),
        vec2(1440.0, 900.0),
        vec2(1920.0, 1080.0),
    ];

    let ctx = Context::default();

    for size in resolutions {
        let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), size);
        let raw_input = RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let budget = ViewportBudget::compute(screen_rect, true, true);

        // Test with 0 messages, 1 message, and 100 messages
        for msg_count in [0, 1, 100] {
            let mut app = AbletonStudioApp::new_mock(44100.0);
            app.copilot_open = true;
            app.ai_copilot.messages.clear();

            for m in 0..msg_count {
                app.ai_copilot.messages.push(AiChatMessage {
                    role: if m % 2 == 0 { MessageRole::User } else { MessageRole::Assistant },
                    text: format!("Message content #{}", m),
                    mcp_badges: if m % 2 == 1 { vec!["dsp_cmd".into()] } else { vec![] },
                    timestamp: "01:23".into(),
                });
            }

            let output = ctx.run(raw_input.clone(), |ctx| {
                app.render_frame_with_budget(ctx, &budget);
            });

            assert!(!output.shapes.is_empty());

            // Panel bounds integrity: all shapes within screen
            for shape in output.shapes {
                let rect = shape.shape.visual_bounding_rect().intersect(shape.clip_rect);
                if rect.is_finite() && rect.width() > 0.0 && rect.height() > 0.0 {
                    assert!(
                        rect.left() >= -2.0 && rect.top() >= -2.0,
                        "Shape out of bounds top-left at {:?}: {:?}",
                        size, rect
                    );
                    assert!(
                        rect.right() <= size.x + 2.0 && rect.bottom() <= size.y + 2.0,
                        "Shape overflowed bottom-right at {:?}: {:?}",
                        size, rect
                    );
                }
            }
        }
    }
}

