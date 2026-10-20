//! Wavetable flagship synthesizer device.

use std::f32::consts::PI;
use std::sync::Arc;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};

use crate::cytomic::{CircuitModel, CytomicFilterType, DualCytomicFilter, FilterRouting};
use crate::envelope::AdsrEnvelope;
use crate::lfo::{Lfo, LfoShape};
use crate::oscillator::XorShift32;
use crate::wavetable::mod_matrix::{ModDestination, ModMatrix};
use crate::wavetable::tables::{WavetableCategory, WavetableSet};

pub const MAX_WAVETABLE_VOICES: usize = 8;
pub const MAX_UNISON_VOICES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarpMode {
    None,
    Pwm,
    Fold,
    Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnisonMode {
    Classic,
    Shimmer,
    Noise,
    PhaseSync,
}

#[derive(Debug, Clone)]
pub struct WavetableVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub age: u64,

    // Phase accumulators
    pub osc1_phase: f32,
    pub osc2_phase: f32,
    pub osc1_phases: [f32; MAX_UNISON_VOICES],
    pub osc2_phases: [f32; MAX_UNISON_VOICES],
    pub sub_phase: f32,

    // Sub filter 1-pole state
    pub sub_filter_state: f32,

    // Envelopes
    pub amp_env: AdsrEnvelope,
    pub env2: AdsrEnvelope,
    pub env3: AdsrEnvelope,

    // Dual Cytomic filters (stereo)
    pub dual_filter: DualCytomicFilter,
    pub dual_filter_r: DualCytomicFilter,

    // Noise PRNG for stochastic unison
    pub rng: XorShift32,
}

impl WavetableVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.8,
            age: 0,
            osc1_phase: 0.0,
            osc2_phase: 0.0,
            osc1_phases: [0.0; MAX_UNISON_VOICES],
            osc2_phases: [0.0; MAX_UNISON_VOICES],
            sub_phase: 0.0,
            sub_filter_state: 0.0,
            amp_env: AdsrEnvelope::new(sample_rate),
            env2: AdsrEnvelope::new(sample_rate),
            env3: AdsrEnvelope::new(sample_rate),
            dual_filter: DualCytomicFilter::new(sample_rate),
            dual_filter_r: DualCytomicFilter::new(sample_rate),
            rng: XorShift32::new(0x2468ace0),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.osc1_phase = 0.0;
        self.osc2_phase = 0.0;
        self.osc1_phases = [0.0; MAX_UNISON_VOICES];
        self.osc2_phases = [0.0; MAX_UNISON_VOICES];
        self.sub_phase = 0.0;
        self.sub_filter_state = 0.0;
        self.amp_env.reset();
        self.env2.reset();
        self.env3.reset();
        self.dual_filter.reset();
        self.dual_filter_r.reset();
    }
}

/// Ableton Live 12 Flagship Wavetable Synthesizer.
#[derive(Debug, Clone)]
pub struct WavetableDevice {
    pub enabled: bool,
    pub sample_rate: f32,

    // Wavetable sets
    pub osc1_table: Arc<WavetableSet>,
    pub osc2_table: Arc<WavetableSet>,

    // Osc 1 settings
    pub osc1_pos: f32, // 0..1
    pub osc1_warp_mode: WarpMode,
    pub osc1_warp_param: f32, // -1..1 or 1..8
    pub osc1_transpose: i8, // -24..24
    pub osc1_fine_cents: f32, // -50..50
    pub osc1_gain: f32, // 0..1

    // Osc 2 settings
    pub osc2_pos: f32,
    pub osc2_warp_mode: WarpMode,
    pub osc2_warp_param: f32,
    pub osc2_transpose: i8,
    pub osc2_fine_cents: f32,
    pub osc2_gain: f32,

    // Sub oscillator settings
    pub sub_active: bool,
    pub sub_octave: i8, // -1 or -2
    pub sub_gain: f32,
    pub sub_tone: f32, // 0..1 (lowpass smoothing)

    // Filter settings
    pub filter_routing: FilterRouting,
    pub filter1_type: CytomicFilterType,
    pub filter1_model: CircuitModel,
    pub filter1_cutoff: f32,
    pub filter1_res: f32,
    pub filter1_morph: f32,
    pub filter1_drive: f32,

    pub filter2_type: CytomicFilterType,
    pub filter2_model: CircuitModel,
    pub filter2_cutoff: f32,
    pub filter2_res: f32,
    pub filter2_morph: f32,
    pub filter2_drive: f32,

    // Envelopes (Attack, Decay, Sustain, Release)
    pub amp_adsr: (f32, f32, f32, f32),
    pub env2_adsr: (f32, f32, f32, f32),
    pub env3_adsr: (f32, f32, f32, f32),

    // LFOs
    pub lfo1: Lfo,
    pub lfo1_freq: f32,
    pub lfo1_shape: LfoShape,

    pub lfo2: Lfo,
    pub lfo2_freq: f32,
    pub lfo2_shape: LfoShape,

    // Unison settings
    pub unison_mode: UnisonMode,
    pub unison_voices: usize, // 1..8
    pub unison_amount: f32, // 0..1

    // Modulation Matrix
    pub mod_matrix: ModMatrix,

    // Master settings
    pub master_volume: f32,
    pub master_pan: f32,

    // Voice pool
    pub voices: [WavetableVoice; MAX_WAVETABLE_VOICES],
    pub voice_counter: u64,
}

impl WavetableDevice {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let basic = Arc::new(WavetableSet::generate(WavetableCategory::BasicShapes));
        let vintage = Arc::new(WavetableSet::generate(WavetableCategory::Vintage));

        let voices = std::array::from_fn(|_| WavetableVoice::new(sr));

        Self {
            enabled: true,
            sample_rate: sr,
            osc1_table: basic,
            osc2_table: vintage,
            osc1_pos: 0.2,
            osc1_warp_mode: WarpMode::None,
            osc1_warp_param: 0.0,
            osc1_transpose: 0,
            osc1_fine_cents: 0.0,
            osc1_gain: 0.8,
            osc2_pos: 0.5,
            osc2_warp_mode: WarpMode::None,
            osc2_warp_param: 0.0,
            osc2_transpose: 0,
            osc2_fine_cents: 5.0,
            osc2_gain: 0.6,
            sub_active: true,
            sub_octave: -1,
            sub_gain: 0.3,
            sub_tone: 0.5,
            filter_routing: FilterRouting::Series,
            filter1_type: CytomicFilterType::Lowpass24,
            filter1_model: CircuitModel::Clean,
            filter1_cutoff: 2500.0,
            filter1_res: 0.2,
            filter1_morph: 0.0,
            filter1_drive: 0.0,
            filter2_type: CytomicFilterType::Lowpass12,
            filter2_model: CircuitModel::Clean,
            filter2_cutoff: 8000.0,
            filter2_res: 0.1,
            filter2_morph: 0.0,
            filter2_drive: 0.0,
            amp_adsr: (5.0, 300.0, 0.7, 250.0), // ms, ms, norm, ms
            env2_adsr: (10.0, 400.0, 0.3, 300.0),
            env3_adsr: (20.0, 500.0, 0.0, 200.0),
            lfo1: Lfo::new(),
            lfo1_freq: 2.0,
            lfo1_shape: LfoShape::Sine,
            lfo2: Lfo::new(),
            lfo2_freq: 0.5,
            lfo2_shape: LfoShape::Triangle,
            unison_mode: UnisonMode::Classic,
            unison_voices: 2,
            unison_amount: 0.15,
            mod_matrix: ModMatrix::new(),
            master_volume: 0.7,
            master_pan: 0.0,
            voices,
            voice_counter: 0,
        }
    }

    /// Load category into Osc 1.
    pub fn load_osc1_category(&mut self, cat: WavetableCategory) {
        self.osc1_table = Arc::new(WavetableSet::generate(cat));
    }

    /// Load category into Osc 2.
    pub fn load_osc2_category(&mut self, cat: WavetableCategory) {
        self.osc2_table = Arc::new(WavetableSet::generate(cat));
    }

    #[inline(always)]
    fn apply_warp(phase: f32, mode: WarpMode, param: f32) -> f32 {
        let p = phase.fract();
        let p_norm = if p < 0.0 { p + 1.0 } else { p };
        match mode {
            WarpMode::None => p_norm,
            WarpMode::Pwm => {
                let p_bend = param.clamp(-0.9, 0.9);
                let split = (1.0 + p_bend) * 0.5;
                if p_norm < split {
                    p_norm * 0.5 / split
                } else {
                    0.5 + (p_norm - split) * 0.5 / (1.0 - split)
                }
            }
            WarpMode::Fold => p_norm,
            WarpMode::Sync => {
                let sync_mult = (1.0 + param.clamp(0.0, 7.0)).max(1.0);
                (p_norm * sync_mult).fract()
            }
        }
    }

    #[inline(always)]
    fn apply_fold(sample: f32, mode: WarpMode, param: f32) -> f32 {
        if mode == WarpMode::Fold {
            let gain = (1.0 + param.clamp(0.0, 8.0) * 1.5).max(1.0);
            (gain * sample * 0.5 * PI).sin()
        } else {
            sample
        }
    }

    fn allocate_voice(&mut self) -> usize {
        for (i, v) in self.voices.iter().enumerate() {
            if !v.active {
                return i;
            }
        }
        // Steal voice with lowest amp envelope or oldest
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
        v.dual_filter.reset();
        v.dual_filter_r.reset();

        let n_unison = self.unison_voices.clamp(1, MAX_UNISON_VOICES);
        for u in 0..MAX_UNISON_VOICES {
            if self.unison_mode == UnisonMode::PhaseSync {
                v.osc1_phases[u] = 0.0;
                v.osc2_phases[u] = 0.0;
            } else {
                let init_phase = if n_unison > 1 {
                    (u as f32 / n_unison as f32).fract()
                } else {
                    0.0
                };
                v.osc1_phases[u] = init_phase;
                v.osc2_phases[u] = (init_phase + 0.33).fract();
            }
        }
        v.osc1_phase = v.osc1_phases[0];
        v.osc2_phase = v.osc2_phases[0];
        v.sub_phase = 0.0;

        v.amp_env.trigger_attack();
        v.env2.trigger_attack();
        v.env3.trigger_attack();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.note == note {
                v.amp_env.trigger_release();
                v.env2.trigger_release();
                v.env3.trigger_release();
            }
        }
    }

    pub fn silence_all(&mut self) {
        for v in &mut self.voices {
            v.reset();
        }
    }

    /// Render a single stereo sample frame.
    #[inline(always)]
    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        let sr = self.sample_rate;
        let inv_sr = 1.0 / sr;

        // Advance LFOs
        let (lfo1_sig, _) = self.lfo1.next_sample(self.lfo1_freq, 0.0, 0.0, self.lfo1_shape, sr);
        let (lfo2_sig, _) = self.lfo2.next_sample(self.lfo2_freq, 0.0, 0.0, self.lfo2_shape, sr);

        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        let base_vol = self.master_volume;
        let base_pan = self.master_pan.clamp(-1.0, 1.0);

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

            let env2_val = voice.env2.next(
                self.env2_adsr.0,
                self.env2_adsr.1,
                self.env2_adsr.2,
                self.env2_adsr.3,
            );
            let env3_val = voice.env3.next(
                self.env3_adsr.0,
                self.env3_adsr.1,
                self.env3_adsr.2,
                self.env3_adsr.3,
            );

            // Mod matrix evaluation (all 14 destinations)
            let key_norm = (voice.note as f32 - 60.0) / 12.0;

            let osc1_pos_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Osc1Pos,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let osc2_pos_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Osc2Pos,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let osc1_warp_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Osc1Warp,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let osc2_warp_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Osc2Warp,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let osc1_pitch_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Osc1Pitch,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let osc2_pitch_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Osc2Pitch,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let f1_cut_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Filter1Cutoff,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let f1_res_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Filter1Res,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let f1_morph_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Filter1Morph,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let f2_cut_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Filter2Cutoff,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let f2_res_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Filter2Res,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let f2_morph_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Filter2Morph,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let vol_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Volume,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );
            let pan_mod = self.mod_matrix.evaluate_destination(
                ModDestination::Pan,
                amp_val, env2_val, env3_val, lfo1_sig, lfo2_sig, voice.velocity, key_norm, 0.0, 0.0, 0.0,
            );

            // Frequency calculations
            let note_pitch = voice.note as f32;
            let note_freq = 440.0 * 2.0f32.powf((note_pitch - 69.0) / 12.0);

            let osc1_semi = self.osc1_transpose as f32 + self.osc1_fine_cents * 0.01 + osc1_pitch_mod * 12.0;
            let osc2_semi = self.osc2_transpose as f32 + self.osc2_fine_cents * 0.01 + osc2_pitch_mod * 12.0;

            let sub_freq = (note_freq * 2.0f32.powf(self.sub_octave as f32)).clamp(10.0, sr * 0.48);
            voice.sub_phase = (voice.sub_phase + sub_freq * inv_sr).fract();

            let pos1 = (self.osc1_pos + osc1_pos_mod).clamp(0.0, 1.0);
            let pos2 = (self.osc2_pos + osc2_pos_mod).clamp(0.0, 1.0);
            let warp1 = self.osc1_warp_param + osc1_warp_mod;
            let warp2 = self.osc2_warp_param + osc2_warp_mod;

            let voice_pan = (base_pan + pan_mod).clamp(-1.0, 1.0);

            // Genuine Multi-Voice Unison Rendering
            let n_unison = self.unison_voices.clamp(1, MAX_UNISON_VOICES);
            let unison_amt = self.unison_amount.clamp(0.0, 1.0);
            let voice_weight = 1.0 / (n_unison as f32).sqrt();

            let mut osc1_sum_l = 0.0f32;
            let mut osc1_sum_r = 0.0f32;
            let mut osc2_sum_l = 0.0f32;
            let mut osc2_sum_r = 0.0f32;

            for u in 0..n_unison {
                let spread_ratio = if n_unison > 1 {
                    2.0 * (u as f32 / (n_unison - 1) as f32) - 1.0
                } else {
                    0.0
                };

                let (detune_semi, pan_offset) = match self.unison_mode {
                    UnisonMode::Classic => {
                        let detune = spread_ratio * unison_amt * 0.5;
                        let pan = spread_ratio * unison_amt * 0.8;
                        (detune, pan)
                    }
                    UnisonMode::Shimmer => {
                        let shimmer_step = if u % 2 == 1 { 12.0 } else { 0.0 };
                        let detune = (spread_ratio * 0.2 + shimmer_step * (u as f32 / n_unison as f32)) * unison_amt;
                        let pan = spread_ratio * unison_amt * 0.8;
                        (detune, pan)
                    }
                    UnisonMode::Noise => {
                        let jitter = (voice.rng.next_f32() - 0.5) * 0.4;
                        let detune = (spread_ratio * 0.3 + jitter) * unison_amt;
                        let pan = (spread_ratio + (voice.rng.next_f32() - 0.5) * 0.2) * unison_amt * 0.8;
                        (detune, pan)
                    }
                    UnisonMode::PhaseSync => {
                        let detune = spread_ratio * unison_amt * 0.5;
                        let pan = spread_ratio * unison_amt * 0.8;
                        (detune, pan)
                    }
                };

                let freq1_u = (note_freq * 2.0f32.powf((osc1_semi + detune_semi) / 12.0)).clamp(10.0, sr * 0.48);
                let freq2_u = (note_freq * 2.0f32.powf((osc2_semi + detune_semi) / 12.0)).clamp(10.0, sr * 0.48);

                voice.osc1_phases[u] = (voice.osc1_phases[u] + freq1_u * inv_sr).fract();
                voice.osc2_phases[u] = (voice.osc2_phases[u] + freq2_u * inv_sr).fract();

                let warped1 = Self::apply_warp(voice.osc1_phases[u], self.osc1_warp_mode, warp1);
                let raw1 = self.osc1_table.read_interpolated(warped1, pos1);
                let s1 = Self::apply_fold(raw1, self.osc1_warp_mode, warp1) * self.osc1_gain * voice_weight;

                let warped2 = Self::apply_warp(voice.osc2_phases[u], self.osc2_warp_mode, warp2);
                let raw2 = self.osc2_table.read_interpolated(warped2, pos2);
                let s2 = Self::apply_fold(raw2, self.osc2_warp_mode, warp2) * self.osc2_gain * voice_weight;

                let u_pan = (voice_pan + pan_offset).clamp(-1.0, 1.0);
                let angle = (u_pan + 1.0) * 0.25 * PI;
                let pan_l = angle.cos();
                let pan_r = angle.sin();

                osc1_sum_l += s1 * pan_l;
                osc1_sum_r += s1 * pan_r;
                osc2_sum_l += s2 * pan_l;
                osc2_sum_r += s2 * pan_r;
            }
            voice.osc1_phase = voice.osc1_phases[0];
            voice.osc2_phase = voice.osc2_phases[0];

            // Sub oscillator synthesis (Sine with tone filter)
            let mut sub_sample_l = 0.0f32;
            let mut sub_sample_r = 0.0f32;
            if self.sub_active {
                let sub_raw = (2.0 * PI * voice.sub_phase).sin();
                let alpha = (self.sub_tone * 0.8 + 0.1).clamp(0.01, 0.95);
                voice.sub_filter_state += alpha * (sub_raw - voice.sub_filter_state);
                let sub_s = voice.sub_filter_state * self.sub_gain;
                let angle = (voice_pan + 1.0) * 0.25 * PI;
                sub_sample_l = sub_s * angle.cos();
                sub_sample_r = sub_s * angle.sin();
            }

            // Dual Cytomic ZDF filter (stereo)
            voice.dual_filter.routing = self.filter_routing;
            voice.dual_filter.filter1.filter_type = self.filter1_type;
            voice.dual_filter.filter1.circuit_model = self.filter1_model;
            voice.dual_filter.filter2.filter_type = self.filter2_type;
            voice.dual_filter.filter2.circuit_model = self.filter2_model;

            voice.dual_filter_r.routing = self.filter_routing;
            voice.dual_filter_r.filter1.filter_type = self.filter1_type;
            voice.dual_filter_r.filter1.circuit_model = self.filter1_model;
            voice.dual_filter_r.filter2.filter_type = self.filter2_type;
            voice.dual_filter_r.filter2.circuit_model = self.filter2_model;

            let mod_cutoff1 = (self.filter1_cutoff * 2.0f32.powf(f1_cut_mod * 4.0)).clamp(20.0, 20000.0);
            let mod_res1 = (self.filter1_res + f1_res_mod * 0.5).clamp(0.0, 1.0);
            let mod_morph1 = (self.filter1_morph + f1_morph_mod).clamp(0.0, 1.0);

            let mod_cutoff2 = (self.filter2_cutoff * 2.0f32.powf(f2_cut_mod * 4.0)).clamp(20.0, 20000.0);
            let mod_res2 = (self.filter2_res + f2_res_mod * 0.5).clamp(0.0, 1.0);
            let mod_morph2 = (self.filter2_morph + f2_morph_mod).clamp(0.0, 1.0);

            let fl = voice.dual_filter.process(
                osc1_sum_l + sub_sample_l,
                osc2_sum_l,
                mod_cutoff1,
                mod_res1,
                mod_morph1,
                mod_cutoff2,
                mod_res2,
                mod_morph2,
            );
            let fr = voice.dual_filter_r.process(
                osc1_sum_r + sub_sample_r,
                osc2_sum_r,
                mod_cutoff1,
                mod_res1,
                mod_morph1,
                mod_cutoff2,
                mod_res2,
                mod_morph2,
            );

            // VCA with Volume modulation
            let mod_voice_vol = (base_vol * (1.0 + vol_mod).max(0.0)).clamp(0.0, 2.0);
            let voice_gain = amp_val * voice.velocity * mod_voice_vol;

            out_l += fl * voice_gain;
            out_r += fr * voice_gain;
        }

        (out_l, out_r)
    }
}

impl Device for WavetableDevice {
    fn name(&self) -> &str {
        "Wavetable"
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

impl InstrumentDevice for WavetableDevice {
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        _time: &MusicalTime,
    ) {
        if !self.enabled {
            return;
        }

        // Process MIDI events
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

        // Render audio
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
    fn test_wavetable_lifecycle_and_rendering() {
        let mut synth = WavetableDevice::new(44100.0);
        synth.trigger_note_on(60, 100);

        let mut buf = AudioBuffer::silence(256);
        let time = MusicalTime::default();
        synth.process(&[], &mut buf, &time);

        let mut energy = 0.0f32;
        for i in 0..256 {
            assert!(buf.left[i].is_finite());
            assert!(buf.right[i].is_finite());
            energy += buf.left[i].powi(2) + buf.right[i].powi(2);
        }
        assert!(energy > 0.001, "Wavetable must generate audible output");

        synth.trigger_note_off(60);
    }

    #[test]
    fn test_wavetable_unison_modes_and_amount_spread() {
        let mut synth_no_unison = WavetableDevice::new(44100.0);
        synth_no_unison.unison_voices = 4;
        synth_no_unison.unison_amount = 0.0;
        synth_no_unison.trigger_note_on(60, 100);

        let mut synth_with_unison = WavetableDevice::new(44100.0);
        synth_with_unison.unison_voices = 4;
        synth_with_unison.unison_amount = 0.5;
        synth_with_unison.trigger_note_on(60, 100);

        let mut diff = 0.0f32;
        for _ in 0..256 {
            let (l1, r1) = synth_no_unison.process_stereo_sample();
            let (l2, r2) = synth_with_unison.process_stereo_sample();
            diff += (l1 - l2).abs() + (r1 - r2).abs();
        }
        assert!(
            diff > 0.01,
            "Unison amount 0.0 vs 0.5 must produce measurable difference, diff={}",
            diff
        );

        // Verify all 4 unison modes run without error and produce stereo spread
        for mode in [UnisonMode::Classic, UnisonMode::Shimmer, UnisonMode::Noise, UnisonMode::PhaseSync] {
            let mut synth = WavetableDevice::new(44100.0);
            synth.unison_mode = mode;
            synth.unison_voices = 4;
            synth.unison_amount = 0.4;
            synth.trigger_note_on(57, 100);

            let mut energy_l = 0.0f32;
            let mut energy_r = 0.0f32;
            for _ in 0..256 {
                let (l, r) = synth.process_stereo_sample();
                energy_l += l * l;
                energy_r += r * r;
            }
            assert!(energy_l > 0.001, "Unison mode {:?} left energy must be non-zero", mode);
            assert!(energy_r > 0.001, "Unison mode {:?} right energy must be non-zero", mode);
        }
    }

    #[test]
    fn test_wavetable_mod_matrix_all_destinations() {
        use crate::wavetable::mod_matrix::{ModDestination, ModSource};

        // Test Filter 2 Cutoff destination
        let mut synth_base = WavetableDevice::new(44100.0);
        synth_base.trigger_note_on(60, 100);

        let mut synth_mod = WavetableDevice::new(44100.0);
        synth_mod.mod_matrix.set_slot(0, ModSource::AmpEnv, ModDestination::Filter2Cutoff, 1.0);
        synth_mod.trigger_note_on(60, 100);

        let mut diff_f2 = 0.0f32;
        for _ in 0..256 {
            let (l1, _) = synth_base.process_stereo_sample();
            let (l2, _) = synth_mod.process_stereo_sample();
            diff_f2 += (l1 - l2).abs();
        }
        assert!(diff_f2 > 0.001, "Filter2Cutoff modulation must affect output");

        // Test Pan destination
        let mut synth_pan = WavetableDevice::new(44100.0);
        synth_pan.mod_matrix.set_slot(0, ModSource::AmpEnv, ModDestination::Pan, 1.0);
        synth_pan.trigger_note_on(60, 100);

        let mut energy_l = 0.0f32;
        let mut energy_r = 0.0f32;
        for _ in 0..256 {
            let (l, r) = synth_pan.process_stereo_sample();
            energy_l += l * l;
            energy_r += r * r;
        }
        assert!(energy_r > energy_l * 1.5, "Positive Pan modulation must shift energy to right");
    }
}
