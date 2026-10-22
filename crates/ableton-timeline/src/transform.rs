//! Live 12 Clip Transform Tools.
//!
//! Tools:
//! - Arpeggiate
//! - Connect (scale-locked passing tones)
//! - Ornament (flams and grace notes)
//! - Quantize (grid snap with strength & swing)
//! - Recombine (parameter permutation and rotation)
//! - Span (legato, tenuto, staccato length sculpting)
//! - Strum (micro-delay chord strumming)
//! - Time Warp (non-linear accelerando/ritardando timing warp)
//! - Invert (mirror pitch direction vertically)
//! - Reverse (mirror note timings horizontally)
//! - Legato (extend durations to next note onset)
//! - Humanize (organic micro-timing jitter and velocity variance)

use crate::generate::{Rng, ScaleLock};
use crate::pattern::{MidiNote, MidiPattern};

/// 1. Arpeggiate: Spreads chord clusters across sequential grid steps.
pub fn transform_arpeggiate(
    pattern: &mut MidiPattern,
    step_duration_beats: f64,
    downwards: bool,
) {
    if pattern.notes.is_empty() {
        return;
    }

    // Group notes by approximate start_beat
    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());

    let mut new_notes = Vec::new();
    let mut i = 0;
    while i < pattern.notes.len() {
        let base_start = pattern.notes[i].start_beat;
        let mut chord_group = vec![pattern.notes[i]];
        let mut j = i + 1;
        while j < pattern.notes.len() && (pattern.notes[j].start_beat - base_start).abs() < 0.05 {
            chord_group.push(pattern.notes[j]);
            j += 1;
        }

        // Sort chord group by pitch
        chord_group.sort_by_key(|n| n.pitch);
        if downwards {
            chord_group.reverse();
        }

        for (step_idx, mut note) in chord_group.into_iter().enumerate() {
            note.start_beat = base_start + step_idx as f64 * step_duration_beats;
            note.duration_beats = step_duration_beats * 0.9;
            new_notes.push(note);
        }

        i = j;
    }

    pattern.notes = new_notes;
}

/// 2. Connect: Fills gaps between sequential notes with scale-locked passing tones.
pub fn transform_connect(
    pattern: &mut MidiPattern,
    scale: ScaleLock,
    max_step_beats: f64,
) {
    if pattern.notes.len() < 2 {
        return;
    }

    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
    let mut passing_notes = Vec::new();

    for i in 0..pattern.notes.len() - 1 {
        let n1 = &pattern.notes[i];
        let n2 = &pattern.notes[i + 1];

        let time_gap = n2.start_beat - n1.end_beat();
        let pitch_diff = n2.pitch as i16 - n1.pitch as i16;

        if time_gap >= max_step_beats && pitch_diff.abs() >= 2 {
            let num_passing = ((time_gap / max_step_beats).floor() as usize).min(3);
            let time_step = time_gap / (num_passing + 1) as f64;
            let pitch_step = pitch_diff as f32 / (num_passing + 1) as f32;

            for p in 1..=num_passing {
                let raw_pitch = (n1.pitch as f32 + pitch_step * p as f32).round() as u8;
                let pitch = scale.quantize(raw_pitch);
                let start = n1.end_beat() + (p - 1) as f64 * time_step;
                let vel = ((n1.velocity as u16 + n2.velocity as u16) / 2) as u8;
                passing_notes.push(MidiNote::new(pitch, vel, start, time_step * 0.9));
            }
        }
    }

    pattern.notes.extend(passing_notes);
    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
}

/// 3. Ornament: Adds flams or grace notes preceding note onsets.
pub fn transform_ornament(
    pattern: &mut MidiPattern,
    grace_offset_beats: f64,
    semitone_offset: i8,
    velocity_scale: f32,
) {
    let mut ornaments = Vec::new();
    let offset = grace_offset_beats.max(0.02);

    for note in &pattern.notes {
        if note.start_beat >= offset {
            let orn_pitch = (note.pitch as i16 + semitone_offset as i16).clamp(0, 127) as u8;
            let orn_vel = ((note.velocity as f32 * velocity_scale).clamp(1.0, 127.0)) as u8;
            ornaments.push(MidiNote::new(
                orn_pitch,
                orn_vel,
                note.start_beat - offset,
                offset * 0.9,
            ));
        }
    }

    pattern.notes.extend(ornaments);
    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
}

/// 4. Quantize: Snaps note start_beat and duration to a metric grid with strength & swing.
pub fn transform_quantize(
    pattern: &mut MidiPattern,
    grid_beats: f64,
    strength: f32,
    swing: f32,
) {
    let grid = grid_beats.max(0.0625);
    let str_factor = strength.clamp(0.0, 1.0) as f64;
    let swing_offset = (grid * swing.clamp(0.0, 0.5) as f64) * 0.5;

    for note in &mut pattern.notes {
        let grid_index = (note.start_beat / grid).round();
        let mut target_start = grid_index * grid;

        // Apply 16th-note swing on off-beats
        if (grid_index as i64) % 2 != 0 {
            target_start += swing_offset;
        }

        note.start_beat = note.start_beat * (1.0 - str_factor) + target_start * str_factor;

        let dur_index = (note.duration_beats / grid).round().max(1.0);
        let target_dur = dur_index * grid;
        note.duration_beats = note.duration_beats * (1.0 - str_factor) + target_dur * str_factor;
    }
}

/// 5. Recombine: Permutes or rotates note parameters independently.
pub fn transform_recombine(
    pattern: &mut MidiPattern,
    pitch_rot: isize,
    velocity_rot: isize,
    duration_rot: isize,
) {
    let len = pattern.notes.len();
    if len <= 1 {
        return;
    }

    let pitches: Vec<u8> = pattern.notes.iter().map(|n| n.pitch).collect();
    let velocities: Vec<u8> = pattern.notes.iter().map(|n| n.velocity).collect();
    let durations: Vec<f64> = pattern.notes.iter().map(|n| n.duration_beats).collect();

    for (i, note) in pattern.notes.iter_mut().enumerate() {
        let p_idx = (i as isize - pitch_rot).rem_euclid(len as isize) as usize;
        let v_idx = (i as isize - velocity_rot).rem_euclid(len as isize) as usize;
        let d_idx = (i as isize - duration_rot).rem_euclid(len as isize) as usize;

        note.pitch = pitches[p_idx];
        note.velocity = velocities[v_idx];
        note.duration_beats = durations[d_idx];
    }
}

/// Span modes for length sculpting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanMode {
    Legato,
    Tenuto,
    Staccato,
}

/// 6. Span: Sculpts note lengths (Legato, Tenuto, Staccato).
pub fn transform_span(pattern: &mut MidiPattern, mode: SpanMode, grid_beats: f64) {
    if pattern.notes.is_empty() {
        return;
    }

    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
    let len = pattern.notes.len();

    match mode {
        SpanMode::Legato => {
            for i in 0..len - 1 {
                let next_start = pattern.notes[i + 1].start_beat;
                if next_start > pattern.notes[i].start_beat {
                    pattern.notes[i].duration_beats = next_start - pattern.notes[i].start_beat;
                }
            }
        }
        SpanMode::Tenuto => {
            for note in &mut pattern.notes {
                note.duration_beats = grid_beats.max(0.125);
            }
        }
        SpanMode::Staccato => {
            for note in &mut pattern.notes {
                note.duration_beats = (note.duration_beats * 0.4).max(0.0625);
            }
        }
    }
}

/// Strum direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrumDirection {
    Up,
    Down,
}

/// 7. Strum: Introduces micro-delays between notes sharing a start beat.
pub fn transform_strum(
    pattern: &mut MidiPattern,
    delay_per_note_beats: f64,
    direction: StrumDirection,
) {
    if pattern.notes.is_empty() {
        return;
    }

    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());

    let mut i = 0;
    while i < pattern.notes.len() {
        let base_start = pattern.notes[i].start_beat;
        let mut chord_indices = vec![i];
        let mut j = i + 1;
        while j < pattern.notes.len() && (pattern.notes[j].start_beat - base_start).abs() < 0.05 {
            chord_indices.push(j);
            j += 1;
        }

        if chord_indices.len() > 1 {
            // Sort by pitch
            chord_indices.sort_by_key(|&idx| pattern.notes[idx].pitch);
            if direction == StrumDirection::Down {
                chord_indices.reverse();
            }

            for (step, &idx) in chord_indices.iter().enumerate() {
                pattern.notes[idx].start_beat += step as f64 * delay_per_note_beats;
            }
        }

        i = j;
    }
}

/// 8. Time Warp: Non-linear accelerando / ritardando curve across note sequence.
pub fn transform_time_warp(pattern: &mut MidiPattern, exponent: f64) {
    if pattern.notes.len() < 2 {
        return;
    }

    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
    let t_start = pattern.notes.first().unwrap().start_beat;
    let t_end = pattern.notes.last().unwrap().start_beat;
    let span = t_end - t_start;

    if span <= 0.0 {
        return;
    }

    let gamma = exponent.clamp(0.1, 10.0);

    for note in &mut pattern.notes {
        let norm = (note.start_beat - t_start) / span;
        let warped_norm = norm.powf(gamma);
        note.start_beat = t_start + warped_norm * span;
    }
}

/// 9. Invert: Reflects note pitches vertically across a center pitch axis.
/// If `pivot_pitch` is None, the midpoint of the minimum and maximum note pitch in the pattern is used.
pub fn transform_invert(pattern: &mut MidiPattern, pivot_pitch: Option<u8>) {
    if pattern.notes.is_empty() {
        return;
    }
    let pivot = match pivot_pitch {
        Some(p) => p as i16,
        None => {
            let min_p = pattern.notes.iter().map(|n| n.pitch).min().unwrap() as i16;
            let max_p = pattern.notes.iter().map(|n| n.pitch).max().unwrap() as i16;
            (min_p + max_p) / 2
        }
    };
    for note in &mut pattern.notes {
        let inverted = 2 * pivot - note.pitch as i16;
        note.pitch = inverted.clamp(0, 127) as u8;
    }
}

/// 10. Reverse: Horizontally mirrors note timings across the pattern timeline.
pub fn transform_reverse(pattern: &mut MidiPattern) {
    if pattern.notes.is_empty() {
        return;
    }
    let total_len = if pattern.length_beats > 0.0 {
        pattern.length_beats
    } else {
        pattern.notes.iter().map(|n| n.end_beat()).fold(0.0, f64::max)
    };

    for note in &mut pattern.notes {
        let new_start = (total_len - (note.start_beat + note.duration_beats)).max(0.0);
        note.start_beat = new_start;
    }
    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
}

/// 11. Legato: Extends note durations so that each note rings until the next note onset (or pattern end).
pub fn transform_legato(pattern: &mut MidiPattern) {
    if pattern.notes.is_empty() {
        return;
    }
    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
    let len = pattern.notes.len();
    for i in 0..len - 1 {
        let next_start = pattern.notes[i + 1].start_beat;
        if next_start > pattern.notes[i].start_beat {
            pattern.notes[i].duration_beats = next_start - pattern.notes[i].start_beat;
        }
    }
    if let Some(last) = pattern.notes.last_mut() {
        if pattern.length_beats > last.start_beat {
            last.duration_beats = pattern.length_beats - last.start_beat;
        }
    }
}

/// 12. Humanize: Injects subtle organic micro-timing jitter (default ±15 ticks / ~0.0156 beats)
/// and velocity randomization (default ±8 velocity units) to emulate human performance.
pub fn transform_humanize(
    pattern: &mut MidiPattern,
    timing_jitter_beats: f64,
    velocity_jitter: i16,
    seed: u64,
) {
    if pattern.notes.is_empty() {
        return;
    }
    let mut rng = Rng::new(seed);
    for note in &mut pattern.notes {
        let r_time = (rng.next_f32() * 2.0 - 1.0) as f64 * timing_jitter_beats;
        note.start_beat = (note.start_beat + r_time).max(0.0);

        let r_vel = ((rng.next_f32() * 2.0 - 1.0) * velocity_jitter as f32).round() as i16;
        let new_vel = (note.velocity as i16 + r_vel).clamp(1, 127) as u8;
        note.velocity = new_vel;
    }
    pattern.notes.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
}

/// Unified Live 12 MIDI Transform & Generative Engine facade.
#[derive(Debug, Clone, Copy, Default)]
pub struct MidiTransformEngine;

impl MidiTransformEngine {
    pub fn arpeggiate(pattern: &mut MidiPattern, step_duration_beats: f64, downwards: bool) {
        transform_arpeggiate(pattern, step_duration_beats, downwards);
    }

    pub fn invert(pattern: &mut MidiPattern, pivot_pitch: Option<u8>) {
        transform_invert(pattern, pivot_pitch);
    }

    pub fn reverse(pattern: &mut MidiPattern) {
        transform_reverse(pattern);
    }

    pub fn legato(pattern: &mut MidiPattern) {
        transform_legato(pattern);
    }

    pub fn humanize(
        pattern: &mut MidiPattern,
        timing_jitter_beats: f64,
        velocity_jitter: i16,
        seed: u64,
    ) {
        transform_humanize(pattern, timing_jitter_beats, velocity_jitter, seed);
    }

    pub fn quantize(pattern: &mut MidiPattern, grid_beats: f64, strength: f32, swing: f32) {
        transform_quantize(pattern, grid_beats, strength, swing);
    }

    pub fn connect(pattern: &mut MidiPattern, scale: ScaleLock, max_step_beats: f64) {
        transform_connect(pattern, scale, max_step_beats);
    }

    pub fn ornament(
        pattern: &mut MidiPattern,
        grace_offset_beats: f64,
        semitone_offset: i8,
        velocity_scale: f32,
    ) {
        transform_ornament(pattern, grace_offset_beats, semitone_offset, velocity_scale);
    }

    pub fn recombine(
        pattern: &mut MidiPattern,
        pitch_rot: isize,
        velocity_rot: isize,
        duration_rot: isize,
    ) {
        transform_recombine(pattern, pitch_rot, velocity_rot, duration_rot);
    }

    pub fn strum(pattern: &mut MidiPattern, delay_per_note_beats: f64, direction: StrumDirection) {
        transform_strum(pattern, delay_per_note_beats, direction);
    }

    pub fn time_warp(pattern: &mut MidiPattern, exponent: f64) {
        transform_time_warp(pattern, exponent);
    }

    pub fn euclidean(
        pulses: usize,
        total_steps: usize,
        rotation: usize,
        pitch: u8,
        step_duration_beats: f64,
        velocity: u8,
    ) -> MidiPattern {
        crate::generate::generate_euclidean(
            pulses,
            total_steps,
            rotation,
            pitch,
            step_duration_beats,
            velocity,
        )
    }

    pub fn chord(
        roots: &[u8],
        chord_type: crate::generate::StacksChordType,
        voicing: crate::generate::StacksVoicing,
        inversion: usize,
        duration_beats: f64,
        velocity: u8,
    ) -> MidiPattern {
        crate::generate::generate_stacks(
            roots,
            chord_type,
            voicing,
            inversion,
            duration_beats,
            velocity,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_strum() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        pat.add_note(MidiNote::new(64, 100, 0.0, 1.0));
        pat.add_note(MidiNote::new(67, 100, 0.0, 1.0));

        transform_strum(&mut pat, 0.02, StrumDirection::Up);

        assert_eq!(pat.notes[0].pitch, 60);
        assert!((pat.notes[0].start_beat - 0.0).abs() < 1e-4);
        assert_eq!(pat.notes[1].pitch, 64);
        assert!((pat.notes[1].start_beat - 0.02).abs() < 1e-4);
        assert_eq!(pat.notes[2].pitch, 67);
        assert!((pat.notes[2].start_beat - 0.04).abs() < 1e-4);
    }

    #[test]
    fn test_transform_recombine() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 80, 0.0, 0.5));
        pat.add_note(MidiNote::new(64, 100, 1.0, 1.0));

        transform_recombine(&mut pat, 1, 0, 0); // Rotate pitch by 1
        assert_eq!(pat.notes[0].pitch, 64);
        assert_eq!(pat.notes[1].pitch, 60);
    }

    #[test]
    fn test_transform_quantize() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 100, 0.23, 0.26));

        transform_quantize(&mut pat, 0.25, 1.0, 0.0);
        assert!((pat.notes[0].start_beat - 0.25).abs() < 1e-4);
        assert!((pat.notes[0].duration_beats - 0.25).abs() < 1e-4);
    }

    #[test]
    fn test_transform_invert() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 100, 0.0, 1.0)); // C4
        pat.add_note(MidiNote::new(64, 100, 1.0, 1.0)); // E4
        pat.add_note(MidiNote::new(67, 100, 2.0, 1.0)); // G4

        // Invert with explicit pivot at 60 (C4)
        transform_invert(&mut pat, Some(60));
        assert_eq!(pat.notes[0].pitch, 60); // 2*60 - 60 = 60
        assert_eq!(pat.notes[1].pitch, 56); // 2*60 - 64 = 56 (Ab3)
        assert_eq!(pat.notes[2].pitch, 53); // 2*60 - 67 = 53 (F3)

        // Invert back with explicit pivot at 60
        transform_invert(&mut pat, Some(60));
        assert_eq!(pat.notes[0].pitch, 60);
        assert_eq!(pat.notes[1].pitch, 64);
        assert_eq!(pat.notes[2].pitch, 67);
    }

    #[test]
    fn test_transform_reverse() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        pat.add_note(MidiNote::new(64, 90, 1.0, 1.0));
        pat.add_note(MidiNote::new(67, 110, 3.0, 0.5));

        transform_reverse(&mut pat);

        // Note 67 was at 3.0..3.5 -> new start = 4.0 - 3.5 = 0.5
        assert_eq!(pat.notes[0].pitch, 67);
        assert!((pat.notes[0].start_beat - 0.5).abs() < 1e-4);
        assert_eq!(pat.notes[0].duration_beats, 0.5);

        // Note 64 was at 1.0..2.0 -> new start = 4.0 - 2.0 = 2.0
        assert_eq!(pat.notes[1].pitch, 64);
        assert!((pat.notes[1].start_beat - 2.0).abs() < 1e-4);

        // Note 60 was at 0.0..1.0 -> new start = 4.0 - 1.0 = 3.0
        assert_eq!(pat.notes[2].pitch, 60);
        assert!((pat.notes[2].start_beat - 3.0).abs() < 1e-4);
    }

    #[test]
    fn test_transform_legato() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 100, 0.0, 0.5));
        pat.add_note(MidiNote::new(62, 100, 1.0, 0.5));
        pat.add_note(MidiNote::new(64, 100, 2.5, 0.5));

        transform_legato(&mut pat);

        // Note 0: extended from 0.0 to 1.0 (dur = 1.0)
        assert!((pat.notes[0].duration_beats - 1.0).abs() < 1e-4);
        // Note 1: extended from 1.0 to 2.5 (dur = 1.5)
        assert!((pat.notes[1].duration_beats - 1.5).abs() < 1e-4);
        // Note 2: extended from 2.5 to 4.0 (pattern length) (dur = 1.5)
        assert!((pat.notes[2].duration_beats - 1.5).abs() < 1e-4);
    }

    #[test]
    fn test_transform_humanize() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 100, 1.0, 0.5));
        pat.add_note(MidiNote::new(64, 100, 2.0, 0.5));

        // 15 ticks = 15 / 960 beats ~ 0.015625 beats
        let timing_jitter = 15.0 / 960.0;
        transform_humanize(&mut pat, timing_jitter, 8, 12345);

        assert!((pat.notes[0].start_beat - 1.0).abs() <= timing_jitter + 1e-5);
        assert!((pat.notes[0].velocity as i16 - 100).abs() <= 8);
        assert!((pat.notes[1].start_beat - 2.0).abs() <= timing_jitter + 1e-5);
        assert!((pat.notes[1].velocity as i16 - 100).abs() <= 8);
    }

    #[test]
    fn test_midi_transform_engine_facade() {
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        pat.add_note(MidiNote::new(64, 100, 1.0, 1.0));

        MidiTransformEngine::invert(&mut pat, Some(62));
        assert_eq!(pat.notes[0].pitch, 64);
        assert_eq!(pat.notes[1].pitch, 60);

        let euclid = MidiTransformEngine::euclidean(4, 16, 0, 36, 0.25, 100);
        assert_eq!(euclid.notes.len(), 4);

        let chords = MidiTransformEngine::chord(
            &[60],
            crate::generate::StacksChordType::MajorTriad,
            crate::generate::StacksVoicing::Closed,
            0,
            1.0,
            100,
        );
        assert_eq!(chords.notes.len(), 3);
    }
}
