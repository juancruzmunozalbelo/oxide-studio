//! Single-Window 60 FPS GPU Studio UI/UX for Oxide Studio.
//!
//! Provides:
//! - Dark Carbon Studio Theme (`#07080a`, `#111318`, `#ffbe00`, `#00d2ff`, `#00e676`)
//! - Top transport bar with real-time oscilloscope, spectrum, and CPU load meters
//! - Center workspace with Session Matrix vs Linear Arrangement Timeline
//! - Bottom deck with Device rack, 4-pole ZDF frequency-response curve, 16-step sequencer, and XY perform pad
//! - Computer keyboard playing (Piano and Wicki-Hayden Isomorphic modes)
//! - Zero-allocation lock-free audio engine integration
//! - Native Non-Blocking Live GUI RPC Server (`127.0.0.1:9877`)

pub mod app;
pub mod keyboard;
pub mod layout;
pub mod layout_guard;
pub mod live_rpc;
pub mod okf_knowledge;
pub mod panels;
pub mod routing;
pub mod shortcuts;
pub mod theme;
pub mod visualizers;
pub mod widgets;

pub use app::{OxideStudioApp, DeckTab, WorkspaceView};
pub use keyboard::{ComputerKeyboardController, KeyboardMode};
pub use layout::{BoundedCard, SessionSplitBudget, SplitRegion, StudioComponent, StudioPanelSpec, ViewportBudget};
pub use live_rpc::OxideGuiRpcServer;
pub use routing::{RoutingValidationError, SignalEdge, SignalRoutingGraph, SignalRoutingNode};
pub use shortcuts::{StudioShortcutAction, StudioShortcutManager};
pub use theme::apply_dark_carbon_theme;

/// Launch the native macOS single-window studio GUI via eframe.
pub fn run_gui() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Oxide Studio — Native Electronic Music Workstation"),
        ..Default::default()
    };

    eframe::run_native(
        "Oxide Studio",
        native_options,
        Box::new(|cc| Ok(Box::new(OxideStudioApp::new(Some(cc))))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ableton_studio_app_headless_creation_and_telemetry() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        assert_eq!(app.workspace_view, WorkspaceView::SessionMatrix);
        assert_eq!(app.deck_tab, DeckTab::ClipView);
        assert_eq!(app.keyboard.mode, KeyboardMode::Piano);

        // Advance audio & poll telemetry
        app.send_command(oxide_core::AudioCommand::Play);
        app.poll_audio();

        assert_eq!(app.telemetry.track_peaks_l.len(), 16);
        assert_eq!(app.telemetry.oscilloscope_slice.len(), 256);
        assert_eq!(app.telemetry.spectrum_bins.len(), 64);
    }

    #[test]
    fn test_one_click_wav_export() {
        let mut app = OxideStudioApp::new_mock(44100.0);
        let out_path = "/tmp/test_oxide_gui_one_click_export.wav";
        let res = app.export_demo_wav(out_path);
        assert!(res.is_ok(), "WAV export must succeed");
        assert!(std::path::Path::new(out_path).exists());
        let _ = std::fs::remove_file(out_path);
    }

    #[test]
    fn test_browser_load_all_categories() {
        use crate::panels::browser::{BrowserCategory, BrowserItem};

        let mut app = OxideStudioApp::new_mock(44100.0);

        // 1. Load Instrument
        let inst_item = BrowserItem::new(
            "inst_wavetable",
            "Wavetable",
            BrowserCategory::Instruments,
            &["Lead"],
            "Wavetable synth",
            [0.5; 8],
        );
        let init_dev_count = app.device_chain.devices.len();
        app.handle_browser_load(&inst_item);
        assert_eq!(app.device_chain.devices.len(), init_dev_count + 1);
        assert!(app.status_message.contains("Loaded: Wavetable"));

        // 2. Load Drum Sound
        let drum_item = BrowserItem::new(
            "drum_909_kick",
            "909 Punch Kick",
            BrowserCategory::Drums,
            &["Kick"],
            "Punchy kick",
            [0.5; 8],
        );
        app.handle_browser_load(&drum_item);
        assert!(app.status_message.contains("Loaded Drum Sound: 909 Punch Kick (Pad 1)"));

        // 3. Load MIDI Clip
        let clip_item = BrowserItem::new(
            "clip_house_riff",
            "Deep House 124 Lead Riff",
            BrowserCategory::Clips,
            &["Lead"],
            "House riff",
            [0.5; 8],
        );
        app.handle_browser_load(&clip_item);
        assert_eq!(app.midi_pattern.notes.len(), 4);
        assert!(app.matrix.tracks[0].slots[0].clip.is_some());
        assert!(app.status_message.contains("Loaded Clip: Deep House 124 Lead Riff"));

        // 4. Load Audio Sample
        let sample_item = BrowserItem::new(
            "smp_vocal_chop",
            "Soulful Vocal Chop C3",
            BrowserCategory::Samples,
            &["Acoustic"],
            "Vocal chop",
            [0.5; 8],
        );
        app.handle_browser_load(&sample_item);
        assert!(app.clip_view.is_audio);
        assert!(app.status_message.contains("Loaded Sample: Soulful Vocal Chop C3"));

        // 5. Load Synth Preset Sound
        let sound_item = BrowserItem::new(
            "snd_acid_lead",
            "303 Resonant Screamer",
            BrowserCategory::Sounds,
            &["Lead"],
            "Acid lead",
            [0.5; 8],
        );
        app.handle_browser_load(&sound_item);
        assert_eq!(app.synth_state.cutoff_hz, 620.0);
        assert_eq!(app.synth_state.resonance, 4.8);
        assert!(app.status_message.contains("Loaded Sound Preset: 303 Resonant Screamer"));
    }
}
