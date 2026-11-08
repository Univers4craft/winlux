//! ddraw.dll : DirectDraw 1 à 7 (#237), pour les jeux 2D des années
//! 1995-2005.
//!
//! Les surfaces sont de la mémoire ordinaire (8 bits à palette, 16 bits
//! 565/555, 24 et 32 bits) ; Blt et BltFast (étirement, clés de couleur,
//! remplissage, miroir, conversion de format), Lock, GetDC, Flip, palettes
//! et découpeurs sont faits par le runtime. L'image de la surface primaire
//! est envoyée à la fenêtre du jeu par StretchDIBits à chaque modification
//! (Unlock, Blt, Flip, ReleaseDC, nouvelle palette) ; Flip et
//! WaitForVerticalBlank attendent la prochaine image à 60 Hz.
//!
//! Plein écran exclusif : SetDisplayMode change vraiment la définition par
//! ChangeDisplaySettings (RandR, défait à la sortie même en cas de
//! plantage), puis la fenêtre sans bordure couvre l'écran ;
//! `WINLUX_DDRAW_MODESWITCH=0` garde la définition du bureau, la fenêtre
//! prend alors la taille du mode au centre de l'écran.
//!
//! Chaque objet a une « face » par version d'interface (IDirectDraw,
//! IDirectDraw2/4/7, IDirectDrawSurface 1/2/3/4/7) ; voir `legacy_com`
//! pour les programmes 32 bits. Direct3D « immédiat » (IDirect3D7…) n'est
//! pas fourni : QueryInterface le refuse.

#![allow(clippy::too_many_arguments)]

use super::comobj::{guid, Guid};
use super::legacy_com::{call_back, put_ptr, put_str, rd32, vtable, wr32, x86};
use crate::dlls::lock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match crate::dlls::lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

// ---------------------------------------------------------------------
// Constantes publiques de ddraw.h
// ---------------------------------------------------------------------

const DD_OK: i32 = 0;
const fn dderr(code: u32) -> i32 {
    (0x8876_0000u32 + code) as i32
}
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const DDERR_INVALIDPARAMS: i32 = 0x8007_0057u32 as i32;
const DDERR_UNSUPPORTED: i32 = 0x8000_4001u32 as i32;
const DDERR_GENERIC: i32 = 0x8000_4005u32 as i32;
const DDERR_INVALIDOBJECT: i32 = dderr(130);
const DDERR_INVALIDPIXELFORMAT: i32 = dderr(145);
const DDERR_NOCOLORKEY: i32 = dderr(215);
const DDERR_NOCLIPPERATTACHED: i32 = dderr(205);
const DDERR_NOPALETTEATTACHED: i32 = dderr(572);
const DDERR_NOTFOUND: i32 = dderr(255);
const DDERR_NOTFLIPPABLE: i32 = dderr(582);
const DDERR_NOTLOCKED: i32 = dderr(584);
const DDERR_NOTPALETTIZED: i32 = dderr(589);
const DDERR_CANTCREATEDC: i32 = dderr(581);
const DDERR_NODC: i32 = dderr(586);
const DDERR_ALREADYINITIALIZED: i32 = dderr(5);

const DDSD_CAPS: u32 = 0x1;
const DDSD_HEIGHT: u32 = 0x2;
const DDSD_WIDTH: u32 = 0x4;
const DDSD_PITCH: u32 = 0x8;
const DDSD_BACKBUFFERCOUNT: u32 = 0x20;
const DDSD_LPSURFACE: u32 = 0x800;
const DDSD_PIXELFORMAT: u32 = 0x1000;
const DDSD_CKSRCBLT: u32 = 0x10000;
const DDSD_CKDESTBLT: u32 = 0x4000;
const DDSD_REFRESHRATE: u32 = 0x40000;

const DDSCAPS_BACKBUFFER: u32 = 0x4;
const DDSCAPS_COMPLEX: u32 = 0x8;
const DDSCAPS_FLIP: u32 = 0x10;
const DDSCAPS_FRONTBUFFER: u32 = 0x20;
const DDSCAPS_OFFSCREENPLAIN: u32 = 0x40;
const DDSCAPS_PALETTE: u32 = 0x100;
const DDSCAPS_PRIMARYSURFACE: u32 = 0x200;
const DDSCAPS_SYSTEMMEMORY: u32 = 0x800;
const DDSCAPS_VIDEOMEMORY: u32 = 0x4000;
const DDSCAPS_VISIBLE: u32 = 0x8000;
const DDSCAPS_ZBUFFER: u32 = 0x20000;
const DDSCAPS_LOCALVIDMEM: u32 = 0x1000_0000;

const DDPF_ALPHAPIXELS: u32 = 0x1;
const DDPF_PALETTEINDEXED8: u32 = 0x20;
const DDPF_RGB: u32 = 0x40;

const DDSCL_FULLSCREEN: u32 = 0x1;
const DDSCL_EXCLUSIVE: u32 = 0x10;

const DDBLT_COLORFILL: u32 = 0x400;
const DDBLT_DDFX: u32 = 0x800;
const DDBLT_KEYDEST: u32 = 0x2000;
const DDBLT_KEYDESTOVERRIDE: u32 = 0x4000;
const DDBLT_KEYSRC: u32 = 0x8000;
const DDBLT_KEYSRCOVERRIDE: u32 = 0x10000;
const DDBLT_ROP: u32 = 0x20000;
const DDBLT_DEPTHFILL: u32 = 0x200_0000;
const DDBLTFX_MIRRORLEFTRIGHT: u32 = 0x2;
const DDBLTFX_MIRRORUPDOWN: u32 = 0x4;
const DDBLTFAST_SRCCOLORKEY: u32 = 0x1;
const DDBLTFAST_DESTCOLORKEY: u32 = 0x2;

const DDCKEY_DESTBLT: u32 = 0x2;
const DDCKEY_SRCBLT: u32 = 0x8;
const DDCKEY_COLORSPACE: u32 = 0x1;

const DDFLIP_NOVSYNC: u32 = 0x8;
const DDENUMRET_CANCEL: usize = 0;
const DDEDM_REFRESHRATES: u32 = 0x1;

const SRCCOPY: u32 = 0x00CC_0020;
const BLACKNESS: u32 = 0x0000_0042;
const WHITENESS: u32 = 0x00FF_0062;

// GUID publics (ddraw.h).
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_DD: [Guid; 4] = [
    guid(
        0x6C14DB80,
        0xA733,
        0x11CE,
        [0xA5, 0x21, 0x00, 0x20, 0xAF, 0x0B, 0xE5, 0x60],
    ),
    guid(
        0xB3A6F3E0,
        0x2B43,
        0x11CF,
        [0xA2, 0xDE, 0x00, 0xAA, 0x00, 0xB9, 0x33, 0x56],
    ),
    guid(
        0x9C59509A,
        0x39BD,
        0x11D1,
        [0x8C, 0x4A, 0x00, 0xC0, 0x4F, 0xD9, 0x30, 0xC5],
    ),
    guid(
        0x15E65EC0,
        0x3B9C,
        0x11D2,
        [0xB9, 0x2F, 0x00, 0x60, 0x97, 0x97, 0xEA, 0x5B],
    ),
];
const IID_SURF: [Guid; 5] = [
    guid(
        0x6C14DB81,
        0xA733,
        0x11CE,
        [0xA5, 0x21, 0x00, 0x20, 0xAF, 0x0B, 0xE5, 0x60],
    ),
    guid(
        0x57805885,
        0x6EEC,
        0x11CF,
        [0x94, 0x41, 0xA8, 0x23, 0x03, 0xC1, 0x0E, 0x27],
    ),
    guid(
        0xDA044E00,
        0x69B2,
        0x11D0,
        [0xA1, 0xD5, 0x00, 0xAA, 0x00, 0xB8, 0xDF, 0xBB],
    ),
    guid(
        0x0B2B8630,
        0xAD35,
        0x11D0,
        [0x8E, 0xA6, 0x00, 0x60, 0x97, 0x97, 0xEA, 0x5B],
    ),
    guid(
        0x06675A80,
        0x3B9B,
        0x11D2,
        [0xB9, 0x2F, 0x00, 0x60, 0x97, 0x97, 0xEA, 0x5B],
    ),
];
const IID_PALETTE: Guid = guid(
    0x6C14DB84,
    0xA733,
    0x11CE,
    [0xA5, 0x21, 0x00, 0x20, 0xAF, 0x0B, 0xE5, 0x60],
);
const IID_CLIPPER: Guid = guid(
    0x6C14DB85,
    0xA733,
    0x11CE,
    [0xA5, 0x21, 0x00, 0x20, 0xAF, 0x0B, 0xE5, 0x60],
);

/// Versions des interfaces, dans l'ordre des faces.
const DD_VERSIONS: [u8; 4] = [1, 2, 4, 7];
const SURF_VERSIONS: [u8; 5] = [1, 2, 3, 4, 7];

fn dd_index(ver: u8) -> usize {
    DD_VERSIONS.iter().position(|&v| v == ver).unwrap_or(0)
}
fn surf_index(ver: u8) -> usize {
    SURF_VERSIONS.iter().position(|&v| v == ver).unwrap_or(0)
}

// ---------------------------------------------------------------------
// Formats de pixels
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fmt {
    P8,
    Rgb555,
    Rgb565,
    Rgb888,
    Xrgb8888,
    Argb8888,
}

impl Fmt {
    pub fn bytes(self) -> usize {
        match self {
            Fmt::P8 => 1,
            Fmt::Rgb555 | Fmt::Rgb565 => 2,
            Fmt::Rgb888 => 3,
            Fmt::Xrgb8888 | Fmt::Argb8888 => 4,
        }
    }

    fn from_bpp(bpp: u32) -> Option<Fmt> {
        Some(match bpp {
            8 => Fmt::P8,
            16 => Fmt::Rgb565,
            24 => Fmt::Rgb888,
            32 => Fmt::Xrgb8888,
            _ => return None,
        })
    }

    /// (drapeaux, bits, masques R, G, B, A) de DDPIXELFORMAT.
    fn ddpf(self) -> (u32, u32, u32, u32, u32, u32) {
        match self {
            Fmt::P8 => (DDPF_RGB | DDPF_PALETTEINDEXED8, 8, 0, 0, 0, 0),
            Fmt::Rgb555 => (DDPF_RGB, 16, 0x7C00, 0x03E0, 0x001F, 0),
            Fmt::Rgb565 => (DDPF_RGB, 16, 0xF800, 0x07E0, 0x001F, 0),
            Fmt::Rgb888 => (DDPF_RGB, 24, 0xFF0000, 0xFF00, 0xFF, 0),
            Fmt::Xrgb8888 => (DDPF_RGB, 32, 0xFF0000, 0xFF00, 0xFF, 0),
            Fmt::Argb8888 => (
                DDPF_RGB | DDPF_ALPHAPIXELS,
                32,
                0xFF0000,
                0xFF00,
                0xFF,
                0xFF00_0000,
            ),
        }
    }

    /// Format décrit par un DDPIXELFORMAT (32 octets, même disposition
    /// en x86 et en x64).
    unsafe fn from_ddpf(pf: usize) -> Option<Fmt> {
        let flags = rd32(pf + 4);
        let bits = rd32(pf + 12);
        if flags & DDPF_PALETTEINDEXED8 != 0 {
            return Some(Fmt::P8);
        }
        if flags & DDPF_RGB == 0 {
            // Tampon de profondeur et formats FourCC : gardés comme mémoire
            // brute de la bonne largeur.
            return Fmt::from_bpp(bits);
        }
        let r = rd32(pf + 16);
        Some(match bits {
            8 => Fmt::P8,
            16 if r == 0x7C00 => Fmt::Rgb555,
            16 => Fmt::Rgb565,
            24 => Fmt::Rgb888,
            32 if flags & DDPF_ALPHAPIXELS != 0 && rd32(pf + 28) != 0 => Fmt::Argb8888,
            32 => Fmt::Xrgb8888,
            _ => return None,
        })
    }

    unsafe fn write_ddpf(self, pf: usize) {
        let (flags, bits, r, g, b, a) = self.ddpf();
        std::ptr::write_bytes(pf as *mut u8, 0, 32);
        wr32(pf, 32);
        wr32(pf + 4, flags);
        wr32(pf + 12, bits);
        wr32(pf + 16, r);
        wr32(pf + 20, g);
        wr32(pf + 24, b);
        wr32(pf + 28, a);
    }

    /// Valeur brute du pixel à `p`.
    #[inline]
    fn read(self, p: &[u8]) -> u32 {
        match self.bytes() {
            1 => p[0] as u32,
            2 => u16::from_le_bytes([p[0], p[1]]) as u32,
            3 => p[0] as u32 | (p[1] as u32) << 8 | (p[2] as u32) << 16,
            _ => u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
        }
    }

    #[inline]
    fn write(self, p: &mut [u8], v: u32) {
        match self.bytes() {
            1 => p[0] = v as u8,
            2 => p[..2].copy_from_slice(&(v as u16).to_le_bytes()),
            3 => p[..3].copy_from_slice(&v.to_le_bytes()[..3]),
            _ => p[..4].copy_from_slice(&v.to_le_bytes()),
        }
    }

    /// Valeur brute → 0xAARRGGBB.
    #[inline]
    pub fn to_argb(self, v: u32, pal: &[u32; 256]) -> u32 {
        let x5 = |c: u32| (c << 3) | (c >> 2);
        match self {
            Fmt::P8 => pal[(v & 0xFF) as usize] | 0xFF00_0000,
            Fmt::Rgb555 => {
                0xFF00_0000 | x5((v >> 10) & 31) << 16 | x5((v >> 5) & 31) << 8 | x5(v & 31)
            }
            Fmt::Rgb565 => {
                let g = (v >> 5) & 63;
                0xFF00_0000 | x5((v >> 11) & 31) << 16 | ((g << 2) | (g >> 4)) << 8 | x5(v & 31)
            }
            Fmt::Rgb888 | Fmt::Xrgb8888 => 0xFF00_0000 | (v & 0xFF_FFFF),
            Fmt::Argb8888 => v,
        }
    }

    /// 0xAARRGGBB → valeur brute (`inv` : couleur → index pour P8).
    #[inline]
    fn encode_argb(self, c: u32, inv: &mut Inverse) -> u32 {
        let (r, g, b) = ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF);
        match self {
            Fmt::P8 => inv.index(c & 0xFF_FFFF) as u32,
            Fmt::Rgb555 => (r >> 3) << 10 | (g >> 3) << 5 | b >> 3,
            Fmt::Rgb565 => (r >> 3) << 11 | (g >> 2) << 5 | b >> 3,
            Fmt::Rgb888 | Fmt::Xrgb8888 => c & 0xFF_FFFF,
            Fmt::Argb8888 => c,
        }
    }

    fn mask(self) -> u32 {
        match self.bytes() {
            1 => 0xFF,
            2 => 0xFFFF,
            3 => 0xFF_FFFF,
            _ => 0xFFFF_FFFF,
        }
    }
}

/// Couleur → index de palette le plus proche, avec cache.
pub struct Inverse<'a> {
    pal: &'a [u32; 256],
    cache: HashMap<u32, u8>,
}

impl<'a> Inverse<'a> {
    pub fn new(pal: &'a [u32; 256]) -> Self {
        Inverse {
            pal,
            cache: HashMap::new(),
        }
    }

    fn index(&mut self, rgb: u32) -> u8 {
        if let Some(&i) = self.cache.get(&rgb) {
            return i;
        }
        let d = |a: u32, b: u32| {
            let dr = ((a >> 16) & 0xFF) as i32 - ((b >> 16) & 0xFF) as i32;
            let dg = ((a >> 8) & 0xFF) as i32 - ((b >> 8) & 0xFF) as i32;
            let db = (a & 0xFF) as i32 - (b & 0xFF) as i32;
            dr * dr + dg * dg + db * db
        };
        let i = (0..256)
            .min_by_key(|&i| d(self.pal[i] & 0xFF_FFFF, rgb))
            .unwrap_or(0) as u8;
        self.cache.insert(rgb, i);
        i
    }
}

/// Palette système par défaut : niveaux de gris (la plupart des jeux posent
/// la leur aussitôt).
fn default_palette() -> [u32; 256] {
    let mut p = [0u32; 256];
    for (i, e) in p.iter_mut().enumerate() {
        let v = i as u32;
        *e = v << 16 | v << 8 | v;
    }
    p
}

// ---------------------------------------------------------------------
// Objets
// ---------------------------------------------------------------------

/// Une face : ce que voit le programme (table de méthodes en tête).
#[repr(C)]
struct Face {
    vtbl: usize,
    owner: usize,
    ver: u8,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl Rect {
    unsafe fn read(p: usize) -> Option<Rect> {
        if p == 0 {
            return None;
        }
        let r = |o: usize| (p as *const i32).add(o).read_unaligned();
        Some(Rect {
            l: r(0),
            t: r(1),
            r: r(2),
            b: r(3),
        })
    }
    fn w(&self) -> i32 {
        self.r - self.l
    }
    fn h(&self) -> i32 {
        self.b - self.t
    }
    fn intersect(&self, o: &Rect) -> Rect {
        Rect {
            l: self.l.max(o.l),
            t: self.t.max(o.t),
            r: self.r.min(o.r),
            b: self.b.min(o.b),
        }
    }
    fn empty(&self) -> bool {
        self.w() <= 0 || self.h() <= 0
    }
}

struct SurfData {
    w: u32,
    h: u32,
    fmt: Fmt,
    pitch: usize,
    caps: u32,
    mem: Vec<u8>,
    /// Surfaces suivantes de la chaîne d'échange (tampons arrière), tenues
    /// par la surface primaire.
    chain: Vec<usize>,
    /// Tampon arrière rendu par GetAttachedSurface(DDSCAPS_BACKBUFFER).
    back: usize,
    /// Surfaces attachées par AddAttachedSurface (tampon de profondeur…).
    attached: Vec<usize>,
    palette: usize,
    clipper: usize,
    ck_src: Option<(u32, u32)>,
    ck_dest: Option<(u32, u32)>,
    locks: u32,
    /// (HDC, HBITMAP, ancien objet, pixels de la section DIB) de GetDC.
    dc: Option<(usize, usize, usize, usize)>,
    private: HashMap<Guid, Vec<u8>>,
    unique: u32,
    priority: u32,
    lod: u32,
}

pub struct Surface {
    faces: [Face; 5],
    refs: AtomicU32,
    dd: usize,
    /// Surface primaire (front) dont celle-ci fait partie, ou 0.
    root: AtomicUsize,
    data: Mutex<SurfData>,
}

struct Palette {
    face: Face,
    refs: AtomicU32,
    /// 0x00RRGGBB par entrée.
    entries: Mutex<[u32; 256]>,
    caps: u32,
    dd: usize,
}

struct Clipper {
    face: Face,
    refs: AtomicU32,
    hwnd: AtomicUsize,
    list: Mutex<Option<Vec<Rect>>>,
}

struct DdState {
    hwnd: usize,
    coop: u32,
    mode: Option<(u32, u32, u32)>,
    switched: bool,
    primary: usize,
    surfaces: Vec<usize>,
}

pub struct DirectDraw {
    faces: [Face; 4],
    refs: AtomicU32,
    st: Mutex<DdState>,
}

unsafe fn face<'a>(this: usize) -> &'a Face {
    &*(this as *const Face)
}
unsafe fn dd_of<'a>(this: usize) -> &'a DirectDraw {
    &*(face(this).owner as *const DirectDraw)
}
unsafe fn surf_of<'a>(this: usize) -> &'a Surface {
    &*(face(this).owner as *const Surface)
}
/// Surface d'un pointeur d'interface passé en argument (0 → None).
unsafe fn surf_arg<'a>(p: usize) -> Option<&'a Surface> {
    if p == 0 {
        None
    } else {
        Some(surf_of(p))
    }
}
unsafe fn pal_of<'a>(this: usize) -> &'a Palette {
    &*(face(this).owner as *const Palette)
}
unsafe fn clip_of<'a>(this: usize) -> &'a Clipper {
    &*(face(this).owner as *const Clipper)
}

fn face_addr(f: &Face) -> usize {
    f as *const Face as usize
}

// ---------------------------------------------------------------------
// Bureau, temps et présentation
// ---------------------------------------------------------------------

fn desktop() -> (u32, u32) {
    unsafe {
        let w = api!("user32", "GetSystemMetrics", fn(i32) -> i32, 0).unwrap_or(0);
        let h = api!("user32", "GetSystemMetrics", fn(i32) -> i32, 1).unwrap_or(0);
        if w > 0 && h > 0 {
            (w as u32, h as u32)
        } else {
            (1024, 768)
        }
    }
}

fn epoch() -> std::time::Instant {
    static E: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *E.get_or_init(std::time::Instant::now)
}

const FRAME_NS: u128 = 16_666_667;

/// Attend le début de la prochaine image (60 Hz).
fn wait_vblank() {
    let now = epoch().elapsed().as_nanos();
    let next = (now / FRAME_NS + 1) * FRAME_NS;
    std::thread::sleep(std::time::Duration::from_nanos((next - now) as u64));
}

/// Position dans l'image en cours, de 0 à 1.
fn frame_phase() -> f64 {
    let now = epoch().elapsed().as_nanos();
    (now % FRAME_NS) as f64 / FRAME_NS as f64
}

unsafe fn client_rect(hwnd: usize) -> Option<(Rect, i32, i32)> {
    let mut rc = [0i32; 4];
    api!(
        "user32",
        "GetClientRect",
        fn(usize, *mut i32) -> i32,
        hwnd,
        rc.as_mut_ptr()
    )?;
    let mut pt = [0i32; 2];
    api!(
        "user32",
        "ClientToScreen",
        fn(usize, *mut i32) -> i32,
        hwnd,
        pt.as_mut_ptr()
    );
    Some((
        Rect {
            l: rc[0],
            t: rc[1],
            r: rc[2],
            b: rc[3],
        },
        pt[0],
        pt[1],
    ))
}

/// Envoie l'image de la surface primaire `front` à la fenêtre du jeu.
unsafe fn present(s: &Surface) {
    let dd = &*(s.dd as *const DirectDraw);
    let (hwnd, exclusive) = {
        let st = lock(&dd.st);
        (st.hwnd, st.coop & DDSCL_EXCLUSIVE != 0)
    };
    let (img, sw, sh, clip_hwnd) = {
        let d = lock(&s.data);
        let clip_hwnd = if d.clipper != 0 {
            (*(d.clipper as *const Clipper))
                .hwnd
                .load(Ordering::Relaxed)
        } else {
            0
        };
        let target = if clip_hwnd != 0 { clip_hwnd } else { hwnd };
        if target == 0 {
            return;
        }
        let Some((client, sx, sy)) = client_rect(target) else {
            return;
        };
        // Fenêtré : la primaire est le bureau, on prend la zone sous la
        // fenêtre ; plein écran : toute l'image.
        let src = if exclusive {
            Rect {
                l: 0,
                t: 0,
                r: d.w as i32,
                b: d.h as i32,
            }
        } else {
            Rect {
                l: sx,
                t: sy,
                r: sx + client.w(),
                b: sy + client.h(),
            }
        }
        .intersect(&Rect {
            l: 0,
            t: 0,
            r: d.w as i32,
            b: d.h as i32,
        });
        if src.empty() {
            return;
        }
        let pal = palette_of(&d);
        let (w, h) = (src.w() as usize, src.h() as usize);
        let mut img = vec![0u32; w * h];
        let bpp = d.fmt.bytes();
        for y in 0..h {
            let row = (src.t as usize + y) * d.pitch + src.l as usize * bpp;
            for x in 0..w {
                let v = d.fmt.read(&d.mem[row + x * bpp..]);
                img[y * w + x] = d.fmt.to_argb(v, &pal) & 0xFF_FFFF;
            }
        }
        (img, w, h, target)
    };
    let Some((client, _, _)) = client_rect(clip_hwnd) else {
        return;
    };
    let mut bmi = [0u8; 44];
    bmi[0..4].copy_from_slice(&40u32.to_le_bytes());
    bmi[4..8].copy_from_slice(&(sw as i32).to_le_bytes());
    bmi[8..12].copy_from_slice(&(-(sh as i32)).to_le_bytes());
    bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
    bmi[14..16].copy_from_slice(&32u16.to_le_bytes());
    let hdc = api!("user32", "GetDC", fn(usize) -> usize, clip_hwnd).unwrap_or(0);
    if hdc == 0 {
        return;
    }
    let (dw, dh) = if exclusive {
        (client.w(), client.h())
    } else {
        (sw as i32, sh as i32)
    };
    api!(
        "gdi32",
        "StretchDIBits",
        fn(usize, i32, i32, i32, i32, i32, i32, i32, i32, *const u32, *const u8, u32, u32) -> i32,
        hdc,
        0,
        0,
        dw,
        dh,
        0,
        0,
        sw as i32,
        sh as i32,
        img.as_ptr(),
        bmi.as_ptr(),
        0,
        SRCCOPY
    );
    api!(
        "user32",
        "ReleaseDC",
        fn(usize, usize) -> i32,
        clip_hwnd,
        hdc
    );
}

/// La surface est-elle l'image affichée ?
fn is_front(d: &SurfData) -> bool {
    d.caps & DDSCAPS_PRIMARYSURFACE != 0
}

fn palette_of(d: &SurfData) -> [u32; 256] {
    if d.palette != 0 {
        unsafe { *lock(&(*(d.palette as *const Palette)).entries) }
    } else {
        default_palette()
    }
}

/// Palette d'une surface 8 bits : la sienne, sinon celle de la primaire.
unsafe fn effective_palette(s: &Surface, d: &SurfData) -> [u32; 256] {
    if d.palette != 0 || d.fmt != Fmt::P8 {
        return palette_of(d);
    }
    let dd = &*(s.dd as *const DirectDraw);
    let prim = lock(&dd.st).primary;
    if prim != 0 && prim != s as *const Surface as usize {
        let p = &*(prim as *const Surface);
        if let Ok(pd) = p.data.try_lock() {
            return palette_of(&pd);
        }
    }
    default_palette()
}

// ---------------------------------------------------------------------
// DDSURFACEDESC(2)
// ---------------------------------------------------------------------

/// Décalage d'un champ de DDSURFACEDESC donné à sa place x86 : lpSurface
/// (36) passe à 40 en x64 et les champs suivants sont décalés de 8.
fn doff(o: usize) -> usize {
    if x86() || o < 36 {
        o
    } else if o == 36 {
        40
    } else {
        o + 8
    }
}

fn desc_size(v2: bool) -> u32 {
    match (x86(), v2) {
        (true, false) => 108,
        (true, true) => 124,
        (false, false) => 120,
        (false, true) => 136,
    }
}

/// La structure passée est-elle une DDSURFACEDESC2 (d'après dwSize) ?
unsafe fn is_desc2(p: usize, default: bool) -> bool {
    match rd32(p) {
        s if s == desc_size(true) => true,
        s if s == desc_size(false) => false,
        _ => default,
    }
}

unsafe fn write_desc(p: usize, d: &SurfData, v2: bool, surface_ptr: usize) {
    let size = desc_size(v2) as usize;
    std::ptr::write_bytes(p as *mut u8, 0, size);
    wr32(p, size as u32);
    let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PITCH | DDSD_PIXELFORMAT;
    wr32(p + 8, d.h);
    wr32(p + 12, d.w);
    wr32(p + 16, d.pitch as u32);
    if !d.chain.is_empty() {
        flags |= DDSD_BACKBUFFERCOUNT;
        wr32(p + 20, d.chain.len() as u32);
    }
    if surface_ptr != 0 {
        flags |= DDSD_LPSURFACE;
        put_ptr(p + doff(36), surface_ptr);
    }
    if let Some((lo, hi)) = d.ck_dest {
        flags |= DDSD_CKDESTBLT;
        wr32(p + doff(48), lo);
        wr32(p + doff(52), hi);
    }
    if let Some((lo, hi)) = d.ck_src {
        flags |= DDSD_CKSRCBLT;
        wr32(p + doff(64), lo);
        wr32(p + doff(68), hi);
    }
    d.fmt.write_ddpf(p + doff(72));
    wr32(p + doff(104), d.caps);
    wr32(p + 4, flags);
}

struct DescIn {
    flags: u32,
    w: u32,
    h: u32,
    backs: u32,
    fmt: Option<Option<Fmt>>,
    caps: u32,
    ck_src: Option<(u32, u32)>,
    ck_dest: Option<(u32, u32)>,
}

unsafe fn read_desc(p: usize) -> DescIn {
    let flags = rd32(p + 4);
    DescIn {
        flags,
        h: if flags & DDSD_HEIGHT != 0 {
            rd32(p + 8)
        } else {
            0
        },
        w: if flags & DDSD_WIDTH != 0 {
            rd32(p + 12)
        } else {
            0
        },
        backs: if flags & DDSD_BACKBUFFERCOUNT != 0 {
            rd32(p + 20)
        } else {
            0
        },
        fmt: (flags & DDSD_PIXELFORMAT != 0).then(|| Fmt::from_ddpf(p + doff(72))),
        caps: if flags & DDSD_CAPS != 0 {
            rd32(p + doff(104))
        } else {
            0
        },
        ck_src: (flags & DDSD_CKSRCBLT != 0).then(|| (rd32(p + doff(64)), rd32(p + doff(68)))),
        ck_dest: (flags & DDSD_CKDESTBLT != 0).then(|| (rd32(p + doff(48)), rd32(p + doff(52)))),
    }
}

// ---------------------------------------------------------------------
// Création des objets
// ---------------------------------------------------------------------

fn pitch_for(w: u32, fmt: Fmt) -> usize {
    (w as usize * fmt.bytes() + 7) & !7
}

fn new_surface(dd: usize, w: u32, h: u32, fmt: Fmt, caps: u32) -> &'static Surface {
    let pitch = pitch_for(w, fmt);
    let s = Box::leak(Box::new(Surface {
        faces: std::array::from_fn(|i| Face {
            vtbl: surface_vtable(SURF_VERSIONS[i]),
            owner: 0,
            ver: SURF_VERSIONS[i],
        }),
        refs: AtomicU32::new(1),
        dd,
        root: AtomicUsize::new(0),
        data: Mutex::new(SurfData {
            w,
            h,
            fmt,
            pitch,
            caps,
            mem: vec![0; pitch * h as usize],
            chain: Vec::new(),
            back: 0,
            attached: Vec::new(),
            palette: 0,
            clipper: 0,
            ck_src: None,
            ck_dest: None,
            locks: 0,
            dc: None,
            private: HashMap::new(),
            unique: 1,
            priority: 0,
            lod: 0,
        }),
    }));
    let me = s as *const Surface as usize;
    for f in s.faces.iter_mut() {
        f.owner = me;
    }
    unsafe {
        let ddo = &*(dd as *const DirectDraw);
        ddo.refs.fetch_add(1, Ordering::AcqRel);
        lock(&ddo.st).surfaces.push(me);
    }
    s
}

unsafe fn free_surface(s: &Surface) {
    let me = s as *const Surface as usize;
    let (chain, attached, pal, clip) = {
        let mut d = lock(&s.data);
        (
            std::mem::take(&mut d.chain),
            std::mem::take(&mut d.attached),
            d.palette,
            d.clipper,
        )
    };
    for c in chain.into_iter().chain(attached) {
        surface_release_obj(&*(c as *const Surface));
    }
    if pal != 0 {
        palette_release(face_addr(&(*(pal as *const Palette)).face));
    }
    if clip != 0 {
        clipper_release(face_addr(&(*(clip as *const Clipper)).face));
    }
    let dd = &*(s.dd as *const DirectDraw);
    {
        let mut st = lock(&dd.st);
        if st.primary == me {
            st.primary = 0;
        }
        st.surfaces.retain(|&x| x != me);
    }
    dd_release_obj(dd);
    drop(Box::from_raw(me as *mut Surface));
}

unsafe fn surface_release_obj(s: &Surface) -> u32 {
    let n = s.refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
    if n == 0 {
        free_surface(s);
    }
    n
}

unsafe fn dd_release_obj(dd: &DirectDraw) -> u32 {
    let n = dd.refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
    if n == 0 {
        let switched = lock(&dd.st).switched;
        if switched {
            restore_mode();
        }
        drop(Box::from_raw(dd as *const DirectDraw as *mut DirectDraw));
    }
    n
}

fn new_ddraw() -> &'static DirectDraw {
    let dd = Box::leak(Box::new(DirectDraw {
        faces: std::array::from_fn(|i| Face {
            vtbl: ddraw_vtable(DD_VERSIONS[i]),
            owner: 0,
            ver: DD_VERSIONS[i],
        }),
        refs: AtomicU32::new(1),
        st: Mutex::new(DdState {
            hwnd: 0,
            coop: 0,
            mode: None,
            switched: false,
            primary: 0,
            surfaces: Vec::new(),
        }),
    }));
    let me = dd as *const DirectDraw as usize;
    for f in dd.faces.iter_mut() {
        f.owner = me;
    }
    dd
}

// ---------------------------------------------------------------------
// IUnknown communs
// ---------------------------------------------------------------------

unsafe fn read_guid(p: usize) -> Option<Guid> {
    if p == 0 {
        None
    } else {
        Some(std::ptr::read_unaligned(p as *const Guid))
    }
}

unsafe extern "win64" fn dd_qi(this: usize, iid: usize, out: usize) -> i32 {
    let Some(g) = read_guid(iid) else {
        return DDERR_INVALIDPARAMS;
    };
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let dd = dd_of(this);
    let idx = if g == IID_IUNKNOWN {
        Some(dd_index(face(this).ver))
    } else {
        IID_DD.iter().position(|x| *x == g)
    };
    match idx {
        Some(i) => {
            dd.refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, face_addr(&dd.faces[i]));
            DD_OK
        }
        None => {
            crate::log_warn!(
                "ddraw",
                "QueryInterface {} refusé (Direct3D immédiat non fourni)",
                super::comobj::guid_str(&g)
            );
            put_ptr(out, 0);
            E_NOINTERFACE
        }
    }
}

unsafe extern "win64" fn dd_addref(this: usize) -> u32 {
    dd_of(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn dd_release(this: usize) -> u32 {
    dd_release_obj(dd_of(this))
}

unsafe extern "win64" fn ok0(_this: usize) -> i32 {
    DD_OK
}
unsafe extern "win64" fn ok1(_this: usize, _a: usize) -> i32 {
    DD_OK
}
unsafe extern "win64" fn ok2(_this: usize, _a: usize, _b: usize) -> i32 {
    DD_OK
}
unsafe extern "win64" fn unsupported1(_this: usize, _a: usize) -> i32 {
    DDERR_UNSUPPORTED
}
unsafe extern "win64" fn unsupported2(_this: usize, _a: usize, _b: usize) -> i32 {
    DDERR_UNSUPPORTED
}
unsafe extern "win64" fn unsupported3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    DDERR_UNSUPPORTED
}
unsafe extern "win64" fn unsupported5(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
) -> i32 {
    DDERR_UNSUPPORTED
}
unsafe extern "win64" fn already_init1(_this: usize, _a: usize) -> i32 {
    DDERR_ALREADYINITIALIZED
}
unsafe extern "win64" fn already_init2(_this: usize, _a: usize, _b: usize) -> i32 {
    DDERR_ALREADYINITIALIZED
}

// ---------------------------------------------------------------------
// IDirectDraw, 2, 4, 7
// ---------------------------------------------------------------------

unsafe extern "win64" fn dd_create_clipper(
    _this: usize,
    _flags: u32,
    out: usize,
    _outer: usize,
) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    put_ptr(out, new_clipper());
    DD_OK
}

fn new_clipper() -> usize {
    let c = Box::leak(Box::new(Clipper {
        face: Face {
            vtbl: clipper_vtable(),
            owner: 0,
            ver: 1,
        },
        refs: AtomicU32::new(1),
        hwnd: AtomicUsize::new(0),
        list: Mutex::new(None),
    }));
    c.face.owner = c as *const Clipper as usize;
    face_addr(&c.face)
}

const DDPCAPS_8BIT: u32 = 0x4;
const DDPCAPS_ALLOW256: u32 = 0x40;

unsafe extern "win64" fn dd_create_palette(
    this: usize,
    flags: u32,
    entries: usize,
    out: usize,
    _outer: usize,
) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let mut table = [0u32; 256];
    let n = if flags & DDPCAPS_8BIT != 0 {
        256
    } else if flags & 0x2 != 0 {
        16 // DDPCAPS_4BIT
    } else if flags & 0x1000 != 0 {
        2 // DDPCAPS_1BIT
    } else if flags & 0x800 != 0 {
        4 // DDPCAPS_2BIT
    } else {
        256
    };
    if entries != 0 {
        for (i, e) in table.iter_mut().enumerate().take(n) {
            let v = rd32(entries + 4 * i);
            *e = (v & 0xFF) << 16 | (v & 0xFF00) | (v >> 16) & 0xFF;
        }
    }
    if flags & DDPCAPS_ALLOW256 == 0 && n == 256 {
        // Entrées 0 et 255 réservées au système : noir et blanc.
        table[0] = 0;
        table[255] = 0xFF_FFFF;
        if entries != 0 {
            table[0] = {
                let v = rd32(entries);
                (v & 0xFF) << 16 | (v & 0xFF00) | (v >> 16) & 0xFF
            };
            table[255] = {
                let v = rd32(entries + 4 * 255);
                (v & 0xFF) << 16 | (v & 0xFF00) | (v >> 16) & 0xFF
            };
        }
    }
    let dd = dd_of(this);
    dd.refs.fetch_add(1, Ordering::AcqRel);
    let p = Box::leak(Box::new(Palette {
        face: Face {
            vtbl: palette_vtable(),
            owner: 0,
            ver: 1,
        },
        refs: AtomicU32::new(1),
        entries: Mutex::new(table),
        caps: flags,
        dd: dd as *const DirectDraw as usize,
    }));
    p.face.owner = p as *const Palette as usize;
    put_ptr(out, face_addr(&p.face));
    DD_OK
}

unsafe extern "win64" fn dd_create_surface(
    this: usize,
    desc: usize,
    out: usize,
    _outer: usize,
) -> i32 {
    if desc == 0 || out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let ver = face(this).ver;
    let dd = dd_of(this);
    let ddp = dd as *const DirectDraw as usize;
    let d = read_desc(desc);
    let sver = match ver {
        7 => 7,
        4 => 4,
        _ => 1,
    };
    let (mode, exclusive) = {
        let st = lock(&dd.st);
        (st.mode, st.coop & DDSCL_EXCLUSIVE != 0)
    };
    let screen_fmt = mode
        .and_then(|m| Fmt::from_bpp(m.2))
        .unwrap_or(Fmt::Xrgb8888);
    let s = if d.caps & DDSCAPS_PRIMARYSURFACE != 0 {
        if lock(&dd.st).primary != 0 {
            return dderr(564); // DDERR_PRIMARYSURFACEALREADYEXISTS
        }
        let (w, h) = match mode {
            Some((w, h, _)) if exclusive => (w, h),
            _ => desktop(),
        };
        let caps = d.caps
            | DDSCAPS_FRONTBUFFER
            | DDSCAPS_VISIBLE
            | DDSCAPS_VIDEOMEMORY
            | DDSCAPS_LOCALVIDMEM;
        let s = new_surface(ddp, w, h, screen_fmt, caps);
        if d.caps & DDSCAPS_FLIP != 0 {
            let n = d.backs.clamp(1, 3);
            let mut chain = Vec::new();
            for i in 0..n {
                let mut bc = DDSCAPS_FLIP | DDSCAPS_VIDEOMEMORY | DDSCAPS_LOCALVIDMEM;
                bc |= d.caps & (DDSCAPS_COMPLEX | 0x2000); // 3DDEVICE
                if i == 0 {
                    bc |= DDSCAPS_BACKBUFFER;
                }
                let b = new_surface(ddp, w, h, screen_fmt, bc);
                b.root
                    .store(s as *const Surface as usize, Ordering::Relaxed);
                chain.push(b as *const Surface as usize);
            }
            let mut sd = lock(&s.data);
            sd.back = chain[0];
            for (i, &c) in chain.iter().enumerate() {
                let next = chain.get(i + 1).copied().unwrap_or(0);
                lock(&(*(c as *const Surface)).data).back = next;
            }
            sd.chain = chain;
        }
        lock(&dd.st).primary = s as *const Surface as usize;
        s
    } else {
        if d.w == 0 || d.h == 0 || d.w > 16384 || d.h > 16384 {
            return DDERR_INVALIDPARAMS;
        }
        let fmt = match d.fmt {
            Some(Some(f)) => f,
            Some(None) => return DDERR_INVALIDPIXELFORMAT,
            None if d.caps & DDSCAPS_ZBUFFER != 0 => Fmt::Rgb565,
            None => screen_fmt,
        };
        let mut caps = d.caps;
        if caps & (DDSCAPS_SYSTEMMEMORY | DDSCAPS_VIDEOMEMORY) == 0 {
            caps |= DDSCAPS_VIDEOMEMORY | DDSCAPS_LOCALVIDMEM;
        }
        if caps & 0x7FFF_FFFF == 0 {
            caps |= DDSCAPS_OFFSCREENPLAIN;
        }
        new_surface(ddp, d.w, d.h, fmt, caps)
    };
    {
        let mut sd = lock(&s.data);
        sd.ck_src = d.ck_src;
        sd.ck_dest = d.ck_dest;
    }
    let _ = d.flags;
    put_ptr(out, face_addr(&s.faces[surf_index(sver)]));
    DD_OK
}

unsafe extern "win64" fn dd_duplicate_surface(this: usize, src: usize, out: usize) -> i32 {
    let Some(src) = surf_arg(src) else {
        return DDERR_INVALIDPARAMS;
    };
    let (w, h, fmt, caps, mem, ck_src) = {
        let d = lock(&src.data);
        if d.caps & DDSCAPS_PRIMARYSURFACE != 0 {
            return DDERR_UNSUPPORTED;
        }
        (d.w, d.h, d.fmt, d.caps, d.mem.clone(), d.ck_src)
    };
    let s = new_surface(dd_of(this) as *const DirectDraw as usize, w, h, fmt, caps);
    {
        let mut d = lock(&s.data);
        d.mem = mem;
        d.ck_src = ck_src;
    }
    let sver = match face(this).ver {
        7 => 7,
        4 => 4,
        _ => 1,
    };
    put_ptr(out, face_addr(&s.faces[surf_index(sver)]));
    DD_OK
}

/// Définitions proposées : les classiques jusqu'à celle du bureau.
fn mode_list() -> Vec<(u32, u32)> {
    let (dw, dh) = desktop();
    let mut v: Vec<(u32, u32)> = [
        (320, 200),
        (320, 240),
        (640, 400),
        (640, 480),
        (800, 600),
        (1024, 768),
        (1152, 864),
        (1280, 720),
        (1280, 800),
        (1280, 960),
        (1280, 1024),
        (1366, 768),
        (1440, 900),
        (1600, 900),
        (1600, 1200),
        (1680, 1050),
        (1920, 1080),
        (1920, 1200),
        (2560, 1440),
        (3840, 2160),
    ]
    .into_iter()
    .filter(|&(w, h)| w <= dw && h <= dh)
    .collect();
    if !v.contains(&(dw, dh)) {
        v.push((dw, dh));
    }
    v
}

unsafe extern "win64" fn dd_enum_display_modes(
    this: usize,
    flags: u32,
    filter: usize,
    ctx: usize,
    cb: usize,
) -> i32 {
    if cb == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let v2 = face(this).ver >= 4;
    let f = (filter != 0).then(|| read_desc(filter));
    let buf = [0u8; 160];
    let p = buf.as_ptr() as usize;
    for (w, h) in mode_list() {
        for bpp in [8u32, 16, 24, 32] {
            if let Some(f) = &f {
                if (f.w != 0 && f.w != w) || (f.h != 0 && f.h != h) {
                    continue;
                }
                if let Some(Some(ff)) = f.fmt {
                    if ff.bytes() * 8 != bpp as usize {
                        continue;
                    }
                }
            }
            let fmt = Fmt::from_bpp(bpp).unwrap_or(Fmt::Xrgb8888);
            let d = SurfData {
                w,
                h,
                fmt,
                pitch: pitch_for(w, fmt),
                caps: 0,
                mem: Vec::new(),
                chain: Vec::new(),
                back: 0,
                attached: Vec::new(),
                palette: 0,
                clipper: 0,
                ck_src: None,
                ck_dest: None,
                locks: 0,
                dc: None,
                private: HashMap::new(),
                unique: 0,
                priority: 0,
                lod: 0,
            };
            write_desc(p, &d, v2, 0);
            wr32(
                p + 4,
                DDSD_HEIGHT | DDSD_WIDTH | DDSD_PITCH | DDSD_PIXELFORMAT | DDSD_REFRESHRATE,
            );
            if flags & DDEDM_REFRESHRATES != 0 || true {
                wr32(p + 24, 60);
            }
            if call_back(cb, &[p, ctx]) == DDENUMRET_CANCEL {
                return DD_OK;
            }
        }
    }
    DD_OK
}

unsafe extern "win64" fn dd_enum_surfaces(
    _this: usize,
    _flags: u32,
    _desc: usize,
    _ctx: usize,
    _cb: usize,
) -> i32 {
    DD_OK
}

unsafe extern "win64" fn dd_flip_to_gdi(_this: usize) -> i32 {
    DD_OK
}

/// Remplit une structure DDCAPS (taille donnée par dwSize).
unsafe fn fill_caps(p: usize) {
    if p == 0 {
        return;
    }
    let size = (rd32(p) as usize).clamp(4, 512);
    std::ptr::write_bytes((p + 4) as *mut u8, 0, size - 4);
    let set = |o: usize, v: u32| {
        if o + 4 <= size {
            wr32(p + o, v)
        }
    };
    // DDCAPS_BLT | BLTCOLORFILL | BLTSTRETCH | COLORKEY | PALETTE | CANBLTSYSMEM
    // | BLTDEPTHFILL | CANCLIP | CANCLIPSTRETCHED.
    set(
        4,
        0x40 | 0x0400_0000
            | 0x200
            | 0x40_0000
            | 0x8000
            | 0x8000_0000
            | 0x1000_0000
            | 0x2000_0000
            | 0x4000_0000,
    );
    set(8, 0x8_0000 | 0x2_0000); // DDCAPS2_WIDESURFACES | DDCAPS2_NOPAGELOCKREQUIRED
    set(12, 0x200 | 0x2); // DDCKEYCAPS_SRCBLT | DDCKEYCAPS_DESTBLT
    set(16, 0x40 | 0x20 | 0x10 | 0x2_0000 | 0x4_0000); // FX : miroirs, étirements
    set(24, 0x4 | 0x40 | 0x400); // DDPCAPS_8BIT | ALLOW256 | PRIMARYSURFACE
    set(60, 256 << 20); // dwVidMemTotal
    set(64, 224 << 20); // dwVidMemFree
}

unsafe extern "win64" fn dd_get_caps(_this: usize, drv: usize, hel: usize) -> i32 {
    fill_caps(drv);
    fill_caps(hel);
    DD_OK
}

unsafe extern "win64" fn dd_get_display_mode(this: usize, desc: usize) -> i32 {
    if desc == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let dd = dd_of(this);
    let (w, h, bpp) = lock(&dd.st).mode.unwrap_or_else(|| {
        let (w, h) = desktop();
        (w, h, 32)
    });
    let fmt = Fmt::from_bpp(bpp).unwrap_or(Fmt::Xrgb8888);
    let v2 = is_desc2(desc, face(this).ver >= 4);
    let d = SurfData {
        w,
        h,
        fmt,
        pitch: pitch_for(w, fmt),
        caps: 0,
        mem: Vec::new(),
        chain: Vec::new(),
        back: 0,
        attached: Vec::new(),
        palette: 0,
        clipper: 0,
        ck_src: None,
        ck_dest: None,
        locks: 0,
        dc: None,
        private: HashMap::new(),
        unique: 0,
        priority: 0,
        lod: 0,
    };
    write_desc(desc, &d, v2, 0);
    wr32(
        desc + 4,
        DDSD_HEIGHT | DDSD_WIDTH | DDSD_PITCH | DDSD_PIXELFORMAT | DDSD_REFRESHRATE,
    );
    wr32(desc + 24, 60);
    DD_OK
}

unsafe extern "win64" fn dd_get_fourcc(_this: usize, num: usize, _codes: usize) -> i32 {
    if num != 0 {
        wr32(num, 0);
    }
    DD_OK
}

unsafe extern "win64" fn dd_get_gdi_surface(this: usize, out: usize) -> i32 {
    let dd = dd_of(this);
    let prim = lock(&dd.st).primary;
    if prim == 0 || out == 0 {
        return DDERR_NOTFOUND;
    }
    let s = &*(prim as *const Surface);
    s.refs.fetch_add(1, Ordering::AcqRel);
    let sver = match face(this).ver {
        7 => 7,
        4 => 4,
        _ => 1,
    };
    put_ptr(out, face_addr(&s.faces[surf_index(sver)]));
    DD_OK
}

unsafe extern "win64" fn dd_get_monitor_frequency(_this: usize, out: usize) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    wr32(out, 60);
    DD_OK
}

unsafe extern "win64" fn dd_get_scan_line(this: usize, out: usize) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let h = lock(&dd_of(this).st).mode.map_or(desktop().1, |m| m.1);
    let total = h as f64 * 1.05;
    let line = (frame_phase() * total) as u32;
    if line >= h {
        wr32(out, 0);
        return dderr(537); // DDERR_VERTICALBLANKINPROGRESS
    }
    wr32(out, line);
    DD_OK
}

unsafe extern "win64" fn dd_get_vblank_status(_this: usize, out: usize) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    wr32(out, (frame_phase() > 0.95) as u32);
    DD_OK
}

fn restore_mode() {
    unsafe {
        crate::dlls::displaymode::ChangeDisplaySettingsW(std::ptr::null(), 0);
    }
}

unsafe extern "win64" fn dd_restore_display_mode(this: usize) -> i32 {
    let dd = dd_of(this);
    let switched = {
        let mut st = lock(&dd.st);
        st.mode = None;
        std::mem::replace(&mut st.switched, false)
    };
    if switched {
        restore_mode();
    }
    DD_OK
}

const GWL_STYLE: i32 = -16;
const WS_POPUP: u32 = 0x8000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const SWP_SHOWWINDOW: u32 = 0x40;
const SWP_FRAMECHANGED: u32 = 0x20;

/// Fenêtre du jeu en plein écran exclusif : sans bordure, au premier plan,
/// à la taille du mode (qui est alors celle de l'écran si la définition a
/// pu changer).
unsafe fn place_window(hwnd: usize, w: u32, h: u32, switched: bool) {
    if hwnd == 0 {
        return;
    }
    api!(
        "user32",
        "SetWindowLongW",
        fn(usize, i32, i32) -> i32,
        hwnd,
        GWL_STYLE,
        (WS_POPUP | WS_VISIBLE) as i32
    );
    let (x, y) = if switched {
        (0, 0)
    } else {
        let (dw, dh) = desktop();
        (
            (dw as i32 - w as i32).max(0) / 2,
            (dh as i32 - h as i32).max(0) / 2,
        )
    };
    api!(
        "user32",
        "SetWindowPos",
        fn(usize, isize, i32, i32, i32, i32, u32) -> i32,
        hwnd,
        -1, // HWND_TOPMOST
        x,
        y,
        w as i32,
        h as i32,
        SWP_SHOWWINDOW | SWP_FRAMECHANGED
    );
}

unsafe extern "win64" fn dd_set_coop(this: usize, hwnd: usize, flags: u32) -> i32 {
    let dd = dd_of(this);
    let (mode, switched) = {
        let mut st = lock(&dd.st);
        st.hwnd = hwnd;
        st.coop = flags;
        (st.mode, st.switched)
    };
    if flags & DDSCL_FULLSCREEN != 0 && flags & DDSCL_EXCLUSIVE != 0 {
        if let Some((w, h, _)) = mode {
            place_window(hwnd, w, h, switched);
        }
    }
    DD_OK
}

unsafe fn set_mode(dd: &DirectDraw, w: u32, h: u32, bpp: u32) -> i32 {
    if w == 0 || h == 0 || Fmt::from_bpp(bpp).is_none() {
        return DDERR_INVALIDPARAMS;
    }
    let (hwnd, exclusive, prev_switched) = {
        let st = lock(&dd.st);
        (st.hwnd, st.coop & DDSCL_EXCLUSIVE != 0, st.switched)
    };
    let (dw, dh) = desktop();
    let mut switched = prev_switched;
    let allow = std::env::var("WINLUX_DDRAW_MODESWITCH").map_or(true, |v| v != "0");
    if exclusive && allow && (w, h) != (dw, dh) {
        let mut dm = [0u8; 220];
        dm[68..70].copy_from_slice(&220u16.to_le_bytes());
        dm[72..76].copy_from_slice(&(0x8_0000u32 | 0x10_0000).to_le_bytes());
        dm[172..176].copy_from_slice(&w.to_le_bytes());
        dm[176..180].copy_from_slice(&h.to_le_bytes());
        let r = crate::dlls::displaymode::ChangeDisplaySettingsW(dm.as_ptr(), 0x4); // CDS_FULLSCREEN
        switched = r == 0;
        if !switched {
            crate::log_warn!(
                "ddraw",
                "définition {w}x{h} refusée par l'écran ({r}) : image au centre du bureau"
            );
        }
    } else if (w, h) == (dw, dh) && prev_switched {
        restore_mode();
        switched = false;
    }
    let prim = {
        let mut st = lock(&dd.st);
        st.mode = Some((w, h, bpp));
        st.switched = switched;
        st.primary
    };
    // Une primaire déjà créée suit le nouveau mode (les jeux la recréent
    // en général après SetDisplayMode).
    if prim != 0 && exclusive {
        let s = &*(prim as *const Surface);
        let fmt = Fmt::from_bpp(bpp).unwrap_or(Fmt::Xrgb8888);
        let chain = {
            let mut d = lock(&s.data);
            resize(&mut d, w, h, fmt);
            d.chain.clone()
        };
        for c in chain {
            resize(&mut lock(&(*(c as *const Surface)).data), w, h, fmt);
        }
    }
    if exclusive {
        place_window(hwnd, w, h, switched);
    }
    DD_OK
}

fn resize(d: &mut SurfData, w: u32, h: u32, fmt: Fmt) {
    d.w = w;
    d.h = h;
    d.fmt = fmt;
    d.pitch = pitch_for(w, fmt);
    d.mem = vec![0; d.pitch * h as usize];
}

unsafe extern "win64" fn dd_set_display_mode1(this: usize, w: u32, h: u32, bpp: u32) -> i32 {
    set_mode(dd_of(this), w, h, bpp)
}

unsafe extern "win64" fn dd_set_display_mode2(
    this: usize,
    w: u32,
    h: u32,
    bpp: u32,
    _refresh: u32,
    _flags: u32,
) -> i32 {
    set_mode(dd_of(this), w, h, bpp)
}

unsafe extern "win64" fn dd_wait_vblank(_this: usize, flags: u32, _event: usize) -> i32 {
    // DDWAITVB_BLOCKBEGIN (1) ou BLOCKEND (4) : on attend la prochaine image.
    if flags & 0x5 != 0 {
        wait_vblank();
    }
    DD_OK
}

unsafe extern "win64" fn dd_get_available_vid_mem(
    _this: usize,
    _caps: usize,
    total: usize,
    free: usize,
) -> i32 {
    if total != 0 {
        wr32(total, 256 << 20);
    }
    if free != 0 {
        wr32(free, 224 << 20);
    }
    DD_OK
}

unsafe extern "win64" fn dd_get_surface_from_dc(this: usize, hdc: usize, out: usize) -> i32 {
    let dd = dd_of(this);
    let list = lock(&dd.st).surfaces.clone();
    for s in list {
        let s = &*(s as *const Surface);
        if lock(&s.data).dc.is_some_and(|d| d.0 == hdc) {
            s.refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, face_addr(&s.faces[surf_index(7)]));
            return DD_OK;
        }
    }
    put_ptr(out, 0);
    DDERR_NOTFOUND
}

/// DDDEVICEIDENTIFIER(2) : szDriver[512], szDescription[512],
/// liDriverVersion, dwVendorId, dwDeviceId, dwSubSysId, dwRevision,
/// guidDeviceIdentifier, (dwWHQLLevel).
unsafe extern "win64" fn dd_get_device_identifier(this: usize, out: usize, _flags: u32) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let size = if face(this).ver >= 7 { 1072 } else { 1068 };
    std::ptr::write_bytes(out as *mut u8, 0, size);
    put_str(out, 512, "winlux");
    put_str(out + 512, 512, "Winlux DirectDraw");
    (out as *mut u8)
        .add(1024)
        .cast::<u64>()
        .write_unaligned(0x0006_0001_0000_0001);
    DD_OK
}

unsafe extern "win64" fn dd_test_coop(_this: usize) -> i32 {
    DD_OK
}

fn ddraw_vtable(ver: u8) -> usize {
    let a = |f: usize, n: u8| (f, n);
    let mut m = vec![
        a(dd_qi as *const () as usize, 2),
        a(dd_addref as *const () as usize, 0),
        a(dd_release as *const () as usize, 0),
        a(ok0 as *const () as usize, 0), // Compact
        a(dd_create_clipper as *const () as usize, 3),
        a(dd_create_palette as *const () as usize, 4),
        a(dd_create_surface as *const () as usize, 3),
        a(dd_duplicate_surface as *const () as usize, 2),
        a(dd_enum_display_modes as *const () as usize, 4),
        a(dd_enum_surfaces as *const () as usize, 4),
        a(dd_flip_to_gdi as *const () as usize, 0),
        a(dd_get_caps as *const () as usize, 2),
        a(dd_get_display_mode as *const () as usize, 1),
        a(dd_get_fourcc as *const () as usize, 2),
        a(dd_get_gdi_surface as *const () as usize, 1),
        a(dd_get_monitor_frequency as *const () as usize, 1),
        a(dd_get_scan_line as *const () as usize, 1),
        a(dd_get_vblank_status as *const () as usize, 1),
        a(already_init1 as *const () as usize, 1), // Initialize
        a(dd_restore_display_mode as *const () as usize, 0),
        a(dd_set_coop as *const () as usize, 2),
        if ver == 1 {
            a(dd_set_display_mode1 as *const () as usize, 3)
        } else {
            a(dd_set_display_mode2 as *const () as usize, 5)
        },
        a(dd_wait_vblank as *const () as usize, 2),
    ];
    if ver >= 2 {
        m.push(a(dd_get_available_vid_mem as *const () as usize, 3));
    }
    if ver >= 4 {
        m.push(a(dd_get_surface_from_dc as *const () as usize, 2));
        m.push(a(ok0 as *const () as usize, 0)); // RestoreAllSurfaces
        m.push(a(dd_test_coop as *const () as usize, 0));
        m.push(a(dd_get_device_identifier as *const () as usize, 2));
    }
    if ver >= 7 {
        m.push(a(unsupported3 as *const () as usize, 3)); // StartModeTest
        m.push(a(unsupported2 as *const () as usize, 2)); // EvaluateMode
    }
    let name: &'static str = match ver {
        1 => "IDirectDraw",
        2 => "IDirectDraw2",
        4 => "IDirectDraw4",
        _ => "IDirectDraw7",
    };
    vtable(name, &m)
}

// ---------------------------------------------------------------------
// IDirectDrawSurface 1 à 7
// ---------------------------------------------------------------------

unsafe extern "win64" fn s_qi(this: usize, iid: usize, out: usize) -> i32 {
    let Some(g) = read_guid(iid) else {
        return DDERR_INVALIDPARAMS;
    };
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let s = surf_of(this);
    let idx = if g == IID_IUNKNOWN {
        Some(surf_index(face(this).ver))
    } else {
        IID_SURF.iter().position(|x| *x == g)
    };
    match idx {
        Some(i) => {
            s.refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, face_addr(&s.faces[i]));
            DD_OK
        }
        None => {
            crate::log_warn!(
                "ddraw",
                "surface : QueryInterface {} refusé",
                super::comobj::guid_str(&g)
            );
            put_ptr(out, 0);
            E_NOINTERFACE
        }
    }
}

unsafe extern "win64" fn s_addref(this: usize) -> u32 {
    surf_of(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn s_release(this: usize) -> u32 {
    surface_release_obj(surf_of(this))
}

unsafe extern "win64" fn s_add_attached(this: usize, other: usize) -> i32 {
    let Some(o) = surf_arg(other) else {
        return DDERR_INVALIDPARAMS;
    };
    o.refs.fetch_add(1, Ordering::AcqRel);
    lock(&surf_of(this).data)
        .attached
        .push(o as *const Surface as usize);
    DD_OK
}

unsafe extern "win64" fn s_delete_attached(this: usize, _flags: u32, other: usize) -> i32 {
    let s = surf_of(this);
    let target = surf_arg(other).map(|o| o as *const Surface as usize);
    let removed: Vec<usize> = {
        let mut d = lock(&s.data);
        let (gone, keep): (Vec<usize>, Vec<usize>) = std::mem::take(&mut d.attached)
            .into_iter()
            .partition(|&a| target.is_none_or(|t| t == a));
        d.attached = keep;
        gone
    };
    if removed.is_empty() {
        return dderr(240); // DDERR_SURFACENOTATTACHED
    }
    for r in removed {
        surface_release_obj(&*(r as *const Surface));
    }
    DD_OK
}

/// Copie d'une zone source, détachée de sa surface (pas de double verrou,
/// et Blt d'une surface sur elle-même correct).
struct SrcCopy {
    fmt: Fmt,
    w: usize,
    h: usize,
    pixels: Vec<u8>,
    pal: [u32; 256],
    key: Option<(u32, u32)>,
}

unsafe fn grab(s: &Surface, r: Rect) -> SrcCopy {
    let d = lock(&s.data);
    let bpp = d.fmt.bytes();
    let (w, h) = (r.w().max(0) as usize, r.h().max(0) as usize);
    let mut pixels = vec![0u8; w * h * bpp];
    for y in 0..h {
        let from = (r.t as usize + y) * d.pitch + r.l as usize * bpp;
        pixels[y * w * bpp..(y + 1) * w * bpp].copy_from_slice(&d.mem[from..from + w * bpp]);
    }
    let pal = effective_palette(s, &d);
    SrcCopy {
        fmt: d.fmt,
        w,
        h,
        pixels,
        pal,
        key: d.ck_src,
    }
}

fn bounds(d: &SurfData) -> Rect {
    Rect {
        l: 0,
        t: 0,
        r: d.w as i32,
        b: d.h as i32,
    }
}

/// Zone autorisée d'une surface : ses bords et, pour la primaire avec un
/// découpeur, la fenêtre (ou la liste de rectangles) du découpeur.
unsafe fn clip_bounds(d: &SurfData) -> Rect {
    let b = bounds(d);
    if d.clipper == 0 {
        return b;
    }
    let c = &*(d.clipper as *const Clipper);
    if let Some(list) = lock(&c.list).as_ref() {
        if let Some(first) = list.first() {
            let mut u = *first;
            for r in list {
                u.l = u.l.min(r.l);
                u.t = u.t.min(r.t);
                u.r = u.r.max(r.r);
                u.b = u.b.max(r.b);
            }
            return b.intersect(&u);
        }
    }
    let hwnd = c.hwnd.load(Ordering::Relaxed);
    if hwnd != 0 && is_front(d) {
        if let Some((client, x, y)) = client_rect(hwnd) {
            return b.intersect(&Rect {
                l: x,
                t: y,
                r: x + client.w(),
                b: y + client.h(),
            });
        }
    }
    b
}

/// Remplit `r` (déjà découpé) de la valeur brute `v`.
fn fill(d: &mut SurfData, r: Rect, v: u32) {
    let bpp = d.fmt.bytes();
    let bytes = v.to_le_bytes();
    for y in r.t..r.b {
        let row = y as usize * d.pitch;
        for x in r.l..r.r {
            let at = row + x as usize * bpp;
            d.mem[at..at + bpp].copy_from_slice(&bytes[..bpp]);
        }
    }
}

struct BltOpts {
    src_key: Option<(u32, u32)>,
    dest_key: Option<(u32, u32)>,
    mirror_lr: bool,
    mirror_ud: bool,
}

/// Copie `src` (entière) dans `dst_rect` de `d`, étirée, découpée à `clip`.
fn draw(d: &mut SurfData, dst_rect: Rect, clip: Rect, src: &SrcCopy, o: &BltOpts) {
    let r = dst_rect.intersect(&clip);
    if r.empty() || src.w == 0 || src.h == 0 {
        return;
    }
    let (dw, dh) = (dst_rect.w() as i64, dst_rect.h() as i64);
    let dbpp = d.fmt.bytes();
    let sbpp = src.fmt.bytes();
    let dpal = palette_of(d);
    let mut inv = Inverse::new(&dpal);
    let same = src.fmt == d.fmt;
    let smask = src.fmt.mask();
    let dmask = d.fmt.mask();
    // Copie directe ligne à ligne : même format, même taille, sans clé.
    if same
        && dw as usize == src.w
        && dh as usize == src.h
        && o.src_key.is_none()
        && o.dest_key.is_none()
        && !o.mirror_lr
        && !o.mirror_ud
    {
        let n = r.w() as usize * dbpp;
        for y in r.t..r.b {
            let sy = (y - dst_rect.t) as usize;
            let sx = (r.l - dst_rect.l) as usize;
            let from = (sy * src.w + sx) * sbpp;
            let to = y as usize * d.pitch + r.l as usize * dbpp;
            d.mem[to..to + n].copy_from_slice(&src.pixels[from..from + n]);
        }
        return;
    }
    for y in r.t..r.b {
        let mut sy = ((y - dst_rect.t) as i64 * src.h as i64 / dh) as usize;
        if o.mirror_ud {
            sy = src.h - 1 - sy;
        }
        let row = y as usize * d.pitch;
        for x in r.l..r.r {
            let mut sx = ((x - dst_rect.l) as i64 * src.w as i64 / dw) as usize;
            if o.mirror_lr {
                sx = src.w - 1 - sx;
            }
            let sv = src.fmt.read(&src.pixels[(sy * src.w + sx) * sbpp..]) & smask;
            if let Some((lo, hi)) = o.src_key {
                if sv >= lo & smask && sv <= hi.max(lo) & smask {
                    continue;
                }
            }
            let at = row + x as usize * dbpp;
            if let Some((lo, hi)) = o.dest_key {
                let dv = d.fmt.read(&d.mem[at..]) & dmask;
                if !(dv >= lo & dmask && dv <= hi.max(lo) & dmask) {
                    continue;
                }
            }
            let v = if same {
                sv
            } else {
                d.fmt.encode_argb(src.fmt.to_argb(sv, &src.pal), &mut inv)
            };
            d.fmt.write(&mut d.mem[at..], v);
        }
    }
}

unsafe fn after_write(s: &Surface) {
    let front = {
        let mut d = lock(&s.data);
        d.unique = d.unique.wrapping_add(1);
        is_front(&d)
    };
    if front {
        present(s);
    }
}

/// DDBLTFX : décalages x86 / x64 des champs utiles.
fn fx_off(x86_off: usize) -> usize {
    if x86() {
        x86_off
    } else {
        match x86_off {
            80 => 104,
            84 => 112,
            88 => 116,
            92 => 120,
            96 => 124,
            o => o,
        }
    }
}

unsafe extern "win64" fn s_blt(
    this: usize,
    dst_rect: usize,
    src: usize,
    src_rect: usize,
    flags: u32,
    fx: usize,
) -> i32 {
    let s = surf_of(this);
    let (full, clip) = {
        let d = lock(&s.data);
        (bounds(&d), clip_bounds(&d))
    };
    let dr = Rect::read(dst_rect).unwrap_or(full);
    if dr.empty() {
        return DD_OK;
    }
    if flags & (DDBLT_COLORFILL | DDBLT_DEPTHFILL) != 0
        || (flags & DDBLT_ROP != 0 && fx != 0 && matches!(rd32(fx + 8), BLACKNESS | WHITENESS))
    {
        if fx == 0 {
            return DDERR_INVALIDPARAMS;
        }
        let v = if flags & (DDBLT_COLORFILL | DDBLT_DEPTHFILL) != 0 {
            rd32(fx + fx_off(80))
        } else if rd32(fx + 8) == WHITENESS {
            u32::MAX
        } else {
            0
        };
        {
            let mut d = lock(&s.data);
            let r = dr.intersect(&clip);
            if !r.empty() {
                fill(&mut d, r, v);
            }
        }
        after_write(s);
        return DD_OK;
    }
    let Some(src_s) = surf_arg(src) else {
        // Blt sans source et sans remplissage : rien à faire.
        return DD_OK;
    };
    let sfull = bounds(&lock(&src_s.data));
    let mut sr = Rect::read(src_rect).unwrap_or(sfull);
    // Source hors de la surface : on la ramène aux bords, et la
    // destination proportionnellement.
    let mut dr = dr;
    let clipped = sr.intersect(&sfull);
    if clipped != sr {
        if clipped.empty() || sr.empty() {
            return DD_OK;
        }
        let (sw, sh) = (sr.w() as i64, sr.h() as i64);
        let (dw, dh) = (dr.w() as i64, dr.h() as i64);
        let nl = dr.l as i64 + (clipped.l - sr.l) as i64 * dw / sw;
        let nr = dr.l as i64 + (clipped.r - sr.l) as i64 * dw / sw;
        let nt = dr.t as i64 + (clipped.t - sr.t) as i64 * dh / sh;
        let nb = dr.t as i64 + (clipped.b - sr.t) as i64 * dh / sh;
        dr = Rect {
            l: nl as i32,
            t: nt as i32,
            r: nr as i32,
            b: nb as i32,
        };
        sr = clipped;
    }
    let copy = grab(src_s, sr);
    let mut o = BltOpts {
        src_key: None,
        dest_key: None,
        mirror_lr: false,
        mirror_ud: false,
    };
    if flags & DDBLT_KEYSRC != 0 {
        o.src_key = copy.key;
    }
    if flags & DDBLT_KEYSRCOVERRIDE != 0 && fx != 0 {
        o.src_key = Some((rd32(fx + fx_off(92)), rd32(fx + fx_off(96))));
    }
    if flags & DDBLT_KEYDEST != 0 {
        o.dest_key = lock(&s.data).ck_dest;
    }
    if flags & DDBLT_KEYDESTOVERRIDE != 0 && fx != 0 {
        o.dest_key = Some((rd32(fx + fx_off(84)), rd32(fx + fx_off(88))));
    }
    if flags & DDBLT_DDFX != 0 && fx != 0 {
        let f = rd32(fx + 4);
        o.mirror_lr = f & DDBLTFX_MIRRORLEFTRIGHT != 0;
        o.mirror_ud = f & DDBLTFX_MIRRORUPDOWN != 0;
    }
    {
        let mut d = lock(&s.data);
        draw(&mut d, dr, clip, &copy, &o);
    }
    after_write(s);
    DD_OK
}

unsafe extern "win64" fn s_blt_fast(
    this: usize,
    x: u32,
    y: u32,
    src: usize,
    src_rect: usize,
    trans: u32,
) -> i32 {
    let s = surf_of(this);
    let Some(src_s) = surf_arg(src) else {
        return DDERR_INVALIDPARAMS;
    };
    let sfull = bounds(&lock(&src_s.data));
    let sr = Rect::read(src_rect).unwrap_or(sfull).intersect(&sfull);
    if sr.empty() {
        return DD_OK;
    }
    let copy = grab(src_s, sr);
    let (x, y) = (x as i32, y as i32);
    let dr = Rect {
        l: x,
        t: y,
        r: x + sr.w(),
        b: y + sr.h(),
    };
    let o = BltOpts {
        src_key: if trans & DDBLTFAST_SRCCOLORKEY != 0 {
            copy.key
        } else {
            None
        },
        dest_key: if trans & DDBLTFAST_DESTCOLORKEY != 0 {
            lock(&s.data).ck_dest
        } else {
            None
        },
        mirror_lr: false,
        mirror_ud: false,
    };
    {
        let mut d = lock(&s.data);
        let clip = bounds(&d);
        draw(&mut d, dr, clip, &copy, &o);
    }
    after_write(s);
    DD_OK
}

unsafe extern "win64" fn s_enum_attached(this: usize, ctx: usize, cb: usize) -> i32 {
    if cb == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let s = surf_of(this);
    let ver = face(this).ver;
    let (list, v2) = {
        let d = lock(&s.data);
        let mut l: Vec<usize> = Vec::new();
        if d.back != 0 {
            l.push(d.back);
        }
        l.extend(d.attached.iter().copied());
        (l, ver >= 4)
    };
    let desc = [0u8; 160];
    for o in list {
        let os = &*(o as *const Surface);
        os.refs.fetch_add(1, Ordering::AcqRel);
        write_desc(desc.as_ptr() as usize, &lock(&os.data), v2, 0);
        let f = face_addr(&os.faces[surf_index(ver)]);
        if call_back(cb, &[f, desc.as_ptr() as usize, ctx]) == DDENUMRET_CANCEL {
            break;
        }
    }
    DD_OK
}

unsafe extern "win64" fn s_flip(this: usize, target: usize, flags: u32) -> i32 {
    let s = surf_of(this);
    let chain = {
        let d = lock(&s.data);
        if d.chain.is_empty() {
            return DDERR_NOTFLIPPABLE;
        }
        d.chain.clone()
    };
    // Rotation des images : la primaire reçoit le premier tampon arrière
    // (ou la cible donnée), chaque tampon le suivant, le dernier l'ancienne
    // primaire.
    let mut order: Vec<usize> = vec![s as *const Surface as usize];
    order.extend(chain.iter().copied());
    if let Some(t) = surf_arg(target) {
        let t = t as *const Surface as usize;
        if let Some(pos) = order.iter().position(|&x| x == t) {
            order.swap(1, pos);
        }
    }
    let mut mems: Vec<Vec<u8>> = order
        .iter()
        .map(|&p| std::mem::take(&mut lock(&(*(p as *const Surface)).data).mem))
        .collect();
    mems.rotate_left(1);
    for (&p, m) in order.iter().zip(mems) {
        lock(&(*(p as *const Surface)).data).mem = m;
    }
    if flags & DDFLIP_NOVSYNC == 0 {
        wait_vblank();
    }
    present(s);
    DD_OK
}

unsafe extern "win64" fn s_get_attached(this: usize, caps: usize, out: usize) -> i32 {
    if caps == 0 || out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let want = rd32(caps);
    let s = surf_of(this);
    let ver = face(this).ver;
    let (back, attached) = {
        let d = lock(&s.data);
        (d.back, d.attached.clone())
    };
    let mut cand = Vec::new();
    if back != 0 {
        cand.push(back);
    }
    cand.extend(attached);
    for c in cand {
        let cs = &*(c as *const Surface);
        let ccaps = lock(&cs.data).caps;
        if ccaps & want == want {
            cs.refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, face_addr(&cs.faces[surf_index(ver)]));
            return DD_OK;
        }
    }
    put_ptr(out, 0);
    DDERR_NOTFOUND
}

unsafe extern "win64" fn s_get_caps(this: usize, caps: usize) -> i32 {
    if caps == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let c = lock(&surf_of(this).data).caps;
    wr32(caps, c);
    if face(this).ver >= 4 {
        std::ptr::write_bytes((caps + 4) as *mut u8, 0, 12);
    }
    DD_OK
}

unsafe extern "win64" fn s_get_clipper(this: usize, out: usize) -> i32 {
    let c = lock(&surf_of(this).data).clipper;
    if c == 0 {
        put_ptr(out, 0);
        return DDERR_NOCLIPPERATTACHED;
    }
    let c = &*(c as *const Clipper);
    c.refs.fetch_add(1, Ordering::AcqRel);
    put_ptr(out, face_addr(&c.face));
    DD_OK
}

unsafe extern "win64" fn s_get_color_key(this: usize, flags: u32, key: usize) -> i32 {
    if key == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let d = lock(&surf_of(this).data);
    let k = if flags & DDCKEY_SRCBLT != 0 {
        d.ck_src
    } else if flags & DDCKEY_DESTBLT != 0 {
        d.ck_dest
    } else {
        None
    };
    match k {
        Some((lo, hi)) => {
            wr32(key, lo);
            wr32(key + 4, hi);
            DD_OK
        }
        None => DDERR_NOCOLORKEY,
    }
}

unsafe extern "win64" fn s_set_color_key(this: usize, flags: u32, key: usize) -> i32 {
    let mut d = lock(&surf_of(this).data);
    let k = if key == 0 {
        None
    } else {
        let lo = rd32(key);
        let hi = if flags & DDCKEY_COLORSPACE != 0 {
            rd32(key + 4)
        } else {
            lo
        };
        Some((lo, hi))
    };
    if flags & DDCKEY_SRCBLT != 0 {
        d.ck_src = k;
    } else if flags & DDCKEY_DESTBLT != 0 {
        d.ck_dest = k;
    }
    DD_OK
}

/// BITMAPINFO 32 bits haut en bas.
fn bmi32(w: u32, h: u32) -> [u8; 44] {
    let mut bmi = [0u8; 44];
    bmi[0..4].copy_from_slice(&40u32.to_le_bytes());
    bmi[4..8].copy_from_slice(&(w as i32).to_le_bytes());
    bmi[8..12].copy_from_slice(&(-(h as i32)).to_le_bytes());
    bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
    bmi[14..16].copy_from_slice(&32u16.to_le_bytes());
    bmi
}

unsafe extern "win64" fn s_get_dc(this: usize, out: usize) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let s = surf_of(this);
    let mut d = lock(&s.data);
    if d.dc.is_some() {
        return dderr(588); // DDERR_DCALREADYCREATED
    }
    let pal = effective_palette(s, &d);
    let hdc = api!("gdi32", "CreateCompatibleDC", fn(usize) -> usize, 0).unwrap_or(0);
    if hdc == 0 {
        return DDERR_CANTCREATEDC;
    }
    let bmi = bmi32(d.w, d.h);
    let mut bits: usize = 0;
    let hbm = api!(
        "gdi32",
        "CreateDIBSection",
        fn(usize, *const u8, u32, *mut usize, usize, u32) -> usize,
        hdc,
        bmi.as_ptr(),
        0,
        &mut bits,
        0,
        0
    )
    .unwrap_or(0);
    if hbm == 0 || bits == 0 {
        api!("gdi32", "DeleteDC", fn(usize) -> i32, hdc);
        return DDERR_CANTCREATEDC;
    }
    let old = api!("gdi32", "SelectObject", fn(usize, usize) -> usize, hdc, hbm).unwrap_or(0);
    let bpp = d.fmt.bytes();
    let px = bits as *mut u32;
    for y in 0..d.h as usize {
        for x in 0..d.w as usize {
            let v = d.fmt.read(&d.mem[y * d.pitch + x * bpp..]);
            *px.add(y * d.w as usize + x) = d.fmt.to_argb(v, &pal) & 0xFF_FFFF;
        }
    }
    d.dc = Some((hdc, hbm, old, bits));
    put_ptr(out, hdc);
    DD_OK
}

unsafe extern "win64" fn s_release_dc(this: usize, hdc: usize) -> i32 {
    let s = surf_of(this);
    {
        let mut d = lock(&s.data);
        let Some((dc, hbm, old, bits)) = d.dc else {
            return DDERR_NODC;
        };
        if hdc != 0 && hdc != dc {
            return DDERR_INVALIDPARAMS;
        }
        api!("gdi32", "GdiFlush", fn() -> i32);
        if bits != 0 {
            let pal = effective_palette(s, &d);
            let mut inv = Inverse::new(&pal);
            let bpp = d.fmt.bytes();
            let fmt = d.fmt;
            let (w, h, pitch) = (d.w as usize, d.h as usize, d.pitch);
            let px = bits as *const u32;
            for y in 0..h {
                for x in 0..w {
                    let c = *px.add(y * w + x) | 0xFF00_0000;
                    let v = fmt.encode_argb(c, &mut inv);
                    fmt.write(&mut d.mem[y * pitch + x * bpp..], v);
                }
            }
        }
        api!("gdi32", "SelectObject", fn(usize, usize) -> usize, dc, old);
        api!("gdi32", "DeleteObject", fn(usize) -> i32, hbm);
        api!("gdi32", "DeleteDC", fn(usize) -> i32, dc);
        d.dc = None;
    }
    after_write(s);
    DD_OK
}

unsafe extern "win64" fn s_get_palette(this: usize, out: usize) -> i32 {
    let p = lock(&surf_of(this).data).palette;
    if p == 0 {
        put_ptr(out, 0);
        return DDERR_NOPALETTEATTACHED;
    }
    let p = &*(p as *const Palette);
    p.refs.fetch_add(1, Ordering::AcqRel);
    put_ptr(out, face_addr(&p.face));
    DD_OK
}

unsafe extern "win64" fn s_get_pixel_format(this: usize, pf: usize) -> i32 {
    if pf == 0 {
        return DDERR_INVALIDPARAMS;
    }
    lock(&surf_of(this).data).fmt.write_ddpf(pf);
    DD_OK
}

unsafe extern "win64" fn s_get_surface_desc(this: usize, desc: usize) -> i32 {
    if desc == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let v2 = is_desc2(desc, face(this).ver >= 4);
    write_desc(desc, &lock(&surf_of(this).data), v2, 0);
    DD_OK
}

unsafe extern "win64" fn s_lock(
    this: usize,
    rect: usize,
    desc: usize,
    _flags: u32,
    _event: usize,
) -> i32 {
    if desc == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let s = surf_of(this);
    let mut d = lock(&s.data);
    let full = bounds(&d);
    let r = Rect::read(rect).unwrap_or(full);
    if r.intersect(&full) != r || r.empty() {
        return dderr(150); // DDERR_INVALIDRECT
    }
    d.locks += 1;
    let base = d.mem.as_ptr() as usize;
    let p = base + r.t as usize * d.pitch + r.l as usize * d.fmt.bytes();
    let v2 = is_desc2(desc, face(this).ver >= 4);
    write_desc(desc, &d, v2, p);
    DD_OK
}

unsafe extern "win64" fn s_unlock(this: usize, _p: usize) -> i32 {
    let s = surf_of(this);
    {
        let mut d = lock(&s.data);
        if d.locks == 0 {
            return DDERR_NOTLOCKED;
        }
        d.locks -= 1;
    }
    after_write(s);
    DD_OK
}

unsafe extern "win64" fn s_set_clipper(this: usize, clip: usize) -> i32 {
    let s = surf_of(this);
    let new = if clip == 0 {
        0
    } else {
        let c = clip_of(clip);
        c.refs.fetch_add(1, Ordering::AcqRel);
        c as *const Clipper as usize
    };
    let old = std::mem::replace(&mut lock(&s.data).clipper, new);
    if old != 0 {
        clipper_release(face_addr(&(*(old as *const Clipper)).face));
    }
    DD_OK
}

unsafe extern "win64" fn s_set_palette(this: usize, pal: usize) -> i32 {
    let s = surf_of(this);
    let new = if pal == 0 {
        0
    } else {
        let p = pal_of(pal);
        p.refs.fetch_add(1, Ordering::AcqRel);
        p as *const Palette as usize
    };
    let (old, front, p8) = {
        let mut d = lock(&s.data);
        if d.fmt != Fmt::P8
            && new != 0
            && d.caps & DDSCAPS_PALETTE == 0
            && d.caps & DDSCAPS_PRIMARYSURFACE == 0
        {
            // Les surfaces non palettisées acceptent quand même une palette
            // (sans effet), comme beaucoup de pilotes.
        }
        (
            std::mem::replace(&mut d.palette, new),
            is_front(&d),
            d.fmt == Fmt::P8,
        )
    };
    if old != 0 {
        palette_release(face_addr(&(*(old as *const Palette)).face));
    }
    if front && p8 {
        present(s);
    }
    let _ = DDERR_NOTPALETTIZED;
    DD_OK
}

unsafe extern "win64" fn s_get_dd_interface(this: usize, out: usize) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let s = surf_of(this);
    let dd = &*(s.dd as *const DirectDraw);
    dd.refs.fetch_add(1, Ordering::AcqRel);
    let i = match face(this).ver {
        7 => dd_index(7),
        4 => dd_index(4),
        _ => 0,
    };
    put_ptr(out, face_addr(&dd.faces[i]));
    DD_OK
}

unsafe extern "win64" fn s_set_private(
    this: usize,
    g: usize,
    data: usize,
    size: u32,
    flags: u32,
) -> i32 {
    let Some(g) = read_guid(g) else {
        return DDERR_INVALIDPARAMS;
    };
    // DDSPD_IUNKNOWNPOINTER (1) : interface gardée par sa valeur.
    let bytes = if flags & 1 != 0 {
        data.to_le_bytes()[..super::legacy_com::ptr_size()].to_vec()
    } else if data != 0 {
        std::slice::from_raw_parts(data as *const u8, size as usize).to_vec()
    } else {
        Vec::new()
    };
    lock(&surf_of(this).data).private.insert(g, bytes);
    DD_OK
}

unsafe extern "win64" fn s_get_private(this: usize, g: usize, data: usize, size: usize) -> i32 {
    let Some(g) = read_guid(g) else {
        return DDERR_INVALIDPARAMS;
    };
    if size == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let d = lock(&surf_of(this).data);
    let Some(v) = d.private.get(&g) else {
        return DDERR_NOTFOUND;
    };
    let cap = rd32(size) as usize;
    wr32(size, v.len() as u32);
    if data == 0 || cap < v.len() {
        return dderr(600); // DDERR_MOREDATA
    }
    std::ptr::copy_nonoverlapping(v.as_ptr(), data as *mut u8, v.len());
    DD_OK
}

unsafe extern "win64" fn s_free_private(this: usize, g: usize) -> i32 {
    let Some(g) = read_guid(g) else {
        return DDERR_INVALIDPARAMS;
    };
    match lock(&surf_of(this).data).private.remove(&g) {
        Some(_) => DD_OK,
        None => DDERR_NOTFOUND,
    }
}

unsafe extern "win64" fn s_get_unique(this: usize, out: usize) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    wr32(out, lock(&surf_of(this).data).unique);
    DD_OK
}

unsafe extern "win64" fn s_change_unique(this: usize) -> i32 {
    let mut d = lock(&surf_of(this).data);
    d.unique = d.unique.wrapping_add(1);
    DD_OK
}

unsafe extern "win64" fn s_set_priority(this: usize, v: u32) -> i32 {
    lock(&surf_of(this).data).priority = v;
    DD_OK
}
unsafe extern "win64" fn s_get_priority(this: usize, out: usize) -> i32 {
    wr32(out, lock(&surf_of(this).data).priority);
    DD_OK
}
unsafe extern "win64" fn s_set_lod(this: usize, v: u32) -> i32 {
    lock(&surf_of(this).data).lod = v;
    DD_OK
}
unsafe extern "win64" fn s_get_lod(this: usize, out: usize) -> i32 {
    wr32(out, lock(&surf_of(this).data).lod);
    DD_OK
}

unsafe extern "win64" fn s_get_overlay_position(_this: usize, _x: usize, _y: usize) -> i32 {
    dderr(580) // DDERR_NOTAOVERLAYSURFACE
}

unsafe extern "win64" fn s_set_surface_desc(this: usize, desc: usize, _flags: u32) -> i32 {
    if desc == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let din = read_desc(desc);
    let s = surf_of(this);
    let mut d = lock(&s.data);
    if din.w != 0 && din.h != 0 {
        let fmt = match din.fmt {
            Some(Some(f)) => f,
            _ => d.fmt,
        };
        resize(&mut d, din.w, din.h, fmt);
    }
    DD_OK
}

fn surface_vtable(ver: u8) -> usize {
    let a = |f: usize, n: u8| (f, n);
    let mut m = vec![
        a(s_qi as *const () as usize, 2),
        a(s_addref as *const () as usize, 0),
        a(s_release as *const () as usize, 0),
        a(s_add_attached as *const () as usize, 1),
        a(ok1 as *const () as usize, 1), // AddOverlayDirtyRect
        a(s_blt as *const () as usize, 5),
        a(unsupported3 as *const () as usize, 3), // BltBatch
        a(s_blt_fast as *const () as usize, 5),
        a(s_delete_attached as *const () as usize, 2),
        a(s_enum_attached as *const () as usize, 2),
        a(unsupported3 as *const () as usize, 3), // EnumOverlayZOrders
        a(s_flip as *const () as usize, 2),
        a(s_get_attached as *const () as usize, 2),
        a(ok1 as *const () as usize, 1), // GetBltStatus
        a(s_get_caps as *const () as usize, 1),
        a(s_get_clipper as *const () as usize, 1),
        a(s_get_color_key as *const () as usize, 2),
        a(s_get_dc as *const () as usize, 1),
        a(ok1 as *const () as usize, 1), // GetFlipStatus
        a(s_get_overlay_position as *const () as usize, 2),
        a(s_get_palette as *const () as usize, 1),
        a(s_get_pixel_format as *const () as usize, 1),
        a(s_get_surface_desc as *const () as usize, 1),
        a(already_init2 as *const () as usize, 2), // Initialize
        a(ok0 as *const () as usize, 0),           // IsLost
        a(s_lock as *const () as usize, 4),
        a(s_release_dc as *const () as usize, 1),
        a(ok0 as *const () as usize, 0), // Restore
        a(s_set_clipper as *const () as usize, 1),
        a(s_set_color_key as *const () as usize, 2),
        a(unsupported2 as *const () as usize, 2), // SetOverlayPosition
        a(s_set_palette as *const () as usize, 1),
        a(s_unlock as *const () as usize, 1),
        a(unsupported5 as *const () as usize, 5), // UpdateOverlay
        a(unsupported1 as *const () as usize, 1), // UpdateOverlayDisplay
        a(unsupported2 as *const () as usize, 2), // UpdateOverlayZOrder
    ];
    if ver >= 2 {
        m.push(a(s_get_dd_interface as *const () as usize, 1));
        m.push(a(ok1 as *const () as usize, 1)); // PageLock
        m.push(a(ok1 as *const () as usize, 1)); // PageUnlock
    }
    if ver >= 3 {
        m.push(a(s_set_surface_desc as *const () as usize, 2));
    }
    if ver >= 4 {
        m.push(a(s_set_private as *const () as usize, 4));
        m.push(a(s_get_private as *const () as usize, 3));
        m.push(a(s_free_private as *const () as usize, 1));
        m.push(a(s_get_unique as *const () as usize, 1));
        m.push(a(s_change_unique as *const () as usize, 0));
    }
    if ver >= 7 {
        m.push(a(s_set_priority as *const () as usize, 1));
        m.push(a(s_get_priority as *const () as usize, 1));
        m.push(a(s_set_lod as *const () as usize, 1));
        m.push(a(s_get_lod as *const () as usize, 1));
    }
    let name: &'static str = match ver {
        1 => "IDirectDrawSurface",
        2 => "IDirectDrawSurface2",
        3 => "IDirectDrawSurface3",
        4 => "IDirectDrawSurface4",
        _ => "IDirectDrawSurface7",
    };
    vtable(name, &m)
}

// ---------------------------------------------------------------------
// IDirectDrawPalette
// ---------------------------------------------------------------------

unsafe extern "win64" fn p_qi(this: usize, iid: usize, out: usize) -> i32 {
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_PALETTE => {
            pal_of(this).refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, this);
            DD_OK
        }
        _ => {
            put_ptr(out, 0);
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn p_addref(this: usize) -> u32 {
    pal_of(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn palette_release(this: usize) -> u32 {
    let p = pal_of(this);
    let n = p.refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
    if n == 0 {
        let dd = &*(p.dd as *const DirectDraw);
        drop(Box::from_raw(p as *const Palette as *mut Palette));
        dd_release_obj(dd);
    }
    n
}
unsafe extern "win64" fn p_get_caps(this: usize, out: usize) -> i32 {
    wr32(out, pal_of(this).caps);
    DD_OK
}
unsafe extern "win64" fn p_get_entries(
    this: usize,
    _flags: u32,
    base: u32,
    n: u32,
    out: usize,
) -> i32 {
    if out == 0 || base + n > 256 {
        return DDERR_INVALIDPARAMS;
    }
    let e = lock(&pal_of(this).entries);
    for i in 0..n as usize {
        let c = e[base as usize + i];
        wr32(
            out + 4 * i,
            (c >> 16) & 0xFF | (c & 0xFF00) | (c & 0xFF) << 16,
        );
    }
    DD_OK
}
unsafe extern "win64" fn p_set_entries(
    this: usize,
    _flags: u32,
    base: u32,
    n: u32,
    src: usize,
) -> i32 {
    if src == 0 || base + n > 256 {
        return DDERR_INVALIDPARAMS;
    }
    let p = pal_of(this);
    {
        let mut e = lock(&p.entries);
        for i in 0..n as usize {
            let v = rd32(src + 4 * i);
            e[base as usize + i] = (v & 0xFF) << 16 | (v & 0xFF00) | (v >> 16) & 0xFF;
        }
    }
    // Palette de l'écran : nouvelle image tout de suite (fondus au noir).
    let dd = &*(p.dd as *const DirectDraw);
    let prim = lock(&dd.st).primary;
    if prim != 0 {
        let s = &*(prim as *const Surface);
        let uses = {
            let d = lock(&s.data);
            d.palette == p as *const Palette as usize && d.fmt == Fmt::P8
        };
        if uses {
            present(s);
        }
    }
    DD_OK
}

fn palette_vtable() -> usize {
    vtable(
        "IDirectDrawPalette",
        &[
            (p_qi as *const () as usize, 2),
            (p_addref as *const () as usize, 0),
            (palette_release as *const () as usize, 0),
            (p_get_caps as *const () as usize, 1),
            (p_get_entries as *const () as usize, 4),
            (unsupported3 as *const () as usize, 3), // Initialize
            (p_set_entries as *const () as usize, 4),
        ],
    )
}

// ---------------------------------------------------------------------
// IDirectDrawClipper
// ---------------------------------------------------------------------

unsafe extern "win64" fn c_qi(this: usize, iid: usize, out: usize) -> i32 {
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_CLIPPER => {
            clip_of(this).refs.fetch_add(1, Ordering::AcqRel);
            put_ptr(out, this);
            DD_OK
        }
        _ => {
            put_ptr(out, 0);
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn c_addref(this: usize) -> u32 {
    clip_of(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn clipper_release(this: usize) -> u32 {
    let c = clip_of(this);
    let n = c.refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
    if n == 0 {
        drop(Box::from_raw(c as *const Clipper as *mut Clipper));
    }
    n
}

/// GetClipList(rect, RGNDATA*, taille*) : RGNDATAHEADER (32 octets) puis
/// les rectangles.
unsafe extern "win64" fn c_get_clip_list(
    this: usize,
    rect: usize,
    data: usize,
    size: usize,
) -> i32 {
    if size == 0 {
        return DDERR_INVALIDPARAMS;
    }
    let c = clip_of(this);
    let mut rects = match lock(&c.list).clone() {
        Some(l) => l,
        None => {
            let hwnd = c.hwnd.load(Ordering::Relaxed);
            match client_rect(hwnd) {
                Some((r, x, y)) if hwnd != 0 => vec![Rect {
                    l: x,
                    t: y,
                    r: x + r.w(),
                    b: y + r.h(),
                }],
                _ => {
                    let (w, h) = desktop();
                    vec![Rect {
                        l: 0,
                        t: 0,
                        r: w as i32,
                        b: h as i32,
                    }]
                }
            }
        }
    };
    if let Some(lim) = Rect::read(rect) {
        rects = rects
            .into_iter()
            .map(|r| r.intersect(&lim))
            .filter(|r| !r.empty())
            .collect();
    }
    let need = 32 + 16 * rects.len();
    let cap = rd32(size) as usize;
    wr32(size, need as u32);
    if data == 0 {
        return DD_OK;
    }
    if cap < need {
        return dderr(170); // DDERR_REGIONTOOSMALL
    }
    let mut bound = rects.first().copied().unwrap_or_default();
    for r in &rects {
        bound.l = bound.l.min(r.l);
        bound.t = bound.t.min(r.t);
        bound.r = bound.r.max(r.r);
        bound.b = bound.b.max(r.b);
    }
    wr32(data, 32);
    wr32(data + 4, 1); // RDH_RECTANGLES
    wr32(data + 8, rects.len() as u32);
    wr32(data + 12, (16 * rects.len()) as u32);
    for (i, r) in std::iter::once(bound).chain(rects).enumerate() {
        let at = data + 16 + 16 * i;
        wr32(at, r.l as u32);
        wr32(at + 4, r.t as u32);
        wr32(at + 8, r.r as u32);
        wr32(at + 12, r.b as u32);
    }
    DD_OK
}

unsafe extern "win64" fn c_get_hwnd(this: usize, out: usize) -> i32 {
    put_ptr(out, clip_of(this).hwnd.load(Ordering::Relaxed));
    DD_OK
}

unsafe extern "win64" fn c_is_changed(_this: usize, out: usize) -> i32 {
    if out != 0 {
        wr32(out, 0);
    }
    DD_OK
}

unsafe extern "win64" fn c_set_clip_list(this: usize, data: usize, _flags: u32) -> i32 {
    let c = clip_of(this);
    if data == 0 {
        *lock(&c.list) = None;
        return DD_OK;
    }
    let n = rd32(data + 8) as usize;
    let rects = (0..n.min(4096))
        .filter_map(|i| Rect::read(data + 32 + 16 * i))
        .collect();
    *lock(&c.list) = Some(rects);
    DD_OK
}

unsafe extern "win64" fn c_set_hwnd(this: usize, _flags: u32, hwnd: usize) -> i32 {
    clip_of(this).hwnd.store(hwnd, Ordering::Relaxed);
    DD_OK
}

fn clipper_vtable() -> usize {
    vtable(
        "IDirectDrawClipper",
        &[
            (c_qi as *const () as usize, 2),
            (c_addref as *const () as usize, 0),
            (clipper_release as *const () as usize, 0),
            (c_get_clip_list as *const () as usize, 3),
            (c_get_hwnd as *const () as usize, 1),
            (ok2 as *const () as usize, 2), // Initialize
            (c_is_changed as *const () as usize, 1),
            (c_set_clip_list as *const () as usize, 2),
            (c_set_hwnd as *const () as usize, 2),
        ],
    )
}

// ---------------------------------------------------------------------
// Fonctions exportées
// ---------------------------------------------------------------------

/// DirectDrawCreate(lpGUID, lplpDD, pUnkOuter) : rend un IDirectDraw.
pub unsafe extern "win64" fn direct_draw_create(_guid: usize, out: usize, outer: usize) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    if outer != 0 {
        return dderr(570); // CLASS_E_NOAGGREGATION n'existe pas en ddraw
    }
    let dd = new_ddraw();
    put_ptr(out, face_addr(&dd.faces[0]));
    DD_OK
}

/// DirectDrawCreateEx(lpGUID, lplpDD, iid, pUnkOuter) : IID_IDirectDraw7.
pub unsafe extern "win64" fn direct_draw_create_ex(
    _guid: usize,
    out: usize,
    iid: usize,
    outer: usize,
) -> i32 {
    if out == 0 || outer != 0 {
        return DDERR_INVALIDPARAMS;
    }
    let Some(g) = read_guid(iid) else {
        return DDERR_INVALIDPARAMS;
    };
    let Some(i) = IID_DD.iter().position(|x| *x == g) else {
        return DDERR_INVALIDPARAMS;
    };
    let dd = new_ddraw();
    put_ptr(out, face_addr(&dd.faces[i]));
    DD_OK
}

pub unsafe extern "win64" fn direct_draw_create_clipper(
    _flags: u32,
    out: usize,
    _outer: usize,
) -> i32 {
    if out == 0 {
        return DDERR_INVALIDPARAMS;
    }
    put_ptr(out, new_clipper());
    DD_OK
}

/// Nom du pilote d'affichage, dans la langue de Paramètres.
fn driver_description() -> &'static str {
    match crate::intl::locale::current().lang.iso639 {
        "fr" => "Pilote d'affichage principal",
        "de" => "Primärer Anzeigetreiber",
        "es" => "Controlador de pantalla principal",
        _ => "Primary Display Driver",
    }
}

unsafe fn enumerate(cb: usize, ctx: usize, wide: bool, ex: bool) -> i32 {
    if cb == 0 {
        return DDERR_INVALIDPARAMS;
    }
    // Chaînes gardées en mémoire (sous 4 Go pour un programme 32 bits).
    let (desc, name): (Vec<u8>, Vec<u8>) = if wide {
        let w = |s: &str| {
            s.encode_utf16()
                .chain([0])
                .flat_map(|c| c.to_le_bytes())
                .collect::<Vec<u8>>()
        };
        (w(driver_description()), w("display"))
    } else {
        let mut d = crate::dlls::encode_ansi(driver_description());
        d.push(0);
        (d, b"display\0".to_vec())
    };
    let args: Vec<usize> = if ex {
        vec![0, desc.as_ptr() as usize, name.as_ptr() as usize, ctx, 0]
    } else {
        vec![0, desc.as_ptr() as usize, name.as_ptr() as usize, ctx]
    };
    call_back(cb, &args);
    DD_OK
}

pub unsafe extern "win64" fn direct_draw_enumerate_a(cb: usize, ctx: usize) -> i32 {
    enumerate(cb, ctx, false, false)
}
pub unsafe extern "win64" fn direct_draw_enumerate_w(cb: usize, ctx: usize) -> i32 {
    enumerate(cb, ctx, true, false)
}
pub unsafe extern "win64" fn direct_draw_enumerate_ex_a(cb: usize, ctx: usize, _flags: u32) -> i32 {
    enumerate(cb, ctx, false, true)
}
pub unsafe extern "win64" fn direct_draw_enumerate_ex_w(cb: usize, ctx: usize, _flags: u32) -> i32 {
    enumerate(cb, ctx, true, true)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DirectDrawCreate" => fn direct_draw_create,
        "DirectDrawCreateEx" => fn direct_draw_create_ex,
        "DirectDrawCreateClipper" => fn direct_draw_create_clipper,
        "DirectDrawEnumerateA" => fn direct_draw_enumerate_a,
        "DirectDrawEnumerateW" => fn direct_draw_enumerate_w,
        "DirectDrawEnumerateExA" => fn direct_draw_enumerate_ex_a,
        "DirectDrawEnumerateExW" => fn direct_draw_enumerate_ex_w,
    }
}

// Empêche l'avertissement « jamais lu » des champs gardés pour plus tard.
#[allow(dead_code)]
fn _unused() -> (i32, i32, u32) {
    (
        DDERR_INVALIDOBJECT,
        DDERR_GENERIC,
        DDSCAPS_COMPLEX | DDSCAPS_OFFSCREENPLAIN,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_de_formats() {
        let pal = default_palette();
        assert_eq!(Fmt::Rgb565.to_argb(0xF800, &pal), 0xFFFF_0000);
        assert_eq!(Fmt::Rgb565.to_argb(0x07E0, &pal), 0xFF00_FF00);
        assert_eq!(Fmt::Rgb555.to_argb(0x001F, &pal), 0xFF00_00FF);
        let mut inv = Inverse::new(&pal);
        assert_eq!(Fmt::Rgb565.encode_argb(0xFFFF_FFFF, &mut inv), 0xFFFF);
        assert_eq!(Fmt::P8.encode_argb(0x0080_8080, &mut inv), 0x80);
    }

    #[test]
    fn decalages_ddsurfacedesc() {
        // Programme 64 bits (tests) : lpSurface à 40, ddpfPixelFormat à 80.
        assert_eq!(doff(36), 40);
        assert_eq!(doff(72), 80);
        assert_eq!(doff(104), 112);
        assert_eq!(desc_size(true), 136);
        assert_eq!(fx_off(80), 104);
    }

    fn surf(w: u32, h: u32, fmt: Fmt) -> SurfData {
        SurfData {
            w,
            h,
            fmt,
            pitch: pitch_for(w, fmt),
            caps: 0,
            mem: vec![0; pitch_for(w, fmt) * h as usize],
            chain: Vec::new(),
            back: 0,
            attached: Vec::new(),
            palette: 0,
            clipper: 0,
            ck_src: None,
            ck_dest: None,
            locks: 0,
            dc: None,
            private: HashMap::new(),
            unique: 0,
            priority: 0,
            lod: 0,
        }
    }

    #[test]
    fn blt_cle_de_couleur_et_etirement() {
        let mut d = surf(4, 4, Fmt::Xrgb8888);
        let b = bounds(&d);
        fill(&mut d, b, 0x11);
        // Source 2x1 : un pixel transparent (0), un rouge.
        let src = SrcCopy {
            fmt: Fmt::Xrgb8888,
            w: 2,
            h: 1,
            pixels: [0u32, 0xFF0000]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
            pal: default_palette(),
            key: Some((0, 0)),
        };
        let o = BltOpts {
            src_key: src.key,
            dest_key: None,
            mirror_lr: false,
            mirror_ud: false,
        };
        let dst = Rect {
            l: 0,
            t: 0,
            r: 4,
            b: 2,
        };
        draw(&mut d, dst, b, &src, &o);
        let px =
            |d: &SurfData, x: usize, y: usize| Fmt::Xrgb8888.read(&d.mem[y * d.pitch + x * 4..]);
        assert_eq!(px(&d, 0, 0), 0x11, "clé : pixel gardé");
        assert_eq!(px(&d, 1, 1), 0x11);
        assert_eq!(px(&d, 2, 0), 0xFF0000, "étiré à 2 pixels");
        assert_eq!(px(&d, 3, 1), 0xFF0000);
        assert_eq!(px(&d, 3, 2), 0x11, "hors du rectangle");
    }

    #[test]
    fn blt_8_bits_vers_16_bits() {
        let mut d = surf(2, 1, Fmt::Rgb565);
        let mut pal = default_palette();
        pal[5] = 0xFF0000;
        let src = SrcCopy {
            fmt: Fmt::P8,
            w: 2,
            h: 1,
            pixels: vec![5, 0],
            pal,
            key: None,
        };
        let o = BltOpts {
            src_key: None,
            dest_key: None,
            mirror_lr: true,
            mirror_ud: false,
        };
        let b = bounds(&d);
        draw(&mut d, b, b, &src, &o);
        assert_eq!(Fmt::Rgb565.read(&d.mem[0..]), 0);
        assert_eq!(
            Fmt::Rgb565.read(&d.mem[2..]),
            0xF800,
            "miroir gauche-droite"
        );
    }
}
