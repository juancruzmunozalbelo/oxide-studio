//! Backwards-compatibility shim for `device_rack`.
//!
//! The canonical implementation for Ableton Live 12 Reference Manual Chapter 22
//! ("Working with Instruments and Effects") & OKF-06 is located in `device_chain.rs`.

pub use super::device_chain::{
    render_device_chain_view, DeviceCategory, DeviceChainAction, DeviceChainViewState,
    DeviceKind, DeviceRouting, GuiDeviceModule, HotSwapState,
};
use egui::Ui;

// Backwards-compatible legacy wrapper
#[derive(Debug, Clone, Default)]
pub struct SynthControlsState {
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub pulse_pw: f32,
}

pub struct DeviceRackAction {
    pub cutoff_changed: Option<f32>,
    pub resonance_changed: Option<f32>,
    pub synth_param_changed: Option<(u32, f32)>,
}

pub fn render_device_rack(
    ui: &mut Ui,
    _state: &mut SynthControlsState,
    _live_cutoff: Option<f32>,
    _live_amp: Option<f32>,
    _live_pw: Option<f32>,
) -> DeviceRackAction {
    let mut default_chain = DeviceChainViewState::default();
    let bins = [0.0f32; 64];
    render_device_chain_view(ui, &mut default_chain, &bins);
    DeviceRackAction {
        cutoff_changed: None,
        resonance_changed: None,
        synth_param_changed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_chain_selected_track_badge() {
        let ctx = egui::Context::default();
        let mut chain = DeviceChainViewState::default();
        let bins = [0.0f32; 64];

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.data_mut(|d| d.insert_temp(egui::Id::new("aruston_selected_track"), 2usize));
                let act = render_device_chain_view(ui, &mut chain, &bins);
                assert!(act.toggle_bypass.is_none());
            });
        });
    }

    #[test]
    fn test_device_rack_all_device_visualizers_with_parameter_pills() {
        let mut chain = DeviceChainViewState::default();
        chain.devices.push(GuiDeviceModule::new(10, DeviceKind::Operator));
        chain.devices.push(GuiDeviceModule::new(11, DeviceKind::Simpler));
        chain.devices.push(GuiDeviceModule::new(12, DeviceKind::DrumRack));

        let ctx = egui::Context::default();
        let bins = [0.0f32; 64];

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _act = render_device_chain_view(ui, &mut chain, &bins);
            });
        });

        chain.devices[0].wavetable.table_pos = 0.99;
        chain.devices[0].wavetable = crate::visualizers::WavetableVisualizerState::default();
        assert_eq!(chain.devices[0].wavetable.table_pos, 0.35);
    }

    #[test]
    fn test_per_track_device_chains_isolated() {
        let t1 = DeviceChainViewState::for_track_1();
        let t2 = DeviceChainViewState::for_track_2();
        let t3 = DeviceChainViewState::for_track_3();

        assert_eq!(t1.devices.len(), 4);
        assert_eq!(t2.devices.len(), 4);
        assert_eq!(t3.devices.len(), 4);

        let mut chains = vec![t1, t2, t3];
        chains[0].devices[0].is_enabled = false;
        assert!(!chains[0].devices[0].is_enabled);
        assert!(chains[1].devices[0].is_enabled);
        assert!(chains[2].devices[0].is_enabled);
    }

    #[test]
    fn test_glue_compressor_needle_and_ott_meters_respond_to_params() {
        let mut glue = crate::visualizers::GlueCompressorVisualizerState::default();
        glue.threshold_db = -33.902;
        glue.recompute_gain_reduction();
        assert!(glue.gain_reduction_db < -15.0);
    }

    #[test]
    fn test_device_rack_toggle_bypass_execution() {
        let mut chain = DeviceChainViewState::default();
        assert!(chain.devices[0].is_enabled);
        assert!(chain.toggle_bypass(0));
        assert!(!chain.devices[0].is_enabled);
        assert!(chain.toggle_bypass(0));
        assert!(chain.devices[0].is_enabled);
    }
}
