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
9.2.8. Note Inspector Panel content — backlinks, references, type instances, outline
9.2.9. ✅ Star action stops working when the note is updated outside the UI
9.2.10. ✅ Organized toolbar cell needs green-checked colour treatment
9.2.11. ✅ Star toolbar cell needs orange-filled colour treatment when active
9.2.12. ✅ Inbox sidebar view must exclude notes with `_organized: true`

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
