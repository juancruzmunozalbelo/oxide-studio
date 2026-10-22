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

            if offset + path_len > payload_len {
                return Err(AlpError::UnexpectedEof);
            }
            let path = String::from_utf8(bytes[offset..offset + path_len].to_vec())
                .map_err(|e| AlpError::CorruptedData(format!("Invalid entry path UTF-8: {}", e)))?;
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

            if offset + data_len > payload_len {
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
            project.title
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
                track.name, track.volume_db, track.pan, track.is_muted
            ));

            for clip in &track.clips {
                project_xml.push_str(&format!(
                    "      <clip name=\"{}\" start=\"{:.3}\" duration=\"{:.3}\">\n",
                    clip.name, clip.start_beat, clip.duration_beats
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
        let title = extract_tag_value(metadata_xml, "title")
            .unwrap_or_else(|| "Untitled DAWproject".to_string());

        let tempo_str = extract_tag_value(project_xml, "tempo")
            .ok_or_else(|| DawProjectError::MissingField("tempo".into()))?;
        let tempo: f64 = tempo_str
            .parse()
            .map_err(|_| DawProjectError::XmlParseError("Invalid tempo format".into()))?;

        let time_sig_str = extract_tag_value(project_xml, "timeSignature")
            .unwrap_or_else(|| "4/4".to_string());
        let time_signature = parse_time_signature(&time_sig_str);

        let mut tracks = Vec::new();

        // Extract track blocks
        let mut cur_pos = 0;
        while let Some(track_start) = project_xml[cur_pos..].find("<track ") {
            let abs_track_start = cur_pos + track_start;
            let tag_close = match project_xml[abs_track_start..].find('>') {
                Some(idx) => abs_track_start + idx,
                None => break,
            };
            let track_header = &project_xml[abs_track_start..tag_close];

            let track_end = match project_xml[abs_track_start..].find("</track>") {
                Some(idx) => abs_track_start + idx + 8,
                None => break,
            };
            let track_body = &project_xml[tag_close + 1..track_end - 8];

            let name = extract_attr_value(track_header, "name").unwrap_or_else(|| "Track".into());
            let volume_db: f32 = extract_attr_value(track_header, "volume")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            let pan: f32 = extract_attr_value(track_header, "pan")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            let is_muted: bool = extract_attr_value(track_header, "mute")
                .map(|v| v == "true")
                .unwrap_or(false);

            let mut clips = Vec::new();
            let mut clip_pos = 0;
            while let Some(c_start) = track_body[clip_pos..].find("<clip ") {
                let abs_clip_start = clip_pos + c_start;
                let c_tag_close = match track_body[abs_clip_start..].find('>') {
                    Some(idx) => abs_clip_start + idx,
                    None => break,
                };
                let clip_header = &track_body[abs_clip_start..c_tag_close];

                let clip_end = match track_body[abs_clip_start..].find("</clip>") {
                    Some(idx) => abs_clip_start + idx + 7,
                    None => break,
                };
                let clip_body = &track_body[c_tag_close + 1..clip_end - 7];

                let clip_name = extract_attr_value(clip_header, "name").unwrap_or_else(|| "Clip".into());
                let clip_start: f64 = extract_attr_value(clip_header, "start")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0);
                let clip_duration: f64 = extract_attr_value(clip_header, "duration")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(4.0);

                let mut notes = Vec::new();
                let mut note_pos = 0;
                while let Some(n_start) = clip_body[note_pos..].find("<note ") {
                    let abs_note_start = note_pos + n_start;
                    let n_tag_close = match clip_body[abs_note_start..].find("/>") {
                        Some(idx) => abs_note_start + idx + 2,
                        None => break,
                    };
                    let note_str = &clip_body[abs_note_start..n_tag_close];

                    let key: u8 = extract_attr_value(note_str, "key")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(60);
                    let velocity: u8 = extract_attr_value(note_str, "velocity")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(100);
                    let start_beat: f64 = extract_attr_value(note_str, "start")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.0);
                    let duration_beats: f64 = extract_attr_value(note_str, "duration")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1.0);

                    notes.push(DawNote {
                        key,
                        velocity,
                        start_beat,
                        duration_beats,
                    });

                    note_pos = n_tag_close;
                }

                clips.push(DawClip {
                    name: clip_name,
                    start_beat: clip_start,
                    duration_beats: clip_duration,
                    notes,
                });

                clip_pos = clip_end;
            }

            tracks.push(DawTrack {
                name,
                volume_db,
                pan,
                is_muted,
                clips,
            });

            cur_pos = track_end;
        }

        Ok(DawProject {
            title,
            tempo,
            time_signature,
            tracks,
        })
    }
}

fn extract_tag_value(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);
    let start = xml.find(&open_tag)? + open_tag.len();
    let end = xml[start..].find(&close_tag)? + start;
    Some(xml[start..end].trim().to_string())
}

fn extract_attr_value(tag_str: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    let start = tag_str.find(&pattern)? + pattern.len();
    let end = tag_str[start..].find('"')? + start;
    Some(tag_str[start..end].to_string())
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
        let q = u >> k;
        let rem = u & ((1 << k) - 1);
        for _ in 0..q {
            self.write_bit(false);
        }
        self.write_bit(true);
        if k > 0 {
            self.write_bits(rem, k);
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
        Ok((q << k) | rem)
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
        let bitstream = &bytes[12..];
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
}
