//! Electric: Electro-Mechanical Piano Physical Modeling Synthesizer.

use std::f32::consts::PI;
use oxide_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::envelope::AdsrEnvelope;
use crate::physical::collision::ModalFilter;

pub const MAX_ELECTRIC_VOICES: usize = 8;

#[derive(Debug, Clone)]
pub struct ElectricVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Tine & Tone Bar modal resonators
    pub tine_mode1: ModalFilter, // Fundamental
    pub tine_mode2: ModalFilter, // Inharmonic overtone (bell clang)
    pub tone_bar_mode: ModalFilter, // Sustained tone bar body mode

    pub hammer_pulse_left: usize,
    pub hammer_force: f32,

    pub amp_env: AdsrEnvelope,
}

impl ElectricVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            tine_mode1: ModalFilter::new(),
            tine_mode2: ModalFilter::new(),
            tone_bar_mode: ModalFilter::new(),
            hammer_pulse_left: 0,
            hammer_force: 1.0,
            amp_env: AdsrEnvelope::new(sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.tine_mode1.reset();
        self.tine_mode2.reset();
        self.tone_bar_mode.reset();
        self.hammer_pulse_left = 0;
        self.amp_env.reset();
    }
}

/// Ableton Live 12 Electric Electro-Mechanical Piano Physical Modeling Synthesizer.
#[derive(Debug, Clone)]
pub struct ElectricDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // Hammer settings
    pub hammer_hardness: f32, // 0.0 (soft felt) .. 1.0 (hard neoprene)

    // Tine & Tone Bar
    pub tine_decay: f32,       // 0.5 .. 8.0 s
    pub bell_inharmonicity: f32, // 5.2 .. 6.5 x fundamental
    pub bell_gain: f32,
    pub tone_bar_decay: f32,

    // Pickup
    pub pickup_distance: f32, // 0.1 (hot/distorted) .. 1.0 (clean)
    pub pickup_symmetry: f32, // -1.0 .. +1.0

    // Tremolo
    pub tremolo_enabled: bool,
    pub tremolo_rate: f32,  // 0.5 .. 12 Hz
    pub tremolo_depth: f32, // 0.0 .. 1.0
    pub tremolo_phase: f32,

    pub master_volume: f32,

    // Voices
    pub voices: [ElectricVoice; MAX_ELECTRIC_VOICES],
    pub voice_counter: u64,
}

impl ElectricDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let voices = std::array::from_fn(|_| ElectricVoice::new(sr));

        Self {
            enabled: true,
            sample_rate: sr,
            hammer_hardness: 0.5,
            tine_decay: 2.5,
            bell_inharmonicity: 5.8,
            bell_gain: 0.35,
            tone_bar_decay: 3.5,
            pickup_distance: 0.4,
            pickup_symmetry: 0.1,
            tremolo_enabled: true,
            tremolo_rate: 4.5,
            tremolo_depth: 0.3,
            tremolo_phase: 0.0,
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

        let pulse_samples = (3.0 + (1.0 - self.hammer_hardness) * 12.0) as usize;
        v.hammer_pulse_left = pulse_samples.max(2);
        v.hammer_force = v.velocity;

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
        let inv_sr = 1.0 / sr;

        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let base_vol = self.master_volume;

        // Advance Tremolo LFO
        if self.tremolo_enabled {
            self.tremolo_phase = (self.tremolo_phase + self.tremolo_rate * inv_sr).fract();
        }
        let trem_mod_l = 1.0 + self.tremolo_depth * (2.0 * PI * self.tremolo_phase).sin();
        let trem_mod_r = 1.0 + self.tremolo_depth * (2.0 * PI * (self.tremolo_phase + 0.5)).sin();

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let amp_val = voice.amp_env.next(1.0, 3000.0, 0.02, 150.0);
            if !voice.amp_env.is_active() && amp_val < 0.0001 && voice.hammer_pulse_left == 0 {
                voice.active = false;
                continue;
            }

            // Felt hammer impact pulse
            let impact = if voice.hammer_pulse_left > 0 {
                voice.hammer_pulse_left -= 1;
                voice.hammer_force * 2.0
            } else {
                0.0
            };

            let f0 = 440.0 * 2.0f32.powf((voice.note as f32 - 69.0) / 12.0);

            // Modal resonators:
            // 1. Tine Fundamental
            let s1 = voice.tine_mode1.process(impact, f0, self.tine_decay, sr);

            // 2. Tine Bell Inharmonic overtone
            let f_bell = f0 * self.bell_inharmonicity;
            let s2 = voice.tine_mode2.process(impact, f_bell, self.tine_decay * 0.4, sr) * self.bell_gain;

            // 3. Tone bar resonant coupling
            let f_tonebar = f0 * 1.002; // Slight coupling detune
            let s3 = voice.tone_bar_mode.process(s1 * 0.4, f_tonebar, self.tone_bar_decay, sr);

            let mechanical_disp = s1 + s2 + s3;

            // Electromagnetic Pickup:
            // Non-linear reluctance saturation as tine swings close to pole piece
            let dist_inv = 1.0 / self.pickup_distance.max(0.1);
            let raw_pickup = mechanical_disp * dist_inv + self.pickup_symmetry * 0.1;
            let pickup_out = (raw_pickup * 1.5).tanh() * 0.7;

            let voice_mono = pickup_out * amp_val * base_vol;

            out_l += voice_mono * trem_mod_l * 0.5;
            out_r += voice_mono * trem_mod_r * 0.5;
        }

        (out_l, out_r)
    }
}

impl Device for ElectricDevice {
    fn name(&self) -> &str {
        "Electric"
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

impl InstrumentDevice for ElectricDevice {
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
    fn test_electric_piano_rendering_and_pickup_saturation() {
        let mut electric = ElectricDevice::new(44100.0);
        electric.trigger_note_on(60, 110);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        electric.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Electric piano must render audible audio");
    }
}
