//! Live 12 Chapter 30: Clip Envelopes & Modulation.
//!
//! Provides clip-level automation and modulation envelopes, unlinked loop lengths,
//! polymetric phasing, pitch bend, modulation wheel (MIDI CC 1), and continuous controller (CC)
//! automation curves.
//!
//! # Chapter 30 Parity Requirements:
//! - **Clip-Level Envelopes**: Envelopes reside directly on clips (Session & Arrangement),
//!   independent of track-level automation.
//! - **Envelope vs Track Automation**: Clip envelopes travel with the clip during duplicate,
//!   scene triggers, and arrangement manipulation.
//! - **Automation vs Modulation**:
//!   - `EnvelopeMode::Automation`: Sets absolute parameter values (0.0 .. 1.0).
//!   - `EnvelopeMode::Modulation`: Computes a continuous relative offset (bipolar centered at 0.5,
//!     or additive/multiplicative offset) applied to the base parameter value.
//! - **Linked vs Unlinked Loop Lengths**:
//!   - Linked: Follows parent clip's loop start, loop end, and loop length.
//!   - Unlinked: Has its own independent loop braces (`loop_start_beat`, `loop_end_beat`),
//!     allowing polymetric phasing (e.g. 3-beat modulation cycling across a 4-beat pattern).
//! - **MIDI Target Envelopes**:
//!   - `PitchBend`: 14-bit signed bipolar value (-8192 .. +8191), or semitone offset.
//!   - `ModulationWheel`: MIDI CC 1 (7-bit 0 .. 127 or normalized 0.0 .. 1.0).
//!   - `ContinuousController(u8)`: Any CC number (e.g. CC 74 Brightness, CC 64 Sustain, CC 7 Volume, CC 10 Pan).
//!   - `ChannelPressure`: Monophonic aftertouch.
//!   - `MixerVolume`, `MixerPan`, `MixerSend`.
//!   - `DeviceParameter`: Specific native synth or FX parameter.

use serde::{Deserialize, Serialize};
use crate::automation::{AutomationCurveType, AutomationLane, AutomationPoint};

/// Operational mode of the clip envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeMode {
    /// Absolute parameter automation (clamps to 0.0 .. 1.0).
    Automation,
    /// Relative bipolar modulation offset centered at 0.5 (± modulation range).
    Modulation,
}

/// Target destination for clip envelope control.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClipEnvelopeTarget {
    /// 14-bit MIDI Pitch Bend (-8192 .. +8191).
    PitchBend,
    /// MIDI Continuous Controller 1 (Modulation Wheel).
    ModulationWheel,
    /// Arbitrary MIDI Continuous Controller (0 .. 127).
    ContinuousController(u8),
    /// Channel Pressure / Monophonic Aftertouch.
    ChannelPressure,
    /// Clip-level Mixer Volume.
    MixerVolume,
    /// Clip-level Mixer Panning.
    MixerPan,
    /// Clip-level Aux Send bus.
    MixerSend { send_idx: u8 },
    /// Native or hosted device parameter.
    DeviceParameter {
        device_id: u32,
        param_id: u32,
        param_name: String,
    },
}

impl ClipEnvelopeTarget {
    pub fn display_name(&self) -> String {
        match self {
            Self::PitchBend => "Pitch Bend".to_string(),
            Self::ModulationWheel => "Mod Wheel (CC 1)".to_string(),
            Self::ContinuousController(cc) => match *cc {
                7 => "CC 7 (Volume)".to_string(),
                10 => "CC 10 (Pan)".to_string(),
                11 => "CC 11 (Expression)".to_string(),
                64 => "CC 64 (Sustain)".to_string(),
                71 => "CC 71 (Resonance)".to_string(),
                74 => "CC 74 (Brightness/Cutoff)".to_string(),
                other => format!("MIDI CC {}", other),
            },
            Self::ChannelPressure => "Aftertouch".to_string(),
            Self::MixerVolume => "Mixer Volume".to_string(),
            Self::MixerPan => "Mixer Pan".to_string(),
            Self::MixerSend { send_idx } => {
                let letter = (b'A' + send_idx) as char;
                format!("Send {}", letter)
            }
            Self::DeviceParameter { param_name, .. } => param_name.clone(),
        }
    }
}

/// Loop synchronization settings for a clip envelope.
///
/// Supports Ableton Live's Linked vs. Unlinked loop semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipEnvelopeLoop {
    /// If true, the envelope length and phase track the parent clip's loop braces.
    /// If false, the envelope loops with independent braces, creating polymetric phasing.
    pub linked: bool,
    /// Whether looping is active when unlinked. If false, envelope plays as a one-shot.
    pub loop_enabled: bool,
    /// Start beat for the independent envelope loop.
    pub loop_start_beat: f64,
    /// End beat for the independent envelope loop.
    pub loop_end_beat: f64,
}

impl Default for ClipEnvelopeLoop {
    fn default() -> Self {
        Self {
            linked: true,
            loop_enabled: true,
            loop_start_beat: 0.0,
            loop_end_beat: 4.0,
        }
    }
}

impl ClipEnvelopeLoop {
    pub fn new_linked() -> Self {
        Self {
            linked: true,
            loop_enabled: true,
            loop_start_beat: 0.0,
            loop_end_beat: 4.0,
        }
    }

    pub fn new_unlinked(start_beat: f64, end_beat: f64) -> Self {
        Self {
            linked: false,
            loop_enabled: true,
            loop_start_beat: start_beat,
            loop_end_beat: end_beat.max(start_beat + 1e-4),
        }
    }

    /// Effective unlinked loop length in beats.
    pub fn loop_length_beats(&self) -> f64 {
        (self.loop_end_beat - self.loop_start_beat).max(1e-4)
    }

    /// Compute wrapped envelope phase at a given clip playhead beat.
    ///
    /// For Linked envelopes:
    /// `phase = clip_loop_start + ((clip_beat - clip_loop_start) % clip_length)`
    ///
    /// For Unlinked envelopes:
    /// `phase = loop_start_beat + ((clip_beat - loop_start_beat) % env_length)`
    pub fn phase_at(&self, clip_beat: f64, clip_loop_start: f64, clip_loop_end: f64) -> f64 {
        if self.linked {
            let clip_len = (clip_loop_end - clip_loop_start).max(1e-4);
            let rel = clip_beat - clip_loop_start;
            clip_loop_start + (((rel % clip_len) + clip_len) % clip_len)
        } else if self.loop_enabled {
            let env_len = self.loop_length_beats();
            let rel = clip_beat - self.loop_start_beat;
            self.loop_start_beat + (((rel % env_len) + env_len) % env_len)
        } else {
            // One-shot unlinked envelope: clamped to bounds
            clip_beat.clamp(self.loop_start_beat, self.loop_end_beat)
        }
    }
}

/// Clip-level envelope controlling a target parameter via breakpoint curves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipEnvelope {
    pub target: ClipEnvelopeTarget,
    pub mode: EnvelopeMode,
    pub loop_config: ClipEnvelopeLoop,
    pub lane: AutomationLane,
    /// Modulation depth range (1.0 = ±100% relative swing).
    pub modulation_range: f32,
}

impl ClipEnvelope {
    pub fn new(target: ClipEnvelopeTarget, mode: EnvelopeMode) -> Self {
        let name = target.display_name();
        Self {
            target,
            mode,
            loop_config: ClipEnvelopeLoop::default(),
            lane: AutomationLane::new(0, name),
            modulation_range: 1.0,
        }
    }

    pub fn new_pitch_bend() -> Self {
        Self::new(ClipEnvelopeTarget::PitchBend, EnvelopeMode::Modulation)
    }

    pub fn new_mod_wheel() -> Self {
        Self::new(ClipEnvelopeTarget::ModulationWheel, EnvelopeMode::Automation)
    }

    pub fn new_cc(cc_num: u8) -> Self {
        Self::new(ClipEnvelopeTarget::ContinuousController(cc_num), EnvelopeMode::Automation)
    }

    pub fn new_mixer_volume(mode: EnvelopeMode) -> Self {
        Self::new(ClipEnvelopeTarget::MixerVolume, mode)
    }

    pub fn new_mixer_pan(mode: EnvelopeMode) -> Self {
        Self::new(ClipEnvelopeTarget::MixerPan, mode)
    }

    pub fn new_device_parameter(
        device_id: u32,
        param_id: u32,
        param_name: impl Into<String>,
        mode: EnvelopeMode,
    ) -> Self {
        let name = param_name.into();
        Self {
            target: ClipEnvelopeTarget::DeviceParameter {
                device_id,
                param_id,
                param_name: name.clone(),
            },
            mode,
            loop_config: ClipEnvelopeLoop::default(),
            lane: AutomationLane::new(param_id, name),
            modulation_range: 1.0,
        }
    }

    /// Add or update a breakpoint on this envelope.
    pub fn add_breakpoint(&mut self, beat: f64, value_norm: f32, curve: f32) {
        self.lane.add_point(AutomationPoint::new(beat, value_norm.clamp(0.0, 1.0), curve));
    }

    /// Add a linear breakpoint.
    pub fn add_linear_point(&mut self, beat: f64, value_norm: f32) {
        self.lane.add_point(AutomationPoint::linear(beat, value_norm.clamp(0.0, 1.0)));
    }

    /// Add a breakpoint with explicit curve type.
    pub fn add_curved_point(&mut self, beat: f64, value_norm: f32, curve_type: AutomationCurveType) {
        self.lane.add_point(AutomationPoint::with_curve_type(beat, value_norm.clamp(0.0, 1.0), curve_type));
    }

    /// Clear all breakpoints.
    pub fn clear(&mut self) {
        self.lane.clear();
    }

    /// Set envelope loop to follow parent clip loop braces.
    pub fn set_linked(&mut self) {
        self.loop_config.linked = true;
    }

    /// Set envelope loop to independent unlinked range.
    pub fn set_unlinked(&mut self, start_beat: f64, end_beat: f64) {
        self.loop_config.linked = false;
        self.loop_config.loop_start_beat = start_beat;
        self.loop_config.loop_end_beat = end_beat.max(start_beat + 1e-4);
    }

    /// Configure unlinked looping state.
    pub fn set_loop_enabled(&mut self, enabled: bool) {
        self.loop_config.loop_enabled = enabled;
    }

    /// Default neutral raw normalized value when lane is unpopulated.
    pub fn default_raw_value(&self) -> f32 {
        match self.target {
            ClipEnvelopeTarget::PitchBend => 0.5, // Center pitch (0 deflection)
            ClipEnvelopeTarget::MixerPan => 0.5,   // Center pan
            _ => {
                if self.mode == EnvelopeMode::Modulation {
                    0.5 // Neutral 0% modulation offset
                } else {
                    0.0
                }
            }
        }
    }

    /// Calculate phase beat according to linked/unlinked loop configuration.
    pub fn phase_at(&self, clip_beat: f64, clip_loop_start: f64, clip_loop_end: f64) -> f64 {
        self.loop_config.phase_at(clip_beat, clip_loop_start, clip_loop_end)
    }

    /// Evaluate normalized raw breakpoint value at `clip_beat`.
    pub fn evaluate_raw(&self, clip_beat: f64, clip_loop_start: f64, clip_loop_end: f64) -> f32 {
        let phase = self.phase_at(clip_beat, clip_loop_start, clip_loop_end);
        let default_val = self.default_raw_value();
        self.lane.interpolate_at(phase, default_val).clamp(0.0, 1.0)
    }

    /// Calculate relative modulation delta offset (-modulation_range .. +modulation_range).
    ///
    /// At breakpoint 0.5, delta is 0.0.
    /// At breakpoint 1.0, delta is +modulation_range.
    /// At breakpoint 0.0, delta is -modulation_range.
    pub fn evaluate_modulation_delta(&self, clip_beat: f64, clip_loop_start: f64, clip_loop_end: f64) -> f32 {
        let raw = self.evaluate_raw(clip_beat, clip_loop_start, clip_loop_end);
        (raw - 0.5) * 2.0 * self.modulation_range
    }

    /// Evaluate effective parameter value given a base parameter value.
    ///
    /// If `Automation` mode: returns raw envelope value in [0.0, 1.0].
    /// If `Modulation` mode: returns `(base_val + delta).clamp(0.0, 1.0)`.
    pub fn evaluate_effective(
        &self,
        clip_beat: f64,
        clip_loop_start: f64,
        clip_loop_end: f64,
        base_val: f32,
    ) -> f32 {
        match self.mode {
            EnvelopeMode::Automation => self.evaluate_raw(clip_beat, clip_loop_start, clip_loop_end),
            EnvelopeMode::Modulation => {
                let delta = self.evaluate_modulation_delta(clip_beat, clip_loop_start, clip_loop_end);
                (base_val + delta).clamp(0.0, 1.0)
            }
        }
    }

    /// Evaluate 14-bit signed MIDI Pitch Bend (-8192 .. +8191).
    pub fn evaluate_pitch_bend_14bit(&self, clip_beat: f64, clip_loop_start: f64, clip_loop_end: f64) -> i16 {
        let raw = self.evaluate_raw(clip_beat, clip_loop_start, clip_loop_end);
        let centered = (raw - 0.5) * 2.0; // [-1.0, 1.0]
        if centered >= 0.0 {
            (centered * 8191.0).round() as i16
        } else {
            (centered * 8192.0).round() as i16
        }
    }

    /// Evaluate pitch bend in musical semitones given a bend range (e.g. ±2.0 or ±12.0 st).
    pub fn evaluate_pitch_bend_semitones(
        &self,
        clip_beat: f64,
        clip_loop_start: f64,
        clip_loop_end: f64,
        bend_range_semitones: f32,
    ) -> f32 {
        let raw = self.evaluate_raw(clip_beat, clip_loop_start, clip_loop_end);
        (raw - 0.5) * 2.0 * bend_range_semitones
    }

    /// Evaluate 7-bit MIDI CC 1 Modulation Wheel value (0 .. 127).
    pub fn evaluate_mod_wheel_7bit(&self, clip_beat: f64, clip_loop_start: f64, clip_loop_end: f64) -> u8 {
        let raw = self.evaluate_raw(clip_beat, clip_loop_start, clip_loop_end);
        (raw * 127.0).round() as u8
    }

    /// Evaluate 7-bit MIDI Continuous Controller value (0 .. 127).
    pub fn evaluate_cc_7bit(&self, clip_beat: f64, clip_loop_start: f64, clip_loop_end: f64) -> u8 {
        let raw = self.evaluate_raw(clip_beat, clip_loop_start, clip_loop_end);
        (raw * 127.0).round() as u8
    }
}

/// Collection of clip-level envelopes associated with a clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ClipEnvelopeCollection {
    pub envelopes: Vec<ClipEnvelope>,
}

impl ClipEnvelopeCollection {
    pub fn new() -> Self {
        Self {
            envelopes: Vec::new(),
        }
    }

    /// Find an envelope for a specific target.
    pub fn get_envelope(&self, target: &ClipEnvelopeTarget) -> Option<&ClipEnvelope> {
        self.envelopes.iter().find(|e| &e.target == target)
    }

    /// Find a mutable envelope for a specific target.
    pub fn get_envelope_mut(&mut self, target: &ClipEnvelopeTarget) -> Option<&mut ClipEnvelope> {
        self.envelopes.iter_mut().find(|e| &e.target == target)
    }

    /// Retrieve or create an envelope for the specified target.
    pub fn get_or_create_envelope(
        &mut self,
        target: ClipEnvelopeTarget,
        mode: EnvelopeMode,
    ) -> &mut ClipEnvelope {
        if let Some(pos) = self.envelopes.iter().position(|e| e.target == target) {
            &mut self.envelopes[pos]
        } else {
            self.envelopes.push(ClipEnvelope::new(target, mode));
            self.envelopes.last_mut().unwrap()
        }
    }

    /// Insert or replace an envelope in the collection.
    pub fn add_envelope(&mut self, envelope: ClipEnvelope) {
        if let Some(pos) = self.envelopes.iter().position(|e| e.target == envelope.target) {
            self.envelopes[pos] = envelope;
        } else {
            self.envelopes.push(envelope);
        }
    }

    /// Remove an envelope matching target.
    pub fn remove_envelope(&mut self, target: &ClipEnvelopeTarget) -> Option<ClipEnvelope> {
        if let Some(pos) = self.envelopes.iter().position(|e| &e.target == target) {
            Some(self.envelopes.remove(pos))
        } else {
            None
        }
    }

    /// Evaluate effective value for a target parameter, or return `base_val` if no envelope exists.
    pub fn evaluate_effective(
        &self,
        target: &ClipEnvelopeTarget,
        clip_beat: f64,
        clip_loop_start: f64,
        clip_loop_end: f64,
        base_val: f32,
    ) -> f32 {
        if let Some(env) = self.get_envelope(target) {
            env.evaluate_effective(clip_beat, clip_loop_start, clip_loop_end, base_val)
        } else {
            base_val
        }
    }

    /// Evaluate pitch bend 14-bit integer, or 0 if no pitch bend envelope exists.
    pub fn evaluate_pitch_bend_14bit(
        &self,
        clip_beat: f64,
        clip_loop_start: f64,
        clip_loop_end: f64,
    ) -> i16 {
        if let Some(env) = self.get_envelope(&ClipEnvelopeTarget::PitchBend) {
            env.evaluate_pitch_bend_14bit(clip_beat, clip_loop_start, clip_loop_end)
        } else {
            0
        }
    }

    /// Evaluate pitch bend semitones, or 0.0 if none exists.
    pub fn evaluate_pitch_bend_semitones(
        &self,
        clip_beat: f64,
        clip_loop_start: f64,
        clip_loop_end: f64,
        bend_range_semitones: f32,
    ) -> f32 {
        if let Some(env) = self.get_envelope(&ClipEnvelopeTarget::PitchBend) {
            env.evaluate_pitch_bend_semitones(clip_beat, clip_loop_start, clip_loop_end, bend_range_semitones)
        } else {
            0.0
        }
    }

    /// Evaluate modulation wheel 7-bit, or default 0 if none exists.
    pub fn evaluate_mod_wheel_7bit(
        &self,
        clip_beat: f64,
        clip_loop_start: f64,
        clip_loop_end: f64,
    ) -> u8 {
        if let Some(env) = self.get_envelope(&ClipEnvelopeTarget::ModulationWheel) {
            env.evaluate_mod_wheel_7bit(clip_beat, clip_loop_start, clip_loop_end)
        } else {
            0
        }
    }

    /// Evaluate Continuous Controller 7-bit, or default 0 if none exists.
    pub fn evaluate_cc_7bit(
        &self,
        cc_num: u8,
        clip_beat: f64,
        clip_loop_start: f64,
        clip_loop_end: f64,
    ) -> u8 {
        if let Some(env) = self.get_envelope(&ClipEnvelopeTarget::ContinuousController(cc_num)) {
            env.evaluate_cc_7bit(clip_beat, clip_loop_start, clip_loop_end)
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_envelope_linked_loop_evaluation() {
        let mut env = ClipEnvelope::new_mixer_volume(EnvelopeMode::Automation);
        // Ramp from 0.0 at beat 0.0 to 1.0 at beat 4.0
        env.add_linear_point(0.0, 0.0);
        env.add_linear_point(4.0, 1.0);

        // Linked loop over 4.0 beats: [0.0, 4.0)
        assert_eq!(env.evaluate_raw(0.0, 0.0, 4.0), 0.0);
        assert!((env.evaluate_raw(2.0, 0.0, 4.0) - 0.5).abs() < 1e-4);
        assert!((env.evaluate_raw(4.0, 0.0, 4.0) - 0.0).abs() < 1e-4); // Wraps to beat 0.0
        assert!((env.evaluate_raw(6.0, 0.0, 4.0) - 0.5).abs() < 1e-4); // Wraps to beat 2.0
    }

    #[test]
    fn test_clip_envelope_unlinked_polymetric_phasing() {
        let mut env = ClipEnvelope::new_pitch_bend();
        // Unlinked 3-beat envelope looping inside a 4-beat clip
        env.set_unlinked(0.0, 3.0);
        env.add_linear_point(0.0, 0.5); // Center 0 bend
        env.add_linear_point(1.5, 1.0); // Maximum bend up
        env.add_linear_point(3.0, 0.0); // Maximum bend down

        let clip_start = 0.0;
        let clip_end = 4.0;

        // At beat 0.0: phase = 0.0 (raw = 0.5)
        assert_eq!(env.phase_at(0.0, clip_start, clip_end), 0.0);
        assert!((env.evaluate_raw(0.0, clip_start, clip_end) - 0.5).abs() < 1e-4);

        // At beat 1.5: phase = 1.5 (raw = 1.0)
        assert_eq!(env.phase_at(1.5, clip_start, clip_end), 1.5);
        assert!((env.evaluate_raw(1.5, clip_start, clip_end) - 1.0).abs() < 1e-4);

        // At beat 3.0: phase wraps to 0.0 (raw = 0.5)
        assert_eq!(env.phase_at(3.0, clip_start, clip_end), 0.0);
        assert!((env.evaluate_raw(3.0, clip_start, clip_end) - 0.5).abs() < 1e-4);

        // At beat 4.0 (start of clip's second iteration):
        // Envelope is at phase 4.0 % 3.0 = 1.0 beat!
        assert!((env.phase_at(4.0, clip_start, clip_end) - 1.0).abs() < 1e-4);

        // At beat 8.0 (start of clip's third iteration):
        // Envelope is at phase 8.0 % 3.0 = 2.0 beats!
        assert!((env.phase_at(8.0, clip_start, clip_end) - 2.0).abs() < 1e-4);

        // At beat 12.0 (LCM of 4 and 3 = 12 beats):
        // Phasing cycles realign at beat 0.0!
        assert!((env.phase_at(12.0, clip_start, clip_end) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_clip_envelope_automation_vs_modulation_mode() {
        // 1. Automation mode: absolute override
        let mut auto_env = ClipEnvelope::new(ClipEnvelopeTarget::MixerVolume, EnvelopeMode::Automation);
        auto_env.add_linear_point(0.0, 0.8);
        assert_eq!(auto_env.evaluate_effective(0.0, 0.0, 4.0, 0.2), 0.8);

        // 2. Modulation mode: relative bipolar offset
        let mut mod_env = ClipEnvelope::new(ClipEnvelopeTarget::MixerVolume, EnvelopeMode::Modulation);
        // Breakpoint at 0.5 (center) -> delta 0.0
        mod_env.add_linear_point(0.0, 0.5);
        // Breakpoint at 1.0 (+100% modulation range) -> delta +1.0
        mod_env.add_linear_point(2.0, 1.0);
        // Breakpoint at 0.25 (-50% modulation range) -> delta -0.5
        mod_env.add_linear_point(4.0, 0.25);

        let base_val = 0.5;
        assert_eq!(mod_env.evaluate_effective(0.0, 0.0, 8.0, base_val), 0.5);
        assert_eq!(mod_env.evaluate_effective(2.0, 0.0, 8.0, base_val), 1.0); // 0.5 + 0.5 clamped
        assert_eq!(mod_env.evaluate_effective(4.0, 0.0, 8.0, base_val), 0.0); // 0.5 - 0.5 = 0.0
        // At beat 8.0, wraps to 0.0 -> 0.5
        assert_eq!(mod_env.evaluate_effective(8.0, 0.0, 8.0, base_val), 0.5);
    }

    #[test]
    fn test_clip_envelope_pitch_bend_14bit_and_semitones() {
        let mut pb_env = ClipEnvelope::new_pitch_bend();
        pb_env.add_linear_point(0.0, 0.5);  // Neutral center
        pb_env.add_linear_point(1.0, 1.0);  // Maximum bend up
        pb_env.add_linear_point(2.0, 0.0);  // Maximum bend down
        pb_env.add_linear_point(3.0, 0.75); // Half bend up

        let clip_start = 0.0;
        let clip_end = 4.0;

        // Neutral center: 0 in 14-bit signed, 0.0 semitones
        assert_eq!(pb_env.evaluate_pitch_bend_14bit(0.0, clip_start, clip_end), 0);
        assert_eq!(pb_env.evaluate_pitch_bend_semitones(0.0, clip_start, clip_end, 2.0), 0.0);

        // Maximum bend up: +8191, +2.0 semitones
        assert_eq!(pb_env.evaluate_pitch_bend_14bit(1.0, clip_start, clip_end), 8191);
        assert_eq!(pb_env.evaluate_pitch_bend_semitones(1.0, clip_start, clip_end, 2.0), 2.0);

        // Maximum bend down: -8192, -2.0 semitones
        assert_eq!(pb_env.evaluate_pitch_bend_14bit(2.0, clip_start, clip_end), -8192);
        assert_eq!(pb_env.evaluate_pitch_bend_semitones(2.0, clip_start, clip_end, 2.0), -2.0);

        // Half bend up: ~4096, +1.0 semitones
        assert!((pb_env.evaluate_pitch_bend_semitones(3.0, clip_start, clip_end, 2.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_clip_envelope_mod_wheel_and_cc_automation() {
        let mut mw_env = ClipEnvelope::new_mod_wheel();
        mw_env.add_linear_point(0.0, 0.0);
        mw_env.add_linear_point(2.0, 0.5);
        mw_env.add_linear_point(4.0, 1.0);

        assert_eq!(mw_env.evaluate_mod_wheel_7bit(0.0, 0.0, 8.0), 0);
        assert_eq!(mw_env.evaluate_mod_wheel_7bit(2.0, 0.0, 8.0), 64);
        assert_eq!(mw_env.evaluate_mod_wheel_7bit(4.0, 0.0, 8.0), 127);
        // At beat 8.0, wraps to 0.0 -> 0
        assert_eq!(mw_env.evaluate_mod_wheel_7bit(8.0, 0.0, 8.0), 0);

        let mut cc74_env = ClipEnvelope::new_cc(74); // Brightness / Cutoff
        cc74_env.add_linear_point(0.0, 0.25);
        cc74_env.add_linear_point(4.0, 0.75);

        assert_eq!(cc74_env.evaluate_cc_7bit(0.0, 0.0, 8.0), 32);
        assert_eq!(cc74_env.evaluate_cc_7bit(4.0, 0.0, 8.0), 95);
        // At beat 8.0, wraps to 0.0 -> 32
        assert_eq!(cc74_env.evaluate_cc_7bit(8.0, 0.0, 8.0), 32);
    }

    #[test]
    fn test_clip_envelope_collection_multi_target_evaluation() {
        let mut coll = ClipEnvelopeCollection::new();

        let mut pb = ClipEnvelope::new_pitch_bend();
        pb.add_linear_point(0.0, 1.0);
        coll.add_envelope(pb);

        let mut mw = ClipEnvelope::new_mod_wheel();
        mw.add_linear_point(0.0, 0.8);
        coll.add_envelope(mw);

        let mut cc74 = ClipEnvelope::new_cc(74);
        cc74.add_linear_point(0.0, 0.5);
        coll.add_envelope(cc74);

        assert_eq!(coll.evaluate_pitch_bend_14bit(0.0, 0.0, 4.0), 8191);
        assert_eq!(coll.evaluate_mod_wheel_7bit(0.0, 0.0, 4.0), 102);
        assert_eq!(coll.evaluate_cc_7bit(74, 0.0, 0.0, 4.0), 64);

        // Missing CC 10 returns default 0
        assert_eq!(coll.evaluate_cc_7bit(10, 0.0, 0.0, 4.0), 0);
    }

    #[test]
    fn test_clip_envelope_unlinked_one_shot_non_looping() {
        let mut env = ClipEnvelope::new_mixer_volume(EnvelopeMode::Automation);
        env.set_unlinked(0.0, 2.0);
        env.set_loop_enabled(false); // One-shot
        env.add_linear_point(0.0, 0.0);
        env.add_linear_point(2.0, 1.0);

        // Beat 1.0 is halfway
        assert!((env.evaluate_raw(1.0, 0.0, 4.0) - 0.5).abs() < 1e-4);
        // Past beat 2.0, one-shot clamps to end (1.0), does NOT wrap
        assert_eq!(env.evaluate_raw(3.0, 0.0, 4.0), 1.0);
        assert_eq!(env.evaluate_raw(10.0, 0.0, 4.0), 1.0);
    }
}
