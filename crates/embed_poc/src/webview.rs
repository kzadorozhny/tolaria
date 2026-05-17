//! WKWebView embedding for the ADR-0115 Phase 0 spike (task #4).
//!
//! Leverages longbridge/gpui-component's `gpui_wry::WebView` wrapper, which
//! already adds the underlying `wry::WebView` as a sibling `NSView` inside
//! the window's content view and re-applies `set_bounds(...)` on every
//! prepaint. The canonical NSView-as-sibling pattern this builds on lives
//! in upstream Zed at
//! `crates/gpui_macos/src/window.rs:783–884` (content view lookup,
//! autoresizing-mask wiring, `addSubview_`, and `makeFirstResponder_`).
//!
//! Frame-sync tightness instrumentation lands on top of this in task #5;
//! the stub `window.tolariaPoc.log(...)` JS bridge that task #7 will
//! extend feeds the `embed_poc::ipc` log target.

use gpui::{App, AppContext, Context, Entity, Window};
use gpui_wry::WebView;
use raw_window_handle::HasWindowHandle;
use wry::WebViewBuilder;

const TEST_PAGE_HTML: &str = include_str!("../assets/test-page.html");
const IPC_TARGET: &str = "embed_poc::ipc";

/// Construct the spike's WebView entity — built `as_child` of the current
/// `NSWindow`'s content view so it sits next to GPUI's Metal renderer.
pub fn spawn_test_webview(window: &mut Window, cx: &mut App) -> Entity<WebView> {
    cx.new(|cx: &mut Context<WebView>| {
        let window_handle = window
            .window_handle()
            .expect("window handle unavailable while building WebView");

        let builder = WebViewBuilder::new()
            .with_html(TEST_PAGE_HTML)
            .with_devtools(true)
            .with_ipc_handler(|req| {
                let body = req.body();
                log::info!(target: IPC_TARGET, "ipc raw={body}");
            });

        let webview = builder
            .build_as_child(&window_handle)
            .expect("failed to build child wry::WebView");

        WebView::new(webview, window, cx)
    })
}
