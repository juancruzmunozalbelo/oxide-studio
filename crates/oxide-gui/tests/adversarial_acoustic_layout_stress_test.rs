//! Adversarial Empirical Stress Test Suite for Acoustic Output & Layout Anti-Breakage.
//!
//! Specifically validates:
//! 1. Acoustic stress-testing:
//!    - Render 2048 samples from every pad 0..15 in `crates/oxide-drums`.
//!    - Verify non-zero energy, distinct zero crossings, and that no pad duplicates Kick909.
//!    - Pairwise waveform dissimilarity: all 16 pads have unique spectral profiles.
//!    - Pulling faders to 0.0 or muting renders bit-exact 0.0 float samples and 0.0 post-fader VU peak meters.
//! 2. Layout stress-testing:
//!    - Run layout audit across all extreme splitter heights (180.0px, 200.0px, 300.0px, 450.0px)
//!      and 4 standard resolutions (1280x800, 1440x900, 1728x1117, 1920x1080).
//!    - Assert 0 overflows, 0 clipped texts, 0 collisions, 0 tofu glyphs.

use oxide_audio::{
    parameters::AtomicParameters,
    processor::{
        AudioEngineProcessor, PARAM_ID_MASTER_VOL, PARAM_ID_TRACK_MUTE_BASE,
        PARAM_ID_TRACK_VOL_BASE,
    },
    queues::AudioQueues,
};
use oxide_core::AudioCommand;
use oxide_drums::drum_rack::DrumRack;
use oxide_gui::{
    app::{OxideStudioApp, DeckTab, WorkspaceView},
    layout::ViewportBudget,
    layout_guard::audit_rendered_frame,
    panels::{
        clip_view::ClipViewSubTab,
        sequencer::{DrumBank, BANK_A_PAD_NAMES, BANK_B_PAD_NAMES},
    },
    theme::apply_dark_carbon_theme,
};
use egui::{pos2, vec2, Context, RawInput, Rect};

// =========================================================================
// 1. Acoustic Output: 2048 Samples Render & Pairwise Spectral Distinction
// =========================================================================

#[test]
fn test_adv_acoustic_output_2048_samples_all_16_pads_distinct() {
    let sample_rate = 44100.0_f32;
    let block_size = 2048;

    struct PadBuffer {
        idx: usize,
        name: &'static str,
        bank: &'static str,
        samples: Vec<f32>,
        rms: f32,
        zero_crossings: usize,
    }

    let mut pad_buffers: Vec<PadBuffer> = Vec::with_capacity(16);

    for pad_idx in 0..16 {
        let mut rack = DrumRack::new(sample_rate);
        rack.trigger_pad(pad_idx as u8, 100);

        let mut out_l = vec![0.0_f32; block_size];
        let mut out_r = vec![0.0_f32; block_size];
        rack.process_block(&mut out_l, &mut out_r);

        // 1. Calculate RMS energy over 2048 samples
        let sum_sq: f32 = out_l.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / block_size as f32).sqrt();

        // Must produce non-zero audio energy
        assert!(
            rms > 1e-4,
            "Pad {} ({}) rendered zero energy: rms = {:.6}",
            pad_idx,
            rack.pads[pad_idx].name,
            rms
        );

        // 2. Count Zero Crossings over 2048 samples
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

        pad_buffers.push(PadBuffer {
            idx: pad_idx,
            name,
            bank,
            samples: out_l,
            rms,
            zero_crossings,
        });
    }

    let kick = &pad_buffers[0];
    assert_eq!(kick.name, "Kick");

    // Assert NO pad in Bank B duplicates Kick909
    for pad in &pad_buffers[8..16] {
        let is_identical_to_kick =
            (pad.rms - kick.rms).abs() < 1e-5 && pad.zero_crossings == kick.zero_crossings;
        assert!(
            !is_identical_to_kick,
            "Pad {} ({} in {}) duplicates Kick909! (RMS={:.5}, ZC={})",
            pad.idx,
            pad.name,
            pad.bank,
            pad.rms,
            pad.zero_crossings
        );
    }

    // Pairwise waveform distance verification: all 16 drum pads must be mutually distinct
    for i in 0..16 {
        for j in (i + 1)..16 {
            let p_i = &pad_buffers[i];
            let p_j = &pad_buffers[j];

            let diff_sq: f32 = p_i
                .samples
                .iter()
                .zip(p_j.samples.iter())
                .map(|(&a, &b)| (a - b) * (a - b))
                .sum();
            let distance = (diff_sq / block_size as f32).sqrt();

            assert!(
                distance > 1e-3,
                "Pads {} ({}) and {} ({}) have nearly identical acoustic waveforms (dist={:.6})",
                p_i.idx,
                p_i.name,
                p_j.idx,
                p_j.name,
                distance
            );
        }
    }
}

// =========================================================================
// 2. Fader & Mute Silence: Bit-Exact 0.0 Samples & 0.0 VU Meters
// =========================================================================

#[test]
fn test_adv_fader_and_mute_silence_bit_exact_zeros() {
    let queues = AudioQueues::new();
    let (mut ui, audio) = queues.split();
    let params = AtomicParameters::new();
    let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

    // 1. Play active audio on Track 0 and Track 1
    ui.send_command(AudioCommand::Play);
    ui.send_command(AudioCommand::NoteOn {
        track_id: 0,
        note: 60,
        velocity: 110,
    });
    ui.send_command(AudioCommand::NoteOn {
        track_id: 1,
        note: 36,
        velocity: 110,
    });

    let mut buf = [0.0_f32; 2048];
    proc.process_interleaved(&mut buf);
    let active_energy: f32 = buf.iter().map(|&s| s * s).sum();
    assert!(active_energy > 1e-4, "Audio must be rendered in active playback");
    assert!(proc.mixer.tracks[0].peak_l > 0.0);
    assert!(proc.mixer.tracks[1].peak_l > 0.0);
    assert!(proc.mixer.master_peak_l > 0.0);

    // 2. Mute Track 0: verify Track 0 VU meter drops to exact 0.0
    ui.send_command(AudioCommand::SetParameter {
        track_id: 0,
        param_id: PARAM_ID_TRACK_MUTE_BASE,
        value: 1.0,
    });
    proc.process_interleaved(&mut buf);
    assert_eq!(proc.mixer.tracks[0].peak_l, 0.0_f32);
    assert_eq!(proc.mixer.tracks[0].peak_r, 0.0_f32);

    // 3. Set Track 1 Volume to 0.0 (fader at -inf dB): verify Track 1 VU meter is 0.0
    ui.send_command(AudioCommand::SetParameter {
        track_id: 1,
        param_id: PARAM_ID_TRACK_VOL_BASE + 1,
        value: 0.0,
    });
    // Trigger drum again while fader is 0.0
    ui.send_command(AudioCommand::NoteOn {
        track_id: 1,
        note: 38,
        velocity: 127,
    });
    proc.process_interleaved(&mut buf);
    assert_eq!(proc.mixer.tracks[1].peak_l, 0.0_f32);
    assert_eq!(proc.mixer.tracks[1].peak_r, 0.0_f32);

    // With Track 0 muted and Track 1 at 0.0 volume, master buffer MUST be bit-exact 0.0
    for (idx, &sample) in buf.iter().enumerate() {
        assert_eq!(
            sample, 0.0_f32,
            "Sample at index {} is not bit-exact 0.0: {:+.8e}",
            idx, sample
        );
    }
    assert_eq!(proc.mixer.master_peak_l, 0.0_f32);
    assert_eq!(proc.mixer.master_peak_r, 0.0_f32);

    // 4. Test Master Volume at 0.0 (-inf dB) with unmuted tracks
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
        note: 67,
        velocity: 120,
    });
    ui.send_command(AudioCommand::NoteOn {
        track_id: 1,
        note: 36,
        velocity: 120,
    });

    proc.process_interleaved(&mut buf);
    for (idx, &sample) in buf.iter().enumerate() {
        assert_eq!(
            sample, 0.0_f32,
            "Master silence sample at index {} is not bit-exact 0.0: {:+.8e}",
            idx, sample
        );
    }
    assert_eq!(proc.mixer.master_peak_l, 0.0_f32);
    assert_eq!(proc.mixer.master_peak_r, 0.0_f32);
}

// =========================================================================
// 3. Layout Stress-Testing: Splitter Heights & Multi-Resolution Anti-Breakage
// =========================================================================

#[test]
fn test_adv_layout_stress_splitter_heights_and_multi_resolution() {
    let resolutions = [
        ("1280x800 Standard Laptop", 1280.0_f32, 800.0_f32),
        ("1440x900 MacBook Air Retina", 1440.0, 900.0),
        ("1728x1117 MacBook Pro 16\"", 1728.0, 1117.0),
        ("1920x1080 Studio Display FHD", 1920.0, 1080.0),
    ];

    let splitter_heights = [180.0_f32, 200.0_f32, 300.0_f32, 450.0_f32];

    #[derive(Debug, Clone, Copy)]
    enum TestDeckState {
        ClipNotes,
        ClipTransform,
        ClipGenerate,
        ClipWarping,
        DeviceRackStandard,
        DeviceRackAddDeviceOpen,
        DrumBankA,
        DrumBankB,
        XySurface,
    }

    let deck_states = [
        TestDeckState::ClipNotes,
        TestDeckState::ClipTransform,
        TestDeckState::ClipGenerate,
        TestDeckState::ClipWarping,
        TestDeckState::DeviceRackStandard,
        TestDeckState::DeviceRackAddDeviceOpen,
        TestDeckState::DrumBankA,
        TestDeckState::DrumBankB,
        TestDeckState::XySurface,
    ];

    let workspace_views = [
        WorkspaceView::SessionMatrix,
        WorkspaceView::ArrangementTimeline,
    ];

    let mut all_failures = Vec::new();

    for (res_name, w, h) in resolutions {
        let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));

        for &deck_h in &splitter_heights {
            let budget = ViewportBudget::compute_with_deck_height(
                screen_rect,
                true,
                true,
                Some(deck_h),
            );

            // Verify budget invariants
            assert!(
                budget.center_height >= h * 0.55 - 0.01,
                "{} at deck_h={}: center height {:.2} < 55% of screen height {:.2}",
                res_name,
                deck_h,
                budget.center_height,
                h * 0.55
            );
            assert!(
                budget.bottom_deck_height <= h * 0.34 + 0.01,
                "{} at deck_h={}: bottom deck height {:.2} > 34% of screen height {:.2}",
                res_name,
                deck_h,
                budget.bottom_deck_height,
                h * 0.34
            );
            assert!(
                budget.bottom_deck_height >= 180.0 - 0.01,
                "{} at deck_h={}: bottom deck height {:.2} < 180.0px floor",
                res_name,
                deck_h,
                budget.bottom_deck_height
            );

            for &view in &workspace_views {
                for &deck in &deck_states {
                    for &key_drawer in &[false, true] {
                        let ctx = Context::default();
                        apply_dark_carbon_theme(&ctx);

                        let mut app = OxideStudioApp::new_mock(44100.0);
                        app.workspace_view = view;
                        app.browser.is_collapsed = false;
                        app.copilot_open = true;
                        app.ai_copilot.show_key_settings = key_drawer;

                        match deck {
                            TestDeckState::ClipNotes => {
                                app.deck_tab = DeckTab::ClipView;
                                app.clip_view.sub_tab = ClipViewSubTab::Notes;
                            }
                            TestDeckState::ClipTransform => {
                                app.deck_tab = DeckTab::ClipView;
                                app.clip_view.sub_tab = ClipViewSubTab::Transform;
                            }
                            TestDeckState::ClipGenerate => {
                                app.deck_tab = DeckTab::ClipView;
                                app.clip_view.sub_tab = ClipViewSubTab::Generate;
                            }
                            TestDeckState::ClipWarping => {
                                app.deck_tab = DeckTab::ClipView;
                                app.clip_view.sub_tab = ClipViewSubTab::Warping;
                            }
                            TestDeckState::DeviceRackStandard => {
                                app.deck_tab = DeckTab::DeviceRack;
                                app.device_chain.quick_picker_open = false;
                            }
                            TestDeckState::DeviceRackAddDeviceOpen => {
                                app.deck_tab = DeckTab::DeviceRack;
                                app.device_chain.quick_picker_open = true;
                            }
                            TestDeckState::DrumBankA => {
                                app.deck_tab = DeckTab::DrumSequencer;
                                app.drum_sequencer.bank = DrumBank::BankA;
                            }
                            TestDeckState::DrumBankB => {
                                app.deck_tab = DeckTab::DrumSequencer;
                                app.drum_sequencer.bank = DrumBank::BankB;
                            }
                            TestDeckState::XySurface => {
                                app.deck_tab = DeckTab::XyPad;
                            }
                        }

                        let raw_input = RawInput {
                            screen_rect: Some(screen_rect),
                            ..Default::default()
                        };

                        let output = ctx.run(raw_input, |ctx| {
                            app.render_frame_with_budget(ctx, &budget);
                        });

                        let report = audit_rendered_frame(screen_rect, &output.shapes);

                        if !report.is_clean() {
                            let failure_desc = format!(
                                "{} | deck_h={:.1} | view={:?} | deck={:?} | key_drawer={}:\n\
                                 - Viewport Overflows: {:?}\n\
                                 - Clipped Texts: {:?}\n\
                                 - Text Collisions: {:?}\n\
                                 - Tofu Glyphs: {:?}",
                                res_name,
                                deck_h,
                                view,
                                deck,
                                key_drawer,
                                report.viewport_overflows,
                                report.clipped_texts,
                                report.text_collisions,
                                report.unsupported_glyphs,
                            );
                            all_failures.push(failure_desc);
                        }
                    }
                }
            }
        }
    }

    assert!(
        all_failures.is_empty(),
        "Anti-Breakage layout audit failed in {} configurations:\n{}",
        all_failures.len(),
        all_failures.join("\n---\n")
    );
}
