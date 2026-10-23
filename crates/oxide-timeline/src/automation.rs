//! Automation lanes, continuous time-value breakpoint interpolation,
//! cubic Bezier curve editing, Draw Mode, and hardware parameter takeover.

use oxide_core::TakeoverMode;
use serde::{Deserialize, Serialize};

/// Curve interpolation profiles supported for automation breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AutomationCurveType {
    #[default]
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

    pub fn from_curve_tension(tension: f32) -> Self {
        if (tension - 0.0).abs() < 1e-4 {
            AutomationCurveType::Linear
        } else if tension > 0.0 {
            AutomationCurveType::Exponential
        } else {
            AutomationCurveType::Logarithmic
        }
    }
}

/// Normalized 2D control points for a cubic Bezier curve segment.
///
/// In Ableton Live 12, holding Alt while dragging an envelope segment allows
/// continuous sculpting of cubic Bezier curves with two control handles (P1 and P2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CubicBezierHandle {
    /// Normalized X coordinate of control point P1 (0.0 .. 1.0 relative to beat span).
    pub cx1: f32,
    /// Normalized Y coordinate of control point P1 (0.0 .. 1.0 relative to value delta).
    pub cy1: f32,
    /// Normalized X coordinate of control point P2 (0.0 .. 1.0 relative to beat span).
    pub cx2: f32,
    /// Normalized Y coordinate of control point P2 (0.0 .. 1.0 relative to value delta).
    pub cy2: f32,
}

impl Default for CubicBezierHandle {
    fn default() -> Self {
        Self::linear()
    }
}

impl CubicBezierHandle {
    pub const fn new(cx1: f32, cy1: f32, cx2: f32, cy2: f32) -> Self {
        Self { cx1, cy1, cx2, cy2 }
    }

    pub const fn linear() -> Self {
        Self {
            cx1: 1.0 / 3.0,
            cy1: 1.0 / 3.0,
            cx2: 2.0 / 3.0,
            cy2: 2.0 / 3.0,
        }
    }

    pub const fn ease_in() -> Self {
        Self {
            cx1: 0.42,
            cy1: 0.0,
            cx2: 1.0,
            cy2: 1.0,
        }
    }

    pub const fn ease_out() -> Self {
        Self {
            cx1: 0.0,
            cy1: 0.0,
            cx2: 0.58,
            cy2: 1.0,
        }
    }

    pub const fn ease_in_out() -> Self {
        Self {
            cx1: 0.42,
            cy1: 0.0,
            cx2: 0.58,
            cy2: 1.0,
        }
    }

    /// Convert Live 12 Alt-drag curve tension handle [-1.0, 1.0] to cubic Bezier control handles.
    pub fn from_tension(tension: f32) -> Self {
        let t = tension.clamp(-1.0, 1.0);
        if t.abs() < 1e-4 {
            Self::linear()
        } else if t > 0.0 {
            // Concave up (late acceleration)
            let cy1 = (1.0 / 3.0) * (1.0 - t);
            let cy2 = (2.0 / 3.0) * (1.0 - t);
            Self {
                cx1: 1.0 / 3.0,
                cy1,
                cx2: 2.0 / 3.0,
                cy2,
            }
        } else {
            // Concave down (early acceleration)
            let t_abs = t.abs();
            let cy1 = (1.0 / 3.0) + (2.0 / 3.0) * t_abs;
            let cy2 = (2.0 / 3.0) + (1.0 / 3.0) * t_abs;
            Self {
                cx1: 1.0 / 3.0,
                cy1,
                cx2: 2.0 / 3.0,
                cy2,
            }
        }
    }

    /// Sample x coordinate along the cubic Bezier at parameter u in [0, 1].
    #[inline]
    pub fn sample_x(&self, u: f32) -> f32 {
        let one_minus_u = 1.0 - u;
        3.0 * one_minus_u * one_minus_u * u * self.cx1
            + 3.0 * one_minus_u * u * u * self.cx2
            + u * u * u
    }

    /// Sample y coordinate along the cubic Bezier at parameter u in [0, 1].
    #[inline]
    pub fn sample_y(&self, u: f32) -> f32 {
        let one_minus_u = 1.0 - u;
        3.0 * one_minus_u * one_minus_u * u * self.cy1
            + 3.0 * one_minus_u * u * u * self.cy2
            + u * u * u
    }

    /// Solve for parameter u such that sample_x(u) == t using Newton-Raphson with bisection fallback.
    pub fn solve_u_for_x(&self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }

        let mut u = t;
        for _ in 0..8 {
            let current_x = self.sample_x(u) - t;
            if current_x.abs() < 1e-5 {
                return u;
            }
            let one_minus_u = 1.0 - u;
            let dx = 3.0 * one_minus_u * one_minus_u * self.cx1
                + 6.0 * one_minus_u * u * (self.cx2 - self.cx1)
                + 3.0 * u * u * (1.0 - self.cx2);
            if dx.abs() < 1e-6 {
                break;
            }
            u -= current_x / dx;
            if !(0.0..=1.0).contains(&u) {
                break;
            }
        }

        // Bisection fallback
        let mut low = 0.0f32;
        let mut high = 1.0f32;
        let mut u = t;
        for _ in 0..16 {
            let current_x = self.sample_x(u);
            if (current_x - t).abs() < 1e-5 {
                return u;
            }
            if current_x > t {
                high = u;
            } else {
                low = u;
            }
            u = 0.5 * (low + high);
        }
        u
    }

    /// Evaluate normalized y value for normalized time fraction t in [0, 1].
    pub fn evaluate_at(&self, t: f32) -> f32 {
        let u = self.solve_u_for_x(t.clamp(0.0, 1.0));
        self.sample_y(u).clamp(0.0, 1.0)
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
    /// Explicit curve profile (Linear, Exponential, Logarithmic, SCurve, Step).
    #[serde(default)]
    pub curve_type: AutomationCurveType,
    /// Optional explicit cubic Bezier handle for Alt-drag curve sculpting.
    #[serde(default)]
    pub bezier: Option<CubicBezierHandle>,
}

impl Default for AutomationPoint {
    fn default() -> Self {
        Self {
            beat: 0.0,
            value_norm: 0.0,
            curve: 0.0,
            curve_type: AutomationCurveType::Linear,
            bezier: None,
        }
    }
}

impl AutomationPoint {
    pub const fn new(beat: f64, value_norm: f32, curve: f32) -> Self {
        Self {
            beat,
            value_norm,
            curve,
            curve_type: AutomationCurveType::Linear,
            bezier: None,
        }
    }

    pub const fn linear(beat: f64, value_norm: f32) -> Self {
        Self {
            beat,
            value_norm,
            curve: 0.0,
            curve_type: AutomationCurveType::Linear,
            bezier: None,
        }
    }

    pub fn with_curve_type(beat: f64, value_norm: f32, curve_type: AutomationCurveType) -> Self {
        Self {
            beat,
            value_norm,
            curve: curve_type.to_curve_tension(),
            curve_type,
            bezier: None,
        }
    }

    pub fn with_bezier(beat: f64, value_norm: f32, bezier: CubicBezierHandle) -> Self {
        Self {
            beat,
            value_norm,
            curve: 0.0,
            curve_type: AutomationCurveType::Linear,
            bezier: Some(bezier),
        }
    }

    pub const fn step(beat: f64, value_norm: f32) -> Self {
        Self {
            beat,
            value_norm,
            curve: 1.0,
            curve_type: AutomationCurveType::Step,
            bezier: None,
        }
    }

    pub const fn scurve(beat: f64, value_norm: f32) -> Self {
        Self {
            beat,
            value_norm,
            curve: 0.25,
            curve_type: AutomationCurveType::SCurve,
            bezier: None,
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

    // 1. If explicit cubic Bezier handle is present
    if let Some(bezier) = p0.bezier {
        let t_curved = bezier.evaluate_at(t);
        return p0.value_norm + (p1.value_norm - p0.value_norm) * t_curved;
    }

    // 2. Profile curve evaluation
    let t_curved = match p0.curve_type {
        AutomationCurveType::Step => 0.0, // Discrete hold until p1.beat
        AutomationCurveType::SCurve => {
            // Cubic Hermite smoothstep S-curve: 3t^2 - 2t^3
            t * t * (3.0 - 2.0 * t)
        }
        AutomationCurveType::Exponential => {
            let tension = if p0.curve != 0.0 { p0.curve.abs() } else { 0.5 };
            t.powf(1.0 + 3.0 * tension)
        }
        AutomationCurveType::Logarithmic => {
            let tension = if p0.curve != 0.0 { p0.curve.abs() } else { 0.5 };
            1.0 - (1.0 - t).powf(1.0 + 3.0 * tension)
        }
        AutomationCurveType::Linear => {
            if p0.curve == 0.0 {
                t
            } else if p0.curve > 0.0 {
                t.powf(1.0 + 3.0 * p0.curve)
            } else {
                1.0 - (1.0 - t).powf(1.0 + 3.0 * p0.curve.abs())
            }
        }
    };

    p0.value_norm + (p1.value_norm - p0.value_norm) * t_curved
}

/// Ramer-Douglas-Peucker (RDP) trajectory simplification algorithm.
///
/// Reduces redundant breakpoints in freehand automation while preserving
/// envelope contours within the specified vertical tolerance.
pub fn simplify_points_rdp(points: &[(f64, f32)], tolerance: f32) -> Vec<(f64, f32)> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let first = points[0];
    let last = points[points.len() - 1];

    let mut max_dist = 0.0f32;
    let mut index = 0;

    let span_x = (last.0 - first.0).abs();

    for i in 1..(points.len() - 1) {
        let p = points[i];
        let dist = if span_x > 1e-6 {
            let t = ((p.0 - first.0) / span_x) as f32;
            let y_line = first.1 + (last.1 - first.1) * t;
            (p.1 - y_line).abs()
        } else {
            (p.1 - first.1).abs()
        };

        if dist > max_dist {
            max_dist = dist;
            index = i;
        }
    }

    if max_dist > tolerance {
        let mut left = simplify_points_rdp(&points[0..=index], tolerance);
        let right = simplify_points_rdp(&points[index..], tolerance);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![first, last]
    }
}

/// Draw Mode state configuration (Ableton Live 12 shortcut 'B').
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DrawModeState {
    /// Whether Draw Mode pencil is active.
    pub enabled: bool,
    /// Grid quantization step in beats (e.g., 0.25 for 1/16th, 0.5 for 1/8th, 1.0 for quarter note).
    /// If None, freehand mode is active.
    pub grid_step: Option<f64>,
    /// Tolerance for freehand curve thinning (e.g. 0.01 = 1% normalized value).
    pub simplify_tolerance: f32,
}

impl Default for DrawModeState {
    fn default() -> Self {
        Self {
            enabled: false,
            grid_step: Some(0.25),
            simplify_tolerance: 0.01,
        }
    }
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

    /// Number of breakpoints in this lane.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether this lane has no breakpoints.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
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

    /// Remove a breakpoint at a specific index.
    pub fn remove_point_at(&mut self, index: usize) -> Option<AutomationPoint> {
        if index < self.points.len() {
            Some(self.points.remove(index))
        } else {
            None
        }
    }

    /// Remove all breakpoints in the inclusive beat range `[start_beat, end_beat]`.
    pub fn remove_points_in_range(&mut self, start_beat: f64, end_beat: f64) {
        self.points.retain(|p| p.beat < start_beat - 1e-5 || p.beat > end_beat + 1e-5);
    }

    /// Query all breakpoints residing in `[start_beat, end_beat]`.
    pub fn points_in_range(&self, start_beat: f64, end_beat: f64) -> Vec<&AutomationPoint> {
        self.points
            .iter()
            .filter(|p| p.beat >= start_beat - 1e-5 && p.beat <= end_beat + 1e-5)
            .collect()
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

    /// Set curve tension for segment starting at `point_idx`.
    pub fn set_segment_curve(&mut self, point_idx: usize, tension: f32) {
        if let Some(pt) = self.points.get_mut(point_idx) {
            pt.curve = tension.clamp(-1.0, 1.0);
            pt.curve_type = AutomationCurveType::from_curve_tension(pt.curve);
            pt.bezier = None;
        }
    }

    /// Set explicit cubic Bezier handle for segment starting at `point_idx`.
    pub fn set_segment_bezier(&mut self, point_idx: usize, bezier: CubicBezierHandle) {
        if let Some(pt) = self.points.get_mut(point_idx) {
            pt.bezier = Some(bezier);
        }
    }

    /// Set curve type for segment starting at `point_idx`.
    pub fn set_segment_curve_type(&mut self, point_idx: usize, curve_type: AutomationCurveType) {
        if let Some(pt) = self.points.get_mut(point_idx) {
            pt.curve_type = curve_type;
            pt.curve = curve_type.to_curve_tension();
            pt.bezier = None;
        }
    }

    /// Apply Alt-drag vertical displacement delta to sculpt segment curve into Bezier/tension.
    pub fn apply_alt_drag_delta(&mut self, point_idx: usize, delta: f32) {
        if let Some(pt) = self.points.get_mut(point_idx) {
            pt.curve = (pt.curve + delta).clamp(-1.0, 1.0);
            pt.bezier = Some(CubicBezierHandle::from_tension(pt.curve));
        }
    }

    /// Draw stepped grid automation across `[start_beat, end_beat]` with quantization `grid_step`.
    pub fn draw_stepped(&mut self, start_beat: f64, end_beat: f64, value_norm: f32, grid_step: f64) {
        if grid_step <= 0.0 || end_beat <= start_beat {
            return;
        }
        let step_size = grid_step.max(0.0625);
        let aligned_start = (start_beat / step_size).floor() * step_size;
        let aligned_end = (end_beat / step_size).ceil() * step_size;

        let val_at_end = self.interpolate_at(aligned_end, value_norm);
        let val_before = if aligned_start > 1e-4 {
            Some(self.interpolate_at(aligned_start - 1e-5, value_norm))
        } else {
            None
        };

        // Remove points strictly within [aligned_start, aligned_end]
        self.points.retain(|p| p.beat < aligned_start - 1e-5 || p.beat > aligned_end + 1e-5);

        // Preceding anchor point to maintain prior value right before the step jump
        if let Some(vb) = val_before {
            self.add_point(AutomationPoint::linear(aligned_start - 1e-4, vb));
        }

        let mut current_beat = aligned_start;
        while current_beat < aligned_end - 1e-5 {
            self.add_point(AutomationPoint::step(current_beat, value_norm));
            current_beat += step_size;
        }

        // Boundary closure
        if !self.points.iter().any(|p| (p.beat - aligned_end).abs() < 1e-5) {
            self.add_point(AutomationPoint::step(aligned_end, val_at_end));
        }
    }

    /// Draw freehand automation points and simplify using Ramer-Douglas-Peucker algorithm.
    pub fn draw_freehand(&mut self, raw_points: &[(f64, f32)], tolerance: f32) {
        if raw_points.is_empty() {
            return;
        }
        if raw_points.len() == 1 {
            self.add_point(AutomationPoint::linear(raw_points[0].0, raw_points[0].1));
            return;
        }

        let start_beat = raw_points.first().unwrap().0;
        let end_beat = raw_points.last().unwrap().0;

        self.points.retain(|p| p.beat < start_beat - 1e-5 || p.beat > end_beat + 1e-5);

        let thinned = simplify_points_rdp(raw_points, tolerance);
        for (beat, val) in thinned {
            self.add_point(AutomationPoint::linear(beat, val));
        }
    }

    /// Thin all existing breakpoints in place to reduce redundant points.
    pub fn thin_points(&mut self, tolerance: f32) {
        if self.points.len() <= 2 {
            return;
        }
        let raw: Vec<(f64, f32)> = self.points.iter().map(|p| (p.beat, p.value_norm)).collect();
        let thinned = simplify_points_rdp(&raw, tolerance);

        let mut new_points = Vec::with_capacity(thinned.len());
        for (beat, val) in thinned {
            if let Some(existing) = self.points.iter().find(|p| (p.beat - beat).abs() < 1e-5) {
                new_points.push(*existing);
            } else {
                new_points.push(AutomationPoint::linear(beat, val));
            }
        }
        self.points = new_points;
    }
}

/// Parameter Takeover Tracker implementing Ableton Live 12 Chapter 29 & 30 takeover modes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationTakeoverTracker {
    pub mode: TakeoverMode,
    pub is_latched: bool,
    pub last_hw_val: Option<f32>,
    pub is_overridden: bool,
}

impl AutomationTakeoverTracker {
    pub fn new(mode: TakeoverMode) -> Self {
        Self {
            mode,
            is_latched: false,
            last_hw_val: None,
            is_overridden: false,
        }
    }

    /// Process incoming hardware control movement against software parameter value.
    /// Returns `Some(updated_val)` if the software parameter should update, or `None` if movement is ignored.
    pub fn process_hw_movement(&mut self, current_soft: f32, new_hw: f32) -> Option<f32> {
        let soft_clamped = current_soft.clamp(0.0, 1.0);
        let hw_clamped = new_hw.clamp(0.0, 1.0);

        match self.mode {
            TakeoverMode::Jump => {
                self.is_latched = true;
                self.is_overridden = true;
                self.last_hw_val = Some(hw_clamped);
                Some(hw_clamped)
            }
            TakeoverMode::Pickup => {
                if self.is_latched {
                    self.last_hw_val = Some(hw_clamped);
                    return Some(hw_clamped);
                }
                let prev_hw = self.last_hw_val.unwrap_or(hw_clamped);
                self.last_hw_val = Some(hw_clamped);

                if let Some(val) = self.mode.evaluate_takeover(soft_clamped, prev_hw, hw_clamped) {
                    self.is_latched = true;
                    self.is_overridden = true;
                    Some(val)
                } else {
                    None
                }
            }
            TakeoverMode::ValueScaling => {
                if self.is_latched {
                    self.last_hw_val = Some(hw_clamped);
                    return Some(hw_clamped);
                }
                let prev_hw = match self.last_hw_val {
                    Some(prev) => prev,
                    None => {
                        self.last_hw_val = Some(hw_clamped);
                        return Some(soft_clamped);
                    }
                };
                self.last_hw_val = Some(hw_clamped);

                if let Some(val) = self.mode.evaluate_takeover(soft_clamped, prev_hw, hw_clamped) {
                    self.is_overridden = true;
                    if (val - hw_clamped).abs() < 1e-4 || hw_clamped <= 1e-6 || hw_clamped >= 1.0 - 1e-6 {
                        self.is_latched = true;
                    }
                    Some(val)
                } else {
                    None
                }
            }
        }
    }

    /// Re-enable automation (Live 12 "Back to Arrangement" / orange playhead reset).
    /// Clears the manual override state and releases hardware latch.
    pub fn re_enable_automation(&mut self) {
        self.is_overridden = false;
        self.is_latched = false;
    }

    /// Process hardware movement and, if takeover is active, record a breakpoint into the automation lane.
    pub fn record_into_lane(
        &mut self,
        lane: &mut AutomationLane,
        beat: f64,
        current_soft: f32,
        new_hw: f32,
    ) -> Option<f32> {
        let updated = self.process_hw_movement(current_soft, new_hw)?;
        lane.add_point(AutomationPoint::linear(beat, updated));
        Some(updated)
    }
}

/// Multi-lane parameter automation container managing simultaneous parameter envelopes for a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiLaneAutomation {
    pub lanes: Vec<AutomationLane>,
    pub is_expanded: bool,
    pub draw_mode: DrawModeState,
}

impl Default for MultiLaneAutomation {
    fn default() -> Self {
        Self {
            lanes: Vec::new(),
            is_expanded: false,
            draw_mode: DrawModeState::default(),
        }
    }
}

impl MultiLaneAutomation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of active automation lanes.
    pub fn lanes_count(&self) -> usize {
        self.lanes.len()
    }

    /// Toggle Draw Mode pencil tool.
    pub fn toggle_draw_mode(&mut self) -> bool {
        self.draw_mode.enabled = !self.draw_mode.enabled;
        self.draw_mode.enabled
    }

    /// Retrieve an automation lane by parameter ID.
    pub fn get_lane(&self, parameter_id: u32) -> Option<&AutomationLane> {
        self.lanes.iter().find(|l| l.parameter_id == parameter_id)
    }

    /// Retrieve mutable reference to an automation lane by parameter ID.
    pub fn get_lane_mut(&mut self, parameter_id: u32) -> Option<&mut AutomationLane> {
        self.lanes.iter_mut().find(|l| l.parameter_id == parameter_id)
    }

    /// Remove an automation lane.
    pub fn remove_lane(&mut self, parameter_id: u32) -> Option<AutomationLane> {
        if let Some(pos) = self.lanes.iter().position(|l| l.parameter_id == parameter_id) {
            Some(self.lanes.remove(pos))
        } else {
            None
        }
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
        let p0 = AutomationPoint::new(0.0, 0.1, 0.5);
        let p1 = AutomationPoint::new(4.0, 0.9, 0.0);

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
        let p0_neg = AutomationPoint::new(0.0, 0.1, -0.5);
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

    #[test]
    fn test_step_curve_hold_and_discrete_jump() {
        let mut lane = AutomationLane::new(10, "track_mute");
        lane.add_point(AutomationPoint::step(0.0, 0.0));
        lane.add_point(AutomationPoint::step(4.0, 1.0));
        lane.add_point(AutomationPoint::step(8.0, 0.0));

        // In step mode, value stays at p0 until reaching p1
        assert_eq!(lane.interpolate_at(0.0, 0.0), 0.0);
        assert_eq!(lane.interpolate_at(1.0, 0.0), 0.0);
        assert_eq!(lane.interpolate_at(3.99, 0.0), 0.0);
        assert_eq!(lane.interpolate_at(4.0, 0.0), 1.0);
        assert_eq!(lane.interpolate_at(6.0, 0.0), 1.0);
        assert_eq!(lane.interpolate_at(7.99, 0.0), 1.0);
        assert_eq!(lane.interpolate_at(8.0, 0.0), 0.0);
    }

    #[test]
    fn test_scurve_cubic_hermite_interpolation() {
        let p0 = AutomationPoint::scurve(0.0, 0.0);
        let p1 = AutomationPoint::scurve(4.0, 1.0);

        assert_eq!(interpolate_automation(&p0, &p1, 0.0), 0.0);
        assert_eq!(interpolate_automation(&p0, &p1, 4.0), 1.0);

        // Midpoint of S-curve must be exactly 0.5 (inflection point)
        let mid = interpolate_automation(&p0, &p1, 2.0);
        assert!((mid - 0.5).abs() < 1e-4, "S-curve midpoint must be 0.5, got {}", mid);

        // Ease in: at 25% time, value must be below 0.25 (slower start)
        let early = interpolate_automation(&p0, &p1, 1.0);
        assert!(early < 0.25, "S-curve must ease in slowly at t=0.25, got {}", early);

        // Ease out: at 75% time, value must be above 0.75 (slower end)
        let late = interpolate_automation(&p0, &p1, 3.0);
        assert!(late > 0.75, "S-curve must ease out smoothly at t=0.75, got {}", late);
    }

    #[test]
    fn test_cubic_bezier_handles_and_solver() {
        let linear_handle = CubicBezierHandle::linear();
        assert!((linear_handle.evaluate_at(0.0) - 0.0).abs() < 1e-4);
        assert!((linear_handle.evaluate_at(0.5) - 0.5).abs() < 1e-4);
        assert!((linear_handle.evaluate_at(1.0) - 1.0).abs() < 1e-4);

        // Ease-In (acceleration)
        let ease_in = CubicBezierHandle::ease_in();
        let mid_in = ease_in.evaluate_at(0.5);
        assert!(mid_in < 0.5, "Ease-in must lag below linear midpoint, got {}", mid_in);

        // Ease-Out (deceleration)
        let ease_out = CubicBezierHandle::ease_out();
        let mid_out = ease_out.evaluate_at(0.5);
        assert!(mid_out > 0.5, "Ease-out must lead above linear midpoint, got {}", mid_out);

        // Direct point with Bezier handle
        let p0 = AutomationPoint::with_bezier(0.0, 0.2, ease_in);
        let p1 = AutomationPoint::linear(4.0, 0.8);
        let val_mid = interpolate_automation(&p0, &p1, 2.0);
        assert!(
            val_mid > 0.2 && val_mid < 0.5,
            "Bezier ease-in must produce concave curve between 0.2 and 0.8, got {}",
            val_mid
        );
    }

    #[test]
    fn test_alt_drag_curve_tension_editing() {
        let mut lane = AutomationLane::new(5, "pan");
        lane.add_point(AutomationPoint::linear(0.0, 0.0));
        lane.add_point(AutomationPoint::linear(4.0, 1.0));

        // Initial linear interpolation at beat 2.0 -> 0.5
        assert!((lane.interpolate_at(2.0, 0.0) - 0.5).abs() < 1e-4);

        // Alt-drag downward (concave up tension +0.6)
        lane.apply_alt_drag_delta(0, 0.6);
        let val_curved = lane.interpolate_at(2.0, 0.0);
        assert!(val_curved < 0.5, "Alt-drag positive delta must curve downward, got {}", val_curved);

        // Alt-drag upward (concave down tension -0.6)
        lane.apply_alt_drag_delta(0, -1.2);
        let val_neg_curved = lane.interpolate_at(2.0, 0.0);
        assert!(val_neg_curved > 0.5, "Alt-drag negative delta must curve upward, got {}", val_neg_curved);
    }

    #[test]
    fn test_draw_mode_stepped_grid_quantization() {
        let mut lane = AutomationLane::new(1, "volume");
        lane.add_point(AutomationPoint::linear(0.0, 0.2));
        lane.add_point(AutomationPoint::linear(8.0, 0.2));

        // Draw stepped automation from beat 1.0 to 4.0 with grid step 1.0 at value 0.8
        lane.draw_stepped(1.0, 4.0, 0.8, 1.0);

        assert_eq!(lane.interpolate_at(0.5, 0.0), 0.2);
        assert_eq!(lane.interpolate_at(1.0, 0.0), 0.8);
        assert_eq!(lane.interpolate_at(2.5, 0.0), 0.8);
        assert_eq!(lane.interpolate_at(3.9, 0.0), 0.8);
    }

    #[test]
    fn test_draw_mode_freehand_rdp_simplification() {
        let mut lane = AutomationLane::new(2, "filter_res");

        // Simulate 20 points along a straight line y = 0.5 + 0.1x with micro-noise (< 0.005)
        let mut raw = Vec::new();
        for i in 0..=20 {
            let beat = i as f64 * 0.2;
            let val = 0.2 + (beat as f32) * 0.1 + (i % 2) as f32 * 0.002;
            raw.push((beat, val));
        }

        // Draw freehand with 1% tolerance
        lane.draw_freehand(&raw, 0.01);

        // RDP should thin 21 points down to substantially fewer (e.g. <= 4)
        assert!(
            lane.len() <= 4,
            "RDP thinning should eliminate collinear points (reduced to {} points)",
            lane.len()
        );
        assert!((lane.interpolate_at(0.0, 0.0) - 0.2).abs() < 0.01);
        assert!((lane.interpolate_at(4.0, 0.0) - 0.6).abs() < 0.02);
    }

    #[test]
    fn test_automation_takeover_jump_mode() {
        let mut tracker = AutomationTakeoverTracker::new(TakeoverMode::Jump);
        assert!(!tracker.is_latched);

        // Jump immediately latches and overrides
        let updated = tracker.process_hw_movement(0.2, 0.75);
        assert_eq!(updated, Some(0.75));
        assert!(tracker.is_latched);
        assert!(tracker.is_overridden);
    }

    #[test]
    fn test_automation_takeover_pickup_mode() {
        let mut tracker = AutomationTakeoverTracker::new(TakeoverMode::Pickup);
        let current_soft = 0.65;

        // Hardware moves from 0.1 to 0.3 (does not reach 0.65 -> ignored)
        assert_eq!(tracker.process_hw_movement(current_soft, 0.1), None);
        assert_eq!(tracker.process_hw_movement(current_soft, 0.3), None);
        assert!(!tracker.is_latched);
        assert!(!tracker.is_overridden);

        // Hardware crosses 0.65 to 0.70 -> takeover acquired!
        let crossed = tracker.process_hw_movement(current_soft, 0.70);
        assert_eq!(crossed, Some(0.70));
        assert!(tracker.is_latched);
        assert!(tracker.is_overridden);

        // Subsequent hardware movements follow 1:1
        assert_eq!(tracker.process_hw_movement(0.70, 0.85), Some(0.85));
    }

    #[test]
    fn test_automation_takeover_value_scaling_mode() {
        let mut tracker = AutomationTakeoverTracker::new(TakeoverMode::ValueScaling);
        let current_soft = 0.5;

        // Hardware is initially at 0.0 (initial position registration)
        let initial = tracker.process_hw_movement(current_soft, 0.0).unwrap();
        assert_eq!(initial, 0.5);

        // Hardware turns upwards towards 0.1
        let val1 = tracker.process_hw_movement(current_soft, 0.1).unwrap();
        assert!(val1 > 0.5 && val1 < 0.6, "Value scaling must smoothly scale upwards (got {})", val1);
        assert!(tracker.is_overridden);

        // Hardware reaches 1.0 -> convergence latch
        let val_max = tracker.process_hw_movement(val1, 1.0).unwrap();
        assert_eq!(val_max, 1.0);
        assert!(tracker.is_latched);
    }

    #[test]
    fn test_automation_takeover_back_to_arrangement_re_enable() {
        let mut tracker = AutomationTakeoverTracker::new(TakeoverMode::Jump);
        tracker.process_hw_movement(0.2, 0.8);
        assert!(tracker.is_overridden);
        assert!(tracker.is_latched);

        // Live 12 Back to Arrangement button reset
        tracker.re_enable_automation();
        assert!(!tracker.is_overridden);
        assert!(!tracker.is_latched);
    }

    #[test]
    fn test_takeover_recording_into_automation_lane() {
        let mut lane = AutomationLane::new(1, "filter_cutoff");
        let mut tracker = AutomationTakeoverTracker::new(TakeoverMode::Pickup);

        // CC before crossing -> no points recorded
        assert_eq!(tracker.record_into_lane(&mut lane, 1.0, 0.6, 0.2), None);
        assert_eq!(lane.len(), 0);

        // CC crossing -> recorded into lane
        let rec = tracker.record_into_lane(&mut lane, 2.0, 0.6, 0.65);
        assert_eq!(rec, Some(0.65));
        assert_eq!(lane.len(), 1);
        assert_eq!(lane.points[0].beat, 2.0);
        assert_eq!(lane.points[0].value_norm, 0.65);
    }

    #[test]
    fn test_automation_lane_range_operations_and_thinning() {
        let mut lane = AutomationLane::new(3, "send_a");
        for i in 0..10 {
            lane.add_point(AutomationPoint::linear(i as f64, 0.1 * i as f32));
        }
        assert_eq!(lane.len(), 10);

        let query = lane.points_in_range(3.0, 6.0);
        assert_eq!(query.len(), 4);

        lane.remove_points_in_range(4.0, 7.0);
        assert_eq!(lane.len(), 6);
        assert!(!lane.points.iter().any(|p| p.beat >= 4.0 && p.beat <= 7.0));

        let removed = lane.remove_point_at(0);
        assert_eq!(removed.unwrap().beat, 0.0);
        assert_eq!(lane.len(), 5);
    }
}
