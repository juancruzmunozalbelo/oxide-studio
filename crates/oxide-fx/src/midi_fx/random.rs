//! Ableton Live 12 Random MIDI Device.

use oxide_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomMode {
    Add,
    Sub,
    Bipolar,
}

pub struct Random {
    pub name: String,
    pub enabled: bool,
    pub choices: u8,   // 1 .. 24
    pub chance: f32,   // 0.0 .. 1.0
    pub mode: RandomMode,
    active_shifts: Vec<(u8, u8, i16)>, // (channel, original_note, applied_shift)
}

impl Random {
    pub fn new() -> Self {
        Self {
            name: "Random".to_string(),
            enabled: true,
            choices: 12,
            chance: 0.5,
            mode: RandomMode::Bipolar,
            active_shifts: Vec::with_capacity(16),
        }
    }
    fn pop_shift(&mut self, channel: u8, note: u8) -> i16 {
        if let Some(idx) = self
            .active_shifts
            .iter()
            .position(|(ch, p, _)| *ch == channel && *p == note)
        {
            self.active_shifts.swap_remove(idx).2
        } else {
            0
        }
    }
}

impl Default for Random {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for Random {
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
        self.active_shifts.clear();
    }
}

impl MidiEffectDevice for Random {
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
                MidiEvent::NoteOn { channel, note, velocity } if *velocity == 0 => {
                    let shift = self.pop_shift(*channel, *note);
                    let new_note = (*note as i16 + shift).clamp(0, 127) as u8;
                    events_out.push(MidiEvent::NoteOn {
                        channel: *channel,
                        note: new_note,
                        velocity: 0,
                    });
                }
                MidiEvent::NoteOn { channel, note, velocity } => {
                    let rand_val = ((*note as f32 * 1234.567 + *velocity as f32 * 98.76).sin()).abs();
                    let shift = if rand_val < self.chance {
                        let c = self.choices.max(1) as i16;
                        let pick = ((*note as i16 * 7 + *velocity as i16 * 13) % c).abs();
                        match self.mode {
                            RandomMode::Add => pick,
                            RandomMode::Sub => -pick,
                            RandomMode::Bipolar => pick - (c / 2),
                        }
                    } else {
                        0
                    };

                    self.active_shifts.push((*channel, *note, shift));
                    let new_note = (*note as i16 + shift).clamp(0, 127) as u8;
                    events_out.push(MidiEvent::NoteOn {
                        channel: *channel,
                        note: new_note,
                        velocity: *velocity,
                    });
                }
                MidiEvent::NoteOff { channel, note } => {
                    let shift = self.pop_shift(*channel, *note);
                    let new_note = (*note as i16 + shift).clamp(0, 127) as u8;
                    events_out.push(MidiEvent::NoteOff {
                        channel: *channel,
                        note: new_note,
                    });
                }
                _ => events_out.push(*ev),
            }
        }
    }
}
