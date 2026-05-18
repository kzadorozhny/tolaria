//! `Pane` view — ordered list of open items with an active index (ADR-0115 Phase 2a).
//!
//! Phase 2a ships a single-tab-capable pane: a `Vec<Box<dyn ItemHandle>>` and
//! an active index.  The tab-strip UI, drag-to-reorder, and multi-pane splits
//! are Phase 2b additions, modelled on `zed/crates/workspace/src/pane.rs:397`.

use gpui::{div, AnyElement, Context, IntoElement, ParentElement, Render, Styled, Window};

use crate::item::ItemHandle;

// ---------------------------------------------------------------------------
// Activation
// ---------------------------------------------------------------------------

/// Controls whether a newly added item immediately becomes the active (visible)
/// item in the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Make the new item the active (visible) item.
    Activate,
    /// Append the item without changing the currently active item.
    Defer,
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

/// An ordered list of open [`Item`][crate::item::Item]s with one active.
///
/// Phase 2a renders the active item in the full available area.  The tab
/// strip, split controls, and drag-to-reorder are Phase 2b.
pub struct Pane {
    items: Vec<Box<dyn ItemHandle>>,
    /// Index into `items` of the currently visible item.
    ///
    /// # Invariant
    /// `active_item_index < items.len()` whenever `items` is non-empty.
    /// When `items` is empty this field is `0` (a harmless sentinel).
    active_item_index: usize,
}

impl Pane {
    /// Create an empty pane with no items.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            active_item_index: 0,
        }
    }

    /// Append `item` to the pane.
    ///
    /// If `activation` is [`Activation::Activate`] the new item becomes the
    /// active one immediately.
    ///
    /// # Postconditions
    /// `item_count()` is one greater than before.  When `activation ==
    /// Activation::Activate`, `active_item()` returns the newly appended item.
    pub fn add_item(
        &mut self,
        item: impl ItemHandle + 'static,
        activation: Activation,
        cx: &mut Context<Self>,
    ) {
        self.items.push(Box::new(item));
        if activation == Activation::Activate {
            self.active_item_index = self.items.len() - 1;
        }
        cx.notify();
    }

    /// The currently active item, or `None` if the pane is empty.
    pub fn active_item(&self) -> Option<&dyn ItemHandle> {
        self.items.get(self.active_item_index).map(|b| b.as_ref())
    }

    /// Close the active item and activate the preceding one.
    ///
    /// No-op when the pane is already empty.
    pub fn close_active(&mut self, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        self.items.remove(self.active_item_index);
        self.active_item_index = if self.items.is_empty() {
            0
        } else {
            self.active_item_index.saturating_sub(1)
        };
        cx.notify();
    }

    /// Number of items currently open in this pane.
    pub fn item_count(&self) -> usize {
        self.items.len()
    }
}

impl Default for Pane {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let content: AnyElement = if let Some(item) = self.items.get(self.active_item_index) {
            item.to_any().into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child("No items open")
                .into_any_element()
        };
        div().size_full().child(content)
    }
}
