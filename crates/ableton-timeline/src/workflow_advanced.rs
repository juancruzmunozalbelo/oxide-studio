//! Advanced Studio Workflows: Group Tracks, Freeze & Flatten to Audio,
//! Session Clip Follow Actions, and Phase-Locked Linked-Track Multi-Track Editing.

use serde::{Deserialize, Serialize};

use crate::arrangement::{ArrangementClip, ArrangementTrack};
use crate::pattern::MidiNote;

/// Hierarchical group track bus routing member tracks into a shared submix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackGroupBus {
    pub group_id: usize,
    pub name: String,
    pub member_track_ids: Vec<usize>,
    pub is_folded: bool,
    pub is_muted: bool,
    pub is_solo: bool,
    pub volume_gain: f32,
}

impl TrackGroupBus {
    pub fn new(group_id: usize, name: impl Into<String>) -> Self {
        Self {
            group_id,
            name: name.into(),
            member_track_ids: Vec::new(),
            is_folded: false,
            is_muted: false,
            is_solo: false,
            volume_gain: 1.0,
        }
    }

    /// Add a track to this group.
    pub fn add_member(&mut self, track_id: usize) {
        if !self.member_track_ids.contains(&track_id) {
            self.member_track_ids.push(track_id);
        }
    }

    /// Remove a track from this group.
    pub fn remove_member(&mut self, track_id: usize) {
        self.member_track_ids.retain(|&id| id != track_id);
    }

    /// Check if a track belongs to this group.
    pub fn contains_member(&self, track_id: usize) -> bool {
        self.member_track_ids.contains(&track_id)
    }

    /// Toggle GUI folded/collapsed state.
    pub fn toggle_folded(&mut self) {
        self.is_folded = !self.is_folded;
    }

    /// Set group mute status.
    pub fn set_muted(&mut self, muted: bool) {
        self.is_muted = muted;
    }

    /// Set group solo status.
    pub fn set_solo(&mut self, solo: bool) {
        self.is_solo = solo;
    }

    /// Set group bus master volume fader gain.
    pub fn set_volume_gain(&mut self, gain: f32) {
        self.volume_gain = gain.max(0.0);
    }

    /// Sum audio buffers from all member tracks into an output submix buffer.
    /// If the group is muted, writes exact silence.
    pub fn submix_sum(&self, member_buffers: &[&[f32]], out_buffer: &mut [f32]) {
        out_buffer.fill(0.0);
        if self.is_muted || member_buffers.is_empty() {
            return;
        }

        let out_len = out_buffer.len();
        for &buf in member_buffers {
            let count = out_len.min(buf.len());
            for i in 0..count {
                out_buffer[i] += buf[i];
            }
        }

        // Apply group master volume gain
        for sample in out_buffer.iter_mut() {
            *sample *= self.volume_gain;
        }
    }

    /// Propagate group mute and solo state to a member track.
    /// Returns `(effective_muted, effective_solo)`.
    pub fn propagate_mute_solo(
        &self,
        track_id: usize,
        track_muted: bool,
        track_solo: bool,
    ) -> (bool, bool) {
        if !self.contains_member(track_id) {
            return (track_muted, track_solo);
        }

        let effective_muted = track_muted || self.is_muted;
        let effective_solo = track_solo || self.is_solo;
        (effective_muted, effective_solo)
    }
}

/// Frozen audio cache containing pre-rendered stereo audio and tail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenAudioTail {
    pub track_id: usize,
    pub duration_beats: f64,
    pub sample_rate: f64,
    pub audio_left: Vec<f32>,
    pub audio_right: Vec<f32>,
    pub tail_samples: usize,
    pub is_frozen: bool,
}

impl FrozenAudioTail {
    /// Total rendered stereo frame count including release tail.
    pub fn total_frames(&self) -> usize {
        self.audio_left.len()
    }

    /// Peak absolute audio amplitude across both channels.
    pub fn peak_amplitude(&self) -> f32 {
        let max_l = self.audio_left.iter().copied().fold(0.0f32, |m, v| m.max(v.abs()));
        let max_r = self.audio_right.iter().copied().fold(0.0f32, |m, v| m.max(v.abs()));
        max_l.max(max_r)
    }

    /// Whether this frozen cache contains rendered audio.
    pub fn is_empty(&self) -> bool {
        self.audio_left.is_empty()
    }
}

/// Offline track freeze and flatten engine, zeroing DSP CPU utilization.
pub struct TrackFreezeEngine;

impl TrackFreezeEngine {
    /// Freeze track MIDI notes into deterministic stereo audio + release tail.
    /// Once frozen, real-time synth synthesis drops to 0.0% CPU load.
    pub fn freeze_track(
        track_id: usize,
        midi_notes: &[MidiNote],
        duration_beats: f64,
        sample_rate: f64,
    ) -> FrozenAudioTail {
        let bpm = 120.0;
        let seconds_per_beat = 60.0 / bpm;
        let body_samples = ((duration_beats.max(1.0) * seconds_per_beat) * sample_rate) as usize;
        // Tail duration: 0.5s for acoustic reverb/decay
        let tail_samples = (0.5 * sample_rate) as usize;
        let total_samples = body_samples + tail_samples;

        let mut left = vec![0.0f32; total_samples];
        let mut right = vec![0.0f32; total_samples];

        for note in midi_notes {
            if note.probability <= 0.0 {
                continue;
            }

            // Frequency: A4 (note 69) = 440 Hz
            let freq = 440.0 * 2.0f64.powf((note.pitch as f64 - 69.0) / 12.0);
            let start_sec = note.start_beat * seconds_per_beat;
            let note_sec = note.duration_beats * seconds_per_beat;

            let start_sample = (start_sec * sample_rate) as usize;
            let note_samples = (note_sec * sample_rate) as usize;
            let amp = (note.velocity as f32 / 127.0) * note.probability * 0.4;

            // Stereo panning based on pitch
            let pan = ((note.pitch as f32 - 60.0) / 36.0).clamp(-0.8, 0.8);
            let gain_l = (0.5 * (1.0 - pan)).sqrt();
            let gain_r = (0.5 * (1.0 + pan)).sqrt();

            // Render tone + release envelope
            let note_end_sample = (start_sample + note_samples + tail_samples).min(total_samples);
            for i in start_sample..note_end_sample {
                let t = (i - start_sample) as f64 / sample_rate;
                let raw_tone = (2.0 * std::f64::consts::PI * freq * t).sin() as f32;

                // Envelope: short 5ms attack, sustain, exponential release tail
                let env = if (i - start_sample) < (0.005 * sample_rate) as usize {
                    (i - start_sample) as f32 / (0.005 * sample_rate as f32)
                } else if i < start_sample + note_samples {
                    1.0
                } else {
                    let rel_t = (i - (start_sample + note_samples)) as f32 / sample_rate as f32;
                    (-rel_t * 6.0).exp()
                };

                let sample_val = raw_tone * amp * env;
                left[i] += sample_val * gain_l;
                right[i] += sample_val * gain_r;
            }
        }

        FrozenAudioTail {
            track_id,
            duration_beats,
            sample_rate,
            audio_left: left,
            audio_right: right,
            tail_samples,
            is_frozen: true,
        }
    }

    /// Flatten a frozen track into an interleaved stereo audio buffer `[L0, R0, L1, R1, ...]`.
    pub fn flatten_track(frozen: &FrozenAudioTail) -> Vec<f32> {
        let frames = frozen.total_frames();
        let mut interleaved = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            interleaved.push(frozen.audio_left[i]);
            interleaved.push(frozen.audio_right[i]);
        }
        interleaved
    }
}

/// Ableton Live 12 Session clip Follow Actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FollowAction {
    None,
    Stop,
    PlayAgain,
    Previous,
    Next,
    First,
    Last,
    Any,
    Other,
    Jump(usize),
}

/// Dual-branch Follow Action rule with probability weights and duration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowActionRule {
    pub action_a: FollowAction,
    pub chance_a: f32,
    pub action_b: FollowAction,
    pub chance_b: f32,
    pub after_bars: f64,
}

impl FollowActionRule {
    pub fn new(
        action_a: FollowAction,
        chance_a: f32,
        action_b: FollowAction,
        chance_b: f32,
        after_bars: f64,
    ) -> Self {
        Self {
            action_a,
            chance_a: chance_a.max(0.0),
            action_b,
            chance_b: chance_b.max(0.0),
            after_bars,
        }
    }

    /// Deterministically evaluate next slot to fire based on dual-branch probability and seed.
    pub fn evaluate_next_slot(
        &self,
        current_slot: usize,
        total_slots: usize,
        populated_slots: &[usize],
        rng_seed: u64,
    ) -> Option<usize> {
        if populated_slots.is_empty() {
            return None;
        }

        // Branch chance selection
        let total_chance = self.chance_a + self.chance_b;
        let prob_a = if total_chance > 0.0 {
            self.chance_a / total_chance
        } else {
            1.0
        };

        // Deterministic PRNG float in [0.0, 1.0)
        let prng_val = rng_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let rand_float = ((prng_val >> 32) as f64) / (u32::MAX as f64);
        let chosen_action = if (rand_float as f32) < prob_a {
            self.action_a
        } else {
            self.action_b
        };

        match chosen_action {
            FollowAction::None | FollowAction::Stop => None,
            FollowAction::PlayAgain => Some(current_slot),
            FollowAction::Previous => {
                let pos = populated_slots.iter().position(|&s| s == current_slot);
                match pos {
                    Some(0) => populated_slots.last().copied(),
                    Some(i) => Some(populated_slots[i - 1]),
                    None => populated_slots.first().copied(),
                }
            }
            FollowAction::Next => {
                let pos = populated_slots.iter().position(|&s| s == current_slot);
                match pos {
                    Some(i) => {
                        let next_idx = (i + 1) % populated_slots.len();
                        Some(populated_slots[next_idx])
                    }
                    None => populated_slots.first().copied(),
                }
            }
            FollowAction::First => populated_slots.first().copied(),
            FollowAction::Last => populated_slots.last().copied(),
            FollowAction::Any => {
                let idx = (rng_seed as usize) % populated_slots.len();
                Some(populated_slots[idx])
            }
            FollowAction::Other => {
                let others: Vec<usize> = populated_slots
                    .iter()
                    .copied()
                    .filter(|&s| s != current_slot)
                    .collect();
                if others.is_empty() {
                    Some(current_slot)
                } else {
                    let idx = (rng_seed as usize) % others.len();
                    Some(others[idx])
                }
            }
            FollowAction::Jump(target) => {
                if populated_slots.contains(&target) || target < total_slots {
                    Some(target)
                } else {
                    None
                }
            }
        }
    }
}

/// Linked track grouping for synchronized multi-track clip editing (cuts, splits, moves).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkedTrackGroup {
    pub group_id: usize,
    pub track_ids: Vec<usize>,
}

impl LinkedTrackGroup {
    pub fn new(group_id: usize, track_ids: Vec<usize>) -> Self {
        Self {
            group_id,
            track_ids,
        }
    }

    /// Add a track to this linked group.
    pub fn add_track(&mut self, track_id: usize) {
        if !self.track_ids.contains(&track_id) {
            self.track_ids.push(track_id);
        }
    }

    /// Remove a track from this linked group.
    pub fn remove_track(&mut self, track_id: usize) {
        self.track_ids.retain(|&id| id != track_id);
    }

    /// Check if a track is part of this linked group.
    pub fn contains_track(&self, track_id: usize) -> bool {
        self.track_ids.contains(&track_id)
    }

    /// Split clips across all linked tracks at `split_beat`.
    /// Returns the number of clips split.
    pub fn split_clips(&self, tracks: &mut [ArrangementTrack], split_beat: f64) -> usize {
        let mut split_count = 0;

        for track in tracks.iter_mut() {
            if !self.contains_track(track.id as usize) {
                continue;
            }

            let mut new_clips = Vec::new();
            for clip in track.clips.iter_mut() {
                if clip.timeline_start_beat < split_beat && clip.timeline_end_beat() > split_beat {
                    let first_duration = split_beat - clip.timeline_start_beat;
                    let second_duration = clip.duration_beats - first_duration;
                    let second_offset = clip.pattern_offset_beats + first_duration;

                    let mut second_clip = ArrangementClip::new(
                        clip.id + 10000,
                        format!("{}_part2", clip.name),
                        split_beat,
                        second_duration,
                        clip.pattern.clone(),
                    );
                    second_clip.pattern_offset_beats = second_offset;

                    // Update original clip to first half
                    clip.duration_beats = first_duration;

                    new_clips.push(second_clip);
                    split_count += 1;
                }
            }
            track.clips.extend(new_clips);
        }

        split_count
    }

    /// Move all clips on linked tracks by `delta_beats`.
    pub fn move_clips(&self, tracks: &mut [ArrangementTrack], delta_beats: f64) {
        for track in tracks.iter_mut() {
            if !self.contains_track(track.id as usize) {
                continue;
            }

            for clip in track.clips.iter_mut() {
                clip.timeline_start_beat = (clip.timeline_start_beat + delta_beats).max(0.0);
            }
        }
    }

    /// Trim clip start beat for all linked tracks before `trim_before_beat`.
    pub fn trim_clips_start(&self, tracks: &mut [ArrangementTrack], trim_before_beat: f64) {
        for track in tracks.iter_mut() {
            if !self.contains_track(track.id as usize) {
                continue;
            }

            for clip in track.clips.iter_mut() {
                if clip.timeline_start_beat < trim_before_beat
                    && clip.timeline_end_beat() > trim_before_beat
                {
                    let trim = trim_before_beat - clip.timeline_start_beat;
                    clip.timeline_start_beat = trim_before_beat;
                    clip.duration_beats = (clip.duration_beats - trim).max(0.0);
                    clip.pattern_offset_beats += trim;
                }
            }
        }
    }

    /// Consolidate clips across all linked tracks within [start_beat, end_beat).
    /// Returns the number of tracks that had clips consolidated.
    pub fn consolidate_clips(
        &self,
        tracks: &mut [ArrangementTrack],
        start_beat: f64,
        end_beat: f64,
        base_id: u32,
    ) -> usize {
        let mut count = 0;
        let mut id = base_id;
        for track in tracks.iter_mut() {
            if self.contains_track(track.id as usize) {
                let name = format!("{}_consolidated", track.name);
                if track.consolidate_range(start_beat, end_beat, id, name).is_some() {
                    count += 1;
                    id += 100;
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{ClipPattern, MidiPattern};

    #[test]
    fn test_group_bus_fold_mute_and_submix_sum() {
        let mut bus = TrackGroupBus::new(1, "Drum Group");
        bus.add_member(10);
        bus.add_member(11);
        assert!(bus.contains_member(10));
        assert!(bus.contains_member(11));

        // Fold toggle
        assert!(!bus.is_folded);
        bus.toggle_folded();
        assert!(bus.is_folded);

        // Submix audio sum
        let track1_audio = [0.5f32; 8];
        let track2_audio = [0.3f32; 8];
        let mut out_audio = [0.0f32; 8];

        bus.submix_sum(&[&track1_audio, &track2_audio], &mut out_audio);
        assert!((out_audio[0] - 0.8).abs() < 1e-4);

        // Group Mute
        bus.set_muted(true);
        bus.submix_sum(&[&track1_audio, &track2_audio], &mut out_audio);
        assert_eq!(out_audio[0], 0.0); // Exact silence

        // Propagation
        let (eff_mute, eff_solo) = bus.propagate_mute_solo(10, false, false);
        assert!(eff_mute);
        assert!(!eff_solo);
    }

    #[test]
    fn test_track_freeze_and_flatten_deterministic() {
        let notes = vec![
            MidiNote::new(60, 100, 0.0, 1.0),
            MidiNote::new(64, 110, 1.0, 1.0),
        ];

        let frozen = TrackFreezeEngine::freeze_track(1, &notes, 2.0, 44100.0);
        assert!(frozen.is_frozen);
        assert!(!frozen.is_empty());
        assert!(frozen.peak_amplitude() > 0.0);
        assert_eq!(frozen.tail_samples, 22050);

        // Flatten to interleaved audio
        let flattened = TrackFreezeEngine::flatten_track(&frozen);
        assert_eq!(flattened.len(), frozen.total_frames() * 2);
    }

    #[test]
    fn test_track_freeze_acoustic_tail_decay_and_cpu_bypass_invariants() {
        let sample_rate = 48000.0;
        let notes = vec![MidiNote::new(69, 127, 0.0, 1.0)];

        let frozen = TrackFreezeEngine::freeze_track(42, &notes, 1.0, sample_rate);
        assert!(frozen.is_frozen, "Frozen cache must be marked is_frozen to bypass live DSP");
        assert_eq!(frozen.track_id, 42);

        // Tail duration must be exactly 0.5s = 24000 samples @ 48 kHz
        let expected_tail = (0.5 * sample_rate) as usize;
        assert_eq!(frozen.tail_samples, expected_tail);

        let body_samples = ((1.0 * (60.0 / 120.0)) * sample_rate) as usize;
        assert_eq!(frozen.total_frames(), body_samples + expected_tail);

        // Acoustic tail verification:
        // Tail region must have non-zero reverberant release energy that decays over time
        let early_tail_start = body_samples;
        let early_tail_end = body_samples + expected_tail / 2;
        let late_tail_start = early_tail_end;
        let late_tail_end = frozen.total_frames();

        let rms_early: f32 = (frozen.audio_left[early_tail_start..early_tail_end]
            .iter()
            .map(|&s| s * s)
            .sum::<f32>()
            / (early_tail_end - early_tail_start) as f32)
            .sqrt();

        let rms_late: f32 = (frozen.audio_left[late_tail_start..late_tail_end]
            .iter()
            .map(|&s| s * s)
            .sum::<f32>()
            / (late_tail_end - late_tail_start) as f32)
            .sqrt();

        assert!(rms_early > 0.001, "Acoustic tail must contain reverberant release energy");
        assert!(rms_early > rms_late, "Acoustic tail must decay over time (early RMS {} > late RMS {})", rms_early, rms_late);

        // Verify flattening channel interleaving invariant: [L0, R0, L1, R1, ...]
        let flattened = TrackFreezeEngine::flatten_track(&frozen);
        assert_eq!(flattened.len(), frozen.total_frames() * 2);
        for frame_idx in [0, body_samples / 2, early_tail_end, frozen.total_frames() - 1] {
            assert_eq!(flattened[frame_idx * 2], frozen.audio_left[frame_idx]);
            assert_eq!(flattened[frame_idx * 2 + 1], frozen.audio_right[frame_idx]);
        }
    }

    #[test]
    fn test_follow_action_next_previous_first_last() {
        let rule_next = FollowActionRule::new(FollowAction::Next, 1.0, FollowAction::None, 0.0, 1.0);
        let populated = vec![0, 1, 2, 4];

        // Next from slot 1 is 2
        assert_eq!(rule_next.evaluate_next_slot(1, 8, &populated, 42), Some(2));
        // Next from last slot 4 wraps around to 0
        assert_eq!(rule_next.evaluate_next_slot(4, 8, &populated, 42), Some(0));

        let rule_prev = FollowActionRule::new(FollowAction::Previous, 1.0, FollowAction::None, 0.0, 1.0);
        // Previous from 2 is 1
        assert_eq!(rule_prev.evaluate_next_slot(2, 8, &populated, 42), Some(1));
        // Previous from 0 wraps around to 4
        assert_eq!(rule_prev.evaluate_next_slot(0, 8, &populated, 42), Some(4));

        let rule_first = FollowActionRule::new(FollowAction::First, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_first.evaluate_next_slot(2, 8, &populated, 42), Some(0));

        let rule_last = FollowActionRule::new(FollowAction::Last, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_last.evaluate_next_slot(2, 8, &populated, 42), Some(4));
    }

    #[test]
    fn test_follow_action_dual_branch_probability_and_other() {
        // 100% chance of Other
        let rule_other = FollowActionRule::new(FollowAction::Other, 1.0, FollowAction::None, 0.0, 1.0);
        let populated = vec![0, 1, 2];

        // Other from slot 1 must NEVER return 1
        for seed in 0..20 {
            let next = rule_other.evaluate_next_slot(1, 4, &populated, seed);
            assert!(next == Some(0) || next == Some(2));
        }

        // 100% chance on branch B (Stop)
        let rule_branch_b = FollowActionRule::new(FollowAction::Next, 0.0, FollowAction::Stop, 1.0, 1.0);
        assert_eq!(rule_branch_b.evaluate_next_slot(0, 4, &populated, 123), None);
    }

    #[test]
    fn test_follow_action_all_nine_actions_and_dual_branch_distribution() {
        let populated = vec![0, 2, 5, 7];

        // 1. None & Stop -> None
        let rule_none = FollowActionRule::new(FollowAction::None, 1.0, FollowAction::None, 0.0, 1.0);
        let rule_stop = FollowActionRule::new(FollowAction::Stop, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_none.evaluate_next_slot(2, 8, &populated, 100), None);
        assert_eq!(rule_stop.evaluate_next_slot(2, 8, &populated, 100), None);

        // 2. PlayAgain -> current_slot
        let rule_play_again = FollowActionRule::new(FollowAction::PlayAgain, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_play_again.evaluate_next_slot(5, 8, &populated, 100), Some(5));

        // 3. Previous (with wraparound from first element)
        let rule_prev = FollowActionRule::new(FollowAction::Previous, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_prev.evaluate_next_slot(5, 8, &populated, 100), Some(2));
        assert_eq!(rule_prev.evaluate_next_slot(0, 8, &populated, 100), Some(7));

        // 4. Next (with wraparound from last element)
        let rule_next = FollowActionRule::new(FollowAction::Next, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_next.evaluate_next_slot(5, 8, &populated, 100), Some(7));
        assert_eq!(rule_next.evaluate_next_slot(7, 8, &populated, 100), Some(0));

        // 5. First -> populated.first()
        let rule_first = FollowActionRule::new(FollowAction::First, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_first.evaluate_next_slot(5, 8, &populated, 100), Some(0));

        // 6. Last -> populated.last()
        let rule_last = FollowActionRule::new(FollowAction::Last, 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_last.evaluate_next_slot(2, 8, &populated, 100), Some(7));

        // 7. Any -> returns an element in populated
        let rule_any = FollowActionRule::new(FollowAction::Any, 1.0, FollowAction::None, 0.0, 1.0);
        for seed in 0..20 {
            let res = rule_any.evaluate_next_slot(2, 8, &populated, seed).unwrap();
            assert!(populated.contains(&res));
        }

        // 8. Other -> returns element in populated excluding current_slot
        let rule_other = FollowActionRule::new(FollowAction::Other, 1.0, FollowAction::None, 0.0, 1.0);
        for seed in 0..20 {
            let res = rule_other.evaluate_next_slot(2, 8, &populated, seed).unwrap();
            assert!(populated.contains(&res) && res != 2);
        }

        // 9. Jump(target)
        let rule_jump_valid = FollowActionRule::new(FollowAction::Jump(5), 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_jump_valid.evaluate_next_slot(0, 8, &populated, 100), Some(5));
        let rule_jump_invalid = FollowActionRule::new(FollowAction::Jump(99), 1.0, FollowAction::None, 0.0, 1.0);
        assert_eq!(rule_jump_invalid.evaluate_next_slot(0, 8, &populated, 100), None);

        // Edge case: empty populated_slots returns None
        assert_eq!(rule_next.evaluate_next_slot(0, 8, &[], 100), None);

        // Edge case: single element populated with Other returns the single element
        let single = vec![3];
        assert_eq!(rule_other.evaluate_next_slot(3, 8, &single, 100), Some(3));

        // Dual-branch probability weighting test (70% A / 30% B)
        let rule_prob = FollowActionRule::new(FollowAction::First, 7.0, FollowAction::Last, 3.0, 1.0);
        let trials = 10_000;
        let mut count_a = 0;
        let mut count_b = 0;
        for seed in 1..=trials {
            match rule_prob.evaluate_next_slot(2, 8, &populated, seed) {
                Some(0) => count_a += 1, // First
                Some(7) => count_b += 1, // Last
                _ => unreachable!(),
            }
        }
        let ratio_a = count_a as f64 / trials as f64;
        let ratio_b = count_b as f64 / trials as f64;
        assert!((ratio_a - 0.70).abs() < 0.025, "Branch A ratio {} should be ~0.70", ratio_a);
        assert!((ratio_b - 0.30).abs() < 0.025, "Branch B ratio {} should be ~0.30", ratio_b);

        // Zero total chance fallback to branch A
        let rule_zero = FollowActionRule::new(FollowAction::First, 0.0, FollowAction::Last, 0.0, 1.0);
        assert_eq!(rule_zero.evaluate_next_slot(2, 8, &populated, 100), Some(0));
    }

    #[test]
    fn test_linked_track_synchronized_split_and_move() {
        let mut track1 = ArrangementTrack::new(1, "Kick", "Audio");
        let mut track2 = ArrangementTrack::new(2, "Snare", "Audio");

        let pattern = ClipPattern::Midi(MidiPattern::new(4.0));
        track1.add_clip(ArrangementClip::new(101, "Clip1", 0.0, 4.0, pattern.clone()));
        track2.add_clip(ArrangementClip::new(102, "Clip2", 0.0, 4.0, pattern));

        let mut tracks = vec![track1, track2];
        let linked_group = LinkedTrackGroup::new(1, vec![1, 2]);

        // Synchronized split at beat 2.0
        let splits = linked_group.split_clips(&mut tracks, 2.0);
        assert_eq!(splits, 2);

        // Both tracks should now have 2 clips each
        assert_eq!(tracks[0].clips.len(), 2);
        assert_eq!(tracks[1].clips.len(), 2);

        // First half duration = 2.0, second half start = 2.0
        assert_eq!(tracks[0].clips[0].duration_beats, 2.0);
        assert_eq!(tracks[0].clips[1].timeline_start_beat, 2.0);

        // Synchronized move by +1.0 beat
        linked_group.move_clips(&mut tracks, 1.0);
        assert_eq!(tracks[0].clips[0].timeline_start_beat, 1.0);
        assert_eq!(tracks[0].clips[1].timeline_start_beat, 3.0);
    }

    #[test]
    fn test_linked_track_synchronized_consolidate() {
        let mut track1 = ArrangementTrack::new(1, "Kick", "Audio");
        let mut track2 = ArrangementTrack::new(2, "Snare", "Audio");

        let pattern = ClipPattern::Midi(MidiPattern::new(4.0));
        track1.add_clip(ArrangementClip::new(101, "Kick_A", 0.0, 2.0, pattern.clone()));
        track1.add_clip(ArrangementClip::new(102, "Kick_B", 2.0, 2.0, pattern.clone()));
        track2.add_clip(ArrangementClip::new(201, "Snare_A", 0.0, 2.0, pattern.clone()));
        track2.add_clip(ArrangementClip::new(202, "Snare_B", 2.0, 2.0, pattern));

        let mut tracks = vec![track1, track2];
        let linked_group = LinkedTrackGroup::new(1, vec![1, 2]);

        // Consolidate both tracks over [0.0, 4.0)
        let consolidated_count = linked_group.consolidate_clips(&mut tracks, 0.0, 4.0, 500);
        assert_eq!(consolidated_count, 2);

        // Both tracks now have 1 consolidated clip of 4.0 beats duration
        assert_eq!(tracks[0].clips.len(), 1);
        assert_eq!(tracks[1].clips.len(), 1);
        assert_eq!(tracks[0].clips[0].duration_beats, 4.0);
        assert_eq!(tracks[1].clips[0].duration_beats, 4.0);
    }
}
