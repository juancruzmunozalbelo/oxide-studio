//! Session Clip Matrix: Grid of tracks (columns) x scenes (rows), quantized triggering, and slot states.

use serde::{Deserialize, Serialize};
use crate::{ClipEnvelopeCollection, ClipEnvelopeTarget};
use crate::pattern::ClipPattern;
use crate::quantization::{calculate_launch_beat, QuantizeMode, TriggerMode};
use crate::slot::SlotState;

/// An individual clip residing in a Session slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionClip {
    pub id: u32,
    pub name: String,
    pub color_hex: String,
    pub loop_enabled: bool,
    pub loop_start_beat: f64,
    pub loop_end_beat: f64,
    pub pattern: ClipPattern,
    #[serde(default)]
    pub trigger_mode: TriggerMode,
    #[serde(default)]
    pub envelopes: ClipEnvelopeCollection,
}

impl SessionClip {
    pub fn new(id: u32, name: impl Into<String>, pattern: ClipPattern) -> Self {
        let length = pattern.length_beats();
        Self {
            id,
            name: name.into(),
            color_hex: "#00e676".to_string(),
            loop_enabled: true,
            loop_start_beat: 0.0,
            loop_end_beat: length,
            pattern,
            trigger_mode: TriggerMode::Trigger,
            envelopes: ClipEnvelopeCollection::default(),
        }
    }

    /// Builder to configure trigger mode (Trigger, Gate, Toggle, Repeat).
    pub fn with_trigger_mode(mut self, trigger_mode: TriggerMode) -> Self {
        self.trigger_mode = trigger_mode;
        self
    }

    /// Effective loop length in beats.
    pub fn loop_length_beats(&self) -> f64 {
        let diff = self.loop_end_beat - self.loop_start_beat;
        if diff > 0.0 {
            diff
        } else {
            self.pattern.length_beats().max(4.0)
        }
    }

    /// Evaluate effective parameter value from clip envelopes.
    pub fn evaluate_envelope(
        &self,
        target: &ClipEnvelopeTarget,
        clip_beat: f64,
        base_val: f32,
    ) -> f32 {
        self.envelopes
            .evaluate_effective(target, clip_beat, self.loop_start_beat, self.loop_end_beat, base_val)
    }

    /// Evaluate 14-bit MIDI pitch bend (-8192 .. +8191).
    pub fn evaluate_pitch_bend(&self, clip_beat: f64) -> i16 {
        self.envelopes
            .evaluate_pitch_bend_14bit(clip_beat, self.loop_start_beat, self.loop_end_beat)
    }

    /// Evaluate pitch bend in semitones given a bend range (e.g. ±2.0 or ±12.0 st).
    pub fn evaluate_pitch_bend_semitones(&self, clip_beat: f64, bend_range_semitones: f32) -> f32 {
        self.envelopes.evaluate_pitch_bend_semitones(
            clip_beat,
            self.loop_start_beat,
            self.loop_end_beat,
            bend_range_semitones,
        )
    }

    /// Evaluate modulation wheel CC 1 (0 .. 127).
    pub fn evaluate_mod_wheel(&self, clip_beat: f64) -> u8 {
        self.envelopes
            .evaluate_mod_wheel_7bit(clip_beat, self.loop_start_beat, self.loop_end_beat)
    }

    /// Evaluate MIDI Continuous Controller (0 .. 127).
    pub fn evaluate_cc(&self, cc_num: u8, clip_beat: f64) -> u8 {
        self.envelopes
            .evaluate_cc_7bit(cc_num, clip_beat, self.loop_start_beat, self.loop_end_beat)
    }
}

/// An individual slot inside a track column that can host an optional clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSlot {
    pub clip: Option<SessionClip>,
    pub state: SlotState,
}

impl Default for SessionSlot {
    fn default() -> Self {
        Self {
            clip: None,
            state: SlotState::Empty,
        }
    }
}

impl SessionSlot {
    pub fn new(clip: Option<SessionClip>) -> Self {
        let state = if clip.is_some() {
            SlotState::Stopped
        } else {
            SlotState::Empty
        };
        Self { clip, state }
    }

    pub fn set_clip(&mut self, clip: Option<SessionClip>) {
        self.clip = clip;
        self.state = if self.clip.is_some() {
            SlotState::Stopped
        } else {
            SlotState::Empty
        };
    }
}

/// A track column in the Session Matrix containing a vertical stack of clip slots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionTrack {
    pub id: u32,
    pub name: String,
    pub slots: Vec<SessionSlot>,
}

impl SessionTrack {
    pub fn new(id: u32, name: impl Into<String>, slot_count: usize) -> Self {
        let mut slots = Vec::with_capacity(slot_count);
        for _ in 0..slot_count {
            slots.push(SessionSlot::default());
        }
        Self {
            id,
            name: name.into(),
            slots,
        }
    }

    /// Trigger a clip in this track's slot at `slot_idx` with quantization mode.
    ///
    /// Any other active or queued slot on this track will be queued to stop at `target_beat`,
    /// ensuring monophonic clip exclusivity per track.
    pub fn trigger_slot(&mut self, slot_idx: usize, current_beat: f64, mode: QuantizeMode) -> Option<f64> {
        if slot_idx >= self.slots.len() || self.slots[slot_idx].clip.is_none() {
            return None;
        }

        let target_beat = calculate_launch_beat(current_beat, mode);

        // Queue stop on all other currently playing or queued slots
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if i != slot_idx && (slot.state.is_playing() || slot.state.is_queued_launch()) {
                if mode == QuantizeMode::Immediate {
                    slot.state.stop_immediate();
                } else {
                    slot.state.queue_stop(target_beat);
                }
            }
        }

        // Queue launch or immediately launch target slot
        if mode == QuantizeMode::Immediate {
            self.slots[slot_idx].state.start_playing_immediate(current_beat);
        } else {
            self.slots[slot_idx].state.queue_launch(target_beat);
        }

        Some(target_beat)
    }

    /// Trigger a clip in this track's slot at `slot_idx` adhering to the specified `TriggerMode`.
    ///
    /// - `Trigger`: standard launch / restart.
    /// - `Gate`: starts playback; releasing via `release_gate_slot` stops playback.
    /// - `Toggle`: if playing or queued, clicking queues stop; if stopped, starts playback.
    /// - `Repeat`: restarts / retriggers playback at the target beat.
    pub fn trigger_slot_with_mode(
        &mut self,
        slot_idx: usize,
        current_beat: f64,
        mode: QuantizeMode,
        trigger_mode: TriggerMode,
    ) -> Option<f64> {
        if slot_idx >= self.slots.len() || self.slots[slot_idx].clip.is_none() {
            return None;
        }

        let is_active = self.slots[slot_idx].state.is_playing()
            || self.slots[slot_idx].state.is_queued_launch();

        match trigger_mode {
            TriggerMode::Toggle => {
                if is_active {
                    let target_beat = self.stop(current_beat, mode);
                    return Some(target_beat);
                }
            }
            TriggerMode::Gate | TriggerMode::Repeat | TriggerMode::Trigger => {}
        }

        self.trigger_slot(slot_idx, current_beat, mode)
    }

    /// Stop playback on a Gate-mode slot when the user or MIDI trigger releases.
    pub fn release_gate_slot(
        &mut self,
        slot_idx: usize,
        current_beat: f64,
        mode: QuantizeMode,
    ) -> Option<f64> {
        if slot_idx >= self.slots.len() {
            return None;
        }
        let is_active = self.slots[slot_idx].state.is_playing()
            || self.slots[slot_idx].state.is_queued_launch();
        if is_active {
            let target_beat = calculate_launch_beat(current_beat, mode);
            if mode == QuantizeMode::Immediate {
                self.slots[slot_idx].state.stop_immediate();
            } else {
                self.slots[slot_idx].state.queue_stop(target_beat);
            }
            Some(target_beat)
        } else {
            None
        }
    }

    /// Stop playback on this track at the next quantization boundary.
    pub fn stop(&mut self, current_beat: f64, mode: QuantizeMode) -> f64 {
        let target_beat = calculate_launch_beat(current_beat, mode);
        for slot in &mut self.slots {
            if slot.state.is_playing() || slot.state.is_queued_launch() {
                if mode == QuantizeMode::Immediate {
                    slot.state.stop_immediate();
                } else {
                    slot.state.queue_stop(target_beat);
                }
            }
        }
        target_beat
    }

    /// Immediately stop all slots on this track.
    pub fn stop_immediate(&mut self) {
        for slot in &mut self.slots {
            if !slot.state.is_empty() {
                slot.state.stop_immediate();
            }
        }
    }

    /// Advance transport time for all slots on this track.
    pub fn tick(&mut self, current_beat: f64) {
        for slot in &mut self.slots {
            let clip_len = slot.clip.as_ref().map(|c| c.loop_length_beats()).unwrap_or(4.0);
            slot.state.tick(current_beat, clip_len);
        }
    }

    /// Returns index and reference to the active playing or queued slot, if any.
    pub fn active_playing_slot(&self) -> Option<(usize, &SessionSlot)> {
        self.slots.iter().enumerate().find(|(_, s)| s.state.is_playing())
    }
}

/// A horizontal scene row across all tracks in the Session Matrix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: u32,
    pub name: String,
    pub bpm_override: Option<f64>,
}

impl Scene {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            bpm_override: None,
        }
    }
}

/// Complete Session Clip Matrix: Grid of tracks (columns) x scenes (rows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMatrix {
    pub tracks: Vec<SessionTrack>,
    pub scenes: Vec<Scene>,
}

impl SessionMatrix {
    /// Create a new session grid with `num_tracks` columns and `num_scenes` rows.
    pub fn new(num_tracks: usize, num_scenes: usize) -> Self {
        let mut tracks = Vec::with_capacity(num_tracks);
        for t in 0..num_tracks {
            tracks.push(SessionTrack::new(t as u32, format!("Track {}", t + 1), num_scenes));
        }

        let mut scenes = Vec::with_capacity(num_scenes);
        for s in 0..num_scenes {
            scenes.push(Scene::new(s as u32, format!("Scene {}", s + 1)));
        }

        Self { tracks, scenes }
    }

    /// Trigger an individual clip slot at (track_idx, scene_idx).
    pub fn trigger_clip(
        &mut self,
        track_idx: usize,
        scene_idx: usize,
        current_beat: f64,
        mode: QuantizeMode,
    ) -> Option<f64> {
        if let Some(track) = self.tracks.get_mut(track_idx) {
            track.trigger_slot(scene_idx, current_beat, mode)
        } else {
            None
        }
    }

    /// Trigger an individual clip slot at (track_idx, scene_idx) with trigger mode.
    pub fn trigger_clip_with_mode(
        &mut self,
        track_idx: usize,
        scene_idx: usize,
        current_beat: f64,
        mode: QuantizeMode,
        trigger_mode: TriggerMode,
    ) -> Option<f64> {
        if let Some(track) = self.tracks.get_mut(track_idx) {
            track.trigger_slot_with_mode(scene_idx, current_beat, mode, trigger_mode)
        } else {
            None
        }
    }

    /// Release a Gate-mode clip slot at (track_idx, scene_idx).
    pub fn release_gate_clip(
        &mut self,
        track_idx: usize,
        scene_idx: usize,
        current_beat: f64,
        mode: QuantizeMode,
    ) -> Option<f64> {
        if let Some(track) = self.tracks.get_mut(track_idx) {
            track.release_gate_slot(scene_idx, current_beat, mode)
        } else {
            None
        }
    }

    /// Stop playback on a given track.
    pub fn stop_track(&mut self, track_idx: usize, current_beat: f64, mode: QuantizeMode) -> Option<f64> {
        self.tracks.get_mut(track_idx).map(|t| t.stop(current_beat, mode))
    }

    /// Launch an entire scene row across all tracks.
    ///
    /// For every track that has a clip at `scene_idx`, that clip is queued or launched.
    pub fn trigger_scene(&mut self, scene_idx: usize, current_beat: f64, mode: QuantizeMode) -> f64 {
        let target_beat = calculate_launch_beat(current_beat, mode);
        for track in &mut self.tracks {
            if scene_idx < track.slots.len() && track.slots[scene_idx].clip.is_some() {
                track.trigger_slot(scene_idx, current_beat, mode);
            }
        }
        target_beat
    }

    /// Process transport tick across all tracks and slots.
    pub fn tick(&mut self, current_beat: f64) {
        for track in &mut self.tracks {
            track.tick(current_beat);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{MidiNote, MidiPattern};

    #[test]
    fn test_session_matrix_scene_launch() {
        let mut matrix = SessionMatrix::new(2, 4);

        // Put a clip in Track 0, Scene 0
        let mut m0 = MidiPattern::new(4.0);
        m0.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        matrix.tracks[0].slots[0].set_clip(Some(SessionClip::new(1, "Lead", ClipPattern::Midi(m0))));

        // Put a clip in Track 1, Scene 0
        let mut m1 = MidiPattern::new(4.0);
        m1.add_note(MidiNote::new(36, 120, 0.0, 0.5));
        matrix.tracks[1].slots[0].set_clip(Some(SessionClip::new(2, "Kick", ClipPattern::Midi(m1))));

        // Launch Scene 0 at current_beat = 1.0 with 1 Bar quantization
        let target = matrix.trigger_scene(0, 1.0, QuantizeMode::OneBar);
        assert_eq!(target, 4.0);

        // Verify both slots are queued for beat 4.0
        assert_eq!(matrix.tracks[0].slots[0].state, SlotState::QueuedLaunch(4.0));
        assert_eq!(matrix.tracks[1].slots[0].state, SlotState::QueuedLaunch(4.0));

        // Advance to 3.9: still queued
        matrix.tick(3.9);
        assert!(matrix.tracks[0].slots[0].state.is_queued_launch());

        // Advance to 4.0: both start playing
        matrix.tick(4.0);
        assert!(matrix.tracks[0].slots[0].state.is_playing());
        assert!(matrix.tracks[1].slots[0].state.is_playing());

        // Trigger scene 1 (empty) at beat 5.0 with immediate stop:
        // Track 0 still has slot 0 playing
        matrix.stop_track(0, 5.0, QuantizeMode::Immediate);
        assert!(matrix.tracks[0].slots[0].state.is_stopped());
        // Track 1 is still playing
        assert!(matrix.tracks[1].slots[0].state.is_playing());
    }

    #[test]
    fn test_session_matrix_trigger_modes_lifecycle() {
        let mut matrix = SessionMatrix::new(2, 4);

        // Track 0, Slot 0: Toggle mode clip
        let m0 = MidiPattern::new(4.0);
        let clip_toggle = SessionClip::new(1, "Toggle Clip", ClipPattern::Midi(m0))
            .with_trigger_mode(TriggerMode::Toggle);
        matrix.tracks[0].slots[0].set_clip(Some(clip_toggle));

        // Track 1, Slot 0: Gate mode clip
        let m1 = MidiPattern::new(4.0);
        let clip_gate = SessionClip::new(2, "Gate Clip", ClipPattern::Midi(m1))
            .with_trigger_mode(TriggerMode::Gate);
        matrix.tracks[1].slots[0].set_clip(Some(clip_gate));

        // 1. Toggle mode: First trigger queues launch
        let t1 = matrix.trigger_clip_with_mode(0, 0, 1.0, QuantizeMode::FourBars, TriggerMode::Toggle);
        assert_eq!(t1, Some(16.0), "4 Bars launch quantization schedules to beat 16.0");
        assert_eq!(matrix.tracks[0].slots[0].state, SlotState::QueuedLaunch(16.0));

        // Second trigger while queued/playing triggers stop in Toggle mode!
        let t2 = matrix.trigger_clip_with_mode(0, 0, 2.0, QuantizeMode::OneBar, TriggerMode::Toggle);
        assert_eq!(t2, Some(4.0));
        assert_eq!(matrix.tracks[0].slots[0].state, SlotState::QueuedStop(4.0));

        // Tick to beat 4.0: becomes stopped
        matrix.tick(4.0);
        assert_eq!(matrix.tracks[0].slots[0].state, SlotState::Stopped);

        // 2. Gate mode: trigger starts playback
        let g1 = matrix.trigger_clip_with_mode(1, 0, 4.0, QuantizeMode::Immediate, TriggerMode::Gate);
        assert_eq!(g1, Some(4.0));
        assert!(matrix.tracks[1].slots[0].state.is_playing());

        // Gate release stops playback
        let g2 = matrix.release_gate_clip(1, 0, 6.0, QuantizeMode::Immediate);
        assert_eq!(g2, Some(6.0));
        assert_eq!(matrix.tracks[1].slots[0].state, SlotState::Stopped);
    }

    #[test]
    fn test_session_matrix_trigger_and_repeat_modes_and_quantization() {
        let mut matrix = SessionMatrix::new(2, 4);

        let m0 = MidiPattern::new(4.0);
        let clip_trigger = SessionClip::new(1, "Trigger Clip", ClipPattern::Midi(m0))
            .with_trigger_mode(TriggerMode::Trigger);
        matrix.tracks[0].slots[0].set_clip(Some(clip_trigger));

        let m1 = MidiPattern::new(4.0);
        let clip_repeat = SessionClip::new(2, "Repeat Clip", ClipPattern::Midi(m1))
            .with_trigger_mode(TriggerMode::Repeat);
        matrix.tracks[1].slots[0].set_clip(Some(clip_repeat));

        // 1. Trigger mode: launches with 1 Bar quantization
        let t1 = matrix.trigger_clip_with_mode(0, 0, 1.5, QuantizeMode::OneBar, TriggerMode::Trigger);
        assert_eq!(t1, Some(4.0));
        assert_eq!(matrix.tracks[0].slots[0].state, SlotState::QueuedLaunch(4.0));

        // Tick to beat 4.0: starts playing
        matrix.tick(4.0);
        assert!(matrix.tracks[0].slots[0].state.is_playing());

        // Retriggering while playing re-queues launch at next 1 Bar boundary (beat 8.0)
        let t2 = matrix.trigger_clip_with_mode(0, 0, 4.5, QuantizeMode::OneBar, TriggerMode::Trigger);
        assert_eq!(t2, Some(8.0));
        assert_eq!(matrix.tracks[0].slots[0].state, SlotState::QueuedLaunch(8.0));

        // 2. Repeat mode: re-triggers at quantization rate
        let r1 = matrix.trigger_clip_with_mode(1, 0, 0.1, QuantizeMode::Quarter, TriggerMode::Repeat);
        assert_eq!(r1, Some(1.0));
        assert_eq!(matrix.tracks[1].slots[0].state, SlotState::QueuedLaunch(1.0));
        matrix.tick(1.0);
        assert!(matrix.tracks[1].slots[0].state.is_playing());

        let r2 = matrix.trigger_clip_with_mode(1, 0, 1.1, QuantizeMode::Quarter, TriggerMode::Repeat);
        assert_eq!(r2, Some(2.0));
        assert_eq!(matrix.tracks[1].slots[0].state, SlotState::QueuedLaunch(2.0));

        // 3. Verify all QuantizeMode intervals (None to 4 Bars)
        let modes = [
            (QuantizeMode::Immediate, 1.25, 1.25),
            (QuantizeMode::Sixteenth, 1.20, 1.25),
            (QuantizeMode::Eighth, 1.20, 1.50),
            (QuantizeMode::Quarter, 1.20, 2.00),
            (QuantizeMode::Half, 1.20, 2.00),
            (QuantizeMode::OneBar, 1.20, 4.00),
            (QuantizeMode::TwoBars, 1.20, 8.00),
            (QuantizeMode::FourBars, 1.20, 16.00),
        ];
        for (mode, cur, expected) in modes {
            assert_eq!(calculate_launch_beat(cur, mode), expected);
        }
    }
}
