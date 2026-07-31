use crate::monitor;
use crate::snowflake::SnowWindow;
use crate::ui::{default_configs, window};
use crate::ui::{extract_sections, WidgetRegistry};
use gtk::glib::{timeout_add_local, ControlFlow};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

pub struct App {
    pub window: gtk::ApplicationWindow,
    pub registry: Rc<WidgetRegistry>,
    pub monitor_state: monitor::MonitorState,
    pub detach_lock: bool,
    pub tick_id: Option<gtk::glib::source::SourceId>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct LayoutFile {
    snow: bool,
    detach_lock: bool,
    widgets: Vec<crate::widget::DataWidgetConfig>,
}

pub struct AppBuilder {
    app: Rc<RefCell<App>>,
    snow_switch: Option<gtk::Switch>,
    detach_lock_switch: Option<gtk::Switch>,
    registry: Option<Rc<WidgetRegistry>>,
}

impl AppBuilder {
    pub fn new() -> Self {
        let dummy_window = gtk::ApplicationWindow::builder().build();
        Self {
            app: Rc::new(RefCell::new(App {
                window: dummy_window,
                registry: Rc::new(WidgetRegistry::empty()),
                monitor_state: monitor::MonitorState::new(),
                detach_lock: false,
                tick_id: None,
            })),
            snow_switch: None,
            detach_lock_switch: None,
            registry: None,
        }
    }

    pub fn setup_window(self, application: &gtk::Application) -> Self {
        let builder = gtk::Builder::from_string(include_str!("ui/overlay.xml"));
        let window = window::create_window(application, &builder);
        let sections_map = extract_sections(&builder);

        // Collect section boxes without headers for widget creation
        let section_boxes: std::collections::HashMap<String, gtk::Box> = sections_map
            .iter()
            .map(|(k, (box_, _))| (k.clone(), box_.clone()))
            .collect();

        // Load saved configs or use defaults
        let configs = if let Ok(data) = fs::read_to_string("widget_layout.json") {
            if let Ok(saved) = serde_json::from_str::<Vec<crate::widget::DataWidgetConfig>>(&data) {
                saved
            } else {
                default_configs()
            }
        } else {
            default_configs()
        };

        let root: gtk::Box = builder.object("root").unwrap();
        let snow_switch: gtk::Switch = builder.object("snow_switch").unwrap();
        let detach_lock_switch: gtk::Switch = builder.object("detach_lock_switch").unwrap();
        drop(builder);

        // Separate headers for section-level detach
        let section_headers: std::collections::HashMap<String, gtk::Label> = sections_map
            .iter()
            .map(|(k, (_, hdr))| (k.clone(), hdr.clone()))
            .collect();

        // Create widget registry
        let registry = Rc::new(WidgetRegistry::new(configs, &section_boxes));
        WidgetRegistry::init_self_ref(&registry);
        registry.set_section_headers(section_headers);
        registry.set_context(root.clone(), application.clone());

        let mut app_state = self.app.borrow_mut();
        app_state.window = window;
        app_state.registry = registry.clone();
        drop(app_state);

        Self {
            snow_switch: Some(snow_switch),
            detach_lock_switch: Some(detach_lock_switch),
            registry: Some(registry),
            ..self
        }
    }

    pub fn bind_switches_and_restore(
        self,
        _application: &gtk::Application,
        snow: Rc<RefCell<SnowWindow>>,
    ) -> Self {
        if let Some(ref switch) = self.detach_lock_switch {
            let app_rc = self.app.clone();
            let reg = self.registry.clone();
            switch.connect_active_notify(move |sw| {
                eprintln!("[DEBUG] detach_lock switch toggled to {}", sw.is_active());
                app_rc.borrow_mut().detach_lock = sw.is_active();
                if let Some(r) = &reg {
                    r.set_lock(sw.is_active());
                }
            });
        }

        if let Some(ref switch) = self.snow_switch {
            let snow_clone = snow.clone();
            switch.connect_active_notify(move |sw| {
                eprintln!("[DEBUG] snow switch toggled to {}", sw.is_active());
                snow_clone.borrow_mut().set_snow_state(sw.is_active());
            });
        }

        // Set up widget detach gestures
        if let Some(ref registry) = self.registry {
            registry.setup_detach_gestures();
        }

        // Restore saved layout
        if let Some(ref registry) = self.registry {
            if let Ok(data) = fs::read_to_string("current_layout.json") {
                if let Ok(parsed) = serde_json::from_str::<LayoutFile>(&data) {
                    self.app.borrow_mut().detach_lock = parsed.detach_lock;
                    if let Some(ref sw) = self.detach_lock_switch {
                        sw.set_active(parsed.detach_lock);
                    }
                    if let Some(ref sw) = self.snow_switch {
                        sw.set_active(parsed.snow);
                    }
                    if parsed.snow {
                        snow.borrow_mut().show();
                    }
                    registry.restore_layout(&parsed.widgets);
                }
            }
        }

        self
    }

    pub fn schedule_ticks(self) -> Rc<RefCell<App>> {
        let app = self.app.clone();
        let id = timeout_add_local(std::time::Duration::from_secs(1), move || {
            let mut a = app.borrow_mut();
            a.monitor_state.refresh();
            a.registry.update_all(&a.monitor_state);
            ControlFlow::Continue
        });
        self.app.borrow_mut().tick_id = Some(id);
        self.app
    }
}

pub fn save_layout(app: &App, snow_window: &SnowWindow) {
    app.registry.pre_save_update_positions();
    let widgets = app.registry.save_layout();
    let layout = LayoutFile {
        snow: snow_window.is_on(),
        detach_lock: app.detach_lock,
        widgets,
    };
    if let Ok(json) = serde_json::to_string_pretty(&layout) {
        let _ = fs::write("current_layout.json", json);
    }
}
