//! Icônes (HICON) et listes d'images de comctl32 (HIMAGELIST).
//!
//! Les images sont gardées en mémoire sous forme de pixels COLORREF avec
//! l'alpha dans l'octet haut. Elles viennent des ressources du programme
//! (RT_GROUP_ICON / RT_ICON, RT_BITMAP), de fichiers .ico, de bitmaps GDI
//! (CreateIconIndirect, ImageList_Add) ou des icônes du thème Linux pour la
//! liste d'images système.
//!
//! Le dessin passe par les commandes GDI existantes : masque en ET puis
//! couleur en OU, comme le fait Windows pour une icône à masque. Quand la
//! couleur du fond est connue (contrôles de Winlux), les bords
//! semi-transparents sont mélangés à ce fond.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::gui::{self, DrawCmd, Rect};

/// Image en pixels COLORREF, alpha dans l'octet haut, ligne du haut d'abord.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Image {
    pub fn blank(w: usize, h: usize) -> Image {
        Image {
            w,
            h,
            px: vec![0; w * h],
        }
    }

    /// Redimensionnement par moyenne de zone (réduction) ou plus proche
    /// voisin (agrandissement), alpha prémultiplié pour le calcul.
    pub fn scaled(&self, w: usize, h: usize) -> Image {
        if (w, h) == (self.w, self.h) || self.w == 0 || self.h == 0 || w == 0 || h == 0 {
            return self.clone();
        }
        let mut out = Image::blank(w, h);
        for y in 0..h {
            let y0 = y * self.h / h;
            let y1 = ((y + 1) * self.h / h).max(y0 + 1).min(self.h);
            for x in 0..w {
                let x0 = x * self.w / w;
                let x1 = ((x + 1) * self.w / w).max(x0 + 1).min(self.w);
                let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let c = self.px[sy * self.w + sx];
                        let ca = c >> 24;
                        r += (c & 0xFF) * ca;
                        g += ((c >> 8) & 0xFF) * ca;
                        b += ((c >> 16) & 0xFF) * ca;
                        a += ca;
                        n += 1;
                    }
                }
                out.px[y * w + x] = if a == 0 {
                    0
                } else {
                    ((a / n) << 24) | ((b / a) << 16) | ((g / a) << 8) | (r / a)
                };
            }
        }
        out
    }
}

#[derive(Clone)]
struct ImageList {
    cx: usize,
    cy: usize,
    flags: u32,
    images: Vec<Image>,
    bk: u32,
}

const CLR_NONE: u32 = 0xFFFF_FFFF;

fn icons() -> &'static Mutex<HashMap<usize, Image>> {
    static S: OnceLock<Mutex<HashMap<usize, Image>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lists() -> &'static Mutex<HashMap<usize, ImageList>> {
    static S: OnceLock<Mutex<HashMap<usize, ImageList>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Enregistre une image comme HICON.
pub fn new_icon(img: Image) -> usize {
    let h = gui::next_handle(0x0021_0000);
    lock(icons()).insert(h, img);
    h
}

pub fn icon_image(h: usize) -> Option<Image> {
    lock(icons()).get(&h).cloned()
}

// ---------------------------------------------------------------------
// Décodage
// ---------------------------------------------------------------------

fn rd16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn rd32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// DIB brute (BITMAPINFOHEADER et suivants). `icon` : la hauteur est
/// doublée et un masque ET de 1 bit suit l'image.
pub fn decode_dib(d: &[u8], icon: bool) -> Option<Image> {
    let hsize = rd32(d, 0)? as usize;
    if hsize == 12 {
        return None; // BITMAPCOREHEADER : très ancien, non pris en charge
    }
    let w = rd32(d, 4)? as i32;
    let raw_h = rd32(d, 8)? as i32;
    let bpp = rd16(d, 14)? as usize;
    let compression = rd32(d, 16)?;
    let clr_used = rd32(d, 32)? as usize;
    if w <= 0 || w > 4096 || raw_h == 0 {
        return None;
    }
    let top_down = raw_h < 0;
    let total_h = raw_h.unsigned_abs() as usize;
    let h = if icon { total_h / 2 } else { total_h };
    if h == 0 || h > 4096 {
        return None;
    }
    let w = w as usize;
    let mut masks = (0x00FF_0000u32, 0x0000_FF00u32, 0x0000_00FFu32, 0u32);
    let mut off = hsize;
    if compression == 3 {
        if hsize >= 52 {
            masks = (rd32(d, 40)?, rd32(d, 44)?, rd32(d, 48)?, 0);
            if hsize >= 56 {
                masks.3 = rd32(d, 52)?;
            }
        } else {
            masks = (rd32(d, off)?, rd32(d, off + 4)?, rd32(d, off + 8)?, 0);
            off += 12;
        }
    } else if compression != 0 {
        return None; // RLE et JPEG/PNG embarqués : pas ici
    }
    let palette_len = if bpp <= 8 {
        if clr_used == 0 {
            1 << bpp
        } else {
            clr_used.min(256)
        }
    } else {
        0
    };
    let mut palette = Vec::with_capacity(palette_len);
    for i in 0..palette_len {
        let q = rd32(d, off + i * 4)?;
        // RGBQUAD : B, G, R → COLORREF 0x00BBGGRR
        let (b, g, r) = (q & 0xFF, (q >> 8) & 0xFF, (q >> 16) & 0xFF);
        palette.push((b << 16) | (g << 8) | r);
    }
    off += palette_len * 4;
    let stride = (w * bpp).div_ceil(32) * 4;
    let rows = d.get(off..off + stride * h)?;
    let mut img = Image::blank(w, h);
    let channel = |v: u32, m: u32| -> u32 {
        if m == 0 {
            return 0;
        }
        let shift = m.trailing_zeros();
        let max = m >> shift;
        ((v & m) >> shift) * 255 / max.max(1)
    };
    let mut any_alpha = false;
    for y in 0..h {
        let src_row = if top_down { y } else { h - 1 - y };
        let row = &rows[src_row * stride..src_row * stride + stride];
        for x in 0..w {
            let (c, a) = match bpp {
                1 | 4 | 8 => {
                    let bit = x * bpp;
                    let byte = row[bit / 8];
                    let shift = 8 - bpp - (bit % 8);
                    let i = ((byte >> shift) as usize) & ((1 << bpp) - 1);
                    (palette.get(i).copied().unwrap_or(0), 255)
                }
                16 => {
                    let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]) as u32;
                    let m = if compression == 3 {
                        masks
                    } else {
                        (0x7C00, 0x03E0, 0x001F, 0)
                    };
                    let (r, g, b) = (channel(v, m.0), channel(v, m.1), channel(v, m.2));
                    ((b << 16) | (g << 8) | r, 255)
                }
                24 => {
                    let p = &row[x * 3..x * 3 + 3];
                    (
                        ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32,
                        255,
                    )
                }
                32 => {
                    let v = u32::from_le_bytes([
                        row[x * 4],
                        row[x * 4 + 1],
                        row[x * 4 + 2],
                        row[x * 4 + 3],
                    ]);
                    let (r, g, b) = (
                        channel(v, masks.0),
                        channel(v, masks.1),
                        channel(v, masks.2),
                    );
                    let a = if compression == 3 && masks.3 != 0 {
                        channel(v, masks.3)
                    } else {
                        v >> 24
                    };
                    if a != 0 {
                        any_alpha = true;
                    }
                    ((b << 16) | (g << 8) | r, a)
                }
                _ => return None,
            };
            img.px[y * w + x] = c | (a << 24);
        }
    }
    // Sans alpha (ou bitmap ordinaire) : opaque, puis masque ET d'icône.
    if !(bpp == 32 && any_alpha) {
        for p in img.px.iter_mut() {
            *p |= 0xFF00_0000;
        }
    }
    if icon {
        let mstride = w.div_ceil(32) * 4;
        let moff = off + stride * h;
        if let Some(mask) = d.get(moff..moff + mstride * h) {
            if !(bpp == 32 && any_alpha) {
                for y in 0..h {
                    let src_row = if top_down { y } else { h - 1 - y };
                    for x in 0..w {
                        let bit = mask[src_row * mstride + x / 8] >> (7 - x % 8) & 1;
                        if bit == 1 {
                            img.px[y * w + x] = 0;
                        }
                    }
                }
            }
        }
    }
    Some(img)
}

/// PNG en mémoire (icônes 256 px de Vista et suivants).
pub fn decode_png_bytes(d: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(d));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let size = reader.output_buffer_size()?;
    if size > 64 * 1024 * 1024 {
        return None;
    }
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let bytes = &buf[..info.buffer_size()];
    let n = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return None,
    };
    if bytes.len() < w * h * n {
        return None;
    }
    let px = bytes
        .chunks_exact(n)
        .take(w * h)
        .map(|p| {
            let (r, g, b, a) = match n {
                4 => (p[0], p[1], p[2], p[3]),
                3 => (p[0], p[1], p[2], 255),
                2 => (p[0], p[0], p[0], p[1]),
                _ => (p[0], p[0], p[0], 255),
            };
            ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | r as u32
        })
        .collect();
    Some(Image { w, h, px })
}

/// Contenu d'une entrée RT_ICON ou d'une image de fichier .ico.
pub fn decode_icon_bits(d: &[u8]) -> Option<Image> {
    if d.starts_with(b"\x89PNG") {
        decode_png_bytes(d)
    } else {
        decode_dib(d, true)
    }
}

/// Choix de l'entrée la plus proche de la taille voulue (`want` px), en
/// préférant la plus grande profondeur de couleur à taille égale.
fn best_entry(entries: &[(usize, usize, usize)], want: usize) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .min_by_key(|(_, &(size, bpp, _))| {
            // Une image plus grande réduite est plus nette qu'une petite agrandie.
            let dist = if size >= want {
                (size - want) * 2
            } else {
                (want - size) * 5
            };
            (dist, usize::MAX - bpp)
        })
        .map(|(i, _)| i)
}

/// RT_GROUP_ICON → identifiant RT_ICON de la meilleure taille.
pub fn group_pick(group: &[u8], want: usize) -> Option<u16> {
    let count = rd16(group, 4)? as usize;
    let mut entries = Vec::new();
    for i in 0..count {
        let o = 6 + i * 14;
        let w = *group.get(o)? as usize;
        let bpp = rd16(group, o + 6)? as usize;
        let id = rd16(group, o + 12)? as usize;
        entries.push((if w == 0 { 256 } else { w }, bpp, id));
    }
    let i = best_entry(&entries, want)?;
    Some(entries[i].2 as u16)
}

/// Fichier .ico (ou .cur) complet.
pub fn decode_ico_file(d: &[u8], want: usize) -> Option<Image> {
    if rd16(d, 0)? != 0 || !matches!(rd16(d, 2)?, 1 | 2) {
        return None;
    }
    let count = rd16(d, 4)? as usize;
    let mut entries = Vec::new();
    for i in 0..count {
        let o = 6 + i * 16;
        let w = *d.get(o)? as usize;
        let bpp = rd16(d, o + 6)? as usize;
        entries.push((if w == 0 { 256 } else { w }, bpp, o));
    }
    let i = best_entry(&entries, want)?;
    let o = entries[i].2;
    let size = rd32(d, o + 8)? as usize;
    let at = rd32(d, o + 12)? as usize;
    decode_icon_bits(d.get(at..at.checked_add(size)?)?)
}

// ---------------------------------------------------------------------
// Dessin
// ---------------------------------------------------------------------

/// Dessine une image à (x, y) en w × h. `bg` : couleur du fond si elle est
/// connue, pour mélanger les bords semi-transparents.
pub fn draw(hdc: usize, x: i32, y: i32, w: i32, h: i32, img: &Image, bg: Option<u32>) -> bool {
    if w <= 0 || h <= 0 || img.w == 0 || img.h == 0 {
        return false;
    }
    let img = img.scaled(w as usize, h as usize);
    let rect = Rect {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    let opaque = img.px.iter().all(|&p| p >> 24 == 0xFF);
    let color: Vec<u32> = img
        .px
        .iter()
        .map(|&p| {
            let a = p >> 24;
            match (a, bg) {
                (0, _) => 0,
                (255, _) => p & 0x00FF_FFFF,
                (_, Some(bg)) => blend(p, bg, a),
                (a, None) if a >= 128 => p & 0x00FF_FFFF,
                _ => 0,
            }
        })
        .collect();
    if opaque {
        return gui::record(
            hdc,
            DrawCmd::Bitmap {
                rect,
                pixels: color,
                rop: gui::RasterOp::Copy,
            },
        );
    }
    let visible = |p: u32| {
        let a = p >> 24;
        a != 0 && (bg.is_some() || a >= 128)
    };
    let mask: Vec<u32> = img
        .px
        .iter()
        .map(|&p| if visible(p) { 0 } else { 0x00FF_FFFF })
        .collect();
    gui::record(
        hdc,
        DrawCmd::Bitmap {
            rect,
            pixels: mask,
            rop: gui::RasterOp::And,
        },
    );
    gui::record(
        hdc,
        DrawCmd::Bitmap {
            rect,
            pixels: color,
            rop: gui::RasterOp::Or,
        },
    )
}

fn blend(p: u32, bg: u32, a: u32) -> u32 {
    let mix = |s: u32, d: u32| (s * a + d * (255 - a)) / 255;
    let r = mix(p & 0xFF, bg & 0xFF);
    let g = mix((p >> 8) & 0xFF, (bg >> 8) & 0xFF);
    let b = mix((p >> 16) & 0xFF, (bg >> 16) & 0xFF);
    (b << 16) | (g << 8) | r
}

// ---------------------------------------------------------------------
// user32 : icônes
// ---------------------------------------------------------------------

const IMAGE_BITMAP: u32 = 0;
const IMAGE_ICON: u32 = 1;
const IMAGE_CURSOR: u32 = 2;
const LR_LOADFROMFILE: u32 = 0x10;
const LR_DEFAULTSIZE: u32 = 0x40;
const RT_BITMAP: usize = 2;
const RT_ICON: usize = 3;
const RT_GROUP_ICON: usize = 14;

fn system_icon_size(small: bool) -> usize {
    if small {
        16
    } else {
        32
    }
}

/// Icône d'une ressource RT_GROUP_ICON du module.
pub fn load_icon_resource(hmod: usize, name: usize, want: usize) -> Option<Image> {
    let group = super::resource::resource_bytes(hmod, name, RT_GROUP_ICON)?;
    let id = group_pick(group, want)?;
    let bits = super::resource::resource_bytes(hmod, id as usize, RT_ICON)?;
    decode_icon_bits(bits)
}

/// Bitmap RT_BITMAP du module (DIB sans en-tête de fichier).
pub fn load_bitmap_resource(hmod: usize, name: usize) -> Option<Image> {
    decode_dib(
        super::resource::resource_bytes(hmod, name, RT_BITMAP)?,
        false,
    )
}

/// Icône de repli pour les IDI_* système : une fenêtre stylisée.
fn system_icon(want: usize) -> Image {
    let mut img = Image::blank(want, want);
    let s = want as f32 / 32.0;
    for y in 0..want {
        for x in 0..want {
            let (fx, fy) = (x as f32 / s, y as f32 / s);
            let inside = (3.0..29.0).contains(&fx) && (5.0..27.0).contains(&fy);
            if !inside {
                continue;
            }
            let border = fx < 4.0 || fx >= 28.0 || fy < 6.0 || fy >= 26.0;
            img.px[y * want + x] = if border {
                0xFF5A_5A5A
            } else if fy < 10.0 {
                0xFFF0_3D6A // barre de titre violette (BGR)
            } else {
                0xFFFF_FFFF
            };
        }
    }
    img
}

unsafe fn wide_name(name: usize, wide: bool) -> (usize, Option<Vec<u16>>) {
    if name < 0x10000 || wide {
        return (name, None);
    }
    let s: Vec<u16> = super::cstr(name as *const u8)
        .encode_utf16()
        .chain([0])
        .collect();
    (s.as_ptr() as usize, Some(s))
}

unsafe fn load_image(
    inst: usize,
    name: usize,
    ty: u32,
    cx: i32,
    cy: i32,
    flags: u32,
    wide: bool,
) -> usize {
    if ty == IMAGE_CURSOR && flags & LR_LOADFROMFILE == 0 {
        return gui::load_cursor(inst, name);
    }
    let want = |small: bool| -> (usize, usize) {
        let d = system_icon_size(small);
        let w = if cx > 0 { cx as usize } else { d };
        let h = if cy > 0 { cy as usize } else { w };
        let _ = LR_DEFAULTSIZE;
        (w, h)
    };
    if flags & LR_LOADFROMFILE != 0 {
        let path = if wide {
            super::wstr(name as *const u16)
        } else {
            super::cstr(name as *const u8)
        };
        let Ok(bytes) = std::fs::read(crate::paths::to_linux(&path)) else {
            crate::process::set_last_error(2);
            return 0;
        };
        return match ty {
            IMAGE_ICON | IMAGE_CURSOR => {
                let (w, h) = want(false);
                decode_ico_file(&bytes, w).map_or(0, |img| new_icon(fit(img, cx, cy, w, h)))
            }
            IMAGE_BITMAP => bytes
                .get(14..)
                .filter(|_| bytes.starts_with(b"BM"))
                .and_then(|d| decode_dib(d, false))
                .and_then(|img| gui::bitmap_from_pixels(img.w, img.h, &img.px))
                .unwrap_or(0),
            _ => 0,
        };
    }
    let (name, _keep) = wide_name(name, wide);
    match ty {
        IMAGE_ICON => {
            let (w, h) = want(false);
            match load_icon_resource(inst, name, w) {
                Some(img) => new_icon(fit(img, cx, cy, w, h)),
                None if inst == 0 && name < 0x10000 => new_icon(system_icon(w)),
                None => {
                    crate::process::set_last_error(1813);
                    0
                }
            }
        }
        IMAGE_BITMAP => match load_bitmap_resource(inst, name) {
            Some(img) => {
                let img = if cx > 0 && cy > 0 {
                    img.scaled(cx as usize, cy as usize)
                } else {
                    img
                };
                gui::bitmap_from_pixels(img.w, img.h, &img.px).unwrap_or(0)
            }
            None => {
                crate::process::set_last_error(1814);
                0
            }
        },
        _ => 0,
    }
}

fn fit(img: Image, cx: i32, cy: i32, w: usize, h: usize) -> Image {
    if cx > 0 || cy > 0 {
        img.scaled(w, h)
    } else {
        img
    }
}

pub unsafe extern "win64" fn load_image_w(
    inst: usize,
    name: *const u16,
    ty: u32,
    cx: i32,
    cy: i32,
    flags: u32,
) -> usize {
    load_image(inst, name as usize, ty, cx, cy, flags, true)
}

pub unsafe extern "win64" fn load_image_a(
    inst: usize,
    name: *const u8,
    ty: u32,
    cx: i32,
    cy: i32,
    flags: u32,
) -> usize {
    load_image(inst, name as usize, ty, cx, cy, flags, false)
}

pub unsafe extern "win64" fn load_icon_w(inst: usize, name: *const u16) -> usize {
    load_image(inst, name as usize, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE, true)
}

pub unsafe extern "win64" fn load_icon_a(inst: usize, name: *const u8) -> usize {
    load_image(inst, name as usize, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE, false)
}

pub unsafe extern "win64" fn load_bitmap_w(inst: usize, name: *const u16) -> usize {
    load_image(inst, name as usize, IMAGE_BITMAP, 0, 0, 0, true)
}

pub unsafe extern "win64" fn load_bitmap_a(inst: usize, name: *const u8) -> usize {
    load_image(inst, name as usize, IMAGE_BITMAP, 0, 0, 0, false)
}

/// CreateIconFromResourceEx : `bits` est le contenu d'une entrée RT_ICON.
pub unsafe extern "win64" fn create_icon_from_resource_ex(
    bits: *const u8,
    size: u32,
    _icon: i32,
    _ver: u32,
    cx: i32,
    cy: i32,
    _flags: u32,
) -> usize {
    if bits.is_null() || size < 16 {
        return 0;
    }
    let d = std::slice::from_raw_parts(bits, size as usize);
    match decode_icon_bits(d) {
        Some(img) => {
            let img = if cx > 0 && cy > 0 {
                img.scaled(cx as usize, cy as usize)
            } else {
                img
            };
            new_icon(img)
        }
        None => 0,
    }
}

pub unsafe extern "win64" fn create_icon_from_resource(
    bits: *const u8,
    size: u32,
    icon: i32,
    ver: u32,
) -> usize {
    create_icon_from_resource_ex(bits, size, icon, ver, 0, 0, 0)
}

/// LookupIconIdFromDirectoryEx : identifiant RT_ICON le plus proche.
pub unsafe extern "win64" fn lookup_icon_id_from_directory_ex(
    dir: *const u8,
    _icon: i32,
    cx: i32,
    _cy: i32,
    _flags: u32,
) -> i32 {
    if dir.is_null() {
        return 0;
    }
    let count = std::ptr::read_unaligned(dir.add(4) as *const u16) as usize;
    let d = std::slice::from_raw_parts(dir, 6 + count * 14);
    let want = if cx > 0 { cx as usize } else { 32 };
    group_pick(d, want).map_or(0, |id| id as i32)
}

pub unsafe extern "win64" fn lookup_icon_id_from_directory(dir: *const u8, icon: i32) -> i32 {
    lookup_icon_id_from_directory_ex(dir, icon, 0, 0, 0)
}

/// ICONINFO : fIcon, xHotspot, yHotspot, (bourrage), hbmMask, hbmColor.
#[repr(C)]
pub struct IconInfo {
    icon: i32,
    x_hot: u32,
    y_hot: u32,
    mask: usize,
    color: usize,
}

/// Image d'une icône construite à partir de ses deux bitmaps.
pub fn image_from_bitmaps(color: usize, mask: usize) -> Option<Image> {
    let m = if mask != 0 {
        gui::bitmap_pixels(mask)
    } else {
        None
    };
    if color == 0 {
        // Icône monochrome : masque ET en haut, image XOR en bas.
        let (w, h2, px, _) = m?;
        let h = h2 / 2;
        let mut img = Image::blank(w, h);
        for y in 0..h {
            for x in 0..w {
                let and = px[y * w + x] & 0x00FF_FFFF != 0;
                let xor = px[(y + h) * w + x] & 0x00FF_FFFF;
                img.px[y * w + x] = if and { 0 } else { 0xFF00_0000 | xor };
            }
        }
        return Some(img);
    }
    let (w, h, px, has_alpha) = gui::bitmap_pixels(color)?;
    let any_alpha = has_alpha && px.iter().any(|p| p >> 24 != 0);
    let mut img = Image { w, h, px };
    if !any_alpha {
        for (i, p) in img.px.iter_mut().enumerate() {
            let transparent = m.as_ref().is_some_and(|(mw, mh, mpx, _)| {
                let (x, y) = (i % w, i / w);
                x < *mw && y < *mh && mpx[y * mw + x] & 0x00FF_FFFF != 0
            });
            *p = if transparent {
                0
            } else {
                (*p & 0x00FF_FFFF) | 0xFF00_0000
            };
        }
    }
    Some(img)
}

pub unsafe extern "win64" fn create_icon_indirect(info: *const IconInfo) -> usize {
    if info.is_null() {
        return 0;
    }
    let ii = std::ptr::read_unaligned(info);
    match image_from_bitmaps(ii.color, ii.mask) {
        Some(img) => new_icon(img),
        None => {
            crate::process::set_last_error(87);
            0
        }
    }
}

/// CreateIcon : plans de bits AND et XOR fournis directement (1 ou 32 bits).
pub unsafe extern "win64" fn create_icon(
    _inst: usize,
    w: i32,
    h: i32,
    planes: u8,
    bpp: u8,
    and_bits: *const u8,
    xor_bits: *const u8,
) -> usize {
    if w <= 0 || h <= 0 || and_bits.is_null() || xor_bits.is_null() || planes != 1 {
        return 0;
    }
    let (w, h) = (w as usize, h as usize);
    let astride = w.div_ceil(16) * 2;
    let mut img = Image::blank(w, h);
    for y in 0..h {
        for x in 0..w {
            let and = *and_bits.add(y * astride + x / 8) >> (7 - x % 8) & 1 == 1;
            let c = match bpp {
                32 => {
                    let p = xor_bits.add((y * w + x) * 4);
                    let (b, g, r, a) = (*p as u32, *p.add(1) as u32, *p.add(2) as u32, *p.add(3));
                    let a = if a == 0 && !and { 255 } else { a as u32 };
                    (a << 24) | (b << 16) | (g << 8) | r
                }
                1 => {
                    let xstride = w.div_ceil(16) * 2;
                    let bit = *xor_bits.add(y * xstride + x / 8) >> (7 - x % 8) & 1;
                    0xFF00_0000 | if bit == 1 { 0x00FF_FFFF } else { 0 }
                }
                _ => return 0,
            };
            img.px[y * w + x] = if and && bpp != 32 { 0 } else { c };
        }
    }
    new_icon(img)
}

pub unsafe extern "win64" fn get_icon_info(icon: usize, info: *mut IconInfo) -> i32 {
    let Some(img) = icon_image(icon) else {
        crate::process::set_last_error(1402); // ERROR_INVALID_CURSOR_HANDLE
        return 0;
    };
    if info.is_null() {
        return 0;
    }
    let mask: Vec<u32> = img
        .px
        .iter()
        .map(|&p| if p >> 24 == 0 { 0x00FF_FFFF } else { 0 })
        .collect();
    let out = IconInfo {
        icon: 1,
        x_hot: (img.w / 2) as u32,
        y_hot: (img.h / 2) as u32,
        mask: gui::bitmap_from_pixels(img.w, img.h, &mask).unwrap_or(0),
        color: gui::bitmap_from_pixels(img.w, img.h, &img.px).unwrap_or(0),
    };
    std::ptr::write_unaligned(info, out);
    1
}

pub extern "win64" fn destroy_icon(icon: usize) -> i32 {
    lock(icons()).remove(&icon);
    1
}

pub extern "win64" fn copy_icon(icon: usize) -> usize {
    match icon_image(icon) {
        Some(img) => new_icon(img),
        None => icon,
    }
}

/// CopyImage : les icônes sont dupliquées (et redimensionnées), le reste
/// est rendu tel quel.
pub extern "win64" fn copy_image(h: usize, ty: u32, cx: i32, cy: i32, _flags: u32) -> usize {
    if ty == IMAGE_ICON || ty == IMAGE_CURSOR {
        if let Some(img) = icon_image(h) {
            let img = if cx > 0 && cy > 0 {
                img.scaled(cx as usize, cy as usize)
            } else {
                img
            };
            return new_icon(img);
        }
    }
    h
}

pub extern "win64" fn draw_icon_ex(
    hdc: usize,
    x: i32,
    y: i32,
    icon: usize,
    cx: i32,
    cy: i32,
    _step: u32,
    brush: usize,
    _flags: u32,
) -> i32 {
    let Some(img) = icon_image(icon) else {
        return (icon != 0) as i32;
    };
    let w = if cx > 0 { cx } else { img.w as i32 };
    let h = if cy > 0 { cy } else { img.h as i32 };
    let bg = if brush == 0 {
        None
    } else {
        gui::object_color(brush)
    };
    if let Some(color) = bg {
        gui::record(
            hdc,
            DrawCmd::FillRect {
                rect: Rect {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                color,
            },
        );
    }
    draw(hdc, x, y, w, h, &img, bg) as i32
}

pub extern "win64" fn draw_icon(hdc: usize, x: i32, y: i32, icon: usize) -> i32 {
    draw_icon_ex(hdc, x, y, icon, 0, 0, 0, 0, 0)
}

// ---------------------------------------------------------------------
// comctl32 : listes d'images
// ---------------------------------------------------------------------

const ILC_MASK: u32 = 0x1;
const ILD_TRANSPARENT: u32 = 0x1;

fn with_list<R>(himl: usize, f: impl FnOnce(&mut ImageList) -> R) -> Option<R> {
    lock(lists()).get_mut(&himl).map(f)
}

/// Image `i` d'une liste (pour le dessin des contrôles).
pub fn list_image(himl: usize, i: i32) -> Option<Image> {
    if i < 0 {
        return None;
    }
    lock(lists())
        .get(&himl)
        .and_then(|l| l.images.get(i as usize).cloned())
}

/// Taille des images d'une liste.
pub fn list_size(himl: usize) -> Option<(i32, i32)> {
    lock(lists()).get(&himl).map(|l| (l.cx as i32, l.cy as i32))
}

/// Dessine l'image `i` d'une liste sur un fond connu (contrôles de Winlux).
pub fn draw_list_image(himl: usize, i: i32, hdc: usize, x: i32, y: i32, bg: Option<u32>) -> bool {
    let Some(img) = list_image(himl, i) else {
        return false;
    };
    draw(hdc, x, y, img.w as i32, img.h as i32, &img, bg)
}

pub fn create_list(cx: usize, cy: usize, flags: u32) -> usize {
    let h = gui::next_handle(0x0021_0000);
    lock(lists()).insert(
        h,
        ImageList {
            cx: cx.max(1),
            cy: cy.max(1),
            flags,
            images: Vec::new(),
            bk: CLR_NONE,
        },
    );
    h
}

/// Ajoute une image à une liste, en la redimensionnant si besoin.
pub fn list_add(himl: usize, img: Image) -> i32 {
    with_list(himl, |l| {
        let img = img.scaled(l.cx, l.cy);
        l.images.push(img);
        l.images.len() as i32 - 1
    })
    .unwrap_or(-1)
}

/// Découpe une bande d'images (bitmap de largeur n × cx) en images.
fn split_strip(img: &Image, cx: usize, cy: usize) -> Vec<Image> {
    let n = (img.w / cx.max(1)).max(1);
    (0..n)
        .map(|k| {
            let mut out = Image::blank(cx, cy);
            for y in 0..cy.min(img.h) {
                for x in 0..cx {
                    let sx = k * cx + x;
                    if sx < img.w {
                        out.px[y * cx + x] = img.px[y * img.w + sx];
                    }
                }
            }
            out
        })
        .collect()
}

/// Ajoute toutes les images d'une bande ; renvoie l'index de la première.
pub fn list_add_strip(himl: usize, strip: &Image) -> i32 {
    let Some((cx, cy)) = list_size(himl) else {
        return -1;
    };
    let mut first = -1;
    for img in split_strip(strip, cx as usize, cy as usize) {
        let i = list_add(himl, img);
        if first < 0 {
            first = i;
        }
    }
    first
}

pub extern "win64" fn image_list_create(
    cx: i32,
    cy: i32,
    flags: u32,
    _initial: i32,
    _grow: i32,
) -> usize {
    if cx <= 0 || cy <= 0 {
        return 0;
    }
    create_list(cx as usize, cy as usize, flags)
}

pub extern "win64" fn image_list_destroy(himl: usize) -> i32 {
    lock(lists()).remove(&himl).is_some() as i32
}

pub extern "win64" fn image_list_add(himl: usize, bmp: usize, mask: usize) -> i32 {
    let Some(flags) = with_list(himl, |l| l.flags) else {
        return -1;
    };
    let mask = if flags & ILC_MASK != 0 { mask } else { 0 };
    let Some(img) = image_from_bitmaps(bmp, mask) else {
        return -1;
    };
    list_add_strip(himl, &img)
}

/// ImageList_AddMasked : la couleur `key` devient transparente.
pub extern "win64" fn image_list_add_masked(himl: usize, bmp: usize, key: u32) -> i32 {
    let Some((w, h, px, alpha)) = gui::bitmap_pixels(bmp) else {
        return -1;
    };
    // Un canal alpha ne compte que s'il varie : LoadBitmap rend des DIB
    // 32 bits entièrement opaques (7-Zip), dont le fond magenta doit
    // disparaître par la couleur de masque.
    let any_alpha = alpha && px.iter().any(|p| p >> 24 != 0) && px.iter().any(|p| p >> 24 != 0xFF);
    let key = key & 0x00FF_FFFF;
    let px = px
        .into_iter()
        .map(|p| {
            if any_alpha {
                p
            } else if p & 0x00FF_FFFF == key {
                0
            } else {
                p | 0xFF00_0000
            }
        })
        .collect();
    list_add_strip(himl, &Image { w, h, px })
}

pub extern "win64" fn image_list_replace_icon(himl: usize, i: i32, icon: usize) -> i32 {
    let Some(img) = icon_image(icon) else {
        return -1;
    };
    with_list(himl, |l| {
        let img = img.scaled(l.cx, l.cy);
        if i < 0 || i as usize >= l.images.len() {
            l.images.push(img);
            l.images.len() as i32 - 1
        } else {
            l.images[i as usize] = img;
            i
        }
    })
    .unwrap_or(-1)
}

pub extern "win64" fn image_list_add_icon(himl: usize, icon: usize) -> i32 {
    image_list_replace_icon(himl, -1, icon)
}

pub extern "win64" fn image_list_replace(himl: usize, i: i32, bmp: usize, mask: usize) -> i32 {
    let Some(img) = image_from_bitmaps(bmp, mask) else {
        return 0;
    };
    with_list(himl, |l| {
        let img = img.scaled(l.cx, l.cy);
        match l.images.get_mut(i.max(0) as usize) {
            Some(slot) if i >= 0 => {
                *slot = img;
                1
            }
            _ => 0,
        }
    })
    .unwrap_or(0)
}

pub extern "win64" fn image_list_remove(himl: usize, i: i32) -> i32 {
    with_list(himl, |l| {
        if i < 0 {
            l.images.clear();
            1
        } else if (i as usize) < l.images.len() {
            l.images.remove(i as usize);
            1
        } else {
            0
        }
    })
    .unwrap_or(0)
}

pub extern "win64" fn image_list_get_image_count(himl: usize) -> i32 {
    with_list(himl, |l| l.images.len() as i32).unwrap_or(0)
}

pub extern "win64" fn image_list_set_image_count(himl: usize, count: u32) -> i32 {
    with_list(himl, |l| {
        let (cx, cy) = (l.cx, l.cy);
        l.images
            .resize_with(count as usize, || Image::blank(cx, cy));
        1
    })
    .unwrap_or(0)
}

pub unsafe extern "win64" fn image_list_get_icon_size(
    himl: usize,
    cx: *mut i32,
    cy: *mut i32,
) -> i32 {
    let Some((w, h)) = list_size(himl) else {
        return 0;
    };
    if !cx.is_null() {
        *cx = w;
    }
    if !cy.is_null() {
        *cy = h;
    }
    1
}

pub extern "win64" fn image_list_set_icon_size(himl: usize, cx: i32, cy: i32) -> i32 {
    with_list(himl, |l| {
        l.cx = cx.max(1) as usize;
        l.cy = cy.max(1) as usize;
        l.images.clear();
        1
    })
    .unwrap_or(0)
}

pub extern "win64" fn image_list_set_bk_color(himl: usize, color: u32) -> u32 {
    with_list(himl, |l| std::mem::replace(&mut l.bk, color)).unwrap_or(CLR_NONE)
}

pub extern "win64" fn image_list_get_bk_color(himl: usize) -> u32 {
    with_list(himl, |l| l.bk).unwrap_or(CLR_NONE)
}

pub extern "win64" fn image_list_draw_ex(
    himl: usize,
    i: i32,
    hdc: usize,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    bk: u32,
    _fg: u32,
    style: u32,
) -> i32 {
    let Some(img) = list_image(himl, i) else {
        return 0;
    };
    let w = if dx > 0 { dx } else { img.w as i32 };
    let h = if dy > 0 { dy } else { img.h as i32 };
    // CLR_DEFAULT (0xFF000000) : couleur de fond de la liste.
    let bk = if bk == 0xFF00_0000 {
        image_list_get_bk_color(himl)
    } else {
        bk
    };
    let bg = (style & ILD_TRANSPARENT == 0 && bk != CLR_NONE).then_some(bk & 0x00FF_FFFF);
    if let Some(color) = bg {
        gui::record(
            hdc,
            DrawCmd::FillRect {
                rect: Rect {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                color,
            },
        );
    }
    draw(hdc, x, y, w, h, &img, bg) as i32
}

pub extern "win64" fn image_list_draw(
    himl: usize,
    i: i32,
    hdc: usize,
    x: i32,
    y: i32,
    style: u32,
) -> i32 {
    image_list_draw_ex(himl, i, hdc, x, y, 0, 0, 0xFF00_0000, CLR_NONE, style)
}

pub extern "win64" fn image_list_get_icon(himl: usize, i: i32, _flags: u32) -> usize {
    list_image(himl, i).map_or(0, new_icon)
}

pub extern "win64" fn image_list_duplicate(himl: usize) -> usize {
    let Some(copy) = lock(lists()).get(&himl).cloned() else {
        return 0;
    };
    let h = gui::next_handle(0x0021_0000);
    lock(lists()).insert(h, copy);
    h
}

/// IMAGEINFO : hbmImage, hbmMask, Unused1, Unused2, rcImage.
pub unsafe extern "win64" fn image_list_get_image_info(himl: usize, i: i32, info: *mut u8) -> i32 {
    let Some(img) = list_image(himl, i) else {
        return 0;
    };
    if info.is_null() {
        return 0;
    }
    let color = gui::bitmap_from_pixels(img.w, img.h, &img.px).unwrap_or(0);
    std::ptr::write_unaligned(info as *mut usize, color);
    std::ptr::write_unaligned(info.add(8) as *mut usize, 0);
    std::ptr::write_unaligned(info.add(16) as *mut [i32; 4], [0; 4]);
    std::ptr::write_unaligned(
        info.add(32) as *mut [i32; 4],
        [0, 0, img.w as i32, img.h as i32],
    );
    1
}

/// ImageList_LoadImage : bande de bitmaps (ou icône) d'une ressource.
unsafe fn image_list_load(
    inst: usize,
    name: usize,
    cx: i32,
    _grow: i32,
    key: u32,
    ty: u32,
    flags: u32,
    wide: bool,
) -> usize {
    let handle = load_image(inst, name, ty, 0, 0, flags, wide);
    if handle == 0 {
        return 0;
    }
    if ty == IMAGE_ICON {
        let Some(img) = icon_image(handle) else {
            return 0;
        };
        let cx = if cx > 0 { cx as usize } else { img.w };
        let l = create_list(cx, cx, 0x20 | ILC_MASK);
        list_add(l, img);
        return l;
    }
    let Some((w, h, _, _)) = gui::bitmap_pixels(handle) else {
        return 0;
    };
    let cx = if cx > 0 { cx as usize } else { h.max(1) };
    let _ = w;
    let l = create_list(cx, h, 0x20 | ILC_MASK);
    if key == CLR_NONE {
        image_list_add(l, handle, 0);
    } else {
        image_list_add_masked(l, handle, key);
    }
    gui::delete_object(handle);
    l
}

pub unsafe extern "win64" fn image_list_load_image_w(
    inst: usize,
    name: *const u16,
    cx: i32,
    grow: i32,
    key: u32,
    ty: u32,
    flags: u32,
) -> usize {
    image_list_load(inst, name as usize, cx, grow, key, ty, flags, true)
}

pub unsafe extern "win64" fn image_list_load_image_a(
    inst: usize,
    name: *const u8,
    cx: i32,
    grow: i32,
    key: u32,
    ty: u32,
    flags: u32,
) -> usize {
    image_list_load(inst, name as usize, cx, grow, key, ty, flags, false)
}

/// Opérations de glisser des listes d'images : sans image fantôme.
pub extern "win64" fn image_list_ok() -> i32 {
    1
}

// ---------------------------------------------------------------------
// Liste d'images système (SHGetFileInfo)
// ---------------------------------------------------------------------

/// Index des icônes de la liste système.
pub const SYS_FOLDER: i32 = 0;
pub const SYS_FILE: i32 = 1;
pub const SYS_DRIVE: i32 = 2;
pub const SYS_ARCHIVE: i32 = 3;
pub const SYS_PROGRAM: i32 = 4;
pub const SYS_IMAGE: i32 = 5;
pub const SYS_TEXT: i32 = 6;

/// Liste d'images système (une par taille), remplie avec les icônes du
/// thème Linux : dossier, fichier, disque, archive, programme, image, texte.
pub fn system_list(small: bool) -> usize {
    static LISTS: Mutex<[usize; 2]> = Mutex::new([0, 0]);
    let mut l = lock(&LISTS);
    let i = small as usize;
    if l[i] == 0 {
        let px = system_icon_size(small);
        let himl = create_list(px, px, 0x20 | ILC_MASK);
        let names: [&[&str]; 7] = [
            &["folder", "inode-directory"],
            &["text-x-generic", "unknown", "application-x-generic"],
            &["drive-harddisk", "drive-harddisk-system"],
            &[
                "package-x-generic",
                "application-x-archive",
                "application-zip",
            ],
            &[
                "application-x-executable",
                "application-x-ms-dos-executable",
                "system-run",
            ],
            &["image-x-generic"],
            &["text-x-generic"],
        ];
        for candidates in names {
            let img = candidates
                .iter()
                .find_map(|n| super::gdi32::theme_icon(n))
                .unwrap_or_else(|| system_icon(px));
            list_add(himl, img);
        }
        l[i] = himl;
    }
    l[i]
}

/// Icône de la liste système pour un nom de fichier (ou un dossier).
pub fn system_index(name: &str, dir: bool) -> i32 {
    if dir {
        return if name.len() <= 3 && name.ends_with(":\\") {
            SYS_DRIVE
        } else {
            SYS_FOLDER
        };
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "7z" | "zip" | "rar" | "tar" | "gz" | "xz" | "bz2" | "zst" | "cab" | "iso" => SYS_ARCHIVE,
        "exe" | "msi" | "bat" | "cmd" | "com" => SYS_PROGRAM,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "svg" | "tif" | "tiff" => {
            SYS_IMAGE
        }
        "txt" | "md" | "log" | "ini" | "cfg" | "csv" | "json" | "xml" => SYS_TEXT,
        _ => SYS_FILE,
    }
}

// ---------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------

/// Remplacent les anciennes versions vides de user32.
pub fn user32_overrides() -> crate::dlls::Exports {
    crate::exports! {
        "LoadImageW" => fn load_image_w, "LoadImageA" => fn load_image_a,
        "LoadIconW" => fn load_icon_w, "LoadIconA" => fn load_icon_a,
        "LoadBitmapW" => fn load_bitmap_w, "LoadBitmapA" => fn load_bitmap_a,
        "CreateIconFromResource" => fn create_icon_from_resource,
        "CreateIconFromResourceEx" => fn create_icon_from_resource_ex,
        "CreateIconIndirect" => fn create_icon_indirect,
        "CreateIcon" => fn create_icon,
        "LookupIconIdFromDirectory" => fn lookup_icon_id_from_directory,
        "LookupIconIdFromDirectoryEx" => fn lookup_icon_id_from_directory_ex,
        "GetIconInfo" => fn get_icon_info,
        "DestroyIcon" => fn destroy_icon,
        "CopyIcon" => fn copy_icon,
        "CopyImage" => fn copy_image,
        "DrawIconEx" => fn draw_icon_ex,
        "DrawIcon" => fn draw_icon,
    }
}

/// Remplacent les listes d'images factices de comctl32.
pub fn comctl32_overrides() -> crate::dlls::Exports {
    crate::exports! {
        "ImageList_Create" => fn image_list_create,
        "ImageList_Destroy" => fn image_list_destroy,
        "ImageList_Add" => fn image_list_add,
        "ImageList_AddMasked" => fn image_list_add_masked,
        "ImageList_AddIcon" => fn image_list_add_icon,
        "ImageList_ReplaceIcon" => fn image_list_replace_icon,
        "ImageList_Replace" => fn image_list_replace,
        "ImageList_Remove" => fn image_list_remove,
        "ImageList_GetImageCount" => fn image_list_get_image_count,
        "ImageList_SetImageCount" => fn image_list_set_image_count,
        "ImageList_GetIconSize" => fn image_list_get_icon_size,
        "ImageList_SetIconSize" => fn image_list_set_icon_size,
        "ImageList_SetBkColor" => fn image_list_set_bk_color,
        "ImageList_GetBkColor" => fn image_list_get_bk_color,
        "ImageList_Draw" => fn image_list_draw,
        "ImageList_DrawEx" => fn image_list_draw_ex,
        "ImageList_GetIcon" => fn image_list_get_icon,
        "ImageList_Duplicate" => fn image_list_duplicate,
        "ImageList_GetImageInfo" => fn image_list_get_image_info,
        "ImageList_LoadImageW" => fn image_list_load_image_w,
        "ImageList_LoadImageA" => fn image_list_load_image_a,
        "ImageList_LoadImage" => fn image_list_load_image_a,
        "ImageList_SetOverlayImage" => fn image_list_ok,
        "ImageList_BeginDrag" => fn image_list_ok,
        "ImageList_EndDrag" => fn image_list_ok,
        "ImageList_DragEnter" => fn image_list_ok,
        "ImageList_DragLeave" => fn image_list_ok,
        "ImageList_DragMove" => fn image_list_ok,
        "ImageList_DragShowNolock" => fn image_list_ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Icône DIB 2×2, 32 bits sans alpha, avec masque ET.
    fn dib_icon() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(40u32.to_le_bytes());
        d.extend(2i32.to_le_bytes());
        d.extend(4i32.to_le_bytes()); // hauteur doublée
        d.extend(1u16.to_le_bytes());
        d.extend(32u16.to_le_bytes());
        d.extend([0u8; 24]);
        // XOR, de bas en haut : rouge, vert / bleu, blanc
        for bgra in [
            [0, 0, 255, 0],
            [0, 255, 0, 0],
            [255, 0, 0, 0],
            [255, 255, 255, 0],
        ] {
            d.extend(bgra);
        }
        // AND, de bas en haut, lignes de 4 octets : 2e pixel du bas transparent
        d.extend([0b0100_0000, 0, 0, 0]);
        d.extend([0, 0, 0, 0]);
        d
    }

    /// 7-Zip : bitmap 32 bits entièrement opaque à fond magenta ; la couleur
    /// de masque d'ImageList_AddMasked doit quand même s'appliquer (#504).
    #[test]
    fn add_masked_retire_le_magenta_d_une_bitmap_opaque() {
        let magenta = 0x00FF_00FF;
        let px = [0xFF00_0000 | magenta, 0xFF00_00FF];
        let bmp = gui::bitmap_from_pixels(2, 1, &px).unwrap();
        let list = create_list(1, 1, 0x21);
        assert_eq!(image_list_add_masked(list, bmp, magenta), 0);
        assert_eq!(list_image(list, 0).unwrap().px[0] >> 24, 0);
        assert_eq!(list_image(list, 1).unwrap().px[0], 0xFF00_00FF);
    }

    #[test]
    fn icone_dib_avec_masque() {
        let img = decode_icon_bits(&dib_icon()).unwrap();
        assert_eq!((img.w, img.h), (2, 2));
        // Ligne du haut : bleu (COLORREF 0x00FF0000), blanc
        assert_eq!(img.px[0], 0xFFFF_0000);
        assert_eq!(img.px[1], 0xFFFF_FFFF);
        // Ligne du bas : rouge opaque, vert masqué
        assert_eq!(img.px[2], 0xFF00_00FF);
        assert_eq!(img.px[3], 0);
    }

    #[test]
    fn groupe_d_icones_choisit_la_taille_proche() {
        let mut g = vec![0, 0, 1, 0, 3, 0];
        for (w, bpp, id) in [(16u8, 32u16, 1u16), (32, 8, 2), (32, 32, 3), (0, 32, 4)] {
            g.extend([w, w, 0, 0]);
            g.extend(1u16.to_le_bytes());
            g.extend(bpp.to_le_bytes());
            g.extend(0u32.to_le_bytes());
            g.extend(id.to_le_bytes());
        }
        g[4] = 4;
        assert_eq!(group_pick(&g, 32), Some(3));
        assert_eq!(group_pick(&g, 16), Some(1));
        assert_eq!(group_pick(&g, 200), Some(4));
    }

    #[test]
    fn liste_d_images_bande_et_reduction() {
        let himl = image_list_create(2, 2, 0x21, 0, 4);
        let strip = Image {
            w: 4,
            h: 2,
            px: vec![0xFF00_00FF; 8],
        };
        assert_eq!(list_add_strip(himl, &strip), 0);
        assert_eq!(image_list_get_image_count(himl), 2);
        let big = new_icon(Image {
            w: 4,
            h: 4,
            px: vec![0xFF00_FF00; 16],
        });
        assert_eq!(image_list_add_icon(himl, big), 2);
        assert_eq!(list_image(himl, 2).unwrap().w, 2);
        assert_eq!(image_list_remove(himl, 0), 1);
        assert_eq!(image_list_get_image_count(himl), 2);
        assert_eq!(image_list_destroy(himl), 1);
    }

    #[test]
    fn bitmap_et_icone_aller_retour() {
        let img = Image {
            w: 2,
            h: 1,
            px: vec![0xFF12_3456, 0],
        };
        let icon = new_icon(img.clone());
        let mut info = IconInfo {
            icon: 0,
            x_hot: 0,
            y_hot: 0,
            mask: 0,
            color: 0,
        };
        assert_eq!(unsafe { get_icon_info(icon, &mut info) }, 1);
        let back = image_from_bitmaps(info.color, info.mask).unwrap();
        assert_eq!(back, img);
    }

    #[test]
    fn index_de_la_liste_systeme() {
        assert_eq!(system_index("Documents", true), SYS_FOLDER);
        assert_eq!(system_index("C:\\", true), SYS_DRIVE);
        assert_eq!(system_index("a.7z", false), SYS_ARCHIVE);
        assert_eq!(system_index("setup.EXE", false), SYS_PROGRAM);
        assert_eq!(system_index("notes", false), SYS_FILE);
    }
}
