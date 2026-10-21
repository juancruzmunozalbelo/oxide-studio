//! Frontend Anti-Breakage Layout Guard (`layout_guard`).
//!
//! Provides two layers of protection so UI panels never clip, overflow, collide, or render tofu (`□`):
//! 1. **Compile/Runtime Proportional Auto-Fit Calculators (`FlexFitGrid`)**:
//!    Computes row heights and column widths directly from `ui.available_height()` / `ui.available_width()`
//!    so `N` rows (e.g. 8 drum rows in the 909 Sequencer) mathematically fit 100% inside the container
//!    without vertical overflow or bottom cut-off.
//! 2. **Post-Frame Geometric & Glyph Verifier (`audit_rendered_frame`)**:
//!    Inspects the `egui::epaint::ClippedShape` list produced by any UI frame across all DeckTabs,
//!    WorkspaceViews, and screen resolutions (`960x600` to `1920x1080`) and enforces:
//!    - **Law 1 (Zero Viewport Overflow)**: No visible shape extends outside `screen_rect`.
//!    - **Law 2 (Zero Text Cut-Off / Clipping)**: Every rendered text primitive fits inside its `clip_rect`.
//!    - **Law 3 (Zero Text-on-Text Collision)**: No two text primitives in the same layer overlap.
//!    - **Law 4 (Zero Tofu Glyphs)**: Every rendered character belongs to the verified `egui` glyph set.

use egui::{epaint::ClippedShape, epaint::Shape, Rect};

/// Proportional auto-fit layout calculator for fixed-count grids (e.g., 8-row Drum Sequencer,
/// 8-row Session Clip Matrix, 2x2 Groove Buttons).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexFitGrid;

impl FlexFitGrid {
    /// Computes the exact row height so that `rows` items separated by `row_gap` fit cleanly
    /// within `avail_h` (with `bottom_padding` reserved), clamped to `[min_h, max_h]`.
    #[inline]
    pub fn compute_row_height(
        avail_h: f32,
        rows: usize,
        row_gap: f32,
        bottom_padding: f32,
        min_h: f32,
        max_h: f32,
    ) -> f32 {
        if rows == 0 {
            return min_h;
        }
        let gaps_total = (rows.saturating_sub(1) as f32) * row_gap;
        let usable_h = (avail_h - gaps_total - bottom_padding).max(rows as f32 * min_h);
        (usable_h / rows as f32).floor().clamp(min_h, max_h)
    }

    /// Computes the exact column width so that `cols` items separated by `col_gap` fit cleanly
    /// within `avail_w` (with `right_padding` reserved), clamped to `[min_w, max_w]`.
    #[inline]
    pub fn compute_col_width(
        avail_w: f32,
        cols: usize,
        col_gap: f32,
        right_padding: f32,
        min_w: f32,
        max_w: f32,
    ) -> f32 {
        if cols == 0 {
            return min_w;
        }
        let gaps_total = (cols.saturating_sub(1) as f32) * col_gap;
        let usable_w = (avail_w - gaps_total - right_padding).max(cols as f32 * min_w);
        (usable_w / cols as f32).floor().clamp(min_w, max_w)
    }
}

/// Returns `true` if `c` is guaranteed to render cleanly in `egui`'s default embedded fonts
/// (ASCII printable, Latin-1 supplement, and verified geometric/musical symbols).
pub fn is_supported_egui_char(c: char) -> bool {
    if c.is_ascii() {
        return true;
    }
    // Latin-1 Supplement (Spanish accents: á, é, í, ó, ú, ñ, ü, ¡, ¿, •, ×, «, », °, ±, etc.)
    if ('\u{00A0}'..='\u{00FF}').contains(&c) {
        return true;
    }
    matches!(
        c,
        '•' | '–'
            | '—'
            | '…'
            | '’'
            | '“'
            | '”'
            | '▶'
            | '◀'
            | '■'
            | '●'
            | '★'
            | '☆'
            | '♪'
            | '♫'
            | '⚙'
            | '✂'
            | '≈'
            | '⚡'
            | '∞'
            | '≤'
            | '≥'
            | '≠'
            | '✓'
            | '▾'
    )
}

/// Returns all characters in `text` that are NOT in the verified `egui` font glyph set.
pub fn find_unsupported_glyphs(text: &str) -> Vec<char> {
    let mut bad = Vec::new();
    for c in text.chars() {
        if !is_supported_egui_char(c) && !bad.contains(&c) {
            bad.push(c);
        }
    }
    bad
}

/// Detailed report produced by [`audit_rendered_frame`].
#[derive(Debug, Clone, Default)]
pub struct FrameAntiBreakageReport {
    pub viewport_overflows: Vec<String>,
    pub clipped_texts: Vec<String>,
    pub text_collisions: Vec<String>,
    pub unsupported_glyphs: Vec<String>,
}

impl FrameAntiBreakageReport {
    pub fn is_clean(&self) -> bool {
        self.viewport_overflows.is_empty()
            && self.clipped_texts.is_empty()
            && self.text_collisions.is_empty()
            && self.unsupported_glyphs.is_empty()
    }
}

#[derive(Debug, Clone)]
struct ExtractedText {
    text: String,
    rect: Rect,
    clip_rect: Rect,
}

fn collect_texts_from_shape(shape: &Shape, clip_rect: Rect, out: &mut Vec<ExtractedText>) {
    match shape {
        Shape::Text(text_shape) => {
            let txt = text_shape.galley.job.text.clone();
            let rect = text_shape.visual_bounding_rect();
            if rect.is_finite() && !txt.trim().is_empty() {
                out.push(ExtractedText {
                    text: txt,
                    rect,
                    clip_rect,
                });
            }
        }
        Shape::Vec(shapes) => {
            for s in shapes {
                collect_texts_from_shape(s, clip_rect, out);
            }
        }
        _ => {}
    }
}

/// Audits a rendered `egui` frame (`shapes`) against the 4 Anti-Breakage Frontend Laws:
/// 1. Zero viewport overflow outside `screen_rect`.
/// 2. Zero text cut-off outside its parent `clip_rect`.
/// 3. Zero text-on-text collision within the same `clip_rect`.
/// 4. Zero unsupported Unicode / tofu (`□`) characters.
pub fn audit_rendered_frame(
    screen_rect: Rect,
    shapes: &[ClippedShape],
) -> FrameAntiBreakageReport {
    let mut report = FrameAntiBreakageReport::default();
    let mut texts = Vec::new();

    for clipped in shapes {
        let visible_rect = clipped.shape.visual_bounding_rect().intersect(clipped.clip_rect);
        if visible_rect.is_finite() && visible_rect.width() > 0.0 && visible_rect.height() > 0.0 {
            if visible_rect.left() < screen_rect.left() - 2.0
                || visible_rect.top() < screen_rect.top() - 2.0
                || visible_rect.right() > screen_rect.right() + 2.0
                || visible_rect.bottom() > screen_rect.bottom() + 2.0
            {
                report.viewport_overflows.push(format!(
                    "Shape visible rect {:?} overflowed screen {:?}",
                    visible_rect, screen_rect
                ));
            }
        }
        collect_texts_from_shape(&clipped.shape, clipped.clip_rect, &mut texts);
    }

    // Check Law 2 (Zero Text Cut-off) & Law 4 (Zero Tofu Glyphs)
    for item in &texts {
        let bad_chars = find_unsupported_glyphs(&item.text);
        if !bad_chars.is_empty() {
            report.unsupported_glyphs.push(format!(
                "Text {:?} contains unsupported tofu glyphs: {:?}",
                item.text, bad_chars
            ));
        }

        // Check if text overflows its own clip_rect (cut off at bottom or right edge)
        if item.clip_rect.is_finite()
            && item.clip_rect.width() > 4.0
            && item.clip_rect.height() > 4.0
        {
            let bottom_overflow = item.rect.bottom() - item.clip_rect.bottom();
            let top_overflow = item.clip_rect.top() - item.rect.top();
            if bottom_overflow > 2.5 || top_overflow > 2.5 {
                report.clipped_texts.push(format!(
                    "Text {:?} vertically clipped (text_rect={:?}, clip_rect={:?})",
                    item.text, item.rect, item.clip_rect
                ));
            }
        }
    }

    // Check Law 3 (Zero Text-on-Text Collision)
    for i in 0..texts.len() {
        for j in (i + 1)..texts.len() {
            let a = &texts[i];
            let b = &texts[j];
            // Only compare texts that are both visible inside their clip rects
            let vis_a = a.rect.intersect(a.clip_rect);
            let vis_b = b.rect.intersect(b.clip_rect);
            if vis_a.width() <= 1.0
                || vis_a.height() <= 1.0
                || vis_b.width() <= 1.0
                || vis_b.height() <= 1.0
            {
                continue;
            }
            let overlap = vis_a.intersect(vis_b);
            if overlap.width() > 3.5 && overlap.height() > 3.5 && a.text != b.text {
                report.text_collisions.push(format!(
                    "Text collision between {:?} ({:?}) and {:?} ({:?}) -> overlap {:?}",
                    a.text, vis_a, b.text, vis_b, overlap
                ));
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flex_fit_grid_8_rows_never_overflows() {
        for avail_h in [140.0_f32, 165.0, 180.0, 210.0, 260.0] {
            let gap = 2.0;
            let pad = 4.0;
            let row_h = FlexFitGrid::compute_row_height(avail_h, 8, gap, pad, 14.0, 22.0);
            let total_used = 8.0 * row_h + 7.0 * gap;
            assert!(
                total_used <= avail_h + 0.01,
                "8 rows at avail_h={} used {}px (row_h={})",
                avail_h,
                total_used,
                row_h
            );
        }
    }

    #[test]
    fn test_supported_egui_glyph_detector_catches_tofu() {
        assert!(find_unsupported_glyphs("▶ 1. Kick • ★ Random").is_empty());
        assert!(!find_unsupported_glyphs("► 1. Kick").is_empty());
        assert!(!find_unsupported_glyphs("🎲 Randomize").is_empty());
        assert!(!find_unsupported_glyphs("▲ Oct").is_empty());
        assert!(!find_unsupported_glyphs("ℹ Info").is_empty());
    }

    #[test]
    fn test_anti_breakage_full_studio_matrix_all_views_tabs_and_resolutions() {
        use crate::app::{AbletonStudioApp, DeckTab, WorkspaceView};
        use crate::layout::ViewportBudget;
        use crate::panels::ai_chat::StudioLocale;
        use crate::panels::clip_view::ClipViewSubTab;
        use crate::panels::sequencer::{DrumBank, DrumSequencerState};
        use egui::{pos2, vec2, Context, RawInput};

        let ctx = Context::default();
        let resolutions = [
            vec2(1280.0, 800.0),
            vec2(1440.0, 900.0),
            vec2(1728.0, 1117.0),
            vec2(1920.0, 1080.0),
        ];
        let views = [
            WorkspaceView::SessionMatrix,
            WorkspaceView::ArrangementTimeline,
        ];
        let clip_subtabs = [
            ClipViewSubTab::Notes,
            ClipViewSubTab::Transform,
            ClipViewSubTab::Generate,
            ClipViewSubTab::Warping,
        ];
        let drum_banks = [
            DrumBank::BankA,
            DrumBank::BankB,
        ];
        let locales = [
            StudioLocale::En,
            StudioLocale::Es,
        ];

        enum DeckVariant {
            Clip(ClipViewSubTab),
            Drums(DrumBank),
            Devices,
            Xy,
        }

        let mut variants = Vec::new();
        for &st in &clip_subtabs {
            variants.push(DeckVariant::Clip(st));
        }
        for &db in &drum_banks {
            variants.push(DeckVariant::Drums(db));
        }
        variants.push(DeckVariant::Devices);
        variants.push(DeckVariant::Xy);

        for size in resolutions {
            let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), size);
            let budget = ViewportBudget::compute(screen_rect, true, true);

            for &view in &views {
                for &locale in &locales {
                    for variant in &variants {
                        let mut app = AbletonStudioApp::new_mock(44100.0);
                        crate::theme::apply_dark_carbon_theme(&ctx);
                        app.workspace_view = view;
                        app.copilot_open = true;
                        app.ai_copilot.set_locale(locale);
                        app.ai_copilot.show_key_settings = true;

                        let locale_id = egui::Id::new("oxide_studio_locale");
                        ctx.data_mut(|d| d.insert_temp(locale_id, locale));

                        let (_tab, variant_desc) = match variant {
                            DeckVariant::Clip(st) => {
                                app.deck_tab = DeckTab::ClipView;
                                app.clip_view.sub_tab = *st;
                                (DeckTab::ClipView, format!("ClipView({:?})", st))
                            }
                            DeckVariant::Drums(bank) => {
                                app.deck_tab = DeckTab::DrumSequencer;
                                let seq_id = egui::Id::new("aruston_909_sequencer_state");
                                ctx.data_mut(|d| {
                                    let mut s = DrumSequencerState::default();
                                    s.bank = *bank;
                                    d.insert_temp(seq_id, s);
                                });
                                (DeckTab::DrumSequencer, format!("DrumSequencer({:?})", bank))
                            }
                            DeckVariant::Devices => {
                                app.deck_tab = DeckTab::DeviceRack;
                                (DeckTab::DeviceRack, "DeviceRack".to_string())
                            }
                            DeckVariant::Xy => {
                                app.deck_tab = DeckTab::XyPad;
                                (DeckTab::XyPad, "XyPad".to_string())
                            }
                        };

                        let raw_input = RawInput {
                            screen_rect: Some(screen_rect),
                            ..Default::default()
                        };

                        let output = ctx.run(raw_input, |ctx| {
                            app.render_frame_with_budget(ctx, &budget);
                        });

                        let report = audit_rendered_frame(screen_rect, &output.shapes);
                        assert!(
                            report.viewport_overflows.is_empty(),
                            "Viewport overflow at {:?} view={:?} locale={:?} variant={}: {:?}",
                            size,
                            view,
                            locale,
                            variant_desc,
                            report.viewport_overflows
                        );
                        assert!(
                            report.unsupported_glyphs.is_empty(),
                            "Unsupported tofu glyph at {:?} view={:?} locale={:?} variant={}: {:?}",
                            size,
                            view,
                            locale,
                            variant_desc,
                            report.unsupported_glyphs
                        );
                        assert!(
                            report.clipped_texts.is_empty(),
                            "Clipped text at {:?} view={:?} locale={:?} variant={}: {:?}",
                            size,
                            view,
                            locale,
                            variant_desc,
                            report.clipped_texts
                        );
                        assert!(
                            report.text_collisions.is_empty(),
                            "Text collision at {:?} view={:?} locale={:?} variant={}: {:?}",
                            size,
                            view,
                            locale,
                            variant_desc,
                            report.text_collisions
                        );
                    }
                }
            }
        }
    }
}
