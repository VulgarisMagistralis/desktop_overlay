use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Label};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use crate::json_to_readers::call_by_name;
use crate::monitor::MonitorState;

// Single config struct.  JSON keys renamed via serde where they differ from Rust naming.
// docked ↔ floating is derived from margin_from_x / margin_from_y == 0.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DataWidgetConfig {
    pub id: String,
    #[serde(rename = "label")]
    pub header_text: String,
    #[serde(rename = "section")]
    pub section_id: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub value_type: String,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub margin_from_x: i32,
    #[serde(default)]
    pub margin_from_y: i32,
}

impl DataWidgetConfig {
    pub fn is_docked(&self) -> bool {
        self.margin_from_x == 0 && self.margin_from_y == 0
    }

    pub fn is_floating(&self) -> bool {
        !self.is_docked()
    }
}

#[derive(Debug, Deserialize)]
pub struct WidgetFile {
    pub widgets: Vec<DataWidgetConfig>,
}

pub fn load_widgets() -> Vec<DataWidgetConfig> {
    let data = fs::read_to_string("src/widgets.json").expect("cannot read widgets.json");
    let file: WidgetFile = serde_json::from_str(&data).expect("invalid JSON");
    file.widgets
}

pub struct DataWidget {
    pub config: DataWidgetConfig,
    pub row_box: gtk::Box,
    pub header_label: Label,
    pub value_label: Label,
    pub float_win: Option<ApplicationWindow>,
    pub drag_state: Option<Rc<RefCell<(i32, i32)>>>, // (top_margin, right_margin)
}

impl DataWidget {
    pub fn new(config: DataWidgetConfig) -> Self {
        let state = MonitorState::new();
        let header = Label::builder()
            .label(&config.header_text)
            .halign(gtk::Align::Start)
            .build();
        header.add_css_class("row-label");
        let value = Label::builder()
            .label(call_by_name(&state, &config.method))
            .halign(gtk::Align::End)
            .hexpand(true)
            .build();
        value.add_css_class("row-label");
        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        box_.append(&header);
        box_.append(&value);
        Self {
            config,
            row_box: box_,
            header_label: header,
            value_label: value,
            float_win: None,
            drag_state: None,
        }
    }

    // Docked/floating is derived from config margins.
    pub fn is_docked(&self) -> bool {
        self.config.is_docked()
    }

    pub fn is_floating(&self) -> bool {
        !self.is_docked()
    }

    fn saved_margins(&self) -> (i32, i32) {
        let t = if self.config.margin_from_y == 0 {
            70
        } else {
            self.config.margin_from_y
        };
        let r = if self.config.margin_from_x == 0 {
            70
        } else {
            self.config.margin_from_x
        };
        (t, r)
    }

    pub fn detach(&mut self, section_box: &gtk::Box, application: &Application) {
        if !self.is_docked() {
            return;
        }
        section_box.remove(&self.row_box);

        let (initial_top_margin, initial_right_margin) = self.saved_margins();
        let fw = Self::make_float_window(application, initial_top_margin, initial_right_margin);
        fw.set_child(Some(&self.row_box));
        fw.present();

        let state = Rc::new(RefCell::new((initial_top_margin, initial_right_margin)));
        fw.add_controller(Self::make_drag_gesture(state.clone(), &fw));
        self.float_win = Some(fw);
        self.config.margin_from_x = initial_right_margin;
        self.config.margin_from_y = initial_top_margin;
        self.drag_state = Some(state);
    }

    pub fn make_float_window(
        application: &Application,
        top_margin: i32,
        right_margin: i32,
    ) -> ApplicationWindow {
        let win = gtk::ApplicationWindow::builder()
            .application(application)
            .build();
        win.init_layer_shell();
        win.set_layer(Layer::Bottom);
        win.set_anchor(Edge::Top, true);
        win.set_margin(Edge::Top, top_margin);
        win.set_anchor(Edge::Right, true);
        win.set_margin(Edge::Right, right_margin);
        // Explicitly disable left/bottom anchors so layer-shell sizes to content.
        win.set_anchor(Edge::Left, false);
        win.set_anchor(Edge::Bottom, false);
        win.set_exclusive_zone(0);
        // Give a minimum width so the window is not 0 × 0 when empty.
        win.set_size_request(256, -1);
        win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
        win.set_can_focus(true);
        win.set_decorated(false);
        // win.set_resizable(false);
        win.add_css_class("vbox");
        win
    }

    pub fn make_drag_gesture(
        state: Rc<RefCell<(i32, i32)>>,
        window: &ApplicationWindow,
    ) -> gtk::GestureDrag {
        let base_at_start = Rc::new(RefCell::new(None::<(i32, i32)>));

        let g = gtk::GestureDrag::new();
        g.set_button(1);

        {
            let bs = base_at_start.clone();
            let sg = state.clone();
            g.connect_drag_begin(move |_, _, _| {
                let (t, r) = *sg.borrow();
                *bs.borrow_mut() = Some((t, r));
            });
        }

        {
            let bs2 = base_at_start.clone();
            let w2 = window.clone();
            let su = state.clone();

            g.connect_drag_update(move |_, ox: f64, oy: f64| {
                if let Some((bt, br)) = *bs2.borrow() {
                    let new_top = bt + oy as i32;
                    let new_right = br - ox as i32;
                    *su.borrow_mut() = (new_top, new_right);
                    w2.set_margin(Edge::Top, new_top);
                    w2.set_margin(Edge::Right, new_right);
                }
            });
        }

        {
            let bs4 = base_at_start.clone();
            let se = state.clone();
            g.connect_drag_end(move |_, ox: f64, oy: f64| {
                if let Some((bt, br)) = *bs4.borrow() {
                    *se.borrow_mut() = (bt + oy as i32, br - ox as i32);
                }
            });
        }

        g
    }

    // Returns (right_margin, top_margin).
    pub fn get_float_position(&self) -> (i32, i32) {
        if let Some(ref ds) = self.drag_state {
            let (t, r) = *ds.borrow();
            (r, t)
        } else {
            (60, 60)
        }
    }

    pub fn update_config_position(&mut self) {
        if self.is_floating() {
            let (right_margin, top_margin) = self.get_float_position();
            self.config.margin_from_x = right_margin;
            self.config.margin_from_y = top_margin;
        }
    }

    pub fn attach(&mut self, section_box: &gtk::Box) {
        if !self.is_floating() {
            return;
        }
        self.config.margin_from_x = 0;
        self.config.margin_from_y = 0;
        self.drag_state = None;

        if let Some(win) = self.float_win.take() {
            win.set_child(None::<&gtk::Widget>);
            win.close();
        }
        section_box.append(&self.row_box);
    }

    pub fn update_value(&self, monitor_state: &MonitorState) {
        let text = call_by_name(monitor_state, &self.config.method);
        self.value_label.set_label(&text);
    }
}
