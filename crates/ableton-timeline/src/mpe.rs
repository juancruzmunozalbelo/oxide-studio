//! MIDI Polyphonic Expression (MPE) per-note expression models and stream parser.
//!
//! Compliant with Ableton Live 12 Suite Chapter 12 ("Editing MPE: Per-Note Expression & Slide").
//! Provides per-note Pressure (polyphonic aftertouch / CC11), Slide (CC74 timbre / Y-axis),
//! and per-note Pitch Bend parsing across multi-channel MPE zones (Lower and Upper).

use crate::pattern::MidiNote;
use ableton_core::MidiEvent;
use serde::{Deserialize, Serialize};

/// Breakpoint within a per-note expression envelope.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MpeExpressionPoint {
    /// Beat offset from note onset.
    pub beat_offset: f64,
    /// Value:
    /// - Pressure: normalized `[0.0, 1.0]`
    /// - Slide: normalized `[0.0, 1.0]` (CC74)
    /// - Pitch Bend: semitones `[-48.0, +48.0]`
    pub value: f32,
}

impl MpeExpressionPoint {
    pub const fn new(beat_offset: f64, value: f32) -> Self {
        Self {
            beat_offset,
            value,
        }
    }
}

/// Polyphonic per-note expression curves for an MPE note.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PerNoteExpression {
    /// Per-note pressure / aftertouch breakpoints
    pub pressure_points: Vec<MpeExpressionPoint>,
    /// Per-note slide (CC74) breakpoints
    pub slide_points: Vec<MpeExpressionPoint>,
    /// Per-note pitch bend breakpoints (semitones)
    pub pitch_bend_points: Vec<MpeExpressionPoint>,
}

impl PerNoteExpression {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_pressure(&mut self, beat_offset: f64, value: f32) {
        self.pressure_points.push(MpeExpressionPoint::new(
            beat_offset,
            value.clamp(0.0, 1.0),
        ));
    }

    pub fn add_slide(&mut self, beat_offset: f64, value: f32) {
        self.slide_points.push(MpeExpressionPoint::new(
            beat_offset,
            value.clamp(0.0, 1.0),
        ));
    }

    pub fn add_pitch_bend(&mut self, beat_offset: f64, semitones: f32) {
        self.pitch_bend_points.push(MpeExpressionPoint::new(
            beat_offset,
            semitones.clamp(-48.0, 48.0),
        ));
    }

    fn interpolate_points(points: &[MpeExpressionPoint], beat_offset: f64, default_val: f32) -> f32 {
        if points.is_empty() {
            return default_val;
        }
        if beat_offset <= points[0].beat_offset {
            return points[0].value;
        }
        if beat_offset >= points[points.len() - 1].beat_offset {
            return points[points.len() - 1].value;
        }

        for i in 0..points.len() - 1 {
            let p0 = &points[i];
            let p1 = &points[i + 1];
            if beat_offset >= p0.beat_offset && beat_offset <= p1.beat_offset {
                let span = p1.beat_offset - p0.beat_offset;
                if span <= 1e-9 {
                    return p1.value;
                }
                let t = ((beat_offset - p0.beat_offset) / span) as f32;
                return p0.value + t * (p1.value - p0.value);
            }
        }

        default_val
    }

    /// Evaluate per-note pressure at `beat_offset` (defaults to 0.0).
    pub fn evaluate_pressure_at(&self, beat_offset: f64) -> f32 {
        Self::interpolate_points(&self.pressure_points, beat_offset, 0.0)
    }

    /// Evaluate per-note slide (CC74) at `beat_offset` (defaults to 0.0).
    pub fn evaluate_slide_at(&self, beat_offset: f64) -> f32 {
        Self::interpolate_points(&self.slide_points, beat_offset, 0.0)
    }

    /// Evaluate per-note pitch bend (semitones) at `beat_offset` (defaults to 0.0).
    pub fn evaluate_pitch_bend_at(&self, beat_offset: f64) -> f32 {
        Self::interpolate_points(&self.pitch_bend_points, beat_offset, 0.0)
    }
}

/// An MPE-aware musical note event with dedicated channel and per-note expression curves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MpeNote {
    pub note: MidiNote,
    pub channel: u8,
    pub expression: PerNoteExpression,
}

impl MpeNote {
    pub fn new(note: MidiNote, channel: u8) -> Self {
        Self {
            note,
            channel,
            expression: PerNoteExpression::new(),
        }
    }

    pub fn with_expression(mut self, expression: PerNoteExpression) -> Self {
        self.expression = expression;
        self
    }
}

/// MPE Zone configuration according to the MIDI Polyphonic Expression specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MpeZone {
    /// Lower zone: Global/Master channel 1, member channels 2..=16
    Lower,
    /// Upper zone: Global/Master channel 16, member channels 1..=15
    Upper,
}

impl Default for MpeZone {
    fn default() -> Self {
        MpeZone::Lower
    }
}

/// In-flight voice tracking for active MPE notes during stream parsing.
#[derive(Debug, Clone)]
struct ActiveVoice {
    note_number: u8,
    velocity: u8,
    start_beat: f64,
    channel: u8,
    expression: PerNoteExpression,
}

/// Parser that decodes raw multi-channel MIDI/MPE event streams into per-note expression data.
#[derive(Debug, Clone)]
pub struct MpeStreamParser {
    pub zone: MpeZone,
    pub pitch_bend_range_semitones: f32, // Typically 48.0 for MPE
    active_voices: [Option<ActiveVoice>; 16],
    finished_notes: Vec<MpeNote>,
}

impl Default for MpeStreamParser {
    fn default() -> Self {
        Self::new(MpeZone::Lower)
    }
}

impl MpeStreamParser {
    pub fn new(zone: MpeZone) -> Self {
        Self {
            zone,
            pitch_bend_range_semitones: 48.0,
            active_voices: std::array::from_fn(|_| None),
            finished_notes: Vec::new(),
        }
    }

    /// Check if a MIDI channel is a member voice channel within the configured MPE zone.
    pub fn is_member_channel(&self, channel: u8) -> bool {
        match self.zone {
            MpeZone::Lower => channel >= 1 && channel <= 15, // 0-indexed: channels 1..15 are 2..16 in 1-indexed MIDI
            MpeZone::Upper => channel <= 14,                // 0-indexed: channels 0..14 are 1..15 in 1-indexed MIDI
        }
    }

    /// Process a timestamped MIDI event and route per-note expression to the corresponding voice.
    pub fn process_event(&mut self, event: &MidiEvent, beat: f64) {
        match *event {
            MidiEvent::NoteOn { channel, note, velocity } => {
                if velocity == 0 {
                    self.end_note(channel, note, beat);
                } else {
                    let ch_idx = (channel as usize).min(15);
                    // If channel was already occupied, end previous note
                    if let Some(prev) = self.active_voices[ch_idx].take() {
                        let dur = (beat - prev.start_beat).max(0.01);
                        let m_note = MidiNote::new(prev.note_number, prev.velocity, prev.start_beat, dur);
                        self.finished_notes.push(MpeNote {
                            note: m_note,
                            channel: prev.channel,
                            expression: prev.expression,
                        });
                    }

                    self.active_voices[ch_idx] = Some(ActiveVoice {
                        note_number: note,
                        velocity,
                        start_beat: beat,
                        channel,
                        expression: PerNoteExpression::new(),
                    });
                }
            }
            MidiEvent::NoteOff { channel, note } => {
                self.end_note(channel, note, beat);
            }
            MidiEvent::PitchBend { channel, value } => {
                let ch_idx = (channel as usize).min(15);
                if let Some(voice) = &mut self.active_voices[ch_idx] {
                    let semitones = (value as f32 / 8192.0) * self.pitch_bend_range_semitones;
                    let offset = (beat - voice.start_beat).max(0.0);
                    voice.expression.add_pitch_bend(offset, semitones);
                }
            }
            MidiEvent::ControlChange { channel, controller, value } => {
                let ch_idx = (channel as usize).min(15);
                if let Some(voice) = &mut self.active_voices[ch_idx] {
                    let offset = (beat - voice.start_beat).max(0.0);
                    let norm = value as f32 / 127.0;
                    if controller == 74 {
                        // MPE Slide (CC74)
                        voice.expression.add_slide(offset, norm);
                    } else if controller == 11 {
                        // MPE Pressure / Expression (CC11)
                        voice.expression.add_pressure(offset, norm);
                    }
                }
            }
        }
    }

    fn end_note(&mut self, channel: u8, note: u8, beat: f64) {
        let ch_idx = (channel as usize).min(15);
        if let Some(voice) = &self.active_voices[ch_idx] {
            if voice.note_number == note {
                if let Some(v) = self.active_voices[ch_idx].take() {
                    let dur = (beat - v.start_beat).max(0.01);
                    let m_note = MidiNote::new(v.note_number, v.velocity, v.start_beat, dur);
                    self.finished_notes.push(MpeNote {
                        note: m_note,
                        channel: v.channel,
                        expression: v.expression,
                    });
                }
            }
        }
    }

    /// Flush any remaining active voices up to `end_beat`.
    pub fn flush_active_voices(&mut self, end_beat: f64) {
        for slot in &mut self.active_voices {
            if let Some(v) = slot.take() {
                let dur = (end_beat - v.start_beat).max(0.01);
                let m_note = MidiNote::new(v.note_number, v.velocity, v.start_beat, dur);
                self.finished_notes.push(MpeNote {
                    note: m_note,
                    channel: v.channel,
                    expression: v.expression,
                });
            }
        }
    }

    /// Parse an entire sequence of timestamped MIDI events into parsed `MpeNote`s.
    pub fn parse_stream(&mut self, events: &[(f64, MidiEvent)], end_beat: f64) -> Vec<MpeNote> {
        self.finished_notes.clear();
        for (beat, ev) in events {
            self.process_event(ev, *beat);
        }
        self.flush_active_voices(end_beat);
        self.finished_notes.clone()
    }

    /// Access currently completed parsed MPE notes.
    pub fn finished_notes(&self) -> &[MpeNote] {
        &self.finished_notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_per_note_expression_interpolation() {
        let mut expr = PerNoteExpression::new();
        expr.add_pressure(0.0, 0.2);
        expr.add_pressure(1.0, 0.8);

        expr.add_slide(0.0, 0.1);
        expr.add_slide(2.0, 0.9);

        expr.add_pitch_bend(0.0, 0.0);
        expr.add_pitch_bend(1.0, 12.0);

        // Test pressure interpolation
        assert_eq!(expr.evaluate_pressure_at(0.0), 0.2);
        assert!((expr.evaluate_pressure_at(0.5) - 0.5).abs() < 1e-4);
        assert_eq!(expr.evaluate_pressure_at(1.0), 0.8);
        assert_eq!(expr.evaluate_pressure_at(2.0), 0.8); // Clamped to last

        // Test slide interpolation
        assert_eq!(expr.evaluate_slide_at(0.0), 0.1);
        assert!((expr.evaluate_slide_at(1.0) - 0.5).abs() < 1e-4);
        assert_eq!(expr.evaluate_slide_at(2.0), 0.9);

        // Test pitch bend interpolation
        assert_eq!(expr.evaluate_pitch_bend_at(0.0), 0.0);
        assert!((expr.evaluate_pitch_bend_at(0.5) - 6.0).abs() < 1e-4);
        assert_eq!(expr.evaluate_pitch_bend_at(1.0), 12.0);
    }

    #[test]
    fn test_mpe_stream_parser_polyphonic_notes() {
        let mut parser = MpeStreamParser::new(MpeZone::Lower);

        // Stream 2 simultaneous notes on independent channels (ch 1 and ch 2)
        // Ch 1 Note 60 (C4) with upward pitch bend and high pressure
        // Ch 2 Note 64 (E4) with downward pitch bend and high slide
        let stream = vec![
            (0.0, MidiEvent::NoteOn { channel: 1, note: 60, velocity: 100 }),
            (0.0, MidiEvent::NoteOn { channel: 2, note: 64, velocity: 90 }),
            (0.2, MidiEvent::PitchBend { channel: 1, value: 4096 }), // +24 semitones on note 60
            (0.2, MidiEvent::PitchBend { channel: 2, value: -4096 }), // -24 semitones on note 64
            (0.5, MidiEvent::ControlChange { channel: 1, controller: 11, value: 100 }), // Pressure on note 60
            (0.5, MidiEvent::ControlChange { channel: 2, controller: 74, value: 110 }), // Slide on note 64
            (1.0, MidiEvent::NoteOff { channel: 1, note: 60 }),
            (1.0, MidiEvent::NoteOff { channel: 2, note: 64 }),
        ];

        let notes = parser.parse_stream(&stream, 1.0);
        assert_eq!(notes.len(), 2, "Must parse exactly 2 polyphonic MPE notes");

        let note60 = notes.iter().find(|n| n.note.pitch == 60).unwrap();
        let note64 = notes.iter().find(|n| n.note.pitch == 64).unwrap();

        // Verify independent pitch bends
        let pb60 = note60.expression.evaluate_pitch_bend_at(0.2);
        let pb64 = note64.expression.evaluate_pitch_bend_at(0.2);
        assert!(pb60 > 20.0, "Note 60 must have positive pitch bend, got {}", pb60);
        assert!(pb64 < -20.0, "Note 64 must have negative pitch bend, got {}", pb64);

        // Verify note 60 has pressure and note 64 has slide
        let press60 = note60.expression.evaluate_pressure_at(0.5);
        let slide64 = note64.expression.evaluate_slide_at(0.5);
        assert!(press60 > 0.7, "Note 60 must have pressure, got {}", press60);
        assert!(slide64 > 0.8, "Note 64 must have slide, got {}", slide64);
    }
}
