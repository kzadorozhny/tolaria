//! Theme global for Tolaria (ADR-0115 Phase 1).
//!
//! Thin wrapper around `gpui_component`'s `Theme` global. `init` is
//! idempotent: calling it multiple times is safe because
//! `gpui_component::init` is itself idempotent.
//!
//! `reload_from_settings` is a stub in Phase 1 — real light/dark switching
//! wires into `settings_store`'s observer in Phase 2.

use std::fmt;
use std::str::FromStr;

mod palette;

/// Install the `gpui_component::Theme` global into `cx`.
///
/// Must be called before any `gpui_component` primitive is rendered.
/// Mirrors the `gpui_component::init` call in `embed_poc::main`.
pub fn init(cx: &mut gpui::App) {
    gpui_component::init(cx);
}

/// User-facing theme choice surfaced via the `--theme` CLI flag (and,
/// later, the settings store).
///
/// - [`ThemeChoice::System`] tracks the macOS Light/Dark appearance —
///   useful for users who already have a global preference.
/// - [`ThemeChoice::Light`] / [`ThemeChoice::Dark`] pin the app to a
///   single mode regardless of OS preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    /// Follow the OS appearance.  Default — matches what users
    /// expect from a well-behaved native macOS app.
    #[default]
    System,
    /// Force the light theme.
    Light,
    /// Force the dark theme.
    Dark,
}

impl ThemeChoice {
    /// Stable serialization for log output and settings persistence.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

impl fmt::Display for ThemeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ThemeChoice {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "system" | "auto" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            other => {
                anyhow::bail!("unknown theme {other:?}; expected one of: system, light, dark")
            }
        }
    }
}

/// Apply `choice` to the active `gpui_component::Theme` global.
///
/// `theme::init` must have run first so the global exists.  For
/// [`ThemeChoice::System`] the function reads `cx.window_appearance()`
/// and forwards through gpui-component's
/// `Theme::sync_system_appearance`.  For the explicit modes it calls
/// `Theme::change` directly — same code path as the user toggling the
/// theme at runtime.
///
/// # Panics
///
/// Panics if `gpui_component::Theme` is not yet registered as a
/// `gpui::Global`.  Always call after [`init`].
pub fn apply_choice(cx: &mut gpui::App, choice: ThemeChoice) {
    use gpui_component::theme::{Theme, ThemeMode};
    use gpui_component::ActiveTheme as _;

    match choice {
        ThemeChoice::System => Theme::sync_system_appearance(None, cx),
        ThemeChoice::Light => Theme::change(ThemeMode::Light, None, cx),
        ThemeChoice::Dark => Theme::change(ThemeMode::Dark, None, cx),
    }
    // Overwrite the just-installed `ThemeColor` with our CSS-derived
    // palette so the native chrome matches `src/index.css` and the
    // Tauri-era reference screenshots exactly.
    let theme = cx.global_mut::<Theme>();
    if theme.is_dark() {
        palette::apply_dark(theme);
    } else {
        palette::apply_light(theme);
    }
    log::info!(
        "theme: applied {choice} (resolved is_dark={})",
        cx.theme().is_dark()
    );
}

/// Toggle the active theme between [`ThemeChoice::Light`] and
/// [`ThemeChoice::Dark`].  Reads the live state via
/// `gpui_component::ActiveTheme::is_dark`, then calls
/// [`apply_choice`] with the inverse.  Used by the status-bar
/// theme-switcher button so users can flip the chrome without going
/// through the menu / settings.
///
/// `System` is intentionally not part of the cycle — the user opts
/// into "follow system" via the `--theme` CLI flag or future
/// settings UI, not the status-bar toggle.
pub fn cycle(cx: &mut gpui::App) {
    use gpui_component::ActiveTheme as _;
    let next = if cx.theme().is_dark() {
        ThemeChoice::Light
    } else {
        ThemeChoice::Dark
    };
    apply_choice(cx, next);
}

/// Reload the active theme from the current `SettingsStore` global.
///
/// Phase 1 stub — `cx` is accepted now so the Phase 2 wiring
/// (`SettingsStore::get(cx).theme` → `gpui_component` theme-switch API)
/// is a non-breaking change. Currently logs the request and returns.
pub fn reload_from_settings(cx: &mut gpui::App) {
    let _ = cx; // reserved for Phase 2
    log::info!("theme reload requested (stub — real switching in Phase 2)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn init_installs_theme_global(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            init(cx);
            assert!(
                cx.try_global::<gpui_component::theme::Theme>().is_some(),
                "gpui_component::Theme global must be present after theme::init"
            );
        });
    }

    #[test]
    fn theme_choice_round_trip_via_from_str() {
        for choice in [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark] {
            let s = choice.as_str();
            let parsed: ThemeChoice = s.parse().expect("round-trip");
            assert_eq!(parsed, choice, "round-trip failed for {s}");
        }
    }

    #[test]
    fn theme_choice_from_str_is_case_insensitive_and_accepts_auto_alias() {
        assert_eq!(
            "System".parse::<ThemeChoice>().unwrap(),
            ThemeChoice::System
        );
        assert_eq!("LIGHT".parse::<ThemeChoice>().unwrap(), ThemeChoice::Light);
        assert_eq!("dArK".parse::<ThemeChoice>().unwrap(), ThemeChoice::Dark);
        assert_eq!("auto".parse::<ThemeChoice>().unwrap(), ThemeChoice::System);
    }

    #[test]
    fn theme_choice_from_str_rejects_garbage() {
        let err = "puce".parse::<ThemeChoice>().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("system") && msg.contains("light") && msg.contains("dark"),
            "error must enumerate the valid choices: got {msg:?}"
        );
    }

    #[gpui::test]
    fn apply_choice_dark_flips_theme_mode_to_dark(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            init(cx);
            apply_choice(cx, ThemeChoice::Dark);
            let theme = cx
                .try_global::<gpui_component::theme::Theme>()
                .expect("Theme global");
            assert!(theme.is_dark(), "Theme::is_dark() must be true after Dark");
        });
    }

    #[gpui::test]
    fn apply_choice_light_flips_theme_mode_to_light(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            init(cx);
            apply_choice(cx, ThemeChoice::Dark); // start dark to prove we move it
            apply_choice(cx, ThemeChoice::Light);
            let theme = cx
                .try_global::<gpui_component::theme::Theme>()
                .expect("Theme global");
            assert!(
                !theme.is_dark(),
                "Theme::is_dark() must be false after Light"
            );
        });
    }
}
