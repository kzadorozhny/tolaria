# ADR-0115 migration progress ledger

Branch: `feat/native-gpui-chrome`.  Push-to-`main` workflow per ADR-0021;
intermediates are dogfood-only.  Tauri stack under `src-tauri/` stays
untouched until the cut-over phase.

**Roadmap is MVP-first** as of 2026-05-18 — see [`roadmap.md`](roadmap.md)
for the live phase order and [`mvp-scope.md`](mvp-scope.md) for the
MVP cut definition.  Original full roadmap preserved in §A of
[`00-overview.md`](00-overview.md) for historical reference.

| Phase | Status | Commit | Tests | Crates added |
|-------|--------|--------|-------|--------------|
| 0 — `embed_poc` spike | ✅ done | `9f26531e` | 26 | `embed_poc` |
| 1 — Foundation crates | ✅ done | `3a8d54d5` | +19 (45) | `paths`, `theme`, `actions`, `ui`, `settings_store`, `workspace`, `tolaria` (bin) |
| 2a — Workspace topology + mocks + Picker | ✅ done | `956f8c58` | +51 (96) | `mock_fixtures` ; expanded `workspace` (Dock/Pane/PaneGroup/Panel/Item/MockNoteItem) ; vendored Picker into `ui` |
| 2b — First chrome surfaces | ✅ done | `e31bc7fc` | +19 (115) | `status_bar`, `breadcrumb_bar`, `toasts`, `banners` |
| 2c — Chrome wiring + TOLARIA_MOCK | ✅ done | `3131ccc7` | +3 (118) | — (integration wave; touched 5 existing crates) |
| 2d — Big panels | ✅ done | `6d96cca8` | +31 (149) | `sidebar_panel`, `note_list_pane`, `inspector_panel`, `ai_panel`, `search_panel`, `settings_panel`, `diff_view` |
| **3-MVP — Vault service (minimal)** | ✅ done | `ad1581cb` | +9 (158) | `vault` (open dir, list, read, save, rescan; sync IO; markdown-only; shape-compatible with `mock_fixtures::MockVault`) |
| **4-MVP — Editor host integration** | ✅ done | `8c31dd32` / `a6d221ec` / `bc69b714` | +29 (187) | `editor_bridge`, `note_item`; `editor-host/` Vite project |
| **5-MVP — MVP wiring + launch** | ✅ done | `f3eef114` / `e0a2b6f0` | +4 (191) | `tolaria --vault`; chrome `from_vault`; `open_note` helper; IPC channel routing |
| **— MVP CUT** | | | | App opens local vault, navigates, renders + saves notes.  Tauri stack still parallel. |
| 6 — Remaining chrome surfaces | ⏳ planned | — | — | `command_palette`, `quick_open`, `dialogs`, `wikilink_inputs`, `image_lightbox`, `emoji_picker`, `startup` |
| 7 — `gpui-component` eval | ⏳ scheduled | — | — | Decision matrix per [`eval-gpui-component-removal.md`](eval-gpui-component-removal.md) |
| 8 — Service expansion | ⏳ planned | — | — | `git_provider`, full `vault_search`, `vault_watcher` (advanced), `cli_agents`, `mcp_bridge`, `telemetry`, `app_updater`, `localization`; wire AI/search/settings_panel chrome to real services |
| 9 — Parity hardening | ⏳ planned | — | — | Multi-tab Pane UX; autogit + conflict resolver; onboarding; measurement gate |
| 10 — Cut-over | ⏳ planned | — | — | `src-tauri/` deleted; `src/` pruned to carry-overs; superseded ADRs flipped; signing rewired |
| 11 — Post-cutover | ⏳ planned | — | — | Windows / Linux feature flags; iPad strategy; native-editor R&D |

---

## Phase-by-phase detail

### Phase 0 — embed_poc spike

Validation crate proving the four ADR-0115 §6 re-evaluation triggers
on macOS: WKWebView focus handoff, IME mid-composition, frame-sync
during sidebar drag, Cmd+S delivery via native menu.  26 in-process
GPUI tests (Test*Context, simulate_keystrokes, simulate_window_resize)
cover Scenarios 1/3/4; IME stays a manual pass.

### Phase 1 — Foundation

Seven crates, empty native shell:

- `paths` — app data/config dir resolver; panics on `dirs::data_dir()` miss
- `theme` — wraps `gpui_component::Theme` as idempotent Global
- `actions` — `actions!()` registry + default+user keymap merge (infallible)
- `ui` — Phase 2 compounds placeholder
- `settings_store` — `Global`; atomic JSON persist via `.tmp`+rename; observer fan-out
- `workspace` — `TolariaWorkspace` skeleton; `ModalLayer` + `ToastLayer`; public methods (`empty`, `push_toast`, `toggle_modal`, `dismiss_modal`, `has_active_modal`, `toast_count`)
- `tolaria` — binary; native menu + Cmd+Q; opens root window

API decisions during per-crate idiomatic-rust-review pass:

- `actions::init` dropped `Result` (always `Ok`)
- `SettingsStore.settings` → `pub(crate)`; callers use `::get(cx)`
- `TolariaWorkspace` overlay fields private + delegate methods

### Phase 2a — Workspace topology + mocks + Picker

Three foundation deliverables that unblock the chrome panel waves:

**`workspace` expansion** — Dock + DockState enum (`Empty/Closed/Open`) + Pane + PaneGroup + Panel trait + Item trait + ItemHandle object-safe wrapper + Activation enum + MockNoteItem stub.  `TolariaWorkspace::empty` mounts 3 docks (Left/Right/Bottom) + center PaneGroup via `h_resizable`.

**`mock_fixtures` crate** — MockVault (30 seeded notes), MockGit (3 modified + 1 untracked + 5-commit history), MockSearch (keyword table, `f32::total_cmp` sort), MockAi (1 four-turn thread with tool-use round-trip), MockSettings.  Every public method returns `Task<T>` (via `Task::ready` for instant) so Phase 3 swap is shape-compatible.

**Picker port from Zed** — `crates/ui/src/picker.rs` (~495 LOC).  PickerDelegate trait (8 methods, RPITIT default for placeholder_text).  Enter / Cmd+Enter consumed via `on_action(InputEnter)`; Esc → `DismissEvent`.  Module header lists every dropped upstream feature with `TODO(Phase 2)` tags.  Upstream sha: `f2df3f9e`.

### Phase 2b — First chrome surfaces

Four small, isolated chrome crates against mock_fixtures (each self-contained, wiring deferred):

- `status_bar` — StatusItem enum (VaultName/GitBranch/DirtyCount/Mode); EditorMode (Normal/Search); `from_mock(cx)` pulls from MockVault/MockGit
- `breadcrumb_bar` — stateless view; BreadcrumbSegment {label, icon}; namespaced ElementIds
- `toasts` — typed Toast variants (Info/Success/Warning/Error); opaque ToastId via `AtomicU64`; `#[non_exhaustive]`; div-based renderer
- `banners` — 6 plan-locked variants (ArchivedNote/ConflictNote/RenameDetected/Update/TrashWarning/DeleteProgressNotice); BannerSeverity; `gpui_component::alert::Alert` renderer

Review pass: 1 MUST + 13 SHOULDs applied (breadcrumb_bar is_last fix; toasts public-field tightening; Default impl on BreadcrumbBar; namespaced ElementIds; `# Panics` docs; etc.).

### Phase 2c — Chrome wiring + TOLARIA_MOCK

Integration wave:

- `StatusBar::from_or_empty(cx)` helper — returns `from_mock(cx)` if mock globals registered, empty otherwise
- `workspace::ToastLayer` switched from `Vec<SharedString>` to `Vec<toasts::Toast>` + `toasts::render_toast`
- `TolariaWorkspace::push_toast` now takes `Toast` directly; new `status_bar: Entity<StatusBar>` field rendered in the status-bar slot
- `MockNoteItem` composes `Vec<BreadcrumbSegment>` (derived from path) + `Vec<Banner>` stack via `with_banner(...)` builder
- `tolaria` binary reads `TOLARIA_MOCK` env var (truthy: `1`/`true`/`yes`/`on`, case-insensitive); installs MockVault/MockGit/MockAi/MockSearch as Globals before `observe_global` registrations

Manual verify: `TOLARIA_MOCK=1 cargo run -p tolaria` launches cleanly; log shows `installed mock_fixtures globals`.

Review pass: 2 MUST + 3 SHOULD applied (status_bar doc concatenation; mock-install ordering; `bar: BreadcrumbBar` → direct `Vec<BreadcrumbSegment>`; tightened TOLARIA_MOCK truthy match; inlined awkward two-step construction).

### Phase 3-MVP — Vault service (minimal)

First service crate.  Public API mirrors `mock_fixtures::MockVault` so chrome panels can swap implementations in Phase 5-MVP with minimal call-site churn.

- `Vault: Global` rooted at a canonicalised path; opens via `Vault::open_at(root)`
- `Note { id: NoteId, title: SharedString, path: PathBuf, kind: NoteKind, modified: DateTime<Utc>, byte_size: u64 }`
- `NoteId(u64)` newtype: monotonically increasing within a single `Vault` instance, never reused after delete+rescan, not persisted (restart at 0 on reopen)
- `VaultError::{NotFound(NoteId), Io { path, source }}` via `thiserror`
- Methods: `notes() -> Task<Vec<NoteId>>`, `note(id) -> Task<Option<Note>>`, `note_content(id) -> Task<Result<String, VaultError>>`, `save(id, &str) -> Task<Result<(), VaultError>>`, `search_titles(query) -> Task<Vec<NoteId>>`, `rescan() -> Result<()>`
- Recursive markdown walker, depth cap 32, skips hidden directories (`.git/`, `.obsidian/`), markdown-only (assets + folders deferred to Phase 8)
- Synchronous IO inside `Task::ready(...)` for MVP; Phase 8 moves long ops to `cx.background_executor().spawn(...)` + adds the FS watcher
- 9 tests: opens_empty_vault, indexes_markdown_files, skips_hidden_directories, skips_non_markdown_files, save_writes_to_disk_and_updates_byte_size, save_unknown_id_returns_not_found, rescan_preserves_ids_for_unchanged_paths, rescan_drops_vanished_notes, open_nonexistent_dir_errors

Review pass: 1 MUST + 4 SHOULD applied (metadata-refresh failure now `log::warn!` instead of silent swallow; `NoteId` docstring spells out monotonic-never-reused-not-persisted contract; `save_sync` test backdoor so tests assert on `Result` directly; `save` signature takes `&str` instead of `String`; `note_ids_vec()` dedups between `notes()` and the test accessor).

### Phase 4-MVP — Editor host integration

Three deliverables wire the embedded editor into the native shell.  Phase 5-MVP glues the IPC routing back into GPUI entities.

**`editor_bridge` crate (4a, `8c31dd32`)** — typed JSON envelope.  `ToHost` (native → editor): NoteOpen, FocusEditor, SaveRequest, ThemeSet.  `FromHost` (editor → native): Ready, Dirty, Save, Saved, LinkClick, Keydown.  `{ "k": "<kind>", "v": <payload> }` shape via `#[serde(tag, content, rename_all = "snake_case")]`.  Typed `Mods { alt, ctrl, meta, shift }` with `skip_serializing_if` so `Cmd+S` sends just `{"meta":true}`.  `vault::NoteId` gains `#[derive(Serialize, Deserialize)] + #[serde(transparent)]` so IDs travel as bare integers and are typed across the boundary.  `BridgeError::{Encode,Decode}` carries the `serde_json::Error` source chain.  17 in-process tests including snake_case lock-in for every variant.

Review pass: 2 MUST + 5 SHOULD applied — `thiserror.workspace = true`; reuse `vault::NoteId` instead of bare `u64`; `#[source]` on `BridgeError`; typed `Mods` struct (was stringly-typed); symmetric encode/decode helpers; `WKWebView` rustdoc backticks; `#![warn(missing_docs)]` + struct-level docs.

**`editor-host/` Vite project (4b)** — minimal markdown editor inside the WKWebView.  Single-file output via `vite-plugin-singlefile` so `dist/index.html` is fully self-contained (~3.95 kB) and `crates/note_item` embeds it via `include_str!()`.  `src/bridge.ts` mirrors the Rust enums as discriminated unions (TS literal-tag dispatch); `src/editor.ts` is a `<textarea>` MVP that emits Dirty/Save/Keydown and accepts NoteOpen/FocusEditor/SaveRequest/ThemeSet.  BlockNote+CodeMirror carry-over from `src/` deferred to post-MVP.

**`note_item` crate (4c)** — `workspace::Item` implementation owning a per-note WKWebView.  Pure-logic `apply_from_host(&mut self, FromHost) -> Outcome` dispatches Dirty/Save/Saved/LinkClick/Keydown; `Outcome::{None, PersistSave{body}, NavigateLink(LinkTarget)}` describes follow-up effects.  `LinkTarget::classify` discriminates wikilinks from URLs (`http(s)://`, `mailto:`).  macOS `new_with_webview` returns `Result<Self>` (no panics on user-triggered paths).  `InstrumentedWebView` mirrors `embed_poc`'s 0.5px epsilon-guard pattern with `set_bounds` failures now logged at `warn!`.  All macOS-specific code lives in `mod macos { … }` to keep cfg-gates from scattering.  12 tests cover dispatch + classification + HTML embedding.

Review pass: 2 MUST + 5 SHOULD applied — `spawn_webview` / `new_with_webview` return `Result` (was panicking via `.expect`); macOS code consolidated into `mod macos` (was scattered cfg blocks); `path()` returns `&Path` (was `&PathBuf`); `Outcome::PersistSave` drops redundant `id`; `LinkTarget` enum (was stringly-typed `target`); `set_bounds` failure logs `warn!`; `apply_from_host` arm duplication factored into `check_own` helper.  `vault::NoteId::from_raw` added as a `#[doc(hidden)]` test constructor so downstream crates don't depend on the serde round-trip.

### Phase 5-MVP — MVP wiring + launch

End-to-end integration of the foundation crates.  Shipped in two commit waves: 5a/b/c (vault wiring) and 5d/e (open-note + IPC channel).

**5a — Type unification.**  `vault::NoteId` is canonical; `mock_fixtures` re-exports it; `NoteId::from_raw` promoted to public since `MockVault` legitimately uses it at runtime.  All `NoteId(N)` construction sites and `.0` field access swept across mock_fixtures, inspector_panel, note_list_pane, search_panel, sidebar_panel.

**5b — `tolaria --vault <path>` CLI flag.**  `parse_args()` walks argv; `Vault::open_at(path)` installs the real vault as a `Global` before observers register.  TOLARIA_MOCK=1 path still works.

**5c — `SidebarPanel::from_vault` / `NoteListPane::from_vault`.**  Mirror existing `from_mock` constructors against real vault.  `from_or_empty` precedence: `vault::Vault` > `MockVault` > empty.  `from_or_empty_prefers_real_vault` test locks the contract.

**5d — Open-note flow.**  `note_list_pane::OpenNoteEvent` + `EventEmitter<OpenNoteEvent>`; row click emits via `cx.emit`.  `workspace::TolariaWorkspace::add_item_to_active_pane` adds an `ItemHandle` to the center `PaneGroup`'s active `Pane` (creating one if empty).  `tolaria::open_note::open_note(workspace, id, window, cx)` helper reads metadata + body from `vault::Vault`, builds `NoteItem::new_with_webview`, routes through `add_item_to_active_pane`.  Lives in the binary crate because the dep graph forbids `workspace → note_item`.

**5e — IPC channel routing + save persistence.**  `note_item::spawn_webview` takes a `flume::Sender<FromHost>`; the wry IPC handler decodes each message and pushes it down the channel.  `NoteItem::install_dispatch_task(entity, rx, cx)` spawns a detached foreground task that drains the receiver, runs `apply_from_host`, and on `Outcome::PersistSave` calls `vault::Vault::save(id, &body).detach()`.  Task exits cleanly when the WebView entity drops (sender drops → channel closes → loop ends).  `NoteItem::new_with_webview` now returns `Entity<Self>` and installs the dispatch task internally.

End-to-end test `dispatch_task_persists_save_to_vault`: tempdir vault + simulated `FromHost::Save` on the channel + `run_until_parked` → assert disk content equals the new body.  Proves the MVP save persistence works without a real WKWebView.

**Remaining UI mounting (post-MVP CUT).**  `NoteListPane` is not yet mounted in `TolariaWorkspace::empty`, and the `OpenNoteEvent → open_note` subscription is not wired.  The data path is complete and the open_note flow is fully testable; the chrome layout integration is a follow-on layout task.

---

## Durable feedback memories applied throughout

- **cargo fmt after every Rust edit** — `~/.claude/projects/-Users-konstantin-tolaria/memory/feedback_rust_cargo_fmt.md`
- **idiomatic-rust-review subagent before commit** — `feedback_rust_reviewer.md` (auto-apply every MUST and SHOULD)
- **No prompt for grep bash commands** — `feedback_grep_no_prompt.md`
