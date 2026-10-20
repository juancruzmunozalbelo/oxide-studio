//! Mixer channel strips, insert FX chains, and master track limiter configuration.

use serde::{Deserialize, Serialize};

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
        }
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
}

/// Master bus processing definition including brickwall limiter configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterTrackDefinition {
    pub volume_db: f32,
    pub pan: f32,
    pub limiter_threshold_db: f32,
    pub limiter_ceiling_db: f32,
    pub limiter_release_ms: f32,
}

impl Default for MasterTrackDefinition {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            limiter_threshold_db: 0.0,
            limiter_ceiling_db: -0.3,
            limiter_release_ms: 100.0,
        }
    }
}
