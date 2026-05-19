# Live roadmap — MVP-first

> **Authoritative.**  Supersedes the §A roadmap table in
> [`00-overview.md`](00-overview.md) (kept for historical reference
> only).
>
> See [`mvp-scope.md`](mvp-scope.md) for the MVP cut definition.

## Visual guide

[`tolaria-demo-vault-v2-light.png`](tolaria-demo-vault-v2-light.png)
and [`tolaria-demo-vault-v2-dark.png`](tolaria-demo-vault-v2-dark.png)
are the authoritative visual targets for every chrome component the
native GPUI shell ships.  The dark variant is reached in the reference
app via the moon-icon theme switcher at the right end of the status
bar.  Every implementation, in any phase, must strive for the
**minimum visible delta** against these screenshots — same spacings,
weights, colours, glyph treatment, row geometry — in **both** light
and dark modes.

Per-panel match-to-image notes live in
[`phase-2d-next.md` § Visual guide](phase-2d-next.md#visual-guide-authoritative).
New chrome surfaces (Phase 6 modals, Phase 8/9 service-wired panels)
inherit the same constraint: when in doubt, sample pixels off the
reference images instead of improvising.

The periscope harness ([`e2e-harness.md`](e2e-harness.md)) is the
verification loop: capture the live app in both themes, diff against
these images, refine.  To regenerate the references, launch the
shipped Tauri build (`/Applications/Tolaria.app/Contents/MacOS/tolaria`)
on `demo-vault-v2/`, capture with `periscope screenshot --pid <pid>`,
click the moon-icon at the bottom-right of the status bar to flip
themes, and capture again.

### React source = behavioural reference

The screenshots lock the look; the existing React + TypeScript
implementations under `src/components/` (the Tauri-era frontend) lock
the behaviour.  When porting a chrome surface to Rust, **read the
React counterpart first** and follow it as the spec — hover / active
states, count derivation, keyboard handling, multi-select model,
sort/filter rules, copy text, and edge cases all live there (and in
the colocated `*.test.{ts,tsx}` files).  The per-crate React ↔ Rust
mapping is in
[`phase-2d-next.md` § React source = behavioural reference](phase-2d-next.md#react-source--behavioural-reference).

## Phase order

| # | Name | Cut | Notes |
|---|------|-----|-------|
| 0 | `embed_poc` spike | ✅ shipped | WKWebView-in-GPUI viability proof |
| 1 | Foundation crates | ✅ shipped | `paths`/`theme`/`actions`/`ui`/`settings_store`/`workspace`/`tolaria` |
| 2a | Workspace topology + mocks + Picker | ✅ shipped | Dock/Pane/PaneGroup/Panel/Item/MockNoteItem; `mock_fixtures`; Picker port |
| 2b | First chrome surfaces | ✅ shipped | `status_bar`/`breadcrumb_bar`/`toasts`/`banners` |
| 2c | Chrome wiring + TOLARIA_MOCK | ✅ shipped | 3-dock layout populated; typed toasts; mock-globals bootstrap |
| 2d | Big panels | ✅ shipped | `sidebar_panel`/`inspector_panel`/`ai_panel`/`search_panel`/`settings_panel`/`diff_view`/`note_list_pane` |
| **3-MVP** | Vault service (minimal) | ✅ shipped | `vault` crate: open dir, list, read, save, basic notify.  Shape-compatible swap with `mock_fixtures::MockVault`. |
| **4-MVP** | Editor host integration | ✅ shipped | `editor_host/` Vite project, `editor_bridge` crate, `note_item` crate (per-note `WKWebView` via `gpui-wry`).  Per ADR-0115 §4 + §5. |
| **5-MVP** | MVP wiring + launch | ✅ shipped | `tolaria --vault <path>` CLI arg; swap `sidebar_panel` / `note_list_pane` from MockVault to real `vault::Vault` global; open-note → spawn `note_item` in center Pane. |
| **6-MVP** | Rust e2e screenshot harness | ✅ shipped | `periscope` crate; xcap + AX bindings; CLI; smoke test.  Closes the diff-against-screenshot loop. |
| **✅ MVP cut** | | shipped | App opens local vault, navigates, renders + saves notes.  Tauri stack still alongside. |
| **6** | **Remaining chrome surfaces + visual fidelity pass** | ⏳ active | Two streams:  **(a) visible chrome parity** — sidebar/note-list/status-bar/window-chrome polish until live capture matches `tolaria-demo-vault-v2.png` row-by-row;  **(b) missing modal surfaces** — `command_palette`/`quick_open`/`dialogs`/`wikilink_inputs`/`image_lightbox`/`emoji_picker`/`startup`.  Stream (a) is the higher visible delta and runs first. |
| 8 | Service expansion | ⏳ planned | Remaining services: `git_provider`, full `vault_search`, `vault_watcher` (advanced), `cli_agents`, `mcp_bridge`, `telemetry`, `app_updater`, `localization`.  Wire AI/search/settings_panel chrome to real services. |
| 9 | Parity hardening | ⏳ planned | Multi-tab `Pane` UX; autogit + conflict resolver; onboarding flow; measurement gate (memory, startup time). |

## Why MVP-first

Original plan ordered work as **chrome → services → editor host** to
maximize visible UI progress.  After Phase 2d we have a populated
chrome shell (3 docks + 7 panels + status bar + breadcrumb + toasts +
banners) running against `mock_fixtures` Globals.  That's a strong
visual deliverable but doesn't let the user *do* anything yet.

The MVP cut reorders the remaining work so the next three phases
(3-MVP / 4-MVP / 5-MVP) land an actually-usable app — open a vault,
navigate, render and save a note — before we sink time into the
remaining chrome modals, service expansion, parity work, or
cross-platform.

This lets us:

- **Dogfood sooner.**  Phase 6+ work happens with the maintainer
  using the new app for actual notes.
- **De-risk the editor-host bridge earlier.**  Phase 4 is the
  highest-risk integration point (ADR-0115 §6 re-eval triggers were
  validated in Phase 0, but the production bridge is bigger).
  Doing it before the long tail of chrome means bridge bugs surface
  on a tighter feedback loop.
- **Make the cut-over decision visible.**  Once MVP ships, we know
  how much of the legacy app still has parity gaps, which makes
  Phase 10 cut-over scoping concrete instead of speculative.

## Where MockVault still lives after MVP

Even after Phase 3-MVP / 5-MVP swap the chrome panels to use the real
`vault::Vault` global, `mock_fixtures::MockVault` stays around for:

- Test harnesses (every panel crate's `from_or_empty` + tests).
- The `TOLARIA_MOCK=1` launch path (handy for chrome work without a
  real vault on disk).

Removal of `mock_fixtures` is **not** on the roadmap; it's a permanent
test/dev utility.
