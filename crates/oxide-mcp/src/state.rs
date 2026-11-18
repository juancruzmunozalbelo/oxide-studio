//! DAW state container for the Model Context Protocol server.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use oxide_core::device::DeviceChain;
use oxide_core::scale::{GlobalScale, ScaleMode};
use oxide_core::AudioTelemetry;
use oxide_dsp::{SynthEngine, PARAMETER_DESCRIPTORS};
use oxide_drums::DrumRack;
use oxide_fx::{DelaySubdivision, FdnReverb, MasterLimiter, Mixer, Saturator, TempoDelay};
use oxide_project::{
    DAWProject, DrumRackDefinition, InstrumentConfig, TrackDefinition, TrackType,
};
use oxide_timeline::groove::GroovePool;
use oxide_timeline::track::{SendRoutingMode, Track as TimelineTrack, TrackManager, TrackType as TimelineTrackType};
use oxide_timeline::warping::{WarpMode, WarpedAudioClip};
use oxide_timeline::{ArrangementTimeline, AudioClipData, ClipPattern, SessionClip, SessionMatrix};

use crate::bridge::TcpLomClient;
use crate::browser::BrowserLibrary;
use crate::devices::{
    add_device_to_chain, from_insert_effect_config, ManagedChainHandles,
};

fn resolve_assets_dir() -> Option<PathBuf> {
    if let Ok(env_dir) = std::env::var("OXIDE_ASSETS_DIR") {
        let p = PathBuf::from(env_dir.trim());
        if p.exists() {
            return Some(p);
        }
    }
    for candidate in ["assets", "../../assets", "../../../assets"] {
        let p = Path::new(candidate);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

fn default_lom_client() -> TcpLomClient {
    let mut client = TcpLomClient::default();
    if let Ok(port_str) = std::env::var("OXIDE_LOM_PORT") {
        if let Ok(port) = port_str.trim().parse::<u16>() {
            client.port = port;
        }
    }
    client
}

fn parse_warp_mode_str(s: &str) -> WarpMode {
    match s.trim().to_ascii_lowercase().as_str() {
        "tones" => WarpMode::Tones,
        "texture" => WarpMode::Texture,
        "repitch" | "re-pitch" => WarpMode::RePitch,
        "complex" => WarpMode::Complex,
        "complexpro" | "complex_pro" => WarpMode::ComplexPro,
        _ => WarpMode::Beats,
    }
}

fn timeline_track_from_project(idx: usize, trk: &TrackDefinition) -> TimelineTrack {
    let t_type = match trk.track_type {
        TrackType::Audio => TimelineTrackType::Audio,
        TrackType::Group => TimelineTrackType::Group,
        TrackType::Return => TimelineTrackType::Return,
        TrackType::MidiSynth | TrackType::DrumRack => TimelineTrackType::Midi,
    };
    let mut tl = TimelineTrack::new(idx as u32, &trk.name, t_type);
    tl.volume_db = trk.mixer.volume_db;
    tl.pan = trk.mixer.pan;
    tl.mute = trk.mixer.mute;
    tl.solo = trk.mixer.solo;
    tl.arm = trk.mixer.arm;
    for (s_idx, db) in trk.mixer.effective_sends_db().into_iter().enumerate() {
        tl.set_send(s_idx as u32, db, SendRoutingMode::PostFader);
    }
    tl
}

pub struct DawState {
    pub project: DAWProject,
    pub synth: SynthEngine,
    pub drum_rack: DrumRack,
    pub mixer: Mixer,
    pub saturator: Saturator,
    pub delay: TempoDelay,
    pub reverb: FdnReverb,
    pub limiter: MasterLimiter,
    pub sample_rate: f32,
    pub is_playing: bool,
    pub playhead_beat: f64,
    pub bpm: f64,
    pub swing: f32,
    pub matrix: SessionMatrix,
    pub timeline: ArrangementTimeline,
    pub last_saved_path: Option<String>,
    pub groove_pool: GroovePool,
    pub track_manager: TrackManager,
    pub scale: GlobalScale,
    pub browser: BrowserLibrary,
    pub audio_clips: HashMap<(u32, u32), WarpedAudioClip>,
    pub device_chains: HashMap<u32, DeviceChain>,
    pub device_handles: HashMap<u32, ManagedChainHandles>,
    pub return_device_chains: HashMap<u32, DeviceChain>,
    pub return_device_handles: HashMap<u32, ManagedChainHandles>,
    pub custom_return_ids: Vec<u32>,
    pub active_synth_track_id: usize,
    pub active_drum_track_id: usize,
    pub master_saturator_active: bool,
    pub master_delay_active: bool,
    pub master_reverb_active: bool,
    pub lom_client: TcpLomClient,
}

impl DawState {
    fn create_default_drum_rack(sample_rate: f32) -> DrumRack {
        let mut drum_rack = DrumRack::new(sample_rate);
        if let Some(assets_dir) = resolve_assets_dir() {
            if let Some(dir_str) = assets_dir.to_str() {
                drum_rack.load_default_909_samples(dir_str);
            }
        }
        drum_rack
    }

    pub fn new() -> Self {
        let sample_rate = 44100.0;
        let mut project = DAWProject::new("Ableton MCP Session");

        // Add 2 default tracks: 0 = Lead Synth, 1 = 909 Drum Rack
        let synth_track = TrackDefinition::new_synth(0, "Lead Synth");
        let drum_track = TrackDefinition::new_drum_rack(1, "909 Drum Rack");
        project.tracks.push(synth_track);
        project.tracks.push(drum_track);

        let mut synth = SynthEngine::new(sample_rate);
        synth.load_preset("wowbass.preset");

        let drum_rack = Self::create_default_drum_rack(sample_rate);

        let matrix = SessionMatrix::new(2, 4);
        let mut timeline = ArrangementTimeline::new();
        timeline
            .tracks
            .push(oxide_timeline::ArrangementTrack::new(0, "Lead Synth", "MidiSynth"));
        timeline
            .tracks
            .push(oxide_timeline::ArrangementTrack::new(1, "909 Drum Rack", "DrumRack"));

        let mut track_manager = TrackManager::new();
        for (i, trk) in project.tracks.iter().enumerate() {
            track_manager.tracks.push(timeline_track_from_project(i, trk));
        }

        let groove_pool = GroovePool::new();
        let scale = GlobalScale::new(5, ScaleMode::Dorian);
        let browser = BrowserLibrary::new_factory();

        let mut state = Self {
            project,
            synth,
            drum_rack,
            mixer: Mixer::new(sample_rate),
            saturator: Saturator::new(),
            delay: TempoDelay::new(sample_rate, 120.0),
            reverb: FdnReverb::new(sample_rate),
            limiter: MasterLimiter::new(sample_rate),
            sample_rate,
            is_playing: false,
            playhead_beat: 0.0,
            bpm: 120.0,
            swing: 0.0,
            matrix,
            timeline,
            last_saved_path: None,
            groove_pool,
            track_manager,
            scale,
            browser,
            audio_clips: HashMap::new(),
            device_chains: HashMap::new(),
            device_handles: HashMap::new(),
            return_device_chains: HashMap::new(),
            return_device_handles: HashMap::new(),
            custom_return_ids: Vec::new(),
            active_synth_track_id: 0,
            active_drum_track_id: 1,
            master_saturator_active: false,
            master_delay_active: false,
            master_reverb_active: false,
            lom_client: default_lom_client(),
        };
        state.sync_synth_params_to_project();
        state
    }

    /// Reset project to fresh empty session with given name.
    pub fn reset_project(&mut self, name: &str) {
        self.project = DAWProject::new(name);
        self.matrix = SessionMatrix::new(2, 4);
        self.timeline = ArrangementTimeline::new();
        self.playhead_beat = 0.0;
        self.bpm = 120.0;
        self.swing = 0.0;
        self.is_playing = false;

        let synth_track = TrackDefinition::new_synth(0, "Lead Synth");
        let drum_track = TrackDefinition::new_drum_rack(1, "909 Drum Rack");
        self.project.tracks.push(synth_track);
        self.project.tracks.push(drum_track);

        self.timeline
            .tracks
            .push(oxide_timeline::ArrangementTrack::new(0, "Lead Synth", "MidiSynth"));
        self.timeline
            .tracks
            .push(oxide_timeline::ArrangementTrack::new(1, "909 Drum Rack", "DrumRack"));

        self.synth = SynthEngine::new(self.sample_rate);
        self.synth.load_preset("wowbass.preset");
        self.drum_rack = Self::create_default_drum_rack(self.sample_rate);
        self.mixer = Mixer::new(self.sample_rate);
        self.saturator = Saturator::new();
        self.delay = TempoDelay::new(self.sample_rate, 120.0);
        self.reverb = FdnReverb::new(self.sample_rate);
        self.limiter = MasterLimiter::new(self.sample_rate);
        self.groove_pool = GroovePool::new();
        self.track_manager = TrackManager::new();
        for (i, trk) in self.project.tracks.iter().enumerate() {
            self.track_manager
                .tracks
                .push(timeline_track_from_project(i, trk));
        }
        self.scale = GlobalScale::new(5, ScaleMode::Dorian);
        self.browser = BrowserLibrary::new_factory();
        self.audio_clips.clear();
        self.device_chains.clear();
        self.device_handles.clear();
        self.return_device_chains.clear();
        self.return_device_handles.clear();
        self.custom_return_ids.clear();
        self.active_synth_track_id = 0;
        self.active_drum_track_id = 1;
        self.master_saturator_active = false;
        self.master_delay_active = false;
        self.master_reverb_active = false;
        self.lom_client = default_lom_client();
        self.sync_synth_params_to_project();
    }

    /// Ensure `self.track_manager.tracks` has an entry matching `project.tracks[track_idx]`
    /// and return a mutable reference to it. Returns `None` if `track_idx` does not exist.
    pub fn ensure_timeline_track_mut(&mut self, track_idx: usize) -> Option<&mut TimelineTrack> {
        if track_idx >= self.project.tracks.len() {
            return None;
        }
        while self.track_manager.tracks.len() <= track_idx {
            let i = self.track_manager.tracks.len();
            let trk = &self.project.tracks[i];
            self.track_manager
                .tracks
                .push(timeline_track_from_project(i, trk));
        }
        self.track_manager.tracks.get_mut(track_idx)
    }

    /// Remove a track by index across all internal state containers so deleted tracks never resurrect.
    pub fn delete_track_at(&mut self, track_idx: usize) -> Result<TrackDefinition, String> {
        if track_idx >= self.project.tracks.len() {
            return Err(format!("Track index {} out of range", track_idx));
        }

        let removed = self.project.tracks.remove(track_idx);
        for (i, trk) in self.project.tracks.iter_mut().enumerate() {
            trk.id = i as u32;
        }

        if track_idx < self.matrix.tracks.len() {
            self.matrix.tracks.remove(track_idx);
            for (i, trk) in self.matrix.tracks.iter_mut().enumerate() {
                trk.id = i as u32;
            }
        }

        if track_idx < self.timeline.tracks.len() {
            self.timeline.tracks.remove(track_idx);
            for (i, trk) in self.timeline.tracks.iter_mut().enumerate() {
                trk.id = i as u32;
            }
        }

        if track_idx < self.mixer.tracks.len() {
            for i in track_idx..(self.mixer.tracks.len() - 1) {
                self.mixer.tracks[i] = self.mixer.tracks[i + 1].clone();
            }
            let last = self.mixer.tracks.len() - 1;
            self.mixer.tracks[last] = oxide_fx::MixerTrack::new(&format!("Track {}", last + 1));
        }

        if track_idx < self.track_manager.tracks.len() {
            self.track_manager.tracks.remove(track_idx);
            for (i, trk) in self.track_manager.tracks.iter_mut().enumerate() {
                trk.id = i as u32;
            }
        }

        // Re-key device_chains
        let old_chains = std::mem::take(&mut self.device_chains);
        for (k, v) in old_chains {
            let ku = k as usize;
            if ku < track_idx {
                self.device_chains.insert(k, v);
            } else if ku > track_idx {
                self.device_chains.insert((ku - 1) as u32, v);
            }
        }

        // Re-key device_handles
        let old_handles = std::mem::take(&mut self.device_handles);
        for (k, v) in old_handles {
            let ku = k as usize;
            if ku < track_idx {
                self.device_handles.insert(k, v);
            } else if ku > track_idx {
                self.device_handles.insert((ku - 1) as u32, v);
            }
        }

        // Re-key audio_clips
        let old_clips = std::mem::take(&mut self.audio_clips);
        for ((t, s), v) in old_clips {
            let tu = t as usize;
            if tu < track_idx {
                self.audio_clips.insert((t, s), v);
            } else if tu > track_idx {
                self.audio_clips.insert(((tu - 1) as u32, s), v);
            }
        }

        // Reconcile active synth/drum track cursors
        if self.active_synth_track_id == track_idx {
            self.active_synth_track_id = self
                .project
                .tracks
                .iter()
                .position(|t| matches!(t.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_)))
                .unwrap_or(0);
        } else if self.active_synth_track_id > track_idx {
            self.active_synth_track_id -= 1;
        }

        if self.active_drum_track_id == track_idx {
            self.active_drum_track_id = self
                .project
                .tracks
                .iter()
                .position(|t| matches!(t.instrument, InstrumentConfig::ElectronicDrumRack(_)))
                .unwrap_or(0);
        } else if self.active_drum_track_id > track_idx {
            self.active_drum_track_id -= 1;
        }

        Ok(removed)
    }

    /// Push live `self.synth` parameters onto the active synth track in `self.project`.
    pub fn sync_synth_params_to_project(&mut self) {
        let target_idx = self
            .project
            .tracks
            .get(self.active_synth_track_id)
            .filter(|t| matches!(t.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_)))
            .map(|_| self.active_synth_track_id)
            .or_else(|| {
                self.project
                    .tracks
                    .iter()
                    .position(|t| matches!(t.instrument, InstrumentConfig::BuiltinSubtractiveSynth(_)))
            });

        if let Some(idx) = target_idx {
            if let InstrumentConfig::BuiltinSubtractiveSynth(ref mut p) =
                self.project.tracks[idx].instrument
            {
                for (i, desc) in PARAMETER_DESCRIPTORS.iter().enumerate() {
                    p.set_parameter(desc.name, self.synth.params.values[i]);
                }
            }
        }
    }

    /// Push live `self.drum_rack` pad parameters onto the active drum rack track in `self.project`.
    pub fn sync_drum_rack_to_project(&mut self) {
        let target_idx = self
            .project
            .tracks
            .get(self.active_drum_track_id)
            .filter(|t| matches!(t.instrument, InstrumentConfig::ElectronicDrumRack(_)))
            .map(|_| self.active_drum_track_id)
            .or_else(|| {
                self.project
                    .tracks
                    .iter()
                    .position(|t| matches!(t.instrument, InstrumentConfig::ElectronicDrumRack(_)))
            });

        if let Some(idx) = target_idx {
            if let InstrumentConfig::ElectronicDrumRack(ref mut dr_def) =
                self.project.tracks[idx].instrument
            {
                for (pad_idx, pad) in self.drum_rack.pads.iter().enumerate() {
                    if let Some(pad_def) = dr_def.pads.get_mut(pad_idx) {
                        pad_def.volume_db = if pad.volume <= 1e-5 {
                            -60.0
                        } else {
                            (20.0 * pad.volume.log10()).clamp(-60.0, 12.0)
                        };
                        pad_def.pan = pad.pan;
                        pad_def.pitch_offset_semitones = pad.pitch_semitones;
                        pad_def.decay_ms = (pad.decay_seconds * 1000.0).max(5.0);
                        pad_def.choke_group = pad.choke_group;
                        pad_def.mute = pad.mute;
                        pad_def.solo = pad.solo;
                    }
                }
            }
        }
    }

    /// Synchronize project document with live state before saving, exporting, or rendering.
    pub fn sync_project_from_state(&mut self) {
        self.project.transport.bpm = self.bpm;
        self.project.transport.swing_16th_ratio = self.swing;

        // Only auto-expand tracks if actual non-empty clips or audio clips were placed past `project.tracks.len()`.
        let max_matrix_nonempty = self
            .matrix
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.slots.iter().any(|s| s.clip.is_some()))
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0);
        let max_timeline_nonempty = self
            .timeline
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.clips.is_empty() || !t.automation_lanes.is_empty())
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0);
        let max_audio_clip_track = self
            .audio_clips
            .keys()
            .map(|(t, _)| (*t as usize) + 1)
            .max()
            .unwrap_or(0);

        let max_track_idx = self
            .project
            .tracks
            .len()
            .max(max_matrix_nonempty)
            .max(max_timeline_nonempty)
            .max(max_audio_clip_track);

        while self.project.tracks.len() < max_track_idx {
            let tid = self.project.tracks.len() as u32;
            self.project
                .tracks
                .push(TrackDefinition::new_synth(tid, format!("Track {}", tid)));
        }

        // Unconditionally sync mixer channel states (including resetting back to 0.0 / unmuted)
        for (i, trk) in self.project.tracks.iter_mut().enumerate() {
            if trk.track_type == TrackType::Return {
                continue;
            }
            if let Some(m_trk) = self.mixer.tracks.get(i) {
                trk.mixer.volume_db = m_trk.volume_db;
                trk.mixer.pan = m_trk.pan;
                trk.mixer.mute = m_trk.mute;
                trk.mixer.solo = m_trk.solo;
            }
            if let Some(tm_trk) = self.track_manager.tracks.get(i) {
                for send in &tm_trk.sends {
                    trk.mixer
                        .set_send_db(send.return_track_id as usize, send.level_db);
                }
            }
        }

        // Sync active SynthEngine & DrumRack state
        self.sync_synth_params_to_project();
        self.sync_drum_rack_to_project();

        // Sync per-track DeviceChain handles -> insert_fx
        for (&track_id, handles) in &self.device_handles {
            if let Some(p_trk) = self.project.tracks.get_mut(track_id as usize) {
                p_trk.insert_fx = handles
                    .audio_effects
                    .iter()
                    .filter_map(|h| h.to_insert_effect_config())
                    .collect();
            }
        }

        // Sync custom Return tracks & their FX into project.tracks
        for &ret_id in &self.custom_return_ids {
            if let Some(ret_trk) = self
                .track_manager
                .return_tracks
                .iter()
                .find(|r| r.id == ret_id)
            {
                let insert_fx = self
                    .return_device_handles
                    .get(&ret_id)
                    .map(|hs| {
                        hs.audio_effects
                            .iter()
                            .filter_map(|h| h.to_insert_effect_config())
                            .collect()
                    })
                    .unwrap_or_default();

                if let Some(existing) = self
                    .project
                    .tracks
                    .iter_mut()
                    .find(|t| t.track_type == TrackType::Return && t.name == ret_trk.name)
                {
                    existing.mixer.volume_db = ret_trk.volume_db;
                    existing.mixer.pan = ret_trk.pan;
                    existing.mixer.mute = ret_trk.mute;
                    existing.mixer.solo = ret_trk.solo;
                    existing.insert_fx = insert_fx;
                } else {
                    let tid = self.project.tracks.len() as u32;
                    let mut p_ret = TrackDefinition::new_return(tid, &ret_trk.name);
                    p_ret.mixer.volume_db = ret_trk.volume_db;
                    p_ret.mixer.pan = ret_trk.pan;
                    p_ret.mixer.mute = ret_trk.mute;
                    p_ret.mixer.solo = ret_trk.solo;
                    p_ret.insert_fx = insert_fx;
                    self.project.tracks.push(p_ret);
                }
            }
        }

        // Sync session matrix clips into project tracks
        for (i, m_track) in self.matrix.tracks.iter().enumerate() {
            if let Some(p_track) = self.project.tracks.get_mut(i) {
                for (slot_idx, slot) in m_track.slots.iter().enumerate() {
                    if let Some(clip) = &slot.clip {
                        while p_track.session_clips.len() <= slot_idx {
                            p_track.session_clips.push(None);
                        }
                        p_track.session_clips[slot_idx] = Some(clip.clone());
                    }
                }
            }
        }

        // Sync warped audio clips into project tracks
        for (&(track_id, slot_id), w_clip) in &self.audio_clips {
            if let Some(p_track) = self.project.tracks.get_mut(track_id as usize) {
                p_track.track_type = TrackType::Audio;
                p_track.instrument = InstrumentConfig::None;
                let slot_idx = slot_id as usize;
                while p_track.session_clips.len() <= slot_idx {
                    p_track.session_clips.push(None);
                }
                let clip_id = (track_id * 100) + slot_id + 1;
                let name = Path::new(&w_clip.name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Audio Clip")
                    .to_string();
                let audio_data = AudioClipData {
                    file_path: w_clip.name.clone(),
                    sample_rate: w_clip.sample_rate,
                    channels: w_clip.channels,
                    length_beats: w_clip.length_beats,
                    gain_db: 0.0,
                    warp_mode: format!("{:?}", w_clip.warper.config.mode),
                    samples: w_clip.raw_samples.clone(),
                };
                p_track.session_clips[slot_idx] = Some(SessionClip::new(
                    clip_id,
                    name,
                    ClipPattern::Audio(audio_data),
                ));
            }
        }

        // Sync arrangement timeline clips and automation lanes into project tracks
        for (i, a_track) in self.timeline.tracks.iter().enumerate() {
            if let Some(p_track) = self.project.tracks.get_mut(i) {
                if !a_track.clips.is_empty() {
                    p_track.arrangement_clips = a_track.clips.clone();
                }
                if !a_track.automation_lanes.is_empty() {
                    p_track.automation_lanes = a_track.automation_lanes.clone();
                }
            }
        }

        // Sync Master FX -> MasterTrackDefinition
        let m = &mut self.project.master_track;
        m.limiter_ceiling_db = 20.0 * self.limiter.ceiling.max(1e-5).log10();
        m.saturator_enabled = self.master_saturator_active;
        if self.master_saturator_active {
            m.saturator_drive_db = 20.0 * self.saturator.drive_linear.max(1e-5).log10();
            m.saturator_output_db = 20.0 * self.saturator.output_gain.max(1e-5).log10();
            m.saturator_curve = self.saturator.wet;
        }
        if self.master_delay_active {
            m.delay_time_beats = self.delay.subdivision.beats() as f32;
            m.delay_feedback = self.delay.feedback;
            m.delay_mix = self.delay.wet;
        }
        if self.master_reverb_active {
            m.reverb_decay_ms = self.reverb.decay * 1000.0;
            m.reverb_mix = self.reverb.wet;
        }
    }

    /// Clear and rebuild live DAW runtime state from the loaded `self.project`.
    pub fn sync_state_from_project(&mut self) {
        self.bpm = self.project.transport.bpm;
        self.swing = self.project.transport.swing_16th_ratio;
        self.is_playing = false;
        self.playhead_beat = 0.0;

        self.mixer = Mixer::new(self.sample_rate);
        self.saturator = Saturator::new();
        self.delay = TempoDelay::new(self.sample_rate, self.bpm);
        self.reverb = FdnReverb::new(self.sample_rate);
        self.limiter = MasterLimiter::new(self.sample_rate);

        // Restore Master FX
        let m = &self.project.master_track;
        self.limiter.ceiling = 10.0_f32.powf(m.limiter_ceiling_db.clamp(-24.0, 0.0) / 20.0);
        self.master_saturator_active = m.saturator_enabled;
        if m.saturator_enabled {
            self.saturator.set_drive_db(m.saturator_drive_db);
            self.saturator.set_output_gain_db(m.saturator_output_db);
            if m.saturator_curve > 0.0 {
                self.saturator.wet = m.saturator_curve.clamp(0.0, 1.0);
            }
        }
        self.master_delay_active = m.delay_mix > 0.0;
        if self.master_delay_active {
            self.delay.feedback = m.delay_feedback.clamp(0.0, 0.98);
            self.delay.wet = m.delay_mix.clamp(0.0, 1.0);
            let sub = if (m.delay_time_beats - 0.25).abs() < 0.05 {
                DelaySubdivision::Sixteenth
            } else if (m.delay_time_beats - 0.375).abs() < 0.05 {
                DelaySubdivision::SixteenthDotted
            } else if (m.delay_time_beats - 0.5).abs() < 0.05 {
                DelaySubdivision::Eighth
            } else if (m.delay_time_beats - 1.0).abs() < 0.1 {
                DelaySubdivision::Quarter
            } else {
                DelaySubdivision::EighthDotted
            };
            self.delay.set_subdivision(sub);
        }
        self.master_reverb_active = m.reverb_mix > 0.0;
        if self.master_reverb_active {
            self.reverb.wet = m.reverb_mix.clamp(0.0, 1.0);
            if m.reverb_decay_ms > 0.0 {
                self.reverb.decay = (m.reverb_decay_ms / 1000.0).clamp(0.1, 0.98);
            }
        }

        // Clear all clip, device, and track containers cleanly before repopulating
        self.audio_clips.clear();
        self.device_chains.clear();
        self.device_handles.clear();
        self.return_device_chains.clear();
        self.return_device_handles.clear();
        self.custom_return_ids.clear();
        self.track_manager = TrackManager::new();
        self.timeline = ArrangementTimeline::new();
        self.synth = SynthEngine::new(self.sample_rate);
        self.drum_rack = Self::create_default_drum_rack(self.sample_rate);

        let non_return_count = self
            .project
            .tracks
            .iter()
            .filter(|t| t.track_type != TrackType::Return)
            .count()
            .max(1);
        let max_slots = self
            .project
            .tracks
            .iter()
            .map(|t| t.session_clips.len())
            .max()
            .unwrap_or(4)
            .max(4);
        self.matrix = SessionMatrix::new(non_return_count, max_slots);
        if !self.project.scenes.is_empty() {
            self.matrix.scenes = self.project.scenes.clone();
        }

        let mut loaded_first_synth = false;
        let mut loaded_first_drum = false;

        for (i, trk) in self.project.tracks.iter().enumerate() {
            if trk.track_type == TrackType::Return {
                let ret_id = self.track_manager.add_return_track(&trk.name);
                self.custom_return_ids.push(ret_id);
                if let Some(rt) = self
                    .track_manager
                    .return_tracks
                    .iter_mut()
                    .find(|r| r.id == ret_id)
                {
                    rt.volume_db = trk.mixer.volume_db;
                    rt.pan = trk.mixer.pan;
                    rt.mute = trk.mixer.mute;
                    rt.solo = trk.mixer.solo;
                }
                let chain = self.return_device_chains.entry(ret_id).or_default();
                let handles = self.return_device_handles.entry(ret_id).or_default();
                for fx_cfg in &trk.insert_fx {
                    if let Ok((dev, handle)) = from_insert_effect_config(fx_cfg, self.sample_rate) {
                        let cat = handle.category();
                        add_device_to_chain(chain, dev, None);
                        handles.add_handle(cat, handle, None);
                    }
                }
                continue;
            }

            // Mixer
            if let Some(m_trk) = self.mixer.tracks.get_mut(i) {
                m_trk.name = trk.name.clone();
                m_trk.set_volume_db(trk.mixer.volume_db);
                m_trk.set_pan(trk.mixer.pan);
                m_trk.mute = trk.mixer.mute;
                m_trk.solo = trk.mixer.solo;
            }

            // TrackManager
            self.track_manager
                .tracks
                .push(timeline_track_from_project(i, trk));

            // Timeline
            let type_label = match trk.track_type {
                TrackType::MidiSynth => "MidiSynth",
                TrackType::DrumRack => "DrumRack",
                TrackType::Audio => "Audio",
                TrackType::Group => "Group",
                TrackType::Return => "Return",
            };
            let mut arr_trk = oxide_timeline::ArrangementTrack::new(i as u32, &trk.name, type_label);
            arr_trk.clips = trk.arrangement_clips.clone();
            arr_trk.automation_lanes = trk.automation_lanes.clone();
            self.timeline.tracks.push(arr_trk);

            // Session matrix clips & warped audio clips
            if let Some(m_track) = self.matrix.tracks.get_mut(i) {
                m_track.id = i as u32;
                m_track.name = trk.name.clone();
                for (slot_idx, maybe_clip) in trk.session_clips.iter().enumerate() {
                    if let Some(clip) = maybe_clip {
                        while m_track.slots.len() <= slot_idx {
                            m_track.slots.push(oxide_timeline::SessionSlot::default());
                        }
                        m_track.slots[slot_idx].set_clip(Some(clip.clone()));
                        if let ClipPattern::Audio(ref a_data) = clip.pattern {
                            let mut w_clip = WarpedAudioClip::new(
                                a_data.file_path.clone(),
                                a_data.sample_rate.max(8000),
                                a_data.channels.max(1),
                                a_data.samples.clone(),
                                self.bpm,
                            );
                            w_clip.length_beats = a_data.length_beats;
                            w_clip.warper.config.mode = parse_warp_mode_str(&a_data.warp_mode);
                            self.audio_clips.insert((i as u32, slot_idx as u32), w_clip);
                        }
                    }
                }
            }

            // Instrument parameters -> Synth / DrumRack
            match &trk.instrument {
                InstrumentConfig::BuiltinSubtractiveSynth(patch) if !loaded_first_synth => {
                    loaded_first_synth = true;
                    self.active_synth_track_id = i;
                    if let Some(ref pname) = patch.preset_name {
                        let _ = self.synth.load_preset(pname);
                    }
                    for (pname, val) in patch.to_parameter_map() {
                        self.synth.set_parameter(&pname, val);
                    }
                }
                InstrumentConfig::ElectronicDrumRack(dr_def) if !loaded_first_drum => {
                    loaded_first_drum = true;
                    self.active_drum_track_id = i;
                    Self::apply_drum_def_to_rack(&mut self.drum_rack, dr_def);
                }
                _ => {}
            }

            // Insert FX -> DeviceChain & ManagedChainHandles
            if !trk.insert_fx.is_empty() {
                let chain = self.device_chains.entry(i as u32).or_default();
                let handles = self.device_handles.entry(i as u32).or_default();
                for fx_cfg in &trk.insert_fx {
                    if let Ok((dev, handle)) = from_insert_effect_config(fx_cfg, self.sample_rate) {
                        let cat = handle.category();
                        add_device_to_chain(chain, dev, None);
                        handles.add_handle(cat, handle, None);
                    }
                }
            }
        }
    }

    fn apply_drum_def_to_rack(rack: &mut DrumRack, dr_def: &DrumRackDefinition) {
        for (idx, pad_def) in dr_def.pads.iter().enumerate() {
            if let Some(pad) = rack.pads.get_mut(idx) {
                pad.volume = if pad_def.volume_db <= -59.5 {
                    0.0
                } else {
                    10.0_f32.powf(pad_def.volume_db / 20.0)
                };
                pad.pan = pad_def.pan;
                pad.pitch_semitones = pad_def.pitch_offset_semitones;
                pad.decay_seconds = (pad_def.decay_ms / 1000.0).max(0.005);
                pad.choke_group = pad_def.choke_group;
                pad.mute = pad_def.mute;
                pad.solo = pad_def.solo;
            }
        }
    }

    /// Query live telemetry snapshot.
    pub fn get_telemetry(&self) -> AudioTelemetry {
        let mut telemetry = AudioTelemetry::new();
        telemetry.playhead_beat = self.playhead_beat;
        telemetry.cpu_load = 0.05;

        for (i, trk) in self.mixer.tracks.iter().enumerate().take(16) {
            telemetry.track_peaks_l[i] = trk.peak_l;
            telemetry.track_peaks_r[i] = trk.peak_r;
        }

        telemetry.master_peak_l = self.mixer.master_peak_l;
        telemetry.master_peak_r = self.mixer.master_peak_r;
        telemetry.master_rms_l = self.mixer.master_rms_l;
        telemetry.master_rms_r = self.mixer.master_rms_r;

        telemetry
    }
}

impl Default for DawState {
    fn default() -> Self {
        Self::new()
    }
}
