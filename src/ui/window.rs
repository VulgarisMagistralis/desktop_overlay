use gtk::cairo;
use gtk::gdk::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use gtk::{ApplicationWindow, Builder};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

/// Create and configure the overlay window from overlay.xml.
pub fn create_window(application: &gtk::Application, builder: &Builder) -> ApplicationWindow {
    // let builder = Builder::from_string(include_str!("overlay.xml"));
    let window: ApplicationWindow = builder.object("main_window").unwrap();

    // Set application and parent
    window.set_application(Some(application));
    let root: gtk::Box = builder.object("root").unwrap();
    window.set_child(Some(&root));

    // Layer shell setup
    window.init_layer_shell();
    window.set_layer(Layer::Bottom);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, 0);
    window.set_margin(Edge::Right, 0);
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    window.set_can_focus(true);
    window.set_show_menubar(false);
    window.set_resizable(false);

    // Consume entry widget to avoid builder warning
    // let _: gtk::Entry = builder.object("entry_url").unwrap();

    // Set initial input region on map (tick loop keeps it updated after content grows)
    window.connect_map(move |w| {
        glib::idle_add_local_once({
            let w = w.clone();
            move || {
                if let Some(surface) = w.surface() {
                    let full_region = cairo::Region::create_rectangle(&cairo::RectangleInt::new(
                        0,
                        0,
                        w.width() as i32,
                        w.height() as i32,
                    ));
                    surface.set_input_region(Some(&full_region));
                }
            }
        });
    });

    window.present();
    window
}
