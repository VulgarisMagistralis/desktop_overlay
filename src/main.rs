use gtk::gdk::Display;
use gtk::glib::ExitCode;
use gtk::prelude::*;
use gtk::{Application, CssProvider};

mod app;
mod monitor;
mod snowflake;
mod tick;
mod ui;

// GDK_BACKEND=wayland cargo run --bin desktop_overlay
fn main() -> ExitCode {
    let application = Application::builder()
        .application_id("com.cenkt.desktop.overlay")
        .build();

    application.connect_activate(|app| {
        // Load CSS provider
        let provider = CssProvider::new();
        provider.load_from_string(include_str!("style/bg.css"));
        gtk::style_context_add_provider_for_display(
            &Display::default().expect("Could not connect to a display."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );

        // Build the overlay app
        app::AppBuilder::new()
            .setup_window(app)
            .schedule_ticks();
    });

    application.run()
}
