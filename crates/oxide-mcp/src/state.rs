//! DAW state container for the Model Context Protocol server.

use std::collections::HashMap;
use std::path::Path;
use oxide_core::device::DeviceChain;
use oxide_core::scale::{GlobalScale, ScaleMode};
use oxide_core::AudioTelemetry;
use oxide_dsp::SynthEngine;
use oxide_drums::DrumRack;
use oxide_fx::{FdnReverb, MasterLimiter, Mixer, Saturator, TempoDelay};
use oxide_project::{DAWProject, TrackDefinition};
use oxide_timeline::groove::GroovePool;
use oxide_timeline::track::TrackManager;
use oxide_timeline::warping::WarpedAudioClip;
use oxide_timeline::{ArrangementTimeline, SessionMatrix};

use crate::bridge::TcpLomClient;
use crate::browser::BrowserLibrary;

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
    pub lom_client: TcpLomClient,
}

impl DawState {
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

        let mut drum_rack = DrumRack::new(sample_rate);
        let assets_dir = "/Users/juancruz/ableton-rust/assets";
        if Path::new(assets_dir).exists() {
            drum_rack.load_default_909_samples(assets_dir);
        } else if Path::new("/Users/juancruz/ableton-rea/public/sounds").exists() {
            drum_rack.load_default_909_samples("/Users/juancruz/ableton-rea/public/sounds");
        }

        let matrix = SessionMatrix::new(2, 4);
        let timeline = ArrangementTimeline::new();
        let groove_pool = GroovePool::new();
        let track_manager = TrackManager::new();
        let scale = GlobalScale::new(5, ScaleMode::Dorian);
        let browser = BrowserLibrary::new_factory();
        let audio_clips = HashMap::new();
        let device_chains = HashMap::new();
        let lom_client = TcpLomClient::default();

        Self {
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
            audio_clips,
            device_chains,
            lom_client,
        }
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

        // Default tracks
        let synth_track = TrackDefinition::new_synth(0, "Lead Synth");
        let drum_track = TrackDefinition::new_drum_rack(1, "909 Drum Rack");
        self.project.tracks.push(synth_track);
        self.project.tracks.push(drum_track);

        self.synth.reset();
        self.synth.load_preset("wowbass.preset");
        self.mixer = Mixer::new(self.sample_rate);
        self.groove_pool = GroovePool::new();
        self.track_manager = TrackManager::new();
        self.scale = GlobalScale::new(5, ScaleMode::Dorian);
        self.browser = BrowserLibrary::new_factory();
        self.audio_clips.clear();
        self.device_chains.clear();
        self.lom_client = TcpLomClient::default();
    }

    /// Synchronize project document with live state before saving.
    pub fn sync_project_from_state(&mut self) {
        self.project.transport.bpm = self.bpm;
        self.project.transport.swing_16th_ratio = self.swing;
        // Sync mixer tracks
        for (i, trk) in self.project.tracks.iter_mut().enumerate() {
            if let Some(m_trk) = self.mixer.tracks.get(i) {
                trk.mixer.volume_db = m_trk.volume_db;
                trk.mixer.pan = m_trk.pan;
                trk.mixer.mute = m_trk.mute;
                trk.mixer.solo = m_trk.solo;
            }
        }
    }

    /// Synchronize live state from loaded project.
    pub fn sync_state_from_project(&mut self) {
        self.bpm = self.project.transport.bpm;
        self.swing = self.project.transport.swing_16th_ratio;
        for (i, trk) in self.project.tracks.iter().enumerate() {
            if i < self.mixer.tracks.len() {
                self.mixer.tracks[i].set_volume_db(trk.mixer.volume_db);
                self.mixer.tracks[i].set_pan(trk.mixer.pan);
                self.mixer.tracks[i].mute = trk.mixer.mute;
                self.mixer.tracks[i].solo = trk.mixer.solo;
            }
        }
    }

    /// Query live telemetry snapshot.
    pub fn get_telemetry(&self) -> AudioTelemetry {
        let mut telemetry = AudioTelemetry::new();
        telemetry.playhead_beat = self.playhead_beat;
        telemetry.cpu_load = 0.05; // Nominal CPU load

        // Copy track peak levels from mixer
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
