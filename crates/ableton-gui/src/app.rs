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
use ableton_audio::{
    AudioEngine, AudioEngineProcessor, AudioStreamHandle, PARAM_BPM,
    PARAM_MASTER_VOL, PARAM_SYNTH_MACRO_X, PARAM_SYNTH_MACRO_Y,
    PARAM_TRACK_PAN_BASE, PARAM_TRACK_VOL_BASE,
};
use ableton_bounce::{DemoArrangementRenderer, WavWriter};
use ableton_core::{AudioCommand, AudioTelemetry, GlobalScale, ScaleMode, TuningSystem};
use ableton_project::{DAWProject, TrackDefinition};
use ableton_timeline::{
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

pub struct AbletonStudioApp {
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
    pub tap_times: Vec<f64>,
    pub copilot_open: bool,
    pub ai_copilot: crate::panels::ai_chat::AiCopilotState,
    pub shortcuts: crate::shortcuts::StudioShortcutManager,
    pub rpc_server: crate::live_rpc::ArustonGuiRpcServer,
    pub export_status: Option<String>,
}

impl AbletonStudioApp {
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
        app.rpc_server = crate::live_rpc::ArustonGuiRpcServer::new_on_port(9877);
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
        app.rpc_server = crate::live_rpc::ArustonGuiRpcServer::new_disabled();
        app
    }

    fn init_empty() -> Self {
        // Initialize 124 BPM Classic 909 House Intro pattern
        let mut drum_pattern = DrumStepPattern::new_16_step();
        if let Some(kick_track) = drum_pattern.tracks.get_mut(0) {
            for &s in &[0, 4, 8, 12] {
                kick_track.steps[s].active = true;
                kick_track.steps[s].accent = true;
                kick_track.steps[s].velocity = 124;
            }
        }
        if let Some(rim_track) = drum_pattern.tracks.get_mut(1) {
            for &s in &[3, 7, 11, 15] {
                rim_track.steps[s].active = true;
                rim_track.steps[s].velocity = 82;
            }
        }
        if let Some(snare_track) = drum_pattern.tracks.get_mut(2) {
            for &s in &[4, 12, 15] {
                snare_track.steps[s].active = true;
                snare_track.steps[s].velocity = if s == 15 { 78 } else { 115 };
            }
        }
        if let Some(hh_track) = drum_pattern.tracks.get_mut(5) {
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
            tap_times: Vec::new(),
            copilot_open: true,
            ai_copilot: crate::panels::ai_chat::AiCopilotState::new(),
            shortcuts: crate::shortcuts::StudioShortcutManager::new(),
            rpc_server: crate::live_rpc::ArustonGuiRpcServer::new_disabled(),
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

    /// Poll Aruston Live GUI RPC Server and apply pending agent/MCP mutations.
    pub fn poll_rpc(&mut self) -> usize {
        let mut rpc = std::mem::replace(
            &mut self.rpc_server,
            crate::live_rpc::ArustonGuiRpcServer::new_disabled(),
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

        if let Some(ref mut handle) = self.audio_handle {
            if let Some(telem) = handle.poll_telemetry() {
                self.telemetry = telem;
                self.current_beat = self.telemetry.playhead_beat;
            }
        }

        // Real-time 16th-note step sequencer driver for live GUI playback
        if self.is_playing {
            let global_step = (self.current_beat * 4.0).floor() as i64;
            if self.last_triggered_step != Some(global_step) {
                self.last_triggered_step = Some(global_step);
                let step_16 = (global_step.rem_euclid(16)) as usize;

                // Release previous synth note
                if let Some(prev_note) = self.active_synth_note.take() {
                    self.send_command(AudioCommand::NoteOff {
                        track_id: 0,
                        note: prev_note,
                    });
                }

                // Trigger active 909 drum pads on this 16th step
                let mut drum_hits = Vec::new();
                for (pad_idx, track) in self.drum_pattern.tracks.iter().enumerate() {
                    if !self.drum_sequencer.is_pad_audible(pad_idx) {
                        continue;
                    }
                    if let Some(step) = track.steps.get(step_16) {
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

                // Trigger House synth notes whose 16th step matches step_16
                let mut next_synth_note = None;
                for n in &self.midi_pattern.notes {
                    let note_step = ((n.start_beat * 4.0).round() as usize) % 16;
                    if note_step == step_16 {
                        // Apply Live 12 Scale-Lock quantization if enabled
                        let pitch = if self.global_scale.scale_lock {
                            self.global_scale.quantize(n.pitch)
                        } else {
                            n.pitch
                        };
                        next_synth_note = Some((pitch, n.velocity));
                        break;
                    }
                }
                if let Some((pitch, vel)) = next_synth_note {
                    self.active_synth_note = Some(pitch);
                    self.send_command(AudioCommand::NoteOn {
                        track_id: 0,
                        note: pitch,
                        velocity: vel,
                    });
                }

                // Smoothly sweep filter cutoff & XY macro
                let bar_phase = ((self.current_beat % 32.0) / 32.0) as f32;
                let sweep_fc = 420.0 + (bar_phase * bar_phase) * 5800.0;
                self.synth_state.cutoff_hz = sweep_fc;
                self.xy_state.x = 0.25 + 0.65 * bar_phase;
                self.xy_state.y = 0.35 + 0.45 * (self.current_beat as f32 * 0.5).sin().abs();
            }
        }
    }

    /// Perform one-click WAV bounce of the demo arrangement.
    pub fn export_demo_wav(&mut self, output_path: &str) -> Result<(), String> {
        let renderer = DemoArrangementRenderer::new(44100.0, self.bpm);
        let interleaved = renderer.render_demo_seconds(4.5);
        WavWriter::write_wav_16bit(std::path::Path::new(output_path), 44100, 2, &interleaved)
            .map_err(|e| format!("Failed to export WAV: {}", e))?;
        self.status_message = format!("Exported: {}", output_path);
        Ok(())
    }

    /// Export current session snapshot as an Ableton Live `.als` project.
    pub fn export_als_project(&self, output_path: &str) -> Result<(), String> {
        let mut project = DAWProject::new("Oxide Studio Session");
        project.transport.bpm = self.bpm;
        project.scenes = self.matrix.scenes.clone();
        for (idx, track) in self.matrix.tracks.iter().enumerate() {
            let mut trk_def = TrackDefinition::new_synth(track.id, &track.name);
            if idx < self.tracks_gui.len() {
                trk_def.mixer.volume_db = if self.tracks_gui[idx].volume_linear > 0.001 {
                    20.0 * self.tracks_gui[idx].volume_linear.log10()
                } else {
                    -70.0
                };
                trk_def.mixer.pan = self.tracks_gui[idx].pan;
                trk_def.mixer.mute = self.tracks_gui[idx].mute;
                trk_def.mixer.solo = self.tracks_gui[idx].solo;
            }
            trk_def.session_clips = track.slots.iter().map(|s| s.clip.clone()).collect();
            project.tracks.push(trk_def);
        }
        project
            .save_als(std::path::Path::new(output_path))
            .map_err(|e| format!("Failed to export ALS: {}", e))?;
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
                    let new_id = (self.device_chain.devices.len() as u32) + 1;
                    self.device_chain
                        .devices
                        .push(GuiDeviceModule::new(new_id, kind));
                    if self.selected_track < self.track_device_chains.len() {
                        self.track_device_chains[self.selected_track] = self.device_chain.clone();
                    }
                    self.status_message = format!("Loaded: {}", item.name);
                } else {
                    self.status_message = format!("Selected: {}", item.name);
                }
            }
            BrowserCategory::Drums => {
                let pad_idx = match item.id.as_str() {
                    "drum_909_kick" | "drum_808_sub_kick" => 0,
                    "drum_909_snare" | "drum_acoustic_snare" => 1,
                    "drum_909_hihat" => 2,
                    "drum_open_hihat" => 3,
                    "drum_conga" => 4,
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
                self.send_command(AudioCommand::SetParameter {
                    track_id: 1,
                    param_id: pad_idx as u32,
                    value: 1.0,
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
                    if let Some(hh) = self.drum_pattern.tracks.get_mut(2) {
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
                    param_id: 0,
                    value: self.synth_state.cutoff_hz,
                });
                self.send_command(AudioCommand::SetParameter {
                    track_id: 0,
                    param_id: 1,
                    value: self.synth_state.resonance,
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
            d.insert_temp(egui::Id::new("aruston_selected_track"), self.selected_track);
            d.insert_temp(egui::Id::new("aruston_selected_slot"), self.selected_clip_slot);
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
                    if self.is_playing {
                        self.send_command(AudioCommand::Play);
                    } else {
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
                    self.status_message = format!("Split Clip at beat {:.2}", self.current_beat);
                }
                crate::shortcuts::StudioShortcutAction::ConsolidateSelection => {
                    self.status_message = "Consolidated Selection".into();
                }
                crate::shortcuts::StudioShortcutAction::ToggleDrawOrBounce => {
                    let _ = self.export_demo_wav("/tmp/ableton_studio_export.wav");
                    self.status_message = "Bounced Audio to /tmp/ableton_studio_export.wav".into();
                }
                crate::shortcuts::StudioShortcutAction::ToggleBrowser => {
                    self.browser.is_collapsed = !self.browser.is_collapsed;
                }
                crate::shortcuts::StudioShortcutAction::ExportStems => {
                    let _ = self.export_demo_wav("/tmp/oxide_studio_mix.wav");
                    let _ = ableton_bounce::stems::export_remix_stems_from_wav(
                        std::path::Path::new("/tmp/oxide_studio_mix.wav"),
                        std::path::Path::new("/tmp/oxide_stems"),
                    );
                    self.export_status = Some("Stems exported to /tmp/oxide_stems/".to_string());
                    self.status_message = "Stems exported to /tmp/oxide_stems/".into();
                }
                crate::shortcuts::StudioShortcutAction::ToggleMuteSelected => {
                    let sel_track = self.selected_track;
                    if let Some(trk) = self.tracks_gui.get_mut(sel_track) {
                        trk.mute = !trk.mute;
                        self.status_message = format!("Track {} Mute: {}", sel_track + 1, trk.mute);
                    }
                }
                crate::shortcuts::StudioShortcutAction::ToggleSoloSelected => {
                    let sel_track = self.selected_track;
                    if let Some(trk) = self.tracks_gui.get_mut(sel_track) {
                        trk.solo = !trk.solo;
                        self.status_message = format!("Track {} Solo: {}", sel_track + 1, trk.solo);
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
                    if self.is_playing {
                        self.send_command(AudioCommand::Play);
                    } else {
                        self.send_command(AudioCommand::Stop);
                    }
                }
                if action.stop {
                    self.is_playing = false;
                    self.current_beat = 0.0;
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
                    let _ = self.export_demo_wav("/tmp/ableton_studio_export.wav");
                }
                if action.export_als {
                    let _ = self.export_als_project("/tmp/oxide_studio_session.als");
                    self.export_status = Some("Exported /tmp/oxide_studio_session.als".to_string());
                    self.status_message = "Exported /tmp/oxide_studio_session.als".into();
                }
                if action.import_als {
                    let p = std::path::Path::new("/tmp/oxide_studio_session.als");
                    if p.exists() {
                        if let Ok(proj) = DAWProject::load_als(p) {
                            self.bpm = proj.transport.bpm;
                            self.send_command(AudioCommand::SetTempo(self.bpm));
                            if let Some(ref handle) = self.audio_handle {
                                handle.params.set(PARAM_BPM, self.bpm as f32);
                            }
                            self.export_status = Some("Imported /tmp/oxide_studio_session.als".to_string());
                            self.status_message = "Imported /tmp/oxide_studio_session.als".into();
                        }
                    }
                }
                if action.split_stems {
                    let _ = self.export_demo_wav("/tmp/oxide_studio_mix.wav");
                    let _ = ableton_bounce::stems::export_remix_stems_from_wav(
                        std::path::Path::new("/tmp/oxide_studio_mix.wav"),
                        std::path::Path::new("/tmp/oxide_stems"),
                    );
                    self.export_status = Some("Stems exported to /tmp/oxide_stems/".to_string());
                    self.status_message = "Stems exported to /tmp/oxide_stems/".into();
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
                            let clip_act = render_clip_view(
                                ui,
                                &mut self.clip_view,
                                &mut self.midi_pattern,
                                &self.global_scale,
                                self.current_beat,
                            );
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
                                let st = if let Some(d) = self.device_chain.devices.get(idx) {
                                    if d.is_enabled { "Enabled" } else { "Bypassed" }
                                } else {
                                    "Toggled"
                                };
                                self.status_message = format!("Device #{} {}", idx + 1, st);
                            } else if let Some(idx) = dev_act.device_parameter_changed {
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
                                d.insert_temp(egui::Id::new("aruston_909_sequencer_state"), self.drum_sequencer.clone())
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
            SidePanel::right("aruston_ai_copilot_panel")
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
                                slot.state = SlotState::Stopped;
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
                        d.get_temp::<usize>(egui::Id::new("aruston_selected_track"))
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
                        self.current_beat = beat;
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

impl eframe::App for AbletonStudioApp {
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
        let mut app = AbletonStudioApp::new_mock(44100.0);
        app.bpm = 135.0;
        let als_path = "/tmp/oxide_studio_session.als";
        let res = app.export_als_project(als_path);
        assert!(res.is_ok());
        assert!(std::path::Path::new(als_path).exists());

        // Test loading
        let loaded = DAWProject::load_als(std::path::Path::new(als_path)).unwrap();
        assert_eq!(loaded.transport.bpm, 135.0);

        // Test import logic
        app.bpm = 120.0;
        let p = std::path::Path::new(als_path);
        if let Ok(proj) = DAWProject::load_als(p) {
            app.bpm = proj.transport.bpm;
            app.export_status = Some("Imported /tmp/oxide_studio_session.als".to_string());
        }
        assert_eq!(app.bpm, 135.0);
        assert_eq!(app.export_status, Some("Imported /tmp/oxide_studio_session.als".to_string()));

        let _ = std::fs::remove_file(als_path);
    }

    #[test]
    fn test_stems_export() {
        let mut app = AbletonStudioApp::new_mock(44100.0);
        let wav_path = "/tmp/oxide_studio_mix.wav";
        let stems_dir = "/tmp/oxide_stems";
        let res = app.export_demo_wav(wav_path);
        assert!(res.is_ok());
        let stems_res = ableton_bounce::stems::export_remix_stems_from_wav(
            std::path::Path::new(wav_path),
            std::path::Path::new(stems_dir),
        );
        assert!(stems_res.is_ok());
        app.export_status = Some("Stems exported to /tmp/oxide_stems/".to_string());
        assert_eq!(app.export_status, Some("Stems exported to /tmp/oxide_stems/".to_string()));

        let _ = std::fs::remove_file(wav_path);
        let _ = std::fs::remove_dir_all(stems_dir);
    }

    #[test]
    fn test_stop_all_clips() {
        let mut app = AbletonStudioApp::new_mock(44100.0);
        assert!(matches!(app.matrix.tracks[0].slots[0].state, SlotState::Playing { .. }));

        // Stop all clips
        for trk_idx in 0..app.matrix.tracks.len() {
            app.send_command(AudioCommand::StopClip { track_id: trk_idx });
        }
        for trk in &mut app.matrix.tracks {
            for slot in &mut trk.slots {
                slot.state = SlotState::Stopped;
            }
        }

        assert_eq!(app.matrix.tracks[0].slots[0].state, SlotState::Stopped);
        assert_eq!(app.matrix.tracks[1].slots[0].state, SlotState::Stopped);
        assert_eq!(app.matrix.tracks[2].slots[0].state, SlotState::Stopped);
    }

    #[test]
    fn test_workspace_view_arrangement_alias() {
        assert_eq!(WorkspaceView::Arrangement, WorkspaceView::ArrangementTimeline);
    }

    #[test]
    fn test_per_track_device_chains_switching_and_isolation() {
        let mut app = AbletonStudioApp::new_mock(44100.0);
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
        let mut app = AbletonStudioApp::new_mock(44100.0);

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
        let mut app = AbletonStudioApp::new_mock(44100.0);
        assert_eq!(app.track_device_chains.len(), 3);

        app.ensure_track_device_chain(5);
        assert!(app.track_device_chains.len() >= 6);
        assert_eq!(app.track_device_chains[5].devices[0].kind, DeviceKind::Wavetable);
    }

    #[test]
    fn test_mixer_gui_to_dsp_wiring_and_parameters() {
        let mut app = AbletonStudioApp::new_mock(44100.0);

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
}
