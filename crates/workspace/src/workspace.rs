//! `TolariaWorkspace` root view (ADR-0115 Phase 1 → 2a).
//!
//! Phase 1 shipped an empty shell.  Phase 2a grows it with the 3-dock +
//! `PaneGroup` topology:
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │ native title bar spacer (28 pt)     │
//! ├──────────┬──────────────┬───────────┤
//! │ Left     │              │ Right     │
//! │ Dock     │ PaneGroup    │ Dock      │
//! │          │ (centre)     │           │
//! ├──────────┴──────────────┴───────────┤
//! │ Bottom Dock                         │
//! ├─────────────────────────────────────┤
//! │ status bar slot (empty Phase 2a)    │
//! └─────────────────────────────────────┘
//! ModalLayer / ToastLayer rendered as overlays above all content.
//! ```
//!
//! Dock panels (Sidebar, Inspector, etc.) are added in Phase 2b.  The Phase 1
//! public API (`push_toast`, `toggle_modal`, `dismiss_modal`, `has_active_modal`,
//! `toast_count`) is unchanged.

use gpui::{
    div, px, App, AppContext as _, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Window,
};
use gpui_component::resizable::{h_resizable, resizable_panel};

use crate::{
    dock::Dock,
    modal_layer::{ModalLayer, ModalView},
    pane_group::PaneGroup,
    panel::DockPosition,
    toast_layer::ToastLayer,
};

/// Root GPUI view for the Tolaria application window.
///
/// Instantiate via [`TolariaWorkspace::empty`] inside `cx.add_window`'s root
/// closure; GPUI wraps the returned `Self` in an `Entity<TolariaWorkspace>`
/// automatically.
pub struct TolariaWorkspace {
    modal_layer: Entity<ModalLayer>,
    toast_layer: Entity<ToastLayer>,
    left_dock: Entity<Dock>,
    right_dock: Entity<Dock>,
    bottom_dock: Entity<Dock>,
    center_group: Entity<PaneGroup>,
}

impl TolariaWorkspace {
    /// Construct the root workspace view with the 3-dock + pane-group layout.
    ///
    /// All docks start empty and closed; Phase 2b chrome crates attach panels
    /// via [`Dock::set_panel`][crate::dock::Dock::set_panel].
    ///
    /// Called from inside the `cx.add_window(|window, cx| …)` closure in
    /// `crates/tolaria/src/main.rs`.
    pub fn empty(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let modal_layer = cx.new(|_| ModalLayer::default());
        let toast_layer = cx.new(|_| ToastLayer::default());
        let left_dock = cx.new(|_| Dock::new(DockPosition::Left));
        let right_dock = cx.new(|_| Dock::new(DockPosition::Right));
        let bottom_dock = cx.new(|_| Dock::new(DockPosition::Bottom));
        let center_group = cx.new(|_| PaneGroup::new());
        Self {
            modal_layer,
            toast_layer,
            left_dock,
            right_dock,
            bottom_dock,
            center_group,
        }
    }

    // -----------------------------------------------------------------------
    // Phase 1 public API — must remain intact through all Phase 2+ work.
    // -----------------------------------------------------------------------

    /// Show or toggle a modal view inside the workspace's `ModalLayer`.
    ///
    /// Re-entering with the same `V` type closes the active modal (toggle
    /// semantics, see `ModalLayer::toggle_modal`).
    pub fn toggle_modal<V, B>(&self, window: &mut Window, cx: &mut App, build: B)
    where
        V: ModalView,
        B: FnOnce(&mut Window, &mut Context<V>) -> V,
    {
        self.modal_layer
            .update(cx, |layer, cx| layer.toggle_modal(window, cx, build));
    }

    /// Dismiss the active modal, if any.
    pub fn dismiss_modal(&self, cx: &mut App) {
        self.modal_layer.update(cx, |layer, cx| layer.dismiss(cx));
    }

    /// Enqueue a toast message in the workspace's `ToastLayer`.
    pub fn push_toast(&self, message: SharedString, cx: &mut App) {
        self.toast_layer
            .update(cx, |layer, cx| layer.push(message, cx));
    }

    /// Whether a modal view is currently shown.
    pub fn has_active_modal(&self, cx: &App) -> bool {
        self.modal_layer.read(cx).has_active_modal()
    }

    /// Number of currently queued toasts (for testing).
    #[cfg(test)]
    pub fn toast_count(&self, cx: &App) -> usize {
        self.toast_layer.read(cx).len()
    }
}

impl Render for TolariaWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let left_dock = self.left_dock.clone();
        let right_dock = self.right_dock.clone();
        let center_group = self.center_group.clone();

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            // Native macOS title bar spacer (~28 pt).
            .child(div().h(px(28.0)))
            // Horizontal split: Left Dock | Center PaneGroup | Right Dock.
            // Phase 2b will wire ResizableState for drag-resize persistence.
            .child(
                div().flex_1().child(
                    h_resizable("workspace-main-layout")
                        .child(resizable_panel().child(left_dock))
                        .child(resizable_panel().child(center_group))
                        .child(resizable_panel().child(right_dock)),
                ),
            )
            // Bottom dock (empty placeholder in Phase 2a).
            .child(self.bottom_dock.clone())
            // Status bar slot (empty in Phase 2a).
            .child(div().h(px(24.0)))
            // Overlay layers rendered on top (absolute-positioned internally).
            .child(self.modal_layer.clone())
            .child(self.toast_layer.clone())
    }
}
