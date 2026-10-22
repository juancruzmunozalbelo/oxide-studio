//! Multi-Lane Comping Take Lanes & Micro-Crossfade Splice Architecture for Ableton Live 12.
//!
//! Provides non-destructive multi-pass audio/MIDI recording lane management on Arrangement tracks,
//! seamless region selection comping into the main content lane, and automatic 4-10 ms equal-power
//! micro-crossfades at splice boundaries to eliminate DC clicks and phase pops.

use serde::{Deserialize, Serialize};
use crate::arrangement::ArrangementClip;
use std::f32::consts::PI;

/// Automatic equal-power micro-crossfade metadata at comp slice boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicroCrossfade {
    /// Duration of the crossfade transition in milliseconds (4.0 .. 10.0 ms).
    pub duration_ms: f32,
    /// Beat position where the crossfade occurs.
    pub beat_position: f64,
    /// Inbound clip ID.
    pub in_clip_id: u32,
    /// Outbound clip ID.
    pub out_clip_id: u32,
}

impl MicroCrossfade {
    pub fn new(duration_ms: f32, beat_position: f64, out_clip_id: u32, in_clip_id: u32) -> Self {
        Self {
            duration_ms: duration_ms.clamp(1.0, 50.0),
            beat_position,
            in_clip_id,
            out_clip_id,
        }
    }

    /// Calculate equal-power fade-in multiplier for sample index `n` out of `total_frames`.
    /// Formula: sin(pi * n / (2 * L))
    pub fn fade_in_gain(n: usize, total_frames: usize) -> f32 {
        if total_frames == 0 {
            return 1.0;
        }
        let frac = (n as f32 / total_frames as f32).clamp(0.0, 1.0);
        (0.5 * PI * frac).sin()
    }

    /// Calculate equal-power fade-out multiplier for sample index `n` out of `total_frames`.
    /// Formula: cos(pi * n / (2 * L))
    pub fn fade_out_gain(n: usize, total_frames: usize) -> f32 {
        if total_frames == 0 {
            return 0.0;
        }
        let frac = (n as f32 / total_frames as f32).clamp(0.0, 1.0);
        (0.5 * PI * frac).cos()
    }

    /// Calculate the number of audio frames corresponding to this crossfade duration at a given sample rate.
    pub fn frames_at_sample_rate(&self, sample_rate: f64) -> usize {
        ((self.duration_ms as f64 * sample_rate) / 1000.0).round().max(1.0) as usize
    }

    /// Apply equal-power crossfade between two overlapping audio slices in-place.
    /// `out_buffer` is faded out, `in_buffer` is faded in, and the summed result is written to `output`.
    pub fn apply_crossfade(
        &self,
        out_buffer: &[f32],
        in_buffer: &[f32],
        output: &mut [f32],
    ) {
        let len = out_buffer.len().min(in_buffer.len()).min(output.len());
        for i in 0..len {
            let fade_out = Self::fade_out_gain(i, len);
            let fade_in = Self::fade_in_gain(i, len);
            output[i] = out_buffer[i] * fade_out + in_buffer[i] * fade_in;
        }
    }
}

/// An individual take lane holding recorded or generated audio/MIDI phrase takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeLane {
    pub id: u32,
    pub name: String,
    pub is_muted: bool,
    pub is_auditioning: bool,
    pub color_rgb: [u8; 3],
    pub clips: Vec<ArrangementClip>,
}

impl TakeLane {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            is_muted: false,
            is_auditioning: false,
            color_rgb: [120, 150, 200],
            clips: Vec::new(),
        }
    }

    /// Add a clip to this take lane.
    pub fn add_clip(&mut self, clip: ArrangementClip) {
        self.clips.push(clip);
    }

    /// Find all clips overlapping the time range `[start_beat, end_beat)`.
    pub fn clips_in_range(&self, start_beat: f64, end_beat: f64) -> Vec<&ArrangementClip> {
        self.clips
            .iter()
            .filter(|c| c.timeline_start_beat < end_beat && c.timeline_end_beat() > start_beat)
            .collect()
    }

    /// Split any clip overlapping `split_beat` into two contiguous clips (Blade Tool).
    pub fn split_at_beat(&mut self, split_beat: f64, next_clip_id: &mut u32) -> bool {
        let mut split_occurred = false;
        let mut new_clips = Vec::new();
        for clip in self.clips.drain(..) {
            if clip.timeline_start_beat < split_beat && clip.timeline_end_beat() > split_beat {
                let orig_start = clip.timeline_start_beat;
                let orig_dur = clip.duration_beats;
                let left_dur = split_beat - orig_start;
                let right_dur = orig_dur - left_dur;

                let mut left_clip = clip.clone();
                left_clip.duration_beats = left_dur;

                let mut right_clip = clip;
                *next_clip_id += 1;
                right_clip.id = *next_clip_id;
                right_clip.timeline_start_beat = split_beat;
                right_clip.duration_beats = right_dur;
                right_clip.pattern_offset_beats += left_dur;

                new_clips.push(left_clip);
                new_clips.push(right_clip);
                split_occurred = true;
            } else {
                new_clips.push(clip);
            }
        }
        self.clips = new_clips;
        split_occurred
    }
}

/// Manages multiple take lanes, visibility state, and region comping operations for an Arrangement track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackCompingManager {
    pub take_lanes: Vec<TakeLane>,
    pub is_lanes_visible: bool,
    pub active_take_id: Option<u32>,
    pub crossfades: Vec<MicroCrossfade>,
    next_clip_id: u32,
}

impl Default for TrackCompingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackCompingManager {
    pub fn new() -> Self {
        Self {
            take_lanes: Vec::new(),
            is_lanes_visible: false,
            active_take_id: None,
            crossfades: Vec::new(),
            next_clip_id: 1000,
        }
    }

    /// Add a new take lane and return its ID.
    pub fn add_take_lane(&mut self, name: impl Into<String>) -> u32 {
        let id = self.take_lanes.len() as u32 + 1;
        self.take_lanes.push(TakeLane::new(id, name));
        id
    }

    /// Retrieve a mutable reference to a take lane by ID.
    pub fn get_take_lane_mut(&mut self, lane_id: u32) -> Option<&mut TakeLane> {
        self.take_lanes.iter_mut().find(|l| l.id == lane_id)
    }

    /// Retrieve an immutable reference to a take lane by ID.
    pub fn get_take_lane(&self, lane_id: u32) -> Option<&TakeLane> {
        self.take_lanes.iter().find(|l| l.id == lane_id)
    }

    /// Set or clear the actively auditioned take lane.
    /// When a take lane is auditioned, only its clips are heard in place of the main track clips.
    pub fn set_audition_take(&mut self, lane_id: Option<u32>) {
        self.active_take_id = lane_id;
        for lane in &mut self.take_lanes {
            lane.is_auditioning = Some(lane.id) == lane_id;
        }
    }

    /// Toggle auditioning for a take lane.
    pub fn toggle_audition_take(&mut self, lane_id: u32) {
        if self.active_take_id == Some(lane_id) {
            self.set_audition_take(None);
        } else {
            self.set_audition_take(Some(lane_id));
        }
    }

    /// Returns true if a specific take lane is currently auditioned.
    pub fn is_lane_auditioning(&self, lane_id: u32) -> bool {
        self.active_take_id == Some(lane_id)
    }

    /// Retrieve the clips that should be rendered: either the auditioned take lane's clips,
    /// or the main track's clips if no take lane is being auditioned.
    pub fn get_effective_clips<'a>(&'a self, main_clips: &'a [ArrangementClip]) -> &'a [ArrangementClip] {
        if let Some(lane_id) = self.active_take_id {
            if let Some(lane) = self.get_take_lane(lane_id) {
                return &lane.clips;
            }
        }
        main_clips
    }

    /// Blade slice tool on a specific take lane: splits any clip overlapping `split_beat`.
    pub fn blade_split_take_lane(&mut self, lane_id: u32, split_beat: f64) -> Result<bool, String> {
        let next_id = &mut self.next_clip_id;
        if let Some(lane) = self.take_lanes.iter_mut().find(|l| l.id == lane_id) {
            Ok(lane.split_at_beat(split_beat, next_id))
        } else {
            Err(format!("Take lane {} not found", lane_id))
        }
    }

    /// Non-destructively comp a timeline region `[start_beat, end_beat)` from `lane_id` into `main_clips`.
    ///
    /// 1. Slices and clears existing clips on the main track in `[start_beat, end_beat)`.
    /// 2. Clones and bounds matching slices from the source take lane.
    /// 3. Automatically registers equal-power micro-crossfades (4-10 ms) at slice junctions to avoid clicks.
    pub fn comp_region_to_main(
        &mut self,
        main_clips: &mut Vec<ArrangementClip>,
        lane_id: u32,
        start_beat: f64,
        end_beat: f64,
        crossfade_ms: f32,
    ) -> Result<usize, String> {
        if start_beat >= end_beat {
            return Err("Invalid comp region: start_beat must be strictly less than end_beat".into());
        }

        let lane = match self.get_take_lane(lane_id) {
            Some(l) => l.clone(),
            None => return Err(format!("Take lane {} not found", lane_id)),
        };

        let overlapping_source_clips = lane.clips_in_range(start_beat, end_beat);
        if overlapping_source_clips.is_empty() {
            return Ok(0);
        }

        // --- Step 1: Split or remove overlapping clips on the main track ---
        let mut new_main_clips = Vec::new();
        let mut split_left_id = None;
        let mut split_right_id = None;
        let mut adjacent_left_id = None;
        let mut adjacent_right_id = None;

        for clip in main_clips.drain(..) {
            let c_start = clip.timeline_start_beat;
            let c_end = clip.timeline_end_beat();

            if c_end <= start_beat || c_start >= end_beat {
                // Completely outside range: keep unmodified
                if (c_end - start_beat).abs() < 1e-4 {
                    adjacent_left_id = Some(clip.id);
                }
                if (c_start - end_beat).abs() < 1e-4 {
                    adjacent_right_id = Some(clip.id);
                }
                new_main_clips.push(clip);
            } else if c_start < start_beat && c_end > end_beat {
                // Surrounds the comp region: split into left piece and right piece
                let mut left_clip = clip.clone();
                left_clip.duration_beats = start_beat - c_start;
                split_left_id = Some(left_clip.id);
                new_main_clips.push(left_clip);

                let mut right_clip = clip;
                self.next_clip_id += 1;
                right_clip.id = self.next_clip_id;
                right_clip.timeline_start_beat = end_beat;
                right_clip.duration_beats = c_end - end_beat;
                right_clip.pattern_offset_beats += end_beat - c_start;
                split_right_id = Some(right_clip.id);
                new_main_clips.push(right_clip);
            } else if c_start < start_beat && c_end <= end_beat {
                // Overlaps the start boundary: trim end
                let mut left_clip = clip;
                left_clip.duration_beats = start_beat - c_start;
                split_left_id = Some(left_clip.id);
                new_main_clips.push(left_clip);
            } else if c_start >= start_beat && c_end > end_beat {
                // Overlaps the end boundary: trim start
                let mut right_clip = clip;
                let trimmed_duration = c_end - end_beat;
                let offset_added = end_beat - c_start;
                right_clip.timeline_start_beat = end_beat;
                right_clip.duration_beats = trimmed_duration;
                right_clip.pattern_offset_beats += offset_added;
                split_right_id = Some(right_clip.id);
                new_main_clips.push(right_clip);
            }
            // If completely enveloped in [start_beat, end_beat), it is dropped
        }

        // --- Step 2: Clone and insert bounded slices from the take lane ---
        let mut comped_count = 0;
        let mut first_comped_id = None;
        let mut last_comped_id = None;

        for src_clip in overlapping_source_clips {
            let c_start = src_clip.timeline_start_beat;
            let c_end = src_clip.timeline_end_beat();

            let slice_start = c_start.max(start_beat);
            let slice_end = c_end.min(end_beat);
            let slice_duration = slice_end - slice_start;

            if slice_duration > 1e-6 {
                self.next_clip_id += 1;
                let mut new_clip = src_clip.clone();
                new_clip.id = self.next_clip_id;
                new_clip.timeline_start_beat = slice_start;
                new_clip.duration_beats = slice_duration;
                new_clip.pattern_offset_beats += slice_start - c_start;

                if first_comped_id.is_none() {
                    first_comped_id = Some(new_clip.id);
                }
                last_comped_id = Some(new_clip.id);

                new_main_clips.push(new_clip);
                comped_count += 1;
            }
        }

        // --- Step 3: Insert automatic micro-crossfades at splice junctions ---
        let fade_ms = crossfade_ms.clamp(4.0, 10.0);
        let outbound_left = split_left_id.or(adjacent_left_id);
        let inbound_right = split_right_id.or(adjacent_right_id);

        if let (Some(left_id), Some(first_id)) = (outbound_left, first_comped_id) {
            self.crossfades.push(MicroCrossfade::new(fade_ms, start_beat, left_id, first_id));
        }
        if let (Some(last_id), Some(right_id)) = (last_comped_id, inbound_right) {
            self.crossfades.push(MicroCrossfade::new(fade_ms, end_beat, last_id, right_id));
        }

        // Sort main clips chronologically
        new_main_clips.sort_by(|a, b| {
            a.timeline_start_beat
                .partial_cmp(&b.timeline_start_beat)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        *main_clips = new_main_clips;
        Ok(comped_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{MidiNote, MidiPattern, ClipPattern};

    #[test]
    fn test_take_lane_region_comping_and_micro_crossfade() {
        let mut manager = TrackCompingManager::new();
        let lane_id = manager.add_take_lane("Vocal Take 2");

        // Create Take 2 clip spanning 0.0 .. 8.0 beats
        let mut midi_take = MidiPattern::new(8.0);
        midi_take.add_note(MidiNote::new(60, 100, 2.0, 1.0));
        let take_clip = ArrangementClip::new(20, "Take 2", 0.0, 8.0, ClipPattern::Midi(midi_take));
        manager.get_take_lane_mut(lane_id).unwrap().add_clip(take_clip);

        // Main track currently has Take 1 spanning 0.0 .. 8.0 beats
        let mut midi_main = MidiPattern::new(8.0);
        midi_main.add_note(MidiNote::new(67, 100, 0.0, 2.0));
        let main_clip = ArrangementClip::new(10, "Take 1", 0.0, 8.0, ClipPattern::Midi(midi_main));
        let mut main_clips = vec![main_clip];

        // Comp region 2.0 .. 4.0 from Take 2 into main track
        let count = manager
            .comp_region_to_main(&mut main_clips, lane_id, 2.0, 4.0, 5.0)
            .expect("Comping must succeed");

        assert_eq!(count, 1, "Should insert 1 comped slice");
        assert_eq!(main_clips.len(), 3, "Main track should now have 3 pieces: [0..2], [2..4], [4..8]");

        // Verify bounds of the 3 pieces
        assert_eq!(main_clips[0].timeline_start_beat, 0.0);
        assert_eq!(main_clips[0].duration_beats, 2.0);

        assert_eq!(main_clips[1].timeline_start_beat, 2.0);
        assert_eq!(main_clips[1].duration_beats, 2.0);
        assert_eq!(main_clips[1].name, "Take 2");

        assert_eq!(main_clips[2].timeline_start_beat, 4.0);
        assert_eq!(main_clips[2].duration_beats, 4.0);

        // Verify micro-crossfades were inserted at boundary beats 2.0 and 4.0
        assert_eq!(manager.crossfades.len(), 2);
        assert_eq!(manager.crossfades[0].beat_position, 2.0);
        assert_eq!(manager.crossfades[0].duration_ms, 5.0);
        assert_eq!(manager.crossfades[1].beat_position, 4.0);

        // Check equal power math
        let in_gain = MicroCrossfade::fade_in_gain(50, 100);
        let out_gain = MicroCrossfade::fade_out_gain(50, 100);
        let power = in_gain * in_gain + out_gain * out_gain;
        assert!((power - 1.0).abs() < 1e-4, "Equal-power crossfade must sum to 1.0 power");
    }

    #[test]
    fn test_take_lane_auditioning_and_effective_clips() {
        let mut manager = TrackCompingManager::new();
        let lane1_id = manager.add_take_lane("Vocal Take 1");
        let lane2_id = manager.add_take_lane("Vocal Take 2");

        let clip1 = ArrangementClip::new(101, "T1", 0.0, 4.0, ClipPattern::Midi(MidiPattern::new(4.0)));
        let clip2 = ArrangementClip::new(102, "T2", 0.0, 4.0, ClipPattern::Midi(MidiPattern::new(4.0)));
        manager.get_take_lane_mut(lane1_id).unwrap().add_clip(clip1.clone());
        manager.get_take_lane_mut(lane2_id).unwrap().add_clip(clip2.clone());

        let main_clip = ArrangementClip::new(1, "Main Comp", 0.0, 4.0, ClipPattern::Midi(MidiPattern::new(4.0)));
        let main_clips = vec![main_clip];

        // Initially no lane auditioned: effective clips must be main track clips
        assert!(!manager.is_lane_auditioning(lane1_id));
        assert!(!manager.is_lane_auditioning(lane2_id));
        assert_eq!(manager.get_effective_clips(&main_clips)[0].id, 1);

        // Audition Take 1
        manager.set_audition_take(Some(lane1_id));
        assert!(manager.is_lane_auditioning(lane1_id));
        assert!(!manager.is_lane_auditioning(lane2_id));
        assert!(manager.get_take_lane(lane1_id).unwrap().is_auditioning);
        assert!(!manager.get_take_lane(lane2_id).unwrap().is_auditioning);
        assert_eq!(manager.get_effective_clips(&main_clips)[0].id, 101);

        // Toggle audition to Take 2
        manager.toggle_audition_take(lane2_id);
        assert!(!manager.is_lane_auditioning(lane1_id));
        assert!(manager.is_lane_auditioning(lane2_id));
        assert_eq!(manager.get_effective_clips(&main_clips)[0].id, 102);

        // Toggle Take 2 off: back to main clips
        manager.toggle_audition_take(lane2_id);
        assert!(!manager.is_lane_auditioning(lane1_id));
        assert!(!manager.is_lane_auditioning(lane2_id));
        assert_eq!(manager.get_effective_clips(&main_clips)[0].id, 1);
    }

    #[test]
    fn test_take_lane_blade_splicing_and_4ms_audio_crossfade() {
        let mut manager = TrackCompingManager::new();
        let lane_id = manager.add_take_lane("Guitar Take");

        let clip = ArrangementClip::new(50, "Full Take", 0.0, 16.0, ClipPattern::Midi(MidiPattern::new(16.0)));
        manager.get_take_lane_mut(lane_id).unwrap().add_clip(clip);

        // Blade slice at beat 6.0
        let split = manager.blade_split_take_lane(lane_id, 6.0).expect("Blade slice must succeed");
        assert!(split, "Clip should be split at beat 6.0");

        let lane = manager.get_take_lane(lane_id).unwrap();
        assert_eq!(lane.clips.len(), 2);
        assert_eq!(lane.clips[0].timeline_start_beat, 0.0);
        assert_eq!(lane.clips[0].duration_beats, 6.0);
        assert_eq!(lane.clips[1].timeline_start_beat, 6.0);
        assert_eq!(lane.clips[1].duration_beats, 10.0);

        // Verify 4ms crossfade audio frames calculation and buffer crossfade
        let xfade = MicroCrossfade::new(4.0, 6.0, lane.clips[0].id, lane.clips[1].id);
        assert_eq!(xfade.duration_ms, 4.0);
        assert_eq!(xfade.beat_position, 6.0);

        let frames_44k = xfade.frames_at_sample_rate(44100.0);
        assert_eq!(frames_44k, 176, "4ms @ 44.1 kHz = 176 frames");

        let frames_48k = xfade.frames_at_sample_rate(48000.0);
        assert_eq!(frames_48k, 192, "4ms @ 48.0 kHz = 192 frames");

        let frames_96k = xfade.frames_at_sample_rate(96000.0);
        assert_eq!(frames_96k, 384, "4ms @ 96.0 kHz = 384 frames");

        // Apply equal power crossfade to audio buffers
        let out_buf = vec![1.0f32; frames_44k];
        let in_buf = vec![0.5f32; frames_44k];
        let mut mixed = vec![0.0f32; frames_44k];

        xfade.apply_crossfade(&out_buf, &in_buf, &mut mixed);

        // At frame 0: out_buf full (1.0), in_buf zero (0.0) -> result ~1.0
        assert!((mixed[0] - 1.0).abs() < 1e-4);

        // At midpoint: out_buf * 0.7071 + in_buf * 0.7071 = 1.0 * 0.7071 + 0.5 * 0.7071 = 1.0606
        let mid = frames_44k / 2;
        let expected_mid = 1.0 * (0.25 * PI).cos() + 0.5 * (0.25 * PI).sin();
        assert!((mixed[mid] - expected_mid).abs() < 1e-3);

        // At end: out_buf zero (0.0), in_buf full (0.5) -> result ~0.5
        assert!((mixed[frames_44k - 1] - 0.5).abs() < 1e-2);
    }
}
