# e2e harness — Claude workflow for observing Tolaria

`crates/periscope/` is the screenshot harness that lets Claude (the
AI assistant) inspect a running `tolaria` window via its multimodal
`Read` tool.  This doc captures the typical flows Claude follows
during interactive debugging.

For library / CLI reference see `crates/periscope/README.md`.

---

## Setup, once per machine

1. Build the binaries: `cargo build -p tolaria -p periscope`.
2. Grant the parent terminal application **Screen Recording**
   permission under **System Settings → Privacy & Security →
   Screen Recording**.  Required for any capture to return a
   non-black PNG.
3. Grant the same terminal **Accessibility** permission (under
   **… → Accessibility**) — needed for the `--raise` flag and
   the `list` subcommand.

Confirm with the smoke test (opt-in via env var so the default
`cargo test` lane stays green on permission-less hosts):

```sh
TOLARIA_E2E_SMOKE=1 cargo test -p periscope
```

It spawns `tolaria --vault demo-vault-v2` against the demo fixture,
captures a PNG, asserts the file is > 100 kB (sized to catch the
`font-kit` regression — a window without rendered text serialises
at ~88 kB), and tears the child down.  If it errors with "PNG too
small", revisit the Screen Recording grant for the terminal that
ran the test.

---

## One-shot screenshot (the common case)

```sh
# Terminal A — user starts the app once.  `--width` / `--height`
# pin the window to the logical-point size of the Tauri-era
# reference captures (`docs/plans/native-gpui-chrome/tolaria-demo-vault-v2-*.png`,
# 3032×2104 @ 2× Retina) so harness screenshots line up with
# the references without any window-resizing wrangling.
cargo run -p tolaria -- \
    --vault demo-vault-v2 \
    --width 1516 --height 1052

# Terminal B — Claude (via the Bash tool) captures the current state
cargo run -q -p periscope -- screenshot \
    --title Tolaria --raise --out /tmp/tolaria-now.png
```

Then Claude calls `Read /tmp/tolaria-now.png` — the multimodal Read
returns the image content directly to the model.

`--raise` brings the Tolaria window forward via the Accessibility
API before capture.  Drop it if the window is already focused (saves
a ~250 ms settle delay).

`--width` / `--height` are independent overrides for the persisted
`WindowSettings` in `~/Library/Application Support/Tolaria/settings.json`.
Each takes a strictly-positive `f32` in logical points; omit either
to fall back to the persisted value.  The smoke test passes both —
see `crates/periscope/tests/screenshot_smoke.rs`.

---

## Driving the UI — `click` subcommand

When inspection alone isn't enough — e.g. you want to verify the
note-open flow lands an item in the center pane — synthesize a
left-click at a window-local coordinate:

```sh
cargo run -q -p periscope -- click \
    --title Tolaria --raise --x 200 --y 100
```

Coordinates are in window points with the origin at the window's
top-left corner (matching GPUI's layout coordinates).  The harness
translates to screen space via `xcap::Window::x()` / `.y()` before
posting a `CGEvent` mouse-down + mouse-up pair through the OS event
queue, so GPUI's own hit-testing sees the click as if it had come
from a real cursor.

This is what the smoke test uses to exercise `NoteListPane`'s
`OpenNoteEvent` end-to-end: capture before, click at the first row,
capture after, assert the rendered output differs.  See
`crates/periscope/tests/screenshot_smoke.rs`.

The Accessibility-API path that GPUI components offer is *not* an
option — GPUI draws controls into a Metal layer, so the AX
hierarchy doesn't see them and `AXUIElementPerformAction` never
reaches the click handlers.  CGEvent is the only path that works.

---

## Driving the UI by element name — `click-id` + `tree_dump`

Hand-picked pixel coordinates rot the moment a layout tweak shifts an
element.  For stable targets, Tolaria (in debug builds) ships a
SIGUSR1-triggered **element-tree dump** that records the laid-out
window-local bounds of every `.dump_as("name")`-tagged element.
Periscope reads that JSON to translate names → click coordinates.

```sh
# 1. Inspect what's available (refreshes the dump first):
cargo run -q -p periscope -- dump-tree --title Tolaria

# Output:
#   # tree_dump  pid=10830  path="…/tolaria-ui-tree-10830.json"  entries=1
#   status-bar-theme-toggle    x= 1422.0 y= 1056.5 w=  27.0 h=  19.5

# 2. Click an element by name:
cargo run -q -p periscope -- click-id \
    --title Tolaria --raise --id status-bar-theme-toggle
```

The flow under the hood:

1. `periscope` resolves the target window's PID (via `xcap`).
2. `periscope` sends `SIGUSR1` to the PID.
3. Tolaria's `ui::tree_dump` signal-handler thread snapshots its
   registry (every paint pass writes laid-out bounds for opted-in
   elements) to `$TMPDIR/tolaria-ui-tree-<pid>.json` (atomic via
   tmp + rename).
4. `periscope` polls the file's `mtime` until it's strictly newer
   than the previous value (2 s default deadline).
5. `periscope` reads the JSON, looks up the requested name, and
   calls `periscope::click(target, center_x, center_y)`.

Coordinate-space note: the JSON's `y` is **frame-relative** (includes
the 28 pt native title bar offset).  `ui::tree_dump::set_window_y_offset(28.0)`
in `tolaria/src/main.rs` adds the offset at register time so the
dump and `periscope click --x --y` use the same coordinate system.

### Opting an element in

Add `.dump_as("stable-name")` to the relevant GPUI element:

```rust
use ui::tree_dump::DumpAsExt as _;

div()
    .id("status-bar-theme-toggle")
    .cursor_pointer()
    .on_click(|_, _window, cx| theme::cycle(cx))
    .child(theme_toggle_label)
    .dump_as("status-bar-theme-toggle")
```

Names are `&'static str` (zero-alloc) and overwrite in place on every
paint pass — there's no cleanup story, so the registry always
reflects the most recent layout.

### Why SIGUSR1?

It's the cheapest cross-process trigger that doesn't bring up an
IPC socket or a debug-only HTTP server.  Tolaria already runs an
event loop; the signal-handler thread is one extra OS thread, IO
happens off the signal-delivery path, and the bash one-liner
`kill -USR1 <pid>` is all an external tool needs.  Release builds
skip the install entirely (`#[cfg(debug_assertions)]`), so the
developer-facing IPC channel never ships.

---

## Long debug session — `watch` mode

For multi-turn debugging where Claude inspects the UI repeatedly,
run the watcher in the background and read `latest.png` between
turns:

```sh
# Background watch — kill with Ctrl-C or `pkill periscope`
cargo run -q -p periscope -- watch \
    --title Tolaria --dir target/e2e/ --interval-secs 3
```

The harness writes `target/e2e/frame-0001.png`, `frame-0002.png`, …
and maintains a `target/e2e/latest.png` symlink to the most recent
frame.  Claude just `Read target/e2e/latest.png` whenever it wants
to look.

Add `--max-frames N` to stop automatically after N frames; default
`0` loops forever.

Clean the directory between sessions:

```sh
rm -rf target/e2e/
```

---

## Diagnostics

`window not found` errors are usually a title mismatch.  Dump every
visible window to confirm:

```sh
cargo run -q -p periscope -- list
```

Expected output looks like:

```
pid=12345    app=Tolaria                          title=Tolaria
pid=67890    app=Terminal                         title=Terminal — bash …
pid=11111    app=Finder                           title=
```

If the Tolaria row is missing, the app isn't running.  If the title
column for Tolaria isn't exactly `Tolaria`, something changed in
`crates/tolaria/src/main.rs:214` and the harness needs the new value
(or use `--pid` instead of `--title`).

`list` requires Accessibility permission — same panel as `--raise`.

---

## Failure modes and what to do

| Symptom | Likely cause | Fix |
|---|---|---|
| Error: `PNG too small (X bytes) — Screen Recording permission missing for $TERM_PROGRAM` | Screen Recording not granted | System Settings → Privacy & Security → Screen Recording; toggle on for the terminal app |
| Error: `AXUIElement.windows attribute fetch failed` | Accessibility not granted | … → Accessibility; toggle on for the terminal app |
| Error: `no window with title "Tolaria"` | Tolaria not running, or title changed | `cargo run -p periscope -- list` to inspect; relaunch Tolaria if needed |
| Smoke test hangs / times out at 15s | Cold debug build, WKWebView slow to init | Run `cargo build -p tolaria` first; retry; bump deadline in `tests/screenshot_smoke.rs` if persistent |
| `list` shows Tolaria but `screenshot --title Tolaria` says not found | Title comparison is exact-match including whitespace | Copy the exact title from `list` output |

---

## What captures look like

A successful capture is a full-window PNG: GPUI chrome (left dock
with `NoteListPane`, status bar, modal layer if open) **plus** the
WKWebView editor body content (markdown text from the loaded note).
This is the load-bearing reason for OS-compositor capture — in-process
`Window::render_to_image()` would have a black rectangle where the
editor body lives.

A capture missing the editor body (black rectangle in the middle of
the screenshot) usually means the harness pivoted to in-process
capture by mistake; check that `xcap::Window::capture_image()` is
still the capture path in `crates/periscope/src/capture.rs`.
