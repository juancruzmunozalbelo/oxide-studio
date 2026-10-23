//! Adversarial Empirical Stress Test Suite for Milestone M8:
//! Filters, Wavetable, Operator, Drift & Meld Synthesizers.
//!
//! Objectives:
//! 1. Cytomic ZDF Filter: Test extreme cutoff sweeps (10 Hz to 22 kHz), resonance = 1.0 (self-oscillation),
//!    extreme drive values across all 5 models (Clean, OSR, MS2, SMP, PRD). Verify output is strictly bounded
//!    (|y| < 2.5) and contains NO NaN or Inf.
//! 2. Wavetable: Test rapid continuous wavetable position sweeps, extreme Warp/Fold values, unison 8-voice stacking.
//!    Verify stability and numerical integrity.
//! 3. Operator: Test high frequency FM ratios (e.g. 32x), maximum feedback, rapid polyphonic note onslaught.
//! 4. Drift & Meld: Test high-rate MPE pitch bends, scale quantizer with out-of-scale frequencies,
//!    plate resonator stability under impulsive excitation.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, InstrumentDevice, MidiEvent, MusicalTime};
use oxide_dsp::{
    CircuitModel, CytomicFilterType, CytomicZdfFilter, DualCytomicFilter, FilterRouting,
    DriftDevice, FmAlgorithm, MeldDevice, MeldMacroEngine, MeldScale, PlateResonator,
    OperatorDevice, OperatorFreqMode, OperatorWaveform,
    UnisonMode, WarpMode, WavetableCategory, WavetableDevice,
};

/// Assert sample is strictly bounded, finite, and non-NaN.
#[inline(always)]
fn assert_sample_bounded(y: f32, max_bound: f32, ctx: &str) {
    assert!(!y.is_nan(), "{}: Sample is NaN", ctx);
    assert!(y.is_finite(), "{}: Sample is not finite (Inf)", ctx);
    assert!(
        y.abs() < max_bound,
        "{}: Sample |{}| exceeded bound {}",
        ctx,
        y,
        max_bound
    );
}

// ============================================================================
// 1. CYTOMIC ZDF FILTER ADVERSARIAL STRESS TESTS
// ============================================================================

#[test]
fn test_adv_cytomic_extreme_sweeps_and_self_oscillation_all_models() {
    let circuit_models = [
        CircuitModel::Clean,
        CircuitModel::OSR,
        CircuitModel::MS2,
        CircuitModel::SMP,
        CircuitModel::PRD,
    ];

    let filter_types = [
        CytomicFilterType::Lowpass12,
        CytomicFilterType::Lowpass24,
        CytomicFilterType::Highpass12,
        CytomicFilterType::Highpass24,
        CytomicFilterType::Bandpass12,
        CytomicFilterType::Bandpass24,
        CytomicFilterType::Notch12,
        CytomicFilterType::Morph,
    ];

    let sample_rates = [44100.0, 48000.0, 96000.0];
    let drives = [0.0, 6.0, 12.0, 18.0, 24.0]; // dB

    for &sr in &sample_rates {
        for &model in &circuit_models {
            for &ftype in &filter_types {
                for &drive_db in &drives {
                    let mut filter = CytomicZdfFilter::new(sr);
                    filter.circuit_model = model;
                    filter.filter_type = ftype;

                    // 1. Extreme Linear Sweep UP: 10 Hz -> 22,000 Hz with Res = 1.0 (self-oscillation)
                    // Input: Dirac impulse at sample 0, then silence to test sustained self-oscillation
                    let num_samples = 2048;
                    for i in 0..num_samples {
                        let t = i as f32 / num_samples as f32;
                        let cutoff = 10.0 + t * (22000.0 - 10.0);
                        let input = if i == 0 { 1.0 } else { 0.0 };
                        let morph = t; // morph sweep if Morph type

                        let out = filter.process_sample(input, cutoff, 1.0, morph, drive_db);
                        assert_sample_bounded(
                            out,
                            2.5,
                            &format!(
                                "ZDF Sweep UP: sr={}, model={:?}, type={:?}, drive={}dB, frame={}",
                                sr, model, ftype, drive_db, i
                            ),
                        );
                    }

                    // 2. Extreme Linear Sweep DOWN: 22,000 Hz -> 10 Hz with Full-Scale White Noise
                    filter.reset();
                    let mut rng_state: u32 = 0x98765432;
                    for i in 0..num_samples {
                        let t = i as f32 / num_samples as f32;
                        let cutoff = 22000.0 - t * (22000.0 - 10.0);
                        // PRNG noise
                        rng_state ^= rng_state << 13;
                        rng_state ^= rng_state >> 17;
                        rng_state ^= rng_state << 5;
                        let noise = (rng_state as i32 as f32) * 4.65661287e-10;

                        let out = filter.process_sample(noise, cutoff, 1.0, 1.0 - t, drive_db);
                        assert_sample_bounded(
                            out,
                            2.5,
                            &format!(
                                "ZDF Sweep DOWN: sr={}, model={:?}, type={:?}, drive={}dB, frame={}",
                                sr, model, ftype, drive_db, i
                            ),
                        );
                    }

                    // 3. Instantaneous Square Cutoff Switching: 10 Hz <-> 22,000 Hz every 32 samples
                    // With alternating Nyquist spikes (+10.0, -10.0)
                    filter.reset();
                    for i in 0..1024 {
                        let cutoff = if (i / 32) % 2 == 0 { 10.0 } else { 22000.0 };
                        let input = if i % 2 == 0 { 10.0 } else { -10.0 };

                        let out = filter.process_sample(input, cutoff, 1.0, 0.5, drive_db);
                        assert_sample_bounded(
                            out,
                            2.5,
                            &format!(
                                "ZDF Square Cutoff: sr={}, model={:?}, type={:?}, drive={}dB, frame={}",
                                sr, model, ftype, drive_db, i
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn test_adv_cytomic_dual_filter_topologies_and_cross_sweeps() {
    let routings = [FilterRouting::Series, FilterRouting::Parallel, FilterRouting::Split];
    let sr = 48000.0;

    for &routing in &routings {
        let mut dual = DualCytomicFilter::new(sr);
        dual.routing = routing;
        dual.filter1.circuit_model = CircuitModel::MS2;
        dual.filter1.filter_type = CytomicFilterType::Lowpass24;
        dual.filter2.circuit_model = CircuitModel::PRD;
        dual.filter2.filter_type = CytomicFilterType::Highpass24;

        for i in 0..2048 {
            let t = i as f32 / 2048.0;
            let cut1 = 10.0 + t * 21990.0;
            let cut2 = 22000.0 - t * 21990.0;
            let s1 = (2.0 * PI * 440.0 * (i as f32 / sr)).sin();
            let s2 = (2.0 * PI * 880.0 * (i as f32 / sr)).cos();

            let out = dual.process(s1, s2, cut1, 1.0, 0.0, cut2, 1.0, 0.0);
            assert_sample_bounded(
                out,
                2.5,
                &format!("DualCytomic {:?} cross-sweep at frame {}", routing, i),
            );
        }
    }
}

// ============================================================================
// 2. WAVETABLE ADVERSARIAL STRESS TESTS
// ============================================================================

#[test]
fn test_adv_wavetable_rapid_continuous_position_sweeps() {
    let categories = [
        WavetableCategory::BasicShapes,
        WavetableCategory::Harmonics,
        WavetableCategory::Formant,
        WavetableCategory::Digital,
        WavetableCategory::Vintage,
    ];

    let mut wt = WavetableDevice::new(44100.0);
    wt.trigger_note_on(60, 110);
    wt.trigger_note_on(67, 100);

    for &cat in &categories {
        wt.load_osc1_category(cat);
        wt.load_osc2_category(cat);

        // Sweep position continuously at audio rate (triangle wave)
        for i in 0..4096 {
            let t = (i as f32 * 0.05).sin().abs();
            wt.osc1_pos = t;
            wt.osc2_pos = 1.0 - t;

            let (l, r) = wt.process_stereo_sample();
            assert_sample_bounded(l, 3.5, &format!("Wavetable Pos Sweep Osc1 ({:?}) L frame {}", cat, i));
            assert_sample_bounded(r, 3.5, &format!("Wavetable Pos Sweep Osc2 ({:?}) R frame {}", cat, i));
        }
    }
}

#[test]
fn test_adv_wavetable_extreme_warp_and_fold_values() {
    let mut wt = WavetableDevice::new(44100.0);
    wt.trigger_note_on(55, 120);

    // PWM extreme values: outside standard [-0.9, 0.9]
    let pwm_params = [-5.0, -1.5, -1.0, -0.9, 0.0, 0.9, 1.0, 1.5, 10.0];
    wt.osc1_warp_mode = WarpMode::Pwm;
    for &p in &pwm_params {
        wt.osc1_warp_param = p;
        for i in 0..512 {
            let (l, r) = wt.process_stereo_sample();
            assert_sample_bounded(l, 3.5, &format!("Wavetable PWM param {} L frame {}", p, i));
            assert_sample_bounded(r, 3.5, &format!("Wavetable PWM param {} R frame {}", p, i));
        }
    }

    // Fold extreme values
    let fold_params = [-10.0, 0.0, 1.0, 4.0, 8.0, 16.0, 100.0];
    wt.osc1_warp_mode = WarpMode::Fold;
    for &p in &fold_params {
        wt.osc1_warp_param = p;
        for i in 0..512 {
            let (l, r) = wt.process_stereo_sample();
            assert_sample_bounded(l, 3.5, &format!("Wavetable Fold param {} L frame {}", p, i));
            assert_sample_bounded(r, 3.5, &format!("Wavetable Fold param {} R frame {}", p, i));
        }
    }

    // Sync extreme values
    let sync_params = [-5.0, 0.0, 1.0, 7.0, 8.0, 16.0, 64.0];
    wt.osc1_warp_mode = WarpMode::Sync;
    for &p in &sync_params {
        wt.osc1_warp_param = p;
        for i in 0..512 {
            let (l, r) = wt.process_stereo_sample();
            assert_sample_bounded(l, 3.5, &format!("Wavetable Sync param {} L frame {}", p, i));
            assert_sample_bounded(r, 3.5, &format!("Wavetable Sync param {} R frame {}", p, i));
        }
    }
}

#[test]
fn test_adv_wavetable_unison_8_voice_stacking_and_stealing() {
    let mut wt = WavetableDevice::new(44100.0);
    let unison_modes = [
        UnisonMode::Classic,
        UnisonMode::Shimmer,
        UnisonMode::Noise,
        UnisonMode::PhaseSync,
    ];

    for &mode in &unison_modes {
        wt.silence_all();
        wt.unison_mode = mode;
        wt.unison_voices = 8;
        wt.unison_amount = 0.8;

        // Trigger all 8 polyphonic voices simultaneously
        let chord = [48, 52, 55, 59, 62, 65, 69, 72];
        for &note in &chord {
            wt.trigger_note_on(note, 127);
        }

        // Render 2048 frames of 8-voice stack
        for i in 0..2048 {
            let (l, r) = wt.process_stereo_sample();
            assert_sample_bounded(l, 8.0, &format!("Wavetable 8-voice Unison {:?} L frame {}", mode, i));
            assert_sample_bounded(r, 8.0, &format!("Wavetable 8-voice Unison {:?} R frame {}", mode, i));
        }

        // Rapid onslaught to stress voice stealing while 8 voices are active
        for onslaught_note in 36..=52 {
            wt.trigger_note_on(onslaught_note, 127);
            for _ in 0..64 {
                let (l, r) = wt.process_stereo_sample();
                assert_sample_bounded(l, 8.0, &format!("Wavetable Voice Steal L on note {}", onslaught_note));
                assert_sample_bounded(r, 8.0, &format!("Wavetable Voice Steal R on note {}", onslaught_note));
            }
        }
    }
}

// ============================================================================
// 3. OPERATOR ADVERSARIAL STRESS TESTS
// ============================================================================

#[test]
fn test_adv_operator_high_frequency_fm_ratios_and_fixed_mode() {
    let mut op = OperatorDevice::new(44100.0);

    // Set extreme coarse ratios (up to 32x) and fine tuning (+100 cents)
    op.ops[0].coarse = 1.0;
    op.ops[1].coarse = 16.0;
    op.ops[2].coarse = 24.0;
    op.ops[3].coarse = 32.0;
    op.ops[3].fine = 100.0;

    let waveforms = [
        OperatorWaveform::Sine,
        OperatorWaveform::Saw,
        OperatorWaveform::Square,
        OperatorWaveform::UserHarmonics,
    ];

    for &wave in &waveforms {
        for i in 0..4 {
            op.ops[i].waveform = wave;
            if wave == OperatorWaveform::UserHarmonics {
                op.ops[i].user_harmonics = [1.0; 16]; // All 16 harmonics at full amplitude
            }
        }

        // High notes up to 127
        for &note in &[84, 96, 108, 120, 127] {
            op.silence_all();
            op.trigger_note_on(note, 127);

            for f in 0..1024 {
                let (l, r) = op.process_stereo_sample();
                assert_sample_bounded(
                    l,
                    5.0,
                    &format!("Operator High-FM Wave {:?} Note {} L frame {}", wave, note, f),
                );
                assert_sample_bounded(
                    r,
                    5.0,
                    &format!("Operator High-FM Wave {:?} Note {} R frame {}", wave, note, f),
                );
            }
        }

        // Fixed frequency mode at near-Nyquist (20,000 Hz)
        op.ops[3].freq_mode = OperatorFreqMode::Fixed;
        op.ops[3].fixed_hz = 20000.0;
        op.ops[2].freq_mode = OperatorFreqMode::Fixed;
        op.ops[2].fixed_hz = 15000.0;

        op.silence_all();
        op.trigger_note_on(60, 127);
        for f in 0..1024 {
            let (l, r) = op.process_stereo_sample();
            assert_sample_bounded(
                l,
                5.0,
                &format!("Operator Fixed-Hz 20kHz Wave {:?} L frame {}", wave, f),
            );
            assert_sample_bounded(
                r,
                5.0,
                &format!("Operator Fixed-Hz 20kHz Wave {:?} R frame {}", wave, f),
            );
        }
    }
}

#[test]
fn test_adv_operator_maximum_feedback_across_all_11_algorithms() {
    let mut op = OperatorDevice::new(44100.0);
    // Maximum feedback on Op D
    op.feedback_d = 1.0;
    op.ops[3].level = 1.0;

    let algorithms = [
        FmAlgorithm::Alg1,
        FmAlgorithm::Alg2,
        FmAlgorithm::Alg3,
        FmAlgorithm::Alg4,
        FmAlgorithm::Alg5,
        FmAlgorithm::Alg6,
        FmAlgorithm::Alg7,
        FmAlgorithm::Alg8,
        FmAlgorithm::Alg9,
        FmAlgorithm::Alg10,
        FmAlgorithm::Alg11,
    ];

    for &alg in &algorithms {
        op.silence_all();
        op.algorithm = alg;
        op.trigger_note_on(60, 127);

        // Run 4096 samples under max feedback
        for i in 0..4096 {
            let (l, r) = op.process_stereo_sample();
            assert_sample_bounded(l, 5.0, &format!("Operator Max FB {:?} L frame {}", alg, i));
            assert_sample_bounded(r, 5.0, &format!("Operator Max FB {:?} R frame {}", alg, i));

            // Verify feedback state variables in active voices remain finite
            for v in &op.voices {
                if v.active {
                    assert!(v.fb_y1.is_finite(), "Feedback fb_y1 blew up in {:?}", alg);
                    assert!(v.fb_y2.is_finite(), "Feedback fb_y2 blew up in {:?}", alg);
                }
            }
        }
    }
}

#[test]
fn test_adv_operator_rapid_polyphonic_note_onslaught() {
    let mut op = OperatorDevice::new(44100.0);
    op.pitch_env_enabled = true;
    op.pitch_env_semitones = 48.0; // extreme +4 octave pitch sweep

    let mut rng: u32 = 0x12345678;
    for i in 0..512 {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let note = (rng % 128) as u8;
        let is_on = (rng % 3) != 0;

        if is_on {
            op.trigger_note_on(note, 127);
        } else {
            op.trigger_note_off(note);
        }

        // Render 16 samples between rapid MIDI triggers
        for f in 0..16 {
            let (l, r) = op.process_stereo_sample();
            assert_sample_bounded(l, 8.0, &format!("Operator Onslaught event {} frame {}", i, f));
            assert_sample_bounded(r, 8.0, &format!("Operator Onslaught event {} frame {}", i, f));
        }
    }
}

// ============================================================================
// 4. DRIFT & MELD ADVERSARIAL STRESS TESTS
// ============================================================================

#[test]
fn test_adv_drift_mpe_pitch_bends_and_thermal_drift() {
    let mut drift = DriftDevice::new(44100.0);
    drift.drift_amount = 1.0; // Maximum thermal drift instability
    drift.env2_cycle_mode = true;
    drift.env2_cycle_rate_hz = 200.0; // Fast cycling envelope LFO

    let mut buf = AudioBuffer::silence(256);
    let time = MusicalTime::default();

    // Note On
    drift.process(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 120 }], &mut buf, &time);

    // Rapid high-rate MPE pitch bends alternating extreme values
    for cycle in 0..100 {
        buf.clear();
        let bend_val = if cycle % 2 == 0 { 8191 } else { -8192 };
        let slide_val = ((cycle * 7) % 128) as u8;

        let midi = [
            MidiEvent::PitchBend { channel: 0, value: bend_val },
            MidiEvent::ControlChange { channel: 0, controller: 74, value: slide_val },
        ];

        drift.process(&midi, &mut buf, &time);

        for f in 0..256 {
            let l = buf.left[f];
            let r = buf.right[f];
            assert_sample_bounded(l, 3.5, &format!("Drift MPE Cycle {} L frame {}", cycle, f));
            assert_sample_bounded(r, 3.5, &format!("Drift MPE Cycle {} R frame {}", cycle, f));
        }

        // Verify drift state variables remain clamped and finite
        for v in &drift.voices {
            if v.active {
                assert!(v.drift_pitch_cents.abs() <= 25.01, "Drift pitch cents out of bounds: {}", v.drift_pitch_cents);
                assert!(v.drift_cutoff_hz.abs() <= 100.01, "Drift cutoff hz out of bounds: {}", v.drift_cutoff_hz);
            }
        }
    }
}

#[test]
fn test_adv_meld_scale_quantizer_out_of_scale_and_extreme_frequencies() {
    let scales = [
        MeldScale::Chromatic,
        MeldScale::Major,
        MeldScale::Minor,
        MeldScale::Dorian,
        MeldScale::Phrygian,
        MeldScale::Pentatonic,
    ];

    let test_pitches = [
        // Out-of-scale microtonal fractional pitches
        60.12, 60.33, 60.5, 60.88, 66.5, 71.49, 12.75,
        // Large frequencies treated as pitches
        20.0, 440.0, 1000.0, 5000.0, 20000.0,
        // Extreme negative pitches
        -1.0, -12.0, -25.5, -100.0, -1000.0,
        // Extreme positive pitches
        150.0, 500.0, 10000.0,
    ];

    for &scale in &scales {
        for &pitch in &test_pitches {
            let quantized = scale.quantize_pitch(pitch);
            assert!(
                !quantized.is_nan(),
                "Quantized pitch was NaN for scale {:?} with input {}",
                scale,
                pitch
            );
            assert!(
                quantized.is_finite(),
                "Quantized pitch was Inf for scale {:?} with input {}",
                scale,
                pitch
            );

            // If input is near an integer, verify semitone of rounded quantized pitch is in scale
            let rounded = quantized.round() as i32;
            let semi = ((rounded % 12 + 12) % 12) as u8;
            assert!(
                scale.is_in_scale(semi),
                "Quantized pitch {} -> {} semi {} not in scale {:?}",
                pitch,
                quantized,
                semi,
                scale
            );
        }
    }
}

#[test]
fn test_adv_meld_plate_resonator_impulsive_excitation_stability() {
    let decays = [0.001, 0.01, 0.5, 2.0, 10.0, 60.0]; // seconds
    let dampings = [0.0, 0.2, 0.5, 1.0, 5.0];
    let base_freqs = [20.0, 100.0, 440.0, 2000.0, 10000.0, 20000.0];
    let sr = 48000.0;

    for &decay in &decays {
        for &damp in &dampings {
            for &freq in &base_freqs {
                let mut plate = PlateResonator::new();

                // 1. Dirac Impulse test (impulse amplitude = 50.0)
                let impulse_out = plate.process(50.0, freq, decay, damp, sr);
                assert_sample_bounded(
                    impulse_out,
                    50.0,
                    &format!("Plate impulse 50.0, freq={}, decay={}, damp={}", freq, decay, damp),
                );

                // Run 2048 samples of silent ring-out
                for f in 0..2048 {
                    let out = plate.process(0.0, freq, decay, damp, sr);
                    assert_sample_bounded(
                        out,
                        50.0,
                        &format!("Plate ringout frame {}, freq={}, decay={}, damp={}", f, freq, decay, damp),
                    );
                    if f > 1000 && decay < 0.1 {
                        // For short decay, energy must decay to near zero
                        assert!(out.abs() < 0.1, "Short decay {} must damp out by frame {}, got {}", decay, f, out);
                    }
                }

                // 2. Dense pulse train test
                plate.reset();
                for f in 0..1024 {
                    let in_val = if f % 32 == 0 { 5.0 } else { 0.0 };
                    let out = plate.process(in_val, freq, decay, damp, sr);
                    assert_sample_bounded(
                        out,
                        25.0,
                        &format!("Plate pulse train frame {}, freq={}, decay={}", f, freq, decay),
                    );
                }
            }
        }
    }
}

#[test]
fn test_adv_meld_device_swarm_and_cross_modulated_plate() {
    let mut meld = MeldDevice::new(44100.0);
    meld.engine_a_type = MeldMacroEngine::Swarm;
    meld.engine_b_type = MeldMacroEngine::Noise;
    meld.use_plate_a = true;
    meld.use_plate_b = true;
    meld.cross_mod_amount = 1.0; // Maximum cross modulation
    meld.plate_decay = 2.0;
    meld.plate_damping = 0.3;

    meld.trigger_note_on(57, 127);
    meld.trigger_note_on(69, 110);

    for i in 0..4096 {
        let (l, r) = meld.process_stereo_sample();
        assert_sample_bounded(l, 5.0, &format!("Meld Swarm+Plate Cross-Mod L frame {}", i));
        assert_sample_bounded(r, 5.0, &format!("Meld Swarm+Plate Cross-Mod R frame {}", i));
    }
}
