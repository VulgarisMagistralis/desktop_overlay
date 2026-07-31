use crate::json_to_readers::call_by_name;
use crate::monitor::MonitorState;
use crate::widget::load_widgets;
use gtk::gdk::Display;
use gtk::glib::ExitCode;
use gtk::prelude::*;
use gtk::{Application, CssProvider};
use std::cell::RefCell;
use std::rc::Rc;
mod app;
mod json_to_readers;
mod monitor;
mod services;
mod snowflake;
mod ui;
mod widget;

// GDK_BACKEND=wayland cargo run --bin desktop_overlay
fn main() -> ExitCode {
    let widgets = load_widgets();
    let state = MonitorState::new();
    for widget in &widgets {
        let value = call_by_name(&state, &widget.method);
        println!("{} → {}", widget.id, value);
    }

    let application = Application::builder()
        .application_id("com.cenkt.desktop.overlay")
        .build();

    application.connect_activate(|app| {
        let provider = CssProvider::new();
        provider.load_from_string(include_str!("style/bg.css"));
        gtk::style_context_add_provider_for_display(
            &Display::default().expect("Could not connect to a display."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );

        // Get monitor dimensions for snow overlay
        let (monitor_width, monitor_height) =
            snowflake::SnowWindow::get_monitor_dimensions(&Display::default().expect("No display"));
        let snow = Rc::new(RefCell::new(snowflake::SnowWindow::new(
            app,
            monitor_width,
            monitor_height,
        )));

        let app_rc = app::AppBuilder::new()
            .setup_window(app)
            .bind_switches_and_restore(app, snow.clone())
            .schedule_ticks();
        let app_for_save = app_rc.clone();
        let snow_for_save = snow.clone();
        app.connect_shutdown(move |_| {
            let a = app_for_save.borrow();
            let s = snow_for_save.borrow();
            app::save_layout(&a, &s);
        });
    });

    application.run()
}
