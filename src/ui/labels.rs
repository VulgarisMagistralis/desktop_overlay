use crate::widget::{DataWidget, DataWidgetConfig};
use gtk::prelude::*;
use gtk::{Builder, Label};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

pub const SECTION_IDS: &[&str] = &[
    "SYSTEM",
    "FILESYSTEMS",
    "NETWORKING",
    "GPU",
    "Temperature",
    "Power",
    "Clocks",
    "Misc",
];

struct RegistryInner {
    widgets: HashMap<String, DataWidget>,
    section_boxes: HashMap<String, gtk::Box>,
    section_headers: HashMap<String, Label>,
    root: Option<gtk::Box>,
    application: Option<gtk::Application>,
    detach_lock: Cell<bool>,
    section_float_wins: HashMap<String, gtk::ApplicationWindow>,
}

pub struct WidgetRegistry {
    inner: RefCell<RegistryInner>,
    weak_self: RefCell<Option<Weak<WidgetRegistry>>>,
}

impl WidgetRegistry {
    pub fn empty() -> Self {
        Self {
            inner: RefCell::new(RegistryInner {
                widgets: HashMap::new(),
                section_boxes: HashMap::new(),
                section_headers: HashMap::new(),
                root: None,
                application: None,
                detach_lock: Cell::new(false),
                section_float_wins: HashMap::new(),
            }),
            weak_self: RefCell::new(None),
        }
    }

    pub fn new(configs: Vec<DataWidgetConfig>, section_boxes: &HashMap<String, gtk::Box>) -> Self {
        let mut wg = HashMap::new();
        let mut sof = HashMap::new();

        for config in &configs {
            let widget = DataWidget::new(config.clone());
            sof.insert(config.id.clone(), config.section_id.clone());
            if config.is_docked() {
                if let Some(box_) = section_boxes.get(&config.section_id) {
                    box_.append(&widget.row_box);
                }
            }
            wg.insert(config.id.clone(), widget);
        }

        Self {
            inner: RefCell::new(RegistryInner {
                widgets: wg,
                section_boxes: section_boxes.clone(),
                section_headers: HashMap::new(),
                root: None,
                application: None,
                detach_lock: Cell::new(false),
                section_float_wins: HashMap::new(),
            }),
            weak_self: RefCell::new(None),
        }
    }

    pub fn init_self_ref(this: &Rc<WidgetRegistry>) {
        *this.weak_self.borrow_mut() = Some(Rc::downgrade(this));
    }

    fn get_weak(&self) -> Weak<WidgetRegistry> {
        self.weak_self
            .borrow()
            .as_ref()
            .expect("WidgetRegistry::init_self_ref not called")
            .clone()
    }

    pub fn set_context(&self, root: gtk::Box, application: gtk::Application) {
        let mut inner = self.inner.borrow_mut();
        inner.root = Some(root);
        inner.application = Some(application);
    }

    pub fn set_section_headers(&self, headers: HashMap<String, Label>) {
        let mut inner = self.inner.borrow_mut();
        inner.section_headers = headers;
    }

    pub fn setup_detach_gestures(&self) {
        eprintln!(
            "[SETUP-DETACH] entering, widgets={}, sections={}",
            self.inner.borrow().widgets.len(),
            self.inner.borrow().section_headers.len()
        );
        let weak = self.get_weak();
        let inner = self.inner.borrow();
        for (id, widget) in &inner.widgets {
            let id_clone = id.clone();
            let weak_clone = weak.clone();
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            gesture.connect_released(move |_, n_press, _, _| {
                if n_press == 2 {
                    if let Some(reg) = weak_clone.upgrade() {
                        // Block detach/reattach when lock is active.
                        if reg.is_lock_active() {
                            return;
                        }
                        if reg.is_docked(&id_clone) {
                            reg.detach_widget(&id_clone);
                        } else {
                            reg.reattach_widget(&id_clone);
                        }
                    }
                }
            });
            widget.header_label.add_controller(gesture);
        }

        // Section header double-click to detach/reattach entire section.
        for (section_id, header) in &inner.section_headers {
            let sec_clone = section_id.clone();
            let weak_clone = weak.clone();
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            gesture.connect_released(move |_, n_press, _, _| {
                eprintln!(
                    "[SECTION-DETACH] gesture released n_press={} sec={}",
                    n_press, sec_clone
                );
                if n_press == 2 {
                    if let Some(reg) = weak_clone.upgrade() {
                        if reg.is_lock_active() {
                            eprintln!("[SECTION-DETACH] lock active, ignoring");
                            return;
                        }
                        if reg.is_section_docked(&sec_clone) {
                            eprintln!("[SECTION-DETACH] section docked, detaching");
                            reg.detach_section(&sec_clone);
                        } else {
                            eprintln!("[SECTION-DETACH] detaching->reattaching");
                            reg.reattach_section(&sec_clone);
                        }
                    }
                }
            });
            header.add_controller(gesture);
        }

        drop(inner);
    }

    pub fn detach_widget(&self, id: &str) {
        let section_box = {
            let inner = self.inner.borrow();
            let widget = match inner.widgets.get(id) {
                Some(w) if w.is_docked() => w,
                _ => return,
            };
            let section_id = widget.config.section_id.clone();
            match inner.section_boxes.get(&section_id) {
                Some(b) => b.clone(),
                None => return,
            }
        };

        let application = {
            let inner = self.inner.borrow();
            match inner.application {
                Some(ref app) => app.clone(),
                None => return,
            }
        };

        {
            let mut inner = self.inner.borrow_mut();
            let widget = inner.widgets.get_mut(id).unwrap();
            widget.detach(&section_box, &application);
        }

        // Set up reattach gesture on the floating window
        let weak = self.get_weak();
        let id_clone = id.to_string();
        let inner = self.inner.borrow();
        let widget = inner.widgets.get(id).unwrap();
        if let Some(ref win) = widget.float_win {
            let weak_clone = weak.clone();
            let win_gesture = gtk::GestureClick::new();
            win_gesture.set_button(1);
            win_gesture.connect_released(move |_, n_press, _, _| {
                if n_press == 2 {
                    if let Some(reg) = weak_clone.upgrade() {
                        reg.reattach_widget(&id_clone);
                    }
                }
            });
            win.add_controller(win_gesture);
        }
    }

    pub fn reattach_widget(&self, id: &str) {
        let section_box = {
            let inner = self.inner.borrow();
            let widget = match inner.widgets.get(id) {
                Some(w) if w.is_floating() => w,
                _ => return,
            };
            let section_id = widget.config.section_id.clone();
            match inner.section_boxes.get(&section_id) {
                Some(b) => b.clone(),
                None => return,
            }
        };

        let mut inner = self.inner.borrow_mut();
        let widget = inner.widgets.get_mut(id).unwrap();
        widget.attach(&section_box);
    }

    pub fn pre_save_update_positions(&self) {
        let mut inner = self.inner.borrow_mut();
        for widget in inner.widgets.values_mut() {
            widget.update_config_position();
        }
    }

    pub fn save_layout(&self) -> Vec<DataWidgetConfig> {
        let inner = self.inner.borrow();
        inner.widgets.values().map(|w| w.config.clone()).collect()
    }

    pub fn restore_layout(&self, configs: &[DataWidgetConfig]) {
        for c in configs {
            if let Some(w) = self.inner.borrow_mut().widgets.get_mut(&c.id) {
                w.config.margin_from_x = c.margin_from_x;
                w.config.margin_from_y = c.margin_from_y;
            }
            if c.is_floating() {
                self.detach_widget(&c.id);
            }
        }
    }

    pub fn is_docked(&self, id: &str) -> bool {
        let inner = self.inner.borrow();
        inner
            .widgets
            .get(id)
            .map(|w| w.is_docked())
            .unwrap_or(false)
    }

    /// Checks if all widgets in a section are docked.
    pub fn is_section_docked(&self, section_id: &str) -> bool {
        let inner = self.inner.borrow();
        for (_, widget) in &inner.widgets {
            if widget.config.section_id == section_id && widget.is_floating() {
                return false;
            }
        }
        true
    }

    /// Detach all widgets of a section into a single shared floating window.
    pub fn detach_section(&self, section_id: &str) {
        eprintln!("[DETACH-SEC] entry for '{}'", section_id);

        let application = {
            let inner = self.inner.borrow();
            match inner.application {
                Some(ref app) => app.clone(),
                None => {
                    eprintln!("[DETACH-SEC] FAIL: application is None");
                    return;
                }
            }
        };

        let section_box = {
            let inner = self.inner.borrow();
            match inner.section_boxes.get(section_id) {
                Some(b) => b.clone(),
                None => {
                    eprintln!(
                        "[DETACH-SEC] FAIL: section_box missing for '{}'",
                        section_id
                    );
                    return;
                }
            }
        };

        let (initial_top, initial_right) = self
            .inner
            .borrow()
            .widgets
            .iter()
            .find(|(_, w)| w.config.section_id == section_id && w.config.margin_from_y != 0)
            .map(|(_, w)| (w.config.margin_from_y, w.config.margin_from_x))
            .unwrap_or((70, 70));

        let ids: Vec<String> = self
            .inner
            .borrow()
            .widgets
            .iter()
            .filter(|(_, w)| w.config.section_id == section_id && w.is_docked())
            .map(|(id, _)| id.clone())
            .collect();

        if ids.is_empty() {
            eprintln!(
                "[DETACH-SEC] no docked widgets in '{}', exiting",
                section_id
            );
            return;
        }

        eprintln!("[DETACH-SEC] detaching {} widgets: {:?}", ids.len(), ids);

        // Grab the header label so we can move it into the float window.
        let header = self.inner.borrow().section_headers.get(section_id).cloned();

        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Put the header first inside the floating container.
        if let Some(ref hdr) = header {
            section_box.remove(hdr);
            container.append(hdr);
        }

        {
            let mut inner = self.inner.borrow_mut();
            for wid in &ids {
                if let Some(w) = inner.widgets.get_mut(wid) {
                    section_box.remove(&w.row_box);
                    container.append(&w.row_box);
                    w.config.margin_from_x = initial_right;
                    w.config.margin_from_y = initial_top;
                }
            }
        }

        let fw = DataWidget::make_float_window(&application, initial_top, initial_right);
        fw.set_child(Some(&container));
        fw.present();

        {
            let mut inner = self.inner.borrow_mut();
            inner
                .section_float_wins
                .insert(section_id.to_string(), fw.clone());
        }

        // Shared drag state for all widgets in this section.
        let shared_state = Rc::new(RefCell::new((initial_top, initial_right)));
        {
            let mut inner = self.inner.borrow_mut();
            for wid in &ids {
                if let Some(w) = inner.widgets.get_mut(wid) {
                    w.float_win = Some(fw.clone());
                    w.drag_state = Some(shared_state.clone());
                }
            }
        }

        fw.add_controller(DataWidget::make_drag_gesture(shared_state, &fw));

        // Double-click anywhere to reattach section.
        let weak = self.get_weak();
        let sec_clone = section_id.to_string();
        {
            let weak_clone = weak.clone();
            let id_c2 = sec_clone.clone();
            let win_gesture = gtk::GestureClick::new();
            win_gesture.set_button(1);
            win_gesture.connect_released(move |_, n_press, _, _| {
                if n_press == 2 {
                    if let Some(reg) = weak_clone.upgrade() {
                        reg.reattach_section(&id_c2);
                    }
                }
            });
            fw.add_controller(win_gesture);
        }
    }

    /// Reattach all floating widgets of a section back to the main overlay.
    pub fn reattach_section(&self, section_id: &str) {
        eprintln!("[REATTACH-SEC] entry for '{}'", section_id);

        // Grab target box once outside any mutable borrow.
        let section_box = match self.inner.borrow().section_boxes.get(section_id) {
            Some(b) => b.clone(),
            None => {
                eprintln!(
                    "[REATTACH-SEC] FAIL: section_box missing for '{}'",
                    section_id
                );
                return;
            }
        };

        // Collect IDs of floating widgets in this section.
        let ids: Vec<String> = self
            .inner
            .borrow()
            .widgets
            .iter()
            .filter(|(_, w)| w.config.section_id == section_id && w.is_floating())
            .map(|(id, _)| id.clone())
            .collect();

        if ids.is_empty() {
            eprintln!(
                "[REATTACH-SEC] no floating widgets in '{}', exiting",
                section_id
            );
            return;
        }

        eprintln!(
            "[REATTACH-SEC] reattaching {} widgets: {:?}",
            ids.len(),
            ids
        );

        // 1. Remove each row_box + header from the float container and re-append to parent.
        let mut inner = self.inner.borrow_mut();
        for wid in &ids {
            if let Some(w) = inner.widgets.get_mut(wid.as_str()) {
                if let Some(ref win) = w.float_win {
                    if let Some(fw_container) = win.child() {
                        if let Some(c) = fw_container.dynamic_cast_ref::<gtk::Box>() {
                            c.remove(&w.row_box);
                        }
                    }
                }
                section_box.append(&w.row_box);
                w.config.margin_from_x = 0;
                w.config.margin_from_y = 0;
                w.drag_state = None;
            }
        }

        // Re-attach the header label at top of section_box.
        if let Some(hdr) = inner.section_headers.get(section_id) {
            // If it's sitting inside the float container, remove it first.
            if let Some(ref win) = inner
                .widgets
                .values()
                .next()
                .and_then(|w| w.float_win.as_ref())
            {
                if let Some(fw_container) = win.child() {
                    if let Some(c) = fw_container.dynamic_cast_ref::<gtk::Box>() {
                        c.remove(hdr);
                    }
                }
            }
            // If header is a toplevel (no parent), just append; otherwise remove from current.
            if hdr.parent().is_some() {
                hdr.unparent();
            }
            section_box.prepend(hdr);
        }

        // 2. Close and drop the shared float window after all rows are back in place.
        let win_opt = inner.section_float_wins.remove(section_id);
        for wid in &ids {
            if let Some(w) = inner.widgets.get_mut(wid.as_str()) {
                w.float_win = None;
            }
        }
        drop(inner);

        // 3. Actually destroy the window outside the borrow to avoid refcount loops.
        if let Some(win) = win_opt {
            win.set_child(None::<&gtk::Widget>);
            win.close();
        }
    }

    pub fn is_lock_active(&self) -> bool {
        let inner = self.inner.borrow();
        inner.detach_lock.get()
    }

    pub fn set_lock(&self, locked: bool) {
        let inner = self.inner.borrow_mut();
        inner.detach_lock.set(locked);
    }

    pub fn update_all(&self, monitor_state: &crate::monitor::MonitorState) {
        let inner = self.inner.borrow();
        for widget in inner.widgets.values() {
            widget.update_value(monitor_state);
        }
    }
}

pub fn extract_sections(builder: &Builder) -> HashMap<String, (gtk::Box, Label)> {
    let mut map = HashMap::new();
    for &sid in SECTION_IDS {
        let container_id = format!("section_{}", sid);
        let header_id = format!("hdr_{}", sid);
        if let (Some(container), Some(header)) = (
            builder.object::<gtk::Box>(&container_id),
            builder.object::<Label>(&header_id),
        ) {
            map.insert(sid.to_string(), (container, header));
        }
    }
    map
}

pub fn default_configs() -> Vec<crate::widget::DataWidgetConfig> {
    let mut widgets = crate::widget::load_widgets();
    widgets.sort_by(|a, b| {
        a.section_id
            .cmp(&b.section_id)
            .then(a.order.unwrap_or(0).cmp(&b.order.unwrap_or(0)))
    });
    widgets
}
