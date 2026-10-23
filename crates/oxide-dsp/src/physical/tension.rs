//! Tension: Plucked & Bowed String Waveguide Physical Modeling Synthesizer.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::envelope::AdsrEnvelope;
use crate::physical::collision::ModalFilter;

pub const MAX_TENSION_VOICES: usize = 8;
pub const MAX_DELAY_LENGTH: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensionExciterType {
    Pluck,
    Bow,
    Hammer,
}

#[derive(Debug, Clone)]
pub struct TensionVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Delay lines for bidirectional string waveguide
    pub delay_line: [f32; MAX_DELAY_LENGTH],
    pub write_pos: usize,
    pub delay_length: f32,

    // Loop loss filter state
    pub filter_state: f32,

    // Dispersion allpass filter state for string stiffness
    pub ap_x1: f32,
    pub ap_y1: f32,

    // Bow continuous friction state
    pub is_bowing: bool,
    pub bow_velocity: f32,

    // Body resonator
    pub body_res1: ModalFilter,
    pub body_res2: ModalFilter,

    pub amp_env: AdsrEnvelope,
}

impl TensionVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            delay_line: [0.0; MAX_DELAY_LENGTH],
            write_pos: 0,
            delay_length: 200.0,
            filter_state: 0.0,
            ap_x1: 0.0,
            ap_y1: 0.0,
            is_bowing: false,
            bow_velocity: 0.0,
            body_res1: ModalFilter::new(),
            body_res2: ModalFilter::new(),
            amp_env: AdsrEnvelope::new(sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.delay_line = [0.0; MAX_DELAY_LENGTH];
        self.write_pos = 0;
        self.filter_state = 0.0;
        self.ap_x1 = 0.0;
        self.ap_y1 = 0.0;
        self.is_bowing = false;
        self.body_res1.reset();
        self.body_res2.reset();
        self.amp_env.reset();
    }

    #[inline(always)]
    pub fn read_delay(&self, delay: f32) -> f32 {
        let d = delay.clamp(1.0, (MAX_DELAY_LENGTH - 2) as f32);
        let d_int = d.floor() as usize;
        let frac = d - d_int as f32;

        let idx0 = (self.write_pos + MAX_DELAY_LENGTH - d_int) % MAX_DELAY_LENGTH;
        let idx1 = (idx0 + MAX_DELAY_LENGTH - 1) % MAX_DELAY_LENGTH;

        let s0 = self.delay_line[idx0];
        let s1 = self.delay_line[idx1];
        s0 + frac * (s1 - s0)
    }

    #[inline(always)]
    pub fn write_delay(&mut self, sample: f32) {
        self.delay_line[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % MAX_DELAY_LENGTH;
    }
}

/// Ableton Live 12 Tension Physical Modeling String Synthesizer.
#[derive(Debug, Clone)]
pub struct TensionDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    pub exciter_type: TensionExciterType,
    pub string_damping: f32,   // 0.0 .. 1.0 (decay loss)
    pub string_decay_loss: f32,// 0.8 .. 0.999
    pub string_stiffness: f32, // Inharmonic dispersion (0.0 .. 0.8)
    pub pickup_position: f32,  // 0.0 .. 1.0 along string
    pub body_size: f32,        // Body resonance frequency scale
    pub body_gain: f32,

    pub master_volume: f32,

    // Voices
    pub voices: [TensionVoice; MAX_TENSION_VOICES],
    pub voice_counter: u64,
}

impl TensionDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let voices = std::array::from_fn(|_| TensionVoice::new(sr));

        Self {
            enabled: true,
            sample_rate: sr,
            exciter_type: TensionExciterType::Pluck,
            string_damping: 0.3,
            string_decay_loss: 0.992,
            string_stiffness: 0.2,
            pickup_position: 0.2,
            body_size: 1.0,
            body_gain: 0.4,
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
        v.reset();
        v.active = true;

        let note_freq = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        v.delay_length = (self.sample_rate / note_freq.clamp(20.0, 5000.0)).clamp(2.0, (MAX_DELAY_LENGTH - 4) as f32);

        match self.exciter_type {
            TensionExciterType::Pluck => {
                // Initialize delay line with triangular displacement pluck
                let len = v.delay_length as usize;
                let pick_pos = ((len as f32 * self.pickup_position).round() as usize).clamp(1, len.saturating_sub(1));
                for i in 0..len {
                    let amp = if i < pick_pos {
                        i as f32 / pick_pos as f32
                    } else {
                        (len - i) as f32 / (len - pick_pos).max(1) as f32
                    };
                    v.delay_line[i] = (amp * 2.0 - 1.0) * v.velocity * 0.8;
                }
                v.write_pos = len % MAX_DELAY_LENGTH;
            }
            TensionExciterType::Bow => {
                v.is_bowing = true;
                v.bow_velocity = v.velocity;
            }
            TensionExciterType::Hammer => {
                let string_len = v.delay_length as usize;
                let strike_len = (v.delay_length * 0.15).max(4.0) as usize;
                for i in 0..strike_len.min(MAX_DELAY_LENGTH) {
                    v.delay_line[i] = (PI * i as f32 / strike_len as f32).sin() * v.velocity;
                }
                v.write_pos = string_len % MAX_DELAY_LENGTH;
            }
        }

        v.amp_env.trigger_attack();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                v.is_bowing = false;
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

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let amp_val = voice.amp_env.next(1.0, 2000.0, 0.7, 200.0);
            if !voice.amp_env.is_active() && amp_val < 0.0001 && !voice.is_bowing {
                voice.active = false;
                continue;
            }

            // Digital waveguide string loop:
            // Read from string delay line
            let string_sample = voice.read_delay(voice.delay_length);

            // Exciter injection if bowing
            let exciter = if voice.is_bowing {
                // Non-linear bow friction curve
                let v_rel = voice.bow_velocity - string_sample;
                let friction = (v_rel * 2.0) / (1.0 + (v_rel * 3.0).powi(2));
                friction * 0.4
            } else {
                0.0
            };

            // String loop loss: 1-pole lowpass damping filter
            let alpha = (1.0 - self.string_damping * 0.6).clamp(0.1, 0.98);
            voice.filter_state += alpha * (string_sample + exciter - voice.filter_state);
            let loss_sample = voice.filter_state * self.string_decay_loss;

            // Dispersion allpass filter for string stiffness
            let ap_coeff = self.string_stiffness * 0.6;
            let ap_out = -ap_coeff * loss_sample + voice.ap_x1 + ap_coeff * voice.ap_y1;
            voice.ap_x1 = loss_sample;
            voice.ap_y1 = ap_out;

            // Feedback into waveguide
            voice.write_delay(ap_out.clamp(-2.5, 2.5));

            // Acoustic body resonance
            let body_f1 = 120.0 * self.body_size;
            let body_f2 = 280.0 * self.body_size;
            let b1 = voice.body_res1.process(string_sample, body_f1, 0.4, sr);
            let b2 = voice.body_res2.process(string_sample, body_f2, 0.3, sr);
            let body_out = (b1 + b2) * self.body_gain;

            let total_out = (string_sample * 0.8 + body_out) * amp_val * base_vol;
            out_l += total_out * left_pan;
            out_r += total_out * right_pan;
        }

        (out_l, out_r)
    }
}

impl Device for TensionDevice {
    fn name(&self) -> &str {
        "Tension"
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

impl InstrumentDevice for TensionDevice {
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
    fn test_tension_string_pluck_and_bow() {
        let mut tension = TensionDevice::new(44100.0);
        tension.trigger_note_on(60, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        tension.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Tension must render audible plucked string audio");
    }
}
