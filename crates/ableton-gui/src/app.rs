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
    vec2, Button, CentralPanel, Context, Key, Layout, RichText, SidePanel, TopBottomPanel,
};
use ableton_audio::{
    AudioEngine, AudioEngineProcessor, AudioStreamHandle, PARAM_BPM,
    PARAM_SYNTH_MACRO_X, PARAM_SYNTH_MACRO_Y,
};
use ableton_bounce::{DemoArrangementRenderer, WavWriter};
use ableton_core::{AudioCommand, AudioTelemetry, GlobalScale, ScaleMode, TuningSystem};
use ableton_timeline::{
    ArrangementClip, ArrangementTimeline, ArrangementTrack, ClipPattern, DrumStepPattern,
    MidiNote, MidiPattern, SessionClip, SessionMatrix, SlotState,
};

use crate::keyboard::{ComputerKeyboardController, KeyboardMode};
use crate::panels::{
    render_arrangement_timeline, render_browser_panel, render_clip_view, render_device_chain_view,
    render_drum_sequencer, render_piano_roll, render_session_matrix, render_top_control_bar,
    BrowserState, ClipViewState, CrossfaderAssign,
    DeviceChainViewState, DeviceKind, GuiDeviceModule, GuiReturnTrack,
    GuiTrackState, LaunchQuantization, SessionMixerSections,
    SynthControlsState, TimelineGuiState,
};
use crate::theme::{
    apply_dark_carbon_theme, COLOR_AMBER, COLOR_CYAN, COLOR_GREEN, COLOR_TEXT_MUTED,
};
use crate::visualizers::{XyPerformPadState, XyPerformPadWidget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceView {
    SessionMatrix,
    ArrangementTimeline,
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
    pub device_chain: DeviceChainViewState,

    pub keyboard: ComputerKeyboardController,
    pub synth_state: SynthControlsState,
    pub drum_pattern: DrumStepPattern,
    pub midi_pattern: MidiPattern,
    pub xy_state: XyPerformPadState,
    pub tracks_gui: Vec<GuiTrackState>,
    pub has_session_override: bool,
    pub status_message: String,
    pub current_time_seconds: f64,
    pub last_triggered_step: Option<i64>,
    pub active_synth_note: Option<u8>,
    pub tap_times: Vec<f64>,
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
        if cc.is_some() {
            app.is_playing = true;
            app.deck_tab = DeckTab::ClipView;
            app.send_command(AudioCommand::SetTempo(app.bpm));
            app.send_command(AudioCommand::Play);
            app.status_message = "Live 12 Studio Ready (124 BPM — F Dorian)".into();
        }
        app
    }

    /// Headless constructor for testing without eframe context.
    pub fn new_mock(sample_rate: f32) -> Self {
        let (handle, processor) = AudioEngine::start_mock(sample_rate);
        let mut app = Self::init_empty();
        app.audio_handle = Some(handle);
        app.mock_processor = Some(processor);
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
            device_chain: DeviceChainViewState::default(),
            keyboard: ComputerKeyboardController::new(),
            synth_state: SynthControlsState::default(),
            drum_pattern,
            midi_pattern,
            xy_state: XyPerformPadState::default(),
            tracks_gui,
            has_session_override: false,
            status_message: "Studio Ready".into(),
            current_time_seconds: 0.0,
            last_triggered_step: None,
            active_synth_note: None,
            tap_times: Vec::new(),
        }
    }

    /// Send a command to the audio engine.
    pub fn send_command(&mut self, cmd: AudioCommand) {
        if let Some(ref mut handle) = self.audio_handle {
            handle.ui_handles.send_command(cmd);
        }
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
                    if let Some(step) = track.steps.get(step_16) {
                        if step.active {
                            let midi_note = match pad_idx {
                                0 => 36,
                                1 => 39,
                                2 => 38,
                                3 => 42,
                                _ => 36 + (pad_idx as u8).min(15),
                            };
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
}

impl eframe::App for AbletonStudioApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.current_time_seconds = ctx.input(|i| i.time);

        // 1. Poll real-time audio telemetry
        self.poll_audio();

        // 2. Intercept computer keyboard events for note playing & octave shifting
        let key_commands = self.keyboard.process_events(ctx);
        for cmd in key_commands {
            self.send_command(cmd);
        }

        // Toggle workspace view with Tab key
        if ctx.input(|i| i.key_pressed(Key::Tab)) {
            self.workspace_view = match self.workspace_view {
                WorkspaceView::SessionMatrix => WorkspaceView::ArrangementTimeline,
                WorkspaceView::ArrangementTimeline => WorkspaceView::SessionMatrix,
            };
        }

        // 3. Top Control & Scale Bar Panel (~48px height)
        TopBottomPanel::top("top_control_bar")
            .exact_height(48.0)
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
                );

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
                    self.status_message = format!("Global Scale: {}", self.global_scale.display_name());
                }
                if action.export_wav {
                    let _ = self.export_demo_wav("/tmp/ableton_studio_export.wav");
                }
            });

        // 4. Left Collapsible Browser Panel (220-300px)
        if !self.browser.is_collapsed {
            SidePanel::left("left_browser_panel")
                .exact_width(self.browser.width.clamp(220.0, 300.0))
                .resizable(true)
                .show(ctx, |ui| {
                    let browser_action = render_browser_panel(ui, &mut self.browser);

                    if let Some(item) = browser_action.load_item {
                        // Load item into active track or chain
                        let dev_kind = match item.id.as_str() {
                            "inst_wavetable" => Some(DeviceKind::Wavetable),
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
                            self.device_chain.devices.push(GuiDeviceModule::new(new_id, kind));
                            self.status_message = format!("Loaded: {}", item.name);
                        } else {
                            self.status_message = format!("Selected: {}", item.name);
                        }
                    }
                });
        } else {
            // Collapsed Strip with Quick-Expand Button
            SidePanel::left("left_browser_collapsed")
                .exact_width(24.0)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        let expand_btn = Button::new(RichText::new("▶").size(11.0).color(COLOR_AMBER));
                        if ui.add(expand_btn).on_hover_text("Open Browser").clicked() {
                            self.browser.is_collapsed = false;
                        }
                    });
                });
        }

        // 5. Bottom Detail View Panel (~240px height)
        TopBottomPanel::bottom("bottom_detail_panel")
            .exact_height(240.0)
            .show(ctx, |ui| {
                // Header Tabs Selector
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                    let clip_btn = Button::new(
                        RichText::new("Clip View (Piano Roll / Warping)")
                            .size(11.0)
                            .color(if self.deck_tab == DeckTab::ClipView { COLOR_AMBER } else { COLOR_TEXT_MUTED }),
                    );
                    if ui.add(clip_btn).clicked() {
                        self.deck_tab = DeckTab::ClipView;
                    }

                    let dev_btn = Button::new(
                        RichText::new("Device Chain View")
                            .size(11.0)
                            .color(if self.deck_tab == DeckTab::DeviceRack { COLOR_CYAN } else { COLOR_TEXT_MUTED }),
                    );
                    if ui.add(dev_btn).clicked() {
                        self.deck_tab = DeckTab::DeviceRack;
                    }

                    let seq_btn = Button::new(
                        RichText::new("16-Step 909 Sequencer")
                            .size(11.0)
                            .color(if self.deck_tab == DeckTab::DrumSequencer { COLOR_GREEN } else { COLOR_TEXT_MUTED }),
                    );
                    if ui.add(seq_btn).clicked() {
                        self.deck_tab = DeckTab::DrumSequencer;
                    }

                    let xy_btn = Button::new(
                        RichText::new("XY Perform Pad")
                            .size(11.0)
                            .color(if self.deck_tab == DeckTab::XyPad { COLOR_AMBER } else { COLOR_TEXT_MUTED }),
                    );
                    if ui.add(xy_btn).clicked() {
                        self.deck_tab = DeckTab::XyPad;
                    }

                    // Keyboard Mode Indicator on right
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        let kb_text = match self.keyboard.mode {
                            KeyboardMode::Piano => format!("Keys: Piano (Octave {})", self.keyboard.octave_offset),
                            KeyboardMode::Isomorphic => "Keys: Wicki-Hayden Isomorphic".into(),
                        };
                        let kb_btn = Button::new(RichText::new(kb_text).size(10.0).color(COLOR_TEXT_MUTED));
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
                        if clip_act.pattern_mutated {
                            self.status_message = "MIDI Pattern Updated via Live 12 Tool".into();
                        }
                    }
                    DeckTab::DeviceRack => {
                        let dev_act = render_device_chain_view(
                            ui,
                            &mut self.device_chain,
                            &self.telemetry.spectrum_bins,
                        );
                        if let Some(kind) = dev_act.insert_device {
                            self.status_message = format!("Inserted Device: {}", kind.name());
                        }
                    }
                    DeckTab::DrumSequencer => {
                        let current_16th_step = ((self.current_beat * 4.0).floor() as usize) % 16;
                        let seq_action = render_drum_sequencer(ui, &mut self.drum_pattern, current_16th_step);

                        if let Some(pad_idx) = seq_action.pad_triggered {
                            self.send_command(AudioCommand::NoteOn {
                                track_id: 1,
                                note: 36 + pad_idx,
                                velocity: 110,
                            });
                        }

                        ui.add_space(6.0);
                        render_piano_roll(ui, &mut self.midi_pattern, self.current_beat);
                    }
                    DeckTab::XyPad => {
                        let rms_bloom = self.telemetry.master_rms_l.max(self.telemetry.master_rms_r);
                        let xy_widget = XyPerformPadWidget::new(
                            &mut self.xy_state,
                            self.current_time_seconds,
                            rms_bloom,
                            vec2(ui.available_width().min(420.0), 180.0),
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

        // 6. Center Workspace Panel (Dual View: Session vs Arrangement)
        CentralPanel::default().show(ctx, |ui| {
            // Workspace Selector Toolbar
            ui.horizontal(|ui| {
                let session_btn = Button::new(
                    RichText::new("Session View")
                        .size(11.5)
                        .color(if self.workspace_view == WorkspaceView::SessionMatrix { COLOR_AMBER } else { COLOR_TEXT_MUTED }),
                );
                if ui.add(session_btn).clicked() {
                    self.workspace_view = WorkspaceView::SessionMatrix;
                }

                let arr_btn = Button::new(
                    RichText::new("Arrangement View")
                        .size(11.5)
                        .color(if self.workspace_view == WorkspaceView::ArrangementTimeline { COLOR_CYAN } else { COLOR_TEXT_MUTED }),
                );
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
                    if let Some(scene) = action.trigger_scene {
                        self.send_command(AudioCommand::TriggerScene(scene));
                        self.has_session_override = true;
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

        // Request repaint for continuous 60 FPS visualizers
        ctx.request_repaint();
    }
}
