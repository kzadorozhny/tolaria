#![forbid(unsafe_code)]
//! Inspector panel for the Tolaria right dock (ADR-0115 Phase 9 worklist 9.2.8).
//!
//! Shows contextual metadata for the active note in eight collapsible
//! accordion sections: Properties, Outline, Backlinks, Instances,
//! References, Relationships, Info, and GitHistory.  Worklist 9.2.13a
//! added the read-only Info section between Relationships and
//! GitHistory.
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

use chrono::{DateTime, Utc};
use editor_bridge::Heading;
use gpui::{
    div, px, AnyElement, App, Context, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Window,
};
use gpui_component::{tooltip::Tooltip, ActiveTheme, IconName, StyledExt as _};
use mock_fixtures::{MockCommit, MockGit, MockNote, MockVault};
use note_item::NOTE_TOOLBAR_HEIGHT_PT;
use vault::{FrontmatterValue, NoteId, Vault};
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
    /// Read-only file metadata for the active note (worklist 9.2.13a).
    /// Sits between Relationships and GitHistory to mirror the
    /// user-shared React reference: `Modified` / `Created` / `Words` /
    /// `Size`.  The first pass ships `Modified` + `Size` only — the
    /// other two depend on plumbing not yet on `vault::Note`.
    Info,
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
        Self::Info,
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
            Self::Info => "Info",
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
    /// Real-vault Properties (worklist 9.2.13a) — sorted, filtered
    /// frontmatter pairs ready for read-only rendering.  Internal keys
    /// (`_favorite`, `_organized`, `_favorite_index`) are filtered out
    /// at resolve time so the render path stays a pure projection.
    pub properties: Vec<(SharedString, FrontmatterValue)>,
    /// Real-vault Relationships (worklist 9.2.13a) — one entry per
    /// relationship key (`aliases`, `belongs-to`, `owner`,
    /// `related-to`, `has`, `parent`, `child`) whose value parses to
    /// at least one `[[wikilink]]` target.  Each entry carries the
    /// raw key (lower-cased) and the ordered list of target stems.
    pub relationships: Vec<(SharedString, Vec<SharedString>)>,
    /// Inverse relationships (worklist 9.2.13a) — every note in
    /// `vault.backlinks(active_id)` whose frontmatter declares a
    /// relationship key targeting the active note.  Sub-scope 9.2.13a
    /// ships a single combined group; per-key inverse splitting is
    /// deferred to 9.2.13e.
    pub inverse_relationships: Vec<NoteRow>,
    /// Real-vault `Modified` timestamp for the Info section.  `None`
    /// when no note is active or when the panel is reading from the
    /// mock-only path.
    pub modified: Option<DateTime<Utc>>,
    /// Real-vault `byte_size` for the Info section.  `None` when no
    /// note is active.
    pub byte_size: Option<u64>,
}

impl InspectorState {
    /// Empty state — no note selected.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build resolved state for `active_id` from the real [`Vault`].
    ///
    /// Resolves Backlinks / Instances / References via the vault's
    /// query APIs, plus Properties / Relationships / Info from the
    /// active note's frontmatter + metadata (worklist 9.2.13a).
    pub fn resolve_from_vault(active_id: NoteId, vault: &Vault) -> Self {
        let Some(active) = vault.note_sync(active_id) else {
            return Self::empty();
        };

        let backlink_ids = vault.backlinks(active_id);
        let backlinks = note_rows_from_ids(vault, backlink_ids.clone());
        let references = note_rows_from_ids(vault, vault.outbound_links(active_id));
        let instances = resolve_type_instances(vault, active);

        let properties = collect_properties(active);
        let relationships = collect_relationships(active);
        let inverse_relationships = collect_inverse_relationships(vault, active, &backlink_ids);

        Self {
            note: None,
            backlinks,
            instances,
            references,
            mock_outbound_links: Vec::new(),
            properties,
            relationships,
            inverse_relationships,
            modified: Some(active.modified),
            byte_size: Some(active.byte_size),
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

        let modified = Some(active.modified);
        // `usize → u64` is lossless on every Tolaria target (no
        // 128-bit pointer hardware in scope); `try_from` keeps the
        // conversion clippy-clean under `-D warnings` without a
        // bespoke `#[allow(clippy::cast_possible_truncation)]`.
        let byte_size = u64::try_from(active.content.len()).ok();

        Self {
            note: Some(active.clone()),
            backlinks,
            instances,
            references,
            mock_outbound_links,
            // Mock path stays empty for the real-vault sections — the
            // existing fixture-driven render still falls back to the
            // legacy `note.properties` rendering for Properties /
            // Relationships, kept as regression cover.
            properties: Vec::new(),
            relationships: Vec::new(),
            inverse_relationships: Vec::new(),
            modified,
            byte_size,
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

// ---------------------------------------------------------------------------
// Properties / Relationships resolvers (worklist 9.2.13a)
// ---------------------------------------------------------------------------

/// Internal frontmatter keys that drive chrome state (favorite, organized)
/// rather than user-visible properties.  Filtered out of the Properties
/// list so they don't appear as rows.  Keep this list in sync with the
/// `Frontmatter::favorite` / `Frontmatter::organized` accessors and the
/// `_favorite_index` sort key added by worklist 9.2.1.
fn is_internal_key(key: &str) -> bool {
    matches!(key, "_favorite" | "_organized" | "_favorite_index")
}

/// Frontmatter keys whose value is interpreted as a list of `[[wikilink]]`
/// targets and surfaced under the Relationships section instead of
/// Properties.  Mirrors the React inspector's relationship-key set; both
/// the dash and space spellings (`belongs-to` vs `belongs to`) are
/// accepted because real vaults use both.
fn is_relationship_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "aliases"
            | "belongs-to"
            | "belongs to"
            | "owner"
            | "related-to"
            | "related to"
            | "has"
            | "parent"
            | "child"
    )
}

/// Collect the Properties rows for the active note.  Filters out
/// internal keys (`_favorite`, `_organized`, `_favorite_index`) and
/// relationship keys (so they don't double up between sections).
/// Iteration is already sorted by [`Frontmatter::iter`] (BTreeMap).
fn collect_properties(active: &vault::Note) -> Vec<(SharedString, FrontmatterValue)> {
    active
        .frontmatter()
        .iter()
        .filter(|(key, _)| !is_internal_key(key) && !is_relationship_key(key))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Collect the Relationships rows for the active note.  For each
/// relationship-shaped frontmatter key, parse `[[wikilink]]` targets out
/// of the value (handling both `Text` and `List` shapes) and emit one
/// `(key, targets)` entry.  Keys with no parseable targets are dropped
/// so the render path doesn't show an empty group header.
fn collect_relationships(active: &vault::Note) -> Vec<(SharedString, Vec<SharedString>)> {
    active
        .frontmatter()
        .iter()
        .filter(|(key, _)| is_relationship_key(key))
        .filter_map(|(key, value)| {
            let targets = parse_relationship_targets(value);
            if targets.is_empty() {
                None
            } else {
                Some((key.clone(), targets))
            }
        })
        .collect()
}

/// Pull `[[wikilink]]` targets out of a relationship-shaped frontmatter
/// value.  Accepts either a single `Text` scalar containing one or
/// more wikilinks, or a `List` of `Text` scalars each containing one.
/// Returns the de-duplicated, order-preserving list of targets.
fn parse_relationship_targets(value: &FrontmatterValue) -> Vec<SharedString> {
    fn push_from_text(out: &mut Vec<SharedString>, text: &str) {
        for stem in scan_wikilinks(text) {
            let shared = SharedString::from(stem);
            if !out.contains(&shared) {
                out.push(shared);
            }
        }
    }

    let mut out = Vec::new();
    match value {
        FrontmatterValue::Text(s) => push_from_text(&mut out, s.as_ref()),
        FrontmatterValue::List(items) => {
            for item in items {
                if let FrontmatterValue::Text(s) = item {
                    push_from_text(&mut out, s.as_ref());
                }
            }
        }
        _ => {}
    }
    out
}

/// Build the combined `Referenced From` row list (worklist 9.2.13a).
///
/// Walks every note returned by `vault.backlinks(active_id)` and keeps
/// the ones whose frontmatter has a relationship key pointing at the
/// active note's title.  This narrows the body-text backlink set down
/// to genuine frontmatter inverse relations (e.g. `parent: [[A]]`
/// backed by note B surfaces on A's inspector).
///
/// Sub-scope 9.2.13a ships one combined group; per-key inverse
/// splitting (e.g. `← Has`) is deferred to 9.2.13e.
fn collect_inverse_relationships(
    vault: &Vault,
    active: &vault::Note,
    backlink_ids: &[NoteId],
) -> Vec<NoteRow> {
    let active_title = active.title.as_ref();
    let mut hits = Vec::new();
    for &id in backlink_ids {
        let Some(other) = vault.note_sync(id) else {
            continue;
        };
        if note_relationships_target(other, active_title) {
            hits.push(NoteRow {
                id,
                title: other.title.clone(),
            });
        }
    }
    hits
}

/// True iff `note`'s frontmatter declares any relationship-shaped key
/// whose parsed `[[wikilink]]` targets include `target_title`.  Used
/// to filter `vault.backlinks` down to frontmatter inverse relations.
fn note_relationships_target(note: &vault::Note, target_title: &str) -> bool {
    note.frontmatter()
        .iter()
        .filter(|(key, _)| is_relationship_key(key))
        .any(|(_, value)| {
            parse_relationship_targets(value)
                .iter()
                .any(|t| t.as_ref() == target_title)
        })
}

/// Format the lowercased, leading-underscore-stripped key the way the
/// user-shared reference shows it (e.g. `_favorite` → `favorite`,
/// `belongs-to` → `belongs-to`).  Used only by the render path; the
/// stored key keeps its raw shape.
fn display_property_key(key: &str) -> String {
    key.trim_start_matches('_').to_ascii_lowercase()
}

/// Format a relationship-key header the way the user-shared reference
/// shows it: dashes become spaces and the first character is title-
/// cased.  Example: `belongs-to` → `Belongs to`.
fn display_relationship_label(key: &str) -> String {
    let with_spaces = key.replace('-', " ");
    let mut out = String::with_capacity(with_spaces.len());
    let mut chars = with_spaces.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    out
}

/// Render a single [`FrontmatterValue`] as a one-line string for the
/// read-only Properties section.  `Text` / `Date` map to the inner
/// `SharedString`; `Number` formats as an integer when it has no
/// fractional part; `Bool` maps to literal `true` / `false`; `List`
/// joins the rendered items with `, ` so a `tags: [a, b]` shape reads
/// as `a, b`.
fn render_value_string(value: &FrontmatterValue) -> SharedString {
    match value {
        FrontmatterValue::Text(s) | FrontmatterValue::Date(s) => s.clone(),
        FrontmatterValue::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e16 {
                SharedString::from(format!("{n:.0}"))
            } else {
                SharedString::from(n.to_string())
            }
        }
        FrontmatterValue::Bool(b) => SharedString::from(if *b { "true" } else { "false" }),
        FrontmatterValue::List(items) => {
            // Build the comma-separated repr in one allocation rather
            // than materialising a `Vec<String>` first.  `Frontmatter`
            // flattens nested lists at parse time so the recursion
            // depth is at most one in practice.
            let mut out = String::new();
            for (ix, item) in items.iter().enumerate() {
                if ix > 0 {
                    out.push_str(", ");
                }
                out.push_str(render_value_string(item).as_ref());
            }
            SharedString::from(out)
        }
    }
}

/// Format a byte count as `N B`, `N KB`, or `N MB` so the Info section's
/// Size row reads the way the user-shared reference shows it (e.g.
/// `443 B`).  KB / MB use base-1024 since that's what every filesystem
/// reports (this is metadata, not network throughput).
fn humanize_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{} KB", bytes / KB)
    } else if bytes < GB {
        format!("{} MB", bytes / MB)
    } else {
        format!("{} GB", bytes / GB)
    }
}

/// Format a `DateTime<Utc>` the way the user-shared reference shows it:
/// `Mon D, YYYY` (e.g. `May 20, 2026`).  Used by the Info section's
/// `Modified` row.  `%-d` would drop the leading zero on Unix but is
/// non-portable; we strip it manually after the format call.
fn format_inspector_date(dt: &DateTime<Utc>) -> String {
    let raw = dt.format("%b %d, %Y").to_string();
    // Strip the leading zero on the day-of-month: `May 02, 2026` →
    // `May 2, 2026`.  Done after format() so the formatter stays
    // platform-portable.
    if let Some(month_end) = raw.find(' ') {
        let (month, rest) = raw.split_at(month_end);
        let rest = rest.trim_start();
        if let Some(stripped) = rest.strip_prefix('0') {
            return format!("{month} {stripped}");
        }
    }
    raw
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

/// Right-dock panel showing note metadata in eight collapsible accordion sections.
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

        // Worklist 9.2.13a — real-vault path.  When the resolver
        // populated `properties` from `vault::Note::frontmatter()`,
        // render those rows.  Internal + relationship-shaped keys are
        // already filtered out at collect time.  TODO(9.2.13b): wire
        // type-aware inline editors (date, status, URL, icon, wikilink)
        // and a `+ Add property` link (`9.2.13c`).
        if self.note_id.is_some() && !self.state.properties.is_empty() {
            return render_property_rows(&self.state.properties, muted);
        }

        // Mock-vault fallback path (regression cover for existing
        // fixture-driven tests).  Skips the same internal keys so a
        // mock that sets `_favorite: true` doesn't leak it into the
        // user-visible list.
        let Some(note) = &self.state.note else {
            return empty_body("No note selected.", muted);
        };

        let mut pairs: Vec<_> = note
            .properties
            .iter()
            .filter(|(k, _)| !is_internal_key(k))
            .collect();
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
                            .child(SharedString::from(display_property_key(key))),
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
        let muted = cx.theme().muted_foreground;

        if self.note_id.is_none() && self.state.note.is_none() {
            return empty_body("No note selected.", muted);
        }

        // Worklist 9.2.13a — real-vault path.  Renders one group per
        // relationship key parsed from the active note's frontmatter,
        // plus a single combined "Referenced From" group for the
        // inverse-relationship set.  TODO(9.2.13c): per-section `Add`
        // slot + `+ Add relationship` footer button.  TODO(9.2.13e):
        // split `Referenced From` into one group per inverse key
        // (e.g. `← Has`).
        let has_relationships =
            !self.state.relationships.is_empty() || !self.state.inverse_relationships.is_empty();
        if has_relationships {
            let mut children: Vec<AnyElement> =
                Vec::with_capacity(self.state.relationships.len() + 1);

            for (key, targets) in &self.state.relationships {
                children.push(render_relationship_group(
                    display_relationship_label(key),
                    targets,
                    muted,
                ));
            }

            if !self.state.inverse_relationships.is_empty() {
                children.push(render_inverse_relationship_group(
                    &self.state.inverse_relationships,
                    muted,
                ));
            }

            return div()
                .flex()
                .flex_col()
                .gap_2()
                .children(children)
                .into_any_element();
        }

        // Mock fallback (regression cover): legacy outbound-stems
        // rendering.  Kept so the seven-section mock test path still
        // produces something to render.
        if !self.state.mock_outbound_links.is_empty() {
            return div()
                .flex()
                .flex_col()
                .gap_1()
                .children(self.state.mock_outbound_links.iter().map(|stem| {
                    div()
                        .text_sm()
                        .child(SharedString::from(stem.clone()))
                        .into_any_element()
                }))
                .into_any_element();
        }

        empty_body("No relationships.", muted)
    }

    fn render_info_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.note_id.is_none() && self.state.note.is_none() {
            return empty_body("No note selected.", muted);
        }

        let mut rows: Vec<AnyElement> = Vec::with_capacity(2);

        if let Some(modified) = self.state.modified {
            rows.push(render_info_row(
                "Modified",
                SharedString::from(format_inspector_date(&modified)),
                muted,
            ));
        }

        // TODO(9.2.13a-created): wire `Created` once `vault::Note`
        // carries a `created` timestamp (or once we plumb
        // `fs::metadata().created()` through the rescan path).
        // TODO(9.2.13a-words): wire `Words` once `vault::Note` carries
        // a cached body length or once the editor pumps a word-count
        // bridge envelope.

        if let Some(byte_size) = self.state.byte_size {
            rows.push(render_info_row(
                "Size",
                SharedString::from(humanize_bytes(byte_size)),
                muted,
            ));
        }

        if rows.is_empty() {
            return empty_body("No info.", muted);
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(rows)
            .into_any_element()
    }

    fn render_git_history_body(&self, cx: &mut Context<Self>) -> AnyElement {
        // TODO(9.2.13d-git-history): replace the [`MockCommit`] feed
        // with a path-filtered query against `git_provider` (lands in
        // Phase 11).  Today the section renders the `MockGit::seeded`
        // commits when the mock global is installed and the empty
        // state otherwise — that matches the React inspector's
        // placeholder shape until the live provider lands.
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
/// content to display.  Pulled out so the eight section renderers all
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

/// Render the Properties section's row list (worklist 9.2.13a) — one
/// `key · value` row per `(SharedString, FrontmatterValue)` pair.  The
/// key is lowercased and stripped of a leading underscore at the
/// render site (via [`display_property_key`]) so the stored key shape
/// stays untouched for downstream writers.
fn render_property_rows(
    pairs: &[(SharedString, FrontmatterValue)],
    muted: gpui::Hsla,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .children(pairs.iter().map(|(key, value)| {
            let key_label = SharedString::from(display_property_key(key.as_ref()));
            let value_label = render_value_string(value);
            div()
                .flex()
                .flex_row()
                .gap_2()
                .text_sm()
                .child(div().text_color(muted).child(key_label))
                .child(value_label)
                .into_any_element()
        }))
        .into_any_element()
}

/// Render one relationship group (worklist 9.2.13a): a section header
/// row carrying the human-readable label followed by a horizontally-
/// flowing list of wikilink-target pills.  Targets stay as plain text
/// for the read-only pass — click-to-open + per-target affordances
/// are tracked alongside the React inspector's `Add` slot work in
/// `9.2.13c`.
fn render_relationship_group(
    label: impl Into<SharedString>,
    targets: &[SharedString],
    muted: gpui::Hsla,
) -> AnyElement {
    let header = div().text_sm().text_color(muted).child(label.into());

    // Each target is already a `SharedString`; cloning is a cheap
    // reference bump, no String round-trip.
    let pills = div().flex().flex_row().flex_wrap().gap_1().children(
        targets
            .iter()
            .map(|t| div().text_sm().child(t.clone()).into_any_element()),
    );

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(header)
        .child(pills)
        .into_any_element()
}

/// Render the combined `Referenced From` group (worklist 9.2.13a).
/// Mirrors [`render_relationship_group`]'s shape but the pills are
/// real [`NoteRow`]s so clicks emit [`InspectorOpenNoteEvent`].  The
/// per-row click handler is intentionally NOT wired in 9.2.13a — the
/// read-only pass keeps the surface inert.  Wired up in 9.2.13e when
/// the per-key inverse split lands.
fn render_inverse_relationship_group(rows: &[NoteRow], muted: gpui::Hsla) -> AnyElement {
    let header = div()
        .text_sm()
        .text_color(muted)
        .child(SharedString::from("Referenced From"));

    let pills = div().flex().flex_row().flex_wrap().gap_1().children(
        rows.iter()
            .map(|row| div().text_sm().child(row.title.clone()).into_any_element()),
    );

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(header)
        .child(pills)
        .into_any_element()
}

/// Render one row of the Info section (worklist 9.2.13a) — a muted
/// label on the left and the value on the right.  Mirrors the
/// Properties row shape so the two sections feel coherent.
fn render_info_row(label: &'static str, value: SharedString, muted: gpui::Hsla) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap_2()
        .text_sm()
        .child(div().text_color(muted).child(label))
        .child(value)
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
        // Worklist 9.3.2 Reopened — the initial closure (`d9766aa5`)
        // pinned this to the sidebar's 200-pt opening width per the
        // row spec "at least the default width of the sidebar", but
        // the user reported the panel reads as too narrow for the
        // property labels.  React's app defaults to **280pt** for the
        // inspector (`src/hooks/useLayoutPanels.ts:20` `inspector:
        // 280`); that's the muscle-memory width users carry over.
        // Pin to 280 directly here instead of inheriting the sidebar
        // constant — the two columns have different content density
        // (the sidebar holds tree rows; the inspector holds
        // property-value pairs that wrap at narrow widths), so they
        // shouldn't share one knob.
        px(workspace::workspace::WORKSPACE_RIGHT_DOCK_INITIAL_WIDTH_PT)
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
                    InspectorSection::Info => self.render_info_body(cx),
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

        // Worklist 9.3.3 + 9.3.4 — header strip pinned to the same
        // 52-pt baseline as the note toolbar (`NOTE_TOOLBAR_HEIGHT_PT`)
        // so the inspector header sits flush with the editor's
        // breadcrumb across the workspace.  Background + border match
        // the note toolbar's chrome (`theme.background` + `border_b_1`
        // in `theme.border`) so the two strips read as one continuous
        // strip across the row.  Layout: left cluster carries the
        // panel-right toggle that dispatches `ToggleInspector`,
        // centre carries the `Properties` label, right cluster
        // carries a close `X` that also dispatches `ToggleInspector`
        // (the action is a toggle — both clicks close the panel).
        let header_strip = render_header_strip(cx);

        let sections = div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .children(children);

        // Worklist 9.2.13 Reopened-3 closure — the outer container
        // MUST set `w_full()` alongside `h_full()`.  Without it, the
        // panel renders with zero width (a flex column with only
        // `h_full` collapses to content-width along the cross axis,
        // which is 0 when children also lack explicit widths).  The
        // dispatch chain attaches the panel correctly (verified by
        // the production stderr trace + the end-to-end test in
        // `tolaria/src/main.rs`), but a zero-width render is
        // visually indistinguishable from "the panel didn't open."
        // [`sidebar_panel::SidebarPanel::render`] sets both
        // (`crates/sidebar_panel/src/lib.rs:1395-1396`); inspector
        // must mirror that.
        div()
            .flex()
            .flex_col()
            .h_full()
            .w_full()
            .overflow_hidden()
            .bg(background)
            .text_color(foreground)
            .child(header_strip)
            .child(sections)
    }
}

/// Render the inspector panel's header strip (worklist 9.3.3 + 9.3.4).
///
/// The strip is `NOTE_TOOLBAR_HEIGHT_PT`-tall and carries three
/// children laid out left / centre / right:
///
/// - **Left** — a [`IconName::PanelRight`] glyph that dispatches
///   [`actions::ToggleInspector`] on click, closing the panel.  Mirrors
///   the React reference's in-panel toggle glyph; the title-bar's
///   new right-side toggle (worklist 9.3.5) is the closed-state
///   affordance, this one is the open-state affordance.
/// - **Centre** — the `Properties` label, painted in
///   `theme.foreground` at the same weight as the section headers
///   below it so the user reads the strip as the panel's title bar.
/// - **Right** — a [`IconName::Close`] glyph that also dispatches
///   [`actions::ToggleInspector`].  Both clicks close the panel since
///   the action is a toggle.
///
/// Dispatch routes through [`Window::dispatch_action`] (not
/// `App::dispatch_action`) for the same re-entrancy reason the
/// note-toolbar's cells route that way — the click closure runs
/// inside an active window update, so `App::dispatch_action` would
/// trip the `cx.windows.get_mut(id)?.take()?` re-entrancy guard and
/// silently swallow the dispatch via `.log_err()`.  See the
/// `note_toolbar.rs` neighbourhood-cell comment for the full story.
///
/// The helper takes `cx: &App` and resolves theme tokens itself rather
/// than accepting three same-typed [`Hsla`] parameters — keeping the
/// signature one-parameter eliminates the colour-swap footgun that
/// three positional `Hsla` values would invite at a future call site.
fn render_header_strip(cx: &App) -> AnyElement {
    let theme = cx.theme();
    let border_color = theme.border;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let cell_tint = gpui::hsla(0.0, 0.0, 0.5, 0.12);

    let icon = div()
        .id("inspector-panel-header-toggle")
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .w(px(24.0))
        .rounded_sm()
        .child(IconName::Info);

    let close_button = div()
        .id("inspector-panel-header-close")
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .w(px(24.0))
        .rounded_sm()
        .cursor_pointer()
        .text_color(muted)
        .hover(move |this| this.bg(cell_tint))
        .on_click(|_, window, cx| {
            window.dispatch_action(Box::new(actions::ToggleInspector), cx);
        })
        .tooltip(|window, cx| Tooltip::new("Close Inspector").build(window, cx))
        .child(IconName::Close);

    let title = div()
        .flex_1()
        .text_sm()
        .font_semibold()
        .text_color(foreground)
        .child(SharedString::new_static("Properties"));

    div()
        .id("inspector-panel-header")
        .flex()
        .flex_row()
        .items_center()
        .h(px(NOTE_TOOLBAR_HEIGHT_PT))
        .min_h(px(NOTE_TOOLBAR_HEIGHT_PT))
        .px(px(12.0))
        .gap(px(8.0))
        .border_b_1()
        .border_color(border_color)
        .child(icon)
        .child(title)
        .child(close_button)
        .into_any_element()
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
        extract_outline, format_inspector_date, humanize_bytes, is_internal_key,
        is_relationship_key, parse_relationship_targets, render_value_string, scan_wikilinks,
        InspectorPanel, InspectorSection, InspectorState,
    };
    use vault::FrontmatterValue;
    use workspace::panel::{DockPosition, Panel};

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// `InspectorSection::ALL` must contain exactly 8 sections after
    /// worklist 9.2.13a added `Info` between Relationships and
    /// GitHistory.
    #[gpui::test]
    fn all_returns_8_sections(_cx: &mut TestAppContext) {
        assert_eq!(InspectorSection::ALL.len(), 8);
    }

    /// A freshly-constructed panel must report `DockPosition::Right`.
    #[gpui::test]
    fn panel_position_is_right(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let panel = InspectorPanel::new();
            assert_eq!(panel.position(cx), DockPosition::Right);
        });
    }

    /// Worklist 9.3.2 Reopened — the inspector panel's `default_size`
    /// must track the workspace's right-dock initial width constant
    /// (280pt — wider than the sidebar's 200pt to accommodate
    /// property-value pair content density).  Pins the contract so a
    /// future tweak to either side's literal can't silently regress
    /// the inspector's opening width.
    #[gpui::test]
    fn default_size_matches_right_dock_initial_width(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let panel = InspectorPanel::new();
            let size = panel.default_size(cx);
            assert_eq!(
                size,
                gpui::px(workspace::workspace::WORKSPACE_RIGHT_DOCK_INITIAL_WIDTH_PT),
                "inspector panel default_size must match the workspace \
                 right-dock initial width (280pt, distinct from the \
                 sidebar's 200pt for content density)",
            );
        });
    }

    /// Worklist 9.3.3 — the panel header strip must build cleanly when
    /// invoked through the same `cx: &App` route the render path uses.
    /// The full render path is covered by `panel_renders_without_panic`
    /// (below); this pins the new helper so a future refactor that
    /// breaks the click closure chain or the icon child surfaces as a
    /// builder panic instead of a runtime-only regression.
    #[gpui::test]
    fn header_strip_helper_builds(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let _strip = super::render_header_strip(cx);
        });
    }

    /// Worklist 9.3.3 — render must not panic with the header strip in
    /// place.  Mirrors `title_bar::title_bar_renders` for the panel's
    /// composite render chain (header + sections column).
    #[gpui::test]
    fn panel_renders_without_panic(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| InspectorPanel::new());
        cx.run_until_parked();
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

    // -----------------------------------------------------------------------
    // Phase 9 worklist 9.2.13a — Properties / Relationships / Info
    // -----------------------------------------------------------------------

    /// Properties: 3-note vault, one note with `{type: Note, status: Done,
    /// _favorite: true}` frontmatter — Properties resolves to exactly the
    /// `type` and `status` rows (in sorted order) and filters
    /// `_favorite` out as an internal key.
    #[gpui::test]
    fn real_vault_properties_lists_frontmatter_minus_internal_keys(cx: &mut TestAppContext) {
        let body = "---\ntype: Note\nstatus: Done\n_favorite: true\n---\n\n# Hi\n";
        let (vault, _dir) = open_temp_vault(&[("alpha.md", body), ("beta.md", "\n")]);
        let alpha_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "alpha")
            .expect("alpha note")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(alpha_id, cx);

            let keys: Vec<String> = state
                .properties
                .iter()
                .map(|(k, _)| k.to_string())
                .collect();
            assert_eq!(
                keys,
                vec!["status".to_string(), "type".to_string()],
                "Properties must list status + type in sorted order and filter _favorite"
            );

            let values: Vec<String> = state
                .properties
                .iter()
                .map(|(_, v)| render_value_string(v).to_string())
                .collect();
            assert_eq!(values, vec!["Done".to_string(), "Note".to_string()]);
        });
    }

    /// Relationship-shaped keys (e.g. `belongs-to`) must be filtered
    /// out of the Properties row list so they don't double up between
    /// the Properties and Relationships sections.
    #[gpui::test]
    fn properties_filter_excludes_relationship_keys(cx: &mut TestAppContext) {
        let body = "---\ntype: Note\nbelongs-to: \"[[Q4 2024]]\"\n---\n\nbody\n";
        let (vault, _dir) = open_temp_vault(&[("alpha.md", body)]);
        let alpha_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "alpha")
            .expect("alpha")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(alpha_id, cx);

            let keys: Vec<String> = state
                .properties
                .iter()
                .map(|(k, _)| k.to_string())
                .collect();
            assert_eq!(
                keys,
                vec!["type".to_string()],
                "belongs-to must land in Relationships, not Properties"
            );
        });
    }

    /// Relationships: a note with `belongs-to: "[[Q4 2024]]"` and
    /// `owner: "[[Luca Rossi]]"` produces two relationship groups
    /// (`belongs-to` and `owner`), each with one parsed target.
    #[gpui::test]
    fn real_vault_relationships_parses_wikilink_targets(cx: &mut TestAppContext) {
        let body = "---\nbelongs-to: \"[[Q4 2024]]\"\nowner: \"[[Luca Rossi]]\"\n---\n\nbody\n";
        let (vault, _dir) = open_temp_vault(&[("project.md", body)]);
        let project_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "project")
            .expect("project")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(project_id, cx);

            let mut groups: Vec<(String, Vec<String>)> = state
                .relationships
                .iter()
                .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
                .collect();
            groups.sort_by(|a, b| a.0.cmp(&b.0));
            assert_eq!(
                groups,
                vec![
                    ("belongs-to".to_string(), vec!["Q4 2024".to_string()]),
                    ("owner".to_string(), vec!["Luca Rossi".to_string()]),
                ],
                "relationships must surface both groups with their wikilink targets"
            );
        });
    }

    /// A list-shaped `aliases: [[a]], [[b]]` value must parse both
    /// wikilink targets out of the YAML sequence.
    #[gpui::test]
    fn relationships_parse_list_of_wikilinks(_cx: &mut TestAppContext) {
        // List of strings, each containing one [[wikilink]].
        let value = FrontmatterValue::List(vec![
            FrontmatterValue::Text("[[Alpha]]".into()),
            FrontmatterValue::Text("[[Beta]]".into()),
        ]);
        let targets: Vec<String> = parse_relationship_targets(&value)
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(targets, vec!["Alpha".to_string(), "Beta".to_string()]);
    }

    /// Inverse relationships: a 2-note vault where note `child.md`
    /// has `parent: "[[parent]]"` — opening `parent.md` in the
    /// inspector must surface `child` under `inverse_relationships`.
    #[gpui::test]
    fn real_vault_inverse_relationships_lists_frontmatter_backlinks(cx: &mut TestAppContext) {
        let (vault, _dir) = open_temp_vault(&[
            ("parent.md", "---\ntype: Note\n---\n\n"),
            ("child.md", "---\nparent: \"[[parent]]\"\n---\n\n"),
        ]);
        let parent_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "parent")
            .expect("parent")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(parent_id, cx);

            let titles: Vec<String> = state
                .inverse_relationships
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            assert_eq!(
                titles,
                vec!["child".to_string()],
                "inverse_relationships must surface notes whose frontmatter parent: targets us"
            );
        });
    }

    /// Inverse relationships drop body-text-only backlinks: a note that
    /// links to the active one via `[[wikilink]]` in the body but does
    /// NOT declare a relationship-shaped frontmatter key must NOT
    /// appear in `inverse_relationships` (it stays in `backlinks`).
    #[gpui::test]
    fn inverse_relationships_drop_body_only_backlinks(cx: &mut TestAppContext) {
        let (vault, _dir) = open_temp_vault(&[
            ("parent.md", "---\ntype: Note\n---\n\nbody\n"),
            ("loose.md", "I link to [[parent]] from the body.\n"),
        ]);
        let parent_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "parent")
            .expect("parent")
            .id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(parent_id, cx);

            assert!(
                state.inverse_relationships.is_empty(),
                "body-text-only backlinks must NOT surface as inverse relationships"
            );
            // The body backlink itself still lands in `backlinks` —
            // that's the cover for the regular Backlinks section.
            let backlink_titles: Vec<String> = state
                .backlinks
                .iter()
                .map(|r| r.title.to_string())
                .collect();
            assert_eq!(backlink_titles, vec!["loose".to_string()]);
        });
    }

    /// Info section: a note with known frontmatter + body must drive
    /// `state.modified` and `state.byte_size` so the Info row formatter
    /// renders deterministic strings.  Modified shape: `May 20, 2026`.
    /// Size shape: under-1024 bytes render as `N B`.
    #[gpui::test]
    fn real_vault_info_state_carries_modified_and_byte_size(cx: &mut TestAppContext) {
        let body = "---\ntype: Note\n---\n\nhello\n";
        let (vault, _dir) = open_temp_vault(&[("alpha.md", body)]);
        let alpha = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "alpha")
            .expect("alpha")
            .clone();
        let expected_size = alpha.byte_size;
        let alpha_id = alpha.id;
        cx.update(|cx| {
            cx.set_global(vault);
            let state = InspectorState::resolve(alpha_id, cx);

            assert!(state.modified.is_some(), "Info must carry a modified time");
            assert_eq!(
                state.byte_size,
                Some(expected_size),
                "Info must carry the on-disk byte size"
            );
        });
    }

    /// `humanize_bytes` matches the user-shared reference's `443 B` /
    /// `2 KB` / `5 MB` shape.  Base-1024 (the unit reported by every
    /// filesystem).
    #[gpui::test]
    fn humanize_bytes_renders_b_kb_mb(_cx: &mut TestAppContext) {
        assert_eq!(humanize_bytes(0), "0 B");
        assert_eq!(humanize_bytes(443), "443 B");
        assert_eq!(humanize_bytes(1023), "1023 B");
        assert_eq!(humanize_bytes(1024), "1 KB");
        assert_eq!(humanize_bytes(2048), "2 KB");
        assert_eq!(humanize_bytes(1024 * 1024), "1 MB");
        assert_eq!(humanize_bytes(5 * 1024 * 1024), "5 MB");
        assert_eq!(humanize_bytes(1024 * 1024 * 1024), "1 GB");
    }

    /// `format_inspector_date` produces the `May 2, 2026` shape — no
    /// leading zero on the day-of-month, matching the user-shared
    /// reference.
    #[gpui::test]
    fn format_inspector_date_strips_day_leading_zero(_cx: &mut TestAppContext) {
        use chrono::TimeZone as _;
        let dt = chrono::Utc.with_ymd_and_hms(2026, 5, 2, 12, 0, 0).unwrap();
        assert_eq!(format_inspector_date(&dt), "May 2, 2026");
        let dt = chrono::Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).unwrap();
        assert_eq!(format_inspector_date(&dt), "May 20, 2026");
    }

    /// `is_internal_key` covers the three flags that drive chrome
    /// state (favorite, organized, favorite_index sort key) and rejects
    /// everything else.
    #[gpui::test]
    fn is_internal_key_recognises_chrome_flags(_cx: &mut TestAppContext) {
        assert!(is_internal_key("_favorite"));
        assert!(is_internal_key("_organized"));
        assert!(is_internal_key("_favorite_index"));
        assert!(!is_internal_key("type"));
        assert!(!is_internal_key("status"));
        assert!(!is_internal_key("_unknown"));
    }

    /// `is_relationship_key` recognises both the dash and space
    /// spellings (`belongs-to` vs `belongs to`) and stays case-
    /// insensitive.
    #[gpui::test]
    fn is_relationship_key_accepts_dash_and_space_spellings(_cx: &mut TestAppContext) {
        assert!(is_relationship_key("aliases"));
        assert!(is_relationship_key("belongs-to"));
        assert!(is_relationship_key("belongs to"));
        assert!(is_relationship_key("Belongs-To"));
        assert!(is_relationship_key("owner"));
        assert!(is_relationship_key("related-to"));
        assert!(is_relationship_key("related to"));
        assert!(is_relationship_key("has"));
        assert!(is_relationship_key("parent"));
        assert!(is_relationship_key("child"));
        assert!(!is_relationship_key("type"));
        assert!(!is_relationship_key("status"));
    }

    /// `render_value_string` walks every variant — `Text` / `Date` /
    /// `Bool` / `Number` (integer + float) / `List` — and matches the
    /// shape the Properties section uses.
    #[gpui::test]
    fn render_value_string_handles_every_variant(_cx: &mut TestAppContext) {
        assert_eq!(
            render_value_string(&FrontmatterValue::Text("hi".into())).as_ref(),
            "hi"
        );
        assert_eq!(
            render_value_string(&FrontmatterValue::Date("2026-05-21".into())).as_ref(),
            "2026-05-21"
        );
        assert_eq!(
            render_value_string(&FrontmatterValue::Bool(true)).as_ref(),
            "true"
        );
        assert_eq!(
            render_value_string(&FrontmatterValue::Bool(false)).as_ref(),
            "false"
        );
        assert_eq!(
            render_value_string(&FrontmatterValue::Number(42.0)).as_ref(),
            "42"
        );
        assert_eq!(
            render_value_string(&FrontmatterValue::Number(1.5)).as_ref(),
            "1.5"
        );
        assert_eq!(
            render_value_string(&FrontmatterValue::List(vec![
                FrontmatterValue::Text("a".into()),
                FrontmatterValue::Text("b".into()),
            ]))
            .as_ref(),
            "a, b"
        );
    }
}
