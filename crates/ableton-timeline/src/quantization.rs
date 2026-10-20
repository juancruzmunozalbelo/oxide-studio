//! Quantization modes and launch calculations for Session clips and scenes.

use serde::{Deserialize, Serialize};

/// Quantized launch interval modes for clips and scenes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizeMode {
    Immediate,
    Sixteenth,
    Quarter,
    OneBar,
    TwoBars,
}

impl QuantizeMode {
    /// Return the quantization interval in musical beats (4/4 time).
    /// Immediate mode returns None as it does not wait for a beat grid.
    pub const fn interval_beats(&self) -> Option<f64> {
        match self {
            QuantizeMode::Immediate => None,
            QuantizeMode::Sixteenth => Some(0.25),
            QuantizeMode::Quarter => Some(1.0),
            QuantizeMode::OneBar => Some(4.0),
            QuantizeMode::TwoBars => Some(8.0),
        }
    }
}

/// Default epsilon threshold (in beats) for near-boundary quantization scheduling.
/// If a launch command is received within epsilon of an upcoming target beat,
/// it is scheduled for the subsequent interval to avoid transient clipping.
pub const DEFAULT_EPSILON: f64 = 1e-4;

/// Calculate the exact target beat for a launch event based on current beat and quantization mode.
///
/// Mathematical formula:
/// `target_beat = ceil(current_beat / interval) * interval`
/// For `QuantizeMode::Immediate`, returns `current_beat`.
pub fn calculate_launch_beat(current_beat: f64, mode: QuantizeMode) -> f64 {
    match mode.interval_beats() {
        None => current_beat,
        Some(interval) => (current_beat / interval).ceil() * interval,
    }
}

/// Calculate the target launch beat with near-boundary epsilon scheduling.
///
/// If `(target_beat - current_beat) < epsilon`, the event is scheduled
/// for the following interval (`target_beat + interval`).
pub fn calculate_launch_beat_with_epsilon(current_beat: f64, mode: QuantizeMode, epsilon: f64) -> f64 {
    match mode.interval_beats() {
        None => current_beat,
        Some(interval) => {
            let target = (current_beat / interval).ceil() * interval;
            if (target - current_beat) < epsilon {
                target + interval
            } else {
                target
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Two bars: 8 beats
        assert_eq!(calculate_launch_beat(0.5, QuantizeMode::TwoBars), 8.0);
        assert_eq!(calculate_launch_beat(8.0, QuantizeMode::TwoBars), 8.0);
        assert_eq!(calculate_launch_beat(8.01, QuantizeMode::TwoBars), 16.0);
        // Immediate mode: always returns current beat
        assert_eq!(calculate_launch_beat(3.1415, QuantizeMode::Immediate), 3.1415);
    }

    #[test]
    fn test_near_boundary_epsilon_scheduling() {
        let eps = 0.05;
        // Exact boundary (diff = 0.0 < eps): pushes to next bar
        assert_eq!(calculate_launch_beat_with_epsilon(4.0, QuantizeMode::OneBar, eps), 8.0);
        // Near boundary (diff = 4.0 - 3.98 = 0.02 < eps): pushes to next bar
        assert_eq!(calculate_launch_beat_with_epsilon(3.98, QuantizeMode::OneBar, eps), 8.0);
        // Well before boundary (diff = 4.0 - 3.90 = 0.10 >= eps): launches at 4.0
        assert_eq!(calculate_launch_beat_with_epsilon(3.90, QuantizeMode::OneBar, eps), 4.0);
        // Immediate mode is unaffected by epsilon
        assert_eq!(calculate_launch_beat_with_epsilon(3.99, QuantizeMode::Immediate, eps), 3.99);
    }
}
