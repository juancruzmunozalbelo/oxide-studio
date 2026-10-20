//! Device Rack panel: Synth controls and interactive ZDF curve canvas.

use egui::{vec2, ComboBox, RichText, Ui};
use crate::theme::{COLOR_AMBER, COLOR_CYAN, COLOR_TEXT_MUTED};
use crate::visualizers::ZdfCurveWidget;
use crate::widgets::GhostSlider;

pub struct DeviceRackAction {
    pub cutoff_changed: Option<f32>,
    pub resonance_changed: Option<f32>,
    pub synth_param_changed: Option<(u32, f32)>,
}

#[derive(Debug, Clone)]
pub struct SynthControlsState {
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    pub saw_level: f32,
    pub pulse_level: f32,
    pub pulse_pw: f32,
    pub noise_level: f32,
    pub lfo_rate_hz: f32,
    pub lfo_shape: u8,
}

impl Default for SynthControlsState {
    fn default() -> Self {
        Self {
            cutoff_hz: 1250.0,
            resonance: 0.65,
            attack_ms: 10.0,
            decay_ms: 250.0,
            sustain: 0.4,
            release_ms: 300.0,
            saw_level: 0.8,
            pulse_level: 0.5,
            pulse_pw: 0.5,
            noise_level: 0.05,
            lfo_rate_hz: 2.0,
            lfo_shape: 0,
        }
    }
}

pub fn render_device_rack(
    ui: &mut Ui,
    state: &mut SynthControlsState,
    live_cutoff: Option<f32>,
    live_amp_env: Option<f32>,
    live_pw: Option<f32>,
) -> DeviceRackAction {
    let mut action = DeviceRackAction {
        cutoff_changed: None,
        resonance_changed: None,
        synth_param_changed: None,
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);

        // Column 1: Oscillators & Mix
        ui.vertical(|ui| {
            ui.set_width(170.0);
            ui.label(RichText::new("PolyBLEP Oscillators").size(12.0).color(COLOR_AMBER).strong());
            ui.separator();

            let prev_saw = state.saw_level;
            ui.add(GhostSlider::new("Saw Level", &mut state.saw_level, 0.0, 1.0));
            if state.saw_level != prev_saw {
                action.synth_param_changed = Some((1, state.saw_level));
            }

            let prev_pulse = state.pulse_level;
            ui.add(GhostSlider::new("Pulse Level", &mut state.pulse_level, 0.0, 1.0));
            if state.pulse_level != prev_pulse {
                action.synth_param_changed = Some((2, state.pulse_level));
            }

            let prev_pw = state.pulse_pw;
            ui.add(GhostSlider::new("Pulse Width", &mut state.pulse_pw, 0.05, 0.95).with_ghost(live_pw));
            if state.pulse_pw != prev_pw {
                action.synth_param_changed = Some((3, state.pulse_pw));
            }

            let prev_noise = state.noise_level;
            ui.add(GhostSlider::new("White Noise", &mut state.noise_level, 0.0, 1.0));
            if state.noise_level != prev_noise {
                action.synth_param_changed = Some((4, state.noise_level));
            }
        });

        ui.separator();

        // Column 2: 4-Pole Mystran ZDF Ladder Filter controls & interactive curve
        ui.vertical(|ui| {
            ui.set_width(320.0);
            ui.label(RichText::new("Mystran 2x-ZDF 4-Pole Ladder").size(12.0).color(COLOR_AMBER).strong());
            ui.separator();

            // Interactive ZDF Frequency-Response Curve
            let prev_fc = state.cutoff_hz;
            let prev_res = state.resonance;

            let curve = ZdfCurveWidget::new(
                &mut state.cutoff_hz,
                &mut state.resonance,
                live_cutoff,
                vec2(310.0, 110.0),
            );
            ui.add(curve);

            if state.cutoff_hz != prev_fc {
                action.cutoff_changed = Some(state.cutoff_hz);
            }
            if state.resonance != prev_res {
                action.resonance_changed = Some(state.resonance);
            }

            // Slider backups
            ui.add(
                GhostSlider::new("Cutoff Hz", &mut state.cutoff_hz, 20.0, 15000.0)
                    .with_ghost(live_cutoff)
                    .with_format(|v| format!("{:.0} Hz", v)),
            );
            ui.add(
                GhostSlider::new("Resonance", &mut state.resonance, 0.0, 1.0)
                    .with_format(|v| format!("{:.2}", v)),
            );
        });

        ui.separator();

        // Column 3: Envelopes (ADSR) with animated live amplitude indicator
        ui.vertical(|ui| {
            ui.set_width(170.0);
            ui.label(RichText::new("ADSR Envelopes").size(12.0).color(COLOR_CYAN).strong());
            ui.separator();

            ui.add(
                GhostSlider::new("Attack", &mut state.attack_ms, 1.0, 2000.0)
                    .with_format(|v| format!("{:.0} ms", v)),
            );
            ui.add(
                GhostSlider::new("Decay", &mut state.decay_ms, 5.0, 3000.0)
                    .with_format(|v| format!("{:.0} ms", v)),
            );
            ui.add(
                GhostSlider::new("Sustain", &mut state.sustain, 0.0, 1.0)
                    .with_ghost(live_amp_env)
                    .with_format(|v| format!("{:.2}", v)),
            );
            ui.add(
                GhostSlider::new("Release", &mut state.release_ms, 5.0, 4000.0)
                    .with_format(|v| format!("{:.0} ms", v)),
            );
        });

        ui.separator();

        // Column 4: 4-Shape LFO
        ui.vertical(|ui| {
            ui.set_width(140.0);
            ui.label(RichText::new("Modulation LFO").size(12.0).color(COLOR_AMBER).strong());
            ui.separator();

            ui.add(
                GhostSlider::new("Rate Hz", &mut state.lfo_rate_hz, 0.05, 30.0)
                    .with_format(|v| format!("{:.2} Hz", v)),
            );

            ui.horizontal(|ui| {
                ui.label(RichText::new("Shape:").size(11.0).color(COLOR_TEXT_MUTED));
                ComboBox::from_id_salt("lfo_shape_select")
                    .selected_text(match state.lfo_shape {
                        0 => "Sine",
                        1 => "Saw Down",
                        2 => "Square",
                        _ => "Sample & Hold",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.lfo_shape, 0, "Sine");
                        ui.selectable_value(&mut state.lfo_shape, 1, "Saw Down");
                        ui.selectable_value(&mut state.lfo_shape, 2, "Square");
                        ui.selectable_value(&mut state.lfo_shape, 3, "Sample & Hold");
                    });
            });
        });
    });

    action
}
