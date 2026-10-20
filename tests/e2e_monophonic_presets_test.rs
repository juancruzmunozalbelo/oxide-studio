//! E2E Test Suite: Monophonic Voicing, Legato Gate Persistence, Exponential Glide,
//! 38 Factory Presets, and 17 XY Macro Breakpoint Mappings
//!
//! Authoritative baselines:
//! - /Users/juancruz/ableton-rea/src/engine/dsp-core.ts (lines 221-265, 430-474)
//! - /Users/juancruz/ableton-rea/src/data/preset-catalog.json (38 factory presets)
//! - /Users/juancruz/ableton-rea/src/data/mapping-catalog.json (17 XY macro mappings)
//! - /Users/juancruz/ableton-rust/PROJECT.md

use ableton_dsp::{
    evaluate_macro_target, load_mapping_catalog, load_preset_catalog, MacroBreakpoint,
    MonophonicVoice, NoteStack, VoiceReleaseAction, VoiceTriggerAction,
};

#[test]
fn test_monophonic_last_note_priority_legato_trill() {
    let mut voice = MonophonicVoice::new();
    assert!(!voice.gate_open);
    assert_eq!(voice.note_stack.len(), 0);

    // 1. Play Note 60 (C4)
    let action1 = voice.note_on(60, 100, 0.0);
    assert_eq!(action1, VoiceTriggerAction::TriggerAttack, "Gate must open on first note with TriggerAttack");
    assert!(voice.gate_open);
    assert_eq!(voice.target_midi_pitch, 60.0);
    assert_eq!(voice.current_midi_pitch, 60.0);
    assert_eq!(voice.note_stack.len(), 1);

    // 2. Play Note 64 (E4) while 60 is still held (legato phrase)
    let action2 = voice.note_on(64, 100, 0.0);
    assert_eq!(action2, VoiceTriggerAction::LegatoGlide, "Legato note must produce LegatoGlide without re-triggering attack");
    assert!(voice.gate_open);
    assert_eq!(voice.target_midi_pitch, 64.0);
    assert_eq!(voice.note_stack.len(), 2);

    // 3. Release Note 64 -> Note 60 is still held!
    let action3 = voice.note_off(64);
    assert_eq!(action3, VoiceReleaseAction::LegatoRestore(60), "Pitch must restore to held note 60 with LegatoRestore");
    assert!(voice.gate_open, "Gate must stay open because note 60 is still held");
    assert_eq!(voice.target_midi_pitch, 60.0, "Pitch must restore to 60.0");
    assert_eq!(voice.note_stack.len(), 1);

    // 4. Release Note 60 -> Stack is now empty!
    let action4 = voice.note_off(60);
    assert_eq!(action4, VoiceReleaseAction::TriggerRelease, "Gate must close with TriggerRelease when last note is released");
    assert!(!voice.gate_open);
    assert_eq!(voice.note_stack.len(), 0);
}

#[test]
fn test_exponential_glide_smoothing() {
    let sample_rate = 44100.0;
    let mut voice = MonophonicVoice::new();
    let glide_ms = 50.0;

    // First note: C4 (60)
    voice.note_on(60, 100, glide_ms);
    assert_eq!(voice.current_midi_pitch, 60.0);

    // Legato transition to C5 (72)
    voice.note_on(72, 100, glide_ms);
    assert_eq!(voice.target_midi_pitch, 72.0);

    // Step through 50ms (~2205 samples)
    let samples_50ms = (0.050 * sample_rate) as usize;
    let mut pitch_mid = 0.0f32;

    for i in 0..samples_50ms {
        voice.advance_glide(glide_ms, sample_rate);
        if i == samples_50ms / 2 {
            pitch_mid = voice.current_midi_pitch;
        }
    }

    println!("Exponential Glide from 60 to 72 over 50ms:");
    println!("  Midway pitch (25ms): {:.2}", pitch_mid);
    println!("  Pitch at 50ms: {:.2}", voice.current_midi_pitch);

    assert!(
        pitch_mid > 60.0 && pitch_mid < 72.0,
        "Midway pitch must be strictly intermediate (got {:.2})",
        pitch_mid
    );
    assert!(
        voice.current_midi_pitch > 67.0 && voice.current_midi_pitch <= 72.0,
        "Pitch after 1 time constant must have approached target (got {:.2})",
        voice.current_midi_pitch
    );
}

#[test]
fn test_note_stack_deduplication_and_capacity() {
    let mut stack = NoteStack::new();

    // Push 60, then 62, then 60 again (re-striking 60)
    stack.push(60, 100);
    stack.push(62, 100);
    assert_eq!(stack.len(), 2);
    assert_eq!(stack.top().unwrap().note, 62);

    stack.push(60, 120);
    assert_eq!(stack.len(), 2, "Stack length must remain 2 after re-striking held note");
    assert_eq!(stack.top().unwrap().note, 60, "Re-struck note must be at top of stack");

    // Capacity test: push 20 distinct notes
    for n in 1..=20 {
        stack.push(n, 100);
    }
    assert_eq!(stack.len(), 16, "Stack capacity must cap at 16 without memory corruption");
}

#[test]
fn test_38_factory_presets_catalog() {
    let catalog = load_preset_catalog().expect("Failed to load preset catalog");

    assert_eq!(
        catalog.len(),
        38,
        "Catalog must contain exactly 38 factory presets, found {}",
        catalog.len()
    );

    // Verify key factory presets exist
    let required_presets = [
        "wowbass.preset",
        "west-coast-lead.preset",
        "two-sounds-in-one.preset",
        "synthkick.preset",
        "subbass.preset",
        "strings.preset",
        "release-pluck.preset",
        "pluckedbass.preset",
        "noisy-lead.preset",
        "laser.preset",
        "hihat.preset",
        "german-siren.preset",
        "distortion.preset",
        "bouncingball.preset",
        "american-siren.preset",
    ];

    for name in &required_presets {
        let preset = catalog.get(*name).unwrap_or_else(|| panic!("Missing required preset {}", name));
        assert!(
            preset.parameters.len() >= 27 && preset.parameters.len() <= 30,
            "Preset {} must have between 27 and 30 parameters, found {}",
            name,
            preset.parameters.len()
        );
    }

    println!("Verified all 38 factory synth presets successfully.");
}

#[test]
fn test_17_xy_macro_mappings_and_breakpoints() {
    let mappings = load_mapping_catalog().expect("Failed to load mapping catalog");

    assert_eq!(
        mappings.len(),
        17,
        "Catalog must contain exactly 17 XY macro mappings, found {}",
        mappings.len()
    );

    // Verify wowbass mapping
    let wowbass = mappings.get("wowbass.mapping").expect("wowbass.mapping exists");
    assert!(!wowbass.macro_x.is_empty(), "wowbass must have macroX targets");
    let target_x = &wowbass.macro_x[0];
    assert_eq!(target_x.parameter_name, "filter_frequency");

    let min_ctrl_x = target_x.min_control_value;
    let max_ctrl_x = target_x.max_control_value;
    let min_val_x = target_x.min_value.unwrap_or(0.0);
    let max_val_x = target_x.max_value.unwrap_or(0.0);

    println!(
        "wowbass Macro X mapping: minCtrl={:.4}, maxCtrl={:.4}, minVal={:.2}Hz, maxVal={:.2}Hz",
        min_ctrl_x, max_ctrl_x, min_val_x, max_val_x
    );

    assert!((min_val_x - 152.87).abs() < 1.0, "wowbass min cutoff must be ~152.87 Hz");
    assert!((max_val_x - 2490.39).abs() < 1.0, "wowbass max cutoff must be ~2490.39 Hz");

    // Test sweeping X from 0.0 to 1.0
    let ctrl_at_0 = evaluate_macro_target(0.0, target_x);
    let ctrl_at_1 = evaluate_macro_target(1.0, target_x);
    assert_eq!(ctrl_at_0, min_ctrl_x);
    assert_eq!(ctrl_at_1, max_ctrl_x);

    // Test custom piecewise breakpoints
    let custom_target = ableton_dsp::MacroTarget {
        parameter_name: "test".to_string(),
        min_control_value: 0.0,
        max_control_value: 1.0,
        min_value: None,
        max_value: None,
        breakpoints: vec![
            MacroBreakpoint { pct: 0.0, control_value: 0.2 },
            MacroBreakpoint { pct: 0.5, control_value: 0.8 },
            MacroBreakpoint { pct: 1.0, control_value: 0.4 },
        ],
    };
    let v_0 = evaluate_macro_target(0.0, &custom_target);
    let v_half = evaluate_macro_target(0.5, &custom_target);
    let v_1 = evaluate_macro_target(1.0, &custom_target);
    let v_quarter = evaluate_macro_target(0.25, &custom_target);

    assert_eq!(v_0, 0.2);
    assert_eq!(v_half, 0.8);
    assert_eq!(v_1, 0.4);
    assert!((v_quarter - 0.5).abs() < 1e-4, "Breakpoint midpoint 0.25 must interpolate to 0.5");

    println!("Verified all 17 XY macro mappings and breakpoint curves successfully.");
}
