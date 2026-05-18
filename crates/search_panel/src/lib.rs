#![forbid(unsafe_code)]
//! Full-text search panel for the Bottom Dock (ADR-0115 Phase 2d).
//!
//! `SearchPanel` implements `workspace::Panel` and sits in the Bottom Dock.
//! It renders a query display strip on top and a scrollable result list below.
//! Each result row shows a note identifier (bold), a snippet excerpt, and a
//! relevance score.
//!
//! # Usage (mock mode)
//!
//! ```rust,ignore
//! cx.set_global(MockSearch::seeded());
//! // Pre-populate with results for "todo":
//! let panel = cx.new(|_window, cx| SearchPanel::with_query("todo", cx));
//! // Or start empty:
//! let panel = cx.new(|_window, _cx| SearchPanel::new());
//! ```

use gpui::{
    div, px, AnyElement, App, Context, IntoElement, ParentElement, Pixels, Render, SharedString,
    Styled, Window,
};
use gpui_component::{h_flex, scroll::ScrollableElement as _, v_flex, ActiveTheme, StyledExt as _};
use mock_fixtures::{MockSearch, SearchHit};
use workspace::{DockPosition, Panel};

// ---------------------------------------------------------------------------
// SearchPanel view
// ---------------------------------------------------------------------------

/// Full-text search panel rendered in the Bottom Dock.
///
/// Constructed via [`SearchPanel::new`] for an empty panel or
/// [`SearchPanel::with_query`] to populate results from the installed
/// [`MockSearch`] global.
///
/// # Note on note titles
///
/// Phase 2d renders note identifiers as `Note #N` because `search_panel` does
/// not depend on `MockVault`.  Phase 3 will resolve real titles via the vault
/// service.
pub struct SearchPanel {
    query: SharedString,
    hits: Vec<SearchHit>,
    position: DockPosition,
}

impl SearchPanel {
    /// Empty panel with no query and no results.
    #[must_use]
    pub fn new() -> Self {
        Self {
            query: SharedString::default(),
            hits: Vec::new(),
            position: DockPosition::Bottom,
        }
    }

    /// Build a panel pre-populated with results for `query` fetched from the
    /// installed [`MockSearch`] global.
    ///
    /// Uses the `Task::ready` + `block_on` pattern so the call is synchronous
    /// on the foreground thread (matching the Phase 2 mock pattern from
    /// `StatusBar::from_mock`).
    ///
    /// # Panics
    ///
    /// Panics if the [`MockSearch`] global is not installed on `cx`, or if
    /// `MockSearch::query` returns a non-ready task (i.e. `block_on` would
    /// block the foreground thread).  Phase 3 replaces this with an async
    /// service injection path.
    #[must_use]
    pub fn with_query(query: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let query: SharedString = query.into();
        let task = cx.global::<MockSearch>().query(&query);
        let hits = cx.foreground_executor().block_on(task);
        Self {
            query,
            hits,
            position: DockPosition::Bottom,
        }
    }
}

impl Default for SearchPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Panel trait
// ---------------------------------------------------------------------------

impl Panel for SearchPanel {
    fn persistent_name(&self) -> &str {
        "SearchPanel"
    }

    fn panel_key(&self) -> &str {
        "search"
    }

    fn position(&self, _cx: &App) -> DockPosition {
        self.position
    }

    fn set_position(&mut self, position: DockPosition, _cx: &mut Context<Self>) {
        self.position = position;
    }

    fn default_size(&self, _cx: &App) -> Pixels {
        px(200.0)
    }

    fn icon(&self) -> Option<&str> {
        Some("magnifying-glass")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        // Placeholder — Phase 2e will add a dedicated `ToggleSearchPanel`.
        Box::new(actions::ToggleSidebar)
    }

    fn starts_open(&self, _cx: &App) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for SearchPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query.clone();
        let border_color = cx.theme().border;
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        // Pre-collect display data before building the element tree to avoid
        // borrow-checker conflicts between immutable theme borrows and the
        // hit-list iterator.
        let hit_rows: Vec<(SharedString, SharedString, u32)> = self
            .hits
            .iter()
            .map(|h| {
                // Phase 3 will resolve real note titles via the vault service.
                let title = SharedString::from(format!("Note #{}", h.note_id.get()));
                let excerpt = SharedString::from(h.excerpt.as_str());
                let score_pct = (h.score.clamp(0.0, 1.0) * 100.0).round() as u32;
                (title, excerpt, score_pct)
            })
            .collect();

        // --- Query strip ---
        let query_bar = h_flex()
            .h(px(32.0))
            .px(px(8.0))
            .gap_2()
            .border_b_1()
            .border_color(border_color)
            .child(
                div()
                    .text_sm()
                    .text_color(if query.is_empty() { muted } else { fg })
                    .child(if query.is_empty() {
                        SharedString::from("Search\u{2026}")
                    } else {
                        query.clone()
                    }),
            );

        // --- Result list ---
        let result_list: AnyElement = if hit_rows.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(if query.is_empty() {
                            "Type to search"
                        } else {
                            "No results"
                        }),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .overflow_y_scrollbar()
                .children(hit_rows.into_iter().map(|(title, excerpt, score_pct)| {
                    div()
                        .px(px(8.0))
                        .py(px(6.0))
                        .border_b_1()
                        .border_color(border_color)
                        .child(div().text_sm().font_semibold().text_color(fg).child(title))
                        .child(div().text_sm().text_color(muted).child(excerpt))
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(SharedString::from(format!("Score: {score_pct}%"))),
                        )
                }))
                .into_any_element()
        };

        v_flex().size_full().child(query_bar).child(result_list)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use mock_fixtures::MockSearch;

    /// Install the `gpui_component::Theme` global required by any view that
    /// reads it during render (mirrors `embed_poc/src/layout.rs:243`).
    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// An empty search panel must render without panicking.
    #[gpui::test]
    fn empty_panel_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| SearchPanel::new());
        cx.run_until_parked();
    }

    /// `with_query("todo")` must return at least one hit from [`MockSearch`].
    #[gpui::test]
    fn query_returns_hits_from_mock(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockSearch::seeded());
        });
        let window = cx.add_window(|_window, cx| SearchPanel::with_query("todo", cx));
        window
            .update(cx, |panel, _window, _cx| {
                assert!(
                    !panel.hits.is_empty(),
                    "expected at least 1 hit for query \"todo\", got 0",
                );
            })
            .unwrap();
    }

    /// An unrecognised query must produce zero hits but must not panic.
    #[gpui::test]
    fn empty_results_render(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockSearch::seeded());
        });
        let window = cx.add_window(|_window, cx| SearchPanel::with_query("zzz-no-match", cx));
        window
            .update(cx, |panel, _window, _cx| {
                assert!(
                    panel.hits.is_empty(),
                    "expected 0 hits for \"zzz-no-match\", got {}",
                    panel.hits.len(),
                );
            })
            .unwrap();
        cx.run_until_parked();
    }

    /// The panel must report `DockPosition::Bottom`.
    #[gpui::test]
    fn panel_position_is_bottom(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            assert_eq!(
                SearchPanel::new().position(cx),
                DockPosition::Bottom,
                "SearchPanel must dock at Bottom",
            );
        });
    }
}
