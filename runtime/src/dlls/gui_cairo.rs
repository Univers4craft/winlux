//! Antialiased vector primitives on the same Xlib connection as Xft text.
use std::ffi::{c_int, c_ulong, c_void};

pub enum Shape {
    Ellipse {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        width: f64,
    },
}

pub struct Cairo {
    library: *mut c_void,
    surface: *mut c_void,
    context: *mut c_void,
    key: (u32, i32, i32),
    surface_create:
        unsafe extern "C" fn(*mut c_void, c_ulong, *mut c_void, c_int, c_int) -> *mut c_void,
    surface_destroy: unsafe extern "C" fn(*mut c_void),
    surface_flush: unsafe extern "C" fn(*mut c_void),
    surface_dirty: unsafe extern "C" fn(*mut c_void),
    create: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    destroy: unsafe extern "C" fn(*mut c_void),
    status: unsafe extern "C" fn(*mut c_void) -> c_int,
    new_path: unsafe extern "C" fn(*mut c_void),
    source: unsafe extern "C" fn(*mut c_void, f64, f64, f64),
    move_to: unsafe extern "C" fn(*mut c_void, f64, f64),
    line_to: unsafe extern "C" fn(*mut c_void, f64, f64),
    line_width: unsafe extern "C" fn(*mut c_void, f64),
    stroke: unsafe extern "C" fn(*mut c_void),
    arc: unsafe extern "C" fn(*mut c_void, f64, f64, f64, f64, f64),
    save: unsafe extern "C" fn(*mut c_void),
    restore: unsafe extern "C" fn(*mut c_void),
    translate: unsafe extern "C" fn(*mut c_void, f64, f64),
    scale: unsafe extern "C" fn(*mut c_void, f64, f64),
    fill: unsafe extern "C" fn(*mut c_void),
}
unsafe impl Send for Cairo {}

impl Cairo {
    pub unsafe fn load() -> Option<Self> {
        let library = libc::dlopen(c"libcairo.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if library.is_null() {
            return None;
        }
        macro_rules! symbol {
            ($name:literal) => {{
                let ptr = libc::dlsym(library, concat!($name, "\0").as_ptr().cast());
                if ptr.is_null() {
                    libc::dlclose(library);
                    return None;
                }
                std::mem::transmute_copy(&ptr)
            }};
        }
        Some(Self {
            library,
            surface: std::ptr::null_mut(),
            context: std::ptr::null_mut(),
            key: (0, 0, 0),
            surface_create: symbol!("cairo_xlib_surface_create"),
            surface_destroy: symbol!("cairo_surface_destroy"),
            surface_flush: symbol!("cairo_surface_flush"),
            surface_dirty: symbol!("cairo_surface_mark_dirty"),
            create: symbol!("cairo_create"),
            destroy: symbol!("cairo_destroy"),
            status: symbol!("cairo_status"),
            new_path: symbol!("cairo_new_path"),
            source: symbol!("cairo_set_source_rgb"),
            move_to: symbol!("cairo_move_to"),
            line_to: symbol!("cairo_line_to"),
            line_width: symbol!("cairo_set_line_width"),
            stroke: symbol!("cairo_stroke"),
            arc: symbol!("cairo_arc"),
            save: symbol!("cairo_save"),
            restore: symbol!("cairo_restore"),
            translate: symbol!("cairo_translate"),
            scale: symbol!("cairo_scale"),
            fill: symbol!("cairo_fill"),
        })
    }

    pub unsafe fn clear(&mut self) {
        if !self.context.is_null() {
            (self.destroy)(self.context);
            self.context = std::ptr::null_mut();
        }
        if !self.surface.is_null() {
            (self.surface_destroy)(self.surface);
            self.surface = std::ptr::null_mut();
        }
    }

    pub unsafe fn draw(
        &mut self,
        display: *mut c_void,
        visual: *mut c_void,
        drawable: u32,
        width: i32,
        height: i32,
        color: u32,
        shape: Shape,
    ) -> bool {
        if width <= 0 || height <= 0 {
            return false;
        }
        let key = (drawable, width, height);
        if self.context.is_null() || self.key != key {
            self.clear();
            self.surface =
                (self.surface_create)(display, drawable as c_ulong, visual, width, height);
            self.context = (self.create)(self.surface);
            if (self.status)(self.context) != 0 {
                self.clear();
                return false;
            }
            self.key = key;
        }
        let cr = self.context;
        (self.surface_dirty)(self.surface);
        (self.new_path)(cr);
        (self.source)(
            cr,
            (color & 255) as f64 / 255.0,
            ((color >> 8) & 255) as f64 / 255.0,
            ((color >> 16) & 255) as f64 / 255.0,
        );
        match shape {
            Shape::Ellipse {
                x,
                y,
                width,
                height,
            } => {
                if width <= 0.0 || height <= 0.0 {
                    return false;
                }
                (self.save)(cr);
                (self.translate)(cr, x + width / 2.0, y + height / 2.0);
                (self.scale)(cr, width / 2.0, height / 2.0);
                (self.arc)(cr, 0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
                (self.restore)(cr);
                (self.fill)(cr);
            }
            Shape::Line {
                x1,
                y1,
                x2,
                y2,
                width,
            } => {
                (self.line_width)(cr, width.max(1.0));
                (self.move_to)(cr, x1 + 0.5, y1 + 0.5);
                (self.line_to)(cr, x2 + 0.5, y2 + 0.5);
                (self.stroke)(cr);
            }
        }
        (self.surface_flush)(self.surface);
        (self.status)(cr) == 0
    }
}
impl Drop for Cairo {
    fn drop(&mut self) {
        unsafe {
            self.clear();
            libc::dlclose(self.library);
        }
    }
}
