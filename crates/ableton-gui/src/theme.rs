//! Dark Carbon Studio Theme tokens and egui configuration.
//!
//! Enforces the studio color palette:
//! - Backgrounds: `#07080a` (void), `#111318` (carbon surface), `#181d28` (dividers)
//! - Accents:
//!   - Amber `#ffbe00` (play/cutoff/macro X)
//!   - Cyan `#00d2ff` (spectrum/modulation/macro Y)
//!   - Phosphor Green `#00e676` (oscilloscope/active clips/solo)
//!   - Ruby `#ff1744` (record/mute/clip warning)
//!   - Lavender `#b388ff` (mod envelope)
//! - Tabular monospace fonts for all telemetry readouts to eliminate layout jitter.

use egui::{
    Color32, Context, FontFamily, FontId, Margin, Rounding, Stroke, Style, TextStyle, Visuals,
};

pub const COLOR_BG_VOID: Color32 = Color32::from_rgb(0x07, 0x08, 0x0a);
pub const COLOR_BG_SURFACE: Color32 = Color32::from_rgb(0x11, 0x13, 0x18);
pub const COLOR_BG_PANEL_ALT: Color32 = Color32::from_rgb(0x0d, 0x0f, 0x14);
pub const COLOR_DIVIDER: Color32 = Color32::from_rgb(0x18, 0x1d, 0x28);
pub const COLOR_BORDER: Color32 = Color32::from_rgb(0x23, 0x2a, 0x3b);

// Accents
pub const COLOR_AMBER: Color32 = Color32::from_rgb(0xff, 0xbe, 0x00);
pub const COLOR_CYAN: Color32 = Color32::from_rgb(0x00, 0xd2, 0xff);
pub const COLOR_GREEN: Color32 = Color32::from_rgb(0x00, 0xe6, 0x76);
pub const COLOR_RUBY: Color32 = Color32::from_rgb(0xff, 0x17, 0x44);
pub const COLOR_LAVENDER: Color32 = Color32::from_rgb(0xb3, 0x88, 0xff);

// Text colors
pub const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(0xed, 0xf2, 0xf7);
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(0x71, 0x80, 0x96);
pub const COLOR_TEXT_DIM: Color32 = Color32::from_rgb(0x4a, 0x55, 0x68);

/// Applies the Dark Carbon Studio theme to an egui context.
pub fn apply_dark_carbon_theme(ctx: &Context) {
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(COLOR_TEXT_PRIMARY);
    visuals.panel_fill = COLOR_BG_VOID;
    visuals.window_fill = COLOR_BG_SURFACE;
    visuals.window_stroke = Stroke::new(1.0_f32, COLOR_DIVIDER);
    visuals.faint_bg_color = COLOR_BG_PANEL_ALT;
    visuals.extreme_bg_color = COLOR_BG_VOID;

    // Widgets inactive
    visuals.widgets.inactive.bg_fill = COLOR_BG_SURFACE;
    visuals.widgets.inactive.weak_bg_fill = COLOR_BG_SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, COLOR_DIVIDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_PRIMARY);
    visuals.widgets.inactive.rounding = Rounding::same(3.0);

    // Widgets hovered
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x1c, 0x22, 0x30);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x1c, 0x22, 0x30);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, COLOR_CYAN);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.rounding = Rounding::same(3.0);

    // Widgets active / pressed
    visuals.widgets.active.bg_fill = Color32::from_rgb(0x28, 0x32, 0x48);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x28, 0x32, 0x48);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, COLOR_AMBER);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, COLOR_AMBER);
    visuals.widgets.active.rounding = Rounding::same(3.0);

    // Widgets open / selected
    visuals.widgets.open.bg_fill = COLOR_BG_SURFACE;
    visuals.widgets.open.weak_bg_fill = COLOR_BG_SURFACE;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, COLOR_GREEN);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, COLOR_GREEN);

    // Window rounding and spacing
    visuals.window_rounding = Rounding::same(4.0);

    let mut style = Style {
        visuals,
        ..Style::default()
    };

    style.spacing.item_spacing = egui::vec2(6.0, 6.0);
    style.spacing.window_margin = Margin::same(6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);

    // Monospace tabular text styles for telemetry
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.5, FontFamily::Monospace),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(12.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(15.0, FontFamily::Proportional),
    );

    ctx.set_style(style);
}

/// Helper for tabular monospace telemetry string formatting.
#[inline]
pub fn format_telemetry_db(db: f32) -> String {
    if db <= -90.0 {
        "-inf dB".to_string()
    } else {
        format!("{:>+5.1} dB", db)
    }
}

/// Helper for tabular BPM readout.
#[inline]
pub fn format_bpm(bpm: f64) -> String {
    format!("{:>6.2} BPM", bpm)
}

/// Helper for musical beat format `Bar.Beat.16th`.
#[inline]
pub fn format_musical_beat(beat: f64) -> String {
    let non_neg = beat.max(0.0);
    let bar = (non_neg / 4.0).floor() as u64 + 1;
    let beat_in_bar = (non_neg % 4.0).floor() as u64 + 1;
    let sixteenth = ((non_neg * 4.0) % 4.0).floor() as u64 + 1;
    format!("{}.{}.{}", bar, beat_in_bar, sixteenth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_helpers() {
        assert_eq!(format_telemetry_db(-100.0), "-inf dB");
        assert_eq!(format_telemetry_db(-3.24), " -3.2 dB");
        assert_eq!(format_bpm(124.5), "124.50 BPM");
        assert_eq!(format_musical_beat(0.0), "1.1.1");
        assert_eq!(format_musical_beat(4.0), "2.1.1");
        assert_eq!(format_musical_beat(1.25), "1.2.2");
    }
}
