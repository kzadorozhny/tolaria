//! macOS input-synthesis adapter for `periscope::click`.
//!
//! Posts low-level `CGEvent` mouse events via `CGEventCreateMouseEvent`.
//! GPUI draws its own controls into the Metal layer, so the Accessibility
//! API hierarchy doesn't see them — `AXUIElementPerformAction` is not an
//! option.  Synthesising at the OS event-queue level is the only path
//! that actually reaches GPUI's hit-testing.

use anyhow::{anyhow, Context as _, Result};
use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

use crate::WindowTarget;

/// Click a point inside `target` window.
///
/// `x` / `y` are in window-local points (origin at the window's top-left,
/// matching GPUI's coordinate convention).  We translate to global screen
/// space using `xcap::Window::x()` + `.y()` before posting the event.
///
/// The function emits one full click cycle (mouse-down then mouse-up at the
/// same point) and waits ~20 ms in between — empirically required for GPUI's
/// gesture recognizer to register the press / release pair as a click.
pub(crate) fn click_macos(target: &WindowTarget, x: f64, y: f64) -> Result<()> {
    let window = crate::capture::find_window(target)?;
    let win_x = window.x().context("xcap::Window::x")?;
    let win_y = window.y().context("xcap::Window::y")?;

    let screen_x = f64::from(win_x) + x;
    let screen_y = f64::from(win_y) + y;
    let point = CGPoint::new(screen_x, screen_y);

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("CGEventSource::new(HIDSystemState) failed"))?;

    let down = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    )
    .map_err(|_| anyhow!("CGEvent::new_mouse_event(LeftMouseDown) failed"))?;
    down.post(core_graphics::event::CGEventTapLocation::HID);

    // Small inter-event delay so GPUI's gesture recognizer sees a discrete
    // press / release pair.  Without this the click is sometimes coalesced
    // with the next event in the queue and the underlying handler doesn't
    // fire (mirrors the wait you'd see in AppleScript's `delay 0.02`).
    std::thread::sleep(CLICK_PRESS_HOLD);

    let up = CGEvent::new_mouse_event(source, CGEventType::LeftMouseUp, point, CGMouseButton::Left)
        .map_err(|_| anyhow!("CGEvent::new_mouse_event(LeftMouseUp) failed"))?;
    up.post(core_graphics::event::CGEventTapLocation::HID);

    log::info!(
        "click: window={target} window-local=({x:.1},{y:.1}) screen=({screen_x:.1},{screen_y:.1})"
    );
    Ok(())
}

/// Time the synthetic press is held before release.  Empirically tuned —
/// anything below ~10 ms occasionally lands as a "no click" on a busy
/// machine; 20 ms is safe without being perceptibly slow.
const CLICK_PRESS_HOLD: std::time::Duration = std::time::Duration::from_millis(20);
