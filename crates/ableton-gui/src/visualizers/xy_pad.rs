//! Interactive XY Perform Pad with 8 Studio Macros, Orbit LFO, and audio-reactive visualization.
//!
//! Provides:
//! - Full multi-macro control surface alongside 2D XY morphing canvas
//! - 8 Ableton Live 12 Studio Macro Knobs/Sliders (Cutoff, Resonance, Wavetable Morph, Analog Drive,
//!   Sub Punch, Env Mod, Reverb Space, Delay Dub)
//! - Automated `Orbit / Motion LFO` toggle animating (X, Y) trajectory in real time across the canvas
//! - Dynamic phosphor decay trail and audio-reactive RMS bloom

use egui::{
    pos2, vec2, Align2, Button, Color32, FontFamily, FontId, Painter, Pos2, Rect, Response,
    RichText, Sense, Slider, Stroke, Ui,
};
use ableton_audio::{
    PARAM_FX_DELAY_WET, PARAM_FX_DRIVE, PARAM_FX_REVERB_WET, PARAM_SYNTH_CUTOFF,
    PARAM_SYNTH_MACRO_X, PARAM_SYNTH_MACRO_Y, PARAM_SYNTH_RESONANCE,
};
use crate::theme::{
    COLOR_AMBER, COLOR_BG_PANEL_ALT, COLOR_BG_SURFACE, COLOR_BG_VOID, COLOR_BORDER, COLOR_CYAN,
    COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

/// Trajectory shape for the automated Orbit / Motion LFO engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrbitShape {
    Circle,
    FigureEight,
    Lissajous,
}

/// Ableton Live 12 Studio Macro Parameter model.
#[derive(Debug, Clone)]
pub struct StudioMacroParam {
    pub name: &'static str,
    pub label: &'static str,
    pub value: f32, // Normalized 0.0 .. 1.0
    pub default_value: f32,
    pub min_val: f32,
    pub max_val: f32,
    pub unit: &'static str,
    pub audio_param_id: usize,
}

impl StudioMacroParam {
    pub const fn new(
        name: &'static str,
        label: &'static str,
        default_val: f32,
        min_val: f32,
        max_val: f32,
        unit: &'static str,
        audio_param_id: usize,
    ) -> Self {
        Self {
            name,
            label,
            value: default_val,
            default_value: default_val,
            min_val,
            max_val,
            unit,
            audio_param_id,
        }
    }

    /// Computes engineering/display value based on min_val and max_val.
    pub fn display_value(&self) -> f32 {
        self.min_val + self.value * (self.max_val - self.min_val)
    }

    /// Sets normalized value from an engineering display value.
    pub fn set_display_value(&mut self, val: f32) {
        if self.max_val > self.min_val {
            self.value = ((val - self.min_val) / (self.max_val - self.min_val)).clamp(0.0, 1.0);
        }
    }

    /// Formats formatted readout with engineering units (Hz, kHz, dB, %).
    pub fn format_display(&self) -> String {
        let val = self.display_value();
        if self.unit == "Hz" || self.unit == "kHz" {
            if val >= 1000.0 {
                format!("{:.1} kHz", val / 1000.0)
            } else {
                format!("{:.0} Hz", val)
            }
        } else if self.unit == "dB" {
            format!("{:.1} dB", val)
        } else if self.unit == "%" {
            format!("{:.0}%", val * 100.0)
        } else {
            format!("{:.2} {}", val, self.unit)
        }
    }
}

/// Point on the phosphor decay trail.
#[derive(Debug, Clone)]
pub struct PhosphorPoint {
    pub pos: Pos2,
    pub timestamp: f64,
}

/// Persistent state for the XY Perform Pad and 8 Studio Macros.
#[derive(Debug, Clone)]
pub struct XyPerformPadState {
    pub x: f32, // 0.0 .. 1.0
    pub y: f32, // 0.0 .. 1.0
    pub trail: Vec<PhosphorPoint>,
    // Automated Orbit / Motion LFO
    pub orbit_enabled: bool,
    pub orbit_speed: f32,        // 0.1 .. 4.0 Hz, default 0.8 Hz
    pub orbit_radius: f32,       // 0.05 .. 0.45, default 0.30
    pub orbit_shape: OrbitShape,
    pub orbit_center: (f32, f32), // default (0.5, 0.5)
    // 8 Studio Macros
    pub macros: [StudioMacroParam; 8],
    pub map_x_to_macro: Option<usize>, // default Some(0) -> Cutoff
    pub map_y_to_macro: Option<usize>, // default Some(1) -> Resonance
}

impl XyPerformPadState {
    /// Factory for the 8 Ableton Live 12 Studio Macros.
    pub fn default_macros() -> [StudioMacroParam; 8] {
        [
            StudioMacroParam::new("Macro 1", "Cutoff", 0.65, 20.0, 20000.0, "Hz", PARAM_SYNTH_CUTOFF),
            StudioMacroParam::new("Macro 2", "Resonance", 0.40, 0.0, 1.0, "%", PARAM_SYNTH_RESONANCE),
            StudioMacroParam::new("Macro 3", "WT Morph", 0.50, 0.0, 1.0, "%", PARAM_SYNTH_MACRO_X),
            StudioMacroParam::new("Macro 4", "Analog Drive", 0.25, 0.0, 24.0, "dB", PARAM_FX_DRIVE),
            StudioMacroParam::new("Macro 5", "Sub Punch", 0.70, 0.0, 1.0, "%", PARAM_SYNTH_MACRO_Y),
            StudioMacroParam::new("Macro 6", "Env Mod", 0.50, 0.0, 1.0, "%", 9),
            StudioMacroParam::new("Macro 7", "Reverb Space", 0.20, 0.0, 1.0, "%", PARAM_FX_REVERB_WET),
            StudioMacroParam::new("Macro 8", "Delay Dub", 0.30, 0.0, 1.0, "%", PARAM_FX_DELAY_WET),
        ]
    }

    /// Advances Orbit / Motion LFO math and updates (x, y) coordinates.
    pub fn update_orbit(&mut self, current_time: f64) {
        if !self.orbit_enabled {
            return;
        }

        let omega = current_time * self.orbit_speed as f64 * std::f64::consts::TAU;
        let (dx, dy) = match self.orbit_shape {
            OrbitShape::Circle => (omega.cos(), omega.sin()),
            OrbitShape::FigureEight => (omega.cos(), (2.0 * omega).sin() * 0.5),
            OrbitShape::Lissajous => ((3.0 * omega).cos(), (2.0 * omega).sin()),
        };

        self.x = (self.orbit_center.0 as f64 + self.orbit_radius as f64 * dx).clamp(0.0, 1.0) as f32;
        self.y = (self.orbit_center.1 as f64 + self.orbit_radius as f64 * dy).clamp(0.0, 1.0) as f32;

        self.sync_mapped_macros();
    }

    /// Propagates current (x, y) coordinates to mapped Studio Macros.
    pub fn sync_mapped_macros(&mut self) {
        if let Some(x_idx) = self.map_x_to_macro {
            if x_idx < self.macros.len() {
                self.macros[x_idx].value = self.x;
            }
        }
        if let Some(y_idx) = self.map_y_to_macro {
            if y_idx < self.macros.len() {
                self.macros[y_idx].value = self.y;
            }
        }
    }
}

impl Default for XyPerformPadState {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            trail: Vec::with_capacity(64),
            orbit_enabled: false,
            orbit_speed: 0.8,
            orbit_radius: 0.30,
            orbit_shape: OrbitShape::Circle,
            orbit_center: (0.5, 0.5),
            macros: Self::default_macros(),
            map_x_to_macro: Some(0),
            map_y_to_macro: Some(1),
        }
    }
}

/// Interactive XY Perform Pad & 8 Studio Macros widget.
pub struct XyPerformPadWidget<'a> {
    pub state: &'a mut XyPerformPadState,
    pub current_time: f64,
    pub rms_bloom: f32, // 0.0 .. 1.0
    pub size: egui::Vec2,
}

impl<'a> XyPerformPadWidget<'a> {
    pub fn new(
        state: &'a mut XyPerformPadState,
        current_time: f64,
        rms_bloom: f32,
        size: egui::Vec2,
    ) -> Self {
        Self {
            state,
            current_time,
            rms_bloom,
            size,
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let mut changed = false;

        let total_w = ui.available_width();
        let split_layout = total_w >= 540.0;

        // Animate orbit trajectory if enabled
        if self.state.orbit_enabled {
            self.state.update_orbit(self.current_time);
            ui.ctx().request_repaint();
            changed = true;
        }

        // Age phosphor decay trail (1.2s lifespan)
        let now = self.current_time;
        self.state.trail.retain(|pt| now - pt.timestamp < 1.2);

        let canvas_w = if split_layout {
            (total_w * 0.38).clamp(270.0, 350.0)
        } else {
            total_w
        };
        let canvas_h = (ui.available_height() - 28.0).clamp(60.0, 140.0);

        let response = if split_layout {
            ui.horizontal(|ui| {
                // --- Left Column: 2D XY Morphing Canvas + Orbit Controls ---
                ui.allocate_ui_with_layout(
                    vec2(canvas_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let (pad_rect, pad_response) = ui.allocate_exact_size(
                            vec2(canvas_w, canvas_h),
                            Sense::drag(),
                        );

                        // Handle direct mouse drag / click on canvas
                        if pad_response.dragged() || pad_response.clicked() {
                            if let Some(pos) = pad_response.interact_pointer_pos() {
                                let norm_x = ((pos.x - pad_rect.left()) / pad_rect.width()).clamp(0.0, 1.0);
                                let norm_y = ((pad_rect.bottom() - pos.y) / pad_rect.height()).clamp(0.0, 1.0);

                                self.state.x = norm_x;
                                self.state.y = norm_y;
                                self.state.sync_mapped_macros();

                                self.state.trail.push(PhosphorPoint {
                                    pos: pos2(
                                        pad_rect.left() + norm_x * pad_rect.width(),
                                        pad_rect.bottom() - norm_y * pad_rect.height(),
                                    ),
                                    timestamp: self.current_time,
                                });
                                changed = true;
                            }
                        } else if self.state.orbit_enabled {
                            // Record orbit path onto phosphor trail
                            self.state.trail.push(PhosphorPoint {
                                pos: pos2(
                                    pad_rect.left() + self.state.x * pad_rect.width(),
                                    pad_rect.bottom() - self.state.y * pad_rect.height(),
                                ),
                                timestamp: self.current_time,
                            });
                        }

                        if ui.is_rect_visible(pad_rect) {
                            let painter = ui.painter_at(pad_rect);
                            render_xy_pad_canvas(
                                &painter,
                                pad_rect,
                                self.state,
                                now,
                                self.rms_bloom,
                            );
                        }

                        ui.add_space(3.0);

                        // Orbit Controls Toolbar
                        ui.horizontal(|ui| {
                            let orbit_active = self.state.orbit_enabled;
                            let orbit_text = RichText::new(if orbit_active { "• Orbit ON" } else { "• Orbit OFF" })
                                .size(9.5)
                                .color(if orbit_active { COLOR_AMBER } else { COLOR_TEXT_MUTED })
                                .strong();
                            let orbit_btn = Button::new(orbit_text)
                                .fill(if orbit_active { Color32::from_rgb(0x28, 0x1f, 0x0c) } else { COLOR_BG_PANEL_ALT })
                                .stroke(Stroke::new(1.0_f32, if orbit_active { COLOR_AMBER } else { COLOR_BORDER }));
                            if ui.add(orbit_btn).clicked() {
                                self.state.orbit_enabled = !self.state.orbit_enabled;
                                changed = true;
                            }

                            // Shape selector buttons
                            let shape_c = Button::new(RichText::new("●").size(10.0).color(
                                if self.state.orbit_shape == OrbitShape::Circle { COLOR_AMBER } else { COLOR_TEXT_MUTED },
                            )).min_size(vec2(18.0, 18.0));
                            if ui.add(shape_c).clicked() {
                                self.state.orbit_shape = OrbitShape::Circle;
                                changed = true;
                            }

                            let shape_f8 = Button::new(RichText::new("∞").size(11.0).color(
                                if self.state.orbit_shape == OrbitShape::FigureEight { COLOR_AMBER } else { COLOR_TEXT_MUTED },
                            )).min_size(vec2(18.0, 18.0));
                            if ui.add(shape_f8).clicked() {
                                self.state.orbit_shape = OrbitShape::FigureEight;
                                changed = true;
                            }

                            let shape_lis = Button::new(RichText::new("~").size(11.0).color(
                                if self.state.orbit_shape == OrbitShape::Lissajous { COLOR_AMBER } else { COLOR_TEXT_MUTED },
                            )).min_size(vec2(18.0, 18.0));
                            if ui.add(shape_lis).clicked() {
                                self.state.orbit_shape = OrbitShape::Lissajous;
                                changed = true;
                            }

                            // Speed slider
                            ui.label(RichText::new("Spd").size(9.0).color(COLOR_TEXT_MUTED));
                            let speed_slider = Slider::new(&mut self.state.orbit_speed, 0.1..=4.0)
                                .suffix("Hz")
                                .show_value(true);
                            if ui.add(speed_slider).changed() {
                                changed = true;
                            }
                        });
                    },
                );

                ui.separator();

                // --- Right Column: 8 Live 12 Studio Macros Surface ---
                let right_w = (total_w - canvas_w - 24.0).max(280.0);
                ui.allocate_ui_with_layout(
                    vec2(right_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let is_compact_deck = ui.available_height() <= 180.0;
                        if is_compact_deck {
                            ui.spacing_mut().item_spacing = vec2(4.0, 1.0);
                        }

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("STUDIO MACRO SURFACE")
                                    .size(if is_compact_deck { 9.5 } else { 11.0 })
                                    .color(COLOR_AMBER)
                                    .strong(),
                            );
                            ui.label(
                                RichText::new("Live 12 Multi-Param Control")
                                    .size(if is_compact_deck { 8.5 } else { 9.5 })
                                    .color(COLOR_TEXT_MUTED),
                            );
                        });

                        ui.separator();

                        // 8 Macros in 2-row x 4-column layout
                        let col_w = ((right_w - 3.0 * 6.0) / 4.0).floor().max(68.0);
                        let row_gap = if is_compact_deck { 2.0 } else { 3.0 };
                        let grid_avail_h = (ui.available_height() - 2.0).max(80.0);
                        let card_h = ((grid_avail_h - row_gap) * 0.5).clamp(42.0, 66.0);
                        let is_compact_card = is_compact_deck || card_h < 56.0;

                        for row in 0..2 {
                            ui.horizontal(|ui| {
                                for col in 0..4 {
                                    let macro_idx = row * 4 + col;
                                    let is_mapped_x = self.state.map_x_to_macro == Some(macro_idx);
                                    let is_mapped_y = self.state.map_y_to_macro == Some(macro_idx);

                                    let card_size = vec2(col_w, card_h);
                                    ui.allocate_ui_with_layout(
                                        card_size,
                                        egui::Layout::top_down(egui::Align::Min),
                                        |card_ui| {
                                            if is_compact_card {
                                                card_ui.spacing_mut().item_spacing = vec2(2.0, 1.0);
                                                card_ui.spacing_mut().interact_size.y = 11.0;
                                                card_ui.spacing_mut().button_padding = vec2(2.0, 0.5);
                                            }

                                            let card_rect = card_ui.max_rect();
                                            if card_ui.is_rect_visible(card_rect) {
                                                let painter = card_ui.painter();
                                                painter.rect_filled(card_rect, 3.0, COLOR_BG_SURFACE);
                                                let stroke_color = if is_mapped_x {
                                                    COLOR_AMBER
                                                } else if is_mapped_y {
                                                    COLOR_CYAN
                                                } else {
                                                    COLOR_BORDER
                                                };
                                                painter.rect_stroke(card_rect, 3.0, Stroke::new(1.0_f32, stroke_color));

                                                // Macro title
                                                let p = &self.state.macros[macro_idx];
                                                let title_size = if is_compact_card { 8.5 } else { 9.5 };
                                                let title_top = if is_compact_card { 3.0 } else { 4.0 };
                                                painter.text(
                                                    pos2(card_rect.left() + 5.0, card_rect.top() + title_top),
                                                    Align2::LEFT_TOP,
                                                    p.label,
                                                    FontId::new(title_size, FontFamily::Proportional),
                                                    if is_mapped_x {
                                                        COLOR_AMBER
                                                    } else if is_mapped_y {
                                                        COLOR_CYAN
                                                    } else {
                                                        COLOR_TEXT_PRIMARY
                                                    },
                                                );
                                            }

                                            let top_space = if card_h < 48.0 { 10.0 } else if card_h < 56.0 { 12.0 } else { 16.0 };
                                            card_ui.add_space(top_space);

                                            // Slider bar
                                            let slider = Slider::new(&mut self.state.macros[macro_idx].value, 0.0..=1.0)
                                                .show_value(false);
                                            if card_ui.add(slider).changed() {
                                                if is_mapped_x {
                                                    self.state.x = self.state.macros[macro_idx].value;
                                                }
                                                if is_mapped_y {
                                                    self.state.y = self.state.macros[macro_idx].value;
                                                }
                                                changed = true;
                                            }

                                            // Axis Mapping Chips [X] and [Y] + Readout on right
                                            let p = &self.state.macros[macro_idx];
                                            card_ui.horizontal(|card_ui| {
                                                let chip_size = if is_compact_card { 7.5 } else { 8.5 };
                                                let chip_btn_w = if is_compact_card { 14.0 } else { 16.0 };
                                                let chip_btn_h = if is_compact_card { 11.0 } else { 14.0 };

                                                let x_btn = Button::new(
                                                    RichText::new("X")
                                                        .size(chip_size)
                                                        .color(if is_mapped_x { Color32::BLACK } else { COLOR_TEXT_MUTED })
                                                        .strong(),
                                                )
                                                .fill(if is_mapped_x { COLOR_AMBER } else { COLOR_BG_PANEL_ALT })
                                                .min_size(vec2(chip_btn_w, chip_btn_h));
                                                if card_ui.add(x_btn).clicked() {
                                                    if is_mapped_x {
                                                        self.state.map_x_to_macro = None;
                                                    } else {
                                                        self.state.map_x_to_macro = Some(macro_idx);
                                                        self.state.x = self.state.macros[macro_idx].value;
                                                    }
                                                    changed = true;
                                                }

                                                let y_btn = Button::new(
                                                    RichText::new("Y")
                                                        .size(chip_size)
                                                        .color(if is_mapped_y { Color32::BLACK } else { COLOR_TEXT_MUTED })
                                                        .strong(),
                                                )
                                                .fill(if is_mapped_y { COLOR_CYAN } else { COLOR_BG_PANEL_ALT })
                                                .min_size(vec2(chip_btn_w, chip_btn_h));
                                                if card_ui.add(y_btn).clicked() {
                                                    if is_mapped_y {
                                                        self.state.map_y_to_macro = None;
                                                    } else {
                                                        self.state.map_y_to_macro = Some(macro_idx);
                                                        self.state.y = self.state.macros[macro_idx].value;
                                                    }
                                                    changed = true;
                                                }

                                                let readout_size = if is_compact_card { 8.0 } else { 8.5 };
                                                card_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |card_ui| {
                                                    card_ui.label(
                                                        RichText::new(p.format_display())
                                                            .size(readout_size)
                                                            .monospace()
                                                            .color(COLOR_TEXT_MUTED),
                                                    );
                                                });
                                            });
                                        },
                                    );
                                }
                            });
                            if row == 0 {
                                ui.add_space(row_gap);
                            }
                        }
                    },
                );
            })
            .response
        } else {
            // Narrow vertical fallback
            ui.vertical(|ui| {
                let (pad_rect, pad_response) = ui.allocate_exact_size(
                    vec2(total_w, canvas_h),
                    Sense::drag(),
                );

                if pad_response.dragged() || pad_response.clicked() {
                    if let Some(pos) = pad_response.interact_pointer_pos() {
                        let norm_x = ((pos.x - pad_rect.left()) / pad_rect.width()).clamp(0.0, 1.0);
                        let norm_y = ((pad_rect.bottom() - pos.y) / pad_rect.height()).clamp(0.0, 1.0);
                        self.state.x = norm_x;
                        self.state.y = norm_y;
                        self.state.sync_mapped_macros();
                        changed = true;
                    }
                }

                if ui.is_rect_visible(pad_rect) {
                    let painter = ui.painter_at(pad_rect);
                    render_xy_pad_canvas(&painter, pad_rect, self.state, now, self.rms_bloom);
                }
            })
            .response
        };

        let mut final_response = response;
        if changed {
            final_response.mark_changed();
        }
        final_response
    }
}

/// Renders the 2D XY morphing canvas with crosshair, orbit guide, phosphor trail, and RMS bloom.
pub fn render_xy_pad_canvas(
    painter: &Painter,
    rect: Rect,
    state: &XyPerformPadState,
    current_time: f64,
    rms_bloom: f32,
) {
    // 1. Background and border
    painter.rect_filled(rect, 4.0, COLOR_BG_VOID);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, COLOR_BORDER));

    // 2. Crosshair grid
    let mid_x = rect.center().x;
    let mid_y = rect.center().y;
    painter.line_segment(
        [pos2(mid_x, rect.top()), pos2(mid_x, rect.bottom())],
        Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
    );
    painter.line_segment(
        [pos2(rect.left(), mid_y), pos2(rect.right(), mid_y)],
        Stroke::new(1.0_f32, Color32::from_rgb(0x18, 0x1d, 0x28)),
    );

    // 3. Faint Orbit Guide Path if orbit is running or active
    if state.orbit_enabled {
        let segments = 32;
        let mut prev_pt: Option<Pos2> = None;
        for i in 0..=segments {
            let theta = (i as f64 / segments as f64) * std::f64::consts::TAU;
            let (dx, dy) = match state.orbit_shape {
                OrbitShape::Circle => (theta.cos(), theta.sin()),
                OrbitShape::FigureEight => (theta.cos(), (2.0 * theta).sin() * 0.5),
                OrbitShape::Lissajous => ((3.0 * theta).cos(), (2.0 * theta).sin()),
            };
            let ox = (state.orbit_center.0 as f64 + state.orbit_radius as f64 * dx).clamp(0.0, 1.0) as f32;
            let oy = (state.orbit_center.1 as f64 + state.orbit_radius as f64 * dy).clamp(0.0, 1.0) as f32;
            let pt = pos2(
                rect.left() + ox * rect.width(),
                rect.bottom() - oy * rect.height(),
            );
            if let Some(prev) = prev_pt {
                painter.line_segment(
                    [prev, pt],
                    Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(0xff, 0xbe, 0x00, 40)),
                );
            }
            prev_pt = Some(pt);
        }
    }

    // 4. Axis Labels
    let x_label = state
        .map_x_to_macro
        .and_then(|idx| state.macros.get(idx))
        .map(|m| format!("X: {}", m.label))
        .unwrap_or_else(|| "X: CUTOFF".to_string());

    let y_label = state
        .map_y_to_macro
        .and_then(|idx| state.macros.get(idx))
        .map(|m| format!("Y: {}", m.label))
        .unwrap_or_else(|| "Y: RESONANCE".to_string());

    painter.text(
        pos2(rect.right() - 8.0, rect.bottom() - 8.0),
        Align2::RIGHT_BOTTOM,
        x_label,
        FontId::new(9.0, FontFamily::Monospace),
        COLOR_AMBER,
    );
    painter.text(
        pos2(rect.left() + 8.0, rect.top() + 8.0),
        Align2::LEFT_TOP,
        y_label,
        FontId::new(9.0, FontFamily::Monospace),
        COLOR_CYAN,
    );

    // 5. Phosphor trail with alpha decay
    for pt in &state.trail {
        let age = (current_time - pt.timestamp).max(0.0) as f32;
        let alpha = (1.0 - (age / 1.2)).clamp(0.0, 1.0);
        let radius = 6.0 * (1.0 + age * 1.5);
        let alpha_u8 = (alpha * 120.0) as u8;

        painter.circle_stroke(
            pt.pos,
            radius,
            Stroke::new(
                1.5 * alpha,
                Color32::from_rgba_premultiplied(0x00, 0xd2, 0xff, alpha_u8),
            ),
        );
    }

    // 6. Current Puck Position
    let puck_pos = pos2(
        rect.left() + state.x.clamp(0.0, 1.0) * rect.width(),
        rect.bottom() - state.y.clamp(0.0, 1.0) * rect.height(),
    );

    // 7. Audio-Reactive RMS Bloom Ring
    let bloom_factor = rms_bloom.clamp(0.0, 1.0);
    let bloom_radius = 12.0 + bloom_factor * 28.0;
    let bloom_alpha = (0.2 + bloom_factor * 0.6).min(1.0);
    painter.circle_stroke(
        puck_pos,
        bloom_radius,
        Stroke::new(
            1.5_f32,
            Color32::from_rgba_premultiplied(
                0xff,
                0xbe,
                0x00,
                (bloom_alpha * 200.0) as u8,
            ),
        ),
    );

    // Inner subtle glow ring
    painter.circle_stroke(
        puck_pos,
        bloom_radius * 0.6,
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_premultiplied(
                0x00,
                0xd2,
                0xff,
                (bloom_alpha * 150.0) as u8,
            ),
        ),
    );

    // Core puck
    painter.circle_filled(puck_pos, 7.0, COLOR_AMBER);
    painter.circle_stroke(puck_pos, 7.0, Stroke::new(1.5_f32, Color32::WHITE));
    painter.circle_filled(puck_pos, 2.5, Color32::WHITE);

    // Readout
    let readout = format!("X: {:.2}  Y: {:.2}", state.x, state.y);
    painter.text(
        pos2(rect.left() + 8.0, rect.bottom() - 8.0),
        Align2::LEFT_BOTTOM,
        readout,
        FontId::new(10.0, FontFamily::Monospace),
        COLOR_AMBER,
    );
}

/// Backward-compatible entry point for rendering the XY pad on a painter.
pub fn render_xy_pad(
    painter: &Painter,
    rect: Rect,
    norm_x: f32,
    norm_y: f32,
    trail: &[PhosphorPoint],
    current_time: f64,
    rms_bloom: f32,
) {
    let mut state = XyPerformPadState::default();
    state.x = norm_x;
    state.y = norm_y;
    state.trail = trail.to_vec();
    render_xy_pad_canvas(painter, rect, &state, current_time, rms_bloom);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_macros_initialization() {
        let macros = XyPerformPadState::default_macros();
        assert_eq!(macros.len(), 8);

        // Macro 1: Cutoff
        assert_eq!(macros[0].label, "Cutoff");
        assert_eq!(macros[0].unit, "Hz");
        assert_eq!(macros[0].min_val, 20.0);
        assert_eq!(macros[0].max_val, 20000.0);
        assert_eq!(macros[0].audio_param_id, PARAM_SYNTH_CUTOFF);

        // Macro 2: Resonance
        assert_eq!(macros[1].label, "Resonance");
        assert_eq!(macros[1].unit, "%");
        assert_eq!(macros[1].audio_param_id, PARAM_SYNTH_RESONANCE);

        // Macro 3: WT Morph
        assert_eq!(macros[2].label, "WT Morph");
        assert_eq!(macros[2].audio_param_id, PARAM_SYNTH_MACRO_X);

        // Macro 4: Analog Drive
        assert_eq!(macros[3].label, "Analog Drive");
        assert_eq!(macros[3].unit, "dB");
        assert_eq!(macros[3].audio_param_id, PARAM_FX_DRIVE);

        // Macro 5: Sub Punch
        assert_eq!(macros[4].label, "Sub Punch");
        assert_eq!(macros[4].audio_param_id, PARAM_SYNTH_MACRO_Y);

        // Macro 6: Env Mod
        assert_eq!(macros[5].label, "Env Mod");

        // Macro 7: Reverb Space
        assert_eq!(macros[6].label, "Reverb Space");
        assert_eq!(macros[6].audio_param_id, PARAM_FX_REVERB_WET);

        // Macro 8: Delay Dub
        assert_eq!(macros[7].label, "Delay Dub");
        assert_eq!(macros[7].audio_param_id, PARAM_FX_DELAY_WET);
    }

    #[test]
    fn test_macro_display_formatting() {
        let mut cutoff = StudioMacroParam::new("Macro 1", "Cutoff", 0.5, 20.0, 20000.0, "Hz", PARAM_SYNTH_CUTOFF);
        cutoff.set_display_value(2500.0);
        assert_eq!(cutoff.format_display(), "2.5 kHz");

        cutoff.set_display_value(440.0);
        assert_eq!(cutoff.format_display(), "440 Hz");

        let mut drive = StudioMacroParam::new("Macro 4", "Drive", 0.5, 0.0, 24.0, "dB", PARAM_FX_DRIVE);
        drive.set_display_value(6.0);
        assert_eq!(drive.format_display(), "6.0 dB");

        let reso = StudioMacroParam::new("Macro 2", "Reso", 0.4, 0.0, 1.0, "%", PARAM_SYNTH_RESONANCE);
        assert_eq!(reso.format_display(), "40%");
    }

    #[test]
    fn test_orbit_math_circle() {
        let mut state = XyPerformPadState::default();
        state.orbit_enabled = true;
        state.orbit_shape = OrbitShape::Circle;
        state.orbit_speed = 1.0;
        state.orbit_radius = 0.30;
        state.orbit_center = (0.5, 0.5);

        // At t = 0.0, omega = 0: x = 0.5 + 0.3 = 0.8, y = 0.5
        state.update_orbit(0.0);
        assert!((state.x - 0.80).abs() < 1e-4);
        assert!((state.y - 0.50).abs() < 1e-4);

        // At t = 0.25s (quarter period), x = 0.5, y = 0.5 + 0.3 = 0.8
        state.update_orbit(0.25);
        assert!((state.x - 0.50).abs() < 1e-4);
        assert!((state.y - 0.80).abs() < 1e-4);

        // Clamping check
        assert!(state.x >= 0.0 && state.x <= 1.0);
        assert!(state.y >= 0.0 && state.y <= 1.0);
    }

    #[test]
    fn test_orbit_math_figure_eight() {
        let mut state = XyPerformPadState::default();
        state.orbit_enabled = true;
        state.orbit_shape = OrbitShape::FigureEight;
        state.orbit_speed = 1.0;
        state.orbit_radius = 0.40;
        state.orbit_center = (0.5, 0.5);

        // Over 100 timesteps, coordinates must stay within [0.0, 1.0]
        for step in 0..100 {
            let t = step as f64 * 0.01;
            state.update_orbit(t);
            assert!(state.x >= 0.0 && state.x <= 1.0);
            assert!(state.y >= 0.0 && state.y <= 1.0);
        }
    }

    #[test]
    fn test_xy_to_macro_mapping_sync() {
        let mut state = XyPerformPadState::default();
        state.map_x_to_macro = Some(0); // Cutoff
        state.map_y_to_macro = Some(1); // Resonance

        state.x = 0.75;
        state.y = 0.25;
        state.sync_mapped_macros();

        assert_eq!(state.macros[0].value, 0.75);
        assert_eq!(state.macros[1].value, 0.25);
    }

    #[test]
    fn test_phosphor_trail_decay() {
        let mut state = XyPerformPadState::default();
        state.trail.push(PhosphorPoint {
            pos: pos2(100.0, 100.0),
            timestamp: 1.0,
        });
        state.trail.push(PhosphorPoint {
            pos: pos2(120.0, 120.0),
            timestamp: 2.0,
        });

        // At now = 2.5s:
        // pt 1 age = 1.5s (> 1.2s -> retain false)
        // pt 2 age = 0.5s (< 1.2s -> retain true)
        let now = 2.5;
        state.trail.retain(|pt| now - pt.timestamp < 1.2);
        assert_eq!(state.trail.len(), 1);
        assert_eq!(state.trail[0].timestamp, 2.0);
    }
}
