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
const FOCUS_TARGET: &str = "embed_poc::focus";
const IME_TARGET: &str = "embed_poc::ime";
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
                dispatch_ipc(body);
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

/// Structured result of parsing the `{k, v}` envelope the test page sends
/// over wry's IPC channel. Extracted as a pure function so the dispatch
/// table can be unit-tested without spinning up GPUI, AppKit, or wry.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum ParsedIpc {
    WebviewFocus { state: &'static str, target: String },
    Ime { phase: String, data: String, value_len: usize },
    Keydown { key: String, mods: String },
    Raw { body: String },
}

pub(crate) fn parse_ipc_body(body: &str) -> ParsedIpc {
    let Ok(envelope) = serde_json::from_str::<serde_json::Value>(body) else {
        return ParsedIpc::Raw { body: body.to_string() };
    };

    let kind = envelope.get("k").and_then(|v| v.as_str()).unwrap_or("?");
    let value = envelope.get("v").cloned().unwrap_or(serde_json::Value::Null);

    match kind {
        "focus" | "blur" => {
            let state = if kind == "focus" { "in" } else { "out" };
            let target = value.as_str().unwrap_or("?").to_string();
            ParsedIpc::WebviewFocus { state, target }
        }
        "composition" => {
            let phase = value
                .get("phase")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let data = value
                .get("data")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value_len = value
                .get("value")
                .and_then(|v| v.as_str())
                .map(|s| s.chars().count())
                .unwrap_or(0);
            ParsedIpc::Ime { phase, data, value_len }
        }
        "keydown" => {
            let key = value
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let mods = value
                .get("mods")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            ParsedIpc::Keydown { key, mods }
        }
        _ => ParsedIpc::Raw { body: body.to_string() },
    }
}

/// Emit the log line that corresponds to a parsed IPC event. Kept
/// separate from `parse_ipc_body` so tests can assert on the parsed
/// structure without grepping captured logs.
fn dispatch_ipc(body: &str) {
    match parse_ipc_body(body) {
        ParsedIpc::WebviewFocus { state, target } => {
            log::info!(
                target: FOCUS_TARGET,
                "webview_focus state={state} target={target}"
            );
        }
        ParsedIpc::Ime { phase, data, value_len } => {
            log::info!(
                target: IME_TARGET,
                "ime phase={phase} data={data:?} value_len={value_len}"
            );
        }
        ParsedIpc::Keydown { key, mods } => {
            log::info!(target: IPC_TARGET, "keydown key={key} mods={mods}");
        }
        ParsedIpc::Raw { body } => {
            log::info!(target: IPC_TARGET, "ipc raw={body}");
        }
    }
}

pub(crate) fn close_enough(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
    let dx = (f32::from(a.origin.x) - f32::from(b.origin.x)).abs();
    let dy = (f32::from(a.origin.y) - f32::from(b.origin.y)).abs();
    let dw = (f32::from(a.size.width) - f32::from(b.size.width)).abs();
    let dh = (f32::from(a.size.height) - f32::from(b.size.height)).abs();
    dx < EPSILON && dy < EPSILON && dw < EPSILON && dh < EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Point, Size as GpuiSize, px};

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(Point::new(px(x), px(y)), GpuiSize::new(px(w), px(h)))
    }

    #[test]
    fn close_enough_is_reflexive() {
        let r = b(10.0, 20.0, 300.0, 400.0);
        assert!(close_enough(r, r));
    }

    #[test]
    fn close_enough_accepts_sub_epsilon_drift() {
        let a = b(10.0, 20.0, 300.0, 400.0);
        let drifted = b(10.4, 20.4, 300.4, 400.4);
        assert!(close_enough(a, drifted), "drift below 0.5 px must be treated as same bounds");
    }

    #[test]
    fn close_enough_rejects_one_pixel_diff() {
        let a = b(10.0, 20.0, 300.0, 400.0);
        let moved = b(11.0, 20.0, 300.0, 400.0);
        assert!(!close_enough(a, moved));
    }

    #[test]
    fn close_enough_rejects_diff_at_exact_epsilon_boundary() {
        let a = b(10.0, 20.0, 300.0, 400.0);
        let at_boundary = b(10.5, 20.0, 300.0, 400.0);
        assert!(
            !close_enough(a, at_boundary),
            "guard uses strict < EPSILON; exactly 0.5 px diff must NOT be suppressed"
        );
    }

    #[test]
    fn close_enough_rejects_size_drift() {
        let a = b(0.0, 0.0, 300.0, 400.0);
        let resized = b(0.0, 0.0, 301.0, 400.0);
        assert!(!close_enough(a, resized));
    }

    #[test]
    fn parse_ipc_focus_event() {
        assert_eq!(
            parse_ipc_body(r#"{"k":"focus","v":"textarea"}"#),
            ParsedIpc::WebviewFocus { state: "in", target: "textarea".into() }
        );
    }

    #[test]
    fn parse_ipc_blur_event() {
        assert_eq!(
            parse_ipc_body(r#"{"k":"blur","v":"single-line"}"#),
            ParsedIpc::WebviewFocus { state: "out", target: "single-line".into() }
        );
    }

    #[test]
    fn parse_ipc_ime_compositionstart_empty() {
        assert_eq!(
            parse_ipc_body(
                r#"{"k":"composition","v":{"phase":"compositionstart","data":"","value":""}}"#
            ),
            ParsedIpc::Ime {
                phase: "compositionstart".into(),
                data: "".into(),
                value_len: 0,
            }
        );
    }

    #[test]
    fn parse_ipc_ime_value_len_counts_chars_not_bytes() {
        // 5 chars, 15 UTF-8 bytes. README's PASS criterion requires the
        // char count, otherwise CJK compositions report `value_len=15`
        // and the validation script counts them as failures.
        let parsed = parse_ipc_body(
            r#"{"k":"composition","v":{"phase":"compositionend","data":"こんにちは","value":"こんにちは"}}"#,
        );
        assert_eq!(
            parsed,
            ParsedIpc::Ime {
                phase: "compositionend".into(),
                data: "こんにちは".into(),
                value_len: 5,
            }
        );
    }

    #[test]
    fn parse_ipc_keydown() {
        assert_eq!(
            parse_ipc_body(r#"{"k":"keydown","v":{"key":"s","mods":"meta"}}"#),
            ParsedIpc::Keydown { key: "s".into(), mods: "meta".into() }
        );
    }

    #[test]
    fn parse_ipc_unknown_kind_falls_back_to_raw() {
        let body = r#"{"k":"button_click","v":42}"#;
        assert_eq!(parse_ipc_body(body), ParsedIpc::Raw { body: body.into() });
    }

    #[test]
    fn parse_ipc_malformed_json_falls_back_to_raw() {
        let body = "this is not json";
        assert_eq!(parse_ipc_body(body), ParsedIpc::Raw { body: body.into() });
    }

    #[test]
    fn parse_ipc_focus_with_missing_v_uses_question_mark_target() {
        assert_eq!(
            parse_ipc_body(r#"{"k":"focus"}"#),
            ParsedIpc::WebviewFocus { state: "in", target: "?".into() }
        );
    }
}
