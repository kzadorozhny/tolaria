# ADR-0115 — Next Steps: Phases 1–2 Plan

## Context

ADR-0115 (`docs/adr/0115-native-gpui-chrome.md`) replaces the Tauri shell with native GPUI chrome while keeping BlockNote + CodeMirror as an embedded per-note WKWebView. **Phase 0 is done**: `crates/embed_poc/` ships the spike that validates the four ADR-0115 re-evaluation triggers (focus handoff, IME, frame sync, Cmd+S delivery) with 26 in-process GPUI tests green.

This plan covers what comes next. The goal is a dogfoodable native macOS shell on `feat/native-gpui-chrome` with the Tauri stack still alive in parallel until the cut-over (Phase 6). Decisions baked in based on user input:

- **Crate naming**: no prefixes (Zed style). `workspace`, `actions`, `vault`, not `tolaria_workspace`. Deviates from ADR §1 but matches the reference codebase.
- **Phase order**: chrome-first. Native shell with mock-fixture data before porting services or wiring the editor host.
- **Branch policy**: ADR-0021 push-to-`main`; all intermediates land on `feat/native-gpui-chrome` and are dogfood-only.

---

## Top-Level Roadmap

| # | Name | Scope | Exit criterion |
|---|------|-------|----------------|
| 1 | **Foundation** | Empty `tolaria` binary. Workspace skeleton, theme, settings, keymap, action registry, native menu. No panels, no services, no editor. | `cargo run -p tolaria` opens themed empty window with native menu; Cmd+Q quits via action; settings round-trips; `cargo test --workspace` green. |
| 2 | **Chrome surfaces against mocks** | `TolariaWorkspace` with 3 Docks + status bar + command palette + quick-open + dialogs + banners + toasts + modals, all rendering against `mock_fixtures`. No services. No editor pane. | `TOLARIA_MOCK=1 cargo run -p tolaria` shows full chrome with mock vault; every action reachable; every dialog/banner invocable; ~150+ tests green. |
| 3 | **Services migration** | Repackage `src-tauri/{vault,git,frontmatter,search,vault_watcher,vault_list,settings,mcp,app_updater}` + CLI agents into `vault`, `git_provider`, `vault_search`, `vault_watcher`, `cli_agents`, `mcp_bridge`, `telemetry`, `app_updater`, `localization`, `settings_store` crates as `Global` services and `Entity<T>` handles. Chrome swaps mocks for live services. `Task<Result<T>>` + `EventEmitter` replace IPC. | Real vault opens; sidebar/note-list/inspector show live data; search, git, AI agents operate end-to-end (note open shows placeholder editor body). |
| 4 | **Editor host integration** | Stand up `editor-host/` Vite project; `crates/editor_bridge` JSON envelope; `crates/note_item` `Item` with per-note `WKWebView` via `gpui-wry` (LRU cap 10); reuse `InstrumentedWebView` frame-sync pattern from Phase 0. | Open a note, edit, Cmd+S persists via `vault` service; multi-tab `Pane` works; one WKWebView per note. |
| 5 | **Parity hardening** | Autogit checkpoints, conflict resolver, telemetry, app updater, onboarding, localization. Measurement gate (memory, startup time). | Native shell passes existing Playwright editor-body smoke + fresh native chrome QA. |
| 6 | **Cut-over** | Delete `src-tauri/`; prune `src/` to editor-host carry-overs; flip superseded ADRs (0001, 0003, 0030, 0052, 0053, 0079, 0080, 0083, 0104, 0106); rewire signing + `script/bundle-mac`; reset `.codescene-thresholds` per ADR-0064. | Single signed/notarized `.app` ships from new pipeline; `src-tauri/` removed in same commit. |
| 7 | **Post-cutover follow-up** | Re-enable Windows/Linux behind feature flags; iPad strategy; potentially start native-GPUI editor R&D. | Tracked as separate ADRs. |

---

## Phase 1 — Foundation (deep plan)

### New crates (all under `crates/`, snake_case, no prefix)

| Crate | Type | Purpose | Key deps |
|---|---|---|---|
| `tolaria` | bin | App entry. `main.rs` builds App, installs Globals, registers actions, opens root window. | `gpui`, `gpui_platform`, `gpui-component`, `workspace`, `actions`, `settings_store`, `theme`, `ui`, `paths` |
| `paths` | lib | App data/config dirs, vault paths. Zed has a crate of the same name. | `dirs`, `anyhow` |
| `theme` | lib | `Theme` global wrapping `gpui_component::Theme`; light/dark; observable. | `gpui`, `gpui-component` |
| `settings_store` | lib | `impl Global` for `SettingsStore`; JSON on disk; `cx.observe_global` notifications; schema versioning. | `gpui`, `serde`, `serde_json`, `paths` |
| `actions` | lib | `actions!()` registry (`NewNote`, `Save`, `QuickOpen`, `CommandPalette`, `ToggleSidebar`, `ToggleInspector`, `Quit`, `CloseTab`, …); keymap JSON loader. Replaces ADR-0106's manifest. | `gpui`, `serde`, `serde_json` |
| `ui` | lib | Tolaria-specific compounds not in `gpui-component` (RichTooltip-with-shortcut, IconPicker over Phosphor, ShortcutBadge, FocusRing). No app logic. | `gpui`, `gpui-component` |
| `workspace` | lib | `TolariaWorkspace` skeleton root view; `ModalLayer`, `ToastLayer`, native title bar wrapper. Phase 1 ships empty (title bar + centered placeholder). Docks/Panels expand in Phase 2. | `gpui`, `gpui-component`, `theme`, `actions` |

### `crates/tolaria/src/main.rs` registration sequence

Order matters; Globals must exist before observers/views read them. Mirrors `crates/embed_poc/src/main.rs:48–90` and `/Users/konstantin/zed/crates/zed/src/main.rs:481–860`.

1. `env_logger` init.
2. `gpui_platform::application().run(|cx| { … })`.
3. `gpui_component::init(cx)` (required before `h_resizable`, see `embed_poc/src/layout.rs:243`).
4. `theme::init(cx)` installs `Theme` global.
5. `paths::init(cx)` resolves `~/Library/Application Support/Tolaria/`.
6. `settings_store::SettingsStore::load_and_install(cx)` — reads `paths::settings_file()`, sets `Global`; defaults on miss.
7. `actions::init(cx)` declares the `actions!()` set; loads bundled `default.json` keymap + user override.
8. Global handlers: `cx.on_action(|_: &Quit, cx| cx.quit())` etc. (pattern from `embed_poc/src/main.rs:62`). Phase 1 wires only `Quit`, `CloseWindow`, `OpenSettings`, `ReloadKeymap`.
9. `cx.bind_keys([...])` from the loaded keymap. Edit-menu chords stay `OsAction` per `embed_poc/src/menus.rs:54–62`.
10. `cx.set_menus(menus::app_menus())` — installed *before* window open so AppKit picks accelerators immediately.
11. `cx.observe_global::<SettingsStore, _>(|cx| theme::reload_from_settings(cx))`.
12. Open root window with `WindowOptions { titlebar: Some(TitlebarOptions { title: "Tolaria" }), ..default() }`; root view `TolariaWorkspace::empty(window, cx)`.
13. `cx.activate(true)`.

### Reference files (read order)

- `/Users/konstantin/tolaria/crates/embed_poc/src/main.rs` — proven registration shape.
- `/Users/konstantin/tolaria/crates/embed_poc/src/menus.rs` — `actions!()` + `OsAction` Edit menu skeleton; reuse verbatim.
- `/Users/konstantin/tolaria/crates/embed_poc/src/layout.rs` — `gpui_component::init` ordering, focus + observer patterns.
- `/Users/konstantin/zed/crates/zed/src/main.rs:481–860` — production-grade ordering, `set_global` + `observe_global`.
- `/Users/konstantin/zed/crates/zed/src/zed/app_menus.rs` — full File/Edit/View/Go/Window/Help menu model.
- `/Users/konstantin/zed/crates/workspace/src/workspace.rs:1130` (`WorkspaceStore`) and `:1348` (`Workspace`) — informs Phase 2 expansion.

### Test strategy (all `#[gpui::test]`, no real window)

Mirror Phase 0's 26-test approach.

- `settings_store`: JSON round-trip; default fallback on missing file; observer fires on mutation.
- `actions`: every action dispatchable; keymap parses; conflicting bindings rejected; user override beats default.
- `theme`: global installed; setting switch propagates.
- `tolaria` (binary integration): `TestAppContext::add_empty_window` + Cmd+Q dispatches `Quit`; reload-keymap fires once.
- `workspace`: empty `TolariaWorkspace::new_for_tests(cx)` renders without panic; `ModalLayer` accepts and removes a dummy modal.

### Exit criteria

1. `cargo build -p tolaria` cold-clean succeeds on macOS.
2. `cargo run -p tolaria` opens a single window, title bar visible, menu installed, Cmd+Q quits, Cmd+, opens a placeholder toast "settings UI in Phase 2".
3. `cargo test --workspace` green; no test relies on a real `NSWindow`.
4. `~/Library/Application Support/Tolaria/settings.json` is created/read/observed.
5. `src-tauri/` untouched; `pnpm tauri dev` still works.
6. `cargo fmt --all` clean; branch pushed to `main`.

---

## Phase 2 — Chrome against mock fixtures (deep plan)

### New chrome crates (`crates/`)

| Crate | Owns these `src/components/` surfaces |
|---|---|
| `mock_fixtures` (dev-dep) | Hardcoded vault, notes, git status, search results, AI threads, settings, types/views/folders. Single source of truth. |
| `sidebar_panel` | `Sidebar.tsx`, `sidebar/*` (6), `FolderTree.tsx`, `WorkspaceSelector.tsx`, `TypeSelector.tsx` |
| `note_list_pane` | `NoteList.tsx`, `note-list/*` (11), `NoteItem.tsx`, `note-item/*`, `FilterBuilder.tsx`, `FilePreview.tsx` |
| `inspector_panel` | `Inspector.tsx`, `inspector/*` (8 — Backlinks, GitHistory, InspectorChrome, Instances, NoteInfo, ReferencedBy, Relationships), `TableOfContentsPanel.tsx` |
| `ai_panel` | `AiPanel.tsx`, `AiPanelChrome.tsx`, `AiMessage.tsx`, `AiActionCard.tsx`, onboarding prompts |
| `status_bar` | `StatusBar.tsx`, `status-bar/*` (2) |
| `breadcrumb_bar` | `BreadcrumbBar.tsx` |
| `command_palette` | `CommandPalette.tsx`, `CommandPaletteAiMode.tsx` |
| `quick_open` | `QuickOpenPalette.tsx` |
| `search_panel` | `SearchPanel.tsx` |
| `diff_view` | `DiffView.tsx` |
| `dialogs` | All 11 `*Dialog.tsx` / `*Modal.tsx` (Commit, ConfirmDelete, CreateNote, CreateType, CreateView, Feedback, McpSetup, TelemetryConsent, GitRequiredModal, ConflictResolverModal, AddRemoteModal, CloneVaultModal, NoteRetargetingDialogs, RetargetNoteDialog, OnboardingShell) |
| `banners` | 6 banner surfaces (Archived, Conflict, RenameDetected, Update, TrashWarning, DeleteProgressNotice) |
| `toasts` | `Toast.tsx` wrapping `gpui-component` `Notification` mounted into `ToastLayer` |
| `settings_panel` | `SettingsPanel.tsx` + 6 section files |
| `startup` | `WelcomeScreen.tsx`, `StartupScreen.tsx` |
| `wikilink_inputs` | `Wikilink{Chat,Suggestion,Inline}` |
| `image_lightbox` | `ImageLightbox.tsx` |
| `emoji_picker` | `EmojiPicker.tsx`, `TagsDropdown.tsx` |

### `workspace` expansion (Phase 1 crate, now grown)

- `Dock` (3 instances: Left/Right/Bottom) — model on `/Users/konstantin/zed/crates/workspace/src/dock.rs:269`.
- `Pane` + `PaneGroup` — model on `pane.rs:397` and `pane_group.rs:30`.
- `Panel` trait implemented by every panel crate (`dock.rs:36–94`: `persistent_name`/`panel_key`/`position`/`set_position`/`default_size`/`icon`/`toggle_action`/`starts_open`/`activation_priority`).
- `Item` trait skeleton (`item.rs:167–350`); ship one stub `MockNoteItem` rendering breadcrumb + banners + "Editor body in Phase 4" placeholder.
- `ModalLayer` per `/Users/konstantin/zed/crates/workspace/src/modal_layer.rs:10–90`.

### Mock-fixture strategy

`crates/mock_fixtures` exposes `MockVault` (30-note vault mirroring `demo-vault-v2/`), `MockGit`, `MockSearch`, `MockAi`, `MockSettings`. Every chrome crate puts `mock_fixtures` under `[dev-dependencies]`. The `tolaria` binary exposes `TOLARIA_MOCK=1` that registers mock-fixture-backed Entities in place of the (not-yet-existent) service Globals. **Every mock method returns `Task<T>` even if instantly resolved** — keeps the API shape forward-compatible with Phase 3 real services.

### `gpui-component` primitive coverage (ADR §7 says 16/26 direct)

| Surface | Primitive | Coverage |
|---|---|---|
| Sidebar Dock | `Sidebar`, `Tree`, `Tag`, `Avatar` | direct |
| Note list | `List`, `ScrollArea`, `Skeleton` | direct |
| Inspector | `Accordion`, `Tab`, `Badge`, `Breadcrumb` | direct |
| AI panel | `ScrollArea`, `Avatar`, `Input`, `Button`, `Spinner` | direct |
| Status bar | `Button`, `Tooltip`, `Progress` | direct |
| Resizable Docks | `Resizable` (proven by `embed_poc/src/layout.rs:30–144`) | direct |
| Settings panel | `Switch`, `Select`, `Slider`, `Input`, `ColorPicker`, `DatePicker`, `Calendar`, `Tab` | direct |
| Dialogs | `Dialog`, `Sheet`, `Button`, `Input` | direct |
| Banners | `Alert` | direct |
| Toasts | `Notification` | direct |
| Command palette / Quick open / Wikilink combobox | `Picker<Delegate>` — **gap, see Risk #2** | port from Zed |
| Diff view | `ScrollArea` + custom row renderer | partial |
| Emoji picker | `Popover` + custom grid | partial |
| Image lightbox | `Dialog` + custom overlay | partial |
| Filter builder | none — build in `ui` | gap |
| Folder tree drag-drop | `Tree` lacks DnD per ADR §7 — build in `ui` | gap |
| IconPicker over Phosphor | none — `ui` | gap |
| RichTooltip-with-shortcut | `Tooltip` lacks rich content — `ui` | gap |

### Migration discipline

- Every `.tsx` from the chrome inventory has a Rust counterpart in exactly one crate above.
- Modal/popover surfaces are `impl ModalView` mounted via `workspace::ModalLayer::toggle_modal` (`modal_layer.rs:30–60`).
- Pickers vendor a minimal port of Zed's `Picker<Delegate>` (`/Users/konstantin/zed/crates/picker/src/picker.rs`, ~400 LOC) into `crates/ui`.
- One inspector sub-panel = one `Render` view; the 7 compose via `Accordion`/`Tab` in `inspector_panel::Inspector`.

### Test strategy

Per chrome crate: render-snapshot + interaction + observer-fanout tests with `TestAppContext::add_window` / `simulate_keystrokes` / `simulate_mouse_down` (patterns from `embed_poc/src/layout.rs:251`, `embed_poc/src/menus.rs:127`). Plus:

- An integration test in `tolaria` that enumerates `actions!()` and dispatches each — every action must be wired.
- A keymap-conflict test (no two surfaces claim the same chord in the default keymap).
- A `Task::ready(...)` observer test — confirms a mock method returning a resolved `Task` re-renders dependent views, proving Phase 3 swap is shape-compatible.

Target ~150–200 tests across Phase 2 crates.

### Exit criteria

1. `TOLARIA_MOCK=1 cargo run -p tolaria` shows full chrome: 3 Docks, status bar, breadcrumb, note list, mock note item with placeholder editor body, all banners attachable, inspector 7 panels.
2. Every action reachable via menu, keymap, command palette — enforced by integration test.
3. All 11 dialogs and 6 banners openable.
4. `cargo test --workspace` green, ~150+ tests.
5. Tauri stack still launches in parallel; no build-artifact overlap.
6. `cargo fmt --all` clean.

---

## Risks (Phases 1–2)

| # | Risk | L | I | Mitigation |
|---|------|---|---|------------|
| 1 | `gpui-component` primitive gaps wider than 16/26 estimate | M | M | Scope `crates/ui` work explicitly at Phase 2 kickoff; budget 1.5× the obvious-gap list. Pin already at `a5268cd` in `Cargo.toml:47`. |
| 2 | **`Picker<Delegate>` not exported** by any pinned dep. Three modal surfaces depend on it. | H | H | Decide vendor-vs-port at Phase 2 kickoff. Port minimal `Picker` into `crates/ui` from `/Users/konstantin/zed/crates/picker/src/picker.rs`; document divergence. |
| 3 | Async runtime story not decided (Phase 3 hazard). Mock APIs may need to swap to `Task<Result<T>>`. | M | M | Phase 2 mock methods return `Task<T>` even if instant; add test that observes a `Task::ready` resolve. |
| 4 | Native menu vs layered modal focus interaction untested. Phase 0 only proved menu wins over WKWebView. | M | M | Phase 2 test: open Picker → trigger Cmd+S → assert action fires, modal stays. |
| 5 | Multi-tab `Pane` UX (ADR §3 revisits ADR-0003) — close hotkey, drag-reorder, persistence unanswered. | M | M | Phase 2 ships single-tab `Pane` with stub for multi-tab; spec multi-tab UX in follow-up ADR before Phase 4. |
| 6 | CodeScene ratchet on new crates (ADR-0064) triggers gate failures during build-up. | L | L | Defer `.codescene-thresholds` reset until Phase 6 cut-over. Gate keeps evaluating stable `src-tauri/` + `src/`. |

---

## Verification discipline

After **every iteration** of Rust source changes (per crate or per logical sub-task within a phase), in this order:

1. `cargo fmt -p <crate>` (or `--all` if multi-crate).
2. `cargo test -p <crate>` — confirm green.
3. **Spawn `oh-my-claudecode:code-reviewer` with the `idiomatic-rust-review` skill** against the changed files. **Auto-apply every MUST and SHOULD finding** without prompting the user (MAY findings get surfaced separately for the user to decide).
4. Re-run `cargo fmt` + `cargo test` after applying review findings.
5. Commit.

At every **phase boundary** the full sweep:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p tolaria --release
```

Phase 1 specific:

```sh
cargo run -p tolaria                                  # window opens, Cmd+Q quits
ls ~/Library/Application\ Support/Tolaria/settings.json
```

Phase 2 specific:

```sh
TOLARIA_MOCK=1 cargo run -p tolaria                   # full mock chrome
cargo test -p command_palette -- --nocapture
cargo test -p dialogs --test all_dialogs_open
```

Parallel sanity (Tauri stays alive through Phase 5):

```sh
pnpm tauri dev
pnpm test                                             # editor-body Playwright suite
```

---

## Critical files

**Phase 0 reference (read first):**
- `/Users/konstantin/tolaria/crates/embed_poc/src/main.rs`
- `/Users/konstantin/tolaria/crates/embed_poc/src/menus.rs`
- `/Users/konstantin/tolaria/crates/embed_poc/src/layout.rs`
- `/Users/konstantin/tolaria/crates/embed_poc/src/webview.rs`

**Zed reference (template):**
- `/Users/konstantin/zed/crates/zed/src/main.rs` (registration sequence)
- `/Users/konstantin/zed/crates/zed/src/zed/app_menus.rs` (full menu)
- `/Users/konstantin/zed/crates/workspace/src/{workspace.rs,dock.rs,pane.rs,pane_group.rs,item.rs,modal_layer.rs}` (topology)
- `/Users/konstantin/zed/crates/picker/src/picker.rs` (vendor source for `Picker`)
- `/Users/konstantin/zed/crates/settings/src/settings_store.rs` (Global + observer pattern)

**To create in Phase 1:**
- `crates/tolaria/{Cargo.toml,src/main.rs,src/menus.rs}`
- `crates/paths/{Cargo.toml,src/lib.rs}`
- `crates/theme/{Cargo.toml,src/lib.rs}`
- `crates/settings_store/{Cargo.toml,src/lib.rs}`
- `crates/actions/{Cargo.toml,src/lib.rs,assets/default.json}`
- `crates/ui/{Cargo.toml,src/lib.rs}`
- `crates/workspace/{Cargo.toml,src/{lib.rs,workspace.rs,modal_layer.rs,toast_layer.rs}}`
- Update root `Cargo.toml` workspace `members` list.

**To create in Phase 2:** every chrome crate listed in the Phase 2 table; expand `crates/workspace/src/` with `dock.rs`, `pane.rs`, `pane_group.rs`, `item.rs`.

**Untouched through Phases 1–5:** `src-tauri/`, `src/`, `editor-host/` (latter doesn't exist yet — created in Phase 4).
