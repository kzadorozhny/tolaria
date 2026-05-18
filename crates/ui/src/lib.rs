#![deny(missing_docs)]
#![warn(clippy::all)]
//! Tolaria-specific UI compounds (ADR-0115 Phase 1 placeholder).
//!
//! In Phase 2 this crate will grow to contain:
//! - `RichTooltip` — tooltip with an embedded shortcut badge.
//! - `IconPicker` — icon chooser built over the Phosphor icon set.
//! - `ShortcutBadge` — small keyboard-shortcut label.
//! - `FocusRing` — consistent focus indicator overlay.
//! - A vendored minimal port of Zed's `Picker<Delegate>` for the command
//!   palette, quick-open, and wikilink combobox surfaces.
//!
//! For Phase 1 only `init` is exported; callers include it in the
//! registration sequence so the call site is already wired when Phase 2
//! content lands.

/// No-op initializer. Phase 2 will register any compound-level globals here.
pub fn init(_cx: &mut gpui::App) {}
