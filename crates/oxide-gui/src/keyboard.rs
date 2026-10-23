//! Computer keyboard playing controllers (Piano & Wicki-Hayden Isomorphic).
//!
//! Features:
//! - Piano layout (C4 at KeyA, sharps at W-E-T-Y-U-O-P, octave shifts Z/X).
//! - Wicki-Hayden Isomorphic layout (+3 semitones row offset up, +2 semitones column offset right).
//! - Active note map tracking to guarantee zero stuck notes on key releases or octave transpositions.

use std::collections::HashMap;
use egui::{Context, Event, Key};
use oxide_core::AudioCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardMode {
    Piano,
    Isomorphic,
}

#[derive(Debug, Clone)]
pub struct ComputerKeyboardController {
    pub mode: KeyboardMode,
    pub octave_offset: i8, // -3 .. +3
    pub velocity: u8,
    pub target_track_id: usize,
    active_keys: HashMap<Key, u8>,
}

impl Default for ComputerKeyboardController {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputerKeyboardController {
    pub fn new() -> Self {
        Self {
            mode: KeyboardMode::Piano,
            octave_offset: 0,
            velocity: 100,
            target_track_id: 0,
            active_keys: HashMap::new(),
        }
    }

    pub fn set_mode(&mut self, mode: KeyboardMode) {
        self.mode = mode;
    }

    pub fn set_octave_offset(&mut self, offset: i8) {
        self.octave_offset = offset.clamp(-3, 3);
    }

    pub fn transpose_octave(&mut self, delta: i8) {
        self.octave_offset = (self.octave_offset + delta).clamp(-3, 3);
    }

    /// Convert a physical key to MIDI note under the current layout mode and octave.
    pub fn key_to_midi(&self, key: Key) -> Option<u8> {
        let base_note = match self.mode {
            KeyboardMode::Piano => match key {
                Key::A => Some(60), // C4
                Key::W => Some(61), // C#4
                Key::S => Some(62), // D4
                Key::E => Some(63), // D#4
                Key::D => Some(64), // E4
                Key::F => Some(65), // F4
                Key::T => Some(66), // F#4
                Key::G => Some(67), // G4
                Key::Y => Some(68), // G#4
                Key::H => Some(69), // A4
                Key::U => Some(70), // A#4
                Key::J => Some(71), // B4
                Key::K => Some(72), // C5
                Key::O => Some(73), // C#5
                Key::L => Some(74), // D5
                Key::P => Some(75), // D#5
                Key::Colon | Key::Semicolon => Some(76), // E5
                Key::Quote => Some(77), // F5
                _ => None,
            },
            KeyboardMode::Isomorphic => {
                // Wicki-Hayden:
                // Row 0 (Digit1..): Base 57 (A3)
                // Row 1 (Q..): Base 54 (F#3)
                // Row 2 (A..): Base 51 (Eb3)
                // Row 3 (Z..): Base 48 (C3)
                // Col offset: +2 semitones right
                match key {
                    // Row 0
                    Key::Num1 => Some(57),
                    Key::Num2 => Some(59),
                    Key::Num3 => Some(61),
                    Key::Num4 => Some(63),
                    Key::Num5 => Some(65),
                    Key::Num6 => Some(67),
                    Key::Num7 => Some(69),
                    Key::Num8 => Some(71),
                    Key::Num9 => Some(73),
                    Key::Num0 => Some(75),
                    // Row 1
                    Key::Q => Some(54),
                    Key::W => Some(56),
                    Key::E => Some(58),
                    Key::R => Some(60),
                    Key::T => Some(62),
                    Key::Y => Some(64),
                    Key::U => Some(66),
                    Key::I => Some(68),
                    Key::O => Some(70),
                    Key::P => Some(72),
                    // Row 2
                    Key::A => Some(51),
                    Key::S => Some(53),
                    Key::D => Some(55),
                    Key::F => Some(57),
                    Key::G => Some(59),
                    Key::H => Some(61),
                    Key::J => Some(63),
                    Key::K => Some(65),
                    Key::L => Some(67),
                    Key::Colon | Key::Semicolon => Some(69),
                    Key::Quote => Some(71),
                    // Row 3
                    Key::Z => Some(48),
                    Key::X => Some(50),
                    Key::C => Some(52),
                    Key::V => Some(54),
                    Key::B => Some(56),
                    Key::N => Some(58),
                    Key::M => Some(60),
                    Key::Comma => Some(62),
                    Key::Period => Some(64),
                    Key::Slash => Some(66),
                    _ => None,
                }
            }
        };

        base_note.map(|note| {
            let transposed = note as i16 + (self.octave_offset as i16 * 12);
            transposed.clamp(0, 127) as u8
        })
    }

    /// Convert string physical key code name (for test compatibility).
    pub fn code_to_midi(&self, code: &str) -> Option<u8> {
        let key = match code {
            "KeyA" => Key::A,
            "KeyW" => Key::W,
            "KeyS" => Key::S,
            "KeyE" => Key::E,
            "KeyD" => Key::D,
            "KeyF" => Key::F,
            "KeyT" => Key::T,
            "KeyG" => Key::G,
            "KeyY" => Key::Y,
            "KeyH" => Key::H,
            "KeyU" => Key::U,
            "KeyJ" => Key::J,
            "KeyK" => Key::K,
            "KeyO" => Key::O,
            "KeyL" => Key::L,
            "KeyP" => Key::P,
            "KeyZ" => Key::Z,
            "KeyX" => Key::X,
            "KeyC" => Key::C,
            "KeyV" => Key::V,
            "KeyB" => Key::B,
            "KeyN" => Key::N,
            "KeyM" => Key::M,
            "KeyQ" => Key::Q,
            "KeyR" => Key::R,
            "KeyI" => Key::I,
            _ => return None,
        };
        self.key_to_midi(key)
    }

    /// Process egui input events and dispatch resulting AudioCommands.
    pub fn process_events(&mut self, ctx: &Context) -> Vec<AudioCommand> {
        let mut commands = Vec::new();

        ctx.input(|i| {
            for event in &i.events {
                match event {
                    Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } => {
                        // Octave transposition hotkeys in Piano mode
                        if self.mode == KeyboardMode::Piano && !modifiers.command {
                            if *key == Key::Z {
                                self.transpose_octave(-1);
                                continue;
                            } else if *key == Key::X {
                                self.transpose_octave(1);
                                continue;
                            }
                        }

                        if let Some(note) = self.key_to_midi(*key) {
                            if !self.active_keys.contains_key(key) {
                                self.active_keys.insert(*key, note);
                                commands.push(AudioCommand::NoteOn {
                                    track_id: self.target_track_id,
                                    note,
                                    velocity: self.velocity,
                                });
                            }
                        }
                    }
                    Event::Key {
                        key,
                        pressed: false,
                        ..
                    } => {
                        if let Some(active_note) = self.active_keys.remove(key) {
                            commands.push(AudioCommand::NoteOff {
                                track_id: self.target_track_id,
                                note: active_note,
                            });
                        }
                    }
                    _ => {}
                }
            }
        });

        commands
    }

    /// Reset all sounding notes.
    pub fn all_notes_off(&mut self) -> Vec<AudioCommand> {
        let mut commands = Vec::new();
        for (_, note) in self.active_keys.drain() {
            commands.push(AudioCommand::NoteOff {
                track_id: self.target_track_id,
                note,
            });
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piano_layout_mappings_and_octave_shifts() {
        let mut controller = ComputerKeyboardController::new();
        controller.set_mode(KeyboardMode::Piano);

        assert_eq!(controller.code_to_midi("KeyA"), Some(60)); // C4
        assert_eq!(controller.code_to_midi("KeyW"), Some(61)); // C#4
        assert_eq!(controller.code_to_midi("KeyK"), Some(72)); // C5

        controller.transpose_octave(1);
        assert_eq!(controller.code_to_midi("KeyA"), Some(72)); // C5

        controller.transpose_octave(-2);
        assert_eq!(controller.code_to_midi("KeyA"), Some(48)); // C3
    }

    #[test]
    fn test_isomorphic_wicki_hayden_geometry() {
        let mut controller = ComputerKeyboardController::new();
        controller.set_mode(KeyboardMode::Isomorphic);
        controller.set_octave_offset(0);

        assert_eq!(controller.code_to_midi("KeyZ"), Some(48)); // C3
        assert_eq!(controller.code_to_midi("KeyA"), Some(51)); // Eb3 (+3 semitones row up)
        assert_eq!(controller.code_to_midi("KeyX"), Some(50)); // D3 (+2 semitones col right)
        assert_eq!(controller.code_to_midi("KeyQ"), Some(54)); // F#3 (+6 semitones 2 rows up)
    }
}
