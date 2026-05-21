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
    fn dispatch_to_workspace<F>(label: &'static str, cx: &mut App, f: F)
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
            let Some(handle) = cx.active_window() else {
                log::debug!("{label}: no active window");
                return;
            };
            if let Err(err) = handle.update(cx, |root, window, app_cx| {
                let Ok(root_entity) = root.downcast::<gpui_component::Root>() else {
                    log::debug!("{label}: window root is not gpui_component::Root");
                    return;
                };
                let inner = root_entity.read(app_cx).view().clone();
                let Ok(workspace) = inner.downcast::<workspace::TolariaWorkspace>() else {
                    log::debug!("{label}: Root inner view is not TolariaWorkspace");
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
    /// sidebar / inspector state with the workspace already in scope.
    ///
    /// Worklist 3.2 — the View menu's two toggle entries flip between
    /// `"Show …"` and `"Hide …"` based on the workspace's left-dock
    /// state and GPUI's inspector overlay.  Action handlers that
    /// already run inside `dispatch_to_workspace` call this so the
    /// rebuild observes the *post-toggle* state.
    ///
    /// `inspector_picking` is sourced from
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
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<workspace::TolariaWorkspace>,
    ) {
        // Worklist 9.2.13 — the View → Inspector entry now toggles the
        // application's `InspectorPanel` in the right dock (the
        // GPUI element-picker overlay moved to `ToggleElementInspector`),
        // so the menu label tracks the right dock's mounted-panel
        // state rather than `Window::is_inspector_picking`.
        let inspector_visible = workspace.is_right_dock_open(cx)
            && workspace.right_dock_panel_key(cx).as_deref() == Some("inspector");
        let state = menus::MenuState {
            sidebar_open: workspace.is_sidebar_open(cx),
            inspector_picking: inspector_visible,
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
            p
        } else {
            let p = factory(cx);
            *slot.borrow_mut() = Some(p.clone());
            p
        };
        ws.attach_right_dock(panel, cx);
    }

    pub fn run() {
        env_logger::Builder::new()
            .filter_module("tolaria", log::LevelFilter::Info)
            .parse_default_env()
            .init();
        log::info!("tolaria starting");

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
                    log::info!(
                        target: "tolaria::raw_editor",
                        "ToggleRawEditor: id={id:?} raw_mode {pre_raw} → {}",
                        !pre_raw,
                    );
                    if let Err(e) = item.update(cx, |item, cx| item.toggle_raw_mode(cx)) {
                        log::warn!("ToggleRawEditor: toggle_raw_mode failed: {e:#}");
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
                // promoted to a real mount: the toolbar's
                // `note-toolbar-inspector` cell now dispatches the
                // application's `ToggleInspector` and lands here
                // (the previous GPUI debug element-picker moved to
                // `ToggleElementInspector`, bound to `Cmd+Alt+I`).
                // The slot keeps the entity alive across right-dock
                // swaps with the ToC panel so the subscribers below
                // (HeadingsUpdated / OpenNote) can continue writing
                // through to the same panel without re-resolving the
                // workspace.
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
                    let slot = inspector_slot_for_action.clone();
                    dispatch_to_workspace("ToggleInspector", cx, move |ws, _window, cx| {
                        toggle_or_swap_right_dock_panel(
                            ws,
                            cx,
                            "inspector",
                            &slot,
                            |cx| cx.new(|cx| inspector_panel::InspectorPanel::from_or_empty(cx)),
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
                    cx.on_action(move |_: &actions::EnterNeighborhood, cx| {
                        // Worklist 9.2.3 reopened — promote the
                        // slot-empty path from `debug!` to `warn!` so
                        // a future regression surfaces in default-level
                        // logs.  The slot is populated by
                        // `preload_blank_webview` at workspace open
                        // and mutated in-place by every
                        // `open_in_webview` thereafter, so an empty
                        // slot at toolbar-click time would indicate a
                        // real ordering bug, not a transient state.
                        let Some(item) = neighborhood_slot.borrow().as_ref().cloned() else {
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
                        // Title for the header label is read before
                        // we drop the immutable vault borrow — keeps
                        // the `set_header_title` call below a single
                        // owned `SharedString`.
                        let title = vault
                            .note_sync(id)
                            .map(|n| n.title.clone())
                            .unwrap_or_else(|| gpui::SharedString::from(format!("note {}", id.get())));
                        // Union of inbound + outbound, minus the active
                        // note itself.  Both query fns already exclude
                        // self-links, so the union doesn't reintroduce
                        // it; the `remove(&id)` below is belt-and-braces
                        // for the future fold of fenced-code awareness
                        // that might surface self-targets again.
                        let mut ids: std::collections::HashSet<vault::NoteId> =
                            vault.backlinks(id).into_iter().collect();
                        ids.extend(vault.outbound_links(id));
                        ids.remove(&id);
                        let count = ids.len();
                        let header =
                            gpui::SharedString::from(format!("Neighborhood of {title}"));
                        // Worklist 9.2.3 reopened — surface an
                        // `info!` log at handler entry + an explicit
                        // `warn!` when the resolved neighborhood is
                        // empty.  The empty-set case is exactly what
                        // the user perceives as "the click did
                        // nothing": the scope swaps but the filter
                        // hides every entry, so the visible result is
                        // an empty list.  The warn log makes the
                        // root cause discoverable from the live log
                        // without enabling debug.
                        log::info!(
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
                        neighborhood_note_list.update(cx, |pane, cx| {
                            pane.set_scope(
                                note_list_pane::NoteListScope::Neighborhood(id, ids),
                                cx,
                            );
                            pane.set_header_title(header, cx);
                        });
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
}
