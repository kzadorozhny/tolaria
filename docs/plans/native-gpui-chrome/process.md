# Process & invariants

Workflow, naming, and verification rules that apply throughout the ADR-0115 migration.

## Top-level invariants

1. **Worklist row titles are written once and never edited.**  They exist to be scanned and recognised at a glance by the user.  The *only* permitted in-place edit to an existing row is the leading status emoji (✅, ⏳, ➡️, ❌).  All context — commit hashes, deferral targets, won't-fix reasons, root-cause notes, design alternatives — goes to `### Annotations and details § #### <phase>.<severity>.<number>`, never to the row line itself.  Those four status emojis live ONLY on worklist row lines — never in commit subjects, commit bodies, branch names, or PR titles (see `feedback_no_status_emoji_in_commits.md`).
2. **Row IDs are stable.**  Numbers are assigned once when a row is added and never renumber even if the row is reopened, deferred, or won't-fixed.
3. **Three-segment row IDs.**  Every worklist row reads as `<phase>.<severity>.<number>` (e.g. `9.2.7`).  The phase prefix is written on the row even though the row lives inside a phase-scoped file, so the same identifier reads correctly in commit messages, cross-phase references, and out-of-context citations.
4. **User-driven verification.**  Every ✅ is validated live by the user in the running app.  Subagents never auto-validate via periscope or any other oracle (`feedback_manual_validation.md`).
5. **Subagents write code; the orchestrator dispatches + verifies + ledgers** (`feedback_delegate_plan_tasks.md` + `feedback_orchestrator_post_dispatch_verify.md`).  Mechanical research work can run on Sonnet; substantive code changes stay on Opus default.  Subagents skip the `git commit` step ~80% of the time even with explicit guardrails — the orchestrator's post-dispatch verify step is load-bearing, not optional.

## The continuous loop

Per-iteration verification, worklist mechanics, user verification, and close-out are subsections of **one row-driven workflow**, not three parallel processes.

- Rows track *outcomes*.  Commits track *increments*.  Cardinality is 1:N — one row may span several commits.
- The ✅ goes on the row only after the last iteration in its cluster lands and the user has eyeballed the live result.
- A reopened row's ✅ comes off; the row's status reverts to ⏳; the next iteration starts.

```
user reports [high] foo           →  append row N.<sev>.<n>. foo   (no emoji yet)
       ▼
orchestrator picks up the row     →  row flips to ⏳
       ▼
orchestrator dispatches subagent
       ▼
subagent: read → edit → cargo fmt → cargo test → idiomatic-rust-review (auto-apply MUST/SHOULD)
       ▼
re-run fmt + test → commit
       ▼
(optional further iterations — same row stays ⏳)
       ▼
orchestrator flips row to ✅; records last commit sha into #### N.<sev>.<n> annotation
       ▼
USER VERIFIES LIVE  ← load-bearing inner step
       ├─ accepts implicitly        → row stays ✅
       └─ reports [N.<sev>.<n>] note → row flips back to ⏳; original commit sha
                                       already in annotation; dispatch next iteration
```

The "append row" and "pick up the row" steps collapse into one when a user reports an issue mid-sweep — the orchestrator appends the row and immediately starts working on it, so the row's emoji visibly steps "(none) → ⏳" in a single edit pass.  When a phase is being **scoped** ahead of time (e.g. the close-out of one phase populating the next phase's worklist with deferred rows, or any pre-loaded planning row), the appended row stays at *(no emoji)* until the orchestrator later commits to driving it.  Never write ⏳ on a row that hasn't been picked up.

## Per-iteration verification loop

After **every iteration** of Rust source changes (per crate or per logical sub-task within a row), in this order:

1. `cargo fmt -p <crate>` (or `--all` if multi-crate).
2. `cargo test -p <crate>` — confirm green.
3. **Spawn `code-reviewer` agent with the `idiomatic-rust-review` skill** against the changed files.  **Auto-apply every MUST and SHOULD finding** without prompting the user (MAY findings get surfaced separately for the user to decide).
4. Re-run `cargo fmt` + `cargo test` after applying review findings.
5. Commit.
6. **Orchestrator post-dispatch verification.**  After the subagent returns: `git status --short` + `git log --oneline -3` from the main worktree.  If the subagent worked in a worktree (`worktreeBranch` in result), also run `git log <branch> --oneline ^feat/native-gpui-chrome` to catch orphan follow-up commits, then `git merge --ff-only <branch>` (or `git cherry-pick` if diverged) and `git worktree remove -f -f <path>` to reclaim ~10GB of `target/` cache.  See `feedback_orchestrator_post_dispatch_verify.md` for the full playbook.  This step is mandatory — subagents skip the final `git commit` ~80% of the time and worktrees leak disk catastrophically if not removed (observed 86GB leak in one Phase 9 session).

One row may span multiple iterations.  The ✅ goes on the row only after the last iteration lands and the user has verified live.

## Worklist mechanics

Every active phase has exactly one `phases/phase-N/worklist.md`.  The file's lifecycle:

1. Empty scaffold is copied from `phases/_template/` when the phase opens.
2. Rows are appended row-by-row as the user reports issues.
3. Annotations accumulate under `### Annotations and details § #### N.<severity>.<number>`.
4. At phase close, a `## Close-out (YYYY-MM-DD)` section is appended (see "Closing a phase" below) and then **lifted** into a sibling `phases/phase-N/close-out.md`.  The worklist file ages cleanly without the close-out tail.

### Sections + numbering

Three sections (always present, in this order): `## 1. Blockers`, `## 2. High Priority`, `## 3. Low Priority`.  Each row is `<phase>.<severity>.<number>. <emoji> <title>`, where severity matches the section number and number is one-up within the section.

```markdown
## 1. Blockers
9.1.1. <issue description>
9.1.2. <issue description>

## 2. High Priority
9.2.1. <issue description>

## 3. Low Priority
9.3.1. <issue description>
---
```

When appending a new row to section 2 of Phase 9, the next available ID is `9.2.<N+1>` where `N` is the count of existing rows in that section.  IDs never re-pack: if `9.2.3` is deleted or won't-fixed, `9.2.4` does not slide down.

### Status markers (the only mutable surface on a row)

- *(no emoji)* — **pending**.  Row exists but work hasn't started.  Every new row enters in this state, whether it was appended inline during a sweep or pre-loaded during phase scoping.
- `⏳` — **in progress**.  Applied when the orchestrator commits to driving the row (dispatches the first subagent, opens the file, claims the task).  For inline user-reports during an active sweep this happens in the same step as appending the row; for phase scoping it happens later when the row is picked up.
- `✅` — **resolved**.  User has verified live in the running app.
- `➡️` — **deferred to next phase**.  Applied at close-out for rows that didn't ship in time, OR mid-phase when the user explicitly removes a row from the active scope (e.g. "push 9.2.5 to Phase 10").  Mid-phase deferral writes a `**Deferred (YYYY-MM-DD)** ➡️` note inside the row's `#### N.<sev>.<n>` annotation explaining what's blocking the deferred row and which phase will pick it up.
- `❌` — **won't fix**.  Reason goes to the row's `#### N.<sev>.<n>` annotation, never to the row line.

The leading status emoji is the **only** in-place edit ever applied to an existing row.  Everything else stays frozen.

A row may cycle through `(none) → ⏳ → ✅ → ⏳ → ✅ → …` as the user surfaces regressions live; the trail accumulates as `Closure → Reopened → Re-closure → Reopened-2 → …` paragraphs inside the row's annotation.  See `## Reopen mechanics` below.

### User-driven verification

After orchestrator marks ✅, the user verifies in the running app.  Subagents never auto-validate (`feedback_manual_validation.md`).

If the user reports a regression — phrased `[<phase>.<severity>.<number>] [optional note]`, e.g. `[9.2.1] still broken in dark mode` — the orchestrator:

1. Locates the row, removes its ✅, restores `⏳`.
2. Appends a `**Reopened (YYYY-MM-DD)** ⏳` paragraph to the row's `#### N.<sev>.<n>` annotation capturing the user's note + the original `Closure` commit sha as the trace anchor.  The earlier `Closure` paragraph stays intact.
3. Dispatches a subagent with the user's new note attached as additional context.

The original row title stays unchanged.  No new sentences, no parentheticals, no commit hashes appended to the line.

For repeat reopens (`Reopened-2`, `Re-closure-3`, …) see `## Reopen mechanics` below — the accumulating-paragraph pattern handles arbitrary cycles.

### Appending new rows

The user proposes new rows via `[<severity>] <description>`, e.g. `[high] menu is broken`.

1. Strip the bracketed severity (the section already encodes it).
2. Pick the matching section by severity.
3. Append at the next available `<phase>.<severity>.<number>` — **write the row with no leading emoji**.  The row is now pending.  The orchestrator adds ⏳ later, when it picks up the row to dispatch a subagent (often the same step as appending it for inline user-reports, but distinct when the row is added during phase scoping).
4. Keep the title short and scannable.  Explanations go to a new `#### N.<sev>.<n>` annotation under `### Annotations and details`.

If multiple subagents may touch the same area, instruct each to bail out fast and report back the names of files changing unexpectedly so the orchestrator can serialise.

### Annotations area structure

After the row sections, a horizontal rule `---` separates the close-out / observations area from the working surface.  Inside:

- Optional per-sweep observations first (environment notes, reproduction blockers, follow-up suggestions).
- Then `### Annotations and details` with `#### <phase>.<severity>.<number>` subheadings for long-form technical notes (root-cause analysis, follow-up trails, design alternatives, commit-sha trails for reopened rows).

Many rows have no annotation subheading.  That's fine — only rows with non-obvious history need one.

## Subagent dispatch invariants

Cross-references to the durable-memory entries that govern subagent dispatch.  These ride on every subagent prompt:

- `feedback_husky_hookspath.md` — unset `core.hooksPath` after any `pnpm install`; husky rebuilds shims that block non-`main` commits.
- `feedback_no_claude_coauthor.md` — never add a `Co-Authored-By: Claude …` trailer to commit messages.
- `feedback_rust_reviewer.md` — `idiomatic-rust-review` agent runs after every `.rs` edit cluster; auto-apply MUST and SHOULD findings.
- `feedback_rust_cargo_fmt.md` — `cargo fmt` after any series of `.rs` edits before commit/report.
- `feedback_manual_validation.md` — no periscope auto-validation during regression sweeps; the user is the validator.
- `feedback_delegate_plan_tasks.md` — orchestrator dispatches + verifies + ledgers; subagents read references and write code.
- `feedback_periscope_followup_test.md` — every periscope-driven UI finding gets a `#[gpui::test]` regression in the same commit.
- `feedback_orchestrator_post_dispatch_verify.md` — every dispatch ends with `git status --short` + `git log --oneline -3` + (if worktree) FF-merge + `git worktree remove -f -f`.  Subagents skip the commit step ~80% of the time; worktrees leak ~10GB of `target/` cache each.
- `feedback_gpui_dispatch_from_click_closure.md` — when an action is dispatched from inside an `on_click` closure, use `window.dispatch_action(Box::new(action), cx)` NOT `cx.dispatch_action(&action)`; the latter silently fails because we're already inside the window's update.
- `feedback_no_status_emoji_in_commits.md` — the worklist status emojis (`⏳ ✅ ➡️ ❌`) live on worklist row lines only; never in commit subjects, commit bodies, or branch names.  "stamp 9.2.7 ⏳" → "pick up 9.2.7" or "stamp 9.2.7 in progress" in commit subjects.

### Pre-dispatch hygiene check

Before starting a new code-writing dispatch, confirm a clean slate:

```sh
git worktree list                                                    # no leftover worktrees
git branch | grep -E '^  worktree-agent|^  phase-' || echo "clean"   # no zombie refs
```

If anything is left over from a prior session, clean it first via the worktree reconciliation playbook below.  Stacking new dispatches on top of leftover state is how multi-GB disk leaks + orphan-branch clutter accumulate.

### Dispatch prompt template

Every code-writing dispatch carries these sections in order:
1. **Branch + tip pointer.**  `feat/native-gpui-chrome` and the latest 2–3 commit shas the dispatch builds on.
2. **Row reference.**  Worklist file path + `#### N.M.N` annotation as the source of truth.
3. **What ships.**  Numbered list of concrete deliverables.
4. **Out of scope.**  Explicit list — defers + deferred reasons.
5. **Process invariants.**  Inline copy of the cross-refs above, especially the "YOU MUST `git commit`" guardrail.
6. **Worktree contract** (verbatim, when the dispatch lands in a worktree — see below).
7. **Critical files.**  Absolute paths with line numbers for the load-bearing references.
8. **Report back template.**  Final commit sha(s), files changed, test delta, MAY findings.

### Parallel dispatch is the default

Before every code-writing dispatch, scan the pending row queue for **disjoint-scope rows** that can run concurrently.  If 2-3 candidates exist, send them as **multiple `Agent` tool calls in a single response** so the harness spawns them in parallel.  Wall-clock time then equals the slowest row, not the sum.

Observed in Phase 9: ~15-18 subagent dispatches, all serialized 1-at-a-time.  Missed parallelism opportunities: at least 3-5 row pairs that touched disjoint files / stacks.  Going forward, the orchestrator's first thought at every "what's next" gate is "what 2-3 rows could run together?", not "which single row is next?".

**Disjoint-scope decision matrix:**

| Scope shape | Parallel? |
|---|---|
| Rust crate A + TypeScript editor-host work | ✅ always |
| Rust crate A + Rust crate B with zero shared files | ✅ always |
| Docs-only edits + code work | ✅ always |
| Two rows both touching `Cargo.lock` | ⚠️ serialize the lock-file write |
| Two rows both adding `editor_bridge` variants | ⚠️ serialize the enum addition |
| Two rows in same crate, different files | ⚠️ usually safe; verify no shared struct definitions |
| Row B depends on Row A's output (e.g. `vault::backlinks` from A, consumed by B) | ❌ must serialize |
| Two rows editing the same function | ❌ must serialize |

Cap: **3 concurrent dispatches** when the disjoint-scope check passes.  Each lands in its own worktree (per the worktree contract below), so cross-dispatch interference is impossible at the working-tree level.

### Worktree contract (paste verbatim into every code-writing dispatch)

```
You are running in a git worktree at <path-from-result>.  Your HEAD
is the worktree-agent-XXX branch (do not create new branches — no
`git checkout -b`).

Commit contract:
- ONE commit on this branch, covering everything the row asked for.
- If you genuinely need a follow-up commit (e.g. logging cleanup,
  doc tweaks after the main fix), you MUST enumerate every commit
  sha in your final report.  The orchestrator will FF-merge every
  commit on this branch into feat/native-gpui-chrome; any commit
  not in your report is an orphan and surfaces as a regression.

Final action contract:
- `cargo fmt --all` after every .rs cluster.
- `cargo clippy --workspace --all-targets -- -D warnings` green.
- `cargo test --workspace` green.
- `git status --short` returning ZERO modified/staged lines for tracked files.
- `git log <branch> --oneline ^feat/native-gpui-chrome` listing every
  commit you made — paste this list verbatim into your final report.
```

### Worktree reconciliation playbook

When the dispatch result includes `worktreeBranch`:

```sh
# 1. Orphan check — subagents sometimes commit twice and only report one sha.
git log <branch> --oneline ^feat/native-gpui-chrome

# 2a. If linear descendant: fast-forward in.
git merge --ff-only <branch>

# 2b. If diverged: cherry-pick the orphan(s) and fix conflicts.
git cherry-pick <orphan-sha>

# 3. Clean up the working tree + its target/ cache.
git worktree remove -f -f <path>

# 4. Delete the zombie branch ref — `git worktree remove` only deletes
#    the working-tree directory and the lock, not the branch.
git branch -D <branch>
```

When the dispatch result is main-tree (no `worktreeBranch`):

```sh
git status --short                                            # check for M / A / staged
cargo clippy --workspace --all-targets -- -D warnings        # verify compiles
git add <files> && git commit                                 # if subagent skipped
```

## GPUI dispatch invariants

GPUI actions dispatched from inside a click / hover / hit-test closure must use **`window.dispatch_action(Box::new(action), cx)`**, NOT `cx.dispatch_action(&action)`.

**Why:** the `cx`-flavoured call attempts to re-enter the same window's update via `cx.windows[id].take()`, which returns `None` because we're already inside that window's update.  The failure is swallowed by `.log_err()` — the click silently does nothing, the App-scope `cx.on_action` handler never fires, no log line appears.  `Window::dispatch_action` internally defers via `cx.defer`, queueing the dispatch for after the click update unwinds.

**Symptom signature:** the click does nothing in the running app, but `#[gpui::test]`s that invoke the dispatch path directly all pass.  Tests don't reproduce because they bypass the click-closure re-entrancy context.

**Observed in Phase 9:** five user reports (`9.2.3`, `9.2.4`, `9.2.6`, `9.2.13`, plus the title-bar sidebar toggle) all traced to this trap.  Diagnosis + fix shipped at commit `a71cc191`.

See `feedback_gpui_dispatch_from_click_closure.md` for the durable rule.

## Reopen mechanics

A row may flip ✅ → ⏳ → ✅ → ⏳ multiple times as the user re-tests live and surfaces regressions that previous fixes missed.  Each cycle accumulates into the row's `#### N.<sev>.<n>` annotation:

- **First reopen.**  Append a `**Reopened (YYYY-MM-DD)** ⏳` paragraph capturing the user's report verbatim, the suspected root cause, and the diagnosis path.  Note the original `Closure` commit sha for trace continuity.
- **Re-closure.**  After the next fix lands, append `**Re-closure (commit `<sha>`).**` describing the actual root cause and the seam.  Do NOT overwrite the earlier `Closure` paragraph.
- **Second reopen.**  Append `**Reopened-2 (YYYY-MM-DD)** ⏳`.  Earlier `Reopened` / `Re-closure` paragraphs stay intact.
- **Third re-closure.**  Append `**Re-closure-3 (commit `<sha>`).**`.  Etc.

The trail `Closure → Reopened → Re-closure → Reopened-2 → Re-closure-2 → …` IS the row's history.  The leading status emoji on the row line is the only in-place edit; everything else accumulates.

The row's title stays frozen — even across multiple reopens, even if the root cause turns out to be entirely different from what the title implies.

## Triage hypotheses (in order)

When a user report contradicts the source code or in-process tests pass green, walk these hypotheses in order before dispatching a speculative fix:

1. **Orphan worktree commits.**  The user's binary is built from `feat/native-gpui-chrome`, not the subagent's worktree branch.  If the subagent made follow-up commits the orchestrator never FF-merged, those changes never reach the user.  Diagnose: `git log <worktree-branch> --oneline ^feat/native-gpui-chrome` for every worktree.  See `feedback_orchestrator_post_dispatch_verify.md`.
2. **Stale `cargo` incremental cache.**  Suggest `cargo clean -p <crate> && cargo run`.  Workspace-level dep changes sometimes don't trigger a rebuild of binaries that look unaffected.
3. **Click-vs-dispatch gap.**  Tests calling the dispatch path directly miss bugs in `on_click → cx.dispatch_action → handler`.  Suspect re-entrancy; switch to `window.dispatch_action`.  See the §GPUI dispatch invariants section above.
4. **State-mutation vs render-trigger gap.**  The handler runs and mutates state, but no `cx.notify()` reaches the observer that drives the render.  Diagnose by adding `info!` logs at every layer of the state-change chain (action handler → entity update → observer → render-side accessor).
5. **Live-window context mismatch.**  The handler is registered at the wrong context (App vs Window) and never receives bubbled actions.  Compare to a known-working sibling handler (e.g. `ToggleSidebar` is the canonical reference for title-bar action handlers).

Diagnostic-first (instrument + ask the user for log output) beats speculative-fix when the bug doesn't reproduce in tests.  Ship the instrumentation as its own commit so the trace lands in the same place every future regression will use.

## Closing a phase

Close-out is **punctuation**, not a separate process.  When the user declares "close out phase N", the orchestrator appends a `## Close-out (YYYY-MM-DD)` section to `phases/phase-N/worklist.md` with five elements:

1. **Scoreboard** — table of section / count / status (`✅`, `➡️`, `❌`).
2. **Architectural deltas** — numbered list of beyond-MVP-scope changes this phase landed, each citing the closing worklist row by its 3-segment ID.
3. **Per-row commit ledger** — table of row → commit sha(s) → summary, newest-first.
4. **Deferred rows** — table of row → target-phase pointer → notes; each deferred row keeps its `➡️` marker so the next phase's plan can copy them across.
5. **Pending external gates** — any verifications that depend on host capability the assistant can't satisfy (e.g. periscope smoke requiring Screen Recording entitlement).

After the close-out section settles, it is lifted verbatim into a new `phases/phase-N/close-out.md`, leaving `worklist.md` clean of post-mortem chrome.  The worklist file then ages as a permanent record of what was done.

## Phase-boundary sweep

After a phase closes, before declaring it shipped externally:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p tolaria --release
```

Parallel sanity (Tauri stack stays alive throughout every chrome / service / harness phase):

```sh
pnpm tauri dev                                        # legacy app still runs
pnpm test                                             # editor-body Playwright suite
```

## Crate naming

No prefixes (Zed style).  `workspace`, `actions`, `vault` — not `tolaria_workspace`.  Deviates from ADR-0115 §1 but matches the reference codebase.

## Branch policy

ADR-0021 push-to-`main`.  All intermediates land on `feat/native-gpui-chrome` and are dogfood-only.  The Tauri stack under `src-tauri/` stays untouched throughout every chrome / service / harness phase.

## Visual fidelity rule

Every chrome surface, in every phase, targets **minimum visible delta** against [`tolaria-demo-vault-v2-light.png`](tolaria-demo-vault-v2-light.png) and [`tolaria-demo-vault-v2-dark.png`](tolaria-demo-vault-v2-dark.png), in both themes.  When implementation shortcuts the visual (placeholder styling, missing icons, wrong weights), it carries a `TODO(visual-parity)` comment so a periscope diff pass can find it later.

[`components.md`](components.md) holds the per-component visual contract; [`e2e-harness.md`](e2e-harness.md) is the verification loop.

## Hard rules

- Each chrome crate is **self-contained**; no cross-panel deps.  Plumbing through `TolariaWorkspace` events lands in later phases.
- Every crate's tests use the `install_theme(cx)` helper pattern from `crates/embed_poc/src/layout.rs:243`.
- Mock services are accessed via the `Global` accessor pattern; never hold mock data inline in panel state.
- `cargo fmt` + per-crate test green + workspace clippy `-D warnings` before any commit.
- `cargo build --workspace` clean per crate landing (no cascade breakage on `tolaria`, `embed_poc`, or sibling chrome crates).
- Mock methods return `Task<T>` (via `Task::ready` for instant) so the shape stays forward-compatible with the real service swap.
