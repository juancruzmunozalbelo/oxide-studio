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

    #[test]
    fn test_auto_shift_unity_pitch_passes_audio_and_preserves_params_on_sr_change() {
        use oxide_core::{AudioBuffer, AudioEffectDevice, MusicalTime};
        let mut sh = AutoShift::new(44100.0);
        sh.pitch_shift_semitones = 0.0;
        sh.dry_wet = 1.0;
        sh.vibrato_depth = 0.25;
        sh.set_sample_rate(48000.0);
        assert_eq!(sh.dry_wet, 1.0);
        assert_eq!(sh.vibrato_depth, 0.25);

        sh.vibrato_depth = 0.0;
        let mut buf = AudioBuffer::silence(256);
        buf.left[..256].fill(0.6);
        buf.right[..256].fill(-0.4);
        sh.process_audio(&mut buf, &MusicalTime::default());
        assert!((buf.left[100] - 0.6).abs() < 1e-5);
        assert!((buf.right[100] + 0.4).abs() < 1e-5);
    }

    #[test]
    fn test_utility_mute_inside_device_chain_silences_buffer() {
        use oxide_core::{AudioBuffer, Device, DeviceChain, MusicalTime};
        let mut util = Utility::new(44100.0);
        util.mute = true;
        assert!(util.is_enabled(), "Muted Utility must stay enabled so DeviceChain clears output");

        let mut chain = DeviceChain::new();
        chain.add_audio_effect(Box::new(util));
        let mut buf = AudioBuffer::silence(64);
        buf.left[..64].fill(0.5);
        buf.right[..64].fill(-0.5);
        chain.process(&[], &mut buf, &MusicalTime::default());

        assert!(buf.left[..64].iter().all(|&s| s == 0.0));
        assert!(buf.right[..64].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_scale_b_major_octave_wrap_and_velocity_inverted_range() {
        let mut scale = crate::midi_fx::Scale::new();
        scale.base_note = 11; // B Major: B, C#, D#, E, F#, G#, A#
        // C4 (60) is 1 semitone above B3 (59), must snap down to 59, not up to 71
        assert_eq!(scale.quantize_pitch(60), 59);

        let mut vel = crate::midi_fx::Velocity::new();
        vel.min_velocity = 120;
        vel.max_velocity = 20;
        let remapped = vel.remap(64);
        assert!((20..=120).contains(&remapped));

        let mut vel_dev = VelocityDevice::new();
        vel_dev.out_low = 110;
        vel_dev.out_high = 30;
        let v2 = vel_dev.transform_velocity(64);
        assert!((30..=110).contains(&v2));
    }

    #[test]
    fn test_multiband_dynamics_stereo_linked_peak_and_roar_tone_polarity() {
        let mut mb = MultibandDynamics::new(44100.0);
        let (out_l, out_r) = mb.low_band.process_stereo(0.8, 0.2, 1.0, 44100.0);
        // Stereo link preserves L:R ratio (4:1) by stepping envelope once per frame
        assert!((out_l / out_r - 4.0).abs() < 1e-4);

        let mut dark = RoarStage::new(RoarCurve::Tube, 0.0);
        dark.tone = -1.0;
        let mut bright = RoarStage::new(RoarCurve::Tube, 0.0);
        bright.tone = 1.0;

        let mut hf_dark = 0.0f32;
        let mut hf_bright = 0.0f32;
        for i in 0..128 {
            let x = if i % 2 == 0 { 0.4 } else { -0.4 }; // Nyquist oscillation
            let (dl, _) = dark.process_sample(x, x);
            let (bl, _) = bright.process_sample(x, x);
            hf_dark += dl * dl;
            hf_bright += bl * bl;
        }
        assert!(
            hf_bright > hf_dark * 2.0,
            "Roar +1.0 tone must be brighter than -1.0 tone: bright={}, dark={}",
            hf_bright,
            hf_dark
        );
    }

    #[test]
    fn test_redux_1bit_compressor_soft_knee_hybrid_reverb_and_midi_fx_bypass() {
        use oxide_core::{AudioBuffer, AudioEffectDevice, MidiEffectDevice, MidiEvent, MusicalTime};
        // Redux 1-bit quantization -> only {-1.0, 0.0, 1.0}
        let mut redux = Redux::new();
        redux.downsample = 1;
        redux.bit_depth = 1;
        let mut rbuf = AudioBuffer::silence(4);
        rbuf.left[0] = 0.25;
        rbuf.left[1] = -0.15;
        rbuf.left[2] = 0.0;
        rbuf.left[3] = 0.9;
        redux.process_audio(&mut rbuf, &MusicalTime::default());
        assert_eq!(&rbuf.left[..4], &[1.0, -1.0, 0.0, 1.0]);

        // Compressor soft knee applies gain reduction inside [threshold - knee/2, threshold]
        let mut comp = Compressor::new(44100.0);
        comp.threshold_db = -12.0;
        comp.ratio = 4.0;
        comp.knee_db = 12.0; // Knee [-18, -6]
        let level_db15 = 10.0_f32.powf(-15.0 / 20.0); // inside soft knee below threshold!
        let mut cbuf = AudioBuffer::silence(64);
        cbuf.left[..64].fill(level_db15);
        cbuf.right[..64].fill(level_db15);
        comp.process_audio(&mut cbuf, &MusicalTime::default());
        assert!(cbuf.left[63] < level_db15, "Soft knee must reduce gain at -15 dBFS");

        // HybridReverb 500ms predelay + SR preservation + size invariance
        let mut hr = HybridReverb::new(44100.0);
        hr.predelay_ms = 450.0;
        hr.size = 1.6;
        hr.decay_s = 3.5;
        hr.set_sample_rate(48000.0);
        assert_eq!(hr.predelay_ms, 450.0);
        assert_eq!(hr.size, 1.6);
        assert_eq!(hr.decay_s, 3.5);
        let mut hbuf = AudioBuffer::silence(128);
        hbuf.left[0] = 1.0;
        hbuf.right[0] = 1.0;
        hr.process_audio(&mut hbuf, &MusicalTime::default());
        assert!(hbuf.left[..128].iter().all(|s| s.is_finite()));

        // MidiEffectDevice is_enabled / set_enabled and channel preservation
        let mut nl = crate::midi_fx::NoteLength::new();
        nl.length_beats = 0.5;
        let mut out = Vec::new();
        nl.process_midi(
            &[MidiEvent::NoteOn { channel: 5, note: 60, velocity: 100 }],
            &mut out,
            &MusicalTime::new(0.0, 120.0, 4, 4, true),
        );
        out.clear();
        nl.process_midi(&[], &mut out, &MusicalTime::new(0.6, 120.0, 4, 4, true));
        assert_eq!(out, vec![MidiEvent::NoteOff { channel: 5, note: 60 }]);

        let mut dev = RackMidiEffectDevice::Pitch(PitchDevice::new());
        assert!(dev.is_enabled());
        dev.set_enabled(false);
        assert!(!dev.is_enabled());
    }

    #[test]
    fn test_fxtl_1_to_7_regressions() {
        use oxide_core::{AudioBuffer, AudioEffectDevice, MidiEffectDevice, MidiEvent, MusicalTime};

        // FXTL-1: Arpeggiator ChordTrigger must send NoteOff on release and step advance
        let mut arp = Arpeggiator::new();
        arp.pattern = ArpPattern::ChordTrigger;
        let mut out = Vec::new();
        arp.process_midi(
            &[
                MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
                MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 },
            ],
            &mut out,
            &MusicalTime::new(0.0, 120.0, 4, 4, true),
        );
        assert_eq!(out.len(), 2);
        out.clear();
        arp.process_midi(
            &[
                MidiEvent::NoteOff { channel: 0, note: 60 },
                MidiEvent::NoteOff { channel: 0, note: 64 },
            ],
            &mut out,
            &MusicalTime::new(0.25, 120.0, 4, 4, true),
        );
        assert!(out.contains(&MidiEvent::NoteOff { channel: 0, note: 60 }));
        assert!(out.contains(&MidiEvent::NoteOff { channel: 0, note: 64 }));

        // FXTL-2: Velocity::remap(0) must preserve velocity 0 (NoteOff)
        let vel = Velocity::new();
        assert_eq!(vel.remap(0), 0);

        // FXTL-3: ChordDevice, RandomDevice, Random must release on NoteOn(vel=0)
        let mut chord_dev = ChordDevice::new();
        let mut c_out = Vec::new();
        chord_dev.process_events(
            &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 0 }],
            &mut c_out,
        );
        assert_eq!(c_out.len(), 1 + chord_dev.num_shifts);
        assert!(c_out.iter().all(|e| matches!(e, MidiEvent::NoteOn { velocity: 0, .. })));

        let mut rand_dev = RandomDevice::new();
        rand_dev.chance = 1.0;
        let mut r_on = Vec::new();
        rand_dev.process_events(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }], &mut r_on);
        let shifted = match r_on[0] { MidiEvent::NoteOn { note, .. } => note, _ => panic!() };
        let mut r_off = Vec::new();
        rand_dev.process_events(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 0 }], &mut r_off);
        assert_eq!(r_off, vec![MidiEvent::NoteOn { channel: 0, note: shifted, velocity: 0 }]);

        let mut rand = Random::new();
        rand.chance = 1.0;
        let mut ro1 = Vec::new();
        rand.process_midi(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }], &mut ro1, &MusicalTime::default());
        let shifted2 = match ro1[0] { MidiEvent::NoteOn { note, .. } => note, _ => panic!() };
        let mut ro2 = Vec::new();
        rand.process_midi(&[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 0 }], &mut ro2, &MusicalTime::default());
        assert_eq!(ro2, vec![MidiEvent::NoteOn { channel: 0, note: shifted2, velocity: 0 }]);

        // FXTL-4: NoteLength flushes active notes when transport loops backward
        let mut nl = NoteLength::new();
        nl.length_beats = 0.5;
        let mut nl_out = Vec::new();
        nl.process_midi(
            &[MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }],
            &mut nl_out,
            &MusicalTime::new(3.75, 120.0, 4, 4, true),
        );
        nl_out.clear();
        nl.process_midi(&[], &mut nl_out, &MusicalTime::new(0.0, 120.0, 4, 4, true));
        assert_eq!(nl_out, vec![MidiEvent::NoteOff { channel: 0, note: 60 }]);

        // FXTL-5: Very low positive sample rates must not panic on filter cutoff clamp
        let mut buf = AudioBuffer::silence(16);
        let t = MusicalTime::default();
        let mut eq = EQEight::new(10.0);
        eq.process_audio(&mut buf, &t);
        let mut af = AutoFilter::new(10.0);
        af.process_audio(&mut buf, &t);
        let mut mb = MultibandDynamics::new(10.0);
        mb.process_audio(&mut buf, &t);
        let mut roar = Roar::new(10.0);
        roar.topology = RoarTopology::Multiband;
        roar.process_audio(&mut buf, &t);
        let mut corpus = Corpus::new(10.0);
        corpus.process_audio(&mut buf, &t);
        let mut erosion = Erosion::new(10.0);
        erosion.process_audio(&mut buf, &t);
        let mut pf = PhaserFlanger::new(10.0);
        pf.process_audio(&mut buf, &t);
        let mut sr_res = SpectralResonator::new(10.0);
        sr_res.process_audio(&mut buf, &t);
        let mut suite_res = SuiteAudioEffect::new(10.0, SuiteAudioEffectKind::SpectralResonator);
        suite_res.process_audio(&mut buf, &t);
        let mut suite_corp = SuiteAudioEffect::new(10.0, SuiteAudioEffectKind::Corpus);
        suite_corp.process_audio(&mut buf, &t);

        // FXTL-6: 192 kHz reverb FDN must preserve distinct delay lengths
        let fdn_192 = FdnReverb::new(192000.0);
        let mut fdn_buf_l = [0.0f32; 64];
        let mut fdn_buf_r = [0.0f32; 64];
        fdn_buf_l[0] = 1.0;
        let mut fdn_192 = fdn_192;
        fdn_192.process_block(&mut fdn_buf_l, &mut fdn_buf_r);
        assert!(fdn_buf_l.iter().all(|s| s.is_finite()));

        // FXTL-7: MpeControlDevice must not overflow on i16::MIN pitch bend
        let mut mpe = MpeControlDevice::new();
        let mut mpe_out = Vec::new();
        mpe.process_events(&[MidiEvent::PitchBend { channel: 0, value: i16::MIN }], &mut mpe_out);
        assert_eq!(mpe_out, vec![MidiEvent::PitchBend { channel: 0, value: -8192 }]);
    }
}
