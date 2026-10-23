//! Effects and mixing engine for Oxide Studio.
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
pub mod midi_fx_rack;
pub mod mixer;
pub mod multiband_dynamics;
pub mod reverb;
pub mod roar;
pub mod saturator;
pub mod suite_audio_fx;
pub mod suite_fx;

pub use oxide_core::DeviceChain;
pub use auto_filter::{AutoFilter, AutoFilterLfoShape, CircuitModel, CytomicFilterType};
pub use compressor::{Compressor, CompressorMode};
pub use delay::{DelaySubdivision, TempoDelay, MAX_DELAY_SAMPLES};
pub use eq8::{EQEight, EqBand, EqBandType, EqStereoMode};
pub use glue_compressor::GlueCompressor;
pub use limiter::{MasterLimiter, CEILING_LINEAR, LOOKAHEAD_SAMPLES};
pub use midi_fx::{
    ArpPattern, Arpeggiator, Chord, NoteLength, Pitch, Random, RandomMode, Scale, Velocity,
};
pub use midi_fx_rack::{
    ArpRate, ArpStyle, ArpeggiatorDevice, ChordDevice, MidiEffectChain,
    MidiEffectDevice as RackMidiEffectDevice, MpeControlDevice, NoteLengthDevice, PitchDevice,
    RandomDevice, RandomSign, ScaleDevice, VelocityCurve, VelocityDevice,
};
pub use mixer::{Mixer, MixerTrack, NUM_TRACKS};
pub use multiband_dynamics::{BandDynamics, MultibandDynamics};
pub use reverb::{ConvolutionReverb, FdnReverb};
pub use roar::{Roar, RoarCurve, RoarStage, RoarTopology};
pub use saturator::Saturator;
pub use suite_audio_fx::{
    CorpusEffect, CorpusMode, EchoEffect, HybridReverbEffect, SpectralResonatorEffect,
    SpectralTimeEffect, SuiteAudioEffect, SuiteAudioEffectKind, TruePeakLimiterEffect,
    VocoderEffect,
};
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

    #[test]
    fn test_chapter22_instruments_and_effects_device_chain_routing_and_bypass() {
        use oxide_core::{AudioBuffer, DeviceCategory, MusicalTime};

        let sr = 44100.0;
        let mut chain = DeviceChain::new();

        let mut filter = AutoFilter::new(sr);
        filter.cutoff_hz = 2000.0;
        filter.resonance = 0.5;

        let mut glue = GlueCompressor::new(sr);
        glue.threshold_db = -15.0;
        glue.ratio = 4.0;

        chain.add_audio_effect(Box::new(filter));
        chain.add_audio_effect(Box::new(glue));

        assert_eq!(chain.audio_effect_count(), 2);
        assert_eq!(chain.device_count(), 2);

        let time = MusicalTime::default();
        let mut buf = AudioBuffer::silence(256);
        for i in 0..256 {
            buf.left[i] = 0.5;
            buf.right[i] = 0.5;
        }

        // 1. Process through chain
        chain.process(&[], &mut buf, &time);
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
        }

        // 2. Bypass testing: bypass both audio effects
        assert!(chain.set_bypass(DeviceCategory::AudioEffect, 0, true));
        assert!(chain.set_bypass(DeviceCategory::AudioEffect, 1, true));

        let mut dry_buf = AudioBuffer::silence(256);
        for i in 0..256 {
            dry_buf.left[i] = 0.75;
            dry_buf.right[i] = 0.75;
        }

        chain.process(&[], &mut dry_buf, &time);
        for i in 0..256 {
            assert_eq!(dry_buf.left[i], 0.75, "Bypassed device chain must be dry-thru");
            assert_eq!(dry_buf.right[i], 0.75, "Bypassed device chain must be dry-thru");
        }

        // 3. Re-enable effects
        assert!(chain.set_bypass(DeviceCategory::AudioEffect, 0, false));
        assert!(chain.set_bypass(DeviceCategory::AudioEffect, 1, false));

        chain.process(&[], &mut dry_buf, &time);
        let processed = dry_buf.left[100];
        assert_ne!(processed, 0.75);
    }
}
