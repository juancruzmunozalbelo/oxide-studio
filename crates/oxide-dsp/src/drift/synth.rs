//! Drift: Live 12 Subtractive Synthesizer with Analog Drift Instability & MPE.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::cytomic::{CircuitModel, CytomicFilterType, CytomicZdfFilter};
use crate::envelope::AdsrEnvelope;
use crate::oscillator::{poly_blep, XorShift32};

pub const MAX_DRIFT_VOICES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftOsc2Waveform {
    Sine,
    Triangle,
    Saw,
    Pulse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftVoiceMode {
    Poly,
    Mono,
    StereoUnison,
}

#[derive(Debug, Clone)]
pub struct DriftVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Phase accumulators
    pub osc1_phase: f32,
    pub osc2_phase: f32,

    // MPE per-voice expression
    pub pitch_bend_semitones: f32,
    pub slide: f32,    // CC74 [0.0, 1.0]
    pub pressure: f32, // Aftertouch [0.0, 1.0]

    // Ornstein-Uhlenbeck thermal drift state
    pub drift_pitch_cents: f32,
    pub drift_cutoff_hz: f32,
    pub rng: XorShift32,

    // Envelopes
    pub amp_env: AdsrEnvelope,
    pub env2: AdsrEnvelope,

    // Cycling envelope phase & state
    pub cycle_phase: f32,

    // Dual filters: Lowpass and Highpass
    pub lp_filter: CytomicZdfFilter,
    pub hp_filter: CytomicZdfFilter,
}

impl DriftVoice {
    pub fn new(sample_rate: f32, seed: u32) -> Self {
        let mut hp = CytomicZdfFilter::new(sample_rate);
        hp.filter_type = CytomicFilterType::Highpass12;
        let lp = CytomicZdfFilter::new(sample_rate);

        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            osc1_phase: 0.0,
            osc2_phase: 0.0,
            pitch_bend_semitones: 0.0,
            slide: 0.0,
            pressure: 0.0,
            drift_pitch_cents: 0.0,
            drift_cutoff_hz: 0.0,
            rng: XorShift32::new(seed),
            amp_env: AdsrEnvelope::new(sample_rate),
            env2: AdsrEnvelope::new(sample_rate),
            cycle_phase: 0.0,
            lp_filter: lp,
            hp_filter: hp,
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.osc1_phase = 0.0;
        self.osc2_phase = 0.0;
        self.pitch_bend_semitones = 0.0;
        self.slide = 0.0;
        self.pressure = 0.0;
        self.drift_pitch_cents = 0.0;
        self.drift_cutoff_hz = 0.0;
        self.amp_env.reset();
        self.env2.reset();
        self.cycle_phase = 0.0;
        self.lp_filter.reset();
        self.hp_filter.reset();
    }

    /// Step the Ornstein-Uhlenbeck stochastic process for thermal drift.
    #[inline(always)]
    pub fn step_drift(&mut self, drift_intensity: f32, dt: f32) {
        let theta = 1.5; // Mean reversion rate
        let sigma = 12.0 * drift_intensity; // Noise amplitude

        let d_w = (self.rng.next_f32() - 0.5) * 2.0;
        let d_drift = -theta * self.drift_pitch_cents * dt + sigma * d_w * dt.sqrt();
        self.drift_pitch_cents = (self.drift_pitch_cents + d_drift).clamp(-25.0, 25.0);

        let d_cut = -theta * self.drift_cutoff_hz * dt + (sigma * 4.0) * d_w * dt.sqrt();
        self.drift_cutoff_hz = (self.drift_cutoff_hz + d_cut).clamp(-100.0, 100.0);
    }
}

/// Ableton Live 12 Drift Synthesizer.
#[derive(Debug, Clone)]
pub struct DriftDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // Osc 1: Continuous shape morph (Sine -> Tri -> Saw -> Pulse)
    pub osc1_shape: f32, // 0.0 .. 1.0
    pub osc1_octave: i8, // -2 .. +2
    pub osc1_gain: f32,  // 0.0 .. 1.0

    // Osc 2: Selectable waveform + transpose + cross mod
    pub osc2_waveform: DriftOsc2Waveform,
    pub osc2_octave: i8, // -2 .. +2
    pub osc2_detune_cents: f32, // -50 .. +50
    pub osc2_gain: f32,
    pub osc2_cross_mod: f32, // 0.0 .. 1.0 (Freq mod from Osc 1)

    // Noise Generator
    pub noise_gain: f32,
    pub noise_gen: XorShift32,

    // Drift amount
    pub drift_amount: f32, // 0.0 .. 1.0

    // Filters: Lowpass & Highpass
    pub lp_cutoff: f32, // 20 .. 20000 Hz
    pub lp_resonance: f32, // 0 .. 1
    pub hp_cutoff: f32, // 20 .. 20000 Hz
    pub filter_model: CircuitModel,

    // Envelopes
    pub amp_adsr: (f32, f32, f32, f32),
    pub env2_adsr: (f32, f32, f32, f32),
    pub env2_cycle_mode: bool, // Cycling envelope toggle
    pub env2_cycle_rate_hz: f32, // 0.05 .. 200 Hz

    // Modulation routings
    pub env2_to_cutoff: f32, // -1.0 .. 1.0
    pub env2_to_shape: f32,  // -1.0 .. 1.0

    // Voice mode
    pub voice_mode: DriftVoiceMode,
    pub master_volume: f32,

    // Voices
    pub voices: [DriftVoice; MAX_DRIFT_VOICES],
    pub voice_counter: u64,
}

impl DriftDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let voices = std::array::from_fn(|i| DriftVoice::new(sr, 0x13579bde + (i as u32 * 0x2468)));

        Self {
            enabled: true,
            sample_rate: sr,
            osc1_shape: 0.35, // Triangle/Saw warm blend
            osc1_octave: 0,
            osc1_gain: 0.8,
            osc2_waveform: DriftOsc2Waveform::Saw,
            osc2_octave: 0,
            osc2_detune_cents: 6.0,
            osc2_gain: 0.6,
            osc2_cross_mod: 0.0,
            noise_gain: 0.05,
            noise_gen: XorShift32::new(0xabcdef01),
            drift_amount: 0.4,
            lp_cutoff: 4000.0,
            lp_resonance: 0.25,
            hp_cutoff: 40.0,
            filter_model: CircuitModel::Clean,
            amp_adsr: (5.0, 200.0, 0.7, 200.0),
            env2_adsr: (10.0, 300.0, 0.2, 200.0),
            env2_cycle_mode: false,
            env2_cycle_rate_hz: 2.0,
            env2_to_cutoff: 0.4,
            env2_to_shape: 0.2,
            voice_mode: DriftVoiceMode::Poly,
            master_volume: 0.7,
            voices,
            voice_counter: 0,
        }
    }

    #[inline(always)]
    fn evaluate_shape(shape: f32, phase: f32, dt: f32) -> f32 {
        let p = phase.fract();
        let p_norm = if p < 0.0 { p + 1.0 } else { p };
        let s = shape.clamp(0.0, 1.0);

        // Sine (0.0) -> Tri (0.33) -> Saw (0.66) -> Pulse (1.0)
        let angle = 2.0 * PI * p_norm;
        if s < 0.333333 {
            let t = s / 0.333333;
            let sine = angle.sin();
            let tri = 2.0 * (2.0 * (p_norm - (p_norm + 0.5).floor()).abs()) - 1.0;
            (1.0 - t) * sine + t * tri
        } else if s < 0.666667 {
            let t = (s - 0.333333) / 0.333334;
            let tri = 2.0 * (2.0 * (p_norm - (p_norm + 0.5).floor()).abs()) - 1.0;
            let mut saw = 2.0 * p_norm - 1.0;
            saw -= poly_blep(p_norm, dt);
            (1.0 - t) * tri + t * saw
        } else {
            let t = (s - 0.666667) / 0.333333;
            let mut saw = 2.0 * p_norm - 1.0;
            saw -= poly_blep(p_norm, dt);
            let mut sq = if p_norm < 0.5 { 1.0 } else { -1.0 };
            sq += poly_blep(p_norm, dt);
            let wrap = (p_norm - 0.5 + 1.0).fract();
            sq -= poly_blep(wrap, dt);
            (1.0 - t) * saw + t * sq
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
        v.env2.trigger_attack();
        v.cycle_phase = 0.0;
        v.lp_filter.reset();
        v.hp_filter.reset();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                v.amp_env.trigger_release();
                v.env2.trigger_release();
            }
        }
    }

    pub fn silence_all(&mut self) {
        for v in &mut self.voices {
            v.reset();
        }
    }

    /// Process a stereo sample.
    #[inline(always)]
    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        let sr = self.sample_rate;
        let inv_sr = 1.0 / sr;

        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let base_vol = self.master_volume;
        let pan_angle = 0.25 * PI; // Center pan
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

            // Step Ornstein-Uhlenbeck drift
            voice.step_drift(self.drift_amount, inv_sr);

            // Envelope 2 or Cycling mode
            let env2_val = if self.env2_cycle_mode {
                voice.cycle_phase = (voice.cycle_phase + self.env2_cycle_rate_hz * inv_sr).fract();
                // Looping triangle/saw cycle
                2.0 * (voice.cycle_phase - 0.5).abs()
            } else {
                voice.env2.next(
                    self.env2_adsr.0,
                    self.env2_adsr.1,
                    self.env2_adsr.2,
                    self.env2_adsr.3,
                )
            };

            // Calculate frequencies with drift and MPE pitch bend
            let base_pitch = voice.note as f32 + voice.pitch_bend_semitones;
            let note_freq = 440.0 * 2.0f32.powf((base_pitch - 69.0) / 12.0);

            let osc1_semi = self.osc1_octave as f32 * 12.0 + (voice.drift_pitch_cents * 0.01);
            let osc1_freq = (note_freq * 2.0f32.powf(osc1_semi / 12.0)).clamp(10.0, sr * 0.48);
            let osc1_dt = osc1_freq * inv_sr;

            let osc2_semi = self.osc2_octave as f32 * 12.0 + self.osc2_detune_cents * 0.01 - (voice.drift_pitch_cents * 0.01);
            let osc2_freq = (note_freq * 2.0f32.powf(osc2_semi / 12.0)).clamp(10.0, sr * 0.48);
            let osc2_dt = osc2_freq * inv_sr;

            // Osc 1 synthesis with shape mod + slide
            let shape1 = (self.osc1_shape + env2_val * self.env2_to_shape + voice.slide * 0.3).clamp(0.0, 1.0);
            voice.osc1_phase = (voice.osc1_phase + osc1_dt).fract();
            let osc1_sample = Self::evaluate_shape(shape1, voice.osc1_phase, osc1_dt) * self.osc1_gain;

            // Osc 2 cross-mod from Osc 1
            let cross_mod_dt = osc2_dt * (1.0 + self.osc2_cross_mod * osc1_sample * 0.5);
            voice.osc2_phase = (voice.osc2_phase + cross_mod_dt).fract();
            let osc2_p = voice.osc2_phase;
            let osc2_raw = match self.osc2_waveform {
                DriftOsc2Waveform::Sine => (2.0 * PI * osc2_p).sin(),
                DriftOsc2Waveform::Triangle => 2.0 * (2.0 * (osc2_p - (osc2_p + 0.5).floor()).abs()) - 1.0,
                DriftOsc2Waveform::Saw => {
                    let mut s = 2.0 * osc2_p - 1.0;
                    s -= poly_blep(osc2_p, osc2_dt);
                    s
                }
                DriftOsc2Waveform::Pulse => {
                    let mut sq = if osc2_p < 0.5 { 1.0 } else { -1.0 };
                    sq += poly_blep(osc2_p, osc2_dt);
                    let wrap = (osc2_p - 0.5 + 1.0).fract();
                    sq -= poly_blep(wrap, osc2_dt);
                    sq
                }
            };
            let osc2_sample = osc2_raw * self.osc2_gain;

            // Noise generator
            let noise_sample = self.noise_gen.next_f32() * self.noise_gain;

            // Mixed oscillator bus
            let mixed = osc1_sample + osc2_sample + noise_sample;

            // Modulated Cutoff with thermal drift, envelope 2, and MPE slide/pressure
            let cut_octaves = env2_val * self.env2_to_cutoff * 4.0 + voice.slide * 2.0;
            let mod_cutoff = (self.lp_cutoff * 2.0f32.powf(cut_octaves) + voice.drift_cutoff_hz).clamp(20.0, 20000.0);
            let mod_res = (self.lp_resonance + voice.pressure * 0.3).clamp(0.0, 1.0);

            // Filter chain: Highpass -> Lowpass
            voice.hp_filter.circuit_model = self.filter_model;
            let hp_out = voice.hp_filter.process_sample(mixed, self.hp_cutoff, 0.1, 0.0, 0.0);

            voice.lp_filter.circuit_model = self.filter_model;
            let filtered = voice.lp_filter.process_sample(hp_out, mod_cutoff, mod_res, 0.0, 0.0);

            let voice_out = filtered * amp_val * voice.velocity * base_vol;
            out_l += voice_out * left_pan;
            out_r += voice_out * right_pan;
        }

        (out_l, out_r)
    }
}

impl Device for DriftDevice {
    fn name(&self) -> &str {
        "Drift"
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

impl InstrumentDevice for DriftDevice {
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
                MidiEvent::PitchBend { value, .. } => {
                    // 14-bit pitch bend mapped to +/- 2 semitones
                    let bend_semi = (value as f32 / 8192.0) * 2.0;
                    for v in &mut self.voices {
                        if v.active {
                            v.pitch_bend_semitones = bend_semi;
                        }
                    }
                }
                MidiEvent::ControlChange { controller, value, .. } => {
                    if controller == 74 { // MPE Slide
                        let slide_norm = value as f32 / 127.0;
                        for v in &mut self.voices {
                            if v.active {
                                v.slide = slide_norm;
                            }
                        }
                    }
                }
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
    fn test_drift_rendering_and_cycling_envelope() {
        let mut drift = DriftDevice::new(44100.0);
        drift.env2_cycle_mode = true;
        drift.trigger_note_on(57, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        drift.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Drift must render audible audio");
    }
}
