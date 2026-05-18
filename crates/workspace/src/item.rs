//! `Item` trait and `ItemHandle` object-safe wrapper (ADR-0115 Phase 2a).
//!
//! `Item` is implemented by content views hosted in a [`Pane`][crate::pane::Pane]
//! (e.g. `MockNoteItem`, and later the real per-note editor).  `ItemHandle` is
//! an object-safe wrapper that lets `Pane` store a `Vec<Box<dyn ItemHandle>>`
//! without knowing each item's concrete type.
//!
//! Phase 2a ships the minimal surface needed by `MockNoteItem`.  Navigation
//! history, dirty-state observers, and breadcrumb machinery are Phase 3+,
//! modelled on `zed/crates/workspace/src/item.rs:167–350`.

use anyhow::Result;
use gpui::{
    div, AnyElement, AnyView, App, Context, Entity, EntityId, IntoElement, ParentElement, Render,
    SharedString, Task,
};

// ---------------------------------------------------------------------------
// Item trait
// ---------------------------------------------------------------------------

/// A content view that can be hosted inside a [`Pane`][crate::pane::Pane].
///
/// Implementors must also implement [`Render`] so the pane can display them.
/// Phase 2a keeps the surface minimal; the full Zed `Item` API (`nav_history`,
/// `breadcrumbs`, save-as, reload, etc.) is Phase 3.
pub trait Item: Render + 'static {
    /// Short title shown in the tab strip.
    fn tab_content_text(&self, cx: &App) -> SharedString;

    /// Element rendered in the tab chip.
    ///
    /// Defaults to a plain text label using [`tab_content_text`][Item::tab_content_text].
    fn tab_content(&self, cx: &App) -> AnyElement {
        div().child(self.tab_content_text(cx)).into_any_element()
    }

    /// Optional icon name shown in the tab (e.g. a Phosphor identifier).
    ///
    /// Returns `None` by default.
    fn tab_icon(&self) -> Option<&str> {
        None
    }

    /// Whether this item supports the save operation.
    fn can_save(&self) -> bool {
        false
    }

    /// Persist the item.
    ///
    /// Only called when [`can_save`][Item::can_save] returns `true`.  The
    /// default implementation returns `Task::ready(Ok(()))`.
    fn save(&mut self, _cx: &mut Context<Self>) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    /// Whether the item has unsaved changes.
    fn is_dirty(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// ItemHandle — object-safe wrapper
// ---------------------------------------------------------------------------

/// Object-safe handle to an [`Entity<I: Item>`][Entity].
///
/// [`Pane`][crate::pane::Pane] stores `Vec<Box<dyn ItemHandle>>` so it can
/// hold items of different concrete types without monomorphisation.  Each
/// method delegates through the GPUI entity read/update API.
///
/// `Send + Sync` are intentionally absent: GPUI views are single-threaded and
/// `Entity<T>` is `!Send + !Sync`.
pub trait ItemHandle: 'static {
    /// Short title for the tab strip.
    fn tab_content_text(&self, cx: &App) -> SharedString;

    /// Optional icon name for the tab, as an owned [`SharedString`].
    fn tab_icon(&self, cx: &App) -> Option<SharedString>;

    /// Whether this item supports saving.
    fn can_save(&self, cx: &App) -> bool;

    /// Whether this item has unsaved changes.
    fn is_dirty(&self, cx: &App) -> bool;

    /// Persist the item, returning a `Task` that resolves when done.
    fn save(&self, cx: &mut App) -> Task<Result<()>>;

    /// The item as a renderable [`AnyView`].
    fn to_any(&self) -> AnyView;

    /// Stable entity identifier (for deduplication and equality checks).
    fn entity_id(&self) -> EntityId;
}

impl<I: Item> ItemHandle for Entity<I> {
    fn tab_content_text(&self, cx: &App) -> SharedString {
        self.read(cx).tab_content_text(cx)
    }

    fn tab_icon(&self, cx: &App) -> Option<SharedString> {
        // `tab_icon` borrows from the entity; convert to owned before the
        // temporary `&I` (and therefore the `cx` reborrow) is released.
        self.read(cx).tab_icon().map(SharedString::from)
    }

    fn can_save(&self, cx: &App) -> bool {
        self.read(cx).can_save()
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.read(cx).is_dirty()
    }

    fn save(&self, cx: &mut App) -> Task<Result<()>> {
        self.update(cx, |item, cx| item.save(cx))
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }

    fn entity_id(&self) -> EntityId {
        Entity::entity_id(self)
    }
}
