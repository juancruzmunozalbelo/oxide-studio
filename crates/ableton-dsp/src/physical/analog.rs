//! Analog: Virtual Analog Dual-Oscillator Dual-Filter Synthesizer.

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::cytomic::{CircuitModel, CytomicFilterType, DualCytomicFilter, FilterRouting};
use crate::envelope::AdsrEnvelope;
use crate::lfo::{Lfo, LfoShape};
use crate::oscillator::{poly_blep, XorShift32};

pub const MAX_ANALOG_VOICES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalogWaveform {
    Sine,
    Saw,
    Pulse,
}

#[derive(Debug, Clone)]
pub struct AnalogVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    pub osc1_phase: f32,
    pub osc2_phase: f32,
    pub sub_phase: f32,

    pub amp_env: AdsrEnvelope,
    pub filter_env: AdsrEnvelope,
    pub dual_filter: DualCytomicFilter,
}

impl AnalogVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            osc1_phase: 0.0,
            osc2_phase: 0.0,
            sub_phase: 0.0,
            amp_env: AdsrEnvelope::new(sample_rate),
            filter_env: AdsrEnvelope::new(sample_rate),
            dual_filter: DualCytomicFilter::new(sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.osc1_phase = 0.0;
        self.osc2_phase = 0.0;
        self.sub_phase = 0.0;
        self.amp_env.reset();
        self.filter_env.reset();
        self.dual_filter.reset();
    }
}

/// Ableton Live 12 Analog Virtual Analog Synthesizer.
#[derive(Debug, Clone)]
pub struct AnalogDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // Osc 1
    pub osc1_waveform: AnalogWaveform,
    pub osc1_octave: i8, // -2 .. +2
    pub osc1_detune_cents: f32,
    pub osc1_pwm: f32, // 0.05 .. 0.95
    pub osc1_gain: f32,

    // Osc 2
    pub osc2_waveform: AnalogWaveform,
    pub osc2_octave: i8,
    pub osc2_detune_cents: f32,
    pub osc2_pwm: f32,
    pub osc2_gain: f32,

    // Sub & Noise
    pub sub_gain: f32,
    pub noise_gain: f32,
    pub noise_gen: XorShift32,

    // Dual Filters
    pub filter_routing: FilterRouting,
    pub filter1_cutoff: f32,
    pub filter1_res: f32,
    pub filter1_env_amt: f32, // -1 .. +1
    pub filter2_cutoff: f32,
    pub filter2_res: f32,

    // Envelopes
    pub amp_adsr: (f32, f32, f32, f32),
    pub filter_adsr: (f32, f32, f32, f32),

    // LFOs
    pub lfo1: Lfo,
    pub lfo1_freq: f32,
    pub lfo1_shape: LfoShape,
    pub lfo1_to_pitch: f32,
    pub lfo1_to_filter: f32,

    pub master_volume: f32,
    pub master_pan: f32,

    // Voices
    pub voices: [AnalogVoice; MAX_ANALOG_VOICES],
    pub voice_counter: u64,
}

impl AnalogDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let voices = std::array::from_fn(|_| AnalogVoice::new(sr));

        Self {
            enabled: true,
            sample_rate: sr,
            osc1_waveform: AnalogWaveform::Saw,
            osc1_octave: 0,
            osc1_detune_cents: 0.0,
            osc1_pwm: 0.5,
            osc1_gain: 0.8,
            osc2_waveform: AnalogWaveform::Pulse,
            osc2_octave: 0,
            osc2_detune_cents: 8.0,
            osc2_pwm: 0.4,
            osc2_gain: 0.6,
            sub_gain: 0.25,
            noise_gain: 0.05,
            noise_gen: XorShift32::new(0xdeadbeef),
            filter_routing: FilterRouting::Series,
            filter1_cutoff: 2800.0,
            filter1_res: 0.25,
            filter1_env_amt: 0.4,
            filter2_cutoff: 6000.0,
            filter2_res: 0.1,
            amp_adsr: (5.0, 200.0, 0.7, 200.0),
            filter_adsr: (10.0, 300.0, 0.2, 200.0),
            lfo1: Lfo::new(),
            lfo1_freq: 2.5,
            lfo1_shape: LfoShape::Triangle,
            lfo1_to_pitch: 0.0,
            lfo1_to_filter: 0.0,
            master_volume: 0.7,
            master_pan: 0.0,
            voices,
            voice_counter: 0,
        }
    }

    #[inline(always)]
    fn render_osc(waveform: AnalogWaveform, phase: f32, dt: f32, pwm: f32) -> f32 {
        let p = phase.fract();
        let p_norm = if p < 0.0 { p + 1.0 } else { p };

        match waveform {
            AnalogWaveform::Sine => (2.0 * PI * p_norm).sin(),
            AnalogWaveform::Saw => {
                let mut saw = 2.0 * p_norm - 1.0;
                saw -= poly_blep(p_norm, dt);
                saw
            }
            AnalogWaveform::Pulse => {
                let duty = pwm.clamp(0.05, 0.95);
                let mut sq = if p_norm < duty { 1.0 } else { -1.0 };
                sq += poly_blep(p_norm, dt);
                let wrap = (p_norm - duty + 1.0).fract();
                sq -= poly_blep(wrap, dt);
                sq
            }
        }
    }

    fn allocate_voice(&mut self) -> usize {
        for (i, v) in self.voices.iter().enumerate() {
            if !v.active {
                return i;
            }
        }
        let mut min_val = f32::MAX;
        let mut min_idx = 0;
        for (i, v) in self.voices.iter().enumerate() {
            if v.amp_env.value < min_val {
                min_val = v.amp_env.value;
                min_idx = i;
            }
        }
        min_idx
    }

    pub fn trigger_note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            self.trigger_note_off(note);
            return;
        }

        let idx = self.allocate_voice();
        self.voice_counter += 1;
        let v = &mut self.voices[idx];
        v.active = true;
        v.note = note;
        v.velocity = velocity as f32 / 127.0;
        v.age = self.voice_counter;
        v.amp_env.trigger_attack();
        v.filter_env.trigger_attack();
        v.dual_filter.reset();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                v.amp_env.trigger_release();
                v.filter_env.trigger_release();
            }
        }
    }

    pub fn silence_all(&mut self) {
        for v in &mut self.voices {
            v.reset();
        }
    }

    /// Process a single stereo sample frame.
    #[inline(always)]
    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        let sr = self.sample_rate;
        let inv_sr = 1.0 / sr;

        let (lfo1_sig, _) = self.lfo1.next_sample(self.lfo1_freq, 0.0, 0.0, self.lfo1_shape, sr);

        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let base_vol = self.master_volume;
        let pan_angle = (self.master_pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;
        let left_pan = pan_angle.cos();
        let right_pan = pan_angle.sin();

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let amp_val = voice.amp_env.next(
                self.amp_adsr.0,
                self.amp_adsr.1,
                self.amp_adsr.2,
                self.amp_adsr.3,
            );

            if !voice.amp_env.is_active() && amp_val < 0.0001 {
                voice.active = false;
                continue;
            }

            let fenv_val = voice.filter_env.next(
                self.filter_adsr.0,
                self.filter_adsr.1,
                self.filter_adsr.2,
                self.filter_adsr.3,
            );

            let note_freq = 440.0 * 2.0f32.powf((voice.note as f32 - 69.0) / 12.0);

            // Osc 1
            let lfo_pitch_mod = lfo1_sig * self.lfo1_to_pitch * 2.0;
            let osc1_semi = self.osc1_octave as f32 * 12.0 + self.osc1_detune_cents * 0.01 + lfo_pitch_mod;
            let osc1_freq = (note_freq * 2.0f32.powf(osc1_semi / 12.0)).clamp(10.0, sr * 0.48);
            let osc1_dt = osc1_freq * inv_sr;
            voice.osc1_phase = (voice.osc1_phase + osc1_dt).fract();
            let sig1 = Self::render_osc(self.osc1_waveform, voice.osc1_phase, osc1_dt, self.osc1_pwm) * self.osc1_gain;

            // Osc 2
            let osc2_semi = self.osc2_octave as f32 * 12.0 + self.osc2_detune_cents * 0.01 + lfo_pitch_mod;
            let osc2_freq = (note_freq * 2.0f32.powf(osc2_semi / 12.0)).clamp(10.0, sr * 0.48);
            let osc2_dt = osc2_freq * inv_sr;
            voice.osc2_phase = (voice.osc2_phase + osc2_dt).fract();
            let sig2 = Self::render_osc(self.osc2_waveform, voice.osc2_phase, osc2_dt, self.osc2_pwm) * self.osc2_gain;

            // Sub & Noise
            let sub_freq = (note_freq * 0.5).clamp(10.0, sr * 0.48);
            voice.sub_phase = (voice.sub_phase + sub_freq * inv_sr).fract();
            let sub_sig = if voice.sub_phase < 0.5 { 1.0 } else { -1.0 } * self.sub_gain;
            let noise_sig = self.noise_gen.next_f32() * self.noise_gain;

            // Dual Filters
            voice.dual_filter.routing = self.filter_routing;
            voice.dual_filter.filter1.filter_type = CytomicFilterType::Lowpass24;
            voice.dual_filter.filter1.circuit_model = CircuitModel::PRD;
            voice.dual_filter.filter2.filter_type = CytomicFilterType::Lowpass12;
            voice.dual_filter.filter2.circuit_model = CircuitModel::Clean;

            let lfo_filter_mod = lfo1_sig * self.lfo1_to_filter * 2.0;
            let mod_cutoff1 = (self.filter1_cutoff * 2.0f32.powf(fenv_val * self.filter1_env_amt * 4.0 + lfo_filter_mod)).clamp(20.0, 20000.0);

            let filtered = voice.dual_filter.process(
                sig1 + sub_sig + noise_sig,
                sig2,
                mod_cutoff1,
                self.filter1_res,
                0.0,
                self.filter2_cutoff,
                self.filter2_res,
                0.0,
            );

            let voice_out = filtered * amp_val * voice.velocity * base_vol;
            out_l += voice_out * left_pan;
            out_r += voice_out * right_pan;
        }

        (out_l, out_r)
    }
}

impl Device for AnalogDevice {
    fn name(&self) -> &str {
        "Analog"
    }

    fn category(&self) -> DeviceCategory {
        DeviceCategory::Instrument
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn reset(&mut self) {
        self.silence_all();
    }
}

impl InstrumentDevice for AnalogDevice {
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        _time: &MusicalTime,
    ) {
        if !self.enabled {
            return;
        }

        for event in midi_in {
            match *event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.trigger_note_on(note, velocity);
                }
                MidiEvent::NoteOff { note, .. } => {
                    self.trigger_note_off(note);
                }
                _ => {}
            }
        }

        let frames = audio_out.frame_count.min(1024);
        for i in 0..frames {
            let (l, r) = self.process_stereo_sample();
            audio_out.left[i] += l;
            audio_out.right[i] += r;
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.trigger_note_on(note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        self.trigger_note_off(note);
    }

    fn all_notes_off(&mut self) {
        self.silence_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analog_synth_rendering() {
        let mut analog = AnalogDevice::new(44100.0);
        analog.trigger_note_on(48, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        analog.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Analog synth must render audible audio");
    }

    #[test]
    fn test_analog_lfo1_modulation_pitch_and_filter() {
        let mut analog_base = AnalogDevice::new(44100.0);
        analog_base.trigger_note_on(60, 100);

        let mut analog_mod = AnalogDevice::new(44100.0);
        analog_mod.lfo1_to_pitch = 1.0;
        analog_mod.lfo1_to_filter = 1.0;
        analog_mod.trigger_note_on(60, 100);

        let mut diff = 0.0f32;
        for _ in 0..256 {
            let (l1, _) = analog_base.process_stereo_sample();
            let (l2, _) = analog_mod.process_stereo_sample();
            diff += (l1 - l2).abs();
        }
        assert!(
            diff > 0.01,
            "LFO1 pitch and filter modulation must produce measurable audio change, diff={}",
            diff
        );
    }
}
