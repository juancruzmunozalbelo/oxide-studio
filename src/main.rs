//! ableton-rust — Native macOS Electronic Music DAW CLI & Studio Entrypoint.
//!
//! Subcommands:
//! - `--render-demo <output_path>`: Offline 4.5s multi-track demo WAV bounce with signal verification.
//! - `--scan-plugins [--output <path>]`: Out-of-process AU/VST3 plugin scanner.
//! - default: Launches GPU single-window studio GUI via `eframe::run_native`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::exit;

use ableton_bounce::{DemoArrangementRenderer, WavWriter};
use ableton_plugin_scanner::PluginScannerSupervisor;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Check for --render-demo
    if let Some(pos) = args.iter().position(|a| a == "--render-demo") {
        if pos + 1 >= args.len() {
            eprintln!("Error: --render-demo requires an output file path (e.g. --render-demo /tmp/demo.wav)");
            exit(1);
        }
        let output_path = PathBuf::from(&args[pos + 1]);
        render_demo_and_verify(&output_path);
        return;
    }

    // Check for --scan-plugins
    if args.iter().any(|a| a == "--scan-plugins") {
        let mut output_path = PathBuf::from("plugin_manifest.json");
        if let Some(pos) = args.iter().position(|a| a == "--output" || a == "-o") {
            if pos + 1 < args.len() {
                output_path = PathBuf::from(&args[pos + 1]);
            }
        }
        scan_plugins_cli(&output_path);
        return;
    }

    // Check for --mcp
    if args.iter().any(|a| a == "--mcp") {
        ableton_mcp::run_mcp_server_stdio();
        return;
    }

    // Check for --help
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }

    // Default: Launch native single-window studio GUI
    println!("Launching Ableton Rust Dark Carbon Studio GUI...");
    if let Err(e) = ableton_gui::run_gui() {
        eprintln!("Notice: GUI terminated: {}", e);
    }
}

fn print_usage() {
    println!(
        "Ableton Rust — Native macOS Electronic Music DAW\n\
        \n\
        USAGE:\n\
            ableton-rust [OPTIONS]\n\
        \n\
        OPTIONS:\n\
            -h, --help                     Print this help message\n\
            --render-demo <OUTPUT_PATH>    Render 4.5s multi-track demo WAV and verify signal metrics\n\
            --scan-plugins [--output PATH] Run out-of-process AU/VST3 plugin scanner\n\
            --mcp                          Run Model Context Protocol (MCP) server over stdio JSON-RPC 2.0\n\
            (default)                      Launch native single-window studio GUI\n"
    );
}

/// Headless demo bounce renderer and acceptance verification.
fn render_demo_and_verify(output_path: &Path) {
    let sample_rate = 44100.0;
    let bpm = 126.0;
    let duration_seconds = 4.5; // >= 4.0s required

    println!("============================================================");
    println!("  Ableton Rust Multi-Track Arrangement Demo Renderer");
    println!("============================================================");
    println!("• Target Path:    {}", output_path.display());
    println!("• Sample Rate:    {:.0} Hz (Stereo 24/16-bit PCM)", sample_rate);
    println!("• Tempo:          {:.1} BPM", bpm);
    println!("• Duration:       {:.2} seconds", duration_seconds);

    let renderer = DemoArrangementRenderer::new(sample_rate, bpm);
    let interleaved = renderer.render_demo_seconds(duration_seconds);

    let total_samples = interleaved.len();
    let expected_samples = (duration_seconds * sample_rate as f64 * 2.0) as usize;
    assert_eq!(
        total_samples, expected_samples,
        "Rendered buffer frame count mismatch"
    );

    // Compute metrics: peak, RMS, verify zero NaN/Inf
    let mut sum_sq = 0.0f64;
    let mut peak = 0.0f32;

    for &s in &interleaved {
        assert!(s.is_finite(), "Sample is not finite!");
        assert!(!s.is_nan(), "Sample is NaN!");
        let abs_val = s.abs();
        if abs_val > peak {
            peak = abs_val;
        }
        sum_sq += (s * s) as f64;
    }

    let rms = (sum_sq / total_samples as f64).sqrt() as f32;

    println!("• Metrics:");
    println!("  - Peak Amplitude: {:.4} ({:.2} dBFS)", peak, 20.0 * peak.max(1e-5).log10());
    println!("  - RMS Energy:     {:.4} ({:.2} dBFS)", rms, 20.0 * rms.max(1e-5).log10());
    println!("  - Total Frames:   {} frames", total_samples / 2);

    // Invariant assertions
    assert!(
        duration_seconds >= 4.0,
        "Duration must be >= 4.0 seconds, got {}",
        duration_seconds
    );
    assert!(
        rms > 0.02,
        "Audio must be audible: RMS must be > 0.02, got {}",
        rms
    );
    assert!(
        peak <= 1.0,
        "Limiter ceiling violated: peak {} > 1.0",
        peak
    );

    // Write to standard 44-byte RIFF stereo WAV
    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    WavWriter::write_wav_16bit(output_path, 44100, 2, &interleaved)
        .expect("Failed to write demo WAV file");

    println!("✓ Verified: Duration >= 4.0s, RMS > 0.02, Zero NaN/Inf, Peak <= 1.0");
    println!("✓ Export Succeeded: {}", output_path.display());
    println!("============================================================");
}

/// Out-of-process AU/VST3 plugin scanner CLI handler.
fn scan_plugins_cli(output_path: &Path) {
    println!("============================================================");
    println!("  Ableton Plugin Scanner (Out-of-Process macOS AU/VST3)");
    println!("============================================================");
    println!("• Output Manifest: {}", output_path.display());

    let supervisor = PluginScannerSupervisor::new();
    let candidates = supervisor.discover_candidate_bundles();
    println!("• Discovered {} plugin candidate bundles", candidates.len());

    let manifest = supervisor.scan_all();
    println!("• Scanned {} plugins ({} valid, {} quarantined)",
        manifest.plugins.len(),
        manifest.plugins.iter().filter(|p| p.is_valid).count(),
        manifest.plugins.iter().filter(|p| p.quarantined).count(),
    );

    for p in &manifest.plugins {
        let status = if p.is_valid { "VALID" } else { "QUARANTINED" };
        println!("  - [{}] {} ({}) — {}", status, p.name, p.format, p.path.display());
    }

    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    manifest
        .save_to_file(output_path)
        .expect("Failed to write plugin manifest file");

    println!("✓ Manifest saved to: {}", output_path.display());
    println!("============================================================");
}
