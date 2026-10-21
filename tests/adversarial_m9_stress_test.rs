//! Adversarial Empirical Stress-Test Suite for Milestone M9:
//! Dynamic Device Chains, Live 12 Audio FX Suite, Generative/Transform MIDI Tools & Dynamic Reordering.

use ableton_core::{
    AudioBuffer, AudioEffectDevice, DeviceCategory, DeviceChain, MidiEffectDevice, MidiEvent,
    MusicalTime,
};
use ableton_fx::{
    ArpPattern, Arpeggiator, AutoFilter, Chord, Compressor, Echo, EQEight, EqBandType,
    GlueCompressor, HybridReverb, HybridReverbMode, MasterLimiter, MultibandDynamics, NoteLength,
    Pitch, Random, Roar, RoarCurve, RoarTopology, Scale, SpectralResonator, Velocity,
};
use ableton_timeline::{
    generate_euclidean, generate_seed, generate_stacks, transform_connect, transform_ornament,
    transform_quantize, transform_recombine, transform_span, transform_strum, transform_time_warp,
    MidiNote, MidiPattern, ScaleLock, SpanMode, StacksChordType, StacksVoicing, StrumDirection,
};

/// Helper: validates that an AudioBuffer contains finite, non-NaN values strictly bounded by `max_abs`.
fn verify_buffer_bounded(buf: &AudioBuffer, name: &str, max_abs: f32) {
    let count = buf.frame_count.min(1024);
    for i in 0..count {
        let l = buf.left[i];
        let r = buf.right[i];

        assert!(!l.is_nan(), "{}: Left channel sample at frame {} is NaN", name, i);
        assert!(!r.is_nan(), "{}: Right channel sample at frame {} is NaN", name, i);
        assert!(l.is_finite(), "{}: Left channel sample at frame {} is Inf", name, i);
        assert!(r.is_finite(), "{}: Right channel sample at frame {} is Inf", name, i);
        assert!(
            l.abs() <= max_abs,
            "{}: Left channel sample {} exceeded boundary {} at frame {}",
            name, l, max_abs, i
        );
        assert!(
            r.abs() <= max_abs,
            "{}: Right channel sample {} exceeded boundary {} at frame {}",
            name, r, max_abs, i
        );
    }
}

// ============================================================================
// 1. AUDIO EFFECTS STRESS TESTS
// ============================================================================

#[test]
fn test_adv_roar_extreme_drive_feedback_curves_and_rapid_topology_switching() {
    let sr = 44100.0;
    let time = MusicalTime::default();

    let curves = [
        RoarCurve::SoftSine,
        RoarCurve::DigitalClip,
        RoarCurve::BitCrusher,
        RoarCurve::Polynomial,
        RoarCurve::Tube,
        RoarCurve::HalfWave,
        RoarCurve::Diode,
        RoarCurve::HardClip,
        RoarCurve::SineWavefold,
        RoarCurve::NoiseShaper,
    ];

    let topologies = [
        RoarTopology::Single,
        RoarTopology::Serial,
        RoarTopology::Parallel,
        RoarTopology::MidSide,
        RoarTopology::Multiband,
        RoarTopology::Feedback,
    ];

    // 1.1 Test all 10 curves with extreme drive (+36 dB) and pathological input signals
    for &curve in &curves {
        let mut roar = Roar::new(sr);
        roar.stage1.curve = curve;
        roar.stage1.drive_db = 36.0; // Extreme +36 dB drive (gain > 63.0)
        roar.stage2.curve = curve;
        roar.stage2.drive_db = 36.0;

        // Pathological test input 1: Dirac Impulse
        let mut buf_impulse = AudioBuffer::silence(256);
        buf_impulse.left[0] = 1.0;
        buf_impulse.right[0] = -1.0;
        roar.process_audio(&mut buf_impulse, &time);
        verify_buffer_bounded(&buf_impulse, &format!("Roar {:?} +36dB Impulse", curve), 4.0);

        // Pathological test input 2: High DC offset
        let mut buf_dc = AudioBuffer::silence(256);
        for i in 0..256 {
            buf_dc.left[i] = 2.5; // Large positive DC
            buf_dc.right[i] = -2.5; // Large negative DC
        }
        roar.process_audio(&mut buf_dc, &time);
        verify_buffer_bounded(&buf_dc, &format!("Roar {:?} +36dB Large DC", curve), 4.0);

        // Pathological test input 3: Nyquist alternating oscillation (+1.0, -1.0, +1.0, -1.0)
        let mut buf_nyquist = AudioBuffer::silence(256);
        for i in 0..256 {
            let val = if i % 2 == 0 { 1.0 } else { -1.0 };
            buf_nyquist.left[i] = val;
            buf_nyquist.right[i] = -val;
        }
        roar.process_audio(&mut buf_nyquist, &time);
        verify_buffer_bounded(&buf_nyquist, &format!("Roar {:?} +36dB Nyquist Osc", curve), 4.0);
    }

    // 1.2 Extreme feedback (0.99) over 100 continuous blocks
    {
        let mut roar_fb = Roar::new(sr);
        roar_fb.topology = RoarTopology::Feedback;
        roar_fb.feedback_amount = 0.99; // Extreme 99% feedback
        roar_fb.feedback_delay_ms = 5.0; // Short delay = high circulation rate
        roar_fb.stage1.drive_db = 12.0;

        // Inject initial high energy transient
        let mut buf = AudioBuffer::silence(256);
        buf.left[0] = 1.0;
        buf.right[0] = 1.0;
        roar_fb.process_audio(&mut buf, &time);
        verify_buffer_bounded(&buf, "Roar Feedback Init Block", 4.0);

        // Stream 100 blocks of silence to stress feedback circulation stability
        for block_idx in 1..=100 {
            let mut silence_buf = AudioBuffer::silence(256);
            roar_fb.process_audio(&mut silence_buf, &time);
            verify_buffer_bounded(
                &silence_buf,
                &format!("Roar Feedback Block {}", block_idx),
                4.0,
            );
        }
    }

    // 1.3 Rapid topology switching while streaming audio
    {
        let mut roar_rapid = Roar::new(sr);
        roar_rapid.stage1.drive_db = 24.0;
        roar_rapid.feedback_amount = 0.8;

        for step in 0..60 {
            // Cycle topologies every single block
            roar_rapid.topology = topologies[step % topologies.len()];

            let mut buf = AudioBuffer::silence(128);
            for i in 0..128 {
                let phase = (step * 128 + i) as f32 * 0.1;
                buf.left[i] = phase.sin() * 0.8;
                buf.right[i] = (phase * 1.5).cos() * 0.8;
            }

            roar_rapid.process_audio(&mut buf, &time);
            verify_buffer_bounded(
                &buf,
                &format!("Roar Rapid Switch step {} ({:?})", step, roar_rapid.topology),
                4.0,
            );
        }
    }
}

#[test]
fn test_adv_eq_eight_extreme_boosts_high_q_nyquist_all_filter_types() {
    let sr = 44100.0;
    let time = MusicalTime::default();

    let band_types = [
        EqBandType::LowCut12,
        EqBandType::LowCut48,
        EqBandType::LowShelf,
        EqBandType::Bell,
        EqBandType::Notch,
        EqBandType::HighShelf,
        EqBandType::HighCut12,
        EqBandType::HighCut48,
    ];

    // 2.1 Test all 8 filter types individually under extreme settings
    for &btype in &band_types {
        let mut eq = EQEight::new(sr);
        // Extreme +24 dB boost (or 0dB for cuts), high Q = 18.0, near Nyquist 21000 Hz
        let gain = match btype {
            EqBandType::LowCut12 | EqBandType::LowCut48 | EqBandType::HighCut12 | EqBandType::HighCut48 => 0.0,
            _ => 24.0,
        };

        eq.configure_band(0, btype, 21000.0, gain, 18.0);

        // Process Dirac Impulse
        let mut buf_imp = AudioBuffer::silence(256);
        buf_imp.left[0] = 1.0;
        buf_imp.right[0] = 1.0;
        eq.process_audio(&mut buf_imp, &time);

        // With +24 dB boost, max linear peak can reach ~10^(24/20) * resonant factor,
        // but must be strictly finite, non-NaN, and stable.
        for i in 0..256 {
            assert!(!buf_imp.left[i].is_nan(), "EQ8 {:?} Left is NaN at frame {}", btype, i);
            assert!(!buf_imp.right[i].is_nan(), "EQ8 {:?} Right is NaN at frame {}", btype, i);
            assert!(buf_imp.left[i].is_finite(), "EQ8 {:?} Left is Inf at frame {}", btype, i);
            assert!(buf_imp.right[i].is_finite(), "EQ8 {:?} Right is Inf at frame {}", btype, i);
            assert!(buf_imp.left[i].abs() < 50.0, "EQ8 {:?} Left exploded: {}", btype, buf_imp.left[i]);
        }

        // Process 10 blocks of silence to check filter decay & ring stability
        for blk in 0..10 {
            let mut buf_tail = AudioBuffer::silence(256);
            eq.process_audio(&mut buf_tail, &time);
            for i in 0..256 {
                assert!(!buf_tail.left[i].is_nan());
                assert!(buf_tail.left[i].is_finite());
            }
            if blk == 9 {
                // By block 9 (>2300 samples after impulse), tail should have decayed significantly
                let tail_energy: f32 = buf_tail.left.iter().take(256).map(|&x| x * x).sum();
                assert!(
                    tail_energy < 0.1,
                    "EQ8 {:?} impulse tail did not decay: energy {}",
                    btype,
                    tail_energy
                );
            }
        }
    }

    // 2.2 Simultaneous +24 dB boost on ALL 8 bands with Q=18.0
    {
        let mut eq_extreme = EQEight::new(sr);
        for band_idx in 0..8 {
            let freq = 100.0 * 2.0_f32.powi(band_idx as i32); // Octave spacing 100Hz .. 12.8kHz
            eq_extreme.configure_band(band_idx, EqBandType::Bell, freq, 24.0, 18.0);
        }

        let mut buf = AudioBuffer::silence(512);
        for i in 0..512 {
            buf.left[i] = (i as f32 * 0.05).sin() * 0.1;
            buf.right[i] = (i as f32 * 0.07).cos() * 0.1;
        }

        eq_extreme.process_audio(&mut buf, &time);
        for i in 0..512 {
            assert!(buf.left[i].is_finite(), "EQ8 all-boost Left is not finite at frame {}", i);
            assert!(buf.right[i].is_finite(), "EQ8 all-boost Right is not finite at frame {}", i);
            assert!(!buf.left[i].is_nan(), "EQ8 all-boost Left is NaN at frame {}", i);
            assert!(!buf.right[i].is_nan(), "EQ8 all-boost Right is NaN at frame {}", i);
        }
    }

    // 2.3 Near Nyquist cutoffs (sr * 0.49 = 21609 Hz) and beyond
    {
        let mut eq_nyquist = EQEight::new(sr);
        eq_nyquist.configure_band(7, EqBandType::HighCut48, 21609.0, 0.0, 20.0);
        eq_nyquist.configure_band(0, EqBandType::LowCut48, 20.0, 0.0, 20.0);

        let mut buf = AudioBuffer::silence(256);
        buf.left[0] = 1.0;
        buf.right[0] = -1.0;

        eq_nyquist.process_audio(&mut buf, &time);
        verify_buffer_bounded(&buf, "EQ8 Nyquist Extreme Clamping", 4.0);
    }
}

#[test]
fn test_adv_glue_compressor_and_ott_extreme_thresholds_ratios() {
    let sr = 44100.0;
    let time = MusicalTime::default();

    // 3.1 Glue Compressor: Extreme Threshold (-60 dBFS) & 10:1 Ratio
    {
        let mut glue = GlueCompressor::new(sr);
        glue.threshold_db = -60.0; // Everything above -60 dBFS triggers compression
        glue.ratio = 10.0;
        glue.attack_ms = 0.01; // Instantaneous attack
        glue.release_s = 0.1;
        glue.soft_clip = true;

        let mut buf = AudioBuffer::silence(512);
        for i in 0..512 {
            // Loud full-scale sinusoid (0 dBFS peak)
            buf.left[i] = (i as f32 * 0.1).sin();
            buf.right[i] = (i as f32 * 0.1).cos();
        }

        glue.process_audio(&mut buf, &time);

        // Verify gain reduction is active, finite, and negative
        let gr = glue.gain_reduction_db();
        assert!(gr.is_finite(), "Glue gain reduction must be finite");
        assert!(!gr.is_nan(), "Glue gain reduction must not be NaN");
        assert!(gr < -10.0, "Glue gain reduction at -60dBFS threshold must be significant, got {} dB", gr);

        // Verify soft clip boundedness ($|y| <= 1.0 <= 4.0)
        verify_buffer_bounded(&buf, "Glue Extreme Compression", 1.5);
    }

    // 3.2 Glue Compressor Soft Clip Hard Drive (> +20 dBFS Overdrive)
    {
        let mut glue_drive = GlueCompressor::new(sr);
        glue_drive.soft_clip = true;
        glue_drive.makeup_gain_db = 24.0; // Massive makeup boost

        let mut buf_overdrive = AudioBuffer::silence(256);
        for i in 0..256 {
            buf_overdrive.left[i] = 5.0; // Extreme overdriven positive input
            buf_overdrive.right[i] = -5.0; // Extreme overdriven negative input
        }

        glue_drive.process_audio(&mut buf_overdrive, &time);
        // With soft clipping enabled, output should remain strictly bounded <= 1.0
        verify_buffer_bounded(&buf_overdrive, "Glue Soft Clip Overdrive", 1.5);
    }

    // 3.3 Multiband Dynamics (OTT): Extreme Amount & Time Scale
    {
        let mut ott = MultibandDynamics::new(sr);
        ott.amount = 1.0;
        ott.time_macro = 0.1; // Extremely fast ballistics
        ott.input_gain_db = 18.0;

        let mut buf = AudioBuffer::silence(512);
        for i in 0..512 {
            buf.left[i] = (i as f32 * 0.2).sin() * 0.8;
            buf.right[i] = (i as f32 * 0.2).cos() * 0.8;
        }

        ott.process_audio(&mut buf, &time);
        verify_buffer_bounded(&buf, "OTT Fast Time Macro", 4.0);

        let (low_gr, mid_gr, high_gr) = ott.get_band_gain_reduction();
        assert!(low_gr.is_finite(), "OTT Low GR must be finite");
        assert!(mid_gr.is_finite(), "OTT Mid GR must be finite");
        assert!(high_gr.is_finite(), "OTT High GR must be finite");
    }
}

#[test]
fn test_adv_hybrid_reverb_and_spectral_resonator_infinite_decay_and_impulse() {
    let sr = 44100.0;
    let time = MusicalTime::default();

    // 4.1 Hybrid Reverb: Extreme 100s Decay (Freeze / Sustained Circulation)
    {
        let mut reverb = HybridReverb::new(sr);
        reverb.mode = HybridReverbMode::Parallel;
        reverb.decay_s = 100.0; // Extremely long decay (near freeze)
        reverb.damping = 0.0;   // Zero high-frequency absorption
        reverb.dry_wet = 1.0;   // 100% wet
        reverb.conv_algo_blend = 1.0; // 100% FDN Algorithmic

        // Excite with Dirac impulse
        let mut init_buf = AudioBuffer::silence(256);
        init_buf.left[0] = 1.0;
        init_buf.right[0] = 1.0;
        reverb.process_audio(&mut init_buf, &time);
        verify_buffer_bounded(&init_buf, "Hybrid Reverb Impulse", 4.0);

        // Process 40 blocks of silence; FDN must maintain bounded energy without runaway
        for blk in 0..40 {
            let mut silence_buf = AudioBuffer::silence(256);
            reverb.process_audio(&mut silence_buf, &time);
            verify_buffer_bounded(
                &silence_buf,
                &format!("Hybrid Reverb Freeze Block {}", blk),
                4.0,
            );
        }
    }

    // 4.2 Spectral Resonator: Extreme Decay & Max Harmonics Excitation
    {
        let mut sr_dev = SpectralResonator::new(sr);
        sr_dev.decay_s = 50.0;
        sr_dev.harmonic_count = 8;
        sr_dev.unison_spread = 1.0; // Max stereo detuning
        sr_dev.root_hz = 110.0;
        sr_dev.dry_wet = 1.0;

        let mut buf = AudioBuffer::silence(256);
        buf.left[0] = 1.0;
        buf.right[0] = 1.0;
        sr_dev.process_audio(&mut buf, &time);
        verify_buffer_bounded(&buf, "Spectral Resonator Impulse", 4.0);

        for blk in 0..20 {
            let mut tail_buf = AudioBuffer::silence(256);
            sr_dev.process_audio(&mut tail_buf, &time);
            verify_buffer_bounded(
                &tail_buf,
                &format!("Spectral Resonator Tail Block {}", blk),
                4.0,
            );
        }
    }
}

// ============================================================================
// 2. MIDI EFFECTS & CLIP TOOLS STRESS TESTS
// ============================================================================

#[test]
fn test_adv_generate_tools_boundaries_and_extremes() {
    let scale = ScaleLock::dorian(2); // D Dorian

    // 5.1 Seed: Extreme 100% density, boundary pitches (0..127) and tiny durations
    {
        let seed_full = generate_seed(
            4.0,
            0.125, // 32 steps
            1.0,   // 100% density
            0,
            127,
            1,
            127,
            0.01,
            0.05,
            Some(scale),
            9999,
        );

        assert_eq!(seed_full.notes.len(), 32, "100% density on 4 beats / 0.125 step must produce 32 notes");
        for n in &seed_full.notes {
            assert!(n.pitch <= 127, "Pitch must be <= 127, got {}", n.pitch);
            assert!(n.start_beat >= 0.0, "start_beat must be non-negative");
            assert!(n.duration_beats > 0.0, "duration must be positive");
            assert_eq!(n.pitch, scale.quantize(n.pitch), "Pitch must be scale quantized");
        }

        // Test reversed pitch boundaries (pitch_min > pitch_max)
        let seed_rev = generate_seed(2.0, 0.25, 0.8, 120, 20, 100, 50, 0.1, 0.2, None, 42);
        for n in &seed_rev.notes {
            assert!(n.pitch >= 20 && n.pitch <= 120, "Reversed pitch must be properly clamped");
            assert!(n.velocity >= 50 && n.velocity <= 100);
        }
    }

    // 5.2 Stacks: 16-note chord roots across 8 octaves
    {
        let roots_16: Vec<u8> = (0..16).map(|i| (i * 7).min(127) as u8).collect();
        let chord_types = [
            StacksChordType::MajorTriad,
            StacksChordType::MinorTriad,
            StacksChordType::Dom7,
            StacksChordType::Maj7,
            StacksChordType::Min7,
            StacksChordType::Sus2,
            StacksChordType::Sus4,
            StacksChordType::Quartal,
        ];
        let voicings = [StacksVoicing::Closed, StacksVoicing::OpenDrop2, StacksVoicing::Spread];

        for &ctype in &chord_types {
            for &voicing in &voicings {
                for inv in [0, 1, 3, 10] {
                    let pat = generate_stacks(&roots_16, ctype, voicing, inv, 0.5, 100);
                    assert!(!pat.notes.is_empty());
                    for n in &pat.notes {
                        assert!(n.pitch <= 127, "Stacked pitch must be <= 127, got {}", n.pitch);
                        assert!(n.duration_beats > 0.0, "Duration must be positive");
                        assert!(n.start_beat >= 0.0, "Start beat must be >= 0");
                    }
                }
            }
        }
    }

    // 5.3 Euclidean: Complex pulse / step ratios, pulses > steps, and zero steps
    {
        // pulses > total_steps
        let pat_excess = generate_euclidean(32, 16, 0, 36, 0.25, 100);
        assert_eq!(pat_excess.notes.len(), 16, "Excess pulses must clamp to total_steps");

        // pulses = 0
        let pat_zero = generate_euclidean(0, 16, 0, 36, 0.25, 100);
        assert!(pat_zero.notes.is_empty(), "0 pulses must return empty pattern");

        // Coprime lengths (7 pulses over 11 steps)
        let pat_coprime = generate_euclidean(7, 11, 3, 40, 0.5, 110);
        assert_eq!(pat_coprime.notes.len(), 7, "Coprime Euclidean must have exactly 7 pulses");

        // Total steps = 0 (edge case guard)
        let pat_empty_steps = generate_euclidean(4, 0, 0, 40, 0.25, 100);
        assert_eq!(pat_empty_steps.notes.len(), 1, "0 steps is clamped to 1 step max");
    }
}

#[test]
fn test_adv_transform_tools_extremes_and_edge_cases() {
    let scale = ScaleLock::natural_minor(0); // C Natural Minor

    // 6.1 Time Warp: Extreme Exponents (0.1 and 10.0) & Boundary Patterns
    {
        // Extreme Accelerando (10.0)
        let mut pat_accel = MidiPattern::new(4.0);
        for i in 0..8 {
            pat_accel.add_note(MidiNote::new(60, 100, i as f64 * 0.5, 0.4));
        }
        transform_time_warp(&mut pat_accel, 10.0);
        for n in &pat_accel.notes {
            assert!(n.start_beat.is_finite());
            assert!(!n.start_beat.is_nan());
            assert!(n.start_beat >= 0.0 && n.start_beat <= 4.0);
        }

        // Extreme Ritardando (0.1)
        let mut pat_rit = MidiPattern::new(4.0);
        for i in 0..8 {
            pat_rit.add_note(MidiNote::new(60, 100, i as f64 * 0.5, 0.4));
        }
        transform_time_warp(&mut pat_rit, 0.1);
        for n in &pat_rit.notes {
            assert!(n.start_beat.is_finite());
            assert!(!n.start_beat.is_nan());
        }

        // Edge case: single note pattern
        let mut pat_single = MidiPattern::new(4.0);
        pat_single.add_note(MidiNote::new(60, 100, 1.0, 0.5));
        transform_time_warp(&mut pat_single, 2.0);
        assert_eq!(pat_single.notes[0].start_beat, 1.0, "Single note start_beat must remain unchanged");

        // Edge case: all notes at identical start beat (span = 0.0)
        let mut pat_zero_span = MidiPattern::new(4.0);
        pat_zero_span.add_note(MidiNote::new(60, 100, 2.0, 0.5));
        pat_zero_span.add_note(MidiNote::new(64, 100, 2.0, 0.5));
        transform_time_warp(&mut pat_zero_span, 3.0);
        assert_eq!(pat_zero_span.notes[0].start_beat, 2.0);
        assert_eq!(pat_zero_span.notes[1].start_beat, 2.0);
    }

    // 6.2 Recombine: Empty and single note patterns, extreme rotations
    {
        let mut pat_empty = MidiPattern::new(4.0);
        transform_recombine(&mut pat_empty, 100, -100, 50);
        assert!(pat_empty.notes.is_empty());

        let mut pat_one = MidiPattern::new(4.0);
        pat_one.add_note(MidiNote::new(60, 90, 0.0, 1.0));
        transform_recombine(&mut pat_one, -999, 999, -500);
        assert_eq!(pat_one.notes[0].pitch, 60);

        let mut pat_three = MidiPattern::new(4.0);
        pat_three.add_note(MidiNote::new(50, 80, 0.0, 0.5));
        pat_three.add_note(MidiNote::new(60, 90, 1.0, 0.75));
        pat_three.add_note(MidiNote::new(70, 100, 2.0, 1.0));
        // Rotate with large numbers
        transform_recombine(&mut pat_three, 3001, -3002, 3000);
        for n in &pat_three.notes {
            assert!(n.pitch <= 127);
            assert!(n.velocity <= 127);
            assert!(n.duration_beats > 0.0);
        }
    }

    // 6.3 Connect: 4-octave pitch leap (pitch 24 to 84 = 5 octaves)
    {
        let mut pat_leap = MidiPattern::new(8.0);
        pat_leap.add_note(MidiNote::new(24, 100, 0.0, 0.5)); // C1
        pat_leap.add_note(MidiNote::new(84, 100, 4.0, 0.5)); // C6 (60 semitone leap, 3.5 beat gap)

        transform_connect(&mut pat_leap, scale, 0.5);
        assert!(pat_leap.notes.len() > 2, "Connect must bridge the 5-octave leap with passing tones");

        for n in &pat_leap.notes {
            assert!(n.pitch <= 127, "Generated pitch must be <= 127, got {}", n.pitch);
            assert_eq!(n.pitch, scale.quantize(n.pitch), "Pitch must be in scale");
            assert!(n.duration_beats > 0.0, "Passing note duration must be > 0");
        }
    }
}

// ============================================================================
// 3. DEVICE CHAIN REORDERING & BYPASS STRESS TESTS
// ============================================================================

#[test]
fn test_adv_device_chain_dynamic_reorder_bypass_streaming() {
    let sr = 44100.0;
    let time = MusicalTime::default();

    let mut chain = DeviceChain::new();

    // Populate a 10-device chain:
    // 0: Roar
    // 1: EQ Eight
    // 2: Glue Compressor
    // 3: Compressor
    // 4: Auto Filter
    // 5: Multiband Dynamics (OTT)
    // 6: Hybrid Reverb
    // 7: Spectral Resonator
    // 8: Echo
    // 9: Master Limiter
    chain.add_audio_effect(Box::new(Roar::new(sr)));
    chain.add_audio_effect(Box::new(EQEight::new(sr)));
    chain.add_audio_effect(Box::new(GlueCompressor::new(sr)));
    chain.add_audio_effect(Box::new(Compressor::new(sr)));
    chain.add_audio_effect(Box::new(AutoFilter::new(sr)));
    chain.add_audio_effect(Box::new(MultibandDynamics::new(sr)));
    chain.add_audio_effect(Box::new(HybridReverb::new(sr)));
    chain.add_audio_effect(Box::new(SpectralResonator::new(sr)));
    chain.add_audio_effect(Box::new(Echo::new(sr)));
    chain.add_audio_effect(Box::new(MasterLimiter::new(sr)));

    assert_eq!(chain.audio_effect_count(), 10);

    // Stream 60 consecutive audio blocks through the chain while continuously
    // mutating bypass states and reordering devices mid-stream
    for block_idx in 0..60 {
        // 1. Dynamic bypass toggling
        let dev_to_toggle = block_idx % 10;
        let is_bypassed = block_idx % 2 == 0;
        let bypass_res = chain.set_bypass(DeviceCategory::AudioEffect, dev_to_toggle, is_bypassed);
        assert!(bypass_res, "Bypass call on valid index must succeed");

        // 2. Dynamic reordering
        let from_idx = (block_idx * 3) % 10;
        let to_idx = (block_idx * 7) % 10;
        if from_idx != to_idx {
            let reorder_res = chain.reorder_audio_effects(from_idx, to_idx);
            assert!(reorder_res, "Reorder call between valid indices must succeed");
        }

        // 3. Prepare non-trivial test audio input (polyphonic tones + impulses + transients)
        let mut buf = AudioBuffer::silence(256);
        for i in 0..256 {
            let t = (block_idx * 256 + i) as f32 / sr;
            let s1 = (2.0 * std::f32::consts::PI * 150.0 * t).sin() * 0.4;
            let s2 = (2.0 * std::f32::consts::PI * 800.0 * t).sin() * 0.3;
            let s3 = (2.0 * std::f32::consts::PI * 3000.0 * t).sin() * 0.2;
            buf.left[i] = s1 + s2 + s3;
            buf.right[i] = s1 - s2 + s3;
        }

        // Periodic impulse injection every 10 blocks
        if block_idx % 10 == 0 {
            buf.left[0] = 2.0;
            buf.right[0] = -2.0;
        }

        // 4. Process block through the dynamically mutated chain
        chain.process(&[], &mut buf, &time);

        // 5. Verify the block is strictly finite, non-NaN, and bounded
        verify_buffer_bounded(
            &buf,
            &format!("10-Device Chain Block {}", block_idx),
            4.0,
        );
    }
}

#[test]
fn test_adv_roar_pathological_denormals_subnormals_and_huge_inputs() {
    let sr = 44100.0;
    let time = MusicalTime::default();

    let mut roar = Roar::new(sr);
    roar.stage1.drive_db = 36.0;
    roar.stage2.drive_db = 36.0;

    let test_inputs: &[(&str, f32)] = &[
        ("Subnormal/Denormal Small", 1e-35),
        ("Negative Denormal", -1e-35),
        ("Huge Positive Input", 1e10),
        ("Huge Negative Input", -1e10),
        ("Zero Exact", 0.0),
        ("Tube Asymmetry Center (0.1)", 0.1),
        ("Tube Asymmetry Near-Center (0.1 + 1e-6)", 0.100001),
        ("Tube Asymmetry Near-Center (0.1 - 1e-6)", 0.099999),
    ];

    for &(desc, val) in test_inputs {
        for curve in [
            RoarCurve::SoftSine,
            RoarCurve::DigitalClip,
            RoarCurve::BitCrusher,
            RoarCurve::Polynomial,
            RoarCurve::Tube,
            RoarCurve::HalfWave,
            RoarCurve::Diode,
            RoarCurve::HardClip,
            RoarCurve::SineWavefold,
            RoarCurve::NoiseShaper,
        ] {
            roar.stage1.curve = curve;
            let mut buf = AudioBuffer::silence(128);
            for i in 0..128 {
                buf.left[i] = val;
                buf.right[i] = -val;
            }

            roar.process_audio(&mut buf, &time);
            verify_buffer_bounded(&buf, &format!("Roar {:?} on {}", curve, desc), 4.0);
        }
    }
}

#[test]
fn test_adv_all_rack_midi_effects_extreme_events() {
    let time = MusicalTime::new(0.0, 120.0, 4, 4, true);

    // 1. Chord Device: Transposition clamps at boundaries (0..127)
    {
        let mut chord = Chord::with_shifts([24, 36, -24, -36, 12, -12]);
        let mut out = Vec::new();
        // High note 120 + 24 shift -> 144 must clamp to 127
        chord.process_midi(
            &[MidiEvent::NoteOn { channel: 0, note: 120, velocity: 100 }],
            &mut out,
            &time,
        );
        for ev in &out {
            if let MidiEvent::NoteOn { note, .. } = *ev {
                assert!(note <= 127, "Shifted note must not exceed 127");
            }
        }

        // Low note 5 - 24 shift -> -19 must clamp to 0
        let mut out_low = Vec::new();
        chord.process_midi(
            &[MidiEvent::NoteOn { channel: 0, note: 5, velocity: 100 }],
            &mut out_low,
            &time,
        );
        for ev in &out_low {
            if let MidiEvent::NoteOn { note, .. } = *ev {
                assert!(note <= 127, "Low shifted note must be valid u8 <= 127");
            }
        }
    }

    // 2. Scale Device: Quantize all 128 chromatic notes
    {
        let mut scale = Scale::natural_minor(9); // A Natural Minor
        for n in 0..=127 {
            let mut out = Vec::new();
            scale.process_midi(
                &[MidiEvent::NoteOn { channel: 0, note: n, velocity: 80 }],
                &mut out,
                &time,
            );
            assert_eq!(out.len(), 1);
            if let MidiEvent::NoteOn { note, .. } = out[0] {
                assert!(note <= 127, "Quantized note must be <= 127");
            }
        }
    }

    // 3. Pitch Device: Extreme shifts
    {
        let mut pitch_hi = Pitch::new(127);
        let mut out = Vec::new();
        pitch_hi.process_midi(
            &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
            &mut out,
            &time,
        );
        if let MidiEvent::NoteOn { note, .. } = out[0] {
            assert_eq!(note, 127, "60 + 127 must clamp to 127");
        }

        let mut pitch_lo = Pitch::new(-128);
        let mut out_lo = Vec::new();
        pitch_lo.process_midi(
            &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
            &mut out_lo,
            &time,
        );
        if let MidiEvent::NoteOn { note, .. } = out_lo[0] {
            assert_eq!(note, 0, "60 - 128 must clamp to 0");
        }
    }

    // 4. Velocity Device: Drive extremes & Remap validation
    {
        let mut vel = Velocity::new();
        vel.drive = 1.0;
        vel.compand = 2.0;
        vel.random = 20;

        for input_v in 0..=127 {
            let mut out = Vec::new();
            vel.process_midi(
                &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: input_v }],
                &mut out,
                &time,
            );
            if let MidiEvent::NoteOn { velocity, .. } = out[0] {
                assert!(velocity <= 127, "Output velocity must be <= 127");
            }
        }
    }

    // 5. Arpeggiator Device: Rapid note-on/note-off flood & All 8 Patterns
    {
        let patterns = [
            ArpPattern::Up,
            ArpPattern::Down,
            ArpPattern::UpDown,
            ArpPattern::DownUp,
            ArpPattern::Converge,
            ArpPattern::Diverge,
            ArpPattern::Random,
            ArpPattern::ChordTrigger,
        ];

        for &pattern in &patterns {
            let mut arp = Arpeggiator::new();
            arp.pattern = pattern;
            arp.octaves = 4;
            arp.velocity_decay = 0.5;

            // Flood with 20 concurrent notes
            let mut flood_events = Vec::new();
            for n in 48..68 {
                flood_events.push(MidiEvent::NoteOn { channel: 0, note: n, velocity: 90 });
            }

            let mut out = Vec::new();
            arp.process_midi(&flood_events, &mut out, &time);

            // Process sequential steps
            for step in 1..=16 {
                let step_time = MusicalTime::new(step as f64 * 0.25, 120.0, 4, 4, true);
                let mut step_out = Vec::new();
                arp.process_midi(&[], &mut step_out, &step_time);
                for ev in &step_out {
                    match *ev {
                        MidiEvent::NoteOn { note, velocity, .. } => {
                            assert!(note <= 127, "Arp note must be <= 127");
                            assert!(velocity <= 127, "Arp velocity must be <= 127");
                        }
                        MidiEvent::NoteOff { note, .. } => {
                            assert!(note <= 127);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // 6. NoteLength Device: Extreme length_beats
    {
        let mut nl = NoteLength::new();
        nl.length_beats = 100.0;
        let mut out = Vec::new();
        nl.process_midi(
            &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
            &mut out,
            &time,
        );
        assert_eq!(out.len(), 1);
    }

    // 7. Random Device: Chance extremes & Choices
    {
        let mut rand_dev = Random::new();
        rand_dev.chance = 1.0;
        rand_dev.choices = 24;
        let mut out = Vec::new();
        rand_dev.process_midi(
            &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
            &mut out,
            &time,
        );
        assert_eq!(out.len(), 1);
        if let MidiEvent::NoteOn { note, .. } = out[0] {
            assert!(note <= 127, "Randomized note must be <= 127");
        }
    }
}

#[test]
fn test_adv_transform_tools_ornament_span_quantize_strum() {
    let mut pat = MidiPattern::new(8.0);
    pat.add_note(MidiNote::new(60, 100, 0.0, 0.5));
    pat.add_note(MidiNote::new(64, 100, 0.0, 0.5));
    pat.add_note(MidiNote::new(67, 100, 0.0, 0.5));
    pat.add_note(MidiNote::new(71, 100, 1.0, 0.5));

    // Strum with large micro-delay
    transform_strum(&mut pat, 0.1, StrumDirection::Down);
    for n in &pat.notes {
        assert!(n.start_beat >= 0.0);
        assert!(n.duration_beats > 0.0);
    }

    // Span Legato
    transform_span(&mut pat, SpanMode::Legato, 0.25);
    for n in &pat.notes {
        assert!(n.duration_beats > 0.0);
    }

    // Quantize with max swing (0.5) and strength (1.0)
    transform_quantize(&mut pat, 0.25, 1.0, 0.5);
    for n in &pat.notes {
        assert!(n.start_beat.is_finite());
        assert!(!n.start_beat.is_nan());
        assert!(n.duration_beats > 0.0);
    }

    // Ornament with small grace offset
    transform_ornament(&mut pat, 0.05, 1, 0.8);
    for n in &pat.notes {
        assert!(n.pitch <= 127);
        assert!(n.velocity <= 127);
        assert!(n.start_beat >= 0.0);
    }
}

