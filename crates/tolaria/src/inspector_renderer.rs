//! Tolaria's separate-window inspector dev-tool (worklist 10.1.3,
//! superseding the in-workspace renderer from worklist 3.1).
//!
//! **Worklist 10.1.3 architecture.**  The inspector lives in its own
//! borderless NSWindow opened on `Cmd+Alt+I` (or `View → Show / Hide
//! Inspector`).  We deliberately **do not** call
//! [`gpui::Window::toggle_inspector`] on the workspace, because GPUI's
//! built-in inspector reserves a hard-coded 30-rem (≈480 pt) strip on
//! the right edge of the workspace's root layout — see
//! `gpui::Window::draw_roots` (`~/.cargo/git/checkouts/.../gpui/src/window.rs`
//! around the `if self.inspector.is_some() { … }` branch).  That width
//! shrink is unavoidable while `Window.inspector.is_some()`, which
//! made the workspace reflow every time the user opened the
//! inspector — the regression the user flagged as "main tolaria
//! window gets unnecessary resized" on the first 10.1.3 cut.
//!
//! We therefore manage the inspector's lifetime entirely from this
//! module:
//!
//! - [`InspectorBridge`] (App-level [`gpui::Global`]) carries an
//!   `Option<WindowHandle<InspectorWindow>>` — the typed handle to the
//!   open inspector window (or `None` when closed).
//! - [`toggle_inspector_window`] is the user-facing toggle.  It reads
//!   `bridge.window`'s shape to decide whether the next press opens or
//!   closes; the workspace window is left untouched.
//! - [`InspectorWindow`] is the root view of the separate OS window.
//!   It currently renders a static placeholder ("Element picker not
//!   yet wired") because GPUI's per-window `Inspector` is the only
//!   path that registers `insert_inspector_hitbox` from the per-paint
//!   pipeline, and we just opted out of that path.  Hooking the
//!   picker back up via a custom mouse listener + the `ui::tree_dump`
//!   registry is tracked separately so this row can land the
//!   "separate OS window" half of the ask without dragging the picker
//!   rewrite in with it.
//!
//! [`render_tolaria_inspector`] is retained as the
//! `cx.set_inspector_renderer` callback so that if a future change
//! ever re-enables `Window::toggle_inspector` on this workspace, the
//! in-workspace render path stays empty rather than reverting to
//! GPUI's default panel (which our chrome is no longer designed to
//! accommodate).  In the current 10.1.3 setup, `toggle_inspector` is
//! never called, so this callback is dormant.
//!
//! Gated on `#[cfg(debug_assertions)]` because
//! [`gpui::App::set_inspector_renderer`] is gated on
//! `cfg(any(feature = "inspector", debug_assertions))`, and our
//! workspace doesn't enable the `inspector` feature in release.

#![cfg(debug_assertions)]

use gpui::{
    div, px, AnyElement, App, AppContext as _, BorrowAppContext as _, Bounds, Context, Empty,
    Global, Inspector, IntoElement, ParentElement, Render, SharedString, Styled, TitlebarOptions,
    Window, WindowBounds, WindowHandle, WindowOptions,
};
use gpui_component::ActiveTheme;

/// Process-global state for the separate inspector window.
///
/// The bridge stores nothing more than the open-window handle.  Toggle
/// state is `bridge.window.is_some()`; menu labels and the
/// `ToggleElementInspector` handler both read this single source of
/// truth so the View-menu "Show / Hide Inspector" label and the
/// physical window stay in lockstep.
#[derive(Default)]
pub struct InspectorBridge {
    window: Option<WindowHandle<InspectorWindow>>,
}

impl Global for InspectorBridge {}

impl InspectorBridge {
    /// `true` when the separate inspector window is currently open.
    /// Used by [`crate::rebuild_menus_with_workspace`] to drive the
    /// View-menu label without re-reading per-Window inspector state
    /// (which is no longer the source of truth — see the
    /// "Worklist 10.1.3 architecture" note at the module head).
    #[must_use]
    pub fn is_window_open(&self) -> bool {
        self.window.is_some()
    }
}

/// Install the bridge global if not already present, then run the
/// callback against a mutable reference to it.  Both the toggle
/// handler and the menu rebuild path call this — keeping the
/// install-on-first-touch in one place avoids a startup-order
/// dependency on `main.rs`.
fn with_bridge<R>(cx: &mut App, f: impl FnOnce(&mut InspectorBridge) -> R) -> R {
    if !cx.has_global::<InspectorBridge>() {
        cx.set_global(InspectorBridge::default());
    }
    cx.update_global::<InspectorBridge, _>(|bridge, _cx| f(bridge))
}

/// Dormant `cx.set_inspector_renderer` callback.  Returns `Empty` so
/// that if a future change ever calls
/// [`gpui::Window::toggle_inspector`] on this workspace the
/// in-workspace panel stays empty — the separate window owns the UI.
/// Currently no callsite reaches this code path; see the
/// "Worklist 10.1.3 architecture" note at the module head.
pub fn render_tolaria_inspector(
    _inspector: &mut Inspector,
    _window: &mut Window,
    _cx: &mut Context<Inspector>,
) -> AnyElement {
    Empty.into_any_element()
}

/// Top-level toggle for the separate inspector window.  Called from
/// the `ToggleElementInspector` action handler in `main.rs`.
///
/// Reads `bridge.window` to decide open vs. close — the bridge is the
/// single source of truth for "is the inspector visible?".  This is
/// what lets us avoid `gpui::Window::toggle_inspector` (and its
/// hard-coded workspace-width shrink, see module head).
pub fn toggle_inspector_window(cx: &mut App) {
    let was_open = with_bridge(cx, |bridge| bridge.window.is_some());
    if was_open {
        close_inspector_window(cx);
    } else {
        ensure_inspector_window_open(cx);
    }
}

/// Width of the inspector window in logical points.  Wide enough to
/// hold a typical `GlobalElementId` debug-format chain on one line
/// before the picker-not-wired follow-up (`10.1.4`) fills the body
/// with real content.
const INSPECTOR_WINDOW_WIDTH_PT: f32 = 360.0;

/// Fallback inspector-window bounds for the early-startup race where
/// the workspace window isn't open yet (so
/// [`crate::macos::workspace_window_bounds`] returns `None`).  Keeps
/// the inspector visible at a sensible spot near the top-left rather
/// than failing to open.
const INSPECTOR_FALLBACK_BOUNDS: Bounds<gpui::Pixels> = Bounds {
    origin: gpui::Point {
        x: gpui::px(40.0),
        y: gpui::px(40.0),
    },
    size: gpui::Size {
        width: gpui::px(INSPECTOR_WINDOW_WIDTH_PT),
        height: gpui::px(480.0),
    },
};

fn ensure_inspector_window_open(cx: &mut App) {
    // Worklist 10.1.3 third follow-up — anchor the inspector flush
    // against the workspace's right edge, matching its height, rather
    // than the previous fixed `(40, 40)` / `360×480` placement.  The
    // bounds come from `Window::bounds()` on the workspace window
    // (global screen-coordinate space).  When no workspace window
    // exists yet (early startup race) fall back to the original
    // sensible default so the inspector still opens visibly.
    let inspector_bounds = crate::macos::workspace_window_bounds(cx)
        .map(|ws| Bounds {
            origin: gpui::Point {
                x: ws.origin.x + ws.size.width,
                y: ws.origin.y,
            },
            size: gpui::Size {
                width: px(INSPECTOR_WINDOW_WIDTH_PT),
                height: ws.size.height,
            },
        })
        .unwrap_or(INSPECTOR_FALLBACK_BOUNDS);

    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(inspector_bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::new_static("Inspector — Tolaria")),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        focus: true,
        show: true,
        ..Default::default()
    };

    match cx.open_window(opts, |window, cx| {
        // Worklist 10.1.3 follow-up — register a should-close hook so
        // the bridge state stays in sync when the user dismisses the
        // window via the macOS traffic-light close button (rather
        // than via Cmd+Alt+I).  Without this, `bridge.window` keeps a
        // stale handle, the next Cmd+Alt+I sees `is_some()`, takes
        // the close branch, and does nothing visible — the user
        // reported "Closing window using system close button does not
        // update the state".  The closure also fires the menu rebuild
        // so `View → Show Inspector` reads correctly after the close.
        window.on_window_should_close(cx, |_window, app_cx| {
            with_bridge(app_cx, |bridge| {
                bridge.window = None;
            });
            crate::macos::rebuild_menus(app_cx);
            true
        });
        cx.new(|_cx| InspectorWindow)
    }) {
        Ok(handle) => {
            with_bridge(cx, move |bridge| {
                bridge.window = Some(handle);
            });
        }
        Err(err) => {
            log::error!("toggle_inspector_window: open_window failed: {err:#}");
        }
    }
}

fn close_inspector_window(cx: &mut App) {
    let handle = with_bridge(cx, |bridge| bridge.window.take());
    let Some(handle) = handle else {
        return;
    };
    if let Err(err) = handle.update(cx, |_root, window, _cx| {
        window.remove_window();
    }) {
        log::warn!(
            "toggle_inspector_window: close failed: {err:#} \
             (window may have already been dismissed by the user)"
        );
    }
}

/// Root view of the separate inspector window.
///
/// Renders a static placeholder until the picker integration lands.
/// The picker rewrite is deferred so 10.1.3 can land the "separate OS
/// window" half of the ask without coupling it to the larger custom-
/// mouse-listener + `ui::tree_dump`-query work the picker would need
/// (since we explicitly opted out of GPUI's per-window `Inspector`
/// path — see the module head).
pub struct InspectorWindow;

impl Render for InspectorWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let border = theme.border;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(10.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .text_color(fg)
                    .text_sm()
                    .child(SharedString::new_static("Inspector — Tolaria")),
            )
            .child(
                div()
                    .text_color(muted)
                    .text_xs()
                    .child(SharedString::new_static("⌘⌥I to close")),
            );

        let body = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(10.0))
            .child(
                div()
                    .text_xs()
                    .text_color(fg)
                    .child(SharedString::new_static("Element picker: not yet wired")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::new_static(
                        "Worklist 10.1.3 lands the separate OS window.  Picker integration \
                 (custom mouse listener + tree_dump lookup) is a follow-up.",
                    )),
            );

        div()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .flex()
            .flex_col()
            .child(header)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    /// Installing `render_tolaria_inspector` via the public
    /// `set_inspector_renderer` boxing path must accept our function
    /// signature — even though the callback is dormant in 10.1.3's
    /// architecture, the signature must still match
    /// `gpui::InspectorRenderer` so a future caller of
    /// `Window::toggle_inspector` doesn't crash on install.
    #[gpui::test]
    fn render_tolaria_inspector_signature_matches_gpui_renderer(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.set_inspector_renderer(Box::new(render_tolaria_inspector));
        });
        cx.run_until_parked();
    }

    /// `toggle_inspector_window` with no prior state opens a fresh
    /// bridge and routes to the open branch; on the next call the
    /// bridge's `window.is_some()` flips and the close branch fires.
    /// Pins the single-source-of-truth invariant — we never touch
    /// `Window::toggle_inspector` (no workspace resize) and the
    /// open/close cycle reads from `bridge.window` alone.
    #[gpui::test]
    fn toggle_inspector_window_alternates_open_and_close(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            toggle_inspector_window(cx);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let open = cx
                .try_global::<InspectorBridge>()
                .is_some_and(InspectorBridge::is_window_open);
            assert!(
                open,
                "first toggle must open the inspector window (bridge.window = Some)"
            );
        });
        cx.update(|cx| {
            toggle_inspector_window(cx);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let open = cx
                .try_global::<InspectorBridge>()
                .is_some_and(InspectorBridge::is_window_open);
            assert!(
                !open,
                "second toggle must close the inspector window (bridge.window = None)"
            );
        });
    }

    /// `InspectorBridge::is_window_open` is the source of truth for the
    /// View-menu `Show / Hide Inspector` label.  Default is `false`
    /// (no window open) so menu labels lay down as "Show …" before
    /// the user ever presses Cmd+Alt+I.
    #[test]
    fn inspector_bridge_default_is_closed() {
        let bridge = InspectorBridge::default();
        assert!(!bridge.is_window_open());
    }
}
