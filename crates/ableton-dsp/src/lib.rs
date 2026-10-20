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

pub mod cytomic;
pub mod drift;
pub mod envelope;
pub mod filter;
pub mod lfo;
pub mod meld;
pub mod operator;
pub mod oscillator;
pub mod parameters;
pub mod physical;
pub mod presets;
pub mod synth;
pub mod voice;
pub mod wavetable;

pub use cytomic::{CircuitModel, CytomicFilterType, CytomicSvfStage, CytomicZdfFilter, DualCytomicFilter, FilterRouting};
pub use drift::{DriftDevice, DriftOsc2Waveform, DriftVoice, DriftVoiceMode};
pub use envelope::{AdsrEnvelope, EnvStage};
pub use filter::MystranZdfLadder;
pub use lfo::{Lfo, LfoShape};
pub use meld::{MeldDevice, MeldMacroEngine, MeldScale, MeldVoice, PlateResonator};
pub use operator::{FmAlgorithm, OperatorDevice, OperatorFreqMode, OperatorUnit, OperatorVoice, OperatorWaveform};
pub use physical::{AnalogDevice, CollisionDevice, ElectricDevice, ResonatorStructure, TensionDevice, TensionExciterType};
pub use wavetable::{UnisonMode, WarpMode, WavetableCategory, WavetableDevice, WavetableSet, WavetableVoice};
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
