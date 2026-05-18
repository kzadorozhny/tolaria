//! `MockNoteItem` — stub `Item` for Phase 2a topology testing (Phase 2c
//! grew breadcrumb + banner composition on top of it).
//!
//! Renders a `breadcrumb_bar::BreadcrumbBar` derived from the vault-relative
//! path, an optional stack of `banners::Banner`s, and the centered "Editor
//! body — Phase 4" placeholder beneath.  Used by integration tests and the
//! `TOLARIA_MOCK=1` launch path to exercise `Pane` / `PaneGroup` without a
//! live vault.

use anyhow::Result;
use banners::{render_banner, Banner};
use breadcrumb_bar::BreadcrumbSegment;
use gpui::{
    div, px, App, Context, IntoElement, ParentElement, Render, SharedString, Styled, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    ActiveTheme,
};

use crate::item::Item;

// ---------------------------------------------------------------------------
// MockNoteItem
// ---------------------------------------------------------------------------

/// A test/mock note item that renders a breadcrumb header, optional banner
/// stack, and an editor placeholder.
pub struct MockNoteItem {
    /// Human-readable title shown in the tab strip.
    title: SharedString,
    /// Breadcrumb segments derived from the path supplied to [`new`] by
    /// splitting on `/`. Owned directly (no `Entity<BreadcrumbBar>` wrapper)
    /// because the bar here is stateless display data tied to the item.
    segments: Vec<BreadcrumbSegment>,
    /// Optional persistent banners (Archived, Conflict, RenameDetected, …).
    /// Phase 2c stacks them under the breadcrumb header; Phase 3 will wire
    /// them to live vault state.
    banners: Vec<Banner>,
}

impl MockNoteItem {
    /// Create a new mock note item.  The `path`'s `/`-delimited components
    /// become the breadcrumb [`BreadcrumbSegment`]s rendered above the
    /// editor placeholder.
    #[must_use]
    pub fn new(title: impl Into<SharedString>, path: impl Into<SharedString>) -> Self {
        let path = path.into();
        let segments: Vec<BreadcrumbSegment> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| BreadcrumbSegment {
                label: SharedString::from(s.to_owned()),
                icon: None,
            })
            .collect();
        Self {
            title: title.into(),
            segments,
            banners: Vec::new(),
        }
    }

    /// Attach a persistent banner to the item.  Builder pattern lets tests
    /// chain multiple banners without intermediate `let`s.
    #[must_use]
    pub fn with_banner(mut self, banner: Banner) -> Self {
        self.banners.push(banner);
        self
    }

    /// Number of attached banners (test-only accessor).
    #[cfg(test)]
    pub fn banner_count(&self) -> usize {
        self.banners.len()
    }

    /// Number of breadcrumb segments (test-only accessor).
    #[cfg(test)]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

impl Item for MockNoteItem {
    fn tab_content_text(&self, _cx: &App) -> SharedString {
        self.title.clone()
    }

    fn can_save(&self) -> bool {
        true
    }

    fn save(&mut self, _cx: &mut Context<Self>) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }
}

impl Render for MockNoteItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let divider_color = cx.theme().border;

        // Inline breadcrumb render — matches `breadcrumb_bar`'s visual but
        // skips the Entity wrapper since the segments are stateless display
        // data tied to the item.
        let breadcrumb = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(28.0))
            .px(px(8.0))
            .children(self.segments.iter().enumerate().flat_map(|(i, segment)| {
                let separator = (i > 0).then(|| {
                    div()
                        .px(px(4.0))
                        .text_color(divider_color)
                        .child("\u{203a}") // ›
                        .into_any_element()
                });
                let button = Button::new(("breadcrumb-mock", i))
                    .label(segment.label.clone())
                    .ghost()
                    .into_any_element();
                separator.into_iter().chain(std::iter::once(button))
            }));

        let banner_stack = div()
            .flex()
            .flex_col()
            .children(self.banners.iter().map(render_banner));

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(breadcrumb)
            .child(banner_stack)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .child("Editor body \u{2014} Phase 4"),
            )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use banners::Banner;
    use chrono::Utc;
    use gpui::TestAppContext;

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// `new()` derives breadcrumb segments from the `/`-delimited path.
    #[gpui::test]
    fn new_derives_breadcrumb_segments(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| {
            let item = MockNoteItem::new("Note A", "vault/notes/a.md");
            assert_eq!(
                item.segment_count(),
                3,
                "expected 3 segments for vault/notes/a.md"
            );
            item
        });
    }

    /// Empty path produces zero segments (no leading-empty entries).
    #[gpui::test]
    fn empty_path_has_no_segments(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| {
            let item = MockNoteItem::new("Untitled", "");
            assert_eq!(item.segment_count(), 0);
            item
        });
    }

    /// `with_banner` chains as a builder.
    #[gpui::test]
    fn with_banner_chains(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| {
            let item = MockNoteItem::new("Note B", "vault/b.md")
                .with_banner(Banner::ArchivedNote {
                    archived_at: Utc::now(),
                })
                .with_banner(Banner::TrashWarning { days_remaining: 7 });
            assert_eq!(item.banner_count(), 2);
            item
        });
    }
}
