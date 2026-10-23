//! 16-pad Electronic Drum Rack state and pad configuration.

use serde::{Deserialize, Serialize};

/// State of an individual pad within a 16-pad Drum Rack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrumPadDefinition {
    pub pad_index: u8,
    pub name: String,
    pub sample_path: String,
    pub root_pitch: u8,
    pub choke_group: Option<u8>,
    pub volume_db: f32,
    pub pan: f32,
    pub pitch_offset_semitones: f32,
    pub decay_ms: f32,
}

impl DrumPadDefinition {
    pub fn new(pad_index: u8, name: impl Into<String>, sample_path: impl Into<String>, root_pitch: u8) -> Self {
        Self {
            pad_index,
            name: name.into(),
            sample_path: sample_path.into(),
            root_pitch,
            choke_group: None,
            volume_db: 0.0,
            pan: 0.0,
            pitch_offset_semitones: 0.0,
            decay_ms: 500.0,
        }
    }
}

/// 16-pad Drum Rack container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrumRackDefinition {
    pub pads: Vec<DrumPadDefinition>,
}

impl Default for DrumRackDefinition {
    fn default() -> Self {
        Self::default_909()
    }
}

impl DrumRackDefinition {
    /// Create standard Roland TR-909 default kit configuration with 16 pads.
    pub fn default_909() -> Self {
        let pad_configs = [
            (0, "Bass Drum", "assets/kick.wav", 36, None),
            (1, "Rim Shot", "assets/snare.wav", 37, None),
            (2, "Snare Drum", "assets/snare.wav", 38, None),
            (3, "Hand Clap", "assets/snare.wav", 39, None),
            (4, "Clave", "assets/snare.wav", 40, None),
            (5, "Low Tom", "assets/kick.wav", 41, None),
            (6, "Closed Hi-Hat", "assets/hihat.wav", 42, Some(1)),
            (7, "Mid Tom", "assets/kick.wav", 43, None),
            (8, "Hi Tom", "assets/kick.wav", 44, None),
            (9, "Crash Cymbal", "assets/hihat.wav", 45, None),
            (10, "Open Hi-Hat", "assets/hihat.wav", 46, Some(1)),
            (11, "Ride Cymbal", "assets/hihat.wav", 47, None),
            (12, "Splash Cymbal", "assets/hihat.wav", 48, None),
            (13, "Tambourine", "assets/hihat.wav", 49, None),
            (14, "Drop FX", "assets/kick.wav", 50, None),
            (15, "Cowbell", "assets/hihat.wav", 51, None),
        ];

        let mut pads = Vec::with_capacity(16);
        for (idx, name, sample, pitch, choke) in pad_configs {
            let mut pad = DrumPadDefinition::new(idx, name, sample, pitch);
            pad.choke_group = choke;
            pads.push(pad);
        }

        Self { pads }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drum_rack_default_909() {
        let rack = DrumRackDefinition::default_909();
        assert_eq!(rack.pads.len(), 16);
        assert_eq!(rack.pads[0].name, "Bass Drum");
        assert_eq!(rack.pads[0].root_pitch, 36);
        // Closed HH (pad 6) and Open HH (pad 10) share choke group 1
        assert_eq!(rack.pads[6].choke_group, Some(1));
        assert_eq!(rack.pads[10].choke_group, Some(1));
    }
}
