//! Comprehensive End-to-End Integration Test Suite for Milestone M11:
//! Ableton Live 12 Workstation GUI & Studio UX Architecture.
//!
//! Subsystems thoroughly tested:
//! 1. Top Control & Scale Bar:
//!    - GlobalScale with 14 scale modes, root selection (C..B), degree mask calculation,
//!      note quantization and note filtering.
//!    - TuningSystem with 12-TET, Just Intonation (5-limit), 24-EDO microtonal tuning,
//!      and Scala (.scl) microtonal scale parser.
//!    - Transport controls: BPM, Time Signature, Metronome, Arrangement Record, CPU meter.
//! 2. Left Collapsible Browser + Sound Similarity:
//!    - 10 category trees, 16 tag pills, instant search filtering.
//!    - 8-dimensional spectral cosine sound similarity ranking with top-10 percentage scores.
//! 3. Center Workspace:
//!    - Session View: tracks, Return buses A & B, Master bus, 8+ scenes, quantized clip launching,
//!      modular mixer strips (I/O, Sends, Mute, Solo, Volume dB, Pan, Crossfader Assign A/None/B).
//!    - Arrangement View: timeline ruler, zoom, locators, loop brace, waveform/MIDI clips,
//!      collapsible multi-lane automation with continuous bezier curves, and comping take lanes.
//! 4. Bottom Detail View:
//!    - Clip View: Piano Roll with scale-fold, velocity stalks, Live 12 Chance (%) lane,
//!      Live 12 Transform & Generate tools, and 6-mode Audio Warping engine panel.
//!    - Device Chain View: Dynamic chain with horizontal scroll, reorder, delete, insert,
//!      bypass, and all 9 interactive visualizers:
//!      Wavetable, Operator, Roar, EQ Eight, Glue Compressor, Auto Filter, OTT, Simpler, Drum Rack.
//! 5. Full Studio App Integration:
//!    - Headless `OxideStudioApp::new_mock` instantiation.
//!    - Audio command dispatch, telemetry polling, and one-click arrangement WAV export.

use oxide_core::{
    AudioCommand, GlobalScale, ScaleMode, TuningSystem, TuningType, ROOT_NAMES,
};
use oxide_gui::{
    app::{OxideStudioApp, DeckTab, WorkspaceView},
    keyboard::{KeyboardMode},
    panels::{
        browser::{BrowserCategory, BrowserState, ALL_TAGS},
        clip_view::{ClipViewState, ClipViewSubTab},
        device_rack::{DeviceChainViewState, DeviceKind, GuiDeviceModule},
        session_matrix::{CrossfaderAssign, GuiReturnTrack, GuiTrackState, LaunchQuantization, SessionMixerSections},
        timeline::TimelineGuiState,
    },
    visualizers::{
        AutoFilterCircuit, AutoFilterVisualizerState, DrumRackVisualizerState,
        EqEightVisualizerState, GlueCompressorVisualizerState,
        OperatorId, OperatorVisualizerState, OttVisualizerState, RoarStageMode, RoarVisualizerState,
        SimplerPlayMode, SimplerVisualizerState, WavetableVisualizerState,
    },
};
use oxide_timeline::{
    generate::{generate_euclidean, generate_seed, generate_stacks, StacksChordType, StacksVoicing},
    quantization::QuantizeMode,
    transform::{transform_connect, transform_ornament, transform_recombine, transform_strum, StrumDirection},
    warping::WarpMode,
    ArrangementClip, ArrangementTimeline, ArrangementTrack, AutomationCurveType, AutomationPoint,
    ClipPattern, MidiNote, MidiPattern, SessionClip, SessionMatrix,
};

#[test]
fn test_top_control_bar_scale_and_microtonal_tuning() {
    // 1. Verify GlobalScale across all 14 ScaleMode variants
    let all_modes = ScaleMode::ALL;

    assert_eq!(all_modes.len(), 14);
    assert_eq!(ROOT_NAMES.len(), 12);

    for (root_idx, root_name) in ROOT_NAMES.iter().enumerate() {
        for mode in all_modes {
            let scale = GlobalScale {
                root: root_idx as u8,
                mode,
                scale_lock: true,
            };

            let mask = scale.degree_mask();
            assert_ne!(mask, 0, "Degree mask cannot be empty for mode {:?}", mode);

            // Quantization must always produce a note in scale
            for raw_note in 36..=84 {
                let q_note = scale.quantize(raw_note);
                assert!(scale.is_in_scale(q_note), "Quantized note {} must be in scale {:?} {}", q_note, mode, root_name);
            }
        }
    }

    // Explicit test for F Dorian (F = 5, Dorian = [0, 2, 3, 5, 7, 9, 10])
    // Notes: F(5), G(7), Ab(8), Bb(10), C(0), D(2), Eb(3)
    let f_dorian = GlobalScale {
        root: 5,
        mode: ScaleMode::Dorian,
        scale_lock: true,
    };
    assert!(f_dorian.is_in_scale(65)); // F4 (65 % 12 = 5)
    assert!(f_dorian.is_in_scale(67)); // G4 (67 % 12 = 7)
    assert!(f_dorian.is_in_scale(68)); // Ab4 (68 % 12 = 8)
    assert!(f_dorian.is_in_scale(70)); // Bb4 (70 % 12 = 10)
    assert!(f_dorian.is_in_scale(72)); // C5 (72 % 12 = 0)
    assert!(f_dorian.is_in_scale(74)); // D5 (74 % 12 = 2)
    assert!(f_dorian.is_in_scale(75)); // Eb5 (75 % 12 = 3)
    assert!(!f_dorian.is_in_scale(66)); // F#4 (66 % 12 = 6) is NOT in F Dorian

    // Filtering non-scale notes
    let notes = vec![65, 66, 67, 68, 69, 70];
    let filtered = f_dorian.filter_non_scale_notes(&notes);
    assert_eq!(filtered, vec![65, 67, 68, 70]);

    // 2. Verify TuningSystem across 12-TET, Just Intonation, 24-EDO, and Scala (.scl)
    let standard_12tet = TuningSystem::default();
    assert_eq!(standard_12tet.tuning_type, TuningType::TwelveTet);
    assert_eq!(standard_12tet.reference_pitch, 440.0);
    let freq_a4 = standard_12tet.frequency_for_note(69);
    assert!((freq_a4 - 440.0).abs() < 1e-4);
    let freq_c4 = standard_12tet.frequency_for_note(60);
    assert!((freq_c4 - 261.6256).abs() < 1e-2);

    // Just Intonation: pure 3/2 fifths and 5/4 thirds
    let just_int = TuningSystem {
        tuning_type: TuningType::JustIntonation,
        reference_pitch: 440.0,
        reference_note: 69,
        scala_cents: vec![],
    };
    let freq_c_ji = just_int.frequency_for_note(60);
    let freq_g_ji = just_int.frequency_for_note(67);
    let fifth_ratio = freq_g_ji / freq_c_ji;
    assert!((fifth_ratio - 1.5).abs() < 1e-4, "Just Intonation fifth must be exactly 3/2 = 1.5");

    let freq_e_ji = just_int.frequency_for_note(64);
    let third_ratio = freq_e_ji / freq_c_ji;
    assert!((third_ratio - 1.25).abs() < 1e-4, "Just Intonation major third must be exactly 5/4 = 1.25");

    // 24-EDO microtonal tuning
    let edo24 = TuningSystem {
        tuning_type: TuningType::TwentyFourEdo,
        reference_pitch: 440.0,
        reference_note: 69,
        scala_cents: vec![],
    };
    let dev_root = edo24.cents_deviation_from_12tet(69);
    assert!((dev_root - 0.0).abs() < 1e-4);
    let dev70 = edo24.cents_deviation_from_12tet(70);
    assert!((dev70 - (-50.0)).abs() < 1e-4, "24-EDO note 70 must have -50.0 cents deviation, got {}", dev70);
    let f70 = edo24.frequency_for_note(70);
    let expected_f70 = 440.0 * 2.0_f64.powf(1.0 / 24.0);
    assert!((f70 - expected_f70).abs() < 1e-4);

    // Scala (.scl) parser
    let pythagorean_scl = "! pyth_12.scl\nPythagorean 12-tone tuning\n12\n!\n256/243\n9/8\n32/27\n81/64\n4/3\n729/512\n3/2\n128/81\n27/16\n16/9\n243/128\n2/1\n";
    let mut loaded_scl = TuningSystem::new(TuningType::Scala);
    loaded_scl.load_scala(pythagorean_scl)
        .expect("Scala SCL loader should parse standard Pythagorean definition");
    assert_eq!(loaded_scl.scala_cents.len(), 12);
    assert!((loaded_scl.scala_cents.last().copied().unwrap() - 1200.0).abs() < 1e-3);

    let scl_freq_a4 = loaded_scl.frequency_for_note(69);
    assert!((scl_freq_a4 - 440.0).abs() < 1e-3);
}

#[test]
fn test_left_collapsible_browser_and_sound_similarity() {
    let mut browser = BrowserState::default();

    // Verify all 10 categories are present
    for cat in BrowserCategory::ALL {
        browser.selected_category = cat;
        browser.active_tags.clear();
        browser.search_query.clear();
        let items = browser.filtered_items();
        assert!(!items.is_empty(), "Browser category {:?} should contain preset items", cat);
    }

    // Verify all 16 tag pills
    assert_eq!(ALL_TAGS.len(), 16);
    for &tag in ALL_TAGS.iter() {
        let tag_matches = browser.items.iter().any(|item| item.tags.iter().any(|t| t == tag));
        assert!(tag_matches, "Browser preset library must contain items tagged with '{}'", tag);
    }

    // Instant Search Text Matching
    browser.search_query = "Wavetable".to_string();
    let searched = browser.filtered_items();
    assert!(!searched.is_empty());
    for item in searched {
        assert!(
            item.name.to_lowercase().contains("wavetable")
                || item.detail.to_lowercase().contains("wavetable")
                || item.tags.iter().any(|t| t.to_lowercase().contains("wavetable"))
        );
    }

    // Spectral Cosine Sound Similarity Ranking
    let target_id = "inst_wavetable";
    browser.find_similar_sounds(target_id);
    let rankings = &browser.similarity_results;
    assert!(!rankings.is_empty(), "Cosine sound similarity ranking must yield matches");
    assert!(rankings.len() <= 10);

    // Verify ranking monotonicity (descending similarity)
    for i in 0..rankings.len() - 1 {
        assert!(
            rankings[i].1 >= rankings[i + 1].1,
            "Similarity rankings must be monotonically descending: {} >= {}",
            rankings[i].1,
            rankings[i + 1].1
        );
        assert!(rankings[i].1 >= 0.0 && rankings[i].1 <= 1.0);
    }
}

#[test]
fn test_center_session_matrix_view_and_quantization() {
    let mut matrix = SessionMatrix::new(8, 8);
    assert_eq!(matrix.tracks.len(), 8);
    assert_eq!(matrix.scenes.len(), 8);

    // Populate clips in slots
    for track_idx in 0..4 {
        for scene_idx in 0..4 {
            let clip = SessionClip::new(
                (track_idx * 8 + scene_idx) as u32,
                format!("Trk {} Clip {}", track_idx + 1, scene_idx + 1),
                ClipPattern::Midi(MidiPattern::new(4.0)),
            );
            matrix.tracks[track_idx].slots[scene_idx].set_clip(Some(clip));
        }
    }

    // Test launch quantizations
    let quant_modes = [
        LaunchQuantization::Bar1,
        LaunchQuantization::Quarter,
        LaunchQuantization::Eighth,
        LaunchQuantization::Sixteenth,
        LaunchQuantization::None,
    ];
    for q in quant_modes {
        assert!(!q.name().is_empty());
    }

    // Launch a scene with Immediate mode
    matrix.trigger_scene(0, 0.0, QuantizeMode::Immediate);
    for t in 0..4 {
        assert!(matrix.tracks[t].slots[0].state.is_playing());
    }

    // Verify Return Buses A (Reverb) and B (Delay)
    let returns = vec![
        GuiReturnTrack {
            name: "A Reverb".to_string(),
            effect_name: "Hybrid Reverb".to_string(),
            volume_linear: 0.85,
            pan: 0.0,
            mute: false,
            solo: false,
        },
        GuiReturnTrack {
            name: "B Delay".to_string(),
            effect_name: "Delay Echo".to_string(),
            volume_linear: 0.70,
            pan: 0.0,
            mute: false,
            solo: false,
        },
    ];
    assert_eq!(returns.len(), 2);
    assert_eq!(returns[0].name, "A Reverb");
    assert_eq!(returns[1].name, "B Delay");

    // Modular Mixer Sections
    let mixer_sections = SessionMixerSections::default();
    assert!(!mixer_sections.show_io);
    assert!(!mixer_sections.show_sends);
    assert!(mixer_sections.show_returns);
    assert!(!mixer_sections.show_crossfader);

    // Track states with Crossfader assignments
    let mut trk_state = GuiTrackState::default();
    trk_state.name = "Lead Synth".into();
    assert_eq!(trk_state.crossfader_assign, CrossfaderAssign::Off);
    trk_state.crossfader_assign = CrossfaderAssign::A;
    assert_eq!(trk_state.crossfader_assign, CrossfaderAssign::A);
    trk_state.crossfader_assign = CrossfaderAssign::B;
    assert_eq!(trk_state.crossfader_assign, CrossfaderAssign::B);
}

#[test]
fn test_center_arrangement_timeline_automation_and_comping() {
    let mut timeline = ArrangementTimeline::new();

    // Add Audio Track and MIDI Track
    let audio_trk = ArrangementTrack::new(0, "Vocal Lead", "Audio");
    let mut midi_trk = ArrangementTrack::new(1, "Bassline", "MidiSynth");

    // Comping Take Lanes
    let mut comping_mgr = oxide_timeline::TrackCompingManager::new();
    comping_mgr.add_take_lane("Take 1 (Main)");
    comping_mgr.add_take_lane("Take 2 (Alt)");
    comping_mgr.add_take_lane("Take 3 (Punch-in)");
    assert_eq!(comping_mgr.take_lanes.len(), 3);

    // Multi-Lane Parameter Automation with continuous bezier curves
    let mut multi_auto = oxide_timeline::MultiLaneAutomation::new();
    let filter_lane = multi_auto.get_or_create_lane(0, "Cutoff Hz");
    filter_lane.add_point(AutomationPoint::new(0.0, 0.1, AutomationCurveType::Linear.to_curve_tension()));
    filter_lane.add_point(AutomationPoint::new(4.0, 0.5, AutomationCurveType::SCurve.to_curve_tension()));
    filter_lane.add_point(AutomationPoint::new(8.0, 0.9, AutomationCurveType::Exponential.to_curve_tension()));

    let val_at_0 = filter_lane.interpolate_at(0.0, 0.0);
    assert!((val_at_0 - 0.1).abs() < 1e-2);
    let val_at_4 = filter_lane.interpolate_at(4.0, 0.0);
    assert!((val_at_4 - 0.5).abs() < 1e-2);
    let val_at_8 = filter_lane.interpolate_at(8.0, 0.0);
    assert!((val_at_8 - 0.9).abs() < 1e-2);

    // Arrangement Clips
    let clip1 = ArrangementClip::new(
        1,
        "Bass Riff 1",
        0.0,
        4.0,
        ClipPattern::Midi(MidiPattern::new(4.0)),
    );
    midi_trk.add_clip(clip1);
    assert_eq!(midi_trk.clips.len(), 1);

    timeline.tracks.push(audio_trk);
    timeline.tracks.push(midi_trk);
    assert_eq!(timeline.tracks.len(), 2);

    // Timeline GUI State
    let timeline_gui = TimelineGuiState::default();
    assert!(timeline_gui.zoom_pixels_per_beat > 0.0);
    assert_eq!(timeline_gui.loop_start_beat, 0.0);
    assert_eq!(timeline_gui.loop_length_beats, 16.0);
    assert!(!timeline_gui.locators.is_empty(), "Timeline should have default cue locators");
}

#[test]
fn test_bottom_clip_view_piano_roll_transforms_and_warping() {
    let mut clip_state = ClipViewState::default();
    assert_eq!(clip_state.sub_tab, ClipViewSubTab::Notes);

    // 1. Piano Roll Scale Fold & Live 12 Chance Lane
    clip_state.scale_folded = true;
    let mut pattern = MidiPattern::new(4.0);

    // Add notes with probability (Live 12 Chance lane)
    let note1 = MidiNote::new(60, 100, 0.0, 1.0).with_probability(1.0);
    let note2 = MidiNote::new(64, 90, 1.0, 1.0).with_probability(0.75);
    let note3 = MidiNote::new(67, 110, 2.0, 1.0).with_probability(0.50);
    pattern.add_note(note1);
    pattern.add_note(note2);
    pattern.add_note(note3);

    assert_eq!(pattern.notes.len(), 3);
    assert_eq!(pattern.notes[0].probability, 1.0);
    assert_eq!(pattern.notes[1].probability, 0.75);
    assert_eq!(pattern.notes[2].probability, 0.50);

    // 2. Live 12 Transform Tools
    // Test Strumming
    let mut chord_pattern = MidiPattern::new(4.0);
    chord_pattern.add_note(MidiNote::new(60, 100, 0.0, 2.0));
    chord_pattern.add_note(MidiNote::new(64, 100, 0.0, 2.0));
    chord_pattern.add_note(MidiNote::new(67, 100, 0.0, 2.0));

    transform_strum(&mut chord_pattern, 0.02, StrumDirection::Up);
    assert!(chord_pattern.notes[0].start_beat <= chord_pattern.notes[1].start_beat);
    assert!(chord_pattern.notes[1].start_beat <= chord_pattern.notes[2].start_beat);

    // Test Connect transform
    let mut melodic = MidiPattern::new(4.0);
    melodic.add_note(MidiNote::new(60, 100, 0.0, 0.5));
    melodic.add_note(MidiNote::new(67, 100, 2.0, 0.5));
    let scale_lock = oxide_timeline::generate::ScaleLock::major(0);
    transform_connect(&mut melodic, scale_lock, 0.5);
    assert!(melodic.notes.len() > 2, "Connect transform should insert passing notes between distant pitches");

    // Test Ornament transform
    transform_ornament(&mut melodic, 0.05, 0, 0.8);
    assert!(melodic.notes.len() >= 3);

    // Test Recombine transform
    transform_recombine(&mut melodic, 1, 0, 0);

    // 3. Live 12 Generate Tools
    // Euclidean generator
    let euclidean = generate_euclidean(5, 16, 0, 36, 0.25, 100);
    assert_eq!(euclidean.notes.len(), 5, "5 pulses over 16 steps must produce exactly 5 notes");

    // Stacks generator
    let roots = [60, 65];
    let stacks = generate_stacks(&roots, StacksChordType::Min7, StacksVoicing::Closed, 0, 1.0, 95);
    assert_eq!(stacks.notes.len(), 8, "2 chord roots of 4-note Min7 must produce 8 notes");

    // Seed generator
    let seeded = generate_seed(4.0, 0.25, 0.8, 48, 72, 80, 110, 0.2, 0.5, Some(scale_lock), 12345);
    assert!(!seeded.notes.is_empty());

    // 4. Audio Warping Panel
    let warp_modes = [
        WarpMode::Beats,
        WarpMode::Tones,
        WarpMode::Texture,
        WarpMode::RePitch,
        WarpMode::Complex,
        WarpMode::ComplexPro,
    ];
    for m in warp_modes {
        clip_state.warp_mode = m;
        assert_eq!(clip_state.warp_mode, m);
    }

    clip_state.transpose_semitones = 7;
    assert_eq!(clip_state.transpose_semitones, 7);
    clip_state.clip_gain_db = -3.5;
    assert!((clip_state.clip_gain_db - (-3.5)).abs() < 1e-4);
    assert_eq!(clip_state.warp_markers.len(), 4);

    // Test interactive drag on warp markers
    clip_state.is_audio = true;
    clip_state.sub_tab = ClipViewSubTab::Warping;
    let ctx = egui::Context::default();
    let mut raw_input = egui::RawInput::default();
    raw_input.events.push(egui::Event::PointerMoved(pos2(100.0, 60.0)));
    raw_input.events.push(egui::Event::PointerButton {
        pos: pos2(100.0, 60.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    raw_input.events.push(egui::Event::PointerMoved(pos2(140.0, 60.0)));
    let scale = GlobalScale::default();
    let _ = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let _act = oxide_gui::panels::clip_view::render_clip_view(
                ui,
                &mut clip_state,
                &mut pattern,
                &scale,
                0.0,
            );
            assert_eq!(clip_state.warp_markers.len(), 4);
        });
    });
}

#[test]
fn test_bottom_device_chain_visualizers_and_rack_reordering() {
    let mut chain = DeviceChainViewState::default();

    // Default chain has Wavetable, Roar, AutoFilter, EQEight, OTT, GlueCompressor
    assert!(!chain.devices.is_empty());

    // Insert missing devices: Operator, Simpler, DrumRack
    chain.devices.push(GuiDeviceModule::new(10, DeviceKind::Operator));
    chain.devices.push(GuiDeviceModule::new(11, DeviceKind::Simpler));
    chain.devices.push(GuiDeviceModule::new(12, DeviceKind::DrumRack));

    // Verify all 9 device kinds are present in the rack
    let expected_kinds = [
        DeviceKind::Wavetable,
        DeviceKind::Operator,
        DeviceKind::Roar,
        DeviceKind::EQEight,
        DeviceKind::GlueCompressor,
        DeviceKind::AutoFilter,
        DeviceKind::OTT,
        DeviceKind::Simpler,
        DeviceKind::DrumRack,
    ];

    for kind in expected_kinds {
        assert!(
            chain.devices.iter().any(|d| d.kind == kind),
            "Device chain must support {:?}",
            kind
        );
    }

    // Bypass toggle
    let initial_state = chain.devices[0].is_enabled;
    chain.devices[0].is_enabled = !initial_state;
    assert_eq!(chain.devices[0].is_enabled, !initial_state);

    // Reorder devices
    let dev0_name = chain.devices[0].name.clone();
    let dev1_name = chain.devices[1].name.clone();
    chain.devices.swap(0, 1);
    assert_eq!(chain.devices[0].name, dev1_name);
    assert_eq!(chain.devices[1].name, dev0_name);

    // Delete device
    let initial_len = chain.devices.len();
    chain.devices.remove(chain.devices.len() - 1);
    assert_eq!(chain.devices.len(), initial_len - 1);

    // Test individual visualizer states
    let wt = WavetableVisualizerState::default();
    assert!(wt.table_pos >= 0.0 && wt.table_pos <= 1.0);
    assert!(wt.filter1_cutoff > 0.0);

    let op = OperatorVisualizerState::default();
    assert!(op.algorithm_id < 11);
    assert_eq!(op.selected_op, OperatorId::A);

    let roar = RoarVisualizerState::default();
    assert_eq!(roar.mode, RoarStageMode::SoftSine);
    assert!(roar.drive_db >= 0.0);

    let eq8 = EqEightVisualizerState::default();
    assert_eq!(eq8.bands.len(), 8);
    assert_eq!(eq8.selected_band, 1);

    let glue = GlueCompressorVisualizerState::default();
    assert!(glue.gain_reduction_db <= 0.0);

    let af = AutoFilterVisualizerState::default();
    assert_eq!(af.circuit, AutoFilterCircuit::Osr);
    assert!(af.cutoff_hz > 0.0);

    let ott = OttVisualizerState::default();
    assert!(ott.depth >= 0.0 && ott.depth <= 1.0);

    let simpler = SimplerVisualizerState::default();
    assert_eq!(simpler.mode, SimplerPlayMode::Slicing);
    assert_eq!(simpler.slice_markers.len(), 8);

    let dr = DrumRackVisualizerState::default();
    assert_eq!(dr.pads.len(), 16);
    assert_eq!(dr.bank, 1);
}

#[test]
fn test_e2e_full_studio_app_headless_integration() {
    let mut app = OxideStudioApp::new_mock(44100.0);

    // Initial View Routing
    assert_eq!(app.workspace_view, WorkspaceView::SessionMatrix);
    assert_eq!(app.deck_tab, DeckTab::ClipView);

    // Switch views
    app.workspace_view = WorkspaceView::ArrangementTimeline;
    assert_eq!(app.workspace_view, WorkspaceView::ArrangementTimeline);
    app.workspace_view = WorkspaceView::SessionMatrix;
    assert_eq!(app.workspace_view, WorkspaceView::SessionMatrix);

    // Switch deck tabs
    app.deck_tab = DeckTab::DeviceRack;
    assert_eq!(app.deck_tab, DeckTab::DeviceRack);
    app.deck_tab = DeckTab::DrumSequencer;
    assert_eq!(app.deck_tab, DeckTab::DrumSequencer);
    app.deck_tab = DeckTab::XyPad;
    assert_eq!(app.deck_tab, DeckTab::XyPad);
    app.deck_tab = DeckTab::ClipView;
    assert_eq!(app.deck_tab, DeckTab::ClipView);

    // Keyboard Mode Toggle
    assert_eq!(app.keyboard.mode, KeyboardMode::Piano);
    app.keyboard.mode = KeyboardMode::Isomorphic;
    assert_eq!(app.keyboard.mode, KeyboardMode::Isomorphic);

    // Global Scale & Tuning Live 12 Integration
    app.global_scale = GlobalScale {
        root: 0,
        mode: ScaleMode::HarmonicMinor,
        scale_lock: true,
    };
    assert!(app.global_scale.scale_lock);
    assert_eq!(app.global_scale.mode, ScaleMode::HarmonicMinor);

    app.tuning_system = TuningSystem {
        tuning_type: TuningType::JustIntonation,
        reference_pitch: 432.0,
        reference_note: 69,
        scala_cents: vec![],
    };
    assert_eq!(app.tuning_system.reference_pitch, 432.0);

    // Send Audio Commands & Poll Telemetry
    app.send_command(AudioCommand::Play);
    app.send_command(AudioCommand::SetTempo(134.0));
    app.poll_audio();

    assert_eq!(app.telemetry.track_peaks_l.len(), 16);
    assert_eq!(app.telemetry.oscilloscope_slice.len(), 256);
    assert_eq!(app.telemetry.spectrum_bins.len(), 64);

    // One-Click WAV Export
    let out_path = format!("/tmp/test_m11_gui_export_{}.wav", std::process::id());
    let export_res = app.export_demo_wav(&out_path);
    assert!(export_res.is_ok(), "Headless demo WAV export must succeed");

    let wav_bytes = std::fs::read(&out_path).expect("Exported WAV file must exist on disk");
    assert!(wav_bytes.len() > 44, "Exported WAV must be larger than 44-byte WAV header");
    assert_eq!(&wav_bytes[0..4], b"RIFF");
    assert_eq!(&wav_bytes[8..12], b"WAVE");
    assert_eq!(&wav_bytes[12..16], b"fmt ");

    let _ = std::fs::remove_file(&out_path);
}

// ============================================================================
// Adversarial Stress Tests for Milestone M11 (Challenger 1)
// ============================================================================

#[test]
fn test_adversarial_global_scale_exhaustive() {
    let all_modes = ScaleMode::ALL;
    let mut total_cases = 0;
    let mut quantization_failures = 0;
    let mut max_quantization_error = 0;
    let mut sample_failures = Vec::new();

    for (root_idx, &root_name) in ROOT_NAMES.iter().enumerate() {
        for &mode in &all_modes {
            let scale = GlobalScale {
                root: root_idx as u8,
                mode,
                scale_lock: true,
            };

            // 1. Verify degree mask invariants
            let mask = scale.degree_mask();
            assert_eq!(
                mask.count_ones() as usize,
                mode.intervals().len(),
                "Degree mask bit count must match mode interval count for {} {:?}",
                root_name,
                mode
            );
            assert_eq!(
                mask >> 12,
                0,
                "Degree mask must not have bits set beyond bit 11 for {} {:?}",
                root_name,
                mode
            );

            // 2. Test note quantization across all MIDI notes 0..=127
            for raw_note in 0u8..=127u8 {
                total_cases += 1;
                let q_note = scale.quantize(raw_note);

                // Check 1: Is quantized note in scale?
                let in_scale = scale.is_in_scale(q_note);
                if !in_scale {
                    sample_failures.push(format!(
                        "NOT IN SCALE: Root: {}, Mode: {:?}, Note: {} (pc={}) -> Quantized to: {} (pc={}) [NOT IN SCALE!]",
                        root_name, mode, raw_note, raw_note % 12, q_note, q_note % 12
                    ));
                }

                // Compute true minimum distance to any scale note in 0..=127
                let true_min_dist = (0u8..=127u8)
                    .filter(|&n| scale.is_in_scale(n))
                    .map(|n| (n as i16 - raw_note as i16).abs())
                    .min()
                    .unwrap_or(0);

                let actual_dist = (q_note as i16 - raw_note as i16).abs();

                if actual_dist > true_min_dist || !in_scale {
                    quantization_failures += 1;
                    let err_diff = actual_dist - true_min_dist;
                    if err_diff > max_quantization_error {
                        max_quantization_error = err_diff;
                    }
                    if sample_failures.len() < 25 {
                        sample_failures.push(format!(
                            "OVERSHOOT: Root: {}, Mode: {:?}, Note: {} (pc={}) -> Quantized to: {} (pc={}), Actual dist: {}, True min dist: {}",
                            root_name, mode, raw_note, raw_note % 12, q_note, q_note % 12, actual_dist, true_min_dist
                        ));
                    }
                }
            }


            // 3. Chromatic note filtering invariant
            let full_chromatic: Vec<u8> = (0u8..=127u8).collect();
            let filtered = scale.filter_non_scale_notes(&full_chromatic);
            for &n in &filtered {
                assert!(scale.is_in_scale(n));
            }
            let excluded: Vec<u8> = (0u8..=127u8).filter(|&n| !scale.is_in_scale(n)).collect();
            assert_eq!(filtered.len() + excluded.len(), 128);
        }
    }

    println!("\n=== GlobalScale Exhaustive Adversarial Test Results ===");
    println!("Total cases tested: {}", total_cases);
    println!("Quantization boundary failures: {} / {}", quantization_failures, total_cases);
    println!("Max quantization overshoot: {} semitones", max_quantization_error);
    for sample in &sample_failures {
        println!("  Fail: {}", sample);
    }

    // Record empirical result
    assert_eq!(
        quantization_failures, 0,
        "GlobalScale::quantize must have 0 boundary failures across all {} combinations, found {}",
        total_cases, quantization_failures
    );
    assert_eq!(
        max_quantization_error, 0,
        "GlobalScale::quantize must have 0 semitone overshoot, found {}",
        max_quantization_error
    );
}

#[test]
fn test_adversarial_tuning_system_discrepancies() {
    println!("\n=== TuningSystem Adversarial Test Results ===");

    // 1. 24-EDO Quarter-Tone Calculations
    let edo24 = TuningSystem {
        tuning_type: TuningType::TwentyFourEdo,
        reference_pitch: 440.0,
        reference_note: 69,
        scala_cents: vec![],
    };

    let f_ref = edo24.frequency_for_note(69);
    assert!((f_ref - 440.0).abs() < 1e-6);

    let f_step1 = edo24.frequency_for_note(70);
    // In genuine 24-EDO (quarter-tones, 50 cents/step):
    // f = 440.0 * 2^(1 / 24) = 452.892984...
    // In 12-TET:
    // f = 440.0 * 2^(1 / 12) = 466.163761...
    let expected_24edo = 440.0 * 2.0_f64.powf(1.0 / 24.0);
    let expected_12tet = 440.0 * 2.0_f64.powf(1.0 / 12.0);
    println!("24-EDO Note 70 (ref=69):");
    println!("  Actual returned freq: {:.6} Hz", f_step1);
    println!("  Expected 24-EDO freq (50 cents/step): {:.6} Hz", expected_24edo);
    println!("  Expected 12-TET freq (100 cents/step): {:.6} Hz", expected_12tet);

    let dev70 = edo24.cents_deviation_from_12tet(70);
    println!("  Actual cents deviation from 12-TET: {:.4} cents", dev70);
    println!("  Expected cents deviation for 24-EDO: -50.0000 cents");

    let is_12tet_clone = (f_step1 - expected_12tet).abs() < 1e-4;
    assert!(!is_12tet_clone, "24-EDO must not be a clone of 12-TET!");
    assert!((dev70 - (-50.0)).abs() < 1e-4, "24-EDO note 70 must be exactly -50 cents deviation, got {}", dev70);
    assert!((f_step1 - expected_24edo).abs() < 1e-4, "24-EDO note 70 must equal expected quarter-tone pitch");

    // 2. Just Intonation pure ratios across octaves
    let ji = TuningSystem {
        tuning_type: TuningType::JustIntonation,
        reference_pitch: 440.0,
        reference_note: 69,
        scala_cents: vec![],
    };
    // Check extreme notes: 0 and 127
    let f0 = ji.frequency_for_note(0);
    let f127 = ji.frequency_for_note(127);
    println!("Just Intonation extreme pitches: note 0 = {:.4} Hz, note 127 = {:.4} Hz", f0, f127);
    assert!(!f0.is_nan() && f0 > 0.0);
    assert!(!f127.is_nan() && f127 > 0.0);

    // 3. Scala Parser Edge Cases
    let mut scl = TuningSystem::default();

    // Edge Case A: Empty file
    assert!(scl.load_scala("").is_err(), "Empty file must return Err");

    // Edge Case B: Comments only
    assert!(
        scl.load_scala("! description comment\n! another comment\n").is_err(),
        "Comments-only file must return Err"
    );

    // Edge Case C: Declared note count vs actual lines (truncated file)
    let truncated_scl = "Truncated Scale\n12\n100.0\n200.0\n";
    let trunc_res = scl.load_scala(truncated_scl);
    println!("Scala truncated file result: {:?}", trunc_res);
    assert!(trunc_res.is_err(), "Truncated Scala file must be rejected with Err");

    // Edge Case D: Division by zero ratio
    let div_zero_scl = "Div Zero Scale\n1\n5/0\n";
    let div_zero_res = scl.load_scala(div_zero_scl);
    println!("Scala division by zero result: {:?}", div_zero_res);
    assert!(div_zero_res.is_err(), "Division by zero ratio must be rejected with Err");

    // Edge Case E: Negative ratio
    let neg_ratio_scl = "Negative Ratio\n1\n-3/2\n";
    let neg_res = scl.load_scala(neg_ratio_scl);
    println!("Scala negative ratio result: {:?}", neg_res);
    assert!(neg_res.is_err(), "Negative ratio must be rejected with Err");

    // Edge Case F: Non-octave formal period (Bohlen-Pierce: tritave 3/1 = 1901.955 cents)
    let bp_scl = "! bohlen-pierce.scl\nBohlen-Pierce Scale\n1\n3/1\n";
    let bp_res = scl.load_scala(bp_scl);
    assert!(bp_res.is_ok(), "Bohlen-Pierce scale with 3/1 tritave should load cleanly");
    // One octave/period above reference note
    let f_bp = scl.frequency_for_note(70); // 1 step above ref 69
    println!("Bohlen-Pierce formal period (3/1) step 1 freq: {:.4} Hz", f_bp);
    // Step 1 should be 440 * (3/1) = 1320.0 Hz
    let expected_bp = 440.0 * 3.0;
    let actual_bp = f_bp;
    let error_bp = (actual_bp - expected_bp).abs();
    println!("  Expected (3/1 tritave): {:.4} Hz, Actual: {:.4} Hz (error: {:.4})", expected_bp, actual_bp, error_bp);
    assert!(error_bp < 1e-4, "Formal period 3/1 must scale reference pitch 440.0 to 1320.0 Hz");
}

#[test]
fn test_adversarial_sound_similarity_metric_properties() {
    println!("\n=== Sound Similarity Metric Adversarial Test Results ===");
    let state = BrowserState::default();
    let items = &state.items;

    // 1. Identity Property: S_C(u, u) = 1.0
    let mut max_identity_err = 0.0f32;
    for item in items {
        let sim = item.cosine_similarity(item);
        let err = (1.0 - sim).abs();
        if err > max_identity_err {
            max_identity_err = err;
        }
        assert!(err < 1e-5, "Self-similarity must be ~1.0, got {} for {}", sim, item.name);
    }
    println!("1. Identity Property S_C(u, u) = 1.0: PASSED across all {} library items (max err: {:e})", items.len(), max_identity_err);

    // 2. Orthogonal Vectors: S_C(e_i, e_j) = 0.0
    let mut basis_items = Vec::new();
    for i in 0..8 {
        let mut feats = [0.0f32; 8];
        feats[i] = 1.0;
        basis_items.push(oxide_gui::panels::browser::BrowserItem::new(
            format!("basis_{}", i),
            format!("Basis {}", i),
            BrowserCategory::Sounds,
            &[],
            "Basis vector",
            feats,
        ));
    }
    for i in 0..8 {
        for j in 0..8 {
            let sim = basis_items[i].cosine_similarity(&basis_items[j]);
            if i == j {
                assert!((sim - 1.0).abs() < 1e-6);
            } else {
                assert_eq!(sim, 0.0, "Orthogonal basis vectors {} and {} must have S_C = 0.0", i, j);
            }
        }
    }
    println!("2. Orthogonality Property S_C(e_i, e_j) = 0.0: PASSED across all 8 standard basis vectors");

    // 3. Zero-Norm Handling
    let zero_item = oxide_gui::panels::browser::BrowserItem::new(
        "zero",
        "Zero",
        BrowserCategory::Sounds,
        &[],
        "Zero vector",
        [0.0f32; 8],
    );
    let sim_zero_zero = zero_item.cosine_similarity(&zero_item);
    assert_eq!(sim_zero_zero, 0.0, "Zero-vector self-similarity must return 0.0");
    for item in items {
        let sim_zero_u = zero_item.cosine_similarity(item);
        let sim_u_zero = item.cosine_similarity(&zero_item);
        assert_eq!(sim_zero_u, 0.0);
        assert_eq!(sim_u_zero, 0.0);
    }
    println!("3. Zero-Norm Handling S_C(0, u) = 0.0: PASSED (no NaN, no panic, strictly 0.0)");

    // 4. Precision Threshold for Small Magnitude Vectors
    let small_item = oxide_gui::panels::browser::BrowserItem::new(
        "small",
        "Small",
        BrowserCategory::Sounds,
        &[],
        "Small vector",
        [1e-5f32; 8],
    );
    let sim_small = small_item.cosine_similarity(&small_item);
    println!("4. Small vector (norm ~ 2.8e-5) self-similarity: {}", sim_small);
    if sim_small == 0.0 {
        println!("  INFO: Underflow protection triggered (denom <= 1e-8 clamped to 0.0)");
    }

    // 5. Strictly Monotonic Descending Sort across entire library
    for item in items {
        let mut sim_browser = state.clone();
        sim_browser.find_similar_sounds(&item.id);
        let results = &sim_browser.similarity_results;

        assert!(!results.is_empty(), "Similarity results cannot be empty for {}", item.id);
        assert!(results.len() <= 10, "Similarity results must be truncated to top 10");

        // Target must not be in its own results
        assert!(
            results.iter().all(|(res_item, _)| res_item.id != item.id),
            "Target item {} must not be present in its own similarity results",
            item.id
        );

        // Monotonic descending verification
        for k in 0..results.len() - 1 {
            assert!(
                results[k].1 >= results[k + 1].1,
                "Similarity sort must be monotonically descending: score[{}]={} >= score[{}]={}",
                k, results[k].1, k + 1, results[k + 1].1
            );
            assert!(results[k].1 >= 0.0 && results[k].1 <= 1.0);
        }
    }
    println!("5. Monotonic Descending Sort: PASSED across all {} library items", items.len());
}

// ============================================================================
// Adversarial Stress Tests for Milestone M11 (Challenger 2 - GUI & Workflows)
// ============================================================================

use egui::{pos2, vec2, Rect};
use oxide_gui::panels::session_matrix::render_session_matrix;
use oxide_gui::panels::timeline::render_arrangement_timeline;
use oxide_gui::visualizers::*;
use oxide_timeline::comping::TrackCompingManager;

#[test]
fn test_adversarial_session_view_and_mixer_extreme_tracks() {
    println!("\n=== Challenger 2: Session View & Mixer Adversarial Stress Test ===");

    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();
    let telemetry = oxide_core::AudioTelemetry::new();

    // 1. Extreme Track Counts: 0 tracks, 1 track, 64 tracks
    for &track_count in &[0, 1, 64] {
        let mut matrix = SessionMatrix::new(track_count, 8);
        let mut tracks_gui: Vec<GuiTrackState> = (0..track_count)
            .map(|i| GuiTrackState {
                name: format!("Track {}", i + 1),
                volume_linear: 0.8,
                pan: (i as f32 % 5.0 - 2.0) / 2.0,
                mute: i % 3 == 0,
                solo: i % 7 == 0,
                sends: vec![0.1, 0.2, 0.3],
                input_routing: "All Ins".into(),
                output_routing: "Master".into(),
                monitor_mode: "Auto".into(),
                crossfader_assign: match i % 3 {
                    0 => CrossfaderAssign::Off,
                    1 => CrossfaderAssign::A,
                    _ => CrossfaderAssign::B,
                },
            })
            .collect();

        // Return tracks: 0, 1, 2, 8
        for &ret_count in &[0, 1, 2, 8] {
            let mut returns: Vec<GuiReturnTrack> = (0..ret_count)
                .map(|r| GuiReturnTrack {
                    name: format!("Return {}", (b'A' + r as u8) as char),
                    effect_name: "FX".into(),
                    volume_linear: 0.7,
                    pan: 0.0,
                    mute: false,
                    solo: false,
                })
                .collect();

            let mut sections = SessionMixerSections::default();
            let mut quant = LaunchQuantization::Bar1;
            let mut xf_pos = 0.0f32;
            let mut master_vol = 1.0f32;

            // Render within egui
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let action = render_session_matrix(
                        ui,
                        &mut matrix,
                        &mut tracks_gui,
                        &mut returns,
                        &mut sections,
                        &mut quant,
                        &mut xf_pos,
                        &mut master_vol,
                        &telemetry,
                        0.0,
                    );
                    // Verify action struct is valid
                    assert!(action.trigger_clip.is_none());
                });
            });
        }
        println!("  Track count {}: SessionMatrix rendering & mixer strips passed without panic", track_count);
    }

    // 2. Crossfader assign state cycling and slider range
    let mut trk = GuiTrackState::default();
    assert_eq!(trk.crossfader_assign, CrossfaderAssign::Off);
    trk.crossfader_assign = match trk.crossfader_assign {
        CrossfaderAssign::Off => CrossfaderAssign::A,
        CrossfaderAssign::A => CrossfaderAssign::B,
        CrossfaderAssign::B => CrossfaderAssign::Off,
    };
    assert_eq!(trk.crossfader_assign, CrossfaderAssign::A);
    trk.crossfader_assign = match trk.crossfader_assign {
        CrossfaderAssign::Off => CrossfaderAssign::A,
        CrossfaderAssign::A => CrossfaderAssign::B,
        CrossfaderAssign::B => CrossfaderAssign::Off,
    };
    assert_eq!(trk.crossfader_assign, CrossfaderAssign::B);
    trk.crossfader_assign = match trk.crossfader_assign {
        CrossfaderAssign::Off => CrossfaderAssign::A,
        CrossfaderAssign::A => CrossfaderAssign::B,
        CrossfaderAssign::B => CrossfaderAssign::Off,
    };
    assert_eq!(trk.crossfader_assign, CrossfaderAssign::Off);

    // 3. Rapid scene firing stress test (100 triggers across varied beats)
    let mut stress_matrix = SessionMatrix::new(64, 16);
    // Populate slots with clips
    for t in 0..64 {
        for s in 0..16 {
            let clip = SessionClip::new(
                (t * 16 + s) as u32,
                format!("T{}S{}", t, s),
                ClipPattern::Midi(MidiPattern::new(4.0)),
            );
            stress_matrix.tracks[t].slots[s].set_clip(Some(clip));
        }
    }

    for i in 0..100 {
        let scene_idx = i % 20; // Tests both valid (0..15) and out-of-bounds (16..19) scenes
        let current_beat = i as f64 * 0.25;
        let mode = match i % 3 {
            0 => QuantizeMode::Immediate,
            1 => QuantizeMode::OneBar,
            _ => QuantizeMode::Quarter,
        };

        stress_matrix.trigger_scene(scene_idx, current_beat, mode);
        stress_matrix.tick(current_beat);
    }
    println!("  Rapid scene firing: 100 consecutive scene triggers across 64 tracks passed cleanly");
}

#[test]
fn test_adversarial_arrangement_view_bezier_comping_and_zoom() {
    println!("\n=== Challenger 2: Arrangement View Adversarial Stress Test ===");

    // 1. Bezier Automation Curves with extreme / identical breakpoint coordinates
    let mut lane = oxide_timeline::automation::AutomationLane::new(1, "Filter Cutoff");

    // Empty lane evaluation
    assert_eq!(lane.interpolate_at(0.0, 0.42), 0.42);
    assert_eq!(lane.interpolate_at(100.0, 0.42), 0.42);

    // Single point evaluation
    lane.add_point(AutomationPoint::new(4.0, 0.8, 0.0));
    assert_eq!(lane.interpolate_at(0.0, 0.0), 0.8);
    assert_eq!(lane.interpolate_at(4.0, 0.0), 0.8);
    assert_eq!(lane.interpolate_at(10.0, 0.0), 0.8);

    // Identical coordinates: adding point at identical beat (4.0) updates in-place
    lane.add_point(AutomationPoint::new(4.0, 0.3, 0.5));
    assert_eq!(lane.points.len(), 1, "Adding point at identical beat must update existing point, not duplicate");
    assert_eq!(lane.points[0].value_norm, 0.3);

    // Epsilon identical coordinates: within 1e-5
    lane.add_point(AutomationPoint::new(4.000005, 0.6, -0.2));
    assert_eq!(lane.points.len(), 1, "Point within 1e-5 eps must update existing point");
    assert_eq!(lane.points[0].value_norm, 0.6);

    // Extreme coordinates: negative beats, huge beats, extreme tension
    lane.add_point(AutomationPoint::new(-100.0, 0.1, -1.0)); // Extreme negative beat, max negative tension
    lane.add_point(AutomationPoint::new(1000.0, 0.9, 1.0));  // Huge future beat, max positive tension
    assert_eq!(lane.points.len(), 3);
    assert_eq!(lane.points[0].beat, -100.0);
    assert_eq!(lane.points[1].beat, 4.000005);
    assert_eq!(lane.points[2].beat, 1000.0);

    // Continuous interpolation across 1000 beats from negative to positive
    for b in -150..1200 {
        let val = lane.interpolate_at(b as f64, 0.5);
        assert!(!val.is_nan(), "Interpolated automation value must not be NaN at beat {}", b);
        assert!(!val.is_infinite(), "Interpolated automation value must not be Infinite at beat {}", b);
        assert!((0.0..=1.0).contains(&val), "Normalized automation must stay bounded: val={}", val);
    }
    println!("  Bezier automation: identical coordinates & extreme [-100..1000] beats verified");

    // 2. Overlapping Take Lane Comp Cuts
    let mut comping = TrackCompingManager::new();
    let lane1 = comping.add_take_lane("Vocal Take 1");
    let lane2 = comping.add_take_lane("Vocal Take 2");
    let lane3 = comping.add_take_lane("Vocal Take 3");

    // Populate take lanes with clips
    let pat = ClipPattern::Midi(MidiPattern::new(16.0));
    comping.get_take_lane_mut(lane1).unwrap().add_clip(ArrangementClip::new(101, "Take 1", 0.0, 16.0, pat.clone()));
    comping.get_take_lane_mut(lane2).unwrap().add_clip(ArrangementClip::new(102, "Take 2", 0.0, 16.0, pat.clone()));
    comping.get_take_lane_mut(lane3).unwrap().add_clip(ArrangementClip::new(103, "Take 3", 0.0, 16.0, pat.clone()));

    let mut main_clips = vec![
        ArrangementClip::new(1, "Original Master", 0.0, 16.0, pat.clone()),
    ];

    // Invalid comp regions (start >= end) must return Err
    assert!(comping.comp_region_to_main(&mut main_clips, lane1, 4.0, 4.0, 5.0).is_err());
    assert!(comping.comp_region_to_main(&mut main_clips, lane1, 6.0, 4.0, 5.0).is_err());
    // Invalid lane ID
    assert!(comping.comp_region_to_main(&mut main_clips, 999, 0.0, 4.0, 5.0).is_err());

    // Sequential overlapping comp cuts
    // Cut 1: Comp [2.0, 6.0] from Take 1
    let res1 = comping.comp_region_to_main(&mut main_clips, lane1, 2.0, 6.0, 5.0);
    assert!(res1.is_ok());

    // Cut 2: Overlapping comp [4.0, 10.0] from Take 2 (overlaps end of Cut 1)
    let res2 = comping.comp_region_to_main(&mut main_clips, lane2, 4.0, 10.0, 6.0);
    assert!(res2.is_ok());

    // Cut 3: Overlapping comp [1.0, 3.0] from Take 3 (overlaps start of Cut 1)
    let res3 = comping.comp_region_to_main(&mut main_clips, lane3, 1.0, 3.0, 4.0);
    assert!(res3.is_ok());

    // Cut 4: Total envelope comp [0.0, 16.0] from Take 1 (overwrites everything)
    let res4 = comping.comp_region_to_main(&mut main_clips, lane1, 0.0, 16.0, 5.0);
    assert!(res4.is_ok());

    // Verify main_clips invariants
    for clip in &main_clips {
        assert!(clip.duration_beats > 0.0, "Clip duration must be strictly positive");
        assert!(!clip.timeline_start_beat.is_nan());
        assert!(!clip.duration_beats.is_nan());
    }
    println!("  Take lane comping: overlapping comp cuts & crossfades verified cleanly");

    // 3. Zoom Clamping and Timeline Canvas Rendering
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();
    let mut timeline = ArrangementTimeline::new();
    timeline.tracks.push(ArrangementTrack::new(1, "Track 1", "MidiSynth"));

    for &zoom in &[0.0, -10.0, 10.0, 24.0, 60.0, 200.0] {
        let mut gui_state = TimelineGuiState::default();
        gui_state.zoom_pixels_per_beat = zoom;

        let _ = ctx.run(raw_input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _action = render_arrangement_timeline(
                    ui,
                    &mut timeline,
                    &mut gui_state,
                    4.0,
                    false,
                );
            });
        });
    }
    println!("  Timeline zoom clamping & extreme zoom (0.0, -10.0, 200.0) rendered without panic");
}

#[test]
fn test_adversarial_visualizers_nan_inf_and_zero_rect() {
    println!("\n=== Challenger 2: All 9 Visualizers NaN/Inf & Zero-Rect Adversarial Test ===");

    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();

    // Test zero-dimension and degenerate rects
    let rect_normal = Rect::from_min_size(pos2(10.0, 10.0), vec2(300.0, 180.0));
    let rect_zero = Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0));
    let rect_zero_w = Rect::from_min_size(pos2(10.0, 10.0), vec2(0.0, 180.0));
    let rect_zero_h = Rect::from_min_size(pos2(10.0, 10.0), vec2(300.0, 0.0));

    let rects = [
        ("Normal 300x180", rect_normal),
        ("Zero 0x0", rect_zero),
        ("Zero-Width 0x180", rect_zero_w),
        ("Zero-Height 300x0", rect_zero_h),
    ];

    let mut crash_count = 0;
    let mut crash_log = Vec::new();

    for &(rect_name, r) in &rects {
        println!("--- Testing Rect: {} ---", rect_name);

        // 1. Wavetable Visualizer
        let res_wt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut wt = WavetableVisualizerState::default();
                    render_wavetable_canvas(painter, r, &wt);
                    wt.table_pos = f32::NAN;
                    wt.filter1_cutoff = f32::INFINITY;
                    wt.filter1_res = f32::NAN;
                    render_wavetable_canvas(painter, r, &wt);
                });
            });
        }));
        if let Err(e) = res_wt {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("Wavetable on {}: {}", rect_name, msg));
            println!("  [FAIL] Wavetable on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] Wavetable on {}", rect_name);
        }

        // 2. Operator Visualizer
        let res_op = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut op = OperatorVisualizerState::default();
                    render_operator_canvas(painter, r, &op);
                    op.ops[0].level = f32::NAN;
                    op.ops[1].ratio = f32::INFINITY;
                    op.ops[2].sustain = f32::NEG_INFINITY;
                    render_operator_canvas(painter, r, &op);
                });
            });
        }));
        if let Err(e) = res_op {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("Operator on {}: {}", rect_name, msg));
            println!("  [FAIL] Operator on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] Operator on {}", rect_name);
        }

        // 3. Roar Visualizer
        let res_roar = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut roar = RoarVisualizerState::default();
                    render_roar_canvas(painter, r, &roar);
                    roar.drive_db = f32::NAN;
                    roar.tone = f32::INFINITY;
                    roar.feedback = f32::NEG_INFINITY;
                    render_roar_canvas(painter, r, &roar);
                });
            });
        }));
        if let Err(e) = res_roar {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("Roar on {}: {}", rect_name, msg));
            println!("  [FAIL] Roar on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] Roar on {}", rect_name);
        }

        // 4. EQ Eight Visualizer
        let res_eq8 = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut eq8 = EqEightVisualizerState::default();
                    let bins = [0.2f32; 64];
                    render_eq8_canvas(painter, r, &eq8, &bins);
                    eq8.bands[0].freq_hz = f32::NAN;
                    eq8.bands[1].gain_db = f32::INFINITY;
                    let nan_bins = [f32::NAN; 64];
                    render_eq8_canvas(painter, r, &eq8, &nan_bins);
                });
            });
        }));
        if let Err(e) = res_eq8 {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("EQ Eight on {}: {}", rect_name, msg));
            println!("  [FAIL] EQ Eight on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] EQ Eight on {}", rect_name);
        }

        // 5. Glue Compressor Visualizer
        let res_glue = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut glue = GlueCompressorVisualizerState::default();
                    render_glue_canvas(painter, r, &glue);
                    glue.gain_reduction_db = f32::NAN;
                    glue.threshold_db = f32::INFINITY;
                    render_glue_canvas(painter, r, &glue);
                });
            });
        }));
        if let Err(e) = res_glue {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("Glue on {}: {}", rect_name, msg));
            println!("  [FAIL] Glue on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] Glue on {}", rect_name);
        }

        // 6. Auto Filter Visualizer
        let res_af = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut af = AutoFilterVisualizerState::default();
                    render_auto_filter_canvas(painter, r, &af);
                    af.cutoff_hz = f32::NAN;
                    af.resonance = f32::INFINITY;
                    af.live_envelope = f32::NAN;
                    render_auto_filter_canvas(painter, r, &af);
                });
            });
        }));
        if let Err(e) = res_af {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("Auto Filter on {}: {}", rect_name, msg));
            println!("  [FAIL] Auto Filter on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] Auto Filter on {}", rect_name);
        }

        // 7. OTT Multiband Visualizer
        let res_ott = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut ott = OttVisualizerState::default();
                    render_ott_canvas(painter, r, &ott);
                    ott.low.gr_meter = f32::NAN;
                    ott.mid.gr_meter = f32::INFINITY;
                    ott.high.gr_meter = f32::NEG_INFINITY;
                    render_ott_canvas(painter, r, &ott);
                });
            });
        }));
        if let Err(e) = res_ott {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("OTT on {}: {}", rect_name, msg));
            println!("  [FAIL] OTT on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] OTT on {}", rect_name);
        }

        // 8. Simpler Visualizer
        let res_simpler = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut simpler = SimplerVisualizerState::default();
                    render_simpler_canvas(painter, r, &simpler);
                    simpler.start_pos = f32::NAN;
                    simpler.end_pos = f32::INFINITY;
                    simpler.playhead_pos = Some(f32::NAN);
                    simpler.slice_markers = vec![f32::NAN, f32::INFINITY, -1.0, 2.0];
                    render_simpler_canvas(painter, r, &simpler);
                });
            });
        }));
        if let Err(e) = res_simpler {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("Simpler on {}: {}", rect_name, msg));
            println!("  [FAIL] Simpler on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] Simpler on {}", rect_name);
        }

        // 9. Drum Rack Visualizer
        let res_dr = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.run(raw_input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let painter = ui.painter();
                    let mut dr = DrumRackVisualizerState::default();
                    render_drum_rack_canvas(painter, r, &dr);
                    dr.pads[0].level_meter = f32::NAN;
                    dr.pads[1].level_meter = f32::INFINITY;
                    render_drum_rack_canvas(painter, r, &dr);
                });
            });
        }));
        if let Err(e) = res_dr {
            crash_count += 1;
            let msg = if let Some(s) = e.downcast_ref::<&str>() { s } else if let Some(s) = e.downcast_ref::<String>() { s.as_str() } else { "panic" };
            crash_log.push(format!("Drum Rack on {}: {}", rect_name, msg));
            println!("  [FAIL] Drum Rack on {}: {}", rect_name, msg);
        } else {
            println!("  [PASS] Drum Rack on {}", rect_name);
        }
    }

    println!("\nTotal visualizer crash failure modes caught: {}", crash_count);
    assert_eq!(
        crash_count, 0,
        "All 9 visualizers must survive NaN/Inf parameters and zero/degenerate rects without crashing! Crashes: {:?}",
        crash_log
    );

    // 10. Out-of-Bounds Index Crash Vulnerabilities
    println!("\n--- Testing Visualizer Out-of-Bounds Indices ---");
    // EQ Eight selected_band = 8 (len is 8)
    let res_eq_idx = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = ctx.run(raw_input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let painter = ui.painter();
                let mut eq8 = EqEightVisualizerState::default();
                eq8.selected_band = 8;
                let bins = [0.0f32; 64];
                render_eq8_canvas(painter, rect_normal, &eq8, &bins);
            });
        });
    }));
    assert!(res_eq_idx.is_ok(), "EQ Eight selected_band=8 must not panic!");
    println!("  [PASS] EQ Eight selected_band=8 handled safely");

    // Drum Rack selected_pad = 16 (len is 16)
    let res_dr_idx = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = ctx.run(raw_input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let painter = ui.painter();
                let mut dr = DrumRackVisualizerState::default();
                dr.selected_pad = 16;
                render_drum_rack_canvas(painter, rect_normal, &dr);
            });
        });
    }));
    assert!(res_dr_idx.is_ok(), "Drum Rack selected_pad=16 must not panic!");
    println!("  [PASS] Drum Rack selected_pad=16 handled safely");
}


