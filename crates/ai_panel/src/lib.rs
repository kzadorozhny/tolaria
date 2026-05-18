#![forbid(unsafe_code)]
//! AI conversation panel for Tolaria (ADR-0115 Phase 2d).
//!
//! Renders the active conversation thread in the Right dock.  Alternates
//! visibility with the Inspector panel — `starts_open = false` so the Inspector
//! takes the Right dock by default.
//!
//! # Usage
//!
//! ```rust,ignore
//! // In mock / dev mode — global must be installed first:
//! cx.set_global(MockAi::seeded());
//! let panel = cx.new(|_| AiPanel::from_mock(cx));
//! ```

use gpui::{
    div, px, AnyElement, App, Context, IntoElement, ParentElement, Pixels, Render, SharedString,
    Styled, Window,
};
use gpui_component::{scroll::ScrollableElement as _, ActiveTheme};
use mock_fixtures::{MessageRole, MockAi, MockThread};
use workspace::DockPosition;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// AI conversation panel rendered in the Right dock.
///
/// Constructed via [`AiPanel::new`] for an empty panel or
/// [`AiPanel::from_mock`] to populate from the installed [`MockAi`] global.
///
/// # Panics
///
/// [`AiPanel::from_mock`] panics if the [`MockAi`] global has not been
/// installed on `cx` prior to the call.
pub struct AiPanel {
    thread: Option<MockThread>,
    /// Placeholder input buffer.  Phase 3 should replace with `String` (or an
    /// editor entity) once the field becomes editable — `SharedString` is an
    /// immutable `Arc<str>` and requires a full re-allocation per keystroke.
    input: SharedString,
    position: DockPosition,
}

impl AiPanel {
    /// An empty AI panel with no thread loaded.
    pub fn new() -> Self {
        Self {
            thread: None,
            input: SharedString::default(),
            position: DockPosition::Right,
        }
    }

    /// Build from the [`MockAi`] global if it is installed; otherwise return
    /// an empty panel.  Used by `TolariaWorkspace` so the panel populates
    /// under `TOLARIA_MOCK=1` and degrades gracefully in normal launches
    /// before Phase 3 services land.
    pub fn from_or_empty(cx: &mut App) -> Self {
        if cx.try_global::<MockAi>().is_some() {
            Self::from_mock(cx)
        } else {
            Self::new()
        }
    }

    /// Build an AI panel populated from the [`MockAi`] global installed on
    /// `cx`.
    ///
    /// Takes the first available thread.  Returns an empty panel (thread =
    /// `None`) if the fixture service has no threads.
    ///
    /// Both [`MockAi::threads`] and [`MockAi::thread`] return
    /// `Task::ready(…)`, so `block_on` returns immediately without blocking
    /// the foreground thread.  Phase 3 will replace this constructor with an
    /// async-safe service injection path.
    ///
    /// # Panics
    ///
    /// Panics if the [`MockAi`] global is not installed on `cx`.
    pub fn from_mock(cx: &mut App) -> Self {
        let ids_task = cx.global::<MockAi>().threads();
        let ids = cx.foreground_executor().block_on(ids_task);

        let thread = ids.into_iter().next().and_then(|id| {
            let thread_task = cx.global::<MockAi>().thread(id);
            cx.foreground_executor().block_on(thread_task)
        });

        Self {
            thread,
            input: SharedString::default(),
            position: DockPosition::Right,
        }
    }
}

impl Default for AiPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Panel impl
// ---------------------------------------------------------------------------

impl workspace::Panel for AiPanel {
    fn persistent_name(&self) -> &str {
        "AiPanel"
    }

    fn panel_key(&self) -> &str {
        "ai"
    }

    fn position(&self, _cx: &App) -> DockPosition {
        self.position
    }

    fn set_position(&mut self, position: DockPosition, _cx: &mut Context<Self>) {
        self.position = position;
    }

    fn default_size(&self, _cx: &App) -> Pixels {
        px(320.0)
    }

    fn icon(&self) -> Option<&str> {
        Some("sparkles")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        // Phase 2e will add a dedicated ToggleAiPanel action; for now we
        // reuse ToggleInspector as a placeholder so the dock machinery has a
        // valid action without forward-declaring an unused symbol.
        Box::new(actions::ToggleInspector)
    }

    fn starts_open(&self, _cx: &App) -> bool {
        // Inspector takes the Right dock by default; AI panel is hidden until
        // explicitly toggled.
        false
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for AiPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.border;
        let muted_bg = theme.muted;

        let message_list: AnyElement = match &self.thread {
            Some(thread) => {
                let rows: Vec<AnyElement> = thread
                    .messages
                    .iter()
                    .map(|msg| {
                        let role_label = match msg.role {
                            MessageRole::User => "User",
                            MessageRole::Assistant => "Assistant",
                            MessageRole::Tool => "Tool",
                        };
                        let text = SharedString::from(format!("[{role_label}] {}", msg.content));
                        let row = div().px(px(8.0)).py(px(4.0)).child(text);
                        if msg.role == MessageRole::Tool {
                            row.bg(muted_bg).into_any_element()
                        } else {
                            row.into_any_element()
                        }
                    })
                    .collect();

                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .children(rows)
                    .into_any_element()
            }
            None => div()
                .flex_1()
                .px(px(8.0))
                .py(px(8.0))
                .child(SharedString::from("No conversation loaded."))
                .into_any_element(),
        };

        let input_row = div()
            .h(px(36.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .border_t_1()
            .border_color(border_color)
            .child(self.input.clone());

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(message_list)
            .child(input_row)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use mock_fixtures::MockAi;
    use workspace::Panel as _;

    /// Install the `gpui_component::Theme` global required by any view that
    /// reads it during render.
    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// An empty AI panel must render without panicking.
    #[gpui::test]
    fn empty_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| AiPanel::new());
        cx.run_until_parked();
    }

    /// `from_mock` with a seeded [`MockAi`] global must load a thread.
    #[gpui::test]
    fn from_mock_loads_thread(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockAi::seeded());
            let panel = AiPanel::from_mock(cx);
            assert!(
                panel.thread.is_some(),
                "from_mock must load a thread when MockAi is seeded"
            );
        });
    }

    /// The seeded fixture thread must contain exactly 4 messages.
    #[gpui::test]
    fn thread_has_4_turns(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockAi::seeded());
            let panel = AiPanel::from_mock(cx);
            let count = panel.thread.as_ref().map(|t| t.messages.len()).unwrap_or(0);
            assert_eq!(count, 4, "seeded thread must have 4 messages, got {count}");
        });
    }

    /// The panel position must be Right.
    #[gpui::test]
    fn panel_position_is_right(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let panel = AiPanel::new();
            assert_eq!(
                panel.position(cx),
                DockPosition::Right,
                "AiPanel must occupy the Right dock"
            );
        });
    }
}
