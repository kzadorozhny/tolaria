#![forbid(unsafe_code)]
//! Inspector panel for the Tolaria right dock (ADR-0115 Phase 2d).
//!
//! Shows contextual metadata for the active note in seven collapsible
//! accordion sections: Properties, Outline, Backlinks, Instances,
//! ReferencedBy, Relationships, and GitHistory.
//!
//! # Usage
//!
//! ```rust,ignore
//! cx.set_global(MockVault::seeded());
//! cx.set_global(MockGit::seeded());
//! let panel = cx.new(|_window, cx| InspectorPanel::from_mock(cx));
//! ```

use std::collections::HashSet;

use gpui::{
    div, px, AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, Pixels,
    Render, SharedString, StatefulInteractiveElement, Styled, Window,
};
use gpui_component::{ActiveTheme, StyledExt as _};
use mock_fixtures::{MockCommit, MockGit, MockNote, MockVault, NoteId};
use workspace::panel::{DockPosition, Panel};

// ---------------------------------------------------------------------------
// InspectorSection
// ---------------------------------------------------------------------------

/// One collapsible section of the inspector accordion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorSection {
    Properties,
    Outline,
    Backlinks,
    Instances,
    ReferencedBy,
    Relationships,
    GitHistory,
}

impl InspectorSection {
    /// All sections in stable display order.
    pub const ALL: &'static [Self] = &[
        Self::Properties,
        Self::Outline,
        Self::Backlinks,
        Self::Instances,
        Self::ReferencedBy,
        Self::Relationships,
        Self::GitHistory,
    ];

    /// Human-readable label shown in the section header row.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Properties => "Properties",
            Self::Outline => "Outline",
            Self::Backlinks => "Backlinks",
            Self::Instances => "Instances",
            Self::ReferencedBy => "Referenced By",
            Self::Relationships => "Relationships",
            Self::GitHistory => "Git History",
        }
    }
}

// ---------------------------------------------------------------------------
// InspectorPanel
// ---------------------------------------------------------------------------

/// Right-dock panel showing note metadata in seven collapsible accordion sections.
///
/// Construct via [`InspectorPanel::new`] for an empty state, or
/// [`InspectorPanel::from_mock`] to pre-populate from installed mock globals.
pub struct InspectorPanel {
    expanded: HashSet<InspectorSection>,
    note_id: Option<NoteId>,
    position: DockPosition,
    /// Cached active note (Phase 3 wires a live subscription).
    note: Option<MockNote>,
    /// Cached git history — up to 5 commits shown (Phase 3 wires live data).
    git_history: Vec<MockCommit>,
    /// Count of vault notes sharing the same `type` property value.
    same_kind_count: usize,
}

/// Extract the `"type"` property value from a note as a `&str`, if present.
fn note_type(note: &MockNote) -> Option<&str> {
    note.properties.get("type")?.as_str()
}

impl InspectorPanel {
    /// Create an empty panel: no note selected, all sections collapsed.
    pub fn new() -> Self {
        Self {
            expanded: HashSet::new(),
            note_id: None,
            position: DockPosition::Right,
            note: None,
            git_history: Vec::new(),
            same_kind_count: 0,
        }
    }

    /// Build from [`MockVault`] and [`MockGit`] globals: selects the first note
    /// and pre-caches its data and git history.
    ///
    /// # Panics
    ///
    /// Panics if either the [`MockVault`] or [`MockGit`] global is not installed
    /// on `cx`, or if either service returns a non-ready task (Phase 3 will
    /// replace this with async service injection).
    pub fn from_mock(cx: &mut App) -> Self {
        let ids_task = cx.global::<MockVault>().notes();
        let ids = cx.foreground_executor().block_on(ids_task);

        let note_id = ids.first().copied();

        let note: Option<MockNote> = note_id.and_then(|id| {
            let task = cx.global::<MockVault>().note(id);
            cx.foreground_executor().block_on(task)
        });

        // Count notes that share the same `type` property value.
        let same_kind_count = {
            let target_type = note.as_ref().and_then(note_type).map(str::to_owned);
            match target_type.as_deref() {
                None | Some("") => 0,
                Some(target) => ids
                    .iter()
                    .filter_map(|&id| {
                        let task = cx.global::<MockVault>().note(id);
                        cx.foreground_executor().block_on(task)
                    })
                    .filter(|other| note_type(other) == Some(target))
                    .count(),
            }
        };

        let history_task = cx.global::<MockGit>().history();
        let git_history = cx.foreground_executor().block_on(history_task);

        Self {
            expanded: HashSet::new(),
            note_id,
            position: DockPosition::Right,
            note,
            git_history,
            same_kind_count,
        }
    }

    /// Build from mock globals if both [`MockVault`] and [`MockGit`] are
    /// installed; otherwise return [`InspectorPanel::new`].
    pub fn from_or_empty(cx: &mut App) -> Self {
        if cx.try_global::<MockVault>().is_some() && cx.try_global::<MockGit>().is_some() {
            Self::from_mock(cx)
        } else {
            Self::new()
        }
    }

    /// The ID of the currently active note, if any.
    pub fn note_id(&self) -> Option<NoteId> {
        self.note_id
    }

    /// Whether `section` is currently expanded.
    pub fn is_expanded(&self, section: InspectorSection) -> bool {
        self.expanded.contains(&section)
    }

    /// Toggle `section` between expanded and collapsed, then notify the view.
    pub fn toggle(&mut self, section: InspectorSection, cx: &mut Context<Self>) {
        // `HashSet::insert` returns false when the value was already present,
        // meaning the section was expanded — remove it to collapse.
        if !self.expanded.insert(section) {
            self.expanded.remove(&section);
        }
        cx.notify();
    }

    // -----------------------------------------------------------------------
    // Section body renderers
    // -----------------------------------------------------------------------

    fn render_properties_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(note) = &self.note else {
            return div()
                .text_sm()
                .text_color(muted)
                .child("No note selected.")
                .into_any_element();
        };

        let mut pairs: Vec<_> = note.properties.iter().collect();
        pairs.sort_by_key(|(k, _)| k.as_str());

        if pairs.is_empty() {
            return div()
                .text_sm()
                .text_color(muted)
                .child("No properties.")
                .into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(pairs.into_iter().map(|(key, value)| {
                let value_str: SharedString = value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string())
                    .into();
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .text_sm()
                    .child(
                        div()
                            .text_color(muted)
                            .child(SharedString::from(format!("{key}:"))),
                    )
                    .child(value_str)
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_outline_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(note) = &self.note else {
            return div()
                .text_sm()
                .text_color(muted)
                .child("No note selected.")
                .into_any_element();
        };

        let mut headings = note
            .content
            .lines()
            .filter(|line| {
                line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ")
            })
            .peekable();

        if headings.peek().is_none() {
            div()
                .text_sm()
                .text_color(muted)
                .child("No headings.")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(headings.map(|line| {
                    let label: SharedString =
                        line.trim_start_matches('#').trim().to_string().into();
                    div().text_sm().child(label).into_any_element()
                }))
                .into_any_element()
        }
    }

    fn render_backlinks_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().text_color(muted).child("Note B"))
            .child(div().text_sm().text_color(muted).child("Note C"))
            .into_any_element()
    }

    fn render_instances_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let count = self.same_kind_count;
        let label: SharedString = format!(
            "{count} note{} of this type.",
            if count == 1 { "" } else { "s" }
        )
        .into();
        div()
            .text_sm()
            .text_color(muted)
            .child(label)
            .into_any_element()
    }

    fn render_referenced_by_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().text_color(muted).child("Note A"))
            .into_any_element()
    }

    fn render_relationships_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        div()
            .text_sm()
            .text_color(muted)
            .child("Relationships (Phase 3 wires the graph)")
            .into_any_element()
    }

    fn render_git_history_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.git_history.is_empty() {
            return div()
                .text_sm()
                .text_color(muted)
                .child("No commits.")
                .into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(self.git_history.iter().take(5).map(|commit| {
                let meta: SharedString = format!("{} · {}", commit.sha, commit.author).into();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .child(SharedString::from(commit.message.clone())),
                    )
                    .child(div().text_sm().text_color(muted).child(meta))
                    .into_any_element()
            }))
            .into_any_element()
    }
}

impl Default for InspectorPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Panel trait
// ---------------------------------------------------------------------------

impl Panel for InspectorPanel {
    fn persistent_name(&self) -> &str {
        "InspectorPanel"
    }

    fn panel_key(&self) -> &str {
        "inspector"
    }

    fn position(&self, _cx: &App) -> DockPosition {
        self.position
    }

    fn set_position(&mut self, position: DockPosition, cx: &mut Context<Self>) {
        self.position = position;
        cx.notify();
    }

    fn default_size(&self, _cx: &App) -> Pixels {
        px(320.0)
    }

    fn icon(&self) -> Option<&str> {
        Some("info")
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        Box::new(actions::ToggleInspector)
    }

    fn starts_open(&self, _cx: &App) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for InspectorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border_color = cx.theme().border;
        let muted = cx.theme().muted_foreground;

        // Snapshot expanded state so we can borrow `self` freely inside the loop.
        let section_states: Vec<(InspectorSection, bool)> = InspectorSection::ALL
            .iter()
            .map(|&s| (s, self.is_expanded(s)))
            .collect();

        let mut children: Vec<AnyElement> = Vec::with_capacity(section_states.len() * 2);

        for (ix, (section, is_expanded)) in section_states.into_iter().enumerate() {
            let chevron: &'static str = if is_expanded { "▾" } else { "▸" };

            // Header row — click to toggle.
            let header = div()
                .id(("inspector-header", ix))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .px(px(12.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(border_color)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.toggle(section, cx);
                }))
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(SharedString::from(section.label())),
                )
                .child(div().text_sm().text_color(muted).child(chevron));

            children.push(header.into_any_element());

            if is_expanded {
                let body_content = match section {
                    InspectorSection::Properties => self.render_properties_body(cx),
                    InspectorSection::Outline => self.render_outline_body(cx),
                    InspectorSection::Backlinks => self.render_backlinks_body(cx),
                    InspectorSection::Instances => self.render_instances_body(cx),
                    InspectorSection::ReferencedBy => self.render_referenced_by_body(cx),
                    InspectorSection::Relationships => self.render_relationships_body(cx),
                    InspectorSection::GitHistory => self.render_git_history_body(cx),
                };
                let body = div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(border_color)
                    .child(body_content)
                    .into_any_element();
                children.push(body);
            }
        }

        div()
            .flex()
            .flex_col()
            .h_full()
            .overflow_hidden()
            .children(children)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use mock_fixtures::{MockGit, MockVault, NoteId};

    use super::{InspectorPanel, InspectorSection};
    use workspace::panel::{DockPosition, Panel};

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// `InspectorSection::ALL` must contain exactly 7 sections.
    #[gpui::test]
    fn all_returns_7_sections(_cx: &mut TestAppContext) {
        assert_eq!(InspectorSection::ALL.len(), 7);
    }

    /// A freshly-constructed panel must report `DockPosition::Right`.
    #[gpui::test]
    fn panel_position_is_right(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let panel = InspectorPanel::new();
            assert_eq!(panel.position(cx), DockPosition::Right);
        });
    }

    /// Toggle must move a section from collapsed → expanded → collapsed.
    #[gpui::test]
    fn toggle_round_trips(cx: &mut TestAppContext) {
        install_theme(cx);
        let window = cx.add_window(|_window, _cx| InspectorPanel::new());

        // Initially collapsed.
        window
            .update(cx, |panel, _window, _cx| {
                assert!(!panel.is_expanded(InspectorSection::Outline));
            })
            .unwrap();

        // Expand.
        window
            .update(cx, |panel, _window, cx| {
                panel.toggle(InspectorSection::Outline, cx);
            })
            .unwrap();
        window
            .update(cx, |panel, _window, _cx| {
                assert!(panel.is_expanded(InspectorSection::Outline));
            })
            .unwrap();

        // Collapse again.
        window
            .update(cx, |panel, _window, cx| {
                panel.toggle(InspectorSection::Outline, cx);
            })
            .unwrap();
        window
            .update(cx, |panel, _window, _cx| {
                assert!(!panel.is_expanded(InspectorSection::Outline));
            })
            .unwrap();
    }

    /// `from_mock` must select `NoteId::from_raw(1)` — the first note in the seeded vault.
    #[gpui::test]
    fn from_mock_picks_first_note(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            cx.set_global(MockGit::seeded());
            let panel = InspectorPanel::from_mock(cx);
            assert_eq!(
                panel.note_id(),
                Some(NoteId::from_raw(1)),
                "first seeded note must be NoteId::from_raw(1)"
            );
        });
    }

    /// With all sections expanded the panel must render without panicking.
    #[gpui::test]
    fn all_sections_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            cx.set_global(MockGit::seeded());
        });

        let window = cx.add_window(|_window, cx| InspectorPanel::from_mock(cx));

        // Expand every section.
        window
            .update(cx, |panel, _window, cx| {
                for &section in InspectorSection::ALL {
                    panel.toggle(section, cx);
                }
            })
            .unwrap();

        // Drive the render pass — must not panic.
        cx.run_until_parked();
    }
}
