//! Mock settings snapshot for Phase 2 chrome rendering.
//!
//! [`MockSettings`] duplicates the shape of `settings_store::Settings` so
//! chrome crates can render and unit-test against a known fixture without
//! loading a real settings file.  It implements [`gpui::Global`] so that
//! `SettingsPanel` (Phase 2d) can read it via `cx.try_global::<MockSettings>()`
//! and fall back to [`MockSettings::default`] when it has not been installed.

use gpui::Global;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Light / dark / system theme preference.
///
/// Mirrors `settings_store::ThemeChoice` so chrome crates can compile against
/// either type interchangeably.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Window geometry and restore behaviour.
///
/// Mirrors `settings_store::WindowSettings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowSettings {
    pub width: f32,
    pub height: f32,
    pub restore_position: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            restore_position: true,
        }
    }
}

// ---------------------------------------------------------------------------
// MockSettings
// ---------------------------------------------------------------------------

/// Mock settings snapshot mirroring the Phase-1 `settings_store::Settings`
/// JSON schema.
///
/// Use `MockSettings::seeded()` to obtain a consistent fixture value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MockSettings {
    pub version: u32,
    pub theme: ThemeChoice,
    pub window: WindowSettings,
}

impl Global for MockSettings {}

impl MockSettings {
    /// Standard fixture settings used in tests and `TOLARIA_MOCK=1` mode.
    pub fn seeded() -> Self {
        Self {
            version: 1,
            theme: ThemeChoice::Light,
            window: WindowSettings {
                width: 1440.0,
                height: 900.0,
                restore_position: true,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_settings_uses_light_theme() {
        let settings = MockSettings::seeded();
        assert_eq!(settings.theme, ThemeChoice::Light);
        assert_eq!(settings.version, 1);
    }

    #[test]
    fn seeded_settings_round_trips_json() {
        let original = MockSettings::seeded();
        let json = serde_json::to_string(&original).expect("serialisation must succeed");
        let parsed: MockSettings =
            serde_json::from_str(&json).expect("deserialisation must succeed");
        assert_eq!(
            original, parsed,
            "MockSettings must round-trip through JSON"
        );
    }

    #[test]
    fn default_settings_uses_system_theme() {
        let defaults = MockSettings::default();
        assert_eq!(defaults.theme, ThemeChoice::System);
    }

    #[test]
    fn window_defaults_are_reasonable() {
        let w = WindowSettings::default();
        assert!(w.width > 0.0 && w.height > 0.0);
        assert!(w.restore_position);
    }
}
