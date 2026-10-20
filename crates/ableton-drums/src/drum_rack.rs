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
}

impl DrumRack {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let pad_names = [
            "Kick", "Rim", "Snare", "Clap", "Elec Snare", "Low Floor Tom",
            "Closed HH", "High Floor Tom", "Pedal HH", "Low Tom", "Open HH",
            "Mid Tom", "High Tom", "Crash", "Ride", "Perc",
        ];

        // 1. Initialize Bank A (visible default 16 pads, MIDI notes 36..51)
        let default_bank_a: [DrumPad; NUM_DRUM_PADS] = std::array::from_fn(|i| {
            let choke = match i {
                6 | 8 | 10 => Some(1), // Choke Group 1: Closed HH, Pedal HH, Open HH
                _ => None,
            };
            DrumPad {
                name: pad_names[i].to_string(),
                midi_note: (36 + i) as u8,
                source: DrumPadSource::Synth(match i {
                    0 => SynthDrumType::Kick909,
                    1 => SynthDrumType::Rim909,
                    2 => SynthDrumType::Snare909,
                    3 => SynthDrumType::Clap909,
                    6 => SynthDrumType::HihatClosed909,
                    8 => SynthDrumType::HihatClosed909,
                    10 => SynthDrumType::HihatOpen909,
                    _ => SynthDrumType::Kick909,
                }),
                volume: 1.0,
                pan: 0.0,
                pitch_semitones: 0.0,
                decay_seconds: match i {
                    6 | 8 => 0.05,
                    10 => 0.35,
                    _ => 0.25,
                },
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
            let midi_note = if i < 36 {
                (i - 16) as u8 // Notes 0..19
            } else if i < 52 {
                (i - 16) as u8 // Notes 20..35
            } else {
                i as u8        // Notes 52..127
            };

            all_pads.push(DrumPad {
                name: format!("Pad {}", i + 1),
                midi_note,
                source: DrumPadSource::Synth(SynthDrumType::Kick909),
                volume: 1.0,
                pan: 0.0,
                pitch_semitones: 0.0,
                decay_seconds: 0.25,
                choke_group: None,
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

    /// Load standard 909 sample assets into pads (Kick: pad 0, Snare: pad 2, Hihat: pad 6 & 10).
    pub fn load_default_909_samples(&mut self, assets_dir: &str) {
        let dir = Path::new(assets_dir);
        let kick_path = dir.join("kick.wav");
        let snare_path = dir.join("snare.wav");
        let hihat_path = dir.join("hihat.wav");

        let _ = self.load_sample_pad(0, kick_path);
        let _ = self.load_sample_pad(2, snare_path);
        let _ = self.load_sample_pad(6, &hihat_path);
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

        self.sync_visible_to_all();
        let pad = &self.pads[pad_idx as usize];

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
        voice.pad_idx = pad_idx;
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
                voice.pad_idx = (i % PADS_PER_BANK) as u8;
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
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            let pad = &self.pads[voice.pad_idx as usize];

            let pan_norm = pad.pan.clamp(-1.0, 1.0);
            let theta = (pan_norm + 1.0) * 0.25 * PI;
            let pan_l = theta.cos();
            let pan_r = theta.sin();
            let gain = pad.volume * voice.velocity;

            for i in 0..block_len {
                let sample = match &pad.source {
                    DrumPadSource::Sample { sample_data, channels, .. } => {
                        let ch = *channels as usize;
                        let total_frames = sample_data.len() / ch;
                        if voice.phase as usize >= total_frames {
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
                out_left[i] += sig * pan_l;
                out_right[i] += sig * pan_r;
            }
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
            match *event {
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.trigger_midi_note(note, velocity);
                }
                _ => {}
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
