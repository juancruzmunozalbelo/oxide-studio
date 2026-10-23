//! Drift Subtractive Synthesizer with Analog Drift Instability & MPE.

pub mod synth;

pub use synth::{DriftDevice, DriftOsc2Waveform, DriftVoice, DriftVoiceMode, MAX_DRIFT_VOICES};
