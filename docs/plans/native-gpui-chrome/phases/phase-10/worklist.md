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

10.3.1. address review feedback from `review.md`

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

#### 10.2.1

Consumed by `actions` and Phase 12.1 `command_palette`
