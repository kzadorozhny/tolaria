# `embed_poc` — ADR-0115 Phase 0 spike

This crate is the Phase 0 validation spike for
[ADR-0115](../../docs/adr/0115-native-gpui-chrome.md): a tiny native-GPUI
shell that embeds a single `WKWebView` (via the
[longbridge/gpui-component `crates/webview` package](https://github.com/longbridge/gpui-component/tree/main/crates/webview),
crate name `gpui-wry`) as a sibling `NSView` in the main window. Its only
purpose is to give a tester enough surface to either green-light or kill
the embedded-JS-editor path before the rest of the migration starts.

The spike is **macOS-only**. On any other platform the binary prints a
single line and exits with code 2 — that is deliberate, the embedding
pattern this validates (wry's `build_as_child` + AppKit `addSubview_`) is
macOS-specific.

## What this spike validates

ADR-0115 §9 + the "re-evaluation triggers" in *Consequences* both name
four assumptions that must hold before further crates are written:

1. **WKWebView focus handoff** between GPUI chrome and the embedded
   webview must be clean — clicks across the boundary must move
   firstResponder without dropped events.
2. **IME mid-composition** must survive simultaneous chrome activity —
   `compositionstart` → `compositionupdate*` → `compositionend` must
   complete with the correct buffer even while another input source is
   sending the webview keystrokes.
3. **Frame sync during sidebar drag** must keep the webview's NSView
   pinned to the GPUI-computed bounds with no perceptible lag and no
   visible tearing.
4. **Cmd+S delivery** (and other native-menu key equivalents) must reach
   Rust *before* the focused WKWebView gets a chance to swallow them.

The spike emits structured log lines on dedicated `log` targets so each
goal can be checked from stdout without screenshots. The validation
script in this README walks through all four.

## How to run

```sh
# All log targets at info level (the recommended default).
RUST_LOG=embed_poc=info cargo run -p embed_poc

# Add debug to see frame-sync epsilon-suppressed lines from
# InstrumentedWebView (helps verify the 0.5 px guard is actually firing
# when the layout settles).
RUST_LOG=embed_poc=debug cargo run -p embed_poc

# Or target individual streams if stdout is too noisy.
RUST_LOG=embed_poc::focus=info,embed_poc::ime=info,embed_poc::menu=info \
  cargo run -p embed_poc
```

The window opens at ~1200×800 with a dark sidebar on the left
(`Sidebar` label), a draggable splitter, and the test HTML page on the
right — `<h1>Phase 0 Spike WebView</h1>` plus a textarea, a single-line
input, and a button. Closing the window exits cleanly.

Log targets you will see, with the format produced:

| Target                | Format                                                                                |
| --------------------- | ------------------------------------------------------------------------------------- |
| `embed_poc::macos`    | startup banner                                                                        |
| `embed_poc::frame`    | `frame_event kind=sidebar_resize ...`, `frame_event kind=window_resize ...`, `frame_sync x= y= w= h=` (info), `frame_sync_skip ...` (debug) |
| `embed_poc::focus`    | `gpui_focus state=in/out target=sidebar`, `webview_focus state=in/out target=textarea` |
| `embed_poc::ime`      | `ime phase=compositionstart/update/end data=<str> value_len=<n>`                       |
| `embed_poc::menu`     | `cmd_s_fired`                                                                          |
| `embed_poc::ipc`      | `keydown key=<k> mods=<m>` and raw fallback `ipc raw=<json>`                          |

## Validation script

Run each scenario in order. For each goal, mark **PASS** if the expected
output appears and the rendered window behaves as described, or **FAIL**
otherwise — and append a row to `RESULTS.md` (see "Reporting back"
below).

> The textarea ships with `autofocus` so on launch it is the active
> element inside the webview. The very first stdout entries may be a
> `frame_sync ...` (the InstrumentedWebView's initial bounds push) and a
> `frame_event kind=window_resize ...` (any window-bounds adjustment
> AppKit emits during open). Both are expected and not part of the
> scenarios below.

### 1. WKWebView focus handoff (ADR-0115 re-eval trigger: focus)

**Steps**

1. Click once inside the dark sidebar on the left.
2. Click once inside the textarea on the right.
3. Click once inside the single-line input below the textarea.
4. Click once back inside the sidebar.

**Expected stdout** (order matters; only `embed_poc::focus` lines listed
— other targets may interleave):

```
gpui_focus state=in target=sidebar
gpui_focus state=out target=sidebar
webview_focus state=in target=textarea
webview_focus state=out target=textarea
webview_focus state=in target=single-line
webview_focus state=out target=single-line
gpui_focus state=in target=sidebar
```

**PASS criteria**

- Every transition is observed exactly once.
- No duplicate `state=in` / `state=out` for the same target without a
  matching counterpart.
- No focus event for `target=textarea` arrives after `target=single-line`
  has taken focus.

**FAIL examples**

- A `gpui_focus state=out target=sidebar` arrives without the matching
  `webview_focus state=in ...` (focus disappeared).
- Two `webview_focus state=in target=textarea` in a row (the entity
  thought it lost focus when it never did).
- Clicking inside the webview *body* (outside any form element) yields
  no `webview_focus` line — that is a known gap, not a failure (the JS
  bridge only listens on the textarea / single-line input). Note in
  `RESULTS.md` instead of marking FAIL.

### 2. IME mid-composition (ADR-0115 re-eval trigger: IME)

**Setup**

You need a CJK input method available. The cheapest option:

1. macOS **System Settings → Keyboard → Input Sources → Edit / +**.
2. Add **Japanese → Hiragana** if it is not already there.
3. Switch input source with **Ctrl+Space** (or Caps Lock-toggle, or the
   menu-bar IME picker — whichever you have set up).

**Steps**

1. Click into the textarea so it has focus (look for
   `webview_focus state=in target=textarea`).
2. Switch the input source to Japanese Hiragana.
3. Type the romaji `konnichiha` slowly. Hiragana ime should compose into
   `こんにちは`.
4. Press **Return** (or **Space**+**Return**) to commit.
5. Switch back to your normal input source.

**Expected stdout** (just the `embed_poc::ime` lines; values vary by IME
behaviour, the structure is what matters):

```
ime phase=compositionstart data="" value_len=0
ime phase=compositionupdate data="k" value_len=1
ime phase=compositionupdate data="こ" value_len=1
ime phase=compositionupdate data="こn" value_len=2
ime phase=compositionupdate data="こん" value_len=2
...
ime phase=compositionend data="こんにちは" value_len=5
```

**PASS criteria**

- Exactly one `compositionstart` precedes any `compositionupdate`.
- Exactly one `compositionend` follows the last `compositionupdate`.
- `value_len` on `compositionend` matches the final character count in
  the textarea (5 for `こんにちは`).
- No keystroke was lost — every romaji letter you typed produced at
  least one `compositionupdate` entry.

**Repeat with chrome activity** (the harder check)

1. Click back into the textarea.
2. Start a second composition (`arigatou` → `ありがとう`).
3. **While composing**, use a second hand to drag the sidebar splitter
   left or right.
4. Finish the composition with Return.

**PASS criteria for the harder check**

- The composition completes (a single `compositionend` line appears with
  `value_len` for the appended Japanese string — it should be the
  previous text length + 5).
- The intermediate `frame_event kind=sidebar_resize ...` lines from the
  drag are interleaved with `compositionupdate` lines but no
  `compositionend` line is emitted prematurely.
- No `compositionupdate` events get an empty `data=""` that didn't come
  from the user.

**FAIL examples**

- `compositionend` arrives before the user committed (IME aborted by
  layout).
- `compositionupdate` fires after `compositionend` (re-entered
  composition).
- Lost keystrokes — total length of the appended Japanese text is less
  than the romaji entered.

### 3. Frame sync (ADR-0115 re-eval trigger: frame sync)

**Steps**

1. Press and hold the mouse on the splitter between the sidebar and the
   webview.
2. Drag the splitter slowly left and right several times. Sidebar is
   clamped to `[160, 480]` px.
3. Release the mouse.
4. Drag the OS window corner to resize the window itself.
5. Let the window come to rest.

**Expected stdout** (selected `embed_poc::frame` lines; `frame_sync`
should pair with each `frame_event`):

```
frame_event kind=sidebar_resize width=232.0 content_w=968.0 content_h=842.0
frame_sync x=232.0 y=0.0 w=968.0 h=842.0
frame_event kind=window_resize viewport_w=1180.0 viewport_h=840.0
frame_sync x=232.0 y=0.0 w=948.0 h=840.0
```

(In `RUST_LOG=embed_poc=debug` mode you should also see one or more
`frame_sync_skip ...` lines once the drag has settled — the 0.5 px
epsilon guard suppressing same-bounds re-prepaints.)

**PASS criteria**

- The webview re-flows immediately with each drag tick; no flash of
  empty space and no overlap of the webview on top of the sidebar.
- Every `frame_event` line has a `frame_sync` line with matching
  dimensions within 1 px.
- After release, the very next prepaint logs `frame_sync_skip` at debug
  level (instead of another full `frame_sync`), confirming the epsilon
  guard.
- Final webview bounds match the visible content area to the eye.

**FAIL examples**

- The webview lags more than one frame behind the splitter (visible
  empty strip on the trailing edge).
- `frame_sync` lines stop firing while the drag is still in progress
  (the prepaint hook is being bypassed somehow).
- The webview ends up at a different size than `frame_sync` reports.
- `frame_sync_skip` never appears at debug level (the epsilon guard is
  silently broken).

### 4. Cmd+S delivery (ADR-0115 re-eval trigger: native menu)

**Steps**

1. With the spike running, look at the macOS menu bar — confirm there
   are **Tolaria PoC**, **File**, and **Edit** menus.
2. Open **File** in the menu bar — confirm a **Save** item with the
   `⌘S` key equivalent rendered on the right.
3. Click into the sidebar (so GPUI chrome holds focus) and press **⌘S**.
4. Click into the textarea, type a few ASCII characters, then press
   **⌘S** while the textarea still has focus.
5. Type a few more characters into the textarea, then perform a normal
   **⌘C** / **⌘V** within it (select text first if needed).
6. Press **⌘Q**.

**Expected stdout** (selected `embed_poc::menu` and `embed_poc::ipc`
lines):

```
cmd_s_fired                                # step 3 (GPUI-focused)
cmd_s_fired                                # step 4 (webview-focused)
```

`step 5` should produce no `cmd_s_fired` lines; the textarea contents
must remain visible and pasted text must appear. `step 6` quits the
process.

**PASS criteria**

- `cmd_s_fired` fires for both step 3 *and* step 4. The webview must NOT
  swallow `⌘S`; the textarea must NOT receive an `s` character on step
  4.
- Standard Edit-menu operations (`⌘C`, `⌘V`) keep working inside the
  textarea — the `os_action` wiring in `menus.rs` does not over-capture
  them.
- `⌘Q` exits cleanly with code 0 (`cx.quit()` path).

**FAIL examples** — *these are the conditions that would re-open the
ADR-0115 native-GPUI-editor alternative*:

- `⌘S` typed into the textarea inserts an `s` (or any character)
  instead of firing `cmd_s_fired`.
- `cmd_s_fired` fires twice for one keystroke (the menu and the webview
  both processed it).
- `⌘C` / `⌘V` stop working inside the textarea (we accidentally
  swallowed standard Edit selectors).

## Known limitations

These were surfaced during the build-up tasks (`#2`–`#7`); please call
them out in `RESULTS.md` rather than marking FAIL if you hit them:

- **`gpui-component` is pinned to upstream HEAD.** v0.5.1 lacks the
  `crates/webview` (`gpui-wry`) package used here, and v0.5.2 has not
  been tagged upstream yet. The pin will move to the v0.5.2 sha when it
  ships.
- **`runtime_shaders` feature is on.** Without it, `gpui_macos`'s build
  script invokes `xcrun metal`, which only ships with the full Xcode.
  CLI-Tools-only hosts work fine because of this flag — but it also
  means shader compilation happens at first paint, so the very first
  frame may take a fraction of a second longer than steady state.
- **The mouse-blur-on-click-outside helper in upstream `gpui-wry` is not
  active.** Task #5 replaced the upstream `Render` impl with
  `InstrumentedWebView` to bolt on epsilon + logging, which dropped
  upstream's `MouseDownEvent` handler that called `focus_parent()` on
  clicks outside the webview bounds. In practice the AppKit hit-test
  still routes clicks elsewhere correctly, but Rust-side focus
  arbitration may differ; flag any anomaly.
- **JS-side `webview_focus` only fires from the textarea and single-line
  input.** Clicks on `<button>`, `<pre>`, or empty whitespace inside
  the webview produce no focus event on this target.
- **`frame_sync` runs once per layout pass.** During a fast drag you
  will see many `frame_sync` lines per second; that is by design — the
  log target is the evidence stream. If you want only committed values,
  filter for `frame_event` (the sidebar/window resize callbacks fire on
  drag-end, not per frame).

## Reference repos

When something looks wrong, the source of truth for each subsystem:

- **ADR-0115** — `docs/adr/0115-native-gpui-chrome.md` on this branch.
- **Embedding pattern (NSView-as-sibling proof)** — Zed's
  `crates/gpui_macos/src/window.rs:783–884` (content-view lookup,
  autoresizing-mask wiring, `addSubview_`, `makeFirstResponder_`).
- **`gpui-wry` upstream wrapper** —
  `gpui-component/crates/webview/src/lib.rs`. See `WebViewElement::prepaint`
  at lines 178–204 for the canonical bounds-translation math the
  `InstrumentedWebView` here mirrors.
- **Native menu template** — Zed's
  `crates/zed/src/zed/app_menus.rs`. The Edit-menu `os_action` pattern
  is copied verbatim.
- **Focus listener API** — Zed's `crates/gpui/src/app/context.rs:547–660`
  for `on_focus` / `on_blur` / `on_focus_in` / `on_focus_out` signatures.

## Automated QA

All automation runs in-process through GPUI's test infrastructure
(`TestAppContext`, `TestDispatcher`, the `gpui::test` proc macro and
the `test-support` feature on `gpui_platform`). No real window opens,
no `osascript` keystrokes, no accessibility prompts. Run:

```sh
cargo test -p embed_poc
```

What's covered, per scenario:

| Scenario | Coverage | Remaining manual |
| --- | --- | --- |
| 1. Focus handoff | `layout::tests::focusing_sidebar_handle_sets_window_focus` and `moving_focus_off_sidebar_clears_window_focus_for_it` drive `window.focus(handle)` on the sidebar's `FocusHandle` and assert `Window::focused()` follows. | Sidebar↔WKWebView boundary clicks. The webview side of focus only exists in the real Cocoa+WKWebView stack; the test platform has no NSView to hand firstResponder to. |
| 2. IME composition | None — see [§IME](#2-ime-mid-composition-adr-0115-re-eval-trigger-ime). The full romaji-to-CJK + mid-frame composition behaviour stays a human pass because the test platform has no real IME pipe. | Entire scenario. |
| 3. Frame sync | `layout::tests::simulate_window_resize_updates_last_viewport` and `simulate_window_resize_with_sub_epsilon_drift_is_a_noop` drive `TestAppContext::simulate_window_resize`, which fires the same `observe_window_bounds` callback `RootView` registers in production. Together with `close_enough` / `same_size` epsilon-math tests they cover both the resize-detection and the dedupe halves. | Sidebar splitter drag — splitter geometry math lives in `gpui-component::resizable` and the splitter UI is interactive-only. |
| 4. Cmd+S delivery | `menus::tests::cmd_s_dispatches_save_action` binds the same `cmd-s → Save` keymap entry `main` registers, drives the keystroke through `TestAppContext::simulate_keystrokes`, and asserts the global `on_action(Save)` handler fires once. `standard_edit_chords_do_not_dispatch_save` and `plain_s_does_not_dispatch_save` cover the negative paths (cmd-a/c/v and plain `s` must NOT route to Save). | The AppKit-vs-WKWebView race the ADR-0115 §6 claim is really about — that's intrinsic to the OS and only the README's manual walk can exercise it. |

Plus a layer of pure unit tests on the helpers each scenario depends
on: `close_enough` and `same_size` (epsilon guards), `parse_ipc_body`
(the `{k, v}` envelope from the test page), and `app_menus_lists_app_file_and_edit_with_save_and_quit`
(menu structure sanity).

A regression in any of these will turn red under `cargo test -p embed_poc`
before the manual validation pass even starts.
