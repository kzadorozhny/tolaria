#![forbid(unsafe_code)]
//! Status-bar chrome view for Tolaria (ADR-0115 Phase 2b).
//!
//! Renders the thin strip at the bottom of the workspace window:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │ demo-vault-v2 │ main │ ~4 │                  Normal │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! Items are separated by thin vertical dividers.  The vault name renders as
//! a [`gpui_component::button::Button`] (clickable popover-trigger placeholder);
//! all other items are plain text labels.
//!
//! # Usage
//!
//! ```rust,ignore
//! // In mock / dev mode — globals must be installed first:
//! cx.set_global(MockVault::seeded());
//! cx.set_global(MockGit::seeded());
//! let bar = cx.new(|_| StatusBar::from_mock(cx));
//! ```

use gpui::{
    div, px, AnyElement, App, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    ActiveTheme,
};
use mock_fixtures::{FileStatus, MockGit};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Editor interaction mode shown in the right-hand status slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Search,
}

/// A single logically-distinct slot in the [`StatusBar`].
#[derive(Debug, Clone, PartialEq)]
pub enum StatusItem {
    /// Name of the open vault (clickable popover trigger).
    VaultName(SharedString),
    /// Current git branch name.
    GitBranch(SharedString),
    /// Count of uncommitted (modified + untracked) files.
    DirtyCount(usize),
    /// Current editor interaction mode.
    Mode(EditorMode),
}

// ---------------------------------------------------------------------------
// StatusBar view
// ---------------------------------------------------------------------------

/// Horizontal status strip rendered at the bottom of `TolariaWorkspace`.
///
/// Constructed via [`StatusBar::empty`] for a blank bar or
/// [`StatusBar::from_mock`] to populate items from the installed mock globals.
///
/// # Panics
///
/// [`StatusBar::from_mock`] panics if the [`MockGit`] global has not been
/// installed on `cx` prior to the call.
pub struct StatusBar {
    items: Vec<StatusItem>,
}

impl StatusBar {
    /// An empty status bar with no items.
    pub fn empty() -> Self {
        Self { items: Vec::new() }
    }

    /// Build a status bar populated from the [`MockVault`] and [`MockGit`]
    /// globals installed on `cx`.
    ///
    /// - **VaultName** — synthesised as `"demo-vault-v2"` (MockVault carries no
    ///   name field).
    /// - **GitBranch** — synthesised as `"main"` (MockGit carries no branch
    ///   field).
    /// - **DirtyCount** — modified-file count + untracked-file count resolved
    ///   synchronously from [`MockGit::status`] via the foreground executor.
    /// - **Mode** — defaults to [`EditorMode::Normal`].
    ///
    /// # Panics
    ///
    /// Panics if the [`MockGit`] global is not installed on `cx`, or if
    /// [`MockGit::status`] returns a non-ready task (i.e. `block_on` would
    /// block the foreground thread). Phase 3 will replace this constructor
    /// with an async-safe service injection path.
    pub fn from_mock(cx: &mut App) -> Self {
        // MockVault has no name field → synthesise the demo-vault identifier.
        let vault_name: SharedString = "demo-vault-v2".into();

        // MockGit has no branch field → synthesise the primary branch name.
        let branch: SharedString = "main".into();

        // status() returns Task::ready(…) — block_on returns immediately.
        let status_task = cx.global::<MockGit>().status();
        let git_status = cx.foreground_executor().block_on(status_task);
        let dirty_count =
            git_status.count(FileStatus::Modified) + git_status.count(FileStatus::Untracked);

        Self {
            items: vec![
                StatusItem::VaultName(vault_name),
                StatusItem::GitBranch(branch),
                StatusItem::DirtyCount(dirty_count),
                StatusItem::Mode(EditorMode::Normal),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let divider_color = cx.theme().border;

        // TODO(phase3): cache rendered children — items rarely change but this
        // Vec is re-allocated on every frame.
        let children: Vec<AnyElement> = self
            .items
            .iter()
            .enumerate()
            .flat_map(|(i, item)| {
                // Insert a thin vertical divider before every item except the first.
                let separator: Option<AnyElement> = if i > 0 {
                    Some(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .mx(px(4.0))
                            .bg(divider_color)
                            .into_any_element(),
                    )
                } else {
                    None
                };

                let element: AnyElement = match item {
                    StatusItem::VaultName(name) => Button::new("status-vault-name")
                        .label(name.clone())
                        .ghost()
                        .into_any_element(),
                    StatusItem::GitBranch(branch) => {
                        div().px(px(8.0)).child(branch.clone()).into_any_element()
                    }
                    StatusItem::DirtyCount(n) => div()
                        .px(px(8.0))
                        .child(SharedString::from(format!("~{n}")))
                        .into_any_element(),
                    StatusItem::Mode(mode) => {
                        let label: SharedString = match mode {
                            EditorMode::Normal => "Normal".into(),
                            EditorMode::Search => "Search".into(),
                        };
                        div().px(px(8.0)).child(label).into_any_element()
                    }
                };

                separator.into_iter().chain(std::iter::once(element))
            })
            .collect();

        div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(24.0))
            .px(px(8.0))
            .children(children)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use mock_fixtures::{MockGit, MockVault};

    /// Install the `gpui_component::Theme` global required by any view that
    /// reads it during render (mirrors `embed_poc/src/layout.rs:243`).
    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// An empty status bar must render without panicking.
    #[gpui::test]
    fn empty_status_bar_renders(cx: &mut TestAppContext) {
        install_theme(cx);
        let _window = cx.add_window(|_window, _cx| StatusBar::empty());
        cx.run_until_parked();
    }

    /// `from_mock` must pull the git branch and dirty count from the installed
    /// globals.  MockGit seeded: 3 modified + 1 untracked = 4 dirty files.
    #[gpui::test]
    fn from_mock_pulls_branch_and_dirty_count(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            cx.set_global(MockGit::seeded());
            let bar = StatusBar::from_mock(cx);
            assert!(
                bar.items.contains(&StatusItem::DirtyCount(4)),
                "expected DirtyCount(4), got items: {:?}",
                bar.items,
            );
            assert!(
                bar.items.contains(&StatusItem::GitBranch("main".into())),
                "expected GitBranch(\"main\"), got items: {:?}",
                bar.items,
            );
        });
    }

    /// The default mode must be `EditorMode::Normal`.
    #[gpui::test]
    fn editor_mode_normal_is_default(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            cx.set_global(MockVault::seeded());
            cx.set_global(MockGit::seeded());
            let bar = StatusBar::from_mock(cx);
            assert!(
                bar.items.contains(&StatusItem::Mode(EditorMode::Normal)),
                "expected Mode(Normal), got items: {:?}",
                bar.items,
            );
        });
    }
}
