//! Track Domain Model supporting unlimited MIDI, Audio, Group, and Return Tracks for Ableton Live 12.
//!
//! Features:
//! - Flexible track types: `Midi`, `Audio`, `Group`, `Return`, `Master`.
//! - Per-track mixer channel state: Volume dB, Pan [-1.0, 1.0], Mute, Solo, Record Arm.
//! - Dynamic Pre/Post-fader Send knobs routing into Return tracks (e.g. Return A Reverb, Return B Delay).
//! - Flexible I/O routing destinations: `Master`, `GroupTrack(id)`, `SendsOnly`, `Resampling`, `External(ch)`.
//! - Hardware/Track Delay compensation (-100.0 .. +100.0 ms).
//! - 3-way DJ Crossfader assignment: `A`, `Off`, `B`.
//! - Embedded Take Lanes comping manager and Multi-lane parameter automation.

use serde::{Deserialize, Serialize};
use oxide_core::TrackArchetype;
use crate::arrangement::ArrangementClip;
use crate::automation::MultiLaneAutomation;
use crate::comping::TrackCompingManager;

/// Ableton Live track category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackType {
    Midi,
    Audio,
    Group,
    Return,
    Master,
}

impl From<TrackArchetype> for TrackType {
    fn from(arch: TrackArchetype) -> Self {
        match arch {
            TrackArchetype::Audio => TrackType::Audio,
            TrackArchetype::Midi => TrackType::Midi,
            TrackArchetype::Return => TrackType::Return,
            TrackArchetype::Master => TrackType::Master,
            TrackArchetype::Group => TrackType::Group,
        }
    }
}

impl From<TrackType> for TrackArchetype {
    fn from(tt: TrackType) -> Self {
        match tt {
            TrackType::Audio => TrackArchetype::Audio,
            TrackType::Midi => TrackArchetype::Midi,
            TrackType::Return => TrackArchetype::Return,
            TrackType::Master => TrackArchetype::Master,
            TrackType::Group => TrackArchetype::Group,
        }
    }
}

/// 3-way DJ Crossfader assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossfadeAssign {
    A,
    Off,
    B,
}

/// Send tapping position relative to the track volume fader and pan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendRoutingMode {
    PreFader,
    PostFader,
}

/// Audio output destination selector for flexible sub-mix routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OutputDestination {
    Master,
    GroupTrack(u32),
    SendsOnly,
    Resampling,
    External(u16),
}

/// Per-track auxiliary send state routing into a designated Return track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackSend {
    pub return_track_id: u32,
    pub level_db: f32, // -70.0 .. +6.0 dB (-70 = -inf silence)
    pub mode: SendRoutingMode,
}

impl TrackSend {
    pub fn new(return_track_id: u32, level_db: f32, mode: SendRoutingMode) -> Self {
        Self {
            return_track_id,
            level_db: level_db.clamp(-70.0, 6.0),
            mode,
        }
    }

    /// Linear gain factor.
    pub fn linear_gain(&self) -> f32 {
        if self.level_db <= -60.0 {
            0.0
        } else {
            10.0_f32.powf(self.level_db / 20.0)
        }
    }
}

/// Full multi-track workstation track model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: u32,
    pub name: String,
    pub track_type: TrackType,
    pub volume_db: f32,
    pub pan: f32, // -1.0 .. +1.0
    pub mute: bool,
    pub solo: bool,
    pub arm: bool,
    pub crossfade_assign: CrossfadeAssign,
    pub track_delay_ms: f32, // -100.0 .. +100.0 ms
    pub output_destination: OutputDestination,
    pub sends: Vec<TrackSend>,
    pub clips: Vec<ArrangementClip>,
    pub comping: TrackCompingManager,
    pub automation: MultiLaneAutomation,
}

impl Track {
    pub fn new(id: u32, name: impl Into<String>, track_type: TrackType) -> Self {
        Self {
            id,
            name: name.into(),
            track_type,
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            arm: false,
            crossfade_assign: CrossfadeAssign::Off,
            track_delay_ms: 0.0,
            output_destination: OutputDestination::Master,
            sends: Vec::new(),
            clips: Vec::new(),
            comping: TrackCompingManager::new(),
            automation: MultiLaneAutomation::new(),
        }
    }

    /// Set volume in decibels [-70.0, +6.0 dB].
    pub fn set_volume_db(&mut self, db: f32) {
        self.volume_db = db.clamp(-70.0, 6.0);
    }

    /// Linear volume multiplier.
    pub fn volume_linear(&self) -> f32 {
        if self.volume_db <= -60.0 {
            0.0
        } else {
            10.0_f32.powf(self.volume_db / 20.0)
        }
    }

    /// Set pan [-1.0, 1.0].
    pub fn set_pan(&mut self, pan: f32) {
        self.pan = pan.clamp(-1.0, 1.0);
    }

    /// Set track delay in milliseconds [-100.0, +100.0 ms].
    pub fn set_track_delay_ms(&mut self, ms: f32) {
        self.track_delay_ms = ms.clamp(-100.0, 100.0);
    }

    /// Set send level to a return track. Creates send if not present.
    pub fn set_send(&mut self, return_track_id: u32, level_db: f32, mode: SendRoutingMode) {
        if let Some(send) = self.sends.iter_mut().find(|s| s.return_track_id == return_track_id) {
            send.level_db = level_db.clamp(-70.0, 6.0);
            send.mode = mode;
        } else {
            self.sends.push(TrackSend::new(return_track_id, level_db, mode));
        }
    }

    /// Add an arrangement clip.
    pub fn add_clip(&mut self, clip: ArrangementClip) {
        self.clips.push(clip);
    }

    /// Comp a region from a take lane into this track's main arrangement clips with automatic micro-crossfades.
    pub fn comp_take_region(
        &mut self,
        lane_id: u32,
        start_beat: f64,
        end_beat: f64,
        crossfade_ms: f32,
    ) -> Result<usize, String> {
        self.comping
            .comp_region_to_main(&mut self.clips, lane_id, start_beat, end_beat, crossfade_ms)
    }

    /// Set or clear the actively auditioned take lane for this track.
    pub fn set_audition_take(&mut self, lane_id: Option<u32>) {
        self.comping.set_audition_take(lane_id);
    }

    /// Returns the effective clips for playback (auditioned take lane's clips if active, otherwise main clips).
    pub fn effective_clips(&self) -> &[ArrangementClip] {
        self.comping.get_effective_clips(&self.clips)
    }
}

/// Collection of unlimited tracks with return buses and DAG routing validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackManager {
    pub tracks: Vec<Track>,
    pub return_tracks: Vec<Track>,
    pub master_track: Track,
    pub crossfader_position: f32, // -1.0 (A) .. 0.0 (Center) .. +1.0 (B)
    next_track_id: u32,
}

impl Default for TrackManager {
    fn default() -> Self {
        let mut mgr = Self {
            tracks: Vec::new(),
            return_tracks: Vec::new(),
            master_track: Track::new(0, "Master", TrackType::Master),
            crossfader_position: 0.0,
            next_track_id: 1,
        };

        // Create standard default Live 12 return tracks: A-Reverb and B-Delay
        let rev_id = mgr.add_return_track("A-Reverb");
        let del_id = mgr.add_return_track("B-Delay");

        // Sync default sends
        mgr.sync_track_sends(&[rev_id, del_id]);
        mgr
    }
}

impl TrackManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a standard MIDI track.
    pub fn add_midi_track(&mut self, name: impl Into<String>) -> u32 {
        let id = self.next_track_id;
        self.next_track_id += 1;
        let mut trk = Track::new(id, name, TrackType::Midi);
        for ret in &self.return_tracks {
            trk.sends.push(TrackSend::new(ret.id, -70.0, SendRoutingMode::PostFader));
        }
        self.tracks.push(trk);
        id
    }

    /// Add a standard Audio track.
    pub fn add_audio_track(&mut self, name: impl Into<String>) -> u32 {
        let id = self.next_track_id;
        self.next_track_id += 1;
        let mut trk = Track::new(id, name, TrackType::Audio);
        for ret in &self.return_tracks {
            trk.sends.push(TrackSend::new(ret.id, -70.0, SendRoutingMode::PostFader));
        }
        self.tracks.push(trk);
        id
    }

    /// Add a Group track.
    pub fn add_group_track(&mut self, name: impl Into<String>) -> u32 {
        let id = self.next_track_id;
        self.next_track_id += 1;
        let mut trk = Track::new(id, name, TrackType::Group);
        for ret in &self.return_tracks {
            trk.sends.push(TrackSend::new(ret.id, -70.0, SendRoutingMode::PostFader));
        }
        self.tracks.push(trk);
        id
    }

    /// Add a Return track and update sends on all existing tracks.
    pub fn add_return_track(&mut self, name: impl Into<String>) -> u32 {
        let id = self.next_track_id;
        self.next_track_id += 1;
        let ret = Track::new(id, name, TrackType::Return);
        self.return_tracks.push(ret);

        for trk in &mut self.tracks {
            if !trk.sends.iter().any(|s| s.return_track_id == id) {
                trk.sends.push(TrackSend::new(id, -70.0, SendRoutingMode::PostFader));
            }
        }
        id
    }

    /// Synchronize send slots across all tracks to match current return tracks.
    fn sync_track_sends(&mut self, return_ids: &[u32]) {
        for trk in &mut self.tracks {
            for &ret_id in return_ids {
                if !trk.sends.iter().any(|s| s.return_track_id == ret_id) {
                    trk.sends.push(TrackSend::new(ret_id, -70.0, SendRoutingMode::PostFader));
                }
            }
        }
    }

    /// Set crossfader position in range [-1.0, 1.0].
    pub fn set_crossfader(&mut self, pos: f32) {
        self.crossfader_position = pos.clamp(-1.0, 1.0);
    }

    /// Check if any track or return track has solo engaged.
    pub fn is_any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo) || self.return_tracks.iter().any(|t| t.solo)
    }

    /// Find track by ID.
    pub fn get_track(&self, id: u32) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
            .or_else(|| self.return_tracks.iter().find(|t| t.id == id))
            .or_else(|| if self.master_track.id == id { Some(&self.master_track) } else { None })
    }

    /// Find mutable track by ID.
    pub fn get_track_mut(&mut self, id: u32) -> Option<&mut Track> {
        if self.master_track.id == id {
            return Some(&mut self.master_track);
        }
        if let Some(pos) = self.tracks.iter().position(|t| t.id == id) {
            return Some(&mut self.tracks[pos]);
        }
        if let Some(pos) = self.return_tracks.iter().position(|t| t.id == id) {
            return Some(&mut self.return_tracks[pos]);
        }
        None
    }

    /// Validate routing graph to prevent circular routing (e.g. Group A -> Group B -> Group A).
    pub fn validate_no_cycles(&self) -> Result<(), String> {
        for trk in &self.tracks {
            let mut visited = Vec::new();
            let mut cur_dest = &trk.output_destination;
            while let OutputDestination::GroupTrack(grp_id) = cur_dest {
                if visited.contains(grp_id) {
                    return Err(format!("Circular routing detected involving Group track {}", grp_id));
                }
                visited.push(*grp_id);
                if let Some(grp) = self.tracks.iter().find(|t| t.id == *grp_id) {
                    cur_dest = &grp.output_destination;
                } else {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_manager_unlimited_tracks_and_returns() {
        let mut mgr = TrackManager::new();
        assert_eq!(mgr.return_tracks.len(), 2, "Default must have A-Reverb and B-Delay");

        let t1 = mgr.add_midi_track("Lead Synth");
        let t2 = mgr.add_audio_track("Vocal");
        let t3 = mgr.add_group_track("Vocals Group");
        let ret3 = mgr.add_return_track("C-Shimmer");

        assert_eq!(mgr.tracks.len(), 3);
        assert_eq!(mgr.return_tracks.len(), 3);

        // Every track automatically gets sends for all 3 return tracks
        let trk1 = mgr.get_track(t1).unwrap();
        assert_eq!(trk1.sends.len(), 3);
        assert!(trk1.sends.iter().any(|s| s.return_track_id == ret3));

        // Group routing
        mgr.get_track_mut(t2).unwrap().output_destination = OutputDestination::GroupTrack(t3);
        assert!(mgr.validate_no_cycles().is_ok());

        // Test cycle detection
        mgr.get_track_mut(t3).unwrap().output_destination = OutputDestination::GroupTrack(t3);
        assert!(mgr.validate_no_cycles().is_err());
    }
}
