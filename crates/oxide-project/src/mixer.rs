//! Mixer channel strips, insert FX chains, and master track limiter configuration.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

fn default_one() -> f32 {
    1.0
}

fn default_glue_threshold() -> f32 {
    0.0
}

fn default_glue_ratio() -> f32 {
    4.0
}

fn default_glue_attack() -> f32 {
    1.0
}

fn default_glue_release() -> f32 {
    0.2
}

fn default_roar_drive() -> f32 {
    6.0
}

fn default_filter_cutoff() -> f32 {
    1000.0
}

fn default_filter_res() -> f32 {
    0.2
}

fn default_comp_threshold() -> f32 {
    -12.0
}

fn default_comp_attack() -> f32 {
    5.0
}

fn default_comp_release() -> f32 {
    50.0
}

/// Channel strip mixer state for an individual track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerState {
    pub volume_db: f32,
    pub pan: f32, // -1.0 (hard left) .. +1.0 (hard right)
    pub mute: bool,
    pub solo: bool,
    pub arm: bool,
    pub send_a_db: f32,
    pub send_b_db: f32,
    #[serde(default)]
    pub sends_db: Vec<f32>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            arm: false,
            send_a_db: -60.0,
            send_b_db: -60.0,
            sends_db: Vec::new(),
        }
    }
}

impl MixerState {
    /// Get send level in dB for return index `idx`, keeping `send_a_db` (0) and `send_b_db` (1)
    /// synchronized with `sends_db`.
    pub fn get_send_db(&self, idx: usize) -> f32 {
        match idx {
            0 => {
                if !self.sends_db.is_empty() && (self.send_a_db - (-60.0)).abs() < 1e-4 {
                    self.sends_db[0]
                } else {
                    self.send_a_db
                }
            }
            1 => {
                if self.sends_db.len() > 1 && (self.send_b_db - (-60.0)).abs() < 1e-4 {
                    self.sends_db[1]
                } else {
                    self.send_b_db
                }
            }
            _ => self.sends_db.get(idx).copied().unwrap_or(-70.0),
        }
    }

    /// Set send level in dB for return index `idx`, keeping `send_a_db` (0), `send_b_db` (1),
    /// and `sends_db` synchronized.
    pub fn set_send_db(&mut self, idx: usize, db: f32) {
        let clamped = db.clamp(-70.0, 6.0);
        if idx == 0 {
            self.send_a_db = clamped;
        } else if idx == 1 {
            self.send_b_db = clamped;
        }
        if self.sends_db.len() <= idx {
            let old_len = self.sends_db.len();
            self.sends_db.resize(idx + 1, -70.0);
            if old_len == 0 && idx > 0 {
                self.sends_db[0] = self.send_a_db;
            }
            if old_len <= 1 && idx > 1 {
                self.sends_db[1] = self.send_b_db;
            }
        }
        self.sends_db[idx] = clamped;
    }

    /// Return effective send levels for `0..max(2, sends_db.len())`.
    pub fn effective_sends_db(&self) -> Vec<f32> {
        let count = self.sends_db.len().max(2);
        (0..count).map(|i| self.get_send_db(i)).collect()
    }
}

/// Insert effect configurations supported by the track FX chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InsertEffectConfig {
    Saturator {
        drive_db: f32,
        curve: f32,
        output_gain_db: f32,
    },
    TempoDelay {
        time_beats: f32,
        feedback: f32,
        mix: f32,
    },
    Reverb {
        ir_path: Option<String>,
        decay_ms: f32,
        mix: f32,
    },
    EqEight {
        #[serde(default)]
        params: HashMap<String, f32>,
    },
    GlueCompressor {
        #[serde(default = "default_glue_threshold")]
        threshold_db: f32,
        #[serde(default = "default_glue_ratio")]
        ratio: f32,
        #[serde(default = "default_glue_attack")]
        attack_ms: f32,
        #[serde(default = "default_glue_release")]
        release_s: f32,
        #[serde(default)]
        makeup_gain_db: f32,
        #[serde(default = "default_one")]
        dry_wet: f32,
    },
    Roar {
        #[serde(default = "default_roar_drive")]
        drive_db: f32,
        #[serde(default)]
        tone: f32,
        #[serde(default = "default_one")]
        dry_wet: f32,
        #[serde(default)]
        output_gain_db: f32,
    },
    AutoFilter {
        #[serde(default = "default_filter_cutoff")]
        cutoff_hz: f32,
        #[serde(default = "default_filter_res")]
        resonance: f32,
        #[serde(default = "default_one")]
        dry_wet: f32,
    },
    Compressor {
        #[serde(default = "default_comp_threshold")]
        threshold_db: f32,
        #[serde(default = "default_glue_ratio")]
        ratio: f32,
        #[serde(default = "default_comp_attack")]
        attack_ms: f32,
        #[serde(default = "default_comp_release")]
        release_ms: f32,
        #[serde(default)]
        makeup_gain_db: f32,
        #[serde(default = "default_one")]
        dry_wet: f32,
    },
    MultibandDynamics {
        #[serde(default = "default_one")]
        amount: f32,
        #[serde(default = "default_one")]
        time_macro: f32,
        #[serde(default)]
        input_gain_db: f32,
        #[serde(default)]
        output_gain_db: f32,
    },
    GenericDevice {
        device_type: String,
        #[serde(default)]
        params: HashMap<String, f32>,
    },
}

/// Master bus processing definition including master FX and brickwall limiter configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterTrackDefinition {
    pub volume_db: f32,
    pub pan: f32,
    pub limiter_threshold_db: f32,
    pub limiter_ceiling_db: f32,
    pub limiter_release_ms: f32,
    #[serde(default)]
    pub saturator_drive_db: f32,
    #[serde(default)]
    pub saturator_curve: f32,
    #[serde(default)]
    pub saturator_output_db: f32,
    #[serde(default)]
    pub saturator_enabled: bool,
    #[serde(default)]
    pub delay_time_beats: f32,
    #[serde(default)]
    pub delay_feedback: f32,
    #[serde(default)]
    pub delay_mix: f32,
    #[serde(default)]
    pub reverb_decay_ms: f32,
    #[serde(default)]
    pub reverb_mix: f32,
}

impl Default for MasterTrackDefinition {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            limiter_threshold_db: 0.0,
            limiter_ceiling_db: -0.3,
            limiter_release_ms: 100.0,
            saturator_drive_db: 0.0,
            saturator_curve: 0.0,
            saturator_output_db: 0.0,
            saturator_enabled: false,
            delay_time_beats: 0.0,
            delay_feedback: 0.0,
            delay_mix: 0.0,
            reverb_decay_ms: 0.0,
            reverb_mix: 0.0,
        }
    }
}
