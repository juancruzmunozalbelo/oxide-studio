//! Bidirectional Ableton Live 12 `.als` Live Set XML generation, gzip compression/decompression, and parsing.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use oxide_timeline::{
    ClipPattern, MidiNote, MidiPattern, Scene, SessionClip,
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

impl DAWProject {
    /// Emit a complete, valid Ableton Live 12 XML string.
    pub fn to_als_xml(&self) -> String {
        let mut out = String::with_capacity(16384);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<Ableton MajorVersion=\"5\" MinorVersion=\"12.0_12049\" SchemaChangeCount=\"3\" Creator=\"Oxide Studio\" Revision=\"oxide-12.0\">\n");
        out.push_str("  <LiveSet>\n");

        // Metadata & Annotation
        out.push_str("    <Annotation Value=\"");
        out.push_str(&xml_escape(&self.metadata.name));
        out.push_str("\"/>\n");
        out.push_str("    <Metadata>\n");
        out.push_str(&format!(
            "      <Name Value=\"{}\"/>\n",
            xml_escape(&self.metadata.name)
        ));
        out.push_str(&format!(
            "      <Author Value=\"{}\"/>\n",
            xml_escape(&self.metadata.author)
        ));
        out.push_str(&format!(
            "      <Version Value=\"{}\"/>\n",
            xml_escape(&self.metadata.version)
        ));
        out.push_str(&format!(
            "      <CreatedAt Value=\"{}\"/>\n",
            xml_escape(&self.metadata.created_at_utc)
        ));
        out.push_str(&format!(
            "      <ModifiedAt Value=\"{}\"/>\n",
            xml_escape(&self.metadata.modified_at_utc)
        ));
        out.push_str("    </Metadata>\n");

        // Tempo
        out.push_str("    <Tempo>\n");
        out.push_str(&format!(
            "      <Manual Value=\"{:.4}\"/>\n",
            self.transport.bpm
        ));
        out.push_str("    </Tempo>\n");

        // Time Signature
        out.push_str("    <TimeSignature>\n");
        out.push_str("      <TimeSignatures>\n");
        out.push_str("        <RemoteableTimeSignature>\n");
        out.push_str(&format!(
            "          <Numerator Value=\"{}\"/>\n",
            self.transport.time_signature_numerator
        ));
        out.push_str(&format!(
            "          <Denominator Value=\"{}\"/>\n",
            self.transport.time_signature_denominator
        ));
        out.push_str("        </RemoteableTimeSignature>\n");
        out.push_str("      </TimeSignatures>\n");
        out.push_str("    </TimeSignature>\n");

        // Transport
        let loop_len = (self.transport.loop_end_beat - self.transport.loop_start_beat).max(0.0);
        out.push_str("    <Transport>\n");
        out.push_str(&format!(
            "      <LoopOn Value=\"{}\"/>\n",
            self.transport.loop_enabled
        ));
        out.push_str(&format!(
            "      <LoopStart Value=\"{:.4}\"/>\n",
            self.transport.loop_start_beat
        ));
        out.push_str(&format!(
            "      <LoopLength Value=\"{:.4}\"/>\n",
            loop_len
        ));
        out.push_str("    </Transport>\n");

        // Master Track
        out.push_str("    <MasterTrack Id=\"0\">\n");
        out.push_str("      <Name>\n");
        out.push_str("        <EffectiveName Value=\"Master\"/>\n");
        out.push_str("        <UserName Value=\"Master\"/>\n");
        out.push_str("      </Name>\n");
        out.push_str("      <DeviceChain>\n");
        out.push_str("        <Mixer>\n");
        out.push_str(&format!(
            "          <Volume><Manual Value=\"{:.4}\"/></Volume>\n",
            self.master_track.volume_db
        ));
        out.push_str(&format!(
            "          <Pan><Manual Value=\"{:.4}\"/></Pan>\n",
            self.master_track.pan
        ));
        out.push_str("        </Mixer>\n");
        out.push_str("      </DeviceChain>\n");
        out.push_str("    </MasterTrack>\n");

        // Tracks
        out.push_str("    <Tracks>\n");
        for track in &self.tracks {
            let (tag, color_idx) = match track.track_type {
                TrackType::Audio => ("AudioTrack", 24),
                TrackType::DrumRack => ("MidiTrack", 48),
                TrackType::MidiSynth => ("MidiTrack", 12),
            };

            out.push_str(&format!("      <{} Id=\"{}\">\n", tag, track.id));
            out.push_str("        <Name>\n");
            out.push_str(&format!(
                "          <EffectiveName Value=\"{}\"/>\n",
                xml_escape(&track.name)
            ));
            out.push_str(&format!(
                "          <UserName Value=\"{}\"/>\n",
                xml_escape(&track.name)
            ));
            out.push_str("        </Name>\n");
            out.push_str(&format!("        <Color Value=\"{}\"/>\n", color_idx));
            out.push_str(&format!(
                "        <ColorHex Value=\"{}\"/>\n",
                xml_escape(&track.color_hex)
            ));
            out.push_str(&format!(
                "        <TrackType Value=\"{:?}\"/>\n",
                track.track_type
            ));

            out.push_str("        <DeviceChain>\n");
            out.push_str("          <Mixer>\n");
            out.push_str(&format!(
                "            <Volume><Manual Value=\"{:.4}\"/></Volume>\n",
                track.mixer.volume_db
            ));
            out.push_str(&format!(
                "            <Pan><Manual Value=\"{:.4}\"/></Pan>\n",
                track.mixer.pan
            ));
            out.push_str(&format!(
                "            <On><Manual Value=\"{}\"/></On>\n",
                !track.mixer.mute
            ));
            out.push_str(&format!(
                "            <SoloSink><Manual Value=\"{}\"/></SoloSink>\n",
                track.mixer.solo
            ));
            out.push_str(&format!(
                "            <Speaker><Manual Value=\"{}\"/></Speaker>\n",
                !track.mixer.mute
            ));
            out.push_str(&format!(
                "            <Arm><Manual Value=\"{}\"/></Arm>\n",
                track.mixer.arm
            ));
            out.push_str("            <Sends>\n");
            out.push_str(&format!(
                "              <TrackSendHolder Id=\"0\"><Send><Manual Value=\"{:.4}\"/></Send></TrackSendHolder>\n",
                track.mixer.send_a_db
            ));
            out.push_str(&format!(
                "              <TrackSendHolder Id=\"1\"><Send><Manual Value=\"{:.4}\"/></Send></TrackSendHolder>\n",
                track.mixer.send_b_db
            ));
            out.push_str("            </Sends>\n");
            out.push_str("          </Mixer>\n");

            out.push_str("          <MainSequencer>\n");
            out.push_str("            <ClipSlotList>\n");
            for (slot_idx, maybe_clip) in track.session_clips.iter().enumerate() {
                out.push_str(&format!("              <ClipSlot Id=\"{}\">\n", slot_idx));
                out.push_str("                <ClipSlot>\n");
                out.push_str("                  <Value>\n");
                if let Some(clip) = maybe_clip {
                    match &clip.pattern {
                        ClipPattern::Midi(midi_pat) => {
                            out.push_str("                    <MidiClip>\n");
                            out.push_str(&format!(
                                "                      <LomId Value=\"{}\"/>\n",
                                clip.id
                            ));
                            out.push_str(&format!(
                                "                      <Name Value=\"{}\"/>\n",
                                xml_escape(&clip.name)
                            ));
                            out.push_str(&format!(
                                "                      <ColorHex Value=\"{}\"/>\n",
                                xml_escape(&clip.color_hex)
                            ));
                            out.push_str("                      <Loop>\n");
                            out.push_str(&format!(
                                "                        <LoopOn Value=\"{}\"/>\n",
                                clip.loop_enabled
                            ));
                            out.push_str(&format!(
                                "                        <LoopStart Value=\"{:.4}\"/>\n",
                                clip.loop_start_beat
                            ));
                            out.push_str(&format!(
                                "                        <LoopEnd Value=\"{:.4}\"/>\n",
                                clip.loop_end_beat
                            ));
                            out.push_str(&format!(
                                "                        <StartRelative Value=\"{:.4}\"/>\n",
                                clip.loop_start_beat
                            ));
                            out.push_str(&format!(
                                "                        <OutMarker Value=\"{:.4}\"/>\n",
                                clip.loop_end_beat
                            ));
                            out.push_str(&format!(
                                "                        <HiddenLoopStart Value=\"{:.4}\"/>\n",
                                clip.loop_start_beat
                            ));
                            out.push_str(&format!(
                                "                        <HiddenLoopEnd Value=\"{:.4}\"/>\n",
                                clip.loop_end_beat
                            ));
                            out.push_str("                      </Loop>\n");
                            out.push_str(&format!(
                                "                      <CurrentStart Value=\"{:.4}\"/>\n",
                                clip.loop_start_beat
                            ));
                            out.push_str(&format!(
                                "                      <CurrentEnd Value=\"{:.4}\"/>\n",
                                clip.loop_end_beat
                            ));

                            out.push_str("                      <Notes>\n");
                            out.push_str("                        <KeyTracks>\n");
                            let mut notes_by_key: BTreeMap<u8, Vec<&MidiNote>> = BTreeMap::new();
                            for note in &midi_pat.notes {
                                notes_by_key.entry(note.pitch).or_default().push(note);
                            }
                            for (pitch, key_notes) in notes_by_key {
                                out.push_str("                          <KeyTrack>\n");
                                out.push_str(&format!(
                                    "                            <MidiKey Value=\"{}\"/>\n",
                                    pitch
                                ));
                                out.push_str("                            <Notes>\n");
                                for n in key_notes {
                                    out.push_str(&format!(
                                        "                              <MidiNoteEvent Time=\"{:.4}\" Duration=\"{:.4}\" Velocity=\"{:.1}\" OffVelocity=\"64\" Probability=\"{:.2}\" IsEnabled=\"true\"/>\n",
                                        n.start_beat, n.duration_beats, n.velocity as f32, n.probability
                                    ));
                                }
                                out.push_str("                            </Notes>\n");
                                out.push_str("                          </KeyTrack>\n");
                            }
                            out.push_str("                        </KeyTracks>\n");
                            out.push_str("                      </Notes>\n");
                            out.push_str("                    </MidiClip>\n");
                        }
                        ClipPattern::Drum(drum_pat) => {
                            out.push_str("                    <MidiClip>\n");
                            out.push_str(&format!(
                                "                      <LomId Value=\"{}\"/>\n",
                                clip.id
                            ));
                            out.push_str(&format!(
                                "                      <Name Value=\"{}\"/>\n",
                                xml_escape(&clip.name)
                            ));
                            out.push_str(&format!(
                                "                      <ColorHex Value=\"{}\"/>\n",
                                xml_escape(&clip.color_hex)
                            ));
                            out.push_str("                      <Loop>\n");
                            out.push_str(&format!(
                                "                        <LoopOn Value=\"{}\"/>\n",
                                clip.loop_enabled
                            ));
                            out.push_str(&format!(
                                "                        <LoopStart Value=\"{:.4}\"/>\n",
                                clip.loop_start_beat
                            ));
                            out.push_str(&format!(
                                "                        <LoopEnd Value=\"{:.4}\"/>\n",
                                clip.loop_end_beat
                            ));
                            out.push_str("                      </Loop>\n");

                            out.push_str("                      <Notes>\n");
                            out.push_str("                        <KeyTracks>\n");
                            let mut drum_notes_by_key: BTreeMap<u8, Vec<(f64, f64, u8)>> =
                                BTreeMap::new();
                            for trk in &drum_pat.tracks {
                                for (step_idx, step) in trk.steps.iter().enumerate() {
                                    if step.active {
                                        let time =
                                            step_idx as f64 * drum_pat.step_duration_beats;
                                        let dur = drum_pat.step_duration_beats;
                                        drum_notes_by_key.entry(step.pitch).or_default().push((
                                            time,
                                            dur,
                                            step.velocity,
                                        ));
                                    }
                                }
                            }
                            for (pitch, steps) in drum_notes_by_key {
                                out.push_str("                          <KeyTrack>\n");
                                out.push_str(&format!(
                                    "                            <MidiKey Value=\"{}\"/>\n",
                                    pitch
                                ));
                                out.push_str("                            <Notes>\n");
                                for (time, dur, vel) in steps {
                                    out.push_str(&format!(
                                        "                              <MidiNoteEvent Time=\"{:.4}\" Duration=\"{:.4}\" Velocity=\"{:.1}\" OffVelocity=\"64\" Probability=\"1.00\" IsEnabled=\"true\"/>\n",
                                        time, dur, vel as f32
                                    ));
                                }
                                out.push_str("                            </Notes>\n");
                                out.push_str("                          </KeyTrack>\n");
                            }
                            out.push_str("                        </KeyTracks>\n");
                            out.push_str("                      </Notes>\n");
                            out.push_str("                    </MidiClip>\n");
                        }
                        ClipPattern::Audio(audio_data) => {
                            out.push_str("                    <AudioClip>\n");
                            out.push_str(&format!(
                                "                      <LomId Value=\"{}\"/>\n",
                                clip.id
                            ));
                            out.push_str(&format!(
                                "                      <Name Value=\"{}\"/>\n",
                                xml_escape(&clip.name)
                            ));
                            out.push_str(&format!(
                                "                      <ColorHex Value=\"{}\"/>\n",
                                xml_escape(&clip.color_hex)
                            ));
                            out.push_str("                      <Loop>\n");
                            out.push_str(&format!(
                                "                        <LoopOn Value=\"{}\"/>\n",
                                clip.loop_enabled
                            ));
                            out.push_str(&format!(
                                "                        <LoopStart Value=\"{:.4}\"/>\n",
                                clip.loop_start_beat
                            ));
                            out.push_str(&format!(
                                "                        <LoopEnd Value=\"{:.4}\"/>\n",
                                clip.loop_end_beat
                            ));
                            out.push_str("                      </Loop>\n");
                            out.push_str("                      <SampleRef>\n");
                            out.push_str("                        <FileRef>\n");
                            out.push_str(&format!(
                                "                          <Path Value=\"{}\"/>\n",
                                xml_escape(&audio_data.file_path)
                            ));
                            out.push_str(&format!(
                                "                          <Name Value=\"{}\"/>\n",
                                xml_escape(&audio_data.file_path)
                            ));
                            out.push_str("                        </FileRef>\n");
                            out.push_str("                      </SampleRef>\n");
                            out.push_str("                    </AudioClip>\n");
                        }
                    }
                }
                out.push_str("                  </Value>\n");
                out.push_str("                </ClipSlot>\n");
                out.push_str("              </ClipSlot>\n");
            }
            out.push_str("            </ClipSlotList>\n");
            out.push_str("          </MainSequencer>\n");

            // Devices
            out.push_str("          <Devices>\n");
            match track.track_type {
                TrackType::DrumRack => {
                    out.push_str("            <DrumGroupDevice>\n");
                    out.push_str("              <UserName Value=\"909 Drum Rack\"/>\n");
                    out.push_str("              <IsExpanded Value=\"true\"/>\n");
                    out.push_str("            </DrumGroupDevice>\n");
                }
                TrackType::MidiSynth => {
                    out.push_str("            <Wavetable>\n");
                    out.push_str("              <UserName Value=\"Wavetable\"/>\n");
                    out.push_str("              <IsExpanded Value=\"true\"/>\n");
                    out.push_str("            </Wavetable>\n");
                }
                TrackType::Audio => {}
            }
            for fx in &track.insert_fx {
                match fx {
                    InsertEffectConfig::Saturator { drive_db, .. } => {
                        out.push_str(&format!(
                            "            <Roar Drive=\"{:.2}\"><UserName Value=\"Roar\"/></Roar>\n",
                            drive_db
                        ));
                    }
                    InsertEffectConfig::TempoDelay {
                        time_beats,
                        feedback,
                        ..
                    } => {
                        out.push_str(&format!(
                            "            <Delay Time=\"{:.2}\" Feedback=\"{:.2}\"><UserName Value=\"Delay\"/></Delay>\n",
                            time_beats, feedback
                        ));
                    }
                    InsertEffectConfig::Reverb { decay_ms, .. } => {
                        out.push_str(&format!(
                            "            <Reverb Decay=\"{:.1}\"><UserName Value=\"Reverb\"/></Reverb>\n",
                            decay_ms
                        ));
                    }
                }
            }
            out.push_str("          </Devices>\n");

            out.push_str("        </DeviceChain>\n");
            out.push_str(&format!("      </{}>\n", tag));
        }
        out.push_str("    </Tracks>\n");

        // Scenes
        out.push_str("    <Scenes>\n");
        for scene in &self.scenes {
            out.push_str(&format!("      <Scene Id=\"{}\">\n", scene.id));
            out.push_str(&format!(
                "        <Name Value=\"{}\"/>\n",
                xml_escape(&scene.name)
            ));
            out.push_str(&format!(
                "        <Tempo Value=\"{:.4}\"/>\n",
                self.transport.bpm
            ));
            out.push_str("      </Scene>\n");
        }
        out.push_str("    </Scenes>\n");

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
            Self::from_als_xml(&xml)
        } else {
            if bytes.len() as u64 > MAX_ALS_XML_BYTES {
                return Err(DAWProjectError::Validation(
                    "ALS decompressed payload exceeds 64 MiB safety limit".to_string(),
                ));
            }
            let xml = std::str::from_utf8(bytes).map_err(|e| {
                DAWProjectError::Validation(format!("Invalid UTF-8 in ALS bytes: {}", e))
            })?;
            Self::from_als_xml(xml)
        }
    }

    /// Parse project from uncompressed Ableton Live XML string.
    pub fn from_als_xml(xml: &str) -> Result<DAWProject, DAWProjectError> {
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

        // Transport & Tempo
        let mut transport = TransportConfig::default();
        if let Some(bpm_str) = liveset.find_descendant("Tempo").and_then(|t| {
            t.find_child("Manual")
                .and_then(|m| m.get_attr_or_text("Value"))
                .or_else(|| t.get_attr_or_text("Value"))
        }) {
            if let Ok(bpm) = bpm_str.parse::<f64>() {
                transport.bpm = bpm;
            }
        }

        if let Some(ts) = liveset.find_descendant("TimeSignature") {
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
        if let Some(mt) = liveset.find_child("MasterTrack") {
            if let Some(vol) = mt
                .find_descendant("Volume")
                .and_then(|v| {
                    v.find_child("Manual")
                        .and_then(|m| m.get_attr_or_text("Value"))
                        .or_else(|| v.get_attr_or_text("Value"))
                })
                .and_then(|v| v.parse::<f32>().ok())
            {
                master_track.volume_db = vol;
            }
            if let Some(pan) = mt
                .find_descendant("Pan")
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

        // Tracks
        let mut tracks = Vec::new();
        if let Some(tracks_node) = liveset.find_child("Tracks") {
            for (idx, child) in tracks_node.children.iter().enumerate() {
                if child.tag != "MidiTrack"
                    && child.tag != "AudioTrack"
                    && child.tag != "ReturnTrack"
                {
                    continue;
                }

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

                let track_type_str = child
                    .find_child("TrackType")
                    .and_then(|t| t.get_attr_or_text("Value"));
                let is_drum = child.find_descendant("DrumGroupDevice").is_some()
                    || track_name.to_lowercase().contains("drum")
                    || track_name.to_lowercase().contains("909")
                    || track_type_str.as_deref() == Some("DrumRack");

                let track_type = if child.tag == "AudioTrack"
                    || track_type_str.as_deref() == Some("Audio")
                {
                    TrackType::Audio
                } else if is_drum {
                    TrackType::DrumRack
                } else {
                    TrackType::MidiSynth
                };

                let color_hex = child
                    .find_child("ColorHex")
                    .and_then(|c| c.get_attr_or_text("Value"))
                    .unwrap_or_else(|| match track_type {
                        TrackType::MidiSynth => "#ffbe00".to_string(),
                        TrackType::DrumRack => "#00d2ff".to_string(),
                        TrackType::Audio => "#00e676".to_string(),
                    });

                let mut mixer = MixerState::default();
                if let Some(mixer_node) = child.find_descendant("Mixer") {
                    if let Some(vol) = mixer_node
                        .find_descendant("Volume")
                        .and_then(|v| {
                            v.find_child("Manual")
                                .and_then(|m| m.get_attr_or_text("Value"))
                                .or_else(|| v.get_attr_or_text("Value"))
                        })
                        .and_then(|v| v.parse::<f32>().ok())
                    {
                        mixer.volume_db = vol;
                    }
                    if let Some(pan) = mixer_node
                        .find_descendant("Pan")
                        .and_then(|p| {
                            p.find_child("Manual")
                                .and_then(|m| m.get_attr_or_text("Value"))
                                .or_else(|| p.get_attr_or_text("Value"))
                        })
                        .and_then(|v| v.parse::<f32>().ok())
                    {
                        mixer.pan = pan;
                    }
                    if let Some(on) = mixer_node
                        .find_descendant("On")
                        .and_then(|o| {
                            o.find_child("Manual")
                                .and_then(|m| m.get_attr_or_text("Value"))
                                .or_else(|| o.get_attr_or_text("Value"))
                        })
                        .and_then(|v| v.parse::<bool>().ok())
                    {
                        mixer.mute = !on;
                    }
                    if let Some(solo) = mixer_node
                        .find_descendant("SoloSink")
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
                        .find_descendant("Arm")
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
                        let holders = sends.find_children("TrackSendHolder");
                        if let Some(s0) = holders
                            .first()
                            .and_then(|h| h.find_descendant("Manual"))
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .and_then(|v| v.parse::<f32>().ok())
                        {
                            mixer.send_a_db = s0;
                        }
                        if let Some(s1) = holders
                            .get(1)
                            .and_then(|h| h.find_descendant("Manual"))
                            .and_then(|m| m.get_attr_or_text("Value"))
                            .and_then(|v| v.parse::<f32>().ok())
                        {
                            mixer.send_b_db = s1;
                        }
                    }
                }

                let instrument = match track_type {
                    TrackType::MidiSynth => InstrumentConfig::BuiltinSubtractiveSynth(
                        SynthPatchParameters::default(),
                    ),
                    TrackType::DrumRack => InstrumentConfig::ElectronicDrumRack(
                        crate::drums::DrumRackDefinition::default_909(),
                    ),
                    TrackType::Audio => InstrumentConfig::None,
                };

                // Clips
                let mut session_clips = Vec::new();
                if let Some(slot_list) = child.find_descendant("ClipSlotList") {
                    for (slot_idx, slot_node) in
                        slot_list.find_children("ClipSlot").iter().enumerate()
                    {
                        let clip_id = slot_node
                            .get_attr("Id")
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(slot_idx as u32);
                        if let Some(midi_clip_node) = slot_node.find_descendant("MidiClip") {
                            let name = midi_clip_node
                                .find_child("Name")
                                .and_then(|n| n.get_attr_or_text("Value"))
                                .unwrap_or_else(|| format!("Clip {}", slot_idx + 1));
                            let clip_color = midi_clip_node
                                .find_child("ColorHex")
                                .and_then(|c| c.get_attr_or_text("Value"))
                                .unwrap_or_else(|| color_hex.clone());

                            let loop_node = midi_clip_node.find_child("Loop");
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
                                .unwrap_or(4.0);

                            let mut notes = Vec::new();
                            let key_tracks = midi_clip_node.find_descendants("KeyTrack");
                            if !key_tracks.is_empty() {
                                for kt in key_tracks {
                                    let pitch = kt
                                        .find_descendant("MidiKey")
                                        .and_then(|k| k.get_attr_or_text("Value"))
                                        .and_then(|v| v.parse::<u8>().ok())
                                        .unwrap_or(60);
                                    for note_ev in kt.find_descendants("MidiNoteEvent") {
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
                                for note_ev in midi_clip_node.find_descendants("MidiNoteEvent") {
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

                            notes.sort_by(|a, b| {
                                a.start_beat
                                    .partial_cmp(&b.start_beat)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                                    .then_with(|| a.pitch.cmp(&b.pitch))
                            });
                            let pat_len = (loop_end - loop_start).max(4.0);
                            let pattern = MidiPattern {
                                length_beats: pat_len,
                                notes,
                            };
                            let mut clip =
                                SessionClip::new(clip_id, name, ClipPattern::Midi(pattern));
                            clip.loop_enabled = loop_on;
                            clip.loop_start_beat = loop_start;
                            clip.loop_end_beat = loop_end;
                            clip.color_hex = clip_color;
                            session_clips.push(Some(clip));
                        } else if let Some(audio_clip_node) =
                            slot_node.find_descendant("AudioClip")
                        {
                            let name = audio_clip_node
                                .find_child("Name")
                                .and_then(|n| n.get_attr_or_text("Value"))
                                .unwrap_or_else(|| format!("Audio {}", slot_idx + 1));
                            let path = audio_clip_node
                                .find_descendant("Path")
                                .and_then(|p| p.get_attr_or_text("Value"))
                                .unwrap_or_default();
                            let loop_node = audio_clip_node.find_child("Loop");
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
                                .unwrap_or(4.0);

                            let audio_data = oxide_timeline::AudioClipData::new(
                                path,
                                44100,
                                2,
                                (loop_end - loop_start).max(4.0),
                            );
                            let mut clip =
                                SessionClip::new(clip_id, name, ClipPattern::Audio(audio_data));
                            clip.loop_enabled = loop_on;
                            clip.loop_start_beat = loop_start;
                            clip.loop_end_beat = loop_end;
                            clip.color_hex = color_hex.clone();
                            session_clips.push(Some(clip));
                        } else {
                            session_clips.push(None);
                        }
                    }
                }

                if session_clips.iter().all(|c| c.is_none()) {
                    session_clips.clear();
                }

                // Insert FX
                let mut insert_fx = Vec::new();
                if let Some(devs) = child.find_descendant("Devices") {
                    for dev in &devs.children {
                        if dev.tag == "Roar" || dev.tag == "Saturator" {
                            let drive = dev
                                .get_attr("Drive")
                                .and_then(|v| v.parse::<f32>().ok())
                                .unwrap_or(4.0);
                            insert_fx.push(InsertEffectConfig::Saturator {
                                drive_db: drive,
                                curve: 0.5,
                                output_gain_db: 0.0,
                            });
                        } else if dev.tag == "Delay" {
                            let time = dev
                                .get_attr("Time")
                                .and_then(|v| v.parse::<f32>().ok())
                                .unwrap_or(0.5);
                            let fb = dev
                                .get_attr("Feedback")
                                .and_then(|v| v.parse::<f32>().ok())
                                .unwrap_or(0.4);
                            insert_fx.push(InsertEffectConfig::TempoDelay {
                                time_beats: time,
                                feedback: fb,
                                mix: 0.3,
                            });
                        } else if dev.tag == "Reverb" {
                            let decay = dev
                                .get_attr("Decay")
                                .and_then(|v| v.parse::<f32>().ok())
                                .unwrap_or(1500.0);
                            insert_fx.push(InsertEffectConfig::Reverb {
                                ir_path: None,
                                decay_ms: decay,
                                mix: 0.25,
                            });
                        }
                    }
                }

                tracks.push(TrackDefinition {
                    id: track_id,
                    name: track_name,
                    color_hex,
                    track_type,
                    mixer,
                    instrument,
                    insert_fx,
                    session_clips,
                    arrangement_clips: Vec::new(),
                    automation_lanes: Vec::new(),
                });
            }
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
                scenes.push(Scene::new(id, name));
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

    /// Load project from disk from a .als file.
    pub fn load_als(path: impl AsRef<Path>) -> Result<DAWProject, DAWProjectError> {
        let bytes = fs::read(path)?;
        Self::from_als_bytes(&bytes)
    }
}

/// Load an Ableton Live `.als` file from disk directly.
pub fn load_als(path: impl AsRef<Path>) -> Result<DAWProject, DAWProjectError> {
    DAWProject::load_als(path)
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
}
