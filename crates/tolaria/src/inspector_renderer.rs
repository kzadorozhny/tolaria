//! Tolaria-side rendering for GPUI's built-in [`gpui::Inspector`] dev-
//! tool (worklist 3.1 follow-up, expanded for worklist 10.1.3).
//!
//! GPUI ships an `Inspector` entity that tracks an active element id +
//! "picking" mode on a per-Window basis; its `Render` impl delegates the
//! actual UI to a callback stored on `App` (`cx.inspector_renderer`).
//! Without that callback the inspector view renders as `Empty`, which
//! means the `Cmd+Alt+I` toggle flips the internal state but no overlay
//! ever appears.  See `~/.cargo/git/checkouts/.../crates/gpui/src/inspector.rs`
//! around `impl Render for Inspector`.
//!
//! **Worklist 10.1.3** — the inspector UI now lives in a separate
//! borderless NSWindow rather than being composited into the workspace
//! window's top-right corner.  Architecture:
//!
//! - The workspace window stays in pick mode via the standard
//!   [`gpui::Window::toggle_inspector`] flag, so GPUI's per-paint
//!   `insert_inspector_hitbox` machinery keeps populating the
//!   workspace's `Inspector` entity from cursor hits.
//! - [`render_tolaria_inspector`] (the renderer GPUI invokes for the
//!   workspace's inspector) captures `cx.entity()` (an
//!   `Entity<gpui::Inspector>`) into an App-level [`InspectorBridge`]
//!   global on every paint, then returns `Empty` so nothing composites
//!   inside the workspace.  Capturing every frame is idempotent —
//!   GPUI's entity registry returns the same handle while the inspector
//!   is alive, and the cost is one global write per inspector-on paint.
//! - [`toggle_inspector_window`] (called from the
//!   `ToggleElementInspector` handler in `main.rs` right after
//!   `window.toggle_inspector(app_cx)`) opens or closes a separate
//!   window whose [`InspectorWindow`] view holds the captured inspector
//!   entity and `cx.observe`s it.  When the workspace's inspector
//!   updates (pick state, active element id), the observer re-renders
//!   the separate window.
//!
//! The separate-window approach matches the user-driven 10.1.3 ask:
//! Cmd+Alt+I now spawns a floating, draggable, resizable OS window with
//! the inspector content, freeing the workspace's top-right corner for
//! whatever live UI is anchored there.
//!
//! Gated on `#[cfg(debug_assertions)]` because both
//! [`gpui::Window::toggle_inspector`] and [`gpui::App::set_inspector_renderer`]
//! are gated on `cfg(any(feature = "inspector", debug_assertions))`,
//! and our workspace doesn't enable the `inspector` feature in release.

#![cfg(debug_assertions)]

use gpui::{
    div, point, px, size, AnyElement, App, AppContext as _, BorrowAppContext as _, Bounds, Context,
    Empty, Entity, EventEmitter, Global, Inspector, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowOptions,
};
use gpui_component::ActiveTheme;

/// Process-global bridge between the workspace's `Inspector` entity and
/// the separate inspector window.  GPUI's `Inspector` is Window-bound,
/// so the only place we can grab a handle to it is the renderer
/// callback (which receives `&mut Inspector` + a context that yields
/// `cx.entity()`).  Stashing the handle in an App global lets the
/// separate window observe the workspace's inspector even though they
/// live in different windows.
///
/// `window` carries the typed [`WindowHandle<InspectorWindow>`] for the
/// separate window so [`toggle_inspector_window`] can close it on the
/// inverse toggle without doing a workspace-wide window scan.
#[derive(Default)]
pub struct InspectorBridge {
    inspector: Option<Entity<Inspector>>,
    window: Option<WindowHandle<InspectorWindow>>,
}

impl Global for InspectorBridge {}

/// Install the bridge global if not already present, then run the
/// callback against a mutable reference to it.  Both this module's
/// rendering hot path and the toggle handler call this — keeping the
/// install-on-first-touch in one place avoids a startup-order
/// dependency on `main.rs`.
fn with_bridge<R>(cx: &mut App, f: impl FnOnce(&mut InspectorBridge) -> R) -> R {
    if !cx.has_global::<InspectorBridge>() {
        cx.set_global(InspectorBridge::default());
    }
    cx.update_global::<InspectorBridge, _>(|bridge, _cx| f(bridge))
}

/// Inspector renderer invoked by GPUI on every paint while the
/// workspace window has `window.inspector = Some(…)`.  Captures the
/// inspector entity into the [`InspectorBridge`] (so the separate
/// window can observe it) and returns `Empty` — the workspace itself
/// no longer paints a top-right inspector panel because that UI lives
/// in the separate [`InspectorWindow`] instead.
///
/// Wired in `main.rs` via
/// `cx.set_inspector_renderer(Box::new(render_tolaria_inspector))`.
pub fn render_tolaria_inspector(
    _inspector: &mut Inspector,
    _window: &mut Window,
    cx: &mut Context<Inspector>,
) -> AnyElement {
    let entity = cx.entity();
    with_bridge(cx, move |bridge| {
        bridge.inspector = Some(entity);
    });
    Empty.into_any_element()
}

/// Open / close the separate inspector window.  Idempotent — calling
/// twice in a row in the same direction (open + open) is a no-op.
/// Called from the `ToggleElementInspector` action handler in
/// `main.rs` immediately after `window.toggle_inspector(app_cx)` so
/// the workspace's pick mode and the separate window's lifetime stay
/// in sync.
///
/// `workspace_inspector_on` mirrors the workspace's
/// `Window.inspector.is_some()` state *after* the toggle has run:
/// `true` means "the workspace just turned the inspector on, open the
/// window"; `false` means "the workspace just turned it off, close
/// the window".  The caller computes this from the toggle's pre-state
/// because GPUI doesn't expose `Option<Inspector>` existence directly
/// (only [`Window::is_inspector_picking`], which checks the sub-flag
/// `Inspector::is_picking`, not whether the inspector exists at all).
pub fn toggle_inspector_window(workspace_inspector_on: bool, cx: &mut App) {
    if workspace_inspector_on {
        ensure_inspector_window_open(cx);
    } else {
        close_inspector_window(cx);
    }
}

fn ensure_inspector_window_open(cx: &mut App) {
    let existing = with_bridge(cx, |bridge| bridge.window);
    if existing.is_some() {
        return;
    }
    let Some(inspector) = with_bridge(cx, |bridge| bridge.inspector.clone()) else {
        log::warn!(
            "toggle_inspector_window: no inspector entity captured yet — \
             the workspace must paint at least once with `set_inspector_renderer` \
             installed before the separate window can mirror it"
        );
        return;
    };

    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(40.0), px(40.0)),
            size: size(px(360.0), px(480.0)),
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::new_static("Inspector — Tolaria")),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        ..Default::default()
    };

    match cx.open_window(opts, |_window, cx| {
        cx.new(|cx| InspectorWindow::new(inspector.clone(), cx))
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

/// Root view of the separate inspector window.  Observes the
/// workspace's [`gpui::Inspector`] entity so a pick state change or
/// active-element update in the workspace re-paints this window.
pub struct InspectorWindow {
    inspector: Entity<Inspector>,
    _observer: Subscription,
}

impl InspectorWindow {
    /// Construct the root view + subscribe to the inspector entity.
    /// The returned `Subscription` is stashed in `_observer` and lives
    /// as long as the view does; dropping it cancels the observation.
    pub fn new(inspector: Entity<Inspector>, cx: &mut Context<Self>) -> Self {
        let observer = cx.observe(&inspector, |_this, _ent, cx| cx.notify());
        Self {
            inspector,
            _observer: observer,
        }
    }
}

impl EventEmitter<()> for InspectorWindow {}

impl Render for InspectorWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.background;
        let border = theme.border;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;

        let inspector = self.inspector.read(cx);
        let is_picking = inspector.is_picking();
        let active_id = inspector.active_element_id().cloned();

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

        let (picking_label, picking_color) = if is_picking {
            (
                SharedString::new_static("Picking: ON — hover an element"),
                fg,
            )
        } else {
            (SharedString::new_static("Picking: OFF"), muted)
        };
        let picking_row = div()
            .px(px(10.0))
            .py(px(6.0))
            .text_xs()
            .text_color(picking_color)
            .child(picking_label);

        let active_row = match active_id {
            Some(id) => {
                let global_id_text: SharedString = format!("{:?}", id.path.global_id).into();
                let source_text: SharedString = format!(
                    "{}:{}",
                    id.path.source_location.file(),
                    id.path.source_location.line()
                )
                .into();
                div()
                    .px(px(10.0))
                    .py(px(6.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_xs()
                            .text_color(fg)
                            .child(SharedString::new_static("Active element")),
                    )
                    .child(div().text_xs().text_color(muted).child(global_id_text))
                    .child(div().text_xs().text_color(muted).child(source_text))
            }
            None => div()
                .px(px(10.0))
                .py(px(6.0))
                .text_xs()
                .text_color(muted)
                .child(SharedString::new_static("No element selected")),
        };

        div()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .flex()
            .flex_col()
            .child(header)
            .child(picking_row)
            .child(active_row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    /// Installing `render_tolaria_inspector` via the public
    /// `set_inspector_renderer` boxing path must accept our function
    /// signature.  Without a working install, gpui's `Inspector::render`
    /// falls back to `Empty` — see
    /// `~/.cargo/git/checkouts/.../crates/gpui/src/inspector.rs` around
    /// `impl Render for Inspector`.
    ///
    /// `Inspector::new` is `pub(crate)` in gpui, so we can't construct
    /// a `Context<Inspector>` in a unit test to invoke the closure
    /// directly.  Exercising `set_inspector_renderer(Box::new(...))`
    /// nonetheless catches the signature regression we care about
    /// (renderer parameters drifting away from `gpui::InspectorRenderer`).
    #[gpui::test]
    fn render_tolaria_inspector_signature_matches_gpui_renderer(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.set_inspector_renderer(Box::new(render_tolaria_inspector));
        });
        cx.run_until_parked();
    }

    /// Toggling the inspector on a live window must not panic when the
    /// renderer is installed.  This is the path `Cmd+Alt+I` actually
    /// drives in production: `cx.set_inspector_renderer(...)` first,
    /// then `window.toggle_inspector(cx)`.
    #[gpui::test]
    fn toggle_inspector_with_renderer_installed_does_not_panic(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.set_inspector_renderer(Box::new(render_tolaria_inspector));
        });
        let (_root, vcx) = cx.add_window_view(|_window, _cx| EmptyRoot);
        vcx.update(|window, cx| {
            window.toggle_inspector(cx);
        });
        vcx.run_until_parked();
    }

    /// Worklist 10.1.3 — `toggle_inspector_window(false, …)` with no
    /// previously-opened window must not panic and must not log an
    /// error: the bridge starts empty (no inspector captured, no
    /// window open), so closing is a no-op.  Pins the idempotency
    /// guard that lets `ToggleElementInspector` call this
    /// unconditionally after every workspace toggle.
    #[gpui::test]
    fn toggle_inspector_window_close_with_no_open_window_is_a_no_op(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            toggle_inspector_window(false, cx);
        });
        cx.run_until_parked();
    }

    /// Worklist 10.1.3 — `toggle_inspector_window(true, …)` before any
    /// workspace paint has captured an inspector entity logs a warning
    /// and returns instead of panicking.  This is the
    /// "Cmd+Alt+I pressed before the workspace painted its first
    /// frame" race; the user sees no separate window but the workspace
    /// still flips into pick mode.
    #[gpui::test]
    fn toggle_inspector_window_open_without_captured_entity_warns_and_returns(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            toggle_inspector_window(true, cx);
        });
        cx.run_until_parked();
    }

    /// A minimal root view used solely as the host for the
    /// `toggle_inspector` regression test.  Renders nothing — the test
    /// only cares that the toggle flips the inspector on without
    /// panicking when the Tolaria renderer is installed.
    struct EmptyRoot;

    impl gpui::Render for EmptyRoot {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::Empty
        }
    }
}
