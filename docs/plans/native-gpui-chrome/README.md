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
| `roadmap.md` | **Live phase order** — MVP-first.  Authoritative; supersedes §A of `00-overview.md`. |
| `mvp-scope.md` | What "MVP" means: open a local vault, navigate notes, render + save in the editor.  Lists what's in and explicitly defers everything else. |
| `progress.md` | Running ledger: what shipped per phase, with commit refs + test counts + key API decisions. |
| `00-overview.md` | Original full multi-phase plan (Phases 1–7) generated at the start of the migration.  **Frozen for reference** — `roadmap.md` is the live order now.  Section B / C still have the deep specs for Phases 1 and 2 verbatim. |
| `phase-2d-next.md` | Outline for Phase 2d (large chrome panels) + **authoritative per-component visual guide** anchored on `tolaria-demo-vault-v2.png`.  Skeleton shipped at `6d96cca8`; visual-parity pass against the screenshot is ongoing. |
| `tolaria-demo-vault-v2-light.png` / `tolaria-demo-vault-v2-dark.png` | Reference captures of the Tauri-era app rendering `demo-vault-v2/` in both light and dark mode (toggled via the moon-icon theme switcher in the bottom-right of the status bar).  The single visual source of truth for every chrome component — implementations strive for minimum visible delta against these images in **both** themes. |
| `tolaria-demo-vault-v2.png` | Legacy single-mode capture kept for backward links; superseded by the light/dark pair above. |
| `eval-gpui-component-removal.md` | Evaluation pass scheduled as **Phase 7** in the new MVP-first roadmap (post-MVP, before service expansion).  Decision matrix: keep / pin / vendor / replace. |

Add more files here for any future phase that needs a dedicated plan
(`phase-3-services.md`, `phase-4-editor-host.md`, etc.) or evaluation
work item (`eval-*.md`).

## Why not Todoist / Linear / etc

This branch is dogfood-only per ADR-0021 (push-to-`main` workflow).
The actual project tracker carries production tasks; this directory
just persists the in-flight planning state so a fresh session can
resume mid-migration without re-deriving the topology.
