# Phase 8 — Close-out (2026-05-21)

**Resolution scoreboard.**

| Bucket | Count | Status |
|--------|------:|--------|
| 1. Blockers | 2 / 2 | ✅ |
| 2. High Priority — bugs | 25 / 25 | ✅ |
| 2. High Priority — new product features | 0 / 7 | ➡️ Phase 9 (8.2.9, 8.2.10, 8.2.11, 8.2.12, 8.2.13, 8.2.14, 8.2.17) |
| 3. Low Priority — chrome polish | 2 / 2 | ✅ |
| Total in-scope rows | **29 / 29** | ✅ |

**Branch tip:** `feat/native-gpui-chrome` is ready to merge into `main` (subject to user-driven manual eyeball pass — durable memory `feedback_manual_validation.md`).  The branch carries the following architectural deltas beyond ADR-0115 Phase 4/5/6/7 MVP scope:

1. **Angle-C2 transparent base layer** (worklist 8.2.31) — `WindowBackgroundAppearance::Transparent` on the workspace, WKWebView NSView z-ordered behind the GPUI Metal layer via `addSubview:positioned:NSWindowBelow`, chrome surfaces audited to paint their own opaque `bg(theme.background)`.  Eliminates the entire `OverlayTooltipExt` NSPanel workaround (632 LOC removed at `649a686c`) — `gpui_component::Tooltip` now z-orders above the WebView natively.
2. **GPUI element-picker inspector** (worklist 8.3.1) — `Cmd+Alt+I` toggles the in-window dev-tool overlay.  `cx.set_inspector_renderer(...)` is now installed so the toggle is *visible* (a 100-LOC top-right dev panel showing the active element id + picking state).  Without this install GPUI's `Inspector::render` would return `Empty` — `5cd51756` shipped the toggle without ever wiring the renderer, so the inspector was invisible from day one.
3. **Byte-identical YAML frontmatter round-trip** (worklist 8.2.26) — `editor-host` captures the body's leading + trailing whitespace into per-note handler-ref slots, sandwiches the BlockNote-serialised body between them on every save (both `save_request` and auto-save), strips the WebView origin from absolutised link/image URLs, and routes the body through a verbatim React port of `compactMarkdown`.  31/31 demo-vault files round-trip byte-for-byte.
4. **Dynamic native menu labels** (worklist 8.3.2) — `MenuState { sidebar_open, inspector_picking }` drives "Show Sidebar" ↔ "Hide Sidebar" and "Show Inspector" ↔ "Hide Inspector".  Menu rebuilds happen inside `dispatch_to_workspace`'s deferred closure so post-toggle state is observed.

**Deferred to Phase 9.** All seven remaining open rows are net-new product features rather than chrome regressions:

| Row | Toolbar slot | Notes |
|-----|--------------|-------|
| 8.2.9 | `note-toolbar-star` | Wire to user-vault "starred notes" persistence (no current store). |
| 8.2.10 | `note-toolbar-organized` | Auto-organise into folder by type/date — needs vault-policy primitives. |
| 8.2.11 | `note-toolbar-neighborhood` | Backlinks/forward-links graph view — non-trivial UI surface. |
| 8.2.12 | `note-toolbar-raw` | Toggle raw-Markdown view inside the WKWebView — coordinates with `editor-host`. |
| 8.2.13 | `note-toolbar-ai` | AI assistant entry point — requires LLM provider plumbing. |
| 8.2.14 | `note-toolbar-toc` | Outline panel — render-only; reuses `editor-host` outline emission. |
| 8.2.17 | `note-toolbar-more` | Overflow menu hosting items 8.2.9-8.2.14 plus future actions. |

Each row in section 2 of [`worklist.md`](worklist.md) keeps its `➡️` marker so the Phase 9 plan can copy them across without re-discovery.

**Periscope smoke sweep.** Still `⏳ pending` — requires Screen Recording + Accessibility permissions on the parent terminal and a windowed Tolaria binary, which the assistant sandbox cannot satisfy.  Run on host before declaring Phase 8 closed externally; the in-branch test suite (`cargo test --workspace`: 402 passed, `editor-host` byte-roundtrip: 31/31) and the user's live manual sweep are the proxies that closed the worklist rows here.

**This-session sweep commits** (newest first):

- `2c680c9a` feat(tolaria): minimal GPUI inspector renderer — fixes invisible toggle (8.3.1 follow-up)
- `2f3c7101` docs(phase-8): close worklist 8.3.1 — GPUI inspector overlay restored to original 5cd51756 semantic
- `62d64673` fix(tolaria,actions): restore GPUI inspector overlay as ToggleInspector — revert 6c14e83c misinterpretation (8.3.1)
- `a0971295` docs(phase-8): close worklist row 8.2.31 — Angle-C2 all three phases landed
- `649a686c` refactor(chrome): replace OverlayTooltipExt fan-out with inline gpui_component::Tooltip — Angle-C2 Phase 3 (8.2.31)
- `e9214f51` fix(inspector_panel): paint root with theme.background + theme.foreground (8.3.1 — superseded by 62d64673; theme tokens still apply if InspectorPanel is reused)
- `b4dd5efe` fix(note_item,ui): WKWebView send-to-back z-order + UI objc2 deps — Angle-C2 Phase 2 (8.2.31)
- `f0a38d84` feat(tolaria,chrome): transparent workspace base layer — Phase 1 of Angle C2 (8.2.31)
- `b227ea53` fix(editor-host): byte-identical round-trip + remove PropertiesPanel (8.2.26 + 8.2.27)
- `211717f9` perf+fix(ui): cache OverlayTooltip NSPanel + prefer-Above placement (8.2.28 + 8.2.30)
- `cfdde37e` test(editor-host): round-trip the user-named demo notes (8.2.27 follow-up)
- `6b19ddf5` fix(editor-host): auto-save was stripping YAML frontmatter from disk (8.2.27 regression)
- `caf6641e` feat(tolaria,workspace): dynamic Show/Hide menu labels for Sidebar & Inspector (8.3.2)
- `6c14e83c` feat(tolaria,actions): open InspectorPanel as a separate macOS window (8.3.1 — reverted by 62d64673)
