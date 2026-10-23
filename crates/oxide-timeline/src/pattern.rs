//! Pattern models: 16/32-step drum patterns, piano-roll melodic patterns, and audio clip data.

use serde::{Deserialize, Serialize};

/// An individual step within a drum sequencer track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrumStep {
    pub active: bool,
    pub velocity: u8, // 1..127
    pub pitch: u8,    // MIDI note number, e.g. 36 for kick
    pub accent: bool,
    pub tie: bool,
    pub choke_group: Option<u8>,
}

impl Default for DrumStep {
    fn default() -> Self {
        Self {
            active: false,
            velocity: 100,
            pitch: 36,
            accent: false,
            tie: false,
            choke_group: None,
        }
    }
}

impl DrumStep {
    pub const fn new(active: bool, velocity: u8, pitch: u8) -> Self {
        Self {
            active,
            velocity,
            pitch,
            accent: false,
            tie: false,
            choke_group: None,
        }
    }

    pub const fn with_accent(mut self, accent: bool) -> Self {
        self.accent = accent;
        self
    }

    pub const fn with_choke_group(mut self, group: Option<u8>) -> Self {
        self.choke_group = group;
        self
    }
}

/// A track of steps dedicated to a specific drum pad (e.g. Pad 0 = Kick, Pad 1 = Snare).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrumStepTrack {
    pub pad_index: u8,
    pub name: String,
    pub steps: Vec<DrumStep>,
}

impl DrumStepTrack {
    pub fn new(pad_index: u8, name: impl Into<String>, step_count: usize, default_pitch: u8) -> Self {
        let mut steps = Vec::with_capacity(step_count);
        for _ in 0..step_count {
            steps.push(DrumStep::new(false, 100, default_pitch));
        }
        Self {
            pad_index,
            name: name.into(),
            steps,
        }
    }
}

/// Multi-pad 16-step or 32-step drum pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrumStepPattern {
    pub step_count: usize,        // Typically 16 or 32
    pub step_duration_beats: f64, // Typically 0.25 (16th note)
    pub tracks: Vec<DrumStepTrack>,
}

impl DrumStepPattern {
    /// Create a standard 16-pad, 16-step drum pattern (length: 4.0 beats / 1 bar).
    pub fn new_16_step() -> Self {
        let pad_names = [
            (0, "Kick", 36),
            (1, "Snare", 37),
            (2, "Low Tom", 38),
            (3, "Mid Tom", 39),
            (4, "Hi Tom", 40),
            (5, "Rimshot", 41),
            (6, "Clap", 42),
            (7, "Cowbell", 43),
            (8, "Closed HH", 44),
            (9, "Open HH", 45),
            (10, "Crash", 46),
            (11, "Ride", 47),
            (12, "Perc 1", 48),
            (13, "Perc 2", 49),
            (14, "Shaker", 50),
            (15, "Tambourine", 51),
        ];

        let mut tracks = Vec::with_capacity(16);
        for (idx, name, pitch) in pad_names {
            let mut trk = DrumStepTrack::new(idx, name, 16, pitch);
            if idx == 8 || idx == 9 {
                // Closed & Open Hi-Hats share choke group 1
                for s in &mut trk.steps {
                    s.choke_group = Some(1);
                }
            }
            tracks.push(trk);
        }

        Self {
            step_count: 16,
            step_duration_beats: 0.25,
            tracks,
        }
    }

    /// Create a 32-step drum pattern (length: 8.0 beats / 2 bars).
    pub fn new_32_step() -> Self {
        let mut p = Self::new_16_step();
        p.step_count = 32;
        for trk in &mut p.tracks {
            let pitch = trk.steps.first().map(|s| s.pitch).unwrap_or(36);
            let choke = trk.steps.first().and_then(|s| s.choke_group);
            while trk.steps.len() < 32 {
                let mut step = DrumStep::new(false, 100, pitch);
                step.choke_group = choke;
                trk.steps.push(step);
            }
        }
        p
    }

    /// Total duration of the drum pattern in musical beats.
    pub fn length_beats(&self) -> f64 {
        self.step_count as f64 * self.step_duration_beats
    }

    /// Set an individual step.
    pub fn set_step(&mut self, pad_index: u8, step_idx: usize, step: DrumStep) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.pad_index == pad_index) {
            if step_idx < track.steps.len() {
                track.steps[step_idx] = step;
            }
        }
    }

    /// Toggle active state of a step.
    pub fn toggle_step(&mut self, pad_index: u8, step_idx: usize) -> bool {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.pad_index == pad_index) {
            if step_idx < track.steps.len() {
                track.steps[step_idx].active = !track.steps[step_idx].active;
                return track.steps[step_idx].active;
            }
        }
        false
    }

    /// Get all active step triggers at a given step index.
    pub fn active_steps_at(&self, step_idx: usize) -> Vec<(u8, DrumStep)> {
        let mut hits = Vec::new();
        for trk in &self.tracks {
            if let Some(step) = trk.steps.get(step_idx) {
                if step.active {
                    hits.push((trk.pad_index, *step));
                }
            }
        }
        hits
    }
}

/// A chromatic note event in a piano-roll pattern.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MidiNote {
    pub pitch: u8,           // MIDI Note (0..127)
    pub velocity: u8,        // Velocity (1..127)
    pub start_beat: f64,     // Beat start position within the clip
    pub duration_beats: f64, // Duration in beats
    pub probability: f32,    // 0.0 .. 1.0 (1.0 = always plays)
}

impl MidiNote {
    pub const fn new(pitch: u8, velocity: u8, start_beat: f64, duration_beats: f64) -> Self {
        Self {
            pitch,
            velocity,
            start_beat,
            duration_beats,
            probability: 1.0,
        }
    }

    pub const fn with_probability(mut self, prob: f32) -> Self {
        self.probability = prob;
        self
    }

    pub fn end_beat(&self) -> f64 {
        self.start_beat + self.duration_beats
    }
}

/// A melodic or harmonic pattern of chromatic MIDI notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiPattern {
    pub length_beats: f64,
    pub notes: Vec<MidiNote>,
}

impl MidiPattern {
    pub fn new(length_beats: f64) -> Self {
        Self {
            length_beats,
            notes: Vec::new(),
        }
    }

    pub fn add_note(&mut self, note: MidiNote) {
        self.notes.push(note);
    }

    /// Returns notes that are currently sounding at `beat`.
    pub fn notes_active_at_beat(&self, beat: f64) -> Vec<&MidiNote> {
        self.notes
            .iter()
            .filter(|n| beat >= n.start_beat && beat < n.end_beat())
            .collect()
    }

    /// Returns notes whose start_beat falls within [start, end).
    pub fn notes_starting_in_range(&self, start: f64, end: f64) -> Vec<&MidiNote> {
        self.notes
            .iter()
            .filter(|n| n.start_beat >= start && n.start_beat < end)
            .collect()
    }

    /// Nudge a specific note by `delta_beats`, clamping within `[0.0, length_beats - duration]`.
    pub fn nudge_note(&mut self, note_idx: usize, delta_beats: f64) -> bool {
        if let Some(note) = self.notes.get_mut(note_idx) {
            let max_start = (self.length_beats - note.duration_beats).max(0.0);
            note.start_beat = (note.start_beat + delta_beats).clamp(0.0, max_start);
            true
        } else {
            false
        }
    }

    /// Nudge all notes in the pattern by `delta_beats`, clamping each within `[0.0, length_beats - duration]`.
    pub fn nudge_all(&mut self, delta_beats: f64) {
        let len = self.length_beats;
        for note in &mut self.notes {
            let max_start = (len - note.duration_beats).max(0.0);
            note.start_beat = (note.start_beat + delta_beats).clamp(0.0, max_start);
        }
    }

    /// Stretch all notes relative to an `origin_beat` by `factor` (> 0.0).
    /// Both note start positions and durations are scaled, clamped within pattern length.
    pub fn stretch_notes(&mut self, factor: f64, origin_beat: f64) {
        let f = factor.max(0.01);
        let len = self.length_beats;
        for note in &mut self.notes {
            let rel_start = note.start_beat - origin_beat;
            note.start_beat = (origin_beat + rel_start * f).clamp(0.0, len);
            note.duration_beats = (note.duration_beats * f).max(0.0625);
            if note.end_beat() > len {
                note.duration_beats = (len - note.start_beat).max(0.0625);
            }
        }
    }

    /// Quantize all note start times and durations to `grid_beats` with strength and swing.
    pub fn quantize(&mut self, grid_beats: f64, strength: f32, swing: f32) {
        crate::transform::transform_quantize(self, grid_beats, strength, swing);
    }
}

/// Audio sample clip metadata for timeline playback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioClipData {
    pub file_path: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub gain_db: f32,
    pub warp_mode: String,
    pub length_beats: f64,
}

impl AudioClipData {
    pub fn new(file_path: impl Into<String>, sample_rate: u32, channels: u16, length_beats: f64) -> Self {
        Self {
            file_path: file_path.into(),
            sample_rate,
            channels,
            gain_db: 0.0,
            warp_mode: "Beats".to_string(),
            length_beats,
        }
    }
}

/// Polymorphic pattern payload contained in a clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClipPattern {
    Midi(MidiPattern),
    Drum(DrumStepPattern),
    Audio(AudioClipData),
}

impl ClipPattern {
    /// Total duration of the pattern in musical beats.
    pub fn length_beats(&self) -> f64 {
        match self {
            ClipPattern::Midi(m) => m.length_beats,
            ClipPattern::Drum(d) => d.length_beats(),
            ClipPattern::Audio(a) => a.length_beats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drum_step_pattern_16_step() {
        let mut drum = DrumStepPattern::new_16_step();
        assert_eq!(drum.step_count, 16);
        assert_eq!(drum.length_beats(), 4.0);
        assert_eq!(drum.tracks.len(), 16);

        // Toggle kick on steps 0, 4, 8, 12
        drum.toggle_step(0, 0);
        drum.toggle_step(0, 4);
        drum.toggle_step(0, 8);
        drum.toggle_step(0, 12);

        // Toggle snare on step 4 and 12
        drum.toggle_step(1, 4);
        drum.toggle_step(1, 12);

        let hits_step_0 = drum.active_steps_at(0);
        assert_eq!(hits_step_0.len(), 1);
        assert_eq!(hits_step_0[0].0, 0); // kick

        let hits_step_4 = drum.active_steps_at(4);
        assert_eq!(hits_step_4.len(), 2); // kick + snare
    }

    #[test]
    fn test_midi_pattern_queries() {
        let mut midi = MidiPattern::new(8.0);
        midi.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        midi.add_note(MidiNote::new(64, 90, 0.5, 1.5));
        midi.add_note(MidiNote::new(67, 110, 2.0, 2.0));

        // Active notes at 0.75: note 60 (0..1) and note 64 (0.5..2.0)
        let active = midi.notes_active_at_beat(0.75);
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].pitch, 60);
        assert_eq!(active[1].pitch, 64);

        // Starting in range [1.5, 3.0): note 67 (start=2.0)
        let starting = midi.notes_starting_in_range(1.5, 3.0);
        assert_eq!(starting.len(), 1);
        assert_eq!(starting[0].pitch, 67);
    }

    #[test]
    fn test_midi_note_nudge() {
        let mut midi = MidiPattern::new(8.0);
        midi.add_note(MidiNote::new(60, 100, 2.0, 1.0));
        assert!(midi.nudge_note(0, 0.5));
        assert_eq!(midi.notes[0].start_beat, 2.5);

        // Nudge backwards past 0.0 clamps to 0.0
        assert!(midi.nudge_note(0, -5.0));
        assert_eq!(midi.notes[0].start_beat, 0.0);

        // Nudge forward past length clamps to length - duration
        assert!(midi.nudge_note(0, 10.0));
        assert_eq!(midi.notes[0].start_beat, 7.0);

        // Invalid index returns false
        assert!(!midi.nudge_note(99, 1.0));

        // Nudge all notes
        midi.add_note(MidiNote::new(64, 100, 1.0, 1.0));
        midi.nudge_all(-1.0);
        assert_eq!(midi.notes[0].start_beat, 6.0);
        assert_eq!(midi.notes[1].start_beat, 0.0);
    }

    #[test]
    fn test_midi_note_stretch() {
        let mut midi = MidiPattern::new(16.0);
        midi.add_note(MidiNote::new(60, 100, 2.0, 1.0));
        midi.add_note(MidiNote::new(64, 100, 4.0, 2.0));

        // 2x stretch relative to origin 0.0
        midi.stretch_notes(2.0, 0.0);
        assert_eq!(midi.notes[0].start_beat, 4.0);
        assert_eq!(midi.notes[0].duration_beats, 2.0);
        assert_eq!(midi.notes[1].start_beat, 8.0);
        assert_eq!(midi.notes[1].duration_beats, 4.0);

        // 0.5x stretch relative to origin 0.0
        midi.stretch_notes(0.5, 0.0);
        assert_eq!(midi.notes[0].start_beat, 2.0);
        assert_eq!(midi.notes[0].duration_beats, 1.0);
        assert_eq!(midi.notes[1].start_beat, 4.0);
        assert_eq!(midi.notes[1].duration_beats, 2.0);
    }

    #[test]
    fn test_midi_note_quantize() {
        let mut midi = MidiPattern::new(8.0);
        midi.add_note(MidiNote::new(60, 100, 0.23, 0.26));
        midi.add_note(MidiNote::new(64, 100, 0.98, 0.52));

        midi.quantize(0.25, 1.0, 0.0);
        assert!((midi.notes[0].start_beat - 0.25).abs() < 1e-4);
        assert!((midi.notes[0].duration_beats - 0.25).abs() < 1e-4);
        assert!((midi.notes[1].start_beat - 1.0).abs() < 1e-4);
        assert!((midi.notes[1].duration_beats - 0.5).abs() < 1e-4);
    }
}
