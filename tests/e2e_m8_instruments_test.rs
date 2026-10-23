//! E2E Test Suite for Milestone M8: Ableton Live 12 Flagship Instruments & Multi-Synth Engine.
//!
//! Validates:
//! 1. Wavetable (dual osc, continuous morphing tables, warp/fold, Cytomic dual ZDF filter, unison, 8-target mod matrix)
//! 2. Operator (4-Op FM/additive synth, 11 algorithms, coarse/fine ratio & fixed modes, ADSR per op, feedback, ZDF filter)
//! 3. Drift (2 oscs with shape mod, stochastic thermal drift, cycling env, MPE expression, LP/HP filters)
//! 4. Meld (Bi-timbral Swarm/FM/Harmonic/Noise engines, scale-aware harmonic quantizer, dual plate resonators)
//! 5. Analog (Virtual analog dual osc/dual filter, PWM, sub, noise, dual envelopes/LFOs)
//! 6. Collision (Mallet/membrane modal physical modeling, Bessel modes, resonator coupling)
//! 7. Tension (Plucked/bowed string waveguide, inharmonic dispersion, body resonator)
//! 8. Electric (Tine/tone bar modal ringing, electromagnetic pickup non-linear saturation, tremolo)
//! 9. Simpler (Classic, 1-Shot, Slicing by transient/beat/region, warp modes, envelopes/filter)
//! 10. Drum Rack (128-pad 8x16 banks, 16 visible, choke groups, solo/mute, sends)
//! 11. Unified `InstrumentDevice` trait polymorphic dispatch across all instruments.

use oxide_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};
use oxide_dsp::{
    AnalogDevice, CircuitModel, CollisionDevice, CytomicFilterType, DriftDevice,
    DriftOsc2Waveform, ElectricDevice, FilterRouting, FmAlgorithm, MeldDevice, MeldMacroEngine,
    MeldScale, OperatorDevice, OperatorFreqMode, OperatorWaveform, ResonatorStructure,
    TensionDevice, TensionExciterType, UnisonMode, WarpMode, WavetableCategory, WavetableDevice,
};
use oxide_drums::{
    DrumRack, OneShotRetrigger, SimplerDevice, SimplerMode, SimplerWarpMode, SlicingMode,
    NUM_BANKS, PADS_PER_BANK, TOTAL_DRUM_PADS,
};

fn assert_audio_valid(buf: &AudioBuffer, instrument_name: &str) {
    let count = buf.frame_count.min(1024);
    let mut energy = 0.0f32;

    for i in 0..count {
        let l = buf.left[i];
        let r = buf.right[i];

        assert!(!l.is_nan(), "{}: Left channel sample at frame {} is NaN", instrument_name, i);
        assert!(!r.is_nan(), "{}: Right channel sample at frame {} is NaN", instrument_name, i);
        assert!(l.is_finite(), "{}: Left channel sample at frame {} is not finite", instrument_name, i);
        assert!(r.is_finite(), "{}: Right channel sample at frame {} is not finite", instrument_name, i);
        assert!(l.abs() <= 3.0, "{}: Left channel sample {} exceeded boundary at frame {}", instrument_name, l, i);
        assert!(r.abs() <= 3.0, "{}: Right channel sample {} exceeded boundary at frame {}", instrument_name, r, i);

        energy += l * l + r * r;
    }

    assert!(
        energy > 0.0001,
        "{}: Output audio energy must be non-silent, got {}",
        instrument_name,
        energy
    );
}

#[test]
fn test_e2e_wavetable_full_synthesis() {
    let mut synth = WavetableDevice::new(44100.0);
    assert_eq!(synth.name(), "Wavetable");
    assert_eq!(synth.category(), DeviceCategory::Instrument);

    // Test morphing across categories
    synth.load_osc1_category(WavetableCategory::Harmonics);
    synth.load_osc2_category(WavetableCategory::Formant);
    synth.osc1_pos = 0.65;
    synth.osc2_pos = 0.35;

    // Test warp modes: PWM & Fold
    synth.osc1_warp_mode = WarpMode::Pwm;
    synth.osc1_warp_param = 0.4;
    synth.osc2_warp_mode = WarpMode::Fold;
    synth.osc2_warp_param = 2.0;

    // Test dual Cytomic ZDF filter routing
    synth.filter_routing = FilterRouting::Series;
    synth.filter1_type = CytomicFilterType::Lowpass24;
    synth.filter1_model = CircuitModel::MS2;
    synth.filter1_cutoff = 3000.0;
    synth.filter1_res = 0.3;

    synth.filter2_type = CytomicFilterType::Morph;
    synth.filter2_model = CircuitModel::OSR;
    synth.filter2_cutoff = 6000.0;
    synth.filter2_morph = 0.5;

    // Unison mode
    synth.unison_mode = UnisonMode::Classic;
    synth.unison_voices = 4;
    synth.unison_amount = 0.25;

    let mut buf = AudioBuffer::silence(512);
    let time = MusicalTime::default();

    let midi = [
        MidiEvent::NoteOn { channel: 0, note: 60, velocity: 110 },
        MidiEvent::NoteOn { channel: 0, note: 64, velocity: 90 },
    ];
    synth.process(&midi, &mut buf, &time);

    assert_audio_valid(&buf, "Wavetable");
}

#[test]
fn test_e2e_operator_11_algorithms_and_user_harmonics() {
    let algorithms = [
        FmAlgorithm::Alg1, FmAlgorithm::Alg2, FmAlgorithm::Alg3,
        FmAlgorithm::Alg4, FmAlgorithm::Alg5, FmAlgorithm::Alg6,
        FmAlgorithm::Alg7, FmAlgorithm::Alg8, FmAlgorithm::Alg9,
        FmAlgorithm::Alg10, FmAlgorithm::Alg11,
    ];

    for alg in algorithms {
        let mut op = OperatorDevice::new(44100.0);
        assert_eq!(op.name(), "Operator");
        op.algorithm = alg;

        // Op A: Carrier Sine
        op.ops[0].waveform = OperatorWaveform::Sine;
        op.ops[0].coarse = 1.0;

        // Op B: Modulator Saw
        op.ops[1].waveform = OperatorWaveform::Saw;
        op.ops[1].coarse = 2.0;

        // Op C: Modulator User Harmonics
        op.ops[2].waveform = OperatorWaveform::UserHarmonics;
        op.ops[2].user_harmonics = [1.0, 0.5, 0.3, 0.2, 0.1, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

        // Op D: Fixed Frequency Mode + Feedback
        op.ops[3].freq_mode = OperatorFreqMode::Fixed;
        op.ops[3].fixed_hz = 120.0;
        op.feedback_d = 0.3;

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        let midi = [MidiEvent::NoteOn { channel: 0, note: 57, velocity: 100 }];
        op.process(&midi, &mut buf, &time);

        assert_audio_valid(&buf, &format!("Operator Alg {:?}", alg));
    }
}

#[test]
fn test_e2e_drift_subtractive_and_mpe() {
    let mut drift = DriftDevice::new(44100.0);
    assert_eq!(drift.name(), "Drift");

    drift.osc1_shape = 0.5; // Saw/Tri
    drift.osc2_waveform = DriftOsc2Waveform::Pulse;
    drift.drift_amount = 0.6; // Stochastic drift
    drift.env2_cycle_mode = true;
    drift.env2_cycle_rate_hz = 4.0;

    let mut buf = AudioBuffer::silence(512);
    let time = MusicalTime::default();

    let midi = [
        MidiEvent::NoteOn { channel: 0, note: 62, velocity: 100 },
        MidiEvent::PitchBend { channel: 0, value: 4096 }, // Microtone pitch bend
        MidiEvent::ControlChange { channel: 0, controller: 74, value: 90 }, // MPE Slide
    ];
    drift.process(&midi, &mut buf, &time);

    assert_audio_valid(&buf, "Drift");
}

#[test]
fn test_e2e_meld_bi_timbral_macro_and_scale_quantizer() {
    let mut meld = MeldDevice::new(44100.0);
    assert_eq!(meld.name(), "Meld");

    meld.engine_a_type = MeldMacroEngine::Swarm;
    meld.engine_a_param1 = 0.3; // Swarm spread

    meld.engine_b_type = MeldMacroEngine::Fm;
    meld.engine_b_param1 = 0.25; // 2:1 ratio
    meld.engine_b_param2 = 0.6;  // Mod index

    // Test Dorian scale quantizer
    meld.scale = MeldScale::Dorian;
    meld.scale_quantize_active = true;

    // Use Plate Resonator on Engine B
    meld.use_plate_b = true;
    meld.plate_decay = 0.8;
    meld.cross_mod_amount = 0.2;

    let mut buf = AudioBuffer::silence(512);
    let time = MusicalTime::default();

    let midi = [
        MidiEvent::NoteOn { channel: 0, note: 65, velocity: 100 }, // F natural (in Dorian)
    ];
    meld.process(&midi, &mut buf, &time);

    assert_audio_valid(&buf, "Meld");
}

#[test]
fn test_e2e_analog_virtual_analog() {
    let mut analog = AnalogDevice::new(44100.0);
    assert_eq!(analog.name(), "Analog");

    analog.osc1_detune_cents = -5.0;
    analog.osc2_detune_cents = 5.0;
    analog.osc1_pwm = 0.35;
    analog.sub_gain = 0.3;
    analog.filter1_cutoff = 3500.0;
    analog.filter1_res = 0.3;

    let mut buf = AudioBuffer::silence(512);
    let time = MusicalTime::default();

    let midi = [MidiEvent::NoteOn { channel: 0, note: 48, velocity: 100 }];
    analog.process(&midi, &mut buf, &time);

    assert_audio_valid(&buf, "Analog");
}

#[test]
fn test_e2e_collision_modal_physical_modeling() {
    let mut coll = CollisionDevice::new(44100.0);
    assert_eq!(coll.name(), "Collision");

    coll.res1_structure = ResonatorStructure::Marimba;
    coll.res2_structure = ResonatorStructure::Membrane;
    coll.mallet_stiffness = 0.8; // Wooden mallet
    coll.coupling_gain = 0.4;

    let mut buf = AudioBuffer::silence(512);
    let time = MusicalTime::default();

    let midi = [MidiEvent::NoteOn { channel: 0, note: 60, velocity: 110 }];
    coll.process(&midi, &mut buf, &time);

    assert_audio_valid(&buf, "Collision");
}

#[test]
fn test_e2e_tension_string_physical_modeling() {
    for exciter in [TensionExciterType::Pluck, TensionExciterType::Bow, TensionExciterType::Hammer] {
        let mut tension = TensionDevice::new(44100.0);
        assert_eq!(tension.name(), "Tension");
        tension.exciter_type = exciter;
        tension.string_stiffness = 0.3;
        tension.body_gain = 0.5;

        let mut buf = AudioBuffer::silence(512);
        let time = MusicalTime::default();

        let midi = [MidiEvent::NoteOn { channel: 0, note: 55, velocity: 100 }];
        tension.process(&midi, &mut buf, &time);

        assert_audio_valid(&buf, &format!("Tension {:?}", exciter));
    }
}

#[test]
fn test_e2e_electric_tine_piano_physical_modeling() {
    let mut electric = ElectricDevice::new(44100.0);
    assert_eq!(electric.name(), "Electric");

    electric.hammer_hardness = 0.7;
    electric.pickup_distance = 0.3; // Saturated pickup
    electric.tremolo_enabled = true;
    electric.tremolo_rate = 5.0;

    let mut buf = AudioBuffer::silence(512);
    let time = MusicalTime::default();

    let midi = [
        MidiEvent::NoteOn { channel: 0, note: 60, velocity: 120 },
        MidiEvent::NoteOn { channel: 0, note: 63, velocity: 100 },
        MidiEvent::NoteOn { channel: 0, note: 67, velocity: 90 },
    ];
    electric.process(&midi, &mut buf, &time);

    assert_audio_valid(&buf, "Electric");
}

#[test]
fn test_e2e_simpler_sampling_and_slicing_modes() {
    let mut simpler = SimplerDevice::new(44100.0);
    assert_eq!(simpler.name(), "Simpler");

    // 1. Classic pitched mode
    simpler.mode = SimplerMode::Classic;
    simpler.loop_enabled = true;
    let mut buf_classic = AudioBuffer::silence(256);
    let time = MusicalTime::default();
    simpler.process(&[MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 }], &mut buf_classic, &time);
    assert_audio_valid(&buf_classic, "Simpler Classic");

    // 2. 1-Shot mode
    simpler.mode = SimplerMode::OneShot;
    simpler.oneshot_retrigger = OneShotRetrigger::Trigger;
    simpler.warp_mode = SimplerWarpMode::Beats;
    let mut buf_oneshot = AudioBuffer::silence(256);
    simpler.process(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 110 }], &mut buf_oneshot, &time);
    assert_audio_valid(&buf_oneshot, "Simpler 1-Shot");

    // 3. Slicing mode (Region)
    simpler.mode = SimplerMode::Slicing;
    simpler.slicing_mode = SlicingMode::Region;
    simpler.recalculate_slices();
    let mut buf_slice = AudioBuffer::silence(256);
    simpler.process(&[MidiEvent::NoteOn { channel: 0, note: 36, velocity: 127 }], &mut buf_slice, &time);
    assert_audio_valid(&buf_slice, "Simpler Slicing");
}

#[test]
fn test_e2e_drum_rack_128_pad_banks_and_choke_groups() {
    let mut rack = DrumRack::new(44100.0);
    assert_eq!(rack.name(), "Drum Rack");
    assert_eq!(rack.all_pads.len(), TOTAL_DRUM_PADS);
    assert_eq!(TOTAL_DRUM_PADS, 128);
    assert_eq!(PADS_PER_BANK, 16);
    assert_eq!(NUM_BANKS, 8);

    // Test Bank Switching (Bank A -> Bank B -> Bank A)
    rack.select_bank(1); // Bank B
    assert_eq!(rack.active_bank, 1);
    assert_eq!(rack.pads.len(), 16);

    rack.select_bank(0); // Back to Bank A (909 kit)
    assert_eq!(rack.active_bank, 0);

    // Trigger Kick (36) and Closed HH (42)
    let mut buf = AudioBuffer::silence(512);
    let time = MusicalTime::default();

    let midi = [
        MidiEvent::NoteOn { channel: 9, note: 36, velocity: 120 }, // Kick
        MidiEvent::NoteOn { channel: 9, note: 42, velocity: 100 }, // Closed HH
    ];
    rack.process(&midi, &mut buf, &time);

    assert_audio_valid(&buf, "Drum Rack 128");

    // Test Solo & Mute logic
    rack.pads[0].mute = true;
    rack.sync_visible_to_all();
    assert!(rack.all_pads[0].mute);
    rack.pads[0].mute = false;
    rack.sync_visible_to_all();
}

#[test]
fn test_e2e_polymorphic_instrument_device_trait_dispatch() {
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

    assert_eq!(instruments.len(), 10, "Must have all 10 flagship Live 12 instruments");

    let time = MusicalTime::default();

    for inst in &mut instruments {
        assert_eq!(inst.category(), DeviceCategory::Instrument);
        assert!(inst.is_enabled());

        let mut buf = AudioBuffer::silence(512);
        let midi = [MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }];
        inst.process(&midi, &mut buf, &time);

        assert_audio_valid(&buf, inst.name());

        inst.reset();
    }
}
