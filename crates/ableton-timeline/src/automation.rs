//! Automation lanes and continuous time-value breakpoint interpolation.

use serde::{Deserialize, Serialize};

/// Curve interpolation profiles supported for automation breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationCurveType {
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    Step,
}

impl AutomationCurveType {
    pub fn to_curve_tension(&self) -> f32 {
        match self {
            AutomationCurveType::Linear => 0.0,
            AutomationCurveType::Exponential => 0.5,
            AutomationCurveType::Logarithmic => -0.5,
            AutomationCurveType::SCurve => 0.25,
            AutomationCurveType::Step => 1.0,
        }
    }
}

/// Breakpoint on an automation lane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    /// Beat position along the timeline.
    pub beat: f64,
    /// Normalized parameter value [0.0, 1.0].
    pub value_norm: f32,
    /// Curve tension handle in range [-1.0, 1.0].
    /// - 0.0: linear interpolation
    /// - positive: concave-up / late acceleration
    /// - negative: concave-down / early acceleration
    pub curve: f32,
}

impl AutomationPoint {
    pub const fn new(beat: f64, value_norm: f32, curve: f32) -> Self {
        Self {
            beat,
            value_norm,
            curve,
        }
    }

    pub const fn linear(beat: f64, value_norm: f32) -> Self {
        Self {
            beat,
            value_norm,
            curve: 0.0,
        }
    }

    pub fn with_curve_type(beat: f64, value_norm: f32, curve_type: AutomationCurveType) -> Self {
        Self {
            beat,
            value_norm,
            curve: curve_type.to_curve_tension(),
        }
    }
}

/// Interpolate parameter value between two automation points at a target beat.
///
/// Ensures strict $C^0$ continuity at point boundaries and clamps outside the segment range.
pub fn interpolate_automation(p0: &AutomationPoint, p1: &AutomationPoint, beat: f64) -> f32 {
    if beat <= p0.beat {
        return p0.value_norm;
    }
    if beat >= p1.beat {
        return p1.value_norm;
    }

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

/// Automation lane managing a sequence of ordered time-value breakpoints for a specific parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationLane {
    pub parameter_id: u32,
    pub parameter_name: String,
    pub points: Vec<AutomationPoint>,
}

impl AutomationLane {
    pub fn new(parameter_id: u32, parameter_name: impl Into<String>) -> Self {
        Self {
            parameter_id,
            parameter_name: parameter_name.into(),
            points: Vec::new(),
        }
    }

    /// Insert or update an automation point, maintaining chronological order.
    pub fn add_point(&mut self, point: AutomationPoint) {
        let eps = 1e-5;
        // Check if an existing point matches beat within epsilon
        if let Some(existing) = self.points.iter_mut().find(|p| (p.beat - point.beat).abs() < eps) {
            *existing = point;
            return;
        }

        // Insert maintaining sort order
        let idx = self.points.partition_point(|p| p.beat < point.beat);
        self.points.insert(idx, point);
    }

    /// Clear all automation breakpoints.
    pub fn clear(&mut self) {
        self.points.clear();
    }

    /// Evaluate the automation curve at `beat`.
    /// Returns `default_value` if lane has no points.
    pub fn interpolate_at(&self, beat: f64, default_value: f32) -> f32 {
        match self.points.len() {
            0 => default_value,
            1 => self.points[0].value_norm,
            _ => {
                if beat <= self.points[0].beat {
                    return self.points[0].value_norm;
                }
                let last = self.points.len() - 1;
                if beat >= self.points[last].beat {
                    return self.points[last].value_norm;
                }

                // Binary search for the surrounding segment
                let next_idx = self.points.partition_point(|p| p.beat <= beat);
                let prev_idx = next_idx.saturating_sub(1);
                interpolate_automation(&self.points[prev_idx], &self.points[next_idx], beat)
            }
        }
    }
}

/// Multi-lane parameter automation container managing simultaneous parameter envelopes for a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiLaneAutomation {
    pub lanes: Vec<AutomationLane>,
    pub is_expanded: bool,
}

impl Default for MultiLaneAutomation {
    fn default() -> Self {
        Self {
            lanes: Vec::new(),
            is_expanded: false,
        }
    }
}

impl MultiLaneAutomation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieve or create an automation lane for a parameter.
    pub fn get_or_create_lane(&mut self, parameter_id: u32, parameter_name: impl Into<String>) -> &mut AutomationLane {
        if let Some(pos) = self.lanes.iter().position(|l| l.parameter_id == parameter_id) {
            &mut self.lanes[pos]
        } else {
            self.lanes.push(AutomationLane::new(parameter_id, parameter_name));
            self.lanes.last_mut().unwrap()
        }
    }

    /// Record an automation breakpoint value at `beat`.
    pub fn record_value(&mut self, parameter_id: u32, parameter_name: &str, beat: f64, value_norm: f32) {
        let lane = self.get_or_create_lane(parameter_id, parameter_name);
        lane.add_point(AutomationPoint::linear(beat, value_norm));
    }

    /// Evaluate a parameter value at `beat`.
    pub fn evaluate_param(&self, parameter_id: u32, beat: f64, default_value: f32) -> f32 {
        if let Some(lane) = self.lanes.iter().find(|l| l.parameter_id == parameter_id) {
            lane.interpolate_at(beat, default_value)
        } else {
            default_value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Negative curve handle bends above linear midpoint
        let p0_neg = AutomationPoint { beat: 0.0, value_norm: 0.1, curve: -0.5 };
        let mid_val_neg = interpolate_automation(&p0_neg, &p1, 2.0);
        assert!(
            mid_val_neg > 0.5 && mid_val_neg < 0.9,
            "Negative curve handle must bend above linear midpoint (got {:.4})",
            mid_val_neg
        );
    }

    #[test]
    fn test_automation_lane_evaluation() {
        let mut lane = AutomationLane::new(1, "filter_cutoff");
        assert_eq!(lane.interpolate_at(2.0, 0.5), 0.5);

        lane.add_point(AutomationPoint::linear(0.0, 0.2));
        assert_eq!(lane.interpolate_at(5.0, 0.5), 0.2);

        lane.add_point(AutomationPoint::linear(4.0, 0.8));
        lane.add_point(AutomationPoint::linear(8.0, 0.4));

        assert_eq!(lane.interpolate_at(-1.0, 0.0), 0.2);
        assert_eq!(lane.interpolate_at(0.0, 0.0), 0.2);
        assert_eq!(lane.interpolate_at(2.0, 0.0), 0.5);
        assert_eq!(lane.interpolate_at(4.0, 0.0), 0.8);
        assert_eq!(lane.interpolate_at(6.0, 0.0), 0.6);
        assert_eq!(lane.interpolate_at(8.0, 0.0), 0.4);
        assert_eq!(lane.interpolate_at(10.0, 0.0), 0.4);
    }

    #[test]
    fn test_multi_lane_automation() {
        let mut multi = MultiLaneAutomation::new();
        multi.record_value(1, "cutoff", 0.0, 0.2);
        multi.record_value(1, "cutoff", 4.0, 0.8);

        multi.record_value(2, "resonance", 0.0, 0.5);
        multi.record_value(2, "resonance", 4.0, 0.1);

        assert_eq!(multi.lanes.len(), 2);
        assert_eq!(multi.evaluate_param(1, 2.0, 0.0), 0.5);
        assert_eq!(multi.evaluate_param(2, 2.0, 0.0), 0.3);
        assert_eq!(multi.evaluate_param(3, 2.0, 0.99), 0.99); // default
    }
}
