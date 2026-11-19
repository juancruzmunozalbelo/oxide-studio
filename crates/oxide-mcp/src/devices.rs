//! Device instantiation, live parameter handles, and device chain serialization for MCP.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use oxide_core::device::{
    AudioEffectDevice, Device, DeviceCategory, DeviceChain, InstrumentDevice, MidiEffectDevice,
};
use oxide_core::{AudioBuffer, MidiEvent, MusicalTime};
use oxide_drums::{DrumRack, SimplerDevice};
use oxide_dsp::drift::DriftDevice;
use oxide_dsp::meld::MeldDevice;
use oxide_dsp::operator::OperatorDevice;
use oxide_dsp::physical::{AnalogDevice, CollisionDevice, ElectricDevice, TensionDevice};
use oxide_dsp::wavetable::WavetableDevice;
use oxide_fx::midi_fx::{Arpeggiator, Chord, Scale, Velocity};
use oxide_fx::suite_fx::{
    BeatRepeat, Corpus, Echo, HybridReverb, Redux, SpectralResonator, Utility,
};
use oxide_fx::{AutoFilter, EQEight, GlueCompressor, MasterLimiter, MultibandDynamics, Roar};
use oxide_project::InsertEffectConfig;
use serde_json::{json, Map, Value};

pub enum CreatedDevice {
    Midi(Box<dyn MidiEffectDevice>),
    Instrument(Box<dyn InstrumentDevice>),
    Audio(Box<dyn AudioEffectDevice>),
}

/// Thread-safe shared device wrapper allowing `DeviceChain` and MCP parameter handlers
/// to mutate the exact same live device instance.
pub struct SharedDevice<T> {
    pub name: String,
    pub inner: Arc<Mutex<T>>,
}

impl<T: Device> SharedDevice<T> {
    pub fn new(inner: Arc<Mutex<T>>) -> Self {
        let name = inner.lock().unwrap_or_else(|e| e.into_inner()).name().to_string();
        Self { name, inner }
    }
}

impl<T: Device> Device for SharedDevice<T> {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> DeviceCategory {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).category()
    }

    fn is_enabled(&self) -> bool {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).is_enabled()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).set_enabled(enabled);
    }

    fn reset(&mut self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).reset();
    }

    fn latency_samples(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).latency_samples()
    }
}

impl<T: MidiEffectDevice> MidiEffectDevice for SharedDevice<T> {
    fn process_midi(
        &mut self,
        events_in: &[MidiEvent],
        events_out: &mut Vec<MidiEvent>,
        time: &MusicalTime,
    ) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .process_midi(events_in, events_out, time);
    }
}

impl<T: InstrumentDevice> InstrumentDevice for SharedDevice<T> {
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        time: &MusicalTime,
    ) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .process(midi_in, audio_out, time);
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).note_on(note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).note_off(note);
    }

    fn all_notes_off(&mut self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).all_notes_off();
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).set_parameter(param_id, value);
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).get_parameter(param_id)
    }
}

impl<T: AudioEffectDevice> AudioEffectDevice for SharedDevice<T> {
    fn process_audio(&mut self, buffer: &mut AudioBuffer, time: &MusicalTime) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).process_audio(buffer, time);
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).set_parameter(param_id, value);
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).get_parameter(param_id)
    }
}

/// Live mutable handle pointing to an instantiated device inside a track's `DeviceChain`.
#[derive(Clone)]
pub enum ManagedDeviceHandle {
    Wavetable(Arc<Mutex<WavetableDevice>>),
    Operator(Arc<Mutex<OperatorDevice>>),
    Drift(Arc<Mutex<DriftDevice>>),
    Meld(Arc<Mutex<MeldDevice>>),
    Analog(Arc<Mutex<AnalogDevice>>),
    Collision(Arc<Mutex<CollisionDevice>>),
    Tension(Arc<Mutex<TensionDevice>>),
    Electric(Arc<Mutex<ElectricDevice>>),
    Simpler(Arc<Mutex<SimplerDevice>>),
    DrumRack(Arc<Mutex<DrumRack>>),
    EqEight {
        dev: Arc<Mutex<EQEight>>,
        sample_rate: f32,
    },
    GlueCompressor(Arc<Mutex<GlueCompressor>>),
    Roar(Arc<Mutex<Roar>>),
    AutoFilter(Arc<Mutex<AutoFilter>>),
    MultibandDynamics(Arc<Mutex<MultibandDynamics>>),
    HybridReverb(Arc<Mutex<HybridReverb>>),
    SpectralResonator(Arc<Mutex<SpectralResonator>>),
    Echo(Arc<Mutex<Echo>>),
    BeatRepeat(Arc<Mutex<BeatRepeat>>),
    Corpus(Arc<Mutex<Corpus>>),
    Redux(Arc<Mutex<Redux>>),
    Utility(Arc<Mutex<Utility>>),
    MasterLimiter(Arc<Mutex<MasterLimiter>>),
    Arpeggiator(Arc<Mutex<Arpeggiator>>),
    Chord(Arc<Mutex<Chord>>),
    Scale(Arc<Mutex<Scale>>),
    Velocity(Arc<Mutex<Velocity>>),
}

/// Parallel handle structure mirroring a track's `DeviceChain` slots.
#[derive(Clone, Default)]
pub struct ManagedChainHandles {
    pub midi_effects: Vec<ManagedDeviceHandle>,
    pub instrument: Option<ManagedDeviceHandle>,
    pub audio_effects: Vec<ManagedDeviceHandle>,
}

impl ManagedChainHandles {
    pub fn device_count(&self) -> usize {
        self.midi_effects.len()
            + usize::from(self.instrument.is_some())
            + self.audio_effects.len()
    }

    /// Resolve a device handle across `[midi_effects..., instrument?, audio_effects...]`.
    pub fn get_at(&self, index: usize) -> Option<&ManagedDeviceHandle> {
        let m_len = self.midi_effects.len();
        if index < m_len {
            return self.midi_effects.get(index);
        }
        let mut cursor = m_len;
        if let Some(ref inst) = self.instrument {
            if index == cursor {
                return Some(inst);
            }
            cursor += 1;
        }
        self.audio_effects.get(index.saturating_sub(cursor))
    }

    pub fn add_handle(
        &mut self,
        category: DeviceCategory,
        handle: ManagedDeviceHandle,
        position: Option<usize>,
    ) -> usize {
        match category {
            DeviceCategory::MidiEffect => {
                if let Some(pos) = position {
                    let clamped = pos.min(self.midi_effects.len());
                    self.midi_effects.insert(clamped, handle);
                    clamped
                } else {
                    self.midi_effects.push(handle);
                    self.midi_effects.len() - 1
                }
            }
            DeviceCategory::Instrument => {
                self.instrument = Some(handle);
                self.midi_effects.len()
            }
            DeviceCategory::AudioEffect => {
                let prefix = self.midi_effects.len() + usize::from(self.instrument.is_some());
                let local_idx = if let Some(pos) = position {
                    let clamped = pos.min(self.audio_effects.len());
                    self.audio_effects.insert(clamped, handle);
                    clamped
                } else {
                    self.audio_effects.push(handle);
                    self.audio_effects.len() - 1
                };
                prefix + local_idx
            }
        }
    }

    /// Remove a handle by global chain index matching `DeviceChain` ordering.
    pub fn remove_at(&mut self, index: usize) -> Option<ManagedDeviceHandle> {
        let m_len = self.midi_effects.len();
        if index < m_len {
            return Some(self.midi_effects.remove(index));
        }
        let mut cursor = m_len;
        if self.instrument.is_some() {
            if index == cursor {
                return self.instrument.take();
            }
            cursor += 1;
        }
        let a_idx = index.saturating_sub(cursor);
        if a_idx < self.audio_effects.len() {
            Some(self.audio_effects.remove(a_idx))
        } else {
            None
        }
    }

    pub fn reorder_audio(&mut self, from: usize, to: usize) -> bool {
        if from < self.audio_effects.len() && to < self.audio_effects.len() {
            let h = self.audio_effects.remove(from);
            self.audio_effects.insert(to, h);
            true
        } else {
            false
        }
    }

    pub fn reorder_midi(&mut self, from: usize, to: usize) -> bool {
        if from < self.midi_effects.len() && to < self.midi_effects.len() {
            let h = self.midi_effects.remove(from);
            self.midi_effects.insert(to, h);
            true
        } else {
            false
        }
    }
}

fn normalize_param_key(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

impl ManagedDeviceHandle {
    pub fn category(&self) -> DeviceCategory {
        match self {
            Self::Wavetable(_)
            | Self::Operator(_)
            | Self::Drift(_)
            | Self::Meld(_)
            | Self::Analog(_)
            | Self::Collision(_)
            | Self::Tension(_)
            | Self::Electric(_)
            | Self::Simpler(_)
            | Self::DrumRack(_) => DeviceCategory::Instrument,
            Self::Arpeggiator(_) | Self::Chord(_) | Self::Scale(_) | Self::Velocity(_) => {
                DeviceCategory::MidiEffect
            }
            _ => DeviceCategory::AudioEffect,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Wavetable(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Operator(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Drift(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Meld(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Analog(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Collision(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Tension(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Electric(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Simpler(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::DrumRack(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::EqEight { dev, .. } => dev.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::GlueCompressor(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Roar(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::AutoFilter(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::MultibandDynamics(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::HybridReverb(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::SpectralResonator(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Echo(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::BeatRepeat(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Corpus(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Redux(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Utility(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::MasterLimiter(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Arpeggiator(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Chord(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Scale(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
            Self::Velocity(d) => d.lock().unwrap_or_else(|e| e.into_inner()).name().to_string(),
        }
    }

    /// Return all canonical parameter `(name, value)` pairs in deterministic order.
    pub fn get_params(&self) -> Vec<(String, f32)> {
        match self {
            Self::Wavetable(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("filter1_cutoff".into(), d.filter1_cutoff),
                    ("filter1_res".into(), d.filter1_res),
                    ("filter1_morph".into(), d.filter1_morph),
                    ("filter1_drive".into(), d.filter1_drive),
                    ("filter2_cutoff".into(), d.filter2_cutoff),
                    ("filter2_res".into(), d.filter2_res),
                    ("osc1_pos".into(), d.osc1_pos),
                    ("osc1_gain".into(), d.osc1_gain),
                    ("osc2_pos".into(), d.osc2_pos),
                    ("osc2_gain".into(), d.osc2_gain),
                    ("sub_gain".into(), d.sub_gain),
                    ("unison_amount".into(), d.unison_amount),
                    ("master_volume".into(), d.master_volume),
                    ("master_pan".into(), d.master_pan),
                ]
            }
            Self::Operator(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("filter_cutoff".into(), d.filter_cutoff),
                    ("filter_resonance".into(), d.filter_resonance),
                    ("filter_drive".into(), d.filter_drive),
                    ("feedback_d".into(), d.feedback_d),
                    ("op_a_level".into(), d.ops[0].level),
                    ("op_a_coarse".into(), d.ops[0].coarse),
                    ("op_b_level".into(), d.ops[1].level),
                    ("op_b_coarse".into(), d.ops[1].coarse),
                    ("op_c_level".into(), d.ops[2].level),
                    ("op_d_level".into(), d.ops[3].level),
                    ("master_volume".into(), d.master_volume),
                    ("master_pan".into(), d.master_pan),
                ]
            }
            Self::Drift(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("lp_cutoff".into(), d.lp_cutoff),
                    ("lp_resonance".into(), d.lp_resonance),
                    ("hp_cutoff".into(), d.hp_cutoff),
                    ("osc1_shape".into(), d.osc1_shape),
                    ("osc1_gain".into(), d.osc1_gain),
                    ("osc2_detune_cents".into(), d.osc2_detune_cents),
                    ("osc2_gain".into(), d.osc2_gain),
                    ("osc2_cross_mod".into(), d.osc2_cross_mod),
                    ("drift_amount".into(), d.drift_amount),
                    ("noise_gain".into(), d.noise_gain),
                    ("env2_to_cutoff".into(), d.env2_to_cutoff),
                    ("master_volume".into(), d.master_volume),
                ]
            }
            Self::Meld(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("filter_a_cutoff".into(), d.filter_a_cutoff),
                    ("filter_a_res".into(), d.filter_a_res),
                    ("filter_b_cutoff".into(), d.filter_b_cutoff),
                    ("filter_b_res".into(), d.filter_b_res),
                    ("engine_a_param1".into(), d.engine_a_param1),
                    ("engine_a_param2".into(), d.engine_a_param2),
                    ("engine_a_gain".into(), d.engine_a_gain),
                    ("engine_b_param1".into(), d.engine_b_param1),
                    ("engine_b_param2".into(), d.engine_b_param2),
                    ("engine_b_gain".into(), d.engine_b_gain),
                    ("cross_mod_amount".into(), d.cross_mod_amount),
                    ("plate_decay".into(), d.plate_decay),
                    ("plate_damping".into(), d.plate_damping),
                    ("master_volume".into(), d.master_volume),
                ]
            }
            Self::Analog(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("filter1_cutoff".into(), d.filter1_cutoff),
                    ("filter1_res".into(), d.filter1_res),
                    ("filter1_env_amt".into(), d.filter1_env_amt),
                    ("filter2_cutoff".into(), d.filter2_cutoff),
                    ("filter2_res".into(), d.filter2_res),
                    ("osc1_pwm".into(), d.osc1_pwm),
                    ("osc1_gain".into(), d.osc1_gain),
                    ("osc2_detune_cents".into(), d.osc2_detune_cents),
                    ("osc2_pwm".into(), d.osc2_pwm),
                    ("osc2_gain".into(), d.osc2_gain),
                    ("sub_gain".into(), d.sub_gain),
                    ("noise_gain".into(), d.noise_gain),
                    ("master_volume".into(), d.master_volume),
                ]
            }
            Self::Collision(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("mallet_stiffness".into(), d.mallet_stiffness),
                    ("mallet_noise".into(), d.mallet_noise),
                    ("res1_decay".into(), d.res1_decay),
                    ("res1_material".into(), d.res1_material),
                    ("res1_gain".into(), d.res1_gain),
                    ("res2_decay".into(), d.res2_decay),
                    ("res2_material".into(), d.res2_material),
                    ("res2_gain".into(), d.res2_gain),
                    ("coupling_gain".into(), d.coupling_gain),
                    ("master_volume".into(), d.master_volume),
                ]
            }
            Self::Tension(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("string_damping".into(), d.string_damping),
                    ("string_decay_loss".into(), d.string_decay_loss),
                    ("string_stiffness".into(), d.string_stiffness),
                    ("pickup_position".into(), d.pickup_position),
                    ("body_size".into(), d.body_size),
                    ("body_gain".into(), d.body_gain),
                    ("master_volume".into(), d.master_volume),
                ]
            }
            Self::Electric(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("hammer_hardness".into(), d.hammer_hardness),
                    ("tine_decay".into(), d.tine_decay),
                    ("bell_inharmonicity".into(), d.bell_inharmonicity),
                    ("bell_gain".into(), d.bell_gain),
                    ("tone_bar_decay".into(), d.tone_bar_decay),
                    ("pickup_distance".into(), d.pickup_distance),
                    ("pickup_symmetry".into(), d.pickup_symmetry),
                    ("tremolo_rate".into(), d.tremolo_rate),
                    ("tremolo_depth".into(), d.tremolo_depth),
                    ("master_volume".into(), d.master_volume),
                ]
            }
            Self::Simpler(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("filter_cutoff".into(), d.filter_cutoff),
                    ("filter_res".into(), d.filter_res),
                    ("filter_env_amt".into(), d.filter_env_amt),
                    ("start_pos".into(), d.start_pos),
                    ("length_pos".into(), d.length_pos),
                    ("fade_in_ms".into(), d.fade_in_ms),
                    ("fade_out_ms".into(), d.fade_out_ms),
                    ("transpose_semitones".into(), d.transpose_semitones as f32),
                    ("detune_cents".into(), d.detune_cents),
                    ("master_volume".into(), d.master_volume),
                    ("master_pan".into(), d.master_pan),
                ]
            }
            Self::DrumRack(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                let mut out = vec![
                    ("return_a_level".into(), d.return_a_level),
                    ("return_b_level".into(), d.return_b_level),
                ];
                for i in 0..d.pads.len().min(16) {
                    out.push((format!("pad{}_volume", i), d.pads[i].volume));
                    out.push((format!("pad{}_pan", i), d.pads[i].pan));
                    out.push((format!("pad{}_pitch", i), d.pads[i].pitch_semitones));
                    out.push((format!("pad{}_decay", i), d.pads[i].decay_seconds));
                }
                out
            }
            Self::EqEight { dev, .. } => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let mut out = Vec::with_capacity(32);
                for (i, b) in d.bands.iter().enumerate() {
                    let n = i + 1;
                    out.push((format!("band{}_freq_hz", n), b.freq_hz));
                    out.push((format!("band{}_gain_db", n), b.gain_db));
                    out.push((format!("band{}_q", n), b.q));
                    out.push((format!("band{}_enabled", n), if b.enabled { 1.0 } else { 0.0 }));
                }
                out
            }
            Self::GlueCompressor(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("threshold_db".into(), d.threshold_db),
                    ("ratio".into(), d.ratio),
                    ("attack_ms".into(), d.attack_ms),
                    ("release_s".into(), d.release_s),
                    ("makeup_gain_db".into(), d.makeup_gain_db),
                    ("range_db".into(), d.range_db),
                    ("dry_wet".into(), d.dry_wet),
                    ("soft_clip".into(), if d.soft_clip { 1.0 } else { 0.0 }),
                ]
            }
            Self::Roar(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("drive_db".into(), d.stage1.drive_db),
                    ("bias".into(), d.stage1.bias),
                    ("tone".into(), d.stage1.tone),
                    ("stage2_drive_db".into(), d.stage2.drive_db),
                    ("stage2_tone".into(), d.stage2.tone),
                    ("feedback_amount".into(), d.feedback_amount),
                    ("feedback_delay_ms".into(), d.feedback_delay_ms),
                    ("dry_wet".into(), d.dry_wet),
                    ("output_gain_db".into(), d.output_gain_db),
                ]
            }
            Self::AutoFilter(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("cutoff_hz".into(), d.cutoff_hz),
                    ("resonance".into(), d.resonance),
                    ("drive".into(), d.drive),
                    ("morph".into(), d.morph),
                    ("env_amount".into(), d.env_amount),
                    ("lfo_rate_hz".into(), d.lfo_rate_hz),
                    ("lfo_amount".into(), d.lfo_amount),
                    ("dry_wet".into(), d.dry_wet),
                ]
            }
            Self::MultibandDynamics(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("amount".into(), d.amount),
                    ("time_macro".into(), d.time_macro),
                    ("input_gain_db".into(), d.input_gain_db),
                    ("output_gain_db".into(), d.output_gain_db),
                    ("low_gain_db".into(), d.low_band.gain_db),
                    ("mid_gain_db".into(), d.mid_band.gain_db),
                    ("high_gain_db".into(), d.high_band.gain_db),
                ]
            }
            Self::HybridReverb(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("decay_s".into(), d.decay_s),
                    ("size".into(), d.size),
                    ("damping".into(), d.damping),
                    ("predelay_ms".into(), d.predelay_ms),
                    ("conv_algo_blend".into(), d.conv_algo_blend),
                    ("dry_wet".into(), d.dry_wet),
                ]
            }
            Self::SpectralResonator(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("root_hz".into(), d.root_hz),
                    ("decay_s".into(), d.decay_s),
                    ("unison_spread".into(), d.unison_spread),
                    ("dry_wet".into(), d.dry_wet),
                ]
            }
            Self::Echo(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("delay_time_l_ms".into(), d.delay_time_l_ms),
                    ("delay_time_r_ms".into(), d.delay_time_r_ms),
                    ("feedback".into(), d.feedback),
                    ("cross_feed".into(), d.cross_feed),
                    ("wow".into(), d.wow),
                    ("flutter".into(), d.flutter),
                    ("ducking_db".into(), d.ducking_db),
                    ("dry_wet".into(), d.dry_wet),
                ]
            }
            Self::BeatRepeat(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("interval_beats".into(), d.interval_beats as f32),
                    ("grid_beats".into(), d.grid_beats as f32),
                    ("chance".into(), d.chance),
                    ("gate".into(), d.gate),
                    ("pitch_decay".into(), d.pitch_decay),
                    ("dry_wet".into(), d.dry_wet),
                ]
            }
            Self::Corpus(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("tune_hz".into(), d.tune_hz),
                    ("decay_s".into(), d.decay_s),
                    ("material".into(), d.material),
                    ("dry_wet".into(), d.dry_wet),
                ]
            }
            Self::Redux(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("downsample".into(), d.downsample as f32),
                    ("bit_depth".into(), d.bit_depth as f32),
                    ("jitter".into(), d.jitter),
                    ("dry_wet".into(), d.dry_wet),
                ]
            }
            Self::Utility(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("gain_db".into(), d.gain_db),
                    ("pan".into(), d.pan),
                    ("width".into(), d.width),
                    ("bass_mono_freq_hz".into(), d.bass_mono_freq_hz),
                    ("bass_mono".into(), if d.bass_mono { 1.0 } else { 0.0 }),
                    ("mute".into(), if d.mute { 1.0 } else { 0.0 }),
                ]
            }
            Self::MasterLimiter(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                let db = 20.0 * d.ceiling.max(1e-5).log10();
                vec![("ceiling_db".into(), db)]
            }
            Self::Arpeggiator(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("rate_beats".into(), d.rate_beats as f32),
                    ("gate".into(), d.gate),
                    ("octaves".into(), d.octaves as f32),
                    ("velocity_decay".into(), d.velocity_decay),
                ]
            }
            Self::Chord(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                d.shifts
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (format!("shift{}", i + 1), *s as f32))
                    .collect()
            }
            Self::Scale(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("base_note".into(), d.base_note as f32),
                    ("transpose".into(), d.transpose as f32),
                ]
            }
            Self::Velocity(d) => {
                let d = d.lock().unwrap_or_else(|e| e.into_inner());
                vec![
                    ("drive".into(), d.drive),
                    ("compand".into(), d.compand),
                    ("random".into(), d.random as f32),
                    ("min_velocity".into(), d.min_velocity as f32),
                    ("max_velocity".into(), d.max_velocity as f32),
                ]
            }
        }
    }

    /// Set any parameter on this live device handle by canonical name, alias, or numeric index.
    pub fn set_parameter(&self, key: &str, value: f32) -> Result<(String, f32), String> {
        if !value.is_finite() {
            return Err(format!(
                "Parameter '{}' on device '{}' requires a finite value, got {}",
                key,
                self.name(),
                value
            ));
        }
        let params = self.get_params();
        let resolved_key = if let Ok(idx) = key.trim().parse::<usize>() {
            params
                .get(idx)
                .map(|(name, _)| name.clone())
                .ok_or_else(|| {
                    format!(
                        "Parameter index {} out of range for device '{}' (count: {})",
                        idx,
                        self.name(),
                        params.len()
                    )
                })?
        } else {
            normalize_param_key(key)
        };

        let k = resolved_key.as_str();
        match self {
            Self::Wavetable(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "filter1_cutoff" | "cutoff" | "cutoff_hz" => {
                        d.filter1_cutoff = value.clamp(20.0, 20000.0);
                        d.filter1_cutoff
                    }
                    "filter1_res" | "resonance" | "res" => {
                        d.filter1_res = value.clamp(0.0, 1.0);
                        d.filter1_res
                    }
                    "filter1_morph" => {
                        d.filter1_morph = value.clamp(0.0, 1.0);
                        d.filter1_morph
                    }
                    "filter1_drive" => {
                        d.filter1_drive = value.clamp(0.0, 4.0);
                        d.filter1_drive
                    }
                    "filter2_cutoff" => {
                        d.filter2_cutoff = value.clamp(20.0, 20000.0);
                        d.filter2_cutoff
                    }
                    "filter2_res" => {
                        d.filter2_res = value.clamp(0.0, 1.0);
                        d.filter2_res
                    }
                    "osc1_pos" => {
                        d.osc1_pos = value.clamp(0.0, 1.0);
                        d.osc1_pos
                    }
                    "osc1_gain" => {
                        d.osc1_gain = value.clamp(0.0, 1.0);
                        d.osc1_gain
                    }
                    "osc2_pos" => {
                        d.osc2_pos = value.clamp(0.0, 1.0);
                        d.osc2_pos
                    }
                    "osc2_gain" => {
                        d.osc2_gain = value.clamp(0.0, 1.0);
                        d.osc2_gain
                    }
                    "sub_gain" => {
                        d.sub_gain = value.clamp(0.0, 1.0);
                        d.sub_gain
                    }
                    "unison_amount" => {
                        d.unison_amount = value.clamp(0.0, 1.0);
                        d.unison_amount
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    "master_pan" | "pan" => {
                        d.master_pan = value.clamp(-1.0, 1.0);
                        d.master_pan
                    }
                    _ => return Err(format!("Unknown Wavetable parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Operator(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "filter_cutoff" | "cutoff" | "cutoff_hz" => {
                        d.filter_cutoff = value.clamp(20.0, 20000.0);
                        d.filter_cutoff
                    }
                    "filter_resonance" | "resonance" | "res" => {
                        d.filter_resonance = value.clamp(0.0, 1.0);
                        d.filter_resonance
                    }
                    "filter_drive" | "drive" => {
                        d.filter_drive = value.clamp(0.0, 4.0);
                        d.filter_drive
                    }
                    "feedback_d" | "feedback" => {
                        d.feedback_d = value.clamp(0.0, 1.0);
                        d.feedback_d
                    }
                    "op_a_level" => {
                        d.ops[0].level = value.clamp(0.0, 2.0);
                        d.ops[0].level
                    }
                    "op_a_coarse" => {
                        d.ops[0].coarse = value.clamp(0.5, 32.0);
                        d.ops[0].coarse
                    }
                    "op_b_level" => {
                        d.ops[1].level = value.clamp(0.0, 2.0);
                        d.ops[1].level
                    }
                    "op_b_coarse" => {
                        d.ops[1].coarse = value.clamp(0.5, 32.0);
                        d.ops[1].coarse
                    }
                    "op_c_level" => {
                        d.ops[2].level = value.clamp(0.0, 2.0);
                        d.ops[2].level
                    }
                    "op_d_level" => {
                        d.ops[3].level = value.clamp(0.0, 2.0);
                        d.ops[3].level
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    "master_pan" | "pan" => {
                        d.master_pan = value.clamp(-1.0, 1.0);
                        d.master_pan
                    }
                    _ => return Err(format!("Unknown Operator parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Drift(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "lp_cutoff" | "cutoff" | "cutoff_hz" => {
                        d.lp_cutoff = value.clamp(20.0, 20000.0);
                        d.lp_cutoff
                    }
                    "lp_resonance" | "resonance" | "res" => {
                        d.lp_resonance = value.clamp(0.0, 1.0);
                        d.lp_resonance
                    }
                    "hp_cutoff" => {
                        d.hp_cutoff = value.clamp(20.0, 20000.0);
                        d.hp_cutoff
                    }
                    "osc1_shape" => {
                        d.osc1_shape = value.clamp(0.0, 1.0);
                        d.osc1_shape
                    }
                    "osc1_gain" => {
                        d.osc1_gain = value.clamp(0.0, 1.0);
                        d.osc1_gain
                    }
                    "osc2_detune_cents" => {
                        d.osc2_detune_cents = value.clamp(-50.0, 50.0);
                        d.osc2_detune_cents
                    }
                    "osc2_gain" => {
                        d.osc2_gain = value.clamp(0.0, 1.0);
                        d.osc2_gain
                    }
                    "osc2_cross_mod" => {
                        d.osc2_cross_mod = value.clamp(0.0, 1.0);
                        d.osc2_cross_mod
                    }
                    "drift_amount" | "drift" => {
                        d.drift_amount = value.clamp(0.0, 1.0);
                        d.drift_amount
                    }
                    "noise_gain" => {
                        d.noise_gain = value.clamp(0.0, 1.0);
                        d.noise_gain
                    }
                    "env2_to_cutoff" => {
                        d.env2_to_cutoff = value.clamp(-1.0, 1.0);
                        d.env2_to_cutoff
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    _ => return Err(format!("Unknown Drift parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Meld(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "filter_a_cutoff" | "cutoff" | "cutoff_hz" => {
                        d.filter_a_cutoff = value.clamp(20.0, 20000.0);
                        d.filter_a_cutoff
                    }
                    "filter_a_res" | "resonance" => {
                        d.filter_a_res = value.clamp(0.0, 1.0);
                        d.filter_a_res
                    }
                    "filter_b_cutoff" => {
                        d.filter_b_cutoff = value.clamp(20.0, 20000.0);
                        d.filter_b_cutoff
                    }
                    "filter_b_res" => {
                        d.filter_b_res = value.clamp(0.0, 1.0);
                        d.filter_b_res
                    }
                    "engine_a_param1" => {
                        d.engine_a_param1 = value.clamp(0.0, 1.0);
                        d.engine_a_param1
                    }
                    "engine_a_param2" => {
                        d.engine_a_param2 = value.clamp(0.0, 1.0);
                        d.engine_a_param2
                    }
                    "engine_a_gain" => {
                        d.engine_a_gain = value.clamp(0.0, 1.0);
                        d.engine_a_gain
                    }
                    "engine_b_param1" => {
                        d.engine_b_param1 = value.clamp(0.0, 1.0);
                        d.engine_b_param1
                    }
                    "engine_b_param2" => {
                        d.engine_b_param2 = value.clamp(0.0, 1.0);
                        d.engine_b_param2
                    }
                    "engine_b_gain" => {
                        d.engine_b_gain = value.clamp(0.0, 1.0);
                        d.engine_b_gain
                    }
                    "cross_mod_amount" => {
                        d.cross_mod_amount = value.clamp(0.0, 1.0);
                        d.cross_mod_amount
                    }
                    "plate_decay" => {
                        d.plate_decay = value.clamp(0.05, 10.0);
                        d.plate_decay
                    }
                    "plate_damping" => {
                        d.plate_damping = value.clamp(0.0, 1.0);
                        d.plate_damping
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    _ => return Err(format!("Unknown Meld parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Analog(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "filter1_cutoff" | "cutoff" | "cutoff_hz" => {
                        d.filter1_cutoff = value.clamp(20.0, 20000.0);
                        d.filter1_cutoff
                    }
                    "filter1_res" | "resonance" => {
                        d.filter1_res = value.clamp(0.0, 1.0);
                        d.filter1_res
                    }
                    "filter1_env_amt" => {
                        d.filter1_env_amt = value.clamp(-1.0, 1.0);
                        d.filter1_env_amt
                    }
                    "filter2_cutoff" => {
                        d.filter2_cutoff = value.clamp(20.0, 20000.0);
                        d.filter2_cutoff
                    }
                    "filter2_res" => {
                        d.filter2_res = value.clamp(0.0, 1.0);
                        d.filter2_res
                    }
                    "osc1_pwm" => {
                        d.osc1_pwm = value.clamp(0.05, 0.95);
                        d.osc1_pwm
                    }
                    "osc1_gain" => {
                        d.osc1_gain = value.clamp(0.0, 1.0);
                        d.osc1_gain
                    }
                    "osc2_detune_cents" => {
                        d.osc2_detune_cents = value.clamp(-50.0, 50.0);
                        d.osc2_detune_cents
                    }
                    "osc2_pwm" => {
                        d.osc2_pwm = value.clamp(0.05, 0.95);
                        d.osc2_pwm
                    }
                    "osc2_gain" => {
                        d.osc2_gain = value.clamp(0.0, 1.0);
                        d.osc2_gain
                    }
                    "sub_gain" => {
                        d.sub_gain = value.clamp(0.0, 1.0);
                        d.sub_gain
                    }
                    "noise_gain" => {
                        d.noise_gain = value.clamp(0.0, 1.0);
                        d.noise_gain
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    _ => return Err(format!("Unknown Analog parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Collision(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "mallet_stiffness" => {
                        d.mallet_stiffness = value.clamp(0.0, 1.0);
                        d.mallet_stiffness
                    }
                    "mallet_noise" => {
                        d.mallet_noise = value.clamp(0.0, 1.0);
                        d.mallet_noise
                    }
                    "res1_decay" => {
                        d.res1_decay = value.clamp(0.05, 10.0);
                        d.res1_decay
                    }
                    "res1_material" => {
                        d.res1_material = value.clamp(0.0, 1.0);
                        d.res1_material
                    }
                    "res1_gain" => {
                        d.res1_gain = value.clamp(0.0, 1.0);
                        d.res1_gain
                    }
                    "res2_decay" => {
                        d.res2_decay = value.clamp(0.05, 10.0);
                        d.res2_decay
                    }
                    "res2_material" => {
                        d.res2_material = value.clamp(0.0, 1.0);
                        d.res2_material
                    }
                    "res2_gain" => {
                        d.res2_gain = value.clamp(0.0, 1.0);
                        d.res2_gain
                    }
                    "coupling_gain" => {
                        d.coupling_gain = value.clamp(0.0, 1.0);
                        d.coupling_gain
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    _ => return Err(format!("Unknown Collision parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Tension(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "string_damping" => {
                        d.string_damping = value.clamp(0.0, 1.0);
                        d.string_damping
                    }
                    "string_decay_loss" => {
                        d.string_decay_loss = value.clamp(0.8, 0.999);
                        d.string_decay_loss
                    }
                    "string_stiffness" => {
                        d.string_stiffness = value.clamp(0.0, 0.8);
                        d.string_stiffness
                    }
                    "pickup_position" => {
                        d.pickup_position = value.clamp(0.0, 1.0);
                        d.pickup_position
                    }
                    "body_size" => {
                        d.body_size = value.clamp(0.1, 4.0);
                        d.body_size
                    }
                    "body_gain" => {
                        d.body_gain = value.clamp(0.0, 2.0);
                        d.body_gain
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    _ => return Err(format!("Unknown Tension parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Electric(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "hammer_hardness" => {
                        d.hammer_hardness = value.clamp(0.0, 1.0);
                        d.hammer_hardness
                    }
                    "tine_decay" => {
                        d.tine_decay = value.clamp(0.1, 8.0);
                        d.tine_decay
                    }
                    "bell_inharmonicity" => {
                        d.bell_inharmonicity = value.clamp(1.0, 12.0);
                        d.bell_inharmonicity
                    }
                    "bell_gain" => {
                        d.bell_gain = value.clamp(0.0, 2.0);
                        d.bell_gain
                    }
                    "tone_bar_decay" => {
                        d.tone_bar_decay = value.clamp(0.1, 8.0);
                        d.tone_bar_decay
                    }
                    "pickup_distance" => {
                        d.pickup_distance = value.clamp(0.05, 1.0);
                        d.pickup_distance
                    }
                    "pickup_symmetry" => {
                        d.pickup_symmetry = value.clamp(-1.0, 1.0);
                        d.pickup_symmetry
                    }
                    "tremolo_rate" => {
                        d.tremolo_rate = value.clamp(0.1, 20.0);
                        d.tremolo_rate
                    }
                    "tremolo_depth" => {
                        d.tremolo_depth = value.clamp(0.0, 1.0);
                        d.tremolo_depth
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    _ => return Err(format!("Unknown Electric parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Simpler(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "filter_cutoff" | "cutoff" | "cutoff_hz" => {
                        d.filter_cutoff = value.clamp(20.0, 20000.0);
                        d.filter_cutoff
                    }
                    "filter_res" | "resonance" => {
                        d.filter_res = value.clamp(0.0, 1.0);
                        d.filter_res
                    }
                    "filter_env_amt" => {
                        d.filter_env_amt = value.clamp(-1.0, 1.0);
                        d.filter_env_amt
                    }
                    "start_pos" => {
                        d.start_pos = value.clamp(0.0, 1.0);
                        d.start_pos
                    }
                    "length_pos" => {
                        d.length_pos = value.clamp(0.01, 1.0);
                        d.length_pos
                    }
                    "fade_in_ms" => {
                        d.fade_in_ms = value.clamp(0.0, 1000.0);
                        d.fade_in_ms
                    }
                    "fade_out_ms" => {
                        d.fade_out_ms = value.clamp(0.0, 1000.0);
                        d.fade_out_ms
                    }
                    "transpose_semitones" | "transpose" => {
                        d.transpose_semitones = value.round().clamp(-48.0, 48.0) as i8;
                        d.transpose_semitones as f32
                    }
                    "detune_cents" => {
                        d.detune_cents = value.clamp(-50.0, 50.0);
                        d.detune_cents
                    }
                    "master_volume" | "volume" => {
                        d.master_volume = value.clamp(0.0, 2.0);
                        d.master_volume
                    }
                    "master_pan" | "pan" => {
                        d.master_pan = value.clamp(-1.0, 1.0);
                        d.master_pan
                    }
                    _ => return Err(format!("Unknown Simpler parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::DrumRack(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "return_a_level" => {
                        d.return_a_level = value.clamp(0.0, 2.0);
                        d.return_a_level
                    }
                    "return_b_level" => {
                        d.return_b_level = value.clamp(0.0, 2.0);
                        d.return_b_level
                    }
                    _ => {
                        let mut matched = None;
                        if let Some(rest) = k.strip_prefix("pad") {
                            if let Some((idx_str, field)) = rest.split_once('_') {
                                if let Ok(pad_idx) = idx_str.parse::<usize>() {
                                    if pad_idx < d.pads.len().min(16) {
                                        let pad = &mut d.pads[pad_idx];
                                        matched = match field {
                                            "volume" => {
                                                pad.volume = value.clamp(0.0, 2.0);
                                                Some(pad.volume)
                                            }
                                            "pan" => {
                                                pad.pan = value.clamp(-1.0, 1.0);
                                                Some(pad.pan)
                                            }
                                            "pitch" => {
                                                pad.pitch_semitones = value.clamp(-24.0, 24.0);
                                                Some(pad.pitch_semitones)
                                            }
                                            "decay" => {
                                                pad.decay_seconds = value.clamp(0.02, 5.0);
                                                Some(pad.decay_seconds)
                                            }
                                            _ => None,
                                        };
                                    }
                                }
                            }
                        }
                        match matched {
                            Some(v) => v,
                            None => return Err(format!("Unknown DrumRack parameter '{}'", key)),
                        }
                    }
                };
                Ok((k.to_string(), applied))
            }
            Self::EqEight { dev, sample_rate } => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let mut applied = None;
                if let Some(rest) = k.strip_prefix("band") {
                    if let Some((num_str, field)) = rest.split_once('_') {
                        if let Ok(band_num) = num_str.parse::<usize>() {
                            if (1..=8).contains(&band_num) {
                                let b = &mut d.bands[band_num - 1];
                                match field {
                                    "freq_hz" | "freq" => {
                                        b.freq_hz = value.clamp(10.0, 20000.0);
                                        b.recompute(*sample_rate);
                                        applied = Some(b.freq_hz);
                                    }
                                    "gain_db" | "gain" => {
                                        b.gain_db = value.clamp(-15.0, 15.0);
                                        b.recompute(*sample_rate);
                                        applied = Some(b.gain_db);
                                    }
                                    "q" => {
                                        b.q = value.clamp(0.1, 18.0);
                                        b.recompute(*sample_rate);
                                        applied = Some(b.q);
                                    }
                                    "enabled" => {
                                        b.enabled = value >= 0.5;
                                        applied = Some(if b.enabled { 1.0 } else { 0.0 });
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                match applied {
                    Some(v) => Ok((k.to_string(), v)),
                    None => Err(format!("Unknown EQ Eight parameter '{}'", key)),
                }
            }
            Self::GlueCompressor(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "threshold_db" | "threshold" => {
                        d.threshold_db = value.clamp(-40.0, 0.0);
                        d.threshold_db
                    }
                    "ratio" => {
                        d.ratio = value.clamp(1.0, 20.0);
                        d.ratio
                    }
                    "attack_ms" | "attack" => {
                        d.attack_ms = value.clamp(0.01, 100.0);
                        d.attack_ms
                    }
                    "release_s" | "release" => {
                        d.release_s = value.clamp(-1.0, 5.0);
                        d.release_s
                    }
                    "makeup_gain_db" | "makeup" | "makeup_db" => {
                        d.makeup_gain_db = value.clamp(-12.0, 24.0);
                        d.makeup_gain_db
                    }
                    "range_db" => {
                        d.range_db = value.clamp(0.0, 70.0);
                        d.range_db
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    "soft_clip" => {
                        d.soft_clip = value >= 0.5;
                        if d.soft_clip { 1.0 } else { 0.0 }
                    }
                    _ => return Err(format!("Unknown Glue Compressor parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Roar(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "drive_db" | "drive" => {
                        d.stage1.drive_db = value.clamp(0.0, 36.0);
                        d.stage1.drive_db
                    }
                    "bias" => {
                        d.stage1.bias = value.clamp(-1.0, 1.0);
                        d.stage1.bias
                    }
                    "tone" => {
                        d.stage1.tone = value.clamp(-1.0, 1.0);
                        d.stage1.tone
                    }
                    "stage2_drive_db" => {
                        d.stage2.drive_db = value.clamp(0.0, 36.0);
                        d.stage2.drive_db
                    }
                    "stage2_tone" => {
                        d.stage2.tone = value.clamp(-1.0, 1.0);
                        d.stage2.tone
                    }
                    "feedback_amount" | "feedback" => {
                        d.feedback_amount = value.clamp(0.0, 1.0);
                        d.feedback_amount
                    }
                    "feedback_delay_ms" => {
                        d.feedback_delay_ms = value.clamp(1.0, 500.0);
                        d.feedback_delay_ms
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    "output_gain_db" | "output_db" => {
                        d.output_gain_db = value.clamp(-36.0, 24.0);
                        d.output_gain_db
                    }
                    _ => return Err(format!("Unknown Roar parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::AutoFilter(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "cutoff_hz" | "cutoff" | "filter_cutoff" | "frequency" => {
                        d.cutoff_hz = value.clamp(20.0, 20000.0);
                        d.cutoff_hz
                    }
                    "resonance" | "res" => {
                        d.resonance = value.clamp(0.0, 1.0);
                        d.resonance
                    }
                    "drive" => {
                        d.drive = value.clamp(0.5, 4.0);
                        d.drive
                    }
                    "morph" => {
                        d.morph = value.clamp(0.0, 1.0);
                        d.morph
                    }
                    "env_amount" => {
                        d.env_amount = value.clamp(-5.0, 5.0);
                        d.env_amount
                    }
                    "lfo_rate_hz" => {
                        d.lfo_rate_hz = value.clamp(0.05, 50.0);
                        d.lfo_rate_hz
                    }
                    "lfo_amount" => {
                        d.lfo_amount = value.clamp(0.0, 5.0);
                        d.lfo_amount
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    _ => return Err(format!("Unknown Auto Filter parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::MultibandDynamics(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "amount" | "dry_wet" | "wet" | "mix" => {
                        d.amount = value.clamp(0.0, 1.0);
                        d.amount
                    }
                    "time_macro" | "time" => {
                        d.time_macro = value.clamp(0.1, 10.0);
                        d.time_macro
                    }
                    "input_gain_db" => {
                        d.input_gain_db = value.clamp(-24.0, 24.0);
                        d.input_gain_db
                    }
                    "output_gain_db" => {
                        d.output_gain_db = value.clamp(-24.0, 24.0);
                        d.output_gain_db
                    }
                    "low_gain_db" => {
                        d.low_band.gain_db = value.clamp(-24.0, 24.0);
                        d.low_band.gain_db
                    }
                    "mid_gain_db" => {
                        d.mid_band.gain_db = value.clamp(-24.0, 24.0);
                        d.mid_band.gain_db
                    }
                    "high_gain_db" => {
                        d.high_band.gain_db = value.clamp(-24.0, 24.0);
                        d.high_band.gain_db
                    }
                    _ => return Err(format!("Unknown Multiband Dynamics parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::HybridReverb(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "decay_s" | "decay" => {
                        d.decay_s = value.clamp(0.1, 20.0);
                        d.decay_s
                    }
                    "size" | "room_size" => {
                        d.size = value.clamp(0.2, 2.0);
                        d.size
                    }
                    "damping" => {
                        d.damping = value.clamp(0.0, 1.0);
                        d.damping
                    }
                    "predelay_ms" | "predelay" => {
                        d.predelay_ms = value.clamp(0.0, 200.0);
                        d.predelay_ms
                    }
                    "conv_algo_blend" | "blend" => {
                        d.conv_algo_blend = value.clamp(0.0, 1.0);
                        d.conv_algo_blend
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    _ => return Err(format!("Unknown Hybrid Reverb parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::SpectralResonator(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "root_hz" | "freq" => {
                        d.root_hz = value.clamp(20.0, 4000.0);
                        d.root_hz
                    }
                    "decay_s" | "decay" => {
                        d.decay_s = value.clamp(0.05, 10.0);
                        d.decay_s
                    }
                    "unison_spread" | "spread" => {
                        d.unison_spread = value.clamp(0.0, 1.0);
                        d.unison_spread
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    _ => return Err(format!("Unknown Spectral Resonator parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Echo(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "delay_time_l_ms" | "time_l" | "delay_ms" => {
                        d.delay_time_l_ms = value.clamp(1.0, 2400.0);
                        d.delay_time_l_ms
                    }
                    "delay_time_r_ms" | "time_r" => {
                        d.delay_time_r_ms = value.clamp(1.0, 2400.0);
                        d.delay_time_r_ms
                    }
                    "feedback" => {
                        d.feedback = value.clamp(0.0, 0.98);
                        d.feedback
                    }
                    "cross_feed" => {
                        d.cross_feed = value.clamp(0.0, 1.0);
                        d.cross_feed
                    }
                    "wow" => {
                        d.wow = value.clamp(0.0, 1.0);
                        d.wow
                    }
                    "flutter" => {
                        d.flutter = value.clamp(0.0, 1.0);
                        d.flutter
                    }
                    "ducking_db" => {
                        d.ducking_db = value.clamp(-30.0, 0.0);
                        d.ducking_db
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    _ => return Err(format!("Unknown Echo parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::BeatRepeat(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "interval_beats" | "interval" => {
                        d.interval_beats = (value as f64).clamp(0.25, 16.0);
                        d.interval_beats as f32
                    }
                    "grid_beats" | "grid" => {
                        d.grid_beats = (value as f64).clamp(0.0625, 4.0);
                        d.grid_beats as f32
                    }
                    "chance" => {
                        d.chance = value.clamp(0.0, 1.0);
                        d.chance
                    }
                    "gate" => {
                        d.gate = value.clamp(0.1, 1.0);
                        d.gate
                    }
                    "pitch_decay" => {
                        d.pitch_decay = value.clamp(-12.0, 12.0);
                        d.pitch_decay
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    _ => return Err(format!("Unknown Beat Repeat parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Corpus(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "tune_hz" | "tune" => {
                        AudioEffectDevice::set_parameter(&mut *d, 1, value);
                        d.tune_hz
                    }
                    "decay_s" | "decay" => {
                        AudioEffectDevice::set_parameter(&mut *d, 2, value);
                        d.decay_s
                    }
                    "material" => {
                        AudioEffectDevice::set_parameter(&mut *d, 3, value);
                        d.material
                    }
                    "dry_wet" | "wet" | "mix" => {
                        AudioEffectDevice::set_parameter(&mut *d, 0, value);
                        d.dry_wet
                    }
                    _ => return Err(format!("Unknown Corpus parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Redux(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "downsample" => {
                        d.downsample = (value.round() as usize).clamp(1, 256);
                        d.downsample as f32
                    }
                    "bit_depth" | "bits" => {
                        d.bit_depth = (value.round() as u8).clamp(1, 16);
                        d.bit_depth as f32
                    }
                    "jitter" => {
                        d.jitter = value.clamp(0.0, 1.0);
                        d.jitter
                    }
                    "dry_wet" | "wet" | "mix" => {
                        d.dry_wet = value.clamp(0.0, 1.0);
                        d.dry_wet
                    }
                    _ => return Err(format!("Unknown Redux parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Utility(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "gain_db" | "gain" => {
                        d.gain_db = value.clamp(-60.0, 35.0);
                        d.gain_db
                    }
                    "pan" => {
                        d.pan = value.clamp(-1.0, 1.0);
                        d.pan
                    }
                    "width" => {
                        d.width = value.clamp(0.0, 4.0);
                        d.width
                    }
                    "bass_mono_freq_hz" => {
                        d.bass_mono_freq_hz = value.clamp(20.0, 500.0);
                        d.bass_mono_freq_hz
                    }
                    "bass_mono" => {
                        d.bass_mono = value >= 0.5;
                        if d.bass_mono { 1.0 } else { 0.0 }
                    }
                    "mute" => {
                        d.mute = value >= 0.5;
                        if d.mute { 1.0 } else { 0.0 }
                    }
                    _ => return Err(format!("Unknown Utility parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::MasterLimiter(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "ceiling" | "ceiling_linear" => {
                        d.ceiling = value.clamp(0.01, 1.0);
                        d.ceiling
                    }
                    "ceiling_db" => {
                        d.ceiling = 10.0_f32.powf(value.clamp(-24.0, 0.0) / 20.0);
                        d.ceiling
                    }
                    _ => return Err(format!("Unknown Limiter parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Arpeggiator(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "rate_beats" | "rate" => {
                        d.rate_beats = (value as f64).clamp(0.0625, 4.0);
                        d.rate_beats as f32
                    }
                    "gate" => {
                        d.gate = value.clamp(0.1, 2.0);
                        d.gate
                    }
                    "octaves" => {
                        d.octaves = (value.round() as u8).clamp(1, 4);
                        d.octaves as f32
                    }
                    "velocity_decay" => {
                        d.velocity_decay = value.clamp(0.0, 1.0);
                        d.velocity_decay
                    }
                    _ => return Err(format!("Unknown Arpeggiator parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Chord(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(num) = k.strip_prefix("shift").and_then(|s| s.parse::<usize>().ok()) {
                    if (1..=6).contains(&num) {
                        d.shifts[num - 1] = value.round().clamp(-36.0, 36.0) as i8;
                        return Ok((k.to_string(), d.shifts[num - 1] as f32));
                    }
                }
                Err(format!("Unknown Chord parameter '{}'", key))
            }
            Self::Scale(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "base_note" => {
                        d.base_note = (value.round() as i32).rem_euclid(12) as u8;
                        d.base_note as f32
                    }
                    "transpose" => {
                        d.transpose = value.round().clamp(-24.0, 24.0) as i8;
                        d.transpose as f32
                    }
                    _ => return Err(format!("Unknown Scale parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
            Self::Velocity(dev) => {
                let mut d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let applied = match k {
                    "drive" => {
                        d.drive = value.clamp(-1.0, 1.0);
                        d.drive
                    }
                    "compand" => {
                        d.compand = value.clamp(0.0, 2.0);
                        d.compand
                    }
                    "random" => {
                        d.random = value.round().clamp(0.0, 40.0) as u8;
                        d.random as f32
                    }
                    "min_velocity" => {
                        d.min_velocity = value.round().clamp(1.0, 127.0) as u8;
                        d.min_velocity as f32
                    }
                    "max_velocity" => {
                        d.max_velocity = value.round().clamp(1.0, 127.0) as u8;
                        d.max_velocity as f32
                    }
                    _ => return Err(format!("Unknown Velocity parameter '{}'", key)),
                };
                Ok((k.to_string(), applied))
            }
        }
    }

    /// Serialize an audio effect handle into `oxide_project::InsertEffectConfig`
    /// so project save, `.als` export, and offline bounce preserve all device parameters.
    pub fn to_insert_effect_config(&self, bpm: f64) -> Option<InsertEffectConfig> {
        match self {
            Self::EqEight { dev, .. } => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let mut params = HashMap::new();
                for (i, b) in d.bands.iter().enumerate() {
                    let n = i + 1;
                    params.insert(format!("band{}_freq_hz", n), b.freq_hz);
                    params.insert(format!("band{}_gain_db", n), b.gain_db);
                    params.insert(format!("band{}_q", n), b.q);
                    params.insert(
                        format!("band{}_enabled", n),
                        if b.enabled { 1.0 } else { 0.0 },
                    );
                }
                Some(InsertEffectConfig::EqEight { params })
            }
            Self::GlueCompressor(dev) => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                Some(InsertEffectConfig::GlueCompressor {
                    threshold_db: d.threshold_db,
                    ratio: d.ratio,
                    attack_ms: d.attack_ms,
                    release_s: d.release_s,
                    makeup_gain_db: d.makeup_gain_db,
                    dry_wet: d.dry_wet,
                })
            }
            Self::Roar(dev) => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                Some(InsertEffectConfig::Roar {
                    drive_db: d.stage1.drive_db,
                    tone: d.stage1.tone,
                    dry_wet: d.dry_wet,
                    output_gain_db: d.output_gain_db,
                })
            }
            Self::AutoFilter(dev) => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                Some(InsertEffectConfig::AutoFilter {
                    cutoff_hz: d.cutoff_hz,
                    resonance: d.resonance,
                    dry_wet: d.dry_wet,
                })
            }
            Self::MultibandDynamics(dev) => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                Some(InsertEffectConfig::MultibandDynamics {
                    amount: d.amount,
                    time_macro: d.time_macro,
                    input_gain_db: d.input_gain_db,
                    output_gain_db: d.output_gain_db,
                })
            }
            Self::Echo(dev) => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                let safe_bpm = if bpm.is_finite() && bpm >= 20.0 {
                    bpm
                } else {
                    120.0
                };
                let beat_ms = (60_000.0 / safe_bpm) as f32;
                Some(InsertEffectConfig::TempoDelay {
                    time_beats: (d.delay_time_l_ms / beat_ms).clamp(0.0625, 4.0),
                    feedback: d.feedback,
                    mix: d.dry_wet,
                })
            }
            Self::HybridReverb(dev) => {
                let d = dev.lock().unwrap_or_else(|e| e.into_inner());
                Some(InsertEffectConfig::Reverb {
                    ir_path: None,
                    decay_ms: d.decay_s * 1000.0,
                    mix: d.dry_wet,
                })
            }
            Self::SpectralResonator(_)
            | Self::BeatRepeat(_)
            | Self::Corpus(_)
            | Self::Redux(_)
            | Self::Utility(_)
            | Self::MasterLimiter(_) => {
                let params: HashMap<String, f32> = self.get_params().into_iter().collect();
                Some(InsertEffectConfig::GenericDevice {
                    device_type: self.name(),
                    params,
                })
            }
            _ => None,
        }
    }
}

/// Instantiate a device along with a live `ManagedDeviceHandle` sharing the exact `Arc<Mutex<T>>`.
pub fn instantiate_managed_device(
    device_type: &str,
    sample_rate: f32,
) -> Result<(CreatedDevice, ManagedDeviceHandle), String> {
    let clean = device_type
        .trim()
        .to_lowercase()
        .replace([' ', '-', '_'], "");
    let sr = if sample_rate <= 0.0 {
        44100.0
    } else {
        sample_rate
    };

    macro_rules! mk_inst {
        ($variant:ident, $expr:expr) => {{
            let inner = Arc::new(Mutex::new($expr));
            let dev = CreatedDevice::Instrument(Box::new(SharedDevice::new(Arc::clone(&inner))));
            let handle = ManagedDeviceHandle::$variant(inner);
            Ok((dev, handle))
        }};
    }

    macro_rules! mk_audio {
        ($variant:ident, $expr:expr) => {{
            let inner = Arc::new(Mutex::new($expr));
            let dev = CreatedDevice::Audio(Box::new(SharedDevice::new(Arc::clone(&inner))));
            let handle = ManagedDeviceHandle::$variant(inner);
            Ok((dev, handle))
        }};
    }

    macro_rules! mk_midi {
        ($variant:ident, $expr:expr) => {{
            let inner = Arc::new(Mutex::new($expr));
            let dev = CreatedDevice::Midi(Box::new(SharedDevice::new(Arc::clone(&inner))));
            let handle = ManagedDeviceHandle::$variant(inner);
            Ok((dev, handle))
        }};
    }

    match clean.as_str() {
        // --- Instruments ---
        "wavetable" => mk_inst!(Wavetable, WavetableDevice::new(sr)),
        "operator" => mk_inst!(Operator, OperatorDevice::new(sr)),
        "drift" => mk_inst!(Drift, DriftDevice::new(sr)),
        "meld" => mk_inst!(Meld, MeldDevice::new(sr)),
        "analog" => mk_inst!(Analog, AnalogDevice::new(sr)),
        "collision" => mk_inst!(Collision, CollisionDevice::new(sr)),
        "tension" => mk_inst!(Tension, TensionDevice::new(sr)),
        "electric" => mk_inst!(Electric, ElectricDevice::new(sr)),
        "simpler" => mk_inst!(Simpler, SimplerDevice::new(sr)),
        "drumrack" | "drums" => mk_inst!(DrumRack, DrumRack::new(sr)),

        // --- Audio Effects ---
        "eqeight" | "eq8" => {
            let inner = Arc::new(Mutex::new(EQEight::new(sr)));
            let dev = CreatedDevice::Audio(Box::new(SharedDevice::new(Arc::clone(&inner))));
            let handle = ManagedDeviceHandle::EqEight {
                dev: inner,
                sample_rate: sr,
            };
            Ok((dev, handle))
        }
        "gluecompressor" | "glue" => mk_audio!(GlueCompressor, GlueCompressor::new(sr)),
        "roar" => mk_audio!(Roar, Roar::new(sr)),
        "autofilter" => mk_audio!(AutoFilter, AutoFilter::new(sr)),
        "ott" | "multibanddynamics" => {
            mk_audio!(MultibandDynamics, MultibandDynamics::new(sr))
        }
        "hybridreverb" | "reverb" => mk_audio!(HybridReverb, HybridReverb::new(sr)),
        "spectralresonator" => mk_audio!(SpectralResonator, SpectralResonator::new(sr)),
        "echo" | "delay" => mk_audio!(Echo, Echo::new(sr)),
        "beatrepeat" => mk_audio!(BeatRepeat, BeatRepeat::new(sr)),
        "corpus" => mk_audio!(Corpus, Corpus::new(sr)),
        "redux" => mk_audio!(Redux, Redux::new()),
        "utility" => mk_audio!(Utility, Utility::new(sr)),
        "limiter" | "masterlimiter" => mk_audio!(MasterLimiter, MasterLimiter::new(sr)),

        // --- MIDI Effects ---
        "arpeggiator" | "arp" => mk_midi!(Arpeggiator, Arpeggiator::new()),
        "chord" => mk_midi!(Chord, Chord::new()),
        "scale" => mk_midi!(Scale, Scale::new()),
        "velocity" => mk_midi!(Velocity, Velocity::new()),

        _ => Err(format!(
            "Device '{}' not recognized. Supported: Wavetable, Operator, Drift, Meld, Analog, Collision, Tension, Electric, Simpler, DrumRack, EQEight, GlueCompressor, Roar, AutoFilter, OTT, HybridReverb, SpectralResonator, Echo, BeatRepeat, Corpus, Redux, Utility, Limiter, Arpeggiator, Chord, Scale, Velocity",
            device_type
        )),
    }
}

/// Reconstruct a live device + handle from an `oxide_project::InsertEffectConfig`.
pub fn from_insert_effect_config(
    cfg: &InsertEffectConfig,
    sample_rate: f32,
) -> Result<(CreatedDevice, ManagedDeviceHandle), String> {
    match cfg {
        InsertEffectConfig::Saturator {
            drive_db,
            output_gain_db,
            ..
        } => {
            let (dev, handle) = instantiate_managed_device("Roar", sample_rate)?;
            let _ = handle.set_parameter("drive_db", *drive_db);
            let _ = handle.set_parameter("output_gain_db", *output_gain_db);
            Ok((dev, handle))
        }
        InsertEffectConfig::TempoDelay {
            time_beats,
            feedback,
            mix,
        } => {
            let (dev, handle) = instantiate_managed_device("Echo", sample_rate)?;
            let ms = (*time_beats * 500.0).clamp(1.0, 2400.0);
            let _ = handle.set_parameter("delay_time_l_ms", ms);
            let _ = handle.set_parameter("delay_time_r_ms", ms * 0.75);
            let _ = handle.set_parameter("feedback", *feedback);
            let _ = handle.set_parameter("dry_wet", *mix);
            Ok((dev, handle))
        }
        InsertEffectConfig::Reverb { decay_ms, mix, .. } => {
            let (dev, handle) = instantiate_managed_device("HybridReverb", sample_rate)?;
            let _ = handle.set_parameter("decay_s", *decay_ms / 1000.0);
            let _ = handle.set_parameter("dry_wet", *mix);
            Ok((dev, handle))
        }
        InsertEffectConfig::EqEight { params } => {
            let (dev, handle) = instantiate_managed_device("EQEight", sample_rate)?;
            for (k, v) in params {
                let _ = handle.set_parameter(k, *v);
            }
            Ok((dev, handle))
        }
        InsertEffectConfig::GlueCompressor {
            threshold_db,
            ratio,
            attack_ms,
            release_s,
            makeup_gain_db,
            dry_wet,
        } => {
            let (dev, handle) = instantiate_managed_device("GlueCompressor", sample_rate)?;
            let _ = handle.set_parameter("threshold_db", *threshold_db);
            let _ = handle.set_parameter("ratio", *ratio);
            let _ = handle.set_parameter("attack_ms", *attack_ms);
            let _ = handle.set_parameter("release_s", *release_s);
            let _ = handle.set_parameter("makeup_gain_db", *makeup_gain_db);
            let _ = handle.set_parameter("dry_wet", *dry_wet);
            Ok((dev, handle))
        }
        InsertEffectConfig::Roar {
            drive_db,
            tone,
            dry_wet,
            output_gain_db,
        } => {
            let (dev, handle) = instantiate_managed_device("Roar", sample_rate)?;
            let _ = handle.set_parameter("drive_db", *drive_db);
            let _ = handle.set_parameter("tone", *tone);
            let _ = handle.set_parameter("dry_wet", *dry_wet);
            let _ = handle.set_parameter("output_gain_db", *output_gain_db);
            Ok((dev, handle))
        }
        InsertEffectConfig::AutoFilter {
            cutoff_hz,
            resonance,
            dry_wet,
        } => {
            let (dev, handle) = instantiate_managed_device("AutoFilter", sample_rate)?;
            let _ = handle.set_parameter("cutoff_hz", *cutoff_hz);
            let _ = handle.set_parameter("resonance", *resonance);
            let _ = handle.set_parameter("dry_wet", *dry_wet);
            Ok((dev, handle))
        }
        InsertEffectConfig::Compressor {
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_gain_db,
            dry_wet,
        } => {
            let (dev, handle) = instantiate_managed_device("GlueCompressor", sample_rate)?;
            let _ = handle.set_parameter("threshold_db", *threshold_db);
            let _ = handle.set_parameter("ratio", *ratio);
            let _ = handle.set_parameter("attack_ms", *attack_ms);
            let _ = handle.set_parameter("release_s", *release_ms / 1000.0);
            let _ = handle.set_parameter("makeup_gain_db", *makeup_gain_db);
            let _ = handle.set_parameter("dry_wet", *dry_wet);
            Ok((dev, handle))
        }
        InsertEffectConfig::MultibandDynamics {
            amount,
            time_macro,
            input_gain_db,
            output_gain_db,
        } => {
            let (dev, handle) = instantiate_managed_device("MultibandDynamics", sample_rate)?;
            let _ = handle.set_parameter("amount", *amount);
            let _ = handle.set_parameter("time_macro", *time_macro);
            let _ = handle.set_parameter("input_gain_db", *input_gain_db);
            let _ = handle.set_parameter("output_gain_db", *output_gain_db);
            Ok((dev, handle))
        }
        InsertEffectConfig::GenericDevice {
            device_type,
            params,
        } => {
            let (dev, handle) = instantiate_managed_device(device_type, sample_rate)?;
            for (k, v) in params {
                let _ = handle.set_parameter(k, *v);
            }
            Ok((dev, handle))
        }
    }
}

/// Factory function to instantiate any Live 12 device by name.
pub fn instantiate_device(device_type: &str, sample_rate: f32) -> Result<CreatedDevice, String> {
    instantiate_managed_device(device_type, sample_rate).map(|(dev, _)| dev)
}

/// Attach an instantiated device into a DeviceChain and return its global index
/// in `[midi_effects..., instrument?, audio_effects...]`.
pub fn add_device_to_chain(
    chain: &mut DeviceChain,
    dev: CreatedDevice,
    position: Option<usize>,
) -> usize {
    match dev {
        CreatedDevice::Midi(mfx) => {
            if let Some(pos) = position {
                let clamped = pos.min(chain.midi_effects.len());
                chain.insert_midi_effect(clamped, mfx);
                clamped
            } else {
                chain.add_midi_effect(mfx)
            }
        }
        CreatedDevice::Instrument(inst) => {
            chain.set_instrument(Some(inst));
            chain.midi_effects.len()
        }
        CreatedDevice::Audio(afx) => {
            let prefix = chain.midi_effects.len() + usize::from(chain.has_instrument());
            let local_idx = if let Some(pos) = position {
                let clamped = pos.min(chain.audio_effects.len());
                chain.insert_audio_effect(clamped, afx);
                clamped
            } else {
                chain.add_audio_effect(afx)
            };
            prefix + local_idx
        }
    }
}

/// Reorder devices within a matching category using global chain indices
/// `[midi_effects..., instrument?, audio_effects...]`.
pub fn reorder_device_chain_global(
    chain: &mut DeviceChain,
    handles: Option<&mut ManagedChainHandles>,
    from_index: usize,
    to_index: usize,
) -> Result<(), String> {
    let total = chain.device_count();
    if from_index >= total || to_index >= total {
        return Err(format!(
            "Reorder indices ({}, {}) out of bounds for chain with {} devices",
            from_index, to_index, total
        ));
    }
    let midi_len = chain.midi_effects.len();
    let inst_offset = usize::from(chain.has_instrument());
    let audio_offset = midi_len + inst_offset;

    if from_index < midi_len && to_index < midi_len {
        if !chain.reorder_midi_effects(from_index, to_index) {
            return Err("Failed reordering MIDI effects".to_string());
        }
        if let Some(hs) = handles {
            hs.reorder_midi(from_index, to_index);
        }
        Ok(())
    } else if from_index >= audio_offset && to_index >= audio_offset {
        let from_local = from_index - audio_offset;
        let to_local = to_index - audio_offset;
        if !chain.reorder_audio_effects(from_local, to_local) {
            return Err("Failed reordering audio effects".to_string());
        }
        if let Some(hs) = handles {
            hs.reorder_audio(from_local, to_local);
        }
        Ok(())
    } else {
        Err(format!(
            "Cannot reorder across device categories or move instrument slot (from={}, to={})",
            from_index, to_index
        ))
    }
}

fn params_to_json(params: Vec<(String, f32)>) -> Value {
    let mut map = Map::new();
    for (k, v) in params {
        map.insert(k, json!(v));
    }
    Value::Object(map)
}

/// Serialize a DeviceChain into a structured JSON representation.
pub fn dump_device_chain(chain: &DeviceChain) -> Value {
    dump_device_chain_detailed(chain, None)
}

/// Serialize a DeviceChain with live parameter snapshots when `handles` are provided.
pub fn dump_device_chain_detailed(
    chain: &DeviceChain,
    handles: Option<&ManagedChainHandles>,
) -> Value {
    let mut devices = Vec::new();
    let mut idx = 0;

    for (m_idx, mfx) in chain.midi_effects.iter().enumerate() {
        let mut entry = json!({
            "index": idx,
            "category_index": m_idx,
            "name": mfx.name(),
            "category": "MidiEffect",
            "enabled": mfx.is_enabled(),
            "latency_samples": mfx.latency_samples()
        });
        if let Some(h) = handles.and_then(|hs| hs.midi_effects.get(m_idx)) {
            entry["parameters"] = params_to_json(h.get_params());
        }
        devices.push(entry);
        idx += 1;
    }

    if let Some(ref inst) = chain.instrument {
        let mut entry = json!({
            "index": idx,
            "category_index": 0,
            "name": inst.name(),
            "category": "Instrument",
            "enabled": inst.is_enabled(),
            "latency_samples": inst.latency_samples()
        });
        if let Some(ref h) = handles.and_then(|hs| hs.instrument.as_ref()) {
            entry["parameters"] = params_to_json(h.get_params());
        }
        devices.push(entry);
        idx += 1;
    }

    for (a_idx, afx) in chain.audio_effects.iter().enumerate() {
        let mut entry = json!({
            "index": idx,
            "category_index": a_idx,
            "name": afx.name(),
            "category": "AudioEffect",
            "enabled": afx.is_enabled(),
            "latency_samples": afx.latency_samples()
        });
        if let Some(h) = handles.and_then(|hs| hs.audio_effects.get(a_idx)) {
            entry["parameters"] = params_to_json(h.get_params());
        }
        devices.push(entry);
        idx += 1;
    }

    json!({
        "chain_name": chain.name(),
        "enabled": chain.is_enabled(),
        "total_latency_samples": chain.total_latency_samples(),
        "device_count": devices.len(),
        "devices": devices
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejects_non_finite_parameters_and_global_indices_and_drum_pads() {
        let mut chain = DeviceChain::new();
        let mut handles = ManagedChainHandles::default();

        let (inst, inst_h) = instantiate_managed_device("Wavetable", 44100.0).unwrap();
        let (echo, echo_h) = instantiate_managed_device("Echo", 44100.0).unwrap();
        let (lim, lim_h) = instantiate_managed_device("MasterLimiter", 44100.0).unwrap();

        assert_eq!(add_device_to_chain(&mut chain, inst, None), 0);
        assert_eq!(handles.add_handle(DeviceCategory::Instrument, inst_h, None), 0);
        assert_eq!(add_device_to_chain(&mut chain, echo, None), 1);
        assert_eq!(handles.add_handle(DeviceCategory::AudioEffect, echo_h, None), 1);
        assert_eq!(add_device_to_chain(&mut chain, lim, None), 2);
        assert_eq!(handles.add_handle(DeviceCategory::AudioEffect, lim_h, None), 2);

        // Reject NaN/Inf across devices
        assert!(handles.get_at(1).unwrap().set_parameter("feedback", f32::NAN).is_err());
        assert!(handles.get_at(1).unwrap().set_parameter("feedback", f32::INFINITY).is_err());

        // Global reorder of AudioEffects (indices 1 <-> 2) succeeds; cross-category (0 <-> 1) fails
        assert!(reorder_device_chain_global(&mut chain, Some(&mut handles), 0, 1).is_err());
        assert!(reorder_device_chain_global(&mut chain, Some(&mut handles), 1, 2).is_ok());
        assert_eq!(handles.get_at(1).unwrap().name(), "Limiter");

        // MasterLimiter emits ceiling_db
        handles.get_at(1).unwrap().set_parameter("ceiling_db", -6.0).unwrap();
        let lim_params = handles.get_at(1).unwrap().get_params();
        assert_eq!(lim_params[0].0, "ceiling_db");
        assert!((lim_params[0].1 - (-6.0)).abs() < 0.05);

        // Echo uses BPM for time_beats conversion (e.g. 500ms at 60 BPM = 0.5 beat!)
        handles.get_at(2).unwrap().set_parameter("delay_time_l_ms", 500.0).unwrap();
        if let Some(InsertEffectConfig::TempoDelay { time_beats, .. }) =
            handles.get_at(2).unwrap().to_insert_effect_config(60.0)
        {
            assert!((time_beats - 0.5).abs() < 1e-4, "500ms at 60 BPM must be 0.5 beat, got {}", time_beats);
        } else {
            panic!("Expected TempoDelay");
        }

        // DrumRack exposes pad0..15
        let (_, drum_h) = instantiate_managed_device("DrumRack", 44100.0).unwrap();
        assert!(drum_h.set_parameter("pad15_volume", 0.42).is_ok());
        assert!(drum_h.get_params().iter().any(|(k, v)| k == "pad15_volume" && (*v - 0.42).abs() < 1e-4));

        // Corpus tune_hz runs rebuild_modes via AudioEffectDevice::set_parameter
        let (_, corpus_h) = instantiate_managed_device("Corpus", 44100.0).unwrap();
        assert!(corpus_h.set_parameter("tune_hz", 330.0).is_ok());
    }
}
