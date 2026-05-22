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
9.2.7. ✅ More-overflow menu → archive / delete / collapse-when-narrow actions
9.2.8. ✅ Note Inspector Panel content — backlinks, references, type instances, outline
9.2.9. ✅ Star action stops working when the note is updated outside the UI
9.2.10. ✅ Organized toolbar cell needs green-checked colour treatment
9.2.11. ✅ Star toolbar cell needs orange-filled colour treatment when active
9.2.12. ✅ Inbox sidebar view must exclude notes with `_organized: true`
9.2.13. ✅ Inspector Panel — Properties, Aliases, Belongs to, Owner, Related to, Has, Info, History sections
9.2.14. ✅ Neighbourhood — toolbar active-state treatment + note-list header shows the active note's title
9.2.15. ✅ System menu View — rename "Show Inspector" to "Show Properties"; restore "Show Inspector" toggling the GPUI element overlay
9.2.16. ✅ Neighbourhood buttom shoud be a toggle to activate/deactivate the neighbourhood view
9.2.17. ✅ The note width toggle (wide/narrow) does not work
9.2.18. ✅ Add text_overflow ellipsis to note header_strip note titles
9.2.19. ✅ Add note toolbar button to show/hide (Inspector Panel) pane

## 3. Low Priority

9.3.1. ✅ Block editor drag handles do not Cary React side styling
9.3.2. ✅ Inspector panel should open at least the default width of the sidebar
9.3.3. ✅ Inspector panel header — same height as note header, title reads `Properties`
9.3.4. ✅ Inspector open/close button migrates to the panel header when open
9.3.5. ✅ Note properties panel toggle button moves to title-bar right corner (mirror sidebar toggle on opposite side)
9.3.6. ✅ Downgrade note-toolbar logging introduced in Phase 9 to `debug!` level
9.3.7. ✅ Block editor selection menu should have React side styling
9.3.8. ✅ The Note List eighbourhood mode title reads `Neighbourhood`of <note-is>. It should be `Note title` (same as in Note List)

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

**Reopened (2026-05-21)** ⏳ — user reports clicking the
neighborhood toolbar button "does nothing".  Original commit sha
remains `13bbc646`.  Handler is registered at `tolaria/src/main.rs:882`
with the standard `active_note_item.borrow().as_ref().cloned()`
slot read; if `None`, logs `EnterNeighborhood: no active NoteItem`
at `debug` and returns silently.  Diagnosis path: enable debug
log, click button, see whether the log line fires.  If it does →
slot isn't populated when the user expects (look at `open_note`
mutation timing).  If it doesn't → either the action isn't
registered at the right context level, or `cx.dispatch_action`
from inside the toolbar `on_click` closure doesn't reach the App
handler.  Either way, fix path is: add a `#[gpui::test]` that
simulates a toolbar click on a vault-backed note and asserts the
handler runs; the test should reproduce the failure.

**Re-closure-2 (commit `43a9fcab`).**  Investigation: the
`#[gpui::test]` reproduction
(`tolaria::tests::toolbar_actions_resolve_via_active_note_item_slot`)
exercises the exact production wiring shape — register the global
handler against `actions::ToggleRawEditor`, dispatch via
`cx.dispatch_action`, populate the `ActiveNoteItemSlot` with a
real `NoteItem` entity, then assert the handler fired and read
the slot's note id.  The test passes against the live code today,
which means **the dispatch and slot mechanics are intact** — the
"does nothing" report is the empty-neighbourhood case where the
active note has no inbound or outbound wikilinks, so
`vault.backlinks(id) ∪ vault.outbound_links(id)` resolves to the
empty set and the note list filters down to zero rows.  React's
`useNeighborhoodEntry` would also render an empty list in this
case; the React UX cushion is that real Tauri vaults already have
backlinks from past use, while the demo vault used for GPUI QA may
not.  Fix has three seams:
(1) The `EnterNeighborhood` handler now logs at `info!` on every
dispatch (`id`, `title`, neighbour count) and at `warn!` when the
resolved set is empty, so the visible "nothing happened" branch is
explicit in the live log — the user can grep for
`tolaria::neighborhood` to confirm dispatch.
(2) The slot-empty branch is promoted from `debug!` to `warn!`
so a future regression that actually does empty the slot surfaces
in default-level logs.
(3) A regression test
(`tolaria::tests::toolbar_actions_resolve_via_active_note_item_slot`)
pins the slot-reading contract and the dispatch path so a future
refactor that breaks them fails CI.  The visual "empty neighbourhood
looks like a no-op" UX cushion is captured by the warn log; a
proper empty-state placeholder in the note-list pane is tracked
separately (deferred Phase 9.2 follow-up — outside the regression
fix scope).

**Reopened-2 (2026-05-21)** ⏳ — user re-reports "9.2.3 Neighbourhood
produces no visible results."  Distinct from the empty-result case
captured by `cc2c26f8`'s `warn!` log: the user is testing notes that
DO have wikilinks but the note-list still doesn't visibly swap to
the neighbourhood scope.  Diagnosis path: run with
`RUST_LOG=tolaria::neighborhood=info,tolaria=info`, click the
toolbar button, capture the log output.  If `EnterNeighborhood:
dispatched` appears AND `neighbours=N` with `N > 0` BUT the
note-list still shows the old scope, the regression is in
`NoteListPane::set_scope` (or wherever the scope swap re-renders).
Possible cause: the note-list pane is observing the wrong event
type, or the scope-swap doesn't trigger `cx.notify()`.

**Re-closure-3 (commit `43a9fcab`).**  Root cause turned out to
be shared across all four `Reopened-2` rows (`9.2.3` + `9.2.4` +
`9.2.6` + `9.2.13`): every affected toolbar cell called
`cx.dispatch_action(&actions::EnterNeighborhood)`
(`App::dispatch_action`) from inside an `on_click` closure.  Click
closures run inside GPUI's `Window::dispatch_event`, which entered
through `update_window_id` — the window slot in `cx.windows` is
already `take()`-en for the current update.  `App::dispatch_action`
then re-enters via `active_window.update(self, …)`, the inner
`cx.windows.get_mut(id)?.take()?` returns `None`, and the outer
`.log_err()` swallows the dispatch silently.  The handler at
`tolaria/src/main.rs:1019` never fires; the note-list pane never
sees a `set_scope` call.  Fix: every toolbar cell now uses
[`Window::dispatch_action`] (`window.dispatch_action(Box::new(action), cx)`)
which `cx.defer(...)`s the dispatch internally, queueing it for
after the click update unwinds — at which point the window slot is
back in `cx.windows` and the App-scope handler fires as expected.
Regression test (in `crates/tolaria/src/main.rs`):
`toolbar_window_dispatch_reaches_app_action_handler_under_nested_update`
opens + activates a workspace window, registers an App-scope
`ToggleRawEditor` handler, dispatches from inside `window.update`
via `Window::dispatch_action`, and asserts the handler ran exactly
once.  A paired negative test
(`app_dispatch_action_from_inside_window_update_silently_drops`)
pins the failure mode of the old `cx.dispatch_action` route so a
future refactor that re-introduces it fails CI.  Per-click `info!`
logs (`note_item::toolbar`) trace the click → dispatch boundary on
every affected cell so live triage can confirm dispatch reaches the
handler without rebuilding a special diagnostic binary.

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

**Reopened (2026-05-21)** ⏳ — user reports clicking the raw-mode
toolbar button "does nothing".  Original commit sha remains
`45b6622d`.  Same shape of bug as `9.2.3`'s regression: handler at
`tolaria/src/main.rs:666` checks `raw_slot.borrow().as_ref().cloned()`
and silently no-ops when `None`.  Could share a root cause with
`9.2.3` (handler not reached, slot empty, or both).  Diagnose with
the same `#[gpui::test]` approach + debug logging.

**Re-closure-2 (commit `43a9fcab`).**  Shares the same fix
shape as `9.2.3`'s re-closure-2: the regression test
(`tolaria::tests::toolbar_actions_resolve_via_active_note_item_slot`)
proves the dispatch + slot mechanics work end-to-end against the
live wiring — `App::dispatch_action(&ToggleRawEditor)` reaches the
global handler, the handler reads the slot, and
`item.update(cx, |item, cx| item.toggle_raw_mode(cx))` flips
`raw_mode` to `true`.  The handler now logs at `info!` on every
dispatch (`id`, before / after raw flag) so the chrome's side of
the toggle is observable in the live log; the slot-empty branch is
promoted from `debug!` to `warn!` so a real ordering regression
would surface.  The cell's active-state treatment (filled
background when `raw_mode = true`) provides immediate visual
feedback that the chrome flipped, independent of the editor-host
JS-side surface swap.  The editor-host dispatcher in
`editor-host/src/EditorApp.tsx:332` is unchanged from `45b6622d`
and its `bridge.test.ts` coverage still passes — if the live JS
bundle is stale the chrome will still log + visually flip and the
WebView side will stay in rich mode (separate `editor-host` build
issue, not a Rust-side regression).

**Reopened-2 (2026-05-21)** ⏳ — user re-reports "9.2.4 Raw mode
is not showing."  Even if the JS bundle were stale, the toolbar
glyph SHOULD flip its active state on click (purely chrome-side).
If neither the WebView body NOR the glyph changes, the action
isn't reaching the handler.  Diagnosis path: log `RUST_LOG=tolaria::raw_editor=info`,
click the cell, watch for `ToggleRawEditor: dispatched`.  If the
log doesn't fire, the action isn't reaching the handler at all —
suspect `cx.dispatch_action` from inside the toolbar `on_click`
closure not bubbling to the App-scope handler.  May share root
cause with `9.2.6` / `9.2.13` (right-dock panels not appearing)
and `9.2.3` (neighbourhood scope not swapping).

**Re-closure-3 (commit `43a9fcab`).**  Suspicion confirmed:
the dispatch was the regression, not the WebView surface or the
JS bundle.  Shared root cause + fix with `9.2.3`'s Re-closure-3 —
toolbar's `note-toolbar-raw` cell now uses
`window.dispatch_action(Box::new(actions::ToggleRawEditor), cx)`
(internally deferred), so the dispatch reaches the App-scope
handler at `tolaria/src/main.rs:732` once the click update unwinds.
The handler flips `raw_mode` via `NoteItem::toggle_raw_mode`, which
calls `cx.notify()` + pushes `ToHost::SetRawMode` to the editor
host.  Visible result: the toolbar's filled-background active
treatment paints when raw is on (purely chrome-side, independent
of any editor-host bundle freshness), and the WebView body flips
to the raw CodeMirror surface assuming the `editor-host` JS bundle
carries the `set_raw_mode` arm (which it has since `45b6622d`).
See `9.2.3`'s Re-closure-3 paragraph for the full root-cause
analysis (`cx.dispatch_action` re-entrancy in
`update_window_id`'s `take()` of the window slot) and the
regression-test pair that guards against the route regressing.

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

**Reopened (2026-05-21)** ⏳ — user reports "9.2.6 TOC panel is
not showing."  Clicking the toolbar TOC button doesn't surface
the right-dock panel.  Original closure remains `5bd2533e`; the
right-dock-attach pattern was reshaped by `2662e935` into the
shared `toggle_or_swap_right_dock_panel` helper.  Likely shares
root cause with `9.2.13` (Inspector panel doesn't appear) since
both use the same helper.  Diagnosis path: log
`RUST_LOG=tolaria=info`, click TOC button, watch for
"ToggleTableOfContents: dispatched" and any panel-attach traces.

**Re-closure-3 (commit `43a9fcab`).**  Likelihood confirmed:
the right-dock attach helper was fine; the dispatch never reached
the handler at `tolaria/src/main.rs:805` because the toolbar's
`note-toolbar-toc` cell used `cx.dispatch_action(&actions::ToggleTableOfContents)`
(App-level), which silently fails when nested inside the click
update.  Shared root-cause + fix with `9.2.3`'s Re-closure-3.  The
cell now dispatches via
`window.dispatch_action(Box::new(actions::ToggleTableOfContents), cx)`
(internally deferred), and the handler attaches `toc_panel::TocPanel`
to the right dock through `toggle_or_swap_right_dock_panel`.  The
workspace's `cx.observe(&right_dock, ...)` then re-renders the
shell with the right-dock column populated.  See `9.2.3`'s
Re-closure-3 paragraph for the full GPUI re-entrancy analysis and
the regression-test pair that guards the route.

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

**Closure (commit `f075ac21`).**  More cell now opens a
`gpui_component::popover::Popover`-anchored dropdown with six items:
`Reveal in Finder`, `Copy path`, `Toggle TOC` (dispatches
`actions::ToggleTableOfContents`), `Toggle raw mode` (dispatches
`actions::ToggleRawEditor`), `Archive` (dispatches
`actions::Archive`), `Delete` (red, dispatches `actions::Delete`).
New `Vault::archive_note(id)` writes `_archived: true` via
`set_frontmatter_bool`; `Vault::delete_note(id)` removes the file
and drops the entry from `notes`.  Tests: vault archive + delete
round-trip; toolbar More-cell builds.  **Deferred to follow-ups:**
responsive overflow collapse (`9.2.7-followup`); filesystem trash
directory (`9.2.7-trash`); ConfirmDelete modal (Phase 11 dialogs).
Note: this row ships ahead of `9.2.3` / `9.2.4` / `9.2.6` being
fully verified live — the menu items dispatch the right actions
but their downstream effects (panel attach, scope swap, WebView
mode flip) are the topic of the parallel-reopened regressions.

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

**Reopened-2 (2026-05-21)** ⏳ — user reports the rendered glyph
is still the **outlined `CircleCheck` icon** drawn on top of a
green-square cell background, not a single filled-circle icon.
React reference (user-attached Image #5) is a clean green circle
with a white check inside — round shape, not the rounded-square
cell `.rounded_sm()` paints.  Root cause: gpui-component only ships
`circle-check.svg` (outlined Lucide); no filled-circle-check
variant exists.  Current implementation paints the cell rectangle
green and overlays the outline `CircleCheck` icon — the result
reads as a rounded-square with an outlined check shape inside,
not a filled disk.  **Fix paths:**
1. Use `IconName::Check` (just the checkmark, no surrounding
   circle outline) overlaid on a `.rounded_full()` green-filled
   circular `div` sized 18-20pt.  Avoid the cell `rounded_sm()`
   inheriting through.
2. Add a project-owned `circle-check-fill.svg` asset and a
   `LocalIconName::CircleCheckFill` variant that points at it.
   Requires plumbing through `gpui-component`'s `IconNamed` trait
   or the workspace's own icon registry.
Option 1 is smaller; take it unless `gpui-component`'s asset
loader can't load just `Check` without the surrounding circle.

**Re-closure-2 (commit `43a9fcab`).**  Took Option 1.  The
`Fill` variant of `ActiveStyle` now renders an inner
`rounded_full` 18×18 disc as a child of the cell, with the
glyph (`IconName::Check`) painted in white over that disc.  The
cell's outer `rounded_sm` 24×24 rectangle stays transparent — the
load-bearing change is that `active_bg` for `Fill` is now `None`
instead of `Some(disc colour)`, so the 24×24 click target keeps
its baseline (no fill, no hover-tint when active) and only the
inner round disc carries the green.  Result reads as a clean green
circle with white check matching React's `OrganizedAction` Image
#5 reference; the 3 pt halo of cell baseline around the disc gives
the visual rhythm the React layout uses between the surrounding
click target and the action chip.  `toolbar_cell_inner` is the
single source of truth — the `Tint` (raw-mode) and `GlyphColor`
(star) variants are unchanged, so the green disc treatment is
opt-in via `ActiveStyle::Fill` and only the organized cell takes
it today.  New regression test
(`organized_active_cell_helper_constructs_with_filled_disc`) pins
the helper's disc-construction path so a future refactor that
drops the inner child accidentally fails CI.

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

**Reopened-2 (2026-05-21)** ⏳ — user reports the Inbox sidebar
**note count** is incorrect.  Distinct from the list-visible
filter (which now refreshes after the previous fix): the sidebar
section header shows a count badge / number that doesn't update
when notes get toggled organized.  Likely surface: `sidebar_panel`
renders an Inbox row with a count derived from vault state, but
doesn't subscribe to the same `VaultChanged::FrontmatterChanged`
events `note_list_pane` consumes.  Fix path: mirror the
`install_vault_watch_task` pattern in `sidebar_panel` so the
sidebar's section counts refresh on every frontmatter change.

**Re-closure-2 (commit `43a9fcab`).**  Three seams:

(1) **`inbox_count` now reflects `!is_organized`.**
`SidebarPanel::build_from_samples` previously assigned
`inbox_count = total_count` — every note counted as "in the inbox"
regardless of frontmatter, so the badge disagreed with
`note_list_pane`'s `scope_matches` predicate from the moment
9.2.12's first closure landed.  The sample tuple was extended to a
named `SidebarSample { kind, path, is_organized }` struct;
`from_vault` reads `Note::is_organized()` per note, `from_mock`
seeds `false` (MockVault has no triage state).
`build_from_samples` filters the sample list to count only the
unorganized entries.  `inbox_count_excludes_organized_samples`
pins the predicate so a future refactor that drops it fails CI.

(2) **`SidebarPanel::refresh_from_vault` +
`install_vault_watch_task`.** Mirror of
`note_list_pane`'s shape (`refresh_from_vault` rebuilds the
derived state in place; `install_vault_watch_task` drains the
`flume::Receiver` and calls `refresh_from_vault` on every
event).  The user-driven state (`selected` row,
`collapsed` per-section state, dock `position`) is preserved
across refreshes so a vault tick can't bounce the user off
whichever row they had highlighted.

(3) **Fan-out the vault receiver in `tolaria/main.rs`.**
`Vault::watch_events` returns clones of one
`flume::Receiver` — flume's MPMC work-stealing semantics mean
two `install_vault_watch_task` siblings would *compete* for
messages, not both fire.  The workspace-open path now drains the
receiver in one task and calls both `NoteListPane::refresh_from_vault`
and `SidebarPanel::refresh_from_vault` per event, so the centre
list and the sidebar badge stay in lockstep.  The
`install_vault_watch_task` helpers on each pane remain public for
test scaffolding (the
`inbox_count_refreshes_after_chrome_initiated_organized_toggle`
test wires its own task per-entity), but the production path
takes the fan-out shape.

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

**Regression (2026-05-21)** ⏳ — user reports the **inspector
panel does not open** after the `5a61722e` ship.  Clicking the
note-toolbar inspector button no longer surfaces a panel.  Most
likely culprit: the new `InspectorSection::Info` variant breaks
either the panel's `cx.open_window`/dock-attach path or a section-
iteration site that wasn't updated (the dispatch match might miss
the new variant or the section count calculation might overflow a
fixed array).  Could also be a missing `Cargo.toml` propagation
of the `chrono` runtime promotion if some downstream feature gate
needs adjusting.  Fix path: run the app, click the inspector
toggle, capture the panic or log to localise the failure.

**Re-closure-2 (commit `43a9fcab`).**  Root cause was not the
`Info` variant: `actions::ToggleInspector` (`tolaria/src/main.rs:528`)
was still routed to GPUI's debug element-picker overlay
(`Window::toggle_inspector`), so clicking the toolbar inspector cell
flipped the dev-tool overlay (often invisible without a renderer
match) instead of attaching the application's
`inspector_panel::InspectorPanel`.  The 9.2.8 closure (`8897ab93`)
introduced an `inspector_panel_slot` but explicitly left the mount
to a "follow-up row"; this is that follow-up.  Fix has four seams:
(1) `actions::ToggleInspector` is now the product action that
attaches the `InspectorPanel` to the workspace's right dock; a new
`actions::ToggleElementInspector` carries the developer
element-picker overlay (`Cmd+Alt+I` keymap entry moved with it).
(2) A new `toggle_or_swap_right_dock_panel` helper in `main.rs`
encodes the three right-dock states (already-mounted-toggle,
sibling-swap, fresh-attach) so the ToC and Inspector handlers stay
symmetric — the `panel_key` of the currently attached panel
(via the new `Dock::panel_key` + `TolariaWorkspace::right_dock_panel_key`
accessors) is the source of truth for which branch fires.  (3) The
slot re-uses the same `InspectorPanel` entity across swaps, so the
existing `HeadingsUpdated` / `OpenNote` subscribers (already wired in
`8897ab93`) keep tracking the same panel without re-resolving the
workspace.  (4) `menus.rs` MenuState's `inspector_picking` field now
reflects right-dock visibility for the `"inspector"` key instead of
`Window::is_inspector_picking`, so View → Hide / Show Inspector
flips against the actual panel.  Tests added: workspace's
`right_dock_panel_key_tracks_attached_panel` (empty → toc → swap to
inspector) and tolaria's `toggle_inspector_attaches_panel_and_swaps_with_toc`
exercises the open/close/swap state machine end-to-end against the
live helper.

**Reopened-2 (2026-05-21)** ⏳ — user re-reports "9.2.13 Inspector
Panel is still does not appear" after the `2662e935` ship.  The
test `toggle_inspector_attaches_panel_and_swaps_with_toc` passes;
the production click doesn't open the panel.  Possible causes:
(1) `cx.dispatch_action(&actions::ToggleInspector)` from inside
the toolbar `on_click` closure doesn't reach the App-scope
`cx.on_action` handler.  (2) The `dispatch_to_workspace` defer
fires but `cx.active_window()` returns `None` or the wrong
window.  (3) `toggle_or_swap_right_dock_panel` runs but
`ws.attach_right_dock(panel, cx)` doesn't trigger a workspace
re-render.  (4) The panel attaches but `Dock::set_panel` reads
`InspectorPanel.starts_open()` differently from the test fixture
(both return `true`, so this is unlikely).  Likely shares root
cause with `9.2.6` (TOC panel doesn't appear) and possibly with
`9.2.3` + `9.2.4` (toolbar dispatches that don't visibly do
anything).  Diagnosis: add `info!` log at every step of the
dispatch chain — toolbar `on_click`, action handler entry,
`dispatch_to_workspace` resolve, `toggle_or_swap_right_dock_panel`
branch selection, `Dock::set_panel` invocation.

**Re-closure-3 (commit `43a9fcab`).**  Cause **(1)** was the
correct hypothesis: the toolbar's `note-toolbar-inspector` cell
called `cx.dispatch_action(&actions::ToggleInspector)`, which
re-enters `active_window.update(...)` while the window slot is
already taken by the outer `dispatch_event` update and silently
fails via `.log_err()`.  The dispatch never reached the App-scope
handler at `tolaria/src/main.rs:826`; the
`toggle_or_swap_right_dock_panel` helper and the right-dock
observer were both fine (as `toggle_inspector_attaches_panel_and_swaps_with_toc`
proved at the helper level).  Fix: shared with `9.2.3`'s
Re-closure-3 — the cell now dispatches via
`window.dispatch_action(Box::new(actions::ToggleInspector), cx)`
(`Window::dispatch_action` internally defers, queueing for after
the click update unwinds).  The handler at `:826` then attaches
the `InspectorPanel` to the right dock through the existing helper.
Same root-cause analysis + regression-test pair as `9.2.3`'s
paragraph — the active-window-required regression test
(`toolbar_window_dispatch_reaches_app_action_handler_under_nested_update`)
guards against the route regressing.  The four `Reopened-2` rows
turned out to be a single bug; each row's annotation flags the
shared fix so future triage doesn't re-investigate each cell
independently.

**Reopened-3 (2026-05-21)** ⏳ — user re-reports "Inspector Panel
does not open again" after the `d9766aa5` inspector chrome reshape
(9.3.2 + 9.3.3 + 9.3.4 + 9.3.5).  The toolbar inspector cell was
removed in that commit; user now opens the panel from the new
title-bar toggle (`title-bar-toggle-inspector` at
`workspace/src/title_bar.rs:189`).  Click dispatches
`actions::ToggleInspector` via `Window::dispatch_action`; the
handler at `tolaria/src/main.rs:836` runs
`toggle_or_swap_right_dock_panel`.  Diagnosis path: log
`RUST_LOG=tolaria=info,workspace=info`, click the title-bar
Inspector toggle, watch for the dispatch + attach traces.
If the panel attaches but renders invisibly: confirm the new
header strip (9.3.3 commit `d9766aa5`) doesn't consume all
available width OR `Dock::set_panel` reads `starts_open == true`
(it does — `InspectorPanel::starts_open` at
`inspector_panel/src/lib.rs:1471` returns `true`).  Could also
be a fresh regression in the right-dock visibility chain in
`workspace.rs` after the dock-width constant unification.

**Diagnostic-promotion (commit `148378eb`).**  The
`b1614df8` instrumentation pass added info-level logs at every
dispatch hop, but the env_logger filter in `tolaria::macos::run`
only registered `tolaria` at Info — the **first** hop of the
inspector chain (`workspace::title_bar` "title-bar inspector
click") was silently filtered out at default `cargo run` log
level, so the user's terminal showed nothing on click and the
diagnosis stalled.  Fix: extend the env_logger filter to include
`workspace` at Info as well, and `eprintln!` the
`TOLARIA_BUILD_TAG` banner at startup (bypasses any log filter,
makes it trivial for the user to confirm a fresh binary is being
run — addresses 9.3.5 `Reopened` "still in toolbar" cache
suspicion in the same change).  The four inspector-chain info!
sites now print to the user's terminal under plain `cargo run`:
(1) title-bar click, (2) ToggleInspector handler entry, (3)
toggle_or_swap_right_dock_panel branch + factory invocation, (4)
the InspectorPanel factory body.

**End-to-end test (commit `d9387f49`).**  New
`toggle_inspector_dispatch_chain_attaches_panel_end_to_end` test
in `tolaria/src/main.rs` pins the full production dispatch path
(Window::dispatch_action from inside an active-window update →
App-scope cx.on_action handler → dispatch_to_workspace defer →
toggle_or_swap_right_dock_panel → fresh-attach factory → set_panel)
with per-hop counters (handler_called, workspace_resolved,
factory_called) so a future regression's failure message tells
the developer which hop broke.  Test wraps the workspace in
`gpui_component::Root` to match the production `cx.open_window`
setup (the simpler `cx.add_window(TolariaWorkspace::empty)`
shape used by helper-direct tests bypasses the root downcast in
`dispatch_to_workspace` and would hide regressions there).
`dispatch_to_workspace` promoted from `fn` to `pub(crate) fn` so
the test can register a handler shaped like production.

**Re-closure-4 (commit `c66b6e1a`).**  User's production
stderr trace (2026-05-22) confirmed the dispatch chain runs
**end-to-end successfully**: title-bar click → handler entered →
fresh-attach branch → slot empty → factory invoked.  The panel
**is** constructed and attached — but the user still sees nothing.
Root cause: [`InspectorPanel::render`] at
`inspector_panel/src/lib.rs:1574` set `.h_full()` on its outer
container but **not** `.w_full()` — a flex column with only
`h_full` collapses to content-width along the cross axis, and
when the children also lack explicit widths the whole panel
renders at zero width (visually indistinguishable from "didn't
attach").  Fix: add `.w_full()` alongside `.h_full()` to the
panel's outer div (matching [`SidebarPanel::render`] at
`sidebar_panel/src/lib.rs:1395-1396`).  The dispatch instrumentation
+ end-to-end test from the prior two paragraphs stay green — they
pin the chain itself.  This row finally closes after four
reopens: 9.2.13 (`Regression` after `5a61722e`), `Reopened-2`
(`a71cc191` cx vs window dispatch), `Reopened-3` (this thread —
title-bar toggle didn't open).

#### 9.3.1

**Source:** user-reported polish on the embedded BlockNote editor,
2026-05-21.  **Symptom:** the SideMenu drag handles (`+` add-block
button + `⋮⋮` drag grip) inside the editor-host WebView look like
BlockNote's stock controls — clipped grip glyph, BlockNote's stock
"Colors" pane in the drag-handle menu, jittery HTML-5 dragstart on
reorder — instead of the polished treatment the React app ships
through `src/components/tolariaBlockNoteSideMenu.tsx`.

**Discovery:** the React app replaces BlockNote's default SideMenu
with a 800-line `TolariaSideMenu` component (mounted via
`<SideMenuController sideMenu={TolariaSideMenu} />` at
`src/components/SingleEditorView.tsx:1170`) that wraps the default
`<SideMenu>` with three custom controls — `TolariaAddBlockButton`,
`TolariaDragHandleButton` (the `.tolaria-block-drag-handle` CSS hook),
and `TolariaDragHandleMenu` (markdown-safe Delete + table-header
items, no Colors).  It also runs a pointer-based reorder gesture
inside ProseMirror's `transact()` so the OS-drag flow the WKWebView
mishandles is bypassed entirely, plus a `useSideMenuTextAlignment`
hook that pins the floating menu to the block's first text-line
centre.  The editor-host port had the CSS rules already (ported in
Phase 8's `EditorTheme.css` migration into `editor-host/src/style.css`
— see the `.editor-host-container .tolaria-block-drag-handle` rule
at line 421) but never landed the component; the class never appeared
on any DOM node, so the rules were dead.

**Scope:** verbatim TypeScript port of `tolariaBlockNoteSideMenu.tsx`
into `editor-host/src/tolariaBlockNoteSideMenu.tsx`, mounted via
`<SideMenuController sideMenu={TolariaSideMenu} />` from
`editor-host/src/menus.tsx`.  Phosphor icons are *not* added — the
single-file bundle would balloon ~100 kB.  Two inline SVGs match
the visual weight (`Plus` 20-px line glyph + a 6-dot `DotsSixVertical`
column).  The editor-host's `sideMenuElementForEditor` selector
keys off `.editor-host-container` instead of the React-side
`.editor__blocknote-container` so the alignment math finds the
correct scope.

**Closure (commit `fa740de6`).**  Landed a 600-line
`editor-host/src/tolariaBlockNoteSideMenu.tsx` (verbatim port of
the React component with the inline-SVG icons + scope rename) and
a 175-line vitest at
`editor-host/src/tolariaBlockNoteSideMenu.test.tsx` (mocks
BlockNote's components, asserts the `.tolaria-block-drag-handle`
CSS hook lands on a DOM `HTMLElement` and that the drag-handle
menu carries the markdown-safe `Delete` item rename).  Mount is
the single-line swap in `editor-host/src/menus.tsx` —
`<SideMenuController />` → `<SideMenuController sideMenu={TolariaSideMenu} />`.
Tests: 379 → 381.  Bundle: 2,497,279 → 2,508,200 B (+10,921 B,
+0.44%).  Out of scope: the pointer-reorder geometry + alignment
math are exercised transitively by the React-side
`src/components/tolariaBlockNoteSideMenu.test.tsx` suite (~600
lines of fixtures); duplicating them in the host would just bind
the same logic to the same shape.  The smoke test pins the
*wiring* (CSS hook + DOM shape), not the math.

#### 9.3.2

**Source:** user-reported polish on the right-dock Inspector panel,
2026-05-21.  **Symptom:** the panel opens at a width too narrow to
render Properties / Aliases / etc. comfortably.

**Scope:** raise `InspectorPanel::default_size` to the same width
the sidebar opens at by default (look up `SidebarPanel::default_size`
or the workspace's left-dock `.size(px(...))` initial value in
`crates/workspace/src/workspace.rs:402`).  Aligns the right dock's
opening width with the user's existing left-dock muscle memory.
Surface: `crates/inspector_panel/src/lib.rs` `default_size` impl.
**Size:** trivial.

**Closure (commit `d9766aa5`).**  Introduced a shared
`workspace::workspace::WORKSPACE_LEFT_DOCK_INITIAL_WIDTH_PT` constant
(`200.0`) so the left dock's `.size(px(...))` paint, the right
dock's `.size(px(...))` paint, and `InspectorPanel::default_size`
all read from one source of truth.  The right dock used to mount at
`px(240.0)` and the inspector panel reported `default_size` =
`px(320.0)`; both now resolve to the sidebar's 200-pt baseline so
the user's muscle memory carries across.

**Reopened (2026-05-22)** — user reports the side dock panel is
too narrow.  The `d9766aa5` fix literally satisfied the row spec
("at least the default width of the sidebar"), but 200pt isn't
enough column width for the inspector's property-value pairs to
render comfortably.  React's app defaults to **280pt** for the
inspector (`src/hooks/useLayoutPanels.ts:20`).

**Re-closure (commit `5e8cc075`).**  Added a new
`workspace::workspace::WORKSPACE_RIGHT_DOCK_INITIAL_WIDTH_PT`
constant (`280.0`) independent from the sidebar's 200pt knob.
The workspace render's right-dock `resizable_panel().size(...)`
call now reads the new constant, and `InspectorPanel::default_size`
reads the same constant so both surfaces stay in lockstep.

**Reopened-2 (2026-05-22)** ⏳ — user reports the right dock
still collapses to fit displayed text (≈ 90pt instead of 280pt);
the TOC panel shows the same regression.  Root cause: when the
right dock attaches for the first time, the workspace's panels
vec grows from 3 → 4 entries.  Gpui-component's
`ResizableState::sync_panels_count`
(`gpui-component/.../resizable/mod.rs:124`) extends the new slot
with `PANEL_MIN_SIZE` (100pt), then `adjust_to_container_size`
redistributes column widths by ratio across ALL panels.  With
sizes `[200, 300, ~1000, 100]` summing to 1600pt against a
1500pt container, the new right-dock column ends up at
`1500 * 100/1600 ≈ 94pt`.  The `.size(280)` on the resizable
panel is the *initial* size — only used when the state's
per-panel `size` slot is `None`, but `sync_panels_count`
pre-populates `sizes[3] = PANEL_MIN_SIZE` ahead of the first
render, so the state already has a value that wins over the
initial.

**Re-closure-2 (commit `7ced27dd`).**  Always-push the
right dock into the panels vec (mirrors the left-dock pattern at
`workspace.rs:421-431`).  The panel slot is now stable from the
*first* render — `panels.len()` is always 4 regardless of dock
state — so `sync_panels_count` doesn't late-insert a new slot
with `PANEL_MIN_SIZE`.  Visibility is gated by
`right_dock_visible = self.right_dock.read(cx).active_panel().is_some()`;
when no panel is attached, `.visible(false)` returns a
zero-width div so the editor still flushes to the right edge of
the window.  On first user-driven attach, the resizable layer
reads the panel's `.size(WORKSPACE_RIGHT_DOCK_INITIAL_WIDTH_PT)`
initial size because `panel_state.size` is `None`, opening the
dock at the configured 280pt.  Fixes the TOC panel symptom
simultaneously since both panels mount through the same right
dock.  All 41 workspace + 33 tolaria tests stay green.

**Reopened-3 (2026-05-22)** — user confirms manual resize is
persistent (the 9.3.2 Reopened-2 always-push fix landed cleanly)
but reports the **default** opening width is still too narrow.
The React-side `inspector: 280` baseline doesn't carry over
1-to-1: in the GPUI chrome the inspector renders sections that
need more horizontal room (property-value pairs + wikilink-pill
columns are tighter than the React Mantine equivalents), so
280pt clips real property labels with the 9.2.18 truncate.

**Re-closure-3 (commit `<this-commit>`).**  Bump
`WORKSPACE_RIGHT_DOCK_INITIAL_WIDTH_PT` from `280.0` to `360.0`.
Still comfortably under React's `inspector: 500` max-width cap;
gives property / aliases / "Belongs to" rows enough room to
render labels alongside wikilink-pill values without immediate
`…` truncation.  Manual resize remains persistent across
sessions via the keyed `ResizableState` (untouched by this
change).  Single-constant edit; `InspectorPanel::default_size`
reads the constant via the workspace reexport, so the panel
trait contract stays in lockstep.

#### 9.3.3

**Source:** user-reported polish, 2026-05-21, with attached Image #6.
**Symptom:** the panel currently has no header bar; content starts
at the top edge.

**Scope:** add a header strip to `InspectorPanel` matching the note
toolbar's height (`note_toolbar::NOTE_TOOLBAR_HEIGHT_PT` = 52pt) so
the panel header sits flush with the toolbar across the workspace.
Header content: a `Properties` text label (theme.foreground, same
weight as note-toolbar breadcrumb segments).  Surface:
`crates/inspector_panel/src/lib.rs` render path.  **Size:** small.

**Closure (commit `d9766aa5`).**  Added a 52-pt header strip
to `InspectorPanel::render` via a private `render_header_strip`
helper.  The strip mirrors the note-toolbar chrome
(`theme.background` background, `border_b_1` in `theme.border`) so
the two strips read as one continuous band across the workspace
row.  The `Properties` label sits in the centre slot in
`theme.foreground` at `font_semibold` weight — same weight as the
section labels below.  Required adding `note_item = { path =
"../note_item" }` to `inspector_panel`'s Cargo deps so the height
constant comes from a single source of truth; the edge stays
acyclic (`note_item → workspace`, no path back).

#### 9.3.4

**Source:** user-reported polish, 2026-05-21, with attached Image #6.
**Symptom:** the inspector toggle stays in the note toolbar even when
the panel is open; React's reference moves it into the panel header
when the panel is visible.

**Scope:** when `InspectorPanel` is mounted + open on the right dock,
the panel header (added in `9.3.3`) renders:
- **Left**: the inspector toggle glyph (same `IconName::PanelRight`
  the note-toolbar uses), dispatching `actions::ToggleInspector`.
- **Centre**: `Properties` title.
- **Right**: a close `X` button, also dispatching `actions::ToggleInspector`.
Simultaneously the note-toolbar's inspector cell hides while the
panel is open (read `workspace.is_right_dock_open(cx)` +
`right_dock_panel_key == Some("inspector")` — both accessors already
exist).  When the panel closes, the cell reappears in the toolbar.
**Deps:** depends on `9.3.3` (header strip) shipping first.  **Size:** small.

**Closure (commit `d9766aa5`).**  The header strip (added in
`9.3.3` above) carries both buttons: `IconName::PanelRight` on the
left, `IconName::Close` on the right, both dispatching
`actions::ToggleInspector` via `Window::dispatch_action` (the same
re-entrancy-safe route the `a71cc191` analysis documented).  Both
clicks close the panel because the action is a toggle — symmetry
the user sees as "two ways out".  The original spec called for the
note-toolbar inspector cell to hide while the panel was open;
worklist `9.3.5` overrode that by lifting the toolbar cell to the
title bar entirely, so the show / hide branch is no longer needed
— the title-bar button is the closed-state primary, this header's
buttons are the open-state primary.

#### 9.2.14

**Source:** user-reported, 2026-05-21, against the post-`a71cc191`
build with neighbourhood working end-to-end.  Two paired polish
items on the neighbourhood UX:

1. **Toolbar active-state treatment.**  The `note-toolbar-neighborhood`
   cell currently has no on/off treatment — clicking it sets the
   note-list scope to `NoteListScope::Neighborhood(id, ids)` but
   the toolbar glyph stays muted.  Add active-state styling
   (filled `Map` variant if `gpui-component` has one, OR
   `ActiveStyle::GlyphColor` with theme accent) when the current
   note-list scope is `Neighborhood(id, …)` AND the id matches
   the active item's id.  Mirror the star / organized active-state
   pattern from `9.2.10` + `9.2.11`.  Reading the workspace's
   current note-list scope from the toolbar render path requires
   plumbing — likely a new `&App`-readable accessor on
   `NoteListPane` exposed via the existing pane handle, or a
   shared `Rc<RefCell<Option<NoteId>>>` slot updated by the
   `EnterNeighborhood` handler.

2. **Note-list header shows active note title.**  The `9.2.3`
   closure (commit `13bbc646`) emits a `SidebarSelectionChangedEvent`
   with `display_label = "Neighborhood of <title>"`, but per the
   user's report the header doesn't actually render the note
   title.  Diagnose path: confirm whether the
   `SidebarSelectionChangedEvent` reaches the note-list header,
   and whether the header binds the display_label correctly.
   May be a sibling of the `a71cc191` toolbar-dispatch fix —
   the event may emit but the header may not observe.

**Surface:** `crates/note_item/src/note_toolbar.rs` neighbourhood
cell + `crates/note_list_pane/src/lib.rs` header.  **Size:** small
each; ship as one commit.

**Closure (commit `7d697f5a`).**  Both items landed in one
commit, both backed by tests.

*Item 1 — toolbar active state.*  Introduced
`note_item::NeighborhoodAnchor` as a `Copy + gpui::Global` newtype over
`Option<NoteId>`.  The `EnterNeighborhood` handler in `tolaria::main`
writes `Some(id)` into the global alongside the existing
`set_scope` / `set_header_title` pair, and the
`SidebarSelectionChangedEvent` subscriber clears it on every selection
change (mirrors React's `useNeighborhoodEntry`, which exits
neighbourhood mode on any sidebar pick).  The toolbar render at
`crates/note_item/src/note_toolbar.rs:79` reads the global via
`cx.try_global::<NeighborhoodAnchor>()` and routes the neighbourhood
cell through `toolbar_cell_with_active_color(...)` with
`theme.primary` as the active glyph hue — same shape as the star
cell's `GlyphColor` pattern (worklist 9.2.11).  The handler also
calls `cx.refresh_windows()` because the toolbar observes a global
(not an entity), so a `cx.notify()` on the pane wouldn't repaint it
without a window-wide nudge.

*Item 2 — header label.*  Root cause investigation: the existing
handler already calls `pane.set_header_title(format!("Neighborhood
of {title}"))` — the test
`enter_neighborhood_updates_header_and_anchor` confirms the call
reaches the pane and the next render reads the new title.  The
in-isolation contract is intact; the user's "header still shows
Inbox" report likely traced to the pre-`a71cc191` build where the
toolbar dispatch silently dropped via `App::dispatch_action`.  This
commit pins the contract end-to-end with the new test so any future
regression of the `set_header_title` call surfaces as a failing
assertion rather than a silent UI desync.

*Tests.*  Three new tests:
`note_toolbar::neighborhood_active_color_matches_theme_primary`
(anchor for the colour token),
`note_toolbar::neighborhood_anchor_matches_only_named_id` (the
`NeighborhoodAnchor::matches` truth table),
`note_toolbar::neighborhood_cell_with_active_color_builds_in_both_states`
(both visual states construct cleanly),
`tolaria::tests::enter_neighborhood_updates_header_and_anchor` (the
full action-dispatch pipeline updates scope + header + global), and
`tolaria::tests::sidebar_selection_clears_neighborhood_anchor` (the
exit-mode contract).  Out of scope and deferred: a body-derived
display title for the header (current path uses `Note::title` =
filename stem, which renders as e.g. `"Neighborhood of person-jane"`;
upgrading to the H1 / frontmatter title needs the async
`vault.note_content` read and stays a 9.2.14 follow-up).

#### 9.2.15

**Source:** user-reported, 2026-05-21.  **Symptom:** the View menu's
`Show Inspector` / `Hide Inspector` toggle was repurposed in `9.2.13`
(commit `2662e935`) to drive the product `InspectorPanel` right-dock
mount.  The user wants the menu names to follow the actual surface:
the right-dock panel shows note **Properties** (matching the user's
Image #6 panel title), so the menu item should read **Show Properties** /
**Hide Properties**.  The previous **Show Inspector** / **Hide Inspector**
menu item (which toggled the GPUI element-picker debug overlay) was
removed during the `2662e935` cleanup — restore it under that name
so the developer overlay regains a discoverable surface.

**Scope:**
1. `crates/tolaria/src/menus.rs`: rename the existing `Show/Hide Inspector`
   View-menu entry to `Show/Hide Properties`, still dispatching
   `actions::ToggleInspector` (the product right-dock action).  Driver
   stays the existing `MenuState::inspector_picking` (or whatever the
   field is named after `2662e935`) — semantics already track the
   right-dock open state.
2. Add a separate `Show/Hide Inspector` View-menu entry dispatching
   `actions::ToggleElementInspector` (the GPUI debug overlay action;
   `Cmd+Alt+I` accelerator stays).  Driver is
   `Window::is_inspector_picking` (the GPUI accessor that flips when
   the debug overlay is active).
3. `MenuState` may need a second `inspector_overlay_picking: bool`
   field to drive the new entry's label flip independently from the
   product panel's state.

**Surface:** `crates/tolaria/src/menus.rs` + the workspace's
`rebuild_menus_with_workspace` call sites in `tolaria/src/main.rs`.
**Size:** small.

**Closure (commit `43a9fcab`).**  `MenuState::inspector_picking`
renamed to `properties_open` (drives the product right-dock toggle's
label).  New `MenuState::inspector_overlay_picking` field drives the
restored GPUI element-picker entry's label.  View menu now renders
both entries: `Show / Hide Properties` (dispatches
`actions::ToggleInspector` → right-dock panel) and a separate
`Show / Hide Inspector` (dispatches `actions::ToggleElementInspector`
→ GPUI debug overlay, `Cmd+Alt+I`).  `rebuild_menus_with_workspace`
populates both fields — `properties_open` from
`workspace.is_right_dock_open(cx) && panel_key == Some("inspector")`,
`inspector_overlay_picking` from `Window::is_inspector_picking`
(debug-only; field stays `false` in release builds).  Tests extended
to cover all four state-axis combinations (both closed, both open,
only properties, only overlay).

#### 9.2.16

**Source:** user-filed, 2026-05-21.  **Symptom:** clicking the
neighbourhood toolbar button always SETS the neighbourhood scope
(per `9.2.3`'s `EnterNeighborhood` handler).  There's no way to
exit neighbourhood mode from the same button — the user has to
click another sidebar row.  The user wants the toolbar cell to
behave as a true toggle: click once → enter neighbourhood; click
again → exit back to the previous scope.

**Scope:**
1. The `EnterNeighborhood` handler at `tolaria/src/main.rs:1019`
   currently always writes `NoteListScope::Neighborhood(id, …)`.
   Add an "exit if currently in this neighbourhood" branch:
   read the current `NoteListScope`; if it's
   `Neighborhood(active_id, …)` matching the active note's id,
   pop back to whatever the previous scope was (or default to
   `NoteListScope::Inbox` / `AllNotes` — pick whichever feels
   natural; React's `useNeighborhoodSelection` pops via
   `nav_history`).
2. The `NeighborhoodAnchor` global (introduced in `9.2.14` /
   commit `7d697f5a`) flips alongside the scope — when exiting,
   set `*anchor = None` so the toolbar cell deactivates.
3. The previous-scope memory: simplest is a small `Rc<RefCell<NoteListScope>>`
   slot updated by the handler before setting Neighborhood, and
   read on exit to restore.  Acceptable size; Phase 10's
   `nav_history` will replace it.
4. The note-list header should also pop back from "Neighborhood of <title>"
   to the previous scope's header label on exit.

**Surface:** `tolaria/src/main.rs` `EnterNeighborhood` handler;
optional small state in `note_item` or a new slot.  **Size:** small.

**Closure (commit `fa740de6`).**  Extracted the handler body
into a `handle_enter_neighborhood(active_note_item, note_list,
prev_scope, cx)` free function in `tolaria/src/main.rs::macos` so
both the production `cx.on_action` closure and the new
`#[gpui::test]`s reach the same code path.  A new
`Rc<RefCell<Option<NoteListScope>>>` "prev-scope" slot — owned by
`fn run` alongside `active_note_item` — backs the toggle memory.
Branch logic: read current pane scope; if it's
`NoteListScope::Neighborhood(anchor, _)` with `anchor == active_id`,
restore the saved scope (falling back to `Inbox` if none) + clear
`NeighborhoodAnchor` to `None`; otherwise save current scope and
set the new neighbourhood scope as before.  Two new
`#[gpui::test]`s — `neighborhood_handler_enters_when_scope_is_not_neighborhood`
and `neighborhood_handler_exits_when_scope_matches_active_id` —
pin the enter and exit branches against an on-disk vault fixture.

#### 9.2.17

**Source:** user-filed, 2026-05-22.  **Symptom:** the
`note-toolbar-width` cell (between the raw-mode and AI cells)
has been a `stub_cell` since Phase 8 — clicking it does nothing.
The React app's `BreadcrumbBar.tsx::NoteWidthAction` toggles
between a constrained reading column (`normal`) and an
unconstrained "wide" column.  User wants the toggle to actually
work in the GPUI chrome.

**Scope:** mirror the worklist `9.2.4` raw-mode pattern:

1. Add `actions::ToggleNoteWidth` action verb.
2. Add `editor_bridge::ToHost::SetWideMode(SetWideMode { wide:
   bool })` envelope variant — wire format
   `{"k":"set_wide_mode","v":{"wide":true}}`.
3. Add `NoteItem::wide_mode: bool` field + `wide_mode()` getter
   + `toggle_wide_mode(cx)` method (flip flag, push bridge
   envelope, `cx.notify()`).  Reset to `false` on
   `open_in_webview` so each tab opens narrow (matches React's
   per-component state shape).
4. Swap the `note-toolbar-width` `stub_cell` for a
   `toolbar_cell_with_active` (same shape as raw-mode + ToC) that
   paints in `ActiveStyle::Tint` when wide and dispatches
   `ToggleNoteWidth`.  Tooltip flips text between "Use wide note
   width" and "Use narrow note width".
5. App-scope `cx.on_action(|_: &ToggleNoteWidth, cx| ...)` handler
   in `tolaria::macos::run` reads the active `NoteItem` from
   `active_note_item` slot, logs the transition at `debug!`, and
   calls `toggle_wide_mode`.  Same active-item shape the raw
   handler uses.
6. Editor-host `bridge.ts` adds the `set_wide_mode` variant to
   the `ToHost` union; `EditorApp.tsx` `applyToHost` handler
   toggles `.wide-mode` on `.editor-host-container`.
7. `editor-host/src/style.css` adds
   `.editor-host-container.wide-mode .bn-editor { max-width: none; }`
   to lift the `--editor-max-width` constraint.

**Out of scope:** per-note frontmatter persistence
(`_note_width_mode`).  The chrome-side toggle is in-memory only;
each `open_in_webview` resets to narrow.  React stores the
setting in frontmatter — a follow-up row can wire that read
without touching the toggle path.

**Closure (commit `55561ed7`).**  Shipped all 7 scope items.
Bridge tests added: `to_host_set_wide_mode_roundtrip` +
`to_host_set_wide_mode_disabled_roundtrip` (both ways across the
JSON envelope, including the explicit `false` field).  NoteItem
tests added: `toggle_wide_mode_flips_the_flag` (false → true →
false round trip) + `wide_mode_defaults_to_false`.  Active-state
glyph treatment uses the same `ActiveStyle::Tint` as raw-mode +
ToC so the toolbar reads consistently.  No new Phosphor icons —
`IconName::Maximize` (already in the stub) doubles as the
wide-mode glyph.  Bundle: 2,492.64 kB → 2,492.87 kB (+0.23 kB
from the bridge handler + CSS rule).  Rust test counts grew by
2 in `note_item` (47) and 2 in `editor_bridge` (45).

#### 9.2.18

**Source:** user-filed, 2026-05-22.  **Symptom:** the
`note_list_pane` header strip's title element has no
`text-overflow` handling.  With `9.3.8` closed, the
neighbourhood-mode header reads the active note's H1 /
frontmatter display title, which can be arbitrarily long — and a
title past the strip's width currently either wraps (blows the
52pt header height) or pushes the right-side controls cluster
(sort / search / `+`) out of view.

**Scope:** the title `div` at
`crates/note_list_pane/src/lib.rs:1494` lives inside an
`h_flex().justify_between()` next to the controls cluster.  Add
`.flex_1()` + `.min_w_0()` + `.truncate()` so the title:
1. Grows to fill remaining row width (`flex_1`).
2. Can shrink below its content's natural width (`min_w_0`).
3. Truncates overflow with a trailing `…`
   (`truncate` composes `overflow_hidden + whitespace_nowrap +
   text_ellipsis` from gpui's `Styled`).

Sidebar / view labels (Inbox, Favorites, Archive, type names ≤16
chars) never trigger the truncate path, so the visual is
unchanged for those scopes — only over-long neighbourhood
titles get the `…` treatment.  Surface: 3 chained method calls
+ a comment paragraph.  **Size:** trivial.

**Closure (commit `54e7df0e`).**  Applied
`.flex_1().min_w_0().truncate()` to the title element at
`note_list_pane/src/lib.rs:1494`.  37 note_list_pane tests stay
green.  No new test added — the change is a styling chain that
GPUI doesn't surface back to test queries; visual confirmation
needs a periscope screenshot the user can drive in the running
app.

#### 9.2.19

**Source:** user-filed, 2026-05-22.  **Symptom:** worklist 9.3.5
moved the inspector toggle off the note toolbar onto the
workspace title bar.  The user now wants the per-note toolbar
button back — both surfaces should carry the toggle, mirroring
the React-era `BreadcrumbBar` which always had this cell, while
the title-bar primary added in 9.3.5 stays.

**Scope:** restore a `note-toolbar-inspector` cell to the
note-toolbar's right cluster, just before the
more-overflow popover.  Same shape as the toc / reveal /
copy-path cells — plain `toolbar_cell` (no active-state
treatment since right-dock-open state is workspace-level, not
per-note), `IconName::PanelRight` glyph (matches the title-bar
primary), tooltip "Toggle inspector", dispatches
`actions::ToggleInspector` via `Window::dispatch_action` (the
re-entrancy-safe route documented in the 9.2.3 / 9.2.6 cells).
Surface: 1 chained `.child(toolbar_cell(...))` call between
`note-toolbar-copy-path` and `more_overflow_cell`.  **Size:**
trivial.

**Closure (commit `<this-commit>`).**  Inserted the
`note-toolbar-inspector` cell at
`crates/note_item/src/note_toolbar.rs` between copy-path and
more.  Glyph: `IconName::PanelRight`; tooltip: "Toggle
inspector"; dispatch: `Window::dispatch_action(Box::new(
actions::ToggleInspector), cx)` with the same `debug!` log
target (`note_item::toolbar`) the other cells use.  Both
affordances now coexist — the title-bar primary stays (always
visible regardless of which note is open), the toolbar cell
sits in per-note context next to the other note-level actions
(ToC, reveal, copy path).  47 note_item tests + 41 workspace
tests stay green.

#### 9.3.7

**Source:** user-filed, 2026-05-21.  **Symptom:** the BlockNote
selection menu (the floating formatting toolbar that appears when
the user selects text) renders with BlockNote's stock styling
instead of the React app's polished treatment (custom colours,
spacing, hover states).  Mirror the 9.3.1 work: the React side has
a custom `tolariaBlockNoteFormattingToolbar*` (or similar) that
overrides BlockNote's default; port it to `editor-host/`.

**Scope:**
1. Find the React-side custom formatting toolbar — likely
   `src/components/blockNoteFormattingToolbar*` (the worklist
   `8.2.25` work referenced `blockNoteFormattingToolbarHoverGuard`).
2. Port to `editor-host/src/` if the React component supplants
   BlockNote's default toolbar.  If only CSS, port the styles to
   `editor-host/src/style.css`.
3. Mount via `FormattingToolbarController` in
   `editor-host/src/menus.tsx` (same shape as the `SideMenuController`
   mount for `9.3.1`).
4. Vitest covering wiring (DOM class / mount).

**Surface:** `editor-host/` TS+CSS.  **Size:** medium.

**Closure (commit `140fb64c`).**  Mirrored 9.3.1's SideMenu port for
the BlockNote selection menu / formatting toolbar.  Two new editor-host
files: `tolariaBlockNoteFormattingToolbar.tsx` (mantine-free port of
the React `tolariaEditorFormatting.tsx` — `TolariaBasicTextStyleButton`,
`TolariaBlockTypeSelect`, `TolariaFileDownloadButton`,
`TolariaFormattingToolbar`, `TolariaFormattingToolbarController`,
the close-grace + deduped-toolbar-store + hover/focus tracking the
React controller carries), and the 1-line mount swap in
`editor-host/src/menus.tsx`.  No `Config.ts` companion file — the
filter list + block-type rows are <30 lines combined and read
better co-located with the components that consume them.
Mantine-free port: the Mantine `<Menu>` powering the
BlockTypeSelect dropdown swapped to BlockNote's vanilla
`Components.Generic.Menu.Root/Trigger/Dropdown/Item` (same
primitives the 9.3.1 SideMenu port uses for its drag-handle menu),
the Mantine `<Button>` trigger swapped to
`Components.FormattingToolbar.Button`, and the `MantineCheckIcon`
swapped to an inline-SVG checkmark.  Filtered keys: `underlineStyleButton`,
`textAlignLeftButton`, `textAlignCenterButton`, `textAlignRightButton`,
`colorStyleButton` (all five removed by
`filterTolariaFormattingToolbarItems` before mount).  Inline-code
button inserted after the strike button by `insertInlineCodeButton`
(pinned by a vitest case).  Hover guard: the existing
`useBlockNoteFormattingToolbarHoverGuard` hook (ported in 8.25 and
already passing 11 tests) is now driven by the controller's full
`isOpen` signal (composition + focus + close-grace) instead of the
"any file block selected" approximation `menus.tsx` previously used;
the duplicate wiring in `menus.tsx` was removed.  File download:
`window.open(url, '_blank', 'noopener,noreferrer')` instead of
the React app's `openEditorAttachmentOrUrl` shell-IPC bridge — TODO
to wire a `FromHost::OpenAttachment` bridge message and route
vault-relative URLs through the host when that bridge lands.  CSS:
no new `bn-formatting-toolbar*` rules needed — the only React-side
selector (`.editor__blocknote-container :is(.bn-toolbar,
.bn-formatting-toolbar, .bn-menu-dropdown, .bn-grid-suggestion-menu)
button svg`) was already mirrored at
`editor-host/src/style.css:447`; Tolaria-specific CSS hooks
(`.tolaria-format-{bold,italic,strike,code,file-download}`,
`.tolaria-block-type-select`) are new but optional — they exist
for downstream stylesheets to target without depending on
BlockNote's internal class names.  Phosphor icons replaced with
six 16-px inline SVGs (Bold, Italic, Strikethrough, Code,
ExternalLink, CaretDown) plus block-type select icons (Paragraph,
H1-H6, Quote, BulletList, NumberedList, Checklist, CodeBlock) —
zero added package weight.  Mount: `editor-host/src/menus.tsx`
swapped `<FormattingToolbarController />` for
`<TolariaFormattingToolbarController />`; the file-block hover-guard
plumbing that 8.25 wired here moved into the controller (which
sees the richer `isOpen`).  Tests: four new vitest cases in
`tolariaBlockNoteFormattingToolbar.test.tsx` — (1) `filterTolariaFormattingToolbarItems`
drops all five unsupported keys and preserves the supported ones in
order; (2) `insertInlineCodeButton` inserts the inline-code button
immediately after strike and trails fileDownload; (3) the insert is
a no-op when no strike button is present; (4) `<TolariaFormattingToolbar />`
mounts without crashing, the BlockTypeSelect trigger lands as a
`.tolaria-block-type-select` DOM element, the inline-code button
lands as a `.tolaria-format-code` DOM element, and no
`.tolaria-format-underline` element appears.  Tests: 381 → 385.
Bundle: 2,478.72 kB → 2,491.59 kB (+12.87 kB, +0.52%).
`pnpm build` + `pnpm test` clean.  Visual diff against the React app
was not run (no live native build in this session).  Out of scope:
the file-download bridge (TODO above) and the `floatingUIOptions`
`onMouseDownCapture` plumbing the React `SingleEditorView` mount
adds — neither is needed for the visual-parity row, both stay
deferred until a follow-up task names them.

#### 9.3.5

**Source:** user-reported polish, 2026-05-21.  **Symptom:** the
Inspector toggle currently lives on the note toolbar.  The user
wants it in the title bar's right corner, mirroring the sidebar
toggle that lives in the title bar's left corner — a workspace-level
chrome affordance, not a per-note one.

**Scope:** add a toggle button to the workspace's title bar
(`crates/workspace/src/title_bar.rs`) on the right side, glyph
`IconName::PanelRight`, dispatching `actions::ToggleInspector`
(the product right-dock action).  Mirror the existing sidebar-toggle
button shape (icon, hover state, tooltip).  When the user wants
this, the note-toolbar inspector cell can stay (redundant
affordance) OR be removed (cleaner) — drop it; the title-bar
affordance is the new primary.  Coordinates with `9.3.4`
(toggle migrates to panel header WHEN OPEN) — the title-bar
button is the closed-state affordance, the panel-header X is the
open-state affordance.  **Size:** small.  **Deps:** none, but
coordinate sequencing with `9.3.4`.

**Closure (commit `d9766aa5`).**  Added a
`title-bar-toggle-inspector` cell to the right cluster of
`workspace::title_bar`, mirroring the existing
`title-bar-toggle-sidebar` cell on the left.  Same shape (28x20-pt
hit target, `rounded_sm`, 12 % grey hover overlay,
`Tooltip::new("Toggle inspector")`).  Dispatches
`actions::ToggleInspector` via `Window::dispatch_action` (the
re-entrancy-safe route the `a71cc191` analysis pinned).  The
note-toolbar's `note-toolbar-inspector` cell was removed in the
same commit — title-bar = closed-state primary, panel header =
open-state primary (per `9.3.4`).  Updated the actions docstring
and the `tolaria::main` mount comment so future grep traces land
on the new dispatch sites.

**Reopened (2026-05-21)** ⏳ — user reports "the icon is still in
the note toolbar."  Source code at `crates/note_item/src/note_toolbar.rs`
no longer adds a `note-toolbar-inspector` cell (verified by
greppping); the cell list ends at `note-toolbar-copy-path` +
the More-overflow popover.  Likely causes:
1. **Stale binary.**  User testing against a pre-`d9766aa5` build;
   `cargo run` should pick up the fresh source but a stale
   incremental build cache could be returning the old binary.
   Fix path: ask the user to `cargo clean -p tolaria && cargo run`.
2. **Title-bar toggle invisible at user's window size.**  The
   `title-bar-toggle-inspector` is in the right cluster at
   `title_bar.rs:189`; if the title-bar's right cluster is clipped
   by some other element OR the title-bar wraps, the user might
   not see it and conclude the inspector "is still in the toolbar"
   when they actually mean "the toolbar is the only place I can
   see it."
3. **Some other cell paints with the inspector glyph.**
   `IconName::PanelRight` is the same glyph the sidebar-toggle
   uses on the left — confusion with that is possible.
Diagnosis: confirm via screenshot whether (a) the title-bar
button is visible OR (b) any toolbar cell uses
`IconName::PanelRight`.

**Build-tag banner (commit `148378eb`).**  To unblock the
"stale binary" hypothesis the user can't easily check on their
own, the `tolaria::macos::run` entrypoint now emits a clear
`=== tolaria build=<TOLARIA_BUILD_TAG> ===` banner to stderr via
`eprintln!` (bypasses any RUST_LOG / env_logger filter).  The
banner is the first line of stderr on every launch, so a triage
screenshot of the terminal prove-or-disproves the stale-cache
suspicion in one glance — no need to ask the user to
`cargo clean` blindly.  Same change also extends the env_logger
filter to include `workspace` at Info (worklist 9.2.13 cross-row
fix), so the title-bar click log now appears under plain
`cargo run`.

**Re-closure (2026-05-22).**  User's production stderr trace
(2026-05-22) confirmed a fresh binary: the build-tag banner
prints `=== tolaria build=v0.1.0 git:tolaria ===` and the
title-bar inspector click reaches the on_click closure (logged
via `workspace::title_bar` "title-bar inspector click →
dispatching ToggleInspector").  The toolbar inspector cell was
removed in `d9766aa5` and `crates/note_item/src/note_toolbar.rs`
no longer references `note-toolbar-inspector` (verified at the
source level + re-confirmed by the user clicking the new
title-bar primary instead of the old toolbar cell to reproduce
9.2.13).  Of the three reopened hypotheses — (1) stale binary,
(2) title-bar toggle invisible, (3) some other cell paints the
glyph — none survived: the banner ruled out (1), the click trace
ruled out (2) + (3).  Closing.

#### 9.3.6

**Source:** user-reported polish, 2026-05-21.  **Symptom:** the
note-toolbar emits `info!`-level logs on every click (added in
`a71cc191` to make the dispatch chain observable during the four-
regression debug session).  Now that the dispatch path is wired
correctly, the per-click traces are noise at `info!`.

**Scope:** downgrade the four `note_item::toolbar` `info!`
"click registered" log lines (neighborhood, raw, toc, inspector)
to `debug!`.  Keep the per-handler `info!` traces at
`tolaria::*` (those are useful when diagnosing a future regression
and don't fire as often).  Surface: `crates/note_item/src/note_toolbar.rs`
toolbar cells.  **Size:** trivial.

**Closure (commit `d9766aa5`).**  Downgraded the three
remaining `info!` "click registered" lines (neighborhood, raw,
toc) at `note_item::toolbar` to `debug!`.  The fourth (inspector)
went away with the cell removal in `9.3.5` — no log to downgrade
there.  The per-handler `info!` traces at `tolaria::*` stay
unchanged so a future dispatch regression still surfaces in
production logs without the per-click noise.

#### 9.3.8

**Source:** user-filed, 2026-05-22.  **Symptom:** the note-list
header in neighbourhood mode reads `Neighborhood of <title>`
(e.g. `Neighborhood of My Note`).  The user wants the header to
just show the active note's title alone — the prefix verbiage
adds clutter, and the note-list pane already styles its header
the same way (large, dense title text) for the inbox view, so a
title-only label reads as a direct echo of "which note are we
showing the neighbourhood of."

**Scope:** drop the `"Neighborhood of "` prefix from the
`set_header_title` call in `tolaria::macos::handle_enter_neighborhood`
(`crates/tolaria/src/main.rs`, near the `format!(...)` call) and
pass the bare `title` `SharedString`.  Update the related test
(`enter_neighborhood_updates_header_and_anchor`) and the
neighborhood-handler test (`neighborhood_handler_enters_when_scope_is_not_neighborhood`)
to assert the title-only label.  Surface: 1 fn body + 2 test
assertions.  **Size:** trivial.

**Closure (commit `55561ed7`).**  Replaced
`gpui::SharedString::from(format!("Neighborhood of {title}"))`
with `title.clone()` in `handle_enter_neighborhood`.  Both
affected tests now assert against `anchor_stem.as_ref()` directly.
The 9.2.14 closure paragraph already credited the note-list
header to "show the active note's title"; this row finishes the
job by dropping the prefix from the literal.  Closure docstring
on the test was also updated.  31 tolaria tests + 33 tests in
the surrounding crates stay green.

**Reopened (2026-05-22)** ⏳ — user reports the header reads the
note's **ID** (file stem like `20251031-meeting-notes`) not its
**title** (the H1 / frontmatter display name).  Root cause:
`vault::Note::title` is built from the file stem at
`crates/vault/src/lib.rs:855` (`path.file_stem()`), NOT from the
H1 / frontmatter.  The note-list rows use a different code path
(`note_list_pane::collect_vault_entries` at line 647) that
prefers `extract_title(body)` (first `# H1`, else frontmatter
`title:`) and falls back to the file stem only when those are
absent.  Two display-title surfaces ⇒ two answers for the same
note.

**Re-closure (commit `7ced27dd`).**  Make
`note_list_pane::extract_title` `pub` and call it from
`handle_enter_neighborhood`: load the note body via
`vault::Vault::note_content` (blocking on the foreground
executor, same shape `collect_vault_entries` uses), feed it
through `extract_title`, fall back to `Note::title` when the
extractor returns `None`.  Now both surfaces (note-list row +
neighbourhood header) read the same display title — the H1 /
frontmatter title where present, file stem otherwise.  Both
affected tolaria tests updated: the fixture writes
`# B\nbody\n` to `b.md`, so the extractor returns `"B"` while
`Note::title` is `"b"`; the assertion variable was renamed from
`anchor_stem` to `anchor_display_title` to reflect that switch.
33 tolaria tests stay green.

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
