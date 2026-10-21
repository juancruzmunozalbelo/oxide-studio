//! 1:1 Ableton Live Muscle-Memory Keyboard Shortcuts for Oxide Studio.
//!
//! Provides standard Live 12 keyboard shortcuts:
//! - Tab: Switch Session <-> Arrangement view
//! - Shift+Tab: Toggle Clip View <-> Device Chain
//! - Space: Toggle Play / Stop
//! - Cmd+T: Insert Audio Track
//! - Cmd+Shift+T: Insert MIDI Track
//! - Cmd+Option+T: Insert Return Track
//! - Cmd+Shift+M: Insert New MIDI Clip
//! - Cmd+D: Duplicate Selected Track / Clip
//! - Cmd+E: Split Clip at Playhead
//! - Cmd+J: Consolidate Selection
//! - Cmd+B: Toggle Draw Mode / Render Bounce
//! - Cmd+Option+B: Toggle Left Browser Panel
//! - Cmd+Shift+R: Stem Export
//! - 0: Toggle Track Mute
//! - S: Toggle Track Solo
//! - Z / X: Octave Down / Up (-3..+3)
//! - C / V: Velocity Down / Up (-/+ 10)
//! - ? / F1: Shortcuts Guide Modal Overlay

use egui::{vec2, Align, Align2, Context, Grid, Key, Layout, Modifiers, RichText, Window};
use crate::theme::{COLOR_AMBER, COLOR_TEXT_PRIMARY};

/// Discrete high-level DAW shortcut actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioShortcutAction {
    ToggleWorkspaceView,        // Tab
    ToggleDetailView,           // Shift+Tab
    TogglePlayStop,             // Space
    NewAudioTrack,              // Cmd+T
    NewMidiTrack,               // Cmd+Shift+T
    NewReturnTrack,             // Cmd+Alt+T
    NewMidiClip,                // Cmd+Shift+M
    DuplicateSelected,          // Cmd+D
    SplitAtPlayhead,            // Cmd+E
    ConsolidateSelection,       // Cmd+J
    ToggleDrawOrBounce,         // Cmd+B
    ToggleBrowser,              // Cmd+Alt+B
    ExportStems,                // Cmd+Shift+R
    ToggleMuteSelected,         // 0
    ToggleSoloSelected,         // S
    OctaveDown,                 // Z
    OctaveUp,                   // X
    VelocityDown,               // C
    VelocityUp,                 // V
    ToggleShortcutsModal,       // ? or F1
}

/// Controller managing keyboard shortcuts and the interactive shortcut guide modal.
#[derive(Debug, Clone, Default)]
pub struct StudioShortcutManager {
    pub show_shortcuts_modal: bool,
}

impl StudioShortcutManager {
    pub fn new() -> Self {
        Self {
            show_shortcuts_modal: false,
        }
    }

    /// Process live egui input events and extract active DAW shortcut actions.
    pub fn process_input(&mut self, ctx: &Context) -> Vec<StudioShortcutAction> {
        let mut actions = Vec::new();
        let wants_keyboard = ctx.wants_keyboard_input();

        ctx.input(|i| {
            let cmd = i.modifiers.command || i.modifiers.ctrl;
            let shift = i.modifiers.shift;
            let alt = i.modifiers.alt;

            // 1. Shortcuts modal overlay (? or F1)
            if i.key_pressed(Key::F1) || (shift && i.key_pressed(Key::Slash)) {
                actions.push(StudioShortcutAction::ToggleShortcutsModal);
            }

            // 2. Tab key combinations
            if i.key_pressed(Key::Tab) {
                if shift {
                    actions.push(StudioShortcutAction::ToggleDetailView);
                } else {
                    actions.push(StudioShortcutAction::ToggleWorkspaceView);
                }
            }

            // 3. Space: Play / Stop transport (only when not typing in text field)
            if i.key_pressed(Key::Space) && !wants_keyboard {
                actions.push(StudioShortcutAction::TogglePlayStop);
            }

            // 4. Command combos
            if cmd {
                if i.key_pressed(Key::T) {
                    if shift {
                        actions.push(StudioShortcutAction::NewMidiTrack);
                    } else if alt {
                        actions.push(StudioShortcutAction::NewReturnTrack);
                    } else {
                        actions.push(StudioShortcutAction::NewAudioTrack);
                    }
                }

                if shift && i.key_pressed(Key::M) {
                    actions.push(StudioShortcutAction::NewMidiClip);
                }

                if i.key_pressed(Key::D) && !shift && !alt {
                    actions.push(StudioShortcutAction::DuplicateSelected);
                }

                if i.key_pressed(Key::E) && !shift && !alt {
                    actions.push(StudioShortcutAction::SplitAtPlayhead);
                }

                if i.key_pressed(Key::J) && !shift && !alt {
                    actions.push(StudioShortcutAction::ConsolidateSelection);
                }

                if i.key_pressed(Key::B) {
                    if alt {
                        actions.push(StudioShortcutAction::ToggleBrowser);
                    } else if !shift {
                        actions.push(StudioShortcutAction::ToggleDrawOrBounce);
                    }
                }

                if shift && i.key_pressed(Key::R) {
                    actions.push(StudioShortcutAction::ExportStems);
                }
            } else if !alt && !shift && !wants_keyboard {
                // 5. Single key shortcuts (guarded: suppressed when typing in text field)
                if i.key_pressed(Key::Num0) {
                    actions.push(StudioShortcutAction::ToggleMuteSelected);
                }
                if i.key_pressed(Key::S) {
                    actions.push(StudioShortcutAction::ToggleSoloSelected);
                }
                if i.key_pressed(Key::Z) {
                    actions.push(StudioShortcutAction::OctaveDown);
                }
                if i.key_pressed(Key::X) {
                    actions.push(StudioShortcutAction::OctaveUp);
                }
                if i.key_pressed(Key::C) {
                    actions.push(StudioShortcutAction::VelocityDown);
                }
                if i.key_pressed(Key::V) {
                    actions.push(StudioShortcutAction::VelocityUp);
                }
            }
        });

        for action in &actions {
            if *action == StudioShortcutAction::ToggleShortcutsModal {
                self.show_shortcuts_modal = !self.show_shortcuts_modal;
            }
        }

        actions
    }

    /// Render 1:1 Ableton shortcuts modal dialog when toggled.
    pub fn render_shortcuts_modal(&mut self, ctx: &Context) {
        if !self.show_shortcuts_modal {
            return;
        }

        let mut is_open = self.show_shortcuts_modal;
        let mut close_clicked = false;

        Window::new("⌨ Ableton Live 1:1 Keyboard Shortcuts")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.heading("⚡ Oxide Studio Muscle-Memory Shortcuts");
                ui.separator();

                Grid::new("shortcuts_help_grid")
                    .striped(true)
                    .spacing(vec2(16.0, 6.0))
                    .show(ui, |ui| {
                        let rows = [
                            ("Tab", "Switch Session <-> Arrangement View"),
                            ("Shift + Tab", "Toggle Clip View <-> Device Chain"),
                            ("Space", "Play / Stop Transport"),
                            ("Cmd + T", "Insert Audio Track"),
                            ("Cmd + Shift + T", "Insert MIDI Track"),
                            ("Cmd + Option + T", "Insert Return Track"),
                            ("Cmd + Shift + M", "Insert New MIDI Clip"),
                            ("Cmd + D", "Duplicate Selected Track / Clip"),
                            ("Cmd + E", "Split Clip at Playhead"),
                            ("Cmd + J", "Consolidate Selection"),
                            ("Cmd + B", "Toggle Draw Mode / Bounce"),
                            ("Cmd + Option + B", "Toggle Left Browser Panel"),
                            ("Cmd + Shift + R", "Export Stems / Audio"),
                            ("0", "Mute / Unmute Selected Track"),
                            ("S", "Solo / Unsolo Selected Track"),
                            ("Z / X", "Octave Down / Up (-3..+3)"),
                            ("C / V", "Velocity Down / Up (-/+ 10)"),
                            ("? / F1", "Toggle this Shortcuts Guide"),
                        ];

                        for (key, desc) in rows {
                            ui.label(RichText::new(key).monospace().strong().color(COLOR_AMBER));
                            ui.label(RichText::new(desc).color(COLOR_TEXT_PRIMARY));
                            ui.end_row();
                        }
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                });
            });

        self.show_shortcuts_modal = is_open && !close_clicked;
    }

    /// Pure evaluation function mapping (Key, Modifiers) to Option<StudioShortcutAction> for deterministic unit testing.
    pub fn evaluate_shortcut(key: Key, modifiers: Modifiers) -> Option<StudioShortcutAction> {
        let cmd = modifiers.command || modifiers.ctrl;
        let shift = modifiers.shift;
        let alt = modifiers.alt;

        if key == Key::F1 || (shift && key == Key::Slash) {
            return Some(StudioShortcutAction::ToggleShortcutsModal);
        }

        if key == Key::Tab {
            return if shift {
                Some(StudioShortcutAction::ToggleDetailView)
            } else {
                Some(StudioShortcutAction::ToggleWorkspaceView)
            };
        }

        if key == Key::Space {
            return Some(StudioShortcutAction::TogglePlayStop);
        }

        if cmd {
            if key == Key::T {
                return if shift {
                    Some(StudioShortcutAction::NewMidiTrack)
                } else if alt {
                    Some(StudioShortcutAction::NewReturnTrack)
                } else {
                    Some(StudioShortcutAction::NewAudioTrack)
                };
            }
            if shift && key == Key::M {
                return Some(StudioShortcutAction::NewMidiClip);
            }
            if key == Key::D && !shift && !alt {
                return Some(StudioShortcutAction::DuplicateSelected);
            }
            if key == Key::E && !shift && !alt {
                return Some(StudioShortcutAction::SplitAtPlayhead);
            }
            if key == Key::J && !shift && !alt {
                return Some(StudioShortcutAction::ConsolidateSelection);
            }
            if key == Key::B {
                return if alt {
                    Some(StudioShortcutAction::ToggleBrowser)
                } else if !shift {
                    Some(StudioShortcutAction::ToggleDrawOrBounce)
                } else {
                    None
                };
            }
            if shift && key == Key::R {
                return Some(StudioShortcutAction::ExportStems);
            }
        } else if !alt && !shift {
            match key {
                Key::Num0 => return Some(StudioShortcutAction::ToggleMuteSelected),
                Key::S => return Some(StudioShortcutAction::ToggleSoloSelected),
                Key::Z => return Some(StudioShortcutAction::OctaveDown),
                Key::X => return Some(StudioShortcutAction::OctaveUp),
                Key::C => return Some(StudioShortcutAction::VelocityDown),
                Key::V => return Some(StudioShortcutAction::VelocityUp),
                _ => {}
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortcuts_tab_and_views() {
        let none = Modifiers::NONE;
        let shift = Modifiers::SHIFT;

        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::Tab, none),
            Some(StudioShortcutAction::ToggleWorkspaceView)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::Tab, shift),
            Some(StudioShortcutAction::ToggleDetailView)
        );
    }

    #[test]
    fn test_shortcuts_transport_space() {
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::Space, Modifiers::NONE),
            Some(StudioShortcutAction::TogglePlayStop)
        );
    }

    #[test]
    fn test_shortcuts_cmd_track_creation() {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let cmd_alt = Modifiers::COMMAND | Modifiers::ALT;

        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::T, cmd),
            Some(StudioShortcutAction::NewAudioTrack)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::T, cmd_shift),
            Some(StudioShortcutAction::NewMidiTrack)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::T, cmd_alt),
            Some(StudioShortcutAction::NewReturnTrack)
        );
    }

    #[test]
    fn test_shortcuts_editing_and_clips() {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let cmd_alt = Modifiers::COMMAND | Modifiers::ALT;

        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::M, cmd_shift),
            Some(StudioShortcutAction::NewMidiClip)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::D, cmd),
            Some(StudioShortcutAction::DuplicateSelected)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::E, cmd),
            Some(StudioShortcutAction::SplitAtPlayhead)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::J, cmd),
            Some(StudioShortcutAction::ConsolidateSelection)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::B, cmd),
            Some(StudioShortcutAction::ToggleDrawOrBounce)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::B, cmd_alt),
            Some(StudioShortcutAction::ToggleBrowser)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::R, cmd_shift),
            Some(StudioShortcutAction::ExportStems)
        );
    }

    #[test]
    fn test_shortcuts_single_keys_and_modal() {
        let none = Modifiers::NONE;

        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::Num0, none),
            Some(StudioShortcutAction::ToggleMuteSelected)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::S, none),
            Some(StudioShortcutAction::ToggleSoloSelected)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::Z, none),
            Some(StudioShortcutAction::OctaveDown)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::X, none),
            Some(StudioShortcutAction::OctaveUp)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::C, none),
            Some(StudioShortcutAction::VelocityDown)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::V, none),
            Some(StudioShortcutAction::VelocityUp)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::F1, none),
            Some(StudioShortcutAction::ToggleShortcutsModal)
        );
        assert_eq!(
            StudioShortcutManager::evaluate_shortcut(Key::Slash, Modifiers::SHIFT),
            Some(StudioShortcutAction::ToggleShortcutsModal)
        );
    }

    #[test]
    fn test_shortcuts_suppressed_when_wants_keyboard_input() {
        let ctx = Context::default();
        let mut manager = StudioShortcutManager::new();

        // 1. Without text input focus: Space fires TogglePlayStop
        let mut raw_input = egui::RawInput::default();
        raw_input.events.push(egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        let _ = ctx.run(raw_input, |_ctx| {});
        let actions = manager.process_input(&ctx);
        assert_eq!(actions, vec![StudioShortcutAction::TogglePlayStop]);

        // 2. With text input active (wants_keyboard_input = true): Space & single keys are suppressed
        let mut raw_input2 = egui::RawInput::default();
        raw_input2.events.push(egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        raw_input2.events.push(egui::Event::Key {
            key: Key::S,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        let _ = ctx.run(raw_input2, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut text = String::new();
                let te = ui.text_edit_singleline(&mut text);
                te.request_focus();
            });
        });
        assert!(ctx.wants_keyboard_input());
        let actions2 = manager.process_input(&ctx);
        assert!(actions2.is_empty(), "Expected single-key shortcuts to be suppressed when text input has focus, got {:?}", actions2);
    }
}
