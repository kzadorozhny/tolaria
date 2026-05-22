//! Tolaria-side renderer for GPUI's built-in [`gpui::Inspector`] view
//! (worklist 3.1 follow-up; worklist 10.1.4 revision).
//!
//! GPUI ships an `Inspector` entity that tracks an active element id +
//! "picking" mode; its `Render` impl delegates the actual UI to a
//! callback stored on `App` (`cx.inspector_renderer`).  Without that
//! callback the inspector view renders as `Empty`, which means the
//! `Cmd+Alt+I` toggle flips the internal state but no overlay ever
//! appears.  See `~/.cargo/git/checkouts/.../crates/gpui/src/inspector.rs`
//! around `impl Render for Inspector`.
//!
//! **Worklist 10.1.4 architecture (option A).**  The inspector is
//! GPUI's built-in [`Window::toggle_inspector`] pane composited as a
//! 30-rem (~480 pt) strip on the right edge of the workspace window
//! — the per-paint code in `gpui::Window::draw_roots` always reserves
//! that strip while `Window.inspector.is_some()` (see the
//! `if self.inspector.is_some() { size.width -= 30rem }` block).
//! We use GPUI's built-in path because it's the only one that
//! populates `insert_inspector_hitbox` per-paint from *every*
//! interactive element (broader coverage than the `.dump_as`-tagged
//! subset that `ui::tree_dump` exposes).
//!
//! To avoid the workspace's visible region shrinking when the
//! inspector opens (the user-flagged "main tolaria window gets
//! unnecessary resized" regression on the 10.1.3 first cut), the
//! toggle handler in `main.rs` *grows the workspace window* by
//! `INSPECTOR_PANE_WIDTH_PT` (the same 30 rems GPUI carves off) when
//! the inspector opens, and *shrinks it back* when the inspector
//! closes.  Net effect: the workspace's visible chrome stays the
//! same size on screen; the inspector pane appears as additional
//! width on the right edge of the (now-wider) window.
//!
//! The renderer below paints a small dev-tool panel showing the
//! picking state + the currently-active element.  It also auto-
//! enables picking on every render via [`gpui::Inspector::start_picking`]
//! so the user doesn't have to hunt for a "start" button — Cmd+Alt+I
//! → pane appears → hovering a workspace element immediately updates
//! the "Active element" row.  `start_picking` is idempotent so
//! calling it on every paint is cheap.
//!
//! Gated on `#[cfg(debug_assertions)]` because both
//! [`gpui::Window::toggle_inspector`] and [`gpui::App::set_inspector_renderer`]
//! are gated on `cfg(any(feature = "inspector", debug_assertions))`,
//! and our workspace doesn't enable the `inspector` feature in
//! release.

#![cfg(debug_assertions)]

use gpui::{
    div, px, rems, AnyElement, Context, Inspector, IntoElement, ParentElement, Pixels,
    SharedString, Styled, Window,
};
use gpui_component::ActiveTheme;

/// Width GPUI's `draw_roots` carves off the workspace's root layout
/// while the inspector is open — `rems(30.0).to_pixels(rem_size)`.
/// We compute this lazily at toggle time (because `rem_size` is a
/// per-window value) and use it both to grow the workspace window
/// when the inspector opens and to shrink it when the inspector
/// closes.  Keeping the constant in one place anchors the
/// "grow-by-the-amount-GPUI-shrinks-by" invariant against a future
/// gpui upstream that picks a different rem multiple.
pub(crate) fn inspector_pane_width(window: &Window) -> Pixels {
    rems(30.0).to_pixels(window.rem_size())
}

/// Renderer wired via `cx.set_inspector_renderer` in `main.rs`.  Paints
/// the in-workspace inspector pane and auto-enables picking so a fresh
/// inspector toggle is immediately useful.
///
/// Three rows: header (title + dismiss hint), picking state,
/// active-element details (or "No element selected").  Width is
/// taken from GPUI's reserved 30-rem strip (the renderer is
/// prepainted into a fixed-width box by
/// `gpui::Window::prepaint_inspector`), so we use `size_full` to fill
/// it cleanly.
pub fn render_tolaria_inspector(
    inspector: &mut Inspector,
    _window: &mut Window,
    cx: &mut Context<Inspector>,
) -> AnyElement {
    // Auto-enable picking so the user doesn't have to find a "start"
    // button — Cmd+Alt+I → hover any element → "Active element" row
    // populates.  `start_picking` flips a `bool` on the entity;
    // calling it when already picking is a no-op (cheap to run on
    // every paint).
    if !inspector.is_picking() {
        inspector.start_picking();
    }

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
        .size_full()
        .bg(bg)
        .text_color(fg)
        .flex()
        .flex_col()
        .child(header)
        .child(picking_row)
        .child(active_row)
        .into_any_element()
}

/// Process-global flag tracking whether the workspace's GPUI
/// inspector pane is currently open.  GPUI doesn't expose
/// `Window.inspector.is_some()` directly (only `is_inspector_picking`,
/// which reads the sub-flag, not the option), so we track our own
/// bool alongside every `Window::toggle_inspector` call.
///
/// Read by the menu rebuild path (so the "Show / Hide Inspector"
/// label tracks the pane's open/close state without depending on the
/// `is_picking` sub-state) and written by the
/// `ToggleElementInspector` handler in `main.rs`.
#[derive(Default)]
pub struct InspectorPaneOpen(pub bool);

impl gpui::Global for InspectorPaneOpen {}

impl InspectorPaneOpen {
    /// `true` while the workspace's GPUI inspector pane is mounted
    /// (`Window.inspector.is_some()`).  Used by
    /// [`crate::macos::rebuild_menus_with_workspace`] to drive the
    /// View-menu label.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    /// Installing `render_tolaria_inspector` via the public
    /// `set_inspector_renderer` boxing path must accept our function
    /// signature — protects against renderer parameters drifting away
    /// from `gpui::InspectorRenderer`.
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

    /// `InspectorPaneOpen::is_open` is the source of truth for the
    /// menu label — default is `false` so labels render "Show …"
    /// before the user ever presses Cmd+Alt+I.
    #[test]
    fn inspector_pane_open_default_is_false() {
        assert!(!InspectorPaneOpen::default().is_open());
    }

    /// A minimal root view used solely as the host for the
    /// `toggle_inspector` regression test.
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
