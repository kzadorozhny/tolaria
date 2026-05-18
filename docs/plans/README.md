# `docs/plans/`

Ephemeral planning workspaces for in-flight initiatives.

Each subdirectory is the planning workspace for one initiative — typically
a feature branch or migration that needs its own multi-phase plan,
progress ledger, and resumable-across-sessions context.

## Convention

- **One subdirectory per initiative.**  Name it kebab-case after the
  thing you're building (`native-gpui-chrome/`, `mobile-sync/`, …).
- **Required contents** (suggested, not enforced):
  - `README.md` — what this initiative is, when to delete the directory
  - One or more plan / progress / per-phase files (`progress.md`,
    `00-overview.md`, `phase-N-thing.md`, etc.)
- **Lifecycle**: create the subdirectory when you start the work,
  **delete it** when the branch merges to `main` or the initiative
  is abandoned.  These files are not durable architecture
  documentation — they're scratchpad-grade plans that exist to make
  multi-session work resumable.

## What belongs here vs. elsewhere

- **Here** — multi-phase plans, todo lists, per-commit progress
  ledgers, "what shipped last and what's next" notes.  Useful while
  the work is in flight; noise after it ships.
- **`docs/adr/`** — durable decisions that outlive the initiative.
  Create an ADR for any new dependency, storage strategy, platform
  target, or cross-cutting pattern.  See `/create-adr`.
- **`docs/ARCHITECTURE.md`** / **`docs/ABSTRACTIONS.md`** — long-lived
  shape docs.  Update in the same commit that lands the structural
  change.

## Current initiatives

| Directory | Initiative | Branch | Status |
|-----------|------------|--------|--------|
| [`native-gpui-chrome/`](native-gpui-chrome/) | ADR-0115: replace Tauri with native GPUI chrome + embedded WKWebView editor | `feat/native-gpui-chrome` | In flight (Phase 2c shipped, 2d planned) |
