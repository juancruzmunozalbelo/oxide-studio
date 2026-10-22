//! Steady-state audio callback processor with zero heap allocations.

use ableton_core::{AudioCommand, AudioTelemetry, MusicalTime};
use ableton_dsp::SynthEngine;
use ableton_drums::DrumRack;
use ableton_fx::{Mixer, Saturator, TempoDelay, FdnReverb};

use crate::parameters::{
    AtomicParameters, PARAM_BPM, PARAM_MASTER_VOL, PARAM_TRACK_PAN_BASE, PARAM_TRACK_VOL_BASE,
};
use crate::queues::AudioThreadHandles;

pub const MAX_BLOCK_FRAMES: usize = 1024;

// Parameter IDs for AudioCommand::SetParameter
pub const PARAM_ID_MASTER_VOL: u32 = 1;
pub const PARAM_ID_TEST_TONE_ENABLE: u32 = 2;
pub const PARAM_ID_TEST_TONE_FREQ: u32 = 3;
pub const PARAM_ID_TEST_TONE_VOL: u32 = 4;
pub const PARAM_ID_TRACK_VOL_BASE: u32 = 1000;
pub const PARAM_ID_TRACK_PAN_BASE: u32 = 1100;
pub const PARAM_ID_TRACK_MUTE_BASE: u32 = 1200;
pub const PARAM_ID_TRACK_SOLO_BASE: u32 = 1300;
pub const PARAM_ID_TRACK_SEND_A_BASE: u32 = 1400;
pub const PARAM_ID_TRACK_SEND_B_BASE: u32 = 1500;
pub const PARAM_ID_CROSSFADER: u32 = 2000;

pub struct AudioEngineProcessor {
    pub sample_rate: f32,
    pub transport: MusicalTime,
    pub synth: SynthEngine,
    pub drum_rack: DrumRack,
    pub mixer: Mixer,
    pub saturator: Saturator,
    pub delay: TempoDelay,
    pub reverb: FdnReverb,
    pub params: AtomicParameters,
    pub send_a: [f32; 16],
    pub send_b: [f32; 16],
    pub crossfader: f32,
    pub test_tone_enabled: bool,
    pub test_tone_freq: f32,
    pub test_tone_volume: f32,
    pub test_tone_phase: f32,
    pub cpu_meter: crate::engine::CpuLoadMeter,
    handles: AudioThreadHandles,

    // Pre-allocated zero-allocation planar scratch buffers
    scratch_l: [f32; MAX_BLOCK_FRAMES],
    scratch_r: [f32; MAX_BLOCK_FRAMES],
    synth_buf_l: [f32; MAX_BLOCK_FRAMES],
    synth_buf_r: [f32; MAX_BLOCK_FRAMES],
    drum_buf_l: [f32; MAX_BLOCK_FRAMES],
    drum_buf_r: [f32; MAX_BLOCK_FRAMES],

    // Oscilloscope ring buffer
    osc_buffer: [f32; 256],
    osc_write_pos: usize,
}

impl AudioEngineProcessor {
    pub fn new(
        sample_rate: f32,
        handles: AudioThreadHandles,
        params: AtomicParameters,
    ) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let mut transport = MusicalTime::default();
        transport.bpm = params.get(PARAM_BPM) as f64;

        Self {
            sample_rate: sr,
            transport,
            synth: SynthEngine::new(sr),
            drum_rack: DrumRack::new(sr),
            mixer: Mixer::new(sr),
            saturator: Saturator::new(),
            delay: TempoDelay::new(sr, transport.bpm),
            reverb: FdnReverb::new(sr),
            params,
            send_a: [0.0; 16],
            send_b: [0.0; 16],
            crossfader: 0.0,
            test_tone_enabled: false,
            test_tone_freq: 440.0,
            test_tone_volume: 0.1,
            test_tone_phase: 0.0,
            cpu_meter: crate::engine::CpuLoadMeter::new(),
            handles,
            scratch_l: [0.0; MAX_BLOCK_FRAMES],
            scratch_r: [0.0; MAX_BLOCK_FRAMES],
            synth_buf_l: [0.0; MAX_BLOCK_FRAMES],
            synth_buf_r: [0.0; MAX_BLOCK_FRAMES],
            drum_buf_l: [0.0; MAX_BLOCK_FRAMES],
            drum_buf_r: [0.0; MAX_BLOCK_FRAMES],
            osc_buffer: [0.0; 256],
            osc_write_pos: 0,
        }
    }

    /// Enable or disable the Chapter 2 test tone generator (default 440 Hz standard pitch).
    pub fn enable_test_tone(&mut self, enabled: bool, freq_hz: f32, volume: f32) {
        self.test_tone_enabled = enabled;
        self.test_tone_freq = freq_hz.clamp(20.0, 20000.0);
        self.test_tone_volume = volume.clamp(0.0, 1.0);
    }

    /// Process incoming commands from the SPSC ring buffer. Zero allocations.
    #[inline(always)]
    fn process_commands(&mut self) {
        let synth = &mut self.synth;
        let drum_rack = &mut self.drum_rack;
        let transport = &mut self.transport;
        let delay = &mut self.delay;
        let mixer = &mut self.mixer;
        let params = &self.params;
        let send_a = &mut self.send_a;
        let send_b = &mut self.send_b;
        let crossfader = &mut self.crossfader;
        let test_tone_enabled = &mut self.test_tone_enabled;
        let test_tone_freq = &mut self.test_tone_freq;
        let test_tone_volume = &mut self.test_tone_volume;

        self.handles.drain_commands(|cmd| match cmd {
            AudioCommand::Play => {
                transport.is_playing = true;
            }
            AudioCommand::Stop => {
                transport.is_playing = false;
                transport.beat = 0.0;
                synth.all_notes_off();
            }
            AudioCommand::SetTempo(bpm) => {
                transport.bpm = bpm;
                delay.set_tempo(bpm);
            }
            AudioCommand::NoteOn { track_id, note, velocity } => {
                if track_id == 1 {
                    // Drum track: note 36..51 -> pad 0..15
                    if note >= 36 && note <= 51 {
                        drum_rack.trigger_pad(note - 36, velocity);
                    }
                } else {
                    // Melodic synth track
                    synth.note_on(note, velocity);
                }
            }
            AudioCommand::NoteOff { track_id, note } => {
                if track_id != 1 {
                    synth.note_off(note);
                }
            }
            AudioCommand::SetParameter { track_id, param_id, value } => {
                if param_id == 100 {
                    synth.set_macro_x(value);
                } else if param_id == 101 {
                    synth.set_macro_y(value);
                } else if param_id == 0 || param_id == PARAM_ID_MASTER_VOL {
                    mixer.master_volume_linear = value;
                    params.set(PARAM_MASTER_VOL, value);
                } else if (PARAM_ID_TRACK_VOL_BASE..=PARAM_ID_TRACK_VOL_BASE + 15).contains(&param_id)
                    || (PARAM_TRACK_VOL_BASE as u32..=(PARAM_TRACK_VOL_BASE + 15) as u32).contains(&param_id)
                {
                    let tid = if (PARAM_ID_TRACK_VOL_BASE..=PARAM_ID_TRACK_VOL_BASE + 15).contains(&param_id) {
                        if param_id > PARAM_ID_TRACK_VOL_BASE {
                            (param_id - PARAM_ID_TRACK_VOL_BASE) as usize
                        } else {
                            track_id
                        }
                    } else if (PARAM_TRACK_VOL_BASE as u32..=(PARAM_TRACK_VOL_BASE + 15) as u32).contains(&param_id) {
                        (param_id - PARAM_TRACK_VOL_BASE as u32) as usize
                    } else {
                        track_id
                    };
                    if tid < 16 {
                        let lin = if value <= 0.001 { 0.0 } else { value };
                        mixer.tracks[tid].volume_linear = lin;
                        if lin <= 0.001 {
                            mixer.tracks[tid].volume_db = -60.0;
                            mixer.tracks[tid].peak_l = 0.0;
                            mixer.tracks[tid].peak_r = 0.0;
                        } else {
                            mixer.tracks[tid].volume_db = (20.0 * lin.log10()).clamp(-60.0, 6.0);
                        }
                        params.set(PARAM_TRACK_VOL_BASE + tid, lin);
                    }
                } else if (PARAM_ID_TRACK_PAN_BASE..=PARAM_ID_TRACK_PAN_BASE + 15).contains(&param_id)
                    || (PARAM_TRACK_PAN_BASE as u32..=(PARAM_TRACK_PAN_BASE + 15) as u32).contains(&param_id)
                {
                    let tid = if (PARAM_ID_TRACK_PAN_BASE..=PARAM_ID_TRACK_PAN_BASE + 15).contains(&param_id) {
                        if param_id > PARAM_ID_TRACK_PAN_BASE {
                            (param_id - PARAM_ID_TRACK_PAN_BASE) as usize
                        } else {
                            track_id
                        }
                    } else if (PARAM_TRACK_PAN_BASE as u32..=(PARAM_TRACK_PAN_BASE + 15) as u32).contains(&param_id) {
                        (param_id - PARAM_TRACK_PAN_BASE as u32) as usize
                    } else {
                        track_id
                    };
                    if tid < 16 {
                        let clamped = value.clamp(-1.0, 1.0);
                        mixer.tracks[tid].pan = clamped;
                        params.set(PARAM_TRACK_PAN_BASE + tid, clamped);
                    }
                } else if (PARAM_ID_TRACK_MUTE_BASE..=PARAM_ID_TRACK_MUTE_BASE + 15).contains(&param_id) {
                    let tid = if param_id > PARAM_ID_TRACK_MUTE_BASE {
                        (param_id - PARAM_ID_TRACK_MUTE_BASE) as usize
                    } else {
                        track_id
                    };
                    if tid < 16 {
                        let is_muted = value > 0.5;
                        mixer.tracks[tid].mute = is_muted;
                        if is_muted {
                            mixer.tracks[tid].peak_l = 0.0;
                            mixer.tracks[tid].peak_r = 0.0;
                        }
                    }
                } else if (PARAM_ID_TRACK_SOLO_BASE..=PARAM_ID_TRACK_SOLO_BASE + 15).contains(&param_id) {
                    let tid = if param_id > PARAM_ID_TRACK_SOLO_BASE {
                        (param_id - PARAM_ID_TRACK_SOLO_BASE) as usize
                    } else {
                        track_id
                    };
                    if tid < 16 {
                        mixer.tracks[tid].solo = value > 0.5;
                    }
                } else if (PARAM_ID_TRACK_SEND_A_BASE..=PARAM_ID_TRACK_SEND_A_BASE + 15).contains(&param_id) {
                    let tid = if param_id > PARAM_ID_TRACK_SEND_A_BASE {
                        (param_id - PARAM_ID_TRACK_SEND_A_BASE) as usize
                    } else {
                        track_id
                    };
                    if tid < 16 {
                        send_a[tid] = value.clamp(0.0, 1.0);
                    }
                } else if (PARAM_ID_TRACK_SEND_B_BASE..=PARAM_ID_TRACK_SEND_B_BASE + 15).contains(&param_id) {
                    let tid = if param_id > PARAM_ID_TRACK_SEND_B_BASE {
                        (param_id - PARAM_ID_TRACK_SEND_B_BASE) as usize
                    } else {
                        track_id
                    };
                    if tid < 16 {
                        send_b[tid] = value.clamp(0.0, 1.0);
                    }
                } else if param_id == PARAM_ID_CROSSFADER {
                    *crossfader = value.clamp(-1.0, 1.0);
                } else if param_id == PARAM_ID_TEST_TONE_ENABLE {
                    *test_tone_enabled = value > 0.5;
                } else if param_id == PARAM_ID_TEST_TONE_FREQ {
                    *test_tone_freq = value.clamp(20.0, 20000.0);
                } else if param_id == PARAM_ID_TEST_TONE_VOL {
                    *test_tone_volume = value.clamp(0.0, 1.0);
                }
            }
            _ => {}
        });
    }

    /// Process a sub-block of up to MAX_BLOCK_FRAMES frames.
    fn process_sub_block(&mut self, output_chunk: &mut [f32], frames: usize) {
        // 1. Advance transport
        self.transport.advance_frames(frames, self.sample_rate as f64);

        // 2. Clear scratch buffers
        self.scratch_l[..frames].fill(0.0);
        self.scratch_r[..frames].fill(0.0);
        self.synth_buf_l[..frames].fill(0.0);
        self.synth_buf_r[..frames].fill(0.0);
        self.drum_buf_l[..frames].fill(0.0);
        self.drum_buf_r[..frames].fill(0.0);

        // 3. Render Synth Track (Track 0)
        for i in 0..frames {
            let s = self.synth.process_sample();
            self.synth_buf_l[i] = s;
            self.synth_buf_r[i] = s;
        }

        // 4. Render Drum Rack (Track 1)
        self.drum_rack.process_block(
            &mut self.drum_buf_l[..frames],
            &mut self.drum_buf_r[..frames],
        );

        // 5. Process through Mixer tracks
        let synth_audible = self.mixer.process_track(
            0,
            &mut self.synth_buf_l[..frames],
            &mut self.synth_buf_r[..frames],
        );
        let drum_audible = self.mixer.process_track(
            1,
            &mut self.drum_buf_l[..frames],
            &mut self.drum_buf_r[..frames],
        );

        // Ensure muted or zero-volume tracks render absolute silence and immediate 0.0 peak
        if !synth_audible || self.mixer.tracks[0].mute || self.mixer.tracks[0].volume_linear <= 0.001 {
            self.synth_buf_l[..frames].fill(0.0);
            self.synth_buf_r[..frames].fill(0.0);
            self.mixer.tracks[0].peak_l = 0.0;
            self.mixer.tracks[0].peak_r = 0.0;
            self.mixer.tracks[0].rms_l = 0.0;
            self.mixer.tracks[0].rms_r = 0.0;
        }
        if !drum_audible || self.mixer.tracks[1].mute || self.mixer.tracks[1].volume_linear <= 0.001 {
            self.drum_buf_l[..frames].fill(0.0);
            self.drum_buf_r[..frames].fill(0.0);
            self.mixer.tracks[1].peak_l = 0.0;
            self.mixer.tracks[1].peak_r = 0.0;
            self.mixer.tracks[1].rms_l = 0.0;
            self.mixer.tracks[1].rms_r = 0.0;
        }
        for t in 2..16 {
            if self.mixer.tracks[t].mute || self.mixer.tracks[t].volume_linear <= 0.001 {
                self.mixer.tracks[t].peak_l = 0.0;
                self.mixer.tracks[t].peak_r = 0.0;
                self.mixer.tracks[t].rms_l = 0.0;
                self.mixer.tracks[t].rms_r = 0.0;
            }
        }

        let mut any_track_audible = false;

        // 6. Sum audible tracks into Master Planar Scratch
        if synth_audible && !self.mixer.tracks[0].mute && self.mixer.tracks[0].volume_linear > 0.001 {
            any_track_audible = true;
            for i in 0..frames {
                self.scratch_l[i] += self.synth_buf_l[i];
                self.scratch_r[i] += self.synth_buf_r[i];
            }
        }
        if drum_audible && !self.mixer.tracks[1].mute && self.mixer.tracks[1].volume_linear > 0.001 {
            any_track_audible = true;
            for i in 0..frames {
                self.scratch_l[i] += self.drum_buf_l[i];
                self.scratch_r[i] += self.drum_buf_r[i];
            }
        }

        let test_tone_active = self.test_tone_enabled && self.test_tone_volume > 0.0001;

        if !any_track_audible && !test_tone_active {
            self.scratch_l[..frames].fill(0.0);
            self.scratch_r[..frames].fill(0.0);
            self.mixer.master_peak_l = 0.0;
            self.mixer.master_peak_r = 0.0;
            self.mixer.master_rms_l = 0.0;
            self.mixer.master_rms_r = 0.0;
        } else {
            if any_track_audible {
                // 7. Process Master Insert FX (Saturator -> Delay -> Reverb)
                self.saturator.process_block(
                    &mut self.scratch_l[..frames],
                    &mut self.scratch_r[..frames],
                );
                self.delay.process_block(
                    &mut self.scratch_l[..frames],
                    &mut self.scratch_r[..frames],
                );
                self.reverb.process_block(
                    &mut self.scratch_l[..frames],
                    &mut self.scratch_r[..frames],
                );
            } else {
                self.scratch_l[..frames].fill(0.0);
                self.scratch_r[..frames].fill(0.0);
            }

            // Test Tone Generator (Chapter 2: Pure reference sine tone injected into master output)
            if test_tone_active {
                let phase_step = (self.test_tone_freq * 2.0 * std::f32::consts::PI) / self.sample_rate;
                for i in 0..frames {
                    let tone = self.test_tone_phase.sin() * self.test_tone_volume;
                    self.test_tone_phase = (self.test_tone_phase + phase_step).rem_euclid(2.0 * std::f32::consts::PI);
                    self.scratch_l[i] += tone;
                    self.scratch_r[i] += tone;
                }
            }

            // 8. Master Bus & Brickwall Limiter
            let master_vol = self.params.get(PARAM_MASTER_VOL);
            self.mixer.master_volume_linear = master_vol;
            if master_vol <= 0.001 {
                self.scratch_l[..frames].fill(0.0);
                self.scratch_r[..frames].fill(0.0);
                self.mixer.master_peak_l = 0.0;
                self.mixer.master_peak_r = 0.0;
                self.mixer.master_rms_l = 0.0;
                self.mixer.master_rms_r = 0.0;
            } else {
                self.mixer.process_master(
                    &mut self.scratch_l[..frames],
                    &mut self.scratch_r[..frames],
                );
            }
        }

        // 9. Update Oscilloscope ring buffer
        for i in 0..frames {
            let mono_sample = 0.5 * (self.scratch_l[i] + self.scratch_r[i]);
            self.osc_buffer[self.osc_write_pos] = mono_sample;
            self.osc_write_pos = (self.osc_write_pos + 1) % 256;
        }

        // 10. Interleave into output chunk slice
        for (i, frame) in output_chunk.chunks_exact_mut(2).enumerate().take(frames) {
            frame[0] = self.scratch_l[i];
            frame[1] = self.scratch_r[i];
        }
    }

    /// Primary audio callback executing on real-time IOThread.
    /// Interleaved output format: `[L0, R0, L1, R1, ...]`.
    /// Supports arbitrary buffer sizes (including standard Live 12 sizes 64..2048)
    /// via zero-allocation sub-block processing.
    pub fn process_interleaved(&mut self, output: &mut [f32]) {
        let start_time = std::time::Instant::now();

        // 1. Drain SPSC commands once at the start of callback
        self.process_commands();

        let total_frames = output.len() / 2;
        if total_frames == 0 {
            return;
        }

        // 2. Process in sub-blocks of up to MAX_BLOCK_FRAMES (1024)
        let mut frame_offset = 0;
        while frame_offset < total_frames {
            let chunk_frames = (total_frames - frame_offset).min(MAX_BLOCK_FRAMES);
            let sample_start = frame_offset * 2;
            let sample_end = sample_start + chunk_frames * 2;
            let output_chunk = &mut output[sample_start..sample_end];

            self.process_sub_block(output_chunk, chunk_frames);
            frame_offset += chunk_frames;
        }

        // 3. Compute real-time CPU load (callback duration / audio buffer duration)
        let elapsed_sec = start_time.elapsed().as_secs_f32();
        let buffer_sec = total_frames as f32 / self.sample_rate;
        self.cpu_meter.record_block(elapsed_sec, buffer_sec);
        let cpu_load = self.cpu_meter.current().min(1.0);

        // 4. Update Telemetry
        let mut telemetry = AudioTelemetry::new();
        telemetry.playhead_beat = self.transport.beat;
        telemetry.cpu_load = cpu_load;
        telemetry.master_peak_l = self.mixer.master_peak_l;
        telemetry.master_peak_r = self.mixer.master_peak_r;
        telemetry.master_rms_l = self.mixer.master_rms_l;
        telemetry.master_rms_r = self.mixer.master_rms_r;
        for t in 0..16 {
            telemetry.track_peaks_l[t] = self.mixer.tracks[t].peak_l;
            telemetry.track_peaks_r[t] = self.mixer.tracks[t].peak_r;
        }

        // Copy aligned oscilloscope slice
        for k in 0..256 {
            let idx = (self.osc_write_pos + k) % 256;
            telemetry.oscilloscope_slice[k] = self.osc_buffer[idx];
        }

        // Compute fast 64-band log spectrum approximation
        let rms_val = telemetry.master_rms_l.max(telemetry.master_rms_r);
        for b in 0..64 {
            let falloff = 1.0 - (b as f32 / 64.0);
            telemetry.spectrum_bins[b] = (rms_val * falloff * (1.0 + (b as f32 * 0.1).sin() * 0.2)).min(1.0);
        }

        // Push telemetry without blocking
        self.handles.push_telemetry(telemetry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queues::AudioQueues;

    #[test]
    fn test_mixer_parameters_command_routing() {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

        // 1. Master Volume
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_MASTER_VOL,
            value: 0.75,
        });
        // 2. Track 0 Volume
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_TRACK_VOL_BASE,
            value: 1.5,
        });
        // 3. Track 1 Pan
        ui.send_command(AudioCommand::SetParameter {
            track_id: 1,
            param_id: PARAM_ID_TRACK_PAN_BASE + 1,
            value: -0.6,
        });
        // 4. Track 0 Mute
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_TRACK_MUTE_BASE,
            value: 1.0,
        });
        // 5. Track 1 Solo
        ui.send_command(AudioCommand::SetParameter {
            track_id: 1,
            param_id: PARAM_ID_TRACK_SOLO_BASE + 1,
            value: 1.0,
        });
        // 6. Track 0 Send A
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_TRACK_SEND_A_BASE,
            value: 0.42,
        });
        // 7. Track 1 Send B
        ui.send_command(AudioCommand::SetParameter {
            track_id: 1,
            param_id: PARAM_ID_TRACK_SEND_B_BASE + 1,
            value: 0.85,
        });
        // 8. Crossfader
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_CROSSFADER,
            value: 0.33,
        });

        proc.process_commands();

        assert!((proc.mixer.master_volume_linear - 0.75).abs() < 1e-4);
        assert!((proc.params.get(PARAM_MASTER_VOL) - 0.75).abs() < 1e-4);
        assert!((proc.mixer.tracks[0].volume_linear - 1.5).abs() < 1e-4);
        assert!((proc.params.get(PARAM_TRACK_VOL_BASE) - 1.5).abs() < 1e-4);
        assert!((proc.mixer.tracks[1].pan - (-0.6)).abs() < 1e-4);
        assert!((proc.params.get(PARAM_TRACK_PAN_BASE + 1) - (-0.6)).abs() < 1e-4);
        assert!(proc.mixer.tracks[0].mute);
        assert!(proc.mixer.tracks[1].solo);
        assert!((proc.send_a[0] - 0.42).abs() < 1e-4);
        assert!((proc.send_b[1] - 0.85).abs() < 1e-4);
        assert!((proc.crossfader - 0.33).abs() < 1e-4);
    }

    #[test]
    fn test_track_silence_when_muted_or_zero_vol() {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

        // Turn on a synth note and kick drum
        ui.send_command(AudioCommand::Play);
        ui.send_command(AudioCommand::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 120,
        });
        ui.send_command(AudioCommand::NoteOn {
            track_id: 1,
            note: 36,
            velocity: 127,
        });

        let mut buf = [0.0f32; 1024];
        proc.process_interleaved(&mut buf);
        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 1e-4, "Active note/drum must generate sound");

        // Now mute both Track 0 and Track 1
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_TRACK_MUTE_BASE,
            value: 1.0,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 1,
            param_id: PARAM_ID_TRACK_MUTE_BASE + 1,
            value: 1.0,
        });
        proc.process_interleaved(&mut buf);
        let energy_muted: f32 = buf.iter().map(|s| s * s).sum();
        assert_eq!(energy_muted, 0.0, "Muted tracks must render absolute silence");
        assert_eq!(proc.mixer.tracks[0].peak_l, 0.0);
        assert_eq!(proc.mixer.tracks[0].peak_r, 0.0);
        assert_eq!(proc.mixer.tracks[1].peak_l, 0.0);
        assert_eq!(proc.mixer.tracks[1].peak_r, 0.0);

        // Unmute tracks, but set their volume to 0.0
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_TRACK_MUTE_BASE,
            value: 0.0,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 1,
            param_id: PARAM_ID_TRACK_MUTE_BASE + 1,
            value: 0.0,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_TRACK_VOL_BASE,
            value: 0.0,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 1,
            param_id: PARAM_ID_TRACK_VOL_BASE + 1,
            value: 0.0,
        });
        // Retrigger drum
        ui.send_command(AudioCommand::NoteOn {
            track_id: 1,
            note: 36,
            velocity: 127,
        });
        proc.process_interleaved(&mut buf);
        let energy_zero_vol: f32 = buf.iter().map(|s| s * s).sum();
        assert_eq!(energy_zero_vol, 0.0, "Zero-volume tracks must render absolute silence");
        assert_eq!(proc.mixer.tracks[0].peak_l, 0.0);
        assert_eq!(proc.mixer.tracks[0].peak_r, 0.0);
        assert_eq!(proc.mixer.tracks[1].peak_l, 0.0);
        assert_eq!(proc.mixer.tracks[1].peak_r, 0.0);
    }

    #[test]
    fn test_master_silence_when_zero_vol() {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

        // Turn on a synth note on Track 0
        ui.send_command(AudioCommand::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 120,
        });
        // Set master volume to 0.0
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_MASTER_VOL,
            value: 0.0,
        });

        let mut buf = [0.0f32; 128];
        proc.process_interleaved(&mut buf);
        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert_eq!(energy, 0.0, "Master volume at 0 must render absolute silence");
        assert_eq!(proc.mixer.master_peak_l, 0.0);
        assert_eq!(proc.mixer.master_peak_r, 0.0);
    }

    #[test]
    fn test_buffer_sizing_64_to_2048_frames() {
        let buffer_sizes = [64, 128, 256, 512, 1024, 2048];
        let sample_rates = [44100.0, 48000.0, 88200.0, 96000.0];

        for &sr in &sample_rates {
            for &bs in &buffer_sizes {
                let queues = AudioQueues::new();
                let (mut ui, audio) = queues.split();
                let params = AtomicParameters::new();
                let mut proc = AudioEngineProcessor::new(sr, audio, params);

                ui.send_command(AudioCommand::Play);
                ui.send_command(AudioCommand::NoteOn {
                    track_id: 0,
                    note: 60,
                    velocity: 100,
                });

                // Buffer is stereo interleaved: len = bs * 2
                let mut output = vec![0.0f32; bs * 2];

                // Process enough blocks to prime the 128-sample lookahead buffer in MasterLimiter
                let num_blocks = (256 / bs).max(1) + 1;
                for _ in 0..num_blocks {
                    output.fill(0.0);
                    proc.process_interleaved(&mut output);
                }

                // Entire buffer must have energy rendered (especially at 2048 frames)
                let half_samples = bs; // bs samples = bs / 2 stereo frames
                let first_half_energy: f32 = output[..half_samples].iter().map(|s| s * s).sum();
                let second_half_energy: f32 = output[half_samples..].iter().map(|s| s * s).sum();

                assert!(
                    first_half_energy > 1e-4,
                    "First half must have audio at bs={}, sr={}",
                    bs,
                    sr
                );
                assert!(
                    second_half_energy > 1e-4,
                    "Second half must have audio at bs={}, sr={}",
                    bs,
                    sr
                );

                // Verify telemetry received
                let telem = ui.poll_telemetry();
                assert!(telem.is_some(), "Telemetry must be emitted at bs={}, sr={}", bs, sr);
                let t = telem.unwrap();
                assert!(t.cpu_load >= 0.0 && t.cpu_load <= 1.0, "CPU load must be valid percentage");
            }
        }
    }

    #[test]
    fn test_test_tone_generator_and_cpu_load() {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

        // Turn on 440 Hz test tone at 0.5 volume
        proc.enable_test_tone(true, 440.0, 0.5);

        let mut output = [0.0f32; 1024];
        proc.process_interleaved(&mut output);

        let energy: f32 = output.iter().map(|s| s * s).sum();
        assert!(energy > 10.0, "Test tone must render strong audio energy");

        let telem = ui.poll_telemetry().expect("Telemetry");
        assert!(telem.master_peak_l > 0.1, "Test tone must trigger master VU meter");
        assert!(telem.master_peak_r > 0.1, "Test tone must trigger master VU meter");

        // Turn test tone off
        proc.enable_test_tone(false, 440.0, 0.0);
        let mut silent_output = [0.0f32; 1024];
        // Process two blocks to flush lookahead buffer
        proc.process_interleaved(&mut silent_output);
        proc.process_interleaved(&mut silent_output);
        let energy_off: f32 = silent_output.iter().map(|s| s * s).sum();
        assert_eq!(energy_off, 0.0, "Disabling test tone must return to silence");
    }
}

