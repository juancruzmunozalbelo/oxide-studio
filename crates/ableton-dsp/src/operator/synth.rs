//! Operator 4-operator FM & Additive Synthesizer.

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::cytomic::{CircuitModel, CytomicFilterType, CytomicZdfFilter};
use crate::envelope::AdsrEnvelope;
use crate::oscillator::poly_blep;

pub const MAX_OPERATOR_VOICES: usize = 8;
pub const NUM_OPERATORS: usize = 4; // 0=A, 1=B, 2=C, 3=D

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorWaveform {
    Sine,
    Saw,
    Square,
    UserHarmonics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorFreqMode {
    Ratio,
    Fixed,
}

#[derive(Debug, Clone)]
pub struct OperatorUnit {
    pub enabled: bool,
    pub waveform: OperatorWaveform,
    pub freq_mode: OperatorFreqMode,
    pub coarse: f32, // 0.5, 1, 2, ... 32
    pub fine: f32,   // -100 .. +100
    pub fixed_hz: f32, // 10 .. 20000 Hz
    pub level: f32,  // 0.0 .. 1.0 (or modulation index)
    pub user_harmonics: [f32; 16],
    pub adsr: (f32, f32, f32, f32), // ms, ms, norm, ms
}

impl Default for OperatorUnit {
    fn default() -> Self {
        let mut user_harmonics = [0.0f32; 16];
        user_harmonics[0] = 1.0;
        Self {
            enabled: true,
            waveform: OperatorWaveform::Sine,
            freq_mode: OperatorFreqMode::Ratio,
            coarse: 1.0,
            fine: 0.0,
            fixed_hz: 440.0,
            level: 0.8,
            user_harmonics,
            adsr: (5.0, 300.0, 0.7, 250.0),
        }
    }
}

/// The 11 classic FM algorithms of Operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmAlgorithm {
    /// Alg 1: D -> C -> B -> A -> Output
    Alg1,
    /// Alg 2: (D + C) -> B -> A -> Output
    Alg2,
    /// Alg 3: (D -> C + B) -> A -> Output
    Alg3,
    /// Alg 4: (D -> C) -> A -> Output and B -> A -> Output
    Alg4,
    /// Alg 5: (D -> C) -> Output and (B -> A) -> Output
    Alg5,
    /// Alg 6: (D + C + B) -> A -> Output
    Alg6,
    /// Alg 7: (D -> C) -> Output, B -> Output, A -> Output
    Alg7,
    /// Alg 8: (D -> C -> B) -> Output, A -> Output
    Alg8,
    /// Alg 9: (D -> C) -> B -> Output, A -> Output
    Alg9,
    /// Alg 10: D -> (C + B + A) -> Output
    Alg10,
    /// Alg 11: A + B + C + D -> Output (Additive)
    Alg11,
}

#[derive(Debug, Clone)]
pub struct OperatorVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Phase accumulators for A, B, C, D
    pub phases: [f32; NUM_OPERATORS],

    // Feedback state for Operator D
    pub fb_y1: f32,
    pub fb_y2: f32,

    // Envelopes per operator
    pub envs: [AdsrEnvelope; NUM_OPERATORS],

    // Pitch envelope
    pub pitch_env: AdsrEnvelope,

    // Master filter
    pub master_filter: CytomicZdfFilter,
}

impl OperatorVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            phases: [0.0; NUM_OPERATORS],
            fb_y1: 0.0,
            fb_y2: 0.0,
            envs: std::array::from_fn(|_| AdsrEnvelope::new(sample_rate)),
            pitch_env: AdsrEnvelope::new(sample_rate),
            master_filter: CytomicZdfFilter::new(sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.phases = [0.0; NUM_OPERATORS];
        self.fb_y1 = 0.0;
        self.fb_y2 = 0.0;
        for e in &mut self.envs {
            e.reset();
        }
        self.pitch_env.reset();
        self.master_filter.reset();
    }
}

/// Ableton Live 12 4-Operator FM Synthesizer.
#[derive(Debug, Clone)]
pub struct OperatorDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // 4 Operators: A (0), B (1), C (2), D (3)
    pub ops: [OperatorUnit; NUM_OPERATORS],

    // Algorithm & Feedback
    pub algorithm: FmAlgorithm,
    pub feedback_d: f32, // 0.0 .. 1.0

    // Pitch envelope settings
    pub pitch_env_enabled: bool,
    pub pitch_env_adsr: (f32, f32, f32, f32),
    pub pitch_env_semitones: f32, // -48.0 .. +48.0

    // Master filter
    pub filter_enabled: bool,
    pub filter_type: CytomicFilterType,
    pub filter_model: CircuitModel,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_drive: f32,

    // Master
    pub master_volume: f32,
    pub master_pan: f32,

    // Voices
    pub voices: [OperatorVoice; MAX_OPERATOR_VOICES],
    pub voice_counter: u64,
}

impl OperatorDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let ops: [OperatorUnit; NUM_OPERATORS] = std::array::from_fn(|i| {
            let mut unit = OperatorUnit::default();
            match i {
                0 => { // Op A (Carrier)
                    unit.level = 0.8;
                    unit.coarse = 1.0;
                }
                1 => { // Op B (Modulator)
                    unit.level = 0.5;
                    unit.coarse = 2.0;
                }
                2 => { // Op C
                    unit.level = 0.3;
                    unit.coarse = 3.0;
                }
                3 => { // Op D
                    unit.level = 0.2;
                    unit.coarse = 4.0;
                }
                _ => {}
            }
            unit
        });

        let voices = std::array::from_fn(|_| OperatorVoice::new(sr));

        Self {
            enabled: true,
            sample_rate: sr,
            ops,
            algorithm: FmAlgorithm::Alg1, // Cascade D->C->B->A
            feedback_d: 0.15,
            pitch_env_enabled: false,
            pitch_env_adsr: (1.0, 100.0, 0.0, 100.0),
            pitch_env_semitones: 12.0,
            filter_enabled: true,
            filter_type: CytomicFilterType::Lowpass24,
            filter_model: CircuitModel::Clean,
            filter_cutoff: 8000.0,
            filter_resonance: 0.15,
            filter_drive: 0.0,
            master_volume: 0.7,
            master_pan: 0.0,
            voices,
            voice_counter: 0,
        }
    }

    #[inline(always)]
    fn evaluate_waveform(waveform: OperatorWaveform, phase: f32, dt: f32, user_harmonics: &[f32; 16]) -> f32 {
        let p = phase.fract();
        let p_norm = if p < 0.0 { p + 1.0 } else { p };
        let angle = 2.0 * PI * p_norm;

        match waveform {
            OperatorWaveform::Sine => angle.sin(),
            OperatorWaveform::Saw => {
                let mut saw = 2.0 * p_norm - 1.0;
                saw -= poly_blep(p_norm, dt);
                saw
            }
            OperatorWaveform::Square => {
                let mut sq = if p_norm < 0.5 { 1.0 } else { -1.0 };
                sq += poly_blep(p_norm, dt);
                let wrapped = (p_norm - 0.5 + 1.0).fract();
                sq -= poly_blep(wrapped, dt);
                sq
            }
            OperatorWaveform::UserHarmonics => {
                let mut sum = 0.0f32;
                for (h, &amp) in user_harmonics.iter().enumerate() {
                    if amp > 0.001 {
                        sum += amp * ((h + 1) as f32 * angle).sin();
                    }
                }
                sum * 0.7
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
            if v.envs[0].value < min_val {
                min_val = v.envs[0].value;
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
        v.master_filter.reset();
        v.fb_y1 = 0.0;
        v.fb_y2 = 0.0;

        for e in &mut v.envs {
            e.trigger_attack();
        }
        if self.pitch_env_enabled {
            v.pitch_env.trigger_attack();
        }
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                for e in &mut v.envs {
                    e.trigger_release();
                }
                v.pitch_env.trigger_release();
            }
        }
    }

    pub fn silence_all(&mut self) {
        for v in &mut self.voices {
            v.reset();
        }
    }

    /// Process a stereo sample frame.
    #[inline(always)]
    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        let sr = self.sample_rate;
        let inv_sr = 1.0 / sr;

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

            // Check if carrier envelope is still audible
            let env_a = voice.envs[0].next(
                self.ops[0].adsr.0,
                self.ops[0].adsr.1,
                self.ops[0].adsr.2,
                self.ops[0].adsr.3,
            );

            if !voice.envs[0].is_active() && env_a < 0.0001 {
                voice.active = false;
                continue;
            }

            let env_b = voice.envs[1].next(
                self.ops[1].adsr.0,
                self.ops[1].adsr.1,
                self.ops[1].adsr.2,
                self.ops[1].adsr.3,
            );
            let env_c = voice.envs[2].next(
                self.ops[2].adsr.0,
                self.ops[2].adsr.1,
                self.ops[2].adsr.2,
                self.ops[2].adsr.3,
            );
            let env_d = voice.envs[3].next(
                self.ops[3].adsr.0,
                self.ops[3].adsr.1,
                self.ops[3].adsr.2,
                self.ops[3].adsr.3,
            );

            // Pitch calculation + pitch envelope
            let mut pitch_mod_semi = 0.0f32;
            if self.pitch_env_enabled {
                let penv_val = voice.pitch_env.next(
                    self.pitch_env_adsr.0,
                    self.pitch_env_adsr.1,
                    self.pitch_env_adsr.2,
                    self.pitch_env_adsr.3,
                );
                pitch_mod_semi = penv_val * self.pitch_env_semitones;
            }

            let note_pitch = voice.note as f32 + pitch_mod_semi;
            let note_freq = 440.0 * 2.0f32.powf((note_pitch - 69.0) / 12.0);

            // Calculate operator frequencies
            let mut freqs = [0.0f32; NUM_OPERATORS];
            let mut dts = [0.0f32; NUM_OPERATORS];
            for i in 0..NUM_OPERATORS {
                let op = &self.ops[i];
                let f = match op.freq_mode {
                    OperatorFreqMode::Ratio => {
                        let fine_ratio = 1.0 + 0.01 * op.fine;
                        (note_freq * op.coarse * fine_ratio).clamp(1.0, sr * 0.48)
                    }
                    OperatorFreqMode::Fixed => op.fixed_hz.clamp(1.0, sr * 0.48),
                };
                freqs[i] = f;
                dts[i] = f * inv_sr;
            }

            // Advance phases
            for i in 0..NUM_OPERATORS {
                voice.phases[i] = (voice.phases[i] + dts[i]).fract();
            }

            // Calculate operator amplitudes
            let amp_a = env_a * self.ops[0].level * voice.velocity;
            let amp_b = env_b * self.ops[1].level;
            let amp_c = env_c * self.ops[2].level;
            let amp_d = env_d * self.ops[3].level;

            // Operator D Feedback
            let fb_amount = self.feedback_d.clamp(0.0, 1.0) * 2.5;
            let fb_signal = 0.5 * (voice.fb_y1 + voice.fb_y2);
            let phase_d_mod = (voice.phases[3] + fb_amount * fb_signal / (2.0 * PI)).fract();
            let raw_d = Self::evaluate_waveform(self.ops[3].waveform, phase_d_mod, dts[3], &self.ops[3].user_harmonics);
            let out_d = amp_d * raw_d;
            voice.fb_y2 = voice.fb_y1;
            voice.fb_y1 = out_d;

            // Algorithm routing FM phase modulation:
            let mod_index_scale = 3.0; // FM index scaling factor

            let out_final = match self.algorithm {
                // Alg 1: D -> C -> B -> A -> Out
                FmAlgorithm::Alg1 => {
                    let phase_c = (voice.phases[2] + mod_index_scale * out_d).fract();
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, phase_c, dts[2], &self.ops[2].user_harmonics);
                    let phase_b = (voice.phases[1] + mod_index_scale * out_c).fract();
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, phase_b, dts[1], &self.ops[1].user_harmonics);
                    let phase_a = (voice.phases[0] + mod_index_scale * out_b).fract();
                    amp_a * Self::evaluate_waveform(self.ops[0].waveform, phase_a, dts[0], &self.ops[0].user_harmonics)
                }
                // Alg 2: (D + C) -> B -> A -> Out
                FmAlgorithm::Alg2 => {
                    let raw_c = Self::evaluate_waveform(self.ops[2].waveform, voice.phases[2], dts[2], &self.ops[2].user_harmonics);
                    let out_c = amp_c * raw_c;
                    let phase_b = (voice.phases[1] + mod_index_scale * (out_d + out_c)).fract();
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, phase_b, dts[1], &self.ops[1].user_harmonics);
                    let phase_a = (voice.phases[0] + mod_index_scale * out_b).fract();
                    amp_a * Self::evaluate_waveform(self.ops[0].waveform, phase_a, dts[0], &self.ops[0].user_harmonics)
                }
                // Alg 3: (D -> C + B) -> A -> Out
                FmAlgorithm::Alg3 => {
                    let phase_c = (voice.phases[2] + mod_index_scale * out_d).fract();
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, phase_c, dts[2], &self.ops[2].user_harmonics);
                    let raw_b = Self::evaluate_waveform(self.ops[1].waveform, voice.phases[1], dts[1], &self.ops[1].user_harmonics);
                    let out_b = amp_b * raw_b;
                    let phase_a = (voice.phases[0] + mod_index_scale * (out_c + out_b)).fract();
                    amp_a * Self::evaluate_waveform(self.ops[0].waveform, phase_a, dts[0], &self.ops[0].user_harmonics)
                }
                // Alg 4: D -> (C + B) -> A -> Out
                FmAlgorithm::Alg4 => {
                    let phase_c = (voice.phases[2] + mod_index_scale * out_d).fract();
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, phase_c, dts[2], &self.ops[2].user_harmonics);
                    let phase_b = (voice.phases[1] + mod_index_scale * out_d).fract();
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, phase_b, dts[1], &self.ops[1].user_harmonics);
                    let phase_a = (voice.phases[0] + mod_index_scale * (out_c + out_b)).fract();
                    amp_a * Self::evaluate_waveform(self.ops[0].waveform, phase_a, dts[0], &self.ops[0].user_harmonics)
                }
                // Alg 5: (D -> C) -> Out and (B -> A) -> Out
                FmAlgorithm::Alg5 => {
                    let phase_c = (voice.phases[2] + mod_index_scale * out_d).fract();
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, phase_c, dts[2], &self.ops[2].user_harmonics);
                    let raw_b = Self::evaluate_waveform(self.ops[1].waveform, voice.phases[1], dts[1], &self.ops[1].user_harmonics);
                    let out_b = amp_b * raw_b;
                    let phase_a = (voice.phases[0] + mod_index_scale * out_b).fract();
                    let out_a = amp_a * Self::evaluate_waveform(self.ops[0].waveform, phase_a, dts[0], &self.ops[0].user_harmonics);
                    0.5 * (out_c + out_a)
                }
                // Alg 6: (D + C + B) -> A -> Out
                FmAlgorithm::Alg6 => {
                    let raw_c = Self::evaluate_waveform(self.ops[2].waveform, voice.phases[2], dts[2], &self.ops[2].user_harmonics);
                    let out_c = amp_c * raw_c;
                    let raw_b = Self::evaluate_waveform(self.ops[1].waveform, voice.phases[1], dts[1], &self.ops[1].user_harmonics);
                    let out_b = amp_b * raw_b;
                    let phase_a = (voice.phases[0] + mod_index_scale * (out_d + out_c + out_b)).fract();
                    amp_a * Self::evaluate_waveform(self.ops[0].waveform, phase_a, dts[0], &self.ops[0].user_harmonics)
                }
                // Alg 7: (D -> C) -> Out, B -> Out, A -> Out
                FmAlgorithm::Alg7 => {
                    let phase_c = (voice.phases[2] + mod_index_scale * out_d).fract();
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, phase_c, dts[2], &self.ops[2].user_harmonics);
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, voice.phases[1], dts[1], &self.ops[1].user_harmonics);
                    let out_a = amp_a * Self::evaluate_waveform(self.ops[0].waveform, voice.phases[0], dts[0], &self.ops[0].user_harmonics);
                    0.333 * (out_c + out_b + out_a)
                }
                // Alg 8: (D -> C -> B) -> Out, A -> Out
                FmAlgorithm::Alg8 => {
                    let phase_c = (voice.phases[2] + mod_index_scale * out_d).fract();
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, phase_c, dts[2], &self.ops[2].user_harmonics);
                    let phase_b = (voice.phases[1] + mod_index_scale * out_c).fract();
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, phase_b, dts[1], &self.ops[1].user_harmonics);
                    let out_a = amp_a * Self::evaluate_waveform(self.ops[0].waveform, voice.phases[0], dts[0], &self.ops[0].user_harmonics);
                    0.5 * (out_b + out_a)
                }
                // Alg 9: (D + C) -> B -> Out, A -> Out
                FmAlgorithm::Alg9 => {
                    let raw_c = Self::evaluate_waveform(self.ops[2].waveform, voice.phases[2], dts[2], &self.ops[2].user_harmonics);
                    let out_c = amp_c * raw_c;
                    let phase_b = (voice.phases[1] + mod_index_scale * (out_d + out_c)).fract();
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, phase_b, dts[1], &self.ops[1].user_harmonics);
                    let out_a = amp_a * Self::evaluate_waveform(self.ops[0].waveform, voice.phases[0], dts[0], &self.ops[0].user_harmonics);
                    0.5 * (out_b + out_a)
                }
                // Alg 10: D -> (C + B + A) -> Out
                FmAlgorithm::Alg10 => {
                    let phase_c = (voice.phases[2] + mod_index_scale * out_d).fract();
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, phase_c, dts[2], &self.ops[2].user_harmonics);
                    let phase_b = (voice.phases[1] + mod_index_scale * out_d).fract();
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, phase_b, dts[1], &self.ops[1].user_harmonics);
                    let phase_a = (voice.phases[0] + mod_index_scale * out_d).fract();
                    let out_a = amp_a * Self::evaluate_waveform(self.ops[0].waveform, phase_a, dts[0], &self.ops[0].user_harmonics);
                    0.333 * (out_c + out_b + out_a)
                }
                // Alg 11: A + B + C + D -> Out (Pure Additive)
                FmAlgorithm::Alg11 => {
                    let out_c = amp_c * Self::evaluate_waveform(self.ops[2].waveform, voice.phases[2], dts[2], &self.ops[2].user_harmonics);
                    let out_b = amp_b * Self::evaluate_waveform(self.ops[1].waveform, voice.phases[1], dts[1], &self.ops[1].user_harmonics);
                    let out_a = amp_a * Self::evaluate_waveform(self.ops[0].waveform, voice.phases[0], dts[0], &self.ops[0].user_harmonics);
                    0.25 * (out_d + out_c + out_b + out_a)
                }
            };

            // Master ZDF Filter
            let filtered = if self.filter_enabled {
                voice.master_filter.filter_type = self.filter_type;
                voice.master_filter.circuit_model = self.filter_model;
                voice.master_filter.process_sample(
                    out_final,
                    self.filter_cutoff,
                    self.filter_resonance,
                    0.0,
                    self.filter_drive,
                )
            } else {
                out_final
            };

            let voice_out = filtered * base_vol;
            out_l += voice_out * left_pan;
            out_r += voice_out * right_pan;
        }

        (out_l, out_r)
    }
}

impl Device for OperatorDevice {
    fn name(&self) -> &str {
        "Operator"
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

impl InstrumentDevice for OperatorDevice {
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
    fn test_operator_all_11_algorithms() {
        for alg in [
            FmAlgorithm::Alg1,
            FmAlgorithm::Alg2,
            FmAlgorithm::Alg3,
            FmAlgorithm::Alg4,
            FmAlgorithm::Alg5,
            FmAlgorithm::Alg6,
            FmAlgorithm::Alg7,
            FmAlgorithm::Alg8,
            FmAlgorithm::Alg9,
            FmAlgorithm::Alg10,
            FmAlgorithm::Alg11,
        ] {
            let mut op = OperatorDevice::new(44100.0);
            op.algorithm = alg;
            op.trigger_note_on(60, 100);

            let mut energy = 0.0f32;
            for _ in 0..256 {
                let (l, r) = op.process_stereo_sample();
                assert!(l.is_finite());
                assert!(r.is_finite());
                energy += l * l + r * r;
            }
            assert!(energy > 0.0001, "Algorithm {:?} must produce sound", alg);
        }
    }

    #[test]
    fn test_operator_all_11_algorithms_produce_distinct_waveforms() {
        let algorithms = [
            FmAlgorithm::Alg1,
            FmAlgorithm::Alg2,
            FmAlgorithm::Alg3,
            FmAlgorithm::Alg4,
            FmAlgorithm::Alg5,
            FmAlgorithm::Alg6,
            FmAlgorithm::Alg7,
            FmAlgorithm::Alg8,
            FmAlgorithm::Alg9,
            FmAlgorithm::Alg10,
            FmAlgorithm::Alg11,
        ];

        let mut outputs: Vec<Vec<f32>> = Vec::new();

        for &alg in &algorithms {
            let mut op = OperatorDevice::new(44100.0);
            op.algorithm = alg;
            op.ops[0].coarse = 1.0;
            op.ops[0].level = 1.0;
            op.ops[1].coarse = 2.0;
            op.ops[1].level = 0.8;
            op.ops[2].coarse = 3.0;
            op.ops[2].level = 0.7;
            op.ops[3].coarse = 4.0;
            op.ops[3].level = 0.6;
            op.feedback_d = 0.2;

            op.trigger_note_on(60, 100);

            let mut buf = Vec::with_capacity(128);
            for _ in 0..128 {
                let (l, _) = op.process_stereo_sample();
                buf.push(l);
            }
            outputs.push(buf);
        }

        // Pairwise verify every algorithm produces distinct waveform from all others
        for i in 0..algorithms.len() {
            for j in (i + 1)..algorithms.len() {
                let mut diff = 0.0f32;
                for k in 0..128 {
                    diff += (outputs[i][k] - outputs[j][k]).abs();
                }
                assert!(
                    diff > 0.001,
                    "Algorithms {:?} and {:?} produced identical waveforms (diff: {})",
                    algorithms[i],
                    algorithms[j],
                    diff
                );
            }
        }
    }
}
