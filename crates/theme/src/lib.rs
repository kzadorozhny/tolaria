//! Theme global for Tolaria (ADR-0115 Phase 1).
//!
//! Thin wrapper around `gpui_component`'s `Theme` global. `init` is
//! idempotent: calling it multiple times is safe because
//! `gpui_component::init` is itself idempotent.
//!
//! `reload_from_settings` is a stub in Phase 1 — real light/dark switching
//! wires into `settings_store`'s observer in Phase 2.

/// Install the `gpui_component::Theme` global into `cx`.
///
/// Must be called before any `gpui_component` primitive is rendered.
/// Mirrors the `gpui_component::init` call in `embed_poc::main`.
pub fn init(cx: &mut gpui::App) {
    gpui_component::init(cx);
}

/// Reload the active theme from the current `SettingsStore` global.
///
/// Phase 1 stub — `cx` is accepted now so the Phase 2 wiring
/// (`SettingsStore::get(cx).theme` → `gpui_component` theme-switch API)
/// is a non-breaking change. Currently logs the request and returns.
pub fn reload_from_settings(cx: &mut gpui::App) {
    let _ = cx; // reserved for Phase 2
    log::info!("theme reload requested (stub — real switching in Phase 2)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn init_installs_theme_global(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            init(cx);
            assert!(
                cx.try_global::<gpui_component::theme::Theme>().is_some(),
                "gpui_component::Theme global must be present after theme::init"
            );
        });
    }
}
