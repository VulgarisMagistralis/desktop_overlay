use gtk::gdk::Display;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, DrawingArea};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// GTK is single-threaded — thread_local gives us Cell semantics without needing Sync.
thread_local! {
    static SEED: Cell<u64> = Cell::new(0);
}

pub struct Snowflake {
    x: f64,
    y: f64,
    speed: f64,
    size: f64,
}

struct SnowScene {
    snowflakes: Vec<Snowflake>,
}

fn rand_f64(min: f64, max: f64) -> f64 {
    SEED.with(|seed| {
        let mut s = seed.get();
        if s == 0 {
            s = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
        }
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
        seed.set(s);
        min + (s as f64 / u64::MAX as f64) * (max - min)
    })
}

/// Manages the snow overlay window lifecycle (create, toggle, destroy).
pub struct SnowWindow {
    app: Application,
    width: i32,
    height: i32,
    win: Option<ApplicationWindow>,
}

impl SnowWindow {
    pub fn new(app: &Application, width: i32, height: i32) -> Self {
        Self {
            app: app.clone(),
            width,
            height,
            win: None,
        }
    }

    pub fn is_on(&self) -> bool {
        self.win.is_some()
    }

    pub fn show(&mut self) {
        if self.is_on() {
            return;
        }
        let win: ApplicationWindow =
            ApplicationWindow::builder().application(&self.app).build();
        win.init_layer_shell();
        win.set_layer(Layer::Background);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            win.set_anchor(edge, true);
        }
        win.add_css_class("vbox-snow");
        win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
        win.set_exclusive_zone(-1);
        let da = SnowOverlay::new(self.width, self.height).drawing_area().clone();
        da.set_can_target(false);
        win.set_child(Some(&da));
        win.present();
        self.win = Some(win);
    }

    pub fn hide(&mut self) {
        if !self.is_on() {
            return;
        }
        if let Some(ref win) = self.win {
            win.close();
        }
        self.win = None;
    }

    pub fn set_snow_state(&mut self, on: bool) {
        if on && !self.is_on() {
            self.show();
        } else if !on && self.is_on() {
            self.hide();
        }
    }

    #[allow(dead_code)]
    pub fn toggle(&mut self) {
        if self.is_on() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn get_monitor_dimensions(display: &Display) -> (i32, i32) {
        let monitors = display.monitors();
        let gdk_monitor: gtk::gdk::Monitor = monitors
            .item(0)
            .expect("No monitor found")
            .downcast()
            .expect("Not a Monitor");
        let geo = gdk_monitor.geometry();
        let scale = gdk_monitor.scale_factor() as i32;
        (geo.width() * scale, geo.height() * scale)
    }
}

#[allow(dead_code)]
pub struct SnowOverlay {
    drawing_area: DrawingArea,
    _scene: Rc<RefCell<SnowScene>>,
    _width: f64,
    _height: f64,
}

impl SnowOverlay {
    pub fn new(width: i32, height: i32) -> Self {
        let drawing_area = DrawingArea::new();
        drawing_area.set_can_target(false);

        let scene = Rc::new(RefCell::new(SnowScene {
            snowflakes: (0..500)
                .map(|_| Snowflake {
                    x: rand_f64(0.0, width.into()),
                    y: rand_f64(0.0, height.into()),
                    speed: rand_f64(1.0, 3.0),
                    size: rand_f64(1.0, 4.0),
                })
                .collect(),
        }));

        let w = width as f64;
        let h = height as f64;

        // Animation loop — update positions every ~10ms (60fps)
        let scene_clone = scene.clone();
        let da_clone = drawing_area.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(10), move || {
            let mut s = scene_clone.borrow_mut();
            for flake in &mut s.snowflakes {
                flake.y += flake.speed;
                if flake.y > h {
                    flake.y = -10.0;
                    flake.x = rand_f64(0.0, w);
                }
            }
            da_clone.queue_draw();
            glib::ControlFlow::Continue
        });

        // Drawing logic
        let scene_draw = scene.clone();
        drawing_area.set_draw_func(move |_, cr, _, _| {
            let s = scene_draw.borrow();
            // Transparent background
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
            cr.paint().expect("Failed to paint background");
            // Draw snowflakes — white
            cr.set_source_rgb(1.0, 1.0, 1.0);
            for flake in &s.snowflakes {
                cr.set_line_width(flake.size * 0.4);
                for i in 0..6 {
                    let angle = (i as f64) * std::f64::consts::PI / 3.0;
                    let end_x = flake.x + flake.size * angle.cos();
                    let end_y = flake.y + flake.size * angle.sin();
                    cr.move_to(flake.x, flake.y);
                    cr.line_to(end_x, end_y);

                    // Small branches at the ends of arms
                    let branch_angle1 = angle + std::f64::consts::PI / 6.0;
                    let branch_angle2 = angle - std::f64::consts::PI / 6.0;
                    let b_len = flake.size * 0.4;
                    cr.move_to(end_x, end_y);
                    cr.line_to(
                        end_x + b_len * branch_angle1.cos(),
                        end_y + b_len * branch_angle1.sin(),
                    );
                    cr.move_to(end_x, end_y);
                    cr.line_to(
                        end_x + b_len * branch_angle2.cos(),
                        end_y + b_len * branch_angle2.sin(),
                    );
                }
                cr.stroke().expect("Failed to draw snowflake");
            }
        });

        SnowOverlay {
            drawing_area,
            _scene: scene,
            _width: w,
            _height: h,
        }
    }

    pub fn drawing_area(&self) -> &DrawingArea {
        &self.drawing_area
    }
}
