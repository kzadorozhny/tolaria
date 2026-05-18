//! Top-of-pane breadcrumb bar view (ADR-0115 Phase 2b).
//!
//! `BreadcrumbBar` is a GPUI view that renders a horizontal trail of
//! [`BreadcrumbSegment`]s separated by "›" glyphs.  Each non-terminal
//! segment renders as a ghost [`gpui_component::button::Button`]; the
//! terminal segment renders in stronger (foreground) text without a click
//! target.

use gpui::{div, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme, Sizable as _, StyledExt as _,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single segment in the breadcrumb trail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreadcrumbSegment {
    /// Display label for this segment.
    pub label: SharedString,
    /// Optional icon name (e.g. a folder or file icon).  Not rendered in
    /// Phase 2b but stored so callers can populate it without an API break.
    pub icon: Option<SharedString>,
}

impl BreadcrumbSegment {
    /// Convenience constructor — no icon.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
        }
    }
}

/// Top-of-pane breadcrumb bar view.
///
/// # Example
///
/// ```ignore
/// let bar = cx.new(|_| {
///     BreadcrumbBar::with_segments(vec![
///         BreadcrumbSegment::new("Vault"),
///         BreadcrumbSegment::new("Notes"),
///         BreadcrumbSegment::new("my-note.md"),
///     ])
/// });
/// ```
pub struct BreadcrumbBar {
    segments: Vec<BreadcrumbSegment>,
}

impl BreadcrumbBar {
    /// Create an empty breadcrumb bar.
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Create a breadcrumb bar pre-populated with `segments`.
    pub fn with_segments(segments: Vec<BreadcrumbSegment>) -> Self {
        Self { segments }
    }

    /// Return a slice of the current segments.
    pub fn segments(&self) -> &[BreadcrumbSegment] {
        &self.segments
    }

    /// Append `segment` to the trail.
    pub fn push(&mut self, segment: BreadcrumbSegment) {
        self.segments.push(segment);
    }

    /// Remove all segments from the trail.
    pub fn clear(&mut self) {
        self.segments.clear();
    }
}

impl Default for BreadcrumbBar {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for BreadcrumbBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.segments.len();
        let last_ix = count.checked_sub(1);

        let mut children: Vec<gpui::AnyElement> = Vec::with_capacity(count * 2);

        for (ix, segment) in self.segments.iter().enumerate() {
            let label = segment.label.clone();
            let is_last = Some(ix) == last_ix;

            if is_last {
                // Terminal segment: non-clickable, foreground colour.
                children.push(
                    div()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .font_semibold()
                        .child(label)
                        .into_any_element(),
                );
            } else {
                // Non-terminal segment: ghost button.
                // Namespace the element ID to avoid collisions when multiple
                // BreadcrumbBars render in the same frame.
                children.push(
                    Button::new(("breadcrumb", ix))
                        .label(label)
                        .ghost()
                        .small()
                        .into_any_element(),
                );
                // Separator glyph.
                children.push(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("›")
                        .into_any_element(),
                );
            }
        }

        h_flex().h_7().items_center().gap_1().children(children)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::{BreadcrumbBar, BreadcrumbSegment};

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    fn seg(label: &'static str) -> BreadcrumbSegment {
        BreadcrumbSegment::new(label)
    }

    // -----------------------------------------------------------------------

    /// An empty bar must render without panicking.
    #[gpui::test]
    fn empty_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| BreadcrumbBar::new());
        cx.run_until_parked();
    }

    /// Pushing three segments and reading them back must produce the same data.
    #[gpui::test]
    fn with_segments_round_trips(cx: &mut TestAppContext) {
        install_theme(cx);

        let window = cx.add_window(|_window, _cx| {
            BreadcrumbBar::with_segments(vec![seg("Vault"), seg("Notes"), seg("my-note.md")])
        });

        window
            .update(cx, |bar: &mut BreadcrumbBar, _window, _cx| {
                let segs = bar.segments();
                assert_eq!(segs.len(), 3);
                assert_eq!(segs[0].label, "Vault");
                assert_eq!(segs[1].label, "Notes");
                assert_eq!(segs[2].label, "my-note.md");
            })
            .unwrap();
    }

    /// After `clear`, the segment list must be empty.
    #[gpui::test]
    fn clear_resets_segments(cx: &mut TestAppContext) {
        install_theme(cx);

        let window = cx.add_window(|_window, _cx| BreadcrumbBar::new());

        window
            .update(cx, |bar: &mut BreadcrumbBar, _window, _cx| {
                bar.push(seg("Vault"));
                bar.push(seg("Notes"));
                assert_eq!(bar.segments().len(), 2);

                bar.clear();
                assert_eq!(bar.segments().len(), 0);
            })
            .unwrap();
    }

    /// A bar with three segments must render without panicking.
    #[gpui::test]
    fn render_with_segments_no_panic(cx: &mut TestAppContext) {
        install_theme(cx);

        let window = cx.add_window(|_window, _cx| {
            BreadcrumbBar::with_segments(vec![seg("Vault"), seg("Notes"), seg("my-note.md")])
        });

        // `render` is driven by the event loop; parking triggers a layout pass.
        cx.run_until_parked();

        // Confirm the view is still accessible after the render cycle.
        window
            .update(cx, |bar: &mut BreadcrumbBar, _window, _cx| {
                assert_eq!(bar.segments().len(), 3);
            })
            .unwrap();
    }
}
