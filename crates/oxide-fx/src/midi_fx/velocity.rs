//! Ableton Live 12 Velocity MIDI Device.

use oxide_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

pub struct Velocity {
    pub name: String,
    pub enabled: bool,
    pub drive: f32,       // -1.0 .. +1.0 (curvature)
    pub compand: f32,     // 0.0 .. 2.0 (scaling)
    pub random: u8,       // 0 .. 40 jitter
    pub min_velocity: u8, // 1 .. 127
    pub max_velocity: u8, // 1 .. 127
}

impl Velocity {
    pub fn new() -> Self {
        Self {
            name: "Velocity".to_string(),
            enabled: true,
            drive: 0.0,
            compand: 1.0,
            random: 0,
            min_velocity: 1,
            max_velocity: 127,
        }
    }

    pub fn remap(&self, vel: u8) -> u8 {
        let norm = (vel as f32) / 127.0;

        // Apply drive curve
        let curved = if self.drive > 0.0 {
            norm.powf(1.0 / (1.0 + self.drive * 2.0))
        } else if self.drive < 0.0 {
            norm.powf(1.0 - self.drive * 2.0)
        } else {
            norm
        };

        // Apply compand scaling
        let scaled = curved * self.compand;

        // Apply random jitter
        let jitter = if self.random > 0 {
            ((vel as f32 * 432.1).sin() * (self.random as f32 * 0.5)) as i16
        } else {
            0
        };

        let raw = (scaled * 127.0) as i16 + jitter;
        raw.clamp(self.min_velocity as i16, self.max_velocity as i16) as u8
    }
}

impl Default for Velocity {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for Velocity {
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

impl MidiEffectDevice for Velocity {
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
                        note: *note,
                        velocity: self.remap(*velocity),
                    });
                }
                _ => events_out.push(*ev),
            }
        }
    }
}
