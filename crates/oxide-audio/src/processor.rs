//! Steady-state audio callback processor with zero heap allocations.

use oxide_core::{AudioCommand, AudioTelemetry, MusicalTime};
use oxide_dsp::SynthEngine;
use oxide_drums::DrumRack;
use oxide_fx::{Mixer, Saturator, TempoDelay, FdnReverb};

use crate::parameters::{
    AtomicParameters, PARAM_BPM, PARAM_FX_DRIVE, PARAM_MASTER_VOL, PARAM_TRACK_PAN_BASE,
    PARAM_TRACK_VOL_BASE,
};
use crate::queues::AudioThreadHandles;

pub const MAX_BLOCK_FRAMES: usize = 1024;

// Parameter IDs for AudioCommand::SetParameter
pub const PARAM_ID_MASTER_VOL: u32 = 1;
pub const PARAM_ID_TEST_TONE_ENABLE: u32 = 2;
pub const PARAM_ID_TEST_TONE_FREQ: u32 = 3;
pub const PARAM_ID_TEST_TONE_VOL: u32 = 4;
pub const PARAM_ID_SYNTH_CUTOFF: u32 = 10;
pub const PARAM_ID_SYNTH_RESONANCE: u32 = 11;
pub const PARAM_ID_SYNTH_PULSE_PW: u32 = 12;
pub const PARAM_ID_SYNTH_ENV_MOD: u32 = 13;
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
    scratch_l: Box<[f32]>,
    scratch_r: Box<[f32]>,
    synth_buf_l: Box<[f32]>,
    synth_buf_r: Box<[f32]>,
    drum_buf_l: Box<[f32]>,
    drum_buf_r: Box<[f32]>,
    send_a_l: Box<[f32]>,
    send_a_r: Box<[f32]>,
    send_b_l: Box<[f32]>,
    send_b_r: Box<[f32]>,
    send_a_active_tail: bool,
    send_b_active_tail: bool,

    // Oscilloscope ring buffer & precomputed 64-band Goertzel analyzer
    osc_buffer: [f32; 256],
    osc_write_pos: usize,
    spec_hann: [f32; 256],
    spec_coeff: [f32; 64],
    spec_cos: [f32; 64],
    spec_sin: [f32; 64],
    spec_norm: f32,
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

        let mut spec_hann = [0.0f32; 256];
        let mut hann_sum = 0.0f32;
        for n in 0..256 {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * (n as f32) / 256.0).cos());
            spec_hann[n] = w;
            hann_sum += w;
        }
        let spec_norm = if hann_sum > 0.0 { 2.0 / hann_sum } else { 1.0 / 128.0 };

        let min_freq = 20.0f32;
        let max_freq = (sr * 0.45).min(18000.0).max(min_freq * 2.0);
        let mut spec_coeff = [0.0f32; 64];
        let mut spec_cos = [0.0f32; 64];
        let mut spec_sin = [0.0f32; 64];
        for b in 0..64 {
            let t = b as f32 / 63.0;
            let freq = min_freq * (max_freq / min_freq).powf(t);
            let omega = 2.0 * std::f32::consts::PI * freq / sr;
            let c = omega.cos();
            spec_coeff[b] = 2.0 * c;
            spec_cos[b] = c;
            spec_sin[b] = omega.sin();
        }

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
            scratch_l: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            scratch_r: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            synth_buf_l: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            synth_buf_r: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            drum_buf_l: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            drum_buf_r: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            send_a_l: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            send_a_r: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            send_b_l: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            send_b_r: vec![0.0; MAX_BLOCK_FRAMES].into_boxed_slice(),
            send_a_active_tail: false,
            send_b_active_tail: false,
            osc_buffer: [0.0; 256],
            osc_write_pos: 0,
            spec_hann,
            spec_coeff,
            spec_cos,
            spec_sin,
            spec_norm,
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
        let saturator = &mut self.saturator;
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
                    let safe_vol = if value.is_finite() {
                        value.clamp(0.0, 2.0)
                    } else {
                        0.0
                    };
                    mixer.master_volume_linear = safe_vol;
                    params.set(PARAM_MASTER_VOL, safe_vol);
                } else if param_id == PARAM_ID_SYNTH_CUTOFF {
                    if value.is_finite() {
                        synth.set_parameter("filter_frequency", value.clamp(20.0, 20000.0));
                    }
                } else if param_id == PARAM_ID_SYNTH_RESONANCE {
                    if value.is_finite() {
                        // Accept either normalized [0.0..=1.0] or GUI Q-scale [>1.0..=5.0]
                        let norm = if value > 1.0 {
                            (value / 5.0).clamp(0.0, 1.0)
                        } else {
                            value.clamp(0.0, 1.0)
                        };
                        synth.set_parameter("filter_resonance", norm);
                    }
                } else if param_id == PARAM_ID_SYNTH_PULSE_PW {
                    if value.is_finite() {
                        let pw_norm = ((value.clamp(0.05, 0.95) - 0.5) / 0.45).clamp(-1.0, 1.0);
                        synth.set_parameter("oscillator_pulse_width", pw_norm);
                    }
                } else if param_id == PARAM_ID_SYNTH_ENV_MOD {
                    if value.is_finite() {
                        synth.set_parameter("filter_envelope_amount", value.clamp(-1.0, 1.0));
                    }
                } else if param_id == PARAM_FX_DRIVE as u32 {
                    if value.is_finite() {
                        let drive_norm = value.clamp(0.0, 2.0);
                        params.set(PARAM_FX_DRIVE, drive_norm);
                        saturator.set_drive_db(drive_norm * 12.0);
                    }
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
                        let safe_val = if value.is_finite() { value } else { 0.0 };
                        let lin = if safe_val <= 0.001 { 0.0 } else { safe_val.clamp(0.0, 4.0) };
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
                        let clamped = if value.is_finite() { value.clamp(-1.0, 1.0) } else { 0.0 };
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
                        send_a[tid] = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
                    }
                } else if (PARAM_ID_TRACK_SEND_B_BASE..=PARAM_ID_TRACK_SEND_B_BASE + 15).contains(&param_id) {
                    let tid = if param_id > PARAM_ID_TRACK_SEND_B_BASE {
                        (param_id - PARAM_ID_TRACK_SEND_B_BASE) as usize
                    } else {
                        track_id
                    };
                    if tid < 16 {
                        send_b[tid] = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
                    }
                } else if param_id == PARAM_ID_CROSSFADER {
                    *crossfader = if value.is_finite() { value.clamp(-1.0, 1.0) } else { 0.0 };
                } else if param_id == PARAM_ID_TEST_TONE_ENABLE {
                    *test_tone_enabled = value > 0.5;
                } else if param_id == PARAM_ID_TEST_TONE_FREQ {
                    if value.is_finite() {
                        *test_tone_freq = value.clamp(20.0, 20000.0);
                    }
                } else if param_id == PARAM_ID_TEST_TONE_VOL {
                    *test_tone_volume = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
                }
            }
            _ => {}
        });
    }

    /// Process a sub-block of up to MAX_BLOCK_FRAMES frames.
    fn process_sub_block(&mut self, output_chunk: &mut [f32], frames: usize, channels: usize) {
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
        let track0_active =
            synth_audible && !self.mixer.tracks[0].mute && self.mixer.tracks[0].volume_linear > 0.001;
        let track1_active =
            drum_audible && !self.mixer.tracks[1].mute && self.mixer.tracks[1].volume_linear > 0.001;

        self.send_a_l[..frames].fill(0.0);
        self.send_a_r[..frames].fill(0.0);
        self.send_b_l[..frames].fill(0.0);
        self.send_b_r[..frames].fill(0.0);

        let xf = if self.crossfader.is_finite() {
            self.crossfader.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        // Equal-power DJ crossfader gains: Track 0 -> A, Track 1 -> B
        let xf_gain_0 = if xf <= 0.0 {
            1.0
        } else if xf >= 1.0 {
            0.0
        } else {
            (0.5 * std::f32::consts::PI * xf).cos().max(0.0)
        };
        let xf_gain_1 = if xf >= 0.0 {
            1.0
        } else if xf <= -1.0 {
            0.0
        } else {
            (0.5 * std::f32::consts::PI * xf.abs()).cos().max(0.0)
        };

        // 6. Sum audible tracks into Master Planar Scratch + Send A/B Buses
        let send_a_0 = if self.send_a[0].is_finite() {
            self.send_a[0].clamp(0.0, 1.0)
        } else {
            0.0
        };
        let send_b_0 = if self.send_b[0].is_finite() {
            self.send_b[0].clamp(0.0, 1.0)
        } else {
            0.0
        };
        if track0_active && xf_gain_0 > 0.0 {
            any_track_audible = true;
            for i in 0..frames {
                let l = self.synth_buf_l[i] * xf_gain_0;
                let r = self.synth_buf_r[i] * xf_gain_0;
                self.scratch_l[i] += l;
                self.scratch_r[i] += r;
                if send_a_0 > 0.0 {
                    self.send_a_l[i] += l * send_a_0;
                    self.send_a_r[i] += r * send_a_0;
                }
                if send_b_0 > 0.0 {
                    self.send_b_l[i] += l * send_b_0;
                    self.send_b_r[i] += r * send_b_0;
                }
            }
        }

        let send_a_1 = if self.send_a[1].is_finite() {
            self.send_a[1].clamp(0.0, 1.0)
        } else {
            0.0
        };
        let send_b_1 = if self.send_b[1].is_finite() {
            self.send_b[1].clamp(0.0, 1.0)
        } else {
            0.0
        };
        if track1_active && xf_gain_1 > 0.0 {
            any_track_audible = true;
            for i in 0..frames {
                let l = self.drum_buf_l[i] * xf_gain_1;
                let r = self.drum_buf_r[i] * xf_gain_1;
                self.scratch_l[i] += l;
                self.scratch_r[i] += r;
                if send_a_1 > 0.0 {
                    self.send_a_l[i] += l * send_a_1;
                    self.send_a_r[i] += r * send_a_1;
                }
                if send_b_1 > 0.0 {
                    self.send_b_l[i] += l * send_b_1;
                    self.send_b_r[i] += r * send_b_1;
                }
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
                // 7. Process Master Insert Saturator & Return A (Delay) / Return B (Reverb) Sends
                self.saturator.process_block(
                    &mut self.scratch_l[..frames],
                    &mut self.scratch_r[..frames],
                );
                let send_a_active =
                    (track0_active && xf_gain_0 > 0.0 && send_a_0 > 0.0001)
                        || (track1_active && xf_gain_1 > 0.0 && send_a_1 > 0.0001);
                if send_a_active || self.send_a_active_tail {
                    self.delay.process_block(
                        &mut self.send_a_l[..frames],
                        &mut self.send_a_r[..frames],
                    );
                    let mut peak_ret = 0.0f32;
                    for i in 0..frames {
                        let l = self.send_a_l[i];
                        let r = self.send_a_r[i];
                        self.scratch_l[i] += l;
                        self.scratch_r[i] += r;
                        peak_ret = peak_ret.max(l.abs()).max(r.abs());
                    }
                    self.send_a_active_tail = peak_ret > 1e-6;
                }

                let send_b_active =
                    (track0_active && xf_gain_0 > 0.0 && send_b_0 > 0.0001)
                        || (track1_active && xf_gain_1 > 0.0 && send_b_1 > 0.0001);
                if send_b_active || self.send_b_active_tail {
                    self.reverb.process_block(
                        &mut self.send_b_l[..frames],
                        &mut self.send_b_r[..frames],
                    );
                    let mut peak_ret = 0.0f32;
                    for i in 0..frames {
                        let l = self.send_b_l[i];
                        let r = self.send_b_r[i];
                        self.scratch_l[i] += l;
                        self.scratch_r[i] += r;
                        peak_ret = peak_ret.max(l.abs()).max(r.abs());
                    }
                    self.send_b_active_tail = peak_ret > 1e-6;
                }
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
            let raw_master_vol = self.params.get(PARAM_MASTER_VOL);
            let master_vol = if raw_master_vol.is_finite() {
                raw_master_vol.clamp(0.0, 2.0)
            } else {
                0.0
            };
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

        // 10. Interleave into output chunk slice supporting arbitrary hardware channel counts
        if channels == 1 {
            for (i, frame) in output_chunk.chunks_exact_mut(1).enumerate().take(frames) {
                frame[0] = 0.5 * (self.scratch_l[i] + self.scratch_r[i]);
            }
        } else {
            for (i, frame) in output_chunk.chunks_exact_mut(channels).enumerate().take(frames) {
                frame[0] = self.scratch_l[i];
                frame[1] = self.scratch_r[i];
                for ch in 2..channels {
                    frame[ch] = 0.0;
                }
            }
        }
    }

    /// Primary audio callback executing on real-time IOThread.
    /// Interleaved output format: `[L0, R0, L1, R1, ...]`.
    /// Supports arbitrary buffer sizes (including standard Live 12 sizes 64..2048)
    /// via zero-allocation sub-block processing.
    pub fn process_interleaved(&mut self, output: &mut [f32]) {
        self.process_interleaved_with_channels(output, 2);
    }

    /// Interleaved audio callback supporting arbitrary output channel counts
    /// (mono downmix for 1ch, stereo on ch 0/1 with zeroed extra channels for >2ch).
    pub fn process_interleaved_with_channels(&mut self, output: &mut [f32], channels: usize) {
        let start_time = std::time::Instant::now();

        // 1. Drain SPSC commands once at the start of callback
        self.process_commands();

        if channels == 0 {
            return;
        }
        let total_frames = output.len() / channels;
        if total_frames == 0 {
            return;
        }

        // 2. Process in sub-blocks of up to MAX_BLOCK_FRAMES (1024)
        let mut frame_offset = 0;
        while frame_offset < total_frames {
            let chunk_frames = (total_frames - frame_offset).min(MAX_BLOCK_FRAMES);
            let sample_start = frame_offset * channels;
            let sample_end = sample_start + chunk_frames * channels;
            let output_chunk = &mut output[sample_start..sample_end];

            self.process_sub_block(output_chunk, chunk_frames, channels);
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

        // Copy aligned oscilloscope slice & apply Hann window for Goertzel spectral analysis
        let mut windowed = [0.0f32; 256];
        let mut osc_energy = 0.0f32;
        for k in 0..256 {
            let idx = (self.osc_write_pos + k) % 256;
            let s = self.osc_buffer[idx];
            telemetry.oscilloscope_slice[k] = s;
            let w = s * self.spec_hann[k];
            windowed[k] = w;
            osc_energy += w * w;
        }

        // Compute real 64-band log-spaced spectral magnitudes via precomputed Goertzel bank
        if osc_energy > 1e-12 {
            for b in 0..64 {
                let coeff = self.spec_coeff[b];
                let mut s1 = 0.0f32;
                let mut s2 = 0.0f32;
                for &x in &windowed {
                    let s0 = x + coeff * s1 - s2;
                    s2 = s1;
                    s1 = s0;
                }
                let real = s1 - self.spec_cos[b] * s2;
                let imag = self.spec_sin[b] * s2;
                let mag = (real * real + imag * imag).max(0.0).sqrt() * self.spec_norm;
                telemetry.spectrum_bins[b] = if mag.is_finite() {
                    mag.clamp(0.0, 1.0)
                } else {
                    0.0
                };
            }
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

    #[test]
    fn test_synth_params_and_master_clamp() {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_MASTER_VOL,
            value: 5000.0,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_SYNTH_CUTOFF,
            value: 1200.0,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_SYNTH_RESONANCE,
            value: 0.75,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_SYNTH_PULSE_PW,
            value: 0.95,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_SYNTH_ENV_MOD,
            value: -0.5,
        });

        proc.process_commands();

        assert!((proc.mixer.master_volume_linear - 2.0).abs() < 1e-6);
        assert!((proc.params.get(PARAM_MASTER_VOL) - 2.0).abs() < 1e-6);
        assert!((proc.synth.params.get_value("filter_frequency") - 1200.0).abs() < 1e-3);
        assert!((proc.synth.params.get_value("filter_resonance") - 0.75).abs() < 1e-4);
        assert!((proc.synth.params.get_value("oscillator_pulse_width") - 1.0).abs() < 1e-4);
        assert!((proc.synth.params.get_value("filter_envelope_amount") - (-0.5)).abs() < 1e-4);
    }

    #[test]
    fn test_multichannel_interleaved_sends_crossfader_and_spectrum() {
        let queues = AudioQueues::new();
        let (mut ui, audio) = queues.split();
        let params = AtomicParameters::new();
        let mut proc = AudioEngineProcessor::new(44100.0, audio, params);

        // Pure 440 Hz tone on 4 channels: ch 0/1 contain audio, ch 2/3 zeroed, Goertzel peaks near 440 Hz
        proc.enable_test_tone(true, 440.0, 0.5);
        let mut quad_buf = vec![99.0f32; 256 * 4];
        proc.process_interleaved_with_channels(&mut quad_buf, 4);
        for f in 0..256 {
            assert_eq!(quad_buf[f * 4 + 2], 0.0);
            assert_eq!(quad_buf[f * 4 + 3], 0.0);
        }
        let quad_energy: f32 = quad_buf.chunks_exact(4).map(|c| c[0] * c[0]).sum();
        assert!(quad_energy > 1.0);

        let telem = ui.poll_telemetry().expect("Spectrum telemetry");
        let peak_bin = telem
            .spectrum_bins
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(idx, _)| idx)
            .unwrap();
        assert!(
            (25..=32).contains(&peak_bin),
            "440 Hz tone should peak around mid-low log bins (25..32), got {}",
            peak_bin
        );
        assert!(telem.spectrum_bins[peak_bin] > telem.spectrum_bins[63] * 5.0);

        // Mono 1ch downmix
        let mut mono_buf = vec![0.0f32; 128];
        proc.process_interleaved_with_channels(&mut mono_buf, 1);
        let mono_energy: f32 = mono_buf.iter().map(|s| s * s).sum();
        assert!(mono_energy > 1.0);

        // Crossfader and sends isolation: flush 128-sample limiter lookahead after disabling test tone,
        // then verify Track 0 (A) is completely silenced when crossfader is full B (1.0).
        proc.enable_test_tone(false, 440.0, 0.0);
        let mut flush_buf = [0.0f32; 512];
        proc.process_interleaved(&mut flush_buf);
        ui.send_command(AudioCommand::Play);
        ui.send_command(AudioCommand::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 120,
        });
        ui.send_command(AudioCommand::SetParameter {
            track_id: 0,
            param_id: PARAM_ID_CROSSFADER,
            value: 1.0,
        });
        let mut xf_buf = [0.0f32; 256];
        proc.process_interleaved(&mut xf_buf);
        let xf_energy: f32 = xf_buf.iter().map(|s| s * s).sum();
        assert_eq!(xf_energy, 0.0, "Full B crossfader must silence Track 0 (A)");
    }
}

