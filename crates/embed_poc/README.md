# embed_poc — ADR-0115 Phase 0 spike

Minimal Rust binary that proves the native GPUI shell + embedded WKWebView
path proposed in [ADR-0115](../../docs/adr/0115-native-gpui-chrome.md). It is
**macOS only** and intentionally additive: the existing Tauri app under
`src-tauri/` is untouched.

The spike is built up across tasks #2–#8 and ultimately validates four
behaviours that the ADR's "Re-evaluation triggers" depend on:

1. **Focus handoff** — clicking between GPUI chrome and the embedded
   WKWebView transfers first-responder cleanly.
2. **IME composition** — multi-keystroke input (kana, dead keys) survives
   round-tripping while the WKWebView holds focus.
3. **Frame sync** — dragging the GPUI sidebar resizes the WKWebView in
   lockstep, with no tearing or one-frame lag.
4. **Cmd+S delivery** — the native NSMenu fires its Cmd+S key equivalent
   before the WKWebView swallows the event.

Task #2 (this commit) only stands up the workspace and opens an empty GPUI
window titled "Tolaria Phase 0 Spike". The full four-check validation script
arrives in task #8.

## Run it

```
cargo run -p embed_poc
```

A blank dark-grey window should appear; closing it exits cleanly.
