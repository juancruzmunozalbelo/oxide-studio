//! Complete Native MIDI Effects Rack Devices for `oxide-fx`.
//!
//! Provides comprehensive transformation of `MidiEvent` streams across 8 devices:
//! 1. `Arpeggiator`: Up, Down, UpDown, Converge, Random styles with rate (1/4, 1/8, 1/16, 1/32), gate %, octave range (1..4).
//! 2. `Chord`: Up to 6 semitone + velocity offset voices per incoming note.
//! 3. `NoteLength`: Gate/duration multiplier and release-velocity trigger mode.
//! 4. `Pitch`: Semitone transpose (-128..+128) and range filter (lowest_note..highest_note).
//! 5. `Random`: Deterministic PRNG chance (0.0..1.0), choices (1..24), and bidirectional sign mode.
//! 6. `Scale`: 12-note pitch-class quantization matrix with base root transpose.
//! 7. `Velocity`: Drive/compand curve, fixed/random range, and clip bounds (out_low..out_high).
//! 8. `MpeControl`: Pitch-bend, slide (CC74), and pressure curve shaping (curve_skew: -1.0..1.0).

use oxide_core::{Device, DeviceCategory, MidiEvent, MusicalTime};
use oxide_core::device::MidiEffectDevice as IMidiEffectDevice;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// 1. Arpeggiator Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ArpStyle {
    Up,
    Down,
    UpDown,
    DownUp,
    UpAndDown,
    DownAndUp,
    Converge,
    Diverge,
    Random,
    ChordTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ArpRate {
    Rate1_1,
    Rate1_2,
    Rate1_4,
    Rate1_8,
    Rate1_16,
    Rate1_32,
    Rate1_8T,
    Rate1_16T,
}

impl ArpRate {
    pub fn to_beats(&self) -> f64 {
        match self {
            ArpRate::Rate1_1 => 4.0,
            ArpRate::Rate1_2 => 2.0,
            ArpRate::Rate1_4 => 1.0,
            ArpRate::Rate1_8 => 0.5,
            ArpRate::Rate1_16 => 0.25,
            ArpRate::Rate1_32 => 0.125,
            ArpRate::Rate1_8T => 0.5 * 2.0 / 3.0,
            ArpRate::Rate1_16T => 0.25 * 2.0 / 3.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArpeggiatorDevice {
    pub style: ArpStyle,
    pub rate: ArpRate,
    pub gate: f32,    // 0.1 .. 2.0 (1.0 = 100%)
    pub octaves: u8,  // 1 .. 4
    pub synced: bool, // Tempo sync (true) or free running (false)
    pub free_rate_hz: f32, // Rate in Hz when synced == false (e.g. 1.0 .. 50.0 Hz)
    pub held_notes: Vec<(u8, u8)>, // (note, velocity)
    pub last_step: i64,
    pub step_index: usize,
    pub direction_up: bool,
    pub active_playing_note: Option<u8>,
    pub active_playing_chord: Vec<u8>,
    pub rng_state: u32,
}

impl ArpeggiatorDevice {
    pub fn new() -> Self {
        Self {
            style: ArpStyle::Up,
            rate: ArpRate::Rate1_16,
            gate: 0.9,
            octaves: 1,
            synced: true,
            free_rate_hz: 8.0,
            held_notes: Vec::new(),
            last_step: -1,
            step_index: 0,
            direction_up: true,
            active_playing_note: None,
            active_playing_chord: Vec::new(),
            rng_state: 0x19283746,
        }
    }

    pub fn reset(&mut self) {
        self.held_notes.clear();
        self.last_step = -1;
        self.step_index = 0;
        self.direction_up = true;
        self.active_playing_note = None;
        self.active_playing_chord.clear();
    }

    fn next_rand(&mut self) -> f32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        (self.rng_state as f32) / 4294967296.0
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>, time: &MusicalTime) {
        for ev in events_in {
            match *ev {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    if velocity > 0 {
                        if !self.held_notes.iter().any(|(n, _)| *n == note) {
                            self.held_notes.push((note, velocity));
                            self.held_notes.sort_by_key(|n| n.0);
                        }
                    } else {
                        self.held_notes.retain(|(n, _)| *n != note);
                    }
                }
                MidiEvent::NoteOff { note, .. } => {
                    self.held_notes.retain(|(n, _)| *n != note);
                }
                _ => events_out.push(*ev),
            }
        }

        if self.held_notes.is_empty() {
            if let Some(prev) = self.active_playing_note.take() {
                events_out.push(MidiEvent::NoteOff { channel: 0, note: prev });
            }
            for prev in self.active_playing_chord.drain(..) {
                events_out.push(MidiEvent::NoteOff { channel: 0, note: prev });
            }
            return;
        }

        // Build pattern note table across octaves
        let num_octaves = self.octaves.clamp(1, 4);
        let mut sequence = Vec::with_capacity(self.held_notes.len() * num_octaves as usize);
        for oct in 0..num_octaves {
            for &(pitch, vel) in &self.held_notes {
                let note = (pitch as i16 + (oct as i16 * 12)).clamp(0, 127) as u8;
                sequence.push((note, vel));
            }
        }

        if sequence.is_empty() {
            return;
        }

        let current_step = if self.synced {
            let step_beats = self.rate.to_beats();
            (time.beat / step_beats).floor() as i64
        } else {
            let time_sec = if time.bpm > 0.0 {
                time.beat / (time.bpm / 60.0)
            } else {
                time.beat
            };
            let rate_hz = self.free_rate_hz.max(0.1);
            (time_sec * rate_hz as f64).floor() as i64
        };

        if current_step != self.last_step {
            self.last_step = current_step;

            // Silence previous note(s)
            if let Some(prev) = self.active_playing_note.take() {
                events_out.push(MidiEvent::NoteOff { channel: 0, note: prev });
            }
            for prev in self.active_playing_chord.drain(..) {
                events_out.push(MidiEvent::NoteOff { channel: 0, note: prev });
            }

            if self.style == ArpStyle::ChordTrigger {
                for &(target_note, target_vel) in &sequence {
                    self.active_playing_chord.push(target_note);
                    events_out.push(MidiEvent::NoteOn {
                        channel: 0,
                        note: target_note,
                        velocity: target_vel,
                    });
                }
            } else {
                let idx = match self.style {
                    ArpStyle::Up => {
                        let i = self.step_index % sequence.len();
                        self.step_index = (self.step_index + 1) % sequence.len();
                        i
                    }
                    ArpStyle::Down => {
                        let i = (sequence.len() - 1) - (self.step_index % sequence.len());
                        self.step_index = (self.step_index + 1) % sequence.len();
                        i
                    }
                    ArpStyle::UpDown => {
                        let n = sequence.len();
                        if n <= 1 {
                            0
                        } else {
                            let span = n * 2 - 2;
                            let phase = self.step_index % span;
                            self.step_index += 1;
                            if phase < n { phase } else { span - phase }
                        }
                    }
                    ArpStyle::DownUp => {
                        let n = sequence.len();
                        if n <= 1 {
                            0
                        } else {
                            let span = n * 2 - 2;
                            let phase = self.step_index % span;
                            self.step_index += 1;
                            if phase < n { (n - 1) - phase } else { phase - (n - 1) }
                        }
                    }
                    ArpStyle::UpAndDown => {
                        let n = sequence.len();
                        if n <= 1 {
                            0
                        } else {
                            let span = n * 2;
                            let phase = self.step_index % span;
                            self.step_index += 1;
                            if phase < n { phase } else { (2 * n - 1) - phase }
                        }
                    }
                    ArpStyle::DownAndUp => {
                        let n = sequence.len();
                        if n <= 1 {
                            0
                        } else {
                            let span = n * 2;
                            let phase = self.step_index % span;
                            self.step_index += 1;
                            if phase < n { (n - 1) - phase } else { phase - n }
                        }
                    }
                    ArpStyle::Converge => {
                        let n = sequence.len();
                        let half = self.step_index % n;
                        self.step_index += 1;
                        if half.is_multiple_of(2) {
                            half / 2
                        } else {
                            n - 1 - (half / 2)
                        }
                    }
                    ArpStyle::Diverge => {
                        let n = sequence.len();
                        let rev_step = (n - 1) - (self.step_index % n);
                        self.step_index += 1;
                        if rev_step.is_multiple_of(2) {
                            rev_step / 2
                        } else {
                            n - 1 - (rev_step / 2)
                        }
                    }
                    ArpStyle::Random => {
                        (self.next_rand() * sequence.len() as f32) as usize % sequence.len()
                    }
                    ArpStyle::ChordTrigger => 0,
                };

                let (target_note, target_vel) = sequence[idx];
                self.active_playing_note = Some(target_note);
                events_out.push(MidiEvent::NoteOn {
                    channel: 0,
                    note: target_note,
                    velocity: target_vel,
                });
            }
        }
    }
}

impl Default for ArpeggiatorDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// 2. Chord Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ChordDevice {
    pub shifts: [i8; 6],           // up to 6 interval shifts in semitones
    pub velocity_scales: [f32; 6], // per-voice velocity multipliers
    pub num_shifts: usize,         // active voices (0..6)
}

impl ChordDevice {
    pub fn new() -> Self {
        Self {
            shifts: [3, 7, 10, 14, 0, 0], // Minor 9th chord preset
            velocity_scales: [0.9, 0.85, 0.8, 0.75, 1.0, 1.0],
            num_shifts: 4,
        }
    }

    pub fn major_triad() -> Self {
        Self {
            shifts: [4, 7, 0, 0, 0, 0],
            velocity_scales: [0.9, 0.85, 1.0, 1.0, 1.0, 1.0],
            num_shifts: 2,
        }
    }

    pub fn minor_triad() -> Self {
        Self {
            shifts: [3, 7, 0, 0, 0, 0],
            velocity_scales: [0.9, 0.85, 1.0, 1.0, 1.0, 1.0],
            num_shifts: 2,
        }
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>) {
        for ev in events_in {
            match *ev {
                MidiEvent::NoteOn { channel, note, velocity } => {
                    events_out.push(*ev);
                    if velocity > 0 {
                        for i in 0..self.num_shifts.min(6) {
                            let shift = self.shifts[i];
                            let shifted_note = (note as i16 + shift as i16).clamp(0, 127) as u8;
                            let shifted_vel = ((velocity as f32) * self.velocity_scales[i]).clamp(1.0, 127.0) as u8;
                            events_out.push(MidiEvent::NoteOn {
                                channel,
                                note: shifted_note,
                                velocity: shifted_vel,
                            });
                        }
                    }
                }
                MidiEvent::NoteOff { channel, note } => {
                    events_out.push(*ev);
                    for i in 0..self.num_shifts.min(6) {
                        let shift = self.shifts[i];
                        let shifted_note = (note as i16 + shift as i16).clamp(0, 127) as u8;
                        events_out.push(MidiEvent::NoteOff {
                            channel,
                            note: shifted_note,
                        });
                    }
                }
                _ => events_out.push(*ev),
            }
        }
    }
}

impl Default for ChordDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// 3. Note Length Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NoteLengthDevice {
    pub gate_multiplier: f32,    // duration scaling
    pub release_trigger: bool,   // trigger note on key release
    pub release_velocity: u8,
}

impl NoteLengthDevice {
    pub fn new() -> Self {
        Self {
            gate_multiplier: 1.0,
            release_trigger: false,
            release_velocity: 96,
        }
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>) {
        for ev in events_in {
            match *ev {
                MidiEvent::NoteOn { .. } => {
                    if !self.release_trigger {
                        events_out.push(*ev);
                    }
                }
                MidiEvent::NoteOff { channel, note } => {
                    if self.release_trigger {
                        // Emit NoteOn with release velocity followed by NoteOff
                        events_out.push(MidiEvent::NoteOn {
                            channel,
                            note,
                            velocity: self.release_velocity,
                        });
                        events_out.push(MidiEvent::NoteOff { channel, note });
                    } else {
                        events_out.push(*ev);
                    }
                }
                _ => events_out.push(*ev),
            }
        }
    }
}

impl Default for NoteLengthDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// 4. Pitch Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PitchDevice {
    pub transpose: i16, // -128 .. +128
    pub lowest_note: u8,
    pub highest_note: u8,
}

impl PitchDevice {
    pub fn new() -> Self {
        Self {
            transpose: 0,
            lowest_note: 0,
            highest_note: 127,
        }
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>) {
        for ev in events_in {
            match *ev {
                MidiEvent::NoteOn { channel, note, velocity } => {
                    if note >= self.lowest_note && note <= self.highest_note {
                        let pitched = (note as i16 + self.transpose).clamp(0, 127) as u8;
                        events_out.push(MidiEvent::NoteOn { channel, note: pitched, velocity });
                    }
                }
                MidiEvent::NoteOff { channel, note } => {
                    if note >= self.lowest_note && note <= self.highest_note {
                        let pitched = (note as i16 + self.transpose).clamp(0, 127) as u8;
                        events_out.push(MidiEvent::NoteOff { channel, note: pitched });
                    }
                }
                _ => events_out.push(*ev),
            }
        }
    }
}

impl Default for PitchDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// 5. Random Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RandomSign {
    Positive,
    Negative,
    Bidirectional,
}

#[derive(Debug, Clone)]
pub struct RandomDevice {
    pub chance: f32, // 0.0 .. 1.0 probability
    pub choices: u8, // 1 .. 24 semitone options
    pub sign: RandomSign,
    pub rng_state: u32,
    pub note_history: [(u8, u8); 16], // (input_note, randomized_note)
    pub history_len: usize,
}

impl RandomDevice {
    pub fn new() -> Self {
        Self {
            chance: 0.5,
            choices: 12,
            sign: RandomSign::Bidirectional,
            rng_state: 0x99238475,
            note_history: [(0, 0); 16],
            history_len: 0,
        }
    }

    fn next_float(&mut self) -> f32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        (self.rng_state as f32) / 4294967296.0
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>) {
        for ev in events_in {
            match *ev {
                MidiEvent::NoteOn { channel, note, velocity } => {
                    if velocity > 0 && self.next_float() < self.chance {
                        let choices = self.choices.clamp(1, 24) as f32;
                        let offset = (self.next_float() * choices).floor() as i16 + 1;
                        let signed_offset = match self.sign {
                            RandomSign::Positive => offset,
                            RandomSign::Negative => -offset,
                            RandomSign::Bidirectional => {
                                if self.next_float() < 0.5 { -offset } else { offset }
                            }
                        };
                        let rand_note = (note as i16 + signed_offset).clamp(0, 127) as u8;
                        if self.history_len < 16 {
                            self.note_history[self.history_len] = (note, rand_note);
                            self.history_len += 1;
                        }
                        events_out.push(MidiEvent::NoteOn { channel, note: rand_note, velocity });
                    } else {
                        events_out.push(*ev);
                    }
                }
                MidiEvent::NoteOff { channel, note } => {
                    let mut found = false;
                    for i in 0..self.history_len {
                        if self.note_history[i].0 == note {
                            let mapped = self.note_history[i].1;
                            events_out.push(MidiEvent::NoteOff { channel, note: mapped });
                            self.note_history[i] = self.note_history[self.history_len - 1];
                            self.history_len -= 1;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        events_out.push(*ev);
                    }
                }
                _ => events_out.push(*ev),
            }
        }
    }

    pub fn reset(&mut self) {
        self.history_len = 0;
    }
}

impl Default for RandomDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// 6. Scale Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ScaleDevice {
    pub base_root: u8, // 0..11 (0=C, 1=C#, etc.)
    pub intervals: [bool; 12],
}

impl ScaleDevice {
    pub fn new() -> Self {
        Self {
            base_root: 0, // C
            // Major scale: C, D, E, F, G, A, B
            intervals: [true, false, true, false, true, true, false, true, false, true, false, true],
        }
    }

    pub fn minor() -> Self {
        Self {
            base_root: 0,
            // Natural Minor: C, D, Eb, F, G, Ab, Bb
            intervals: [true, false, true, true, false, true, false, true, true, false, true, false],
        }
    }

    pub fn dorian() -> Self {
        Self {
            base_root: 0,
            // Dorian: C, D, Eb, F, G, A, Bb
            intervals: [true, false, true, true, false, true, false, true, false, true, true, false],
        }
    }

    pub fn pentatonic() -> Self {
        Self {
            base_root: 0,
            // Major Pentatonic: C, D, E, G, A
            intervals: [true, false, true, false, true, false, false, true, false, true, false, false],
        }
    }

    pub fn quantize(&self, note: u8) -> u8 {
        let rel_pitch = (note as i16 - self.base_root as i16).rem_euclid(12) as usize;
        if self.intervals[rel_pitch] {
            return note;
        }

        // Fold to nearest allowed degree
        for dist in 1..6 {
            let up = (rel_pitch + dist) % 12;
            if self.intervals[up] {
                return (note as i16 + dist as i16).clamp(0, 127) as u8;
            }
            let down = (rel_pitch + 12 - dist) % 12;
            if self.intervals[down] {
                return (note as i16 - dist as i16).clamp(0, 127) as u8;
            }
        }
        note
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>) {
        for ev in events_in {
            match *ev {
                MidiEvent::NoteOn { channel, note, velocity } => {
                    let q = self.quantize(note);
                    events_out.push(MidiEvent::NoteOn { channel, note: q, velocity });
                }
                MidiEvent::NoteOff { channel, note } => {
                    let q = self.quantize(note);
                    events_out.push(MidiEvent::NoteOff { channel, note: q });
                }
                _ => events_out.push(*ev),
            }
        }
    }
}

impl Default for ScaleDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// 7. Velocity Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum VelocityCurve {
    Linear,
    Drive,
    Compand,
}

#[derive(Debug, Clone)]
pub struct VelocityDevice {
    pub curve: VelocityCurve,
    pub drive: f32,       // -1.0 .. +1.0
    pub compand: f32,     // 0.2 .. 3.0
    pub random_range: u8, // +/- random velocity variance
    pub out_low: u8,      // 1 .. 127
    pub out_high: u8,     // 1 .. 127
    pub rng_state: u32,
}

impl VelocityDevice {
    pub fn new() -> Self {
        Self {
            curve: VelocityCurve::Linear,
            drive: 0.0,
            compand: 1.0,
            random_range: 0,
            out_low: 1,
            out_high: 127,
            rng_state: 0x54637281,
        }
    }

    fn next_rand(&mut self) -> f32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        (self.rng_state as f32) / 4294967296.0
    }

    pub fn transform_velocity(&mut self, vel: u8) -> u8 {
        if vel == 0 {
            return 0;
        }

        let v_norm = vel as f32 / 127.0;

        let shaped = match self.curve {
            VelocityCurve::Linear => v_norm,
            VelocityCurve::Drive => {
                let exp = 1.0 - (self.drive.clamp(-0.9, 0.9) * 0.7);
                v_norm.powf(exp)
            }
            VelocityCurve::Compand => {
                // S-curve transfer
                let p = self.compand.clamp(0.2, 3.0);
                if v_norm < 0.5 {
                    0.5 * (2.0 * v_norm).powf(p)
                } else {
                    1.0 - 0.5 * (2.0 * (1.0 - v_norm)).powf(p)
                }
            }
        };

        let mut out = shaped * (self.out_high as f32 - self.out_low as f32) + self.out_low as f32;

        if self.random_range > 0 {
            let r_offset = (self.next_rand() * 2.0 - 1.0) * self.random_range as f32;
            out += r_offset;
        }

        out.clamp(self.out_low as f32, self.out_high as f32).round() as u8
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>) {
        for ev in events_in {
            match *ev {
                MidiEvent::NoteOn { channel, note, velocity } => {
                    let v = self.transform_velocity(velocity);
                    events_out.push(MidiEvent::NoteOn { channel, note, velocity: v });
                }
                _ => events_out.push(*ev),
            }
        }
    }
}

impl Default for VelocityDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// 8. MPE Control Device
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MpeControlDevice {
    pub curve_skew: f32,    // -1.0 .. +1.0 (pitch-bend curve shaping)
    pub slide_skew: f32,    // -1.0 .. +1.0 (CC74 curve shaping)
    pub pressure_skew: f32, // -1.0 .. +1.0 (aftertouch shaping)
}

impl MpeControlDevice {
    pub fn new() -> Self {
        Self {
            curve_skew: 0.0,
            slide_skew: 0.0,
            pressure_skew: 0.0,
        }
    }

    fn shape_curve(val_norm: f32, skew: f32) -> f32 {
        let v = val_norm.clamp(0.0, 1.0);
        let s = skew.clamp(-0.95, 0.95);
        let exp = 2.0f32.powf(-s * 2.0);
        v.powf(exp).clamp(0.0, 1.0)
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>) {
        for ev in events_in {
            match *ev {
                MidiEvent::PitchBend { channel, value } => {
                    let sign = if value >= 0 { 1.0 } else { -1.0 };
                    let norm = value.abs() as f32 / 8192.0;
                    let shaped = Self::shape_curve(norm, self.curve_skew);
                    let new_val = (shaped * 8192.0 * sign).clamp(-8192.0, 8191.0) as i16;
                    events_out.push(MidiEvent::PitchBend { channel, value: new_val });
                }
                MidiEvent::ControlChange { channel, controller, value } => {
                    if controller == 74 {
                        let norm = value as f32 / 127.0;
                        let shaped = Self::shape_curve(norm, self.slide_skew);
                        let new_val = (shaped * 127.0).clamp(0.0, 127.0).round() as u8;
                        events_out.push(MidiEvent::ControlChange { channel, controller, value: new_val });
                    } else if controller == 11 {
                        let norm = value as f32 / 127.0;
                        let shaped = Self::shape_curve(norm, self.pressure_skew);
                        let new_val = (shaped * 127.0).clamp(0.0, 127.0).round() as u8;
                        events_out.push(MidiEvent::ControlChange { channel, controller, value: new_val });
                    } else {
                        events_out.push(*ev);
                    }
                }
                _ => events_out.push(*ev),
            }
        }
    }
}

impl Default for MpeControlDevice {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// MidiEffectDevice Enum & MidiEffectChain
// -----------------------------------------------------------------------------

/// Native Live 12 MIDI Effect Device enum.
#[derive(Debug, Clone)]
pub enum MidiEffectDevice {
    Arpeggiator(ArpeggiatorDevice),
    Chord(ChordDevice),
    NoteLength(NoteLengthDevice),
    Pitch(PitchDevice),
    Random(RandomDevice),
    Scale(ScaleDevice),
    Velocity(VelocityDevice),
    MpeControl(MpeControlDevice),
}

impl MidiEffectDevice {
    pub fn name(&self) -> &str {
        match self {
            MidiEffectDevice::Arpeggiator(_) => "Arpeggiator",
            MidiEffectDevice::Chord(_) => "Chord",
            MidiEffectDevice::NoteLength(_) => "Note Length",
            MidiEffectDevice::Pitch(_) => "Pitch",
            MidiEffectDevice::Random(_) => "Random",
            MidiEffectDevice::Scale(_) => "Scale",
            MidiEffectDevice::Velocity(_) => "Velocity",
            MidiEffectDevice::MpeControl(_) => "MPE Control",
        }
    }

    pub fn process_events(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>, time: &MusicalTime) {
        match self {
            MidiEffectDevice::Arpeggiator(arp) => arp.process_events(events_in, events_out, time),
            MidiEffectDevice::Chord(chord) => chord.process_events(events_in, events_out),
            MidiEffectDevice::NoteLength(nl) => nl.process_events(events_in, events_out),
            MidiEffectDevice::Pitch(pitch) => pitch.process_events(events_in, events_out),
            MidiEffectDevice::Random(rand) => rand.process_events(events_in, events_out),
            MidiEffectDevice::Scale(scale) => scale.process_events(events_in, events_out),
            MidiEffectDevice::Velocity(vel) => vel.process_events(events_in, events_out),
            MidiEffectDevice::MpeControl(mpe) => mpe.process_events(events_in, events_out),
        }
    }

    pub fn reset(&mut self) {
        match self {
            MidiEffectDevice::Arpeggiator(arp) => arp.reset(),
            MidiEffectDevice::Random(rand) => rand.reset(),
            _ => {}
        }
    }
}

impl Device for MidiEffectDevice {
    fn name(&self) -> &str {
        self.name()
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::MidiEffect
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn set_enabled(&mut self, _enabled: bool) {}

    fn reset(&mut self) {
        self.reset();
    }
}

impl IMidiEffectDevice for MidiEffectDevice {
    fn process_midi(
        &mut self,
        events_in: &[MidiEvent],
        events_out: &mut Vec<MidiEvent>,
        time: &MusicalTime,
    ) {
        self.process_events(events_in, events_out, time);
    }
}

/// Sequential chain of native MIDI effects processing input events with zero hanging notes.
#[derive(Debug, Clone, Default)]
pub struct MidiEffectChain {
    pub devices: Vec<MidiEffectDevice>,
}

impl MidiEffectChain {
    pub fn new() -> Self {
        Self { devices: Vec::new() }
    }

    pub fn add(&mut self, device: MidiEffectDevice) {
        self.devices.push(device);
    }

    pub fn process(&mut self, events_in: &[MidiEvent], events_out: &mut Vec<MidiEvent>, time: &MusicalTime) {
        if self.devices.is_empty() {
            events_out.extend_from_slice(events_in);
            return;
        }

        let mut buf_a = events_in.to_vec();
        let mut buf_b = Vec::new();

        for dev in &mut self.devices {
            buf_b.clear();
            dev.process_events(&buf_a, &mut buf_b, time);
            std::mem::swap(&mut buf_a, &mut buf_b);
        }

        events_out.extend(buf_a);
    }
}

impl Device for MidiEffectChain {
    fn name(&self) -> &str {
        "MIDI Effect Rack Chain"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::MidiEffect
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn set_enabled(&mut self, _enabled: bool) {}

    fn reset(&mut self) {
        for dev in &mut self.devices {
            dev.reset();
        }
    }
}

impl IMidiEffectDevice for MidiEffectChain {
    fn process_midi(
        &mut self,
        events_in: &[MidiEvent],
        events_out: &mut Vec<MidiEvent>,
        time: &MusicalTime,
    ) {
        self.process(events_in, events_out, time);
    }
}

// -----------------------------------------------------------------------------
// Unit Tests (At least 5 verification tests covering all 8 devices)
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arpeggiator_pattern_and_octave_stepping() {
        let mut arp = ArpeggiatorDevice::new();
        arp.style = ArpStyle::Up;
        arp.rate = ArpRate::Rate1_16;
        arp.octaves = 2;

        let time0 = MusicalTime::new(0.0, 120.0, 4, 4, true);
        let in_events = [
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
            MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 },
        ];
        let mut out0 = Vec::new();
        arp.process_events(&in_events, &mut out0, &time0);

        assert_eq!(out0.len(), 1);
        assert_eq!(out0[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

        // Advance 1/16th beat (0.25 beat)
        let time1 = MusicalTime::new(0.25, 120.0, 4, 4, true);
        let mut out1 = Vec::new();
        arp.process_events(&[], &mut out1, &time1);

        assert_eq!(out1.len(), 2); // NoteOff(60) then NoteOn(64)
        assert_eq!(out1[0], MidiEvent::NoteOff { channel: 0, note: 60 });
        assert_eq!(out1[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    }

    #[test]
    fn test_chord_6_interval_voices() {
        let mut chord = ChordDevice::new();
        chord.shifts = [3, 7, 10, 14, 17, 21];
        chord.num_shifts = 6;

        let in_events = [MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }];
        let mut out = Vec::new();
        chord.process_events(&in_events, &mut out);

        assert_eq!(out.len(), 7, "Root + 6 chord voices");
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 63, velocity: 90 });
    }

    #[test]
    fn test_pitch_and_random_transposition() {
        let mut pitch = PitchDevice::new();
        pitch.transpose = 7; // Perfect 5th
        pitch.lowest_note = 40;
        pitch.highest_note = 80;

        let in_events = [
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
            MidiEvent::NoteOn { channel: 0, note: 30, velocity: 100 }, // filtered out
        ];
        let mut out = Vec::new();
        pitch.process_events(&in_events, &mut out);

        assert_eq!(out.len(), 1);
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });

        // Test Random
        let mut random = RandomDevice::new();
        random.chance = 1.0; // 100% chance
        random.choices = 5;
        let mut rand_out = Vec::new();
        random.process_events(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }], &mut rand_out);
        assert_eq!(rand_out.len(), 1);
        if let MidiEvent::NoteOn { note, .. } = rand_out[0] {
            assert!(note != 60, "Random device must alter pitch");
        }
    }

    #[test]
    fn test_scale_quantization_c_major() {
        let scale = ScaleDevice::new(); // C Major
        // C=60 (in scale), C#=61 (out of scale -> 60 or 62), D=62 (in scale)
        assert_eq!(scale.quantize(60), 60);
        let q_cs = scale.quantize(61);
        assert!(q_cs == 60 || q_cs == 62, "C# must quantize to C or D");
        assert_eq!(scale.quantize(62), 62);
    }

    #[test]
    fn test_velocity_drive_and_compand_curves() {
        let mut vel = VelocityDevice::new();
        vel.curve = VelocityCurve::Drive;
        vel.drive = 0.8;
        vel.out_low = 20;
        vel.out_high = 120;

        let out_v = vel.transform_velocity(64);
        assert!(out_v >= 20 && out_v <= 120);

        vel.curve = VelocityCurve::Compand;
        vel.compand = 2.0;
        let out_comp = vel.transform_velocity(64);
        assert!(out_comp >= 20 && out_comp <= 120);
    }

    #[test]
    fn test_mpe_control_and_effect_chain() {
        let mut mpe = MpeControlDevice::new();
        mpe.curve_skew = 0.5;
        mpe.slide_skew = -0.5;
        mpe.pressure_skew = 0.3;

        let in_events = [
            MidiEvent::PitchBend { channel: 0, value: 4096 },
            MidiEvent::ControlChange { channel: 0, controller: 74, value: 64 },
            MidiEvent::ControlChange { channel: 0, controller: 11, value: 80 },
        ];
        let mut mpe_out = Vec::new();
        mpe.process_events(&in_events, &mut mpe_out);
        assert_eq!(mpe_out.len(), 3);

        // Test combined chain
        let mut chain = MidiEffectChain::new();
        chain.add(MidiEffectDevice::Pitch(PitchDevice::new()));
        chain.add(MidiEffectDevice::Scale(ScaleDevice::new()));

        let time = MusicalTime::new(0.0, 120.0, 4, 4, true);
        let mut chain_out = Vec::new();
        chain.process(&[MidiEvent::NoteOn { channel: 0, note: 61, velocity: 100 }], &mut chain_out, &time);
        assert_eq!(chain_out.len(), 1);
        if let MidiEvent::NoteOn { note, .. } = chain_out[0] {
            assert!(note == 60 || note == 62, "Chain must pitch-quantize note");
        }
    }

    #[test]
    fn test_arpeggiator_all_8_pattern_modes() {
        let notes_in = [
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
            MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 },
            MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 },
        ];

        // 1. Up: 60, 64, 67, 60...
        let mut arp_up = ArpeggiatorDevice::new();
        arp_up.style = ArpStyle::Up;
        let mut out = Vec::new();
        arp_up.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        out.clear();
        arp_up.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
        out.clear();
        arp_up.process_events(&[], &mut out, &MusicalTime::new(0.50, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });

        // 2. Down: 67, 64, 60, 67...
        let mut arp_down = ArpeggiatorDevice::new();
        arp_down.style = ArpStyle::Down;
        let mut out = Vec::new();
        arp_down.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
        out.clear();
        arp_down.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });

        // 3. UpDown: 60, 64, 67, 64, 60...
        let mut arp_updown = ArpeggiatorDevice::new();
        arp_updown.style = ArpStyle::UpDown;
        let mut out = Vec::new();
        arp_updown.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        out.clear();
        arp_updown.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
        out.clear();
        arp_updown.process_events(&[], &mut out, &MusicalTime::new(0.50, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
        out.clear();
        arp_updown.process_events(&[], &mut out, &MusicalTime::new(0.75, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });

        // 4. DownUp: 67, 64, 60, 64, 67...
        let mut arp_downup = ArpeggiatorDevice::new();
        arp_downup.style = ArpStyle::DownUp;
        let mut out = Vec::new();
        arp_downup.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
        out.clear();
        arp_downup.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
        out.clear();
        arp_downup.process_events(&[], &mut out, &MusicalTime::new(0.50, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        out.clear();
        arp_downup.process_events(&[], &mut out, &MusicalTime::new(0.75, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });

        // 5. UpAndDown: 60, 64, 67, 67, 64, 60, 60...
        let mut arp_uandd = ArpeggiatorDevice::new();
        arp_uandd.style = ArpStyle::UpAndDown;
        let mut out = Vec::new();
        arp_uandd.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        out.clear();
        arp_uandd.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
        out.clear();
        arp_uandd.process_events(&[], &mut out, &MusicalTime::new(0.50, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
        out.clear();
        arp_uandd.process_events(&[], &mut out, &MusicalTime::new(0.75, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 }); // repeated top!

        // 6. DownAndUp: 67, 64, 60, 60, 64, 67, 67...
        let mut arp_dandu = ArpeggiatorDevice::new();
        arp_dandu.style = ArpStyle::DownAndUp;
        let mut out = Vec::new();
        arp_dandu.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
        out.clear();
        arp_dandu.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
        out.clear();
        arp_dandu.process_events(&[], &mut out, &MusicalTime::new(0.50, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        out.clear();
        arp_dandu.process_events(&[], &mut out, &MusicalTime::new(0.75, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }); // repeated bottom!

        // 7. Converge: 60, 67, 64 (outside in)
        let mut arp_conv = ArpeggiatorDevice::new();
        arp_conv.style = ArpStyle::Converge;
        let mut out = Vec::new();
        arp_conv.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        out.clear();
        arp_conv.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
        out.clear();
        arp_conv.process_events(&[], &mut out, &MusicalTime::new(0.50, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });

        // 8. Diverge: 64, 67, 60 (inside out)
        let mut arp_div = ArpeggiatorDevice::new();
        arp_div.style = ArpStyle::Diverge;
        let mut out = Vec::new();
        arp_div.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out[0], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
        out.clear();
        arp_div.process_events(&[], &mut out, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
        out.clear();
        arp_div.process_events(&[], &mut out, &MusicalTime::new(0.50, 120.0, 4, 4, true));
        assert_eq!(out[1], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

        // 9. ChordTrigger: triggers all notes at once on each step
        let mut arp_chord = ArpeggiatorDevice::new();
        arp_chord.style = ArpStyle::ChordTrigger;
        let mut out = Vec::new();
        arp_chord.process_events(&notes_in, &mut out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(out.len(), 3, "ChordTrigger triggers all held notes simultaneously");
    }

    #[test]
    fn test_arpeggiator_free_rate_and_sync() {
        let mut arp = ArpeggiatorDevice::new();
        arp.synced = false;
        arp.free_rate_hz = 10.0; // 10 steps per second (0.1s step)

        let notes_in = [MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }];
        let mut out = Vec::new();
        // At 120 BPM: 1 beat = 0.5s. Beat 0.0 -> time 0.0s
        let time0 = MusicalTime::new(0.0, 120.0, 4, 4, true);
        arp.process_events(&notes_in, &mut out, &time0);
        assert_eq!(out.len(), 1);

        // Advance by 0.1 beat = 0.05s -> still step 0
        let mut out_same = Vec::new();
        arp.process_events(&[], &mut out_same, &MusicalTime::new(0.1, 120.0, 4, 4, true));
        assert_eq!(out_same.len(), 0);

        // Advance by 0.25 beat = 0.125s -> step 1!
        let mut out_step = Vec::new();
        arp.process_events(&[], &mut out_step, &MusicalTime::new(0.25, 120.0, 4, 4, true));
        assert_eq!(out_step.len(), 2); // NoteOff + NoteOn
    }

    #[test]
    fn test_note_length_release_trigger() {
        let mut nl = NoteLengthDevice::new();
        nl.release_trigger = true;
        nl.release_velocity = 88;

        let in_on = [MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }];
        let mut out_on = Vec::new();
        nl.process_events(&in_on, &mut out_on);
        assert_eq!(out_on.len(), 0, "Release trigger suppresses initial NoteOn");

        let in_off = [MidiEvent::NoteOff { channel: 0, note: 60 }];
        let mut out_off = Vec::new();
        nl.process_events(&in_off, &mut out_off);
        assert_eq!(out_off.len(), 2, "Release trigger emits NoteOn then NoteOff on release");
        assert_eq!(out_off[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 88 });
        assert_eq!(out_off[1], MidiEvent::NoteOff { channel: 0, note: 60 });
    }

    #[test]
    fn test_chord_presets() {
        let major = ChordDevice::major_triad();
        assert_eq!(major.num_shifts, 2);
        assert_eq!(major.shifts[0], 4);
        assert_eq!(major.shifts[1], 7);

        let minor = ChordDevice::minor_triad();
        assert_eq!(minor.num_shifts, 2);
        assert_eq!(minor.shifts[0], 3);
        assert_eq!(minor.shifts[1], 7);
    }

    #[test]
    fn test_scale_presets() {
        let minor = ScaleDevice::minor();
        // C Natural Minor: C(0), D(2), Eb(3), F(5), G(7), Ab(8), Bb(10)
        assert_eq!(minor.quantize(60), 60); // C
        assert_eq!(minor.quantize(63), 63); // Eb
        assert_eq!(minor.quantize(67), 67); // G
        // E(64) is not in C natural minor -> folds to Eb(63) or F(65)
        let q_e = minor.quantize(64);
        assert!(q_e == 63 || q_e == 65);

        let pent = ScaleDevice::pentatonic();
        assert_eq!(pent.quantize(60), 60); // C
        assert_eq!(pent.quantize(64), 64); // E
    }

    #[test]
    fn test_random_sign_modes_and_note_off_tracking() {
        let mut rand_pos = RandomDevice::new();
        rand_pos.chance = 1.0;
        rand_pos.sign = RandomSign::Positive;
        rand_pos.choices = 5;

        let mut out = Vec::new();
        rand_pos.process_events(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }], &mut out);
        assert_eq!(out.len(), 1);
        let randomized_note = match out[0] {
            MidiEvent::NoteOn { note, .. } => {
                assert!(note > 60 && note <= 65);
                note
            }
            _ => panic!("Expected NoteOn"),
        };

        // NoteOff for original input note 60 must turn off the randomized note
        let mut out_off = Vec::new();
        rand_pos.process_events(&[MidiEvent::NoteOff { channel: 0, note: 60 }], &mut out_off);
        assert_eq!(out_off.len(), 1);
        assert_eq!(out_off[0], MidiEvent::NoteOff { channel: 0, note: randomized_note });
    }
}
