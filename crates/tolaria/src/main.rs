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
fn main() {
    macos::run();
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;

    use gpui::{
        px, size, App, AppContext, Bounds, SharedString, TitlebarOptions, WindowBounds,
        WindowOptions,
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

    /// Parsed command-line arguments.  Phase 5-MVP only carries the
    /// `--vault <path>` option; everything else is forwarded by GPUI /
    /// AppKit (e.g. `--bundle` arguments from `open`).
    struct CliArgs {
        vault_path: Option<PathBuf>,
    }

    fn parse_args() -> CliArgs {
        let mut iter = std::env::args().skip(1);
        let mut vault_path = None;
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--vault" => {
                    vault_path = iter.next().map(PathBuf::from);
                }
                "--help" | "-h" => {
                    eprintln!("Usage: tolaria [--vault <path>]");
                    std::process::exit(0);
                }
                _ => {
                    // Ignore unrecognised flags so future GPUI / AppKit
                    // additions don't break startup.
                }
            }
        }
        CliArgs { vault_path }
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

    pub fn run() {
        env_logger::Builder::new()
            .filter_module("tolaria", log::LevelFilter::Info)
            .parse_default_env()
            .init();
        log::info!("tolaria starting (ADR-0115 Phase 5-MVP)");

        let args = parse_args();

        application().run(move |cx: &mut App| {
            // 3. Theme / gpui-component global (must precede any primitive render).
            theme::init(cx);

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
            log_stub::<actions::CloseWindow>(
                cx,
                "CloseWindow",
                "Phase 2 will close the focused window via cx.active_window()",
            );
            log_stub::<actions::OpenSettings>(
                cx,
                "OpenSettings",
                "Phase 2 will push the settings panel onto TolariaWorkspace",
            );
            log_stub::<actions::ReloadKeymap>(
                cx,
                "ReloadKeymap",
                "Phase 2 will re-run actions::init to reload the user keymap",
            );

            // 7. Native menu bar (installed before window open so AppKit picks
            //    up accelerators immediately — ADR-0115 §6).
            cx.set_menus(menus::app_menus());

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
                    Ok(vault) => {
                        log::info!("--vault {path:?}: installed vault::Vault global");
                        cx.set_global(vault);
                    }
                    Err(err) => {
                        log::error!("--vault {path:?}: failed to open: {err:#}");
                    }
                }
            }

            // 9. Re-apply theme whenever settings change.
            cx.observe_global::<SettingsStore>(|cx| {
                theme::reload_from_settings(cx);
            })
            .detach();

            // 10. Open root window.  Copy f32 size values out before passing cx
            //    to Bounds::centered so the borrow of SettingsStore is released.
            let (width, height) = {
                let w = &SettingsStore::get(cx).window;
                (w.width, w.height)
            };
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            let opts = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Tolaria")),
                    ..Default::default()
                }),
                ..Default::default()
            };

            if let Err(err) = cx.open_window(opts, |window, cx| {
                cx.new(|model_cx| workspace::TolariaWorkspace::empty(window, model_cx))
            }) {
                log::error!("failed to open Tolaria window: {err:#}");
            }

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
}
