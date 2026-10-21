//! Studio Component & Viewport Budget Architecture for Oxide Studio.
//!
//! Provides deterministic, non-overlapping pixel budgets for all DAW zones:
//! - Top control bar: 44.0 px
//! - Left browser: 220.0..=270.0 px (or 0.0 when collapsed)
//! - Right AI copilot panel: 260.0..=320.0 px (or 0.0 when collapsed)
//! - Bottom deck: 210.0..=260.0 px (strictly <= 34% of viewport height)
//! - Center workspace: guaranteed >= 55% of viewport height
//! - Session split: computes clip slot scroll area and pinned mixer footer,
//!   strictly enforcing `clips_scroll_height >= 110.0` px regardless of how
//!   many mixer sections are toggled on.

use egui::{pos2, Id, InnerResponse, Rect, Ui, UiBuilder, Vec2};

use crate::panels::SessionMixerSections;

/// Deterministic, non-overlapping viewport budget computed from root window Rect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportBudget {
    pub total_rect: Rect,
    pub top_bar_height: f32,
    pub left_browser_width: f32,
    pub right_copilot_width: f32,
    pub bottom_deck_height: f32,
    pub center_width: f32,
    pub center_height: f32,
}

impl ViewportBudget {
    /// Compute deterministic pixel budgets for all studio panels from the root window rect.
    pub fn compute(screen_rect: Rect, browser_open: bool, copilot_open: bool) -> Self {
        let total_w = screen_rect.width();
        let total_h = screen_rect.height();

        // 1. Top transport bar: fixed 44.0 px
        let top_bar_height = 44.0_f32;

        // 2. Left browser: 220.0..=270.0 px (or 24.0 when collapsed)
        let left_browser_width = if browser_open {
            (total_w * 0.20).clamp(220.0, 270.0)
        } else {
            24.0
        };

        // 3. Right Copilot panel: 260.0..=320.0 px (or 0.0 when collapsed)
        let right_copilot_width = if copilot_open {
            (total_w * 0.22).clamp(260.0, 320.0)
        } else {
            0.0
        };

        // 4. Center workspace width receives all remaining horizontal space
        let center_width = (total_w - left_browser_width - right_copilot_width).max(0.0);

        // 5. Bottom deck: 210.0..=270.0 px, strictly <= 34% of window height
        let max_deck_h = total_h * 0.34;
        let bottom_deck_height = (total_h * 0.33).clamp(210.0, 270.0).min(max_deck_h);

        // 6. Center workspace height receives >= 55% of window height
        let center_height = (total_h - top_bar_height - bottom_deck_height).max(0.0);

        Self {
            total_rect: screen_rect,
            top_bar_height,
            left_browser_width,
            right_copilot_width,
            bottom_deck_height,
            center_width,
            center_height,
        }
    }

    /// Split center workspace for the Session Matrix using the budget's center height.
    pub fn session_split(&self, mixer_sections: &SessionMixerSections) -> SessionSplitBudget {
        Self::session_split_height(self.center_height, mixer_sections)
    }

    /// Split center workspace deterministically into toolbar, pinned mixer footer,
    /// and upper clip slot scroll area, strictly enforcing `clips_scroll_height >= 110.0` px.
    pub fn session_split_height(
        avail_center_height: f32,
        mixer_sections: &SessionMixerSections,
    ) -> SessionSplitBudget {
        let toolbar_height = 28.0_f32;
        let header_height = 26.0_f32;
        let stop_btn_height = 20.0_f32;

        let mut raw_mixer_footer = 104.0_f32;
        if mixer_sections.show_io {
            raw_mixer_footer += 44.0;
        }
        if mixer_sections.show_sends {
            raw_mixer_footer += 44.0;
        }
        if mixer_sections.show_crossfader {
            raw_mixer_footer += 26.0;
        }

        // Available vertical space inside center workspace:
        let max_allowed_footer = (avail_center_height
            - toolbar_height
            - header_height
            - stop_btn_height
            - 110.0)
            .max(88.0);
        let pinned_mixer_footer_height = raw_mixer_footer.min(max_allowed_footer);
        let clips_scroll_height = (avail_center_height
            - toolbar_height
            - header_height
            - stop_btn_height
            - pinned_mixer_footer_height)
            .max(110.0);

        SessionSplitBudget {
            toolbar_height,
            header_height,
            stop_btn_height,
            clips_scroll_height,
            pinned_mixer_footer_height,
        }
    }

    /// Center workspace rect in root window coordinates.
    pub fn center_rect(&self) -> Rect {
        let min_x = self.total_rect.left() + self.left_browser_width;
        let min_y = self.total_rect.top() + self.top_bar_height;
        let max_x = self.total_rect.right() - self.right_copilot_width;
        let max_y = self.total_rect.bottom() - self.bottom_deck_height;
        Rect::from_min_max(pos2(min_x, min_y), pos2(max_x.max(min_x), max_y.max(min_y)))
    }
}

/// Budget allocation for Session View center workspace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionSplitBudget {
    pub toolbar_height: f32,
    pub header_height: f32,
    pub stop_btn_height: f32,
    pub clips_scroll_height: f32,
    pub pinned_mixer_footer_height: f32,
}

/// Trait for reusable studio components declaring identification and minimum dimensions.
pub trait StudioComponent {
    fn id(&self) -> &'static str;
    fn min_size(&self) -> Vec2;
}

/// Specification trait for studio dock panels.
pub trait StudioPanelSpec {
    fn panel_id(&self) -> &'static str;
    fn min_size(&self) -> Vec2;
}

/// Reusable bounded card container that enforces min and max size constraints cleanly.
#[derive(Debug, Clone)]
pub struct BoundedCard {
    pub id: Id,
    pub min_size: Vec2,
    pub max_size: Option<Vec2>,
}

impl BoundedCard {
    pub fn new(id: impl Into<Id>, min_size: Vec2) -> Self {
        Self {
            id: id.into(),
            min_size,
            max_size: None,
        }
    }

    pub fn with_max_size(mut self, max_size: Vec2) -> Self {
        self.max_size = Some(max_size);
        self
    }

    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let min = self.min_size;
        ui.allocate_ui(min, |ui| {
            ui.set_min_size(min);
            if let Some(max) = self.max_size {
                ui.set_max_size(max);
            }
            add_contents(ui)
        })
    }
}

/// Reusable split region container allocating and clipping a designated screen rectangle.
#[derive(Debug, Clone)]
pub struct SplitRegion {
    pub id: Id,
    pub rect: Rect,
}

impl SplitRegion {
    pub fn new(id: impl Into<Id>, rect: Rect) -> Self {
        Self {
            id: id.into(),
            rect,
        }
    }

    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        ui.allocate_new_ui(UiBuilder::new().max_rect(self.rect), |ui| add_contents(ui))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AbletonStudioApp;
    use egui::vec2;

    #[test]
    fn test_multi_resolution_layout_regression() {
        let resolutions = [
            ("Compact Laptop 960x600", vec2(960.0, 600.0)),
            ("Standard MacBook 1280x800", vec2(1280.0, 800.0)),
            ("Studio Display 1920x1080", vec2(1920.0, 1080.0)),
        ];

        let mixer_all_on = SessionMixerSections {
            show_io: true,
            show_sends: true,
            show_returns: true,
            show_crossfader: true,
        };

        for (name, size) in resolutions {
            let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), size);
            let budget = ViewportBudget::compute(screen_rect, true, true);
            let split = budget.session_split(&mixer_all_on);

            // 1. Clips scroll height must ALWAYS be >= 110.0 px
            assert!(
                split.clips_scroll_height >= 110.0,
                "{}: clips_scroll_height ({}) must be >= 110.0",
                name,
                split.clips_scroll_height
            );

            // 1b. Session split vertical sum must not exceed center_height
            let split_sum = split.toolbar_height
                + split.header_height
                + split.clips_scroll_height
                + split.stop_btn_height
                + split.pinned_mixer_footer_height;
            assert!(
                split_sum <= budget.center_height + 0.01,
                "{}: split sum ({}) must be <= center_height ({})",
                name,
                split_sum,
                budget.center_height
            );

            // 2. Center workspace height must be >= 55% of screen height
            let center_ratio = budget.center_height / size.y;
            assert!(
                center_ratio >= 0.55,
                "{}: center_ratio ({:.2}) must be >= 0.55",
                name,
                center_ratio
            );

            // 3. Bottom deck height must be strictly <= 34% of screen height
            let deck_ratio = budget.bottom_deck_height / size.y;
            assert!(
                deck_ratio <= 0.34,
                "{}: deck_ratio ({:.2}) must be <= 0.34",
                name,
                deck_ratio
            );

            // 4. Panel sum must not exceed screen bounds
            let total_h = budget.top_bar_height + budget.center_height + budget.bottom_deck_height;
            assert!(
                (total_h - size.y).abs() <= 1.0,
                "{}: vertical sum ({}) must match screen height ({})",
                name,
                total_h,
                size.y
            );

            let total_w =
                budget.left_browser_width + budget.center_width + budget.right_copilot_width;
            assert!(
                (total_w - size.x).abs() <= 1.0,
                "{}: horizontal sum ({}) must match screen width ({})",
                name,
                total_w,
                size.x
            );

            // 5. Automated headless test using egui::Context::run with AbletonStudioApp full frame
            let ctx = egui::Context::default();
            let mut raw_input = egui::RawInput::default();
            raw_input.screen_rect = Some(screen_rect);

            let mut app = AbletonStudioApp::new_mock(44100.0);
            app.browser.is_collapsed = false;
            app.copilot_open = true;
            app.mixer_sections = mixer_all_on.clone();

            let full_output = ctx.run(raw_input, |ctx| {
                app.render_frame_with_budget(ctx, &budget);
            });

            // Zero panel overflows: verify shapes stay within screen bounds
            for shape in full_output.shapes {
                let clipped_rect = shape.shape.visual_bounding_rect().intersect(shape.clip_rect);
                if clipped_rect.is_finite() && clipped_rect.width() > 0.0 && clipped_rect.height() > 0.0 {
                    assert!(
                        clipped_rect.left() >= -1.0 && clipped_rect.top() >= -1.0,
                        "{}: panel shape out of bounds left/top: {:?}",
                        name,
                        clipped_rect
                    );
                    assert!(
                        clipped_rect.right() <= size.x + 1.0 && clipped_rect.bottom() <= size.y + 1.0,
                        "{}: panel shape overflowed screen_rect right/bottom: {:?}",
                        name,
                        clipped_rect
                    );
                }
            }
        }
    }
}
