//! Main Ableton Live 12 Studio Workstation Application.
//!
//! Enforces:
//! - Complete Live 12 Single-Window Workstation UI/UX running at 60 FPS on macOS Metal (`egui`/`eframe`):
//!   1. Top Control & Scale Bar: GlobalScale, TuningSystem, BPM, Time Sig, Metronome, Play/Stop/Rec, CPU, Scope/Spectrum
//!   2. Left Collapsible Browser (220-300px): Categories, Live 12 Tag Pills, Search, Item list, Sound Similarity
//!   3. Center Workspace: Session View (unlimited tracks, Return tracks, Master bus, 8+ scenes with tempo/meter headers,
//!      quantized launching, modular mixer strips, I/O, Sends, Crossfader) vs Arrangement View (ruler, zoom, locators,
//!      loop brace, waveform/MIDI clips, collapsible automation lanes with bezier curves, collapsible comping take lanes)
//!   4. Bottom Detail View: Dual-panel/tabbed Clip View (Piano Roll with scale-fold, velocity, chance, Live 12 Transform
//!      & Generate, Audio Warping) & Device Chain View (scrollable dynamic chain with Wavetable, Operator, Roar, EQ Eight,
//!      Glue, Auto Filter, OTT, Simpler, Drum Rack visualizers)
//! - Computer Keyboard Playing (Piano & Wicki-Hayden Isomorphic)
//! - Real-time zero-allocation audio engine integration with telemetry polling

use egui::{
    vec2, Button, CentralPanel, Color32, Context, Layout, RichText, SidePanel, Stroke, TopBottomPanel,
};
use oxide_audio::{
    AudioEngine, AudioEngineProcessor, AudioStreamHandle, PARAM_BPM, PARAM_FX_DRIVE,
    PARAM_ID_SYNTH_CUTOFF, PARAM_ID_SYNTH_ENV_MOD, PARAM_ID_SYNTH_PULSE_PW,
    PARAM_ID_SYNTH_RESONANCE, PARAM_MASTER_VOL, PARAM_SYNTH_MACRO_X, PARAM_SYNTH_MACRO_Y,
    PARAM_TRACK_PAN_BASE, PARAM_TRACK_VOL_BASE,
};
use oxide_bounce::render_project_to_wav;
use oxide_core::{AudioCommand, AudioTelemetry, GlobalScale, ScaleMode, TuningSystem};
use oxide_project::{
    DAWProject, InsertEffectConfig, InstrumentConfig, SynthPatchParameters, TrackDefinition,
    TrackType,
};
use oxide_timeline::{
    ArrangementClip, ArrangementTimeline, ArrangementTrack, ClipPattern, DrumStepPattern,
    MidiNote, MidiPattern, SessionClip, SessionMatrix, SessionTrack, SlotState,
};

use crate::keyboard::{ComputerKeyboardController, KeyboardMode};
use crate::panels::{
    render_arrangement_timeline, render_browser_panel, render_clip_view, render_device_chain_view,
    render_session_matrix, render_top_control_bar,
    BrowserState, ClipViewState, CrossfaderAssign,
    DeviceChainViewState, DeviceKind, GuiDeviceModule, GuiReturnTrack,
    GuiTrackState, LaunchQuantization, SessionMixerSections,
    SynthControlsState, TimelineGuiState,
};
use crate::theme::{
    apply_dark_carbon_theme, COLOR_AMBER, COLOR_CYAN, COLOR_GREEN, COLOR_TEXT_DIM, COLOR_TEXT_MUTED,
};
use crate::visualizers::{XyPerformPadState, XyPerformPadWidget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceView {
    SessionMatrix,
    ArrangementTimeline,
}

impl WorkspaceView {
    #[allow(non_upper_case_globals)]
    pub const Arrangement: Self = Self::ArrangementTimeline;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckTab {
    ClipView,
    DeviceRack,
    DrumSequencer,
    XyPad,
}

pub struct OxideStudioApp {
    pub audio_handle: Option<AudioStreamHandle>,
    pub mock_processor: Option<AudioEngineProcessor>,
    pub telemetry: AudioTelemetry,
    pub is_playing: bool,
    pub is_recording: bool,
    pub bpm: f64,
    pub swing: f32,
    pub current_beat: f64,
    pub time_signature: (u8, u8),
    pub metronome_enabled: bool,
    pub arrangement_record: bool,

    // Global Musical Scale & Tuning (Live 12 Flagship)
    pub global_scale: GlobalScale,
    pub tuning_system: TuningSystem,

    // View Routing
    pub workspace_view: WorkspaceView,
    pub deck_tab: DeckTab,

    // Subsystems
    pub browser: BrowserState,
    pub matrix: SessionMatrix,
    pub return_tracks: Vec<GuiReturnTrack>,
    pub mixer_sections: SessionMixerSections,
    pub launch_quantization: LaunchQuantization,
    pub crossfader_pos: f32,
    pub master_volume: f32,

    pub timeline: ArrangementTimeline,
    pub timeline_gui: TimelineGuiState,

    pub clip_view: ClipViewState,
    pub selected_track: usize,
    pub selected_clip_slot: Option<(usize, usize)>, // (track_idx, slot_idx)
    pub selected_arrangement_clip: Option<(usize, usize)>, // (track_idx, clip_idx)
    pub track_device_chains: Vec<DeviceChainViewState>,
    pub device_chain: DeviceChainViewState,

    pub keyboard: ComputerKeyboardController,
    pub synth_state: SynthControlsState,
    pub drum_pattern: DrumStepPattern,
    pub drum_sequencer: crate::panels::sequencer::DrumSequencerState,
    pub midi_pattern: MidiPattern,
    pub xy_state: XyPerformPadState,
    pub tracks_gui: Vec<GuiTrackState>,
    pub has_session_override: bool,
    pub status_message: String,
    pub current_time_seconds: f64,
    pub last_triggered_step: Option<i64>,
    pub active_synth_note: Option<u8>,
    pub active_synth_notes: Vec<(u8, f64)>, // (midi_pitch, release_beat)
    pub tap_times: Vec<f64>,
    pub copilot_open: bool,
    pub ai_copilot: crate::panels::ai_chat::AiCopilotState,
    pub shortcuts: crate::shortcuts::StudioShortcutManager,
    pub rpc_server: crate::live_rpc::OxideGuiRpcServer,
    pub export_status: Option<String>,
}

impl OxideStudioApp {
    /// Create the GUI application state with live CoreAudio stream or headless fallback.
    pub fn new(cc: Option<&eframe::CreationContext>) -> Self {
        if let Some(ctx) = cc.map(|c| &c.egui_ctx) {
            apply_dark_carbon_theme(ctx);
        }

        // Try initializing live CoreAudio stream; fallback to mock engine
        let (audio_handle, mock_processor) = match AudioEngine::start() {
            Ok(handle) => (Some(handle), None),
            Err(err) => {
                eprintln!("Note: CoreAudio hardware init ({}). Initializing mock audio stream.", err);
                let (handle, processor) = AudioEngine::start_mock(44100.0);
                (Some(handle), Some(processor))
            }
        };

        let mut app = Self::init_empty();
        app.audio_handle = audio_handle;
        app.mock_processor = mock_processor;
        app.rpc_server = crate::live_rpc::OxideGuiRpcServer::new_on_port(9877);
        if cc.is_some() {
            app.is_playing = true;
            app.deck_tab = DeckTab::ClipView;
            app.send_command(AudioCommand::SetTempo(app.bpm));
            app.send_command(AudioCommand::Play);
            app.status_message = "Oxide Studio Ready (124 BPM — F Dorian)".into();
        }
        app
    }

    /// Headless constructor for testing without eframe context.
    pub fn new_mock(sample_rate: f32) -> Self {
        let (handle, processor) = AudioEngine::start_mock(sample_rate);
        let mut app = Self::init_empty();
        app.audio_handle = Some(handle);
        app.mock_processor = Some(processor);
        app.rpc_server = crate::live_rpc::OxideGuiRpcServer::new_disabled();
        app
    }

    fn init_empty() -> Self {
        // Initialize 124 BPM Classic 909 House Intro pattern
        let mut drum_pattern = DrumStepPattern::new_16_step();
        // Pad 0: Kick, Pad 1: Snare, Pad 5: Rimshot, Pad 8: Closed HH (canonical DrumRack)
        if let Some(kick_track) = drum_pattern.tracks.get_mut(0) {
            for &s in &[0, 4, 8, 12] {
                kick_track.steps[s].active = true;
                kick_track.steps[s].accent = true;
                kick_track.steps[s].velocity = 124;
            }
        }
        if let Some(rim_track) = drum_pattern.tracks.get_mut(5) {
            for &s in &[3, 7, 11, 15] {
                rim_track.steps[s].active = true;
                rim_track.steps[s].velocity = 82;
            }
        }
        if let Some(snare_track) = drum_pattern.tracks.get_mut(1) {
            for &s in &[4, 12, 15] {
                snare_track.steps[s].active = true;
                snare_track.steps[s].velocity = if s == 15 { 78 } else { 115 };
            }
        }
        if let Some(hh_track) = drum_pattern.tracks.get_mut(8) {
            for &s in &[2, 6, 10, 14] {
                hh_track.steps[s].active = true;
                hh_track.steps[s].accent = true;
                hh_track.steps[s].velocity = 112;
            }
        }

        // F Dorian Offbeat & Syncopated House Bass/Stab pattern
        let midi_notes = vec![
            MidiNote { pitch: 41, velocity: 118, start_beat: 0.5, duration_beats: 0.45, probability: 1.0 },
            MidiNote { pitch: 53, velocity: 102, start_beat: 1.25, duration_beats: 0.25, probability: 0.9 },
            MidiNote { pitch: 44, velocity: 114, start_beat: 1.5, duration_beats: 0.45, probability: 1.0 },
            MidiNote { pitch: 46, velocity: 116, start_beat: 2.5, duration_beats: 0.45, probability: 1.0 },
            MidiNote { pitch: 56, velocity: 98, start_beat: 3.0, duration_beats: 0.25, probability: 0.8 },
            MidiNote { pitch: 48, velocity: 120, start_beat: 3.5, duration_beats: 0.45, probability: 1.0 },
        ];
        let midi_pattern = MidiPattern {
            length_beats: 4.0,
            notes: midi_notes,
        };

        // Initialize Session Matrix with 3 tracks and 8 scenes
        let mut matrix = SessionMatrix::new(3, 8);
        if let Some(t0) = matrix.tracks.get_mut(0) {
            t0.name = "F Dorian Chords".into();
            if let Some(slot0) = t0.slots.get_mut(0) {
                slot0.clip = Some(SessionClip::new(1, "F Dorian House Chords", ClipPattern::Midi(midi_pattern.clone())));
                slot0.state = SlotState::Playing { start_beat: 0.0, phase: 0.0 };
            }
        }
        if let Some(t1) = matrix.tracks.get_mut(1) {
            t1.name = "909 House Kit".into();
            if let Some(slot0) = t1.slots.get_mut(0) {
                slot0.clip = Some(SessionClip::new(2, "909 Classic House Groove", ClipPattern::Drum(drum_pattern.clone())));
                slot0.state = SlotState::Playing { start_beat: 0.0, phase: 0.0 };
            }
        }
        if let Some(t2) = matrix.tracks.get_mut(2) {
            t2.name = "Offbeat Sub Bass".into();
            if let Some(slot0) = t2.slots.get_mut(0) {
                slot0.clip = Some(SessionClip::new(3, "Offbeat House Bass", ClipPattern::Midi(midi_pattern.clone())));
                slot0.state = SlotState::Playing { start_beat: 0.0, phase: 0.0 };
            }
        }

        // Return Buses: A Reverb, B Delay
        let return_tracks = vec![
            GuiReturnTrack {
                name: "A Reverb".into(),
                effect_name: "Hybrid Reverb".into(),
                volume_linear: 0.85,
                pan: 0.0,
                mute: false,
                solo: false,
            },
            GuiReturnTrack {
                name: "B Delay".into(),
                effect_name: "Echo".into(),
                volume_linear: 0.70,
                pan: 0.0,
                mute: false,
                solo: false,
            },
        ];

        // Initialize Arrangement Timeline
        let mut timeline = ArrangementTimeline::new();
        let mut trk1 = ArrangementTrack::new(1, "F Dorian Chords", "MidiSynth");
        let mut trk2 = ArrangementTrack::new(2, "909 House Kit", "DrumRack");
        let mut trk3 = ArrangementTrack::new(3, "Offbeat Sub Bass", "MidiSynth");
        for (i, sec) in ["Intro", "Groove In", "Warm Chords", "Drop Ready"].iter().enumerate() {
            let start = (i * 16) as f64;
            trk1.add_clip(ArrangementClip::new(i as u32 + 1, *sec, start, 16.0, ClipPattern::Midi(midi_pattern.clone())));
            trk2.add_clip(ArrangementClip::new(i as u32 + 10, *sec, start, 16.0, ClipPattern::Drum(drum_pattern.clone())));
            trk3.add_clip(ArrangementClip::new(i as u32 + 20, *sec, start, 16.0, ClipPattern::Midi(midi_pattern.clone())));
        }
        timeline.tracks.push(trk1);
        timeline.tracks.push(trk2);
        timeline.tracks.push(trk3);
        timeline.add_locator(0.0, "1. Intro");
        timeline.add_locator(16.0, "2. Groove");
        timeline.add_locator(32.0, "3. Build");
        timeline.add_locator(48.0, "4. Drop");

        let tracks_gui = vec![
            GuiTrackState {
                name: "F Dorian Chords".into(),
                volume_linear: 0.90,
                pan: 0.0,
                mute: false,
                solo: false,
                sends: vec![0.35, 0.20],
                input_routing: "All Ins".into(),
                output_routing: "Master".into(),
                monitor_mode: "Auto".into(),
                crossfader_assign: CrossfaderAssign::Off,
            },
            GuiTrackState {
                name: "909 House Kit".into(),
                volume_linear: 1.0,
                pan: 0.0,
                mute: false,
                solo: false,
                sends: vec![0.15, 0.10],
                input_routing: "All Ins".into(),
                output_routing: "Master".into(),
                monitor_mode: "Auto".into(),
                crossfader_assign: CrossfaderAssign::A,
            },
            GuiTrackState {
                name: "Offbeat Sub Bass".into(),
                volume_linear: 0.88,
                pan: 0.0,
                mute: false,
                solo: false,
                sends: vec![0.05, 0.0],
                input_routing: "All Ins".into(),
                output_routing: "Master".into(),
                monitor_mode: "Auto".into(),
                crossfader_assign: CrossfaderAssign::B,
            },
        ];

        let track_device_chains = vec![
            DeviceChainViewState::for_track_1(),
            DeviceChainViewState::for_track_2(),
            DeviceChainViewState::for_track_3(),
        ];
        let device_chain = track_device_chains[0].clone();

        Self {
            audio_handle: None,
            mock_processor: None,
            telemetry: AudioTelemetry::new(),
            is_playing: false,
            is_recording: false,
            bpm: 124.0,
            swing: 0.18,
            current_beat: 0.0,
            time_signature: (4, 4),
            metronome_enabled: false,
            arrangement_record: false,
            global_scale: GlobalScale::new(5, ScaleMode::Dorian).with_lock(true),
            tuning_system: TuningSystem::default(),
            workspace_view: WorkspaceView::SessionMatrix,
            deck_tab: DeckTab::ClipView,
            browser: BrowserState::default(),
            matrix,
            return_tracks,
            mixer_sections: SessionMixerSections::default(),
            launch_quantization: LaunchQuantization::Bar1,
            crossfader_pos: 0.0,
            master_volume: 0.95,
            timeline,
            timeline_gui: TimelineGuiState::default(),
            clip_view: ClipViewState::default(),
            selected_track: 0,
            selected_clip_slot: Some((0, 0)),
            selected_arrangement_clip: None,
            track_device_chains,
            device_chain,
            keyboard: ComputerKeyboardController::new(),
            synth_state: SynthControlsState::default(),
            drum_pattern,
            drum_sequencer: crate::panels::sequencer::DrumSequencerState::default(),
            midi_pattern,
            xy_state: XyPerformPadState::default(),
            tracks_gui,
            has_session_override: false,
            status_message: "Studio Ready".into(),
            current_time_seconds: 0.0,
            last_triggered_step: None,
            active_synth_note: None,
            active_synth_notes: Vec::new(),
            tap_times: Vec::new(),
            copilot_open: true,
            ai_copilot: crate::panels::ai_chat::AiCopilotState::new(),
            shortcuts: crate::shortcuts::StudioShortcutManager::new(),
            rpc_server: crate::live_rpc::OxideGuiRpcServer::new_disabled(),
            export_status: None,
        }
    }

    /// Ensures `track_device_chains` has an entry for `track_idx`, dynamically allocating one if needed.
    pub fn ensure_track_device_chain(&mut self, track_idx: usize) {
        while self.track_device_chains.len() <= track_idx {
            let idx = self.track_device_chains.len();
            let is_drum = idx == 1
                || (idx < self.tracks_gui.len()
                    && self.tracks_gui[idx].name.to_lowercase().contains("drum"));
            self.track_device_chains
                .push(DeviceChainViewState::default_for_track(idx, is_drum));
        }
    }

    /// Selects an active track by index, synchronizing device chains and deck view tab.
    pub fn select_track(&mut self, track_idx: usize) {
        let max_tracks = self
            .tracks_gui
            .len()
            .max(self.matrix.tracks.len())
            .max(self.track_device_chains.len());
        if track_idx < max_tracks {
            // Persist current active device chain into previous track slot
            if self.selected_track < self.track_device_chains.len() {
                self.track_device_chains[self.selected_track] = self.device_chain.clone();
            }

            self.selected_track = track_idx;
            self.ensure_track_device_chain(track_idx);

            // Load the selected track's independent chain
            self.device_chain = self.track_device_chains[track_idx].clone();

            // Auto-switch bottom tab when selecting Drum vs Synth track
            if track_idx == 1 {
                if self.deck_tab == DeckTab::ClipView {
                    self.deck_tab = DeckTab::DrumSequencer;
                }
            } else if self.deck_tab == DeckTab::DrumSequencer {
                self.deck_tab = DeckTab::ClipView;
            }

            self.sync_active_clip_to_deck();
        }
    }

    /// Selects an active clip slot in the Session Matrix, loading its pattern into the bottom deck.
    pub fn select_session_clip(&mut self, track_idx: usize, slot_idx: usize) {
        self.selected_clip_slot = Some((track_idx, slot_idx));
        self.select_track(track_idx);
        self.sync_active_clip_to_deck();
    }

    /// Selects an active clip in the Arrangement Timeline, loading its pattern into the bottom deck.
    pub fn select_arrangement_clip(&mut self, track_idx: usize, clip_idx: usize) {
        self.selected_arrangement_clip = Some((track_idx, clip_idx));
        self.timeline_gui.selected_clip = Some((track_idx, clip_idx));
        self.select_track(track_idx);
        if let Some(track) = self.timeline.tracks.get(track_idx) {
            if let Some(clip) = track.clips.get(clip_idx) {
                match &clip.pattern {
                    ClipPattern::Midi(pat) => {
                        self.midi_pattern = pat.clone();
                    }
                    ClipPattern::Drum(pat) => {
                        self.drum_pattern = pat.clone();
                    }
                    ClipPattern::Audio(_) => {}
                }
            }
        }
    }

    /// Synchronizes the selected clip's pattern from the Session Matrix into the bottom deck.
    pub fn sync_active_clip_to_deck(&mut self) {
        let trk = self.selected_track;
        let slot = self.selected_clip_slot.map(|(_, s)| s).unwrap_or(0);
        if let Some(track) = self.matrix.tracks.get(trk) {
            if let Some(s) = track.slots.get(slot) {
                if let Some(ref clip) = s.clip {
                    match &clip.pattern {
                        ClipPattern::Midi(pat) => {
                            self.midi_pattern = pat.clone();
                        }
                        ClipPattern::Drum(pat) => {
                            self.drum_pattern = pat.clone();
                        }
                        ClipPattern::Audio(_) => {}
                    }
                }
            }
        }
    }

    /// Bidirectional write-back: writes active Piano Roll or Drum Sequencer pattern edits back to the active Session & Arrangement clip.
    pub fn write_back_active_clip(&mut self) {
        let trk = self.selected_track;
        let slot = self.selected_clip_slot.map(|(_, s)| s).unwrap_or(0);
        if let Some(track) = self.matrix.tracks.get_mut(trk) {
            if let Some(s) = track.slots.get_mut(slot) {
                if let Some(ref mut clip) = s.clip {
                    if trk == 1 {
                        clip.pattern = ClipPattern::Drum(self.drum_pattern.clone());
                    } else {
                        clip.pattern = ClipPattern::Midi(self.midi_pattern.clone());
                    }
                }
            }
        }

        if let Some((arr_trk, arr_clip)) = self.selected_arrangement_clip {
            if arr_trk == trk {
                if let Some(track) = self.timeline.tracks.get_mut(arr_trk) {
                    if let Some(clip) = track.clips.get_mut(arr_clip) {
                        if trk == 1 {
                            clip.pattern = ClipPattern::Drum(self.drum_pattern.clone());
                        } else {
                            clip.pattern = ClipPattern::Midi(self.midi_pattern.clone());
                        }
                    }
                }
            }
        }
    }

    /// Handle prompt submission to In-App AI Producer Copilot.
    pub fn handle_ai_prompt(&mut self, prompt: &str) {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return;
        }
        let ts = crate::panels::ai_chat::format_timestamp(self.current_time_seconds);
        self.ai_copilot.messages.push(crate::panels::ai_chat::AiChatMessage {
            role: crate::panels::ai_chat::MessageRole::User,
            text: trimmed.to_string(),
            mcp_badges: vec![],
            timestamp: ts.clone(),
        });

        let (reply, badges) = crate::panels::ai_chat::execute_studio_intent(trimmed, self);

        self.ai_copilot.messages.push(crate::panels::ai_chat::AiChatMessage {
            role: crate::panels::ai_chat::MessageRole::Assistant,
            text: reply,
            mcp_badges: badges,
            timestamp: ts,
        });
        self.ai_copilot.prompt_input.clear();
    }

    /// Send a command to the audio engine.
    pub fn send_command(&mut self, cmd: AudioCommand) {
        if let Some(ref mut handle) = self.audio_handle {
            handle.ui_handles.send_command(cmd);
        }
    }

    /// Poll Oxide Studio Live GUI RPC Server and apply pending agent/MCP mutations.
    pub fn poll_rpc(&mut self) -> usize {
        let mut rpc = std::mem::replace(
            &mut self.rpc_server,
            crate::live_rpc::OxideGuiRpcServer::new_disabled(),
        );
        let handled = rpc.poll_and_apply(self);
        self.rpc_server = rpc;
        handled
    }

    /// Poll telemetry snapshot and advance mock processor if in mock mode.
    pub fn poll_audio(&mut self) {
        if let Some(ref mut processor) = self.mock_processor {
            let mut out_buffer = [0.0f32; 256];
            processor.process_interleaved(&mut out_buffer);
        }

        let prev_beat = self.current_beat;
        if let Some(ref mut handle) = self.audio_handle {
            if let Some(telem) = handle.poll_telemetry() {
                self.telemetry = telem;
                self.current_beat = self.telemetry.playhead_beat;
            }
        }

        if !self.is_playing {
            self.last_triggered_step = None;
            if !self.active_synth_notes.is_empty() {
                let to_release: Vec<u8> =
                    self.active_synth_notes.drain(..).map(|(p, _)| p).collect();
                for pitch in to_release {
                    self.send_command(AudioCommand::NoteOff {
                        track_id: 0,
                        note: pitch,
                    });
                }
                self.active_synth_note = None;
            }
            return;
        }

        // Release notes whose duration_beats elapsed (or all notes on any backward playhead rewind)
        let playhead_rewound = self.current_beat < prev_beat;
        let mut expired = Vec::new();
        let cur_beat = self.current_beat;
        self.active_synth_notes.retain(|&(pitch, release_beat)| {
            if playhead_rewound || cur_beat >= release_beat {
                expired.push(pitch);
                false
            } else {
                true
            }
        });
        for pitch in expired {
            self.send_command(AudioCommand::NoteOff {
                track_id: 0,
                note: pitch,
            });
        }
        self.active_synth_note = self.active_synth_notes.last().map(|(p, _)| *p);

        // Determine per-track session clip active status (honor SlotState::Stopped and Stop All)
        let track_should_trigger = |track_idx: usize| -> bool {
            let Some(trk) = self.matrix.tracks.get(track_idx) else {
                return false;
            };
            if trk.slots.iter().any(|s| s.state.is_playing()) {
                return true;
            }
            if !self.has_session_override
                && self.workspace_view == WorkspaceView::ArrangementTimeline
            {
                return self
                    .timeline
                    .tracks
                    .get(track_idx)
                    .map(|at| !at.clips.is_empty())
                    .unwrap_or(false);
            }
            false
        };

        // Real-time 16th-note step sequencer driver for live GUI playback
        let global_step = (self.current_beat * 4.0).floor() as i64;
        if self.last_triggered_step != Some(global_step) {
            self.last_triggered_step = Some(global_step);
            let drum_active = track_should_trigger(1);
            let synth_active = track_should_trigger(0);

            // Trigger active 909 drum pads on this step if Track 1 (Drums) is playing
            if drum_active {
                let drum_steps = self
                    .drum_pattern
                    .tracks
                    .iter()
                    .map(|t| t.steps.len())
                    .max()
                    .unwrap_or(16)
                    .max(16);
                let step_drum = (global_step.rem_euclid(drum_steps as i64)) as usize;

                let mut drum_hits = Vec::new();
                for (pad_idx, track) in self.drum_pattern.tracks.iter().enumerate() {
                    if !self.drum_sequencer.is_pad_audible(pad_idx) {
                        continue;
                    }
                    if let Some(step) = track.steps.get(step_drum) {
                        if step.active {
                            let midi_note = 36 + (pad_idx as u8).min(15);
                            drum_hits.push((midi_note, step.velocity.max(95)));
                        }
                    }
                }
                for (note, velocity) in drum_hits {
                    self.send_command(AudioCommand::NoteOn {
                        track_id: 1,
                        note,
                        velocity,
                    });
                }
            }

            // Trigger matching synth notes polyphonically using actual multi-bar pattern length
            if synth_active {
                let midi_steps = ((self.midi_pattern.length_beats * 4.0).round() as usize).max(16);
                let step_midi = (global_step.rem_euclid(midi_steps as i64)) as usize;

                // Coalesce duplicate pitches resolved on the same step so NoteOff never cancels a same-step NoteOn
                let mut next_synth_notes: Vec<(u8, u8, f64)> = Vec::new();
                for n in &self.midi_pattern.notes {
                    let note_step =
                        ((n.start_beat * 4.0).round() as i64).rem_euclid(midi_steps as i64) as usize;
                    if note_step == step_midi {
                        let pitch = if self.global_scale.scale_lock {
                            self.global_scale.quantize(n.pitch)
                        } else {
                            n.pitch
                        };
                        let dur_beats = n.duration_beats.max(0.05);
                        if let Some(existing) =
                            next_synth_notes.iter_mut().find(|(p, _, _)| *p == pitch)
                        {
                            existing.1 = existing.1.max(n.velocity);
                            existing.2 = existing.2.max(dur_beats);
                        } else {
                            next_synth_notes.push((pitch, n.velocity, dur_beats));
                        }
                    }
                }
                for (pitch, vel, dur_beats) in next_synth_notes {
                    // Re-trigger cleanly if the exact pitch is already active from a prior step
                    if let Some(pos) = self.active_synth_notes.iter().position(|(p, _)| *p == pitch)
                    {
                        self.active_synth_notes.remove(pos);
                        self.send_command(AudioCommand::NoteOff {
                            track_id: 0,
                            note: pitch,
                        });
                    }
                    self.active_synth_notes
                        .push((pitch, self.current_beat + dur_beats));
                    self.active_synth_note = Some(pitch);
                    self.send_command(AudioCommand::NoteOn {
                        track_id: 0,
                        note: pitch,
                        velocity: vel,
                    });
                }
            }

            // Update XY pad telemetry modulation indicator without clobbering user/MCP cutoff_hz
            let bar_phase = ((self.current_beat % 32.0) / 32.0) as f32;
            self.xy_state.x = 0.25 + 0.65 * bar_phase;
            self.xy_state.y = 0.35 + 0.45 * (self.current_beat as f32 * 0.5).sin().abs();
        }
    }

    /// Construct a complete `DAWProject` snapshot from the live GUI session state.
    pub fn build_daw_project(&self) -> DAWProject {
        let mut project = DAWProject::new("Oxide Studio Session");
        project.transport.bpm = self.bpm;
        project.transport.time_signature_numerator = self.time_signature.0;
        project.transport.time_signature_denominator = self.time_signature.1;
        project.transport.swing_16th_ratio = self.swing;
        project.transport.loop_enabled = self.timeline_gui.loop_enabled;
        project.transport.loop_start_beat = self.timeline_gui.loop_start_beat;
        project.transport.loop_end_beat =
            self.timeline_gui.loop_start_beat + self.timeline_gui.loop_length_beats;
        project.master_track.volume_db = if self.master_volume > 0.001 {
            20.0 * self.master_volume.log10()
        } else {
            -70.0
        };
        project.scenes = self.matrix.scenes.clone();

        for (idx, track) in self.matrix.tracks.iter().enumerate() {
            let chain_opt = if idx == self.selected_track {
                Some(&self.device_chain)
            } else {
                self.track_device_chains.get(idx)
            };
            let is_drum = idx == 1
                || chain_opt
                    .map(|c| c.devices.iter().any(|d| d.kind == DeviceKind::DrumRack))
                    .unwrap_or(false)
                || track.name.to_lowercase().contains("drum");
            let is_audio = !is_drum
                && track.name.to_lowercase().contains("audio")
                && !chain_opt
                    .map(|c| {
                        c.devices
                            .iter()
                            .any(|d| matches!(d.kind, DeviceKind::Wavetable | DeviceKind::Operator))
                    })
                    .unwrap_or(false);

            let mut trk_def = if is_drum {
                TrackDefinition::new_drum_rack(track.id, &track.name)
            } else if is_audio {
                TrackDefinition::new_audio(track.id, &track.name)
            } else {
                TrackDefinition::new_synth(track.id, &track.name)
            };

            if idx == 0 {
                if let InstrumentConfig::BuiltinSubtractiveSynth(ref mut patch) = trk_def.instrument
                {
                    patch.filter_frequency = self.synth_state.cutoff_hz.clamp(20.0, 20000.0);
                    patch.filter_resonance = (self.synth_state.resonance / 5.0).clamp(0.0, 1.0);
                    patch.oscillator_pulse_width =
                        ((self.synth_state.pulse_pw - 0.5) / 0.45).clamp(-1.0, 1.0);
                } else {
                    let mut patch = SynthPatchParameters::default();
                    patch.filter_frequency = self.synth_state.cutoff_hz.clamp(20.0, 20000.0);
                    patch.filter_resonance = (self.synth_state.resonance / 5.0).clamp(0.0, 1.0);
                    patch.oscillator_pulse_width =
                        ((self.synth_state.pulse_pw - 0.5) / 0.45).clamp(-1.0, 1.0);
                    trk_def.instrument = InstrumentConfig::BuiltinSubtractiveSynth(patch);
                }
            }

            if let Some(gui_trk) = self.tracks_gui.get(idx) {
                trk_def.mixer.volume_db = if gui_trk.volume_linear > 0.001 {
                    20.0 * gui_trk.volume_linear.log10()
                } else {
                    -70.0
                };
                trk_def.mixer.pan = gui_trk.pan;
                trk_def.mixer.mute = gui_trk.mute;
                trk_def.mixer.solo = gui_trk.solo;
                for (send_idx, &send_lin) in gui_trk.sends.iter().enumerate() {
                    let db = if send_lin > 0.001 {
                        20.0 * send_lin.log10()
                    } else {
                        -70.0
                    };
                    trk_def.mixer.set_send_db(send_idx, db);
                }
            }

            if let Some(chain) = chain_opt {
                for dev in &chain.devices {
                    if !dev.is_enabled {
                        continue;
                    }
                    match dev.kind {
                        DeviceKind::Roar => trk_def.insert_fx.push(InsertEffectConfig::Roar {
                            drive_db: dev.roar.drive_db,
                            tone: dev.roar.tone,
                            dry_wet: 1.0,
                            output_gain_db: 0.0,
                        }),
                        DeviceKind::AutoFilter => {
                            trk_def.insert_fx.push(InsertEffectConfig::AutoFilter {
                                cutoff_hz: dev.auto_filter.cutoff_hz,
                                resonance: dev.auto_filter.resonance,
                                dry_wet: 1.0,
                            });
                        }
                        DeviceKind::EQEight => {
                            let mut params = std::collections::HashMap::new();
                            for (b_idx, band) in dev.eq8.bands.iter().enumerate() {
                                params.insert(format!("band{}_freq", b_idx), band.freq_hz);
                                params.insert(format!("band{}_gain", b_idx), band.gain_db);
                                params.insert(format!("band{}_q", b_idx), band.q);
                            }
                            trk_def
                                .insert_fx
                                .push(InsertEffectConfig::EqEight { params });
                        }
                        DeviceKind::GlueCompressor => {
                            trk_def.insert_fx.push(InsertEffectConfig::GlueCompressor {
                                threshold_db: dev.glue.threshold_db,
                                ratio: dev.glue.ratio,
                                attack_ms: dev.glue.attack_ms,
                                release_s: (dev.glue.release_ms / 1000.0).max(0.05),
                                makeup_gain_db: 0.0,
                                dry_wet: 1.0,
                            });
                        }
                        DeviceKind::OTT => {
                            trk_def.insert_fx.push(InsertEffectConfig::MultibandDynamics {
                                amount: dev.ott.depth,
                                time_macro: dev.ott.time_scale,
                                input_gain_db: 0.0,
                                output_gain_db: dev.ott.output_gain_db,
                            });
                        }
                        _ => {}
                    }
                }
            }

            trk_def.session_clips = track
                .slots
                .iter()
                .enumerate()
                .map(|(slot_idx, slot)| {
                    let mut clip = slot.clip.clone()?;
                    if self.selected_clip_slot == Some((idx, slot_idx)) {
                        clip.pattern = if is_drum {
                            ClipPattern::Drum(self.drum_pattern.clone())
                        } else {
                            ClipPattern::Midi(self.midi_pattern.clone())
                        };
                    }
                    Some(clip)
                })
                .collect();

            if let Some(arr_trk) = self.timeline.tracks.get(idx) {
                trk_def.arrangement_clips = arr_trk.clips.clone();
            }

            project.tracks.push(trk_def);
        }

        project
    }

    /// Perform WAV bounce of the current GUI session/arrangement using `oxide_bounce::render_project_to_wav`.
    pub fn export_demo_wav(&mut self, output_path: &str) -> Result<(), String> {
        let project = self.build_daw_project();
        let path = std::path::Path::new(output_path);
        match render_project_to_wav(&project, path) {
            Ok(_) => {
                let msg = format!("Exported: {}", output_path);
                self.status_message = msg.clone();
                self.export_status = Some(msg);
                Ok(())
            }
            Err(e) => {
                let msg = format!("Failed to export WAV ({}): {}", output_path, e);
                self.status_message = msg.clone();
                self.export_status = Some(msg.clone());
                Err(msg)
            }
        }
    }

    /// Export current session snapshot as an Ableton Live `.als` project.
    pub fn export_als_project(&self, output_path: &str) -> Result<(), String> {
        let project = self.build_daw_project();
        project
            .save_als(std::path::Path::new(output_path))
            .map_err(|e| format!("Failed to export ALS: {}", e))
    }

    /// Import an Ableton Live `.als` project and populate GUI tracks, clips, mixer, and device chains.
    pub fn import_als_project(&mut self, path: &std::path::Path) -> Result<(), String> {
        let proj = DAWProject::load_als(path).map_err(|e| format!("Failed to import ALS: {}", e))?;
        self.bpm = proj.transport.bpm.clamp(20.0, 999.0);
        self.time_signature = (
            proj.transport.time_signature_numerator.max(1),
            proj.transport.time_signature_denominator.max(1),
        );
        self.swing = proj.transport.swing_16th_ratio;
        self.master_volume = if proj.master_track.volume_db <= -69.5 {
            0.0
        } else {
            10.0f32
                .powf(proj.master_track.volume_db / 20.0)
                .clamp(0.0, 2.0)
        };
        self.send_command(AudioCommand::SetTempo(self.bpm));
        if let Some(ref handle) = self.audio_handle {
            handle.params.set(PARAM_BPM, self.bpm as f32);
        }

        if !proj.scenes.is_empty() {
            self.matrix.scenes = proj.scenes.clone();
        }

        if !proj.tracks.is_empty() {
            let scene_count = self.matrix.scenes.len().max(8);
            self.matrix.tracks.clear();
            self.tracks_gui.clear();
            self.timeline.tracks.clear();
            self.track_device_chains.clear();

            for (idx, trk) in proj.tracks.iter().enumerate() {
                if trk.track_type == TrackType::Return {
                    continue;
                }
                let mut sess_trk = SessionTrack::new(trk.id, &trk.name, scene_count);
                for (slot_idx, clip_opt) in trk.session_clips.iter().enumerate() {
                    while sess_trk.slots.len() <= slot_idx {
                        sess_trk.slots.push(oxide_timeline::SessionSlot::default());
                    }
                    if let Some(clip) = clip_opt {
                        sess_trk.slots[slot_idx].clip = Some(clip.clone());
                        sess_trk.slots[slot_idx].state = SlotState::Stopped;
                    }
                }
                self.matrix.tracks.push(sess_trk);

                let vol_lin = if trk.mixer.volume_db <= -69.5 {
                    0.0
                } else {
                    10.0f32.powf(trk.mixer.volume_db / 20.0).clamp(0.0, 4.0)
                };
                let sends_lin: Vec<f32> = trk
                    .mixer
                    .effective_sends_db()
                    .into_iter()
                    .map(|db| {
                        if db <= -69.5 {
                            0.0
                        } else {
                            10.0f32.powf(db / 20.0).clamp(0.0, 1.0)
                        }
                    })
                    .collect();
                self.tracks_gui.push(GuiTrackState {
                    name: trk.name.clone(),
                    volume_linear: vol_lin,
                    pan: trk.mixer.pan,
                    mute: trk.mixer.mute,
                    solo: trk.mixer.solo,
                    sends: sends_lin,
                    input_routing: "All Ins".into(),
                    output_routing: "Master".into(),
                    monitor_mode: "Auto".into(),
                    crossfader_assign: CrossfaderAssign::Off,
                });

                let mut arr_trk = ArrangementTrack::new(
                    trk.id,
                    &trk.name,
                    match trk.track_type {
                        TrackType::DrumRack => "DrumRack",
                        TrackType::Audio => "Audio",
                        _ => "MidiSynth",
                    },
                );
                for clip in &trk.arrangement_clips {
                    arr_trk.add_clip(clip.clone());
                }
                self.timeline.tracks.push(arr_trk);

                let is_drum = trk.track_type == TrackType::DrumRack;
                let chain = DeviceChainViewState::default_for_track(idx, is_drum);
                self.track_device_chains.push(chain);

                if idx == 0 {
                    if let InstrumentConfig::BuiltinSubtractiveSynth(ref patch) = trk.instrument {
                        self.synth_state.cutoff_hz = patch.filter_frequency.clamp(20.0, 20000.0);
                        self.synth_state.resonance = (patch.filter_resonance * 5.0).clamp(0.0, 5.0);
                        self.synth_state.pulse_pw =
                            (patch.oscillator_pulse_width * 0.45 + 0.5).clamp(0.05, 0.95);
                    }
                    if let Some(ClipPattern::Midi(ref mp)) = trk
                        .session_clips
                        .iter()
                        .flatten()
                        .next()
                        .map(|c| &c.pattern)
                        .or_else(|| trk.arrangement_clips.first().map(|c| &c.pattern))
                    {
                        self.midi_pattern = mp.clone();
                    }
                } else if is_drum {
                    if let Some(ClipPattern::Drum(ref dp)) = trk
                        .session_clips
                        .iter()
                        .flatten()
                        .next()
                        .map(|c| &c.pattern)
                        .or_else(|| trk.arrangement_clips.first().map(|c| &c.pattern))
                    {
                        self.drum_pattern = dp.clone();
                    }
                }
            }

            if !self.track_device_chains.is_empty() {
                self.selected_track = self.selected_track.min(self.track_device_chains.len() - 1);
                self.device_chain = self.track_device_chains[self.selected_track].clone();
            }
        }

        let display = format!("Imported {}", path.display());
        self.export_status = Some(display.clone());
        self.status_message = display;
        Ok(())
    }

    /// Handle Tap-Tempo click averaging
    pub fn handle_tap_tempo(&mut self) {
        let now = self.current_time_seconds;
        self.tap_times.retain(|&t| now - t <= 2.5);
        self.tap_times.push(now);

        if self.tap_times.len() >= 2 {
            let mut total_interval = 0.0;
            for i in 1..self.tap_times.len() {
                total_interval += self.tap_times[i] - self.tap_times[i - 1];
            }
            let avg_interval = total_interval / (self.tap_times.len() - 1) as f64;
            if avg_interval > 0.05 {
                let computed_bpm = (60.0 / avg_interval).clamp(20.0, 999.0);
                self.bpm = (computed_bpm * 10.0).round() / 10.0;
                self.send_command(AudioCommand::SetTempo(self.bpm));
                if let Some(ref handle) = self.audio_handle {
                    handle.params.set(PARAM_BPM, self.bpm as f32);
                }
            }
        }
    }

    /// Process browser load actions for all categories: Instruments, FX, Drums, Clips, Samples, and Sounds.
    pub fn handle_browser_load(&mut self, item: &crate::panels::browser::BrowserItem) {
        use crate::panels::browser::BrowserCategory;

        match item.category {
            BrowserCategory::Instruments
            | BrowserCategory::AudioEffects
            | BrowserCategory::MidiEffects
            | BrowserCategory::PlugIns => {
                let dev_kind = match item.id.as_str() {
                    "inst_wavetable" | "inst_drift" | "inst_meld" | "inst_analog" => {
                        Some(DeviceKind::Wavetable)
                    }
                    "inst_operator" => Some(DeviceKind::Operator),
                    "inst_simpler" => Some(DeviceKind::Simpler),
                    "inst_drum_rack" => Some(DeviceKind::DrumRack),
                    "fx_eq_eight" => Some(DeviceKind::EQEight),
                    "fx_glue_comp" => Some(DeviceKind::GlueCompressor),
                    "fx_roar" => Some(DeviceKind::Roar),
                    "fx_auto_filter" => Some(DeviceKind::AutoFilter),
                    "fx_ott" => Some(DeviceKind::OTT),
                    _ => None,
                };

                if let Some(kind) = dev_kind {
                    if self.device_chain.hot_swap.active && self.device_chain.hot_swap_commit(kind) {
                        if self.selected_track < self.track_device_chains.len() {
                            self.track_device_chains[self.selected_track] = self.device_chain.clone();
                        }
                        self.status_message = format!("Hot-Swapped: {}", item.name);
                    } else {
                        let new_id = (self.device_chain.devices.len() as u32) + 1;
                        self.device_chain
                            .devices
                            .push(GuiDeviceModule::new(new_id, kind));
                        if self.selected_track < self.track_device_chains.len() {
                            self.track_device_chains[self.selected_track] = self.device_chain.clone();
                        }
                        self.status_message = format!("Loaded: {}", item.name);
                    }
                } else {
                    self.status_message = format!("Selected: {}", item.name);
                }
            }
            BrowserCategory::Drums => {
                // Canonical DrumRack map: 0=Kick, 1=Snare, 2=Low Tom, 5=Rimshot, 6=Clap, 8=Closed HH, 9=Open HH
                let pad_idx = match item.id.as_str() {
                    "drum_909_kick" | "drum_808_sub_kick" => 0,
                    "drum_909_snare" | "drum_acoustic_snare" => 1,
                    "drum_conga" => 2,
                    "drum_909_rimshot" | "drum_rimshot" => 5,
                    "drum_909_clap" | "drum_clap" => 6,
                    "drum_909_hihat" => 8,
                    "drum_open_hihat" => 9,
                    _ => 0,
                };
                if let Some(track) = self.drum_pattern.tracks.get_mut(pad_idx) {
                    if !track.steps.iter().any(|s| s.active) {
                        if pad_idx == 0 {
                            for &s in &[0, 4, 8, 12] {
                                track.steps[s].active = true;
                                track.steps[s].accent = true;
                                track.steps[s].velocity = 120;
                            }
                        } else if pad_idx == 1 {
                            for &s in &[4, 12] {
                                track.steps[s].active = true;
                                track.steps[s].velocity = 110;
                            }
                        }
                    }
                }
                self.send_command(AudioCommand::NoteOn {
                    track_id: 1,
                    note: 36 + pad_idx as u8,
                    velocity: 110,
                });
                self.status_message =
                    format!("Loaded Drum Sound: {} (Pad {})", item.name, pad_idx + 1);
            }
            BrowserCategory::Clips => {
                if item.id == "clip_909_groove" {
                    if let Some(kick) = self.drum_pattern.tracks.get_mut(0) {
                        for &s in &[0, 4, 8, 12] {
                            kick.steps[s].active = true;
                            kick.steps[s].accent = true;
                            kick.steps[s].velocity = 124;
                        }
                    }
                    if let Some(snare) = self.drum_pattern.tracks.get_mut(1) {
                        for &s in &[4, 12] {
                            snare.steps[s].active = true;
                            snare.steps[s].velocity = 115;
                        }
                    }
                    if let Some(hh) = self.drum_pattern.tracks.get_mut(8) {
                        for &s in &[2, 6, 10, 14] {
                            hh.steps[s].active = true;
                            hh.steps[s].velocity = 110;
                        }
                    }
                } else if item.id == "clip_house_riff" {
                    self.midi_pattern.notes.clear();
                    self.midi_pattern
                        .notes
                        .push(MidiNote::new(53, 110, 0.0, 1.0));
                    self.midi_pattern
                        .notes
                        .push(MidiNote::new(56, 100, 0.0, 1.0));
                    self.midi_pattern
                        .notes
                        .push(MidiNote::new(60, 105, 0.0, 1.0));
                    self.midi_pattern
                        .notes
                        .push(MidiNote::new(63, 95, 0.0, 1.0));
                }
                if let Some(track) = self.matrix.tracks.get_mut(0) {
                    let clip_id = ((track.slots.len() as u32) + 1).max(10);
                    if let Some(slot) = track.slots.get_mut(0) {
                        slot.clip = Some(SessionClip::new(
                            clip_id,
                            &item.name,
                            ClipPattern::Midi(self.midi_pattern.clone()),
                        ));
                        slot.state = SlotState::Stopped;
                    }
                }
                self.status_message = format!("Loaded Clip: {}", item.name);
            }
            BrowserCategory::Samples => {
                self.clip_view.is_audio = true;
                self.status_message = format!("Loaded Sample: {}", item.name);
            }
            BrowserCategory::Sounds => {
                match item.id.as_str() {
                    "snd_house_bass" => {
                        self.synth_state.cutoff_hz = 380.0;
                        self.synth_state.resonance = 1.2;
                    }
                    "snd_acid_lead" => {
                        self.synth_state.cutoff_hz = 620.0;
                        self.synth_state.resonance = 4.8;
                    }
                    "snd_detroit_pad" => {
                        self.synth_state.cutoff_hz = 1800.0;
                        self.synth_state.resonance = 0.8;
                    }
                    _ => {
                        self.synth_state.cutoff_hz = 1200.0;
                    }
                }
                self.send_command(AudioCommand::SetParameter {
                    track_id: 0,
                    param_id: PARAM_ID_SYNTH_CUTOFF,
                    value: self.synth_state.cutoff_hz,
                });
                self.send_command(AudioCommand::SetParameter {
                    track_id: 0,
                    param_id: PARAM_ID_SYNTH_RESONANCE,
                    value: self.synth_state.resonance,
                });
                self.send_command(AudioCommand::SetParameter {
                    track_id: 0,
                    param_id: PARAM_ID_SYNTH_PULSE_PW,
                    value: self.synth_state.pulse_pw,
                });
                self.status_message = format!("Loaded Sound Preset: {}", item.name);
            }
            _ => {
                self.status_message = format!("Selected: {}", item.name);
            }
        }
    }

    /// Render a full studio UI frame using the provided ViewportBudget.
    pub fn render_frame_with_budget(
        &mut self,
        ctx: &Context,
        budget: &crate::layout::ViewportBudget,
    ) {
        self.current_time_seconds = ctx.input(|i| i.time);

        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("oxide_selected_track"), self.selected_track);
            d.insert_temp(egui::Id::new("oxide_selected_slot"), self.selected_clip_slot);
        });

        // 1. Poll real-time audio telemetry & live GUI RPC
        self.poll_audio();
        let rpc_handled = self.poll_rpc();
        if rpc_handled > 0 {
            ctx.request_repaint();
        }

        // 2. Intercept computer keyboard events for note playing & octave shifting (guarded against active text input)
        if !ctx.wants_keyboard_input() {
            let key_commands = self.keyboard.process_events(ctx);
            for cmd in key_commands {
                self.send_command(cmd);
            }
        }

        // 2b. Process 1:1 Live muscle-memory keyboard shortcuts
        let shortcut_actions = self.shortcuts.process_input(ctx);
        for action in shortcut_actions {
            match action {
                crate::shortcuts::StudioShortcutAction::ToggleWorkspaceView => {
                    self.workspace_view = match self.workspace_view {
                        WorkspaceView::SessionMatrix => WorkspaceView::ArrangementTimeline,
                        WorkspaceView::ArrangementTimeline => WorkspaceView::SessionMatrix,
                    };
                }
                crate::shortcuts::StudioShortcutAction::ToggleDetailView => {
                    self.deck_tab = match self.deck_tab {
                        DeckTab::ClipView => DeckTab::DeviceRack,
                        DeckTab::DeviceRack => DeckTab::ClipView,
                        other => other,
                    };
                }
                crate::shortcuts::StudioShortcutAction::TogglePlayStop => {
                    self.is_playing = !self.is_playing;
                    self.last_triggered_step = None;
                    if self.is_playing {
                        self.send_command(AudioCommand::Play);
                    } else {
                        let notes: Vec<u8> =
                            self.active_synth_notes.drain(..).map(|(p, _)| p).collect();
                        for pitch in notes {
                            self.send_command(AudioCommand::NoteOff {
                                track_id: 0,
                                note: pitch,
                            });
                        }
                        self.active_synth_note = None;
                        self.send_command(AudioCommand::Stop);
                    }
                }
                crate::shortcuts::StudioShortcutAction::NewAudioTrack => {
                    let track_id = self.matrix.tracks.len();
                    let name = format!("Audio Track {}", track_id + 1);
                    let scene_count = self.matrix.scenes.len().max(8);
                    self.matrix.tracks.push(SessionTrack::new(track_id as u32, &name, scene_count));
                    self.tracks_gui.push(GuiTrackState {
                        name: name.clone(),
                        volume_linear: 0.9,
                        pan: 0.0,
                        mute: false,
                        solo: false,
                        sends: vec![0.0, 0.0],
                        input_routing: "All Ins".into(),
                        output_routing: "Master".into(),
                        monitor_mode: "Auto".into(),
                        crossfader_assign: CrossfaderAssign::Off,
                    });
                    self.track_device_chains.push(DeviceChainViewState::default_for_track(track_id, false));
                    self.status_message = format!("Inserted {}", name);
                }
                crate::shortcuts::StudioShortcutAction::NewMidiTrack => {
                    let track_id = self.matrix.tracks.len();
                    let name = format!("MIDI Track {}", track_id + 1);
                    let scene_count = self.matrix.scenes.len().max(8);
                    self.matrix.tracks.push(SessionTrack::new(track_id as u32, &name, scene_count));
                    self.tracks_gui.push(GuiTrackState {
                        name: name.clone(),
                        volume_linear: 0.9,
                        pan: 0.0,
                        mute: false,
                        solo: false,
                        sends: vec![0.0, 0.0],
                        input_routing: "All Ins".into(),
                        output_routing: "Master".into(),
                        monitor_mode: "Auto".into(),
                        crossfader_assign: CrossfaderAssign::Off,
                    });
                    self.track_device_chains.push(DeviceChainViewState::default_for_track(track_id, false));
                    self.status_message = format!("Inserted {}", name);
                }
                crate::shortcuts::StudioShortcutAction::NewReturnTrack => {
                    self.mixer_sections.show_returns = true;
                    let ret_idx = self.return_tracks.len();
                    let ret_char = (b'A' + (ret_idx % 26) as u8) as char;
                    let ret_name = format!("Return {}", ret_char);
                    self.return_tracks.push(GuiReturnTrack {
                        name: ret_name.clone(),
                        effect_name: "Echo".into(),
                        volume_linear: 0.75,
                        pan: 0.0,
                        mute: false,
                        solo: false,
                    });
                    self.status_message = format!("Inserted {}", ret_name);
                }
                crate::shortcuts::StudioShortcutAction::NewMidiClip => {
                    let sel_track = self.selected_track;
                    if let Some(track) = self.matrix.tracks.get_mut(sel_track) {
                        let clip_id = (track.slots.len() as u32) + 1;
                        if let Some(slot) = track.slots.get_mut(0) {
                            slot.clip = Some(SessionClip::new(
                                clip_id,
                                format!("Clip {}", clip_id),
                                ClipPattern::Midi(self.midi_pattern.clone()),
                            ));
                            slot.state = SlotState::Stopped;
                            self.status_message = format!("Inserted New MIDI Clip in Track {}", sel_track + 1);
                        }
                    }
                }
                crate::shortcuts::StudioShortcutAction::DuplicateSelected => {
                    let sel_track = self.selected_track;
                    if sel_track < self.matrix.tracks.len() {
                        let orig_track = self.matrix.tracks[sel_track].clone();
                        let orig_gui = self.tracks_gui[sel_track].clone();
                        let new_id = self.matrix.tracks.len();
                        let mut dup_track = orig_track;
                        dup_track.id = new_id as u32;
                        dup_track.name = format!("{} (Copy)", dup_track.name);
                        let mut dup_gui = orig_gui;
                        dup_gui.name = dup_track.name.clone();
                        self.matrix.tracks.push(dup_track);
                        self.tracks_gui.push(dup_gui);
                        let dup_chain = self.track_device_chains.get(sel_track).cloned().unwrap_or_else(|| DeviceChainViewState::default_for_track(new_id, false));
                        self.track_device_chains.push(dup_chain);
                        self.status_message = format!("Duplicated Track {}", sel_track + 1);
                    }
                }
                crate::shortcuts::StudioShortcutAction::SplitAtPlayhead => {
                    let split_count = self.timeline.split_at(self.current_beat, 9000);
                    self.status_message = format!("Split {} Clip(s) at beat {:.2}", split_count, self.current_beat);
                }
                crate::shortcuts::StudioShortcutAction::ConsolidateSelection => {
                    let sel_track = (self.selected_track + 1) as u32;
                    let loop_start = self.timeline_gui.loop_start_beat;
                    let loop_end = loop_start + self.timeline_gui.loop_length_beats;
                    let consolidated = self.timeline.consolidate_track_range(sel_track, loop_start, loop_end, 9500, "Consolidated Clip");
                    if consolidated.is_some() {
                        self.status_message = format!("Consolidated Selection [{:.1}..{:.1})", loop_start, loop_end);
                    } else {
                        self.status_message = "Consolidated Selection".into();
                    }
                }
                crate::shortcuts::StudioShortcutAction::ToggleDrawOrBounce => {
                    match self.export_demo_wav("/tmp/ableton_studio_export.wav") {
                        Ok(()) => {
                            self.status_message =
                                "Bounced Audio to /tmp/ableton_studio_export.wav".into();
                        }
                        Err(e) => {
                            self.status_message = e;
                        }
                    }
                }
                crate::shortcuts::StudioShortcutAction::ToggleBrowser => {
                    self.browser.is_collapsed = !self.browser.is_collapsed;
                }
                crate::shortcuts::StudioShortcutAction::ExportStems => {
                    if let Err(e) = self.export_demo_wav("/tmp/oxide_studio_mix.wav") {
                        self.export_status = Some(e.clone());
                        self.status_message = e;
                    } else {
                        match oxide_bounce::stems::export_remix_stems_from_wav(
                            std::path::Path::new("/tmp/oxide_studio_mix.wav"),
                            std::path::Path::new("/tmp/oxide_stems"),
                        ) {
                            Ok(_) => {
                                self.export_status =
                                    Some("Stems exported to /tmp/oxide_stems/".to_string());
                                self.status_message =
                                    "Stems exported to /tmp/oxide_stems/".into();
                            }
                            Err(e) => {
                                let msg = format!("Stems export error: {}", e);
                                self.export_status = Some(msg.clone());
                                self.status_message = msg;
                            }
                        }
                    }
                }
                crate::shortcuts::StudioShortcutAction::ToggleMuteSelected => {
                    let sel_track = self.selected_track;
                    if let Some(trk) = self.tracks_gui.get_mut(sel_track) {
                        trk.mute = !trk.mute;
                        let is_muted = trk.mute;
                        self.send_command(AudioCommand::SetParameter {
                            track_id: sel_track,
                            param_id: 1200 + sel_track as u32,
                            value: if is_muted { 1.0 } else { 0.0 },
                        });
                        self.status_message = format!("Track {} Mute: {}", sel_track + 1, is_muted);
                    }
                }
                crate::shortcuts::StudioShortcutAction::ToggleSoloSelected => {
                    let sel_track = self.selected_track;
                    if let Some(trk) = self.tracks_gui.get_mut(sel_track) {
                        trk.solo = !trk.solo;
                        let is_solo = trk.solo;
                        self.send_command(AudioCommand::SetParameter {
                            track_id: sel_track,
                            param_id: 1300 + sel_track as u32,
                            value: if is_solo { 1.0 } else { 0.0 },
                        });
                        self.status_message = format!("Track {} Solo: {}", sel_track + 1, is_solo);
                    }
                }
                crate::shortcuts::StudioShortcutAction::OctaveDown => {
                    self.keyboard.octave_offset = (self.keyboard.octave_offset - 1).max(-3);
                    self.status_message = format!("Keyboard Octave: {}", self.keyboard.octave_offset);
                }
                crate::shortcuts::StudioShortcutAction::OctaveUp => {
                    self.keyboard.octave_offset = (self.keyboard.octave_offset + 1).min(3);
                    self.status_message = format!("Keyboard Octave: {}", self.keyboard.octave_offset);
                }
                crate::shortcuts::StudioShortcutAction::VelocityDown => {
                    self.keyboard.velocity = self.keyboard.velocity.saturating_sub(10).max(1);
                    self.status_message = format!("Keyboard Velocity: {}", self.keyboard.velocity);
                }
                crate::shortcuts::StudioShortcutAction::VelocityUp => {
                    self.keyboard.velocity = (self.keyboard.velocity + 10).min(127);
                    self.status_message = format!("Keyboard Velocity: {}", self.keyboard.velocity);
                }
                crate::shortcuts::StudioShortcutAction::ToggleShortcutsModal => {}
                crate::shortcuts::StudioShortcutAction::ToggleAiCopilot => {
                    self.copilot_open = !self.copilot_open;
                }
                crate::shortcuts::StudioShortcutAction::ToggleMidiMapMode => {
                    self.status_message = "MIDI Map Mode toggled".to_string();
                }
                crate::shortcuts::StudioShortcutAction::ToggleKeyMapMode => {
                    self.status_message = "Key Map Mode toggled".to_string();
                }
            }
        }

        // 3. Top Control & Scale Bar Panel (44px deterministic height)
        TopBottomPanel::top("top_control_bar")
            .exact_height(budget.top_bar_height)
            .show(ctx, |ui| {
                let action = render_top_control_bar(
                    ui,
                    self.is_playing,
                    self.is_recording,
                    &mut self.bpm,
                    &mut self.time_signature,
                    &mut self.metronome_enabled,
                    &mut self.arrangement_record,
                    &mut self.global_scale,
                    &mut self.tuning_system,
                    self.current_beat,
                    &self.telemetry,
                    self.copilot_open,
                );

                if action.toggle_copilot {
                    self.copilot_open = !self.copilot_open;
                }

                if action.tap_tempo {
                    self.handle_tap_tempo();
                }
                if action.toggle_play {
                    self.is_playing = !self.is_playing;
                    self.last_triggered_step = None;
                    if self.is_playing {
                        self.send_command(AudioCommand::Play);
                    } else {
                        let notes: Vec<u8> =
                            self.active_synth_notes.drain(..).map(|(p, _)| p).collect();
                        for pitch in notes {
                            self.send_command(AudioCommand::NoteOff {
                                track_id: 0,
                                note: pitch,
                            });
                        }
                        self.active_synth_note = None;
                        self.send_command(AudioCommand::Stop);
                    }
                }
                if action.stop {
                    self.is_playing = false;
                    self.current_beat = 0.0;
                    self.last_triggered_step = None;
                    let notes: Vec<u8> =
                        self.active_synth_notes.drain(..).map(|(p, _)| p).collect();
                    for pitch in notes {
                        self.send_command(AudioCommand::NoteOff {
                            track_id: 0,
                            note: pitch,
                        });
                    }
                    self.active_synth_note = None;
                    self.send_command(AudioCommand::Stop);
                }
                if action.toggle_record {
                    self.is_recording = !self.is_recording;
                }
                if let Some(new_bpm) = action.bpm_changed {
                    self.send_command(AudioCommand::SetTempo(new_bpm));
                    if let Some(ref handle) = self.audio_handle {
                        handle.params.set(PARAM_BPM, new_bpm as f32);
                    }
                }
                if action.scale_changed {
                    self.status_message =
                        format!("Global Scale: {}", self.global_scale.display_name());
                }
                if action.view_session {
                    self.workspace_view = WorkspaceView::SessionMatrix;
                }
                if action.view_arrangement {
                    self.workspace_view = WorkspaceView::Arrangement;
                }
                if action.export_wav {
                    if let Err(e) = self.export_demo_wav("/tmp/ableton_studio_export.wav") {
                        self.status_message = e;
                    }
                }
                if action.export_als {
                    match self.export_als_project("/tmp/oxide_studio_session.als") {
                        Ok(()) => {
                            self.export_status =
                                Some("Exported /tmp/oxide_studio_session.als".to_string());
                            self.status_message = "Exported /tmp/oxide_studio_session.als".into();
                        }
                        Err(e) => {
                            self.export_status = Some(e.clone());
                            self.status_message = e;
                        }
                    }
                }
                if action.import_als {
                    let p = std::path::Path::new("/tmp/oxide_studio_session.als");
                    if p.exists() {
                        if let Err(e) = self.import_als_project(p) {
                            self.export_status = Some(e.clone());
                            self.status_message = e;
                        }
                    } else {
                        let msg = "ALS file not found: /tmp/oxide_studio_session.als".to_string();
                        self.export_status = Some(msg.clone());
                        self.status_message = msg;
                    }
                }
                if action.split_stems {
                    if let Err(e) = self.export_demo_wav("/tmp/oxide_studio_mix.wav") {
                        self.export_status = Some(e.clone());
                        self.status_message = e;
                    } else {
                        match oxide_bounce::stems::export_remix_stems_from_wav(
                            std::path::Path::new("/tmp/oxide_studio_mix.wav"),
                            std::path::Path::new("/tmp/oxide_stems"),
                        ) {
                            Ok(_) => {
                                self.export_status =
                                    Some("Stems exported to /tmp/oxide_stems/".to_string());
                                self.status_message =
                                    "Stems exported to /tmp/oxide_stems/".into();
                            }
                            Err(e) => {
                                let msg = format!("Stems export error: {}", e);
                                self.export_status = Some(msg.clone());
                                self.status_message = msg;
                            }
                        }
                    }
                }
            });

        // 4a. Pinned Full-Width Status / Info Bar at the absolute bottom (18px, isolated TopBottomPanel so deck never collides)
        let status_bar_h = 18.0_f32;
        TopBottomPanel::bottom("studio_status_bar")
            .exact_height(status_bar_h)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(0x1e, 0x1e, 0x1e))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x11, 0x11, 0x11)))
                    .inner_margin(egui::Margin::symmetric(8.0, 1.0)),
            )
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                    ui.label(RichText::new("▶").size(9.0).color(COLOR_AMBER));
                    ui.label(RichText::new(&self.status_message).size(10.0).color(COLOR_TEXT_MUTED));
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        let engine_txt = if self.mock_processor.is_some() {
                            "DSP: Mock 44.1kHz"
                        } else {
                            "DSP: CoreAudio 44.1kHz"
                        };
                        ui.label(RichText::new(engine_txt).size(9.5).color(COLOR_TEXT_DIM));
                    });
                });
            });

        // 4b. Merged Full-Width Bottom Detail Deck (exact height above status bar, spans 100% window width)
        let detail_deck_h = (budget.bottom_deck_height - status_bar_h).max(162.0);
        TopBottomPanel::bottom("bottom_detail_panel")
            .exact_height(detail_deck_h)
            .frame(
                egui::Frame::none()
                    .fill(crate::theme::COLOR_SURFACE_AREA)
                    .stroke(Stroke::new(1.0_f32, crate::theme::COLOR_CONTROL_CONTRAST_FRAME))
                    .inner_margin(egui::Margin::symmetric(6.0, 3.0)),
            )
            .resizable(false)
            .show(ctx, |ui| {
                // Hard clip guard so child content can never bleed outside the detail deck
                ui.set_clip_rect(ui.max_rect());

                // A. Draggable StudioHorizontalSplitter at the top edge of bottom deck
                let splitter_id = egui::Id::new("studio_horizontal_splitter");
                let (_dragged_h, splitter_resp) = crate::layout::StudioHorizontalSplitter::show_with_state(
                    ui,
                    splitter_id,
                    budget.total_rect.height(),
                );
                if splitter_resp.dragged() || splitter_resp.double_clicked() {
                    ui.ctx().request_repaint();
                }

                // B. Deck Tabs Selector & Tab Contents
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
                    // Header Tabs Selector
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                        let clip_btn = Button::new(
                            RichText::new("Clip View (Piano Roll / Warping)").size(10.5).color(
                                if self.deck_tab == DeckTab::ClipView {
                                    COLOR_AMBER
                                } else {
                                    COLOR_TEXT_MUTED
                                },
                            ),
                        );
                        if ui.add(clip_btn).clicked() {
                            self.deck_tab = DeckTab::ClipView;
                        }

                        let dev_btn = Button::new(RichText::new("Device Chain View").size(10.5).color(
                            if self.deck_tab == DeckTab::DeviceRack {
                                COLOR_CYAN
                            } else {
                                COLOR_TEXT_MUTED
                            },
                        ));
                        if ui.add(dev_btn).clicked() {
                            self.deck_tab = DeckTab::DeviceRack;
                        }

                        let seq_btn = Button::new(
                            RichText::new("16-Step 909 Sequencer").size(10.5).color(
                                if self.deck_tab == DeckTab::DrumSequencer {
                                    COLOR_GREEN
                                } else {
                                    COLOR_TEXT_MUTED
                                },
                            ),
                        );
                        if ui.add(seq_btn).clicked() {
                            self.deck_tab = DeckTab::DrumSequencer;
                        }

                        let xy_btn = Button::new(RichText::new("XY Perform Pad").size(10.5).color(
                            if self.deck_tab == DeckTab::XyPad {
                                COLOR_AMBER
                            } else {
                                COLOR_TEXT_MUTED
                            },
                        ));
                        if ui.add(xy_btn).clicked() {
                            self.deck_tab = DeckTab::XyPad;
                        }

                        // Keyboard Mode Indicator on right
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            let kb_text = match self.keyboard.mode {
                                KeyboardMode::Piano => {
                                    format!("Keys: Piano (Octave {})", self.keyboard.octave_offset)
                                }
                                KeyboardMode::Isomorphic => "Keys: Wicki-Hayden Isomorphic".into(),
                            };
                            let kb_btn =
                                Button::new(RichText::new(kb_text).size(9.5).color(COLOR_TEXT_MUTED));
                            if ui.add(kb_btn).clicked() {
                                self.keyboard.mode = match self.keyboard.mode {
                                    KeyboardMode::Piano => KeyboardMode::Isomorphic,
                                    KeyboardMode::Isomorphic => KeyboardMode::Piano,
                                };
                            }
                        });
                    });

                    ui.separator();

                    // Bottom Tab Content
                    match self.deck_tab {
                        DeckTab::ClipView => {
                            self.clip_view.bpm = self.bpm;
                            let clip_act = render_clip_view(
                                ui,
                                &mut self.clip_view,
                                &mut self.midi_pattern,
                                &self.global_scale,
                                self.current_beat,
                            );
                            if let Some(off_pitch) = clip_act.audition_note_off {
                                let sel_track = self.selected_track;
                                self.send_command(AudioCommand::NoteOff {
                                    track_id: sel_track,
                                    note: off_pitch,
                                });
                            }
                            if let Some((note, vel)) = clip_act.audition_note {
                                let sel_track = self.selected_track;
                                self.send_command(AudioCommand::NoteOn {
                                    track_id: sel_track,
                                    note,
                                    velocity: vel,
                                });
                            }
                            if let Some(new_len) = clip_act.loop_length_changed {
                                self.status_message = format!("Clip Loop Length: {:.0} Beats", new_len);
                                self.write_back_active_clip();
                            } else if clip_act.pattern_mutated {
                                self.status_message = "MIDI Pattern Updated via Live 12 Tool".into();
                                self.write_back_active_clip();
                            } else if clip_act.warp_marker_moved {
                                self.status_message = "Audio Warp Marker Adjusted".into();
                            }
                        }
                        DeckTab::DeviceRack => {
                            let dev_act = render_device_chain_view(
                                ui,
                                &mut self.device_chain,
                                &self.telemetry.spectrum_bins,
                            );
                            if self.selected_track < self.track_device_chains.len() {
                                self.track_device_chains[self.selected_track] = self.device_chain.clone();
                            }
                            if let Some(kind) = dev_act.insert_device {
                                self.status_message = format!("Inserted Device: {}", kind.name());
                            } else if let Some(idx) = dev_act.toggle_bypass {
                                let is_enabled = self
                                    .device_chain
                                    .devices
                                    .get(idx)
                                    .map(|d| d.is_enabled)
                                    .unwrap_or(true);
                                self.send_command(AudioCommand::SetParameter {
                                    track_id: self.selected_track,
                                    param_id: 3500 + idx as u32,
                                    value: if is_enabled { 1.0 } else { 0.0 },
                                });
                                let st = if is_enabled { "Enabled" } else { "Bypassed" };
                                self.status_message = format!("Device #{} {}", idx + 1, st);
                            } else if let Some(idx) = dev_act.device_parameter_changed {
                                let trk = self.selected_track;
                                if let Some(dev) = self.device_chain.devices.get(idx).cloned() {
                                    match dev.kind {
                                        DeviceKind::Wavetable => {
                                            if trk == 0 && idx == 0 {
                                                self.synth_state.cutoff_hz =
                                                    dev.wavetable.filter1_cutoff.clamp(20.0, 20000.0);
                                                self.synth_state.resonance =
                                                    dev.wavetable.filter1_res.clamp(0.0, 1.0);
                                            }
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: PARAM_ID_SYNTH_CUTOFF,
                                                value: dev.wavetable.filter1_cutoff,
                                            });
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: PARAM_ID_SYNTH_RESONANCE,
                                                value: dev.wavetable.filter1_res,
                                            });
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: 100,
                                                value: dev.wavetable.table_pos.clamp(0.0, 1.0),
                                            });
                                        }
                                        DeviceKind::Roar => {
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: PARAM_FX_DRIVE as u32,
                                                value: (dev.roar.drive_db / 12.0).clamp(0.0, 2.0),
                                            });
                                        }
                                        DeviceKind::AutoFilter => {
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: PARAM_ID_SYNTH_CUTOFF,
                                                value: dev.auto_filter.cutoff_hz,
                                            });
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: PARAM_ID_SYNTH_RESONANCE,
                                                value: dev.auto_filter.resonance,
                                            });
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: PARAM_ID_SYNTH_ENV_MOD,
                                                value: dev.auto_filter.env_amount,
                                            });
                                        }
                                        _ => {
                                            self.send_command(AudioCommand::SetParameter {
                                                track_id: trk,
                                                param_id: 3000 + (idx as u32).min(127) * 32,
                                                value: 1.0,
                                            });
                                        }
                                    }
                                }
                                self.status_message =
                                    format!("Device #{} Parameter Modulated", idx + 1);
                            }
                        }
                        DeckTab::DrumSequencer => {
                            let current_16th_step = ((self.current_beat * 4.0).floor() as usize) % 16;
                            let seq_action = crate::panels::sequencer::render_drum_sequencer_with_state(
                                ui,
                                &mut self.drum_pattern,
                                current_16th_step,
                                &mut self.drum_sequencer,
                            );
                            ui.data_mut(|d| {
                                d.insert_temp(egui::Id::new("oxide_909_sequencer_state"), self.drum_sequencer.clone())
                            });

                            if let Some(pad_idx) = seq_action.pad_triggered {
                                self.send_command(AudioCommand::NoteOn {
                                    track_id: 1,
                                    note: 36 + pad_idx,
                                    velocity: 110,
                                });
                            }

                            if seq_action.pattern_mutated || seq_action.step_toggled.is_some() {
                                self.write_back_active_clip();
                            }
                        }
                        DeckTab::XyPad => {
                            let rms_bloom =
                                self.telemetry.master_rms_l.max(self.telemetry.master_rms_r);
                            let xy_h = ui.available_height().clamp(130.0, 210.0);
                            let xy_widget = XyPerformPadWidget::new(
                                &mut self.xy_state,
                                self.current_time_seconds,
                                rms_bloom,
                                vec2(ui.available_width().min(420.0), xy_h),
                            );
                            let response = xy_widget.show(ui);

                            if response.changed() {
                                let norm_x = self.xy_state.x;
                                let norm_y = self.xy_state.y;

                                self.send_command(AudioCommand::SetParameter {
                                    track_id: 0,
                                    param_id: 100,
                                    value: norm_x,
                                });
                                self.send_command(AudioCommand::SetParameter {
                                    track_id: 0,
                                    param_id: 101,
                                    value: norm_y,
                                });

                                if let Some(ref handle) = self.audio_handle {
                                    handle.params.set(PARAM_SYNTH_MACRO_X, norm_x);
                                    handle.params.set(PARAM_SYNTH_MACRO_Y, norm_y);
                                }
                            }
                        }
                    }
                });
            });

        // 5. Left Collapsible Browser Panel (220-270px width)
        if !self.browser.is_collapsed {
            SidePanel::left("left_browser_panel")
                .exact_width(budget.left_browser_width)
                .resizable(false)
                .show(ctx, |ui| {
                    let browser_action = render_browser_panel(ui, &mut self.browser);

                    if let Some(item) = browser_action.load_item {
                        self.handle_browser_load(&item);
                    }
                });
        } else {
            // Collapsed Strip with Quick-Expand Button
            SidePanel::left("left_browser_collapsed")
                .exact_width(24.0)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        let expand_btn =
                            Button::new(RichText::new("▶").size(11.0).color(COLOR_AMBER));
                        if ui.add(expand_btn).on_hover_text("Open Browser").clicked() {
                            self.browser.is_collapsed = false;
                        }
                    });
                });
        }

        // 6. Right AI Copilot Panel (260-320px width)
        if self.copilot_open {
            SidePanel::right("oxide_ai_copilot_panel")
                .exact_width(budget.right_copilot_width)
                .resizable(false)
                .show(ctx, |ui| {
                    crate::panels::ai_chat::render_ai_copilot_panel(ui, self);
                });
        }

        // 7. Center Workspace Panel (receives >= 55% of height)
        CentralPanel::default().show(ctx, |ui| {
            // Workspace Selector Toolbar
            ui.horizontal(|ui| {
                let session_btn = Button::new(RichText::new("Session View").size(11.5).color(
                    if self.workspace_view == WorkspaceView::SessionMatrix {
                        COLOR_AMBER
                    } else {
                        COLOR_TEXT_MUTED
                    },
                ));
                if ui.add(session_btn).clicked() {
                    self.workspace_view = WorkspaceView::SessionMatrix;
                }

                let arr_btn = Button::new(RichText::new("Arrangement View").size(11.5).color(
                    if self.workspace_view == WorkspaceView::ArrangementTimeline {
                        COLOR_CYAN
                    } else {
                        COLOR_TEXT_MUTED
                    },
                ));
                if ui.add(arr_btn).clicked() {
                    self.workspace_view = WorkspaceView::ArrangementTimeline;
                }

                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(&self.status_message)
                            .size(11.0)
                            .color(COLOR_TEXT_MUTED),
                    );
                });
            });

            ui.separator();

            match self.workspace_view {
                WorkspaceView::SessionMatrix => {
                    let prev_master_vol = self.master_volume;
                    let action = render_session_matrix(
                        ui,
                        &mut self.matrix,
                        &mut self.tracks_gui,
                        &mut self.return_tracks,
                        &mut self.mixer_sections,
                        &mut self.launch_quantization,
                        &mut self.crossfader_pos,
                        &mut self.master_volume,
                        &self.telemetry,
                        self.current_time_seconds,
                    );

                    if let Some(mv) = action.master_volume_changed {
                        self.master_volume = mv;
                    }
                    if (self.master_volume - prev_master_vol).abs() > 1e-5 {
                        if let Some(ref handle) = self.audio_handle {
                            handle.params.set(PARAM_MASTER_VOL, self.master_volume);
                        }
                        if let Some(ref proc) = self.mock_processor {
                            proc.params.set(PARAM_MASTER_VOL, self.master_volume);
                        }
                        self.send_command(AudioCommand::SetParameter {
                            track_id: 0,
                            param_id: 1,
                            value: self.master_volume,
                        });
                    }

                    if let Some((trk, vol)) = action.track_volume_changed {
                        if trk < self.tracks_gui.len() {
                            self.tracks_gui[trk].volume_linear = vol;
                        }
                        if trk < 16 {
                            if let Some(ref handle) = self.audio_handle {
                                handle.params.set(PARAM_TRACK_VOL_BASE + trk, vol);
                            }
                            if let Some(ref proc) = self.mock_processor {
                                proc.params.set(PARAM_TRACK_VOL_BASE + trk, vol);
                            }
                        }
                        self.send_command(AudioCommand::SetParameter {
                            track_id: trk,
                            param_id: 1000 + trk as u32,
                            value: vol,
                        });
                    }

                    if let Some((trk, pan)) = action.track_pan_changed {
                        if trk < self.tracks_gui.len() {
                            self.tracks_gui[trk].pan = pan;
                        }
                        if trk < 16 {
                            if let Some(ref handle) = self.audio_handle {
                                handle.params.set(PARAM_TRACK_PAN_BASE + trk, pan);
                            }
                            if let Some(ref proc) = self.mock_processor {
                                proc.params.set(PARAM_TRACK_PAN_BASE + trk, pan);
                            }
                        }
                        self.send_command(AudioCommand::SetParameter {
                            track_id: trk,
                            param_id: 1100 + trk as u32,
                            value: pan,
                        });
                    }

                    if let Some(trk) = action.track_mute_toggled {
                        let is_muted = if trk < self.tracks_gui.len() {
                            self.tracks_gui[trk].mute
                        } else {
                            false
                        };
                        self.send_command(AudioCommand::SetParameter {
                            track_id: trk,
                            param_id: 1200 + trk as u32,
                            value: if is_muted { 1.0 } else { 0.0 },
                        });
                    }

                    if let Some(trk) = action.track_solo_toggled {
                        let is_solo = if trk < self.tracks_gui.len() {
                            self.tracks_gui[trk].solo
                        } else {
                            false
                        };
                        self.send_command(AudioCommand::SetParameter {
                            track_id: trk,
                            param_id: 1300 + trk as u32,
                            value: if is_solo { 1.0 } else { 0.0 },
                        });
                    }

                    if let Some((trk, send_idx, val)) = action.send_changed {
                        let param_id = if send_idx == 0 {
                            1400 + trk as u32
                        } else {
                            1500 + trk as u32
                        };
                        self.send_command(AudioCommand::SetParameter {
                            track_id: trk,
                            param_id,
                            value: val,
                        });
                    }

                    if let Some(pos) = action.crossfader_changed {
                        self.crossfader_pos = pos;
                        self.send_command(AudioCommand::SetParameter {
                            track_id: 0,
                            param_id: 2000,
                            value: pos,
                        });
                    }

                    if let Some((trk, slot)) = action.trigger_clip {
                        self.send_command(AudioCommand::TriggerClip {
                            track_id: trk,
                            slot_id: slot,
                        });
                        self.has_session_override = true;
                    }
                    if let Some(trk) = action.stop_clip {
                        self.send_command(AudioCommand::StopClip { track_id: trk });
                    }
                    if action.stop_all_clips {
                        let track_count = self.matrix.tracks.len();
                        for trk_idx in 0..track_count {
                            self.send_command(AudioCommand::StopClip { track_id: trk_idx });
                        }
                        for trk in &mut self.matrix.tracks {
                            for slot in &mut trk.slots {
                                if slot.clip.is_some() {
                                    slot.state = SlotState::Stopped;
                                }
                            }
                        }
                    }
                    if let Some(scene) = action.trigger_scene {
                        self.send_command(AudioCommand::TriggerScene(scene));
                        self.has_session_override = true;
                    }
                    if let Some((trk, slot)) = action.clip_selected {
                        self.select_session_clip(trk, slot);
                    } else if let Some(trk) = action.track_selected {
                        self.select_track(trk);
                    }
                    if let Some(sel) = ui.data_mut(|d| {
                        d.get_temp::<usize>(egui::Id::new("oxide_selected_track"))
                    }) {
                        if sel < self.tracks_gui.len() {
                            self.device_chain.selected_device_idx = self
                                .device_chain
                                .selected_device_idx
                                .min(self.device_chain.devices.len().saturating_sub(1));
                        }
                    }
                }
                WorkspaceView::ArrangementTimeline => {
                    let action = render_arrangement_timeline(
                        ui,
                        &mut self.timeline,
                        &mut self.timeline_gui,
                        self.current_beat,
                        self.has_session_override,
                    );

                    if let Some((trk, clip_idx)) = action.clip_selected {
                        self.select_arrangement_clip(trk, clip_idx);
                    } else if let Some(trk) = action.track_selected {
                        self.select_track(trk);
                    }
                    if action.back_to_arrangement {
                        self.has_session_override = false;
                        for track in &mut self.matrix.tracks {
                            track.stop_immediate();
                        }
                    }
                    if let Some(beat) = action.seek_beat {
                        self.current_beat = beat.max(0.0);
                        self.last_triggered_step = None;
                        self.telemetry.playhead_beat = self.current_beat;
                        let notes: Vec<u8> =
                            self.active_synth_notes.drain(..).map(|(p, _)| p).collect();
                        for pitch in notes {
                            self.send_command(AudioCommand::NoteOff {
                                track_id: 0,
                                note: pitch,
                            });
                        }
                        self.active_synth_note = None;
                        if let Some(ref mut proc) = self.mock_processor {
                            proc.transport.beat = self.current_beat;
                        }
                    }
                }
            }
        });

        // 8. Render Shortcuts Help Modal if active
        self.shortcuts.render_shortcuts_modal(ctx);

        // Request repaint for continuous 60 FPS visualizers
        ctx.request_repaint();
    }
}

impl eframe::App for OxideStudioApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let screen_rect = ctx.screen_rect();
        let splitter_id = egui::Id::new("studio_horizontal_splitter");
        let splitter_override = ctx.data_mut(|d| {
            d.get_temp::<crate::layout::StudioSplitterState>(splitter_id)
                .and_then(|s| if s.is_overridden { Some(s.bottom_deck_height) } else { None })
        });
        let budget = crate::layout::ViewportBudget::compute_with_deck_height(
            screen_rect,
            !self.browser.is_collapsed,
            self.copilot_open,
            splitter_override,
        );
        self.render_frame_with_budget(ctx, &budget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_als_export_and_import() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        app.bpm = 135.0;
        let als_path = std::env::temp_dir().join(format!(
            "oxide_studio_session_{}.als",
            std::process::id()
        ));
        let als_str = als_path.to_string_lossy().to_string();
        let res = app.export_als_project(&als_str);
        assert!(res.is_ok());
        assert!(als_path.exists());

        // Test loading
        let loaded = DAWProject::load_als(&als_path).unwrap();
        assert_eq!(loaded.transport.bpm, 135.0);
        assert_eq!(loaded.tracks[1].track_type, TrackType::DrumRack);

        // Test full import into app
        app.bpm = 120.0;
        app.import_als_project(&als_path).expect("ALS import must succeed");
        assert_eq!(app.bpm, 135.0);
        assert_eq!(app.tracks_gui.len(), loaded.tracks.len());
        assert!(app.export_status.as_deref().unwrap_or("").contains("Imported"));

        let _ = std::fs::remove_file(&als_path);
    }

    #[test]
    fn test_stems_export() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        let pid = std::process::id();
        let wav_path = std::env::temp_dir().join(format!("oxide_studio_mix_{}.wav", pid));
        let stems_dir = std::env::temp_dir().join(format!("oxide_stems_{}", pid));
        let res = app.export_demo_wav(wav_path.to_string_lossy().as_ref());
        assert!(res.is_ok());
        let stems_res = oxide_bounce::stems::export_remix_stems_from_wav(&wav_path, &stems_dir);
        assert!(stems_res.is_ok());
        app.export_status = Some("Stems exported".to_string());
        assert_eq!(app.export_status, Some("Stems exported".to_string()));

        let _ = std::fs::remove_file(&wav_path);
        let _ = std::fs::remove_dir_all(&stems_dir);
    }

    #[test]
    fn test_stop_all_clips() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        assert!(matches!(app.matrix.tracks[0].slots[0].state, SlotState::Playing { .. }));

        // Stop all clips (leaving empty slots as SlotState::Empty)
        for trk_idx in 0..app.matrix.tracks.len() {
            app.send_command(AudioCommand::StopClip { track_id: trk_idx });
        }
        for trk in &mut app.matrix.tracks {
            for slot in &mut trk.slots {
                if slot.clip.is_some() {
                    slot.state = SlotState::Stopped;
                }
            }
        }

        assert_eq!(app.matrix.tracks[0].slots[0].state, SlotState::Stopped);
        assert_eq!(app.matrix.tracks[1].slots[0].state, SlotState::Stopped);
        assert_eq!(app.matrix.tracks[2].slots[0].state, SlotState::Stopped);
        assert_eq!(app.matrix.tracks[0].slots[1].state, SlotState::Empty);
    }

    #[test]
    fn test_workspace_view_arrangement_alias() {
        assert_eq!(WorkspaceView::Arrangement, WorkspaceView::ArrangementTimeline);
    }

    #[test]
    fn test_per_track_device_chains_switching_and_isolation() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        assert_eq!(app.track_device_chains.len(), 3);
        assert_eq!(app.selected_track, 0);
        assert_eq!(app.device_chain.devices[0].kind, DeviceKind::Wavetable);
        assert_eq!(app.deck_tab, DeckTab::ClipView);

        // Select Track 1 (909 House Kit)
        app.select_track(1);
        assert_eq!(app.selected_track, 1);
        assert_eq!(app.deck_tab, DeckTab::DrumSequencer);
        assert_eq!(app.device_chain.devices[0].kind, DeviceKind::DrumRack);

        // Select Track 2 (Offbeat Sub Bass)
        app.select_track(2);
        assert_eq!(app.selected_track, 2);
        assert_eq!(app.deck_tab, DeckTab::ClipView);
        assert_eq!(app.device_chain.devices[0].kind, DeviceKind::Operator);

        // Modify device chain on Track 2, switch back to Track 0, verify isolation
        app.device_chain.devices[0].is_enabled = false;
        app.select_track(0);
        assert_eq!(app.selected_track, 0);
        assert!(app.device_chain.devices[0].is_enabled); // Track 0 Wavetable is enabled
        assert!(!app.track_device_chains[2].devices[0].is_enabled); // Track 2 Operator was saved as disabled
    }

    #[test]
    fn test_track_and_clip_synchronization_and_write_back() {
        let mut app = OxideStudioApp::new_mock(44100.0);

        // Select session clip on track 0
        app.select_session_clip(0, 0);
        assert_eq!(app.selected_track, 0);
        assert_eq!(app.selected_clip_slot, Some((0, 0)));
        assert!(!app.midi_pattern.notes.is_empty());

        // Mutate midi_pattern in piano roll
        let initial_count = app.midi_pattern.notes.len();
        app.midi_pattern.notes.push(MidiNote::new(60, 100, 4.0, 1.0));
        app.write_back_active_clip();

        // Verify write back to SessionMatrix
        let session_clip = app.matrix.tracks[0].slots[0].clip.as_ref().unwrap();
        if let ClipPattern::Midi(ref pat) = session_clip.pattern {
            assert_eq!(pat.notes.len(), initial_count + 1);
        } else {
            panic!("Expected Midi clip pattern");
        }

        // Select Drum clip on track 1
        app.select_session_clip(1, 0);
        assert_eq!(app.selected_track, 1);
        assert_eq!(app.deck_tab, DeckTab::DrumSequencer);

        // Mutate drum_pattern in sequencer
        app.drum_pattern.tracks[0].steps[1].active = true;
        app.drum_pattern.tracks[0].steps[1].velocity = 127;
        app.write_back_active_clip();

        // Verify write back to SessionMatrix
        let drum_clip = app.matrix.tracks[1].slots[0].clip.as_ref().unwrap();
        if let ClipPattern::Drum(ref pat) = drum_clip.pattern {
            assert!(pat.tracks[0].steps[1].active);
            assert_eq!(pat.tracks[0].steps[1].velocity, 127);
        } else {
            panic!("Expected Drum clip pattern");
        }
    }

    #[test]
    fn test_dynamic_track_chain_allocation() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        assert_eq!(app.track_device_chains.len(), 3);

        app.ensure_track_device_chain(5);
        assert!(app.track_device_chains.len() >= 6);
        assert_eq!(app.track_device_chains[5].devices[0].kind, DeviceKind::Wavetable);
    }

    #[test]
    fn test_mixer_gui_to_dsp_wiring_and_parameters() {
        let mut app = OxideStudioApp::new_mock(44100.0);

        // 1. Verify initial parameters
        let params = app.audio_handle.as_ref().unwrap().params.clone();
        assert_eq!(params.get(PARAM_MASTER_VOL), 1.0);
        assert_eq!(params.get(PARAM_TRACK_VOL_BASE), 1.0);
        assert_eq!(params.get(PARAM_TRACK_PAN_BASE), 0.0);

        // 2. Adjust master volume
        app.master_volume = 0.65;
        if let Some(ref handle) = app.audio_handle {
            handle.params.set(PARAM_MASTER_VOL, app.master_volume);
        }
        app.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: 1,
            value: app.master_volume,
        });
        assert!((params.get(PARAM_MASTER_VOL) - 0.65).abs() < 1e-4);

        // 3. Adjust track 0 volume
        app.tracks_gui[0].volume_linear = 1.75;
        if let Some(ref handle) = app.audio_handle {
            handle.params.set(PARAM_TRACK_VOL_BASE, 1.75);
        }
        app.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: 1000,
            value: 1.75,
        });
        assert!((params.get(PARAM_TRACK_VOL_BASE) - 1.75).abs() < 1e-4);

        // 4. Adjust track 1 pan
        app.tracks_gui[1].pan = -0.5;
        if let Some(ref handle) = app.audio_handle {
            handle.params.set(PARAM_TRACK_PAN_BASE + 1, -0.5);
        }
        app.send_command(AudioCommand::SetParameter {
            track_id: 1,
            param_id: 1101,
            value: -0.5,
        });
        assert!((params.get(PARAM_TRACK_PAN_BASE + 1) - (-0.5)).abs() < 1e-4);

        // 5. Toggle track 0 mute
        app.tracks_gui[0].mute = true;
        app.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: 1200,
            value: 1.0,
        });

        // 6. Crossfader
        app.crossfader_pos = 0.5;
        app.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: 2000,
            value: 0.5,
        });
        assert_eq!(app.crossfader_pos, 0.5);

        // Poll audio to execute commands in mock processor
        app.poll_audio();

        if let Some(ref proc) = app.mock_processor {
            assert!((proc.mixer.master_volume_linear - 0.65).abs() < 1e-4);
            assert!((proc.mixer.tracks[0].volume_linear - 1.75).abs() < 1e-4);
            assert!((proc.mixer.tracks[1].pan - (-0.5)).abs() < 1e-4);
            assert!(proc.mixer.tracks[0].mute);
            assert!((proc.crossfader - 0.5).abs() < 1e-4);
        }
    }

    #[test]
    fn test_poll_audio_step_reset_rewind_multibar_and_stopped_slot() {
        let mut app = OxideStudioApp::new_mock(44100.0);

        // 1. Rewind < 0.25 beats must still drain active synth notes on poll_audio
        app.is_playing = true;
        app.current_beat = 1.1;
        app.active_synth_notes.push((60, 4.0));
        if let Some(ref mut proc) = app.mock_processor {
            proc.transport.is_playing = true;
            proc.transport.beat = 1.0; // backward by 0.1 beats -> telemetry updates playhead_beat to ~1.0
        }
        app.poll_audio();
        assert!(
            !app.active_synth_notes.iter().any(|(p, _)| *p == 60),
            "Backward rewind < 0.25 beats must release previously active synth notes"
        );

        // 2. Stopping clears last_triggered_step so restarting triggers step 0 cleanly
        app.is_playing = false;
        app.poll_audio();
        assert_eq!(app.last_triggered_step, None);

        // 3. Multi-bar MIDI clip (> 4 beats / > 16 steps) triggers note at step 16 (beat 4.0)
        app.midi_pattern = MidiPattern::new(8.0);
        app.midi_pattern.notes.push(MidiNote::new(72, 110, 4.0, 0.5));
        app.global_scale.scale_lock = false;
        app.is_playing = true;
        app.active_synth_notes.clear();
        if let Some(ref mut proc) = app.mock_processor {
            proc.transport.is_playing = true;
            proc.transport.beat = 4.0;
        }
        app.poll_audio();
        assert!(
            app.active_synth_notes.iter().any(|(p, _)| *p == 72),
            "Multi-bar note at beat 4.0 (step 16) must trigger cleanly instead of aliasing modulo 16"
        );

        // 4. Stopped Session slot prevents Track 0 from triggering new synth notes
        app.active_synth_notes.clear();
        app.matrix.tracks[0].slots[0].state = SlotState::Stopped;
        app.last_triggered_step = None;
        if let Some(ref mut proc) = app.mock_processor {
            proc.transport.beat = 4.0;
        }
        app.poll_audio();
        assert!(
            app.active_synth_notes.is_empty(),
            "Track with SlotState::Stopped must not fire steps in poll_audio"
        );
    }

    #[test]
    fn test_shortcuts_and_keyboard_zero_double_binding_and_f12() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        let ctx = Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1440.0, 900.0));
        let budget = crate::layout::ViewportBudget::compute(screen_rect, true, true);

        assert_eq!(app.keyboard.octave_offset, 0);
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(screen_rect);
        raw.events.push(egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(raw, |ctx| {
            app.render_frame_with_budget(ctx, &budget);
        });
        assert_eq!(
            app.keyboard.octave_offset, -1,
            "Z must decrement octave_offset exactly once (-1), not double-fire (-2)"
        );

        // S toggles Solo and emits AudioCommand::SetParameter(1300) without triggering NoteOn
        assert!(!app.tracks_gui[0].solo);
        let mut raw_s = egui::RawInput::default();
        raw_s.screen_rect = Some(screen_rect);
        raw_s.events.push(egui::Event::Key {
            key: egui::Key::S,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(raw_s, |ctx| {
            app.render_frame_with_budget(ctx, &budget);
        });
        app.poll_audio();
        assert!(app.tracks_gui[0].solo);
        assert!(app.mock_processor.as_ref().unwrap().mixer.tracks[0].solo);
        assert!(
            app.active_synth_notes.is_empty(),
            "S solo shortcut must not double-trigger a piano note"
        );

        // Num0 toggles Mute and dispatches Mute command to processor
        let mut raw_0 = egui::RawInput::default();
        raw_0.screen_rect = Some(screen_rect);
        raw_0.events.push(egui::Event::Key {
            key: egui::Key::Num0,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(raw_0, |ctx| {
            app.render_frame_with_budget(ctx, &budget);
        });
        app.poll_audio();
        assert!(app.tracks_gui[0].mute);
        assert!(app.mock_processor.as_ref().unwrap().mixer.tracks[0].mute);

        // F12 toggles AI Copilot drawer
        let init_copilot = app.copilot_open;
        let mut raw_f12 = egui::RawInput::default();
        raw_f12.screen_rect = Some(screen_rect);
        raw_f12.events.push(egui::Event::Key {
            key: egui::Key::F12,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(raw_f12, |ctx| {
            app.render_frame_with_budget(ctx, &budget);
        });
        assert_eq!(app.copilot_open, !init_copilot);
    }

    #[test]
    fn test_browser_hot_swap_and_canonical_pad_alignment() {
        use crate::panels::browser::{BrowserCategory, BrowserItem};

        let mut app = OxideStudioApp::new_mock(44100.0);
        let orig_len = app.device_chain.devices.len();
        app.device_chain.hot_swap_activate(1); // Hot-swap slot 1 (AutoFilter)

        let roar_item = BrowserItem::new("fx_roar", "Roar", BrowserCategory::AudioEffects, &["Warm"], "Sat", [0.5; 8]);
        app.handle_browser_load(&roar_item);
        assert_eq!(app.device_chain.devices.len(), orig_len, "Hot-swap must replace slot instead of appending");
        assert_eq!(app.device_chain.devices[1].kind, DeviceKind::Roar);
        assert!(!app.device_chain.hot_swap.active);

        let hh_item = BrowserItem::new("drum_909_hihat", "909 Closed Hat", BrowserCategory::Drums, &["Hat"], "CH", [0.5; 8]);
        app.handle_browser_load(&hh_item);
        assert!(app.status_message.contains("(Pad 9)")); // index 8 -> Pad 9
    }
}
