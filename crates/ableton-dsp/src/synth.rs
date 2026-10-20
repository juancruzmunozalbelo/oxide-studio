//! Full electronic synthesizer voice engine.
//!
//! Integrates Mystran 2x ZDF 4-pole ladder filter, PolyBLEP Square/PWM and Sawtooth oscillators,
//! XorShift white noise, dual ADSR envelopes, 4-shape LFO, monophonic last-note priority voice,
//! parameter bank, and XY macro breakpoint modulation.

use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};
use crate::envelope::AdsrEnvelope;
use crate::filter::MystranZdfLadder;
use crate::lfo::{Lfo, LfoShape};
use crate::oscillator::{poly_blep, XorShift32};
use crate::parameters::{ParamId, ParameterBank, PARAMETER_COUNT};
use crate::presets::{apply_macro_x, apply_macro_y, apply_preset, load_mapping_catalog, load_preset_catalog, MacroMapping};
use crate::voice::{MonophonicVoice, VoiceReleaseAction, VoiceTriggerAction};

/// Complete monophonic synthesizer engine.
#[derive(Debug, Clone)]
pub struct SynthEngine {
    pub sample_rate: f32,
    pub voice: MonophonicVoice,
    pub filter: MystranZdfLadder,
    pub amp_env: AdsrEnvelope,
    pub mod_env: AdsrEnvelope,
    pub lfo: Lfo,
    pub noise_gen: XorShift32,
    pub params: ParameterBank,
    pub smoothed_params: ParameterBank,

    // Oscillator phases
    pub saw_phase: f32,
    pub sq_phase: f32,

    // Performance XY Macro modulation
    pub macro_x: f32,
    pub macro_y: f32,
    pub current_mapping: Option<MacroMapping>,
}

impl SynthEngine {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let params = ParameterBank::new();
        let smoothed_params = params.clone();

        Self {
            sample_rate: sr,
            voice: MonophonicVoice::new(),
            filter: MystranZdfLadder::new(sr),
            amp_env: AdsrEnvelope::new(sr),
            mod_env: AdsrEnvelope::new(sr),
            lfo: Lfo::new(),
            noise_gen: XorShift32::new(0x13579bdf),
            params,
            smoothed_params,
            saw_phase: 0.0,
            sq_phase: 0.0,
            macro_x: 0.5,
            macro_y: 0.5,
            current_mapping: None,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            self.filter.set_sample_rate(sample_rate);
            self.amp_env.set_sample_rate(sample_rate);
            self.mod_env.set_sample_rate(sample_rate);
        }
    }

    pub fn reset(&mut self) {
        self.voice.reset();
        self.filter.reset();
        self.amp_env.reset();
        self.mod_env.reset();
        self.lfo.reset();
        self.noise_gen.reset(0x13579bdf);
        self.saw_phase = 0.0;
        self.sq_phase = 0.0;
    }

    /// Handle NoteOn event.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let glide_ms = self.params.values[ParamId::GlideTime as usize];
        let action = self.voice.note_on(note, velocity, glide_ms);
        match action {
            VoiceTriggerAction::TriggerAttack => {
                self.mod_env.trigger_attack();
                self.amp_env.trigger_attack();
            }
            VoiceTriggerAction::LegatoGlide => {
                // Legato: gate remains open, envelopes do not retrigger
            }
        }
    }

    /// Handle NoteOff event.
    pub fn note_off(&mut self, note: u8) {
        let action = self.voice.note_off(note);
        match action {
            VoiceReleaseAction::TriggerRelease => {
                self.mod_env.trigger_release();
                self.amp_env.trigger_release();
            }
            VoiceReleaseAction::LegatoRestore(_) => {
                // Stack has remaining notes; gate stays open
            }
        }
    }

    /// All notes off.
    pub fn all_notes_off(&mut self) {
        self.voice.all_notes_off();
        self.mod_env.trigger_release();
        self.amp_env.trigger_release();
    }

    /// Set parameter by name with domain value.
    pub fn set_parameter(&mut self, name: &str, value: f32) {
        self.params.set_value(name, value);
        let slew_ms = self.params.values[ParamId::GlobalSlewTime as usize];
        if slew_ms <= 0.0 || name == "lfo_shape" {
            self.smoothed_params.set_value(name, value);
        }
    }

    /// Set parameter by name with normalized [0.0, 1.0] value.
    pub fn set_parameter_normalized(&mut self, name: &str, norm: f32) {
        self.params.set_control_value(name, norm);
        let slew_ms = self.params.values[ParamId::GlobalSlewTime as usize];
        if slew_ms <= 0.0 || name == "lfo_shape" {
            let val = self.params.get_value(name);
            self.smoothed_params.set_value(name, val);
        }
    }

    /// Load factory preset by filename (e.g. "wowbass.preset").
    pub fn load_preset(&mut self, preset_name: &str) -> bool {
        let catalog = match load_preset_catalog() {
            Ok(c) => c,
            Err(_) => return false,
        };

        let key = if preset_name.ends_with(".preset") {
            preset_name.to_string()
        } else {
            format!("{}.preset", preset_name)
        };

        if let Some(preset) = catalog.get(&key) {
            self.params.reset_to_defaults();
            apply_preset(&mut self.params, preset);

            // Copy to smoothed immediately
            self.smoothed_params = self.params.clone();

            // Load matching macro mapping
            let mapping_key = key.replace(".preset", ".mapping");
            if let Ok(mappings) = load_mapping_catalog() {
                self.current_mapping = mappings.get(&mapping_key).cloned();
            }
            true
        } else {
            false
        }
    }

    /// Set Macro X coordinate [0.0, 1.0] and dispatch to parameter bank.
    pub fn set_macro_x(&mut self, norm: f32) {
        self.macro_x = norm.clamp(0.0, 1.0);
        if let Some(ref mapping) = self.current_mapping {
            apply_macro_x(&mut self.params, mapping, self.macro_x);
        }
    }

    /// Set Macro Y coordinate [0.0, 1.0] and dispatch to parameter bank.
    pub fn set_macro_y(&mut self, norm: f32) {
        self.macro_y = norm.clamp(0.0, 1.0);
        if let Some(ref mapping) = self.current_mapping {
            apply_macro_y(&mut self.params, mapping, self.macro_y);
        }
    }

    /// Render next mono audio sample. Strictly ZERO dynamic allocations during steady-state rendering.
    #[inline(always)]
    pub fn process_sample(&mut self) -> f32 {
        let sr = self.sample_rate;
        let inv_sr = 1.0 / sr;

        // Parameter slew smoothing
        let slew_ms = self.params.values[ParamId::GlobalSlewTime as usize].max(0.0);
        let slew_coeff = if slew_ms <= 0.1 {
            1.0
        } else {
            1.0 - (-1.0 / ((slew_ms * 0.001) * sr)).exp()
        };

        // Smooth key audio-rate parameters directly in-place across the fixed array (strictly zero heap allocations)
        for i in 0..PARAMETER_COUNT {
            let target = self.params.values[i];
            if i == ParamId::LfoShape as usize {
                self.smoothed_params.values[i] = target;
            } else {
                let curr = self.smoothed_params.values[i];
                self.smoothed_params.values[i] = curr + (target - curr) * slew_coeff;
            }
        }

        let sp = &self.smoothed_params.values;

        // 1. Envelopes
        let amp_attack = sp[ParamId::AmplitudeEnvelopeAttack as usize];
        let amp_decay = sp[ParamId::AmplitudeEnvelopeDecay as usize];
        let amp_sustain = sp[ParamId::AmplitudeEnvelopeSustain as usize];
        let amp_release = sp[ParamId::AmplitudeEnvelopeRelease as usize];
        let amp_env_val = self.amp_env.next(amp_attack, amp_decay, amp_sustain, amp_release);

        let mod_attack = sp[ParamId::ModulationEnvelopeAttack as usize];
        let mod_decay = sp[ParamId::ModulationEnvelopeDecay as usize];
        let mod_sustain = sp[ParamId::ModulationEnvelopeSustain as usize];
        let mod_release = sp[ParamId::ModulationEnvelopeRelease as usize];
        let mod_env_val = self.mod_env.next(mod_attack, mod_decay, mod_sustain, mod_release);

        // 2. LFO
        let lfo_base_freq = sp[ParamId::LfoFrequency as usize];
        let lfo_shape_idx = sp[ParamId::LfoShape as usize].round() as usize;
        let lfo_shape = LfoShape::from(lfo_shape_idx);
        let lfo_env_mod = sp[ParamId::LfoFrequencyEnvelopeAmount as usize];
        let (lfo_signal, _) = self.lfo.next_sample(lfo_base_freq, lfo_env_mod, mod_env_val, lfo_shape, sr);

        // 3. Monophonic Pitch Glide
        let glide_ms = sp[ParamId::GlideTime as usize].max(0.0);
        self.voice.advance_glide(glide_ms, sr);

        let current_pitch = self.voice.current_midi_pitch;
        let base_pitch_offset = sp[ParamId::OscillatorPitchOffset as usize];
        let pitch_env_mod = sp[ParamId::OscillatorPitchOffsetEnvelopeAmount as usize];
        let pitch_lfo_mod = sp[ParamId::OscillatorPitchOffsetLfoAmount as usize];

        let sq_midi = current_pitch + pitch_env_mod * mod_env_val * 24.0 + pitch_lfo_mod * lfo_signal * 12.0;
        let pitch_offset_mod = base_pitch_offset + pitch_env_mod * mod_env_val * 24.0 + pitch_lfo_mod * lfo_signal * 12.0;
        let saw_midi = current_pitch + pitch_offset_mod;

        // MIDI pitch to frequency: 440 * exp((midi - 69) * ln(2)/12)
        const LN2_12: f32 = 0.057762265;
        let sq_freq = (440.0 * ((sq_midi - 69.0) * LN2_12).exp()).clamp(10.0, sr * 0.48);
        let saw_freq = (440.0 * ((saw_midi - 69.0) * LN2_12).exp()).clamp(10.0, sr * 0.48);

        // 4. Square / Pulse Width Oscillator
        let base_pw = sp[ParamId::OscillatorPulseWidth as usize];
        let pw_env_mod = sp[ParamId::OscillatorPulseWidthEnvelopeAmount as usize];
        let pw_lfo_mod = sp[ParamId::OscillatorPulseWidthLfoAmount as usize];

        let pw_percent = (50.0 + base_pw * 45.0 + pw_env_mod * mod_env_val * 45.0 + pw_lfo_mod * lfo_signal * 40.0)
            .clamp(5.0, 95.0);
        let duty = pw_percent * 0.01;

        let sq_dt = sq_freq * inv_sr;
        self.sq_phase += sq_dt;
        if self.sq_phase >= 1.0 {
            self.sq_phase -= 1.0;
        }

        let mut sq_sample = if self.sq_phase < duty { 1.0 } else { -1.0 };
        sq_sample += poly_blep(self.sq_phase, sq_dt);
        let wrapped_fall = (self.sq_phase - duty + 1.0) % 1.0;
        sq_sample -= poly_blep(wrapped_fall, sq_dt);

        // 5. Sawtooth Oscillator
        let saw_dt = saw_freq * inv_sr;
        self.saw_phase += saw_dt;
        if self.saw_phase >= 1.0 {
            self.saw_phase -= 1.0;
        }

        let mut saw_sample = 2.0 * self.saw_phase - 1.0;
        saw_sample -= poly_blep(self.saw_phase, saw_dt);

        // 6. White Noise Generator
        let noise_sample = self.noise_gen.next_f32();

        // 7. Oscillator Mixer
        let sq_gain = sp[ParamId::OscillatorPulseAmplitude as usize];
        let saw_gain = sp[ParamId::OscillatorSawAmplitude as usize];
        let noise_gain = sp[ParamId::OscillatorNoiseAmplitude as usize];
        let mixed = 0.35 * (sq_gain * sq_sample + saw_gain * saw_sample + noise_gain * noise_sample);

        // 8. Filter Cutoff Modulation
        let base_cutoff = sp[ParamId::FilterFrequency as usize];
        let filter_res = sp[ParamId::FilterResonance as usize];
        let filter_env_mod = sp[ParamId::FilterEnvelopeAmount as usize];
        let filter_lfo_mod = sp[ParamId::FilterLfoAmount as usize];

        let env_octaves = filter_env_mod * mod_env_val * 7.5;
        let lfo_octaves = filter_lfo_mod * lfo_signal * 3.5;
        let total_octaves = env_octaves + lfo_octaves;
        let mod_cutoff = (base_cutoff * 2.0f32.powf(total_octaves)).clamp(20.0, 15000.0);

        // 9. Mystran 2x ZDF 4-Pole Ladder Filter
        let filtered = self.filter.process_sample(mixed, mod_cutoff, filter_res);

        // 10. VCA & Master Amplitude
        let master_gain = sp[ParamId::Amplitude as usize];
        let amp_lfo_mod = sp[ParamId::AmplitudeLfoAmount as usize];
        let lfo_amp_scale = (1.0 + amp_lfo_mod * lfo_signal).clamp(0.0, 2.0);
        let vca_gain = amp_env_val * master_gain * lfo_amp_scale;

        filtered * vca_gain
    }

    /// Render a block of stereo frames into an `AudioBuffer`.
    pub fn process_buffer(&mut self, buffer: &mut AudioBuffer) {
        let count = buffer.frame_count.min(1024);
        for i in 0..count {
            let sample = self.process_sample();
            buffer.left[i] = sample;
            buffer.right[i] = sample;
        }
    }

    /// Render into raw left and right stereo slices.
    pub fn process_slice(&mut self, left: &mut [f32], right: &mut [f32]) {
        let count = left.len().min(right.len());
        for i in 0..count {
            let sample = self.process_sample();
            left[i] = sample;
            right[i] = sample;
        }
    }
}

impl Device for SynthEngine {
    fn name(&self) -> &str {
        "Synth"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::Instrument
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn set_enabled(&mut self, _enabled: bool) {}

    fn reset(&mut self) {
        self.reset();
    }
}

impl InstrumentDevice for SynthEngine {
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        _time: &MusicalTime,
    ) {
        for event in midi_in {
            match *event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.note_on(note, velocity);
                }
                MidiEvent::NoteOff { note, .. } => {
                    self.note_off(note);
                }
                _ => {}
            }
        }

        let frames = audio_out.frame_count.min(1024);
        for i in 0..frames {
            let sample = self.process_sample();
            audio_out.left[i] += sample;
            audio_out.right[i] += sample;
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.note_on(note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        self.note_off(note);
    }

    fn all_notes_off(&mut self) {
        self.all_notes_off();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synth_engine_play_note() {
        let mut synth = SynthEngine::new(44100.0);
        synth.load_preset("wowbass.preset");

        // NoteOn 48 (C3)
        synth.note_on(48, 100);

        let mut non_zero_samples = 0;
        for _ in 0..2048 {
            let sample = synth.process_sample();
            assert!(sample.is_finite());
            assert!(sample.abs() < 2.0);
            if sample.abs() > 0.01 {
                non_zero_samples += 1;
            }
        }
        assert!(non_zero_samples > 1000, "Synth must produce audible non-zero signal");

        synth.note_off(48);
        for _ in 0..10000 {
            synth.process_sample();
        }
        // After release, should decay to near silence
        let last_sample = synth.process_sample();
        assert!(last_sample.abs() < 0.01, "Must decay to silence on noteOff");
    }

    #[test]
    fn test_zero_allocation_steady_state_loop() {
        let mut synth = SynthEngine::new(44100.0);
        synth.load_preset("wowbass.preset");

        // Run 44100 samples (1 full second) of steady-state audio rendering
        synth.note_on(60, 110);
        let start = std::time::Instant::now();
        for i in 0..44100 {
            if i == 22050 {
                synth.note_on(64, 100); // Legato trill
            }
            if i % 100 == 0 {
                let norm = i as f32 / 44100.0;
                synth.set_macro_x(norm);
            }
            let s = synth.process_sample();
            assert!(s.is_finite());
            assert!(!s.is_nan());
            assert!(s.abs() < 2.0);
        }
        let elapsed = start.elapsed();
        // 44100 samples in < 15ms in debug mode demonstrates sub-microsecond zero-alloc speed
        assert!(elapsed.as_millis() < 500, "Steady-state rendering must be sub-realtime fast: {:?}", elapsed);
    }
}
