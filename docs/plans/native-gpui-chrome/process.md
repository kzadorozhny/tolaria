# Process & invariants

Workflow, naming, and verification rules that apply throughout the ADR-0115 migration.

## Top-level invariants

1. **Worklist row titles are written once and never edited.**  They exist to be scanned and recognised at a glance by the user.  The *only* permitted in-place edit to an existing row is the leading status emoji (✅, ⏳, ➡️, ❌).  All context — commit hashes, deferral targets, won't-fix reasons, root-cause notes, design alternatives — goes to `### Annotations and details § #### <phase>.<severity>.<number>`, never to the row line itself.
2. **Row IDs are stable.**  Numbers are assigned once when a row is added and never renumber even if the row is reopened, deferred, or won't-fixed.
3. **Three-segment row IDs.**  Every worklist row reads as `<phase>.<severity>.<number>` (e.g. `9.2.7`).  The phase prefix is written on the row even though the row lives inside a phase-scoped file, so the same identifier reads correctly in commit messages, cross-phase references, and out-of-context citations.
4. **User-driven verification.**  Every ✅ is validated live by the user in the running app.  Subagents never auto-validate via periscope or any other oracle (`feedback_manual_validation.md`).
5. **Subagents write code; the orchestrator dispatches + verifies + ledgers** (`feedback_delegate_plan_tasks.md`).  Mechanical sweep / capture work runs on Sonnet; substantive code changes stay on Opus default (`feedback_periscope_sweep_sonnet.md`).

## The continuous loop

Per-iteration verification, worklist mechanics, user verification, and close-out are subsections of **one row-driven workflow**, not three parallel processes.

- Rows track *outcomes*.  Commits track *increments*.  Cardinality is 1:N — one row may span several commits.
- The ✅ goes on the row only after the last iteration in its cluster lands and the user has eyeballed the live result.
- A reopened row's ✅ comes off; the row's status reverts to ⏳; the next iteration starts.

```
user reports [high] foo           →  append row N.<sev>.<n>. ⏳ foo
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

## Per-iteration verification loop

After **every iteration** of Rust source changes (per crate or per logical sub-task within a row), in this order:

1. `cargo fmt -p <crate>` (or `--all` if multi-crate).
2. `cargo test -p <crate>` — confirm green.
3. **Spawn `code-reviewer` agent with the `idiomatic-rust-review` skill** against the changed files.  **Auto-apply every MUST and SHOULD finding** without prompting the user (MAY findings get surfaced separately for the user to decide).
4. Re-run `cargo fmt` + `cargo test` after applying review findings.
5. Commit.

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

- `⏳` in progress
- `✅` resolved (user has verified live)
- `➡️` deferred to next phase — only applied at close-out
- `❌` won't fix — reason goes to the row's `#### N.<sev>.<n>` annotation, never to the row line

The leading status emoji is the **only** in-place edit ever applied to an existing row.  Everything else stays frozen.

### User-driven verification

After orchestrator marks ✅, the user verifies in the running app.  Subagents never auto-validate (`feedback_manual_validation.md`).

If the user reports a regression — phrased `[<phase>.<severity>.<number>] [optional note]`, e.g. `[9.2.1] still broken in dark mode` — the orchestrator:

1. Locates the row, removes its ✅, restores `⏳`.
2. Records the *original* commit sha in the row's `#### N.<sev>.<n>` annotation (so the next iteration can reference what was tried).
3. Dispatches a subagent with the user's new note attached as additional context.

The original row title stays unchanged.  No new sentences, no parentheticals, no commit hashes appended to the line.

### Appending new rows

The user proposes new rows via `[<severity>] <description>`, e.g. `[high] menu is broken`.

1. Strip the bracketed severity (the section already encodes it).
2. Pick the matching section by severity.
3. Append at the next available `<phase>.<severity>.<number>`.
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
- `feedback_periscope_sweep_sonnet.md` — periscope sweep / capture runs use `model: "sonnet"`; substantive code changes stay on Opus default.

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
