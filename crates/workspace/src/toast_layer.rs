//! Toast notification layer for `TolariaWorkspace` (ADR-0115 Phase 1).
//!
//! Phase 1 is a minimal skeleton: toasts are `SharedString` messages stored
//! in a `Vec`. The layer renders nothing while the list is empty. Phase 2
//! will wire up `gpui_component::notification::Notification` and the dismiss
//! timer.

use gpui::{div, Context, IntoElement, Render, SharedString, Window};

/// Layer that displays ephemeral toast messages above all other content.
#[derive(Default)]
pub struct ToastLayer {
    toasts: Vec<SharedString>,
}

impl ToastLayer {
    /// Construct an empty [`ToastLayer`] with no queued messages.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a toast message. Phase 2 will add auto-dismiss timers.
    pub fn push(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toasts.push(message.into());
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
        // Phase 1: renders nothing; Phase 2 maps toasts to Notification elements.
        div()
    }
}
