//! `Dock` view — hosts one active panel per workspace edge (ADR-0115 Phase 2a).
//!
//! A `Dock` is constructed for each of the three positions (`Left`, `Right`,
//! `Bottom`) and held by `TolariaWorkspace`.  Until `set_panel` is called the
//! dock renders nothing.  After a panel is attached, `toggle` controls
//! visibility.
//!
//! Phase 2a renders the panel content directly when open.  Drag-resize handles
//! and the full object-safe `PanelHandle` wrapper are Phase 2b additions
//! (modelled on `zed/crates/workspace/src/dock.rs:98–300`).

use gpui::{
    div, px, AnyView, Context, Entity, IntoElement, ParentElement, Pixels, Render, Styled, Window,
};

use crate::panel::{DockPosition, Panel};

/// Default panel size used when a panel reports 0 or the dock has no panel.
///
/// Exposed so that chrome crates can reference the same baseline without
/// hard-coding 240.
pub const DEFAULT_PANEL_SIZE: Pixels = px(240.0);

/// Whether a dock has a panel attached and whether it is currently visible.
///
/// Encoding open/closed/empty as a typed enum makes the invalid state
/// `is_open == true && panel == None` unrepresentable.
enum DockState {
    /// No panel has been attached yet.
    Empty,
    /// A panel is attached but the dock is currently hidden.
    Closed(AnyView),
    /// A panel is attached and the dock is currently visible.
    Open(AnyView),
}

/// A dockable container that hosts one [`Panel`] at a fixed workspace edge.
///
/// Create one per `DockPosition` inside `TolariaWorkspace::empty`, then call
/// [`set_panel`][Dock::set_panel] from Phase 2b chrome crates to attach a
/// concrete panel.
pub struct Dock {
    position: DockPosition,
    /// Cached `default_size` from the attached panel.
    panel_size: Pixels,
    state: DockState,
}

impl Dock {
    /// Create an empty, closed dock at the given edge.
    ///
    /// The dock starts with no panel; it opens automatically when `set_panel`
    /// is called with a panel whose `starts_open` returns `true`.
    #[must_use]
    pub fn new(position: DockPosition) -> Self {
        Self {
            position,
            panel_size: DEFAULT_PANEL_SIZE,
            state: DockState::Empty,
        }
    }

    /// Attach `panel` to this dock, reading its `default_size` and
    /// `starts_open` to initialise the dock state.
    pub fn set_panel<P: Panel>(&mut self, panel: Entity<P>, cx: &mut Context<Self>) {
        // Read metadata before consuming `panel` via `into()`.
        let size = panel.read(cx).default_size(cx);
        let open = panel.read(cx).starts_open(cx);
        self.panel_size = if size > px(0.0) {
            size
        } else {
            DEFAULT_PANEL_SIZE
        };
        let view: AnyView = panel.into();
        self.state = if open {
            DockState::Open(view)
        } else {
            DockState::Closed(view)
        };
        cx.notify();
    }

    /// Toggle the dock between open and closed.
    ///
    /// No-op if no panel has been attached.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.state = match std::mem::replace(&mut self.state, DockState::Empty) {
            DockState::Empty => DockState::Empty,
            DockState::Closed(v) => DockState::Open(v),
            DockState::Open(v) => DockState::Closed(v),
        };
        if !matches!(self.state, DockState::Empty) {
            cx.notify();
        }
    }

    /// Whether the dock is currently visible.
    pub fn is_open(&self) -> bool {
        matches!(self.state, DockState::Open(_))
    }

    /// The dock's edge position.
    pub fn position(&self) -> DockPosition {
        self.position
    }

    /// The active panel as an `AnyView`, or `None` if the dock is closed or
    /// has no attached panel.
    pub fn active_panel(&self) -> Option<&AnyView> {
        match &self.state {
            DockState::Open(v) => Some(v),
            _ => None,
        }
    }
}

impl Render for Dock {
    /// Pure pass-through render; observes nothing directly.
    ///
    /// Left/Right docks fill width and height of whatever container
    /// wraps them — for the workspace that's a
    /// `gpui_component::resizable::ResizablePanel`, which owns the
    /// width and drives drag-to-resize.  Clamping to
    /// `self.panel_size` here would override the resizable panel and
    /// freeze the dock at its initial width; instead we let the
    /// parent decide.  Bottom docks still own their height because
    /// the workspace mounts the bottom dock outside the `h_resizable`
    /// row (see `crates/workspace/src/workspace.rs:210`).
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            DockState::Open(panel) => match self.position {
                DockPosition::Left | DockPosition::Right => div().size_full().child(panel.clone()),
                DockPosition::Bottom => div().w_full().h(self.panel_size).child(panel.clone()),
            },
            _ => div(),
        }
    }
}
