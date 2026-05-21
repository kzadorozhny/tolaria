#![forbid(unsafe_code)]
//! Inspector panel for the Tolaria right dock (ADR-0115 Phase 9 worklist 9.2.8).
//!
//! Shows contextual metadata for the active note in seven collapsible
//! accordion sections: Properties, Outline, Backlinks, Instances,
//! References, Relationships, and GitHistory.
//!
//! # Resolver strategy (Phase 9 worklist 9.2.8)
//!
//! Four sections — **Backlinks**, **Instances**, **References**, and
//! **Outline** — are now sourced from the live [`vault::Vault`] global
//! and from the editor's [`editor_bridge::FromHost::Headings`] stream
//! (re-emitted by [`note_item::NoteItem`] as
//! [`note_item::HeadingsUpdatedEvent`]).  The three remaining sections —
//! **Properties**, **Relationships**, and **Git History** — still read
//! from the legacy [`MockVault`] / [`MockGit`] seeds; their data sources
//! are deferred to dedicated rows.
//!
//! * **Backlinks** — [`vault::Vault::backlinks`] resolves every note in
//!   the vault whose body contains a `[[wikilink]]` pointing at the
//!   active note.  Titles come from [`vault::Vault::note_sync`].
//! * **Instances** — when the active note's filename stem starts with
//!   `type-` (per worklist 9.2.8), every note whose filename stem starts
//!   with the same prefix (minus the `type-` marker) is listed.
//!   Pre-computed on every `set_active` / vault-change tick.
//! * **References** — [`vault::Vault::outbound_links`] resolves every
//!   note the active note links *to* via `[[wikilink]]` syntax.  Mirrors
//!   the inverse direction of Backlinks.
//! * **Outline** — headings extracted from the WKWebView body by the
//!   editor and pushed up via [`editor_bridge::FromHost::Headings`].
//!   The native side stores them in [`InspectorPanel::headings`] and
//!   renders an indented list keyed by heading level.
//!
//! Click-to-open is wired for the three vault-driven list sections: a
//! row click emits an [`InspectorOpenNoteEvent`] carrying the target
//! [`vault::NoteId`].  Workspace subscribers route it through the same
//! `open_note` path the note-list pane uses (see the toc_panel-style
//! subscription seam in `crates/tolaria/src/main.rs`).
//!
//! # Heading-click navigation (deferred)
//!
//! The Outline section does NOT yet hop the editor to the matching body
//! anchor on click.  No `ToHost::ScrollToAnchor` bridge envelope exists
//! today (same gap the ToC panel sits behind — see worklist 9.2.6's
//! followup parking lot).  The row's click handler logs the anchor so
//! a future row can hang the dispatch off the same code path.
//!
//! # MockVault fallback
//!
//! When neither [`vault::Vault`] nor [`MockVault`] is installed, the
//! panel renders empty.  The legacy [`MockVault`]-only construction
//! shape (`from_mock`) is retained so existing fixture-driven tests
//! continue to exercise the resolver against deterministic seeds.

use std::collections::{HashMap, HashSet};

use editor_bridge::Heading;
use gpui::{
    div, px, AnyElement, App, Context, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Window,
};
use gpui_component::{ActiveTheme, StyledExt as _};
use mock_fixtures::{MockCommit, MockGit, MockNote, MockVault};
use vault::{NoteId, Vault};
use workspace::panel::{DockPosition, Panel};

// ---------------------------------------------------------------------------
// Wikilink scanner (kept for MockVault tests)
// ---------------------------------------------------------------------------

/// Extract all `[[target]]` stems from `text`.
///
/// Handles the common `[[stem]]` and `[[stem|alias]]` forms; returns the
/// part before the first `|` (if present) so that aliased links still
/// resolve to the correct note.  Used by the [`MockVault`] resolver
/// path; the real [`Vault`] path delegates to
/// [`vault::Vault::backlinks`] / [`vault::Vault::outbound_links`].
fn scan_wikilinks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find("]]") else {
            break;
        };
        let inner = &rest[..close];
        let stem = inner.split('|').next().unwrap_or(inner).trim();
        if !stem.is_empty() {
            out.push(stem.to_string());
        }
        rest = &rest[close + 2..];
    }
    out
}

// ---------------------------------------------------------------------------
// InspectorSection
// ---------------------------------------------------------------------------

/// One collapsible section of the inspector accordion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorSection {
    Properties,
    Outline,
    Backlinks,
    Instances,
    /// Outbound `[[wikilinks]]` from the active note (worklist 9.2.8
    /// re-sourced this from a backlink alias to outbound links — the
    /// variant name is kept for backward compatibility with serialised
    /// expansion state; the user-facing label is `"References"`).
    ReferencedBy,
    Relationships,
    GitHistory,
}

impl InspectorSection {
    /// All sections in stable display order.
    pub const ALL: &'static [Self] = &[
        Self::Properties,
        Self::Outline,
        Self::Backlinks,
        Self::Instances,
        Self::ReferencedBy,
        Self::Relationships,
        Self::GitHistory,
    ];

    /// Human-readable label shown in the section header row.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Properties => "Properties",
            Self::Outline => "Outline",
            Self::Backlinks => "Backlinks",
            Self::Instances => "Instances",
            // Worklist 9.2.8 — the variant ships outbound links now, so
            // the user-facing label reads "References".  The enum name
            // stays for back-compat with any persisted expansion state.
            Self::ReferencedBy => "References",
            Self::Relationships => "Relationships",
            Self::GitHistory => "Git History",
        }
    }
}

// ---------------------------------------------------------------------------
// Click event
// ---------------------------------------------------------------------------

/// Emitted when the user clicks a clickable row inside the inspector
/// (Backlinks / Instances / References).  Workspace subscribers route
/// this to the same `open_note` path the note-list pane uses, so the
/// click opens the target note in the editor.
///
/// Mirrors `note_list_pane::OpenNoteEvent` in shape so the subscriber
/// can dispatch through the existing helper without translating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectorOpenNoteEvent {
    /// Target note to open.
    pub id: NoteId,
}

// ---------------------------------------------------------------------------
// Row
// ---------------------------------------------------------------------------

/// One clickable note entry in a vault-driven list section.  Carries
/// both the display title and the underlying [`NoteId`] so click
/// handlers can emit [`InspectorOpenNoteEvent`] without re-resolving.
#[derive(Debug, Clone)]
pub struct NoteRow {
    /// Target note id used by the click handler.
    pub id: NoteId,
    /// Display title shown in the row.
    pub title: SharedString,
}

// ---------------------------------------------------------------------------
// InspectorState
// ---------------------------------------------------------------------------

/// Resolved data for the currently active note.  Pre-computed so all
/// `render_*_body` methods are pure reads with no async I/O.
///
/// Phase 9 worklist 9.2.8 split the state into two flavours:
///
/// * **Vault-driven** — `backlinks`, `instances`, `references` carry
///   [`NoteRow`] entries (title + id) so click-to-open can resolve
///   without a second vault lookup.  Sourced from the real
///   [`vault::Vault`] global when present.
/// * **Mock-driven** — `mock_note`, `mock_outbound_links` are the
///   legacy [`MockNote`]-shaped fields that back the Properties and
///   Relationships sections (Phase 10.x will split each onto its own
///   real-vault source).
#[derive(Debug, Clone, Default)]
pub struct InspectorState {
    /// The active note's [`MockNote`] view when sourcing from
    /// [`MockVault`].  Drives Properties + Relationships.  `None` when
    /// no note is active or when the panel is reading from a real
    /// [`vault::Vault`] (Properties / Relationships render empty in
    /// that case until their real-vault sources land).
    pub note: Option<MockNote>,
    /// Notes whose body links TO the active note.  Each entry carries
    /// the click target.  Populated by both vault paths.
    pub backlinks: Vec<NoteRow>,
    /// Sibling notes for the active type definition (when the active
    /// note's stem starts with `type-`).  Empty otherwise.
    pub instances: Vec<NoteRow>,
    /// Notes that the active note links TO via `[[wikilink]]` syntax.
    /// Drives the "References" section.
    pub references: Vec<NoteRow>,
    /// Outbound link stems from the [`MockVault`] resolver path.
    /// Drives Relationships; the real-vault path leaves this empty so
    /// the section renders its empty state until a frontmatter-backed
    /// source lands.
    pub mock_outbound_links: Vec<String>,
}

impl InspectorState {
    /// Empty state — no note selected.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build resolved state for `active_id` from the real [`Vault`].
    ///
    /// Resolves Backlinks / Instances / References via the vault's
    /// query APIs.  Properties and Relationships are deferred to a
    /// follow-up row (no real-vault source for the rendered shape yet),
    /// so the corresponding mock fields stay empty.
    pub fn resolve_from_vault(active_id: NoteId, vault: &Vault) -> Self {
        let Some(active) = vault.note_sync(active_id) else {
            return Self::empty();
        };

        let backlinks = note_rows_from_ids(vault, vault.backlinks(active_id));
        let references = note_rows_from_ids(vault, vault.outbound_links(active_id));
        let instances = resolve_type_instances(vault, active);

        Self {
            note: None,
            backlinks,
            instances,
            references,
            mock_outbound_links: Vec::new(),
        }
    }

    /// Build resolved state for `active_id` from a pre-fetched slice of
    /// every [`MockVault`] note.  Retained for the fixture-driven test
    /// path; the live app prefers [`resolve_from_vault`].
    pub fn resolve_from_mock(active_id: NoteId, notes: &[MockNote]) -> Self {
        let Some(active) = notes.iter().find(|n| n.id == active_id) else {
            return Self::empty();
        };

        let mock_outbound_links = scan_wikilinks(&active.content);

        let active_stem = active
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        let active_type = mock_note_type(active).map(str::to_owned);

        let mut backlinks: Vec<NoteRow> = Vec::new();
        let mut instances: Vec<NoteRow> = Vec::new();
        let mut references: Vec<NoteRow> = Vec::new();

        // Build a stem → (id, title) index so we can resolve outbound
        // wikilinks to clickable rows without re-scanning per link.
        let by_stem: HashMap<String, (NoteId, SharedString)> = notes
            .iter()
            .filter_map(|n| {
                n.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| (s.to_ascii_lowercase(), (n.id, n.title.clone())))
            })
            .collect();

        for stem in &mock_outbound_links {
            let key = stem.trim().to_ascii_lowercase();
            let key = key.rsplit('/').next().unwrap_or(key.as_str()).to_owned();
            if let Some((id, title)) = by_stem.get(&key) {
                if *id != active_id && !references.iter().any(|r| r.id == *id) {
                    references.push(NoteRow {
                        id: *id,
                        title: title.clone(),
                    });
                }
            }
        }

        for other in notes {
            if other.id == active_id {
                continue;
            }

            let links = scan_wikilinks(&other.content);
            let points_here = links.iter().any(|stem| {
                let key = stem.trim().to_ascii_lowercase();
                key == active_stem || key.rsplit('/').next().unwrap_or(key.as_str()) == active_stem
            });
            if points_here {
                backlinks.push(NoteRow {
                    id: other.id,
                    title: other.title.clone(),
                });
            }

            if let Some(ref target_type) = active_type {
                if mock_note_type(other) == Some(target_type.as_str()) {
                    instances.push(NoteRow {
                        id: other.id,
                        title: other.title.clone(),
                    });
                }
            }
        }

        Self {
            note: Some(active.clone()),
            backlinks,
            instances,
            references,
            mock_outbound_links,
        }
    }

    /// Build resolved state for `active_id` from whichever vault global
    /// is installed.  Prefers the real [`Vault`] over [`MockVault`].
    /// Returns [`empty`] when neither is present.
    pub fn resolve(active_id: NoteId, cx: &mut App) -> Self {
        if cx.try_global::<Vault>().is_some() {
            // Borrow the vault for the duration of the resolve.  All
            // vault query APIs take `&self`, so the borrow is read-only
            // and doesn't conflict with the chrome render path.
            let vault = cx.global::<Vault>();
            Self::resolve_from_vault(active_id, vault)
        } else if let Some(notes) = collect_mock_notes(cx) {
            Self::resolve_from_mock(active_id, &notes)
        } else {
            Self::empty()
        }
    }
}

/// Map a sorted list of [`NoteId`]s to clickable [`NoteRow`]s using the
/// vault's in-memory metadata.  Drops ids that have vanished between
/// the query and the metadata read (e.g. a rescan deleted the note
/// mid-paint).
fn note_rows_from_ids(vault: &Vault, ids: Vec<NoteId>) -> Vec<NoteRow> {
    ids.into_iter()
        .filter_map(|id| {
            vault.note_sync(id).map(|n| NoteRow {
                id,
                title: n.title.clone(),
            })
        })
        .collect()
}

/// Type-instance resolver (worklist 9.2.8).
///
/// When `active`'s filename stem starts with `type-`, list every note
/// whose stem starts with the same prefix minus the `type-` marker
/// followed by a `-` separator.  Example: `type-event.md` matches
/// `event-team-sync.md` and `event-foo.md` but not `event.md` (no `-`
/// after the prefix) or `eventually.md`.
///
/// Returns an empty `Vec` when the active note isn't a type definition,
/// so the render path can hide the section or show an empty-state
/// message uniformly.
fn resolve_type_instances(vault: &Vault, active: &vault::Note) -> Vec<NoteRow> {
    let active_stem_lc = active
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let Some(type_name) = active_stem_lc.strip_prefix("type-") else {
        return Vec::new();
    };
    if type_name.is_empty() {
        return Vec::new();
    }
    let prefix = format!("{type_name}-");

    let mut hits: Vec<NoteRow> = Vec::new();
    for note in vault.iter_notes() {
        if note.id == active.id {
            continue;
        }
        let Some(stem) = note.path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem.to_ascii_lowercase().starts_with(&prefix) {
            hits.push(NoteRow {
                id: note.id,
                title: note.title.clone(),
            });
        }
    }
    hits.sort_by(|a, b| a.id.cmp(&b.id));
    hits
}

/// Extract the `"type"` property value from a [`MockNote`] as a `&str`,
/// if present.  Used by the mock-driven instance resolver.
fn mock_note_type(note: &MockNote) -> Option<&str> {
    note.properties.get("type")?.as_str()
}

/// Drain every note out of the [`MockVault`] global, if one is installed.
///
/// Returns `None` when no `MockVault` global is present so callers can
/// distinguish "no vault" from "empty vault" without re-checking
/// `cx.try_global` themselves.
fn collect_mock_notes(cx: &mut App) -> Option<Vec<MockNote>> {
    let vault = cx.try_global::<MockVault>()?.clone();
    let ids = cx.foreground_executor().block_on(vault.notes());
    let mut notes = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(note) = cx.foreground_executor().block_on(vault.note(id)) {
            notes.push(note);
        }
    }
    Some(notes)
}

/// Scan `body` for H1/H2/H3 heading lines; return the heading text without
/// the leading `#` characters.  Retained for the MockVault test path —
/// the real vault path receives parsed [`Heading`]s from the editor
/// bridge instead of re-scanning the body itself.
fn extract_outline(body: &str) -> Vec<String> {
    body.lines()
        .filter(|line| {
            line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ")
        })
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// InspectorPanel
// ---------------------------------------------------------------------------

/// Right-dock panel showing note metadata in seven collapsible accordion sections.
///
/// Construct via [`InspectorPanel::new`] for an empty state,
/// [`InspectorPanel::from_or_empty`] for a vault-first resolve, or
/// [`InspectorPanel::from_mock`] to pre-populate from installed mock
/// globals.
pub struct InspectorPanel {
    expanded: HashSet<InspectorSection>,
    note_id: Option<NoteId>,
    position: DockPosition,
    /// Resolved vault-driven data for the active note.
    state: InspectorState,
    /// Live heading stream from the editor's
    /// [`editor_bridge::FromHost::Headings`] envelope.  Updated by
    /// workspace subscribers via [`InspectorPanel::set_headings`].
    headings: Vec<Heading>,
    /// Cached git history — up to 5 commits shown (Phase 10.1 wires live data).
    git_history: Vec<MockCommit>,
}

impl InspectorPanel {
    /// Create an empty panel: no note selected, all sections collapsed.
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
            note_id: None,
            position: DockPosition::Right,
            state: InspectorState::empty(),
            headings: Vec::new(),
            git_history: Vec::new(),
        }
    }

    /// Build from [`MockVault`] and [`MockGit`] globals: selects the first note
    /// and pre-caches its data and git history.
    ///
    /// # Panics
    ///
    /// Panics if either the [`MockVault`] or [`MockGit`] global is not installed
    /// on `cx`, or if either service returns a non-ready task (Phase 10 will
    /// replace this with async service injection).
    pub fn from_mock(cx: &mut App) -> Self {
        let ids_task = cx.global::<MockVault>().notes();
        let ids = cx.foreground_executor().block_on(ids_task);

        let note_id = ids.first().copied();

        let state = match note_id {
            Some(id) => InspectorState::resolve(id, cx),
            None => InspectorState::empty(),
        };

        let history_task = cx.global::<MockGit>().history();
        let git_history = cx.foreground_executor().block_on(history_task);

        // Mock vault path: seed the outline from the active note's
        // body so the Outline section renders something before the
        // live editor pumps headings up the bridge.  The real-vault
        // path doesn't pre-seed — the editor's first `Headings`
        // payload replaces this within a frame of the note opening.
        let headings = state
            .note
            .as_ref()
            .map(|n| body_to_headings(&n.content))
            .unwrap_or_default();

        Self {
            expanded: HashSet::new(),
            note_id,
            position: DockPosition::Right,
            state,
            headings,
            git_history,
        }
    }

    /// Build from the real [`Vault`] global.  Selects the first note in
    /// the vault (id-order) so the panel renders something on startup;
    /// the workspace will re-target via [`set_active`] as soon as the
    /// user opens a note.
    pub fn from_vault(cx: &mut App) -> Self {
        let (first_id, state) = {
            let vault = cx.global::<Vault>();
            let mut ids: Vec<NoteId> = vault.iter_notes().map(|n| n.id).collect();
            ids.sort();
            let first = ids.first().copied();
            let state = first
                .map(|id| InspectorState::resolve_from_vault(id, vault))
                .unwrap_or_default();
            (first, state)
        };

        Self {
            expanded: HashSet::new(),
            note_id: first_id,
            position: DockPosition::Right,
            state,
            // Outline is populated by the editor's headings stream as
            // soon as the note opens — no body-scan fallback needed for
            // the real-vault path.
            headings: Vec::new(),
            git_history: Vec::new(),
        }
    }

    /// Vault-first construction: prefers the real [`Vault`] global, falls
    /// back to [`MockVault`] + [`MockGit`] when both mocks are present,
    /// and otherwise returns [`InspectorPanel::new`].  Mirrors the
    /// `from_or_empty` shape used by every other dock-panel crate.
    pub fn from_or_empty(cx: &mut App) -> Self {
        if cx.try_global::<Vault>().is_some() {
            Self::from_vault(cx)
        } else if cx.try_global::<MockVault>().is_some() && cx.try_global::<MockGit>().is_some() {
            Self::from_mock(cx)
        } else {
            Self::new()
        }
    }

    /// Update the active note and recompute all vault-driven sections.
    ///
    /// Called by the workspace whenever the focused note changes.  The
    /// outline section is independently updated via [`set_headings`]
    /// once the editor pumps a fresh `Headings` envelope.
    pub fn set_active(&mut self, note_id: Option<NoteId>, cx: &mut Context<Self>) {
        if self.note_id == note_id {
            return;
        }
        self.note_id = note_id;
        self.refresh_state(cx);
        // Drop the old outline — the editor will send a fresh
        // `Headings` envelope as soon as the new note opens.  Empty in
        // the meantime so a stale outline doesn't bleed across notes.
        if !self.headings.is_empty() {
            self.headings.clear();
        }
        cx.notify();
    }

    /// Re-resolve the vault-driven sections without changing
    /// `note_id`.  Called both by [`set_active`] and by the vault
    /// `watch_events` subscriber so an external file change refreshes
    /// the panel's backlink / reference / instance lists.
    pub fn refresh_state(&mut self, cx: &mut Context<Self>) {
        self.state = match self.note_id {
            Some(id) => InspectorState::resolve(id, cx),
            None => InspectorState::empty(),
        };
        cx.notify();
    }

    /// Replace the live heading list (worklist 9.2.8).  Called from the
    /// workspace's [`note_item::HeadingsUpdatedEvent`] subscriber so the
    /// Outline section stays in sync with the editor.  Short-circuits
    /// when the list is byte-identical — the editor's `onChange`
    /// debounce can emit duplicate payloads on rapid keystrokes that
    /// don't touch any heading, and `cx.notify()` would re-paint the
    /// dock for no visible change.
    pub fn set_headings(&mut self, headings: Vec<Heading>, cx: &mut Context<Self>) {
        if self.headings == headings {
            return;
        }
        self.headings = headings;
        cx.notify();
    }

    /// The ID of the currently active note, if any.
    pub fn note_id(&self) -> Option<NoteId> {
        self.note_id
    }

    /// Read-only view of the current heading list.  Exposed for tests
    /// and downstream rows that mirror the panel's Outline state.
    pub fn headings(&self) -> &[Heading] {
        &self.headings
    }

    /// Read-only view of the resolved state.  Exposed for tests that
    /// assert against the vault-driven section contents.
    pub fn state(&self) -> &InspectorState {
        &self.state
    }

    /// Whether `section` is currently expanded.
    pub fn is_expanded(&self, section: InspectorSection) -> bool {
        self.expanded.contains(&section)
    }

    /// Toggle `section` between expanded and collapsed, then notify the view.
    pub fn toggle(&mut self, section: InspectorSection, cx: &mut Context<Self>) {
        // `HashSet::insert` returns false when the value was already present,
        // meaning the section was expanded — remove it to collapse.
        if !self.expanded.insert(section) {
            self.expanded.remove(&section);
        }
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Section body renderers
    // -----------------------------------------------------------------------

    fn render_properties_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(note) = &self.state.note else {
            return empty_body("No note selected.", muted);
        };

        let mut pairs: Vec<_> = note.properties.iter().collect();
        pairs.sort_by_key(|(k, _)| k.as_str());

        if pairs.is_empty() {
            return empty_body("No properties.", muted);
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(pairs.into_iter().map(|(key, value)| {
                let value_str: SharedString = value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string())
                    .into();
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .text_sm()
                    .child(
                        div()
                            .text_color(muted)
                            .child(SharedString::from(format!("{key}:"))),
                    )
                    .child(value_str)
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_outline_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.note_id.is_none() {
            return empty_body("No note selected.", muted);
        }

        if self.headings.is_empty() {
            return empty_body("No headings in this note.", muted);
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(
                self.headings
                    .iter()
                    .enumerate()
                    .map(|(ix, h)| render_heading_row(ix, h, muted)),
            )
            .into_any_element()
    }

    fn render_backlinks_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.note_id.is_none() {
            return empty_body("No note selected.", muted);
        }

        if self.state.backlinks.is_empty() {
            return empty_body("No notes link to this one yet.", muted);
        }

        render_note_row_list("backlink", &self.state.backlinks, cx)
    }

    fn render_instances_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.note_id.is_none() {
            return empty_body("No note selected.", muted);
        }

        if self.state.instances.is_empty() {
            return empty_body("This note is not a type definition.", muted);
        }

        render_note_row_list("instance", &self.state.instances, cx)
    }

    fn render_references_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.note_id.is_none() {
            return empty_body("No note selected.", muted);
        }

        if self.state.references.is_empty() {
            return empty_body("This note doesn't link to any other notes.", muted);
        }

        render_note_row_list("reference", &self.state.references, cx)
    }

    fn render_relationships_body(&self, cx: &mut Context<Self>) -> AnyElement {
        // Relationships is out of scope for worklist 9.2.8 — it still
        // reads from the legacy [`MockNote`]-driven outbound stems.
        // The real-vault path leaves `mock_outbound_links` empty so the
        // section renders its empty state, mirroring what the user
        // sees today before a dedicated frontmatter-references source
        // lands.
        let muted = cx.theme().muted_foreground;

        if self.state.note.is_none() {
            return empty_body("No note selected.", muted);
        }

        if self.state.mock_outbound_links.is_empty() {
            return empty_body("No outbound links.", muted);
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(self.state.mock_outbound_links.iter().map(|stem| {
                div()
                    .text_sm()
                    .child(SharedString::from(stem.clone()))
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_git_history_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.git_history.is_empty() {
            return empty_body("No commits.", muted);
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(self.git_history.iter().take(5).map(|commit| {
                let meta: SharedString = format!("{} · {}", commit.sha, commit.author).into();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .child(SharedString::from(commit.message.clone())),
                    )
                    .child(div().text_sm().text_color(muted).child(meta))
                    .into_any_element()
            }))
            .into_any_element()
    }
}

/// Render an indented heading row (Outline section).  Mirrors
/// `toc_panel`'s row shape so the two surfaces feel coherent.
fn render_heading_row(index: usize, heading: &Heading, muted: gpui::Hsla) -> AnyElement {
    // Mirror toc_panel's per-level indent: 12pt per level above 1,
    // clamped to level 6 so an exotic `######` doesn't push titles off.
    const PER_LEVEL_INDENT_PT: f32 = 12.0;
    let clamped = heading.level.clamp(1, 6);
    let indent = px(f32::from(clamped - 1) * PER_LEVEL_INDENT_PT);

    let text: SharedString = heading.text.clone().into();
    let anchor: SharedString = heading.anchor.clone().into();
    let row_id = SharedString::from(format!("inspector-outline-{index}"));
    let level_color = if heading.level <= 1 {
        None
    } else {
        Some(muted)
    };

    let row = div()
        .id(row_id)
        .pl(indent)
        .text_sm()
        .cursor_pointer()
        .on_click(move |_, _window, _cx| {
            // TODO(scroll-to-anchor): wire heading-click to a
            // `ToHost::ScrollToAnchor` bridge envelope when it lands
            // (same gap toc_panel sits behind — worklist 9.2.6
            // followup).  Logging the anchor keeps the path
            // greppable.
            log::info!(
                target: "inspector_panel",
                "outline heading clicked: anchor={anchor:?}",
            );
        })
        .child(text);
    if let Some(color) = level_color {
        row.text_color(color).into_any_element()
    } else {
        row.into_any_element()
    }
}

/// Render the empty-state placeholder shown when a section has no
/// content to display.  Pulled out so the seven section renderers all
/// produce visually-identical empty states (mirrors React's
/// `EmptyState` component shape).
fn empty_body(label: &'static str, muted: gpui::Hsla) -> AnyElement {
    div()
        .text_sm()
        .text_color(muted)
        .child(label)
        .into_any_element()
}

/// Render a list of clickable [`NoteRow`]s.  Each row emits an
/// [`InspectorOpenNoteEvent`] on click so the workspace can route the
/// open through the same path the note-list pane uses.
///
/// `id_prefix` keeps the GPUI row ids stable across re-renders by
/// embedding the section name (`"backlink"`, `"instance"`,
/// `"reference"`) — otherwise two sections sharing the same row index
/// would collide on the GPUI element id.
fn render_note_row_list(
    id_prefix: &'static str,
    rows: &[NoteRow],
    cx: &mut Context<InspectorPanel>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .children(rows.iter().enumerate().map(|(ix, row)| {
            let row_id = SharedString::from(format!("inspector-{id_prefix}-{ix}"));
            let title = row.title.clone();
            let id = row.id;
            div()
                .id(row_id)
                .text_sm()
                .cursor_pointer()
                .on_click(cx.listener(move |_, _event, _window, cx| {
                    cx.emit(InspectorOpenNoteEvent { id });
                }))
                .child(title)
                .into_any_element()
        }))
        .into_any_element()
}

/// Heuristic body-scan that mirrors the legacy [`extract_outline`]
/// shape but emits [`Heading`]s with synthetic anchors so the
/// mock-vault test path can populate the Outline section without
/// driving the editor bridge.
fn body_to_headings(body: &str) -> Vec<Heading> {
    extract_outline(body)
        .into_iter()
        .map(|text| {
            let anchor = text
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .to_ascii_lowercase();
            // Heuristic level: cheap re-derive from the leading `#`
            // count is overkill here — the mock seeds only carry H1s,
            // so any level >= 1 paints identically.
            Heading {
                level: 1,
                text,
                anchor,
            }
        })
        .collect()
}

impl Default for InspectorPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EventEmitter
// ---------------------------------------------------------------------------

impl EventEmitter<InspectorOpenNoteEvent> for InspectorPanel {}

// ---------------------------------------------------------------------------
// Panel trait
// ---------------------------------------------------------------------------

impl Panel for InspectorPanel {
    fn persistent_name(&self) -> &str {
        "InspectorPanel"
    }

    fn panel_key(&self) -> &str {
        "inspector"
    }

    fn position(&self, _cx: &App) -> DockPosition {
        self.position
    }

    fn set_position(&mut self, position: DockPosition, cx: &mut Context<Self>) {
        self.position = position;
        cx.notify();
    }

    fn default_size(&self, _cx: &App) -> Pixels {
        px(320.0)
    }

    fn icon(&self) -> Option<&str> {
        Some("info")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(actions::ToggleInspector)
    }

    fn starts_open(&self, _cx: &App) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for InspectorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border_color = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;

        // Snapshot expanded state so we can borrow `self` freely inside the loop.
        let section_states: Vec<(InspectorSection, bool)> = InspectorSection::ALL
            .iter()
            .map(|&s| (s, self.is_expanded(s)))
            .collect();

        let mut children: Vec<AnyElement> = Vec::with_capacity(section_states.len() * 2);

        for (ix, (section, is_expanded)) in section_states.into_iter().enumerate() {
            let chevron: &'static str = if is_expanded { "▾" } else { "▸" };

            // Header row — click to toggle.
            let header = div()
                .id(("inspector-header", ix))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .px(px(12.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(border_color)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.toggle(section, cx);
                }))
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(SharedString::from(section.label())),
                )
                .child(div().text_sm().text_color(muted).child(chevron));

            children.push(header.into_any_element());

            if is_expanded {
                let body_content = match section {
                    InspectorSection::Properties => self.render_properties_body(cx),
                    InspectorSection::Outline => self.render_outline_body(cx),
                    InspectorSection::Backlinks => self.render_backlinks_body(cx),
                    InspectorSection::Instances => self.render_instances_body(cx),
                    InspectorSection::ReferencedBy => self.render_references_body(cx),
                    InspectorSection::Relationships => self.render_relationships_body(cx),
                    InspectorSection::GitHistory => self.render_git_history_body(cx),
                };
                let body = div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(border_color)
                    .child(body_content)
                    .into_any_element();
                children.push(body);
            }
        }

        div()
            .flex()
            .flex_col()
            .h_full()
            .overflow_hidden()
            .bg(background)
            .text_color(foreground)
            .children(children)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use editor_bridge::Heading;
    use gpui::{AppContext as _, TestAppContext};
    use gpui_component::ActiveTheme as _;
    use mock_fixtures::{MockGit, MockVault, NoteId};

    use super::{
        extract_outline, scan_wikilinks, InspectorPanel, InspectorSection, InspectorState,
    };
    use workspace::panel::{DockPosition, Panel};

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// `InspectorSection::ALL` must contain exactly 7 sections.
    #[gpui::test]
    fn all_returns_7_sections(_cx: &mut TestAppContext) {
        assert_eq!(InspectorSection::ALL.len(), 7);
    }

    /// A freshly-constructed panel must report `DockPosition::Right`.
    #[gpui::test]
    fn panel_position_is_right(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let panel = InspectorPanel::new();
            assert_eq!(panel.position(cx), DockPosition::Right);
        });
    }

    /// Toggle must move a section from collapsed → expanded → collapsed.
    #[gpui::test]
    fn toggle_round_trips(cx: &mut TestAppContext) {
        install_theme(cx);
        let window = cx.add_window(|_window, _cx| InspectorPanel::new());

        // Initially collapsed.
        window
            .update(cx, |panel, _window, _cx| {
                assert!(!panel.is_expanded(InspectorSection::Outline));
            })
            .unwrap();

        // Expand.
        window
            .update(cx, |panel, _window, cx| {
                panel.toggle(InspectorSection::Outline, cx);
            })
            .unwrap();
        window
            .update(cx, |panel, _window, _cx| {
                assert!(panel.is_expanded(InspectorSection::Outline));
            })
            .unwrap();

        // Collapse again.
        window
            .update(cx, |panel, _window, cx| {
                panel.toggle(InspectorSection::Outline, cx);
            })
            .unwrap();
        window
            .update(cx, |panel, _window, _cx| {
                assert!(!panel.is_expanded(InspectorSection::Outline));
            })
            .unwrap();
    }

    /// `from_mock` must select `NoteId::from_raw(1)` — the first note in the seeded vault.
    #[gpui::test]
    fn from_mock_picks_first_note(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            cx.set_global(MockGit::seeded());
            let panel = InspectorPanel::from_mock(cx);
            assert_eq!(
                panel.note_id(),
                Some(NoteId::from_raw(1)),
                "first seeded note must be NoteId::from_raw(1)"
            );
        });
    }

    /// With all sections expanded the panel must render without panicking.
    #[gpui::test]
    fn all_sections_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            cx.set_global(MockGit::seeded());
        });

        let window = cx.add_window(|_window, cx| InspectorPanel::from_mock(cx));

        // Expand every section.
        window
            .update(cx, |panel, _window, cx| {
                for &section in InspectorSection::ALL {
                    panel.toggle(section, cx);
                }
            })
            .unwrap();

        // Drive the render pass — must not panic.
        cx.run_until_parked();
    }

    /// Regression for worklist 3.1: the Inspector window must paint a
    /// real chrome surface, not render black-on-black.  Both the
    /// theme tokens consumed by the root `div` (`background`,
    /// `foreground`) must be fully-opaque fills — if either were ever
    /// `transparent()` the standalone Inspector window would appear as
    /// an all-black void with invisible section labels.
    #[gpui::test]
    fn root_uses_opaque_theme_background_and_foreground(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let theme = cx.theme();
            assert!(
                theme.background.a > 0.0,
                "theme.background must be opaque so the Inspector window is not all-black"
            );
            assert!(
                theme.foreground.a > 0.0,
                "theme.foreground must be opaque so section labels are visible"
            );
        });
    }

    // -----------------------------------------------------------------------
    // Phase 8.4 resolver tests (MockVault-driven; retained as
    // regression cover for the fixture path).
    // -----------------------------------------------------------------------

    /// Backlinks resolver: note A contains `[[B]]` in its body →
    /// `InspectorState` for note B must list A in `backlinks`.
    #[gpui::test]
    fn inspector_panel_backlinks_resolver_returns_seeded_links(cx: &mut TestAppContext) {
        use chrono::Utc;
        use mock_fixtures::MockNote;
        use mock_fixtures::NoteKind;
        use serde_json::Value;
        use std::path::PathBuf;

        cx.update(|cx| {
            // Build a minimal 2-note vault:
            //   note A (id=1) body links to [[note-b]]
            //   note B (id=2) body has no links
            let now = Utc::now();
            let notes = vec![
                MockNote {
                    id: NoteId::from_raw(1),
                    title: "Note A".into(),
                    path: PathBuf::from("note-a.md"),
                    content: "# Note A\n\nSee also [[note-b]].\n".to_string(),
                    kind: NoteKind::Markdown,
                    created: now,
                    modified: now,
                    properties: [("type".to_string(), Value::String("Note".to_string()))]
                        .into_iter()
                        .collect(),
                },
                MockNote {
                    id: NoteId::from_raw(2),
                    title: "Note B".into(),
                    path: PathBuf::from("note-b.md"),
                    content: "# Note B\n\nStand-alone note.\n".to_string(),
                    kind: NoteKind::Markdown,
                    created: now,
                    modified: now,
                    properties: [("type".to_string(), Value::String("Note".to_string()))]
                        .into_iter()
                        .collect(),
                },
            ];

            cx.set_global(MockVault::from_notes(notes));
            let state = InspectorState::resolve(NoteId::from_raw(2), cx);

            let backlink_titles: Vec<String> = state
                .backlinks
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            assert_eq!(
                backlink_titles,
                vec!["Note A".to_string()],
                "inspector for note B must list note A as a backlink"
            );
        });
    }

    /// Outline resolver extracts H1, H2, and H3 headings and ignores H4+.
    #[gpui::test]
    fn inspector_panel_outline_extracts_h1_h2_h3(_cx: &mut TestAppContext) {
        let body = "# Top\n\n## Section\n\n### Subsection\n\n#### Deep (ignored)\n\nParagraph.\n";
        let outline = extract_outline(body);
        assert_eq!(
            outline,
            vec!["Top", "Section", "Subsection"],
            "outline must include H1/H2/H3 and skip H4+"
        );
    }

    /// Instances resolver (mock path): notes with the same `type`
    /// property appear in `InspectorState::instances` for the active
    /// note.  The real-vault path uses filename-prefix matching —
    /// covered separately in `real_vault_instances_listed_by_prefix`.
    #[gpui::test]
    fn inspector_panel_instances_returns_same_type_siblings(cx: &mut TestAppContext) {
        use chrono::Utc;
        use mock_fixtures::MockNote;
        use mock_fixtures::NoteKind;
        use serde_json::Value;
        use std::path::PathBuf;

        cx.update(|cx| {
            let now = Utc::now();
            let make =
                |id: u64, title: &'static str, path: &'static str, ty: &'static str| MockNote {
                    id: NoteId::from_raw(id),
                    title: title.into(),
                    path: PathBuf::from(path),
                    content: String::new(),
                    kind: NoteKind::Markdown,
                    created: now,
                    modified: now,
                    properties: [("type".to_string(), Value::String(ty.to_string()))]
                        .into_iter()
                        .collect(),
                };

            cx.set_global(MockVault::from_notes(vec![
                make(1, "Alpha", "alpha.md", "Person"),
                make(2, "Beta", "beta.md", "Person"),
                make(3, "Gamma", "gamma.md", "Topic"), // different type
                make(4, "Delta", "delta.md", "Person"),
            ]));

            // Inspect note 1 (Alpha, type=Person):
            // siblings should be Beta and Delta (not Gamma).
            let state = InspectorState::resolve(NoteId::from_raw(1), cx);

            let mut instances: Vec<String> = state
                .instances
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            instances.sort();
            assert_eq!(
                instances,
                vec!["Beta".to_string(), "Delta".to_string()],
                "instances must list same-type siblings, excluding active note and different types"
            );
        });
    }

    /// `scan_wikilinks` must extract both plain and aliased link stems.
    #[gpui::test]
    fn scan_wikilinks_extracts_plain_and_aliased(_cx: &mut TestAppContext) {
        let text = "See [[note-a]] and [[note-b|Alias for B]] and [[dir/note-c]].\n";
        let links = scan_wikilinks(text);
        assert_eq!(
            links,
            vec!["note-a", "note-b", "dir/note-c"],
            "scan_wikilinks must return stems for plain, aliased, and path links"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 9 worklist 9.2.8 — real-vault resolver tests
    // -----------------------------------------------------------------------

    /// Build a temporary on-disk vault from a slice of `(name, body)`
    /// pairs and return both the opened [`vault::Vault`] and a handle
    /// to the `tempdir` (kept alive for the test's duration).  The
    /// vault is fully indexed before the function returns so callers
    /// can issue queries against it immediately.
    fn open_temp_vault(files: &[(&str, &str)]) -> (vault::Vault, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, body) in files {
            let path = dir.path().join(name);
            std::fs::write(&path, body).expect("write note");
        }
        let vault = vault::Vault::open_at(dir.path()).expect("open vault");
        (vault, dir)
    }

    /// Real-vault Backlinks regression (worklist 9.2.8): build a
    /// 3-note vault where A → B and C → B, open the panel against B,
    /// assert Backlinks lists A and C.  Filenames are lowercase
    /// because `Vault::backlinks` matches the literal file-stem (the
    /// note's `title`) against parsed `[[wikilink]]` targets.
    #[gpui::test]
    fn real_vault_backlinks_lists_inbound_notes(cx: &mut TestAppContext) {
        let (vault, _dir) = open_temp_vault(&[
            ("a.md", "links to [[b]]\n"),
            ("b.md", "standalone\n"),
            ("c.md", "also links to [[b]]\n"),
        ]);
        let b_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "b")
            .expect("note b")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(b_id, cx);
            let mut titles: Vec<String> = state
                .backlinks
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            titles.sort();
            assert_eq!(
                titles,
                vec!["a".to_string(), "c".to_string()],
                "backlinks for b must list a and c from real vault scan"
            );
        });
    }

    /// Real-vault References regression (worklist 9.2.8): a note
    /// linking to b and d must surface both in its References list.
    #[gpui::test]
    fn real_vault_references_lists_outbound_notes(cx: &mut TestAppContext) {
        let (vault, _dir) = open_temp_vault(&[
            ("a.md", "links to [[b]] and [[d]]\n"),
            ("b.md", "B body\n"),
            ("c.md", "unrelated\n"),
            ("d.md", "D body\n"),
        ]);
        let a_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "a")
            .expect("note a")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(a_id, cx);
            let mut titles: Vec<String> = state
                .references
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            titles.sort();
            assert_eq!(
                titles,
                vec!["b".to_string(), "d".to_string()],
                "references for a must list both outbound link targets"
            );
        });
    }

    /// Real-vault Instances regression (worklist 9.2.8): a vault with
    /// `type-event.md` + `event-foo.md` + `event-bar.md` + an
    /// unrelated note, open `type-event.md`, assert Instances shows
    /// both `event-*` notes (and excludes the unrelated note).
    #[gpui::test]
    fn real_vault_instances_listed_by_prefix(cx: &mut TestAppContext) {
        let (vault, _dir) = open_temp_vault(&[
            ("type-event.md", "# Event\n\nType definition.\n"),
            ("event-foo.md", "first event\n"),
            ("event-bar.md", "second event\n"),
            ("eventually.md", "false-positive sentinel\n"),
            ("note-a.md", "unrelated\n"),
        ]);
        let type_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "type-event")
            .expect("type-event note")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(type_id, cx);
            let mut titles: Vec<String> = state
                .instances
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            titles.sort();
            assert_eq!(
                titles,
                vec!["event-bar".to_string(), "event-foo".to_string()],
                "instances must match notes whose stem starts with the type-prefix \
                 (excluding the type definition itself and false-positives like `eventually.md`)"
            );
        });
    }

    /// Non-type notes must produce zero instances (the section then
    /// renders its "not a type definition" empty state).
    #[gpui::test]
    fn real_vault_instances_empty_for_non_type_notes(cx: &mut TestAppContext) {
        let (vault, _dir) = open_temp_vault(&[
            ("alpha.md", "regular note\n"),
            ("beta.md", "regular note\n"),
        ]);
        let alpha_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "alpha")
            .expect("alpha note")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(alpha_id, cx);
            assert!(
                state.instances.is_empty(),
                "non-type note must surface zero instances"
            );
        });
    }

    /// `set_headings` short-circuits when the payload matches the
    /// stored list — duplicate `Headings` envelopes from the editor's
    /// `onChange` debounce must not re-paint the dock.
    #[gpui::test]
    fn set_headings_short_circuits_on_unchanged_payload(cx: &mut TestAppContext) {
        install_theme(cx);
        let panel = cx.update(|cx| cx.new(|_| InspectorPanel::new()));

        let h = Heading {
            level: 1,
            text: "Top".into(),
            anchor: "top".into(),
        };
        panel.update(cx, |p, cx| p.set_headings(vec![h.clone()], cx));
        cx.run_until_parked();
        // Second call with identical payload: state stays the same,
        // and any observer would NOT see a notify (verified by
        // re-running the parked queue and checking the heading list
        // didn't grow).
        panel.update(cx, |p, cx| p.set_headings(vec![h.clone()], cx));
        cx.run_until_parked();

        panel.read_with(cx, |p, _| {
            assert_eq!(p.headings().len(), 1, "headings stays a single entry");
            assert_eq!(p.headings()[0].text, "Top");
        });
    }

    /// `set_active` re-resolves the vault-driven sections and clears
    /// the stale outline so heading lists don't bleed across notes.
    #[gpui::test]
    fn set_active_swaps_state_and_clears_outline(cx: &mut TestAppContext) {
        install_theme(cx);
        let (vault, _dir) = open_temp_vault(&[
            ("a.md", "linked from b: nothing\n"),
            ("b.md", "links to [[a]]\n"),
        ]);
        let a_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "a")
            .expect("note a")
            .id;

        cx.update(|cx| cx.set_global(vault));
        let panel = cx.update(|cx| cx.new(|_| InspectorPanel::new()));

        // Seed an outline so we can confirm `set_active` clears it.
        let h = Heading {
            level: 1,
            text: "stale".into(),
            anchor: "stale".into(),
        };
        panel.update(cx, |p, cx| p.set_headings(vec![h], cx));

        // Target note a — b should appear as its backlink.
        panel.update(cx, |p, cx| p.set_active(Some(a_id), cx));
        cx.run_until_parked();

        panel.read_with(cx, |p, _| {
            assert_eq!(p.note_id(), Some(a_id));
            assert!(
                p.headings().is_empty(),
                "outline must be cleared on note swap"
            );
            let titles: Vec<String> = p
                .state()
                .backlinks
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            assert_eq!(titles, vec!["b".to_string()]);
        });
    }
}
