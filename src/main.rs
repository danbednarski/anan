//! Binary entry point — wires the iced runtime to `app::App`.

use gramps_desktop::app::App;

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
        .run_with(App::new)
}
