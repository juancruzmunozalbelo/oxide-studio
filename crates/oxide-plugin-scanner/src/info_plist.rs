use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use crate::types::{PluginCategory, PluginFormat, PluginParameter};

/// Parsed metadata extracted from a plugin bundle's Info.plist and Mach-O binary.
#[derive(Debug, Clone)]
pub struct BundleInfo {
    pub name: String,
    pub identifier: String,
    pub vendor: String,
    pub version: String,
    pub category: PluginCategory,
    pub executable_name: Option<String>,
    pub architectures: Vec<String>,
    pub parameters: Vec<PluginParameter>,
}

/// Extracts metadata from an AU (.component) or VST3 (.vst3) bundle.
pub fn extract_bundle_info(bundle_path: &Path, format: PluginFormat) -> BundleInfo {
    let default_name = bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let plist_path = bundle_path.join("Contents/Info.plist");
    let fallback_plist_path = bundle_path.join("Info.plist");

    let actual_plist = if plist_path.exists() {
        Some(plist_path)
    } else if fallback_plist_path.exists() {
        Some(fallback_plist_path)
    } else {
        None
    };

    let mut name = default_name.clone();
    let mut identifier = format!("local.{}.{}", format.as_str().to_lowercase(), default_name);
    let mut vendor = "Unknown".to_string();
    let mut version = "1.0.0".to_string();
    let mut category = PluginCategory::Effect;
    let mut category_determined = format == PluginFormat::AudioUnit;
    let mut executable_name = None;

    if let Some(ref plist_file) = actual_plist {
        let mut acc = PlistAccumulator {
            default_name: &default_name,
            name: &mut name,
            identifier: &mut identifier,
            vendor: &mut vendor,
            version: &mut version,
            category: &mut category,
            category_determined: &mut category_determined,
            executable_name: &mut executable_name,
        };

        if let Some(parsed) = parse_plist_json(plist_file) {
            apply_plist_json(&parsed, &mut acc);
        } else if let Ok(content) = fs::read_to_string(plist_file) {
            apply_plist_xml(&content, &mut acc);
        }
    }

    // For VST3 bundles, inspect moduleinfo.json if present for accurate Effect vs Instrument classification
    if format == PluginFormat::Vst3 {
        let moduleinfo_path = bundle_path.join("Contents/Resources/moduleinfo.json");
        let alt_moduleinfo_path = bundle_path.join("moduleinfo.json");
        let actual_moduleinfo = if moduleinfo_path.exists() {
            Some(moduleinfo_path)
        } else if alt_moduleinfo_path.exists() {
            Some(alt_moduleinfo_path)
        } else {
            None
        };

        if let Some(ref mi_path) = actual_moduleinfo {
            if let Ok(mi_content) = fs::read_to_string(mi_path) {
                if let Ok(mi_json) = serde_json::from_str::<serde_json::Value>(&mi_content) {
                    apply_vst3_moduleinfo(
                        &mi_json,
                        &mut name,
                        &mut vendor,
                        &mut version,
                        &mut category,
                        &mut category_determined,
                    );
                }
            }
        }

        // Heuristic fallback when neither Info.plist nor moduleinfo.json specified category
        if !category_determined {
            let lower_name = name.to_lowercase();
            let looks_like_fx = lower_name.contains("eq")
                || lower_name.contains("comp")
                || lower_name.contains("reverb")
                || lower_name.contains("delay")
                || lower_name.contains("filter")
                || lower_name.contains("limiter")
                || lower_name.contains("saturat")
                || lower_name.contains("chorus")
                || lower_name.contains("phaser")
                || lower_name.contains("flanger")
                || lower_name.contains("distort")
                || lower_name.contains("effect")
                || lower_name.contains("fx");
            let looks_like_inst = lower_name.contains("synth")
                || lower_name.contains("sampler")
                || lower_name.contains("instrument")
                || lower_name.contains("piano")
                || lower_name.contains("organ")
                || lower_name.contains("drum");
            if looks_like_inst && !looks_like_fx {
                category = PluginCategory::Instrument;
            } else {
                category = PluginCategory::Effect;
            }
        }
    }

    // Attempt to detect architectures from the binary
    let exec_candidate = executable_name
        .clone()
        .unwrap_or_else(|| default_name.clone());
    let binary_path = bundle_path.join("Contents/MacOS").join(&exec_candidate);
    let architectures = if binary_path.exists() {
        detect_macho_architectures(&binary_path)
    } else {
        vec![std::env::consts::ARCH.to_string()]
    };

    // Synthesize standard exposed parameters based on category
    let parameters = synthesize_parameters(category);

    BundleInfo {
        name,
        identifier,
        vendor,
        version,
        category,
        executable_name,
        architectures,
        parameters,
    }
}

/// Convert and parse Info.plist to JSON using macOS plutil.
fn parse_plist_json(plist_path: &Path) -> Option<serde_json::Value> {
    let output = Command::new("plutil")
        .arg("-convert")
        .arg("json")
        .arg("-o")
        .arg("-")
        .arg(plist_path)
        .output()
        .ok()?;

    if output.status.success() {
        serde_json::from_slice(&output.stdout).ok()
    } else {
        None
    }
}

struct PlistAccumulator<'a> {
    default_name: &'a str,
    name: &'a mut String,
    identifier: &'a mut String,
    vendor: &'a mut String,
    version: &'a mut String,
    category: &'a mut PluginCategory,
    category_determined: &'a mut bool,
    executable_name: &'a mut Option<String>,
}

fn apply_vst3_moduleinfo(
    val: &serde_json::Value,
    name: &mut String,
    vendor: &mut String,
    version: &mut String,
    category: &mut PluginCategory,
    category_determined: &mut bool,
) {
    if let Some(classes) = val.get("Classes").and_then(|v| v.as_array()) {
        for cls in classes {
            let cls_cat = cls.get("Category").and_then(|v| v.as_str()).unwrap_or("");
            if !cls_cat.is_empty() && cls_cat != "Audio Module Class" {
                continue;
            }
            if let Some(cls_name) = cls.get("Name").and_then(|v| v.as_str()) {
                if !cls_name.trim().is_empty() {
                    *name = cls_name.trim().to_string();
                }
            }
            if let Some(cls_vendor) = cls.get("Vendor").and_then(|v| v.as_str()) {
                if *vendor == "Unknown" && !cls_vendor.trim().is_empty() {
                    *vendor = cls_vendor.trim().to_string();
                }
            }
            if let Some(cls_ver) = cls.get("Version").and_then(|v| v.as_str()) {
                if !cls_ver.trim().is_empty() {
                    *version = cls_ver.trim().to_string();
                }
            }
            if let Some(sub_cats) = cls.get("Sub Categories").and_then(|v| v.as_array()) {
                for sc in sub_cats {
                    if let Some(s) = sc.as_str() {
                        let lower = s.to_lowercase();
                        if lower.contains("instrument") || lower.contains("synth") {
                            *category = PluginCategory::Instrument;
                            *category_determined = true;
                            return;
                        } else if lower.contains("fx")
                            || lower.contains("effect")
                            || lower.contains("dynamics")
                            || lower.contains("spatial")
                            || lower.contains("analyzer")
                            || lower.contains("mastering")
                        {
                            *category = PluginCategory::Effect;
                            *category_determined = true;
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn apply_plist_json(val: &serde_json::Value, acc: &mut PlistAccumulator<'_>) {
    if let Some(id) = val.get("CFBundleIdentifier").and_then(|v| v.as_str()) {
        *acc.identifier = id.to_string();
        // Infer vendor from reverse domain if available
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() >= 2 && *acc.vendor == "Unknown" {
            let candidate_vendor = parts[1];
            if !candidate_vendor.is_empty() {
                *acc.vendor = uppercase_first_letter(candidate_vendor);
            }
        }
    }

    if let Some(exec) = val.get("CFBundleExecutable").and_then(|v| v.as_str()) {
        *acc.executable_name = Some(exec.to_string());
    }

    if let Some(v) = val.get("CFBundleShortVersionString").and_then(|v| v.as_str()) {
        *acc.version = v.to_string();
    } else if let Some(v) = val.get("CFBundleVersion").and_then(|v| v.as_str()) {
        *acc.version = v.to_string();
    }

    if let Some(display_name) = val.get("CFBundleDisplayName").and_then(|v| v.as_str()) {
        *acc.name = display_name.to_string();
    } else if let Some(cf_name) = val.get("CFBundleName").and_then(|v| v.as_str()) {
        *acc.name = cf_name.to_string();
    } else {
        *acc.name = acc.default_name.to_string();
    }

    // Inspect AudioComponents array if present
    if let Some(components) = val.get("AudioComponents").and_then(|v| v.as_array()) {
        if let Some(first_comp) = components.first() {
            if let Some(comp_name) = first_comp.get("name").and_then(|v| v.as_str()) {
                if let Some((v_part, n_part)) = comp_name.split_once(':') {
                    *acc.vendor = v_part.trim().to_string();
                    *acc.name = n_part.trim().to_string();
                } else {
                    *acc.name = comp_name.trim().to_string();
                }
            }

            if let Some(comp_type) = first_comp.get("type").and_then(|v| v.as_str()) {
                match comp_type {
                    "aumu" | "aumi" | "augn" => {
                        *acc.category = PluginCategory::Instrument;
                        *acc.category_determined = true;
                    }
                    "aufx" | "aumf" | "auol" => {
                        *acc.category = PluginCategory::Effect;
                        *acc.category_determined = true;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn apply_plist_xml(xml: &str, acc: &mut PlistAccumulator<'_>) {
    if let Some(id) = extract_xml_string(xml, "CFBundleIdentifier") {
        *acc.identifier = id.clone();
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() >= 2 && *acc.vendor == "Unknown" {
            *acc.vendor = uppercase_first_letter(parts[1]);
        }
    }

    if let Some(exec) = extract_xml_string(xml, "CFBundleExecutable") {
        *acc.executable_name = Some(exec);
    }

    if let Some(v) = extract_xml_string(xml, "CFBundleShortVersionString") {
        *acc.version = v;
    } else if let Some(v) = extract_xml_string(xml, "CFBundleVersion") {
        *acc.version = v;
    }

    if let Some(dn) = extract_xml_string(xml, "CFBundleDisplayName") {
        *acc.name = dn;
    } else if let Some(cn) = extract_xml_string(xml, "CFBundleName") {
        *acc.name = cn;
    } else {
        *acc.name = acc.default_name.to_string();
    }

    if xml.contains("aumu") || xml.contains("aumi") {
        *acc.category = PluginCategory::Instrument;
        *acc.category_determined = true;
    } else if xml.contains("aufx") || xml.contains("aumf") {
        *acc.category = PluginCategory::Effect;
        *acc.category_determined = true;
    }
}

fn extract_xml_string(xml: &str, key: &str) -> Option<String> {
    let key_tag = format!("<key>{}</key>", key);
    let key_pos = xml.find(&key_tag)?;
    let after_key = &xml[key_pos + key_tag.len()..];
    let str_start = after_key.find("<string>")? + "<string>".len();
    let str_end = after_key[str_start..].find("</string>")? + str_start;
    Some(after_key[str_start..str_end].trim().to_string())
}

fn uppercase_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Detects binary architectures in a Mach-O file by inspecting header magic and cpu types.
pub fn detect_macho_architectures(path: &Path) -> Vec<String> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![std::env::consts::ARCH.to_string()],
    };

    let mut header = [0u8; 4096];
    let bytes_read = match file.read(&mut header) {
        Ok(n) if n >= 8 => n,
        _ => return vec![std::env::consts::ARCH.to_string()],
    };

    let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let mut archs = Vec::new();

    match magic {
        // Mach-O Universal / Fat Binary 32-bit (Big Endian)
        0xCAFEBABE => {
            let nfat = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
            let mut offset = 8;
            for _ in 0..nfat {
                if offset + 20 > bytes_read {
                    break;
                }
                let cputype = u32::from_be_bytes([header[offset], header[offset + 1], header[offset + 2], header[offset + 3]]);
                if let Some(arch_name) = map_cputype(cputype) {
                    if !archs.contains(&arch_name) {
                        archs.push(arch_name);
                    }
                }
                offset += 20;
            }
        }
        // Mach-O Universal / Fat Binary 64-bit (Big Endian)
        0xCAFEBABF => {
            let nfat = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
            let mut offset = 8;
            for _ in 0..nfat {
                if offset + 32 > bytes_read {
                    break;
                }
                let cputype = u32::from_be_bytes([header[offset], header[offset + 1], header[offset + 2], header[offset + 3]]);
                if let Some(arch_name) = map_cputype(cputype) {
                    if !archs.contains(&arch_name) {
                        archs.push(arch_name);
                    }
                }
                offset += 32;
            }
        }
        // Mach-O 64-bit (Big Endian)
        0xFEEDFACF => {
            let cputype = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
            if let Some(arch_name) = map_cputype(cputype) {
                archs.push(arch_name);
            }
        }
        // Mach-O 64-bit (Little Endian)
        0xCFFAEDFE => {
            let cputype = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
            if let Some(arch_name) = map_cputype(cputype) {
                archs.push(arch_name);
            }
        }
        _ => {}
    }

    if archs.is_empty() {
        vec![std::env::consts::ARCH.to_string()]
    } else {
        archs
    }
}

fn map_cputype(cputype: u32) -> Option<String> {
    match cputype {
        0x0100000C => Some("arm64".to_string()),
        0x01000007 => Some("x86_64".to_string()),
        7 => Some("i386".to_string()),
        12 => Some("arm".to_string()),
        _ => None,
    }
}

fn synthesize_parameters(category: PluginCategory) -> Vec<PluginParameter> {
    match category {
        PluginCategory::Instrument => vec![
            PluginParameter::new(0, "Volume", 0.0, 1.0, 0.8),
            PluginParameter::new(1, "Cutoff", 20.0, 20000.0, 2500.0),
            PluginParameter::new(2, "Resonance", 0.0, 1.0, 0.2),
            PluginParameter::new(3, "Attack", 0.001, 5.0, 0.01),
            PluginParameter::new(4, "Decay", 0.001, 5.0, 0.3),
            PluginParameter::new(5, "Sustain", 0.0, 1.0, 0.5),
            PluginParameter::new(6, "Release", 0.001, 10.0, 0.5),
            PluginParameter::new(7, "Pitch", -24.0, 24.0, 0.0),
        ],
        PluginCategory::Effect => vec![
            PluginParameter::new(0, "InputGain", -24.0, 24.0, 0.0),
            PluginParameter::new(1, "OutputGain", -24.0, 24.0, 0.0),
            PluginParameter::new(2, "DryWet", 0.0, 1.0, 0.5),
            PluginParameter::new(3, "Parameter1", 0.0, 1.0, 0.5),
            PluginParameter::new(4, "Parameter2", 0.0, 1.0, 0.5),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_real_asaf_channels() {
        let bundle_path = Path::new("/Library/Audio/Plug-Ins/Components/ASAF Channels.component");
        if bundle_path.exists() {
            let info = extract_bundle_info(bundle_path, PluginFormat::AudioUnit);
            assert_eq!(info.category, PluginCategory::Effect);
            assert!(info.name.contains("Channels"));
            assert!(info.vendor.contains("Apple"));
            assert!(info.architectures.contains(&"arm64".to_string()));
            assert!(info.parameters.len() >= 4);
        }
    }

    #[test]
    fn test_vst3_effect_vs_instrument_classification() {
        let base = std::env::temp_dir().join("oxide_vst3_classify_test");
        let _ = fs::remove_dir_all(&base);

        let fx_bundle = base.join("ValhallaVintageVerb.vst3");
        fs::create_dir_all(fx_bundle.join("Contents/Resources")).unwrap();
        fs::write(
            fx_bundle.join("Contents/Resources/moduleinfo.json"),
            r#"{"Classes":[{"Category":"Audio Module Class","Name":"ValhallaVintageVerb","Vendor":"Valhalla DSP","Version":"4.0.0","Sub Categories":["Fx|Reverb"]}]}"#,
        )
        .unwrap();
        let fx_info = extract_bundle_info(&fx_bundle, PluginFormat::Vst3);
        assert_eq!(fx_info.category, PluginCategory::Effect);
        assert_eq!(fx_info.vendor, "Valhalla DSP");

        let bare_fx_bundle = base.join("CustomProcessor.vst3");
        fs::create_dir_all(&bare_fx_bundle).unwrap();
        let bare_info = extract_bundle_info(&bare_fx_bundle, PluginFormat::Vst3);
        assert_eq!(bare_info.category, PluginCategory::Effect);

        let inst_bundle = base.join("Vital.vst3");
        fs::create_dir_all(inst_bundle.join("Contents/Resources")).unwrap();
        fs::write(
            inst_bundle.join("Contents/Resources/moduleinfo.json"),
            r#"{"Classes":[{"Category":"Audio Module Class","Name":"Vital","Vendor":"Matt Tytel","Version":"1.5.5","Sub Categories":["Instrument|Synth"]}]}"#,
        )
        .unwrap();
        let inst_info = extract_bundle_info(&inst_bundle, PluginFormat::Vst3);
        assert_eq!(inst_info.category, PluginCategory::Instrument);

        let _ = fs::remove_dir_all(&base);
    }
}
