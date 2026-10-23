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

// Ground truth Ableton Live 12 architectural surface tokens
pub const COLOR_DESKTOP: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a); // Desktop #2a2a2a
pub const COLOR_SURFACE_AREA: Color32 = Color32::from_rgb(0x24, 0x24, 0x24); // SurfaceArea #242424
pub const COLOR_SURFACE_BG: Color32 = Color32::from_rgb(0x36, 0x36, 0x36); // SurfaceBackground #363636
pub const COLOR_DETAIL_VIEW_BG: Color32 = Color32::from_rgb(0x3e, 0x3e, 0x3e); // DetailViewBackground #3e3e3e
pub const COLOR_SURFACE_HIGHLIGHT: Color32 = Color32::from_rgb(0x46, 0x46, 0x46); // SurfaceHighlight #464646
pub const COLOR_CONTROL_BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e); // ControlBackground #1e1e1e
pub const COLOR_CONTROL_CONTRAST_FRAME: Color32 = Color32::from_rgb(0x11, 0x11, 0x11); // ControlContrastFrame #111111
pub const COLOR_METER_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x18); // MeterBackground #181818

// Official State & Accent Tokens (.ask)
pub const COLOR_CHOSEN_DEFAULT: Color32 = Color32::from_rgb(0xff, 0xad, 0x56); // ChosenDefault #ffad56 (Amber)
pub const COLOR_CHOSEN_PLAY: Color32 = Color32::from_rgb(0x00, 0xd3, 0x8d); // ChosenPlay #00d38d (Green)
pub const COLOR_CHOSEN_RECORD: Color32 = Color32::from_rgb(0xff, 0x55, 0x59); // ChosenRecord #ff5559 (Ruby/Red)
pub const COLOR_CHOSEN_PRELISTEN: Color32 = Color32::from_rgb(0x3c, 0x6a, 0xb6); // ChosenPreListen #3c6ab6 (Solo Blue)
pub const COLOR_RANGE_DEFAULT: Color32 = Color32::from_rgb(0x03, 0xc3, 0xd5); // RangeDefault #03c3d5 (Cyan)
pub const COLOR_SCALE_AWARENESS: Color32 = Color32::from_rgb(0xb5, 0x95, 0xfc); // ScaleAwareness #b595fc (Lavender)

// Text and foreground
pub const COLOR_CONTROL_FG: Color32 = Color32::from_rgb(0xb5, 0xb5, 0xb5); // ControlForeground #b5b5b5
pub const COLOR_CONTROL_ON_FG: Color32 = Color32::from_rgb(0x07, 0x07, 0x07); // ControlOnForeground #070707

// Standard VU Meter Segments (.ask)
pub const COLOR_VU_MIN: Color32 = Color32::from_rgb(0x00, 0xf7, 0x58); // #00f758 (<= -12 dB)
pub const COLOR_VU_MID: Color32 = Color32::from_rgb(0xff, 0xd1, 0x00); // #ffd100 (-12 dB to 0 dB)
pub const COLOR_VU_MAX: Color32 = Color32::from_rgb(0xff, 0x0a, 0x0a); // #ff0a0a (> 0 dB)

// Ableton Live 12 Signature Track Colors
pub const COLOR_TRACK_TEAL: Color32 = Color32::from_rgb(0x6b, 0xaa, 0xce); // #6baace Synth/Keys
pub const COLOR_TRACK_AMBER: Color32 = Color32::from_rgb(0xe0, 0xaa, 0x2a); // #e0aa2a Drum/Percussion
pub const COLOR_TRACK_GREEN: Color32 = Color32::from_rgb(0x52, 0xba, 0x46); // #52ba46 Audio/Lead
pub const COLOR_TRACK_PURPLE: Color32 = Color32::from_rgb(0x95, 0x4e, 0xb2); // #954eb2 Bass/Sub
pub const COLOR_TRACK_BLUE: Color32 = Color32::from_rgb(0x48, 0x81, 0xaa); // #4881aa Return A

// Compatibility aliases mapped directly to Live 12 matte architectural tokens
pub const COLOR_BG_VOID: Color32 = COLOR_DESKTOP; // #2a2a2a
pub const COLOR_BG_SURFACE: Color32 = COLOR_SURFACE_AREA; // #242424
pub const COLOR_BG_PANEL_ALT: Color32 = COLOR_SURFACE_BG; // #363636
pub const COLOR_DIVIDER: Color32 = COLOR_CONTROL_CONTRAST_FRAME; // #111111
pub const COLOR_BORDER: Color32 = COLOR_CONTROL_CONTRAST_FRAME; // #111111

// Accents mapped to Live 12 tokens
pub const COLOR_AMBER: Color32 = COLOR_CHOSEN_DEFAULT; // #ffad56
pub const COLOR_CYAN: Color32 = COLOR_RANGE_DEFAULT; // #03c3d5
pub const COLOR_GREEN: Color32 = COLOR_CHOSEN_PLAY; // #00d38d
pub const COLOR_RUBY: Color32 = COLOR_CHOSEN_RECORD; // #ff5559
pub const COLOR_SOLO_BLUE: Color32 = COLOR_CHOSEN_PRELISTEN; // #3c6ab6
pub const COLOR_LAVENDER: Color32 = COLOR_SCALE_AWARENESS; // #b595fc

// Text colors
pub const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(0xed, 0xf2, 0xf7);
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(0x8c, 0x8c, 0x8c);
pub const COLOR_TEXT_DIM: Color32 = Color32::from_rgb(0x5c, 0x5c, 0x5c);

/// Applies the Live 12 Matte Architectural theme to an egui context.
pub fn apply_dark_carbon_theme(ctx: &Context) {
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(COLOR_TEXT_PRIMARY);
    visuals.panel_fill = COLOR_DESKTOP;
    visuals.window_fill = COLOR_SURFACE_AREA;
    visuals.window_stroke = Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME);
    visuals.faint_bg_color = COLOR_SURFACE_BG;
    visuals.extreme_bg_color = COLOR_CONTROL_BG;

    // Widgets inactive
    visuals.widgets.inactive.bg_fill = COLOR_SURFACE_AREA;
    visuals.widgets.inactive.weak_bg_fill = COLOR_SURFACE_AREA;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, COLOR_CONTROL_CONTRAST_FRAME);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, COLOR_CONTROL_FG);
    visuals.widgets.inactive.rounding = Rounding::same(0.0);

    // Widgets hovered
    visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HIGHLIGHT;
    visuals.widgets.hovered.weak_bg_fill = COLOR_SURFACE_HIGHLIGHT;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, COLOR_RANGE_DEFAULT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.rounding = Rounding::same(0.0);

    // Widgets active / pressed
    visuals.widgets.active.bg_fill = COLOR_SURFACE_BG;
    visuals.widgets.active.weak_bg_fill = COLOR_SURFACE_BG;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, COLOR_AMBER);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, COLOR_AMBER);
    visuals.widgets.active.rounding = Rounding::same(0.0);

    // Widgets open / selected
    visuals.widgets.open.bg_fill = COLOR_SURFACE_AREA;
    visuals.widgets.open.weak_bg_fill = COLOR_SURFACE_AREA;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, COLOR_GREEN);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, COLOR_GREEN);

    // Window rounding and spacing
    visuals.window_rounding = Rounding::same(2.0);

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

    #[test]
    fn test_live12_palette_tokens() {
        assert_eq!(COLOR_DESKTOP, Color32::from_rgb(0x2a, 0x2a, 0x2a));
        assert_eq!(COLOR_SURFACE_AREA, Color32::from_rgb(0x24, 0x24, 0x24));
        assert_eq!(COLOR_SURFACE_BG, Color32::from_rgb(0x36, 0x36, 0x36));
        assert_eq!(COLOR_DETAIL_VIEW_BG, Color32::from_rgb(0x3e, 0x3e, 0x3e));
        assert_eq!(COLOR_SURFACE_HIGHLIGHT, Color32::from_rgb(0x46, 0x46, 0x46));
        assert_eq!(COLOR_CONTROL_BG, Color32::from_rgb(0x1e, 0x1e, 0x1e));
        assert_eq!(COLOR_CONTROL_CONTRAST_FRAME, Color32::from_rgb(0x11, 0x11, 0x11));
        assert_eq!(COLOR_METER_BG, Color32::from_rgb(0x18, 0x18, 0x18));
        assert_eq!(COLOR_CHOSEN_DEFAULT, Color32::from_rgb(0xff, 0xad, 0x56));
        assert_eq!(COLOR_CHOSEN_PLAY, Color32::from_rgb(0x00, 0xd3, 0x8d));
        assert_eq!(COLOR_CHOSEN_RECORD, Color32::from_rgb(0xff, 0x55, 0x59));
        assert_eq!(COLOR_CHOSEN_PRELISTEN, Color32::from_rgb(0x3c, 0x6a, 0xb6));
        assert_eq!(COLOR_RANGE_DEFAULT, Color32::from_rgb(0x03, 0xc3, 0xd5));
        assert_eq!(COLOR_SCALE_AWARENESS, Color32::from_rgb(0xb5, 0x95, 0xfc));
        assert_eq!(COLOR_VU_MIN, Color32::from_rgb(0x00, 0xf7, 0x58));
        assert_eq!(COLOR_VU_MID, Color32::from_rgb(0xff, 0xd1, 0x00));
        assert_eq!(COLOR_VU_MAX, Color32::from_rgb(0xff, 0x0a, 0x0a));
    }
}
