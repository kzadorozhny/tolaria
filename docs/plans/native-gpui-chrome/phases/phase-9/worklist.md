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
9.2.3. Neighbourhood action → backlink filter in note-list
9.2.4. Raw-mode toggle → editor-host raw bridge
9.2.5. AI button → attach `ai_panel` to right dock + `ToggleAiPanel`
9.2.6. ToC action → new `toc_panel` crate + headings bridge
9.2.7. More-overflow menu → archive / delete / collapse-when-narrow actions

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

### Cross-row notes

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
