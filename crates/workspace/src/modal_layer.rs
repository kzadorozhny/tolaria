//! Modal overlay layer for `TolariaWorkspace` (ADR-0115 Phase 1).
//!
//! Intentionally minimal for Phase 1: stores one `AnyView` at a time, renders
//! it as an absolute-positioned overlay, and exposes `toggle_modal` / `dismiss`
//! for callers. Focus management, `DismissEvent` subscriptions, and the full
//! `ManagedView` machinery are Phase 2 additions modelled on
//! `zed/crates/workspace/src/modal_layer.rs:78–178`.

use gpui::{
    div, AnyView, AppContext as _, Context, IntoElement, ParentElement, Render, Styled, Window,
};

// ---------------------------------------------------------------------------
// ModalView trait
// ---------------------------------------------------------------------------

/// Marker trait for views that can be mounted into a [`ModalLayer`].
///
/// Phase 1 requires only that the view is renderable. Phase 2 will extend
/// this with `on_before_dismiss`, `fade_out_background`, and
/// `EventEmitter<DismissEvent>` in line with the Zed pattern.
pub trait ModalView: Render + 'static {}

// ---------------------------------------------------------------------------
// ModalLayer
// ---------------------------------------------------------------------------

/// Stores and renders at most one active modal view as a full-screen
/// absolute-positioned overlay.
#[derive(Default)]
pub struct ModalLayer {
    pub(crate) active: Option<AnyView>,
}

impl ModalLayer {
    /// Construct an empty [`ModalLayer`] with no active modal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Show a modal of type `V`, or dismiss it if the same type is already
    /// active (toggle semantics).
    ///
    /// Type identity is determined by `AnyView::downcast::<V>`: two calls with
    /// the same concrete `V` toggle the modal off; different types replace the
    /// active modal. `build` receives `(&mut Window, &mut Context<V>)` so the
    /// caller can wire observers, focus handles, or other window-scoped setup
    /// at construction time.
    pub fn toggle_modal<V, B>(&mut self, window: &mut Window, cx: &mut Context<Self>, build: B)
    where
        V: ModalView,
        B: FnOnce(&mut Window, &mut Context<V>) -> V,
    {
        // If the same type is already active, toggle it off.
        if let Some(existing) = &self.active {
            if existing.clone().downcast::<V>().is_ok() {
                self.active = None;
                cx.notify();
                return;
            }
        }

        let view = cx.new(|cx| build(window, cx));
        self.active = Some(view.into());
        cx.notify();
    }

    /// Dismiss the active modal, if any.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.active.take().is_some() {
            cx.notify();
        }
    }

    /// Whether a modal is currently shown.
    pub fn has_active_modal(&self) -> bool {
        self.active.is_some()
    }
}

impl Render for ModalLayer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(ref modal) = self.active else {
            return div().into_any_element();
        };

        div()
            .absolute()
            .size_full()
            .inset_0()
            .child(modal.clone())
            .into_any_element()
    }
}
