//! Global Groove Pool & .agr Groove Templates for Ableton Live 12.
//!
//! Provides non-destructive rhythmic swing, micro-timing displacement, and velocity dynamics
//! applied to both MIDI clip notes and audio clip warp markers.
//!
//! Features:
//! - Selectable classic groove templates:
//!   - `MPC 16th Swing (54% .. 75%)`
//!   - `Roland TR-909 Shuffle`
//!   - `E-mu SP-1200 Funk`
//!   - `Latin / Triplet`
//! - Parameters: `Base` (1/8, 1/16, 1/32), `Quantize %`, `Timing %`, `Random %`, `Velocity %`, and `Global Amount %`.
//! - Non-destructive evaluation during playback and permanent "Commit Groove" bake.

use serde::{Deserialize, Serialize};
use crate::pattern::{MidiNote, MidiPattern};
use crate::warping::{OWarper, WarpMarker};

/// Grid subdivision base for groove template evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrooveBase {
    Eighth,
    EighthTriplet,
    Sixteenth,
    SixteenthTriplet,
    ThirtySecond,
}

impl GrooveBase {
    /// Nominal duration of one grid step in quarter-note beats.
    pub fn step_duration_beats(&self) -> f64 {
        match self {
            GrooveBase::Eighth => 0.5,
            GrooveBase::EighthTriplet => 1.0 / 3.0,
            GrooveBase::Sixteenth => 0.25,
            GrooveBase::SixteenthTriplet => 1.0 / 6.0,
            GrooveBase::ThirtySecond => 0.125,
        }
    }
}

/// Timing and velocity offset for a specific step within a groove cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrooveStepOffset {
    pub timing_offset_beats: f32,
    pub velocity_offset: f32, // -127.0 .. +127.0
}

impl GrooveStepOffset {
    pub const fn new(timing_offset_beats: f32, velocity_offset: f32) -> Self {
        Self {
            timing_offset_beats,
            velocity_offset,
        }
    }
}

/// An individual groove template loaded in the Groove Pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrooveTemplate {
    pub id: u32,
    pub name: String,
    pub base: GrooveBase,
    pub quantize_amount: f32, // 0.0 .. 1.0 (Quantize %)
    pub timing_amount: f32,   // 0.0 .. 2.0 (Timing %)
    pub random_amount: f32,   // 0.0 .. 1.0 (Random %)
    pub velocity_amount: f32, // 0.0 .. 2.0 (Velocity %)
    pub step_offsets: Vec<GrooveStepOffset>,
}

impl GrooveTemplate {
    pub fn new(id: u32, name: impl Into<String>, base: GrooveBase) -> Self {
        Self {
            id,
            name: name.into(),
            base,
            quantize_amount: 0.0,
            timing_amount: 1.0,
            random_amount: 0.0,
            velocity_amount: 1.0,
            step_offsets: Vec::new(),
        }
    }

    /// Retrieve interpolated timing and velocity offsets at a given beat position.
    pub fn evaluate_offsets(&self, beat: f64) -> (f32, f32) {
        if self.step_offsets.is_empty() {
            return (0.0, 0.0);
        }

        let g = self.base.step_duration_beats();
        let num_steps = self.step_offsets.len();
        let cycle_len = num_steps as f64 * g;
        let wrapped_beat = ((beat % cycle_len) + cycle_len) % cycle_len;

        let step_f = wrapped_beat / g;
        let step_idx = step_f.floor() as usize % num_steps;
        let next_idx = (step_idx + 1) % num_steps;
        let phase = (step_f - step_idx as f64) as f32;

        let s0 = &self.step_offsets[step_idx];
        let s1 = &self.step_offsets[next_idx];

        let timing = s0.timing_offset_beats * (1.0 - phase) + s1.timing_offset_beats * phase;
        let velocity = s0.velocity_offset * (1.0 - phase) + s1.velocity_offset * phase;

        (timing, velocity)
    }
}

/// Global Groove Pool managing all loaded groove templates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroovePool {
    pub global_amount: f32, // 0.0 .. 1.30 (Global Amount %, default 1.0)
    pub templates: Vec<GrooveTemplate>,
}

impl Default for GroovePool {
    fn default() -> Self {
        let mut pool = Self {
            global_amount: 1.0,
            templates: Vec::new(),
        };
        pool.load_factory_templates();
        pool
    }
}

impl GroovePool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a template to the pool.
    pub fn add_template(&mut self, template: GrooveTemplate) -> u32 {
        let id = template.id;
        self.templates.push(template);
        id
    }

    /// Find a template by ID.
    pub fn get_template(&self, id: u32) -> Option<&GrooveTemplate> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// Find a template by name (case-insensitive substring).
    pub fn find_template(&self, query: &str) -> Option<&GrooveTemplate> {
        let q = query.to_lowercase();
        self.templates.iter().find(|t| t.name.to_lowercase().contains(&q))
    }

    /// Populate standard Live 12 factory groove templates.
    pub fn load_factory_templates(&mut self) {
        // 1. MPC 16th Swing templates (54% to 75%)
        let mpc_swings = [
            (1, "MPC 16th 54%", 0.54f32),
            (2, "MPC 16th 57%", 0.57f32),
            (3, "MPC 16th 60%", 0.60f32),
            (4, "MPC 16th 63%", 0.63f32),
            (5, "MPC 16th 66%", 0.667f32),
            (6, "MPC 16th 70%", 0.70f32),
            (7, "MPC 16th 75%", 0.75f32),
        ];

        let g_16th = GrooveBase::Sixteenth.step_duration_beats() as f32; // 0.25

        for (id, name, swing) in mpc_swings {
            let mut t = GrooveTemplate::new(id, name, GrooveBase::Sixteenth);
            // 4-step (1-beat) or 8-step cycle
            let mut steps = Vec::new();
            for k in 0..8 {
                let timing_offset = if k % 2 == 1 {
                    2.0 * (swing - 0.50) * g_16th
                } else {
                    0.0
                };
                let vel_offset = if k % 2 == 1 { 4.0 } else { 0.0 };
                steps.push(GrooveStepOffset::new(timing_offset, vel_offset));
            }
            t.step_offsets = steps;
            self.templates.push(t);
        }

        // 2. Roland TR-909 Shuffle
        {
            let mut t = GrooveTemplate::new(8, "TR-909 Shuffle", GrooveBase::Sixteenth);
            let steps = vec![
                GrooveStepOffset::new(0.000, 0.0),
                GrooveStepOffset::new(0.028, 14.0), // ~14ms push at 120bpm + punchy offbeat
                GrooveStepOffset::new(0.000, -4.0),
                GrooveStepOffset::new(0.022, 10.0),
            ];
            t.step_offsets = steps;
            self.templates.push(t);
        }

        // 3. E-mu SP-1200 Funk
        {
            let mut t = GrooveTemplate::new(9, "SP-1200 Funk", GrooveBase::Sixteenth);
            let steps = vec![
                GrooveStepOffset::new(-0.012, 18.0), // Early downbeat push
                GrooveStepOffset::new(0.032, -8.0),  // Dragged second 16th
                GrooveStepOffset::new(0.005, 8.0),
                GrooveStepOffset::new(0.038, 12.0),
            ];
            t.step_offsets = steps;
            self.templates.push(t);
        }

        // 4. Latin / Triplet
        {
            let mut t = GrooveTemplate::new(10, "Latin Triplet", GrooveBase::Sixteenth);
            let steps = vec![
                GrooveStepOffset::new(0.000, 0.0),
                GrooveStepOffset::new(0.042, 16.0), // Pulled strongly toward triplet subdivision
                GrooveStepOffset::new(0.000, 4.0),
                GrooveStepOffset::new(0.035, 12.0),
            ];
            t.step_offsets = steps;
            self.templates.push(t);
        }
    }

    /// Non-destructively apply groove feel to a list of MIDI notes.
    ///
    /// Applies Quantize %, Timing %, Random %, Velocity % and Global Amount %
    /// using Section 3.3 transfer functions.
    pub fn apply_to_midi_notes(
        &self,
        notes: &[MidiNote],
        template_id: u32,
        seed: u64,
    ) -> Vec<MidiNote> {
        let template = match self.get_template(template_id) {
            Some(t) => t,
            None => return notes.to_vec(),
        };

        let g = template.base.step_duration_beats();
        let glob = self.global_amount.clamp(0.0, 1.30);
        let q = template.quantize_amount.clamp(0.0, 1.0);
        let t_amount = template.timing_amount.clamp(0.0, 2.0);
        let r_amount = template.random_amount.clamp(0.0, 1.0);
        let v_amount = template.velocity_amount.clamp(0.0, 2.0);

        let mut out = Vec::with_capacity(notes.len());

        for (i, note) in notes.iter().enumerate() {
            let t = note.start_beat;

            // 1. Quantize toward grid
            let t_grid = (t / g).round() * g;
            let t_q = t + (q as f64) * (t_grid - t);

            // 2. Timing offset from groove template
            let (dt_template, dv_template) = template.evaluate_offsets(t_q);
            let t_grooved = t_q + (t_amount as f64) * (dt_template as f64) * (glob as f64);

            // 3. Random Gaussian humanizing micro-jitter
            let jitter_sigma = 0.015 * g;
            let pseudo_rnd = lcg_gaussian(seed.wrapping_add(i as u64 * 31337));
            let t_final = (t_grooved + (r_amount as f64) * jitter_sigma * pseudo_rnd * (glob as f64)).max(0.0);

            // 4. Velocity modulation
            let v_offset = (v_amount * dv_template * glob).round() as i32;
            let v_final = ((note.velocity as i32) + v_offset).clamp(1, 127) as u8;

            let mut grooved_note = *note;
            grooved_note.start_beat = t_final;
            grooved_note.velocity = v_final;
            out.push(grooved_note);
        }

        out
    }

    /// Non-destructively apply groove timing to audio clip Warp Markers.
    pub fn apply_to_warp_markers(
        &self,
        markers: &[WarpMarker],
        template_id: u32,
        seed: u64,
    ) -> Vec<WarpMarker> {
        let template = match self.get_template(template_id) {
            Some(t) => t,
            None => return markers.to_vec(),
        };

        let g = template.base.step_duration_beats();
        let glob = self.global_amount.clamp(0.0, 1.30);
        let q = template.quantize_amount.clamp(0.0, 1.0);
        let t_amount = template.timing_amount.clamp(0.0, 2.0);
        let r_amount = template.random_amount.clamp(0.0, 1.0);

        let mut out = Vec::with_capacity(markers.len());

        for (i, m) in markers.iter().enumerate() {
            // Anchor marker 0 remains at beat 0.0
            if m.beat <= 1e-6 {
                out.push(m.clone());
                continue;
            }

            let t = m.beat;
            let t_grid = (t / g).round() * g;
            let t_q = t + (q as f64) * (t_grid - t);

            let (dt_template, _) = template.evaluate_offsets(t_q);
            let t_grooved = t_q + (t_amount as f64) * (dt_template as f64) * (glob as f64);

            let jitter_sigma = 0.015 * g;
            let pseudo_rnd = lcg_gaussian(seed.wrapping_add(i as u64 * 42421));
            let t_final = (t_grooved + (r_amount as f64) * jitter_sigma * pseudo_rnd * (glob as f64)).max(0.0);

            out.push(WarpMarker::new(t_final, m.sample_time));
        }

        // Ensure monotonic sorting by beat
        out.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Destructively commit groove offsets into a MIDI pattern.
    pub fn commit_to_midi_pattern(&self, pattern: &mut MidiPattern, template_id: u32, seed: u64) {
        pattern.notes = self.apply_to_midi_notes(&pattern.notes, template_id, seed);
    }

    /// Destructively commit groove offsets into an audio warper's warp markers.
    pub fn commit_to_warper(&self, warper: &mut OWarper, template_id: u32, seed: u64) {
        warper.markers = self.apply_to_warp_markers(&warper.markers, template_id, seed);
    }
}

/// Simple deterministic zero-mean Gaussian generator via Box-Muller transform using LCG.
fn lcg_gaussian(seed: u64) -> f64 {
    let s1 = (seed.wrapping_mul(6364136223846793005).wrapping_add(1) >> 33) as f64 / 2147483648.0;
    let s2 = (seed.wrapping_mul(1442695040888963407).wrapping_add(1) >> 33) as f64 / 2147483648.0;

    let u1 = s1.clamp(1e-6, 0.999999);
    let u2 = s2.clamp(0.0, 1.0);

    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mpc_swing_timing_application() {
        let pool = GroovePool::default();
        let mpc_template = pool.find_template("MPC 16th 66%").expect("Must have MPC 66% template");

        // Straight 16th notes: 0.0, 0.25, 0.50, 0.75
        let notes = vec![
            MidiNote::new(36, 100, 0.00, 0.2),
            MidiNote::new(42, 100, 0.25, 0.2),
            MidiNote::new(38, 100, 0.50, 0.2),
            MidiNote::new(42, 100, 0.75, 0.2),
        ];

        let grooved = pool.apply_to_midi_notes(&notes, mpc_template.id, 12345);

        // Step 0 and 2 (downbeats) stay on grid
        assert_eq!(grooved[0].start_beat, 0.00);
        assert_eq!(grooved[2].start_beat, 0.50);

        // Step 1 and 3 (offbeats) are pushed late by swing
        assert!(grooved[1].start_beat > 0.25, "Offbeat 1 must be delayed by swing");
        assert!(grooved[3].start_beat > 0.75, "Offbeat 3 must be delayed by swing");
        assert!((grooved[1].start_beat - 0.25) > 0.05);
    }

    #[test]
    fn test_velocity_groove_bounds() {
        let pool = GroovePool::default();
        let sp1200 = pool.find_template("SP-1200").expect("Must have SP-1200 template");

        let notes = vec![
            MidiNote::new(36, 120, 0.0, 0.25),
            MidiNote::new(38, 10, 0.25, 0.25),
        ];

        let grooved = pool.apply_to_midi_notes(&notes, sp1200.id, 999);
        assert!(grooved[0].velocity >= 1 && grooved[0].velocity <= 127);
        assert!(grooved[1].velocity >= 1 && grooved[1].velocity <= 127);
    }

    #[test]
    fn test_warp_markers_groove_application() {
        let pool = GroovePool::default();
        let tr909 = pool.find_template("TR-909").expect("Must have 909 template");

        let markers = vec![
            WarpMarker::new(0.00, 0.0),
            WarpMarker::new(0.25, 0.125),
            WarpMarker::new(0.50, 0.25),
            WarpMarker::new(0.75, 0.375),
        ];

        let grooved_markers = pool.apply_to_warp_markers(&markers, tr909.id, 555);
        assert_eq!(grooved_markers.len(), 4);
        assert_eq!(grooved_markers[0].beat, 0.0);
        assert!(grooved_markers[1].beat > 0.25);
    }
}
