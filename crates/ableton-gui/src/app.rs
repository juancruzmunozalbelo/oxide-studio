//! Main single-window Ableton Studio application.
//!
//! Enforces:
//! - Dark Carbon Studio Theme
//! - Zero-Menu-Diving Single-Window Studio Layout:
//!   - Top Transport Bar (~48px): Play/Stop/Rec, BPM, Swing, CPU, Oscilloscope, Spectrum, Export WAV
//!   - Center Workspace: Toggle between Session Matrix and Linear Arrangement Timeline
//!   - Bottom Deck: Tabbed between Device Rack (ZDF curve), 16-Step Sequencer, and XY Perform Pad
//! - Computer Keyboard Playing (Piano & Wicki-Hayden Isomorphic)
//! - Audio Engine live integration via lock-free queues & telemetry polling

use egui::{
    vec2, Button, CentralPanel, Context, Key, Layout, RichText, TopBottomPanel,
};
use ableton_audio::{
    AudioEngine, AudioEngineProcessor, AudioStreamHandle, PARAM_BPM,
    PARAM_SYNTH_CUTOFF, PARAM_SYNTH_MACRO_X, PARAM_SYNTH_MACRO_Y, PARAM_SYNTH_RESONANCE,
};
use ableton_bounce::{DemoArrangementRenderer, WavWriter};
use ableton_core::{AudioCommand, AudioTelemetry};
use ableton_timeline::{
    ArrangementTimeline, ClipPattern, DrumStepPattern, MidiNote, MidiPattern,
    SessionClip, SessionMatrix,
};

use crate::keyboard::{ComputerKeyboardController, KeyboardMode};
use crate::panels::{
    render_arrangement_timeline, render_device_rack, render_drum_sequencer, render_piano_roll,
    render_session_matrix, render_transport_bar, GuiTrackState, SynthControlsState,
};
use crate::theme::{
    apply_dark_carbon_theme, COLOR_AMBER, COLOR_CYAN, COLOR_GREEN, COLOR_TEXT_MUTED,
};
use crate::visualizers::XyPerformPadWidget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceView {
    SessionMatrix,
    ArrangementTimeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckTab {
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

    pub workspace_view: WorkspaceView,
    pub deck_tab: DeckTab,

    pub keyboard: ComputerKeyboardController,
    pub matrix: SessionMatrix,
    pub timeline: ArrangementTimeline,
    pub synth_state: SynthControlsState,
    pub drum_pattern: DrumStepPattern,
    pub midi_pattern: MidiPattern,
    pub xy_state: crate::visualizers::XyPerformPadState,
    pub tracks_gui: Vec<GuiTrackState>,
    pub has_session_override: bool,
    pub status_message: String,
    pub current_time_seconds: f64,
    pub last_triggered_step: Option<i64>,
    pub active_synth_note: Option<u8>,
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
            app.deck_tab = DeckTab::DrumSequencer;
            app.send_command(AudioCommand::SetTempo(app.bpm));
            app.send_command(AudioCommand::Play);
            app.status_message = "Playing: Deep House 909 Intro (124 BPM — F Dorian)".into();
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
        // Initialize 124 BPM Classic 909 House Intro pattern (Kick on 1/2/3/4, Offbeat Open Hat on 2/6/10/14, Snare/Clap on 4/12, Shuffled 16ths)
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
            MidiNote { pitch: 53, velocity: 102, start_beat: 1.25, duration_beats: 0.25, probability: 1.0 },
            MidiNote { pitch: 44, velocity: 114, start_beat: 1.5, duration_beats: 0.45, probability: 1.0 },
            MidiNote { pitch: 46, velocity: 116, start_beat: 2.5, duration_beats: 0.45, probability: 1.0 },
            MidiNote { pitch: 56, velocity: 98, start_beat: 3.0, duration_beats: 0.25, probability: 1.0 },
            MidiNote { pitch: 48, velocity: 120, start_beat: 3.5, duration_beats: 0.45, probability: 1.0 },
        ];
        let midi_pattern = MidiPattern {
            length_beats: 4.0,
            notes: midi_notes,
        };

        // Initialize Session Matrix with House Intro clips
        let mut matrix = SessionMatrix::new(3, 4);
        if let Some(t0) = matrix.tracks.get_mut(0) {
            t0.name = "F Dorian Chords".into();
            if let Some(slot0) = t0.slots.get_mut(0) {
                slot0.clip = Some(SessionClip::new(1, "F Dorian House Chords", ClipPattern::Midi(midi_pattern.clone())));
                slot0.state = ableton_timeline::SlotState::Playing { start_beat: 0.0, phase: 0.0 };
            }
        }
        if let Some(t1) = matrix.tracks.get_mut(1) {
            t1.name = "909 House Kit".into();
            if let Some(slot0) = t1.slots.get_mut(0) {
                slot0.clip = Some(SessionClip::new(2, "909 Classic House Groove", ClipPattern::Drum(drum_pattern.clone())));
                slot0.state = ableton_timeline::SlotState::Playing { start_beat: 0.0, phase: 0.0 };
            }
        }
        if let Some(t2) = matrix.tracks.get_mut(2) {
            t2.name = "Offbeat Sub Bass".into();
            if let Some(slot0) = t2.slots.get_mut(0) {
                slot0.clip = Some(SessionClip::new(3, "Offbeat House Bass", ClipPattern::Midi(midi_pattern.clone())));
                slot0.state = ableton_timeline::SlotState::Playing { start_beat: 0.0, phase: 0.0 };
            }
        }

        // Initialize Arrangement Timeline with 16-bar House Intro sections & Filter Cutoff Sweep
        let mut timeline = ArrangementTimeline::new();
        let mut trk1 = ableton_timeline::ArrangementTrack::new(1, "F Dorian Chords", "MidiSynth");
        let mut trk2 = ableton_timeline::ArrangementTrack::new(2, "909 House Kit", "DrumRack");
        let mut trk3 = ableton_timeline::ArrangementTrack::new(3, "Offbeat Sub Bass", "MidiSynth");
        for (i, sec) in ["Intro (909 + Filter Rise)", "Groove In", "Warm Chords", "Drop Ready"].iter().enumerate() {
            let start = (i * 16) as f64;
            trk1.add_clip(ableton_timeline::ArrangementClip::new(i as u32 + 1, *sec, start, 16.0, ClipPattern::Midi(midi_pattern.clone())));
            trk2.add_clip(ableton_timeline::ArrangementClip::new(i as u32 + 10, *sec, start, 16.0, ClipPattern::Drum(drum_pattern.clone())));
            trk3.add_clip(ableton_timeline::ArrangementClip::new(i as u32 + 20, *sec, start, 16.0, ClipPattern::Midi(midi_pattern.clone())));
        }
        timeline.tracks.push(trk1);
        timeline.tracks.push(trk2);
        timeline.tracks.push(trk3);

        let tracks_gui = vec![
            GuiTrackState { name: "F Dorian Chords".into(), volume_linear: 0.9, pan: 0.0, mute: false, solo: false },
            GuiTrackState { name: "909 House Kit".into(), volume_linear: 1.0, pan: 0.0, mute: false, solo: false },
            GuiTrackState { name: "Offbeat Sub Bass".into(), volume_linear: 0.88, pan: 0.0, mute: false, solo: false },
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
            workspace_view: WorkspaceView::SessionMatrix,
            deck_tab: DeckTab::DeviceRack,
            keyboard: ComputerKeyboardController::new(),
            matrix,
            timeline,
            synth_state: SynthControlsState::default(),
            drum_pattern,
            midi_pattern,
            xy_state: crate::visualizers::XyPerformPadState::default(),
            tracks_gui,
            has_session_override: false,
            status_message: "Studio Ready".into(),
            current_time_seconds: 0.0,
            last_triggered_step: None,
            active_synth_note: None,
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
        // If running with mock processor, drive mock buffer processing
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
                            // Map row 0 -> Kick (36), row 1 -> Rim/Clap (37), row 2 -> Snare (38), row 3/5 -> Hat (42/46)
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
                        next_synth_note = Some((n.pitch, n.velocity));
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

                // Smoothly sweep the ZDF filter cutoff & XY macro during the House intro
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

        // 3. Top Transport Bar Panel (~48px height)
        TopBottomPanel::top("top_transport_bar")
            .exact_height(48.0)
            .show(ctx, |ui| {
                let action = render_transport_bar(
                    ui,
                    self.is_playing,
                    self.is_recording,
                    &mut self.bpm,
                    &mut self.swing,
                    self.current_beat,
                    &self.telemetry,
                );

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
                if action.export_wav {
                    let _ = self.export_demo_wav("/tmp/ableton_studio_export.wav");
                }
            });

        // 4. Bottom Deck Panel (~240px height)
        TopBottomPanel::bottom("bottom_deck_panel")
            .exact_height(240.0)
            .show(ctx, |ui| {
                // Deck Tab Selector Header
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 0.0);

                    let dev_btn = Button::new(
                        RichText::new("Device Rack & Filter Curve")
                            .size(11.0)
                            .color(if self.deck_tab == DeckTab::DeviceRack { COLOR_AMBER } else { COLOR_TEXT_MUTED }),
                    );
                    if ui.add(dev_btn).clicked() {
                        self.deck_tab = DeckTab::DeviceRack;
                    }

                    let seq_btn = Button::new(
                        RichText::new("16-Step Sequencer & Piano Roll")
                            .size(11.0)
                            .color(if self.deck_tab == DeckTab::DrumSequencer { COLOR_CYAN } else { COLOR_TEXT_MUTED }),
                    );
                    if ui.add(seq_btn).clicked() {
                        self.deck_tab = DeckTab::DrumSequencer;
                    }

                    let xy_btn = Button::new(
                        RichText::new("XY Perform Pad")
                            .size(11.0)
                            .color(if self.deck_tab == DeckTab::XyPad { COLOR_GREEN } else { COLOR_TEXT_MUTED }),
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

                // Deck Content based on selected tab
                match self.deck_tab {
                    DeckTab::DeviceRack => {
                        let live_fc = self.synth_state.cutoff_hz;
                        let live_pw = self.synth_state.pulse_pw;
                        let rms_val = self.telemetry.master_rms_l.max(self.telemetry.master_rms_r);
                        let action = render_device_rack(
                            ui,
                            &mut self.synth_state,
                            Some(live_fc),
                            Some(rms_val),
                            Some(live_pw),
                        );

                        if let Some(fc) = action.cutoff_changed {
                            if let Some(ref handle) = self.audio_handle {
                                handle.params.set(PARAM_SYNTH_CUTOFF, fc);
                            }
                        }
                        if let Some(res) = action.resonance_changed {
                            if let Some(ref handle) = self.audio_handle {
                                handle.params.set(PARAM_SYNTH_RESONANCE, res);
                            }
                        }
                    }
                    DeckTab::DrumSequencer => {
                        let current_16th_step = ((self.current_beat * 4.0).floor() as usize) % 16;
                        let seq_action = render_drum_sequencer(ui, &mut self.drum_pattern, current_16th_step);

                        if let Some(pad_idx) = seq_action.pad_triggered {
                            // Trigger pad (MIDI note 36 + pad_idx on track 1)
                            self.send_command(AudioCommand::NoteOn {
                                track_id: 1,
                                note: 36 + pad_idx,
                                velocity: 110,
                            });
                        }

                        ui.add_space(8.0);
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
                                param_id: 100, // Macro X
                                value: norm_x,
                            });
                            self.send_command(AudioCommand::SetParameter {
                                track_id: 0,
                                param_id: 101, // Macro Y
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

        // 5. Center Workspace Panel (Flex Height)
        CentralPanel::default().show(ctx, |ui| {
            // Workspace view selector toolbar
            ui.horizontal(|ui| {
                let session_btn = Button::new(
                    RichText::new("Session Matrix")
                        .size(12.0)
                        .color(if self.workspace_view == WorkspaceView::SessionMatrix { COLOR_AMBER } else { COLOR_TEXT_MUTED }),
                );
                if ui.add(session_btn).clicked() {
                    self.workspace_view = WorkspaceView::SessionMatrix;
                }

                let arr_btn = Button::new(
                    RichText::new("Arrangement Timeline")
                        .size(12.0)
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
                        self.current_beat,
                        self.has_session_override,
                    );

                    if action.back_to_arrangement {
                        self.has_session_override = false;
                        for track in &mut self.matrix.tracks {
                            track.stop_immediate();
                        }
                    }
                }
            }
        });

        // Request repaint for continuous 60 FPS visualizers
        ctx.request_repaint();
    }
}
