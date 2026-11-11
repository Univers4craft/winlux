//! Backend X11 de Winlux : crée de vraies fenêtres sur le serveur X et traduit
//! ses événements en messages Win32. Le fond utilise la palette claire
//! Winlux inspirée de Windows 11, définie dans `gui::theme`.

use crate::dlls::gui::theme;
use crate::dlls::gui::{self, BitmapSnapshot, DrawCmd, FontSpec, PendingEvent, Rect};
use crate::dlls::gui_cairo::{Cairo, Shape};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_uchar, c_ulong, c_void, CString};
use std::os::unix::io::AsRawFd;
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
    /// Sous-fenêtre X d'un contrôle `WS_CHILD` : coordonnées relatives au
    /// parent, ni décorations ni propriétés de gestionnaire de fenêtres.
    child: bool,
    /// Fenêtre de premier niveau avec barre de menus : `frame` est la
    /// fenêtre X gérée par le gestionnaire (titre, taille totale) qui porte
    /// la barre sur `bar` pixels ; `window` est alors la zone client, placée
    /// dessous. 0 sans barre.
    frame: Window,
    bar: u16,
}

/// Dégradé de fond déjà calculé, invalidé quand le contraste change.
#[derive(Default)]
struct ThemeCache {
    contrast: u8,
    bands: HashMap<u16, std::sync::Arc<Vec<(i16, u16, u32)>>>,
}

/// Bandes (y, hauteur, pixel) du fond d'une fenêtre de `height` pixels :
/// 32 bandes du dégradé clair, ou une seule bande de la couleur de fond du
/// thème de contraste. `alloc` convertit un COLORREF en pixel X11.
fn compute_theme_bands(
    height: u16,
    contrast: u8,
    alloc: impl Fn(u32) -> u32,
) -> Vec<(i16, u16, u32)> {
    let h = height.max(1) as u32;
    if let Some(colors) = crate::dlls::a11y::contrast_colors(contrast) {
        return vec![(0, h.min(u16::MAX as u32) as u16, alloc(colors.background))];
    }
    let bands = 32u32;
    (0..bands)
        .map(|i| {
            let t = i as f32 / (bands - 1) as f32;
            let c = theme::lerp_colorref(theme::GRADIENT_TOP, theme::GRADIENT_BOTTOM, t);
            let y = (i * h / bands) as i16;
            let band_h = ((i + 1) * h / bands - i * h / bands).max(1) as u16;
            (y, band_h, alloc(c))
        })
        .collect()
}

/// FocusIn/FocusOut qui ne concerne pas la fenêtre elle-même : reçu par un
/// ancêtre de celle qui prend ou perd le focus (NotifyVirtual,
/// NotifyNonlinearVirtual), ou par celle qui contient le pointeur quand le
/// focus suit le pointeur (NotifyPointer).
fn is_virtual_focus(detail: x11rb::protocol::xproto::NotifyDetail) -> bool {
    use x11rb::protocol::xproto::NotifyDetail;
    detail == NotifyDetail::VIRTUAL
        || detail == NotifyDetail::NONLINEAR_VIRTUAL
        || detail == NotifyDetail::POINTER
}

/// Vrai pour un FocusIn/FocusOut dû à un grab (menu, glisser) : le focus
/// réel de la fenêtre n'a pas changé.
fn focus_is_grab(mode: x11rb::protocol::xproto::NotifyMode) -> bool {
    use x11rb::protocol::xproto::NotifyMode;
    matches!(mode, NotifyMode::GRAB | NotifyMode::UNGRAB)
}

/// Touche AltGr : ISO_Level3_Shift ou Mode_switch.
fn is_altgr_keysym(base: Option<u32>) -> bool {
    matches!(base, Some(0xFE03 | 0xFF7E))
}

/// (VK, lParam) des deux messages d'AltGr dans l'ordre d'envoi : Ctrl
/// gauche (balayage 0x1D) puis Alt droit (0x38 étendu) à l'appui, l'inverse
/// au relâchement (bits 30 et 31).
fn altgr_messages(down: bool) -> [(usize, isize); 2] {
    let up_bits: isize = if down { 0 } else { (1 << 30) | (1 << 31) };
    let ctrl = (0x11, 1 | (0x1D << 16) | up_bits);
    let alt = (0x12, 1 | (0x38 << 16) | (1 << 24) | up_bits);
    if down {
        [ctrl, alt]
    } else {
        [alt, ctrl]
    }
}

/// Famille fontconfig d'une police GDI : les noms de police par défaut
/// (« fixed », vide, police système des boîtes) deviennent Segoe UI (#1054).
fn xft_family(face: &str) -> &str {
    match face.to_ascii_lowercase().as_str() {
        "" | "fixed" | "system" | "ms shell dlg" | "ms shell dlg 2" => "Segoe UI",
        _ => face,
    }
}

/// FocusOut d'un contrôle dont le focus remonte à un ancêtre alors que le
/// dernier SetFocus de Winlux visait ce contrôle : c'est le gestionnaire de
/// fenêtres qui a activé la fenêtre de premier niveau (clic dedans), pas le
/// programme. Le contrôle doit garder le focus Win32 (#1137).
fn focus_taken_by_wm(
    detail: x11rb::protocol::xproto::NotifyDetail,
    asked_focus: u32,
    xwin: u32,
    child: bool,
) -> bool {
    detail == x11rb::protocol::xproto::NotifyDetail::ANCESTOR && child && asked_focus == xwin
}

/// FocusIn d'un contrôle qui n'est pas la dernière cible de SetFocus :
/// écho périmé d'une remise du focus, sans effet côté Win32 (#1286).
fn stale_child_focus(child: bool, asked_focus: u32, xwin: u32) -> bool {
    child && asked_focus != xwin
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
    /// XftCharExists (facultatif) : la police a-t-elle ce glyphe ?
    char_exists: Option<unsafe extern "C" fn(*mut Display, *mut XftFont, u32) -> c_int>,
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
            char_exists: symbol(xft, b"XftCharExists\0"),
        })
    }
}

/// Équivalent courant d'un symbole souvent absent des polices de texte.
fn symbol_substitute(c: char) -> Option<char> {
    Some(match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0)?, // pleine chasse
        '\u{2715}' | '\u{2716}' | '\u{2A2F}' | '\u{274C}' => '\u{00D7}',
        '\u{2795}' | '\u{271A}' => '+',
        '\u{2796}' | '\u{2212}' => '-',
        '\u{2714}' | '\u{2713}' => '\u{221A}',
        '\u{25BE}' | '\u{2BC6}' | '\u{2B9F}' => '\u{25BC}',
        '\u{25B4}' | '\u{2BC5}' => '\u{25B2}',
        _ => return None,
    })
}

struct XftRenderer {
    api: XftApi,
    display: *mut Display,
    screen: c_int,
    visual: *mut Visual,
    colormap: c_ulong,
    fonts: HashMap<FontSpec, *mut XftFont>,
    /// Polices de repli ouvertes pour une police demandée : les caractères
    /// qu'elle n'a pas (chinois, japonais…) y sont cherchés, comme la
    /// liaison de polices de GDI.
    fallbacks: HashMap<FontSpec, Vec<*mut XftFont>>,
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
            fallbacks: HashMap::new(),
            cairo: Cairo::load(),
        })
    }

    const RENDER: &'static str =
        ":antialias=true:hinting=true:hintstyle=hintfull:rgba=none:lcdfilter=lcdnone";

    /// Police de repli qui a le caractère `c`, à la taille de `spec` :
    /// d'abord parmi celles déjà ouvertes, sinon fontconfig en choisit une
    /// par « charset ». None si aucune ne l'a.
    fn fallback(&mut self, spec: &FontSpec, c: char) -> Option<*mut XftFont> {
        let exists = self.api.char_exists?;
        let has = |f: *mut XftFont| unsafe { exists(self.display, f, c as u32) != 0 };
        if let Some(f) = self
            .fallbacks
            .get(spec)
            .and_then(|l| l.iter().copied().find(|&f| has(f)))
        {
            return Some(f);
        }
        let size = spec.height.saturating_abs().max(9);
        let weight = if spec.weight >= 600 {
            "bold"
        } else {
            "regular"
        };
        let pattern = CString::new(format!(
            ":charset={:x}:pixelsize={size}:weight={weight}{}",
            c as u32,
            Self::RENDER
        ))
        .ok()?;
        let font =
            unsafe { (self.api.font_open_name)(self.display, self.screen, pattern.as_ptr()) };
        if font.is_null() {
            return None;
        }
        if !has(font) {
            unsafe { (self.api.font_close)(self.display, font) };
            return None;
        }
        self.fallbacks.entry(spec.clone()).or_default().push(font);
        Some(font)
    }

    /// Découpe le texte en morceaux, chacun avec la police qui a ses
    /// caractères : la police demandée, ou une police de repli.
    fn runs(&mut self, spec: &FontSpec, text: &[u8]) -> Option<Vec<(*mut XftFont, Vec<u8>)>> {
        let font = self.font(spec)?;
        let text = self.substitute(font, text).into_owned();
        let (Some(exists), Ok(s)) = (self.api.char_exists, std::str::from_utf8(&text)) else {
            return Some(vec![(font, text)]);
        };
        let s = s.to_string();
        let mut out: Vec<(*mut XftFont, Vec<u8>)> = Vec::new();
        for c in s.chars() {
            let own = c.is_ascii() || unsafe { exists(self.display, font, c as u32) != 0 };
            let f = if own {
                font
            } else {
                self.fallback(spec, c).unwrap_or(font)
            };
            let mut buf = [0u8; 4];
            let bytes = c.encode_utf8(&mut buf).as_bytes();
            match out.last_mut() {
                Some((last, run)) if *last == f => run.extend_from_slice(bytes),
                _ => out.push((f, bytes.to_vec())),
            }
        }
        Some(out)
    }

    fn font(&mut self, spec: &FontSpec) -> Option<*mut XftFont> {
        if let Some(font) = self.fonts.get(spec) {
            return Some(*font);
        }
        // Police par défaut (« fixed », vide, police système des boîtes) :
        // Segoe UI, comme usp10 et les chemins GDI, que fontconfig remplace
        // par Liberation Sans (métriques d'Arial). « Noto Sans » n'est pas
        // dans l'ISO : fontconfig retombait sur DejaVu Sans, 15 % plus large,
        // et le titre de l'accueil d'Audacity débordait à gauche (#1054).
        let family = xft_family(&spec.face);
        let size = spec.height.saturating_abs().max(9);
        let weight = if spec.weight >= 600 {
            "bold"
        } else {
            "regular"
        };
        let slant = if spec.italic { "italic" } else { "roman" };
        let pattern = CString::new(format!(
            "{family}:pixelsize={size}:weight={weight}:slant={slant}{}",
            Self::RENDER
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

    /// Remplace les symboles absents de la police par un équivalent qu'elle
    /// a, au lieu d'un carré « ▯ » : « ＋ » et « ✕ » de la barre de menus de
    /// Notepad++ deviennent « + » et « × ».
    fn substitute<'t>(&self, font: *mut XftFont, text: &'t [u8]) -> std::borrow::Cow<'t, [u8]> {
        let Some(exists) = self.api.char_exists else {
            return text.into();
        };
        let Ok(s) = std::str::from_utf8(text) else {
            return text.into();
        };
        let has = |c: char| unsafe { exists(self.display, font, c as u32) != 0 };
        if s.chars().all(|c| c.is_ascii() || has(c)) {
            return text.into();
        }
        let out: String = s
            .chars()
            .map(|c| {
                if c.is_ascii() || has(c) {
                    return c;
                }
                let alt = symbol_substitute(c).unwrap_or(c);
                if alt.is_ascii() || has(alt) {
                    alt
                } else {
                    c
                }
            })
            .collect();
        out.into_bytes().into()
    }

    fn advance(&self, font: *mut XftFont, text: &[u8]) -> (i32, i32) {
        let mut extents = XGlyphInfo::default();
        unsafe {
            (self.api.text_extents)(
                self.display,
                font,
                text.as_ptr(),
                text.len() as c_int,
                &mut extents,
            );
        }
        (extents.x_off as i32, extents.width as i32)
    }

    fn measure(&mut self, spec: &FontSpec, text: &[u8]) -> Option<(i32, i32, i32)> {
        let font = self.font(spec)?;
        let runs = self.runs(spec, text)?;
        let mut width = 0;
        let mut ink = 0;
        for (f, run) in &runs {
            let (adv, w) = self.advance(*f, run);
            ink = ink.max(width + w);
            width += adv;
        }
        unsafe { Some((width.max(ink), (*font).height.max(1), (*font).ascent.max(1))) }
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
        let Some(runs) = self.runs(spec, text) else {
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
            let mut pen = x;
            for (font, run) in &runs {
                (self.api.draw_string)(
                    draw,
                    &color,
                    *font,
                    pen,
                    baseline,
                    run.as_ptr(),
                    run.len() as c_int,
                );
                pen += self.advance(*font, run).0;
            }
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
            for (_, list) in self.fallbacks.drain() {
                for font in list {
                    (self.api.font_close)(self.display, font);
                }
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

/// Ce que l'on sait de la densité de l'écran, du plus au moins fiable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ScreenDensity {
    /// `Xft.dpi` choisi par l'utilisateur (Apparence XFCE, xrdb). 96 est la
    /// valeur par défaut de toutes les sessions : elle n'apprend rien.
    user_dpi: Option<f32>,
    /// Densité physique de l'écran principal, d'après sa taille en
    /// millimètres lue dans l'EDID (RandR).
    physical_dpi: Option<f32>,
    /// Hauteur de l'écran principal en pixels.
    height: u32,
}

/// Facteur d'échelle automatique, aux paliers de Windows (100 %, 125 %…).
///
/// Xorg annonce toujours 96 ppp sur sa fenêtre racine : la taille réelle vient
/// donc de l'EDID. La cible est d'environ 110 ppp effectifs, ce qui redonne les
/// réglages par défaut de Windows : 100 % pour un 24 pouces 1080p ou un 27
/// pouces 1440p, 125 % pour un portable 15,6 pouces 1080p, 150 % pour un 27
/// pouces 4K ou un 13 pouces 1080p, 250 % pour un portable 15,6 pouces 4K.
/// Sans aucune information physique (machine virtuelle, EDID absent), seule la
/// définition compte : 150 % dès 1440 lignes, 200 % en 4K.
fn automatic_scale(density: ScreenDensity) -> f32 {
    if let Some(dpi) = density.user_dpi.filter(|dpi| (*dpi - 96.0).abs() > 1.0) {
        return dpi_scale(dpi, None);
    }
    if let Some(dpi) = density.physical_dpi {
        return dpi_scale(dpi * 96.0 / 110.0, None);
    }
    if density.height >= 2000 {
        2.0
    } else if density.height >= 1400 {
        1.5
    } else {
        1.0
    }
}

/// Lit `Xft.dpi` dans la base de ressources X (propriété RESOURCE_MANAGER).
fn resource_dpi(resources: &str) -> Option<f32> {
    resources.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == "Xft.dpi")
            .then(|| value.trim().parse::<f32>().ok())
            .flatten()
            .filter(|dpi| dpi.is_finite() && (48.0..=480.0).contains(dpi))
    })
}

/// Densité physique d'après la largeur en pixels et en millimètres. Les EDID
/// fantaisistes (0 mm, ou 16x9 « cm » des projecteurs et de certaines
/// télévisions) sont écartés.
fn physical_dpi(width_px: u32, width_mm: u32) -> Option<f32> {
    if width_mm < 100 || width_px == 0 {
        return None;
    }
    let dpi = width_px as f32 * 25.4 / width_mm as f32;
    (50.0..=400.0).contains(&dpi).then_some(dpi)
}

/// Taille annoncée par la fenêtre racine. Xorg la calcule toujours pour 96
/// ppp, ce qui n'apprend rien ; une autre valeur vient d'une vraie mesure
/// (DisplaySize, `-dpi`, serveur X sans RandR physique).
fn root_dpi(width_px: u16, width_mm: u16) -> Option<f32> {
    physical_dpi(width_px as u32, width_mm as u32).filter(|dpi| (*dpi - 96.0).abs() > 1.5)
}

/// Interroge le serveur X : `Xft.dpi`, puis la taille physique de l'écran
/// principal (ou du premier écran branché) par RandR.
fn screen_density(
    conn: &RustConnection,
    screen: &x11rb::protocol::xproto::Screen,
) -> ScreenDensity {
    let mut density = screen_density_randr(conn, screen.root, screen.height_in_pixels);
    if density.physical_dpi.is_none() {
        density.physical_dpi = root_dpi(screen.width_in_pixels, screen.width_in_millimeters);
    }
    density
}

fn screen_density_randr(conn: &RustConnection, root: u32, root_height: u16) -> ScreenDensity {
    use x11rb::protocol::randr::{self, ConnectionExt as _};
    let mut density = ScreenDensity {
        height: root_height as u32,
        ..ScreenDensity::default()
    };
    if let Some(reply) = conn
        .get_property(
            false,
            root,
            x11rb::protocol::xproto::AtomEnum::RESOURCE_MANAGER,
            x11rb::protocol::xproto::AtomEnum::STRING,
            0,
            1 << 16,
        )
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    {
        density.user_dpi = resource_dpi(&String::from_utf8_lossy(&reply.value));
    }
    let randr_ok = conn
        .randr_query_version(1, 3)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|v| v.major_version > 1 || v.minor_version >= 3);
    if !randr_ok {
        return density;
    }
    let Some(resources) = conn
        .randr_get_screen_resources_current(root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return density;
    };
    let primary = conn
        .randr_get_output_primary(root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.output)
        .filter(|output| *output != 0);
    let outputs = primary.into_iter().chain(
        resources
            .outputs
            .iter()
            .copied()
            .filter(|o| Some(*o) != primary),
    );
    for output in outputs {
        let Some(info) = conn
            .randr_get_output_info(output, resources.config_timestamp)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            continue;
        };
        if info.connection != randr::Connection::CONNECTED || info.crtc == 0 {
            continue;
        }
        let Some(crtc) = conn
            .randr_get_crtc_info(info.crtc, resources.config_timestamp)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            continue;
        };
        // Écran tourné d'un quart de tour : la largeur physique suit la hauteur.
        let rotated = u16::from(crtc.rotation)
            & (u16::from(randr::Rotation::ROTATE90) | u16::from(randr::Rotation::ROTATE270))
            != 0;
        let width_px = if rotated { crtc.height } else { crtc.width };
        density.height = if rotated { crtc.width } else { crtc.height } as u32;
        density.physical_dpi = physical_dpi(width_px as u32, info.mm_width);
        break;
    }
    density
}

/// Jeu ou programme qui dessine lui-même (SDL, OpenGL, Vulkan, Direct3D) :
/// il n'est pas « DPI-aware » et appelle glViewport avec la taille logique de
/// sa fenêtre. Ses fenêtres restent donc à l'échelle 1 (pixels réels).
pub static RENDERS_ITSELF: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Imports qui désignent un programme dessinant lui-même sa fenêtre.
pub fn renders_itself(imports: &[String]) -> bool {
    imports.iter().any(|name| {
        let n = name.to_ascii_lowercase();
        n.starts_with("d3d")
            || n.starts_with("dxgi")
            || n == "ddraw.dll"
            || n == "opengl32.dll"
            || n.starts_with("vulkan-1")
            || n.starts_with("sdl2")
            || n.starts_with("sdl3")
            || n.starts_with("glfw")
            || n == "libglesv2.dll"
            || n == "libegl.dll"
    })
}

/// WINLUX_WINDOW_DIAG=1 : journal borné (200 lignes) des peintures X11
/// faites par Winlux, pour trouver ce qui recouvre une image GPU (#1421).
fn diag_paint(what: &str, hwnd: usize, xid: u32) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    if crate::dlls::user32::window_diag() && N.fetch_add(1, Ordering::Relaxed) < 200 {
        eprintln!("winlux[fenetre] peinture {what} hwnd={hwnd:#x} x={xid:#x}");
    }
}

/// SDL ou GLFW liés en statique (Xonotic : ni SDL2.dll ni opengl32.dll
/// dans les imports) : le nom de la classe de fenêtre qu'ils enregistrent,
/// « SDL_app » ou « GLFW30 », figure dans l'image (#1421).
///
/// Un seul passage pour les quatre formes (UTF-16 et UTF-8) : on ne
/// compare qu'aux octets « S » et « G ». Quatre balayages complets de
/// l'image doublaient le temps de lancement de chaque .exe (#1559).
pub fn statically_linked_renderer(bytes: &[u8]) -> bool {
    const NEEDLES: [&[u8]; 4] = [
        b"SDL_app\0",
        b"S\0D\0L\0_\0a\0p\0p\0\0\0",
        b"GLFW30\0",
        b"G\0L\0F\0W\x003\x000\0\0\0",
    ];
    let mut rest = bytes;
    while let Some(i) = rest.iter().position(|&b| b == b'S' || b == b'G') {
        let tail = &rest[i..];
        if NEEDLES.iter().any(|n| tail.starts_with(n)) {
            return true;
        }
        rest = &rest[i + 1..];
    }
    false
}

/// Comme `statically_linked_renderer`, limité aux sections de données
/// initialisées de l'image chargée (.rdata, .data) : les noms de classe
/// sont des chaînes constantes. Le code, les ressources et les relocations
/// ne sont pas lus, ce qui évite de toucher les pages d'un gros .exe
/// (claude.exe : 161 Mo de code) au lancement (#1559).
pub fn renderer_in_data_sections(image: &[u8], sections: &[crate::pe::Section]) -> bool {
    const INITIALIZED_DATA: u32 = 0x40;
    const CODE_OR_EXEC: u32 = 0x20 | 0x2000_0000;
    sections
        .iter()
        .filter(|s| s.characteristics & INITIALIZED_DATA != 0)
        .filter(|s| s.characteristics & CODE_OR_EXEC == 0)
        .filter(|s| {
            ![".rsrc", ".reloc", ".pdata", ".idata", ".edata", ".tls"].contains(&s.name.as_str())
        })
        .any(|s| {
            let start = s.rva as usize;
            let end = start.saturating_add(s.virtual_size.max(s.raw_size) as usize);
            image
                .get(start.min(image.len())..end.min(image.len()))
                .is_some_and(statically_linked_renderer)
        })
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
    /// Fenêtre X à laquelle Winlux a donné le focus en dernier (SetFocus).
    /// Le gestionnaire de fenêtres rend le focus à la fenêtre de premier
    /// niveau à chaque clic ; sous Windows, un clic dans la fenêtre active
    /// ne retire pas le focus au contrôle qui l'a (#1137).
    asked_focus: u32,
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
    /// Bandes du dégradé de fond par hauteur de fenêtre (voir `theme_bands`).
    theme_cache: RefCell<ThemeCache>,
    /// One bounded server-side cache for the large desktop background.
    bitmap_cache: RefCell<Option<BitmapCache>>,
    /// Rendu vectoriel antialiasé Xft/Fontconfig, si les bibliothèques hôte sont présentes.
    xft: Option<XftRenderer>,
    /// Facteur DPI appliqué aux coordonnées logiques Win32.
    scale: f32,
    /// Table de correspondance keycode X11 → keysyms (lue à la demande).
    keymap: HashMap<u8, Vec<u32>>,
    /// Raccourcis globaux pris sur la racine : (keycode, modificateurs X) →
    /// (MOD_* Win32, VK).
    hotkeys: HashMap<(u8, u16), (u32, u32)>,
    keyboard: Option<XkbKeyboard>,
    /// Touche morte en attente (^, ¨…), composée avec la frappe suivante.
    dead_keys: crate::intl::keyboard::DeadKeys,
    /// Tampon arrière de chaque fenêtre : la peinture y est composée puis
    /// recopiée d'un bloc, sans scintillement, et les Expose sont servis
    /// sans rejouer les commandes GDI.
    back: HashMap<usize, (u32, u16, u16)>,
    /// Fenêtres X où Direct3D présente (ce processus ou un autre : processus
    /// GPU de Chromium) : leur contenu ne vient pas du dessin GDI (#1374).
    direct3d: std::collections::HashSet<u32>,
    /// Enfants transparents (WS_CHILD et WS_EX_TRANSPARENT, hors
    /// contrôles intégrés) : fenêtres X « InputOnly », qui reçoivent la souris
    /// sans masquer ce qu'un autre processus présente dans le parent
    /// (fenêtre « Chrome Legacy Window » de Chromium, #1337).
    see_through: std::collections::HashSet<usize>,
    /// Fenêtres de premier niveau réduites par le gestionnaire de fenêtres,
    /// telles que le programme le sait (WM_SIZE SIZE_MINIMIZED envoyé, #1498).
    iconic: std::collections::HashSet<usize>,
    /// Barre de menus de chaque fenêtre qui en a une, et menu ouvert.
    menu_bars: HashMap<usize, (Vec<gui::MenuBarItem>, Option<usize>)>,
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
    /// Gestes du pavé tactile (XInput 2.4) disponibles sur ce serveur.
    gestures: bool,
    /// Atomes des valuateurs « Abs Pressure », « Abs Tilt X », « Abs Tilt Y »
    /// (0 si le serveur ne les connaît pas) ; périphériques déjà examinés,
    /// `None` quand ce n'est pas un stylet (#1199).
    pen_labels: [u32; 3],
    pen_devices: HashMap<u16, Option<crate::dlls::pointer::PenAxes>>,
    /// Échelle du pincement au dernier cran de zoom envoyé.
    pinch_last: f32,
    /// Dernier appui (fenêtre, bouton, instant, x, y) pour reconnaître un
    /// double-clic.
    last_click: Option<(usize, u8, std::time::Instant, i32, i32)>,
    /// Glisser-déposer reçu depuis les applis Linux (XDND).
    xdnd: Option<crate::dlls::xdnd::Xdnd>,
    /// Fenêtre possédée → propriétaire. openbox ne relève pas un
    /// transitoire avec sa propriétaire : Winlux le fait lui-même (#1068).
    owners: HashMap<usize, usize>,
}

impl X11Backend {
    /// WM_NORMAL_HINTS d'une fenêtre de premier niveau : position et taille
    /// voulues par le programme (USPosition | USSize | PPosition | PSize |
    /// PWinGravity). NorthWestGravity : la position est celle du cadre, barre
    /// de titre comprise, comme le rectangle d'une fenêtre Windows ; une
    /// fenêtre demandée en (0,0) garde ainsi sa barre de titre à l'écran.
    fn position_hints(&self, target: u32, x: i32, y: i32, width: u16, height: u16) {
        let mut hints = [0u32; 18];
        hints[0] = 1 | 2 | 4 | 8 | 0x200;
        hints[1] = x as u32;
        hints[2] = y as u32;
        hints[3] = width as u32;
        hints[4] = height as u32;
        hints[17] = 1;
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            target,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &hints,
        );
    }

    /// Axes de stylet du périphérique `device` (pression au moins), relevés
    /// une fois par XIQueryDevice.
    fn pen_axes(&mut self, device: u16) -> Option<crate::dlls::pointer::PenAxes> {
        if let Some(known) = self.pen_devices.get(&device) {
            return *known;
        }
        use x11rb::protocol::xinput::{ConnectionExt as _, DeviceClassData};
        let reply = self
            .conn
            .xinput_xi_query_device(device)
            .ok()
            .and_then(|c| c.reply().ok());
        let mut axes = crate::dlls::pointer::PenAxes::default();
        if let Some(info) = reply.as_ref().and_then(|r| r.infos.first()) {
            let fp = |v: x11rb::protocol::xinput::Fp3232| {
                v.integral as f64 + v.frac as f64 / 4_294_967_296.0
            };
            for class in &info.classes {
                if let DeviceClassData::Valuator(v) = &class.data {
                    let axis = Some((v.number, fp(v.min), fp(v.max)));
                    if v.label == self.pen_labels[0] {
                        axes.pressure = axis;
                    } else if v.label != 0 && v.label == self.pen_labels[1] {
                        axes.tilt_x = axis;
                    } else if v.label != 0 && v.label == self.pen_labels[2] {
                        axes.tilt_y = axis;
                    }
                }
            }
            let name = String::from_utf8_lossy(&info.name).to_lowercase();
            axes.eraser = name.contains("eraser") || name.contains("gomme");
        }
        let found = axes.pressure.is_some().then_some(axes);
        self.pen_devices.insert(device, found);
        found
    }

    /// Événement brut XInput : relève la pression et l'inclinaison d'un
    /// stylet pour le message souris qui suit (`pointer`, #1199).
    fn raw_pen_event(
        &mut self,
        e: &x11rb::protocol::xinput::RawButtonPressEvent,
        button: Option<bool>,
    ) {
        // Avec XIAllDevices, le maître répète l'événement de l'esclave.
        if e.deviceid != e.sourceid {
            return;
        }
        let Some(axes) = self.pen_axes(e.sourceid) else {
            crate::dlls::pointer::note_other_device();
            return;
        };
        let values: Vec<f64> = e
            .axisvalues
            .iter()
            .map(|v| v.integral as f64 + v.frac as f64 / 4_294_967_296.0)
            .collect();
        let (pressure, tilt_x, tilt_y) =
            crate::dlls::pointer::decode_valuators(&axes, &e.valuator_mask, &values);
        crate::dlls::pointer::note_pen(pressure, tilt_x, tilt_y, axes.eraser);
        if let Some(down) = button {
            crate::dlls::pointer::note_pen_button(e.detail, down);
        }
    }

    /// Ouvre la connexion X et prépare les atomes; `None` si pas de serveur.
    pub fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let setup = conn.setup();
        let scr = &setup.roots[screen];
        let colormap = scr.default_colormap;
        // WINLUX_UI_SCALE (Paramètres > Écran, session live) l'emporte ;
        // « auto » ou rien : densité réelle de l'écran.
        let requested_scale = std::env::var("WINLUX_UI_SCALE")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value > 0.0);
        let scale = match requested_scale {
            _ if RENDERS_ITSELF.load(std::sync::atomic::Ordering::Relaxed) => 1.0,
            Some(scale) => dpi_scale(96.0, Some(scale)),
            None => automatic_scale(screen_density(&conn, scr)),
        };
        gui::set_display_scale(scale);
        // Pincer pour zoomer : XInput 2.4 (Xorg 21.1+, pilote libinput).
        // Stylet : événements bruts XInput 2.2 sur la racine, qui donnent la
        // pression et l'inclinaison sans retirer les événements souris (#1199).
        let xi_version = {
            use x11rb::protocol::xinput::ConnectionExt as _;
            conn.xinput_xi_query_version(2, 4)
                .ok()
                .and_then(|c| c.reply().ok())
                .map_or((0, 0), |v| (v.major_version, v.minor_version))
        };
        let gestures = xi_version >= (2, 4);
        let pen_labels = [
            b"Abs Pressure".as_slice(),
            b"Abs Tilt X".as_slice(),
            b"Abs Tilt Y".as_slice(),
        ]
        .map(|name| {
            conn.intern_atom(true, name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map_or(0, |r| r.atom)
        });
        if xi_version >= (2, 2) && pen_labels[0] != 0 {
            use x11rb::protocol::xinput::{ConnectionExt as _, EventMask, XIEventMask};
            let _ = conn.xinput_xi_select_events(
                scr.root,
                &[EventMask {
                    deviceid: 0, // XIAllDevices : chaque périphérique esclave
                    mask: vec![
                        XIEventMask::RAW_MOTION
                            | XIEventMask::RAW_BUTTON_PRESS
                            | XIEventMask::RAW_BUTTON_RELEASE,
                    ],
                }],
            );
        }
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
        let xdnd = crate::dlls::xdnd::Xdnd::new(&conn);
        Some(X11Backend {
            conn,
            screen,
            colormap,
            wm_protocols,
            wm_delete,
            windows: HashMap::new(),
            by_xwin: HashMap::new(),
            asked_focus: 0,
            pending: Vec::new(),
            deferred_events: Vec::new(),
            content: HashMap::new(),
            fonts: HashMap::new(),
            color_cache: RefCell::new(HashMap::new()),
            theme_cache: RefCell::new(ThemeCache::default()),
            bitmap_cache: RefCell::new(None),
            xft: unsafe { XftRenderer::connect() },
            scale,
            keymap: HashMap::new(),
            hotkeys: HashMap::new(),
            keyboard: unsafe { XkbKeyboard::connect() },
            dead_keys: Default::default(),
            back: HashMap::new(),
            direct3d: std::collections::HashSet::new(),
            see_through: std::collections::HashSet::new(),
            iconic: std::collections::HashSet::new(),
            menu_bars: HashMap::new(),
            overlay: crate::overlay::Overlay::new(),
            x11_dirty: Cell::new(false),
            xft_dirty: Cell::new(false),
            x11_cursors: HashMap::new(),
            current_cursor: 0x0001_0001,
            pointer_style: (1, 0),
            gestures,
            pen_labels,
            pen_devices: HashMap::new(),
            pinch_last: 1.0,
            last_click: None,
            xdnd,
            owners: HashMap::new(),
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
        // Hauteur négative (taille des caractères, la forme habituelle des
        // LOGFONT) : on garde le signe. `max(1)` la ramenait à 1, donc à la
        // taille plancher de 9 pixels : le texte des applications était
        // dessiné et mesuré trop petit (onglets de FileZilla, #798).
        scaled.height = if spec.height < 0 {
            -self.scaled(-spec.height).max(1)
        } else {
            self.scaled(spec.height).max(1)
        };
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
        diag_paint("tampon", 0, w.window);
        self.before_x11();
        let _ = self
            .conn
            .copy_area(pixmap, w.window, w.gc, x, y, x, y, width, height);
        let _ = self.conn.flush();
    }

    /// Bandes du dégradé de fond pour une hauteur de fenêtre : (y, hauteur,
    /// pixel X11). Calculées une fois par hauteur et mises en cache ; le cache
    /// est vidé quand les préférences d'accessibilité (contraste) changent,
    /// `a11y::current` relisant le fichier de réglages sur sa date de
    /// modification. `draw` ne recalcule donc plus le thème à chaque flush.
    fn theme_bands(&self, height: u16) -> std::sync::Arc<Vec<(i16, u16, u32)>> {
        let contrast = crate::dlls::a11y::current().contrast;
        let mut cache = self.theme_cache.borrow_mut();
        if cache.contrast != contrast {
            cache.contrast = contrast;
            cache.bands.clear();
        }
        if let Some(bands) = cache.bands.get(&height) {
            return bands.clone();
        }
        let bands = std::sync::Arc::new(compute_theme_bands(height, contrast, |c| {
            self.alloc_color(c)
        }));
        cache.bands.insert(height, bands.clone());
        bands
    }

    /// Dessine un fond clair avec un dégradé discret, proche des surfaces
    /// d'application de Windows 11. Le contenu GDI invité est posé ensuite.
    fn paint_theme(&self, hwnd: usize, w: &XWin) {
        self.before_x11();
        // Un contrôle enfant est une fenêtre X opaque : il prend le fond uni
        // de son parent (boîte de dialogue effacée avec COLOR_BTNFACE), sinon
        // un contrôle transparent montrait un morceau du dégradé du thème,
        // plus foncé que la boîte autour (« À propos » de Notepad++, #745).
        let solid = if w.child {
            crate::dlls::gui::parent_backdrop_color(hwnd).map(|c| self.alloc_color(c))
        } else {
            None
        };
        let bands = match solid {
            Some(pixel) => std::sync::Arc::new(vec![(0i16, w.height.max(1), pixel)]),
            None => self.theme_bands(w.height),
        };
        for &(y, band_h, pixel) in bands.iter() {
            let _ = self
                .conn
                .change_gc(w.gc, &ChangeGCAux::new().foreground(pixel));
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
    /// Touche physique d'un VK (niveau non modifié), pour XGrabKey.
    fn vk_keycode(&mut self, vk: u32) -> Option<u8> {
        let (min, max) = {
            let setup = self.conn.setup();
            (setup.min_keycode, setup.max_keycode)
        };
        (min..=max).find(|&kc| {
            self.key_syms(kc).is_some_and(|syms| {
                syms.first().is_some_and(|&b| b != 0)
                    && key_levels_to_vk(syms[0], syms.get(1).copied().unwrap_or(0))
                        == Some(vk as usize)
            })
        })
    }

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

    /// Forme X de `window` : les rectangles logiques visibles, ou toute la
    /// fenêtre (`None`).
    fn apply_shape(&self, window: Window, width: u16, height: u16, rects: Option<&[Rect]>) {
        use x11rb::protocol::shape::{self, ConnectionExt as _};
        let rects: Vec<x11rb::protocol::xproto::Rectangle> = match rects {
            Some(list) => list
                .iter()
                .map(|r| {
                    let (x, y) = (self.scaled(r.left), self.scaled(r.top));
                    x11rb::protocol::xproto::Rectangle {
                        x: x.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                        y: y.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                        width: (self.scaled(r.right) - x).clamp(0, u16::MAX as i32) as u16,
                        height: (self.scaled(r.bottom) - y).clamp(0, u16::MAX as i32) as u16,
                    }
                })
                .collect(),
            None => vec![x11rb::protocol::xproto::Rectangle {
                x: 0,
                y: 0,
                width,
                height,
            }],
        };
        let _ = self.conn.shape_rectangles(
            shape::SO::SET,
            shape::SK::BOUNDING,
            x11rb::protocol::xproto::ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &rects,
        );
        let _ = self.conn.flush();
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
                diag_paint("expose", hwnd, e.window);
                if self.presented_by_direct3d(e.window) {
                    // Image de Direct3D : ni tampon GDI ni thème par-dessus,
                    // le programme redessine sur WM_PAINT (#1374).
                    return (e.count == 0).then_some(PendingEvent {
                        hwnd,
                        message: gui::WM_PAINT,
                        wparam: 0,
                        lparam: 0,
                    });
                }
                if self.is_frame(hwnd, e.window) {
                    if e.count == 0 {
                        self.paint_menu_bar(hwnd);
                    }
                    return None;
                }
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
                        self.paint_theme(hwnd, &w);
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
            Event::PropertyNotify(e) => self.on_property(e),
            Event::SelectionNotify(e) => {
                let root = self.conn.setup().roots[self.screen].root;
                let scale = self.scale;
                self.xdnd
                    .as_mut()?
                    .selection_notify(&self.conn, &e, scale, root)?
            }
            Event::ClientMessage(e) => {
                let hwnd = *self.by_xwin.get(&e.window)?;
                if let Some(xdnd) = self.xdnd.as_mut() {
                    if let Some(out) = xdnd.client_message(&self.conn, &e, hwnd) {
                        return out;
                    }
                }
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
                gui::set_cursor_screen_pos(
                    self.logical(e.root_x as i32),
                    self.logical(e.root_y as i32),
                );
                let hwnd = *self.by_xwin.get(&e.event)?;
                if self.is_frame(hwnd, e.event) {
                    // Clic dans la barre de menus : WM_NCLBUTTONDOWN sur
                    // HTMENU, en coordonnées écran, que DefWindowProc ouvre.
                    if e.detail != 1 {
                        return None;
                    }
                    return Some(PendingEvent {
                        hwnd,
                        message: 0x00A1,
                        wparam: 5,
                        lparam: pack_i16_pair(
                            self.logical(e.root_x as i32),
                            self.logical(e.root_y as i32),
                        ),
                    });
                }
                let x = self.logical(e.event_x as i32);
                let y = self.logical(e.event_y as i32);
                if input_diag() {
                    eprintln!("Winlux X11 button {} at {x},{y} hwnd {hwnd:#x}", e.detail);
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
                let now = std::time::Instant::now();
                let double = self.last_click.take().is_some_and(|(h, b, t, lx, ly)| {
                    h == hwnd
                        && b == e.detail
                        && now.duration_since(t).as_millis()
                            <= crate::dlls::mouse::double_click_time() as u128
                        && (x - lx).abs() <= 4
                        && (y - ly).abs() <= 4
                }) && gui::wants_double_clicks(hwnd);
                // Le clic qui conclut un double-clic n'en commence pas un autre.
                if !double {
                    self.last_click = Some((hwnd, e.detail, now, x, y));
                }
                let message = button_message(e.detail, true);
                Some(PendingEvent {
                    hwnd,
                    message: if double {
                        double_click_message(message)
                    } else {
                        message
                    },
                    wparam,
                    lparam,
                })
            }
            Event::XinputRawMotion(e) => {
                self.raw_pen_event(&e, None);
                None
            }
            Event::XinputRawButtonPress(e) => {
                self.raw_pen_event(&e, Some(true));
                None
            }
            Event::XinputRawButtonRelease(e) => {
                self.raw_pen_event(&e, Some(false));
                None
            }
            Event::XinputGesturePinchBegin(_) => {
                crate::power::note_activity();
                self.pinch_last = 1.0;
                None
            }
            Event::XinputGesturePinchUpdate(e) => {
                crate::power::note_activity();
                let hwnd = *self.by_xwin.get(&e.event)?;
                let scale = e.scale as f32 / 65536.0;
                let (delta, last) = pinch_step(self.pinch_last, scale)?;
                self.pinch_last = last;
                // Comme un pavé tactile de précision sous Windows : le
                // pincement devient Ctrl + molette (zoom des applications).
                let lparam =
                    pack_i16_pair(self.logical(e.root_x >> 16), self.logical(e.root_y >> 16));
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_MOUSEWHEEL,
                    wparam: ((delta as u16 as usize) << 16) | 0x0008,
                    lparam,
                })
            }
            Event::XinputGesturePinchEnd(_) => {
                self.pinch_last = 1.0;
                None
            }
            Event::ButtonRelease(e) => {
                gui::raw_mouse_button(e.detail, false);
                gui::set_cursor_screen_pos(
                    self.logical(e.root_x as i32),
                    self.logical(e.root_y as i32),
                );
                let hwnd = *self.by_xwin.get(&e.event)?;
                if self.is_frame(hwnd, e.event) {
                    return None;
                }
                let x = self.logical(e.event_x as i32);
                let y = self.logical(e.event_y as i32);
                if input_diag() {
                    eprintln!("Winlux X11 release {} at {x},{y} hwnd {hwnd:#x}", e.detail);
                }
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
                gui::set_cursor_screen_pos(
                    self.logical(e.root_x as i32),
                    self.logical(e.root_y as i32),
                );
                let hwnd = *self.by_xwin.get(&e.event)?;
                if self.is_frame(hwnd, e.event) {
                    return None;
                }
                let x = self.logical(e.event_x as i32);
                let y = self.logical(e.event_y as i32);
                if input_diag() && button_state(e.state) != 0 {
                    eprintln!("Winlux X11 drag {x},{y} hwnd {hwnd:#x}");
                }
                let lparam = ((y as isize) << 16) | (x as isize & 0xFFFF);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_MOUSEMOVE,
                    wparam: button_state(e.state),
                    lparam,
                })
            }
            Event::FocusIn(e) => {
                if input_diag() {
                    eprintln!(
                        "Winlux X11 focus-in xwin {:#x} hwnd {:#x} mode {:?} detail {:?}",
                        e.event,
                        self.by_xwin.get(&e.event).copied().unwrap_or(0),
                        e.mode,
                        e.detail
                    );
                }
                // NotifyGrab/NotifyUngrab : focus temporairement pris par un
                // menu ou un glisser (grab du clavier) ; la fenêtre ne perd
                // pas réellement le focus, ne pas le signaler.
                if focus_is_grab(e.mode) {
                    return None;
                }
                let hwnd = *self.by_xwin.get(&e.event)?;
                if self.is_frame(hwnd, e.event)
                    && e.detail == x11rb::protocol::xproto::NotifyDetail::INFERIOR
                {
                    return None;
                }
                // FocusIn périmé d'un contrôle : seul Winlux donne le focus X
                // à un contrôle, et sa dernière demande vise une autre fenêtre.
                // C'est l'écho d'une remise du focus (#1137) arrivé après un
                // clic sur un autre bouton : le traiter rendait le focus Win32
                // au bouton par défaut pendant l'appui, et le clic était perdu
                // (#1286).
                let child = self.windows.get(&hwnd).is_some_and(|w| w.child);
                if stale_child_focus(child, self.asked_focus, e.event) {
                    return None;
                }
                // Focus remonté d'un contrôle à sa fenêtre de premier niveau
                // sans SetFocus de Winlux : c'est le gestionnaire de fenêtres
                // (clic pour activer). Le contrôle garde le focus Win32 ; son
                // FocusOut a déjà rendu le focus X au contrôle (#1137).
                if e.detail == x11rb::protocol::xproto::NotifyDetail::INFERIOR
                    && self.asked_focus != e.event
                    && self.windows.get(&hwnd).is_some_and(|w| !w.child)
                    && self.focus_kept_by_child()
                {
                    return None;
                }
                // Virtual / NonlinearVirtual : le focus va à une fenêtre
                // descendante (un contrôle), qui reçoit son propre FocusIn.
                // Le traiter rendait le focus au dialogue, qui le redonnait
                // au contrôle, et le bouton cliqué perdait son appui (#709).
                if is_virtual_focus(e.detail) {
                    return None;
                }
                crate::dlls::rawinput::set_app_focus(true);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_SETFOCUS,
                    wparam: 0,
                    lparam: 0,
                })
            }
            Event::FocusOut(e) => {
                if input_diag() {
                    eprintln!(
                        "Winlux X11 focus-out xwin {:#x} hwnd {:#x} mode {:?} detail {:?}",
                        e.event,
                        self.by_xwin.get(&e.event).copied().unwrap_or(0),
                        e.mode,
                        e.detail
                    );
                }
                if focus_is_grab(e.mode) {
                    return None;
                }
                let hwnd = *self.by_xwin.get(&e.event)?;
                // Le focus passe du cadre à sa zone client : même fenêtre Win32.
                if self.is_frame(hwnd, e.event)
                    && e.detail == x11rb::protocol::xproto::NotifyDetail::INFERIOR
                {
                    return None;
                }
                // Le focus quittait un descendant : celui-ci a son FocusOut.
                if is_virtual_focus(e.detail) {
                    return None;
                }
                // Le gestionnaire de fenêtres donne le focus à la fenêtre de
                // premier niveau quand on clique dedans, alors que Winlux
                // l'avait mis sur ce contrôle. Sous Windows, le contrôle le
                // garde : le WM_KILLFOCUS refermait la liste déroulante d'une
                // ComboBox juste après son ouverture, et le choix d'un
                // élément à la souris restait sans effet (#1137).
                let child = self.windows.get(&hwnd).is_some_and(|w| w.child);
                if focus_taken_by_wm(e.detail, self.asked_focus, e.event, child) {
                    let _ = self.conn.set_input_focus(
                        x11rb::protocol::xproto::InputFocus::PARENT,
                        e.event,
                        0u32,
                    );
                    let _ = self.conn.flush();
                    return None;
                }
                // Le focus passe de la fenêtre de premier niveau au contrôle
                // qui le reprend (voir ci-dessus) : rien ne change côté Win32.
                if e.detail == x11rb::protocol::xproto::NotifyDetail::INFERIOR
                    && self.asked_focus != e.event
                    && self.windows.get(&hwnd).is_some_and(|w| !w.child)
                    && self.focus_kept_by_child()
                {
                    return None;
                }
                // Un FocusIn suit si le focus passe à une autre fenêtre du programme.
                crate::dlls::rawinput::set_app_focus(false);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_KILLFOCUS,
                    wparam: 0,
                    lparam: 0,
                })
            }
            Event::KeyPress(e) => {
                crate::power::note_activity();
                let state = u16::from(e.state) & !(LOCK_MASK | NUM_LOCK_MASK);
                if let Some(&(mods, vk)) = self.hotkeys.get(&(e.detail, state)) {
                    // Raccourci global : WM_HOTKEY, jamais de WM_KEYDOWN.
                    return Some(PendingEvent {
                        hwnd: 0,
                        message: gui::WM_HOTKEY,
                        wparam: vk as usize,
                        lparam: mods as isize,
                    });
                }
                gui::raw_key(e.detail, true);
                let hwnd = *self.by_xwin.get(&e.event)?;
                if input_diag() {
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
                // AltGr : sous Windows, Ctrl gauche + Alt droit. Vu comme Alt
                // seul, AltGr+3 (« # ») déclenchait le raccourci Alt+3 de
                // Notepad++ au lieu du caractère, et une autre combinaison
                // le fermait (#1151).
                if is_altgr_keysym(xkb.map(|(b, _, _, _)| b).or(base)) {
                    return Some(self.altgr_events(hwnd, true));
                }
                let extended = vk_is_extended(vk);
                let lparam = key_lparam(e.detail, false, extended);
                gui::update_async_key_state(vk, true);
                if let Some(side) = gui::sided_modifier(vk, lparam) {
                    gui::update_async_key_state(side, true);
                }
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
                // Touches mortes : « ^ » puis « e » donnent « ê », comme le
                // pilote clavier Windows (accent seul suivi de la lettre
                // quand ils ne se combinent pas).
                let typed = self
                    .dead_keys
                    .feed(sym.unwrap_or(0), ch.and_then(|c| char::from_u32(c as u32)));
                if let Some(prefix) = typed.prefix {
                    gui::set_key_prefix(hwnd, vk, lparam, prefix as u16);
                }
                let ch = typed.ch.and_then(|c| u16::try_from(c as u32).ok());
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
                let xkb = self
                    .keyboard
                    .as_ref()
                    .and_then(|k| k.translate(e.detail, e.state.into()));
                if is_altgr_keysym(
                    xkb.map(|(b, _, _, _)| b)
                        .or_else(|| syms.as_deref().and_then(base_keysym)),
                ) {
                    return Some(self.altgr_events(hwnd, false));
                }
                let vk = xkb
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
                let lparam = key_lparam(e.detail, true, vk_is_extended(vk));
                if let Some(side) = gui::sided_modifier(vk, lparam) {
                    gui::update_async_key_state(side, false);
                }
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_KEYUP,
                    wparam: vk,
                    lparam,
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

    /// AltGr enfoncé ou relâché : VK_CONTROL (Ctrl gauche) puis VK_MENU
    /// (Alt droit, étendu), comme le pilote clavier de Windows. Le premier
    /// message est rendu, le second suit dans le même lot.
    fn altgr_events(&mut self, hwnd: usize, down: bool) -> PendingEvent {
        for vk in [0x11, 0xA2, 0x12, 0xA5] {
            gui::update_async_key_state(vk, down);
        }
        let message = if down { gui::WM_KEYDOWN } else { gui::WM_KEYUP };
        let [first, second] = altgr_messages(down);
        self.deferred_events.push(PendingEvent {
            hwnd,
            message,
            wparam: second.0,
            lparam: second.1,
        });
        PendingEvent {
            hwnd,
            message,
            wparam: first.0,
            lparam: first.1,
        }
    }

    /// Vrai si le dernier SetFocus de Winlux visait un contrôle (fenêtre X
    /// enfant) encore connu : c'est lui qui garde le focus Win32.
    fn focus_kept_by_child(&self) -> bool {
        self.by_xwin
            .get(&self.asked_focus)
            .and_then(|h| self.windows.get(h))
            .is_some_and(|w| w.child && w.window == self.asked_focus)
    }

    /// Vrai si `xwin` est le cadre (barre de menus) de `hwnd`.
    fn is_frame(&self, hwnd: usize, xwin: Window) -> bool {
        self.windows
            .get(&hwnd)
            .is_some_and(|w| w.frame != 0 && w.frame == xwin)
    }

    /// Fenêtre X qui porte le titre et la taille totale (gestionnaire).
    /// Fenêtre X affichée (mappée et visible).
    fn is_viewable(&self, xwin: Window) -> bool {
        self.conn
            .get_window_attributes(xwin)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state == x11rb::protocol::xproto::MapState::VIEWABLE)
    }

    /// Demande au gestionnaire de passer la fenêtre au premier plan.
    fn restack_above(&self, xwin: Window) {
        let aux = ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::ABOVE);
        let _ = self.conn.configure_window(xwin, &aux);
    }

    /// Relève les fenêtres affichées que possède `owner`, puis les leurs.
    ///
    /// WM_TRANSIENT_FOR est aussi reposé : openbox lit ce lien quand il
    /// prend la boîte en charge, et une propriétaire pas encore affichée à
    /// ce moment lui est inconnue. Le lien était alors perdu, et un clic sur
    /// la fenêtre principale la repassait devant sa boîte (#1068).
    fn raise_owned(&self, owner: usize) {
        let mut todo = vec![owner];
        let mut seen = 0;
        while let Some(o) = todo.pop() {
            seen += 1;
            if seen > 64 {
                break;
            }
            let owner_top = match self.windows.get(&o) {
                Some(w) => Self::top_xwin(w),
                None => continue,
            };
            for (&owned, _) in self.owners.iter().filter(|(_, &ow)| ow == o) {
                if let Some(w) = self.windows.get(&owned) {
                    let top = Self::top_xwin(w);
                    let _ = self.conn.change_property32(
                        x11rb::protocol::xproto::PropMode::REPLACE,
                        top,
                        AtomEnum::WM_TRANSIENT_FOR,
                        AtomEnum::WINDOW,
                        &[owner_top],
                    );
                    if self.is_viewable(top) {
                        self.restack_above(top);
                        todo.push(owned);
                    }
                }
            }
        }
    }

    fn top_xwin(w: &XWin) -> Window {
        if w.frame != 0 {
            w.frame
        } else {
            w.window
        }
    }

    /// Dessine la barre de menus sur la fenêtre cadre (au-dessus de la zone client).
    fn paint_menu_bar(&mut self, hwnd: usize) {
        let Some(w) = self.windows.get(&hwnd).copied() else {
            return;
        };
        if w.frame == 0 || w.bar == 0 {
            return;
        }
        let Some((items, hot)) = self.menu_bars.get(&hwnd).cloned() else {
            return;
        };
        let target = XWin {
            window: w.frame,
            height: w.bar,
            ..w
        };
        self.before_x11();
        let full = Rect {
            left: 0,
            top: 0,
            right: w.width as i32,
            bottom: w.bar as i32,
        };
        self.paint_fill(&target, &full, theme::BG_SURFACE);
        let font = self.scaled_font(&crate::dlls::menus::bar_font());
        let bar = w.bar as i32;
        for (i, item) in items.iter().enumerate() {
            let left = self.scaled(item.left);
            let right = self.scaled(item.right);
            if hot == Some(i) {
                self.before_x11();
                let r = Rect {
                    left: left + 1,
                    top: 1,
                    right: right - 1,
                    bottom: bar - 1,
                };
                self.paint_fill(&target, &r, theme::HOVER);
            }
            let color = if item.enabled {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            };
            let (_, th) = self
                .xft
                .as_mut()
                .and_then(|x| x.measure(&font, item.label.as_bytes()))
                .map(|(w, h, _)| (w, h))
                .unwrap_or((0, font.height.abs()));
            let x = left + self.scaled(7);
            let y = ((bar - th) / 2).max(0);
            self.paint_text(&target, x, y, &item.label, color, None, &font);
        }
        self.before_x11();
        let _ = self.conn.flush();
    }

    fn direct3d_atom(&self) -> Option<Atom> {
        self.conn
            .intern_atom(false, b"_WINLUX_DIRECT3D")
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    }

    /// La fenêtre X reçoit-elle les images d'une chaîne d'échange Direct3D,
    /// d'après la marque `_WINLUX_DIRECT3D` posée par le présentateur (#1374) ?
    fn presented_by_direct3d(&mut self, xwin: u32) -> bool {
        if self.direct3d.contains(&xwin) {
            return true;
        }
        let Some(atom) = self.direct3d_atom() else {
            return false;
        };
        let marked = self
            .conn
            .get_property(false, xwin, atom, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.value_len > 0);
        if marked {
            self.direct3d.insert(xwin);
        }
        marked
    }

    /// Crée, déplace ou retire la zone client placée sous la barre de menus.
    fn layout_menu_frame(&mut self, hwnd: usize, bar: u16) {
        let Some(w) = self.windows.get(&hwnd).copied() else {
            return;
        };
        if w.child {
            return;
        }
        if w.frame == 0 {
            if bar == 0 {
                return;
            }
            // La fenêtre existante devient le cadre ; une sous-fenêtre client
            // la recouvre sous la barre et reçoit dessin, souris et enfants.
            let frame = w.window;
            let Ok(client) = self.conn.generate_id() else {
                return;
            };
            let visual = self.conn.setup().roots[self.screen].root_visual;
            let bg = self.alloc_color(theme::BG_DEEP);
            let height = w.height.saturating_sub(bar).max(1);
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
            if self
                .conn
                .create_window(
                    0,
                    client,
                    frame,
                    0,
                    bar as i16,
                    w.width.max(1),
                    height,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    visual,
                    &aux,
                )
                .is_err()
            {
                return;
            }
            // Les enfants déjà créés passent dans la zone client.
            if let Ok(Ok(tree)) = self.conn.query_tree(frame).map(|c| c.reply()) {
                for sub in tree.children {
                    if sub == client {
                        continue;
                    }
                    let pos = self
                        .by_xwin
                        .get(&sub)
                        .and_then(|h| self.windows.get(h))
                        .map(|c| (c.x as i16, c.y as i16))
                        .unwrap_or((0, 0));
                    let _ = self.conn.reparent_window(sub, client, pos.0, pos.1);
                }
            }
            let _ = self.conn.map_window(client);
            if let Some((pixmap, _, _)) = self.back.remove(&hwnd) {
                let _ = self.conn.free_pixmap(pixmap);
            }
            self.by_xwin.insert(client, hwnd);
            if let Some(x) = self.windows.get_mut(&hwnd) {
                x.frame = frame;
                x.window = client;
                x.bar = bar;
                x.height = height;
            }
            let _ = self.conn.flush();
            return;
        }
        if w.bar == bar {
            return;
        }
        let total = w.height as u32 + w.bar as u32;
        let height = (total.saturating_sub(bar as u32)).max(1);
        let aux = ConfigureWindowAux::new().y(bar as i32).height(height);
        let _ = self.conn.configure_window(w.window, &aux);
        if let Some(x) = self.windows.get_mut(&hwnd) {
            x.bar = bar;
            x.height = height.min(u16::MAX as u32) as u16;
        }
        let _ = self.conn.flush();
    }

    /// WM_STATE changé : le gestionnaire de fenêtres a réduit ou restauré la
    /// fenêtre (bouton de la barre des tâches, wmctrl, Réduire du programme).
    /// Windows passe alors par SetWindowPos : WM_SIZE SIZE_MINIMIZED, puis
    /// SIZE_RESTORED ou SIZE_MAXIMIZED au retour. Chromium n'y remet son rendu
    /// en marche qu'ainsi : réactivée, la fenêtre de Claude restait vide
    /// (#1498).
    fn on_property(
        &mut self,
        e: x11rb::protocol::xproto::PropertyNotifyEvent,
    ) -> Option<PendingEvent> {
        let hwnd = *self.by_xwin.get(&e.window)?;
        let w = self.windows.get(&hwnd)?;
        if w.child || Self::top_xwin(w) != e.window {
            return None;
        }
        let wm_state = self
            .conn
            .intern_atom(true, b"WM_STATE")
            .ok()
            .and_then(|r| r.reply().ok())
            .map(|r| r.atom)?;
        if e.atom != wm_state {
            return None;
        }
        let (width, height) = (w.width, w.height);
        let state = gui::GuiBackend::window_state(self, hwnd);
        let minimized = state == gui::WINDOW_MINIMIZED;
        if minimized == self.iconic.contains(&hwnd) {
            return None;
        }
        let lparam = if minimized {
            self.iconic.insert(hwnd);
            0
        } else {
            self.iconic.remove(&hwnd);
            let lw = self.logical(width as i32).clamp(1, u16::MAX as i32);
            let lh = self.logical(height as i32).clamp(1, u16::MAX as i32);
            ((lh as isize) << 16) | (lw as isize & 0xFFFF)
        };
        Some(PendingEvent {
            hwnd,
            message: gui::WM_SIZE,
            wparam: gui::size_kind(state),
            lparam,
        })
    }

    fn on_configure(&mut self, e: ConfigureNotifyEvent) -> Option<PendingEvent> {
        let hwnd = *self.by_xwin.get(&e.window)?;
        let (old_x, old_y, old_w, old_h, window, root, child, frame, bar) = {
            let w = self.windows.get(&hwnd)?;
            (
                w.x,
                w.y,
                w.width,
                w.height,
                w.window,
                self.conn.setup().roots[self.screen].root,
                w.child,
                w.frame,
                w.bar,
            )
        };
        if frame != 0 && e.window != frame {
            // Zone client sous la barre : suivie à partir du cadre.
            return None;
        }
        let window = if frame != 0 { frame } else { window };
        // Avec une barre de menus, la zone client prend la hauteur restante.
        let client_h = e.height.saturating_sub(bar).max(1);
        if frame != 0 && (e.width != old_w || client_h != old_h) {
            let aux = ConfigureWindowAux::new()
                .width(e.width.max(1) as u32)
                .height(client_h as u32);
            let _ = self
                .conn
                .configure_window(self.windows.get(&hwnd)?.window, &aux);
            let _ = self.conn.flush();
        }
        let e = ConfigureNotifyEvent {
            height: if frame != 0 { client_h } else { e.height },
            ..e
        };
        // ConfigureNotify.x/y sont relatifs au parent; traduire l'origine vers
        // la racine couvre les fenêtres reparentées par un gestionnaire X11.
        // Un contrôle enfant garde ses coordonnées relatives au parent (Win32).
        let (x, y) = if child {
            (e.x as i32, e.y as i32)
        } else {
            self.conn
                .translate_coordinates(window, root, 0, 0)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| (reply.dst_x as i32, reply.dst_y as i32))
                .unwrap_or((e.x as i32, e.y as i32))
        };
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
            // SIZE_MAXIMIZED ou SIZE_MINIMIZED quand le gestionnaire a
            // agrandi ou réduit la fenêtre (#1328).
            let wparam = if child {
                0
            } else {
                gui::size_kind(gui::GuiBackend::window_state(self, hwnd))
            };
            return Some(PendingEvent {
                hwnd,
                message: gui::WM_SIZE,
                wparam,
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

/// Pincement : un cran de zoom (±120) chaque fois que l'échelle a varié
/// de 10 % depuis le dernier cran. Renvoie (delta, nouvelle référence).
fn pinch_step(last: f32, scale: f32) -> Option<(i16, f32)> {
    const STEP: f32 = 1.1;
    if !scale.is_finite() || scale <= 0.0 {
        None
    } else if scale >= last * STEP {
        Some((120, last * STEP))
    } else if scale <= last / STEP {
        Some((-120, last / STEP))
    } else {
        None
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

/// WM_xBUTTONDOWN → WM_xBUTTONDBLCLK (0x201→0x203, 0x204→0x206, 0x207→0x209).
fn double_click_message(down: u32) -> u32 {
    down + 2
}

/// Boutons X11 4/5 (molette verticale) et 6/7 (horizontale) : un cran vaut
/// `WHEEL_DELTA` (120), positif vers le haut ou vers la droite.
/// WINLUX_INPUT_DIAG : journal des boutons, relâchements et changements de
/// focus X reçus (#1286, glisser de Sumatra au banc).
fn input_diag() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("WINLUX_INPUT_DIAG").is_some_and(|v| !v.is_empty()))
}

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
        // Pavé numérique (#1378) : opérateurs, Entrée, chiffres et, sans
        // Verr. Num, touches de déplacement.
        0xFF8D => gui::VK_RETURN,                          // KP_Enter
        0xFFAA => 0x6A,                                    // VK_MULTIPLY
        0xFFAB => 0x6B,                                    // VK_ADD
        0xFFAC => 0x6C,                                    // VK_SEPARATOR
        0xFFAD => 0x6D,                                    // VK_SUBTRACT
        0xFFAE => 0x6E,                                    // VK_DECIMAL
        0xFFAF => 0x6F,                                    // VK_DIVIDE
        0xFFB0..=0xFFB9 => 0x60 + (sym - 0xFFB0) as usize, // VK_NUMPAD0..9
        0xFF95 => 0x24,                                    // KP_Home
        0xFF96 => 0x25,                                    // KP_Left
        0xFF97 => 0x26,                                    // KP_Up
        0xFF98 => 0x27,                                    // KP_Right
        0xFF99 => 0x28,                                    // KP_Down
        0xFF9A => 0x21,                                    // KP_Prior
        0xFF9B => 0x22,                                    // KP_Next
        0xFF9C => 0x23,                                    // KP_End
        0xFF9D => 0x0C,                                    // KP_Begin / VK_CLEAR
        0xFF9E => 0x2D,                                    // KP_Insert
        0xFF9F => 0x2E,                                    // KP_Delete
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

const LOCK_MASK: u16 = 1 << 1;
const NUM_LOCK_MASK: u16 = 1 << 4;

/// MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN → Mod1, Control, Shift, Mod4.
fn hotkey_x_mods(mods: u32) -> u16 {
    let mut x = 0;
    if mods & 1 != 0 {
        x |= 1 << 3;
    }
    if mods & 2 != 0 {
        x |= 1 << 2;
    }
    if mods & 4 != 0 {
        x |= 1;
    }
    if mods & 8 != 0 {
        x |= 1 << 6;
    }
    x
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
/// lParam d'un WM_KEYDOWN/UP : bits 16-23 = scancode PS/2 (jeu 1), comme
/// sous Windows, et non le keycode X (evdev + 8) ; bit 24 = touche étendue.
/// ToAscii/ToUnicode lisent ce scancode : PuTTY le leur repasse, et un
/// keycode X brut y faisait taper « l » pour « a » (#773).
fn key_lparam(keycode: u8, up: bool, extended: bool) -> isize {
    let (code, e0) = (keycode as u16)
        .checked_sub(8)
        .and_then(crate::dlls::rawinput::scancode)
        .unwrap_or((0, false));
    let scan = ((code as isize & 0xFF) << 16) | if extended || e0 { 1 << 24 } else { 0 };
    if up {
        scan | (1 << 30) | (1 << 31)
    } else {
        scan | 1
    }
}

/// Logo Winlux en 16, 32 et 48 px au format _NET_WM_ICON (largeur, hauteur,
/// puis pixels ARGB), décodé une fois.
pub(crate) fn winlux_window_icon() -> &'static [u32] {
    static ICON: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        let mut out = Vec::new();
        for png in [
            &include_bytes!("../../../assets/brand/winlux-logo-16.png")[..],
            &include_bytes!("../../../assets/brand/winlux-logo-32.png")[..],
            &include_bytes!("../../../assets/brand/winlux-logo-48.png")[..],
        ] {
            let Some(image) = super::icons::decode_png_bytes(png) else {
                continue;
            };
            out.push(image.w as u32);
            out.push(image.h as u32);
            // Image : 0xAABBGGRR ; _NET_WM_ICON : 0xAARRGGBB.
            out.extend(
                image
                    .px
                    .iter()
                    .map(|&p| (p & 0xFF00_FF00) | ((p & 0xFF) << 16) | ((p >> 16) & 0xFF)),
            );
        }
        out
    })
}

#[cfg(test)]
mod tests {
    /// #1421 : SDL lié en statique est reconnu à sa classe de fenêtre.
    #[test]
    fn sdl_statique_reconnu() {
        use super::statically_linked_renderer;
        let mut image = vec![0u8; 64];
        assert!(!statically_linked_renderer(&image));
        image.extend(
            "SDL_app"
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes),
        );
        image.extend([1, 2, 3]);
        assert!(statically_linked_renderer(&image));
        assert!(statically_linked_renderer(b"..SDL_app\0.."));
        assert!(!statically_linked_renderer(b"SDL_application"));
        let glfw: Vec<u8> = "GLFW30"
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(statically_linked_renderer(&glfw));
        assert!(statically_linked_renderer(b"xxGLFW30\0"));
        assert!(!statically_linked_renderer(b"GLFW3"));
    }

    #[test]
    fn classe_cherchee_dans_les_seules_sections_de_donnees() {
        // #1559 : le code et les ressources ne sont pas lus au lancement.
        use super::renderer_in_data_sections;
        use crate::pe::Section;
        let section = |name: &str, rva: u32, size: u32, characteristics: u32| Section {
            name: name.into(),
            virtual_size: size,
            rva,
            raw_size: size,
            raw_ptr: 0,
            characteristics,
        };
        let mut image = vec![0u8; 0x3000];
        image[0x1100..0x1108].copy_from_slice(b"SDL_app\0");
        let text = section(".text", 0x1000, 0x1000, 0x6000_0020);
        let rdata = section(".rdata", 0x2000, 0x1000, 0x4000_0040);
        assert!(!renderer_in_data_sections(
            &image,
            &[text.clone(), rdata.clone()]
        ));
        image[0x2200..0x2208].copy_from_slice(b"SDL_app\0");
        assert!(renderer_in_data_sections(&image, &[text, rdata.clone()]));
        let rsrc = section(".rsrc", 0x2000, 0x1000, 0x4000_0040);
        assert!(!renderer_in_data_sections(&image, &[rsrc]));
        // Section qui dépasse l'image : bornée, sans panique.
        let long = section(".data", 0x2800, 0x10_0000, 0xC000_0040);
        assert!(!renderer_in_data_sections(&image, &[long]));
    }

    #[test]
    fn symboles_remplaces_par_un_equivalent() {
        assert_eq!(super::symbol_substitute('\u{FF0B}'), Some('+'));
        assert_eq!(super::symbol_substitute('\u{2715}'), Some('\u{00D7}'));
        assert_eq!(super::symbol_substitute('é'), None);
    }

    use super::*;

    #[test]
    fn icone_winlux_publiee_en_trois_tailles() {
        // #1425 : _NET_WM_ICON = largeur, hauteur, pixels ARGB, 16/32/48 px.
        let icon = super::winlux_window_icon();
        let mut at = 0;
        let mut sizes = Vec::new();
        while at + 2 <= icon.len() {
            let (w, h) = (icon[at] as usize, icon[at + 1] as usize);
            sizes.push((w, h));
            at += 2 + w * h;
        }
        assert_eq!(sizes, [(16, 16), (32, 32), (48, 48)]);
        assert_eq!(at, icon.len());
        // Le logo a des pixels opaques et transparents (fond non rempli).
        let pixels = &icon[2 + 16 * 16 + 2..2 + 16 * 16 + 2 + 32 * 32];
        assert!(pixels.iter().any(|p| p >> 24 == 0xFF));
        assert!(pixels.iter().any(|p| p >> 24 == 0));
    }

    #[test]
    fn altgr_est_ctrl_gauche_plus_alt_droit() {
        assert!(is_altgr_keysym(Some(0xFE03)));
        assert!(is_altgr_keysym(Some(0xFF7E)));
        assert!(!is_altgr_keysym(Some(0xFFE9))); // Alt_L
        let [ctrl, alt] = altgr_messages(true);
        assert_eq!(ctrl.0, 0x11);
        assert_eq!(gui::sided_modifier(ctrl.0, ctrl.1), Some(0xA2));
        assert_eq!(alt.0, 0x12);
        assert_eq!(gui::sided_modifier(alt.0, alt.1), Some(0xA5));
        let [first, second] = altgr_messages(false);
        assert_eq!((first.0, second.0), (0x12, 0x11));
        assert!(first.1 & (1 << 31) != 0 && second.1 & (1 << 31) != 0);
        // Ctrl et Alt enfoncés : un raccourci Alt+3 (Notepad++) ne répond
        // pas à AltGr+3, qui tape « # » sur un clavier français (#1151).
        let alt_3 = [crate::dlls::accel::Accel {
            virt: 0x01 | 0x10, // FVIRTKEY | FALT
            key: 0x33,
            cmd: 42,
        }];
        let mods = (false, true, true);
        assert_eq!(
            crate::dlls::accel::match_accel(&alt_3, gui::WM_KEYDOWN, 0x33, mods),
            None
        );
        assert_eq!(
            crate::dlls::accel::match_accel(&alt_3, gui::WM_KEYDOWN, 0x33, (false, false, true)),
            Some(42)
        );
    }

    #[test]
    fn police_par_defaut_de_wx_rendue_en_segoe_ui() {
        assert_eq!(xft_family("fixed"), "Segoe UI");
        assert_eq!(xft_family(""), "Segoe UI");
        assert_eq!(xft_family("MS Shell Dlg 2"), "Segoe UI");
        assert_eq!(xft_family("Consolas"), "Consolas");
    }

    #[test]
    fn clic_du_gestionnaire_ne_retire_pas_le_focus_au_controle() {
        use x11rb::protocol::xproto::NotifyDetail;
        // Le gestionnaire active la fenêtre de premier niveau : le focus
        // remonte du contrôle que Winlux avait choisi, qui le garde.
        assert!(focus_taken_by_wm(NotifyDetail::ANCESTOR, 7, 7, true));
        // Le programme a lui-même donné le focus ailleurs.
        assert!(!focus_taken_by_wm(NotifyDetail::ANCESTOR, 3, 7, true));
        // Le focus part vers une autre fenêtre : vraie perte de focus.
        assert!(!focus_taken_by_wm(NotifyDetail::NONLINEAR, 7, 7, true));
        // Fenêtre de premier niveau : pas concernée.
        assert!(!focus_taken_by_wm(NotifyDetail::ANCESTOR, 7, 7, false));
    }

    #[test]
    fn focus_perime_d_un_controle_ignore() {
        // « Oui » (7) reprend le focus X après que le clic sur « Non » (9)
        // l'a demandé : l'écho est périmé et ne rend pas le focus à « Oui ».
        assert!(stale_child_focus(true, 9, 7));
        // Focus X donné au contrôle visé par le dernier SetFocus.
        assert!(!stale_child_focus(true, 9, 9));
        // Fenêtre de premier niveau : le gestionnaire peut l'activer.
        assert!(!stale_child_focus(false, 9, 7));
    }

    #[test]
    fn pinch_becomes_ctrl_wheel_steps() {
        // Écarter les doigts : un cran de zoom tous les 10 %.
        assert_eq!(pinch_step(1.0, 1.05), None);
        let (d, last) = pinch_step(1.0, 1.12).unwrap();
        assert_eq!(d, 120);
        assert!((last - 1.1).abs() < 1e-5);
        assert_eq!(pinch_step(last, 1.15), None);
        // Rapprocher : cran négatif.
        assert_eq!(pinch_step(1.0, 0.85).map(|v| v.0), Some(-120));
        assert_eq!(pinch_step(1.0, f32::NAN), None);
        assert_eq!(pinch_step(1.0, 0.0), None);
    }

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
        // Pavé numérique : « + » donne VK_ADD, pas la touche V (#1378).
        assert_eq!(keysym_to_vk(0xFFAB), Some(0x6B));
        assert_eq!(keysym_to_vk(0xFFAD), Some(0x6D));
        assert_eq!(keysym_to_vk(0xFFB7), Some(0x67));
        assert_eq!(keysym_to_vk(0xFF8D), Some(gui::VK_RETURN));
        assert_eq!(key_levels_to_vk(0xFF9C, 0xFFB1), Some(0x23));
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

        // Keycode X 38 = evdev 30 = scancode 0x1E (touche A en QWERTY).
        assert_eq!(key_lparam(38, false, false), (0x1Eisize << 16) | 1);
        // Keycode X 113 = evdev 105 = flèche gauche, E0 4B.
        assert_eq!(
            key_lparam(113, true, true),
            (0x4Bisize << 16) | (1 << 24) | (1 << 30) | (1 << 31),
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
    fn echelle_automatique_suit_la_taille_reelle_de_l_ecran() {
        let ecran = |pixels: u32, mm: u32, lignes: u32| ScreenDensity {
            user_dpi: None,
            physical_dpi: physical_dpi(pixels, mm),
            height: lignes,
        };
        // 24 pouces 1080p (531 mm) et 27 pouces 1440p (597 mm) : 100 %.
        assert_eq!(automatic_scale(ecran(1920, 531, 1080)), 1.0);
        assert_eq!(automatic_scale(ecran(2560, 597, 1440)), 1.0);
        // Portable 15,6 pouces 1080p (344 mm) : 125 %.
        assert_eq!(automatic_scale(ecran(1920, 344, 1080)), 1.25);
        // Portable 13,3 pouces 1080p (294 mm) et 27 pouces 4K (597 mm) : 150 %.
        assert_eq!(automatic_scale(ecran(1920, 294, 1080)), 1.5);
        assert_eq!(automatic_scale(ecran(3840, 597, 2160)), 1.5);
        // Portable 15,6 pouces 4K : 250 %.
        assert_eq!(automatic_scale(ecran(3840, 344, 2160)), 2.5);
        // Portable 1366x768 (15,6 pouces) : 100 %.
        assert_eq!(automatic_scale(ecran(1366, 344, 768)), 1.0);
        // EDID absent ou fantaisiste (projecteur « 16x9 ») : définition seule.
        assert_eq!(automatic_scale(ecran(3840, 16, 2160)), 2.0);
        assert_eq!(automatic_scale(ecran(2560, 0, 1440)), 1.5);
        assert_eq!(automatic_scale(ecran(1920, 0, 1080)), 1.0);
        // Xft.dpi choisi par l'utilisateur : prioritaire, sauf la valeur 96
        // posée par défaut par toutes les sessions.
        let choisi = |dpi: f32| ScreenDensity {
            user_dpi: Some(dpi),
            ..ecran(1920, 531, 1080)
        };
        assert_eq!(automatic_scale(choisi(144.0)), 1.5);
        assert_eq!(
            automatic_scale(ScreenDensity {
                user_dpi: Some(96.0),
                ..ecran(3840, 597, 2160)
            }),
            1.5
        );
    }

    #[test]
    fn racine_a_96_ppp_n_apprend_rien() {
        assert_eq!(root_dpi(1920, 508), None); // valeur inventée par Xorg
        assert_eq!(root_dpi(3840, 0), None);
        let dpi = root_dpi(3840, 598).unwrap();
        assert!((dpi - 163.1).abs() < 0.5);
    }

    #[test]
    fn xft_dpi_est_lu_dans_les_ressources_x() {
        assert_eq!(
            resource_dpi("Xft.antialias:\t1\nXft.dpi:\t144\n"),
            Some(144.0)
        );
        assert_eq!(resource_dpi("Xft.dpi: 120.5"), Some(120.5));
        assert_eq!(resource_dpi("Xcursor.size: 24\n"), None);
        assert_eq!(resource_dpi("Xft.dpi: abc\n"), None);
        assert_eq!(resource_dpi("Xft.dpi: 0\n"), None);
    }

    #[test]
    fn interpolation_bilineaire_des_bitmaps_est_stable() {
        let pixels = [0x0000_0000, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000];
        assert_eq!(bilinear_colorref(&pixels, 2, 2, 0.0, 0.0), 0x0000_0000);
        assert_eq!(bilinear_colorref(&pixels, 2, 2, 1.0, 1.0), 0x00ff_0000);
        assert_eq!(bilinear_colorref(&pixels, 2, 2, 0.5, 0.5), 0x0040_4040);
    }

    /// #1498 : réduite puis restaurée par le gestionnaire de fenêtres, la
    /// fenêtre reçoit WM_SIZE SIZE_MINIMIZED, puis SIZE_RESTORED avec sa
    /// taille. Il faut un gestionnaire de fenêtres (openbox) pour WM_STATE.
    #[test]
    #[ignore = "requires an X11 server and a window manager"]
    fn reduite_puis_restauree_par_le_gestionnaire_x11() {
        use crate::dlls::gui::GuiBackend;
        let mut backend = X11Backend::connect().expect("X11");
        let hwnd = gui::alloc_hwnd();
        gui::insert_window(hwnd, gui::tests::fenetre_essai(0x06c8_0000, 0, 0));
        backend.create_window(hwnd, "essai 1498", Rect::from_size(100, 100, 400, 300));
        backend.show_window(hwnd, true);
        let attendre = |b: &mut X11Backend, kind: usize| {
            let fin = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < fin {
                for e in b.pump() {
                    if e.hwnd == hwnd && e.message == gui::WM_SIZE && e.wparam == kind {
                        return Some(e.lparam);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            None
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        backend.set_window_state(hwnd, gui::WINDOW_MINIMIZED);
        assert_eq!(attendre(&mut backend, 1), Some(0), "SIZE_MINIMIZED");
        assert_eq!(backend.window_state(hwnd), gui::WINDOW_MINIMIZED);
        backend.set_window_state(hwnd, gui::WINDOW_NORMAL);
        let restored = attendre(&mut backend, 0).expect("SIZE_RESTORED");
        assert_eq!(restored & 0xFFFF, 400);
        backend.destroy_window(hwnd);
        gui::destroy_window(hwnd);
    }

    /// #1477 : WM_NCLBUTTONDOWN(HTCAPTION) → _NET_WM_MOVERESIZE ; le
    /// gestionnaire de fenêtres déplace la fenêtre tant que le bouton est
    /// tenu. Il faut openbox et xdotool.
    #[test]
    #[ignore = "requires an X11 server, a window manager and xdotool"]
    fn glisser_la_legende_deplace_la_fenetre_x11() {
        use crate::dlls::gui::GuiBackend;
        let xdotool = |args: &[&str]| {
            std::process::Command::new("xdotool")
                .args(args)
                .status()
                .expect("xdotool");
        };
        let mut backend = X11Backend::connect().expect("X11");
        let hwnd = gui::alloc_hwnd();
        gui::insert_window(hwnd, gui::tests::fenetre_essai(0x06c8_0000, 0, 0));
        backend.create_window(hwnd, "essai 1477", Rect::from_size(100, 100, 400, 300));
        backend.set_decorations(hwnd, 0);
        backend.show_window(hwnd, true);
        let pause = |ms| std::thread::sleep(std::time::Duration::from_millis(ms));
        let position = |b: &mut X11Backend| {
            for _ in 0..10 {
                b.pump();
                pause(30);
            }
            (b.windows[&hwnd].x, b.windows[&hwnd].y)
        };
        pause(300);
        let avant = position(&mut backend);
        xdotool(&[
            "mousemove",
            &(avant.0 + 200).to_string(),
            &(avant.1 + 10).to_string(),
        ]);
        xdotool(&["mousedown", "1"]);
        pause(100);
        backend.pump();
        backend.begin_move_resize(hwnd, 2);
        pause(200);
        for pas in 1..=10 {
            xdotool(&[
                "mousemove",
                &(avant.0 + 200 + pas * 15).to_string(),
                &(avant.1 + 10 + pas * 10).to_string(),
            ]);
            pause(30);
        }
        xdotool(&["mouseup", "1"]);
        pause(200);
        let apres = position(&mut backend);
        assert!(
            apres.0 > avant.0 + 100 && apres.1 > avant.1 + 60,
            "{avant:?} → {apres:?}"
        );
        backend.destroy_window(hwnd);
        gui::destroy_window(hwnd);
    }

    /// Un enfant WS_EX_TRANSPARENT devient une fenêtre X InputOnly : il ne
    /// masque pas l'image qu'un autre processus présente dans le parent
    /// (Chromium, #1337).
    #[test]
    #[ignore = "requires an X11 server"]
    fn enfant_transparent_en_inputonly_x11() {
        use crate::dlls::gui::GuiBackend;
        use x11rb::protocol::xproto::WindowClass;
        let mut backend = X11Backend::connect().expect("X11");
        let parent = gui::alloc_hwnd();
        gui::insert_window(parent, gui::tests::fenetre_essai(0x06c8_0000, 0, 0));
        backend.create_window(parent, "essai", Rect::from_size(0, 0, 200, 150));
        let legacy = gui::alloc_hwnd();
        gui::insert_window(legacy, gui::tests::fenetre_essai(0x4600_0000, 0x20, parent));
        backend.create_child_window(legacy, parent, Rect::from_size(0, 37, 200, 113));
        let opaque = gui::alloc_hwnd();
        gui::insert_window(opaque, gui::tests::fenetre_essai(0x4600_0000, 0, parent));
        backend.create_child_window(opaque, parent, Rect::from_size(0, 0, 20, 20));
        let class = |b: &X11Backend, h: usize| {
            let w = b.windows[&h].window;
            b.conn
                .get_window_attributes(w)
                .unwrap()
                .reply()
                .unwrap()
                .class
        };
        assert_eq!(class(&backend, legacy), WindowClass::INPUT_ONLY);
        assert_eq!(class(&backend, opaque), WindowClass::INPUT_OUTPUT);
        // Aucun dessin ne va dans la fenêtre InputOnly (pas d'erreur X).
        backend.draw(legacy, &[]);
        backend.conn.sync().unwrap();
        for h in [legacy, opaque, parent] {
            backend.destroy_window(h);
            gui::destroy_window(h);
        }
        backend.conn.sync().unwrap();
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
            child: false,
            frame: 0,
            bar: 0,
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
            child: false,
            frame: 0,
            bar: 0,
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

    /// #1357 : forme X posée sur la fenêtre d'un autre client X.
    #[test]
    #[ignore = "requires an X11 server; run with xvfb-run"]
    fn forme_posee_sur_une_fenetre_etrangere_x11() {
        use crate::dlls::gui::GuiBackend;
        use x11rb::protocol::shape::ConnectionExt as _;
        let mut backend = X11Backend::connect().expect("X11");
        let root = backend.conn.setup().roots[backend.screen].clone();
        let win = backend.conn.generate_id().unwrap();
        backend
            .conn
            .create_window(
                0,
                win,
                root.root,
                0,
                0,
                40,
                20,
                0,
                x11rb::protocol::xproto::WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap()
            .check()
            .unwrap();
        let s = backend.scaled(1);
        let rect = Rect {
            left: 10,
            top: 0,
            right: 30,
            bottom: 20,
        };
        backend.shape_foreign(win as u64, Some(&[rect]));
        let got = backend
            .conn
            .shape_get_rectangles(win, x11rb::protocol::shape::SK::BOUNDING)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(got.rectangles.len(), 1);
        assert_eq!(got.rectangles[0].x as i32, 10 * s);
        assert_eq!(got.rectangles[0].width as i32, 20 * s);
        backend.conn.destroy_window(win).unwrap();
    }

    /// #1374 : la marque Direct3D posée par un client (processus GPU) est
    /// lue par le propriétaire de la fenêtre, sur une autre connexion.
    #[test]
    #[ignore = "requires an X11 server; run with xvfb-run"]
    fn marque_direct3d_lue_par_le_proprietaire_x11() {
        use crate::dlls::gui::GuiBackend;
        let mut owner = X11Backend::connect().expect("X11");
        let mut gpu = X11Backend::connect().expect("X11");
        let root = owner.conn.setup().roots[owner.screen].root;
        let win = owner.conn.generate_id().unwrap();
        owner
            .conn
            .create_window(
                0,
                win,
                root,
                0,
                0,
                40,
                20,
                0,
                x11rb::protocol::xproto::WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().background_pixel(0xF3F3F3),
            )
            .unwrap()
            .check()
            .unwrap();
        assert!(!owner.presented_by_direct3d(win));
        gpu.mark_direct3d(win as u64);
        assert!(
            owner.presented_by_direct3d(win),
            "marque invisible du propriétaire"
        );
        owner.conn.destroy_window(win).unwrap();
    }

    /// #1345 : image posée par un autre processus (processus GPU de
    /// Chromium) sur un drawable X11 qui ne lui appartient pas.
    #[test]
    #[ignore = "requires an X11 server; run with xvfb-run"]
    fn image_posee_sur_un_drawable_etranger_x11() {
        use crate::dlls::gui::GuiBackend;
        let mut backend = X11Backend::connect().expect("X11");
        let root = backend.conn.setup().roots[backend.screen].clone();
        let pixmap = backend.conn.generate_id().unwrap();
        backend
            .conn
            .create_pixmap(root.root_depth, pixmap, root.root, 64, 64)
            .unwrap()
            .check()
            .unwrap();
        let rect = Rect {
            left: 2,
            top: 2,
            right: 6,
            bottom: 5,
        };
        let scaled = backend.scaled_rect(&rect);
        backend.paint_foreign(pixmap as u64, rect, &[0x0000_00ff; 12], gui::RasterOp::Copy);
        let pixel = backend
            .conn
            .get_image(
                x11rb::protocol::xproto::ImageFormat::Z_PIXMAP,
                pixmap,
                scaled.left as i16,
                scaled.top as i16,
                1,
                1,
                u32::MAX,
            )
            .unwrap()
            .reply()
            .unwrap();
        let visual = root
            .allowed_depths
            .iter()
            .flat_map(|d| &d.visuals)
            .find(|v| v.visual_id == root.root_visual)
            .unwrap();
        let mut value = [0u8; 4];
        let len = pixel.data.len().min(4);
        value[..len].copy_from_slice(&pixel.data[..len]);
        let value = if backend.conn.setup().image_byte_order
            == x11rb::protocol::xproto::ImageOrder::LSB_FIRST
        {
            u32::from_le_bytes(value)
        } else {
            u32::from_be_bytes(value) >> (8 * (4 - len))
        };
        assert_eq!(value & visual.red_mask, visual.red_mask);
        assert_eq!(value & visual.green_mask, 0);
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
        // Les événements déjà lus par x11rb (pendant une réponse) ne rendent
        // pas la socket lisible : on les prend d'abord sans bloquer.
        if self.poll_pending() {
            return true;
        }
        // Puis on dort sur la socket X11 jusqu'au prochain événement ou à
        // l'échéance, au lieu d'interroger le serveur toutes les 5 ms.
        let mut pfd = libc::pollfd {
            fd: self.conn.stream().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout = timeout_ms.min(i32::MAX as u32) as i32;
        // SAFETY : `pfd` est une entrée valide pour la durée de l'appel.
        unsafe { libc::poll(&mut pfd, 1, timeout) };
        self.poll_pending();
        // Backend réel : GetMessage doit continuer d'attendre.
        true
    }

    fn event_fd(&mut self) -> Option<i32> {
        Some(self.conn.stream().as_raw_fd())
    }

    fn poll_pending(&mut self) -> bool {
        // Une erreur (connexion perdue) arrête la lecture : la boucle de
        // messages se vide avec ce qui a déjà été lu.
        while let Ok(Some(ev)) = self.conn.poll_for_event() {
            self.pending.push(ev);
        }
        !self.pending.is_empty()
    }

    fn native_window(&mut self, hwnd: usize) -> Option<u64> {
        self.windows.get(&hwnd).map(|w| w.window as u64)
    }

    fn mark_direct3d(&mut self, xid: u64) {
        let Ok(xid) = u32::try_from(xid) else {
            return;
        };
        let Some(atom) = self.direct3d_atom() else {
            return;
        };
        use x11rb::wrapper::ConnectionExt as _;
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            xid,
            atom,
            AtomEnum::CARDINAL,
            &[1],
        );
        // Fond « None » : une exposition ne repeint pas le fond gris par-dessus
        // la dernière image présentée.
        let _ = self.conn.change_window_attributes(
            xid,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new().background_pixmap(0),
        );
        let _ = self.conn.flush();
        self.direct3d.insert(xid);
    }

    fn paint_foreign(&mut self, xid: u64, rect: Rect, pixels: &[u32], rop: gui::RasterOp) {
        let Ok(xid) = u32::try_from(xid) else {
            return;
        };
        let Some(g) = self
            .conn
            .get_geometry(xid)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return; // fenêtre détruite entre-temps
        };
        // L'image est encodée pour la profondeur de la racine.
        if g.depth != self.conn.setup().roots[self.screen].root_depth {
            return;
        }
        let Ok(gc) = self.conn.generate_id() else {
            return;
        };
        if !self
            .conn
            .create_gc(gc, xid, &CreateGCAux::new())
            .is_ok_and(|c| c.check().is_ok())
        {
            return;
        }
        let window = XWin {
            window: xid,
            gc,
            x: 0,
            y: 0,
            width: g.width,
            height: g.height,
            child: false,
            frame: 0,
            bar: 0,
        };
        let destination = self.scaled_rect(&rect);
        self.paint_bitmap(&window, &rect, &destination, pixels, rop);
        let _ = self.conn.free_gc(gc);
        let _ = self.conn.flush();
    }

    fn foreign_size(&mut self, xid: u64) -> Option<(i32, i32)> {
        let g = self.conn.get_geometry(xid as u32).ok()?.reply().ok()?;
        Some((
            self.logical(g.width as i32).max(1),
            self.logical(g.height as i32).max(1),
        ))
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

    fn work_area(&mut self) -> Option<Rect> {
        // _NET_WORKAREA du bureau courant : x, y, largeur, hauteur.
        let root = self.conn.setup().roots[self.screen].root;
        let atom = self
            .conn
            .intern_atom(true, b"_NET_WORKAREA")
            .ok()?
            .reply()
            .ok()?
            .atom;
        if atom == 0 {
            return None;
        }
        let reply = self
            .conn
            .get_property(false, root, atom, AtomEnum::CARDINAL, 0, 4)
            .ok()?
            .reply()
            .ok()?;
        let v: Vec<u32> = reply.value32()?.collect();
        if v.len() < 4 || v[2] == 0 || v[3] == 0 {
            return None;
        }
        let (x, y) = (v[0] as i32, v[1] as i32);
        Some(Rect {
            left: self.logical(x),
            top: self.logical(y),
            right: self.logical(x + v[2] as i32),
            bottom: self.logical(y + v[3] as i32),
        })
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
                | x11rb::protocol::xproto::EventMask::KEY_RELEASE
                // WM_STATE : réduite ou restaurée par le gestionnaire (#1498).
                | x11rb::protocol::xproto::EventMask::PROPERTY_CHANGE,
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

        if self.gestures {
            use x11rb::protocol::xinput::{ConnectionExt as _, EventMask, XIEventMask};
            // XI_GesturePinchBegin/Update/End (27..29), périphérique maître
            // « tous » (1) : absents des constantes de xcb-proto 1.15.
            let mask = XIEventMask::from((1u32 << 27) | (1 << 28) | (1 << 29));
            let _ = self.conn.xinput_xi_select_events(
                win,
                &[EventMask {
                    deviceid: 1,
                    mask: vec![mask],
                }],
            );
        }

        // Demande la fermeture propre via WM_DELETE_WINDOW.
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win,
            self.wm_protocols,
            AtomEnum::ATOM,
            &[self.wm_protocols, self.wm_delete],
        );
        if let Some(xdnd) = self.xdnd.as_ref() {
            xdnd.set_aware(&self.conn, win);
        }
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
        // Machine (WM_CLIENT_MACHINE, ICCCM) : sans elle, wmctrl affiche
        // « N/A » et des pagers ou lanceurs traitent mal nos fenêtres (#683).
        // _NET_WM_DESKTOP, lui, est posé par le gestionnaire de fenêtres sur
        // une fenêtre qu'il gère (test-pe-window-props.py le vérifie).
        let host = client_machine();
        let _ = self.conn.change_property8(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win,
            AtomEnum::WM_CLIENT_MACHINE,
            AtomEnum::STRING,
            host.as_bytes(),
        );
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
        // Icône de la fenêtre hors de la session Winlux (#1425) : sous KDE ou
        // un XFCE ordinaire, personne ne publie _NET_WM_ICON et la barre des
        // tâches montrait l'icône X11 par défaut. La session Winlux publie
        // elle-même l'icône extraite du programme (winlux-applications).
        if std::env::var("XDG_SESSION_DESKTOP").as_deref() != Ok("winlux") {
            if let Some(icon) = atom(b"_NET_WM_ICON") {
                let _ = self.conn.change_property32(
                    x11rb::protocol::xproto::PropMode::REPLACE,
                    win,
                    icon,
                    AtomEnum::CARDINAL,
                    winlux_window_icon(),
                );
            }
        }
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
        // Position voulue dès la création : une boîte de message n'est
        // jamais déplacée ensuite, et openbox la plaçait sinon à sa guise
        // (« Recharger » de Notepad++ en bas à gauche, #744).
        self.position_hints(win, x as i32, y as i32, w, h);

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
                child: false,
                frame: 0,
                bar: 0,
            },
        );
        self.by_xwin.insert(win, hwnd);
    }

    fn create_child_window(&mut self, hwnd: usize, parent: usize, rect: Rect) {
        let Some(parent_win) = self.windows.get(&parent).map(|w| w.window) else {
            // Parent inconnu du serveur X (headless, détruit) : fenêtre isolée.
            self.create_window(hwnd, "", rect);
            return;
        };
        if self.x11_cursors.is_empty() {
            self.init_cursors();
        }
        let visual = self.conn.setup().roots[self.screen].root_visual;
        let x = self
            .scaled(rect.left)
            .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let y = self
            .scaled(rect.top)
            .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let w = self.scaled(rect.width()).max(1).clamp(1, u16::MAX as i32) as u16;
        let h = self.scaled(rect.height()).max(1).clamp(1, u16::MAX as i32) as u16;
        let Ok(win) = self.conn.generate_id() else {
            return;
        };
        let see_through = gui::is_see_through_child(hwnd);
        let aux = if see_through {
            // InputOnly : ni fond ni dessin, la souris seulement.
            CreateWindowAux::new()
        } else {
            let bg = self.alloc_color(theme::BG_DEEP);
            CreateWindowAux::new().background_pixel(bg)
        };
        // Une fenêtre InputOnly n'est jamais exposée.
        let exposure = if see_through {
            x11rb::protocol::xproto::EventMask::NO_EVENT
        } else {
            x11rb::protocol::xproto::EventMask::EXPOSURE
        };
        let aux = aux.event_mask(
            exposure
                | x11rb::protocol::xproto::EventMask::STRUCTURE_NOTIFY
                | x11rb::protocol::xproto::EventMask::BUTTON_PRESS
                | x11rb::protocol::xproto::EventMask::BUTTON_RELEASE
                | x11rb::protocol::xproto::EventMask::POINTER_MOTION
                | x11rb::protocol::xproto::EventMask::FOCUS_CHANGE
                | x11rb::protocol::xproto::EventMask::KEY_PRESS
                | x11rb::protocol::xproto::EventMask::KEY_RELEASE,
        );
        let aux = match self.x11_cursors.get(&0x0001_0001).copied() {
            Some(cursor) => aux.cursor(cursor),
            None => aux,
        };
        if self
            .conn
            .create_window(
                0,
                win,
                parent_win,
                x,
                y,
                w,
                h,
                0,
                if see_through {
                    WindowClass::INPUT_ONLY
                } else {
                    WindowClass::INPUT_OUTPUT
                },
                if see_through { 0 } else { visual },
                &aux,
            )
            .is_err()
        {
            return;
        }
        let Ok(gc) = self.conn.generate_id() else {
            return;
        };
        // Un GC ne se crée pas sur une fenêtre InputOnly : celui du parent
        // (jamais utilisé, aucun dessin n'y va).
        let gc_drawable = if see_through { parent_win } else { win };
        let _ = self
            .conn
            .create_gc(gc, gc_drawable, &CreateGCAux::new().graphics_exposures(0));
        if see_through {
            self.see_through.insert(hwnd);
        }
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
                child: true,
                frame: 0,
                bar: 0,
            },
        );
        self.by_xwin.insert(win, hwnd);
    }

    fn draw(&mut self, hwnd: usize, cmds: &[DrawCmd]) {
        if self.see_through.contains(&hwnd) {
            return;
        }
        // Fenêtre où OpenGL ou Direct3D présente : la peinture GDI (thème,
        // tampon arrière) effacerait son image (#1421, #1374).
        if self
            .windows
            .get(&hwnd)
            .is_some_and(|w| self.direct3d.contains(&w.window))
        {
            self.content.insert(hwnd, cmds.to_vec());
            return;
        }
        self.content.insert(hwnd, cmds.to_vec());
        let Some(w) = self.windows.get(&hwnd).copied() else {
            return;
        };
        if let Some(pixmap) = self.back_buffer(hwnd, &w) {
            let target = XWin {
                window: pixmap,
                ..w
            };
            self.paint_theme(hwnd, &target);
            self.paint_cmds(&target, cmds);
            self.overlay.frame(hwnd);
            self.paint_overlay(&target, hwnd);
            self.present(&w, pixmap, 0, 0, w.width, w.height);
        } else {
            self.paint_theme(hwnd, &w);
            self.paint_cmds(&w, cmds);
            self.overlay.frame(hwnd);
            self.paint_overlay(&w, hwnd);
            self.before_x11();
            let _ = self.conn.flush();
        }
    }

    fn draw_over(&mut self, hwnd: usize, cmds: &[DrawCmd]) {
        if self.see_through.contains(&hwnd) {
            return;
        }
        if self
            .windows
            .get(&hwnd)
            .is_some_and(|w| self.direct3d.contains(&w.window))
        {
            return;
        }
        let Some(w) = self.windows.get(&hwnd).copied() else {
            return;
        };
        // Rien de peint encore, ou tampon à refaire (taille changée) : il
        // n'y a pas de contenu sur lequel poser ces commandes.
        let ready = self.content.contains_key(&hwnd)
            && self
                .back
                .get(&hwnd)
                .is_some_and(|&(_, width, height)| width == w.width && height == w.height);
        if !ready {
            let mut all = self.content.get(&hwnd).cloned().unwrap_or_default();
            all.extend_from_slice(cmds);
            gui::GuiBackend::draw(self, hwnd, &all);
            return;
        }
        // La liste rejouée sans tampon arrière grandit avec chaque dessin
        // partiel : au-delà d'un plafond, on garde les plus récents.
        const CONTENT_MAX: usize = 50_000;
        if let Some(content) = self.content.get_mut(&hwnd) {
            content.extend_from_slice(cmds);
            if content.len() > CONTENT_MAX {
                let excess = content.len() - CONTENT_MAX;
                content.drain(..excess);
            }
        }
        let Some(pixmap) = self.back_buffer(hwnd, &w) else {
            return;
        };
        let target = XWin {
            window: pixmap,
            ..w
        };
        self.paint_cmds(&target, cmds);
        self.overlay.frame(hwnd);
        self.paint_overlay(&target, hwnd);
        self.present(&w, pixmap, 0, 0, w.width, w.height);
    }

    fn set_menu_bar(
        &mut self,
        hwnd: usize,
        items: &[gui::MenuBarItem],
        height: i32,
        hot: Option<usize>,
    ) {
        let bar = if height > 0 && !items.is_empty() {
            self.scaled(height).clamp(1, u16::MAX as i32) as u16
        } else {
            0
        };
        if bar == 0 {
            self.menu_bars.remove(&hwnd);
        } else {
            self.menu_bars.insert(hwnd, (items.to_vec(), hot));
        }
        self.layout_menu_frame(hwnd, bar);
        self.paint_menu_bar(hwnd);
    }

    fn destroy_window(&mut self, hwnd: usize) {
        self.see_through.remove(&hwnd);
        self.iconic.remove(&hwnd);
        self.owners.remove(&hwnd);
        self.owners.retain(|_, o| *o != hwnd);
        self.content.remove(&hwnd);
        self.menu_bars.remove(&hwnd);
        self.overlay.forget(hwnd);
        if let Some((pixmap, _, _)) = self.back.remove(&hwnd) {
            let _ = self.conn.free_pixmap(pixmap);
        }
        if let Some(w) = self.windows.remove(&hwnd) {
            self.by_xwin.remove(&w.window);
            if w.frame != 0 {
                self.by_xwin.remove(&w.frame);
            }
            let _ = self.conn.free_gc(w.gc);
            let _ = self.conn.destroy_window(Self::top_xwin(&w));
            let _ = self.conn.flush();
        }
    }

    fn show_window(&mut self, hwnd: usize, show: bool) {
        if let Some(w) = self.windows.get(&hwnd) {
            let top = Self::top_xwin(w);
            if show {
                let _ = self.conn.map_window(top);
            } else {
                let _ = self.conn.unmap_window(top);
            }
            if show {
                // Une propriétaire affichée après ses boîtes (VLC ouvre
                // « Vie privée / Réseau » avant sa fenêtre principale) passe
                // dessous ses fenêtres possédées, comme sous Windows (#1068).
                self.raise_owned(hwnd);
            }
            let _ = self.conn.flush();
        }
    }

    fn set_title(&mut self, hwnd: usize, title: &str) {
        if let Some(w) = self.windows.get(&hwnd) {
            let top = Self::top_xwin(w);
            let title_bytes = crate::dlls::encode_ansi(title);
            let _ = self.conn.change_property8(
                x11rb::protocol::xproto::PropMode::REPLACE,
                top,
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
                        top,
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
        let (window, frame, bar, child) = match self.windows.get_mut(&hwnd) {
            Some(w) => {
                w.x = x;
                w.y = y;
                w.width = width;
                w.height = height.saturating_sub(w.bar).max(1);
                (w.window, w.frame, w.bar, w.child)
            }
            None => return,
        };
        if !child {
            // Position voulue par le programme (WM_NORMAL_HINTS) : sans
            // elle, openbox place la fenêtre à sa guise au moment de
            // l'afficher (« À propos » de Notepad++ en bas à gauche, #706).
            self.position_hints(if frame != 0 { frame } else { window }, x, y, width, height);
        }
        let aux = ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .width(width as u32)
            .height(height as u32);
        if frame != 0 {
            let _ = self.conn.configure_window(frame, &aux);
            let client = ConfigureWindowAux::new()
                .width(width as u32)
                .height(height.saturating_sub(bar).max(1) as u32);
            let _ = self.conn.configure_window(window, &client);
        } else {
            let _ = self.conn.configure_window(window, &aux);
        }
        let _ = self.conn.flush();
    }

    fn reparent_window(&mut self, hwnd: usize, parent: usize, rect: Rect) {
        let Some(parent_win) = self.windows.get(&parent).map(|w| w.window) else {
            return;
        };
        let x = self
            .scaled(rect.left)
            .clamp(i16::MIN as i32, i16::MAX as i32);
        let y = self
            .scaled(rect.top)
            .clamp(i16::MIN as i32, i16::MAX as i32);
        let Some(w) = self.windows.get_mut(&hwnd) else {
            return;
        };
        if !w.child {
            return; // fenêtre de premier niveau : laissée au gestionnaire
        }
        w.x = x;
        w.y = y;
        let win = w.window;
        let _ = self
            .conn
            .reparent_window(win, parent_win, x as i16, y as i16);
        let _ = self.conn.flush();
    }

    fn set_popup(&mut self, hwnd: usize) {
        if let Some(w) = self.windows.get(&hwnd) {
            let aux = ChangeWindowAttributesAux::new().override_redirect(1);
            let _ = self.conn.change_window_attributes(Self::top_xwin(w), &aux);
            let _ = self.conn.flush();
        }
    }

    fn set_shape(&mut self, hwnd: usize, rects: Option<&[Rect]>) {
        let Some(w) = self.windows.get(&hwnd).copied() else {
            return;
        };
        if w.frame != 0 {
            return; // cadre Winlux (barre de menus) : jamais en couches
        }
        self.apply_shape(w.window, w.width, w.height, rects);
    }

    fn shape_foreign(&mut self, xid: u64, rects: Option<&[Rect]>) {
        let Ok(xid) = u32::try_from(xid) else {
            return;
        };
        let Some(g) = self
            .conn
            .get_geometry(xid)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return;
        };
        self.apply_shape(xid, g.width, g.height, rects);
    }

    fn set_decorations(&mut self, hwnd: usize, level: u8) {
        let Some(w) = self.windows.get(&hwnd) else {
            return;
        };
        let top = Self::top_xwin(w);
        let Ok(Ok(atom)) = self
            .conn
            .intern_atom(false, b"_MOTIF_WM_HINTS")
            .map(|c| c.reply())
        else {
            return;
        };
        // MWM_HINTS_DECORATIONS ; MWM_DECOR_ALL (1) ou MWM_DECOR_BORDER (2).
        let decorations = match level {
            0 => 0,
            1 => 2,
            _ => 1,
        };
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            top,
            atom.atom,
            atom.atom,
            &[2, 0, decorations, 0, 0],
        );
        let _ = self.conn.flush();
    }

    fn set_owner(&mut self, hwnd: usize, owner: usize) {
        let (Some(w), Some(o)) = (self.windows.get(&hwnd), self.windows.get(&owner)) else {
            return;
        };
        let (top, owner_top) = (Self::top_xwin(w), Self::top_xwin(o));
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            top,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[owner_top],
        );
        self.owners.insert(hwnd, owner);
        // Propriétaire donné après l'affichage (Qt 5, GWLP_HWNDPARENT) :
        // la boîte passe tout de suite au-dessus.
        if self.is_viewable(top) {
            self.restack_above(top);
        }
        let _ = self.conn.flush();
    }

    fn set_dialog_type(&mut self, hwnd: usize) {
        let Some(w) = self.windows.get(&hwnd) else {
            return;
        };
        let top = Self::top_xwin(w);
        let atom = |name: &[u8]| {
            self.conn
                .intern_atom(false, name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.atom)
        };
        if let (Some(kind), Some(dialog)) = (
            atom(b"_NET_WM_WINDOW_TYPE"),
            atom(b"_NET_WM_WINDOW_TYPE_DIALOG"),
        ) {
            let _ = self.conn.change_property32(
                x11rb::protocol::xproto::PropMode::REPLACE,
                top,
                kind,
                AtomEnum::ATOM,
                &[dialog],
            );
            let _ = self.conn.flush();
        }
    }

    fn set_no_initial_focus(&mut self, hwnd: usize) {
        let Some(w) = self.windows.get(&hwnd) else {
            return;
        };
        if w.child {
            return;
        }
        let top = Self::top_xwin(w);
        // _NET_WM_USER_TIME = 0 (EWMH) : ne pas donner le focus à
        // l'affichage. La liste d'auto-complétion de Notepad++ prenait sinon
        // toutes les frappes destinées à l'éditeur (#1127).
        let Ok(Ok(atom)) = self
            .conn
            .intern_atom(false, b"_NET_WM_USER_TIME")
            .map(|c| c.reply())
        else {
            return;
        };
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            top,
            atom.atom,
            AtomEnum::CARDINAL,
            &[0],
        );
        let _ = self.conn.flush();
    }

    fn set_window_state(&mut self, hwnd: usize, state: u8) {
        let Some(w) = self.windows.get(&hwnd) else {
            return;
        };
        if w.child {
            return;
        }
        let top = Self::top_xwin(w);
        let root = self.conn.setup().roots[self.screen].root;
        let atom = |c: &RustConnection, name: &[u8]| {
            c.intern_atom(false, name)
                .ok()
                .and_then(|r| r.reply().ok())
                .map(|r| r.atom)
        };
        let mask = x11rb::protocol::xproto::EventMask::SUBSTRUCTURE_REDIRECT
            | x11rb::protocol::xproto::EventMask::SUBSTRUCTURE_NOTIFY;
        let send = |c: &RustConnection, kind: Atom, data: [u32; 5]| {
            let ev = x11rb::protocol::xproto::ClientMessageEvent::new(32, top, kind, data);
            let _ = c.send_event(false, root, mask, ev);
        };
        if state == gui::WINDOW_MINIMIZED {
            // ICCCM : WM_CHANGE_STATE vers IconicState (3).
            if let Some(change) = atom(&self.conn, b"WM_CHANGE_STATE") {
                send(&self.conn, change, [3, 0, 0, 0, 0]);
            }
        } else if let (Some(net_state), Some(vert), Some(horz)) = (
            atom(&self.conn, b"_NET_WM_STATE"),
            atom(&self.conn, b"_NET_WM_STATE_MAXIMIZED_VERT"),
            atom(&self.conn, b"_NET_WM_STATE_MAXIMIZED_HORZ"),
        ) {
            // EWMH : 1 ajoute, 0 retire ; source 1 (application).
            let add = u32::from(state == gui::WINDOW_MAXIMIZED);
            send(&self.conn, net_state, [add, vert, horz, 1, 0]);
            let _ = self.conn.map_window(top);
        }
        let _ = self.conn.flush();
    }

    fn window_state(&mut self, hwnd: usize) -> u8 {
        let Some(w) = self.windows.get(&hwnd) else {
            return gui::WINDOW_NORMAL;
        };
        let top = Self::top_xwin(w);
        let atom = |c: &RustConnection, name: &[u8]| {
            c.intern_atom(false, name)
                .ok()
                .and_then(|r| r.reply().ok())
                .map(|r| r.atom)
        };
        let read = |c: &RustConnection, prop: Atom| -> Vec<u32> {
            c.get_property(false, top, prop, AtomEnum::ANY, 0, 64)
                .ok()
                .and_then(|r| r.reply().ok())
                .and_then(|r| r.value32().map(|v| v.collect()))
                .unwrap_or_default()
        };
        if let Some(wm_state) = atom(&self.conn, b"WM_STATE") {
            if read(&self.conn, wm_state).first() == Some(&3) {
                return gui::WINDOW_MINIMIZED;
            }
        }
        if let (Some(net_state), Some(vert), Some(horz)) = (
            atom(&self.conn, b"_NET_WM_STATE"),
            atom(&self.conn, b"_NET_WM_STATE_MAXIMIZED_VERT"),
            atom(&self.conn, b"_NET_WM_STATE_MAXIMIZED_HORZ"),
        ) {
            let states = read(&self.conn, net_state);
            if states.contains(&vert) && states.contains(&horz) {
                return gui::WINDOW_MAXIMIZED;
            }
        }
        gui::WINDOW_NORMAL
    }

    fn begin_move_resize(&mut self, hwnd: usize, edge: u32) {
        let Some(w) = self.windows.get(&hwnd) else {
            return;
        };
        if w.child {
            return;
        }
        // HT* de Win32 → direction _NET_WM_MOVERESIZE (EWMH).
        let direction = match edge {
            2 => 8,
            10 => 7,
            11 => 3,
            12 => 1,
            13 => 0,
            14 => 2,
            15 => 5,
            16 => 6,
            17 => 4,
            _ => return,
        };
        let top = Self::top_xwin(w);
        let root = self.conn.setup().roots[self.screen].root;
        let Some(pointer) = self
            .conn
            .query_pointer(root)
            .ok()
            .and_then(|r| r.reply().ok())
        else {
            return;
        };
        let Some(kind) = self
            .conn
            .intern_atom(false, b"_NET_WM_MOVERESIZE")
            .ok()
            .and_then(|r| r.reply().ok())
            .map(|r| r.atom)
        else {
            return;
        };
        // Le clic tient une saisie implicite du pointeur : le gestionnaire ne
        // peut prendre la main qu'une fois relâchée.
        let _ = self.conn.ungrab_pointer(x11rb::CURRENT_TIME);
        let data = [
            pointer.root_x as i32 as u32,
            pointer.root_y as i32 as u32,
            direction,
            1,
            1,
        ];
        let ev = x11rb::protocol::xproto::ClientMessageEvent::new(32, top, kind, data);
        let mask = x11rb::protocol::xproto::EventMask::SUBSTRUCTURE_REDIRECT
            | x11rb::protocol::xproto::EventMask::SUBSTRUCTURE_NOTIFY;
        let _ = self.conn.send_event(false, root, mask, ev);
        let _ = self.conn.flush();
    }

    fn grab_hotkey(&mut self, mods: u32, vk: u32, grab: bool) -> bool {
        use x11rb::protocol::xproto::{GrabMode, ModMask};
        let Some(keycode) = self.vk_keycode(vk) else {
            return false;
        };
        let root = self.conn.setup().roots[self.screen].root;
        let base = hotkey_x_mods(mods);
        // Verr. Maj et Verr. Num ne doivent pas empêcher le raccourci.
        let variants = [0, LOCK_MASK, NUM_LOCK_MASK, LOCK_MASK | NUM_LOCK_MASK];
        if !grab {
            for extra in variants {
                let _ = self
                    .conn
                    .ungrab_key(keycode, root, ModMask::from(base | extra));
            }
            self.hotkeys.remove(&(keycode, base));
            let _ = self.conn.flush();
            return true;
        }
        for (done, extra) in variants.iter().enumerate() {
            // BadAccess : un autre client (openbox, un autre programme) tient
            // déjà la combinaison.
            let taken = self
                .conn
                .grab_key(
                    false,
                    root,
                    ModMask::from(base | extra),
                    keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )
                .map_or(true, |cookie| cookie.check().is_err());
            if taken {
                for undo in &variants[..done] {
                    let _ = self
                        .conn
                        .ungrab_key(keycode, root, ModMask::from(base | undo));
                }
                let _ = self.conn.flush();
                return false;
            }
        }
        self.hotkeys.insert((keycode, base), (mods, vk));
        true
    }

    fn grab_pointer(&mut self, hwnd: usize, grab: bool) {
        use x11rb::protocol::xproto::{EventMask, GrabMode};
        if !grab {
            let _ = self.conn.ungrab_pointer(x11rb::CURRENT_TIME);
            let _ = self.conn.flush();
            return;
        }
        if let Some(w) = self.windows.get(&hwnd) {
            // owner_events : les fenêtres du programme reçoivent toujours
            // leurs clics ; le reste de l'écran arrive au menu.
            let _ = self.conn.grab_pointer(
                true,
                w.window,
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                x11rb::NONE,
                x11rb::NONE,
                x11rb::CURRENT_TIME,
            );
            let _ = self.conn.flush();
        }
    }

    fn lower_window(&mut self, hwnd: usize) {
        if let Some(w) = self.windows.get(&hwnd) {
            let aux =
                ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::BELOW);
            let _ = self.conn.configure_window(Self::top_xwin(w), &aux);
            let _ = self.conn.flush();
        }
    }

    fn stack_below(&mut self, hwnd: usize, sibling: usize) {
        if let (Some(w), Some(s)) = (self.windows.get(&hwnd), self.windows.get(&sibling)) {
            let aux = ConfigureWindowAux::new()
                .sibling(Self::top_xwin(s))
                .stack_mode(x11rb::protocol::xproto::StackMode::BELOW);
            let _ = self.conn.configure_window(Self::top_xwin(w), &aux);
            let _ = self.conn.flush();
        }
    }

    fn raise_window(&mut self, hwnd: usize) {
        if let Some(w) = self.windows.get(&hwnd) {
            let aux =
                ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::ABOVE);
            let _ = self.conn.configure_window(Self::top_xwin(w), &aux);
            self.raise_owned(hwnd);
            let _ = self.conn.flush();
        }
    }

    fn set_focus(&mut self, hwnd: usize) {
        let focus = if hwnd == 0 {
            0
        } else if let Some(w) = self.windows.get(&hwnd) {
            w.window
        } else {
            return;
        };
        self.asked_focus = focus;
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
            diag_paint("effacement", hwnd, w.window);
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

/// Nom de la machine pour WM_CLIENT_MACHINE (ICCCM), comme `hostname`.
fn client_machine() -> String {
    ["/proc/sys/kernel/hostname", "/etc/hostname"]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "localhost".to_string())
}
