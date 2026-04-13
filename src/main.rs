//! Binary entry point — wires the iced runtime to `app::App`.

use anan::app::App;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    iced::application(App::title, App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(iced::window::Settings {
            #[cfg(target_os = "macos")]
            platform_specific: iced::window::settings::PlatformSpecific {
                titlebar_transparent: true,
                fullsize_content_view: true,
                title_hidden: true,
            },
            ..Default::default()
        })
        .run_with(App::new)
}
