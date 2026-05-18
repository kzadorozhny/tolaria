#![forbid(unsafe_code)]
//! Side-by-side diff modal view for Tolaria (ADR-0115 Phase 2d).
//!
//! Renders a two-pane split: the left pane shows "before" content with
//! red-tinted backgrounds on removed lines; the right pane shows "after"
//! content with green-tinted backgrounds on added lines.
//!
//! Implements [`workspace::ModalView`] so it can be mounted into
//! [`workspace::TolariaWorkspace`] via `toggle_modal`.
//!
//! # Example
//!
//! ```text
//! workspace.toggle_modal::<DiffView, _>(window, cx, |_window, _cx| DiffView::demo());
//! ```

use std::collections::HashSet;

use gpui::{
    div, px, rgba, AnyElement, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Window,
};
use gpui_component::ActiveTheme;

// ---------------------------------------------------------------------------
// HunkKind
// ---------------------------------------------------------------------------

/// Distinguishes a diff hunk's change type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkKind {
    /// Line was added in the right ("after") side.
    Added,
    /// Line was removed from the left ("before") side.
    Removed,
    /// Unchanged context line present on both sides.
    Context,
}

// ---------------------------------------------------------------------------
// DiffHunk
// ---------------------------------------------------------------------------

/// A single diff hunk: a 1-based line number and its change kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// 1-based line number this hunk applies to.
    pub line: u32,
    /// The kind of change at this line.
    pub kind: HunkKind,
}

// ---------------------------------------------------------------------------
// DiffView
// ---------------------------------------------------------------------------

/// Full-screen modal that renders a side-by-side diff.
///
/// Constructed via [`DiffView::new`] for an empty placeholder or
/// [`DiffView::demo`] for a hardcoded demonstration diff.
pub struct DiffView {
    /// Raw "before" text, newline-separated.
    left: SharedString,
    /// Raw "after" text, newline-separated.
    right: SharedString,
    /// Hunks describing which lines were added, removed, or unchanged.
    hunks: Vec<DiffHunk>,
}

impl DiffView {
    /// Construct an empty [`DiffView`] with no content or hunks.
    pub fn new() -> Self {
        Self {
            left: SharedString::default(),
            right: SharedString::default(),
            hunks: Vec::new(),
        }
    }

    /// Construct a [`DiffView`] pre-populated with a simple demo diff.
    ///
    /// - Left (before): `"line 1\nold line 2\nline 3"`
    /// - Right (after):  `"line 1\nnew line 2\nline 3"`
    pub fn demo() -> Self {
        Self {
            left: "line 1\nold line 2\nline 3".into(),
            right: "line 1\nnew line 2\nline 3".into(),
            hunks: vec![
                DiffHunk {
                    line: 2,
                    kind: HunkKind::Removed,
                },
                DiffHunk {
                    line: 2,
                    kind: HunkKind::Added,
                },
            ],
        }
    }

    /// Returns the hunks describing which lines were added, removed, or
    /// unchanged. Line numbers are 1-based and must remain consistent with
    /// the `left`/`right` content.
    pub fn hunks(&self) -> &[DiffHunk] {
        &self.hunks
    }
}

impl Default for DiffView {
    fn default() -> Self {
        Self::new()
    }
}

impl workspace::ModalView for DiffView {} // marker impl — intentionally empty

// ---------------------------------------------------------------------------
// Render helpers
// ---------------------------------------------------------------------------

/// Subtle red background for removed lines (`#ff0000` at ~12 % opacity).
const REMOVED_BG: u32 = 0xff_00_00_1f;
/// Subtle green background for added lines (`#00aa00` at ~12 % opacity).
const ADDED_BG: u32 = 0x00_aa_00_1f;

/// Render one pane's lines, tinting any line whose 1-based index is in
/// `highlighted` with the given RGBA `tint` constant.
///
/// Takes the raw pane text and splits on `'\n'` internally to avoid an
/// intermediate `Vec<&str>` allocation at the call site.
fn render_pane_rows(text: &str, highlighted: &HashSet<u32>, tint: u32) -> Vec<AnyElement> {
    text.split('\n')
        .enumerate()
        .map(|(i, line)| {
            let line_no = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let row = div()
                .w_full()
                .px(px(8.0))
                .py(px(1.0))
                .text_xs()
                .font_family("Menlo")
                .child(SharedString::from(line));
            if highlighted.contains(&line_no) {
                row.bg(rgba(tint)).into_any_element()
            } else {
                row.into_any_element()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for DiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let border = cx.theme().border;

        let removed: HashSet<u32> = self
            .hunks
            .iter()
            .filter(|h| h.kind == HunkKind::Removed)
            .map(|h| h.line)
            .collect();

        let added: HashSet<u32> = self
            .hunks
            .iter()
            .filter(|h| h.kind == HunkKind::Added)
            .map(|h| h.line)
            .collect();

        let left_rows = render_pane_rows(&self.left, &removed, REMOVED_BG);
        let right_rows = render_pane_rows(&self.right, &added, ADDED_BG);

        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(bg)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .children(left_rows),
            )
            .child(div().w(px(1.0)).h_full().bg(border))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .children(right_rows),
            )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// An empty [`DiffView`] must render without panicking.
    #[gpui::test]
    fn empty_diff_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| DiffView::new());
        cx.run_until_parked();
    }

    /// [`DiffView::demo`] must produce a non-empty hunks vec.
    #[test]
    fn demo_renders_with_hunks() {
        let view = DiffView::demo();
        assert!(
            !view.hunks().is_empty(),
            "demo() should produce non-empty hunks, got: {:?}",
            view.hunks(),
        );
        assert_eq!(view.hunks().len(), 2, "expected exactly 2 demo hunks");
    }

    /// All three [`HunkKind`] variants must be pairwise distinct.
    #[test]
    fn hunk_kinds_distinct() {
        assert_ne!(HunkKind::Added, HunkKind::Removed, "Added vs Removed");
        assert_ne!(HunkKind::Added, HunkKind::Context, "Added vs Context");
        assert_ne!(HunkKind::Removed, HunkKind::Context, "Removed vs Context");
    }
}
