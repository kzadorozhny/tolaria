//! `MockNoteItem` — stub `Item` for Phase 2a topology testing.
//!
//! Renders a breadcrumb-style note-path header above a centered placeholder for
//! the editor body (which arrives in Phase 4).  Used by integration tests and
//! the `TOLARIA_MOCK=1` launch path to demonstrate `Pane` / `PaneGroup`
//! without a live vault.

use anyhow::Result;
use gpui::{
    div, px, App, Context, IntoElement, ParentElement, Render, SharedString, Styled, Task, Window,
};

use crate::item::Item;

// ---------------------------------------------------------------------------
// MockNoteItem
// ---------------------------------------------------------------------------

/// A test/mock note item that renders a path header and an editor placeholder.
pub struct MockNoteItem {
    /// Human-readable title shown in the tab strip.
    title: SharedString,
    /// Vault-relative path shown in the breadcrumb header.
    path: SharedString,
}

impl MockNoteItem {
    /// Create a new mock note item.
    #[must_use]
    pub fn new(title: impl Into<SharedString>, path: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            path: path.into(),
        }
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            // Breadcrumb header showing the vault-relative path.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .py(px(6.0))
                    .text_sm()
                    .child(self.path.to_string()),
            )
            // Placeholder for the Phase 4 editor body.
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
