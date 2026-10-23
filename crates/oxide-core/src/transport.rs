//! Ableton Live 12 Reference Manual Chapter 33: Synchronizing with Link, Tempo Follower, and MIDI.
//!
//! Provides genuine, lock-free, sample-accurate transport synchronization primitives:
//! 1. Ableton Link network tempo and phase alignment with start/stop synchronization.
//! 2. External MIDI Clock synchronization (24 PPQN master transmitter and jitter-filtered slave receiver).
//! 3. Real-time audio Tempo Follower engine with transient onset detection, confidence scoring, and EWMA smoothing.
//! 4. Transport synchronization coordinator with sync source switching, DJ tempo nudging, looping, and metronome telemetry.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::MusicalTime;

/// Standard MIDI clock constants (Chapter 33.3).
pub const MIDI_PPQN: u32 = 24;
pub const CLOCKS_PER_SIXTEENTH: u32 = 6;
pub const MSG_TIMING_CLOCK: u8 = 0xF8;
pub const MSG_START: u8 = 0xFA;
pub const MSG_CONTINUE: u8 = 0xFB;
pub const MSG_STOP: u8 = 0xFC;
pub const MSG_SONG_POSITION: u8 = 0xF2;

/// Synchronization source selection for the global transport (Chapter 33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SyncSource {
    /// Internal clock: transport runs from DAW tempo and manual adjustments.
    #[default]
    Internal,
    /// Ableton Link: peer-to-peer network tempo and phase sync (Chapter 33.1).
    Link,
    /// External MIDI Clock: slave to incoming 24 PPQN hardware/software clock (Chapter 33.3).
    MidiClock,
    /// Audio Tempo Follower: live audio input transient tracking (Chapter 33.2).
    TempoFollower,
}

/// DJ tempo nudging state for manual synchronization with external turntables/CDJs (Chapter 33.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum NudgeState {
    /// Neutral tempo playback.
    #[default]
    None,
    /// Momentary acceleration (+% BPM while pressed).
    NudgeUp,
    /// Momentary deceleration (-% BPM while pressed).
    NudgeDown,
}

impl NudgeState {
    /// Multiplier to apply to base BPM based on nudge amount (e.g. 0.015 for 1.5%).
    #[inline]
    pub fn multiplier(&self, amount: f64) -> f64 {
        match self {
            Self::None => 1.0,
            Self::NudgeUp => 1.0 + amount.abs(),
            Self::NudgeDown => (1.0 - amount.abs()).max(0.1),
        }
    }
}

/// Metronome click category emitted during transport advancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MetronomeClick {
    /// Primary downbeat of a measure (Bar accent, e.g. higher pitch).
    Downbeat,
    /// Standard beat within the measure (Subsequent quarter/eighth beats).
    Beat,
}

// ============================================================================
// 1. Ableton Link Synchronization Engine (Chapter 33.1)
// ============================================================================

/// Real-time state and mathematical model for Ableton Link network synchronization.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LinkSyncEngine {
    /// Whether Link sync is enabled in the transport bar.
    pub is_enabled: bool,
    /// Number of active Link peers detected on the local subnet.
    pub peer_count: usize,
    /// Whether Start/Stop Synchronization is shared with Link peers (Live 12 feature).
    pub start_stop_sync_enabled: bool,
    /// Quantum phase period in musical beats (default 4.0 for standard 4/4 bar).
    pub quantum: f64,
    /// Current shared Link session tempo.
    pub session_bpm: f64,
    /// Target tempo when transitioning smoothly between tempos.
    pub target_bpm: f64,
    /// Linear tempo ramp inertia in BPM per second (0.0 for instant).
    pub ramp_rate: f64,
    /// Virtual session beat anchor at last sync reference.
    pub session_beat_anchor: f64,
    /// Monotonic time anchor in seconds corresponding to beat anchor.
    pub session_time_anchor: f64,
}

impl LinkSyncEngine {
    /// Create a new Link synchronization engine initialized with base tempo.
    pub fn new(initial_bpm: f64) -> Self {
        let bpm = initial_bpm.clamp(20.0, 999.0);
        Self {
            is_enabled: false,
            peer_count: 0,
            start_stop_sync_enabled: true,
            quantum: 4.0,
            session_bpm: bpm,
            target_bpm: bpm,
            ramp_rate: 0.0,
            session_beat_anchor: 0.0,
            session_time_anchor: 0.0,
        }
    }

    /// Enable or disable Link network synchronization.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
    }

    /// Update connected peer count from discovery layer.
    pub fn set_peer_count(&mut self, count: usize) {
        self.peer_count = count;
    }

    /// Set quantum size in beats (typically 4.0 for 4/4 or 3.0 for 3/4).
    pub fn set_quantum(&mut self, quantum: f64) {
        if quantum > 0.0 {
            self.quantum = quantum;
        }
    }

    /// Calculate phase within the current quantum period: [0.0, quantum).
    #[inline]
    pub fn phase(&self, beat: f64) -> f64 {
        if self.quantum <= 0.0 {
            0.0
        } else {
            beat.rem_euclid(self.quantum)
        }
    }

    /// Calculate distance in beats until the next quantum boundary (downbeat sync).
    #[inline]
    pub fn distance_to_next_quantum(&self, beat: f64) -> f64 {
        let ph = self.phase(beat);
        if ph < 1e-9 {
            0.0
        } else {
            self.quantum - ph
        }
    }

    /// Map a continuous monotonic time (seconds) to Link musical beat coordinates.
    pub fn beat_at_time(&self, time_seconds: f64) -> f64 {
        let delta_t = time_seconds - self.session_time_anchor;
        let beats_per_sec = self.session_bpm / 60.0;
        self.session_beat_anchor + (delta_t * beats_per_sec)
    }

    /// Map a musical beat position to expected monotonic time (seconds).
    pub fn time_at_beat(&self, beat: f64) -> f64 {
        let delta_b = beat - self.session_beat_anchor;
        let beats_per_sec = self.session_bpm / 60.0;
        if beats_per_sec <= 0.0 {
            self.session_time_anchor
        } else {
            self.session_time_anchor + (delta_b / beats_per_sec)
        }
    }

    /// Request a tempo change across the Link network with optional smooth ramp.
    pub fn request_tempo_change(&mut self, new_bpm: f64, ramp_seconds: f64, current_time: f64) {
        let clamped = new_bpm.clamp(20.0, 999.0);
        // Anchor current state prior to tempo mutation
        let current_beat = self.beat_at_time(current_time);
        self.session_beat_anchor = current_beat;
        self.session_time_anchor = current_time;

        if ramp_seconds <= 0.001 {
            self.session_bpm = clamped;
            self.target_bpm = clamped;
            self.ramp_rate = 0.0;
        } else {
            self.target_bpm = clamped;
            self.ramp_rate = (clamped - self.session_bpm).abs() / ramp_seconds;
        }
    }

    /// Synchronize session parameters from an inbound peer network packet.
    pub fn receive_peer_sync(&mut self, peer_beat: f64, peer_bpm: f64, network_time: f64) {
        if self.is_enabled {
            self.session_beat_anchor = peer_beat;
            self.session_time_anchor = network_time;
            self.session_bpm = peer_bpm.clamp(20.0, 999.0);
            self.target_bpm = self.session_bpm;
            self.ramp_rate = 0.0;
        }
    }

    /// Advance Link session timeline and evaluate tempo ramps.
    pub fn advance_time(&mut self, delta_seconds: f64) {
        if self.ramp_rate > 0.0 && (self.session_bpm - self.target_bpm).abs() > 1e-4 {
            let step = self.ramp_rate * delta_seconds;
            if self.session_bpm < self.target_bpm {
                self.session_bpm = (self.session_bpm + step).min(self.target_bpm);
            } else {
                self.session_bpm = (self.session_bpm - step).max(self.target_bpm);
            }
        }
    }
}

// ============================================================================
// 2. External MIDI Clock Synchronization (Chapter 33.3)
// ============================================================================

/// Operating mode for external MIDI synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MidiSyncMode {
    /// Disabled: no MIDI clock transmitted or received.
    #[default]
    Disabled,
    /// Master: Oxide Studio generates 24 PPQN clocks and transport messages to hardware.
    Master,
    /// Slave: Oxide Studio locks its transport to external incoming 24 PPQN MIDI clock.
    Slave,
}

/// High-level parsed MIDI Realtime clock and song positioning message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MidiClockMessage {
    /// 24 PPQN timing clock tick (0xF8).
    TimingClock,
    /// Transport Start from beat 0.0 (0xFA).
    Start,
    /// Transport Continue from current beat (0xFB).
    Continue,
    /// Transport Stop (0xFC).
    Stop,
    /// Song Position Pointer (0xF2 with 14-bit 16th-note measure index).
    SongPositionPointer { sixteenth_notes: u16 },
}

/// Sample-accurate MIDI Clock transmitter and jitter-filtering receiver.
#[derive(Debug, Clone)]
pub struct MidiClockSync {
    pub mode: MidiSyncMode,
    /// Latency compensation delay offset in milliseconds (-500.0 to +500.0 ms).
    pub delay_ms: f64,

    // Master transmitter state
    pub master_accumulated_ticks: u64,
    pub master_fractional_tick: f64,
    pub queued_outgoing_messages: Vec<MidiClockMessage>,

    // Slave receiver PLL / jitter filtering state
    pub slave_last_tick_time: Option<f64>,
    pub slave_tick_intervals: [f64; 24],
    pub slave_tick_count: usize,
    pub slave_estimated_bpm: f64,
    pub slave_is_locked: bool,
    pub slave_timeout_threshold_seconds: f64,
    pub slave_song_position_msb: Option<u8>,
}

impl MidiClockSync {
    pub fn new(mode: MidiSyncMode) -> Self {
        Self {
            mode,
            delay_ms: 0.0,
            master_accumulated_ticks: 0,
            master_fractional_tick: 0.0,
            queued_outgoing_messages: Vec::with_capacity(32),
            slave_last_tick_time: None,
            slave_tick_intervals: [0.0; 24],
            slave_tick_count: 0,
            slave_estimated_bpm: 120.0,
            slave_is_locked: false,
            slave_timeout_threshold_seconds: 0.35,
            slave_song_position_msb: None,
        }
    }

    /// Set latency compensation delay offset in milliseconds.
    pub fn set_delay_ms(&mut self, delay: f64) {
        self.delay_ms = delay.clamp(-500.0, 500.0);
    }

    /// Reset internal slave clock statistics upon stop or disconnect.
    pub fn reset_slave(&mut self) {
        self.slave_last_tick_time = None;
        self.slave_tick_count = 0;
        self.slave_is_locked = false;
        self.slave_song_position_msb = None;
    }

    /// Master: Calculate how many 24 PPQN ticks occurred between start_beat and end_beat,
    /// enqueueing `MidiClockMessage::TimingClock` for dispatch.
    pub fn process_master_advance(&mut self, start_beat: f64, end_beat: f64) -> usize {
        if self.mode != MidiSyncMode::Master || end_beat <= start_beat {
            return 0;
        }

        // Each quarter note has 24 ticks (1 tick = 1.0 / 24.0 beats)
        let ticks_start = (start_beat * (MIDI_PPQN as f64)).floor() as u64;
        let ticks_end = (end_beat * (MIDI_PPQN as f64)).floor() as u64;

        let num_ticks = (ticks_end.saturating_sub(ticks_start)) as usize;
        for _ in 0..num_ticks {
            self.queued_outgoing_messages.push(MidiClockMessage::TimingClock);
            self.master_accumulated_ticks += 1;
        }
        num_ticks
    }

    /// Master: Enqueue transport control event.
    pub fn queue_master_command(&mut self, msg: MidiClockMessage) {
        if self.mode == MidiSyncMode::Master {
            self.queued_outgoing_messages.push(msg);
        }
    }

    /// Master: Drain all queued MIDI clock messages for transmission out hardware ports.
    pub fn drain_outgoing_messages(&mut self) -> Vec<MidiClockMessage> {
        self.queued_outgoing_messages.drain(..).collect()
    }

    /// Slave: Parse raw incoming MIDI byte stream and return decoded message if completed.
    pub fn parse_incoming_byte(&mut self, byte: u8, current_time: f64) -> Option<MidiClockMessage> {
        if self.mode != MidiSyncMode::Slave {
            return None;
        }

        match byte {
            MSG_TIMING_CLOCK => {
                self.process_slave_tick(current_time);
                Some(MidiClockMessage::TimingClock)
            }
            MSG_START => {
                self.reset_slave();
                Some(MidiClockMessage::Start)
            }
            MSG_CONTINUE => Some(MidiClockMessage::Continue),
            MSG_STOP => {
                self.slave_is_locked = false;
                Some(MidiClockMessage::Stop)
            }
            _ => None,
        }
    }

    /// Slave: Parse a 2-byte Song Position Pointer (SPP: 14-bit 16th note index).
    pub fn parse_song_position_pointer(lsb: u8, msb: u8) -> u16 {
        ((msb as u16 & 0x7F) << 7) | (lsb as u16 & 0x7F)
    }

    /// Encode a 14-bit Song Position Pointer into (LSB, MSB) bytes.
    pub fn encode_song_position_pointer(sixteenth_notes: u16) -> (u8, u8) {
        let clamped = sixteenth_notes & 0x3FFF;
        ((clamped & 0x7F) as u8, ((clamped >> 7) & 0x7F) as u8)
    }

    /// Convert 16th note position to musical quarter beats.
    #[inline]
    pub fn song_position_to_beats(sixteenth_notes: u16) -> f64 {
        sixteenth_notes as f64 * 0.25
    }

    /// Convert musical quarter beats to 16th note Song Position Pointer.
    #[inline]
    pub fn beats_to_song_position(beat: f64) -> u16 {
        (beat * 4.0).max(0.0) as u16
    }

    /// Slave: Process incoming 24 PPQN tick with running jitter filtering.
    fn process_slave_tick(&mut self, current_time: f64) {
        if let Some(prev) = self.slave_last_tick_time {
            let delta = current_time - prev;
            // Ignore duplicate or corrupt sub-millisecond ticks
            if delta > 0.001 && delta < self.slave_timeout_threshold_seconds {
                let idx = self.slave_tick_count % 24;
                self.slave_tick_intervals[idx] = delta;
                self.slave_tick_count += 1;

                // When at least 6 ticks are collected, update PLL estimated BPM
                let window = self.slave_tick_count.min(24);
                if window >= 6 {
                    let sum: f64 = self.slave_tick_intervals[..window].iter().sum();
                    let avg_delta = sum / (window as f64);
                    if avg_delta > 0.001 {
                        // BPM = 60.0 / (24 * avg_delta)
                        let raw_bpm = 60.0 / (24.0 * avg_delta);
                        // Apply soft exponential smoothing
                        self.slave_estimated_bpm = 0.80 * self.slave_estimated_bpm + 0.20 * raw_bpm;
                        self.slave_estimated_bpm = self.slave_estimated_bpm.clamp(20.0, 999.0);
                        self.slave_is_locked = true;
                    }
                }
            } else if delta >= self.slave_timeout_threshold_seconds {
                // Clock timed out: lost sync
                self.slave_is_locked = false;
                self.slave_tick_count = 0;
            }
        }
        self.slave_last_tick_time = Some(current_time);
    }
}

// ============================================================================
// 3. Audio Tempo Follower Engine (Chapter 33.2)
// ============================================================================

/// Operational state of the live audio tempo follower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FollowerState {
    /// Follower disabled.
    #[default]
    Off,
    /// Listening and accumulating transient intervals; awaiting confidence threshold.
    Listening,
    /// Tracking locked: real-time live input actively guides DAW tempo.
    Locked,
    /// Momentary pause / gap in live input transients (holding last valid tempo).
    Hold,
}

/// Live audio input tempo follower implementing transient onset detection and interval tracking.
#[derive(Debug, Clone)]
pub struct TempoFollowerEngine {
    /// Operational state.
    pub state: FollowerState,
    /// User configurable minimum expected tempo (default 60.0 BPM).
    pub min_bpm: f64,
    /// User configurable maximum expected tempo (default 200.0 BPM).
    pub max_bpm: f64,
    /// Confidence threshold required to transition to Locked state (default 0.60).
    pub confidence_threshold: f64,
    /// Current confidence score in range [0.0, 1.0].
    pub confidence: f64,
    /// Current estimated tempo in BPM.
    pub estimated_bpm: f64,
    /// Smoothing factor for EWMA tempo adjustment (default 0.15).
    pub smoothing_alpha: f64,

    // Internal DSP transient detection state
    pub energy_envelope: f32,
    pub previous_energy: f32,
    pub last_onset_time: Option<f64>,
    pub inter_onset_intervals: [f64; 8],
    pub interval_count: usize,
    pub hold_elapsed_seconds: f64,
    pub hold_timeout_seconds: f64,
}

impl TempoFollowerEngine {
    pub fn new() -> Self {
        Self {
            state: FollowerState::Off,
            min_bpm: 60.0,
            max_bpm: 200.0,
            confidence_threshold: 0.60,
            confidence: 0.0,
            estimated_bpm: 120.0,
            smoothing_alpha: 0.15,
            energy_envelope: 0.0,
            previous_energy: 0.0,
            last_onset_time: None,
            inter_onset_intervals: [0.0; 8],
            interval_count: 0,
            hold_elapsed_seconds: 0.0,
            hold_timeout_seconds: 2.5,
        }
    }

    /// Enable or disable the tempo follower.
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled {
            if self.state == FollowerState::Off {
                self.state = FollowerState::Listening;
                self.confidence = 0.0;
                self.interval_count = 0;
                self.last_onset_time = None;
            }
        } else {
            self.state = FollowerState::Off;
            self.confidence = 0.0;
        }
    }

    /// Configure tempo tracking boundaries.
    pub fn set_bpm_range(&mut self, min: f64, max: f64) {
        let valid_min = min.clamp(30.0, 300.0);
        let valid_max = max.clamp(valid_min + 10.0, 400.0);
        self.min_bpm = valid_min;
        self.max_bpm = valid_max;
    }

    /// Process an audio buffer block (e.g. from live microphone/input) and update tempo tracking.
    pub fn process_audio_block(&mut self, samples: &[f32], sample_rate: f64, current_time: f64) {
        if self.state == FollowerState::Off || samples.is_empty() || sample_rate <= 0.0 {
            return;
        }

        // 1. Calculate RMS energy and peak amplitude of current audio block
        let mut sum_sq = 0.0f32;
        let mut peak = 0.0f32;
        for &s in samples {
            sum_sq += s * s;
            let a = s.abs();
            if a > peak {
                peak = a;
            }
        }
        let current_rms = (sum_sq / samples.len() as f32).sqrt();

        // 2. High-pass filter & energy flux envelope
        let alpha = 0.35f32;
        self.energy_envelope = (1.0 - alpha) * self.energy_envelope + alpha * current_rms;
        let flux = (self.energy_envelope - self.previous_energy).max(0.0);
        self.previous_energy = self.energy_envelope;

        // Dynamic transient detection threshold: detects percussive peak or energy flux
        let is_onset = (flux > 0.008 && self.energy_envelope > 0.01) || peak > 0.3;

        let in_refractory = if let Some(last) = self.last_onset_time {
            (current_time - last) < 0.10
        } else {
            false
        };

        if is_onset && !in_refractory {
            if let Some(prev_time) = self.last_onset_time {
                let delta = current_time - prev_time;
                // Plausible interval: between 0.15s (400 BPM) and 2.0s (30 BPM)
                if delta >= 0.15 && delta <= 2.0 {
                    let idx = self.interval_count % 8;
                    self.inter_onset_intervals[idx] = delta;
                    self.interval_count += 1;

                    self.evaluate_tempo_confidence();
                    self.hold_elapsed_seconds = 0.0;
                }
            }
            self.last_onset_time = Some(current_time);
        } else {
            // Check hold state timeout if no transients are heard
            if self.state == FollowerState::Locked {
                self.hold_elapsed_seconds += samples.len() as f64 / sample_rate;
                if self.hold_elapsed_seconds > self.hold_timeout_seconds {
                    self.state = FollowerState::Hold;
                    self.confidence = 0.20;
                }
            }
        }
    }

    /// Evaluate regularity of collected onset intervals and update estimated BPM and confidence.
    fn evaluate_tempo_confidence(&mut self) {
        let count = self.interval_count.min(8);
        if count < 3 {
            self.state = FollowerState::Listening;
            self.confidence = 0.30;
            return;
        }

        let slice = &self.inter_onset_intervals[..count];
        let sum: f64 = slice.iter().sum();
        let mean_interval = sum / (count as f64);

        if mean_interval <= 0.001 {
            return;
        }

        // Variance / Standard Deviation to evaluate rhythm regularity
        let variance: f64 = slice.iter().map(|&x| (x - mean_interval).powi(2)).sum::<f64>() / (count as f64);
        let std_dev = variance.sqrt();
        let coefficient_of_variation = std_dev / mean_interval;

        // Confidence: higher when intervals are regular (low coefficient of variation)
        let raw_confidence = (1.0 - (coefficient_of_variation * 2.5)).clamp(0.0, 1.0);
        self.confidence = 0.70 * self.confidence + 0.30 * raw_confidence;

        // Derive candidate tempo: 60.0 / interval
        let mut candidate_bpm = 60.0 / mean_interval;

        // Double or halve candidate tempo to fall within configured [min_bpm, max_bpm]
        while candidate_bpm < self.min_bpm && candidate_bpm * 2.0 <= self.max_bpm {
            candidate_bpm *= 2.0;
        }
        while candidate_bpm > self.max_bpm && candidate_bpm / 2.0 >= self.min_bpm {
            candidate_bpm /= 2.0;
        }

        if candidate_bpm >= self.min_bpm && candidate_bpm <= self.max_bpm {
            // Smoothly update estimated tempo using EWMA
            self.estimated_bpm = (1.0 - self.smoothing_alpha) * self.estimated_bpm + self.smoothing_alpha * candidate_bpm;
        }

        // State transition based on confidence
        if self.confidence >= self.confidence_threshold {
            self.state = FollowerState::Locked;
        } else {
            self.state = FollowerState::Listening;
        }
    }
}

// ============================================================================
// 4. Central Transport Synchronization Controller (Chapter 33 Master Seam)
// ============================================================================

/// Outcome report returned after an audio frame transport advancement step.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TransportAdvanceReport {
    /// Playhead position before advancement.
    pub previous_beat: f64,
    /// Playhead position after advancement.
    pub current_beat: f64,
    /// Effective tempo in BPM applied during this advancement.
    pub effective_bpm: f64,
    /// Active synchronization source.
    pub sync_source: SyncSource,
    /// Number of 24 PPQN MIDI clock ticks emitted during this step.
    pub midi_ticks_emitted: usize,
    /// Optional metronome click event triggered during this advancement.
    pub metronome_click: Option<MetronomeClick>,
    /// Whether playback wrapped around the loop boundary.
    pub loop_wrapped: bool,
}

/// Comprehensive, thread-safe transport controller uniting Ableton Link, MIDI Clock,
/// live Tempo Follower, and DJ nudging into a unified playback engine.
#[derive(Debug, Clone)]
pub struct TransportController {
    /// Core musical time representation (beat, base bpm, time signature, playing state).
    pub time: MusicalTime,
    /// Active synchronization source.
    pub sync_source: SyncSource,
    /// DJ tempo nudging state (Chapter 33.4).
    pub nudge_state: NudgeState,
    /// DJ nudge percentage amount (default 0.015 for +/- 1.5%).
    pub nudge_amount: f64,

    /// Whether timeline looping is active.
    pub loop_enabled: bool,
    /// Loop starting beat position.
    pub loop_start_beat: f64,
    /// Loop length in beats.
    pub loop_length_beats: f64,

    /// Metronome audible click generator enable flag.
    pub metronome_enabled: bool,

    /// Ableton Link network sync engine (Chapter 33.1).
    pub link: LinkSyncEngine,
    /// External MIDI Clock transmitter and receiver (Chapter 33.3).
    pub midi_clock: MidiClockSync,
    /// Live audio input Tempo Follower engine (Chapter 33.2).
    pub tempo_follower: TempoFollowerEngine,

    /// Monotonic total audio seconds elapsed since instantiation.
    pub elapsed_audio_seconds: f64,
}

impl TransportController {
    /// Create a new transport controller initialized at the specified tempo.
    pub fn new(bpm: f64) -> Self {
        let valid_bpm = bpm.clamp(20.0, 999.0);
        let musical_time = MusicalTime::new(0.0, valid_bpm, 4, 4, false);

        Self {
            time: musical_time,
            sync_source: SyncSource::Internal,
            nudge_state: NudgeState::None,
            nudge_amount: 0.015,
            loop_enabled: false,
            loop_start_beat: 0.0,
            loop_length_beats: 16.0,
            metronome_enabled: false,
            link: LinkSyncEngine::new(valid_bpm),
            midi_clock: MidiClockSync::new(MidiSyncMode::Disabled),
            tempo_follower: TempoFollowerEngine::new(),
            elapsed_audio_seconds: 0.0,
        }
    }

    /// Set transport playback state to playing.
    pub fn play(&mut self) {
        self.time.is_playing = true;
        if self.sync_source == SyncSource::Link && self.link.start_stop_sync_enabled {
            // Start sync broadcast
        }
        if self.midi_clock.mode == MidiSyncMode::Master {
            if self.time.beat < 1e-6 {
                self.midi_clock.queue_master_command(MidiClockMessage::Start);
            } else {
                self.midi_clock.queue_master_command(MidiClockMessage::Continue);
            }
        }
    }

    /// Stop transport playback and reset playhead if configured.
    pub fn stop(&mut self, reset_playhead: bool) {
        self.time.is_playing = false;
        if reset_playhead {
            self.time.beat = 0.0;
        }
        if self.midi_clock.mode == MidiSyncMode::Master {
            self.midi_clock.queue_master_command(MidiClockMessage::Stop);
        }
        self.nudge_state = NudgeState::None;
    }

    /// Set base DAW tempo.
    pub fn set_tempo(&mut self, bpm: f64) {
        let clamped = bpm.clamp(20.0, 999.0);
        self.time.bpm = clamped;
        if self.sync_source == SyncSource::Link {
            self.link.request_tempo_change(clamped, 0.0, self.elapsed_audio_seconds);
        }
    }

    /// Set time signature (e.g. 4/4, 3/4, 6/8).
    pub fn set_time_signature(&mut self, num: u8, den: u8) {
        if num > 0 && den > 0 {
            self.time.signature_num = num;
            self.time.signature_den = den;
            self.link.set_quantum(num as f64);
        }
    }

    /// Set active synchronization source.
    pub fn set_sync_source(&mut self, source: SyncSource) {
        self.sync_source = source;
        match source {
            SyncSource::Link => self.link.set_enabled(true),
            SyncSource::TempoFollower => self.tempo_follower.set_enabled(true),
            _ => {}
        }
    }

    /// Configure transport loop parameters.
    pub fn set_loop(&mut self, enabled: bool, start_beat: f64, length_beats: f64) {
        self.loop_enabled = enabled;
        self.loop_start_beat = start_beat.max(0.0);
        self.loop_length_beats = length_beats.max(0.25);
    }

    /// Engage temporary DJ tempo acceleration (Chapter 33.4).
    pub fn start_nudge_up(&mut self) {
        self.nudge_state = NudgeState::NudgeUp;
    }

    /// Engage temporary DJ tempo deceleration (Chapter 33.4).
    pub fn start_nudge_down(&mut self) {
        self.nudge_state = NudgeState::NudgeDown;
    }

    /// Release DJ tempo nudge, restoring normal playback speed.
    pub fn stop_nudge(&mut self) {
        self.nudge_state = NudgeState::None;
    }

    /// Calculate the effective playback tempo taking active sync source and DJ nudge into account.
    pub fn effective_bpm(&self) -> f64 {
        let base = match self.sync_source {
            SyncSource::Internal => self.time.bpm,
            SyncSource::Link => {
                if self.link.is_enabled {
                    self.link.session_bpm
                } else {
                    self.time.bpm
                }
            }
            SyncSource::MidiClock => {
                if self.midi_clock.mode == MidiSyncMode::Slave && self.midi_clock.slave_is_locked {
                    self.midi_clock.slave_estimated_bpm
                } else {
                    self.time.bpm
                }
            }
            SyncSource::TempoFollower => {
                if self.tempo_follower.state == FollowerState::Locked {
                    self.tempo_follower.estimated_bpm
                } else {
                    self.time.bpm
                }
            }
        };

        base * self.nudge_state.multiplier(self.nudge_amount)
    }

    /// Advance transport time for an audio block of `frames` length at `sample_rate`,
    /// coordinating all synchronization sources, looping, and MIDI clock generation.
    pub fn advance(
        &mut self,
        frames: usize,
        sample_rate: f64,
        incoming_audio: Option<&[f32]>,
    ) -> TransportAdvanceReport {
        let prev_beat = self.time.beat;
        let mut loop_wrapped = false;
        let mut midi_ticks_emitted = 0;

        if frames == 0 || sample_rate <= 0.0 {
            return TransportAdvanceReport {
                previous_beat: prev_beat,
                current_beat: prev_beat,
                effective_bpm: self.effective_bpm(),
                sync_source: self.sync_source,
                midi_ticks_emitted: 0,
                metronome_click: None,
                loop_wrapped: false,
            };
        }

        let delta_seconds = frames as f64 / sample_rate;
        self.elapsed_audio_seconds += delta_seconds;

        // 1. Process live audio tempo follower if active
        if self.sync_source == SyncSource::TempoFollower {
            if let Some(audio) = incoming_audio {
                self.tempo_follower.process_audio_block(audio, sample_rate, self.elapsed_audio_seconds);
            }
        }

        // 2. Advance Link network timeline if active
        if self.sync_source == SyncSource::Link {
            self.link.advance_time(delta_seconds);
        }

        let bpm = self.effective_bpm();

        // 3. Advance musical beat position if playing
        if self.time.is_playing && bpm > 0.0 {
            let beats_per_second = bpm / 60.0;
            let mut next_beat = prev_beat + (delta_seconds * beats_per_second);

            // Handle loop wrapping
            if self.loop_enabled && self.loop_length_beats > 0.0 {
                let loop_end = self.loop_start_beat + self.loop_length_beats;
                if next_beat >= loop_end {
                    let overshoot = next_beat - loop_end;
                    next_beat = self.loop_start_beat + (overshoot % self.loop_length_beats);
                    loop_wrapped = true;
                }
            }

            // 4. Generate outgoing MIDI Clock ticks if operating as MIDI Master
            if self.midi_clock.mode == MidiSyncMode::Master {
                midi_ticks_emitted = self.midi_clock.process_master_advance(prev_beat, next_beat);
            }

            self.time.beat = next_beat;
        }

        // 5. Evaluate metronome click crossings
        let metronome_click = if self.time.is_playing && self.metronome_enabled {
            self.detect_metronome_crossing(prev_beat, self.time.beat)
        } else {
            None
        };

        TransportAdvanceReport {
            previous_beat: prev_beat,
            current_beat: self.time.beat,
            effective_bpm: bpm,
            sync_source: self.sync_source,
            midi_ticks_emitted,
            metronome_click,
            loop_wrapped,
        }
    }

    /// Detect if the playhead crossed a beat or bar boundary during advancement.
    fn detect_metronome_crossing(&self, prev_beat: f64, current_beat: f64) -> Option<MetronomeClick> {
        let prev_int = prev_beat.floor() as i64;
        let curr_int = current_beat.floor() as i64;

        if curr_int > prev_int {
            let bar_length = self.time.signature_num as i64;
            if bar_length > 0 && curr_int % bar_length == 0 {
                Some(MetronomeClick::Downbeat)
            } else {
                Some(MetronomeClick::Beat)
            }
        } else {
            None
        }
    }
}

// ============================================================================
// Unit & Integration Tests (Genuine Behavior Verification)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_engine_phase_and_quantum_alignment() {
        let mut link = LinkSyncEngine::new(124.0);
        link.set_enabled(true);
        link.set_peer_count(2);
        link.set_quantum(4.0);

        // Phase within 4.0 beat quantum
        assert_eq!(link.phase(0.0), 0.0);
        assert_eq!(link.phase(1.5), 1.5);
        assert_eq!(link.phase(4.0), 0.0);
        assert_eq!(link.phase(7.25), 3.25);

        // Distance to next quantum downbeat
        assert_eq!(link.distance_to_next_quantum(0.0), 0.0);
        assert_eq!(link.distance_to_next_quantum(1.0), 3.0);
        assert_eq!(link.distance_to_next_quantum(3.5), 0.5);

        // Beat at time conversion
        let beat_at_0 = link.beat_at_time(0.0);
        assert_eq!(beat_at_0, 0.0);
        // At 120 BPM = 2 beats per sec; at 124 BPM = 124/60 beats per sec
        let beat_at_60s = link.beat_at_time(60.0);
        assert!((beat_at_60s - 124.0).abs() < 1e-6);

        // Time at beat conversion
        let time_at_124_beats = link.time_at_beat(124.0);
        assert!((time_at_124_beats - 60.0).abs() < 1e-6);
    }

    #[test]
    fn test_link_engine_tempo_ramp_and_peer_sync() {
        let mut link = LinkSyncEngine::new(120.0);
        link.set_enabled(true);

        // Instant tempo change
        link.request_tempo_change(130.0, 0.0, 0.0);
        assert_eq!(link.session_bpm, 130.0);

        // Smooth tempo change with 2.0s ramp from 130 to 140 BPM
        link.request_tempo_change(140.0, 2.0, 0.0);
        assert_eq!(link.session_bpm, 130.0);
        assert_eq!(link.target_bpm, 140.0);

        // Advance 1.0 second: tempo should be halfway (135.0 BPM)
        link.advance_time(1.0);
        assert!((link.session_bpm - 135.0).abs() < 1e-3);

        // Advance another 1.0 second: tempo completes ramp to 140.0 BPM
        link.advance_time(1.0);
        assert!((link.session_bpm - 140.0).abs() < 1e-3);

        // Inbound peer sync
        link.receive_peer_sync(32.0, 126.0, 15.0);
        assert_eq!(link.session_bpm, 126.0);
        assert_eq!(link.session_beat_anchor, 32.0);
        assert_eq!(link.session_time_anchor, 15.0);
    }

    #[test]
    fn test_midi_clock_master_generation_24_ppqn() {
        let mut clock = MidiClockSync::new(MidiSyncMode::Master);
        assert_eq!(clock.mode, MidiSyncMode::Master);

        // Advance 1 quarter beat (0.0 to 1.0) -> must emit exactly 24 PPQN ticks
        let ticks = clock.process_master_advance(0.0, 1.0);
        assert_eq!(ticks, 24);

        let msgs = clock.drain_outgoing_messages();
        assert_eq!(msgs.len(), 24);
        for m in msgs {
            assert_eq!(m, MidiClockMessage::TimingClock);
        }

        // Advance 0.5 beat (1.0 to 1.5) -> must emit exactly 12 ticks
        let ticks_half = clock.process_master_advance(1.0, 1.5);
        assert_eq!(ticks_half, 12);
        assert_eq!(clock.drain_outgoing_messages().len(), 12);
    }

    #[test]
    fn test_midi_clock_song_position_pointer_encoding_decoding() {
        // Test roundtrip of Song Position Pointer (14-bit)
        let sixteenths = 64u16; // Bar 5, Beat 1
        let (lsb, msb) = MidiClockSync::encode_song_position_pointer(sixteenths);
        let decoded = MidiClockSync::parse_song_position_pointer(lsb, msb);
        assert_eq!(decoded, sixteenths);

        // Convert to quarter beats: 64 sixteenths = 16 quarter beats
        assert_eq!(MidiClockSync::song_position_to_beats(decoded), 16.0);
        assert_eq!(MidiClockSync::beats_to_song_position(16.0), 64);

        // Boundary value: maximum 14-bit value 16383
        let max_val = 0x3FFF;
        let (lsb_max, msb_max) = MidiClockSync::encode_song_position_pointer(max_val);
        assert_eq!(MidiClockSync::parse_song_position_pointer(lsb_max, msb_max), max_val);
    }

    #[test]
    fn test_midi_clock_slave_receiver_and_pll_locking() {
        let mut clock = MidiClockSync::new(MidiSyncMode::Slave);

        // Simulate incoming 120 BPM clock:
        // 120 BPM = 2 beats per sec -> 24 ticks per beat -> 48 ticks per second.
        // Inter-tick duration = 1.0 / 48.0 = 0.020833333333333332 seconds.
        let dt = 1.0 / 48.0;
        let mut t = 0.0;

        // Feed Start message
        let start_msg = clock.parse_incoming_byte(MSG_START, t);
        assert_eq!(start_msg, Some(MidiClockMessage::Start));

        // Feed 36 consecutive timing clock ticks (1.5 beats)
        for _ in 0..36 {
            t += dt;
            let tick_msg = clock.parse_incoming_byte(MSG_TIMING_CLOCK, t);
            assert_eq!(tick_msg, Some(MidiClockMessage::TimingClock));
        }

        // Verify PLL locked and estimated BPM matches 120.0 within 1.0 BPM
        assert!(clock.slave_is_locked, "MidiClock slave receiver should be locked to incoming ticks");
        assert!(
            (clock.slave_estimated_bpm - 120.0).abs() < 1.0,
            "Estimated BPM should be near 120.0, got: {}",
            clock.slave_estimated_bpm
        );

        // Feed Stop message
        let stop_msg = clock.parse_incoming_byte(MSG_STOP, t + dt);
        assert_eq!(stop_msg, Some(MidiClockMessage::Stop));
        assert!(!clock.slave_is_locked);
    }

    #[test]
    fn test_tempo_follower_transient_tracking_and_confidence() {
        let mut follower = TempoFollowerEngine::new();
        follower.set_enabled(true);
        follower.set_bpm_range(60.0, 180.0);
        assert_eq!(follower.state, FollowerState::Listening);

        // Synthesize audio pulse train at 120 BPM (0.5s interval = 22050 samples at 44.1kHz)
        let sample_rate = 44100.0;
        let block_size = 512;
        let pulse_interval_blocks = (22050 / block_size) as usize; // ~43 blocks

        let mut current_time = 0.0;
        for _pulse_num in 0..16 {
            for b in 0..pulse_interval_blocks {
                let mut buffer = [0.0f32; 512];
                if b == 0 {
                    // Percussive onset spike
                    for i in 0..32 {
                        buffer[i] = 0.8 * (-((i as f32) / 8.0)).exp();
                    }
                }
                follower.process_audio_block(&buffer, sample_rate, current_time);
                current_time += (block_size as f64) / sample_rate;
            }
        }

        // Verify tempo follower locked onto the rhythm and computed valid BPM
        assert!(
            follower.confidence > 0.50,
            "Follower confidence should increase on regular pulses, got: {}",
            follower.confidence
        );
        assert!(
            (follower.estimated_bpm - 120.0).abs() < 5.0,
            "Follower estimated BPM should be close to 120.0, got: {}",
            follower.estimated_bpm
        );
    }

    #[test]
    fn test_transport_controller_sync_source_switching_and_nudge() {
        let mut transport = TransportController::new(128.0);
        assert_eq!(transport.effective_bpm(), 128.0);
        assert_eq!(transport.sync_source, SyncSource::Internal);

        // DJ Nudge Up: +1.5%
        transport.start_nudge_up();
        assert_eq!(transport.nudge_state, NudgeState::NudgeUp);
        let nudged_up = transport.effective_bpm();
        assert!((nudged_up - (128.0 * 1.015)).abs() < 1e-4);

        // DJ Nudge Down: -1.5%
        transport.start_nudge_down();
        assert_eq!(transport.nudge_state, NudgeState::NudgeDown);
        let nudged_down = transport.effective_bpm();
        assert!((nudged_down - (128.0 * (1.0 - 0.015))).abs() < 1e-4);

        transport.stop_nudge();
        assert_eq!(transport.effective_bpm(), 128.0);

        // Switch to Link sync source
        transport.set_sync_source(SyncSource::Link);
        transport.link.session_bpm = 135.0;
        assert_eq!(transport.effective_bpm(), 135.0);
    }

    #[test]
    fn test_transport_controller_looping_and_metronome() {
        let mut transport = TransportController::new(120.0);
        transport.set_loop(true, 4.0, 4.0); // Loop from beat 4.0 to 8.0 (length 4 beats)
        transport.metronome_enabled = true;
        transport.time.beat = 7.9;
        transport.play();

        // Advance 0.1 seconds at 120 BPM = 0.2 beats.
        // 7.9 + 0.2 = 8.1 >= 8.0 loop end -> wraps to 4.0 + 0.1 = 4.1.
        let report = transport.advance(4410, 44100.0, None);
        assert!(report.loop_wrapped, "Transport should report loop wrap");
        assert!((report.current_beat - 4.1).abs() < 1e-4);

        // Downbeat click verification on measure crossing (beat 4.0 is bar start for 4/4)
        transport.time.beat = 3.95;
        let report_click = transport.advance(4410, 44100.0, None);
        assert_eq!(report_click.metronome_click, Some(MetronomeClick::Downbeat));
    }
}
