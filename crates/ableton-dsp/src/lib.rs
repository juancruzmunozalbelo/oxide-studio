//! Real-time DSP library for ableton-rust.
//!
//! Provides the core digital signal processing modules reverse-engineered from
//! Ableton Live 12.4.6 Suite and Ableton Learning Synths:
//! - Mystran 2x-oversampled non-linear ZDF 4-pole low-pass ladder filter
//! - Band-limited PolyBLEP Sawtooth and Square/PWM oscillators
//! - 32-bit XorShift white noise generator
//! - Dual ADSR envelopes with RNBO convex attack lockout and exponential decay/release
//! - 4-shape LFO (Sine, Saw Down, Square, Sample & Hold)
//! - Monophonic last-note priority voice stack with exponential one-pole glide
//! - 38 factory synth presets catalog & 17 XY macro mappings with piece-wise breakpoint interpolation
//! - Unified `SynthEngine` instrument

pub mod envelope;
pub mod filter;
pub mod lfo;
pub mod oscillator;
pub mod parameters;
pub mod presets;
pub mod synth;
pub mod voice;

pub use envelope::{AdsrEnvelope, EnvStage};
pub use filter::MystranZdfLadder;
pub use lfo::{Lfo, LfoShape};
pub use oscillator::{mix_oscillators, poly_blep, PulseOscillator, SawOscillator, XorShift32};
pub use parameters::{
    ParamId, ParameterBank, ParameterDescriptor, ScalingCurve, PARAMETER_COUNT,
    PARAMETER_DESCRIPTORS,
};
pub use presets::{
    apply_macro_x, apply_macro_y, apply_preset, evaluate_macro_target, load_mapping_catalog,
    load_preset_catalog, MacroBreakpoint, MacroMapping, MacroTarget, Preset, PresetParamEntry,
};
pub use synth::SynthEngine;
pub use voice::{
    MonophonicVoice, NoteEvent, NoteStack, VoiceReleaseAction, VoiceTriggerAction,
};
