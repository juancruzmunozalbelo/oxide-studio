use std::fs;
use std::path::{Path, PathBuf};
use crate::types::PluginFormat;

/// Candidate bundle discovered during filesystem traversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateBundle {
    pub path: PathBuf,
    pub format: PluginFormat,
}

/// Returns the standard macOS search directories for AU and VST3 plugins.
pub fn standard_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/Library/Audio/Plug-Ins/Components"),
        PathBuf::from("/Library/Audio/Plug-Ins/VST3"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        paths.push(PathBuf::from(format!("{}/Library/Audio/Plug-Ins/Components", home)));
        paths.push(PathBuf::from(format!("{}/Library/Audio/Plug-Ins/VST3", home)));
    }

    paths
}

/// Recursively discovers all `.component` (AudioUnit) and `.vst3` (VST3) bundles within search paths.
pub fn discover_candidate_bundles(search_paths: &[PathBuf]) -> Vec<CandidateBundle> {
    let mut candidates = Vec::new();
    for root in search_paths {
        if root.exists() {
            scan_dir_recursive(root, 0, 8, &mut candidates);
        }
    }
    // Deduplicate and sort for deterministic ordering
    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    candidates.dedup_by(|a, b| a.path == b.path);
    candidates
}

fn scan_dir_recursive(
    dir: &Path,
    current_depth: usize,
    max_depth: usize,
    results: &mut Vec<CandidateBundle>,
) {
    if current_depth > max_depth {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        
        // Skip hidden files/directories (starting with dot)
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if file_name.starts_with('.') {
                continue;
            }
        }

        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if ext_lower == "component" {
                results.push(CandidateBundle {
                    path,
                    format: PluginFormat::AudioUnit,
                });
                // Do not recurse into the bundle itself
                continue;
            } else if ext_lower == "vst3" {
                results.push(CandidateBundle {
                    path,
                    format: PluginFormat::Vst3,
                });
                // Do not recurse into the bundle itself
                continue;
            }
        }

        // If it's a directory (and not a bundle), traverse recursively
        if path.is_dir() {
            scan_dir_recursive(&path, current_depth + 1, max_depth, results);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_search_paths() {
        let paths = standard_search_paths();
        assert!(paths.len() >= 2);
        assert!(paths.iter().any(|p| p.ends_with("Plug-Ins/Components")));
        assert!(paths.iter().any(|p| p.ends_with("Plug-Ins/VST3")));
    }

    #[test]
    fn test_discover_system_components() {
        let paths = vec![PathBuf::from("/Library/Audio/Plug-Ins/Components")];
        let candidates = discover_candidate_bundles(&paths);
        if Path::new("/Library/Audio/Plug-Ins/Components").exists() {
            // Should find Apple ASAF components or other system plugins
            assert!(!candidates.is_empty());
            assert!(candidates.iter().all(|c| c.format == PluginFormat::AudioUnit));
        }
    }
}
