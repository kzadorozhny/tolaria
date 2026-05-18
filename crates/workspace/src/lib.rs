//! Tolaria workspace root view and overlay layers (ADR-0115 Phase 1).
//!
//! This crate owns the top-level `TolariaWorkspace` GPUI view that is opened
//! as the single application window, plus the `ModalLayer` and `ToastLayer`
//! overlays that sit above all other content.
//!
//! Phase 2 will grow this crate with `Dock`, `Pane`, `PaneGroup`, and the
//! `Panel` trait, modelled on `zed/crates/workspace/src/`.

pub mod modal_layer;
pub mod toast_layer;
pub mod workspace;

pub use modal_layer::{ModalLayer, ModalView};
pub use toast_layer::ToastLayer;
pub use workspace::TolariaWorkspace;

#[cfg(test)]
mod tests {
    use gpui::{Context, IntoElement, ParentElement, Render, TestAppContext, Window};

    use crate::{modal_layer::ModalView, TolariaWorkspace};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Install the `gpui_component::Theme` global required by any primitive
    /// that reads it during render (mirrors `embed_poc/src/layout.rs:243`).
    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    // -----------------------------------------------------------------------
    // Dummy modal for testing
    // -----------------------------------------------------------------------

    struct DummyModal;

    impl ModalView for DummyModal {}

    impl Render for DummyModal {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div().child("modal content")
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// Constructing an empty workspace must not panic.
    #[gpui::test]
    fn empty_workspace_renders_without_panic(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(TolariaWorkspace::empty);
        cx.run_until_parked();
    }

    /// Pushing a dummy ModalView and then dismissing it must leave the
    /// active-modal flag false again — exercised through the public
    /// `TolariaWorkspace` API only (no field reach-in).
    #[gpui::test]
    fn modal_layer_accepts_and_dismisses_dummy_modal(cx: &mut TestAppContext) {
        install_theme(cx);

        let window = cx.add_window(TolariaWorkspace::empty);

        window
            .update(cx, |workspace, window, cx| {
                workspace.toggle_modal::<DummyModal, _>(window, cx, |_window, _cx| DummyModal);
            })
            .unwrap();

        let is_active = window
            .update(cx, |workspace, _window, cx| workspace.has_active_modal(cx))
            .unwrap();
        assert!(is_active, "modal should be active after toggle_modal");

        window
            .update(cx, |workspace, _window, cx| workspace.dismiss_modal(cx))
            .unwrap();

        let is_active_after = window
            .update(cx, |workspace, _window, cx| workspace.has_active_modal(cx))
            .unwrap();
        assert!(!is_active_after, "modal should not be active after dismiss");
    }

    /// Pushing a toast message must enqueue it on the `ToastLayer` — verified
    /// through `TolariaWorkspace::toast_count`, the test-only accessor.
    #[gpui::test]
    fn toast_layer_push_does_not_panic(cx: &mut TestAppContext) {
        install_theme(cx);

        let window = cx.add_window(TolariaWorkspace::empty);

        window
            .update(cx, |workspace, _window, cx| {
                workspace.push_toast("settings UI in Phase 2".into(), cx);
            })
            .unwrap();

        let len = window
            .update(cx, |workspace, _window, cx| workspace.toast_count(cx))
            .unwrap();
        assert_eq!(len, 1, "toast should be queued after push");
    }
}
