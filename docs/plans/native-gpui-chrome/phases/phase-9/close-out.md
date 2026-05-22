# Phase 9 — Close-out (2026-05-22)

**Resolution scoreboard.**

| Bucket | Count | Status |
|--------|------:|--------|
| 1. Blockers | — | n/a (none filed) |
| 2. High Priority | 18 / 19 | ✅ (1 ➡️ deferred — `9.2.5` AI button) |
| 3. Low Priority | 8 / 8 | ✅ |
| Total in-scope rows | **26 / 27** | ✅ (`9.2.5` ➡️ Phase 10) |

**Branch tip:** `feat/native-gpui-chrome` at `fcc0677b`, ready for merge into `main` subject to the user-driven manual eyeball pass (durable memory `feedback_manual_validation.md`).  Phase 9 carried the seven Phase-8-deferred toolbar slots all the way to real behaviour (star / organised / neighbourhood / raw / toc / more — plus the inspector-panel content + chrome polish that emerged mid-phase) and added eight low-priority polish items on top.

**Per-row count.**  Phase 9 grew from the originally-scoped 14 rows (9.2.1–9.2.9 + 9.3.1–9.3.6) to 27 once mid-phase regressions and follow-up polish landed: 19 high-priority rows (`9.2.1`–`9.2.19`) + 8 low-priority rows (`9.3.1`–`9.3.8`).  Every closed row carries a `Closure (commit <sha>)` paragraph in [`worklist.md`](worklist.md); rows that survived multiple bug reports also carry `Reopened` / `Re-closure-N` annotations recording each round.

## Architectural deltas

1. **Per-note frontmatter bool writes** (`9.2.1` + `9.2.2`).  `vault::Vault::set_frontmatter_bool(id, key, value) -> Task<Result<…>>` shipped a byte-identical YAML rewriter (`vault::frontmatter::set_bool_in_raw`) that flips a single key without touching the rest of the document.  `Note::is_favorite()` / `is_organized()` accessors layer over the bool; the star + organised toolbar cells write through this seam.

2. **Backlinks + outbound-links indexes** (`9.2.3` + `9.2.8`).  `vault::Vault::backlinks(id) -> Vec<NoteId>` + `outbound_links(id) -> Vec<NoteId>` lifted the wikilink scan out of one-off note-list code into vault-global resolved indexes.  Neighbourhood mode (`9.2.3`) consumes both; the inspector's Backlinks / References / Has sections (`9.2.8`) consume the same indexes.  Shared infrastructure — one query lands the read path, every downstream feature gets it for free.

3. **Chrome-owned editor toggles** (`9.2.4` raw-mode + `9.2.17` wide-mode).  Two new `ToHost::*` bridge variants (`SetRawMode { enabled }`, `SetWideMode { wide }`) plus matching `NoteItem` fields, getters, and `toggle_*` methods.  The chrome owns the toggle state per `NoteItem`; the editor-host reacts to the bridge envelope by switching its surface (raw CodeMirror vs BlockNote, max-width vs unconstrained).  Mirrors the React-app component-local state model.

4. **Right-dock panel framework** (`9.2.6` ToC + `9.2.8` Inspector + `9.2.13` chrome).  New `toc_panel` crate, full `inspector_panel` content (7 sections: Properties, Outline, Backlinks, Instances, ReferencedBy, Relationships, Info, GitHistory stub).  Shared `toggle_or_swap_right_dock_panel` helper in `tolaria::macos` encodes the three right-dock states (already-mounted-toggle, sibling-swap, fresh-attach) so the ToC and Inspector handlers stay symmetric.  Live `Dock::panel_key` accessor makes "which panel is mounted right now" addressable from outside the workspace crate.

5. **Display-title resolution** (`9.2.14` neighbourhood title + `9.3.8`).  `note_list_pane::extract_title(body)` (first `# H1` → frontmatter `title:` → file stem) is now `pub`; the neighbourhood handler in `tolaria::macos::handle_enter_neighborhood` calls it via `vault::Vault::note_content`, so the note-list rows and the neighbourhood-mode header read the same display string.

6. **Editor-host shadcn parity** (`9.3.1` + `9.3.7`).  Custom `tolariaBlockNoteSideMenu.tsx` (600-line port) + `tolariaBlockNoteFormattingToolbar.tsx` (1,191-line port) — Mantine-free rebuilds of the React-app's BlockNote chrome using `Components.Generic.Menu.*` vanilla primitives.  CSS swap from `@blocknote/react/style.css` to `@blocknote/shadcn/style.css` brings 16 `.bn-shadcn`-prefixed rules into the host bundle (side-menu alignment, button sizing, toolbar overflow).

7. **Inspector chrome reshape** (`9.3.2` + `9.3.3` + `9.3.4` + `9.3.5` + `9.3.6`).  Workspace-level title-bar inspector toggle (`title-bar-toggle-inspector` mirrors the sidebar toggle on the opposite side).  In-panel header strip at the note-toolbar's 52pt baseline with a `Properties` title + panel-header close `X`.  Two `WORKSPACE_*_DOCK_INITIAL_WIDTH_PT` constants (200pt left / 200pt right) keep the column rhythm aligned with the sidebar.

8. **Resizable-state pollution workaround** (`9.3.2` Reopened-4).  `gpui-component::ResizableState::sync_panels_count` extends new slots with `PANEL_MIN_SIZE` (100pt), then `adjust_to_container_size` ratio-redistributes — which pinned the initially-invisible right-dock column to ≈ 94pt before the user ever toggled it open.  Fix: `TolariaWorkspace` holds the `ResizableState` externally (`main_resizable_state`), binds the row via `with_state(&entity)`, and observes the right_dock for the close → open transition.  On the first transition, calls `state.resize_panel(3, WORKSPACE_RIGHT_DOCK_INITIAL_WIDTH_PT, window, cx)` exactly once per session (gated by `right_dock_ever_opened: bool`) so the column opens at 200pt while subsequent manual drag-resizes survive.

9. **Re-entrancy-safe action dispatch** (cross-row, fixed across `9.2.3` / `9.2.4` / `9.2.6` / `9.2.13` Reopened-2).  Every toolbar / title-bar click closure dispatches via `window.dispatch_action(Box::new(action), cx)` rather than `cx.dispatch_action(&action)` — the former internally `cx.defer`s for after the click update unwinds, the latter silently fails the `cx.windows[id].take()` re-entrancy guard via `.log_err()`.  Regression test `toolbar_window_dispatch_reaches_app_action_handler_under_nested_update` + companion negative test `app_dispatch_action_from_inside_window_update_silently_drops` pin both branches.

10. **Build-tag + diagnostic chain** (`9.2.13` + `9.3.5`).  `tolaria::macos::run` emits `=== tolaria build=v0.1.0 git:<hash> ===` via `eprintln!` at startup (bypasses any log filter), and the env_logger filter promotes `workspace` to Info so the full title-bar-click → dispatch_to_workspace → toggle_or_swap chain prints under default `cargo run`.  `dispatch_to_workspace`'s three early-exit branches (no active window, non-Root, non-Workspace) promoted from `debug!` to `warn!` so silent failures surface.

## Deferred to Phase 10

| Row | Slot / surface | Reason |
|-----|----------------|--------|
| `9.2.5` | `note-toolbar-ai` | AI assistant entry point — requires `cli_agents` provider plumbing.  Toolbar cell is commented out in `note_item::note_toolbar`; restoring it is a 1-line edit once the provider crate lands. |

## Test gates

- **Rust workspace:** `cargo test --workspace` → 519 passed / 0 failed.
- **Editor-host:** `pnpm test --run` → 385 passed / 0 failed (43 vitest files).
- **Clippy:** `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **Fmt:** `cargo fmt --all --check` clean.
- **Periscope sweep:** still `⏳ pending` per the Phase 8 close-out note — runs on host with the user driving the binary.  The end-to-end test `toggle_inspector_dispatch_chain_attaches_panel_end_to_end` plus per-hop counters in `tolaria/src/main.rs` pin the dispatch chain so a future regression localises automatically.

## This-phase sweep commits (newest first, truncated to 30)

```
fcc0677b docs(phase-9): resolve 9.3.2 Re-closure-4 sha to 144a8884
144a8884 fix(workspace): force right-dock to initial width on first open via external ResizableState (worklist 9.3.2 Reopened-4)
057cb642 bottoms tweaks
7e8ffc89 docs(phase-9): resolve 9.3.2 Re-closure-3 + 9.2.19 sha to 0e4f62d1
0e4f62d1 feat(workspace,note_item): bump right-dock default to 360pt + restore note-toolbar inspector cell (worklist 9.3.2 Reopened-3 + 9.2.19)
6fa40771 docs(phase-9): resolve 9.2.18 closure sha to 54e7df0e
54e7df0e feat(note_list_pane): truncate over-long header titles with ellipsis (worklist 9.2.18)
eea56bd5 docs(phase-9): resolve 9.3.2 + 9.3.8 Re-closure sha to 7ced27dd
7ced27dd fix(workspace,note_list_pane,tolaria): right-dock width + neighbourhood display title (worklist 9.3.2 + 9.3.8 Reopened)
46ab2113 docs(phase-9): resolve 9.2.17 + 9.3.8 closure sha to 55561ed7
55561ed7 feat(actions,editor_bridge,note_item,tolaria,editor-host): note width toggle + neighbourhood header (worklist 9.2.17 + 9.3.8)
1a4d59ed docs(phase-9): close 9.3.1 + 9.3.2 Re-closure annotations (sha 5e8cc075)
5e8cc075 fix(workspace,inspector_panel,editor-host): inspector width 280pt + shadcn css import (worklist 9.3.1 + 9.3.2 Reopened)
24185878 docs(phase-9): resolve 9.2.13 Re-closure-4 sha to c66b6e1a; phase 9 fully closed
c66b6e1a fix(inspector_panel): add w_full to render's outer div (worklist 9.2.13 Re-closure-4)
4ef245a2 docs(phase-9): resolve 9.3.7 closure sha to 140fb64c
ce26e520 chore(editor-host): rebuild dist/index.html for 9.3.7
d9387f49 test(tolaria): pin full ToggleInspector dispatch chain end-to-end (worklist 9.2.13)
140fb64c feat(editor-host): BlockNote formatting toolbar parity (worklist 9.3.7)
d209bfb0 feat(tolaria): promote dispatch_to_workspace early-exit logs to warn (worklist 9.2.13)
148378eb feat(tolaria): make inspector dispatch trace visible without RUST_LOG (worklist 9.2.13 + 9.3.5 diagnostic-promotion)
b1614df8 feat(workspace,tolaria): instrument inspector dispatch chain + build-tag log (worklist 9.2.13 + 9.3.5 diagnostic)
fa740de6 feat(tolaria): neighbourhood toggle on/off (worklist 9.2.16)
d9766aa5 feat(inspector_panel,workspace,note_item,tolaria): inspector chrome reshape (worklist 9.3.2+9.3.3+9.3.4+9.3.5+9.3.6)
43a9fcab feat(tolaria): View menu — rename to Properties + restore Show Inspector for GPUI overlay (worklist 9.2.15)
7d697f5a feat(tolaria,note_item): neighbourhood active-state + note-list header title (worklist 9.2.14)
8897ab93 feat(inspector_panel,workspace,tolaria): inspector panel content + right-dock mount (worklist 9.2.8 + 9.2.13a)
a71cc191 fix(note_item,tolaria,workspace): re-entrancy-safe action dispatch via Window::dispatch_action (worklist 9.2.3+9.2.4+9.2.6+9.2.13 Reopened-2)
9a3839c9 feat(vault,note_item,sidebar_panel): star + organised toolbar wiring (worklist 9.2.1 + 9.2.2)
1a96c20a docs(phase-8): close-out (Phase 8 boundary — for reference)
```

`git log --oneline 1a96c20a..HEAD | wc -l` reports **304 commits** for the full Phase 9 span (includes the per-row `docs(phase-9): resolve <sha>` follow-ups + the orchestration / process commits that codified the dispatch + worktree learnings).

## Process learnings

- **Worklist new-row hygiene** (`feedback_worklist_no_emoji_on_new_rows.md`): new rows enter with no emoji; ⏳ stamps land when the orchestrator picks the row up.  Codified mid-phase after the first sweep accidentally stamped every new row with ⏳.
- **Window vs App action dispatch** (`feedback_gpui_dispatch_from_click_closure.md`): every click closure dispatches via `window.dispatch_action(Box::new(action), cx)`.  Four near-identical regressions converged on this single fix.
- **Orchestrator post-dispatch verify** (`feedback_orchestrator_post_dispatch_verify.md`): subagents skipped `git commit` ~80% of the time; every dispatch ends with `git log` + `git status` + `git worktree remove -f -f` from the orchestrator.
- **No status emojis in commit messages** (`feedback_no_status_emoji_in_commits.md`): `⏳ ✅ ➡️ ❌` live on worklist row lines only; never in commit subjects or bodies.
- **Periscope follow-up tests** (`feedback_periscope_followup_test.md`): every periscope-driven UI finding gets a `#[gpui::test]` regression in the same commit.

Codified into [`../../process.md`](../../process.md) (the `Phase 9 retrospective` block) so subsequent phases inherit the rules without re-discovery.
