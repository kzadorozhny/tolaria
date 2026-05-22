# Phase 10 worklist — behavioral layers

> **Phase 10 scope.**  Extract Phase 8's ad-hoc closures + `cx.observe()`
> calls into 5 named, `mock_fixtures`-compatible GPUI crates so Phase
> 11 service expansion and Phase 12 modal chrome consume a stable
> behavioral layer.  One inherited blocker (`10.1.1`) lands first
> because it gates `10.4 dialog_stack`.
>
> **Scope adjusted before opening (2026-05-22):** `auto_git` +
> `telemetry_pipeline` moved to Phase 11 (rows 11.13 + 11.14) — they
> are wrappers around Phase 11 services and land adjacent to those
> services in Phase 11.  See [`README.md`](README.md) and
> [`../../roadmap.md`](../../roadmap.md) §Phase 10 for rationale.

## 1. Blockers

10.1.1. ⏳ WKWebView z-order fix

## 2. High Priority

10.2.1. `command_registry` — global command dispatch + shortcut table
10.2.2. `nav_history` — back / forward / neighborhood drill-down
10.2.3. `multi_select` — shared multi-row selection model
10.2.4. `dialog_stack` — modal queue, focus return, Escape handling
10.2.5. `vault_lifecycle` — open / switch / rename-detection state machine

## 3. Low Priority

10.3.1. ✅ address review feedback from `review.md` (first pass) — see annotation below for the five per-fix shas

---

### Annotations and details

#### 10.1.1

**Investigation pointers:**
- WKWebView z-order fix — GPUI overlays (popovers, dropdowns,
  dialog stack, slash-menu suggestion overlay) render behind
  the embedded WKWebView editor body.  Blocks 10.4
  `dialog_stack` from delivering a working modal surface and
  every Phase 12 modal chrome row (12.1 command palette, 12.2
  quick open, 12.3 dialogs, 12.4 wikilink inputs, 12.5 image
  lightbox, 12.6 emoji picker, 12.7 startup
- Phase 8 close-out architectural delta noted "Angle-C2 transparent
  base layer + WKWebView z-order reversal" — that fix addressed the
  background-layer composition but did not resolve cross-layer
  overlay ordering for GPUI popovers / modals over the editor body.
- Periscope README confirms the WKWebView is a sibling NSView
  composited above GPUI's Metal CALayer (`crates/periscope/README.md`).
- Candidate approaches (each needs its own probe before commit):
  1. Drop the WKWebView's layer z-order below the GPUI overlay
     surface on overlay-show / restore on overlay-hide.
  2. Render overlays in a separate `NSWindow` child window that
     floats above the editor.
  3. Move the editor body into a `CAMetalLayer` rendered by GPUI
     directly (loses native WKWebView text input / IME / spell-check
     handling — likely off the table).
- Acceptance: a `#[gpui::test]` or periscope smoke that opens a
  GPUI popover anchored over the editor body and verifies the
  popover paints above the editor pixels.

**2026-05-22 diagnostic pass — handed back unresolved.**

*What the architecture actually does (verified by source-reading
gpui_macos + lb-wry + gpui-component):*

- `gpui_macos::window` mints a custom `native_view` (VIEW_CLASS) +
  `setWantsLayer(YES)` + `makeBackingLayer` returning the renderer's
  `CAMetalLayer` (window.rs:783-878).  `native_view` is added as a
  subview of `NSWindow.contentView` and becomes layer-*hosting* (its
  layer IS the CAMetalLayer that GPUI renders into via Metal).
- `gpui_macos` exposes `native_view` (NOT contentView) through
  `raw_window_handle::AppKitWindowHandle::ns_view`.
- `lb-wry::WebViewBuilder::build_as_child(&handle)` → calls
  `ns_view.addSubview(&webview)` (lb-wry-0.53.3 wkwebview/mod.rs:632)
  where `ns_view` IS GPUI's `native_view`.  So the WKWebView lands as
  a *child* of GPUI's Metal-hosting view, not a sibling under
  contentView.
- Phase 8's `note_item::macos::fix_z_order_send_to_back` calls
  `parent.addSubview_positioned_relativeTo(wk_view, NSWindowBelow,
  None)` where `parent = wk_view.superview()` IS `native_view`.  The
  reorder happens within `native_view`'s subviews array — but
  WKWebView is the only subview, so the reorder is a no-op for
  visible compositing.  The original comment ("places this view below
  every other sibling, so GPUI Metal layer composites above") rests
  on a mental model where WebView and Metal are *siblings*, which the
  actual handle plumbing contradicts.
- GPUI's deferred-draw overlays (Popover, Tooltip, anchored modals)
  all render into the same Metal layer via `deferred()` + `with_priority()`.
  CAMetalLayer's contents (the Metal-rendered framebuffer) draws
  *before* sublayers — which means WKWebView's CALayer (a sublayer of
  the Metal layer) draws *on top* of whatever GPUI painted into the
  Metal-rendered region behind it.  Chrome OUTSIDE the WebView frame
  (toolbar, sidebar, status bar) reads as visible because the
  WebView's CALayer doesn't extend over those regions — not because
  Phase 8's z-order swap actually changed compositing.

*Why Phase 8's close-out claim of "tooltips z-order above WebView
natively" plausibly held in practice:* every tooltip the user
verified manually was on chrome OUTSIDE the editor-body frame
(sidebar buttons, toolbar buttons reachable above the editor).
Tooltips anchored over editor pixels were not exercised because the
demo vault flows used the toolbar above the editor or status bar
below it.

*Periscope tooling failed me here:* I could not reliably synthesise a
click on the toolbar `More` trigger, the status-bar vault chip, or
the title-bar inspector toggle — none of them mutated visible state
or fired their `on_mouse_down` / `on_click` log.  Sidebar clicks
*did* fire but on the row one slot *below* the dump-tree-named
target (clicking `--id sidebar-all-notes` selected `Archive`), which
points at a coordinate-mapping bug in `ui::tree_dump` /
`set_window_y_offset` against macOS Tahoe (Darwin 25.5) — separate
issue worth its own row, but it blocks any pixel-level z-order
verification through periscope today.

*Candidate approach re-ranked after the source read:*

1. **Re-parent WKWebView to be a sibling of `native_view` under
   `contentView`**, ordered NSWindowBelow.  Visual ordering becomes
   correct (Metal layer composites above WebView via standard sibling
   sublayer order).  Open problem: `native_view`'s default `hitTest:`
   returns self for every point inside its bounds, so editor-area
   clicks no longer reach the WebView.  Needs either a method-swizzle
   on Zed's `VIEW_CLASS` (fragile against Zed upstream churn) or an
   intermediate wrapper NSView between contentView and native_view
   that owns the hitTest fall-through to WebView for editor-body
   regions.  Net: low-to-medium LOC, but couples us to Zed-class
   internals.
2. **Re-introduce a child `NSPanel` (floating window) for every
   overlay surface** — popover, tooltip, dialog, slash-menu.  This
   reverses Phase 8's deletion of `OverlayTooltipExt` (632 LOC
   removed at `649a686c`) and would have to grow to cover popovers +
   dialogs as well.  Per-overlay cost is high; the routing for focus,
   Escape, click-outside dismiss, and key-equivalents needs to span
   the panel + main window.
3. **Move the editor into a `CAMetalLayer` rendered by GPUI** —
   loses IME, spell-check, native text editing, WebKit-backed
   BlockNote rendering.  Off the table per the original worklist
   filing.

*Open questions for the user before any fix lands:*

- (a) Manual confirmation of which overlays are currently broken.
  Candidates we expected to see fail: the note-toolbar `More`
  popover (anchored Top-Right inside the toolbar, expanding down
  into the editor body), the status-bar vault dropdown (anchored
  above the status bar, expanding up into the editor body), any
  Phase 12 dialog covering the editor.
- (b) Which approach — re-parent (1) vs child `NSPanel` (2) — fits
  the appetite for touching Zed-class internals.  (1) is cheaper if
  the wrapper-NSView hitTest pattern is acceptable; (2) is more
  isolated but rebuilds the surface we just removed.

*Status:* row left in ⏳; no code changes shipped this pass.  Pick
back up after (a) confirms which overlays we're covering and (b)
picks the approach.

#### 10.2.1

Consumed by `actions` and Phase 12.1 `command_palette`

#### 10.3.1

First pass — five MUST / SHOULD findings from
[`review.md`](review.md) landed as separate commits.  Each fix is a
real bug or release-readiness gate; the larger refactors flagged in
the cross-cutting themes (foreground-thread blocking I/O, bool-pair
→ enum migrations, log-level discipline, mutex-poison handling,
string-typed panel keys) stay deferred to their own rows because
they touch more code than a single review-pass should bundle.

**Fixes landed:**

1. `eaa05800` — `crates/tolaria/src/main.rs:420-430`,
   `TOLARIA_BUILD_TAG` printed `git:tolaria` because `concat!`
   silently substituted `env!("CARGO_PKG_NAME")` for the rejected
   `option_env!("GIT_HASH")`.  Split into
   `TOLARIA_BUILD_VERSION` + `TOLARIA_BUILD_GIT_HASH` consts and
   compose at the format-string level.
2. `45777a60` — `crates/workspace/src/workspace.rs:203`, magic
   index `3` in the right-dock observer's
   `ResizableState::resize_panel(3, …)` call was wrong for any
   workspace that never calls `attach_note_list_column` (panel
   vec is `[left, center, right]`, right dock at index 2).  Added
   private `right_dock_panel_index(&self)` helper, `debug_assert!`
   anchoring the invariant at the panel-push site, and a
   `#[gpui::test]` pinning both arms of the conditional.
3. `0ce2db16` — `crates/note_item/src/lib.rs:1044`,
   `WebViewBuilder::with_devtools(true)` shipped in release.
   Gated on `cfg!(debug_assertions)` so optimised builds drop the
   Safari Web Inspector hook.
4. `8bbe3501` — `crates/status_bar/src/lib.rs:697`, settings
   gear's `on_click` called `cx.dispatch_action(&action)` from
   inside the click frame — the active-window re-entrancy guard
   silently dropped it in production.  Routed through
   `window.dispatch_action(Box::new(action), cx)` and tightened
   the existing test by activating the window first (mirrors the
   `toolbar_window_dispatch_reaches_app_action_handler_under_nested_update`
   fixture so the regression is actually catchable).
5. `5e5974a8` — `crates/note_item/src/note_toolbar.rs`, dropped
   two tombstone blocks (the "Defeered" commented-out `stub_cell`
   call between active children and the 7-line "no stub_cell
   callers remain" preamble) and removed the dangling `[stub_cell]`
   rustdoc reference.

**Deferred (each warrants its own worklist row):**

- Foreground-thread blocking I/O in `7ced27dd`, `9a3839c9`,
  `c1f896b3`, `13bbc646` — push to `cx.background_spawn` with
  an event when the result is ready.
- `bool` pairs / triples for domain state — `MenuState`,
  organized-icon `(Option, Option, Option)`, editor
  `raw_mode` + `wide_mode`, `right_dock_ever_opened`.
- Log-level churn across `d209bfb0` / `148378eb` / `b1614df8` /
  `a71cc191` / `40fd9f44` / `d9766aa5` — land at the final level
  with a stable `target` so the gradient is env-filter-driven.
- `expect(POISON_MSG)` on the inspector-slot mutex hit from the
  menu-rebuild hot path (`2e666913` + `6796dc0a`) — needs
  poison-recovery or `parking_lot::Mutex`.
- String-typed panel keys / `SidebarSelection::Favorite(u64)`
  vs newtype `PanelKey` / `NoteId`.
- Lumped `// SAFETY:` invariants in `0206465d` / `a20b1295`.
