use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::discovery::{discover_candidate_bundles, standard_search_paths};
use crate::info_plist::extract_bundle_info;
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
            manifest_cache_path: Some(PathBuf::from("plugin_manifest.json")),
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
        let name = plugin_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let mtime_secs = get_path_mtime(plugin_path);

        let mut child = match worker_cmd_builder(plugin_path).spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to spawn worker process: {}", e);
                return PluginDescriptor {
                    name: name.clone(),
                    identifier: format!("local.quarantine.{}", plugin_path.display()),
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
                };
            }
        };

        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        let output = child.wait_with_output().ok();
                        let stdout_bytes = output.map(|o| o.stdout).unwrap_or_default();

                        // If worker emitted valid JSON PluginDescriptor, parse it
                        if let Ok(mut desc) = serde_json::from_slice::<PluginDescriptor>(&stdout_bytes) {
                            desc.mtime_secs = mtime_secs;
                            return desc;
                        }

                        // Otherwise extract info directly from bundle
                        let bundle_info = extract_bundle_info(plugin_path, format);
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
                    } else {
                        // Subprocess crashed (SIGSEGV / SIGBUS or error exit)
                        let err_msg = format!(
                            "Plugin scanner crashed with exit status: {:?} (SIGSEGV/SIGBUS isolated)",
                            status
                        );
                        return PluginDescriptor {
                            name: name.clone(),
                            identifier: format!("quarantine.crash.{}", plugin_path.display()),
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
                        };
                    }
                }
                Ok(None) => {
                    if start.elapsed() > self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let err_msg = "Plugin scanner timed out (infinite loop during init)".to_string();
                        return PluginDescriptor {
                            name: name.clone(),
                            identifier: format!("quarantine.timeout.{}", plugin_path.display()),
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
                        };
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    let err_msg = format!("Process wait error: {}", e);
                    return PluginDescriptor {
                        name: name.clone(),
                        identifier: format!("quarantine.error.{}", plugin_path.display()),
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
                    };
                }
            }
        }
    }

    /// Default isolated scan using the standalone worker binary or an isolated shell probe.
    pub fn scan_plugin(&self, plugin_path: &Path, format: PluginFormat) -> PluginDescriptor {
        self.scan_plugin_isolated(plugin_path, format, |path| {
            build_default_worker_command(path)
        })
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

fn build_default_worker_command(plugin_path: &Path) -> Command {
    // If an explicit scanner binary path is provided via environment
    if let Ok(bin_path) = std::env::var("OXIDE_SCANNER_BIN") {
        let mut cmd = Command::new(bin_path);
        cmd.arg("--scan-single").arg(plugin_path);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        return cmd;
    }

    // Try finding the compiled oxide-plugin-scanner binary in current target directory
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(bin_dir) = current_exe.parent() {
            let candidate_bin = bin_dir.join("oxide-plugin-scanner");
            if candidate_bin.exists() {
                let mut cmd = Command::new(candidate_bin);
                cmd.arg("--scan-single").arg(plugin_path);
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
                return cmd;
            }
        }
    }

    // Fallback: spawn isolated shell command verifying the bundle structure
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(format!(
        "test -d '{}' && exit 0 || exit 1",
        plugin_path.display()
    ));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}
