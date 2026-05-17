//! Two-pane sidebar layout for the ADR-0115 Phase 0 spike (task #3).
//!
//! Wraps gpui-component's `h_resizable` primitive so the draggable splitter
//! and clamping are handled upstream. The view itself only owns
//! (i) the `ResizableState` entity backing the splitter so we can read panel
//!     sizes from event callbacks, and
//! (ii) the previous viewport size so window-resize logs deduplicate against
//!      pure window-move events (which also fire `observe_window_bounds`).
//!
//! Two log streams feed task #5's validation script — both on the
//! `embed_poc::frame` log target so a single `RUST_LOG=embed_poc::frame=info`
//! captures only frame-sync evidence and nothing else:
//!   - `frame_event kind=sidebar_resize ...` on every committed splitter drag
//!     (via `ResizablePanelGroup::on_resize`, which only fires at drag end).
//!   - `frame_event kind=window_resize ...` on every OS window content-area
//!     size change (via `cx.observe_window_bounds`, deduped against window
//!     moves).

use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Pixels, Render, Size, Styled,
    Window, div, px, rgb,
};
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};

const SIDEBAR_DEFAULT: f32 = 240.0;
const SIDEBAR_MIN: f32 = 160.0;
const SIDEBAR_MAX: f32 = 480.0;
const FRAME_TARGET: &str = "embed_poc::frame";

pub struct RootView {
    resizable_state: Entity<ResizableState>,
    last_viewport: Size<Pixels>,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let resizable_state = cx.new(|_| ResizableState::default());

        cx.observe_window_bounds(window, |this, window, cx| {
            this.log_window_resize(window, cx);
        })
        .detach();

        Self {
            resizable_state,
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
        div().size_full().bg(rgb(0x1e1f24)).child(
            h_resizable("sidebar-layout")
                .with_state(&state)
                .child(
                    resizable_panel()
                        .size(px(SIDEBAR_DEFAULT))
                        .size_range(px(SIDEBAR_MIN)..px(SIDEBAR_MAX))
                        .flex_none()
                        .child(sidebar_panel()),
                )
                .child(resizable_panel().child(content_panel()))
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

fn sidebar_panel() -> impl IntoElement {
    div()
        .size_full()
        .bg(rgb(0x282a36))
        .text_color(rgb(0xe6e6e6))
        .p_3()
        .text_sm()
        .child("Sidebar")
}

fn content_panel() -> impl IntoElement {
    div()
        .size_full()
        .bg(rgb(0x12141a))
        .text_color(rgb(0x9aa0a6))
        .p_3()
        .text_sm()
        .child("editor goes here")
}

fn same_size(a: Size<Pixels>, b: Size<Pixels>) -> bool {
    const EPSILON: f32 = 0.5;
    (f32::from(a.width) - f32::from(b.width)).abs() < EPSILON
        && (f32::from(a.height) - f32::from(b.height)).abs() < EPSILON
}
