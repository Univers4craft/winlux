//! Backend X11 de Winlux : crée de vraies fenêtres sur le serveur X et traduit
//! ses événements en messages Win32. Le fond utilise la palette claire
//! Winlux inspirée de Windows 11, définie dans `gui::theme`.

use crate::dlls::gui::theme;
use crate::dlls::gui::{self, BitmapSnapshot, DrawCmd, FontSpec, PendingEvent, Rect};
use crate::dlls::gui_cairo::{Cairo, Shape};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_uchar, c_ulong, c_void, CString};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeGCAux, ChangeWindowAttributesAux, ConfigureNotifyEvent,
    ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux, Gcontext, Window,
    WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

#[derive(Clone, Copy)]
struct XWin {
    window: Window,
    gc: Gcontext,
    x: i32,
    y: i32,
    width: u16,
    height: u16,
}

#[derive(Clone, Copy)]
struct XFont {
    id: u32,
    ascent: i16,
    char_width: i16,
}

struct BitmapCache {
    key: [i32; 8],
    pixels: Vec<u32>,
    pixmap: u32,
}

fn truecolor_pixel(colorref: u32, red: u32, green: u32, blue: u32) -> u32 {
    let channel = |value: u32, mask: u32| {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        ((((value as u64 * (mask >> shift) as u64 + 127) / 255) as u32) << shift) & mask
    };
    channel(colorref & 255, red)
        | channel((colorref >> 8) & 255, green)
        | channel((colorref >> 16) & 255, blue)
}

use crate::dlls::gui::bilinear_colorref;

#[repr(C)]
#[derive(Clone, Copy)]
struct XRenderColor {
    red: u16,
    green: u16,
    blue: u16,
    alpha: u16,
}

#[repr(C)]
struct XftColor {
    pixel: c_ulong,
    color: XRenderColor,
}

#[repr(C)]
struct XftFont {
    ascent: c_int,
    descent: c_int,
    height: c_int,
    max_advance_width: c_int,
    charset: *mut c_void,
    pattern: *mut c_void,
}

#[repr(C)]
#[derive(Default)]
struct XGlyphInfo {
    width: u16,
    height: u16,
    x: i16,
    y: i16,
    x_off: i16,
    y_off: i16,
}

type Display = c_void;
type Visual = c_void;
type XftDraw = c_void;

/// Xlib applique la vraie table XKB du serveur (AZERTY/QWERTY, AltGr,
/// groupes et Caps Lock). Le rendu Xft peut être absent sans perdre le clavier.
struct XkbKeyboard {
    library: *mut c_void,
    display: *mut Display,
    close: unsafe extern "C" fn(*mut Display) -> c_int,
    lookup: unsafe extern "C" fn(*mut Display, u8, u32, *mut u32, *mut c_ulong) -> c_int,
    symbol: unsafe extern "C" fn(*mut Display, u8, c_int, c_int) -> c_ulong,
}

unsafe impl Send for XkbKeyboard {}

impl XkbKeyboard {
    unsafe fn connect() -> Option<Self> {
        unsafe fn symbol<T: Copy>(lib: *mut c_void, name: &[u8]) -> Option<T> {
            let ptr = libc::dlsym(lib, name.as_ptr().cast());
            if ptr.is_null() {
                None
            } else {
                Some(std::mem::transmute_copy(&ptr))
            }
        }
        let library = libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if library.is_null() {
            return None;
        }
        let api = (|| {
            let open: unsafe extern "C" fn(*const c_char) -> *mut Display =
                symbol(library, b"XOpenDisplay\0")?;
            let close = symbol(library, b"XCloseDisplay\0")?;
            let lookup = symbol(library, b"XkbLookupKeySym\0")?;
            let key_symbol = symbol(library, b"XkbKeycodeToKeysym\0")?;
            let display = open(std::ptr::null());
            if display.is_null() {
                return None;
            }
            Some(Self {
                library,
                display,
                close,
                lookup,
                symbol: key_symbol,
            })
        })();
        if api.is_none() {
            libc::dlclose(library);
        }
        api
    }

    fn translate(&self, code: u8, state: u16) -> Option<(u32, u32, u32, u32)> {
        let mut consumed = 0;
        let mut effective = 0;
        unsafe {
            if (self.lookup)(
                self.display,
                code,
                state as u32,
                &mut consumed,
                &mut effective,
            ) == 0
            {
                return None;
            }
            let group = ((state >> 13) & 3) as c_int;
            let base = (self.symbol)(self.display, code, group, 0) as u32;
            let shifted = (self.symbol)(self.display, code, group, 1) as u32;
            Some((base, shifted, effective as u32, consumed))
        }
    }
}

impl Drop for XkbKeyboard {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.display);
            libc::dlclose(self.library);
        }
    }
}

struct XftApi {
    x11: *mut c_void,
    xft: *mut c_void,
    open_display: unsafe extern "C" fn(*const c_char) -> *mut Display,
    close_display: unsafe extern "C" fn(*mut Display) -> c_int,
    default_screen: unsafe extern "C" fn(*mut Display) -> c_int,
    default_visual: unsafe extern "C" fn(*mut Display, c_int) -> *mut Visual,
    default_colormap: unsafe extern "C" fn(*mut Display, c_int) -> c_ulong,
    sync: unsafe extern "C" fn(*mut Display, c_int) -> c_int,
    draw_create: unsafe extern "C" fn(*mut Display, c_ulong, *mut Visual, c_ulong) -> *mut XftDraw,
    draw_destroy: unsafe extern "C" fn(*mut XftDraw),
    font_open_name: unsafe extern "C" fn(*mut Display, c_int, *const c_char) -> *mut XftFont,
    font_close: unsafe extern "C" fn(*mut Display, *mut XftFont),
    color_alloc: unsafe extern "C" fn(
        *mut Display,
        *mut Visual,
        c_ulong,
        *const XRenderColor,
        *mut XftColor,
    ) -> c_int,
    color_free: unsafe extern "C" fn(*mut Display, *mut Visual, c_ulong, *mut XftColor),
    text_extents:
        unsafe extern "C" fn(*mut Display, *mut XftFont, *const c_uchar, c_int, *mut XGlyphInfo),
    draw_string: unsafe extern "C" fn(
        *mut XftDraw,
        *const XftColor,
        *mut XftFont,
        c_int,
        c_int,
        *const c_uchar,
        c_int,
    ),
}

unsafe impl Send for XftApi {}

impl XftApi {
    unsafe fn load() -> Option<Self> {
        unsafe fn open(name: &[u8]) -> *mut c_void {
            libc::dlopen(name.as_ptr().cast(), libc::RTLD_LAZY | libc::RTLD_LOCAL)
        }
        unsafe fn symbol<T: Copy>(library: *mut c_void, name: &[u8]) -> Option<T> {
            let ptr = libc::dlsym(library, name.as_ptr().cast());
            if ptr.is_null() {
                None
            } else {
                Some(std::mem::transmute_copy(&ptr))
            }
        }

        let x11 = open(b"libX11.so.6\0");
        let xft = open(b"libXft.so.2\0");
        if x11.is_null() || xft.is_null() {
            if !x11.is_null() {
                libc::dlclose(x11);
            }
            if !xft.is_null() {
                libc::dlclose(xft);
            }
            return None;
        }
        macro_rules! required {
            ($library:expr, $name:expr) => {
                match symbol($library, $name) {
                    Some(value) => value,
                    None => {
                        libc::dlclose(xft);
                        libc::dlclose(x11);
                        return None;
                    }
                }
            };
        }
        Some(Self {
            x11,
            xft,
            open_display: required!(x11, b"XOpenDisplay\0"),
            close_display: required!(x11, b"XCloseDisplay\0"),
            default_screen: required!(x11, b"XDefaultScreen\0"),
            default_visual: required!(x11, b"XDefaultVisual\0"),
            default_colormap: required!(x11, b"XDefaultColormap\0"),
            sync: required!(x11, b"XSync\0"),
            draw_create: required!(xft, b"XftDrawCreate\0"),
            draw_destroy: required!(xft, b"XftDrawDestroy\0"),
            font_open_name: required!(xft, b"XftFontOpenName\0"),
            font_close: required!(xft, b"XftFontClose\0"),
            color_alloc: required!(xft, b"XftColorAllocValue\0"),
            color_free: required!(xft, b"XftColorFree\0"),
            text_extents: required!(xft, b"XftTextExtentsUtf8\0"),
            draw_string: required!(xft, b"XftDrawStringUtf8\0"),
        })
    }
}

struct XftRenderer {
    api: XftApi,
    display: *mut Display,
    screen: c_int,
    visual: *mut Visual,
    colormap: c_ulong,
    fonts: HashMap<FontSpec, *mut XftFont>,
    cairo: Option<Cairo>,
}

unsafe impl Send for XftRenderer {}

impl XftRenderer {
    unsafe fn connect() -> Option<Self> {
        let api = XftApi::load()?;
        let display = (api.open_display)(std::ptr::null());
        if display.is_null() {
            return None;
        }
        let screen = (api.default_screen)(display);
        let visual = (api.default_visual)(display, screen);
        let colormap = (api.default_colormap)(display, screen);
        if visual.is_null() {
            (api.close_display)(display);
            return None;
        }
        Some(Self {
            api,
            display,
            screen,
            visual,
            colormap,
            fonts: HashMap::new(),
            cairo: Cairo::load(),
        })
    }

    fn font(&mut self, spec: &FontSpec) -> Option<*mut XftFont> {
        if let Some(font) = self.fonts.get(spec) {
            return Some(*font);
        }
        let family = if spec.face.is_empty() || spec.face.eq_ignore_ascii_case("fixed") {
            "Noto Sans"
        } else {
            spec.face.as_str()
        };
        let size = spec.height.saturating_abs().max(9);
        let weight = if spec.weight >= 600 {
            "bold"
        } else {
            "regular"
        };
        let slant = if spec.italic { "italic" } else { "roman" };
        let pattern = CString::new(format!(
            "{family}:pixelsize={size}:weight={weight}:slant={slant}:antialias=true:hinting=true:hintstyle=hintfull:rgba=none:lcdfilter=lcdnone"
        ))
        .ok()?;
        let font =
            unsafe { (self.api.font_open_name)(self.display, self.screen, pattern.as_ptr()) };
        if font.is_null() {
            return None;
        }
        self.fonts.insert(spec.clone(), font);
        Some(font)
    }

    fn measure(&mut self, spec: &FontSpec, text: &[u8]) -> Option<(i32, i32, i32)> {
        let font = self.font(spec)?;
        let mut extents = XGlyphInfo::default();
        unsafe {
            (self.api.text_extents)(
                self.display,
                font,
                text.as_ptr(),
                text.len() as c_int,
                &mut extents,
            );
            Some((
                extents.x_off.max(extents.width as i16) as i32,
                (*font).height.max(1),
                (*font).ascent.max(1),
            ))
        }
    }

    fn draw(
        &mut self,
        drawable: u32,
        spec: &FontSpec,
        x: i32,
        baseline: i32,
        text: &[u8],
        colorref: u32,
    ) -> bool {
        let Some(font) = self.font(spec) else {
            return false;
        };
        let render = XRenderColor {
            red: ((colorref & 0xFF) as u16) * 257,
            green: (((colorref >> 8) & 0xFF) as u16) * 257,
            blue: (((colorref >> 16) & 0xFF) as u16) * 257,
            alpha: u16::MAX,
        };
        unsafe {
            let draw = (self.api.draw_create)(
                self.display,
                drawable as c_ulong,
                self.visual,
                self.colormap,
            );
            if draw.is_null() {
                return false;
            }
            let mut color = std::mem::zeroed::<XftColor>();
            if (self.api.color_alloc)(
                self.display,
                self.visual,
                self.colormap,
                &render,
                &mut color,
            ) == 0
            {
                (self.api.draw_destroy)(draw);
                return false;
            }
            (self.api.draw_string)(
                draw,
                &color,
                font,
                x,
                baseline,
                text.as_ptr(),
                text.len() as c_int,
            );
            (self.api.color_free)(self.display, self.visual, self.colormap, &mut color);
            (self.api.draw_destroy)(draw);
        }
        true
    }

    /// Attend que le serveur ait traité les glyphes envoyés par Xlib.
    fn sync(&self) {
        unsafe {
            (self.api.sync)(self.display, 0);
        }
    }
}

impl Drop for XftRenderer {
    fn drop(&mut self) {
        unsafe {
            // Cairo registers XCloseDisplay callbacks. Release drawables first,
            // but keep its code loaded until Xlib has run those callbacks.
            let mut cairo = self.cairo.take();
            if let Some(renderer) = cairo.as_mut() {
                renderer.clear();
            }
            for (_, font) in self.fonts.drain() {
                (self.api.font_close)(self.display, font);
            }
            (self.api.close_display)(self.display);
            drop(cairo);
            libc::dlclose(self.api.xft);
            libc::dlclose(self.api.x11);
        }
    }
}

fn font_candidates(spec: &FontSpec) -> Vec<Vec<u8>> {
    let height = if spec.height == 0 {
        13
    } else {
        spec.height.saturating_abs().max(1)
    };
    let size = if height <= 8 {
        b"5x8".as_slice()
    } else if height <= 10 {
        b"6x10".as_slice()
    } else if height <= 12 {
        b"6x12".as_slice()
    } else if height <= 13 {
        b"6x13".as_slice()
    } else if height <= 14 {
        b"7x14".as_slice()
    } else if height <= 15 {
        b"9x15".as_slice()
    } else {
        b"10x20".as_slice()
    };
    let mut names = Vec::new();
    if !spec.face.is_empty() && !spec.face.eq_ignore_ascii_case("fixed") {
        names.push(spec.face.as_bytes().to_vec());
    }
    if spec.weight >= 600 {
        let mut bold = size.to_vec();
        bold.extend_from_slice(b"bold");
        names.push(bold);
    }
    names.push(size.to_vec());
    names.extend([b"fixed".to_vec(), b"6x13".to_vec()]);
    let mut unique = Vec::new();
    for name in names {
        if !unique.contains(&name) {
            unique.push(name);
        }
    }
    unique
}

fn dpi_scale(dpi: f32, requested: Option<f32>) -> f32 {
    let automatic = ((dpi.max(1.0) / 96.0) * 4.0).round() / 4.0;
    requested
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(automatic)
        .clamp(1.0, 3.0)
}

pub struct X11Backend {
    conn: RustConnection,
    screen: usize,
    colormap: u32,
    wm_protocols: Atom,
    wm_delete: Atom,
    /// hwnd → fenêtre X.
    windows: HashMap<usize, XWin>,
    /// Fenêtre X → hwnd (pour router les événements).
    by_xwin: HashMap<u32, usize>,
    /// Événements lus pendant `wait`, consommés par le prochain `pump`.
    pending: Vec<Event>,
    /// Événement supplémentaire issu d'un ConfigureNotify simultanément move/size.
    deferred_events: Vec<PendingEvent>,
    /// Dernière liste de commandes GDI par fenêtre, rejouée à chaque Expose.
    content: HashMap<usize, Vec<DrawCmd>>,
    /// Polices « core » ouvertes à la demande, indexées par les attributs GDI.
    fonts: HashMap<FontSpec, XFont>,
    /// Pixels X11 déjà alloués, notamment pour éviter des milliers d'aller-retours
    /// lors du rafraîchissement animé des icônes PNG.
    color_cache: RefCell<HashMap<u32, u32>>,
    /// One bounded server-side cache for the large desktop background.
    bitmap_cache: RefCell<Option<BitmapCache>>,
    /// Rendu vectoriel antialiasé Xft/Fontconfig, si les bibliothèques hôte sont présentes.
    xft: Option<XftRenderer>,
    /// Facteur DPI appliqué aux coordonnées logiques Win32.
    scale: f32,
    /// Table de correspondance keycode X11 → keysyms (lue à la demande).
    keymap: HashMap<u8, Vec<u32>>,
    keyboard: Option<XkbKeyboard>,
    /// Tampon arrière de chaque fenêtre : la peinture y est composée puis
    /// recopiée d'un bloc, sans scintillement, et les Expose sont servis
    /// sans rejouer les commandes GDI.
    back: HashMap<usize, (u32, u16, u16)>,
    /// Requêtes de dessin x11rb pas encore vues par la connexion Xlib (Xft).
    x11_dirty: Cell<bool>,
    /// Glyphes Xft pas encore confirmés par le serveur.
    xft_dirty: Cell<bool>,
    /// Curseurs X11 pré-créés, indexés par handle Win32 (0x0001_0001..0x0001_000A).
    x11_cursors: HashMap<usize, u32>,
    /// Dernier handle de curseur Win32 appliqué à la fenêtre X11.
    current_cursor: usize,
    /// Overlay de performances (Paramètres > Graphismes).
    overlay: crate::overlay::Overlay,
    /// Taille et couleur du pointeur (réglages d'accessibilité appliqués).
    pointer_style: (u8, u8),
}

impl X11Backend {
    /// Ouvre la connexion X et prépare les atomes; `None` si pas de serveur.
    pub fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let setup = conn.setup();
        let scr = &setup.roots[screen];
        let colormap = scr.default_colormap;
        let dpi_x = if scr.width_in_millimeters > 0 {
            scr.width_in_pixels as f32 * 25.4 / scr.width_in_millimeters as f32
        } else {
            96.0
        };
        let requested_scale = std::env::var("WINLUX_UI_SCALE")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value > 0.0);
        let scale = dpi_scale(dpi_x, requested_scale);
        let wm_protocols = conn
            .intern_atom(false, b"WM_PROTOCOLS")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let wm_delete = conn
            .intern_atom(false, b"WM_DELETE_WINDOW")
            .ok()?
            .reply()
            .ok()?
            .atom;
        Some(X11Backend {
            conn,
            screen,
            colormap,
            wm_protocols,
            wm_delete,
            windows: HashMap::new(),
            by_xwin: HashMap::new(),
            pending: Vec::new(),
            deferred_events: Vec::new(),
            content: HashMap::new(),
            fonts: HashMap::new(),
            color_cache: RefCell::new(HashMap::new()),
            bitmap_cache: RefCell::new(None),
            xft: unsafe { XftRenderer::connect() },
            scale,
            keymap: HashMap::new(),
            keyboard: unsafe { XkbKeyboard::connect() },
            back: HashMap::new(),
            overlay: crate::overlay::Overlay::new(),
            x11_dirty: Cell::new(false),
            xft_dirty: Cell::new(false),
            x11_cursors: HashMap::new(),
            current_cursor: 0x0001_0001,
            pointer_style: (1, 0),
        })
    }

    fn create_font_cursor(&self, glyph: u16) -> Option<u32> {
        let font = self.conn.generate_id().ok()?;
        self.conn.open_font(font, b"cursor").ok()?.check().ok()?;
        let cursor = self.conn.generate_id().ok()?;
        let created = self.conn.create_glyph_cursor(
            cursor,
            font,
            font,
            glyph,
            glyph + 1,
            u16::MAX,
            u16::MAX,
            u16::MAX,
            0,
            0,
            0,
        );
        let _ = self.conn.close_font(font);
        created.ok()?.check().ok()?;
        Some(cursor)
    }

    fn init_cursors(&mut self) {
        let a11y = crate::dlls::a11y::current();
        self.pointer_style = (a11y.pointer_size, a11y.pointer_color);
        let arrow = self.arrow_cursor().or_else(|| self.create_font_cursor(68));
        if let Some(c) = arrow {
            self.x11_cursors.insert(0x0001_0001, c); // IDC_ARROW
        }
        // IDC_IBEAM → xterm (glyph 152)
        if let Some(c) = self.create_font_cursor(152) {
            self.x11_cursors.insert(0x0001_0002, c);
        }
        // IDC_WAIT → watch (glyph 150)
        if let Some(c) = self.create_font_cursor(150) {
            self.x11_cursors.insert(0x0001_0003, c);
        }
        // IDC_CROSS → crosshair (glyph 34)
        if let Some(c) = self.create_font_cursor(34) {
            self.x11_cursors.insert(0x0001_0004, c);
        }
        // IDC_SIZENWSE → top_left_corner (glyph 134)
        if let Some(c) = self.create_font_cursor(134) {
            self.x11_cursors.insert(0x0001_0005, c);
        }
        // IDC_SIZENESW → bottom_left_corner (glyph 12)
        if let Some(c) = self.create_font_cursor(12) {
            self.x11_cursors.insert(0x0001_0006, c);
        }
        // IDC_SIZEWE → sb_h_double_arrow (glyph 108)
        if let Some(c) = self.create_font_cursor(108) {
            self.x11_cursors.insert(0x0001_0007, c);
        }
        // IDC_SIZENS → sb_v_double_arrow (glyph 116)
        if let Some(c) = self.create_font_cursor(116) {
            self.x11_cursors.insert(0x0001_0008, c);
        }
        // IDC_SIZEALL → fleur (glyph 52)
        if let Some(c) = self.create_font_cursor(52) {
            self.x11_cursors.insert(0x0001_0009, c);
        }
        // IDC_HAND → hand2 (glyph 60)
        if let Some(c) = self.create_font_cursor(60) {
            self.x11_cursors.insert(0x0001_000A, c);
        }
    }

    /// Reconstruit la flèche quand la taille ou la couleur du pointeur change.
    fn refresh_pointer_style(&mut self) {
        if self.x11_cursors.is_empty() {
            return;
        }
        let a11y = crate::dlls::a11y::current();
        let style = (a11y.pointer_size, a11y.pointer_color);
        if style == self.pointer_style {
            return;
        }
        self.pointer_style = style;
        if let Some(arrow) = self.arrow_cursor() {
            if let Some(old) = self.x11_cursors.insert(0x0001_0001, arrow) {
                let _ = self.conn.free_cursor(old);
            }
            // Force la réapplication du curseur courant aux fenêtres.
            self.current_cursor = 0;
        }
    }

    fn apply_cursor(&mut self) {
        self.refresh_pointer_style();
        let handle = gui::CURSOR_HANDLE.load(std::sync::atomic::Ordering::Relaxed);
        if handle == self.current_cursor {
            return;
        }
        self.current_cursor = handle;
        if let Some(&xcursor) = self.x11_cursors.get(&handle) {
            for xwin in self.windows.values() {
                let aux = ChangeWindowAttributesAux::new().cursor(xcursor);
                let _ = self.conn.change_window_attributes(xwin.window, &aux);
            }
            let _ = self.conn.flush();
        }
    }

    /// Construit le curseur flèche (contour noir, intérieur blanc) à partir
    /// d'un motif 12×19 agrandi selon l'échelle d'interface.
    fn arrow_cursor(&self) -> Option<u32> {
        const ARROW: [&[u8; 12]; 19] = [
            b"X           ",
            b"XX          ",
            b"XoX         ",
            b"XooX        ",
            b"XoooX       ",
            b"XooooX      ",
            b"XoooooX     ",
            b"XooooooX    ",
            b"XoooooooX   ",
            b"XooooooooX  ",
            b"XoooooooooX ",
            b"XooooooXXXXX",
            b"XoooXooX    ",
            b"XooXXooX    ",
            b"XoX  XooX   ",
            b"XX   XooX   ",
            b"X     XooX  ",
            b"      XooX  ",
            b"       XX   ",
        ];
        // Taille d'accessibilité : 1×, 1,5×, 2×, 2,5× ou 3× la flèche.
        let size = self.pointer_style.0.clamp(1, 5) as f32;
        let scale = self.scale.max(1.0) * (1.0 + (size - 1.0) * 0.5);
        let width = (12.0 * scale).ceil() as u16;
        let height = (19.0 * scale).ceil() as u16;
        let setup = self.conn.setup();
        let pad = setup
            .pixmap_formats
            .iter()
            .find(|f| f.depth == 1)
            .map(|f| f.scanline_pad as usize)
            .unwrap_or(32);
        let lsb_first =
            setup.bitmap_format_bit_order == x11rb::protocol::xproto::ImageOrder::LSB_FIRST;
        let stride = (width as usize).div_ceil(pad) * pad / 8;
        let mut source = vec![0u8; stride * height as usize];
        let mut mask = vec![0u8; stride * height as usize];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let row = ARROW[((y as f32 / scale) as usize).min(18)];
                let cell = row[((x as f32 / scale) as usize).min(11)];
                let bit = if lsb_first {
                    1u8 << (x % 8)
                } else {
                    0x80u8 >> (x % 8)
                };
                let at = y * stride + x / 8;
                if cell != b' ' {
                    mask[at] |= bit;
                }
                if cell == b'o' {
                    source[at] |= bit;
                }
            }
        }
        let root = setup.roots[self.screen].root;
        let mut pixmaps = [0u32; 2];
        for (slot, data) in [&source, &mask].into_iter().enumerate() {
            let pixmap = self.conn.generate_id().ok()?;
            self.conn
                .create_pixmap(1, pixmap, root, width, height)
                .ok()?;
            let gc = self.conn.generate_id().ok()?;
            self.conn
                .create_gc(gc, pixmap, &x11rb::protocol::xproto::CreateGCAux::new())
                .ok()?;
            self.conn
                .put_image(
                    x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                    pixmap,
                    gc,
                    width,
                    height,
                    0,
                    0,
                    0,
                    1,
                    data,
                )
                .ok()?;
            let _ = self.conn.free_gc(gc);
            pixmaps[slot] = pixmap;
        }
        // Intérieur / contour : blanc, noir ou vert vif (bien visible).
        let ((fr, fg, fb), (br, bg, bb)) = match self.pointer_style.1 {
            1 => ((0, 0, 0), (u16::MAX, u16::MAX, u16::MAX)),
            2 => ((0xC8C8, u16::MAX, 0), (0, 0, 0)),
            _ => ((u16::MAX, u16::MAX, u16::MAX), (0, 0, 0)),
        };
        let cursor = self.conn.generate_id().ok()?;
        let created = self
            .conn
            .create_cursor(cursor, pixmaps[0], pixmaps[1], fr, fg, fb, br, bg, bb, 0, 0);
        for p in pixmaps {
            let _ = self.conn.free_pixmap(p);
        }
        created.ok()?.check().ok()?;
        Some(cursor)
    }

    /// Lit un drawable X11 (fenêtre, pixmap ou racine) en pixels COLORREF.
    fn read_drawable(
        &self,
        drawable: u32,
        area: x11rb::protocol::xproto::Rectangle,
    ) -> Option<BitmapSnapshot> {
        let setup = self.conn.setup();
        let screen = &setup.roots[self.screen];
        let depth = screen.root_depth;
        let masks = screen
            .allowed_depths
            .iter()
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.visual_id == screen.root_visual)
            .map(|v| (v.red_mask, v.green_mask, v.blue_mask))?;
        let format = setup.pixmap_formats.iter().find(|f| f.depth == depth)?;
        let bits_per_pixel = format.bits_per_pixel as usize;
        let pad = format.scanline_pad as usize;
        if bits_per_pixel == 0
            || bits_per_pixel > 32
            || !bits_per_pixel.is_multiple_of(8)
            || pad < 8
            || !pad.is_multiple_of(8)
        {
            return None;
        }
        let bytes_per_pixel = bits_per_pixel / 8;
        let stride = (area.width as usize * bits_per_pixel).div_ceil(pad) * (pad / 8);
        let image_len = stride.checked_mul(area.height as usize)?;
        if image_len > 256 * 1024 * 1024 {
            return None;
        }
        let image = self
            .conn
            .get_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                drawable,
                area.x,
                area.y,
                area.width,
                area.height,
                u32::MAX,
            )
            .ok()?
            .reply()
            .ok()?
            .data;
        if image.len() < image_len {
            return None;
        }
        let mut pixels = vec![0; area.width as usize * area.height as usize];
        for y in 0..area.height as usize {
            for x in 0..area.width as usize {
                let start = y * stride + x * bytes_per_pixel;
                let mut pixel = 0u32;
                match setup.image_byte_order {
                    x11rb::protocol::xproto::ImageOrder::LSB_FIRST => {
                        for i in 0..bytes_per_pixel {
                            pixel |= (image[start + i] as u32) << (8 * i);
                        }
                    }
                    _ => {
                        for i in 0..bytes_per_pixel {
                            pixel = (pixel << 8) | image[start + i] as u32;
                        }
                    }
                }
                pixels[y * area.width as usize + x] = pixel_to_colorref(pixel, masks);
            }
        }
        Some(BitmapSnapshot {
            width: area.width as usize,
            height: area.height as usize,
            pixels,
        })
    }

    fn scaled(&self, value: i32) -> i32 {
        (value as f32 * self.scale).round() as i32
    }

    fn logical(&self, value: i32) -> i32 {
        (value as f32 / self.scale).round() as i32
    }

    fn scaled_rect(&self, rect: &Rect) -> Rect {
        Rect {
            left: self.scaled(rect.left),
            top: self.scaled(rect.top),
            right: self.scaled(rect.right),
            bottom: self.scaled(rect.bottom),
        }
    }

    fn scaled_font(&self, spec: &FontSpec) -> FontSpec {
        let mut scaled = spec.clone();
        scaled.height = self.scaled(spec.height).max(1);
        if spec.width != 0 {
            scaled.width = self.scaled(spec.width).max(1);
        }
        scaled
    }

    /// Alloue un pixel vrai-color depuis un COLORREF du thème.
    fn alloc_color(&self, colorref: u32) -> u32 {
        if let Some(pixel) = self.color_cache.borrow().get(&colorref).copied() {
            return pixel;
        }
        let rgb = theme::colorref_to_rgb(colorref);
        let r = (((rgb >> 16) & 0xFF) as u16) * 257;
        let g = (((rgb >> 8) & 0xFF) as u16) * 257;
        let b = ((rgb & 0xFF) as u16) * 257;
        let pixel = match self.conn.alloc_color(self.colormap, r, g, b) {
            Ok(reply) => match reply.reply() {
                Ok(c) => c.pixel,
                Err(_) => rgb,
            },
            Err(_) => rgb,
        };
        self.color_cache.borrow_mut().insert(colorref, pixel);
        pixel
    }

    /// Les glyphes Xft passent par une seconde connexion : avant de reprendre
    /// le dessin x11rb, on attend qu'ils soient posés.
    fn before_x11(&self) {
        if self.xft_dirty.replace(false) {
            if let Some(xft) = self.xft.as_ref() {
                xft.sync();
            }
        }
        self.x11_dirty.set(true);
    }

    /// Inverse de `before_x11` : un seul aller-retour pour toute une suite de
    /// textes, au lieu de deux par texte.
    fn before_xft(&self) {
        if self.x11_dirty.replace(false) {
            let _ = self.conn.flush();
            if let Ok(cookie) = self.conn.get_input_focus() {
                let _ = cookie.reply();
            }
        }
        self.xft_dirty.set(true);
    }

    /// Tampon arrière à la taille actuelle de la fenêtre (recréé si besoin).
    fn back_buffer(&mut self, hwnd: usize, w: &XWin) -> Option<u32> {
        if let Some(&(pixmap, width, height)) = self.back.get(&hwnd) {
            if width == w.width && height == w.height {
                return Some(pixmap);
            }
            let _ = self.conn.free_pixmap(pixmap);
            self.back.remove(&hwnd);
        }
        if std::env::var_os("WINLUX_NO_BACKBUFFER").is_some_and(|v| !v.is_empty()) {
            return None;
        }
        let depth = self.conn.setup().roots[self.screen].root_depth;
        let pixmap = self.conn.generate_id().ok()?;
        self.conn
            .create_pixmap(depth, pixmap, w.window, w.width.max(1), w.height.max(1))
            .ok()?
            .check()
            .ok()?;
        self.back.insert(hwnd, (pixmap, w.width, w.height));
        Some(pixmap)
    }

    /// Pose l'overlay de performances dans un coin de la fenêtre.
    fn paint_overlay(&mut self, w: &XWin, hwnd: usize) {
        let lines = self.overlay.lines(hwnd);
        if lines.is_empty() {
            return;
        }
        let line_h = self.scaled(16).max(12);
        let pad = self.scaled(8).max(4);
        let spec = FontSpec {
            height: self.scaled(13).max(10),
            weight: 600,
            face: "Consolas".into(),
            ..FontSpec::default()
        };
        let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32;
        let box_w = longest * spec.height * 66 / 100 + pad * 2;
        let box_h = lines.len() as i32 * line_h + pad * 2;
        let (ww, wh) = (w.width as i32, w.height as i32);
        let margin = self.scaled(10);
        let (left, top) = match self.overlay.position() {
            crate::overlay::Position::TopLeft => (margin, margin),
            crate::overlay::Position::TopRight => (ww - box_w - margin, margin),
            crate::overlay::Position::BottomLeft => (margin, wh - box_h - margin),
            crate::overlay::Position::BottomRight => (ww - box_w - margin, wh - box_h - margin),
        };
        let (left, top) = (left.max(0), top.max(0));
        self.before_x11();
        let border = Rect {
            left,
            top,
            right: left + box_w,
            bottom: top + box_h,
        };
        self.paint_fill(w, &border, 0x0060_5040);
        let inner = Rect {
            left: left + 1,
            top: top + 1,
            right: left + box_w - 1,
            bottom: top + box_h - 1,
        };
        self.paint_fill(w, &inner, 0x0020_1A16);
        for (i, line) in lines.iter().enumerate() {
            let color = if i == 0 { 0x0080_FF9A } else { 0x00F0_F0F0 };
            self.paint_text(
                w,
                left + pad,
                top + pad + i as i32 * line_h,
                line,
                color,
                None,
                &spec,
            );
        }
        self.before_x11();
    }

    /// Recopie une zone du tampon arrière vers la fenêtre.
    fn present(&self, w: &XWin, pixmap: u32, x: i16, y: i16, width: u16, height: u16) {
        self.before_x11();
        let _ = self
            .conn
            .copy_area(pixmap, w.window, w.gc, x, y, x, y, width, height);
        let _ = self.conn.flush();
    }

    /// Dessine un fond clair avec un dégradé discret, proche des surfaces
    /// d'application de Windows 11. Le contenu GDI invité est posé ensuite.
    fn paint_theme(&self, w: &XWin) {
        self.before_x11();
        let bands = 32u32;
        let h = w.height.max(1) as u32;
        for i in 0..bands {
            let t = i as f32 / (bands - 1) as f32;
            let c = theme::lerp_colorref(theme::GRADIENT_TOP, theme::GRADIENT_BOTTOM, t);
            let pixel = self.alloc_color(c);
            let _ = self
                .conn
                .change_gc(w.gc, &ChangeGCAux::new().foreground(pixel));
            let y = (i * h / bands) as i16;
            let band_h = ((i + 1) * h / bands - i * h / bands).max(1) as u16;
            let _ = self.conn.poly_fill_rectangle(
                w.window,
                w.gc,
                &[x11rb::protocol::xproto::Rectangle {
                    x: 0,
                    y,
                    width: w.width,
                    height: band_h,
                }],
            );
        }
        let _ = self.conn.flush();
    }

    /// Convertit un rectangle logique en rectangle X rogné à la fenêtre.
    fn to_x_rect(w: &XWin, r: &Rect) -> Option<x11rb::protocol::xproto::Rectangle> {
        let x0 = r.left.max(0).min(w.width as i32);
        let y0 = r.top.max(0).min(w.height as i32);
        let x1 = r.right.max(0).min(w.width as i32);
        let y1 = r.bottom.max(0).min(w.height as i32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(x11rb::protocol::xproto::Rectangle {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0) as u16,
            height: (y1 - y0) as u16,
        })
    }

    fn set_foreground(&self, w: &XWin, color: u32) {
        let pixel = self.alloc_color(color);
        let _ = self
            .conn
            .change_gc(w.gc, &ChangeGCAux::new().foreground(pixel));
    }

    /// Ouvre une police core de taille voisine; replie vers `fixed` si le
    /// serveur ne fournit pas l'alias demandé.
    fn ensure_font(&mut self, spec: &FontSpec) -> Option<XFont> {
        if let Some(font) = self.fonts.get(spec) {
            return Some(*font);
        }
        for name in font_candidates(spec) {
            let id = self.conn.generate_id().ok()?;
            let cookie = match self.conn.open_font(id, &name) {
                Ok(cookie) => cookie,
                Err(_) => continue,
            };
            if cookie.check().is_err() {
                continue;
            }
            let metrics = match self.conn.query_font(id) {
                Ok(cookie) => match cookie.reply() {
                    Ok(metrics) => metrics,
                    Err(_) => {
                        let _ = self.conn.close_font(id);
                        continue;
                    }
                },
                Err(_) => {
                    let _ = self.conn.close_font(id);
                    continue;
                }
            };
            let font = XFont {
                id,
                ascent: metrics.font_ascent.max(1),
                char_width: metrics.max_bounds.character_width.max(1),
            };
            self.fonts.insert(spec.clone(), font);
            return Some(font);
        }
        None
    }

    /// Keysyms X11 associés à un keycode (table de niveaux chargée à la demande).
    fn key_syms(&mut self, keycode: u8) -> Option<Vec<u32>> {
        if !self.keymap.contains_key(&keycode) {
            let reply = self
                .conn
                .get_keyboard_mapping(keycode, 1)
                .ok()?
                .reply()
                .ok()?;
            self.keymap.insert(keycode, reply.keysyms);
        }
        self.keymap.get(&keycode).cloned()
    }

    fn paint_fill(&self, w: &XWin, r: &Rect, color: u32) {
        if let Some(xr) = Self::to_x_rect(w, r) {
            self.set_foreground(w, color);
            let _ = self.conn.poly_fill_rectangle(w.window, w.gc, &[xr]);
        }
    }

    fn paint_frame(&self, w: &XWin, r: &Rect, color: u32, width: u16) {
        self.set_foreground(w, color);
        let t = width.max(1) as i32;
        let edges = [
            Rect {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.top + t,
            },
            Rect {
                left: r.left,
                top: r.bottom - t,
                right: r.right,
                bottom: r.bottom,
            },
            Rect {
                left: r.left,
                top: r.top + t,
                right: r.left + t,
                bottom: r.bottom - t,
            },
            Rect {
                left: r.right - t,
                top: r.top + t,
                right: r.right,
                bottom: r.bottom - t,
            },
        ];
        let xs: Vec<_> = edges.iter().filter_map(|e| Self::to_x_rect(w, e)).collect();
        if !xs.is_empty() {
            let _ = self.conn.poly_fill_rectangle(w.window, w.gc, &xs);
        }
    }

    fn paint_ellipse(&self, w: &XWin, r: &Rect, color: u32) {
        if let Some(xr) = Self::to_x_rect(w, r) {
            self.set_foreground(w, color);
            let arc = x11rb::protocol::xproto::Arc {
                x: xr.x,
                y: xr.y,
                width: xr.width,
                height: xr.height,
                angle1: 0,
                angle2: 360 * 64,
            };
            let _ = self.conn.poly_fill_arc(w.window, w.gc, &[arc]);
        }
    }

    fn paint_line(&self, w: &XWin, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        self.set_foreground(w, color);
        let p1 = x11rb::protocol::xproto::Point {
            x: x1 as i16,
            y: y1 as i16,
        };
        let p2 = x11rb::protocol::xproto::Point {
            x: x2 as i16,
            y: y2 as i16,
        };
        let _ = self.conn.poly_line(
            x11rb::protocol::xproto::CoordMode::ORIGIN,
            w.window,
            w.gc,
            &[p1, p2],
        );
    }

    /// Pose un texte ANSI. `bk` est la couleur de fond (`Some`) quand le mode
    /// de fond est opaque, ou `None` pour un fond transparent : seul le fond
    /// décide du primitive X11 employée (`image_text8` repeint le fond,
    /// `poly_text8` ne pose que les glyphes).
    fn paint_text(
        &mut self,
        w: &XWin,
        x: i32,
        y: i32,
        text: &str,
        color: u32,
        bk: Option<u32>,
        spec: &FontSpec,
    ) {
        let utf8 = text.as_bytes();
        if utf8.is_empty() {
            return;
        }
        let vector_metrics = self.xft.as_mut().and_then(|xft| xft.measure(spec, utf8));
        if let Some((width, height, ascent)) = vector_metrics {
            if let Some(background) = bk {
                self.before_x11();
                self.paint_fill(
                    w,
                    &Rect {
                        left: x,
                        top: y,
                        right: x + width.max(1),
                        bottom: y + height.max(1),
                    },
                    background,
                );
            }
            // Xft utilise une seconde connexion Xlib : les surfaces envoyées
            // par x11rb doivent être posées avant les glyphes.
            self.before_xft();
            let drawn = self
                .xft
                .as_mut()
                .map(|xft| xft.draw(w.window, spec, x, y + ascent, utf8, color))
                .unwrap_or(false);
            if drawn {
                if spec.underline || spec.strikeout {
                    self.before_x11();
                }
                if spec.underline {
                    self.paint_fill(
                        w,
                        &Rect {
                            left: x,
                            top: y + ascent + 1,
                            right: x + width.max(1),
                            bottom: y + ascent + 2,
                        },
                        color,
                    );
                }
                if spec.strikeout {
                    let middle = y + ascent / 2;
                    self.paint_fill(
                        w,
                        &Rect {
                            left: x,
                            top: middle,
                            right: x + width.max(1),
                            bottom: middle + 1,
                        },
                        color,
                    );
                }
                return;
            }
        }

        self.before_x11();
        let font = match self.ensure_font(spec) {
            Some(f) => f,
            None => return,
        };
        let pixel = self.alloc_color(color);
        let mut aux = ChangeGCAux::new().foreground(pixel).font(font.id);
        let opaque = match bk {
            Some(bg) => {
                aux = aux.background(self.alloc_color(bg));
                true
            }
            None => false,
        };
        let _ = self.conn.change_gc(w.gc, &aux);
        // `y` désigne le haut logique ; l'ascendant du serveur donne la ligne de base.
        let mut bytes = text.as_bytes();
        if bytes.len() > 255 {
            bytes = &bytes[..255];
        }
        if bytes.is_empty() {
            return;
        }
        let px = x.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let py = (y + font.ascent as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        if opaque {
            let _ = self.conn.image_text8(w.window, w.gc, px, py, bytes);
        } else {
            // Élément TEXT8 : nombre d'octets, puis déplacement (0) et glyphes.
            // Le serveur X attend `n` avant `delta` ; le tampon est complété à
            // un multiple de 4 octets (éléments vides ignorés).
            let mut item = Vec::with_capacity(bytes.len() + 4);
            item.push(bytes.len() as u8);
            item.push(0u8);
            item.extend_from_slice(bytes);
            while item.len() % 4 != 0 {
                item.push(0);
            }
            let _ = self.conn.poly_text8(w.window, w.gc, px, py, &item);
        }
        if spec.underline || spec.strikeout {
            let width =
                (bytes.len() as i32 * font.char_width as i32).clamp(1, u16::MAX as i32) as u16;
            let mut lines = Vec::new();
            if spec.underline {
                lines.push(x11rb::protocol::xproto::Rectangle {
                    x: px,
                    y: py.saturating_add(1),
                    width,
                    height: 1,
                });
            }
            if spec.strikeout {
                lines.push(x11rb::protocol::xproto::Rectangle {
                    x: px,
                    y: py.saturating_sub((font.ascent / 2).max(1)),
                    width,
                    height: 1,
                });
            }
            let _ = self.conn.poly_fill_rectangle(w.window, w.gc, &lines);
        }
    }

    fn paint_blit(
        &self,
        destination_window: &XWin,
        source_hwnd: usize,
        source_rect: &Rect,
        destination_rect: &Rect,
        stretch: bool,
        smooth: bool,
        rop: gui::RasterOp,
    ) {
        let Some(mut source_window) = self.windows.get(&source_hwnd).copied() else {
            return;
        };
        if let Some(&(pixmap, width, height)) = self.back.get(&source_hwnd) {
            if width == source_window.width && height == source_window.height {
                source_window.window = pixmap;
            }
        }
        let Some(src) = Self::to_x_rect(&source_window, source_rect) else {
            return;
        };
        let Some(dst) = Self::to_x_rect(destination_window, destination_rect) else {
            return;
        };

        if !stretch || (src.width == dst.width && src.height == dst.height) {
            let width = src.width.min(dst.width);
            let height = src.height.min(dst.height);
            let _ = self.conn.change_gc(
                destination_window.gc,
                &ChangeGCAux::new().function(x11_function(rop)),
            );
            let _ = self.conn.copy_area(
                source_window.window,
                destination_window.window,
                destination_window.gc,
                src.x,
                src.y,
                dst.x,
                dst.y,
                width,
                height,
            );
            let _ = self.conn.change_gc(
                destination_window.gc,
                &ChangeGCAux::new().function(x11rb::protocol::xproto::GX::COPY),
            );
            return;
        }

        self.paint_stretched_pixels(
            source_window.window,
            destination_window,
            src,
            dst,
            smooth,
            rop,
        );
    }

    /// Rééchantillonnage des pixels ZPixmap (formats 8/16/24/32bpp) :
    /// plus proche voisin, ou bilinéaire (mode HALFTONE) pour la loupe.
    fn paint_stretched_pixels(
        &self,
        source: Window,
        destination: &XWin,
        src: x11rb::protocol::xproto::Rectangle,
        dst: x11rb::protocol::xproto::Rectangle,
        smooth: bool,
        rop: gui::RasterOp,
    ) {
        let depth = self.conn.setup().roots[self.screen].root_depth;
        let Some(format) = self
            .conn
            .setup()
            .pixmap_formats
            .iter()
            .find(|f| f.depth == depth)
        else {
            return;
        };
        let bits_per_pixel = format.bits_per_pixel as usize;
        let pad = format.scanline_pad as usize;
        if bits_per_pixel == 0
            || bits_per_pixel > 32
            || !bits_per_pixel.is_multiple_of(8)
            || pad < 8
            || !pad.is_multiple_of(8)
        {
            return;
        }
        let bytes_per_pixel = bits_per_pixel / 8;
        let stride = |width: usize| (width * bits_per_pixel).div_ceil(pad) * (pad / 8);
        let src_stride = stride(src.width as usize);
        let dst_stride = stride(dst.width as usize);
        // Une loupe plein écran 4K tient sous cette limite ; au-delà, refus.
        const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
        let Some(src_len) = src_stride.checked_mul(src.height as usize) else {
            return;
        };
        let Some(dst_len) = dst_stride.checked_mul(dst.height as usize) else {
            return;
        };
        if src_len > MAX_TOTAL_BYTES || dst_len > MAX_TOTAL_BYTES {
            return;
        }
        let image = match self.conn.get_image(
            x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
            source,
            src.x,
            src.y,
            src.width,
            src.height,
            u32::MAX,
        ) {
            Ok(cookie) => match cookie.reply() {
                Ok(image) => image.data,
                Err(_) => return,
            },
            Err(_) => return,
        };
        if image.len() < src_len {
            return;
        }
        let output = resample(
            &image,
            (src.width as usize, src.height as usize, src_stride),
            (dst.width as usize, dst.height as usize, dst_stride),
            bytes_per_pixel,
            smooth,
        );
        let _ = self.conn.change_gc(
            destination.gc,
            &ChangeGCAux::new().function(x11_function(rop)),
        );
        // PutImage doit tenir dans une requête X11 de base : envoi par bandes.
        const MAX_REQUEST_BYTES: usize = 256 * 1024 - 64;
        let rows = (MAX_REQUEST_BYTES / dst_stride.max(1)).max(1);
        let mut row = 0usize;
        while row < dst.height as usize {
            let count = rows.min(dst.height as usize - row);
            let _ = self.conn.put_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                destination.window,
                destination.gc,
                dst.width,
                count as u16,
                dst.x,
                dst.y.saturating_add(row as i16),
                0,
                depth,
                &output[row * dst_stride..(row + count) * dst_stride],
            );
            row += count;
        }
        let _ = self.conn.change_gc(
            destination.gc,
            &ChangeGCAux::new().function(x11rb::protocol::xproto::GX::COPY),
        );
    }

    fn paint_bitmap(
        &self,
        window: &XWin,
        source: &Rect,
        destination: &Rect,
        pixels: &[u32],
        rop: gui::RasterOp,
    ) {
        let Some(dst) = Self::to_x_rect(window, destination) else {
            return;
        };
        let source_width = source.width();
        let source_height = source.height();
        let destination_width = destination.width();
        let destination_height = destination.height();
        if source_width <= 0 || source_height <= 0 {
            return;
        }
        let Some(source_count) = (source_width as usize).checked_mul(source_height as usize) else {
            return;
        };
        if pixels.len() < source_count {
            return;
        }
        let setup = self.conn.setup();
        let depth = setup.roots[self.screen].root_depth;
        let Some(format) = setup.pixmap_formats.iter().find(|f| f.depth == depth) else {
            return;
        };
        let bits_per_pixel = format.bits_per_pixel as usize;
        let pad = format.scanline_pad as usize;
        if bits_per_pixel == 0
            || bits_per_pixel > 32
            || !bits_per_pixel.is_multiple_of(8)
            || pad < 8
            || !pad.is_multiple_of(8)
        {
            return;
        }
        let bytes_per_pixel = bits_per_pixel / 8;
        let stride = |width: usize| (width * bits_per_pixel).div_ceil(pad) * (pad / 8);
        let row_bytes = stride(dst.width as usize);
        let Some(image_len) = row_bytes.checked_mul(dst.height as usize) else {
            return;
        };
        // PutImage must fit in a core X11 request. Large desktop images are
        // uploaded in whole-row bands instead of being silently discarded.
        const MAX_IMAGE_BYTES: usize = 256 * 1024 - 64;
        let rows_per_request = MAX_IMAGE_BYTES / row_bytes.max(1);
        if rows_per_request == 0 {
            return;
        }
        let key = [
            source_width,
            source_height,
            destination_width,
            destination_height,
            dst.x as i32 - destination.left,
            dst.y as i32 - destination.top,
            dst.width as i32,
            dst.height as i32,
        ];
        let cacheable = image_len > MAX_IMAGE_BYTES && matches!(rop, gui::RasterOp::Copy);
        if cacheable {
            if let Some(cached) = self
                .bitmap_cache
                .borrow()
                .as_ref()
                .filter(|c| c.key == key && c.pixels == pixels)
            {
                let _ = self.conn.copy_area(
                    cached.pixmap,
                    window.window,
                    window.gc,
                    0,
                    0,
                    dst.x,
                    dst.y,
                    dst.width,
                    dst.height,
                );
                return;
            }
        }
        let visual = setup.roots[self.screen]
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .find(|v| {
                v.visual_id == setup.roots[self.screen].root_visual
                    && v.class == x11rb::protocol::xproto::VisualClass::TRUE_COLOR
            });
        let mut image = vec![0u8; image_len];
        let mut color_pixels = HashMap::new();
        for y in 0..dst.height as usize {
            for x in 0..dst.width as usize {
                let destination_x = dst.x as i32 + x as i32 - destination.left;
                let destination_y = dst.y as i32 + y as i32 - destination.top;
                let source_x = (destination_x as f32 + 0.5) * source_width as f32
                    / destination_width as f32
                    - 0.5;
                let source_y = (destination_y as f32 + 0.5) * source_height as f32
                    / destination_height as f32
                    - 0.5;
                let colorref = bilinear_colorref(
                    pixels,
                    source_width as usize,
                    source_height as usize,
                    source_x,
                    source_y,
                );
                // TrueColor pixels are encoded using the visual masks. Asking
                // the server to allocate every photo colour stalls the UI.
                let pixel = if let Some(v) = visual {
                    truecolor_pixel(colorref, v.red_mask, v.green_mask, v.blue_mask)
                } else {
                    *color_pixels
                        .entry(colorref)
                        .or_insert_with(|| self.alloc_color(colorref))
                };
                let pixel_bytes = match setup.image_byte_order {
                    x11rb::protocol::xproto::ImageOrder::LSB_FIRST => pixel.to_le_bytes(),
                    _ => pixel.to_be_bytes(),
                };
                let offset = y * row_bytes + x * bytes_per_pixel;
                let bytes =
                    if setup.image_byte_order == x11rb::protocol::xproto::ImageOrder::LSB_FIRST {
                        &pixel_bytes[..bytes_per_pixel]
                    } else {
                        &pixel_bytes[4 - bytes_per_pixel..]
                    };
                image[offset..offset + bytes_per_pixel].copy_from_slice(bytes);
            }
        }
        let _ = self
            .conn
            .change_gc(window.gc, &ChangeGCAux::new().function(x11_function(rop)));
        let cache_pixmap = if cacheable {
            self.conn.generate_id().ok().filter(|&id| {
                self.conn
                    .create_pixmap(depth, id, window.window, dst.width, dst.height)
                    .is_ok_and(|cookie| cookie.check().is_ok())
            })
        } else {
            None
        };
        for first_row in (0..dst.height as usize).step_by(rows_per_request) {
            let rows = rows_per_request.min(dst.height as usize - first_row);
            let _ = self.conn.put_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                cache_pixmap.unwrap_or(window.window),
                window.gc,
                dst.width,
                rows as u16,
                if cache_pixmap.is_some() { 0 } else { dst.x },
                (if cache_pixmap.is_some() {
                    0
                } else {
                    dst.y as i32
                } + first_row as i32) as i16,
                0,
                depth,
                &image[first_row * row_bytes..(first_row + rows) * row_bytes],
            );
        }
        if let Some(pixmap) = cache_pixmap {
            let _ = self.conn.copy_area(
                pixmap,
                window.window,
                window.gc,
                0,
                0,
                dst.x,
                dst.y,
                dst.width,
                dst.height,
            );
            let old = self.bitmap_cache.replace(Some(BitmapCache {
                key,
                pixels: pixels.to_vec(),
                pixmap,
            }));
            if let Some(old) = old {
                let _ = self.conn.free_pixmap(old.pixmap);
            }
        }
        let _ = self.conn.change_gc(
            window.gc,
            &ChangeGCAux::new().function(x11rb::protocol::xproto::GX::COPY),
        );
    }

    fn paint_pattern(&self, window: &XWin, rect: &Rect, color: u32, rop: gui::RasterOp) {
        if let Some(xr) = Self::to_x_rect(window, rect) {
            self.set_foreground(window, color);
            let _ = self
                .conn
                .change_gc(window.gc, &ChangeGCAux::new().function(x11_function(rop)));
            let _ = self
                .conn
                .poly_fill_rectangle(window.window, window.gc, &[xr]);
            let _ = self.conn.change_gc(
                window.gc,
                &ChangeGCAux::new().function(x11rb::protocol::xproto::GX::COPY),
            );
        }
    }

    /// Rejoue la liste de commandes GDI d'une fenêtre.
    fn smooth_shape(&mut self, w: &XWin, color: u32, shape: Shape) -> bool {
        if !self.xft.as_ref().is_some_and(|r| r.cairo.is_some()) {
            return false;
        }
        self.before_xft();
        let renderer = self.xft.as_mut().unwrap();
        unsafe {
            renderer.cairo.as_mut().unwrap().draw(
                renderer.display.cast(),
                renderer.visual.cast(),
                w.window,
                w.width as i32,
                w.height as i32,
                color,
                shape,
            )
        }
    }

    fn paint_cmds(&mut self, w: &XWin, cmds: &[DrawCmd]) {
        for cmd in cmds {
            if !matches!(
                cmd,
                DrawCmd::Text { .. }
                    | DrawCmd::Ellipse { .. }
                    | DrawCmd::Line { .. }
                    | DrawCmd::RoundRect { .. }
            ) {
                self.before_x11();
            }
            match cmd {
                DrawCmd::FillRect { rect, color } => {
                    let rect = self.scaled_rect(rect);
                    self.paint_fill(w, &rect, *color)
                }
                DrawCmd::FrameRect { rect, color, width } => {
                    let rect = self.scaled_rect(rect);
                    self.paint_frame(w, &rect, *color, self.scaled(*width as i32).max(1) as u16)
                }
                DrawCmd::Ellipse { rect, color } => {
                    let rect = self.scaled_rect(rect);
                    if !self.smooth_shape(
                        w,
                        *color,
                        Shape::Ellipse {
                            x: rect.left as f64,
                            y: rect.top as f64,
                            width: rect.width() as f64,
                            height: rect.height() as f64,
                        },
                    ) {
                        self.before_x11();
                        self.paint_ellipse(w, &rect, *color);
                    }
                }
                DrawCmd::PatternRect { rect, color, rop } => {
                    let rect = self.scaled_rect(rect);
                    self.paint_pattern(w, &rect, *color, *rop)
                }
                DrawCmd::Text {
                    x,
                    y,
                    text,
                    color,
                    bk,
                    font,
                } => {
                    let font = self.scaled_font(font);
                    self.paint_text(
                        w,
                        self.scaled(*x),
                        self.scaled(*y),
                        text,
                        *color,
                        *bk,
                        &font,
                    )
                }
                DrawCmd::Blit {
                    source_hwnd,
                    source,
                    destination,
                    stretch,
                    smooth,
                    rop,
                } => {
                    let source = self.scaled_rect(source);
                    let destination = self.scaled_rect(destination);
                    self.paint_blit(
                        w,
                        *source_hwnd,
                        &source,
                        &destination,
                        *stretch,
                        *smooth,
                        *rop,
                    )
                }
                DrawCmd::Bitmap { rect, pixels, rop } => {
                    let destination = self.scaled_rect(rect);
                    self.paint_bitmap(w, rect, &destination, pixels, *rop)
                }
                DrawCmd::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color,
                    width,
                } => {
                    let (x1, y1, x2, y2) = (
                        self.scaled(*x1),
                        self.scaled(*y1),
                        self.scaled(*x2),
                        self.scaled(*y2),
                    );
                    if !self.smooth_shape(
                        w,
                        *color,
                        Shape::Line {
                            x1: x1 as f64,
                            y1: y1 as f64,
                            x2: x2 as f64,
                            y2: y2 as f64,
                            width: self.scaled(*width as i32) as f64,
                        },
                    ) {
                        self.before_x11();
                        self.paint_line(w, x1, y1, x2, y2, *color);
                    }
                }
                DrawCmd::RoundRect {
                    rect,
                    color,
                    rx,
                    ry,
                } => {
                    let rect = self.scaled_rect(rect);
                    let rx = self.scaled(*rx);
                    let ry = self.scaled(*ry);
                    if !self.smooth_shape(
                        w,
                        *color,
                        Shape::RoundRect {
                            x: rect.left as f64,
                            y: rect.top as f64,
                            width: rect.width() as f64,
                            height: rect.height() as f64,
                            rx: rx as f64,
                            ry: ry as f64,
                        },
                    ) {
                        self.before_x11();
                        self.paint_fill(w, &rect, *color);
                    }
                }
            }
        }
    }

    fn translate(&mut self, ev: Event) -> Option<PendingEvent> {
        match ev {
            Event::Expose(e) => {
                let hwnd = *self.by_xwin.get(&e.window)?;
                let buffered = self.windows.get(&hwnd).copied().and_then(|w| {
                    let &(pixmap, width, height) = self.back.get(&hwnd)?;
                    (width == w.width && height == w.height).then_some((w, pixmap))
                });
                if let Some((w, pixmap)) = buffered {
                    self.present(&w, pixmap, e.x as i16, e.y as i16, e.width, e.height);
                    // Le contenu est déjà restauré depuis le tampon serveur.
                    // Ne pas refaire tout le dessin GDI lors d'un déplacement
                    // ou du passage d'une autre fenêtre devant celle-ci.
                    return None;
                } else if e.count == 0 {
                    // count > 0 : d'autres Expose suivent, on attend le dernier.
                    if let Some(w) = self.windows.get(&hwnd).copied() {
                        self.paint_theme(&w);
                        let content = self.content.get(&hwnd).cloned().unwrap_or_default();
                        self.paint_cmds(&w, &content);
                        let _ = self.conn.flush();
                    }
                }
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_PAINT,
                    wparam: 0,
                    lparam: 0,
                })
            }
            Event::ConfigureNotify(e) => self.on_configure(e),
            Event::ClientMessage(e) => {
                let hwnd = *self.by_xwin.get(&e.window)?;
                let data = e.data.as_data32();
                if e.type_ == self.wm_protocols && data[0] == self.wm_delete {
                    Some(PendingEvent {
                        hwnd,
                        message: gui::WM_CLOSE,
                        wparam: 0,
                        lparam: 0,
                    })
                } else {
                    None
                }
            }
            Event::ButtonPress(e) => {
                crate::power::note_activity();
                gui::raw_mouse_button(e.detail, true);
                let hwnd = *self.by_xwin.get(&e.event)?;
                let x = self.logical(e.event_x as i32);
                let y = self.logical(e.event_y as i32);
                if std::env::var_os("WINLUX_INPUT_DIAG").is_some_and(|v| !v.is_empty()) {
                    eprintln!("Winlux X11 button {} at {x},{y}", e.detail);
                }
                let lparam = ((y as isize) << 16) | (x as isize & 0xFFFF);
                if let Some((message, delta)) = wheel_message(e.detail) {
                    // La molette Win32 donne des coordonnées écran.
                    let lparam =
                        pack_i16_pair(self.logical(e.root_x as i32), self.logical(e.root_y as i32));
                    let wparam = ((delta as u16 as usize) << 16) | button_state(e.state);
                    return Some(PendingEvent {
                        hwnd,
                        message,
                        wparam,
                        lparam,
                    });
                }
                let wparam = button_wparam(e.detail) | button_state(e.state);
                Some(PendingEvent {
                    hwnd,
                    message: button_message(e.detail, true),
                    wparam,
                    lparam,
                })
            }
            Event::ButtonRelease(e) => {
                gui::raw_mouse_button(e.detail, false);
                let hwnd = *self.by_xwin.get(&e.event)?;
                let x = self.logical(e.event_x as i32);
                let y = self.logical(e.event_y as i32);
                if wheel_message(e.detail).is_some() {
                    return None;
                }
                let lparam = ((y as isize) << 16) | (x as isize & 0xFFFF);
                let wparam = button_state(e.state) & !button_wparam(e.detail);
                Some(PendingEvent {
                    hwnd,
                    message: button_message(e.detail, false),
                    wparam,
                    lparam,
                })
            }
            Event::MotionNotify(e) => {
                crate::power::note_activity();
                gui::raw_mouse_motion(e.root_x as i32, e.root_y as i32);
                let hwnd = *self.by_xwin.get(&e.event)?;
                let x = self.logical(e.event_x as i32);
                let y = self.logical(e.event_y as i32);
                let lparam = ((y as isize) << 16) | (x as isize & 0xFFFF);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_MOUSEMOVE,
                    wparam: button_state(e.state),
                    lparam,
                })
            }
            Event::FocusIn(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_SETFOCUS,
                    wparam: 0,
                    lparam: 0,
                })
            }
            Event::FocusOut(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_KILLFOCUS,
                    wparam: 0,
                    lparam: 0,
                })
            }
            Event::KeyPress(e) => {
                crate::power::note_activity();
                gui::raw_key(e.detail, true);
                let hwnd = *self.by_xwin.get(&e.event)?;
                if std::env::var_os("WINLUX_INPUT_DIAG").is_some_and(|v| !v.is_empty()) {
                    eprintln!("Winlux X11 keycode {}", e.detail);
                }
                let shift = e.state.contains(x11rb::protocol::xproto::KeyButMask::SHIFT);
                let ctrl = e
                    .state
                    .contains(x11rb::protocol::xproto::KeyButMask::CONTROL);
                let caps = e.state.contains(x11rb::protocol::xproto::KeyButMask::LOCK);
                let syms = self.key_syms(e.detail);
                let base = syms.as_deref().and_then(base_keysym);
                let xkb = self
                    .keyboard
                    .as_ref()
                    .and_then(|k| k.translate(e.detail, e.state.into()));
                let level3 = e.state.contains(x11rb::protocol::xproto::KeyButMask::MOD5);
                let sym = xkb.map(|(_, _, sym, _)| sym).or_else(|| {
                    syms.as_deref()
                        .and_then(|s| effective_keysym(s, shift, caps, level3))
                });
                // Le VK désigne la touche physique (niveau non modifié), tandis
                // que le caractère tient compte de Maj/Caps Lock.
                let vk = xkb
                    .and_then(|(base, shifted, _, _)| key_levels_to_vk(base, shifted))
                    .or_else(|| {
                        syms.as_deref().and_then(|s| {
                            key_levels_to_vk(base.unwrap_or(0), s.get(1).copied().unwrap_or(0))
                        })
                    })
                    .unwrap_or(e.detail as usize);
                let alt = e.state.contains(x11rb::protocol::xproto::KeyButMask::MOD1);
                if self.overlay.hotkey(vk, ctrl, shift, alt) {
                    // Raccourci de l'overlay : consommé, jamais transmis au jeu.
                    if let Some(cmds) = self.content.get(&hwnd).cloned() {
                        gui::GuiBackend::draw(self, hwnd, &cmds);
                    }
                    return None;
                }
                let extended = vk_is_extended(vk);
                let lparam = key_lparam(e.detail, false, extended);
                gui::update_async_key_state(vk, true);
                // Caractère effectif pour `TranslateMessage` ; Ctrl+lettre ne
                // produit pas de caractère (raccourci clavier).
                // AltGr peut être encodé Ctrl+Alt par le serveur : XKB indique
                // les modificateurs consommés pour le caractère de niveau 3.
                let altgr = level3
                    || xkb.is_some_and(|(_, _, _, consumed)| {
                        consumed & 0x84 != 0
                            && e.state.contains(x11rb::protocol::xproto::KeyButMask::MOD1)
                    });
                let ch = if ctrl && !altgr {
                    None
                } else {
                    sym.map(keysym_to_char).filter(|c| *c != 0)
                };
                gui::set_key_char(hwnd, vk, lparam, ch);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_KEYDOWN,
                    wparam: vk,
                    lparam,
                })
            }
            Event::KeyRelease(e) => {
                gui::raw_key(e.detail, false);
                let hwnd = *self.by_xwin.get(&e.event)?;
                let syms = self.key_syms(e.detail);
                let vk = self
                    .keyboard
                    .as_ref()
                    .and_then(|k| k.translate(e.detail, e.state.into()))
                    .and_then(|(base, shifted, _, _)| key_levels_to_vk(base, shifted))
                    .or_else(|| {
                        syms.as_deref().and_then(|s| {
                            key_levels_to_vk(
                                s.first().copied().unwrap_or(0),
                                s.get(1).copied().unwrap_or(0),
                            )
                        })
                    })
                    .unwrap_or(e.detail as usize);
                gui::update_async_key_state(vk, false);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_KEYUP,
                    wparam: vk,
                    lparam: key_lparam(e.detail, true, vk_is_extended(vk)),
                })
            }
            Event::MappingNotify(_) => {
                self.keymap.clear();
                self.keyboard = unsafe { XkbKeyboard::connect() };
                None
            }
            _ => None,
        }
    }

    fn on_configure(&mut self, e: ConfigureNotifyEvent) -> Option<PendingEvent> {
        let hwnd = *self.by_xwin.get(&e.window)?;
        let (old_x, old_y, old_w, old_h, window, root) = {
            let w = self.windows.get(&hwnd)?;
            (
                w.x,
                w.y,
                w.width,
                w.height,
                w.window,
                self.conn.setup().roots[self.screen].root,
            )
        };
        // ConfigureNotify.x/y sont relatifs au parent; traduire l'origine vers
        // la racine couvre les fenêtres reparentées par un gestionnaire X11.
        let (x, y) = self
            .conn
            .translate_coordinates(window, root, 0, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| (reply.dst_x as i32, reply.dst_y as i32))
            .unwrap_or((e.x as i32, e.y as i32));
        if let Some(w) = self.windows.get_mut(&hwnd) {
            w.x = x;
            w.y = y;
            w.width = e.width;
            w.height = e.height;
        }
        let moved = x != old_x || y != old_y;
        let resized = e.width != old_w || e.height != old_h;
        if resized {
            let logical_width = self.logical(e.width as i32).clamp(1, u16::MAX as i32);
            let logical_height = self.logical(e.height as i32).clamp(1, u16::MAX as i32);
            let lparam = ((logical_height as isize) << 16) | (logical_width as isize & 0xFFFF);
            if moved {
                self.deferred_events.push(PendingEvent {
                    hwnd,
                    message: gui::WM_MOVE,
                    wparam: 0,
                    lparam: pack_i16_pair(self.logical(x), self.logical(y)),
                });
            }
            return Some(PendingEvent {
                hwnd,
                message: gui::WM_SIZE,
                wparam: 0,
                lparam,
            });
        }
        if moved {
            return Some(PendingEvent {
                hwnd,
                message: gui::WM_MOVE,
                wparam: 0,
                lparam: pack_i16_pair(self.logical(x), self.logical(y)),
            });
        }
        None
    }
}

/// Traduit le bouton X11 en bits `wParam` Win32 (MK_LBUTTON…).
fn button_wparam(detail: u8) -> usize {
    match detail {
        1 => 0x0001, // MK_LBUTTON
        2 => 0x0010, // MK_MBUTTON
        3 => 0x0002, // MK_RBUTTON
        _ => 0,
    }
}

/// Message Win32 d'un bouton X11 (1 gauche, 2 milieu, 3 droit).
fn button_message(detail: u8, down: bool) -> u32 {
    match (detail, down) {
        (2, true) => gui::WM_MBUTTONDOWN,
        (2, false) => gui::WM_MBUTTONUP,
        (3, true) => gui::WM_RBUTTONDOWN,
        (3, false) => gui::WM_RBUTTONUP,
        (_, true) => gui::WM_LBUTTONDOWN,
        (_, false) => gui::WM_LBUTTONUP,
    }
}

/// Boutons X11 4/5 (molette verticale) et 6/7 (horizontale) : un cran vaut
/// `WHEEL_DELTA` (120), positif vers le haut ou vers la droite.
fn wheel_message(detail: u8) -> Option<(u32, i16)> {
    match detail {
        4 => Some((gui::WM_MOUSEWHEEL, 120)),
        5 => Some((gui::WM_MOUSEWHEEL, -120)),
        6 => Some((gui::WM_MOUSEHWHEEL, -120)),
        7 => Some((gui::WM_MOUSEHWHEEL, 120)),
        _ => None,
    }
}

/// Boutons et modificateurs enfoncés (masque d'état X11) en bits MK_*.
fn button_state(state: x11rb::protocol::xproto::KeyButMask) -> usize {
    use x11rb::protocol::xproto::KeyButMask;
    let mut bits = 0;
    if state.contains(KeyButMask::BUTTON1) {
        bits |= 0x0001;
    }
    if state.contains(KeyButMask::BUTTON3) {
        bits |= 0x0002;
    }
    if state.contains(KeyButMask::SHIFT) {
        bits |= 0x0004;
    }
    if state.contains(KeyButMask::CONTROL) {
        bits |= 0x0008;
    }
    if state.contains(KeyButMask::BUTTON2) {
        bits |= 0x0010;
    }
    bits
}

fn pack_i16_pair(low: i32, high: i32) -> isize {
    (((high as u16 as u32) << 16) | low as u16 as u32) as isize
}

fn x11_function(rop: gui::RasterOp) -> x11rb::protocol::xproto::GX {
    use x11rb::protocol::xproto::GX;
    match rop {
        gui::RasterOp::Copy => GX::COPY,
        gui::RasterOp::And => GX::AND,
        gui::RasterOp::Or => GX::OR,
        gui::RasterOp::Xor => GX::XOR,
        gui::RasterOp::Invert => GX::INVERT,
        gui::RasterOp::CopyInverted => GX::COPY_INVERTED,
        gui::RasterOp::Clear => GX::CLEAR,
        gui::RasterOp::Set => GX::SET,
        gui::RasterOp::SrcErase => GX::AND_INVERTED, // (NOT source) AND destination
        gui::RasterOp::NotSrcErase => GX::NOR,       // NOT (source OR destination)
        gui::RasterOp::MergePaint => GX::OR_INVERTED, // (NOT source) OR destination
        gui::RasterOp::PatCopy => GX::COPY,          // pattern copié
        gui::RasterOp::PatPaint => GX::OR,           // approximation
        gui::RasterOp::DstCopy => GX::COPY,          // destination copié
    }
}

fn pixel_channel(pixel: u32, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    let value = (pixel & mask) >> shift;
    ((value as u64 * 255 + max as u64 / 2) / max as u64) as u32
}

fn pixel_to_colorref(pixel: u32, masks: (u32, u32, u32)) -> u32 {
    let r = pixel_channel(pixel, masks.0);
    let g = pixel_channel(pixel, masks.1);
    let b = pixel_channel(pixel, masks.2);
    (b << 16) | (g << 8) | r
}

/// Convertit un keysym X11 en code de touche virtuelle Win32 (`VK_*`).
fn keysym_to_vk(sym: u32) -> Option<usize> {
    let vk = match sym {
        0xFF08 => gui::VK_BACK,
        0xFF09 => gui::VK_TAB,
        0xFF0D => gui::VK_RETURN,
        0xFF1B => gui::VK_ESCAPE,
        0xFFFF => 0x2E,                                    // VK_DELETE
        0xFF51 => 0x25,                                    // VK_LEFT
        0xFF52 => 0x26,                                    // VK_UP
        0xFF53 => 0x27,                                    // VK_RIGHT
        0xFF54 => 0x28,                                    // VK_DOWN
        0xFF50 => 0x24,                                    // VK_HOME
        0xFF57 => 0x23,                                    // VK_END
        0xFF55 => 0x21,                                    // VK_PRIOR
        0xFF56 => 0x22,                                    // VK_NEXT
        0xFF61 => 0x2C,                                    // VK_SNAPSHOT (Impr. écran)
        0xFF63 => 0x2D,                                    // VK_INSERT
        0xFFBE..=0xFFC9 => 0x70 + (sym - 0xFFBE) as usize, // VK_F1..VK_F12
        0xFFE1 | 0xFFE2 => 0x10,                           // VK_SHIFT
        0xFFE3 | 0xFFE4 => 0x11,                           // VK_CONTROL
        0xFFE9 | 0xFFEA => 0x12,                           // VK_MENU
        0xFE03 | 0xFF7E => 0x12,                           // ISO_Level3_Shift / Mode_switch (AltGr)
        0xFFEB => 0x5B,                                    // Super_L / VK_LWIN
        0xFFEC => 0x5C,                                    // Super_R / VK_RWIN
        0xFF67 => 0x5D,                                    // Menu / VK_APPS
        0xFFE5 => gui::VK_CAPITAL,
        0xFF7F => gui::VK_NUMLOCK,
        0xFF14 => gui::VK_SCROLL,
        // ASCII imprimable : VK_A..VK_Z, chiffres et touches OEM Windows.
        0x20 => gui::VK_SPACE,
        0x21..=0x7E => {
            let c = sym as u8;
            match c {
                b'0'..=b'9' | b'A'..=b'Z' => c as usize,
                b'a'..=b'z' => (c - b'a' + b'A') as usize,
                b'!' => b'1' as usize,
                b'@' => b'2' as usize,
                b'#' => b'3' as usize,
                b'$' => b'4' as usize,
                b'%' => b'5' as usize,
                b'^' => b'6' as usize,
                b'&' => b'7' as usize,
                b'*' => b'8' as usize,
                b'(' => b'9' as usize,
                b')' => b'0' as usize,
                b'-' | b'_' => 0xBD,  // VK_OEM_MINUS
                b'=' | b'+' => 0xBB,  // VK_OEM_PLUS
                b'[' | b'{' => 0xDB,  // VK_OEM_4
                b']' | b'}' => 0xDD,  // VK_OEM_6
                b'\\' | b'|' => 0xDC, // VK_OEM_5
                b';' | b':' => 0xBA,  // VK_OEM_1
                b'\'' | b'"' => 0xDE, // VK_OEM_7
                b',' | b'<' => 0xBC,  // VK_OEM_COMMA
                b'.' | b'>' => 0xBE,  // VK_OEM_PERIOD
                b'/' | b'?' => 0xBF,  // VK_OEM_2
                b'`' | b'~' => 0xC0,  // VK_OEM_3
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(vk)
}

/// Premier keysym non nul : VK codes restent stables sous Maj et Caps Lock.
fn base_keysym(syms: &[u32]) -> Option<u32> {
    syms.first().copied().filter(|sym| *sym != 0)
}

/// Sélectionne le niveau effectif d'une touche, en appliquant Caps Lock aux
/// lettres mais pas aux chiffres ni à la ponctuation.
fn effective_keysym(syms: &[u32], shift: bool, caps: bool, level3: bool) -> Option<u32> {
    let group = if level3 && syms.get(2).is_some_and(|sym| *sym != 0) {
        2
    } else {
        0
    };
    let base = syms.get(group).copied().filter(|sym| *sym != 0)?;
    let is_letter = matches!(base, 0x41..=0x5A | 0x61..=0x7A);
    let level = (if is_letter { shift ^ caps } else { shift }) as usize;
    syms.get(group + level)
        .copied()
        .filter(|sym| *sym != 0)
        .or(Some(base))
}

fn key_levels_to_vk(base: u32, shifted: u32) -> Option<usize> {
    // Sur AZERTY le chiffre est au niveau Maj (&/1, é/2…), mais le VK reste
    // VK_1/VK_2 ; les lettres suivent la disposition choisie sur le serveur.
    if matches!(shifted, 0x30..=0x39) && !matches!(base, 0x41..=0x5A | 0x61..=0x7A) {
        return Some(shifted as usize);
    }
    keysym_to_vk(base)
}

fn vk_is_extended(vk: usize) -> bool {
    matches!(vk, 0x21..=0x2E | 0x5B..=0x5D)
}

/// Caractère produit par un keysym, 0 si la touche n'en produit pas.
fn keysym_to_char(sym: u32) -> u16 {
    match sym {
        0xFF08 => 0x08, // Retour arrière
        0xFF09 => 0x09, // Tabulation
        0xFF0D => 0x0D, // Entrée
        0xFF1B => 0x1B, // Échap
        0x20..=0x7E => sym as u16,
        0xA0..=0xFF => sym as u16, // Latin-1 (é, à, ç…)
        0x20AC => 0x20AC,          // EuroSign, keysym historique de l'AltGr+E
        0x0100_0000..=0x0110_FFFF => {
            let codepoint = sym & 0x00FF_FFFF;
            char::from_u32(codepoint)
                .and_then(|ch| u16::try_from(ch as u32).ok())
                .unwrap_or(0)
        }
        _ => 0,
    }
}

/// `lParam` d'un message clavier : une répétition, code matériel, transition.
fn key_lparam(keycode: u8, up: bool, extended: bool) -> isize {
    let scan = ((keycode as isize) << 16) | if extended { 1 << 24 } else { 0 };
    if up {
        scan | (1 << 30) | (1 << 31)
    } else {
        scan | 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loupe_agrandissement_lisse_ou_net() {
        // Deux pixels 32 bits : noir puis blanc, agrandis 4×.
        let image = [0, 0, 0, 0, 255, 255, 255, 0];
        let net = resample(&image, (2, 1, 8), (8, 1, 32), 4, false);
        assert_eq!(&net[..4], &[0, 0, 0, 0]);
        assert_eq!(&net[28..], &[255, 255, 255, 0]);
        assert!(net.chunks(4).all(|p| p[0] == 0 || p[0] == 255));
        let lisse = resample(&image, (2, 1, 8), (8, 1, 32), 4, true);
        assert_eq!(lisse[0], 0, "le bord gauche reste noir");
        assert_eq!(lisse[28], 255, "le bord droit reste blanc");
        let gris: Vec<u8> = lisse.chunks(4).map(|p| p[0]).collect();
        assert!(
            gris.windows(2).all(|w| w[0] <= w[1]),
            "dégradé croissant {gris:?}"
        );
        assert!(gris.iter().any(|&v| v > 0 && v < 255), "transition lissée");
        // Pas de lignes (stride) plus large que la largeur utile.
        let haut = resample(&[9, 9, 9, 0, 0, 0, 0, 0], (1, 2, 4), (2, 4, 8), 4, true);
        assert_eq!(haut.len(), 32);
        assert_eq!(haut[0], 9);
    }

    #[test]
    fn les_keysyms_deviennent_des_codes_vk_win32() {
        assert_eq!(keysym_to_vk(b'a' as u32), Some(0x41));
        assert_eq!(keysym_to_vk(b'A' as u32), Some(0x41));
        assert_eq!(keysym_to_vk(b'7' as u32), Some(0x37));
        assert_eq!(keysym_to_vk(b';' as u32), Some(0xBA)); // VK_OEM_1
        assert_eq!(keysym_to_vk(b'!' as u32), Some(0x31)); // touche 1
        assert_eq!(keysym_to_vk(b'?' as u32), Some(0xBF)); // VK_OEM_2
        assert_eq!(keysym_to_vk(0xFF53), Some(0x27)); // VK_RIGHT
        assert_eq!(keysym_to_vk(0xFFC2), Some(0x74)); // VK_F5
        assert_eq!(keysym_to_vk(0xFFE3), Some(0x11)); // VK_CONTROL
        assert_eq!(keysym_to_vk(0xFFEB), Some(0x5B)); // Super gauche
        assert_eq!(keysym_to_vk(0xFFEC), Some(0x5C)); // Super droit
        assert_eq!(keysym_to_vk(0xFE03), Some(0x12)); // AltGr
        assert_eq!(keysym_to_vk(0x1008_FF12), None);
    }

    #[test]
    fn niveaux_clavier_gerent_maj_caps_et_ponctuation() {
        let letter = [b'a' as u32, b'A' as u32];
        assert_eq!(
            effective_keysym(&letter, false, false, false),
            Some(b'a' as u32)
        );
        assert_eq!(
            effective_keysym(&letter, true, false, false),
            Some(b'A' as u32)
        );
        assert_eq!(
            effective_keysym(&letter, false, true, false),
            Some(b'A' as u32)
        );
        assert_eq!(
            effective_keysym(&letter, true, true, false),
            Some(b'a' as u32)
        );

        let digit = [b'1' as u32, b'!' as u32];
        assert_eq!(
            effective_keysym(&digit, false, true, false),
            Some(b'1' as u32)
        );
        assert_eq!(
            effective_keysym(&digit, true, false, false),
            Some(b'!' as u32)
        );
        assert_eq!(base_keysym(&digit), Some(b'1' as u32));
    }

    #[test]
    fn dispositions_azerty_et_altgr_preservent_raccourcis_et_texte() {
        assert_eq!(key_levels_to_vk(b'&' as u32, b'1' as u32), Some(0x31));
        assert_eq!(key_levels_to_vk(0xE9, b'2' as u32), Some(0x32));
        assert_eq!(key_levels_to_vk(b'a' as u32, b'A' as u32), Some(0x41));
        let french_e = [b'e' as u32, b'E' as u32, 0x20AC, 0x0100_00A2];
        assert_eq!(
            effective_keysym(&french_e, false, false, true),
            Some(0x20AC)
        );
        assert_eq!(
            effective_keysym(&french_e, true, false, true),
            Some(0x0100_00A2)
        );
        assert_eq!(keysym_to_char(0x20AC), 0x20AC);
        assert_eq!(keysym_to_char(0x0100_03A9), 0x03A9);
        assert_eq!(keysym_to_char(0x0100_D800), 0); // surrogate invalide
    }

    #[test]
    #[ignore = "requires an X11 server configured with setxkbmap fr"]
    fn xkb_azerty_reel_produit_altgr_et_touche_windows() {
        let keyboard = unsafe { XkbKeyboard::connect() }.expect("libX11/XKB");
        let (conn, _) = x11rb::connect(None).expect("X11");
        let setup = conn.setup();
        let mapping = conn
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)
            .unwrap()
            .reply()
            .unwrap();
        let width = mapping.keysyms_per_keycode as usize;
        let code = |sym| {
            mapping
                .keysyms
                .chunks(width)
                .position(|row| row.first() == Some(&sym))
                .map(|i| setup.min_keycode + i as u8)
                .expect("keysym in French keymap")
        };
        let e = code(b'e' as u32);
        assert_eq!(keyboard.translate(e, 0).unwrap().2, b'e' as u32);
        assert_eq!(keyboard.translate(e, 1).unwrap().2, b'E' as u32);
        assert_eq!(keyboard.translate(e, 2).unwrap().2, b'E' as u32);
        assert_eq!(
            keysym_to_char(keyboard.translate(e, 128).unwrap().2),
            0x20AC
        );
        let (base, shifted, _, _) = keyboard.translate(code(b'&' as u32), 0).unwrap();
        assert_eq!(key_levels_to_vk(base, shifted), Some(0x31));
        let (base, shifted, _, _) = keyboard.translate(code(0xFFEB), 0).unwrap();
        assert_eq!(key_levels_to_vk(base, shifted), Some(0x5B));
    }

    #[test]
    fn caracteres_et_parametres_de_messages_clavier_sont_traduits() {
        assert_eq!(keysym_to_char(b'!' as u32), b'!' as u16);
        assert_eq!(keysym_to_char(0xE9), 0xE9); // é Latin-1
        assert_eq!(keysym_to_char(0x0100_00E9), 0xE9); // é Unicode keysym
        assert_eq!(keysym_to_char(0x0101_F600), 0); // supplementary plane non pris en charge
        assert_eq!(keysym_to_char(0xFF0D), 0x0D); // Entrée
        assert_eq!(keysym_to_char(0xFF51), 0); // flèche : pas de WM_CHAR

        assert_eq!(key_lparam(38, false, false), (38isize << 16) | 1);
        assert_eq!(
            key_lparam(113, true, true),
            (113isize << 16) | (1 << 24) | (1 << 30) | (1 << 31),
        );
    }

    #[test]
    fn echelle_dpi_couvre_1080p_1440p_et_4k() {
        assert_eq!(dpi_scale(96.0, None), 1.0);
        assert_eq!(dpi_scale(144.0, None), 1.5);
        assert_eq!(dpi_scale(192.0, None), 2.0);
        assert_eq!(dpi_scale(96.0, Some(2.5)), 2.5);
        assert_eq!(dpi_scale(500.0, None), 3.0);
    }

    #[test]
    fn interpolation_bilineaire_des_bitmaps_est_stable() {
        let pixels = [0x0000_0000, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000];
        assert_eq!(bilinear_colorref(&pixels, 2, 2, 0.0, 0.0), 0x0000_0000);
        assert_eq!(bilinear_colorref(&pixels, 2, 2, 1.0, 1.0), 0x00ff_0000);
        assert_eq!(bilinear_colorref(&pixels, 2, 2, 0.5, 0.5), 0x0040_4040);
    }

    #[test]
    #[ignore = "requires an X11 server with Cairo and Xft"]
    fn contours_anticreneles_x11() {
        let mut backend = X11Backend::connect().expect("X11");
        assert!(backend.xft.as_ref().is_some_and(|r| r.cairo.is_some()));
        let screen = &backend.conn.setup().roots[backend.screen];
        let (root, depth) = (screen.root, screen.root_depth);
        let pixmap = backend.conn.generate_id().unwrap();
        let gc = backend.conn.generate_id().unwrap();
        backend
            .conn
            .create_pixmap(depth, pixmap, root, 64, 64)
            .unwrap()
            .check()
            .unwrap();
        backend
            .conn
            .create_gc(gc, pixmap, &CreateGCAux::new())
            .unwrap()
            .check()
            .unwrap();
        let window = XWin {
            window: pixmap,
            gc,
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        };
        backend.paint_cmds(
            &window,
            &[
                DrawCmd::FillRect {
                    rect: Rect {
                        left: 0,
                        top: 0,
                        right: 64,
                        bottom: 64,
                    },
                    color: 0,
                },
                DrawCmd::Ellipse {
                    rect: Rect {
                        left: 3,
                        top: 3,
                        right: 30,
                        bottom: 30,
                    },
                    color: 0x00ff_ffff,
                },
                DrawCmd::Line {
                    x1: 34,
                    y1: 5,
                    x2: 53,
                    y2: 57,
                    color: 0x00ff_ffff,
                    width: 2,
                },
            ],
        );
        backend.before_x11();
        let image = backend
            .conn
            .get_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                pixmap,
                0,
                0,
                64,
                64,
                u32::MAX,
            )
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(
            image.data.len(),
            64 * 64 * 4,
            "test requires 32-bit Xvfb pixels"
        );
        let gray = |data: &[u8]| {
            data.chunks_exact(4)
                .any(|p| p[0] > 0 && p[0] < 255 && p[0] == p[1] && p[1] == p[2])
        };
        let left: Vec<u8> = image
            .data
            .chunks_exact(64 * 4)
            .flat_map(|row| row[..32 * 4].iter().copied())
            .collect();
        let right: Vec<u8> = image
            .data
            .chunks_exact(64 * 4)
            .flat_map(|row| row[32 * 4..].iter().copied())
            .collect();
        assert!(gray(&left), "ellipse must contain fractional edge coverage");
        assert!(
            gray(&right),
            "diagonal must contain fractional edge coverage"
        );
        backend.conn.free_gc(gc).unwrap();
        backend.conn.free_pixmap(pixmap).unwrap();
    }

    #[test]
    fn truecolor_respecte_les_masques_du_serveur() {
        assert_eq!(
            truecolor_pixel(0x0033_2211, 0xff0000, 0xff00, 0xff),
            0x0011_2233
        );
        assert_eq!(truecolor_pixel(0x00ff_ffff, 0xf800, 0x07e0, 0x001f), 0xffff);
        assert_eq!(truecolor_pixel(0, 0xf800, 0x07e0, 0x001f), 0);
    }

    #[test]
    #[ignore = "requires an X11 server; run with xvfb-run"]
    fn grande_bitmap_1080p_atteint_la_derniere_bande_x11() {
        let backend = X11Backend::connect().expect("X11");
        let root = &backend.conn.setup().roots[backend.screen];
        let pixmap = backend.conn.generate_id().unwrap();
        let gc = backend.conn.generate_id().unwrap();
        backend
            .conn
            .create_pixmap(root.root_depth, pixmap, root.root, 1920, 1080)
            .unwrap()
            .check()
            .unwrap();
        backend
            .conn
            .create_gc(gc, pixmap, &CreateGCAux::new())
            .unwrap()
            .check()
            .unwrap();
        let window = XWin {
            window: pixmap,
            gc,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let rect = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let mut pixels = vec![0x0000_00ff; 1920 * 1080];
        pixels[1920 * 540..].fill(0x00ff_0000);
        backend.paint_bitmap(&window, &rect, &rect, &pixels, gui::RasterOp::Copy);
        let cached = backend.bitmap_cache.borrow().as_ref().unwrap().pixmap;
        let started = std::time::Instant::now();
        backend.paint_bitmap(&window, &rect, &rect, &pixels, gui::RasterOp::Copy);
        assert_eq!(
            backend.bitmap_cache.borrow().as_ref().unwrap().pixmap,
            cached
        );
        eprintln!("1080p cached repaint: {:?}", started.elapsed());
        // Changed content must invalidate the cache, even with identical dimensions.
        pixels[1920 * 1080 - 1] = 0x0000_ff00;
        backend.paint_bitmap(&window, &rect, &rect, &pixels, gui::RasterOp::Copy);
        assert_ne!(
            backend.bitmap_cache.borrow().as_ref().unwrap().pixmap,
            cached
        );
        // Synchronise and inspect both ends, beyond the former 256 KiB limit.
        let top = backend
            .conn
            .get_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                pixmap,
                0,
                0,
                1,
                1,
                u32::MAX,
            )
            .unwrap()
            .reply()
            .unwrap();
        let bottom = backend
            .conn
            .get_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                pixmap,
                1919,
                1079,
                1,
                1,
                u32::MAX,
            )
            .unwrap()
            .reply()
            .unwrap();
        let format = backend
            .conn
            .setup()
            .pixmap_formats
            .iter()
            .find(|f| f.depth == root.root_depth)
            .unwrap();
        let visual = root
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .find(|v| v.visual_id == root.root_visual)
            .unwrap();
        let decode = |bytes: &[u8]| {
            let mut value = [0u8; 4];
            let len = (format.bits_per_pixel / 8) as usize;
            if backend.conn.setup().image_byte_order
                == x11rb::protocol::xproto::ImageOrder::LSB_FIRST
            {
                value[..len].copy_from_slice(&bytes[..len]);
                u32::from_le_bytes(value)
            } else {
                value[4 - len..].copy_from_slice(&bytes[..len]);
                u32::from_be_bytes(value)
            }
        };
        assert_eq!(decode(&top.data), visual.red_mask);
        assert_eq!(decode(&bottom.data), visual.green_mask);
        backend.conn.free_gc(gc).unwrap();
        backend.conn.free_pixmap(pixmap).unwrap();
    }
}

/// Agrandit/réduit une image ZPixmap. `src`/`dst` = (largeur, hauteur, pas).
/// En mode lissé (pixels 24/32 bits), interpolation bilinéaire en virgule
/// fixe ; sinon plus proche voisin, comme le mode COLORONCOLOR de Windows.
fn resample(
    image: &[u8],
    src: (usize, usize, usize),
    dst: (usize, usize, usize),
    bytes_per_pixel: usize,
    smooth: bool,
) -> Vec<u8> {
    let (sw, sh, s_stride) = src;
    let (dw, dh, d_stride) = dst;
    let mut output = vec![0u8; d_stride * dh];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return output;
    }
    let bilinear = smooth && bytes_per_pixel >= 3 && (sw > 1 || sh > 1);
    for dy in 0..dh {
        let row = &mut output[dy * d_stride..];
        if !bilinear {
            let sy = dy * sh / dh;
            for dx in 0..dw {
                let sx = dx * sw / dw;
                let from = sy * s_stride + sx * bytes_per_pixel;
                row[dx * bytes_per_pixel..(dx + 1) * bytes_per_pixel]
                    .copy_from_slice(&image[from..from + bytes_per_pixel]);
            }
            continue;
        }
        // Centre du pixel de destination projeté dans la source (1/256 px).
        let fy = ((2 * dy + 1) * sh * 128 / dh).saturating_sub(128);
        let y0 = (fy >> 8).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let wy = (fy & 0xFF) as u32;
        for dx in 0..dw {
            let fx = ((2 * dx + 1) * sw * 128 / dw).saturating_sub(128);
            let x0 = (fx >> 8).min(sw - 1);
            let x1 = (x0 + 1).min(sw - 1);
            let wx = (fx & 0xFF) as u32;
            let at = |x: usize, y: usize, c: usize| {
                u32::from(image[y * s_stride + x * bytes_per_pixel + c])
            };
            for c in 0..bytes_per_pixel {
                let top = at(x0, y0, c) * (256 - wx) + at(x1, y0, c) * wx;
                let bottom = at(x0, y1, c) * (256 - wx) + at(x1, y1, c) * wx;
                let value = (top * (256 - wy) + bottom * wy + 32768) >> 16;
                row[dx * bytes_per_pixel + c] = value.min(255) as u8;
            }
        }
    }
    output
}

impl gui::GuiBackend for X11Backend {
    fn pump(&mut self) -> Vec<PendingEvent> {
        // Événements déjà lus pendant `wait`, puis lecture non bloquante.
        let mut evs: Vec<Event> = std::mem::take(&mut self.pending);
        while let Ok(Some(ev)) = self.conn.poll_for_event() {
            evs.push(ev);
        }
        let mut out = std::mem::take(&mut self.deferred_events);
        for ev in evs {
            if let Some(p) = self.translate(ev) {
                out.push(p);
            }
            out.append(&mut self.deferred_events);
        }
        self.apply_cursor();
        out
    }

    fn wait(&mut self, timeout_ms: u32) -> bool {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        loop {
            match self.conn.poll_for_event() {
                Ok(Some(ev)) => self.pending.push(ev),
                Ok(None) => {}
                // Connexion perdue : on laisse la boucle de messages se vider.
                Err(_) => return !self.pending.is_empty(),
            }
            if !self.pending.is_empty() {
                return true;
            }
            if Instant::now() >= deadline {
                // Backend réel : GetMessage doit continuer d'attendre.
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn native_window(&mut self, hwnd: usize) -> Option<u64> {
        self.windows.get(&hwnd).map(|w| w.window as u64)
    }

    fn screen_size(&mut self) -> Option<(i32, i32)> {
        // La géométrie de la racine est relue à chaque appel : après un
        // changement de mode RandR, la valeur du setup initial est périmée.
        let root = self.conn.setup().roots[self.screen].root;
        let geometry = self.conn.get_geometry(root).ok()?.reply().ok()?;
        Some((
            self.logical(geometry.width as i32).max(1),
            self.logical(geometry.height as i32).max(1),
        ))
    }

    fn monitors(&mut self) -> Vec<crate::dlls::gui::Monitor> {
        use x11rb::protocol::randr::ConnectionExt as _;
        let root = self.conn.setup().roots[self.screen].root;
        let Some(reply) = self
            .conn
            .randr_get_monitors(root, true)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return Vec::new();
        };
        reply
            .monitors
            .iter()
            .filter(|m| m.width > 0 && m.height > 0)
            .map(|m| crate::dlls::gui::Monitor {
                rect: Rect {
                    left: self.logical(i32::from(m.x)),
                    top: self.logical(i32::from(m.y)),
                    right: self.logical(i32::from(m.x) + i32::from(m.width)),
                    bottom: self.logical(i32::from(m.y) + i32::from(m.height)),
                },
                primary: m.primary,
            })
            .collect()
    }

    fn create_window(&mut self, hwnd: usize, title: &str, rect: Rect) {
        if self.x11_cursors.is_empty() {
            self.init_cursors();
        }
        let setup = self.conn.setup();
        let scr = &setup.roots[self.screen];
        // Profondeur CopyFromParent (0) : le serveur prend celle de la racine.
        let visual = scr.root_visual;

        let x = self
            .scaled(rect.left)
            .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let y = self
            .scaled(rect.top)
            .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let w = self.scaled(rect.width()).max(1).clamp(1, u16::MAX as i32) as u16;
        let h = self.scaled(rect.height()).max(1).clamp(1, u16::MAX as i32) as u16;

        let win = match self.conn.generate_id() {
            Ok(id) => id,
            Err(_) => return,
        };
        let bg = self.alloc_color(theme::BG_DEEP);
        let pointer_cursor = self.x11_cursors.get(&0x0001_0001).copied();
        let aux = CreateWindowAux::new().background_pixel(bg).event_mask(
            x11rb::protocol::xproto::EventMask::EXPOSURE
                | x11rb::protocol::xproto::EventMask::STRUCTURE_NOTIFY
                | x11rb::protocol::xproto::EventMask::BUTTON_PRESS
                | x11rb::protocol::xproto::EventMask::BUTTON_RELEASE
                | x11rb::protocol::xproto::EventMask::POINTER_MOTION
                | x11rb::protocol::xproto::EventMask::FOCUS_CHANGE
                | x11rb::protocol::xproto::EventMask::KEY_PRESS
                | x11rb::protocol::xproto::EventMask::KEY_RELEASE,
        );
        let aux = if let Some(cursor) = pointer_cursor {
            aux.cursor(cursor)
        } else {
            aux
        };

        if self
            .conn
            .create_window(
                0, // CopyFromParent depth
                win,
                scr.root,
                x,
                y,
                w,
                h,
                0, // border
                WindowClass::INPUT_OUTPUT,
                visual,
                &aux,
            )
            .is_err()
        {
            return;
        }

        // Demande la fermeture propre via WM_DELETE_WINDOW.
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win,
            self.wm_protocols,
            AtomEnum::ATOM,
            &[self.wm_protocols, self.wm_delete],
        );
        let title_bytes = crate::dlls::encode_ansi(title);
        let _ = self.conn.change_property8(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            &title_bytes,
        );

        let atom = |name: &[u8]| -> Option<Atom> {
            self.conn
                .intern_atom(false, name)
                .ok()?
                .reply()
                .ok()
                .map(|v| v.atom)
        };
        if let (Some(name), Some(utf8)) = (atom(b"_NET_WM_NAME"), atom(b"UTF8_STRING")) {
            let _ = self.conn.change_property8(
                x11rb::protocol::xproto::PropMode::REPLACE,
                win,
                name,
                utf8,
                title.as_bytes(),
            );
        }
        if let Some(pid) = atom(b"_NET_WM_PID") {
            let _ = self.conn.change_property32(
                x11rb::protocol::xproto::PropMode::REPLACE,
                win,
                pid,
                AtomEnum::CARDINAL,
                &[std::process::id()],
            );
        }
        let class = std::env::var("WINLUX_SHELL_APP")
            .map(|app| format!("winlux-{app}"))
            .unwrap_or_else(|_| {
                std::env::args()
                    .nth(1)
                    .and_then(|arg| {
                        std::path::Path::new(&arg)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                    })
                    .unwrap_or_else(|| "Winlux".into())
            });
        let class = format!("{class}\0{class}\0");
        let _ = self.conn.change_property8(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            class.as_bytes(),
        );
        if title == "Winlux Desktop" && std::env::var_os("WINLUX_NATIVE_SHELL").is_some() {
            if let (Some(kind), Some(desktop)) = (
                atom(b"_NET_WM_WINDOW_TYPE"),
                atom(b"_NET_WM_WINDOW_TYPE_DESKTOP"),
            ) {
                let _ = self.conn.change_property32(
                    x11rb::protocol::xproto::PropMode::REPLACE,
                    win,
                    kind,
                    AtomEnum::ATOM,
                    &[desktop],
                );
            }
        }

        let gc = match self.conn.generate_id() {
            Ok(id) => id,
            Err(_) => return,
        };
        let _ = self
            .conn
            .create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0));

        let _ = self.conn.flush();
        self.windows.insert(
            hwnd,
            XWin {
                window: win,
                gc,
                x: x as i32,
                y: y as i32,
                width: w,
                height: h,
            },
        );
        self.by_xwin.insert(win, hwnd);
    }

    fn draw(&mut self, hwnd: usize, cmds: &[DrawCmd]) {
        self.content.insert(hwnd, cmds.to_vec());
        let Some(w) = self.windows.get(&hwnd).copied() else {
            return;
        };
        if let Some(pixmap) = self.back_buffer(hwnd, &w) {
            let target = XWin {
                window: pixmap,
                ..w
            };
            self.paint_theme(&target);
            self.paint_cmds(&target, cmds);
            self.overlay.frame(hwnd);
            self.paint_overlay(&target, hwnd);
            self.present(&w, pixmap, 0, 0, w.width, w.height);
        } else {
            self.paint_theme(&w);
            self.paint_cmds(&w, cmds);
            self.overlay.frame(hwnd);
            self.paint_overlay(&w, hwnd);
            self.before_x11();
            let _ = self.conn.flush();
        }
    }

    fn destroy_window(&mut self, hwnd: usize) {
        self.content.remove(&hwnd);
        self.overlay.forget(hwnd);
        if let Some((pixmap, _, _)) = self.back.remove(&hwnd) {
            let _ = self.conn.free_pixmap(pixmap);
        }
        if let Some(w) = self.windows.remove(&hwnd) {
            self.by_xwin.remove(&w.window);
            let _ = self.conn.free_gc(w.gc);
            let _ = self.conn.destroy_window(w.window);
            let _ = self.conn.flush();
        }
    }

    fn show_window(&mut self, hwnd: usize, show: bool) {
        if let Some(w) = self.windows.get(&hwnd) {
            if show {
                let _ = self.conn.map_window(w.window);
            } else {
                let _ = self.conn.unmap_window(w.window);
            }
            let _ = self.conn.flush();
        }
    }

    fn set_title(&mut self, hwnd: usize, title: &str) {
        if let Some(w) = self.windows.get(&hwnd) {
            let title_bytes = crate::dlls::encode_ansi(title);
            let _ = self.conn.change_property8(
                x11rb::protocol::xproto::PropMode::REPLACE,
                w.window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                &title_bytes,
            );
            if let (Ok(name), Ok(utf8)) = (
                self.conn.intern_atom(false, b"_NET_WM_NAME"),
                self.conn.intern_atom(false, b"UTF8_STRING"),
            ) {
                if let (Ok(name), Ok(utf8)) = (name.reply(), utf8.reply()) {
                    let _ = self.conn.change_property8(
                        x11rb::protocol::xproto::PropMode::REPLACE,
                        w.window,
                        name.atom,
                        utf8.atom,
                        title.as_bytes(),
                    );
                }
            }
            let _ = self.conn.flush();
        }
    }

    fn set_window_rect(&mut self, hwnd: usize, rect: Rect) {
        let x = self
            .scaled(rect.left)
            .clamp(i16::MIN as i32, i16::MAX as i32);
        let y = self
            .scaled(rect.top)
            .clamp(i16::MIN as i32, i16::MAX as i32);
        let width = self.scaled(rect.width()).max(1).min(u16::MAX as i32) as u16;
        let height = self.scaled(rect.height()).max(1).min(u16::MAX as i32) as u16;
        let window = match self.windows.get_mut(&hwnd) {
            Some(w) => {
                w.x = x;
                w.y = y;
                w.width = width;
                w.height = height;
                w.window
            }
            None => return,
        };
        let aux = ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .width(width as u32)
            .height(height as u32);
        let _ = self.conn.configure_window(window, &aux);
        let _ = self.conn.flush();
    }

    fn set_focus(&mut self, hwnd: usize) {
        let focus = if hwnd == 0 {
            0
        } else if let Some(w) = self.windows.get(&hwnd) {
            w.window
        } else {
            return;
        };
        let _ = self
            .conn
            .set_input_focus(x11rb::protocol::xproto::InputFocus::PARENT, focus, 0u32);
        let _ = self.conn.flush();
        // SetFocus est synchrone côté Win32 : le round-trip force le serveur à
        // traiter la requête et rend FocusIn/FocusOut immédiatement pompables.
        if let Ok(cookie) = self.conn.get_input_focus() {
            let _ = cookie.reply();
        }
    }

    fn measure_text(&mut self, font: &FontSpec, text: &str) -> Option<(i32, i32)> {
        let spec = self.scaled_font(font);
        if let Some((w, h, _)) = self
            .xft
            .as_mut()
            .and_then(|xft| xft.measure(&spec, text.as_bytes()))
        {
            return Some((self.logical(w), self.logical(h)));
        }
        let f = self.ensure_font(&spec)?;
        let w = text.chars().count() as i32 * f.char_width as i32;
        Some((self.logical(w), font.height.abs().max(f.ascent as i32)))
    }

    fn capture(&mut self, hwnd: usize, rect: Rect) -> Option<BitmapSnapshot> {
        let mut window = self.windows.get(&hwnd).copied()?;
        if let Some(&(pixmap, width, height)) = self.back.get(&hwnd) {
            if width == window.width && height == window.height {
                window.window = pixmap;
            }
        }
        self.before_x11();
        let area = Self::to_x_rect(&window, &rect)?;
        self.read_drawable(window.window, area)
    }

    fn capture_screen(&mut self, rect: Rect) -> Option<BitmapSnapshot> {
        // La racine d'un Xorg sans compositeur contient l'image réellement
        // affichée : bureau Winlux et fenêtres des autres programmes.
        self.before_x11();
        let _ = self.conn.flush();
        let root = self.conn.setup().roots[self.screen].root;
        let geometry = self.conn.get_geometry(root).ok()?.reply().ok()?;
        let physical = self.scaled_rect(&rect);
        let x0 = physical.left.clamp(0, geometry.width as i32);
        let y0 = physical.top.clamp(0, geometry.height as i32);
        let x1 = physical.right.clamp(0, geometry.width as i32);
        let y1 = physical.bottom.clamp(0, geometry.height as i32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let area = x11rb::protocol::xproto::Rectangle {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0) as u16,
            height: (y1 - y0) as u16,
        };
        let snapshot = self.read_drawable(root, area)?;
        let width = rect.width().max(1) as usize;
        let height = rect.height().max(1) as usize;
        if snapshot.width == width && snapshot.height == height {
            return Some(snapshot);
        }
        // Mise à l'échelle HiDPI : on revient en unités logiques Win32.
        let mut pixels = vec![0; width.checked_mul(height)?];
        for y in 0..height {
            let sy = (y * snapshot.height / height).min(snapshot.height - 1);
            for x in 0..width {
                let sx = (x * snapshot.width / width).min(snapshot.width - 1);
                pixels[y * width + x] = snapshot.pixels[sy * snapshot.width + sx];
            }
        }
        Some(BitmapSnapshot {
            width,
            height,
            pixels,
        })
    }

    fn invalidate(&mut self, hwnd: usize) {
        // Avec un tampon arrière, effacer la fenêtre ne ferait que clignoter :
        // la prochaine peinture recouvre tout d'un bloc.
        if self.back.contains_key(&hwnd) {
            return;
        }
        if let Some(w) = self.windows.get(&hwnd) {
            let _ = self.conn.clear_area(false, w.window, 0, 0, 0, 0);
            let _ = self.conn.flush();
        }
    }
}

impl Drop for X11Backend {
    fn drop(&mut self) {
        if let Some(cached) = self.bitmap_cache.get_mut().take() {
            let _ = self.conn.free_pixmap(cached.pixmap);
        }
        for font in self.fonts.values() {
            let _ = self.conn.close_font(font.id);
        }
        for &cursor in self.x11_cursors.values() {
            let _ = self.conn.free_cursor(cursor);
        }
        let _ = self.conn.flush();
    }
}
