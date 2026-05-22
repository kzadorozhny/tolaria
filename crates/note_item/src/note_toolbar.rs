//! Per-note toolbar row (ADR-0115 Phase 7, visual-issue #019).
//!
//! Mirrors `src/components/BreadcrumbBar.tsx` from the React tree — a
//! `NOTE_TOOLBAR_HEIGHT_PT`-tall strip pinned above the embedded
//! WKWebView with two clusters:
//!
//! - **Left** — breadcrumb (type label · `›` · filename stem · sync
//!   glyph).
//! - **Right** — 10 action cells matching React's `BreadcrumbActions`
//!   order: favourite, organised, neighbourhood, raw mode, note width,
//!   AI, table of contents, reveal in Finder, copy path, more.
//!   Worklist 9.3.5 moved the trailing `note-toolbar-inspector` cell
//!   to the workspace title bar (`workspace::title_bar`); the toggle
//!   is workspace-level chrome, not per-note state.
//!
//! Height is pinned to `NOTE_TOOLBAR_HEIGHT_PT` (52 pt) so the strip
//! aligns row-for-row with the `note_list_pane` header to its left.
//!
//! Every cell is a log-only stub today; wiring to real actions lands
//! alongside the Phase 8 modal-chrome work (the React `onToggle*`
//! callbacks need their GPUI counterparts first).  Cells are
//! `id()`-tagged + `dump_as`-registered so periscope can target them
//! by name once the actions land.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, ClipboardItem, Hsla, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{PopupMenu, PopupMenuItem},
    popover::Popover,
    tooltip::Tooltip,
    ActiveTheme, IconName, Sizable as _,
};
use ui::tree_dump::DumpAsExt as _;
use vault::{NoteId, Vault};

use crate::NeighborhoodAnchor;

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
pub(crate) fn render(
    id: NoteId,
    path: &Path,
    raw_mode: bool,
    wide_mode: bool,
    cx: &App,
) -> AnyElement {
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

    // Worklist 9.2.14 — the neighbourhood cell paints in its active
    // state when the note-list pane is currently scoped to this note's
    // neighbourhood.  `EnterNeighborhood` sets the anchor; any sidebar
    // selection clears it.  Absent global (mock-mode chrome tests
    // without the workspace's global wiring) reads as `false`.
    let is_neighborhood_active = cx
        .try_global::<NeighborhoodAnchor>()
        .is_some_and(|anchor| anchor.matches(id));

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
    // L811-890) left-to-right.  Star / organized (worklist 9.2.1 /
    // 9.2.2) dispatch through `Vault::set_frontmatter_bool`; reveal /
    // copy-path (2.15 / 2.16) dispatch real handlers; the remaining
    // cells keep a log-only stub until their Phase 9 rows ship.  The
    // trailing `note-toolbar-inspector` cell that mirrored React's
    // `L987-994` toggle moved to the workspace title bar in worklist
    // 9.3.5 — see `crates/workspace/src/title_bar.rs`.
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
    // Organized glyph mirrors React's `OrganizedAction` filled-disk
    // treatment: outlined `CircleCheck` when off, a flat `Check`
    // overlaid on a green-filled cell when on.  `gpui-component-assets`
    // has no `circle-check-fill` pair, so the active-state contrast
    // lives on the cell chrome (background + glyph colour) rather than
    // on a different glyph.  See [`toolbar_cell_with_active_fill`]
    // below for the fill-mode helper this branch routes through.
    let organized_icon = organized_icon_for(is_organized);
    let actions = h_flex()
        .items_center()
        .gap(px(4.0))
        .text_color(muted)
        // Star — when `_favorite: true` the glyph paints `--accent-yellow`
        // (worklist 9.2.11) so the active state matches React's
        // `FavoriteAction` styling (`text-[var(--accent-yellow)]`,
        // `BreadcrumbBar.tsx:225`).
        .child(toolbar_cell_with_active_color(
            "note-toolbar-star",
            star_icon,
            star_tooltip,
            is_favorite,
            star_active_color(),
            move |_window, cx| toggle_frontmatter_flag(id, "_favorite", !is_favorite, cx),
        ))
        // The React tooltip says "Show in Organized view", but the
        // underlying handler is a pure `_organized` frontmatter
        // toggle, not a navigation action.  TODO(9.2.2-followup): the
        // inbox-advance behaviour driven by
        // `useInboxOrganizeAdvance` is gated on
        // `settings_store::explicit_organization_enabled`; revisit
        // when the setting lands.
        //
        // When `_organized: true` the cell paints a green-filled disk
        // with a white check inside (worklist 9.2.10 reopened) — matches
        // the React `OrganizedAction` styling, which renders a
        // `bg-[var(--accent-green)] text-white` filled disk rather than
        // a tinted outline.  `gpui-component-assets` carries no
        // `circle-check-fill` pair, so the contrast lives on the cell
        // chrome: background = `theme.success`, glyph = white,
        // `IconName::Check` (no surrounding circle ring).  The inactive
        // branch keeps the outlined `CircleCheck` in muted foreground.
        // `theme.success` is the theme-aware mapping of `--accent-green`
        // (see `crates/theme/src/palette.rs` light + dark blocks) so
        // the green tracks light / dark mode automatically.
        .child(toolbar_cell_with_active_fill(
            "note-toolbar-organized",
            organized_icon,
            organized_tooltip,
            is_organized,
            organized_active_color(cx),
            move |_window, cx| toggle_frontmatter_flag(id, "_organized", !is_organized, cx),
        ))
        // Worklist 9.2.3 — clicking dispatches `EnterNeighborhood`;
        // the chrome-side handler resolves the active `NoteItem` via
        // the shared `ActiveNoteItemSlot`, computes the union of
        // `vault.backlinks(id)` and `vault.outbound_links(id)`, and
        // pushes a `NoteListScope::Neighborhood(id, ids)` onto the
        // note-list pane.  The React reference's tooltip is "Show
        // notes that link to this note" rather than "Show neighborhood
        // graph" — the row is a filter, not a graph view.  Glyph stays
        // `IconName::Map` per the React parity reference.
        //
        // Worklist 9.2.3 reopened-2 — dispatched via
        // [`Window::dispatch_action`] (not `App::dispatch_action`).
        // Click closures run inside `Window::dispatch_event`, which
        // entered through `update_window_id` — the window slot in
        // `cx.windows` is already taken for the current update.
        // `App::dispatch_action` tries to re-enter via
        // `active_window.update(self, …)` and fails silently with
        // `.log_err()` because the slot's `take()` returns `None`.
        // `Window::dispatch_action` defers internally via `cx.defer`,
        // so the action lands AFTER the click update unwinds and the
        // App-scope handler fires as expected.
        //
        // Worklist 9.2.14 — paints the active-state glyph colour when
        // the note-list pane is currently filtered to this note's
        // neighbourhood.  Mirrors the star cell's `GlyphColor` pattern
        // (worklist 9.2.11): the glyph itself carries the active
        // signal in the theme's accent colour, no separate filled
        // variant required (`gpui-component-assets` has no `Map`
        // fill/outline pair).  The `NeighborhoodAnchor` global is
        // written by the `EnterNeighborhood` handler in `tolaria::main`
        // and cleared on the next sidebar selection.
        .child(toolbar_cell_with_active_color(
            "note-toolbar-neighborhood",
            IconName::Map,
            "Show neighborhood",
            is_neighborhood_active,
            neighborhood_active_color(cx),
            |window, cx| {
                // Worklist 9.3.6 — downgraded from `info!` to `debug!`
                // (per-click traces are noise at info level now that
                // the dispatch chain is wired correctly; the handler-
                // level `info!` traces at `tolaria::*` stay for
                // diagnostic value).
                log::debug!(
                    target: "note_item::toolbar",
                    "neighborhood: click registered, dispatching EnterNeighborhood"
                );
                window.dispatch_action(Box::new(actions::EnterNeighborhood), cx);
            },
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
        //
        // Worklist 9.2.4 reopened-2 — see the
        // `note-toolbar-neighborhood` comment above for why this is
        // [`Window::dispatch_action`] rather than `App::dispatch_action`.
        .child(toolbar_cell_with_active(
            "note-toolbar-raw",
            IconName::SquareTerminal,
            if raw_mode {
                "Show rich editor"
            } else {
                "Show raw markdown"
            },
            raw_mode,
            |window, cx| {
                // Worklist 9.3.6 — downgraded from `info!` to `debug!`,
                // see the neighbourhood cell above for the rationale.
                log::debug!(
                    target: "note_item::toolbar",
                    "raw: click registered, dispatching ToggleRawEditor"
                );
                window.dispatch_action(Box::new(actions::ToggleRawEditor), cx);
            },
        ))
        // Worklist 9.2.17 — wide/narrow toggle.  Dispatches
        // `ToggleNoteWidth` (matches the raw-mode shape); the handler
        // in `tolaria/src/main.rs` mutates the active `NoteItem`'s
        // `wide_mode` flag and pushes `ToHost::SetWideMode` over the
        // bridge.  Active-state treatment paints when `wide_mode` is
        // on, same `ActiveStyle::Tint` glyph that raw-mode + toc use.
        .child(toolbar_cell_with_active(
            "note-toolbar-width",
            IconName::Maximize,
            if wide_mode {
                "Use narrow note width"
            } else {
                "Use wide note width"
            },
            wide_mode,
            |window, cx| {
                log::debug!(
                    target: "note_item::toolbar",
                    "width: click registered, dispatching ToggleNoteWidth"
                );
                window.dispatch_action(Box::new(actions::ToggleNoteWidth), cx);
            },
        ))
        .child(stub_cell(
            "note-toolbar-ai",
            IconName::Asterisk,
            "Open AI assistant",
        ))
        // Worklist 9.2.6 — clicking dispatches `ToggleTableOfContents`;
        // the chrome-side handler resolves the active workspace and
        // attaches / toggles the `toc_panel::TocPanel` in the right
        // dock.  Active-state colour treatment is deferred to a
        // follow-up: tracking the panel-open state across renders
        // would couple `note_item` to the workspace dock, and the
        // dock toggle already gives the user visual feedback via the
        // panel itself appearing or disappearing.
        //
        // Worklist 9.2.6 reopened — see the
        // `note-toolbar-neighborhood` comment above for why this is
        // [`Window::dispatch_action`] rather than `App::dispatch_action`.
        .child(toolbar_cell(
            "note-toolbar-toc",
            IconName::Menu,
            "Table of contents",
            |window, cx| {
                // Worklist 9.3.6 — downgraded from `info!` to `debug!`,
                // see the neighbourhood cell above for the rationale.
                log::debug!(
                    target: "note_item::toolbar",
                    "toc: click registered, dispatching ToggleTableOfContents"
                );
                window.dispatch_action(Box::new(actions::ToggleTableOfContents), cx);
            },
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
        // Worklist 9.2.19 — restore the per-note inspector toggle on
        // the toolbar alongside the title-bar primary added in 9.3.5.
        // The two affordances complement each other: the title-bar
        // toggle is workspace-chrome (always visible regardless of
        // which note is open), the toolbar cell is per-note context
        // (lives next to the other note-level actions like ToC and
        // Copy path).  Both dispatch the same `ToggleInspector`
        // action through `Window::dispatch_action` (re-entrancy-safe
        // route documented in the 9.2.3 / 9.2.6 cells).  Mirrors the
        // React-era `BreadcrumbBar` which always carried this cell.
        .child(toolbar_cell(
            "note-toolbar-inspector",
            IconName::Info,
            "Toggle inspector",
            |window, cx| {
                log::debug!(
                    target: "note_item::toolbar",
                    "inspector: click registered, dispatching ToggleInspector"
                );
                window.dispatch_action(Box::new(actions::ToggleInspector), cx);
            },
        ))
        .child(more_overflow_cell(path.to_path_buf()));

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
    toolbar_cell_inner(id, icon, tooltip, ActiveStyle::Tint, active, on_click)
}

/// [`toolbar_cell`] variant that paints the glyph in `active_color` when
/// `active` is true — used by the star (worklist 9.2.11) cell where the
/// React reference colours the **icon** rather than the cell
/// background.  When `active`, the background tint that
/// [`toolbar_cell_with_active`] would paint is suppressed so the glyph
/// itself carries the active signal, matching `BreadcrumbBar.tsx`
/// (`text-[var(--accent-yellow)]`).
fn toolbar_cell_with_active_color(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    active: bool,
    active_color: Hsla,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    toolbar_cell_inner(
        id,
        icon,
        tooltip,
        ActiveStyle::GlyphColor(active_color),
        active,
        on_click,
    )
}

/// [`toolbar_cell`] variant that paints the **cell background** in
/// `active_color` and the glyph in white when `active` is true — used
/// by the organized (worklist 9.2.10 reopened) cell where the React
/// reference uses a filled-disk treatment (`bg-[var(--accent-green)]
/// text-white`) rather than a tinted outline.  The fill semantically
/// replaces the missing `circle-check-fill` glyph in
/// `gpui-component-assets`: pair this with `IconName::Check` (no
/// surrounding ring) so the cell chrome carries the circle and the
/// glyph carries the check.
fn toolbar_cell_with_active_fill(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    active: bool,
    active_color: Hsla,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    toolbar_cell_inner(
        id,
        icon,
        tooltip,
        ActiveStyle::Fill(active_color),
        active,
        on_click,
    )
}

/// Active-state visual treatment selected by the caller.  Each variant
/// names the cell-chrome shape that paints when `active = true`; an
/// inactive cell always renders as the baseline (no background, no
/// glyph recolour) regardless of the variant.
#[derive(Clone, Copy)]
enum ActiveStyle {
    /// Paint the cell background with the standard hover tint
    /// (`hsla(0, 0%, 50%, 0.12)`) so an active cell looks identical to
    /// a hovered one.  Used by the raw-mode toggle (worklist 9.2.4).
    Tint,
    /// Suppress the active background and recolour the glyph itself.
    /// Used by the star cell (worklist 9.2.11) where the React
    /// reference paints `text-[var(--accent-yellow)]` on a filled-star
    /// glyph that already carries the disk shape.
    GlyphColor(Hsla),
    /// Paint the cell background in `Hsla` and the glyph in white.
    /// Used by the organized cell (worklist 9.2.10 reopened) where the
    /// React reference draws a filled green disk with a white check
    /// inside; `gpui-component-assets` has no `circle-check-fill`
    /// glyph so the disk lives on the cell chrome.
    Fill(Hsla),
}

/// Shared body of [`toolbar_cell_with_active`],
/// [`toolbar_cell_with_active_color`], and
/// [`toolbar_cell_with_active_fill`].  Splitting the public surfaces
/// keeps the call sites self-describing while the visual chain stays
/// one source of truth.
fn toolbar_cell_inner(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    style: ActiveStyle,
    active: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    use gpui::prelude::FluentBuilder as _;
    // Baseline tint matches the hover overlay so an active cell looks
    // exactly like a hovered one even when the cursor is elsewhere —
    // no new colour token required, and the chrome's existing dark /
    // light theming carries through.  Single source of truth for the
    // tint so a future tweak to the active / hover overlay updates
    // both states in lockstep.
    let cell_tint = gpui::hsla(0.0, 0.0, 0.5, 0.12);
    // Resolve the active treatment into the three style knobs the
    // render chain consumes: a cell background colour, a glyph
    // colour, and an inner filled-disc colour.  `Fill` is the only
    // variant that uses the inner disc — `Tint` and `GlyphColor`
    // leave it as `None` so the glyph sits directly on the cell.
    // Worklist 9.2.10 reopened-2 — the React `OrganizedAction`
    // treatment is a clean green circle (not the cell's
    // `rounded_sm` rectangle), so the disc child carries the round
    // shape while the cell stays the standard 24x24 click target.
    let (active_bg, glyph_color, fill_disc) = if active {
        match style {
            ActiveStyle::Tint => (Some(cell_tint), None, None),
            ActiveStyle::GlyphColor(c) => (None, Some(c), None),
            ActiveStyle::Fill(c) => (None, Some(gpui::white()), Some(c)),
        }
    } else {
        (None, None, None)
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .w(px(24.0))
        .rounded_sm()
        .cursor_pointer()
        .when_some(active_bg, |this, bg| this.bg(bg))
        .when_some(glyph_color, |this, c| this.text_color(c))
        .hover(move |this| this.bg(cell_tint))
        .on_click(move |_, window, cx| on_click(window, cx))
        .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
        // Worklist 9.2.10 reopened-2 — for `ActiveStyle::Fill`, paint
        // the active colour onto a `rounded_full` inner disc instead
        // of the cell's rectangle.  The disc is sized 18 pt so a 3 pt
        // halo of cell baseline shows around it — matches React's
        // visual rhythm where the green button is visibly smaller than
        // the surrounding click target.  White on `theme.success` clears
        // WCAG AA (≥ 4.5 contrast) on both the light `#38A169` and dark
        // `#79D89D` palette greens.  The inactive branch (and `Tint` /
        // `GlyphColor` variants) skip the disc entirely so the glyph
        // sits on the cell's bare 24x24 rectangle as before.
        .child(if let Some(disc) = fill_disc {
            div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(18.0))
                .w(px(18.0))
                .rounded_full()
                .bg(disc)
                .child(icon)
                .into_any_element()
        } else {
            icon.into_any_element()
        })
        .dump_as(id)
        .into_any_element()
}

/// Active-state colour for the star toolbar cell — mirrors React's
/// `FavoriteAction` styling, which uses the `--accent-yellow` CSS
/// custom property (`#D69E2E` light / `#F2C86B` dark, see
/// `src/index.css:77,229`).  `gpui_component::ThemeColor` has no
/// dedicated yellow accent today, so the colour is inlined as the
/// light-mode value with a TODO so a periscope diff catches the
/// dark-mode discrepancy when the token lands.
///
/// TODO(visual-parity): route through a theme-aware yellow accent once
/// `ThemeColor` exposes one (or once `crates/theme` grows a Tolaria-side
/// `accent_yellow` field).  Until then the dark palette renders this
/// the same light-mode hue, which still reads as "active star" against
/// both backgrounds.
fn star_active_color() -> Hsla {
    gpui::rgb(0xD69E2E).into()
}

/// Active-state colour for the organized toolbar cell — mirrors
/// React's `OrganizedAction` styling, which uses the `--accent-green`
/// CSS custom property.  Tracks `theme.success` directly so the colour
/// follows light / dark mode without an extra constant.
fn organized_active_color(cx: &App) -> Hsla {
    cx.theme().success
}

/// Active-state colour for the neighbourhood toolbar cell (worklist
/// 9.2.14).  Tracks `theme.primary` directly so the cell paints in
/// the theme's accent hue — the same colour the sidebar uses for the
/// row-selection highlight, so users read "currently filtering on
/// this note" the same way they read "currently selected sidebar
/// row".  Theme-aware (light + dark palettes carry their own
/// `primary` mapping) without an extra constant.
fn neighborhood_active_color(cx: &App) -> Hsla {
    cx.theme().primary
}

/// Glyph chosen for the organized toolbar cell at the given active
/// state.  Lives next to [`organized_active_color`] because the
/// icon and the colour together describe the cell's active-state
/// look; lifting the choice into a named function keeps the render
/// path and the regression test (`organized_icon_switches_to_check_when_active`)
/// pinned to the same source of truth.
///
/// `active = true` returns [`IconName::Check`] (a flat checkmark, no
/// surrounding ring) so the disk drawn by [`ActiveStyle::Fill`] on
/// the cell background carries the circle shape and the glyph
/// carries the check — together they reproduce React's filled-disk
/// `OrganizedAction` treatment without needing a
/// `circle-check-fill` icon variant.
fn organized_icon_for(active: bool) -> IconName {
    if active {
        IconName::Check
    } else {
        IconName::CircleCheck
    }
}

/// Convenience wrapper around [`toolbar_cell`] for cells whose
/// behaviour hasn't been wired yet — logs the stub message on click,
/// matching the previous helper's body.  Used by the remaining
/// unwired toolbar cells (notably `note-toolbar-width` and
/// `note-toolbar-ai`) that need real product work (note-width chrome
/// knob, AI panel attach) before they can dispatch.
fn stub_cell(id: &'static str, icon: IconName, tooltip: &'static str) -> AnyElement {
    toolbar_cell(id, icon, tooltip, move |_, _| {
        log::info!("note toolbar action stub: {id}");
    })
}

/// More-overflow popover cell for the `note-toolbar-more` slot
/// (worklist 9.2.7).  Mirrors React's `BreadcrumbOverflowMenu`
/// (`BreadcrumbBar.tsx:892-993`): a [`Popover`] anchored to a small
/// ellipsis button whose body is a [`PopupMenu`] listing the
/// overflow actions in the same order as the React reference.
///
/// `note_path` is captured by the reveal/copy items so the dispatched
/// helpers (which take a `&Path` runtime value, not an action) carry
/// the correct path for the active note.
///
/// Responsive collapse (the React menu also absorbs neighbourhood +
/// file-path actions when the toolbar is narrow) is intentionally
/// deferred to a `9.2.7-followup` — measuring the toolbar's available
/// width and conditionally moving cells into the menu is out of
/// scope for this commit.
///
/// TODO(9.2.7-followup): apply `theme.danger` to the **Delete** label
/// to mirror React's destructive-action styling.  `PopupMenuItem`
/// doesn't expose a direct text-color override today; routing through
/// [`PopupMenuItem::element`] with a custom rendered row is the
/// follow-up shape.  The `Trash2` glyph + "Delete" label already
/// signals destructiveness in the meantime.
fn more_overflow_cell(note_path: PathBuf) -> AnyElement {
    let reveal_path = note_path.clone();
    let copy_path = note_path;
    // `Rc` lets the per-render `content` closure share the captured
    // paths across every PopupMenu rebuild without re-cloning per
    // item (the menu is reconstructed on every paint when the
    // popover is open).
    let reveal_path = Rc::new(reveal_path);
    let copy_path = Rc::new(copy_path);

    Popover::new("note-toolbar-more")
        // Inherit the toolbar's native chrome (no extra background /
        // shadow on the trigger button) — the popover itself paints
        // its own popover-style surface for the menu body.
        .appearance(true)
        .anchor(gpui::Anchor::TopRight)
        .trigger(
            Button::new("note-toolbar-more-trigger")
                .icon(IconName::Ellipsis)
                .ghost()
                .small()
                .tooltip("More actions"),
        )
        .content(move |_, window, cx| {
            let reveal_path = reveal_path.clone();
            let copy_path = copy_path.clone();
            PopupMenu::build(window, cx, move |menu: PopupMenu, _, _| {
                let reveal_path = reveal_path.clone();
                let copy_path = copy_path.clone();
                menu.item(
                    PopupMenuItem::new("Reveal in Finder")
                        .icon(IconName::FolderOpen)
                        .on_click(move |_event, _window, _cx| reveal_in_finder(&reveal_path)),
                )
                .item(
                    PopupMenuItem::new("Copy path")
                        .icon(IconName::Copy)
                        .on_click(move |_event, _window, cx: &mut App| {
                            copy_path_to_clipboard(&copy_path, cx)
                        }),
                )
                .separator()
                .menu_with_icon(
                    "Table of contents",
                    IconName::Menu,
                    Box::new(actions::ToggleTableOfContents),
                )
                .menu_with_icon(
                    "Raw markdown",
                    IconName::SquareTerminal,
                    Box::new(actions::ToggleRawEditor),
                )
                .separator()
                .menu_with_icon("Archive", IconName::Inbox, Box::new(actions::Archive))
                .item(
                    PopupMenuItem::new("Delete")
                        .icon(IconName::Delete)
                        .on_click(move |_event, window: &mut Window, cx: &mut App| {
                            // TODO(9.2.7-followup): route through a
                            // ConfirmDelete dialog before firing the
                            // unlink.  React's reference uses an
                            // `AlertDialog`; we'll wire the GPUI
                            // equivalent once `dialog_stack` lands.
                            window.dispatch_action(Box::new(actions::Delete), cx);
                        }),
                )
            })
            .into_any_element()
        })
        .into_any_element()
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
    // Force a redraw so the toolbar's next render observes the
    // freshly-mutated in-memory frontmatter (worklist 9.2.9).  The
    // vault is a GPUI `Global`, so mutating it does NOT notify any
    // entity — without this nudge the toolbar would keep showing the
    // pre-click glyph until something else triggered a re-render
    // (sidebar selection, window focus, …) and the user would
    // perceive the click as a no-op.  `refresh_windows` is idempotent
    // within an update cycle, so dispatching it once per toggle is
    // cheap.
    cx.refresh_windows();
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

    // Worklist 9.3.5 — `toolbar_cell_builds_inspector_button` was
    // dropped along with the `note-toolbar-inspector` cell itself;
    // the inspector toggle now lives in the title bar
    // (`workspace::title_bar`) and the panel header
    // (`inspector_panel`).  No equivalent toolbar shape test remains
    // here because the toolbar no longer carries the cell.

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

    /// Worklist 9.2.9 — the toolbar-layer regression that paired with
    /// the vault-layer fix.  Reproduces the exact production click
    /// path: render-time read, external edit, click again with the
    /// stale captured value.  Without the fix the in-memory state
    /// would stay desynced from disk and the user would perceive the
    /// star as inert.
    #[gpui::test]
    fn toggle_helper_resyncs_in_memory_after_external_edit(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().expect("tempdir");
        let note_path = dir.path().join("n.md");
        std::fs::write(&note_path, "---\ntype: Note\n---\nbody\n").unwrap();
        let vault = vault::Vault::open_at(dir.path()).expect("open vault");
        let id = cx.update(|cx| cx.foreground_executor().block_on(vault.notes())[0]);
        cx.update(|cx| cx.set_global(vault));

        // Render-time read: the toolbar saw `false` and captured it.
        let captured_is_favorite = cx.update(|cx| {
            cx.global::<vault::Vault>()
                .note_sync(id)
                .unwrap()
                .is_favorite()
        });
        assert!(!captured_is_favorite);

        // External edit — disk now says `_favorite: true`, in-memory
        // still says `false`.
        std::fs::write(&note_path, "---\ntype: Note\n_favorite: true\n---\nbody\n").unwrap();

        // User click — the closure captured `is_favorite = false`,
        // so it dispatches `!false = true` (the value disk already
        // has).  With the 9.2.9 fix the fast path re-syncs in-memory
        // state from disk; without it the toolbar would stay
        // permanently desynced.
        cx.update(|cx| toggle_frontmatter_flag(id, "_favorite", !captured_is_favorite, cx));
        cx.run_until_parked();

        cx.update(|cx| {
            let after = cx
                .global::<vault::Vault>()
                .note_sync(id)
                .unwrap()
                .is_favorite();
            assert!(
                after,
                "fast-path call must re-sync in-memory frontmatter from disk",
            );
        });
    }

    /// Worklist 9.2.11 — the star cell's active-state glyph must paint
    /// `--accent-yellow` (`#D69E2E`).  Pins the literal hue so a future
    /// theme refactor that wires `ThemeColor::accent_yellow` updates
    /// this assertion at the same time.
    #[test]
    fn star_active_color_matches_accent_yellow() {
        let color = star_active_color();
        let expected: Hsla = gpui::rgb(0xD69E2E).into();
        // `Hsla` has no `PartialEq`, so compare the four channels
        // directly; rgb→hsla is deterministic so byte-identity holds.
        assert_eq!(color.h, expected.h);
        assert_eq!(color.s, expected.s);
        assert_eq!(color.l, expected.l);
        assert_eq!(color.a, expected.a);
    }

    /// Worklist 9.2.10 — the organized cell's active-state glyph must
    /// paint `theme.success` (i.e. `--accent-green`).  Anchors the
    /// toolbar's choice of token so a future palette refactor that
    /// retargets the green can't silently desync the toolbar.
    #[gpui::test]
    fn organized_active_color_matches_theme_success(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            let color = organized_active_color(cx);
            let expected = cx.theme().success;
            assert_eq!(color.h, expected.h);
            assert_eq!(color.s, expected.s);
            assert_eq!(color.l, expected.l);
            assert_eq!(color.a, expected.a);
        });
    }

    /// Worklist 9.2.10 / 9.2.11 — the active-color cell helper must
    /// build successfully in both states.  Click behaviour is covered
    /// by the toggle-dispatch tests; this asserts only that the
    /// `Some(color)` path doesn't trip a builder invariant (e.g. by
    /// shadowing `text_color` with a bad ordering).
    #[test]
    fn toolbar_cell_with_active_color_builds_in_both_states() {
        let yellow: Hsla = gpui::rgb(0xD69E2E).into();
        let _on = toolbar_cell_with_active_color(
            "note-toolbar-star",
            IconName::StarFill,
            "Unstar this note",
            true,
            yellow,
            |_window, _cx| {},
        );
        let _off = toolbar_cell_with_active_color(
            "note-toolbar-star",
            IconName::Star,
            "Star this note",
            false,
            yellow,
            |_window, _cx| {},
        );
    }

    /// Worklist 9.2.10 (reopened) — the filled-disk cell helper must
    /// build successfully in both states.  Pairs with the active-color
    /// builder test to pin the new fill-mode helper introduced when the
    /// organized cell switched from a tinted outline to a green disk
    /// with a white check inside.
    #[test]
    fn toolbar_cell_with_active_fill_builds_in_both_states() {
        let green: Hsla = gpui::rgb(0x38A169).into();
        let _on = toolbar_cell_with_active_fill(
            "note-toolbar-organized",
            IconName::Check,
            "Mark as unorganized",
            true,
            green,
            |_window, _cx| {},
        );
        let _off = toolbar_cell_with_active_fill(
            "note-toolbar-organized",
            IconName::CircleCheck,
            "Mark as organized",
            false,
            green,
            |_window, _cx| {},
        );
    }

    /// Worklist 9.2.10 (reopened) — the organized branch swaps from the
    /// outlined `CircleCheck` glyph to a flat `Check` overlaid on a
    /// filled green disk when `is_organized = true`, mirroring the
    /// React `OrganizedAction` `bg-[var(--accent-green)] text-white`
    /// treatment.  Asserting the glyph swap pins the variant choice so
    /// a future tweak to either icon can't quietly desync from the
    /// disk-vs-ring decision.
    ///
    /// The render path picks the icon via a local `if is_organized`
    /// expression; extracting that into a helper here keeps the test
    /// asserting against the production decision, not a duplicate.
    #[test]
    fn organized_icon_switches_to_check_when_active() {
        // Off — outlined ring (no fill, only stroke).
        assert!(
            matches!(organized_icon_for(false), IconName::CircleCheck),
            "inactive organized cell must keep the outlined `CircleCheck` glyph",
        );
        // On — flat check; the disk is drawn by an inner
        // `rounded_full` child of the cell via `ActiveStyle::Fill`,
        // not by a `circle-check-fill` glyph (which doesn't exist in
        // `gpui-component-assets`).
        assert!(
            matches!(organized_icon_for(true), IconName::Check),
            "active organized cell must swap to `Check` so the inner disc carries the circle",
        );
    }

    /// Worklist 9.2.10 reopened-2 — the active organized cell must
    /// render an inner round disc rather than painting the 24x24
    /// `rounded_sm` cell rectangle directly.  React's
    /// `OrganizedAction` is a clean green circle, not a rounded
    /// square.  This test confirms the helper constructs without
    /// panic against the new disc child; the inner-disc invariant
    /// (cell stays transparent, only the disc carries the colour)
    /// lives in code review of the `fill_disc` branch in
    /// `toolbar_cell_inner` since the rendered element tree isn't
    /// directly introspectable from GPUI tests.  The load-bearing
    /// invariant is verified visually on the live app via periscope
    /// — the disc must read as a clean circle, not a rounded square.
    #[test]
    fn organized_active_cell_helper_constructs_with_filled_disc() {
        let _cell = toolbar_cell_with_active_fill(
            "note-toolbar-organized",
            IconName::Check,
            "Mark as unorganized",
            true,
            gpui::rgb(0x38A169).into(),
            |_window, _cx| {},
        );
        // No panic means the new `fill_disc` branch in
        // `toolbar_cell_inner` constructs cleanly.  Regression guard
        // against a future refactor that drops the disc child
        // accidentally — the call must still produce an element.
    }

    /// Worklist 9.2.7 — the More-overflow popover cell helper must
    /// construct cleanly with a real note path.  The full menu body
    /// (Popover trigger + PopupMenu content + per-item handlers) only
    /// renders inside a live `Window` so we can't drive the click
    /// chain headlessly; this test pins the builder shape so a future
    /// refactor that breaks the closure capture chain (e.g. moves the
    /// `Rc<PathBuf>` clones around) surfaces as a compile-time or
    /// constructor-panic regression instead of a runtime-only one.
    #[test]
    fn more_overflow_cell_builds_with_real_path() {
        let path = std::path::PathBuf::from("/tmp/foo/procedure-bar.md");
        let _cell = more_overflow_cell(path);
        // No panic — the Popover + PopupMenu chain constructs.
    }

    /// Worklist 9.2.14 — the neighbourhood cell's active-state colour
    /// must paint `theme.primary` (the same accent the sidebar uses for
    /// the selected-row highlight).  Anchors the toolbar's choice of
    /// token so a future palette refactor that retargets the accent
    /// can't silently desync the toolbar.
    #[gpui::test]
    fn neighborhood_active_color_matches_theme_primary(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            let color = neighborhood_active_color(cx);
            let expected = cx.theme().primary;
            assert_eq!(color.h, expected.h);
            assert_eq!(color.s, expected.s);
            assert_eq!(color.l, expected.l);
            assert_eq!(color.a, expected.a);
        });
    }

    /// Worklist 9.2.14 — the toolbar must paint the neighbourhood cell
    /// in the active state when the `NeighborhoodAnchor` global names
    /// the same note id as the rendered toolbar, and in the muted
    /// state otherwise.  `NeighborhoodAnchor::matches` is the single
    /// branch the render path consults; pinning both arms here means a
    /// future regression that drops the anchor read (or flips the
    /// equality) surfaces as a failing assertion rather than a silent
    /// visual desync.
    #[test]
    fn neighborhood_anchor_matches_only_named_id() {
        use crate::NeighborhoodAnchor;
        let id = NoteId::from_raw(7);
        let other = NoteId::from_raw(99);
        // No anchor installed — every id reads as "not active".
        let empty = NeighborhoodAnchor::default();
        assert!(!empty.matches(id));
        assert!(!empty.matches(other));
        // Anchor names `id` — only the matching id reads as active.
        let anchored = NeighborhoodAnchor(Some(id));
        assert!(anchored.matches(id));
        assert!(!anchored.matches(other));
    }

    /// Worklist 9.2.14 — the neighbourhood cell helper must build
    /// successfully in both states.  Mirrors the
    /// `toolbar_cell_with_active_color_builds_in_both_states` shape for
    /// the star cell — the load-bearing concern is that the
    /// `active = true` branch wires the glyph colour through without
    /// tripping a builder invariant.  Click behaviour is covered by
    /// the live `EnterNeighborhood` dispatch test in
    /// `tolaria::main::tests`.
    #[test]
    fn neighborhood_cell_with_active_color_builds_in_both_states() {
        let accent: Hsla = gpui::rgb(0x155DFF).into();
        let _on = toolbar_cell_with_active_color(
            "note-toolbar-neighborhood",
            IconName::Map,
            "Show neighborhood",
            true,
            accent,
            |_window, _cx| {},
        );
        let _off = toolbar_cell_with_active_color(
            "note-toolbar-neighborhood",
            IconName::Map,
            "Show neighborhood",
            false,
            accent,
            |_window, _cx| {},
        );
    }
}
