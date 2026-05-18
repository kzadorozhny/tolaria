//! Open-note flow (ADR-0115 Phase 5d).
//!
//! Bridges `note_list_pane::OpenNoteEvent` → `note_item::NoteItem`
//! mounted in the workspace's center [`workspace::PaneGroup`].
//!
//! Lives in the binary crate (not in `workspace` or `note_item`) so the
//! type graph stays a forest: `note_item` already depends on
//! `workspace`, and the binary depends on both — adding the converse
//! edge would create a cycle.

#![cfg(target_os = "macos")]

use anyhow::{Context as _, Result};
use gpui::{Context, Window};
use note_item::NoteItem;
use vault::{NoteId, Vault};
use workspace::TolariaWorkspace;

/// Open a note in the workspace's active center [`workspace::Pane`].
///
/// Reads the body via [`Vault::note_content`], constructs a
/// [`NoteItem`] with a live `WKWebView`, and pushes it onto the active
/// pane via [`TolariaWorkspace::add_item_to_active_pane`].
///
/// Subscribed to `NoteListPane::OpenNoteEvent` from the `tolaria`
/// binary's `cx.open_window` closure; the wiring lives in `main.rs`.
///
/// # Errors
///
/// - `Vault` global is not installed (no `--vault <path>` at startup).
/// - The note id is unknown to the vault.
/// - `NoteItem::new_with_webview` fails (window-handle race or wry
///   build failure).
///
/// The current body is read but **not yet** delivered to the WebView
/// — that handshake happens once the editor host emits
/// `FromHost::Ready` (Phase 5e wires the channel that routes Ready
/// back into the entity).  Until then, the note opens with an empty
/// editor view that the user can populate by retriggering the open
/// flow.
pub fn open_note(
    workspace: &TolariaWorkspace,
    id: NoteId,
    window: &mut Window,
    cx: &mut Context<TolariaWorkspace>,
) -> Result<()> {
    let vault = cx
        .try_global::<Vault>()
        .context("Vault global is not installed; pass --vault <path> at startup")?;
    let executor = cx.foreground_executor().clone();
    let note = executor
        .block_on(vault.note(id))
        .with_context(|| format!("note {id:?} not found in vault"))?;
    let _body = executor
        .block_on(vault.note_content(id))
        .with_context(|| format!("reading body for note {id:?}"))?;

    let note_item = NoteItem::new_with_webview(note, window, cx)
        .context("constructing NoteItem with embedded WKWebView")?;

    // Call `add_item_to_active_pane` directly on `&TolariaWorkspace`
    // rather than re-entering via `workspace.update(cx, ...)`.  The
    // caller (the `subscribe_in` closure in `main.rs`) is already
    // executing inside the workspace entity's update context — wrapping
    // in another `.update()` panics with
    // "cannot update TolariaWorkspace while it is already being updated"
    // the moment a real click fires `OpenNoteEvent`.  `add_item_to_active_pane`
    // takes `&self`, so direct invocation is sound and avoids the
    // re-entrancy guard.
    workspace.add_item_to_active_pane(note_item, cx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use gpui::{AppContext as _, SharedString};
    use std::path::PathBuf;
    use vault::{Note, NoteKind};
    use workspace::TolariaWorkspace;

    /// Verify the workspace's `add_item_to_active_pane` populates the
    /// center pane.  Uses `NoteItem::new_for_tests` (no live WebView)
    /// so the test runs in a headless `TestAppContext`.
    #[gpui::test]
    fn add_item_creates_pane_and_activates_item(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let window = cx.add_window(TolariaWorkspace::empty);
        let note = Note {
            id: NoteId::from_raw(7),
            title: SharedString::from("Test Note"),
            path: PathBuf::from("/v/test.md"),
            kind: NoteKind::Markdown,
            modified: Utc::now(),
            byte_size: 0,
        };
        let item = cx.update(|cx| cx.new(|_| NoteItem::new_for_tests(note)));
        window
            .update(cx, |ws_view, _window, cx| {
                ws_view.add_item_to_active_pane(item, cx);
                assert_eq!(
                    ws_view.active_pane_item_count(cx),
                    1,
                    "active pane must hold the freshly added NoteItem"
                );
            })
            .unwrap();
    }
}
