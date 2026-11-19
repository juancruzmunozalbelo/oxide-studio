use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::discovery::{discover_candidate_bundles, standard_search_paths};
use crate::info_plist::try_extract_bundle_info;
use crate::types::{PluginCategory, PluginDescriptor, PluginFormat, PluginManifest};

/// Supervisor managing out-of-process plugin scanning, crash isolation, and manifest caching.
pub struct PluginScannerSupervisor {
    pub timeout: Duration,
    pub search_paths: Vec<PathBuf>,
    pub manifest_cache_path: Option<PathBuf>,
}

impl PluginScannerSupervisor {
    /// Create a supervisor configured with standard macOS search paths and a 10-second timeout.
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            search_paths: standard_search_paths(),
            manifest_cache_path: Some(std::env::temp_dir().join("oxide_plugin_manifest.json")),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_search_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.search_paths = paths;
        self
    }

    pub fn with_manifest_cache_path(mut self, path: PathBuf) -> Self {
        self.manifest_cache_path = Some(path);
        self
    }

    /// Discovers candidate bundles in configured search paths, returning pairs of (Path, Format).
    pub fn discover_candidate_bundles(&self) -> Vec<(PathBuf, PluginFormat)> {
        let candidates = discover_candidate_bundles(&self.search_paths);
        candidates.into_iter().map(|c| (c.path, c.format)).collect()
    }

    /// Isolated worker spawn with timeout and crash capture using a custom command builder.
    pub fn scan_plugin_isolated<F>(
        &self,
        plugin_path: &Path,
        format: PluginFormat,
        worker_cmd_builder: F,
    ) -> PluginDescriptor
    where
        F: FnOnce(&Path) -> Command,
    {
        self.scan_plugin_isolated_result(plugin_path, format, |path| Ok(worker_cmd_builder(path)))
    }

    fn scan_plugin_isolated_result<F>(
        &self,
        plugin_path: &Path,
        format: PluginFormat,
        worker_cmd_builder: F,
    ) -> PluginDescriptor
    where
        F: FnOnce(&Path) -> Result<Command, String>,
    {
        let name = plugin_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let mtime_secs = get_path_mtime(plugin_path);

        let mut cmd = match worker_cmd_builder(plugin_path) {
            Ok(c) => c,
            Err(err_msg) => {
                return quarantined_descriptor(
                    name,
                    format!("local.quarantine.{}", plugin_path.display()),
                    format,
                    plugin_path,
                    err_msg,
                    mtime_secs,
                );
            }
        };

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to spawn worker process: {}", e);
                return quarantined_descriptor(
                    name,
                    format!("local.quarantine.{}", plugin_path.display()),
                    format,
                    plugin_path,
                    err_msg,
                    mtime_secs,
                );
            }
        };

        // Drain stdout and stderr concurrently so child processes writing >64KB do not deadlock
        // before wait/try_wait can observe process exit.
        let stdout_handle = child.stdout.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = pipe.read_to_end(&mut buf);
                buf
            })
        });
        let stderr_handle = child.stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = pipe.read_to_end(&mut buf);
                buf
            })
        });

        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let stdout_bytes = stdout_handle
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();
                    let stderr_bytes = stderr_handle
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();

                    if status.success() {
                        // If worker emitted valid JSON PluginDescriptor, parse it
                        if let Ok(mut desc) =
                            serde_json::from_slice::<PluginDescriptor>(&stdout_bytes)
                        {
                            desc.mtime_secs = mtime_secs;
                            return desc;
                        }

                        // Otherwise extract info directly from bundle
                        match try_extract_bundle_info(plugin_path, format) {
                            Ok(bundle_info) => {
                                let is_inst = bundle_info.category.is_instrument();
                                let p_count = bundle_info.parameters.len() as u32;

                                return PluginDescriptor {
                                    name: bundle_info.name,
                                    identifier: bundle_info.identifier,
                                    vendor: bundle_info.vendor,
                                    version: bundle_info.version,
                                    category: bundle_info.category,
                                    format,
                                    path: plugin_path.to_path_buf(),
                                    is_instrument: is_inst,
                                    is_valid: true,
                                    quarantined: false,
                                    quarantine_reason: None,
                                    error_message: None,
                                    num_inputs: 2,
                                    num_outputs: 2,
                                    num_parameters: p_count,
                                    parameters: bundle_info.parameters,
                                    architectures: bundle_info.architectures,
                                    mtime_secs,
                                };
                            }
                            Err(err_msg) => {
                                return quarantined_descriptor(
                                    name,
                                    format!("quarantine.plist.{}", plugin_path.display()),
                                    format,
                                    plugin_path,
                                    err_msg,
                                    mtime_secs,
                                );
                            }
                        }
                    } else {
                        let stderr_note = String::from_utf8_lossy(&stderr_bytes);
                        let err_msg = if stderr_note.trim().is_empty() {
                            format!(
                                "Plugin scanner crashed with exit status: {:?} (SIGSEGV/SIGBUS isolated)",
                                status
                            )
                        } else {
                            format!(
                                "Plugin scanner crashed with exit status: {:?} (SIGSEGV/SIGBUS isolated): {}",
                                status,
                                stderr_note.trim()
                            )
                        };
                        return quarantined_descriptor(
                            name,
                            format!("quarantine.crash.{}", plugin_path.display()),
                            format,
                            plugin_path,
                            err_msg,
                            mtime_secs,
                        );
                    }
                }
                Ok(None) => {
                    if start.elapsed() > self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let err_msg =
                            "Plugin scanner timed out (infinite loop during init)".to_string();
                        return quarantined_descriptor(
                            name,
                            format!("quarantine.timeout.{}", plugin_path.display()),
                            format,
                            plugin_path,
                            err_msg,
                            mtime_secs,
                        );
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    let err_msg = format!("Process wait error: {}", e);
                    return quarantined_descriptor(
                        name,
                        format!("quarantine.error.{}", plugin_path.display()),
                        format,
                        plugin_path,
                        err_msg,
                        mtime_secs,
                    );
                }
            }
        }
    }

    /// Default isolated scan using the standalone `oxide-plugin-scanner` worker binary.
    /// Returns an explicit quarantined error descriptor if no isolated worker binary is available.
    pub fn scan_plugin(&self, plugin_path: &Path, format: PluginFormat) -> PluginDescriptor {
        self.scan_plugin_isolated_result(plugin_path, format, build_default_worker_command)
    }

    /// Full discovery and scan orchestration across configured paths with mtime cache acceleration.
    pub fn scan_all(&self) -> PluginManifest {
        let candidates = self.discover_candidate_bundles();
        let mut manifest = if let Some(ref cache_path) = self.manifest_cache_path {
            PluginManifest::load_from_file(cache_path).unwrap_or_else(|_| PluginManifest::new())
        } else {
            PluginManifest::new()
        };

        manifest.scan_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for (path, format) in candidates {
            let current_mtime = get_path_mtime(&path);
            
            // Check cache hit: path exists and mtime matches
            let is_cache_hit = if let Some(cached) = manifest.find_by_path(&path) {
                cached.mtime_secs > 0 && cached.mtime_secs == current_mtime
            } else {
                false
            };

            if is_cache_hit {
                // Already in manifest and up-to-date, keep existing entry
                continue;
            }

            // Perform isolated out-of-process scan
            let desc = self.scan_plugin(&path, format);
            manifest.add_or_update(desc);
        }

        // Persist updated manifest
        if let Some(ref cache_path) = self.manifest_cache_path {
            let _ = manifest.save_to_file(cache_path);
        }

        manifest
    }
}

impl Default for PluginScannerSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

fn get_path_mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn quarantined_descriptor(
    name: String,
    identifier: String,
    format: PluginFormat,
    plugin_path: &Path,
    err_msg: String,
    mtime_secs: u64,
) -> PluginDescriptor {
    PluginDescriptor {
        name,
        identifier,
        vendor: "Unknown".into(),
        version: "0.0".into(),
        category: PluginCategory::Effect,
        format,
        path: plugin_path.to_path_buf(),
        is_instrument: false,
        is_valid: false,
        quarantined: true,
        quarantine_reason: Some(err_msg.clone()),
        error_message: Some(err_msg),
        num_inputs: 0,
        num_outputs: 0,
        num_parameters: 0,
        parameters: Vec::new(),
        architectures: Vec::new(),
        mtime_secs,
    }
}

fn build_default_worker_command(plugin_path: &Path) -> Result<Command, String> {
    let env_bin = std::env::var("OXIDE_SCANNER_BIN").ok();
    let current_exe = std::env::current_exe().ok();
    build_default_worker_command_with(
        plugin_path,
        env_bin.as_deref(),
        current_exe.as_deref(),
    )
}

fn build_default_worker_command_with(
    plugin_path: &Path,
    explicit_env_bin: Option<&str>,
    current_exe: Option<&Path>,
) -> Result<Command, String> {
    // If an explicit scanner binary path is provided via environment
    if let Some(bin_path) = explicit_env_bin.filter(|s| !s.trim().is_empty()) {
        let mut cmd = Command::new(bin_path);
        cmd.arg("--scan-single").arg(plugin_path);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        return Ok(cmd);
    }

    // Try finding the compiled oxide-plugin-scanner binary in current or parent target directory
    if let Some(exe) = current_exe {
        if let Some(bin_dir) = exe.parent() {
            let candidates = [
                Some(bin_dir.join("oxide-plugin-scanner")),
                bin_dir.parent().map(|p| p.join("oxide-plugin-scanner")),
            ];
            for candidate_bin in candidates.into_iter().flatten() {
                if candidate_bin.is_file() {
                    let mut cmd = Command::new(candidate_bin);
                    cmd.arg("--scan-single").arg(plugin_path);
                    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
                    return Ok(cmd);
                }
            }
        }
    }

    Err(format!(
        "Isolated plugin scanner worker binary 'oxide-plugin-scanner' unavailable (set OXIDE_SCANNER_BIN or build the scanner binary) while scanning '{}'",
        plugin_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supervisor_default_cache_uses_temp_dir_and_safe_argv() {
        let supervisor = PluginScannerSupervisor::new();
        let cache = supervisor.manifest_cache_path.as_ref().expect("default cache path");
        assert!(
            cache.starts_with(std::env::temp_dir()),
            "Manifest cache should live under temp_dir, got {:?}",
            cache
        );

        let marker = std::env::temp_dir().join("oxide_shell_injection_marker");
        let _ = fs::remove_file(&marker);
        let malicious = PathBuf::from(format!("/nonexistent'; touch {}; '", marker.display()));
        match build_default_worker_command(&malicious) {
            Ok(mut cmd) => {
                let program = cmd.get_program().to_string_lossy().to_string();
                assert_ne!(program, "/bin/sh");
                assert_ne!(program, "sh");
                assert_ne!(program, "/bin/test");
                let _ = cmd.output();
            }
            Err(err) => {
                assert!(err.contains("oxide-plugin-scanner"));
            }
        }
        assert!(!marker.exists(), "Shell metacharacters must never execute");
    }

    #[test]
    fn test_missing_worker_binary_returns_explicit_error_without_fake_fallback() {
        let fake_bundle = std::env::temp_dir().join("FakeBundle.vst3");
        let _ = fs::create_dir_all(&fake_bundle);

        let err = build_default_worker_command_with(
            &fake_bundle,
            None,
            Some(Path::new("/nonexistent_oxide_dir/deps/oxide_plugin_scanner_test")),
        )
        .expect_err("Missing worker binary must return Err rather than /bin/test -d fake success");
        assert!(err.contains("unavailable"));

        let supervisor = PluginScannerSupervisor::new();
        let desc = supervisor.scan_plugin_isolated_result(&fake_bundle, PluginFormat::Vst3, |p| {
            build_default_worker_command_with(
                p,
                None,
                Some(Path::new("/nonexistent_oxide_dir/deps/oxide_plugin_scanner_test")),
            )
        });
        assert!(!desc.is_valid);
        assert!(desc.quarantined);
        assert!(desc
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("unavailable"));

        let _ = fs::remove_dir_all(&fake_bundle);
    }

    #[test]
    fn test_scan_plugin_isolated_drains_large_stdout_without_deadlock() {
        let supervisor = PluginScannerSupervisor::new().with_timeout(Duration::from_secs(2));
        let fake_plugin = std::env::temp_dir().join("LargeOutputPlugin.component");

        // Emit >128KB on stderr and >64KB whitespace + JSON on stdout so an undrained 64KB pipe would deadlock.
        let desc = supervisor.scan_plugin_isolated(&fake_plugin, PluginFormat::AudioUnit, |_p| {
            let mut cmd = Command::new("python3");
            cmd.arg("-c").arg(
                "import sys, json; sys.stderr.write('E' * 131072); sys.stderr.flush(); sys.stdout.write(' ' * 131072); sys.stdout.write(json.dumps({'name':'PipeOk','identifier':'com.oxide.PipeOk','vendor':'Oxide','version':'1.0','category':'Effect','format':'AudioUnit','path':'/tmp/LargeOutputPlugin.component','is_instrument':False,'is_valid':True,'quarantined':False,'quarantine_reason':None,'error_message':None,'num_inputs':2,'num_outputs':2,'num_parameters':0,'parameters':[],'architectures':['arm64'],'mtime_secs':0})); sys.stdout.flush()"
            );
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd
        });

        assert!(
            desc.is_valid && !desc.quarantined,
            "Child emitting >128KB on stdout/stderr must drain cleanly without deadlock, got error: {:?}",
            desc.error_message
        );
        assert_eq!(desc.name, "PipeOk");
    }
}
