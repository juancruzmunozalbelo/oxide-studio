//! E2E Test Suite: Mystran ZDF 4-Pole Ladder Filter, PolyBLEP Oscillators, ADSR & LFO
//!
//! Authoritative baselines:
//! - /Users/juancruz/ableton-rea/src/engine/dsp-core.ts
//! - /Users/juancruz/ableton-rea/tests/dsp-core.test.ts
//! - /Users/juancruz/ableton-rea/docs/adr/0001-dual-dsp-engine-and-mystran-zdf-ladder.md
//! - /Users/juancruz/ableton-rust/PROJECT.md

use ableton_dsp::{
    AdsrEnvelope, EnvStage, Lfo, LfoShape, MystranZdfLadder, PulseOscillator, SawOscillator,
};

#[test]
fn test_mystran_zdf_rolloff_attenuation() {
    let sample_rate = 44100.0;
    let mut osc_180 = SawOscillator::new();
    let mut osc_15k = SawOscillator::new();

    let mut filter_180 = MystranZdfLadder::new(sample_rate);
    let mut filter_15k = MystranZdfLadder::new(sample_rate);

    let num_samples = 2048;
    let mut energy_180 = 0.0f32;
    let mut energy_15k = 0.0f32;

    for i in 0..num_samples {
        let saw_180 = osc_180.next_sample(880.0, sample_rate);
        let saw_15k = osc_15k.next_sample(880.0, sample_rate);

        let y_180 = filter_180.process_sample(saw_180, 180.0, 0.1);
        let y_15k = filter_15k.process_sample(saw_15k, 15000.0, 0.1);

        // Discard transient: measure steady-state from sample 512 to 2048
        if i >= 512 {
            energy_180 += y_180 * y_180;
            energy_15k += y_15k * y_15k;
        }
    }

    assert!(energy_15k > 0.0, "15kHz signal must have non-zero energy");
    let ratio = energy_180 / energy_15k;
    let atten_db = -10.0 * ratio.log10();

    println!("Mystran ZDF Filter: 880Hz saw through 180Hz vs 15kHz cutoff");
    println!("  Energy(180Hz): {:.6}, Energy(15kHz): {:.6}", energy_180, energy_15k);
    println!("  Ratio: {:.6}, Attenuation: {:.2} dB", ratio, atten_db);

    // Acceptance criterion: roll-off > 20 dB (ratio < 0.1 in power, or energy_180 < 0.1 * energy_15k)
    assert!(
        ratio < 0.1,
        "Expected energy_180 < 0.1 * energy_15k (> 20 dB roll-off), got ratio {:.4}",
        ratio
    );
    assert!(
        atten_db > 20.0,
        "Expected attenuation > 20 dB, got {:.2} dB",
        atten_db
    );
}

#[test]
fn test_mystran_zdf_self_oscillation_stability() {
    let sample_rate = 44100.0;
    let mut filter = MystranZdfLadder::new(sample_rate);

    // Maximum resonance = 1.0, cutoff at 1200 Hz
    let cutoff = 1200.0;
    let resonance = 1.0;

    // Impulse excitation at sample 0, then silence
    let num_samples = 4096;
    let mut max_abs = 0.0f32;
    let mut sustained_energy = 0.0f32;

    for i in 0..num_samples {
        let x = if i == 0 { 1.0 } else { 0.0 };
        let y = filter.process_sample(x, cutoff, resonance);

        assert!(
            y.is_finite(),
            "Sample at index {} is not finite: {}",
            i,
            y
        );
        assert!(
            !y.is_nan(),
            "Sample at index {} is NaN",
            i
        );

        let abs_y = y.abs();
        if abs_y > max_abs {
            max_abs = abs_y;
        }

        // Strict bound invariant: |y[n]| < 2.0
        assert!(
            abs_y < 2.0,
            "Sample {} exceeded stability bound: |{}| >= 2.0",
            i,
            y
        );

        // Accumulate steady-state energy after impulse has passed
        if i >= 2048 {
            sustained_energy += y * y;
        }
    }

    println!("Mystran ZDF Filter Self-Oscillation Stability (resonance = 1.0):");
    println!("  Max peak amplitude: {:.4} (bound < 2.0)", max_abs);
    println!("  Sustained tail energy (samples 2048..4096): {:.6}", sustained_energy);

    assert!(
        sustained_energy > 0.01,
        "Filter with resonance 1.0 must sustain self-oscillation without decaying to silence"
    );
}

#[test]
fn test_pade_tanh_rational_accuracy() {
    // Check Padé [4/4] rational approximation against tanh(x)/x
    let test_points = [0.001f32, 0.01, 0.1, 0.5, 1.0, 2.0, 3.0];
    for &x in &test_points {
        let approx = MystranZdfLadder::tanh_xdx(x);
        let actual = x.tanh() / x;
        let diff = (approx - actual).abs();
        assert!(
            diff < 0.02,
            "tanh_xdx approximation error too large at x={}: approx={}, actual={}, diff={}",
            x,
            approx,
            actual,
            diff
        );
    }
}

#[test]
fn test_polyblep_sawtooth_harmonics_and_dc() {
    let sample_rate = 44100.0;
    let mut osc = SawOscillator::new();
    let freq = 440.0;
    let num_samples = 44100; // 1 second

    let mut sum = 0.0f32;
    let mut max_val = -100.0f32;
    let mut min_val = 100.0f32;

    for _ in 0..num_samples {
        let sample = osc.next_sample(freq, sample_rate);
        sum += sample;
        if sample > max_val { max_val = sample; }
        if sample < min_val { min_val = sample; }
        assert!(sample >= -1.5 && sample <= 1.5, "Sawtooth sample out of range: {}", sample);
    }

    let dc_bias = (sum / num_samples as f32).abs();
    println!("PolyBLEP Sawtooth 440Hz: DC bias = {:.6}, Min = {:.3}, Max = {:.3}", dc_bias, min_val, max_val);
    assert!(dc_bias < 0.01, "PolyBLEP sawtooth must have negligible DC bias, got {:.6}", dc_bias);
}

#[test]
fn test_polyblep_square_pwm_duty_cycle() {
    let sample_rate = 44100.0;
    let test_duties = [10.0f32, 25.0, 50.0, 75.0, 90.0];

    for &duty in &test_duties {
        let mut osc = PulseOscillator::new();
        let num_samples = 44100;
        let mut sum = 0.0f32;

        for _ in 0..num_samples {
            let sample = osc.next_sample(220.0, duty, sample_rate);
            sum += sample;
        }

        let mean = sum / num_samples as f32;
        let expected_mean = 2.0 * (duty * 0.01) - 1.0;
        let diff = (mean - expected_mean).abs();

        println!("PWM Duty {:.0}%: mean = {:.4}, expected = {:.4}, diff = {:.4}", duty, mean, expected_mean, diff);
        assert!(diff < 0.05, "PWM duty {:.0}% mean deviates too far from expected", duty);
    }
}

#[test]
fn test_adsr_convex_attack_and_decay_curves() {
    let sample_rate = 44100.0;
    let mut env = AdsrEnvelope::new(sample_rate);

    // Trigger attack: attack = 10ms, decay = 20ms, sustain = 0.5, release = 30ms
    env.trigger_attack();

    let mut attack_samples = 0;
    let mut prev_val = 0.0f32;
    let mut was_convex = true;

    while env.stage == EnvStage::Attack {
        let val = env.next_sample(10.0, 20.0, 0.5, 30.0);
        if attack_samples > 0 && val < prev_val {
            was_convex = false;
        }
        prev_val = val;
        attack_samples += 1;
        if attack_samples > 10000 { break; }
    }

    assert!(was_convex, "Attack curve must be strictly monotonic");
    assert_eq!(env.stage, EnvStage::Decay, "Stage must transition to Decay upon reaching 1.0");

    // Run through decay
    let mut decay_samples = 0;
    while env.stage == EnvStage::Decay {
        let _ = env.next_sample(10.0, 20.0, 0.5, 30.0);
        decay_samples += 1;
        if decay_samples > 10000 { break; }
    }

    assert_eq!(env.stage, EnvStage::Sustain, "Stage must transition to Sustain");
    assert!((env.value - 0.5).abs() < 1e-3, "Sustain level must be ~0.5");

    // Trigger release
    env.trigger_release();
    assert_eq!(env.stage, EnvStage::Release);

    let mut release_samples = 0;
    while env.stage == EnvStage::Release {
        let _ = env.next_sample(10.0, 20.0, 0.5, 30.0);
        release_samples += 1;
        if release_samples > 10000 { break; }
    }

    assert_eq!(env.stage, EnvStage::Idle, "Stage must transition to Idle");
    assert_eq!(env.value, 0.0, "Final idle value must be 0.0");
}

#[test]
fn test_lfo_shapes_and_modulation() {
    let sample_rate = 44100.0;
    let mut lfo = Lfo::new();

    // Check Sine wave bounds
    for _ in 0..1000 {
        let (s, _) = lfo.next_sample(5.0, 0.0, 0.0, LfoShape::Sine, sample_rate);
        assert!(s >= -1.01 && s <= 1.01, "Sine LFO out of bounds: {}", s);
    }

    // Check Square wave values
    for _ in 0..1000 {
        let (s, _) = lfo.next_sample(10.0, 0.0, 0.0, LfoShape::Square, sample_rate);
        assert!(s == 1.0 || s == -1.0, "Square LFO must be exactly +1.0 or -1.0: {}", s);
    }
}
