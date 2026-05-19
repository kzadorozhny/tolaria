#![forbid(unsafe_code)]
//! Left-dock sidebar panel chrome view for Tolaria (ADR-0115 Phase 2d).
//!
//! `SidebarPanel` implements [`workspace::panel::Panel`] for the Left Dock.
//! It renders four clusters mirroring the reference screenshots
//! (`tolaria-demo-vault-v2-{light,dark}.png`):
//!
//! ```text
//! ┌────────────────────────────┐
//! │  Inbox                     │  ← top fixed group
//! │  All Notes              30 │
//! │  Archive                   │
//! │ VIEWS                      │
//! │  Recent                  5 │
//! │ TYPES                      │
//! │  ● Areas                 1 │  ← colored leading glyph per type
//! │  ● Events                1 │
//! │ FOLDERS                    │
//! │   attachments              │
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

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use gpui::{
    div, px, rgb, App, Context, Hsla, IntoElement, ParentElement, Pixels, Render, SharedString,
    Styled, Window,
};
use gpui_component::{ActiveTheme, StyledExt as _};
use mock_fixtures::{MockVault, NoteKind};
use std::path::{Path, PathBuf};
use vault::Vault;
use workspace::panel::{DockPosition, Panel};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// One typed group of notes shown in the `TYPES` section.
///
/// `label` is the display name (e.g. `Areas`, `Events`); `color`
/// matches the type's accent in the reference screenshots; `count`
/// is the number of notes belonging to the type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeEntry {
    /// Display label.
    pub label: SharedString,
    /// Leading-glyph fill colour.
    pub color: Hsla,
    /// Number of notes in this type.
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
    /// Total number of notes — drives the "All Notes" row's count chip
    /// and the fallback for empty vault state.
    total_count: usize,
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
            total_count: 0,
            types: Vec::new(),
            views: Vec::new(),
            folders: Vec::new(),
            position: DockPosition::Left,
        }
    }

    /// Build from `vault::Vault` if installed, falling back to
    /// [`MockVault`] (TOLARIA_MOCK=1 mode), then to an empty panel
    /// (no vault selected yet).  Phase 5-MVP precedence: real > mock > empty.
    #[must_use]
    pub fn from_or_empty(cx: &mut App) -> Self {
        if cx.try_global::<Vault>().is_some() {
            Self::from_vault(cx)
        } else if cx.try_global::<MockVault>().is_some() {
            Self::from_mock(cx)
        } else {
            Self::new()
        }
    }

    /// Build from the real `vault::Vault` global.
    ///
    /// # Panics
    ///
    /// Panics if the [`Vault`] global is not installed on `cx`.
    #[must_use]
    pub fn from_vault(cx: &mut App) -> Self {
        let executor = cx.foreground_executor().clone();
        let vault = cx.global::<Vault>();
        let note_ids = executor.block_on(vault.notes());
        let mut samples = Vec::with_capacity(note_ids.len());
        for id in note_ids {
            if let Some(note) = executor.block_on(vault.note(id)) {
                samples.push((note.kind, note.path));
            }
        }
        Self::build_from_samples(samples)
    }

    /// Build a sidebar panel populated from the [`MockVault`] global.
    ///
    /// # Panics
    ///
    /// Panics if the [`MockVault`] global is not installed on `cx`.
    #[must_use]
    pub fn from_mock(cx: &mut App) -> Self {
        let executor = cx.foreground_executor().clone();
        let vault = cx.global::<MockVault>();
        let note_ids = executor.block_on(vault.notes());
        let mut samples = Vec::with_capacity(note_ids.len());
        for id in note_ids {
            if let Some(note) = executor.block_on(vault.note(id)) {
                samples.push((note.kind, note.path));
            }
        }
        Self::build_from_samples(samples)
    }

    /// Common post-processing for both [`from_mock`] and [`from_vault`].
    ///
    /// Types are derived from the leading filename segment
    /// (`measure-close-rate.md` → `Measures`).  Unknown prefixes — and
    /// non-Markdown kinds — collapse into a single `Notes` group, so
    /// the panel always has at least one TypeEntry when the vault is
    /// non-empty.  Each type carries a fixed accent colour from
    /// [`type_color`] that mirrors the reference screenshots' palette.
    fn build_from_samples(samples: Vec<(NoteKind, PathBuf)>) -> Self {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut folder_paths: BTreeSet<SharedString> = BTreeSet::new();
        let total_count = samples.len();

        for (kind, path) in samples {
            let label = match kind {
                NoteKind::Markdown => type_label_for(&path),
                NoteKind::Asset => "Assets",
                NoteKind::Folder => "Folders",
            };
            *counts.entry(label).or_insert(0) += 1;

            if let Some(parent) = path.parent() {
                let s = parent.to_string_lossy();
                if !s.is_empty() {
                    folder_paths.insert(SharedString::from(s.into_owned()));
                }
            }
        }

        let mut types: Vec<TypeEntry> = counts
            .into_iter()
            .map(|(label, count)| TypeEntry {
                label: SharedString::new_static(label),
                color: type_color(label),
                count,
            })
            .collect();
        // Stable, alphabetical order so toggling between vaults
        // doesn't re-order the rows from one render to the next.
        types.sort_by(|a, b| a.label.cmp(&b.label));

        // Three synthesised saved views — Phase 9 will replace with real
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
            total_count,
            types,
            views,
            folders,
            position: DockPosition::Left,
        }
    }

    #[cfg(test)]
    fn types(&self) -> &[TypeEntry] {
        &self.types
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
// Helpers — type extraction + colour palette
// ---------------------------------------------------------------------------

/// Map a `demo-vault-v2`-style filename prefix to its display type.
///
/// The Tauri-era app stored a `type` field in each note's frontmatter;
/// for the GPUI-first chrome we approximate it from the filename
/// stem until the Phase 9 vault rewrite surfaces the real frontmatter
/// type.  Unknown prefixes (and untyped notes) collapse into `Notes`.
fn type_label_for(path: &Path) -> &'static str {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let prefix = stem
        .split_once('-')
        .map(|(p, _)| p)
        .unwrap_or(stem)
        .to_ascii_lowercase();
    match prefix.as_str() {
        "area" => "Areas",
        "event" => "Events",
        "measure" => "Measures",
        "person" => "People",
        "procedure" => "Procedures",
        "responsibility" => "Responsibilities",
        "topic" => "Topics",
        _ => "Notes",
    }
}

/// Accent colour for a typed group's leading glyph.  Mirrors the
/// `--type-*` colour tokens in the Tauri-era app's stylesheet so the
/// native chrome stays visually consistent with the reference.
fn type_color(label: &str) -> Hsla {
    let rgb_u32: u32 = match label {
        "Areas" => 0x8B5CF6,            // violet
        "Events" => 0x14B8A6,           // teal
        "Measures" => 0x3B82F6,         // blue
        "People" => 0xEF4444,           // red
        "Procedures" => 0x22C55E,       // green
        "Responsibilities" => 0xF59E0B, // amber
        "Topics" => 0xEC4899,           // pink
        "Assets" => 0x64748B,           // slate
        "Folders" => 0x6B7280,          // gray
        _ => 0x6B7280,                  // gray fallback for `Notes`
    };
    rgb(rgb_u32).into()
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for SidebarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // gpui-component ships a dedicated `sidebar_*` palette that
        // mirrors the dock-chrome semantics in the reference: distinct
        // bg, accent fill for the active row, and a muted foreground
        // for section headers.
        let bg = theme.sidebar;
        let border = theme.sidebar_border;
        let fg = theme.sidebar_foreground;
        let muted = theme.muted_foreground;
        let accent_bg = theme.sidebar_accent;
        let accent_fg = theme.sidebar_accent_foreground;
        let neutral_glyph = muted;

        let fixed_top = vec![
            // (label, count, selected)
            ("Inbox", 0usize, false),
            ("All Notes", self.total_count, true),
            ("Archive", 0usize, false),
        ];

        let fixed_rows = fixed_top.into_iter().map(move |(label, count, selected)| {
            sidebar_row(
                label,
                neutral_glyph,
                count,
                selected,
                accent_bg,
                accent_fg,
                muted,
            )
        });

        let type_rows = self.types.iter().map(|entry| {
            sidebar_row(
                entry.label.as_ref(),
                entry.color,
                entry.count,
                false,
                accent_bg,
                accent_fg,
                muted,
            )
        });

        let view_rows = self.views.iter().map(|view| {
            sidebar_row(
                view.name.as_ref(),
                neutral_glyph,
                view.count,
                false,
                accent_bg,
                accent_fg,
                muted,
            )
        });

        // Truncate folder paths to their final segment for display so
        // the narrow column doesn't ellipsise everything to the prefix.
        // `Path::file_name` strips a trailing separator and handles
        // Windows separators in case the codebase ever leaves
        // macOS-only land.
        let folder_rows = self.folders.iter().map(|folder| {
            let leaf = Path::new(folder.path.as_ref())
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(folder.path.as_ref());
            sidebar_folder_row(leaf, folder.depth, muted)
        });

        div()
            .flex()
            .flex_col()
            .h_full()
            .w_full()
            .bg(bg)
            .text_color(fg)
            .border_r_1()
            .border_color(border)
            .py(px(8.0))
            // Top fixed group: Inbox / All Notes / Archive.
            .children(fixed_rows)
            // VIEWS section.
            .child(section_header("VIEWS", muted))
            .children(view_rows)
            // TYPES section.
            .child(section_header("TYPES", muted))
            .children(type_rows)
            // FOLDERS section.
            .child(section_header("FOLDERS", muted))
            .children(folder_rows)
    }
}

/// One small-caps section header rendered between two row groups.
fn section_header(label: &'static str, muted: Hsla) -> gpui::AnyElement {
    div()
        .px(px(12.0))
        .pt(px(14.0))
        .pb(px(4.0))
        .text_color(muted)
        .text_xs()
        .font_semibold()
        .child(SharedString::new_static(label))
        .into_any_element()
}

/// One regular sidebar row: 8-px coloured leading glyph, label, count
/// chip.  `selected` paints the row full-width with the theme accent
/// background; `accent_fg` overrides the label colour so it stays
/// legible against the accent fill.
fn sidebar_row(
    label: &str,
    glyph: Hsla,
    count: usize,
    selected: bool,
    accent_bg: Hsla,
    accent_fg: Hsla,
    muted: Hsla,
) -> gpui::AnyElement {
    let glyph_dot = div()
        .w(px(8.0))
        .h(px(8.0))
        .flex_shrink_0()
        .rounded_full()
        .bg(glyph);
    let count_text = if count > 0 {
        SharedString::from(format!("{count}"))
    } else {
        SharedString::default()
    };
    let row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .w_full()
        .px(px(12.0))
        .py(px(5.0))
        .text_sm()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .flex_1()
                .child(glyph_dot)
                .child(SharedString::from(label.to_string())),
        )
        .child(div().text_xs().text_color(muted).child(count_text));
    if selected {
        row.bg(accent_bg).text_color(accent_fg).into_any_element()
    } else {
        row.into_any_element()
    }
}

/// One folder row with depth-proportional left padding.  Count chip is
/// suppressed because the count of notes per folder is not yet
/// computed (Phase 9 vault rewrite).
fn sidebar_folder_row(label: &str, depth: u8, _muted: Hsla) -> gpui::AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .pl(px(12.0 + f32::from(depth) * 12.0))
        .pr(px(12.0))
        .py(px(5.0))
        .text_sm()
        .child(SharedString::from(label.to_string()))
        .into_any_element()
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

    /// `type_label_for` extracts the leading prefix and maps it to the
    /// display label used in the reference screenshots.
    #[test]
    fn type_label_extracts_known_prefixes() {
        for (path, expected) in [
            ("area-building.md", "Areas"),
            ("event-team-sync.md", "Events"),
            ("measure-close-rate.md", "Measures"),
            ("person-luca-rossi.md", "People"),
            ("procedure-onboarding.md", "Procedures"),
            ("responsibility-sponsorships.md", "Responsibilities"),
            ("topic-writing.md", "Topics"),
            ("24q4.md", "Notes"),                   // no prefix
            ("note-on-clear-prose.md", "Notes"),    // explicit `note-` prefix
            ("rtl-mixed-direction-qa.md", "Notes"), // unrelated prefix
        ] {
            assert_eq!(type_label_for(Path::new(path)), expected, "input={path}");
        }
    }

    /// Building from a synthetic sample list groups notes by filename
    /// prefix and emits one TypeEntry per distinct label.
    #[test]
    fn build_from_samples_groups_by_filename_prefix() {
        let samples = vec![
            (NoteKind::Markdown, PathBuf::from("area-x.md")),
            (NoteKind::Markdown, PathBuf::from("area-y.md")),
            (NoteKind::Markdown, PathBuf::from("event-launch.md")),
            (NoteKind::Markdown, PathBuf::from("untyped.md")),
        ];
        let panel = SidebarPanel::build_from_samples(samples);
        let pairs: Vec<(&str, usize)> = panel
            .types()
            .iter()
            .map(|e| (e.label.as_ref(), e.count))
            .collect();
        assert_eq!(
            pairs,
            vec![("Areas", 2), ("Events", 1), ("Notes", 1)],
            "types must group by prefix and sort alphabetically",
        );
    }
}
