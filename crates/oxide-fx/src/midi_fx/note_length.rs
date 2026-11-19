//! Ableton Live 12 Note Length MIDI Device.

use oxide_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

pub struct NoteLength {
    pub name: String,
    pub enabled: bool,
    pub length_beats: f64, // Fixed note length in beats
    pub gate_percent: f32, // 0.1 .. 2.0
    active_notes: Vec<(u8, u8, f64)>, // (channel, pitch, note_on_beat)
}

impl NoteLength {
    pub fn new() -> Self {
        Self {
            name: "Note Length".to_string(),
            enabled: true,
            length_beats: 0.25, // 1/16th note
            gate_percent: 1.0,
            active_notes: Vec::with_capacity(16),
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
                MidiEvent::NoteOn { channel, note, velocity } => {
                    events_out.push(*ev);
                    if *velocity > 0 {
                        self.active_notes.push((*channel, *note, time.beat));
                    }
                }
                MidiEvent::NoteOff { .. } => {
                    // Suppress immediate NoteOff if we manage note duration
                }
                _ => events_out.push(*ev),
            }
        }

        // Check in-place if any note has reached its target length (zero allocations)
        let mut idx = 0;
        while idx < self.active_notes.len() {
            let (channel, note, start_beat) = self.active_notes[idx];
            if time.beat >= start_beat + target_len {
                events_out.push(MidiEvent::NoteOff { channel, note });
                self.active_notes.swap_remove(idx);
            } else {
                idx += 1;
            }
        }
    }
}
