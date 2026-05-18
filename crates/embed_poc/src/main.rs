//! Phase 0 spike entry point for ADR-0115.
//!
//! Task #3 wires the placeholder body from task #2 into a two-pane layout:
//! draggable sidebar on the left, "editor goes here" placeholder on the
//! right. The actual WKWebView lands in task #4; frame sync against the
//! splitter lands in task #5. Sidebar drags and window resizes both emit
//! `frame_event ...` log lines on the `embed_poc::frame` target so task #5's
//! validation script can grep for them.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("embed_poc is macOS-only (ADR-0115 Phase 0 spike); skipping on this platform.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
mod layout;
#[cfg(target_os = "macos")]
mod menus;
#[cfg(target_os = "macos")]
mod webview;

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}

#[cfg(target_os = "macos")]
mod macos {
    use gpui::{
        px, size, App, AppContext, Bounds, KeyBinding, SharedString, TitlebarOptions, WindowBounds,
        WindowOptions,
    };
    use gpui_platform::application;

    use crate::layout::RootView;
    use crate::menus::{app_menus, Quit, Save};

    const WINDOW_TITLE: &str = "Tolaria Phase 0 Spike";
    const WINDOW_WIDTH: f32 = 1200.0;
    const WINDOW_HEIGHT: f32 = 800.0;

    pub fn run() {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
        log::info!("embed_poc starting (ADR-0115 Phase 0)");

        application().run(|cx: &mut App| {
            // gpui-component reads a Theme global from the App; without this
            // call, primitives like `h_resizable` panic on first render.
            gpui_component::init(cx);

            // ADR-0115 §6: install the native menu BEFORE the window opens so
            // AppKit picks the accelerators up immediately, then register the
            // global action handlers + keybindings for Save/Quit. Edit-menu
            // entries are `MenuItem::os_action(...)` so AppKit's standard
            // selector chain (cut:/copy:/paste:/undo:/redo:/selectAll:) keeps
            // routing into the focused WKWebView untouched.
            cx.on_action(|_: &Save, _cx| {
                log::info!(target: "embed_poc::menu", "cmd_s_fired");
            });
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([
                KeyBinding::new("cmd-s", Save, None),
                KeyBinding::new("cmd-q", Quit, None),
            ]);
            cx.set_menus(app_menus());

            let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

            let opts = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from(WINDOW_TITLE)),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let opened = cx.open_window(opts, |window, cx| {
                let webview = crate::webview::spawn_test_webview(window, cx);
                cx.new(|cx| RootView::new(webview, window, cx))
            });

            if let Err(err) = opened {
                log::error!("failed to open spike window: {err:?}");
            }

            cx.activate(true);
        });
    }
}
