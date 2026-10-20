//! Monophonic last-note priority voice stack and exponential portamento glide.
//!
//! Decompiled & reverse-engineered from Ableton Learning Synths (`src/engine/dsp-core.ts`, lines 220-280, 430-432).

/// MIDI note event in voice stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEvent {
    pub note: u8,
    pub velocity: u8,
}

/// Zero-allocation, fixed-capacity note stack (up to 16 notes) for monophonic last-note priority.
#[derive(Debug, Clone)]
pub struct NoteStack {
    notes: [Option<NoteEvent>; 16],
    len: usize,
}

impl NoteStack {
    pub const fn new() -> Self {
        Self {
            notes: [None; 16],
            len: 0,
        }
    }

    /// Push note to stack, deduplicating any existing entry.
    pub fn push(&mut self, note: u8, velocity: u8) {
        self.remove(note);
        if self.len < 16 {
            self.notes[self.len] = Some(NoteEvent { note, velocity });
            self.len += 1;
        }
    }

    /// Remove note from stack.
    pub fn remove(&mut self, note: u8) {
        let mut idx = None;
        for i in 0..self.len {
            if let Some(entry) = self.notes[i] {
                if entry.note == note {
                    idx = Some(i);
                    break;
                }
            }
        }
        if let Some(i) = idx {
            for j in i..(self.len - 1) {
                self.notes[j] = self.notes[j + 1];
            }
            self.notes[self.len - 1] = None;
            self.len -= 1;
        }
    }

    /// Peek the highest-priority (most recently played) active note.
    pub fn top(&self) -> Option<NoteEvent> {
        if self.len == 0 {
            None
        } else {
            self.notes[self.len - 1]
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.notes.fill(None);
    }
}

impl Default for NoteStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Action to be taken on envelope triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceTriggerAction {
    /// Gate was closed: triggers fresh attack on envelopes.
    TriggerAttack,
    /// Gate was already open: continues in legato mode without re-triggering attack.
    LegatoGlide,
}

/// Action to be taken on note releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceReleaseAction {
    /// Notes remain on the stack: pitch slides to new note, gate stays open.
    LegatoRestore(u8),
    /// Stack is empty: gate closes, triggers release on envelopes.
    TriggerRelease,
}

/// Monophonic voice manager handling last-note priority, legato gate persistence,
/// and exponential one-pole portamento glide.
#[derive(Debug, Clone)]
pub struct MonophonicVoice {
    pub note_stack: NoteStack,
    pub current_midi_pitch: f32,
    pub target_midi_pitch: f32,
    pub has_played_note: bool,
    pub gate_open: bool,
    pub current_velocity: u8,
}

impl MonophonicVoice {
    pub fn new() -> Self {
        Self {
            note_stack: NoteStack::new(),
            current_midi_pitch: 60.0,
            target_midi_pitch: 60.0,
            has_played_note: false,
            gate_open: false,
            current_velocity: 0,
        }
    }

    pub fn reset(&mut self) {
        self.note_stack.clear();
        self.current_midi_pitch = 60.0;
        self.target_midi_pitch = 60.0;
        self.has_played_note = false;
        self.gate_open = false;
        self.current_velocity = 0;
    }

    /// Process a NoteOn event.
    ///
    /// - `note`: MIDI pitch [0, 127]
    /// - `velocity`: MIDI velocity [1, 127] (velocity 0 delegates to NoteOff)
    /// - `glide_ms`: Glide portamento time in ms
    pub fn note_on(&mut self, note: u8, velocity: u8, glide_ms: f32) -> VoiceTriggerAction {
        if velocity == 0 {
            match self.note_off(note) {
                VoiceReleaseAction::TriggerRelease => VoiceTriggerAction::TriggerAttack,
                VoiceReleaseAction::LegatoRestore(_) => VoiceTriggerAction::LegatoGlide,
            }
        } else {
            let was_gate_open = self.gate_open;
            self.note_stack.push(note, velocity);
            self.target_midi_pitch = note as f32;
            self.current_velocity = velocity;

            if !self.has_played_note || !was_gate_open {
                if !self.has_played_note || glide_ms <= 0.5 {
                    self.current_midi_pitch = note as f32;
                }
                self.has_played_note = true;
            }

            self.gate_open = true;

            if was_gate_open {
                VoiceTriggerAction::LegatoGlide
            } else {
                VoiceTriggerAction::TriggerAttack
            }
        }
    }

    /// Process a NoteOff event.
    ///
    /// If stack still has notes, target pitch updates to the top of the stack and gate stays open.
    /// If stack becomes empty, gate closes.
    pub fn note_off(&mut self, note: u8) -> VoiceReleaseAction {
        self.note_stack.remove(note);
        if let Some(top) = self.note_stack.top() {
            self.target_midi_pitch = top.note as f32;
            self.current_velocity = top.velocity;
            self.gate_open = true;
            VoiceReleaseAction::LegatoRestore(top.note)
        } else {
            self.gate_open = false;
            VoiceReleaseAction::TriggerRelease
        }
    }

    pub fn all_notes_off(&mut self) {
        self.note_stack.clear();
        self.gate_open = false;
    }

    /// Advance one-pole exponential glide smoothing toward target pitch.
    #[inline(always)]
    pub fn advance_glide(&mut self, glide_ms: f32, sample_rate: f32) {
        if sample_rate <= 0.0 {
            return;
        }
        let glide_coeff = if glide_ms <= 0.5 {
            1.0
        } else {
            1.0 - (-1.0 / (glide_ms * 0.001 * sample_rate)).exp()
        };
        self.current_midi_pitch += (self.target_midi_pitch - self.current_midi_pitch) * glide_coeff;
    }

    pub fn get_active_note(&self) -> Option<u8> {
        self.note_stack.top().map(|n| n.note)
    }

    pub fn is_gate_open(&self) -> bool {
        self.gate_open
    }
}

impl Default for MonophonicVoice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_stack_priority() {
        let mut stack = NoteStack::new();
        assert!(stack.is_empty());

        stack.push(60, 100);
        assert_eq!(stack.top().unwrap().note, 60);

        stack.push(64, 100);
        assert_eq!(stack.top().unwrap().note, 64);

        stack.push(67, 100);
        assert_eq!(stack.top().unwrap().note, 67);

        // Remove 64 (middle)
        stack.remove(64);
        assert_eq!(stack.top().unwrap().note, 67);
        assert_eq!(stack.len(), 2);

        // Remove 67 (top) -> restores 60
        stack.remove(67);
        assert_eq!(stack.top().unwrap().note, 60);

        // Remove 60 -> empty
        stack.remove(60);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_monophonic_legato_voice() {
        let mut voice = MonophonicVoice::new();

        // 1. NoteOn 60
        let a1 = voice.note_on(60, 100, 10.0);
        assert_eq!(a1, VoiceTriggerAction::TriggerAttack);
        assert_eq!(voice.get_active_note(), Some(60));
        assert!(voice.is_gate_open());

        // 2. NoteOn 64 while 60 is held (legato)
        let a2 = voice.note_on(64, 100, 10.0);
        assert_eq!(a2, VoiceTriggerAction::LegatoGlide);
        assert_eq!(voice.get_active_note(), Some(64));
        assert!(voice.is_gate_open());

        // 3. Release 64 -> restores 60 without closing gate
        let r1 = voice.note_off(64);
        assert_eq!(r1, VoiceReleaseAction::LegatoRestore(60));
        assert_eq!(voice.get_active_note(), Some(60));
        assert!(voice.is_gate_open());

        // 4. Release 60 -> closes gate
        let r2 = voice.note_off(60);
        assert_eq!(r2, VoiceReleaseAction::TriggerRelease);
        assert_eq!(voice.get_active_note(), None);
        assert!(!voice.is_gate_open());
    }
}
