//! Music theory fundamentals, scales, chord progressions, voicings, and rhythmic algorithms.

use ableton_timeline::{DrumStep, DrumStepPattern};

/// Parse note string (e.g. "C", "C#", "Db", "F#", "Bb") to pitch class 0..11.
pub fn parse_pitch_class(name: &str) -> Option<u8> {
    let s = name.trim();
    if s.is_empty() {
        return None;
    }
    let mut chars = s.chars();
    let note_letter = chars.next()?.to_ascii_uppercase();
    let base = match note_letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let modifier: i32 = match chars.next() {
        Some('#') | Some('s') | Some('S') => 1,
        Some('b') | Some('f') | Some('F') => -1,
        None => 0,
        _ => 0,
    };
    Some(((base + modifier).rem_euclid(12)) as u8)
}

/// Parse note name with optional octave, e.g. "C3" -> 48, "A4" -> 69.
pub fn parse_note_with_octave(s: &str, default_octave: i32) -> Option<u8> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split letter+accidental vs digits
    let split_pos = trimmed.find(|c: char| c.is_ascii_digit() || c == '-');
    let (note_part, oct_part) = match split_pos {
        Some(pos) => (&trimmed[..pos], &trimmed[pos..]),
        None => (trimmed, ""),
    };

    let pc = parse_pitch_class(note_part)?;
    let octave: i32 = if oct_part.is_empty() {
        default_octave
    } else {
        oct_part.parse().unwrap_or(default_octave)
    };

    let midi_note = (octave + 1) * 12 + pc as i32;
    Some(midi_note.clamp(0, 127) as u8)
}

/// Musical scale intervals from root.
pub fn get_scale_intervals(scale_name: &str) -> &'static [u8] {
    let lower = scale_name.trim().to_lowercase();
    match lower.as_str() {
        "major" | "ionian" => &[0, 2, 4, 5, 7, 9, 11],
        "minor" | "natural_minor" | "aeolian" => &[0, 2, 3, 5, 7, 8, 10],
        "dorian" => &[0, 2, 3, 5, 7, 9, 10],
        "phrygian" => &[0, 1, 3, 5, 7, 8, 10],
        "lydian" => &[0, 2, 4, 6, 7, 9, 11],
        "mixolydian" => &[0, 2, 4, 5, 7, 9, 10],
        "harmonic_minor" | "harmonic minor" => &[0, 2, 3, 5, 7, 8, 11],
        _ => &[0, 2, 3, 5, 7, 8, 10], // Default natural minor
    }
}

/// Roman numeral degrees (0-indexed scale step, e.g. "I" -> 0, "ii" -> 1).
pub fn parse_roman_degree(token: &str) -> Option<(usize, bool)> {
    let clean = token.trim().trim_end_matches("°").trim_end_matches("dim").trim_end_matches("7");
    let is_minor = clean.chars().next()?.is_lowercase();
    let lower = clean.to_lowercase();
    let step = match lower.as_str() {
        "i" => 0,
        "ii" => 1,
        "iii" => 2,
        "iv" => 3,
        "v" => 4,
        "vi" => 5,
        "vii" => 6,
        _ => return None,
    };
    Some((step, is_minor))
}

/// Build a triad or 7th chord from root degree and scale intervals.
pub fn build_chord_notes(
    root_pitch: u8,
    scale_intervals: &[u8],
    degree_step: usize,
    voicing: &str,
    base_octave: u8,
) -> Vec<u8> {
    let scale_len = scale_intervals.len();
    if scale_len == 0 {
        return Vec::new();
    }
    let root_offset = scale_intervals[degree_step % scale_len] as i32;
    let third_offset = scale_intervals[(degree_step + 2) % scale_len] as i32 + if degree_step + 2 >= scale_len { 12 } else { 0 };
    let fifth_offset = scale_intervals[(degree_step + 4) % scale_len] as i32 + if degree_step + 4 >= scale_len { 12 } else { 0 };

    let clamped_octave = base_octave.min(8) as i32;
    let base_midi = ((clamped_octave + 1) * 12 + root_pitch as i32).clamp(0, 127);

    let n1 = (base_midi + root_offset).clamp(0, 127) as u8;
    let n2 = (base_midi + third_offset).clamp(0, 127) as u8;
    let n3 = (base_midi + fifth_offset).clamp(0, 127) as u8;

    let lower_v = voicing.to_lowercase();
    match lower_v.as_str() {
        "open" => {
            // Drop root an octave, or raise 5th
            vec![n1.saturating_sub(12), n2, (n3 as u16 + 12).min(127) as u8]
        }
        "drop-2" | "drop_2" => {
            // Second highest note dropped down an octave
            vec![n2.saturating_sub(12), n1, n3]
        }
        "spread" => {
            // Root low, 5th mid, 3rd high
            vec![n1.saturating_sub(12), n3, (n2 as u16 + 12).min(127) as u8]
        }
        _ => {
            // Close voicing
            vec![n1, n2, n3]
        }
    }
}

/// Bjorklund algorithm for Euclidean rhythms.
/// Distributes `k` hits as evenly as possible across `n` steps.
pub fn bjorklund(k: usize, n: usize) -> Vec<bool> {
    if n == 0 {
        return Vec::new();
    }
    if k == 0 {
        return vec![false; n];
    }
    if k >= n {
        return vec![true; n];
    }

    let mut a: Vec<Vec<bool>> = vec![vec![true]; k];
    let mut b: Vec<Vec<bool>> = vec![vec![false]; n - k];

    while b.len() > 1 {
        let num_pairs = a.len().min(b.len());
        let mut next_a = Vec::with_capacity(num_pairs);
        for i in 0..num_pairs {
            let mut combined = a[i].clone();
            combined.extend(&b[i]);
            next_a.push(combined);
        }

        let next_b = if a.len() > b.len() {
            a[num_pairs..].to_vec()
        } else {
            b[num_pairs..].to_vec()
        };

        a = next_a;
        b = next_b;
    }

    let mut result = Vec::with_capacity(n);
    for seq in a {
        result.extend(seq);
    }
    for seq in b {
        result.extend(seq);
    }
    result.truncate(n);
    result
}

/// Generate 909 drum groove pattern based on style.
pub fn generate_drum_groove(
    style: &str,
    _tempo: f64,
    _swing: f32,
    complexity: f32,
) -> DrumStepPattern {
    let mut pattern = DrumStepPattern::new_16_step();
    let comp = complexity.clamp(0.0, 1.0);

    let s = style.to_lowercase();
    match s.as_str() {
        "techno_peak" | "techno" => {
            // Driving 4-on-the-floor kick
            for step in [0, 4, 8, 12] {
                pattern.set_step(0, step, DrumStep::new(true, 125, 36).with_accent(true));
            }
            // Claps on 4 and 12
            pattern.set_step(3, 4, DrumStep::new(true, 115, 39));
            pattern.set_step(3, 12, DrumStep::new(true, 118, 39));
            // Open HH on offbeats 2, 6, 10, 14
            for step in [2, 6, 10, 14] {
                pattern.set_step(10, step, DrumStep::new(true, 105, 46));
            }
            // 16th closed hats driving
            for step in 0..16 {
                if step % 2 != 0 {
                    let vel = if step % 4 == 3 { 90 } else { 75 };
                    pattern.set_step(6, step, DrumStep::new(true, vel, 42));
                }
            }
            // High tom or rimshot perc accents if complexity > 0.4
            if comp > 0.4 {
                pattern.set_step(1, 14, DrumStep::new(true, 95, 37));
                pattern.set_step(9, 7, DrumStep::new(true, 85, 47));
            }
        }
        "electro_funk" | "electro" => {
            // Syncopated electro kick pattern: 0, 3, 6, 10, 12
            for step in [0, 3, 6, 10, 12] {
                pattern.set_step(0, step, DrumStep::new(true, 120, 36));
            }
            // Snare on 4 and 12
            pattern.set_step(2, 4, DrumStep::new(true, 115, 38));
            pattern.set_step(2, 12, DrumStep::new(true, 115, 38));
            // Closed HH funky 16ths
            for step in [0, 2, 4, 6, 8, 10, 12, 14] {
                pattern.set_step(6, step, DrumStep::new(true, 90, 42));
            }
            if comp > 0.3 {
                pattern.set_step(6, 7, DrumStep::new(true, 80, 42));
                pattern.set_step(6, 15, DrumStep::new(true, 85, 42));
                pattern.set_step(1, 11, DrumStep::new(true, 95, 37));
            }
        }
        "breakbeat" | "breaks" => {
            // Kick on 0, 6, 10
            pattern.set_step(0, 0, DrumStep::new(true, 125, 36));
            pattern.set_step(0, 6, DrumStep::new(true, 110, 36));
            pattern.set_step(0, 10, DrumStep::new(true, 120, 36));
            // Snare on 4, 12, 15
            pattern.set_step(2, 4, DrumStep::new(true, 120, 38));
            pattern.set_step(2, 12, DrumStep::new(true, 120, 38));
            pattern.set_step(2, 15, DrumStep::new(true, 95, 38));
            // Ghost snares
            pattern.set_step(2, 7, DrumStep::new(true, 65, 38));
            pattern.set_step(2, 11, DrumStep::new(true, 70, 38));
            // Ride/Hat shuffle
            for step in 0..16 {
                if step % 2 == 0 {
                    pattern.set_step(6, step, DrumStep::new(true, 90, 42));
                }
            }
        }
        "minimal_ambient" | "minimal" => {
            // Sparse sub kick on 0 and 8
            pattern.set_step(0, 0, DrumStep::new(true, 110, 36));
            pattern.set_step(0, 8, DrumStep::new(true, 105, 36));
            // Delicate rimshot on 4, 12
            pattern.set_step(1, 4, DrumStep::new(true, 85, 37));
            pattern.set_step(1, 12, DrumStep::new(true, 90, 37));
            // Sparse closed HH on 2, 6, 10, 14
            for step in [2, 6, 10, 14] {
                pattern.set_step(6, step, DrumStep::new(true, 75, 42));
            }
            if comp > 0.5 {
                pattern.set_step(10, 15, DrumStep::new(true, 60, 46));
            }
        }
        _ => {
            // Default "house_909": classic four-on-the-floor house
            for step in [0, 4, 8, 12] {
                pattern.set_step(0, step, DrumStep::new(true, 120, 36));
            }
            // Clap & Snare on 4 and 12
            pattern.set_step(3, 4, DrumStep::new(true, 110, 39));
            pattern.set_step(3, 12, DrumStep::new(true, 110, 39));
            // Offbeat open hi-hat
            for step in [2, 6, 10, 14] {
                pattern.set_step(10, step, DrumStep::new(true, 100, 46));
            }
            // Closed hats on downbeats
            for step in [0, 4, 8, 12] {
                pattern.set_step(6, step, DrumStep::new(true, 80, 42));
            }
            if comp > 0.4 {
                pattern.set_step(1, 7, DrumStep::new(true, 85, 37));
                pattern.set_step(1, 15, DrumStep::new(true, 85, 37));
            }
        }
    }

    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bjorklund_cinquillo_and_tresillo() {
        // E(5, 8) = Cuban Cinquillo: [x . x x . x x .]
        let e5_8 = bjorklund(5, 8);
        assert_eq!(e5_8, vec![true, false, true, true, false, true, true, false]);
        assert_eq!(e5_8.iter().filter(|&&b| b).count(), 5);

        // E(3, 8) = Cuban Tresillo: [x . . x . . x .]
        let e3_8 = bjorklund(3, 8);
        assert_eq!(e3_8, vec![true, false, false, true, false, false, true, false]);
        assert_eq!(e3_8.iter().filter(|&&b| b).count(), 3);

        // Boundary cases
        assert_eq!(bjorklund(0, 0), Vec::<bool>::new());
        assert_eq!(bjorklund(0, 8), vec![false; 8]);
        assert_eq!(bjorklund(8, 8), vec![true; 8]);
        assert_eq!(bjorklund(10, 8), vec![true; 8]);
    }

    #[test]
    fn test_build_chord_notes_safety() {
        // Empty intervals returns empty vec safely without panic
        assert_eq!(build_chord_notes(0, &[], 0, "close", 3), Vec::<u8>::new());

        // Extreme octave does not overflow
        let notes_high_octave = build_chord_notes(0, &[0, 2, 4, 5, 7, 9, 11], 0, "spread", 25);
        for &note in &notes_high_octave {
            assert!(note <= 127);
        }

        // Standard C major close voicing
        let c_maj = build_chord_notes(0, &[0, 2, 4, 5, 7, 9, 11], 0, "close", 3);
        assert_eq!(c_maj, vec![48, 52, 55]); // C3, E3, G3
    }
}

