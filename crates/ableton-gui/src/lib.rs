//! Single-Window 60 FPS GPU Studio UI/UX for ableton-rust.
//!
//! Provides:
//! - Dark Carbon Studio Theme (`#07080a`, `#111318`, `#ffbe00`, `#00d2ff`, `#00e676`)
//! - Top transport bar with real-time oscilloscope, spectrum, and CPU load meters
//! - Center workspace with Session Matrix vs Linear Arrangement Timeline
//! - Bottom deck with Device rack, 4-pole ZDF frequency-response curve, 16-step sequencer, and XY perform pad
//! - Computer keyboard playing (Piano and Wicki-Hayden Isomorphic modes)
//! - Zero-allocation lock-free audio engine integration

pub mod app;
pub mod keyboard;
pub mod panels;
pub mod theme;
pub mod visualizers;
pub mod widgets;

pub use app::{AbletonStudioApp, DeckTab, WorkspaceView};
pub use keyboard::{ComputerKeyboardController, KeyboardMode};
pub use theme::apply_dark_carbon_theme;

/// Launch the native macOS single-window studio GUI via eframe.
pub fn run_gui() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("Ableton Rust — Dark Carbon Studio"),
        ..Default::default()
    };

    eframe::run_native(
        "Ableton Rust",
        native_options,
        Box::new(|cc| Ok(Box::new(AbletonStudioApp::new(Some(cc))))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ableton_studio_app_headless_creation_and_telemetry() {
        let mut app = AbletonStudioApp::new_mock(44100.0);
        assert_eq!(app.workspace_view, WorkspaceView::SessionMatrix);
        assert_eq!(app.deck_tab, DeckTab::ClipView);
        assert_eq!(app.keyboard.mode, KeyboardMode::Piano);

        // Advance audio & poll telemetry
        app.send_command(ableton_core::AudioCommand::Play);
        app.poll_audio();

        assert_eq!(app.telemetry.track_peaks_l.len(), 16);
        assert_eq!(app.telemetry.oscilloscope_slice.len(), 256);
        assert_eq!(app.telemetry.spectrum_bins.len(), 64);
    }

    #[test]
    fn test_one_click_wav_export() {
        let mut app = AbletonStudioApp::new_mock(44100.0);
        let out_path = "/tmp/test_ableton_gui_one_click_export.wav";
        let res = app.export_demo_wav(out_path);
        assert!(res.is_ok(), "WAV export must succeed");
        assert!(std::path::Path::new(out_path).exists());
        let _ = std::fs::remove_file(out_path);
    }
}
