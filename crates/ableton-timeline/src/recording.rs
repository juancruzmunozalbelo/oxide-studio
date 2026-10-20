//! Session-to-Arrangement recording and "Back to Arrangement" track override state machine.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::arrangement::{ArrangementClip, ArrangementTrack};
use crate::automation::AutomationPoint;
use crate::session::{SessionClip, SessionMatrix};

/// Track playback control source: following linear Arrangement or overridden by active Session clips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrangementControlState {
    /// Track strictly follows clips and automation lanes on the linear arrangement timeline.
    #[default]
    Arrangement,
    /// A Session clip was launched on this track, overriding arrangement playback for this track.
    SessionOverride,
}

/// In-progress recording clip being captured from a playing session clip.
#[derive(Debug, Clone)]
struct InProgressClip {
    clip_id: u32,
    clip_name: String,
    start_beat: f64,
    session_clip: SessionClip,
}

/// Session-to-Arrangement recording manager.
///
/// Tracks real-time session clip launches and parameter tweaks,
/// writing them into the arrangement timeline during active recording.
#[derive(Debug, Clone, Default)]
pub struct SessionToArrangementRecorder {
    pub control_states: Vec<ArrangementControlState>,
    pub is_recording: bool,
    in_progress_clips: HashMap<usize, InProgressClip>,
    next_clip_id: u32,
}

impl SessionToArrangementRecorder {
    pub fn new(num_tracks: usize) -> Self {
        Self {
            control_states: vec![ArrangementControlState::Arrangement; num_tracks],
            is_recording: false,
            in_progress_clips: HashMap::new(),
            next_clip_id: 1000,
        }
    }

    /// Returns true if any track in the project is currently in `SessionOverride`.
    pub fn has_any_override(&self) -> bool {
        self.control_states.contains(&ArrangementControlState::SessionOverride)
    }

    /// Check if a specific track is currently in `SessionOverride`.
    pub fn is_track_in_override(&self, track_idx: usize) -> bool {
        self.control_states
            .get(track_idx)
            .copied()
            .unwrap_or(ArrangementControlState::Arrangement) == ArrangementControlState::SessionOverride
    }

    /// Set recording armed state.
    pub fn set_recording(&mut self, recording: bool) {
        self.is_recording = recording;
    }

    /// Called when a session clip is triggered or starts playing on a track.
    ///
    /// Sets track to `SessionOverride`. If recording is engaged, captures the clip into arrangement.
    pub fn on_session_clip_launched(
        &mut self,
        track_idx: usize,
        arrangement_track: &mut ArrangementTrack,
        clip: &SessionClip,
        launch_beat: f64,
    ) {
        if track_idx >= self.control_states.len() {
            self.control_states.resize(track_idx + 1, ArrangementControlState::Arrangement);
        }
        self.control_states[track_idx] = ArrangementControlState::SessionOverride;

        if self.is_recording {
            // Close any existing in-progress clip on this track
            self.finalize_track_clip(track_idx, arrangement_track, launch_beat);

            // Register new in-progress clip
            self.in_progress_clips.insert(
                track_idx,
                InProgressClip {
                    clip_id: self.next_clip_id,
                    clip_name: clip.name.clone(),
                    start_beat: launch_beat,
                    session_clip: clip.clone(),
                },
            );
            self.next_clip_id += 1;
        }
    }

    /// Called when a session clip is stopped on a track.
    pub fn on_session_clip_stopped(
        &mut self,
        track_idx: usize,
        arrangement_track: &mut ArrangementTrack,
        stop_beat: f64,
    ) {
        if self.is_recording {
            self.finalize_track_clip(track_idx, arrangement_track, stop_beat);
        }
    }

    /// Record a real-time parameter adjustment into the arrangement track's automation lane.
    pub fn on_parameter_tweaked(
        &mut self,
        _track_idx: usize,
        arrangement_track: &mut ArrangementTrack,
        param_id: u32,
        param_name: &str,
        beat: f64,
        value_norm: f32,
    ) {
        if self.is_recording {
            let lane = arrangement_track.get_or_create_automation_lane(param_id, param_name);
            lane.add_point(AutomationPoint::linear(beat, value_norm));
        }
    }

    /// "Back to Arrangement" resynchronization:
    ///
    /// 1. Finalizes any in-progress recording clips.
    /// 2. Resets all track states to `ArrangementControlState::Arrangement`.
    /// 3. Immediately stops session clips in the matrix.
    pub fn back_to_arrangement(
        &mut self,
        arrangement_tracks: &mut [ArrangementTrack],
        session_matrix: &mut SessionMatrix,
        current_beat: f64,
    ) {
        // Finalize all open clips
        let open_track_indices: Vec<usize> = self.in_progress_clips.keys().copied().collect();
        for t_idx in open_track_indices {
            if let Some(arr_track) = arrangement_tracks.get_mut(t_idx) {
                self.finalize_track_clip(t_idx, arr_track, current_beat);
            }
        }

        // Reset control states
        self.control_states.fill(ArrangementControlState::Arrangement);

        // Stop all session playback
        for track in &mut session_matrix.tracks {
            track.stop_immediate();
        }
    }

    fn finalize_track_clip(
        &mut self,
        track_idx: usize,
        arrangement_track: &mut ArrangementTrack,
        end_beat: f64,
    ) {
        if let Some(in_progress) = self.in_progress_clips.remove(&track_idx) {
            let duration = (end_beat - in_progress.start_beat).max(0.25);
            let arr_clip = ArrangementClip::new(
                in_progress.clip_id,
                in_progress.clip_name,
                in_progress.start_beat,
                duration,
                in_progress.session_clip.pattern,
            );
            arrangement_track.add_clip(arr_clip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{MidiNote, MidiPattern, ClipPattern};

    #[test]
    fn test_session_to_arrangement_recording_and_back() {
        let mut recorder = SessionToArrangementRecorder::new(2);
        let mut arr_tracks = vec![
            ArrangementTrack::new(0, "Synth", "MidiSynth"),
            ArrangementTrack::new(1, "Drums", "DrumRack"),
        ];
        let mut matrix = SessionMatrix::new(2, 2);

        // Put a clip in Track 0
        let mut midi = MidiPattern::new(4.0);
        midi.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        let clip = SessionClip::new(101, "Lead Clip", ClipPattern::Midi(midi));

        // Arm recording
        recorder.set_recording(true);

        // Clip launched at beat 4.0
        recorder.on_session_clip_launched(0, &mut arr_tracks[0], &clip, 4.0);
        assert!(recorder.is_track_in_override(0));
        assert!(recorder.has_any_override());

        // Parameter tweak at beat 6.0
        recorder.on_parameter_tweaked(0, &mut arr_tracks[0], 10, "filter_cutoff", 6.0, 0.75);

        // Clip stopped at beat 8.0
        recorder.on_session_clip_stopped(0, &mut arr_tracks[0], 8.0);

        // Verify arrangement track received the recorded clip
        assert_eq!(arr_tracks[0].clips.len(), 1);
        let recorded = &arr_tracks[0].clips[0];
        assert_eq!(recorded.timeline_start_beat, 4.0);
        assert_eq!(recorded.duration_beats, 4.0);
        assert_eq!(recorded.name, "Lead Clip");

        // Verify automation point was recorded
        let lane = arr_tracks[0].get_automation_lane("filter_cutoff").expect("Lane should exist");
        assert_eq!(lane.points.len(), 1);
        assert_eq!(lane.points[0].beat, 6.0);
        assert_eq!(lane.points[0].value_norm, 0.75);

        // Click "Back to Arrangement" at beat 12.0
        recorder.back_to_arrangement(&mut arr_tracks, &mut matrix, 12.0);
        assert!(!recorder.has_any_override());
        assert!(!recorder.is_track_in_override(0));
    }
}
