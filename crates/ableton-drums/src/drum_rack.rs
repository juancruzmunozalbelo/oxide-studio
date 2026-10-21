//! 128-pad Drum Rack with 8x16 banks, 32-voice static pool, choke groups, solo/mute, and sends.

use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::synth_models::{SynthDrumType, SynthPercussion};
use crate::voice::DrumVoice;
use crate::wav_loader::load_wav_file;

pub const MAX_DRUM_VOICES: usize = 32;
pub const NUM_DRUM_PADS: usize = 16;
pub const TOTAL_DRUM_PADS: usize = 128;
pub const PADS_PER_BANK: usize = 16;
pub const NUM_BANKS: usize = 8;

#[derive(Clone)]
pub enum DrumPadSource {
    Sample {
        sample_data: Arc<[f32]>,
        channels: u8,
        sample_rate: u32,
    },
    Synth(SynthDrumType),
}

#[derive(Clone)]
pub struct DrumPad {
    pub name: String,
    pub midi_note: u8,
    pub source: DrumPadSource,
    pub volume: f32,
    pub pan: f32, // [-1.0, 1.0]
    pub pitch_semitones: f32, // [-24.0, 24.0]
    pub decay_seconds: f32,
    pub choke_group: Option<u8>, // 1..16
    pub use_cubic_interpolation: bool,
    pub solo: bool,
    pub mute: bool,
    pub send_a: f32, // 0.0 .. 1.0
    pub send_b: f32, // 0.0 .. 1.0
}

pub struct DrumRack {
    pub enabled: bool,
    pub pads: [DrumPad; NUM_DRUM_PADS], // Active visible bank of 16 pads
    pub all_pads: Vec<DrumPad>,          // Full 128-pad bank (8 banks x 16 pads)
    pub active_bank: usize,              // 0..7
    pub voices: [DrumVoice; MAX_DRUM_VOICES],
    pub sample_rate: f32,
    pub return_a_level: f32,
    pub return_b_level: f32,
    // Return chains state
    pub return_a_buffer_l: [f32; 2048],
    pub return_a_buffer_r: [f32; 2048],
    pub return_a_pos: usize,
    pub return_a_filter_l: f32,
    pub return_a_filter_r: f32,
    pub return_b_filter_l: f32,
    pub return_b_filter_r: f32,
}

impl DrumRack {
    pub fn default_pad_voice(idx: usize) -> (SynthDrumType, f32) {
        match idx % 16 {
            0 => (SynthDrumType::Kick909, 0.30),
            1 => (SynthDrumType::Snare909, 0.22),
            2 => (SynthDrumType::LowTom909, 0.28),
            3 => (SynthDrumType::MidTom909, 0.24),
            4 => (SynthDrumType::HiTom909, 0.20),
            5 => (SynthDrumType::Rim909, 0.09),
            6 => (SynthDrumType::Clap909, 0.22),
            7 => (SynthDrumType::Cowbell909, 0.16),
            8 => (SynthDrumType::HihatClosed909, 0.05),
            9 => (SynthDrumType::HihatOpen909, 0.38),
            10 => (SynthDrumType::Crash909, 0.65),
            11 => (SynthDrumType::Ride909, 0.48),
            12 => (SynthDrumType::PercLow, 0.14),
            13 => (SynthDrumType::PercHigh, 0.11),
            14 => (SynthDrumType::Shaker, 0.08),
            15 => (SynthDrumType::Tambourine, 0.18),
            _ => unreachable!(),
        }
    }

    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let pad_names = [
            "Kick", "Snare", "Low Tom", "Mid Tom", "Hi Tom", "Rimshot", "Clap", "Cowbell",
            "Closed HH", "Open HH", "Crash", "Ride", "Perc 1", "Perc 2", "Shaker", "Tambourine",
        ];

        // 1. Initialize Bank A (visible default 16 pads, MIDI notes 36..51)
        let default_bank_a: [DrumPad; NUM_DRUM_PADS] = std::array::from_fn(|i| {
            let choke = match i {
                6 | 8 | 9 | 10 => Some(1), // Choke Group 1: Hi-Hats
                _ => None,
            };
            let (synth_type, decay) = Self::default_pad_voice(i);
            DrumPad {
                name: pad_names[i].to_string(),
                midi_note: (36 + i) as u8,
                source: DrumPadSource::Synth(synth_type),
                volume: 1.0,
                pan: 0.0,
                pitch_semitones: 0.0,
                decay_seconds: decay,
                choke_group: choke,
                use_cubic_interpolation: false,
                solo: false,
                mute: false,
                send_a: 0.0,
                send_b: 0.0,
            }
        });

        // 2. Initialize all 128 pads across 8 banks
        let mut all_pads = Vec::with_capacity(TOTAL_DRUM_PADS);
        // Bank A is indices 0..16
        for p in &default_bank_a {
            all_pads.push(p.clone());
        }

        // Banks B..H (remaining 112 pads, covering notes 0..35 and 52..127)
        for i in 16..TOTAL_DRUM_PADS {
            let midi_note = if i < 52 {
                (i - 16) as u8 // Notes 0..35
            } else {
                i as u8        // Notes 52..127
            };
            let choke = match i % 16 {
                6 | 8 | 10 => Some(1),
                _ => None,
            };
            let (synth_type, decay) = Self::default_pad_voice(i);

            all_pads.push(DrumPad {
                name: format!("Pad {}", i + 1),
                midi_note,
                source: DrumPadSource::Synth(synth_type),
                volume: 1.0,
                pan: 0.0,
                pitch_semitones: 0.0,
                decay_seconds: decay,
                choke_group: choke,
                use_cubic_interpolation: false,
                solo: false,
                mute: false,
                send_a: 0.0,
                send_b: 0.0,
            });
        }

        Self {
            enabled: true,
            pads: default_bank_a,
            all_pads,
            active_bank: 0,
            voices: [DrumVoice::default(); MAX_DRUM_VOICES],
            sample_rate: sr,
            return_a_level: 1.0,
            return_b_level: 1.0,
            return_a_buffer_l: [0.0; 2048],
            return_a_buffer_r: [0.0; 2048],
            return_a_pos: 0,
            return_a_filter_l: 0.0,
            return_a_filter_r: 0.0,
            return_b_filter_l: 0.0,
            return_b_filter_r: 0.0,
        }
    }

    /// Select active 16-pad visible bank (0..7 for Banks A..H).
    pub fn select_bank(&mut self, bank_idx: usize) {
        if bank_idx >= NUM_BANKS {
            return;
        }
        // Save current visible pads back to all_pads
        let start = self.active_bank * PADS_PER_BANK;
        for i in 0..PADS_PER_BANK {
            if start + i < self.all_pads.len() {
                self.all_pads[start + i] = self.pads[i].clone();
            }
        }

        self.active_bank = bank_idx;
        let new_start = self.active_bank * PADS_PER_BANK;
        for i in 0..PADS_PER_BANK {
            if new_start + i < self.all_pads.len() {
                self.pads[i] = self.all_pads[new_start + i].clone();
            }
        }
    }

    /// Sync changes made to visible pads into all_pads.
    pub fn sync_visible_to_all(&mut self) {
        let start = self.active_bank * PADS_PER_BANK;
        for i in 0..PADS_PER_BANK {
            if start + i < self.all_pads.len() {
                self.all_pads[start + i] = self.pads[i].clone();
            }
        }
    }

    /// Load standard 909 sample assets into pads (Kick: pad 0, Snare: pad 1 & 2, Hihat: pad 6, 8, 9, 10).
    pub fn load_default_909_samples(&mut self, assets_dir: &str) {
        let dir = Path::new(assets_dir);
        let kick_path = dir.join("kick.wav");
        let snare_path = dir.join("snare.wav");
        let hihat_path = dir.join("hihat.wav");

        let _ = self.load_sample_pad(0, &kick_path);
        let _ = self.load_sample_pad(1, &snare_path);
        let _ = self.load_sample_pad(2, &snare_path);
        let _ = self.load_sample_pad(6, &hihat_path);
        let _ = self.load_sample_pad(8, &hihat_path);
        let _ = self.load_sample_pad(9, &hihat_path);
        let _ = self.load_sample_pad(10, &hihat_path);
    }

    /// Load a WAV file into a specific visible pad index [0..15].
    pub fn load_sample_pad<P: AsRef<Path>>(&mut self, pad_idx: usize, path: P) -> Result<(), String> {
        if pad_idx >= NUM_DRUM_PADS {
            return Err(format!("Pad index {} out of range", pad_idx));
        }

        let decoded = load_wav_file(path)?;
        let arc_data: Arc<[f32]> = Arc::from(decoded.samples.into_boxed_slice());
        self.pads[pad_idx].source = DrumPadSource::Sample {
            sample_data: arc_data.clone(),
            channels: decoded.channels as u8,
            sample_rate: decoded.sample_rate,
        };

        let all_idx = self.active_bank * PADS_PER_BANK + pad_idx;
        if all_idx < self.all_pads.len() {
            self.all_pads[all_idx].source = DrumPadSource::Sample {
                sample_data: arc_data,
                channels: decoded.channels as u8,
                sample_rate: decoded.sample_rate,
            };
        }
        Ok(())
    }

    /// Trigger pad by index [0..15] with velocity [1..127].
    pub fn trigger_pad(&mut self, pad_idx: u8, velocity: u8) {
        if pad_idx as usize >= NUM_DRUM_PADS || velocity == 0 {
            return;
        }

        let global_idx = self.active_bank * PADS_PER_BANK + pad_idx as usize;
        if global_idx >= self.all_pads.len() {
            return;
        }

        // Sync pad DSP parameters from visible bank to all_pads without heap allocation
        {
            let vis = &self.pads[pad_idx as usize];
            let all = &mut self.all_pads[global_idx];
            all.source = vis.source.clone();
            all.volume = vis.volume;
            all.pan = vis.pan;
            all.pitch_semitones = vis.pitch_semitones;
            all.decay_seconds = vis.decay_seconds;
            all.choke_group = vis.choke_group;
            all.use_cubic_interpolation = vis.use_cubic_interpolation;
            all.solo = vis.solo;
            all.mute = vis.mute;
            all.send_a = vis.send_a;
            all.send_b = vis.send_b;
        }

        let pad = &self.all_pads[global_idx];

        // Solo & Mute check
        let any_solo = self.all_pads.iter().any(|p| p.solo);
        if pad.mute || (any_solo && !pad.solo) {
            return;
        }

        // 1. Choke group dampening: clickless exponential fadeout in <5ms (<0.001 / -60 dBFS)
        if let Some(target_choke) = pad.choke_group {
            for voice in &mut self.voices {
                if voice.active && voice.choke_group == Some(target_choke) {
                    voice.is_fading_out = true;
                    voice.fade_out_coeff = (-6.907755 / (0.005 * self.sample_rate)).exp();
                }
            }
        }

        // 2. Static voice allocation
        let voice_idx = self.allocate_voice();
        let voice = &mut self.voices[voice_idx];
        voice.active = true;
        voice.pad_idx = global_idx as u8;
        voice.choke_group = pad.choke_group;
        voice.phase = 0.0;
        voice.velocity = (velocity as f32) / 127.0;
        voice.is_fading_out = false;
        voice.synth_phase = 0.0;
        voice.synth_phase_2 = 0.0;

        let pitch_ratio = 2.0_f64.powf(pad.pitch_semitones as f64 / 12.0);
        let sr_ratio = match &pad.source {
            DrumPadSource::Sample { sample_rate: wav_sr, .. } => *wav_sr as f64 / self.sample_rate as f64,
            DrumPadSource::Synth(_) => 1.0,
        };
        voice.phase_step = pitch_ratio * sr_ratio;

        voice.envelope_val = 1.0;
        let decay_samples = (pad.decay_seconds.max(0.005) * self.sample_rate).max(1.0);
        voice.envelope_decay = (-4.60517 / decay_samples).exp();
    }

    /// Trigger pad by MIDI note number (0..127).
    pub fn trigger_midi_note(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            return;
        }

        // Check if note is in the current visible bank
        for (i, pad) in self.pads.iter().enumerate() {
            if pad.midi_note == note {
                self.trigger_pad(i as u8, velocity);
                return;
            }
        }

        // Check across all 128 pads
        for (i, pad) in self.all_pads.iter().enumerate() {
            if pad.midi_note == note {
                // Solo & Mute check
                let any_solo = self.all_pads.iter().any(|p| p.solo);
                if pad.mute || (any_solo && !pad.solo) {
                    return;
                }

                if let Some(target_choke) = pad.choke_group {
                    for voice in &mut self.voices {
                        if voice.active && voice.choke_group == Some(target_choke) {
                            voice.is_fading_out = true;
                            voice.fade_out_coeff = (-6.907755 / (0.005 * self.sample_rate)).exp();
                        }
                    }
                }

                let voice_idx = self.allocate_voice();
                let voice = &mut self.voices[voice_idx];
                voice.active = true;
                voice.pad_idx = i as u8;
                voice.choke_group = pad.choke_group;
                voice.phase = 0.0;
                voice.velocity = (velocity as f32) / 127.0;
                voice.is_fading_out = false;
                voice.synth_phase = 0.0;
                voice.synth_phase_2 = 0.0;

                let pitch_ratio = 2.0_f64.powf(pad.pitch_semitones as f64 / 12.0);
                let sr_ratio = match &pad.source {
                    DrumPadSource::Sample { sample_rate: wav_sr, .. } => *wav_sr as f64 / self.sample_rate as f64,
                    DrumPadSource::Synth(_) => 1.0,
                };
                voice.phase_step = pitch_ratio * sr_ratio;

                voice.envelope_val = 1.0;
                let decay_samples = (pad.decay_seconds.max(0.005) * self.sample_rate).max(1.0);
                voice.envelope_decay = (-4.60517 / decay_samples).exp();
                return;
            }
        }
    }

    /// Selects an inactive voice, or steals the quietest active voice.
    pub fn allocate_voice(&self) -> usize {
        for (i, v) in self.voices.iter().enumerate() {
            if !v.active {
                return i;
            }
        }
        let mut min_val = f32::MAX;
        let mut min_idx = 0;
        for (i, v) in self.voices.iter().enumerate() {
            if v.envelope_val < min_val {
                min_val = v.envelope_val;
                min_idx = i;
            }
        }
        min_idx
    }

    /// Steady-state block rendering with ZERO dynamic allocations.
    pub fn process_block(&mut self, out_left: &mut [f32], out_right: &mut [f32]) {
        let block_len = out_left.len().min(out_right.len());
        if block_len == 0 {
            return;
        }

        let n_frames = block_len.min(1024);
        let mut send_a_l = [0.0f32; 1024];
        let mut send_a_r = [0.0f32; 1024];
        let mut send_b_l = [0.0f32; 1024];
        let mut send_b_r = [0.0f32; 1024];

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            let pad_idx = voice.pad_idx as usize;
            if pad_idx >= self.all_pads.len() {
                voice.active = false;
                continue;
            }
            let pad = &self.all_pads[pad_idx];

            let pan_norm = pad.pan.clamp(-1.0, 1.0);
            let theta = (pan_norm + 1.0) * 0.25 * PI;
            let pan_l = theta.cos();
            let pan_r = theta.sin();
            let gain = pad.volume * voice.velocity;
            let send_a_amt = pad.send_a.clamp(0.0, 1.0);
            let send_b_amt = pad.send_b.clamp(0.0, 1.0);

            for i in 0..n_frames {
                let sample = match &pad.source {
                    DrumPadSource::Sample { sample_data, channels, .. } => {
                        let ch = (*channels as usize).max(1);
                        if sample_data.is_empty() || *channels == 0 {
                            voice.active = false;
                            break;
                        }
                        let total_frames = sample_data.len() / ch;
                        if total_frames == 0 || voice.phase as usize >= total_frames {
                            voice.active = false;
                            break;
                        }

                        let s = if pad.use_cubic_interpolation {
                            DrumVoice::interpolate_cubic(sample_data, ch, voice.phase)
                        } else {
                            DrumVoice::interpolate_linear(sample_data, ch, voice.phase)
                        };
                        voice.phase += voice.phase_step;
                        s
                    }
                    DrumPadSource::Synth(synth_type) => {
                        SynthPercussion::render_sample(
                            *synth_type,
                            &mut voice.synth_phase,
                            &mut voice.synth_phase_2,
                            voice.envelope_val,
                            self.sample_rate,
                            &mut voice.rng_state,
                        )
                    }
                };

                voice.envelope_val *= voice.envelope_decay;
                if voice.is_fading_out {
                    voice.envelope_val *= voice.fade_out_coeff;
                }
                if voice.envelope_val < 1e-4 {
                    voice.active = false;
                    break;
                }

                let sig = sample * voice.envelope_val * gain;
                let sig_l = sig * pan_l;
                let sig_r = sig * pan_r;

                out_left[i] += sig_l;
                out_right[i] += sig_r;

                if send_a_amt > 0.0 {
                    send_a_l[i] += sig_l * send_a_amt;
                    send_a_r[i] += sig_r * send_a_amt;
                }
                if send_b_amt > 0.0 {
                    send_b_l[i] += sig_l * send_b_amt;
                    send_b_r[i] += sig_r * send_b_amt;
                }
            }
        }

        // Return chains processing
        let delay_len_l = 882;
        let delay_len_r = 1146;
        for i in 0..n_frames {
            // Return Chain A: Stereo space delay with damping
            let read_l = (self.return_a_pos + 2048 - delay_len_l) % 2048;
            let read_r = (self.return_a_pos + 2048 - delay_len_r) % 2048;
            let del_l = self.return_a_buffer_l[read_l];
            let del_r = self.return_a_buffer_r[read_r];

            self.return_a_filter_l += 0.35 * (del_l - self.return_a_filter_l);
            self.return_a_filter_r += 0.35 * (del_r - self.return_a_filter_r);

            self.return_a_buffer_l[self.return_a_pos] = send_a_l[i] + self.return_a_filter_r * 0.4;
            self.return_a_buffer_r[self.return_a_pos] = send_a_r[i] + self.return_a_filter_l * 0.4;
            self.return_a_pos = (self.return_a_pos + 1) % 2048;

            let ret_a_l = self.return_a_filter_l * self.return_a_level;
            let ret_a_r = self.return_a_filter_r * self.return_a_level;

            // Return Chain B: Warm saturation
            let sat_in_l = send_b_l[i] * 2.5;
            let sat_in_r = send_b_r[i] * 2.5;
            let sat_l = sat_in_l.tanh() * 0.6;
            let sat_r = sat_in_r.tanh() * 0.6;

            self.return_b_filter_l += 0.5 * (sat_l - self.return_b_filter_l);
            self.return_b_filter_r += 0.5 * (sat_r - self.return_b_filter_r);

            let ret_b_l = self.return_b_filter_l * self.return_b_level;
            let ret_b_r = self.return_b_filter_r * self.return_b_level;

            out_left[i] += ret_a_l + ret_b_l;
            out_right[i] += ret_a_r + ret_b_r;
        }
    }

    /// Renders into an `ableton_core::AudioBuffer` with zero allocations.
    pub fn process_audio_buffer(&mut self, buffer: &mut AudioBuffer) {
        let count = buffer.frame_count.min(1024);
        self.process_block(&mut buffer.left[..count], &mut buffer.right[..count]);
    }
}

impl Device for DrumRack {
    fn name(&self) -> &str {
        "Drum Rack"
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
        for v in &mut self.voices {
            v.active = false;
        }
    }
}

impl InstrumentDevice for DrumRack {
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
            if let MidiEvent::NoteOn { note, velocity, .. } = *event {
                self.trigger_midi_note(note, velocity);
            }
        }

        self.process_audio_buffer(audio_out);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.trigger_midi_note(note, velocity);
    }

    fn note_off(&mut self, _note: u8) {}

    fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drum_rack_multi_bank_all_128_pads() {
        let mut rack = DrumRack::new(44100.0);
        // Bank A: pad 0 (MIDI 36)
        rack.all_pads[0].volume = 0.5;
        // Bank B: pad 16 (MIDI 0)
        rack.all_pads[16].volume = 0.8;
        // Bank D: pad 52 (MIDI 52)
        rack.all_pads[52].volume = 0.7;
        // Bank H: pad 127
        rack.all_pads[127].volume = 0.2;

        rack.trigger_midi_note(36, 100);
        let v0 = rack.voices.iter().find(|v| v.active && v.pad_idx == 0);
        assert!(v0.is_some(), "MIDI 36 should trigger pad 0");

        rack.trigger_midi_note(0, 100);
        let v16 = rack.voices.iter().find(|v| v.active && v.pad_idx == 16);
        assert!(v16.is_some(), "MIDI 0 should trigger pad 16 in Bank B without aliasing to 0");

        rack.trigger_midi_note(52, 100);
        let v52 = rack.voices.iter().find(|v| v.active && v.pad_idx == 52);
        assert!(v52.is_some(), "MIDI 52 should trigger pad 52 in Bank D without aliasing to Bank A");

        rack.select_bank(7); // Bank H (pads 112..127)
        rack.trigger_pad(15, 100); // 7 * 16 + 15 = 127
        let v127 = rack.voices.iter().find(|v| v.active && v.pad_idx == 127);
        assert!(v127.is_some(), "Triggering visible pad 15 in Bank H should activate voice with pad_idx 127");
    }

    #[test]
    fn test_drum_rack_return_chains_send_a_and_b() {
        let mut rack = DrumRack::new(44100.0);
        // Mute dry pad volume to isolate send output
        rack.all_pads[0].volume = 0.0;
        rack.all_pads[0].send_a = 0.8;
        rack.all_pads[0].send_b = 0.5;
        rack.return_a_level = 1.0;
        rack.return_b_level = 1.0;

        rack.trigger_pad(0, 127);

        let mut left = [0.0; 512];
        let mut right = [0.0; 512];
        rack.process_block(&mut left, &mut right);

        let energy_l: f32 = left.iter().map(|s| s * s).sum();
        let energy_r: f32 = right.iter().map(|s| s * s).sum();
        assert!(
            energy_l > 0.0001 && energy_r > 0.0001,
            "Return chains A and B should produce audible wet audio even with zero dry gain"
        );
    }

    #[test]
    fn test_all_16_drum_pads_distinct_synthesis() {
        let mut rack = DrumRack::new(44100.0);
        let mut pad_outputs: Vec<Vec<f32>> = Vec::new();
        let mut zero_crossings: Vec<usize> = Vec::new();

        for pad in 0..16 {
            rack.reset();
            rack.trigger_pad(pad as u8, 127);
            let mut left = [0.0; 512];
            let mut right = [0.0; 512];
            rack.process_block(&mut left, &mut right);

            let energy: f32 = left.iter().map(|s| s * s).sum();
            assert!(
                energy > 0.001,
                "Pad {} must produce audible non-zero energy, got {}",
                pad,
                energy
            );

            let mut zc = 0;
            for w in left.windows(2) {
                if (w[0] >= 0.0 && w[1] < 0.0) || (w[0] < 0.0 && w[1] >= 0.0) {
                    zc += 1;
                }
            }
            zero_crossings.push(zc);
            pad_outputs.push(left.to_vec());
        }

        // Verify Kick (Pad 0) has low-frequency character (low zero-crossings)
        let kick_zc = zero_crossings[0];
        assert!(kick_zc <= 20, "Kick should have few zero-crossings, got {}", kick_zc);

        // Verify high-frequency / metallic percussion (Closed HH 8, Open HH 9, Crash 10, Ride 11, Shaker 14, Tambourine 15)
        // have significantly higher zero-crossings and do NOT sound like Kick!
        for &hi_pad in &[8, 9, 10, 11, 14, 15] {
            assert!(
                zero_crossings[hi_pad] >= 25,
                "Pad {} (zc: {}) must have high frequency content distinct from Kick (zc: {})",
                hi_pad,
                zero_crossings[hi_pad],
                kick_zc
            );
        }

        // Verify all 16 pads have unique waveforms (pairwise comparison)
        for i in 0..16 {
            for j in (i + 1)..16 {
                let diff: f32 = pad_outputs[i]
                    .iter()
                    .zip(pad_outputs[j].iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum();
                assert!(
                    diff > 0.05,
                    "Pads {} and {} must have distinct audio output, diff was {}",
                    i,
                    j,
                    diff
                );
            }
        }
    }
}
