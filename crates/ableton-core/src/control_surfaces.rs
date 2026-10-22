//! Hardware Control Surface Engine: Ableton Push 2/3 Display & Pad Matrix,
//! Akai APC40 mkII, Novation Launchpad Pro, and Universal MIDI Learn & Takeover Modes.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Supported hardware control surface models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ControlSurfaceModel {
    Push3,
    Push2,
    Apc40MkII,
    LaunchpadProMk3,
    GenericMidi,
}

impl ControlSurfaceModel {
    /// Canonical controller name.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Push3 => "Ableton Push 3",
            Self::Push2 => "Ableton Push 2",
            Self::Apc40MkII => "Akai APC40 mkII",
            Self::LaunchpadProMk3 => "Novation Launchpad Pro MK3",
            Self::GenericMidi => "Generic MIDI Controller",
        }
    }

    /// Whether the controller possesses an embedded display.
    pub const fn has_display(&self) -> bool {
        matches!(self, Self::Push3 | Self::Push2)
    }

    /// Display resolution in pixels `(width, height)`.
    pub const fn display_resolution(&self) -> Option<(u32, u32)> {
        match self {
            Self::Push3 | Self::Push2 => Some((960, 160)),
            _ => None,
        }
    }

    /// Pad matrix dimensions `(rows, columns)`.
    pub const fn pad_matrix_dimensions(&self) -> (usize, usize) {
        match self {
            Self::Push3 | Self::Push2 | Self::LaunchpadProMk3 => (8, 8),
            Self::Apc40MkII => (5, 8),
            Self::GenericMidi => (0, 0),
        }
    }

    /// Whether the controller possesses a ribbon controller / touch strip.
    pub const fn has_touch_strip(&self) -> bool {
        matches!(self, Self::Push3 | Self::Push2)
    }

    /// Whether the controller supports polyphonic expression / MPE per pad.
    pub const fn has_mpe(&self) -> bool {
        matches!(self, Self::Push3 | Self::LaunchpadProMk3)
    }

    /// Total number of dedicated physical hardware volume faders.
    pub const fn fader_count(&self) -> usize {
        match self {
            Self::Apc40MkII => 9, // 8 channel faders + 1 master fader
            _ => 0,
        }
    }

    /// Total number of rotary encoders / knobs.
    pub const fn encoder_count(&self) -> usize {
        match self {
            Self::Push3 | Self::Push2 => 11, // 8 display encoders + 1 tempo + 1 swing + 1 master
            Self::Apc40MkII => 16,           // 8 track knobs + 8 device knobs
            Self::LaunchpadProMk3 => 0,
            Self::GenericMidi => 8,
        }
    }
}

/// Parameter takeover modes preventing abrupt audio jumps when hardware controls
/// are misaligned with software parameter values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TakeoverMode {
    /// Immediate jump: parameter snaps directly to incoming hardware value.
    Jump,
    /// Pick-up: ignores physical CC movement until the hardware knob crosses
    /// or reaches the current software parameter value.
    Pickup,
    /// Value scaling: smoothly converges hardware value to software value
    /// proportionally without jumps or audible step clicks.
    ValueScaling,
}

impl TakeoverMode {
    /// Evaluate incoming hardware movement against current software parameter value.
    /// Values are normalized to [0.0, 1.0].
    /// Returns `Some(new_soft_val)` if the parameter should be updated, or `None` if ignored.
    pub fn evaluate_takeover(
        &self,
        current_soft_val: f32,
        prev_hw_val: f32,
        new_hw_val: f32,
    ) -> Option<f32> {
        let current_soft = current_soft_val.clamp(0.0, 1.0);
        let prev_hw = prev_hw_val.clamp(0.0, 1.0);
        let new_hw = new_hw_val.clamp(0.0, 1.0);

        match self {
            Self::Jump => Some(new_hw),
            Self::Pickup => {
                // Exact match or crossing
                if (new_hw - current_soft).abs() < 1e-4 {
                    return Some(new_hw);
                }
                let crossed = (prev_hw <= current_soft && new_hw >= current_soft)
                    || (prev_hw >= current_soft && new_hw <= current_soft);
                if crossed {
                    Some(new_hw)
                } else {
                    None
                }
            }
            Self::ValueScaling => {
                let delta = new_hw - prev_hw;
                if delta.abs() < 1e-6 {
                    return Some(current_soft);
                }

                if delta > 0.0 {
                    // Moving upwards
                    if prev_hw >= 1.0 - 1e-6 {
                        Some(1.0)
                    } else {
                        let scale = (1.0 - current_soft).max(0.0) / (1.0 - prev_hw).max(1e-6);
                        let converged = (current_soft + delta * scale).clamp(0.0, 1.0);
                        Some(converged)
                    }
                } else {
                    // Moving downwards
                    if prev_hw <= 1e-6 {
                        Some(0.0)
                    } else {
                        let scale = current_soft.max(0.0) / prev_hw.max(1e-6);
                        let converged = (current_soft + delta * scale).clamp(0.0, 1.0);
                        Some(converged)
                    }
                }
            }
        }
    }
}

/// A target DAW parameter bound via MIDI Learn.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MidiLearnTarget {
    pub track_id: usize,
    pub device_id: usize,
    pub param_id: u32,
    pub min_val: f32,
    pub max_val: f32,
}

impl MidiLearnTarget {
    pub fn new(track_id: usize, device_id: usize, param_id: u32, min_val: f32, max_val: f32) -> Self {
        Self {
            track_id,
            device_id,
            param_id,
            min_val,
            max_val,
        }
    }

    /// Map a 7-bit MIDI CC value (0..127) to the target's parameter range.
    pub fn map_cc_to_value(&self, raw_cc: u8) -> f32 {
        let norm = (raw_cc.min(127) as f32) / 127.0;
        self.min_val + norm * (self.max_val - self.min_val)
    }

    /// Map a continuous parameter value back to a 7-bit MIDI CC (0..127) for controller feedback.
    pub fn map_value_to_cc(&self, param_val: f32) -> u8 {
        let span = self.max_val - self.min_val;
        if span.abs() < 1e-6 {
            return 0;
        }
        let norm = ((param_val - self.min_val) / span).clamp(0.0, 1.0);
        (norm * 127.0).round() as u8
    }
}

/// Registry storing mappings from MIDI (channel, CC) and (channel, Note) to DAW parameters with Learn mode.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MidiLearnRegistry {
    bindings: BTreeMap<(u8, u8), MidiLearnTarget>,
    pending_learn: Option<MidiLearnTarget>,
    note_bindings: BTreeMap<(u8, u8), MidiLearnTarget>,
    pending_learn_note: Option<MidiLearnTarget>,
}

impl MidiLearnRegistry {
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
            pending_learn: None,
            note_bindings: BTreeMap::new(),
            pending_learn_note: None,
        }
    }

    /// Arm the registry to learn the next incoming MIDI CC and bind it to `target`.
    pub fn learn_next_cc(&mut self, target: MidiLearnTarget) {
        self.pending_learn = Some(target);
    }

    /// Cancel active CC learn mode.
    pub fn cancel_learn(&mut self) {
        self.pending_learn = None;
    }

    /// Check if currently in MIDI CC learn mode.
    pub fn is_learning(&self) -> bool {
        self.pending_learn.is_some()
    }

    /// Explicitly bind a `(channel, cc)` to a target.
    pub fn bind(&mut self, channel: u8, cc: u8, target: MidiLearnTarget) {
        self.bindings.insert((channel, cc), target);
    }

    /// Unbind a `(channel, cc)`.
    pub fn unbind(&mut self, channel: u8, cc: u8) -> Option<MidiLearnTarget> {
        self.bindings.remove(&(channel, cc))
    }

    /// Retrieve an existing CC binding.
    pub fn get_binding(&self, channel: u8, cc: u8) -> Option<&MidiLearnTarget> {
        self.bindings.get(&(channel, cc))
    }

    /// Total number of active CC bindings.
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Arm the registry to learn the next incoming MIDI Note and bind it to `target`.
    pub fn learn_next_note(&mut self, target: MidiLearnTarget) {
        self.pending_learn_note = Some(target);
    }

    /// Cancel active note learn mode.
    pub fn cancel_learn_note(&mut self) {
        self.pending_learn_note = None;
    }

    /// Check if currently in MIDI note learn mode.
    pub fn is_learning_note(&self) -> bool {
        self.pending_learn_note.is_some()
    }

    /// Explicitly bind a `(channel, note)` to a target.
    pub fn bind_note(&mut self, channel: u8, note: u8, target: MidiLearnTarget) {
        self.note_bindings.insert((channel, note), target);
    }

    /// Unbind a `(channel, note)`.
    pub fn unbind_note(&mut self, channel: u8, note: u8) -> Option<MidiLearnTarget> {
        self.note_bindings.remove(&(channel, note))
    }

    /// Retrieve an existing note binding.
    pub fn get_note_binding(&self, channel: u8, note: u8) -> Option<&MidiLearnTarget> {
        self.note_bindings.get(&(channel, note))
    }

    /// Total number of active note bindings.
    pub fn note_binding_count(&self) -> usize {
        self.note_bindings.len()
    }

    /// Total number of combined CC and Note bindings.
    pub fn total_binding_count(&self) -> usize {
        self.bindings.len() + self.note_bindings.len()
    }

    /// Process an incoming MIDI CC event. If in learn mode, automatically creates
    /// the binding and exits learn mode.
    /// Returns `Some((target, mapped_value))` if a binding matched or was learned.
    pub fn process_midi_cc(&mut self, channel: u8, cc: u8, raw_value: u8) -> Option<(MidiLearnTarget, f32)> {
        if let Some(target) = self.pending_learn.take() {
            let mapped_val = target.map_cc_to_value(raw_value);
            self.bindings.insert((channel, cc), target.clone());
            return Some((target, mapped_val));
        }

        if let Some(target) = self.bindings.get(&(channel, cc)) {
            let mapped_val = target.map_cc_to_value(raw_value);
            return Some((target.clone(), mapped_val));
        }

        None
    }

    /// Process an incoming MIDI Note event (NoteOn with velocity > 0).
    /// If in note learn mode, automatically creates the binding and exits learn mode.
    /// Returns `Some((target, mapped_value))` if a binding matched or was learned.
    pub fn process_midi_note(
        &mut self,
        channel: u8,
        note: u8,
        velocity: u8,
        current_val: f32,
    ) -> Option<(MidiLearnTarget, f32)> {
        if velocity == 0 {
            return None;
        }

        if let Some(target) = self.pending_learn_note.take() {
            let dist_min = (current_val - target.min_val).abs();
            let dist_max = (current_val - target.max_val).abs();
            let mapped_val = if dist_max < dist_min { target.min_val } else { target.max_val };
            self.note_bindings.insert((channel, note), target.clone());
            return Some((target, mapped_val));
        }

        if let Some(target) = self.note_bindings.get(&(channel, note)) {
            let dist_min = (current_val - target.min_val).abs();
            let dist_max = (current_val - target.max_val).abs();
            let mapped_val = if dist_max < dist_min { target.min_val } else { target.max_val };
            return Some((target.clone(), mapped_val));
        }

        None
    }
}

/// A target DAW parameter bound via Computer Keyboard Mapping (Cmd+K).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KeyLearnTarget {
    pub track_id: usize,
    pub device_id: usize,
    pub param_id: u32,
    pub min_val: f32,
    pub max_val: f32,
    pub is_toggle: bool,
}

impl KeyLearnTarget {
    pub fn new(track_id: usize, device_id: usize, param_id: u32, min_val: f32, max_val: f32) -> Self {
        Self {
            track_id,
            device_id,
            param_id,
            min_val,
            max_val,
            is_toggle: true,
        }
    }

    pub fn with_toggle(mut self, is_toggle: bool) -> Self {
        self.is_toggle = is_toggle;
        self
    }

    /// Evaluate parameter value upon key press.
    pub fn evaluate_key_press(&self, current_val: f32) -> f32 {
        if self.is_toggle {
            let dist_min = (current_val - self.min_val).abs();
            let dist_max = (current_val - self.max_val).abs();
            if dist_max < dist_min {
                self.min_val
            } else {
                self.max_val
            }
        } else {
            self.max_val
        }
    }

    /// Evaluate parameter value upon key release (momentary mode resets to min_val).
    pub fn evaluate_key_release(&self) -> f32 {
        self.min_val
    }
}

/// Registry storing mappings from computer keyboard keys to DAW parameters with Key Map mode (Cmd+K).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KeyLearnRegistry {
    bindings: BTreeMap<String, KeyLearnTarget>,
    pending_learn: Option<KeyLearnTarget>,
}

impl KeyLearnRegistry {
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
            pending_learn: None,
        }
    }

    /// Arm the registry to learn the next pressed computer keyboard key.
    pub fn learn_next_key(&mut self, target: KeyLearnTarget) {
        self.pending_learn = Some(target);
    }

    /// Cancel active key learn mode.
    pub fn cancel_learn(&mut self) {
        self.pending_learn = None;
    }

    /// Check if currently in key learn mode.
    pub fn is_learning(&self) -> bool {
        self.pending_learn.is_some()
    }

    /// Explicitly bind a key string identifier (e.g. "Space", "KeyA", "Digit1") to a target.
    pub fn bind(&mut self, key: impl Into<String>, target: KeyLearnTarget) {
        self.bindings.insert(key.into(), target);
    }

    /// Unbind a key.
    pub fn unbind(&mut self, key: &str) -> Option<KeyLearnTarget> {
        self.bindings.remove(key)
    }

    /// Retrieve an existing binding.
    pub fn get_binding(&self, key: &str) -> Option<&KeyLearnTarget> {
        self.bindings.get(key)
    }

    /// Total number of active key bindings.
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Process a key press event. If in learn mode, automatically creates the binding and exits learn mode.
    /// Returns `Some((target, new_value))` if a binding matched or was learned.
    pub fn process_key_press(&mut self, key: &str, current_val: f32) -> Option<(KeyLearnTarget, f32)> {
        if let Some(target) = self.pending_learn.take() {
            let new_val = target.evaluate_key_press(current_val);
            self.bindings.insert(key.to_string(), target.clone());
            return Some((target, new_val));
        }

        if let Some(target) = self.bindings.get(key) {
            let new_val = target.evaluate_key_press(current_val);
            return Some((target.clone(), new_val));
        }

        None
    }

    /// Process a key release event (for momentary non-toggle bindings).
    pub fn process_key_release(&self, key: &str) -> Option<(KeyLearnTarget, f32)> {
        if let Some(target) = self.bindings.get(key) {
            if !target.is_toggle {
                let new_val = target.evaluate_key_release();
                return Some((target.clone(), new_val));
            }
        }
        None
    }
}


/// Push RGB LED pad color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PushPadColor {
    Off,
    GreenPlaying,
    RedRecording,
    AmberStopped,
    White,
    CustomRgb(u8, u8, u8),
}

impl PushPadColor {
    pub const fn to_rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Off => (0, 0, 0),
            Self::GreenPlaying => (0, 230, 118),
            Self::RedRecording => (255, 23, 68),
            Self::AmberStopped => (255, 171, 0),
            Self::White => (255, 255, 255),
            Self::CustomRgb(r, g, b) => (*r, *g, *b),
        }
    }
}

/// Telemetry channel slice for the Push 960x160 hardware display (8 strips x 120px width).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PushTrackStripTelemetry {
    pub track_name: String,
    pub volume_db: f32,
    pub pan: f32,
    pub meter_peak: f32,
    pub active_device_name: String,
    pub parameter_name: String,
    pub parameter_value_str: String,
}

impl Default for PushTrackStripTelemetry {
    fn default() -> Self {
        Self {
            track_name: "Track".to_string(),
            volume_db: 0.0,
            pan: 0.0,
            meter_peak: 0.0,
            active_device_name: "None".to_string(),
            parameter_name: "Param".to_string(),
            parameter_value_str: "0.0".to_string(),
        }
    }
}

/// Telemetry model for the 960x160 Push 2 / Push 3 display strip.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PushDisplayStrip {
    pub strips: [PushTrackStripTelemetry; 8],
    pub tempo_bpm: f64,
    pub time_signature: (u8, u8),
}

impl Default for PushDisplayStrip {
    fn default() -> Self {
        Self {
            strips: [
                PushTrackStripTelemetry::default(),
                PushTrackStripTelemetry::default(),
                PushTrackStripTelemetry::default(),
                PushTrackStripTelemetry::default(),
                PushTrackStripTelemetry::default(),
                PushTrackStripTelemetry::default(),
                PushTrackStripTelemetry::default(),
                PushTrackStripTelemetry::default(),
            ],
            tempo_bpm: 120.0,
            time_signature: (4, 4),
        }
    }
}

/// Operating mode for the Push 2 / Push 3 ribbon touch strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PushTouchStripMode {
    /// Pitch bend mode: center position is 0.5 (8192). Finger release automatically
    /// springs back to 0.5 (zero pitch bend).
    PitchBend,
    /// Modulation wheel mode (CC 1): position holds where released.
    ModWheel,
    /// Relative navigation / scrub mode: emits continuous delta increments.
    NavScroll,
}

/// Real-time state of the Push 2 / Push 3 capacitive touch strip.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PushTouchStrip {
    pub mode: PushTouchStripMode,
    pub is_touching: bool,
    pub position: f32, // Normalized [0.0, 1.0], 0.5 = center
    pub last_touch_position: f32,
}

impl PushTouchStrip {
    pub fn new(mode: PushTouchStripMode) -> Self {
        let initial_pos = match mode {
            PushTouchStripMode::PitchBend => 0.5,
            _ => 0.0,
        };
        Self {
            mode,
            is_touching: false,
            position: initial_pos,
            last_touch_position: initial_pos,
        }
    }

    /// Finger touches down on the strip at normalized position [0.0, 1.0].
    pub fn touch_down(&mut self, pos: f32) {
        self.is_touching = true;
        self.position = pos.clamp(0.0, 1.0);
        self.last_touch_position = self.position;
    }

    /// Finger drags along the touch strip. Returns relative position delta.
    pub fn touch_move(&mut self, pos: f32) -> f32 {
        let prev = self.position;
        self.position = pos.clamp(0.0, 1.0);
        let delta = self.position - prev;
        self.last_touch_position = self.position;
        delta
    }

    /// Finger releases from the touch strip. In PitchBend mode, springs back to center (0.5).
    pub fn touch_up(&mut self) {
        self.is_touching = false;
        if self.mode == PushTouchStripMode::PitchBend {
            self.position = 0.5;
        }
    }

    /// Map current position to 14-bit MIDI pitch bend [0..16383] with center at 8192.
    pub fn pitch_bend_14bit(&self) -> u16 {
        (self.position.clamp(0.0, 1.0) * 16383.0).round() as u16
    }

    /// Map current position to 7-bit MIDI Modulation Wheel CC 1 [0..127].
    pub fn mod_wheel_cc(&self) -> u8 {
        (self.position.clamp(0.0, 1.0) * 127.0).round() as u8
    }

    /// Compute which LED index (0..total_leds-1) on the ribbon LED ladder is illuminated.
    pub fn led_meter_index(&self, total_leds: usize) -> usize {
        if total_leds == 0 {
            return 0;
        }
        let max_idx = total_leds.saturating_sub(1);
        ((self.position.clamp(0.0, 1.0) * (max_idx as f32)).round()) as usize
    }
}

impl Default for PushTouchStrip {
    fn default() -> Self {
        Self::new(PushTouchStripMode::PitchBend)
    }
}

/// Native 960x160 60 FPS display buffer for Ableton Push 2 and Push 3 hardware.
/// Pixel format: 16-bit BGR565 (307,200 bytes per 60 FPS frame).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PushDisplayFrameBuffer {
    pixels: Vec<u16>,
    pub frame_count: u64,
}

impl PushDisplayFrameBuffer {
    pub const WIDTH: usize = 960;
    pub const HEIGHT: usize = 160;
    pub const PIXEL_COUNT: usize = Self::WIDTH * Self::HEIGHT; // 153,600
    pub const FRAME_BYTES: usize = Self::PIXEL_COUNT * 2;      // 307,200 bytes
    pub const TARGET_FPS: f64 = 60.0;
    pub const FRAME_BUDGET_MS: f64 = 1000.0 / Self::TARGET_FPS; // 16.666 ms
    pub const PUSH2_USB_VID: u16 = 0x2972;
    pub const PUSH2_USB_PID: u16 = 0x1967;
    pub const PUSH2_DISPLAY_ENDPOINT: u8 = 0x01;
    pub const PUSH2_DISPLAY_XOR_MASK: u32 = 0xFFE7_F3E7;

    pub fn new() -> Self {
        Self {
            pixels: vec![0u16; Self::PIXEL_COUNT],
            frame_count: 0,
        }
    }

    /// Convert 24-bit RGB (8-8-8) to Push 2 hardware 16-bit BGR565 format.
    pub const fn rgb_to_bgr565(r: u8, g: u8, b: u8) -> u16 {
        let b5 = ((b as u16) >> 3) & 0x1F;
        let g6 = ((g as u16) >> 2) & 0x3F;
        let r5 = ((r as u16) >> 3) & 0x1F;
        (b5 << 11) | (g6 << 5) | r5
    }

    /// Convert 16-bit BGR565 format back to 24-bit RGB (8-8-8).
    pub const fn bgr565_to_rgb(bgr: u16) -> (u8, u8, u8) {
        let b5 = (bgr >> 11) & 0x1F;
        let g6 = (bgr >> 5) & 0x3F;
        let r5 = bgr & 0x1F;
        let r = ((r5 * 255 + 15) / 31) as u8;
        let g = ((g6 * 255 + 31) / 63) as u8;
        let b = ((b5 * 255 + 15) / 31) as u8;
        (r, g, b)
    }

    /// Clear entire display to given 16-bit BGR565 color.
    pub fn clear(&mut self, bgr565: u16) {
        self.pixels.fill(bgr565);
    }

    /// Set an individual pixel in the 960x160 frame.
    pub fn set_pixel(&mut self, x: usize, y: usize, bgr565: u16) {
        if x < Self::WIDTH && y < Self::HEIGHT {
            self.pixels[y * Self::WIDTH + x] = bgr565;
        }
    }

    /// Retrieve an individual pixel from the 960x160 frame.
    pub fn get_pixel(&self, x: usize, y: usize) -> u16 {
        if x < Self::WIDTH && y < Self::HEIGHT {
            self.pixels[y * Self::WIDTH + x]
        } else {
            0
        }
    }

    /// Fill a solid rectangle within the 960x160 canvas.
    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, bgr565: u16) {
        let max_x = (x + w).min(Self::WIDTH);
        let max_y = (y + h).min(Self::HEIGHT);
        for py in y..max_y {
            let row_offset = py * Self::WIDTH;
            for px in x..max_x {
                self.pixels[row_offset + px] = bgr565;
            }
        }
    }

    /// Render a single 120-pixel track telemetry strip onto the 960x160 display.
    pub fn render_track_strip(&mut self, strip_idx: usize, telemetry: &PushTrackStripTelemetry) {
        if strip_idx >= 8 {
            return;
        }
        let start_x = strip_idx * 120;
        let bg_color = Self::rgb_to_bgr565(36, 36, 36);
        let border_color = Self::rgb_to_bgr565(60, 60, 60);

        // Fill strip background (120x160)
        self.fill_rect(start_x, 0, 120, Self::HEIGHT, bg_color);

        // Separator vertical line on the right edge
        for py in 0..Self::HEIGHT {
            self.set_pixel(start_x + 119, py, border_color);
        }

        // Header region for track name (height 28)
        let header_color = Self::rgb_to_bgr565(50, 50, 50);
        self.fill_rect(start_x, 0, 119, 28, header_color);

        // Peak volume meter (x in [start_x + 8, start_x + 20], y in [35, 145], height = 110)
        let meter_max_h = 110.0f32;
        let peak = telemetry.meter_peak.clamp(0.0, 1.0);
        let filled_h = (peak * meter_max_h).round() as usize;
        let meter_bg = Self::rgb_to_bgr565(20, 20, 20);
        self.fill_rect(start_x + 8, 35, 12, 110, meter_bg);

        for h in 0..filled_h {
            let y = 145 - 1 - h;
            let meter_color = if h < 70 {
                Self::rgb_to_bgr565(0, 230, 118) // Green normal
            } else if h < 95 {
                Self::rgb_to_bgr565(255, 171, 0) // Amber warning
            } else {
                Self::rgb_to_bgr565(255, 23, 68)  // Red peak
            };
            for px in (start_x + 8)..(start_x + 20) {
                self.set_pixel(px, y, meter_color);
            }
        }

        // Parameter slider bar (x in [start_x + 30, start_x + 110], y in [120, 132])
        let param_bg = Self::rgb_to_bgr565(24, 24, 24);
        self.fill_rect(start_x + 30, 120, 80, 12, param_bg);
        let norm_vol = ((telemetry.volume_db + 60.0) / 66.0).clamp(0.0, 1.0);
        let vol_w = (norm_vol * 80.0).round() as usize;
        let vol_color = Self::rgb_to_bgr565(3, 195, 213); // Cyan
        self.fill_rect(start_x + 30, 120, vol_w, 12, vol_color);
    }

    /// Render all 8 track strips and update frame count.
    pub fn render_full_display(&mut self, display: &PushDisplayStrip) {
        for (i, strip) in display.strips.iter().enumerate() {
            self.render_track_strip(i, strip);
        }
        self.frame_count += 1;
    }

    /// Apply Push 2 USB EMI-reduction XOR scrambling mask (0xFFE7_F3E7).
    pub fn apply_push2_usb_xor_mask(&mut self) {
        let mask_low = (Self::PUSH2_DISPLAY_XOR_MASK & 0xFFFF) as u16;
        let mask_high = ((Self::PUSH2_DISPLAY_XOR_MASK >> 16) & 0xFFFF) as u16;
        for (i, px) in self.pixels.iter_mut().enumerate() {
            let mask = if i % 2 == 0 { mask_low } else { mask_high };
            *px ^= mask;
        }
    }

    /// Access raw 307,200-byte frame payload for high-speed USB endpoint streaming.
    pub fn as_raw_bytes(&self) -> &[u8] {
        let ptr = self.pixels.as_ptr() as *const u8;
        let len = self.pixels.len() * 2;
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

impl Default for PushDisplayFrameBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// State of the Ableton Push 2 / Push 3 8x8 RGB pad matrix and display strip.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PushPadMatrixState {
    pub pads: [[PushPadColor; 8]; 8],
    pub root_note: u8,
    pub row_interval: u8,
    pub display_strip: PushDisplayStrip,
    pub touch_strip: PushTouchStrip,
    pub frame_buffer: PushDisplayFrameBuffer,
}

impl PushPadMatrixState {
    pub fn new() -> Self {
        Self {
            pads: [[PushPadColor::Off; 8]; 8],
            root_note: 36, // C1 (Standard electronic bass/drum root)
            row_interval: 5, // Fourths layout (standard Ableton Push isomorphic layout)
            display_strip: PushDisplayStrip::default(),
            touch_strip: PushTouchStrip::new(PushTouchStripMode::PitchBend),
            frame_buffer: PushDisplayFrameBuffer::new(),
        }
    }

    /// Set an individual pad color.
    pub fn set_pad(&mut self, row: usize, col: usize, color: PushPadColor) {
        if row < 8 && col < 8 {
            self.pads[row][col] = color;
        }
    }

    /// Retrieve an individual pad color.
    pub fn get_pad(&self, row: usize, col: usize) -> PushPadColor {
        if row < 8 && col < 8 {
            self.pads[row][col]
        } else {
            PushPadColor::Off
        }
    }

    /// Synchronize 8x8 matrix LED colors from Session clip states.
    /// Code mappings: 0 = Empty (Off), 1 = Stopped (Amber), 2 = Playing (Green), 3 = Recording (Red).
    pub fn sync_session_clip_state_matrix(&mut self, clip_states: &[[u8; 8]; 8]) {
        for r in 0..8 {
            for c in 0..8 {
                self.pads[r][c] = match clip_states[r][c] {
                    1 => PushPadColor::AmberStopped,
                    2 => PushPadColor::GreenPlaying,
                    3 => PushPadColor::RedRecording,
                    _ => PushPadColor::Off,
                };
            }
        }
    }

    /// Configure isomorphic MPE pad layout.
    pub fn set_isomorphic_layout(&mut self, root_note: u8, row_interval: u8) {
        self.root_note = root_note;
        self.row_interval = row_interval;
    }

    /// Map a pad coordinate `(row, col)` to its corresponding MIDI note number in isomorphic mode.
    /// Row 0 is the bottom row; Row 7 is the top row.
    pub fn pad_to_midi_note(&self, row: usize, col: usize) -> u8 {
        let note = (self.root_note as usize) + row * (self.row_interval as usize) + col;
        (note.min(127)) as u8
    }

    /// Rasterize current display telemetry into the 960x160 60 FPS frame buffer.
    pub fn render_frame_buffer(&mut self) {
        self.frame_buffer.render_full_display(&self.display_strip);
    }
}

impl Default for PushPadMatrixState {
    fn default() -> Self {
        Self::new()
    }
}

/// Akai APC40 mkII control surface state (5x8 clip matrix, 9 faders, crossfader, 16 knobs).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Apc40State {
    pub clips: [[u8; 8]; 5],       // 5 scenes x 8 tracks: 0=Off, 1=Amber, 2=Green, 3=Red
    pub channel_faders: [f32; 8],  // Normalized [0.0, 1.0]
    pub master_fader: f32,         // Normalized [0.0, 1.0]
    pub crossfader: f32,           // Bipolar [-1.0, 1.0], 0.0 = center
    pub device_knobs: [f32; 8],    // 8 Device Control endless encoders
    pub track_knobs: [f32; 8],     // 8 Track Pan/Send encoders
}

impl Apc40State {
    pub fn new() -> Self {
        Self {
            clips: [[0u8; 8]; 5],
            channel_faders: [0.75; 8], // Default ~0 dB level
            master_fader: 0.75,
            crossfader: 0.0,
            device_knobs: [0.5; 8],
            track_knobs: [0.5; 8],
        }
    }

    pub fn set_clip_state(&mut self, scene: usize, track: usize, state: u8) {
        if scene < 5 && track < 8 {
            self.clips[scene][track] = state;
        }
    }

    pub fn get_clip_state(&self, scene: usize, track: usize) -> u8 {
        if scene < 5 && track < 8 {
            self.clips[scene][track]
        } else {
            0
        }
    }

    pub fn set_channel_fader(&mut self, track: usize, val: f32) {
        if track < 8 {
            self.channel_faders[track] = val.clamp(0.0, 1.0);
        }
    }

    pub fn get_channel_fader(&self, track: usize) -> f32 {
        if track < 8 {
            self.channel_faders[track]
        } else {
            0.0
        }
    }

    pub fn set_master_fader(&mut self, val: f32) {
        self.master_fader = val.clamp(0.0, 1.0);
    }

    pub fn set_crossfader(&mut self, val: f32) {
        self.crossfader = val.clamp(-1.0, 1.0);
    }
}

impl Default for Apc40State {
    fn default() -> Self {
        Self::new()
    }
}

/// Novation Launchpad Pro MK3 control surface state (8x8 RGB pads, poly aftertouch, scene & function buttons).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LaunchpadProState {
    pub pads: [[PushPadColor; 8]; 8],
    pub pressure: [[u8; 8]; 8],              // Polyphonic aftertouch pressure [0..127]
    pub scene_buttons: [PushPadColor; 8],    // 8 right-column scene launch buttons
    pub function_buttons: [bool; 8],         // 8 top-row mode/function buttons
}

impl LaunchpadProState {
    pub fn new() -> Self {
        Self {
            pads: [[PushPadColor::Off; 8]; 8],
            pressure: [[0u8; 8]; 8],
            scene_buttons: [PushPadColor::Off; 8],
            function_buttons: [false; 8],
        }
    }

    pub fn set_pad(&mut self, row: usize, col: usize, color: PushPadColor) {
        if row < 8 && col < 8 {
            self.pads[row][col] = color;
        }
    }

    pub fn get_pad(&self, row: usize, col: usize) -> PushPadColor {
        if row < 8 && col < 8 {
            self.pads[row][col]
        } else {
            PushPadColor::Off
        }
    }

    pub fn set_pad_pressure(&mut self, row: usize, col: usize, pressure: u8) {
        if row < 8 && col < 8 {
            self.pressure[row][col] = pressure.min(127);
        }
    }

    pub fn get_pad_pressure(&self, row: usize, col: usize) -> u8 {
        if row < 8 && col < 8 {
            self.pressure[row][col]
        } else {
            0
        }
    }

    pub fn set_scene_button(&mut self, idx: usize, color: PushPadColor) {
        if idx < 8 {
            self.scene_buttons[idx] = color;
        }
    }

    pub fn get_scene_button(&self, idx: usize) -> PushPadColor {
        if idx < 8 {
            self.scene_buttons[idx]
        } else {
            PushPadColor::Off
        }
    }

    pub fn set_function_button(&mut self, idx: usize, active: bool) {
        if idx < 8 {
            self.function_buttons[idx] = active;
        }
    }

    pub fn get_function_button(&self, idx: usize) -> bool {
        if idx < 8 {
            self.function_buttons[idx]
        } else {
            false
        }
    }
}

impl Default for LaunchpadProState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_takeover_pickup_logic() {
        let mode = TakeoverMode::Pickup;
        let current_soft = 0.63; // ~80 in 0..127 range (80 / 127 = 0.6299)

        // Moving hardware from 0.0 upwards
        let prev_hw = 0.0;
        let new_hw = 0.3;
        // Not crossed yet -> must return None (no audio jump)
        assert_eq!(mode.evaluate_takeover(current_soft, prev_hw, new_hw), None);

        let prev_hw2 = 0.3;
        let new_hw2 = 0.5;
        // Still below 0.63 -> None
        assert_eq!(mode.evaluate_takeover(current_soft, prev_hw2, new_hw2), None);

        let prev_hw3 = 0.5;
        let new_hw3 = 0.7;
        // Crossed 0.63! Takeover acquired -> Some(0.7)
        assert_eq!(mode.evaluate_takeover(current_soft, prev_hw3, new_hw3), Some(0.7));

        // Subsequent movement remains in sync
        let prev_hw4 = 0.7;
        let new_hw4 = 0.85;
        // Crossing soft (0.63) again? Wait, once pickup is reached, or crossing:
        // When already past it and moving away:
        // Here, crossing checks if interval encompasses current_soft:
        assert_eq!(mode.evaluate_takeover(0.7, prev_hw4, new_hw4), Some(0.85));
    }

    #[test]
    fn test_takeover_value_scaling_convergence() {
        let mode = TakeoverMode::ValueScaling;
        let current_soft = 0.5;
        let prev_hw = 0.0;
        let new_hw = 0.1;

        // Software moves proportionally without jumping to 0.1
        let val1 = mode.evaluate_takeover(current_soft, prev_hw, new_hw).unwrap();
        assert!(val1 > 0.5);
        assert!(val1 < 0.6); // smooth scaling

        // Hardware reaches 1.0 -> software converges smoothly to exactly 1.0
        let val_end = mode.evaluate_takeover(0.8, 0.8, 1.0).unwrap();
        assert_eq!(val_end, 1.0);

        // Downward sweep: hardware reaches 0.0 -> software converges to 0.0
        let val_zero = mode.evaluate_takeover(0.2, 0.2, 0.0).unwrap();
        assert_eq!(val_zero, 0.0);
    }

    #[test]
    fn test_midi_learn_registry_binding_and_dispatch() {
        let mut registry = MidiLearnRegistry::new();
        assert!(!registry.is_learning());

        let target = MidiLearnTarget::new(1, 0, 7, -60.0, 6.0); // Track 1, Device 0, Volume (-60 to +6 dB)

        // Arm learn mode
        registry.learn_next_cc(target.clone());
        assert!(registry.is_learning());

        // First incoming CC (Ch 0, CC 14, value 64) automatically binds and maps
        let res = registry.process_midi_cc(0, 14, 64);
        assert!(res.is_some());
        let (learned_target, mapped_val) = res.unwrap();
        assert_eq!(learned_target, target);
        assert!(!registry.is_learning()); // Learn mode consumed
        assert_eq!(registry.binding_count(), 1);

        // Mapped value around -27 dB
        assert!((mapped_val - (-26.69)).abs() < 1.0);

        // Subsequent CC 14 events dispatch to the learned target
        let res2 = registry.process_midi_cc(0, 14, 127);
        assert!(res2.is_some());
        let (_, max_val) = res2.unwrap();
        assert_eq!(max_val, 6.0);
    }

    #[test]
    fn test_push_pad_matrix_state_and_display_strip() {
        let mut push = PushPadMatrixState::new();

        // Check defaults
        assert_eq!(push.get_pad(0, 0), PushPadColor::Off);
        assert_eq!(push.pad_to_midi_note(0, 0), 36); // C1
        assert_eq!(push.pad_to_midi_note(1, 0), 41); // F1 (+5 fourths)

        // Sync session matrix clip states
        let mut clip_states = [[0u8; 8]; 8];
        clip_states[0][0] = 2; // Playing -> Green
        clip_states[0][1] = 1; // Stopped -> Amber
        clip_states[0][2] = 3; // Recording -> Red

        push.sync_session_clip_state_matrix(&clip_states);
        assert_eq!(push.get_pad(0, 0), PushPadColor::GreenPlaying);
        assert_eq!(push.get_pad(0, 1), PushPadColor::AmberStopped);
        assert_eq!(push.get_pad(0, 2), PushPadColor::RedRecording);
        assert_eq!(push.get_pad(0, 3), PushPadColor::Off);

        // Telemetry display strip
        push.display_strip.strips[0].track_name = "Kick 909".to_string();
        push.display_strip.strips[0].parameter_name = "Decay".to_string();
        push.display_strip.strips[0].parameter_value_str = "450 ms".to_string();

        assert_eq!(push.display_strip.strips[0].track_name, "Kick 909");
        assert_eq!(ControlSurfaceModel::Push3.pad_matrix_dimensions(), (8, 8));
        assert_eq!(ControlSurfaceModel::Push3.display_resolution(), Some((960, 160)));
    }

    #[test]
    fn test_parameter_min_max_scaling_and_inversion() {
        // Standard range: 0.0 to 100.0
        let target_norm = MidiLearnTarget::new(0, 0, 1, 0.0, 100.0);
        assert_eq!(target_norm.map_cc_to_value(0), 0.0);
        assert_eq!(target_norm.map_cc_to_value(127), 100.0);
        assert!((target_norm.map_cc_to_value(64) - 50.3937).abs() < 0.1);
        // CC clamping beyond 127
        assert_eq!(target_norm.map_cc_to_value(200), 100.0);

        // Bidirectional feedback mapping
        assert_eq!(target_norm.map_value_to_cc(0.0), 0);
        assert_eq!(target_norm.map_value_to_cc(100.0), 127);
        assert_eq!(target_norm.map_value_to_cc(50.0), 64);

        // Inverted range: 100.0 (at CC 0) to 0.0 (at CC 127)
        let target_inv = MidiLearnTarget::new(0, 0, 2, 100.0, 0.0);
        assert_eq!(target_inv.map_cc_to_value(0), 100.0);
        assert_eq!(target_inv.map_cc_to_value(127), 0.0);
        assert!((target_inv.map_cc_to_value(64) - 49.606).abs() < 0.1);

        // Negative to positive range: -12.0 dB to +12.0 dB
        let target_pan = MidiLearnTarget::new(0, 0, 3, -12.0, 12.0);
        assert_eq!(target_pan.map_cc_to_value(0), -12.0);
        assert_eq!(target_pan.map_cc_to_value(127), 12.0);
        assert!((target_pan.map_cc_to_value(64) - 0.094).abs() < 0.1);
    }

    #[test]
    fn test_midi_note_learn_and_remote_control() {
        let mut registry = MidiLearnRegistry::new();
        assert!(!registry.is_learning_note());
        assert_eq!(registry.note_binding_count(), 0);

        let target_mute = MidiLearnTarget::new(0, 0, 10, 0.0, 1.0);

        // Arm learn note
        registry.learn_next_note(target_mute.clone());
        assert!(registry.is_learning_note());

        // Note-off (velocity 0) during learn is ignored
        assert!(registry.process_midi_note(0, 60, 0, 0.0).is_none());
        assert!(registry.is_learning_note());

        // NoteOn (channel 0, note 60, velocity 127) learns target and toggles from 0.0 to 1.0
        let res = registry.process_midi_note(0, 60, 127, 0.0);
        assert!(res.is_some());
        let (learned, val) = res.unwrap();
        assert_eq!(learned, target_mute);
        assert_eq!(val, 1.0);
        assert!(!registry.is_learning_note());
        assert_eq!(registry.note_binding_count(), 1);
        assert_eq!(registry.total_binding_count(), 1);

        // Subsequent NoteOn toggles back to 0.0
        let res2 = registry.process_midi_note(0, 60, 100, 1.0);
        assert!(res2.is_some());
        let (_, val2) = res2.unwrap();
        assert_eq!(val2, 0.0);

        // Unbind note
        assert!(registry.unbind_note(0, 60).is_some());
        assert_eq!(registry.note_binding_count(), 0);
    }

    #[test]
    fn test_key_learn_registry_binding_toggle_and_momentary() {
        let mut key_reg = KeyLearnRegistry::new();
        assert!(!key_reg.is_learning());
        assert_eq!(key_reg.binding_count(), 0);

        // Target: Track Solo (Toggle, min 0.0, max 1.0)
        let target_solo = KeyLearnTarget::new(1, 0, 20, 0.0, 1.0);

        // Arm key learn
        key_reg.learn_next_key(target_solo.clone());
        assert!(key_reg.is_learning());

        // Key 'S' pressed while solo is 0.0 -> binds 'S' and returns 1.0
        let res = key_reg.process_key_press("KeyS", 0.0);
        assert!(res.is_some());
        let (learned, val) = res.unwrap();
        assert_eq!(learned, target_solo);
        assert_eq!(val, 1.0);
        assert!(!key_reg.is_learning());
        assert_eq!(key_reg.binding_count(), 1);

        // Subsequent 'S' press toggles back to 0.0
        let res2 = key_reg.process_key_press("KeyS", 1.0);
        assert_eq!(res2.unwrap().1, 0.0);

        // Momentary Key binding (e.g. Play / Tap, press 1.0, release 0.0)
        let target_momentary = KeyLearnTarget::new(0, 0, 99, 0.0, 1.0).with_toggle(false);
        key_reg.bind("Space", target_momentary);
        assert_eq!(key_reg.binding_count(), 2);

        // Press sets max_val (1.0)
        let press_res = key_reg.process_key_press("Space", 0.0);
        assert_eq!(press_res.unwrap().1, 1.0);

        // Release resets min_val (0.0)
        let rel_res = key_reg.process_key_release("Space");
        assert_eq!(rel_res.unwrap().1, 0.0);

        // Inverted range Key mapping (min 100.0, max 0.0)
        let target_inv = KeyLearnTarget::new(2, 0, 30, 100.0, 0.0);
        key_reg.bind("KeyX", target_inv);
        // If current value is 100.0, toggle goes to 0.0
        assert_eq!(key_reg.process_key_press("KeyX", 100.0).unwrap().1, 0.0);
        // If current value is 0.0, toggle goes to 100.0
        assert_eq!(key_reg.process_key_press("KeyX", 0.0).unwrap().1, 100.0);
    }

    #[test]
    fn test_push_touch_strip_pitch_bend_and_mod_wheel() {
        // Pitch bend mode (default)
        let mut touch_strip = PushTouchStrip::new(PushTouchStripMode::PitchBend);
        assert_eq!(touch_strip.position, 0.5); // Center default
        assert_eq!(touch_strip.pitch_bend_14bit(), 8192); // Center 14-bit pitch bend
        assert!(!touch_strip.is_touching);

        // Touch down at 0.75
        touch_strip.touch_down(0.75);
        assert!(touch_strip.is_touching);
        assert_eq!(touch_strip.position, 0.75);
        assert_eq!(touch_strip.pitch_bend_14bit(), 12287);

        // Touch move up to 1.0 (maximum pitch bend)
        let delta = touch_strip.touch_move(1.0);
        assert!((delta - 0.25).abs() < 1e-4);
        assert_eq!(touch_strip.position, 1.0);
        assert_eq!(touch_strip.pitch_bend_14bit(), 16383);

        // Touch release: PitchBend mode must spring back to 0.5 (center)
        touch_strip.touch_up();
        assert!(!touch_strip.is_touching);
        assert_eq!(touch_strip.position, 0.5);
        assert_eq!(touch_strip.pitch_bend_14bit(), 8192);

        // Modulation wheel mode (CC 1)
        let mut mod_strip = PushTouchStrip::new(PushTouchStripMode::ModWheel);
        assert_eq!(mod_strip.position, 0.0);
        assert_eq!(mod_strip.mod_wheel_cc(), 0);

        // Touch down at 0.8
        mod_strip.touch_down(0.8);
        assert_eq!(mod_strip.mod_wheel_cc(), 102);

        // Touch release: ModWheel mode holds position
        mod_strip.touch_up();
        assert_eq!(mod_strip.position, 0.8);
        assert_eq!(mod_strip.mod_wheel_cc(), 102);

        // LED ladder feedback calculation across 24 LEDs
        assert_eq!(mod_strip.led_meter_index(24), 18);
    }

    #[test]
    fn test_push_display_frame_buffer_bgr565_rasterization_and_xor_mask() {
        let mut fb = PushDisplayFrameBuffer::new();

        // Check geometry and timing specifications
        assert_eq!(PushDisplayFrameBuffer::WIDTH, 960);
        assert_eq!(PushDisplayFrameBuffer::HEIGHT, 160);
        assert_eq!(PushDisplayFrameBuffer::PIXEL_COUNT, 153_600);
        assert_eq!(PushDisplayFrameBuffer::FRAME_BYTES, 307_200);
        assert_eq!(PushDisplayFrameBuffer::TARGET_FPS, 60.0);
        assert!((PushDisplayFrameBuffer::FRAME_BUDGET_MS - 16.6666).abs() < 0.01);
        assert_eq!(PushDisplayFrameBuffer::PUSH2_USB_VID, 0x2972);
        assert_eq!(PushDisplayFrameBuffer::PUSH2_DISPLAY_ENDPOINT, 0x01);

        // Check BGR565 color conversions
        let red_bgr = PushDisplayFrameBuffer::rgb_to_bgr565(255, 0, 0);
        assert_eq!(PushDisplayFrameBuffer::bgr565_to_rgb(red_bgr), (255, 0, 0));

        let green_bgr = PushDisplayFrameBuffer::rgb_to_bgr565(0, 255, 0);
        assert_eq!(PushDisplayFrameBuffer::bgr565_to_rgb(green_bgr), (0, 255, 0));

        let blue_bgr = PushDisplayFrameBuffer::rgb_to_bgr565(0, 0, 255);
        assert_eq!(PushDisplayFrameBuffer::bgr565_to_rgb(blue_bgr), (0, 0, 255));

        // Clear and pixel inspection
        fb.clear(0);
        assert_eq!(fb.get_pixel(0, 0), 0);
        assert_eq!(fb.get_pixel(959, 159), 0);

        // Rasterize display strips
        let mut display = PushDisplayStrip::default();
        display.strips[0].track_name = "Lead Synth".to_string();
        display.strips[0].meter_peak = 0.85;
        display.strips[0].volume_db = -6.0;

        fb.render_full_display(&display);
        assert_eq!(fb.frame_count, 1);

        // Strip 0 separator line at x=119
        let border_color = PushDisplayFrameBuffer::rgb_to_bgr565(60, 60, 60);
        assert_eq!(fb.get_pixel(119, 50), border_color);

        // Raw bytes size must equal exactly 307,200 bytes
        let raw = fb.as_raw_bytes();
        assert_eq!(raw.len(), 307_200);

        // Push 2 XOR scrambling mask verification
        let original_px0 = fb.get_pixel(0, 0);
        let original_px1 = fb.get_pixel(1, 0);
        fb.apply_push2_usb_xor_mask();
        assert_ne!(fb.get_pixel(0, 0), original_px0);
        assert_ne!(fb.get_pixel(1, 0), original_px1);

        // Double XOR recovers exact original image
        fb.apply_push2_usb_xor_mask();
        assert_eq!(fb.get_pixel(0, 0), original_px0);
        assert_eq!(fb.get_pixel(1, 0), original_px1);
    }

    #[test]
    fn test_apc40_and_launchpad_pro_control_surface_states() {
        // APC40 mkII
        let mut apc = Apc40State::new();
        assert_eq!(ControlSurfaceModel::Apc40MkII.pad_matrix_dimensions(), (5, 8));
        assert_eq!(ControlSurfaceModel::Apc40MkII.fader_count(), 9);
        assert_eq!(ControlSurfaceModel::Apc40MkII.encoder_count(), 16);
        assert!(!ControlSurfaceModel::Apc40MkII.has_display());
        assert!(!ControlSurfaceModel::Apc40MkII.has_touch_strip());

        apc.set_clip_state(0, 1, 2); // Scene 0, Track 1: Green playing
        assert_eq!(apc.get_clip_state(0, 1), 2);

        apc.set_channel_fader(0, 0.85);
        assert_eq!(apc.get_channel_fader(0), 0.85);

        apc.set_crossfader(-0.5);
        assert_eq!(apc.crossfader, -0.5);

        // Launchpad Pro MK3
        let mut lp = LaunchpadProState::new();
        assert_eq!(ControlSurfaceModel::LaunchpadProMk3.pad_matrix_dimensions(), (8, 8));
        assert!(ControlSurfaceModel::LaunchpadProMk3.has_mpe());
        assert!(!ControlSurfaceModel::LaunchpadProMk3.has_display());

        lp.set_pad(2, 3, PushPadColor::AmberStopped);
        assert_eq!(lp.get_pad(2, 3), PushPadColor::AmberStopped);

        lp.set_pad_pressure(2, 3, 98);
        assert_eq!(lp.get_pad_pressure(2, 3), 98);

        lp.set_scene_button(0, PushPadColor::GreenPlaying);
        assert_eq!(lp.get_scene_button(0), PushPadColor::GreenPlaying);

        lp.set_function_button(1, true);
        assert!(lp.get_function_button(1));

        // Push 2 and Push 3 capabilities
        assert!(ControlSurfaceModel::Push2.has_display());
        assert!(ControlSurfaceModel::Push2.has_touch_strip());
        assert!(ControlSurfaceModel::Push3.has_display());
        assert!(ControlSurfaceModel::Push3.has_touch_strip());
        assert!(ControlSurfaceModel::Push3.has_mpe());
    }
}


