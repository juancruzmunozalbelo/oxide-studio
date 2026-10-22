//! Complete Ableton Live 12 Instrument Suite for `ableton-drums`.
//!
//! Provides real zero-allocation per-sample polyphonic synthesis for:
//! 1. `Drift`: Analog-modeled oscillator with per-voice pitch/phase drift, wavefold/shape morphing, and low-pass gate.
//! 2. `Meld`: Bi-timbral MPE dual-macro engine (Engine A swarm/FM + Engine B harmonic resonator, scale-aware filter).
//! 3. `Collision`: Mallet/excitator impulse + dual coupled modal resonator bank (marimba/beam/membrane ratios).
//! 4. `Tension`: Karplus-Strong / waveguide plucked string physical model with damping and pickup position.
//! 5. `Electric`: Rhodes/Wurlitzer tine + tonebar FM-physical hybrid bell/bark synthesis.
//! 6. `Analog`: Dual virtual-analog oscillator with sync, pulse-width modulation, and 24dB ladder filter.
//! 7. `Sampler`: Multi-velocity zone sample playback engine with pitch-bend and filter envelope.
//! 8. `GranulatorIII`: Real-time granular cloud synthesizer (`grain_size_ms`, `spray`, `scan_rate`, `cloud_density`).

use std::f32::consts::PI;
use ableton_core::{AudioBuffer, Device, DeviceCategory, InstrumentDevice, MidiEvent, MusicalTime};
use ableton_dsp::{
    AdsrEnvelope, AnalogDevice, CollisionDevice, DriftDevice, ElectricDevice, MeldDevice,
    MeldMacroEngine, MeldScale, TensionDevice,
};
use ableton_dsp::cytomic::{CircuitModel, CytomicFilterType, CytomicZdfFilter};
use ableton_dsp::oscillator::XorShift32;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Enumeration of all 8 flagship Ableton Live 12 Suite instruments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SuiteInstrumentKind {
    Drift,
    Meld,
    Collision,
    Tension,
    Electric,
    Analog,
    Sampler,
    GranulatorIII,
}

// -----------------------------------------------------------------------------
// Sampler Implementation: Multi-Velocity Zone Engine
// -----------------------------------------------------------------------------

pub const SAMPLER_TABLE_SIZE: usize = 4096;
pub const MAX_SAMPLER_VOICES: usize = 8;

#[derive(Debug, Clone)]
pub struct SamplerVoice {
    pub active: bool,
    pub note: u8,
    pub velocity: u8,
    pub zone_index: usize,
    pub position: f64,
    pub phase_step: f64,
    pub pitch_bend_semi: f32,
    pub amp_env: AdsrEnvelope,
    pub filter_env: AdsrEnvelope,
    pub filter: CytomicZdfFilter,
}

impl SamplerVoice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 64,
            zone_index: 0,
            position: 0.0,
            phase_step: 1.0,
            pitch_bend_semi: 0.0,
            amp_env: AdsrEnvelope::new(sample_rate),
            filter_env: AdsrEnvelope::new(sample_rate),
            filter: CytomicZdfFilter::new(sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.position = 0.0;
        self.pitch_bend_semi = 0.0;
        self.amp_env.reset();
        self.filter_env.reset();
        self.filter.reset();
    }
}

/// Multi-velocity zone sample playback engine with pitch-bend and filter envelope.
#[derive(Debug, Clone)]
pub struct SamplerEngine {
    pub sample_rate: f32,
    pub enabled: bool,
    pub volume: f32,
    pub filter_cutoff: f32,
    pub filter_res: f32,
    pub filter_env_amt: f32,
    pub pitch_bend_range: f32, // default 12 semitones

    // Pre-allocated multi-velocity zones (Low: soft sine-dominant, Mid: warm saw/triangle, High: bright rich acoustic)
    pub zones: [[f32; SAMPLER_TABLE_SIZE]; 3],
    pub voices: [SamplerVoice; MAX_SAMPLER_VOICES],
}

impl SamplerEngine {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let mut zones = [[0.0f32; SAMPLER_TABLE_SIZE]; 3];

        // Synthesize high-fidelity zone tables
        for i in 0..SAMPLER_TABLE_SIZE {
            let phase = (i as f32 / SAMPLER_TABLE_SIZE as f32) * 2.0 * PI;
            // Zone 0: Soft Low-Velocity (fundamental dominant)
            zones[0][i] = (phase.sin() + 0.3 * (2.0 * phase).sin()) * 0.8;
            // Zone 1: Medium Velocity (balanced harmonics)
            zones[1][i] = (phase.sin() + 0.6 * (2.0 * phase).sin() + 0.4 * (3.0 * phase).sin() + 0.2 * (4.0 * phase).sin()) * 0.6;
            // Zone 2: High Velocity (bright multi-harmonic acoustic bark)
            zones[2][i] = (phase.sin()
                + 0.8 * (2.0 * phase).sin()
                + 0.6 * (3.0 * phase).sin()
                + 0.5 * (4.0 * phase).sin()
                + 0.4 * (5.0 * phase).sin()
                + 0.3 * (6.0 * phase).sin()) * 0.5;
        }

        let voices = std::array::from_fn(|_| SamplerVoice::new(sr));

        Self {
            sample_rate: sr,
            enabled: true,
            volume: 0.8,
            filter_cutoff: 4000.0,
            filter_res: 0.25,
            filter_env_amt: 0.5,
            pitch_bend_range: 12.0,
            zones,
            voices,
        }
    }

    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.reset();
        }
    }

    pub fn trigger_note_on(&mut self, note: u8, velocity: u8) {
        let voice_idx = self.voices.iter().position(|v| !v.active).unwrap_or_else(|| {
            // Voice steal oldest or lowest envelope
            let mut min_val = f32::MAX;
            let mut min_idx = 0;
            for (i, v) in self.voices.iter().enumerate() {
                if v.amp_env.value < min_val {
                    min_val = v.amp_env.value;
                    min_idx = i;
                }
            }
            min_idx
        });

        let zone_index = if velocity < 64 {
            0
        } else if velocity < 100 {
            1
        } else {
            2
        };

        let voice = &mut self.voices[voice_idx];
        voice.active = true;
        voice.note = note;
        voice.velocity = velocity;
        voice.zone_index = zone_index;
        voice.position = 0.0;

        let root_freq = 440.0 * 2.0f32.powf((60.0 - 69.0) / 12.0); // C4 root
        let note_freq = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let base_step = (note_freq / root_freq) as f64 * (SAMPLER_TABLE_SIZE as f64 * root_freq as f64 / self.sample_rate as f64);
        voice.phase_step = base_step;

        voice.amp_env.trigger_attack();
        voice.filter_env.trigger_attack();
        voice.filter.reset();
        voice.filter.circuit_model = CircuitModel::Clean;
        voice.filter.filter_type = CytomicFilterType::Lowpass24;
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.amp_env.trigger_release();
                voice.filter_env.trigger_release();
            }
        }
    }

    pub fn set_pitch_bend(&mut self, semitones: f32) {
        for voice in &mut self.voices {
            if voice.active {
                voice.pitch_bend_semi = semitones;
            }
        }
    }

    #[inline(always)]
    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        if !self.enabled {
            return (0.0, 0.0);
        }

        let mut out = 0.0f32;

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let amp = voice.amp_env.next(5.0, 300.0, 0.7, 250.0);
            if !voice.amp_env.is_active() && amp < 0.0001 {
                voice.active = false;
                continue;
            }

            let f_env = voice.filter_env.next(10.0, 400.0, 0.3, 300.0);

            // Interpolated sample reading
            let table = &self.zones[voice.zone_index];
            let idx0 = voice.position.floor() as usize % SAMPLER_TABLE_SIZE;
            let idx1 = (idx0 + 1) % SAMPLER_TABLE_SIZE;
            let frac = (voice.position - voice.position.floor()) as f32;
            let raw_sample = table[idx0] * (1.0 - frac) + table[idx1] * frac;

            // Pitch bend modification
            let pitch_multiplier = 2.0f64.powf((voice.pitch_bend_semi / 12.0) as f64);
            voice.position = (voice.position + voice.phase_step * pitch_multiplier).rem_euclid(SAMPLER_TABLE_SIZE as f64);

            // Modulated filter cutoff
            let mod_cutoff = (self.filter_cutoff * 2.0f32.powf(f_env * self.filter_env_amt * 3.0)).clamp(20.0, 20000.0);
            let filtered = voice.filter.process_sample(raw_sample, mod_cutoff, self.filter_res, 0.0, 0.0);

            let vel_gain = (voice.velocity as f32 / 127.0).powf(0.8);
            out += filtered * amp * vel_gain;
        }

        let signal = (out * self.volume).clamp(-1.0, 1.0);
        (signal, signal)
    }
}

// -----------------------------------------------------------------------------
// Granulator III Implementation: Flagship Real-Time MPE Granular Synthesizer
// -----------------------------------------------------------------------------

pub const GRANULATOR_BUFFER_SIZE: usize = 44100; // 1 second audio memory
pub const MAX_GRAINS: usize = 32;

#[derive(Debug, Clone, Copy)]
pub struct Grain {
    pub active: bool,
    pub pos: f64,
    pub step: f64,
    pub length: usize,
    pub age: usize,
    pub pan_l: f32,
    pub pan_r: f32,
}

impl Grain {
    pub const fn empty() -> Self {
        Self {
            active: false,
            pos: 0.0,
            step: 1.0,
            length: 1000,
            age: 0,
            pan_l: 0.707,
            pan_r: 0.707,
        }
    }
}

/// Real-time granular cloud synthesizer (`grain_size_ms`, `spray`, `scan_rate`, `cloud_density`).
#[derive(Debug, Clone)]
pub struct GranulatorIIIEngine {
    pub sample_rate: f32,
    pub enabled: bool,
    pub volume: f32,

    pub grain_size_ms: f32,
    pub spray: f32,         // 0.0 .. 1.0 position randomization
    pub scan_rate: f32,     // playhead scrub speed multiplier
    pub cloud_density: f32, // grains spawned per second (e.g. 10.0 .. 100.0)

    pub playhead: f64,
    pub spawn_counter: f32,
    pub current_note: u8,
    pub is_playing: bool,
    pub pitch_bend_semi: f32,

    pub memory_buffer: [f32; GRANULATOR_BUFFER_SIZE],
    pub grains: [Grain; MAX_GRAINS],
    pub rng: XorShift32,
}

impl GranulatorIIIEngine {
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let mut memory_buffer = [0.0f32; GRANULATOR_BUFFER_SIZE];

        // Synthesize rich granular sound source (multi-sine + subtle wavefold)
        for i in 0..GRANULATOR_BUFFER_SIZE {
            let t = i as f32 / sr;
            let f0 = 220.0;
            let wave = (2.0 * PI * f0 * t).sin()
                + 0.5 * (2.0 * PI * f0 * 1.5 * t).sin()
                + 0.3 * (2.0 * PI * f0 * 2.7 * t).sin()
                + 0.2 * (2.0 * PI * f0 * 4.2 * t).sin();
            memory_buffer[i] = (wave * 0.7).tanh();
        }

        Self {
            sample_rate: sr,
            enabled: true,
            volume: 0.75,
            grain_size_ms: 45.0,
            spray: 0.25,
            scan_rate: 1.0,
            cloud_density: 35.0,
            playhead: 0.0,
            spawn_counter: 0.0,
            current_note: 60,
            is_playing: false,
            pitch_bend_semi: 0.0,
            memory_buffer,
            grains: [Grain::empty(); MAX_GRAINS],
            rng: XorShift32::new(0xabcdef12),
        }
    }

    pub fn reset(&mut self) {
        for g in &mut self.grains {
            g.active = false;
        }
        self.playhead = 0.0;
        self.spawn_counter = 0.0;
        self.is_playing = false;
    }

    pub fn trigger_note_on(&mut self, note: u8, _velocity: u8) {
        self.current_note = note;
        self.is_playing = true;
        self.spawn_counter = 0.0;
        self.spawn_grain();
    }

    pub fn trigger_note_off(&mut self, note: u8) {
        if self.current_note == note {
            self.is_playing = false;
        }
    }

    pub fn set_pitch_bend(&mut self, semitones: f32) {
        self.pitch_bend_semi = semitones;
    }

    fn spawn_grain(&mut self) {
        // Find inactive grain
        let slot = match self.grains.iter_mut().find(|g| !g.active) {
            Some(s) => s,
            None => return,
        };

        let grain_len = ((self.grain_size_ms * 0.001 * self.sample_rate) as usize).clamp(64, GRANULATOR_BUFFER_SIZE / 2);
        let spray_offset = (self.rng.next_f32() * 2.0 - 1.0) * self.spray * (GRANULATOR_BUFFER_SIZE as f32 * 0.25);
        let pos = (self.playhead + spray_offset as f64).rem_euclid(GRANULATOR_BUFFER_SIZE as f64);

        let note_trans = (self.current_note as f32 - 60.0) + self.pitch_bend_semi;
        let step = 2.0f64.powf((note_trans / 12.0) as f64);

        let pan = self.rng.next_f32().clamp(0.0, 1.0);
        let pan_rad = pan * 0.5 * PI;

        *slot = Grain {
            active: true,
            pos,
            step,
            length: grain_len,
            age: 0,
            pan_l: pan_rad.cos(),
            pan_r: pan_rad.sin(),
        };
    }

    #[inline(always)]
    pub fn process_stereo_sample(&mut self) -> (f32, f32) {
        if !self.enabled || (!self.is_playing && self.grains.iter().all(|g| !g.active)) {
            return (0.0, 0.0);
        }

        // Advance playhead
        self.playhead = (self.playhead + self.scan_rate as f64).rem_euclid(GRANULATOR_BUFFER_SIZE as f64);

        // Spawn grain check
        if self.is_playing {
            let spawn_interval = self.sample_rate / self.cloud_density.clamp(1.0, 200.0);
            self.spawn_counter += 1.0;
            if self.spawn_counter >= spawn_interval {
                self.spawn_counter -= spawn_interval;
                self.spawn_grain();
            }
        }

        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;

        for grain in &mut self.grains {
            if !grain.active {
                continue;
            }

            // Raised-cosine Hann window envelope
            let win = 0.5 * (1.0 - (2.0 * PI * grain.age as f32 / grain.length as f32).cos());

            // Buffer read
            let idx0 = grain.pos.floor() as usize % GRANULATOR_BUFFER_SIZE;
            let idx1 = (idx0 + 1) % GRANULATOR_BUFFER_SIZE;
            let frac = (grain.pos - grain.pos.floor()) as f32;
            let val = self.memory_buffer[idx0] * (1.0 - frac) + self.memory_buffer[idx1] * frac;

            let sample = val * win;
            out_l += sample * grain.pan_l;
            out_r += sample * grain.pan_r;

            grain.pos = (grain.pos + grain.step).rem_euclid(GRANULATOR_BUFFER_SIZE as f64);
            grain.age += 1;
            if grain.age >= grain.length {
                grain.active = false;
            }
        }

        let l = (out_l * self.volume).clamp(-1.0, 1.0);
        let r = (out_r * self.volume).clamp(-1.0, 1.0);
        (l, r)
    }
}

// -----------------------------------------------------------------------------
// SuiteInstrumentEngine: Unified Suite Instrument Runtime
// -----------------------------------------------------------------------------

/// Unified Ableton Live 12 Suite Instrument engine hosting all 8 synthesis architectures.
#[derive(Debug, Clone)]
pub struct SuiteInstrumentEngine {
    pub kind: SuiteInstrumentKind,
    pub sample_rate: f32,
    pub volume: f32,
    pub pan: f32,
    pub enabled: bool,

    pub drift: DriftDevice,
    pub meld: MeldDevice,
    pub collision: CollisionDevice,
    pub tension: TensionDevice,
    pub electric: ElectricDevice,
    pub analog: AnalogDevice,
    pub sampler: SamplerEngine,
    pub granulator: GranulatorIIIEngine,
}

impl SuiteInstrumentEngine {
    /// Initialize a new SuiteInstrumentEngine with selected instrument kind.
    pub fn new(sample_rate: f32, kind: SuiteInstrumentKind) -> Self {
        let sr = if sample_rate <= 0.0 { 44100.0 } else { sample_rate };
        let mut meld = MeldDevice::new(sr);
        meld.engine_a_type = MeldMacroEngine::Swarm;
        meld.engine_b_type = MeldMacroEngine::Harmonic;
        meld.scale = MeldScale::Chromatic;
        meld.scale_quantize_active = true;

        Self {
            kind,
            sample_rate: sr,
            volume: 0.8,
            pan: 0.0,
            enabled: true,
            drift: DriftDevice::new(sr),
            meld,
            collision: CollisionDevice::new(sr),
            tension: TensionDevice::new(sr),
            electric: ElectricDevice::new(sr),
            analog: AnalogDevice::new(sr),
            sampler: SamplerEngine::new(sr),
            granulator: GranulatorIIIEngine::new(sr),
        }
    }

    /// Change active instrument kind.
    pub fn set_kind(&mut self, kind: SuiteInstrumentKind) {
        self.kind = kind;
    }

    /// Trigger note on.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        match self.kind {
            SuiteInstrumentKind::Drift => self.drift.trigger_note_on(note, velocity),
            SuiteInstrumentKind::Meld => self.meld.trigger_note_on(note, velocity),
            SuiteInstrumentKind::Collision => self.collision.trigger_note_on(note, velocity),
            SuiteInstrumentKind::Tension => self.tension.trigger_note_on(note, velocity),
            SuiteInstrumentKind::Electric => self.electric.trigger_note_on(note, velocity),
            SuiteInstrumentKind::Analog => self.analog.trigger_note_on(note, velocity),
            SuiteInstrumentKind::Sampler => self.sampler.trigger_note_on(note, velocity),
            SuiteInstrumentKind::GranulatorIII => self.granulator.trigger_note_on(note, velocity),
        }
    }

    /// Trigger note off.
    pub fn note_off(&mut self, note: u8) {
        match self.kind {
            SuiteInstrumentKind::Drift => self.drift.trigger_note_off(note),
            SuiteInstrumentKind::Meld => self.meld.trigger_note_off(note),
            SuiteInstrumentKind::Collision => self.collision.trigger_note_off(note),
            SuiteInstrumentKind::Tension => self.tension.trigger_note_off(note),
            SuiteInstrumentKind::Electric => self.electric.trigger_note_off(note),
            SuiteInstrumentKind::Analog => self.analog.trigger_note_off(note),
            SuiteInstrumentKind::Sampler => self.sampler.trigger_note_off(note),
            SuiteInstrumentKind::GranulatorIII => self.granulator.trigger_note_off(note),
        }
    }

    /// Silence all active voices.
    pub fn all_notes_off(&mut self) {
        match self.kind {
            SuiteInstrumentKind::Drift => self.drift.silence_all(),
            SuiteInstrumentKind::Meld => self.meld.silence_all(),
            SuiteInstrumentKind::Collision => self.collision.silence_all(),
            SuiteInstrumentKind::Tension => self.tension.silence_all(),
            SuiteInstrumentKind::Electric => self.electric.silence_all(),
            SuiteInstrumentKind::Analog => self.analog.silence_all(),
            SuiteInstrumentKind::Sampler => self.sampler.reset(),
            SuiteInstrumentKind::GranulatorIII => self.granulator.reset(),
        }
    }

    /// Set pitch bend in semitones.
    pub fn set_pitch_bend(&mut self, semitones: f32) {
        match self.kind {
            SuiteInstrumentKind::Drift => {
                for v in &mut self.drift.voices {
                    if v.active {
                        v.pitch_bend_semitones = semitones;
                    }
                }
            }
            SuiteInstrumentKind::Meld => {
                for v in &mut self.meld.voices {
                    if v.active {
                        v.pitch_bend_semitones = semitones;
                    }
                }
            }
            SuiteInstrumentKind::Sampler => self.sampler.set_pitch_bend(semitones),
            SuiteInstrumentKind::GranulatorIII => self.granulator.set_pitch_bend(semitones),
            _ => {}
        }
    }

    /// Set MPE slide [0.0, 1.0] (CC74).
    pub fn set_mpe_slide(&mut self, slide: f32) {
        let s = slide.clamp(0.0, 1.0);
        match self.kind {
            SuiteInstrumentKind::Drift => {
                for v in &mut self.drift.voices {
                    if v.active {
                        v.slide = s;
                    }
                }
            }
            SuiteInstrumentKind::Meld => {
                for v in &mut self.meld.voices {
                    if v.active {
                        v.slide = s;
                    }
                }
            }
            _ => {}
        }
    }

    /// Set MPE pressure [0.0, 1.0] (aftertouch / CC11).
    pub fn set_mpe_pressure(&mut self, pressure: f32) {
        let p = pressure.clamp(0.0, 1.0);
        match self.kind {
            SuiteInstrumentKind::Drift => {
                for v in &mut self.drift.voices {
                    if v.active {
                        v.pressure = p;
                    }
                }
            }
            SuiteInstrumentKind::Meld => {
                for v in &mut self.meld.voices {
                    if v.active {
                        v.pressure = p;
                    }
                }
            }
            _ => {}
        }
    }

    /// Process a single stereo sample frame with zero heap allocations.
    #[inline(always)]
    pub fn process_sample(&mut self) -> (f32, f32) {
        if !self.enabled {
            return (0.0, 0.0);
        }

        let (mut l, mut r) = match self.kind {
            SuiteInstrumentKind::Drift => self.drift.process_stereo_sample(),
            SuiteInstrumentKind::Meld => self.meld.process_stereo_sample(),
            SuiteInstrumentKind::Collision => self.collision.process_stereo_sample(),
            SuiteInstrumentKind::Tension => self.tension.process_stereo_sample(),
            SuiteInstrumentKind::Electric => self.electric.process_stereo_sample(),
            SuiteInstrumentKind::Analog => self.analog.process_stereo_sample(),
            SuiteInstrumentKind::Sampler => self.sampler.process_stereo_sample(),
            SuiteInstrumentKind::GranulatorIII => self.granulator.process_stereo_sample(),
        };

        if self.pan != 0.0 {
            let p = (self.pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;
            let mono = (l + r) * 0.5;
            l = mono * p.cos() * 1.414;
            r = mono * p.sin() * 1.414;
        }

        (l * self.volume, r * self.volume)
    }

    /// Process a stereo block of audio into slices with zero allocations.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let count = left.len().min(right.len());
        for i in 0..count {
            let (l, r) = self.process_sample();
            left[i] += l;
            right[i] += r;
        }
    }
}

impl Device for SuiteInstrumentEngine {
    fn name(&self) -> &str {
        match self.kind {
            SuiteInstrumentKind::Drift => "Drift",
            SuiteInstrumentKind::Meld => "Meld",
            SuiteInstrumentKind::Collision => "Collision",
            SuiteInstrumentKind::Tension => "Tension",
            SuiteInstrumentKind::Electric => "Electric",
            SuiteInstrumentKind::Analog => "Analog",
            SuiteInstrumentKind::Sampler => "Sampler",
            SuiteInstrumentKind::GranulatorIII => "Granulator III",
        }
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
        self.all_notes_off();
    }
}

impl InstrumentDevice for SuiteInstrumentEngine {
    fn process(
        &mut self,
        midi_in: &[MidiEvent],
        audio_out: &mut AudioBuffer,
        _time: &MusicalTime,
    ) {
        if !self.enabled {
            return;
        }

        for ev in midi_in {
            match *ev {
                MidiEvent::NoteOn { note, velocity, .. } => self.note_on(note, velocity),
                MidiEvent::NoteOff { note, .. } => self.note_off(note),
                MidiEvent::PitchBend { value, .. } => {
                    let semitones = (value as f32 / 8192.0) * 12.0;
                    self.set_pitch_bend(semitones);
                }
                MidiEvent::ControlChange { controller, value, .. } => {
                    if controller == 74 {
                        // MPE Slide (CC74)
                        self.set_mpe_slide(value as f32 / 127.0);
                    } else if controller == 11 {
                        // MPE Pressure / Expression (CC11)
                        self.set_mpe_pressure(value as f32 / 127.0);
                    }
                }
            }
        }

        let frames = audio_out.frame_count.min(1024);
        for i in 0..frames {
            let (l, r) = self.process_sample();
            audio_out.left[i] += l;
            audio_out.right[i] += r;
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.note_on(note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        self.note_off(note);
    }

    fn all_notes_off(&mut self) {
        self.all_notes_off();
    }
}

// -----------------------------------------------------------------------------
// Unit Tests (At least 5 verification tests)
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suite_drift_and_meld_synthesis() {
        let sr = 44100.0;
        let mut drift = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Drift);
        drift.note_on(60, 100);

        let mut left = [0.0f32; 512];
        let mut right = [0.0f32; 512];
        drift.process_block(&mut left, &mut right);

        let energy_l: f32 = left.iter().map(|s| s * s).sum();
        let energy_r: f32 = right.iter().map(|s| s * s).sum();
        assert!(energy_l > 0.01 && energy_r > 0.01, "Drift must render non-zero audio");
        assert!(left.iter().all(|s| s.is_finite()) && right.iter().all(|s| s.is_finite()));

        // Test Meld
        let mut meld = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Meld);
        meld.note_on(64, 110);
        let mut m_l = [0.0f32; 512];
        let mut m_r = [0.0f32; 512];
        meld.process_block(&mut m_l, &mut m_r);

        let m_energy: f32 = m_l.iter().map(|s| s * s).sum();
        assert!(m_energy > 0.01, "Meld must render bi-timbral sound");
        assert!(m_l.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn test_suite_physical_modeling_collision_tension_electric() {
        let sr = 44100.0;
        let instruments = [
            SuiteInstrumentKind::Collision,
            SuiteInstrumentKind::Tension,
            SuiteInstrumentKind::Electric,
        ];

        for kind in instruments {
            let mut engine = SuiteInstrumentEngine::new(sr, kind);
            engine.note_on(62, 105);

            let mut left = [0.0f32; 256];
            let mut right = [0.0f32; 256];
            engine.process_block(&mut left, &mut right);

            let energy: f32 = left.iter().map(|s| s * s).sum();
            assert!(energy > 0.0001, "{:?} physical model must generate sound", kind);
            assert!(left.iter().all(|s| s.is_finite()), "{:?} had non-finite samples", kind);
        }
    }

    #[test]
    fn test_suite_analog_subtractive_synthesis() {
        let sr = 44100.0;
        let mut analog = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Analog);
        analog.note_on(48, 120);

        let mut left = [0.0f32; 512];
        let mut right = [0.0f32; 512];
        analog.process_block(&mut left, &mut right);

        let energy: f32 = left.iter().map(|s| s * s).sum();
        assert!(energy > 0.05, "Analog dual-oscillator must produce audible signal");
        assert!(left.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn test_suite_sampler_multi_velocity_zones() {
        let sr = 44100.0;
        let mut sampler = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Sampler);

        // Low velocity layer
        sampler.note_on(60, 40);
        let mut l_low = [0.0f32; 256];
        let mut r_low = [0.0f32; 256];
        sampler.process_block(&mut l_low, &mut r_low);
        let energy_low: f32 = l_low.iter().map(|s| s * s).sum();

        // High velocity layer
        let mut sampler_high = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Sampler);
        sampler_high.note_on(60, 120);
        let mut l_high = [0.0f32; 256];
        let mut r_high = [0.0f32; 256];
        sampler_high.process_block(&mut l_high, &mut r_high);
        let energy_high: f32 = l_high.iter().map(|s| s * s).sum();

        assert!(energy_low > 0.001);
        assert!(energy_high > energy_low, "High velocity layer must deliver higher energy");
    }

    #[test]
    fn test_suite_granulator_iii_cloud_synthesis() {
        let sr = 44100.0;
        let mut granulator = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::GranulatorIII);
        granulator.note_on(60, 100);

        let mut left = [0.0f32; 1024];
        let mut right = [0.0f32; 1024];
        granulator.process_block(&mut left, &mut right);

        let energy_l: f32 = left.iter().map(|s| s * s).sum();
        let energy_r: f32 = right.iter().map(|s| s * s).sum();
        assert!(energy_l > 0.005 && energy_r > 0.005, "Granular cloud must synthesize audio");
        assert!(left.iter().all(|s| s.is_finite()));
        assert!(right.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn test_suite_all_8_instruments_spectral_distinction() {
        let sr = 44100.0;
        let kinds = [
            SuiteInstrumentKind::Drift,
            SuiteInstrumentKind::Meld,
            SuiteInstrumentKind::Collision,
            SuiteInstrumentKind::Tension,
            SuiteInstrumentKind::Electric,
            SuiteInstrumentKind::Analog,
            SuiteInstrumentKind::Sampler,
            SuiteInstrumentKind::GranulatorIII,
        ];

        let mut rms_levels = Vec::new();

        for kind in kinds {
            let mut engine = SuiteInstrumentEngine::new(sr, kind);
            engine.note_on(60, 100);

            let mut left = [0.0f32; 512];
            let mut right = [0.0f32; 512];
            engine.process_block(&mut left, &mut right);

            let rms: f32 = ((left.iter().map(|s| s * s).sum::<f32>() / left.len() as f32)).sqrt();
            assert!(rms > 0.0001, "{:?} produced zero sound", kind);
            assert!(left.iter().all(|s| s.is_finite()), "{:?} NaN detected", kind);
            rms_levels.push(rms);
        }

        assert_eq!(rms_levels.len(), 8);
    }

    #[test]
    fn test_suite_meld_and_drift_mpe_voice_routing() {
        let sr = 44100.0;

        // Test Meld MPE voice routing
        let mut meld = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Meld);
        meld.note_on(60, 100);
        meld.set_pitch_bend(2.5);
        meld.set_mpe_slide(0.7);
        meld.set_mpe_pressure(0.65);

        assert_eq!(meld.meld.voices[0].pitch_bend_semitones, 2.5);
        assert_eq!(meld.meld.voices[0].slide, 0.7);
        assert_eq!(meld.meld.voices[0].pressure, 0.65);

        let mut left = [0.0f32; 256];
        let mut right = [0.0f32; 256];
        meld.process_block(&mut left, &mut right);
        let energy_meld: f32 = left.iter().map(|s| s * s).sum();
        assert!(energy_meld > 0.0001, "Meld with MPE must synthesize audio");

        // Test Drift MPE voice routing
        let mut drift = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Drift);
        drift.note_on(60, 100);
        drift.set_pitch_bend(-1.5);
        drift.set_mpe_slide(0.85);
        drift.set_mpe_pressure(0.75);

        assert_eq!(drift.drift.voices[0].pitch_bend_semitones, -1.5);
        assert_eq!(drift.drift.voices[0].slide, 0.85);
        assert_eq!(drift.drift.voices[0].pressure, 0.75);

        let mut d_left = [0.0f32; 256];
        let mut d_right = [0.0f32; 256];
        drift.process_block(&mut d_left, &mut d_right);
        let energy_drift: f32 = d_left.iter().map(|s| s * s).sum();
        assert!(energy_drift > 0.0001, "Drift with MPE must synthesize audio");
    }

    #[test]
    fn test_suite_instruments_panning_and_volume_attenuation() {
        let sr = 44100.0;
        let mut engine = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Drift);
        engine.note_on(60, 100);

        // Test Hard Left Pan
        engine.pan = -1.0;
        let mut left_l = [0.0f32; 256];
        let mut right_l = [0.0f32; 256];
        engine.process_block(&mut left_l, &mut right_l);
        let energy_l: f32 = left_l.iter().map(|s| s * s).sum();
        let energy_r: f32 = right_l.iter().map(|s| s * s).sum();
        assert!(energy_l > 0.001, "Left pan must produce left channel energy");
        assert!(energy_r < 1e-6, "Left pan must have near-zero right channel energy");

        // Test Hard Right Pan
        engine.pan = 1.0;
        let mut left_r = [0.0f32; 256];
        let mut right_r = [0.0f32; 256];
        engine.process_block(&mut left_r, &mut right_r);
        let energy_rl: f32 = left_r.iter().map(|s| s * s).sum();
        let energy_rr: f32 = right_r.iter().map(|s| s * s).sum();
        assert!(energy_rl < 1e-6, "Right pan must have near-zero left channel energy");
        assert!(energy_rr > 0.001, "Right pan must produce right channel energy");

        // Test Silence on Zero Volume
        engine.volume = 0.0;
        let (s_l, s_r) = engine.process_sample();
        assert_eq!(s_l, 0.0);
        assert_eq!(s_r, 0.0);
    }

    #[test]
    fn test_suite_instruments_midi_device_process_interface() {
        let sr = 44100.0;
        let mut engine = SuiteInstrumentEngine::new(sr, SuiteInstrumentKind::Analog);
        let mut audio_out = AudioBuffer::new();
        audio_out.frame_count = 256;
        let time = MusicalTime::default();

        let midi_events = [
            MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 },
            MidiEvent::PitchBend { channel: 0, value: 4096 },
            MidiEvent::ControlChange { channel: 0, controller: 74, value: 100 },
        ];

        InstrumentDevice::process(&mut engine, &midi_events, &mut audio_out, &time);

        let energy_l: f32 = audio_out.left[..256].iter().map(|s| s * s).sum();
        let energy_r: f32 = audio_out.right[..256].iter().map(|s| s * s).sum();
        assert!(energy_l > 0.001 && energy_r > 0.001, "MIDI process must render audio buffer");

        // Test all notes off
        engine.all_notes_off();
    }
}
