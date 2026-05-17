//! Phase 0 spike entry point for ADR-0115.
//!
//! Task #2 keeps this binary deliberately minimal: open a single GPUI window
//! with a placeholder body. Sidebar chrome (#3), WKWebView embedding (#4),
//! frame sync (#5), native menus (#6), and focus/IME instrumentation (#7)
//! land in their own commits.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("embed_poc is macOS-only (ADR-0115 Phase 0 spike); skipping on this platform.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}

#[cfg(target_os = "macos")]
mod macos {
    use gpui::{
        App, Bounds, Context, Render, SharedString, TitlebarOptions, Window, WindowBounds,
        WindowOptions, div, prelude::*, px, rgb, size,
    };
    use gpui_platform::application;

    const WINDOW_TITLE: &str = "Tolaria Phase 0 Spike";
    const WINDOW_WIDTH: f32 = 1200.0;
    const WINDOW_HEIGHT: f32 = 800.0;

    struct SpikeRoot {
        title: SharedString,
    }

    impl Render for SpikeRoot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .bg(rgb(0x1e1f24))
                .text_color(rgb(0xe6e6e6))
                .text_xl()
                .child(self.title.clone())
                .child(
                    div()
                        .text_color(rgb(0x9aa0a6))
                        .text_sm()
                        .child("ADR-0115 · Phase 0 · embed_poc"),
                )
        }
    }

    pub fn run() {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
        log::info!("embed_poc starting (ADR-0115 Phase 0)");

        application().run(|cx: &mut App| {
            let bounds =
                Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

            let opts = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from(WINDOW_TITLE)),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let opened = cx.open_window(opts, |_, cx| {
                cx.new(|_| SpikeRoot {
                    title: SharedString::from(WINDOW_TITLE),
                })
            });

            if let Err(err) = opened {
                log::error!("failed to open spike window: {err:?}");
            }

            cx.activate(true);
        });
    }
}
