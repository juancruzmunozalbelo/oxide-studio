//! Live 12 Clip Generate Tools (Seed, Shape, Stacks, Rhythm, Euclidean).

use crate::pattern::{MidiNote, MidiPattern};

/// Scale intervals helper for note snapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaleLock {
    pub root: u8,          // 0 = C, 1 = C#, ..., 11 = B
    pub degree_mask: u16,  // Bitmask of 12 semitones
}

impl ScaleLock {
    pub fn major(root: u8) -> Self {
        // 0, 2, 4, 5, 7, 9, 11
        let mask = 1 | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);
        Self { root, degree_mask: mask }
    }

    pub fn natural_minor(root: u8) -> Self {
        // 0, 2, 3, 5, 7, 8, 10
        let mask = 1 | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 8) | (1 << 10);
        Self { root, degree_mask: mask }
    }

    pub fn dorian(root: u8) -> Self {
        // 0, 2, 3, 5, 7, 9, 10
        let mask = 1 | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 10);
        Self { root, degree_mask: mask }
    }

    pub fn chromatic() -> Self {
        Self { root: 0, degree_mask: 0x0FFF }
    }

    pub fn quantize(&self, note: u8) -> u8 {
        let octave = note / 12;
        let pc = (note % 12 + 12 - self.root % 12) % 12;
        if (self.degree_mask & (1 << pc)) != 0 {
            return note;
        }
        for d in 1..=6 {
            let lower = (pc + 12 - d) % 12;
            if (self.degree_mask & (1 << lower)) != 0 {
                return ((octave as i16 * 12 + (lower + self.root) as i16).clamp(0, 127)) as u8;
            }
            let upper = (pc + d) % 12;
            if (self.degree_mask & (1 << upper)) != 0 {
                return ((octave as i16 * 12 + (upper + self.root) as i16).clamp(0, 127)) as u8;
            }
        }
        note
    }
}

// Simple LCG PRNG for deterministic generative reproducibility
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_add(0x9E3779B97F4A7C15) }
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.state >> 32) as u32
    }

    pub(crate) fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16777216.0
    }
}

/// 1. Seed: Probabilistic Pitch/Time/Velocity Cloud Generator.
#[allow(clippy::too_many_arguments)]
pub fn generate_seed(
    length_beats: f64,
    step_size_beats: f64,
    density: f32,
    pitch_min: u8,
    pitch_max: u8,
    velocity_min: u8,
    velocity_max: u8,
    duration_min: f64,
    duration_max: f64,
    scale: Option<ScaleLock>,
    seed: u64,
) -> MidiPattern {
    let mut pat = MidiPattern::new(length_beats);
    let mut rng = Rng::new(seed);
    let step = step_size_beats.max(0.125);
    let num_steps = (length_beats / step).floor() as usize;

    let p_min = pitch_min.min(pitch_max);
    let p_max = pitch_min.max(pitch_max);
    let v_min = velocity_min.min(velocity_max);
    let v_max = velocity_min.max(velocity_max);

    for s in 0..num_steps {
        if rng.next_f32() < density {
            let start = s as f64 * step;
            let raw_p = p_min + (rng.next_f32() * (p_max - p_min + 1) as f32) as u8;
            let pitch = if let Some(ref sc) = scale {
                sc.quantize(raw_p)
            } else {
                raw_p
            };
            let vel = v_min + (rng.next_f32() * (v_max - v_min + 1) as f32) as u8;
            let dur = duration_min + rng.next_f32() as f64 * (duration_max - duration_min).max(0.0625);
            pat.add_note(MidiNote::new(pitch, vel, start, dur));
        }
    }
    pat
}

/// Contour shapes for the Shape generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContourShape {
    LinearRise,
    LinearFall,
    BellArch,
    ValleyCup,
    SCurve,
    TriangleWave,
}

/// 2. Shape: Contour-Driven Melodic Phrase Generator.
pub fn generate_shape(
    length_beats: f64,
    step_duration_beats: f64,
    contour: ContourShape,
    pitch_min: u8,
    pitch_max: u8,
    velocity: u8,
    scale: Option<ScaleLock>,
) -> MidiPattern {
    let mut pat = MidiPattern::new(length_beats);
    let step = step_duration_beats.max(0.125);
    let steps = (length_beats / step).floor() as usize;
    if steps == 0 {
        return pat;
    }

    let p_min = pitch_min.min(pitch_max) as f32;
    let p_max = pitch_min.max(pitch_max) as f32;

    for s in 0..steps {
        let t = s as f32 / (steps - 1).max(1) as f32; // 0.0 .. 1.0
        let norm_val = match contour {
            ContourShape::LinearRise => t,
            ContourShape::LinearFall => 1.0 - t,
            ContourShape::BellArch => (t * std::f32::consts::PI).sin(),
            ContourShape::ValleyCup => 1.0 - (t * std::f32::consts::PI).sin(),
            ContourShape::SCurve => (1.0 - (t * std::f32::consts::PI).cos()) * 0.5,
            ContourShape::TriangleWave => {
                if t < 0.5 { 2.0 * t } else { 2.0 * (1.0 - t) }
            }
        };

        let raw_p = (p_min + norm_val * (p_max - p_min)).round() as u8;
        let pitch = if let Some(ref sc) = scale {
            sc.quantize(raw_p)
        } else {
            raw_p
        };

        let start = s as f64 * step;
        pat.add_note(MidiNote::new(pitch, velocity, start, step * 0.9));
    }
    pat
}

/// Chord types for Stacks generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StacksChordType {
    MajorTriad,
    MinorTriad,
    Dom7,
    Maj7,
    Min7,
    Sus2,
    Sus4,
    Quartal,
}

/// Voicing styles for Stacks generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StacksVoicing {
    Closed,
    OpenDrop2,
    Spread,
}

/// 3. Stacks: Multi-Chord Voicing & Inversion Stacker.
pub fn generate_stacks(
    roots: &[u8],
    chord_type: StacksChordType,
    voicing: StacksVoicing,
    inversion: usize,
    step_duration_beats: f64,
    velocity: u8,
) -> MidiPattern {
    let length = roots.len() as f64 * step_duration_beats;
    let mut pat = MidiPattern::new(length);

    let base_intervals: &[i8] = match chord_type {
        StacksChordType::MajorTriad => &[0, 4, 7],
        StacksChordType::MinorTriad => &[0, 3, 7],
        StacksChordType::Dom7 => &[0, 4, 7, 10],
        StacksChordType::Maj7 => &[0, 4, 7, 11],
        StacksChordType::Min7 => &[0, 3, 7, 10],
        StacksChordType::Sus2 => &[0, 2, 7],
        StacksChordType::Sus4 => &[0, 5, 7],
        StacksChordType::Quartal => &[0, 5, 10],
    };

    for (idx, &root) in roots.iter().enumerate() {
        let start = idx as f64 * step_duration_beats;
        let mut notes: Vec<u8> = base_intervals
            .iter()
            .map(|&i| (root as i16 + i as i16).clamp(0, 127) as u8)
            .collect();

        // Inversions
        let inv = inversion % notes.len().max(1);
        for i in 0..inv {
            if i < notes.len() {
                notes[i] = (notes[i] as u16 + 12).min(127) as u8;
            }
        }
        notes.sort();

        // Voicing adjustments
        match voicing {
            StacksVoicing::Closed => {}
            StacksVoicing::OpenDrop2 => {
                if notes.len() >= 3 {
                    // Drop 2nd highest note down an octave
                    let drop_idx = notes.len() - 2;
                    notes[drop_idx] = notes[drop_idx].saturating_sub(12);
                    notes.sort();
                }
            }
            StacksVoicing::Spread => {
                // Transpose alternating notes up an octave
                for i in (1..notes.len()).step_by(2) {
                    notes[i] = (notes[i] as u16 + 12).min(127) as u8;
                }
                notes.sort();
            }
        }

        for &p in &notes {
            pat.add_note(MidiNote::new(p, velocity, start, step_duration_beats * 0.95));
        }
    }

    pat
}

/// Rhythm pattern template styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RhythmStyle {
    FourOnTheFloor,
    BoomBap,
    TrapHiHats,
    Clave32,
    Afrobeat,
}

/// 4. Rhythm: Step Density & Syncopation Pattern Generator.
pub fn generate_rhythm(
    length_beats: f64,
    style: RhythmStyle,
    pitch: u8,
    velocity: u8,
    accent_velocity: u8,
) -> MidiPattern {
    let mut pat = MidiPattern::new(length_beats);
    let step_16th = 0.25f64;
    let total_steps = (length_beats / step_16th).floor() as usize;

    let pattern_steps: &[u8] = match style {
        RhythmStyle::FourOnTheFloor => &[1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
        RhythmStyle::BoomBap => &[1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0],
        RhythmStyle::TrapHiHats => &[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        RhythmStyle::Clave32 => &[1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0],
        RhythmStyle::Afrobeat => &[1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0],
    };

    for s in 0..total_steps {
        let pat_idx = s % pattern_steps.len();
        if pattern_steps[pat_idx] == 1 {
            let start = s as f64 * step_16th;
            let is_accent = s % 4 == 0;
            let v = if is_accent { accent_velocity } else { velocity };
            pat.add_note(MidiNote::new(pitch, v, start, step_16th * 0.8));
        }
    }

    pat
}

/// 5. Euclidean: Bjorklund Symmetrical Pulse Distribution Algorithm.
pub fn generate_euclidean(
    pulses: usize,
    total_steps: usize,
    rotation: usize,
    pitch: u8,
    step_duration_beats: f64,
    velocity: u8,
) -> MidiPattern {
    let steps = total_steps.max(1);
    let k = pulses.min(steps);
    let length = steps as f64 * step_duration_beats;
    let mut pat = MidiPattern::new(length);

    if k == 0 {
        return pat;
    }

    // Bjorklund formula: pulse at step i if floor(i * k / n) != floor((i - 1) * k / n)
    for i in 0..steps {
        let step_rot = (i + rotation) % steps;
        let p1 = (step_rot * k) / steps;
        let p0 = if step_rot > 0 {
            ((step_rot - 1) * k) / steps
        } else {
            ((steps - 1) * k) / steps
        };

        let is_hit = if step_rot == 0 {
            // First step is always hit if k > 0
            true
        } else {
            p1 != p0
        };

        if is_hit {
            let start = i as f64 * step_duration_beats;
            pat.add_note(MidiNote::new(pitch, velocity, start, step_duration_beats * 0.8));
        }
    }

    pat
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_euclidean_bjorklund_tresillo() {
        // 3 pulses over 8 steps = Tresillo (1, 0, 0, 1, 0, 0, 1, 0)
        let pat = generate_euclidean(3, 8, 0, 36, 0.25, 100);
        assert_eq!(pat.notes.len(), 3);
        assert_eq!(pat.notes[0].start_beat, 0.0);
    }

    #[test]
    fn test_seed_scale_lock() {
        let scale = ScaleLock::natural_minor(0); // C Minor
        let pat = generate_seed(4.0, 0.25, 1.0, 48, 72, 80, 100, 0.2, 0.2, Some(scale), 42);
        assert!(!pat.notes.is_empty());
        for n in &pat.notes {
            assert_eq!(n.pitch, scale.quantize(n.pitch));
        }
    }

    #[test]
    fn test_shape_linear_rise() {
        let pat = generate_shape(4.0, 0.25, ContourShape::LinearRise, 40, 80, 100, None);
        assert_eq!(pat.notes.len(), 16);
        assert!(pat.notes.first().unwrap().pitch <= pat.notes.last().unwrap().pitch);
    }

    #[test]
    fn test_stacks_chord_voicing() {
        let roots = [60, 62, 64]; // C4, D4, E4
        let pat = generate_stacks(&roots, StacksChordType::Maj7, StacksVoicing::Closed, 0, 1.0, 100);
        assert_eq!(pat.notes.len(), 12); // 3 chords * 4 notes
    }
}
