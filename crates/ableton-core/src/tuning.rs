//! Live 12 Tuning System and Microtonal Pitch Computation.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Microtonal and historical tuning systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TuningType {
    TwelveTet,
    NineteenEdo,
    TwentyFourEdo,
    ThirtyOneEdo,
    BohlenPierce,
    JustIntonation,
    Scala,
}

impl TuningType {
    pub const ALL: [TuningType; 7] = [
        TuningType::TwelveTet,
        TuningType::NineteenEdo,
        TuningType::TwentyFourEdo,
        TuningType::ThirtyOneEdo,
        TuningType::BohlenPierce,
        TuningType::JustIntonation,
        TuningType::Scala,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            TuningType::TwelveTet => "12-TET (Equal)",
            TuningType::NineteenEdo => "19-EDO",
            TuningType::TwentyFourEdo => "24-EDO (Quarter-Tone)",
            TuningType::ThirtyOneEdo => "31-EDO",
            TuningType::BohlenPierce => "Bohlen-Pierce (Tri-Scale)",
            TuningType::JustIntonation => "Just Intonation (5-Limit)",
            TuningType::Scala => "Scala Custom (.scl)",
        }
    }
}

/// Ableton Live 12 Tuning System.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TuningSystem {
    pub tuning_type: TuningType,
    pub reference_pitch: f64, // e.g. 440.0 Hz
    pub reference_note: u8,   // e.g. 69 (A4)
    pub scala_cents: Vec<f64>,
}

impl TuningSystem {
    pub fn new(tuning_type: TuningType) -> Self {
        Self {
            tuning_type,
            reference_pitch: 440.0,
            reference_note: 69,
            scala_cents: Vec::new(),
        }
    }

    pub fn frequency_for_note(&self, note: u8) -> f64 {
        match self.tuning_type {
            TuningType::TwelveTet => {
                let semitones = note as f64 - self.reference_note as f64;
                self.reference_pitch * 2.0_f64.powf(semitones / 12.0)
            }
            TuningType::NineteenEdo => {
                let steps = note as f64 - self.reference_note as f64;
                self.reference_pitch * 2.0_f64.powf(steps / 19.0)
            }
            TuningType::TwentyFourEdo => {
                let steps = note as f64 - self.reference_note as f64;
                self.reference_pitch * 2.0_f64.powf(steps / 24.0)
            }
            TuningType::ThirtyOneEdo => {
                let steps = note as f64 - self.reference_note as f64;
                self.reference_pitch * 2.0_f64.powf(steps / 31.0)
            }
            TuningType::BohlenPierce => {
                // Bohlen-Pierce equal temperament (13-EDT): 13 equal divisions of the tritave (3:1)
                let steps = note as f64 - self.reference_note as f64;
                self.reference_pitch * 3.0_f64.powf(steps / 13.0)
            }
            TuningType::JustIntonation => {
                // 5-limit Just Intonation ratios relative to C
                // 1/1, 16/15, 9/8, 6/5, 5/4, 4/3, 45/32, 3/2, 8/5, 5/3, 9/5, 15/8
                const RATIOS: [f64; 12] = [
                    1.0,
                    16.0 / 15.0,
                    9.0 / 8.0,
                    6.0 / 5.0,
                    5.0 / 4.0,
                    4.0 / 3.0,
                    45.0 / 32.0,
                    3.0 / 2.0,
                    8.0 / 5.0,
                    5.0 / 3.0,
                    9.0 / 5.0,
                    15.0 / 8.0,
                ];
                let octave = (note as i32 / 12) - (self.reference_note as i32 / 12);
                let note_pc = (note % 12) as usize;
                let ref_pc = (self.reference_note % 12) as usize;
                let ratio = RATIOS[note_pc] / RATIOS[ref_pc];
                self.reference_pitch * ratio * 2.0_f64.powi(octave)
            }
            TuningType::Scala => {
                if self.scala_cents.is_empty() {
                    let semitones = note as f64 - self.reference_note as f64;
                    self.reference_pitch * 2.0_f64.powf(semitones / 12.0)
                } else {
                    let count = self.scala_cents.len();
                    let period_cents = self.scala_cents.last().copied().unwrap_or(1200.0);
                    let diff = note as i32 - self.reference_note as i32;
                    let oct = diff.div_euclid(count as i32);
                    let rem = diff.rem_euclid(count as i32) as usize;
                    let step_cents = if rem == 0 {
                        0.0
                    } else {
                        self.scala_cents[rem - 1]
                    };
                    let cents = step_cents + (oct as f64 * period_cents);
                    self.reference_pitch * 2.0_f64.powf(cents / 1200.0)
                }
            }
        }
    }

    /// Generate a 128-element frequency lookup table for MIDI notes 0..127 in Hz (f64).
    pub fn generate_frequency_table(&self) -> [f64; 128] {
        let mut table = [0.0; 128];
        for note in 0..128 {
            table[note] = self.frequency_for_note(note as u8);
        }
        table
    }

    /// Generate a 128-element frequency lookup table for MIDI notes 0..127 in Hz (f32).
    pub fn generate_frequency_table_f32(&self) -> [f32; 128] {
        let mut table = [0.0f32; 128];
        for note in 0..128 {
            table[note] = self.frequency_for_note(note as u8) as f32;
        }
        table
    }

    pub fn cents_deviation_from_12tet(&self, note: u8) -> f64 {
        let f_actual = self.frequency_for_note(note);
        let semitones = note as f64 - self.reference_note as f64;
        let f_12tet = self.reference_pitch * 2.0_f64.powf(semitones / 12.0);
        1200.0 * (f_actual / f_12tet).log2()
    }

    /// Load Scala (.scl) format string.
    pub fn load_scala(&mut self, scl_content: &str) -> Result<(), String> {
        let mut lines = scl_content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('!'));
        // Line 1: description
        let _desc = lines.next().ok_or("Missing description in Scala file")?;
        // Line 2: number of notes
        let num_str = lines.next().ok_or("Missing note count in Scala file")?;
        let num_notes: usize = num_str.parse().map_err(|_| "Invalid note count in Scala file")?;

        let mut cents_list = Vec::with_capacity(num_notes);
        for line in lines {
            let token = line.split_whitespace().next().ok_or("Empty pitch line in Scala file")?;
            if token.contains('.') {
                let cents: f64 = token.parse().map_err(|_| "Failed to parse cents value")?;
                if cents.is_nan() || cents.is_infinite() {
                    return Err("Invalid cents value in Scala file".into());
                }
                cents_list.push(cents);
            } else if token.contains('/') {
                let parts: Vec<&str> = token.split('/').collect();
                if parts.len() != 2 {
                    return Err("Invalid ratio format in Scala file".into());
                }
                let num: f64 = parts[0].parse().map_err(|_| "Invalid ratio numerator")?;
                let den: f64 = parts[1].parse().map_err(|_| "Invalid ratio denominator")?;
                if den <= 0.0 || num <= 0.0 || num.is_nan() || den.is_nan() || num.is_infinite() || den.is_infinite() {
                    return Err("Ratio numerator and denominator must be strictly positive".into());
                }
                cents_list.push(1200.0 * (num / den).log2());
            } else {
                let integer_val: f64 = token.parse().map_err(|_| "Invalid pitch value")?;
                if integer_val <= 0.0 || integer_val.is_nan() || integer_val.is_infinite() {
                    return Err("Pitch value must be strictly positive".into());
                }
                cents_list.push(1200.0 * integer_val.log2());
            }
            if cents_list.len() == num_notes {
                break;
            }
        }

        if cents_list.len() != num_notes {
            return Err("Incomplete Scala file: note count mismatch".into());
        }

        self.scala_cents = cents_list;
        self.tuning_type = TuningType::Scala;
        Ok(())
    }
}

impl Default for TuningSystem {
    fn default() -> Self {
        Self::new(TuningType::TwelveTet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_12tet_tuning() {
        let tuning = TuningSystem::new(TuningType::TwelveTet);
        let a4 = tuning.frequency_for_note(69);
        assert!((a4 - 440.0).abs() < 1e-6);
        let a5 = tuning.frequency_for_note(81);
        assert!((a5 - 880.0).abs() < 1e-6);
        let dev = tuning.cents_deviation_from_12tet(69);
        assert!(dev.abs() < 1e-6);
    }

    #[test]
    fn test_scala_loader() {
        let scl_text = "\
! 12-TET test
12 tone equal temperament
12
!
100.0
200.0
300.0
400.0
500.0
600.0
700.0
800.0
900.0
1000.0
1100.0
1200.0
";
        let mut tuning = TuningSystem::default();
        tuning.load_scala(scl_text).expect("Scala load failed");
        assert_eq!(tuning.scala_cents.len(), 12);
        assert_eq!(tuning.tuning_type, TuningType::Scala);
    }

    #[test]
    fn test_24edo_tuning() {
        let tuning = TuningSystem::new(TuningType::TwentyFourEdo);
        let f69 = tuning.frequency_for_note(69);
        assert!((f69 - 440.0).abs() < 1e-6);
        let f70 = tuning.frequency_for_note(70);
        let expected_f70 = 440.0 * 2.0_f64.powf(1.0 / 24.0);
        assert!((f70 - expected_f70).abs() < 1e-6);
        let dev70 = tuning.cents_deviation_from_12tet(70);
        assert!((dev70 - (-50.0)).abs() < 1e-4);
    }

    #[test]
    fn test_scala_validation() {
        let mut tuning = TuningSystem::default();
        // Truncated
        assert!(tuning.load_scala("Truncated\n12\n100.0\n").is_err());
        // Zero denominator
        assert!(tuning.load_scala("Div Zero\n1\n5/0\n").is_err());
        // Negative ratio
        assert!(tuning.load_scala("Negative\n1\n-3/2\n").is_err());
    }

    #[test]
    fn test_19edo_tuning() {
        let tuning = TuningSystem::new(TuningType::NineteenEdo);
        let f69 = tuning.frequency_for_note(69);
        assert!((f69 - 440.0).abs() < 1e-6);
        let f88 = tuning.frequency_for_note(88); // 19 steps up = 1 octave
        assert!((f88 - 880.0).abs() < 1e-6);
        let f70 = tuning.frequency_for_note(70);
        let expected_f70 = 440.0 * 2.0_f64.powf(1.0 / 19.0);
        assert!((f70 - expected_f70).abs() < 1e-6);
    }

    #[test]
    fn test_31edo_tuning() {
        let tuning = TuningSystem::new(TuningType::ThirtyOneEdo);
        let f69 = tuning.frequency_for_note(69);
        assert!((f69 - 440.0).abs() < 1e-6);
        let f100 = tuning.frequency_for_note(100); // 31 steps up = 1 octave
        assert!((f100 - 880.0).abs() < 1e-6);
        let f70 = tuning.frequency_for_note(70);
        let expected_f70 = 440.0 * 2.0_f64.powf(1.0 / 31.0);
        assert!((f70 - expected_f70).abs() < 1e-6);
    }

    #[test]
    fn test_bohlen_pierce_tuning() {
        let tuning = TuningSystem::new(TuningType::BohlenPierce);
        let f69 = tuning.frequency_for_note(69);
        assert!((f69 - 440.0).abs() < 1e-6);
        let f82 = tuning.frequency_for_note(82); // 13 steps up = 1 tritave (3:1)
        assert!((f82 - 1320.0).abs() < 1e-6); // 440 * 3.0 = 1320.0
        let f70 = tuning.frequency_for_note(70);
        let expected_f70 = 440.0 * 3.0_f64.powf(1.0 / 13.0);
        assert!((f70 - expected_f70).abs() < 1e-6);
    }

    #[test]
    fn test_just_intonation_tuning() {
        let tuning = TuningSystem::new(TuningType::JustIntonation);
        let f69 = tuning.frequency_for_note(69); // A4
        assert!((f69 - 440.0).abs() < 1e-6);
        let f60 = tuning.frequency_for_note(60); // C4: 440 * (1.0 / (5/3)) = 264.0
        assert!((f60 - 264.0).abs() < 1e-6);
        let f72 = tuning.frequency_for_note(72); // C5: 264.0 * 2.0 = 528.0
        assert!((f72 - 528.0).abs() < 1e-6);
    }

    #[test]
    fn test_frequency_lookup_table() {
        let tuning = TuningSystem::new(TuningType::TwelveTet);
        let table = tuning.generate_frequency_table();
        assert_eq!(table.len(), 128);
        assert!((table[69] - 440.0).abs() < 1e-6);
        assert!((table[81] - 880.0).abs() < 1e-6);

        let table_f32 = tuning.generate_frequency_table_f32();
        assert_eq!(table_f32.len(), 128);
        assert!((table_f32[69] - 440.0).abs() < 1e-4);
    }
}
