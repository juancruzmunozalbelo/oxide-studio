//! Collision: Mallet & Membrane Modal Resonator Physical Modeling Synthesizer.

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::envelope::AdsrEnvelope;

pub const MAX_COLLISION_VOICES: usize = 8;
pub const NUM_MODAL_MODES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResonatorStructure {
    Beam,
    Marimba,
    Membrane,
    Plate,
    Pipe,
}

impl ResonatorStructure {
    pub fn mode_ratios(&self) -> [f32; NUM_MODAL_MODES] {
        match self {
            ResonatorStructure::Beam => [1.0, 2.76, 5.40, 8.93, 13.34, 18.64],
            ResonatorStructure::Marimba => [1.0, 3.98, 9.25, 14.1, 20.2, 27.5],
            ResonatorStructure::Membrane => [1.0, 1.59, 2.14, 2.30, 2.65, 2.92],
            ResonatorStructure::Plate => [1.0, 1.58, 2.29, 2.91, 3.65, 4.38],
            ResonatorStructure::Pipe => [1.0, 3.0, 5.0, 7.0, 9.0, 11.0], // Closed pipe odd harmonics
        }
    }
}

/// 2nd order damped harmonic resonator mode.
#[derive(Debug, Clone)]
pub struct ModalFilter {
    pub y1: f32,
    pub y2: f32,
}

impl ModalFilter {
    pub fn new() -> Self {
        Self { y1: 0.0, y2: 0.0 }
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    #[inline(always)]
    pub fn process(&mut self, input: f32, freq_hz: f32, decay_s: f32, sample_rate: f32) -> f32 {
        let w = (2.0 * PI * freq_hz.clamp(20.0, sample_rate * 0.45) / sample_rate).min(PI * 0.95);
        let gamma = 1.0 / (decay_s.max(0.005) * sample_rate);
        let r = (-gamma).exp();

        let a1 = 2.0 * r * w.cos();
        let a2 = r * r;
        let b0 = 1.0;

        let out = a1 * self.y1 - a2 * self.y2 + b0 * input;
        self.y2 = self.y1;
        self.y1 = out.clamp(-3.0, 3.0);
        out
    }
}

#[derive(Debug, Clone)]
pub struct CollisionVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Mallet impact exciter state
    pub mallet_samples_left: usize,
    pub mallet_stiffness: f32,
    pub mallet_vel: f32,

    // Resonator 1 modal bank
    pub res1_modes: [ModalFilter; NUM_MODAL_MODES],
    // Resonator 2 modal bank
    pub res2_modes: [ModalFilter; NUM_MODAL_MODES],

    pub amp_env: AdsrEnvelope,
}

impl CollisionVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            mallet_samples_left: 0,
            mallet_stiffness: 0.5,
            mallet_vel: 1.0,
            res1_modes: std::array::from_fn(|_| ModalFilter::new()),
            res2_modes: std::array::from_fn(|_| ModalFilter::new()),
            amp_env: AdsrEnvelope::new(sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.mallet_samples_left = 0;
        for m in &mut self.res1_modes {
            m.reset();
        }
        for m in &mut self.res2_modes {
            m.reset();
        }
        self.amp_env.reset();
    }
}

/// Ableton Live 12 Collision Physical Modeling Synthesizer.
#[derive(Debug, Clone)]
pub struct CollisionDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // Mallet Exciter
    pub mallet_stiffness: f32, // 0.0 (soft felt) .. 1.0 (hard wood)
    pub mallet_noise: f32,     // Strike noise blend

    // Resonator 1
    pub res1_structure: ResonatorStructure,
    pub res1_decay: f32,    // 0.1 .. 10.0 s
    pub res1_material: f32, // Damping factor (wood vs metal)
    pub res1_gain: f32,

    // Resonator 2 & Coupling
    pub res2_enabled: bool,
    pub res2_structure: ResonatorStructure,
    pub res2_decay: f32,
    pub res2_material: f32,
    pub res2_gain: f32,
    pub coupling_gain: f32, // Res1 -> Res2 coupling

    pub master_volume: f32,

    // Voices
    pub voices: [CollisionVoice; MAX_COLLISION_VOICES],
    pub voice_counter: u64,
}

impl CollisionDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let voices = std::array::from_fn(|_| CollisionVoice::new(sr));

        Self {
            enabled: true,
            sample_rate: sr,
            mallet_stiffness: 0.6,
            mallet_noise: 0.15,
            res1_structure: ResonatorStructure::Marimba,
            res1_decay: 1.2,
            res1_material: 0.4,
            res1_gain: 0.8,
            res2_enabled: true,
            res2_structure: ResonatorStructure::Pipe,
            res2_decay: 0.8,
            res2_material: 0.3,
            res2_gain: 0.5,
            coupling_gain: 0.3,
            master_volume: 0.7,
            voices,
            voice_counter: 0,
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

        // Mallet impact pulse: duration depends on stiffness (harder = shorter pulse)
        let pulse_ms = 1.0 + (1.0 - self.mallet_stiffness) * 8.0;
        v.mallet_samples_left = ((pulse_ms * 0.001 * self.sample_rate) as usize).max(4);
        v.mallet_stiffness = self.mallet_stiffness;
        v.mallet_vel = v.velocity;

        for m in &mut v.res1_modes {
            m.reset();
        }
        for m in &mut v.res2_modes {
            m.reset();
        }

        v.amp_env.trigger_attack();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                v.amp_env.trigger_release();
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
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let base_vol = self.master_volume;
        let pan_angle = 0.25 * PI;
        let left_pan = pan_angle.cos();
        let right_pan = pan_angle.sin();

        let ratios1 = self.res1_structure.mode_ratios();
        let ratios2 = self.res2_structure.mode_ratios();

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            // Envelope to avoid long hanging tails
            let amp_val = voice.amp_env.next(1.0, 1500.0, 0.7, 300.0);
            if !voice.amp_env.is_active() && amp_val < 0.0001 && voice.mallet_samples_left == 0 {
                voice.active = false;
                continue;
            }

            // Mallet Hertzian impact excitation pulse
            let exciter = if voice.mallet_samples_left > 0 {
                let frac = voice.mallet_samples_left as f32 / 20.0;
                let hertz_force = (frac.clamp(0.0, 1.0)).powf(1.5);
                voice.mallet_samples_left -= 1;
                hertz_force * voice.mallet_vel * 2.0
            } else {
                0.0
            };

            let fundamental_freq = 440.0 * 2.0f32.powf((voice.note as f32 - 69.0) / 12.0);

            // Resonator 1 modal bank
            let mut res1_out = 0.0f32;
            for (i, m) in voice.res1_modes.iter_mut().enumerate() {
                let mode_freq = fundamental_freq * ratios1[i];
                let mode_decay = self.res1_decay * (1.0 / (1.0 + self.res1_material * (i as f32 * 0.4)));
                let mode_weight = 1.0 / (i + 1) as f32;
                res1_out += m.process(exciter, mode_freq, mode_decay, sr) * mode_weight;
            }
            res1_out *= self.res1_gain;

            // Resonator 2 modal bank (driven by exciter + coupled from Resonator 1)
            let mut res2_out = 0.0f32;
            if self.res2_enabled {
                let res2_in = exciter * 0.5 + res1_out * self.coupling_gain;
                for (i, m) in voice.res2_modes.iter_mut().enumerate() {
                    let mode_freq = fundamental_freq * ratios2[i];
                    let mode_decay = self.res2_decay * (1.0 / (1.0 + self.res2_material * (i as f32 * 0.4)));
                    let mode_weight = 1.0 / (i + 1) as f32;
                    res2_out += m.process(res2_in, mode_freq, mode_decay, sr) * mode_weight;
                }
                res2_out *= self.res2_gain;
            }

            let voice_out = ((res1_out + res2_out) * 0.4 * amp_val * base_vol).clamp(-1.8, 1.8);
            out_l += voice_out * left_pan;
            out_r += voice_out * right_pan;
        }

        (out_l, out_r)
    }
}

impl Device for CollisionDevice {
    fn name(&self) -> &str {
        "Collision"
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

impl InstrumentDevice for CollisionDevice {
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
    fn test_collision_rendering() {
        let mut coll = CollisionDevice::new(44100.0);
        coll.trigger_note_on(60, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        coll.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Collision must render audible modal physical modeling audio");
    }
}
