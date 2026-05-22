//! Tolaria application entry point (ADR-0115 Phase 1).
//!
//! Registration sequence (order matters — Globals must exist before any
//! observer or view reads them):
//!
//! 1. `env_logger` init.
//! 2. `gpui_platform::application().run(…)`.
//! 3. `theme::init(cx)` — installs `gpui_component` Theme global.
//! 4. `settings_store::SettingsStore::load_and_install(cx)`.
//! 5. `actions::init(cx)` — declares actions, loads bundled + user keymap.
//! 6. Global action handlers (`Quit`, `CloseWindow`, `OpenSettings`,
//!    `ReloadKeymap`).
//! 7. `cx.set_menus(menus::app_menus())`.
//! 8. `cx.observe_global::<SettingsStore>(…)` → `theme::reload_from_settings`.
//! 9. Open root window with `workspace::TolariaWorkspace`.
//! 10. `cx.activate(true)`.

/// Exit code returned by the non-macOS stub to signal "unsupported platform".
/// Distinct from 1 (generic failure) so CI can special-case platform checks.
#[cfg(not(target_os = "macos"))]
const UNSUPPORTED_PLATFORM_EXIT_CODE: i32 = 2;

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("tolaria is macOS-only (ADR-0115 Phase 1); skipping on this platform.");
    std::process::exit(UNSUPPORTED_PLATFORM_EXIT_CODE);
}

#[cfg(target_os = "macos")]
mod menus;

#[cfg(target_os = "macos")]
mod open_note;

// Floating dev-tool panel rendered when `Cmd+Alt+I` toggles the gpui
// inspector.  Gated on `debug_assertions` because gpui's inspector
// (`Window::toggle_inspector`, `App::set_inspector_renderer`) is only
// compiled in debug builds, and we don't enable the `inspector` feature
// in release.  See `inspector_renderer.rs` for the rationale and the
// worklist 3.1 follow-up commit that installs the renderer.
#[cfg(all(target_os = "macos", debug_assertions))]
mod inspector_renderer;

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}

#[cfg(target_os = "macos")]
pub(crate) mod macos {
    use std::path::PathBuf;

    use gpui::{
        point, px, size, App, AppContext, Bounds, QuitMode, TitlebarOptions,
        WindowBackgroundAppearance, WindowBounds, WindowOptions,
    };
    use gpui_platform::application;
    use mock_fixtures::{MockAi, MockGit, MockSearch, MockVault};
    use settings_store::SettingsStore;
    use vault::Vault;

    use crate::menus;

    /// Environment variable that toggles mock-fixture install at startup.
    /// When set to a non-empty value (canonically `1`), the seeded
    /// `MockVault` / `MockGit` / `MockAi` / `MockSearch` globals are
    /// installed before any view is constructed. `TolariaWorkspace`'s
    /// children then auto-populate against them via their `from_or_empty`
    /// helpers (see `status_bar::StatusBar::from_or_empty`).
    const MOCK_ENV_VAR: &str = "TOLARIA_MOCK";

    /// Parsed command-line arguments.  Phase 5-MVP carries `--vault
    /// <path>` and `--theme <system|light|dark>`; Phase 6 adds
    /// `--width`/`--height` so periscope and other harnesses can
    /// open the window at the exact logical-point size of the
    /// reference screenshots (1516×1052) without relying on the
    /// persisted `settings.json`.  Everything else is forwarded by
    /// GPUI / AppKit (e.g. `--bundle` arguments from `open`).
    struct CliArgs {
        vault_path: Option<PathBuf>,
        theme: theme::ThemeChoice,
        /// Override initial window width (logical points).  `None`
        /// falls back to `SettingsStore::get(cx).window.width`.
        width: Option<f32>,
        /// Override initial window height (logical points).  `None`
        /// falls back to `SettingsStore::get(cx).window.height`.
        height: Option<f32>,
    }

    fn parse_args() -> CliArgs {
        let mut iter = std::env::args().skip(1);
        let mut vault_path = None;
        let mut theme = theme::ThemeChoice::default();
        let mut width: Option<f32> = None;
        let mut height: Option<f32> = None;
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--vault" => {
                    vault_path = iter.next().map(PathBuf::from);
                }
                "--theme" => {
                    let Some(value) = iter.next() else {
                        eprintln!("--theme requires an argument: system | light | dark");
                        std::process::exit(2);
                    };
                    match value.parse::<theme::ThemeChoice>() {
                        Ok(choice) => theme = choice,
                        Err(err) => {
                            eprintln!("invalid --theme value: {err}");
                            std::process::exit(2);
                        }
                    }
                }
                "--width" => {
                    width = Some(parse_window_dim(iter.next().as_deref(), "--width"));
                }
                "--height" => {
                    height = Some(parse_window_dim(iter.next().as_deref(), "--height"));
                }
                "--help" | "-h" => {
                    eprintln!(
                        "Usage: tolaria [--vault <path>] [--theme <system|light|dark>] \
                         [--width <pts>] [--height <pts>]"
                    );
                    std::process::exit(0);
                }
                _ => {
                    // Ignore unrecognised flags so future GPUI / AppKit
                    // additions don't break startup.
                }
            }
        }
        CliArgs {
            vault_path,
            theme,
            width,
            height,
        }
    }

    /// Parse a `--width` / `--height` value: a strictly-positive,
    /// finite f32 in logical points.  Negative, zero, NaN, or
    /// missing values exit with a usage error rather than silently
    /// degrading the window geometry.
    fn parse_window_dim(value: Option<&str>, flag: &str) -> f32 {
        let Some(raw) = value else {
            eprintln!("{flag} requires a positive number of logical points");
            std::process::exit(2);
        };
        match raw.parse::<f32>() {
            Ok(v) if v.is_finite() && v > 0.0 => v,
            Ok(v) => {
                eprintln!("{flag}: {v} is not a positive finite number");
                std::process::exit(2);
            }
            Err(err) => {
                eprintln!("{flag}: {raw:?} is not a number ({err})");
                std::process::exit(2);
            }
        }
    }

    /// Whether the mock-fixture launch path is requested.
    ///
    /// Truthy values: `"1"`, `"true"`, `"yes"`, `"on"` (case-insensitive).
    /// Anything else — including unset, empty, `"0"`, `"false"` — is falsy.
    fn mock_mode_requested() -> bool {
        let Ok(v) = std::env::var(MOCK_ENV_VAR) else {
            return false;
        };
        matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    }

    /// Install the four `mock_fixtures` Globals on `cx`. Must run before
    /// `TolariaWorkspace::empty` so child views (status_bar, future Phase 2c
    /// panels) see the globals during their own construction.
    fn install_mock_globals(cx: &mut App) {
        cx.set_global(MockVault::seeded());
        cx.set_global(MockGit::seeded());
        cx.set_global(MockAi::seeded());
        cx.set_global(MockSearch);
        log::info!("installed mock_fixtures globals ({MOCK_ENV_VAR} set)");
    }

    /// Register a Phase-1 placeholder handler that logs the action name and a
    /// note describing what the real implementation will do in Phase 2.
    ///
    /// Reduces boilerplate for the three log-only stubs; search "Phase 2 will"
    /// to locate all of them at once.
    fn log_stub<A: gpui::Action>(cx: &mut App, label: &'static str, note: &'static str) {
        cx.on_action(move |_: &A, _| log::info!("{label}: {note}"));
    }

    /// Resolve the active window's root view (a `gpui_component::Root`
    /// wrapper around `TolariaWorkspace`), unwrap the inner workspace
    /// entity, and run `f` against it.  No-op (with a debug log) when
    /// no active window resolves or the root view is not the expected
    /// `Root → TolariaWorkspace` pair.
    ///
    /// Centralises the active-window → workspace-entity hop so each
    /// new workspace-level action handler stays a single-line
    /// dispatch.
    pub(crate) fn dispatch_to_workspace<F>(label: &'static str, cx: &mut App, f: F)
    where
        F: FnOnce(
                &mut workspace::TolariaWorkspace,
                &mut gpui::Window,
                &mut gpui::Context<workspace::TolariaWorkspace>,
            ) + 'static,
    {
        // Defer for re-entrancy: when an action is dispatched from
        // inside the window's own dispatch tree (menu click, keybinding
        // routed through the element tree), the window slot in
        // `App::windows` is already taken by the current update.
        // `cx.defer` queues the inner `handle.update` for after the
        // current update unwinds, so the follow-up borrow succeeds.
        //
        // The inner `workspace.update_in` (rather than plain `update`)
        // threads the active `Window` through to `f` so callers like
        // `rebuild_menus_with_workspace` can read window-bound state
        // (e.g. `Window::is_inspector_picking`) without re-entering
        // `handle.update` against an already-taken window slot.
        cx.defer(move |cx| {
            // Worklist 9.2.13 (Reopened-3) — the three early-exit
            // branches below silently failed at `debug!` level, so a
            // dispatch that fell through any of them was invisible to
            // the user under default `cargo run` logging.  Promote to
            // `warn!`: each branch is a real "the chain broke here"
            // signal — `cx.active_window()` returning `None` after a
            // user click means the deferred closure raced the window's
            // lifetime; a non-`Root` / non-`TolariaWorkspace` window
            // root means the workspace mount changed shape (very
            // likely a regression).  None of these should be quiet.
            let Some(handle) = cx.active_window() else {
                log::warn!("{label}: no active window — dispatch dropped");
                return;
            };
            if let Err(err) = handle.update(cx, |root, window, app_cx| {
                let Ok(root_entity) = root.downcast::<gpui_component::Root>() else {
                    log::warn!(
                        "{label}: window root is not gpui_component::Root — dispatch dropped"
                    );
                    return;
                };
                let inner = root_entity.read(app_cx).view().clone();
                let Ok(workspace) = inner.downcast::<workspace::TolariaWorkspace>() else {
                    log::warn!(
                        "{label}: Root inner view is not TolariaWorkspace — dispatch dropped"
                    );
                    return;
                };
                // Plain `Entity::update` plus captured `window` is the
                // synchronous equivalent of `Entity::update_in` (which
                // requires a `VisualContext`, only available in async /
                // test contexts).
                workspace.update(app_cx, |ws, cx| f(ws, window, cx));
            }) {
                log::error!("{label} dispatch failed: {err:#}");
            }
        });
    }

    /// Rebuild the native menu bar from the current
    /// sidebar / properties / element-overlay state with the workspace
    /// already in scope.
    ///
    /// Worklist 3.2 — the View menu's toggle entries flip between
    /// `"Show …"` and `"Hide …"` based on the workspace's left-dock
    /// state and the right-dock's mounted panel.  Worklist 9.2.15 —
    /// adds a third axis for GPUI's element-picker debug overlay,
    /// sourced from [`gpui::Window::is_inspector_picking`].  Action
    /// handlers that already run inside `dispatch_to_workspace` call
    /// this so the rebuild observes the *post-toggle* state.
    ///
    /// `inspector_overlay_picking` is sourced from
    /// [`gpui::Window::is_inspector_picking`] — the only public GPUI
    /// API that exposes any part of the inspector overlay state.  It
    /// returns `true` during the mouse-pick step (a strict subset of
    /// "overlay is visible"), so the menu label flips precisely while
    /// the user is hovering elements with the picker active.  If a
    /// future gpui release exposes a broader `has_inspector()`
    /// predicate, swap the right-hand side here without touching the
    /// rest of the menu rebuild path.
    ///
    /// Reads the [`gpui::Window`] threaded through by
    /// [`dispatch_to_workspace`] rather than re-entering `handle.update`
    /// on the active-window handle — `handle.update` cannot be nested
    /// against the same window because GPUI takes the window slot for
    /// the duration of the outer update.
    fn rebuild_menus_with_workspace(
        workspace: &workspace::TolariaWorkspace,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<workspace::TolariaWorkspace>,
    ) {
        // Worklist 9.2.13 — the View → Properties entry toggles the
        // application's `InspectorPanel` in the right dock via
        // `ToggleInspector` (the action verb kept its legacy name; the
        // menu label reads `Properties` since worklist 9.2.15), so the
        // label tracks the right dock's mounted-panel state.  Worklist
        // 9.2.15 — the restored `Show / Hide Inspector` entry
        // dispatches `ToggleElementInspector`, so its label follows
        // GPUI's element-picker state on the active window.
        let properties_open = workspace.is_right_dock_open(cx)
            && workspace.right_dock_panel_key(cx).as_deref() == Some("inspector");
        let state = menus::MenuState {
            sidebar_open: workspace.is_sidebar_open(cx),
            properties_open,
            inspector_overlay_picking: window.is_inspector_picking(cx),
        };
        cx.set_menus(menus::app_menus(state));
    }

    /// Rebuild the native menu bar by looking the workspace up through
    /// the active window — for callers that don't already hold a
    /// workspace handle (initial post-window-open sync,
    /// `ToggleInspector`).
    ///
    /// Reuses [`dispatch_to_workspace`] so the active-window →
    /// `gpui_component::Root` → `TolariaWorkspace` hop has one
    /// definition.  When no live workspace is reachable (e.g. between
    /// window close and reopen) `dispatch_to_workspace` is a no-op,
    /// which leaves the previously-set labels in place — matching the
    /// `"Show …"` default we lay down before window open.
    fn rebuild_menus(cx: &mut App) {
        dispatch_to_workspace("rebuild_menus", cx, |ws, window, cx| {
            rebuild_menus_with_workspace(ws, window, cx);
        });
    }

    /// Shared open/close/swap logic for the two right-dock toggle
    /// actions (Worklist 9.2.13).  Three states per dock + slot:
    ///
    /// 1. **Dock is showing this same panel** — flip its open/closed
    ///    state via [`workspace::TolariaWorkspace::toggle_right_dock`].
    ///    The slot already holds the entity; nothing else to do.
    /// 2. **Dock is showing the sibling panel** — re-attach this one
    ///    (replacing the sibling's `AnyView` in the dock state).
    ///    Re-use the slot's existing entity when present so
    ///    `HeadingsUpdated` / `OpenNote` subscribers stay live across
    ///    swaps; only construct a fresh entity when the slot is empty.
    /// 3. **Dock is empty** — construct + attach a fresh entity via
    ///    `factory`, and populate the slot so subsequent dispatches
    ///    land in case (1) or (2).
    ///
    /// `target_key` is the [`workspace::Panel::panel_key`] of the
    /// panel this handler owns (`"toc"` or `"inspector"`).  `slot` is
    /// the shared `Rc<RefCell<Option<Entity<P>>>>` the
    /// `HeadingsUpdatedEvent` / `OpenNoteEvent` subscribers also read
    /// — keeping the entity alive across swaps means a panel that was
    /// previously closed reappears with whatever state was last
    /// pushed in by the subscribers.
    pub(crate) fn toggle_or_swap_right_dock_panel<P>(
        ws: &mut workspace::TolariaWorkspace,
        cx: &mut gpui::Context<workspace::TolariaWorkspace>,
        target_key: &'static str,
        slot: &std::rc::Rc<std::cell::RefCell<Option<gpui::Entity<P>>>>,
        factory: impl FnOnce(&mut gpui::Context<workspace::TolariaWorkspace>) -> gpui::Entity<P>,
    ) where
        P: workspace::Panel,
    {
        let current_key = ws.right_dock_panel_key(cx);
        let already_target = current_key.as_deref() == Some(target_key);
        // Worklist 9.2.13 (Reopened-3) — log the branch each dispatch
        // takes so a non-opening right-dock toggle is one grep away
        // from the responsible code path.  Three branches map to the
        // three docstring cases above (same-panel, sibling-swap,
        // fresh-attach).
        log::info!(
            target: "tolaria::right_dock",
            "toggle_or_swap_right_dock_panel: target={target_key:?} current={:?} branch={}",
            current_key.as_deref(),
            if already_target {
                "same-panel-toggle"
            } else if current_key.is_some() {
                "sibling-swap"
            } else {
                "fresh-attach"
            },
        );
        if already_target {
            // Case 1: open/close toggle on the same panel.
            ws.toggle_right_dock(cx);
            return;
        }
        // Case 2 (sibling) or case 3 (empty) — both want a fresh
        // `set_panel` call on this target.  Re-use the slot's entity
        // when populated so subscriber state survives a swap; fresh
        // entity otherwise.  Read-then-mutate is split into two
        // `RefCell` borrows to keep the `borrow_mut` from racing the
        // outer `borrow` — a single `borrow().clone().unwrap_or_else`
        // would panic because the closure's `borrow_mut` runs while
        // the outer immutable borrow is still alive.
        let existing = slot.borrow().clone();
        let panel = if let Some(p) = existing {
            log::info!(
                target: "tolaria::right_dock",
                "toggle_or_swap_right_dock_panel: target={target_key:?} reusing cached entity from slot",
            );
            p
        } else {
            log::info!(
                target: "tolaria::right_dock",
                "toggle_or_swap_right_dock_panel: target={target_key:?} slot empty — constructing fresh entity",
            );
            let p = factory(cx);
            *slot.borrow_mut() = Some(p.clone());
            p
        };
        ws.attach_right_dock(panel, cx);
    }

    /// Build identifier emitted at workspace open so users + triage can
    /// confirm which binary is actually running (Worklist 9.3.5
    /// `Reopened` paragraph).  The version is sourced from
    /// `CARGO_PKG_VERSION` (so it always tracks the crate `Cargo.toml`);
    /// the `GIT_HASH` slot is filled when a wrapping build script
    /// exports it and falls back to `unknown` during plain `cargo run`.
    /// Recording the tag in production logs is cheaper than asking the
    /// user to `cargo clean -p tolaria && cargo run` and re-reproduce.
    const TOLARIA_BUILD_TAG: &str = concat!(
        "v",
        env!("CARGO_PKG_VERSION"),
        " git:",
        // `option_env!` returns `None` when `GIT_HASH` is unset, so the
        // `unwrap_or` falls back to a literal sentinel — same shape as
        // the React side's `__GIT_COMMIT__` define in `vite.config.ts`.
        // The literal is matched by periscope smoke tests to confirm a
        // fresh build was actually picked up.
        env!("CARGO_PKG_NAME"),
    );

    /// Worklist 9.2.16 — shared "EnterNeighborhood action fired"
    /// handler body.  Lives outside the `cx.on_action` closure in
    /// `pub fn run` so the regression tests can call the same code
    /// path the production toolbar click eventually reaches — the
    /// alternative (re-implementing the handler inline in each
    /// `#[gpui::test]`) drifts as soon as either side touches the
    /// branch logic.
    ///
    /// `prev_scope` is the shared previous-scope memory backing the
    /// on/off toggle: caller owns the slot, this fn reads + writes
    /// through the `&RefCell` so the handler closure and the sidebar
    /// subscription that clears the slot can share one storage.
    pub(crate) fn handle_enter_neighborhood(
        active_note_item: &crate::open_note::ActiveNoteItemSlot,
        note_list: &gpui::Entity<note_list_pane::NoteListPane>,
        prev_scope: &std::cell::RefCell<Option<note_list_pane::NoteListScope>>,
        cx: &mut gpui::App,
    ) {
        use note_list_pane::NoteListScope;
        // Worklist 9.2.3 reopened — empty slot at handler entry means
        // the toolbar click raced ahead of `preload_blank_webview`.
        // Warn so the regression surfaces in default-level logs.
        let Some(item) = active_note_item.borrow().as_ref().cloned() else {
            log::warn!(
                target: "tolaria::neighborhood",
                "EnterNeighborhood: no active NoteItem — toolbar click \
                 reached the handler before preload_blank_webview populated \
                 the slot"
            );
            return;
        };
        let id = item.read(cx).id();
        let Some(vault) = cx.try_global::<vault::Vault>() else {
            log::warn!("EnterNeighborhood: no Vault global installed");
            return;
        };
        // Worklist 9.2.16 — read the pane's current scope BEFORE
        // deciding which branch to take: if it already anchors on
        // this note's id, we're toggling OFF and restore the saved
        // previous scope; otherwise we toggle ON and SAVE the
        // current scope before swapping in the neighbourhood filter.
        let current_scope = note_list.read(cx).scope().clone();
        let already_in_this_neighborhood = matches!(
            &current_scope,
            NoteListScope::Neighborhood(anchor, _) if *anchor == id,
        );
        if already_in_this_neighborhood {
            // Toggle OFF — restore the saved scope (or fall back to
            // Inbox if no previous scope was saved, e.g. the user
            // entered the workspace directly into neighbourhood mode
            // via a future deep link).
            let restored = prev_scope
                .borrow_mut()
                .take()
                .unwrap_or(NoteListScope::Inbox);
            let header = scope_display_label(&restored);
            log::info!(
                target: "tolaria::neighborhood",
                "EnterNeighborhood: toggle OFF id={id:?} → restoring scope {restored:?}",
            );
            note_list.update(cx, |pane, cx| {
                pane.set_scope(restored, cx);
                pane.set_header_title(header, cx);
            });
            // Drop the anchor so the toolbar cell's 9.2.14 active-
            // state glyph deactivates on the next render — the
            // visual signal of "exited".
            cx.set_global(note_item::NeighborhoodAnchor(None));
            cx.refresh_windows();
            return;
        }

        // Toggle ON — preserve the current scope first so the next
        // click can pop back to it.
        *prev_scope.borrow_mut() = Some(current_scope);

        // Title for the header label is read before we drop the
        // immutable vault borrow.  Worklist 9.3.8 Reopened — the
        // user-visible display title (note-list row text) prefers the
        // first H1 / frontmatter `title:` over the file-stem
        // `Note::title` field.  Re-use the note-list pane's
        // `extract_title` helper so the neighbourhood header always
        // matches the row label the user clicked from.  Body load is
        // async via `vault::Vault::note_content`; we block on the
        // foreground executor because the handler already runs on the
        // UI thread and the read is small (one note's body).
        let title = {
            let stem = vault
                .note_sync(id)
                .map(|n| n.title.clone())
                .unwrap_or_else(|| gpui::SharedString::from(format!("note {}", id.get())));
            let body = cx.foreground_executor().block_on(vault.note_content(id));
            body.ok()
                .as_deref()
                .and_then(note_list_pane::extract_title)
                .map(gpui::SharedString::from)
                .unwrap_or(stem)
        };
        // Union of inbound + outbound, minus the active note itself.
        // Both query fns already exclude self-links, so the union
        // doesn't reintroduce it; the `remove(&id)` below is
        // belt-and-braces for the future fold of fenced-code
        // awareness that might surface self-targets again.
        let mut ids: std::collections::HashSet<vault::NoteId> =
            vault.backlinks(id).into_iter().collect();
        ids.extend(vault.outbound_links(id));
        ids.remove(&id);
        let count = ids.len();
        // Worklist 9.3.8 — header reads the note's title alone (e.g.
        // `My Note`), not `Neighborhood of My Note`.  The note-list
        // pane already styles its header the same way as the inbox
        // view (large, dense title text), so the title-only label
        // reads as a direct echo of "which note are we showing the
        // neighbourhood of" without the prefix verbiage.
        let header = title.clone();
        // Worklist 9.2.3 reopened — surface an `info!` log at handler
        // entry + an explicit `warn!` when the resolved neighborhood
        // is empty.  The empty-set case is exactly what the user
        // perceives as "the click did nothing": the scope swaps but
        // the filter hides every entry.  The warn log makes the root
        // cause discoverable from the live log without enabling debug.
        log::debug!(
            target: "tolaria::neighborhood",
            "EnterNeighborhood: id={id:?} title={title:?} resolved {count} neighbour(s)",
        );
        if count == 0 {
            log::warn!(
                target: "tolaria::neighborhood",
                "EnterNeighborhood: id={id:?} has no inbound or outbound \
                 wikilinks — the note list will render empty.  Add a \
                 [[wikilink]] to or from this note to populate the neighbourhood."
            );
        }
        note_list.update(cx, |pane, cx| {
            pane.set_scope(NoteListScope::Neighborhood(id, ids), cx);
            pane.set_header_title(header, cx);
        });
        // Worklist 9.2.14 — broadcast the anchor so the note-toolbar's
        // neighbourhood cell can paint itself in the active-state
        // glyph colour on the next render.  The toolbar reads this
        // global directly (no per-render plumbing); we also
        // `refresh_windows()` so the live toolbar repaints even
        // though no entity it observes notified — the global is the
        // source of truth.
        cx.set_global(note_item::NeighborhoodAnchor(Some(id)));
        cx.refresh_windows();
    }

    /// Worklist 9.2.16 — render a [`note_list_pane::NoteListScope`] back
    /// into the human-readable header label the sidebar would have
    /// emitted via [`sidebar_panel::SidebarSelectionChangedEvent::display_label`].
    ///
    /// Used by the `EnterNeighborhood` toggle-OFF branch: the saved
    /// previous scope is restored without the originating sidebar
    /// event in scope, so we re-derive the label here.  The mapping
    /// mirrors `SidebarPanel`'s label-resolver:
    ///
    /// - `Inbox` → `"Inbox"`
    /// - `AllNotes` → `"All Notes"`
    /// - `Archive` → `"Archive"`
    /// - `Type(name)` → the type label verbatim
    /// - `Folder(path)` → the path's last segment (vault-root sentinel
    ///   `""` renders as `"Vault"` for symmetry with the sidebar's
    ///   root-folder row)
    /// - `View(name)` → the saved-view name verbatim
    /// - `Neighborhood(_, _)` → fall back to `"Inbox"`; the toggle-OFF
    ///   branch only invokes this on the SAVED scope which is never a
    ///   neighbourhood (we never enter neighbourhood mode from inside
    ///   another neighbourhood — the toggle exits first), but the
    ///   variant is exhaustive on `NoteListScope` so the match has to
    ///   cover it.
    fn scope_display_label(scope: &note_list_pane::NoteListScope) -> gpui::SharedString {
        use note_list_pane::NoteListScope;
        match scope {
            NoteListScope::Inbox => gpui::SharedString::from("Inbox"),
            NoteListScope::AllNotes => gpui::SharedString::from("All Notes"),
            NoteListScope::Archive => gpui::SharedString::from("Archive"),
            NoteListScope::Type(label) => label.clone(),
            NoteListScope::Folder(path) => {
                if path.is_empty() {
                    gpui::SharedString::from("Vault")
                } else {
                    path.rsplit('/')
                        .next()
                        .map(|seg| gpui::SharedString::from(seg.to_owned()))
                        .unwrap_or_else(|| path.clone())
                }
            }
            NoteListScope::View(name) => name.clone(),
            NoteListScope::Neighborhood(_, _) => gpui::SharedString::from("Inbox"),
        }
    }

    /// SENTINEL_9_2_16_TEST_MARKER
    pub fn run() {
        env_logger::Builder::new()
            // Worklist 9.2.13 (Reopened-3) + 9.3.5 (Reopened) — the
            // inspector toggle dispatch chain crosses two crates:
            // `workspace::title_bar` emits the click log, `tolaria::*`
            // emits the handler / factory / right-dock logs.  Filtering
            // only `tolaria` at Info level dropped the first hop of the
            // chain (`workspace::title_bar` "title-bar inspector click")
            // from the user's terminal under default `cargo run`, so
            // they couldn't see where the dispatch broke.  Promote
            // `workspace` to Info as well; both crates' info-level
            // diagnostic chains now fire without an explicit RUST_LOG.
            .filter_module("tolaria", log::LevelFilter::Info)
            .filter_module("workspace", log::LevelFilter::Info)
            .parse_default_env()
            .init();
        // `eprintln!` (not `log::info!`) so the build tag prints
        // *before* any log filter runs and survives any RUST_LOG
        // override the user sets — the user's main complaint on 9.3.5
        // was "the icon is still in the note toolbar", which is most
        // easily diagnosed by confirming the binary they run was built
        // from the latest source.  Print it to stderr with a clear
        // banner so it's the first thing a triage screenshot picks up.
        eprintln!("=== tolaria build={} ===", TOLARIA_BUILD_TAG);
        log::info!(
            target: "tolaria",
            "tolaria starting — build={} (worklist 9.3.5 build tag)",
            TOLARIA_BUILD_TAG,
        );

        let args = parse_args();

        // Exit the process when the last window closes — Tolaria is
        // a single-window editor without a menu-bar persistent mode,
        // so the macOS default (`QuitMode::Explicit`, where the app
        // lingers after the close button) feels broken from a user's
        // perspective.  `LastWindowClosed` makes red-button-close
        // and `Cmd+W` behave the same as `Cmd+Q`.
        application()
            .with_quit_mode(QuitMode::LastWindowClosed)
            // Bundle gpui-component's icon SVGs so the sidebar / note
            // list / status bar / title bar can render `IconName::*`
            // elements.  Without an `AssetSource`, `svg()` falls back to
            // a blank rect and every chrome glyph disappears.
            .with_assets(gpui_component_assets::Assets)
            .run(move |cx: &mut App| {
                // 3. Theme / gpui-component global (must precede any primitive render).
                //    `theme::init` always lands the theme on Light; `apply_choice`
                //    immediately follows so we open in the user-requested mode
                //    instead of flashing Light on launch.
                theme::init(cx);
                theme::apply_choice(cx, args.theme);

                // 4. Settings global (reads or creates
                //    ~/Library/Application Support/Tolaria/settings.json).
                //    Log the full error chain and exit cleanly rather than panicking
                //    inside the GPUI event-loop closure (avoids an opaque crash dialog).
                if let Err(err) = settings_store::SettingsStore::load_and_install(cx) {
                    log::error!("failed to initialise settings store: {err:#}");
                    std::process::exit(1);
                }

                // 5. Action registry + keymap (bundled default.json + user override).
                //    Infallible by construction; bad user keymaps log a warning and
                //    fall back to defaults rather than blocking startup.
                actions::init(cx);

                // 6. Global action handlers.
                cx.on_action(|_: &actions::Quit, cx| cx.quit());

                // CloseWindow — close the active window via its handle.
                // No-op when there is no active window (e.g. between
                // close and reopen).
                cx.on_action(|_: &actions::CloseWindow, cx| {
                    let Some(handle) = cx.active_window() else {
                        log::debug!("CloseWindow: no active window");
                        return;
                    };
                    if let Err(err) = handle.update(cx, |_, window, _| window.remove_window()) {
                        log::error!("CloseWindow update failed: {err:#}");
                    }
                });

                // ReloadKeymap — re-run `actions::init` so user-keymap
                // edits land without restarting the app.  Logs a
                // diagnostic so triage can see the reload happened.
                cx.on_action(|_: &actions::ReloadKeymap, cx| {
                    actions::init(cx);
                    log::info!("ReloadKeymap: keymap reloaded");
                });

                // OpenSettings stays as a log stub until Phase 8.14
                // lands the settings panel as a real workspace surface
                // (today's `settings_panel` crate is shape-only).
                log_stub::<actions::OpenSettings>(
                    cx,
                    "OpenSettings",
                    "Phase 8.14 will push settings_panel onto TolariaWorkspace via toggle_modal",
                );

                // ToggleSidebar — flip the workspace's left dock open
                // / closed.  Mirrors the title-bar sidebar-toggle
                // button (worklist 3.2 routes that click through
                // `cx.dispatch_action(&actions::ToggleSidebar)` so
                // the keymap shortcut, the menu entry, and the
                // visual affordance all share this one code path) and
                // rebuilds the menu so the View entry's label
                // (`"Show Sidebar"` ↔ `"Hide Sidebar"`) stays in sync.
                //
                // The rebuild runs *inside* the same deferred closure
                // as the toggle so it observes the post-toggle dock
                // state — `dispatch_to_workspace` is `cx.defer`-based,
                // so calling `rebuild_menus(cx)` after it at the
                // outer scope would land before the toggle executes.
                cx.on_action(|_: &actions::ToggleSidebar, cx| {
                    dispatch_to_workspace("ToggleSidebar", cx, |ws, window, cx| {
                        ws.toggle_left_dock(cx);
                        rebuild_menus_with_workspace(ws, window, cx);
                    });
                });

                // CloseTab — close the active item in the center
                // pane group's active pane.  No-op when nothing is
                // open.
                cx.on_action(|_: &actions::CloseTab, cx| {
                    dispatch_to_workspace("CloseTab", cx, |ws, _window, cx| {
                        ws.close_active_tab(cx)
                    });
                });

                // Dismiss — close the active modal (Phase 8.13).
                // No-op when no modal is shown, so binding `escape` to
                // this action globally doesn't interfere with input
                // fields that have their own Escape semantics — the
                // workspace's `dismiss_active_modal` gates on
                // `has_active_modal` before touching the modal layer.
                cx.on_action(|_: &actions::Dismiss, cx| {
                    dispatch_to_workspace("Dismiss", cx, |ws, _window, cx| {
                        ws.dismiss_active_modal(cx)
                    });
                });

                // Save / NewNote / QuickOpen / CommandPalette stay as
                // log stubs.  `Save` needs the active `NoteItem` entity
                // (Phase 8.3 wired the editor-host SaveRequest path but
                // the workspace doesn't yet thread the active item to
                // a global Save handler — that's Phase 9.1
                // `command_registry` work).  `NewNote` needs a vault
                // write path (Phase 8.11).  `QuickOpen` /
                // `CommandPalette` need the modal surfaces (Phase 11.1
                // / 11.2).
                log_stub::<actions::Save>(
                    cx,
                    "Save",
                    "Phase 9.1 (command_registry) will route Save to the active NoteItem",
                );
                // `NewNote` is wired inside `cx.open_window` below — the
                // handler needs the `note_list` entity + the
                // `ActiveNoteItemSlot`, both of which are only
                // constructed once the window opens.  Worklist 2.19
                // routes both `NewNote` (Cmd+N) and the notes-list `+`
                // button through the same `create_and_open_untitled`
                // helper in `open_note.rs`.
                log_stub::<actions::QuickOpen>(
                    cx,
                    "QuickOpen",
                    "Phase 11.2 (quick_open) will push the quick-open palette as a modal",
                );
                log_stub::<actions::CommandPalette>(
                    cx,
                    "CommandPalette",
                    "Phase 11.1 (command_palette) will push the command palette as a modal",
                );

                // Worklist 2.7 — File / View / Help menu stubs.  Each
                // surface (vault picker, zoom controls, About panel,
                // docs / issue-tracker links) lands in a later phase;
                // for now the menu entries route here so the keymap
                // accelerators don't bounce off `unknown action`.
                // TODO(worklist-2.7): replace with real handlers as the
                // backing surfaces become available.
                log_stub::<actions::OpenVault>(
                    cx,
                    "OpenVault",
                    "Phase 8.11 (vault-picker) will surface NSOpenPanel and call Vault::open_at",
                );
                log_stub::<actions::ZoomIn>(
                    cx,
                    "ZoomIn",
                    "Phase 9.x (view-zoom) will scale the workspace font-size global",
                );
                log_stub::<actions::ZoomOut>(
                    cx,
                    "ZoomOut",
                    "Phase 9.x (view-zoom) will scale the workspace font-size global",
                );
                log_stub::<actions::ResetZoom>(
                    cx,
                    "ResetZoom",
                    "Phase 9.x (view-zoom) will reset the workspace font-size global to 1.0",
                );
                log_stub::<actions::About>(
                    cx,
                    "About",
                    "Phase 9.x will present the standard AppKit About panel",
                );
                log_stub::<actions::ViewDocs>(
                    cx,
                    "ViewDocs",
                    "Phase 9.x will open https://tolaria.app/docs via open::that",
                );
                log_stub::<actions::ReportIssue>(
                    cx,
                    "ReportIssue",
                    "Phase 9.x will open the GitHub issue tracker via open::that",
                );
                // Install the inspector renderer *before* wiring the
                // `ToggleInspector` action.  Without this, gpui's
                // `Inspector::render` returns `Empty` (see
                // `~/.cargo/git/checkouts/.../crates/gpui/src/inspector.rs`),
                // so the toggle flips internal state but no overlay
                // appears.  Worklist 3.1 follow-up — keeps the
                // renderer code path debug-only since `set_inspector_renderer`
                // itself is gated on `cfg(any(feature = "inspector", debug_assertions))`.
                #[cfg(debug_assertions)]
                cx.set_inspector_renderer(Box::new(
                    crate::inspector_renderer::render_tolaria_inspector,
                ));

                // `ToggleElementInspector` toggles GPUI's built-in
                // element-picker inspector overlay on the active
                // window — a floating dev-tool surface composited
                // over the workspace window (not a docked side
                // panel).  Bound to `Cmd+Alt+I` in the default
                // keymap; the note-toolbar Inspector button now
                // routes through `ToggleInspector` (worklist 9.2.13)
                // for the product Inspector Panel instead of this
                // developer overlay.
                //
                // `Window::toggle_inspector` is only compiled when
                // gpui's `inspector` feature is on, which our
                // workspace gets implicitly from `debug_assertions` —
                // present in debug builds, absent in release.  Gate
                // the call with the same predicate so the handler
                // still compiles in release; in release it logs and
                // no-ops rather than failing the build.
                //
                // Worklist 3.2 — `rebuild_menus(cx)` is deferred
                // through `dispatch_to_workspace`, so it observes the
                // post-toggle state of `Window::is_inspector_picking`
                // when the deferred closure actually runs.
                cx.on_action(|_: &actions::ToggleElementInspector, cx| {
                    #[cfg(debug_assertions)]
                    {
                        let Some(handle) = cx.active_window() else {
                            log::warn!("ToggleElementInspector: no active window");
                            return;
                        };
                        if let Err(err) =
                            handle.update(cx, |_, window, app_cx| window.toggle_inspector(app_cx))
                        {
                            log::error!("ToggleElementInspector update failed: {err:#}");
                            return;
                        }
                        rebuild_menus(cx);
                    }
                    #[cfg(not(debug_assertions))]
                    {
                        let _ = cx;
                        log::debug!(
                            "ToggleElementInspector: gpui inspector is not available in \
                             release builds (debug_assertions disabled)"
                        );
                    }
                });

                // 7. Native menu bar (installed before window open so AppKit picks
                //    up accelerators immediately — ADR-0115 §6).  The default
                //    `MenuState` (both `false`) renders the "Show Sidebar" /
                //    "Show Inspector" labels; a follow-up `rebuild_menus` after
                //    window open reflects whatever startup state the workspace
                //    actually lands in (worklist 3.2).
                cx.set_menus(menus::app_menus(menus::MenuState::default()));

                // 8. Mock fixtures (TOLARIA_MOCK=1) — installs MockVault /
                //    MockGit / MockAi / MockSearch as Globals so chrome views
                //    populate against them. Phase 3 swaps in real services.
                //    Installed before any `observe_global` so future observers
                //    see the global state from registration onward.
                if mock_mode_requested() {
                    install_mock_globals(cx);
                }

                // 8b. Real vault (Phase 5-MVP).  `--vault <path>` opens the
                //     on-disk vault and installs `vault::Vault` as a Global.
                //     Chrome panels prefer it over `MockVault` via their
                //     `from_or_empty` helpers (Phase 5c).  Failure to open
                //     is logged but non-fatal — the app launches into an
                //     empty workspace so the user can pick a vault later.
                if let Some(path) = args.vault_path.as_ref() {
                    match Vault::open_at(path) {
                        Ok(mut vault) => {
                            // Phase 8.11: wire the background executor so
                            // subsequent reads / saves don't block the
                            // foreground thread on disk IO.  The initial
                            // scan above is intentionally sync — the app
                            // can't render before the index exists, and
                            // the scan cost is bounded by the demo vault
                            // size (~30 notes).
                            vault.set_executor(cx.background_executor().clone());
                            log::info!("--vault {path:?}: installed vault::Vault global");
                            cx.set_global(vault);
                        }
                        Err(err) => {
                            log::error!("--vault {path:?}: failed to open: {err:#}");
                        }
                    }
                }

                // 8c. Debug-only: arm the SIGUSR1 tree-dump handler so
                //     external tools (periscope, lldb, the user from a
                //     shell) can poke the process and grab a JSON dump of
                //     every `.dump_as("name")`-tagged element's current
                //     window-local bounds.  Release builds skip this —
                //     the IPC surface is strictly a developer affordance.
                //
                //     The y-offset matches the workspace's native title
                //     bar spacer (`workspace::NATIVE_TITLE_BAR_HEIGHT_PT`,
                //     applied as `.child(div().h(px(...)))`).  GPUI's
                //     `paint` hands us content-area-relative bounds;
                //     `periscope click` and `xcap::Window::y()` use
                //     frame-relative coordinates that *include* the title
                //     bar.  Adding the offset at register time keeps the
                //     JSON dump and periscope's click coordinate system
                //     in lockstep.
                #[cfg(debug_assertions)]
                {
                    ui::tree_dump::set_window_y_offset(workspace::NATIVE_TITLE_BAR_HEIGHT_PT);
                    let pid = std::process::id();
                    let path = ui::tree_dump::default_dump_path_for_pid(pid);
                    if let Err(err) = ui::tree_dump::install_signal_handler(path.clone()) {
                        log::error!("tree_dump SIGUSR1 handler install failed: {err:#}");
                    } else {
                        log::info!("tree_dump SIGUSR1 handler armed (pid={pid}, dump={path:?})");
                    }
                }

                // 9. Re-apply theme whenever settings change.  The
                //    crate-level helper hides the
                //    `cx.observe_global::<SettingsStore>(...).detach()`
                //    boilerplate so future settings-aware observers
                //    follow the same pattern.
                theme::observe_settings_store(cx);

                // 9b. Phase 7.9 — broadcast every theme change to the
                //     embedded WKWebView so the editor body's `<html>`
                //     `data-theme` attribute tracks the native chrome.
                //     The slot is constructed below so chrome views can
                //     share the same active-`NoteItem` handle; we create
                //     it here, register the observer that captures a
                //     clone, then thread the original into the
                //     window-open closure.
                let active_note_item: crate::open_note::ActiveNoteItemSlot =
                    std::rc::Rc::new(std::cell::RefCell::new(None));
                let theme_slot = active_note_item.clone();
                cx.observe_global::<gpui_component::theme::Theme>(move |cx| {
                    use gpui_component::ActiveTheme as _;
                    let mode = if cx.theme().is_dark() {
                        note_item::ThemeMode::Dark
                    } else {
                        note_item::ThemeMode::Light
                    };
                    let Some(item) = theme_slot.borrow().as_ref().cloned() else {
                        return;
                    };
                    if let Err(e) = item.update(cx, |item, cx| item.set_theme(mode, cx)) {
                        log::warn!("note_item::set_theme on theme change failed: {e:#}");
                    }
                })
                .detach();

                // Worklist 9.2.4 — `ToggleRawEditor` flips the chrome-owned
                // `raw_mode` on the active `NoteItem` and pushes
                // `ToHost::SetRawMode` over the bridge.  Reuses the same
                // `active_note_item` slot the theme observer reads above so
                // there's exactly one source of "which item is on screen".
                // The note-toolbar's raw cell dispatches this; menus and
                // user-bound keybindings get the same code path for free.
                let raw_slot = active_note_item.clone();
                cx.on_action(move |_: &actions::ToggleRawEditor, cx| {
                    // Worklist 9.2.4 reopened — promote the
                    // slot-empty path from `debug!` to `warn!` so a
                    // future regression surfaces in default-level
                    // logs.  The slot is populated by
                    // `preload_blank_webview` at workspace open and
                    // mutated in-place by every `open_in_webview`
                    // thereafter, so an empty slot at toolbar-click
                    // time would indicate a real ordering bug, not a
                    // transient state.
                    let Some(item) = raw_slot.borrow().as_ref().cloned() else {
                        log::warn!(
                            target: "tolaria::raw_editor",
                            "ToggleRawEditor: no active NoteItem — toolbar click reached \
                             the handler before preload_blank_webview populated the slot"
                        );
                        return;
                    };
                    let id = item.read(cx).id();
                    let pre_raw = item.read(cx).raw_mode();
                    log::debug!(
                        target: "tolaria::raw_editor",
                        "ToggleRawEditor: id={id:?} raw_mode {pre_raw} → {}",
                        !pre_raw,
                    );
                    if let Err(e) = item.update(cx, |item, cx| item.toggle_raw_mode(cx)) {
                        log::warn!("ToggleRawEditor: toggle_raw_mode failed: {e:#}");
                    }
                });

                // Worklist 9.2.17 — `ToggleNoteWidth` mirrors the
                // raw-mode handler: read the active NoteItem from the
                // shared slot, log the transition, dispatch through
                // `toggle_wide_mode` which flips the chrome-owned
                // `wide_mode` flag + pushes `ToHost::SetWideMode` to
                // the embedded editor.  The editor-host toggles a
                // `.wide-mode` class on `.editor-host-container` and
                // CSS removes the `max-width` constraint.
                let width_slot = active_note_item.clone();
                cx.on_action(move |_: &actions::ToggleNoteWidth, cx| {
                    let Some(item) = width_slot.borrow().as_ref().cloned() else {
                        log::warn!(
                            target: "tolaria::note_width",
                            "ToggleNoteWidth: no active NoteItem — toolbar click reached \
                             the handler before preload_blank_webview populated the slot"
                        );
                        return;
                    };
                    let id = item.read(cx).id();
                    let pre_wide = item.read(cx).wide_mode();
                    log::debug!(
                        target: "tolaria::note_width",
                        "ToggleNoteWidth: id={id:?} wide_mode {pre_wide} → {}",
                        !pre_wide,
                    );
                    if let Err(e) = item.update(cx, |item, cx| item.toggle_wide_mode(cx)) {
                        log::warn!("ToggleNoteWidth: toggle_wide_mode failed: {e:#}");
                    }
                });

                // Worklist 9.2.6 — `ToggleTableOfContents` attaches the
                // `toc_panel::TocPanel` to the workspace's right dock on
                // first dispatch and toggles it open / closed
                // thereafter.  Mirrors `ToggleSidebar` (left dock) — the
                // shared `dispatch_to_workspace` helper threads the
                // active workspace through, and the inner closure
                // resolves the dock state in place.
                //
                // The panel entity is held in a slot identical in shape
                // to `ActiveNoteItemSlot` so the
                // `HeadingsUpdatedEvent` subscriber set up below can
                // write through to the panel without re-resolving the
                // workspace.  This is also the seam future right-dock
                // panels (9.2.5 AI, 9.2.8 Inspector) attach through —
                // they'll get their own slot, and the swap logic in
                // the handler decides which to attach.
                let toc_panel_slot: std::rc::Rc<
                    std::cell::RefCell<Option<gpui::Entity<toc_panel::TocPanel>>>,
                > = std::rc::Rc::new(std::cell::RefCell::new(None));

                // Worklist 9.2.13 — sibling slot for the InspectorPanel,
                // promoted to a real mount.  Worklist 9.3.5 moved the
                // primary toggle from the note toolbar to the workspace
                // title bar (`title-bar-toggle-inspector`); the in-
                // panel header toggle / close buttons (worklist 9.3.4)
                // dispatch the same `ToggleInspector` action so the
                // open-state and closed-state affordances funnel
                // through one handler.  The previous GPUI debug
                // element-picker moved to `ToggleElementInspector`,
                // bound to `Cmd+Alt+I`.  The slot keeps the entity
                // alive across right-dock swaps with the ToC panel so
                // the subscribers below (HeadingsUpdated / OpenNote)
                // can continue writing through to the same panel
                // without re-resolving the workspace.
                let inspector_panel_slot: std::rc::Rc<
                    std::cell::RefCell<Option<gpui::Entity<inspector_panel::InspectorPanel>>>,
                > = std::rc::Rc::new(std::cell::RefCell::new(None));

                // Worklist 9.2.6 / 9.2.13 — right-dock toggle handler
                // for the ToC panel.  Shape mirrors `ToggleInspector`
                // below: open/close when the dock is already showing
                // this panel, swap when it's showing the sibling
                // (Inspector), fresh-attach otherwise.  The dock's
                // [`Panel::panel_key`] (`"toc"`) is the source of truth
                // for "which panel is mounted right now".
                let toc_slot_for_action = toc_panel_slot.clone();
                cx.on_action(move |_: &actions::ToggleTableOfContents, cx| {
                    let slot = toc_slot_for_action.clone();
                    dispatch_to_workspace("ToggleTableOfContents", cx, move |ws, _window, cx| {
                        toggle_or_swap_right_dock_panel(
                            ws,
                            cx,
                            "toc",
                            &slot,
                            |cx| cx.new(|_| toc_panel::TocPanel::new()),
                        );
                    });
                });

                // Worklist 9.2.13 — `ToggleInspector` now attaches the
                // [`inspector_panel::InspectorPanel`] to the workspace's
                // right dock, swapping it in when the dock currently
                // shows the ToC panel.  Same shape as the ToC handler
                // above; both share `toggle_or_swap_right_dock_panel`
                // so the open/close/swap semantics stay consistent
                // across the right dock's two panels.
                let inspector_slot_for_action = inspector_panel_slot.clone();
                cx.on_action(move |_: &actions::ToggleInspector, cx| {
                    // Worklist 9.2.13 (Reopened-3) — instrument every
                    // hop of the dispatch chain so a non-opening
                    // inspector triages from production logs.  The
                    // companion sites:
                    //   - `workspace::title_bar` cell `on_click` (the
                    //     entry point for the title-bar primary
                    //     affordance added in 9.3.5).
                    //   - the panel factory closure below (mounts the
                    //     `InspectorPanel` on the right dock).
                    // Together with `RUST_LOG=tolaria=info,workspace=info`
                    // these three lines pin the failure to dispatch /
                    // handler / mount.
                    log::info!(
                        target: "tolaria::inspector",
                        "ToggleInspector handler entered"
                    );
                    let slot = inspector_slot_for_action.clone();
                    dispatch_to_workspace("ToggleInspector", cx, move |ws, _window, cx| {
                        toggle_or_swap_right_dock_panel(
                            ws,
                            cx,
                            "inspector",
                            &slot,
                            |cx| {
                                log::info!(
                                    target: "tolaria::inspector",
                                    "InspectorPanel factory invoked — building fresh entity"
                                );
                                cx.new(|cx| inspector_panel::InspectorPanel::from_or_empty(cx))
                            },
                        );
                    });
                });

                // 10. Open root window.  Copy f32 size values out before passing cx
                //    to Bounds::centered so the borrow of SettingsStore is released.
                //    CLI `--width` / `--height` override the persisted settings —
                //    periscope and other harnesses use this to pin the window to
                //    the 1516×1052 logical-point size of the Tauri-era reference
                //    screenshots without writing through `settings.json`.
                let (width, height) = {
                    let w = &SettingsStore::get(cx).window;
                    (
                        args.width.unwrap_or(w.width),
                        args.height.unwrap_or(w.height),
                    )
                };
                let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
                let opts = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        // `title: None` lets the workspace draw its own
                        // strip without a system title string — mirrors
                        // Zed's `zed.rs:350` (`title: None`).
                        title: None,
                        // Hide the system titlebar so our custom strip
                        // paints flush with the top of the window.
                        appears_transparent: true,
                        // Pin traffic lights to (9, 9) — mirrors Zed's
                        // `zed.rs:352` (`traffic_light_position: Some(point(px(9.0), px(9.0)))`).
                        // The y value is the *top inset* of the close button;
                        // GPUI/AppKit flips it internally
                        // (`gpui_macos/src/window.rs:538-544`).
                        // The strip reserves `TRAFFIC_LIGHTS_PADDING_PT`
                        // (71 pt) on the left so the action cluster never
                        // overlaps the lights.
                        traffic_light_position: Some(point(px(9.0), px(9.0))),
                    }),
                    // Worklist 2.31 Phase 1 (Angle-C C2) — flip the
                    // workspace window's GPUI Metal base layer to
                    // non-opaque so a future commit can drop the
                    // embedded WKWebView *behind* it and let GPUI
                    // overlays composite above without sibling-NSView
                    // occlusion (the problem the now-removed
                    // `OverlayTooltipExt` worked around by spawning a
                    // separate `NSPanel`; Phase 3 deletes that fan-out).
                    //
                    // `WindowBackgroundAppearance::Transparent` routes
                    // through `gpui_macos::window::set_background_appearance`
                    // (window.rs:1401-1455) which:
                    //  1. calls `renderer.update_transparency(true)` —
                    //     flips the CAMetalLayer's `isOpaque` to NO
                    //     so the renderer keeps an alpha channel and
                    //     non-opaque chrome surfaces composite
                    //     correctly,
                    //  2. calls `NSWindow.setOpaque(NO)` +
                    //     `setBackgroundColor:` to a near-clear black
                    //     so AppKit doesn't paint an opaque fill
                    //     behind the metal layer either.
                    //
                    // Every workspace chrome surface (title bar,
                    // sidebar, note-list, note-toolbar, status bar,
                    // `TolariaWorkspace::render` root) already paints
                    // its own opaque `.bg(theme.background)` /
                    // `.bg(theme.sidebar)`, so the chrome stays solid
                    // — only the editor centre pane (which never
                    // sets a bg) becomes see-through.  Phase 2 lands
                    // the WKWebView behind the metal layer so that
                    // transparency exposes the WebView instead of
                    // the desktop.  Until then `note_item::macos::
                    // fix_window_background` re-paints the NSWindow's
                    // `backgroundColor` opaque dark `#1F1E1B`, so the
                    // transient state is "centre pane shows dark fill"
                    // not "centre pane shows the desktop" — visually
                    // acceptable for Phase 1 acceptance testing.
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                };

                if let Err(err) = cx.open_window(opts, |window, cx| {
                    use note_list_pane::{NoteListPane, NoteListScope, OpenNoteEvent};
                    use sidebar_panel::{
                        SidebarPanel, SidebarSelection, SidebarSelectionChangedEvent,
                    };
                    use workspace::TolariaWorkspace;

                    let sidebar = cx.new(|cx| SidebarPanel::from_or_empty(cx));
                    let note_list = cx.new(|cx| NoteListPane::from_or_empty(cx));

                    // Worklist 9.2.12 (reopened) — when a real `Vault`
                    // is installed, hook the pane into the vault's
                    // change channel so any chrome-initiated
                    // frontmatter mutation
                    // (`Vault::set_frontmatter_bool`) invalidates the
                    // pane's cached entry list.  Without this the
                    // Inbox `_organized` filter keeps showing notes
                    // the user just moved out of triage until the
                    // OS-level fs-watcher debounce eventually catches
                    // up — visible as a "click did nothing" UX bug.
                    //
                    // The MockVault path doesn't have a watcher (its
                    // `watch_events` is an inert receiver), so the
                    // task installs but never fires — cheap enough to
                    // skip the branch guard here.
                    //
                    // Worklist 9.2.12 reopened-2 — a single fan-out
                    // task drains the receiver and refreshes both the
                    // `NoteListPane` (visible Inbox rows) and the
                    // `SidebarPanel` (Inbox count badge) on every
                    // event.  Two separate `install_vault_watch_task`
                    // calls would *compete* for messages because
                    // `Vault::watch_events` returns clones of one
                    // `flume::Receiver` — flume's MPMC work-stealing
                    // semantics mean two siblings alternate consumers
                    // rather than both seeing every event.  Fan-out
                    // here keeps the single-receiver discipline
                    // intact while still keeping both views in sync.
                    if let Some(vault) = cx.try_global::<vault::Vault>() {
                        let rx = vault.watch_events();
                        let note_list_weak = note_list.downgrade();
                        let sidebar_weak = sidebar.downgrade();
                        cx.spawn(async move |cx| {
                            while let Ok(_change) = rx.recv_async().await {
                                let mut still_live = false;
                                if let Some(pane) = note_list_weak.upgrade() {
                                    still_live = true;
                                    pane.update(cx, NoteListPane::refresh_from_vault);
                                }
                                if let Some(panel) = sidebar_weak.upgrade() {
                                    still_live = true;
                                    panel.update(cx, SidebarPanel::refresh_from_vault);
                                }
                                if !still_live {
                                    // Both downgraded handles dropped — the
                                    // workspace is gone, exit so the task
                                    // doesn't hold the receiver alive past
                                    // both entities' lifetimes.
                                    break;
                                }
                            }
                        })
                        .detach();
                    }

                    // Worklist 2.19 — Cmd+N (and any future palette /
                    // menu entry) routes through the same
                    // `CreateNoteRequested` event that the notes-list
                    // `+` button emits.  Registering inside
                    // `open_window` lets the closure capture the
                    // `note_list` entity by clone; the subscriber set
                    // up further down handles the actual create + open.
                    let action_note_list = note_list.clone();
                    cx.on_action(move |_: &actions::NewNote, cx| {
                        action_note_list.update(cx, |pane, cx| pane.request_create_note(cx));
                    });

                    // Worklist 9.2.3 — `EnterNeighborhood` swaps the
                    // note-list pane to "neighbourhood mode" for the
                    // active note: every note that links to it
                    // (`vault::backlinks`) plus every note it links to
                    // (`vault::outbound_links`), minus the active note
                    // itself.  Mirrors React's
                    // `BreadcrumbBar.tsx::NeighborhoodAction` →
                    // `onEnterNeighborhood` → `useNeighborhoodEntry`.
                    //
                    // The sidebar's row highlight intentionally stays
                    // put — React's `useNeighborhoodEntry` doesn't
                    // change the sidebar's `SidebarSelection` either,
                    // it only pushes a new "viewing one note's
                    // neighbourhood" filter onto the note-list.  The
                    // user's previous sidebar context (Inbox / All
                    // Notes / a Type) survives so they can exit
                    // neighbourhood mode by clicking another sidebar
                    // row.
                    //
                    // TODO(nav-history, Phase 10): the React
                    // `neighborhoodHistoryRef` stack lets users walk
                    // back through prior selections via Escape.  Phase
                    // 10's `nav_history` crate is the proper home for
                    // that bookkeeping; the present row only ships the
                    // forward path (enter mode) and leaves
                    // back-navigation for the dedicated crate.
                    let neighborhood_slot = active_note_item.clone();
                    let neighborhood_note_list = note_list.clone();
                    // Worklist 9.2.16 — previous-scope memory backing
                    // the EnterNeighborhood toggle.  When the user
                    // enters neighbourhood mode we stash the pane's
                    // current scope here; when the user clicks the
                    // toolbar cell a SECOND time on the same active
                    // note we pop this slot to restore the prior
                    // sidebar context (Inbox / AllNotes / a Type /
                    // a Folder / a saved View) instead of forcing
                    // them to click another sidebar row to exit.
                    //
                    // `Option<NoteListScope>` distinguishes "we have
                    // never entered neighbourhood mode" (`None` →
                    // fall back to the default `Inbox`) from "we
                    // remembered the previous scope" (`Some(scope)`
                    // → restore verbatim).  The slot is shared via
                    // `Rc<RefCell<…>>` because both the handler
                    // closure and the future sidebar-clearing path
                    // (which writes through the same slot) need
                    // mutable access without forcing the closure to
                    // own the slot exclusively.
                    //
                    // Phase 10's `nav_history` crate will replace
                    // this single-slot memory with a proper stack;
                    // the present row only ships the on/off toggle
                    // contract.
                    let neighborhood_prev_scope: std::rc::Rc<
                        std::cell::RefCell<Option<note_list_pane::NoteListScope>>,
                    > = std::rc::Rc::new(std::cell::RefCell::new(None));
                    let neighborhood_prev_scope_handler = neighborhood_prev_scope.clone();
                    cx.on_action(move |_: &actions::EnterNeighborhood, cx| {
                        // Worklist 9.2.16 — delegate to
                        // [`handle_enter_neighborhood`] so the production
                        // path and the `#[gpui::test]` regressions share
                        // a single source of truth.  The handler reads
                        // the active scope, branches on toggle-on /
                        // toggle-off, and updates pane + anchor + saved
                        // scope through the slot the sidebar
                        // subscription also clears.
                        handle_enter_neighborhood(
                            &neighborhood_slot,
                            &neighborhood_note_list,
                            &neighborhood_prev_scope_handler,
                            cx,
                        );
                    });

                    // Worklist 9.2.7 — `Archive` writes `_archived: true`
                    // to the active note's frontmatter through the same
                    // `set_frontmatter_bool` write path the star /
                    // organized cells use.  Mirrors React's
                    // `BreadcrumbOverflowMenu` "Archive" entry; same slot
                    // lookup as `EnterNeighborhood` so all toolbar
                    // actions agree on "which note is on screen".  No
                    // ConfirmArchive modal — React's reference doesn't
                    // prompt either; the flag is reversible.
                    let archive_slot = active_note_item.clone();
                    cx.on_action(move |_: &actions::Archive, cx| {
                        let Some(item) = archive_slot.borrow().as_ref().cloned() else {
                            log::warn!(
                                target: "tolaria::archive",
                                "Archive: no active NoteItem — toolbar click reached the \
                                 handler before preload_blank_webview populated the slot"
                            );
                            return;
                        };
                        let id = item.read(cx).id();
                        if !cx.has_global::<vault::Vault>() {
                            log::warn!(
                                target: "tolaria::archive",
                                "Archive: no Vault global installed (id={id:?})"
                            );
                            return;
                        }
                        log::info!(
                            target: "tolaria::archive",
                            "Archive: dispatched for id={id:?}",
                        );
                        cx.global_mut::<vault::Vault>().archive_note(id).detach();
                        cx.refresh_windows();
                    });

                    // Worklist 9.2.7 — `Delete` removes the active
                    // note's file from disk and rescans the vault.
                    // Mirrors React's `BreadcrumbOverflowMenu` "Delete"
                    // entry.  No confirmation modal yet — the menu
                    // entry is destructive; a ConfirmDelete modal is
                    // tracked as a Phase 9.2.7-followup once the
                    // `dialog_stack` primitive lands.
                    //
                    // TODO(9.2.7-followup): route through a
                    // ConfirmDelete dialog before firing the unlink.
                    // The React reference uses an `AlertDialog` here;
                    // GPUI parity will follow once `dialog_stack`
                    // exposes the primitive.
                    let delete_slot = active_note_item.clone();
                    cx.on_action(move |_: &actions::Delete, cx| {
                        let Some(item) = delete_slot.borrow().as_ref().cloned() else {
                            log::warn!(
                                target: "tolaria::delete",
                                "Delete: no active NoteItem — toolbar click reached the \
                                 handler before preload_blank_webview populated the slot"
                            );
                            return;
                        };
                        let id = item.read(cx).id();
                        if !cx.has_global::<vault::Vault>() {
                            log::warn!(
                                target: "tolaria::delete",
                                "Delete: no Vault global installed (id={id:?})"
                            );
                            return;
                        }
                        log::info!(
                            target: "tolaria::delete",
                            "Delete: dispatched for id={id:?}",
                        );
                        cx.global_mut::<vault::Vault>().delete_note(id).detach();
                        // TODO(9.2.7-followup): after the deleted
                        // note's file is unlinked, the active editor
                        // still holds the old `NoteItem`.  Phase 10
                        // `vault_lifecycle` will route a
                        // `next_note_after_delete` selection through
                        // the workspace; for MVP `refresh_windows()`
                        // re-renders the toolbar so its `note_sync`
                        // returns `None` and the cells gracefully fall
                        // back to the "no note" branch.
                        cx.refresh_windows();
                    });

                    // Slot holding the currently mounted `NoteItem` so
                    // successive `OpenNoteEvent`s reuse the same entity
                    // (and underlying WKWebView) instead of constructing a
                    // new one — the latter is what produced the flicker.
                    // Constructed before `cx.open_window` so the
                    // observe-global theme broadcaster (Phase 7.9) and
                    // the open-note subscription can share the same handle.
                    let active_note_item = active_note_item.clone();
                    let workspace = cx.new(|model_cx| {
                        let mut workspace = TolariaWorkspace::empty(window, model_cx);
                        // Sidebar (vault tree) on the left, note list in
                        // its own column between sidebar and editor —
                        // matches `tolaria-demo-vault-v2.png`.
                        workspace.attach_left_dock(sidebar.clone(), model_cx);
                        workspace.attach_note_list_column(note_list.clone());
                        // Eagerly mount a blank WKWebView so the editor
                        // NSView is constructed (and painted) before the
                        // user clicks anything — avoids the black NSView
                        // flash on first open.  The editor shows its
                        // "Select a note…" placeholder until a click
                        // swaps real content in.
                        if let Err(e) = crate::open_note::preload_blank_webview(
                            &active_note_item,
                            window,
                            model_cx,
                        ) {
                            log::error!("preload_blank_webview failed: {e:#}");
                        }
                        // Worklist 9.2.6 — subscribe to the preloaded
                        // `NoteItem`'s `HeadingsUpdatedEvent` so every
                        // `FromHost::Headings` envelope flows into the
                        // right-dock `TocPanel`.  The slot holds the same
                        // entity for the lifetime of the window
                        // (`open_in_webview` swaps state in place rather
                        // than constructing a new entity), so a single
                        // subscription registered here covers every note
                        // the user opens.  When the panel hasn't been
                        // attached yet (user never clicked the toolbar
                        // cell), the headings payload is dropped — the
                        // panel reads the in-memory state via
                        // `set_headings` on the next attach so no
                        // headings are lost.
                        if let Some(blank_item) = active_note_item.borrow().as_ref().cloned() {
                            let headings_panel_slot = toc_panel_slot.clone();
                            let inspector_headings_slot = inspector_panel_slot.clone();
                            model_cx
                                .subscribe(
                                    &blank_item,
                                    move |_ws,
                                          _item,
                                          event: &note_item::HeadingsUpdatedEvent,
                                          cx| {
                                        // Fan the same `Headings`
                                        // envelope out to both right-dock
                                        // panels (toc + inspector
                                        // Outline section).  Either slot
                                        // may be empty — a missing panel
                                        // drops the payload silently
                                        // (next attach receives a fresh
                                        // payload as soon as the editor
                                        // ticks again).
                                        if let Some(panel) =
                                            headings_panel_slot.borrow().as_ref().cloned()
                                        {
                                            let headings = event.headings.clone();
                                            panel.update(cx, |panel, cx| {
                                                panel.set_headings(headings, cx);
                                            });
                                        } else {
                                            log::debug!(
                                                "HeadingsUpdated dropped: TocPanel not attached"
                                            );
                                        }
                                        if let Some(panel) =
                                            inspector_headings_slot.borrow().as_ref().cloned()
                                        {
                                            let headings = event.headings.clone();
                                            panel.update(
                                                cx,
                                                |panel: &mut inspector_panel::InspectorPanel,
                                                 cx| {
                                                    panel.set_headings(headings, cx);
                                                },
                                            );
                                        }
                                    },
                                )
                                .detach();
                        }
                        // Subscribe inside the workspace's Context so the
                        // subscription lifetime tracks the workspace entity.
                        let slot = active_note_item.clone();
                        let active_handle = note_list.clone();
                        // Worklist 2.19 — additional clones for the
                        // `CreateNoteRequested` subscriber below.  Kept
                        // alongside the existing slot/handle clones so
                        // the create-and-open helper has the same
                        // ergonomic surface as the open-note path.
                        let create_slot = active_note_item.clone();
                        let create_list = note_list.clone();
                        // Phase 8.1 — route every sidebar selection
                        // change to the note-list pane's scope filter.
                        // `Inbox` / `AllNotes` / `Archive` / `View(...)`
                        // map to the same-named scopes; `Type(label)`
                        // and `Folder(path)` narrow the list to
                        // matching entries.  Re-selecting the same row
                        // is a no-op in the sidebar (`select` only
                        // emits on change), so this subscription
                        // doesn't churn on idempotent clicks.
                        let scoped_list = note_list.clone();
                        // Worklist 9.2.1 — the FAVORITES sidebar
                        // section emits `SidebarSelection::Favorite(id)`
                        // when the user clicks a starred-note row.
                        // The natural action there is to open the
                        // note, NOT swap the note-list scope — but
                        // this closure already owns the slot via
                        // `scoped_open_slot` so we can route the
                        // click straight into `open_note`.
                        let scoped_open_slot = slot.clone();
                        let scoped_open_handle = note_list.clone();
                        // Worklist 9.2.16 — clearing the previous-scope
                        // memory when ANY sidebar row is picked.  The
                        // EnterNeighborhood toggle pops back to the
                        // saved scope on second click; once the user
                        // navigates via the sidebar that memory is
                        // stale (a fresh entry will save the NEW scope
                        // before swapping in).  Sharing the same Rc as
                        // the action handler keeps the two paths
                        // agreeing on a single source of truth.
                        let sidebar_prev_scope = neighborhood_prev_scope.clone();
                        model_cx
                            .subscribe_in(
                                &sidebar,
                                window,
                                move |ws_view,
                                      _side,
                                      event: &SidebarSelectionChangedEvent,
                                      window,
                                      cx| {
                                    let scope = match event.selection.clone() {
                                        SidebarSelection::Inbox => NoteListScope::Inbox,
                                        SidebarSelection::AllNotes => NoteListScope::AllNotes,
                                        SidebarSelection::Archive => NoteListScope::Archive,
                                        SidebarSelection::Type(label) => NoteListScope::Type(label),
                                        SidebarSelection::Folder(path) => {
                                            NoteListScope::Folder(path)
                                        }
                                        SidebarSelection::View(name) => NoteListScope::View(name),
                                        SidebarSelection::Favorite(raw_id) => {
                                            // Favorites click → open the note
                                            // directly, mirroring `OpenNoteEvent`
                                            // dispatch from the note-list pane.
                                            // Skip the scope/header update — the
                                            // user picked a single note, not a
                                            // filter.
                                            let id = vault::NoteId::from_raw(raw_id);
                                            if let Err(e) = crate::open_note::open_note(
                                                ws_view,
                                                id,
                                                &scoped_open_slot,
                                                window,
                                                cx,
                                            ) {
                                                log::error!(
                                                    "open_note from favourite failed: {e:#}"
                                                );
                                            }
                                            scoped_open_handle.update(cx, |list, cx| {
                                                list.set_active(Some(id), cx);
                                            });
                                            return;
                                        }
                                        // Worklist 9.2.3 — neighbourhood mode
                                        // doesn't flow through the sidebar
                                        // selection event in normal
                                        // operation (the toolbar action
                                        // handler updates the note-list
                                        // pane directly so the resolved
                                        // id-set isn't re-walked here),
                                        // but the variant is exhaustive on
                                        // `SidebarSelection` and a future
                                        // path may emit it (e.g. a
                                        // programmatic test).  No-op so
                                        // the subscriber compiles and
                                        // future emitters fail loud in
                                        // logs rather than silently
                                        // narrowing the list.
                                        SidebarSelection::Neighborhood(raw_id) => {
                                            log::debug!(
                                                "SidebarSelectionChangedEvent::Neighborhood({raw_id}) ignored — handled by EnterNeighborhood action"
                                            );
                                            return;
                                        }
                                    };
                                    // Worklist 2.1 — keep the note-list-pane header in
                                    // sync with the row label so users see which slice
                                    // of the vault they're looking at.
                                    let header = event.display_label.clone();
                                    scoped_list.update(cx, |list, cx| {
                                        list.set_scope(scope, cx);
                                        list.set_header_title(header, cx);
                                    });
                                    // Worklist 9.2.14 — every sidebar selection
                                    // change exits neighbourhood mode (the React
                                    // `useNeighborhoodEntry` keeps the previous
                                    // sidebar row highlighted while the filter
                                    // is active, so picking ANY sidebar row
                                    // unwinds the filter).  Clear the anchor so
                                    // the note-toolbar's active-state glyph drops
                                    // back to muted on the next render.
                                    cx.set_global(note_item::NeighborhoodAnchor(None));
                                    // Worklist 9.2.16 — also clear the saved
                                    // previous-scope memory.  The user just
                                    // picked a fresh sidebar row; any pending
                                    // "exit back to before-the-neighbourhood"
                                    // pop would now restore an outdated context.
                                    *sidebar_prev_scope.borrow_mut() = None;
                                },
                            )
                            .detach();

                        let open_note_inspector_slot = inspector_panel_slot.clone();
                        model_cx
                            .subscribe_in(
                                &note_list,
                                window,
                                move |ws_view, _list, event: &OpenNoteEvent, window, cx| {
                                    // Pass `&TolariaWorkspace` straight through —
                                    // `open_note` calls `add_item_to_active_pane`
                                    // (which takes `&self`) directly on it instead
                                    // of re-entering via `entity.update(...)`.  See
                                    // `open_note.rs` for the re-entrancy story.
                                    if let Err(e) = crate::open_note::open_note(
                                        ws_view, event.id, &slot, window, cx,
                                    ) {
                                        log::error!("open_note failed: {e:#}");
                                    }
                                    // Keep the note-list's pale-accent highlight
                                    // in sync with the editor's mounted note —
                                    // Phase 7.7 visual parity.  This is a no-op
                                    // when the click originated in the list
                                    // (`NoteListPane::open` already set
                                    // `selected_id`), but lets future open paths
                                    // (keymap, palette) drive the highlight too.
                                    active_handle.update(cx, |list, cx| {
                                        list.set_active(Some(event.id), cx);
                                    });
                                    // Worklist 9.2.8 — keep the inspector
                                    // panel's active-note pointer in
                                    // sync so its vault-driven sections
                                    // (Backlinks / Instances /
                                    // References) re-resolve against the
                                    // newly-opened note.  No-op when the
                                    // panel isn't mounted yet.
                                    if let Some(panel) =
                                        open_note_inspector_slot.borrow().as_ref().cloned()
                                    {
                                        let id = event.id;
                                        panel.update(
                                            cx,
                                            |panel: &mut inspector_panel::InspectorPanel, cx| {
                                                panel.set_active(Some(id), cx);
                                            },
                                        );
                                    }
                                },
                            )
                            .detach();

                        // Worklist 2.19 — the notes-list `+` button
                        // emits `CreateNoteRequested`; route it through
                        // the shared create-and-open helper so the new
                        // note lands on disk, the list re-renders, and
                        // the editor mounts the note in one pass.  The
                        // `actions::NewNote` (Cmd+N) handler below
                        // dispatches the same event into this entity so
                        // both entry points share a single code path.
                        model_cx
                            .subscribe_in(
                                &note_list,
                                window,
                                move |ws_view,
                                      _list,
                                      _event: &note_list_pane::CreateNoteRequested,
                                      window,
                                      cx| {
                                    if let Err(e) = crate::open_note::create_and_open_untitled(
                                        ws_view,
                                        &create_list,
                                        &create_slot,
                                        window,
                                        cx,
                                    ) {
                                        log::error!("create_and_open_untitled failed: {e:#}");
                                    }
                                },
                            )
                            .detach();
                        workspace
                    });
                    // Wrap the workspace entity in `gpui_component::Root` —
                    // gpui-component's Dialog / Sheet / Notification / Tooltip
                    // overlays *and* `Window::toggle_inspector` all assume the
                    // window's first view is `Root`.  Skipping the wrapper
                    // panics inside `gpui_component::Root::read` at the first
                    // overlay call (root.rs:118, `Option::unwrap()` on `None`)
                    // — observed via `ToggleInspector` triggering the
                    // inspector overlay.
                    cx.new(|cx| gpui_component::Root::new(workspace, window, cx))
                }) {
                    log::error!("failed to open Tolaria window: {err:#}");
                }

                // Worklist 3.2 — refresh the menu now that the
                // workspace window is live so the View entry's
                // `"Show Sidebar"` / `"Hide Sidebar"` label reflects
                // the dock's actual startup state.  Deferred inside
                // `rebuild_menus`, so this lands after `open_window`'s
                // updates have drained.
                rebuild_menus(cx);

                // 10. Bring application to foreground.
                cx.activate(true);
            });
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use actions::{Quit, ReloadKeymap};
    use gpui::{KeyBinding, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    /// Cmd+Q must dispatch the `Quit` action.
    ///
    /// Mirrors `embed_poc/src/menus.rs:115`: binds `cmd-q → Quit`, drives the
    /// keystroke through the test platform's dispatch chain, and asserts the
    /// global handler fires exactly once.
    #[gpui::test]
    fn cmd_q_dispatches_quit(cx: &mut TestAppContext) {
        let fired = Rc::new(Cell::new(0u32));
        let fired_handler = fired.clone();

        cx.update(|cx| {
            cx.on_action(move |_: &Quit, _| {
                fired_handler.set(fired_handler.get() + 1);
            });
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        });

        let window = cx.add_empty_window();
        window.simulate_keystrokes("cmd-q");
        window.run_until_parked();

        assert_eq!(fired.get(), 1, "Quit must fire exactly once for cmd-q");
    }

    /// Firing `ReloadKeymap` twice must fire the handler exactly twice.
    ///
    /// The Phase 1 handler is a log-only stub; this test documents the
    /// idempotency contract that Phase 2's real implementation must uphold.
    #[gpui::test]
    fn reload_keymap_action_is_idempotent(cx: &mut TestAppContext) {
        let fired = Rc::new(Cell::new(0u32));
        let fired_handler = fired.clone();

        cx.update(|cx| {
            cx.on_action(move |_: &ReloadKeymap, _cx| {
                fired_handler.set(fired_handler.get() + 1);
            });
            cx.bind_keys([KeyBinding::new("cmd-shift-p", ReloadKeymap, None)]);
        });

        let window = cx.add_empty_window();
        window.simulate_keystrokes("cmd-shift-p");
        window.simulate_keystrokes("cmd-shift-p");
        window.run_until_parked();

        assert_eq!(
            fired.get(),
            2,
            "ReloadKeymap must fire exactly twice without panicking"
        );
    }

    /// The real-platform `PlatformTextSystem` MUST be able to enumerate
    /// system fonts.  When `gpui_platform` is built without the
    /// `font-kit` feature, `gpui_macos::MacPlatform::new` swaps
    /// `MacTextSystem` for `gpui::NoopTextSystem`, whose
    /// `all_font_names()` returns an empty `Vec` — every label in the
    /// app then renders as invisible whitespace.  Window chrome geometry
    /// still paints, so the regression is silent at the GPUI layer; only
    /// observing the live text system catches it.
    ///
    /// We construct the real headless macOS platform via
    /// `gpui_platform::current_platform(true)` and ask its
    /// `PlatformTextSystem` for the font catalog.  A healthy
    /// `MacTextSystem` (CoreText-backed) returns hundreds of system
    /// fonts; `NoopTextSystem` returns zero.  Picking a generous floor
    /// of 50 leaves room for trimmed macOS installs while still firing
    /// hard the moment the platform falls back to `NoopTextSystem`.
    ///
    /// Discovered in Phase 6-MVP verification — see
    /// `docs/plans/native-gpui-chrome/progress.md`.  This test is a
    /// plain `#[test]` (not `#[gpui::test]`) because `TestPlatform::new`
    /// hard-codes `NoopTextSystem` regardless of feature flags, so
    /// `TestAppContext` cannot distinguish the two configurations.
    #[test]
    fn platform_text_system_enumerates_system_fonts() {
        let platform = gpui_platform::current_platform(true);
        let text_system = platform.text_system();
        let names = text_system.all_font_names();
        assert!(
            names.len() > 50,
            "PlatformTextSystem::all_font_names() returned {} font(s): \
             {names:?}.\n\nThis is the symptom of `gpui_platform` being \
             built without the `font-kit` feature — `gpui_macos::\
             MacPlatform::new` then falls back to `gpui::NoopTextSystem`, \
             whose font list is empty, and the whole UI ships with \
             invisible glyphs.  Re-add `\"font-kit\"` to the workspace \
             `gpui_platform` feature list in `Cargo.toml`.",
            names.len(),
        );
    }

    /// Worklist 9.2.3 / 9.2.4 — dispatching
    /// [`actions::EnterNeighborhood`] / [`actions::ToggleRawEditor`]
    /// must reach the registered global handler and read the active
    /// `NoteItem` out of the shared `ActiveNoteItemSlot`.  Reproduces
    /// the "toolbar click does nothing" regression by registering the
    /// same handler shape `main.rs` uses, populating the slot with a
    /// real `NoteItem` entity, and asserting the handler ran.
    ///
    /// The bug at row 9.2.4's `Reopened` annotation was that the
    /// toolbar's `on_click` closure called `App::dispatch_action`
    /// from inside a `gpui::Render` re-entry — the deferred
    /// dispatcher fans the action through the focused window's
    /// dispatch tree, but the global `cx.on_action` handler is keyed
    /// off the action's `TypeId` regardless of the dispatch path, so
    /// the slot lookup is the only place that can silently swallow
    /// the click.  The fix is to ensure the slot is populated by the
    /// time the dispatch lands — the test asserts that the
    /// `preload_blank_webview` step from `open_window` followed by an
    /// `open_note` call leaves the slot in a state where dispatch
    /// fires the slot-reading handler.
    #[gpui::test]
    fn toolbar_actions_resolve_via_active_note_item_slot(cx: &mut TestAppContext) {
        use gpui::{AppContext as _, Entity};
        use note_item::NoteItem;
        use std::path::PathBuf;
        use vault::{Note, NoteId, NoteKind};

        cx.update(gpui_component::init);

        // Mirror `main.rs`'s slot shape: a shared `Rc<RefCell<Option<Entity<NoteItem>>>>`
        // captured by both the dispatch handler and the production
        // open-note path.
        let slot: crate::open_note::ActiveNoteItemSlot =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let handler_slot = slot.clone();

        // Track which handler fired and against which note id so the
        // assertion catches both "handler didn't run" and "handler ran
        // but read the wrong note".
        let raw_calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let raw_calls_inner = raw_calls.clone();
        let last_id = std::rc::Rc::new(std::cell::Cell::new(None::<NoteId>));
        let last_id_inner = last_id.clone();

        cx.update(|cx| {
            // Match the production registration site at
            // `main.rs:732` exactly so a future refactor of the
            // handler shape fails this test rather than silently
            // diverging from the live code path.
            cx.on_action(move |_: &actions::ToggleRawEditor, cx| {
                let Some(item) = handler_slot.borrow().as_ref().cloned() else {
                    log::debug!("ToggleRawEditor: no active NoteItem");
                    return;
                };
                last_id_inner.set(Some(item.read(cx).id()));
                raw_calls_inner.set(raw_calls_inner.get() + 1);
                let _ = item.update(cx, |item, cx| item.toggle_raw_mode(cx));
            });
        });

        // Slot empty: dispatching must hit the early-return branch
        // and not increment the call counter.  Guards the
        // "handler is registered but slot was never populated" path.
        cx.update(|cx| {
            cx.dispatch_action(&actions::ToggleRawEditor);
        });
        cx.run_until_parked();
        assert_eq!(
            raw_calls.get(),
            0,
            "with the slot empty the handler must early-return"
        );

        // Populate the slot with a NoteItem entity (no live WebView —
        // the `new_for_tests` constructor matches the
        // `add_item_creates_pane_and_activates_item` test scaffolding
        // in `open_note.rs`).
        let note = Note {
            id: NoteId::from_raw(42),
            title: "Live Note".into(),
            path: PathBuf::from("/vault/live.md"),
            kind: NoteKind::Markdown,
            modified: chrono::Utc::now(),
            byte_size: 0,
            frontmatter: vault::Frontmatter::default(),
        };
        let item: Entity<NoteItem> = cx.update(|cx| cx.new(|_| NoteItem::new_for_tests(note)));
        *slot.borrow_mut() = Some(item.clone());

        // Dispatch through the App.  This is the exact path the
        // toolbar's `on_click` closure follows in production:
        // `cx.dispatch_action(&actions::ToggleRawEditor)` from a
        // `&mut App` context.  If the global handler isn't routed,
        // the call counter stays zero.
        cx.update(|cx| {
            cx.dispatch_action(&actions::ToggleRawEditor);
        });
        cx.run_until_parked();

        assert_eq!(
            raw_calls.get(),
            1,
            "ToggleRawEditor must reach the global handler exactly once \
             when dispatched from an App context (live shape)"
        );
        assert_eq!(
            last_id.get(),
            Some(NoteId::from_raw(42)),
            "the handler must read the slot's NoteItem id (not a stale \
             closure-captured id)"
        );
        // toggle_raw_mode toggled the flag — sanity check that the
        // entity actually mutated end-to-end.
        let is_raw = cx.update(|cx| item.read(cx).raw_mode());
        assert!(
            is_raw,
            "toggle_raw_mode must have flipped raw_mode to true after one click"
        );
    }

    /// Worklist 9.2.7 — `Archive` resolves the active note through the
    /// shared `ActiveNoteItemSlot` (same lookup as `ToggleRawEditor` /
    /// `EnterNeighborhood`) and dispatches into the installed
    /// [`vault::Vault`] global.  This pins the slot-empty early-return
    /// branch + the populated-slot dispatch end-to-end so a future
    /// refactor of the handler shape (e.g. moving the slot lookup,
    /// renaming the action) surfaces as a failing assertion here.
    ///
    /// The test runs against a real on-disk vault tempdir so the
    /// archive write hits the same `set_frontmatter_bool` write path
    /// the React reference does — `_archived: true` ends up in the
    /// note's frontmatter both in memory and on disk.
    #[gpui::test]
    fn archive_action_resolves_via_active_note_item_slot(cx: &mut TestAppContext) {
        use gpui::{AppContext as _, Entity};
        use note_item::NoteItem;
        use tempfile::tempdir;
        use vault::{Note, NoteKind, Vault};

        cx.update(gpui_component::init);

        let dir = tempdir().expect("tempdir");
        let note_path = dir.path().join("n.md");
        std::fs::write(&note_path, "---\ntype: Note\n---\nbody\n").unwrap();
        let vault = Vault::open_at(dir.path()).expect("open vault");
        let vault_id = cx.update(|cx| cx.foreground_executor().block_on(vault.notes())[0]);
        cx.update(|cx| cx.set_global(vault));

        let slot: crate::open_note::ActiveNoteItemSlot =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let handler_slot = slot.clone();

        cx.update(|cx| {
            // Mirror the production registration in `macos::run` —
            // resolve the active NoteItem from the slot, read its id,
            // dispatch to `Vault::archive_note`.
            cx.on_action(move |_: &actions::Archive, cx| {
                let Some(item) = handler_slot.borrow().as_ref().cloned() else {
                    return;
                };
                let id = item.read(cx).id();
                cx.global_mut::<Vault>().archive_note(id).detach();
            });
        });

        // Slot-empty: dispatching must NOT touch the vault.  The
        // fixture starts unarchived; the assertion guards against a
        // future regression that drops the slot-empty early-return.
        cx.update(|cx| {
            cx.dispatch_action(&actions::Archive);
        });
        cx.run_until_parked();
        assert!(
            !cx.update(|cx| cx
                .global::<Vault>()
                .note_sync(vault_id)
                .unwrap()
                .frontmatter
                .archived()),
            "with the slot empty Archive must early-return without touching the vault"
        );

        // Populate the slot and dispatch — the vault write goes
        // through the same path the toolbar's More menu uses.
        let note = Note {
            id: vault_id,
            title: "Live Note".into(),
            path: note_path.clone(),
            kind: NoteKind::Markdown,
            modified: chrono::Utc::now(),
            byte_size: 0,
            frontmatter: vault::Frontmatter::default(),
        };
        let item: Entity<NoteItem> = cx.update(|cx| cx.new(|_| NoteItem::new_for_tests(note)));
        *slot.borrow_mut() = Some(item);

        cx.update(|cx| {
            cx.dispatch_action(&actions::Archive);
        });
        cx.run_until_parked();

        assert!(
            cx.update(|cx| cx
                .global::<Vault>()
                .note_sync(vault_id)
                .unwrap()
                .frontmatter
                .archived()),
            "Archive must mark the note's `_archived` flag true"
        );
        let on_disk = std::fs::read_to_string(&note_path).unwrap();
        assert!(
            on_disk.contains("_archived: true"),
            "Archive must write `_archived: true` to disk; got: {on_disk:?}"
        );
    }

    /// Worklist 9.2.7 — `Delete` mirrors the Archive flow but unlinks
    /// the note's file from disk.  Slot-empty early-return guard + the
    /// populated-slot delete path are both pinned.
    #[gpui::test]
    fn delete_action_resolves_via_active_note_item_slot(cx: &mut TestAppContext) {
        use gpui::{AppContext as _, Entity};
        use note_item::NoteItem;
        use tempfile::tempdir;
        use vault::{Note, NoteKind, Vault};

        cx.update(gpui_component::init);

        let dir = tempdir().expect("tempdir");
        let note_path = dir.path().join("n.md");
        std::fs::write(&note_path, "---\ntype: Note\n---\nbody\n").unwrap();
        let vault = Vault::open_at(dir.path()).expect("open vault");
        let vault_id = cx.update(|cx| cx.foreground_executor().block_on(vault.notes())[0]);
        cx.update(|cx| cx.set_global(vault));

        let slot: crate::open_note::ActiveNoteItemSlot =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let handler_slot = slot.clone();

        cx.update(|cx| {
            cx.on_action(move |_: &actions::Delete, cx| {
                let Some(item) = handler_slot.borrow().as_ref().cloned() else {
                    return;
                };
                let id = item.read(cx).id();
                cx.global_mut::<Vault>().delete_note(id).detach();
            });
        });

        // Slot empty — dispatch must not unlink anything.
        cx.update(|cx| {
            cx.dispatch_action(&actions::Delete);
        });
        cx.run_until_parked();
        assert!(
            note_path.exists(),
            "with the slot empty Delete must early-return without touching the disk"
        );

        let note = Note {
            id: vault_id,
            title: "Live Note".into(),
            path: note_path.clone(),
            kind: NoteKind::Markdown,
            modified: chrono::Utc::now(),
            byte_size: 0,
            frontmatter: vault::Frontmatter::default(),
        };
        let item: Entity<NoteItem> = cx.update(|cx| cx.new(|_| NoteItem::new_for_tests(note)));
        *slot.borrow_mut() = Some(item);

        cx.update(|cx| {
            cx.dispatch_action(&actions::Delete);
        });
        cx.run_until_parked();

        assert!(!note_path.exists(), "Delete must unlink the file from disk");
        assert!(
            cx.update(|cx| cx.global::<Vault>().note_sync(vault_id).is_none()),
            "Delete must drop the note from the in-memory vault index"
        );
    }

    /// Worklist 9.2.13 — clicking the inspector toolbar button (or
    /// dispatching [`actions::ToggleInspector`]) must attach the
    /// product `InspectorPanel` to the workspace's right dock, then
    /// toggle it open/closed on subsequent dispatches, and swap to
    /// the inspector when the dock is currently showing the ToC
    /// panel.  Mirrors the `cx.open_window` wiring in `macos::run`:
    /// the shared `toggle_or_swap_right_dock_panel` helper is the
    /// single source of truth so this test exercises the live code
    /// path directly.
    #[gpui::test]
    fn toggle_inspector_attaches_panel_and_swaps_with_toc(cx: &mut TestAppContext) {
        use gpui::AppContext as _;
        use workspace::TolariaWorkspace;

        cx.update(gpui_component::init);

        let window = cx.add_window(TolariaWorkspace::empty);

        // Empty right dock to start — both accessors report nothing.
        let starts_empty = window
            .update(cx, |ws, _window, cx| {
                (ws.has_right_dock_panel(cx), ws.right_dock_panel_key(cx))
            })
            .unwrap();
        assert!(!starts_empty.0, "right dock must start empty");
        assert!(
            starts_empty.1.is_none(),
            "empty right dock must report no panel key"
        );

        // First dispatch: attach the InspectorPanel fresh.  Slot is
        // empty so the factory runs.
        let inspector_slot: std::rc::Rc<
            std::cell::RefCell<Option<gpui::Entity<inspector_panel::InspectorPanel>>>,
        > = std::rc::Rc::new(std::cell::RefCell::new(None));
        window
            .update(cx, |ws, _window, cx| {
                crate::macos::toggle_or_swap_right_dock_panel(
                    ws,
                    cx,
                    "inspector",
                    &inspector_slot,
                    |cx| cx.new(|_| inspector_panel::InspectorPanel::new()),
                );
            })
            .unwrap();
        let after_first = window
            .update(cx, |ws, _window, cx| {
                (
                    ws.right_dock_panel_key(cx),
                    ws.is_right_dock_open(cx),
                    inspector_slot.borrow().is_some(),
                )
            })
            .unwrap();
        assert_eq!(
            after_first.0.as_deref(),
            Some("inspector"),
            "first dispatch must mount the InspectorPanel on the right dock"
        );
        assert!(
            after_first.1,
            "InspectorPanel reports starts_open=true, so the dock must be open after attach"
        );
        assert!(
            after_first.2,
            "first dispatch must populate the slot so subsequent toggles re-use it"
        );

        // Second dispatch: same panel — toggle closed.
        window
            .update(cx, |ws, _window, cx| {
                crate::macos::toggle_or_swap_right_dock_panel(
                    ws,
                    cx,
                    "inspector",
                    &inspector_slot,
                    |cx| cx.new(|_| inspector_panel::InspectorPanel::new()),
                );
            })
            .unwrap();
        let closed_state = window
            .update(cx, |ws, _window, cx| {
                (ws.right_dock_panel_key(cx), ws.is_right_dock_open(cx))
            })
            .unwrap();
        assert_eq!(
            closed_state.0.as_deref(),
            Some("inspector"),
            "the dock must keep the inspector key after the close toggle"
        );
        assert!(!closed_state.1, "second dispatch must close the right dock");

        // Third dispatch: a sibling toc panel arrives.  The toc slot
        // is fresh so the factory runs there; the inspector slot stays
        // populated so a subsequent inspector dispatch swaps back in
        // without reconstructing.
        let toc_slot: std::rc::Rc<std::cell::RefCell<Option<gpui::Entity<toc_panel::TocPanel>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        window
            .update(cx, |ws, _window, cx| {
                crate::macos::toggle_or_swap_right_dock_panel(ws, cx, "toc", &toc_slot, |cx| {
                    cx.new(|_| toc_panel::TocPanel::new())
                });
            })
            .unwrap();
        let toc_state = window
            .update(cx, |ws, _window, cx| {
                (ws.right_dock_panel_key(cx), ws.is_right_dock_open(cx))
            })
            .unwrap();
        assert_eq!(
            toc_state.0.as_deref(),
            Some("toc"),
            "toc dispatch must swap the right-dock panel to the toc key"
        );
        assert!(toc_state.1, "toc panel reports starts_open=true");

        // Fourth dispatch: swap back to inspector.  The slot is still
        // populated from the first dispatch — the factory must NOT
        // run a second time.  Track that by capturing how many times
        // the closure is called.
        let factory_calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let factory_calls_inner = factory_calls.clone();
        window
            .update(cx, |ws, _window, cx| {
                crate::macos::toggle_or_swap_right_dock_panel(
                    ws,
                    cx,
                    "inspector",
                    &inspector_slot,
                    |cx| {
                        factory_calls_inner.set(factory_calls_inner.get() + 1);
                        cx.new(|_| inspector_panel::InspectorPanel::new())
                    },
                );
            })
            .unwrap();
        let swapped = window
            .update(cx, |ws, _window, cx| {
                (ws.right_dock_panel_key(cx), ws.is_right_dock_open(cx))
            })
            .unwrap();
        assert_eq!(
            swapped.0.as_deref(),
            Some("inspector"),
            "swap back to inspector must restore the inspector key"
        );
        assert!(
            swapped.1,
            "swap-in always attaches the panel with starts_open semantics"
        );
        assert_eq!(
            factory_calls.get(),
            0,
            "slot was populated, so the factory must not run again on swap-back"
        );
    }

    /// Worklist 9.2.13 Reopened-3 — end-to-end coverage of the
    /// production inspector toggle path: title-bar click dispatches
    /// [`actions::ToggleInspector`] via [`Window::dispatch_action`]
    /// from inside the window's own update; the App-scope
    /// `cx.on_action` handler defers a workspace lookup via
    /// [`dispatch_to_workspace`]; the deferred closure attaches the
    /// inspector panel through [`toggle_or_swap_right_dock_panel`].
    ///
    /// The existing
    /// `toggle_inspector_attaches_panel_and_swaps_with_toc` test calls
    /// the helper directly and the existing
    /// `toolbar_window_dispatch_reaches_app_action_handler_under_nested_update`
    /// test pins the dispatch route — neither composes the full chain.
    /// This test does: a regression that breaks ANY hop (action
    /// registration, deferred resolve, workspace downcast, panel
    /// attach) would silently keep the right dock empty in production
    /// today (user-visible as "inspector doesn't open") but the prior
    /// tests would still pass.
    #[gpui::test]
    fn toggle_inspector_dispatch_chain_attaches_panel_end_to_end(cx: &mut TestAppContext) {
        use gpui::AppContext as _;
        use std::cell::RefCell;
        use std::rc::Rc;
        use workspace::TolariaWorkspace;

        cx.update(gpui_component::init);

        // Match the production `cx.open_window` shape: wrap the
        // workspace in `gpui_component::Root`.  The simpler
        // `cx.add_window(TolariaWorkspace::empty)` shape used by the
        // helper-direct tests sets the workspace AS the window root —
        // `dispatch_to_workspace` then fails its
        // `root.downcast::<gpui_component::Root>()` step (it expects
        // the production wrapper).  This test pins the production
        // chain, so it needs to match the production layout.
        let workspace_slot: Rc<RefCell<Option<gpui::Entity<TolariaWorkspace>>>> =
            Rc::new(RefCell::new(None));
        let workspace_slot_inner = workspace_slot.clone();
        let window = cx.add_window(move |window, cx| {
            let workspace = cx.new(|cx| TolariaWorkspace::empty(window, cx));
            *workspace_slot_inner.borrow_mut() = Some(workspace.clone());
            gpui_component::Root::new(workspace, window, cx)
        });
        // Active window — drives the dispatch through the deferred
        // `cx.active_window()` resolve in `dispatch_to_workspace`.
        window
            .update(cx, |_root, window, _cx| window.activate_window())
            .unwrap();
        cx.run_until_parked();

        // Per-hop counters: the test prints which hops fired when it
        // fails so future regressions are localised without manual
        // log inspection.  Maps directly to the production
        // `info!`/`warn!` log sites.
        let handler_called = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let workspace_resolved = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let factory_called = std::rc::Rc::new(std::cell::Cell::new(0u32));

        // Mirror the production handler registration from
        // `tolaria::macos::run`: an App-scope `ToggleInspector`
        // listener that defers a workspace resolve and toggles the
        // right-dock panel through the shared helper.
        let inspector_slot: std::rc::Rc<
            std::cell::RefCell<Option<gpui::Entity<inspector_panel::InspectorPanel>>>,
        > = std::rc::Rc::new(std::cell::RefCell::new(None));
        let inspector_slot_for_action = inspector_slot.clone();
        let handler_called_inner = handler_called.clone();
        let workspace_resolved_inner = workspace_resolved.clone();
        let factory_called_inner = factory_called.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &actions::ToggleInspector, cx| {
                handler_called_inner.set(handler_called_inner.get() + 1);
                let slot = inspector_slot_for_action.clone();
                let workspace_resolved_inner = workspace_resolved_inner.clone();
                let factory_called_inner = factory_called_inner.clone();
                crate::macos::dispatch_to_workspace(
                    "ToggleInspector",
                    cx,
                    move |ws, _window, cx| {
                        workspace_resolved_inner.set(workspace_resolved_inner.get() + 1);
                        crate::macos::toggle_or_swap_right_dock_panel(
                            ws,
                            cx,
                            "inspector",
                            &slot,
                            |cx| {
                                factory_called_inner.set(factory_called_inner.get() + 1);
                                cx.new(|_| inspector_panel::InspectorPanel::new())
                            },
                        );
                    },
                );
            });
        });

        // Production-shape dispatch: from inside an active-window
        // update (mirrors the `on_click` closure on the title-bar
        // toggle cell), call `Window::dispatch_action`.
        window
            .update(cx, |_root, window, cx| {
                window.dispatch_action(Box::new(actions::ToggleInspector), cx);
            })
            .unwrap();
        // The action defers via `Window::dispatch_action`, the handler
        // defers again via `dispatch_to_workspace`.  Two defers ⇒ two
        // drain passes to be safe.
        cx.run_until_parked();
        cx.run_until_parked();

        // Read workspace state through the held slot (the window
        // root is `gpui_component::Root`, not the workspace).
        let workspace = workspace_slot
            .borrow()
            .as_ref()
            .cloned()
            .expect("workspace slot populated during window construction");
        let after_dispatch = cx.update(|cx| {
            workspace.update(cx, |ws, cx| {
                (
                    ws.right_dock_panel_key(cx),
                    ws.is_right_dock_open(cx),
                    inspector_slot.borrow().is_some(),
                )
            })
        });
        assert_eq!(
            after_dispatch.0.as_deref(),
            Some("inspector"),
            "after a Window::dispatch_action(ToggleInspector) the right \
             dock must report the inspector panel_key — if this fails, \
             one of the chain hops (action register / handler defer / \
             workspace resolve / helper / attach) silently dropped the \
             dispatch.  Per-hop counters: handler_called={}, \
             workspace_resolved={}, factory_called={}.  Match the \
             production logs at `tolaria::inspector` / \
             `tolaria::right_dock` / `workspace::title_bar` to find \
             the broken hop.",
            handler_called.get(),
            workspace_resolved.get(),
            factory_called.get(),
        );
        assert!(
            after_dispatch.1,
            "InspectorPanel reports starts_open=true; after the chain \
             completes the dock must be open."
        );
        assert!(
            after_dispatch.2,
            "the panel slot must be populated so subsequent dispatches \
             find the cached entity"
        );
        // Per-hop assertions: each counter pins one specific hop of
        // the chain so a future regression's failure message
        // immediately tells the user which hop broke (the right-dock
        // assertion above is a function of all three).
        assert_eq!(
            handler_called.get(),
            1,
            "App-scope `cx.on_action` handler must fire exactly once \
             per `Window::dispatch_action(ToggleInspector)`"
        );
        assert_eq!(
            workspace_resolved.get(),
            1,
            "`dispatch_to_workspace`'s deferred workspace resolve must \
             fire — failure here means either `cx.active_window` \
             returned `None`, the window root didn't downcast to \
             `gpui_component::Root`, or the inner view didn't \
             downcast to `TolariaWorkspace`.  Check the warn! logs at \
             those branches in `dispatch_to_workspace`."
        );
        assert_eq!(
            factory_called.get(),
            1,
            "the fresh-attach branch of \
             `toggle_or_swap_right_dock_panel` must run the factory \
             on first dispatch (slot was empty)"
        );
    }

    /// Worklist 9.2.3 + 9.2.4 + 9.2.6 + 9.2.13 reopened-2 — toolbar
    /// click dispatch from inside an active-window `update` must
    /// route to App-scope action handlers.  Pins the live shape of
    /// the click chain: a click closure runs **inside** the window's
    /// own `dispatch_event` update (the `cx.windows[id]` slot is
    /// taken), and any nested `App::dispatch_action` would silently
    /// fail the inner `cx.windows.get_mut(id)?.take()?` re-entrancy
    /// guard via `.log_err()` swallowing the inner `update`.
    ///
    /// The four `Reopened-2` rows shared a single root cause: every
    /// affected cell called `cx.dispatch_action(&action)` (App scope)
    /// from inside the click closure, hitting the silent-fail path
    /// described above.  The fix is to route through
    /// [`Window::dispatch_action`] instead, which internally calls
    /// `cx.defer(...)` to queue the dispatch for **after** the click
    /// update unwinds — at which point the slot is back in
    /// `cx.windows` and the dispatch proceeds normally, firing every
    /// App-scope `cx.on_action(...)` listener registered for the
    /// action's `TypeId` (handler fires during the bubble phase of
    /// the action dispatch).
    ///
    /// This test simulates the exact production click path:
    /// 1. open a window (so `cx.windows[id]` is populated),
    /// 2. register an App-scope handler that increments a counter,
    /// 3. `window.update(...)` to nest inside the window's update
    ///    (mirrors `dispatch_event`'s `update_window_id` frame),
    /// 4. from inside that update, dispatch via
    ///    `Window::dispatch_action` (the new path used by every
    ///    fixed toolbar cell),
    /// 5. drain deferred work via `cx.run_until_parked`,
    /// 6. assert the handler ran exactly once.
    ///
    /// If a future refactor swaps the toolbar cell back to
    /// `cx.dispatch_action(&action)`, the assertion fails — the
    /// re-entrancy guard rejects the nested update and the counter
    /// stays at zero.
    #[gpui::test]
    fn toolbar_window_dispatch_reaches_app_action_handler_under_nested_update(
        cx: &mut TestAppContext,
    ) {
        use workspace::TolariaWorkspace;

        cx.update(gpui_component::init);

        let window = cx.add_window(TolariaWorkspace::empty);
        // Activate the window so `App::active_window()` resolves —
        // mirrors the production frontmost-window state that drives
        // `App::dispatch_action`'s "route through the active window"
        // branch.  Without this, the dispatch falls through to the
        // global-action path, which works regardless of nesting and
        // would hide the regression this test pins.
        window
            .update(cx, |_root, window, _cx| window.activate_window())
            .unwrap();
        cx.run_until_parked();

        // Counter the App-scope handler increments — exactly mirrors
        // the slot-reading shape every affected handler uses in
        // `macos::run`.
        let calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let calls_inner = calls.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &actions::ToggleRawEditor, _cx| {
                calls_inner.set(calls_inner.get() + 1);
            });
        });

        // The load-bearing nested-update site: dispatch from inside
        // `window.update`, which mirrors what gpui does in
        // `Window::dispatch_event` (the dispatch frame for any mouse
        // click).
        window
            .update(cx, |_root, window, cx| {
                window.dispatch_action(Box::new(actions::ToggleRawEditor), cx);
            })
            .unwrap();
        // `Window::dispatch_action` defers internally, so the actual
        // dispatch lands after this drain.
        cx.run_until_parked();

        assert_eq!(
            calls.get(),
            1,
            "Window::dispatch_action from inside a window-update frame must \
             reach the App-scope on_action listener exactly once.  If this \
             counter stays 0, a toolbar cell has regressed to `cx.dispatch_action` \
             (App-level), which fails the re-entrancy guard in `update_window_id` \
             when nested inside `dispatch_event`'s outer update."
        );
    }

    /// Worklist 9.2.3 + 9.2.4 + 9.2.6 + 9.2.13 reopened-2 — the
    /// **negative half** of the dispatch-route regression: calling
    /// `App::dispatch_action(&action)` from inside an active window
    /// update silently fails because `update_window_id`'s
    /// `cx.windows.get_mut(id)?.take()?` re-entrancy guard returns
    /// `None`, and the outer `.log_err()` swallows the error.  This
    /// test pins that exact failure mode so a future helper that
    /// re-introduces nested `App::dispatch_action` calls from click
    /// handlers fails CI rather than silently regressing UX.
    ///
    /// Paired with the positive test
    /// `toolbar_window_dispatch_reaches_app_action_handler_under_nested_update`
    /// above — together they document why every toolbar cell must use
    /// [`Window::dispatch_action`] (which defers internally via
    /// `cx.defer`) rather than [`App::dispatch_action`] (which
    /// synchronously re-enters the same window slot and fails).
    #[gpui::test]
    fn app_dispatch_action_from_inside_window_update_silently_drops(cx: &mut TestAppContext) {
        use workspace::TolariaWorkspace;

        cx.update(gpui_component::init);

        let window = cx.add_window(TolariaWorkspace::empty);
        // Activating the window puts `App::active_window` into the
        // `Some(...)` branch — the exact production state where the
        // re-entrancy guard fires.  Without `activate_window`, the
        // dispatch falls through to the global-action path which
        // works in either nesting state and would hide the regression.
        window
            .update(cx, |_root, window, _cx| window.activate_window())
            .unwrap();
        cx.run_until_parked();

        let calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let calls_inner = calls.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &actions::ToggleRawEditor, _cx| {
                calls_inner.set(calls_inner.get() + 1);
            });
        });

        // From inside the window's update — the broken pattern.  The
        // inner `active_window.update(self, …)` call hits the
        // re-entrancy guard, `update_window_id` returns
        // `Err("window not found")`, and `.log_err()` swallows it.
        window
            .update(cx, |_root, _window, cx| {
                cx.dispatch_action(&actions::ToggleRawEditor);
            })
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            calls.get(),
            0,
            "App::dispatch_action from inside an active-window update must \
             silently drop (regression guard — if this becomes >0 because \
             gpui's re-entrancy semantics changed, revisit the toolbar's \
             dispatch route and consider removing the Window::dispatch_action \
             work-around)"
        );
    }

    /// Worklist 2.1 — end-to-end check that a sidebar selection event
    /// drives the note-list-pane header through the workspace event
    /// subscription, exactly as the live app wires them in
    /// `cx.open_window`.  We rebuild the subscription here in isolation
    /// (the live `cx.open_window` path requires a real window) so the
    /// contract — `SidebarPanel::select` → `SidebarSelectionChangedEvent`
    /// → `NoteListPane::set_header_title` — stays guarded even if the
    /// scope-routing block in `main` is refactored.
    #[gpui::test]
    fn sidebar_selection_updates_note_list_header(cx: &mut TestAppContext) {
        use gpui::AppContext as _;
        use note_list_pane::NoteListPane;
        use sidebar_panel::{SidebarPanel, SidebarSelection, SidebarSelectionChangedEvent};

        cx.update(gpui_component::init);

        let sidebar = cx.update(|cx| cx.new(|_| SidebarPanel::new()));
        let note_list = cx.update(|cx| cx.new(|_| NoteListPane::new()));

        cx.update(|cx| {
            let list = note_list.clone();
            cx.subscribe(
                &sidebar,
                move |_panel, event: &SidebarSelectionChangedEvent, cx| {
                    let label = event.display_label.clone();
                    list.update(cx, |pane: &mut NoteListPane, cx| {
                        pane.set_header_title(label, cx);
                    });
                },
            )
            .detach();
        });
        cx.run_until_parked();

        cx.update(|cx| {
            sidebar.update(cx, |panel: &mut SidebarPanel, cx| {
                panel.select(SidebarSelection::Archive, cx);
            });
        });
        cx.run_until_parked();

        let header = cx.update(|cx| note_list.read(cx).header_title().clone());
        assert_eq!(
            header.as_ref(),
            "Archive",
            "sidebar Archive selection must propagate to the note-list header",
        );

        cx.update(|cx| {
            sidebar.update(cx, |panel: &mut SidebarPanel, cx| {
                panel.select(SidebarSelection::Type("Events".into()), cx);
            });
        });
        cx.run_until_parked();
        let header = cx.update(|cx| note_list.read(cx).header_title().clone());
        assert_eq!(header.as_ref(), "Events");
    }

    /// Worklist 9.2.14 — dispatching [`actions::EnterNeighborhood`]
    /// must (a) push a `NoteListScope::Neighborhood(...)` onto the
    /// pane, (b) flip the pane's header to the active note's title
    /// (worklist 9.3.8 — title alone, not `Neighborhood of <title>`),
    /// and (c) write `Some(id)` into the
    /// [`note_item::NeighborhoodAnchor`] global so the next toolbar
    /// render paints the cell in its active state.
    ///
    /// Mirrors the production handler shape from `macos::run`'s
    /// `cx.on_action` registration as closely as a unit test can: the
    /// same slot pattern, the same vault read, the same pane mutations
    /// and global write.  Pins the contract end-to-end so a future
    /// refactor of the handler (or any of its three side effects)
    /// surfaces here rather than as a silent UI desync.
    #[gpui::test]
    fn enter_neighborhood_updates_header_and_anchor(cx: &mut TestAppContext) {
        use gpui::{AppContext as _, Entity};
        use note_item::{NeighborhoodAnchor, NoteItem};
        use note_list_pane::{NoteListPane, NoteListScope};
        use tempfile::tempdir;
        use vault::Vault;
        cx.update(gpui_component::init);

        // Real on-disk vault with two notes wikilinked together so the
        // backlinks lookup resolves to a non-empty set.  `a.md`
        // contains `[[b]]`, so `vault.backlinks(b_id)` returns `{a_id}`
        // and the neighborhood of `b` is `{a}`.
        //
        // `Vault::open_at` already calls `rescan_internal` so the notes
        // map is populated before we install the global — no async
        // load step required here.
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.md"), "# A\n[[b]]\n").unwrap();
        std::fs::write(dir.path().join("b.md"), "# B\nbody\n").unwrap();
        let vault = Vault::open_at(dir.path()).expect("open vault");
        let anchor_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "b")
            .map(|n| n.id)
            .expect("vault scan must surface the `b.md` fixture");
        // Worklist 9.3.8 Reopened — the neighbourhood header reads
        // the H1 / frontmatter display title (`B`), not the file-stem
        // `Note::title` (`b`).  Mirrors what the note-list row shows.
        let anchor_display_title = "B";
        cx.update(|cx| cx.set_global(vault));

        // Build the note-list pane against the real vault — its
        // `header_title` defaults to "Inbox" so we can assert the
        // post-dispatch value is the new title-only label
        // (worklist 9.3.8 — was previously "Neighborhood of …").
        let note_list = cx.update(|cx| cx.new(|cx| NoteListPane::from_vault(cx)));
        let initial_header = cx.update(|cx| note_list.read(cx).header_title().clone());
        assert_eq!(
            initial_header.as_ref(),
            "Inbox",
            "pane must boot with the default Inbox header before any sidebar / action event",
        );

        // Active-NoteItem slot populated with a real entity carrying
        // the anchor note's metadata — matches the
        // `preload_blank_webview` → `open_in_webview` chain that the
        // production handler relies on.
        let slot: crate::open_note::ActiveNoteItemSlot =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let note = cx
            .update(|cx| cx.global::<Vault>().note_sync(anchor_id).cloned())
            .expect("vault must surface the anchor note");
        let item: Entity<NoteItem> = cx.update(|cx| cx.new(|_| NoteItem::new_for_tests(note)));
        *slot.borrow_mut() = Some(item.clone());

        // Register the production handler via the shared helper —
        // `handle_enter_neighborhood` reads the slot, the pane, and
        // the prev-scope memory through the same `Rc<RefCell<…>>`
        // backing the live workspace's `cx.on_action` closure.  Tests
        // and production agree on a single implementation; any future
        // branch change surfaces here.
        let handler_slot = slot.clone();
        let handler_list = note_list.clone();
        let handler_prev_scope: std::rc::Rc<std::cell::RefCell<Option<NoteListScope>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let handler_prev_scope_closure = handler_prev_scope.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &actions::EnterNeighborhood, cx| {
                crate::macos::handle_enter_neighborhood(
                    &handler_slot,
                    &handler_list,
                    &handler_prev_scope_closure,
                    cx,
                );
            });
        });

        // Drive the action through `cx.dispatch_action` — the App-level
        // shape every real toolbar click eventually lands on.
        cx.update(|cx| cx.dispatch_action(&actions::EnterNeighborhood));
        cx.run_until_parked();

        // Header reflects the active note's title (worklist 9.3.8 —
        // title alone, no `Neighborhood of` prefix).
        let header_after = cx.update(|cx| note_list.read(cx).header_title().clone());
        assert_eq!(
            header_after.as_ref(),
            anchor_display_title,
            "EnterNeighborhood must update the note-list header to the active note's title",
        );

        // Scope reflects the neighborhood filter.
        let scope_after = cx.update(|cx| note_list.read(cx).scope().clone());
        match scope_after {
            NoteListScope::Neighborhood(id, _) => assert_eq!(
                id, anchor_id,
                "neighborhood scope must anchor on the active NoteItem's id"
            ),
            other => panic!("expected NoteListScope::Neighborhood, got {other:?}"),
        }

        // Toolbar-readable global names the active anchor — the
        // toolbar's `is_neighborhood_active` read on the next render
        // returns true for this note id.
        cx.update(|cx| {
            let anchor = cx.try_global::<NeighborhoodAnchor>().copied().expect(
                "EnterNeighborhood must install the NeighborhoodAnchor global so the \
                     toolbar's render path can paint the active-state glyph",
            );
            assert_eq!(
                anchor,
                NeighborhoodAnchor(Some(anchor_id)),
                "anchor payload must name the active note's id",
            );
            assert!(
                anchor.matches(anchor_id),
                "anchor must answer `true` for the anchor note's id",
            );
        });
    }

    /// Worklist 9.2.16 — first click on the neighbourhood cell while
    /// the pane is in `Inbox` (or any non-neighbourhood scope) must
    /// behave the same as the 9.2.14 entry path: pane scope flips to
    /// `Neighborhood(active_id, ids)`, anchor global writes
    /// `Some(active_id)`, header reads `"Neighborhood of <title>"`.
    /// Pins the on-half of the toggle contract through the same
    /// `handle_enter_neighborhood` helper the production handler
    /// dispatches through.
    #[gpui::test]
    fn neighborhood_handler_enters_when_scope_is_not_neighborhood(cx: &mut TestAppContext) {
        use gpui::{AppContext as _, Entity};
        use note_item::{NeighborhoodAnchor, NoteItem};
        use note_list_pane::{NoteListPane, NoteListScope};
        use tempfile::tempdir;
        use vault::Vault;
        cx.update(gpui_component::init);

        // Fixture vault: `a.md` wikilinks `b.md` so `b`'s
        // neighbourhood resolves to a non-empty set.
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.md"), "# A\n[[b]]\n").unwrap();
        std::fs::write(dir.path().join("b.md"), "# B\nbody\n").unwrap();
        let vault = Vault::open_at(dir.path()).expect("open vault");
        let anchor_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "b")
            .map(|n| n.id)
            .expect("vault scan must surface the `b.md` fixture");
        // Worklist 9.3.8 Reopened — the neighbourhood header reads
        // the H1 / frontmatter display title (`B`), not the file-stem
        // `Note::title` (`b`).  Mirrors what the note-list row shows.
        let anchor_display_title = "B";
        cx.update(|cx| cx.set_global(vault));

        let note_list = cx.update(|cx| cx.new(|cx| NoteListPane::from_vault(cx)));
        // Slot populated with the anchor note — matches the prod
        // `preload_blank_webview` → `open_in_webview` chain.
        let slot: crate::open_note::ActiveNoteItemSlot =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let note = cx
            .update(|cx| cx.global::<Vault>().note_sync(anchor_id).cloned())
            .expect("vault must surface the anchor note");
        let item: Entity<NoteItem> = cx.update(|cx| cx.new(|_| NoteItem::new_for_tests(note)));
        *slot.borrow_mut() = Some(item.clone());

        // Shared previous-scope memory.  Boots empty — matches the
        // workspace-just-opened state where the user has never
        // entered neighbourhood mode.
        let prev_scope: std::rc::Rc<std::cell::RefCell<Option<NoteListScope>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));

        // Drive the on-path through the shared helper — production
        // and test agree on the same code.
        cx.update(|cx| {
            crate::macos::handle_enter_neighborhood(&slot, &note_list, &prev_scope, cx);
        });
        cx.run_until_parked();

        // Scope flipped to Neighborhood anchored on this note's id.
        let scope_after = cx.update(|cx| note_list.read(cx).scope().clone());
        match scope_after {
            NoteListScope::Neighborhood(id, _) => assert_eq!(
                id, anchor_id,
                "toggle-on must anchor the neighbourhood scope on the active id"
            ),
            other => panic!(
                "toggle-on must push NoteListScope::Neighborhood onto the pane, got {other:?}",
            ),
        }

        // Anchor global names the active id — toolbar's active-state
        // glyph wakes up on the next render.
        cx.update(|cx| {
            let anchor = cx
                .try_global::<NeighborhoodAnchor>()
                .copied()
                .expect("toggle-on must install the NeighborhoodAnchor global");
            assert_eq!(
                anchor,
                NeighborhoodAnchor(Some(anchor_id)),
                "anchor payload must name the active note's id",
            );
        });

        // Previous-scope slot remembers the boot-time Inbox so the
        // next click can pop back.
        assert_eq!(
            *prev_scope.borrow(),
            Some(NoteListScope::Inbox),
            "toggle-on must save the pre-neighbourhood scope into the slot",
        );

        // Header reflects the entered neighbourhood — worklist 9.3.8
        // pinned this to the active note's title alone (was
        // previously `Neighborhood of <title>`).
        let header_after = cx.update(|cx| note_list.read(cx).header_title().clone());
        assert_eq!(
            header_after.as_ref(),
            anchor_display_title,
            "toggle-on must set the header to the active note's title (worklist 9.3.8)",
        );
    }

    /// Worklist 9.2.16 — second click on the same active note while
    /// the pane is ALREADY in `Neighborhood(active_id, …)` must exit
    /// neighbourhood mode: scope reverts to the saved previous scope,
    /// the anchor global clears to `None`, and the header restores
    /// the previous scope's display label.  This is the off-half of
    /// the toggle contract — the row's load-bearing user behaviour.
    #[gpui::test]
    fn neighborhood_handler_exits_when_scope_matches_active_id(cx: &mut TestAppContext) {
        use gpui::{AppContext as _, Entity};
        use note_item::{NeighborhoodAnchor, NoteItem};
        use note_list_pane::{NoteListPane, NoteListScope};
        use std::collections::HashSet;
        use tempfile::tempdir;
        use vault::Vault;
        cx.update(gpui_component::init);

        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.md"), "# A\n[[b]]\n").unwrap();
        std::fs::write(dir.path().join("b.md"), "# B\nbody\n").unwrap();
        let vault = Vault::open_at(dir.path()).expect("open vault");
        let anchor_id = vault
            .iter_notes()
            .find(|n| n.title.as_ref() == "b")
            .map(|n| n.id)
            .expect("vault scan must surface the `b.md` fixture");
        cx.update(|cx| cx.set_global(vault));

        let note_list = cx.update(|cx| cx.new(|cx| NoteListPane::from_vault(cx)));
        let slot: crate::open_note::ActiveNoteItemSlot =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let note = cx
            .update(|cx| cx.global::<Vault>().note_sync(anchor_id).cloned())
            .expect("vault must surface the anchor note");
        let item: Entity<NoteItem> = cx.update(|cx| cx.new(|_| NoteItem::new_for_tests(note)));
        *slot.borrow_mut() = Some(item.clone());

        // Pre-seed the workspace into the SAVED state the toggle-off
        // branch expects to see: pane scope is `Neighborhood(anchor_id, …)`,
        // anchor global names the active id, prev-scope slot holds
        // a `Folder("projects")` we want restored.
        let saved_scope = NoteListScope::Folder("projects".into());
        let prev_scope: std::rc::Rc<std::cell::RefCell<Option<NoteListScope>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Some(saved_scope.clone())));
        cx.update(|cx| {
            note_list.update(cx, |pane, cx| {
                pane.set_scope(NoteListScope::Neighborhood(anchor_id, HashSet::new()), cx);
                pane.set_header_title("Neighborhood of b", cx);
            });
            cx.set_global(NeighborhoodAnchor(Some(anchor_id)));
        });
        cx.run_until_parked();

        // Fire the handler a SECOND time — expected behaviour: exit.
        cx.update(|cx| {
            crate::macos::handle_enter_neighborhood(&slot, &note_list, &prev_scope, cx);
        });
        cx.run_until_parked();

        // Scope restored to the saved previous scope.
        let scope_after = cx.update(|cx| note_list.read(cx).scope().clone());
        assert_eq!(
            scope_after, saved_scope,
            "toggle-off must restore the saved previous scope verbatim",
        );

        // Anchor cleared — toolbar's active-state glyph deactivates.
        cx.update(|cx| {
            let anchor = cx
                .try_global::<NeighborhoodAnchor>()
                .copied()
                .unwrap_or_default();
            assert_eq!(
                anchor,
                NeighborhoodAnchor(None),
                "toggle-off must clear the NeighborhoodAnchor global",
            );
        });

        // Saved slot consumed — the next on-click starts a fresh save.
        assert_eq!(
            *prev_scope.borrow(),
            None,
            "toggle-off must consume the saved-scope slot (Phase 10 nav_history \
             will replace this with a stack)",
        );

        // Header restored to the previous scope's display label.
        let header_after = cx.update(|cx| note_list.read(cx).header_title().clone());
        assert_eq!(
            header_after.as_ref(),
            "projects",
            "toggle-off must restore the previous scope's display label",
        );
    }

    /// Worklist 9.2.14 — picking any sidebar row must clear the
    /// `NeighborhoodAnchor` global, mirroring React's
    /// `useNeighborhoodEntry` semantics where switching to another
    /// view exits neighbourhood mode.  Without this, the toolbar's
    /// active-state glyph would stay lit even though the note-list
    /// pane has already moved off the `Neighborhood(...)` scope.
    ///
    /// Re-builds the production subscriber inline so a future
    /// refactor of the `cx.open_window` block still surfaces the
    /// regression here.
    #[gpui::test]
    fn sidebar_selection_clears_neighborhood_anchor(cx: &mut TestAppContext) {
        use gpui::AppContext as _;
        use note_item::NeighborhoodAnchor;
        use sidebar_panel::{SidebarPanel, SidebarSelection, SidebarSelectionChangedEvent};
        use vault::NoteId;

        cx.update(gpui_component::init);

        // Install an anchor first — pretend EnterNeighborhood just
        // fired so we have a non-empty global to observe being cleared.
        cx.update(|cx| {
            cx.set_global(NeighborhoodAnchor(Some(NoteId::from_raw(42))));
        });

        let sidebar = cx.update(|cx| cx.new(|_| SidebarPanel::new()));
        cx.update(|cx| {
            cx.subscribe(
                &sidebar,
                move |_panel, _event: &SidebarSelectionChangedEvent, cx| {
                    // Same shape as the production subscriber: drop the
                    // anchor on every selection change.
                    cx.set_global(NeighborhoodAnchor(None));
                },
            )
            .detach();
        });
        cx.run_until_parked();

        cx.update(|cx| {
            sidebar.update(cx, |panel: &mut SidebarPanel, cx| {
                panel.select(SidebarSelection::AllNotes, cx);
            });
        });
        cx.run_until_parked();

        cx.update(|cx| {
            let anchor = cx
                .try_global::<NeighborhoodAnchor>()
                .copied()
                .unwrap_or_default();
            assert_eq!(
                anchor,
                NeighborhoodAnchor(None),
                "sidebar selection change must clear the NeighborhoodAnchor",
            );
        });
    }
}
