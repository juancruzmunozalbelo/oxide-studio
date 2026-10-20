//! 16-pad Drum Rack with 32-voice static allocation pool and choke groups.

use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;
use ableton_core::AudioBuffer;

use crate::synth_models::{SynthDrumType, SynthPercussion};
use crate::voice::DrumVoice;
use crate::wav_loader::load_wav_file;

pub const MAX_DRUM_VOICES: usize = 32;
pub const NUM_DRUM_PADS: usize = 16;

#[derive(Clone)]
pub enum DrumPadSource {
    Sample {
        sample_data: Arc<[f32]>,
        channels: u8,
        sample_rate: u32,
    },
    Synth(SynthDrumType),
}

pub struct DrumPad {
    pub name: String,
    pub midi_note: u8,
    pub source: DrumPadSource,
    pub volume: f32,
    pub pan: f32, // [-1.0, 1.0]
    pub pitch_semitones: f32, // [-24.0, 24.0]
    pub decay_seconds: f32,
    pub choke_group: Option<u8>,
    pub use_cubic_interpolation: bool,
}

pub struct DrumRack {
    pub pads: [DrumPad; NUM_DRUM_PADS],
    pub voices: [DrumVoice; MAX_DRUM_VOICES],
    pub sample_rate: f32,
}

impl DrumRack {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let pad_names = [
            "Kick", "Rim", "Snare", "Clap", "Elec Snare", "Low Floor Tom",
            "Closed HH", "High Floor Tom", "Pedal HH", "Low Tom", "Open HH",
            "Mid Tom", "High Tom", "Crash", "Ride", "Perc",
        ];

        let pads: [DrumPad; NUM_DRUM_PADS] = std::array::from_fn(|i| {
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
                    6 | 8 => 0.05, // Closed / Pedal HH short
                    10 => 0.35,    // Open HH long
                    _ => 0.25,
                },
                choke_group: choke,
                use_cubic_interpolation: false,
            }
        });

        Self {
            pads,
            voices: [DrumVoice::default(); MAX_DRUM_VOICES],
            sample_rate: sr,
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

    /// Load a WAV file into a specific pad index.
    pub fn load_sample_pad<P: AsRef<Path>>(&mut self, pad_idx: usize, path: P) -> Result<(), String> {
        if pad_idx >= NUM_DRUM_PADS {
            return Err(format!("Pad index {} out of range", pad_idx));
        }

        let decoded = load_wav_file(path)?;
        self.pads[pad_idx].source = DrumPadSource::Sample {
            sample_data: Arc::from(decoded.samples.into_boxed_slice()),
            channels: decoded.channels as u8,
            sample_rate: decoded.sample_rate,
        };
        Ok(())
    }

    /// Trigger pad by index [0..15] with velocity [1..127].
    pub fn trigger_pad(&mut self, pad_idx: u8, velocity: u8) {
        if pad_idx as usize >= NUM_DRUM_PADS || velocity == 0 {
            return;
        }
        let pad = &self.pads[pad_idx as usize];

        // 1. Choke group dampening: clickless exponential fadeout in <5ms (<0.001 / -60 dBFS)
        if let Some(target_choke) = pad.choke_group {
            for voice in &mut self.voices {
                if voice.active && voice.choke_group == Some(target_choke) {
                    voice.is_fading_out = true;
                    // ln(0.001) / (0.005 * sr)
                    voice.fade_out_coeff = (-6.907755 / (0.005 * self.sample_rate)).exp();
                }
            }
        }

        // 2. Static voice allocation (steals quietest or oldest without any heap allocation)
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
        voice.envelope_decay = (-4.60517 / decay_samples).exp(); // e^(-4.605) ≈ 0.01 (-40 dB)
    }

    /// Trigger pad by MIDI note number (default 36..51).
    pub fn trigger_midi_note(&mut self, note: u8, velocity: u8) {
        for (i, pad) in self.pads.iter().enumerate() {
            if pad.midi_note == note {
                self.trigger_pad(i as u8, velocity);
                return;
            }
        }
    }

    /// Selects an inactive voice, or steals the quietest active voice. Zero allocations.
    pub fn allocate_voice(&self) -> usize {
        for (i, v) in self.voices.iter().enumerate() {
            if !v.active {
                return i;
            }
        }
        // Steal voice with lowest envelope value
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

            // Constant-power sine/cosine panning law: L^2 + R^2 = 1.0
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
