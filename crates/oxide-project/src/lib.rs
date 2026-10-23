//! Universal DAW project persistence, JSON serialization schemas, and disk I/O.

pub mod als;
pub mod drums;
pub mod error;
pub mod mixer;
pub mod project;
pub mod synth;
pub mod track;

pub use als::*;
pub use drums::{DrumPadDefinition, DrumRackDefinition};
pub use error::DAWProjectError;
pub use mixer::{InsertEffectConfig, MasterTrackDefinition, MixerState};
pub use project::{DAWProject, ProjectMetadata, TransportConfig};
pub use synth::{
    load_all_factory_macros, load_all_factory_presets, MacroBreakpointDefinition,
    MacroMappingDefinition, MacroTargetDefinition, PresetDefinition, SynthPatchParameters,
};
pub use track::{InstrumentConfig, TrackDefinition, TrackType};

pub type DAWTrack = TrackDefinition;
pub type DAWClip = oxide_timeline::SessionClip;
pub type MixerChannel = MixerState;
