//! Ableton Live Pack (.alp) Bundler, Lossless FLAC Freeze Cache & Open DAWproject XML Exchange.
//!
//! Provides:
//! - `AlpPackArchive`: Bundles `.als`, audio samples, and presets into compressed packages
//!   with CRC32 integrity verification (`bundle_alp`, `extract_alp`).
//! - `DawProjectExchange`: Open Bitwig / PreSonus / Ableton DAWproject (`project.xml` + `metadata.xml`)
//!   bidirectional serializer/deserializer preserving tracks, tempo, clips, notes, and mixer levels.
//! - `LosslessFreezeCodec`: Delta-LPC Rice-coded lossless audio compression for frozen track caches
//!   (`encode_lossless_pcm`, `decode_lossless_pcm`) with 100% bit-exact roundtrip verification.

/// IEEE 802.3 32-bit Cyclic Redundancy Check (CRC32).
pub fn calculate_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

// =============================================================================
// 1. Ableton Live Pack (.alp) Bundler & Extractor
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlpPackEntry {
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlpPackArchive {
    pub name: String,
    pub version: u32,
    pub entries: Vec<AlpPackEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlpError {
    InvalidMagic,
    UnsupportedVersion(u32),
    CorruptedData(String),
    ChecksumMismatch { expected: u32, calculated: u32 },
    UnexpectedEof,
}

impl std::fmt::Display for AlpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlpError::InvalidMagic => write!(f, "Invalid ALP pack magic header"),
            AlpError::UnsupportedVersion(v) => write!(f, "Unsupported ALP pack version: {}", v),
            AlpError::CorruptedData(msg) => write!(f, "Corrupted ALP pack: {}", msg),
            AlpError::ChecksumMismatch { expected, calculated } => {
                write!(
                    f,
                    "ALP checksum mismatch: expected {:#010X}, got {:#010X}",
                    expected, calculated
                )
            }
            AlpError::UnexpectedEof => write!(f, "Unexpected end of ALP archive data"),
        }
    }
}

impl std::error::Error for AlpError {}

impl AlpPackArchive {
    const MAGIC: &'static [u8; 8] = b"LIVEPACK";

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: 1,
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, path: impl Into<String>, data: Vec<u8>) {
        self.entries.push(AlpPackEntry {
            path: path.into(),
            data,
        });
    }

    /// Bundles all project files into an `.alp` binary archive with CRC32 integrity checks.
    pub fn bundle_alp(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // 1. Magic + Version
        buf.extend_from_slice(Self::MAGIC);
        buf.extend_from_slice(&self.version.to_le_bytes());

        // 2. Pack Name
        let name_bytes = self.name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(name_bytes);

        // 3. Entry Count
        buf.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());

        // 4. Entries
        for entry in &self.entries {
            let path_bytes = entry.path.as_bytes();
            buf.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(path_bytes);

            buf.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            let entry_crc = calculate_crc32(&entry.data);
            buf.extend_from_slice(&entry_crc.to_le_bytes());
            buf.extend_from_slice(&entry.data);
        }

        // 5. Global Archive Checksum
        let archive_crc = calculate_crc32(&buf);
        buf.extend_from_slice(&archive_crc.to_le_bytes());

        buf
    }

    /// Extracts an `.alp` binary archive, verifying all checksums.
    pub fn extract_alp(bytes: &[u8]) -> Result<Self, AlpError> {
        if bytes.len() < 16 {
            return Err(AlpError::UnexpectedEof);
        }

        // Verify global checksum
        let payload_len = bytes.len() - 4;
        let expected_archive_crc = u32::from_le_bytes([
            bytes[payload_len],
            bytes[payload_len + 1],
            bytes[payload_len + 2],
            bytes[payload_len + 3],
        ]);
        let actual_archive_crc = calculate_crc32(&bytes[..payload_len]);
        if expected_archive_crc != actual_archive_crc {
            return Err(AlpError::ChecksumMismatch {
                expected: expected_archive_crc,
                calculated: actual_archive_crc,
            });
        }

        let mut offset = 0;

        // Verify Magic
        if &bytes[offset..offset + 8] != Self::MAGIC {
            return Err(AlpError::InvalidMagic);
        }
        offset += 8;

        // Version
        let version = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;
        if version != 1 {
            return Err(AlpError::UnsupportedVersion(version));
        }

        // Pack Name
        if offset + 4 > payload_len {
            return Err(AlpError::UnexpectedEof);
        }
        let name_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + name_len > payload_len {
            return Err(AlpError::UnexpectedEof);
        }
        let name = String::from_utf8(bytes[offset..offset + name_len].to_vec())
            .map_err(|e| AlpError::CorruptedData(format!("Invalid pack name UTF-8: {}", e)))?;
        offset += name_len;

        // Entry Count
        if offset + 4 > payload_len {
            return Err(AlpError::UnexpectedEof);
        }
        let num_entries = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        let remaining = payload_len.saturating_sub(offset);
        let min_entry_bytes = num_entries.checked_mul(12).ok_or(AlpError::UnexpectedEof)?;
        if min_entry_bytes > remaining {
            return Err(AlpError::UnexpectedEof);
        }

        let mut entries = Vec::with_capacity(num_entries);

        for _ in 0..num_entries {
            // Path
            if offset + 4 > payload_len {
                return Err(AlpError::UnexpectedEof);
            }
            let path_len = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) as usize;
            offset += 4;

            if offset.checked_add(path_len).map_or(true, |end| end > payload_len) {
                return Err(AlpError::UnexpectedEof);
            }
            let path = String::from_utf8(bytes[offset..offset + path_len].to_vec())
                .map_err(|e| AlpError::CorruptedData(format!("Invalid entry path UTF-8: {}", e)))?;
            validate_alp_entry_path(&path)?;
            offset += path_len;

            // Data length and CRC
            if offset + 8 > payload_len {
                return Err(AlpError::UnexpectedEof);
            }
            let data_len = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) as usize;
            offset += 4;

            let expected_entry_crc = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            offset += 4;

            if offset.checked_add(data_len).map_or(true, |end| end > payload_len) {
                return Err(AlpError::UnexpectedEof);
            }
            let data = bytes[offset..offset + data_len].to_vec();
            offset += data_len;

            let actual_entry_crc = calculate_crc32(&data);
            if actual_entry_crc != expected_entry_crc {
                return Err(AlpError::ChecksumMismatch {
                    expected: expected_entry_crc,
                    calculated: actual_entry_crc,
                });
            }

            entries.push(AlpPackEntry { path, data });
        }

        Ok(AlpPackArchive {
            name,
            version,
            entries,
        })
    }
}

fn validate_alp_entry_path(path: &str) -> Result<(), AlpError> {
    if path.is_empty()
        || path.contains('\0')
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
    {
        return Err(AlpError::CorruptedData(format!("Invalid ALP entry path: {}", path)));
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(AlpError::CorruptedData(format!(
            "Absolute drive path rejected in ALP entry: {}",
            path
        )));
    }
    for component in path.split('/') {
        if component == ".." || component == "." || component.is_empty() {
            return Err(AlpError::CorruptedData(format!(
                "Unsafe path traversal component in ALP entry: {}",
                path
            )));
        }
    }
    Ok(())
}

// =============================================================================
// 2. Open DAWproject XML Exchange (project.xml + metadata.xml)
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct DawNote {
    pub key: u8,
    pub velocity: u8,
    pub start_beat: f64,
    pub duration_beats: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DawClip {
    pub name: String,
    pub start_beat: f64,
    pub duration_beats: f64,
    pub notes: Vec<DawNote>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DawTrack {
    pub name: String,
    pub volume_db: f32,
    pub pan: f32,
    pub is_muted: bool,
    pub clips: Vec<DawClip>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DawProject {
    pub title: String,
    pub tempo: f64,
    pub time_signature: (u8, u8),
    pub tracks: Vec<DawTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DawProjectError {
    XmlParseError(String),
    MissingField(String),
}

impl std::fmt::Display for DawProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DawProjectError::XmlParseError(msg) => write!(f, "DAWproject XML parse error: {}", msg),
            DawProjectError::MissingField(msg) => write!(f, "DAWproject missing field: {}", msg),
        }
    }
}

impl std::error::Error for DawProjectError {}

pub struct DawProjectExchange;

impl DawProjectExchange {
    /// Serializes active project structure into `project.xml` and `metadata.xml`.
    pub fn export_dawproject_xml(project: &DawProject) -> (String, String) {
        let metadata_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <metadata version=\"1.0\">\n  \
              <title>{}</title>\n  \
              <artist>Oxide Producer</artist>\n  \
              <originator>Oxide Studio Live 12.4.6</originator>\n\
            </metadata>",
            xml_escape(&project.title)
        );

        let mut project_xml = String::new();
        project_xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<project version=\"1.0\">\n");
        project_xml.push_str(&format!(
            "  <transport>\n    <tempo>{:.2}</tempo>\n    <timeSignature>{}/{}</timeSignature>\n  </transport>\n",
            project.tempo, project.time_signature.0, project.time_signature.1
        ));

        project_xml.push_str("  <tracks>\n");
        for track in &project.tracks {
            project_xml.push_str(&format!(
                "    <track name=\"{}\" volume=\"{:.2}\" pan=\"{:.2}\" mute=\"{}\">\n",
                xml_escape(&track.name), track.volume_db, track.pan, track.is_muted
            ));

            for clip in &track.clips {
                project_xml.push_str(&format!(
                    "      <clip name=\"{}\" start=\"{:.3}\" duration=\"{:.3}\">\n",
                    xml_escape(&clip.name), clip.start_beat, clip.duration_beats
                ));

                for note in &clip.notes {
                    project_xml.push_str(&format!(
                        "        <note key=\"{}\" velocity=\"{}\" start=\"{:.3}\" duration=\"{:.3}\" />\n",
                        note.key, note.velocity, note.start_beat, note.duration_beats
                    ));
                }

                project_xml.push_str("      </clip>\n");
            }

            project_xml.push_str("    </track>\n");
        }
        project_xml.push_str("  </tracks>\n</project>");

        (project_xml, metadata_xml)
    }

    /// Deserializes `project.xml` and `metadata.xml` into a `DawProject`.
    pub fn import_dawproject_xml(
        project_xml: &str,
        metadata_xml: &str,
    ) -> Result<DawProject, DawProjectError> {
        let meta_nodes = parse_xml_document(metadata_xml)?;
        let title = find_element_text(&meta_nodes, "title")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Untitled DAWproject".to_string());

        let project_nodes = parse_xml_document(project_xml)?;
        let tempo_str = find_element_text(&project_nodes, "tempo")
            .ok_or_else(|| DawProjectError::MissingField("tempo".into()))?;
        let tempo: f64 = tempo_str
            .trim()
            .parse()
            .map_err(|_| DawProjectError::XmlParseError("Invalid tempo format".into()))?;

        let time_sig_str = find_element_text(&project_nodes, "timeSignature")
            .unwrap_or_else(|| "4/4".to_string());
        let time_signature = parse_time_signature(&time_sig_str);

        let mut track_elems = Vec::new();
        collect_elements_by_name(&project_nodes, "track", &mut track_elems);

        let mut tracks = Vec::with_capacity(track_elems.len());
        for t_el in track_elems {
            let name = t_el.attr("name").unwrap_or("Track").to_string();
            let volume_db: f32 = t_el
                .attr("volume")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            let pan: f32 = t_el
                .attr("pan")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            let is_muted: bool = t_el.attr("mute").map(|v| v == "true").unwrap_or(false);

            let mut clips = Vec::new();
            for c_el in t_el.children.iter().filter(|c| c.name == "clip") {
                let clip_name = c_el.attr("name").unwrap_or("Clip").to_string();
                let clip_start: f64 = c_el
                    .attr("start")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0);
                let clip_duration: f64 = c_el
                    .attr("duration")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(4.0);

                let mut notes = Vec::new();
                for n_el in c_el.children.iter().filter(|n| n.name == "note") {
                    let key: u8 = n_el.attr("key").and_then(|v| v.parse().ok()).unwrap_or(60);
                    let velocity: u8 = n_el
                        .attr("velocity")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(100);
                    let start_beat: f64 = n_el
                        .attr("start")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.0);
                    let duration_beats: f64 = n_el
                        .attr("duration")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1.0);

                    notes.push(DawNote {
                        key,
                        velocity,
                        start_beat,
                        duration_beats,
                    });
                }

                clips.push(DawClip {
                    name: clip_name,
                    start_beat: clip_start,
                    duration_beats: clip_duration,
                    notes,
                });
            }

            tracks.push(DawTrack {
                name,
                volume_db,
                pan,
                is_muted,
                clips,
            });
        }

        Ok(DawProject {
            title,
            tempo,
            time_signature,
            tracks,
        })
    }
}

#[derive(Debug, Clone)]
struct XmlElement {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<XmlElement>,
}

impl XmlElement {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn xml_unescape(s: &str) -> Result<String, DawProjectError> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = after
            .find(';')
            .ok_or_else(|| DawProjectError::XmlParseError("Unterminated XML entity".into()))?;
        let entity = &after[..semi];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                let code = u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| DawProjectError::XmlParseError(format!("Invalid XML entity &{};", entity)))?;
                out.push(code);
            }
            _ if entity.starts_with('#') => {
                let code = entity[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| DawProjectError::XmlParseError(format!("Invalid XML entity &{};", entity)))?;
                out.push(code);
            }
            _ => {
                return Err(DawProjectError::XmlParseError(format!(
                    "Unknown XML entity &{};",
                    entity
                )))
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn find_tag_end_respecting_quotes(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => {
                if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b'>' {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

fn parse_start_tag_and_attrs(tag_inner: &str) -> Result<(String, Vec<(String, String)>, bool), DawProjectError> {
    let trimmed_end = tag_inner.trim_end();
    let (body, self_closing) = if let Some(stripped) = trimmed_end.strip_suffix('/') {
        (stripped.trim(), true)
    } else {
        (trimmed_end.trim(), false)
    };
    if body.is_empty() {
        return Err(DawProjectError::XmlParseError("Empty XML tag name".into()));
    }

    let name_end = body
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(body.len());
    let name = body[..name_end].to_string();
    let mut rest = body[name_end..].trim_start();
    let mut attrs = Vec::new();

    while !rest.is_empty() {
        let eq = rest
            .find('=')
            .ok_or_else(|| DawProjectError::XmlParseError(format!("Malformed attribute in <{}>", name)))?;
        let attr_name = rest[..eq].trim();
        if attr_name.is_empty() || attr_name.contains(char::is_whitespace) {
            return Err(DawProjectError::XmlParseError(format!("Invalid attribute name in <{}>", name)));
        }
        rest = rest[eq + 1..].trim_start();
        let quote = rest
            .chars()
            .next()
            .filter(|&c| c == '"' || c == '\'')
            .ok_or_else(|| DawProjectError::XmlParseError(format!("Missing quote for attribute '{}'", attr_name)))?;
        rest = &rest[1..];
        let val_end = rest
            .find(quote)
            .ok_or_else(|| DawProjectError::XmlParseError(format!("Unterminated attribute '{}' value", attr_name)))?;
        let attr_val = xml_unescape(&rest[..val_end])?;
        attrs.push((attr_name.to_string(), attr_val));
        rest = rest[val_end + 1..].trim_start();
    }

    Ok((name, attrs, self_closing))
}

fn parse_xml_document(xml: &str) -> Result<Vec<XmlElement>, DawProjectError> {
    let mut roots = Vec::new();
    let mut stack: Vec<XmlElement> = Vec::new();
    let mut rest = xml.trim_start_matches('\u{feff}');

    while !rest.is_empty() {
        if let Some(lt) = rest.find('<') {
            if lt > 0 {
                let text = xml_unescape(&rest[..lt])?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err(DawProjectError::XmlParseError("Unexpected text outside root element".into()));
                }
            }
            rest = &rest[lt..];

            if let Some(comment_rest) = rest.strip_prefix("<!--") {
                let end = comment_rest
                    .find("-->")
                    .ok_or_else(|| DawProjectError::XmlParseError("Unterminated XML comment".into()))?;
                rest = &comment_rest[end + 3..];
            } else if let Some(cdata_rest) = rest.strip_prefix("<![CDATA[") {
                let end = cdata_rest
                    .find("]]>")
                    .ok_or_else(|| DawProjectError::XmlParseError("Unterminated CDATA section".into()))?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&cdata_rest[..end]);
                }
                rest = &cdata_rest[end + 3..];
            } else if let Some(pi_rest) = rest.strip_prefix("<?") {
                let end = pi_rest
                    .find("?>")
                    .ok_or_else(|| DawProjectError::XmlParseError("Unterminated XML processing instruction".into()))?;
                rest = &pi_rest[end + 2..];
            } else if let Some(doctype_rest) = rest.strip_prefix("<!") {
                let end = doctype_rest
                    .find('>')
                    .ok_or_else(|| DawProjectError::XmlParseError("Unterminated XML declaration".into()))?;
                rest = &doctype_rest[end + 1..];
            } else if let Some(close_rest) = rest.strip_prefix("</") {
                let end = close_rest
                    .find('>')
                    .ok_or_else(|| DawProjectError::XmlParseError("Unterminated closing XML tag".into()))?;
                let close_name = close_rest[..end].trim();
                let popped = stack.pop().ok_or_else(|| {
                    DawProjectError::XmlParseError(format!("Unexpected closing tag </{}>", close_name))
                })?;
                if popped.name != close_name {
                    return Err(DawProjectError::XmlParseError(format!(
                        "Mismatched closing tag: expected </{}>, found </{}>",
                        popped.name, close_name
                    )));
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(popped);
                } else {
                    roots.push(popped);
                }
                rest = &close_rest[end + 1..];
            } else {
                let after_lt = &rest[1..];
                let end = find_tag_end_respecting_quotes(after_lt)
                    .ok_or_else(|| DawProjectError::XmlParseError("Unterminated XML start tag".into()))?;
                let inner = &after_lt[..end];
                let (name, attrs, self_closing) = parse_start_tag_and_attrs(inner)?;
                let el = XmlElement {
                    name,
                    attrs,
                    text: String::new(),
                    children: Vec::new(),
                };
                if self_closing {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(el);
                    } else {
                        roots.push(el);
                    }
                } else {
                    stack.push(el);
                }
                rest = &after_lt[end + 1..];
            }
        } else {
            if !rest.trim().is_empty() {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&xml_unescape(rest)?);
                } else {
                    return Err(DawProjectError::XmlParseError("Unexpected trailing text outside XML root".into()));
                }
            }
            break;
        }
    }

    if let Some(unclosed) = stack.pop() {
        return Err(DawProjectError::XmlParseError(format!(
            "Unclosed XML tag <{}>",
            unclosed.name
        )));
    }

    Ok(roots)
}

fn find_element_text(nodes: &[XmlElement], target: &str) -> Option<String> {
    for node in nodes {
        if node.name == target {
            return Some(node.text.trim().to_string());
        }
        if let Some(found) = find_element_text(&node.children, target) {
            return Some(found);
        }
    }
    None
}

fn collect_elements_by_name<'a>(nodes: &'a [XmlElement], target: &str, out: &mut Vec<&'a XmlElement>) {
    for node in nodes {
        if node.name == target {
            out.push(node);
        }
        collect_elements_by_name(&node.children, target, out);
    }
}

fn parse_time_signature(sig: &str) -> (u8, u8) {
    let parts: Vec<&str> = sig.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].trim().parse().unwrap_or(4);
        let den = parts[1].trim().parse().unwrap_or(4);
        (num, den)
    } else {
        (4, 4)
    }
}

// =============================================================================
// 3. Lossless FLAC Freeze Codec (Delta-LPC + Rice Coding)
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreezeCodecError {
    InvalidMagic,
    CorruptData(String),
    UnexpectedEof,
}

impl std::fmt::Display for FreezeCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FreezeCodecError::InvalidMagic => write!(f, "Invalid freeze cache magic header"),
            FreezeCodecError::CorruptData(msg) => write!(f, "Corrupted freeze cache data: {}", msg),
            FreezeCodecError::UnexpectedEof => write!(f, "Unexpected end of freeze cache stream"),
        }
    }
}

impl std::error::Error for FreezeCodecError {}

#[inline]
fn zigzag_encode(r: i32) -> u32 {
    ((r << 1) ^ (r >> 31)) as u32
}

#[inline]
fn zigzag_decode(u: u32) -> i32 {
    ((u >> 1) as i32) ^ (-((u & 1) as i32))
}

struct BitWriter {
    bytes: Vec<u8>,
    current_byte: u8,
    num_bits: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            current_byte: 0,
            num_bits: 0,
        }
    }

    fn write_bit(&mut self, bit: bool) {
        if bit {
            self.current_byte |= 1 << (7 - self.num_bits);
        }
        self.num_bits += 1;
        if self.num_bits == 8 {
            self.bytes.push(self.current_byte);
            self.current_byte = 0;
            self.num_bits = 0;
        }
    }

    fn write_bits(&mut self, value: u32, count: u8) {
        for i in (0..count).rev() {
            let bit = ((value >> i) & 1) != 0;
            self.write_bit(bit);
        }
    }

    fn write_rice(&mut self, u: u32, k: u8) {
        let shift = k.min(31);
        let q = u >> shift;
        let rem = if shift == 0 {
            0
        } else {
            u & (((1u64 << shift) - 1) as u32)
        };
        for _ in 0..q {
            self.write_bit(false);
        }
        self.write_bit(true);
        if shift > 0 {
            self.write_bits(rem, shift);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.num_bits > 0 {
            self.bytes.push(self.current_byte);
        }
        self.bytes
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    byte_idx: usize,
    bit_idx: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_idx: 0,
            bit_idx: 0,
        }
    }

    fn read_bit(&mut self) -> Result<bool, FreezeCodecError> {
        if self.byte_idx >= self.data.len() {
            return Err(FreezeCodecError::UnexpectedEof);
        }
        let bit = (self.data[self.byte_idx] >> (7 - self.bit_idx)) & 1;
        self.bit_idx += 1;
        if self.bit_idx == 8 {
            self.byte_idx += 1;
            self.bit_idx = 0;
        }
        Ok(bit != 0)
    }

    fn read_bits(&mut self, count: u8) -> Result<u32, FreezeCodecError> {
        let mut val = 0u32;
        for _ in 0..count {
            let bit = self.read_bit()?;
            val = (val << 1) | (bit as u32);
        }
        Ok(val)
    }

    fn read_rice(&mut self, k: u8) -> Result<u32, FreezeCodecError> {
        if k > 31 {
            return Err(FreezeCodecError::CorruptData(format!(
                "Invalid Rice parameter k={}",
                k
            )));
        }
        let mut q = 0u32;
        while !self.read_bit()? {
            q += 1;
            if q > 20_000_000 {
                return Err(FreezeCodecError::CorruptData("Excessive unary run".into()));
            }
        }
        let rem = if k > 0 {
            self.read_bits(k)?
        } else {
            0
        };
        let shifted = (q as u64) << (k as u32);
        let combined = shifted | (rem as u64);
        u32::try_from(combined).map_err(|_| {
            FreezeCodecError::CorruptData("Rice sample overflows u32".into())
        })
    }
}

pub struct LosslessFreezeCodec;

impl LosslessFreezeCodec {
    const MAGIC: &'static [u8; 4] = b"FRZ1";

    /// Lossless delta-LPC Rice encoder for PCM integer audio samples.
    pub fn encode_lossless_pcm(samples: &[i32]) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(Self::MAGIC);
        output.extend_from_slice(&(samples.len() as u32).to_le_bytes());

        if samples.is_empty() {
            output.push(0); // k = 0
            output.extend_from_slice(&[0, 0, 0]);
            return output;
        }

        // 1. Calculate residuals via order-1 delta prediction
        let mut residuals = Vec::with_capacity(samples.len());
        residuals.push(samples[0]);
        for i in 1..samples.len() {
            residuals.push(samples[i].wrapping_sub(samples[i - 1]));
        }

        // 2. Map to unsigned ZigZag
        let zigzags: Vec<u32> = residuals.iter().map(|&r| zigzag_encode(r)).collect();

        // 3. Search optimal Rice parameter k (0..16)
        let mut best_k = 0u8;
        let mut min_total_bits = u64::MAX;

        for k in 0..=16u8 {
            let mut total_bits = 0u64;
            for &u in &zigzags {
                let q = (u >> k) as u64;
                total_bits += q + 1 + (k as u64);
            }
            if total_bits < min_total_bits {
                min_total_bits = total_bits;
                best_k = k;
            }
        }

        output.push(best_k);
        output.extend_from_slice(&[0, 0, 0]); // 3 reserved padding bytes

        // 4. Encode bitstream
        let mut writer = BitWriter::new();
        for &u in &zigzags {
            writer.write_rice(u, best_k);
        }

        output.extend(writer.finish());
        output
    }

    /// Lossless decoder restoring exact bit-identical integer PCM samples.
    pub fn decode_lossless_pcm(bytes: &[u8]) -> Result<Vec<i32>, FreezeCodecError> {
        if bytes.len() < 12 {
            return Err(FreezeCodecError::UnexpectedEof);
        }

        if &bytes[0..4] != Self::MAGIC {
            return Err(FreezeCodecError::InvalidMagic);
        }

        let num_samples = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        if num_samples == 0 {
            return Ok(Vec::new());
        }

        let k = bytes[8];
        if k > 31 {
            return Err(FreezeCodecError::CorruptData(format!(
                "Invalid Rice parameter k={}",
                k
            )));
        }
        let bitstream = &bytes[12..];
        let min_bits_per_sample = 1usize + k as usize;
        let max_possible_samples = bitstream.len().saturating_mul(8) / min_bits_per_sample;
        if num_samples > max_possible_samples {
            return Err(FreezeCodecError::UnexpectedEof);
        }
        let mut reader = BitReader::new(bitstream);

        let mut samples = Vec::with_capacity(num_samples);

        // First sample
        let u0 = reader.read_rice(k)?;
        let mut prev = zigzag_decode(u0);
        samples.push(prev);

        for _ in 1..num_samples {
            let u = reader.read_rice(k)?;
            let r = zigzag_decode(u);
            let val = prev.wrapping_add(r);
            samples.push(val);
            prev = val;
        }

        Ok(samples)
    }

    /// Lossless 32-bit float audio encoder via bit-level integer conversion.
    pub fn encode_lossless_f32(samples: &[f32]) -> Vec<u8> {
        let ints: Vec<i32> = samples.iter().map(|&s| s.to_bits() as i32).collect();
        Self::encode_lossless_pcm(&ints)
    }

    /// Lossless 32-bit float audio decoder with bit-exact float fidelity.
    pub fn decode_lossless_f32(bytes: &[u8]) -> Result<Vec<f32>, FreezeCodecError> {
        let ints = Self::decode_lossless_pcm(bytes)?;
        Ok(ints.into_iter().map(|i| f32::from_bits(i as u32)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alp_pack_bundle_and_extract_roundtrip() {
        let mut pack = AlpPackArchive::new("Suite Expansion 2026");
        pack.add_entry("Project.als", b"<AbletonLiveSet version=\"12\"/>".to_vec());
        pack.add_entry("Samples/Kick909.wav", vec![0x52, 0x49, 0x46, 0x46, 0x12, 0x34]);
        pack.add_entry("Presets/LeadSynth.adv", b"PRESET_BINARY_BLOB".to_vec());

        let bundled = pack.bundle_alp();
        assert!(!bundled.is_empty(), "Bundled ALP must not be empty");

        let extracted = AlpPackArchive::extract_alp(&bundled).expect("Failed to extract valid ALP");
        assert_eq!(extracted.name, "Suite Expansion 2026");
        assert_eq!(extracted.entries.len(), 3);
        assert_eq!(extracted.entries[0].path, "Project.als");
        assert_eq!(extracted.entries[0].data, b"<AbletonLiveSet version=\"12\"/>");
        assert_eq!(extracted.entries[1].path, "Samples/Kick909.wav");
        assert_eq!(extracted.entries[2].path, "Presets/LeadSynth.adv");
    }

    #[test]
    fn test_alp_pack_extract_checksum_rejection() {
        let mut pack = AlpPackArchive::new("Tampered Pack");
        pack.add_entry("Secret.als", b"AUTHENTIC_AUDIO_DATA".to_vec());
        let mut bundled = pack.bundle_alp();

        // Corrupt a byte in the payload
        let corrupt_idx = bundled.len() / 2;
        bundled[corrupt_idx] ^= 0xFF;

        let result = AlpPackArchive::extract_alp(&bundled);
        assert!(
            result.is_err(),
            "Corrupted ALP payload must trigger checksum rejection"
        );
    }

    #[test]
    fn test_dawproject_xml_export_and_import_roundtrip() {
        let project = DawProject {
            title: "Techno Odyssey".to_string(),
            tempo: 138.5,
            time_signature: (4, 4),
            tracks: vec![
                DawTrack {
                    name: "Bass 303".to_string(),
                    volume_db: -3.5,
                    pan: -0.25,
                    is_muted: false,
                    clips: vec![DawClip {
                        name: "Acid Bassline".to_string(),
                        start_beat: 0.0,
                        duration_beats: 8.0,
                        notes: vec![
                            DawNote {
                                key: 36,
                                velocity: 110,
                                start_beat: 0.0,
                                duration_beats: 0.5,
                            },
                            DawNote {
                                key: 48,
                                velocity: 127,
                                start_beat: 0.5,
                                duration_beats: 0.25,
                            },
                        ],
                    }],
                },
                DawTrack {
                    name: "TR-909 Drums".to_string(),
                    volume_db: 0.0,
                    pan: 0.0,
                    is_muted: true,
                    clips: Vec::new(),
                },
            ],
        };

        let (project_xml, metadata_xml) = DawProjectExchange::export_dawproject_xml(&project);
        assert!(project_xml.contains("<tempo>138.50</tempo>"));
        assert!(project_xml.contains("Acid Bassline"));
        assert!(metadata_xml.contains("<title>Techno Odyssey</title>"));

        let imported = DawProjectExchange::import_dawproject_xml(&project_xml, &metadata_xml)
            .expect("Failed to import DAWproject XML");

        assert_eq!(imported.title, "Techno Odyssey");
        assert!((imported.tempo - 138.5).abs() < 1e-2);
        assert_eq!(imported.time_signature, (4, 4));
        assert_eq!(imported.tracks.len(), 2);
        assert_eq!(imported.tracks[0].name, "Bass 303");
        assert!((imported.tracks[0].volume_db - (-3.5)).abs() < 1e-2);
        assert_eq!(imported.tracks[0].clips.len(), 1);
        assert_eq!(imported.tracks[0].clips[0].notes.len(), 2);
        assert_eq!(imported.tracks[0].clips[0].notes[0].key, 36);
        assert!(imported.tracks[1].is_muted);
    }

    #[test]
    fn test_lossless_freeze_codec_pcm_bit_exact_roundtrip() {
        // Generate dynamic PCM samples representing audio
        let mut samples = Vec::new();
        let mut val = 0i32;
        for i in 0..1000 {
            // Smoothly varying gradient + step changes
            val = val.wrapping_add((((i * 17) % 65) - 32) * 50);
            samples.push(val);
        }

        let encoded = LosslessFreezeCodec::encode_lossless_pcm(&samples);
        assert!(
            encoded.len() < samples.len() * 4,
            "Lossless compression must reduce memory footprint (got {} vs raw {})",
            encoded.len(),
            samples.len() * 4
        );

        let decoded = LosslessFreezeCodec::decode_lossless_pcm(&encoded)
            .expect("Failed to decode lossless freeze cache");
        assert_eq!(
            samples, decoded,
            "Decoded PCM samples must be 100% bit-exact to original input"
        );
    }

    #[test]
    fn test_lossless_freeze_codec_f32_fidelity() {
        let f32_samples = vec![0.0f32, 0.5, -0.75, 0.999, -1.0, 0.123456];
        let encoded = LosslessFreezeCodec::encode_lossless_f32(&f32_samples);
        let decoded = LosslessFreezeCodec::decode_lossless_f32(&encoded)
            .expect("Failed to decode f32 freeze cache");

        assert_eq!(f32_samples.len(), decoded.len());
        for i in 0..f32_samples.len() {
            assert_eq!(
                f32_samples[i].to_bits(),
                decoded[i].to_bits(),
                "Float sample {} must match bit-exactly",
                i
            );
        }
    }

    #[test]
    fn test_alp_pack_empty_archive_and_invalid_magic() {
        let empty_pack = AlpPackArchive::new("Empty Live Pack");
        let bundled = empty_pack.bundle_alp();
        let extracted = AlpPackArchive::extract_alp(&bundled).expect("Failed to extract empty ALP");
        assert_eq!(extracted.name, "Empty Live Pack");
        assert!(extracted.entries.is_empty());

        let mut bad_magic = bundled.clone();
        bad_magic[0..8].copy_from_slice(b"BADMAGIC");
        // Recompute global checksum for the bad magic payload
        let payload_len = bad_magic.len() - 4;
        let new_crc = calculate_crc32(&bad_magic[..payload_len]);
        bad_magic[payload_len..].copy_from_slice(&new_crc.to_le_bytes());

        let err = AlpPackArchive::extract_alp(&bad_magic).expect_err("Must reject invalid magic");
        assert_eq!(err, AlpError::InvalidMagic);
    }

    #[test]
    fn test_lossless_freeze_codec_empty_and_error_handling() {
        let empty_pcm: Vec<i32> = Vec::new();
        let enc = LosslessFreezeCodec::encode_lossless_pcm(&empty_pcm);
        let dec = LosslessFreezeCodec::decode_lossless_pcm(&enc).expect("Decode empty PCM");
        assert!(dec.is_empty());

        let err_magic = LosslessFreezeCodec::decode_lossless_pcm(b"BADM12345678");
        assert_eq!(err_magic, Err(FreezeCodecError::InvalidMagic));

        let err_eof = LosslessFreezeCodec::decode_lossless_pcm(b"FRZ");
        assert_eq!(err_eof, Err(FreezeCodecError::UnexpectedEof));
    }

    #[test]
    fn test_dawproject_missing_tempo_error() {
        let invalid_xml = "<?xml version=\"1.0\"?><project><tracks></tracks></project>";
        let meta = "<?xml version=\"1.0\"?><metadata><title>Test</title></metadata>";
        let res = DawProjectExchange::import_dawproject_xml(invalid_xml, meta);
        assert!(matches!(res, Err(DawProjectError::MissingField(f)) if f == "tempo"));
    }

    #[test]
    fn test_alp_rejects_path_traversal_and_oom_entry_count() {
        let mut pack = AlpPackArchive::new("Evil Pack");
        pack.add_entry("../../etc/passwd", b"x".to_vec());
        let bundled = pack.bundle_alp();
        assert!(matches!(
            AlpPackArchive::extract_alp(&bundled),
            Err(AlpError::CorruptedData(_))
        ));

        // Craft header with huge num_entries (u32::MAX) + valid global CRC
        let mut crafted = Vec::new();
        crafted.extend_from_slice(AlpPackArchive::MAGIC);
        crafted.extend_from_slice(&1u32.to_le_bytes()); // version
        crafted.extend_from_slice(&1u32.to_le_bytes()); // name len
        crafted.push(b'a');
        crafted.extend_from_slice(&u32::MAX.to_le_bytes()); // huge num_entries
        let crc = calculate_crc32(&crafted);
        crafted.extend_from_slice(&crc.to_le_bytes());
        assert_eq!(AlpPackArchive::extract_alp(&crafted), Err(AlpError::UnexpectedEof));
    }

    #[test]
    fn test_dawproject_xml_escaping_comments_and_attr_angle_brackets() {
        let project = DawProject {
            title: "Acid & <Lead> \"303\" 'Set'".to_string(),
            tempo: 132.0,
            time_signature: (3, 4),
            tracks: vec![DawTrack {
                name: "Track <1> & \"Main\"".to_string(),
                volume_db: -1.5,
                pan: 0.0,
                is_muted: false,
                clips: vec![DawClip {
                    name: "Clip <A> & 'B'".to_string(),
                    start_beat: 0.0,
                    duration_beats: 4.0,
                    notes: Vec::new(),
                }],
            }],
        };
        let (px, mx) = DawProjectExchange::export_dawproject_xml(&project);
        assert!(mx.contains("Acid &amp; &lt;Lead&gt;"));
        let imported = DawProjectExchange::import_dawproject_xml(&px, &mx).unwrap();
        assert_eq!(imported.title, project.title);
        assert_eq!(imported.tracks[0].name, project.tracks[0].name);
        assert_eq!(imported.tracks[0].clips[0].name, project.tracks[0].clips[0].name);

        // Comments containing <track .../> and self-closing <track/> must parse safely
        let commented = "<?xml version=\"1.0\"?>\n<project><transport><tempo>125</tempo><timeSignature>4/4</timeSignature></transport><tracks><!-- <track name=\"Fake\"/> --><track name='Real' volume='0.0' pan='0.0' mute='true' /></tracks></project>";
        let parsed = DawProjectExchange::import_dawproject_xml(commented, "<metadata><title>X</title></metadata>").unwrap();
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].name, "Real");
        assert!(parsed.tracks[0].is_muted);
    }

    #[test]
    fn test_lossless_freeze_codec_rejects_large_k_and_oom_num_samples() {
        let mut bad_k = Vec::new();
        bad_k.extend_from_slice(b"FRZ1");
        bad_k.extend_from_slice(&1u32.to_le_bytes()); // num_samples = 1
        bad_k.extend_from_slice(&[32, 0, 0, 0]); // k = 32 (> 31!)
        bad_k.push(0x80);
        assert!(matches!(
            LosslessFreezeCodec::decode_lossless_pcm(&bad_k),
            Err(FreezeCodecError::CorruptData(_))
        ));

        let mut huge_n = Vec::new();
        huge_n.extend_from_slice(b"FRZ1");
        huge_n.extend_from_slice(&u32::MAX.to_le_bytes());
        huge_n.extend_from_slice(&[4, 0, 0, 0]);
        huge_n.push(0xFF);
        assert_eq!(
            LosslessFreezeCodec::decode_lossless_pcm(&huge_n),
            Err(FreezeCodecError::UnexpectedEof)
        );
    }
}
