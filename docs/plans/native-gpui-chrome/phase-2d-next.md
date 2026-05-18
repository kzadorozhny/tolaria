# Phase 2d — Big panel crates (planning outline)

Last shipped: Phase 2c at commit `3131ccc7`.  Phase 2a/2b/2c built the
topology + small chrome + wiring.  Phase 2d builds the substantial
panel crates that live inside the Docks and the center PaneGroup.

## Scope

Seven new chrome crates, each implementing the `workspace::Panel` trait
(panels) or rendering inside `Pane` (content):

| Crate | Owns | Mock source |
|-------|------|-------------|
| `sidebar_panel` | Left Dock; types + saved views + folders + workspace selector | MockVault |
| `note_list_pane` | Center pane content; filter + list + bulk action bar | MockVault + MockSearch |
| `inspector_panel` | Right Dock; 7 sub-panels (properties / outline / backlinks / instances / referenced-by / relationships / git history) | MockVault + MockGit |
| `ai_panel` | Right Dock companion; AI chat surface | MockAi |
| `search_panel` | Bottom Dock; full-text search results | MockSearch |
| `settings_panel` | Modal/workspace item; settings UI | MockSettings |
| `diff_view` | Modal/panel; diff renderer | MockGit |

## Sequence

These can largely proceed in parallel.  Suggested wave structure:

- **Wave 1** (smallest, most isolated): `search_panel`, `diff_view`, `settings_panel`
- **Wave 2** (Dock panels — implement `Panel` trait): `sidebar_panel`, `ai_panel`
- **Wave 3** (largest — 7 sub-panel composition): `inspector_panel`, `note_list_pane`

Each wave: parallel builders + centralized reviewer (pattern from Phase 2b).

## Per-crate guidance

### sidebar_panel
- Implement `workspace::Panel` (position=Left, default_size=240px, starts_open=true).
- Render `gpui_component::Sidebar` primitive with three sections:
  - **Types**: list MockVault's distinct NoteKind values with counts.
  - **Saved views**: synthetic list of 3–5 demo views.
  - **Folders**: tree from MockVault note paths grouped by directory.
- Selecting an item emits an event the workspace can subscribe to.

### note_list_pane
- Implement `workspace::Panel` (custom or center; check Zed pattern).
- Filter bar at top (placeholder for FilterBuilder Phase 2e gap).
- Virtualised list (defer real virtualization — eager render for Phase 2d).
- Bulk action bar appears when ≥1 item selected.

### inspector_panel
- Implement `workspace::Panel` (position=Right, default_size=320px, starts_open=true).
- 7 sub-panels via `gpui_component::Accordion`:
  1. Properties (key/value table from MockNote.properties)
  2. Outline (extracted headings from content)
  3. Backlinks (synthetic — 2-3 fake links)
  4. Instances (count of NoteKind matches)
  5. Referenced-by (inverse of backlinks)
  6. Relationships (synthetic graph)
  7. Git history (last 5 MockCommits)

### ai_panel
- Implement `workspace::Panel` (Right Dock companion — alternate visibility with inspector).
- Renders MockAi thread (4 turns) using gpui-component's chat-style layout.
- Send-message input at bottom; on enter, calls `MockAi::send_message`.

### search_panel
- Implement `workspace::Panel` (position=Bottom, default_size=200px, starts_open=false — toggle via action).
- Search input + result list with snippet excerpts from MockSearch.

### settings_panel
- `Panel` OR `ModalView` — decision: ModalView (matches the React `Dialog`-based pattern).
- Multi-tab settings UI: General, Editor, Git, AI, Vault.
- Each tab reads from MockSettings; Phase 3 wires to real `settings_store`.

### diff_view
- `ModalView` (full-screen).
- Renders MockGit diff (synthetic — two side-by-side text panes).

## Hard rules (carry-over from Phase 2b)

- Each crate is self-contained; no cross-panel deps.  Plumbing through
  TolariaWorkspace events is Phase 2e or later.
- Every crate's tests use `install_theme(cx)` helper pattern from
  `crates/embed_poc/src/layout.rs:243`.
- Mock services accessed via the `Global` accessor pattern; never
  hold mock data inline in panel state.
- Builders use the parallel team pattern (4 in parallel for Wave 1,
  then 2 + 2 + 2 for Waves 2/3).
- Centralized reviewer (single teammate, hand off as crates finish).
- `cargo fmt` + per-crate test green + workspace clippy `-D warnings`
  before any commit.
- `cargo build --workspace` clean per crate landing (no cascade
  breakage on tolaria, embed_poc, or sibling chrome crates).

## Phase 2e — Remaining surfaces (preview)

Modals and small composers that round out the chrome inventory:

- `command_palette` — `Picker<CommandPaletteDelegate>` modal (uses `ui::Picker`)
- `quick_open` — `Picker<QuickOpenDelegate>` modal
- `dialogs` — all 11 plan-locked dialog views (Commit, ConfirmDelete, CreateNote, CreateType, CreateView, Feedback, McpSetup, TelemetryConsent, GitRequiredModal, ConflictResolverModal, AddRemoteModal, CloneVaultModal, NoteRetargetingDialogs, RetargetNoteDialog, OnboardingShell)
- `wikilink_inputs` — `Picker`-based combobox for wikilinks
- `image_lightbox` — full-screen image viewer modal
- `emoji_picker` — popover grid
- `startup` — WelcomeScreen + StartupScreen views

These mostly compose existing primitives (`Picker`, `Dialog`, `Popover`)
so each crate should land in <300 LOC.
