//! Toast notification layer for `TolariaWorkspace` (ADR-0115 Phase 2c).
//!
//! Phase 1 stored toasts as `SharedString`; Phase 2c switched to typed
//! [`Toast`]s from the `toasts` crate and renders each via [`render_toast`].
//! The layer pins itself to the top-right with a vertical stack.

use gpui::{div, px, Context, IntoElement, ParentElement, Render, Styled, Window};
use toasts::{render_toast, Toast};

/// Layer that displays ephemeral toast messages above all other content.
#[derive(Default)]
pub struct ToastLayer {
    toasts: Vec<Toast>,
}

impl ToastLayer {
    /// Construct an empty [`ToastLayer`] with no queued messages.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a [`Toast`]. Phase 4 will add auto-dismiss timers + max-length
    /// trimming; today the toast persists until the layer is dropped.
    pub fn push(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toasts.push(toast);
        cx.notify();
    }

    /// Number of currently queued toasts.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    /// Whether there are no queued toasts.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }
}

impl Render for ToastLayer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Top-right vertical stack of toasts. Empty layer renders an invisible
        // anchor div so the absolute-positioned overlay slot stays valid.
        div()
            .absolute()
            .right(px(16.0))
            .top(px(40.0))
            .flex()
            .flex_col()
            .gap_2()
            .children(self.toasts.iter().map(render_toast))
    }
}
