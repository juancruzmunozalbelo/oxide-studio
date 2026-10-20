//! Physical Modeling & Virtual Analog Synthesizers for Ableton Live 12.

pub mod analog;
pub mod collision;
pub mod electric;
pub mod tension;

pub use analog::{AnalogDevice, AnalogVoice, AnalogWaveform, MAX_ANALOG_VOICES};
pub use collision::{CollisionDevice, CollisionVoice, ModalFilter, ResonatorStructure, MAX_COLLISION_VOICES, NUM_MODAL_MODES};
pub use electric::{ElectricDevice, ElectricVoice, MAX_ELECTRIC_VOICES};
pub use tension::{TensionDevice, TensionExciterType, TensionVoice, MAX_TENSION_VOICES};
