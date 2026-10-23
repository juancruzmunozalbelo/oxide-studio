use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use oxide_core::{AudioBuffer, MidiEvent};
use oxide_plugin_scanner::{
    HostedPluginInstance, PluginCategory, PluginDescriptor, PluginFormat, PluginInstance,
    PluginManifest, PluginParameter, PluginScannerSupervisor,
};

#[test]
fn test_plugin_manifest_search_and_query() {
    let mut manifest = PluginManifest::new();

    let valid_desc = PluginDescriptor::new_valid(
        "Vital",
        "audio.vital.synth",
        "Matt Tytel",
        "1.5.5",
        PluginCategory::Instrument,
        PluginFormat::Vst3,
        PathBuf::from("/Library/Audio/Plug-Ins/VST3/Vital.vst3"),
        2,
        2,
        vec![PluginParameter::new(0, "Volume", 0.0, 1.0, 0.8)],
        vec!["arm64".into()],
        1000,
    );

    let quarantined_desc = PluginDescriptor::new_quarantined(
        "CrashingFilter",
        "quarantine.crash.bad",
        PluginFormat::AudioUnit,
        PathBuf::from("/Library/Audio/Plug-Ins/Components/Bad.component"),
        "SIGSEGV isolated during initialization",
        1000,
    );

    manifest.add_or_update(valid_desc.clone());
    manifest.add_or_update(quarantined_desc.clone());

    assert_eq!(manifest.plugins.len(), 2);
    assert_eq!(manifest.valid_plugins().count(), 1);
    assert_eq!(manifest.quarantined_plugins().count(), 1);

    assert_eq!(
        manifest.find_by_identifier("audio.vital.synth"),
        Some(&valid_desc)
    );
    assert_eq!(manifest.find_by_name("Vital"), Some(&valid_desc));
    assert_eq!(
        manifest.find_by_name("CrashingFilter"),
        Some(&quarantined_desc)
    );
    assert!(manifest.find_by_name("NonExistent").is_none());
}

#[test]
fn test_plugin_instance_buffer_routing_and_parameters() {
    let desc = PluginDescriptor::new_valid(
        "MonoSynth",
        "audio.test.monosynth",
        "TestVendor",
        "1.0",
        PluginCategory::Instrument,
        PluginFormat::Vst3,
        PathBuf::from("/Library/Audio/Plug-Ins/VST3/MonoSynth.vst3"),
        2,
        2,
        vec![
            PluginParameter::new(0, "Volume", 0.0, 1.0, 0.7),
            PluginParameter::new(1, "Cutoff", 20.0, 20000.0, 1000.0),
        ],
        vec!["arm64".into()],
        500,
    );

    let mut instance = HostedPluginInstance::new(desc).expect("Instance creation must succeed");
    assert_eq!(instance.parameter_count(), 2);
    assert_eq!(instance.get_parameter(0), Some(0.7));
    assert_eq!(instance.get_parameter(1), Some(1000.0));

    // Test parameter clamping
    assert!(instance.set_parameter(0, 1.5).is_ok());
    assert_eq!(instance.get_parameter(0), Some(1.0)); // Clamped to max 1.0

    assert!(instance.set_parameter(0, -0.5).is_ok());
    assert_eq!(instance.get_parameter(0), Some(0.0)); // Clamped to min 0.0

    // Reset volume to 0.8
    assert!(instance.set_parameter(0, 0.8).is_ok());

    assert!(instance.set_parameter(99, 1.0).is_err()); // Out of bounds index

    // Initialize and test audio buffer rendering
    instance.initialize(48000.0, 256).unwrap();

    let mut buffer = AudioBuffer::silence(256);
    let midi_on = vec![MidiEvent::NoteOn { channel: 0, note: 60, velocity: 120 }];
    instance.process(&mut buffer, &midi_on);

    let energy = buffer.left[0..256].iter().map(|&s| s * s).sum::<f32>();
    assert!(energy > 0.01, "Audio buffer must contain synthesized sound");

    // Test NoteOff decay
    let midi_off = vec![MidiEvent::NoteOff { channel: 0, note: 60 }];
    instance.process(&mut buffer, &midi_off);
}

#[test]
fn test_supervisor_scan_all_creates_and_loads_manifest() {
    let temp_manifest = std::env::temp_dir().join("ableton_test_scan_all_manifest.json");
    let _ = std::fs::remove_file(&temp_manifest);

    let supervisor = PluginScannerSupervisor::new()
        .with_timeout(Duration::from_secs(5))
        .with_manifest_cache_path(temp_manifest.clone());

    // First scan: discovers and persists
    let manifest1 = supervisor.scan_all();
    assert!(temp_manifest.exists(), "Manifest file must be created on disk");

    // Second scan: repeated scan should load from cache
    let start = Instant::now();
    let manifest2 = supervisor.scan_all();
    let elapsed = start.elapsed();

    assert_eq!(manifest1.plugins.len(), manifest2.plugins.len());
    assert!(
        elapsed < Duration::from_millis(15),
        "Repeated scan with cache must finish in <15ms, took {:?}",
        elapsed
    );

    let _ = std::fs::remove_file(temp_manifest);
}

#[test]
fn test_cli_binary_scan_single_and_quarantine() {
    // Check if target binary exists
    let bin_path = if PathBuf::from("target/debug/oxide-plugin-scanner").exists() {
        PathBuf::from("target/debug/oxide-plugin-scanner")
    } else {
        PathBuf::from("/Users/juancruz/ableton-rust/target/debug/oxide-plugin-scanner")
    };

    if !bin_path.exists() {
        return;
    }

    // 1. Valid bundle scan
    let valid_bundle = PathBuf::from("/Library/Audio/Plug-Ins/Components/ASAF Channels.component");
    if valid_bundle.exists() {
        let output = Command::new(&bin_path)
            .arg("--scan-single")
            .arg(&valid_bundle)
            .output()
            .expect("CLI binary execution failed");

        assert!(output.status.success());
        let desc: PluginDescriptor = serde_json::from_slice(&output.stdout)
            .expect("CLI output must be valid PluginDescriptor JSON");
        assert!(desc.is_valid);
        assert!(!desc.quarantined);
        assert_eq!(desc.format, PluginFormat::AudioUnit);
    }

    // 2. Simulated crash isolation via CLI flag
    let output_crash = Command::new(&bin_path)
        .arg("--simulate-crash")
        .output()
        .expect("CLI binary execution failed");

    // Fatal signal (SIGSEGV) causes failure
    assert!(!output_crash.status.success());
}
