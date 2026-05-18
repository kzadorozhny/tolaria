# Ephemeral planning notes for ADR-0115

These files exist for the duration of the native-GPUI chrome migration
(branch `feat/native-gpui-chrome`).  They mirror the conversation-level
plans, todo lists, and phase-by-phase progress that informed each
commit, so the work is reviewable and resumable across sessions.

**Delete this entire directory before merging `feat/native-gpui-chrome`
into `main`.**  Permanent context belongs in:

- `docs/adr/0115-native-gpui-chrome.md` — the ADR itself
- `docs/ARCHITECTURE.md` / `docs/ABSTRACTIONS.md` — long-lived shape docs

## Layout

| File | Purpose |
|------|---------|
| `00-overview.md` | The full multi-phase plan (Phases 1–7) generated at the start of the migration.  Section B / C contain deep specs for Phases 1 and 2 specifically. |
| `progress.md` | Running ledger: what shipped per phase, with commit refs + test counts + key API decisions. |
| `phase-2d-next.md` | Outline for the next phase (large chrome panels). |

Add more files here for any future phase that needs a dedicated plan
(`phase-3-services.md`, `phase-4-editor-host.md`, etc.).

## Why not Todoist / Linear / etc

This branch is dogfood-only per ADR-0021 (push-to-`main` workflow).
The actual project tracker carries production tasks; this directory
just persists the in-flight planning state so a fresh session can
resume mid-migration without re-deriving the topology.
