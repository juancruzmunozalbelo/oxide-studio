//! Ableton Live 12 Chord MIDI Device.

use oxide_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

pub struct Chord {
    pub name: String,
    pub enabled: bool,
    pub shifts: [i8; 6], // Semitone shifts from root note (-36 .. +36)
}

impl Chord {
    pub fn new() -> Self {
        Self {
            name: "Chord".to_string(),
            enabled: true,
            shifts: [0, 0, 0, 0, 0, 0],
        }
    }

    pub fn with_shifts(shifts: [i8; 6]) -> Self {
        Self {
            name: "Chord".to_string(),
            enabled: true,
            shifts,
        }
    }
}

impl Default for Chord {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for Chord {
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

impl MidiEffectDevice for Chord {
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
                    events_out.push(*ev);
                    for &shift in &self.shifts {
                        if shift != 0 {
                            let new_note = (*note as i16 + shift as i16).clamp(0, 127) as u8;
                            events_out.push(MidiEvent::NoteOn {
                                channel: *channel,
                                note: new_note,
                                velocity: *velocity,
                            });
                        }
                    }
                }
                MidiEvent::NoteOff { channel, note } => {
                    events_out.push(*ev);
                    for &shift in &self.shifts {
                        if shift != 0 {
                            let new_note = (*note as i16 + shift as i16).clamp(0, 127) as u8;
                            events_out.push(MidiEvent::NoteOff {
                                channel: *channel,
                                note: new_note,
                            });
                        }
                    }
                }
                _ => events_out.push(*ev),
            }
        }
    }
}
