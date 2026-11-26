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
pub use oxide_core::TrackArchetype;
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

#[cfg(test)]
mod fxtl_regression_tests {
    use super::*;

    #[test]
    fn test_fxtl_8_to_11_regressions() {
        // FXTL-8: consolidate_range must split surrounding clip [0, 8) into [0, 2) + [2, 6) + [6, 8)
        let mut track = ArrangementTrack::new(1, "Synth", "MidiSynth");
        let mut pat = MidiPattern::new(8.0);
        pat.add_note(MidiNote::new(60, 100, 1.0, 1.0));
        pat.add_note(MidiNote::new(64, 100, 3.0, 1.0));
        pat.add_note(MidiNote::new(67, 100, 6.5, 1.0));
        track.add_clip(ArrangementClip::new(10, "Full", 0.0, 8.0, ClipPattern::Midi(pat)));
        let cons = track.consolidate_range(2.0, 6.0, 20, "Cons").unwrap();
        assert_eq!(cons.timeline_start_beat, 2.0);
        assert_eq!(cons.duration_beats, 4.0);
        assert_eq!(track.clips.len(), 3, "Surrounding clip must split into left, consolidated, right");
        assert!(track.clips.iter().any(|c| c.timeline_start_beat == 0.0 && c.duration_beats == 2.0));
        assert!(track.clips.iter().any(|c| c.timeline_start_beat == 2.0 && c.duration_beats == 4.0));
        assert!(track.clips.iter().any(|c| c.timeline_start_beat == 6.0 && c.duration_beats == 2.0));

        // FXTL-9: ClipEnvelopeLoop one-shot inverted bounds & MidiPattern negative length stretch
        let env_loop = ClipEnvelopeLoop {
            linked: false,
            loop_enabled: false,
            loop_start_beat: 4.0,
            loop_end_beat: 2.0,
        };
        let ph = env_loop.phase_at(1.0, 0.0, 4.0);
        assert!(ph.is_finite());

        let mut neg_pat = MidiPattern::new(-1.0);
        neg_pat.add_note(MidiNote::new(60, 100, 0.5, 0.5));
        neg_pat.stretch_notes(1.0, 0.0);
        assert!(neg_pat.notes[0].start_beat.is_finite());

        // FXTL-10: AutomationLane interpolate_at(NaN) & draw_stepped(Inf)
        let mut lane = AutomationLane::new(1, "vol");
        lane.add_point(AutomationPoint::linear(0.0, 0.2));
        lane.add_point(AutomationPoint::linear(4.0, 0.8));
        assert_eq!(lane.interpolate_at(f64::NAN, 0.5), 0.5);
        lane.draw_stepped(0.0, f64::INFINITY, 0.5, 1.0);

        // FXTL-11: WarpedAudioClip::new(sr=0) & render_at_bpm on empty raw_samples
        let mut clip = WarpedAudioClip::new("ZeroSR", 0, 2, vec![0.0; 100], 120.0);
        assert!(clip.length_beats.is_finite());
        clip.raw_samples.clear();
        assert!(clip.render_at_bpm(120.0).is_empty());
    }
}
