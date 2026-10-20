//! Wavetable Synthesizer module for Ableton Live 12.

pub mod mod_matrix;
pub mod synth;
pub mod tables;

pub use mod_matrix::{ModDestination, ModMatrix, ModSlot, ModSource, NUM_MOD_SLOTS};
pub use synth::{UnisonMode, WarpMode, WavetableDevice, WavetableVoice, MAX_WAVETABLE_VOICES};
pub use tables::{WavetableCategory, WavetableSet, NUM_FRAMES, TABLE_SIZE};
