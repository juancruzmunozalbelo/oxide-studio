//! Out-of-process AU/VST3 plugin scanner, crash isolation supervisor, and hosting abstractions.
//!
//! Inspired by Ableton Live 12's `Ableton Plugin Scanner`.

pub mod types;
pub mod discovery;
pub mod info_plist;
pub mod supervisor;
pub mod instance;

pub use types::{
    PluginCategory, PluginDescriptor, PluginFormat, PluginManifest, PluginParameter,
};
pub use discovery::{discover_candidate_bundles, standard_search_paths, CandidateBundle};
pub use info_plist::{detect_macho_architectures, extract_bundle_info, BundleInfo};
pub use supervisor::PluginScannerSupervisor;
pub use instance::{HostedPluginInstance, PluginInstance};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn test_discover_standard_directories() {
        let supervisor = PluginScannerSupervisor::new();
        let candidates = supervisor.discover_candidate_bundles();

        let comp_dir = Path::new("/Library/Audio/Plug-Ins/Components");
        if comp_dir.exists() {
            assert!(
                !candidates.is_empty(),
                "Must discover installed macOS components in /Library/Audio/Plug-Ins/Components"
            );
        }
    }

    #[test]
    fn test_crash_isolation_sigsegv_survival() {
        let supervisor = PluginScannerSupervisor::new();
        let fake_plugin = PathBuf::from("/Library/Audio/Plug-Ins/Components/FaultyMemoryLeak.component");

        let desc = supervisor.scan_plugin_isolated(&fake_plugin, PluginFormat::AudioUnit, |_path| {
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-c").arg("kill -SEGV $$");
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd
        });

        assert!(!desc.is_valid, "Crashing plugin must not be marked valid");
        assert!(desc.quarantined, "Crashing plugin must be quarantined");
        assert!(desc.error_message.is_some(), "Must have error message");
        let msg = desc.error_message.unwrap();
        assert!(
            msg.contains("crashed") || msg.contains("SIGSEGV") || msg.contains("signal"),
            "Error message must record crash isolation"
        );
    }

    #[test]
    fn test_timeout_isolation_on_hung_process() {
        let mut supervisor = PluginScannerSupervisor::new();
        supervisor.timeout = Duration::from_millis(250);

        let hung_plugin = PathBuf::from("/Library/Audio/Plug-Ins/VST3/InfiniteLoop.vst3");

        let start = Instant::now();
        let desc = supervisor.scan_plugin_isolated(&hung_plugin, PluginFormat::Vst3, |_path| {
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-c").arg("sleep 10");
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd
        });
        let elapsed = start.elapsed();

        assert!(!desc.is_valid);
        assert!(desc.quarantined);
        assert!(elapsed < Duration::from_secs(2));
        let msg = desc.error_message.unwrap();
        assert!(msg.contains("timed out"));
    }

    #[test]
    fn test_manifest_mtime_caching_speed() {
        let mut manifest = PluginManifest::new();
        let candidate_path = PathBuf::from("/Library/Audio/Plug-Ins/Components/ASAF Channels.component");

        let mtime = if candidate_path.exists() {
            std::fs::metadata(&candidate_path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(1000)
        } else {
            1000
        };

        manifest.add_or_update(PluginDescriptor::new_valid(
            "ASAF Channels",
            "com.Apple.ASAFChannels",
            "Apple",
            "2.1.13",
            PluginCategory::Effect,
            PluginFormat::AudioUnit,
            candidate_path.clone(),
            2,
            2,
            vec![PluginParameter::new(0, "InputGain", -24.0, 24.0, 0.0)],
            vec!["arm64".into(), "x86_64".into()],
            mtime,
        ));

        // Save manifest to temporary path
        let temp_manifest_path = std::env::temp_dir().join("test_plugin_manifest_cache.json");
        manifest.save_to_file(&temp_manifest_path).unwrap();

        // Benchmark reload speed
        let start = Instant::now();
        let loaded = PluginManifest::load_from_file(&temp_manifest_path).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(loaded.plugins.len(), 1);
        assert_eq!(loaded.plugins[0].name, "ASAF Channels");
        assert!(
            elapsed < Duration::from_millis(5),
            "Manifest cache reload must take < 5ms, took {:?}",
            elapsed
        );

        let _ = std::fs::remove_file(temp_manifest_path);
    }
}
