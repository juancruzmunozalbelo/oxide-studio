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
    Simpler(Box<crate::simpler::SimplerDevice>),
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

impl DrumPad {
    pub fn load_simpler(&mut self, simpler: crate::simpler::SimplerDevice) {
        self.source = DrumPadSource::Simpler(Box::new(simpler));
    }

    pub fn has_simpler(&self) -> bool {
        matches!(self.source, DrumPadSource::Simpler(_))
    }

    pub fn simpler(&self) -> Option<&crate::simpler::SimplerDevice> {
        match &self.source {
            DrumPadSource::Simpler(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    pub fn simpler_mut(&mut self) -> Option<&mut crate::simpler::SimplerDevice> {
        match &mut self.source {
            DrumPadSource::Simpler(s) => Some(s.as_mut()),
            _ => None,
        }
    }
}

/// Target destination for Drum Rack macro control mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumRackMacroTarget {
    PadVolume(u8),     // Global pad index [0..127]
    PadPan(u8),        // Global pad index [0..127]
    PadPitch(u8),      // Global pad index [0..127]
    PadDecay(u8),      // Global pad index [0..127]
    PadSendA(u8),      // Global pad index [0..127]
    PadSendB(u8),      // Global pad index [0..127]
    ReturnALevel,      // Return chain A master level
    ReturnBLevel,      // Return chain B master level
    AllPadsVolume,     // Global volume master trim
    AllPadsDecay,      // Global decay multiplier
}

/// Parameter modulation routing from macro knob to Drum Rack destination.
#[derive(Debug, Clone, PartialEq)]
pub struct DrumRackMacroMapping {
    pub macro_idx: usize,
    pub target: DrumRackMacroTarget,
    pub range: ableton_core::MacroMappingRange,
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
    // Live 12 Chapter 24 Macro Controls (1..16) & Macro Variations
    pub macro_system: ableton_core::RackMacroSystem,
    pub macro_mappings: Vec<DrumRackMacroMapping>,
    pub rng_state: u32,
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
            macro_system: ableton_core::RackMacroSystem::new(ableton_core::DEFAULT_RACK_MACROS),
            macro_mappings: Vec::new(),
            rng_state: 0x9091982,
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

    /// Load a Simpler instance into a specific visible pad index [0..15].
    pub fn load_simpler_pad(&mut self, pad_idx: usize, simpler: crate::simpler::SimplerDevice) -> Result<(), String> {
        if pad_idx >= NUM_DRUM_PADS {
            return Err(format!("Pad index {} out of range", pad_idx));
        }
        self.pads[pad_idx].load_simpler(simpler.clone());
        let all_idx = self.active_bank * PADS_PER_BANK + pad_idx;
        if all_idx < self.all_pads.len() {
            self.all_pads[all_idx].load_simpler(simpler);
        }
        Ok(())
    }

    /// Load a Simpler instance into any pad across all 8 banks [0..127].
    pub fn load_simpler_pad_global(&mut self, global_idx: usize, simpler: crate::simpler::SimplerDevice) -> Result<(), String> {
        if global_idx >= self.all_pads.len() {
            return Err(format!("Global pad index {} out of range", global_idx));
        }
        self.all_pads[global_idx].load_simpler(simpler.clone());
        let bank_start = self.active_bank * PADS_PER_BANK;
        if global_idx >= bank_start && global_idx < bank_start + PADS_PER_BANK {
            self.pads[global_idx - bank_start].load_simpler(simpler);
        }
        Ok(())
    }

    /// Set choke group (1..16 or None) for a pad across all 8 banks [0..127].
    pub fn set_pad_choke_group(&mut self, global_idx: usize, choke: Option<u8>) {
        if global_idx < self.all_pads.len() {
            self.all_pads[global_idx].choke_group = choke;
            let bank_start = self.active_bank * PADS_PER_BANK;
            if global_idx >= bank_start && global_idx < bank_start + PADS_PER_BANK {
                self.pads[global_idx - bank_start].choke_group = choke;
            }
        }
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

        // Bidirectional sync between visible bank and global pads
        {
            let vis = &mut self.pads[pad_idx as usize];
            let all = &mut self.all_pads[global_idx];
            if all.choke_group.is_some() && vis.choke_group.is_none() {
                vis.choke_group = all.choke_group;
            } else if vis.choke_group.is_some() && all.choke_group.is_none() {
                all.choke_group = vis.choke_group;
            }
            if all.volume != 1.0 && vis.volume == 1.0 {
                vis.volume = all.volume;
            } else if vis.volume != 1.0 && all.volume == 1.0 {
                all.volume = vis.volume;
            }
            if all.pan != 0.0 && vis.pan == 0.0 {
                vis.pan = all.pan;
            } else if vis.pan != 0.0 && all.pan == 0.0 {
                all.pan = vis.pan;
            }
            if all.pitch_semitones != 0.0 && vis.pitch_semitones == 0.0 {
                vis.pitch_semitones = all.pitch_semitones;
            } else if vis.pitch_semitones != 0.0 && all.pitch_semitones == 0.0 {
                all.pitch_semitones = vis.pitch_semitones;
            }
            if all.send_a != 0.0 && vis.send_a == 0.0 {
                vis.send_a = all.send_a;
            } else if vis.send_a != 0.0 && all.send_a == 0.0 {
                all.send_a = vis.send_a;
            }
            if all.send_b != 0.0 && vis.send_b == 0.0 {
                vis.send_b = all.send_b;
            } else if vis.send_b != 0.0 && all.send_b == 0.0 {
                all.send_b = vis.send_b;
            }
            if all.solo || vis.solo {
                all.solo = true;
                vis.solo = true;
            }
            if all.mute || vis.mute {
                all.mute = true;
                vis.mute = true;
            }
        }

        let pad_mute = self.all_pads[global_idx].mute;
        let pad_solo = self.all_pads[global_idx].solo;
        let pad_choke = self.all_pads[global_idx].choke_group;
        let pad_midi_note = self.all_pads[global_idx].midi_note;
        let pad_pitch_semitones = self.all_pads[global_idx].pitch_semitones;
        let pad_decay_seconds = self.all_pads[global_idx].decay_seconds;

        // Solo & Mute check
        let any_solo = self.all_pads.iter().any(|p| p.solo);
        if pad_mute || (any_solo && !pad_solo) {
            return;
        }

        // 1. Choke group dampening: clickless exponential fadeout in <5ms (<0.001 / -60 dBFS)
        if let Some(target_choke) = pad_choke {
            for voice in &mut self.voices {
                if voice.active && voice.choke_group == Some(target_choke) {
                    voice.is_fading_out = true;
                    voice.fade_out_coeff = (-6.907755 / (0.005 * self.sample_rate)).exp();
                }
            }
            for (p_idx, other_pad) in self.all_pads.iter_mut().enumerate() {
                if p_idx != global_idx && other_pad.choke_group == Some(target_choke) {
                    if let DrumPadSource::Simpler(ref mut s) = other_pad.source {
                        s.silence_all();
                    }
                }
            }
            for (v_idx, other_vis) in self.pads.iter_mut().enumerate() {
                if v_idx != pad_idx as usize && other_vis.choke_group == Some(target_choke) {
                    if let DrumPadSource::Simpler(ref mut s) = other_vis.source {
                        s.silence_all();
                    }
                }
            }
        }

        // 2. If pad source is Simpler, trigger simpler directly!
        if let DrumPadSource::Simpler(ref mut s) = self.all_pads[global_idx].source {
            s.trigger_note_on(pad_midi_note, velocity);
            if let DrumPadSource::Simpler(ref mut vis_s) = self.pads[pad_idx as usize].source {
                vis_s.trigger_note_on(pad_midi_note, velocity);
            }
            return;
        }

        // 3. Static voice allocation for Sample & Synth
        let voice_idx = self.allocate_voice();
        let voice = &mut self.voices[voice_idx];
        voice.active = true;
        voice.pad_idx = global_idx as u8;
        voice.choke_group = pad_choke;
        voice.phase = 0.0;
        voice.velocity = (velocity as f32) / 127.0;
        voice.is_fading_out = false;
        voice.synth_phase = 0.0;
        voice.synth_phase_2 = 0.0;

        let pitch_ratio = 2.0_f64.powf(pad_pitch_semitones as f64 / 12.0);
        let sr_ratio = match &self.all_pads[global_idx].source {
            DrumPadSource::Sample { sample_rate: wav_sr, .. } => *wav_sr as f64 / self.sample_rate as f64,
            DrumPadSource::Synth(_) => 1.0,
            DrumPadSource::Simpler(_) => 1.0,
        };
        voice.phase_step = pitch_ratio * sr_ratio;

        voice.envelope_val = 1.0;
        let decay_samples = (pad_decay_seconds.max(0.005) * self.sample_rate).max(1.0);
        voice.envelope_decay = (-4.60517 / decay_samples).exp();
    }

    /// Trigger pad by MIDI note number (0..127).
    pub fn trigger_midi_note(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            self.note_off(note);
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
        let target_idx = self.all_pads.iter().position(|p| p.midi_note == note);
        if let Some(i) = target_idx {
            let pad_mute = self.all_pads[i].mute;
            let pad_solo = self.all_pads[i].solo;
            let pad_choke = self.all_pads[i].choke_group;
            let pad_pitch = self.all_pads[i].pitch_semitones;
            let pad_decay = self.all_pads[i].decay_seconds;

            // Solo & Mute check
            let any_solo = self.all_pads.iter().any(|p| p.solo);
            if pad_mute || (any_solo && !pad_solo) {
                return;
            }

            if let Some(target_choke) = pad_choke {
                for voice in &mut self.voices {
                    if voice.active && voice.choke_group == Some(target_choke) {
                        voice.is_fading_out = true;
                        voice.fade_out_coeff = (-6.907755 / (0.005 * self.sample_rate)).exp();
                    }
                }
                for (p_idx, other_pad) in self.all_pads.iter_mut().enumerate() {
                    if p_idx != i && other_pad.choke_group == Some(target_choke) {
                        if let DrumPadSource::Simpler(ref mut s) = other_pad.source {
                            s.silence_all();
                        }
                    }
                }
            }

            if let DrumPadSource::Simpler(ref mut s) = self.all_pads[i].source {
                s.trigger_note_on(note, velocity);
                return;
            }

            let voice_idx = self.allocate_voice();
            let voice = &mut self.voices[voice_idx];
            voice.active = true;
            voice.pad_idx = i as u8;
            voice.choke_group = pad_choke;
            voice.phase = 0.0;
            voice.velocity = (velocity as f32) / 127.0;
            voice.is_fading_out = false;
            voice.synth_phase = 0.0;
            voice.synth_phase_2 = 0.0;

            let pitch_ratio = 2.0_f64.powf(pad_pitch as f64 / 12.0);
            let sr_ratio = match &self.all_pads[i].source {
                DrumPadSource::Sample { sample_rate: wav_sr, .. } => *wav_sr as f64 / self.sample_rate as f64,
                DrumPadSource::Synth(_) => 1.0,
                DrumPadSource::Simpler(_) => 1.0,
            };
            voice.phase_step = pitch_ratio * sr_ratio;

            voice.envelope_val = 1.0;
            let decay_samples = (pad_decay.max(0.005) * self.sample_rate).max(1.0);
            voice.envelope_decay = (-4.60517 / decay_samples).exp();
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
                    DrumPadSource::Simpler(simpler) => {
                        if let Some(ref sample_data) = simpler.sample_data {
                            let ch = (simpler.sample_channels as usize).max(1);
                            if sample_data.is_empty() {
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
                        } else {
                            voice.active = false;
                            break;
                        }
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

                let voice_sig = sample * voice.envelope_val * voice.velocity;
                let dry_sig_l = voice_sig * pad.volume * pan_l;
                let dry_sig_r = voice_sig * pad.volume * pan_r;

                out_left[i] += dry_sig_l;
                out_right[i] += dry_sig_r;

                if send_a_amt > 0.0 {
                    send_a_l[i] += voice_sig * pan_l * send_a_amt;
                    send_a_r[i] += voice_sig * pan_r * send_a_amt;
                }
                if send_b_amt > 0.0 {
                    send_b_l[i] += voice_sig * pan_l * send_b_amt;
                    send_b_r[i] += voice_sig * pan_r * send_b_amt;
                }
            }
        }

        // Render per-pad Simpler devices (independent polyphonic synthesis per pad)
        let any_solo = self.all_pads.iter().any(|p| p.solo);
        for pad in &mut self.all_pads {
            if pad.mute || (any_solo && !pad.solo) {
                continue;
            }
            if let DrumPadSource::Simpler(ref mut simpler) = pad.source {
                if simpler.voices.iter().any(|v| v.active) {
                    let pan_norm = pad.pan.clamp(-1.0, 1.0);
                    let theta = (pan_norm + 1.0) * 0.25 * PI;
                    let pan_l = theta.cos();
                    let pan_r = theta.sin();
                    let send_a_amt = pad.send_a.clamp(0.0, 1.0);
                    let send_b_amt = pad.send_b.clamp(0.0, 1.0);

                    for i in 0..n_frames {
                        let (sl, sr) = simpler.process_stereo_sample();
                        let dry_sig_l = sl * pad.volume * pan_l;
                        let dry_sig_r = sr * pad.volume * pan_r;

                        out_left[i] += dry_sig_l;
                        out_right[i] += dry_sig_r;

                        if send_a_amt > 0.0 {
                            send_a_l[i] += sl * pan_l * send_a_amt;
                            send_a_r[i] += sr * pan_r * send_a_amt;
                        }
                        if send_b_amt > 0.0 {
                            send_b_l[i] += sl * pan_l * send_b_amt;
                            send_b_r[i] += sr * pan_r * send_b_amt;
                        }
                    }
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

    // -------------------------------------------------------------------------
    // Live 12 Chapter 24 Macro Controls (1..16) & Macro Variations
    // -------------------------------------------------------------------------

    /// Set macro knob value [0.0..1.0] and immediately propagate to mapped targets.
    pub fn set_macro(&mut self, index: usize, value: f32) {
        if self.macro_system.set_macro_value(index, value) {
            self.apply_macro_mappings();
        }
    }

    /// Read macro knob normalized value [0.0..1.0].
    pub fn get_macro(&self, index: usize) -> f32 {
        self.macro_system
            .get_macro(index)
            .map(|m| m.value)
            .unwrap_or(0.0)
    }

    /// Set total visible macro controls (1..16).
    pub fn set_macro_count(&mut self, count: usize) {
        self.macro_system.set_macro_count(count);
    }

    /// Current number of macro controls.
    pub fn macro_count(&self) -> usize {
        self.macro_system.macro_count()
    }

    /// Map a macro knob index to a Drum Rack target parameter with min/max range.
    pub fn map_macro(
        &mut self,
        macro_idx: usize,
        target: DrumRackMacroTarget,
        min: f32,
        max: f32,
    ) {
        self.macro_mappings.push(DrumRackMacroMapping {
            macro_idx,
            target,
            range: ableton_core::MacroMappingRange::new(min, max),
        });
        self.apply_macro_mappings();
    }

    /// Apply all macro mappings to their targets.
    pub fn apply_macro_mappings(&mut self) {
        for mapping in &self.macro_mappings {
            if let Some(m) = self.macro_system.get_macro(mapping.macro_idx) {
                let mapped_val = mapping.range.map(m.value);
                match mapping.target {
                    DrumRackMacroTarget::PadVolume(idx) => {
                        let u = idx as usize;
                        if u < self.all_pads.len() {
                            self.all_pads[u].volume = mapped_val;
                            let bank_start = self.active_bank * PADS_PER_BANK;
                            if u >= bank_start && u < bank_start + PADS_PER_BANK {
                                self.pads[u - bank_start].volume = mapped_val;
                            }
                        }
                    }
                    DrumRackMacroTarget::PadPan(idx) => {
                        let u = idx as usize;
                        if u < self.all_pads.len() {
                            self.all_pads[u].pan = mapped_val.clamp(-1.0, 1.0);
                            let bank_start = self.active_bank * PADS_PER_BANK;
                            if u >= bank_start && u < bank_start + PADS_PER_BANK {
                                self.pads[u - bank_start].pan = mapped_val.clamp(-1.0, 1.0);
                            }
                        }
                    }
                    DrumRackMacroTarget::PadPitch(idx) => {
                        let u = idx as usize;
                        if u < self.all_pads.len() {
                            self.all_pads[u].pitch_semitones = mapped_val;
                            let bank_start = self.active_bank * PADS_PER_BANK;
                            if u >= bank_start && u < bank_start + PADS_PER_BANK {
                                self.pads[u - bank_start].pitch_semitones = mapped_val;
                            }
                        }
                    }
                    DrumRackMacroTarget::PadDecay(idx) => {
                        let u = idx as usize;
                        if u < self.all_pads.len() {
                            self.all_pads[u].decay_seconds = mapped_val.max(0.005);
                            let bank_start = self.active_bank * PADS_PER_BANK;
                            if u >= bank_start && u < bank_start + PADS_PER_BANK {
                                self.pads[u - bank_start].decay_seconds = mapped_val.max(0.005);
                            }
                        }
                    }
                    DrumRackMacroTarget::PadSendA(idx) => {
                        let u = idx as usize;
                        if u < self.all_pads.len() {
                            self.all_pads[u].send_a = mapped_val.clamp(0.0, 1.0);
                            let bank_start = self.active_bank * PADS_PER_BANK;
                            if u >= bank_start && u < bank_start + PADS_PER_BANK {
                                self.pads[u - bank_start].send_a = mapped_val.clamp(0.0, 1.0);
                            }
                        }
                    }
                    DrumRackMacroTarget::PadSendB(idx) => {
                        let u = idx as usize;
                        if u < self.all_pads.len() {
                            self.all_pads[u].send_b = mapped_val.clamp(0.0, 1.0);
                            let bank_start = self.active_bank * PADS_PER_BANK;
                            if u >= bank_start && u < bank_start + PADS_PER_BANK {
                                self.pads[u - bank_start].send_b = mapped_val.clamp(0.0, 1.0);
                            }
                        }
                    }
                    DrumRackMacroTarget::ReturnALevel => {
                        self.return_a_level = mapped_val.clamp(0.0, 2.0);
                    }
                    DrumRackMacroTarget::ReturnBLevel => {
                        self.return_b_level = mapped_val.clamp(0.0, 2.0);
                    }
                    DrumRackMacroTarget::AllPadsVolume => {
                        for p in &mut self.all_pads {
                            p.volume = mapped_val;
                        }
                        for p in &mut self.pads {
                            p.volume = mapped_val;
                        }
                    }
                    DrumRackMacroTarget::AllPadsDecay => {
                        for p in &mut self.all_pads {
                            p.decay_seconds = mapped_val.max(0.005);
                        }
                        for p in &mut self.pads {
                            p.decay_seconds = mapped_val.max(0.005);
                        }
                    }
                }
            }
        }
    }

    /// Capture current macro knob state as a named Macro Variation.
    pub fn capture_variation(&mut self, name: impl Into<String>) -> usize {
        self.macro_system.capture_variation(name)
    }

    /// Recall a saved Macro Variation by index [0..N-1] and apply mappings.
    pub fn recall_variation(&mut self, index: usize) -> Result<(), &'static str> {
        self.macro_system.recall_variation(index)?;
        self.apply_macro_mappings();
        Ok(())
    }

    /// Recall a saved Macro Variation by its string name and apply mappings.
    pub fn recall_variation_by_name(&mut self, name: &str) -> Result<(), &'static str> {
        self.macro_system.recall_variation_by_name(name)?;
        self.apply_macro_mappings();
        Ok(())
    }

    /// Delete a Macro Variation by index.
    pub fn delete_variation(&mut self, index: usize) -> bool {
        self.macro_system.delete_variation(index)
    }

    /// Randomize all non-excluded macro knobs and update mapped targets.
    pub fn randomize_macros(&mut self) {
        self.macro_system.randomize_macros(&mut self.rng_state);
        self.apply_macro_mappings();
    }

    /// Total count of saved Macro Variations.
    pub fn variations_count(&self) -> usize {
        self.macro_system.variations_count()
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
        for pad in &mut self.all_pads {
            if let DrumPadSource::Simpler(ref mut s) = pad.source {
                s.silence_all();
            }
        }
        for pad in &mut self.pads {
            if let DrumPadSource::Simpler(ref mut s) = pad.source {
                s.silence_all();
            }
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
                MidiEvent::NoteOff { note, .. } => {
                    self.note_off(note);
                }
                _ => {}
            }
        }

        self.process_audio_buffer(audio_out);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.trigger_midi_note(note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        for pad in &mut self.all_pads {
            if pad.midi_note == note {
                if let DrumPadSource::Simpler(ref mut s) = pad.source {
                    s.trigger_note_off(note);
                }
            }
        }
        for pad in &mut self.pads {
            if pad.midi_note == note {
                if let DrumPadSource::Simpler(ref mut s) = pad.source {
                    s.trigger_note_off(note);
                }
            }
        }
    }

    fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.active = false;
        }
        for pad in &mut self.all_pads {
            if let DrumPadSource::Simpler(ref mut s) = pad.source {
                s.silence_all();
            }
        }
        for pad in &mut self.pads {
            if let DrumPadSource::Simpler(ref mut s) = pad.source {
                s.silence_all();
            }
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        if (param_id as usize) < 16 {
            self.set_macro(param_id as usize, value);
        } else if param_id == 100 {
            self.return_a_level = value.clamp(0.0, 2.0);
        } else if param_id == 101 {
            self.return_b_level = value.clamp(0.0, 2.0);
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        if (param_id as usize) < 16 {
            self.get_macro(param_id as usize)
        } else if param_id == 100 {
            self.return_a_level
        } else if param_id == 101 {
            self.return_b_level
        } else {
            0.0
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

    #[test]
    fn test_drum_rack_all_16_choke_groups_independence() {
        let mut rack = DrumRack::new(44100.0);

        // Assign 16 distinct pad pairs to all 16 choke groups (1..=16)
        // Group g (1..=16) assigned to pad (g-1) and pad (g-1 + 16)
        for g in 1..=16u8 {
            rack.set_pad_choke_group((g - 1) as usize, Some(g));
            rack.set_pad_choke_group((g - 1 + 16) as usize, Some(g));
        }

        // Test every choke group 1..=16 independently
        for g in 1..=16u8 {
            let pad_a = (g - 1) as usize;
            let pad_b = (g - 1 + 16) as usize;

            // Trigger Pad A
            rack.trigger_midi_note(rack.all_pads[pad_a].midi_note, 120);

            // Process 128 samples
            let mut l = [0.0; 128];
            let mut r = [0.0; 128];
            rack.process_block(&mut l, &mut r);

            // Find active voice for pad A
            let voice_a = rack.voices.iter().find(|v| v.active && v.pad_idx == pad_a as u8);
            assert!(
                voice_a.is_some(),
                "Choke Group {}: Pad A ({}) must have an active voice",
                g,
                pad_a
            );
            assert!(
                !voice_a.unwrap().is_fading_out,
                "Choke Group {}: Pad A must not be fading out before choke",
                g
            );

            // Trigger Pad B of the SAME choke group
            rack.trigger_midi_note(rack.all_pads[pad_b].midi_note, 127);

            // Voice A must now be in rapid clickless fade out (<5ms)
            let voice_a_choked = rack.voices.iter().find(|v| v.active && v.pad_idx == pad_a as u8);
            if let Some(va) = voice_a_choked {
                assert!(
                    va.is_fading_out,
                    "Choke Group {}: Voice A must be fading out upon triggering Pad B",
                    g
                );
            }

            // Process 240 samples (< 5.5 ms at 44.1 kHz)
            let mut fl = [0.0; 240];
            let mut fr = [0.0; 240];
            rack.process_block(&mut fl, &mut fr);

            // Voice A must now be either inactive or damped below -60 dBFS (<0.001)
            let voice_a_after = rack.voices.iter().find(|v| v.active && v.pad_idx == pad_a as u8);
            if let Some(va) = voice_a_after {
                assert!(
                    va.envelope_val < 0.001,
                    "Choke Group {}: Voice A envelope ({}) must be < 0.001 (-60 dBFS) within 5ms",
                    g,
                    va.envelope_val
                );
            }

            // Voice B must remain active and healthy
            let voice_b = rack.voices.iter().find(|v| v.active && v.pad_idx == pad_b as u8);
            assert!(
                voice_b.is_some(),
                "Choke Group {}: Pad B ({}) must be active and playing",
                g,
                pad_b
            );

            rack.reset();
        }
    }

    #[test]
    fn test_drum_rack_per_pad_simpler_instances_and_audio_render() {
        let mut rack = DrumRack::new(44100.0);

        // Create a SimplerDevice with generated sine test wave
        let mut simpler = crate::simpler::SimplerDevice::new(44100.0);
        let sample_frames = 44100; // 1 second
        let mut samples = Vec::with_capacity(sample_frames);
        for i in 0..sample_frames {
            let phase = 2.0 * std::f32::consts::PI * 440.0 * (i as f32) / 44100.0;
            samples.push(phase.sin() * 0.8);
        }
        simpler.load_sample(samples, 44100, 1);
        simpler.mode = crate::simpler::SimplerMode::Classic;

        // Load into visible Pad 3 (MIDI 39)
        assert!(rack.load_simpler_pad(3, simpler.clone()).is_ok());
        assert!(rack.pads[3].has_simpler());
        assert_eq!(rack.pads[3].simpler().unwrap().mode, crate::simpler::SimplerMode::Classic);

        // Load into global Pad 70 in Bank E (MIDI 70)
        assert!(rack.load_simpler_pad_global(70, simpler).is_ok());
        assert!(rack.all_pads[70].has_simpler());

        // Configure routing on Pad 3: Send A = 0.7, Send B = 0.4
        rack.all_pads[3].send_a = 0.7;
        rack.all_pads[3].send_b = 0.4;
        rack.all_pads[3].volume = 0.9;
        rack.pads[3].send_a = 0.7;
        rack.pads[3].send_b = 0.4;
        rack.pads[3].volume = 0.9;

        // Trigger pad 3 via MIDI
        rack.trigger_midi_note(39, 110);

        // Process audio buffer
        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        rack.process(&[], &mut buf, &time);

        let energy_l: f32 = buf.left[..256].iter().map(|s| s * s).sum();
        let energy_r: f32 = buf.right[..256].iter().map(|s| s * s).sum();
        assert!(
            energy_l > 0.01 && energy_r > 0.01,
            "Simpler on Pad 3 must render audible non-zero stereo audio, got L: {}, R: {}",
            energy_l,
            energy_r
        );

        // Verify note-off triggers envelope release on Simpler
        rack.note_off(39);
        // Process release
        rack.process(&[], &mut buf, &time);

        // Verify Choke group on Simpler pad
        rack.set_pad_choke_group(3, Some(5));
        rack.set_pad_choke_group(4, Some(5));
        rack.trigger_midi_note(39, 127);
        // Pad 4 triggered on choke group 5 -> Pad 3 Simpler silenced
        rack.trigger_midi_note(40, 127);
        if let DrumPadSource::Simpler(ref s) = rack.all_pads[3].source {
            assert!(
                !s.voices.iter().any(|v| v.active),
                "Simpler pad on choke group 5 must be silenced when partner pad triggers"
            );
        }
    }

    #[test]
    fn test_drum_rack_macro_controls_1_to_16_and_variations() {
        let mut rack = DrumRack::new(44100.0);

        // 1. Configure 16 Macro controls (Live 12 Chapter 24)
        rack.set_macro_count(16);
        assert_eq!(rack.macro_count(), 16);

        // 2. Map macros to Drum Rack parameters
        // Macro 0 -> Pad 0 Volume [0.0 .. 2.0]
        rack.map_macro(0, DrumRackMacroTarget::PadVolume(0), 0.0, 2.0);
        // Macro 1 -> Pad 0 Pan [-1.0 .. 1.0]
        rack.map_macro(1, DrumRackMacroTarget::PadPan(0), -1.0, 1.0);
        // Macro 2 -> Pad 0 Decay [0.01 .. 1.5]
        rack.map_macro(2, DrumRackMacroTarget::PadDecay(0), 0.01, 1.5);
        // Macro 3 -> Pad 0 Send A [0.0 .. 1.0]
        rack.map_macro(3, DrumRackMacroTarget::PadSendA(0), 0.0, 1.0);
        // Macro 4 -> Return A Master Level [0.0 .. 2.0]
        rack.map_macro(4, DrumRackMacroTarget::ReturnALevel, 0.0, 2.0);
        // Macro 5 -> Return B Master Level (Inverted mapping: 2.0 .. 0.0)
        rack.map_macro(5, DrumRackMacroTarget::ReturnBLevel, 2.0, 0.0);

        // 3. Set Macro 0 to 0.5 -> Pad 0 Volume should become 1.0
        rack.set_macro(0, 0.5);
        assert!((rack.all_pads[0].volume - 1.0).abs() < 1e-4);
        assert!((rack.pads[0].volume - 1.0).abs() < 1e-4);

        // Macro 1 to 0.75 -> Pan should become 0.5
        rack.set_macro(1, 0.75);
        assert!((rack.all_pads[0].pan - 0.5).abs() < 1e-4);

        // Macro 4 to 0.8 -> Return A level = 1.6
        rack.set_macro(4, 0.8);
        assert!((rack.return_a_level - 1.6).abs() < 1e-4);

        // Macro 5 (inverted) to 0.25 -> Return B level = 2.0 - 0.25 * 2.0 = 1.5
        rack.set_macro(5, 0.25);
        assert!((rack.return_b_level - 1.5).abs() < 1e-4);

        // 4. Capture Macro Variation "Warm Studio"
        let var1 = rack.capture_variation("Warm Studio");
        assert_eq!(var1, 0);
        assert_eq!(rack.variations_count(), 1);

        // 5. Change macro settings for a new snapshot "Aggressive Club"
        rack.set_macro(0, 1.0); // Pad 0 Volume = 2.0
        rack.set_macro(1, 0.0); // Pad 0 Pan = -1.0 (Hard Left)
        rack.set_macro(4, 0.2); // Return A = 0.4
        let var2 = rack.capture_variation("Aggressive Club");
        assert_eq!(var2, 1);
        assert_eq!(rack.variations_count(), 2);

        assert!((rack.all_pads[0].volume - 2.0).abs() < 1e-4);
        assert!((rack.all_pads[0].pan - (-1.0)).abs() < 1e-4);

        // 6. Recall "Warm Studio" variation by index 0
        assert!(rack.recall_variation(0).is_ok());
        assert!((rack.get_macro(0) - 0.5).abs() < 1e-4);
        assert!((rack.all_pads[0].volume - 1.0).abs() < 1e-4);
        assert!((rack.all_pads[0].pan - 0.5).abs() < 1e-4);
        assert!((rack.return_a_level - 1.6).abs() < 1e-4);

        // 7. Recall "Aggressive Club" variation by name
        assert!(rack.recall_variation_by_name("Aggressive Club").is_ok());
        assert!((rack.get_macro(0) - 1.0).abs() < 1e-4);
        assert!((rack.all_pads[0].volume - 2.0).abs() < 1e-4);
        assert!((rack.all_pads[0].pan - (-1.0)).abs() < 1e-4);

        // 8. Test Macro Randomization
        rack.macro_system.macros[0].excluded_from_randomization = true;
        rack.randomize_macros();
        // Macro 0 was excluded, stays at 1.0
        assert_eq!(rack.get_macro(0), 1.0);
        // Macro 1 was randomized
        let m1_val = rack.get_macro(1);
        assert!((0.0..=1.0).contains(&m1_val));

        // 9. Delete variation
        assert!(rack.delete_variation(0));
        assert_eq!(rack.variations_count(), 1);
    }

    #[test]
    fn test_drum_rack_device_and_instrument_traits_integration() {
        let mut rack = DrumRack::new(44100.0);

        // Device trait
        assert_eq!(rack.name(), "Drum Rack");
        assert_eq!(rack.category(), DeviceCategory::Instrument);
        assert!(rack.is_enabled());
        rack.set_enabled(false);
        assert!(!rack.is_enabled());
        rack.set_enabled(true);

        // InstrumentDevice trait note_on / note_off
        rack.note_on(36, 100);
        let v0 = rack.voices.iter().find(|v| v.active && v.pad_idx == 0);
        assert!(v0.is_some());

        // all_notes_off
        rack.all_notes_off();
        assert!(!rack.voices.iter().any(|v| v.active));

        // set_parameter & get_parameter
        rack.set_parameter(0, 0.42); // Macro 0
        assert!((rack.get_parameter(0) - 0.42).abs() < 1e-4);

        rack.set_parameter(100, 1.25); // Return A level
        assert!((rack.get_parameter(100) - 1.25).abs() < 1e-4);

        rack.set_parameter(101, 0.75); // Return B level
        assert!((rack.get_parameter(101) - 0.75).abs() < 1e-4);

        // reset
        rack.reset();
        assert!(!rack.voices.iter().any(|v| v.active));
    }
}
