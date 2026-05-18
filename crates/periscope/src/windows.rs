//! macOS Accessibility-API adapter for raising windows on demand.
//!
//! Lives behind [`super::raise`].  Uses `accessibility` crate to
//! reach into other processes' window hierarchies — requires
//! Accessibility permission on the harness binary (separate from
//! Screen Recording).

use anyhow::{anyhow, Context as _, Result};

use crate::WindowTarget;

pub(crate) fn raise_macos(target: &WindowTarget) -> Result<()> {
    use accessibility::{AXAttribute, AXUIElement, AXUIElementActions};
    use core_foundation::string::CFString;

    let pid = resolve_pid(target)?;
    let pid_i32: i32 = pid.try_into().with_context(|| {
        format!("pid {pid} exceeds i32::MAX (AXUIElement::application takes i32)")
    })?;
    let app = AXUIElement::application(pid_i32);

    let windows = app.attribute(&AXAttribute::windows()).map_err(|err| {
        anyhow!(
            "AXUIElement.windows attribute fetch failed (Accessibility \
             permission missing for {term}?): {err:?}",
            term = std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "<unknown>".into()),
        )
    })?;

    // `CFArray::iter` yields `ItemRef<AXUIElement>` which derefs to
    // `&AXUIElement` — no `unsafe` needed.
    match target {
        WindowTarget::ByPid(_) => {
            if let Some(window) = windows.iter().next() {
                window
                    .raise()
                    .map_err(|err| anyhow!("AXUIElement::raise failed: {err:?}"))?;
                return Ok(());
            }
            Err(anyhow!("pid {pid} owns zero AX windows"))
        }
        WindowTarget::ByTitle(want) => {
            // Build the comparison `CFString` once so the inner loop
            // compares CFString-to-CFString without re-allocating each
            // iteration (clippy::cmp_owned).
            let want_cf = CFString::new(want);
            for window in windows.iter() {
                let Ok(title) = window.attribute(&AXAttribute::title()) else {
                    continue;
                };
                if title == want_cf {
                    window
                        .raise()
                        .map_err(|err| anyhow!("AXUIElement::raise failed: {err:?}"))?;
                    return Ok(());
                }
            }
            Err(anyhow!("no AX window with title {want:?} under pid {pid}"))
        }
    }
}

fn resolve_pid(target: &WindowTarget) -> Result<u32> {
    match target {
        WindowTarget::ByPid(pid) => Ok(*pid),
        WindowTarget::ByTitle(title) => {
            // Look up via xcap so we keep one canonical window-enumeration
            // path; AX-side enumeration is per-process and harder to scan.
            let window = crate::capture::find_window(&WindowTarget::ByTitle(title.clone()))
                .context("resolving pid via xcap for raise(ByTitle)")?;
            window.pid().context("xcap::Window::pid")
        }
    }
}
