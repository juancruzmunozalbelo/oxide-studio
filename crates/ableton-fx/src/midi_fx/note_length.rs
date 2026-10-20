//! Ableton Live 12 Note Length MIDI Device.

use ableton_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

pub struct NoteLength {
    pub name: String,
    pub enabled: bool,
    pub length_beats: f64, // Fixed note length in beats
    pub gate_percent: f32, // 0.1 .. 2.0
    active_notes: Vec<(u8, f64)>, // (pitch, note_on_beat)
}

impl NoteLength {
    pub fn new() -> Self {
        Self {
            name: "Note Length".to_string(),
            enabled: true,
            length_beats: 0.25, // 1/16th note
            gate_percent: 1.0,
            active_notes: Vec::new(),
        }
    }
}

impl Default for NoteLength {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for NoteLength {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::MidiEffect
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self) {
        self.active_notes.clear();
    }
}

impl MidiEffectDevice for NoteLength {
    fn process_midi(
        &mut self,
        events_in: &[MidiEvent],
        events_out: &mut Vec<MidiEvent>,
        time: &MusicalTime,
    ) {
        if !self.enabled {
            events_out.extend_from_slice(events_in);
            return;
        }

        let target_len = self.length_beats * (self.gate_percent as f64);

        for ev in events_in {
            match ev {
                MidiEvent::NoteOn { note, .. } => {
                    events_out.push(*ev);
                    self.active_notes.push((*note, time.beat));
                }
                MidiEvent::NoteOff { .. } => {
                    // Suppress immediate NoteOff if we manage note duration
                }
                _ => events_out.push(*ev),
            }
        }

        // Check if any note has reached its target length
        let mut expired = Vec::new();
        self.active_notes.retain(|(note, start_beat)| {
            if time.beat >= *start_beat + target_len {
                expired.push(*note);
                false
            } else {
                true
            }
        });

        for note in expired {
            events_out.push(MidiEvent::NoteOff { channel: 0, note });
        }
    }
}
