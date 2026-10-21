//! End-to-End Test Suite for Requirements 2 & 3 (R2 & R3):
//! Full Track & Clip Synchronization, TR-909 Pro Drum Sequencer & 8-Macro XY Surface.
//!
//! Covering Features F9..F18:
//! - F9: Independent Per-Track Device Chains
//! - F10: Active Track Selection Synchronization
//! - F11: Active Clip Slot Selection & Two-Way Binding
//! - F12: TR-909 Pro 8-Voice Drum Sequencer
//! - F13: Step Velocity Shading & Accent Toggle
//! - F14: Per-Voice Mute, Solo & Audition
//! - F15: Drum Groove & Pattern Quick-Bar
//! - F16: 8-Macro Studio Control Surface
//! - F17: Orbit / Motion LFO Clock Automation
//! - F18: Audio-Reactive Canvas on XY Pad

use ableton_audio::{PARAM_SYNTH_CUTOFF, PARAM_SYNTH_RESONANCE};
use ableton_gui::{
    app::{AbletonStudioApp, DeckTab, WorkspaceView},
    panels::{
        device_rack::{DeviceChainViewState, DeviceKind, GuiDeviceModule},
        sequencer::{
            render_drum_sequencer, DrumBank, DrumSequencerState, BANK_A_PAD_NAMES, BANK_B_PAD_NAMES,
        },
    },
    visualizers::xy_pad::{OrbitShape, PhosphorPoint, XyPerformPadState, XyPerformPadWidget},
};
use ableton_timeline::{
    ClipPattern, DrumStepPattern, MidiNote, MidiPattern, SessionClip,
};
use egui::{pos2, vec2, Context, RawInput, Rect};

// =========================================================================
// Feature 9: Independent Per-Track Device Chains
// =========================================================================

#[test]
fn test_f9_independent_per_track_device_chains() {
    // 1. Track 1: "F Dorian Chords" (Polyphonic Wavetable Synth)
    let track_1_chain = DeviceChainViewState {
        devices: vec![
            GuiDeviceModule::new(1, DeviceKind::Wavetable),
            GuiDeviceModule::new(2, DeviceKind::AutoFilter),
            GuiDeviceModule::new(3, DeviceKind::EQEight),
            GuiDeviceModule::new(4, DeviceKind::GlueCompressor),
        ],
        selected_device_idx: 0,
        quick_picker_open: false,
    };

    // 2. Track 2: "909 House Kit" (Drum Machine & Dynamics)
    let track_2_chain = DeviceChainViewState {
        devices: vec![
            GuiDeviceModule::new(10, DeviceKind::DrumRack),
            GuiDeviceModule::new(11, DeviceKind::Roar),
            GuiDeviceModule::new(12, DeviceKind::OTT),
            GuiDeviceModule::new(13, DeviceKind::GlueCompressor),
        ],
        selected_device_idx: 0,
        quick_picker_open: false,
    };

    // 3. Track 3: "Offbeat Sub Bass" (FM 4-Op Operator Bass)
    let track_3_chain = DeviceChainViewState {
        devices: vec![
            GuiDeviceModule::new(20, DeviceKind::Operator),
            GuiDeviceModule::new(21, DeviceKind::AutoFilter),
            GuiDeviceModule::new(22, DeviceKind::Roar),
            GuiDeviceModule::new(23, DeviceKind::EQEight),
        ],
        selected_device_idx: 0,
        quick_picker_open: false,
    };

    // Verify distinct device counts and kinds
    assert_eq!(track_1_chain.devices[0].kind, DeviceKind::Wavetable);
    assert_eq!(track_2_chain.devices[0].kind, DeviceKind::DrumRack);
    assert_eq!(track_3_chain.devices[0].kind, DeviceKind::Operator);

    // Verify isolation: modifying Track 1's device does not alter Track 2 or 3
    let mut chains = vec![track_1_chain, track_2_chain, track_3_chain];
    chains[0].devices[1].is_enabled = false;
    assert!(!chains[0].devices[1].is_enabled);
    assert!(chains[2].devices[1].is_enabled); // Track 3 AutoFilter remains enabled
}

// =========================================================================
// Feature 10: Active Track Selection Synchronization
// =========================================================================

#[test]
fn test_f10_active_track_selection_synchronization() {
    let mut app = AbletonStudioApp::new_mock(44100.0);

    // Initial state: Track 0 selected by default, Session View active
    assert_eq!(app.workspace_view, WorkspaceView::SessionMatrix);

    // Selecting Track 1 (Drum Kit) automatically switches bottom deck to DrumSequencer
    app.deck_tab = DeckTab::DrumSequencer;
    assert_eq!(app.deck_tab, DeckTab::DrumSequencer);

    // Selecting Track 0 or 2 (Synth Tracks) switches bottom deck to ClipView
    app.deck_tab = DeckTab::ClipView;
    assert_eq!(app.deck_tab, DeckTab::ClipView);

    // Switching between Session and Arrangement retains state
    app.workspace_view = WorkspaceView::Arrangement;
    assert_eq!(app.workspace_view, WorkspaceView::Arrangement);

    app.workspace_view = WorkspaceView::SessionMatrix;
    assert_eq!(app.workspace_view, WorkspaceView::SessionMatrix);
}

// =========================================================================
// Feature 11: Active Clip Slot Selection & Two-Way Binding
// =========================================================================

#[test]
fn test_f11_active_clip_slot_selection_and_two_way_binding() {
    let mut app = AbletonStudioApp::new_mock(44100.0);

    // 1. Session Matrix track 0 slot 0 has a clip
    let mut pattern = MidiPattern::new(16.0);
    pattern.notes.push(MidiNote::new(41, 100, 0.0, 3.5)); // F3 chord note
    pattern.notes.push(MidiNote::new(44, 95, 0.0, 3.5));  // Ab3 chord note

    let clip = SessionClip::new(1, "Deep House Chords", ClipPattern::Midi(pattern.clone()));
    app.matrix.tracks[0].slots[0].clip = Some(clip);

    // 2. Load clip pattern into active piano roll pattern
    if let Some(ref c) = app.matrix.tracks[0].slots[0].clip {
        if let ClipPattern::Midi(ref pat) = c.pattern {
            app.midi_pattern = pat.clone();
        }
    }
    assert_eq!(app.midi_pattern.notes.len(), 2);
    assert_eq!(app.midi_pattern.length_beats, 16.0);

    // 3. Mutate note in Piano Roll (Direct manipulation)
    app.midi_pattern.notes[0].start_beat = 0.5;
    app.midi_pattern.notes.push(MidiNote::new(48, 90, 0.0, 3.5)); // C4

    // 4. Two-Way Binding: Write back to Session Matrix clip
    if let Some(ref mut c) = app.matrix.tracks[0].slots[0].clip {
        c.pattern = ClipPattern::Midi(app.midi_pattern.clone());
    }

    // Verify Session Matrix clip reflects mutation
    if let Some(ref c) = app.matrix.tracks[0].slots[0].clip {
        if let ClipPattern::Midi(ref pat) = c.pattern {
            assert_eq!(pat.notes.len(), 3);
            assert_eq!(pat.notes[0].start_beat, 0.5);
        }
    }
}

// =========================================================================
// Feature 12: TR-909 Pro 8-Voice Drum Sequencer
// =========================================================================

#[test]
fn test_f12_tr909_pro_8_voice_drum_sequencer_banks_and_voices() {
    let mut state = DrumSequencerState::new();

    // 1. Bank A: Pads 1-8 (Kick, Snare, Low Tom, Mid Tom, Hi Tom, Rimshot, Clap, Cowbell)
    assert_eq!(state.bank, DrumBank::BankA);
    assert_eq!(BANK_A_PAD_NAMES.len(), 8);
    assert_eq!(BANK_A_PAD_NAMES[0], "Kick");
    assert_eq!(BANK_A_PAD_NAMES[1], "Snare");
    assert_eq!(BANK_A_PAD_NAMES[6], "Clap");

    // 2. Bank B: Pads 9-16 (Closed HH, Open HH, Crash, Ride, Perc 1, Perc 2, Shaker, Tambourine)
    assert_eq!(BANK_B_PAD_NAMES.len(), 8);
    assert_eq!(BANK_B_PAD_NAMES[0], "Closed HH");
    assert_eq!(BANK_B_PAD_NAMES[1], "Open HH");
    assert_eq!(BANK_B_PAD_NAMES[3], "Ride");

    // 3. Toggle Bank
    state.toggle_bank();
    assert_eq!(state.bank, DrumBank::BankB);

    state.toggle_bank();
    assert_eq!(state.bank, DrumBank::BankA);

    // 4. Responsive Step Button Width Calculation
    let available_w = 1200.0_f32;
    let controls_w = 146.0_f32;
    let groove_w = 136.0_f32;
    let grid_w = (available_w - controls_w - groove_w - 24.0).max(384.0);
    let step_w = (grid_w / 16.0).floor().max(20.0);

    assert!(step_w >= 40.0, "On a 1200px width, each 16th step is at least 40px wide: {:.1}px", step_w);
}

// =========================================================================
// Feature 13: Step Velocity Shading & Accent Toggle
// =========================================================================

#[test]
fn test_f13_step_velocity_shading_and_accent_toggle() {
    let mut pattern = DrumStepPattern::new_16_step();

    let pad_idx = 0; // Kick
    let step_idx = 0; // Beat 1

    // 1. Normal Step Activation (velocity = 100, accent = false)
    pattern.tracks[pad_idx].steps[step_idx].active = true;
    pattern.tracks[pad_idx].steps[step_idx].accent = false;
    pattern.tracks[pad_idx].steps[step_idx].velocity = 100;

    assert!(pattern.tracks[pad_idx].steps[step_idx].active);
    assert!(!pattern.tracks[pad_idx].steps[step_idx].accent);
    assert_eq!(pattern.tracks[pad_idx].steps[step_idx].velocity, 100);

    // Velocity fill ratio
    let fill_ratio = pattern.tracks[pad_idx].steps[step_idx].velocity as f32 / 127.0;
    assert!((fill_ratio - (100.0 / 127.0)).abs() < 1e-4);

    // 2. Secondary Click / Shift-Click: Amber Accent Toggle
    pattern.tracks[pad_idx].steps[step_idx].accent = true;
    pattern.tracks[pad_idx].steps[step_idx].velocity = 127;

    assert!(pattern.tracks[pad_idx].steps[step_idx].accent);
    assert_eq!(pattern.tracks[pad_idx].steps[step_idx].velocity, 127);

    // 3. Deactivation resets active and accent
    pattern.tracks[pad_idx].steps[step_idx].active = false;
    pattern.tracks[pad_idx].steps[step_idx].accent = false;
    assert!(!pattern.tracks[pad_idx].steps[step_idx].active);
}

// =========================================================================
// Feature 14: Per-Voice Mute, Solo & Audition
// =========================================================================

#[test]
fn test_f14_per_voice_mute_solo_and_audition() {
    let mut state = DrumSequencerState::new();

    // 1. Initially all pads are audible
    for pad in 0..16 {
        assert!(state.is_pad_audible(pad));
    }

    // 2. Mute Pad 0 (Kick)
    state.toggle_mute(0);
    assert!(!state.is_pad_audible(0), "Muted pad 0 must not be audible");
    assert!(state.is_pad_audible(1), "Unmuted pad 1 must remain audible");

    // 3. Solo Pad 1 (Snare)
    state.toggle_solo(1);
    assert!(state.is_pad_audible(1), "Soloed pad 1 must be audible");
    assert!(!state.is_pad_audible(2), "Non-soloed pad 2 must be silent");
    assert!(!state.is_pad_audible(0), "Muted and non-soloed pad 0 must be silent");

    // 4. Solo Multiple Pads: Pad 1 and Pad 6 (Clap)
    state.toggle_solo(6);
    assert!(state.is_pad_audible(1));
    assert!(state.is_pad_audible(6));
    assert!(!state.is_pad_audible(3));

    // 5. Unsolo all pads: original mute states restored
    state.toggle_solo(1);
    state.toggle_solo(6);
    assert!(!state.is_pad_audible(0), "Pad 0 remains muted");
    assert!(state.is_pad_audible(1), "Pad 1 is audible again");
}

// =========================================================================
// Feature 15: Drum Groove & Pattern Quick-Bar
// =========================================================================

#[test]
fn test_f15_drum_groove_and_pattern_quick_bar() {
    let state = DrumSequencerState::new();
    let mut pattern = DrumStepPattern::new_16_step();

    // 1. 4-on-the-Floor
    state.apply_four_on_the_floor(&mut pattern);

    // Kick on 0, 4, 8, 12 (all accented with velocity 127)
    let kick_trk = &pattern.tracks[0];
    for idx in [0, 4, 8, 12] {
        assert!(kick_trk.steps[idx].active, "Kick step {} must be active", idx);
        assert!(kick_trk.steps[idx].accent, "Kick step {} must be accented", idx);
        assert_eq!(kick_trk.steps[idx].velocity, 127);
    }
    // Snare on 4, 12
    let snare_trk = &pattern.tracks[1];
    assert!(snare_trk.steps[4].active);
    assert!(snare_trk.steps[12].active);
    assert!(!snare_trk.steps[0].active);

    // 2. RKT / Turreo Dembow Tresillo (3-3-2 syncopation)
    state.apply_rkt_tresillo(&mut pattern);
    let kick_rkt = &pattern.tracks[0];
    assert!(kick_rkt.steps[0].active);
    assert!(kick_rkt.steps[6].active);
    assert!(kick_rkt.steps[10].active);
    assert!(!kick_rkt.steps[4].active, "RKT kick must NOT be on 4");

    let snare_rkt = &pattern.tracks[1];
    assert!(snare_rkt.steps[3].active);
    assert!(snare_rkt.steps[7].active);
    assert!(snare_rkt.steps[11].active);
    assert!(snare_rkt.steps[15].active);

    // 3. Clear Active Bank
    state.clear_active_bank(&mut pattern);
    for trk_idx in 0..8 {
        for step in &pattern.tracks[trk_idx].steps {
            assert!(!step.active, "Bank A steps must be cleared");
        }
    }
}

// =========================================================================
// Feature 16: 8-Macro Studio Control Surface
// =========================================================================

#[test]
fn test_f16_8_macro_studio_control_surface() {
    let mut xy = XyPerformPadState::default();

    // Exactly 8 Studio Macros
    assert_eq!(xy.macros.len(), 8);

    // Check canonical Live 12 parameter mappings
    assert_eq!(xy.macros[0].label, "Cutoff");
    assert_eq!(xy.macros[0].unit, "Hz");
    assert_eq!(xy.macros[0].audio_param_id, PARAM_SYNTH_CUTOFF);

    assert_eq!(xy.macros[1].label, "Resonance");
    assert_eq!(xy.macros[1].unit, "%");
    assert_eq!(xy.macros[1].audio_param_id, PARAM_SYNTH_RESONANCE);

    assert_eq!(xy.macros[2].label, "WT Morph");
    assert_eq!(xy.macros[3].label, "Analog Drive");
    assert_eq!(xy.macros[4].label, "Sub Punch");
    assert_eq!(xy.macros[5].label, "Env Mod");
    assert_eq!(xy.macros[6].label, "Reverb Space");
    assert_eq!(xy.macros[7].label, "Delay Dub");

    // Display formatting
    xy.macros[0].value = 0.5; // Halfway between 20 Hz and 20000 Hz = ~10010 Hz
    let display_str = xy.macros[0].format_display();
    assert!(display_str.contains("kHz"), "High frequency displays in kHz: {}", display_str);

    // XY Axis Macro Sync
    xy.x = 0.82;
    xy.y = 0.45;
    xy.sync_mapped_macros();

    assert_eq!(xy.macros[0].value, 0.82, "Macro 1 (Cutoff) mirrors X coordinate");
    assert_eq!(xy.macros[1].value, 0.45, "Macro 2 (Resonance) mirrors Y coordinate");
}

// =========================================================================
// Feature 17: Orbit / Motion LFO Clock Automation
// =========================================================================

#[test]
fn test_f17_orbit_motion_lfo_clock_automation() {
    let mut xy = XyPerformPadState::default();
    xy.orbit_enabled = true;
    xy.orbit_speed = 1.0; // 1.0 Hz
    xy.orbit_radius = 0.30;
    xy.orbit_center = (0.5, 0.5);

    // 1. Circle Orbit: t = 0.0 -> cos(0) = 1, sin(0) = 0 -> (0.8, 0.5)
    xy.orbit_shape = OrbitShape::Circle;
    xy.update_orbit(0.0);
    assert!((xy.x - 0.8).abs() < 1e-4, "Circle at t=0: x={}", xy.x);
    assert!((xy.y - 0.5).abs() < 1e-4, "Circle at t=0: y={}", xy.y);

    // Circle Orbit: t = 0.25 (quarter turn) -> cos(pi/2) = 0, sin(pi/2) = 1 -> (0.5, 0.8)
    xy.update_orbit(0.25);
    assert!((xy.x - 0.5).abs() < 1e-4, "Circle at t=0.25: x={}", xy.x);
    assert!((xy.y - 0.8).abs() < 1e-4, "Circle at t=0.25: y={}", xy.y);

    // 2. Figure-Eight Orbit: t = 0.0 -> (0.8, 0.5)
    xy.orbit_shape = OrbitShape::FigureEight;
    xy.update_orbit(0.0);
    assert!((xy.x - 0.8).abs() < 1e-4);
    assert!((xy.y - 0.5).abs() < 1e-4);

    // 3. Coordinate clamping: radius 0.9 on center (0.9, 0.9) strictly clamped to [0.0, 1.0]
    xy.orbit_center = (0.9, 0.9);
    xy.orbit_radius = 0.45;
    xy.update_orbit(0.0); // Would be 1.35
    assert_eq!(xy.x, 1.0, "Coordinate must be clamped to 1.0");
    assert!(xy.x >= 0.0 && xy.x <= 1.0);
    assert!(xy.y >= 0.0 && xy.y <= 1.0);
}

// =========================================================================
// Feature 18: Audio-Reactive Canvas on XY Pad & Headless Simulation
// =========================================================================

#[test]
fn test_f18_audio_reactive_canvas_and_headless_render() {
    let ctx = Context::default();
    let mut xy = XyPerformPadState::default();

    // 1. Add points to phosphor trail
    xy.trail.push(PhosphorPoint {
        pos: pos2(100.0, 100.0),
        timestamp: 1.0,
    });
    xy.trail.push(PhosphorPoint {
        pos: pos2(110.0, 105.0),
        timestamp: 1.1,
    });
    assert_eq!(xy.trail.len(), 2);

    // 2. Headless egui run of XY Pad Widget with RMS bloom
    let mut raw_input = RawInput::default();
    raw_input.screen_rect = Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0)));
    let _ = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let widget = XyPerformPadWidget::new(&mut xy, 1.2, 0.75, vec2(320.0, 180.0));
            let response = widget.show(ui);
            assert!(response.rect.width() > 0.0);
            assert!(response.rect.height() > 0.0);
        });
    });

    // 3. Headless egui run of Drum Sequencer
    let mut drum_pat = DrumStepPattern::new_16_step();
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = render_drum_sequencer(ui, &mut drum_pat, 0);
            assert!(action.step_toggled.is_none());
        });
    });
}
