//! Ableton Live 12 Scale MIDI Device.

use oxide_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

pub struct Scale {
    pub name: String,
    pub enabled: bool,
    pub base_note: u8,      // 0 = C, 1 = C#, ..., 11 = B
    pub scale_degrees: u16, // Bitmask of 12 semitones in scale (bit 0 = root)
    pub transpose: i8,      // Semitone transposition
}

impl Scale {
    pub fn new() -> Self {
        // Default Major scale: C D E F G A B (intervals: 0, 2, 4, 5, 7, 9, 11)
        let major_mask = 1 | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11);
        Self {
            name: "Scale".to_string(),
            enabled: true,
            base_note: 0,
            scale_degrees: major_mask,
            transpose: 0,
        }
    }

    pub fn natural_minor(base_note: u8) -> Self {
        // Minor: 0, 2, 3, 5, 7, 8, 10
        let minor_mask = 1 | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 8) | (1 << 10);
        Self {
            name: "Scale".to_string(),
            enabled: true,
            base_note,
            scale_degrees: minor_mask,
            transpose: 0,
        }
    }

    /// Snap pitch to the closest in-scale semitone.
    pub fn quantize_pitch(&self, note: u8) -> u8 {
        let transposed = (note as i16 + self.transpose as i16).clamp(0, 127) as u8;
        let octave = transposed / 12;
        let pitch_class = (transposed % 12 + 12 - self.base_note % 12) % 12;

        if (self.scale_degrees & (1 << pitch_class)) != 0 {
            return transposed;
        }

        // Find nearest scale pitch class
        for dist in 1..=6 {
            let lower = (pitch_class + 12 - dist) % 12;
            if (self.scale_degrees & (1 << lower)) != 0 {
                return ((octave as i16 * 12 + (lower + self.base_note) as i16).clamp(0, 127)) as u8;
            }
            let upper = (pitch_class + dist) % 12;
            if (self.scale_degrees & (1 << upper)) != 0 {
                return ((octave as i16 * 12 + (upper + self.base_note) as i16).clamp(0, 127)) as u8;
            }
        }

        transposed
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for Scale {
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

    fn reset(&mut self) {}
}

impl MidiEffectDevice for Scale {
    fn process_midi(
        &mut self,
        events_in: &[MidiEvent],
        events_out: &mut Vec<MidiEvent>,
        _time: &MusicalTime,
    ) {
        if !self.enabled {
            events_out.extend_from_slice(events_in);
            return;
        }

        for ev in events_in {
            match ev {
                MidiEvent::NoteOn { channel, note, velocity } => {
                    events_out.push(MidiEvent::NoteOn {
                        channel: *channel,
                        note: self.quantize_pitch(*note),
                        velocity: *velocity,
                    });
                }
                MidiEvent::NoteOff { channel, note } => {
                    events_out.push(MidiEvent::NoteOff {
                        channel: *channel,
                        note: self.quantize_pitch(*note),
                    });
                }
                _ => events_out.push(*ev),
            }
        }
    }
}
