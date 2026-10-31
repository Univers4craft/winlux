//! Imagerie native Winlux : décodeurs d'images écrits de zéro (aucune
//! bibliothèque externe), rendu avec zoom/rotation/miroir, aperçus 3D
//! tournants, métadonnées et vignettes avec cache disque.
//!
//! Garde-fou : un format n'est annoncé comme pris en charge (`FORMATS`,
//! `WinluxImageSupported`) que si un décodeur existe réellement ici.

pub mod api;
mod bmp;
mod dds;
mod exr;
mod gif;
mod inflate;
mod jpeg;
mod meta;
pub mod model3d;
mod png;
mod psd;
mod raster;
mod simple;
pub mod svg;
mod tiff;
mod vp8_tables;
pub mod webp;
mod xcf;

use raster::{Canvas, Mat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Format reconnu mais sans décodeur (HEIC, AVIF, JPEG XL...).
    Unsupported(&'static str),
    /// Variante d'un format pris en charge qui n'est pas gérée.
    Variant(&'static str),
    Corrupt,
    TooLarge,
    Io,
}

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Error::Unsupported(_) => 1,
            Error::Variant(_) => 2,
            Error::Corrupt => 3,
            Error::TooLarge => 4,
            Error::Io => 5,
        }
    }
    pub fn detail(&self) -> &'static str {
        match self {
            Error::Unsupported(s) | Error::Variant(s) => s,
            _ => "",
        }
    }
}

pub(crate) fn be16(d: &[u8], p: usize) -> u16 {
    match d.get(p..p + 2) {
        Some(b) => u16::from_be_bytes([b[0], b[1]]),
        None => 0,
    }
}
pub(crate) fn be32(d: &[u8], p: usize) -> u32 {
    match d.get(p..p + 4) {
        Some(b) => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        None => 0,
    }
}
pub(crate) fn le16(d: &[u8], p: usize) -> u16 {
    match d.get(p..p + 2) {
        Some(b) => u16::from_le_bytes([b[0], b[1]]),
        None => 0,
    }
}
pub(crate) fn le32(d: &[u8], p: usize) -> u32 {
    match d.get(p..p + 4) {
        Some(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        None => 0,
    }
}

/// Limite de sécurité : 150 mégapixels par image.
pub(crate) fn check_size(w: u32, h: u32) -> Result<(), Error> {
    if w == 0 || h == 0 {
        return Err(Error::Corrupt);
    }
    if w as u64 * h as u64 > 150_000_000 || w > 65_535 || h > 65_535 {
        return Err(Error::TooLarge);
    }
    Ok(())
}

/// Linéaire -> sRGB.
pub(crate) fn srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Tone mapping (Reinhard étendu, blanc à 4) puis gamma sRGB.
pub(crate) fn tonemap(v: f32) -> f32 {
    if !v.is_finite() {
        return if v > 0.0 { 1.0 } else { 0.0 };
    }
    let x = v.max(0.0);
    srgb(x * (1.0 + x / 16.0) / (1.0 + x))
}

/// Composition « source sur destination » (alpha non prémultiplié).
pub(crate) fn over(s: [u8; 4], d: [u8; 4]) -> [u8; 4] {
    let sa = s[3] as u32;
    if sa == 255 {
        return s;
    }
    if sa == 0 {
        return d;
    }
    let da = d[3] as u32 * (255 - sa) / 255;
    let oa = sa + da;
    if oa == 0 {
        return [0; 4];
    }
    let c = |i: usize| ((s[i] as u32 * sa + d[i] as u32 * da) / oa) as u8;
    [c(0), c(1), c(2), oa as u8]
}

pub(crate) fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut n = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((acc >> n) as u8);
        }
    }
    Some(out)
}

/// Image RGBA 8 bits, alpha non prémultiplié.
#[derive(Clone, Debug)]
pub struct Raster {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Raster {
    pub fn new(w: u32, h: u32) -> Raster {
        Raster {
            w,
            h,
            px: vec![0; w as usize * h as usize * 4],
        }
    }
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
    #[inline]
    pub fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        self.px[i..i + 4].copy_from_slice(&c);
    }
    /// Échantillon bilinéaire (0..1, alpha non prémultiplié).
    pub fn bilinear(&self, u: f32, v: f32) -> [f32; 4] {
        let p = self.sample_premul(u, v);
        if p[3] <= 0.0 {
            return [0.0; 4];
        }
        [p[0] / p[3], p[1] / p[3], p[2] / p[3], p[3]]
    }
    /// Échantillon bilinéaire prémultiplié (0..1).
    fn sample_premul(&self, u: f32, v: f32) -> [f32; 4] {
        let x = (u - 0.5).max(0.0);
        let y = (v - 0.5).max(0.0);
        let x0 = (x as u32).min(self.w - 1);
        let y0 = (y as u32).min(self.h - 1);
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let mut out = [0f32; 4];
        for (xx, yy, wgt) in [
            (x0, y0, (1.0 - fx) * (1.0 - fy)),
            (x1, y0, fx * (1.0 - fy)),
            (x0, y1, (1.0 - fx) * fy),
            (x1, y1, fx * fy),
        ] {
            let c = self.get(xx, yy);
            let a = c[3] as f32 / 255.0;
            out[0] += c[0] as f32 / 255.0 * a * wgt;
            out[1] += c[1] as f32 / 255.0 * a * wgt;
            out[2] += c[2] as f32 / 255.0 * a * wgt;
            out[3] += a * wgt;
        }
        out
    }
    /// Réduction 2:1 (moyenne prémultipliée) pour la pyramide de zoom.
    fn half(&self) -> Raster {
        let w = (self.w / 2).max(1);
        let h = (self.h / 2).max(1);
        let mut r = Raster::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0u32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let c = self.get((x * 2 + dx).min(self.w - 1), (y * 2 + dy).min(self.h - 1));
                    let a = c[3] as u32;
                    acc[0] += c[0] as u32 * a;
                    acc[1] += c[1] as u32 * a;
                    acc[2] += c[2] as u32 * a;
                    acc[3] += a;
                }
                let c = match acc[3] {
                    0 => [0, 0, 0, 0],
                    n => [
                        (acc[0] / n) as u8,
                        (acc[1] / n) as u8,
                        (acc[2] / n) as u8,
                        (n / 4) as u8,
                    ],
                };
                r.set(x, y, c);
            }
        }
        r
    }
    fn transformed(&self, o: u8) -> Raster {
        let (w, h) = (self.w, self.h);
        let swap = o >= 5;
        let mut r = if swap {
            Raster::new(h, w)
        } else {
            Raster::new(w, h)
        };
        for y in 0..h {
            for x in 0..w {
                let (nx, ny) = match o {
                    2 => (w - 1 - x, y),
                    3 => (w - 1 - x, h - 1 - y),
                    4 => (x, h - 1 - y),
                    5 => (y, x),
                    6 => (h - 1 - y, x),
                    7 => (h - 1 - y, w - 1 - x),
                    8 => (y, w - 1 - x),
                    _ => (x, y),
                };
                r.set(nx, ny, self.get(x, y));
            }
        }
        r
    }
}

#[derive(Clone, Debug)]
pub enum Content {
    Raster(Raster),
    Vector(svg::Svg),
    Model(model3d::Mesh),
}

#[derive(Clone, Debug)]
pub struct Page {
    pub name: String,
    pub content: Content,
    pub delay_ms: u32,
}

impl Page {
    pub fn raster(name: String, r: Raster) -> Page {
        Page {
            name,
            content: Content::Raster(r),
            delay_ms: 0,
        }
    }
    pub fn frame(r: Raster, delay: u32) -> Page {
        Page {
            name: String::new(),
            content: Content::Raster(r),
            delay_ms: delay,
        }
    }
    pub fn into_raster(self) -> Raster {
        match self.content {
            Content::Raster(r) => r,
            _ => Raster::new(1, 1),
        }
    }
    /// Taille logique (pixels pour une image, unités pour un SVG, carré
    /// virtuel pour un modèle 3D).
    pub fn size(&self) -> (u32, u32) {
        match &self.content {
            Content::Raster(r) => (r.w, r.h),
            Content::Vector(s) => (s.w.ceil() as u32, s.h.ceil() as u32),
            Content::Model(_) => (1024, 1024),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Image {
    pub format: &'static str,
    pub pages: Vec<Page>,
    pub animated: bool,
    pub layered: bool,
    /// Photo RAW d'appareil (comparaison avant/après proposée).
    pub raw: bool,
    /// L'image affichée est l'aperçu intégré au fichier (RAW, AI, EPS).
    pub embedded_preview: bool,
    pub hdr: bool,
    pub orientation: u8,
    pub meta: Vec<(String, String)>,
}

impl Image {
    pub fn new(format: &'static str) -> Image {
        Image {
            format,
            pages: Vec::new(),
            animated: false,
            layered: false,
            raw: false,
            embedded_preview: false,
            hdr: false,
            orientation: 0,
            meta: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Common,
    Modern,
    Raw,
    Creative,
    Special,
    Model,
}

pub struct Format {
    pub name: &'static str,
    pub exts: &'static [&'static str],
    #[allow(dead_code)]
    pub family: Family,
    /// `false` : reconnu mais pas de décodeur, donc jamais annoncé.
    pub supported: bool,
}

pub const FORMATS: &[Format] = &[
    Format {
        name: "PNG / APNG",
        exts: &["png", "apng"],
        family: Family::Common,
        supported: true,
    },
    Format {
        name: "JPEG",
        exts: &["jpg", "jpeg", "jpe", "jfif", "pjpeg", "pjp"],
        family: Family::Common,
        supported: true,
    },
    Format {
        name: "GIF",
        exts: &["gif"],
        family: Family::Common,
        supported: true,
    },
    Format {
        name: "BMP / DIB",
        exts: &["bmp", "dib", "rle"],
        family: Family::Common,
        supported: true,
    },
    Format {
        name: "TIFF",
        exts: &["tif", "tiff"],
        family: Family::Common,
        supported: true,
    },
    Format {
        name: "ICO / CUR",
        exts: &["ico", "cur"],
        family: Family::Common,
        supported: true,
    },
    Format {
        name: "WebP",
        exts: &["webp"],
        family: Family::Common,
        supported: true,
    },
    Format {
        name: "QOI",
        exts: &["qoi"],
        family: Family::Modern,
        supported: true,
    },
    Format {
        name: "HEIC / HEIF",
        exts: &["heic", "heif", "hif"],
        family: Family::Modern,
        supported: false,
    },
    Format {
        name: "AVIF",
        exts: &["avif"],
        family: Family::Modern,
        supported: false,
    },
    Format {
        name: "JPEG XL",
        exts: &["jxl"],
        family: Family::Modern,
        supported: false,
    },
    Format {
        name: "RAW (aperçu)",
        exts: &[
            "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "rw2", "raf",
            "pef", "srw", "x3f", "3fr", "fff", "iiq", "mos", "kdc", "dcr", "mrw", "erf", "rwl",
            "mef",
        ],
        family: Family::Raw,
        supported: true,
    },
    Format {
        name: "Photoshop",
        exts: &["psd", "psb"],
        family: Family::Creative,
        supported: true,
    },
    Format {
        name: "GIMP",
        exts: &["xcf"],
        family: Family::Creative,
        supported: true,
    },
    Format {
        name: "SVG",
        exts: &["svg", "svgz"],
        family: Family::Creative,
        supported: true,
    },
    Format {
        name: "Illustrator (aperçu)",
        exts: &["ai"],
        family: Family::Creative,
        supported: true,
    },
    Format {
        name: "EPS (aperçu)",
        exts: &["eps", "epsi", "epsf"],
        family: Family::Creative,
        supported: true,
    },
    Format {
        name: "TGA",
        exts: &["tga", "icb", "vda", "vst"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "PCX",
        exts: &["pcx"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "Netpbm",
        exts: &["pbm", "pgm", "ppm", "pnm", "pam", "pfm"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "DDS",
        exts: &["dds"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "OpenEXR",
        exts: &["exr"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "Radiance HDR",
        exts: &["hdr", "rgbe", "pic"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "ICNS (macOS)",
        exts: &["icns"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "SGI",
        exts: &["sgi", "rgb", "rgba", "bw"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "Sun Raster",
        exts: &["ras", "sun"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "XBM / XPM",
        exts: &["xbm", "xpm"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "WBMP",
        exts: &["wbmp"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "farbfeld",
        exts: &["ff"],
        family: Family::Special,
        supported: true,
    },
    Format {
        name: "JPEG 2000",
        exts: &["jp2", "j2k", "jpf", "jpx", "jpm", "j2c"],
        family: Family::Special,
        supported: false,
    },
    Format {
        name: "PDF",
        exts: &["pdf"],
        family: Family::Special,
        supported: false,
    },
    Format {
        name: "Modèles 3D (aperçu)",
        exts: &["stl", "obj", "glb", "gltf", "fbx", "ply", "off"],
        family: Family::Model,
        supported: true,
    },
];

fn ext_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn format_for_ext(ext: &str) -> Option<&'static Format> {
    let ext = ext.to_ascii_lowercase();
    FORMATS.iter().find(|f| f.exts.contains(&ext.as_str()))
}

/// L'extension est-elle annoncée comme lisible par la visionneuse ?
pub fn supported_ext(ext: &str) -> bool {
    format_for_ext(ext).is_some_and(|f| f.supported)
}

const RAW_EXTS: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "rw2", "raf", "pef",
    "srw", "x3f", "3fr", "fff", "iiq", "mos", "kdc", "dcr", "mrw", "erf", "rwl", "mef",
];

/// Décode un fichier en mémoire ; `ext` sert d'indice quand la signature
/// ne suffit pas (TGA, PCX, formats texte, modèles 3D).
pub fn decode_bytes(d: &[u8], ext: &str) -> Result<Image, Error> {
    let ext = ext.to_ascii_lowercase();
    let starts = |m: &[u8]| d.starts_with(m);
    let mut img = if starts(b"\x89PNG\r\n\x1a\n") {
        png::decode(d)?
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        jpeg::decode(d)?
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        gif::decode(d)?
    } else if starts(b"BM") && d.len() > 26 && (le32(d, 14) == 12 || le32(d, 14) >= 40) {
        bmp::decode_bmp(d)?
    } else if starts(b"FUJIFILMCCD-RAW") || starts(b"IIRO") || starts(b"IIU\0") || starts(b"MMOR") {
        tiff::raw_preview(d, "RAW")?
    } else if (starts(b"II*\0") || starts(b"MM\0*")) && RAW_EXTS.contains(&ext.as_str()) {
        tiff::raw_preview(d, raw_name(&ext))?
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        match tiff::decode(d, "TIFF") {
            Ok(i) => i,
            // TIFF dont l'image principale est un capteur brut : aperçu.
            Err(Error::Variant("RAW CFA")) => tiff::raw_preview(d, "DNG")?,
            Err(e) => return Err(e),
        }
    } else if starts(b"II+\0") || starts(b"MM\0+") {
        return Err(Error::Variant("BigTIFF"));
    } else if d.len() > 12 && &d[4..8] == b"ftyp" {
        match &d[8..12] {
            b"crx " => tiff::raw_preview(d, "CR3")?,
            b"avif" | b"avis" => return Err(Error::Unsupported("AVIF")),
            b"jp2 " | b"jpx " | b"jpm " => return Err(Error::Unsupported("JPEG 2000")),
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" | b"heim" | b"heis" => {
                return Err(Error::Unsupported("HEIC"))
            }
            _ => return Err(Error::Unsupported("ISO BMFF")),
        }
    } else if starts(b"\0\0\0\x0cjP  ") || starts(&[0xFF, 0x4F, 0xFF, 0x51]) {
        return Err(Error::Unsupported("JPEG 2000"));
    } else if starts(&[0xFF, 0x0A]) || starts(b"\0\0\0\x0cJXL ") {
        return Err(Error::Unsupported("JPEG XL"));
    } else if starts(b"RIFF") && d.len() > 12 && &d[8..12] == b"WEBP" {
        webp::decode(d)?
    } else if starts(b"8BPS") {
        psd::decode(d)?
    } else if starts(b"gimp xcf") {
        xcf::decode(d)?
    } else if starts(b"DDS ") {
        dds::decode(d)?
    } else if starts(b"qoif") {
        simple::qoi(d)?
    } else if starts(b"farbfeld") {
        simple::farbfeld(d)?
    } else if starts(&[0x76, 0x2F, 0x31, 0x01]) {
        exr::decode(d)?
    } else if starts(b"#?RADIANCE") || starts(b"#?RGBE") {
        simple::hdr(d)?
    } else if starts(b"icns") {
        bmp::decode_icns(d)?
    } else if starts(&[0x01, 0xDA]) {
        simple::sgi(d)?
    } else if starts(&[0x59, 0xA6, 0x6A, 0x95]) {
        simple::sun(d)?
    } else if starts(b"%PDF") {
        if ext == "ai" {
            embedded_xmp_thumb(d, "Illustrator")?
        } else {
            return Err(Error::Unsupported("PDF"));
        }
    } else if starts(&[0xC5, 0xD0, 0xD3, 0xC6]) || starts(b"%!PS") {
        eps(d)?
    } else if starts(b"glTF") || starts(b"Kaydara FBX Binary") || model3d::is_model_ext(&ext) {
        model3d::decode(d, &ext)?
    } else if starts(&[0x1F, 0x8B]) {
        let raw = inflate::gunzip(d, 64 << 20).ok_or(Error::Corrupt)?;
        return decode_bytes(&raw, "svg");
    } else if d.len() > 1 && d[0] == b'P' && matches!(d[1], b'1'..=b'7' | b'F' | b'f') {
        simple::pnm(d)?
    } else if matches!(ext.as_str(), "tga" | "icb" | "vda" | "vst") {
        simple::tga(d)?
    } else if (starts(&[0, 0, 1, 0]) || starts(&[0, 0, 2, 0]))
        && (1..=256).contains(&le16(d, 4))
        && d.len() >= 22
        && (le32(d, 18) as usize) < d.len()
    {
        bmp::decode_ico(d)?
    } else if looks_svg(d) {
        let s = svg::parse(d).ok_or(Error::Corrupt)?;
        let mut img = Image::new("SVG");
        img.pages.push(Page {
            name: String::new(),
            content: Content::Vector(s),
            delay_ms: 0,
        });
        img
    } else if ext == "xpm" || starts(b"/* XPM */") {
        simple::xpm(d)?
    } else if ext == "xbm" || starts(b"#define") {
        simple::xbm(d)?
    } else if ext == "pcx" || (d.len() > 128 && d[0] == 10 && d[2] == 1) && ext == "pcx" {
        simple::pcx(d)?
    } else if ext == "wbmp" {
        simple::wbmp(d)?
    } else if matches!(ext.as_str(), "tga" | "icb" | "vda" | "vst") {
        simple::tga(d)?
    } else if RAW_EXTS.contains(&ext.as_str()) {
        tiff::raw_preview(d, raw_name(&ext))?
    } else if let Some(f) = format_for_ext(&ext) {
        if f.supported {
            return Err(Error::Corrupt);
        }
        return Err(Error::Unsupported(f.name));
    } else {
        return Err(Error::Unsupported("?"));
    };
    if (2..=8).contains(&img.orientation) {
        let o = img.orientation;
        for p in img.pages.iter_mut() {
            if let Content::Raster(r) = &p.content {
                p.content = Content::Raster(r.transformed(o));
            }
        }
    }
    Ok(img)
}

fn raw_name(ext: &str) -> &'static str {
    match ext {
        "cr2" | "crw" => "Canon RAW",
        "cr3" => "CR3",
        "nef" | "nrw" => "Nikon NEF",
        "arw" | "srf" | "sr2" => "Sony ARW",
        "dng" => "DNG",
        "orf" => "Olympus ORF",
        "rw2" | "rwl" => "Panasonic RW2",
        "raf" => "Fujifilm RAF",
        "pef" => "Pentax PEF",
        "srw" => "Samsung SRW",
        _ => "RAW",
    }
}

fn looks_svg(d: &[u8]) -> bool {
    let head = &d[..d.len().min(4096)];
    let s = String::from_utf8_lossy(head);
    s.contains("<svg")
}

/// Vignette XMP (xmpGImg:image) des fichiers Illustrator et assimilés.
fn embedded_xmp_thumb(d: &[u8], name: &'static str) -> Result<Image, Error> {
    let key = b"xmpGImg:image>";
    let pos = d.windows(key.len()).position(|w| w == key);
    let from_attr = || {
        d.windows(15)
            .position(|w| w == b"xmpGImg:image=\"")
            .map(|p| p + 15)
    };
    let start = match pos {
        Some(p) => p + key.len(),
        None => from_attr().ok_or(Error::Variant("aperçu absent"))?,
    };
    let end = start
        + d[start..]
            .iter()
            .position(|&c| c == b'<' || c == b'"')
            .ok_or(Error::Corrupt)?;
    let b64 = String::from_utf8_lossy(&d[start..end]).replace("&#xA;", "");
    let bytes = base64(&b64).ok_or(Error::Corrupt)?;
    let r = jpeg::decode_raster(&bytes, None)?;
    let mut img = Image::new(name);
    meta::xmp(
        d.get(start.saturating_sub(65536)..end).unwrap_or(&[]),
        &mut img,
    );
    img.embedded_preview = true;
    img.pages.push(Page::raster(String::new(), r));
    Ok(img)
}

/// EPS : aperçu TIFF (en-tête DOS binaire), aperçu EPSI ou vignette XMP.
fn eps(d: &[u8]) -> Result<Image, Error> {
    if d.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        let to = le32(d, 20) as usize;
        let tl = le32(d, 24) as usize;
        if tl > 0 {
            if let Some(t) = d.get(to..to + tl) {
                let mut img = tiff::decode(t, "EPS")?;
                img.embedded_preview = true;
                return Ok(img);
            }
        }
        let _wo = le32(d, 12) as usize;
        let wl = le32(d, 16) as usize;
        if wl > 0 {
            return Err(Error::Variant("EPS aperçu WMF"));
        }
    }
    if let Ok(mut i) = embedded_xmp_thumb(d, "EPS") {
        i.embedded_preview = true;
        return Ok(i);
    }
    let s = String::from_utf8_lossy(&d[..d.len().min(8 << 20)]);
    if let Some(p) = s.find("%%BeginPreview:") {
        let head: Vec<u32> = s[p + 15..]
            .lines()
            .next()
            .unwrap_or("")
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        if head.len() >= 3 {
            let (w, h, bits) = (head[0], head[1], head[2].max(1));
            check_size(w, h)?;
            let end = s[p..]
                .find("%%EndPreview")
                .map(|e| p + e)
                .unwrap_or(s.len());
            let hex: Vec<u8> = s[p..end]
                .lines()
                .skip(1)
                .flat_map(|l| {
                    l.trim_start_matches('%')
                        .trim()
                        .as_bytes()
                        .chunks(2)
                        .filter_map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
                        .collect::<Vec<_>>()
                })
                .collect();
            let stride = (w as usize * bits as usize).div_ceil(8);
            let mut r = Raster::new(w, h);
            let maxv = (1u32 << bits.min(8)) - 1;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let bit = x * bits as usize;
                    let b = *hex.get(y * stride + bit / 8).unwrap_or(&0);
                    let v = (b >> (8 - bits as usize - bit % 8)) as u32 & maxv;
                    // EPSI : 0 = blanc
                    let g = 255 - (v * 255 / maxv) as u8;
                    r.set(x as u32, y as u32, [g, g, g, 255]);
                }
            }
            let mut img = Image::new("EPS");
            img.embedded_preview = true;
            img.pages.push(Page::raster(String::new(), r));
            return Ok(img);
        }
    }
    Err(Error::Variant("EPS sans aperçu"))
}

pub fn decode_file(path: &std::path::Path) -> Result<Image, Error> {
    let meta = std::fs::metadata(path).map_err(|_| Error::Io)?;
    if meta.len() > 1 << 30 {
        return Err(Error::TooLarge);
    }
    let d = std::fs::read(path).map_err(|_| Error::Io)?;
    decode_bytes(&d, &ext_of(&path.to_string_lossy()))
}

/// Paramètres de vue pour le rendu.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Pixels écran par pixel image.
    pub zoom: f32,
    /// Centre de la vue en coordonnées image.
    pub cx: f32,
    pub cy: f32,
    /// Quarts de tour horaires.
    pub rot: u8,
    pub mirror: bool,
    /// Angle de rotation des modèles 3D (radians).
    pub yaw: f32,
    pub pitch: f32,
    /// Correction d'exposition en IL (développement RAW / comparaison).
    pub exposure: f32,
    pub auto_levels: bool,
    /// Position du séparateur avant/après (0..1), négatif = désactivé.
    pub split: f32,
}

impl Default for View {
    fn default() -> Self {
        View {
            zoom: 1.0,
            cx: 0.0,
            cy: 0.0,
            rot: 0,
            mirror: false,
            yaw: 0.6,
            pitch: 0.35,
            exposure: 0.0,
            auto_levels: false,
            split: -1.0,
        }
    }
}

/// Image décodée et ses caches de rendu (pyramides par page).
pub struct Loaded {
    pub img: Image,
    mips: Vec<Vec<Raster>>,
    levels: Vec<Option<[f32; 6]>>,
}

impl Loaded {
    pub fn new(img: Image) -> Loaded {
        let n = img.pages.len();
        Loaded {
            img,
            mips: vec![Vec::new(); n],
            levels: vec![None; n],
        }
    }

    fn mip(&mut self, page: usize, level: usize) -> Option<&Raster> {
        let Content::Raster(base) = &self.img.pages.get(page)?.content else {
            return None;
        };
        let chain = &mut self.mips[page];
        while chain.len() < level {
            let src = chain.last().unwrap_or(base);
            if src.w == 1 && src.h == 1 {
                break;
            }
            let h = src.half();
            chain.push(h);
        }
        if level == 0 || chain.is_empty() {
            return Some(base);
        }
        chain.get(level.min(chain.len()) - 1)
    }

    /// Niveaux automatiques (percentiles 0,5 % / 99,5 % par canal).
    fn auto(&mut self, page: usize) -> [f32; 6] {
        if let Some(l) = self.levels.get(page).copied().flatten() {
            return l;
        }
        let mut lv = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        if let Some(Content::Raster(r)) = self.img.pages.get(page).map(|p| &p.content) {
            let mut hist = [[0u32; 256]; 3];
            let step = ((r.px.len() / 4) / 250_000).max(1);
            let mut n = 0u32;
            for p in r.px.as_chunks::<4>().0.iter().step_by(step) {
                for c in 0..3 {
                    hist[c][p[c] as usize] += 1;
                }
                n += 1;
            }
            for c in 0..3 {
                let lo_t = n / 200;
                let mut acc = 0;
                let mut lo = 0;
                for (i, v) in hist[c].iter().enumerate() {
                    acc += v;
                    if acc > lo_t {
                        lo = i;
                        break;
                    }
                }
                acc = 0;
                let mut hi = 255;
                for (i, v) in hist[c].iter().enumerate().rev() {
                    acc += v;
                    if acc > lo_t {
                        hi = i;
                        break;
                    }
                }
                if hi <= lo + 8 {
                    lo = 0;
                    hi = 255;
                }
                lv[c] = lo as f32 / 255.0;
                lv[c + 3] = hi as f32 / 255.0;
            }
        }
        if let Some(slot) = self.levels.get_mut(page) {
            *slot = Some(lv);
        }
        lv
    }

    /// Rend la page dans `out` (BGRA, `stride` octets par ligne).
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        page: usize,
        v: &View,
        ow: usize,
        oh: usize,
        stride: usize,
        out: &mut [u8],
        bg: [u8; 3],
        checker: bool,
    ) {
        if ow == 0 || oh == 0 || out.len() < stride * (oh - 1) + ow * 4 {
            return;
        }
        let Some(pg) = self.img.pages.get(page) else {
            return;
        };
        let (pw, ph) = pg.size();
        // Matrice image -> écran
        let view_m = view_matrix(v, pw as f32, ph as f32, ow as f32, oh as f32);
        let mut cv = Canvas::new(ow, oh);
        match &pg.content {
            Content::Vector(s) => svg::render(s, &view_m, &mut cv),
            Content::Model(m) => {
                let side = (pw.min(ph) as f32) * v.zoom;
                let (sx, sy) = view_m.apply((pw as f32 / 2.0, ph as f32 / 2.0));
                model3d::render(m, &mut cv, sx, sy, side, v.yaw, v.pitch, bg);
            }
            Content::Raster(_) => {
                let level = if v.zoom < 1.0 {
                    ((1.0 / v.zoom).log2().floor() as usize).min(12)
                } else {
                    0
                };
                let nearest = v.zoom >= 3.0;
                let Some(inv) = view_m.inv() else {
                    return;
                };
                let auto = if v.auto_levels {
                    Some(self.auto(page))
                } else {
                    None
                };
                let gain = 2f32.powf(v.exposure);
                let adjust = v.exposure.abs() > 1e-3 || auto.is_some();
                let split_x = if v.split >= 0.0 {
                    (v.split * ow as f32) as usize
                } else {
                    0
                };
                let Some(r) = self.mip(page, level) else {
                    return;
                };
                let scale = r.w as f32 / pw as f32;
                let (rw, rh) = (r.w as f32, r.h as f32);
                for y in 0..oh {
                    let (mut u, mut vv) = inv.apply((0.5, y as f32 + 0.5));
                    let (du, dv) = (inv.0[0], inv.0[1]);
                    for x in 0..ow {
                        let (su, sv) = (u * scale, vv * scale);
                        u += du;
                        vv += dv;
                        if su < 0.0 || sv < 0.0 || su >= rw || sv >= rh {
                            continue;
                        }
                        let mut c = if nearest {
                            let p = r.get(su as u32, sv as u32);
                            let a = p[3] as f32 / 255.0;
                            [
                                p[0] as f32 / 255.0 * a,
                                p[1] as f32 / 255.0 * a,
                                p[2] as f32 / 255.0 * a,
                                a,
                            ]
                        } else {
                            r.sample_premul(su, sv)
                        };
                        if adjust && x >= split_x {
                            for k in 0..3 {
                                let mut val = c[k];
                                if let Some(l) = auto {
                                    let a = c[3].max(1e-6);
                                    val = ((val / a - l[k]) / (l[k + 3] - l[k]).max(1e-3))
                                        .clamp(0.0, 1.0)
                                        * a;
                                }
                                c[k] = (val * gain).min(c[3]);
                            }
                        }
                        cv.px[y * ow + x] = c;
                    }
                }
            }
        }
        // Composition sur le fond (damier là où l'image est transparente)
        let dark = (bg[0] as u32 + bg[1] as u32 + bg[2] as u32) < 384;
        let (c1, c2) = if dark { (0x3A, 0x30) } else { (0xFF, 0xE6) };
        let inv = view_m.inv();
        let bgf = [
            bg[0] as f32 / 255.0,
            bg[1] as f32 / 255.0,
            bg[2] as f32 / 255.0,
        ];
        for y in 0..oh {
            let row = &mut out[y * stride..y * stride + ow * 4];
            for x in 0..ow {
                let p = cv.px[y * ow + x];
                let a = p[3].clamp(0.0, 1.0);
                let base = if checker
                    && a < 1.0
                    && inv.is_some_and(|i| {
                        let (u, v) = i.apply((x as f32 + 0.5, y as f32 + 0.5));
                        u >= 0.0 && v >= 0.0 && u < pw as f32 && v < ph as f32
                    }) {
                    let k = if ((x / 8) + (y / 8)) % 2 == 0 { c1 } else { c2 };
                    [k as f32 / 255.0; 3]
                } else {
                    bgf
                };
                let px =
                    |i: usize| ((p[i] + base[i] * (1.0 - a)).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                row[x * 4] = px(2);
                row[x * 4 + 1] = px(1);
                row[x * 4 + 2] = px(0);
                row[x * 4 + 3] = 255;
            }
        }
        if v.split >= 0.0 && split_x_valid(v.split) {
            let sx = ((v.split * ow as f32) as usize).min(ow - 1);
            for y in 0..oh {
                let o = y * stride + sx * 4;
                out[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
                if sx + 1 < ow {
                    out[o + 4..o + 8].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
    }
}

fn split_x_valid(s: f32) -> bool {
    (0.0..=1.0).contains(&s)
}

/// Image -> écran : centre (cx,cy) au milieu, zoom, rotation, miroir.
pub fn view_matrix(v: &View, _pw: f32, _ph: f32, ow: f32, oh: f32) -> Mat {
    let rot = match v.rot % 4 {
        1 => Mat([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]),
        2 => Mat([-1.0, 0.0, 0.0, -1.0, 0.0, 0.0]),
        3 => Mat([0.0, -1.0, 1.0, 0.0, 0.0, 0.0]),
        _ => Mat::ID,
    };
    let mir = if v.mirror {
        Mat::scale(-1.0, 1.0)
    } else {
        Mat::ID
    };
    Mat::translate(ow / 2.0, oh / 2.0)
        .mul(&mir)
        .mul(&rot)
        .mul(&Mat::scale(v.zoom, v.zoom))
        .mul(&Mat::translate(-v.cx, -v.cy))
}

/// Vignette BGRA carrée `size`×`size` (image centrée, fond transparent).
pub fn thumbnail(img: &mut Loaded, size: u32) -> Vec<u8> {
    let s = size as usize;
    let (pw, ph) = img.img.pages.first().map(|p| p.size()).unwrap_or((1, 1));
    let zoom = (size as f32 / pw.max(1) as f32).min(size as f32 / ph.max(1) as f32);
    let zoom = if matches!(
        img.img.pages.first().map(|p| &p.content),
        Some(Content::Vector(_))
    ) {
        zoom
    } else {
        zoom.min(1.0)
    };
    let v = View {
        zoom,
        cx: pw as f32 / 2.0,
        cy: ph as f32 / 2.0,
        ..Default::default()
    };
    // Rendu sur noir puis sur blanc : reconstitue l'alpha.
    let mut a = vec![0u8; s * s * 4];
    let mut b = vec![0u8; s * s * 4];
    img.render(0, &v, s, s, s * 4, &mut a, [0, 0, 0], false);
    let is_model = matches!(
        img.img.pages.first().map(|p| &p.content),
        Some(Content::Model(_))
    );
    if is_model {
        // l'aperçu 3D n'a pas de transparence partielle : fond = transparent
        let mut out = vec![0u8; s * s * 4];
        for i in 0..s * s {
            let p = &a[i * 4..i * 4 + 4];
            if p[0] != 0 || p[1] != 0 || p[2] != 0 {
                out[i * 4..i * 4 + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        return out;
    }
    img.render(0, &v, s, s, s * 4, &mut b, [255, 255, 255], false);
    let mut out = vec![0u8; s * s * 4];
    for i in 0..s * s {
        let alpha = 255 - (b[i * 4 + 1] as i32 - a[i * 4 + 1] as i32).clamp(0, 255);
        let o = &mut out[i * 4..i * 4 + 4];
        if alpha > 0 {
            for k in 0..3 {
                o[k] = ((a[i * 4 + k] as i32 * 255) / alpha).clamp(0, 255) as u8;
            }
        }
        o[3] = alpha as u8;
    }
    out
}

/// `winlux --image-info FICHIER` : diagnostic du décodeur.
pub fn cli_info(path: &std::path::Path) -> i32 {
    let t = std::time::Instant::now();
    match decode_file(path) {
        Ok(img) => {
            let (w, h) = img.pages.first().map(|p| p.size()).unwrap_or((0, 0));
            println!("format : {}", img.format);
            println!("taille : {w} x {h}");
            println!("pages : {}", img.pages.len());
            if img.embedded_preview {
                println!("aperçu intégré au fichier");
            }
            for (k, v) in &img.meta {
                println!("{k} : {v}");
            }
            println!("décodé en {} ms", t.elapsed().as_millis());
            0
        }
        Err(e) => {
            eprintln!("winlux: image illisible ({e:?})");
            1
        }
    }
}

#[cfg(test)]
mod tests;
