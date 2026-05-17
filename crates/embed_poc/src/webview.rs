//! WKWebView embedding for the ADR-0115 Phase 0 spike (tasks #4 + #5).
//!
//! `gpui_wry::WebView` (longbridge `crates/webview`, package name
//! `gpui-wry`) builds the underlying `wry::WebView` as a sibling `NSView`
//! inside the window's content view. The canonical NSView-as-sibling
//! pattern this builds on lives in upstream Zed at
//! `crates/gpui_macos/src/window.rs:783–884` (content view lookup,
//! autoresizing-mask wiring, `addSubview_`, `makeFirstResponder_`).
//!
//! Task #5 instrumentation: instead of rendering the upstream `WebView`
//! entity directly, we render `InstrumentedWebView`, a thin custom
//! `Element` that mirrors upstream `WebViewElement::prepaint` but adds:
//!   - an epsilon-compare guard (0.5 px tolerance) per ADR-0115 §4 so
//!     same-bounds re-prepaints don't ping `wry::WebView::set_bounds`;
//!   - `frame_sync x= y= w= h=` info logs on real changes and
//!     `frame_sync_skip ...` debug logs on suppressed noop calls.
//!
//! The WebView entity itself is *kept alive* by `RootView` (it owns
//! `Entity<WebView>`), which keeps the underlying `Rc<wry::WebView>`
//! alive, which keeps the NSView attached to the window. We just stop
//! routing through the upstream `Render` impl that would otherwise call
//! `set_bounds` unconditionally.

use std::{cell::Cell, rc::Rc};

use gpui::{
    App, AppContext, Bounds, Context, Element, ElementId, Entity, GlobalElementId, IntoElement,
    LayoutId, Pixels, Size as GpuiSize, Style, Window,
};
use gpui_wry::WebView;
use raw_window_handle::HasWindowHandle;
use wry::{
    Rect, WebViewBuilder,
    dpi::{self, LogicalPosition, LogicalSize},
};

const TEST_PAGE_HTML: &str = include_str!("../assets/test-page.html");
const IPC_TARGET: &str = "embed_poc::ipc";
const FRAME_TARGET: &str = "embed_poc::frame";

/// 0.5-logical-pixel epsilon per ADR-0115 §4.
const EPSILON: f32 = 0.5;

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

/// Shared state tracking the bounds we last pushed to `wry::WebView`.
/// Lives on `RootView` so it survives across render passes; cloned into
/// each freshly-constructed `InstrumentedWebView` element.
pub type FrameSyncState = Rc<Cell<Option<Bounds<Pixels>>>>;

pub fn new_frame_sync_state() -> FrameSyncState {
    Rc::new(Cell::new(None))
}

/// Custom Element wrapping a `WebView` entity that adds:
///   1. Epsilon-compare guard (0.5 px) so noop `set_bounds` calls are
///      logged at debug as `frame_sync_skip` instead of touching the
///      NSView each frame.
///   2. Info-level `frame_sync x= y= w= h=` on every committed bounds
///      change.
///
/// The bounds-translation math (logical pixels via `dpi::Size::Logical`
/// / `dpi::Position::Logical`) mirrors gpui-wry's `WebViewElement::prepaint`
/// at `crates/webview/src/lib.rs:178–204` — we deliberately stay on the
/// logical-pixel API and never multiply by the device scale factor.
pub struct InstrumentedWebView {
    webview: Entity<WebView>,
    last_bounds: FrameSyncState,
}

impl InstrumentedWebView {
    pub fn new(webview: Entity<WebView>, last_bounds: FrameSyncState) -> Self {
        Self { webview, last_bounds }
    }
}

impl IntoElement for InstrumentedWebView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InstrumentedWebView {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: GpuiSize::full(),
            flex_shrink: 1.0,
            ..Default::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let prev = self.last_bounds.get();
        let same = prev.map(|p| close_enough(p, bounds)).unwrap_or(false);

        if same {
            log::debug!(
                target: FRAME_TARGET,
                "frame_sync_skip x={:.1} y={:.1} w={:.1} h={:.1}",
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            );
            return;
        }

        log::info!(
            target: FRAME_TARGET,
            "frame_sync x={:.1} y={:.1} w={:.1} h={:.1}",
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        );

        // WebView derefs to wry::WebView, so set_bounds resolves to the
        // underlying wry call. Keeping the WebView entity in RootView
        // ensures the Rc<wry::WebView> + its NSView stay alive.
        let _ = self.webview.read(cx).set_bounds(Rect {
            size: dpi::Size::Logical(LogicalSize {
                width: bounds.size.width.into(),
                height: bounds.size.height.into(),
            }),
            position: dpi::Position::Logical(LogicalPosition::new(
                bounds.origin.x.into(),
                bounds.origin.y.into(),
            )),
        });
        self.last_bounds.set(Some(bounds));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

fn close_enough(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    let dx = (f32::from(a.origin.x) - f32::from(b.origin.x)).abs();
    let dy = (f32::from(a.origin.y) - f32::from(b.origin.y)).abs();
    let dw = (f32::from(a.size.width) - f32::from(b.size.width)).abs();
    let dh = (f32::from(a.size.height) - f32::from(b.size.height)).abs();
    dx < EPSILON && dy < EPSILON && dw < EPSILON && dh < EPSILON
}
