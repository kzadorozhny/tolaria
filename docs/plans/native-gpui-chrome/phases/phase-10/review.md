# Idiomatic Rust review — last 2 days of commits (2026-05-20 → 2026-05-22)

Scope: 62 Rust-touching commits across the `feat/native-gpui-chrome` branch, reviewed in 6 parallel batches against the idiomatic-rust-review ruleset plus project-specific patterns (GPUI dispatch, WKWebView/objc2 unsafety, foreground-thread blocking I/O, log-level discipline).

Each commit is reviewed inline below with **MUST / SHOULD / MAY** findings tagged by rule ID (`R-1`..`R-12`) or category (`BUG`, `UNSAFE`, `DOC`, `STYLE`). Strengths sections are omitted to keep the document tight.

## Cross-cutting themes worth fixing in a batch

These patterns appear in 3+ commits each and would benefit from a single sweep rather than per-commit fixes.

1. **`cx.dispatch_action(&action)` from `on_click` closures** — silent-fail anti-pattern repeatedly introduced and re-fixed across `45b6622d` (raw-mode toggle), `28296288` (inspector cell), and the surface that `a71cc191` later swept. Confirmed in project memory as the canonical foot-gun for this codebase. Add a clippy lint or a tiny `dispatch_from_click(window, action, cx)` helper that hides the `Box::new(...)` and the window/cx ordering.

2. **Blocking disk I/O on the foreground executor.** `block_on(...)` in `Render`/`on_click` paths appears in `7ced27dd` (note content), `9a3839c9` / `d3f5971e` (frontmatter write), `c1f896b3` (vault rescan after note create), and `13bbc646` (`Vault::backlinks` does *N* `fs::read_to_string` calls synchronously). All of them stutter the chrome on a slow disk and dramatically magnify the cost of a single large-vault test. Push every one onto `cx.background_spawn` and emit an event when the result is ready.

3. **`bool` pairs / triples for domain state** — `MenuState { sidebar_open: bool, inspector_open: bool }` (`6796dc0a`), `(Option, Option, Option)` for organized-icon variants (`0ceec477`), `raw_mode` + `wide_mode` flags on the editor (`55561ed7`), `right_dock_ever_opened: bool` (`144a8884`). Each is one feature away from a flag explosion. R-3 / R-9.

4. **Log-level churn.** `d209bfb0`, `148378eb`, `b1614df8`, `a71cc191`, `40fd9f44`, `d9766aa5` are all "promote info→warn" / "demote info→debug" on the same dispatch lines. Five commits of churn cost. Land at the final level once, with `target = "tolaria::dispatch"` so the gradient can be picked at the env-filter layer instead of recompiled. Also flagged: the `TOLARIA_BUILD_TAG` constant in `b1614df8` concatenates `env!("CARGO_PKG_NAME")` after `" git:"` so the runtime log prints `git:tolaria` rather than a hash — that's a real bug, not a style nit.

5. **Tombstone comments documenting removed code.** `note_toolbar.rs` carries a 7-line comment block in place of a deleted `stub_cell` helper (`144a8884`). `a71cc191` duplicates a 10-line "why `Window::dispatch_action`" comment across four toolbar cells. Per the project's "no comments for removed code" rule (CLAUDE.md), delete the tombstones; the git log carries the story. If the dispatch rule needs in-code prose, anchor it once at the module level.

6. **String-typed panel keys / dock identifiers.** Multiple `Option<String>` panel-key fields and ad-hoc string keys (`right_dock_panel_key`, `MockVault` stem→id index in `8897ab93`, `SidebarSelection::Favorite(u64)` instead of `Favorite(NoteId)` in `9a3839c9`). Newtype the `PanelKey` and `NoteId` boundaries so the wire format is the *only* place `String`/`u64` lives. R-4.

7. **Unsafe / objc2 invariants under one comment.** `0206465d` lumps three distinct FFI invariants under one `// SAFETY:`; `a20b1295` marks `ns_window` `unsafe fn` for ceremony while both callers wrap it in `unsafe { ... }` without restating obligations. Pick a layer (caller proves invariants, OR function is safe and proves them internally) and stick to it.

8. **`expect(POISON_MSG)` on a `Mutex` poisoned by render panics.** `2e666913` plus the `6796dc0a` menu-rebuild path now panic on every subsequent menu refresh after the first render panic. Add poison-recovery or shift to `parking_lot::Mutex` if the lock is genuinely never expected to be poisoned.

---

## Per-commit findings

The per-commit reports follow in chronological order (newest → oldest), grouped by batch.

---

# Batch A — 2026-05-22 (commits 1–10)

# Idiomatic Rust review — Batch A (10 commits)

Scope: feat/native-gpui-chrome, commits 144a8884 → d9387f49.
Reviewed `.rs` diffs only. Findings are MUST → SHOULD → MAY, capped per commit.

---

### 144a8884 — fix(workspace): force right-dock to initial width on first open via external ResizableState

**MUST**
- **[BUG] crates/workspace/src/workspace.rs:202-204** — Magic index `3` baked into a deeply-nested closure. The panel order `[left, note-list, center, right]` is conditionally constructed elsewhere in `Render` (left dock is `is_open()`-gated; `note_list_column` is `Option<AnyView>`). If a future change ever short-circuits one of those upstream panels, this `resize_panel(3, …)` either targets the wrong panel or panics out-of-bounds on `Vec::get`. Either lift the index to a named constant *next to the panel-push order* in `Render`, or compute the index from the iter (`panels.len() - 1`) at the moment the slot is pushed and stash it in the workspace.
  ```rust
  this.main_resizable_state.update(cx, |state, cx| {
      state.resize_panel(3, initial, window, cx);
  });
  ```
  ```rust
  // Either: const RIGHT_DOCK_PANEL_INDEX: usize = 3; // [left, note-list, center, right]
  // — with a debug_assert!(panels.len() == 4) at the push site.
  // Or: store the right-dock panel index on `TolariaWorkspace` and
  //     update it inside Render when the slot is pushed.
  ```

**SHOULD**
- **[R-3] crates/workspace/src/workspace.rs:135-141** — `right_dock_ever_opened: bool` is a domain boolean tracking a state machine. The current shape will silently grow: e.g. a "force-resize on next user-driven open" case can't be encoded. Prefer an enum.
  ```rust
  right_dock_ever_opened: bool,
  ```
  ```rust
  enum RightDockInitialWidth {
      Pending,   // first open hasn't fired yet; observer will force the initial width
      Settled,   // observer ran; user resizes win from here on
  }
  right_dock_initial_width: RightDockInitialWidth,
  ```

- **[STYLE] crates/workspace/src/workspace.rs:79** — `pub const WORKSPACE_RIGHT_DOCK_INITIAL_WIDTH_PT: f32 = WORKSPACE_LEFT_DOCK_INITIAL_WIDTH_PT;` contradicts ~20 lines of doc-comment immediately above, which still argues for **360pt** and discusses content density justifying a distinct value. Either the constant is wrong (regression vs the doc-comment's "bump to 360pt") or the doc-comment is stale. Pick one and align them — this is exactly the "stale doc comments that contradict the current constant value" smell.

**MAY**
- **[DOC] crates/note_item/src/note_toolbar.rs:657-662** — The "tombstone" block-comment documents the removal of `stub_cell`. Useful in the commit message; carrying it as a permanent code comment is noise. Delete the comment in a follow-up — readers can `git log -S stub_cell` to discover the history.

---

### 057cb642 — bottoms tweaks

**MUST**
- **[DOC] crates/note_item/src/note_toolbar.rs:327-333** — The commented-out "Defeered" (typo) block sits as a 7-line dead-code carcass between two active children. Either delete it (the new inspector cell below already calls out 9.2.19) or replace with a single-line `// TODO(9.2.5): AI panel cell — see ai_panel provider.`
  ```rust
  // Defeered
  // .child(stub_cell(
  //     "note-toolbar-ai",
  //     IconName::Asterisk,
  //     "Open AI assistant",
  // ))
  ```
  ```rust
  // TODO(worklist 9.2.5): re-attach the `note-toolbar-ai` cell once
  // the ai_panel provider lands; currently deferred.
  ```

**SHOULD**
- **[STYLE]** Commit subject "bottoms tweaks" (typo + opaque) is the only thing telling readers what this changes. Per AGENTS.md commit-style rules use `refactor(note_item):` or `feat(note_item):` — not a release-blocking issue but the diff is non-trivial (53 lines reshuffled) and deserves a real subject.

---

### 0e4f62d1 — feat(workspace,note_item): bump right-dock default to 360pt + restore note-toolbar inspector cell

**SHOULD**
- **[STYLE] crates/note_item/src/note_toolbar.rs (around inspector cell)** — The 12-line block-comment in front of the new `.child(toolbar_cell("note-toolbar-inspector", …))` reads more like a worklist entry than code documentation. The signal-to-noise on inline comments suffers when every cell carries a paragraph; consider moving the prose to a module-level doc that lists all toolbar cells in one place, and leaving inline comments at `// Worklist 9.2.19 — restore inspector cell; mirrors title-bar toggle.`

**MAY**
- **[R-4] crates/workspace/src/workspace.rs:79 (introduced here)** — `pub const … : f32` for a points-valued width invites accidental mixing with `f32` densities, scale factors, etc. A `Pixels` typed constant (`gpui::Pixels`) or a tiny `Points(f32)` newtype would catch unit confusion at compile time. Low priority — the codebase consistently wraps these in `px(…)` at use sites, so the risk is small.

---

### 54e7df0e — feat(note_list_pane): truncate over-long header titles with ellipsis

_No findings worth listing — a 9-line behavioural change with a reasonable docblock. The `.flex_1().min_w_0().truncate()` triplet is the standard GPUI/Tailwind ellipsis recipe. Comment honestly notes the regression that motivated the change._

**MAY**
- **[DOC]** The new comment block is long (~12 lines) for a 3-line CSS-ish change. Trimming to ~4 lines (rationale only, not historical context) would keep the file readable.

---

### 7ced27dd — fix(workspace,note_list_pane,tolaria): right-dock width + neighbourhood display title

**MUST**
- **[R-1 / BUG] crates/tolaria/src/main.rs:523** — `cx.foreground_executor().block_on(vault.note_content(id))` runs on the UI thread inside a synchronous action handler. The comment claims "the read is small (one note's body)" — but `vault::note_content` is async precisely because it isn't guaranteed to be small or fast (large notes, slow disks, network-mounted vaults). Blocking the foreground executor on disk I/O during a click handler is the kind of latency hit that produces visible jank. Two safer paths:
  1. Make `vault.note_content` offer a `note_content_sync(id) -> Result<String>` and call that from the action handler (the I/O still happens on the UI thread but the contract is honest).
  2. Spawn the resolve via `cx.spawn`, and update the header asynchronously when the body is loaded.
  ```rust
  let body = cx.foreground_executor().block_on(vault.note_content(id));
  body.ok()
      .as_deref()
      .and_then(note_list_pane::extract_title)
      .map(gpui::SharedString::from)
      .unwrap_or(stem)
  ```
  ```rust
  // Preferred: async spawn that updates the header when the body resolves.
  cx.spawn(|cx| async move {
      let body = vault.note_content(id).await.ok();
      let title = body.as_deref().and_then(note_list_pane::extract_title);
      if let Some(title) = title {
          cx.update(|cx| note_list.update(cx, |p, _| p.set_header_title(title.into())))?;
      }
      anyhow::Ok(())
  }).detach();
  ```

**SHOULD**
- **[R-2] crates/note_list_pane/src/lib.rs:382** — `extract_title(body: &str) -> Option<String>` — returning `Option<String>` allocates even when the title is unchanged. Prefer `Option<&str>` (slice into the input) and let callers `to_owned()` only when they actually need to store the value. Saves one alloc per header repaint and is more idiomatic for "extract from borrowed input".
  ```rust
  pub fn extract_title(body: &str) -> Option<String> { … }
  ```
  ```rust
  pub fn extract_title(body: &str) -> Option<&str> { … }
  ```

- **[STYLE] crates/workspace/src/workspace.rs:359-376** — The 16-line block-comment describing `ResizableState::sync_panels_count`'s `PANEL_MIN_SIZE` behaviour belongs in the constant's doc (or a module-level "Panel layout invariants" doc), not inline before two trivial `let` bindings. Inline comments this long become invisible to readers — they skip them.

**MAY**
- **[STYLE] crates/tolaria/src/main.rs:515-525** — Nested binding `let title = { let stem = …; let body = …; … };` is fine, but `match (note_sync, block_on(note_content)) { … }` would be a cleaner single expression and surfaces the "fallback to stem" branch more visibly.

---

### 55561ed7 — feat(actions,editor_bridge,note_item,tolaria,editor-host): note width toggle + neighbourhood header

**SHOULD**
- **[R-3] crates/editor_bridge/src/lib.rs:117-129** — `SetWideMode { wide: bool }` is a domain boolean encoding a display mode. `SetRawMode { enabled: bool }` already exists with the same shape; both would read better as `enum WidthMode { Reading, Wide }` / `enum EditorMode { Rich, Raw }`. Inside the bridge the wire format is just JSON, so the on-the-wire shape can stay (`#[serde(rename_all = "snake_case")]`); but Rust callers benefit from the named variant at every match site.

- **[R-3] crates/note_item/src/lib.rs:380-388** — `wide_mode: bool` on `NoteItem` (and the matching `raw_mode: bool` already there) — same flag-soup smell. Two booleans growing in lockstep is the precursor to "make illegal states unrepresentable" (R-9). When the third "view mode" lands, this should be one enum: `enum NoteViewMode { Reading, Wide, Raw, RawWide }` (or whatever combinations are legal).
  ```rust
  raw_mode: bool,
  wide_mode: bool,
  ```
  ```rust
  view_mode: NoteViewMode,
  ```

- **[STYLE] crates/note_item/src/lib.rs:639-674 (`toggle_wide_mode`)** — Near-verbatim copy of `toggle_raw_mode`. With two of these in place, the duplication starts to bite: same `cx.notify()` + `send_to_host` + identical doc preamble. Worth a small helper:
  ```rust
  pub fn toggle_wide_mode(&mut self, cx: &mut Context<Self>) -> Result<()> {
      self.wide_mode = !self.wide_mode;
      cx.notify();
      self.send_to_host(&ToHost::SetWideMode(SetWideMode { wide: self.wide_mode }), "SetWideMode", cx)
  }
  ```
  ```rust
  fn flip_and_push<F>(&mut self, flag: &mut bool, label: &'static str, build: F, cx: &mut Context<Self>) -> Result<()>
  where F: FnOnce(bool) -> ToHost { … }
  ```

**MAY**
- **[R-1] crates/tolaria/src/main.rs:1041-1056 (new `ToggleNoteWidth` handler)** — The `if let Err(e) = item.update(...) { log::warn!(...) }` pattern silently swallows the failure. For a toolbar action this is probably the right call; just consider whether the user needs a toast/status-bar message when the bridge call fails (consistent with raw-mode's handling).

---

### 5e8cc075 — fix(workspace,inspector_panel,editor-host): inspector width 280pt + shadcn css import

**SHOULD**
- **[STYLE] crates/inspector_panel/src/lib.rs:1458-1473** — The `default_size` method's doc-comment now spans 10 lines explaining why this value isn't the sidebar's value. That historical context is useful in the commit message and the worklist; carrying it on a method that just returns a constant is a maintenance hazard (the next reviewer will be tempted to "clean up" the comment without understanding why it's there). Move the rationale to a single sentence + a worklist ID; keep the constant's own doc-comment as the single source of truth.

**MAY**
- **[STYLE] crates/workspace/src/workspace.rs:60-69 (constant doc)** — Mentions "Mirrors the React app's `inspector: 280` default in `src/hooks/useLayoutPanels.ts:20`" — but commit 144a8884 then bumps to 360 (and aliases to the left-dock 200pt constant value). After that commit, this doc-comment is already stale within the same PR batch. Worth a follow-up cleanup pass to align constant doc-comments with the final value.

---

### 204971fb — Properties panel header

**SHOULD**
- **[STYLE] crates/inspector_panel/src/lib.rs:1635** — Variable rename `let toggle_button = div()…` → `let icon = div()…` is correct (the click handler & tooltip were removed), but the inner id `"inspector-panel-header-toggle"` is now misleading — it's an icon, not a toggle. Either drop the id (it's likely only used by `dump_as`-style debug tracing) or rename to `"inspector-panel-header-icon"`.
  ```rust
  let icon = div()
      .id("inspector-panel-header-toggle")
      …
      .child(IconName::Info);
  ```
  ```rust
  let icon = div()
      .id("inspector-panel-header-icon")
      …
      .child(IconName::Info);
  ```

**MAY**
- **[STYLE]** Commit subject `Properties panel header` doesn't follow the AGENTS.md `feat:` / `refactor:` convention. Minor.

---

### c66b6e1a — fix(inspector_panel): add w_full to render's outer div

**SHOULD**
- **[DOC] crates/inspector_panel/src/lib.rs:1574-1585** — Useful regression comment, but 12 lines of prose for a single `.w_full()` chain link is heavy. A 2-line note ("9.2.13 Reopened-3: flex column without explicit width collapses to zero; mirror `sidebar_panel::SidebarPanel::render`") would be enough — the test (added in d9387f49) is the durable artifact.

---

### d9387f49 — test(tolaria): pin full ToggleInspector dispatch chain end-to-end

**SHOULD**
- **[R-5] crates/tolaria/src/main.rs:2354-2384** — Three `Rc<Cell<u32>>` counters + an `Rc<RefCell<Option<…>>>` workspace slot is the "reflexive Rc/RefCell" smell from the cheat sheet. For a GPUI test these are sometimes the only option (the closures move into `cx.on_action`), but worth checking whether `gpui::TestAppContext`'s borrow-mut + a single `&mut` counter via `cx.update` could replace at least the counters. Even if Rc is needed, prefer `AtomicU32` (no interior mut machinery) for the per-hop counters:
  ```rust
  let handler_called = std::rc::Rc::new(std::cell::Cell::new(0u32));
  ```
  ```rust
  let handler_called = std::rc::Rc::new(std::sync::atomic::AtomicU32::new(0));
  // handler_called.fetch_add(1, Relaxed); … handler_called.load(Relaxed);
  ```

- **[STYLE] crates/tolaria/src/main.rs:2300-2480 (full test)** — 180-line test body is at the upper end of what's readable in a single `#[gpui::test]`. The `// Per-hop counters` block, the action registration, and the assertions could each become a small helper. Optional, but a future fifth hop (Phase 10) lands more cleanly with the test factored.

- **[VIS] crates/tolaria/src/main.rs:203** — Changing `dispatch_to_workspace` from private to `pub(crate)` is fine for test access, but the doc-comment doesn't mention this is a test-visibility relaxation. If the helper grows more callers in non-test paths, the API contract needs documenting. Tiny — a `# Visibility` paragraph would do.

**MAY**
- **[STYLE] crates/tolaria/src/main.rs:2440-2475** — The huge assertion message (`"after a Window::dispatch_action(ToggleInspector) the right dock must report…"`) inlines per-hop debug context inside a `panic!`-style format string. Useful when the test fails — but consider whether `assert!(condition, "summary: handler={} resolved={} factory={}", …)` keeps the lookup-by-grep ergonomic.

---

## Cross-cutting observations

- **Boolean flag soup.** `raw_mode` + `wide_mode` (and looming `ai_mode`, the deferred toolbar cell) keep pairing as parallel booleans. Worth a focused refactor before a third lands.
- **Magic panel indices.** `resize_panel(3, …)` in 144a8884 is the most fragile thing in the batch — the panel order is built conditionally in `Render`. One missed `if let Some(note_list_column)` away from `resize_panel(3, …)` resizing the *center* panel.
- **`block_on` on the foreground executor in click handlers** (7ced27dd) is a latency footgun that the comment acknowledges but doesn't justify away. Worth a follow-up to async this.
- **Inline doc-essays.** Several files now carry 10–20-line block comments on top of trivial changes. Useful for orchestrator review, but they reduce code readability for the next maintainer. Migrate the worklist-prose to constant doc-comments + commit messages.

---

# Batch B — 2026-05-22 → 2026-05-21 (commits 11–20)

# Rust Review — Batch B

Scope: 10 commits on `feat/native-gpui-chrome`.

---

### d209bfb0 — feat(tolaria): promote dispatch_to_workspace early-exit logs to warn

**SHOULD**

- **[STYLE] crates/tolaria/src/main.rs:224** — Log-level promotion via patch is a smell the original level was wrong, but the bigger issue is the comment block now dwarfs the change. A 14-line worklist preamble explaining why `debug!` → `warn!` is heavier than the code; trim to one sentence.
  ```rust
  // Worklist 9.2.13 (Reopened-3) — the three early-exit
  // branches below silently failed at `debug!` level, so a
  // dispatch that fell through any of them was invisible to
  // the user under default `cargo run` logging.  Promote to
  // `warn!`: each branch is a real "the chain broke here"
  // signal — `cx.active_window()` returning `None` after a
  // user click means the deferred closure raced the window's
  // lifetime; a non-`Root` / non-`TolariaWorkspace` window
  // root means the workspace mount changed shape (very
  // likely a regression).  None of these should be quiet.
  ```
  ```rust
  // `warn!` (not `debug!`): each branch means the dispatch
  // chain broke — a regressed window root or a raced closure.
  ```

**MAY**

- **[STYLE] crates/tolaria/src/main.rs:225** — The three `warn!` strings are near-identical; consider a single `warn!` after the branches using a `&str` reason variable, or a tiny helper, to avoid copy-pasted format strings. Low priority.

---

### 148378eb — feat(tolaria): make inspector dispatch trace visible without RUST_LOG

**SHOULD**

- **[STYLE] crates/tolaria/src/main.rs:593** — `eprintln!` for a one-shot build banner that survives `RUST_LOG` is reasonable, but mixing `eprintln!` and `log::info!("tolaria starting — build=…")` immediately after duplicates the same info on two channels. Pick one (the `log::info!` with `target: "tolaria"` is the idiomatic option) and drop the `eprintln!`.
  ```rust
  eprintln!("=== tolaria build={} ===", TOLARIA_BUILD_TAG);
  log::info!(
      target: "tolaria",
      "tolaria starting — build={} (worklist 9.3.5 build tag)",
      TOLARIA_BUILD_TAG,
  );
  ```
  ```rust
  // One channel — `log::info!` already prints unconditionally now that
  // `tolaria` is in the filter set.
  log::info!(
      target: "tolaria",
      "tolaria starting — build={}",
      TOLARIA_BUILD_TAG,
  );
  ```

- **[STYLE] crates/tolaria/src/main.rs:594-595** — A 7-line comment justifying a single `eprintln!` is the kind of tombstone-comment-on-arrival the cheat sheet flags. If the line stays, two lines suffice.

**MAY**

- **[STYLE]** Cosmetic rustfmt-only churn in `mod tests` (joined multi-line generic params) — fine but inflates the diff against the substantive change.

---

### 40fd9f44 — fix(tolaria): downgrade EnterNeighborhood + ToggleRawEditor info! → debug!

_Skipped — purely log-level reversal of work that was added in earlier commits (a71cc191 / fa740de6). The fact that two commits in this batch flip the same lines info↔debug is itself the finding, covered under d209bfb0 / 148378eb / b1614df8 / d9766aa5._

---

### fa740de6 — feat(tolaria): neighbourhood toggle on/off

**MUST**

- **[R-5] crates/tolaria/src/main.rs:444-449** — `handle_enter_neighborhood` takes `&crate::open_note::ActiveNoteItemSlot`, which is itself `Rc<RefCell<Option<Entity<NoteItem>>>>`. Passing `&Rc<RefCell<…>>` is the worst of both worlds — the helper doesn't actually need ownership of the `Rc`, just borrow access to the inner `Option`. Prefer `&RefCell<Option<Entity<NoteItem>>>` (or just `Option<Entity<NoteItem>>` via `slot.borrow().clone()` at the call site) so the helper isn't coupled to the smart-pointer choice.
  ```rust
  pub(crate) fn handle_enter_neighborhood(
      active_note_item: &crate::open_note::ActiveNoteItemSlot,
      note_list: &gpui::Entity<note_list_pane::NoteListPane>,
      prev_scope: &std::cell::RefCell<Option<note_list_pane::NoteListScope>>,
      cx: &mut gpui::App,
  ) {
  ```
  ```rust
  pub(crate) fn handle_enter_neighborhood(
      active_item: Option<gpui::Entity<note_item::NoteItem>>,
      note_list: &gpui::Entity<note_list_pane::NoteListPane>,
      prev_scope: &std::cell::RefCell<Option<note_list_pane::NoteListScope>>,
      cx: &mut gpui::App,
  ) {
  ```

**SHOULD**

- **[R-9] crates/tolaria/src/main.rs:584** — `Neighborhood(_, _) => "Inbox"` in `scope_display_label` is a "this branch must never fire, but we have to compile" sentinel. Make illegal states unrepresentable: either narrow the input to a `NonNeighborhoodScope` enum (a newtype over the four valid variants) or return `Option<SharedString>` and let the caller decide. The current shape silently mislabels if the invariant breaks.

- **[R-11] crates/tolaria/src/main.rs:512** — `prev_scope.borrow_mut().take().unwrap_or(NoteListScope::Inbox)` is fine, but the surrounding state machine is implicit in three mutable `RefCell`s (slot, prev-scope, anchor global). Two of the three are described in a 20-line comment block on each touch — that's a sign the toggle wants a struct (`NeighborhoodToggle { slot, prev_scope, anchor }`) with `enter(&mut self, …)` / `exit(&mut self, …)` methods. Phase-10 ADR-worthy, not a blocker.

- **[STYLE] crates/tolaria/src/main.rs:511** — The shared previous-scope memory `let neighborhood_prev_scope: std::rc::Rc<std::cell::RefCell<Option<note_list_pane::NoteListScope>>>` is declared with a 30-line block-comment preamble. Worth a `type PrevScopeSlot = Rc<RefCell<Option<NoteListScope>>>;` alias and one short doc-comment.

**MAY**

- **[STYLE] crates/tolaria/src/main.rs:567-589** — `Neighborhood(_, _)` returning `"Inbox"` could equally be `unreachable!()` with a doc explaining why the variant is dead in this codepath; either works, but the silent `"Inbox"` fallback is genuinely wrong if the invariant ever breaks.

---

### b1614df8 — feat(workspace,tolaria): instrument inspector dispatch chain + build-tag log

**MUST**

- **[BUG] crates/tolaria/src/main.rs:420-430** — `TOLARIA_BUILD_TAG`'s docstring claims `GIT_HASH` falls back to `unknown` via `option_env!`, but the code actually concatenates `env!("CARGO_PKG_NAME")` after `" git:"`. The runtime log will print `git:tolaria`, not a hash or `unknown`. Either implement what the doc says or fix the doc.
  ```rust
  const TOLARIA_BUILD_TAG: &str = concat!(
      "v",
      env!("CARGO_PKG_VERSION"),
      " git:",
      // `option_env!` returns `None` when `GIT_HASH` is unset, so the
      // `unwrap_or` falls back to a literal sentinel — same shape as
      // the React side's `__GIT_COMMIT__` define in `vite.config.ts`.
      // The literal is matched by periscope smoke tests to confirm a
      // fresh build was actually picked up.
      env!("CARGO_PKG_NAME"),
  );
  ```
  ```rust
  const TOLARIA_BUILD_TAG: &str = concat!(
      "v",
      env!("CARGO_PKG_VERSION"),
      " git:",
      // `option_env!` returns None when GIT_HASH is unset; fall back
      // to the literal sentinel for plain `cargo run`.
      // NOTE: option_env! cannot be unwrapped inside concat! at const
      // context — use a const fn or a build script env-set.
      match option_env!("GIT_HASH") {
          Some(h) => h,
          None => "unknown",
      },
  );
  ```
  (Note: `concat!` doesn't accept arbitrary expressions; the proper fix is a `build.rs` that emits `cargo:rustc-env=GIT_HASH=…` and then `env!("GIT_HASH")` here, OR drop the `git:` segment entirely.)

**SHOULD**

- **[STYLE] crates/tolaria/src/main.rs:349-364, 380-389, 400-405** — Six `log::info!` calls describing branch entry / cache hit / cache miss inside `toggle_or_swap_right_dock_panel`. This is the "spammy info on a hot path" anti-pattern from the cheat sheet — every dock toggle now emits 2–3 info lines. Either:
  (a) collapse into a single end-of-function `log::info!` summarising the chosen branch, or
  (b) demote the inner two (`reusing cached entity` / `slot empty — constructing fresh entity`) to `debug!` and keep only the top-level branch log at `info`.

- **[STYLE] crates/workspace/src/title_bar.rs:212** — The title-bar click adds an `info!` "title-bar inspector click → dispatching ToggleInspector" log. Combined with the handler's own `info!` and the factory's `info!`, every inspector toggle now emits 3+ info lines. The note-toolbar version of this same pattern was downgraded to `debug!` two commits later (40fd9f44 / d9766aa5). Same treatment is warranted for the title-bar / handler / factory chain once the diagnostic phase ends — or skip the promote-then-downgrade churn by starting at `debug!`.

**MAY**

- **[DOC] crates/tolaria/src/main.rs:412-419** — The doc comment for `TOLARIA_BUILD_TAG` is longer than the constant. If you keep the sentinel-style declaration, two lines is plenty.

---

### d9766aa5 — feat(inspector_panel,workspace,note_item,tolaria): inspector chrome reshape

**MUST**

- **[R-5 / STYLE] crates/inspector_panel/src/lib.rs:125-157** — `toggle_button` and `close_button` inside `render_header_strip` are byte-for-byte identical except for `id`, tooltip, and child icon. Three layout/styling lines, three click closure lines, all repeated. Extract a `header_action_button(id, tooltip, icon, cell_tint, muted)` helper.
  ```rust
  let toggle_button = div()
      .id("inspector-panel-header-toggle")
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
      .tooltip(|window, cx| Tooltip::new("Hide Inspector").build(window, cx))
      .child(IconName::PanelRight);

  let close_button = div()
      .id("inspector-panel-header-close")
      // …identical except for id/tooltip/child icon
      .child(IconName::Close);
  ```
  ```rust
  fn header_action_button(
      id: &'static str,
      tooltip: &'static str,
      icon: IconName,
      cell_tint: Hsla,
      muted: Hsla,
  ) -> impl IntoElement { /* one definition */ }

  let toggle_button = header_action_button(
      "inspector-panel-header-toggle",
      "Hide Inspector",
      IconName::PanelRight,
      cell_tint,
      muted,
  );
  let close_button = header_action_button(
      "inspector-panel-header-close",
      "Close Inspector",
      IconName::Close,
      cell_tint,
      muted,
  );
  ```

**SHOULD**

- **[R-9 / STYLE] crates/inspector_panel/src/lib.rs:1461** — `default_size` returns `px(workspace::workspace::WORKSPACE_LEFT_DOCK_INITIAL_WIDTH_PT)`. The `workspace::workspace::…` double-segment is a smell that `WORKSPACE_LEFT_DOCK_INITIAL_WIDTH_PT` is hiding inside the wrong module; re-export it from the `workspace` crate root. Also: the name `WORKSPACE_LEFT_DOCK_INITIAL_WIDTH_PT` baked "left dock" into the constant, which the right dock and the inspector now both consume — rename to `WORKSPACE_DOCK_INITIAL_WIDTH_PT` or `DEFAULT_DOCK_WIDTH_PT`.

- **[STYLE] crates/workspace/src/title_bar.rs:178-210** — A 32-line block declaring a single inline `toggle_inspector` cell with the same shape as `toggle_sidebar` further up the function. Extract a `chrome_toggle_cell(id, icon, tooltip, action)` helper — there are now two identical cells in the same render function and a third (sidebar on the left) of the same shape.

- **[R-1 / STYLE] crates/inspector_panel/src/lib.rs:1561 / 1577** — Two `into_any_element()` calls per render, plus `render_header_strip` returning `AnyElement` rather than `impl IntoElement`. `AnyElement` is heap-erased and worth using only when polymorphism actually crosses an API boundary; here both call sites are local. Return `impl IntoElement` and skip the boxing.

**MAY**

- **[STYLE] crates/note_item/src/note_toolbar.rs:891-896** — Six-line tombstone comment marking the removed `toolbar_cell_builds_inspector_button` test. The git history is the authoritative record of removals; delete the tombstone.

- **[DOC] crates/inspector_panel/src/lib.rs:114-117** — The "one-parameter footgun" docstring on `render_header_strip` over-explains. One sentence ("takes `&App` and resolves theme tokens internally so callers can't pick the wrong colour") covers it.

---

### 43a9fcab — feat(tolaria): View menu — rename to Properties + restore Show Inspector for GPUI overlay

**SHOULD**

- **[R-3 / R-9] crates/tolaria/src/menus.rs:43-59** — `MenuState` now carries three `bool` axes (`sidebar_open`, `properties_open`, `inspector_overlay_picking`). Three positional `bool`s in a struct literal is exactly the case the cheat sheet flags (R-3) — a future caller will swap two of them and the test that "matches by label" won't catch it. Either give each axis a tiny enum (`SidebarVisibility::Open` / `Closed`), or keep `bool` but ensure construction sites always use named-field syntax (already mostly the case in tests).

- **[STYLE] crates/tolaria/src/menus.rs:53** — `inspector_picking` was renamed to `properties_open` to reflect the new semantics, but the docstring still says "Computed by `main.rs::rebuild_menus_with_workspace` as `right_dock_panel_key() == Some("inspector") && is_right_dock_open()`". The string `"inspector"` is the right-dock key (the action verb kept its legacy name), so the doc is technically right but reads as contradictory. Clarify the verb-vs-label split in one sentence.

**MAY**

- **[STYLE] crates/tolaria/src/menus.rs:434-449** — `view_menu_pins_action_per_entry` uses `Some(ToggleSidebar.name())`, etc., where `ToggleSidebar` is a unit struct constructed inline. Slightly cleaner: hoist the names into a `const`-like array.

---

### 7d697f5a — feat(note_item,tolaria): neighbourhood active-state + header pin

**MUST**

- **[R-9] crates/note_item/src/lib.rs:282** — `NeighborhoodAnchor(pub Option<NoteId>)` is a global with two meanings: `None` = no neighbourhood active, `Some(id)` = id is the active anchor. Public tuple field means any caller can write either variant at any time. With one tiny constructor (`NeighborhoodAnchor::for_note(id)` / `NeighborhoodAnchor::cleared()`) and a private inner, the "did we mean to clear or did we mean to set" footgun goes away.
  ```rust
  pub struct NeighborhoodAnchor(pub Option<NoteId>);
  ```
  ```rust
  pub struct NeighborhoodAnchor(Option<NoteId>);

  impl NeighborhoodAnchor {
      pub const fn cleared() -> Self { Self(None) }
      pub const fn for_note(id: NoteId) -> Self { Self(Some(id)) }
      #[must_use]
      pub fn matches(self, id: NoteId) -> bool { self.0 == Some(id) }
  }
  ```

**SHOULD**

- **[R-5 / R-11] crates/tolaria/src/main.rs:1090+** — Toolbar render now reads a `gpui::Global<NeighborhoodAnchor>`, while the production handler `set_global`s a new value and explicitly `cx.refresh_windows()`. Globals + manual refresh is the React Context anti-pattern in disguise. Sibling reactive options exist in GPUI (entity + observe), and going via global forfeits the change-detection. Not a blocker for this commit, but worth an ADR before adding a second global of this shape.

- **[STYLE] crates/note_item/src/note_toolbar.rs:580-589** — `neighborhood_active_color` is a 1-line function (`cx.theme().primary`) wrapped in a 7-line docstring. If the rationale is "track the sidebar's selection accent", that's two lines of doc; the function body inlines fine at the one call site.

**MAY**

- **[STYLE] crates/tolaria/src/main.rs (tests block)** — The end-to-end test `enter_neighborhood_updates_header_and_anchor` re-implements the handler inline (see lines reading `cx.on_action(move |_: &actions::EnterNeighborhood, cx| { … let title = vault.note_sync(id).map(...) … })`). The next commit (fa740de6) extracts `handle_enter_neighborhood` and switches this test to call it. Worth squashing the two so the test never lands referencing a duplicate handler.

---

### 338dddc9 — remove unnesessary items

**SHOULD**

- **[STYLE] commit subject** — `remove unnesessary items` (typo + no commit-type prefix). The repo's convention is `feat:` / `fix:` / `refactor:`, etc. — this should be `refactor(workspace): drop unused title-bar language / profile cells`.

(No Rust findings beyond the commit-message hygiene — the deletion itself is straightforward.)

---

### a71cc191 — fix(tolaria,note_item,workspace): route toolbar clicks via Window::dispatch_action

**SHOULD**

- **[STYLE] crates/note_item/src/note_toolbar.rs:211-310, crates/workspace/src/title_bar.rs:135-152** — Four identical 10-line "Worklist 9.2.X reopened-2 — see the `note-toolbar-neighborhood` comment above for why this is [`Window::dispatch_action`] rather than `App::dispatch_action`" tombstones across four cells, plus a fifth (longer) one on the title-bar sidebar toggle. One module-level doc comment at the top of `note_toolbar.rs` (or a `dispatch_action_window` shim function with the comment) would carry the rationale once. Stale-on-arrival comment debt — the cheat sheet flags exactly this.
  ```rust
  // Worklist 9.2.4 reopened-2 — see the
  // `note-toolbar-neighborhood` comment above for why this is
  // [`Window::dispatch_action`] rather than `App::dispatch_action`.
  .child(toolbar_cell_with_active(
      "note-toolbar-raw",
      …
      |window, cx| {
          log::info!(…);
          window.dispatch_action(Box::new(actions::ToggleRawEditor), cx);
      },
  ))
  ```
  ```rust
  // One file-level doc-comment near the top of `note_toolbar.rs`:
  // > All toolbar cells dispatch via Window::dispatch_action; see
  // > `dispatch_routing` module doc for the re-entrancy rationale.
  // Then each cell stays free of the boilerplate header.
  ```

- **[R-9 / STYLE] crates/note_item/src/note_toolbar.rs (multiple cells)** — Every cell hand-writes `|window, cx| { log::info!(...); window.dispatch_action(Box::new(<Action>), cx); }`. A `dispatch_via_window<A: Action>(action: A, label: &'static str)` helper returning the closure (or just a macro) collapses five copies. Once you have more than three identical click closures, the helper pays for itself.

- **[STYLE] crates/note_item/src/note_toolbar.rs:218-219, 244-249, 285-289, 311-315** — Each cell has a `log::info!("<cell>: click registered, dispatching <Action>")` that the next two commits downgrade to `debug!`. Land them at `debug!` to start with — the diagnostic-promotion / re-downgrade churn is itself documented as a smell.

**MAY**

- **[STYLE] crates/tolaria/src/main.rs (test block)** — The negative test `app_dispatch_action_from_inside_window_update_silently_drops` is a useful regression guard but its assert message is ~10 lines. Trim to one sentence; the docstring already carries the long-form rationale.

- **[STYLE]** Both new `#[gpui::test]`s share boilerplate (`add_window` + `activate_window` + `cx.run_until_parked()` + `on_action` + counter setup). A small `fn dispatch_setup(cx) -> (window, calls)` helper would dedupe.

---

# Batch C — 2026-05-21 afternoon → midday (commits 21–30)

# Idiomatic Rust review — Batch C

Reviewed 10 commits across the GPUI chrome (right-dock, inspector, toc, note-toolbar, vault, sidebar). Findings are scoped to the diff text shown by `git show --stat ... -- '*.rs'`.

---

### f075ac21 — feat(note_item,actions,vault,tolaria): more-overflow menu (9.2.7)

**MUST**
- **[R-5] crates/note_item/src/note_toolbar.rs:608** — the per-render `content` closure wraps both `Rc<PathBuf>` handles, then **clones them once on the outer call** and **clones them again every time the inner `PopupMenu::build` closure runs**. Since the only consumer is a `&Path` passed to `reveal_in_finder` / `copy_path_to_clipboard`, the second `Rc` layer is gratuitous.
  ```rust
  let reveal_path = Rc::new(reveal_path);
  let copy_path = Rc::new(copy_path);
  …
  .content(move |_, window, cx| {
      let reveal_path = reveal_path.clone();
      let copy_path = copy_path.clone();
      PopupMenu::build(window, cx, move |menu, _, _| {
          let reveal_path = reveal_path.clone();
          let copy_path = copy_path.clone();
  ```
  ```rust
  // PathBuf is already cheap to share via `Rc::clone` (one bump);
  // collapse the per-build inner clone since the move-closure can
  // take the `Rc` by value once.
  let reveal_path = Rc::new(note_path.clone());
  let copy_path = Rc::new(note_path);
  .content(move |_, window, cx| {
      let reveal_path = Rc::clone(&reveal_path);
      let copy_path = Rc::clone(&copy_path);
      PopupMenu::build(window, cx, move |menu, _, _| {
          menu.item(PopupMenuItem::new("Reveal in Finder")
              .icon(IconName::FolderOpen)
              .on_click({
                  let p = Rc::clone(&reveal_path);
                  move |_, _, _| reveal_in_finder(&p)
              }))
          // …
      })
  })
  ```

**SHOULD**
- **[R-1] crates/tolaria/src/main.rs:1158** — the production `Archive`/`Delete` action handlers call `Vault::archive_note(id).detach()` and then `cx.refresh_windows()`, so any IO error is swallowed silently with no user-visible feedback. The doc comments explicitly defer "ConfirmDelete" UX, but at least surface a `warn!` log via `task.detach_and_log_err()` (or a small `cx.spawn` that awaits and logs).
  ```rust
  cx.global_mut::<vault::Vault>().delete_note(id).detach();
  cx.refresh_windows();
  ```
  ```rust
  // Log failures so the silent-detach doesn't hide ENOENT / EPERM.
  let task = cx.global_mut::<vault::Vault>().delete_note(id);
  cx.spawn(async move |_| {
      if let Err(err) = task.await {
          log::warn!(target: "tolaria::delete", "delete_note failed: {err:#}");
      }
  })
  .detach();
  cx.refresh_windows();
  ```

- **[R-9] crates/vault/src/lib.rs:660** — `delete_note` returns `Task::ready(Err(VaultError::Rescan(_)))` **after** unlinking the file and removing the in-memory entry, but the doc comment says "keep the in-memory deletion in place (it's the source of truth)". That's a partial-success success masquerading as an error. Either keep the error and document "side effects already applied" prominently in the variant docs, or split the return into a richer enum (`enum DeleteOutcome { Clean, RescanLagged(io::Error) }`) so callers can distinguish.

**MAY**
- **[R-12] crates/tolaria/src/main.rs:1133-1188** — `Archive` and `Delete` handlers are 95% identical (slot pull, global check, log, dispatch, refresh). Factor a private helper `register_active_note_vault_action::<A>(slot, name, vault_fn)` to keep the two registration sites in lockstep.

---

### 7feee93c — fix(sidebar_panel,tolaria): refresh sidebar Inbox count on VaultChanged with fan-out task

**MUST**
- **[BUG] crates/tolaria/src/main.rs:954-978** — the fan-out task's exit condition is "*both* downgraded handles dropped" (`if !still_live { break }`). If the sidebar entity drops but the note_list_pane stays alive, the task will still try to `panel.update` every tick — fine — but the inverse case (one entity gone, the other still alive) is reasonable. The problem is when **both** are gone and the loop hits the break — it never gets there, because `rx.recv_async()` is the blocking step. Once both panels are dead the channel will keep delivering events (the vault holds the sender) and the task spins through `still_live = false → break` on the **next** event only. Acceptable, but worth noting. The bigger MUST is the comment in `set_frontmatter_bool` / `delete_note` / fan-out: each says clones share a single queue with "work-stealing" semantics. **This is correct for `flume`'s receiver clones**, so the fan-out task is necessary. Document this in the `Vault::watch_events` rustdoc more prominently (the current doc mentions it but only in the recently-added paragraph) so future authors don't add a parallel subscriber and silently break event delivery.

**SHOULD**
- **[R-5] crates/sidebar_panel/src/lib.rs:412-428** — `refresh_from_vault` constructs a full `fresh = Self::from_vault(cx)` (allocating new `Vec`s for `types`, `views`, `folders`, `samples`) just to copy 6 fields out. The total_count vault has hundreds of notes; this is O(N) allocations per VaultChanged. Either move the rebuild into a method that takes `&mut self` and writes in place, or keep the alloc but skip the copy by `std::mem::swap`-ing the targeted fields out of `fresh`.
  ```rust
  let fresh = Self::from_vault(cx);
  self.inbox_count = fresh.inbox_count;
  self.total_count = fresh.total_count;
  self.archive_count = fresh.archive_count;
  self.types = fresh.types;
  self.views = fresh.views;
  self.folders = fresh.folders;
  ```
  ```rust
  let mut fresh = Self::from_vault(cx);
  self.inbox_count = fresh.inbox_count;
  self.total_count = fresh.total_count;
  self.archive_count = fresh.archive_count;
  std::mem::swap(&mut self.types, &mut fresh.types);
  std::mem::swap(&mut self.views, &mut fresh.views);
  std::mem::swap(&mut self.folders, &mut fresh.folders);
  ```

- **[R-2] crates/sidebar_panel/src/lib.rs:444** — `build_from_samples(samples: Vec<SidebarSample>, ...)` takes the owning `Vec` only to iterate it once with `.iter().filter()` for the inbox count, then `for SidebarSample { kind, path, .. } in samples`. Either consume each `SidebarSample` once (current style) or take `&[SidebarSample]` and clone where needed; counting then re-iterating the owned `Vec` while still owning it is fine but the `inbox_count` walk reads `&SidebarSample` and the `for` walk moves it — confusing to read.

**MAY**
- **[DOC] crates/sidebar_panel/src/lib.rs:355-367** — the `from_mock` branch fabricates `is_organized: false` for every mock note "so the badge tracks the mock's `total_count` exactly". Worth a TODO to surface this as a `MockSample::with_organized(...)` knob once mock fixtures need a non-zero archive count.

---

### 0ceec477 — fix(note_item): paint organized cell as inner round disc, not a rounded-square fill

**SHOULD**
- **[R-3] crates/note_item/src/note_toolbar.rs:449** — `(active_bg, glyph_color, fill_disc)` is a 3-tuple of `Option`s where only 1-of-3 is `Some` per variant. This is exactly the case for an output enum: lift the destructuring into a tiny `ActiveResolved { bg: Option<Hsla>, glyph: Option<Hsla>, disc: Option<Hsla> }` (or, better, replace it with `enum CellPaint { Baseline, BgOnly(Hsla), GlyphOnly(Hsla), Disc { fg: Hsla, bg: Hsla } }`) so the render branch reads as a single `match`.

**MAY**
- **[R-11] crates/note_item/src/note_toolbar.rs:495** — `.child(if let Some(disc) = fill_disc { … .into_any_element() } else { icon.into_any_element() })` — both arms always end with `.into_any_element()`. Sink the call to the outer expression:
  ```rust
  let child: AnyElement = match fill_disc {
      Some(disc) => div().flex()... .child(icon).into_any_element(),
      None => icon.into_any_element(),
  };
  .child(child)
  ```

---

### cc2c26f8 — fix(tolaria): observable EnterNeighborhood/ToggleRawEditor handlers + slot-reading regression test

**SHOULD**
- **[R-12] crates/tolaria/src/main.rs:733-1043** — repeated `let Some(item) = slot.borrow().as_ref().cloned() else { log::warn!(...); return; };` is now duplicated across `ToggleRawEditor`, `EnterNeighborhood`, `Archive`, `Delete` (next commit). Each block has the same warn payload modulo target. Extract a helper:
  ```rust
  fn resolve_active_note_or_warn(
      slot: &ActiveNoteItemSlot,
      target: &'static str,
  ) -> Option<Entity<NoteItem>> {
      let item = slot.borrow().as_ref().cloned();
      if item.is_none() {
          log::warn!(target: target, "no active NoteItem — toolbar click reached the handler before preload_blank_webview populated the slot");
      }
      item
  }
  ```

**MAY**
- **[DOC] crates/tolaria/src/main.rs:1010-1025** — the `count == 0` warn for `EnterNeighborhood` is good UX-debug, but the message is dual-purpose (user-facing log AND developer hint). Consider splitting: `info!` for the count, `debug!` for the wikilink-hint paragraph. Today every empty-neighborhood click writes a paragraph-length log line.

---

### 2662e935 — fix(tolaria,inspector_panel,workspace,actions): wire InspectorPanel to right dock

**MUST**
- **[R-7] crates/tolaria/src/main.rs:331-349** — `toggle_or_swap_right_dock_panel` has the comment "Read-then-mutate is split into two `RefCell` borrows … a single `borrow().clone().unwrap_or_else` would panic". The actual code uses `let existing = slot.borrow().clone();` followed by `if let Some(p) = existing { p } else { … *slot.borrow_mut() = Some(p.clone()); p }` — this is correct, but the panic-prone alternative the comment warns about is also reachable for future authors. Add a `#[cfg(test)]` regression covering `slot.borrow_mut()` while the closure-captured `borrow()` is alive (the comment says this is the invariant; lock it in).

**SHOULD**
- **[R-2] crates/workspace/src/workspace.rs:259** — `right_dock_panel_key` returns `Option<String>` (owned) with the doc rationale "the caller doesn't have to hold the borrow across an `attach_right_dock` call". Fair, but the call sites (`tolaria::macos::toggle_or_swap_right_dock_panel`) immediately call `.as_deref()` to get `Option<&str>` and never store the `String`. Consider exposing both:
  ```rust
  pub fn right_dock_panel_key(&self, cx: &App) -> Option<String> {
      self.right_dock.read(cx).panel_key().map(str::to_string)
  }
  ```
  ```rust
  // Cheap version for handlers that don't outlive the dock borrow:
  pub fn right_dock_panel_key_owned(&self, cx: &App) -> Option<String> { … }
  pub fn right_dock_panel_key<'a>(&'a self, cx: &'a App) -> Option<&'a str> { … }
  ```
  At minimum, the existing API forces an unnecessary `String` allocation per click.

- **[R-4] crates/workspace/src/dock.rs:53** — `panel_key: Option<String>` — this is a stringly-typed panel identity. Newtype `PanelKey(&'static str)` would lock down the two valid values (`"toc"`, `"inspector"`) at the type level and let `set_panel`/`panel_key()` return `&'static str` rather than allocating a `String` per attach.

**MAY**
- **[STYLE] crates/workspace/src/dock.rs:75-82** — the `(size, open, key)` block reads `panel.read(cx)` once and tuples three accessor calls. Good. But `key.to_string()` allocates per attach — same R-4 newtype change would let it be a copy.

---

### 5a61722e — feat(inspector_panel): properties + relationships + info read-only sections (9.2.13a)

**MUST**
- **[R-5] crates/inspector_panel/src/lib.rs:947-1010** — `render_properties_body` is called on **every render**, and it ends with `pairs.iter().map(|(key, value)| { let key_label = SharedString::from(display_property_key(key.as_ref())); … })`. `display_property_key` allocates a `String`, then wraps it in `SharedString`. For a note with 8 frontmatter rows, that's 8 fresh `String`s per repaint. Move the formatting to `collect_properties` so the rendered label is a pre-computed `SharedString` carried on the `(SharedString, FrontmatterValue)` tuple, not recomputed every frame.
  ```rust
  pub properties: Vec<(SharedString, FrontmatterValue)>,
  // collect:
  active.frontmatter().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
  // render:
  SharedString::from(display_property_key(key.as_ref()))
  ```
  ```rust
  // Pre-compute the display label at resolve time:
  pub properties: Vec<PropertyRow>,
  pub struct PropertyRow {
      pub label: SharedString,   // already display-formatted
      pub value: SharedString,   // already render_value_string'd
  }
  ```

- **[R-5] crates/inspector_panel/src/lib.rs:1063-1083** — same issue in `render_relationships_body` / `render_info_body`. `format_inspector_date(&modified)` and `humanize_bytes(byte_size)` both run on every render. Cache the formatted strings on `InspectorState` so the render path is a pure read.

**SHOULD**
- **[R-9] crates/inspector_panel/src/lib.rs:478-490** — `is_relationship_key` lowercases the input on every call: `match key.to_ascii_lowercase().as_str() { … }`. For a 5-property note this is fine. For a 50-property note this is 50 allocations per resolve. Use a `const ARRAY: &[&str] = &["aliases", ...];` plus an `eq_ignore_ascii_case` scan:
  ```rust
  fn is_relationship_key(key: &str) -> bool {
      const RELATIONSHIP_KEYS: &[&str] = &[
          "aliases", "belongs-to", "belongs to", "owner",
          "related-to", "related to", "has", "parent", "child",
      ];
      RELATIONSHIP_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key))
  }
  ```

- **[R-2] crates/inspector_panel/src/lib.rs:577-595** — `humanize_bytes` returns `String`. Convert the call site to take a `u64` and produce a `SharedString` directly (it's always wrapped at the call site). Even better: define a `format_bytes(bytes: u64, out: &mut String)` so the buffer can be reused.

**MAY**
- **[R-12] crates/inspector_panel/src/lib.rs:599-617** — `format_inspector_date` allocates a `String`, calls `.find(' ')`, splits, conditionally allocates a second `String`. Two paths to a final value; a single `write!` into a small buffer or a `chrono::format::strftime` with a custom `%-d` shim would be tidier. Marginal.

---

### d3f5971e — fix(vault,note_item,note_list_pane): filled-disk organized icon + Inbox refresh on frontmatter change

**MUST**
- **[R-10] crates/vault/src/lib.rs:573-595** — `emit_frontmatter_changed` only sends `paths: vec![path.to_path_buf()]`. Each call allocates a fresh `Vec<PathBuf>` even when the receiver is dropped (the `debug!` branch). Cheap, but consider whether the `VaultChanged` shape should carry a `&Path` or a `Cow<Path>` to skip the allocation in the no-receivers case. Since the channel is unbounded and there's a real listener in production, this is borderline R-11. Leaving as MUST because the fast-path doubles the emit rate (worklist 9.2.12 reopened added one emit on the fast-path AND another on the slow path through this same function) — every `set_frontmatter_bool` is now a guaranteed `PathBuf::to_path_buf` allocation.

**SHOULD**
- **[R-9] crates/vault/src/lib.rs:530-543** — `set_frontmatter_bool` emits `emit_frontmatter_changed(&path)` **before** the background write returns. The doc comments are explicit ("in-memory ahead of disk"), but subscribers can now race: the notification fires synchronously, the executor runs `NoteListPane::refresh_from_vault` which calls `note_sync(id)` which sees the new value — fine. But if `safe_write_atomic` fails inside the executor, the in-memory map still says `true` and disk says `false` and **no second event fires** to roll back. Add a follow-up emit on write failure that re-resolves the in-memory state.
  ```rust
  Some(executor) => executor.clone().spawn(async move {
      let result = std::panic::catch_unwind(|| safe_write_atomic(...)).map_err(...);
      result.map(|_| ()).map_err(|source| VaultError::Io { path, source })
  }),
  ```
  ```rust
  // On Err: re-read disk and re-emit so subscribers can roll back.
  ```

**MAY**
- **[STYLE] crates/note_item/src/note_toolbar.rs:386-407** — `ActiveStyle` is good (R-3 enum-over-bool). The doc comment on `Fill` mentions WCAG AA — leave as is.

---

### 8897ab93 — feat(inspector_panel,tolaria): wire backlinks/references/instances/outline (9.2.8)

**MUST**
- **[R-5] crates/inspector_panel/src/lib.rs:289-308** — `InspectorState::resolve_from_mock` builds a `HashMap<String, (NoteId, SharedString)>` indexed by stem on **every resolve**. With ~hundreds of notes this is fine but it's per-render-when-active-note-changes. Cache the index on `InspectorPanel`, refresh only when the vault emits `VaultChanged`. (Same architectural smell as the per-render property-formatting in commit 5a61722e.)

- **[BUG] crates/inspector_panel/src/lib.rs:265** — `mock_outbound_links` is set via `scan_wikilinks(&active.content)`, but later `for stem in &mock_outbound_links { let key = stem.trim().to_ascii_lowercase(); ... key.rsplit('/').next().unwrap_or(key.as_str()) }`. The `unwrap_or` is correct (it's `Option::unwrap_or`), but reading `key.rsplit('/').next().unwrap_or(key.as_str())` is unusual — `rsplit('/').next()` on a string is always `Some` (returns the whole string if there's no `/`), so the `unwrap_or` branch is unreachable. Clippy may flag this. Use `.last()` for clarity or `.next().unwrap()` with a comment.

**SHOULD**
- **[R-2] crates/inspector_panel/src/lib.rs:419-447** — `resolve_type_instances` does the prefix match on `note.path.file_stem()` (already lowercased) for every iteration. Sort the result by `id` to keep iteration order stable, but the doc says exactly that — good. The function takes `&vault::Note` for the active note and `&Vault` for the scan — the borrow shape is clean. Minor: `format!("{type_name}-")` allocates a small `String`; if hot, pull into a `[u8]` slice match.

- **[R-12] crates/inspector_panel/src/lib.rs:856** — `body_to_headings` uses `extract_outline(body)` + post-process with `.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>().to_ascii_lowercase()`. This produces double-dashes for `## Foo Bar` (`-foo-bar`) and a leading dash for any non-alphanumeric prefix. Test fixture-only, but if it ever gets used to round-trip with the editor's anchor map there'll be a collision.

**MAY**
- **[STYLE] crates/inspector_panel/src/lib.rs:735** — `set_active` calls `if !self.headings.is_empty() { self.headings.clear(); }` — the empty check is redundant (`clear` on an empty `Vec` is a no-op). Just `self.headings.clear();`.

---

### 13bbc646 — feat(vault,sidebar_panel,note_list_pane,note_item,actions,tolaria): neighbourhood mode (9.2.3)

**MUST**
- **[R-1] crates/vault/src/lib.rs:824-846** — `Vault::backlinks` does **synchronous `std::fs::read_to_string` per note in the vault on the UI thread**. The doc rationalises this as "at Tolaria-scale … well under one frame", but for a vault with 2k notes that's 2k file reads per toolbar click. The function is `&self`-only so it can be wrapped in `cx.spawn(...)` at the call site, but the public API invites blocking misuse. Either:
  1. Mark the method `#[doc(hidden)]` with a `_blocking` suffix and expose an async variant via `Vault::set_executor`, or
  2. Build an inbound-link index lazily, invalidated on `VaultChanged`.
  ```rust
  pub fn backlinks(&self, id: NoteId) -> Vec<NoteId> {
      …
      let raw = match std::fs::read_to_string(&note.path) { … };
  ```
  ```rust
  // Async variant routes through the background executor:
  pub fn backlinks(&self, id: NoteId) -> Task<Vec<NoteId>> {
      let snapshot = self.snapshot_paths();
      self.background_executor.spawn(async move {
          // O(N) reads off the UI thread
      })
  }
  ```

**SHOULD**
- **[R-9] crates/sidebar_panel/src/lib.rs:127-130** — `SidebarSelection::Neighborhood(u64)` carries a raw `u64` "for consistency with `Favorite(u64)`". But every consumer immediately calls `NoteId::from_raw(raw_id)`. Make the variant carry `NoteId` directly:
  ```rust
  Neighborhood(u64),
  ```
  ```rust
  Neighborhood(NoteId),
  ```
  The cross-crate dependency is already there (`vault::NoteId` is used in the `from_or_empty` resolver path).

- **[R-12] crates/vault/src/lib.rs:951-995** — `WikilinkTargets` iterator is `pub`-by-construction-of-the-return-type. The `impl Iterator` return is fine; the named struct with `'a` is a noted choice. But the `loop { … continue }` shape would be simpler with a `while let Some(start) = ... { … }`. Marginal.

**MAY**
- **[R-12] crates/vault/src/lib.rs:879-895** — `outbound_links` builds `title_to_id: HashMap<&str, NoteId>` excluding `id`, then walks targets. The `seen` `HashSet<NoteId>` is correct for dedup but could be folded into the `title_to_id` lookup by storing `(NoteId, bool)` and flipping the bool, avoiding a second allocation.

---

### 5bd2533e — feat(editor_bridge,toc_panel,actions,note_item): table-of-contents panel + headings bridge (9.2.6)

**MUST**
- **[R-5] crates/toc_panel/src/lib.rs:286-336** — `render` is hot (every dock repaint) and builds a `Vec<AnyElement>` via `self.headings.iter().enumerate().map(|(i, h)| render_heading_row(i, h, fg, muted, accent)).collect()`. Each row allocates `SharedString::from(format!("toc-row-{index}"))` plus two `SharedString::from(heading.text.clone())` / `heading.anchor.clone()` calls — for a note with 30 headings that's 90 String allocations per repaint. Pre-compute the rendered `Vec<TocRow>` (id + text-shared + anchor-shared + indent) at `set_headings` time and reuse it on every render.
  ```rust
  // Today:
  let rows: Vec<AnyElement> = self.headings.iter().enumerate().map(...).collect();
  ```
  ```rust
  // Cache the per-row shared strings on set_headings:
  struct TocRow { id: SharedString, text: SharedString, anchor: SharedString, indent: Pixels, level_text_color_is_muted: bool }
  // Renders become a pure map over the cached Vec<TocRow>.
  ```

**SHOULD**
- **[R-2] crates/editor_bridge/src/lib.rs:226-249** — `Heading { level: u8, text: String, anchor: String }` is correctly owned for the wire boundary. But on the receiver side (`note_item::Outcome::EmitHeadings(Headings)` → `HeadingsUpdatedEvent { headings: Vec<Heading> }` → `TocPanel::set_headings(items: Vec<Heading>)`), the `String`s are cloned forward through the whole chain. Define a separate native `HeadingNative { level: u8, text: SharedString, anchor: SharedString }` for the chrome-side and convert once at the bridge boundary. Avoids the per-row clones in toc_panel's render path.

- **[R-4] crates/toc_panel/src/lib.rs:243-244** — `panel_key(&self) -> &str { "toc" }` returns a `&'static str`. Good. Tie this back to the R-4 finding on commit 2662e935 — newtype `PanelKey(&'static str)` would make the workspace dock's `panel_key` field typed.

**MAY**
- **[R-11] crates/toc_panel/src/lib.rs:359-377** — `render_heading_row` captures `anchor: SharedString` only to `log::info!` it in the click handler. The whole `TocHeadingClicked` event scaffolding is wired (`impl EventEmitter<TocHeadingClicked> for TocPanel`) but the `on_click` closure logs instead of emitting. Either emit and add a no-op subscriber, or drop the `EventEmitter` impl until the bridge envelope lands.

- **[STYLE] crates/note_item/src/lib.rs:706-714** — `Outcome::EmitHeadings(payload)` → `cx.emit(HeadingsUpdatedEvent { headings: payload.items })` discards the wire-shape wrapper. Fine, but if the bridge ever grows a field on `Headings` (e.g. `active_id`) the unpacking will need updating. Consider `cx.emit(HeadingsUpdatedEvent { headings: payload.items, /* … */ })` with a `From<Headings> for HeadingsUpdatedEvent` so the unpack is one call.

---

---

# Batch D — 2026-05-21 midday → 2026-05-20 evening (commits 31–40)

# Idiomatic Rust review — batch D

Branch: `feat/native-gpui-chrome`
Reviewer model: Opus 4.7 (1M context)
Commits: 10 (e5978cd4 → 184bd976)

---

### e5978cd4 — fix(vault,note_item): star + organized toggles survive external edits

**MUST**
- **[R-1] crates/vault/src/lib.rs:823** — `sync_in_memory_from_disk` silently swallows a YAML parse panic from `frontmatter::parse(raw)`. Disk content is user-controlled (an external editor can write malformed YAML); the fast-path is the load-bearing invariant for the user-perceived toggle, so it must not be allowed to wipe the in-memory frontmatter on bad bytes. Today there is no panic because `frontmatter::parse` returns a tuple, but the comment claims "frontmatter refresh is the load-bearing invariant" while the code provides no recovery if `parse` itself returns an empty `Frontmatter` for a partial/corrupted block — the toolbar would then read `false` for both flags, *worse* than the stale state we set out to fix.
  ```rust
  // anti-pattern
  fn sync_in_memory_from_disk(note: &mut Note, raw: &str, path: &Path) {
      note.frontmatter = frontmatter::parse(raw).0;
      if let Ok(meta) = std::fs::metadata(path) { … }
  }
  ```
  ```rust
  // suggested rewrite — only overwrite when parse yields a usable map
  fn sync_in_memory_from_disk(note: &mut Note, raw: &str, path: &Path) {
      let (parsed, _body) = frontmatter::parse(raw);
      if !parsed.is_empty() || raw.starts_with("---") {
          note.frontmatter = parsed;
      }
      if let Ok(meta) = std::fs::metadata(path) {
          note.byte_size = meta.len();
          if let Ok(t) = meta.modified() {
              note.modified = DateTime::<Utc>::from(t);
          }
      }
  }
  ```

**SHOULD**
- **[R-9] crates/vault/src/lib.rs:486** — The fast-path branch is now doing TWO conceptually distinct things (skip-write, re-sync). Hoisting the disk re-sync above the fast-path comparison and making the comparison purely "did we actually need to write?" makes the invariant unrepresentable-wrong:
  ```rust
  // current — fast path also re-syncs as a side effect
  if new_contents == raw {
      if let Some(note) = self.notes.get_mut(&id) {
          sync_in_memory_from_disk(note, &raw, &path);
      }
      return Task::ready(Ok(()));
  }
  ```
  ```rust
  // suggested — always re-sync from disk we just read, then decide whether to write
  if let Some(note) = self.notes.get_mut(&id) {
      sync_in_memory_from_disk(note, &raw, &path);
  }
  if new_contents == raw {
      return Task::ready(Ok(()));
  }
  ```
  This also removes a subtle bug: between the `read_to_string` above and the in-memory mutation later in the function, the in-memory state still disagrees with disk on the slow path — `set_frontmatter_bool` then calls `frontmatter.insert_bool(...)`/`remove(...)` on a *stale* `Frontmatter`, not the freshly-parsed one. Concurrent flips from disk (e.g. another tool added a key) silently get clobbered.

- **[BUG] crates/vault/src/lib.rs:478..492** — The merge strategy for "star toggle survives external edits" is effectively **last-write-wins, biased toward the in-memory snapshot** plus the new disk re-sync on the fast path. A concurrent external edit that *also* changed another key (say `_organized: true`) will be lost on the slow path because `notes.get_mut(&id).frontmatter.insert_bool(key, value)` mutates the *pre-read* frontmatter, then we write `new_contents` (derived from `set_bool_in_raw(&raw, ...)` — that part is fine on disk) but the in-memory map only reflects the local boolean change. Tests pin the single-flip case only.

**MAY**
- **[STYLE] crates/note_item/src/note_toolbar.rs:480** — The block comment on `cx.refresh_windows()` is ~9 lines explaining an idempotent nudge; a 2-line comment ("force redraw — vault `Global` doesn't auto-notify entities") would carry the same information.

---

### 8ee5fa33 — feat(note_list_pane): inbox scope hides organized notes

**MAY**
- **[STYLE] crates/note_list_pane/src/lib.rs:337** — `is_organized: bool` is the third `bool` on `NoteEntry`. Consider grouping triage flags into a single `TriageFlags { favorite: bool, organized: bool, archived: bool }` newtype (R-3 / R-9) once the next flag lands — keeps `scope_matches` predicates self-documenting (`!entry.flags.organized`) and prevents future `bool` arg-positional bugs at the `collect_vault_entries` call site.

- **[STYLE] crates/note_list_pane/src/lib.rs:892** — The mock branch hard-codes `is_organized: false`; if a future mock note ever needs to seed an "organized" state for snapshot tests this becomes a multi-call-site change. A `..NoteEntry::default()` pattern (if `NoteEntry: Default`) keeps the call self-documenting.

---

### e1d61a32 — feat(note_item): paint active star + organized toolbar glyphs

**MUST**
- **[R-1] crates/note_item/src/note_toolbar.rs:730..740** — Comparing `Hsla` channels via four `assert_eq!` lines per test is fine, but the comment "rgb→hsla is deterministic so byte-identity holds" is fragile: `f32` channel equality after a colour-space conversion is implementation-defined across gpui releases. Prefer a tolerance-based compare or assert on the `gpui::rgb(0xD69E2E).into::<Hsla>()` constant directly via `Eq` on the underlying `u32` rgb input:
  ```rust
  // anti-pattern — pinning f32 byte-identity through a conversion
  assert_eq!(color.h, expected.h);
  assert_eq!(color.s, expected.s);
  assert_eq!(color.l, expected.l);
  assert_eq!(color.a, expected.a);
  ```
  ```rust
  // suggested — assert on the source rgb constant + a tight epsilon
  const EPS: f32 = 1e-5;
  assert!((color.h - expected.h).abs() < EPS);
  assert!((color.s - expected.s).abs() < EPS);
  assert!((color.l - expected.l).abs() < EPS);
  assert!((color.a - expected.a).abs() < EPS);
  ```

**SHOULD**
- **[R-3] crates/note_item/src/note_toolbar.rs:303** — `active: bool` + `active_color: Option<Hsla>` encodes three states (off, on-with-bg-tint, on-with-glyph-color) but only three of the four `bool × Option` combinations are meaningful. Replacing with `enum CellActive { Off, OnTinted, OnGlyph(Hsla) }` makes illegal states (e.g. "active=false, color=Some") unrepresentable:
  ```rust
  // anti-pattern
  fn toolbar_cell_inner(
      …,
      active: bool,
      active_color: Option<Hsla>,
      …,
  ) -> AnyElement
  ```
  ```rust
  // suggested
  enum CellActive { Off, OnTinted, OnGlyph(Hsla) }
  fn toolbar_cell_inner(
      …,
      active: CellActive,
      …,
  ) -> AnyElement
  ```

**MAY**
- **[DOC] crates/note_item/src/note_toolbar.rs:371** — `star_active_color()` hard-codes `#D69E2E` with a TODO. A `const STAR_ACTIVE_RGB: u32 = 0xD69E2E;` at module scope makes the test assertion self-checking (`assert_eq!(star_active_color(), gpui::rgb(STAR_ACTIVE_RGB).into())`) and points the dark-mode follow-up at one place.

---

### 45b6622d — feat(editor_bridge,note_item,actions): raw-mode toggle

**MUST**
- **[BUG] crates/note_item/src/note_toolbar.rs:282..289** — GPUI dispatch from `on_click` MUST use `window.dispatch_action(Box::new(action), cx)` not `cx.dispatch_action(&action)`. This is the exact pattern flagged in user memory (`feedback_gpui_dispatch_from_click_closure.md`) — the latter silently fails because we're already inside the window's update.
  ```rust
  // anti-pattern (verbatim from diff)
  |_window, cx| {
      cx.dispatch_action(&actions::ToggleRawEditor);
      log::info!(
          target: "note_item::toolbar",
          "raw: dispatched ToggleRawEditor"
      );
  },
  ```
  ```rust
  // suggested rewrite
  |window, cx| {
      window.dispatch_action(Box::new(actions::ToggleRawEditor), cx);
      log::info!(
          target: "note_item::toolbar",
          "raw: dispatched ToggleRawEditor"
      );
  },
  ```
  *(Note: this was indeed fixed later in `a71cc191`, but it shipped as a regression in this commit and is the canonical anti-pattern in this codebase.)*

**SHOULD**
- **[R-3] crates/editor_bridge/src/lib.rs:106** — `SetRawMode { enabled: bool }` is a wire envelope that domain-models a two-state mode. Promote the bool to an enum so the JS-side dispatcher and the Rust caller can never invert the meaning silently:
  ```rust
  // anti-pattern
  pub struct SetRawMode { pub enabled: bool }
  ```
  ```rust
  // suggested
  #[derive(Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum EditorSurface { Rich, Raw }
  pub struct SetRawMode { pub surface: EditorSurface }
  ```
  The roundtrip test that pins `"enabled":true` would shift to `"surface":"raw"` — but the test already exists, so the rename is mechanically safe.

- **[R-8] crates/note_item/src/lib.rs:537..547** — `toggle_raw_mode` returns `Result` but the only failure mode (bridge send) is mutated-but-not-rolled-back: the doc comment explicitly says "the state mutation lands regardless of the bridge result." That's a leaky abstraction — `Result` becomes purely informational. Either:
  - return `()` and log the bridge failure internally (matches `detach()` precedent elsewhere), or
  - rollback `self.raw_mode = !self.raw_mode` on `Err` so `Result` carries real semantics.

**MAY**
- **[STYLE] crates/note_item/src/lib.rs:497..502** — `open_in_webview` mutates `self.raw_mode = false` then immediately constructs `pending_open = Some(NoteOpen { … })`. The reset-on-swap invariant is critical; a unit test exists only for "defaults to false." Add a regression that asserts the reset specifically (`open_in_webview` with `raw_mode = true` lands `false` after the call).

---

### 0bea15a3 — fix(vault) + docs(phase-9): close 9.2.1/9.2.2 (resolve sha, fix sync-test clippy)

**MAY**
- **[DOC] crates/vault/src/lib.rs:1329** — The replacement comment ("dropping the task is enough to assert the disk-side effect occurred") leaves an implicit invariant: `set_frontmatter_bool` performs the disk write **before** wrapping the result in `Task::ready` only on the no-executor branch. If a future contributor wires a background executor for the sync test, the assertion would race. A `let _ = … .expect("sync path")` (or an inline `assert!(matches!(_, Task::Ready(Ok(_))))` after polling once) would document the dependency more loudly than `drop`.

---

### 9a3839c9 — feat(vault,note_item,sidebar_panel): star + organized toggles wired to frontmatter

**MUST**
- **[R-7 + BUG] crates/vault/src/lib.rs:454..458** — Manual `match` returning `Err` instead of `?`, AND the pre-read happens on the foreground thread inside what is otherwise a `Task`-returning method:
  ```rust
  // anti-pattern
  let raw = match read_to_string(&path) {
      Ok(raw) => raw,
      Err(err) => return Task::ready(Err(err)),
  };
  ```
  More importantly, this pattern means the foreground thread blocks on disk I/O even when a background executor is installed — defeats the point of `set_executor`. The whole "read → mutate map → write" sequence should either run inside the spawned task (and refresh the in-memory map back on the foreground via `cx.update` from the caller) or use a non-blocking read. The TODO at line 489 ("on write failure, revert the in-memory mutation") confirms the author knew this path was unfinished.

- **[R-1] crates/note_item/src/note_toolbar.rs:301..304** — `cx.global_mut::<Vault>().set_frontmatter_bool(...).detach()` swallows the returned `Task<Result<…>>`. If the disk write fails (vault read-only, permission denied, disk full), the in-memory state is now ahead of disk forever and no chrome feedback fires. The "TODO(9.2-followup): on write failure, revert the in-memory mutation and surface a chrome-side toast" should at minimum land an error-log spawn:
  ```rust
  // anti-pattern
  cx.global_mut::<Vault>()
      .set_frontmatter_bool(id, key, value)
      .detach();
  ```
  ```rust
  // suggested — observe + log failures even before the toast UI lands
  let task = cx.global_mut::<Vault>().set_frontmatter_bool(id, key, value);
  cx.spawn(async move |_| {
      if let Err(e) = task.await {
          log::error!(target: "note_item::toolbar", "{key} toggle failed: {e:#}");
      }
  }).detach();
  ```

**SHOULD**
- **[R-4] crates/sidebar_panel/src/lib.rs:104** — `SidebarSelection::Favorite(u64)` payload is the raw value of a `NoteId` — losing the newtype on the wire. Use `vault::NoteId` directly:
  ```rust
  // anti-pattern
  Favorite(u64),
  ```
  ```rust
  // suggested
  Favorite(vault::NoteId),
  ```
  This already works for `View(SharedString)` / `Type(SharedString)` and removes the `id.get()` / `NoteId::from_raw(raw_id)` ping-pong in `main.rs` and `display_label`.

- **[R-1] crates/sidebar_panel/src/lib.rs:489** — `format!("note {id}")` synthesises a stand-in label when the caller asks for `display_label` of a `Favorite`. This will leak into UI surfaces (event subscribers, accessibility text) the moment a downstream consumer reads `event.display_label`. Either look up the live title from the vault here (consistent with how the title-bar resolves it elsewhere) or change the field to `Option<SharedString>` so the absence is honest.

- **[R-5] crates/sidebar_panel/src/lib.rs:1338..1370** — `sidebar_favorite_row` `clone()`s `entity` per row; for a hundred-row favourites list this is 100 entity-handle bumps per render. The same shape exists in the sibling row builders, so this isn't *new* tech debt — but `entity: &gpui::Entity<…>` taken by reference and only cloned inside the closure body would halve the allocations.

**MAY**
- **[STYLE] crates/vault/src/frontmatter.rs:130..148** — `insert_bool` / `remove` exposed `pub(crate)` are paired exclusively with `Vault::set_frontmatter_bool`. A single `set_bool(&mut self, key: &str, value: Option<bool>)` (None ⇒ remove) keeps the "absence == false" invariant in one place rather than two.

- **[STYLE] crates/vault/src/frontmatter.rs:419..437** — `line_starts_with_key`: rejecting "deeper indentation as a nested-map child" by checking for two leading spaces is correct for Tolaria's flat sheet but brittle. A doc-test pinning the contract (e.g. `assert!(!line_starts_with_key("  _favorite: true", "_favorite"))`) would document the rule near the code.

---

### 6f3311f5 — docs(plans): reorg native-gpui-chrome into per-phase folders

_Skipped — predominantly docs/snapshot moves; the only Rust touches are a 2-line path-string rename (`crates/sidebar_panel/src/lib.rs`) and a shell-script header. No idiomatic-Rust delta._

---

### ae377db9 — feat(tolaria): minimal GPUI inspector renderer — fixes invisible toggle

**MUST**
- **[R-1] crates/tolaria/src/inspector_renderer.rs:118..121** — `format!("{:?}", id.path.global_id)` and friends rely on `Debug` formatting at runtime in a renderer that runs every frame the inspector is open. Two issues:
  1. `Debug` output is not part of GPUI's stable API; a gpui upgrade can shift the rendering.
  2. The allocation happens unconditionally per frame.
  No actual idiomatic-rust *MUST* — but the requested sentinel/`unimplemented!()` audit comes back clean.

**SHOULD**
- **[R-5] crates/tolaria/src/inspector_renderer.rs:119..125** — Two `SharedString` allocations per frame for read-only diagnostic text. Caching the most recent `(id, formatted)` pair behind a `Cell` on the renderer would avoid the per-frame `format!`, but the renderer is debug-only and behind `#[cfg(debug_assertions)]`, so this is a low-priority MAY rather than a SHOULD if it ever proves hot.

- **[R-12] crates/tolaria/src/inspector_renderer.rs:33** — `_window: &mut Window` is unused; if gpui's `InspectorRenderer` signature requires it, suppress with `#[allow(unused_variables)]` once or pattern-bind `_: &mut Window` to keep clippy `unused_variables` happy without an underscore-prefixed parameter name that reads like "internal."

**MAY**
- **[STYLE] crates/tolaria/src/inspector_renderer.rs:171..217** — The two test `gpui::test` functions install the renderer twice across the test process. Because `set_inspector_renderer` is a process-global setter, the second test may race with the first if the runner ever parallelises. Wrap installation in a `sync::Once`/`OnceLock` (or assert it's already-installed) to make the test ordering invariant explicit.

---

### 4af47d87 — fix(tolaria,actions): restore GPUI inspector overlay as ToggleInspector

**SHOULD**
- **[R-9] crates/tolaria/src/menus.rs:46..56** — `inspector_picking: bool` field is documented as "a proxy over `Window::is_inspector_picking`" because gpui exposes no broader predicate. This is fine, but a single named newtype/enum here would prevent the field accidentally being interpreted as "inspector overlay open" by future readers:
  ```rust
  // suggested
  pub enum InspectorMenuState { Hidden, Picking }
  // …
  pub inspector: InspectorMenuState,
  ```
  Then `view_menu` matches on the enum and the label flip is total without leaning on a bool's semantics.

- **[R-10] crates/tolaria/src/main.rs:212..220** — The deferred `handle.update(cx, |root, window, app_cx|...)` chain holds the active-window slot for the duration of the closure, including the inner `workspace.update(...)` call. The comment correctly identifies the re-entrancy risk, but the helper does not protect against `f` itself calling `dispatch_to_workspace` again (recursive defer). A short comment ("`f` MUST NOT re-enter `dispatch_to_workspace`") would document the invariant.

**MAY**
- **[STYLE] crates/tolaria/src/main.rs:516..539** — The `#[cfg(debug_assertions)] { … } #[cfg(not(debug_assertions))] { … }` pair in the action handler is a 25-line block; extracting `fn toggle_gpui_inspector(cx: &mut App)` with the cfg-gate inside would let the action handler read as one expression and keep the `#[cfg]` gate next to the gpui call site only.

- **[DOC] crates/tolaria/src/main.rs:266..273** — Deleting `dispatch_to_any_window` is correct because the new code path doesn't need a fallback, but the comment "Gated by `cfg(debug_assertions)` because the only current caller is itself debug-only" disappeared with the function. The removal is fine; just noting that the rationale lived only in comments, not tests, so the next time someone needs a "dispatch from cold-start" helper they'll re-derive it from scratch.

---

### 184bd976 — Merge remote-tracking branch 'origin/feat/native-gpui-chrome'

_Skipped — merge commit. The only Rust delta vs. the second parent is in `src-tauri/src/pi_discovery.rs`, which is from a previously-reviewed branch and outside the scope of this batch._

---

# Batch E — 2026-05-20 evening (commits 41–50)

# Idiomatic Rust Review — Batch E

Repo: `/Users/konstantin/tolaria` · Branch: `feat/native-gpui-chrome`
Commits reviewed (chronologically: 10 → 1): `c1f896b3`, `3a90b03e`, `70f28d53`, `2e666913`, `6796dc0a`, `a20b1295`, `bcf0dda4`, `0206465d`, `93181648`, `dea0d042`.

---

### dea0d042 — refactor(chrome): replace OverlayTooltipExt fan-out with inline gpui_component::Tooltip — Angle-C2 Phase 3

**MUST**

- **[BUG] crates/note_list_pane/src/lib.rs:1320** — Behavior change hidden in a "refactor": the old code wrapped `sort_button` in a `div().id("note-list-sort-trigger").overlay_tooltip("Sort")` because `gpui_component::Button` does not implement `ParentElement`. The new code drops the wrapper `div` and calls `.tooltip("Sort")` on the `Button` *before* `.dropdown_menu_with_anchor(...)`. The previous in-tree comment explicitly warned that `dropdown_menu_with_anchor` returns `DropdownMenuPopover<Button>` which "doesn't re-expose `Button`'s builder methods"; the inline tooltip works only because it is chained *before* the wrap. This is load-bearing ordering — a future edit that re-orders these two builder calls (or any future contributor that "tidies" the chain) will silently lose the Sort tooltip. The behavior also drops the unique element id `note-list-sort-trigger` used by `dump_as` / tree-dump tooling, which may be referenced by tests.
  ```rust
  // anti-pattern — tooltip silently breaks if reordered
  let sort_button = Button::new(...)
      .label(...)
      .tooltip("Sort")
      .dropdown_menu_with_anchor(...);
  ```
  ```rust
  // suggested rewrite — keep the wrapping div + dump_as id so the
  // tooltip layering is structural, not call-order dependent
  let sort_button = Button::new(...)
      .label(...)
      .dropdown_menu_with_anchor(...);
  div()
      .id("note-list-sort-trigger")
      .child(sort_button)
      .tooltip(|window, cx| Tooltip::new("Sort").build(window, cx))
      .dump_as("note-list-sort-trigger")
  ```

**SHOULD**

- **[R-5] crates/note_item/src/note_toolbar.rs:69, 90, 211** — Every call site repeats the closure shape `|window, cx| Tooltip::new(text).build(window, cx)`. With six identical occurrences in this file alone (and ~15 across the diff) the boilerplate is begging for a thin `chrome_tooltip(text: impl Into<SharedString>)` helper returning the closure. This also makes the eventual swap back to `overlay_tooltip` (or any other tooltip primitive) a one-line change.
  ```rust
  // anti-pattern — duplicated closure shape, 15× in this commit
  .tooltip(|window, cx| Tooltip::new("Note type — click to change").build(window, cx))
  .tooltip(|window, cx| Tooltip::new("Sync status").build(window, cx))
  ```
  ```rust
  // suggested rewrite
  fn chrome_tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
      let text = text.into();
      move |window, cx| Tooltip::new(text.clone()).build(window, cx)
  }
  // ...
  .tooltip(chrome_tooltip("Note type — click to change"))
  ```

**MAY**

- **[STYLE] crates/note_toolbar.rs:208** — `toolbar_cell` accepts `tooltip: &'static str` and then captures it into a `move` closure with no lifetime bound. Since the helper already requires `'static`, taking `impl Into<SharedString>` would let callers pass formatted strings without giving up the same ergonomics.

---

### 93181648 — fix(inspector_panel): paint root with theme.background + theme.foreground

**SHOULD**

- **[R-12] crates/inspector_panel/src/lib.rs:825** — The new test only asserts `theme.background.a > 0.0` / `theme.foreground.a > 0.0`, which is a *weaker* invariant than the bug it's pinning. A theme that returns `Hsla { a: 0.0001 }` would pass the test but reproduce the black-on-black void. Either assert `a >= 1.0` (or `a == 1.0`) or test the actual render output via the existing `tree_dump` harness so the regression catches near-transparent fills too.
  ```rust
  // anti-pattern
  assert!(theme.background.a > 0.0, "...");
  ```
  ```rust
  // suggested rewrite
  assert_eq!(theme.background.a, 1.0,
      "theme.background must be fully opaque so the Inspector window is not all-black");
  ```

**MAY**

- **[STYLE] crates/inspector_panel/src/lib.rs:635-636** — The four locals (`border_color`, `muted`, `background`, `foreground`) all just rename `cx.theme().*` getters that return `Copy` types (`Hsla`). The renames pay off only if used twice; `background` and `foreground` are used once each. Inlining `.bg(cx.theme().background)` keeps the call site self-documenting and matches the existing chrome modules.

---

### 0206465d — fix(note_item,ui): WKWebView send-to-back z-order + UI objc2 deps — Angle-C2 Phase 2

**MUST**

- **[UNSAFE] crates/note_item/src/lib.rs:1020** — The `SAFETY:` comment on `fix_z_order_send_to_back` is good, but the `unsafe { wk.setValue_forKey(...) }` calls and the `addSubview_positioned_relativeTo` call all live inside a single `unsafe` block scoped over multiple statements (the `let Some(parent) = wk_view.superview() else { ... };` is itself an FFI call). Per the project's objc2/WKWebView rule, each individual unsafe operation should have a SAFETY comment that explains *why that call* is safe. The current single block-level comment lumps three distinct invariants together. At minimum, call out that `superview()` is documented to be main-thread-only and that `wk_view: &NSView = &wk` relies on the WryWebView → NSView Deref chain not being undermined by future objc2 upgrades.

**SHOULD**

- **[R-5] crates/note_item/src/lib.rs:1003** — `let wk: Retained<wry::WryWebView> = webview.webview();` retains the WebView for the entire function only to take a single `&NSView` reference from it. Since the caller already holds `webview: &wry::WebView` for the duration of the call, the local `Retained` clone is unnecessary refcount churn on every WebView spawn.
  ```rust
  // anti-pattern
  let wk: Retained<wry::WryWebView> = webview.webview();
  unsafe {
      let wk_view: &NSView = &wk;
      // ...
  }
  ```
  ```rust
  // suggested rewrite — borrow without bumping retain count
  let wk = webview.webview();
  // SAFETY: ... (per-call notes)
  unsafe {
      let wk_view: &NSView = &wk;
      // ...
  }
  ```
  (the retain is still implicit in `webview.webview()`'s return type; the point is to not introduce *another* named binding that prolongs the lifetime).

- **[R-3] crates/note_item/src/lib.rs:1041** — `parent.addSubview_positioned_relativeTo(wk_view, NSWindowOrderingMode::Below, None)` mixes a `bool`-shaped enum (`NSWindowOrderingMode::Below` vs `Above` vs `Out`) with a relative-to-nil sibling pointer. The intent ("send to back") is hidden behind a low-level AppKit verb. A thin local helper `send_to_back(parent, view)` would name the intent at the call site and keep the unsafe block focused on the one FFI selector.

**MAY**

- **[DOC] crates/note_item/src/lib.rs:935-937** — The doc-comment changes around `fix_window_background` switching from opaque `#1F1E1B` to `NSColor::clearColor()` are excellent (they preserve the historical rationale). However the function name is now actively misleading — it no longer *paints* anything, it *clears* a previously-painted fill. Consider renaming to `fix_window_background_transparent` or `clear_window_background` in a follow-up.

---

### bcf0dda4 — feat(tolaria,chrome): transparent workspace base layer — Phase 1 of Angle C2

_Skipped — only Rust change is a single struct-field set (`window_background: WindowBackgroundAppearance::Transparent`) plus a long explanatory comment. No idiomatic findings worth flagging._

---

### a20b1295 — perf+fix(ui): cache OverlayTooltip NSPanel + prefer-Above placement

**MUST**

- **[R-10] crates/ui/src/overlay_tooltip.rs:163-189** — `show_overlay` calls `cx.try_global::<OverlayTooltipState>()` and then immediately calls `handle.update(cx, |...| ...)` inside the `if let Some(handle) = cached` branch. `handle.update` re-enters GPUI's window machinery, which may itself touch globals — and the closure can call `cx.set_global(OverlayTooltipState { ... })` mid-update. Worse, on the `Err(err)` path we then call `open_cold(text, bounds, cx)` *while still inside* the prior `update`'s logical scope. The cached handle is read once at line 163 (good), but the subsequent `set_global` calls may race with another caller that reads the slot in between. Document the main-thread invariant explicitly (the file claims main-thread-only but doesn't enforce it via `Send`-bounds or assertions).
  ```rust
  // anti-pattern — global slot read, then mutated mid-update without an
  // assertion that we hold the only logical reference.
  let cached = cx.try_global::<OverlayTooltipState>().and_then(|s| s.window);
  if let Some(handle) = cached {
      let r = handle.update(cx, |view, w, cx| { ... });
      match r { Ok(()) => cx.set_global(...), Err(_) => { cx.set_global(default); open_cold(...); } }
  }
  ```
  ```rust
  // suggested rewrite — take the slot, work with a local, then put it back
  let cached = cx
      .try_global_mut::<OverlayTooltipState>()
      .map(|s| s.window.take())
      .flatten();
  // ... drive update / fallback ...
  // put new state back exactly once at the end
  cx.set_global(OverlayTooltipState { window: Some(handle), visible: true });
  ```

**SHOULD**

- **[R-7] crates/ui/src/overlay_tooltip.rs:172-189** — `match update_result { Ok(()) => …, Err(err) => … }` is doing per-arm side effects, not error propagation. That is fine *but* the dual `cx.set_global(...)` calls on Ok/Err both rebuild the whole struct manually; a tiny `OverlayTooltipState::with_window(handle, visible)` constructor would dedupe and prevent the easy bug of forgetting to flip `visible` on one of the branches.

- **[UNSAFE] crates/ui/src/overlay_tooltip.rs:316-317** — `unsafe fn ns_window(window: &Window) -> Option<Retained<NSWindow>>` is marked `unsafe` but has no `# Safety` clause for *callers* — the doc comment talks about return-value validity, not the precondition the caller must uphold. With an `unsafe fn`, the contract is what callers must guarantee. Either spell out the caller obligation ("must be called on the main thread, `window` must be a live GPUI window") or — better — make the function safe (the body's only unsafety is the `Retained::retain` of a pointer that GPUI guarantees is live, which is the same invariant the surrounding `pub(super) fn reposition` already documents). Right now both callers (`reposition`, `set_visible`) wrap the call in `unsafe { ... }` purely as ceremony.
  ```rust
  // anti-pattern
  unsafe fn ns_window(window: &Window) -> Option<Retained<NSWindow>> { ... }
  // SAFETY: ns_window only walks AppKit pointers that GPUI keeps alive ...
  unsafe {
      let Some(ns_window) = ns_window(window) else { return; };
      ...
  }
  ```
  ```rust
  // suggested rewrite — push the unsafety down to the single Retained::retain
  // call, make the helper safe, drop the redundant outer unsafe blocks.
  fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
      let raw = <Window as HasWindowHandle>::window_handle(window).ok()?;
      let RawWindowHandle::AppKit(appkit) = raw.as_raw() else { return None; };
      let ns_view_ptr: *mut NSView = appkit.ns_view.as_ptr().cast();
      // SAFETY: GPUI guarantees ns_view is non-null and live for the
      // duration of `window`'s borrow.
      let ns_view = unsafe { Retained::retain(ns_view_ptr) }?;
      ns_view.window()
  }
  ```

- **[R-5] crates/ui/src/overlay_tooltip.rs:46-47** — The unused `std::cell::Cell` and `std::rc::Rc` imports at the top of the file are no longer needed (the cache moved them out of `Render`-time state and into the global `OverlayTooltipState`). Likely a clippy `unused_imports` would catch this — verify before next push.

**MAY**

- **[STYLE] crates/ui/src/overlay_tooltip.rs:460-470** — The local `above_local_y` / `below_local_y` naming is fine, but the placement decision (`if above_local_y >= Pixels::ZERO`) is conceptually "does the tooltip fit?". Renaming or wrapping with a `Placement::Above | Placement::Below` enum (per R-3) would make `position_overlay`'s intent self-documenting and gives you a stable surface for the future `Placement::Left|Right` cases.

---

### 6796dc0a — feat(tolaria,workspace): dynamic Show/Hide menu labels for Sidebar & Inspector

**SHOULD**

- **[R-3] crates/tolaria/src/menus.rs:35** — `MenuState { sidebar_open: bool, inspector_open: bool }` is exactly the "domain bool pair" R-3 warns against. Two `bool`s where each ought to be `enum Visibility { Shown, Hidden }`. The transformation is small and prevents future "did you mean inspector_open or sidebar_open?" bugs (notice the test name "labels_track_each_axis_independently" — that's the smell).
  ```rust
  // anti-pattern
  pub struct MenuState { pub sidebar_open: bool, pub inspector_open: bool }
  let sidebar_label = if state.sidebar_open { "Hide Sidebar" } else { "Show Sidebar" };
  ```
  ```rust
  // suggested rewrite
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub enum Visibility { #[default] Hidden, Shown }
  impl Visibility {
      fn label(self, noun: &'static str) -> String {
          match self { Visibility::Shown => format!("Hide {noun}"), Visibility::Hidden => format!("Show {noun}") }
      }
  }
  pub struct MenuState { pub sidebar: Visibility, pub inspector: Visibility }
  ```

- **[R-5] crates/tolaria/src/main.rs:235-256** — `rebuild_menus_with_workspace` and `rebuild_menus` differ only in how they get the workspace handle. Both end at the same `cx.set_menus(menus::app_menus(state))` call. Consider folding into a single function that takes `Option<&TolariaWorkspace>` (cleaner than two near-identical wrappers). The current shape risks the two diverging when a future field gets added to `MenuState`.

**MAY**

- **[STYLE] crates/tolaria/src/main.rs:407-409** — The two-line comment "The rebuild runs *inside* the same deferred closure as the toggle so it observes the post-toggle dock state" is load-bearing; consider promoting to a `#[doc]` attribute on the action handler (or extracting the closure to a named function) so a future refactor can't accidentally pull the rebuild outside the closure.

---

### 2e666913 — feat(tolaria,actions): open InspectorPanel as a separate macOS window

**MUST**

- **[R-1] crates/tolaria/src/inspector.rs:79, 96, 117** — `inspector_slot().lock().expect(SLOT_POISON_MSG)` is `unwrap`/`expect` on a fallible path in library code. The justification (main-thread only, no contention) is mostly correct *but* poisoned mutexes happen when a panic occurs while the guard is held — and this slot is poked from action handlers that can themselves panic during a render. A poisoned mutex in `is_inspector_open` will then bring down the *menu rebuild* (which calls it from `rebuild_menus_with_workspace` in commit 6796dc0a), turning a recoverable render bug into a hard crash on every subsequent dispatch.
  ```rust
  // anti-pattern
  let guard = inspector_slot().lock().expect(SLOT_POISON_MSG);
  guard.is_some()
  ```
  ```rust
  // suggested rewrite — degrade gracefully on poison
  let guard = inspector_slot().lock().unwrap_or_else(|p| {
      log::warn!("inspector slot mutex poisoned; recovering");
      p.into_inner()
  });
  guard.is_some()
  ```

**SHOULD**

- **[R-5] crates/tolaria/src/inspector.rs:48-58** — `OnceLock<Mutex<Option<AnyWindowHandle>>>` is three layers of indirection for what is functionally a thread-local single-cell. Given the file's own comment ("GPUI dispatches actions on the main thread"), a `thread_local!` cell or a `std::cell::OnceCell<RefCell<Option<...>>>` wrapped in a small `unsafe impl Sync` newtype would communicate the actual invariant much better. The current shape signals "I expect cross-thread contention" while the code says the opposite.

- **[R-9] crates/tolaria/src/inspector.rs:138-153** — `close_inspector_window` takes the handle out of the slot before calling `handle.update`. Good. But if `handle.update` fails partway (panic, error), the slot is permanently empty even though AppKit may still have the window mounted. Consider: only `take()` the slot *after* a successful update, or — better — store the handle as a strong invariant ("present iff the window is mounted") and accept a brief inconsistency window with a `try_close` API that returns the handle on Err so callers can decide.

**MAY**

- **[STYLE] crates/tolaria/src/inspector.rs:107-110** — `*guard = Some(handle.into()); drop(guard);` is redundant — the guard naturally drops at end of scope two lines later when the `match` arm ends. The explicit `drop` is a fine readability nudge, but moving the `Some(handle.into())` assignment into a single `*guard = Some(handle.into());` and letting the natural scope end is cleaner.

- **[DOC] crates/actions/src/lib.rs:36-48** — Excellent rustdoc on the two new actions. The cross-reference to "worklist 3.1 in `docs/plans/.../phase-8-issues.md`" is great because it traces the design intent; the only nit is that `///` comments inside `gpui::actions!` macro invocations don't always render in `cargo doc` depending on macro internals — worth a quick `cargo doc --open` check to confirm they survive expansion.

---

### 70f28d53 — refactor(chrome): use OverlayTooltipExt across all chrome surfaces

_Skipped — pure call-site fan-out of the existing `OverlayTooltipExt` API across four chrome modules (status_bar, sidebar_panel, note_list_pane, title_bar). The same patterns reviewed in `dea0d042` and `a20b1295` apply; no additional findings._

---

### 3a90b03e — refactor(sidebar_panel): repurpose sidebar-types-sort as Types Filter button

_Skipped — three-line label/icon/id rename inside an existing `header_action(...)` call. Pure cosmetic._

---

### c1f896b3 — feat(vault,tolaria): wire notes-list Add button + Cmd+N to create untitled note

**MUST**

- **[R-1] crates/note_list_pane/src/lib.rs:614-624** — `collect_vault_entries` uses `executor.block_on(vault.notes())` and `executor.block_on(vault.note_content(id))` on the foreground executor inside what is effectively a render-path helper. `from_vault` already did this, but the worklist-2.19 `refresh_from_vault` call now triggers this on every new-note creation. If `vault.note_content` is slow (network FS, large file) this blocks the UI thread mid-render. The doc-comment even calls out "Cheap for the demo vault (~30 files); future work can batch this through a vault-side snippet cache" — that promise of "future work" is exactly when these calls bite. Mark this as a known regression risk and gate behind a fast-path that only re-reads the freshly-created note's content, reusing the cached entries for the rest.

- **[R-7] crates/vault/src/lib.rs:418-460** — `Vault::create_note` correctly uses `?` for the I/O write (good) but the post-rescan lookup falls back to manually constructing a `VaultError::Io { source: std::io::Error::new(ErrorKind::NotFound, "...") }`. The strict R-7 reading is fine here (this isn't propagation, it's synthesis), but the synthesised `io::Error` loses the original context — and the new `VaultError::Rescan(anyhow::Error)` variant added just above is the right shape to surface "post-write reconciliation problem" without forging an `io::Error`. Add a dedicated `VaultError::CreatedNoteMissing { path: PathBuf }` variant.
  ```rust
  // anti-pattern — synthesised io::Error pretending to be a real one
  .ok_or_else(|| VaultError::Io {
      path,
      source: std::io::Error::new(
          std::io::ErrorKind::NotFound,
          "freshly-created note not found in post-rescan index",
      ),
  })
  ```
  ```rust
  // suggested rewrite
  #[error("freshly-created note {path:?} did not appear in post-rescan index")]
  CreatedNoteMissing { path: PathBuf },
  // ...
  .ok_or(VaultError::CreatedNoteMissing { path })
  ```

**SHOULD**

- **[R-2] crates/vault/src/lib.rs:418** — `pub fn create_note(&mut self, stem: &str) -> Result<NoteId, VaultError>` is fine, but every caller in the codebase hard-codes `"untitled"`. R-2 wants general inputs; the API already takes `&str`, so that's OK — but the doc-comment promises "{stem}.md" and the public surface lacks any input validation (what if the caller passes `"untitled/../../etc/passwd"`?). Either canonicalise/strip path separators inside `create_note` or document the precondition (R-2 doesn't override input safety).

- **[R-7] crates/tolaria/src/open_note.rs:159-169** — `create_and_open_untitled` uses `?` and `.context(...)` correctly (good). But the `note_list.update(cx, |list, cx| { list.refresh_from_vault(cx); list.set_active(Some(new_id), cx); });` after the create cannot fail and silently swallows any error from `refresh_from_vault` (which logs and returns rather than erroring). If `refresh_from_vault` cannot find the freshly-created note (race with an external delete), the editor still calls `open_note(...)` with a `new_id` that may not resolve. Add a fast assert that the entry landed.

- **[R-5] crates/tolaria/src/main.rs:639-647 + 680-687** — The five lines that create `create_slot`, `create_list`, `action_note_list` are three clones of the same two `Entity` handles for three different subscribers. The clone-spam is the GPUI idiom (R-5 explicitly allows entity handle clones — they're cheap), but a small `let entities = (note_list.clone(), active_note_item.clone());` and then `let (list, slot) = entities.clone();` per closure would compress the visual surface. Optional.

**MAY**

- **[STYLE] crates/vault/src/lib.rs:475-481** — `allocate_note_path` returns `VaultError::Io { ... AlreadyExists ... }` on suffix exhaustion. The error variant is wrong: this is not an I/O failure, it's a domain exhaustion. Consider `VaultError::SuffixExhausted { stem: String, limit: u32 }`.

- **[STYLE] crates/vault/src/lib.rs:445-448** — `std::fs::OpenOptions::new().write(true).create_new(true).open(&path)` writes an empty body. If the goal is "create an empty file", `std::fs::File::create_new(&path).map(drop)` is more idiomatic and matches the surrounding `std::fs` style.


---

# Batch F — 2026-05-20 afternoon → midday (commits 51–62)

# Rust Review — Batch F

12 commits reviewed against the idiomatic-Rust cheat sheet.
File paths/line numbers refer to the post-commit tree.

---

### 28296288 — feat(note_item): wire reveal/copy-path/inspector toolbar buttons

**MUST**

- **[BUG] crates/note_item/src/note_toolbar.rs:160-167** — Inspector cell dispatches via `cx.dispatch_action(&…)` from inside an `on_click` closure. Per project policy (and the user's memory note on GPUI dispatch), `on_click` callbacks fire *inside* the window's update; the `App::dispatch_action` path tries to re-enter and silently no-ops on the focused window. Use `window.dispatch_action(Box::new(action), cx)`. Other toolbar handlers in the same file (lines 269, 301, 324, 352, 366) already use this idiom, so the inspector cell is the lone deviation.
  ```rust
  // anti-pattern
  |_window, cx| {
      cx.dispatch_action(&actions::ToggleInspector);
      log::info!(target: "note_item::toolbar", "inspector: dispatched ToggleInspector");
  },
  ```
  ```rust
  // suggested rewrite
  |window, cx| {
      window.dispatch_action(Box::new(actions::ToggleInspector), cx);
      log::info!(target: "note_item::toolbar", "inspector: dispatched ToggleInspector");
  },
  ```

**SHOULD**

- **[R-2] crates/note_item/src/note_toolbar.rs:248** — `reveal_in_finder(path: &Path)` takes a borrowed `Path` but the call site copies into a `PathBuf` solely to feed this fn via `move`. Accept `impl AsRef<Path>` (and let callers pass owned or borrowed paths without the `to_path_buf()` dance), or simply accept `&Path` and clone only at the closure boundary that needs `'static` lifetime — not at every call site.

- **[STYLE] crates/note_item/src/note_toolbar.rs:255-269** — The `#[cfg(target_os = "macos")]` / `#[cfg(not(...))]` split inside `reveal_in_finder` is fine, but the macOS arm swallows the `Child` handle with `Ok(_)`. Prefer `Ok(child)` and a `log::debug!` of `child.id()` — easier to correlate against `ps` output during a periscope sweep, and matches the named-capture style used elsewhere.

- **[R-8] crates/note_item/src/note_toolbar.rs:262-265** — `log::warn!("reveal: open -R failed: {e:#}")` formats with `{:#}` which works for `io::Error` but produces a confusing single-line dump. Either drop `:#` (use `{e}`) or, if you want full context, lift to `anyhow::Error::new(e).context("open -R …")` for a proper chain.

**MAY**

- **[DOC] crates/note_item/src/note_toolbar.rs:222-226** — `stub_cell` doc says "the seven worklist rows (2.9-2.14, 2.17)" — that's eight cells (Star, Organized, Neighborhood, Raw, Width, AI, ToC, More). Tally is off by one; minor but worth tightening before this becomes the canonical "what's wired vs. stubbed" reference.

---

### 59526e52 — feat(ui): OverlayTooltip via WindowKind::PopUp so tooltips beat WKWebView z-order

**MUST**

- **[R-5] crates/ui/src/overlay_tooltip.rs:108-114** — `Rc<Cell<Bounds<Pixels>>>` plus a clone for the writer is the GPUI-idiomatic single-frame closure pattern, so this is fine — but the global `OverlayTooltipState` (line 145) stores `Option<AnyWindowHandle>` and is mutated through `cx.set_global(...)` which replaces the entire struct on every show/hide. Replace with `cx.update_global::<OverlayTooltipState, _>(|s, _| s.current = Some(handle.into()))` so external observers (if any later subscribe) see incremental change, not a wholesale swap. Strictly speaking this is R-9 territory ("make illegal states unrepresentable") more than R-5.

**SHOULD**

- **[R-7] crates/ui/src/overlay_tooltip.rs:194-201** — `let _ = handle.update(cx, |_, window, _| window.remove_window());` silently discards the `Result`. If the popup window has already been removed (race with a second hover-exit), the error is uninteresting and the swallow is correct — but write that intent: `if let Err(err) = handle.update(...) { log::debug!(target: "overlay_tooltip", "stale handle: {err}"); }`. Bare `let _ =` reads like a bug.

- **[R-4] crates/ui/src/overlay_tooltip.rs:49-56** — `TOOLTIP_WIDTH_PT: f32`, `TOOLTIP_HEIGHT_PT: f32`, `TOOLTIP_GAP_PT: f32` are conceptually `Pixels` (logical points), and downstream code immediately wraps them with `px(...)`. Define them as `Pixels` directly (`const TOOLTIP_WIDTH: Pixels = px(200.0);`) to eliminate the wrap site and prevent accidental misuse against a different unit. (`px` is `const fn` in gpui.)

- **[STYLE] crates/ui/src/overlay_tooltip.rs:154-178** — The `WindowOptions { … ..Default::default() }` struct literal with eight explicit fields is fine, but `focus: false, show: true, is_movable: false, is_resizable: false, is_minimizable: false` is screaming for an `..popup_defaults()` helper. If a second `WindowKind::PopUp` site lands later (and the module docs hint at fan-out) the duplication will rot.

**MAY**

- **[R-11] crates/ui/src/overlay_tooltip.rs:103-105** — `bounds_writer` is captured by `move` into the `on_prepaint` closure and then `trigger_bounds` is captured into the `on_hover` closure. Two clones of an `Rc` is fine, but you can instead clone only inside the `on_hover` closure body and let `on_prepaint` move the original — saves a `.clone()` call. Strictly cosmetic.

---

### 27ca5efd — fix(note_item): force tooltips above the toolbar so WKWebView doesn't occlude them

**SHOULD**

- **[R-11] crates/note_item/src/note_toolbar.rs:68-79, 101, 203** — Three call sites repeat `Tooltip::new(...).m_1().build(window, cx)`. With three sites already and more chrome migrations pending (per the file comment), wrap once: `fn chrome_tooltip(text: &'static str) -> impl Fn(...) { move |window, cx| Tooltip::new(text).m_1().build(window, cx) }`. This commit is the inflection point where extraction would have paid off — the next commit (59526e52) replaces all three with `overlay_tooltip(...)` anyway, so the smell is short-lived but worth flagging for next-time.

**MAY**

- **[DOC] crates/note_item/src/note_toolbar.rs:68-78** — The comment block explaining `.m_1()` mentions "the default 12 px to 4 px" — verify against the gpui_component constant. If the default ever changes upstream, this comment becomes a misleading breadcrumb. Cite the upstream symbol/path instead of the literal.

---

### 12ccea7a — feat(chrome): add hover tooltips to iconographic buttons

**SHOULD**

- **[R-11] crates/note_list_pane/src/lib.rs:1347, 1402; crates/sidebar_panel/src/lib.rs:912, 945; crates/status_bar/src/lib.rs:539, 589, 629, 653, 684, 700; crates/workspace/src/title_bar.rs:144, 234** — Twelve+ sites all spell out `.tooltip(|window, cx| Tooltip::new(LITERAL).build(window, cx))`. With the `OverlayTooltipExt` helper already landing in batch (59526e52), apply the same `.overlay_tooltip(...)`-style ergonomic shortcut to the `gpui_component::Tooltip` flavour (`.chrome_tooltip("…")` taking `impl Into<SharedString>`) so the call site is one line. This commit puts 12+ duplicate closures in the tree at once — exactly the regression the next commit fixes for the toolbar.

- **[R-3] crates/note_list_pane/src/lib.rs:1326-1333** — The "search_tooltip" `if self.filter_open { "Close search" } else { "Search notes" }` could be expressed against the existing `filter_open` boolean, but a dedicated `SearchButtonState::{Open, Closed}` enum (or simply: derive the label off the state inside a method on the pane) reads better and self-documents. Minor.

**MAY**

- **[STYLE] crates/status_bar/src/lib.rs:756, 782, 821, 843, 873** — The five test sites collapse `|window, cx| StatusBar::from_mock(window, cx)` → `StatusBar::from_mock` (point-free). Good. The same change is mechanically applicable to `StatusBar::from_or_empty` / `StatusBar::from_vault` — already done — and consistent across the file. No action; noted for context.

---

### 736c260e — feat(note_item): redirect WebView console + errors to env_logger

**MUST**

- **[R-1] crates/note_item/src/lib.rs:118-121** — `parse_console_envelope` returns `None` on missing fields, which is correct — but the call site in `with_ipc_handler` (lib.rs:866-869) drops the message silently when the envelope is malformed (`return;` short-circuits before the editor_bridge fallback decides). If a future shim version emits `{"__t":"console_log"}` with a typoed `"level"` key, the IPC frame is *both* recognised as console-bridge AND silently discarded. Consider: on prefix-match-but-parse-fail, log a `warn!` with the raw body before returning, rather than dropping. This isn't a panic in library code, but the silent-drop fork is the same shape of bug R-1 guards against.

**SHOULD**

- **[R-9] crates/note_item/src/lib.rs:122-128** — The `match level { "warn" => Warn, "error" => Error, "debug" => Debug, _ => Info }` is fine but maps `"trace"` to Info (with no warning) and conflates an unknown level with the JS-only `"log"` channel. Define `enum ConsoleLevel { Log, Info, Warn, Error, Debug }` deserialized via serde and have the mapping be exhaustive — illegal states (unknown strings) become explicit at decode time.

- **[R-2] crates/note_item/src/lib.rs:114-117** — `parse_console_envelope(body: &str) -> Option<(log::Level, String)>` returns an owned `String` — but `body` is already a `&str` slice from the IPC handler. If the JSON parse path can be replaced with `value.get("msg")?.as_str()?` + a `Cow<'a, str>` you save the allocation on every console line. Hot path (every editor `console.log`), so worth the change.

- **[R-8] crates/note_item/src/lib.rs:62-130** — The embedded `WEBVIEW_CONSOLE_BRIDGE_JS` is a raw 95-line `&str` constant. Ship it as `include_str!("webview_console_bridge.js")` from a sibling file so editors lint/format it and diffs read sensibly. Same pattern as `EDITOR_HOST_HTML` two lines above.

**MAY**

- **[DOC] crates/note_item/src/lib.rs:867-869** — Comment says "discriminate them first so they never hit decode_from_host's error path (which would log them as 'decode_failed')" — good. Inline the link to the upstream symbol (`editor_bridge::decode_from_host`) so a future refactor of that function name catches this comment too.

---

### 65790b55 — fix(note_item): enable WKWebView devtools + worklist 8.1.2 diagnostic; unmark 2.19

**SHOULD**

- **[BUG] crates/note_item/src/lib.rs:713-719** — `.with_devtools(true)` is committed unconditionally. The comment promises "remove or feature-gate before any production cut" but no `#[cfg(debug_assertions)]` is in place yet. The cheapest hedge today: `.with_devtools(cfg!(debug_assertions))` (or a `TOLARIA_DEVTOOLS=1` env-var gate). Land this in the same commit that adds the toggle, before the dogfood window closes.

**MAY**

- _Nothing else — diff is 6 lines of feature flag plus comment._

---

### a67c3af5 — update loogging

_Skipped — single-line log message tweak (`"tolaria starting (ADR-0115 Phase 5-MVP)"` → `"tolaria starting"`). Note: the commit subject typo ("loogging") and the absence of any other change confirm this is low-effort. I checked the diff carefully — no leftover `println!`, `dbg!`, or commented-out code. The only feedback: a one-line touch could have been folded into the next functional commit instead of standing alone with a typo-laden subject._

---

### 4ef676c3 — feat(note_list_pane): worklist 8.2.19/8.2.20/8.2.21/8.2.22 — note list top bar fixes

**SHOULD**

- **[R-5] crates/note_list_pane/src/lib.rs:1130-1135** — `pub fn close_filter(&mut self, window: &mut Window, cx: &mut Context<Self>)` calls `input.update(cx, |state, cx| state.set_value("", window, cx))` — fine, but you also need to call `cx.notify()` *after* the input update; right now `cx.notify()` is at line 1138 *before* the input mutation. The actual write happens inside the inner closure, so the ordering is OK, but a reader has to trace through `update` semantics to verify. Move `cx.notify()` to the end of the function for clarity.

- **[R-11] crates/note_list_pane/src/lib.rs:1278-1322** — The four-arm sort menu now has identical boilerplate per arm — `.on_click({ let e = sort_entity.clone(); move |_, _, cx| { e.update(cx, |p, cx| p.set_sort_order(NoteListSort::X, cx)); } })`. Extract once: `fn sort_menu_item(label: &str, order: NoteListSort, entity: &Entity<NoteListPane>) -> PopupMenuItem`. Four near-identical closures is the threshold where extraction pays for itself, and the cloning ritual is error-prone.

- **[R-3] crates/note_list_pane/src/lib.rs:107-134** — `NoteListSort::label` returns `"Modified ↓"` / `"Title A→Z"` etc. — fine, but the human-visible label and the *meaning* (direction + dimension) are now coupled inside one `&'static str`. If you later want a separate `direction()` / `dimension()` accessor (e.g. for SR/A11y), you'll re-parse the label. Consider:
  ```rust
  enum SortDirection { Asc, Desc }
  enum SortField { Modified, Title }
  impl NoteListSort {
      fn direction(self) -> SortDirection { … }
      fn field(self) -> SortField { … }
      fn label(self) -> &'static str { … }  // composed
  }
  ```
  Same surface, more compositional, R-9-friendly.

**MAY**

- **[STYLE] crates/note_list_pane/src/lib.rs:1382-1407** — The inline `h_flex().id("note-list-new")…on_click(…)` block has a long comment explaining why it isn't going through `header_icon_action`. The comment is right (the helper isn't interactive). Either: extract a sibling helper `interactive_header_icon(…)`, or leave inline but drop the long apology comment — the code is clear enough on its own.

---

### 632de0ab — feat(tolaria): worklist 8.2.7 — system menu adds File / View / Help submenus

**SHOULD**

- **[R-9] crates/tolaria/src/main.rs:417-449** — Seven `log_stub::<actions::X>(cx, "X", "Phase 9.x will …")` calls in a row, each restating the action name and a forward-looking sentence. The repetition is a smell: a macro `register_log_stub!(cx, OpenVault, "Phase 8.11 …")` would prevent the action-name-string drift (e.g. someone renaming `actions::OpenVault` → `actions::OpenWorkspace` without updating the literal). Either macro-ise, or — better — have `log_stub` derive the human label from `std::any::type_name::<A>()` so the literal vanishes.

- **[R-12] crates/tolaria/src/menus.rs:182-198** — The `assert_menu_schema` helper takes `expected_name: &str` and `expected: &[ItemKind]`, then pattern-matches `MenuItem`. The catch-all arm reaches for `MenuItem::Submenu(_) | MenuItem::SystemMenu(_) | _ => "other"` via string substitution. If gpui adds a new `MenuItem` variant, the `_ => "other"` branch hides the breakage. Replace with an exhaustive match (no wildcard) so a new variant is a compile error in the test, not a silent "other".

**MAY**

- **[STYLE] crates/actions/src/lib.rs:54-70** — Seven new actions defined in one `actions!` block — fine. The doc-comment block (lines 54-67) lists them with their planned phases, which is great breadcrumb but bound to rot. Move the per-action notes onto each action variant as `///` doc-comments if `gpui::actions!` supports it; if not, accept the central doc-block but add a `// TODO(worklist-2.7): drop notes when handlers land` so the rot has a sunset date.

---

### aeba298f — feat(status_bar): worklist 8.2.5 — vault picker closes on focus loss

**SHOULD**

- **[R-9] crates/status_bar/src/lib.rs:149-150** — `_window_activation: Option<Subscription>` — the leading underscore says "held only for Drop", but `Option` means tests can construct without it. That's the intent, but a stronger encoding is `#[derive(Debug)] struct WithSubscription<T> { value: T, _sub: Subscription }`. For a one-field case keep it as-is; just be aware that `Option<Subscription>` invites future code to `take()` the subscription and accidentally drop the observer.

- **[R-11] crates/status_bar/src/lib.rs:512-523** — `open_at_render` snapshot dance is well-commented and necessary, but reads like a workaround that should be encapsulated. Consider extracting `fn toggle_with_outside_click(prev: bool, entity: &Entity<Self>, cx: &mut App)` so the trigger pattern can be re-used elsewhere (sidebar dropdowns will need the same).

- **[R-2] crates/status_bar/src/lib.rs:441-466** — `vault_menu_popup(bar: Entity<StatusBar>, vaults: &[SharedString], bg, border, fg)` — five args, three of them theme colours. Bundle the colours into a local `MenuPopupTheme { bg, border, fg }` struct or pass the whole `Palette` if there's one nearby — five-arg fns at the boundary are an R-12/pedantic smell.

**MAY**

- **[STYLE] crates/status_bar/src/lib.rs:170-181** — `_window_activation: None` for the test/empty path mirrors the production path with `Some(Self::observe_window_blur(...))`. The asymmetric default is fine, but consider `Default::default()` for `empty()` plus an explicit `observe_blur` constructor step — composability over conditional construction.

---

### 09ea3895 — feat(sidebar_panel): worklist 8.2.6 — collapsible Views / Types / Folders sections

**SHOULD**

- **[R-9] crates/sidebar_panel/src/lib.rs:200-219** — `SectionCollapseState { views: bool, types: bool, folders: bool }` + a `get`/`toggle` shim that re-matches the section. The doc comment justifies "three booleans rather than a `HashSet`" on allocation grounds — correct — but the *cleaner* shape is `EnumMap<SidebarSection, bool>` (zero-alloc, exhaustive, indexed by enum). If you don't want a new dep, the next best thing is `[bool; 3]` keyed off `SidebarSection as usize`:
  ```rust
  struct SectionCollapseState([bool; 3]);
  impl SectionCollapseState {
      fn get(self, s: SidebarSection) -> bool { self.0[s as usize] }
      fn toggle(&mut self, s: SidebarSection) { self.0[s as usize] ^= true; }
  }
  ```
  Adding a new section is then a one-line array-size bump and a one-line enum variant, instead of touching three `match` arms.

- **[R-5] crates/sidebar_panel/src/lib.rs:1172-1182** — `toggle_section_handler` returns an `impl Fn(&mut App)` that captures `entity.clone()` — fine, but every section header builds its own clone-and-closure pair (lines 1105, 1132, 1160). One closure factory + three calls is fine; just verify the closures are `'static` (they are, via `entity.clone()`).

- **[R-12] crates/sidebar_panel/src/lib.rs:927-940** — `header_action` mounts an `on_mouse_down(Left, |_, _, cx| cx.stop_propagation())` guard. The comment explains why. Document the *invariant* (header rows that have actions MUST swallow mouse-down on the action) somewhere more discoverable than a code comment — e.g. on the `section_header` doc itself — or wrap the guard into a `header_action_inert` helper used uniformly so a future hand-rolled action doesn't forget the guard.

**MAY**

- **[STYLE] crates/sidebar_panel/src/lib.rs:1729-1751** — `toggle_section_renders_after_collapse` uses `cx.open_window(Default::default(), …)` instead of `cx.add_window(…)`. The two other section tests use `add_window`. Minor inconsistency; pick one for the file.

---

### 238b85f9 — fix(note_item): worklist 8.1.2 — drop InstrumentedWebView wrapper, use upstream Render

**MUST**

- **[BUG] crates/note_item/src/lib.rs:553-572** — Dropping `InstrumentedWebView` also drops the **0.5 logical-pixel epsilon-compare guard** that deduped no-op `set_bounds` calls on the WKWebView (ADR-0115 §4). The commit message and code comment claim the upstream `gpui_wry::WebView::Render` impl provides "track_focus + bounds-tracking canvas + the full WebViewElement" — confirm by reading `gpui_wry`'s `WebViewElement::prepaint` (in the vendored crate or its repo) that the upstream implementation has equivalent dedupe behaviour. If upstream *does not* epsilon-compare, this commit reintroduces the per-frame `set_bounds` IPC chatter the ADR warned about. The user-visible symptom (WKWebView blank-on-hover) may be fixed; the resource-hygiene symptom (constant frame-sync churn) may now be back. Either:
  - cite the upstream source in a code comment, OR
  - add an in-crate regression test that drives the render loop and asserts `set_bounds` is called once per *changed* frame, not once per *paint* frame.

**SHOULD**

- **[R-9] crates/note_item/src/lib.rs:184-186** — `MacosState::last_bounds: FrameSyncState` was deleted but `MacosState` still uses `#[derive(Default)]`. Confirm no path constructs `MacosState { webview: Some(…), last_bounds: …, ..Default::default() }` anywhere — it doesn't, but the struct's only remaining field is `webview: Option<Entity<WebView>>`, so consider inlining into `NoteItem` directly: `macos_webview: Option<Entity<WebView>>` instead of a one-field newtype-ish struct. Removes a layer of indirection.

- **[R-12] crates/note_item/src/lib.rs:1313-1342** — The renamed test `macos_render_hands_webview_entity_directly` is a *compile-time* assertion (`fn _assert_webview_entity_is_into_element<T: IntoElement>(_: T) {}`). The original test asserted a runtime contract (`PrepaintState = Option<Hitbox>`). The new test is strictly weaker — it asserts the entity is `IntoElement` (which is trivially true for any entity with a `Render` impl) but does NOT assert the upstream Render's prepaint actually inserts a hitbox. If a future `gpui_wry` upgrade removes the hitbox from `WebViewElement::prepaint`, this test sails through and worklist 1.2 regresses silently. Add a snapshot/inspector test that drives one paint pass and reads back the registered hitboxes, or accept the gap and document it.

**MAY**

- **[DOC] crates/note_item/src/lib.rs:556-570** — The long comment block citing two prior commit hashes (`8ece1e4d`, `ba05c788`) as failed attempts is exactly the kind of breadcrumb that ages well. Keep it. One tweak: link the ADR section that codifies the rule going forward (ADR-0115 §6) so a future reader doesn't have to git-blame.
