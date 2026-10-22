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
pub mod patch_graph;
pub mod physical;
pub mod presets;
pub mod synth;
pub mod voice;
pub mod warp_engine;
pub mod wavetable;

pub use patch_graph::{
    ModularPatchGraph, NodeDspState, PatchCable, PatchFilterMode, PatchGraphError, PatchNode,
    PatchNodeKind, PatchOscShape, MAX_DELAY_SAMPLES, MAX_PATCH_CABLES, MAX_PATCH_NODES,
};
pub use warp_engine::{
    BeatsLoopMode, BeatsParams, ComplexProParams, Radix2Fft2048, SpectralWarpProcessor,
    StreamingDiskCache, TextureParams, WarpMode as AudioWarpMode, WarpPrng, FFT_SIZE, HOP_SIZE,
    PAGE_SIZE,
};

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
pub use ableton_core::{GlobalScale, ScaleMode, TuningSystem, TuningType};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dsp_microtonal_tuning_integration() {
        // Verify all 7 tuning types can be utilized by DSP frequency pipelines
        let systems = [
            TuningType::TwelveTet,
            TuningType::NineteenEdo,
            TuningType::TwentyFourEdo,
            TuningType::ThirtyOneEdo,
            TuningType::BohlenPierce,
            TuningType::JustIntonation,
            TuningType::Scala,
        ];

        for &tt in &systems {
            let mut tuning = TuningSystem::new(tt);
            if tt == TuningType::Scala {
                let scl = "! test\ntest\n2\n!\n600.0\n1200.0\n";
                tuning.load_scala(scl).expect("Scala load");
            }
            let table = tuning.generate_frequency_table_f32();
            assert_eq!(table.len(), 128);
            // Reference pitch at note 69 must be 440 Hz
            assert!((table[69] - 440.0).abs() < 1e-4);
            // All generated frequencies must be positive non-zero and non-NaN
            for (idx, &freq) in table.iter().enumerate() {
                assert!(freq > 0.0, "Frequency at note {} must be positive: {}", idx, freq);
                assert!(!freq.is_nan(), "Frequency must not be NaN");
                assert!(!freq.is_infinite(), "Frequency must not be infinite");
            }
        }
    }
}
