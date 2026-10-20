//! Clip slot state machine and transitions for the Session Matrix.

use serde::{Deserialize, Serialize};

/// State of an individual clip slot in the Session Matrix grid.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum SlotState {
    /// Slot contains no clip data.
    #[default]
    Empty,
    /// Slot has a clip loaded but is idle/stopped.
    Stopped,
    /// Slot is waiting for quantization boundary to begin playback.
    QueuedLaunch(f64),
    /// Slot is actively playing with its launch start beat and current loop phase.
    Playing {
        start_beat: f64,
        phase: f64,
    },
    /// Slot is playing but scheduled to stop at the next quantization boundary.
    QueuedStop(f64),
}

impl SlotState {
    /// Returns true if the slot has no clip assigned.
    pub const fn is_empty(&self) -> bool {
        matches!(self, SlotState::Empty)
    }

    /// Returns true if the slot has a clip and is stopped.
    pub const fn is_stopped(&self) -> bool {
        matches!(self, SlotState::Stopped)
    }

    /// Returns true if the slot is waiting to launch.
    pub const fn is_queued_launch(&self) -> bool {
        matches!(self, SlotState::QueuedLaunch(_))
    }

    /// Returns true if the slot is waiting to stop.
    pub const fn is_queued_stop(&self) -> bool {
        matches!(self, SlotState::QueuedStop(_))
    }

    /// Returns true if the slot is either queued for launch or queued for stop.
    pub const fn is_queued(&self) -> bool {
        self.is_queued_launch() || self.is_queued_stop()
    }

    /// Returns true if the slot is currently producing audio/MIDI notes.
    pub const fn is_playing(&self) -> bool {
        matches!(self, SlotState::Playing { .. } | SlotState::QueuedStop(_))
    }

    /// Queue this slot for launch at `target_beat`.
    /// Has no effect if the slot is empty.
    pub fn queue_launch(&mut self, target_beat: f64) {
        if !self.is_empty() {
            *self = SlotState::QueuedLaunch(target_beat);
        }
    }

    /// Queue this slot for stop at `target_beat`.
    /// Has effect only if the slot is currently playing.
    pub fn queue_stop(&mut self, target_beat: f64) {
        if self.is_playing() {
            *self = SlotState::QueuedStop(target_beat);
        }
    }

    /// Immediately launch playback starting at `start_beat`.
    pub fn start_playing_immediate(&mut self, start_beat: f64) {
        if !self.is_empty() {
            *self = SlotState::Playing {
                start_beat,
                phase: 0.0,
            };
        }
    }

    /// Immediately stop playback.
    pub fn stop_immediate(&mut self) {
        if !self.is_empty() {
            *self = SlotState::Stopped;
        }
    }

    /// Process a transport tick at `current_beat`, advancing states and loop phase.
    ///
    /// - If `QueuedLaunch(target_beat)` and `current_beat >= target_beat`:
    ///   transitions to `Playing { start_beat: target_beat, phase: (current_beat - target_beat) % clip_length }`.
    /// - If `QueuedStop(target_beat)` and `current_beat >= target_beat`:
    ///   transitions to `Stopped`.
    /// - If `Playing`, updates loop phase based on `clip_length`.
    pub fn tick(&mut self, current_beat: f64, clip_length: f64) {
        let length = if clip_length > 0.0 { clip_length } else { 4.0 };

        match *self {
            SlotState::QueuedLaunch(target_beat) => {
                if current_beat >= target_beat {
                    let rel = (current_beat - target_beat).max(0.0);
                    let phase = rel % length;
                    *self = SlotState::Playing {
                        start_beat: target_beat,
                        phase,
                    };
                }
            }
            SlotState::QueuedStop(target_beat) => {
                if current_beat >= target_beat {
                    *self = SlotState::Stopped;
                }
            }
            SlotState::Playing { start_beat, ref mut phase } => {
                let elapsed = current_beat - start_beat;
                if elapsed >= 0.0 {
                    *phase = elapsed % length;
                } else {
                    *phase = 0.0;
                }
            }
            SlotState::Empty | SlotState::Stopped => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_state_lifecycle() {
        let mut state = SlotState::Empty;
        assert!(state.is_empty());
        assert!(!state.is_playing());

        // Queue on empty does nothing
        state.queue_launch(4.0);
        assert!(state.is_empty());

        // Set to stopped
        state = SlotState::Stopped;
        assert!(state.is_stopped());

        // Queue launch at beat 4.0
        state.queue_launch(4.0);
        assert!(state.is_queued_launch());
        assert!(state.is_queued());
        assert!(!state.is_playing());

        // Tick before boundary: remains queued
        state.tick(3.5, 4.0);
        assert!(state.is_queued_launch());

        // Tick at boundary: transitions to playing
        state.tick(4.0, 4.0);
        assert!(state.is_playing());
        if let SlotState::Playing { start_beat, phase } = state {
            assert_eq!(start_beat, 4.0);
            assert_eq!(phase, 0.0);
        } else {
            panic!("Expected Playing state");
        }

        // Tick midway: phase updates
        state.tick(5.5, 4.0);
        if let SlotState::Playing { start_beat, phase } = state {
            assert_eq!(start_beat, 4.0);
            assert_eq!(phase, 1.5);
        } else {
            panic!("Expected Playing state");
        }

        // Queue stop at beat 8.0
        state.queue_stop(8.0);
        assert!(state.is_queued_stop());
        assert!(state.is_playing()); // still playing while queued to stop

        // Tick before stop boundary: remains queued stop
        state.tick(7.9, 4.0);
        assert!(state.is_queued_stop());

        // Tick at stop boundary: transitions to stopped
        state.tick(8.0, 4.0);
        assert!(state.is_stopped());
        assert!(!state.is_playing());
    }

    #[test]
    fn test_immediate_start_and_stop() {
        let mut state = SlotState::Stopped;
        state.start_playing_immediate(12.0);
        assert!(state.is_playing());
        if let SlotState::Playing { start_beat, phase } = state {
            assert_eq!(start_beat, 12.0);
            assert_eq!(phase, 0.0);
        }

        state.stop_immediate();
        assert!(state.is_stopped());
    }
}
