//! Debug-only GPUI element-tree dump over SIGUSR1
//! (ADR-0115 Phase 6 follow-up).
//!
//! Tolaria does not ship a real DOM — GPUI elements are recreated on
//! every paint and have no stable cross-process identity.  For
//! periscope to target click events by *name* instead of by pixel
//! coordinate, individual views opt their important children into a
//! tiny named-bounds registry via the [`DumpAs`] element wrapper:
//!
//! ```rust,ignore
//! use ui::tree_dump::DumpAsExt as _;
//! div().id("status-bar-theme-toggle").dump_as("theme_toggle")
//! ```
//!
//! On every paint pass the wrapper writes the laid-out
//! `Bounds<Pixels>` into a process-global registry keyed by the
//! string.  Sending `SIGUSR1` to the Tolaria process then triggers a
//! background thread to snapshot the registry to a JSON file at
//! `$TMPDIR/tolaria-ui-tree-<pid>.json` (path passed in by the caller via
//! [`install_signal_handler`]).
//!
//! Periscope reads that file, looks up the requested name, and
//! synthesises a click at the centre of the recorded bounds.  See
//! `crates/periscope/src/bin/periscope.rs` (`click-id` subcommand).
//!
//! # Scope
//!
//! - The registry stores **window-local** logical-pixel bounds, which
//!   is exactly the coordinate space periscope's `click` subcommand
//!   accepts.  No screen-space conversion is needed.
//! - The wrapper is **always compiled** so production code paths that
//!   sprinkle `.dump_as(…)` calls don't bifurcate via `#[cfg]`.
//!   `register` is a `Mutex<BTreeMap>` insert — cheap enough that
//!   leaving it on in release builds is fine; release builds simply
//!   never install a signal handler so nobody reads the registry.
//! - Signal-driven IPC is **debug-only** by convention: callers gate
//!   [`install_signal_handler`] on `cfg(debug_assertions)`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use anyhow::{Context as _, Result};
use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Window,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// One entry in the dump file.  Bounds are in **window-local logical
/// pixels** — the same coordinate space `periscope click --x --y`
/// expects, so no transform is required between dump and click.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NamedBounds {
    /// Top-left x (logical points, origin top-left of window).
    pub x: f32,
    /// Top-left y.
    pub y: f32,
    /// Width in logical points.
    pub width: f32,
    /// Height in logical points.
    pub height: f32,
}

impl From<Bounds<Pixels>> for NamedBounds {
    fn from(b: Bounds<Pixels>) -> Self {
        Self {
            x: f32::from(b.origin.x),
            y: f32::from(b.origin.y),
            width: f32::from(b.size.width),
            height: f32::from(b.size.height),
        }
    }
}

/// On-disk wire format.  `sequence` is a monotonic counter bumped by
/// every successful [`dump_to`] call; periscope polls it to detect
/// fresh dumps without depending on filesystem `mtime` resolution
/// (which can collapse two back-to-back writes into the same value
/// on slow clocks or odd filesystems).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumpFile {
    /// Monotonic dump counter — strictly increases across the lifetime
    /// of a single Tolaria process.
    pub sequence: u64,
    /// Element name → laid-out bounds.
    pub entries: BTreeMap<String, NamedBounds>,
}

/// Single-mutex registry state.  Holding `map`, `y_offset_pt`, and
/// `sequence` under the same lock eliminates the torn-read race that
/// a separate atomic offset would otherwise introduce — every
/// `register` sees a coherent `(offset, map_slot)` pair.
struct RegistryState {
    map: BTreeMap<String, NamedBounds>,
    /// Logical-point y-offset added to every registered `bounds.y`
    /// before insertion.  See [`set_window_y_offset`].
    y_offset_pt: f32,
    /// Monotonic dump counter; bumped by [`dump_to`] before write.
    sequence: u64,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
            y_offset_pt: 0.0,
            sequence: 0,
        }
    }
}

static REGISTRY: Lazy<Mutex<RegistryState>> = Lazy::new(|| Mutex::new(RegistryState::default()));

/// Take the registry lock, recovering from poison rather than dropping
/// the registration on the floor.  Poison only happens if a previous
/// holder panicked while inside the critical section — for tree_dump
/// that's effectively "never," but keeping the recovery path explicit
/// means a future panic doesn't silently disable element registration.
fn lock_registry() -> std::sync::MutexGuard<'static, RegistryState> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Set the y-offset (in logical points) added to every subsequently
/// registered bounds.  Idempotent — repeated calls overwrite.  Call
/// this once during application startup, before any view renders.
///
/// GPUI hands `paint` callbacks bounds in **content-area** coordinates
/// — the top of GPUI's drawable area, which on macOS sits *below* the
/// native title bar.  `periscope click --x --y` and `xcap::Window::y()`
/// use **window-frame** coordinates that *include* the title bar.  The
/// workspace passes its native-title-bar-spacer height
/// ([`workspace::NATIVE_TITLE_BAR_HEIGHT`]) here at startup, and every
/// `register` adds it before storing — so the JSON dump is already in
/// the same coordinate system periscope clicks against.
pub fn set_window_y_offset(pt: f32) {
    lock_registry().y_offset_pt = pt;
}

/// Record `name` → `bounds`.  Subsequent calls with the same name
/// overwrite; this is what keeps the registry fresh across window
/// resizes and re-layouts (every paint cycle re-registers).
///
/// Cheap — single `Mutex` insert per paint of an opted-in element.
/// The stored `y` is `bounds.y + window_y_offset` (see
/// [`set_window_y_offset`]).
///
/// `pub(crate)`-visibility: callers outside this module should always
/// go through the [`DumpAs`] element wrapper, which calls `register`
/// from its `paint` lifecycle hook with the post-layout bounds.
/// Bypassing the wrapper with raw `register("name", bounds)` calls
/// would record stale or invented coordinates that don't track
/// re-layout.
pub(crate) fn register(name: &str, bounds: Bounds<Pixels>) {
    let mut nb: NamedBounds = bounds.into();
    let mut state = lock_registry();
    nb.y += state.y_offset_pt;
    state.map.insert(name.to_string(), nb);
}

/// Find the registered element whose bounds contain `point` and
/// return its name + bounds — the topmost-by-specificity match
/// (smallest area), since GPUI elements nest and a hover usually wants
/// the leaf, not the root.  Coordinates are window-local logical
/// points (same space as the registered bounds, same space as
/// [`gpui::Window::mouse_position`]).
///
/// General-purpose point-in-bounds lookup over the dump registry.
/// Currently unused by the worklist 10.1.4 inspector (which uses
/// GPUI's broader per-paint hitbox machinery instead of just the
/// `.dump_as`-tagged subset), but kept as a public primitive for any
/// future name-keyed hit-testing — periscope click-by-id and the
/// dump-tree CLI both already lean on the same registry.
#[must_use]
pub fn lookup_at(point: gpui::Point<gpui::Pixels>) -> Option<(String, NamedBounds)> {
    let x = f32::from(point.x);
    let y = f32::from(point.y);
    let state = lock_registry();
    state
        .map
        .iter()
        .filter(|(_, nb)| x >= nb.x && x < nb.x + nb.width && y >= nb.y && y < nb.y + nb.height)
        // Prefer the smallest area on ties — nested elements (e.g. a
        // button inside a sidebar row) make the leaf the more useful
        // hover target.  `BTreeMap::iter` is ordered by key, so
        // `min_by` stays deterministic across runs (no random tie-
        // breaking on equal-area matches).
        .min_by(|(_, a), (_, b)| {
            let area_a = a.width * a.height;
            let area_b = b.width * b.height;
            area_a
                .partial_cmp(&area_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(name, nb)| (name.clone(), *nb))
}

/// Snapshot the registry to a JSON file at `path`.  Atomic via a
/// `.tmp` file + rename so periscope can poll the path without ever
/// reading a half-written file.  Bumps the on-disk `sequence`
/// counter before writing so periscope can detect "the file
/// changed" without relying on filesystem `mtime`.
pub fn dump_to(path: &Path) -> Result<()> {
    let dump = {
        let mut state = lock_registry();
        state.sequence += 1;
        DumpFile {
            sequence: state.sequence,
            entries: state.map.clone(),
        }
    };
    let json = serde_json::to_string_pretty(&dump).context("serialise registry to JSON")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).with_context(|| format!("write dump to {tmp:?}"))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename {tmp:?} → {path:?}"))?;
    Ok(())
}

/// Convenience: standard dump path under `$TMPDIR` (falls back to
/// `/tmp/`), uniquely keyed by the current process's PID so multiple
/// Tolaria instances can run side-by-side without stomping on each
/// other's dump files.  Periscope re-derives the same path from the
/// target window's PID — keep this naming in sync with
/// `crates/periscope/src/bin/periscope.rs`.
pub fn default_dump_path_for_pid(pid: u32) -> PathBuf {
    let tmp = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    tmp.join(format!("tolaria-ui-tree-{pid}.json"))
}

// ---------------------------------------------------------------------------
// Signal handler (Unix only — Tolaria is macOS-only today)
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod unix_signal {
    use super::dump_to;
    use anyhow::Result;
    use signal_hook::{consts::SIGUSR1, iterator::Signals};
    use std::path::PathBuf;
    use std::thread;

    /// Spawn a daemon thread that consumes `SIGUSR1` and writes a
    /// fresh dump to `path` each time the signal arrives.
    ///
    /// `Signals::new` installs the OS-level handler via
    /// `signal-hook`'s atomic-pipe machinery, so the signal-delivery
    /// path itself is async-signal-safe; the actual file IO runs on
    /// this dedicated thread (never inside the signal handler).
    pub fn install(path: PathBuf) -> Result<()> {
        let mut signals = Signals::new([SIGUSR1])?;
        let handler_path = path.clone();
        thread::Builder::new()
            .name("tree_dump.sigusr1".into())
            .spawn(move || {
                log::info!(
                    "tree_dump SIGUSR1 handler armed; dump path = {:?}",
                    handler_path
                );
                for _ in signals.forever() {
                    match dump_to(&handler_path) {
                        Ok(()) => log::info!("tree_dump wrote {:?}", handler_path),
                        Err(err) => log::error!("tree_dump SIGUSR1 dump failed: {err:#}"),
                    }
                }
            })?;
        Ok(())
    }
}

/// Install a SIGUSR1-triggered dump handler that writes the registry
/// to `path` whenever the process receives `SIGUSR1`.
///
/// Call this once during application startup, gated on
/// `cfg(debug_assertions)` so release builds don't ship the
/// developer-facing IPC channel.  Returns immediately after spawning
/// the handler thread; the thread persists for the rest of the
/// process lifetime.
#[cfg(unix)]
pub fn install_signal_handler(path: PathBuf) -> Result<()> {
    unix_signal::install(path)
}

/// Stub for non-Unix platforms; signal-driven IPC is not available so
/// this is a no-op.  Kept callable so the call site in
/// `crates/tolaria/src/main.rs` doesn't need its own `cfg` gate.
#[cfg(not(unix))]
pub fn install_signal_handler(_path: PathBuf) -> Result<()> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Element wrapper — `.dump_as("name")` on any element
// ---------------------------------------------------------------------------

/// Element wrapper that records its laid-out bounds under `name`
/// each paint cycle.  Constructed via [`DumpAsExt::dump_as`].
pub struct DumpAs<E> {
    inner: E,
    name: &'static str,
}

impl<E: Element> Element for DumpAs<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector_id, bounds, request_layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        register(self.name, bounds);
        self.inner.paint(
            id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        );
    }
}

impl<E: Element> IntoElement for DumpAs<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Extension trait that adds `.dump_as(name)` to every [`Element`].
///
/// `name` is a `&'static str` so the captured pointer can outlive the
/// element without an allocation — most call sites use a string
/// literal anyway (`.dump_as("theme_toggle")`).
pub trait DumpAsExt: Element + Sized {
    /// Wrap this element so its laid-out bounds are recorded under
    /// `name` in the process-global registry on every paint pass.
    fn dump_as(self, name: &'static str) -> DumpAs<Self> {
        DumpAs { inner: self, name }
    }
}

impl<E: Element> DumpAsExt for E {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    /// `register` + `dump_to` must round-trip through JSON.
    #[test]
    fn registry_round_trips_through_json() {
        let bounds = Bounds {
            origin: point(px(10.0), px(20.0)),
            size: size(px(100.0), px(30.0)),
        };
        register("round-trip-test", bounds);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dump.json");
        dump_to(&path).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: DumpFile = serde_json::from_str(&raw).unwrap();
        assert!(parsed.sequence >= 1, "sequence must be bumped on dump");
        let got = parsed
            .entries
            .get("round-trip-test")
            .expect("registered name must appear in dump");
        assert_eq!(got.x, 10.0);
        assert_eq!(got.width, 100.0);
        assert_eq!(got.height, 30.0);
    }

    /// Sequential `dump_to` calls must bump the on-disk sequence so
    /// periscope can detect freshness without depending on filesystem
    /// `mtime` resolution.
    #[test]
    fn sequence_increases_monotonically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dump.json");
        dump_to(&path).unwrap();
        let s1: DumpFile = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        dump_to(&path).unwrap();
        let s2: DumpFile = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            s2.sequence > s1.sequence,
            "sequence must strictly increase ({} → {})",
            s1.sequence,
            s2.sequence,
        );
    }

    /// `register` overwrites in place: subsequent calls with the same
    /// name keep the registry fresh across re-layouts.
    #[test]
    fn register_overwrites_existing_name() {
        let first = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(10.0), px(10.0)),
        };
        let second = Bounds {
            origin: point(px(50.0), px(50.0)),
            size: size(px(20.0), px(20.0)),
        };
        register("overwrite-test", first);
        register("overwrite-test", second);

        let state = lock_registry();
        let got = state.map.get("overwrite-test").unwrap();
        assert_eq!(got.x, 50.0, "second register must replace the first");
    }

    /// `default_dump_path_for_pid` is deterministic given the same PID.
    #[test]
    fn default_path_is_pid_keyed() {
        let p1 = default_dump_path_for_pid(12345);
        let p2 = default_dump_path_for_pid(12345);
        let p3 = default_dump_path_for_pid(99999);
        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
        assert!(p1.to_string_lossy().contains("12345"));
    }

    /// Worklist 10.1.4 — `lookup_at` returns the leaf match when
    /// elements nest.  Registers an outer container + an inner button
    /// where the button's bounds are strictly inside the container's,
    /// then checks that a hit inside the button returns the button
    /// (not the container).  The smallest-area tie-break is what
    /// makes the inspector picker name the most specific element
    /// under the cursor.
    #[test]
    fn lookup_at_returns_smallest_match_when_nested() {
        let outer = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1000.0), px(1000.0)),
        };
        let inner = Bounds {
            origin: point(px(100.0), px(100.0)),
            size: size(px(50.0), px(50.0)),
        };
        register("lookup-outer", outer);
        register("lookup-inner", inner);

        let hit = lookup_at(point(px(120.0), px(120.0)));
        assert!(hit.is_some(), "point inside both bounds must resolve");
        let (name, _) = hit.unwrap();
        assert_eq!(
            name, "lookup-inner",
            "leaf match wins: smallest area is the more useful hover target"
        );
    }

    /// `lookup_at` returns `None` when no registered element
    /// contains the point.  Pins the empty-match branch so callers
    /// can render an empty state without an explicit existence
    /// check.  Uses an absurd coordinate (`1_000_000`, well outside
    /// any other test's registered bounds — the registry is a
    /// process-global `Mutex<BTreeMap>` shared across all tests in
    /// this module) so a sibling test's leftover registration can't
    /// false-positive this assertion.
    #[test]
    fn lookup_at_returns_none_when_no_match() {
        let bounds = Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(10.0), px(10.0)),
        };
        register("lookup-miss-target", bounds);

        let hit = lookup_at(point(px(1_000_000.0), px(1_000_000.0)));
        assert!(
            hit.is_none(),
            "point outside every registered bound resolves to None"
        );
    }
}
