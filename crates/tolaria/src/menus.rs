//! Native macOS menu bar for Tolaria (ADR-0115 Phase 1).
//!
//! Mirrors `crates/embed_poc/src/menus.rs` but imports actions from the
//! `actions` crate instead of re-declaring them. The Edit menu uses
//! `MenuItem::os_action` so AppKit's standard `cut:` / `copy:` / `paste:` /
//! `undo:` / `redo:` / `selectAll:` selectors keep routing into the focused
//! WKWebView unchanged (ADR-0115 §6).
//!
//! Worklist 2.7 expanded the menu bar to a six-submenu layout —
//! Tolaria / File / Edit / View / Window / Help — that matches the
//! standard macOS app convention so muscle-memory for File→Save,
//! View→Toggle Sidebar, and Help→About lands without surprises.
//! Stub-but-wired items (Open Vault…, Zoom In/Out, View Docs, Report
//! Issue) dispatch through log-only handlers in `main.rs` and carry a
//! `TODO(worklist-2.7)` comment beside their action declaration in
//! `crates/actions/src/lib.rs`.

use actions::{
    About, CloseTab, CloseWindow, EditCopy, EditCut, EditPaste, EditRedo, EditSelectAll, EditUndo,
    NewNote, OpenSettings, OpenVault, Quit, ReportIssue, ResetZoom, Save, ToggleElementInspector,
    ToggleInspector, ToggleSidebar, ViewDocs, ZoomIn, ZoomOut,
};
use gpui::{Menu, MenuItem, OsAction};

/// Snapshot of the workspace state the menu labels depend on.
///
/// Worklist 3.2 — the View menu's toggle entries pick their label from
/// the current sidebar / properties / inspector-overlay state
/// (`"Show Sidebar"` vs `"Hide Sidebar"`, `"Show Properties"` vs
/// `"Hide Properties"`, `"Show Inspector"` vs `"Hide Inspector"`)
/// instead of the static `"Toggle …"` verbs.  Passed by value because
/// every field is `bool` and the struct lives only for the duration of
/// one `cx.set_menus(...)` call — `MenuState` is intentionally not a
/// `gpui::Global`; the workspace owns sidebar / right-dock state and
/// `Window::is_inspector_picking` owns the dev-overlay axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuState {
    /// Whether the workspace's left dock (sidebar) is currently open.
    pub sidebar_open: bool,
    /// Whether the product [`inspector_panel::InspectorPanel`] is
    /// currently visible in the workspace's right dock.  Worklist
    /// 9.2.13 — the View → Properties entry toggles the panel via
    /// `actions::ToggleInspector` (the action verb kept its legacy
    /// name; the menu label now reads `Properties` per the panel's
    /// own title).  Computed by `main.rs::rebuild_menus_with_workspace`
    /// as `right_dock_panel_key() == Some("inspector") && is_right_dock_open()`.
    pub properties_open: bool,
    /// Whether GPUI's element-picker debug overlay is currently
    /// picking.  Worklist 9.2.15 — restored as a separate
    /// `Show / Hide Inspector` entry driven by `actions::ToggleElementInspector`
    /// (`Cmd+Alt+I`).  Sourced from `Window::is_inspector_picking`,
    /// which returns `true` during the mouse-pick step (a strict
    /// subset of "overlay is visible") — accurate for the labelling
    /// purpose because the overlay is only useful while picking.
    /// Debug-only in practice: the action is a no-op in release
    /// builds, so the label simply stays `Show Inspector` there.
    pub inspector_overlay_picking: bool,
}

/// Build the application menu bar.
///
/// Call via `cx.set_menus(app_menus(MenuState::default()))` before the
/// first window opens so AppKit picks up the accelerators immediately;
/// then re-call with a fresh [`MenuState`] from the action handlers
/// whenever sidebar / inspector state changes (worklist 3.2).
pub fn app_menus(state: MenuState) -> Vec<Menu> {
    vec![
        app_menu(),
        file_menu(),
        edit_menu(),
        view_menu(state),
        window_menu(),
        help_menu(),
    ]
}

/// Standard macOS App menu — "Tolaria" — that owns About and Quit.
///
/// About sits at the top per AppKit convention; Quit lives at the
/// bottom with the standard `Cmd+Q` accelerator (bound globally in
/// `assets/default.json`).
fn app_menu() -> Menu {
    Menu {
        name: "Tolaria".into(),
        disabled: false,
        items: vec![
            MenuItem::action("About Tolaria", About),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Quit Tolaria", Quit),
        ],
    }
}

/// File menu.  Houses note / vault lifecycle plus the standard "Close
/// Window" exit affordance.  `Close Tab` keeps `Cmd+W` (mirroring the
/// browser-tab convention used inside the workspace); `Close Window`
/// gets `Cmd+Shift+W` so both verbs are reachable without ambiguity.
fn file_menu() -> Menu {
    Menu {
        name: "File".into(),
        disabled: false,
        items: vec![
            MenuItem::action("New Note", NewNote),
            MenuItem::separator(),
            MenuItem::action("Open Vault…", OpenVault),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::separator(),
            MenuItem::action("Close Tab", CloseTab),
            MenuItem::action("Close Window", CloseWindow),
        ],
    }
}

/// Edit menu — wires AppKit's standard selectors so the focused
/// WKWebView keeps receiving cut/copy/paste/undo/redo/selectAll
/// directly (ADR-0115 §6).  Untouched by worklist 2.7.
fn edit_menu() -> Menu {
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
    }
}

/// View menu — sidebar / properties / inspector toggles plus the
/// standard zoom-in / zoom-out / reset-zoom triplet.  Zoom commands
/// are stub-but-wired (Phase 9.x will implement workspace font-size
/// scaling); the menu entries are in place so the muscle-memory
/// accelerators behave the same as any other macOS editor.
///
/// Worklist 3.2 — the sidebar / properties entries flip between
/// `"Show …"` and `"Hide …"` based on [`MenuState`] so the menu
/// reflects the current visibility instead of the static
/// `"Toggle …"` verb.  Worklist 9.2.15 — the legacy `Show Inspector`
/// entry split in two: the product-panel toggle (`actions::ToggleInspector`)
/// now reads `Show / Hide Properties` (matching the panel's own
/// `Properties` title) and a separate `Show / Hide Inspector` entry
/// dispatches `actions::ToggleElementInspector` (`Cmd+Alt+I`) for the
/// GPUI element-picker debug overlay.  Both labels rebuild via the
/// trigger points in `main.rs`: the sidebar / inspector / element-overlay
/// action handlers all call `rebuild_menus` so the label tracks state
/// across every dispatch vector (menu click, Cmd accelerator,
/// title-bar / note-toolbar buttons).  The overlay axis is a proxy
/// over `Window::is_inspector_picking`; see
/// [`MenuState::inspector_overlay_picking`] for the caveat about
/// picking-vs-overlay-open state.
fn view_menu(state: MenuState) -> Menu {
    let sidebar_label = if state.sidebar_open {
        "Hide Sidebar"
    } else {
        "Show Sidebar"
    };
    let properties_label = if state.properties_open {
        "Hide Properties"
    } else {
        "Show Properties"
    };
    let inspector_label = if state.inspector_overlay_picking {
        "Hide Inspector"
    } else {
        "Show Inspector"
    };
    Menu {
        name: "View".into(),
        disabled: false,
        items: vec![
            MenuItem::action(sidebar_label, ToggleSidebar),
            MenuItem::action(properties_label, ToggleInspector),
            MenuItem::separator(),
            MenuItem::action(inspector_label, ToggleElementInspector),
            MenuItem::separator(),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Actual Size", ResetZoom),
        ],
    }
}

/// Window menu — kept narrow (just `Close Window`) now that the tab
/// / sidebar / inspector verbs have moved to File and View.  Leaving
/// the menu in place preserves the macOS "Window" slot AppKit
/// expects between View and Help.
fn window_menu() -> Menu {
    Menu {
        name: "Window".into(),
        disabled: false,
        items: vec![MenuItem::action("Close Window", CloseWindow)],
    }
}

/// Help menu — About reuses the standard AppKit panel via the same
/// `About` action surfaced under the Tolaria menu; the documentation
/// and issue-tracker links land as stub-but-wired log handlers
/// (Phase 9.x replaces them with `open::that(...)` once the docs
/// site and issue tracker have stable URLs).
fn help_menu() -> Menu {
    Menu {
        name: "Help".into(),
        disabled: false,
        items: vec![
            MenuItem::action("Tolaria Documentation", ViewDocs),
            MenuItem::action("Report Issue…", ReportIssue),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity-check the menu skeleton: six top-level menus matching the
    /// macOS convention App / File / Edit / View / Window / Help.  Each
    /// helper owns its own item-count assertion below so regressions
    /// pinpoint the affected submenu.
    #[test]
    fn app_menus_lists_app_file_edit_view_window_help() {
        let menus = app_menus(MenuState::default());
        let names: Vec<_> = menus.iter().map(|m| m.name.to_string()).collect();
        assert_eq!(
            names,
            vec!["Tolaria", "File", "Edit", "View", "Window", "Help"]
        );
    }

    use ItemKind::{Action, Separator};

    /// Expected shape of a menu item — single source of truth for
    /// per-test schema assertions.  See [`assert_menu_schema`].
    #[derive(Debug)]
    enum ItemKind {
        Separator,
        Action(&'static str),
    }

    /// Assert the menu's name and item layout match `expected` exactly.
    /// Length and per-position kind/name are checked together so a
    /// reorder failure points at the divergent slot, not at a stale
    /// count assertion further up.
    #[track_caller]
    fn assert_menu_schema(menu: &Menu, expected_name: &str, expected: &[ItemKind]) {
        assert_eq!(menu.name.as_ref(), expected_name);
        assert_eq!(
            menu.items.len(),
            expected.len(),
            "{expected_name} item count"
        );
        for (i, (actual, want)) in menu.items.iter().zip(expected).enumerate() {
            match (actual, want) {
                (MenuItem::Separator, ItemKind::Separator) => {}
                (MenuItem::Action { name, .. }, ItemKind::Action(label)) => {
                    assert_eq!(name.as_ref(), *label, "{expected_name}[{i}]");
                }
                _ => {
                    let actual_kind = match actual {
                        MenuItem::Separator => "separator",
                        MenuItem::Action { .. } => "action",
                        MenuItem::Submenu(_) => "submenu",
                        _ => "other",
                    };
                    panic!("{expected_name}[{i}]: mismatch (want {want:?}, got {actual_kind})");
                }
            }
        }
    }

    /// App menu: About / sep / Settings / sep / Quit.
    #[test]
    fn app_menu_holds_about_settings_quit() {
        assert_menu_schema(
            &app_menu(),
            "Tolaria",
            &[
                Action("About Tolaria"),
                Separator,
                Action("Settings…"),
                Separator,
                Action("Quit Tolaria"),
            ],
        );
    }

    /// File menu: New / sep / Open / sep / Save / sep / Close Tab / Close Window.
    #[test]
    fn file_menu_holds_new_open_save_close() {
        assert_menu_schema(
            &file_menu(),
            "File",
            &[
                Action("New Note"),
                Separator,
                Action("Open Vault…"),
                Separator,
                Action("Save"),
                Separator,
                Action("Close Tab"),
                Action("Close Window"),
            ],
        );
    }

    /// Edit menu unchanged from Phase 1: 8 entries.  Names are
    /// AppKit-provided so we only pin the count here.
    #[test]
    fn edit_menu_holds_standard_os_actions() {
        let menu = edit_menu();
        assert_eq!(menu.name.as_ref(), "Edit");
        assert_eq!(menu.items.len(), 8);
    }

    /// View menu (default state — every axis closed): "Show Sidebar"
    /// / "Show Properties" / "Show Inspector" / sep / ZoomIn / ZoomOut
    /// / Actual Size.  Worklist 3.2 made the toggle labels
    /// state-driven; worklist 9.2.15 split the legacy Inspector entry
    /// into the product-panel `Properties` toggle and the GPUI
    /// element-overlay `Inspector` toggle, so the default state now
    /// pins three independent "Show …" labels.
    #[test]
    fn view_menu_shows_show_labels_when_state_closed() {
        assert_menu_schema(
            &view_menu(MenuState::default()),
            "View",
            &[
                Action("Show Sidebar"),
                Action("Show Properties"),
                Separator,
                Action("Show Inspector"),
                Separator,
                Action("Zoom In"),
                Action("Zoom Out"),
                Action("Actual Size"),
            ],
        );
    }

    /// View menu (every axis open): labels flip to "Hide Sidebar" /
    /// "Hide Properties" / "Hide Inspector".  Worklist 3.2 — the menu
    /// rebuild driven from the action handlers in `main.rs` keeps the
    /// sidebar and properties labels in sync with the workspace's
    /// dock state; worklist 9.2.15 adds the third entry whose label
    /// follows `Window::is_inspector_picking`.
    #[test]
    fn view_menu_shows_hide_labels_when_state_open() {
        assert_menu_schema(
            &view_menu(MenuState {
                sidebar_open: true,
                properties_open: true,
                inspector_overlay_picking: true,
            }),
            "View",
            &[
                Action("Hide Sidebar"),
                Action("Hide Properties"),
                Separator,
                Action("Hide Inspector"),
                Separator,
                Action("Zoom In"),
                Action("Zoom Out"),
                Action("Actual Size"),
            ],
        );
    }

    /// Mixed state: sidebar open, properties closed, overlay closed.
    /// Guards the independence of the three labels — flipping one
    /// must not bleed into the others.
    #[test]
    fn view_menu_labels_track_each_axis_independently() {
        let menu = view_menu(MenuState {
            sidebar_open: true,
            properties_open: false,
            inspector_overlay_picking: false,
        });
        assert_menu_schema(
            &menu,
            "View",
            &[
                Action("Hide Sidebar"),
                Action("Show Properties"),
                Separator,
                Action("Show Inspector"),
                Separator,
                Action("Zoom In"),
                Action("Zoom Out"),
                Action("Actual Size"),
            ],
        );
    }

    /// Worklist 9.2.15 — properties open, overlay closed.  Pins that
    /// the product right-dock toggle drives its own label without
    /// flipping the dev-overlay entry.
    #[test]
    fn view_menu_properties_open_does_not_flip_inspector_label() {
        let menu = view_menu(MenuState {
            sidebar_open: false,
            properties_open: true,
            inspector_overlay_picking: false,
        });
        assert_menu_schema(
            &menu,
            "View",
            &[
                Action("Show Sidebar"),
                Action("Hide Properties"),
                Separator,
                Action("Show Inspector"),
                Separator,
                Action("Zoom In"),
                Action("Zoom Out"),
                Action("Actual Size"),
            ],
        );
    }

    /// Worklist 9.2.15 — element-overlay picking, properties closed.
    /// Symmetric guard: the dev-overlay axis drives its own label
    /// without flipping the product right-dock entry.
    #[test]
    fn view_menu_overlay_picking_does_not_flip_properties_label() {
        let menu = view_menu(MenuState {
            sidebar_open: false,
            properties_open: false,
            inspector_overlay_picking: true,
        });
        assert_menu_schema(
            &menu,
            "View",
            &[
                Action("Show Sidebar"),
                Action("Show Properties"),
                Separator,
                Action("Hide Inspector"),
                Separator,
                Action("Zoom In"),
                Action("Zoom Out"),
                Action("Actual Size"),
            ],
        );
    }

    /// Worklist 9.2.15 — pin the action dispatched by each View-menu
    /// entry.  Schema tests above match by label, which would still
    /// pass if a future refactor accidentally swapped the Properties
    /// and Inspector entries' actions (both labels survive); this
    /// test catches that by extracting the boxed action's
    /// [`gpui::Action::name`] and asserting each slot's verb.
    ///
    /// Positional check: separators participate in the comparison so a
    /// reorder that nudges the separator up or down also trips the
    /// assertion — the schema tests above match positionally, but they
    /// match by *label*; this test matches by *action verb*, and the
    /// two together pin both axes.
    #[test]
    fn view_menu_pins_action_per_entry() {
        use gpui::Action as _;
        let menu = view_menu(MenuState::default());
        let action_names: Vec<Option<&str>> = menu
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Action { action, .. } => Some(action.name()),
                _ => None,
            })
            .collect();
        assert_eq!(
            action_names,
            vec![
                Some(ToggleSidebar.name()),
                Some(ToggleInspector.name()),
                None, // separator (between product toggles and dev overlay)
                Some(ToggleElementInspector.name()),
                None, // separator
                Some(ZoomIn.name()),
                Some(ZoomOut.name()),
                Some(ResetZoom.name()),
            ],
        );
    }

    /// Window menu trimmed to just Close Window now that the tab /
    /// sidebar / inspector verbs live under File and View.
    #[test]
    fn window_menu_holds_close_window() {
        assert_menu_schema(&window_menu(), "Window", &[Action("Close Window")]);
    }

    /// Help menu: View Docs / Report Issue.  About intentionally
    /// lives only under the Tolaria menu per AppKit convention.
    #[test]
    fn help_menu_holds_docs_and_report_issue() {
        assert_menu_schema(
            &help_menu(),
            "Help",
            &[Action("Tolaria Documentation"), Action("Report Issue…")],
        );
    }

    /// Tiny assertion helper — panics with a clear message when the
    /// matched item is a separator/submenu instead of the expected
    /// labelled action.  Kept for any future call site that wants to
    /// assert a single item in isolation.
    #[allow(dead_code)]
    #[track_caller]
    fn assert_action_named(item: &MenuItem, expected: &str) {
        match item {
            MenuItem::Action { name, .. } => assert_eq!(name.as_ref(), expected),
            MenuItem::Separator => panic!("expected action {expected:?}, got separator"),
            MenuItem::Submenu(_) => panic!("expected action {expected:?}, got submenu"),
            MenuItem::SystemMenu(_) => panic!("expected action {expected:?}, got system menu"),
        }
    }
}
