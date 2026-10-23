//! Live 12 Global Scale System and Note Quantization.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Scale modes supported by Ableton Live 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ScaleMode {
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Aeolian,
    Locrian,
    HarmonicMinor,
    MelodicMinor,
    Pentatonic,
    Blues,
    Diminished,
    WholeTone,
}

impl ScaleMode {
    pub const ALL: [ScaleMode; 14] = [
        ScaleMode::Major,
        ScaleMode::Minor,
        ScaleMode::Dorian,
        ScaleMode::Phrygian,
        ScaleMode::Lydian,
        ScaleMode::Mixolydian,
        ScaleMode::Aeolian,
        ScaleMode::Locrian,
        ScaleMode::HarmonicMinor,
        ScaleMode::MelodicMinor,
        ScaleMode::Pentatonic,
        ScaleMode::Blues,
        ScaleMode::Diminished,
        ScaleMode::WholeTone,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            ScaleMode::Major => "Major",
            ScaleMode::Minor => "Minor",
            ScaleMode::Dorian => "Dorian",
            ScaleMode::Phrygian => "Phrygian",
            ScaleMode::Lydian => "Lydian",
            ScaleMode::Mixolydian => "Mixolydian",
            ScaleMode::Aeolian => "Aeolian",
            ScaleMode::Locrian => "Locrian",
            ScaleMode::HarmonicMinor => "Harmonic Minor",
            ScaleMode::MelodicMinor => "Melodic Minor",
            ScaleMode::Pentatonic => "Pentatonic",
            ScaleMode::Blues => "Blues",
            ScaleMode::Diminished => "Diminished",
            ScaleMode::WholeTone => "Whole Tone",
        }
    }

    pub fn intervals(&self) -> &'static [u8] {
        match self {
            ScaleMode::Major => &[0, 2, 4, 5, 7, 9, 11],
            ScaleMode::Minor | ScaleMode::Aeolian => &[0, 2, 3, 5, 7, 8, 10],
            ScaleMode::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            ScaleMode::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            ScaleMode::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            ScaleMode::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            ScaleMode::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            ScaleMode::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            ScaleMode::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            ScaleMode::Pentatonic => &[0, 2, 4, 7, 9],
            ScaleMode::Blues => &[0, 3, 5, 6, 7, 10],
            ScaleMode::Diminished => &[0, 2, 3, 5, 6, 8, 9, 11],
            ScaleMode::WholeTone => &[0, 2, 4, 6, 8, 10],
        }
    }

    pub fn degree_mask(&self) -> u16 {
        let mut mask = 0u16;
        for &interval in self.intervals() {
            mask |= 1 << (interval % 12);
        }
        mask
    }
}

pub const ROOT_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Global Scale Context in Live 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GlobalScale {
    pub root: u8,
    pub mode: ScaleMode,
    pub scale_lock: bool,
}

impl GlobalScale {
    pub fn new(root: u8, mode: ScaleMode) -> Self {
        Self {
            root: root % 12,
            mode,
            scale_lock: false,
        }
    }

    pub fn with_lock(mut self, lock: bool) -> Self {
        self.scale_lock = lock;
        self
    }

    pub fn root_name(&self) -> &'static str {
        ROOT_NAMES[(self.root % 12) as usize]
    }

    pub fn display_name(&self) -> String {
        format!("{} {}", self.root_name(), self.mode.name())
    }

    pub fn degree_mask(&self) -> u16 {
        let base_mask = self.mode.degree_mask();
        let shift = (self.root % 12) as usize;
        let mut rotated = 0u16;
        for i in 0..12 {
            if (base_mask & (1 << i)) != 0 {
                let note_pc = (i + shift) % 12;
                rotated |= 1 << note_pc;
            }
        }
        rotated
    }

    pub fn is_in_scale(&self, note: u8) -> bool {
        let pc = note % 12;
        (self.degree_mask() & (1 << pc)) != 0
    }

    pub fn quantize(&self, note: u8) -> u8 {
        if self.is_in_scale(note) {
            return note;
        }
        for d in 1..=127 {
            let lower = note as i16 - d;
            if lower >= 0 && self.is_in_scale(lower as u8) {
                return lower as u8;
            }
            let upper = note as i16 + d;
            if upper <= 127 && self.is_in_scale(upper as u8) {
                return upper as u8;
            }
        }
        note
    }

    pub fn filter_non_scale_notes(&self, notes: &[u8]) -> Vec<u8> {
        notes.iter().copied().filter(|&n| self.is_in_scale(n)).collect()
    }
}

impl Default for GlobalScale {
    fn default() -> Self {
        Self {
            root: 5, // F
            mode: ScaleMode::Dorian,
            scale_lock: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_scale_properties() {
        let scale = GlobalScale::new(0, ScaleMode::Major); // C Major
        assert_eq!(scale.root_name(), "C");
        assert_eq!(scale.display_name(), "C Major");
        assert!(scale.is_in_scale(60)); // C4
        assert!(scale.is_in_scale(62)); // D4
        assert!(scale.is_in_scale(64)); // E4
        assert!(scale.is_in_scale(65)); // F4
        assert!(scale.is_in_scale(67)); // G4
        assert!(scale.is_in_scale(69)); // A4
        assert!(scale.is_in_scale(71)); // B4
        assert!(!scale.is_in_scale(61)); // C#4
        assert!(!scale.is_in_scale(63)); // D#4

        // Quantization test
        assert_eq!(scale.quantize(61), 60); // C# -> C
        assert_eq!(scale.quantize(63), 62); // D# -> D
    }

    #[test]
    fn test_f_dorian() {
        let scale = GlobalScale::new(5, ScaleMode::Dorian); // F Dorian
        assert_eq!(scale.root_name(), "F");
        assert_eq!(scale.display_name(), "F Dorian");
        // F, G, Ab, Bb, C, D, Eb
        assert!(scale.is_in_scale(5 + 12 * 4)); // F4
        assert!(scale.is_in_scale(7 + 12 * 4)); // G4
        assert!(scale.is_in_scale(8 + 12 * 4)); // Ab4
        assert!(scale.is_in_scale(10 + 12 * 4)); // Bb4
        assert!(scale.is_in_scale(0 + 12 * 5)); // C5
        assert!(scale.is_in_scale(2 + 12 * 5)); // D5
        assert!(scale.is_in_scale(3 + 12 * 5)); // Eb5
        assert!(!scale.is_in_scale(4 + 12 * 5)); // E natural
    }

    #[test]
    fn test_filter_non_scale_notes() {
        let scale = GlobalScale::new(0, ScaleMode::Pentatonic); // C Major Pentatonic: C, D, E, G, A
        let input = [60, 61, 62, 63, 64, 65, 67, 69, 71];
        let filtered = scale.filter_non_scale_notes(&input);
        assert_eq!(filtered, vec![60, 62, 64, 67, 69]);
    }

    #[test]
    fn test_scale_lock() {
        // 1. Scale Lock flag initialization and builder
        let unlocked = GlobalScale::new(0, ScaleMode::Major);
        assert!(!unlocked.scale_lock);
        let locked = unlocked.with_lock(true);
        assert!(locked.scale_lock);
        let default_scale = GlobalScale::default();
        assert!(default_scale.scale_lock);
        assert_eq!(default_scale.root, 5);
        assert_eq!(default_scale.mode, ScaleMode::Dorian);

        // 2. Global scale awareness across all 12 roots and 14 modes
        for root in 0..12 {
            for &mode in &ScaleMode::ALL {
                let gs = GlobalScale::new(root, mode).with_lock(true);
                assert_eq!(gs.root_name(), ROOT_NAMES[root as usize]);
                assert!(!gs.display_name().is_empty());
                let mask = gs.degree_mask();
                assert_ne!(mask, 0, "Degree mask must never be empty for {} {}", gs.root_name(), mode.name());

                // Root note itself must always be in scale
                let root_midi = 60 + root;
                assert!(gs.is_in_scale(root_midi), "Root note {} must be in scale", root_midi);

                // All notes quantized must be in scale
                for test_note in [0u8, 36, 47, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 127] {
                    let quantized = gs.quantize(test_note);
                    assert!(
                        gs.is_in_scale(quantized),
                        "Quantized note {} must be in scale {} {}",
                        quantized,
                        gs.root_name(),
                        mode.name()
                    );
                }
            }
        }

        // 3. Pitch snapping under Scale Lock in C Major (C, D, E, F, G, A, B)
        let c_major_locked = GlobalScale::new(0, ScaleMode::Major).with_lock(true);
        assert_eq!(c_major_locked.quantize(60), 60); // C4 -> C4
        assert_eq!(c_major_locked.quantize(61), 60); // C#4 -> C4
        assert_eq!(c_major_locked.quantize(62), 62); // D4 -> D4
        assert_eq!(c_major_locked.quantize(63), 62); // D#4 -> D4
        assert_eq!(c_major_locked.quantize(64), 64); // E4 -> E4
        assert_eq!(c_major_locked.quantize(65), 65); // F4 -> F4
        assert_eq!(c_major_locked.quantize(66), 65); // F#4 -> F4
        assert_eq!(c_major_locked.quantize(67), 67); // G4 -> G4
        assert_eq!(c_major_locked.quantize(68), 67); // G#4 -> G4
        assert_eq!(c_major_locked.quantize(69), 69); // A4 -> A4
        assert_eq!(c_major_locked.quantize(70), 69); // A#4 -> A4
        assert_eq!(c_major_locked.quantize(71), 71); // B4 -> B4

        // 4. Pitch snapping under Scale Lock in A Minor (A, B, C, D, E, F, G)
        let a_minor_locked = GlobalScale::new(9, ScaleMode::Minor).with_lock(true);
        assert_eq!(a_minor_locked.quantize(69), 69); // A4 -> A4
        assert_eq!(a_minor_locked.quantize(70), 69); // A#4 -> A4
        assert_eq!(a_minor_locked.quantize(71), 71); // B4 -> B4
        assert_eq!(a_minor_locked.quantize(72), 72); // C5 -> C5
        assert_eq!(a_minor_locked.quantize(73), 72); // C#5 -> C5

        // 5. Note filtering under Scale Lock
        let chromatic = (60..=72).collect::<Vec<u8>>();
        let c_maj_notes = c_major_locked.filter_non_scale_notes(&chromatic);
        assert_eq!(c_maj_notes, vec![60, 62, 64, 65, 67, 69, 71, 72]);
    }
}
