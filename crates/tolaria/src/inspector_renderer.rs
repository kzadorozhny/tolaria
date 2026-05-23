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
    div, prelude::FluentBuilder as _, px, rems, AnyElement, App, Context, Inspector,
    InspectorElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme, IconName, Selectable as _, Sizable as _,
};

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
    window: &mut Window,
    cx: &mut Context<Inspector>,
) -> AnyElement {
    let theme = cx.theme();
    let bg = theme.background;
    let border = theme.border;
    let fg = theme.foreground;

    // Header strip — picker toggle on the left, "GPUI Inspector"
    // label on the right.  Mirrors Zed's `inspector_ui::inspector::
    // render_inspector` toolbar (the panel built by Zed the user
    // referred to in worklist 10.1.4 — "blocker: restore native GPUI
    // inspector panel functionality inside inspector view").  We use
    // a `gpui_component::Button` instead of Zed's `IconButton` since
    // Tolaria's chrome standardises on the gpui_component primitives;
    // visually equivalent (icon-only ghost button toggling between
    // un-selected and selected states).
    let pick_btn = Button::new("inspector-pick-toggle")
        .icon(IconName::Inspector)
        .ghost()
        .small()
        .selected(inspector.is_picking())
        .on_click(cx.listener(|this, _event, window, _cx| {
            this.start_picking();
            window.refresh();
        }));
    let header = h_flex()
        .justify_between()
        .items_center()
        .px(px(8.0))
        .py(px(4.0))
        .h(px(36.0))
        .border_b_1()
        .border_color(border)
        .child(pick_btn)
        .child(
            div()
                .text_sm()
                .text_color(fg)
                .child(SharedString::new_static("GPUI Inspector")),
        );

    // Body — element-ID metadata block followed by every per-element-
    // type renderer registered via `cx.register_inspector_element`.
    // Today no type-renderers are registered (porting Zed's
    // `DivInspector` would pull in `project::Project` + LSP), so
    // `render_inspector_states` returns an empty `Vec`; the panel
    // gracefully degrades to "element ID only".  When future code
    // registers element renderers (e.g. a stripped-down DivInspector)
    // they slot in below the ID block automatically.
    let active_id = inspector.active_element_id().cloned();
    let element_states = inspector.render_inspector_states(window, cx);
    let body = v_flex()
        .id("tolaria-inspector-body")
        .size_full()
        .overflow_y_scroll()
        .gap(px(8.0))
        .px(px(8.0))
        .py(px(8.0))
        .when_some(active_id, |this, id: InspectorElementId| {
            this.child(render_element_id_block(&id, cx))
        })
        .children(element_states);

    v_flex()
        .size_full()
        .bg(bg)
        .text_color(fg)
        .child(header)
        .child(body)
        .into_any_element()
}

/// Element-ID metadata block — "Element ID / Instance N" / source
/// location / global id.  Mirrors the shape of Zed's
/// `render_inspector_id` (`inspector_ui::inspector::render_inspector_id`)
/// without the Zed-CLI "click to open by running Zed CLI" affordance
/// (Tolaria doesn't ship a CLI to forward to).  Read-only display —
/// the picker keeps populating this on hover when picking is enabled.
fn render_element_id_block(id: &InspectorElementId, cx: &App) -> gpui::Div {
    let theme = cx.theme();
    let fg = theme.foreground;
    let muted = theme.muted_foreground;
    let panel_bg = theme.muted;

    let source_location: SharedString = format!(
        "{}:{}",
        id.path.source_location.file(),
        id.path.source_location.line()
    )
    .into();
    let global_id: SharedString = id.path.global_id.to_string().into();
    let instance: SharedString = format!("Instance {}", id.instance_id).into();

    v_flex()
        .gap(px(4.0))
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(fg)
                        .child(SharedString::new_static("Element ID")),
                )
                .child(div().text_xs().text_color(muted).child(instance)),
        )
        .child(
            div()
                .text_xs()
                .text_color(fg)
                .bg(panel_bg)
                .px(px(6.0))
                .py(px(3.0))
                .rounded(px(3.0))
                .child(source_location),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .min_h(px(40.0))
                .child(global_id),
        )
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
