//! Multi-track linear arrangement timeline, loop regions, and playhead tracking.

use serde::{Deserialize, Serialize};
use crate::automation::AutomationLane;
use crate::{ClipEnvelopeCollection, ClipEnvelopeTarget};
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
    #[serde(default)]
    pub envelopes: ClipEnvelopeCollection,
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
            envelopes: ClipEnvelopeCollection::default(),
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

    /// Evaluate effective parameter value at `timeline_beat`.
    pub fn evaluate_envelope(
        &self,
        target: &ClipEnvelopeTarget,
        timeline_beat: f64,
        base_val: f32,
    ) -> f32 {
        let pattern_phase = self.pattern_phase_at_beat(timeline_beat);
        let pattern_len = self.pattern.length_beats().max(1e-4);
        self.envelopes
            .evaluate_effective(target, pattern_phase, 0.0, pattern_len, base_val)
    }

    /// Evaluate 14-bit MIDI pitch bend (-8192 .. +8191) at `timeline_beat`.
    pub fn evaluate_pitch_bend(&self, timeline_beat: f64) -> i16 {
        let pattern_phase = self.pattern_phase_at_beat(timeline_beat);
        let pattern_len = self.pattern.length_beats().max(1e-4);
        self.envelopes
            .evaluate_pitch_bend_14bit(pattern_phase, 0.0, pattern_len)
    }

    /// Evaluate modulation wheel CC 1 (0 .. 127) at `timeline_beat`.
    pub fn evaluate_mod_wheel(&self, timeline_beat: f64) -> u8 {
        let pattern_phase = self.pattern_phase_at_beat(timeline_beat);
        let pattern_len = self.pattern.length_beats().max(1e-4);
        self.envelopes
            .evaluate_mod_wheel_7bit(pattern_phase, 0.0, pattern_len)
    }

    /// Evaluate MIDI Continuous Controller (0 .. 127) at `timeline_beat`.
    pub fn evaluate_cc(&self, cc_num: u8, timeline_beat: f64) -> u8 {
        let pattern_phase = self.pattern_phase_at_beat(timeline_beat);
        let pattern_len = self.pattern.length_beats().max(1e-4);
        self.envelopes
            .evaluate_cc_7bit(cc_num, pattern_phase, 0.0, pattern_len)
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

    /// Split a clip covering `split_beat` on this track into two distinct clips.
    /// The first clip's duration is truncated to `split_beat - timeline_start_beat`.
    /// The second clip starts at `split_beat` with the remaining duration and adjusted pattern offset.
    /// Returns `true` if an active clip was split.
    pub fn split_clip_at(&mut self, split_beat: f64, new_clip_id: u32) -> bool {
        let mut split_clip = None;
        for clip in self.clips.iter_mut() {
            if clip.timeline_start_beat < split_beat && clip.timeline_end_beat() > split_beat {
                let first_dur = split_beat - clip.timeline_start_beat;
                let second_dur = clip.duration_beats - first_dur;
                let second_offset = clip.pattern_offset_beats + first_dur;

                let mut second = ArrangementClip::new(
                    new_clip_id,
                    format!("{}_part2", clip.name),
                    split_beat,
                    second_dur,
                    clip.pattern.clone(),
                );
                second.pattern_offset_beats = second_offset;
                clip.duration_beats = first_dur;
                split_clip = Some(second);
                break;
            }
        }

        if let Some(second) = split_clip {
            self.clips.push(second);
            true
        } else {
            false
        }
    }

    /// Consolidate arrangement clips overlapping `[start_beat, end_beat)` into a single unified clip (Cmd+J).
    /// Gathers all MIDI notes from overlapping MIDI clips into a unified pattern.
    /// Returns the new consolidated clip if any clips overlapped the range.
    pub fn consolidate_range(
        &mut self,
        start_beat: f64,
        end_beat: f64,
        consolidated_id: u32,
        new_name: impl Into<String>,
    ) -> Option<ArrangementClip> {
        if end_beat <= start_beat {
            return None;
        }
        let duration = end_beat - start_beat;

        let overlapping_indices: Vec<usize> = self
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| c.timeline_start_beat < end_beat && c.timeline_end_beat() > start_beat)
            .map(|(i, _)| i)
            .collect();

        if overlapping_indices.is_empty() {
            return None;
        }

        let mut merged_midi = crate::pattern::MidiPattern::new(duration);
        let mut has_midi = false;
        let mut audio_data = None;

        for &idx in &overlapping_indices {
            let clip = &self.clips[idx];
            match &clip.pattern {
                ClipPattern::Midi(midi) => {
                    has_midi = true;
                    for note in &midi.notes {
                        let note_abs_beat = clip.timeline_start_beat + note.start_beat - clip.pattern_offset_beats;
                        if note_abs_beat >= clip.timeline_start_beat
                            && note_abs_beat < clip.timeline_end_beat()
                            && note_abs_beat >= start_beat
                            && note_abs_beat < end_beat
                        {
                            let mut n = note.clone();
                            n.start_beat = note_abs_beat - start_beat;
                            if n.start_beat + n.duration_beats > duration {
                                n.duration_beats = duration - n.start_beat;
                            }
                            merged_midi.add_note(n);
                        }
                    }
                }
                ClipPattern::Audio(audio) => {
                    if audio_data.is_none() {
                        let mut a = audio.clone();
                        a.length_beats = duration;
                        audio_data = Some(a);
                    }
                }
                ClipPattern::Drum(_) => {}
            }
        }

        let pattern = if has_midi {
            ClipPattern::Midi(merged_midi)
        } else if let Some(aud) = audio_data {
            ClipPattern::Audio(aud)
        } else {
            ClipPattern::Midi(merged_midi)
        };

        // Remove fully encompassed clips; trim partially overlapping clips
        let mut i = 0;
        while i < self.clips.len() {
            let c = &self.clips[i];
            if c.timeline_start_beat >= start_beat && c.timeline_end_beat() <= end_beat {
                self.clips.remove(i);
            } else if c.timeline_start_beat < start_beat && c.timeline_end_beat() > start_beat && c.timeline_end_beat() <= end_beat {
                self.clips[i].duration_beats = start_beat - self.clips[i].timeline_start_beat;
                i += 1;
            } else if c.timeline_start_beat >= start_beat && c.timeline_start_beat < end_beat && c.timeline_end_beat() > end_beat {
                let trim = end_beat - self.clips[i].timeline_start_beat;
                self.clips[i].timeline_start_beat = end_beat;
                self.clips[i].duration_beats = (self.clips[i].duration_beats - trim).max(0.0);
                self.clips[i].pattern_offset_beats += trim;
                i += 1;
            } else {
                i += 1;
            }
        }

        let consolidated = ArrangementClip::new(
            consolidated_id,
            new_name,
            start_beat,
            duration,
            pattern,
        );
        self.clips.push(consolidated.clone());
        Some(consolidated)
    }
}

/// A named song section locator or marker on the linear timeline ruler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineLocator {
    pub beat: f64,
    pub name: String,
}

impl TimelineLocator {
    pub fn new(beat: f64, name: impl Into<String>) -> Self {
        Self {
            beat,
            name: name.into(),
        }
    }
}

impl From<(f64, String)> for TimelineLocator {
    fn from((beat, name): (f64, String)) -> Self {
        Self::new(beat, name)
    }
}

impl From<(f64, &str)> for TimelineLocator {
    fn from((beat, name): (f64, &str)) -> Self {
        Self::new(beat, name)
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
    #[serde(default)]
    pub locators: Vec<TimelineLocator>,
}

impl Default for ArrangementTimeline {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            loop_region: LoopRegion::default(),
            playhead_beat: 0.0,
            is_playing: false,
            locators: Vec::new(),
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

    /// Add a locator at `beat` with `name`, maintaining ascending beat order.
    pub fn add_locator(&mut self, beat: f64, name: impl Into<String>) {
        let loc = TimelineLocator::new(beat, name);
        let pos = self.locators.partition_point(|l| l.beat <= beat);
        self.locators.insert(pos, loc);
    }

    /// Remove locator by index.
    pub fn remove_locator(&mut self, index: usize) -> Option<TimelineLocator> {
        if index < self.locators.len() {
            Some(self.locators.remove(index))
        } else {
            None
        }
    }

    /// Jump playhead to locator at `index`.
    pub fn jump_to_locator(&mut self, index: usize) -> bool {
        if let Some(loc) = self.locators.get(index) {
            self.playhead_beat = loc.beat;
            true
        } else {
            false
        }
    }

    /// Jump playhead to locator matching `name` (case-insensitive).
    pub fn jump_to_locator_by_name(&mut self, name: &str) -> bool {
        if let Some(loc) = self.locators.iter().find(|l| l.name.eq_ignore_ascii_case(name)) {
            self.playhead_beat = loc.beat;
            true
        } else {
            false
        }
    }

    /// Navigate to the next locator strictly ahead of the current playhead.
    pub fn next_locator(&mut self) -> Option<&TimelineLocator> {
        const EPSILON: f64 = 1e-4;
        let target = self.locators.iter().find(|l| l.beat > self.playhead_beat + EPSILON);
        if let Some(loc) = target {
            self.playhead_beat = loc.beat;
        }
        target
    }

    /// Navigate to the previous locator strictly behind the current playhead.
    pub fn prev_locator(&mut self) -> Option<&TimelineLocator> {
        const EPSILON: f64 = 1e-4;
        let target = self.locators.iter().rev().find(|l| l.beat < self.playhead_beat - EPSILON);
        if let Some(loc) = target {
            self.playhead_beat = loc.beat;
        }
        target
    }

    /// Split active clips across all tracks at `split_beat`.
    /// Returns the number of clips split.
    pub fn split_at(&mut self, split_beat: f64, next_id_start: u32) -> usize {
        let mut count = 0;
        let mut id = next_id_start;
        for track in &mut self.tracks {
            if track.split_clip_at(split_beat, id) {
                count += 1;
                id += 1;
            }
        }
        count
    }

    /// Split clips across all tracks at current playhead position.
    pub fn split_at_playhead(&mut self, next_id_start: u32) -> usize {
        let beat = self.playhead_beat;
        self.split_at(beat, next_id_start)
    }

    /// Consolidate range on a specific track.
    pub fn consolidate_track_range(
        &mut self,
        track_id: u32,
        start_beat: f64,
        end_beat: f64,
        new_clip_id: u32,
        new_name: impl Into<String>,
    ) -> Option<ArrangementClip> {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == track_id) {
            track.consolidate_range(start_beat, end_beat, new_clip_id, new_name)
        } else {
            None
        }
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

    #[test]
    fn test_arrangement_fractional_beat_placement() {
        let mut midi = MidiPattern::new(4.0);
        midi.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        let clip = ArrangementClip::new(1, "HiHat", 1.25, 2.75, ClipPattern::Midi(midi));

        // Spans 1.25 .. 4.0
        assert_eq!(clip.timeline_start_beat, 1.25);
        assert_eq!(clip.duration_beats, 2.75);
        assert_eq!(clip.timeline_end_beat(), 4.0);

        assert!(!clip.contains_beat(1.24));
        assert!(clip.contains_beat(1.25));
        assert!(clip.contains_beat(2.5));
        assert!(clip.contains_beat(3.99));
        assert!(!clip.contains_beat(4.0));

        // Sub-beat phase
        assert_eq!(clip.pattern_phase_at_beat(1.25), 0.0);
        assert_eq!(clip.pattern_phase_at_beat(2.5), 1.25);
    }

    #[test]
    fn test_arrangement_locator_navigation() {
        let mut timeline = ArrangementTimeline::new();
        timeline.add_locator(0.0, "1. Intro");
        timeline.add_locator(16.5, "2. Build");
        timeline.add_locator(32.25, "3. Drop");
        timeline.add_locator(48.0, "4. Outro");

        assert_eq!(timeline.locators.len(), 4);
        assert_eq!(timeline.locators[1].beat, 16.5);
        assert_eq!(timeline.locators[1].name, "2. Build");

        // Initial playhead at 0.0
        assert_eq!(timeline.playhead_beat, 0.0);

        // Next locator jumps to 16.5
        let next1 = timeline.next_locator().unwrap();
        assert_eq!(next1.name, "2. Build");
        assert_eq!(timeline.playhead_beat, 16.5);

        // Next locator jumps to 32.25
        let next2 = timeline.next_locator().unwrap();
        assert_eq!(next2.name, "3. Drop");
        assert_eq!(timeline.playhead_beat, 32.25);

        // Previous locator jumps back to 16.5
        let prev1 = timeline.prev_locator().unwrap();
        assert_eq!(prev1.name, "2. Build");
        assert_eq!(timeline.playhead_beat, 16.5);

        // Jump by name
        assert!(timeline.jump_to_locator_by_name("4. Outro"));
        assert_eq!(timeline.playhead_beat, 48.0);

        // Jump by index
        assert!(timeline.jump_to_locator(0));
        assert_eq!(timeline.playhead_beat, 0.0);
    }

    #[test]
    fn test_arrangement_track_split_and_consolidate() {
        let mut track = ArrangementTrack::new(1, "Synth", "MidiSynth");
        let mut pattern1 = MidiPattern::new(4.0);
        pattern1.add_note(MidiNote::new(60, 100, 0.5, 1.0));
        let clip1 = ArrangementClip::new(10, "Lead_PartA", 0.0, 4.0, ClipPattern::Midi(pattern1));
        track.add_clip(clip1);

        // Split at beat 2.0
        let did_split = track.split_clip_at(2.0, 11);
        assert!(did_split);
        assert_eq!(track.clips.len(), 2);
        assert_eq!(track.clips[0].duration_beats, 2.0);
        assert_eq!(track.clips[1].timeline_start_beat, 2.0);
        assert_eq!(track.clips[1].duration_beats, 2.0);

        // Consolidate back over [0.0, 4.0)
        let consolidated = track.consolidate_range(0.0, 4.0, 20, "Lead_Consolidated").unwrap();
        assert_eq!(consolidated.timeline_start_beat, 0.0);
        assert_eq!(consolidated.duration_beats, 4.0);
        assert_eq!(track.clips.len(), 1);
        if let ClipPattern::Midi(m) = &track.clips[0].pattern {
            assert_eq!(m.notes.len(), 1);
            assert_eq!(m.notes[0].pitch, 60);
            assert_eq!(m.notes[0].start_beat, 0.5);
        } else {
            panic!("Expected Midi pattern in consolidated clip");
        }
    }
}
