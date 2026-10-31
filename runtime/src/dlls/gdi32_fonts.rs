//! Polices pour les .exe : `EnumFontFamilies(Ex)`, `EnumFonts`,
//! `AddFontResource(Ex)` et `RemoveFontResource(Ex)`.
//!
//! Le catalogue est construit maison : les fichiers TrueType/OpenType
//! (`.ttf`, `.otf`, `.ttc`, `.otc`) des dossiers de polices Linux (ceux que
//! fontconfig indexe, y compris le dossier utilisateur où le gestionnaire de
//! polices de Winlux installe les siennes) et de `C:\Windows\Fonts` sont lus
//! directement : tables `name`, `OS/2`, `head`, `hhea` et `post`. Aucune
//! dépendance n'est ajoutée ; seules les polices ajoutées par
//! `AddFontResource` sont signalées à fontconfig (s'il est présent) pour que
//! Xft puisse les dessiner.

use super::{cstr, lock, wstr};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Une face de police (un style d'une famille).
#[derive(Clone, Debug, PartialEq)]
pub struct FontFace {
    pub family: String,
    pub style: String,
    pub full_name: String,
    pub weight: u16,
    pub italic: bool,
    pub bold: bool,
    pub fixed_pitch: bool,
    pub cff: bool,
    pub units_per_em: u16,
    pub ascent: i16,
    pub descent: i16,
    pub avg_width: i16,
    pub unicode_ranges: [u32; 4],
    pub code_pages: [u32; 2],
    pub path: PathBuf,
}

// ---------------------------------------------------------------------
// Lecture des fichiers sfnt
// ---------------------------------------------------------------------

/// Source d'octets lue par morceaux (fichier sur disque ou tampon).
trait Source {
    fn read(&self, offset: u64, len: usize) -> Option<Vec<u8>>;
}

impl Source for [u8] {
    fn read(&self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(offset).ok()?;
        self.get(start..start.checked_add(len)?).map(<[u8]>::to_vec)
    }
}

impl Source for std::fs::File {
    fn read(&self, offset: u64, len: usize) -> Option<Vec<u8>> {
        use std::os::unix::fs::FileExt;
        let mut buf = vec![0; len];
        self.read_exact_at(&mut buf, offset).ok()?;
        Some(buf)
    }
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}

/// Limite de taille d'une table lue (la table `name` des polices CJK reste
/// bien en dessous).
const MAX_TABLE: u32 = 1 << 20;

/// Analyse un fichier de police complet (une ou plusieurs faces).
fn parse_source(src: &(impl Source + ?Sized), path: &Path) -> Vec<FontFace> {
    let Some(head) = src.read(0, 12) else {
        return Vec::new();
    };
    if &head[..4] == b"ttcf" {
        let count = be32(&head, 8).unwrap_or(0).min(64) as usize;
        let Some(offsets) = src.read(12, count * 4) else {
            return Vec::new();
        };
        (0..count)
            .filter_map(|i| be32(&offsets, i * 4))
            .filter_map(|off| parse_face(src, off as u64, path))
            .collect()
    } else {
        parse_face(src, 0, path).into_iter().collect()
    }
}

/// Analyse les octets d'une police en mémoire (utilisé par les tests).
#[cfg(test)]
pub fn parse_font_bytes(data: &[u8], path: &Path) -> Vec<FontFace> {
    parse_source(data, path)
}

/// Analyse un fichier de police sur disque.
pub fn parse_font_file(path: &Path) -> Vec<FontFace> {
    match std::fs::File::open(path) {
        Ok(f) => parse_source(&f, path),
        Err(_) => Vec::new(),
    }
}

fn parse_face(src: &(impl Source + ?Sized), base: u64, path: &Path) -> Option<FontFace> {
    let header = src.read(base, 12)?;
    let version = be32(&header, 0)?;
    let cff = version == u32::from_be_bytes(*b"OTTO");
    if !(cff || version == 0x0001_0000 || version == u32::from_be_bytes(*b"true")) {
        return None;
    }
    let count = be16(&header, 4)? as usize;
    let records = src.read(base + 12, count.min(256) * 16)?;
    let table = |tag: &[u8; 4]| -> Option<Vec<u8>> {
        (0..count.min(256)).find_map(|i| {
            let r = &records[i * 16..i * 16 + 16];
            if &r[..4] != tag {
                return None;
            }
            let off = be32(r, 8)?;
            let len = be32(r, 12)?.min(MAX_TABLE);
            src.read(off as u64, len as usize)
        })
    };
    let names = table(b"name")?;
    let family = name_string(&names, 1)?;
    let style = name_string(&names, 2).unwrap_or_else(|| "Regular".into());
    let full_name = name_string(&names, 4).unwrap_or_else(|| {
        if style.eq_ignore_ascii_case("regular") {
            family.clone()
        } else {
            format!("{family} {style}")
        }
    });
    let mut face = FontFace {
        family,
        style,
        full_name,
        weight: 400,
        italic: false,
        bold: false,
        fixed_pitch: false,
        cff,
        units_per_em: 2048,
        ascent: 1638,
        descent: -410,
        avg_width: 900,
        unicode_ranges: [1, 0, 0, 0],
        code_pages: [1, 0],
        path: path.to_path_buf(),
    };
    if let Some(os2) = table(b"OS/2") {
        face.avg_width = be16(&os2, 2).map_or(face.avg_width, |v| v as i16);
        face.weight = be16(&os2, 4).filter(|w| *w > 0).unwrap_or(400);
        if let Some(sel) = be16(&os2, 62) {
            face.italic = sel & 0x01 != 0;
            face.bold = sel & 0x20 != 0;
        }
        for (i, slot) in face.unicode_ranges.iter_mut().enumerate() {
            if let Some(v) = be32(&os2, 42 + i * 4) {
                *slot = v;
            }
        }
        if be16(&os2, 0).unwrap_or(0) >= 1 {
            for (i, slot) in face.code_pages.iter_mut().enumerate() {
                if let Some(v) = be32(&os2, 78 + i * 4) {
                    *slot = v;
                }
            }
        }
    } else {
        let lower = face.style.to_ascii_lowercase();
        face.italic = lower.contains("italic") || lower.contains("oblique");
        face.bold = lower.contains("bold");
        face.weight = if face.bold { 700 } else { 400 };
    }
    if let Some(head) = table(b"head") {
        face.units_per_em = be16(&head, 18).filter(|u| *u > 0).unwrap_or(2048);
    }
    if let Some(hhea) = table(b"hhea") {
        face.ascent = be16(&hhea, 4).map_or(face.ascent, |v| v as i16);
        face.descent = be16(&hhea, 6).map_or(face.descent, |v| v as i16);
    }
    if let Some(post) = table(b"post") {
        face.fixed_pitch = be32(&post, 12).unwrap_or(0) != 0;
    }
    Some(face)
}

/// Chaîne `name_id` de la table `name`, en préférant l'anglais Windows.
fn name_string(table: &[u8], name_id: u16) -> Option<String> {
    let count = be16(table, 2)? as usize;
    let strings = be16(table, 4)? as usize;
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let r = 6 + i * 12;
        let (Some(platform), Some(encoding), Some(lang), Some(id), Some(len), Some(off)) = (
            be16(table, r),
            be16(table, r + 2),
            be16(table, r + 4),
            be16(table, r + 6),
            be16(table, r + 8),
            be16(table, r + 10),
        ) else {
            break;
        };
        if id != name_id {
            continue;
        }
        let start = strings + off as usize;
        let Some(raw) = table.get(start..start + len as usize) else {
            continue;
        };
        let (rank, text) = match (platform, encoding) {
            (3, 1 | 10) | (0, _) => {
                let units: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes(*c))
                    .collect();
                let rank = if platform == 3 && lang == 0x0409 {
                    0
                } else if platform == 3 {
                    1
                } else {
                    2
                };
                (rank, String::from_utf16_lossy(&units))
            }
            (1, 0) => (3, raw.iter().map(|&b| b as char).collect()),
            _ => continue,
        };
        let text = text.trim_matches(char::from(0)).trim().to_string();
        if text.is_empty() {
            continue;
        }
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, s)| s)
}

// ---------------------------------------------------------------------
// Catalogue
// ---------------------------------------------------------------------

fn is_font_file(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "ttf" | "otf" | "ttc" | "otc"
        )
    })
}

/// Dossiers de polices parcourus, dans l'ordre de fontconfig.
pub fn font_dirs() -> Vec<PathBuf> {
    if let Some(list) = std::env::var_os("WINLUX_FONT_DIRS") {
        return std::env::split_paths(&list).collect();
    }
    let mut dirs = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match std::env::var_os("XDG_DATA_HOME") {
        Some(d) if !d.is_empty() => dirs.push(PathBuf::from(d).join("fonts")),
        _ => {
            if let Some(h) = &home {
                dirs.push(h.join(".local/share/fonts"));
            }
        }
    }
    if let Some(h) = &home {
        dirs.push(h.join(".fonts"));
    }
    dirs.extend(windows_font_dirs());
    dirs
}

/// Dossiers de polices côté Windows : `C:\Windows\Fonts` (installateurs,
/// msiexec) et le dossier par utilisateur de Windows 10+
/// (`%LOCALAPPDATA%\Microsoft\Windows\Fonts`). La casse réelle du disque
/// est respectée (`Windows` ou `windows`).
pub fn windows_font_dirs() -> Vec<PathBuf> {
    vec![
        windows_fonts_dir(),
        crate::paths::to_linux("C:\\AppData\\Local\\Microsoft\\Windows\\Fonts"),
    ]
}

/// `C:\Windows\Fonts` sur le disque Linux.
pub fn windows_fonts_dir() -> PathBuf {
    crate::paths::to_linux("C:\\Windows\\Fonts")
}

fn scan_dir(dir: &Path, depth: u32, out: &mut Vec<FontFace>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            if depth < 6 {
                scan_dir(&p, depth + 1, out);
            }
        } else if is_font_file(&p) {
            out.extend(parse_font_file(&p));
        }
    }
}

/// Parcourt les dossiers donnés et renvoie toutes les faces trouvées.
pub fn scan(dirs: &[PathBuf]) -> Vec<FontFace> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for d in dirs {
        let canon = std::fs::canonicalize(d).unwrap_or_else(|_| d.clone());
        if seen.contains(&canon) {
            continue;
        }
        seen.push(canon);
        scan_dir(d, 0, &mut out);
    }
    out
}

fn system_cache() -> &'static Mutex<Option<Vec<FontFace>>> {
    static CACHE: OnceLock<Mutex<Option<Vec<FontFace>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn added() -> &'static Mutex<Vec<FontFace>> {
    static ADDED: OnceLock<Mutex<Vec<FontFace>>> = OnceLock::new();
    ADDED.get_or_init(|| Mutex::new(Vec::new()))
}

/// Toutes les faces visibles par le processus (système puis ajoutées).
pub fn all_faces() -> Vec<FontFace> {
    let mut faces = lock(system_cache())
        .get_or_insert_with(|| scan(&font_dirs()))
        .clone();
    faces.extend(lock(added()).iter().cloned());
    faces
}

/// Une face par famille (de préférence le style normal), triées par nom.
pub fn families(faces: &[FontFace]) -> Vec<FontFace> {
    let mut out: Vec<FontFace> = Vec::new();
    for f in faces {
        match out
            .iter_mut()
            .find(|o| o.family.eq_ignore_ascii_case(&f.family))
        {
            Some(o) => {
                let score = |x: &FontFace| (x.italic as u32) * 2000 + x.weight.abs_diff(400) as u32;
                if score(f) < score(o) {
                    *o = f.clone();
                }
            }
            None => out.push(f.clone()),
        }
    }
    out.sort_by_key(|f| f.family.to_lowercase());
    out
}

/// Faces à énumérer : une par famille si `face` est vide, sinon tous les
/// styles de la famille demandée.
pub fn select(faces: &[FontFace], face: &str) -> Vec<FontFace> {
    if face.is_empty() {
        return families(faces);
    }
    let mut out: Vec<FontFace> = faces
        .iter()
        .filter(|f| f.family.eq_ignore_ascii_case(face))
        .cloned()
        .collect();
    out.dedup_by(|a, b| a.style == b.style && a.family == b.family);
    out
}

// ---------------------------------------------------------------------
// Structures Win32
// ---------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LogFontHeader {
    pub height: i32,
    pub width: i32,
    pub escapement: i32,
    pub orientation: i32,
    pub weight: i32,
    pub italic: u8,
    pub underline: u8,
    pub strikeout: u8,
    pub charset: u8,
    pub out_precision: u8,
    pub clip_precision: u8,
    pub quality: u8,
    pub pitch_and_family: u8,
}

#[repr(C)]
pub struct EnumLogFontExA {
    pub lf: LogFontHeader,
    pub face: [u8; 32],
    pub full_name: [u8; 64],
    pub style: [u8; 32],
    pub script: [u8; 32],
}

#[repr(C)]
pub struct EnumLogFontExW {
    pub lf: LogFontHeader,
    pub face: [u16; 32],
    pub full_name: [u16; 64],
    pub style: [u16; 32],
    pub script: [u16; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Metrics {
    pub height: i32,
    pub ascent: i32,
    pub descent: i32,
    pub internal_leading: i32,
    pub external_leading: i32,
    pub ave_char_width: i32,
    pub max_char_width: i32,
    pub weight: i32,
    pub overhang: i32,
    pub digitized_aspect_x: i32,
    pub digitized_aspect_y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Tail {
    pub italic: u8,
    pub underlined: u8,
    pub struck_out: u8,
    pub pitch_and_family: u8,
    pub charset: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NtmExtra {
    pub flags: u32,
    pub size_em: u32,
    pub cell_height: u32,
    pub avg_width: u32,
    pub usb: [u32; 4],
    pub csb: [u32; 2],
}

#[repr(C)]
pub struct NewTextMetricExA {
    pub m: Metrics,
    pub chars: [u8; 4],
    pub tail: Tail,
    pub extra: NtmExtra,
}

#[repr(C)]
pub struct NewTextMetricExW {
    pub m: Metrics,
    pub chars: [u16; 4],
    pub tail: Tail,
    pub extra: NtmExtra,
}

pub const TRUETYPE_FONTTYPE: u32 = 4;
const DEFAULT_CHARSET: u8 = 1;
const NTM_ITALIC: u32 = 0x1;
const NTM_BOLD: u32 = 0x20;
const NTM_REGULAR: u32 = 0x40;
const NTM_PS_OPENTYPE: u32 = 0x2_0000;
const NTM_TT_OPENTYPE: u32 = 0x4_0000;
/// Hauteur de cellule (pixels) à laquelle les métriques sont rapportées.
const ENUM_CELL: i32 = 16;

fn header_for(f: &FontFace, charset: u8) -> LogFontHeader {
    let fixed = f.fixed_pitch;
    LogFontHeader {
        height: ENUM_CELL,
        width: 0,
        escapement: 0,
        orientation: 0,
        weight: f.weight as i32,
        italic: f.italic as u8,
        underline: 0,
        strikeout: 0,
        charset,
        out_precision: 3,  // OUT_STROKE_PRECIS
        clip_precision: 2, // CLIP_STROKE_PRECIS
        quality: 1,        // DRAFT_QUALITY
        pitch_and_family: if fixed { 0x31 } else { 0x22 },
    }
}

fn metrics_for(f: &FontFace) -> (Metrics, Tail, NtmExtra) {
    let cell_units = (f.ascent as i32 - f.descent.min(0) as i32).max(1);
    let scale = |v: i32| (v * ENUM_CELL + cell_units / 2) / cell_units;
    let ascent = scale(f.ascent as i32);
    let internal = ENUM_CELL - scale(f.units_per_em as i32).min(ENUM_CELL);
    let avg = scale(f.avg_width as i32).max(1);
    let m = Metrics {
        height: ENUM_CELL,
        ascent,
        descent: ENUM_CELL - ascent,
        internal_leading: internal.max(0),
        external_leading: 0,
        ave_char_width: avg,
        max_char_width: if f.fixed_pitch { avg } else { avg * 2 },
        weight: f.weight as i32,
        overhang: 0,
        digitized_aspect_x: 96,
        digitized_aspect_y: 96,
    };
    // tmPitchAndFamily : le bit 0 vaut 1 pour une police NON fixe (sic),
    // 0x02 vectorielle, 0x04 TrueType.
    let family = if f.fixed_pitch { 0x30 } else { 0x20 };
    let tail = Tail {
        italic: if f.italic { 0xFF } else { 0 },
        underlined: 0,
        struck_out: 0,
        pitch_and_family: family | 0x06 | (!f.fixed_pitch) as u8,
        charset: 0,
    };
    let mut flags = if f.cff {
        NTM_PS_OPENTYPE
    } else {
        NTM_TT_OPENTYPE
    };
    if f.italic {
        flags |= NTM_ITALIC;
    }
    if f.bold || f.weight >= 600 {
        flags |= NTM_BOLD;
    }
    if !f.italic && !(f.bold || f.weight >= 600) {
        flags |= NTM_REGULAR;
    }
    let extra = NtmExtra {
        flags,
        size_em: f.units_per_em as u32,
        cell_height: cell_units as u32,
        avg_width: f.avg_width.max(0) as u32,
        usb: f.unicode_ranges,
        csb: f.code_pages,
    };
    (m, tail, extra)
}

fn fill_a(dst: &mut [u8], s: &str) {
    let b = super::encode_ansi(s);
    let n = b.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&b[..n]);
}
fn fill_w(dst: &mut [u16], s: &str) {
    let cap = dst.len() - 1;
    for (slot, u) in dst.iter_mut().take(cap).zip(s.encode_utf16()) {
        *slot = u;
    }
}

pub fn enum_entry_a(f: &FontFace, charset: u8) -> (EnumLogFontExA, NewTextMetricExA) {
    let mut lf = EnumLogFontExA {
        lf: header_for(f, charset),
        face: [0; 32],
        full_name: [0; 64],
        style: [0; 32],
        script: [0; 32],
    };
    fill_a(&mut lf.face, &f.family);
    fill_a(&mut lf.full_name, &f.full_name);
    fill_a(&mut lf.style, &f.style);
    fill_a(&mut lf.script, "Western");
    let (m, mut tail, extra) = metrics_for(f);
    tail.charset = charset;
    let tm = NewTextMetricExA {
        m,
        chars: [0x20, 0xFF, 0x1F, 0x20],
        tail,
        extra,
    };
    (lf, tm)
}

pub fn enum_entry_w(f: &FontFace, charset: u8) -> (EnumLogFontExW, NewTextMetricExW) {
    let mut lf = EnumLogFontExW {
        lf: header_for(f, charset),
        face: [0; 32],
        full_name: [0; 64],
        style: [0; 32],
        script: [0; 32],
    };
    fill_w(&mut lf.face, &f.family);
    fill_w(&mut lf.full_name, &f.full_name);
    fill_w(&mut lf.style, &f.style);
    fill_w(&mut lf.script, "Western");
    let (m, mut tail, extra) = metrics_for(f);
    tail.charset = charset;
    let tm = NewTextMetricExW {
        m,
        chars: [0x20, 0xFFFC, 0x1F, 0x20],
        tail,
        extra,
    };
    (lf, tm)
}

// ---------------------------------------------------------------------
// API exportées
// ---------------------------------------------------------------------

type EnumProc = unsafe extern "win64" fn(*const c_void, *const c_void, u32, isize) -> i32;

unsafe fn run_a(face: &str, charset: u8, callback: usize, lparam: isize) -> i32 {
    if callback == 0 {
        return 0;
    }
    let cb: EnumProc = std::mem::transmute(callback);
    let mut last = 1;
    for f in select(&all_faces(), face) {
        let (lf, tm) = enum_entry_a(&f, charset);
        last = cb(
            (&lf as *const EnumLogFontExA).cast(),
            (&tm as *const NewTextMetricExA).cast(),
            TRUETYPE_FONTTYPE,
            lparam,
        );
        if last == 0 {
            break;
        }
    }
    last
}

unsafe fn run_w(face: &str, charset: u8, callback: usize, lparam: isize) -> i32 {
    if callback == 0 {
        return 0;
    }
    let cb: EnumProc = std::mem::transmute(callback);
    let mut last = 1;
    for f in select(&all_faces(), face) {
        let (lf, tm) = enum_entry_w(&f, charset);
        last = cb(
            (&lf as *const EnumLogFontExW).cast(),
            (&tm as *const NewTextMetricExW).cast(),
            TRUETYPE_FONTTYPE,
            lparam,
        );
        if last == 0 {
            break;
        }
    }
    last
}

fn charset_of(requested: u8) -> u8 {
    if requested == DEFAULT_CHARSET {
        0
    } else {
        requested
    }
}

pub unsafe extern "win64" fn enum_font_families_ex_a(
    _hdc: usize,
    logfont: *const c_void,
    callback: usize,
    lparam: isize,
    _flags: u32,
) -> i32 {
    let (face, charset) = if logfont.is_null() {
        (String::new(), 0)
    } else {
        let head = &*(logfont as *const LogFontHeader);
        let name = (logfont as *const u8).add(std::mem::size_of::<LogFontHeader>());
        (cstr(name), charset_of(head.charset))
    };
    run_a(&face, charset, callback, lparam)
}

pub unsafe extern "win64" fn enum_font_families_ex_w(
    _hdc: usize,
    logfont: *const c_void,
    callback: usize,
    lparam: isize,
    _flags: u32,
) -> i32 {
    let (face, charset) = if logfont.is_null() {
        (String::new(), 0)
    } else {
        let head = &*(logfont as *const LogFontHeader);
        let name = (logfont as *const u8).add(std::mem::size_of::<LogFontHeader>());
        (wstr(name.cast()), charset_of(head.charset))
    };
    run_w(&face, charset, callback, lparam)
}

pub unsafe extern "win64" fn enum_font_families_a(
    _hdc: usize,
    family: *const u8,
    callback: usize,
    lparam: isize,
) -> i32 {
    run_a(&cstr(family), 0, callback, lparam)
}

pub unsafe extern "win64" fn enum_font_families_w(
    _hdc: usize,
    family: *const u16,
    callback: usize,
    lparam: isize,
) -> i32 {
    run_w(&wstr(family), 0, callback, lparam)
}

// ---- AddFontResource / RemoveFontResource ----

/// Résout le nom passé à `AddFontResource` : chemin Windows ou Linux, ou
/// simple nom de fichier cherché dans `C:\Windows\Fonts`.
pub fn resolve_font_path(name: &str) -> PathBuf {
    let direct = crate::paths::to_linux(name);
    if direct.exists() || name.contains(['\\', '/', ':']) {
        return direct;
    }
    let fonts = windows_fonts_dir().join(name);
    if fonts.exists() {
        fonts
    } else {
        direct
    }
}

/// Ajoute les faces d'un fichier ; renvoie leur nombre (0 si échec).
pub fn add_font_file(path: &Path) -> usize {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let faces = parse_font_file(&canon);
    if faces.is_empty() {
        return 0;
    }
    let n = faces.len();
    let mut list = lock(added());
    list.retain(|f| f.path != canon);
    list.extend(faces);
    drop(list);
    fontconfig_add(&canon);
    n
}

/// Retire les faces ajoutées depuis ce fichier ; vrai si une face existait.
pub fn remove_font_file(path: &Path) -> bool {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut list = lock(added());
    let before = list.len();
    list.retain(|f| f.path != canon);
    let removed = list.len() != before;
    let remaining: Vec<PathBuf> = list.iter().map(|f| f.path.clone()).collect();
    drop(list);
    if removed {
        fontconfig_reset(&remaining);
    }
    removed
}

struct Fontconfig {
    add: unsafe extern "C" fn(*mut c_void, *const u8) -> i32,
    clear: unsafe extern "C" fn(*mut c_void),
}

fn fontconfig() -> Option<&'static Fontconfig> {
    static FC: OnceLock<Option<Fontconfig>> = OnceLock::new();
    FC.get_or_init(|| unsafe {
        let lib = libc::dlopen(
            c"libfontconfig.so.1".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_LOCAL,
        );
        if lib.is_null() {
            return None;
        }
        let add = libc::dlsym(lib, c"FcConfigAppFontAddFile".as_ptr());
        let clear = libc::dlsym(lib, c"FcConfigAppFontClear".as_ptr());
        if add.is_null() || clear.is_null() {
            return None;
        }
        Some(Fontconfig {
            add: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(*mut c_void, *const u8) -> i32,
            >(add),
            clear: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void)>(clear),
        })
    })
    .as_ref()
}

fn fontconfig_add(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let Some(fc) = fontconfig() else { return };
    let mut bytes = path.as_os_str().as_bytes().to_vec();
    bytes.push(0);
    unsafe {
        (fc.add)(std::ptr::null_mut(), bytes.as_ptr());
    }
}

fn fontconfig_reset(remaining: &[PathBuf]) {
    let Some(fc) = fontconfig() else { return };
    unsafe { (fc.clear)(std::ptr::null_mut()) };
    let mut done: Vec<&PathBuf> = Vec::new();
    for p in remaining {
        if !done.contains(&p) {
            fontconfig_add(p);
            done.push(p);
        }
    }
}

pub unsafe extern "win64" fn add_font_resource_a(name: *const u8) -> i32 {
    add_font_file(&resolve_font_path(&cstr(name))) as i32
}
pub unsafe extern "win64" fn add_font_resource_w(name: *const u16) -> i32 {
    add_font_file(&resolve_font_path(&wstr(name))) as i32
}
pub unsafe extern "win64" fn add_font_resource_ex_a(
    name: *const u8,
    _flags: u32,
    _reserved: *mut c_void,
) -> i32 {
    add_font_resource_a(name)
}
pub unsafe extern "win64" fn add_font_resource_ex_w(
    name: *const u16,
    _flags: u32,
    _reserved: *mut c_void,
) -> i32 {
    add_font_resource_w(name)
}
pub unsafe extern "win64" fn remove_font_resource_a(name: *const u8) -> i32 {
    remove_font_file(&resolve_font_path(&cstr(name))) as i32
}
pub unsafe extern "win64" fn remove_font_resource_w(name: *const u16) -> i32 {
    remove_font_file(&resolve_font_path(&wstr(name))) as i32
}
pub unsafe extern "win64" fn remove_font_resource_ex_a(
    name: *const u8,
    _flags: u32,
    _reserved: *mut c_void,
) -> i32 {
    remove_font_resource_a(name)
}
pub unsafe extern "win64" fn remove_font_resource_ex_w(
    name: *const u16,
    _flags: u32,
    _reserved: *mut c_void,
) -> i32 {
    remove_font_resource_w(name)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "EnumFontFamiliesExA" => fn enum_font_families_ex_a,
        "EnumFontFamiliesExW" => fn enum_font_families_ex_w,
        "EnumFontFamiliesA" => fn enum_font_families_a,
        "EnumFontFamiliesW" => fn enum_font_families_w,
        "EnumFontsA" => fn enum_font_families_a,
        "EnumFontsW" => fn enum_font_families_w,
        "AddFontResourceA" => fn add_font_resource_a,
        "AddFontResourceW" => fn add_font_resource_w,
        "AddFontResourceExA" => fn add_font_resource_ex_a,
        "AddFontResourceExW" => fn add_font_resource_ex_w,
        "RemoveFontResourceA" => fn remove_font_resource_a,
        "RemoveFontResourceW" => fn remove_font_resource_w,
        "RemoveFontResourceExA" => fn remove_font_resource_ex_a,
        "RemoveFontResourceExW" => fn remove_font_resource_ex_w,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Construit une police TrueType minimale (tables name/OS/2/head/hhea/post).
    pub fn sample_font(
        family: &str,
        style: &str,
        weight: u16,
        italic: bool,
        fixed: bool,
    ) -> Vec<u8> {
        let utf16 =
            |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect() };
        let strings = [(1u16, utf16(family)), (2, utf16(style))];
        let mut name = Vec::new();
        name.extend(0u16.to_be_bytes());
        name.extend((strings.len() as u16).to_be_bytes());
        name.extend((6 + strings.len() as u16 * 12).to_be_bytes());
        let mut pool: Vec<u8> = Vec::new();
        for (id, s) in &strings {
            for v in [3u16, 1, 0x0409, *id, s.len() as u16, pool.len() as u16] {
                name.extend(v.to_be_bytes());
            }
            pool.extend(s);
        }
        name.extend(pool);
        let mut os2 = vec![0u8; 96];
        os2[0..2].copy_from_slice(&4u16.to_be_bytes());
        os2[2..4].copy_from_slice(&1000u16.to_be_bytes());
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        let sel: u16 = (italic as u16) | if weight >= 700 { 0x20 } else { 0x40 };
        os2[62..64].copy_from_slice(&sel.to_be_bytes());
        os2[78..82].copy_from_slice(&1u32.to_be_bytes());
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&2048u16.to_be_bytes());
        let mut hhea = vec![0u8; 36];
        hhea[4..6].copy_from_slice(&1854i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-434i16).to_be_bytes());
        let mut post = vec![0u8; 32];
        post[12..16].copy_from_slice(&(fixed as u32).to_be_bytes());
        let tables: [(&[u8; 4], Vec<u8>); 5] = [
            (b"OS/2", os2),
            (b"head", head),
            (b"hhea", hhea),
            (b"name", name),
            (b"post", post),
        ];
        let mut out = Vec::new();
        out.extend(0x0001_0000u32.to_be_bytes());
        out.extend((tables.len() as u16).to_be_bytes());
        out.extend([0u8; 6]);
        let mut offset = 12 + tables.len() * 16;
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend(*tag);
            out.extend(0u32.to_be_bytes());
            out.extend((offset as u32).to_be_bytes());
            out.extend((data.len() as u32).to_be_bytes());
            body.extend(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            offset = 12 + tables.len() * 16 + body.len();
        }
        out.extend(body);
        out
    }

    #[test]
    fn tailles_des_structures_win32() {
        assert_eq!(std::mem::size_of::<LogFontHeader>(), 28);
        assert_eq!(std::mem::size_of::<EnumLogFontExA>(), 188);
        assert_eq!(std::mem::size_of::<EnumLogFontExW>(), 348);
        assert_eq!(std::mem::size_of::<NewTextMetricExA>(), 96);
        assert_eq!(std::mem::size_of::<NewTextMetricExW>(), 100);
    }

    #[test]
    fn lit_les_tables_d_une_police() {
        let data = sample_font("Winlux Sans", "Bold Italic", 700, true, false);
        let faces = parse_font_bytes(&data, Path::new("/x/w.ttf"));
        assert_eq!(faces.len(), 1);
        let f = &faces[0];
        assert_eq!(f.family, "Winlux Sans");
        assert_eq!(f.style, "Bold Italic");
        assert_eq!(f.full_name, "Winlux Sans Bold Italic");
        assert_eq!(f.weight, 700);
        assert!(f.italic && f.bold && !f.fixed_pitch && !f.cff);
        assert_eq!(f.units_per_em, 2048);
        assert_eq!((f.ascent, f.descent), (1854, -434));
    }

    #[test]
    fn collection_ttc_et_fichier_invalide() {
        let a = sample_font("Alpha", "Regular", 400, false, false);
        let b = sample_font("Alpha", "Bold", 700, false, false);
        // En-tête ttcf + deux sous-polices dont les décalages de tables sont
        // relatifs au début du fichier : on recopie et décale chaque police.
        let mut out = b"ttcf".to_vec();
        out.extend(0x0001_0000u32.to_be_bytes());
        out.extend(2u32.to_be_bytes());
        let base_a = 20u32;
        let base_b = base_a + a.len() as u32;
        out.extend(base_a.to_be_bytes());
        out.extend(base_b.to_be_bytes());
        for (base, font) in [(base_a, &a), (base_b, &b)] {
            let mut f = font.clone();
            let n = be16(&f, 4).unwrap() as usize;
            for i in 0..n {
                let at = 12 + i * 16 + 8;
                let off = be32(&f, at).unwrap() + base;
                f[at..at + 4].copy_from_slice(&off.to_be_bytes());
            }
            out.extend(f);
        }
        let faces = parse_font_bytes(&out, Path::new("/x/a.ttc"));
        assert_eq!(faces.len(), 2);
        assert_eq!(faces[1].style, "Bold");
        assert!(parse_font_bytes(b"pas une police", Path::new("/x/z.ttf")).is_empty());
    }

    #[test]
    fn familles_et_selection() {
        let p = Path::new("/x");
        let mut faces = Vec::new();
        for (fam, style, w, it) in [
            ("Zeta", "Bold", 700, false),
            ("Zeta", "Regular", 400, false),
            ("alpha", "Italic", 400, true),
        ] {
            faces.extend(parse_font_bytes(&sample_font(fam, style, w, it, false), p));
        }
        let fams = families(&faces);
        assert_eq!(fams.len(), 2);
        assert_eq!(fams[0].family, "alpha");
        assert_eq!(fams[1].style, "Regular");
        assert_eq!(select(&faces, "ZETA").len(), 2);
        assert!(select(&faces, "Inconnue").is_empty());
    }

    #[test]
    fn entree_d_enumeration() {
        let data = sample_font("Mono Test", "Regular", 400, false, true);
        let f = &parse_font_bytes(&data, Path::new("/x"))[0];
        let (lf, tm) = enum_entry_w(f, 0);
        assert_eq!(String::from_utf16_lossy(&lf.face[..9]), "Mono Test");
        assert_eq!(lf.lf.pitch_and_family & 0x3, 1);
        assert_eq!(tm.m.height, 16);
        assert_eq!(tm.m.ascent + tm.m.descent, 16);
        assert_eq!(tm.tail.pitch_and_family & 1, 0, "fixe : bit 0 à zéro");
        assert_ne!(tm.extra.flags & NTM_REGULAR, 0);
        let (lfa, _) = enum_entry_a(f, 0);
        assert_eq!(&lfa.face[..10], b"Mono Test\0");
    }

    #[test]
    fn scan_et_ajout_prive() {
        let dir = std::env::temp_dir().join(format!("winlux-fonts-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("a.TTF"),
            sample_font("Scan A", "Regular", 400, false, false),
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let found = scan(&[dir.clone(), dir.clone()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].family, "Scan A");

        let private = dir.join("private.otf");
        std::fs::write(
            &private,
            sample_font("Privée", "Regular", 400, false, false),
        )
        .unwrap();
        assert_eq!(add_font_file(&private), 1);
        assert!(lock(added()).iter().any(|f| f.family == "Privée"));
        assert!(remove_font_file(&private));
        assert!(!remove_font_file(&private));
        assert_eq!(add_font_file(&dir.join("absente.ttf")), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
