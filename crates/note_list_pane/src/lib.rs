#![forbid(unsafe_code)]
//! Note-list pane view for Tolaria (ADR-0115 Phase 2d).
//!
//! `NoteListPane` is a GPUI view designed to live as the content of a
//! [`workspace::Pane`].  It renders a filterable, selectable list of notes
//! drawn from [`mock_fixtures::MockVault`].
//!
//! # Layout
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │ Filter notes…                            │  ← filter strip
//! ├──────────────────────────────────────────┤
//! │  ☐  Writing              Note   May 17   │  ┐
//! │  ☐  Product              Note   May 17   │  │ scrollable list
//! │     …                                    │  ┘
//! ├──────────────────────────────────────────┤
//! │  3 selected   [Delete]  [Archive]        │  ← bulk bar (≥1 selected)
//! └──────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! cx.set_global(MockVault::seeded());
//! let pane = cx.new(|_| NoteListPane::from_or_empty(cx));
//! ```

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use gpui::EventEmitter;
use gpui::{
    div, px, AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme,
};
use mock_fixtures::{MockVault, NoteId, NoteKind};
use vault::Vault;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when the user activates a note row (single-click on the title
/// area).  `TolariaWorkspace` subscribes to this event and routes it
/// through [`TolariaWorkspace::open_note`].
#[derive(Debug, Clone, Copy)]
pub struct OpenNoteEvent {
    /// Identifier of the note the user wants to open.
    pub id: NoteId,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Pre-rendered display data for one row in the note list.
///
/// Projected from [`NoteEntry`] during `render()` so that the element-tree
/// closures can be `move` without borrowing `self`.
#[derive(Debug)]
struct RowData {
    id: NoteId,
    title: SharedString,
    kind_label: &'static str,
    date_str: SharedString,
    checked: bool,
}

/// Snapshot of one note's list-view metadata.
#[derive(Debug, Clone)]
pub struct NoteEntry {
    /// Stable identifier for the underlying note.
    pub id: NoteId,
    /// Display title.
    pub title: SharedString,
    /// File-level note kind.
    pub kind: NoteKind,
    /// Last-modified timestamp (UTC).
    pub modified: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// NoteListPane view
// ---------------------------------------------------------------------------

/// Filterable, selectable note-list view for the workspace centre pane.
///
/// # Constructors
///
/// | Constructor | When to use |
/// |---|---|
/// | [`NoteListPane::new`] | Empty list; use in tests or before vault loads. |
/// | [`NoteListPane::from_mock`] | Pre-populate from [`MockVault`] global. |
/// | [`NoteListPane::from_or_empty`] | Degrade gracefully when mock not installed. |
pub struct NoteListPane {
    entries: Vec<NoteEntry>,
    filter: SharedString,
    selected: HashSet<NoteId>,
}

impl NoteListPane {
    /// Create an empty pane with no entries and no filter.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            filter: SharedString::default(),
            selected: HashSet::new(),
        }
    }

    /// Build a pane pre-populated from the [`MockVault`] global.
    ///
    /// All vault calls return `Task::ready(…)` so `block_on` resolves
    /// immediately without blocking the foreground thread.
    ///
    /// # Panics
    ///
    /// Panics if the [`MockVault`] global is not installed on `cx`.
    pub fn from_mock(cx: &mut App) -> Self {
        let executor = cx.foreground_executor().clone();
        let vault = cx.global::<MockVault>();
        let ids = executor.block_on(vault.notes());
        let mut entries = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(note) = executor.block_on(vault.note(id)) {
                entries.push(NoteEntry {
                    id: note.id,
                    title: note.title.clone(),
                    kind: note.kind,
                    modified: note.modified,
                });
            }
        }
        Self {
            entries,
            filter: SharedString::default(),
            selected: HashSet::new(),
        }
    }

    /// Build a pane pre-populated from the real `vault::Vault` global.
    ///
    /// # Panics
    ///
    /// Panics if the [`Vault`] global is not installed on `cx`.
    pub fn from_vault(cx: &mut App) -> Self {
        let executor = cx.foreground_executor().clone();
        let vault = cx.global::<Vault>();
        let ids = executor.block_on(vault.notes());
        let mut entries = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(note) = executor.block_on(vault.note(id)) {
                entries.push(NoteEntry {
                    id: note.id,
                    title: note.title.clone(),
                    kind: note.kind,
                    modified: note.modified,
                });
            }
        }
        Self {
            entries,
            filter: SharedString::default(),
            selected: HashSet::new(),
        }
    }

    /// Build from `vault::Vault` if installed, else [`MockVault`], else
    /// an empty pane.  Phase 5-MVP precedence: real > mock > empty.
    pub fn from_or_empty(cx: &mut App) -> Self {
        if cx.try_global::<Vault>().is_some() {
            Self::from_vault(cx)
        } else if cx.try_global::<MockVault>().is_some() {
            Self::from_mock(cx)
        } else {
            Self::new()
        }
    }

    /// Update the substring filter and notify observers.
    ///
    /// An empty `query` shows all entries.  Matching is case-insensitive
    /// substring search on [`NoteEntry::title`].
    pub fn set_filter(&mut self, query: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.filter = query.into();
        cx.notify();
    }

    /// Toggle selection of `id`: adds if absent, removes if present.
    pub fn toggle_selection(&mut self, id: NoteId, cx: &mut Context<Self>) {
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
        cx.notify();
    }

    /// Clear all selected entries.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected.clear();
        cx.notify();
    }

    /// Number of currently selected entries.
    pub fn selection_count(&self) -> usize {
        self.selected.len()
    }

    /// Entries that pass the current filter, in original insertion order.
    ///
    /// Returns all entries when the filter is empty.  O(n) per call; called
    /// once per render.
    pub fn visible_entries(&self) -> Vec<&NoteEntry> {
        let q = (!self.filter.is_empty()).then(|| self.filter.to_lowercase());
        self.entries
            .iter()
            .filter(|e| {
                q.as_deref()
                    .map_or(true, |q| e.title.to_lowercase().contains(q))
            })
            .collect()
    }
}

impl Default for NoteListPane {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter<OpenNoteEvent> for NoteListPane {}

impl NoteListPane {
    /// Emit an [`OpenNoteEvent`] for `id`.  Called by the row click
    /// handler; exposed publicly so test harnesses can drive the event
    /// without simulating a click.
    pub fn open(&self, id: NoteId, cx: &mut Context<Self>) {
        cx.emit(OpenNoteEvent { id });
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for NoteListPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Pre-extract theme colours to avoid repeated borrows inside closures.
        let border_color = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;

        let entity = cx.entity();
        let n_selected = self.selection_count();
        let has_selection = n_selected > 0;
        let filter_text = self.filter.clone();

        // Pre-collect display data before building the element tree so that
        // the immutable borrow of `self.entries` (from visible_entries) does
        // not conflict with simultaneous borrows of other fields.
        let rows: Vec<RowData> = self
            .visible_entries()
            .into_iter()
            .map(|e| RowData {
                id: e.id,
                title: e.title.clone(),
                kind_label: match e.kind {
                    NoteKind::Markdown => "Note",
                    NoteKind::Asset => "Asset",
                    NoteKind::Folder => "Folder",
                },
                date_str: e.modified.format("%b %d").to_string().into(),
                checked: self.selected.contains(&e.id),
            })
            .collect();

        // --- Filter strip ---
        let filter_strip = h_flex()
            .h(px(32.0))
            .px(px(8.0))
            .border_b_1()
            .border_color(border_color)
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .text_color(if filter_text.is_empty() { muted } else { fg })
                    .child(if filter_text.is_empty() {
                        SharedString::new_static("Filter notes\u{2026}")
                    } else {
                        filter_text
                    }),
            );

        // --- Scrollable list ---
        let list: AnyElement = if rows.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(div().text_sm().text_color(muted).child("No notes"))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .overflow_y_scrollbar()
                .children(rows.into_iter().enumerate().map(|(ix, row)| {
                    let note_id = row.id;
                    let e = entity.clone();
                    let checked = row.checked;

                    let checkbox: AnyElement = if has_selection {
                        Checkbox::new(("nlp-chk", ix as u64))
                            .checked(checked)
                            .on_click(move |_, _window, cx| {
                                e.update(cx, |this, cx| this.toggle_selection(note_id, cx));
                            })
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    };

                    let open_handle = entity.clone();
                    h_flex()
                        .id(("nlp-row", ix as u64))
                        .w_full()
                        .px(px(8.0))
                        .py(px(3.0))
                        .gap_2()
                        .border_b_1()
                        .border_color(border_color)
                        .cursor_pointer()
                        .on_click(move |_, _window, cx| {
                            open_handle.update(cx, |this, cx| this.open(note_id, cx));
                        })
                        .child(checkbox)
                        .child(div().flex_1().text_sm().text_color(fg).child(row.title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::new_static(row.kind_label)),
                        )
                        .child(div().text_xs().text_color(muted).child(row.date_str))
                }))
                .into_any_element()
        };

        // --- Bulk action bar (only visible when ≥1 entry is selected) ---
        let bulk_bar: Option<AnyElement> = if has_selection {
            Some(
                h_flex()
                    .h(px(36.0))
                    .items_center()
                    .px(px(8.0))
                    .gap_2()
                    .border_t_1()
                    .border_color(border_color)
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(fg)
                            .child(SharedString::from(format!("{n_selected} selected"))),
                    )
                    .child(Button::new("nlp-delete").label("Delete").ghost())
                    .child(Button::new("nlp-archive").label("Archive").ghost())
                    .into_any_element(),
            )
        } else {
            None
        };

        v_flex()
            .size_full()
            .child(filter_strip)
            .child(list)
            .children(bulk_bar)
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
    /// reads it during render (mirrors `embed_poc/src/layout.rs`).
    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// An empty pane must render without panicking.
    #[gpui::test]
    fn empty_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| NoteListPane::new());
        cx.run_until_parked();
    }

    /// `from_mock` must load all 30 seeded notes.
    #[gpui::test]
    fn from_mock_loads_30_entries(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            let pane = NoteListPane::from_mock(cx);
            assert_eq!(
                pane.entries.len(),
                30,
                "from_mock must load exactly 30 notes from MockVault::seeded()"
            );
        });
    }

    /// `from_vault` must load every `.md` file from a real on-disk vault.
    #[gpui::test]
    fn from_vault_loads_real_notes(cx: &mut TestAppContext) {
        use std::fs;
        install_theme(cx);
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.md"), "alpha").unwrap();
        fs::write(dir.path().join("b.md"), "beta").unwrap();
        let vault = vault::Vault::open_at(dir.path()).expect("open vault");
        cx.update(|cx| {
            cx.set_global(vault);
            let pane = NoteListPane::from_vault(cx);
            assert_eq!(
                pane.entries.len(),
                2,
                "from_vault must load both .md files from the temp dir"
            );
        });
    }

    /// `from_or_empty` must prefer `vault::Vault` over `MockVault` when both
    /// are installed.  Phase 5-MVP precedence contract.
    #[gpui::test]
    fn from_or_empty_prefers_real_vault(cx: &mut TestAppContext) {
        use std::fs;
        install_theme(cx);
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("only.md"), "x").unwrap();
        let vault = vault::Vault::open_at(dir.path()).expect("open vault");
        cx.update(|cx| {
            cx.set_global(MockVault::seeded()); // 30 notes
            cx.set_global(vault); // 1 note
            let pane = NoteListPane::from_or_empty(cx);
            assert_eq!(
                pane.entries.len(),
                1,
                "real Vault must win over MockVault when both globals present"
            );
        });
    }

    /// A filter substring must narrow `visible_entries` to matching titles.
    #[gpui::test]
    fn filter_narrows_visible(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| cx.set_global(MockVault::seeded()));

        let window = cx.add_window(|_window, cx| NoteListPane::from_mock(cx));

        window
            .update(cx, |pane, _window, cx| {
                // "Laputa" appears in notes 14 ("Start Laputa App Project"),
                // 15 ("Laputa App V1"), 16 ("Laputa App V2"), and 28
                // ("Laputa QA Reference") — 4 matches.
                pane.set_filter("laputa", cx);
                assert_eq!(
                    pane.visible_entries().len(),
                    4,
                    "filter 'laputa' must match exactly 4 titles"
                );
            })
            .unwrap();
    }

    /// `toggle_selection` twice on the same id must leave selection empty.
    #[gpui::test]
    fn toggle_selection_round_trips(cx: &mut TestAppContext) {
        install_theme(cx);

        let window = cx.add_window(|_window, _cx| NoteListPane::new());

        window
            .update(cx, |pane, _window, cx| {
                pane.toggle_selection(NoteId::from_raw(1), cx);
                assert_eq!(
                    pane.selection_count(),
                    1,
                    "count must be 1 after first toggle"
                );
                pane.toggle_selection(NoteId::from_raw(1), cx);
                assert_eq!(
                    pane.selection_count(),
                    0,
                    "count must return to 0 after second toggle on the same id"
                );
            })
            .unwrap();
    }

    /// After toggling a selection the bulk action bar must be logically
    /// present (selection_count > 0) and the view must render without panic.
    #[gpui::test]
    fn bulk_bar_appears_when_selected(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| cx.set_global(MockVault::seeded()));

        let window = cx.add_window(|_window, cx| NoteListPane::from_mock(cx));

        window
            .update(cx, |pane, _window, cx| {
                pane.toggle_selection(NoteId::from_raw(1), cx);
                assert!(
                    pane.selection_count() > 0,
                    "selection_count must be > 0 so bulk bar renders"
                );
            })
            .unwrap();

        // Trigger a render cycle to exercise the bulk-bar branch.
        cx.run_until_parked();
    }

    /// `clear_selection` must reset selection count to zero.
    #[gpui::test]
    fn clear_selection_resets(cx: &mut TestAppContext) {
        install_theme(cx);

        let window = cx.add_window(|_window, _cx| NoteListPane::new());

        window
            .update(cx, |pane, _window, cx| {
                pane.toggle_selection(NoteId::from_raw(1), cx);
                pane.toggle_selection(NoteId::from_raw(2), cx);
                pane.toggle_selection(NoteId::from_raw(3), cx);
                assert_eq!(pane.selection_count(), 3);
                pane.clear_selection(cx);
                assert_eq!(
                    pane.selection_count(),
                    0,
                    "clear_selection must empty the selected set"
                );
            })
            .unwrap();
    }
}
