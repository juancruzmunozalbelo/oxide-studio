//! Multi-track linear arrangement timeline, loop regions, and playhead tracking.

use serde::{Deserialize, Serialize};
use crate::automation::AutomationLane;
use crate::pattern::ClipPattern;

/// A clip placed horizontally on the linear arrangement timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrangementClip {
    pub id: u32,
    pub name: String,
    pub timeline_start_beat: f64,
    pub duration_beats: f64,
    pub pattern_offset_beats: f64,
    pub muted: bool,
    pub pattern: ClipPattern,
}

impl ArrangementClip {
    pub fn new(
        id: u32,
        name: impl Into<String>,
        timeline_start_beat: f64,
        duration_beats: f64,
        pattern: ClipPattern,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            timeline_start_beat,
            duration_beats,
            pattern_offset_beats: 0.0,
            muted: false,
            pattern,
        }
    }

    /// End beat of this clip on the timeline.
    pub fn timeline_end_beat(&self) -> f64 {
        self.timeline_start_beat + self.duration_beats
    }

    /// Check if this clip covers `timeline_beat`.
    pub fn contains_beat(&self, timeline_beat: f64) -> bool {
        !self.muted
            && timeline_beat >= self.timeline_start_beat
            && timeline_beat < self.timeline_end_beat()
    }

    /// Calculate the wrapped pattern loop phase (in beats) for a given timeline beat.
    ///
    /// Formula:
    /// `pattern_phase = (pattern_offset + (timeline_beat - timeline_start)) % pattern_length`
    pub fn pattern_phase_at_beat(&self, timeline_beat: f64) -> f64 {
        let pattern_len = self.pattern.length_beats().max(1e-6);
        let rel_beat = (timeline_beat - self.timeline_start_beat) + self.pattern_offset_beats;
        ((rel_beat % pattern_len) + pattern_len) % pattern_len
    }
}

/// A track in the arrangement timeline managing clips and automation lanes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrangementTrack {
    pub id: u32,
    pub name: String,
    pub track_type: String, // "MidiSynth", "DrumRack", "Audio"
    pub clips: Vec<ArrangementClip>,
    pub automation_lanes: Vec<AutomationLane>,
}

impl ArrangementTrack {
    pub fn new(id: u32, name: impl Into<String>, track_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            track_type: track_type.into(),
            clips: Vec::new(),
            automation_lanes: Vec::new(),
        }
    }

    /// Add a clip to this track.
    pub fn add_clip(&mut self, clip: ArrangementClip) {
        self.clips.push(clip);
    }

    /// Retrieve the active non-muted clip at `timeline_beat`, if any.
    pub fn clip_at_beat(&self, timeline_beat: f64) -> Option<&ArrangementClip> {
        self.clips.iter().find(|c| c.contains_beat(timeline_beat))
    }

    /// Retrieve all clips overlapping the time range `[start_beat, end_beat)`.
    pub fn clips_in_range(&self, start_beat: f64, end_beat: f64) -> Vec<&ArrangementClip> {
        self.clips
            .iter()
            .filter(|c| c.timeline_start_beat < end_beat && c.timeline_end_beat() > start_beat)
            .collect()
    }

    /// Find an automation lane by parameter name.
    pub fn get_automation_lane(&self, param_name: &str) -> Option<&AutomationLane> {
        self.automation_lanes.iter().find(|l| l.parameter_name == param_name)
    }

    /// Find or create an automation lane by parameter ID and name.
    pub fn get_or_create_automation_lane(&mut self, param_id: u32, param_name: &str) -> &mut AutomationLane {
        if let Some(pos) = self.automation_lanes.iter().position(|l| l.parameter_name == param_name) {
            &mut self.automation_lanes[pos]
        } else {
            self.automation_lanes.push(AutomationLane::new(param_id, param_name));
            self.automation_lanes.last_mut().unwrap()
        }
    }
}

/// Loop region bounds on the linear timeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoopRegion {
    pub enabled: bool,
    pub start_beat: f64,
    pub end_beat: f64,
}

impl Default for LoopRegion {
    fn default() -> Self {
        Self {
            enabled: false,
            start_beat: 0.0,
            end_beat: 16.0,
        }
    }
}

impl LoopRegion {
    pub const fn new(enabled: bool, start_beat: f64, end_beat: f64) -> Self {
        Self {
            enabled,
            start_beat,
            end_beat,
        }
    }

    /// Wrap `beat` within loop bounds if loop is enabled and beat is past `end_beat`.
    pub fn wrap_beat(&self, beat: f64) -> f64 {
        if !self.enabled || self.end_beat <= self.start_beat {
            return beat;
        }
        if beat < self.start_beat {
            return beat;
        }
        let length = self.end_beat - self.start_beat;
        self.start_beat + ((beat - self.start_beat) % length)
    }
}

/// Multi-track linear arrangement timeline container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrangementTimeline {
    pub tracks: Vec<ArrangementTrack>,
    pub loop_region: LoopRegion,
    pub playhead_beat: f64,
    pub is_playing: bool,
}

impl Default for ArrangementTimeline {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            loop_region: LoopRegion::default(),
            playhead_beat: 0.0,
            is_playing: false,
        }
    }
}

impl ArrangementTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance playhead by `delta_beats`, applying loop wrapping.
    pub fn advance_beats(&mut self, delta_beats: f64) {
        if self.is_playing && delta_beats > 0.0 {
            let next_beat = self.playhead_beat + delta_beats;
            self.playhead_beat = self.loop_region.wrap_beat(next_beat);
        }
    }

    /// Seek playhead to specified beat position.
    pub fn seek(&mut self, beat: f64) {
        self.playhead_beat = beat.max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{MidiNote, MidiPattern};

    #[test]
    fn test_arrangement_clip_loop_phase() {
        let mut midi = MidiPattern::new(4.0);
        midi.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        let clip = ArrangementClip::new(1, "Bass", 8.0, 12.0, ClipPattern::Midi(midi));

        // Clip spans 8.0 .. 20.0 (duration 12.0 beats, looped 3 times over 4.0 beat pattern)
        assert!(clip.contains_beat(8.0));
        assert!(clip.contains_beat(19.9));
        assert!(!clip.contains_beat(20.0));
        assert!(!clip.contains_beat(7.9));

        // Phase calculations
        assert_eq!(clip.pattern_phase_at_beat(8.0), 0.0);
        assert_eq!(clip.pattern_phase_at_beat(10.5), 2.5);
        assert_eq!(clip.pattern_phase_at_beat(12.0), 0.0);
        assert_eq!(clip.pattern_phase_at_beat(14.5), 2.5);
    }

    #[test]
    fn test_loop_region_wrapping() {
        let mut timeline = ArrangementTimeline::new();
        timeline.loop_region = LoopRegion::new(true, 4.0, 12.0);
        timeline.playhead_beat = 4.0;
        timeline.is_playing = true;

        timeline.advance_beats(4.0);
        assert_eq!(timeline.playhead_beat, 8.0);

        timeline.advance_beats(3.9);
        assert_eq!(timeline.playhead_beat, 11.9);

        // Advance 0.2: hits 12.1 -> wraps to 4.1
        timeline.advance_beats(0.2);
        assert!((timeline.playhead_beat - 4.1).abs() < 1e-9);
    }
}
