//! Native macOS menu bar for the ADR-0115 Phase 0 spike (task #6).
//!
//! Validates the ADR-0115 §6 claim: an `NSMenu` installed via
//! `cx.set_menus(...)` (paired with `cx.bind_keys(...)` accelerators) wins
//! over the focused WKWebView's keyDown chain. macOS routes the key
//! equivalent to the menu first, so `Cmd+S` fires the Rust handler even
//! while the textarea inside the embedded webview holds firstResponder.
//!
//! The shape here mirrors `zed/crates/zed/src/zed/app_menus.rs`: `Menu`
//! / `MenuItem` / `OsAction` from gpui. The Edit menu intentionally uses
//! `MenuItem::os_action(..., OsAction::*)` so AppKit's standard
//! `cut:` / `copy:` / `paste:` / `undo:` / `redo:` / `selectAll:`
//! selectors keep routing into the focused webview unchanged.

use gpui::{Menu, MenuItem, OsAction, actions};

actions!(
    embed_poc,
    [
        /// File > Save — fires the cmd_s_fired log line in the global
        /// action handler installed by `main`.
        Save,
        /// App menu > Quit — bound to Cmd+Q.
        Quit,
        // Standard Edit-menu placeholders. We do NOT register handlers
        // for these; the `os_action` variant routes through AppKit's
        // standard selector chain, which lands in the focused WKWebView
        // for free.
        EditUndo,
        EditRedo,
        EditCut,
        EditCopy,
        EditPaste,
        EditSelectAll,
    ]
);

pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Tolaria PoC".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Quit Tolaria PoC", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Save", Save),
            ],
        },
        Menu {
            name: "Edit".into(),
            disabled: false,
            items: vec![
                MenuItem::os_action("Undo", EditUndo, OsAction::Undo),
                MenuItem::os_action("Redo", EditRedo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", EditCut, OsAction::Cut),
                MenuItem::os_action("Copy", EditCopy, OsAction::Copy),
                MenuItem::os_action("Paste", EditPaste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", EditSelectAll, OsAction::SelectAll),
            ],
        },
    ]
}
