//! Simpler: Live 12 Flagship Sampler & Slicer Device.

use std::sync::Arc;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};
use ableton_dsp::{AdsrEnvelope, CircuitModel, CytomicFilterType, CytomicZdfFilter, Lfo, LfoShape};

pub const MAX_SIMPLER_VOICES: usize = 16;
pub const MAX_SLICES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimplerMode {
    Classic,
    OneShot,
    Slicing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlicingMode {
    Transient,
    Beat,
    Region,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneShotRetrigger {
    Gate,
    Trigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimplerWarpMode {
    Beats,
    Tones,
    Texture,
    RePitch,
    Complex,
    ComplexPro,
}

#[derive(Debug, Clone)]
pub struct SimplerVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Playback position & fractional phase
    pub position: f64,
    pub phase_step: f64,
    pub slice_start: usize,
    pub slice_end: usize,

    // Fade / envelopes
    pub amp_env: AdsrEnvelope,
    pub filter_env: AdsrEnvelope,
    pub pitch_env: AdsrEnvelope,
    pub filter: CytomicZdfFilter,

    // OneShot fade state
    pub fade_in_samples: usize,
    pub fade_out_samples: usize,
    pub played_samples: usize,
}

impl SimplerVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            position: 0.0,
            phase_step: 1.0,
            slice_start: 0,
            slice_end: 0,
            amp_env: AdsrEnvelope::new(sample_rate),
            filter_env: AdsrEnvelope::new(sample_rate),
            pitch_env: AdsrEnvelope::new(sample_rate),
            filter: CytomicZdfFilter::new(sample_rate),
            fade_in_samples: 0,
            fade_out_samples: 0,
            played_samples: 0,
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.position = 0.0;
        self.amp_env.reset();
        self.filter_env.reset();
        self.pitch_env.reset();
        self.filter.reset();
        self.played_samples = 0;
    }
}

/// Ableton Live 12 Simpler device.
#[derive(Debug, Clone)]
pub struct SimplerDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // Sample storage
    pub sample_data: Option<Arc<[f32]>>,
    pub sample_sample_rate: u32,
    pub sample_channels: u8,

    // Modes
    pub mode: SimplerMode,
    pub slicing_mode: SlicingMode,
    pub oneshot_retrigger: OneShotRetrigger,
    pub warp_mode: SimplerWarpMode,
    pub warp_enabled: bool,

    // Handles [0.0 .. 1.0]
    pub start_pos: f32,
    pub length_pos: f32,
    pub loop_enabled: bool,
    pub loop_start: f32,
    pub loop_length: f32,
    pub loop_crossfade_ms: f32,

    // OneShot fades
    pub fade_in_ms: f32,
    pub fade_out_ms: f32,

    // Slices (sample index boundaries)
    pub slices: Vec<usize>,
    pub slice_base_note: u8, // Default 36 (C1)

    // Envelopes & Filter
    pub amp_adsr: (f32, f32, f32, f32),
    pub filter_adsr: (f32, f32, f32, f32),
    pub pitch_adsr: (f32, f32, f32, f32),
    pub pitch_env_semitones: f32, // -48 .. +48

    pub filter_enabled: bool,
    pub filter_type: CytomicFilterType,
    pub filter_model: CircuitModel,
    pub filter_cutoff: f32,
    pub filter_res: f32,
    pub filter_env_amt: f32,

    // LFO
    pub lfo: Lfo,
    pub lfo_freq: f32,
    pub lfo_shape: LfoShape,
    pub lfo_to_pitch: f32,
    pub lfo_to_filter: f32,

    // Tuning & Volume
    pub transpose_semitones: i8, // -48 .. +48
    pub detune_cents: f32,
    pub master_volume: f32,
    pub master_pan: f32,

    // Voices
    pub voices: [SimplerVoice; MAX_SIMPLER_VOICES],
    pub voice_counter: u64,
}

impl SimplerDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let voices = std::array::from_fn(|_| SimplerVoice::new(sr));

        // Create default synthetic sine sample so Simpler always has playable data out-of-the-box
        let default_len = 44100;
        let mut default_sample = Vec::with_capacity(default_len);
        for i in 0..default_len {
            let t = i as f32 / 44100.0;
            // Warm organ-like harmonic tone
            let val = 0.7 * (2.0 * std::f32::consts::PI * 261.63 * t).sin()
                + 0.25 * (2.0 * std::f32::consts::PI * 523.25 * t).sin()
                + 0.1 * (2.0 * std::f32::consts::PI * 784.88 * t).sin();
            default_sample.push(val);
        }

        let mut simpler = Self {
            enabled: true,
            sample_rate: sr,
            sample_data: Some(Arc::from(default_sample.into_boxed_slice())),
            sample_sample_rate: 44100,
            sample_channels: 1,
            mode: SimplerMode::Classic,
            slicing_mode: SlicingMode::Transient,
            oneshot_retrigger: OneShotRetrigger::Trigger,
            warp_mode: SimplerWarpMode::Beats,
            warp_enabled: false,
            start_pos: 0.0,
            length_pos: 1.0,
            loop_enabled: false,
            loop_start: 0.0,
            loop_length: 1.0,
            loop_crossfade_ms: 10.0,
            fade_in_ms: 2.0,
            fade_out_ms: 200.0,
            slices: Vec::new(),
            slice_base_note: 36,
            amp_adsr: (2.0, 300.0, 0.8, 150.0),
            filter_adsr: (5.0, 200.0, 0.2, 100.0),
            pitch_adsr: (1.0, 50.0, 0.0, 50.0),
            pitch_env_semitones: 0.0,
            filter_enabled: true,
            filter_type: CytomicFilterType::Lowpass24,
            filter_model: CircuitModel::Clean,
            filter_cutoff: 12000.0,
            filter_res: 0.1,
            filter_env_amt: 0.0,
            lfo: Lfo::new(),
            lfo_freq: 3.0,
            lfo_shape: LfoShape::Triangle,
            lfo_to_pitch: 0.0,
            lfo_to_filter: 0.0,
            transpose_semitones: 0,
            detune_cents: 0.0,
            master_volume: 0.8,
            master_pan: 0.0,
            voices,
            voice_counter: 0,
        };

        simpler.recalculate_slices();
        simpler
    }

    /// Load sample from buffer.
    pub fn load_sample(&mut self, samples: Vec<f32>, sample_rate: u32, channels: u8) {
        self.sample_data = Some(Arc::from(samples.into_boxed_slice()));
        self.sample_sample_rate = sample_rate;
        self.sample_channels = channels;
        self.recalculate_slices();
    }

    /// Recalculate slice points according to active slicing mode.
    pub fn recalculate_slices(&mut self) {
        self.slices.clear();
        let total_samples = match &self.sample_data {
            Some(s) => s.len() / self.sample_channels.max(1) as usize,
            None => return,
        };

        if total_samples == 0 {
            return;
        }

        match self.slicing_mode {
            SlicingMode::Region => {
                let num_regions = 16;
                let step = total_samples / num_regions;
                for i in 0..num_regions {
                    self.slices.push(i * step);
                }
            }
            SlicingMode::Beat => {
                // 16th note beat slices assuming 120 BPM
                let samples_per_beat = (self.sample_sample_rate as f32 * 0.5) as usize; // 120 BPM = 0.5s/beat
                let step = (samples_per_beat / 4).max(512);
                let mut pos = 0;
                while pos < total_samples && self.slices.len() < MAX_SLICES {
                    self.slices.push(pos);
                    pos += step;
                }
            }
            SlicingMode::Transient => {
                // Spectral flux transient detector: detect energy bursts
                self.slices.push(0);
                if let Some(ref data) = self.sample_data {
                    let ch = self.sample_channels.max(1) as usize;
                    let block_size = 256;
                    let mut prev_energy = 0.0f32;
                    let mut i = block_size;
                    while i + block_size < total_samples && self.slices.len() < MAX_SLICES {
                        let mut curr_energy = 0.0f32;
                        for b in 0..block_size {
                            let s = data[(i + b) * ch];
                            curr_energy += s * s;
                        }
                        // Transient flux threshold
                        if curr_energy > 0.05 && curr_energy > prev_energy * 2.2 {
                            self.slices.push(i);
                            i += block_size * 8; // Retrigger guard
                        } else {
                            i += block_size;
                        }
                        prev_energy = curr_energy;
                    }
                }
            }
            SlicingMode::Manual => {
                if self.slices.is_empty() {
                    self.slices.push(0);
                }
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

        if self.sample_channels == 0 {
            return;
        }

        let samples = match &self.sample_data {
            Some(s) if !s.is_empty() => s,
            _ => return,
        };
        let total_frames = samples.len() / (self.sample_channels as usize);

        let idx = self.allocate_voice();
        self.voice_counter += 1;
        let v = &mut self.voices[idx];
        v.active = true;
        v.note = note;
        v.velocity = velocity as f32 / 127.0;
        v.age = self.voice_counter;
        v.filter.reset();
        v.played_samples = 0;

        let pitch_ratio = match self.mode {
            SimplerMode::Classic => {
                let note_diff = note as f32 - 60.0 + self.transpose_semitones as f32 + self.detune_cents * 0.01;
                2.0_f64.powf(note_diff as f64 / 12.0)
            }
            SimplerMode::OneShot => {
                let note_diff = self.transpose_semitones as f32 + self.detune_cents * 0.01;
                2.0_f64.powf(note_diff as f64 / 12.0)
            }
            SimplerMode::Slicing => {
                // Slicing plays at original pitch
                1.0
            }
        };

        let sr_ratio = self.sample_sample_rate as f64 / self.sample_rate as f64;
        v.phase_step = pitch_ratio * sr_ratio;

        match self.mode {
            SimplerMode::Classic | SimplerMode::OneShot => {
                let start_idx = ((self.start_pos.clamp(0.0, 1.0) * total_frames as f32) as usize).min(total_frames.saturating_sub(1));
                let len = ((self.length_pos.clamp(0.01, 1.0) * total_frames as f32) as usize).max(1);
                v.slice_start = start_idx;
                v.slice_end = (start_idx + len).min(total_frames);
                v.position = start_idx as f64;
            }
            SimplerMode::Slicing => {
                // Note maps to slice index starting from slice_base_note (C1 = 36)
                let slice_idx = if note >= self.slice_base_note {
                    (note - self.slice_base_note) as usize
                } else {
                    0
                };

                if !self.slices.is_empty() {
                    let s_idx = slice_idx.min(self.slices.len() - 1);
                    v.slice_start = self.slices[s_idx];
                    v.slice_end = if s_idx + 1 < self.slices.len() {
                        self.slices[s_idx + 1]
                    } else {
                        total_frames
                    };
                    v.position = v.slice_start as f64;
                } else {
                    v.slice_start = 0;
                    v.slice_end = total_frames;
                    v.position = 0.0;
                }
            }
        }

        v.fade_in_samples = (self.fade_in_ms * 0.001 * self.sample_rate) as usize;
        v.fade_out_samples = (self.fade_out_ms * 0.001 * self.sample_rate) as usize;

        v.amp_env.trigger_attack();
        v.filter_env.trigger_attack();
        v.pitch_env.trigger_attack();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                if self.mode == SimplerMode::OneShot && self.oneshot_retrigger == OneShotRetrigger::Trigger {
                    // Trigger mode ignores NoteOff and plays out to end
                    continue;
                }
                v.amp_env.trigger_release();
                v.filter_env.trigger_release();
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
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        if self.sample_channels == 0 {
            return (0.0, 0.0);
        }

        let samples = match &self.sample_data {
            Some(s) if !s.is_empty() => s,
            _ => return (0.0, 0.0),
        };
        let ch = self.sample_channels as usize;
        let total_frames = samples.len() / ch;
        if total_frames == 0 {
            return (0.0, 0.0);
        }

        let base_vol = self.master_volume;
        let pan_angle = (self.master_pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * std::f32::consts::PI;
        let left_pan = pan_angle.cos();
        let right_pan = pan_angle.sin();

        // LFO
        let (lfo_sig, _) = self.lfo.next_sample(self.lfo_freq, 0.0, 0.0, self.lfo_shape, sr);

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

            voice.played_samples += 1;

            let curr_pos = voice.position;

            // OneShot fade envelope (fade-in and fade-out)
            let fade_factor = if self.mode == SimplerMode::OneShot {
                let fi = if voice.fade_in_samples > 0 && voice.played_samples < voice.fade_in_samples {
                    voice.played_samples as f32 / voice.fade_in_samples as f32
                } else {
                    1.0
                };
                let remaining_frames = (voice.slice_end as f64 - curr_pos).max(0.0) as usize;
                let fo = if voice.fade_out_samples > 0 && remaining_frames < voice.fade_out_samples {
                    remaining_frames as f32 / voice.fade_out_samples as f32
                } else {
                    1.0
                };
                (fi * fo).clamp(0.0, 1.0)
            } else {
                1.0
            };

            // Interpolated sample reading
            if curr_pos >= voice.slice_end as f64 {
                if self.mode == SimplerMode::Classic && self.loop_enabled {
                    voice.position = voice.slice_start as f64;
                } else {
                    voice.active = false;
                    continue;
                }
            }

            let idx0 = (curr_pos.floor() as usize).min(total_frames.saturating_sub(1));
            let idx1 = (idx0 + 1).min(total_frames.saturating_sub(1));
            let frac = (curr_pos - idx0 as f64) as f32;

            let s0_l = samples[idx0 * ch];
            let s1_l = samples[idx1 * ch];
            let interp_l = s0_l + frac * (s1_l - s0_l);

            let interp_r = if ch > 1 {
                let s0_r = samples[idx0 * ch + 1];
                let s1_r = samples[idx1 * ch + 1];
                s0_r + frac * (s1_r - s0_r)
            } else {
                interp_l
            };

            // Pitch envelope & LFO pitch modulation
            let pitch_env_val = voice.pitch_env.next(
                self.pitch_adsr.0,
                self.pitch_adsr.1,
                self.pitch_adsr.2,
                self.pitch_adsr.3,
            );
            let pitch_mod_semi = pitch_env_val * self.pitch_env_semitones + lfo_sig * self.lfo_to_pitch * 12.0;
            let effective_step = voice.phase_step * 2.0_f64.powf(pitch_mod_semi as f64 / 12.0);

            // Advance playback position
            voice.position += effective_step;

            // Filter envelope & LFO filter cutoff modulation
            let filter_env_val = voice.filter_env.next(
                self.filter_adsr.0,
                self.filter_adsr.1,
                self.filter_adsr.2,
                self.filter_adsr.3,
            );

            let (proc_l, proc_r) = if self.filter_enabled {
                voice.filter.filter_type = self.filter_type;
                voice.filter.circuit_model = self.filter_model;
                let cutoff_mod_oct = filter_env_val * self.filter_env_amt * 4.0 + lfo_sig * self.lfo_to_filter * 3.0;
                let mod_cutoff = (self.filter_cutoff * 2.0f32.powf(cutoff_mod_oct)).clamp(20.0, 20000.0);
                let fl = voice.filter.process_sample(interp_l, mod_cutoff, self.filter_res, 0.0, 0.0);
                let fr = voice.filter.process_sample(interp_r, mod_cutoff, self.filter_res, 0.0, 0.0);
                (fl, fr)
            } else {
                (interp_l, interp_r)
            };

            let voice_gain = amp_val * fade_factor * voice.velocity * base_vol;
            out_l += proc_l * voice_gain * left_pan;
            out_r += proc_r * voice_gain * right_pan;
        }

        (out_l, out_r)
    }
}

impl Device for SimplerDevice {
    fn name(&self) -> &str {
        "Simpler"
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

impl InstrumentDevice for SimplerDevice {
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
    fn test_simpler_classic_and_slicing() {
        let mut simpler = SimplerDevice::new(44100.0);
        assert_eq!(simpler.mode, SimplerMode::Classic);

        simpler.trigger_note_on(60, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        simpler.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i] * buf.left[i];
        }
        assert!(energy > 0.0001, "Simpler must render audible audio in Classic mode");

        // Switch to Slicing
        simpler.mode = SimplerMode::Slicing;
        simpler.slicing_mode = SlicingMode::Region;
        simpler.recalculate_slices();
        assert_eq!(simpler.slices.len(), 16);

        // Trigger slice 0 (MIDI 36)
        simpler.trigger_note_on(36, 120);
        let mut buf2 = AudioBuffer::silence(256);
        simpler.process(&[], &mut buf2, &time);
        let mut slice_energy = 0.0f32;
        for i in 0..256 {
            slice_energy += buf2.left[i] * buf2.left[i];
        }
        assert!(slice_energy > 0.0001, "Simpler must render slice 0 audio");
    }

    #[test]
    fn test_simpler_zero_channels_guard() {
        let mut simpler = SimplerDevice::new(44100.0);
        simpler.sample_channels = 0;
        simpler.trigger_note_on(60, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        simpler.process(&[], &mut buf, &time);

        let energy: f32 = buf.left.iter().map(|s| s * s).sum();
        assert_eq!(energy, 0.0, "Simpler with zero channels must safely produce silence without panicking");
    }

    #[test]
    fn test_simpler_pitch_and_filter_modulation() {
        // Run with neutral modulation
        let mut simpler1 = SimplerDevice::new(44100.0);
        simpler1.pitch_env_semitones = 0.0;
        simpler1.lfo_to_pitch = 0.0;
        simpler1.filter_enabled = true;
        simpler1.filter_cutoff = 1000.0;
        simpler1.filter_env_amt = 0.0;
        simpler1.trigger_note_on(60, 100);

        let mut buf1 = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        simpler1.process(&[], &mut buf1, &time);

        // Run with active pitch and filter envelope modulation
        let mut simpler2 = SimplerDevice::new(44100.0);
        simpler2.pitch_env_semitones = 12.0;
        simpler2.pitch_adsr = (0.001, 0.2, 0.5, 0.2);
        simpler2.lfo_to_pitch = 0.5;
        simpler2.filter_enabled = true;
        simpler2.filter_cutoff = 1000.0;
        simpler2.filter_env_amt = 1.0;
        simpler2.filter_adsr = (0.001, 0.2, 0.5, 0.2);
        simpler2.lfo_to_filter = 0.8;
        simpler2.trigger_note_on(60, 100);

        let mut buf2 = AudioBuffer::silence(256);
        simpler2.process(&[], &mut buf2, &time);

        let mut diff = 0.0f32;
        for i in 0..256 {
            diff += (buf1.left[i] - buf2.left[i]).abs();
        }
        assert!(
            diff > 0.01,
            "Pitch and filter envelope/LFO modulation must produce distinct audio compared to neutral"
        );
    }
}
