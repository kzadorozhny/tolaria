//! Two-pane sidebar layout for the ADR-0115 Phase 0 spike (tasks #3 + #4 + #5).
//!
//! Wraps gpui-component's `h_resizable` primitive so the draggable splitter
//! and clamping are handled upstream. The right pane hosts the embedded
//! WKWebView via `InstrumentedWebView` (see `crate::webview`), which adds
//! the ADR-0115 §4 epsilon-compare guard and frame-sync logging on top of
//! gpui-wry's default behavior.
//!
//! Three log streams feed task #8's validation script — all on the
//! `embed_poc::frame` target so a single `RUST_LOG=embed_poc::frame=info`
//! captures only frame-sync evidence:
//!   - `frame_event kind=sidebar_resize ...` on every committed splitter
//!     drag (`ResizablePanelGroup::on_resize`, fires at drag end).
//!   - `frame_event kind=window_resize ...` on every OS window
//!     content-area size change (`cx.observe_window_bounds`, deduped
//!     against pure window moves with a 0.5 px epsilon).
//!   - `frame_sync x= y= w= h=` info / `frame_sync_skip ...` debug from
//!     `InstrumentedWebView::prepaint`, one entry per repaint that
//!     actually changes the WebView's bounds.

use gpui::{
    App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Pixels, Render, Size, Styled, Window, div, px, rgb,
};
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_wry::WebView;

use crate::webview::{FrameSyncState, InstrumentedWebView, new_frame_sync_state};

const SIDEBAR_DEFAULT: f32 = 240.0;
const SIDEBAR_MIN: f32 = 160.0;
const SIDEBAR_MAX: f32 = 480.0;
const FRAME_TARGET: &str = "embed_poc::frame";
const FOCUS_TARGET: &str = "embed_poc::focus";

pub struct RootView {
    resizable_state: Entity<ResizableState>,
    webview: Entity<WebView>,
    webview_last_bounds: FrameSyncState,
    sidebar_focus: FocusHandle,
    last_viewport: Size<Pixels>,
}

impl RootView {
    pub fn new(
        webview: Entity<WebView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let resizable_state = cx.new(|_| ResizableState::default());
        let sidebar_focus = cx.focus_handle();

        cx.observe_window_bounds(window, |this, window, cx| {
            this.log_window_resize(window, cx);
        })
        .detach();

        cx.on_focus(&sidebar_focus, window, |_, _, _| {
            log::info!(
                target: FOCUS_TARGET,
                "gpui_focus state=in target=sidebar"
            );
        })
        .detach();
        cx.on_blur(&sidebar_focus, window, |_, _, _| {
            log::info!(
                target: FOCUS_TARGET,
                "gpui_focus state=out target=sidebar"
            );
        })
        .detach();

        Self {
            resizable_state,
            webview,
            webview_last_bounds: new_frame_sync_state(),
            sidebar_focus,
            last_viewport: window.viewport_size(),
        }
    }

    fn log_window_resize(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        if same_size(viewport, self.last_viewport) {
            return;
        }
        self.last_viewport = viewport;
        log::info!(
            target: FRAME_TARGET,
            "frame_event kind=window_resize viewport_w={:.1} viewport_h={:.1}",
            f32::from(viewport.width),
            f32::from(viewport.height),
        );
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.resizable_state.clone();
        let webview = self.webview.clone();
        let last_bounds = self.webview_last_bounds.clone();
        let sidebar_focus = self.sidebar_focus.clone();
        div().size_full().bg(rgb(0x1e1f24)).child(
            h_resizable("sidebar-layout")
                .with_state(&state)
                .child(
                    resizable_panel()
                        .size(px(SIDEBAR_DEFAULT))
                        .size_range(px(SIDEBAR_MIN)..px(SIDEBAR_MAX))
                        .flex_none()
                        .child(sidebar_panel(sidebar_focus)),
                )
                .child(resizable_panel().child(content_panel(webview, last_bounds)))
                .on_resize(|state, window, cx| log_sidebar_resize(state, window, cx)),
        )
    }
}

fn log_sidebar_resize(state: &Entity<ResizableState>, window: &mut Window, cx: &mut App) {
    let sidebar_w = state
        .read(cx)
        .sizes()
        .first()
        .copied()
        .unwrap_or(px(SIDEBAR_DEFAULT));
    let viewport = window.viewport_size();
    let content_w = (viewport.width - sidebar_w).max(px(0.0));
    let content_h = viewport.height;
    log::info!(
        target: FRAME_TARGET,
        "frame_event kind=sidebar_resize width={:.1} content_w={:.1} content_h={:.1}",
        f32::from(sidebar_w),
        f32::from(content_w),
        f32::from(content_h),
    );
}

fn sidebar_panel(focus: FocusHandle) -> impl IntoElement {
    div()
        .track_focus(&focus)
        .size_full()
        .bg(rgb(0x282a36))
        .text_color(rgb(0xe6e6e6))
        .p_3()
        .text_sm()
        .child("Sidebar")
}

fn content_panel(webview: Entity<WebView>, last_bounds: FrameSyncState) -> impl IntoElement {
    div()
        .size_full()
        .bg(rgb(0x12141a))
        .child(InstrumentedWebView::new(webview, last_bounds))
}

pub(crate) fn same_size(a: Size<Pixels>, b: Size<Pixels>) -> bool {
    const EPSILON: f32 = 0.5;
    (f32::from(a.width) - f32::from(b.width)).abs() < EPSILON
        && (f32::from(a.height) - f32::from(b.height)).abs() < EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    fn s(w: f32, h: f32) -> Size<Pixels> {
        Size::new(px(w), px(h))
    }

    #[test]
    fn same_size_is_reflexive() {
        let r = s(1200.0, 800.0);
        assert!(same_size(r, r));
    }

    #[test]
    fn same_size_ignores_sub_epsilon_drift() {
        // Window-bounds observers fire on pure window *moves* too; suppressing
        // sub-pixel size diffs keeps `frame_event kind=window_resize` quiet
        // when the user is only repositioning the window.
        assert!(same_size(s(1200.0, 800.0), s(1200.4, 800.4)));
    }

    #[test]
    fn same_size_detects_one_pixel_resize() {
        assert!(!same_size(s(1200.0, 800.0), s(1201.0, 800.0)));
        assert!(!same_size(s(1200.0, 800.0), s(1200.0, 801.0)));
    }

    #[test]
    fn same_size_uses_strict_epsilon() {
        // Exactly 0.5 px diff must NOT be suppressed — matches close_enough
        // in webview.rs, so the layout and webview guards agree on what
        // counts as "the same size."
        assert!(!same_size(s(1200.0, 800.0), s(1200.5, 800.0)));
    }
}
