# Planning notes for ADR-0115

Persistent planning + progress + worklists for the native-GPUI chrome migration on branch `feat/native-gpui-chrome`.  This directory survives the merge: the per-phase worklists and close-out post-mortems are durable history, not ephemeral conversation chrome.

## Layout

| File | Purpose |
|------|---------|
| [`roadmap.md`](roadmap.md) | Single canonical phase order. |
| [`progress.md`](progress.md) | Running ledger — per-phase commit refs, test counts, key API decisions.  Mirrors `roadmap.md`'s numbering. |
| [`process.md`](process.md) | Continuous-process spine (per-iteration loop + worklist mechanics + user verification & reopen + close-out punctuation) plus crate naming, branch policy, visual fidelity rule, hard rules.  Canonical source of workflow truth. |
| [`components.md`](components.md) | Per-component visual + behavioural spec.  Reference screenshots, React-source mapping, per-crate visual contracts. |
| [`e2e-harness.md`](e2e-harness.md) | Periscope screenshot + click harness CLI + SIGUSR1 tree-dump IPC contract. |
| [`tree-dump-ids.md`](tree-dump-ids.md) | Element-ID naming convention used by periscope tree-dump. |
| [`tolaria-demo-vault-v2-light.png`](tolaria-demo-vault-v2-light.png) / [`tolaria-demo-vault-v2-dark.png`](tolaria-demo-vault-v2-dark.png) | Reference captures of the Tauri-era app rendering `demo-vault-v2/` in both modes.  Single visual source of truth. |
| [`tolaria-demo-vault-v2.png`](tolaria-demo-vault-v2.png) | Legacy single-mode capture kept for backward links; superseded by the light/dark pair above. |
| [`phases/`](phases/) | Per-phase folders — see below. |

## `phases/`

| Folder | Contains |
|--------|----------|
| [`_template/`](phases/_template/) | Empty scaffold (`worklist.md` + `README.md`).  Copy when opening a new phase: `cp -R phases/_template phases/phase-N`. |
| [`phase-6/`](phases/phase-6/) | MVP cutover (`mvp-scope.md` — frozen Phase 0–6 definition). |
| [`phase-7/`](phases/phase-7/) | Visual fidelity sweep — worklist + close-out + `zed-title-bar-analysis.md` + `react-to-gpui-measurements.md` + `snapshots/` (47 before/after PNGs). |
| [`phase-8/`](phases/phase-8/) | Behavioural fidelity sweep — worklist + close-out + `sweep.md` (periscope smoke recipe). |
| [`phase-9/`](phases/phase-9/) | Active worklist (deferred toolbar slots + new product features). |

Per-phase folders carry `worklist.md` (the working surface during the sweep) and, once the phase closes, `close-out.md` (the post-mortem lifted from the worklist's tail).  See `process.md § Closing a phase` for the close-out lifecycle.
