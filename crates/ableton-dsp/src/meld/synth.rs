//! Meld: Live 12 Bi-Timbral Macro Synthesizer with Plate Resonator & Scale Quantizer.

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::cytomic::{CircuitModel, CytomicFilterType, CytomicZdfFilter};
use crate::envelope::AdsrEnvelope;
use crate::oscillator::{poly_blep, XorShift32};

pub const MAX_MELD_VOICES: usize = 8;
pub const NUM_PLATE_MODES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeldMacroEngine {
    Swarm,
    Fm,
    Harmonic,
    Noise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeldScale {
    Chromatic,
    Major,
    Minor,
    Dorian,
    Phrygian,
    Pentatonic,
}

impl MeldScale {
    /// Check whether a semitone degree (0..11) is in the scale.
    pub fn is_in_scale(&self, semitone: u8) -> bool {
        let deg = semitone % 12;
        match self {
            MeldScale::Chromatic => true,
            MeldScale::Major => matches!(deg, 0 | 2 | 4 | 5 | 7 | 9 | 11),
            MeldScale::Minor => matches!(deg, 0 | 2 | 3 | 5 | 7 | 8 | 10),
            MeldScale::Dorian => matches!(deg, 0 | 2 | 3 | 5 | 7 | 9 | 10),
            MeldScale::Phrygian => matches!(deg, 0 | 1 | 3 | 5 | 7 | 8 | 10),
            MeldScale::Pentatonic => matches!(deg, 0 | 2 | 4 | 7 | 9),
        }
    }

    /// Quantize a MIDI note number or float pitch to the nearest in-scale pitch.
    pub fn quantize_pitch(&self, pitch: f32) -> f32 {
        if *self == MeldScale::Chromatic {
            return pitch;
        }
        let round_note = pitch.round() as i32;
        let mut best_dist = 999;
        let mut best_note = round_note;

        for offset in 0..6 {
            for sign in [0, 1, -1] {
                let candidate = round_note + offset * sign;
                let semi = ((candidate % 12 + 12) % 12) as u8;
                if self.is_in_scale(semi) {
                    let dist = (candidate - round_note).abs();
                    if dist < best_dist {
                        best_dist = dist;
                        best_note = candidate;
                    }
                }
            }
            if best_dist < 999 {
                break;
            }
        }
        best_note as f32 + (pitch - pitch.round()) * 0.2 // Soft pull to scale
    }
}

/// 2nd order damped modal plate resonator stage.
#[derive(Debug, Clone)]
pub struct PlateResonatorStage {
    pub y1: f32,
    pub y2: f32,
}

impl PlateResonatorStage {
    pub fn new() -> Self {
        Self { y1: 0.0, y2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    /// Process input sample through damped harmonic oscillator.
    /// y[n] = 2 * r * cos(w) * y[n-1] - r^2 * y[n-2] + b * x[n]
    #[inline(always)]
    pub fn process(&mut self, input: f32, freq_hz: f32, decay_s: f32, sample_rate: f32) -> f32 {
        let w = (2.0 * PI * freq_hz.clamp(20.0, sample_rate * 0.45) / sample_rate).min(PI * 0.95);
        let gamma = 1.0 / (decay_s.max(0.01) * sample_rate);
        let r = (-gamma).exp();

        let a1 = 2.0 * r * w.cos();
        let a2 = r * r;
        let b0 = (1.0 - r) * (1.0 - a1 + a2).abs().sqrt().clamp(0.01, 1.0);

        let out = a1 * self.y1 - a2 * self.y2 + b0 * input;
        self.y2 = self.y1;
        self.y1 = out.clamp(-3.0, 3.0);
        out
    }
}

/// Thin metallic 2D plate resonator bank.
#[derive(Debug, Clone)]
pub struct PlateResonator {
    pub modes: [PlateResonatorStage; NUM_PLATE_MODES],
}

impl PlateResonator {
    pub fn new() -> Self {
        Self {
            modes: std::array::from_fn(|_| PlateResonatorStage::new()),
        }
    }

    pub fn reset(&mut self) {
        for m in &mut self.modes {
            m.reset();
        }
    }

    /// Process audio through 2D plate modal bank.
    pub fn process(&mut self, input: f32, base_freq: f32, decay: f32, damping: f32, sample_rate: f32) -> f32 {
        // 2D plate modal ratios roughly following Bessel modes m^2 + n^2
        let modal_ratios = [1.0, 1.58, 2.29, 2.91, 3.65, 4.38];
        let mut sum = 0.0f32;

        for (i, stage) in self.modes.iter_mut().enumerate() {
            let mode_freq = base_freq * modal_ratios[i];
            let mode_decay = decay * (1.0 / (1.0 + damping * (i as f32 * 0.5)));
            sum += stage.process(input, mode_freq, mode_decay, sample_rate) * (1.0 / (i + 1) as f32);
        }
        sum * 0.5
    }
}

#[derive(Debug, Clone)]
pub struct MeldVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Engine A & B phases
    pub phase_a: f32,
    pub phase_b: f32,
    pub swarm_phases_a: [f32; 4],
    pub swarm_phases_b: [f32; 4],

    // Envelopes
    pub env_a: AdsrEnvelope,
    pub env_b: AdsrEnvelope,

    // Filters and Plate Resonators
    pub filter_a: CytomicZdfFilter,
    pub filter_b: CytomicZdfFilter,
    pub plate_a: PlateResonator,
    pub plate_b: PlateResonator,

    // MPE per-voice expression
    pub pitch_bend_semitones: f32,
    pub slide: f32,    // CC74 [0.0, 1.0]
    pub pressure: f32, // Aftertouch [0.0, 1.0]

    // PRNG
    pub rng: XorShift32,
}

impl MeldVoice {
    pub fn new(sample_rate: f32, seed: u32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            phase_a: 0.0,
            phase_b: 0.0,
            swarm_phases_a: [0.0; 4],
            swarm_phases_b: [0.0; 4],
            pitch_bend_semitones: 0.0,
            slide: 0.0,
            pressure: 0.0,
            env_a: AdsrEnvelope::new(sample_rate),
            env_b: AdsrEnvelope::new(sample_rate),
            filter_a: CytomicZdfFilter::new(sample_rate),
            filter_b: CytomicZdfFilter::new(sample_rate),
            plate_a: PlateResonator::new(),
            plate_b: PlateResonator::new(),
            rng: XorShift32::new(seed),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.phase_a = 0.0;
        self.phase_b = 0.0;
        self.swarm_phases_a = [0.0; 4];
        self.swarm_phases_b = [0.0; 4];
        self.pitch_bend_semitones = 0.0;
        self.slide = 0.0;
        self.pressure = 0.0;
        self.env_a.reset();
        self.env_b.reset();
        self.filter_a.reset();
        self.filter_b.reset();
        self.plate_a.reset();
        self.plate_b.reset();
    }
}

/// Ableton Live 12 Meld Bi-Timbral Macro Synthesizer.
#[derive(Debug, Clone)]
pub struct MeldDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // Engine A
    pub engine_a_type: MeldMacroEngine,
    pub engine_a_param1: f32, // Swarm spread, FM ratio, Harmonic tilt, Noise freq
    pub engine_a_param2: f32, // Density / FM index / Even-Odd balance / Res
    pub engine_a_gain: f32,
    pub engine_a_pan: f32, // -1 .. 1

    // Engine B
    pub engine_b_type: MeldMacroEngine,
    pub engine_b_param1: f32,
    pub engine_b_param2: f32,
    pub engine_b_gain: f32,
    pub engine_b_pan: f32,

    // Scale Harmonic Quantizer
    pub scale: MeldScale,
    pub scale_quantize_active: bool,

    // Plate resonator vs filter toggle
    pub use_plate_a: bool,
    pub use_plate_b: bool,
    pub plate_decay: f32,
    pub plate_damping: f32,

    // Cross-modulation: Engine A output modulating Filter/Plate B
    pub cross_mod_amount: f32, // 0 .. 1

    // Filters A & B
    pub filter_a_cutoff: f32,
    pub filter_a_res: f32,
    pub filter_b_cutoff: f32,
    pub filter_b_res: f32,

    // Envelopes
    pub env_a_adsr: (f32, f32, f32, f32),
    pub env_b_adsr: (f32, f32, f32, f32),

    pub master_volume: f32,

    // Voices
    pub voices: [MeldVoice; MAX_MELD_VOICES],
    pub voice_counter: u64,
}

impl MeldDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let voices = std::array::from_fn(|i| MeldVoice::new(sr, 0x5a5a5a5a + (i as u32 * 0x1111)));

        Self {
            enabled: true,
            sample_rate: sr,
            engine_a_type: MeldMacroEngine::Swarm,
            engine_a_param1: 0.15, // Detune
            engine_a_param2: 0.5,
            engine_a_gain: 0.8,
            engine_a_pan: -0.3,
            engine_b_type: MeldMacroEngine::Harmonic,
            engine_b_param1: 0.0, // Tilt 0 dB/oct
            engine_b_param2: 0.5,
            engine_b_gain: 0.8,
            engine_b_pan: 0.3,
            scale: MeldScale::Dorian,
            scale_quantize_active: true,
            use_plate_a: false,
            use_plate_b: true,
            plate_decay: 0.6,
            plate_damping: 0.4,
            cross_mod_amount: 0.1,
            filter_a_cutoff: 3500.0,
            filter_a_res: 0.2,
            filter_b_cutoff: 4500.0,
            filter_b_res: 0.3,
            env_a_adsr: (5.0, 300.0, 0.7, 250.0),
            env_b_adsr: (10.0, 400.0, 0.6, 300.0),
            master_volume: 0.7,
            voices,
            voice_counter: 0,
        }
    }

    #[inline(always)]
    fn render_macro_engine(
        engine_type: MeldMacroEngine,
        p1: f32,
        p2: f32,
        base_freq: f32,
        main_phase: &mut f32,
        swarm_phases: &mut [f32; 4],
        rng: &mut XorShift32,
        sample_rate: f32,
    ) -> f32 {
        let inv_sr = 1.0 / sample_rate;
        let dt = base_freq * inv_sr;

        match engine_type {
            MeldMacroEngine::Swarm => {
                // 4-voice detuned supersaw cluster
                let spread = p1.clamp(0.0, 1.0) * 0.05;
                let detunes = [-1.5 * spread, -0.5 * spread, 0.5 * spread, 1.5 * spread];
                let mut sum = 0.0f32;

                for (i, &d) in detunes.iter().enumerate() {
                    let f = (base_freq * (1.0 + d)).clamp(10.0, sample_rate * 0.48);
                    let phase_step = f * inv_sr;
                    swarm_phases[i] = (swarm_phases[i] + phase_step).fract();
                    let mut saw = 2.0 * swarm_phases[i] - 1.0;
                    saw -= poly_blep(swarm_phases[i], phase_step);
                    sum += saw;
                }
                sum * 0.3
            }
            MeldMacroEngine::Fm => {
                // 2-Op FM cell
                let ratio = (p1 * 8.0).max(0.5).round();
                let index = p2.clamp(0.0, 1.0) * 4.0;
                let mod_dt = base_freq * ratio * inv_sr;
                *main_phase = (*main_phase + dt).fract();
                swarm_phases[0] = (swarm_phases[0] + mod_dt).fract();

                let mod_sig = (2.0 * PI * swarm_phases[0]).sin();
                let carrier_angle = 2.0 * PI * *main_phase + index * mod_sig;
                carrier_angle.sin() * 0.7
            }
            MeldMacroEngine::Harmonic => {
                // Additive harmonic series with tilt and odd/even balance
                *main_phase = (*main_phase + dt).fract();
                let angle = 2.0 * PI * *main_phase;
                let tilt = p1.clamp(-1.0, 1.0); // Spectral slope
                let balance = p2.clamp(0.0, 1.0); // Odd/even balance

                let mut sum = 0.0f32;
                for h in 1..=8 {
                    let is_even = h % 2 == 0;
                    let weight = if is_even { balance } else { 1.0 - balance };
                    let slope_decay = (h as f32).powf(1.0 + tilt);
                    let amp = (weight / slope_decay).clamp(0.0, 1.0);
                    sum += amp * (h as f32 * angle).sin();
                }
                sum * 0.5
            }
            MeldMacroEngine::Noise => {
                // Resonant filtered noise
                let white = rng.next_f32();
                *main_phase = (*main_phase + dt).fract();
                let formant = (2.0 * PI * *main_phase).sin();
                (white * 0.7 + formant * 0.3) * 0.6
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
            if v.env_a.value < min_val {
                min_val = v.env_a.value;
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
        v.env_a.trigger_attack();
        v.env_b.trigger_attack();
        v.filter_a.reset();
        v.filter_b.reset();
        v.plate_a.reset();
        v.plate_b.reset();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                v.env_a.trigger_release();
                v.env_b.trigger_release();
            }
        }
    }

    pub fn silence_all(&mut self) {
        for v in &mut self.voices {
            v.reset();
        }
    }

    /// Set MPE pitch bend in semitones for all active voices.
    pub fn set_pitch_bend(&mut self, semitones: f32) {
        for v in &mut self.voices {
            if v.active {
                v.pitch_bend_semitones = semitones;
            }
        }
    }

    /// Set MPE slide [0.0, 1.0] (CC74 timbre / Y-axis) for all active voices.
    pub fn set_mpe_slide(&mut self, slide: f32) {
        let s = slide.clamp(0.0, 1.0);
        for v in &mut self.voices {
            if v.active {
                v.slide = s;
            }
        }
    }

    /// Set MPE pressure [0.0, 1.0] (aftertouch / Z-axis) for all active voices.
    pub fn set_mpe_pressure(&mut self, pressure: f32) {
        let p = pressure.clamp(0.0, 1.0);
        for v in &mut self.voices {
            if v.active {
                v.pressure = p;
            }
        }
    }

    /// Process a stereo sample frame.
    #[inline(always)]
    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        let sr = self.sample_rate;
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let base_vol = self.master_volume;

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let env_a = voice.env_a.next(
                self.env_a_adsr.0,
                self.env_a_adsr.1,
                self.env_a_adsr.2,
                self.env_a_adsr.3,
            );
            let env_b = voice.env_b.next(
                self.env_b_adsr.0,
                self.env_b_adsr.1,
                self.env_b_adsr.2,
                self.env_b_adsr.3,
            );

            if !voice.env_a.is_active() && !voice.env_b.is_active() && env_a < 0.0001 && env_b < 0.0001 {
                voice.active = false;
                continue;
            }

            // Scale Harmonic Quantization with MPE pitch bend
            let raw_pitch = voice.note as f32 + voice.pitch_bend_semitones;
            let eff_pitch = if self.scale_quantize_active {
                self.scale.quantize_pitch(raw_pitch)
            } else {
                raw_pitch
            };

            let base_freq = (440.0 * 2.0f32.powf((eff_pitch - 69.0) / 12.0)).clamp(10.0, sr * 0.48);

            // Engine A synthesis
            let sig_a = Self::render_macro_engine(
                self.engine_a_type,
                self.engine_a_param1,
                self.engine_a_param2,
                base_freq,
                &mut voice.phase_a,
                &mut voice.swarm_phases_a,
                &mut voice.rng,
                sr,
            ) * self.engine_a_gain;

            // Engine B synthesis
            let sig_b = Self::render_macro_engine(
                self.engine_b_type,
                self.engine_b_param1,
                self.engine_b_param2,
                base_freq,
                &mut voice.phase_b,
                &mut voice.swarm_phases_b,
                &mut voice.rng,
                sr,
            ) * self.engine_b_gain;

            // Filter A or Plate A with MPE slide & pressure modulation
            let mod_a_cutoff = (self.filter_a_cutoff + voice.slide * 2000.0).clamp(20.0, 20000.0);
            let mod_a_res = (self.filter_a_res + voice.pressure * 0.3).clamp(0.0, 1.0);
            let out_proc_a = if self.use_plate_a {
                let eff_decay = (self.plate_decay + voice.slide * 0.2).clamp(0.05, 0.98);
                let eff_damping = (self.plate_damping + voice.pressure * 0.2).clamp(0.0, 0.95);
                voice.plate_a.process(sig_a, base_freq, eff_decay, eff_damping, sr)
            } else {
                voice.filter_a.filter_type = CytomicFilterType::Lowpass24;
                voice.filter_a.circuit_model = CircuitModel::Clean;
                voice.filter_a.process_sample(sig_a, mod_a_cutoff, mod_a_res, 0.0, 0.0)
            };

            // Cross-Modulation: Engine A modulating Filter B cutoff
            let cross_mod_offset = out_proc_a * self.cross_mod_amount * 1000.0;
            let mod_b_cutoff = (self.filter_b_cutoff + cross_mod_offset + voice.slide * 2000.0).clamp(20.0, 20000.0);
            let mod_b_res = (self.filter_b_res + voice.pressure * 0.3).clamp(0.0, 1.0);

            // Filter B or Plate B
            let out_proc_b = if self.use_plate_b {
                let eff_decay = (self.plate_decay + voice.slide * 0.2).clamp(0.05, 0.98);
                let eff_damping = (self.plate_damping + voice.pressure * 0.2).clamp(0.0, 0.95);
                voice.plate_b.process(sig_b, base_freq, eff_decay, eff_damping, sr)
            } else {
                voice.filter_b.filter_type = CytomicFilterType::Lowpass24;
                voice.filter_b.circuit_model = CircuitModel::MS2;
                voice.filter_b.process_sample(sig_b, mod_b_cutoff, mod_b_res, 0.0, 0.0)
            };

            // Panning & summing
            let pan_a = (self.engine_a_pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;
            let pan_b = (self.engine_b_pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;

            let voice_a = out_proc_a * env_a * voice.velocity * base_vol;
            let voice_b = out_proc_b * env_b * voice.velocity * base_vol;

            out_l += voice_a * pan_a.cos() + voice_b * pan_b.cos();
            out_r += voice_a * pan_a.sin() + voice_b * pan_b.sin();
        }

        (out_l, out_r)
    }
}

impl Device for MeldDevice {
    fn name(&self) -> &str {
        "Meld"
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

impl InstrumentDevice for MeldDevice {
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
                    let semitones = (value as f32 / 8192.0) * 12.0;
                    self.set_pitch_bend(semitones);
                }
                MidiEvent::ControlChange { controller, value, .. } => {
                    if controller == 74 {
                        // MPE Slide (CC74)
                        self.set_mpe_slide(value as f32 / 127.0);
                    } else if controller == 11 {
                        // MPE Pressure / Expression (CC11)
                        self.set_mpe_pressure(value as f32 / 127.0);
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
    fn test_meld_rendering_and_plate_resonator() {
        let mut meld = MeldDevice::new(44100.0);
        meld.trigger_note_on(64, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        meld.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Meld must generate audible sound");
    }

    #[test]
    fn test_meld_scale_quantizer() {
        let scale = MeldScale::Major; // C, D, E, F, G, A, B
        // Note 66 is F# (not in C Major); should pull to 65 (F) or 67 (G)
        let q = scale.quantize_pitch(66.0);
        let semi = ((q.round() as i32 % 12 + 12) % 12) as u8;
        assert!(scale.is_in_scale(semi), "Quantized pitch must be in scale");
    }

    #[test]
    fn test_meld_mpe_voice_expression() {
        let mut meld = MeldDevice::new(44100.0);
        meld.trigger_note_on(60, 100);

        // Verify active voice initialized with default MPE values
        assert_eq!(meld.voices[0].pitch_bend_semitones, 0.0);
        assert_eq!(meld.voices[0].slide, 0.0);
        assert_eq!(meld.voices[0].pressure, 0.0);

        // Apply MPE Pitch Bend (+2 semitones)
        meld.set_pitch_bend(2.0);
        assert_eq!(meld.voices[0].pitch_bend_semitones, 2.0);

        // Apply MPE Slide (CC74 = 0.75)
        meld.set_mpe_slide(0.75);
        assert_eq!(meld.voices[0].slide, 0.75);

        // Apply MPE Pressure (0.8)
        meld.set_mpe_pressure(0.8);
        assert_eq!(meld.voices[0].pressure, 0.8);

        // Process audio with MPE active and verify finite audible sound
        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        meld.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Meld with MPE modulation must generate audible sound");
    }
}
