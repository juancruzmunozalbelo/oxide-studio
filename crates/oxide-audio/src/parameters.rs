//! Lock-free shared atomic parameters and 1-pole smoothing.

use portable_atomic::AtomicF32;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub const MAX_PARAMS: usize = 64;

// Well-known parameter indices
pub const PARAM_MASTER_VOL: usize = 0;
pub const PARAM_BPM: usize = 1;
pub const PARAM_SYNTH_CUTOFF: usize = 2;
pub const PARAM_SYNTH_RESONANCE: usize = 3;
pub const PARAM_SYNTH_MACRO_X: usize = 4;
pub const PARAM_SYNTH_MACRO_Y: usize = 5;
pub const PARAM_FX_DRIVE: usize = 6;
pub const PARAM_FX_DELAY_WET: usize = 7;
pub const PARAM_FX_REVERB_WET: usize = 8;
pub const PARAM_TRACK_VOL_BASE: usize = 16; // 16..31
pub const PARAM_TRACK_PAN_BASE: usize = 32; // 32..47

#[derive(Clone)]
pub struct AtomicParameters {
    params: Arc<[AtomicF32; MAX_PARAMS]>,
}

impl Default for AtomicParameters {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicParameters {
    pub fn new() -> Self {
        let params: [AtomicF32; MAX_PARAMS] = std::array::from_fn(|i| {
            let default_val = match i {
                PARAM_MASTER_VOL => 1.0,
                PARAM_BPM => 120.0,
                PARAM_SYNTH_CUTOFF => 1000.0,
                PARAM_SYNTH_RESONANCE => 0.5,
                PARAM_SYNTH_MACRO_X => 0.5,
                PARAM_SYNTH_MACRO_Y => 0.5,
                PARAM_FX_DRIVE => 0.0,
                PARAM_FX_DELAY_WET => 0.25,
                PARAM_FX_REVERB_WET => 0.2,
                idx if (PARAM_TRACK_VOL_BASE..PARAM_TRACK_VOL_BASE + 16).contains(&idx) => 1.0,
                idx if (PARAM_TRACK_PAN_BASE..PARAM_TRACK_PAN_BASE + 16).contains(&idx) => 0.0,
                _ => 0.0,
            };
            AtomicF32::new(default_val)
        });

        Self {
            params: Arc::new(params),
        }
    }

    /// Read an atomic parameter value (lock-free, wait-free).
    #[inline(always)]
    pub fn get(&self, param_id: usize) -> f32 {
        if param_id < MAX_PARAMS {
            self.params[param_id].load(Ordering::Relaxed)
        } else {
            0.0
        }
    }

    /// Write an atomic parameter value from UI or sequencer.
    #[inline(always)]
    pub fn set(&self, param_id: usize, value: f32) {
        if param_id < MAX_PARAMS {
            self.params[param_id].store(value, Ordering::Relaxed);
        }
    }
}

/// One-pole smoothing state for an audio-rate parameter.
#[derive(Debug, Clone, Copy)]
pub struct ParameterSmoother {
    pub current: f32,
    pub target: f32,
    alpha: f32,
}

impl ParameterSmoother {
    pub fn new(initial: f32, slew_ms: f32, sample_rate: f32) -> Self {
        let alpha = if slew_ms <= 0.1 {
            1.0
        } else {
            1.0 - (-1.0 / ((slew_ms * 0.001) * sample_rate)).exp()
        };
        Self {
            current: initial,
            target: initial,
            alpha,
        }
    }

    #[inline(always)]
    pub fn update_target(&mut self, new_target: f32) {
        self.target = new_target;
    }

    #[inline(always)]
    pub fn next(&mut self) -> f32 {
        self.current += self.alpha * (self.target - self.current);
        self.current
    }
}
