//! Effects and mixing engine for ableton-rust.
//!
//! Includes:
//! - Padé [4/4] tanh Saturator / Overdrive
//! - Tempo-synced ping-pong Delay with feedback filtering
//! - Algorithmic FDN Reverb and Convolution Reverb
//! - 128-sample lookahead Master Brickwall Limiter (-0.3 dBFS ceiling)
//! - 16-track Mixer with solo/mute and 300ms peak/RMS ballistics metering

pub mod auto_filter;
pub mod compressor;
pub mod delay;
pub mod eq8;
pub mod glue_compressor;
pub mod limiter;
pub mod midi_fx;
pub mod mixer;
pub mod multiband_dynamics;
pub mod reverb;
pub mod roar;
pub mod saturator;
pub mod suite_fx;

pub use ableton_core::DeviceChain;
pub use auto_filter::{AutoFilter, AutoFilterLfoShape, CircuitModel, CytomicFilterType};
pub use compressor::{Compressor, CompressorMode};
pub use delay::{DelaySubdivision, TempoDelay, MAX_DELAY_SAMPLES};
pub use eq8::{EQEight, EqBand, EqBandType, EqStereoMode};
pub use glue_compressor::GlueCompressor;
pub use limiter::{MasterLimiter, CEILING_LINEAR, LOOKAHEAD_SAMPLES};
pub use midi_fx::{
    ArpPattern, Arpeggiator, Chord, NoteLength, Pitch, Random, RandomMode, Scale, Velocity,
};
pub use mixer::{Mixer, MixerTrack, NUM_TRACKS};
pub use multiband_dynamics::{BandDynamics, MultibandDynamics};
pub use reverb::{ConvolutionReverb, FdnReverb};
pub use roar::{Roar, RoarCurve, RoarStage, RoarTopology};
pub use saturator::Saturator;
pub use suite_fx::{
    AutoShift, BeatRepeat, ChorusEnsemble, ChorusMode, Corpus, CorpusResonatorType, Echo, Erosion,
    ErosionMode, HybridReverb, HybridReverbMode, PhaserFlanger, PhaserFlangerMode, Redux,
    SpectralResonator, Utility,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_fx_chain() {
        let sr = 44100.0;
        let saturator = Saturator::new();
        let mut delay = TempoDelay::new(sr, 128.0);
        let mut reverb = FdnReverb::new(sr);
        let mut limiter = MasterLimiter::new(sr);

        let mut left = vec![0.8f32; 256];
        let mut right = vec![0.8f32; 256];

        saturator.process_block(&mut left, &mut right);
        delay.process_block(&mut left, &mut right);
        reverb.process_block(&mut left, &mut right);
        limiter.process_block(&mut left, &mut right);

        for (l, r) in left.iter().zip(right.iter()) {
            assert!(l.is_finite());
            assert!(r.is_finite());
            assert!(l.abs() <= CEILING_LINEAR + 1e-5);
            assert!(r.abs() <= CEILING_LINEAR + 1e-5);
        }
    }
}
