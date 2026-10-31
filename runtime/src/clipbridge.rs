//! Pont entre le presse-papiers Windows de Winlux et celui de Linux (X11).
//!
//! `winlux --clipboard-bridge` tourne une fois par session graphique. Il
//! relie le fichier partagé du presse-papiers Win32 (`dlls::clipboard`) à la
//! sélection X11 `CLIPBOARD`, dans les deux sens :
//!
//! - une copie faite dans un `.exe` (le numéro de séquence du fichier change)
//!   est publiée sur X11 : texte, HTML, image PNG/BMP et fichiers
//!   (`text/uri-list`, `x-special/gnome-copied-files` pour Thunar) ;
//! - une copie faite dans une appli Linux (XFixes signale un nouveau
//!   propriétaire) est importée : CF_UNICODETEXT, « HTML Format », CF_DIB +
//!   « PNG », CF_HDROP + « Preferred DropEffect » (couper/copier).
//!
//! Comme l'import passe par le fichier partagé, l'historique Win+V voit aussi
//! les copies faites côté Linux. Les grosses données passent par INCR.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::dlls::clipboard::{
    self as cb, Contents, CF_DIB, CF_HDROP, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT,
};

pub const CF_DIBV5: u32 = 17;
pub const FORMAT_HTML: &str = "HTML Format";
pub const FORMAT_PNG: &str = "PNG";
pub const FORMAT_DROPEFFECT: &str = "Preferred DropEffect";
pub const DROPEFFECT_COPY: u32 = 1;
pub const DROPEFFECT_MOVE: u32 = 2;

// ---------------------------------------------------------------------------
// Conversions (sans X11, testées unitairement)
// ---------------------------------------------------------------------------

/// Texte Linux (LF) → texte Windows (CRLF).
pub fn to_crlf(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut prev = '\0';
    for c in text.chars() {
        if c == '\n' && prev != '\r' {
            out.push('\r');
        }
        out.push(c);
        prev = c;
    }
    out
}

/// Texte Windows (CRLF) → texte Linux (LF).
pub fn to_lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// CF_HDROP en UTF-16 (DROPFILES de 20 octets puis chemins doublement terminés).
pub fn hdrop_build(paths: &[String]) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    out[0..4].copy_from_slice(&20u32.to_le_bytes());
    out[16..20].copy_from_slice(&1u32.to_le_bytes());
    for p in paths {
        for u in p.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// Chemins Windows d'un CF_HDROP, en UTF-16 ou ANSI.
pub fn hdrop_parse(data: &[u8]) -> Vec<String> {
    if data.len() < 20 {
        return Vec::new();
    }
    let offset = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let wide = u32::from_le_bytes(data[16..20].try_into().unwrap()) != 0;
    let Some(body) = data.get(offset..) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if wide {
        let mut cur: Vec<u16> = Vec::new();
        for c in body.as_chunks::<2>().0 {
            let u = u16::from_le_bytes(*c);
            if u != 0 {
                cur.push(u);
            } else if cur.is_empty() {
                break;
            } else {
                out.push(String::from_utf16_lossy(&cur));
                cur.clear();
            }
        }
    } else {
        for part in body.split(|&b| b == 0) {
            if part.is_empty() {
                break;
            }
            out.push(crate::dlls::decode_ansi(part));
        }
    }
    out
}

/// URI `file://` d'un chemin Linux, avec les octets réservés encodés.
pub fn file_uri(p: &Path) -> String {
    let mut s = String::from("file://");
    for &b in p.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~!$&'()*+,;=:@".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = hex {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Fichiers d'une liste `text/uri-list` ou `x-special/gnome-copied-files`.
/// Renvoie aussi `true` si la liste commence par « cut » (couper).
pub fn parse_uri_list(text: &str) -> (Vec<PathBuf>, bool) {
    let mut cut = false;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_matches(|c: char| c == '\r' || c == '\0' || c == ' ');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if i == 0 && (line == "cut" || line == "copy") {
            cut = line == "cut";
            continue;
        }
        let Some(rest) = line.strip_prefix("file:") else {
            continue;
        };
        // file:///chemin, file://hôte/chemin ou file:/chemin
        let path = if let Some(r) = rest.strip_prefix("//") {
            match r.find('/') {
                Some(slash) => &r[slash..],
                None => continue,
            }
        } else {
            rest
        };
        if !path.starts_with('/') {
            continue;
        }
        out.push(PathBuf::from(OsString::from_vec(percent_decode(path))));
    }
    (out, cut)
}

fn le_u16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}
fn le_u32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn mask_channel(px: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 255;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    (((px & mask) >> shift) * 255 / max.max(1)) as u8
}

/// CF_DIB / CF_DIBV5 → (largeur, hauteur, RGBA). Gère 1/4/8/16/24/32 bits,
/// BI_RGB et BI_BITFIELDS, ainsi que les DIB contenant un PNG ou un JPEG.
pub fn dib_to_rgba(d: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let hsize = le_u32(d, 0)? as usize;
    if hsize < 40 || d.len() < hsize {
        return None;
    }
    let w = le_u32(d, 4)? as i32;
    let h = le_u32(d, 8)? as i32;
    let bpp = le_u16(d, 14)?;
    let comp = le_u32(d, 16)?;
    let clr_used = le_u32(d, 32)? as usize;
    if w <= 0 || h == 0 || w > 32768 || h.unsigned_abs() > 32768 {
        return None;
    }
    let (w, top_down, hh) = (w as u32, h < 0, h.unsigned_abs());
    let mut off = hsize;
    if comp == 4 || comp == 5 {
        // BI_JPEG / BI_PNG : l'image compressée suit l'en-tête.
        let raster = crate::imaging::decode_bytes(d.get(off..)?, "")
            .ok()?
            .pages
            .into_iter()
            .next()?
            .into_raster();
        return Some((raster.w, raster.h, raster.px));
    }
    let masks = if comp == 3 || comp == 6 {
        let m = if hsize == 40 {
            off += if comp == 6 { 16 } else { 12 };
            (
                le_u32(d, 40)?,
                le_u32(d, 44)?,
                le_u32(d, 48)?,
                le_u32(d, 52).unwrap_or(0),
            )
        } else {
            (
                le_u32(d, 40)?,
                le_u32(d, 44)?,
                le_u32(d, 48)?,
                le_u32(d, 52)?,
            )
        };
        Some(m)
    } else if comp == 0 {
        None
    } else {
        return None; // RLE : non géré.
    };
    let mut palette = Vec::new();
    if bpp <= 8 {
        let n = if clr_used > 0 {
            clr_used.min(256)
        } else {
            1 << bpp
        };
        let pal = d.get(off..off + n * 4)?;
        palette = pal
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[2], c[1], c[0], 255])
            .collect();
        off += n * 4;
    }
    let stride = (w as usize * bpp as usize).div_ceil(32) * 4;
    let pixels = d.get(off..off + stride * hh as usize)?;
    let mut out = vec![0u8; w as usize * hh as usize * 4];
    let mut any_alpha = false;
    for y in 0..hh as usize {
        let src_row = if top_down { y } else { hh as usize - 1 - y };
        let row = &pixels[src_row * stride..src_row * stride + stride];
        for x in 0..w as usize {
            let px: [u8; 4] = match bpp {
                32 => {
                    let v = u32::from_le_bytes(row[x * 4..x * 4 + 4].try_into().unwrap());
                    match masks {
                        Some((r, g, b, a)) => [
                            mask_channel(v, r),
                            mask_channel(v, g),
                            mask_channel(v, b),
                            if a == 0 { 0 } else { mask_channel(v, a) },
                        ],
                        None => [row[x * 4 + 2], row[x * 4 + 1], row[x * 4], row[x * 4 + 3]],
                    }
                }
                24 => [row[x * 3 + 2], row[x * 3 + 1], row[x * 3], 255],
                16 => {
                    let v = u16::from_le_bytes(row[x * 2..x * 2 + 2].try_into().unwrap()) as u32;
                    let (r, g, b, _) = masks.unwrap_or((0x7C00, 0x03E0, 0x001F, 0));
                    [
                        mask_channel(v, r),
                        mask_channel(v, g),
                        mask_channel(v, b),
                        255,
                    ]
                }
                8 | 4 | 1 => {
                    let bit = x * bpp as usize;
                    let byte = row[bit / 8];
                    let shift = 8 - bpp as usize - (bit % 8);
                    let idx = (byte >> shift) as usize & ((1 << bpp) - 1);
                    *palette.get(idx).unwrap_or(&[0, 0, 0, 255])
                }
                _ => return None,
            };
            if bpp == 32 && px[3] != 0 {
                any_alpha = true;
            }
            out[(y * w as usize + x) * 4..][..4].copy_from_slice(&px);
        }
    }
    // Un DIB 32 bits sans alpha (cas le plus courant) est entièrement opaque.
    if bpp == 32 && !any_alpha {
        for a in out.iter_mut().skip(3).step_by(4) {
            *a = 255;
        }
    }
    Some((w, hh, out))
}

/// RGBA → CF_DIB 32 bits BI_RGB, de bas en haut comme Windows.
pub fn rgba_to_dib(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + rgba.len());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(w * h * 4).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes()); // 72 ppp
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    for y in (0..h as usize).rev() {
        for px in rgba[y * w as usize * 4..(y + 1) * w as usize * 4]
            .as_chunks::<4>()
            .0
        {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    out
}

/// Fichier `.bmp` (image/bmp) à partir d'un DIB.
pub fn dib_to_bmp_file(dib: &[u8]) -> Option<Vec<u8>> {
    let hsize = le_u32(dib, 0)?;
    let bpp = le_u16(dib, 14)?;
    let comp = le_u32(dib, 16)?;
    let clr_used = le_u32(dib, 32)?;
    let mut bits = hsize;
    if hsize == 40 && comp == 3 {
        bits += 12;
    }
    if bpp <= 8 {
        bits += 4 * if clr_used > 0 { clr_used } else { 1 << bpp };
    }
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(14 + dib.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(14 + bits).to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

pub fn png_encode(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
        writer.finish().ok()?;
    }
    Some(out)
}

/// Décode une image Linux (PNG, JPEG, BMP, GIF, WebP…) en RGBA.
pub fn decode_image(bytes: &[u8], mime: &str) -> Option<(u32, u32, Vec<u8>)> {
    let ext = mime.rsplit('/').next().unwrap_or("");
    let raster = crate::imaging::decode_bytes(bytes, ext)
        .ok()?
        .pages
        .into_iter()
        .next()?
        .into_raster();
    Some((raster.w, raster.h, raster.px))
}

const HTML_HEADER: &str = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";

/// HTML Linux → format « HTML Format » de Windows (CF_HTML, UTF-8).
pub fn cf_html_wrap(fragment: &str) -> Vec<u8> {
    let pre = "<html><body>\r\n<!--StartFragment-->";
    let post = "<!--EndFragment-->\r\n</body></html>";
    let start_html = HTML_HEADER.len();
    let start_frag = start_html + pre.len();
    let end_frag = start_frag + fragment.len();
    let end_html = end_frag + post.len();
    let header = format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_frag:010}\r\nEndFragment:{end_frag:010}\r\n"
    );
    let mut out = header.into_bytes();
    out.extend_from_slice(pre.as_bytes());
    out.extend_from_slice(fragment.as_bytes());
    out.extend_from_slice(post.as_bytes());
    out.push(0);
    out
}

/// Fragment HTML d'un « HTML Format » Windows.
pub fn cf_html_fragment(data: &[u8]) -> Option<String> {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    let data = &data[..end];
    let head = String::from_utf8_lossy(&data[..data.len().min(400)]).into_owned();
    let field = |name: &str| -> Option<usize> {
        let at = head.find(name)? + name.len();
        let digits: String = head[at..]
            .chars()
            .skip_while(|c| *c == ' ')
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        digits
            .parse::<i64>()
            .ok()
            .filter(|v| *v >= 0)
            .map(|v| v as usize)
    };
    let (s, e) = match (field("StartFragment:"), field("EndFragment:")) {
        (Some(s), Some(e)) => (s, e),
        _ => (field("StartHTML:")?, field("EndHTML:")?),
    };
    let slice = data.get(s..e.min(data.len()))?;
    Some(String::from_utf8_lossy(slice).into_owned())
}

/// Texte d'une cible X11 : UTF-8, Latin-1 (`STRING`) ou UTF-16 avec BOM.
pub fn decode_x_text(bytes: &[u8], latin1: bool) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&u| u != 0)
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let bytes = match bytes.iter().rposition(|&b| b != 0) {
        Some(last) => &bytes[..=last],
        None => &[][..],
    };
    if latin1 {
        return bytes.iter().map(|&b| b as char).collect();
    }
    String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).into_owned()
}

/// Ce qu'une appli Linux a mis dans le presse-papiers.
#[derive(Default, Debug)]
pub struct LinuxClip {
    pub text: Option<String>,
    pub html: Option<String>,
    /// Image encodée et son type MIME.
    pub image: Option<(Vec<u8>, String)>,
    pub files: Vec<PathBuf>,
    pub cut: bool,
}

impl LinuxClip {
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.html.is_none() && self.image.is_none() && self.files.is_empty()
    }
}

/// Contenu Windows équivalent à une copie Linux, avec les noms des formats
/// enregistrés à écrire dans le fichier partagé.
pub fn import(
    clip: &LinuxClip,
    to_windows: impl Fn(&Path) -> String,
) -> (Contents, HashMap<u32, String>) {
    let mut c = Contents::default();
    let mut names = HashMap::new();
    let mut put = |c: &mut Contents, name: Option<&str>, format: u32, data: Vec<u8>| {
        if let Some(n) = name {
            names.insert(format, n.to_string());
        }
        c.formats.push((format, data));
    };
    if !clip.files.is_empty() {
        let paths: Vec<String> = clip.files.iter().map(|p| to_windows(p)).collect();
        put(&mut c, None, CF_HDROP, hdrop_build(&paths));
        let effect = if clip.cut {
            DROPEFFECT_MOVE
        } else {
            DROPEFFECT_COPY
        };
        put(
            &mut c,
            Some(FORMAT_DROPEFFECT),
            cb::registered_id(FORMAT_DROPEFFECT),
            effect.to_le_bytes().to_vec(),
        );
    }
    if let Some((bytes, mime)) = &clip.image {
        if let Some((w, h, rgba)) = decode_image(bytes, mime) {
            put(&mut c, None, CF_DIB, rgba_to_dib(w, h, &rgba));
            let png = if mime == "image/png" {
                Some(bytes.clone())
            } else {
                png_encode(w, h, &rgba)
            };
            if let Some(png) = png {
                put(&mut c, Some(FORMAT_PNG), cb::registered_id(FORMAT_PNG), png);
            }
        }
    }
    if let Some(html) = &clip.html {
        put(
            &mut c,
            Some(FORMAT_HTML),
            cb::registered_id(FORMAT_HTML),
            cf_html_wrap(html),
        );
    }
    if let Some(text) = &clip.text {
        let mut w = Vec::with_capacity(text.len() * 2 + 2);
        for u in to_crlf(text).encode_utf16().chain(std::iter::once(0)) {
            w.extend_from_slice(&u.to_le_bytes());
        }
        put(&mut c, None, CF_UNICODETEXT, w);
    }
    (c, names)
}

pub const TEXT_TARGETS: &[&str] = &[
    "UTF8_STRING",
    "text/plain;charset=utf-8",
    "text/plain",
    "STRING",
    "TEXT",
];

fn image_source(c: &Contents) -> Option<&[u8]> {
    c.get(CF_DIB).or_else(|| c.get(CF_DIBV5))
}

fn file_paths(c: &Contents, to_linux: &impl Fn(&str) -> PathBuf) -> Vec<PathBuf> {
    c.get(CF_HDROP)
        .map(|d| hdrop_parse(d).iter().map(|p| to_linux(p)).collect())
        .unwrap_or_default()
}

/// Cibles X11 offertes pour un contenu Windows.
pub fn export_targets(c: &Contents) -> Vec<&'static str> {
    let mut t = Vec::new();
    if c.get(CF_HDROP).is_some() {
        t.extend(["x-special/gnome-copied-files", "text/uri-list"]);
    }
    if c.get(cb::registered_id(FORMAT_PNG)).is_some() || image_source(c).is_some() {
        t.push("image/png");
        if image_source(c).is_some() {
            t.push("image/bmp");
        }
    }
    if c.get(cb::registered_id(FORMAT_HTML)).is_some() {
        t.push("text/html");
    }
    let has_text = c
        .formats
        .iter()
        .any(|(f, _)| matches!(*f, CF_TEXT | CF_OEMTEXT | CF_UNICODETEXT));
    if has_text || c.get(CF_HDROP).is_some() {
        t.extend_from_slice(TEXT_TARGETS);
    }
    t
}

/// Données d'une cible X11 à partir du contenu Windows.
pub fn export_render(
    c: &Contents,
    target: &str,
    to_linux: impl Fn(&str) -> PathBuf,
) -> Option<Vec<u8>> {
    match target {
        "text/uri-list" => {
            let files = file_paths(c, &to_linux);
            (!files.is_empty()).then(|| {
                files
                    .iter()
                    .map(|p| file_uri(p) + "\r\n")
                    .collect::<String>()
                    .into_bytes()
            })
        }
        "x-special/gnome-copied-files" => {
            let files = file_paths(c, &to_linux);
            if files.is_empty() {
                return None;
            }
            let cut = c
                .get(cb::registered_id(FORMAT_DROPEFFECT))
                .and_then(|d| le_u32(d, 0))
                .is_some_and(|e| e & DROPEFFECT_MOVE != 0 && e & DROPEFFECT_COPY == 0);
            let mut s = String::from(if cut { "cut" } else { "copy" });
            for p in &files {
                s.push('\n');
                s.push_str(&file_uri(p));
            }
            Some(s.into_bytes())
        }
        "image/png" => {
            if let Some(png) = c.get(cb::registered_id(FORMAT_PNG)) {
                return Some(png.to_vec());
            }
            let (w, h, rgba) = dib_to_rgba(image_source(c)?)?;
            png_encode(w, h, &rgba)
        }
        "image/bmp" => dib_to_bmp_file(image_source(c)?),
        "text/html" => {
            cf_html_fragment(c.get(cb::registered_id(FORMAT_HTML))?).map(String::into_bytes)
        }
        t if TEXT_TARGETS.contains(&t) => {
            let text = match c.text() {
                Some(t) => to_lf(&t),
                None => file_paths(c, &to_linux)
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            if t == "STRING" {
                Some(
                    text.chars()
                        .map(|ch| if (ch as u32) < 256 { ch as u8 } else { b'?' })
                        .collect(),
                )
            } else {
                Some(text.into_bytes())
            }
        }
        _ => None,
    }
}

/// Cibles à demander à une appli Linux, dans l'ordre, selon ses TARGETS.
pub fn import_plan(offered: &[String]) -> Vec<String> {
    let has = |t: &str| offered.iter().any(|o| o.eq_ignore_ascii_case(t));
    let mut plan = Vec::new();
    if has("x-special/gnome-copied-files") {
        plan.push("x-special/gnome-copied-files".to_string());
    } else if has("text/uri-list") {
        plan.push("text/uri-list".to_string());
    }
    for img in [
        "image/png",
        "image/bmp",
        "image/x-bmp",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "image/tiff",
    ] {
        if has(img) {
            plan.push(img.to_string());
            break;
        }
    }
    if has("text/html") {
        plan.push("text/html".to_string());
    }
    if let Some(t) = TEXT_TARGETS.iter().find(|t| has(t)) {
        plan.push(t.to_string());
    }
    plan
}

/// Range une réponse d'appli Linux dans `clip`.
pub fn absorb(clip: &mut LinuxClip, target: &str, data: &[u8]) {
    match target {
        "x-special/gnome-copied-files" | "text/uri-list" => {
            let (files, cut) = parse_uri_list(&decode_x_text(data, false));
            if !files.is_empty() {
                clip.files = files;
                clip.cut = cut;
            }
        }
        t if t.starts_with("image/") => {
            if !data.is_empty() {
                clip.image = Some((data.to_vec(), t.to_string()));
            }
        }
        "text/html" => {
            let html = decode_x_text(data, false);
            if !html.trim().is_empty() {
                clip.html = Some(html);
            }
        }
        t if TEXT_TARGETS.contains(&t) => {
            clip.text = Some(decode_x_text(data, t == "STRING"));
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Boucle X11
// ---------------------------------------------------------------------------

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask,
    PropMode, Property, SelectionNotifyEvent, SelectionRequestEvent, Timestamp, Window,
    WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::CURRENT_TIME;

type XResult<T> = Result<T, Box<dyn std::error::Error>>;

struct Fetch {
    queue: VecDeque<String>,
    current: Option<String>,
    incr: Option<Vec<u8>>,
    clip: LinuxClip,
    targets_known: bool,
    started: Instant,
    time: Timestamp,
}

struct Outgoing {
    data: Vec<u8>,
    pos: usize,
    kind: Atom,
    started: Instant,
}

struct Bridge {
    conn: RustConnection,
    win: Window,
    clipboard: Atom,
    targets: Atom,
    timestamp: Atom,
    incr: Atom,
    utf8: Atom,
    prop: Atom,
    stamp: Atom,
    atoms: HashMap<String, Atom>,
    names: HashMap<Atom, String>,
    path: PathBuf,
    seen_seq: u32,
    own: Option<(Contents, Timestamp)>,
    fetch: Option<Fetch>,
    outgoing: HashMap<(Window, Atom), Outgoing>,
    backlog: VecDeque<Event>,
    chunk: usize,
}

impl Bridge {
    fn atom(&mut self, name: &str) -> XResult<Atom> {
        if let Some(a) = self.atoms.get(name) {
            return Ok(*a);
        }
        let a = self.conn.intern_atom(false, name.as_bytes())?.reply()?.atom;
        self.atoms.insert(name.to_string(), a);
        self.names.insert(a, name.to_string());
        Ok(a)
    }

    fn atom_name(&mut self, a: Atom) -> String {
        if let Some(n) = self.names.get(&a) {
            return n.clone();
        }
        let n = self
            .conn
            .get_atom_name(a)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.name).into_owned())
            .unwrap_or_default();
        self.names.insert(a, n.clone());
        self.atoms.insert(n.clone(), a);
        n
    }

    /// Heure du serveur, obtenue en modifiant une propriété de notre fenêtre.
    fn server_time(&mut self) -> XResult<Timestamp> {
        self.conn.change_property8(
            PropMode::APPEND,
            self.win,
            self.stamp,
            AtomEnum::STRING,
            &[],
        )?;
        self.conn.flush()?;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.conn.poll_for_event()? {
                Some(Event::PropertyNotify(e)) if e.window == self.win && e.atom == self.stamp => {
                    return Ok(e.time);
                }
                Some(other) => self.backlog.push_back(other),
                None => wait_fd(&self.conn, 50),
            }
        }
        Ok(CURRENT_TIME)
    }

    /// Une copie Windows est arrivée : la publier sur X11.
    fn export(&mut self) -> XResult<()> {
        let seq = cb::read_seq(&self.path);
        if seq == self.seen_seq {
            return Ok(());
        }
        self.seen_seq = seq;
        let Some((contents, _)) = cb::load(&self.path) else {
            return Ok(());
        };
        if export_targets(&contents).is_empty() {
            return Ok(());
        }
        self.fetch = None;
        let time = self.server_time()?;
        self.conn
            .set_selection_owner(self.win, self.clipboard, time)?;
        let owner = self
            .conn
            .get_selection_owner(self.clipboard)?
            .reply()?
            .owner;
        if owner == self.win {
            eprintln!("winlux: presse-papiers : copie Windows n°{seq} publiée pour Linux");
            self.own = Some((contents, time));
        }
        Ok(())
    }

    fn notify(&self, e: &SelectionRequestEvent, property: Atom) -> XResult<()> {
        let ev = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property,
        };
        self.conn
            .send_event(false, e.requestor, EventMask::NO_EVENT, ev)?;
        Ok(())
    }

    fn on_request(&mut self, e: SelectionRequestEvent) -> XResult<()> {
        let property = if e.property == x11rb::NONE {
            e.target
        } else {
            e.property
        };
        let Some((contents, time)) = self.own.clone().filter(|_| e.selection == self.clipboard)
        else {
            return self.notify(&e, x11rb::NONE);
        };
        if e.target == self.targets {
            let mut list = vec![self.targets, self.timestamp];
            for t in export_targets(&contents) {
                list.push(self.atom(t)?);
            }
            self.conn.change_property32(
                PropMode::REPLACE,
                e.requestor,
                property,
                AtomEnum::ATOM,
                &list,
            )?;
            return self.notify(&e, property);
        }
        if e.target == self.timestamp {
            self.conn.change_property32(
                PropMode::REPLACE,
                e.requestor,
                property,
                AtomEnum::INTEGER,
                &[time],
            )?;
            return self.notify(&e, property);
        }
        let name = self.atom_name(e.target);
        let Some(data) = export_render(&contents, &name, crate::paths::to_linux) else {
            return self.notify(&e, x11rb::NONE);
        };
        let kind = match name.as_str() {
            "TEXT" => self.utf8,
            "STRING" => AtomEnum::STRING.into(),
            _ => e.target,
        };
        if data.len() > self.chunk {
            // INCR : le demandeur reçoit les données par morceaux.
            self.conn.change_window_attributes(
                e.requestor,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )?;
            self.conn.change_property32(
                PropMode::REPLACE,
                e.requestor,
                property,
                self.incr,
                &[data.len() as u32],
            )?;
            self.outgoing.insert(
                (e.requestor, property),
                Outgoing {
                    data,
                    pos: 0,
                    kind,
                    started: Instant::now(),
                },
            );
        } else {
            self.conn
                .change_property8(PropMode::REPLACE, e.requestor, property, kind, &data)?;
        }
        self.notify(&e, property)
    }

    /// Le demandeur a lu le morceau précédent : envoyer le suivant.
    fn on_incr_sent(&mut self, window: Window, atom: Atom) -> XResult<()> {
        let Some(out) = self.outgoing.get_mut(&(window, atom)) else {
            return Ok(());
        };
        let end = (out.pos + self.chunk).min(out.data.len());
        let piece = out.data[out.pos..end].to_vec();
        let kind = out.kind;
        out.pos = end;
        self.conn
            .change_property8(PropMode::REPLACE, window, atom, kind, &piece)?;
        if piece.is_empty() {
            self.outgoing.remove(&(window, atom));
        }
        Ok(())
    }

    fn start_fetch(&mut self, time: Timestamp) -> XResult<()> {
        self.own = None;
        self.fetch = Some(Fetch {
            queue: VecDeque::new(),
            current: Some("TARGETS".into()),
            incr: None,
            clip: LinuxClip::default(),
            targets_known: false,
            started: Instant::now(),
            time,
        });
        self.conn.delete_property(self.win, self.prop)?;
        self.conn
            .convert_selection(self.win, self.clipboard, self.targets, self.prop, time)?;
        Ok(())
    }

    fn next_fetch(&mut self) -> XResult<()> {
        let Some(f) = self.fetch.as_mut() else {
            return Ok(());
        };
        f.incr = None;
        match f.queue.pop_front() {
            Some(t) => {
                let time = f.time;
                f.current = Some(t.clone());
                let target = self.atom(&t)?;
                self.conn.delete_property(self.win, self.prop)?;
                self.conn
                    .convert_selection(self.win, self.clipboard, target, self.prop, time)?;
            }
            None => self.commit(),
        }
        Ok(())
    }

    fn got_data(&mut self, data: Vec<u8>) -> XResult<()> {
        let Some(f) = self.fetch.as_mut() else {
            return Ok(());
        };
        let target = f.current.take().unwrap_or_default();
        if target == "TARGETS" && !f.targets_known {
            f.targets_known = true;
            let atoms: Vec<Atom> = data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_ne_bytes(*c))
                .collect();
            let offered: Vec<String> = atoms.into_iter().map(|a| self.atom_name(a)).collect();
            let plan = import_plan(&offered);
            if let Some(f) = self.fetch.as_mut() {
                f.queue = plan.into();
            }
        } else {
            absorb(&mut f.clip, &target, &data);
        }
        self.next_fetch()
    }

    fn on_selection_notify(&mut self, e: SelectionNotifyEvent) -> XResult<()> {
        if e.requestor != self.win || self.fetch.is_none() {
            return Ok(());
        }
        if e.property == x11rb::NONE {
            let f = self.fetch.as_mut().unwrap();
            if f.current.as_deref() == Some("TARGETS") && !f.targets_known {
                // Appli sans TARGETS : on tente le texte.
                f.targets_known = true;
                f.queue = ["UTF8_STRING", "STRING"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
            }
            return self.next_fetch();
        }
        let reply = self
            .conn
            .get_property(true, self.win, self.prop, AtomEnum::ANY, 0, u32::MAX / 4)?
            .reply()?;
        if reply.type_ == self.incr {
            // Les morceaux arrivent par PropertyNotify (la suppression ci-dessus lance l'envoi).
            if let Some(f) = self.fetch.as_mut() {
                f.incr = Some(Vec::new());
            }
            return Ok(());
        }
        self.got_data(reply.value)
    }

    fn on_incr_piece(&mut self) -> XResult<()> {
        if self.fetch.as_ref().is_none_or(|f| f.incr.is_none()) {
            return Ok(());
        }
        let reply = self
            .conn
            .get_property(true, self.win, self.prop, AtomEnum::ANY, 0, u32::MAX / 4)?
            .reply()?;
        let f = self.fetch.as_mut().unwrap();
        let buf = f.incr.as_mut().unwrap();
        if reply.value.is_empty() {
            let data = std::mem::take(buf);
            f.incr = None;
            return self.got_data(data);
        }
        if buf.len() + reply.value.len() > 256 * 1024 * 1024 {
            self.fetch = None;
            return Ok(());
        }
        buf.extend_from_slice(&reply.value);
        Ok(())
    }

    /// Tout est reçu : écrire le contenu Windows équivalent.
    fn commit(&mut self) {
        let Some(f) = self.fetch.take() else {
            return;
        };
        if f.clip.is_empty() {
            return;
        }
        let (mut contents, names) = import(&f.clip, crate::paths::to_windows);
        if contents.formats.is_empty() {
            return;
        }
        contents.seq = cb::read_seq(&self.path)
            .max(self.seen_seq)
            .wrapping_add(1)
            .max(1);
        if cb::store(&self.path, &contents, &names) {
            self.seen_seq = contents.seq;
            eprintln!(
                "winlux: presse-papiers : copie Linux importée (n°{}, {} format(s))",
                contents.seq,
                contents.formats.len()
            );
        }
    }

    fn handle(&mut self, ev: Event) -> XResult<()> {
        match ev {
            Event::XfixesSelectionNotify(e) if e.selection == self.clipboard => {
                if e.owner != x11rb::NONE && e.owner != self.win {
                    self.start_fetch(e.selection_timestamp)?;
                }
            }
            Event::SelectionRequest(e) => self.on_request(e)?,
            Event::SelectionClear(e) if e.selection == self.clipboard => self.own = None,
            Event::SelectionNotify(e) => self.on_selection_notify(e)?,
            Event::PropertyNotify(e) => {
                if e.window == self.win && e.atom == self.prop && e.state == Property::NEW_VALUE {
                    self.on_incr_piece()?;
                } else if e.state == Property::DELETE {
                    self.on_incr_sent(e.window, e.atom)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn tick(&mut self) -> XResult<()> {
        if self
            .fetch
            .as_ref()
            .is_some_and(|f| f.started.elapsed() > Duration::from_secs(5))
        {
            // Appli Linux muette : on garde ce qui est déjà arrivé.
            if let Some(f) = self.fetch.as_mut() {
                f.queue.clear();
                f.current = None;
            }
            self.commit();
        }
        self.outgoing
            .retain(|_, o| o.started.elapsed() < Duration::from_secs(30));
        self.export()
    }
}

fn wait_fd(conn: &RustConnection, ms: i32) {
    use std::os::fd::AsRawFd;
    let mut pfd = libc::pollfd {
        fd: conn.stream().as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe {
        libc::poll(&mut pfd, 1, ms);
    }
}

fn connect() -> XResult<Bridge> {
    let (conn, screen) = RustConnection::connect(None)?;
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id()?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        root,
        -10,
        -10,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?;
    let intern =
        |n: &str| -> XResult<Atom> { Ok(conn.intern_atom(false, n.as_bytes())?.reply()?.atom) };
    let max = conn.maximum_request_bytes();
    let mut b = Bridge {
        clipboard: intern("CLIPBOARD")?,
        targets: intern("TARGETS")?,
        timestamp: intern("TIMESTAMP")?,
        incr: intern("INCR")?,
        utf8: intern("UTF8_STRING")?,
        prop: intern("WINLUX_CLIPBOARD_DATA")?,
        stamp: intern("WINLUX_CLIPBOARD_TIME")?,
        conn,
        win,
        atoms: HashMap::new(),
        names: HashMap::new(),
        path: cb::shared_path(),
        seen_seq: 0,
        own: None,
        fetch: None,
        outgoing: HashMap::new(),
        backlog: VecDeque::new(),
        chunk: (max.saturating_sub(1024)).clamp(4096, 256 * 1024),
    };
    let lock = b.atom("WINLUX_CLIPBOARD_BRIDGE")?;
    if b.conn.get_selection_owner(lock)?.reply()?.owner != x11rb::NONE {
        return Err("un pont du presse-papiers tourne déjà sur cet écran".into());
    }
    b.conn.set_selection_owner(win, lock, CURRENT_TIME)?;
    if b.conn
        .extension_information(x11rb::protocol::xfixes::X11_EXTENSION_NAME)?
        .is_none()
    {
        return Err("extension XFixes absente du serveur X".into());
    }
    b.conn.xfixes_query_version(5, 0)?.reply()?;
    b.conn.xfixes_select_selection_input(
        win,
        b.clipboard,
        SelectionEventMask::SET_SELECTION_OWNER
            | SelectionEventMask::SELECTION_WINDOW_DESTROY
            | SelectionEventMask::SELECTION_CLIENT_CLOSE,
    )?;
    b.conn.flush()?;
    Ok(b)
}

/// `winlux --clipboard-bridge` : tourne jusqu'à la fin de la session X.
pub fn run() -> i32 {
    let mut b = match connect() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("winlux: presse-papiers Linux non relié : {e}");
            return 1;
        }
    };
    eprintln!(
        "winlux: presse-papiers Windows ↔ Linux relié ({})",
        b.path.display()
    );
    // Au démarrage, la copie Linux en cours gagne ; sinon on publie celle de Windows.
    let owner = b
        .conn
        .get_selection_owner(b.clipboard)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.owner)
        .unwrap_or(x11rb::NONE);
    if owner != x11rb::NONE {
        b.seen_seq = cb::read_seq(&b.path);
        let _ = b.start_fetch(CURRENT_TIME);
    }
    loop {
        let step = (|| -> XResult<()> {
            while let Some(ev) = b.backlog.pop_front() {
                b.handle(ev)?;
            }
            while let Some(ev) = b.conn.poll_for_event()? {
                b.handle(ev)?;
            }
            b.tick()?;
            b.conn.flush()?;
            Ok(())
        })();
        if let Err(e) = step {
            // Connexion X perdue : fin de session.
            if b.conn.poll_for_event().is_err() {
                eprintln!("winlux: serveur X fermé, arrêt du pont : {e}");
                return 0;
            }
            eprintln!("winlux: presse-papiers : {e}");
        }
        wait_fd(&b.conn, 200);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texte_crlf_aller_retour() {
        assert_eq!(to_crlf("a\nb\r\nc"), "a\r\nb\r\nc");
        assert_eq!(to_lf("a\r\nb"), "a\nb");
    }

    #[test]
    fn hdrop_utf16_et_ansi() {
        let paths = vec![
            "C:\\Documents\\été.txt".to_string(),
            "Z:\\tmp\\a b".to_string(),
        ];
        assert_eq!(hdrop_parse(&hdrop_build(&paths)), paths);
        let mut ansi = vec![0u8; 20];
        ansi[0] = 20;
        ansi.extend_from_slice(b"C:\\x.txt\0D:\\y\0\0");
        assert_eq!(hdrop_parse(&ansi), vec!["C:\\x.txt", "D:\\y"]);
    }

    #[test]
    fn uri_list_thunar_et_nautilus() {
        let p = Path::new("/home/d/Mes documents/été #1.txt");
        let uri = file_uri(p);
        assert_eq!(
            uri,
            "file:///home/d/Mes%20documents/%C3%A9t%C3%A9%20%231.txt"
        );
        let (files, cut) = parse_uri_list(&format!(
            "cut\n{uri}\nfile://localhost/etc/hosts\r\n# c\nhttps://x/y"
        ));
        assert!(cut);
        assert_eq!(files, vec![p.to_path_buf(), PathBuf::from("/etc/hosts")]);
        let (files, cut) = parse_uri_list("file:///tmp/a%2");
        assert!(!cut);
        assert_eq!(files, vec![PathBuf::from("/tmp/a%2")]);
    }

    #[test]
    fn dib_32_24_8_bits() {
        let rgba: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 9, 9, 9, 255];
        let dib = rgba_to_dib(2, 2, &rgba);
        assert_eq!(dib_to_rgba(&dib).unwrap(), (2, 2, rgba.clone()));
        // 24 bits de bas en haut, lignes alignées sur 4 octets.
        let mut d24 = rgba_to_dib(1, 2, &[0; 8])[..40].to_vec();
        d24[14] = 24;
        d24.extend_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(
            dib_to_rgba(&d24).unwrap().2,
            vec![6, 5, 4, 255, 3, 2, 1, 255]
        );
        // 8 bits avec palette de deux couleurs.
        let mut d8 = rgba_to_dib(2, 1, &[0; 8])[..40].to_vec();
        d8[14] = 8;
        d8[32] = 2;
        d8.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(
            dib_to_rgba(&d8).unwrap().2,
            vec![0, 0, 255, 255, 255, 0, 0, 255]
        );
        // 32 bits sans alpha : opaque.
        let mut opaque = rgba_to_dib(1, 1, &[1, 2, 3, 0]);
        opaque[43] = 0;
        assert_eq!(dib_to_rgba(&opaque).unwrap().2, vec![1, 2, 3, 255]);
        let bmp = dib_to_bmp_file(&dib).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 54);
    }

    #[test]
    fn html_windows_aller_retour() {
        let frag = "<b>Gras</b> & <i>été</i>";
        let cf = cf_html_wrap(frag);
        assert_eq!(cf_html_fragment(&cf).as_deref(), Some(frag));
        let text = String::from_utf8_lossy(&cf);
        let start: usize = text["Version:0.9\r\nStartHTML:".len()..][..10]
            .parse()
            .unwrap();
        assert!(text[start..].starts_with("<html>"));
    }

    #[test]
    fn texte_x11_utf8_latin1_utf16() {
        assert_eq!(decode_x_text("été\0".as_bytes(), false), "été");
        assert_eq!(decode_x_text(&[0xE9], true), "é");
        assert_eq!(decode_x_text(&[0xFF, 0xFE, b'h', 0, b'i', 0], false), "hi");
    }

    #[test]
    fn copie_linux_devient_formats_windows() {
        let png = png_encode(1, 1, &[10, 20, 30, 255]).unwrap();
        let clip = LinuxClip {
            text: Some("ligne 1\nligne 2".into()),
            html: Some("<p>x</p>".into()),
            image: Some((png.clone(), "image/png".into())),
            files: vec![PathBuf::from("/tmp/a.txt")],
            cut: true,
        };
        let (c, names) = import(&clip, |p| {
            format!("Z:{}", p.display().to_string().replace('/', "\\"))
        });
        assert_eq!(c.text().as_deref(), Some("ligne 1\r\nligne 2"));
        assert_eq!(
            hdrop_parse(c.get(CF_HDROP).unwrap()),
            vec!["Z:\\tmp\\a.txt"]
        );
        let effect = cb::registered_id(FORMAT_DROPEFFECT);
        assert_eq!(c.get(effect), Some(&DROPEFFECT_MOVE.to_le_bytes()[..]));
        assert_eq!(
            names.get(&effect).map(String::as_str),
            Some(FORMAT_DROPEFFECT)
        );
        assert_eq!(
            dib_to_rgba(c.get(CF_DIB).unwrap()).unwrap().2,
            vec![10, 20, 30, 255]
        );
        assert_eq!(c.get(cb::registered_id(FORMAT_PNG)), Some(&png[..]));
        // Et retour vers Linux, avec le même contenu.
        let back = |t: &str| {
            export_render(&c, t, |w| {
                PathBuf::from(w.replace("Z:", "").replace('\\', "/"))
            })
        };
        assert_eq!(back("UTF8_STRING").unwrap(), b"ligne 1\nligne 2");
        assert_eq!(
            back("x-special/gnome-copied-files").unwrap(),
            b"cut\nfile:///tmp/a.txt"
        );
        assert_eq!(back("text/uri-list").unwrap(), b"file:///tmp/a.txt\r\n");
        assert_eq!(back("text/html").unwrap(), b"<p>x</p>");
        assert_eq!(back("image/png").unwrap(), png);
        let targets = export_targets(&c);
        for t in [
            "text/uri-list",
            "image/png",
            "image/bmp",
            "text/html",
            "UTF8_STRING",
        ] {
            assert!(targets.contains(&t), "{t}");
        }
    }

    #[test]
    fn plan_import_prefere_fichiers_puis_image_puis_texte() {
        let offered: Vec<String> = [
            "TARGETS",
            "text/plain",
            "UTF8_STRING",
            "image/jpeg",
            "image/png",
            "text/uri-list",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            import_plan(&offered),
            vec!["text/uri-list", "image/png", "UTF8_STRING"]
        );
        let mut clip = LinuxClip::default();
        absorb(&mut clip, "text/uri-list", b"https://example.org/\n");
        assert!(clip.files.is_empty());
        absorb(&mut clip, "STRING", &[0x63, 0x61, 0x66, 0xE9]);
        assert_eq!(clip.text.as_deref(), Some("café"));
    }

    #[test]
    fn texte_ansi_windows_exporte_en_utf8() {
        let c = Contents {
            seq: 1,
            formats: vec![(CF_TEXT, b"abc\r\n\0".to_vec())],
        };
        assert_eq!(
            export_render(&c, "UTF8_STRING", |w: &str| PathBuf::from(w)).unwrap(),
            b"abc\n"
        );
        assert_eq!(
            export_render(&c, "image/png", |w: &str| PathBuf::from(w)),
            None
        );
        assert_eq!(export_targets(&c), TEXT_TARGETS.to_vec());
    }
}
