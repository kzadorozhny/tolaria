//! Native macOS menu bar for Tolaria (ADR-0115 Phase 1).
//!
//! Mirrors `crates/embed_poc/src/menus.rs` but imports actions from the
//! `actions` crate instead of re-declaring them. The Edit menu uses
//! `MenuItem::os_action` so AppKit's standard `cut:` / `copy:` / `paste:` /
//! `undo:` / `redo:` / `selectAll:` selectors keep routing into the focused
//! WKWebView unchanged (ADR-0115 §6).

use actions::{
    EditCopy, EditCut, EditPaste, EditRedo, EditSelectAll, EditUndo, NewNote, OpenSettings, Quit,
    Save,
};
use gpui::{Menu, MenuItem, OsAction};

/// Build the application menu bar.
///
/// Call via `cx.set_menus(app_menus())` before the first window opens so
/// AppKit picks up the accelerators immediately.
pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Tolaria".into(),
            disabled: false,
            items: vec![MenuItem::action("Quit Tolaria", Quit)],
        },
        Menu {
            name: "File".into(),
            disabled: false,
            items: vec![
                MenuItem::action("New Note", NewNote),
                MenuItem::action("Save", Save),
                MenuItem::separator(),
                MenuItem::action("Open Settings…", OpenSettings),
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

    /// Sanity-check the menu skeleton: three top-level menus with the expected
    /// item counts. Mirrors `embed_poc/src/menus.rs` test.
    #[test]
    fn app_menus_lists_app_file_and_edit_with_save_and_quit() {
        let menus = app_menus();
        let names: Vec<_> = menus.iter().map(|m| m.name.to_string()).collect();
        assert_eq!(names, vec!["Tolaria", "File", "Edit"]);

        // App menu: just Quit.
        let app_menu = menus
            .iter()
            .find(|m| m.name == "Tolaria")
            .expect("app_menus() must include the Tolaria menu");
        assert_eq!(app_menu.items.len(), 1, "App menu should hold just Quit");

        // File menu: New Note / Save / separator / Open Settings… = 4 entries.
        let file = menus
            .iter()
            .find(|m| m.name == "File")
            .expect("app_menus() must include the File menu");
        assert_eq!(
            file.items.len(),
            4,
            "File menu should hold New Note/Save/sep/Open Settings (4 entries)"
        );

        // Edit menu: Undo/Redo/sep/Cut/Copy/Paste/sep/SelectAll = 8 entries.
        let edit = menus
            .iter()
            .find(|m| m.name == "Edit")
            .expect("app_menus() must include the Edit menu");
        assert_eq!(
            edit.items.len(),
            8,
            "Edit menu should hold Undo/Redo/sep/Cut/Copy/Paste/sep/SelectAll (8 entries)"
        );
    }
}
