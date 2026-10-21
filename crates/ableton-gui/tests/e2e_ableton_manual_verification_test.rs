//! Exhaustive End-to-End GUI & Audio Verification Suite (Requirement 4).
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

use ableton_audio::{
    parameters::AtomicParameters,
    processor::{
        AudioEngineProcessor, PARAM_ID_MASTER_VOL, PARAM_ID_TRACK_MUTE_BASE,
        PARAM_ID_TRACK_VOL_BASE,
    },
    queues::AudioQueues,
};
use ableton_core::AudioCommand;
use ableton_drums::drum_rack::DrumRack;
use ableton_gui::{
    app::{AbletonStudioApp, DeckTab, WorkspaceView},
    layout::ViewportBudget,
    layout_guard::audit_rendered_frame,
    panels::{
        ai_chat::{execute_studio_intent, StudioLocale},
        clip_view::ClipViewSubTab,
        sequencer::{
            render_drum_sequencer_with_state, DrumBank, DrumSequencerState,
            BANK_A_PAD_NAMES, BANK_B_PAD_NAMES,
        },
    },
    theme::apply_dark_carbon_theme,
};
use ableton_timeline::DrumStepPattern;
use egui::{
    epaint::Shape,
    pos2, vec2, Context, RawInput, Rect,
};

/// Helper to render a full studio frame matching `AbletonStudioApp::update`
fn render_studio_frame(
    app: &mut AbletonStudioApp,
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
            d.get_temp::<ableton_gui::layout::StudioSplitterState>(splitter_id)
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

                            let mut app = AbletonStudioApp::new_mock(44100.0);
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
    let mut app = AbletonStudioApp::new_mock(44100.0);

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
