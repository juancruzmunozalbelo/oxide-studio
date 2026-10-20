//! Steady-state audio callback processor with zero heap allocations.

use ableton_core::{AudioCommand, AudioTelemetry, MusicalTime};
use ableton_dsp::SynthEngine;
use ableton_drums::DrumRack;
use ableton_fx::{Mixer, Saturator, TempoDelay, FdnReverb};

use crate::parameters::{AtomicParameters, PARAM_BPM, PARAM_MASTER_VOL};
use crate::queues::AudioThreadHandles;

pub const MAX_BLOCK_FRAMES: usize = 1024;

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

    /// Process incoming commands from the SPSC ring buffer. Zero allocations.
    #[inline(always)]
    fn process_commands(&mut self) {
        let synth = &mut self.synth;
        let drum_rack = &mut self.drum_rack;
        let transport = &mut self.transport;
        let delay = &mut self.delay;

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
            AudioCommand::SetParameter { track_id: _, param_id, value } => {
                if param_id == 100 {
                    synth.set_macro_x(value);
                } else if param_id == 101 {
                    synth.set_macro_y(value);
                }
            }
            _ => {}
        });
    }

    /// Primary audio callback executing on real-time IOThread.
    /// Interleaved output format: `[L0, R0, L1, R1, ...]`.
    /// Zero heap allocations during steady-state playback.
    pub fn process_interleaved(&mut self, output: &mut [f32]) {
        let total_frames = output.len() / 2;
        let frames = total_frames.min(MAX_BLOCK_FRAMES);

        // 1. Drain SPSC commands
        self.process_commands();

        // 2. Advance transport
        self.transport.advance_frames(frames, self.sample_rate as f64);

        // 3. Clear scratch buffers
        self.scratch_l[..frames].fill(0.0);
        self.scratch_r[..frames].fill(0.0);
        self.synth_buf_l[..frames].fill(0.0);
        self.synth_buf_r[..frames].fill(0.0);
        self.drum_buf_l[..frames].fill(0.0);
        self.drum_buf_r[..frames].fill(0.0);

        // 4. Render Synth Track (Track 0)
        for i in 0..frames {
            let s = self.synth.process_sample();
            self.synth_buf_l[i] = s;
            self.synth_buf_r[i] = s;
        }

        // 5. Render Drum Rack (Track 1)
        self.drum_rack.process_block(
            &mut self.drum_buf_l[..frames],
            &mut self.drum_buf_r[..frames],
        );

        // 6. Process through Mixer tracks
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

        // 7. Sum audible tracks into Master Planar Scratch
        if synth_audible {
            for i in 0..frames {
                self.scratch_l[i] += self.synth_buf_l[i];
                self.scratch_r[i] += self.synth_buf_r[i];
            }
        }
        if drum_audible {
            for i in 0..frames {
                self.scratch_l[i] += self.drum_buf_l[i];
                self.scratch_r[i] += self.drum_buf_r[i];
            }
        }

        // 8. Process Master Insert FX (Saturator -> Delay -> Reverb)
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

        // 9. Master Bus & Brickwall Limiter
        let master_vol = self.params.get(PARAM_MASTER_VOL);
        self.mixer.master_volume_linear = master_vol;
        self.mixer.process_master(
            &mut self.scratch_l[..frames],
            &mut self.scratch_r[..frames],
        );

        // 10. Update Oscilloscope & Spectrum Telemetry
        for i in 0..frames {
            let mono_sample = 0.5 * (self.scratch_l[i] + self.scratch_r[i]);
            self.osc_buffer[self.osc_write_pos] = mono_sample;
            self.osc_write_pos = (self.osc_write_pos + 1) % 256;
        }

        let mut telemetry = AudioTelemetry::new();
        telemetry.playhead_beat = self.transport.beat;
        telemetry.master_peak_l = self.mixer.master_peak_l;
        telemetry.master_peak_r = self.mixer.master_peak_r;
        telemetry.master_rms_l = self.mixer.master_rms_l;
        telemetry.master_rms_r = self.mixer.master_rms_r;
        telemetry.track_peaks_l[0] = self.mixer.tracks[0].peak_l;
        telemetry.track_peaks_r[0] = self.mixer.tracks[0].peak_r;
        telemetry.track_peaks_l[1] = self.mixer.tracks[1].peak_l;
        telemetry.track_peaks_r[1] = self.mixer.tracks[1].peak_r;

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

        // 11. Interleave into output slice
        for (i, frame) in output.chunks_exact_mut(2).enumerate().take(frames) {
            frame[0] = self.scratch_l[i];
            frame[1] = self.scratch_r[i];
        }
    }
}
