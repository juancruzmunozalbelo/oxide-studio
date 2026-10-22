//! Session Clip Matrix, Linear Arrangement Timeline, Automation Curves, 6-Mode Warping,
//! Global Groove Pool, Comping Take Lanes, and Full Track Model.

pub mod arrangement;
pub mod automation;
pub mod clip;
pub mod comping;
pub mod generate;
pub mod groove;
pub mod mpe;
pub mod pattern;
pub mod quantization;
pub mod recording;
pub mod session;
pub mod slot;
pub mod track;
pub mod transform;
pub mod warping;
pub mod workflow_advanced;

pub use arrangement::{
    ArrangementClip, ArrangementTimeline, ArrangementTrack, LoopRegion, TimelineLocator,
};
pub use automation::{
    interpolate_automation, simplify_points_rdp, AutomationCurveType, AutomationLane,
    AutomationPoint, AutomationTakeoverTracker, CubicBezierHandle, DrawModeState,
    MultiLaneAutomation,
};
pub use clip::{
    ClipEnvelope, ClipEnvelopeCollection, ClipEnvelopeLoop, ClipEnvelopeTarget, EnvelopeMode,
};
pub use comping::{MicroCrossfade, TakeLane, TrackCompingManager};
pub use generate::{
    generate_euclidean, generate_rhythm, generate_seed, generate_shape, generate_stacks,
    ContourShape, RhythmStyle, ScaleLock, StacksChordType, StacksVoicing,
};
pub use groove::{GrooveBase, GroovePool, GrooveStepOffset, GrooveTemplate};
pub use mpe::{MpeExpressionPoint, MpeNote, MpeStreamParser, MpeZone, PerNoteExpression};
pub use pattern::{
    AudioClipData, ClipPattern, DrumStep, DrumStepPattern, DrumStepTrack, MidiNote, MidiPattern,
};
pub use quantization::{
    calculate_launch_beat, calculate_launch_beat_with_epsilon, QuantizeMode, TriggerMode,
    DEFAULT_EPSILON,
};
pub use recording::{ArrangementControlState, SessionToArrangementRecorder};
pub use session::{Scene, SessionClip, SessionMatrix, SessionSlot, SessionTrack};
pub use slot::SlotState;
pub use track::{
    CrossfadeAssign, OutputDestination, SendRoutingMode, Track, TrackManager, TrackSend, TrackType,
};
pub use ableton_core::TrackArchetype;
pub use transform::{
    transform_arpeggiate, transform_connect, transform_humanize, transform_invert,
    transform_legato, transform_ornament, transform_quantize, transform_recombine,
    transform_reverse, transform_span, transform_strum, transform_time_warp,
    MidiTransformEngine, SpanMode, StrumDirection,
};
pub use warping::{
    decode_wav_pcm, detect_transients, fft, ifft, BeatsWarpConfig, ComplexProWarpConfig,
    OWarper, TextureWarpConfig, TonesWarpConfig, TransientLoopMode, WarpedAudioClip, WarperConfig,
    WarpMarker, WarpMode,
};
pub use workflow_advanced::{
    FollowAction, FollowActionRule, FrozenAudioTail, LinkedTrackGroup, TrackFreezeEngine,
    TrackGroupBus,
};
