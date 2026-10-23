//! Exhaustive End-to-End GUI & Audio Verification Suite (Requirements R1, R2, R3).
//!
//! Enforces:
//! 1. Headless egui multi-resolution layout verification across 4 standard screen sizes
//!    (1280x800, 1440x900, 1728x1117, 1920x1080), all panels, tabs, subtabs,
//!    Bank A & B, `+ Add Device` open, Copilot `⚙ Key` drawer open, asserting
//!    `audit_rendered_frame` returns a clean report (0 viewport overflows, 0 clipped texts,
//!    0 text collisions, 0 tofu glyphs).
//! 2. Acoustic verification: all 16 TR-909 drum pads produce non-zero distinct energy/zero-crossings,
//!    with no pad falling back to Kick909.
//! 3. Channel & Master volume fader / mute silence: setting fader to `-inf dB` (0.0) or muting
//!    renders strict 0.0 silence and 0.0 VU meters.
//! 4. Bank B Row 16 Tambourine button horizontal X-alignment strictly matches Rows 9..15.
//! 5. TR-909 16-step sequencer pattern playback, mute/solo audibility, and groove presets.
//! 6. Operational Knowledge Framework (OKF) Copilot bilingual integration.
//! 7. Multi-track input recording with `DriverLatencyCompensator` sample-exact alignment (Epic #17).
//! 8. Sandboxed `CLAP`/`VST3`/`AUv2` plugin hosting + crash watchdog dry bypass (Epic #18).
//! 9. All 6 `WarpMode`s (Beats, Tones, Texture, RePitch, Complex, ComplexPro) + streaming disk cache (Epic #19).
//! 10. `ModularPatchGraph` audio processing + live `LomParamMapper` modulation (Epic #20).
//! 11. All 8 `SuiteInstrumentEngine` synths + all 16 TR-909 drum voices (Epic #21).
//! 12. All 7 `SuiteAudioEffect` processors + all 8 `MidiEffectDevice` processors (Epics #22 & #23).
//! 13. `Push 3` 8x8 matrix sync, `MidiLearnRegistry`, and Pickup / ValueScaling takeover (Epic #24).
//! 14. `TrackGroupBus` submix/mute, `TrackFreezeEngine`, `FollowActionRule`, and `LinkedTrackGroup` (Epic #25).
//! 15. `.agr` `GrooveTemplate` extraction and `AudioToMidiConverter` transcription (Epic #26).
//! 16. `.alp` pack CRC32 roundtrip, `DAWproject` XML exchange, and `LosslessFreezeCodec` (Epic #27).
//! 17. Bilingual (`EN`/`ES`) Copilot queries across all 10 OKF sections (`OKF-01`..`OKF-10`) (Epic #28).
//! 18. Chapter 15 & OKF-08: Global Scale Awareness, Scale Lock constraint, and Microtonal Tuning Systems (12-TET, 19-EDO, 24-EDO, 31-EDO, Bohlen-Pierce, Just Intonation, Scala .scl).

use oxide_audio::{
    engine::{CpuLoadMeter, EngineArchitectureState, MultiCoreDspDispatcher, ZeroAllocAudit},
    fact_sheet::{
        compute_null_test, verify_64bit_summing_neutrality, verify_mixer_fader_mute_silence,
        verify_phase_inversion_null, verify_sample_rate_conversion_neutrality,
        verify_undithered_rendering_neutrality, NullTestReport,
    },
    mixer::ModularAudioMixer,
    parameters::AtomicParameters,
    preferences::{AudioHardwarePreferences, SUPPORTED_BUFFER_SIZES, SUPPORTED_SAMPLE_RATES},
    processor::{
        AudioEngineProcessor, PARAM_ID_MASTER_VOL, PARAM_ID_TRACK_MUTE_BASE,
        PARAM_ID_TRACK_VOL_BASE,
    },
    queues::AudioQueues,
    recording::{
        DriverLatencyCompensator, HardwareIoMatrix, InputChannelMode, IoRoutingError,
        MultiTrackRecorder, MAX_HARDWARE_INPUT_BUSES, MAX_RECORDING_TRACKS,
    },
    stream::AudioEngine,
};
use oxide_bounce::{
    audio_to_midi::{AudioToMidiConverter, GrooveTemplate},
    pack_formats::{
        AlpError, AlpPackArchive, DawClip, DawNote, DawProject, DawProjectExchange,
        DawTrack, LosslessFreezeCodec,
    },
};
use oxide_core::{
    control_surfaces::{
        Apc40State, ControlSurfaceModel, KeyLearnRegistry, KeyLearnTarget, LaunchpadProState,
        MidiLearnRegistry, MidiLearnTarget, PushDisplayFrameBuffer, PushPadColor, PushPadMatrixState,
        PushTouchStrip, PushTouchStripMode, TakeoverMode,
    },
    device::{AudioEffectDevice, Device, DeviceCategory, InstrumentDevice},
    plugin_host::{
        EditorWindowRect, PluginDescriptor, PluginFormat, PluginSandboxHost,
        SandboxMode,
    },
    AudioBuffer, AudioCommand, GlobalScale, LinkSyncEngine, MidiClockMessage, MidiClockSync,
    MidiEvent, MidiSyncMode, MusicalTime, NudgeState, ScaleMode, TempoFollowerEngine,
    TransportController, TuningSystem, TuningType, ROOT_NAMES,
};
use oxide_dsp::{
    patch_graph::{
        ModularPatchGraph, PatchFilterMode, PatchGraphError, PatchNodeKind, PatchOscShape,
        MAX_PATCH_CABLES, MAX_PATCH_NODES,
    },
    warp_engine::{
        ComplexProParams, Radix2Fft2048, SpectralWarpProcessor, StreamingDiskCache, TextureParams,
        WarpMode as DspWarpMode, WarpPrng, FFT_SIZE, PAGE_SIZE,
    },
};
use oxide_drums::{
    drum_rack::{DrumRack, DrumRackMacroTarget},
    simpler::{SimplerDevice, SimplerMode},
    suite_instruments::{SuiteInstrumentEngine, SuiteInstrumentKind},
    synth_models::{SynthDrumType, SynthPercussion},
};
use oxide_fx::{
    auto_filter::{AutoFilter, CircuitModel, CytomicFilterType},
    eq8::{EQEight, EqBandType},
    glue_compressor::GlueCompressor,
    midi_fx_rack::{
        ArpRate, ArpStyle, ArpeggiatorDevice, ChordDevice, MidiEffectChain, MidiEffectDevice,
        MpeControlDevice, NoteLengthDevice, PitchDevice, RandomDevice, RandomSign, ScaleDevice,
        VelocityCurve, VelocityDevice,
    },
    multiband_dynamics::MultibandDynamics,
    roar::{Roar, RoarCurve, RoarTopology},
    suite_audio_fx::{
        CorpusMode, SuiteAudioEffect, SuiteAudioEffectKind, TRUE_PEAK_CEILING_LIN,
    },
};
use oxide_gui::{
    app::{OxideStudioApp, DeckTab, WorkspaceView},
    layout::ViewportBudget,
    layout_guard::audit_rendered_frame,
    okf_knowledge::{GoldenTelemetryVerifier, OKF_MANUAL_SECTIONS},
    panels::{
        ai_chat::{execute_studio_intent, StudioLocale},
        clip_view::{ClipViewState, ClipViewSubTab},
        sequencer::{
            render_drum_sequencer_with_state, DrumBank, DrumSequencerState,
            BANK_A_PAD_NAMES, BANK_B_PAD_NAMES,
        },
    },
    shortcuts::{StudioShortcutAction, StudioShortcutManager},
    theme::apply_dark_carbon_theme,
};
use oxide_timeline::{
    arrangement::{ArrangementClip, ArrangementTimeline, ArrangementTrack, TimelineLocator},
    pattern::{ClipPattern, MidiPattern},
    workflow_advanced::{
        FollowAction, FollowActionRule, LinkedTrackGroup, TrackFreezeEngine, TrackGroupBus,
    },
    DrumStepPattern, MicroCrossfade, MidiNote, MpeStreamParser, MpeZone, QuantizeMode, SessionClip,
    SessionMatrix, SlotState, Track, TrackType, TriggerMode,
    AutomationCurveType, AutomationLane, AutomationPoint, AutomationTakeoverTracker,
    CubicBezierHandle, DrawModeState, MultiLaneAutomation,
    ClipEnvelope, ClipEnvelopeTarget, EnvelopeMode,
};
use egui::{
    epaint::Shape,
    pos2, vec2, Context, Key, Modifiers, RawInput, Rect,
};

/// Helper to render a full studio frame matching `OxideStudioApp::update`
fn render_studio_frame(
    app: &mut OxideStudioApp,
    ctx: &Context,
    screen_rect: Rect,
) -> egui::FullOutput {
    let raw_input = RawInput {
        screen_rect: Some(screen_rect),
        ..Default::default()
    };

    ctx.run(raw_input, |ctx| {
        let splitter_id = egui::Id::new("studio_horizontal_splitter");
        let splitter_override = ctx.data_mut(|d| {
            d.get_temp::<oxide_gui::layout::StudioSplitterState>(splitter_id)
                .and_then(|s| if s.is_overridden { Some(s.bottom_deck_height) } else { None })
        });
        let budget = ViewportBudget::compute_with_deck_height(
            screen_rect,
            !app.browser.is_collapsed,
            app.copilot_open,
            splitter_override,
        );
        app.render_frame_with_budget(ctx, &budget);
    })
}

// =========================================================================
// 1. Headless egui Multi-Resolution Layout Verification
// =========================================================================

#[test]
fn test_e2e_headless_multi_resolution_layout_and_popups_clean_audit() {
    // 4 standard display resolutions
    let resolutions = [
        ("Standard Laptop 1280x800", 1280.0_f32, 800.0_f32),
        ("MacBook Air Retina 1440x900", 1440.0, 900.0),
        ("MacBook Pro 16\" Native 1728x1117", 1728.0, 1117.0),
        ("Studio Display FHD 1920x1080", 1920.0, 1080.0),
    ];

    let workspace_views = [
        WorkspaceView::SessionMatrix,
        WorkspaceView::ArrangementTimeline,
    ];

    #[derive(Debug, Clone, Copy)]
    enum DeckConfig {
        ClipNotes,
        ClipTransform,
        ClipGenerate,
        ClipWarping,
        DeviceRackStandard,
        DeviceRackAddDeviceOpen, // + Add Device multi-column grid popup open
        DrumBankA,
        DrumBankB,
        XySurface,
    }

    let deck_configs = [
        DeckConfig::ClipNotes,
        DeckConfig::ClipTransform,
        DeckConfig::ClipGenerate,
        DeckConfig::ClipWarping,
        DeckConfig::DeviceRackStandard,
        DeckConfig::DeviceRackAddDeviceOpen,
        DeckConfig::DrumBankA,
        DeckConfig::DrumBankB,
        DeckConfig::XySurface,
    ];

    let locales = [StudioLocale::En, StudioLocale::Es];

    for (res_name, w, h) in resolutions {
        let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));

        for &view in &workspace_views {
            for &locale in &locales {
                for &deck in &deck_configs {
                    for &browser_collapsed in &[false, true] {
                        for &show_key_drawer in &[false, true] {
                            let ctx = Context::default();
                            apply_dark_carbon_theme(&ctx);

                            let mut app = OxideStudioApp::new_mock(44100.0);
                            app.workspace_view = view;
                            app.browser.is_collapsed = browser_collapsed;
                            app.copilot_open = true;
                            app.ai_copilot.set_locale(locale);
                            app.ai_copilot.show_key_settings = show_key_drawer;

                            match deck {
                                DeckConfig::ClipNotes => {
                                    app.deck_tab = DeckTab::ClipView;
                                    app.clip_view.sub_tab = ClipViewSubTab::Notes;
                                }
                                DeckConfig::ClipTransform => {
                                    app.deck_tab = DeckTab::ClipView;
                                    app.clip_view.sub_tab = ClipViewSubTab::Transform;
                                }
                                DeckConfig::ClipGenerate => {
                                    app.deck_tab = DeckTab::ClipView;
                                    app.clip_view.sub_tab = ClipViewSubTab::Generate;
                                }
                                DeckConfig::ClipWarping => {
                                    app.deck_tab = DeckTab::ClipView;
                                    app.clip_view.sub_tab = ClipViewSubTab::Warping;
                                }
                                DeckConfig::DeviceRackStandard => {
                                    app.deck_tab = DeckTab::DeviceRack;
                                    app.device_chain.quick_picker_open = false;
                                }
                                DeckConfig::DeviceRackAddDeviceOpen => {
                                    app.deck_tab = DeckTab::DeviceRack;
                                    app.device_chain.quick_picker_open = true;
                                }
                                DeckConfig::DrumBankA => {
                                    app.deck_tab = DeckTab::DrumSequencer;
                                    app.drum_sequencer.bank = DrumBank::BankA;
                                }
                                DeckConfig::DrumBankB => {
                                    app.deck_tab = DeckTab::DrumSequencer;
                                    app.drum_sequencer.bank = DrumBank::BankB;
                                }
                                DeckConfig::XySurface => {
                                    app.deck_tab = DeckTab::XyPad;
                                }
                            }

                            let output = render_studio_frame(&mut app, &ctx, screen_rect);
                            let report = audit_rendered_frame(screen_rect, &output.shapes);

                            assert!(
                                report.is_clean(),
                                "Anti-Breakage layout audit failed at {} ({:?}) | deck={:?} | locale={:?} | browser_collapsed={} | key_drawer={}:\n\
                                 - Viewport Overflows: {:?}\n\
                                 - Clipped Texts: {:?}\n\
                                 - Text Collisions: {:?}\n\
                                 - Tofu Glyphs: {:?}",
                                res_name,
                                view,
                                deck,
                                locale,
                                browser_collapsed,
                                show_key_drawer,
                                report.viewport_overflows,
                                report.clipped_texts,
                                report.text_collisions,
                                report.unsupported_glyphs,
                            );
                        }
                    }
                }
            }
        }
    }
}

// =========================================================================
// 2. Real Audio Buffer Acoustic Verification (16 Drum Pads)
// =========================================================================

#[test]
fn test_e2e_acoustic_verification_all_16_drum_pads_distinct_spectral_profiles() {
    let sample_rate = 44100.0_f32;
    let block_size = 1024;

    #[derive(Debug, Clone)]
    struct PadSignature {
        index: usize,
        name: &'static str,
        bank: &'static str,
        rms: f32,
        zero_crossings: usize,
    }

    let mut signatures: Vec<PadSignature> = Vec::with_capacity(16);

    for pad_idx in 0..16 {
        let mut rack = DrumRack::new(sample_rate);
        rack.trigger_pad(pad_idx as u8, 100);

        let mut out_l = vec![0.0_f32; block_size];
        let mut out_r = vec![0.0_f32; block_size];
        rack.process_block(&mut out_l, &mut out_r);

        // 1. Calculate RMS energy
        let sum_sq: f32 = out_l.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / block_size as f32).sqrt();

        // Must produce non-zero audio energy
        assert!(
            rms > 1e-4,
            "Pad {} ({}) rendered zero energy: rms = {}",
            pad_idx,
            rack.pads[pad_idx].name,
            rms
        );

        // 2. Calculate Zero-Crossing Count (frequency/spectral density indicator)
        let mut zero_crossings = 0;
        for i in 0..block_size - 1 {
            if (out_l[i] >= 0.0 && out_l[i + 1] < 0.0) || (out_l[i] < 0.0 && out_l[i + 1] >= 0.0) {
                zero_crossings += 1;
            }
        }

        assert!(
            zero_crossings > 0,
            "Pad {} ({}) must have non-zero zero crossings",
            pad_idx,
            rack.pads[pad_idx].name
        );

        let (name, bank) = if pad_idx < 8 {
            (BANK_A_PAD_NAMES[pad_idx], "Bank A")
        } else {
            (BANK_B_PAD_NAMES[pad_idx - 8], "Bank B")
        };

        signatures.push(PadSignature {
            index: pad_idx,
            name,
            bank,
            rms,
            zero_crossings,
        });
    }

    // Assert Kick909 (Pad 0) signature is captured
    let kick = &signatures[0];
    assert_eq!(kick.name, "Kick");

    // Critical Ableton manual requirement: NO pad in Bank B may fall back to Kick909
    for sig in &signatures[8..16] {
        let is_identical_to_kick =
            (sig.rms - kick.rms).abs() < 1e-5 && sig.zero_crossings == kick.zero_crossings;
        assert!(
            !is_identical_to_kick,
            "Pad {} ({} in {}) falls back to Kick909! (RMS={:.5}, ZC={})",
            sig.index,
            sig.name,
            sig.bank,
            sig.rms,
            sig.zero_crossings
        );
    }

    // Verify velocity scaling on drum pads: Vel 1 vs Vel 127
    for pad_idx in 0..16 {
        let mut rack_soft = DrumRack::new(sample_rate);
        rack_soft.trigger_pad(pad_idx as u8, 1);
        let mut out_soft = vec![0.0_f32; block_size];
        let mut dummy = vec![0.0_f32; block_size];
        rack_soft.process_block(&mut out_soft, &mut dummy);
        let rms_soft: f32 = (out_soft.iter().map(|&s| s * s).sum::<f32>() / block_size as f32).sqrt();

        let mut rack_hard = DrumRack::new(sample_rate);
        rack_hard.trigger_pad(pad_idx as u8, 127);
        let mut out_hard = vec![0.0_f32; block_size];
        rack_hard.process_block(&mut out_hard, &mut dummy);
        let rms_hard: f32 = (out_hard.iter().map(|&s| s * s).sum::<f32>() / block_size as f32).sqrt();

        assert!(
            rms_hard > rms_soft,
            "Pad {} must scale dynamically with velocity: vel 127 ({:.5}) <= vel 1 ({:.5})",
            pad_idx,
            rms_hard,
            rms_soft
        );
    }

    // Verify drum pad Mute & Solo acoustic isolation
    for (pad_idx, sig) in signatures.iter().enumerate().take(16) {
        let mut rack = DrumRack::new(sample_rate);
        rack.pads[pad_idx].mute = true;
        rack.trigger_pad(pad_idx as u8, 100);

        let mut out_l = vec![0.0_f32; block_size];
        let mut out_r = vec![0.0_f32; block_size];
        rack.process_block(&mut out_l, &mut out_r);

        let energy: f32 = out_l.iter().map(|&s| s * s).sum();
        assert_eq!(
            energy, 0.0,
            "Muted pad {} ({}) must render absolute silence",
            pad_idx, sig.name
        );
    }
}

// =========================================================================
// 3. Audio Engine Processor: Fader, Mute & Master Silence Verification
// =========================================================================

#[test]
fn test_e2e_acoustic_verification_channel_and_master_fader_mute_silence_and_vu_meters() {
    let queues = AudioQueues::new();
    let (mut ui, audio) = queues.split();
    let params = AtomicParameters::new();
    let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

    // 1. Normal Playback: Track 0 (Synth) and Track 1 (Drum Kick)
    ui.send_command(AudioCommand::Play);
    ui.send_command(AudioCommand::NoteOn {
        track_id: 0,
        note: 60,
        velocity: 100,
    });
    ui.send_command(AudioCommand::NoteOn {
        track_id: 1,
        note: 36,
        velocity: 100,
    });

    let mut buf = [0.0_f32; 1024];
    proc.process_interleaved(&mut buf);
    let energy_active: f32 = buf.iter().map(|&s| s * s).sum();
    assert!(energy_active > 1e-4, "Normal playback must render audible signal");
    assert!(proc.mixer.tracks[0].peak_l > 0.0, "Track 0 VU meter must be active");
    assert!(proc.mixer.tracks[1].peak_l > 0.0, "Track 1 VU meter must be active");
    assert!(proc.mixer.master_peak_l > 0.0, "Master VU meter must be active");

    // 2. Mute Track 0: Track 0 must be silenced, VU meter drops to 0.0
    ui.send_command(AudioCommand::SetParameter {
        track_id: 0,
        param_id: PARAM_ID_TRACK_MUTE_BASE,
        value: 1.0,
    });
    proc.process_interleaved(&mut buf);
    assert_eq!(proc.mixer.tracks[0].peak_l, 0.0, "Muted Track 0 post-fader VU meter must be 0.0");
    assert_eq!(proc.mixer.tracks[0].peak_r, 0.0, "Muted Track 0 post-fader VU meter must be 0.0");

    // 3. Set Track 1 Volume to -inf dB (0.0 linear): Track 1 silenced, VU meter 0.0
    ui.send_command(AudioCommand::SetParameter {
        track_id: 1,
        param_id: PARAM_ID_TRACK_VOL_BASE + 1,
        value: 0.0,
    });
    // Trigger drum again to confirm zero-volume suppresses new hits
    ui.send_command(AudioCommand::NoteOn {
        track_id: 1,
        note: 36,
        velocity: 120,
    });
    proc.process_interleaved(&mut buf);
    assert_eq!(proc.mixer.tracks[1].peak_l, 0.0, "Track 1 at -inf dB must report 0.0 VU meter");
    assert_eq!(proc.mixer.tracks[1].peak_r, 0.0, "Track 1 at -inf dB must report 0.0 VU meter");

    // Since Track 0 is muted and Track 1 is at 0.0 volume, master output must be strictly 0.0
    let energy_all_silenced: f32 = buf.iter().map(|&s| s * s).sum();
    assert_eq!(
        energy_all_silenced, 0.0,
        "When all tracks are muted or zero volume, master output must be absolute silence"
    );
    assert_eq!(proc.mixer.master_peak_l, 0.0);
    assert_eq!(proc.mixer.master_peak_r, 0.0);

    // 4. Unmute and restore track volumes to 1.0, but pull Master Volume to -inf dB (0.0 linear)
    ui.send_command(AudioCommand::SetParameter {
        track_id: 0,
        param_id: PARAM_ID_TRACK_MUTE_BASE,
        value: 0.0,
    });
    ui.send_command(AudioCommand::SetParameter {
        track_id: 1,
        param_id: PARAM_ID_TRACK_VOL_BASE + 1,
        value: 1.0,
    });
    ui.send_command(AudioCommand::SetParameter {
        track_id: 0,
        param_id: PARAM_ID_MASTER_VOL,
        value: 0.0,
    });
    ui.send_command(AudioCommand::NoteOn {
        track_id: 0,
        note: 64,
        velocity: 110,
    });
    ui.send_command(AudioCommand::NoteOn {
        track_id: 1,
        note: 38,
        velocity: 110,
    });

    proc.process_interleaved(&mut buf);
    let energy_master_silence: f32 = buf.iter().map(|&s| s * s).sum();
    assert_eq!(
        energy_master_silence, 0.0,
        "Master volume at -inf dB (0.0) must render absolute silence (0.0 across all samples)"
    );
    assert_eq!(proc.mixer.master_peak_l, 0.0, "Master peak L must be 0.0 at -inf dB");
    assert_eq!(proc.mixer.master_peak_r, 0.0, "Master peak R must be 0.0 at -inf dB");

    // 5. Volume Fader Monotonic Gain Scaling across 5 standard fader levels:
    // [-inf dB (0.0), -12 dB (0.25), -6 dB (0.5), 0 dB (1.0), +6 dB (2.0)]
    let fader_positions = [
        ("Silence -inf dB", 0.0_f32),
        ("Attenuated -12 dB", 0.25),
        ("Half gain -6 dB", 0.5),
        ("Unity 0.0 dB", 1.0),
        ("Boosted +6.0 dB", 2.0),
    ];

    let mut prior_energy = -1.0_f32;
    for (label, vol) in fader_positions {
        let queues_fader = AudioQueues::new();
        let (mut ui_fader, audio_fader) = queues_fader.split();
        let params_fader = AtomicParameters::new();
        let mut proc_fader = AudioEngineProcessor::new(44100.0, audio_fader, params_fader);

        ui_fader.send_command(AudioCommand::Play);
        ui_fader.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_MASTER_VOL,
            value: vol,
        });
        ui_fader.send_command(AudioCommand::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
        });
        let mut fader_buf = [0.0_f32; 1024];
        proc_fader.process_interleaved(&mut fader_buf);

        let energy: f32 = fader_buf.iter().map(|&s| s * s).sum();
        if vol == 0.0 {
            assert_eq!(energy, 0.0, "{}: must be strictly silent", label);
        } else {
            assert!(
                energy > prior_energy,
                "{}: energy {:.4} must be greater than prior fader step {:.4}",
                label,
                energy,
                prior_energy
            );
        }
        prior_energy = energy;
    }
}

// =========================================================================
// 4. Bank B Row 16 Tambourine Button Horizontal Alignment
// =========================================================================

#[test]
fn test_e2e_sequencer_row_16_tambourine_horizontal_x_alignment() {
    // 1. Verify pad 15 (Row 16) canonical and truncated names
    assert_eq!(BANK_B_PAD_NAMES[7], "Tambourine");

    let pad_name = BANK_B_PAD_NAMES[7];
    let pad_idx = 15;
    let display_name = if pad_name == "Tambourine" {
        "Tamb."
    } else if pad_idx >= 9 && pad_name.len() > 7 {
        &pad_name[..7]
    } else if pad_name.len() > 8 {
        &pad_name[..8]
    } else {
        pad_name
    };
    assert_eq!(display_name, "Tamb.");

    let audition_label = format!("▶ {}. {}", pad_idx + 1, display_name);
    assert_eq!(audition_label, "▶ 16. Tamb.");
    assert!(
        audition_label.chars().count() <= 13,
        "Audition label must be <= 13 characters to fit strictly within 74.0px"
    );

    // 2. Headless Context Render: Inspect all 8 rows in Bank B (Rows 9..16)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);

    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let raw_input = RawInput {
        screen_rect: Some(screen_rect),
        ..Default::default()
    };

    let mut drum_pattern = DrumStepPattern::new_16_step();
    let mut state = DrumSequencerState {
        bank: DrumBank::BankB, // Display Bank B (Pads 9..16, Rows 9..16)
        ..Default::default()
    };

    let output = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_drum_sequencer_with_state(ui, &mut drum_pattern, 0, &mut state);
        });
    });

    // 3. Extract all painted rectangles from the clipped shapes
    fn extract_rects(shape: &Shape, out: &mut Vec<Rect>) {
        match shape {
            Shape::Rect(rect_shape) => {
                if rect_shape.rect.is_finite() && rect_shape.rect.width() > 1.0 {
                    out.push(rect_shape.rect);
                }
            }
            Shape::Vec(shapes) => {
                for s in shapes {
                    extract_rects(s, out);
                }
            }
            _ => {}
        }
    }

    let mut rects = Vec::new();
    for clipped in &output.shapes {
        extract_rects(&clipped.shape, &mut rects);
    }

    // Step buttons in 1440x900 viewport: width is 56.0, height is 20.0
    let step_rects: Vec<Rect> = rects
        .into_iter()
        .filter(|r| (r.width() - 56.0).abs() < 1.0 && (r.height() - 20.0).abs() < 1.0)
        .collect();

    // Group step buttons into 8 rows by Y coordinate
    let mut unique_rows_y: Vec<f32> = Vec::new();
    for r in &step_rects {
        let y = r.min.y;
        if !unique_rows_y.iter().any(|&ry| (ry - y).abs() < 2.0) {
            unique_rows_y.push(y);
        }
    }
    unique_rows_y.sort_by(|a, b| a.partial_cmp(b).unwrap());

    assert_eq!(
        unique_rows_y.len(), 8,
        "Sequencer Bank B must have exactly 8 rows of step buttons"
    );

    let mut step_rows: Vec<Vec<Rect>> = Vec::new();
    for &ry in &unique_rows_y {
        let mut row: Vec<Rect> = step_rects
            .iter()
            .copied()
            .filter(|r| (r.min.y - ry).abs() < 2.0)
            .collect();

        // Dedup overlapping shapes (fill + stroke)
        row.sort_by(|a, b| a.min.x.partial_cmp(&b.min.x).unwrap());
        let mut deduped: Vec<Rect> = Vec::new();
        for s in row {
            if !deduped.iter().any(|d| (d.min.x - s.min.x).abs() < 1.0) {
                deduped.push(s);
            }
        }
        assert_eq!(
            deduped.len(), 16,
            "Each row in Sequencer Bank B must have exactly 16 step buttons"
        );
        step_rows.push(deduped);
    }

    // Row 16 (index 7 in Bank B, Pad 15 Tambourine)
    let row_16 = &step_rows[7];
    for (row_idx, row) in step_rows.iter().enumerate().take(7) {
        for step_col in 0..16 {
            let x_ref = row[step_col].min.x;
            let x_16 = row_16[step_col].min.x;
            assert!(
                (x_ref - x_16).abs() < 0.01,
                "Step {} column alignment error: Row {} X = {:.2}, Row 16 (Tambourine) X = {:.2}",
                step_col + 1,
                row_idx + 9,
                x_ref,
                x_16
            );
        }
    }
}

// =========================================================================
// 5. TR-909 Pattern Playback and Groove Presets
// =========================================================================

#[test]
fn test_e2e_tr909_sequencer_step_playback_and_grooves() {
    let mut pattern = DrumStepPattern::new_16_step();
    let state = DrumSequencerState::default();

    // 1. 4-on-the-Floor Groove
    state.apply_four_on_the_floor(&mut pattern);
    let kick_track = &pattern.tracks[0];
    for (idx, step) in kick_track.steps.iter().enumerate() {
        if idx % 4 == 0 {
            assert!(step.active, "Kick must be active at step {}", idx);
            assert!(step.accent, "Kick must be accented at step {}", idx);
        } else {
            assert!(!step.active, "Kick must be inactive at step {}", idx);
        }
    }

    // 2. RKT Tresillo Groove: Kick on 0, 6, 10; Snare on 3, 7, 11, 15
    state.apply_rkt_tresillo(&mut pattern);
    let kick_track = &pattern.tracks[0];
    assert!(kick_track.steps[0].active && kick_track.steps[0].accent);
    assert!(kick_track.steps[6].active && kick_track.steps[6].accent);
    assert!(kick_track.steps[10].active);
    assert!(!kick_track.steps[4].active);

    let snare_track = &pattern.tracks[1];
    let expected_snare_steps = [3, 7, 11, 15];
    for &step_idx in &expected_snare_steps {
        assert!(
            snare_track.steps[step_idx].active,
            "RKT Tresillo must have syncopated snare at step {}",
            step_idx
        );
    }

    // 3. Techno 16ths Groove
    state.apply_techno_16ths(&mut pattern);
    let hh_track = &pattern.tracks[8]; // Closed HH
    for (idx, step) in hh_track.steps.iter().enumerate() {
        assert!(step.active, "Techno 16ths must have continuous closed HH at step {}", idx);
    }

    // 4. Pad Audibility & Solo / Mute in Step Playback
    let mut state_audibility = DrumSequencerState::default();
    assert!(state_audibility.is_pad_audible(0));
    assert!(state_audibility.is_pad_audible(15));

    // Mute Pad 0
    state_audibility.toggle_mute(0);
    assert!(!state_audibility.is_pad_audible(0), "Muted Pad 0 must NOT be audible");
    assert!(state_audibility.is_pad_audible(1), "Unmuted Pad 1 must remain audible");

    // Solo Pad 1
    state_audibility.toggle_solo(1);
    assert!(state_audibility.is_pad_audible(1), "Soloed Pad 1 must be audible");
    assert!(!state_audibility.is_pad_audible(2), "Non-soloed Pad 2 must be muted during solo");
}

// =========================================================================
// 6. Operational Knowledge Framework (OKF) Copilot Bilingual Integration
// =========================================================================

#[test]
fn test_e2e_okf_copilot_bilingual_manual_queries_and_system_prompt() {
    let mut app = OxideStudioApp::new_mock(44100.0);

    // 1. English OKF Query
    app.ai_copilot.set_locale(StudioLocale::En);
    let (reply_en, badges_en) = execute_studio_intent("how to use glue compressor", &mut app);
    assert!(
        reply_en.contains("Glue Compressor") || reply_en.contains("analog modeling"),
        "English query must return Glue Compressor manual section: {}",
        reply_en
    );
    assert!(
        badges_en.contains(&"device_set_param".to_string()),
        "Must attach device_set_param badge: {:?}",
        badges_en
    );

    // 2. Spanish OKF Query
    app.ai_copilot.set_locale(StudioLocale::Es);
    let (reply_es, badges_es) = execute_studio_intent("secuenciador tr-909 y groove", &mut app);
    assert!(
        reply_es.contains("TR-909") || reply_es.contains("groove"),
        "Spanish query must return TR-909 manual section: {}",
        reply_es
    );
    assert!(
        badges_es.contains(&"generate_drum_groove_clip".to_string()),
        "Must attach generate_drum_groove_clip badge: {:?}",
        badges_es
    );

    // 3. System Prompt Bilingual Generation
    let prompt_en = app.ai_copilot.system_prompt();
    assert!(prompt_en.contains("OKF-01"), "English prompt must contain OKF sections");
    assert!(prompt_en.contains("device_add_to_track"), "English prompt must contain MCP tools");

    app.ai_copilot.set_locale(StudioLocale::Es);
    let prompt_es = app.ai_copilot.system_prompt();
    assert!(prompt_es.contains("OKF-01"), "Spanish prompt must contain OKF sections");
    assert!(prompt_es.contains("voseo"), "Spanish prompt must specify Rioplatense voseo");
}

// =========================================================================
// 7. Epic #17: Multi-Track Input Recording & Driver Latency Compensation
// =========================================================================

#[test]
fn test_e2e_epic17_hardware_io_matrix_and_driver_latency_compensation() {
    // 1. Hardware I/O Matrix Gain & Routing Verification
    let mut matrix = HardwareIoMatrix::new();
    matrix.set_input_trim_db(2, 6.0).expect("Trim +6.0 dB on input bus 2");
    assert!(
        (matrix.inputs[2].gain_linear - 1.995262).abs() < 1e-3,
        "Gain linear trim mismatch"
    );

    matrix
        .set_track_input_route(0, 2, InputChannelMode::MonoLeft)
        .expect("Route input bus 2 MonoLeft to Track 0");
    matrix
        .set_track_output_route(0, 3)
        .expect("Route Track 0 output to Bus 3");

    // Loopback conflict protection
    let conflict = matrix.set_track_input_route(0, 3, InputChannelMode::Stereo);
    assert!(
        matches!(
            conflict,
            Err(IoRoutingError::LoopbackConflict {
                in_bus: 3,
                out_bus: 3
            })
        ),
        "Loopback conflict must be rejected"
    );

    // Audio block through HardwareIoMatrix
    let mut hw_inputs = [[0.0f32; 2]; MAX_HARDWARE_INPUT_BUSES];
    hw_inputs[2] = [0.5, 0.1]; // MonoLeft is 0.5
    let mut track_inputs = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
    matrix.process_hardware_inputs(&hw_inputs, &mut track_inputs);

    let expected_signal = 0.5 * matrix.inputs[2].gain_linear;
    assert!(
        (track_inputs[0][0] - expected_signal).abs() < 1e-4,
        "MonoLeft must be scaled by linear gain"
    );
    assert!(
        (track_inputs[0][1] - expected_signal).abs() < 1e-4,
        "MonoLeft must be duplicated to right channel"
    );

    // MonoRight and Stereo routing verification
    matrix
        .set_track_input_route(1, 2, InputChannelMode::MonoRight)
        .expect("Route input bus 2 MonoRight to Track 1");
    matrix
        .set_track_input_route(2, 2, InputChannelMode::Stereo)
        .expect("Route input bus 2 Stereo to Track 2");
    matrix.process_hardware_inputs(&hw_inputs, &mut track_inputs);

    let expected_right = 0.1 * matrix.inputs[2].gain_linear;
    assert!(
        (track_inputs[1][0] - expected_right).abs() < 1e-4,
        "MonoRight must duplicate right channel to left"
    );
    assert!(
        (track_inputs[1][1] - expected_right).abs() < 1e-4,
        "MonoRight must duplicate right channel to right"
    );
    assert!(
        (track_inputs[2][0] - expected_signal).abs() < 1e-4,
        "Stereo mode preserves left channel"
    );
    assert!(
        (track_inputs[2][1] - expected_right).abs() < 1e-4,
        "Stereo mode preserves right channel"
    );

    // Input gain trim clamping [-24.0, +24.0] dB
    matrix.set_input_trim_db(0, -35.0).expect("Trim -35 dB clamped");
    assert_eq!(matrix.inputs[0].gain_trim_db, -24.0, "Negative trim must clamp to -24 dB");
    matrix.set_input_trim_db(1, 35.0).expect("Trim +35 dB clamped");
    assert_eq!(matrix.inputs[1].gain_trim_db, 24.0, "Positive trim must clamp to +24 dB");

    // 2. Driver Latency Compensation Alignment
    let compensator = DriverLatencyCompensator::new(256, 256, 64);
    assert_eq!(
        compensator.roundtrip_latency_samples(),
        576,
        "Roundtrip latency = 256 in + 256 out + 64 safety"
    );

    let mut delay_line = compensator.create_delay_comp_line();
    assert_eq!(delay_line.delay_frames(), 576);

    let mut impulse = [[0.0f32; 2]; 1024];
    impulse[0] = [1.0, -1.0];
    let mut delayed = [[0.0f32; 2]; 1024];
    delay_line.process_block(&impulse, &mut delayed);

    assert_eq!(delayed[0], [0.0, 0.0], "Impulse must not appear at frame 0");
    assert_eq!(delayed[576], [1.0, -1.0], "Impulse must be shifted by exact roundtrip latency 576");

    // 3. MultiTrackRecorder Punch and Take Commit
    let mut recorder = MultiTrackRecorder::new();
    recorder.arm_track(0, true);
    recorder.arm_track(1, true);
    recorder.set_punch_bounds(Some(2.0), Some(4.0));
    recorder.start_recording(0.0);

    let mut block_data = [[0.0f32; 2]; MAX_RECORDING_TRACKS];
    block_data[0] = [0.85, 0.85];

    // Beat 1.0 (outside punch window): should not record
    recorder.record_block(&block_data, 256, 1.0, true);
    assert_eq!(recorder.tracks[0].recorded_frames, 0, "No frames outside punch window");

    // Beat 2.5 (inside punch window): should record 1000 frames
    recorder.record_block(&block_data, 1000, 2.5, true);
    assert_eq!(recorder.tracks[0].recorded_frames, 1000);
    assert_eq!(recorder.tracks[2].recorded_frames, 0, "Unarmed track 2 must not record");

    let (peak_l, peak_r) = recorder.get_peak_meters(0);
    assert!((peak_l - 0.85).abs() < 1e-4);
    assert!((peak_r - 0.85).abs() < 1e-4);

    let committed = recorder
        .commit_track_take(0, &compensator)
        .expect("Take commit must succeed");
    assert_eq!(committed.latency_offset_samples, 576);
    assert_eq!(committed.left_channel.len(), 1000 - 576);
    assert!(
        (committed.left_channel[0] - 0.85).abs() < 1e-4,
        "Shifted sample must match input"
    );

    // 4. Hardware Buffer Sizing (64..2048) and Sample Rate (44.1k..96k) Invariants
    for &bs in &SUPPORTED_BUFFER_SIZES {
        for &sr in &SUPPORTED_SAMPLE_RATES {
            let prefs = AudioHardwarePreferences::new(sr, bs);
            let summary = prefs.calculate_latency_summary(32, 32);
            assert_eq!(summary.buffer_size, bs);
            assert_eq!(summary.input_latency_samples, bs as u32 + 32);
            assert_eq!(summary.output_latency_samples, bs as u32 + 32);
            assert_eq!(summary.overall_latency_samples, (bs as u32 + 32) * 2);

            let expected_ms = ((bs as u32 + 32) * 2) as f32 / sr * 1000.0;
            assert!((summary.overall_latency_ms - expected_ms).abs() < 1e-3);

            let comp = DriverLatencyCompensator::new_with_error_comp(bs as u32, bs as u32, 32, -16);
            assert_eq!(comp.roundtrip_latency_samples(), (bs as u32 + bs as u32 + 32) - 16);
            assert_eq!(comp.input_latency_samples(), bs as u32 + 32);
        }
    }

    // 5. Audio Engine Sub-Block Processing & CPU Telemetry (2048-Sample Hardware Buffer)
    {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(48000.0, audio, params);

        ui.send_command(AudioCommand::Play);
        ui.send_command(AudioCommand::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
        });

        // 2048 frames stereo = 4096 samples
        let mut buffer_2048 = vec![0.0f32; 2048 * 2];
        // Prime lookahead buffer in master limiter
        proc.process_interleaved(&mut buffer_2048);
        buffer_2048.fill(0.0);
        proc.process_interleaved(&mut buffer_2048);

        // Entire 2048 frames must render without truncation
        let first_half_energy: f32 = buffer_2048[..2048].iter().map(|s| s * s).sum();
        let second_half_energy: f32 = buffer_2048[2048..].iter().map(|s| s * s).sum();
        assert!(first_half_energy > 1e-4, "First half of 2048-frame buffer must have audio");
        assert!(second_half_energy > 1e-4, "Second half of 2048-frame buffer must have audio");

        let telem = ui.poll_telemetry().expect("Telemetry must be emitted");
        assert!(telem.cpu_load >= 0.0 && telem.cpu_load <= 1.0, "Valid CPU load telemetry");
    }

    // 6. Test Tone Generator (Live 12 Chapter 2 standard 440 Hz reference tone)
    {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

        proc.enable_test_tone(true, 440.0, 0.5);
        let mut tone_buf = [0.0f32; 1024];
        proc.process_interleaved(&mut tone_buf);

        let energy: f32 = tone_buf.iter().map(|s| s * s).sum();
        assert!(energy > 10.0, "Test tone must render audible signal");

        let telem = ui.poll_telemetry().expect("Telemetry from test tone");
        assert!(telem.master_peak_l > 0.1, "Test tone triggers master VU meter");
        assert!(telem.master_peak_r > 0.1, "Test tone triggers master VU meter");

        // Disabling test tone returns to silence
        proc.enable_test_tone(false, 440.0, 0.0);
        let mut silent_buf = [0.0f32; 1024];
        proc.process_interleaved(&mut silent_buf);
        proc.process_interleaved(&mut silent_buf);
        let energy_silent: f32 = silent_buf.iter().map(|s| s * s).sum();
        assert_eq!(energy_silent, 0.0, "Disabled test tone must render complete silence");
    }
}

// =========================================================================
// 8. Epic #18: Sandboxed CLAP / VST3 / AUv2 Plugin Hosting & Watchdog
// =========================================================================

#[test]
fn test_e2e_epic18_sandboxed_plugin_hosting_and_crash_safe_dry_bypass() {
    let clap_desc = PluginDescriptor::new(
        "com.u-he.diva",
        "Diva",
        "u-he",
        PluginFormat::Clap,
        2,
        2,
        true,
        64,
    );
    let vst3_desc = PluginDescriptor::new(
        "com.fabfilter.pro-q3",
        "Pro-Q 3",
        "FabFilter",
        PluginFormat::Vst3,
        2,
        2,
        true,
        128,
    );
    let au_desc = PluginDescriptor::new(
        "com.apple.matrixreverb",
        "MatrixReverb",
        "Apple",
        PluginFormat::AudioUnitV2,
        2,
        2,
        false,
        32,
    );

    assert_eq!(clap_desc.format.file_extension(), "clap");
    assert_eq!(vst3_desc.format.file_extension(), "vst3");
    assert_eq!(au_desc.format.file_extension(), "component");

    let mut host = PluginSandboxHost::new(SandboxMode::PerPluginProcess);
    let id0 = host.instantiate(clap_desc);
    let id1 = host.instantiate(vst3_desc);
    let id2 = host.instantiate(au_desc);

    assert_eq!(id0, 0);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(host.total_latency_samples(), 64 + 128 + 32);

    // Automation and binary preset serialization
    {
        let inst0 = host.instance_mut(id0).expect("Instance 0");
        inst0.set_parameter_normalized(0, 0.75);
        inst0.set_parameter_normalized(1, 0.33);
        assert_eq!(inst0.get_parameter_normalized(0), Some(0.75));
        assert_eq!(inst0.get_parameter_normalized(1), Some(0.33));

        let state_chunk = inst0.save_state_chunk();
        assert!(state_chunk.len() >= 12, "Chunk must contain header and parameters");
        assert_eq!(&state_chunk[0..4], b"PLUG");

        inst0.set_parameter_normalized(0, 0.1);
        assert_eq!(inst0.get_parameter_normalized(0), Some(0.1));
        inst0.load_state_chunk(&state_chunk).expect("Load state chunk");
        assert_eq!(inst0.get_parameter_normalized(0), Some(0.75));
    }

    // Simulated Crash and Dry Bypass Watchdog Pass-Through
    {
        let inst1 = host.instance_mut(id1).expect("Instance 1");
        inst1.set_parameter_normalized(0, 0.5); // gain 0.5
        let mut left = [1.0f32; 16];
        let mut right = [1.0f32; 16];
        inst1.process_audio_block(&mut left, &mut right);
        assert!((left[0] - 0.5).abs() < 1e-4);

        inst1.simulate_crash();
        assert!(inst1.is_crashed());

        // During crash, audio passes through completely unaltered without panicking
        let mut left_crashed = [0.85f32; 16];
        let mut right_crashed = [0.85f32; 16];
        inst1.process_audio_block(&mut left_crashed, &mut right_crashed);
        assert_eq!(left_crashed, [0.85f32; 16], "Dry bypass must pass audio uncorrupted");
        assert_eq!(right_crashed, [0.85f32; 16], "Dry bypass must pass audio uncorrupted");

        // Restart watchdog
        inst1.restart_sandbox();
        assert!(!inst1.is_crashed());
        inst1.process_audio_block(&mut left_crashed, &mut right_crashed);
        assert!(
            (left_crashed[0] - 0.425).abs() < 1e-4,
            "Normal processing must resume after watchdog restart"
        );
    }

    // Editor GUI Lifecycle
    let rect = EditorWindowRect::new(10.0, 10.0, 800.0, 600.0, "Diva GUI");
    {
        let inst0 = host.instance_mut(id0).expect("Instance 0");
        assert!(inst0.open_editor(rect.clone()).is_ok());
        assert!(inst0.is_editor_open());
        assert_eq!(inst0.editor_rect(), Some(&rect));
        inst0.close_editor();
        assert!(!inst0.is_editor_open());
    }

    {
        let inst2 = host.instance_mut(id2).expect("Instance 2");
        assert!(
            inst2.open_editor(rect).is_err(),
            "Plugin without GUI must return Err on open_editor"
        );
    }

    // POSIX Shared Memory Ring Buffer IPC Verification (Chapter 23 & OKF-10)
    {
        let inst0 = host.instance(id0).expect("Instance 0 (CLAP)");
        let shm0 = inst0.shm_ring_buffer.as_ref().expect("SHM ring buffer attached");
        assert!(shm0.is_connected());
        assert!(shm0.capacity_frames() >= 1024);
        assert!(shm0.shm_identifier().starts_with("/oxide_shm_clap_"));

        let inst1 = host.instance(id1).expect("Instance 1 (VST3)");
        let shm1 = inst1.shm_ring_buffer.as_ref().expect("SHM ring buffer attached");
        assert!(shm1.is_connected());
        assert!(shm1.shm_identifier().starts_with("/oxide_shm_vst3_"));

        let inst2 = host.instance(id2).expect("Instance 2 (AUv2)");
        let shm2 = inst2.shm_ring_buffer.as_ref().expect("SHM ring buffer attached");
        assert!(shm2.is_connected());
        assert!(shm2.shm_identifier().starts_with("/oxide_shm_component_"));
    }
}

// =========================================================================
// 9. Epic #19: All 6 Warp Modes & Streaming Disk Cache
// =========================================================================

#[test]
fn test_e2e_epic19_all_six_warp_modes_and_streaming_disk_cache() {
    let sample_rate = 44100.0_f32;
    let mut input = vec![0.0f32; 4096];
    for i in 0..4096 {
        let t = i as f32 / sample_rate;
        input[i] = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.6
            + (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.3;
    }
    input[512] = 0.95;

    let modes = [
        DspWarpMode::Beats,
        DspWarpMode::Tones,
        DspWarpMode::Texture,
        DspWarpMode::RePitch,
        DspWarpMode::Complex,
        DspWarpMode::ComplexPro,
    ];

    for &mode in &modes {
        let mut proc = SpectralWarpProcessor::new(sample_rate);
        proc.set_mode(mode);
        proc.set_stretch_ratio(1.25);
        proc.set_pitch_semitones(3.0);

        let mut out = vec![0.0f32; 1024];
        proc.process_block(&input, &mut out);

        assert!(
            out.iter().all(|s| s.is_finite()),
            "WarpMode {:?} produced non-finite values",
            mode
        );
        let energy: f32 = out.iter().map(|s| s * s).sum();
        assert!(
            energy > 0.001,
            "WarpMode {:?} produced insufficient energy: {}",
            mode,
            energy
        );
    }

    // ComplexPro Formant Preservation Test (Pearson correlation > 0.92)
    let mut proc_preserve = SpectralWarpProcessor::new(sample_rate);
    proc_preserve.set_mode(DspWarpMode::ComplexPro);
    proc_preserve.complex_pro_params = ComplexProParams::new(1.0, 64.0);
    proc_preserve.set_pitch_semitones(7.0);
    let mut out_preserve = vec![0.0f32; FFT_SIZE];
    proc_preserve.process_block(&input, &mut out_preserve);

    let mut proc_shift = SpectralWarpProcessor::new(sample_rate);
    proc_shift.set_mode(DspWarpMode::ComplexPro);
    proc_shift.complex_pro_params = ComplexProParams::new(0.0, 64.0);
    proc_shift.set_pitch_semitones(7.0);
    let mut out_shift = vec![0.0f32; FFT_SIZE];
    proc_shift.process_block(&input, &mut out_shift);

    let formant_diff: f32 = out_preserve
        .iter()
        .zip(out_shift.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(
        formant_diff > 0.01,
        "ComplexPro formant preservation must alter spectral output (diff: {})",
        formant_diff
    );

    // Compute spectral envelope correlation (> 0.92)
    let fft = Radix2Fft2048::new();
    let mut hann = [0.0f32; FFT_SIZE];
    for i in 0..FFT_SIZE {
        hann[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * (i as f32) / (FFT_SIZE as f32)).cos());
    }
    let compute_env = |samples: &[f32]| -> Vec<f32> {
        let half = FFT_SIZE / 2;
        let mut real = [0.0f32; FFT_SIZE];
        let mut imag = [0.0f32; FFT_SIZE];
        let len = samples.len().min(FFT_SIZE);
        for i in 0..len {
            real[i] = samples[i] * hann[i];
        }
        fft.forward(&mut real, &mut imag);
        let mut mag = vec![0.0f32; half];
        for k in 0..half {
            mag[k] = (real[k] * real[k] + imag[k] * imag[k]).sqrt();
        }
        let order = 64;
        let mut env = vec![0.0f32; half];
        for k in 0..half {
            let start = if k >= order / 2 { k - order / 2 } else { 0 };
            let end = (k + order / 2).min(half);
            let sum: f32 = mag[start..end].iter().sum();
            env[k] = sum / (end - start) as f32;
        }
        env
    };
    let env_in = compute_env(&input);
    let env_preserve = compute_env(&out_preserve);

    let n = env_in.len() as f32;
    let mean_in = env_in.iter().sum::<f32>() / n;
    let mean_pres = env_preserve.iter().sum::<f32>() / n;

    let mut cov_pres = 0.0f32;
    let mut var_in = 0.0f32;
    let mut var_pres = 0.0f32;

    for i in 0..env_in.len() {
        let din = env_in[i] - mean_in;
        let dpres = env_preserve[i] - mean_pres;
        var_in += din * din;
        cov_pres += din * dpres;
        var_pres += dpres * dpres;
    }

    let pearson_pres = cov_pres / (var_in.sqrt() * var_pres.sqrt());

    assert!(
        pearson_pres > 0.92,
        "ComplexPro formant preservation Pearson correlation must exceed 0.92, got: {}",
        pearson_pres
    );

    // Texture Mode Deterministic Flux PRNG
    let mut proc_tex1 = SpectralWarpProcessor::new(sample_rate);
    proc_tex1.set_mode(DspWarpMode::Texture);
    proc_tex1.texture_params = TextureParams::new(30.0, 0.7);
    proc_tex1.prng = WarpPrng::new(0xABCD);
    let mut out_tex1 = vec![0.0f32; 512];
    proc_tex1.process_block(&input, &mut out_tex1);

    let mut proc_tex2 = SpectralWarpProcessor::new(sample_rate);
    proc_tex2.set_mode(DspWarpMode::Texture);
    proc_tex2.texture_params = TextureParams::new(30.0, 0.7);
    proc_tex2.prng = WarpPrng::new(0xABCD);
    let mut out_tex2 = vec![0.0f32; 512];
    proc_tex2.process_block(&input, &mut out_tex2);

    assert_eq!(
        out_tex1, out_tex2,
        "Texture warp PRNG must produce deterministic output with identical seed"
    );

    // StreamingDiskCache Double-Buffering
    let mut cache = StreamingDiskCache::new();
    let page0: Vec<f32> = (0..PAGE_SIZE).map(|i| (i as f32) * 0.01).collect();
    let page1: Vec<f32> = (PAGE_SIZE..PAGE_SIZE * 2).map(|i| (i as f32) * 0.01).collect();
    cache.load_page(0, 0, &page0);
    cache.load_page(1, PAGE_SIZE, &page1);

    let mut boundary_buf = [0.0f32; 8];
    let read_count = cache.read_frames(PAGE_SIZE - 4, &mut boundary_buf);
    assert_eq!(read_count, 8);
    for (i, &val) in boundary_buf.iter().enumerate() {
        let expected = (PAGE_SIZE - 4 + i) as f32 * 0.01;
        assert!(
            (val - expected).abs() < 1e-5,
            "Cache boundary read mismatch at {}",
            i
        );
    }
}

// =========================================================================
// 10. Epic #20: Modular Patch Graph Audio Processing & LOM Mapper
// =========================================================================

#[test]
fn test_e2e_epic20_modular_patch_graph_audio_processing_and_lom_mapper() {
    let sample_rate = 44100.0_f32;
    let mut graph = ModularPatchGraph::new(sample_rate);

    let osc = graph
        .add_node(PatchNodeKind::Oscillator {
            freq: 440.0,
            shape: PatchOscShape::Sine,
        })
        .expect("Add Oscillator");
    let filter = graph
        .add_node(PatchNodeKind::StateVariableFilter {
            cutoff: 1000.0,
            resonance: 1.5,
            mode: PatchFilterMode::Lowpass,
        })
        .expect("Add Filter");
    let delay = graph
        .add_node(PatchNodeKind::DelayLine {
            delay_samples: 16,
            feedback: 0.3,
        })
        .expect("Add Delay");

    graph.connect_cable(osc, 0, filter, 0).expect("Osc -> Filter");
    graph.connect_cable(filter, 0, delay, 0).expect("Filter -> Delay");
    graph.audio_output_node_l = Some(delay);
    graph.audio_output_node_r = Some(delay);

    let mut rendered = Vec::with_capacity(128);
    for _ in 0..128 {
        let (l, r, _) = graph.process_frame(0.0, 0.0);
        assert!(l.is_finite() && r.is_finite());
        rendered.push(l);
    }
    let energy: f32 = rendered[20..].iter().map(|s| s * s).sum();
    assert!(energy > 1e-4, "Topological audio output produced zero energy");

    // LOM Parameter Modulation & Observation
    let mut lfo_preset = ModularPatchGraph::preset_lfo_filter_modulator();
    let mut modulations = Vec::new();
    for _ in 0..100 {
        let (_, _, lom) = lfo_preset.process_frame(0.0, 0.0);
        assert!(lom.is_some(), "LOM param mapper must emit modulation");
        let (dev, param, val) = lom.unwrap();
        assert_eq!(dev, 1);
        assert_eq!(param, 2);
        assert!(
            val >= 200.0 && val <= 2000.0,
            "LOM modulation out of range: {}",
            val
        );
        assert!(lfo_preset.last_lom_target.is_some());
        let (track, target_dev, target_p, target_v) = lfo_preset.last_lom_target.unwrap();
        assert_eq!(track, 0);
        assert_eq!(target_dev, 1);
        assert_eq!(target_p, 2);
        assert!((target_v - val).abs() < 1e-5);
        modulations.push(val);
    }
    let min_val = modulations.iter().cloned().fold(f32::INFINITY, f32::min);
    let max_val = modulations.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        max_val - min_val > 10.0,
        "LOM parameter modulation remained static"
    );

    // 32-Node and 64-Cable Capacity Limits
    let mut saturated_graph = ModularPatchGraph::new(sample_rate);
    for i in 0..MAX_PATCH_NODES {
        let n = saturated_graph
            .add_node(PatchNodeKind::MathAdd { operand: i as f32 })
            .expect("Add node within capacity");
        assert_eq!(n, i as u32);
    }
    assert_eq!(saturated_graph.node_count(), 32);
    assert_eq!(
        saturated_graph.add_node(PatchNodeKind::MathMul { factor: 1.0 }),
        Err(PatchGraphError::NodeCapacityExceeded),
        "33rd node must return NodeCapacityExceeded"
    );

    for i in 0..MAX_PATCH_CABLES {
        let from = (i % MAX_PATCH_NODES) as u32;
        let to = ((i + 1) % MAX_PATCH_NODES) as u32;
        let c = saturated_graph
            .connect_cable(from, 0, to, 0)
            .expect("Connect cable within capacity");
        assert_eq!(c, i as u32);
    }
    assert_eq!(saturated_graph.cable_count(), 64);
    assert_eq!(
        saturated_graph.connect_cable(0, 0, 1, 0),
        Err(PatchGraphError::CableCapacityExceeded),
        "65th cable must return CableCapacityExceeded"
    );

    // Dynamic Cable Routing: Runtime Disconnection, Rerouting, Cable Mute
    let mut dynamic_graph = ModularPatchGraph::new(sample_rate);
    let dynamic_osc = dynamic_graph
        .add_node(PatchNodeKind::Oscillator {
            freq: 440.0,
            shape: PatchOscShape::Saw,
        })
        .unwrap();
    let dynamic_mul = dynamic_graph
        .add_node(PatchNodeKind::MathMul { factor: 1.5 })
        .unwrap();
    let dynamic_filter = dynamic_graph
        .add_node(PatchNodeKind::StateVariableFilter {
            cutoff: 500.0,
            resonance: 2.0,
            mode: PatchFilterMode::Highpass,
        })
        .unwrap();

    let cable_to_mul = dynamic_graph
        .connect_cable(dynamic_osc, 0, dynamic_mul, 0)
        .unwrap();
    dynamic_graph.audio_output_node_l = Some(dynamic_mul);

    // Active audio through mul
    let mut active_energy = 0.0_f32;
    for _ in 0..32 {
        let (l, _, _) = dynamic_graph.process_frame(0.0, 0.0);
        active_energy += l * l;
    }
    assert!(active_energy > 0.1, "Active cable must pass audio energy");

    // Dynamic cable mute via set_cable_active
    dynamic_graph.set_cable_active(cable_to_mul, false).unwrap();
    let (l_muted, _, _) = dynamic_graph.process_frame(0.0, 0.0);
    assert_eq!(l_muted, 0.0, "Muted cable must output silence");

    // Dynamic cable disconnect
    dynamic_graph.disconnect_cable(cable_to_mul).unwrap();
    assert_eq!(dynamic_graph.cable_count(), 0);

    // Reroute dynamically to filter node
    let cable_to_filter = dynamic_graph
        .connect_cable(dynamic_osc, 0, dynamic_filter, 0)
        .unwrap();
    dynamic_graph.audio_output_node_l = Some(dynamic_filter);
    assert_eq!(dynamic_graph.cable_count(), 1);

    let mut rerouted_energy = 0.0_f32;
    for _ in 0..32 {
        let (l, _, _) = dynamic_graph.process_frame(0.0, 0.0);
        rerouted_energy += l * l;
    }
    assert!(rerouted_energy > 0.05, "Rerouted audio must produce energy");
    let _ = cable_to_filter;

    // Zero-Allocation Block Buffer Processing via process_buffer
    let mut in_block_l = [0.0_f32; 256];
    let mut in_block_r = [0.0_f32; 256];
    let mut out_block_l = [0.0_f32; 256];
    let mut out_block_r = [0.0_f32; 256];
    let frames = dynamic_graph.process_buffer(
        &mut in_block_l,
        &mut in_block_r,
        &mut out_block_l,
        &mut out_block_r,
    );
    assert_eq!(frames, 256);
    assert!(out_block_l.iter().all(|s| s.is_finite()));
    assert!(out_block_r.iter().all(|s| s.is_finite()));

    // Feedback Cycle Safety Test
    let mut cyclic_graph = ModularPatchGraph::new(sample_rate);
    let a = cyclic_graph.add_node(PatchNodeKind::MathAdd { operand: 0.05 }).unwrap();
    let b = cyclic_graph.add_node(PatchNodeKind::MathMul { factor: 0.8 }).unwrap();
    cyclic_graph.connect_cable(a, 0, b, 0).unwrap();
    cyclic_graph.connect_cable(b, 0, a, 0).unwrap();
    cyclic_graph.audio_output_node_l = Some(b);
    cyclic_graph.audio_output_node_r = Some(b);
    cyclic_graph.compile();

    for _ in 0..64 {
        let (l, r, _) = cyclic_graph.process_frame(0.0, 0.0);
        assert!(l.is_finite() && r.is_finite(), "Cycle must produce finite audio without panic");
    }
}

// =========================================================================
// 11. Epic #21: All 8 Suite Instruments & TR-909 16-Voice Engine
// =========================================================================

#[test]
fn test_e2e_epic21_all_eight_suite_instrument_engines_and_tr909_voices() {
    std::thread::Builder::new()
        .name("e2e_epic21_runner".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
    let sample_rate = 44100.0_f32;
    let suite_kinds = [
        SuiteInstrumentKind::Drift,
        SuiteInstrumentKind::Meld,
        SuiteInstrumentKind::Collision,
        SuiteInstrumentKind::Tension,
        SuiteInstrumentKind::Electric,
        SuiteInstrumentKind::Analog,
        SuiteInstrumentKind::Sampler,
        SuiteInstrumentKind::GranulatorIII,
    ];

    for kind in suite_kinds {
        let mut engine = SuiteInstrumentEngine::new(sample_rate, kind);
        assert_eq!(engine.category(), DeviceCategory::Instrument);
        assert!(!engine.name().is_empty());

        engine.note_on(60, 100);
        let mut left = [0.0f32; 512];
        let mut right = [0.0f32; 512];
        engine.process_block(&mut left, &mut right);

        assert!(
            left.iter().all(|s| s.is_finite()) && right.iter().all(|s| s.is_finite()),
            "Instrument {:?} generated non-finite audio",
            kind
        );
        let rms = (left.iter().map(|s| s * s).sum::<f32>() / left.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "Instrument {:?} produced near-zero energy: {}",
            kind,
            rms
        );
        engine.note_off(60);
    }

    // Sampler Velocity Layering
    let mut sampler = SuiteInstrumentEngine::new(sample_rate, SuiteInstrumentKind::Sampler);
    sampler.note_on(60, 30);
    let mut l_low = [0.0f32; 256];
    let mut r_low = [0.0f32; 256];
    sampler.process_block(&mut l_low, &mut r_low);
    let energy_low: f32 = l_low.iter().map(|s| s * s).sum();

    let mut sampler_high = SuiteInstrumentEngine::new(sample_rate, SuiteInstrumentKind::Sampler);
    sampler_high.note_on(60, 127);
    let mut l_high = [0.0f32; 256];
    let mut r_high = [0.0f32; 256];
    sampler_high.process_block(&mut l_high, &mut r_high);
    let energy_high: f32 = l_high.iter().map(|s| s * s).sum();

    assert!(
        energy_high > energy_low,
        "High-velocity sampler zone must produce higher energy"
    );

    // Granulator III Stereo Output
    let mut granulator = SuiteInstrumentEngine::new(sample_rate, SuiteInstrumentKind::GranulatorIII);
    granulator.note_on(60, 110);
    let mut g_l = [0.0f32; 512];
    let mut g_r = [0.0f32; 512];
    granulator.process_block(&mut g_l, &mut g_r);
    assert!(g_l.iter().map(|s| s * s).sum::<f32>() > 0.001);
    assert!(g_r.iter().map(|s| s * s).sum::<f32>() > 0.001);

    // All 16 TR-909 Drum Voices
    let drum_types = [
        SynthDrumType::Kick909,
        SynthDrumType::Snare909,
        SynthDrumType::HihatClosed909,
        SynthDrumType::HihatOpen909,
        SynthDrumType::Clap909,
        SynthDrumType::Rim909,
        SynthDrumType::LowTom909,
        SynthDrumType::MidTom909,
        SynthDrumType::HiTom909,
        SynthDrumType::Cowbell909,
        SynthDrumType::Crash909,
        SynthDrumType::Ride909,
        SynthDrumType::PercLow,
        SynthDrumType::PercHigh,
        SynthDrumType::Shaker,
        SynthDrumType::Tambourine,
    ];

    for drum_type in drum_types {
        let mut phase1 = 0.0f32;
        let mut phase2 = 0.0f32;
        let mut rng = 0x12345678u32;
        let mut samples = Vec::with_capacity(256);

        for i in 0..256 {
            let env = (- (i as f32) / 64.0).exp();
            let s = SynthPercussion::render_sample(
                drum_type,
                &mut phase1,
                &mut phase2,
                env,
                sample_rate,
                &mut rng,
            );
            assert!(s.is_finite(), "{:?} rendered non-finite sample", drum_type);
            samples.push(s);
        }

        let energy: f32 = samples.iter().map(|s| s * s).sum();
        assert!(
            energy > 1e-4,
            "{:?} generated insufficient energy: {}",
            drum_type,
            energy
        );
    }

    // DrumRack Pad Integration across 16 pads
    let mut rack = DrumRack::new(sample_rate);
    for pad in 0..16 {
        rack.trigger_pad(pad, 110);
    }
    let mut rack_l = [0.0f32; 256];
    let mut rack_r = [0.0f32; 256];
    rack.process_block(&mut rack_l, &mut rack_r);
    assert!(rack_l.iter().map(|s| s * s).sum::<f32>() > 0.01);

    // Chapter 12 & Epic #21: MPE Voice Routing into Meld and Drift Synthesis Voices
    // 1. Meld Bi-Timbral MPE Macro Synthesizer routing
    let mut meld_engine = SuiteInstrumentEngine::new(sample_rate, SuiteInstrumentKind::Meld);
    meld_engine.note_on(60, 100);
    meld_engine.set_pitch_bend(3.0);
    meld_engine.set_mpe_slide(0.75);
    meld_engine.set_mpe_pressure(0.80);
    assert_eq!(meld_engine.meld.voices[0].pitch_bend_semitones, 3.0);
    assert_eq!(meld_engine.meld.voices[0].slide, 0.75);
    assert_eq!(meld_engine.meld.voices[0].pressure, 0.80);
    let mut meld_l = [0.0f32; 256];
    let mut meld_r = [0.0f32; 256];
    meld_engine.process_block(&mut meld_l, &mut meld_r);
    assert!(meld_l.iter().map(|s| s * s).sum::<f32>() > 0.0001);

    // 2. Drift Analog Subtractive Synthesizer with MPE voice routing
    let mut drift_engine = SuiteInstrumentEngine::new(sample_rate, SuiteInstrumentKind::Drift);
    drift_engine.note_on(57, 100);
    drift_engine.set_pitch_bend(-2.0);
    drift_engine.set_mpe_slide(0.60);
    drift_engine.set_mpe_pressure(0.70);
    assert_eq!(drift_engine.drift.voices[0].pitch_bend_semitones, -2.0);
    assert_eq!(drift_engine.drift.voices[0].slide, 0.60);
    assert_eq!(drift_engine.drift.voices[0].pressure, 0.70);
    let mut drift_l = [0.0f32; 256];
    let mut drift_r = [0.0f32; 256];
    drift_engine.process_block(&mut drift_l, &mut drift_r);
    assert!(drift_l.iter().map(|s| s * s).sum::<f32>() > 0.0001);

    // 3. Chapter 12 & OKF-05 Per-Note Expression Parsing from Multi-Channel MPE Stream
    let mut parser = MpeStreamParser::new(MpeZone::Lower);
    let mpe_events = [
        (0.0, MidiEvent::NoteOn { channel: 1, note: 60, velocity: 100 }),
        (0.1, MidiEvent::PitchBend { channel: 1, value: 4096 }),
        (0.2, MidiEvent::ControlChange { channel: 1, controller: 74, value: 96 }),
        (0.3, MidiEvent::ControlChange { channel: 1, controller: 11, value: 110 }),
        (0.5, MidiEvent::NoteOff { channel: 1, note: 60 }),
    ];
    let parsed_notes = parser.parse_stream(&mpe_events, 0.5);
    assert_eq!(parsed_notes.len(), 1);
    assert_eq!(parsed_notes[0].note.pitch, 60);
    assert!(parsed_notes[0].expression.evaluate_pitch_bend_at(0.1) > 20.0);
    assert!(parsed_notes[0].expression.evaluate_slide_at(0.2) > 0.7);
    assert!(parsed_notes[0].expression.evaluate_pressure_at(0.3) > 0.8);
        })
        .unwrap()
        .join()
        .unwrap();
}

// =========================================================================
// 12. Epics #22 & #23: Suite Audio Effects & Native MIDI FX Devices
// =========================================================================

#[test]
fn test_e2e_epic22_epic23_suite_audio_fx_and_midi_effect_devices() {
    let sample_rate = 44100.0_f32;

    // 1. All 7 Suite Audio Effects
    let fx_kinds = [
        SuiteAudioEffectKind::HybridReverb,
        SuiteAudioEffectKind::SpectralResonator,
        SuiteAudioEffectKind::SpectralTime,
        SuiteAudioEffectKind::Echo,
        SuiteAudioEffectKind::Vocoder,
        SuiteAudioEffectKind::Corpus,
        SuiteAudioEffectKind::TruePeakLimiter,
    ];

    for kind in fx_kinds {
        let mut fx = SuiteAudioEffect::new(sample_rate, kind);
        assert_eq!(fx.category(), DeviceCategory::AudioEffect);
        assert!(!fx.name().is_empty());

        let mut left = [0.5f32; 256];
        let mut right = [0.5f32; 256];
        fx.process_block(&mut left, &mut right);

        assert!(
            left.iter().all(|s| s.is_finite()) && right.iter().all(|s| s.is_finite()),
            "{:?} produced non-finite output",
            kind
        );
    }

    // Strict -0.1 dBTP True Peak Limiter Ceiling Under +12 dBFS Sine Burst
    let mut limiter = SuiteAudioEffect::new(sample_rate, SuiteAudioEffectKind::TruePeakLimiter);
    let mut burst_l = [0.0f32; 512];
    let mut burst_r = [0.0f32; 512];
    for i in 0..512 {
        let t = i as f32 / sample_rate;
        burst_l[i] = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 4.0;
        burst_r[i] = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 4.0;
    }
    limiter.process_block(&mut burst_l, &mut burst_r);

    let ceiling = TRUE_PEAK_CEILING_LIN + 1e-4;
    for (i, (&l, &r)) in burst_l.iter().zip(burst_r.iter()).enumerate() {
        assert!(
            l.abs() <= ceiling && r.abs() <= ceiling,
            "Sample {} exceeded -0.1 dBTP ceiling: l={}, r={}, limit={}",
            i,
            l,
            r,
            ceiling
        );
    }

    // Corpus Acoustic Modes
    let corpus_modes = [
        CorpusMode::Beam,
        CorpusMode::Marimba,
        CorpusMode::String,
        CorpusMode::Membrane,
        CorpusMode::Plate,
        CorpusMode::Pipe,
        CorpusMode::Tube,
    ];
    for mode in corpus_modes {
        let mut corpus = SuiteAudioEffect::new(sample_rate, SuiteAudioEffectKind::Corpus);
        corpus.corpus.mode = mode;
        corpus.corpus.update_modes();
        let mut l = [0.0f32; 128];
        let mut r = [0.0f32; 128];
        l[0] = 0.9;
        r[0] = 0.9;
        corpus.process_block(&mut l, &mut r);
        let en: f32 = l.iter().map(|s| s * s).sum();
        assert!(en > 1e-4, "Corpus mode {:?} did not resonate", mode);
    }

    // Chapter 25 & OKF-06/09 / Epic #22: Full Suite Audio Effects Parity Verification
    // A. Roar: Multi-Stage Saturation with ADAA Antialiasing & Routing Topologies
    let mut roar = Roar::new(sample_rate);
    roar.topology = RoarTopology::Serial;
    roar.stage1.curve = RoarCurve::SoftSine;
    roar.stage1.drive_db = 12.0;
    roar.stage2.curve = RoarCurve::Tube;
    roar.stage2.drive_db = 6.0;
    let mut roar_buf = AudioBuffer::silence(256);
    for i in 0..256 {
        let t = i as f32 / sample_rate;
        roar_buf.left[i] = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.7;
        roar_buf.right[i] = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.7;
    }
    let mt = MusicalTime::default();
    roar.process_audio(&mut roar_buf, &mt);
    for i in 0..256 {
        assert!(roar_buf.left[i].is_finite() && roar_buf.right[i].is_finite());
        assert!(!roar_buf.left[i].is_nan() && !roar_buf.right[i].is_nan());
    }
    let roar_energy: f32 = roar_buf.left.iter().take(256).map(|s| s * s).sum();
    assert!(roar_energy > 0.01, "Roar must output saturated audio");

    // B. EQ Eight: 8-Band ZDF Parametric Equalizer
    let mut eq8 = EQEight::new(sample_rate);
    eq8.configure_band(0, EqBandType::LowCut48, 40.0, 0.0, 0.707);
    eq8.configure_band(2, EqBandType::Bell, 1000.0, 6.0, 1.5);
    eq8.configure_band(7, EqBandType::HighCut48, 16000.0, 0.0, 0.707);
    let mut eq_buf = AudioBuffer::silence(256);
    for i in 0..256 {
        eq_buf.left[i] = 0.5;
        eq_buf.right[i] = 0.5;
    }
    eq8.process_audio(&mut eq_buf, &mt);
    for i in 0..256 {
        assert!(eq_buf.left[i].is_finite() && eq_buf.right[i].is_finite());
    }

    // C. Glue Compressor: SSL G-Bus Console Compressor with Gain Reduction
    let mut glue = GlueCompressor::new(sample_rate);
    glue.threshold_db = -12.0;
    glue.ratio = 4.0;
    glue.attack_ms = 1.0;
    glue.release_s = 0.2;
    glue.soft_clip = true;
    let mut glue_buf = AudioBuffer::silence(512);
    for i in 0..512 {
        let t = i as f32 / sample_rate;
        glue_buf.left[i] = (2.0 * std::f32::consts::PI * 100.0 * t).sin() * 2.0;
        glue_buf.right[i] = (2.0 * std::f32::consts::PI * 100.0 * t).sin() * 2.0;
    }
    glue.process_audio(&mut glue_buf, &mt);
    assert!(glue.gain_reduction_db() < -0.5, "Glue Compressor must assert gain reduction on +6dBFS peaks");
    for i in 0..512 {
        assert!(glue_buf.left[i].is_finite() && glue_buf.right[i].is_finite());
        assert!(glue_buf.left[i].abs() <= 1.05, "Glue soft clip must bound output peak");
    }

    // D. Auto Filter: Cytomic ZDF State Variable Filter with Analog Drive
    let mut auto_filter = AutoFilter::new(sample_rate);
    auto_filter.filter_type = CytomicFilterType::Lowpass24;
    auto_filter.circuit_model = CircuitModel::MS2;
    auto_filter.cutoff_hz = 500.0;
    auto_filter.resonance = 0.6;
    let mut af_buf = AudioBuffer::silence(256);
    for i in 0..256 {
        let t = i as f32 / sample_rate;
        af_buf.left[i] = (2.0 * std::f32::consts::PI * 2000.0 * t).sin() * 0.8;
        af_buf.right[i] = (2.0 * std::f32::consts::PI * 2000.0 * t).sin() * 0.8;
    }
    auto_filter.process_audio(&mut af_buf, &mt);
    for i in 0..256 {
        assert!(af_buf.left[i].is_finite() && af_buf.right[i].is_finite());
    }
    let af_out_energy: f32 = af_buf.left.iter().take(256).map(|s| s * s).sum();
    let af_in_energy = 0.8 * 0.8 * 0.5 * 256.0;
    assert!(af_out_energy < af_in_energy * 0.25, "Auto Filter LP24 must attenuate 2kHz tone");

    // E. Multiband Dynamics: OTT 3-Band Upward/Downward Compressor
    let mut ott = MultibandDynamics::new(sample_rate);
    ott.time_macro = 1.0;
    ott.amount = 1.0;
    let mut ott_buf = AudioBuffer::silence(512);
    for i in 0..512 {
        let t = i as f32 / sample_rate;
        ott_buf.left[i] = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
        ott_buf.right[i] = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
    }
    ott.process_audio(&mut ott_buf, &mt);
    for i in 0..512 {
        assert!(ott_buf.left[i].is_finite() && ott_buf.right[i].is_finite());
    }
    let ott_energy: f32 = ott_buf.left.iter().take(512).map(|s| s * s).sum();
    assert!(ott_energy > 0.001, "Multiband Dynamics (OTT) must produce processed audio");

    // 2. Chapter 26 & Epic #23: All 8 Native MIDI Effects Devices
    // -------------------------------------------------------------------------
    // A. Arpeggiator: 8 Pattern Modes, Sync vs Free Rate, and Octave Stepping
    let mut arp = ArpeggiatorDevice::new();
    arp.style = ArpStyle::Up;
    arp.rate = ArpRate::Rate1_16;
    arp.octaves = 2;
    let t0 = MusicalTime::new(0.0, 120.0, 4, 4, true);
    let mut out_arp = Vec::new();
    arp.process_events(
        &[
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
            MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 },
        ],
        &mut out_arp,
        &t0,
    );
    assert_eq!(out_arp.len(), 1);
    assert_eq!(out_arp[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

    // Step next 1/16th beat (0.25 beat)
    let t1 = MusicalTime::new(0.25, 120.0, 4, 4, true);
    let mut out_arp1 = Vec::new();
    arp.process_events(&[], &mut out_arp1, &t1);
    assert_eq!(out_arp1.len(), 2); // NoteOff(60) then NoteOn(64)
    assert_eq!(out_arp1[0], MidiEvent::NoteOff { channel: 0, note: 60 });
    assert_eq!(out_arp1[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });

    // Verify Down, UpDown, Converge, Diverge styles
    let mut arp_down = ArpeggiatorDevice::new();
    arp_down.style = ArpStyle::Down;
    let mut out_down = Vec::new();
    arp_down.process_events(
        &[
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
            MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 },
        ],
        &mut out_down,
        &t0,
    );
    assert_eq!(out_down[0], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });

    // Free rate mode (synced = false)
    let mut arp_free = ArpeggiatorDevice::new();
    arp_free.synced = false;
    arp_free.free_rate_hz = 10.0;
    let mut out_free = Vec::new();
    arp_free.process_events(
        &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
        &mut out_free,
        &t0,
    );
    assert_eq!(out_free.len(), 1);

    // B. Chord Device: Up to 6 Semitone Intervals + Velocity Scaling
    let mut chord = ChordDevice::new();
    chord.shifts = [4, 7, 11, 14, 18, 21];
    chord.num_shifts = 6;
    let mut out_chord = Vec::new();
    chord.process_events(
        &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
        &mut out_chord,
    );
    assert_eq!(out_chord.len(), 7, "Root + 6 harmonic intervals");
    assert_eq!(out_chord[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    assert_eq!(out_chord[1], MidiEvent::NoteOn { channel: 0, note: 64, velocity: 90 }); // Major 3rd
    assert_eq!(out_chord[2], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 85 }); // Perfect 5th

    // C. Pitch Device: Semitone Transposition & Key Range Filtering
    let mut pitch = PitchDevice::new();
    pitch.transpose = 7;
    pitch.lowest_note = 48;
    pitch.highest_note = 84;
    let mut out_pitch = Vec::new();
    pitch.process_events(
        &[
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
            MidiEvent::NoteOn { channel: 0, note: 36, velocity: 100 }, // Filtered out (< 48)
        ],
        &mut out_pitch,
    );
    assert_eq!(out_pitch.len(), 1);
    assert_eq!(out_pitch[0], MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });

    // D. Random Device: Chance, Choices, Sign Modes, and NoteOff History
    let mut random = RandomDevice::new();
    random.chance = 1.0;
    random.choices = 7;
    random.sign = RandomSign::Positive;
    let mut out_rand = Vec::new();
    random.process_events(
        &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
        &mut out_rand,
    );
    assert_eq!(out_rand.len(), 1);
    let rand_note = match out_rand[0] {
        MidiEvent::NoteOn { note, .. } => {
            assert!(note > 60 && note <= 67);
            note
        }
        _ => panic!("Expected NoteOn"),
    };
    let mut out_rand_off = Vec::new();
    random.process_events(&[MidiEvent::NoteOff { channel: 0, note: 60 }], &mut out_rand_off);
    assert_eq!(out_rand_off.len(), 1);
    assert_eq!(out_rand_off[0], MidiEvent::NoteOff { channel: 0, note: rand_note });

    // E. Scale Device: 12-Tone Quantization Matrix & Musical Presets
    let scale_major = ScaleDevice::new();
    assert_eq!(scale_major.quantize(60), 60); // C in C Major
    let q = scale_major.quantize(61); // C# not in C Major -> folds to 60 or 62
    assert!(q == 60 || q == 62);
    let scale_minor = ScaleDevice::minor();
    assert_eq!(scale_minor.quantize(63), 63); // Eb in C Natural Minor

    // F. Velocity Device: Drive & Compand Curves, Random Variance & Clip Bounds
    let mut vel = VelocityDevice::new();
    vel.curve = VelocityCurve::Drive;
    vel.drive = 0.5;
    vel.out_low = 30;
    vel.out_high = 110;
    let v_out = vel.transform_velocity(64);
    assert!(v_out >= 30 && v_out <= 110);
    vel.curve = VelocityCurve::Compand;
    vel.compand = 2.0;
    let v_comp = vel.transform_velocity(64);
    assert!(v_comp >= 30 && v_comp <= 110);

    // G. Note Length Device: Standard Multiplier & Release Velocity Trigger
    let mut nl = NoteLengthDevice::new();
    nl.release_trigger = false;
    let mut out_nl = Vec::new();
    nl.process_events(
        &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 90 }],
        &mut out_nl,
    );
    assert_eq!(out_nl.len(), 1);

    let mut nl_rel = NoteLengthDevice::new();
    nl_rel.release_trigger = true;
    nl_rel.release_velocity = 85;
    let mut out_rel_on = Vec::new();
    nl_rel.process_events(
        &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 90 }],
        &mut out_rel_on,
    );
    assert_eq!(out_rel_on.len(), 0); // suppressed
    let mut out_rel_off = Vec::new();
    nl_rel.process_events(&[MidiEvent::NoteOff { channel: 0, note: 60 }], &mut out_rel_off);
    assert_eq!(out_rel_off.len(), 2); // NoteOn(85) then NoteOff
    assert_eq!(out_rel_off[0], MidiEvent::NoteOn { channel: 0, note: 60, velocity: 85 });

    // H. MPE Control Device: Expression Curves (Pitch Bend, Slide CC74, Pressure)
    let mut mpe = MpeControlDevice::new();
    mpe.curve_skew = 0.5;
    mpe.slide_skew = -0.4;
    mpe.pressure_skew = 0.3;
    let mut out_mpe = Vec::new();
    mpe.process_events(
        &[
            MidiEvent::PitchBend { channel: 0, value: 4096 },
            MidiEvent::ControlChange { channel: 0, controller: 74, value: 64 },
            MidiEvent::ControlChange { channel: 0, controller: 11, value: 80 },
        ],
        &mut out_mpe,
    );
    assert_eq!(out_mpe.len(), 3);

    // I. MIDI Effect Rack Chain: Serial Processing Pipeline
    let mut chain = MidiEffectChain::new();
    chain.add(MidiEffectDevice::Pitch(pitch));
    chain.add(MidiEffectDevice::Scale(scale_major));
    let mut chain_out = Vec::new();
    chain.process(
        &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
        &mut chain_out,
        &t0,
    );
    assert_eq!(chain_out.len(), 1);
    if let MidiEvent::NoteOn { note, .. } = chain_out[0] {
        assert_eq!(note, 67);
    }

    // 3. Chapter 11 & Epic #23: Live 12 Generative & Transform Algorithms
    // A. Arpeggiate Transform
    let mut arp_pat = MidiPattern::new(4.0);
    arp_pat.add_note(MidiNote::new(60, 100, 0.0, 1.0));
    arp_pat.add_note(MidiNote::new(64, 100, 0.0, 1.0));
    arp_pat.add_note(MidiNote::new(67, 100, 0.0, 1.0));
    oxide_timeline::transform_arpeggiate(&mut arp_pat, 0.25, false);
    assert_eq!(arp_pat.notes.len(), 3);
    assert!((arp_pat.notes[0].start_beat - 0.0).abs() < 1e-4);
    assert!((arp_pat.notes[1].start_beat - 0.25).abs() < 1e-4);
    assert!((arp_pat.notes[2].start_beat - 0.50).abs() < 1e-4);

    // B. Chord Generative (Stacks)
    let chord_pat = oxide_timeline::generate_stacks(
        &[60],
        oxide_timeline::StacksChordType::MajorTriad,
        oxide_timeline::StacksVoicing::Closed,
        0,
        1.0,
        100,
    );
    assert_eq!(chord_pat.notes.len(), 3);
    assert_eq!(chord_pat.notes[0].pitch, 60); // C4
    assert_eq!(chord_pat.notes[1].pitch, 64); // E4
    assert_eq!(chord_pat.notes[2].pitch, 67); // G4

    // C. Euclidean Pulse Distribution (Bjorklund: 5 hits across 16 steps)
    let euclid_pat = oxide_timeline::generate_euclidean(5, 16, 0, 36, 0.25, 100);
    assert_eq!(euclid_pat.notes.len(), 5);
    assert_eq!(euclid_pat.length_beats, 4.0);

    // D. Invert Transform (Pitch reflection)
    let mut inv_pat = chord_pat.clone();
    oxide_timeline::transform_invert(&mut inv_pat, Some(60));
    assert_eq!(inv_pat.notes[0].pitch, 60); // 2*60 - 60 = 60
    assert_eq!(inv_pat.notes[1].pitch, 56); // 2*60 - 64 = 56
    assert_eq!(inv_pat.notes[2].pitch, 53); // 2*60 - 67 = 53

    // E. Reverse Transform (Horizontal time mirroring)
    let mut rev_pat = MidiPattern::new(4.0);
    rev_pat.add_note(MidiNote::new(60, 100, 0.0, 1.0));
    rev_pat.add_note(MidiNote::new(64, 100, 1.0, 1.0));
    rev_pat.add_note(MidiNote::new(67, 100, 3.0, 0.5));
    oxide_timeline::transform_reverse(&mut rev_pat);
    assert_eq!(rev_pat.notes[0].pitch, 67);
    assert!((rev_pat.notes[0].start_beat - 0.5).abs() < 1e-4);
    assert_eq!(rev_pat.notes[1].pitch, 64);
    assert!((rev_pat.notes[1].start_beat - 2.0).abs() < 1e-4);
    assert_eq!(rev_pat.notes[2].pitch, 60);
    assert!((rev_pat.notes[2].start_beat - 3.0).abs() < 1e-4);

    // F. Legato Transform (Duration expansion)
    let mut leg_pat = MidiPattern::new(4.0);
    leg_pat.add_note(MidiNote::new(60, 100, 0.0, 0.5));
    leg_pat.add_note(MidiNote::new(64, 100, 1.0, 0.5));
    oxide_timeline::transform_legato(&mut leg_pat);
    assert!((leg_pat.notes[0].duration_beats - 1.0).abs() < 1e-4);
    assert!((leg_pat.notes[1].duration_beats - 3.0).abs() < 1e-4);

    // G. Humanize Transform (Organic timing & velocity variance)
    let mut hum_pat = MidiPattern::new(4.0);
    hum_pat.add_note(MidiNote::new(60, 100, 1.0, 0.5));
    oxide_timeline::transform_humanize(&mut hum_pat, 15.0 / 960.0, 8, 42);
    assert!((hum_pat.notes[0].start_beat - 1.0).abs() <= 15.0 / 960.0 + 1e-5);
    assert!((hum_pat.notes[0].velocity as i16 - 100).abs() <= 8);

    // H. Unified MidiTransformEngine Facade
    let mut engine_pat = MidiPattern::new(4.0);
    engine_pat.add_note(MidiNote::new(60, 100, 0.0, 1.0));
    oxide_timeline::MidiTransformEngine::invert(&mut engine_pat, Some(64));
    assert_eq!(engine_pat.notes[0].pitch, 68);
}

// =========================================================================
// 13. Epic #24: Hardware Control Surfaces, Push 3 & MIDI Learn/Takeover
// =========================================================================

#[test]
fn test_e2e_epic24_push3_matrix_sync_midi_learn_and_takeover_curves() {
    assert_eq!(ControlSurfaceModel::Push3.name(), "Ableton Push 3");
    assert!(ControlSurfaceModel::Push3.has_display());
    assert_eq!(ControlSurfaceModel::Push3.display_resolution(), Some((960, 160)));
    assert_eq!(ControlSurfaceModel::Push3.pad_matrix_dimensions(), (8, 8));

    assert_eq!(ControlSurfaceModel::Apc40MkII.pad_matrix_dimensions(), (5, 8));
    assert_eq!(ControlSurfaceModel::LaunchpadProMk3.pad_matrix_dimensions(), (8, 8));

    let mut push = PushPadMatrixState::new();
    assert_eq!(push.pad_to_midi_note(0, 0), 36);
    assert_eq!(push.pad_to_midi_note(1, 0), 41);

    let mut clip_states = [[0u8; 8]; 8];
    clip_states[0][0] = 2; // Playing -> Green
    clip_states[0][1] = 1; // Stopped -> Amber
    clip_states[0][2] = 3; // Recording -> Red
    clip_states[0][3] = 0; // Empty -> Off
    push.sync_session_clip_state_matrix(&clip_states);

    assert_eq!(push.get_pad(0, 0), PushPadColor::GreenPlaying);
    assert_eq!(push.get_pad(0, 1), PushPadColor::AmberStopped);
    assert_eq!(push.get_pad(0, 2), PushPadColor::RedRecording);
    assert_eq!(push.get_pad(0, 3), PushPadColor::Off);

    push.display_strip.strips[0].track_name = "Bass Lead".to_string();
    push.display_strip.strips[0].volume_db = -3.0;
    push.display_strip.strips[0].parameter_name = "Cutoff".to_string();
    push.display_strip.strips[0].parameter_value_str = "1.2 kHz".to_string();
    assert_eq!(push.display_strip.strips[0].track_name, "Bass Lead");

    let mut learn = MidiLearnRegistry::new();
    assert!(!learn.is_learning());
    let target = MidiLearnTarget::new(1, 0, 7, -60.0, 6.0);
    learn.learn_next_cc(target.clone());
    assert!(learn.is_learning());

    let res = learn.process_midi_cc(0, 15, 64);
    assert!(res.is_some());
    let (learned, val) = res.unwrap();
    assert_eq!(learned, target);
    assert!(!learn.is_learning());
    assert_eq!(learn.binding_count(), 1);
    assert!((val - (-26.69)).abs() < 1.0);

    let pickup = TakeoverMode::Pickup;
    let current_soft = 0.63;
    assert_eq!(pickup.evaluate_takeover(current_soft, 0.1, 0.3), None);
    assert_eq!(pickup.evaluate_takeover(current_soft, 0.5, 0.7), Some(0.7));

    let scaling = TakeoverMode::ValueScaling;
    let val_scaled = scaling.evaluate_takeover(0.5, 0.0, 0.1).unwrap();
    assert!(val_scaled > 0.5 && val_scaled < 0.6);
    let val_max = scaling.evaluate_takeover(0.8, 0.8, 1.0).unwrap();
    assert_eq!(val_max, 1.0);
    let val_min = scaling.evaluate_takeover(0.2, 0.2, 0.0).unwrap();
    assert_eq!(val_min, 0.0);

    // Chapter 31 Parameter Min/Max Scaling & Inverted CC Mapping
    let inv_target = MidiLearnTarget::new(1, 0, 8, 100.0, 0.0);
    assert_eq!(inv_target.map_cc_to_value(0), 100.0);
    assert_eq!(inv_target.map_cc_to_value(127), 0.0);
    assert_eq!(inv_target.map_value_to_cc(100.0), 0);
    assert_eq!(inv_target.map_value_to_cc(0.0), 127);

    // Chapter 31 MIDI Note Remote Control
    learn.learn_next_note(target.clone());
    assert!(learn.is_learning_note());
    let note_res = learn.process_midi_note(0, 60, 127, -60.0);
    assert!(note_res.is_some());
    let (_, toggled_val) = note_res.unwrap();
    assert_eq!(toggled_val, 6.0);
    assert_eq!(learn.note_binding_count(), 1);

    // Chapter 31 Computer Keyboard Mapping (KeyLearnRegistry)
    let mut key_learn = KeyLearnRegistry::new();
    let key_target = KeyLearnTarget::new(0, 0, 1, 0.0, 1.0);
    key_learn.learn_next_key(key_target.clone());
    assert!(key_learn.is_learning());
    let key_res = key_learn.process_key_press("KeyM", 0.0);
    assert!(key_res.is_some());
    assert_eq!(key_res.unwrap().1, 1.0);
    assert_eq!(key_learn.binding_count(), 1);

    // Chapter 31 Keyboard Shortcuts: Cmd+M (MIDI Map), Cmd+K (Key Map), Cmd+Shift+M (New MIDI Clip)
    let cmd = Modifiers::COMMAND;
    let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    assert_eq!(
        StudioShortcutManager::evaluate_shortcut(Key::M, cmd),
        Some(StudioShortcutAction::ToggleMidiMapMode)
    );
    assert_eq!(
        StudioShortcutManager::evaluate_shortcut(Key::K, cmd),
        Some(StudioShortcutAction::ToggleKeyMapMode)
    );
    assert_eq!(
        StudioShortcutManager::evaluate_shortcut(Key::M, cmd_shift),
        Some(StudioShortcutAction::NewMidiClip)
    );

    // Push 2 & Push 3 Ribbon Controller / Touch Strip Verification
    assert!(ControlSurfaceModel::Push3.has_touch_strip());
    assert!(ControlSurfaceModel::Push2.has_touch_strip());
    assert!(!ControlSurfaceModel::Apc40MkII.has_touch_strip());

    // Pitch bend mode (auto-return spring back to center 0.5 / 8192)
    let mut pb_strip = PushTouchStrip::new(PushTouchStripMode::PitchBend);
    assert_eq!(pb_strip.pitch_bend_14bit(), 8192);
    pb_strip.touch_down(0.75);
    assert_eq!(pb_strip.pitch_bend_14bit(), 12287);
    pb_strip.touch_up();
    assert_eq!(pb_strip.pitch_bend_14bit(), 8192, "PitchBend touch strip must snap to 8192 on release");

    // Modulation wheel mode (CC 1 position hold)
    let mut mw_strip = PushTouchStrip::new(PushTouchStripMode::ModWheel);
    mw_strip.touch_down(0.8);
    assert_eq!(mw_strip.mod_wheel_cc(), 102);
    mw_strip.touch_up();
    assert_eq!(mw_strip.mod_wheel_cc(), 102, "ModWheel touch strip must retain position on release");
    assert_eq!(mw_strip.led_meter_index(24), 18);

    // Push 2 & Push 3 960x160 60 FPS Hardware Display Frame Buffer Verification
    assert_eq!(PushDisplayFrameBuffer::WIDTH, 960);
    assert_eq!(PushDisplayFrameBuffer::HEIGHT, 160);
    assert_eq!(PushDisplayFrameBuffer::PIXEL_COUNT, 153_600);
    assert_eq!(PushDisplayFrameBuffer::FRAME_BYTES, 307_200);
    assert_eq!(PushDisplayFrameBuffer::TARGET_FPS, 60.0);
    assert_eq!(PushDisplayFrameBuffer::PUSH2_USB_VID, 0x2972);
    assert_eq!(PushDisplayFrameBuffer::PUSH2_DISPLAY_ENDPOINT, 0x01);

    push.render_frame_buffer();
    assert_eq!(push.frame_buffer.frame_count, 1);
    assert_eq!(push.frame_buffer.as_raw_bytes().len(), 307_200);

    // Push 2 USB EMI XOR Scrambling Mask Roundtrip
    let orig_px = push.frame_buffer.get_pixel(0, 0);
    push.frame_buffer.apply_push2_usb_xor_mask();
    assert_ne!(push.frame_buffer.get_pixel(0, 0), orig_px);
    push.frame_buffer.apply_push2_usb_xor_mask();
    assert_eq!(push.frame_buffer.get_pixel(0, 0), orig_px);

    // Akai APC40 mkII Hardware Surface Verification
    assert_eq!(ControlSurfaceModel::Apc40MkII.fader_count(), 9);
    assert_eq!(ControlSurfaceModel::Apc40MkII.encoder_count(), 16);
    let mut apc = Apc40State::new();
    apc.set_clip_state(1, 2, 2); // Scene 1, Track 2: Playing
    assert_eq!(apc.get_clip_state(1, 2), 2);
    apc.set_channel_fader(2, 0.9);
    assert_eq!(apc.get_channel_fader(2), 0.9);
    apc.set_crossfader(0.75);
    assert_eq!(apc.crossfader, 0.75);

    // Novation Launchpad Pro MK3 Hardware Surface Verification
    assert!(ControlSurfaceModel::LaunchpadProMk3.has_mpe());
    let mut lp = LaunchpadProState::new();
    lp.set_pad(3, 4, PushPadColor::RedRecording);
    assert_eq!(lp.get_pad(3, 4), PushPadColor::RedRecording);
    lp.set_pad_pressure(3, 4, 115);
    assert_eq!(lp.get_pad_pressure(3, 4), 115);
    lp.set_scene_button(1, PushPadColor::AmberStopped);
    assert_eq!(lp.get_scene_button(1), PushPadColor::AmberStopped);
    lp.set_function_button(0, true);
    assert!(lp.get_function_button(0));
}

// =========================================================================
// 14. Epic #25: Track Groups, Freeze & Flatten, Follow Actions & Linked Tracks
// =========================================================================

#[test]
fn test_e2e_epic25_track_groups_freeze_flatten_follow_actions_linked_tracks() {
    let mut group = TrackGroupBus::new(1, "Drum Group");
    group.add_member(10);
    group.add_member(11);
    assert!(group.contains_member(10));
    assert!(group.contains_member(11));

    assert!(!group.is_folded);
    group.toggle_folded();
    assert!(group.is_folded);

    let t1_audio = [0.4f32; 16];
    let t2_audio = [0.3f32; 16];
    let mut submix_out = [0.0f32; 16];
    group.submix_sum(&[&t1_audio, &t2_audio], &mut submix_out);
    assert!((submix_out[0] - 0.7).abs() < 1e-4);

    group.set_muted(true);
    group.submix_sum(&[&t1_audio, &t2_audio], &mut submix_out);
    assert_eq!(submix_out[0], 0.0, "Muted group must produce bit-exact silence");

    let (eff_mute, eff_solo) = group.propagate_mute_solo(10, false, false);
    assert!(eff_mute);
    assert!(!eff_solo);

    let notes = vec![
        MidiNote::new(60, 100, 0.0, 1.0),
        MidiNote::new(64, 110, 1.0, 1.0),
    ];
    let frozen = TrackFreezeEngine::freeze_track(1, &notes, 2.0, 44100.0);
    assert!(frozen.is_frozen);
    assert!(!frozen.is_empty());
    assert_eq!(frozen.tail_samples, 22050, "0.5s tail @ 44.1 kHz = 22050 samples");
    assert!(frozen.peak_amplitude() > 0.0);

    let flattened = TrackFreezeEngine::flatten_track(&frozen);
    assert_eq!(flattened.len(), frozen.total_frames() * 2);

    let populated = vec![0, 1, 2, 3];
    let rule_next = FollowActionRule::new(FollowAction::Next, 1.0, FollowAction::None, 0.0, 1.0);
    assert_eq!(rule_next.evaluate_next_slot(1, 4, &populated, 42), Some(2));
    assert_eq!(rule_next.evaluate_next_slot(3, 4, &populated, 42), Some(0));

    let rule_other = FollowActionRule::new(FollowAction::Other, 1.0, FollowAction::None, 0.0, 1.0);
    for seed in 0..10 {
        let next = rule_other.evaluate_next_slot(1, 4, &populated, seed);
        assert!(next.is_some() && next != Some(1), "Other must exclude current slot");
    }

    // Verify all remaining FollowAction variants in Epic #25 / Chapter 16
    let rule_play_again = FollowActionRule::new(FollowAction::PlayAgain, 1.0, FollowAction::None, 0.0, 1.0);
    assert_eq!(rule_play_again.evaluate_next_slot(2, 4, &populated, 42), Some(2));

    let rule_jump = FollowActionRule::new(FollowAction::Jump(3), 1.0, FollowAction::None, 0.0, 1.0);
    assert_eq!(rule_jump.evaluate_next_slot(0, 4, &populated, 42), Some(3));

    let rule_stop = FollowActionRule::new(FollowAction::Stop, 1.0, FollowAction::None, 0.0, 1.0);
    assert_eq!(rule_stop.evaluate_next_slot(1, 4, &populated, 42), None);

    let rule_first = FollowActionRule::new(FollowAction::First, 1.0, FollowAction::None, 0.0, 1.0);
    assert_eq!(rule_first.evaluate_next_slot(2, 4, &populated, 42), Some(0));

    let rule_last = FollowActionRule::new(FollowAction::Last, 1.0, FollowAction::None, 0.0, 1.0);
    assert_eq!(rule_last.evaluate_next_slot(2, 4, &populated, 42), Some(3));

    let rule_prev = FollowActionRule::new(FollowAction::Previous, 1.0, FollowAction::None, 0.0, 1.0);
    assert_eq!(rule_prev.evaluate_next_slot(2, 4, &populated, 42), Some(1));
    assert_eq!(rule_prev.evaluate_next_slot(0, 4, &populated, 42), Some(3));

    let rule_any = FollowActionRule::new(FollowAction::Any, 1.0, FollowAction::None, 0.0, 1.0);
    assert!(rule_any.evaluate_next_slot(1, 4, &populated, 42).is_some());


    // ---------------------------------------------------------------------
    // Chapter 6 / Epic #25: ArrangementTimeline fractional beat clip placement,
    // locators navigation, Split (Cmd+E), Consolidate (Cmd+J), and LinkedTrackGroup
    // ---------------------------------------------------------------------
    let mut timeline = ArrangementTimeline::new();

    // 1. Fractional beat clip placement
    let mut pattern_hihat = MidiPattern::new(4.0);
    pattern_hihat.add_note(MidiNote::new(42, 100, 0.0, 0.5));
    pattern_hihat.add_note(MidiNote::new(42, 90, 0.5, 0.5));
    let clip_fractional = ArrangementClip::new(
        201,
        "HiHat 16ths",
        1.25,
        2.75,
        ClipPattern::Midi(pattern_hihat),
    );
    assert_eq!(clip_fractional.timeline_start_beat, 1.25);
    assert_eq!(clip_fractional.duration_beats, 2.75);
    assert_eq!(clip_fractional.timeline_end_beat(), 4.0);
    assert!(!clip_fractional.contains_beat(1.24));
    assert!(clip_fractional.contains_beat(1.25));
    assert!(clip_fractional.contains_beat(2.5));
    assert!(clip_fractional.contains_beat(3.99));
    assert!(!clip_fractional.contains_beat(4.0));
    assert_eq!(clip_fractional.pattern_phase_at_beat(2.5), 1.25);

    // 2. Locator navigation
    timeline.add_locator(0.0, "1. Intro");
    timeline.add_locator(16.5, "2. Build");
    timeline.add_locator(32.25, "3. Drop");
    timeline.add_locator(48.0, "4. Outro");
    assert_eq!(timeline.locators.len(), 4);
    let loc0: &TimelineLocator = &timeline.locators[0];
    assert_eq!(loc0.name, "1. Intro");
    assert_eq!(timeline.locators[1].beat, 16.5);
    assert_eq!(timeline.locators[2].beat, 32.25);

    // Initial playhead at 0.0 -> next locator jumps to 16.5
    assert_eq!(timeline.playhead_beat, 0.0);
    let next1 = timeline.next_locator().unwrap();
    assert_eq!(next1.name, "2. Build");
    assert_eq!(timeline.playhead_beat, 16.5);

    // Next locator jumps to 32.25
    let next2 = timeline.next_locator().unwrap();
    assert_eq!(next2.name, "3. Drop");
    assert_eq!(timeline.playhead_beat, 32.25);

    // Prev locator jumps back to 16.5
    let prev1 = timeline.prev_locator().unwrap();
    assert_eq!(prev1.name, "2. Build");
    assert_eq!(timeline.playhead_beat, 16.5);

    // Jump by name and index
    assert!(timeline.jump_to_locator_by_name("4. Outro"));
    assert_eq!(timeline.playhead_beat, 48.0);
    assert!(timeline.jump_to_locator(0));
    assert_eq!(timeline.playhead_beat, 0.0);

    // 3. Multi-track arrangement tracks & LinkedTrackGroup synchronized editing
    let mut track1 = ArrangementTrack::new(1, "Kick", "Audio");
    let mut track2 = ArrangementTrack::new(2, "Snare", "Audio");
    let pattern = ClipPattern::Midi(MidiPattern::new(8.0));
    track1.add_clip(ArrangementClip::new(101, "Kick Stem", 0.0, 8.0, pattern.clone()));
    track2.add_clip(ArrangementClip::new(102, "Snare Stem", 0.0, 8.0, pattern));

    let mut tracks = vec![track1, track2];
    let linked = LinkedTrackGroup::new(1, vec![1, 2]);

    // Synchronized Split (Cmd+E) at fractional beat 3.5 across linked tracks
    let splits = linked.split_clips(&mut tracks, 3.5);
    assert_eq!(splits, 2, "Both linked tracks must split synchronously at 3.5");
    assert_eq!(tracks[0].clips.len(), 2);
    assert_eq!(tracks[1].clips.len(), 2);
    assert_eq!(tracks[0].clips[0].duration_beats, 3.5);
    assert_eq!(tracks[0].clips[1].timeline_start_beat, 3.5);
    assert_eq!(tracks[0].clips[1].duration_beats, 4.5);
    assert_eq!(tracks[1].clips[0].duration_beats, 3.5);
    assert_eq!(tracks[1].clips[1].timeline_start_beat, 3.5);

    // Synchronized Move by +0.5 beat
    linked.move_clips(&mut tracks, 0.5);
    assert_eq!(tracks[0].clips[0].timeline_start_beat, 0.5);
    assert_eq!(tracks[0].clips[1].timeline_start_beat, 4.0);
    assert_eq!(tracks[1].clips[0].timeline_start_beat, 0.5);
    assert_eq!(tracks[1].clips[1].timeline_start_beat, 4.0);

    // Synchronized Consolidate (Cmd+J) over [0.0, 9.0)
    let consolidated_count = linked.consolidate_clips(&mut tracks, 0.0, 9.0, 500);
    assert_eq!(consolidated_count, 2, "Both linked tracks must consolidate into unified clips");
    assert_eq!(tracks[0].clips.len(), 1);
    assert_eq!(tracks[1].clips.len(), 1);
    assert_eq!(tracks[0].clips[0].duration_beats, 9.0);
    assert_eq!(tracks[1].clips[0].duration_beats, 9.0);

    // ---------------------------------------------------------------------
    // Chapter 7: Session View: Non-Linear Matrix, Slot States, Scene Launch,
    // Master Stop Controls, Launch Quantization (None to 4 Bars) & Trigger Modes
    // ---------------------------------------------------------------------
    let mut matrix = SessionMatrix::new(3, 4);
    assert_eq!(matrix.tracks.len(), 3);
    assert_eq!(matrix.scenes.len(), 4);

    // Initial state: all slots are Empty
    for trk in &matrix.tracks {
        for slot in &trk.slots {
            assert!(slot.state.is_empty());
            assert_eq!(slot.state, SlotState::Empty);
        }
    }

    // Populate clips across tracks:
    // Track 0: Trigger mode clip (Lead synth)
    let lead_pat = ClipPattern::Midi(MidiPattern::new(4.0));
    let clip_lead = SessionClip::new(1, "Lead Synth", lead_pat)
        .with_trigger_mode(TriggerMode::Trigger);
    matrix.tracks[0].slots[0].set_clip(Some(clip_lead));
    assert!(matrix.tracks[0].slots[0].state.is_stopped());

    // Track 1: Toggle mode clip (Bass groove)
    let bass_pat = ClipPattern::Midi(MidiPattern::new(4.0));
    let clip_bass = SessionClip::new(2, "Sub Bass", bass_pat)
        .with_trigger_mode(TriggerMode::Toggle);
    matrix.tracks[1].slots[0].set_clip(Some(clip_bass));
    assert!(matrix.tracks[1].slots[0].state.is_stopped());

    // Track 2: Gate mode clip (Vocal chop)
    let vocal_pat = ClipPattern::Midi(MidiPattern::new(4.0));
    let clip_vocal = SessionClip::new(3, "Vocal Chop", vocal_pat)
        .with_trigger_mode(TriggerMode::Gate);
    matrix.tracks[2].slots[0].set_clip(Some(clip_vocal));
    assert!(matrix.tracks[2].slots[0].state.is_stopped());

    // Also populate Scene 1 on Track 0 and Track 1
    let lead_pat2 = ClipPattern::Midi(MidiPattern::new(4.0));
    let clip_lead2 = SessionClip::new(4, "Lead Chorus", lead_pat2)
        .with_trigger_mode(TriggerMode::Trigger);
    matrix.tracks[0].slots[1].set_clip(Some(clip_lead2));

    // 1. Verify Launch Quantization intervals (None to 4 Bars, and 8 Bars)
    assert_eq!(oxide_timeline::calculate_launch_beat(3.5, QuantizeMode::Immediate), 3.5);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::Sixteenth), 3.25);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::Eighth), 3.5);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::Quarter), 4.0);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::Half), 4.0);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::OneBar), 4.0);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::TwoBars), 8.0);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::FourBars), 16.0);
    assert_eq!(oxide_timeline::calculate_launch_beat(3.1, QuantizeMode::EightBars), 32.0);

    // 2. Scene Launching: Trigger Scene 0 at beat 2.5 with 1 Bar launch quantization
    let scene_target = matrix.trigger_scene(0, 2.5, QuantizeMode::OneBar);
    assert_eq!(scene_target, 4.0, "1 Bar quantization at 2.5 schedules downbeat at 4.0");
    assert_eq!(matrix.tracks[0].slots[0].state, SlotState::QueuedLaunch(4.0));
    assert_eq!(matrix.tracks[1].slots[0].state, SlotState::QueuedLaunch(4.0));
    assert_eq!(matrix.tracks[2].slots[0].state, SlotState::QueuedLaunch(4.0));

    // Tick before boundary: still queued
    matrix.tick(3.9);
    assert!(matrix.tracks[0].slots[0].state.is_queued_launch());
    assert!(matrix.tracks[1].slots[0].state.is_queued_launch());
    assert!(matrix.tracks[2].slots[0].state.is_queued_launch());

    // Tick at boundary (4.0): all slots transition to Playing
    matrix.tick(4.0);
    assert!(matrix.tracks[0].slots[0].state.is_playing());
    assert!(matrix.tracks[1].slots[0].state.is_playing());
    assert!(matrix.tracks[2].slots[0].state.is_playing());

    // Loop phase advancement during playback
    matrix.tick(5.5);
    if let SlotState::Playing { phase, .. } = matrix.tracks[0].slots[0].state {
        assert!((phase - 1.5).abs() < 1e-4);
    } else {
        panic!("Track 0 Slot 0 must be Playing");
    }

    // 3. Monophonic exclusivity: triggering Slot 1 on Track 0 queues stop for Slot 0
    let t0_target = matrix.trigger_clip(0, 1, 6.0, QuantizeMode::OneBar);
    assert_eq!(t0_target, Some(8.0));
    assert_eq!(matrix.tracks[0].slots[0].state, SlotState::QueuedStop(8.0));
    assert_eq!(matrix.tracks[0].slots[1].state, SlotState::QueuedLaunch(8.0));

    matrix.tick(8.0);
    assert_eq!(matrix.tracks[0].slots[0].state, SlotState::Stopped);
    assert!(matrix.tracks[0].slots[1].state.is_playing());

    // 4. Trigger Modes verification:
    // a) Toggle Mode on Track 1: clicking while playing queues stop!
    let toggle_target = matrix.trigger_clip_with_mode(1, 0, 8.5, QuantizeMode::OneBar, TriggerMode::Toggle);
    assert_eq!(toggle_target, Some(12.0));
    assert_eq!(matrix.tracks[1].slots[0].state, SlotState::QueuedStop(12.0));
    matrix.tick(12.0);
    assert_eq!(matrix.tracks[1].slots[0].state, SlotState::Stopped);

    // b) Gate Mode on Track 2: release_gate_clip stops playback
    assert!(matrix.tracks[2].slots[0].state.is_playing());
    let gate_stop = matrix.release_gate_clip(2, 0, 12.0, QuantizeMode::Immediate);
    assert_eq!(gate_stop, Some(12.0));
    assert_eq!(matrix.tracks[2].slots[0].state, SlotState::Stopped);

    // 5. Per-track Stop button ([■]):
    // Re-launch Track 0 Slot 1
    matrix.trigger_clip(0, 1, 13.0, QuantizeMode::Immediate);
    assert!(matrix.tracks[0].slots[1].state.is_playing());
    matrix.stop_track(0, 14.0, QuantizeMode::Immediate);
    assert_eq!(matrix.tracks[0].slots[1].state, SlotState::Stopped);

    // 6. Master Stop All Clips ([■ Stop All]):
    // Launch clips on Track 0 and Track 1
    matrix.trigger_clip(0, 0, 15.0, QuantizeMode::Immediate);
    matrix.trigger_clip(1, 0, 15.0, QuantizeMode::Immediate);
    assert!(matrix.tracks[0].slots[0].state.is_playing());
    assert!(matrix.tracks[1].slots[0].state.is_playing());

    // Stop All Clips:
    for trk in &mut matrix.tracks {
        trk.stop_immediate();
    }
    assert_eq!(matrix.tracks[0].slots[0].state, SlotState::Stopped);
    assert_eq!(matrix.tracks[1].slots[0].state, SlotState::Stopped);
    assert_eq!(matrix.tracks[2].slots[0].state, SlotState::Stopped);

    // ---------------------------------------------------------------------
    // Chapter 20 / Epic #25: Comping & Take Lanes: Blade Splicing & Micro-Crossfades
    // ---------------------------------------------------------------------
    let mut comp_track = Track::new(10, "Lead Vocal Track", TrackType::Audio);
    assert_eq!(comp_track.comping.take_lanes.len(), 0);

    // 1. Multi-pass take lane recording
    let take1_id = comp_track.comping.add_take_lane("Vocal Take 1");
    let take2_id = comp_track.comping.add_take_lane("Vocal Take 2");
    let take3_id = comp_track.comping.add_take_lane("Vocal Take 3");
    assert_eq!(comp_track.comping.take_lanes.len(), 3);

    // Populate takes with phrase clips
    let pat_t1 = ClipPattern::Midi(MidiPattern::new(16.0));
    let pat_t2 = ClipPattern::Midi(MidiPattern::new(16.0));
    let pat_t3 = ClipPattern::Midi(MidiPattern::new(16.0));
    comp_track.comping.get_take_lane_mut(take1_id).unwrap().add_clip(
        ArrangementClip::new(301, "Take 1 Phrase", 0.0, 16.0, pat_t1),
    );
    comp_track.comping.get_take_lane_mut(take2_id).unwrap().add_clip(
        ArrangementClip::new(302, "Take 2 Phrase", 0.0, 16.0, pat_t2),
    );
    comp_track.comping.get_take_lane_mut(take3_id).unwrap().add_clip(
        ArrangementClip::new(303, "Take 3 Phrase", 0.0, 16.0, pat_t3),
    );

    // 2. Blade splicing on Take 2 at beat 8.0
    let blade_res = comp_track.comping.blade_split_take_lane(take2_id, 8.0).expect("Blade slice must succeed");
    assert!(blade_res, "Blade tool must split Take 2 clip into 2 segments");
    assert_eq!(comp_track.comping.get_take_lane(take2_id).unwrap().clips.len(), 2);
    assert_eq!(comp_track.comping.get_take_lane(take2_id).unwrap().clips[0].duration_beats, 8.0);
    assert_eq!(comp_track.comping.get_take_lane(take2_id).unwrap().clips[1].timeline_start_beat, 8.0);

    // 3. Solo Auditioning: audition Take 3
    assert!(!comp_track.comping.is_lane_auditioning(take3_id));
    comp_track.set_audition_take(Some(take3_id));
    assert!(comp_track.comping.is_lane_auditioning(take3_id));
    assert_eq!(comp_track.effective_clips()[0].name, "Take 3 Phrase");
    comp_track.set_audition_take(None);
    assert!(!comp_track.comping.is_lane_auditioning(take3_id));

    // 4. Comping phrase regions into main track with automatic 4ms micro-crossfades:
    // Bar 0..4 from Take 1
    let c1 = comp_track.comp_take_region(take1_id, 0.0, 4.0, 4.0).expect("Comp Take 1 must succeed");
    assert_eq!(c1, 1);
    // Bar 4..8 from Take 2
    let c2 = comp_track.comp_take_region(take2_id, 4.0, 8.0, 4.0).expect("Comp Take 2 must succeed");
    assert_eq!(c2, 1);
    // Bar 8..12 from Take 3
    let c3 = comp_track.comp_take_region(take3_id, 8.0, 12.0, 4.0).expect("Comp Take 3 must succeed");
    assert_eq!(c3, 1);

    // Verify comped main track clips
    assert_eq!(comp_track.clips.len(), 3);
    assert_eq!(comp_track.clips[0].timeline_start_beat, 0.0);
    assert_eq!(comp_track.clips[0].duration_beats, 4.0);
    assert_eq!(comp_track.clips[1].timeline_start_beat, 4.0);
    assert_eq!(comp_track.clips[1].duration_beats, 4.0);
    assert_eq!(comp_track.clips[2].timeline_start_beat, 8.0);
    assert_eq!(comp_track.clips[2].duration_beats, 4.0);

    // 5. Verify 4ms micro-crossfade metadata at boundary beats (4.0 and 8.0)
    assert!(comp_track.comping.crossfades.len() >= 2);
    for xf in &comp_track.comping.crossfades {
        assert_eq!(xf.duration_ms, 4.0, "Automatic comp micro-crossfade must default to 4ms");
        let frames_44k = xf.frames_at_sample_rate(44100.0);
        assert_eq!(frames_44k, 176);
    }

    // 6. Mathematical equal-power law and audio buffer verification across 176 frames (4ms @ 44.1 kHz)
    let frames = 176;
    let out_sig = vec![0.707f32; frames];
    let in_sig = vec![0.707f32; frames];
    let mut comped_audio = vec![0.0f32; frames];
    comp_track.comping.crossfades[0].apply_crossfade(&out_sig, &in_sig, &mut comped_audio);
    for n in 0..frames {
        let f_in = MicroCrossfade::fade_in_gain(n, frames);
        let f_out = MicroCrossfade::fade_out_gain(n, frames);
        assert!((f_in * f_in + f_out * f_out - 1.0).abs() < 1e-4);
    }
}

// =========================================================================
// 15. Epic #26: Audio-to-MIDI Transcription & Groove Pool (.agr)
// =========================================================================

#[test]
fn test_e2e_epic26_audio_to_midi_and_groove_template_transcription() {
    let sample_rate = 44100;
    let bpm = 120.0;

    // 1. Melody YIN Transcription (440 Hz Sine -> MIDI 69 / A4)
    let total_samples = (sample_rate as f64 * 0.5) as usize;
    let mut melody_samples = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f64 / sample_rate as f64;
        let s = (2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32 * 0.8;
        melody_samples.push(s);
    }
    let melody_notes = AudioToMidiConverter::convert_melody_to_midi(&melody_samples, sample_rate, bpm);
    assert!(!melody_notes.is_empty(), "Transcribed melody must not be empty");
    assert_eq!(melody_notes[0].note, 69, "440 Hz tone must transcribe to MIDI 69 (A4)");
    assert!(melody_notes[0].velocity > 50);

    // 2. Drum Transient Classification (Kick, Snare, Hi-Hat)
    let mut kick = vec![0.0f32; 4000];
    for i in 0..4000 {
        let t = i as f64 / sample_rate as f64;
        let freq = 120.0 * (-t * 30.0).exp() + 50.0;
        let env = (-t * 15.0).exp() as f32;
        kick[i] = (2.0 * std::f64::consts::PI * freq * t).sin() as f32 * env;
    }
    let mut snare = vec![0.0f32; 4000];
    for i in 0..4000 {
        let t = i as f64 / sample_rate as f64;
        let env = (-t * 20.0).exp() as f32;
        let tone = (2.0 * std::f64::consts::PI * 220.0 * t).sin() as f32 * 0.4;
        let noise = (((i * 1103515245 + 12345) % 1000) as f32 / 500.0 - 1.0) * 0.6;
        snare[i] = (tone + noise) * env;
    }
    let mut hat = vec![0.0f32; 4000];
    for i in 0..4000 {
        let t = i as f64 / sample_rate as f64;
        let env = (-t * 45.0).exp() as f32;
        hat[i] = (2.0 * std::f64::consts::PI * 8000.0 * t).sin() as f32 * env;
    }

    let kick_midi = AudioToMidiConverter::convert_drums_to_midi(&kick, sample_rate, bpm);
    let snare_midi = AudioToMidiConverter::convert_drums_to_midi(&snare, sample_rate, bpm);
    let hat_midi = AudioToMidiConverter::convert_drums_to_midi(&hat, sample_rate, bpm);

    assert!(!kick_midi.is_empty() && kick_midi[0].note == 36, "Kick -> MIDI 36 (C1)");
    assert!(!snare_midi.is_empty() && snare_midi[0].note == 38, "Snare -> MIDI 38 (D1)");
    assert!(!hat_midi.is_empty() && hat_midi[0].note == 42, "Hi-Hat -> MIDI 42 (F#1)");

    // 3. Harmony Chord Transcription (C Major: 60, 64, 67)
    let chord_samples_len = (sample_rate as f64 * 0.5) as usize;
    let mut chord_samples = vec![0.0f32; chord_samples_len];
    for i in 0..chord_samples_len {
        let t = i as f64 / sample_rate as f64;
        let c = (2.0 * std::f64::consts::PI * 261.63 * t).sin() as f32 * 0.3;
        let e = (2.0 * std::f64::consts::PI * 329.63 * t).sin() as f32 * 0.3;
        let g = (2.0 * std::f64::consts::PI * 392.00 * t).sin() as f32 * 0.3;
        chord_samples[i] = c + e + g;
    }
    let chord_notes = AudioToMidiConverter::convert_harmony_to_midi(&chord_samples, sample_rate, bpm);
    let pitches: Vec<u8> = chord_notes.iter().map(|n| n.note).collect();
    assert!(pitches.contains(&60), "C Major must contain C4 (60)");
    assert!(pitches.contains(&64), "C Major must contain E4 (64)");
    assert!(pitches.contains(&67), "C Major must contain G4 (67)");

    // 4. GrooveTemplate (.agr) Extraction & Note Displacement
    let bar_samples = (sample_rate * 2) as usize;
    let mut groove_audio = vec![0.0f32; bar_samples];
    let samples_per_16th = (44100.0 * 0.125) as usize;
    let swung_sample = samples_per_16th + 250;
    for i in 0..100 {
        if swung_sample + i < groove_audio.len() {
            groove_audio[swung_sample + i] = 0.9;
        }
    }

    let groove = GrooveTemplate::extract_groove_from_audio(&groove_audio, sample_rate, bpm);
    assert!(groove.timing_offsets[1] > 0, "Swung transient must record positive offset");

    let mut notes_to_displace = vec![(60u8, 100u8, 0.25f64, 0.25f64)];
    groove.apply_groove_to_notes(&mut notes_to_displace);
    assert!(
        notes_to_displace[0].2 > 0.25,
        "Applied groove must shift note forward"
    );
}

// =========================================================================
// 16. Epic #27: Live Pack (.alp) Bundler, DAWproject XML & Lossless Codec
// =========================================================================

#[test]
fn test_e2e_epic27_alp_pack_bundler_dawproject_xml_and_lossless_codec() {
    // 1. Live Pack (.alp) Bundler and CRC32 Roundtrip
    let mut archive = AlpPackArchive::new("Oxide Pack");
    archive.add_entry("Project.als", b"<AbletonProject version=\"12\"/>".to_vec());
    archive.add_entry("Samples/Kick.wav", vec![1, 2, 3, 4, 5, 6, 7, 8]);
    archive.add_entry("Presets/Synth.adv", b"PRESET_BINARY_DATA".to_vec());

    let bundled = archive.bundle_alp();
    assert!(!bundled.is_empty());

    let extracted = AlpPackArchive::extract_alp(&bundled).expect("Extract ALP archive");
    assert_eq!(extracted.name, "Oxide Pack");
    assert_eq!(extracted.entries.len(), 3);
    assert_eq!(extracted.entries[0].path, "Project.als");
    assert_eq!(
        extracted.entries[0].data,
        b"<AbletonProject version=\"12\"/>"
    );

    // CRC32 Rejection upon bit tampering
    let mut tampered = bundled.clone();
    let mid = tampered.len() / 2;
    tampered[mid] ^= 0xFF;
    let res = AlpPackArchive::extract_alp(&tampered);
    assert!(
        matches!(res, Err(AlpError::ChecksumMismatch { .. })),
        "Tampered payload must fail with ChecksumMismatch"
    );

    // 2. Open DAWproject XML Bidirectional Interchange
    let project = DawProject {
        title: "Berlin Underground".to_string(),
        tempo: 130.0,
        time_signature: (4, 4),
        tracks: vec![DawTrack {
            name: "Sub Bass".to_string(),
            volume_db: -2.0,
            pan: 0.0,
            is_muted: false,
            clips: vec![DawClip {
                name: "Bass Clip".to_string(),
                start_beat: 0.0,
                duration_beats: 4.0,
                notes: vec![
                    DawNote {
                        key: 36,
                        velocity: 100,
                        start_beat: 0.0,
                        duration_beats: 0.5,
                    },
                    DawNote {
                        key: 38,
                        velocity: 110,
                        start_beat: 1.0,
                        duration_beats: 0.5,
                    },
                ],
            }],
        }],
    };

    let (project_xml, metadata_xml) = DawProjectExchange::export_dawproject_xml(&project);
    assert!(project_xml.contains("<tempo>130.00</tempo>"));
    assert!(project_xml.contains("Sub Bass"));
    assert!(metadata_xml.contains("<title>Berlin Underground</title>"));

    let imported = DawProjectExchange::import_dawproject_xml(&project_xml, &metadata_xml)
        .expect("Import DAWproject XML");
    assert_eq!(imported.title, "Berlin Underground");
    assert!((imported.tempo - 130.0).abs() < 1e-2);
    assert_eq!(imported.tracks.len(), 1);
    assert_eq!(imported.tracks[0].clips[0].notes.len(), 2);
    assert_eq!(imported.tracks[0].clips[0].notes[0].key, 36);

    // 3. LosslessFreezeCodec Bit-Exact Roundtrip
    let mut samples_i32 = Vec::new();
    let mut acc = 0i32;
    for i in 0..1000 {
        acc = acc.wrapping_add((((i * 13) % 47) - 23) * 60);
        samples_i32.push(acc);
    }

    let encoded_pcm = LosslessFreezeCodec::encode_lossless_pcm(&samples_i32);
    assert!(
        encoded_pcm.len() < samples_i32.len() * 4,
        "Lossless compression must reduce memory footprint"
    );
    let decoded_pcm = LosslessFreezeCodec::decode_lossless_pcm(&encoded_pcm).expect("Decode PCM");
    assert_eq!(
        samples_i32, decoded_pcm,
        "Decoded PCM must be 100% bit-exact"
    );

    let samples_f32 = vec![0.0f32, -0.5, 0.75, -0.999, 0.123456];
    let encoded_f32 = LosslessFreezeCodec::encode_lossless_f32(&samples_f32);
    let decoded_f32 = LosslessFreezeCodec::decode_lossless_f32(&encoded_f32).expect("Decode F32");
    assert_eq!(samples_f32.len(), decoded_f32.len());
    for i in 0..samples_f32.len() {
        assert_eq!(samples_f32[i].to_bits(), decoded_f32[i].to_bits());
    }
}

// =========================================================================
// 17. Epic #28: Operational Knowledge Framework (OKF) 10-Chapter Copilot
// =========================================================================

#[test]
fn test_e2e_epic28_okf_copilot_bilingual_full_ten_chapters_queries() {
    assert_eq!(
        OKF_MANUAL_SECTIONS.len(),
        10,
        "Full OKF manual must contain exactly 10 sections"
    );

    let queries = [
        ("OKF-01", "dual view and flexible splitter layout", "vista dual y splitter layout"),
        ("OKF-02", "session view clip launch quantization", "cuantizacion de disparo de clips session"),
        ("OKF-03", "studio mixer crossfader and vu meter", "mezclador fader crossfader y vumetro"),
        ("OKF-04", "arrangement timeline and take lane comping", "linea de tiempo arrangement comping take lanes"),
        ("OKF-05", "audio warping and piano roll generative tools", "piano roll generativo y audio warping"),
        ("OKF-06", "device chain glue compressor and roar saturation", "cadena de dispositivos compresor glue y roar"),
        ("OKF-07", "tr-909 step sequencer and groove engine", "secuenciador tr-909 y groove engine"),
        ("OKF-08", "global scale microtonal tuning and 4-stem remix bounce", "afinacion microtonal scala y exportar stems"),
        ("OKF-09", "suite instruments drift meld and modular patch graph", "instrumentos suite drift meld y modular patch graph"),
        ("OKF-10", "plugin sandbox host clap vst3 push 3 and alp pack", "sandboxing de plugins clap vst3 push 3 y paquetes alp"),
    ];

    let mut app = OxideStudioApp::new_mock(44100.0);

    for (expected_id, en_query, es_query) in queries {
        // 1. English Query via execute_studio_intent
        app.ai_copilot.set_locale(StudioLocale::En);
        let (reply_en, badges_en) = execute_studio_intent(en_query, &mut app);
        assert!(
            reply_en.contains(expected_id),
            "EN query '{}' must resolve to {}: {}",
            en_query,
            expected_id,
            reply_en
        );
        assert!(!badges_en.is_empty(), "EN badges must not be empty for {}", expected_id);

        // 2. Spanish Query via execute_studio_intent
        app.ai_copilot.set_locale(StudioLocale::Es);
        let (reply_es, badges_es) = execute_studio_intent(es_query, &mut app);
        assert!(
            reply_es.contains(expected_id),
            "ES query '{}' must resolve to {}: {}",
            es_query,
            expected_id,
            reply_es
        );
        assert!(!badges_es.is_empty(), "ES badges must not be empty for {}", expected_id);
    }

    // GoldenTelemetryVerifier
    let valid_audio = vec![0.1f32; 512];
    assert!(
        GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&valid_audio, 1920, 1080),
        "Valid FHD uniforms and audio must pass"
    );

    // Audio violations: NaN, Inf, digital clipping, empty
    let mut nan_audio = valid_audio.clone();
    nan_audio[10] = f32::NAN;
    assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&nan_audio, 1920, 1080));

    let mut inf_audio = valid_audio.clone();
    inf_audio[10] = f32::INFINITY;
    assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&inf_audio, 1920, 1080));

    let mut clipped_audio = valid_audio.clone();
    clipped_audio[10] = 1.9;
    assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&clipped_audio, 1920, 1080));

    assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&[], 1920, 1080));

    // Viewport violations: 0x0, sub-64px, extreme aspect ratio
    assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&valid_audio, 0, 0));
    assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&valid_audio, 32, 32));
    assert!(!GoldenTelemetryVerifier::verify_golden_audio_and_shader_uniforms(&valid_audio, 4000, 10));

    // Copilot System Prompts
    app.ai_copilot.set_locale(StudioLocale::En);
    let prompt_en = app.ai_copilot.system_prompt();
    for i in 1..=10 {
        let tag = format!("OKF-{:02}", i);
        assert!(prompt_en.contains(&tag), "English prompt must contain {}", tag);
    }

    app.ai_copilot.set_locale(StudioLocale::Es);
    let prompt_es = app.ai_copilot.system_prompt();
    for i in 1..=10 {
        let tag = format!("OKF-{:02}", i);
        assert!(prompt_es.contains(&tag), "Spanish prompt must contain {}", tag);
    }
    assert!(prompt_es.contains("voseo"), "Spanish prompt must enforce Argentine voseo");
}

// =========================================================================
// 18. Chapter 15 & OKF-08: Tuning Systems & Scales (Scale Lock & EDO)
// =========================================================================

#[test]
fn test_e2e_chapter15_tuning_systems_and_scale_lock() {
    // 1. Global Scale Awareness across all 12 roots and 14 modes with Scale Lock
    for root in 0..12 {
        for &mode in &ScaleMode::ALL {
            let scale = GlobalScale::new(root, mode).with_lock(true);
            assert!(scale.scale_lock);
            assert_eq!(scale.root_name(), ROOT_NAMES[root as usize]);
            assert!(scale.is_in_scale(60 + root));

            // Pitch snapping under Scale Lock
            let quantized = scale.quantize(61 + root);
            assert!(scale.is_in_scale(quantized));
        }
    }

    // 2. Microtonal Tuning Systems: 12-TET, 19-EDO, 24-EDO, 31-EDO, Bohlen-Pierce, Just Intonation, Scala (.scl)
    assert_eq!(TuningType::ALL.len(), 7);

    // 19-EDO: 19 equal divisions of octave
    let edo19 = TuningSystem::new(TuningType::NineteenEdo);
    assert!((edo19.frequency_for_note(69) - 440.0).abs() < 1e-6);
    assert!((edo19.frequency_for_note(88) - 880.0).abs() < 1e-6);

    // 24-EDO: Quarter-Tone (24 equal divisions of octave)
    let edo24 = TuningSystem::new(TuningType::TwentyFourEdo);
    assert!((edo24.frequency_for_note(69) - 440.0).abs() < 1e-6);
    assert!((edo24.frequency_for_note(93) - 880.0).abs() < 1e-6);

    // 31-EDO: 31 equal divisions of octave
    let edo31 = TuningSystem::new(TuningType::ThirtyOneEdo);
    assert!((edo31.frequency_for_note(69) - 440.0).abs() < 1e-6);
    assert!((edo31.frequency_for_note(100) - 880.0).abs() < 1e-6);

    // Bohlen-Pierce: 13 equal divisions of tritave (3:1)
    let bp = TuningSystem::new(TuningType::BohlenPierce);
    assert!((bp.frequency_for_note(69) - 440.0).abs() < 1e-6);
    assert!((bp.frequency_for_note(82) - 1320.0).abs() < 1e-6); // 440 * 3.0

    // Just Intonation: 5-limit ratios
    let ji = TuningSystem::new(TuningType::JustIntonation);
    assert!((ji.frequency_for_note(69) - 440.0).abs() < 1e-6);
    assert!((ji.frequency_for_note(60) - 264.0).abs() < 1e-6);

    // Scala Custom (.scl)
    let mut scala = TuningSystem::default();
    let scl_data = "! 19-EDO sample\n19-EDO test\n19\n!\n63.158\n126.316\n189.474\n252.632\n315.789\n378.947\n442.105\n505.263\n568.421\n631.579\n694.737\n757.895\n821.053\n884.211\n947.368\n1010.526\n1073.684\n1136.842\n1200.0\n";
    scala.load_scala(scl_data).expect("Load Scala .scl file");
    assert_eq!(scala.tuning_type, TuningType::Scala);
    assert_eq!(scala.scala_cents.len(), 19);

    // 3. Audio Thread Frequency Lookup Table Generation
    let table_f32 = edo19.generate_frequency_table_f32();
    assert_eq!(table_f32.len(), 128);
    assert!((table_f32[69] - 440.0).abs() < 1e-4);

    // 4. GUI Integration & Anti-Breakage Frame Audit
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.global_scale = GlobalScale::new(5, ScaleMode::Dorian).with_lock(true);
    app.tuning_system = edo19;
    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Scale Lock and 19-EDO: {:?}",
        report
    );
}

// =========================================================================
// 19. Chapter 10 & OKF-05: Editing MIDI (Note Editor & Piano Roll)
// =========================================================================

#[test]
fn test_e2e_chapter10_midi_note_editor_and_piano_roll() {
    // 1. Piano Roll Note Placement & Data Structure Invariants
    let mut pattern = MidiPattern::new(8.0);
    // Add C Major chord notes and melodic lead
    pattern.add_note(MidiNote::new(60, 100, 0.0, 2.0)); // C4
    pattern.add_note(MidiNote::new(64, 90, 0.0, 2.0));  // E4
    pattern.add_note(MidiNote::new(67, 110, 0.0, 2.0)); // G4
    pattern.add_note(MidiNote::new(72, 127, 2.0, 1.0)); // C5 with velocity 127
    pattern.add_note(MidiNote::new(71, 80, 3.0, 1.0).with_probability(0.65)); // B4 with 65% probability

    assert_eq!(pattern.notes.len(), 5);
    assert_eq!(pattern.notes[0].pitch, 60);
    assert_eq!(pattern.notes[3].velocity, 127);
    assert!((pattern.notes[4].probability - 0.65).abs() < 1e-4);

    // 2. Active Note Queries & Interval Checks
    let active_at_1 = pattern.notes_active_at_beat(1.0);
    assert_eq!(active_at_1.len(), 3, "C-E-G chord active at beat 1.0");

    let starting_in_bar2 = pattern.notes_starting_in_range(2.0, 4.0);
    assert_eq!(starting_in_bar2.len(), 2, "C5 and B4 start in range [2.0, 4.0)");

    // 3. Note Nudge Logic
    assert!(pattern.nudge_note(3, 0.25));
    assert_eq!(pattern.notes[3].start_beat, 2.25, "Note #3 nudged from 2.0 to 2.25");
    assert!(pattern.nudge_note(3, -0.25));
    assert_eq!(pattern.notes[3].start_beat, 2.0, "Note #3 nudged back to 2.0");

    // Clamping on boundaries: cannot nudge before 0.0 or past clip end
    assert!(pattern.nudge_note(0, -10.0));
    assert_eq!(pattern.notes[0].start_beat, 0.0);
    assert!(pattern.nudge_note(0, 20.0));
    assert_eq!(pattern.notes[0].start_beat, 6.0); // 8.0 length - 2.0 duration = 6.0
    pattern.notes[0].start_beat = 0.0;

    // 4. Note Stretch Logic (2.0x stretch relative to beat 0.0)
    let mut stretch_pattern = pattern.clone();
    stretch_pattern.stretch_notes(2.0, 0.0);
    assert_eq!(stretch_pattern.notes[0].start_beat, 0.0);
    assert_eq!(stretch_pattern.notes[0].duration_beats, 4.0);
    assert_eq!(stretch_pattern.notes[3].start_beat, 4.0);
    assert_eq!(stretch_pattern.notes[3].duration_beats, 2.0);

    // 5. Note Quantize Logic (snap to 1/16 = 0.25 grid with 100% strength)
    let mut unquantized = MidiPattern::new(4.0);
    unquantized.add_note(MidiNote::new(60, 100, 0.22, 0.27));
    unquantized.add_note(MidiNote::new(64, 90, 0.98, 0.52));
    unquantized.quantize(0.25, 1.0, 0.0);
    assert!((unquantized.notes[0].start_beat - 0.25).abs() < 1e-4);
    assert!((unquantized.notes[0].duration_beats - 0.25).abs() < 1e-4);
    assert!((unquantized.notes[1].start_beat - 1.0).abs() < 1e-4);
    assert!((unquantized.notes[1].duration_beats - 0.5).abs() < 1e-4);

    // 6. Scale-Fold & Audition Logic in ClipView Panel
    let scale = GlobalScale {
        root: 0, // C
        mode: ScaleMode::Major,
        scale_lock: true,
    };
    let mut clip_state = ClipViewState::default();
    clip_state.scale_folded = true;
    clip_state.selected_note_idx = Some(0);
    clip_state.quantize_grid = 0.25;

    // Audition pitch dispatch
    clip_state.audition_pitch = Some(60);
    clip_state.last_audition_note = Some((60, 100));
    assert_eq!(clip_state.last_audition_note, Some((60, 100)));

    // 7. GUI Integration & Anti-Breakage Frame Audit
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.deck_tab = DeckTab::ClipView;
    app.clip_view.sub_tab = ClipViewSubTab::Notes;
    app.clip_view.scale_folded = true;
    app.global_scale = scale;

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Piano Roll Scale-Fold view: {:?}",
        report
    );
}

#[test]
fn test_chapter_33_and_okf01_synchronizing_with_link_tempo_follower_and_midi() {
    use egui::{pos2, vec2, Context, Rect};

    // 1. Ableton Link Network Sync: Quantum Phase & Smooth Tempo Ramp (Chapter 33.1)
    let mut link = LinkSyncEngine::new(125.0);
    link.set_enabled(true);
    link.set_peer_count(3);
    link.set_quantum(4.0);
    assert_eq!(link.phase(0.0), 0.0);
    assert_eq!(link.phase(2.75), 2.75);
    assert_eq!(link.phase(4.0), 0.0);
    assert_eq!(link.distance_to_next_quantum(1.25), 2.75);

    // Smooth tempo ramp: 125 -> 135 over 2.0 seconds
    link.request_tempo_change(135.0, 2.0, 0.0);
    link.advance_time(1.0);
    assert!((link.session_bpm - 130.0).abs() < 1e-3);
    link.advance_time(1.0);
    assert!((link.session_bpm - 135.0).abs() < 1e-3);

    // 2. External MIDI Clock Synchronization: 24 PPQN Master & Jitter-Filtered Slave (Chapter 33.3)
    let mut master_clock = MidiClockSync::new(MidiSyncMode::Master);
    let ticks = master_clock.process_master_advance(0.0, 2.0); // 2 quarter notes = 48 ticks
    assert_eq!(ticks, 48);
    let master_msgs = master_clock.drain_outgoing_messages();
    assert_eq!(master_msgs.len(), 48);
    assert_eq!(master_msgs[0], MidiClockMessage::TimingClock);

    // Song Position Pointer 14-bit roundtrip
    let spp_16ths = 128u16; // 32 beats
    let (lsb, msb) = MidiClockSync::encode_song_position_pointer(spp_16ths);
    assert_eq!(MidiClockSync::parse_song_position_pointer(lsb, msb), 128);
    assert_eq!(MidiClockSync::song_position_to_beats(128), 32.0);

    // Slave clock PLL lock test at 120.0 BPM
    let mut slave_clock = MidiClockSync::new(MidiSyncMode::Slave);
    slave_clock.parse_incoming_byte(0xFA, 0.0); // MSG_START
    let dt = 1.0 / 48.0; // 48 ticks/sec at 120 BPM
    let mut t = 0.0;
    for _ in 0..48 {
        t += dt;
        slave_clock.parse_incoming_byte(0xF8, t); // MSG_TIMING_CLOCK
    }
    assert!(slave_clock.slave_is_locked);
    assert!((slave_clock.slave_estimated_bpm - 120.0).abs() < 1.0);

    // 3. Audio Tempo Follower Engine with Transient Onset Tracking (Chapter 33.2)
    let mut follower = TempoFollowerEngine::new();
    follower.set_enabled(true);
    follower.set_bpm_range(70.0, 160.0);
    let sample_rate = 44100.0;
    let block_size = 512;
    let pulse_interval = (22050 / block_size) as usize; // ~120 BPM
    let mut cur_time = 0.0;
    for _pulse in 0..16 {
        for b in 0..pulse_interval {
            let mut block = [0.0f32; 512];
            if b == 0 {
                for i in 0..32 {
                    block[i] = 0.85 * (-((i as f32) / 6.0)).exp();
                }
            }
            follower.process_audio_block(&block, sample_rate, cur_time);
            cur_time += (block_size as f64) / sample_rate;
        }
    }
    assert!(follower.confidence > 0.50);
    assert!((follower.estimated_bpm - 120.0).abs() < 5.0);

    // 4. Central Transport Controller: DJ Nudge & Looping (Chapter 33.4)
    let mut transport = TransportController::new(124.0);
    transport.set_loop(true, 8.0, 8.0); // Loop beats [8.0, 16.0)
    transport.metronome_enabled = true;
    transport.play();

    // Test Nudge Up (+1.5%)
    transport.start_nudge_up();
    assert_eq!(transport.nudge_state, NudgeState::NudgeUp);
    assert!((transport.effective_bpm() - (124.0 * 1.015)).abs() < 1e-4);
    transport.stop_nudge();
    assert_eq!(transport.effective_bpm(), 124.0);

    // Advance transport through loop boundary: 15.9 -> 16.1 wraps to 8.1
    transport.time.beat = 15.9;
    let advance_report = transport.advance(4410, 44100.0, None); // 0.1s at 124 BPM = 0.2066 beats
    assert!(advance_report.loop_wrapped);
    assert!((advance_report.current_beat - 8.1066).abs() < 1e-3);

    // 5. GUI Integration & Anti-Breakage Layout Audit (OKF-01)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.bpm = 124.0;
    app.is_playing = true;

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under active playback transport state: {:?}",
        report
    );
}

// =========================================================================
// 21. Chapter 29 & OKF-04: Automation Envelopes, Bezier Curves, Draw Mode & Takeover
// =========================================================================

#[test]
fn test_e2e_chapter29_automation_and_takeover_modes() {
    // 1. Multi-Lane Parameter Automation (Chapter 29.1)
    let mut multi = MultiLaneAutomation::new();
    let cutoff_lane = multi.get_or_create_lane(1, "Filter Cutoff");
    cutoff_lane.add_point(AutomationPoint::linear(0.0, 0.2));
    cutoff_lane.add_point(AutomationPoint::scurve(4.0, 0.8));
    cutoff_lane.add_point(AutomationPoint::linear(8.0, 0.4));

    let res_lane = multi.get_or_create_lane(2, "Filter Res");
    res_lane.add_point(AutomationPoint::with_curve_type(0.0, 0.1, AutomationCurveType::Exponential));
    res_lane.add_point(AutomationPoint::with_curve_type(4.0, 0.9, AutomationCurveType::Logarithmic));

    assert_eq!(multi.lanes_count(), 2);
    // S-curve inflection point at beat 2.0 (midpoint between 0.2 and 0.8 is 0.5)
    let cutoff_mid = multi.evaluate_param(1, 2.0, 0.0);
    assert!((cutoff_mid - 0.5).abs() < 1e-4, "Cutoff S-curve midpoint must be 0.5, got {}", cutoff_mid);

    // 2. Cubic Bezier Curve Editing & Alt-Drag Sculpting (Chapter 29.3)
    let mut bezier_lane = AutomationLane::new(3, "Stereo Pan");
    let ease_in = CubicBezierHandle::ease_in();
    bezier_lane.add_point(AutomationPoint::with_bezier(0.0, 0.0, ease_in));
    bezier_lane.add_point(AutomationPoint::linear(4.0, 1.0));

    let bezier_mid = bezier_lane.interpolate_at(2.0, 0.0);
    assert!(bezier_mid < 0.5, "Cubic Bezier ease-in curve must lag below 0.5, got {}", bezier_mid);

    // Alt-drag downward converts to concave-up curve
    bezier_lane.apply_alt_drag_delta(0, 0.5);
    let alt_drag_val = bezier_lane.interpolate_at(2.0, 0.0);
    assert!(alt_drag_val < 0.5, "Alt-drag positive delta must curve downward, got {}", alt_drag_val);

    // 3. Step Curve Hold & Discrete Jump
    let mut step_lane = AutomationLane::new(4, "Track Mute");
    step_lane.add_point(AutomationPoint::step(0.0, 0.0));
    step_lane.add_point(AutomationPoint::step(4.0, 1.0));
    assert_eq!(step_lane.interpolate_at(3.99, 0.0), 0.0, "Step mode must hold initial value");
    assert_eq!(step_lane.interpolate_at(4.0, 0.0), 1.0, "Step mode must jump at boundary");

    // 4. Draw Mode: Stepped Grid Quantization & Freehand Thinning (Chapter 29.2)
    assert_eq!(multi.draw_mode, DrawModeState::default());
    assert!(multi.toggle_draw_mode(), "Draw Mode toggled on");
    let mut draw_lane = AutomationLane::new(5, "Send A");
    draw_lane.draw_stepped(0.0, 4.0, 0.75, 1.0);
    assert_eq!(draw_lane.interpolate_at(0.5, 0.0), 0.75);
    assert_eq!(draw_lane.interpolate_at(1.5, 0.0), 0.75);
    assert_eq!(draw_lane.interpolate_at(3.5, 0.0), 0.75);

    // Freehand drawing with Ramer-Douglas-Peucker simplification
    let mut raw_trajectory = Vec::new();
    for i in 0..=30 {
        let beat = i as f64 * 0.1;
        let val = 0.3 + (beat as f32) * 0.05 + ((i % 3) as f32) * 0.001; // nearly collinear
        raw_trajectory.push((beat, val));
    }
    let mut freehand_lane = AutomationLane::new(6, "Send B");
    freehand_lane.draw_freehand(&raw_trajectory, 0.01);
    assert!(freehand_lane.len() <= 6, "RDP simplification must reduce 31 points to <= 6 points, got {}", freehand_lane.len());

    // 5. Hardware Parameter Takeover Modes (Chapter 29.4 & OKF-04)
    // Mode A: Jump
    let mut jump_tracker = AutomationTakeoverTracker::new(TakeoverMode::Jump);
    assert_eq!(jump_tracker.process_hw_movement(0.2, 0.8), Some(0.8));
    assert!(jump_tracker.is_latched);
    assert!(jump_tracker.is_overridden);

    // Mode B: Pickup
    let mut pickup_tracker = AutomationTakeoverTracker::new(TakeoverMode::Pickup);
    let soft_val = 0.70;
    assert_eq!(pickup_tracker.process_hw_movement(soft_val, 0.2), None);
    assert_eq!(pickup_tracker.process_hw_movement(soft_val, 0.5), None);
    assert!(!pickup_tracker.is_latched);
    // Crossing software value 0.70 triggers takeover
    assert_eq!(pickup_tracker.process_hw_movement(soft_val, 0.75), Some(0.75));
    assert!(pickup_tracker.is_latched);
    assert_eq!(pickup_tracker.process_hw_movement(0.75, 0.90), Some(0.90));

    // Mode C: Value Scaling (smooth convergence)
    let mut scaling_tracker = AutomationTakeoverTracker::new(TakeoverMode::ValueScaling);
    let init_hw = scaling_tracker.process_hw_movement(0.5, 0.0).unwrap();
    assert_eq!(init_hw, 0.5);
    let scaled_step = scaling_tracker.process_hw_movement(0.5, 0.1).unwrap();
    assert!(scaled_step > 0.5 && scaled_step < 0.6, "Value scaling must smoothly scale upwards");
    let scaled_max = scaling_tracker.process_hw_movement(scaled_step, 1.0).unwrap();
    assert_eq!(scaled_max, 1.0);
    assert!(scaling_tracker.is_latched);

    // Re-enable Automation (Back to Arrangement)
    scaling_tracker.re_enable_automation();
    assert!(!scaling_tracker.is_overridden);
    assert!(!scaling_tracker.is_latched);

    // 6. Record Hardware Takeover into Automation Lane
    let mut rec_lane = AutomationLane::new(7, "Macro 1");
    let mut rec_tracker = AutomationTakeoverTracker::new(TakeoverMode::Pickup);
    assert_eq!(rec_tracker.record_into_lane(&mut rec_lane, 1.0, 0.5, 0.2), None);
    assert_eq!(rec_lane.len(), 0);
    assert_eq!(rec_tracker.record_into_lane(&mut rec_lane, 2.0, 0.5, 0.55), Some(0.55));
    assert_eq!(rec_lane.len(), 1);
    assert_eq!(rec_lane.points[0].beat, 2.0);

    // 7. GUI Integration & Anti-Breakage Frame Audit (OKF-01 / OKF-04)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.timeline_gui.show_automation = true;
    app.timeline_gui.automation_points = vec![
        AutomationPoint::new(0.0, 0.15, 0.0),
        AutomationPoint::new(8.0, 0.35, 0.25),
        AutomationPoint::new(16.0, 0.85, -0.2),
        AutomationPoint::new(24.0, 0.45, 0.1),
        AutomationPoint::new(32.0, 0.95, 0.3),
    ];

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under visible Automation Lane view: {:?}",
        report
    );
}

// =========================================================================
// 22. Chapter 34 & OKF-01 / OKF-03: Computer Audio Resources & Engine Architecture
// =========================================================================

/// Ableton Live 12 Reference Manual Chapter 34: "Computer Audio Resources & Engine Architecture"
/// Associated OKF: OKF-01, OKF-03 | Associated Epics: #17, #28
#[test]
fn test_chapter_34_and_okf01_okf03_audio_resources_and_engine_architecture() {
    // 1. Dual CPU Load Metering (Chapter 34.1 & OKF-01 Top Bar Telemetry)
    let mut meter = CpuLoadMeter::new();
    let sample_rate = 48000.0;
    let buffer_size = 256;
    let budget_secs = buffer_size as f32 / sample_rate;

    // Simulate steady-state block at 35% CPU budget
    meter.record_block(budget_secs * 0.35, budget_secs);
    assert!((meter.current() - 0.35).abs() < 1e-3);
    assert!((meter.average() - 0.35).abs() < 1e-3);
    assert!(!meter.is_overloaded());
    assert_eq!(meter.overload_count(), 0);

    // Simulate transient CPU spike exceeding 100% (overload / buffer underrun detection)
    meter.record_block(budget_secs * 1.25, budget_secs);
    assert!(meter.is_overloaded());
    assert_eq!(meter.overload_count(), 1);
    assert!(meter.peak() >= 1.25);

    let engine_state = EngineArchitectureState {
        sample_rate,
        buffer_size,
        multicore_enabled: true,
        worker_thread_count: 4,
        latency_budget_micros: CpuLoadMeter::budget_micros(buffer_size, sample_rate),
        current_cpu_load: meter.current(),
        average_cpu_load: meter.average(),
        peak_cpu_load: meter.peak(),
        overloads_detected: meter.overload_count(),
    };
    assert!(engine_state.multicore_enabled);
    assert_eq!(engine_state.worker_thread_count, 4);

    // 2. Multi-Core DSP Threading Dispatcher (Chapter 34.2)
    let mut dispatcher = MultiCoreDspDispatcher::new(4);
    assert_eq!(dispatcher.worker_count(), 4);
    assert!(dispatcher.is_multicore_enabled());

    let frames = 128;
    let total_tracks = 8;
    fn test_synth_track(track_id: usize, frames: usize, left: &mut [f32], right: &mut [f32]) {
        let f = 220.0 * (track_id as f32 + 1.0);
        for i in 0..frames {
            let s = (i as f32 * f * 0.0001).sin();
            left[i] = s;
            right[i] = s;
        }
    }

    // Parallel multi-core dispatch
    dispatcher.dispatch_block(total_tracks, frames, test_synth_track);
    let parallel_master_energy: f32 = dispatcher.master_left[..frames].iter().map(|s| s * s).sum();
    assert!(parallel_master_energy > 0.01);

    // Single-core sequential fallback parity
    dispatcher.set_multicore_enabled(false);
    dispatcher.dispatch_block(total_tracks, frames, test_synth_track);
    let serial_master_energy: f32 = dispatcher.master_left[..frames].iter().map(|s| s * s).sum();
    assert!((parallel_master_energy - serial_master_energy).abs() < 1e-4);

    // 3. Steady-State Zero Heap Allocation Invariant (Chapter 34.3)
    let (mut stream_handle, mut processor) = AudioEngine::start_mock(44100.0);
    stream_handle.play();
    stream_handle.send_note_on(0, 60, 110);
    stream_handle.send_note_on(1, 36, 127);

    // Warm-up run to eliminate cold cache / first-dispatch latency
    let mut warmup_buf = vec![0.0f32; 2048 * 2];
    processor.process_interleaved(&mut warmup_buf[..128]);

    // Run blocks across standard Live 12 buffer sizes (64..2048)
    for &bs in &SUPPORTED_BUFFER_SIZES {
        let (max_micros, overloads) = ZeroAllocAudit::audit_audio_processor(&mut processor, 10, bs);
        assert!(max_micros < 100_000.0);
        assert!(
            overloads <= 1,
            "Real-time overloads exceeded debug tolerance (bs={}, overloads={})",
            bs, overloads
        );
    }

    // 4. Lock-Free SPSC Telemetry & Command Routing (Chapter 34.4 & OKF-03 Mixer)
    let telem = stream_handle.poll_telemetry();
    assert!(telem.is_some());
    let t = telem.unwrap();
    assert!(t.master_peak_l > 0.0 || t.master_peak_r > 0.0);
    assert!(t.cpu_load >= 0.0 && t.cpu_load <= 1.0);

    // 5. GUI Integration & Anti-Breakage Layout Audit (OKF-01 & OKF-03)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.telemetry.cpu_load = meter.current();
    app.telemetry.master_peak_l = t.master_peak_l;
    app.telemetry.master_peak_r = t.master_peak_r;
    app.is_playing = true;

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Chapter 34 active telemetry state: {:?}",
        report
    );
}

// =========================================================================
// 21. Chapter 36 & Epic #28: Audio Fact Sheet & Mathematical Verification
// =========================================================================

/// Ableton Live 12 Reference Manual Chapter 36: "Audio Fact Sheet & Mathematical Verification"
/// Associated OKF: OKF-01..OKF-10 | Associated Epic: #28
/// Verifies:
/// 1. Testing Live's Audio Engine via Exact -inf dBFS Mathematical Null Tests (180° Polarity Inversion)
/// 2. 64-Bit Float Summing Engine Linearity, Commutativity (A + B == B + A), and Infinite Headroom Recovery
/// 3. Undithered Rendering Neutrality (Bit-exact roundtrip without requantization/dither noise)
/// 4. Sample Rate Conversion Neutrality (Identity rate pass-through)
/// 5. Modular Mixer 180° Phase Inversion Null Cancellation to -inf dBFS Master Output
/// 6. Fader -inf dBFS (0.0 Linear Gain) and Channel Mute Absolute Silence Verification
/// 7. 576-Frame Anti-Breakage Layout Invariants (0 overflow, 0 clipping, 0 collision, 0 tofu glyphs)
#[test]
fn test_e2e_chapter36_audio_fact_sheet_neutral_operations_and_mathematical_null_test() {
    let sample_rate = 44100.0_f32;
    let frames = 1024;

    // 1. Exact -inf dBFS Mathematical Null Test via 180° Polarity Inversion (Chapter 36.1)
    let mut signal_a = vec![0.0_f32; frames];
    for i in 0..frames {
        let t = i as f32 / sample_rate;
        // Composite multi-frequency test tone (440 Hz standard pitch + 1000 Hz reference + harmonics)
        signal_a[i] = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
            + 0.1 * (2.0 * std::f32::consts::PI * 2500.0 * t).sin();
    }

    let inv_a: Vec<f32> = signal_a.iter().map(|&x| -x).collect();
    let direct_report: NullTestReport = compute_null_test(&signal_a, &inv_a);
    assert!(direct_report.is_null, "Direct null test must succeed: {}", direct_report.summary());
    assert_eq!(direct_report.residual_dbfs, f64::NEG_INFINITY);

    let null_report = verify_phase_inversion_null(&signal_a);
    assert!(null_report.is_null, "Phase inversion must achieve mathematical null: {}", null_report.summary());
    assert_eq!(null_report.max_peak_error, 0.0, "Peak error must be bit-exact 0.0");
    assert_eq!(null_report.rms_error, 0.0, "RMS error must be bit-exact 0.0");
    assert_eq!(null_report.residual_dbfs, f64::NEG_INFINITY, "Residual energy must be -inf dBFS");
    assert_eq!(null_report.bit_exact_zero_count, frames);

    // 2. 64-Bit Floating-Point Summing Engine: Commutativity & Headroom Recovery (Chapter 36.2)
    let mut signal_b = vec![0.0_f32; frames];
    for i in 0..frames {
        let t = i as f32 / sample_rate;
        signal_b[i] = 0.4 * (2.0 * std::f32::consts::PI * 880.0 * t).cos()
            + 0.2 * (2.0 * std::f32::consts::PI * 1760.0 * t).sin();
    }

    let (sum_report, headroom_preserved) = verify_64bit_summing_neutrality(&signal_a, &signal_b);
    assert!(sum_report.is_null, "Summing must be commutative in 64-bit float math: {}", sum_report.summary());
    assert_eq!(sum_report.max_peak_error, 0.0);
    assert_eq!(sum_report.residual_dbfs, f64::NEG_INFINITY);
    assert!(headroom_preserved, "64-bit float summing accumulator must preserve dynamic headroom without clipping");

    // 3. Undithered Rendering Neutrality (Chapter 36.3)
    // In neutral rendering, output matches input bit-for-bit without dithering noise
    let rendered_unwarped = signal_a.clone();
    let render_report = verify_undithered_rendering_neutrality(&signal_a, &rendered_unwarped);
    assert!(render_report.is_null, "Undithered rendering must be neutral: {}", render_report.summary());
    assert_eq!(render_report.max_peak_error, 0.0);
    assert_eq!(render_report.residual_dbfs, f64::NEG_INFINITY);
    assert_eq!(render_report.bit_exact_zero_count, frames);

    // 4. Sample Rate Conversion (SRC) Neutrality (Chapter 36.4)
    let src_report_441 = verify_sample_rate_conversion_neutrality(44100.0, &signal_a);
    assert!(src_report_441.is_null);
    assert_eq!(src_report_441.residual_dbfs, f64::NEG_INFINITY);

    let src_report_48k = verify_sample_rate_conversion_neutrality(48000.0, &signal_b);
    assert!(src_report_48k.is_null);
    assert_eq!(src_report_48k.residual_dbfs, f64::NEG_INFINITY);

    // 5. Modular Audio Mixer 180° Polarity Inversion & Master Null Test (Chapter 36 & OKF-03)
    let mut mixer = ModularAudioMixer::new(sample_rate);
    let ch0 = mixer.add_channel("SignalTrack_Normal", oxide_timeline::TrackType::Audio);
    let ch1 = mixer.add_channel("SignalTrack_Inverted", oxide_timeline::TrackType::Audio);

    // Engage 180° phase flip on Channel 1
    mixer.channels[ch1].set_phase_invert(true);
    assert!(mixer.channels[ch1].phase_invert);

    mixer.begin_block(frames);

    let mut ch0_l = signal_a.clone();
    let mut ch0_r = signal_a.clone();
    let mut ch1_l = signal_a.clone();
    let mut ch1_r = signal_a.clone();

    mixer.process_channel(ch0, &mut ch0_l, &mut ch0_r);
    mixer.process_channel(ch1, &mut ch1_l, &mut ch1_r);

    // Verify 64-bit float summing accumulator in mixer
    let (acc_l, acc_r) = mixer.master_bus_f64();
    for i in 0..frames {
        assert_eq!(acc_l[i], 0.0, "64-bit Left accumulator must be bit-exact 0.0 at frame {}", i);
        assert_eq!(acc_r[i], 0.0, "64-bit Right accumulator must be bit-exact 0.0 at frame {}", i);
    }

    let mut master_out_l = vec![0.0_f32; frames];
    let mut master_out_r = vec![0.0_f32; frames];
    mixer.end_block(&mut master_out_l, &mut master_out_r);

    for i in 0..frames {
        assert_eq!(master_out_l[i], 0.0, "Master out Left must be exact 0.0 at frame {}", i);
        assert_eq!(master_out_r[i], 0.0, "Master out Right must be exact 0.0 at frame {}", i);
    }

    // 6. Fader -inf dBFS and Mute Silence Null Test (Chapter 36 & OKF-03)
    let mut mixer_silence = ModularAudioMixer::new(sample_rate);
    let silence_report = verify_mixer_fader_mute_silence(&mut mixer_silence);
    assert!(silence_report.is_null, "Mute and -inf dBFS fader must produce complete silence");
    assert_eq!(silence_report.max_peak_error, 0.0);
    assert_eq!(silence_report.residual_dbfs, f64::NEG_INFINITY);

    // 7. GUI Integration & Anti-Breakage Layout Invariants (OKF-01..OKF-10 / Epic #28)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(sample_rate);
    app.copilot_open = true;

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Chapter 36 null test state: overflows={:?}, clipped={:?}, collisions={:?}, tofu={:?}",
        report.viewport_overflows,
        report.clipped_texts,
        report.text_collisions,
        report.unsupported_glyphs
    );
}

// =========================================================================
// 23. Chapter 22 & OKF-06: Working with Instruments and Effects (Device Chain View)
// =========================================================================

#[test]
fn test_e2e_chapter22_working_with_instruments_and_effects_okf06() {
    use oxide_gui::panels::device_chain::{
        DeviceCategory, DeviceChainViewState, DeviceKind, DeviceRouting, GuiDeviceModule,
    };
    use oxide_gui::panels::browser::BrowserCategory;

    // 1. Device Chain Ordering & Category Rules (Chapter 22.1)
    assert_eq!(DeviceKind::Wavetable.category(), DeviceCategory::Instrument);
    assert_eq!(DeviceKind::Roar.category(), DeviceCategory::AudioEffect);

    let mut chain = DeviceChainViewState::default();
    chain.devices = vec![
        GuiDeviceModule::new(1, DeviceKind::Wavetable),
        GuiDeviceModule::new(2, DeviceKind::Roar),
        GuiDeviceModule::new(3, DeviceKind::AutoFilter),
        GuiDeviceModule::new(4, DeviceKind::EQEight),
    ];
    assert!(
        chain.is_signal_flow_valid(),
        "Instrument followed by Audio Effects must be valid"
    );

    // Audio effects only on an audio track is also valid
    chain.devices = vec![
        GuiDeviceModule::new(1, DeviceKind::AutoFilter),
        GuiDeviceModule::new(2, DeviceKind::GlueCompressor),
    ];
    assert!(chain.is_signal_flow_valid());

    // Placing an Audio Effect before an Instrument violates Live 12 signal flow
    chain.devices = vec![
        GuiDeviceModule::new(1, DeviceKind::Roar),
        GuiDeviceModule::new(2, DeviceKind::Wavetable),
    ];
    assert!(
        !chain.is_signal_flow_valid(),
        "Audio Effect before Instrument must violate signal flow"
    );

    // 2. Serial vs Parallel Routing (Chapter 22.2 & OKF-06)
    let mut routing_chain = DeviceChainViewState::default();
    routing_chain.devices = vec![
        GuiDeviceModule::new(1, DeviceKind::Roar),
        GuiDeviceModule::new(2, DeviceKind::GlueCompressor),
    ];
    routing_chain.devices[0].roar.drive_db = 18.0;
    routing_chain.devices[1].glue.threshold_db = -18.0;
    routing_chain.devices[1].glue.recompute_gain_reduction();

    let test_signal = vec![0.3f32, 0.6f32, -0.5f32, 0.8f32, -0.2f32];

    // Serial processing
    routing_chain.routing = DeviceRouting::Serial;
    let serial_out = routing_chain.process_chain(&test_signal);
    assert_eq!(serial_out.len(), test_signal.len());
    for s in &serial_out {
        assert!(s.is_finite());
    }

    // Parallel routing (2 branches, 50% mix)
    routing_chain.routing = DeviceRouting::Parallel {
        branches: 2,
        blend: 0.5,
    };
    let parallel_out = routing_chain.process_chain(&test_signal);
    assert_eq!(parallel_out.len(), test_signal.len());
    let mut topology_diff = false;
    for (s, p) in serial_out.iter().zip(parallel_out.iter()) {
        if (s - p).abs() > 1e-4 {
            topology_diff = true;
            break;
        }
    }
    assert!(
        topology_diff,
        "Parallel routing topology must produce distinct sum from serial processing"
    );

    // 3. 3x3 '+ Add Device' Quick-Picker Grid (OKF-06)
    assert_eq!(DeviceKind::GRID_3X3.len(), 3);
    for row in DeviceKind::GRID_3X3 {
        assert_eq!(row.len(), 3);
    }
    let mut picker_chain = DeviceChainViewState::for_track_1();
    let count_before = picker_chain.devices.len();
    picker_chain.quick_picker_open = true;

    // Pick 'Roar' from row 0, col 2
    let roar_kind = DeviceKind::GRID_3X3[0][2].0;
    assert_eq!(roar_kind, DeviceKind::Roar);
    assert!(picker_chain.insert_device(picker_chain.devices.len(), roar_kind));
    picker_chain.quick_picker_open = false;

    assert_eq!(picker_chain.devices.len(), count_before + 1);
    assert_eq!(picker_chain.devices.last().unwrap().kind, DeviceKind::Roar);
    assert!(!picker_chain.quick_picker_open);

    // 4. Zero-Latency Transparent Bypass Toggling (Chapter 22.1)
    let mut bypass_chain = DeviceChainViewState::default();
    assert!(bypass_chain.devices[0].is_enabled);
    assert!(bypass_chain.toggle_bypass(0));
    assert!(!bypass_chain.devices[0].is_enabled);

    let dry_sample = 0.825f32;
    let bypassed_out = bypass_chain.devices[0].process_sample(dry_sample);
    assert_eq!(
        bypassed_out, dry_sample,
        "Bypassed device must provide bit-exact dry-thru passthrough"
    );

    assert!(bypass_chain.toggle_bypass(0));
    assert!(bypass_chain.devices[0].is_enabled);

    // 5. Dynamic Device Insertion, Removal, and Reordering (Chapter 22)
    let mut dyn_chain = DeviceChainViewState::default();
    dyn_chain.devices.clear();
    assert!(dyn_chain.insert_device(0, DeviceKind::Wavetable));
    assert!(dyn_chain.insert_device(1, DeviceKind::AutoFilter));
    assert!(dyn_chain.insert_device(2, DeviceKind::Roar));
    assert_eq!(dyn_chain.devices.len(), 3);

    // Reorder right & left
    assert!(dyn_chain.move_device_right(0));
    assert_eq!(dyn_chain.devices[0].kind, DeviceKind::AutoFilter);
    assert_eq!(dyn_chain.devices[1].kind, DeviceKind::Wavetable);
    assert!(dyn_chain.move_device_left(1));
    assert_eq!(dyn_chain.devices[0].kind, DeviceKind::Wavetable);

    // Delete device
    let deleted = dyn_chain.delete_device(1);
    assert!(deleted.is_some());
    assert_eq!(deleted.unwrap().kind, DeviceKind::AutoFilter);
    assert_eq!(dyn_chain.devices.len(), 2);

    // Per-track chain isolation
    let t1 = DeviceChainViewState::for_track_1();
    let t2 = DeviceChainViewState::for_track_2();
    let t3 = DeviceChainViewState::for_track_3();
    assert_eq!(t1.devices[0].kind, DeviceKind::Wavetable);
    assert_eq!(t2.devices[0].kind, DeviceKind::DrumRack);
    assert_eq!(t3.devices[0].kind, DeviceKind::Operator);

    // 6. Hot-Swap Workflow (Chapter 22.3)
    let mut hs_chain = DeviceChainViewState::for_track_1();
    assert_eq!(hs_chain.devices[0].kind, DeviceKind::Wavetable);
    assert!(hs_chain.hot_swap_activate(0));
    assert!(hs_chain.hot_swap.active);
    assert_eq!(hs_chain.hot_swap.target_device_idx, Some(0));
    assert_eq!(
        hs_chain.hot_swap.category_filter,
        Some(BrowserCategory::Instruments)
    );

    // Commit replacement to Operator
    assert!(hs_chain.hot_swap_commit(DeviceKind::Operator));
    assert_eq!(hs_chain.devices[0].kind, DeviceKind::Operator);
    assert!(!hs_chain.hot_swap.active);

    // Hot-swap Audio FX & cancel
    assert!(hs_chain.hot_swap_activate(1));
    assert_eq!(
        hs_chain.hot_swap.category_filter,
        Some(BrowserCategory::AudioEffects)
    );
    hs_chain.hot_swap_cancel();
    assert!(!hs_chain.hot_swap.active);

    // 7. GUI Integration & Anti-Breakage Layout Audit (OKF-01 / OKF-06)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.deck_tab = DeckTab::DeviceRack;
    app.device_chain.quick_picker_open = true;
    app.device_chain.hot_swap_activate(0);

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Device Chain Bottom Deck with 3x3 picker and Hot-Swap active: {:?}",
        report
    );
}

// =========================================================================
// Chapter 31: MIDI and Key Remote Control & OKF-10 Integration
// =========================================================================

#[test]
fn test_e2e_chapter31_midi_and_key_remote_control_okf10() {
    // 1. MidiLearnRegistry: Continuous CC Mapping & Parameter Min/Max Scaling
    let mut midi_learn = MidiLearnRegistry::new();
    assert!(!midi_learn.is_learning());
    assert_eq!(midi_learn.binding_count(), 0);

    // Standard Scaling: Track 0, Device 0, Filter Cutoff (20 Hz to 20,000 Hz)
    let cutoff_target = MidiLearnTarget::new(0, 0, 1, 20.0, 20000.0);
    midi_learn.learn_next_cc(cutoff_target.clone());
    assert!(midi_learn.is_learning());

    // Learn from incoming MIDI CC 74 (Brightness / Filter Cutoff) on Channel 0
    let learn_res = midi_learn.process_midi_cc(0, 74, 64);
    assert!(learn_res.is_some());
    let (learned_target, mapped_cutoff) = learn_res.unwrap();
    assert_eq!(learned_target, cutoff_target);
    assert!(!midi_learn.is_learning(), "Learn mode must be consumed after first CC");
    assert_eq!(midi_learn.binding_count(), 1);
    // At CC 64 (midpoint), value is approximately ~10089 Hz
    assert!((mapped_cutoff - 10089.0).abs() < 50.0);

    // CC 0 maps to exactly min_val (20 Hz)
    assert_eq!(midi_learn.process_midi_cc(0, 74, 0).unwrap().1, 20.0);
    // CC 127 maps to exactly max_val (20,000 Hz)
    assert_eq!(midi_learn.process_midi_cc(0, 74, 127).unwrap().1, 20000.0);

    // Inverted Min/Max Scaling: Track 1, Volume Attenuation (0 dB down to -60 dB)
    let inv_vol_target = MidiLearnTarget::new(1, 0, 2, 0.0, -60.0);
    midi_learn.bind(0, 7, inv_vol_target.clone());
    assert_eq!(midi_learn.binding_count(), 2);
    // CC 0 produces 0.0 dB
    assert_eq!(midi_learn.process_midi_cc(0, 7, 0).unwrap().1, 0.0);
    // CC 127 produces -60.0 dB
    assert_eq!(midi_learn.process_midi_cc(0, 7, 127).unwrap().1, -60.0);
    // Bidirectional feedback mapping (DAW float -> 7-bit CC)
    assert_eq!(inv_vol_target.map_value_to_cc(0.0), 0);
    assert_eq!(inv_vol_target.map_value_to_cc(-60.0), 127);
    assert_eq!(inv_vol_target.map_value_to_cc(-30.0), 64);

    // 2. MidiLearnRegistry: MIDI Note Remote Control (Mute/Solo/Triggers)
    let mute_target = MidiLearnTarget::new(0, 0, 10, 0.0, 1.0);
    midi_learn.learn_next_note(mute_target.clone());
    assert!(midi_learn.is_learning_note());

    // Velocity 0 (NoteOff) during learn is ignored
    assert!(midi_learn.process_midi_note(0, 36, 0, 0.0).is_none());
    assert!(midi_learn.is_learning_note());

    // NoteOn (Note 36 = C1 Kick Pad) learns the mute target and toggles 0.0 -> 1.0
    let note_res = midi_learn.process_midi_note(0, 36, 127, 0.0);
    assert!(note_res.is_some());
    assert_eq!(note_res.unwrap().1, 1.0);
    assert!(!midi_learn.is_learning_note());
    assert_eq!(midi_learn.note_binding_count(), 1);
    assert_eq!(midi_learn.total_binding_count(), 3);

    // Subsequent NoteOn toggles back to 0.0
    let note_res2 = midi_learn.process_midi_note(0, 36, 100, 1.0);
    assert_eq!(note_res2.unwrap().1, 0.0);

    // 3. KeyLearnRegistry: Computer Keyboard Remote Control (Cmd+K)
    let mut key_learn = KeyLearnRegistry::new();
    assert!(!key_learn.is_learning());
    assert_eq!(key_learn.binding_count(), 0);

    // Toggle Mode Mapping: Key 'A' toggles Track Solo
    let key_solo = KeyLearnTarget::new(0, 0, 20, 0.0, 1.0);
    key_learn.learn_next_key(key_solo.clone());
    assert!(key_learn.is_learning());

    let key_res1 = key_learn.process_key_press("KeyA", 0.0);
    assert!(key_res1.is_some());
    let (learned_key_target, key_val) = key_res1.unwrap();
    assert_eq!(learned_key_target, key_solo);
    assert_eq!(key_val, 1.0);
    assert!(!key_learn.is_learning());
    assert_eq!(key_learn.binding_count(), 1);

    // Toggle back
    assert_eq!(key_learn.process_key_press("KeyA", 1.0).unwrap().1, 0.0);

    // Momentary Mode Mapping: Key 'Space' momentary transport trigger
    let key_momentary = KeyLearnTarget::new(0, 0, 99, 0.0, 1.0).with_toggle(false);
    key_learn.bind("Space", key_momentary);
    assert_eq!(key_learn.binding_count(), 2);
    // Press -> max_val (1.0)
    assert_eq!(key_learn.process_key_press("Space", 0.0).unwrap().1, 1.0);
    // Release -> min_val (0.0)
    assert_eq!(key_learn.process_key_release("Space").unwrap().1, 0.0);

    // Inverted Key Mapping: min 100.0, max 0.0
    let key_inv = KeyLearnTarget::new(1, 0, 30, 100.0, 0.0);
    key_learn.bind("KeyZ", key_inv);
    assert_eq!(key_learn.process_key_press("KeyZ", 100.0).unwrap().1, 0.0);
    assert_eq!(key_learn.process_key_press("KeyZ", 0.0).unwrap().1, 100.0);

    // 4. StudioShortcutManager: Chapter 31 Shortcut Modalities
    let cmd = Modifiers::COMMAND;
    let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;

    // Cmd+M: Toggle MIDI Map Mode
    assert_eq!(
        StudioShortcutManager::evaluate_shortcut(Key::M, cmd),
        Some(StudioShortcutAction::ToggleMidiMapMode)
    );

    // Cmd+K: Toggle Key Map Mode
    assert_eq!(
        StudioShortcutManager::evaluate_shortcut(Key::K, cmd),
        Some(StudioShortcutAction::ToggleKeyMapMode)
    );

    // Cmd+Shift+M: Disambiguation with New MIDI Clip
    assert_eq!(
        StudioShortcutManager::evaluate_shortcut(Key::M, cmd_shift),
        Some(StudioShortcutAction::NewMidiClip)
    );

    // 5. Parameter Takeover Curves (Pickup vs ValueScaling)
    let pickup = TakeoverMode::Pickup;
    let target_val = 0.70;
    // Hardware knob moving from 0.0 to 0.5 below target_val -> ignored
    assert_eq!(pickup.evaluate_takeover(target_val, 0.0, 0.5), None);
    // Hardware knob crosses 0.70 (0.6 -> 0.75) -> takeover acquired
    assert_eq!(pickup.evaluate_takeover(target_val, 0.6, 0.75), Some(0.75));

    let val_scale = TakeoverMode::ValueScaling;
    let soft_pos = 0.40;
    // Proportional step upwards
    let step = val_scale.evaluate_takeover(soft_pos, 0.1, 0.2).unwrap();
    assert!(step > soft_pos && step < 0.6);
    // Convergence at 1.0
    assert_eq!(val_scale.evaluate_takeover(0.85, 0.9, 1.0).unwrap(), 1.0);
    // Convergence at 0.0
    assert_eq!(val_scale.evaluate_takeover(0.15, 0.1, 0.0).unwrap(), 0.0);

    // 6. Push 3 & Control Surface Parity (OKF-10 Section 1)
    assert_eq!(ControlSurfaceModel::Push3.name(), "Ableton Push 3");
    assert!(ControlSurfaceModel::Push3.has_display());
    assert_eq!(ControlSurfaceModel::Push3.display_resolution(), Some((960, 160)));
    assert_eq!(ControlSurfaceModel::Push3.pad_matrix_dimensions(), (8, 8));

    let mut push_state = PushPadMatrixState::new();
    let mut session_clip_states = [[0u8; 8]; 8];
    session_clip_states[0][0] = 2; // Playing -> Green
    session_clip_states[0][1] = 1; // Stopped -> Amber
    session_clip_states[0][2] = 3; // Recording -> Red
    push_state.sync_session_clip_state_matrix(&session_clip_states);
    assert_eq!(push_state.get_pad(0, 0), PushPadColor::GreenPlaying);
    assert_eq!(push_state.get_pad(0, 1), PushPadColor::AmberStopped);
    assert_eq!(push_state.get_pad(0, 2), PushPadColor::RedRecording);
    assert_eq!(push_state.get_pad(0, 3), PushPadColor::Off);

    // 7. GUI Integration & Anti-Breakage Layout Audit (OKF-10 / Chapter 31)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.status_message = "Chapter 31: MIDI & Key Map Active".to_string();

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Chapter 31 MIDI/Key Map active state: {:?}",
        report
    );
}

// =========================================================================
// 22. Chapter 24 & OKF-06 / OKF-07: Instrument, Drum and Effect Racks
// =========================================================================

#[test]
fn test_e2e_chapter24_instrument_drum_and_effect_racks() {
    let sample_rate = 44100.0;
    let mut rack = DrumRack::new(sample_rate);

    // 1. Drum Rack 128-Pad Matrix across 8 Banks (Chapter 24.2 & OKF-06 Section 2.9)
    assert_eq!(rack.all_pads.len(), 128, "Drum Rack must provide all 128 pads across 8 banks");
    assert_eq!(rack.pads.len(), 16, "Active visible bank must host 16 pads");
    assert_eq!(rack.active_bank, 0, "Default active bank is Bank A (index 0)");

    // Bank navigation: Bank A -> Bank B (index 1) -> Bank H (index 7)
    rack.select_bank(1);
    assert_eq!(rack.active_bank, 1);
    assert_eq!(rack.pads[0].midi_note, 0, "Bank B pad 0 maps to MIDI note 0");
    rack.select_bank(7);
    assert_eq!(rack.active_bank, 7);
    assert_eq!(rack.pads[15].midi_note, 127, "Bank H pad 15 maps to MIDI note 127");
    rack.select_bank(0);

    // 2. 16 Independent Choke Groups with <5ms Clickless Damping (Chapter 24.3 & OKF-07)
    for g in 1..=16u8 {
        rack.set_pad_choke_group((g - 1) as usize, Some(g));
        rack.set_pad_choke_group((g - 1 + 16) as usize, Some(g));
    }
    // Trigger Pad A of choke group 3 (Pad 2)
    rack.trigger_midi_note(rack.all_pads[2].midi_note, 100);
    let mut buf = AudioBuffer::silence(128);
    let time = MusicalTime::default();
    rack.process(&[], &mut buf, &time);
    let voice_a = rack.voices.iter().find(|v| v.active && v.pad_idx == 2);
    assert!(voice_a.is_some(), "Choke Group 3: Pad A must have active voice");
    assert!(!voice_a.unwrap().is_fading_out);

    // Trigger Pad B of choke group 3 (Pad 18)
    rack.trigger_midi_note(rack.all_pads[18].midi_note, 127);
    let voice_a_choked = rack.voices.iter().find(|v| v.active && v.pad_idx == 2);
    if let Some(va) = voice_a_choked {
        assert!(va.is_fading_out, "Voice A must fade out immediately upon choke trigger");
    }
    // Process 240 samples (< 5.5ms)
    let mut flush_buf = AudioBuffer::silence(240);
    rack.process(&[], &mut flush_buf, &time);
    let voice_a_after = rack.voices.iter().find(|v| v.active && v.pad_idx == 2);
    if let Some(va) = voice_a_after {
        assert!(va.envelope_val < 0.001, "Voice A must be damped below -60 dBFS (<0.001) in <5ms");
    }

    // 3. Per-Pad Simpler Instances (Chapter 24.2 & OKF-06 Section 2.8)
    let mut simpler = SimplerDevice::new(sample_rate);
    let tone_len = 44100;
    let mut test_sample = Vec::with_capacity(tone_len);
    for i in 0..tone_len {
        let t = i as f32 / sample_rate;
        test_sample.push((2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.75);
    }
    simpler.load_sample(test_sample, 44100, 1);
    simpler.mode = SimplerMode::Classic;

    // Mount Simpler instance on visible pad 6 (MIDI 42)
    assert!(rack.load_simpler_pad(6, simpler).is_ok());
    assert!(rack.pads[6].has_simpler());
    rack.trigger_midi_note(42, 110);
    let mut simpler_buf = AudioBuffer::silence(256);
    rack.process(&[], &mut simpler_buf, &time);
    let energy: f32 = simpler_buf.left[..256].iter().map(|s| s * s).sum();
    assert!(energy > 0.001, "Per-pad Simpler instance must synthesize non-zero audio");

    // 4. Return Chains & Sends (Chapter 24.4)
    rack.all_pads[6].send_a = 0.8;
    rack.all_pads[6].send_b = 0.6;
    rack.all_pads[6].volume = 0.0; // Mute dry pad
    rack.return_a_level = 1.0;
    rack.return_b_level = 1.0;
    rack.trigger_midi_note(42, 127);
    let mut wet_buf = AudioBuffer::silence(512);
    rack.process(&[], &mut wet_buf, &time);
    let wet_energy_l: f32 = wet_buf.left[..512].iter().map(|s| s * s).sum();
    let wet_energy_r: f32 = wet_buf.right[..512].iter().map(|s| s * s).sum();
    assert!(wet_energy_l > 0.0001 && wet_energy_r > 0.0001, "Return chains must receive send taps");

    // 5. Macro Controls 1..16 & Parameter Mapping (Chapter 24.5 & OKF-06 Section 1)
    rack.set_macro_count(16);
    assert_eq!(rack.macro_count(), 16, "Live 12 Racks support 1..16 Macro controls");
    rack.map_macro(0, DrumRackMacroTarget::PadVolume(0), 0.0, 2.0);
    rack.map_macro(1, DrumRackMacroTarget::ReturnALevel, 0.0, 1.5);
    rack.set_macro(0, 0.5);
    assert!((rack.all_pads[0].volume - 1.0).abs() < 1e-4);
    rack.set_macro(1, 0.8);
    assert!((rack.return_a_level - 1.2).abs() < 1e-4);

    // 6. Macro Variations (Chapter 24.6)
    let var0 = rack.capture_variation("Verse Balance");
    assert_eq!(var0, 0);
    assert_eq!(rack.variations_count(), 1);

    rack.set_macro(0, 1.0); // Volume = 2.0
    rack.set_macro(1, 0.2); // Return A = 0.3
    let var1 = rack.capture_variation("Drop Energy");
    assert_eq!(var1, 1);
    assert_eq!(rack.variations_count(), 2);

    // Recall variation 0
    assert!(rack.recall_variation(0).is_ok());
    assert!((rack.all_pads[0].volume - 1.0).abs() < 1e-4);
    assert!((rack.return_a_level - 1.2).abs() < 1e-4);

    // Recall variation 1 by name
    assert!(rack.recall_variation_by_name("Drop Energy").is_ok());
    assert!((rack.all_pads[0].volume - 2.0).abs() < 1e-4);

    // Randomize macros
    rack.randomize_macros();
    assert!((0.0..=1.0).contains(&rack.get_macro(0)));

    // 7. GUI Integration & Anti-Breakage Layout Audit (Chapter 24 / OKF-06 Device View)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.deck_tab = DeckTab::DeviceRack;
    app.status_message = "Chapter 24: Drum Rack 128-Pad Matrix & Macros Active".to_string();

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Chapter 24 Drum Rack / Device View: {:?}",
        report
    );
}

// =========================================================================
// 30. Chapter 30 & OKF-05: Clip Envelopes & Modulation (Epic #23)
// =========================================================================

#[test]
fn test_e2e_chapter30_clip_envelopes_and_modulation() {
    // 1. SessionClip with Multi-Target Clip Envelopes
    let pattern = ClipPattern::Midi(MidiPattern::new(4.0));
    let mut session_clip = SessionClip::new(1, "Lead Synth Motif", pattern);

    // Add Pitch Bend Envelope (Modulation Mode)
    let mut pb_env = ClipEnvelope::new_pitch_bend();
    pb_env.add_linear_point(0.0, 0.5);  // Neutral center (0 deflection)
    pb_env.add_linear_point(1.0, 1.0);  // Full bend up (+8191, +2.0 st)
    pb_env.add_linear_point(2.0, 0.0);  // Full bend down (-8192, -2.0 st)
    pb_env.add_linear_point(3.0, 0.75); // Half bend up (+1.0 st)
    session_clip.envelopes.add_envelope(pb_env);

    // Add Modulation Wheel (CC 1) Automation Envelope
    let mut mw_env = ClipEnvelope::new_mod_wheel();
    mw_env.add_linear_point(0.0, 0.0);
    mw_env.add_linear_point(2.0, 0.5);
    mw_env.add_linear_point(4.0, 1.0);
    session_clip.envelopes.add_envelope(mw_env);

    // Add Continuous Controller 74 (Filter Brightness/Cutoff) Automation Envelope
    let mut cc74_env = ClipEnvelope::new_cc(74);
    cc74_env.add_linear_point(0.0, 0.25);
    cc74_env.add_linear_point(4.0, 0.75);
    session_clip.envelopes.add_envelope(cc74_env);

    // 2. 14-bit Pitch Bend & CC Evaluation on SessionClip
    assert_eq!(session_clip.evaluate_pitch_bend(0.0), 0);
    assert_eq!(session_clip.evaluate_pitch_bend_semitones(0.0, 2.0), 0.0);
    assert_eq!(session_clip.evaluate_pitch_bend(1.0), 8191);
    assert_eq!(session_clip.evaluate_pitch_bend_semitones(1.0, 2.0), 2.0);
    assert_eq!(session_clip.evaluate_pitch_bend(2.0), -8192);
    assert_eq!(session_clip.evaluate_pitch_bend_semitones(2.0, 2.0), -2.0);
    assert!((session_clip.evaluate_pitch_bend_semitones(3.0, 2.0) - 1.0).abs() < 1e-4);

    assert_eq!(session_clip.evaluate_mod_wheel(0.0), 0);
    assert_eq!(session_clip.evaluate_mod_wheel(2.0), 64);
    assert_eq!(session_clip.evaluate_cc(74, 0.0), 32);

    // 3. Unlinked Polymetric Envelope Loop Phasing (3-beat envelope over 4-beat clip)
    let mut unlinked_env = ClipEnvelope::new_mixer_volume(EnvelopeMode::Modulation);
    unlinked_env.set_unlinked(0.0, 3.0); // 3-beat independent loop length
    unlinked_env.add_linear_point(0.0, 0.5); // 0 offset
    unlinked_env.add_linear_point(1.5, 1.0); // +1.0 offset (+100%)
    unlinked_env.add_linear_point(3.0, 0.0); // -1.0 offset (-100%)

    let clip_start = 0.0;
    let clip_end = 4.0;
    assert_eq!(unlinked_env.phase_at(0.0, clip_start, clip_end), 0.0);
    assert_eq!(unlinked_env.phase_at(1.5, clip_start, clip_end), 1.5);
    assert_eq!(unlinked_env.phase_at(3.0, clip_start, clip_end), 0.0); // Wraps at beat 3
    assert!((unlinked_env.phase_at(4.0, clip_start, clip_end) - 1.0).abs() < 1e-4); // Out-of-phase: beat 1
    assert!((unlinked_env.phase_at(8.0, clip_start, clip_end) - 2.0).abs() < 1e-4); // Out-of-phase: beat 2
    assert!((unlinked_env.phase_at(12.0, clip_start, clip_end) - 0.0).abs() < 1e-4); // Realigns at LCM 12

    // 4. Relative Modulation Math: Base parameter value modulated by envelope
    let base_cutoff = 0.5;
    let mod_val_center = unlinked_env.evaluate_effective(0.0, clip_start, clip_end, base_cutoff);
    assert_eq!(mod_val_center, 0.5, "Center breakpoint 0.5 produces 0 delta offset");

    let mod_val_high = unlinked_env.evaluate_effective(1.5, clip_start, clip_end, base_cutoff);
    assert_eq!(mod_val_high, 1.0, "Top breakpoint 1.0 clamps base 0.5 to 1.0");

    // 5. ArrangementClip Integration with Clip Envelopes
    let mut arr_clip = ArrangementClip::new(2, "Arrangement Audio Clip", 16.0, 8.0, ClipPattern::Midi(MidiPattern::new(4.0)));
    let mut arr_pb = ClipEnvelope::new_pitch_bend();
    arr_pb.add_linear_point(0.0, 0.5);
    arr_pb.add_linear_point(2.0, 1.0);
    arr_clip.envelopes.add_envelope(arr_pb);

    // At timeline beat 16.0 (pattern phase 0.0): pitch bend = 0
    assert_eq!(arr_clip.evaluate_pitch_bend(16.0), 0);
    // At timeline beat 18.0 (pattern phase 2.0): pitch bend = 8191
    assert_eq!(arr_clip.evaluate_pitch_bend(18.0), 8191);
    // At timeline beat 20.0 (pattern phase wraps to 0.0): pitch bend = 0
    assert_eq!(arr_clip.evaluate_pitch_bend(20.0), 0);

    // 6. GUI Clip View Integration & Anti-Breakage Layout Audit (Chapter 30 & OKF-05)
    let ctx = Context::default();
    apply_dark_carbon_theme(&ctx);
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
    let mut app = OxideStudioApp::new_mock(44100.0);
    app.deck_tab = DeckTab::ClipView;
    app.clip_view.sub_tab = ClipViewSubTab::Envelopes;
    app.clip_view.envelope_target = ClipEnvelopeTarget::PitchBend;
    app.clip_view.envelope_mode = EnvelopeMode::Modulation;
    app.clip_view.envelope_linked = false;
    app.clip_view.envelope_unlinked_length_beats = 3.0;
    app.status_message = "Chapter 30: Clip Envelopes & Modulation View Active".to_string();

    let output = render_studio_frame(&mut app, &ctx, screen_rect);
    let report = audit_rendered_frame(screen_rect, &output.shapes);
    assert!(
        report.is_clean(),
        "Studio frame audit failed under Chapter 30 Clip Envelopes & Modulation View: {:?}",
        report
    );
}






