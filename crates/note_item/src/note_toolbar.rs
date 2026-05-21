//! Per-note toolbar row (ADR-0115 Phase 7, visual-issue #019).
//!
//! Mirrors `src/components/BreadcrumbBar.tsx` from the React tree — a
//! `NOTE_TOOLBAR_HEIGHT_PT`-tall strip pinned above the embedded
//! WKWebView with two clusters:
//!
//! - **Left** — breadcrumb (type label · `›` · filename stem · sync
//!   glyph).
//! - **Right** — 11 action cells matching React's `BreadcrumbActions`
//!   order: favourite, organised, neighbourhood, raw mode, note width,
//!   AI, table of contents, reveal in Finder, copy path, more, toggle
//!   inspector.
//!
//! Height is pinned to `NOTE_TOOLBAR_HEIGHT_PT` (52 pt) so the strip
//! aligns row-for-row with the `note_list_pane` header to its left.
//!
//! Every cell is a log-only stub today; wiring to real actions lands
//! alongside the Phase 8 modal-chrome work (the React `onToggle*`
//! callbacks need their GPUI counterparts first).  Cells are
//! `id()`-tagged + `dump_as`-registered so periscope can target them
//! by name once the actions land.

use std::path::Path;

use gpui::{
    div, px, AnyElement, App, ClipboardItem, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, tooltip::Tooltip, ActiveTheme, IconName};
use ui::tree_dump::DumpAsExt as _;
use vault::{NoteId, Vault};

/// Height of the note toolbar strip, in logical points.
///
/// Pinned to React's `.breadcrumb-bar { height: 52px }`
/// (`src/components/BreadcrumbBar.tsx:1061`) and matched to the
/// `note_list_pane` header strip (`crates/note_list_pane/src/lib.rs`)
/// so the two land on the same baseline.
pub const NOTE_TOOLBAR_HEIGHT_PT: f32 = 52.0;

/// Render the toolbar row for a single note.
///
/// `id` is the vault id of the note (used by the star + organized
/// cells to dispatch the frontmatter toggle).  `path` is the on-disk
/// path; the breadcrumb extracts the type label from its filename
/// prefix and uses the file stem as the trailing segment.  `raw_mode`
/// is the active item's chrome-owned raw-mode flag — drives the
/// active-state treatment on the raw cell (worklist 9.2.4).
///
/// The cell-state read (`favorite` / `organized`) goes through the
/// installed [`Vault`] global; absent vault (mock-mode + chrome tests
/// without a real vault) renders the cells in their "off" state and
/// the click handlers become log-only.
pub(crate) fn render(id: NoteId, path: &Path, raw_mode: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let bg = theme.background;
    let border = theme.border;
    let fg = theme.foreground;
    let muted = theme.muted_foreground;

    let (is_favorite, is_organized) = cx
        .try_global::<Vault>()
        .and_then(|v| {
            // `try_global` returns a borrow of the vault; the note
            // lookup is cheap (HashMap of id → Note) and we only need
            // the two booleans.
            v.note_sync(id)
                .map(|note| (note.is_favorite(), note.is_organized()))
        })
        .unwrap_or((false, false));

    let stem: SharedString = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(SharedString::from)
        .unwrap_or_default();
    let type_label = SharedString::new_static(type_label_singular(path));

    let breadcrumb = h_flex()
        .items_center()
        .gap(px(6.0))
        .text_sm()
        .text_color(muted)
        .child(
            div()
                .id("note-toolbar-type")
                .child(type_label)
                .tooltip(|window, cx| Tooltip::new("Note type — click to change").build(window, cx))
                .dump_as("note-toolbar-type"),
        )
        .child(div().text_color(muted).child(IconName::ChevronRight))
        .child(
            div()
                .text_color(fg)
                .child(stem.clone())
                .dump_as("note-toolbar-filename"),
        )
        .child(
            div()
                .id("note-toolbar-sync")
                .cursor_pointer()
                .text_color(muted)
                .hover(|this| this.text_color(fg))
                // React's `BreadcrumbBar` uses Phosphor `ArrowsClockwise` for the
                // sync glyph.  gpui-component's pack has no clockwise icon;
                // `Redo` (Lucide's curving arrow) is the closest visual match
                // — single curved stroke matching the React reference rather
                // than the two-straight-arrows shape of `IconName::Replace`.
                .child(IconName::Undo)
                .tooltip(|window, cx| Tooltip::new("Sync status").build(window, cx))
                .dump_as("note-toolbar-sync")
                .into_any_element(),
        );

    // Action cluster — mirrors `BreadcrumbActions` (BreadcrumbBar.tsx
    // L811-890) left-to-right, plus the trailing inspector toggle
    // (L987-994).  Star / organized (worklist 9.2.1 / 9.2.2) dispatch
    // through `Vault::set_frontmatter_bool`; reveal / copy-path /
    // inspector (2.15 / 2.16 / 2.18) dispatch real handlers; the
    // remaining cells keep a log-only stub until their Phase 9 rows
    // ship.
    let reveal_path = path.to_path_buf();
    let copy_path = path.to_path_buf();
    // Filled-vs-outline star glyph mirrors the React `FavoriteAction`
    // shape: outline when off, filled when on.  `IconName::StarFill`
    // is the closest available pair from `gpui-component-assets`.
    let star_icon = if is_favorite {
        IconName::StarFill
    } else {
        IconName::Star
    };
    let star_tooltip = if is_favorite {
        "Unstar this note"
    } else {
        "Star this note"
    };
    let organized_tooltip = if is_organized {
        "Mark as unorganized"
    } else {
        "Mark as organized"
    };
    let actions = h_flex()
        .items_center()
        .gap(px(4.0))
        .text_color(muted)
        .child(toolbar_cell(
            "note-toolbar-star",
            star_icon,
            star_tooltip,
            move |_window, cx| toggle_frontmatter_flag(id, "_favorite", !is_favorite, cx),
        ))
        // The React tooltip says "Show in Organized view", but the
        // underlying handler is a pure `_organized` frontmatter
        // toggle, not a navigation action.  TODO(9.2.2-followup): the
        // inbox-advance behaviour driven by
        // `useInboxOrganizeAdvance` is gated on
        // `settings_store::explicit_organization_enabled`; revisit
        // when the setting lands.
        .child(toolbar_cell(
            "note-toolbar-organized",
            IconName::CircleCheck,
            organized_tooltip,
            move |_window, cx| toggle_frontmatter_flag(id, "_organized", !is_organized, cx),
        ))
        .child(stub_cell(
            "note-toolbar-neighborhood",
            IconName::Map,
            "Show neighborhood graph",
        ))
        // Worklist 9.2.4 — clicking dispatches `ToggleRawEditor`; the
        // chrome-side handler resolves the active `NoteItem` and flips
        // `raw_mode`, which pushes `ToHost::SetRawMode` down to the
        // embedded editor.  Active-state treatment paints the cell
        // background filled when raw is on — `gpui-component-assets`
        // has no fill/outline pair for `SquareTerminal`, so the
        // contrast lives in the cell chrome rather than the glyph.
        // TODO(visual-parity): adopt a true fill/outline pair when the
        // upstream icon set gains one (or commission a `square-terminal-fill`).
        .child(toolbar_cell_with_active(
            "note-toolbar-raw",
            IconName::SquareTerminal,
            if raw_mode {
                "Show rich editor"
            } else {
                "Show raw markdown"
            },
            raw_mode,
            |_window, cx| {
                cx.dispatch_action(&actions::ToggleRawEditor);
                log::info!(
                    target: "note_item::toolbar",
                    "raw: dispatched ToggleRawEditor"
                );
            },
        ))
        .child(stub_cell(
            "note-toolbar-width",
            IconName::Maximize,
            "Toggle note width",
        ))
        .child(stub_cell(
            "note-toolbar-ai",
            IconName::Asterisk,
            "Open AI assistant",
        ))
        .child(stub_cell(
            "note-toolbar-toc",
            IconName::Menu,
            "Table of contents",
        ))
        .child(toolbar_cell(
            "note-toolbar-reveal",
            IconName::FolderOpen,
            "Reveal in Finder",
            move |_window, _cx| reveal_in_finder(&reveal_path),
        ))
        .child(toolbar_cell(
            "note-toolbar-copy-path",
            IconName::Copy,
            "Copy note path",
            move |_window, cx| copy_path_to_clipboard(&copy_path, cx),
        ))
        .child(stub_cell(
            "note-toolbar-more",
            IconName::Ellipsis,
            "More actions",
        ))
        .child(toolbar_cell(
            "note-toolbar-inspector",
            IconName::PanelRight,
            "Toggle inspector",
            |_window, cx| {
                cx.dispatch_action(&actions::ToggleInspector);
                log::info!(
                    target: "note_item::toolbar",
                    "inspector: dispatched ToggleInspector"
                );
            },
        ));

    h_flex()
        .h(px(NOTE_TOOLBAR_HEIGHT_PT))
        .min_h(px(NOTE_TOOLBAR_HEIGHT_PT))
        .items_center()
        .justify_between()
        .px(px(16.0))
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .child(breadcrumb)
        .child(actions)
        .dump_as("note-toolbar")
        .into_any_element()
}

/// One toolbar action cell — square click target with a single
/// [`IconName`] glyph centred inside.  `tooltip` is the verb-noun
/// label shown on hover (worklist 2.4); `on_click` is invoked when
/// the cell is clicked.
///
/// Single source of truth for the cell's visual chain (size, hover
/// background, `dump_as`, `tooltip`) — see [`stub_cell`] for the
/// log-only default used by the seven still-unwired cells.  Wraps
/// [`toolbar_cell_with_active`] in the `active = false` case so the
/// active-state treatment is opt-in.
fn toolbar_cell(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    toolbar_cell_with_active(id, icon, tooltip, false, on_click)
}

/// [`toolbar_cell`] with an explicit `active` flag — when `true`, the
/// cell paints a baseline tinted background so the user sees the
/// toggle's state without having to hover.  Used by the raw-mode
/// toggle (worklist 9.2.4) where the chrome-side `raw_mode` flag
/// drives the visual.
fn toolbar_cell_with_active(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    active: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    use gpui::prelude::FluentBuilder as _;
    // Baseline tint matches the hover overlay so an active cell looks
    // exactly like a hovered one even when the cursor is elsewhere —
    // no new colour token required, and the chrome's existing dark /
    // light theming carries through.
    let active_bg = gpui::hsla(0.0, 0.0, 0.5, 0.12);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .w(px(24.0))
        .rounded_sm()
        .cursor_pointer()
        .when(active, move |this| this.bg(active_bg))
        .hover(|this| this.bg(gpui::hsla(0.0, 0.0, 0.5, 0.12)))
        .on_click(move |_, window, cx| on_click(window, cx))
        .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
        .child(icon)
        .dump_as(id)
        .into_any_element()
}

/// Convenience wrapper around [`toolbar_cell`] for cells whose
/// behaviour hasn't been wired yet — logs the stub message on click,
/// matching the previous helper's body.  Used by the seven worklist
/// rows (2.9-2.14, 2.17) that need real product work (frontmatter
/// mutation, new panels, AI integration) before they can dispatch.
fn stub_cell(id: &'static str, icon: IconName, tooltip: &'static str) -> AnyElement {
    toolbar_cell(id, icon, tooltip, move |_, _| {
        log::info!("note toolbar action stub: {id}");
    })
}

/// Reveal-in-Finder handler for the `note-toolbar-reveal` cell
/// (worklist 2.15).  Spawns `open -R <path>` so Finder selects the
/// note in its containing folder — `open -R` is the macOS-idiomatic
/// "select" verb, distinct from `open <path>` which would open the
/// note in its default application.  We discard the [`Child`] handle
/// rather than waiting on it; Finder owns user feedback from here.
///
/// [`Child`]: std::process::Child
fn reveal_in_finder(path: &Path) {
    let path_str = path.to_string_lossy();
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("open")
            .args(["-R", path_str.as_ref()])
            .spawn()
        {
            Ok(_) => log::info!(
                target: "note_item::toolbar",
                "reveal: spawned open -R {path_str}"
            ),
            Err(e) => log::warn!(
                target: "note_item::toolbar",
                "reveal: open -R failed: {e:#}"
            ),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        log::warn!(
            target: "note_item::toolbar",
            "reveal: select-in-file-manager not yet implemented on this platform ({path_str})"
        );
    }
}

/// Copy-path handler for the `note-toolbar-copy-path` cell (worklist
/// 2.16).  Writes the note's absolute path to the system clipboard
/// via [`App::write_to_clipboard`] — no toast required because the
/// user confirms the copy by pasting elsewhere.
fn copy_path_to_clipboard(path: &Path, cx: &App) {
    let path_str = path.to_string_lossy().into_owned();
    log::info!(target: "note_item::toolbar", "copy-path: copied {path_str}");
    cx.write_to_clipboard(ClipboardItem::new_string(path_str));
}

/// Dispatch a `_favorite` / `_organized` toggle to the installed
/// [`Vault`] global.  Shared by the star (worklist 9.2.1) and
/// organized (worklist 9.2.2) cells — both call
/// `vault.set_frontmatter_bool(...).detach()`, so factoring it out
/// keeps the click closures one-liners and centralises the
/// "no-vault-installed" guard.
///
/// `detach()` matches the existing `Vault::save` dispatch pattern in
/// `note_item::install_dispatch_task` (lib.rs L595): the returned
/// `Task` is observed via the next render, not awaited inline.
fn toggle_frontmatter_flag(id: NoteId, key: &str, value: bool, cx: &mut App) {
    if !cx.has_global::<Vault>() {
        log::warn!(
            target: "note_item::toolbar",
            "{key} toggle ignored: no Vault global installed (note id={id:?})",
        );
        return;
    }
    cx.global_mut::<Vault>()
        .set_frontmatter_bool(id, key, value)
        .detach();
    log::info!(
        target: "note_item::toolbar",
        "{key} toggle dispatched: id={id:?} value={value}",
    );
}

/// Singular type label for the breadcrumb (`procedure-foo.md` →
/// `"Procedure"`).
///
/// Sibling to `sidebar_panel::type_label_for`, which returns the
/// *plural* form used as a sidebar section header.  Duplicated here
/// rather than pulled across crates because the heuristic is two
/// lines and the singular/plural variants tend to drift independently
/// (e.g. "Person" vs. "People").
fn type_label_singular(path: &Path) -> &'static str {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let prefix = stem
        .split_once('-')
        .map(|(p, _)| p)
        .unwrap_or(stem)
        .to_ascii_lowercase();
    match prefix.as_str() {
        "area" => "Area",
        "event" => "Event",
        "measure" => "Measure",
        "person" => "Person",
        "procedure" => "Procedure",
        "responsibility" => "Responsibility",
        "topic" => "Topic",
        "project" => "Project",
        "quarter" => "Quarter",
        _ => "Note",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn type_label_singular_extracts_known_prefixes() {
        let cases: &[(&str, &str)] = &[
            ("procedure-quarterly-sponsor-outreach.md", "Procedure"),
            ("area-building.md", "Area"),
            ("event-team-sync.md", "Event"),
            ("measure-revenue.md", "Measure"),
            ("person-alice.md", "Person"),
            ("responsibility-sponsorships.md", "Responsibility"),
            ("topic-product.md", "Topic"),
            ("project-tolaria.md", "Project"),
            ("quarter-2026q1.md", "Quarter"),
            ("untitled.md", "Note"),
            ("readme.md", "Note"),
        ];
        for (input, expected) in cases {
            let label = type_label_singular(&PathBuf::from(input));
            assert_eq!(label, *expected, "input={input}");
        }
    }

    #[test]
    fn toolbar_cell_builds_reveal_button() {
        // Worklist 2.15 — the reveal cell must construct successfully
        // with a non-stub handler.  Click behaviour goes through `open
        // -R`, which we can't drive headlessly, so we only assert the
        // builder returns an element.
        let _cell = toolbar_cell(
            "note-toolbar-reveal",
            IconName::FolderOpen,
            "Reveal in Finder",
            |_window, _cx| {},
        );
    }

    #[test]
    fn toolbar_cell_builds_copy_path_button() {
        // Worklist 2.16 — same shape assertion as the reveal cell.
        // The clipboard write needs a live `App`, so the handler body
        // here is a no-op; the wired call site in `render` passes the
        // real `copy_path_to_clipboard` invocation.
        let _cell = toolbar_cell(
            "note-toolbar-copy-path",
            IconName::Copy,
            "Copy note path",
            |_window, _cx| {},
        );
    }

    #[test]
    fn toolbar_cell_builds_inspector_button() {
        // Worklist 2.18 — same shape assertion as the other wired
        // cells.  The real handler dispatches `actions::ToggleInspector`
        // via `cx.dispatch_action`, which requires a live `App`.
        let _cell = toolbar_cell(
            "note-toolbar-inspector",
            IconName::PanelRight,
            "Toggle inspector",
            |_window, _cx| {},
        );
    }

    #[test]
    fn toolbar_height_matches_react_breadcrumb_bar() {
        // React: `.breadcrumb-bar { height: 52px }` (BreadcrumbBar.tsx:1061).
        // 52.0 is exactly representable in f32 — direct equality is sound.
        assert_eq!(NOTE_TOOLBAR_HEIGHT_PT, 52.0);
    }

    /// Worklist 9.2.1 — clicking the star cell flips the `_favorite`
    /// flag on disk and in memory through the installed `Vault`
    /// global.  Exercises the full toolbar dispatch path:
    /// `toggle_frontmatter_flag` → `Vault::set_frontmatter_bool`.
    #[gpui::test]
    fn star_cell_dispatches_favorite_toggle(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().expect("tempdir");
        let note_path = dir.path().join("n.md");
        std::fs::write(&note_path, "# heading\nbody\n").unwrap();
        let vault = vault::Vault::open_at(dir.path()).expect("open vault");
        let id = cx.update(|cx| cx.foreground_executor().block_on(vault.notes())[0]);
        cx.update(|cx| cx.set_global(vault));

        // Read current state (off), then toggle via the same helper
        // the click closure uses.  This intentionally bypasses the
        // GPUI click chain — driving `on_click` headlessly requires a
        // mounted entity + hitbox, and the dispatch behaviour is the
        // load-bearing invariant we want pinned.
        cx.update(|cx| {
            let current = cx
                .global::<vault::Vault>()
                .note_sync(id)
                .map(|n| n.is_favorite())
                .unwrap();
            assert!(!current, "fixture starts unfavoured");
            toggle_frontmatter_flag(id, "_favorite", true, cx);
        });
        cx.run_until_parked();

        // In-memory frontmatter must reflect the toggle synchronously
        // (the toolbar reads `note_sync(id).is_favorite()` on the next
        // render pass).
        cx.update(|cx| {
            let after = cx
                .global::<vault::Vault>()
                .note_sync(id)
                .map(|n| n.is_favorite())
                .unwrap();
            assert!(after, "in-memory frontmatter must mirror the toggle");
        });
        // On-disk bytes carry the new line.
        let on_disk = std::fs::read_to_string(&note_path).unwrap();
        assert!(
            on_disk.contains("_favorite: true"),
            "disk write must include the new line; got: {on_disk:?}",
        );
    }

    /// Worklist 9.2.2 — the organized cell shares the same dispatch
    /// path as the star cell but writes the `_organized` key.  Pin the
    /// key separately so a typo there can't slip past star coverage.
    #[gpui::test]
    fn organized_cell_dispatches_organized_toggle(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().expect("tempdir");
        let note_path = dir.path().join("n.md");
        std::fs::write(&note_path, "---\ntype: Note\n---\nbody\n").unwrap();
        let vault = vault::Vault::open_at(dir.path()).expect("open vault");
        let id = cx.update(|cx| cx.foreground_executor().block_on(vault.notes())[0]);
        cx.update(|cx| cx.set_global(vault));

        cx.update(|cx| {
            toggle_frontmatter_flag(id, "_organized", true, cx);
        });
        cx.run_until_parked();

        cx.update(|cx| {
            let after = cx
                .global::<vault::Vault>()
                .note_sync(id)
                .map(|n| n.is_organized())
                .unwrap();
            assert!(after, "organized toggle must mutate the in-memory map");
        });
        let on_disk = std::fs::read_to_string(&note_path).unwrap();
        assert_eq!(
            on_disk, "---\ntype: Note\n_organized: true\n---\nbody\n",
            "organized cell must write the `_organized` key, not anything else",
        );
    }

    /// Worklist 9.2.1 / 9.2.2 — the toggle helper must degrade
    /// gracefully when no `Vault` global is installed (mock-mode
    /// chrome tests, periscope smokes).  The click stays log-only and
    /// does NOT panic.
    #[gpui::test]
    fn toggle_helper_is_a_noop_without_vault_global(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            // Sanity: no `Vault` global is installed by default.
            assert!(!cx.has_global::<vault::Vault>());
            toggle_frontmatter_flag(NoteId::from_raw(7), "_favorite", true, cx);
        });
    }
}
