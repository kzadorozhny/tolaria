# Phase 7 — Close-out

**Resolution scoreboard.**

| Bucket | Count | Status |
|--------|------:|--------|
| 1. Blockers | 0 / 0 | ✅ (none filed) |
| 2. High Priority — visual fidelity | 20 / 20 | ✅ |
| 3. Low Priority | 0 / 0 | ✅ (none filed) |
| Total in-scope rows | **20 / 20** | ✅ |

**Branch tip.**  Live chrome matches `tolaria-demo-vault-v2-{light,dark}.png` row-by-row in both themes; baseline `897091bf`, final close at `3c70b6b9`.

**Architectural deltas beyond ADR-0115 Phase 0–6.**

1. **CSS-derived theme tokens.**  `crates/theme` palette mirrors the React app's CSS custom properties (`--state-selected`, `--accent-blue`, `--neutral-*`) — sidebar `theme.list_active` + `theme.primary` pair replicates the pale-blue-bg + accent-blue-text selection treatment from worklist 7.2.001.
2. **Filename-prefix-derived TYPES + colour-coded leading dot.**  `SidebarPanel::build_from_samples` reads each `demo-vault-v2/type/*.md` frontmatter (`icon`, `color`, `sidebar label`) and renders a leading colour swatch + per-type icon — closes worklist 7.2.003.
3. **`MMM D · Created MMM D` note-list metadata line.**  `NoteListPane` row renderer produces the React-side compact-date format with `selected_id` + `theme.list_active` pale-accent row — closes worklist 7.2.010 / 7.2.011 / 7.2.012.
4. **Custom title-bar strip + traffic-light alignment.**  `workspace::title_bar::TitleBar` with `TRAFFIC_LIGHTS_PADDING_PT` and `TitlebarOptions::appears_transparent`; Zed-matching dims — closes worklist 7.2.005 / 7.2.007 / 7.2.008 / 7.2.009 / 7.2.016 / 7.2.020.
5. **WKWebView resize-artifact fix + four Tauri-mirrored seamless-resize fixes ported to production.**  Removed `.bg(theme.background)` from `pane_group.rs:75` + `pane.rs:128` active branches; ported `autoresizingMask`, `drawsBackground=false`, matched `NSWindow` background, and `setUnderPageBackgroundColor` from `embed_poc` to `note_item`.  Closes worklist 7.2.018.  Two design docs landed alongside: `docs/plans/wkwebview-seamless-resize.md` (Tauri research) and `docs/plans/wkwebview-seamless-resize-followup.md` (post-`207da697` post-mortem).
6. **Consistent `.dump_as(...)` element-ID hierarchy.**  Every chrome container from `workspace` root through `sidebar_panel`, `note_list_pane`, and `status_bar` carries a stable element ID; periscope tree-dump can address any node by kebab-case path.  See `tree-dump-ids.md`.  Closes worklist 7.2.018 + spans the remainder of the QA wave.

**Per-issue commit ledger** (arrival order; from the legacy `progress.md` § Phase 7 follow-up table):

| Issues | Commit | Crate(s) | Summary |
|--------|--------|----------|---------|
| 7.2.001 + 7.2.002 | `6b92a6ba` | `sidebar_panel` | Selection palette + folder indent |
| 7.2.003 + 7.2.004 | `218fab16` | `sidebar_panel` | Type frontmatter + hover bg |
| 7.2.005 | `f7555520` | `workspace` | Title-bar height for symmetric padding |
| 7.2.006 | `0b3be620` | `sidebar_panel` | VIEWS / TYPES collapse carets |
| 7.2.007 | `4f6c6e07` | `tolaria` | Vertically centre traffic lights |
| 7.2.008 | `9cb25da7` | `workspace` | Align title cluster with traffic lights |
| 7.2.009 | `238121da` | `workspace` | Centre title-bar action cluster |
| 7.2.010 + 7.2.011 + 7.2.012 | `b8b8282a` | `note_list_pane` | Per-type accents, tighter row, native word-wrap |
| 7.2.013 | `29d8e5f4` | `note_list_pane` | Symmetric row padding |
| 7.2.014 + 7.2.015 | `dad72e19` | `theme`, `note_list_pane` | Transparent scrollbar track + sidebar-style hover |
| 7.2.016 | `c1c1aaba` | `workspace` | Zed-matching native title bar dims |
| 7.2.017 | `b9fd4e91` | `status_bar` | Icons + left-aligned services + separators |
| 7.2.018 | `207da697` + `5b3e475d` | `embed_poc`, `workspace`, `note_item` | WKWebView resize artifact — remove obscuring opaque paint; port four Tauri-mirrored fixes to production |
| 7.2.019 | `951d5ea2` (+ `54748e81`, `382b6577`) | `note_item`, `workspace` | Top per-note toolbar row mirroring React's `BreadcrumbBar`; removed redundant note-list right border; sync glyph switched to `IconName::Redo` |
| 7.2.020 | `09ecd907` (+ `94e94a32`, `eff7521d`, `66301216`, `c056bfef`, `bbf31abf`, `3c70b6b9`) | `workspace`, `theme` | Sidebar show/hide button; column collapses on toggle; sized siblings keep widths via `.flex_none()` + `.visible(false)` stable slots; resize-handle colour matches sidebar right border in every state |

**Pending external gates.**  `embed_poc` removal scheduled but deferred — no longer load-bearing now that the resize-artifact fixes have been ported to the production `note_item` path.  Schedule under a future Phase tidy commit, not Phase 8 close-out.
