//! Session Clip Matrix, Linear Arrangement Timeline, Automation Curves, and Session-to-Arrangement Recording.

pub mod arrangement;
pub mod automation;
pub mod generate;
pub mod pattern;
pub mod quantization;
pub mod recording;
pub mod session;
pub mod slot;
pub mod transform;

pub use arrangement::{ArrangementClip, ArrangementTimeline, ArrangementTrack, LoopRegion};
pub use automation::{interpolate_automation, AutomationLane, AutomationPoint};
pub use generate::{
    generate_euclidean, generate_rhythm, generate_seed, generate_shape, generate_stacks,
    ContourShape, RhythmStyle, ScaleLock, StacksChordType, StacksVoicing,
};
pub use pattern::{
    AudioClipData, ClipPattern, DrumStep, DrumStepPattern, DrumStepTrack, MidiNote, MidiPattern,
};
pub use quantization::{
    calculate_launch_beat, calculate_launch_beat_with_epsilon, QuantizeMode, DEFAULT_EPSILON,
};
pub use recording::{ArrangementControlState, SessionToArrangementRecorder};
pub use session::{Scene, SessionClip, SessionMatrix, SessionSlot, SessionTrack};
pub use slot::SlotState;
pub use transform::{
    transform_arpeggiate, transform_connect, transform_ornament, transform_quantize,
    transform_recombine, transform_span, transform_strum, transform_time_warp, SpanMode,
    StrumDirection,
};
