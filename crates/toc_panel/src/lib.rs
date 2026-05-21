#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Table-of-contents panel for the Tolaria right dock (ADR-0115 Phase
//! 9 worklist 9.2.6).
//!
//! Shows the active note's heading outline as a scrollable, indented
//! list.  The headings are pushed up from the embedded editor via
//! [`editor_bridge::FromHost::Headings`] — the editor extracts
//! `heading` blocks from the BlockNote document and emits them
//! whenever the document changes.  The native chrome receives the
//! payload through the `NoteItem` IPC pipe and forwards it to the
//! `TocPanel` via the workspace's `HeadingsUpdate` event (see
//! `note_item` + `tolaria/main.rs`).
//!
//! # React parity
//!
//! Mirrors the structural shape of `src/components/TableOfContentsPanel.tsx`:
//! a vertical list, depth-indented by heading level, with a header
//! row and a scrollable body.  Heading-click navigation to the
//! corresponding body anchor is **deferred** — it would need a new
//! [`editor_bridge::ToHost`] envelope (`ScrollToAnchor` or similar)
//! that doesn't exist yet.  The MVP renders the outline and logs the
//! click; downstream rows pick up the bridge envelope when it lands.
//!
//! # Usage
//!
//! ```rust,ignore
//! let panel = cx.new(|_window, _cx| toc_panel::TocPanel::new());
//! workspace.attach_right_dock(panel.clone(), cx);
//! // Later, when the editor emits headings:
//! panel.update(cx, |panel, cx| panel.set_headings(items, cx));
//! ```

use editor_bridge::Heading;
use gpui::{
    div, px, AnyElement, App, Context, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{scroll::ScrollableElement as _, ActiveTheme, IconName};
use workspace::DockPosition;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Emitted when the user clicks a heading row.  Workspace subscribers
/// route this to a future `editor_bridge::ToHost::ScrollToAnchor`
/// envelope; until that lands the event is logged but unused.
#[derive(Debug, Clone)]
pub struct TocHeadingClicked {
    /// Anchor id from the original [`Heading::anchor`] payload —
    /// either a BlockNote block id or a slug fallback.
    pub anchor: SharedString,
}

// ---------------------------------------------------------------------------
// TocPanel
// ---------------------------------------------------------------------------

/// Right-dock panel rendering the active note's heading outline.
///
/// Construct via [`TocPanel::new`] for an empty panel or
/// [`TocPanel::from_or_empty`] to mirror the construction shape of the
/// other dock-panel crates (`ai_panel`, `inspector_panel`).  Headings
/// are updated by external callers via [`TocPanel::set_headings`]
/// after every [`editor_bridge::FromHost::Headings`] envelope.
pub struct TocPanel {
    /// Last heading set received from the editor.  Re-rendered on
    /// every update.
    headings: Vec<Heading>,
    position: DockPosition,
}

impl TocPanel {
    /// Empty panel — no headings loaded.
    pub fn new() -> Self {
        Self {
            headings: Vec::new(),
            position: DockPosition::Right,
        }
    }

    /// Empty-or-from-mock constructor mirroring the shape used by
    /// `ai_panel::AiPanel` and `inspector_panel::InspectorPanel`.
    /// There is no mock-fixture surface for headings yet — the editor
    /// is the sole producer — so this currently just returns
    /// [`Self::new`].  Kept for symmetry so callers don't special-case
    /// this panel when wiring `TOLARIA_MOCK=1` startup.
    pub fn from_or_empty(_cx: &mut App) -> Self {
        Self::new()
    }

    /// Replace the current heading list and request a redraw.  Called
    /// from the workspace subscriber that receives
    /// [`editor_bridge::FromHost::Headings`] payloads via the active
    /// `NoteItem`'s IPC pipe.
    ///
    /// Short-circuits when the new list is byte-identical to the
    /// previous one — the editor's `onChange` debounce can still emit
    /// duplicate payloads on rapid keystrokes that don't touch any
    /// heading, and a `cx.notify()` here would cascade through the
    /// workspace's `cx.observe(&right_dock, …)` watcher and re-render
    /// the whole right-dock column for no visible change.
    pub fn set_headings(&mut self, items: Vec<Heading>, cx: &mut Context<Self>) {
        if self.headings == items {
            return;
        }
        self.headings = items;
        cx.notify();
    }

    /// Read-only view of the current heading list.  Useful for tests
    /// and for downstream rows that compose the panel with additional
    /// state (e.g. the future inspector-panel `Outline` section in
    /// worklist 9.2.8).
    pub fn headings(&self) -> &[Heading] {
        &self.headings
    }
}

impl Default for TocPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EventEmitter
// ---------------------------------------------------------------------------

impl EventEmitter<TocHeadingClicked> for TocPanel {}

// ---------------------------------------------------------------------------
// Panel impl
// ---------------------------------------------------------------------------

impl workspace::Panel for TocPanel {
    fn persistent_name(&self) -> &str {
        "TocPanel"
    }

    fn panel_key(&self) -> &str {
        "toc"
    }

    fn position(&self, _cx: &App) -> DockPosition {
        self.position
    }

    fn set_position(&mut self, position: DockPosition, _cx: &mut Context<Self>) {
        self.position = position;
    }

    fn default_size(&self, _cx: &App) -> Pixels {
        // Matches the React `TableOfContentsPanel` default column
        // width (`src/App.tsx` lays it out at ~300 pt); the user can
        // still resize via the workspace's resizable group.
        px(300.0)
    }

    fn icon(&self) -> Option<&str> {
        Some("list_bullets")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(actions::ToggleTableOfContents)
    }

    fn starts_open(&self, _cx: &App) -> bool {
        // Worklist 9.2.6 — the action handler in `tolaria/main.rs` is
        // the gate: it only attaches a TocPanel on user dispatch, so
        // by the time `Dock::set_panel` reads `starts_open` the user
        // has already asked for the panel.  Returning `true` here
        // means "visible immediately on attach" rather than
        // "attached-but-hidden, requiring a second toggle".  Future
        // right-dock panels (9.2.5 AI, 9.2.8 Inspector) can pick
        // their own value here; the dock attaches whichever the
        // action handler last selected.
        true
    }
}

// ---------------------------------------------------------------------------
// Render helpers
// ---------------------------------------------------------------------------

/// Indent applied per heading level above 1.  Level-2 → 12 pt,
/// level-3 → 24 pt, etc.  Mirrors the proportional indent the React
/// `TocRow` uses (`getFolderDepthIndent`), simplified for the GPUI
/// surface which doesn't carry the React tree's per-depth connector
/// rails.
const PER_LEVEL_INDENT_PT: f32 = 12.0;

/// Compute the left padding for a heading row.  Clamps to a
/// reasonable maximum so a very deep heading (e.g. `######`) doesn't
/// push the title off-screen.
fn indent_for_level(level: u8) -> Pixels {
    // `level` arrives unsanitised from the wire; treat anything < 1
    // as level 1 and clamp > 6 to 6 so the indent never explodes.
    let clamped = level.clamp(1, 6);
    px(f32::from(clamped - 1) * PER_LEVEL_INDENT_PT)
}

/// Render the header strip above the heading list.  Mirrors React's
/// `TableOfContentsHeader` (icon + title + close button); the close
/// button is omitted because the GPUI workspace toggles the dock
/// through the action handler in `tolaria/src/main.rs`, not from
/// inside the panel.
fn render_header(theme: &gpui_component::theme::Theme) -> AnyElement {
    let muted = theme.muted_foreground;
    let border = theme.border;
    let title: SharedString = "Table of Contents".into();
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(52.0))
        .px(px(12.0))
        .border_b_1()
        .border_color(border)
        .text_color(muted)
        .child(div().text_color(muted).child(IconName::Menu))
        .child(div().text_sm().child(title))
        .into_any_element()
}

/// Render a single heading row.  Click handler emits
/// [`TocHeadingClicked`]; the body-navigation hop lands when a
/// `ToHost::ScrollToAnchor` envelope ships.
fn render_heading_row(
    index: usize,
    heading: &Heading,
    fg: gpui::Hsla,
    muted: gpui::Hsla,
    accent: gpui::Hsla,
) -> AnyElement {
    let text: SharedString = heading.text.clone().into();
    let anchor: SharedString = heading.anchor.clone().into();
    let level_text_color = if heading.level <= 1 { fg } else { muted };
    // Stable id per row so GPUI keeps the click hitbox stable across
    // re-renders when the headings list changes shape (e.g. user
    // adds a new H2 above an existing H3).
    let row_id = SharedString::from(format!("toc-row-{index}"));
    div()
        .id(row_id)
        .flex()
        .items_center()
        .h(px(28.0))
        .pl(indent_for_level(heading.level) + px(12.0))
        .pr(px(12.0))
        .text_sm()
        .text_color(level_text_color)
        .cursor_pointer()
        .hover(move |this| this.bg(accent))
        .on_click(move |_, _window, _cx| {
            // TODO(9.2.6-followup): emit `TocHeadingClicked` and wire
            // a `ToHost::ScrollToAnchor` envelope so the editor
            // scrolls to the matching block.  Today we only log so
            // downstream rows pick up the path.
            log::info!(
                target: "toc_panel",
                "heading clicked: anchor={anchor:?}",
            );
        })
        .child(text)
        .into_any_element()
}

/// Render the placeholder shown when no headings have arrived yet.
/// Distinct from "active note has no headings" only on the wire (the
/// `Headings { items: [] }` envelope), but visually identical — both
/// tell the user there's nothing to navigate.
fn render_empty_placeholder(muted: gpui::Hsla) -> AnyElement {
    let label: SharedString = "No headings yet.".into();
    div()
        .px(px(12.0))
        .py(px(12.0))
        .text_sm()
        .text_color(muted)
        .child(label)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for TocPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let accent = theme.accent;

        let body: AnyElement = if self.headings.is_empty() {
            render_empty_placeholder(muted)
        } else {
            let rows: Vec<AnyElement> = self
                .headings
                .iter()
                .enumerate()
                .map(|(i, h)| render_heading_row(i, h, fg, muted, accent))
                .collect();
            div()
                .flex_1()
                .overflow_y_scrollbar()
                .py(px(4.0))
                .children(rows)
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(render_header(theme))
            .child(body)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, TestAppContext};
    use workspace::Panel as _;

    fn h(level: u8, text: &str, anchor: &str) -> Heading {
        Heading {
            level,
            text: text.into(),
            anchor: anchor.into(),
        }
    }

    /// Install the `gpui_component::Theme` global required by any view
    /// that reads it during render.
    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    #[gpui::test]
    fn empty_panel_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| TocPanel::new());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn from_or_empty_does_not_panic(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let panel = TocPanel::from_or_empty(cx);
            assert!(panel.headings.is_empty());
        });
    }

    /// Panel position is the right dock — the toggle action attaches
    /// it to the workspace's right column, not the left.
    #[gpui::test]
    fn panel_position_is_right(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let panel = TocPanel::new();
            assert_eq!(
                panel.position(cx),
                DockPosition::Right,
                "TocPanel must occupy the Right dock",
            );
        });
    }

    /// `set_headings` mutates the state and triggers a re-render.
    /// Without `cx.notify()` the dock would render the previous list
    /// even after the editor emits a fresh `Headings` envelope.
    #[gpui::test]
    fn set_headings_updates_state_and_renders(cx: &mut TestAppContext) {
        install_theme(cx);

        let panel = cx.update(|cx| cx.new(|_| TocPanel::new()));

        panel.update(cx, |panel, cx| {
            panel.set_headings(
                vec![
                    h(1, "Top", "block-a"),
                    h(2, "Sub", "block-b"),
                    h(3, "Deep", "block-c"),
                ],
                cx,
            );
        });
        cx.run_until_parked();

        panel.read_with(cx, |panel: &TocPanel, _cx| {
            let got: Vec<_> = panel.headings().iter().map(|h| h.text.as_str()).collect();
            assert_eq!(got, vec!["Top", "Sub", "Deep"]);
        });
    }

    /// Indent scales with heading level — level-1 gets zero indent,
    /// level-2 gets `PER_LEVEL_INDENT_PT`, and so on, with a clamp at
    /// level 6 so a runaway depth can't push titles off-screen.
    #[test]
    fn indent_clamps_to_level_six() {
        assert_eq!(indent_for_level(1), px(0.0));
        assert_eq!(indent_for_level(2), px(PER_LEVEL_INDENT_PT));
        assert_eq!(indent_for_level(3), px(PER_LEVEL_INDENT_PT * 2.0));
        // Level above the clamp must not exceed the level-6 indent.
        assert_eq!(indent_for_level(99), px(PER_LEVEL_INDENT_PT * 5.0));
        // Level 0 (wire-bogus) treated as 1.
        assert_eq!(indent_for_level(0), px(0.0));
    }

    /// Rendering with a non-empty heading list must not panic — the
    /// row builder relies on `enumerate`+`SharedString` plumbing that
    /// would explode if a downstream refactor broke the id pattern.
    #[gpui::test]
    fn render_with_headings_does_not_panic(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, cx| {
            let mut panel = TocPanel::new();
            panel.set_headings(
                vec![
                    h(1, "First", "a"),
                    h(2, "Second", "b"),
                    h(2, "Third", "c"),
                    h(3, "Fourth", "d"),
                ],
                cx,
            );
            panel
        });
        cx.run_until_parked();
    }
}
