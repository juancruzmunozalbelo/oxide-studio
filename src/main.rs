//! Oxide Studio — Native macOS Electronic Music DAW CLI & Studio Entrypoint.
//!
//! Subcommands:
//! - `--render-demo <output_path>`: Offline 4.5s multi-track demo WAV bounce with signal verification.
//! - `--import-als <in.als> [out.json]`: Import Ableton Live 12 .als set and optionally save as JSON.
//! - `--export-als <out.als>`: Export project to Ableton Live 12 .als gzip format.
//! - `--split-stems <in.wav> <out_dir>`: 1-click 4-stem remix separation (drums, bass, vocals, other).
//! - `--scan-plugins [--output <path>]`: Out-of-process AU/VST3 plugin scanner.
//! - `--mcp`: Model Context Protocol stdio server.
//! - default: Launches GPU single-window studio GUI via `eframe::run_native`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::exit;

use ableton_bounce::{DemoArrangementRenderer, WavWriter};
use ableton_plugin_scanner::PluginScannerSupervisor;
use ableton_project::DAWProject;

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

    // Check for --import-als
    if let Some(pos) = args.iter().position(|a| a == "--import-als") {
        if pos + 1 >= args.len() {
            eprintln!("Error: --import-als requires an input ALS file path (e.g. --import-als session.als [out.json])");
            exit(1);
        }
        let in_als = PathBuf::from(&args[pos + 1]);
        let out_json = if pos + 2 < args.len() && !args[pos + 2].starts_with("--") {
            Some(PathBuf::from(&args[pos + 2]))
        } else {
            None
        };
        import_als_cli(&in_als, out_json.as_deref());
        return;
    }

    // Check for --export-als
    if let Some(pos) = args.iter().position(|a| a == "--export-als") {
        if pos + 1 >= args.len() {
            eprintln!("Error: --export-als requires an output ALS file path (e.g. --export-als session.als)");
            exit(1);
        }
        let out_als = PathBuf::from(&args[pos + 1]);
        export_als_cli(&out_als);
        return;
    }

    // Check for --split-stems
    if let Some(pos) = args.iter().position(|a| a == "--split-stems") {
        if pos + 2 >= args.len() {
            eprintln!("Error: --split-stems requires input WAV and output directory (e.g. --split-stems mix.wav ./stems)");
            exit(1);
        }
        let in_wav = PathBuf::from(&args[pos + 1]);
        let out_dir = PathBuf::from(&args[pos + 2]);
        split_stems_cli(&in_wav, &out_dir);
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
    println!("Launching Oxide Studio Dark Carbon GUI...");
    if let Err(e) = ableton_gui::run_gui() {
        eprintln!("Notice: GUI terminated: {}", e);
    }
}

fn print_usage() {
    println!(
        "Oxide Studio — Native macOS Electronic Music DAW\n\
        \n\
        USAGE:\n\
            ableton-rust [OPTIONS]\n\
        \n\
        OPTIONS:\n\
            -h, --help                          Print this help message\n\
            --render-demo <OUTPUT_PATH>         Render 4.5s multi-track demo WAV and verify signal metrics\n\
            --import-als <IN_ALS> [OUT_JSON]    Import Ableton Live 12 .als set, inspect, and optionally save JSON\n\
            --export-als <OUT_ALS>              Export project to Ableton Live 12 gzip .als format\n\
            --split-stems <IN_WAV> <OUT_DIR>    Separate WAV file into 4 remix stems (drums, bass, vocals, other)\n\
            --scan-plugins [--output PATH]      Run out-of-process AU/VST3 plugin scanner\n\
            --mcp                               Run Model Context Protocol (MCP) server over stdio JSON-RPC 2.0\n\
            (default)                           Launch native single-window studio GUI\n"
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

/// Import Ableton Live 12 .als set, inspect contents, and optionally save JSON.
fn import_als_cli(in_als: &Path, out_json: Option<&Path>) {
    println!("============================================================");
    println!("  Oxide Studio Ableton Live 12 Set (.als) Importer");
    println!("============================================================");
    println!("• Input ALS:      {}", in_als.display());

    match DAWProject::load_als(in_als) {
        Ok(proj) => {
            println!("✓ Successfully imported Ableton Live Set:");
            println!("  - Project Name:   {}", proj.metadata.name);
            println!("  - Tempo (BPM):    {:.2}", proj.transport.bpm);
            println!(
                "  - Time Signature: {}/{}",
                proj.transport.time_signature_numerator, proj.transport.time_signature_denominator
            );
            println!("  - Tracks ({}):", proj.tracks.len());
            for t in &proj.tracks {
                println!("    * [{}] {} ({:?})", t.id, t.name, t.track_type);
            }
            if let Some(json_path) = out_json {
                if let Err(e) = proj.save_to_file(json_path) {
                    eprintln!("Error saving JSON project to {}: {}", json_path.display(), e);
                    exit(1);
                }
                println!("✓ Saved JSON project to: {}", json_path.display());
            }
        }
        Err(e) => {
            eprintln!("Error importing ALS file: {}", e);
            exit(1);
        }
    }
    println!("============================================================");
}

/// Export project to Ableton Live 12 gzip-compressed .als format.
fn export_als_cli(out_als: &Path) {
    println!("============================================================");
    println!("  Oxide Studio Ableton Live 12 Set (.als) Exporter");
    println!("============================================================");
    println!("• Target ALS:     {}", out_als.display());

    let mut project = DAWProject::new("Oxide Studio Session");
    project.transport.bpm = 126.0;

    let mut lead_synth = ableton_project::TrackDefinition::new_synth(1, "Lead Synth");
    lead_synth.mixer.volume_db = -2.5;
    lead_synth.mixer.pan = 0.0;
    project.tracks.push(lead_synth);

    let mut drums = ableton_project::TrackDefinition::new_drum_rack(2, "909 Drums");
    drums.mixer.volume_db = -1.0;
    project.tracks.push(drums);

    match project.save_als(out_als) {
        Ok(()) => {
            println!("✓ Export Succeeded: {}", out_als.display());
            println!("  - Tracks Exported: {}", project.tracks.len());
            println!("  - Gzip Compressed XML Magic: [0x1f, 0x8b]");
        }
        Err(e) => {
            eprintln!("Error exporting ALS file: {}", e);
            exit(1);
        }
    }
    println!("============================================================");
}

/// Separate WAV file into 4 remix stems (drums, bass, vocals, other).
fn split_stems_cli(in_wav: &Path, out_dir: &Path) {
    println!("============================================================");
    println!("  Oxide Studio 1-Click Remix Stem Extractor");
    println!("============================================================");
    println!("• Input WAV:      {}", in_wav.display());
    println!("• Output Dir:     {}", out_dir.display());

    match ableton_bounce::stems::export_remix_stems_from_wav(in_wav, out_dir) {
        Ok(files) => {
            println!("✓ 4-Stem Separation Succeeded:");
            println!("  - Drums:  {}", files.drums.display());
            println!("  - Bass:   {}", files.bass.display());
            println!("  - Vocals: {}", files.vocals.display());
            println!("  - Other:  {}", files.other.display());
        }
        Err(e) => {
            eprintln!("Error splitting stems: {}", e);
            exit(1);
        }
    }
    println!("============================================================");
}
