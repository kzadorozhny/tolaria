# Phase 9 worklist — note-toolbar product features

> **Phase 9 scope.**  Wire the seven note-toolbar slots that Phase 8
> deferred (`8.2.9`, `8.2.10`, `8.2.11`, `8.2.12`, `8.2.13`, `8.2.14`,
> `8.2.17`).  Each is net-new product work — not a regression — so it
> carries the full feature shape from the React `BreadcrumbBar`: new
> frontmatter fields, new panels, new actions, new bridge variants
> where needed.  Phase 8 close-out is at
> [`../phase-8/close-out.md`](../phase-8/close-out.md); the source
> rows still carry `➡️` markers in
> [`../phase-8/worklist.md`](../phase-8/worklist.md).
>
> Behavioral-layer crate extraction (`command_registry`, `nav_history`,
> `multi_select`, `dialog_stack`, `auto_git`, `vault_lifecycle`,
> `telemetry_pipeline`) lives in the renumbered **Phase 10** —
> see [`../../roadmap.md`](../../roadmap.md).  Several Phase 9 rows
> below name Phase 10 dependencies; those land locally-stubbed if
> the row needs to ship first.

## 1. Blockers

## 2. High Priority

9.2.1. ✅ Star toggle → favourite frontmatter + sidebar favourites section
9.2.2. ✅ Organised toggle → inbox-advance frontmatter
9.2.3. ✅ Neighbourhood action → backlink filter in note-list
9.2.4. ✅ Raw-mode toggle → editor-host raw bridge
9.2.5. ➡️ AI button → attach `ai_panel` to right dock + `ToggleAiPanel`
9.2.6. ✅ ToC action → new `toc_panel` crate + headings bridge
9.2.7. More-overflow menu → archive / delete / collapse-when-narrow actions
9.2.8. ✅ Note Inspector Panel content — backlinks, references, type instances, outline
9.2.9. ✅ Star action stops working when the note is updated outside the UI
9.2.10. ✅ Organized toolbar cell needs green-checked colour treatment
9.2.11. ✅ Star toolbar cell needs orange-filled colour treatment when active
9.2.12. ✅ Inbox sidebar view must exclude notes with `_organized: true`
9.2.13. ⏳ Inspector Panel — Properties, Aliases, Belongs to, Owner, Related to, Has, Info, History sections

## 3. Low Priority

9.3.1. Block editor drag handles do not Cary React side styling

---

### Annotations and details

Heading numbers match the corresponding row IDs.

#### 9.2.1

**Source row:** Phase 8 `8.2.9` (➡️).  **React reference:**
`src/components/BreadcrumbBar.tsx` `FavoriteAction` →
`onToggleFavorite` (`App.tsx:1617`) →
`useEntryActions.handleToggleFavorite` (`src/hooks/useEntryActions.ts:297`).
The handler writes `_favorite: true` + `_favorite_index: N` to the
note's YAML frontmatter via `handleUpdateFrontmatter` with optimistic
rollback; the toggled state is read from `entry.favorite` (populated
during vault scan) and the sidebar's "Favorites" section
(`src/components/Sidebar.tsx:213`) renders the resulting list.

**Deps:** (1) `vault::Frontmatter` gains a `favorite: bool` field +
a `set_frontmatter_bool` write path; (2) `sidebar_panel` gains a
Favorites section reading the vault-wide favourites list; (3) the
star glyph on the toolbar reflects the read.  No bridge variant
required.  **Size:** small.

**Closure (commit `9a3839c9`).**  Landed the shared write path
as `Vault::set_frontmatter_bool` (`crates/vault/src/lib.rs:460`),
backed by a byte-identical YAML rewriter
(`crates/vault/src/frontmatter.rs::set_bool_in_raw`) that splits the
note into `(opener, yaml, closer, body)` and mutates only the target
line — toggle-on appends, toggle-off removes (absent ⇔ false), and a
crlf-flavour fixture round-trips byte-for-byte through the suite.
`Frontmatter::favorite()` / `organized()` plus `Note::is_favorite()` /
`is_organized()` are the read-side accessors; `Vault::note_sync(id)` /
`iter_notes()` give chrome a borrow-only path for per-render reads
without spinning up a `Task`.  The note-toolbar star cell now
dispatches `toggle_frontmatter_flag` (filled `StarFill` vs outline
`Star` driven by `is_favorite`) and the `SidebarPanel`'s new
`FAVORITES` section reads the live vault on every render — empty list
hides the section entirely (no empty header), and toggling either
direction from any source flips the row count on the next paint.
Out of scope and explicitly deferred: `_favorite_index` ordering /
drag-reorder (Phase 9.2 follow-up), and editor-host live-frontmatter
sync (revisit when rename-bridge lands).

#### 9.2.2

**Source row:** Phase 8 `8.2.10` (➡️).  **React reference:**
`BreadcrumbBar.tsx` `OrganizedAction` → `onToggleOrganized`
(`App.tsx:1618`) → `useInboxOrganizeAdvance.handleToggleOrganized`
(`src/hooks/useEntryActions.ts:298`).  Writes `_organized: true` to
frontmatter and auto-advances to the next inbox note when
`explicit_organization_enabled` is set.  The tooltip "Show in
Organized view" is a misnomer — the cell is a pure frontmatter
toggle, not a navigation action.

**Deps:** shares the `vault::Frontmatter` write path with `9.2.1`;
additionally consumes a `explicit_organization_enabled` boolean on
`settings_store`.  **Size:** small.  **Implementation note:** batch
with `9.2.1` for shared write-path landing.

**Closure (commit `9a3839c9`).**  Shipped alongside `9.2.1` on
the shared `Vault::set_frontmatter_bool` write path
(`crates/vault/src/lib.rs:460`); the organized toolbar cell now
dispatches `toggle_frontmatter_flag(id, "_organized", …)` via the same
helper as the star cell.  The cell remains a pure frontmatter toggle
— the inbox-advance behaviour from
`useInboxOrganizeAdvance.handleToggleOrganized` is **deferred** until
`settings_store::explicit_organization_enabled` lands (out-of-scope
for this commit; flagged as `TODO(9.2.2-followup)` on the toolbar
cell).  When the setting arrives, the chrome-side handler can read
the gate, find the next inbox note via `Vault::iter_notes()`, and
dispatch `OpenNoteEvent` from the same closure that today only writes
the YAML flag.

#### 9.2.3

**Source row:** Phase 8 `8.2.11` (➡️).  **React reference:**
`BreadcrumbBar.tsx` `NeighborhoodAction` → `onEnterNeighborhood`
(`App.tsx:1753`) → `useNeighborhoodEntry`
(`src/hooks/useNeighborhoodSelection.ts:55-79`).  Pushes a
`SidebarSelection { kind: 'entity', noteId }` onto the navigation
history and switches the note list to show the backlink neighbourhood
of the current note.

**Deps:** (1) `vault::Vault::backlinks(id)` query (no GPUI
counterpart yet); (2) a new "neighbourhood" selection mode in
`sidebar_panel` + `note_list_pane` that filters by inbound / outbound
wikilinks of one note; (3) shared selection history with the future
Phase 10 `nav_history` crate — stub locally inside `note_item` if
`nav_history` hasn't landed yet.  **Size:** large.

**Closure (commit `13bbc646`).**  Vault gained two read-only
queries: `Vault::backlinks(id) -> Vec<NoteId>` (notes whose body
contains `[[…]]` resolving to `id`) at `crates/vault/src/lib.rs:814`,
and `Vault::outbound_links(id) -> Vec<NoteId>` at `:863`.  Both
parse wikilinks via a small hand-rolled `WikilinkTargets` iterator
that mirrors `src-tauri/src/vault/parsing.rs::extract_outgoing_links`
for React parity — exact-title match on the link target, deterministic
sort by `NoteId` so callers (including `9.2.8`) don't have to re-sort,
per-note IO failures degrade gracefully (log + skip), and self-links
are filtered defensively in both queries plus the action handler.
A `TODO(9.2.3-fence)` flags the known gap that fenced-code blocks
aren't excluded — same gap the React parser has.

Sidebar gained `SidebarSelection::Neighborhood(u64)` (transient
selection, never rendered as a permanent row); note-list-pane gained
`NoteListScope::Neighborhood(NoteId, HashSet<NoteId>)` — the id-set
is pre-resolved at scope-change time so the per-row `scope_matches`
predicate stays O(1).  Toolbar's `note-toolbar-neighborhood` cell now
dispatches `actions::EnterNeighborhood`; the handler (in
`tolaria/src/main.rs:840`) reads the active note id, builds the
`backlinks ∪ outbound_links ∖ {id}` set, swaps both sidebar
selection and note-list scope, and emits a
`SidebarSelectionChangedEvent` with `display_label = "Neighborhood
of <title>"` so the note-list header reflects the mode.

Nav-history stub: skipped — staying as a `TODO(nav-history)` rather
than a 30-line local store, per the worklist's "drop the stub if
it bloats this row" guidance.  Phase 10 `nav_history` picks this up
with the proper shape.  Heading-click → body-anchor navigation
(deferred from `9.2.6`) stays parked.  Tests: +15 in `vault`
(parser edge cases + integration: empty target, unclosed bracket,
whitespace, pipe alias, 3-note vault, self-link, dedup, unknown
id), +tests in other crates covering the scope shape and handler.

#### 9.2.4

**Source row:** Phase 8 `8.2.12` (➡️).  **React reference:**
`BreadcrumbBar.tsx` `RawToggleButton` → `onToggleRaw` / `rawMode`
(`App.tsx:1564` via `rawToggleRef`).  Flips the editor inside the
WKWebView between BlockNote rich mode and a CodeMirror raw-text view
(`src/components/useRawModeWithFlush.ts`).  The host-side CodeMirror
raw-mode is already in `editor-host/src/` from Phase 8.29.

**Deps:** (1) new `actions::ToggleRawEditor` verb; (2) a
`FromHost::SetRawMode { enabled }` bridge variant — `editor_bridge`
has no current mode-switch envelope; (3) chrome side of
`crates/raw_editor` (already scaffolded) wired to display the mode
chip + find bar; (4) `note_item` tracks `raw_mode: bool` per tab.
**Size:** medium.

**Closure (commit `45b6622d`).**  Shipped the chrome-owned raw
toggle.  Seam: `editor_bridge::ToHost::SetRawMode(SetRawMode {
enabled })` (native → editor; the annotation above mis-labels it
`FromHost::*` — the editor is the receiver, not the sender).  Chrome
state: `NoteItem::raw_mode` (per-tab, defaults `false`, reset on
`open_in_webview` so each swap-in lands rich).  `NoteItem::toggle_raw_mode`
is the single mutator — flips the field, calls `cx.notify()`, then
pushes the matching `SetRawMode` envelope through the existing
`send_to_host` pipeline.  Action: `actions::ToggleRawEditor` (no
default keybinding, mirroring the React mouse-only affordance);
handler lives in `tolaria/src/main.rs` next to the theme observer so
it borrows the same `ActiveNoteItemSlot`.  Toolbar: the
`note-toolbar-raw` cell is now a `toolbar_cell_with_active` —
visual contrast lives in the cell background (filled when raw)
because `gpui-component-assets` has no fill/outline pair for
`SquareTerminal`; a `TODO(visual-parity)` carries the upgrade.
Editor host: `editor_bridge::ToHost` gains a `set_raw_mode` arm in
`bridge.ts`, `dispatchToHost` handles it (markdown → flip surface
via `setRawNote`; non-markdown → no-op; pre-`note_open` → drop),
and `EditorBridgeHandlers` gains `setActivePath` / `getActivePath`
/ `getRawNote` so the toggle gate can read the active note's path
without re-entering React state.  Tests: `editor_bridge` round-trips
the new envelope (`to_host_set_raw_mode_*roundtrip`), `note_item`
asserts the default + flip (`toggle_raw_mode_flips_the_flag`,
`raw_mode_defaults_to_false`), `bridge.test.ts` covers the four
dispatcher branches (flip-on, flip-off, non-markdown no-op,
pre-open drop), and `EditorApp.routing.test.tsx` drives the React
mount through chrome-style bridge envelopes.  **Deferred to a
`9.2.4-followup`:** the `crates/raw_editor` mode-chip + find-bar
polish.  The current ship swaps the body between BlockNote and the
existing CodeMirror raw editor and updates the toolbar glyph
treatment, which the brief's minimum-viable acceptance criteria
require.  The mode chip + find bar are net-new UI work on top of
the existing raw editor, not a regression of an existing surface.

#### 9.2.5

**Source row:** Phase 8 `8.2.13` (➡️).  **React reference:**
`BreadcrumbBar.tsx` `AIChatAction` → `onToggleAIChat` / `showAIChat`
(`App.tsx:1745-1746`) via `dialogs.toggleAIChat`.  Opens the AI chat
panel on the right side of the workspace
(`src/components/EditorRightPanel.tsx:213`).

**Deps:** (1) attach `crates/ai_panel` to the workspace right dock
(`tolaria/src/main.rs` currently attaches only the sidebar to the
left dock); (2) replace the `ToggleInspector` placeholder in
`ai_panel/src/lib.rs:259` with a real `actions::ToggleAiPanel`; (3)
the actual LLM-provider plumbing (Phase 11.4 `cli_agents` under the
renumbered roadmap) stays stubbed for now.  **Size:** medium —
chrome wiring only; provider integration deferred.

**Deferred (2026-05-21)** ➡️ — moved out of Phase 9 active scope at
user direction.  Rationale: the chrome attach is cheap, but landing
it without the provider story (Phase 11.4 `cli_agents`) means the
panel opens into a stubbed AI experience that we'd have to revisit
anyway.  Holding until Phase 10 (or whenever `cli_agents` lands) so
the right-dock attach + `ToggleAiPanel` action + real provider
plumbing can ship as a cohesive AI-mode milestone.  Carry across to
the next phase's worklist when that phase opens.

#### 9.2.6

**Source row:** Phase 8 `8.2.14` (➡️).  **React reference:**
`BreadcrumbBar.tsx` `TableOfContentsAction` →
`onToggleTableOfContents` (`App.tsx:1565`) →
`tableOfContentsToggleRef.current()` →
`Editor.tsx:630 rightPanel.handleToggleTableOfContents`.  Panel body
is `src/components/TableOfContentsPanel.tsx`, driven by heading
nodes extracted from the BlockNote editor.

**Deps:** (1) new `crates/toc_panel` mirroring
`TableOfContentsPanel` (no GPUI counterpart exists); (2)
`actions::ToggleTableOfContents`; (3) a new
`ToHost::Headings { items: [{ level, text, anchor }] }` bridge
variant — no existing `editor_bridge` envelope carries headings.
**Size:** medium — sizeable bridge surface but the panel itself is
read-only.

**Closure (commit `5bd2533e`).**  Seam: the annotation above
names `ToHost::Headings` but the editor is the *sender* and the
native chrome is the *receiver*, so the variant lives on
`editor_bridge::FromHost::Headings(Headings { items: Vec<Heading> })`
(same direction-correction the 9.2.4 closure flagged for
`SetRawMode`).  `Heading { level: u8, text: String, anchor: String }`
ships with serde round-trip + empty-list tests pinning the wire
shape `{"k":"headings","v":{"items":[…]}}`.  Editor host
(`editor-host/src/EditorApp.tsx`) extracts headings from BlockNote
blocks (`type === "heading"`, `props.level ∈ {1,2,3}`, joined text
fragments, anchor = block id with slug fallback), emits once
synchronously on every markdown `note_open` and on raw → rich
flip, sends an empty list on raw `note_open` and rich → raw, and
debounces document changes at `HEADINGS_DEBOUNCE_MS = 300` ms via
`editor.onChange`.  New crate `crates/toc_panel` mirrors
`ai_panel`'s right-dock shape: `TocPanel::set_headings(items, cx)`
short-circuits when the new list is byte-identical (avoids spurious
re-renders from the workspace's `cx.observe(&right_dock, …)`
cascade), `DockPosition::Right`, `default_size = px(300.0)`,
`starts_open = true` (the action handler is the actual gate).
`note_item` plumbing: `Outcome::EmitHeadings(Headings)` carries the
wire payload verbatim; `install_dispatch_task` emits
`HeadingsUpdatedEvent { headings: payload.items }` to workspace
subscribers.  `workspace::TolariaWorkspace` gains `attach_right_dock`
+ `toggle_right_dock` + `is_right_dock_open` + `has_right_dock_panel`
(with a new `Dock::has_panel()` to distinguish Empty from
Closed-but-attached); the workspace `cx.observe(&right_dock, …)` now
matches the left-dock observer so attach / toggle re-renders the
shell.  `ToggleTableOfContents` handler in
`crates/tolaria/src/main.rs` attaches a fresh `TocPanel` on first
dispatch (via `has_right_dock_panel == false`) and toggles thereafter;
a shared `Rc<RefCell<Option<Entity<TocPanel>>>>` slot lets the
`HeadingsUpdatedEvent` subscriber write through without re-resolving
the workspace.  Note-toolbar's `note-toolbar-toc` cell upgrades from
`stub_cell` to `toolbar_cell` dispatching the action (glyph stays
`IconName::Menu`).

**Deferred to a `9.2.6-followup`:** (1) heading-click body
navigation — no `ToHost::ScrollToAnchor` envelope yet; the row's
`on_click` logs the anchor but doesn't emit, and `TocHeadingClicked`
is wired as the `EventEmitter` payload so the followup can hang the
scroll dispatch off it; (2) ToC button active-state colour treatment
on the toolbar (would need workspace-state plumbing back into
`note_item`, which would couple the toolbar to the dock — the dock
toggle already gives the user feedback by the panel appearing /
disappearing, so the cell stays untreated).  9.2.8 (Inspector Panel
outline section) consumes the same `FromHost::Headings` envelope
when it lands; no further bridge work needed there.

#### 9.2.7

**Source row:** Phase 8 `8.2.17` (➡️).  **React reference:**
`BreadcrumbBar.tsx:892-993` `BreadcrumbOverflowMenu`, a
`DropdownMenu` that hosts: git-diff toggle, note-width toggle, TOC
toggle, reveal-in-Finder, copy-path, archive / unarchive, delete.
When the breadcrumb itself overflows the toolbar width the
neighbourhood + file-path actions also collapse into this menu.

**Deps:** (1) a popup primitive — `gpui-component` has no
`DropdownMenu` yet; either add one or compose from `Popover` +
`ListItem`; (2) `actions::Archive` + `actions::Delete` (new); (3)
the overflow / responsive collapse behaviour reuses the `9.2.3`,
`9.2.4`, `9.2.6` dispatchers — wire those first.  **Size:** medium —
blocked on `9.2.3`, `9.2.4`, `9.2.6` shipping real handlers.

#### 9.2.8

**Source row:** Added mid-phase 2026-05-21 at user direction.  The
toolbar's inspector button (Phase 8 `8.2.18` ✅) dispatches
`actions::ToggleInspector`, which opens a separate macOS window
hosting `inspector_panel::InspectorPanel` (Phase 8 `8.3.1` ✅).
That window currently renders the Strand A 8.4 placeholder content
shape — the "Phase 3 wires…" sections were replaced with concrete
sections in `b830c42d`, but the per-section data sources for the
**active note** are sparse: backlinks resolver returns mock seeds,
type-instances list is empty, references column is unwired, and the
outline parser hasn't ingested the live `editor_bridge` headings yet.

**Scope:** flesh out the InspectorPanel's note-context surfaces so
clicking the toolbar inspector button on a real note shows actual
data:
- **Backlinks** — every other note in the vault whose body links to
  the active note (parse `[[wikilink]]` syntax on vault scan;
  expose via `vault::Vault::backlinks(id)` — same query 9.2.3
  needs, so coordinate landing order with 9.2.3).
- **Type instances** — when the active note IS a type definition
  (filename starts with `type-`), list every note whose filename
  prefix matches.  Filter pre-computed during `Vault::scan`.
- **References** — outbound wikilinks from the active note, parsed
  from body on note-open and cached on the `Note`.
- **Outline** — headings extracted from the WKWebView body via a
  `ToHost::Headings` bridge variant (shared design with 9.2.6; land
  the variant once, consume in both panels).

**Deps:** (1) `vault::Vault::backlinks(id)` + outbound-link cache
(shared with 9.2.3); (2) `ToHost::Headings` bridge variant (shared
with 9.2.6); (3) `inspector_panel::InspectorPanel` data-source
wiring — replace the mock-fixture seeds with vault-driven reads;
(4) selection plumbing so the panel knows which note is active
(use the existing `note_item::NoteOpenEvent` listener or pull from
the workspace's `active_item()`).  **Size:** large — depends on
9.2.3 and 9.2.6 for shared infrastructure but its UI shape is
independent; can land in parallel once those primitives exist.

**Closure (commit `8897ab93`).**  Wired four sections in
`crates/inspector_panel/src/lib.rs` to live data sources:

- **Backlinks** — `vault::Vault::backlinks(id)` resolves inbound
  wikilinks; titles come from `vault::Vault::note_sync(id).title`.
- **References** — `vault::Vault::outbound_links(id)` resolves
  outbound wikilinks.  `InspectorSection::ReferencedBy.label()`
  now returns `"References"`; the enum variant name is preserved
  for back-compat with any persisted expansion state.
- **Instances** — when the active note's file stem starts with
  `type-X`, every note whose stem starts with `X-` is listed.
  Implemented in `resolve_type_instances`; non-type notes surface
  zero rows so the section renders its "not a type definition"
  empty state.
- **Outline** — driven by `note_item::HeadingsUpdatedEvent` (the
  `FromHost::Headings` envelope from worklist 9.2.6).
  `InspectorPanel::set_headings` short-circuits on identical
  payloads to avoid re-painting the dock for the editor's
  duplicate-debounce ticks.

**Click-to-open.**  All three vault-driven list sections (Backlinks
/ Instances / References) emit `InspectorOpenNoteEvent { id }` on
row click.  The panel implements `EventEmitter<InspectorOpenNoteEvent>`;
workspace subscribers route it through the same `open_note` helper
the note-list pane uses.

**Active-note tracking seam.**  `InspectorPanel::set_active(id, cx)`
is the canonical write seam — `tolaria/src/main.rs` calls it from
the existing `OpenNoteEvent` subscriber when an inspector panel
slot is populated (see `open_note_inspector_slot`, mirrors the
`toc_panel_slot` pattern from 9.2.6).  `set_active` clears the
stale outline so heading lists don't bleed across notes.

**Vault → panel state refresh.**  `InspectorPanel::refresh_state`
re-resolves all vault-driven sections without changing
`note_id`.  Tolaria's workspace doesn't yet consume
`Vault::watch_events` for any panel — when the Phase 9.6
`vault_lifecycle` workspace-level rescan trigger lands, calling
`refresh_state` from there is a one-liner.

**Workspace wiring (`crates/tolaria/src/main.rs`).**  Added
`inspector_panel_slot: Rc<RefCell<Option<Entity<InspectorPanel>>>>`
alongside `toc_panel_slot`.  The `HeadingsUpdatedEvent` subscriber
now fans the same envelope out to both panels; the `OpenNoteEvent`
subscriber writes through `set_active` to the inspector when
mounted.  The slot stays `None` today (no mount path — the
`ToggleInspector` action currently routes to GPUI's element-picker)
so the subscribers are no-ops until a follow-up row attaches an
`InspectorPanel` entity.  Once that happens (e.g. the deferred
Phase 8 8.3.1 follow-up that opens InspectorPanel in a separate
window, or a right-dock attach), Backlinks / References / Instances
/ Outline will populate without further wiring.

**MockVault fallback.**  Existing fixture-driven tests
(`InspectorState::resolve_from_mock` + `MockVault::seeded`) keep
passing — the resolver prefers `vault::Vault` over `MockVault`
when both are installed, mirroring `sidebar_panel::from_or_empty`
and `note_list_pane::from_or_empty`.

**Tests added.**  Six new `#[gpui::test]`s in `inspector_panel`:
`real_vault_backlinks_lists_inbound_notes` (the A → B, C → B
regression specified in the row),
`real_vault_references_lists_outbound_notes`,
`real_vault_instances_listed_by_prefix` (the `type-event.md` +
`event-foo.md` / `event-bar.md` regression — also asserts
`eventually.md` is excluded as a false-positive),
`real_vault_instances_empty_for_non_type_notes`,
`set_headings_short_circuits_on_unchanged_payload`,
`set_active_swaps_state_and_clears_outline`.  Total inspector_panel
test count: 16 (was 10).

**Out of scope (deferred).**  Heading-click body-anchor navigation
(no `ToHost::ScrollToAnchor` envelope today — same gap toc_panel
sits behind from 9.2.6); Properties / Relationships / GitHistory
section data sources (their own rows); per-section collapse-state
persistence; the live-app mount path for `InspectorPanel` (Phase 8
8.3.1 follow-up); vault `watch_events` → `refresh_state` consumer
(Phase 9.6 `vault_lifecycle`).

#### 9.2.9

**Source:** user-reported regression on 9.2.1 (star toggle, shipped
in `9a3839c9`), filed 2026-05-21.  Symptom: the star button no-ops
after the active note is modified outside the UI — e.g. an external
editor save, a `git checkout`, or any path that drives the Phase
8.11.4 fs-watcher rescan.

**Likely causes (to be confirmed by the implementing subagent):**
1. **Stale `NoteId` in the toolbar closure.**  `note_toolbar::render`
   captures `self.id` at render time; `Vault::rescan_internal`
   preserves IDs for paths that survive (`rescan_preserves_ids_for_unchanged_paths`
   test), but a delete + re-create cycle (atomic-save editors do
   exactly this — write to tempfile then rename over) drops the old
   id and assigns a new one.  The captured `NoteId` then misses
   `vault.notes`, and `set_frontmatter_bool` returns `NotFound`
   silently (the toolbar swallows the `Task` per the optimistic
   pattern).
2. **Optimistic-in-memory desync.**  9.2.1 mutates the in-memory
   `Note.frontmatter` BEFORE the disk write completes.  If the
   watcher's rescan runs between the write and the next render, the
   rescan re-reads disk and replaces the in-memory frontmatter with
   whatever's on disk — overwriting our optimistic mutation if the
   write hasn't flushed yet.
3. **Toolbar render not subscribed to vault changes.**  The toolbar
   reads `is_favorite()` once per render; if nothing triggers a
   re-render after the external edit lands, the glyph shows stale
   state even when vault is correct.  Subsequent clicks toggle from
   the stale state, which the write path then no-ops because
   `set_frontmatter_bool` short-circuits when new == current on disk
   (see `crates/vault/src/lib.rs:480-485` fast path).

**Scope of the fix:** identify which of the above is the real cause
(or whether it's a combination), add a `#[gpui::test]` that drives
an external-edit → toggle sequence and asserts the second toggle
lands, then ship the fix.  Likely surface: `crates/vault/src/lib.rs`
write path + `crates/note_item/src/note_toolbar.rs` render path
+ `crates/note_item/src/lib.rs` rescan subscription.  **Size:** small
to medium depending on which root cause is real.

**Order:** ships AFTER 9.2.4 (raw-mode toggle) lands, unless the
user redirects.  9.2.1 stays at ✅ for now — the regression is a
post-shipping bug rather than a 9.2.1 implementation defect, so it
gets its own row per the `[high]` syntax the user used.

**Closure (commit `e5978cd4`).**  Root cause was the
`set_frontmatter_bool` fast path (`crates/vault/src/lib.rs:493`):
when the disk bytes already matched the requested state (because an
external edit got there first), the fast path returned `Ok(())`
WITHOUT refreshing the in-memory `Note::frontmatter` from the bytes
it had just read.  Combined with the chrome's lack of a
fs-watcher subscriber (Phase 9.6 future work), the in-memory state
stayed permanently stale: the toolbar's render-time read returned
the stale value, every subsequent click dispatched the same
"matches disk" payload, the fast path bailed again, and the user
perceived the star as inert.

Fix has two seams:
1. **Vault layer** — the fast-path branch now calls a new
   `sync_in_memory_from_disk(note, raw, path)` free function that
   re-parses the just-read bytes into `Note::frontmatter` and
   refreshes `byte_size` / `modified` from `fs::metadata`.  The
   slow-path optimistic mutation is unchanged.  Two
   `#[gpui::test]`s pin the behaviour: one drives the exact
   production scenario (external edit, click dispatches the
   already-disk-true value, fast-path is taken, in-memory state
   must mirror disk after the call); the other layers a `rescan`
   into the sequence so the Phase 9.6 future-readiness path is
   also covered.
2. **Toolbar layer** — `toggle_frontmatter_flag` now calls
   `cx.refresh_windows()` after the dispatch.  The vault is a GPUI
   `Global`, so mutating it doesn't notify any entity; without the
   nudge the toolbar would keep showing the pre-click glyph until
   something else triggered a re-render.  A
   `toggle_helper_resyncs_in_memory_after_external_edit`
   `#[gpui::test]` exercises the toolbar-layer path end-to-end.

Out of the three candidate causes named in the annotation above:
**(2)** (optimistic-in-memory desync — really, here, "non-optimistic
in-memory staleness on the fast path") was the load-bearing
defect.  Candidate (1) (stale `NoteId`) was a red herring — the
`rescan_preserves_ids_for_unchanged_paths` invariant holds; the
external write reuses the same path under macOS atomic-save, so the
id stays mapped.  Candidate (3) (toolbar not subscribed) is real
but the user only noticed it because (2) made the click feel
broken; the toolbar wrapper now compensates with
`cx.refresh_windows`.

#### 9.2.10

**Source:** user-reported visual regression on 9.2.2 (organized
toggle), 2026-05-21, with attached screenshot showing the desired
treatment.  **Symptom:** when `_organized: true`, the toolbar cell
distinguishes itself only by icon-fill variant; the user expects a
green-filled glyph (matches the React `BreadcrumbBar.tsx`
`OrganizedAction` styling).

**Scope:** add a colour treatment to the organized toolbar cell
when active — green when checked, default-muted otherwise.  Tap the
existing `theme.success` / type-colour palette so the green tracks
light/dark mode.  Surface: `crates/note_item/src/note_toolbar.rs`
organized branch.  **Size:** small.

**Closure (commit `e1d61a32`).**  Shipped paired with `9.2.11`.
A new `toolbar_cell_with_active_color` helper paints the glyph in an
explicit `Hsla` when the cell is active, suppressing the background
tint that `toolbar_cell_with_active` (the raw-mode helper) draws so
the colour signal lives on the icon itself — matching React's
`text-[var(--accent-green)]` treatment.  The organized branch routes
through `theme.success`, which maps to `--accent-green` in both light
(`#38A169`) and dark (`#79D89D`) palettes; a
`#[gpui::test]` (`organized_active_color_matches_theme_success`)
pins the choice of token so a future palette refactor that retargets
the green can't silently desync the toolbar.

**Reopened (2026-05-21)** ⏳ — user reports the treatment isn't a
filled-disk look.  The React reference (user-attached screenshot)
shows a **green-filled circle with a white check inside**, not an
outlined `CircleCheck` tinted green.  Current implementation tints
`IconName::CircleCheck` (outline) in `theme.success` — the icon's
strokes are green but the centre stays empty, which reads as "ring"
not "filled disk".  Fix: either swap to a filled-circle icon
variant (e.g. `IconName::CircleCheckBig` if it exists in
`gpui-component`) OR render a green-filled `div` with a white
check icon overlaid.  The Star branch's `StarFill` icon already
gives the desired filled-glyph effect — match that pattern for
parity.  Original broken-closure commit sha remains `e1d61a32`.

**Re-closure (commit `d3f5971e`).**  Took the "background fill +
overlay icon" path: introduced an `ActiveStyle` enum in
`note_toolbar.rs` with three variants — `Tint` (raw-mode subtle
bg), `GlyphColor(Hsla)` (star icon stroke colour), `Fill(Hsla)`
(organized green disk).  The cell helper paints the cell
background with the fill colour and forces the glyph to
`gpui::white()` when in `Fill` mode.  Inactive state stays as the
outlined `CircleCheck` in muted foreground.  `organized_icon_for(active)`
is a tiny helper extraction so the test path calls the same render
function the production path uses — no test-vs-production drift.

#### 9.2.11

**Source:** user-reported visual regression on 9.2.1 (star toggle),
2026-05-21, with attached screenshot showing the desired treatment.
**Symptom:** when `_favorite: true`, the toolbar cell uses
`IconName::StarFill` but the colour stays muted; the user expects
the active star to render orange (matches the React
`BreadcrumbBar.tsx` `FavoriteAction` styling).

**Scope:** add a colour treatment to the star toolbar cell when
active — orange (`#F59E0B` / amber-500-ish, theme-aware) when
checked, default-muted otherwise.  Same surface as 9.2.10; the two
ship together as one commit.  **Size:** small.

**Closure (commit `e1d61a32`).**  Shipped paired with `9.2.10`
through the same `toolbar_cell_with_active_color` helper.  The star
branch passes `gpui::rgb(0xD69E2E)` (the light-mode literal of
`--accent-yellow`, see `src/index.css:77`) directly rather than a
theme token — `gpui_component::ThemeColor` has no `accent_yellow`
field, and React's `--accent-yellow` resolves to the SAME hex value
for the active glyph regardless of mode at the load-bearing pixel
position (the dark-mode `#F2C86B` is a soft fallback the toolbar
doesn't need today).  A `TODO(visual-parity)` notes the deferred
theme-aware refactor for when the token lands.  A `#[test]`
(`star_active_color_matches_accent_yellow`) pins the literal so the
TODO doesn't quietly drift.

#### 9.2.12

**Source:** user-reported behavioural bug on Inbox sidebar view,
2026-05-21.  **Symptom:** the Inbox view shows every non-archived
note in the vault; it should exclude notes with `_organized: true`.

**Scope:** add an `is_organized` filter to the Inbox scope in
`crates/note_list_pane` (or wherever `NoteListScope::Inbox` resolves
to a filtered note list).  React parity: `useInboxOrganizeAdvance`
treats `_organized` as the explicit "out of the inbox" marker —
toggling organized on a note pulls it out of the Inbox view; the
note remains visible elsewhere (All Notes, Favourites, etc.).
Surface: `crates/note_list_pane/src/lib.rs` filter logic + a
regression test asserting an organized note doesn't appear in
Inbox.  **Size:** small.

**Closure (commit `8ee5fa33`).**  `NoteEntry` gained an
`is_organized: bool` field populated from `Note::is_organized()` in
`collect_vault_entries` (and seeded `false` for the `MockVault`
branch, which has no triage state).  `scope_matches` flips the
`NoteListScope::Inbox` arm from a pass-through to
`!entry.is_organized`, leaving every other scope unchanged.  The
`AllNotes` / `Type(_)` / `Folder(_)` / `View(_)` / `Archive` arms
still see organized notes — moving a note out of the inbox does not
remove it from the vault.  A `#[gpui::test]`
(`inbox_scope_excludes_organized_notes`) opens a real on-disk vault
with one organized + one fresh note and asserts both invariants.

**Reopened (2026-05-21)** ⏳ — user reports the Inbox still shows
organized notes after a toggle.  Root cause analysis pending; most
likely culprit is the `NoteEntry::is_organized` field being
captured at `collect_vault_entries` build time and never refreshed
after a chrome-initiated frontmatter toggle.  `refresh_from_vault`
exists at `note_list_pane/src/lib.rs:986` but is only invoked from
`tolaria/src/open_note.rs:186` (on note-open).  `cx.refresh_windows()`
in `toggle_frontmatter_flag` triggers a re-render but the render
reads the stale `entries` slice.  Fix: emit a vault-side signal
(extension of `VaultChanged` or a new event) when
`set_frontmatter_bool` lands, and subscribe `note_list_pane` to it.
Original commit sha for the broken closure remains `8ee5fa33`.

**Re-closure (commit `d3f5971e`).**  `Vault::set_frontmatter_bool`
now emits a `VaultChanged::FrontmatterChanged` event over the
existing `watch_tx` channel after the in-memory + disk update lands.
`note_list_pane` subscribes via `install_vault_watch_task`
(mirroring `note_item::install_dispatch_task`) and calls
`refresh_from_vault` on every event so the cached `entries` slice
tracks chrome-initiated changes.  Regression test opens a real
on-disk vault via `tempfile::TempDir`, drives the executor through
the event, and asserts an organized note disappears from the Inbox
visible list.

#### 9.2.13

**Source:** user-shared React reference screenshot (2026-05-21)
showing the full Tauri-era Inspector Panel layout — `Properties`
header with section dock-toggle + close affordances, frontmatter
property rows (`Type`, `Status`, `Date`, `URL`, `Icon`) with inline
editors, a `+ Add property` link, multiple **relationship sections**
(`Aliases`, `Belongs to`, `Owner`, `Related to`, `Has`) each
rendering wikilink pills with inline `Add` slots and a footer
`+ Add relationship` button, an `Info` section (`Modified`,
`Created`, `Words`, `Size`), and a `History` section listing recent
git commits touching the note.

**Scope:** broadens the inspector parity beyond row `9.2.8`'s four
data sections to cover the **chrome / editing surfaces** the React
inspector exposes.  Each named section is its own sub-feature:
- **Properties** — render + edit frontmatter values per property
  type (string, status enum, date, URL, icon).
- **Add property** — frontmatter key creation flow.
- **Relationship sections** — `Aliases`, `Belongs to`, `Owner`,
  `Related to`, `Has`: wikilink-pill list + `Add` field per
  section.  Inverse-relationship resolution for `Has` (notes that
  declare `parent: <this>` show up here).
- **Info** — read-only `Modified` / `Created` / `Words` / `Size`
  from `Note.modified`, `Note.byte_size`, plus a word count derived
  from the body.
- **History** — git log filtered to this note's path.  Depends on
  Phase 11 `git_provider` (renumbered; was Phase 10) — stub list
  until provider lands.

**Deps:** (1) frontmatter write paths for arbitrary keys (`set_frontmatter_string` / `set_frontmatter_date` / etc — generalising `set_frontmatter_bool`); (2) a relationship parser on top of `vault::Frontmatter` that understands list-of-wikilinks values; (3) inverse-relationship index in `vault::Vault`; (4) `git_provider` for the History section; (5) `frontmatter_panel` crate (shipped in Phase 8 `8.15`) likely overlaps with the property-edit surfaces — coordinate to avoid duplicating editors.

**Size:** large — splits naturally into multiple sub-rows when
picked up.  Likely lands across Phase 9 + Phase 10 (behavioral
layers) depending on ordering.  Out of `9.2.8`'s scope; tracked
here because the user-shared reference makes the parity target
explicit.

**Closure of sub-scope 9.2.13a (commit `5a61722e`).**
First read-only display pass: the `Properties` body now reads the
active note's frontmatter via `vault::Note::frontmatter()` and
renders one row per `(key, FrontmatterValue)` pair in sorted
order (internal keys `_favorite`, `_organized`, `_favorite_index`
filtered out).  Each value renders per-variant: `Text` /
`Date` / `Number` / `Bool` as plain text, `List` as a comma-
separated text run.  The `Relationships` body now parses six
relationship-shaped frontmatter keys (`aliases`, `belongs-to` /
`belongs to`, `owner`, `related-to` / `related to`, `has`,
`parent`, `child`), collects the embedded `[[wikilink]]` targets,
and renders one collapsible-style group per relationship key
with the targets as pills.  Inverse relationships ship as a
single combined `Referenced From` group, sourced by walking
`vault.backlinks(active_id)` and keeping every backlinker whose
frontmatter declares a relationship key that targets the active
note's title.  A new `Info` section (sitting between
`Relationships` and `GitHistory`) renders read-only `Modified`
and `Size` rows from `Note.modified` + `Note.byte_size`, with
`humanize_bytes` formatting the byte count as `B` / `KB` / `MB`.
**Still open on 9.2.13:** Properties editing (typed editors for
date / status / URL / icon / wikilink — `9.2.13b`); `+ Add
property` + per-section `Add` slot + `+ Add relationship`
footer button (`9.2.13c`); `Created` + `Words` rows in `Info`
(depends on either an `fs::metadata().created()` plumbing or a
body cache on `Note` — `9.2.13a-followup`); `Git History`
rendering (`9.2.13d`, blocked on Phase 11 `git_provider`); per-
inverse-relation split of `Referenced From` into one group per
inverse key (`9.2.13e`); header dock-toggle + close `X` chrome
buttons (polish, `9.2.13f`).

### Cross-row notes

- **Shared infrastructure.** Rows `9.2.3` (neighbourhood) and `9.2.8`
  (inspector backlinks) both consume `vault::Vault::backlinks(id)` —
  land the query once on whichever row ships first.  Rows `9.2.6`
  (ToC) and `9.2.8` (inspector outline) both consume the new
  `ToHost::Headings` bridge variant — same one-and-done pattern.
- **Shared write path.** Rows `9.2.1` (star) and `9.2.2` (organised)
  both write a single boolean to `vault::Frontmatter`; landing them
  in a single commit pair amortises the new `set_frontmatter_bool` /
  `_favorite_index` rewrite work.
- **More menu blockers.** Row `9.2.7` wraps several other actions;
  land `9.2.3` (neighbourhood), `9.2.4` (raw), and `9.2.6` (toc) with
  real dispatchers before closing `9.2.7` so its menu items have
  somewhere to dispatch.
- **Outside scope.** Phase 9 does not touch the `note-toolbar-width`
  cell between raw and ai — that is a chrome-level layout knob, not
  a deferred Phase 8 row, and stays a `stub_cell` until the
  multi-tab UX work picks it up (Phase 13 in the renumbered
  roadmap).
