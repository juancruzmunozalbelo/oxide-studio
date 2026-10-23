//! Adversarial Empirical Stress-Test Suite for Milestone M8:
//! Percussion, Physical Models, and Polymorphic Trait Dispatch.

use std::sync::Arc;
use oxide_core::{AudioBuffer, InstrumentDevice, MidiEvent, MusicalTime};
use oxide_dsp::{
    AnalogDevice, CollisionDevice, DriftDevice, ElectricDevice, MeldDevice,
    OperatorDevice, ResonatorStructure, TensionDevice, TensionExciterType,
    WavetableDevice,
};
use oxide_drums::{
    DrumPadSource, DrumRack, OneShotRetrigger, SimplerDevice, SimplerMode,
    SlicingMode, NUM_BANKS, PADS_PER_BANK,
};

/// Helper: verifies buffer is finite, non-NaN, and bounded.
fn verify_buffer_finite(buf: &AudioBuffer, name: &str, max_amp: f32) {
    let count = buf.frame_count.min(1024);
    for i in 0..count {
        let l = buf.left[i];
        let r = buf.right[i];
        assert!(!l.is_nan(), "{}: Left channel is NaN at frame {}", name, i);
        assert!(!r.is_nan(), "{}: Right channel is NaN at frame {}", name, i);
        assert!(l.is_finite(), "{}: Left channel is Inf at frame {}", name, i);
        assert!(r.is_finite(), "{}: Right channel is Inf at frame {}", name, i);
        assert!(
            l.abs() <= max_amp,
            "{}: Left channel exceeded max amplitude {} (got {}) at frame {}",
            name, max_amp, l, i
        );
        assert!(
            r.abs() <= max_amp,
            "{}: Right channel exceeded max amplitude {} (got {}) at frame {}",
            name, max_amp, r, i
        );
    }
}

// ============================================================================
// 1. SIMPLER ADVERSARIAL TESTS
// ============================================================================

#[test]
fn test_adv_simpler_zero_length_and_corrupt_slices() {
    let mut simpler = SimplerDevice::new(44100.0);

    // Scenario 1.1: Slicing mode with identical/duplicate slice indices [100, 100, 100]
    simpler.mode = SimplerMode::Slicing;
    simpler.slicing_mode = SlicingMode::Manual;
    simpler.slices = vec![100, 100, 100, 500];

    // Trigger slice 0 (which has slice_start == 100, slice_end == 100)
    simpler.trigger_note_on(36, 100);

    let mut buf = AudioBuffer::silence(256);
    let time = MusicalTime::default();
    simpler.process(&[], &mut buf, &time);
    verify_buffer_finite(&buf, "Simpler Zero-Length Slice", 2.0);

    // Scenario 1.2: Slicing mode with empty slice list
    simpler.slices = vec![];
    simpler.trigger_note_on(36, 120);
    simpler.process(&[], &mut buf, &time);
    verify_buffer_finite(&buf, "Simpler Empty Slices", 2.0);

    // Scenario 1.3: Slicing mode with note way below slice_base_note and above 127
    simpler.slices = vec![0, 500, 1000];
    simpler.trigger_note_on(0, 127);   // note < 36
    simpler.trigger_note_on(127, 127); // note > 36 + slices.len()
    simpler.process(&[], &mut buf, &time);
    verify_buffer_finite(&buf, "Simpler Note Boundaries", 2.0);

    // Scenario 1.4: Classic mode with length_pos = 0.0, start_pos = 1.0, loop_enabled = true
    simpler.mode = SimplerMode::Classic;
    simpler.start_pos = 1.0;
    simpler.length_pos = 0.00001; // Extremely small slice
    simpler.loop_enabled = true;
    simpler.trigger_note_on(60, 100);
    let mut loop_buf = AudioBuffer::silence(1024);
    simpler.process(&[], &mut loop_buf, &time);
    verify_buffer_finite(&loop_buf, "Simpler 1-Sample Loop", 2.0);

    // Scenario 1.5: Empty sample data
    simpler.load_sample(vec![], 44100, 1);
    simpler.trigger_note_on(60, 100);
    let mut empty_buf = AudioBuffer::silence(256);
    simpler.process(&[], &mut empty_buf, &time);
    verify_buffer_finite(&empty_buf, "Simpler Empty Sample Buffer", 2.0);
}

#[test]
fn test_adv_simpler_extreme_sample_rates() {
    let rates = [8000.0, 11025.0, 22050.0, 88200.0, 192000.0, 384000.0];
    for &sr in &rates {
        let mut simpler = SimplerDevice::new(sr);
        assert_eq!(simpler.sample_rate, sr);

        // Load 8000 Hz sample into high sample rate engine, and vice versa
        let dummy_samples: Vec<f32> = (0..2048)
            .map(|i| (i as f32 * 0.05).sin())
            .collect();
        simpler.load_sample(dummy_samples, 8000, 1);

        // Filter cutoff = 18000 Hz (which exceeds Nyquist if sr = 8000 or 11025)
        simpler.filter_enabled = true;
        simpler.filter_cutoff = 18000.0;
        simpler.filter_res = 0.95; // High resonance near self-oscillation

        simpler.trigger_note_on(84, 120); // High pitch note
        let mut buf = AudioBuffer::silence(512);
        let time = MusicalTime::default();
        simpler.process(&[], &mut buf, &time);

        verify_buffer_finite(&buf, &format!("Simpler at SR={}", sr), 3.0);
    }
}

#[test]
fn test_adv_simpler_rapid_retrigger_oneshot_and_slicing() {
    let mut simpler = SimplerDevice::new(44100.0);
    simpler.mode = SimplerMode::OneShot;
    simpler.oneshot_retrigger = OneShotRetrigger::Trigger;

    let time = MusicalTime::default();

    // 100 rapid retriggers across a stream of small blocks (32 samples each)
    let mut total_energy = 0.0f32;
    for note_num in 0..100 {
        let note = 36 + (note_num % 24) as u8;
        let midi = [MidiEvent::NoteOn { channel: 0, note, velocity: 100 }];
        let mut buf = AudioBuffer::silence(32);
        simpler.process(&midi, &mut buf, &time);
        verify_buffer_finite(&buf, "Simpler Rapid Retrigger OneShot", 3.0);
        for i in 0..32 {
            total_energy += buf.left[i] * buf.left[i];
        }
    }
    assert!(total_energy > 0.01, "Rapid retrigger must produce active audio energy");

    // Slicing mode rapid retrigger
    simpler.mode = SimplerMode::Slicing;
    simpler.slicing_mode = SlicingMode::Region;
    simpler.recalculate_slices();

    for note_num in 0..100 {
        let note = 36 + (note_num % 16) as u8;
        let midi = [
            MidiEvent::NoteOn { channel: 0, note, velocity: 110 },
            MidiEvent::NoteOff { channel: 0, note },
        ];
        let mut buf = AudioBuffer::silence(32);
        simpler.process(&midi, &mut buf, &time);
        verify_buffer_finite(&buf, "Simpler Slicing Retrigger", 3.0);
    }
}

// ============================================================================
// 2. DRUM RACK ADVERSARIAL TESTS
// ============================================================================

#[test]
fn test_adv_drum_rack_128_pad_bank_boundaries_and_routing() {
    let mut rack = DrumRack::new(44100.0);
    let time = MusicalTime::default();

    // Test all 8 banks (0..7)
    for bank_idx in 0..NUM_BANKS {
        rack.select_bank(bank_idx);
        assert_eq!(rack.active_bank, bank_idx);
        assert_eq!(rack.pads.len(), PADS_PER_BANK);
    }

    // Verify bank boundary pads (first and last pad in every bank)
    for bank in 0..NUM_BANKS {
        rack.select_bank(bank);
        let first_pad_note = rack.pads[0].midi_note;
        let last_pad_note = rack.pads[PADS_PER_BANK - 1].midi_note;

        rack.trigger_midi_note(first_pad_note, 100);
        rack.trigger_midi_note(last_pad_note, 100);

        let mut buf = AudioBuffer::silence(128);
        rack.process(&[], &mut buf, &time);
        verify_buffer_finite(&buf, &format!("DrumRack Bank {} Boundaries", bank), 4.0);
    }

    // Bank switching out-of-range does not panic
    rack.select_bank(8);
    rack.select_bank(99);
}

#[test]
fn test_adv_drum_rack_choke_group_exponential_decay_and_clickless() {
    let sample_rate = 44100.0;
    let mut rack = DrumRack::new(sample_rate);
    let time = MusicalTime::default();

    // Step 1: Set pad 10 (Open HH, choke group 1) to a steady test tone sample
    // to measure exact choke decay kinetics and smoothness
    let tone_len = 88200; // 2 seconds
    let tone_data: Vec<f32> = (0..tone_len)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
        .collect();
    rack.pads[10].source = DrumPadSource::Sample {
        sample_data: Arc::from(tone_data.into_boxed_slice()),
        channels: 1,
        sample_rate: 44100,
    };
    rack.pads[10].decay_seconds = 2.0;
    rack.sync_visible_to_all();

    // Trigger Open HH
    rack.trigger_pad(10, 127);

    // Warm up 256 samples
    let mut warmup_buf = AudioBuffer::silence(256);
    rack.process(&[], &mut warmup_buf, &time);

    // Find the voice playing pad 10
    let v_idx = rack.voices.iter().position(|v| v.active && v.choke_group == Some(1))
        .expect("Voice for pad 10 must be active");
    let initial_env = rack.voices[v_idx].envelope_val;
    assert!(initial_env > 0.9, "Voice envelope should be active");

    // Step 2: Choke it by triggering Closed HH (pad 6, choke group 1) with volume = 0
    // so we can isolate the fading out voice
    rack.pads[6].volume = 0.0;
    rack.sync_visible_to_all();
    rack.trigger_pad(6, 127);

    // Verify voice entered fading out state
    assert!(
        rack.voices[v_idx].is_fading_out,
        "Choked voice must have is_fading_out == true"
    );

    // Render 5ms (221 samples) of audio and check decay
    let five_ms_samples = (0.005 * sample_rate) as usize + 1; // 221 samples
    let mut decay_buf = AudioBuffer::silence(five_ms_samples);

    // Monitor continuity for pops/clicks
    let mut prev_sample = warmup_buf.left[255];
    let mut max_jump = 0.0f32;

    for frame in 0..five_ms_samples {
        let mut single = AudioBuffer::silence(1);
        rack.process(&[], &mut single, &time);
        let curr_sample = single.left[0];
        let jump = (curr_sample - prev_sample).abs();
        if jump > max_jump {
            max_jump = jump;
        }
        prev_sample = curr_sample;
        decay_buf.left[frame] = curr_sample;
    }

    // Envelope after 5ms must be decayed by at least -60 dB (factor of < 0.001)
    let remaining_env = rack.voices[v_idx].envelope_val;
    assert!(
        remaining_env <= 0.0015,
        "Choke decay must achieve <= 0.001 (-60 dB) in 5ms, got {}",
        remaining_env
    );

    // Jump should be smooth without discontinuities
    assert!(
        max_jump < 0.15,
        "Choke must be clickless without sudden pop discontinuity, max jump was {}",
        max_jump
    );

    // Step 3: Simultaneous trigger of ALL pads in the choke group at once (pads 6, 8, 10)
    rack.all_notes_off();
    let simultaneous_choke = [
        MidiEvent::NoteOn { channel: 9, note: 42, velocity: 100 }, // Closed HH
        MidiEvent::NoteOn { channel: 9, note: 44, velocity: 100 }, // Pedal HH
        MidiEvent::NoteOn { channel: 9, note: 46, velocity: 100 }, // Open HH
    ];
    let mut sim_buf = AudioBuffer::silence(512);
    rack.process(&simultaneous_choke, &mut sim_buf, &time);
    verify_buffer_finite(&sim_buf, "DrumRack Simultaneous Choke Trigger", 4.0);
}

#[test]
fn test_adv_drum_rack_empty_and_corrupt_sample_slots() {
    let mut rack = DrumRack::new(44100.0);
    let time = MusicalTime::default();

    // Configure pad 0 with an empty sample buffer
    rack.pads[0].source = DrumPadSource::Sample {
        sample_data: Arc::from([]),
        channels: 1,
        sample_rate: 44100,
    };
    rack.trigger_pad(0, 100);

    let mut buf = AudioBuffer::silence(256);
    rack.process(&[], &mut buf, &time);
    verify_buffer_finite(&buf, "DrumRack Empty Sample Slot", 1.0);

    // Configure pad 1 with a single sample frame, testing linear and cubic interpolation
    for use_cubic in [false, true] {
        rack.pads[1].use_cubic_interpolation = use_cubic;
        rack.pads[1].source = DrumPadSource::Sample {
            sample_data: Arc::from([0.5f32]),
            channels: 1,
            sample_rate: 44100,
        };
        rack.trigger_pad(1, 100);
        let mut single_buf = AudioBuffer::silence(128);
        rack.process(&[], &mut single_buf, &time);
        verify_buffer_finite(&single_buf, "DrumRack 1-Sample Buffer", 2.0);
    }
}

#[test]
fn test_adv_drum_rack_channels_zero_panics_div_by_zero() {
    let mut rack = DrumRack::new(44100.0);
    rack.pads[0].source = DrumPadSource::Sample {
        sample_data: Arc::from([1.0, 2.0]),
        channels: 0,
        sample_rate: 44100,
    };
    rack.trigger_pad(0, 100);
    let mut buf = AudioBuffer::silence(128);
    let time = MusicalTime::default();
    rack.process(&[], &mut buf, &time);
    verify_buffer_finite(&buf, "DrumRack Channels 0 Handled Safely", 1.0);
}


// ============================================================================
// 3. PHYSICAL MODELING ADVERSARIAL TESTS (Collision, Tension, Electric)
// ============================================================================

#[test]
fn test_adv_physical_collision_extreme_velocities_and_frequencies() {
    let mut coll = CollisionDevice::new(44100.0);
    let time = MusicalTime::default();

    // Extreme impact velocities: 1, 64, 127
    for vel in [1, 64, 127] {
        coll.all_notes_off();
        coll.trigger_note_on(60, vel);
        let mut buf = AudioBuffer::silence(256);
        coll.process(&[], &mut buf, &time);
        // Single voice amplitude is clamped to [-1.8, 1.8] * left_pan (~1.27)
        verify_buffer_finite(&buf, &format!("Collision Vel={}", vel), 2.5);
    }

    // Extreme structures & extreme material damping
    for struct_type in [
        ResonatorStructure::Beam,
        ResonatorStructure::Marimba,
        ResonatorStructure::Membrane,
        ResonatorStructure::Plate,
        ResonatorStructure::Pipe,
    ] {
        coll.all_notes_off();
        coll.res1_structure = struct_type;
        coll.res1_material = 1.0; // Max material damping
        coll.res1_decay = 0.05;   // Very short decay
        coll.mallet_stiffness = 1.0; // Hardest mallet

        coll.trigger_note_on(60, 127);
        let mut buf = AudioBuffer::silence(256);
        coll.process(&[], &mut buf, &time);
        verify_buffer_finite(&buf, &format!("Collision Struct {:?}", struct_type), 2.5);
    }

    // Zero-frequency / pitch extremes: MIDI note 0 (8 Hz) and note 127 (12.5 kHz)
    for &note in &[0, 1, 12, 120, 127] {
        coll.all_notes_off();
        coll.trigger_note_on(note, 120);
        let mut buf = AudioBuffer::silence(256);
        coll.process(&[], &mut buf, &time);
        verify_buffer_finite(&buf, &format!("Collision Extreme Note {}", note), 2.5);
    }
}

#[test]
fn test_adv_physical_tension_damping_and_stiffness_stability() {
    let mut tension = TensionDevice::new(44100.0);
    let time = MusicalTime::default();

    // Test all 3 exciter types (Pluck, Bow, Hammer) under extreme parameters
    for exciter in [TensionExciterType::Pluck, TensionExciterType::Bow, TensionExciterType::Hammer] {
        tension.exciter_type = exciter;

        // String damping extremes: 0.0 (near lossless) and 1.0 (heavy damping)
        for &damping in &[0.0, 0.5, 1.0] {
            tension.all_notes_off();
            tension.string_damping = damping;
            tension.string_decay_loss = 0.999;
            tension.string_stiffness = 0.8; // Maximum rated inharmonic stiffness

            tension.trigger_note_on(55, 127);
            let mut buf = AudioBuffer::silence(512);
            tension.process(&[], &mut buf, &time);
            verify_buffer_finite(&buf, &format!("Tension {:?} Damping {}", exciter, damping), 3.0);
        }
    }

    // Test extreme notes: Note 0 (lowest possible frequency) and Note 127 (highest)
    for &note in &[0, 5, 12, 120, 127] {
        tension.all_notes_off();
        tension.trigger_note_on(note, 120);
        let mut buf = AudioBuffer::silence(256);
        tension.process(&[], &mut buf, &time);
        verify_buffer_finite(&buf, &format!("Tension Extreme Note {}", note), 3.0);
    }
}

#[test]
fn test_adv_physical_electric_pickup_and_frequencies() {
    let mut electric = ElectricDevice::new(44100.0);
    let time = MusicalTime::default();

    // Extreme pickup distance: 0.05 (maximum non-linear saturation) and 1.0 (clean)
    for &dist in &[0.05, 0.1, 0.5, 1.0] {
        electric.all_notes_off();
        electric.pickup_distance = dist;
        electric.pickup_symmetry = 0.8; // Asymmetric saturation
        electric.hammer_hardness = 1.0; // Hardest neoprene hammer

        electric.trigger_note_on(60, 127);
        let mut buf = AudioBuffer::silence(512);
        electric.process(&[], &mut buf, &time);
        verify_buffer_finite(&buf, &format!("Electric Pickup Dist {}", dist), 3.0);
    }

    // Extreme notes: 0, 127
    for &note in &[0, 24, 60, 96, 127] {
        electric.all_notes_off();
        electric.trigger_note_on(note, 120);
        let mut buf = AudioBuffer::silence(256);
        electric.process(&[], &mut buf, &time);
        verify_buffer_finite(&buf, &format!("Electric Note {}", note), 3.0);
    }
}

#[test]
fn test_adv_drum_rack_cross_bank_pad_collision() {
    let mut rack = DrumRack::new(44100.0);
    let time = MusicalTime::default();

    // Bank A is visible (active_bank = 0)
    rack.select_bank(0);
    assert_eq!(rack.active_bank, 0);

    // Pad 0 (Bank A, MIDI note 36) is Kick909 with volume 1.0
    // Pad 16 (Bank B pad 0, MIDI note 0) is customized with volume 0.0 (silent)
    rack.all_pads[16].volume = 0.0;

    // Trigger MIDI note 0 (which maps to pad 16 in all_pads)
    rack.trigger_midi_note(0, 100);

    let mut buf = AudioBuffer::silence(256);
    rack.process(&[], &mut buf, &time);

    // If Bank B pad 16 volume is 0.0, output energy MUST be zero (silent).
    // But due to `voice.pad_idx = i % 16` and `&self.pads[voice.pad_idx]`,
    // it reads `self.pads[0]` (Bank A Kick, volume 1.0) and produces sound!
    let mut energy = 0.0f32;
    for i in 0..256 {
        energy += buf.left[i] * buf.left[i] + buf.right[i] * buf.right[i];
    }

    println!("Cross-bank note 0 audio energy (expected 0.0 since all_pads[16].volume=0.0): {}", energy);
    // This demonstrates the architectural defect:
    // When Bank A is visible, triggering Bank B Pad 0 sounds Bank A Pad 0 instead!
    assert_eq!(
        energy, 0.0,
        "CRITICAL BUG: Bank B Pad 16 volume is 0.0, but rendered audio with energy {} because process_block indexed into self.pads[0] instead of all_pads[16]!",
        energy
    );
}

// ============================================================================
// 4. POLYMORPHIC INSTRUMENT DEVICE STRESS TESTS
// ============================================================================

#[test]
fn test_adv_polymorphic_instrument_rapid_interleaved_stress() {
    let sample_rate = 44100.0;
    let mut instruments: Vec<Box<dyn InstrumentDevice>> = vec![
        Box::new(WavetableDevice::new(sample_rate)),
        Box::new(OperatorDevice::new(sample_rate)),
        Box::new(DriftDevice::new(sample_rate)),
        Box::new(MeldDevice::new(sample_rate)),
        Box::new(AnalogDevice::new(sample_rate)),
        Box::new(CollisionDevice::new(sample_rate)),
        Box::new(TensionDevice::new(sample_rate)),
        Box::new(ElectricDevice::new(sample_rate)),
        Box::new(SimplerDevice::new(sample_rate)),
        Box::new(DrumRack::new(sample_rate)),
    ];

    let time = MusicalTime::default();

    // 50 iterations of rapid, interleaved NoteOn and NoteOff events with varying buffer sizes
    // Polyphonic chords (up to 8 voices) can sum up to ~20.0 in 32-bit float without clipping
    for iter in 0..50 {
        let block_size = match iter % 4 {
            0 => 64,
            1 => 128,
            2 => 256,
            _ => 512,
        };

        let note1 = (36 + (iter * 7) % 60) as u8;
        let note2 = (40 + (iter * 11) % 56) as u8;

        // Build burst of MIDI events: NoteOn, NoteOff, rapid pitch changes
        let midi_burst = [
            MidiEvent::NoteOn { channel: 0, note: note1, velocity: 110 },
            MidiEvent::NoteOn { channel: 0, note: note2, velocity: 90 },
            MidiEvent::NoteOff { channel: 0, note: note1 },
            MidiEvent::NoteOn { channel: 0, note: note1 + 2, velocity: 100 },
        ];

        for inst in &mut instruments {
            let mut buf = AudioBuffer::silence(block_size);
            inst.process(&midi_burst, &mut buf, &time);
            verify_buffer_finite(&buf, inst.name(), 25.0);

            if iter % 10 == 9 {
                inst.all_notes_off();
            }
            if iter % 25 == 24 {
                inst.reset();
            }
        }
    }
}

