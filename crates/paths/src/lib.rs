//! App data/config directory resolution for Tolaria (ADR-0115 Phase 1).
//!
//! Provides pure functions returning platform-standard paths. No globals are
//! needed in Phase 1 because the paths are deterministic.
//!
//! Directory *creation* is the responsibility of `settings_store` (which calls
//! `std::fs::create_dir_all` on first write). This crate only resolves paths.

use std::path::PathBuf;

/// Returns the platform-standard application data directory for Tolaria.
///
/// | Platform | Path |
/// |----------|------|
/// | macOS    | `~/Library/Application Support/Tolaria/` |
/// | Linux    | `~/.local/share/Tolaria/` |
/// | Windows  | `%APPDATA%\Tolaria\` |
///
/// # Panics
///
/// Panics if `dirs::data_dir()` returns `None`. On supported platforms (macOS,
/// Linux, Windows) this should never occur. Tolaria v1 targets macOS only
/// (ADR-0115 §8), so this panic is appropriate and intentional.
pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .expect("dirs::data_dir() must resolve on supported platforms; cannot locate Tolaria app data directory")
        .join("Tolaria")
}

fn data_file(name: &str) -> PathBuf {
    app_data_dir().join(name)
}

/// Returns `<app_data_dir>/settings.json`.
///
/// # Panics
///
/// Panics if [`app_data_dir`] panics (see its documentation).
pub fn settings_file() -> PathBuf {
    data_file("settings.json")
}

/// Returns `<app_data_dir>/keymap.json`.
///
/// # Panics
///
/// Panics if [`app_data_dir`] panics (see its documentation).
pub fn keymap_file() -> PathBuf {
    data_file("keymap.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn app_data_dir_is_absolute() {
        let dir = app_data_dir();
        assert!(
            dir.is_absolute(),
            "app_data_dir must be absolute on Unix, got {dir:?}"
        );
    }

    #[test]
    fn app_data_dir_ends_with_tolaria() {
        let dir = app_data_dir();
        assert_eq!(
            dir.file_name().and_then(|n| n.to_str()),
            Some("Tolaria"),
            "app_data_dir must end with 'Tolaria', got {dir:?}"
        );
    }

    #[test]
    fn settings_file_under_app_dir() {
        let dir = app_data_dir();
        let file = settings_file();
        assert_eq!(file.parent(), Some(dir.as_path()));
        assert_eq!(
            file.file_name().and_then(|n| n.to_str()),
            Some("settings.json")
        );
    }

    #[test]
    fn keymap_file_under_app_dir() {
        let dir = app_data_dir();
        let file = keymap_file();
        assert_eq!(file.parent(), Some(dir.as_path()));
        assert_eq!(
            file.file_name().and_then(|n| n.to_str()),
            Some("keymap.json")
        );
    }
}
