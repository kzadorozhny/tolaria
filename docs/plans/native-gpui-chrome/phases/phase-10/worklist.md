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
10.1.2. ✅ ToggleElementInspector update failed: window not found
10.1.3. ✅ ToggleElementInspector window shoud be a separate os window.

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

**2026-05-22 re-parent attempt landed at `3cdd2f66` then reverted.**

Implemented candidate (1) in `note_item::macos::fix_overlay_compositing`:
walked `wk_view.superview().superview()` to reach `contentView`, then
`addSubview:positioned:Above relativeTo:nil` + `wk_layer.setZPosition(-1.0)`.
User-confirmed result on a fresh debug build:

- ✅ Toolbar More popover, tooltips, every GPUI overlay now composites
  *above* the editor body.  The visual layer-ordering bug was fixed.
- ❌ WKWebView's pixels stopped rendering.  The editor area is blank
  (the note still loads — `vault.save` IPC fires on open — but the
  rendered DOM never paints visibly).  Reverting `workspace.rs` root
  `.bg(theme.background)` and forcing `contentView.setWantsLayer(YES)`
  did not help; same blank editor.

Hypothesis (not yet verified): WKWebView's `CALayer` is a remote-layer
client tied to its initial parent.  Re-parenting via
`addSubview:positioned:relativeTo:` survives the NSView graph move but
the remote-CALayer / WebKit GPU-process surface either (a) doesn't
follow the move and ends up off-screen, or (b) is composited in a
3D-flattening pass that hides it when its `zPosition` is non-zero
under a non-3D `contentView` layer.  Either is plausible; both would
need a more aggressive AppKit/CALayer trace than this session could
budget.

**Reverted at the next commit.**  Phase 8's original
`fix_z_order_send_to_back` is back in place — overlays anchored inside
the editor body remain hidden, but the editor renders again.

*Re-ranked candidates for the next attempt:*

1. **Child `NSPanel` (was approach 2).**  Now the leading candidate.
   Re-introduce a minimal child-window mechanism for the overlay
   surfaces that actually need to anchor over the editor body
   (popovers + dialogs, possibly slash-menu).  Don't try to revive the
   full `OverlayTooltipExt` — tooltips already work outside the editor
   frame, and Phase 12 needs the child-window infrastructure regardless.
2. **CALayer mask on WKWebView**, updated when overlays show.  Mask
   out the overlay region from the WebView's `CALayer.mask` so the
   transparent mask reveals the Metal-drawn overlay underneath.  No
   re-parent.  Per-overlay-show: compute the union of overlay rects and
   set as the inverse mask.  Risk: WKWebView may use its own `mask`
   internally for accessibility / scroll-clipping — verify before
   committing.
3. **Re-parent with WebView in front (no zPosition trick).**  Same
   re-parent as the reverted attempt, but order WebView at the FRONT
   in subview order *without* the `zPosition = -1` push.  Visual
   ordering breaks (WebView still on top), but the rendering survives.
   Useless on its own, but worth re-trying with `contentView` made
   layer-backed to see whether the issue is `zPosition` interacting
   badly with the non-layer-backed default.

#### 10.1.2

**Root cause.**  The `Cmd+Alt+I` menu accelerator routes
`actions::ToggleElementInspector` through the active window's
responder chain, so by the time `cx.on_action(|_, cx| …)` fires at
the App level the window slot in `App::windows` is already taken by
the current dispatch update.  `cx.active_window().update(cx, |…|
window.toggle_inspector(app_cx))` then returns the documented
`Err("window not found")` (same re-entrancy guard captured by the
`active_window_update_from_inside_an_active_window_update_silently_drops`
regression test).  `.log_err()`-style swallowing turned the toggle
into a silent no-op for every menu-initiated dispatch.

**Fix (`crates/tolaria/src/main.rs:883–907`).**  Wrap the inner
`handle.update` in `cx.defer(|cx| …)` so the toggle runs *after* the
menu's dispatch update unwinds and the window slot is free.  Mirrors
the pattern used by `dispatch_to_workspace` (already deferring for
exactly this reason — see the deferred-closure comment at
`main.rs:223–256`).  `rebuild_menus(cx)` stays *inside* the deferred
closure so the post-toggle `Window::is_inspector_picking` state is
observed when the menu labels rebuild.

Side fix: `crates/tolaria/src/menus.rs` test-suite (5 cases) updated
to match the extra `View → Properties / Inspector` separator landed at
`fd151868`; the menu structure grew from 7 to 8 items but the tests
weren't refreshed in that commit, leaving the test suite red on
`feat/native-gpui-chrome`.  Tests now match the live menu shape.

**No `#[gpui::test]` regression.**  The re-entrancy contract is
already pinned by the
`active_window_update_from_inside_an_active_window_update_silently_drops`
test (`main.rs:2660–2691`) which captures the exact pattern this fix
escapes.  Adding a second test asserting `cx.defer` lets the toggle
through would duplicate that contract without protecting an
additional invariant.

**User manual validation** — trigger `Cmd+Alt+I` (or `View → Show /
Hide Inspector`) and confirm the GPUI element-picker overlay toggles
visibly + no `ToggleElementInspector update failed` warning lands in
the log.

#### 10.1.3

**Goal.**  Move the GPUI element-inspector UI out of the workspace
window (where it composited as a floating div top-right) and into a
separate, draggable, resizable OS window.  Frees the workspace's
top-right corner and lets the user keep the inspector visible while
poking the workspace.

**Architecture (`crates/tolaria/src/inspector_renderer.rs`).**

- `gpui::Inspector` stays Window-bound; the workspace's
  `Window::toggle_inspector(cx)` still mints the per-window inspector
  entity so GPUI's built-in `insert_inspector_hitbox` per-paint
  machinery keeps populating the entity from cursor hits in pick mode.
- New `InspectorBridge: gpui::Global` (App-level state) carries an
  `Option<Entity<gpui::Inspector>>` and an
  `Option<WindowHandle<InspectorWindow>>`.
- The existing `render_tolaria_inspector` renderer (the callback GPUI
  invokes inside the workspace's `Render for Inspector` impl) now:
    1. Captures `cx.entity()` (an `Entity<Inspector>`) into the
       bridge global on every paint — idempotent, GPUI's entity
       registry returns the same handle, one global write per
       inspector-on paint.
    2. Returns `gpui::Empty` instead of the old top-right panel — the
       in-workspace UI is gone.
- New `InspectorWindow` view holds the captured entity + a
  `cx.observe(&inspector, …)` subscription so any pick-state or
  active-element change in the workspace re-renders this window.  The
  render shape mirrors the old in-workspace panel: header strip,
  picking-state row, active-element row (or "No element selected").
- New `toggle_inspector_window(workspace_inspector_on: bool, cx)` is
  called from `ToggleElementInspector` right after
  `window.toggle_inspector(app_cx)`.  Opens the separate window when
  the workspace just turned the inspector on; closes it when off.

**The on/off arithmetic.**  GPUI doesn't expose
`Window.inspector.is_some()` directly — only
[`Window::is_inspector_picking`], which reads the sub-flag
`Inspector::is_picking` (a fresh inspector defaults to
`is_picking = false`).  So the toggle handler reads
`is_inspector_picking` *before* the toggle inside the same `update`
closure, then computes `new_state = !pre_state`.  The `cx.defer`
wrapper from `10.1.2` is preserved so the menu-accelerator
re-entrancy path keeps working.

**Tests (`inspector_renderer.rs` tests module).**

- `toggle_inspector_window_close_with_no_open_window_is_a_no_op` —
  pins the idempotency guard so the toggle handler can call this
  unconditionally without checking pre-state.
- `toggle_inspector_window_open_without_captured_entity_warns_and_returns`
  — pins the no-renderer-yet race (Cmd+Alt+I before the workspace's
  first paint installs the inspector renderer): the helper logs a
  warning and returns instead of panicking.
- Two existing tests (`render_tolaria_inspector_signature_matches_gpui_renderer`,
  `toggle_inspector_with_renderer_installed_does_not_panic`) keep
  their original contract — the renderer signature and the
  `set_inspector_renderer` install path are unchanged.

**User manual validation** — `Cmd+Alt+I` once: separate OS window
opens at (40, 40) with a 360×480 frame, title "Inspector — Tolaria".
Hovering elements in the workspace updates the panel's "Active
element" rows.  `Cmd+Alt+I` again: window closes; the workspace exits
pick mode.

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
