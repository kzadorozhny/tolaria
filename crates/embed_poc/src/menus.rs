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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{KeyBinding, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    /// Sanity check on the menu skeleton: shape lines up with what the
    /// binary's `set_menus(app_menus())` call expects, and the Edit menu
    /// uses `MenuItem::os_action` for the standard selectors so they
    /// keep routing through AppKit into the focused WKWebView.
    #[test]
    fn app_menus_lists_app_file_and_edit_with_save_and_quit() {
        let menus = app_menus();
        let names: Vec<_> = menus.iter().map(|m| m.name.to_string()).collect();
        assert_eq!(names, vec!["Tolaria PoC", "File", "Edit"]);

        // File > Save must be present (the Cmd+S target). We don't
        // crack open the MenuItem variant — that's gpui-private — but
        // the File menu must have exactly one item.
        let file = menus.iter().find(|m| m.name == "File").unwrap();
        assert_eq!(file.items.len(), 1, "File menu should hold just Save");

        // The Edit menu must include the six standard selectors plus
        // two separators. Over-capturing them with our own handlers
        // would break Cmd+C/V inside the embedded webview.
        let edit = menus.iter().find(|m| m.name == "Edit").unwrap();
        assert_eq!(
            edit.items.len(),
            8,
            "Edit menu should hold Undo/Redo/sep/Cut/Copy/Paste/sep/SelectAll (8 entries)"
        );
    }

    /// Scenario 4 — Cmd+S delivery (positive path). Binds the same
    /// `cmd-s → Save` keymap entry the binary registers in `main`,
    /// then drives `cmd-s` through the test platform and asserts the
    /// global `on_action(Save)` handler runs exactly once. Replaces
    /// the `qa-cmd-s.sh` script's "send Cmd+S, grep for cmd_s_fired"
    /// step.
    ///
    /// The test platform's key dispatch goes through the same
    /// `dispatch_keystroke` plumbing that AppKit's menu key-equivalent
    /// chain feeds in production, so this exercises the keymap binding
    /// resolution end-to-end — the only thing it doesn't cover is the
    /// AppKit-vs-WKWebView race the ADR-0115 §6 claim is really about
    /// (which is intrinsic to the OS and stays in the README's manual
    /// validation script).
    #[gpui::test]
    fn cmd_s_dispatches_save_action(cx: &mut TestAppContext) {
        let fired = Rc::new(Cell::new(0u32));
        let fired_handler = fired.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &Save, _| {
                fired_handler.set(fired_handler.get() + 1);
            });
            cx.bind_keys([KeyBinding::new("cmd-s", Save, None)]);
        });

        let window = cx.add_empty_window();
        window.simulate_keystrokes("cmd-s");
        window.run_until_parked();

        assert_eq!(
            fired.get(),
            1,
            "expected Save to fire once for one cmd-s keystroke"
        );
    }

    /// Scenario 4 — Cmd+S delivery (negative path). Standard Edit-menu
    /// chords (cmd-a, cmd-c, cmd-v) must NOT route to our Save handler;
    /// they are reserved for the focused WKWebView's standard selector
    /// chain. Replaces the `qa-cmd-s.sh` script's second half ("type
    /// cmd-a/cmd-c, verify no cmd_s_fired").
    #[gpui::test]
    fn standard_edit_chords_do_not_dispatch_save(cx: &mut TestAppContext) {
        let fired = Rc::new(Cell::new(0u32));
        let fired_handler = fired.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &Save, _| {
                fired_handler.set(fired_handler.get() + 1);
            });
            cx.bind_keys([KeyBinding::new("cmd-s", Save, None)]);
        });

        let window = cx.add_empty_window();
        window.simulate_keystrokes("cmd-a");
        window.simulate_keystrokes("cmd-c");
        window.simulate_keystrokes("cmd-v");
        window.run_until_parked();

        assert_eq!(
            fired.get(),
            0,
            "Save must not fire for cmd-a / cmd-c / cmd-v — those reach the webview via OsAction"
        );
    }

    /// Scenario "menu wins over plain key" — pressing `s` without
    /// modifiers must NOT dispatch Save (otherwise typing into the
    /// textarea would save on every `s` character).
    #[gpui::test]
    fn plain_s_does_not_dispatch_save(cx: &mut TestAppContext) {
        let fired = Rc::new(Cell::new(0u32));
        let fired_handler = fired.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &Save, _| {
                fired_handler.set(fired_handler.get() + 1);
            });
            cx.bind_keys([KeyBinding::new("cmd-s", Save, None)]);
        });

        let window = cx.add_empty_window();
        window.simulate_keystrokes("s");
        window.run_until_parked();

        assert_eq!(fired.get(), 0, "plain `s` must not dispatch Save");
    }
}
