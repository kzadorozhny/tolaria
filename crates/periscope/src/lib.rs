#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Periscope — Rust e2e test harness for the Tolaria native shell
//! (ADR-0115 Phase 6-MVP).
//!
//! Captures PNG screenshots of a running `tolaria` macOS application
//! so Claude (the AI assistant) can observe the live app between
//! conversational turns via its multimodal `Read` tool.  The harness
//! is an external observer (subprocess + OS compositor capture) — see
//! `docs/plans/native-gpui-chrome/e2e-harness.md` for the workflow.
//!
//! # Why not in-process?
//!
//! GPUI's `Window::render_to_image()` reads the Metal drawable
//! texture, which contains the GPUI chrome drawing only.  The
//! embedded WKWebView editor body is a sibling NSView composited by
//! the OS — captures would show it as a black rectangle.  Since the
//! editor is the central feature of ADR-0115, external compositor
//! capture (via `xcap`) is required.
//!
//! # Platforms
//!
//! macOS only.  Other platforms compile a stub that errors at the
//! public API surface — the harness has no purpose off-Tolaria's
//! target platform.

use anyhow::Result;
use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
pub(crate) mod capture;
#[cfg(target_os = "macos")]
pub(crate) mod input;
#[cfg(target_os = "macos")]
pub(crate) mod windows;

/// Which window the harness targets.
///
/// `ByTitle` is the canonical channel — `tolaria` sets its window
/// title to `"Tolaria"` at `crates/tolaria/src/main.rs:214`.  `ByPid`
/// is for the smoke test that spawns its own child and already knows
/// the process id.
#[derive(Debug, Clone)]
pub enum WindowTarget {
    /// Match by exact window title.
    ByTitle(String),
    /// Match by owning process id.
    ByPid(u32),
}

impl WindowTarget {
    /// Build a [`WindowTarget::ByTitle`] from anything string-like.
    pub fn by_title(title: impl Into<String>) -> Self {
        Self::ByTitle(title.into())
    }

    /// Build a [`WindowTarget::ByPid`].
    pub fn by_pid(pid: u32) -> Self {
        Self::ByPid(pid)
    }
}

impl fmt::Display for WindowTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ByTitle(t) => write!(f, "title={t:?}"),
            Self::ByPid(p) => write!(f, "pid={p}"),
        }
    }
}

/// Diagnostic metadata for a single visible window.  Returned by
/// [`list_windows`] to debug "window not found" issues.
#[derive(Debug, Clone)]
pub struct WindowSummary {
    /// Owning process id.
    pub pid: u32,
    /// Window title as the OS reports it.
    pub title: String,
    /// Application name that owns the window.
    pub app_name: String,
}

/// Capture one matching window and write a PNG to `out`.  Returns the
/// canonical path on success.
///
/// # Errors
///
/// - No window matches `target`.
/// - The OS denies screen capture (Screen Recording permission
///   missing).  Detected by all-black / tiny output and converted to
///   a remediation-message error.
/// - `out` cannot be written.
pub fn screenshot(target: &WindowTarget, out: &Path) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        capture::screenshot_macos(target, out)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, out);
        anyhow::bail!("periscope screenshots are macOS-only (Phase 6-MVP)")
    }
}

/// Bring `target` to the foreground before capture.  Uses the macOS
/// Accessibility API so it works on cross-process windows.
///
/// # Errors
///
/// - Accessibility permission missing for the harness binary.
/// - No window matches `target`.
pub fn raise(target: &WindowTarget) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        windows::raise_macos(target)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        anyhow::bail!("periscope raise is macOS-only (Phase 6-MVP)")
    }
}

/// Synthesize a left-click inside `target` at window-local point `(x, y)`.
///
/// Coordinates are in window points with the origin at the top-left
/// corner of the window (matching GPUI's coordinate convention).  The
/// harness translates to global screen space using the window's
/// `xcap`-reported origin before posting `CGEvent` mouse-down +
/// mouse-up at the resolved point.
///
/// `target` must already be raised — callers that need the window in
/// the foreground should invoke [`raise`] first.  Posting events at a
/// covered or off-screen window silently no-ops.
///
/// # Errors
///
/// - No window matches `target`.
/// - The CoreGraphics event-source / event-create call fails (rare;
///   typically indicates a sandbox / TCC restriction on the harness
///   binary's parent process).
pub fn click(target: &WindowTarget, x: f64, y: f64) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        input::click_macos(target, x, y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, x, y);
        anyhow::bail!("periscope click is macOS-only (Phase 6-MVP)")
    }
}

/// Enumerate every visible window with its title / pid / app name.
/// Diagnostic surface for the `periscope list` CLI subcommand and for
/// debugging missing-window errors.
///
/// # Errors
///
/// Propagates `xcap` enumeration failures (rare; typically permission
/// issues).
pub fn list_windows() -> Result<Vec<WindowSummary>> {
    #[cfg(target_os = "macos")]
    {
        capture::list_macos()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(Vec::new())
    }
}
