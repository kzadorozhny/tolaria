//! `Panel` trait and `DockPosition` enum (ADR-0115 Phase 2a).
//!
//! Every chrome panel (Sidebar, Inspector, AI, etc.) must implement this trait
//! and be attached to a [`Dock`][crate::dock::Dock].
//!
//! Phase 2a ships the minimal set required by `Dock`: `Render + 'static` plus
//! the five metadata methods.  `Focusable` and `EventEmitter<PanelEvent>` are
//! deferred until the first sub-crate subscribes to those events.

use gpui::{Action, App, Context, Pixels, Render};

/// Which edge of the workspace a panel occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Right,
    Bottom,
}

/// Trait implemented by every dockable panel in the Tolaria workspace.
///
/// Phase 2a only requires `Render + 'static`.  `Focusable` and
/// `EventEmitter<PanelEvent>` will be added once a panel sub-crate first
/// subscribes to those events.
pub trait Panel: Render + 'static {
    /// Stable identifier used for layout persistence (e.g. `"SidebarPanel"`).
    fn persistent_name(&self) -> &str;

    /// Short key used for action dispatch lookup (e.g. `"sidebar"`).
    fn panel_key(&self) -> &str;

    /// The dock edge this panel currently occupies.
    ///
    /// `cx` is provided for implementations that derive the position from
    /// persisted app state or theme settings.
    fn position(&self, cx: &App) -> DockPosition;

    /// Move the panel to a different dock edge.
    fn set_position(&mut self, position: DockPosition, cx: &mut Context<Self>);

    /// Preferred initial size in pixels along the dock's major axis.
    ///
    /// `cx` is provided for implementations that read theme-dependent sizing.
    /// Return `px(0.0)` to fall back to the dock's [`DEFAULT_PANEL_SIZE`][crate::dock::DEFAULT_PANEL_SIZE].
    fn default_size(&self, cx: &App) -> Pixels;

    /// Optional icon identifier (e.g. a Phosphor icon name).
    ///
    /// Returns `None` by default.
    fn icon(&self) -> Option<&str> {
        None
    }

    /// The action that toggles this panel's visibility in the dock.
    fn toggle_action(&self) -> Box<dyn Action>;

    /// Whether the panel should be visible when the workspace first opens.
    ///
    /// `cx` is provided for implementations that derive the initial state from
    /// persisted settings.  Returns `false` by default.
    fn starts_open(&self, _cx: &App) -> bool {
        false
    }
}
