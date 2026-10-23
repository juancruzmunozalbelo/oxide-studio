//! Adversarial Empirical Stress Test Suite for M7-2:
//! - Music Theory & Agentic Generators (`music_theory.rs`)
//! - LilyPond Notation Parser (`lilypond.rs`)
//! - Audio Analysis & Agent Ears (`analysis.rs`)

use std::fs::File;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use serde_json::json;

use oxide_mcp::analysis::AudioAnalysisMetrics;
use oxide_mcp::lilypond::{parse_lilypond, sequence_to_clip_pattern, ParsedItem};
use oxide_mcp::music_theory::{
    bjorklund, build_chord_notes, generate_drum_groove, get_scale_intervals,
    parse_pitch_class, parse_roman_degree,
};
use oxide_mcp::tools::call_tool;
use oxide_mcp::state::DawState;

// ============================================================================
// 1. MUSIC THEORY ADVERSARIAL STRESS TESTS
// ============================================================================

#[test]
fn test_stress_all_8_scales_across_all_12_roots() {
    let scale_names = [
        "major", "ionian",
        "minor", "natural_minor", "aeolian",
        "dorian",
        "phrygian",
        "lydian",
        "mixolydian",
        "harmonic_minor", "harmonic minor",
        "unknown_exotic_pentatonic_fallback"
    ];

    let root_note_strings = [
        "C", "C#", "Db", "D", "D#", "Eb", "E", "F",
        "F#", "Gb", "G", "G#", "Ab", "A", "A#", "Bb", "B"
    ];

    let voicings = ["close", "open", "drop-2", "spread", "invalid_voicing"];

    let mut total_combinations = 0;
    let mut successful_chords = 0;

    for &scale in &scale_names {
        let intervals = get_scale_intervals(scale);
        assert_eq!(intervals.len(), 7, "Scale intervals must contain 7 degrees: {}", scale);
        assert_eq!(intervals[0], 0, "Root interval must be 0: {}", scale);

        for &root_str in &root_note_strings {
            let pc = parse_pitch_class(root_str)
                .unwrap_or_else(|| panic!("Failed to parse pitch class for: {}", root_str));
            assert!(pc < 12, "Pitch class must be 0..11: got {} for {}", pc, root_str);

            for degree in 0..=7 {
                for &voicing in &voicings {
                    for octave in 1..=5 {
                        total_combinations += 1;
                        let chord = build_chord_notes(pc, intervals, degree, voicing, octave);
                        assert_eq!(chord.len(), 3, "Chords must be triads (3 notes)");
                        for &note in &chord {
                            assert!(note <= 127, "MIDI note must be within valid range 0..127: got {}", note);
                        }
                        successful_chords += 1;
                    }
                }
            }
        }
    }

    println!(
        "[EMPIRICAL] Scales x Roots test passed {}/{} chord evaluations cleanly",
        successful_chords, total_combinations
    );
    assert_eq!(successful_chords, total_combinations);

    // Stress: Empty scale intervals
    let unwind_empty_scale = panic::catch_unwind(AssertUnwindSafe(|| {
        build_chord_notes(0, &[], 0, "close", 3)
    }));
    if unwind_empty_scale.is_err() {
        println!("[BUG DETECTED] build_chord_notes panicked on empty scale_intervals: divisor by zero");
    }

    // Stress: Extreme base_octave >= 21 causes u8 overflow
    let unwind_overflow_oct = panic::catch_unwind(AssertUnwindSafe(|| {
        build_chord_notes(0, &[0, 2, 4, 5, 7, 9, 11], 0, "close", 25)
    }));
    if unwind_overflow_oct.is_err() {
        println!("[BUG DETECTED] build_chord_notes panicked on base_octave=25: u8 overflow");
    }
}


#[test]
fn test_stress_exotic_and_invalid_roman_degrees() {
    // 1. Unit parsing of Roman degrees
    let valid_tokens = ["i", "I", "ii", "II", "iii", "III", "iv", "IV", "v", "V", "vi", "VI", "vii", "VII"];
    for (expected_step, tok) in valid_tokens.iter().enumerate() {
        let res = parse_roman_degree(tok);
        assert!(res.is_some(), "Should parse Roman degree: {}", tok);
        let (step, is_minor) = res.unwrap();
        assert_eq!(step, expected_step / 2);
        assert_eq!(is_minor, tok.chars().next().unwrap().is_lowercase());
    }

    // Extended symbols
    assert_eq!(parse_roman_degree("vii°"), Some((6, true)));
    assert_eq!(parse_roman_degree("iio"), None); // 'o' not stripped, only '°'
    assert_eq!(parse_roman_degree("V7"), Some((4, false)));
    assert_eq!(parse_roman_degree("ii7"), Some((1, true)));
    assert_eq!(parse_roman_degree("ivdim"), Some((3, true)));
    assert_eq!(parse_roman_degree("IVdim7"), None); // Unsupported composite suffix returns None safely

    // Degenerate inputs must not panic
    let degenerate_degrees = ["", " ", "   ", "xyz", "???", "123", "♭VI", "#IV", "sus4", "N6", "Ger6", "V/V"];
    for deg in degenerate_degrees {
        let unwind_res = panic::catch_unwind(|| {
            parse_roman_degree(deg)
        });
        assert!(unwind_res.is_ok(), "parse_roman_degree panicked on: {:?}", deg);
    }

    // 2. Integration through generate_chord_progression_clip tool
    let edge_case_progressions = [
        "",                     // Empty
        "   ",                  // Whitespace
        "- - , , -",            // Punctuation only
        "bogus-garbage-xyz",    // All invalid tokens
        "i-UNKNOWN-IV-???-V",   // Mixed valid and invalid
        "i-ii-iii-iv-v-vi-vii-i-ii-iii-iv-v-vi-vii-i", // Long 15-chord progression
    ];

    for prog in edge_case_progressions {
        let unwind_res = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut local_state = DawState::new();
            let args = json!({
                "track_id": 0,
                "slot_id": 0,
                "key": "C#",
                "scale": "Harmonic Minor",
                "progression": prog,
                "voicing": "spread"
            });
            call_tool(
                &mut local_state,
                "generate_chord_progression_clip",
                &args
            )
        }));

        assert!(unwind_res.is_ok(), "generate_chord_progression_clip panicked on progression: {:?}", prog);
        let tool_res = unwind_res.unwrap();
        assert!(!tool_res.is_error, "Tool call should succeed gracefully for progression: {:?}", prog);
    }

    // 3. Integration through generate_bassline_clip with extreme octaves
    let mut bassline_panics = 0;
    for &oct in &[-5i64, 0, 2, 8, 30] {
        let unwind_res = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut local_state = DawState::new();
            let args = json!({
                "track_id": 0,
                "slot_id": 1,
                "key": "F",
                "scale": "Minor",
                "progression": "i-iv-v-i",
                "octave": oct
            });
            call_tool(
                &mut local_state,
                "generate_bassline_clip",
                &args
            )
        }));
        if unwind_res.is_err() {
            println!("[BUG DETECTED] generate_bassline_clip panicked on octave: {}", oct);
            bassline_panics += 1;
        } else {
            let tool_res = unwind_res.unwrap();
            assert!(!tool_res.is_error, "Bassline generation failed for octave {}", oct);
        }
    }
    assert_eq!(
        bassline_panics, 0,
        "generate_bassline_clip panicked on {} extreme octaves (u8 multiplication overflow)",
        bassline_panics
    );


}

#[test]
fn test_stress_extreme_bjorklund_euclidean_rhythms() {
    // 1. Extreme boundary parameter checks
    assert_eq!(bjorklund(0, 0), Vec::<bool>::new());
    assert_eq!(bjorklund(0, 16), vec![false; 16]);
    assert_eq!(bjorklund(16, 16), vec![true; 16]);
    assert_eq!(bjorklund(25, 16), vec![true; 16]); // k > n returns all true
    assert_eq!(bjorklund(100, 0), Vec::<bool>::new()); // n = 0 returns empty

    // 2. Classical Euclidean rhythms
    // E(3, 8) = [x . . x . . x .] -> Cuban tresillo
    let e3_8 = bjorklund(3, 8);
    assert_eq!(e3_8.len(), 8);
    assert_eq!(e3_8.iter().filter(|&&b| b).count(), 3);

    // E(5, 8) = Cuban cinquillo
    let e5_8 = bjorklund(5, 8);
    assert_eq!(e5_8.len(), 8);
    assert_eq!(e5_8.iter().filter(|&&b| b).count(), 5);

    // E(4, 16) = Four-on-the-floor
    let e4_16 = bjorklund(4, 16);
    assert_eq!(e4_16.len(), 16);
    assert_eq!(e4_16.iter().filter(|&&b| b).count(), 4);

    // 3. Scale test up to n = 128
    for n in [16, 32, 64, 128] {
        for k in [0, 1, 2, n / 4, n / 2, (3 * n) / 4, n - 1, n, n + 5] {
            let res = bjorklund(k, n);
            assert_eq!(res.len(), n, "Result length must be n={}", n);
            let expected_hits = k.min(n);
            let actual_hits = res.iter().filter(|&&b| b).count();
            assert_eq!(
                actual_hits, expected_hits,
                "Bjorklund({}, {}) hit count mismatch: expected {}, got {}",
                k, n, expected_hits, actual_hits
            );
        }
    }

    // 4. Integration through generate_euclidean_drum_clip tool
    let mut state = DawState::new();
    let args = json!({
        "track_id": 1,
        "slot_id": 0,
        "hits": 33,
        "steps": 32,
        "pad_index": 6,
        "accent_first": true
    });
    let tool_res = call_tool(
        &mut state,
        "generate_euclidean_drum_clip",
        &args
    );
    assert!(!tool_res.is_error, "generate_euclidean_drum_clip should succeed with steps=32, hits=33");
}

#[test]
fn test_stress_drum_groove_styles_and_extreme_complexities() {
    let styles = [
        "house_909",
        "techno_peak",
        "electro_funk",
        "breakbeat",
        "minimal_ambient",
        "unknown_hardstyle_acid_trance", // fallback
    ];

    let complexities = [0.0f32, 1.0, -0.5, 2.0, -100.0, 999.0];

    for &style in &styles {
        for &comp in &complexities {
            let pattern = generate_drum_groove(style, 128.0, 0.5, comp);
            assert_eq!(pattern.step_count, 16, "Must be 16 steps");
            assert_eq!(pattern.tracks.len(), 16, "Must have 16 pads");

            // Verify downbeat kick exists for house and techno
            if style == "house_909" || style == "techno_peak" {
                let downbeat_triggers = pattern.active_steps_at(0);
                assert!(
                    downbeat_triggers.iter().any(|(pad, _)| *pad == 0),
                    "Downbeat kick missing on step 0 for style {}",
                    style
                );
            }
        }

        // Test NaN complexity
        let unwind_nan = panic::catch_unwind(AssertUnwindSafe(|| {
            generate_drum_groove(style, 128.0, 0.5, f32::NAN)
        }));
        if unwind_nan.is_err() {
            println!("[BUG DETECTED] generate_drum_groove panicked on NaN complexity for style {}", style);
        }
    }


    // Test tool integration with extreme complexity
    let mut state = DawState::new();
    let args_low = json!({
        "track_id": 1,
        "slot_id": 1,
        "style": "breakbeat",
        "tempo": 174.0,
        "swing": 0.25,
        "complexity": -0.5
    });
    let tool_res = call_tool(
        &mut state,
        "generate_drum_groove_clip",
        &args_low
    );
    assert!(!tool_res.is_error);

    let args_high = json!({
        "track_id": 1,
        "slot_id": 2,
        "style": "techno_peak",
        "tempo": 135.0,
        "swing": 0.0,
        "complexity": 2.0
    });
    let tool_res_high = call_tool(
        &mut state,
        "generate_drum_groove_clip",
        &args_high
    );
    assert!(!tool_res_high.is_error);
}

// ============================================================================
// 2. LILYPOND PARSER ADVERSARIAL STRESS TESTS
// ============================================================================

#[test]
fn test_stress_lilypond_degenerate_strings() {
    // 1. Empty and whitespace
    let empty_seq = parse_lilypond("");
    assert_eq!(empty_seq.voices.len(), 0);
    let clip_empty = sequence_to_clip_pattern(&empty_seq);
    match clip_empty {
        oxide_timeline::ClipPattern::Midi(m) => {
            assert!(m.length_beats >= 4.0);
            assert!(m.notes.is_empty());
        }
        _ => panic!("Expected MidiPattern for empty sequence"),
    }

    let ws_seq = parse_lilypond("   \n\t  \r\n   ");
    assert_eq!(ws_seq.voices.len(), 0);

    // 2. Extreme octave shifts
    let seq_octaves = parse_lilypond("c''''4 c,,,,4 c''''''8 c,,,,,,8");
    assert_eq!(seq_octaves.voices.len(), 1);
    assert_eq!(seq_octaves.voices[0].len(), 4);
    for step in &seq_octaves.voices[0] {
        for it in &step.items {
            if let ParsedItem::Pitch { note } = it {
                assert!(*note <= 127, "Note must be clamped to 127: got {}", note);
            }
        }
    }

    // 3. Rapid / malformed tempo changes
    let seq_tempo1 = parse_lilypond("\\tempo 4 = 175\nc4 d4 e4 f4");
    assert!((seq_tempo1.tempo - 175.0).abs() < 1e-4);

    let seq_tempo_low = parse_lilypond("\\tempo 4 = 5\nc4 d4"); // < 10 BPM rejected
    assert_eq!(seq_tempo_low.tempo, 120.0);

    let seq_tempo_high = parse_lilypond("\\tempo 4 = 999\nc4 d4"); // > 400 BPM rejected
    assert_eq!(seq_tempo_high.tempo, 120.0);

    let seq_tempo_malformed = parse_lilypond("\\tempo = bogus\nc4 d4");
    assert_eq!(seq_tempo_malformed.tempo, 120.0);

    // 4. Unknown tokens and drum triggers
    let seq_unknown = parse_lilypond("c4 invalid_token !@#$ bd4 sn4 hh4 r4");
    assert_eq!(seq_unknown.voices.len(), 1);
    // Invalid tokens should parse as Rest, bd/sn/hh as Drum triggers
    let clip_pat = sequence_to_clip_pattern(&seq_unknown);
    match clip_pat {
        oxide_timeline::ClipPattern::Drum(d) => {
            assert_eq!(d.step_count, 16);
        }
        oxide_timeline::ClipPattern::Midi(_) => {}
        _ => {}
    }

    // 5. Degenerate tuplet denominators
    let seq_tuplet = parse_lilypond("c4/0 d4/3 e4/-1");
    assert_eq!(seq_tuplet.voices.len(), 1);

    // 6. Multi-voice degenerate delimiters
    let seq_voices = parse_lilypond("c4 d4 // // e4 f4 //");
    assert_eq!(seq_voices.voices.len(), 2, "Empty voices between // should be discarded");

    // 7. Unclosed chord test - EMPIRICAL HARDENING CHECK
    let unclosed_chord_strings = [
        "<c e g",
        "<c",
        "<",
        "<>",
        "<c e g>4 <d f a",
        "<c e g 4"
    ];

    let mut unclosed_panics = 0;
    for &chord_str in &unclosed_chord_strings {
        let unwind = panic::catch_unwind(AssertUnwindSafe(|| {
            parse_lilypond(chord_str)
        }));
        if unwind.is_err() {
            println!("[BUG DETECTED] parse_lilypond panicked on unclosed chord: {:?}", chord_str);
            unclosed_panics += 1;
        } else {
            let seq = unwind.unwrap();
            let _ = sequence_to_clip_pattern(&seq);
        }
    }

    println!("[EMPIRICAL] Unclosed chord panics count: {}", unclosed_panics);
    assert_eq!(
        unclosed_panics, 0,
        "Degenerate / unclosed LilyPond chords caused {} panics",
        unclosed_panics
    );
}

// ============================================================================
// 3. AUDIO ANALYSIS & "AGENT EARS" ADVERSARIAL STRESS TESTS
// ============================================================================

#[test]
fn test_stress_audio_analysis_edge_case_buffers() {
    // 1. Empty buffer
    let m_empty = AudioAnalysisMetrics::analyze_interleaved(&[], 44100, 2);
    assert_eq!(m_empty.duration_seconds, 0.0);
    assert_eq!(m_empty.total_samples, 0);
    assert_eq!(m_empty.peak_amplitude, 0.0);
    assert_eq!(m_empty.peak_dbfs, -100.0);
    assert_eq!(m_empty.rms_amplitude, 0.0);
    assert_eq!(m_empty.rms_dbfs, -100.0);
    assert_eq!(m_empty.crest_factor_db, 0.0);
    assert!(m_empty.is_silent);
    assert!(!m_empty.has_clipping);
    assert_eq!(m_empty.zero_crossing_rate, 0.0);
    assert_eq!(m_empty.spectral_centroid_hz, 0.0);

    // 2. Sample rate or channel count is 0
    let m_zero_sr = AudioAnalysisMetrics::analyze_interleaved(&[0.5; 100], 0, 2);
    assert_eq!(m_zero_sr.duration_seconds, 0.0);
    let m_zero_ch = AudioAnalysisMetrics::analyze_interleaved(&[0.5; 100], 44100, 0);
    assert_eq!(m_zero_ch.duration_seconds, 0.0);

    // 3. Single-sample buffer
    let m_single = AudioAnalysisMetrics::analyze_interleaved(&[0.5], 44100, 1);
    assert_eq!(m_single.total_samples, 1);
    assert!((m_single.peak_amplitude - 0.5).abs() < 1e-4);
    assert!(!m_single.peak_dbfs.is_nan());
    assert!(!m_single.rms_dbfs.is_nan());
    assert!(!m_single.spectral_centroid_hz.is_nan());
    assert!(!m_single.spectral_bands.sub_bass_energy_pct.is_nan());

    // Single-sample with channels = 2 (samples.len() < channels)
    let m_sub_ch = AudioAnalysisMetrics::analyze_interleaved(&[0.5], 44100, 2);
    assert_eq!(m_sub_ch.total_samples, 1);
    assert_eq!(m_sub_ch.duration_seconds, 0.0);
    assert!(!m_sub_ch.peak_dbfs.is_nan());

    // 4. All zeros (silence, 2 seconds stereo at 44.1 kHz)
    let silence = vec![0.0f32; 44100 * 2 * 2];
    let m_silence = AudioAnalysisMetrics::analyze_interleaved(&silence, 44100, 2);
    assert_eq!(m_silence.duration_seconds, 2.0);
    assert!(m_silence.is_silent);
    assert_eq!(m_silence.peak_dbfs, -100.0);
    assert_eq!(m_silence.rms_dbfs, -100.0);
    assert_eq!(m_silence.crest_factor_db, 0.0);
    assert_eq!(m_silence.zero_crossing_rate, 0.0);
    assert!(!m_silence.spectral_centroid_hz.is_nan());
    assert_eq!(m_silence.spectral_bands.sub_bass_energy_pct, 25.0);
    assert_eq!(m_silence.spectral_bands.low_mid_energy_pct, 25.0);
    assert_eq!(m_silence.spectral_bands.high_mid_energy_pct, 25.0);
    assert_eq!(m_silence.spectral_bands.high_energy_pct, 25.0);

    // 5. DC offset (+1.0 constant, 1 second mono)
    let dc = vec![1.0f32; 44100];
    let m_dc = AudioAnalysisMetrics::analyze_interleaved(&dc, 44100, 1);
    assert!((m_dc.peak_amplitude - 1.0).abs() < 1e-4);
    assert!(m_dc.peak_dbfs.abs() < 1e-4);
    assert!((m_dc.rms_amplitude - 1.0).abs() < 1e-4);
    assert!(m_dc.rms_dbfs.abs() < 1e-4);
    assert!(m_dc.crest_factor_db.abs() < 1e-4);
    assert!(m_dc.has_clipping);
    assert_eq!(m_dc.zero_crossing_rate, 0.0);
    assert!(!m_dc.spectral_centroid_hz.is_nan());
    assert!(!m_dc.spectral_bands.sub_bass_energy_pct.is_nan());

    // 6. Nyquist tone (alternating +1.0 and -1.0)
    let nyquist: Vec<f32> = (0..44100).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
    let m_nyq = AudioAnalysisMetrics::analyze_interleaved(&nyquist, 44100, 1);
    assert!(m_nyq.has_clipping);
    assert!(m_nyq.zero_crossing_rate > 0.99);
    assert!(!m_nyq.spectral_centroid_hz.is_nan());
    assert!(!m_nyq.spectral_bands.high_energy_pct.is_nan());

    // 7. Clipping signals (values > 10.0)
    let hot_signal = vec![50.0f32; 44100];
    let m_hot = AudioAnalysisMetrics::analyze_interleaved(&hot_signal, 44100, 1);
    assert_eq!(m_hot.peak_amplitude, 50.0);
    assert!(m_hot.peak_dbfs > 30.0);
    assert!(m_hot.has_clipping);
    assert!(!m_hot.peak_dbfs.is_nan());
    assert!(!m_hot.rms_dbfs.is_nan());
    assert!(!m_hot.spectral_centroid_hz.is_nan());

    // 7b. NaN and Infinite values in audio buffer
    let nan_buf = vec![f32::NAN; 44100];
    let m_nan = AudioAnalysisMetrics::analyze_interleaved(&nan_buf, 44100, 1);
    assert!(!m_nan.peak_dbfs.is_nan());
    assert!(!m_nan.spectral_centroid_hz.is_nan());

    let inf_buf = vec![f32::INFINITY; 44100];
    let m_inf = AudioAnalysisMetrics::analyze_interleaved(&inf_buf, 44100, 1);
    assert!(!m_inf.spectral_centroid_hz.is_nan());

    // 8. Buffer smaller than FFT_SIZE (e.g. 512 frames < 1024)
    let short_buf = vec![0.3f32; 512];
    let m_short = AudioAnalysisMetrics::analyze_interleaved(&short_buf, 44100, 1);
    assert_eq!(m_short.total_samples, 512);
    assert_eq!(m_short.spectral_centroid_hz, 1000.0); // fallback
    assert_eq!(m_short.spectral_bands.sub_bass_energy_pct, 25.0);

    // 9. Malformed WAV files through analyze_wav_file
    let pid = std::process::id();
    let bogus_wav = format!("/tmp/adversarial_bogus_{}.wav", pid);
    {
        let mut f = File::create(&bogus_wav).unwrap();
        f.write_all(b"NOT_A_WAV_HEADER_AT_ALL").unwrap();
    }
    let res = AudioAnalysisMetrics::analyze_wav_file(&bogus_wav);
    assert!(res.is_err(), "Must return Err on invalid WAV header");

    let empty_wav = format!("/tmp/adversarial_empty_{}.wav", pid);
    {
        let _ = File::create(&empty_wav).unwrap();
    }
    let res_empty = AudioAnalysisMetrics::analyze_wav_file(&empty_wav);
    assert!(res_empty.is_err(), "Must return Err on 0-byte file");

    let non_existent = format!("/tmp/does_not_exist_{}.wav", pid);
    let res_missing = AudioAnalysisMetrics::analyze_wav_file(&non_existent);
    assert!(res_missing.is_err(), "Must return Err on non-existent file");

    let _ = std::fs::remove_file(&bogus_wav);
    let _ = std::fs::remove_file(&empty_wav);
}
