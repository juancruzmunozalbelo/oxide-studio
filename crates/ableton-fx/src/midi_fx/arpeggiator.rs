//! Ableton Live 12 Arpeggiator MIDI Device.

use ableton_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpPattern {
    Up,
    Down,
    UpDown,
    DownUp,
    Converge,
    Diverge,
    Random,
    ChordTrigger,
}

pub struct Arpeggiator {
    pub name: String,
    pub enabled: bool,
    pub pattern: ArpPattern,
    pub rate_beats: f64, // e.g. 0.25 (1/16th)
    pub gate: f32,       // 0.1 .. 2.0
    pub octaves: u8,     // 1 .. 4
    pub velocity_decay: f32, // 0.0 .. 1.0

    held_notes: Vec<(u8, u8)>, // (pitch, velocity)
    last_step: i64,
    current_index: usize,
    up_direction: bool,
    active_playing_note: Option<u8>,
}

impl Arpeggiator {
    pub fn new() -> Self {
        Self {
            name: "Arpeggiator".to_string(),
            enabled: true,
            pattern: ArpPattern::Up,
            rate_beats: 0.25,
            gate: 0.9,
            octaves: 1,
            velocity_decay: 0.0,
            held_notes: Vec::new(),
            last_step: -1,
            current_index: 0,
            up_direction: true,
            active_playing_note: None,
        }
    }

    fn build_note_sequence(&self) -> Vec<(u8, u8)> {
        if self.held_notes.is_empty() {
            return Vec::new();
        }

        let mut sorted = self.held_notes.clone();
        sorted.sort_by_key(|n| n.0);

        let mut full_notes = Vec::new();
        let num_octaves = self.octaves.clamp(1, 4);

        for oct in 0..num_octaves {
            for (p, v) in &sorted {
                let shifted_p = (*p as u16 + (oct as u16 * 12)).min(127) as u8;
                let decay_mult = (1.0 - self.velocity_decay * (oct as f32 / num_octaves as f32)).max(0.1);
                let shifted_v = ((*v as f32 * decay_mult).clamp(1.0, 127.0)) as u8;
                full_notes.push((shifted_p, shifted_v));
            }
        }

        match self.pattern {
            ArpPattern::Up => full_notes,
            ArpPattern::Down => {
                full_notes.reverse();
                full_notes
            }
            ArpPattern::UpDown => {
                let mut rev = full_notes.clone();
                rev.reverse();
                if full_notes.len() > 1 {
                    full_notes.extend_from_slice(&rev[1..rev.len() - 1]);
                }
                full_notes
            }
            ArpPattern::DownUp => {
                let mut rev = full_notes.clone();
                rev.reverse();
                if rev.len() > 1 {
                    let up = full_notes.clone();
                    rev.extend_from_slice(&up[1..up.len() - 1]);
                }
                rev
            }
            ArpPattern::Converge => {
                let mut result = Vec::new();
                let mut l = 0;
                let mut r = full_notes.len().saturating_sub(1);
                while l <= r {
                    result.push(full_notes[l]);
                    if l != r {
                        result.push(full_notes[r]);
                    }
                    l += 1;
                    if r == 0 { break; }
                    r -= 1;
                }
                result
            }
            ArpPattern::Diverge => {
                let mut result = Vec::new();
                let mid = full_notes.len() / 2;
                let mut offset = 0;
                while mid + offset < full_notes.len() || (mid >= offset && mid - offset < full_notes.len()) {
                    if mid + offset < full_notes.len() {
                        result.push(full_notes[mid + offset]);
                    }
                    if offset > 0 && mid >= offset {
                        result.push(full_notes[mid - offset]);
                    }
                    offset += 1;
                    if offset > full_notes.len() { break; }
                }
                result
            }
            ArpPattern::Random => full_notes,
            ArpPattern::ChordTrigger => full_notes,
        }
    }
}

impl Default for Arpeggiator {
    fn default() -> Self {
        Self::new()
    }
}

impl Device for Arpeggiator {
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
        self.held_notes.clear();
        self.last_step = -1;
        self.current_index = 0;
        self.up_direction = true;
        self.active_playing_note = None;
    }
}

impl MidiEffectDevice for Arpeggiator {
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

        // Track incoming held notes
        for ev in events_in {
            match ev {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    if *velocity > 0 {
                        if !self.held_notes.iter().any(|(p, _)| p == note) {
                            self.held_notes.push((*note, *velocity));
                        }
                    } else {
                        self.held_notes.retain(|(p, _)| p != note);
                    }
                }
                MidiEvent::NoteOff { note, .. } => {
                    self.held_notes.retain(|(p, _)| p != note);
                }
                _ => events_out.push(*ev),
            }
        }

        if self.held_notes.is_empty() {
            if let Some(note) = self.active_playing_note.take() {
                events_out.push(MidiEvent::NoteOff { channel: 0, note });
            }
            return;
        }

        let seq = self.build_note_sequence();
        if seq.is_empty() {
            return;
        }

        let rate = self.rate_beats.max(0.0625);
        let step = (time.beat / rate).floor() as i64;

        if step != self.last_step {
            self.last_step = step;

            // Turn off previous note
            if let Some(note) = self.active_playing_note.take() {
                events_out.push(MidiEvent::NoteOff { channel: 0, note });
            }

            match self.pattern {
                ArpPattern::ChordTrigger => {
                    for (p, v) in &seq {
                        events_out.push(MidiEvent::NoteOn { channel: 0, note: *p, velocity: *v });
                    }
                }
                ArpPattern::Random => {
                    let idx = ((time.beat * 9876.5).fract().abs() * seq.len() as f64) as usize % seq.len();
                    let (p, v) = seq[idx];
                    events_out.push(MidiEvent::NoteOn { channel: 0, note: p, velocity: v });
                    self.active_playing_note = Some(p);
                }
                _ => {
                    let idx = self.current_index % seq.len();
                    let (p, v) = seq[idx];
                    events_out.push(MidiEvent::NoteOn { channel: 0, note: p, velocity: v });
                    self.active_playing_note = Some(p);
                    self.current_index = (self.current_index + 1) % seq.len();
                }
            }
        }
    }
}
