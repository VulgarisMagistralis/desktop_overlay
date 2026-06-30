use gtk::cairo;
use gtk::gdk::{Display, prelude::*};
use gtk::glib::{self, timeout_add_seconds_local, ControlFlow, Propagation};
use gtk::prelude::*;


use std::cell::RefCell;
use std::rc::Rc;

use crate::snowflake;
use crate::tick;
use crate::ui::Labels;
use crate::{monitor, ui};

/// Central application state.
pub struct App {
    pub window: gtk::ApplicationWindow,
    pub monitor_state: monitor::MonitorState,
    pub labels: Labels,
    pub backends: Vec<Box<dyn monitor::GpuBackend>>,
    pub snow_win: Option<Rc<RefCell<snowflake::SnowWindow>>>,
    pub tick_count: u64,
}

/// Fluent builder for constructing an `App`.
pub struct AppBuilder {
    app: Rc<RefCell<App>>,
}

impl AppBuilder {
    /// Start building the overlay app.
    pub fn new() -> Self {
        Self {
            app: Rc::new(RefCell::new(App {
                window: gtk::ApplicationWindow::builder().build(),
                monitor_state: monitor::MonitorState::new(),
                labels: Labels::default_empty(),
                backends: Vec::new(),
                snow_win: None,
                tick_count: 0,
            })),
        }
    }

    /// Load XML, create window, extract labels and button — all from ONE builder.
    pub fn setup_window(self, application: &gtk::Application) -> Self {
        let builder = gtk::Builder::from_string(include_str!("ui/overlay.xml"));

        // Extract window
        let window = ui::window::create_window(application, &builder);

        // Extract labels from the same builder instance
        let labels = ui::Labels::from_builder(&builder);

        // Extract snow toggle switch
        let snow_switch: gtk::Switch = builder.object("snow_switch").unwrap();

        let display = Display::default().expect("Could not connect to a display.");
        let (snow_width, snow_height) = snowflake::SnowWindow::get_monitor_dimensions(&display);

        let snow_win = Rc::new(RefCell::new(snowflake::SnowWindow::new(
            application,
            snow_width,
            snow_height,
        )));

        {
            let snow_win_inner = snow_win.clone();
            // React to the switch's own state changes — no GestureClick, no race.
            snow_switch.connect_state_set(move |_switch, state| {
                snow_win_inner.borrow_mut().set_snow_state(state);
                Propagation::Proceed
            });
        }

        // Populate all fields at once
        let mut inner = self.app.borrow_mut();
        inner.window = window;
        inner.labels = labels;
        inner.backends = monitor::default_backends();
        inner.snow_win = Some(snow_win);
        drop(inner);

        drop(builder); // Drop builder to release reference counting on widgets
        self
    }

    /// Start the periodic update loop and finalize.
    pub fn schedule_ticks(self) -> Rc<RefCell<App>> {
        let app = self.app.clone();
        // Clone window before closing over it — ApplicationWindow is RefCounted
        let window = self.app.borrow().window.clone();
        timeout_add_seconds_local(2, move || {
            let output = tick::update(&mut app.borrow_mut());
            tick::apply_labels(&app.borrow().labels, &output);

            // Update input region after content changes so the overlay's clickable
            // area matches its new size (content grows when labels populate).
            glib::idle_add_local_once({
                let window = window.clone();
                move || {
                    if let Some(surface) = window.surface() {
                        let region = cairo::Region::create_rectangle(
                            &cairo::RectangleInt::new(
                                0,
                                0,
                                window.width(),
                                window.height(),
                            ),
                        );
                        surface.set_input_region(Some(&region));
                    }
                }
            });

            ControlFlow::Continue
        });
        self.app
    }
}
