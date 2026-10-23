//! Meld Bi-Timbral Macro Synthesizer module.

pub mod synth;

pub use synth::{
    MeldDevice, MeldMacroEngine, MeldScale, MeldVoice, PlateResonator, PlateResonatorStage,
    MAX_MELD_VOICES, NUM_PLATE_MODES,
};
