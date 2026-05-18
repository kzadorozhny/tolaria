//! Global settings store for Tolaria (ADR-0115 Phase 1).
//!
//! `SettingsStore` is a GPUI `Global` holding a minimal [`Settings`] struct.
//! All mutation goes through [`SettingsStore::update`], which persists the new
//! value to disk and triggers `cx.global_mut()` — causing GPUI to emit
//! `NotifyGlobalObservers` and call any `cx.observe_global::<SettingsStore>`
//! subscribers.
//!
//! ## File lifecycle
//! - **Missing**: the parent directory is created (`mkdir -p`) and the default
//!   settings are written. This happens once on first launch.
//! - **Malformed**: the error is logged and defaults are used in memory; the bad
//!   file is NOT overwritten so the user can inspect and fix it.
//! - **Present and valid**: the file is read and deserialised.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Settings schema
// ---------------------------------------------------------------------------

fn default_version() -> u32 {
    1
}

/// Light / dark / system theme choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Minimal Phase-1 window-restore settings.
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

/// The minimal settings schema for Phase 1.
///
/// All fields carry `#[serde(default = …)]` so that forward-compat partial
/// JSON (e.g. only `theme` set) deserialises cleanly.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub theme: ThemeChoice,
    #[serde(default)]
    pub window: WindowSettings,
}

// ---------------------------------------------------------------------------
// SettingsStore
// ---------------------------------------------------------------------------

/// GPUI global that owns the application's [`Settings`] and the path of the
/// on-disk file so mutations can be persisted without re-resolving the path.
pub struct SettingsStore {
    pub(crate) settings: Settings,
    path: PathBuf,
}

impl Global for SettingsStore {}

impl SettingsStore {
    /// Read settings from disk (or create defaults), then install the global.
    ///
    /// This is called once from `crates/tolaria/src/main.rs` during App init,
    /// before any view is opened. It must not panic; IO errors fall back to
    /// defaults.
    pub fn load_and_install(cx: &mut App) -> Result<()> {
        let path = paths::settings_file();
        let settings = load_or_create(&path)?;
        cx.set_global(SettingsStore { settings, path });
        Ok(())
    }

    /// Read the current [`Settings`] from the global.
    pub fn get(cx: &App) -> &Settings {
        &cx.global::<SettingsStore>().settings
    }

    /// Mutate the settings, persist them to disk, and notify global observers.
    ///
    /// `cx.global_mut::<SettingsStore>()` unconditionally pushes
    /// `NotifyGlobalObservers` so every `cx.observe_global::<SettingsStore>`
    /// subscriber is called on the next event-loop tick.
    pub fn update(cx: &mut App, f: impl FnOnce(&mut Settings)) {
        let store = cx.global_mut::<SettingsStore>();
        f(&mut store.settings);
        if let Err(err) = persist(&store.settings, &store.path) {
            log::error!("failed to persist settings to {:?}: {err:#}", store.path);
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn load_or_create(path: &Path) -> Result<Settings> {
    if !path.exists() {
        // First launch: write defaults.
        let dir = path
            .parent()
            .context("settings path has no parent directory")?;
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating settings directory {dir:?}"))?;
        let defaults = Settings::default();
        persist(&defaults, path)
            .with_context(|| format!("writing default settings to {path:?}"))?;
        return Ok(defaults);
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading settings file {path:?}"))?;

    match serde_json::from_str::<Settings>(&content) {
        Ok(settings) => Ok(settings),
        Err(err) => {
            // Bad file — use defaults in memory but do NOT overwrite.
            log::error!("settings file {path:?} is malformed ({err}); using defaults");
            Ok(Settings::default())
        }
    }
}

fn persist(settings: &Settings, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(settings).context("serialising settings to JSON")?;
    // Write to a temp file first, then atomically rename into place so a crash
    // mid-write cannot leave a truncated/empty settings file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json)
        .with_context(|| format!("writing settings to temp file {tmp:?}"))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming {tmp:?} to {path:?}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    /// Helper: install a temporary settings file at `path`, call
    /// `load_and_install`, and return the handle.
    fn install_with_file(cx: &mut TestAppContext, path: &Path, content: &str) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(path, content).unwrap();
        cx.update(|cx| {
            let settings = load_or_create(path).unwrap();
            cx.set_global(SettingsStore {
                settings,
                path: path.to_path_buf(),
            });
        });
    }

    /// When no settings file exists, defaults are used and the file is created.
    #[gpui::test]
    fn default_when_file_missing(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        cx.update(|cx| {
            let settings = load_or_create(&path).unwrap();
            cx.set_global(SettingsStore {
                settings: settings.clone(),
                path: path.clone(),
            });
            assert_eq!(settings, Settings::default());
        });

        // File should now exist with defaults.
        assert!(
            path.exists(),
            "settings file must be created on first launch"
        );
    }

    /// Settings written via `update` must round-trip through JSON and be
    /// re-readable by a fresh `load_or_create` call.
    #[gpui::test]
    fn round_trips_to_disk(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        cx.update(|cx| {
            let settings = load_or_create(&path).unwrap();
            cx.set_global(SettingsStore {
                settings,
                path: path.clone(),
            });
            SettingsStore::update(cx, |s| s.theme = ThemeChoice::Dark);
        });

        // Re-read from disk independently.
        let reloaded = load_or_create(&path).unwrap();
        assert_eq!(
            reloaded.theme,
            ThemeChoice::Dark,
            "persisted theme must round-trip"
        );
    }

    /// A malformed JSON file must not panic; defaults are used in memory and
    /// the bad file is NOT overwritten.
    #[gpui::test]
    fn malformed_json_falls_back_to_defaults_and_does_not_overwrite(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let bad_content = "{ this is not valid json }}}";
        install_with_file(cx, &path, bad_content);

        cx.update(|cx| {
            assert_eq!(
                SettingsStore::get(cx),
                &Settings::default(),
                "malformed file must fall back to defaults in memory"
            );
        });

        // The bad file must remain untouched.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk, bad_content,
            "bad settings file must not be overwritten"
        );
    }

    /// `cx.observe_global::<SettingsStore>` must fire when `update` mutates
    /// settings.
    #[gpui::test]
    fn update_notifies_observers(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        cx.update(|cx| {
            let settings = load_or_create(&path).unwrap();
            cx.set_global(SettingsStore {
                settings,
                path: path.clone(),
            });
        });

        let counter = Arc::new(AtomicU32::new(0));
        let counter_observer = counter.clone();

        cx.update(|cx| {
            cx.observe_global::<SettingsStore>(move |_cx| {
                counter_observer.fetch_add(1, Ordering::SeqCst);
            })
            .detach();

            SettingsStore::update(cx, |s| s.theme = ThemeChoice::Light);
        });

        cx.run_until_parked();

        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "global observer must fire at least once after settings update"
        );
    }
}
