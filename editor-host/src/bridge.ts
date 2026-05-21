// Typed TypeScript mirror of the `editor_bridge` Rust crate.
//
// The wire format MUST stay in lockstep with
// `crates/editor_bridge/src/lib.rs`.  When you change one side, change
// the other in the same commit.  Tests on the Rust side
// (`envelope_kinds_use_snake_case_*`) lock the `k` strings;
// the only catch the type system provides here is the literal-union
// tag — if you misspell `note_open`, TypeScript flags the dispatch.

// ---------------------------------------------------------------------------
// Native → Editor
// ---------------------------------------------------------------------------

export type ThemeMode = "light" | "dark";

export type ToHost =
    | { k: "note_open"; v: { id: number; path: string; body: string } }
    | { k: "focus_editor" }
    | { k: "save_request" }
    | { k: "theme_set"; v: { mode: ThemeMode } }
    | { k: "set_raw_mode"; v: { enabled: boolean } };

// ---------------------------------------------------------------------------
// Editor → Native
// ---------------------------------------------------------------------------

export interface Mods {
    alt?: boolean;
    ctrl?: boolean;
    meta?: boolean;
    shift?: boolean;
}

/**
 * One heading entry shared with the native `editor_bridge::Heading`
 * struct.  The native ToC panel uses `level` for indentation and
 * `anchor` for click navigation; `text` is the visible label.
 *
 * `anchor` is the BlockNote block id when one is available; otherwise
 * a slug derived from `text`.  The native side treats the value as
 * opaque — it must only stay stable for a given heading so a click
 * always resolves to the same point in the editor body.
 */
export interface Heading {
    level: number;
    text: string;
    anchor: string;
}

export type FromHost =
    | { k: "ready" }
    | { k: "dirty"; v: { id: number } }
    | { k: "save"; v: { id: number; body: string } }
    | { k: "saved"; v: { id: number } }
    | { k: "link_click"; v: { target: string } }
    | { k: "keydown"; v: { key: string; mods: Mods } }
    | { k: "headings"; v: { items: Heading[] } };

// ---------------------------------------------------------------------------
// IPC plumbing
// ---------------------------------------------------------------------------

/**
 * `wry` exposes `window.ipc.postMessage(string)` for editor→native.
 * Define a minimal type so call sites stay typed without depending on
 * `@types/tauri` or shipping ambient declarations.
 */
declare global {
    interface Window {
        ipc?: { postMessage(msg: string): void };
        /**
         * Native shell calls this with a JSON-encoded `ToHost` message
         * via `WebView::evaluate_script("tolariaBridge.receive(...)")`.
         */
        tolariaBridge?: { receive(json: string): void };
    }
}

/** Send a [`FromHost`] message to the native shell. */
export function send(msg: FromHost): void {
    if (!window.ipc) {
        // Standalone preview (`vite dev`) — log so the editor still
        // works for layout iteration without a native shell.
        console.info("[editor-host:from_host]", msg);
        return;
    }
    window.ipc.postMessage(JSON.stringify(msg));
}

/**
 * Register a handler for `ToHost` messages.  The handler is installed
 * on `window.tolariaBridge.receive`; the native shell calls it via
 * `WebView::evaluate_script`.
 */
export function onReceive(handler: (msg: ToHost) => void): void {
    window.tolariaBridge = {
        receive(json: string) {
            let parsed: ToHost;
            try {
                parsed = JSON.parse(json) as ToHost;
            } catch (e) {
                console.error("[editor-host] malformed ToHost envelope", json, e);
                return;
            }
            handler(parsed);
        },
    };
}
