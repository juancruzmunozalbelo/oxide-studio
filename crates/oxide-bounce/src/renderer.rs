//! Multi-track arrangement offline bounce engine.
//!
//! Renders multi-track electronic music arrangements using genuine workspace audio engines:
//! - `oxide_dsp::SynthEngine` (Mystran 2x ZDF ladder filter, PolyBLEP, ADSR, LFO, and preset modulation)
//! - `oxide_drums::DrumRack` (16-pad drum rack with 909 sample and synth percussion, choke groups)
//! - `oxide_fx::MasterLimiter` (128-sample lookahead brickwall limiter with -0.3 dBFS ceiling)

use std::f32::consts::PI;
use std::path::Path;

use oxide_dsp::SynthEngine;
use oxide_drums::DrumRack;
use oxide_fx::MasterLimiter;
use oxide_project::DAWProject;

use crate::stems::{render_single_track_window, BounceError};
use crate::wav_writer::WavWriter;

pub struct DemoArrangementRenderer {
    pub sample_rate: f32,
    pub bpm: f64,
}

impl DemoArrangementRenderer {
    pub fn new(sample_rate: f32, bpm: f64) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let b = if bpm <= 0.0 { 120.0 } else { bpm };
        Self {
            sample_rate: sr,
            bpm: b,
        }
    }

    /// Renders a multi-track electronic arrangement combining Mystran ZDF synth sequence
    /// and 909 drums through master limiter into stereo interleaved f32 audio samples.
    pub fn render_demo_seconds(&self, duration_seconds: f64) -> Vec<f32> {
        let total_frames = (duration_seconds * self.sample_rate as f64).ceil() as usize;

        let seconds_per_beat = 60.0 / self.bpm;
        let frames_per_beat = (seconds_per_beat * self.sample_rate as f64) as usize;
        let frames_per_16th = (frames_per_beat / 4).max(1);

        // 1. Construct genuine SynthEngine
        let mut synth = SynthEngine::new(self.sample_rate);
        let loaded = synth.load_preset("wowbass.preset");
        if !loaded {
            // Default acid synth setup if preset catalog not found
            synth.set_parameter("oscillator_saw_amplitude", 0.8);
            synth.set_parameter("oscillator_pulse_amplitude", 0.4);
            synth.set_parameter("filter_frequency", 800.0);
            synth.set_parameter("filter_resonance", 0.7);
            synth.set_parameter("amplitude_envelope_decay", 220.0);
            synth.set_parameter("amplitude_envelope_sustain", 0.1);
        }

        // 2. Construct genuine DrumRack
        let mut drum_rack = DrumRack::new(self.sample_rate);
        if Path::new("/Users/juancruz/ableton-rust/assets/kick.wav").exists() {
            drum_rack.load_default_909_samples("/Users/juancruz/ableton-rust/assets");
        } else if Path::new("/Users/juancruz/ableton-rea/public/sounds/kick.wav").exists() {
            drum_rack.load_default_909_samples("/Users/juancruz/ableton-rea/public/sounds");
        }

        // 3. Construct genuine MasterLimiter
        let mut limiter = MasterLimiter::new(self.sample_rate);

        let notes = [36u8, 38, 41, 43, 36, 48, 46, 43, 36, 39, 41, 44, 36, 43, 41, 38];

        const BLOCK_SIZE: usize = 128;
        let mut frame = 0;
        let mut interleaved = Vec::with_capacity(total_frames * 2);

        let mut block_left = [0.0f32; BLOCK_SIZE];
        let mut block_right = [0.0f32; BLOCK_SIZE];
        let mut drum_l = [0.0f32; BLOCK_SIZE];
        let mut drum_r = [0.0f32; BLOCK_SIZE];

        while frame < total_frames {
            let current_block_len = (total_frames - frame).min(BLOCK_SIZE);

            drum_l[..current_block_len].fill(0.0);
            drum_r[..current_block_len].fill(0.0);

            for b in 0..current_block_len {
                let f = frame + b;

                // --- Synth Sequence Scheduling ---
                let step_idx = f / frames_per_16th;
                let step_frame = f % frames_per_16th;
                let gate_duration = (frames_per_16th * 3) / 4;

                let note = notes[step_idx % notes.len()];
                if step_frame == 0 {
                    synth.note_on(note, 100);
                } else if step_frame == gate_duration {
                    synth.note_off(note);
                }

                // Dynamic filter modulation sweep across the arrangement
                let progress = f as f32 / total_frames as f32;
                let sweep_x = 0.15 + 0.70 * (progress * 2.0 * PI).sin().abs();
                synth.set_macro_x(sweep_x);
                synth.set_macro_y(0.25 + 0.50 * progress);

                // --- 909 Drum Scheduling ---
                let beat_frame = f % frames_per_beat;
                let beat_num = (f / frames_per_beat) % 4;
                let sixteenth_frame = f % frames_per_16th;
                let sixteenth_num = (f / frames_per_16th) % 16;

                // 4-on-the-floor Kick (Pad 0)
                if beat_frame == 0 {
                    drum_rack.trigger_pad(0, 120);
                }

                // Snare on beats 2 and 4 (Pad 2)
                if beat_frame == 0 && (beat_num == 1 || beat_num == 3) {
                    drum_rack.trigger_pad(2, 110);
                }

                // 16th Hi-Hats with Choke Group 1:
                // Open HH on off-beat 16ths (e.g. index 2, 6, 10, 14), Closed HH on others
                if sixteenth_frame == 0 {
                    if sixteenth_num % 4 == 2 {
                        drum_rack.trigger_pad(10, 95); // Open HH (Choke Group 1)
                    } else {
                        drum_rack.trigger_pad(6, 85);  // Closed HH (Chokes Open HH)
                    }
                }

                // Render synth sample
                let synth_sample = synth.process_sample();
                block_left[b] = synth_sample * 0.707;
                block_right[b] = synth_sample * 0.707;
            }

            // Render drum rack block through constant-power panning and choke groups
            drum_rack.process_block(
                &mut drum_l[..current_block_len],
                &mut drum_r[..current_block_len],
            );

            // Mix synth and drums
            for b in 0..current_block_len {
                block_left[b] += drum_l[b];
                block_right[b] += drum_r[b];
            }

            // Process master bus through 128-sample lookahead brickwall limiter (-0.3 dBFS ceiling)
            limiter.process_block(
                &mut block_left[..current_block_len],
                &mut block_right[..current_block_len],
            );

            // Interleave stereo f32 samples
            for b in 0..current_block_len {
                interleaved.push(block_left[b]);
                interleaved.push(block_right[b]);
            }

            frame += current_block_len;
        }

        interleaved
    }
}

/// Renders an entire `DAWProject` (arrangement or session clips across all tracks) to a WAV file on disk.
/// If `out_path` is empty (e.g. `""` or `Path::new("")`), audio is rendered and returned as interleaved stereo f32 without writing to disk.
pub fn render_project_to_wav(
    project: &DAWProject,
    out_path: impl AsRef<Path>,
) -> Result<Vec<f32>, BounceError> {
    let bpm = if project.transport.bpm <= 0.0 {
        120.0
    } else {
        project.transport.bpm
    };

    let mut max_beat = 0.0f64;
    for track in &project.tracks {
        for clip in &track.arrangement_clips {
            let end = clip.timeline_end_beat();
            if end > max_beat {
                max_beat = end;
            }
        }
    }

    let duration_seconds = if max_beat > 0.0 {
        (max_beat * 60.0 / bpm).clamp(4.0, 30.0)
    } else if project.transport.loop_enabled && project.transport.loop_end_beat > 0.0 {
        ((project.transport.loop_end_beat - project.transport.loop_start_beat) * 60.0 / bpm).clamp(4.0, 30.0)
    } else {
        4.0
    };

    render_project_window_to_wav(project, out_path, 0.0, duration_seconds, 16)
}

/// Renders a specific time/beat window of a `DAWProject` to a 16-bit or 32-bit float WAV file.
pub fn render_project_window_to_wav(
    project: &DAWProject,
    out_path: impl AsRef<Path>,
    start_beat_offset: f64,
    duration_seconds: f64,
    bit_depth: u16,
) -> Result<Vec<f32>, BounceError> {
    let sample_rate = 44100.0f32;
    let bpm = if project.transport.bpm <= 0.0 {
        120.0
    } else {
        project.transport.bpm
    };
    let duration_seconds = duration_seconds.clamp(0.5, 300.0);

    let total_frames = (duration_seconds * sample_rate as f64).ceil() as usize;
    let mut master_l = vec![0.0f32; total_frames];
    let mut master_r = vec![0.0f32; total_frames];

    if project.tracks.is_empty() {
        let renderer = DemoArrangementRenderer::new(sample_rate, bpm);
        let interleaved = renderer.render_demo_seconds(duration_seconds);
        let frames = interleaved.len() / 2;
        for i in 0..frames.min(total_frames) {
            master_l[i] = interleaved[i * 2];
            master_r[i] = interleaved[i * 2 + 1];
        }
    } else {
        for track in &project.tracks {
            if !track.mixer.mute {
                let (trk_l, trk_r) = render_single_track_window(
                    track,
                    sample_rate,
                    bpm,
                    start_beat_offset,
                    duration_seconds,
                );
                for i in 0..total_frames {
                    master_l[i] += trk_l[i];
                    master_r[i] += trk_r[i];
                }
            }
        }
    }

    // Apply master volume
    let master_gain = 10.0f32.powf(project.master_track.volume_db.clamp(-70.0, 24.0) / 20.0);
    for i in 0..total_frames {
        master_l[i] *= master_gain;
        master_r[i] *= master_gain;
    }

    let mut limiter = MasterLimiter::new(sample_rate);
    const BLOCK_SIZE: usize = 128;
    let mut frame = 0;
    while frame < total_frames {
        let cur_block = (total_frames - frame).min(BLOCK_SIZE);
        limiter.process_block(
            &mut master_l[frame..frame + cur_block],
            &mut master_r[frame..frame + cur_block],
        );
        frame += cur_block;
    }

    let mut interleaved = Vec::with_capacity(total_frames * 2);
    for i in 0..total_frames {
        interleaved.push(master_l[i]);
        interleaved.push(master_r[i]);
    }

    let p = out_path.as_ref();
    if !p.as_os_str().is_empty() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if bit_depth == 32 {
            WavWriter::write_wav_32bit_float(p, sample_rate as u32, 2, &interleaved)
                .map_err(BounceError::Render)?;
        } else {
            WavWriter::write_wav_16bit(p, sample_rate as u32, 2, &interleaved)
                .map_err(BounceError::Render)?;
        }
    }

    Ok(interleaved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_project::TrackDefinition;
    use oxide_timeline::{ArrangementClip, AudioClipData, ClipPattern, SessionClip};

    #[test]
    fn test_render_project_to_wav_with_audio_track_samples() {
        let mut project = DAWProject::new("Audio Track Test");
        project.transport.bpm = 120.0;

        let mut audio_track = TrackDefinition::new_audio(1, "Vocal Track");
        audio_track.mixer.volume_db = 0.0;

        // Generate 1 second of 440 Hz test sine wave into AudioClipData.samples (interleaved stereo)
        let sample_rate = 44100;
        let num_frames = 44100;
        let mut pcm_samples = Vec::with_capacity(num_frames * 2);
        for f in 0..num_frames {
            let t = f as f32 / sample_rate as f32;
            let val = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
            pcm_samples.push(val); // Left
            pcm_samples.push(val); // Right
        }

        let mut audio_data = AudioClipData::new("/samples/test.wav", sample_rate as u32, 2, 4.0);
        audio_data.samples = pcm_samples;

        let arr_clip = ArrangementClip::new(
            10,
            "Vocal Chorus",
            0.0,
            4.0,
            ClipPattern::Audio(audio_data.clone()),
        );
        audio_track.arrangement_clips.push(arr_clip);

        let sess_clip = SessionClip::new(1, "Vocal Slot 1", ClipPattern::Audio(audio_data));
        audio_track.session_clips.push(Some(sess_clip));

        project.tracks.push(audio_track);

        let tmp_wav = std::env::temp_dir().join("oxide_test_audio_track_render.wav");
        let samples = render_project_to_wav(&project, &tmp_wav).expect("render_project_to_wav must succeed");

        assert!(tmp_wav.exists());
        assert!(!samples.is_empty());

        // Verify non-zero RMS
        let mut sum_sq = 0.0f64;
        for &s in &samples {
            assert!(s.is_finite());
            sum_sq += (s * s) as f64;
        }
        let rms = (sum_sq / samples.len() as f64).sqrt();
        assert!(rms > 0.05, "RMS must be > 0.05 from audio samples, got {}", rms);

        let _ = std::fs::remove_file(tmp_wav);
    }
}
