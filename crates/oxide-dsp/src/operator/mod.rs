//! Operator 4-operator FM & Additive Synthesizer.

pub mod synth;

pub use synth::{
    FmAlgorithm, OperatorDevice, OperatorFreqMode, OperatorUnit, OperatorVoice, OperatorWaveform,
    MAX_OPERATOR_VOICES, NUM_OPERATORS,
};
