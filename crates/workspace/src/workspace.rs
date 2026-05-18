//! `TolariaWorkspace` root view (ADR-0115 Phase 1).
//!
//! Phase 1 ships an empty shell: native title-bar spacer + centered
//! placeholder text + `ModalLayer` / `ToastLayer` overlays.  Docks, Panes,
//! Panels and the live service layer expand in Phase 2.

use gpui::{
    div, px, App, AppContext as _, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Window,
};

use crate::{
    modal_layer::{ModalLayer, ModalView},
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
}

impl TolariaWorkspace {
    /// Construct the root workspace view with an empty content area.
    ///
    /// Called from inside the `cx.add_window(|window, cx| …)` closure in
    /// `crates/tolaria/src/main.rs`; `cx` there is `&mut Context<Self>`,
    /// which implements `AppContext` so `cx.new(…)` works for sub-entities.
    ///
    /// Phase 2 will replace the placeholder content with live `Dock`/`Pane` layout.
    pub fn empty(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let modal_layer = cx.new(|_| ModalLayer::default());
        let toast_layer = cx.new(|_| ToastLayer::default());
        Self {
            modal_layer,
            toast_layer,
        }
    }

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
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            // Spacer matching the native macOS title bar height (~28 pt).
            .child(div().h(px(28.0)))
            // Centered placeholder content for Phase 1.
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Tolaria \u{2014} Phase 1 foundation"),
            )
            // Overlay layers rendered on top (absolute-positioned internally).
            .child(self.modal_layer.clone())
            .child(self.toast_layer.clone())
    }
}
