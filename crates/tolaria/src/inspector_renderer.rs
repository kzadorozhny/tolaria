//! Tolaria-side renderer for GPUI's built-in [`gpui::Inspector`] view
//! (worklist 3.1 follow-up).
//!
//! GPUI ships an `Inspector` entity that tracks an active element id +
//! "picking" mode; its `Render` impl delegates the actual UI to a
//! callback stored on `App` (`cx.inspector_renderer`).  Without that
//! callback the inspector view renders as `Empty`, which means the
//! `Cmd+Alt+I` toggle flips the internal state but no overlay ever
//! appears.  See `~/.cargo/git/checkouts/.../crates/gpui/src/inspector.rs`
//! around `impl Render for Inspector`.
//!
//! This module supplies a minimal floating dev-tool panel anchored to
//! the top-right of the window.  It surfaces:
//!
//! - whether picking mode is active (with a "hover an element" hint),
//! - the `GlobalElementId` + source location of the currently active
//!   element, when one is selected.
//!
//! It is *not* meant to be a full Zed-style inspector — Zed's
//! `crates/inspector_ui` carries dependencies on LSP / project /
//! workspace state that have no analogue in Tolaria.  100 LOC of read-
//! only diagnostics is enough to prove the toggle works and gives the
//! user a usable surface for picking elements when triaging UI bugs.
//!
//! Gated on `#[cfg(debug_assertions)]` because both
//! [`gpui::Window::toggle_inspector`] and [`gpui::App::set_inspector_renderer`]
//! are gated on `cfg(any(feature = "inspector", debug_assertions))`,
//! and our workspace doesn't enable the `inspector` feature in release.

#![cfg(debug_assertions)]

use gpui::{
    div, px, AnyElement, Context, Inspector, IntoElement, ParentElement, SharedString, Styled,
    Window,
};
use gpui_component::ActiveTheme;

/// Floating dev-tool panel that renders the inspector state.
///
/// Wired in `main.rs` via
/// `cx.set_inspector_renderer(Box::new(render_tolaria_inspector))`.
///
/// The panel is anchored top-right with `absolute()` positioning so it
/// composites cleanly over whatever workspace surface is active and
/// doesn't push other chrome around.  Width is fixed at 360 pt to
/// match the size class of similar dev surfaces in `gpui_component`'s
/// own demos, with a 480-pt max height so deep `GlobalElementId`
/// chains can scroll if they ever exceed the panel.
pub fn render_tolaria_inspector(
    inspector: &mut Inspector,
    _window: &mut Window,
    cx: &mut Context<Inspector>,
) -> AnyElement {
    let theme = cx.theme();
    let bg = theme.background;
    let border = theme.border;
    let fg = theme.foreground;
    let muted = theme.muted_foreground;

    // Header row — title + dismiss hint.  The user toggles the panel
    // via `Cmd+Alt+I` or `View → Toggle Inspector`; surfacing that
    // shortcut here saves a round-trip to the keymap docs.
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

    // Picking-state indicator.  When picking is on, the user is
    // expected to hover (or click) over an element and the active id
    // updates accordingly.  Compute label + colour together so the
    // predicate is evaluated once and both literals stay zero-alloc
    // (`SharedString::new_static`) — renderers run on every frame the
    // inspector is open.
    let (picking_label, picking_color) = if inspector.is_picking() {
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

    // Active element details.  `InspectorElementPath` exposes
    // `global_id` (a `GlobalElementId`) and `source_location`
    // (a `&'static std::panic::Location`).  Both implement `Debug`;
    // we render the debug formatting because GPUI doesn't ship public
    // `Display` impls for either type.
    let active_row = match inspector.active_element_id() {
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
        .absolute()
        .top(px(8.0))
        .right(px(8.0))
        .w(px(360.0))
        .max_h(px(480.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .rounded(px(6.0))
        .shadow_lg()
        .flex()
        .flex_col()
        .child(header)
        .child(picking_row)
        .child(active_row)
        .into_any_element()
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
