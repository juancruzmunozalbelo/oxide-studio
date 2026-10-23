//! Legacy action definitions for transport panel.

pub struct LegacyTransportBarAction {
    pub toggle_play: bool,
    pub stop: bool,
    pub toggle_record: bool,
    pub export_wav: bool,
    pub bpm_changed: Option<f64>,
    pub swing_changed: Option<f32>,
}
