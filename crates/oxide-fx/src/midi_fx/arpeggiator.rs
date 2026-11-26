//! Ableton Live 12 Arpeggiator MIDI Device.

use oxide_core::{Device, DeviceCategory, MidiEffectDevice, MidiEvent, MusicalTime};

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

    held_notes: Vec<(u8, u8, u8)>, // (channel, pitch, velocity)
    last_step: i64,
    current_index: usize,
    up_direction: bool,
    active_playing_note: Option<(u8, u8)>, // (channel, note)
    active_playing_chord: Vec<(u8, u8)>,
    sorted_scratch: Vec<(u8, u8, u8)>,
    full_scratch: Vec<(u8, u8, u8)>,
    seq_scratch: Vec<(u8, u8, u8)>,
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
            held_notes: Vec::with_capacity(16),
            last_step: -1,
            current_index: 0,
            up_direction: true,
            active_playing_note: None,
            active_playing_chord: Vec::with_capacity(64),
            sorted_scratch: Vec::with_capacity(16),
            full_scratch: Vec::with_capacity(64),
            seq_scratch: Vec::with_capacity(128),
        }
    }

    fn build_note_sequence(&mut self) {
        self.seq_scratch.clear();
        if self.held_notes.is_empty() {
            return;
        }

        self.sorted_scratch.clear();
        self.sorted_scratch.extend_from_slice(&self.held_notes);
        self.sorted_scratch.sort_by_key(|n| n.1);

        self.full_scratch.clear();
        let num_octaves = self.octaves.clamp(1, 4);

        for oct in 0..num_octaves {
            for &(ch, p, v) in &self.sorted_scratch {
                let shifted_p = (p as u16 + (oct as u16 * 12)).min(127) as u8;
                let decay_mult = (1.0 - self.velocity_decay * (oct as f32 / num_octaves as f32)).max(0.1);
                let shifted_v = ((v as f32 * decay_mult).clamp(1.0, 127.0)) as u8;
                self.full_scratch.push((ch, shifted_p, shifted_v));
            }
        }

        match self.pattern {
            ArpPattern::Up | ArpPattern::Random | ArpPattern::ChordTrigger => {
                self.seq_scratch.extend_from_slice(&self.full_scratch);
            }
            ArpPattern::Down => {
                self.seq_scratch.extend(self.full_scratch.iter().copied().rev());
            }
            ArpPattern::UpDown => {
                self.seq_scratch.extend_from_slice(&self.full_scratch);
                if self.full_scratch.len() > 2 {
                    self.seq_scratch
                        .extend(self.full_scratch[1..self.full_scratch.len() - 1].iter().copied().rev());
                }
            }
            ArpPattern::DownUp => {
                self.seq_scratch.extend(self.full_scratch.iter().copied().rev());
                if self.full_scratch.len() > 2 {
                    self.seq_scratch
                        .extend_from_slice(&self.full_scratch[1..self.full_scratch.len() - 1]);
                }
            }
            ArpPattern::Converge => {
                let mut l = 0;
                let mut r = self.full_scratch.len().saturating_sub(1);
                while l <= r {
                    self.seq_scratch.push(self.full_scratch[l]);
                    if l != r {
                        self.seq_scratch.push(self.full_scratch[r]);
                    }
                    l += 1;
                    if r == 0 { break; }
                    r -= 1;
                }
            }
            ArpPattern::Diverge => {
                let mid = self.full_scratch.len() / 2;
                let mut offset = 0;
                while mid + offset < self.full_scratch.len()
                    || (mid >= offset && mid - offset < self.full_scratch.len())
                {
                    if mid + offset < self.full_scratch.len() {
                        self.seq_scratch.push(self.full_scratch[mid + offset]);
                    }
                    if offset > 0 && mid >= offset {
                        self.seq_scratch.push(self.full_scratch[mid - offset]);
                    }
                    offset += 1;
                    if offset > self.full_scratch.len() { break; }
                }
            }
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
        self.active_playing_chord.clear();
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

        // Track incoming held notes with original MIDI channel
        for ev in events_in {
            match ev {
                MidiEvent::NoteOn { channel, note, velocity } => {
                    if *velocity > 0 {
                        if !self.held_notes.iter().any(|(ch, p, _)| ch == channel && p == note) {
                            self.held_notes.push((*channel, *note, *velocity));
                        }
                    } else {
                        self.held_notes.retain(|(ch, p, _)| !(ch == channel && p == note));
                    }
                }
                MidiEvent::NoteOff { channel, note } => {
                    self.held_notes.retain(|(ch, p, _)| !(ch == channel && p == note));
                }
                _ => events_out.push(*ev),
            }
        }

        if self.held_notes.is_empty() {
            if let Some((ch, note)) = self.active_playing_note.take() {
                events_out.push(MidiEvent::NoteOff { channel: ch, note });
            }
            for (ch, note) in self.active_playing_chord.drain(..) {
                events_out.push(MidiEvent::NoteOff { channel: ch, note });
            }
            return;
        }

        self.build_note_sequence();
        if self.seq_scratch.is_empty() {
            return;
        }

        let rate = self.rate_beats.max(0.0625);
        let step = (time.beat / rate).floor() as i64;

        if step != self.last_step {
            self.last_step = step;

            // Turn off previous note(s)
            if let Some((ch, note)) = self.active_playing_note.take() {
                events_out.push(MidiEvent::NoteOff { channel: ch, note });
            }
            for (ch, note) in self.active_playing_chord.drain(..) {
                events_out.push(MidiEvent::NoteOff { channel: ch, note });
            }

            let seq_len = self.seq_scratch.len();
            match self.pattern {
                ArpPattern::ChordTrigger => {
                    for &(ch, p, v) in &self.seq_scratch {
                        self.active_playing_chord.push((ch, p));
                        events_out.push(MidiEvent::NoteOn { channel: ch, note: p, velocity: v });
                    }
                }
                ArpPattern::Random => {
                    let idx = ((time.beat * 9876.5).fract().abs() * seq_len as f64) as usize % seq_len;
                    let (ch, p, v) = self.seq_scratch[idx];
                    events_out.push(MidiEvent::NoteOn { channel: ch, note: p, velocity: v });
                    self.active_playing_note = Some((ch, p));
                }
                _ => {
                    let idx = self.current_index % seq_len;
                    let (ch, p, v) = self.seq_scratch[idx];
                    events_out.push(MidiEvent::NoteOn { channel: ch, note: p, velocity: v });
                    self.active_playing_note = Some((ch, p));
                    self.current_index = (self.current_index + 1) % seq_len;
                }
            }
        }
    }
}
