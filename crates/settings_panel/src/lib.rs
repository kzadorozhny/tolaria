#![forbid(unsafe_code)]
//! Settings panel modal for Tolaria (ADR-0115 Phase 2d).
//!
//! Implements `workspace::ModalView` so it can be mounted into `TolariaWorkspace`
//! via `ModalLayer::toggle_modal`.
//!
//! Layout:
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │ General  │  General settings (Phase 3 wires …)   │
//! │ Editor   │                                        │
//! │ Git      │                                        │
//! │ AI       │                                        │
//! │ Vault    │                                        │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! The active tab is highlighted with the theme's `tab_active` background.
//! Each right-pane content area is a placeholder pending Phase 3 controls.
//!
//! # Usage
//!
//! ```rust,ignore
//! // Toggle from workspace (mock globals must be installed first):
//! cx.set_global(MockSettings::seeded());
//! workspace.toggle_modal::<SettingsPanel>(window, cx, |_w, cx| SettingsPanel::new(cx));
//! ```

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, App, Context, Div, IntoElement, ParentElement, Render, SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    ActiveTheme,
};
use mock_fixtures::MockSettings;

// ---------------------------------------------------------------------------
// SettingsTab
// ---------------------------------------------------------------------------

/// A tab page shown in the [`SettingsPanel`] sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Editor,
    Git,
    Ai,
    Vault,
}

impl SettingsTab {
    /// All tabs in display order.
    pub const ALL: &'static [Self] = &[
        Self::General,
        Self::Editor,
        Self::Git,
        Self::Ai,
        Self::Vault,
    ];

    /// Human-readable label for this tab.
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Editor => "Editor",
            Self::Git => "Git",
            Self::Ai => "AI",
            Self::Vault => "Vault",
        }
    }

    /// Stable GPUI element ID for this tab's sidebar button.
    ///
    /// Kept on the enum so that adding a new variant forces an update here,
    /// preventing the render function from drifting out of sync.
    pub const fn element_id(self) -> &'static str {
        match self {
            Self::General => "settings-tab-general",
            Self::Editor => "settings-tab-editor",
            Self::Git => "settings-tab-git",
            Self::Ai => "settings-tab-ai",
            Self::Vault => "settings-tab-vault",
        }
    }
}

// ---------------------------------------------------------------------------
// SettingsPanel
// ---------------------------------------------------------------------------

/// Multi-tab settings dialog mounted into `TolariaWorkspace` via `ModalLayer`.
pub struct SettingsPanel {
    active: SettingsTab,
    settings: MockSettings,
}

impl SettingsPanel {
    /// Construct a new panel.  Reads [`MockSettings`] from the GPUI globals if
    /// installed (e.g. under `TOLARIA_MOCK=1`); otherwise falls back to
    /// [`MockSettings::default`].
    pub fn new(cx: &mut App) -> Self {
        let settings = cx.try_global::<MockSettings>().cloned().unwrap_or_default();
        Self {
            active: SettingsTab::General,
            settings,
        }
    }

    /// The currently selected tab.
    pub fn active_tab(&self) -> SettingsTab {
        self.active
    }

    /// Switch to `tab` and notify any observers.
    pub fn set_active(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.active = tab;
        cx.notify();
    }

    /// The settings snapshot held by this panel.
    pub fn settings(&self) -> &MockSettings {
        &self.settings
    }
}

impl workspace::ModalView for SettingsPanel {} // marker impl — intentionally empty

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active;
        let tab_active_bg = cx.theme().tab_active;
        let tab_active_fg = cx.theme().tab_active_foreground;

        let content_label = SharedString::from(format!(
            "{} settings (Phase 3 wires controls)",
            active.label(),
        ));

        div()
            .flex()
            .flex_row()
            .size_full()
            .child(div().flex().flex_col().w(px(160.0)).p(px(8.0)).children(
                SettingsTab::ALL.iter().map(|&tab| {
                    let is_active = tab == active;
                    let btn = Button::new(tab.element_id())
                        .label(SharedString::from(tab.label()))
                        .ghost();

                    // Wrap in a container so we can apply the active-tab
                    // background without reaching into the Button internals.
                    div()
                        .w_full()
                        .px(px(4.0))
                        .py(px(2.0))
                        .when(is_active, |d: Div| {
                            d.bg(tab_active_bg).text_color(tab_active_fg)
                        })
                        .child(btn)
                        .into_any_element()
                }),
            ))
            .child(div().flex_1().p(px(16.0)).child(content_label))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn install_theme(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
    }

    /// When no [`MockSettings`] global is installed, the panel falls back to
    /// [`MockSettings::default`].
    #[gpui::test]
    fn defaults_when_no_mock_settings(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let panel = SettingsPanel::new(cx);
            assert_eq!(
                *panel.settings(),
                MockSettings::default(),
                "settings must default when no global is installed"
            );
        });
    }

    /// The active tab must be [`SettingsTab::General`] right after construction.
    #[gpui::test]
    fn active_tab_starts_general(cx: &mut TestAppContext) {
        install_theme(cx);
        cx.update(|cx| {
            let panel = SettingsPanel::new(cx);
            assert_eq!(
                panel.active_tab(),
                SettingsTab::General,
                "initial active tab must be General"
            );
        });
    }

    /// `set_active` must round-trip through every tab variant.
    #[gpui::test]
    fn set_active_round_trips(cx: &mut TestAppContext) {
        install_theme(cx);
        let panel_view = cx.add_window(|_window, cx| SettingsPanel::new(cx));
        for &tab in SettingsTab::ALL {
            panel_view
                .update(cx, |panel, _window, cx| {
                    panel.set_active(tab, cx);
                    assert_eq!(
                        panel.active_tab(),
                        tab,
                        "active_tab must reflect the tab passed to set_active"
                    );
                })
                .expect("window must be alive");
        }
    }

    /// `SettingsTab::ALL` must contain exactly five tabs.
    #[test]
    fn all_returns_5_tabs() {
        assert_eq!(
            SettingsTab::ALL.len(),
            5,
            "SettingsTab::ALL must contain exactly 5 tabs"
        );
    }
}
