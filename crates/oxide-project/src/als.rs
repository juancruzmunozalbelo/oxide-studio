//! Bidirectional Ableton Live 12 `.als` Live Set XML generation, gzip compression/decompression, and parsing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use oxide_timeline::{
    ArrangementClip, AutomationCurveType, AutomationLane, AutomationPoint, ClipPattern,
    CubicBezierHandle, DrumStepPattern, MidiNote, MidiPattern, Scene, SessionClip,
};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::DAWProjectError;
use crate::mixer::{InsertEffectConfig, MasterTrackDefinition, MixerState};
use crate::project::{DAWProject, ProjectMetadata, TransportConfig};
use crate::synth::{load_all_factory_macros, load_all_factory_presets, SynthPatchParameters};
use crate::track::{InstrumentConfig, TrackDefinition, TrackType};

pub const MAX_ALS_XML_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB cap

/// Ableton Live unity fader value (`0.0 dB`).
pub const ABLETON_UNITY_LINEAR: f32 = 1.0;
/// Ableton Live maximum (`+6.0 dB`) fader/send linear value.
pub const ABLETON_MAX_LINEAR: f32 = 1.99526238;
/// Ableton Live silence (`-70.0 dB` / `-inf`) fader/send linear value.
pub const ABLETON_SILENCE_LINEAR: f32 = 0.00031622776;
pub const ABLETON_MIN_DB: f32 = -70.0;
pub const ABLETON_MAX_DB: f32 = 6.0;

/// Convert Oxide dB (`[-70.0, +6.0]`) to Ableton Live `.als` linear amplitude `10^(dB/20)`:
/// - `0.0 dB` -> `1.0`
/// - `-70.0 dB` -> `0.00031622776`
/// - `+6.0 dB` -> `1.99526238`
/// Invertible with [`ableton_linear_to_db`].
pub fn db_to_ableton_linear(db: f32) -> f32 {
    if !db.is_finite() || db <= ABLETON_MIN_DB {
        return ABLETON_SILENCE_LINEAR;
    }
    if db >= ABLETON_MAX_DB {
        return ABLETON_MAX_LINEAR;
    }
    if db == 0.0 {
        return ABLETON_UNITY_LINEAR;
    }
    (10.0_f64.powf((db as f64) / 20.0) as f32).clamp(ABLETON_SILENCE_LINEAR, ABLETON_MAX_LINEAR)
}

/// Convert Ableton Live `.als` linear amplitude (`10^(dB/20)`) to Oxide dB (`[-70.0, +6.0]`):
/// - `1.0` -> `0.0 dB`
/// - `<= 0.00031622776` -> `-70.0 dB`
/// - `>= 1.99526238` -> `+6.0 dB`
/// Invertible with [`db_to_ableton_linear`].
pub fn ableton_linear_to_db(lin: f32) -> f32 {
    if !lin.is_finite() || lin <= ABLETON_SILENCE_LINEAR {
        return ABLETON_MIN_DB;
    }
    if lin >= ABLETON_MAX_LINEAR {
        return ABLETON_MAX_DB;
    }
    if ((lin as f64) - 1.0).abs() < 1e-9 {
        return 0.0;
    }
    (20.0 * (lin as f64).log10()).clamp(ABLETON_MIN_DB as f64, ABLETON_MAX_DB as f64) as f32
}

fn format_ableton_linear(lin: f32) -> String {
    if lin < 0.01 {
        format!("{:.11}", lin)
    } else {
        format!("{:.7}", lin)
    }
}

fn sanitize_xml_comment(s: &str) -> String {
    s.replace("--", "\\u002d\\u002d")
}

/// Escapes standard XML characters in text content and attribute values.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// Generic node representation of an XML element for parsing.
#[derive(Debug, Clone, Default)]
pub struct XmlNode {
    pub tag: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<XmlNode>,
    pub text: String,
}

impl XmlNode {
    pub fn get_attr(&self, name: &str) -> Option<String> {
        self.attributes.get(name).cloned()
    }

    pub fn get_attr_or_text(&self, name: &str) -> Option<String> {
        if let Some(val) = self.attributes.get(name) {
            return Some(val.clone());
        }
        if !self.text.trim().is_empty() {
            return Some(self.text.trim().to_string());
        }
        None
    }

    pub fn find_child(&self, tag: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.tag == tag)
    }

    pub fn find_children(&self, tag: &str) -> Vec<&XmlNode> {
        self.children.iter().filter(|c| c.tag == tag).collect()
    }

    pub fn find_descendant(&self, tag: &str) -> Option<&XmlNode> {
        if self.tag == tag {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_descendant(tag) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_descendants(&self, tag: &str) -> Vec<&XmlNode> {
        let mut results = Vec::new();
        self.collect_descendants(tag, &mut results);
        results
    }

    fn collect_descendants<'a>(&'a self, tag: &str, out: &mut Vec<&'a XmlNode>) {
        if self.tag == tag {
            out.push(self);
        }
        for child in &self.children {
            child.collect_descendants(tag, out);
        }
    }
}

/// Parse an XML string into a hierarchical `XmlNode` tree using quick-xml.
pub fn parse_xml_to_node(xml: &str) -> Result<XmlNode, DAWProjectError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut attrs = HashMap::new();
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let val = attr.unescape_value().unwrap_or_default().to_string();
                    attrs.insert(key, val);
                }
                stack.push(XmlNode {
                    tag,
                    attributes: attrs,
                    children: Vec::new(),
                    text: String::new(),
                });
            }
            Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut attrs = HashMap::new();
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                    let val = attr.unescape_value().unwrap_or_default().to_string();
                    attrs.insert(key, val);
                }
                let node = XmlNode {
                    tag,
                    attributes: attrs,
                    children: Vec::new(),
                    text: String::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            }
            Ok(Event::End(_)) => {
                if let Some(node) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        root = Some(node);
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if let Some(current) = stack.last_mut() {
                    let text = e.unescape().unwrap_or_default().to_string();
                    if !text.is_empty() {
                        current.text.push_str(&text);
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                if let Some(current) = stack.last_mut() {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    if !text.is_empty() {
                        current.text.push_str(&text);
                    }
                }
            }
            Ok(Event::Comment(ref e)) => {
                if let Some(current) = stack.last_mut() {
                    let comment = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                    if !comment.is_empty() {
                        current.children.push(XmlNode {
                            tag: "__comment__".to_string(),
                            attributes: HashMap::new(),
                            children: Vec::new(),
                            text: comment,
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(DAWProjectError::Validation(format!(
                    "XML parse error: {}",
                    e
                )))
            }
            _ => {}
        }
        buf.clear();
    }

    root.ok_or_else(|| {
        DAWProjectError::Validation("Empty XML document or missing root element".to_string())
    })
}

/// Read all PCM samples from a standard RIFF WAV file without truncation.
/// Returns `(sample_rate, channels, interleaved_stereo_samples)`.
pub fn read_wav_full<P: AsRef<Path>>(path: P) -> Option<(u32, u16, Vec<f32>)> {
    read_wav_preview(path, 0.0)
}

/// Read PCM samples from a standard RIFF WAV file, capped to `max_seconds` (if `> 0.0`).
/// Returns `(sample_rate, channels, interleaved_stereo_samples)`.
pub fn read_wav_preview<P: AsRef<Path>>(path: P, max_seconds: f64) -> Option<(u32, u16, Vec<f32>)> {
    read_wav_preview_checked(path, max_seconds).ok()
}

/// Read PCM samples from a standard RIFF WAV file with explicit error reporting.
/// Returns `(sample_rate, 2, interleaved_stereo_samples)`.
pub fn read_wav_preview_checked<P: AsRef<Path>>(
    path: P,
    max_seconds: f64,
) -> Result<(u32, u16, Vec<f32>), DAWProjectError> {
    let bytes = fs::read(path)?;
    decode_wav_bytes(&bytes, max_seconds)
}

/// Decode PCM / IEEE-float WAV bytes into interleaved stereo `f32` samples.
/// Handles arbitrary RIFF chunk ordering (`"data"` before `"fmt "`), rejects non-PCM formats
/// (`audio_format != 1 && audio_format != 3`), and rejects `channels == 0` or `sample_rate == 0`.
pub fn decode_wav_bytes(
    bytes: &[u8],
    max_seconds: f64,
) -> Result<(u32, u16, Vec<f32>), DAWProjectError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(DAWProjectError::Validation(
            "Invalid RIFF/WAVE header".to_string(),
        ));
    }

    let mut pos = 12usize;
    let mut fmt_info: Option<(u16, u16, u32, u16)> = None;
    let mut data_range: Option<(usize, usize)> = None;

    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(
            bytes[pos + 4..pos + 8]
                .try_into()
                .map_err(|_| DAWProjectError::Validation("Truncated WAV chunk header".to_string()))?,
        ) as usize;
        pos += 8;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || pos + 16 > bytes.len() {
                return Err(DAWProjectError::Validation(
                    "Truncated or malformed WAV fmt chunk".to_string(),
                ));
            }
            let mut audio_format = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
            let channels = u16::from_le_bytes([bytes[pos + 2], bytes[pos + 3]]);
            let sample_rate = u32::from_le_bytes([
                bytes[pos + 4],
                bytes[pos + 5],
                bytes[pos + 6],
                bytes[pos + 7],
            ]);
            let bits_per_sample = u16::from_le_bytes([bytes[pos + 14], bytes[pos + 15]]);
            if audio_format == 0xFFFE && chunk_size >= 40 && pos + 26 <= bytes.len() {
                audio_format = u16::from_le_bytes([bytes[pos + 24], bytes[pos + 25]]);
            }
            if audio_format != 1 && audio_format != 3 {
                return Err(DAWProjectError::Validation(format!(
                    "Unsupported non-PCM WAV audio_format={audio_format}"
                )));
            }
            if channels == 0 || sample_rate == 0 {
                return Err(DAWProjectError::Validation(format!(
                    "Invalid WAV format: channels={channels}, sample_rate={sample_rate}"
                )));
            }
            fmt_info = Some((audio_format, channels, sample_rate, bits_per_sample));
        } else if chunk_id == b"data" {
            let avail = bytes.len().saturating_sub(pos);
            let data_len = chunk_size.min(avail);
            data_range = Some((pos, data_len));
        }

        if fmt_info.is_some() && data_range.is_some() {
            break;
        }
        let padded = chunk_size.saturating_add(chunk_size & 1);
        pos = match pos.checked_add(padded) {
            Some(next) if next > pos || padded == 0 => next,
            _ => break,
        };
    }

    let (audio_format, channels, sample_rate, bits_per_sample) = fmt_info.ok_or_else(|| {
        DAWProjectError::Validation("Missing WAV fmt chunk".to_string())
    })?;
    let (data_offset, data_len) = data_range.ok_or_else(|| {
        DAWProjectError::Validation("Missing WAV data chunk".to_string())
    })?;

    if audio_format != 1 && audio_format != 3 {
        return Err(DAWProjectError::Validation(format!(
            "Unsupported non-PCM WAV audio_format={audio_format}"
        )));
    }
    if channels == 0 || sample_rate == 0 {
        return Err(DAWProjectError::Validation(format!(
            "Invalid WAV format: channels={channels}, sample_rate={sample_rate}"
        )));
    }

    let raw_data = &bytes[data_offset..data_offset + data_len];
    let max_frames = if max_seconds > 0.0 && max_seconds.is_finite() {
        (max_seconds * sample_rate as f64).ceil() as usize
    } else {
        usize::MAX
    };

    let mut stereo_samples = Vec::new();
    let is_float = audio_format == 3;
    let ch_usize = channels as usize;

    if is_float && bits_per_sample == 32 {
        let total_frames = (data_len / 4) / ch_usize;
        let frames_to_read = total_frames.min(max_frames);
        stereo_samples.reserve(frames_to_read * 2);
        for f in 0..frames_to_read {
            let base = f * ch_usize;
            let s_left = f32::from_le_bytes(
                raw_data[base * 4..(base + 1) * 4]
                    .try_into()
                    .map_err(|_| DAWProjectError::Validation("Truncated float WAV frame".to_string()))?,
            );
            let s_right = if channels > 1 {
                f32::from_le_bytes(
                    raw_data[(base + 1) * 4..(base + 2) * 4]
                        .try_into()
                        .map_err(|_| DAWProjectError::Validation("Truncated float WAV frame".to_string()))?,
                )
            } else {
                s_left
            };
            stereo_samples.push(if s_left.is_finite() { s_left.clamp(-16.0, 16.0) } else { 0.0 });
            stereo_samples.push(if s_right.is_finite() { s_right.clamp(-16.0, 16.0) } else { 0.0 });
        }
    } else if !is_float && bits_per_sample == 32 {
        let total_frames = (data_len / 4) / ch_usize;
        let frames_to_read = total_frames.min(max_frames);
        stereo_samples.reserve(frames_to_read * 2);
        for f in 0..frames_to_read {
            let base = f * ch_usize;
            let v_left = i32::from_le_bytes(
                raw_data[base * 4..(base + 1) * 4]
                    .try_into()
                    .map_err(|_| DAWProjectError::Validation("Truncated 32-bit PCM WAV frame".to_string()))?,
            );
            let s_left = v_left as f32 / 2_147_483_648.0;
            let s_right = if channels > 1 {
                let v_right = i32::from_le_bytes(
                    raw_data[(base + 1) * 4..(base + 2) * 4]
                        .try_into()
                        .map_err(|_| DAWProjectError::Validation("Truncated 32-bit PCM WAV frame".to_string()))?,
                );
                v_right as f32 / 2_147_483_648.0
            } else {
                s_left
            };
            stereo_samples.push(if s_left.is_finite() { s_left } else { 0.0 });
            stereo_samples.push(if s_right.is_finite() { s_right } else { 0.0 });
        }
    } else if !is_float && bits_per_sample == 24 {
        let total_frames = (data_len / 3) / ch_usize;
        let frames_to_read = total_frames.min(max_frames);
        stereo_samples.reserve(frames_to_read * 2);
        for f in 0..frames_to_read {
            let base = f * ch_usize;
            let b0 = &raw_data[base * 3..(base + 1) * 3];
            let raw0 = ((b0[2] as i32) << 24) | ((b0[1] as i32) << 16) | ((b0[0] as i32) << 8);
            let s_left = (raw0 >> 8) as f32 / 8_388_608.0;

            let s_right = if channels > 1 {
                let b1 = &raw_data[(base + 1) * 3..(base + 2) * 3];
                let raw1 = ((b1[2] as i32) << 24) | ((b1[1] as i32) << 16) | ((b1[0] as i32) << 8);
                (raw1 >> 8) as f32 / 8_388_608.0
            } else {
                s_left
            };
            stereo_samples.push(if s_left.is_finite() { s_left } else { 0.0 });
            stereo_samples.push(if s_right.is_finite() { s_right } else { 0.0 });
        }
    } else if !is_float && bits_per_sample == 16 {
        let total_frames = (data_len / 2) / ch_usize;
        let frames_to_read = total_frames.min(max_frames);
        stereo_samples.reserve(frames_to_read * 2);
        for f in 0..frames_to_read {
            let base = f * ch_usize;
            let val0 = i16::from_le_bytes(
                raw_data[base * 2..(base + 1) * 2]
                    .try_into()
                    .map_err(|_| DAWProjectError::Validation("Truncated 16-bit PCM WAV frame".to_string()))?,
            );
            let s_left = val0 as f32 / 32768.0;

            let s_right = if channels > 1 {
                let val1 = i16::from_le_bytes(
                    raw_data[(base + 1) * 2..(base + 2) * 2]
                        .try_into()
                        .map_err(|_| DAWProjectError::Validation("Truncated 16-bit PCM WAV frame".to_string()))?,
                );
                val1 as f32 / 32768.0
            } else {
                s_left
            };
            stereo_samples.push(if s_left.is_finite() { s_left } else { 0.0 });
            stereo_samples.push(if s_right.is_finite() { s_right } else { 0.0 });
        }
    } else {
        return Err(DAWProjectError::Validation(format!(
            "Unsupported WAV format: audio_format={audio_format}, bits_per_sample={bits_per_sample}"
        )));
    }

    Ok((sample_rate, 2, stereo_samples))
}

/// Resolves an audio sample path against the filesystem or relative to the ALS base directory.
pub fn resolve_sample_path(
    path_val: &str,
    relative_path_val: &str,
    base_dir: Option<&Path>,
) -> PathBuf {
    // 1. Direct path exists on disk
    if !path_val.is_empty() {
        let p = Path::new(path_val);
        if p.exists() {
            return p.to_path_buf();
        }
    }

    // 2. Resolve relative to base_dir
    if let Some(base) = base_dir {
        // Try relative_path_val
        if !relative_path_val.is_empty() {
            let rel_clean = relative_path_val.replace('\\', "/");
            let p1 = base.join(&rel_clean);
            if p1.exists() {
                return p1;
            }
            let trimmed = rel_clean.trim_start_matches('/');
            let p2 = base.join(trimmed);
            if p2.exists() {
                return p2;
            }
        }

        // Try extracting suffix starting from "Samples/" or "samples/" from path_val or relative_path_val
        for candidate in [relative_path_val, path_val] {
            let norm = candidate.replace('\\', "/");
            if let Some(idx) = norm.to_lowercase().find("samples/") {
                let suffix = &norm[idx..];
                let p = base.join(suffix);
                if p.exists() {
                    return p;
                }
            }
        }

        // Fallback with base_dir only when relative_path_val is actually relative
        if !relative_path_val.is_empty()
            && !relative_path_val.starts_with('/')
            && !relative_path_val.starts_with('\\')
            && !(relative_path_val.len() >= 2 && relative_path_val.as_bytes()[1] == b':')
        {
            let rel_clean = relative_path_val.replace('\\', "/");
            return base.join(rel_clean);
        }
    }

    // Fallback to path_val or relative_path_val
    if !path_val.is_empty() {
        PathBuf::from(path_val)
    } else {
        PathBuf::from(relative_path_val)
    }
}

fn parse_oxide_comment_json(node: &XmlNode, prefix: &str) -> Option<serde_json::Value> {
    for child in &node.children {
        if child.tag == "__comment__" {
            let trimmed = child.text.trim();
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(rest.trim()) {
                    return Some(val);
                }
            }
        }
    }
    None
}

fn read_manual_or_attr_f32(node: &XmlNode, child_tag: &str, attr_name: &str) -> Option<f32> {
    node.get_attr(attr_name)
        .and_then(|v| v.parse::<f32>().ok())
        .or_else(|| {
            node.find_child(child_tag)
                .and_then(|c| {
                    c.find_child("Manual")
                        .and_then(|m| m.get_attr_or_text("Value"))
                        .or_else(|| c.get_attr_or_text("Value"))
                })
                .and_then(|v| v.parse::<f32>().ok())
        })
}

struct ParsedMidiClip {
    clip_id: u32,
    name: String,
    color_hex: String,
    muted: bool,
    loop_on: bool,
    loop_start: f64,
    loop_end: f64,
    pattern_offset_beats: f64,
    cur_start: f64,
    cur_end: f64,
    pattern: MidiPattern,
}

struct ParsedAudioClip {
    clip_id: u32,
    name: String,
    color_hex: String,
    muted: bool,
    loop_on: bool,
    loop_start: f64,
    loop_end: f64,
    pattern_offset_beats: f64,
    cur_start: f64,
    cur_end: f64,
    audio_data: oxide_timeline::AudioClipData,
}

fn parse_midi_clip_node(
    node: &XmlNode,
    fallback_id: u32,
    fallback_name: &str,
    fallback_color: &str,
) -> ParsedMidiClip {
    let clip_id = node
        .get_attr("Id")
        .or_else(|| node.find_child("LomId").and_then(|l| l.get_attr_or_text("Value")))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(fallback_id);

    let name = node
        .find_child("Name")
        .and_then(|n| n.get_attr_or_text("Value"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());

    let clip_meta = parse_oxide_comment_json(node, "oxide-clip:");
    let clip_color = node
        .find_child("ColorHex")
        .and_then(|c| c.get_attr_or_text("Value"))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            clip_meta
                .as_ref()
                .and_then(|m| m.get("color_hex").and_then(|v| v.as_str()).map(|s| s.to_string()))
        })
        .unwrap_or_else(|| fallback_color.to_string());

    let muted = node
        .find_child("Disabled")
        .and_then(|d| d.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<bool>().ok())
        .or_else(|| {
            clip_meta
                .as_ref()
                .and_then(|m| m.get("muted").and_then(|v| v.as_bool()))
        })
        .unwrap_or(false);

    let cur_start = node
        .find_child("CurrentStart")
        .and_then(|c| c.get_attr_or_text("Value"))
        .or_else(|| node.get_attr("Time"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let explicit_cur_end = node
        .find_child("CurrentEnd")
        .and_then(|c| c.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok());

    let loop_node = node.find_child("Loop");
    let loop_on = loop_node
        .and_then(|l| l.find_child("LoopOn"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);
    let loop_start = loop_node
        .and_then(|l| l.find_child("LoopStart"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let loop_end = loop_node
        .and_then(|l| l.find_child("LoopEnd"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or_else(|| {
            explicit_cur_end
                .map(|ce| loop_start + (ce - cur_start).max(0.0625))
                .unwrap_or(4.0)
        });
    let start_relative = loop_node
        .and_then(|l| l.find_child("StartRelative"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    let meta_pattern_offset = clip_meta
        .as_ref()
        .and_then(|m| m.get("pattern_offset_beats").and_then(|v| v.as_f64()));
    let is_oxide_session = clip_meta
        .as_ref()
        .and_then(|m| m.get("is_oxide_session").and_then(|v| v.as_bool()))
        .unwrap_or(false);

    let cur_end = explicit_cur_end.unwrap_or_else(|| cur_start + (loop_end - loop_start).max(0.0625));

    let is_note_enabled = |note_ev: &XmlNode| -> bool {
        note_ev
            .get_attr("IsEnabled")
            .or_else(|| {
                note_ev
                    .find_child("IsEnabled")
                    .and_then(|e| e.get_attr_or_text("Value"))
            })
            .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
            .unwrap_or(true)
    };

    let mut notes = Vec::new();
    let key_tracks = node.find_descendants("KeyTrack");
    if !key_tracks.is_empty() {
        for kt in key_tracks {
            let pitch = kt
                .find_descendant("MidiKey")
                .and_then(|k| k.get_attr_or_text("Value"))
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(60);
            for note_ev in kt.find_descendants("MidiNoteEvent") {
                if !is_note_enabled(note_ev) {
                    continue;
                }
                let time = note_ev
                    .get_attr("Time")
                    .or_else(|| {
                        note_ev
                            .find_child("Time")
                            .and_then(|t| t.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let dur = note_ev
                    .get_attr("Duration")
                    .or_else(|| {
                        note_ev
                            .find_child("Duration")
                            .and_then(|d| d.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(1.0);
                let vel = note_ev
                    .get_attr("Velocity")
                    .or_else(|| {
                        note_ev
                            .find_child("Velocity")
                            .and_then(|v| v.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<f32>().ok())
                    .map(|v| v.round().clamp(1.0, 127.0) as u8)
                    .unwrap_or(100);
                let prob = note_ev
                    .get_attr("Probability")
                    .or_else(|| {
                        note_ev
                            .find_child("Probability")
                            .and_then(|p| p.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(1.0);
                notes.push(MidiNote {
                    pitch,
                    velocity: vel,
                    start_beat: time,
                    duration_beats: dur,
                    probability: prob,
                });
            }
        }
    } else {
        for note_ev in node.find_descendants("MidiNoteEvent") {
            if !is_note_enabled(note_ev) {
                continue;
            }
            let pitch = note_ev
                .get_attr("MidiKey")
                .or_else(|| note_ev.get_attr("Pitch"))
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(60);
            let time = note_ev
                .get_attr("Time")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let dur = note_ev
                .get_attr("Duration")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(1.0);
            let vel = note_ev
                .get_attr("Velocity")
                .and_then(|v| v.parse::<f32>().ok())
                .map(|v| v.round().clamp(1.0, 127.0) as u8)
                .unwrap_or(100);
            let prob = note_ev
                .get_attr("Probability")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.0);
            notes.push(MidiNote {
                pitch,
                velocity: vel,
                start_beat: time,
                duration_beats: dur,
                probability: prob,
            });
        }
    }

    let pat_len = (loop_end - loop_start).max(0.0625);
    let adjust_loop_start = meta_pattern_offset.is_none() && !is_oxide_session && loop_start > 0.0;
    if adjust_loop_start {
        let mut shifted = Vec::with_capacity(notes.len());
        for mut n in notes {
            if n.start_beat + n.duration_beats <= loop_start {
                continue;
            }
            if n.start_beat < loop_start {
                n.duration_beats = (n.start_beat + n.duration_beats - loop_start).max(0.0625);
                n.start_beat = 0.0;
            } else {
                n.start_beat -= loop_start;
            }
            shifted.push(n);
        }
        notes = shifted;
    }

    notes.sort_by(|a, b| {
        a.start_beat
            .partial_cmp(&b.start_beat)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.pitch.cmp(&b.pitch))
    });

    let pattern_offset_beats = meta_pattern_offset.unwrap_or(start_relative.max(0.0));
    let (eff_loop_start, eff_loop_end) = if adjust_loop_start {
        (pattern_offset_beats, pat_len)
    } else {
        (loop_start, loop_end)
    };

    let pattern = MidiPattern {
        length_beats: pat_len,
        notes,
    };

    ParsedMidiClip {
        clip_id,
        name,
        color_hex: clip_color,
        muted,
        loop_on,
        loop_start: eff_loop_start,
        loop_end: eff_loop_end,
        pattern_offset_beats,
        cur_start,
        cur_end,
        pattern,
    }
}

fn parse_audio_clip_node(
    node: &XmlNode,
    fallback_id: u32,
    fallback_name: &str,
    fallback_color: &str,
    base_dir: Option<&Path>,
    sample_cache: &mut HashMap<PathBuf, (u32, u16, Vec<f32>)>,
) -> ParsedAudioClip {
    let clip_id = node
        .get_attr("Id")
        .or_else(|| node.find_child("LomId").and_then(|l| l.get_attr_or_text("Value")))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(fallback_id);

    let name = node
        .find_child("Name")
        .and_then(|n| n.get_attr_or_text("Value"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());

    let clip_meta = parse_oxide_comment_json(node, "oxide-clip:");
    let clip_color = node
        .find_child("ColorHex")
        .and_then(|c| c.get_attr_or_text("Value"))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            clip_meta
                .as_ref()
                .and_then(|m| m.get("color_hex").and_then(|v| v.as_str()).map(|s| s.to_string()))
        })
        .unwrap_or_else(|| fallback_color.to_string());

    let muted = node
        .find_child("Disabled")
        .and_then(|d| d.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<bool>().ok())
        .or_else(|| {
            clip_meta
                .as_ref()
                .and_then(|m| m.get("muted").and_then(|v| v.as_bool()))
        })
        .unwrap_or(false);

    let cur_start = node
        .find_child("CurrentStart")
        .and_then(|c| c.get_attr_or_text("Value"))
        .or_else(|| node.get_attr("Time"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let explicit_cur_end = node
        .find_child("CurrentEnd")
        .and_then(|c| c.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok());

    let loop_node = node.find_child("Loop");
    let loop_on = loop_node
        .and_then(|l| l.find_child("LoopOn"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true);
    let loop_start = loop_node
        .and_then(|l| l.find_child("LoopStart"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    let loop_end = loop_node
        .and_then(|l| l.find_child("LoopEnd"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or_else(|| {
            explicit_cur_end
                .map(|ce| loop_start + (ce - cur_start).max(0.0625))
                .unwrap_or(4.0)
        });
    let start_relative = loop_node
        .and_then(|l| l.find_child("StartRelative"))
        .and_then(|v| v.get_attr_or_text("Value"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    let pattern_offset_beats = clip_meta
        .as_ref()
        .and_then(|m| m.get("pattern_offset_beats").and_then(|v| v.as_f64()))
        .unwrap_or_else(|| (loop_start + start_relative).max(0.0));

    let cur_end = explicit_cur_end.unwrap_or_else(|| cur_start + (loop_end - loop_start).max(0.0625));

    let path_val = node
        .find_descendant("Path")
        .and_then(|p| p.get_attr_or_text("Value"))
        .unwrap_or_default();
    let rel_val = node
        .find_descendant("RelativePath")
        .and_then(|r| r.get_attr_or_text("Value"))
        .unwrap_or_default();

    let resolved = resolve_sample_path(&path_val, &rel_val, base_dir);
    let (sr, ch, samples) = if resolved.exists() {
        let entry = sample_cache.entry(resolved.clone()).or_insert_with(|| {
            read_wav_full(&resolved).unwrap_or((44100, 2, Vec::new()))
        });
        (entry.0, entry.1, entry.2.clone())
    } else {
        (44100, 2, Vec::new())
    };

    let mut audio_data = oxide_timeline::AudioClipData::new(
        resolved.to_string_lossy().to_string(),
        sr,
        ch,
        (loop_end - loop_start).max(0.0625),
    );
    audio_data.samples = samples;

    ParsedAudioClip {
        clip_id,
        name,
        color_hex: clip_color,
        muted,
        loop_on,
        loop_start,
        loop_end,
        pattern_offset_beats,
        cur_start,
        cur_end,
        audio_data,
    }
}

fn alloc_id(next_id: &mut u32) -> u32 {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    id
}

fn drum_pattern_to_midi_notes(drum_pat: &DrumStepPattern) -> Vec<MidiNote> {
    let mut notes = Vec::new();
    for trk in &drum_pat.tracks {
        let pitch = 36u8.saturating_add(trk.pad_index);
        for (step_idx, step) in trk.steps.iter().enumerate() {
            if step.active {
                let time = step_idx as f64 * drum_pat.step_duration_beats;
                let dur = drum_pat.step_duration_beats;
                notes.push(MidiNote::new(pitch, step.velocity.max(1), time, dur));
            }
        }
    }
    notes
}

fn write_midi_notes_xml(
    out: &mut String,
    notes: &[MidiNote],
    indent: &str,
    next_id: &mut u32,
) {
    out.push_str(&format!("{indent}<Notes>\n"));
    out.push_str(&format!("{indent}  <KeyTracks>\n"));
    let mut notes_by_key: BTreeMap<u8, Vec<&MidiNote>> = BTreeMap::new();
    for note in notes {
        notes_by_key.entry(note.pitch).or_default().push(note);
    }
    for (pitch, key_notes) in notes_by_key {
        let kt_id = alloc_id(next_id);
        out.push_str(&format!("{indent}    <KeyTrack Id=\"{kt_id}\">\n"));
        out.push_str(&format!("{indent}      <MidiKey Value=\"{pitch}\"/>\n"));
        out.push_str(&format!("{indent}      <Notes>\n"));
        for n in key_notes {
            let ev_id = alloc_id(next_id);
            out.push_str(&format!(
                "{indent}        <MidiNoteEvent Id=\"{ev_id}\" NoteId=\"{ev_id}\" Time=\"{:.4}\" Duration=\"{:.4}\" Velocity=\"{:.1}\" OffVelocity=\"64\" Probability=\"{:.2}\" IsEnabled=\"true\"/>\n",
                n.start_beat, n.duration_beats, n.velocity as f32, n.probability
            ));
        }
        out.push_str(&format!("{indent}      </Notes>\n"));
        out.push_str(&format!("{indent}    </KeyTrack>\n"));
    }
    out.push_str(&format!("{indent}  </KeyTracks>\n"));
    out.push_str(&format!("{indent}</Notes>\n"));
}

struct ClipWriteMeta {
    muted: bool,
    start_relative: f64,
    pattern_offset_beats: Option<f64>,
    is_oxide_session: bool,
}

fn write_midi_clip_xml(
    out: &mut String,
    clip_id: u32,
    name: &str,
    color_hex: &str,
    loop_on: bool,
    loop_start: f64,
    loop_end: f64,
    cur_start: f64,
    cur_end: f64,
    notes: &[MidiNote],
    meta: ClipWriteMeta,
    indent: &str,
    next_id: &mut u32,
) {
    let xml_id = alloc_id(next_id);
    out.push_str(&format!(
        "{indent}<MidiClip Id=\"{xml_id}\" Time=\"{:.4}\">\n",
        cur_start
    ));
    out.push_str(&format!("{indent}  <LomId Value=\"{clip_id}\"/>\n"));
    out.push_str(&format!(
        "{indent}  <Name Value=\"{}\"/>\n",
        xml_escape(name)
    ));
    let mut meta_obj = serde_json::json!({
        "color_hex": color_hex,
        "muted": meta.muted,
        "is_oxide_session": meta.is_oxide_session,
    });
    if let Some(offset) = meta.pattern_offset_beats {
        meta_obj["pattern_offset_beats"] = serde_json::json!(offset);
    }
    let clip_meta = sanitize_xml_comment(&meta_obj.to_string());
    out.push_str(&format!("{indent}  <!-- oxide-clip:{clip_meta} -->\n"));
    out.push_str(&format!("{indent}  <Disabled Value=\"{}\"/>\n", meta.muted));
    out.push_str(&format!("{indent}  <Loop>\n"));
    out.push_str(&format!("{indent}    <LoopOn Value=\"{loop_on}\"/>\n"));
    out.push_str(&format!(
        "{indent}    <LoopStart Value=\"{:.4}\"/>\n",
        loop_start
    ));
    out.push_str(&format!(
        "{indent}    <LoopEnd Value=\"{:.4}\"/>\n",
        loop_end
    ));
    out.push_str(&format!(
        "{indent}    <StartRelative Value=\"{:.4}\"/>\n",
        meta.start_relative
    ));
    out.push_str(&format!(
        "{indent}    <OutMarker Value=\"{:.4}\"/>\n",
        loop_end
    ));
    out.push_str(&format!(
        "{indent}    <HiddenLoopStart Value=\"{:.4}\"/>\n",
        loop_start
    ));
    out.push_str(&format!(
        "{indent}    <HiddenLoopEnd Value=\"{:.4}\"/>\n",
        loop_end
    ));
    out.push_str(&format!("{indent}  </Loop>\n"));
    out.push_str(&format!(
        "{indent}  <CurrentStart Value=\"{:.4}\"/>\n",
        cur_start
    ));
    out.push_str(&format!(
        "{indent}  <CurrentEnd Value=\"{:.4}\"/>\n",
        cur_end
    ));
    let inner_indent = format!("{indent}  ");
    write_midi_notes_xml(out, notes, &inner_indent, next_id);
    out.push_str(&format!("{indent}</MidiClip>\n"));
}

fn write_audio_clip_xml(
    out: &mut String,
    clip_id: u32,
    name: &str,
    color_hex: &str,
    loop_on: bool,
    loop_start: f64,
    loop_end: f64,
    cur_start: f64,
    cur_end: f64,
    file_path: &str,
    meta: ClipWriteMeta,
    indent: &str,
    next_id: &mut u32,
) {
    let xml_id = alloc_id(next_id);
    out.push_str(&format!(
        "{indent}<AudioClip Id=\"{xml_id}\" Time=\"{:.4}\">\n",
        cur_start
    ));
    out.push_str(&format!("{indent}  <LomId Value=\"{clip_id}\"/>\n"));
    out.push_str(&format!(
        "{indent}  <Name Value=\"{}\"/>\n",
        xml_escape(name)
    ));
    let mut meta_obj = serde_json::json!({
        "color_hex": color_hex,
        "muted": meta.muted,
        "is_oxide_session": meta.is_oxide_session,
    });
    if let Some(offset) = meta.pattern_offset_beats {
        meta_obj["pattern_offset_beats"] = serde_json::json!(offset);
    }
    let clip_meta = sanitize_xml_comment(&meta_obj.to_string());
    out.push_str(&format!("{indent}  <!-- oxide-clip:{clip_meta} -->\n"));
    out.push_str(&format!("{indent}  <Disabled Value=\"{}\"/>\n", meta.muted));
    out.push_str(&format!("{indent}  <Loop>\n"));
    out.push_str(&format!("{indent}    <LoopOn Value=\"{loop_on}\"/>\n"));
    out.push_str(&format!(
        "{indent}    <LoopStart Value=\"{:.4}\"/>\n",
        loop_start
    ));
    out.push_str(&format!(
        "{indent}    <LoopEnd Value=\"{:.4}\"/>\n",
        loop_end
    ));
    out.push_str(&format!(
        "{indent}    <StartRelative Value=\"{:.4}\"/>\n",
        meta.start_relative
    ));
    out.push_str(&format!("{indent}  </Loop>\n"));
    out.push_str(&format!(
        "{indent}  <CurrentStart Value=\"{:.4}\"/>\n",
        cur_start
    ));
    out.push_str(&format!(
        "{indent}  <CurrentEnd Value=\"{:.4}\"/>\n",
        cur_end
    ));
    out.push_str(&format!("{indent}  <SampleRef>\n"));
    out.push_str(&format!("{indent}    <FileRef>\n"));
    out.push_str(&format!(
        "{indent}      <Path Value=\"{}\"/>\n",
        xml_escape(file_path)
    ));
    out.push_str(&format!(
        "{indent}      <RelativePath Value=\"{}\"/>\n",
        xml_escape(file_path)
    ));
    out.push_str(&format!(
        "{indent}      <Name Value=\"{}\"/>\n",
        xml_escape(file_path)
    ));
    out.push_str(&format!("{indent}    </FileRef>\n"));
    out.push_str(&format!("{indent}  </SampleRef>\n"));
    out.push_str(&format!("{indent}</AudioClip>\n"));
}

fn eq8_uses_zero_based_keys(params: &HashMap<String, f32>) -> bool {
    let has_zero = params
        .keys()
        .any(|k| k.starts_with("band0_") || k.starts_with("band_0_"));
    let has_eight = params
        .keys()
        .any(|k| k.starts_with("band8_") || k.starts_with("band_8_"));
    has_zero && !has_eight
}

fn eq8_lookup_band_param(
    params: &HashMap<String, f32>,
    band_idx: usize,
    zero_based: bool,
    suffixes: &[&str],
    legacy_keys: &[&str],
    default_val: f32,
) -> f32 {
    let num = if zero_based { band_idx } else { band_idx + 1 };
    for suf in suffixes {
        for key in [format!("band{num}_{suf}"), format!("band_{num}_{suf}")] {
            if let Some(&v) = params.get(&key) {
                if v.is_finite() {
                    return v;
                }
            }
        }
    }
    for &lk in legacy_keys {
        if let Some(&v) = params.get(lk) {
            if v.is_finite() {
                return v;
            }
        }
    }
    default_val
}

fn write_insert_fx_xml(
    out: &mut String,
    fx: &InsertEffectConfig,
    indent: &str,
    next_id: &mut u32,
) {
    let dev_id = alloc_id(next_id);
    match fx {
        InsertEffectConfig::Saturator {
            drive_db,
            curve,
            output_gain_db,
        } => {
            out.push_str(&format!(
                "{indent}<Saturator Id=\"{dev_id}\" Drive=\"{drive_db:.3}\" Curve=\"{curve:.4}\" OutputGain=\"{output_gain_db:.3}\"><UserName Value=\"Saturator\"/></Saturator>\n"
            ));
        }
        InsertEffectConfig::TempoDelay {
            time_beats,
            feedback,
            mix,
        } => {
            out.push_str(&format!(
                "{indent}<Delay Id=\"{dev_id}\" Time=\"{time_beats:.4}\" Feedback=\"{feedback:.4}\" Mix=\"{mix:.4}\"><UserName Value=\"Delay\"/></Delay>\n"
            ));
        }
        InsertEffectConfig::Reverb {
            ir_path,
            decay_ms,
            mix,
        } => {
            let meta_comment = if let Some(path) = ir_path {
                let s = sanitize_xml_comment(&serde_json::json!({ "ir_path": path }).to_string());
                format!("<!-- oxide-reverb:{s} -->")
            } else {
                String::new()
            };
            out.push_str(&format!(
                "{indent}<Reverb Id=\"{dev_id}\" Decay=\"{decay_ms:.2}\" Mix=\"{mix:.4}\"><UserName Value=\"Reverb\"/>{meta_comment}</Reverb>\n"
            ));
        }
        InsertEffectConfig::EqEight { params } => {
            let meta_json = sanitize_xml_comment(&serde_json::to_string(params).unwrap_or_else(|_| "{}".to_string()));
            let zero_based = eq8_uses_zero_based_keys(params);
            let default_freqs = [30.0f32, 200.0, 1000.0, 5000.0, 100.0, 10000.0, 5000.0, 18000.0];
            let default_modes = [2u8, 3, 3, 5, 3, 3, 3, 6];
            out.push_str(&format!("{indent}<Eq8 Id=\"{dev_id}\">\n"));
            out.push_str(&format!("{indent}  <UserName Value=\"EQ Eight\"/>\n"));
            out.push_str(&format!("{indent}  <!-- oxide-eq8:{meta_json} -->\n"));
            for i in 0..8usize {
                let freq_legacy: &[&str] = match i {
                    0 => &["highpass_freq", "lowcut_freq"],
                    7 => &["lowpass_freq", "highcut_freq"],
                    _ => &[],
                };
                let gain_legacy: &[&str] = match i {
                    1 => &["low_gain", "low_gain_db"],
                    3 => &["mid_gain", "mid_gain_db"],
                    6 => &["high_gain", "high_gain_db"],
                    _ => &[],
                };
                let freq = eq8_lookup_band_param(params, i, zero_based, &["freq_hz", "freq"], freq_legacy, default_freqs[i]);
                let gain = eq8_lookup_band_param(params, i, zero_based, &["gain_db", "gain"], gain_legacy, 0.0);
                let q = eq8_lookup_band_param(
                    params,
                    i,
                    zero_based,
                    &["q"],
                    &[],
                    std::f32::consts::FRAC_1_SQRT_2,
                );
                let def_on = if i < 4 { 1.0 } else { 0.0 };
                let is_on = eq8_lookup_band_param(params, i, zero_based, &["enabled"], &[], def_on) >= 0.5;
                let mode = eq8_lookup_band_param(params, i, zero_based, &["mode"], &[], default_modes[i] as f32).round() as i32;
                out.push_str(&format!("{indent}  <Bands.{i}>\n"));
                out.push_str(&format!("{indent}    <ParameterA>\n"));
                out.push_str(&format!("{indent}      <IsOn><Manual Value=\"{is_on}\"/></IsOn>\n"));
                out.push_str(&format!("{indent}      <Mode><Manual Value=\"{mode}\"/></Mode>\n"));
                out.push_str(&format!("{indent}      <Freq><Manual Value=\"{freq:.4}\"/></Freq>\n"));
                out.push_str(&format!("{indent}      <Gain><Manual Value=\"{gain:.4}\"/></Gain>\n"));
                out.push_str(&format!("{indent}      <Q><Manual Value=\"{q:.6}\"/></Q>\n"));
                out.push_str(&format!("{indent}    </ParameterA>\n"));
                out.push_str(&format!("{indent}  </Bands.{i}>\n"));
            }
            out.push_str(&format!("{indent}</Eq8>\n"));
        }
        InsertEffectConfig::GlueCompressor {
            threshold_db,
            ratio,
            attack_ms,
            release_s,
            makeup_gain_db,
            dry_wet,
        } => {
            out.push_str(&format!(
                "{indent}<GlueCompressor Id=\"{dev_id}\" Threshold=\"{threshold_db:.2}\" Ratio=\"{ratio:.2}\" Attack=\"{attack_ms:.3}\" Release=\"{release_s:.4}\" Makeup=\"{makeup_gain_db:.2}\" DryWet=\"{dry_wet:.4}\"><UserName Value=\"Glue Compressor\"/></GlueCompressor>\n"
            ));
        }
        InsertEffectConfig::Roar {
            drive_db,
            tone,
            dry_wet,
            output_gain_db,
        } => {
            out.push_str(&format!(
                "{indent}<Roar Id=\"{dev_id}\" Drive=\"{drive_db:.3}\" Tone=\"{tone:.4}\" DryWet=\"{dry_wet:.4}\" OutputGain=\"{output_gain_db:.3}\"><UserName Value=\"Roar\"/></Roar>\n"
            ));
        }
        InsertEffectConfig::AutoFilter {
            cutoff_hz,
            resonance,
            dry_wet,
        } => {
            out.push_str(&format!(
                "{indent}<AutoFilter Id=\"{dev_id}\" Cutoff=\"{cutoff_hz:.2}\" Resonance=\"{resonance:.4}\" DryWet=\"{dry_wet:.4}\"><UserName Value=\"Auto Filter\"/></AutoFilter>\n"
            ));
        }
        InsertEffectConfig::Compressor {
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_gain_db,
            dry_wet,
        } => {
            out.push_str(&format!(
                "{indent}<Compressor2 Id=\"{dev_id}\" Threshold=\"{threshold_db:.2}\" Ratio=\"{ratio:.2}\" Attack=\"{attack_ms:.3}\" Release=\"{release_ms:.2}\" Makeup=\"{makeup_gain_db:.2}\" DryWet=\"{dry_wet:.4}\"><UserName Value=\"Compressor\"/></Compressor2>\n"
            ));
        }
        InsertEffectConfig::MultibandDynamics {
            amount,
            time_macro,
            input_gain_db,
            output_gain_db,
        } => {
            out.push_str(&format!(
                "{indent}<MultibandDynamics Id=\"{dev_id}\" Amount=\"{amount:.4}\" Time=\"{time_macro:.4}\" InputGain=\"{input_gain_db:.2}\" OutputGain=\"{output_gain_db:.2}\"><UserName Value=\"Multiband Dynamics\"/></MultibandDynamics>\n"
            ));
        }
        InsertEffectConfig::GenericDevice {
            device_type,
            params,
        } => {
            let meta_json = sanitize_xml_comment(
                &serde_json::json!({ "device_type": device_type, "params": params }).to_string(),
            );
            out.push_str(&format!("{indent}<!-- oxide-device:{meta_json} -->\n"));
        }
    }
}

fn parse_eq8_band_node(band_node: &XmlNode, band_num: usize, params: &mut HashMap<String, f32>) {
    let target = band_node.find_child("ParameterA").unwrap_or(band_node);
    if let Some(is_on_str) = target
        .find_child("IsOn")
        .and_then(|n| {
            n.find_child("Manual")
                .and_then(|m| m.get_attr_or_text("Value"))
                .or_else(|| n.get_attr_or_text("Value"))
        })
        .or_else(|| target.get_attr("IsOn"))
    {
        if let Ok(enabled) = is_on_str.parse::<bool>() {
            params.insert(
                format!("band{band_num}_enabled"),
                if enabled { 1.0 } else { 0.0 },
            );
        }
    }
    if let Some(freq) = read_manual_or_attr_f32(target, "Freq", "Freq") {
        params.insert(format!("band{band_num}_freq_hz"), freq);
    }
    if let Some(gain) = read_manual_or_attr_f32(target, "Gain", "Gain") {
        params.insert(format!("band{band_num}_gain_db"), gain);
    }
    if let Some(q) = read_manual_or_attr_f32(target, "Q", "Q") {
        params.insert(format!("band{band_num}_q"), q);
    }
}

fn parse_eq8_params(dev: &XmlNode) -> HashMap<String, f32> {
    if let Some(meta) = parse_oxide_comment_json(dev, "oxide-eq8:") {
        if let Ok(parsed) = serde_json::from_value::<HashMap<String, f32>>(meta) {
            return parsed;
        }
    }

    let mut params = HashMap::new();
    for i in 0..8usize {
        let tag = format!("Bands.{i}");
        let band_node = dev
            .find_child(&tag)
            .or_else(|| dev.find_child("Bands").and_then(|b| b.find_child(&tag)));
        if let Some(bn) = band_node {
            parse_eq8_band_node(bn, i + 1, &mut params);
        }
    }
    if params.is_empty() {
        if let Some(bands_node) = dev.find_child("Bands") {
            for (idx, bn) in bands_node.find_children("Band").into_iter().take(8).enumerate() {
                let band_num = bn
                    .get_attr("Id")
                    .and_then(|v| v.parse::<usize>().ok())
                    .map(|id| if id < 8 { id + 1 } else { idx + 1 })
                    .unwrap_or(idx + 1);
                if (1..=8).contains(&band_num) {
                    parse_eq8_band_node(bn, band_num, &mut params);
                }
            }
        }
    }
    params
}

fn parse_insert_fx_from_devices_node(devs: &XmlNode) -> Vec<InsertEffectConfig> {
    let mut insert_fx = Vec::new();
    for dev in &devs.children {
        if dev.tag == "__comment__" {
            if let Some(rest) = dev.text.trim().strip_prefix("oxide-device:") {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(rest.trim()) {
                    if let Some(device_type) = val.get("device_type").and_then(|v| v.as_str()) {
                        let params = val
                            .get("params")
                            .and_then(|p| serde_json::from_value::<HashMap<String, f32>>(p.clone()).ok())
                            .unwrap_or_default();
                        insert_fx.push(InsertEffectConfig::GenericDevice {
                            device_type: device_type.to_string(),
                            params,
                        });
                    }
                }
            }
            continue;
        }
        match dev.tag.as_str() {
            "Saturator" => {
                let drive = read_manual_or_attr_f32(dev, "Drive", "Drive").unwrap_or(4.0);
                let curve = read_manual_or_attr_f32(dev, "Curve", "Curve").unwrap_or(0.5);
                let out_gain = read_manual_or_attr_f32(dev, "OutputGain", "OutputGain").unwrap_or(0.0);
                insert_fx.push(InsertEffectConfig::Saturator {
                    drive_db: drive,
                    curve,
                    output_gain_db: out_gain,
                });
            }
            "Roar" => {
                let drive = read_manual_or_attr_f32(dev, "Drive", "Drive").unwrap_or(6.0);
                let tone = read_manual_or_attr_f32(dev, "Tone", "Tone").unwrap_or(0.0);
                let dry_wet = read_manual_or_attr_f32(dev, "DryWet", "DryWet").unwrap_or(1.0);
                let out_gain = read_manual_or_attr_f32(dev, "OutputGain", "OutputGain").unwrap_or(0.0);
                insert_fx.push(InsertEffectConfig::Roar {
                    drive_db: drive,
                    tone,
                    dry_wet,
                    output_gain_db: out_gain,
                });
            }
            "Delay" => {
                let time = read_manual_or_attr_f32(dev, "Time", "Time").unwrap_or(0.5);
                let fb = read_manual_or_attr_f32(dev, "Feedback", "Feedback").unwrap_or(0.4);
                let mix = read_manual_or_attr_f32(dev, "Mix", "Mix")
                    .or_else(|| read_manual_or_attr_f32(dev, "DryWet", "DryWet"))
                    .unwrap_or(0.3);
                insert_fx.push(InsertEffectConfig::TempoDelay {
                    time_beats: time,
                    feedback: fb,
                    mix,
                });
            }
            "Reverb" => {
                let decay = read_manual_or_attr_f32(dev, "Decay", "Decay")
                    .or_else(|| read_manual_or_attr_f32(dev, "DecayTime", "DecayTime"))
                    .unwrap_or(1500.0);
                let mix = read_manual_or_attr_f32(dev, "Mix", "Mix")
                    .or_else(|| read_manual_or_attr_f32(dev, "DryWet", "DryWet"))
                    .unwrap_or(0.25);
                let ir_path = parse_oxide_comment_json(dev, "oxide-reverb:")
                    .and_then(|v| v.get("ir_path").and_then(|p| p.as_str()).map(|s| s.to_string()));
                insert_fx.push(InsertEffectConfig::Reverb {
                    ir_path,
                    decay_ms: decay,
                    mix,
                });
            }
            "Eq8" => {
                let params = parse_eq8_params(dev);
                insert_fx.push(InsertEffectConfig::EqEight { params });
            }
            "GlueCompressor" => {
                insert_fx.push(InsertEffectConfig::GlueCompressor {
                    threshold_db: read_manual_or_attr_f32(dev, "Threshold", "Threshold").unwrap_or(0.0),
                    ratio: read_manual_or_attr_f32(dev, "Ratio", "Ratio").unwrap_or(4.0),
                    attack_ms: read_manual_or_attr_f32(dev, "Attack", "Attack").unwrap_or(1.0),
                    release_s: read_manual_or_attr_f32(dev, "Release", "Release").unwrap_or(0.2),
                    makeup_gain_db: read_manual_or_attr_f32(dev, "Makeup", "Makeup").unwrap_or(0.0),
                    dry_wet: read_manual_or_attr_f32(dev, "DryWet", "DryWet").unwrap_or(1.0),
                });
            }
            "AutoFilter" => {
                insert_fx.push(InsertEffectConfig::AutoFilter {
                    cutoff_hz: read_manual_or_attr_f32(dev, "Cutoff", "Cutoff").unwrap_or(1000.0),
                    resonance: read_manual_or_attr_f32(dev, "Resonance", "Resonance").unwrap_or(0.2),
                    dry_wet: read_manual_or_attr_f32(dev, "DryWet", "DryWet").unwrap_or(1.0),
                });
            }
            "Compressor2" | "Compressor" => {
                insert_fx.push(InsertEffectConfig::Compressor {
                    threshold_db: read_manual_or_attr_f32(dev, "Threshold", "Threshold").unwrap_or(-12.0),
                    ratio: read_manual_or_attr_f32(dev, "Ratio", "Ratio").unwrap_or(4.0),
                    attack_ms: read_manual_or_attr_f32(dev, "Attack", "Attack").unwrap_or(5.0),
                    release_ms: read_manual_or_attr_f32(dev, "Release", "Release").unwrap_or(50.0),
                    makeup_gain_db: read_manual_or_attr_f32(dev, "Makeup", "Makeup").unwrap_or(0.0),
                    dry_wet: read_manual_or_attr_f32(dev, "DryWet", "DryWet").unwrap_or(1.0),
                });
            }
            "MultibandDynamics" => {
                insert_fx.push(InsertEffectConfig::MultibandDynamics {
                    amount: read_manual_or_attr_f32(dev, "Amount", "Amount").unwrap_or(1.0),
                    time_macro: read_manual_or_attr_f32(dev, "Time", "Time").unwrap_or(1.0),
                    input_gain_db: read_manual_or_attr_f32(dev, "InputGain", "InputGain").unwrap_or(0.0),
                    output_gain_db: read_manual_or_attr_f32(dev, "OutputGain", "OutputGain").unwrap_or(0.0),
                });
            }
            _ => {}
        }
    }
    insert_fx
}

fn collect_track_automation_targets(
    mixer_node: Option<&XmlNode>,
    devices_node: Option<&XmlNode>,
) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    if let Some(m) = mixer_node {
        for (tag, name) in [
            ("Volume", "volume_db"),
            ("Pan", "pan"),
            ("Speaker", "mute"),
            ("On", "mute"),
        ] {
            if let Some(id) = m
                .find_child(tag)
                .and_then(|n| n.find_child("AutomationTarget"))
                .and_then(|at| at.get_attr("Id"))
                .and_then(|v| v.parse::<u32>().ok())
            {
                map.entry(id).or_insert_with(|| name.to_string());
            }
        }
        if let Some(sends) = m.find_child("Sends") {
            for (idx, holder) in sends.find_children("TrackSendHolder").into_iter().take(32).enumerate() {
                if let Some(id) = holder
                    .find_child("Send")
                    .and_then(|s| s.find_child("AutomationTarget"))
                    .and_then(|at| at.get_attr("Id"))
                    .and_then(|v| v.parse::<u32>().ok())
                {
                    map.insert(id, format!("send_{idx}"));
                }
            }
        }
    }
    if let Some(devs) = devices_node {
        for dev in &devs.children {
            if dev.tag == "Parameter" {
                if let Some(pname) = dev.get_attr("Name") {
                    if let Some(id) = dev
                        .find_child("AutomationTarget")
                        .and_then(|at| at.get_attr("Id"))
                        .and_then(|v| v.parse::<u32>().ok())
                    {
                        map.insert(id, pname);
                    }
                }
                continue;
            }
            for child in &dev.children {
                if child.tag == "Parameter" {
                    if let Some(pname) = child.get_attr("Name") {
                        if let Some(id) = child
                            .find_child("AutomationTarget")
                            .and_then(|at| at.get_attr("Id"))
                            .and_then(|v| v.parse::<u32>().ok())
                        {
                            map.insert(id, pname);
                        }
                    }
                } else if let Some(id) = child
                    .find_child("AutomationTarget")
                    .and_then(|at| at.get_attr("Id"))
                    .and_then(|v| v.parse::<u32>().ok())
                {
                    map.entry(id).or_insert_with(|| child.tag.clone());
                }
            }
        }
    }
    map
}

fn parse_track_automation_envelopes(
    track_node: &XmlNode,
    pointee_to_param: &HashMap<u32, String>,
) -> Vec<AutomationLane> {
    let mut lanes = Vec::new();
    let Some(auto_envs) = track_node.find_child("AutomationEnvelopes") else {
        return lanes;
    };
    let envelopes = auto_envs.find_descendants("AutomationEnvelope");
    for (env_idx, env_node) in envelopes.iter().enumerate() {
        let oxide_meta = parse_oxide_comment_json(env_node, "oxide-automation:");
        let pointee_id = env_node
            .find_child("EnvelopeTarget")
            .and_then(|et| et.find_child("PointeeId"))
            .and_then(|p| p.get_attr_or_text("Value"))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or((env_idx + 1) as u32);

        let param_id = oxide_meta
            .as_ref()
            .and_then(|m| m.get("parameter_id").and_then(|v| v.as_u64()))
            .map(|v| v as u32)
            .unwrap_or(pointee_id);
        let param_name = oxide_meta
            .as_ref()
            .and_then(|m| m.get("parameter_name").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .or_else(|| pointee_to_param.get(&pointee_id).cloned())
            .unwrap_or_else(|| format!("envelope_{}", pointee_id));

        let mut lane = AutomationLane::new(param_id, param_name);
        if let Some(events_node) = env_node
            .find_child("Automation")
            .and_then(|a| a.find_child("Events"))
        {
            let has_oxide_meta = oxide_meta.is_some();
            for ev in &events_node.children {
                if ev.tag != "FloatEvent" && ev.tag != "IntEvent" && ev.tag != "BoolEvent" {
                    continue;
                }
                let Some(beat) = ev.get_attr("Time").and_then(|v| v.parse::<f64>().ok()) else {
                    continue;
                };
                if !has_oxide_meta && beat < 0.0 {
                    continue;
                }
                let Some(val) = ev.get_attr("Value").and_then(|v| v.parse::<f32>().ok()) else {
                    continue;
                };
                let curve = ev
                    .get_attr("Curve")
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.0);
                let mut pt = AutomationPoint::new(beat, val, curve);
                pt.curve_type = AutomationCurveType::from_curve_tension(curve);
                if let (Some(cx1), Some(cy1), Some(cx2), Some(cy2)) = (
                    ev.get_attr("CurveControl1X").and_then(|v| v.parse::<f32>().ok()),
                    ev.get_attr("CurveControl1Y").and_then(|v| v.parse::<f32>().ok()),
                    ev.get_attr("CurveControl2X").and_then(|v| v.parse::<f32>().ok()),
                    ev.get_attr("CurveControl2Y").and_then(|v| v.parse::<f32>().ok()),
                ) {
                    pt.bezier = Some(CubicBezierHandle::new(cx1, cy1, cx2, cy2));
                }
                lane.add_point(pt);
            }
            if has_oxide_meta || lane.points.len() >= 2 {
                lanes.push(lane);
            }
        }
    }
    lanes
}

impl DAWProject {
    /// Emit a complete, valid Ableton Live 12 XML string.
    pub fn to_als_xml(&self) -> String {
        let max_existing_id = self
            .tracks
            .iter()
            .flat_map(|t| {
                std::iter::once(t.id)
                    .chain(t.group_id)
                    .chain(t.session_clips.iter().flatten().map(|c| c.id))
                    .chain(t.arrangement_clips.iter().map(|c| c.id))
                    .chain(t.automation_lanes.iter().map(|l| l.parameter_id))
            })
            .chain(self.scenes.iter().map(|s| s.id))
            .max()
            .unwrap_or(0);
        let mut next_id: u32 = max_existing_id.saturating_add(100).max(100);

        // Assign unique exported track IDs >= 1 (0 is reserved for MainTrack).
        let mut used_track_ids: HashSet<u32> = HashSet::new();
        used_track_ids.insert(0);
        let mut track_id_remap: HashMap<u32, u32> = HashMap::with_capacity(self.tracks.len());
        let mut exported_track_ids: Vec<u32> = Vec::with_capacity(self.tracks.len());
        for track in &self.tracks {
            let exp_id = if track.id >= 1 && used_track_ids.insert(track.id) {
                track.id
            } else {
                let nid = alloc_id(&mut next_id);
                used_track_ids.insert(nid);
                nid
            };
            track_id_remap.entry(track.id).or_insert(exp_id);
            exported_track_ids.push(exp_id);
        }

        let mut body = String::with_capacity(16384);

        // Metadata & Annotation (Oxide extra fields in XML comment for clean Live 12 compatibility)
        body.push_str("    <Annotation Value=\"");
        body.push_str(&xml_escape(&self.metadata.name));
        body.push_str("\"/>\n");
        let meta_json = sanitize_xml_comment(
            &serde_json::json!({
                "name": self.metadata.name,
                "author": self.metadata.author,
                "version": self.metadata.version,
                "created_at_utc": self.metadata.created_at_utc,
                "modified_at_utc": self.metadata.modified_at_utc,
            })
            .to_string(),
        );
        body.push_str(&format!("    <!-- oxide-metadata:{meta_json} -->\n"));

        // Time Signature
        body.push_str("    <TimeSignature>\n");
        body.push_str("      <TimeSignatures>\n");
        let ts_id = alloc_id(&mut next_id);
        body.push_str(&format!("        <RemoteableTimeSignature Id=\"{ts_id}\">\n"));
        body.push_str(&format!(
            "          <Numerator Value=\"{}\"/>\n",
            self.transport.time_signature_numerator
        ));
        body.push_str(&format!(
            "          <Denominator Value=\"{}\"/>\n",
            self.transport.time_signature_denominator
        ));
        body.push_str("        </RemoteableTimeSignature>\n");
        body.push_str("      </TimeSignatures>\n");
        body.push_str("    </TimeSignature>\n");

        // Transport
        let loop_len = (self.transport.loop_end_beat - self.transport.loop_start_beat).max(0.0);
        body.push_str("    <Transport>\n");
        body.push_str(&format!(
            "      <LoopOn Value=\"{}\"/>\n",
            self.transport.loop_enabled
        ));
        body.push_str(&format!(
            "      <LoopStart Value=\"{:.4}\"/>\n",
            self.transport.loop_start_beat
        ));
        body.push_str(&format!(
            "      <LoopLength Value=\"{:.4}\"/>\n",
            loop_len
        ));
        body.push_str("    </Transport>\n");

        // Main Track (Live 12 MainTrack Id="0")
        let master_vol_lin = db_to_ableton_linear(self.master_track.volume_db);
        let main_tempo_at_id = alloc_id(&mut next_id);
        let main_vol_at_id = alloc_id(&mut next_id);
        let main_pan_at_id = alloc_id(&mut next_id);
        body.push_str("    <MainTrack Id=\"0\">\n");
        body.push_str("      <Name>\n");
        body.push_str("        <EffectiveName Value=\"Master\"/>\n");
        body.push_str("        <UserName Value=\"Master\"/>\n");
        body.push_str("      </Name>\n");
        let master_meta = sanitize_xml_comment(
            &serde_json::to_string(&self.master_track).unwrap_or_else(|_| "{}".to_string()),
        );
        body.push_str(&format!("      <!-- oxide-master:{master_meta} -->\n"));
        body.push_str("      <DeviceChain>\n");
        body.push_str("        <Mixer>\n");
        body.push_str("          <Tempo>\n");
        body.push_str(&format!(
            "            <Manual Value=\"{:.4}\"/>\n",
            self.transport.bpm
        ));
        body.push_str(&format!(
            "            <AutomationTarget Id=\"{main_tempo_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget>\n"
        ));
        body.push_str("          </Tempo>\n");
        body.push_str(&format!(
            "          <Volume><Manual Value=\"{}\"/><AutomationTarget Id=\"{main_vol_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget></Volume>\n",
            format_ableton_linear(master_vol_lin)
        ));
        body.push_str(&format!(
            "          <Pan><Manual Value=\"{:.4}\"/><AutomationTarget Id=\"{main_pan_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget></Pan>\n",
            self.master_track.pan
        ));
        body.push_str("        </Mixer>\n");
        body.push_str("        <Devices>\n");
        if self.master_track.saturator_enabled {
            let fx = InsertEffectConfig::Saturator {
                drive_db: self.master_track.saturator_drive_db,
                curve: self.master_track.saturator_curve,
                output_gain_db: self.master_track.saturator_output_db,
            };
            write_insert_fx_xml(&mut body, &fx, "          ", &mut next_id);
        }
        if self.master_track.delay_mix > 0.0 {
            let fx = InsertEffectConfig::TempoDelay {
                time_beats: self.master_track.delay_time_beats,
                feedback: self.master_track.delay_feedback,
                mix: self.master_track.delay_mix,
            };
            write_insert_fx_xml(&mut body, &fx, "          ", &mut next_id);
        }
        if self.master_track.reverb_mix > 0.0 {
            let fx = InsertEffectConfig::Reverb {
                ir_path: None,
                decay_ms: self.master_track.reverb_decay_ms,
                mix: self.master_track.reverb_mix,
            };
            write_insert_fx_xml(&mut body, &fx, "          ", &mut next_id);
        }
        body.push_str("        </Devices>\n");
        body.push_str("      </DeviceChain>\n");
        body.push_str("    </MainTrack>\n");

        // PreHearTrack (required sibling of MainTrack in Live 12 schema)
        body.push_str("    <PreHearTrack>\n");
        body.push_str("      <Name>\n");
        body.push_str("        <EffectiveName Value=\"Master\"/>\n");
        body.push_str("        <UserName Value=\"\"/>\n");
        body.push_str("      </Name>\n");
        body.push_str("      <DeviceChain>\n");
        body.push_str("        <Mixer>\n");
        body.push_str("          <Volume><Manual Value=\"1.0000000\"/></Volume>\n");
        body.push_str("          <Pan><Manual Value=\"0.0000\"/></Pan>\n");
        body.push_str("          <On><Manual Value=\"true\"/></On>\n");
        body.push_str("          <Speaker><Manual Value=\"true\"/></Speaker>\n");
        body.push_str("        </Mixer>\n");
        body.push_str("      </DeviceChain>\n");
        body.push_str("    </PreHearTrack>\n");

        // Tracks
        body.push_str("    <Tracks>\n");
        for (track, &exported_id) in self.tracks.iter().zip(exported_track_ids.iter()) {
            let (tag, color_idx) = match track.track_type {
                TrackType::Audio => ("AudioTrack", 24),
                TrackType::DrumRack => ("MidiTrack", 48),
                TrackType::MidiSynth => ("MidiTrack", 12),
                TrackType::Return => ("ReturnTrack", 18),
                TrackType::Group => ("GroupTrack", 30),
            };

            let mapped_group_id = track
                .group_id
                .map(|gid| *track_id_remap.get(&gid).unwrap_or(&gid) as i64)
                .unwrap_or(-1);

            body.push_str(&format!("      <{} Id=\"{}\">\n", tag, exported_id));
            body.push_str("        <Name>\n");
            body.push_str(&format!(
                "          <EffectiveName Value=\"{}\"/>\n",
                xml_escape(&track.name)
            ));
            body.push_str(&format!(
                "          <UserName Value=\"{}\"/>\n",
                xml_escape(&track.name)
            ));
            body.push_str("        </Name>\n");
            body.push_str(&format!("        <Color Value=\"{}\"/>\n", color_idx));
            body.push_str(&format!("        <TrackGroupId Value=\"{mapped_group_id}\"/>\n"));
            let track_meta = sanitize_xml_comment(
                &serde_json::json!({
                    "track_type": format!("{:?}", track.track_type),
                    "color_hex": track.color_hex,
                    "group_id": track.group_id.map(|gid| *track_id_remap.get(&gid).unwrap_or(&gid)),
                })
                .to_string(),
            );
            body.push_str(&format!("        <!-- oxide-track:{track_meta} -->\n"));
            let inst_meta = sanitize_xml_comment(
                &serde_json::to_string(&track.instrument).unwrap_or_else(|_| "null".to_string()),
            );
            body.push_str(&format!("        <!-- oxide-instrument:{inst_meta} -->\n"));

            let vol_at_id = alloc_id(&mut next_id);
            let pan_at_id = alloc_id(&mut next_id);
            let on_at_id = alloc_id(&mut next_id);
            let speaker_at_id = alloc_id(&mut next_id);
            let effective_sends = track.mixer.effective_sends_db();
            let send_at_ids: Vec<u32> = (0..effective_sends.len())
                .map(|_| alloc_id(&mut next_id))
                .collect();

            // Resolve or allocate PointeeId / AutomationTarget IDs for every automation lane
            let mut used_pointees: HashSet<u32> = HashSet::new();
            let mut lane_pointee_ids: Vec<u32> = Vec::with_capacity(track.automation_lanes.len());
            let mut device_auto_targets: Vec<(String, u32)> = Vec::new();
            for lane in &track.automation_lanes {
                let pname = lane.parameter_name.to_ascii_lowercase();
                let candidate = match pname.as_str() {
                    "volume" | "volume_db" => Some(vol_at_id),
                    "pan" => Some(pan_at_id),
                    "mute" | "speaker" => Some(speaker_at_id),
                    "send_a" | "send_a_db" | "send_0" => send_at_ids.first().copied(),
                    "send_b" | "send_b_db" | "send_1" => send_at_ids.get(1).copied(),
                    other => other
                        .strip_prefix("send_")
                        .and_then(|s| s.parse::<usize>().ok())
                        .and_then(|idx| send_at_ids.get(idx).copied()),
                };
                let pointee_id = match candidate {
                    Some(id) if used_pointees.insert(id) => id,
                    _ => {
                        let id = alloc_id(&mut next_id);
                        used_pointees.insert(id);
                        device_auto_targets.push((lane.parameter_name.clone(), id));
                        id
                    }
                };
                lane_pointee_ids.push(pointee_id);
            }

            body.push_str("        <DeviceChain>\n");
            body.push_str("          <Mixer>\n");
            let trk_vol_lin = db_to_ableton_linear(track.mixer.volume_db);
            body.push_str(&format!(
                "            <Volume><Manual Value=\"{}\"/><AutomationTarget Id=\"{vol_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget></Volume>\n",
                format_ableton_linear(trk_vol_lin)
            ));
            body.push_str(&format!(
                "            <Pan><Manual Value=\"{:.4}\"/><AutomationTarget Id=\"{pan_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget></Pan>\n",
                track.mixer.pan
            ));
            body.push_str(&format!(
                "            <On><Manual Value=\"true\"/><AutomationTarget Id=\"{on_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget></On>\n"
            ));
            body.push_str(&format!(
                "            <SoloSink><Manual Value=\"{}\"/></SoloSink>\n",
                track.mixer.solo
            ));
            body.push_str(&format!(
                "            <Speaker><Manual Value=\"{}\"/><AutomationTarget Id=\"{speaker_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget></Speaker>\n",
                !track.mixer.mute
            ));
            body.push_str(&format!(
                "            <Arm><Manual Value=\"{}\"/></Arm>\n",
                track.mixer.arm
            ));
            body.push_str("            <Sends>\n");
            for (send_idx, (send_db, &send_at_id)) in
                effective_sends.into_iter().zip(send_at_ids.iter()).enumerate()
            {
                let send_lin = db_to_ableton_linear(send_db);
                body.push_str(&format!(
                    "              <TrackSendHolder Id=\"{send_idx}\"><Send><Manual Value=\"{}\"/><AutomationTarget Id=\"{send_at_id}\"><LockEnvelope Value=\"0\"/></AutomationTarget></Send></TrackSendHolder>\n",
                    format_ableton_linear(send_lin)
                ));
            }
            body.push_str("            </Sends>\n");
            body.push_str("          </Mixer>\n");

            if track.track_type != TrackType::Return && track.track_type != TrackType::Group {
                body.push_str("          <MainSequencer>\n");
                body.push_str("            <ClipSlotList>\n");
                for (slot_idx, maybe_clip) in track.session_clips.iter().enumerate() {
                    body.push_str(&format!("              <ClipSlot Id=\"{}\">\n", slot_idx));
                    body.push_str("                <ClipSlot>\n");
                    body.push_str("                  <Value>\n");
                    if let Some(clip) = maybe_clip {
                        let sess_meta = ClipWriteMeta {
                            muted: false,
                            start_relative: clip.loop_start_beat,
                            pattern_offset_beats: None,
                            is_oxide_session: true,
                        };
                        match &clip.pattern {
                            ClipPattern::Midi(midi_pat) => {
                                write_midi_clip_xml(
                                    &mut body,
                                    clip.id,
                                    &clip.name,
                                    &clip.color_hex,
                                    clip.loop_enabled,
                                    clip.loop_start_beat,
                                    clip.loop_end_beat,
                                    clip.loop_start_beat,
                                    clip.loop_end_beat,
                                    &midi_pat.notes,
                                    sess_meta,
                                    "                    ",
                                    &mut next_id,
                                );
                            }
                            ClipPattern::Drum(drum_pat) => {
                                let notes = drum_pattern_to_midi_notes(drum_pat);
                                write_midi_clip_xml(
                                    &mut body,
                                    clip.id,
                                    &clip.name,
                                    &clip.color_hex,
                                    clip.loop_enabled,
                                    clip.loop_start_beat,
                                    clip.loop_end_beat,
                                    clip.loop_start_beat,
                                    clip.loop_end_beat,
                                    &notes,
                                    sess_meta,
                                    "                    ",
                                    &mut next_id,
                                );
                            }
                            ClipPattern::Audio(audio_data) => {
                                write_audio_clip_xml(
                                    &mut body,
                                    clip.id,
                                    &clip.name,
                                    &clip.color_hex,
                                    clip.loop_enabled,
                                    clip.loop_start_beat,
                                    clip.loop_end_beat,
                                    clip.loop_start_beat,
                                    clip.loop_end_beat,
                                    &audio_data.file_path,
                                    sess_meta,
                                    "                    ",
                                    &mut next_id,
                                );
                            }
                        }
                    }
                    body.push_str("                  </Value>\n");
                    body.push_str("                </ClipSlot>\n");
                    body.push_str("              </ClipSlot>\n");
                }
                body.push_str("            </ClipSlotList>\n");
                if !track.arrangement_clips.is_empty() {
                    match track.track_type {
                        TrackType::Audio => {
                            body.push_str("            <Sample>\n");
                            body.push_str("              <ArrangerAutomation>\n");
                            body.push_str("                <Events>\n");
                            for clip in &track.arrangement_clips {
                                if let ClipPattern::Audio(audio_data) = &clip.pattern {
                                    let offset = clip.pattern_offset_beats.max(0.0);
                                    write_audio_clip_xml(
                                        &mut body,
                                        clip.id,
                                        &clip.name,
                                        &track.color_hex,
                                        true,
                                        offset,
                                        offset + audio_data.length_beats.max(0.0625),
                                        clip.timeline_start_beat,
                                        clip.timeline_end_beat(),
                                        &audio_data.file_path,
                                        ClipWriteMeta {
                                            muted: clip.muted,
                                            start_relative: 0.0,
                                            pattern_offset_beats: Some(clip.pattern_offset_beats),
                                            is_oxide_session: false,
                                        },
                                        "                  ",
                                        &mut next_id,
                                    );
                                }
                            }
                            body.push_str("                </Events>\n");
                            body.push_str("              </ArrangerAutomation>\n");
                            body.push_str("            </Sample>\n");
                        }
                        _ => {
                            body.push_str("            <ClipTimeable>\n");
                            body.push_str("              <ArrangerAutomation>\n");
                            body.push_str("                <Events>\n");
                            for clip in &track.arrangement_clips {
                                let arr_meta = ClipWriteMeta {
                                    muted: clip.muted,
                                    start_relative: clip.pattern_offset_beats,
                                    pattern_offset_beats: Some(clip.pattern_offset_beats),
                                    is_oxide_session: false,
                                };
                                match &clip.pattern {
                                    ClipPattern::Midi(midi_pat) => {
                                        write_midi_clip_xml(
                                            &mut body,
                                            clip.id,
                                            &clip.name,
                                            &track.color_hex,
                                            true,
                                            0.0,
                                            midi_pat.length_beats.max(0.0625),
                                            clip.timeline_start_beat,
                                            clip.timeline_end_beat(),
                                            &midi_pat.notes,
                                            arr_meta,
                                            "                  ",
                                            &mut next_id,
                                        );
                                    }
                                    ClipPattern::Drum(drum_pat) => {
                                        let notes = drum_pattern_to_midi_notes(drum_pat);
                                        write_midi_clip_xml(
                                            &mut body,
                                            clip.id,
                                            &clip.name,
                                            &track.color_hex,
                                            true,
                                            0.0,
                                            drum_pat.length_beats().max(0.0625),
                                            clip.timeline_start_beat,
                                            clip.timeline_end_beat(),
                                            &notes,
                                            arr_meta,
                                            "                  ",
                                            &mut next_id,
                                        );
                                    }
                                    ClipPattern::Audio(_) => {}
                                }
                            }
                            body.push_str("                </Events>\n");
                            body.push_str("              </ArrangerAutomation>\n");
                            body.push_str("            </ClipTimeable>\n");
                        }
                    }
                }
                body.push_str("          </MainSequencer>\n");
            }

            // Devices
            body.push_str("          <Devices>\n");
            let write_dev_params = |out: &mut String, targets: &[(String, u32)], indent: &str| {
                for (pname, pid) in targets {
                    out.push_str(&format!(
                        "{indent}<Parameter Name=\"{}\"><AutomationTarget Id=\"{pid}\"><LockEnvelope Value=\"0\"/></AutomationTarget></Parameter>\n",
                        xml_escape(pname)
                    ));
                }
            };
            match track.track_type {
                TrackType::DrumRack => {
                    let dr_id = alloc_id(&mut next_id);
                    body.push_str(&format!("            <DrumGroupDevice Id=\"{dr_id}\">\n"));
                    body.push_str("              <UserName Value=\"909 Drum Rack\"/>\n");
                    body.push_str("              <IsExpanded Value=\"true\"/>\n");
                    write_dev_params(&mut body, &device_auto_targets, "              ");
                    body.push_str("            </DrumGroupDevice>\n");
                }
                TrackType::MidiSynth => {
                    let wt_id = alloc_id(&mut next_id);
                    body.push_str(&format!("            <Wavetable Id=\"{wt_id}\">\n"));
                    body.push_str("              <UserName Value=\"Wavetable\"/>\n");
                    body.push_str("              <IsExpanded Value=\"true\"/>\n");
                    write_dev_params(&mut body, &device_auto_targets, "              ");
                    body.push_str("            </Wavetable>\n");
                }
                TrackType::Audio | TrackType::Return | TrackType::Group => {
                    write_dev_params(&mut body, &device_auto_targets, "            ");
                }
            }
            for fx in &track.insert_fx {
                write_insert_fx_xml(&mut body, fx, "            ", &mut next_id);
            }
            body.push_str("          </Devices>\n");

            body.push_str("        </DeviceChain>\n");

            if !track.automation_lanes.is_empty() {
                body.push_str("        <AutomationEnvelopes>\n");
                body.push_str("          <Envelopes>\n");
                for (lane, &pointee_id) in
                    track.automation_lanes.iter().zip(lane_pointee_ids.iter())
                {
                    let env_id = alloc_id(&mut next_id);
                    let auto_meta = sanitize_xml_comment(
                        &serde_json::json!({
                            "parameter_id": lane.parameter_id,
                            "parameter_name": lane.parameter_name,
                        })
                        .to_string(),
                    );
                    body.push_str(&format!("            <AutomationEnvelope Id=\"{env_id}\">\n"));
                    body.push_str("              <EnvelopeTarget>\n");
                    body.push_str(&format!("                <PointeeId Value=\"{pointee_id}\"/>\n"));
                    body.push_str("              </EnvelopeTarget>\n");
                    body.push_str(&format!("              <!-- oxide-automation:{auto_meta} -->\n"));
                    body.push_str("              <Automation>\n");
                    body.push_str("                <Events>\n");
                    for pt in &lane.points {
                        let ev_id = alloc_id(&mut next_id);
                        let bezier_attrs = if let Some(b) = pt.bezier {
                            format!(
                                " CurveControl1X=\"{:.5}\" CurveControl1Y=\"{:.5}\" CurveControl2X=\"{:.5}\" CurveControl2Y=\"{:.5}\"",
                                b.cx1, b.cy1, b.cx2, b.cy2
                            )
                        } else {
                            String::new()
                        };
                        body.push_str(&format!(
                            "                  <FloatEvent Id=\"{ev_id}\" Time=\"{:.4}\" Value=\"{:.6}\" Curve=\"{:.4}\"{bezier_attrs}/>\n",
                            pt.beat, pt.value_norm, pt.curve
                        ));
                    }
                    body.push_str("                </Events>\n");
                    body.push_str("              </Automation>\n");
                    body.push_str("            </AutomationEnvelope>\n");
                }
                body.push_str("          </Envelopes>\n");
                body.push_str("        </AutomationEnvelopes>\n");
            }

            body.push_str(&format!("      </{}>\n", tag));
        }
        body.push_str("    </Tracks>\n");

        // Scenes
        body.push_str("    <Scenes>\n");
        for scene in &self.scenes {
            let (scene_tempo, tempo_enabled) = match scene.bpm_override {
                Some(bpm) if bpm.is_finite() && bpm > 0.0 => (bpm, true),
                _ => (self.transport.bpm, false),
            };
            body.push_str(&format!("      <Scene Id=\"{}\">\n", scene.id));
            body.push_str(&format!(
                "        <Name Value=\"{}\"/>\n",
                xml_escape(&scene.name)
            ));
            body.push_str(&format!(
                "        <Tempo Value=\"{:.4}\"/>\n",
                scene_tempo
            ));
            body.push_str(&format!(
                "        <IsTempoEnabled Value=\"{}\"/>\n",
                tempo_enabled
            ));
            body.push_str("      </Scene>\n");
        }
        body.push_str("    </Scenes>\n");

        let mut out = String::with_capacity(body.len() + 256);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<Ableton MajorVersion=\"5\" MinorVersion=\"12.0_12049\" SchemaChangeCount=\"3\" Creator=\"Oxide Studio\" Revision=\"oxide-12.0\">\n");
        out.push_str("  <LiveSet>\n");
        out.push_str(&format!("    <NextPointeeId Value=\"{next_id}\"/>\n"));
        out.push_str(&body);
        out.push_str("  </LiveSet>\n");
        out.push_str("</Ableton>\n");

        out
    }

    /// Serialize project to Gzip-compressed `.als` bytes.
    pub fn to_als_bytes(&self) -> Result<Vec<u8>, DAWProjectError> {
        let xml = self.to_als_xml();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(xml.as_bytes())?;
        let bytes = encoder.finish()?;
        Ok(bytes)
    }

    /// Save project to disk as a Gzip-compressed `.als` file.
    pub fn save_als(&self, path: impl AsRef<Path>) -> Result<(), DAWProjectError> {
        let bytes = self.to_als_bytes()?;
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Parse project from bytes (auto-detects Gzip magic `[0x1f, 0x8b]` or raw XML).
    pub fn from_als_bytes(bytes: &[u8]) -> Result<DAWProject, DAWProjectError> {
        Self::from_als_bytes_with_base_dir(bytes, None)
    }

    /// Parse project from bytes with an optional base directory for resolving relative sample paths.
    pub fn from_als_bytes_with_base_dir(
        bytes: &[u8],
        base_dir: Option<&Path>,
    ) -> Result<DAWProject, DAWProjectError> {
        if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
            let decoder = GzDecoder::new(bytes);
            let mut limited = Read::take(decoder, MAX_ALS_XML_BYTES + 1);
            let mut xml = String::new();
            limited.read_to_string(&mut xml)?;
            if xml.len() as u64 > MAX_ALS_XML_BYTES {
                return Err(DAWProjectError::Validation(
                    "ALS decompressed payload exceeds 64 MiB safety limit".to_string(),
                ));
            }
            Self::from_als_xml_with_base_dir(&xml, base_dir)
        } else {
            if bytes.len() as u64 > MAX_ALS_XML_BYTES {
                return Err(DAWProjectError::Validation(
                    "ALS decompressed payload exceeds 64 MiB safety limit".to_string(),
                ));
            }
            let xml = std::str::from_utf8(bytes).map_err(|e| {
                DAWProjectError::Validation(format!("Invalid UTF-8 in ALS bytes: {}", e))
            })?;
            Self::from_als_xml_with_base_dir(xml, base_dir)
        }
    }

    /// Parse project from uncompressed Ableton Live XML string.
    pub fn from_als_xml(xml: &str) -> Result<DAWProject, DAWProjectError> {
        Self::from_als_xml_with_base_dir(xml, None)
    }

    /// Parse project from uncompressed Ableton Live XML string with an optional base directory for resolving samples.
    pub fn from_als_xml_with_base_dir(
        xml: &str,
        base_dir: Option<&Path>,
    ) -> Result<DAWProject, DAWProjectError> {
        let root = parse_xml_to_node(xml)?;
        let liveset = if root.tag == "LiveSet" {
            &root
        } else {
            root.find_child("LiveSet").ok_or_else(|| {
                DAWProjectError::Validation("Missing <LiveSet> element in Ableton XML".to_string())
            })?
        };

        // Project metadata
        let mut metadata = ProjectMetadata::default();
        if let Some(ann) = liveset
            .find_child("Annotation")
            .and_then(|a| a.get_attr_or_text("Value"))
        {
            if !ann.is_empty() {
                metadata.name = ann;
            }
        }
        if let Some(meta_json) = parse_oxide_comment_json(liveset, "oxide-metadata:") {
            if let Some(name) = meta_json.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                metadata.name = name.to_string();
            }
            if let Some(author) = meta_json.get("author").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                metadata.author = author.to_string();
            }
            if let Some(ver) = meta_json.get("version").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                metadata.version = ver.to_string();
            }
            if let Some(ca) = meta_json.get("created_at_utc").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                metadata.created_at_utc = ca.to_string();
            }
            if let Some(ma) = meta_json.get("modified_at_utc").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                metadata.modified_at_utc = ma.to_string();
            }
        }
        if let Some(meta) = liveset.find_child("Metadata") {
            if let Some(name) = meta
                .find_child("Name")
                .and_then(|n| n.get_attr_or_text("Value"))
            {
                if !name.is_empty() {
                    metadata.name = name;
                }
            }
            if let Some(author) = meta
                .find_child("Author")
                .and_then(|a| a.get_attr_or_text("Value"))
            {
                if !author.is_empty() {
                    metadata.author = author;
                }
            }
            if let Some(ver) = meta
                .find_child("Version")
                .and_then(|v| v.get_attr_or_text("Value"))
            {
                if !ver.is_empty() {
                    metadata.version = ver;
                }
            }
            if let Some(ca) = meta
                .find_child("CreatedAt")
                .and_then(|c| c.get_attr_or_text("Value"))
            {
                if !ca.is_empty() {
                    metadata.created_at_utc = ca;
                }
            }
            if let Some(ma) = meta
                .find_child("ModifiedAt")
                .and_then(|m| m.get_attr_or_text("Value"))
            {
                if !ma.is_empty() {
                    metadata.modified_at_utc = ma;
                }
            }
        }

        // Main / Master Track (Live 12 uses MainTrack; Live <= 11 uses MasterTrack)
        let main_track_node = liveset
            .find_child("MainTrack")
            .or_else(|| liveset.find_child("MasterTrack"));
        let main_mixer_node = main_track_node.and_then(|mt| {
            mt.find_child("DeviceChain")
                .and_then(|dc| dc.find_child("Mixer"))
                .or_else(|| mt.find_child("Mixer"))
        });

        // Transport & Tempo (only check MainTrack/MasterTrack mixer or direct LiveSet child, never arbitrary nested Tempo)
        let mut transport = TransportConfig::default();
        let tempo_node = main_mixer_node
            .and_then(|m| m.find_child("Tempo"))
            .or_else(|| liveset.find_child("Tempo"));
        if let Some(bpm_str) = tempo_node.and_then(|t| {
            t.find_child("Manual")
                .and_then(|m| m.get_attr_or_text("Value"))
                .or_else(|| t.get_attr_or_text("Value"))
        }) {
            if let Ok(bpm) = bpm_str.parse::<f64>() {
                if bpm.is_finite() && bpm > 0.0 {
                    transport.bpm = bpm;
                }
            }
        }

        let ts_node = main_mixer_node
            .and_then(|m| m.find_child("TimeSignature"))
            .or_else(|| liveset.find_child("TimeSignature"));
        if let Some(ts) = ts_node {
            if let Some(num) = ts
                .find_descendant("Numerator")
                .and_then(|n| n.get_attr_or_text("Value"))
                .and_then(|v| v.parse::<u8>().ok())
            {
                transport.time_signature_numerator = num;
            }
            if let Some(den) = ts
                .find_descendant("Denominator")
                .and_then(|d| d.get_attr_or_text("Value"))
                .and_then(|v| v.parse::<u8>().ok())
            {
                transport.time_signature_denominator = den;
            }
        }

        if let Some(tr) = liveset.find_child("Transport") {
            if let Some(loop_on) = tr
                .find_child("LoopOn")
                .and_then(|l| l.get_attr_or_text("Value"))
                .and_then(|v| v.parse::<bool>().ok())
            {
                transport.loop_enabled = loop_on;
            }
            if let Some(start) = tr
                .find_child("LoopStart")
                .and_then(|l| l.get_attr_or_text("Value"))
                .and_then(|v| v.parse::<f64>().ok())
            {
                transport.loop_start_beat = start;
            }
            if let Some(len) = tr
                .find_child("LoopLength")
                .and_then(|l| l.get_attr_or_text("Value"))
                .and_then(|v| v.parse::<f64>().ok())
            {
                transport.loop_end_beat = transport.loop_start_beat + len;
            }
        }

        // Master Track
        let mut master_track = MasterTrackDefinition::default();
        if let Some(mt) = main_track_node {
            if let Some(master_json) = parse_oxide_comment_json(mt, "oxide-master:") {
                if let Ok(parsed_master) = serde_json::from_value::<MasterTrackDefinition>(master_json) {
                    master_track = parsed_master;
                }
            }
            let vol_node = main_mixer_node
                .and_then(|m| m.find_child("Volume"))
                .or_else(|| mt.find_child("Volume"));
            if let Some(vol) = vol_node
                .and_then(|v| {
                    v.find_child("Manual")
                        .and_then(|m| m.get_attr_or_text("Value"))
                        .or_else(|| v.get_attr_or_text("Value"))
                })
                .and_then(|v| v.parse::<f32>().ok())
            {
                master_track.volume_db = ableton_linear_to_db(vol);
            }
            let pan_node = main_mixer_node
                .and_then(|m| m.find_child("Pan"))
                .or_else(|| mt.find_child("Pan"));
            if let Some(pan) = pan_node
                .and_then(|p| {
                    p.find_child("Manual")
                        .and_then(|m| m.get_attr_or_text("Value"))
                        .or_else(|| p.get_attr_or_text("Value"))
                })
                .and_then(|v| v.parse::<f32>().ok())
            {
                master_track.pan = pan;
            }
        }

        // Tracks (including ReturnTrack and GroupTrack, plus optional ReturnTracks container)
        let mut sample_cache: HashMap<PathBuf, (u32, u16, Vec<f32>)> = HashMap::new();
        let mut tracks = Vec::new();
        let mut raw_track_nodes: Vec<&XmlNode> = Vec::new();
        if let Some(tracks_node) = liveset.find_child("Tracks") {
            for child in &tracks_node.children {
                if matches!(
                    child.tag.as_str(),
                    "MidiTrack" | "AudioTrack" | "ReturnTrack" | "GroupTrack"
                ) {
                    raw_track_nodes.push(child);
                }
            }
        }
        if let Some(ret_tracks_node) = liveset.find_child("ReturnTracks") {
            for child in &ret_tracks_node.children {
                if child.tag == "ReturnTrack" {
                    raw_track_nodes.push(child);
                }
            }
        }

        for (idx, child) in raw_track_nodes.into_iter().enumerate() {
            let track_id = child
                .get_attr("Id")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(idx as u32 + 1);
            let track_name = child
                .find_child("Name")
                .and_then(|n| {
                    n.find_child("UserName")
                        .and_then(|u| u.get_attr_or_text("Value"))
                        .filter(|s| !s.is_empty())
                        .or_else(|| {
                            n.find_child("EffectiveName")
                                .and_then(|e| e.get_attr_or_text("Value"))
                                .filter(|s| !s.is_empty())
                        })
                })
                .unwrap_or_else(|| format!("Track {}", track_id));

            let track_meta = parse_oxide_comment_json(child, "oxide-track:");
            let track_type_str = child
                .find_child("TrackType")
                .and_then(|t| t.get_attr_or_text("Value"))
                .or_else(|| {
                    track_meta
                        .as_ref()
                        .and_then(|m| m.get("track_type").and_then(|v| v.as_str()).map(|s| s.to_string()))
                });

            let device_chain = child.find_child("DeviceChain");
            let devices_node = device_chain
                .and_then(|dc| {
                    dc.find_child("DeviceChain")
                        .and_then(|dc2| dc2.find_child("Devices"))
                        .or_else(|| dc.find_child("Devices"))
                })
                .or_else(|| child.find_child("Devices"));

            let track_type = match track_type_str.as_deref() {
                Some("Return") => TrackType::Return,
                Some("Group") => TrackType::Group,
                Some("Audio") => TrackType::Audio,
                Some("DrumRack") => TrackType::DrumRack,
                Some("MidiSynth") => TrackType::MidiSynth,
                _ => match child.tag.as_str() {
                    "ReturnTrack" => TrackType::Return,
                    "GroupTrack" => TrackType::Group,
                    "AudioTrack" => TrackType::Audio,
                    _ => {
                        let has_drum_device = devices_node
                            .map(|d| d.find_descendant("DrumGroupDevice").is_some())
                            .unwrap_or_else(|| {
                                device_chain
                                    .map(|dc| dc.find_descendant("DrumGroupDevice").is_some())
                                    .unwrap_or(false)
                            });
                        if has_drum_device {
                            TrackType::DrumRack
                        } else {
                            TrackType::MidiSynth
                        }
                    }
                },
            };

            let group_id = child
                .find_child("TrackGroupId")
                .and_then(|g| g.get_attr_or_text("Value"))
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|&gid| gid >= 0)
                .map(|gid| gid as u32)
                .or_else(|| {
                    track_meta
                        .as_ref()
                        .and_then(|m| m.get("group_id").and_then(|v| v.as_u64()))
                        .map(|v| v as u32)
                });

            let color_hex = child
                .find_child("ColorHex")
                .and_then(|c| c.get_attr_or_text("Value"))
                .or_else(|| {
                    track_meta
                        .as_ref()
                        .and_then(|m| m.get("color_hex").and_then(|v| v.as_str()).map(|s| s.to_string()))
                })
                .unwrap_or_else(|| match track_type {
                    TrackType::MidiSynth => "#ffbe00".to_string(),
                    TrackType::DrumRack => "#00d2ff".to_string(),
                    TrackType::Audio => "#00e676".to_string(),
                    TrackType::Return => "#00d2ff".to_string(),
                    TrackType::Group => "#b388ff".to_string(),
                });

            // Find this track's own <DeviceChain><Mixer> (never descend into nested Rack chains)
            let mixer_node = device_chain
                .and_then(|dc| dc.find_child("Mixer"))
                .or_else(|| child.find_child("Mixer"));
            let mut mixer = MixerState::default();
            if let Some(mixer_node) = mixer_node {
                if let Some(vol) = mixer_node
                    .find_child("Volume")
                    .and_then(|v| {
                        v.find_child("Manual")
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .or_else(|| v.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<f32>().ok())
                {
                    mixer.volume_db = ableton_linear_to_db(vol);
                }
                if let Some(pan) = mixer_node
                    .find_child("Pan")
                    .and_then(|p| {
                        p.find_child("Manual")
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .or_else(|| p.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<f32>().ok())
                {
                    mixer.pan = pan;
                }
                let speaker_active = mixer_node
                    .find_child("Speaker")
                    .and_then(|s| {
                        s.find_child("Manual")
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .or_else(|| s.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<bool>().ok());
                let on_active = mixer_node
                    .find_child("On")
                    .and_then(|o| {
                        o.find_child("Manual")
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .or_else(|| o.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<bool>().ok());
                if let Some(active) = speaker_active.or(on_active) {
                    mixer.mute = !active;
                }
                if let Some(solo) = mixer_node
                    .find_child("SoloSink")
                    .and_then(|s| {
                        s.find_child("Manual")
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .or_else(|| s.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<bool>().ok())
                {
                    mixer.solo = solo;
                }
                if let Some(arm) = mixer_node
                    .find_child("Arm")
                    .and_then(|a| {
                        a.find_child("Manual")
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .or_else(|| a.get_attr_or_text("Value"))
                    })
                    .and_then(|v| v.parse::<bool>().ok())
                {
                    mixer.arm = arm;
                }
                if let Some(sends) = mixer_node.find_child("Sends") {
                    for (holder_idx, holder) in sends
                        .find_children("TrackSendHolder")
                        .into_iter()
                        .take(32)
                        .enumerate()
                    {
                        if let Some(s_val) = holder
                            .find_child("Send")
                            .and_then(|s| {
                                s.find_child("Manual")
                                    .and_then(|m| m.get_attr_or_text("Value"))
                                    .or_else(|| s.get_attr_or_text("Value"))
                            })
                            .or_else(|| {
                                holder
                                    .find_descendant("Manual")
                                    .and_then(|m| m.get_attr_or_text("Value"))
                            })
                            .and_then(|v| v.parse::<f32>().ok())
                        {
                            mixer.set_send_db(holder_idx, ableton_linear_to_db(s_val));
                        }
                    }
                }
            }

            let parsed_instrument = parse_oxide_comment_json(child, "oxide-instrument:")
                .or_else(|| devices_node.and_then(|d| parse_oxide_comment_json(d, "oxide-instrument:")))
                .and_then(|v| serde_json::from_value::<InstrumentConfig>(v).ok());
            let instrument = parsed_instrument.unwrap_or_else(|| match track_type {
                TrackType::MidiSynth => InstrumentConfig::BuiltinSubtractiveSynth(
                    SynthPatchParameters::default(),
                ),
                TrackType::DrumRack => InstrumentConfig::ElectronicDrumRack(
                    crate::drums::DrumRackDefinition::default_909(),
                ),
                TrackType::Audio | TrackType::Return | TrackType::Group => InstrumentConfig::None,
            });

            let main_seq = device_chain
                .and_then(|dc| dc.find_child("MainSequencer"))
                .or_else(|| child.find_child("MainSequencer"));
            let freeze_seq = device_chain
                .and_then(|dc| dc.find_child("FreezeSequencer"))
                .or_else(|| child.find_child("FreezeSequencer"));

            // 1. Session Clips (never enter TakeLanes)
            let mut session_clips = Vec::new();
            if track_type != TrackType::Return && track_type != TrackType::Group {
                let slot_list = main_seq
                    .and_then(|ms| ms.find_child("ClipSlotList"))
                    .or_else(|| freeze_seq.and_then(|fs| fs.find_child("ClipSlotList")))
                    .or_else(|| device_chain.and_then(|dc| dc.find_child("ClipSlotList")))
                    .or_else(|| child.find_child("ClipSlotList"));
                if let Some(slot_list) = slot_list {
                    for (slot_idx, slot_node) in
                        slot_list.find_children("ClipSlot").iter().enumerate()
                    {
                        if let Some(midi_clip_node) = slot_node.find_descendant("MidiClip") {
                            let parsed = parse_midi_clip_node(
                                midi_clip_node,
                                slot_idx as u32,
                                &format!("Clip {}", slot_idx + 1),
                                &color_hex,
                            );
                            let mut clip = SessionClip::new(
                                parsed.clip_id,
                                parsed.name,
                                ClipPattern::Midi(parsed.pattern),
                            );
                            clip.loop_enabled = parsed.loop_on;
                            clip.loop_start_beat = parsed.loop_start;
                            clip.loop_end_beat = parsed.loop_end;
                            clip.color_hex = parsed.color_hex;
                            session_clips.push(Some(clip));
                        } else if let Some(audio_clip_node) = slot_node.find_descendant("AudioClip") {
                            let parsed = parse_audio_clip_node(
                                audio_clip_node,
                                slot_idx as u32,
                                &format!("Audio {}", slot_idx + 1),
                                &color_hex,
                                base_dir,
                                &mut sample_cache,
                            );
                            let mut clip = SessionClip::new(
                                parsed.clip_id,
                                parsed.name,
                                ClipPattern::Audio(parsed.audio_data),
                            );
                            clip.loop_enabled = parsed.loop_on;
                            clip.loop_start_beat = parsed.loop_start;
                            clip.loop_end_beat = parsed.loop_end;
                            clip.color_hex = parsed.color_hex;
                            session_clips.push(Some(clip));
                        } else {
                            session_clips.push(None);
                        }
                    }
                }
            }

            if session_clips.iter().all(|c| c.is_none()) {
                session_clips.clear();
            }

            // 2. Arrangement Clips: traverse ONLY DeviceChain -> MainSequencer -> (ClipTimeable | Sample) -> ArrangerAutomation
            // (with FreezeSequencer fallback if MainSequencer yields none; NEVER descend into TakeLanes).
            let mut arrangement_clips = Vec::new();
            if track_type != TrackType::Return && track_type != TrackType::Group {
                let mut collect_seq_clips = |seq: &XmlNode, out: &mut Vec<ArrangementClip>| {
                    let mut aa_nodes = Vec::new();
                    if let Some(ct) = seq.find_child("ClipTimeable") {
                        if let Some(aa) = ct.find_child("ArrangerAutomation") {
                            aa_nodes.push(aa);
                        }
                    }
                    if let Some(sample) = seq.find_child("Sample") {
                        if let Some(aa) = sample.find_child("ArrangerAutomation") {
                            aa_nodes.push(aa);
                        }
                    }
                    for aa in aa_nodes {
                        if let Some(events) = aa.find_child("Events") {
                            if track_type != TrackType::Audio {
                                for (m_idx, mc_node) in events.find_children("MidiClip").iter().enumerate() {
                                    let parsed = parse_midi_clip_node(
                                        mc_node,
                                        (out.len() + 1) as u32,
                                        &format!("Midi {}", m_idx + 1),
                                        &color_hex,
                                    );
                                    let dur = (parsed.cur_end - parsed.cur_start).max(0.0625);
                                    let mut arr_clip = ArrangementClip::new(
                                        parsed.clip_id,
                                        parsed.name,
                                        parsed.cur_start,
                                        dur,
                                        ClipPattern::Midi(parsed.pattern),
                                    );
                                    arr_clip.muted = parsed.muted;
                                    arr_clip.pattern_offset_beats = parsed.pattern_offset_beats;
                                    out.push(arr_clip);
                                }
                            }
                            if track_type == TrackType::Audio || out.is_empty() {
                                for (a_idx, ac_node) in events.find_children("AudioClip").iter().enumerate() {
                                    let parsed = parse_audio_clip_node(
                                        ac_node,
                                        (out.len() + 1) as u32,
                                        &format!("Audio {}", a_idx + 1),
                                        &color_hex,
                                        base_dir,
                                        &mut sample_cache,
                                    );
                                    let dur = (parsed.cur_end - parsed.cur_start).max(0.0625);
                                    let mut arr_clip = ArrangementClip::new(
                                        parsed.clip_id,
                                        parsed.name,
                                        parsed.cur_start,
                                        dur,
                                        ClipPattern::Audio(parsed.audio_data),
                                    );
                                    arr_clip.muted = parsed.muted;
                                    arr_clip.pattern_offset_beats = parsed.pattern_offset_beats;
                                    out.push(arr_clip);
                                }
                            }
                        }
                    }
                };

                if let Some(ms) = main_seq {
                    collect_seq_clips(ms, &mut arrangement_clips);
                }
                if arrangement_clips.is_empty() {
                    if let Some(fs) = freeze_seq {
                        collect_seq_clips(fs, &mut arrangement_clips);
                    }
                }
            }

            // Insert FX
            let insert_fx = if let Some(devs) = devices_node {
                parse_insert_fx_from_devices_node(devs)
            } else {
                Vec::new()
            };

            // Automation Lanes
            let pointee_to_param = collect_track_automation_targets(mixer_node, devices_node);
            let automation_lanes = parse_track_automation_envelopes(child, &pointee_to_param);

            tracks.push(TrackDefinition {
                id: track_id,
                name: track_name,
                color_hex,
                track_type,
                mixer,
                instrument,
                insert_fx,
                session_clips,
                arrangement_clips,
                automation_lanes,
                group_id,
            });
        }

        // Scenes
        let mut scenes = Vec::new();
        if let Some(scenes_node) = liveset.find_child("Scenes") {
            for (idx, sc_node) in scenes_node.find_children("Scene").iter().enumerate() {
                let id = sc_node
                    .get_attr("Id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(idx as u32 + 1);
                let name = sc_node
                    .find_child("Name")
                    .and_then(|n| n.get_attr_or_text("Value"))
                    .unwrap_or_else(|| format!("Scene {}", id));
                let tempo_enabled = sc_node
                    .find_child("IsTempoEnabled")
                    .and_then(|t| t.get_attr_or_text("Value"))
                    .and_then(|v| v.parse::<bool>().ok())
                    .unwrap_or(false);
                let bpm_override = if tempo_enabled {
                    sc_node
                        .find_child("Tempo")
                        .and_then(|t| {
                            t.find_child("Manual")
                                .and_then(|m| m.get_attr_or_text("Value"))
                                .or_else(|| t.get_attr_or_text("Value"))
                        })
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|b| b.is_finite() && *b > 0.0)
                } else {
                    None
                };
                let mut scene = Scene::new(id, name);
                scene.bpm_override = bpm_override;
                scenes.push(scene);
            }
        }

        Ok(DAWProject {
            metadata,
            transport,
            master_track,
            tracks,
            scenes,
            presets: load_all_factory_presets(),
            macros: load_all_factory_macros(),
        })
    }

    /// Load project from disk from a .als file, resolving relative sample paths against the parent folder.
    pub fn load_als(path: impl AsRef<Path>) -> Result<DAWProject, DAWProjectError> {
        let p = path.as_ref();
        let base_dir = p.parent();
        let bytes = fs::read(p)?;
        Self::from_als_bytes_with_base_dir(&bytes, base_dir)
    }
}

/// Load an Ableton Live `.als` file from disk directly.
pub fn load_als(path: impl AsRef<Path>) -> Result<DAWProject, DAWProjectError> {
    DAWProject::load_als(path)
}

/// Parse an Ableton Live XML string with an optional base directory for resolving relative sample paths.
pub fn from_als_xml_with_base_dir(
    xml: &str,
    base_dir: Option<&Path>,
) -> Result<DAWProject, DAWProjectError> {
    DAWProject::from_als_xml_with_base_dir(xml, base_dir)
}

/// Parse Ableton Live bytes with an optional base directory for resolving relative sample paths.
pub fn from_als_bytes_with_base_dir(
    bytes: &[u8],
    base_dir: Option<&Path>,
) -> Result<DAWProject, DAWProjectError> {
    DAWProject::from_als_bytes_with_base_dir(bytes, base_dir)
}

/// Save a `DAWProject` as an Ableton Live `.als` file to disk directly.
pub fn save_als(project: &DAWProject, path: impl AsRef<Path>) -> Result<(), DAWProjectError> {
    project.save_als(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_timeline::{ClipPattern, MidiNote, MidiPattern, SessionClip};

    #[test]
    fn test_als_xml_generation_and_parsing_roundtrip() {
        let mut project = DAWProject::new("Oxide Set");
        project.transport.bpm = 126.5;
        project.transport.time_signature_numerator = 4;
        project.transport.time_signature_denominator = 4;

        let mut synth_track = TrackDefinition::new_synth(1, "Lead Synth");
        synth_track.mixer.volume_db = -3.5;
        synth_track.mixer.pan = 0.25;
        synth_track.mixer.mute = false;

        let mut pattern = MidiPattern::new(4.0);
        pattern.add_note(MidiNote::new(60, 100, 0.0, 1.0));
        pattern.add_note(MidiNote::new(64, 110, 1.0, 0.5));
        pattern.add_note(MidiNote::new(67, 95, 2.0, 2.0));

        let clip = SessionClip::new(10, "Melody", ClipPattern::Midi(pattern));
        synth_track.session_clips.push(Some(clip));
        project.tracks.push(synth_track);

        // Add 909 Drum track
        let mut drum_track = TrackDefinition::new_drum_rack(2, "909 Drums");
        drum_track.mixer.volume_db = -1.0;
        drum_track.mixer.pan = -0.1;
        let mut drum_pat = oxide_timeline::DrumStepPattern::new_16_step();
        drum_pat.toggle_step(0, 0); // Kick on 0
        drum_pat.toggle_step(0, 4); // Kick on 4
        let drum_clip = SessionClip::new(20, "909 Beat", ClipPattern::Drum(drum_pat));
        drum_track.session_clips.push(Some(drum_clip));
        project.tracks.push(drum_track);

        // Generate XML
        let xml = project.to_als_xml();
        assert!(xml.contains("<Ableton MajorVersion=\"5\" MinorVersion=\"12.0_12049\""));
        assert!(xml.contains("<Tempo>"));
        assert!(xml.contains("<Manual Value=\"126.5000\"/>"));
        assert!(xml.contains("<MidiTrack Id=\"1\">"));
        assert!(xml.contains("<UserName Value=\"Lead Synth\"/>"));
        assert!(xml.contains("<MidiKey Value=\"60\"/>"));

        // Parse back
        let reloaded = DAWProject::from_als_xml(&xml).expect("Parsing XML must succeed");

        // Verify equality
        assert!((reloaded.transport.bpm - project.transport.bpm).abs() < 1e-3);
        assert_eq!(reloaded.tracks.len(), project.tracks.len());
        assert_eq!(reloaded.tracks[0].name, project.tracks[0].name);
        assert!((reloaded.tracks[0].mixer.volume_db - project.tracks[0].mixer.volume_db).abs() < 1e-3);
        assert!((reloaded.tracks[0].mixer.pan - project.tracks[0].mixer.pan).abs() < 1e-3);

        // Verify notes
        if let Some(clip) = &reloaded.tracks[0].session_clips[0] {
            if let ClipPattern::Midi(pat) = &clip.pattern {
                assert_eq!(pat.notes.len(), 3);
                assert_eq!(pat.notes[0].pitch, 60);
                assert_eq!(pat.notes[1].pitch, 64);
                assert_eq!(pat.notes[2].pitch, 67);
                assert!((pat.notes[0].start_beat - 0.0).abs() < 1e-3);
                assert!((pat.notes[1].start_beat - 1.0).abs() < 1e-3);
                assert!((pat.notes[2].start_beat - 2.0).abs() < 1e-3);
            } else {
                panic!("Expected Midi pattern");
            }
        } else {
            panic!("Expected session clip");
        }
    }

    #[test]
    fn test_gzip_als_bytes_and_magic_header_roundtrip() {
        let mut project = DAWProject::new("Techno Project");
        project.transport.bpm = 135.0;

        let mut track = TrackDefinition::new_synth(1, "Acid 303");
        track.mixer.volume_db = -4.2;
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(36, 127, 0.0, 0.5));
        pat.add_note(MidiNote::new(48, 115, 0.5, 0.5));
        track.session_clips.push(Some(SessionClip::new(1, "Acid Loop", ClipPattern::Midi(pat))));
        project.tracks.push(track);

        // Compress to bytes
        let bytes = project.to_als_bytes().expect("Gzip compression must succeed");
        // Verify Gzip magic bytes [0x1f, 0x8b]
        assert!(bytes.len() > 10);
        assert_eq!(bytes[0], 0x1f, "First byte must be Gzip magic 0x1f");
        assert_eq!(bytes[1], 0x8b, "Second byte must be Gzip magic 0x8b");

        // Decompress and parse
        let loaded = DAWProject::from_als_bytes(&bytes).expect("Gzip decompression must succeed");
        assert!((loaded.transport.bpm - 135.0).abs() < 1e-3);
        assert_eq!(loaded.tracks.len(), 1);
        assert_eq!(loaded.tracks[0].name, "Acid 303");
        assert!((loaded.tracks[0].mixer.volume_db - (-4.2)).abs() < 1e-3);

        let notes = match &loaded.tracks[0].session_clips[0].as_ref().unwrap().pattern {
            ClipPattern::Midi(p) => &p.notes,
            _ => panic!("Expected Midi"),
        };
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].pitch, 36);
        assert_eq!(notes[1].pitch, 48);
    }

    #[test]
    fn test_disk_als_save_and_load_roundtrip() {
        let mut project = DAWProject::new("Disk Test");
        project.transport.bpm = 128.0;
        let mut track = TrackDefinition::new_synth(1, "Saw Bass");
        track.mixer.volume_db = -2.0;
        track.mixer.pan = -0.5;
        let mut pat = MidiPattern::new(4.0);
        pat.add_note(MidiNote::new(40, 105, 0.0, 1.5));
        track.session_clips.push(Some(SessionClip::new(1, "Bassline", ClipPattern::Midi(pat))));
        project.tracks.push(track);

        let tmp_file = std::env::temp_dir().join("oxide_test_save_load.als");
        project.save_als(&tmp_file).expect("save_als must succeed");

        // Verify magic bytes on disk
        let raw_disk = fs::read(&tmp_file).expect("read file must succeed");
        assert_eq!(raw_disk[0], 0x1f);
        assert_eq!(raw_disk[1], 0x8b);

        let loaded = DAWProject::load_als(&tmp_file).expect("load_als must succeed");
        assert!((loaded.transport.bpm - 128.0).abs() < 1e-3);
        assert_eq!(loaded.tracks[0].name, "Saw Bass");
        assert!((loaded.tracks[0].mixer.volume_db - (-2.0)).abs() < 1e-3);
        assert!((loaded.tracks[0].mixer.pan - (-0.5)).abs() < 1e-3);

        let _ = fs::remove_file(tmp_file);
    }

    #[test]
    fn test_als_decompression_safety_limit() {
        // Test raw payload exceeding MAX_ALS_XML_BYTES fails with safety limit error
        let huge_dummy = vec![b' '; (MAX_ALS_XML_BYTES + 10) as usize];
        let res = DAWProject::from_als_bytes(&huge_dummy);
        assert!(res.is_err());
        let err_str = res.err().unwrap().to_string();
        assert!(err_str.contains("safety limit"));
    }

    #[test]
    fn test_ableton_linear_db_conversion_exact_invertible() {
        assert!((db_to_ableton_linear(0.0) - 1.0).abs() < 1e-6);
        assert!(ableton_linear_to_db(1.0).abs() < 1e-6);

        assert!((db_to_ableton_linear(-70.0) - 0.00031622776).abs() < 1e-8);
        assert!((ableton_linear_to_db(0.00031622776) - (-70.0)).abs() < 1e-4);
        assert!((ableton_linear_to_db(0.000316) - (-70.0)).abs() < 1e-4);

        assert!((db_to_ableton_linear(6.0) - 1.99526238).abs() < 1e-6);
        assert!((ableton_linear_to_db(1.99526238) - 6.0).abs() < 1e-4);
        assert!((db_to_ableton_linear(-6.0) - 0.50118723).abs() < 1e-5);
        assert!((ableton_linear_to_db(0.50118723) - (-6.0)).abs() < 1e-3);

        for db in [-60.0, -24.0, -12.0, -6.0, -3.5, 0.0, 2.5, 6.0] {
            let lin = db_to_ableton_linear(db);
            assert!(lin > 0.0 && lin <= ABLETON_MAX_LINEAR);
            let round = ableton_linear_to_db(lin);
            assert!(
                (round - db).abs() < 1e-3,
                "Roundtrip mismatch for {db} dB -> {lin} -> {round} dB"
            );
        }
    }

    #[test]
    fn test_als_roundtrip_return_group_drums_sends_and_2beat_loops() {
        let mut project = DAWProject::new("Live 12 Full Roundtrip");
        project.transport.bpm = 132.0;
        project.master_track.volume_db = 0.0;

        // 1. Synth track with 2-beat loop, muted send A (-70 dB), active send B (-12 dB), and automation
        let mut synth = TrackDefinition::new_synth(1, "Lead Synth");
        synth.mixer.volume_db = 0.0;
        synth.mixer.mute = true;
        synth.mixer.set_send_db(0, -70.0);
        synth.mixer.set_send_db(1, -12.0);
        synth.group_id = Some(3);
        if let InstrumentConfig::BuiltinSubtractiveSynth(ref mut patch) = synth.instrument {
            patch.preset_name = Some("wowbass".to_string());
            patch.set_overridden_parameter("filter_frequency", 3000.0);
        }

        let mut two_beat = MidiPattern::new(2.0);
        two_beat.add_note(MidiNote::new(60, 110, 0.0, 0.5));
        two_beat.add_note(MidiNote::new(67, 100, 1.0, 0.5));
        let mut sess_clip = SessionClip::new(10, "2 Beat Riff", ClipPattern::Midi(two_beat));
        sess_clip.loop_end_beat = 2.0;
        synth.session_clips.push(Some(sess_clip));

        let mut auto = AutomationLane::new(15, "filter_frequency");
        auto.add_point(AutomationPoint::linear(0.0, 0.25));
        auto.add_point(AutomationPoint::linear(8.0, 0.90));
        synth.automation_lanes.push(auto);
        project.tracks.push(synth);

        // 2. DrumRack track with Arrangement Clip (ClipPattern::Drum) + muted + offset
        let mut drums = TrackDefinition::new_drum_rack(2, "909 Kit");
        drums.mixer.volume_db = -3.0;
        drums.mixer.set_send_db(0, -70.0);
        drums.mixer.set_send_db(1, -70.0);
        if let InstrumentConfig::ElectronicDrumRack(ref mut dr) = drums.instrument {
            dr.pads[0].name = "Custom 909 Kick".to_string();
            dr.pads[0].mute = true;
        }
        let mut dpat = DrumStepPattern::new_16_step();
        dpat.toggle_step(0, 0); // Kick -> pitch 36
        dpat.toggle_step(2, 4); // Snare -> pitch 38
        dpat.toggle_step(6, 2); // Closed HH -> pitch 42
        let mut arr_drum = ArrangementClip::new(
            20,
            "Drums Arr",
            0.0,
            8.0,
            ClipPattern::Drum(dpat),
        );
        arr_drum.muted = true;
        arr_drum.pattern_offset_beats = 1.0;
        drums.arrangement_clips.push(arr_drum);
        project.tracks.push(drums);

        // 3. Group Track
        let mut grp = TrackDefinition::new_group(3, "Synth Bus");
        grp.mixer.volume_db = -1.5;
        project.tracks.push(grp);

        // 4. Return Track with Eq8
        let mut ret = TrackDefinition::new_return(4, "A-Reverb");
        ret.mixer.volume_db = 0.0;
        let mut eq_params = HashMap::new();
        eq_params.insert("band1_freq_hz".to_string(), 110.0);
        eq_params.insert("band1_gain_db".to_string(), -4.5);
        eq_params.insert("band1_q".to_string(), 1.1);
        eq_params.insert("band1_enabled".to_string(), 1.0);
        ret.insert_fx.push(InsertEffectConfig::EqEight { params: eq_params });
        ret.insert_fx.push(InsertEffectConfig::Reverb {
            ir_path: None,
            decay_ms: 2200.0,
            mix: 1.0,
        });
        project.tracks.push(ret);

        let mut s1 = Scene::new(1, "Break");
        s1.bpm_override = Some(138.0);
        project.scenes.push(s1);

        let xml = project.to_als_xml();

        // Check Live 12 schema requirements
        assert!(xml.contains("<MainTrack Id=\"0\">"));
        assert!(xml.contains("<PreHearTrack>"));
        assert!(xml.contains("<NextPointeeId Value="));
        assert!(xml.contains("<ReturnTrack Id=\"4\">"));
        assert!(xml.contains("<GroupTrack Id=\"3\">"));
        assert!(xml.contains("<KeyTrack Id="));
        assert!(xml.contains("<MidiNoteEvent Id="));
        assert!(xml.contains("Value=\"1.0000000\""));
        assert!(xml.contains("Value=\"0.000316"));
        assert!(xml.contains("<On><Manual Value=\"true\"/>"));
        assert!(xml.contains("<Speaker><Manual Value=\"false\"/>"));
        assert!(xml.contains("<TrackGroupId Value=\"3\"/>"));
        assert!(xml.contains("<Bands.0>"));
        assert!(!xml.contains("<Metadata>"));
        assert!(!xml.contains("<TrackType "));
        assert!(!xml.contains("<ColorHex "));

        // Verify every PointeeId in AutomationEnvelope has a matching AutomationTarget Id
        let root_node = parse_xml_to_node(&xml).unwrap();
        let all_targets: HashSet<String> = root_node
            .find_descendants("AutomationTarget")
            .into_iter()
            .filter_map(|at| at.get_attr("Id"))
            .collect();
        for pnode in root_node.find_descendants("PointeeId") {
            let val = pnode.get_attr_or_text("Value").unwrap();
            assert!(
                all_targets.contains(&val),
                "Envelope PointeeId={val} must have matching AutomationTarget"
            );
        }

        // Parse back and verify lossless roundtrip
        let reloaded = DAWProject::from_als_xml(&xml).expect("Reload must succeed");
        assert_eq!(reloaded.metadata.name, "Live 12 Full Roundtrip");
        assert!(reloaded.master_track.volume_db.abs() < 1e-3);
        assert_eq!(reloaded.tracks.len(), 4);

        // Track 0: Synth + mute + group_id + instrument + 2-beat loop + muted send (-70 dB) + automation
        let t0 = &reloaded.tracks[0];
        assert_eq!(t0.track_type, TrackType::MidiSynth);
        assert!(t0.mixer.mute, "Mute via <Speaker> must survive roundtrip");
        assert_eq!(t0.group_id, Some(3));
        if let InstrumentConfig::BuiltinSubtractiveSynth(ref patch) = t0.instrument {
            assert_eq!(patch.preset_name.as_deref(), Some("wowbass"));
            assert_eq!(patch.overridden_params, vec!["filter_frequency".to_string()]);
        } else {
            panic!("Expected BuiltinSubtractiveSynth");
        }
        assert!(t0.mixer.volume_db.abs() < 1e-3);
        assert!((t0.mixer.send_a_db - (-70.0)).abs() < 1e-3);
        assert!((t0.mixer.send_b_db - (-12.0)).abs() < 1e-3);
        let c0 = t0.session_clips[0].as_ref().unwrap();
        assert!((c0.loop_end_beat - 2.0).abs() < 1e-3);
        if let ClipPattern::Midi(mp) = &c0.pattern {
            assert!((mp.length_beats - 2.0).abs() < 1e-3);
        } else {
            panic!("Expected Midi pattern");
        }
        assert_eq!(t0.automation_lanes.len(), 1);
        assert_eq!(t0.automation_lanes[0].parameter_name, "filter_frequency");
        assert_eq!(t0.automation_lanes[0].points.len(), 2);

        // Track 1: DrumRack with Arrangement Clip (muted + offset) and NO fabricated session clips
        let t1 = &reloaded.tracks[1];
        assert_eq!(t1.track_type, TrackType::DrumRack);
        assert!(t1.session_clips.is_empty(), "Arrangement-only track must not fabricate session_clips");
        assert_eq!(t1.arrangement_clips.len(), 1);
        assert!(t1.arrangement_clips[0].muted);
        assert!((t1.arrangement_clips[0].pattern_offset_beats - 1.0).abs() < 1e-3);
        assert!((t1.arrangement_clips[0].duration_beats - 8.0).abs() < 1e-3);
        if let InstrumentConfig::ElectronicDrumRack(ref dr) = t1.instrument {
            assert_eq!(dr.pads[0].name, "Custom 909 Kick");
            assert!(dr.pads[0].mute);
        } else {
            panic!("Expected ElectronicDrumRack");
        }
        if let ClipPattern::Midi(dp) = &t1.arrangement_clips[0].pattern {
            assert_eq!(dp.notes.len(), 3);
            assert!(dp.notes.iter().any(|n| n.pitch == 36));
            assert!(dp.notes.iter().any(|n| n.pitch == 38));
            assert!(dp.notes.iter().any(|n| n.pitch == 42));
        } else {
            panic!("Expected Drum arrangement clip imported as Midi");
        }

        // Track 2: GroupTrack
        assert_eq!(reloaded.tracks[2].track_type, TrackType::Group);
        assert!((reloaded.tracks[2].mixer.volume_db - (-1.5)).abs() < 1e-3);

        // Track 3: ReturnTrack with Eq8 + Reverb
        assert_eq!(reloaded.tracks[3].track_type, TrackType::Return);
        assert_eq!(reloaded.tracks[3].insert_fx.len(), 2);

        // Scene bpm_override
        assert_eq!(reloaded.scenes.len(), 1);
        assert_eq!(reloaded.scenes[0].bpm_override, Some(138.0));
    }

    #[test]
    fn test_als_import_disabled_notes_and_external_linear_sends() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12203" Creator="Ableton Live 12.2">
  <LiveSet>
    <MainTrack Id="0">
      <DeviceChain>
        <Mixer>
          <Tempo><Manual Value="128.0"/></Tempo>
          <Volume><Manual Value="1.0"/></Volume>
        </Mixer>
      </DeviceChain>
    </MainTrack>
    <Tracks>
      <MidiTrack Id="1">
        <Name><EffectiveName Value="909 Lead Synth"/></Name>
        <TrackGroupId Value="3"/>
        <DeviceChain>
          <Mixer>
            <Volume><Manual Value="1.0"/></Volume>
            <On><Manual Value="true"/></On>
            <Speaker><Manual Value="false"/></Speaker>
            <Sends>
              <TrackSendHolder Id="62"><Send><Manual Value="0.00031622776"/></Send></TrackSendHolder>
              <TrackSendHolder Id="63"><Send><Manual Value="1.0"/></Send></TrackSendHolder>
            </Sends>
          </Mixer>
          <MainSequencer>
            <ClipSlotList>
              <ClipSlot Id="0">
                <ClipSlot><Value>
                  <MidiClip Id="1">
                    <Loop><LoopStart Value="0.0"/><LoopEnd Value="2.0"/></Loop>
                    <Notes><KeyTracks>
                      <KeyTrack Id="10">
                        <MidiKey Value="48"/>
                        <Notes>
                          <MidiNoteEvent Time="0.0" Duration="0.5" Velocity="100" IsEnabled="true"/>
                          <MidiNoteEvent Time="0.5" Duration="0.5" Velocity="100" IsEnabled="false"/>
                        </Notes>
                      </KeyTrack>
                    </KeyTracks></Notes>
                  </MidiClip>
                </Value></ClipSlot>
              </ClipSlot>
            </ClipSlotList>
          </MainSequencer>
        </DeviceChain>
      </MidiTrack>
      <ReturnTrack Id="2">
        <Name><EffectiveName Value="A-Reverb"/></Name>
        <DeviceChain>
          <Mixer><Volume><Manual Value="1.0"/></Volume></Mixer>
        </DeviceChain>
      </ReturnTrack>
      <GroupTrack Id="3">
        <Name><EffectiveName Value="Drums Group"/></Name>
        <DeviceChain>
          <Mixer><Volume><Manual Value="1.0"/></Volume></Mixer>
        </DeviceChain>
      </GroupTrack>
    </Tracks>
    <Scenes>
      <Scene Id="0">
        <Name Value="Intro"/>
        <Tempo Value="199.0"/>
        <IsTempoEnabled Value="false"/>
      </Scene>
    </Scenes>
  </LiveSet>
</Ableton>"#;

        let proj = DAWProject::from_als_xml(xml).expect("Parse external Live 12 XML");
        assert!((proj.transport.bpm - 128.0).abs() < 1e-3, "Must read MainTrack Mixer Tempo, not Scene Tempo");
        assert!(proj.master_track.volume_db.abs() < 1e-3);
        assert_eq!(proj.tracks.len(), 3);
        // Track named "909 Lead Synth" without DrumGroupDevice must remain MidiSynth!
        assert_eq!(proj.tracks[0].track_type, TrackType::MidiSynth);
        assert_eq!(proj.tracks[0].group_id, Some(3));
        assert!(proj.tracks[0].mixer.mute, "Speaker=false must mute even when On=true");
        assert_eq!(proj.tracks[1].track_type, TrackType::Return);
        assert_eq!(proj.tracks[2].track_type, TrackType::Group);

        assert!(proj.tracks[0].mixer.volume_db.abs() < 1e-3);
        // Sequential TrackSendHolder indexing: Id=62 -> index 0, Id=63 -> index 1
        assert!((proj.tracks[0].mixer.send_a_db - (-70.0)).abs() < 1e-3);
        assert!(proj.tracks[0].mixer.send_b_db.abs() < 1e-3);
        assert_eq!(proj.tracks[0].mixer.sends_db.len(), 2);

        let clip = proj.tracks[0].session_clips[0].as_ref().unwrap();
        assert!((clip.loop_end_beat - 2.0).abs() < 1e-3);
        if let ClipPattern::Midi(mp) = &clip.pattern {
            assert_eq!(mp.notes.len(), 1, "Disabled MidiNoteEvent must be ignored");
            assert!((mp.length_beats - 2.0).abs() < 1e-3);
        } else {
            panic!("Expected Midi");
        }
        assert_eq!(proj.scenes[0].bpm_override, None);
    }

    #[test]
    fn test_als_export_unique_track_ids_from_zero_and_external_eq8_and_take_lanes_exclusion() {
        // Track id == 0 must not collide with MainTrack Id="0", and clip/lane IDs inform NextPointeeId
        let mut proj = DAWProject::new("ID Collision Guard");
        let mut t0 = TrackDefinition::new_synth(0, "Zero Id Synth");
        let mut lane = AutomationLane::new(5000, "volume_db");
        lane.add_point(AutomationPoint::linear(0.0, 0.5));
        t0.automation_lanes.push(lane);
        let pat = MidiPattern::new(4.0);
        let clip = SessionClip::new(6000, "BigClip", ClipPattern::Midi(pat));
        t0.session_clips.push(Some(clip));
        proj.tracks.push(t0);

        let xml = proj.to_als_xml();
        assert!(xml.contains("<MainTrack Id=\"0\">"));
        assert!(!xml.contains("<MidiTrack Id=\"0\">"), "Track Id=0 must be remapped to >= 1");
        assert!(xml.contains("<NextPointeeId Value=\"61"));

        // External ALS with TakeLanes, nested Rack Mixer, external Eq8, LoopStart/StartRelative, Disabled
        let ext_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12402" Creator="Ableton Live 12.4">
  <LiveSet>
    <MainTrack Id="0">
      <DeviceChain>
        <Mixer>
          <Tempo><Manual Value="134.0"/></Tempo>
          <Volume><Manual Value="1.99526238"/></Volume>
        </Mixer>
      </DeviceChain>
    </MainTrack>
    <Tracks>
      <MidiTrack Id="5">
        <Name><EffectiveName Value="Rumble Rack"/></Name>
        <TrackGroupId Value="-1"/>
        <TakeLanes>
          <TakeLanes>
            <TakeLane Id="0">
              <ClipAutomation>
                <Events>
                  <MidiClip Id="999" Time="0">
                    <CurrentStart Value="0.0"/><CurrentEnd Value="4.0"/>
                    <Name Value="Unused Take"/>
                  </MidiClip>
                </Events>
              </ClipAutomation>
              <ArrangerAutomation>
                <Events>
                  <MidiClip Id="998" Time="0">
                    <CurrentStart Value="0.0"/><CurrentEnd Value="4.0"/>
                    <Name Value="Unused Take Arr"/>
                  </MidiClip>
                </Events>
              </ArrangerAutomation>
            </TakeLane>
          </TakeLanes>
        </TakeLanes>
        <DeviceChain>
          <Mixer>
            <Volume><Manual Value="1.0"/></Volume>
            <On><Manual Value="false"/></On>
          </Mixer>
          <MainSequencer>
            <ClipTimeable>
              <ArrangerAutomation>
                <Events>
                  <MidiClip Id="42" Time="8.0">
                    <CurrentStart Value="8.0"/>
                    <CurrentEnd Value="12.0"/>
                    <Disabled Value="true"/>
                    <Name Value="Main Riff"/>
                    <Loop>
                      <LoopStart Value="4.0"/>
                      <LoopEnd Value="8.0"/>
                      <StartRelative Value="1.0"/>
                      <LoopOn Value="true"/>
                    </Loop>
                    <Notes>
                      <KeyTracks>
                        <KeyTrack>
                          <MidiKey Value="36"/>
                          <Notes>
                            <MidiNoteEvent Time="2.0" Duration="0.5" Velocity="100"/>
                            <MidiNoteEvent Time="4.0" Duration="0.5" Velocity="110"/>
                            <MidiNoteEvent Time="5.5" Duration="0.5" Velocity="120"/>
                          </Notes>
                        </KeyTrack>
                      </KeyTracks>
                    </Notes>
                  </MidiClip>
                </Events>
              </ArrangerAutomation>
            </ClipTimeable>
          </MainSequencer>
          <DeviceChain>
            <Devices>
              <InstrumentGroupDevice Id="10">
                <Branches>
                  <InstrumentBranch>
                    <DeviceChain>
                      <Mixer>
                        <Volume><Manual Value="0.00031622776"/></Volume>
                        <Speaker><Manual Value="false"/></Speaker>
                      </Mixer>
                    </DeviceChain>
                  </InstrumentBranch>
                </Branches>
              </InstrumentGroupDevice>
              <Eq8 Id="11">
                <Bands.0>
                  <ParameterA>
                    <IsOn><Manual Value="true"/></IsOn>
                    <Freq><Manual Value="45.0"/></Freq>
                    <Gain><Manual Value="0.0"/></Gain>
                    <Q><Manual Value="0.707"/></Q>
                  </ParameterA>
                </Bands.0>
                <Bands.2>
                  <ParameterA>
                    <IsOn><Manual Value="true"/></IsOn>
                    <Freq><Manual Value="820.0"/></Freq>
                    <Gain><Manual Value="-3.5"/></Gain>
                    <Q><Manual Value="2.0"/></Q>
                  </ParameterA>
                </Bands.2>
              </Eq8>
            </Devices>
          </DeviceChain>
        </DeviceChain>
      </MidiTrack>
    </Tracks>
  </LiveSet>
</Ableton>"#;

        let imported = DAWProject::from_als_xml(ext_xml).expect("Import external Live 12 XML");
        assert!((imported.master_track.volume_db - 6.0).abs() < 1e-3);
        let trk = &imported.tracks[0];
        assert_eq!(trk.group_id, None);
        // Track own mixer has Volume=1.0 (0 dB), not nested Rack Mixer 0.000316 (-70 dB)!
        assert!(trk.mixer.volume_db.abs() < 1e-3);
        // Speaker absent -> falls back to On=false -> mute=true
        assert!(trk.mixer.mute);
        // Only MainSequencer clip imported (TakeLanes ignored!), and no synthetic session_clips
        assert!(trk.session_clips.is_empty());
        assert_eq!(trk.arrangement_clips.len(), 1);
        let clip = &trk.arrangement_clips[0];
        assert_eq!(clip.name, "Main Riff");
        assert!(clip.muted);
        assert!((clip.pattern_offset_beats - 1.0).abs() < 1e-3);
        if let ClipPattern::Midi(mp) = &clip.pattern {
            assert!((mp.length_beats - 4.0).abs() < 1e-3);
            assert_eq!(mp.notes.len(), 2, "Pre-loop note at 2.0 must be dropped");
            assert!((mp.notes[0].start_beat - 0.0).abs() < 1e-3);
            assert!((mp.notes[1].start_beat - 1.5).abs() < 1e-3);
        } else {
            panic!("Expected Midi");
        }
        // External Eq8 parsed without oxide-eq8 comment
        assert_eq!(trk.insert_fx.len(), 1);
        if let InsertEffectConfig::EqEight { params } = &trk.insert_fx[0] {
            assert!((params["band1_freq_hz"] - 45.0).abs() < 1e-3);
            assert!((params["band3_freq_hz"] - 820.0).abs() < 1e-3);
            assert!((params["band3_gain_db"] - (-3.5)).abs() < 1e-3);
        } else {
            panic!("Expected EqEight");
        }
    }

    #[test]
    fn test_wav_decoder_data_before_fmt_and_invalid_formats() {
        // 1. Valid WAV with "data" chunk BEFORE "fmt " chunk
        let mut wav_data_first = Vec::new();
        wav_data_first.extend_from_slice(b"RIFF");
        wav_data_first.extend_from_slice(&0u32.to_le_bytes()); // placeholder size
        wav_data_first.extend_from_slice(b"WAVE");
        // data chunk first (2 mono 16-bit samples = 4 bytes)
        wav_data_first.extend_from_slice(b"data");
        wav_data_first.extend_from_slice(&4u32.to_le_bytes());
        wav_data_first.extend_from_slice(&16384i16.to_le_bytes());
        wav_data_first.extend_from_slice(&(-16384i16).to_le_bytes());
        // fmt chunk second (48000 Hz, 1 ch, 16-bit PCM)
        wav_data_first.extend_from_slice(b"fmt ");
        wav_data_first.extend_from_slice(&16u32.to_le_bytes());
        wav_data_first.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav_data_first.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav_data_first.extend_from_slice(&48000u32.to_le_bytes());
        wav_data_first.extend_from_slice(&(48000u32 * 2).to_le_bytes());
        wav_data_first.extend_from_slice(&2u16.to_le_bytes());
        wav_data_first.extend_from_slice(&16u16.to_le_bytes());
        let riff_len = (wav_data_first.len() - 8) as u32;
        wav_data_first[4..8].copy_from_slice(&riff_len.to_le_bytes());

        let (sr, ch, samples) = decode_wav_bytes(&wav_data_first, 0.0).expect("data-before-fmt WAV must decode");
        assert_eq!(sr, 48000);
        assert_eq!(ch, 2);
        assert_eq!(samples.len(), 4);
        assert!((samples[0] - 0.5).abs() < 1e-3);
        assert!((samples[2] - (-0.5)).abs() < 1e-3);

        // 2. Non-PCM audio_format == 6 (A-law) must error
        let mut non_pcm = wav_data_first.clone();
        // audio_format is at offset 12 + 4(data) + 4(sz) + 4(payload) + 8(fmt hdr) = 32
        non_pcm[32..34].copy_from_slice(&6u16.to_le_bytes());
        assert!(decode_wav_bytes(&non_pcm, 0.0).is_err(), "Non-PCM audio_format=6 must return Err");

        // 3. channels == 0 must error
        let mut zero_ch = wav_data_first.clone();
        zero_ch[34..36].copy_from_slice(&0u16.to_le_bytes());
        assert!(decode_wav_bytes(&zero_ch, 0.0).is_err(), "channels=0 must return Err");

        // 4. sample_rate == 0 must error
        let mut zero_sr = wav_data_first.clone();
        zero_sr[36..40].copy_from_slice(&0u32.to_le_bytes());
        assert!(decode_wav_bytes(&zero_sr, 0.0).is_err(), "sample_rate=0 must return Err");
    }
}
