//! Ableton Live 12 Pitch MIDI Device.

use oxide_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

pub struct Pitch {
    pub name: String,
    pub enabled: bool,
    pub pitch_shift: i16, // Semitones (-128 .. +128)
    pub lowest_note: u8,
    pub highest_note: u8,
}

impl Pitch {
    pub fn new(pitch_shift: i16) -> Self {
        Self {
            name: "Pitch".to_string(),
            enabled: true,
            pitch_shift,
            lowest_note: 0,
            highest_note: 127,
        }
    }
}

impl Default for Pitch {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Device for Pitch {
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

impl MidiEffectDevice for Pitch {
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
                    if *note >= self.lowest_note && *note <= self.highest_note {
                        let new_note = (*note as i16 + self.pitch_shift).clamp(0, 127) as u8;
                        events_out.push(MidiEvent::NoteOn {
                            channel: *channel,
                            note: new_note,
                            velocity: *velocity,
                        });
                    }
                }
                MidiEvent::NoteOff { channel, note } => {
                    if *note >= self.lowest_note && *note <= self.highest_note {
                        let new_note = (*note as i16 + self.pitch_shift).clamp(0, 127) as u8;
                        events_out.push(MidiEvent::NoteOff {
                            channel: *channel,
                            note: new_note,
                        });
                    }
                }
                _ => events_out.push(*ev),
            }
        }
    }
}
