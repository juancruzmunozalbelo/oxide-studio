//! Core domain models and audio interfaces for ableton-rust.

pub mod device;
pub mod scale;
pub mod tuning;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use device::{AudioEffectDevice, Device, DeviceCategory, DeviceChain, InstrumentDevice, MidiEffectDevice};
pub use scale::{GlobalScale, ScaleMode, ROOT_NAMES};
pub use tuning::{TuningSystem, TuningType};

/// Fixed-size zero-allocation audio buffer holding up to 1024 stereo frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioBuffer {
    pub left: [f32; 1024],
    pub right: [f32; 1024],
    pub frame_count: usize,
}

impl AudioBuffer {
    /// Create a new empty AudioBuffer with frame_count 0 and all samples set to 0.0.
    pub const fn new() -> Self {
        Self {
            left: [0.0; 1024],
            right: [0.0; 1024],
            frame_count: 0,
        }
    }

    /// Create an AudioBuffer with specified frame count, all zeroed.
    pub fn silence(frame_count: usize) -> Self {
        Self {
            left: [0.0; 1024],
            right: [0.0; 1024],
            frame_count: frame_count.min(1024),
        }
    }

    /// Reset all samples in active frame_count to 0.0.
    pub fn clear(&mut self) {
        for i in 0..self.frame_count.min(1024) {
            self.left[i] = 0.0;
            self.right[i] = 0.0;
        }
    }

    /// Reset all 1024 samples to 0.0 and set frame_count.
    pub fn reset_all(&mut self, frame_count: usize) {
        self.left = [0.0; 1024];
        self.right = [0.0; 1024];
        self.frame_count = frame_count.min(1024);
    }

    /// Accumulate (mix) samples from another buffer into this buffer.
    pub fn mix_from(&mut self, other: &AudioBuffer) {
        let count = self.frame_count.min(other.frame_count).min(1024);
        for i in 0..count {
            self.left[i] += other.left[i];
            self.right[i] += other.right[i];
        }
    }

    /// Scale all active frames by a gain factor.
    pub fn apply_gain(&mut self, gain: f32) {
        let count = self.frame_count.min(1024);
        for i in 0..count {
            self.left[i] *= gain;
            self.right[i] *= gain;
        }
    }
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Representation of current musical transport time.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MusicalTime {
    pub beat: f64,
    pub bpm: f64,
    pub signature_num: u8,
    pub signature_den: u8,
    pub is_playing: bool,
}

impl MusicalTime {
    pub const fn new(beat: f64, bpm: f64, signature_num: u8, signature_den: u8, is_playing: bool) -> Self {
        Self {
            beat,
            bpm,
            signature_num,
            signature_den,
            is_playing,
        }
    }

    /// Advance transport time by delta_seconds.
    pub fn advance_seconds(&mut self, delta_seconds: f64) {
        if self.is_playing && self.bpm > 0.0 {
            let beats_per_second = self.bpm / 60.0;
            self.beat += delta_seconds * beats_per_second;
        }
    }

    /// Advance transport time by a number of audio frames at given sample rate.
    pub fn advance_frames(&mut self, frames: usize, sample_rate: f64) {
        if sample_rate > 0.0 {
            let delta_seconds = frames as f64 / sample_rate;
            self.advance_seconds(delta_seconds);
        }
    }
}

impl Default for MusicalTime {
    fn default() -> Self {
        Self {
            beat: 0.0,
            bpm: 120.0,
            signature_num: 4,
            signature_den: 4,
            is_playing: false,
        }
    }
}

/// Standard MIDI events routed through tracks and devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    PitchBend { channel: u8, value: i16 },
}

/// Lock-free commands sent from UI or sequencer to the real-time audio thread.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AudioCommand {
    Play,
    Stop,
    SetTempo(f64),
    NoteOn { track_id: usize, note: u8, velocity: u8 },
    NoteOff { track_id: usize, note: u8 },
    SetParameter { track_id: usize, param_id: u32, value: f32 },
    TriggerClip { track_id: usize, slot_id: usize },
    StopClip { track_id: usize },
    TriggerScene(usize),
}

/// Real-time telemetry snapshot returned from the audio thread for UI metering and visualizers.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioTelemetry {
    pub playhead_beat: f64,
    pub cpu_load: f32,
    pub track_peaks_l: [f32; 16],
    pub track_peaks_r: [f32; 16],
    pub master_peak_l: f32,
    pub master_peak_r: f32,
    pub master_rms_l: f32,
    pub master_rms_r: f32,
    pub oscilloscope_slice: [f32; 256],
    pub spectrum_bins: [f32; 64],
}

impl AudioTelemetry {
    pub const fn new() -> Self {
        Self {
            playhead_beat: 0.0,
            cpu_load: 0.0,
            track_peaks_l: [0.0; 16],
            track_peaks_r: [0.0; 16],
            master_peak_l: 0.0,
            master_peak_r: 0.0,
            master_rms_l: 0.0,
            master_rms_r: 0.0,
            oscilloscope_slice: [0.0; 256],
            spectrum_bins: [0.0; 64],
        }
    }
}

impl Default for AudioTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_buffer_lifecycle() {
        let mut buf = AudioBuffer::silence(512);
        assert_eq!(buf.frame_count, 512);
        assert_eq!(buf.left[0], 0.0);
        assert_eq!(buf.right[511], 0.0);

        buf.left[0] = 0.5;
        buf.right[0] = -0.5;
        buf.apply_gain(2.0);
        assert_eq!(buf.left[0], 1.0);
        assert_eq!(buf.right[0], -1.0);

        let mut other = AudioBuffer::silence(512);
        other.left[0] = 0.25;
        other.right[0] = 0.75;
        buf.mix_from(&other);
        assert_eq!(buf.left[0], 1.25);
        assert_eq!(buf.right[0], -0.25);

        buf.clear();
        assert_eq!(buf.left[0], 0.0);
        assert_eq!(buf.right[0], 0.0);
    }

    #[test]
    fn test_musical_time_advancement() {
        let mut time = MusicalTime::default();
        assert_eq!(time.bpm, 120.0);
        assert!(!time.is_playing);

        // Not playing: does not advance
        time.advance_seconds(1.0);
        assert_eq!(time.beat, 0.0);

        time.is_playing = true;
        // At 120 BPM, 1 second = 2 beats
        time.advance_seconds(1.0);
        assert!((time.beat - 2.0).abs() < 1e-9);

        // Advance 44100 frames at 44100 Hz = 1 second = 2 more beats
        time.advance_frames(44100, 44100.0);
        assert!((time.beat - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_midi_event_creation() {
        let note_on = MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 };
        let note_off = MidiEvent::NoteOff { channel: 0, note: 60 };
        let cc = MidiEvent::ControlChange { channel: 0, controller: 7, value: 127 };
        let pb = MidiEvent::PitchBend { channel: 0, value: 8191 };

        assert_eq!(note_on, MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        assert_ne!(note_on, note_off);
        assert_ne!(cc, pb);
    }

    #[test]
    fn test_audio_command_and_telemetry() {
        let cmd = AudioCommand::NoteOn { track_id: 1, note: 64, velocity: 90 };
        assert_eq!(cmd, AudioCommand::NoteOn { track_id: 1, note: 64, velocity: 90 });

        let telemetry = AudioTelemetry::default();
        assert_eq!(telemetry.playhead_beat, 0.0);
        assert_eq!(telemetry.master_peak_l, 0.0);
        assert_eq!(telemetry.oscilloscope_slice.len(), 256);
        assert_eq!(telemetry.spectrum_bins.len(), 64);
    }
}
