# ADR-0115 migration progress ledger

Branch: `feat/native-gpui-chrome`.  Push-to-`main` workflow per ADR-0021;
intermediates are dogfood-only.  Tauri stack under `src-tauri/` stays
untouched until the cut-over phase.

| Phase | Status | Commit | Tests | Crates added |
|-------|--------|--------|-------|--------------|
| 0 — `embed_poc` spike | ✅ done | `9f26531e` | 26 | `embed_poc` |
| 1 — Foundation crates | ✅ done | `3a8d54d5` | +19 (45) | `paths`, `theme`, `actions`, `ui`, `settings_store`, `workspace`, `tolaria` (bin) |
| 2a — Workspace topology + mocks + Picker | ✅ done | `956f8c58` | +51 (96) | `mock_fixtures` ; expanded `workspace` (Dock/Pane/PaneGroup/Panel/Item/MockNoteItem) ; vendored Picker into `ui` |
| 2b — First chrome surfaces | ✅ done | `e31bc7fc` | +19 (115) | `status_bar`, `breadcrumb_bar`, `toasts`, `banners` |
| 2c — Chrome wiring + TOLARIA_MOCK | ✅ done | `3131ccc7` | +3 (118) | — (integration wave; touched 5 existing crates) |
| 2d — Big panels | ⏳ planned | — | — | `sidebar_panel`, `note_list_pane`, `inspector_panel`, `ai_panel`, `search_panel`, `settings_panel`, `diff_view` |
| 2e — Remaining surfaces | ⏳ planned | — | — | `command_palette`, `quick_open`, `dialogs`, `wikilink_inputs`, `image_lightbox`, `emoji_picker`, `startup` |
| 2f — `gpui-component` eval | ⏳ scheduled | — | — | Timeboxed evaluation pass per [`eval-gpui-component-removal.md`](eval-gpui-component-removal.md); produces a keep / pin / vendor / replace recommendation + any follow-on work.  Runs **before** Phase 3 so the chrome's primitive contract is locked before services plumbing lands. |
| 3 — Services migration | ⏳ planned | — | — | `vault`, `git_provider`, `vault_search`, `vault_watcher`, `cli_agents`, `mcp_bridge`, `telemetry`, `app_updater`, `localization` |
| 4 — Editor host | ⏳ planned | — | — | `editor_bridge`, `note_item` ; new `editor-host/` Vite project |
| 5 — Parity hardening | ⏳ planned | — | — | — |
| 6 — Cut-over | ⏳ planned | — | — | `src-tauri/` deleted ; `src/` pruned to carry-overs |
| 7 — Post-cutover | ⏳ planned | — | — | Windows / Linux feature flags ; iPad strategy ; native-editor R&D |

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

---

## Durable feedback memories applied throughout

- **cargo fmt after every Rust edit** — `~/.claude/projects/-Users-konstantin-tolaria/memory/feedback_rust_cargo_fmt.md`
- **idiomatic-rust-review subagent before commit** — `feedback_rust_reviewer.md` (auto-apply every MUST and SHOULD)
- **No prompt for grep bash commands** — `feedback_grep_no_prompt.md`
