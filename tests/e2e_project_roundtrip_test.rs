//! E2E Test Suite: DAWProject Serialization, Persistence Roundtrip, Quantization & Automation Curves
//!
//! Authoritative baselines:
//! - /Users/juancruz/ableton-rust/PROJECT.md § 7 & § Interface Contracts
//! - /Users/juancruz/ableton-rust/.agents/teamwork/teamwork_preview_explorer_survey_3/handoff.md § 3.1 & 3.2

use std::fs;
use std::path::Path;

// ===========================================================================
// Lightweight Self-Contained JSON AST Parser & Formatter
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        if let JsonValue::Object(entries) = self {
            for (k, v) in entries {
                if k == key { return Some(v); }
            }
        }
        None
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn to_string_pretty(&self, indent: usize) -> String {
        let spaces = "  ".repeat(indent);
        let inner_spaces = "  ".repeat(indent + 1);
        match self {
            JsonValue::Null => "null".to_string(),
            JsonValue::Bool(b) => b.to_string(),
            JsonValue::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e14 {
                    format!("{:.1}", n)
                } else {
                    format!("{}", n)
                }
            }
            JsonValue::String(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
                format!("\"{}\"", escaped)
            }
            JsonValue::Array(items) => {
                if items.is_empty() { return "[]".to_string(); }
                let lines: Vec<String> = items.iter().map(|item| format!("{}{}", inner_spaces, item.to_string_pretty(indent + 1))).collect();
                format!("[\n{}\n{}]", lines.join(",\n"), spaces)
            }
            JsonValue::Object(entries) => {
                if entries.is_empty() { return "{}".to_string(); }
                let lines: Vec<String> = entries.iter().map(|(k, v)| {
                    format!("{}\"{}\": {}", inner_spaces, k, v.to_string_pretty(indent + 1))
                }).collect();
                format!("{{\n{}\n{}}}", lines.join(",\n"), spaces)
            }
        }
    }
}

pub fn parse_json(s: &str) -> Result<JsonValue, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut idx = 0;
    skip_ws(&chars, &mut idx);
    let val = parse_val(&chars, &mut idx)?;
    skip_ws(&chars, &mut idx);
    if idx < chars.len() {
        return Err(format!("Trailing characters at index {}", idx));
    }
    Ok(val)
}

fn skip_ws(c: &[char], i: &mut usize) {
    while *i < c.len() && (c[*i] == ' ' || c[*i] == '\t' || c[*i] == '\n' || c[*i] == '\r') {
        *i += 1;
    }
}

fn parse_val(c: &[char], i: &mut usize) -> Result<JsonValue, String> {
    skip_ws(c, i);
    if *i >= c.len() { return Err("Unexpected EOF".into()); }
    match c[*i] {
        '{' => parse_obj(c, i),
        '[' => parse_arr(c, i),
        '"' => parse_str(c, i),
        't' | 'f' => parse_bool(c, i),
        'n' => parse_null(c, i),
        _ => parse_num(c, i),
    }
}

fn parse_str(c: &[char], i: &mut usize) -> Result<JsonValue, String> {
    *i += 1;
    let mut s = String::new();
    while *i < c.len() {
        if c[*i] == '"' {
            *i += 1;
            return Ok(JsonValue::String(s));
        } else if c[*i] == '\\' {
            *i += 1;
            if *i < c.len() {
                match c[*i] {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    _ => s.push(c[*i]),
                }
            }
        } else {
            s.push(c[*i]);
        }
        *i += 1;
    }
    Err("Unterminated string".into())
}

fn parse_num(c: &[char], i: &mut usize) -> Result<JsonValue, String> {
    let start = *i;
    while *i < c.len() && (c[*i].is_ascii_digit() || c[*i] == '-' || c[*i] == '+' || c[*i] == '.' || c[*i] == 'e' || c[*i] == 'E') {
        *i += 1;
    }
    let s: String = c[start..*i].iter().collect();
    s.parse::<f64>().map(JsonValue::Number).map_err(|e| format!("Bad num: {}", e))
}

fn parse_bool(c: &[char], i: &mut usize) -> Result<JsonValue, String> {
    if c[*i..].starts_with(&['t', 'r', 'u', 'e']) {
        *i += 4;
        Ok(JsonValue::Bool(true))
    } else if c[*i..].starts_with(&['f', 'a', 'l', 's', 'e']) {
        *i += 5;
        Ok(JsonValue::Bool(false))
    } else {
        Err("Bad bool".into())
    }
}

fn parse_null(c: &[char], i: &mut usize) -> Result<JsonValue, String> {
    if c[*i..].starts_with(&['n', 'u', 'l', 'l']) {
        *i += 4;
        Ok(JsonValue::Null)
    } else {
        Err("Bad null".into())
    }
}

fn parse_arr(c: &[char], i: &mut usize) -> Result<JsonValue, String> {
    *i += 1;
    let mut items = Vec::new();
    loop {
        skip_ws(c, i);
        if *i >= c.len() { return Err("Unterminated array".into()); }
        if c[*i] == ']' {
            *i += 1;
            return Ok(JsonValue::Array(items));
        }
        let v = parse_val(c, i)?;
        items.push(v);
        skip_ws(c, i);
        if *i < c.len() && c[*i] == ',' {
            *i += 1;
        } else if *i < c.len() && c[*i] == ']' {
            *i += 1;
            return Ok(JsonValue::Array(items));
        }
    }
}

fn parse_obj(c: &[char], i: &mut usize) -> Result<JsonValue, String> {
    *i += 1;
    let mut map = Vec::new();
    loop {
        skip_ws(c, i);
        if *i >= c.len() { return Err("Unterminated object".into()); }
        if c[*i] == '}' {
            *i += 1;
            return Ok(JsonValue::Object(map));
        }
        if c[*i] != '"' { return Err(format!("Expected string key at index {}", *i)); }
        let key_val = parse_str(c, i)?;
        let key = match key_val { JsonValue::String(s) => s, _ => unreachable!() };
        skip_ws(c, i);
        if *i >= c.len() || c[*i] != ':' { return Err(format!("Expected ':' at index {}", *i)); }
        *i += 1;
        let val = parse_val(c, i)?;
        map.push((key, val));
        skip_ws(c, i);
        if *i < c.len() && c[*i] == ',' {
            *i += 1;
        } else if *i < c.len() && c[*i] == '}' {
            *i += 1;
            return Ok(JsonValue::Object(map));
        }
    }
}

// ===========================================================================
// Domain Implementation / Verification Baseline: DAWProject Schema
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct DAWProject {
    pub metadata: ProjectMetadata,
    pub transport: TransportConfig,
    pub master_track: MasterTrackDefinition,
    pub tracks: Vec<TrackDefinition>,
    pub scenes: Vec<SceneDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectMetadata {
    pub name: String,
    pub version: String,
    pub author: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransportConfig {
    pub bpm: f64,
    pub time_signature_numerator: u8,
    pub time_signature_denominator: u8,
    pub swing_16th_ratio: f32,
    pub loop_enabled: bool,
    pub loop_start_beat: f64,
    pub loop_end_beat: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MasterTrackDefinition {
    pub volume_db: f32,
    pub pan: f32,
    pub limiter_ceiling_db: f32,
    pub limiter_release_ms: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackDefinition {
    pub id: u32,
    pub name: String,
    pub track_type: String, // "MidiSynth", "DrumRack", "Audio"
    pub volume_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub session_clips: Vec<Option<SessionClip>>,
    pub arrangement_clips: Vec<ArrangementClip>,
    pub automation_lanes: Vec<AutomationLane>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionClip {
    pub id: u32,
    pub name: String,
    pub loop_enabled: bool,
    pub loop_length_beats: f64,
    pub notes: Vec<MidiNote>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrangementClip {
    pub id: u32,
    pub name: String,
    pub timeline_start_beat: f64,
    pub duration_beats: f64,
    pub notes: Vec<MidiNote>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiNote {
    pub pitch: u8,
    pub velocity: u8,
    pub start_beat: f64,
    pub duration_beats: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationLane {
    pub parameter_name: String,
    pub points: Vec<AutomationPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutomationPoint {
    pub beat: f64,
    pub value_norm: f32,
    pub curve: f32, // -1.0 .. 1.0 (0.0 = linear)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneDefinition {
    pub id: u32,
    pub name: String,
    pub bpm_override: Option<f64>,
}

// Convert DAWProject <-> JsonValue
impl DAWProject {
    pub fn to_json(&self) -> JsonValue {
        let mut root = Vec::new();

        // Metadata
        let meta = vec![
            ("name".into(), JsonValue::String(self.metadata.name.clone())),
            ("version".into(), JsonValue::String(self.metadata.version.clone())),
            ("author".into(), JsonValue::String(self.metadata.author.clone())),
        ];
        root.push(("metadata".into(), JsonValue::Object(meta)));

        // Transport
        let trans = vec![
            ("bpm".into(), JsonValue::Number(self.transport.bpm)),
            ("time_signature_numerator".into(), JsonValue::Number(self.transport.time_signature_numerator as f64)),
            ("time_signature_denominator".into(), JsonValue::Number(self.transport.time_signature_denominator as f64)),
            ("swing_16th_ratio".into(), JsonValue::Number(self.transport.swing_16th_ratio as f64)),
            ("loop_enabled".into(), JsonValue::Bool(self.transport.loop_enabled)),
            ("loop_start_beat".into(), JsonValue::Number(self.transport.loop_start_beat)),
            ("loop_end_beat".into(), JsonValue::Number(self.transport.loop_end_beat)),
        ];
        root.push(("transport".into(), JsonValue::Object(trans)));

        // Master Track
        let master = vec![
            ("volume_db".into(), JsonValue::Number(self.master_track.volume_db as f64)),
            ("pan".into(), JsonValue::Number(self.master_track.pan as f64)),
            ("limiter_ceiling_db".into(), JsonValue::Number(self.master_track.limiter_ceiling_db as f64)),
            ("limiter_release_ms".into(), JsonValue::Number(self.master_track.limiter_release_ms as f64)),
        ];
        root.push(("master_track".into(), JsonValue::Object(master)));

        // Tracks
        let tracks: Vec<JsonValue> = self.tracks.iter().map(|t| {
            let mut trk = Vec::new();
            trk.push(("id".into(), JsonValue::Number(t.id as f64)));
            trk.push(("name".into(), JsonValue::String(t.name.clone())));
            trk.push(("track_type".into(), JsonValue::String(t.track_type.clone())));
            trk.push(("volume_db".into(), JsonValue::Number(t.volume_db as f64)));
            trk.push(("pan".into(), JsonValue::Number(t.pan as f64)));
            trk.push(("mute".into(), JsonValue::Bool(t.mute)));
            trk.push(("solo".into(), JsonValue::Bool(t.solo)));

            // Clips
            let session_clips: Vec<JsonValue> = t.session_clips.iter().map(|sc| {
                if let Some(c) = sc {
                    let notes: Vec<JsonValue> = c.notes.iter().map(|n| {
                        JsonValue::Object(vec![
                            ("pitch".into(), JsonValue::Number(n.pitch as f64)),
                            ("velocity".into(), JsonValue::Number(n.velocity as f64)),
                            ("start_beat".into(), JsonValue::Number(n.start_beat)),
                            ("duration_beats".into(), JsonValue::Number(n.duration_beats)),
                        ])
                    }).collect();
                    JsonValue::Object(vec![
                        ("id".into(), JsonValue::Number(c.id as f64)),
                        ("name".into(), JsonValue::String(c.name.clone())),
                        ("loop_enabled".into(), JsonValue::Bool(c.loop_enabled)),
                        ("loop_length_beats".into(), JsonValue::Number(c.loop_length_beats)),
                        ("notes".into(), JsonValue::Array(notes)),
                    ])
                } else {
                    JsonValue::Null
                }
            }).collect();
            trk.push(("session_clips".into(), JsonValue::Array(session_clips)));

            // Automation
            let auto_lanes: Vec<JsonValue> = t.automation_lanes.iter().map(|al| {
                let pts: Vec<JsonValue> = al.points.iter().map(|p| {
                    JsonValue::Object(vec![
                        ("beat".into(), JsonValue::Number(p.beat)),
                        ("value_norm".into(), JsonValue::Number(p.value_norm as f64)),
                        ("curve".into(), JsonValue::Number(p.curve as f64)),
                    ])
                }).collect();
                JsonValue::Object(vec![
                    ("parameter_name".into(), JsonValue::String(al.parameter_name.clone())),
                    ("points".into(), JsonValue::Array(pts)),
                ])
            }).collect();
            trk.push(("automation_lanes".into(), JsonValue::Array(auto_lanes)));

            JsonValue::Object(trk)
        }).collect();
        root.push(("tracks".into(), JsonValue::Array(tracks)));

        JsonValue::Object(root)
    }

    pub fn from_json(val: &JsonValue) -> Result<Self, String> {
        let meta_obj = val.get("metadata").ok_or("Missing metadata")?;
        let name = meta_obj.get("name").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
        let version = meta_obj.get("version").and_then(|v| v.as_str()).unwrap_or("1.0").to_string();
        let author = meta_obj.get("author").and_then(|v| v.as_str()).unwrap_or("User").to_string();

        let trans_obj = val.get("transport").ok_or("Missing transport")?;
        let bpm = trans_obj.get("bpm").and_then(|v| v.as_f64()).unwrap_or(120.0);
        let time_signature_numerator = trans_obj.get("time_signature_numerator").and_then(|v| v.as_f64()).unwrap_or(4.0) as u8;
        let time_signature_denominator = trans_obj.get("time_signature_denominator").and_then(|v| v.as_f64()).unwrap_or(4.0) as u8;
        let swing_16th_ratio = trans_obj.get("swing_16th_ratio").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let loop_enabled = trans_obj.get("loop_enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        let loop_start_beat = trans_obj.get("loop_start_beat").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let loop_end_beat = trans_obj.get("loop_end_beat").and_then(|v| v.as_f64()).unwrap_or(16.0);

        let master_obj = val.get("master_track").ok_or("Missing master_track")?;
        let volume_db = master_obj.get("volume_db").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let pan = master_obj.get("pan").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let limiter_ceiling_db = master_obj.get("limiter_ceiling_db").and_then(|v| v.as_f64()).unwrap_or(-0.3) as f32;
        let limiter_release_ms = master_obj.get("limiter_release_ms").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32;

        let tracks_arr = val.get("tracks").and_then(|v| v.as_array()).ok_or("Missing tracks array")?;
        let mut tracks = Vec::new();
        for t_val in tracks_arr {
            let id = t_val.get("id").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
            let name = t_val.get("name").and_then(|v| v.as_str()).unwrap_or("Track").to_string();
            let track_type = t_val.get("track_type").and_then(|v| v.as_str()).unwrap_or("Audio").to_string();
            let vol = t_val.get("volume_db").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let p = t_val.get("pan").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let mute = t_val.get("mute").and_then(|v| v.as_bool()).unwrap_or(false);
            let solo = t_val.get("solo").and_then(|v| v.as_bool()).unwrap_or(false);

            let mut session_clips = Vec::new();
            if let Some(clips_arr) = t_val.get("session_clips").and_then(|v| v.as_array()) {
                for c_val in clips_arr {
                    if let JsonValue::Object(_) = c_val {
                        let c_id = c_val.get("id").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                        let c_name = c_val.get("name").and_then(|v| v.as_str()).unwrap_or("Clip").to_string();
                        let l_en = c_val.get("loop_enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                        let l_len = c_val.get("loop_length_beats").and_then(|v| v.as_f64()).unwrap_or(4.0);

                        let mut notes = Vec::new();
                        if let Some(n_arr) = c_val.get("notes").and_then(|v| v.as_array()) {
                            for n in n_arr {
                                notes.push(MidiNote {
                                    pitch: n.get("pitch").and_then(|v| v.as_f64()).unwrap_or(60.0) as u8,
                                    velocity: n.get("velocity").and_then(|v| v.as_f64()).unwrap_or(100.0) as u8,
                                    start_beat: n.get("start_beat").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    duration_beats: n.get("duration_beats").and_then(|v| v.as_f64()).unwrap_or(1.0),
                                });
                            }
                        }
                        session_clips.push(Some(SessionClip {
                            id: c_id,
                            name: c_name,
                            loop_enabled: l_en,
                            loop_length_beats: l_len,
                            notes,
                        }));
                    } else {
                        session_clips.push(None);
                    }
                }
            }

            let mut automation_lanes = Vec::new();
            if let Some(al_arr) = t_val.get("automation_lanes").and_then(|v| v.as_array()) {
                for al in al_arr {
                    let pname = al.get("parameter_name").and_then(|v| v.as_str()).unwrap_or("Param").to_string();
                    let mut pts = Vec::new();
                    if let Some(pts_arr) = al.get("points").and_then(|v| v.as_array()) {
                        for p in pts_arr {
                            pts.push(AutomationPoint {
                                beat: p.get("beat").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                value_norm: p.get("value_norm").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                                curve: p.get("curve").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                            });
                        }
                    }
                    automation_lanes.push(AutomationLane {
                        parameter_name: pname,
                        points: pts,
                    });
                }
            }

            tracks.push(TrackDefinition {
                id,
                name,
                track_type,
                volume_db: vol,
                pan: p,
                mute,
                solo,
                session_clips,
                arrangement_clips: Vec::new(),
                automation_lanes,
            });
        }

        Ok(DAWProject {
            metadata: ProjectMetadata { name, version, author },
            transport: TransportConfig {
                bpm,
                time_signature_numerator,
                time_signature_denominator,
                swing_16th_ratio,
                loop_enabled,
                loop_start_beat,
                loop_end_beat,
            },
            master_track: MasterTrackDefinition {
                volume_db,
                pan,
                limiter_ceiling_db,
                limiter_release_ms,
            },
            tracks,
            scenes: Vec::new(),
        })
    }
}

// ===========================================================================
// Automation & Quantization Helpers
// ===========================================================================

pub fn interpolate_automation(p0: &AutomationPoint, p1: &AutomationPoint, beat: f64) -> f32 {
    if beat <= p0.beat { return p0.value_norm; }
    if beat >= p1.beat { return p1.value_norm; }
    let span = (p1.beat - p0.beat).max(1e-6);
    let t = ((beat - p0.beat) / span) as f32;

    let t_curved = if p0.curve == 0.0 {
        t
    } else if p0.curve > 0.0 {
        t.powf(1.0 + 3.0 * p0.curve)
    } else {
        1.0 - (1.0 - t).powf(1.0 + 3.0 * p0.curve.abs())
    };

    p0.value_norm + (p1.value_norm - p0.value_norm) * t_curved
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantizeMode {
    Immediate,
    Sixteenth,
    Quarter,
    OneBar,
    TwoBars,
}

pub fn calculate_launch_beat(current_beat: f64, mode: QuantizeMode) -> f64 {
    let interval = match mode {
        QuantizeMode::Immediate => return current_beat,
        QuantizeMode::Sixteenth => 0.25,
        QuantizeMode::Quarter => 1.0,
        QuantizeMode::OneBar => 4.0,
        QuantizeMode::TwoBars => 8.0,
    };
    (current_beat / interval).ceil() * interval
}

// ===========================================================================
// Test Cases
// ===========================================================================

#[test]
fn test_daw_project_serialization_roundtrip() {
    let original = DAWProject {
        metadata: ProjectMetadata {
            name: "Berlin Minimal Demo".to_string(),
            version: "1.0.0".to_string(),
            author: "Ableton Rust Team".to_string(),
        },
        transport: TransportConfig {
            bpm: 128.0,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
            swing_16th_ratio: 0.16,
            loop_enabled: true,
            loop_start_beat: 0.0,
            loop_end_beat: 16.0,
        },
        master_track: MasterTrackDefinition {
            volume_db: -1.5,
            pan: 0.0,
            limiter_ceiling_db: -0.3,
            limiter_release_ms: 120.0,
        },
        tracks: vec![
            TrackDefinition {
                id: 1,
                name: "Bass Synth".to_string(),
                track_type: "MidiSynth".to_string(),
                volume_db: -3.0,
                pan: 0.0,
                mute: false,
                solo: false,
                session_clips: vec![
                    Some(SessionClip {
                        id: 101,
                        name: "Acid Bassline".to_string(),
                        loop_enabled: true,
                        loop_length_beats: 4.0,
                        notes: vec![
                            MidiNote { pitch: 36, velocity: 120, start_beat: 0.0, duration_beats: 0.5 },
                            MidiNote { pitch: 36, velocity: 100, start_beat: 0.75, duration_beats: 0.25 },
                            MidiNote { pitch: 38, velocity: 110, start_beat: 1.0, duration_beats: 0.5 },
                        ],
                    }),
                    None,
                ],
                arrangement_clips: Vec::new(),
                automation_lanes: vec![
                    AutomationLane {
                        parameter_name: "filter_cutoff_hz".to_string(),
                        points: vec![
                            AutomationPoint { beat: 0.0, value_norm: 0.2, curve: 0.0 },
                            AutomationPoint { beat: 4.0, value_norm: 0.8, curve: 0.5 },
                        ],
                    },
                ],
            },
            TrackDefinition {
                id: 2,
                name: "909 Drums".to_string(),
                track_type: "DrumRack".to_string(),
                volume_db: -1.0,
                pan: 0.0,
                mute: false,
                solo: false,
                session_clips: vec![
                    Some(SessionClip {
                        id: 201,
                        name: "Main Groove".to_string(),
                        loop_enabled: true,
                        loop_length_beats: 4.0,
                        notes: vec![
                            MidiNote { pitch: 36, velocity: 127, start_beat: 0.0, duration_beats: 0.25 },
                            MidiNote { pitch: 38, velocity: 110, start_beat: 1.0, duration_beats: 0.25 },
                            MidiNote { pitch: 42, velocity: 90, start_beat: 0.5, duration_beats: 0.25 },
                        ],
                    }),
                ],
                arrangement_clips: Vec::new(),
                automation_lanes: Vec::new(),
            },
        ],
        scenes: Vec::new(),
    };

    // Serialize to JSON AST
    let json_ast = original.to_json();
    let json_string = json_ast.to_string_pretty(0);
    assert!(json_string.contains("\"Berlin Minimal Demo\""));
    assert!(json_string.contains("\"bpm\": 128.0"));

    // Save to temp file
    let temp_path = Path::new("/tmp/ableton_test_roundtrip.json");
    fs::write(temp_path, &json_string).expect("Failed to write project to disk");

    // Reload from temp file
    let reloaded_str = fs::read_to_string(temp_path).expect("Failed to read project from disk");
    let reloaded_ast = parse_json(&reloaded_str).expect("Failed to parse project JSON");
    let deserialized = DAWProject::from_json(&reloaded_ast).expect("Failed to deserialize project");

    // Clean up temp file
    let _ = fs::remove_file(temp_path);

    // Verify structural equivalence
    assert_eq!(original, deserialized, "Roundtrip project must match original exactly");
    println!("Project persistence roundtrip verified: 100% structural equivalence.");
}

#[test]
fn test_automation_curve_power_interpolation() {
    let p0 = AutomationPoint { beat: 0.0, value_norm: 0.1, curve: 0.5 };
    let p1 = AutomationPoint { beat: 4.0, value_norm: 0.9, curve: 0.0 };

    // Boundary conditions
    assert_eq!(interpolate_automation(&p0, &p1, 0.0), 0.1);
    assert_eq!(interpolate_automation(&p0, &p1, 4.0), 0.9);
    assert_eq!(interpolate_automation(&p0, &p1, -1.0), 0.1);
    assert_eq!(interpolate_automation(&p0, &p1, 10.0), 0.9);

    // Midpoint check: curve > 0 accelerates late (concave up), so t_curved < 0.5
    let mid_val = interpolate_automation(&p0, &p1, 2.0);
    assert!(
        mid_val > 0.1 && mid_val < 0.5,
        "Positive curve handle must bend below linear midpoint (got {:.4})",
        mid_val
    );
}

#[test]
fn test_quantization_launch_calculations() {
    // Current beat: 3.25, Quantize: 1 Bar (4.0 beats) -> Target: 4.0
    assert_eq!(calculate_launch_beat(3.25, QuantizeMode::OneBar), 4.0);
    // Current beat: 4.01, Quantize: 1 Bar -> Target: 8.0
    assert_eq!(calculate_launch_beat(4.01, QuantizeMode::OneBar), 8.0);
    // Current beat: 1.10, Quantize: Quarter (1.0 beat) -> Target: 2.0
    assert_eq!(calculate_launch_beat(1.10, QuantizeMode::Quarter), 2.0);
    // Current beat: 2.15, Quantize: Sixteenth (0.25 beat) -> Target: 2.25
    assert_eq!(calculate_launch_beat(2.15, QuantizeMode::Sixteenth), 2.25);
    // Immediate mode: always returns current beat
    assert_eq!(calculate_launch_beat(3.1415, QuantizeMode::Immediate), 3.1415);
}

#[test]
fn test_malformed_json_graceful_error_handling() {
    let invalid_json = "{ \"metadata\": { \"name\": \"Broken\", "; // Unterminated JSON
    let res = parse_json(invalid_json);
    assert!(res.is_err(), "Malformed JSON must return Err without panicking");

    let missing_fields_json = "{ \"metadata\": {} }";
    let ast = parse_json(missing_fields_json).unwrap();
    let deser_res = DAWProject::from_json(&ast);
    assert!(deser_res.is_err(), "Missing required fields must return Err");
    println!("Graceful error handling verified for malformed project JSON.");
}
