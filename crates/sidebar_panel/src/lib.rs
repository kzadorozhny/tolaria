#![forbid(unsafe_code)]
//! Left-dock sidebar panel chrome view for Tolaria (ADR-0115 Phase 2d).
//!
//! `SidebarPanel` implements [`workspace::panel::Panel`] for the Left Dock.
//! It renders three sections:
//!
//! ```text
//! ┌────────────────────────────┐
//! │ TYPES                      │
//! │  Markdown              30  │
//! │ VIEWS                      │
//! │  Recent                 5  │
//! │  Archived               3  │
//! │  Drafts                 2  │
//! │ FOLDERS                    │
//! └────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! // In mock / dev mode — globals must be installed first:
//! cx.set_global(MockVault::seeded());
//! let panel = cx.new(|_| SidebarPanel::from_mock(cx));
//! ```

use std::collections::BTreeSet;

use gpui::{
    div, px, App, Context, IntoElement, ParentElement, Pixels, Render, SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme, StyledExt as _,
};
use mock_fixtures::{MockVault, NoteKind};
use workspace::panel::{DockPosition, Panel};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A note-kind-grouped entry in the Types section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEntry {
    pub kind: NoteKind,
    pub count: usize,
}

/// A saved-view entry (synthesised demo data for Phase 2d).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedView {
    pub name: SharedString,
    pub count: usize,
}

/// A folder path entry derived from note file paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub path: SharedString,
    /// Nesting depth, derived from the number of `/` separators in the path.
    pub depth: u8,
}

// ---------------------------------------------------------------------------
// SidebarPanel view
// ---------------------------------------------------------------------------

/// Activation priority used when wiring this panel into the workspace dock.
///
/// A lower value means higher priority (appears first in the dock bar).
pub const ACTIVATION_PRIORITY: u32 = 10;

/// Left-dock sidebar panel view for `TolariaWorkspace`.
///
/// Constructed via [`SidebarPanel::new`] for a blank panel or
/// [`SidebarPanel::from_mock`] to populate from the installed mock globals.
///
/// # Panics
///
/// [`SidebarPanel::from_mock`] panics if the [`MockVault`] global has not been
/// installed on `cx` prior to the call.
pub struct SidebarPanel {
    types: Vec<TypeEntry>,
    views: Vec<SavedView>,
    folders: Vec<FolderEntry>,
    position: DockPosition,
}

impl SidebarPanel {
    /// Create an empty sidebar panel with no entries.
    #[must_use]
    pub fn new() -> Self {
        Self {
            types: Vec::new(),
            views: Vec::new(),
            folders: Vec::new(),
            position: DockPosition::Left,
        }
    }

    /// Build from [`MockVault`] global if it is installed; otherwise return an
    /// empty panel. Used by `TolariaWorkspace` so the sidebar populates under
    /// `TOLARIA_MOCK=1` and degrades gracefully in normal launches before
    /// Phase 3 services land.
    #[must_use]
    pub fn from_or_empty(cx: &mut App) -> Self {
        if cx.try_global::<MockVault>().is_some() {
            Self::from_mock(cx)
        } else {
            Self::new()
        }
    }

    /// Build a sidebar panel populated from the [`MockVault`] global installed
    /// on `cx`.
    ///
    /// - **TypeEntry** — groups notes by [`NoteKind`]; only non-zero kinds
    ///   appear, sorted alphabetically by label.
    /// - **SavedView** — three synthesised demo views: "Recent", "Archived",
    ///   "Drafts".
    /// - **FolderEntry** — unique parent directories derived from note paths,
    ///   sorted lexicographically; empty for flat (no-subdirectory) vaults.
    ///
    /// # Panics
    ///
    /// Panics if the [`MockVault`] global is not installed on `cx`.
    #[must_use]
    pub fn from_mock(cx: &mut App) -> Self {
        // `Task::ready` resolves immediately; `block_on` never blocks the
        // foreground thread in practice.  This mirrors the pattern used in
        // `status_bar::from_mock`.
        let executor = cx.foreground_executor().clone();
        // Hoist the vault reference so we resolve the global once, not N+1 times.
        let vault = cx.global::<MockVault>();
        let note_ids = executor.block_on(vault.notes());

        let mut markdown_count: usize = 0;
        let mut asset_count: usize = 0;
        let mut folder_count: usize = 0;
        // BTreeSet gives stable lexicographic ordering without a sort step.
        let mut folder_paths: BTreeSet<SharedString> = BTreeSet::new();

        for id in note_ids {
            let Some(note) = executor.block_on(vault.note(id)) else {
                continue;
            };
            match note.kind {
                NoteKind::Markdown => markdown_count += 1,
                NoteKind::Asset => asset_count += 1,
                NoteKind::Folder => folder_count += 1,
            }
            // Derive folder hierarchy from parent path components.
            if let Some(parent) = note.path.parent() {
                let s = parent.to_string_lossy();
                if !s.is_empty() {
                    folder_paths.insert(SharedString::from(s.into_owned()));
                }
            }
        }

        // Only include kinds with at least one note; sort alphabetically by label.
        let mut types: Vec<TypeEntry> = Vec::with_capacity(3);
        if markdown_count > 0 {
            types.push(TypeEntry {
                kind: NoteKind::Markdown,
                count: markdown_count,
            });
        }
        if asset_count > 0 {
            types.push(TypeEntry {
                kind: NoteKind::Asset,
                count: asset_count,
            });
        }
        if folder_count > 0 {
            types.push(TypeEntry {
                kind: NoteKind::Folder,
                count: folder_count,
            });
        }
        types.sort_by_key(|e| kind_label(e.kind));

        // Three synthesised saved views — Phase 3 will replace with real
        // persisted view definitions.
        let views = vec![
            SavedView {
                name: "Recent".into(),
                count: 5,
            },
            SavedView {
                name: "Archived".into(),
                count: 3,
            },
            SavedView {
                name: "Drafts".into(),
                count: 2,
            },
        ];

        let folders: Vec<FolderEntry> = folder_paths
            .into_iter()
            .map(|path| {
                // Use bytes() — faster than chars() for ASCII `/`, and saturate
                // instead of wrapping on the (implausible) ≥256-deep path.
                let depth =
                    u8::try_from(path.bytes().filter(|&b| b == b'/').count()).unwrap_or(u8::MAX);
                FolderEntry { path, depth }
            })
            .collect();

        Self {
            types,
            views,
            folders,
            position: DockPosition::Left,
        }
    }
}

impl Default for SidebarPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Panel impl
// ---------------------------------------------------------------------------

impl Panel for SidebarPanel {
    fn persistent_name(&self) -> &str {
        "SidebarPanel"
    }

    fn panel_key(&self) -> &str {
        "sidebar"
    }

    fn position(&self, _cx: &App) -> DockPosition {
        self.position
    }

    fn set_position(&mut self, position: DockPosition, cx: &mut Context<Self>) {
        self.position = position;
        cx.notify();
    }

    fn default_size(&self, _cx: &App) -> Pixels {
        px(240.0)
    }

    fn icon(&self) -> Option<&str> {
        Some("sidebar")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(actions::ToggleSidebar)
    }

    fn starts_open(&self, _cx: &App) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[must_use]
fn kind_label(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::Markdown => "Markdown",
        NoteKind::Asset => "Asset",
        NoteKind::Folder => "Folder",
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for SidebarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header_color = cx.theme().muted_foreground;

        // Types section rows.
        let type_rows: Vec<gpui::AnyElement> = self
            .types
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let label =
                    SharedString::from(format!("{} {}", kind_label(entry.kind), entry.count));
                Button::new(("sidebar-type", i))
                    .label(label)
                    .ghost()
                    .into_any_element()
            })
            .collect();

        // Views section rows.
        let view_rows: Vec<gpui::AnyElement> = self
            .views
            .iter()
            .enumerate()
            .map(|(i, view)| {
                let label = SharedString::from(format!("{} {}", view.name, view.count));
                Button::new(("sidebar-view", i))
                    .label(label)
                    .ghost()
                    .into_any_element()
            })
            .collect();

        // Folders section rows with depth-proportional left indent.
        let folder_rows: Vec<gpui::AnyElement> = self
            .folders
            .iter()
            .enumerate()
            .map(|(i, folder)| {
                let indent = px(f32::from(folder.depth) * 12.0);
                div()
                    .pl(indent)
                    .child(
                        Button::new(("sidebar-folder", i))
                            .label(folder.path.clone())
                            .ghost(),
                    )
                    .into_any_element()
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .h_full()
            .w_full()
            // ── Types ──────────────────────────────────────────────────────
            .child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_xs()
                    .font_semibold()
                    .text_color(header_color)
                    .child("Types"),
            )
            .children(type_rows)
            // ── Views ──────────────────────────────────────────────────────
            .child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_xs()
                    .font_semibold()
                    .text_color(header_color)
                    .child("Views"),
            )
            .children(view_rows)
            // ── Folders ────────────────────────────────────────────────────
            .child(
                div()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_xs()
                    .font_semibold()
                    .text_color(header_color)
                    .child("Folders"),
            )
            .children(folder_rows)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use mock_fixtures::MockVault;

    /// Install the `gpui_component::Theme` global required by any view that
    /// reads it during render (mirrors status_bar and breadcrumb_bar pattern).
    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// An empty sidebar panel must render without panicking.
    #[gpui::test]
    fn empty_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| SidebarPanel::new());
        cx.run_until_parked();
    }

    /// `from_mock` must produce at least one TypeEntry when a vault is
    /// installed (MockVault seeds 30 Markdown notes).
    #[gpui::test]
    fn from_mock_groups_by_kind(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            let panel = SidebarPanel::from_mock(cx);
            assert!(
                !panel.types.is_empty(),
                "expected at least 1 TypeEntry, got none",
            );
        });
    }

    /// The panel must report `DockPosition::Left`.
    #[gpui::test]
    fn panel_position_is_left(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let panel = SidebarPanel::new();
            assert_eq!(
                panel.position(cx),
                DockPosition::Left,
                "SidebarPanel must occupy the Left dock",
            );
        });
    }

    /// `from_or_empty` must return an empty panel when no globals are set.
    #[gpui::test]
    fn from_or_empty_falls_back_when_no_globals(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let panel = SidebarPanel::from_or_empty(cx);
            assert!(
                panel.types.is_empty(),
                "expected empty types when no MockVault global is present",
            );
            assert!(
                panel.views.is_empty(),
                "expected empty views when no MockVault global is present",
            );
        });
    }

    /// `from_mock` must synthesise exactly 3 saved views.
    #[gpui::test]
    fn synthesised_views_count_is_three(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            let panel = SidebarPanel::from_mock(cx);
            assert_eq!(
                panel.views.len(),
                3,
                "expected exactly 3 synthesised saved views, got {}",
                panel.views.len(),
            );
        });
    }
}
