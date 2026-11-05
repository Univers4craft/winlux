//! Uniscribe (usp10.dll) et le rendu des glyphes TrueType de GDI
//! (GetGlyphIndicesW, GetGlyphOutlineW, GetOutlineTextMetricsW).
//!
//! Écrit d'après la documentation publique de Microsoft (salle blanche,
//! voir AGENTS.md). Les deux parties partagent la même police : celle
//! choisie dans le DC, lue par `sfnt` (fichiers TrueType du système, de
//! C:\Windows\Fonts et ceux ajoutés par AddFontResourceEx). Un numéro de
//! glyphe rendu par ScriptShape ou GetGlyphIndicesW se dessine donc tel
//! quel par GetGlyphOutlineW(GGO_GLYPH_INDEX), comme le font OpenTTD et
//! les logiciels qui ont leur propre cache de glyphes.
//!
//! Mise en forme : un glyphe par caractère (paires de substitution
//! comprises), sans ligatures ni formes contextuelles de l'arabe ; ordre
//! bidirectionnel simplifié (lettres hébraïques et arabes de droite à
//! gauche, le reste de gauche à droite) et réordonnancement visuel de la
//! règle L2 d'Unicode dans ScriptLayout.

use super::d2d_raster::{Canvas, FillRule, Matrix, Path};
use super::sfnt::{self, OutlineSink, Pt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const S_OK: u32 = 0;
const E_INVALIDARG: u32 = 0x8007_0057;
const E_OUTOFMEMORY: u32 = 0x8007_000E;
const E_PENDING: u32 = 0x8000_000A;
const GDI_ERROR: u32 = u32::MAX;

/// Police résolue pour un HFONT : fichier TrueType et taille em en pixels.
pub struct GdiFont {
    pub font: Arc<sfnt::Font>,
    pub em: f32,
    pub family: String,
    pub style: String,
    pub weight: u16,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

impl GdiFont {
    fn scale(&self) -> f32 {
        self.em / self.font.metrics.units_per_em.max(1) as f32
    }
    fn ascent(&self) -> i32 {
        (self.font.metrics.ascent as f32 * self.scale()).ceil() as i32
    }
    fn descent(&self) -> i32 {
        (self.font.metrics.descent as f32 * self.scale()).ceil() as i32
    }
    pub fn advance_px(&self, gid: u16) -> i32 {
        (self.font.advance(gid) as f32 * self.scale()).round() as i32
    }
}

type Key = (String, i32, i32, bool, bool, bool);

fn fonts() -> &'static Mutex<HashMap<Key, Arc<GdiFont>>> {
    static F: OnceLock<Mutex<HashMap<Key, Arc<GdiFont>>>> = OnceLock::new();
    F.get_or_init(Default::default)
}

/// Taille em en pixels d'après lfHeight : négatif = hauteur des caractères,
/// positif = hauteur de cellule (ascendante + descendante), 0 = défaut.
fn em_for(height: i32, m: &sfnt::Metrics) -> f32 {
    match height {
        h if h < 0 => (-h) as f32,
        0 => 12.0,
        h => {
            let cell = (m.ascent as f32 + m.descent as f32).max(1.0);
            h as f32 * m.units_per_em as f32 / cell
        }
    }
}

/// Police du HFONT sélectionné dans le DC.
pub fn dc_font(hdc: usize) -> Option<Arc<GdiFont>> {
    let spec = super::gui::font_spec(super::gui::dc_font(hdc)).unwrap_or_default();
    spec_font(&spec)
}

/// Police TrueType d'une description GDI (face, hauteur, graisse…).
pub fn spec_font(spec: &super::gui::FontSpec) -> Option<Arc<GdiFont>> {
    let key: Key = (
        spec.face.to_lowercase(),
        spec.height,
        spec.weight,
        spec.italic,
        spec.underline,
        spec.strikeout,
    );
    if let Some(f) = super::lock(fonts()).get(&key) {
        return Some(f.clone());
    }
    let face = match spec.face.as_str() {
        "" | "fixed" | "System" | "MS Shell Dlg" | "MS Shell Dlg 2" => "Segoe UI",
        f => f,
    };
    let weight = if spec.weight <= 0 {
        400
    } else {
        spec.weight as u32
    };
    let info = super::dwrite::gdi_face(face, weight, spec.italic)?;
    let font = sfnt::load(&info.path, info.index)?;
    let em = em_for(spec.height, &font.metrics);
    let f = Arc::new(GdiFont {
        font,
        em,
        family: info.family,
        style: info.style,
        weight: info.weight,
        italic: spec.italic,
        underline: spec.underline,
        strikeout: spec.strikeout,
    });
    super::lock(fonts()).insert(key, f.clone());
    Some(f)
}

// ── GDI ─────────────────────────────────────────────────────────────────

const GGI_MARK_NONEXISTING_GLYPHS: u32 = 1;

/// GetGlyphIndicesW : numéros de glyphes de la police du DC.
unsafe extern "win64" fn get_glyph_indices_w(
    hdc: usize,
    text: *const u16,
    count: i32,
    out: *mut u16,
    flags: u32,
) -> u32 {
    let Some(f) = dc_font(hdc) else {
        return GDI_ERROR;
    };
    if text.is_null() || out.is_null() || count < 0 {
        return GDI_ERROR;
    }
    for i in 0..count as usize {
        let g = f.font.glyph_index(*text.add(i) as u32);
        *out.add(i) = if g == 0 && flags & GGI_MARK_NONEXISTING_GLYPHS != 0 {
            0xFFFF
        } else {
            g
        };
    }
    count as u32
}

/// Glyphes d'un texte posés à partir du coin haut-gauche (x, y) : pour
/// chacun, position du coin haut-gauche de son raster. Renvoie aussi la
/// boîte occupée (largeur avancée, hauteur de ligne), pour le fond opaque.
pub fn layout_pixels(
    spec: &super::gui::FontSpec,
    text: &str,
) -> Option<(Vec<(i32, i32, Arc<Raster>)>, i32, i32)> {
    let f = spec_font(spec)?;
    let ascent = f.ascent();
    let mut pen = 0;
    let mut out = Vec::new();
    for ch in text.chars() {
        let gid = f.font.glyph_index(ch as u32);
        let r = glyph_raster(&f, gid);
        if !r.empty {
            out.push((pen + r.origin.0, ascent - r.origin.1, r.clone()));
        }
        pen += r.advance;
    }
    Some((out, pen, ascent + f.descent()))
}

type GlyphKey = (usize, u32, u16);

fn glyph_cache() -> &'static Mutex<HashMap<GlyphKey, Arc<Raster>>> {
    static G: OnceLock<Mutex<HashMap<GlyphKey, Arc<Raster>>>> = OnceLock::new();
    G.get_or_init(Default::default)
}

/// Raster d'un glyphe, gardé en cache par police et taille.
fn glyph_raster(f: &Arc<GdiFont>, gid: u16) -> Arc<Raster> {
    let key = (Arc::as_ptr(&f.font) as usize, f.em.to_bits(), gid);
    if let Some(r) = super::lock(glyph_cache()).get(&key) {
        return r.clone();
    }
    let r = Arc::new(rasterize(f, gid));
    let mut cache = super::lock(glyph_cache());
    if cache.len() > 8192 {
        cache.clear();
    }
    cache.insert(key, r.clone());
    r
}

/// Glyphe rendu en niveaux de couverture 0..=255, ligne par ligne.
pub struct Raster {
    pub w: usize,
    pub h: usize,
    /// Origine : coin haut-gauche par rapport au point de base (y vers le haut).
    pub origin: (i32, i32),
    pub advance: i32,
    pub cov: Vec<u8>,
    pub empty: bool,
}

struct ToPath<'a> {
    path: &'a mut Path,
    s: f32,
    dx: f32,
    top: f32,
}

impl ToPath<'_> {
    fn map(&self, p: Pt) -> Pt {
        (p.0 * self.s - self.dx, self.top - p.1 * self.s)
    }
}

impl OutlineSink for ToPath<'_> {
    fn move_to(&mut self, p: Pt) {
        let q = self.map(p);
        self.path.begin(q);
    }
    fn line_to(&mut self, p: Pt) {
        let q = self.map(p);
        self.path.line(q);
    }
    fn quad_to(&mut self, c: Pt, p: Pt) {
        let (c, p) = (self.map(c), self.map(p));
        self.path.quadratic(c, p, 1.0);
    }
    fn close(&mut self) {
        self.path.close();
    }
}

/// Dessine un glyphe à la taille de la police.
pub fn rasterize(f: &GdiFont, gid: u16) -> Raster {
    let s = f.scale();
    let advance = f.advance_px(gid);
    let Some([x0, y0, x1, y1]) = f.font.bbox(gid) else {
        return Raster {
            w: 1,
            h: 1,
            origin: (0, 0),
            advance,
            cov: vec![0],
            empty: true,
        };
    };
    let l = (x0 as f32 * s).floor();
    let r = (x1 as f32 * s).ceil();
    let b = (y0 as f32 * s).floor();
    let t = (y1 as f32 * s).ceil();
    let w = ((r - l) as usize).max(1);
    let h = ((t - b) as usize).max(1);
    let mut path = Path::default();
    let mut sink = ToPath {
        path: &mut path,
        s,
        dx: l,
        top: t,
    };
    f.font.outline(gid, &mut sink);
    let mut px = vec![0u8; w * h * 4];
    let mut canvas = Canvas {
        w: w as u32,
        h: h as u32,
        stride: w * 4,
        px: &mut px,
    };
    let white = [1.0, 1.0, 1.0, 1.0];
    canvas.fill(
        &path.fill_polys(&Matrix::IDENTITY),
        FillRule::NonZero,
        None,
        &|_, _| white,
    );
    Raster {
        w,
        h,
        origin: (l as i32, t as i32),
        advance,
        cov: px.chunks(4).map(|p| p[3]).collect(),
        empty: false,
    }
}

const GGO_METRICS: u32 = 0;
const GGO_BITMAP: u32 = 1;
const GGO_GRAY2_BITMAP: u32 = 4;
const GGO_GRAY4_BITMAP: u32 = 5;
const GGO_GRAY8_BITMAP: u32 = 6;
const GGO_GLYPH_INDEX: u32 = 0x80;

/// Bitmap au format GGO demandé (lignes alignées sur 4 octets).
pub fn ggo_bitmap(r: &Raster, format: u32) -> Option<Vec<u8>> {
    let (w, h) = (r.w, r.h);
    match format {
        GGO_BITMAP => {
            let pitch = (w.div_ceil(8) + 3) & !3;
            let mut out = vec![0u8; pitch * h];
            for y in 0..h {
                for x in 0..w {
                    if r.cov[y * w + x] >= 128 {
                        out[y * pitch + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            Some(out)
        }
        GGO_GRAY2_BITMAP | GGO_GRAY4_BITMAP | GGO_GRAY8_BITMAP => {
            let levels: u32 = match format {
                GGO_GRAY2_BITMAP => 4,
                GGO_GRAY4_BITMAP => 16,
                _ => 64,
            };
            let pitch = (w + 3) & !3;
            let mut out = vec![0u8; pitch * h];
            for y in 0..h {
                for x in 0..w {
                    let c = r.cov[y * w + x] as u32;
                    out[y * pitch + x] = ((c * levels + 127) / 255) as u8;
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// GetGlyphOutlineW : mesures et bitmaps (GGO_BITMAP, GGO_GRAY2/4/8).
/// Les contours vectoriels (GGO_NATIVE, GGO_BEZIER) et la matrice MAT2
/// autre que l'identité ne sont pas encore traités.
unsafe extern "win64" fn get_glyph_outline_w(
    hdc: usize,
    ch: u32,
    format: u32,
    gm: *mut u8,
    size: u32,
    buf: *mut u8,
    _mat: *const u8,
) -> u32 {
    let Some(f) = dc_font(hdc) else {
        return GDI_ERROR;
    };
    let gid = if format & GGO_GLYPH_INDEX != 0 {
        ch as u16
    } else {
        f.font.glyph_index(ch)
    };
    let kind = format & 0x7F & !0x100 & 0xF;
    let r = rasterize(&f, gid);
    if !gm.is_null() {
        let (bx, by) = if r.empty {
            (1, 1)
        } else {
            (r.w as u32, r.h as u32)
        };
        std::ptr::write_unaligned(gm as *mut u32, bx);
        std::ptr::write_unaligned(gm.add(4) as *mut u32, by);
        std::ptr::write_unaligned(gm.add(8) as *mut i32, r.origin.0);
        std::ptr::write_unaligned(gm.add(12) as *mut i32, r.origin.1);
        std::ptr::write_unaligned(gm.add(16) as *mut i16, r.advance as i16);
        std::ptr::write_unaligned(gm.add(18) as *mut i16, 0);
    }
    if kind == GGO_METRICS {
        return 0;
    }
    let Some(bits) = ggo_bitmap(&r, kind) else {
        return GDI_ERROR;
    };
    if buf.is_null() || size == 0 {
        return bits.len() as u32;
    }
    if (size as usize) < bits.len() {
        return GDI_ERROR;
    }
    std::ptr::copy_nonoverlapping(bits.as_ptr(), buf, bits.len());
    bits.len() as u32
}

/// TEXTMETRICW (60 octets) de la police.
fn text_metrics(f: &GdiFont, out: &mut [u8]) {
    let asc = f.ascent();
    let desc = f.descent();
    let height = asc + desc;
    let space = f.font.glyph_index(' ' as u32);
    let avg = f
        .advance_px(f.font.glyph_index('x' as u32))
        .max(f.advance_px(space));
    let max_w = (0..f.font.glyph_count().min(512))
        .map(|g| f.advance_px(g))
        .max()
        .unwrap_or(avg);
    let gap = (f.font.metrics.line_gap.max(0) as f32 * f.scale()).round() as i32;
    let longs = [
        height,
        asc,
        desc,
        (height - f.em.round() as i32).max(0),
        gap,
        avg,
        max_w,
        f.weight as i32,
        0,
        96,
        96,
    ];
    for (i, v) in longs.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, c) in [0x20u16, 0xFFFC, 0x1F, 0x20].iter().enumerate() {
        out[44 + i * 2..46 + i * 2].copy_from_slice(&c.to_le_bytes());
    }
    out[52] = f.italic as u8;
    out[53] = f.underline as u8;
    out[54] = f.strikeout as u8;
    out[55] = 0x06 | 0x20; // TMPF_VECTOR | TMPF_TRUETYPE, famille suisse
    out[56] = 1; // DEFAULT_CHARSET
}

/// OUTLINETEXTMETRICW complet (232 octets + noms en UTF-16).
pub fn outline_text_metrics(f: &GdiFont) -> Vec<u8> {
    let names = [
        f.family.clone(),
        format!("{} {}", f.family, f.style),
        f.style.clone(),
        format!("{} {}", f.family, f.style),
    ];
    let mut out = vec![0u8; 232];
    let mut tail: Vec<u8> = Vec::new();
    let mut offsets = [0u64; 4];
    for (i, n) in names.iter().enumerate() {
        offsets[i] = (232 + tail.len()) as u64;
        for c in n.encode_utf16().chain([0]) {
            tail.extend(c.to_le_bytes());
        }
    }
    let total = 232 + tail.len();
    out[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    text_metrics(f, &mut out[4..64]);
    let m = &f.font.metrics;
    let s = f.scale();
    let px = |v: f32| (v * s).round() as i32;
    let put = |o: &mut Vec<u8>, at: usize, v: i32| o[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut out, 76, if f.italic { 1 } else { 0x40 }); // fsSelection
    put(&mut out, 84, 1); // pente : verticale
    put(&mut out, 96, m.units_per_em as i32);
    put(&mut out, 100, px(m.ascent as f32));
    put(&mut out, 104, -px(m.descent as f32));
    put(&mut out, 108, px(m.line_gap.max(0) as f32));
    put(&mut out, 112, px(m.cap_height as f32));
    put(&mut out, 116, px(m.x_height as f32));
    put(&mut out, 120, 0);
    put(&mut out, 124, px(m.ascent as f32));
    put(&mut out, 128, f.em.round() as i32);
    put(&mut out, 132, -px(m.descent as f32));
    put(&mut out, 136, px(m.ascent as f32));
    put(&mut out, 140, -px(m.descent as f32));
    put(&mut out, 144, px(m.line_gap.max(0) as f32));
    put(&mut out, 148, 9);
    let sub = (f.em * 0.6).round() as i32;
    for (at, v) in [
        (152, sub),
        (156, sub),
        (160, 0),
        (164, sub / 3),
        (168, sub),
        (172, sub),
    ] {
        put(&mut out, at, v);
    }
    put(&mut out, 176, 0);
    put(&mut out, 180, sub);
    put(&mut out, 184, px(m.strikeout_thickness as f32).max(1));
    put(&mut out, 188, px(m.strikeout_position as f32));
    put(&mut out, 192, px(m.underline_thickness as f32).max(1));
    put(&mut out, 196, px(m.underline_position as f32));
    for (i, o) in offsets.iter().enumerate() {
        out[200 + i * 8..208 + i * 8].copy_from_slice(&o.to_le_bytes());
    }
    out.extend(tail);
    out
}

/// GetOutlineTextMetricsW : taille nécessaire si le tampon est nul.
unsafe extern "win64" fn get_outline_text_metrics_w(hdc: usize, size: u32, out: *mut u8) -> u32 {
    let Some(f) = dc_font(hdc) else {
        return 0;
    };
    let otm = outline_text_metrics(&f);
    if out.is_null() {
        return otm.len() as u32;
    }
    if (size as usize) < 232 {
        return 0;
    }
    let n = otm.len().min(size as usize);
    std::ptr::copy_nonoverlapping(otm.as_ptr(), out, n);
    n as u32
}

/// SetDIBColorTable : l'affichage est en couleurs vraies ; la palette d'une
/// DIB à 8 bits n'est pas encore appliquée au rendu.
extern "win64" fn set_dib_color_table(
    _hdc: usize,
    _start: u32,
    count: u32,
    _rgb: *const u8,
) -> u32 {
    count
}

// ── Uniscribe ───────────────────────────────────────────────────────────

/// SCRIPT_CACHE : un numéro qui retient la police résolue.
fn caches() -> &'static Mutex<HashMap<usize, Arc<GdiFont>>> {
    static C: OnceLock<Mutex<HashMap<usize, Arc<GdiFont>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

unsafe fn cache_font(hdc: usize, psc: *mut usize) -> Result<Arc<GdiFont>, u32> {
    if !psc.is_null() && *psc != 0 {
        if let Some(f) = super::lock(caches()).get(&*psc) {
            return Ok(f.clone());
        }
    }
    if hdc == 0 {
        return Err(E_PENDING);
    }
    let f = dc_font(hdc).ok_or(E_INVALIDARG)?;
    if !psc.is_null() {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x5C00);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        super::lock(caches()).insert(id, f.clone());
        *psc = id;
    }
    Ok(f)
}

unsafe extern "win64" fn script_free_cache(psc: *mut usize) -> u32 {
    if !psc.is_null() {
        super::lock(caches()).remove(&*psc);
        *psc = 0;
    }
    S_OK
}

/// Lettre écrite de droite à gauche (hébreu, arabe, syriaque, thâna, n'ko…).
pub fn is_rtl(c: u32) -> bool {
    matches!(c, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
}

/// Caractère à direction forte de gauche à droite.
fn is_ltr(c: u32) -> bool {
    char::from_u32(c).is_some_and(|ch| ch.is_alphabetic()) && !is_rtl(c)
}

fn code_points(text: &[u16]) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i] as u32;
        if (0xD800..0xDC00).contains(&c)
            && i + 1 < text.len()
            && (0xDC00..0xE000).contains(&(text[i + 1] as u32))
        {
            out.push((
                i,
                0x10000 + ((c - 0xD800) << 10) + (text[i + 1] as u32 - 0xDC00),
            ));
            i += 2;
        } else {
            out.push((i, c));
            i += 1;
        }
    }
    out
}

/// Découpe en passages de même direction : (début, niveau bidi).
pub fn itemize(text: &[u16], base_rtl: bool) -> Vec<(usize, u8)> {
    let base = base_rtl as u8;
    let mut runs: Vec<(usize, u8)> = Vec::new();
    let mut current: Option<u8> = None;
    for (pos, c) in code_points(text) {
        let level = if is_rtl(c) {
            Some(1)
        } else if is_ltr(c) || (c as u8 as u32 == c && (c as u8).is_ascii_digit()) {
            Some(if base_rtl { 2 } else { 0 })
        } else {
            None // neutre : suit le passage en cours
        };
        let level = level.or(current).unwrap_or(base);
        if current != Some(level) {
            runs.push((pos, level));
            current = Some(level);
        }
    }
    if runs.is_empty() {
        runs.push((0, base));
    }
    runs
}

/// ScriptItemize : SCRIPT_ITEM { iCharPos, SCRIPT_ANALYSIS } (8 octets),
/// plus un élément final qui marque la fin du texte.
unsafe extern "win64" fn script_itemize(
    text: *const u16,
    n: i32,
    max_items: i32,
    _control: *const u8,
    state: *const u16,
    items: *mut u8,
    count: *mut i32,
) -> u32 {
    if text.is_null() || n <= 0 || max_items < 2 || items.is_null() {
        return E_INVALIDARG;
    }
    let t = std::slice::from_raw_parts(text, n as usize);
    let base_rtl = !state.is_null() && (*state & 0x1F) % 2 == 1;
    let runs = itemize(t, base_rtl);
    if runs.len() + 1 > max_items as usize {
        return E_OUTOFMEMORY;
    }
    for (i, &(pos, level)) in runs.iter().chain([&(n as usize, 0)]).enumerate() {
        let at = items.add(i * 8);
        std::ptr::write_unaligned(at as *mut i32, pos as i32);
        let rtl = level % 2 == 1;
        // eScript = 0 (SCRIPT_UNDEFINED), fRTL, fLayoutRTL.
        let a: u16 = if rtl { (1 << 10) | (1 << 11) } else { 0 };
        std::ptr::write_unaligned(at.add(4) as *mut u16, a);
        std::ptr::write_unaligned(at.add(6) as *mut u16, level as u16 & 0x1F);
    }
    if !count.is_null() {
        *count = runs.len() as i32;
    }
    S_OK
}

fn zero_width(c: u32) -> bool {
    matches!(c, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0xFEFF | 0x00AD)
}

/// ScriptShape : un glyphe par caractère, dans l'ordre visuel du passage.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn script_shape(
    hdc: usize,
    psc: *mut usize,
    text: *const u16,
    n: i32,
    max_glyphs: i32,
    sa: *const u16,
    glyphs: *mut u16,
    clusters: *mut u16,
    visattr: *mut u16,
    count: *mut i32,
) -> u32 {
    if text.is_null() || n <= 0 || glyphs.is_null() || clusters.is_null() || visattr.is_null() {
        return E_INVALIDARG;
    }
    let f = match cache_font(hdc, psc) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let t = std::slice::from_raw_parts(text, n as usize);
    let cps = code_points(t);
    if cps.len() > max_glyphs.max(0) as usize {
        return E_OUTOFMEMORY;
    }
    let rtl = !sa.is_null() && *sa & (1 << 10) != 0;
    let g = cps.len();
    for (k, &(pos, c)) in cps.iter().enumerate() {
        let slot = if rtl { g - 1 - k } else { k };
        let gid = if zero_width(c) {
            f.font.glyph_index(0x200B)
        } else {
            f.font.glyph_index(c)
        };
        *glyphs.add(slot) = gid;
        // uJustification 0, fClusterStart, fZeroWidth.
        *visattr.add(slot) = (1 << 4) | if zero_width(c) { 1 << 6 } else { 0 };
        let next = cps.get(k + 1).map_or(n as usize, |p| p.0);
        for i in pos..next {
            *clusters.add(i) = slot as u16;
        }
    }
    if !count.is_null() {
        *count = g as i32;
    }
    S_OK
}

/// ScriptPlace : avances en pixels, décalages nuls, ABC du passage.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn script_place(
    hdc: usize,
    psc: *mut usize,
    glyphs: *const u16,
    n: i32,
    visattr: *const u16,
    _sa: *mut u16,
    advance: *mut i32,
    goffset: *mut i32,
    abc: *mut i32,
) -> u32 {
    if glyphs.is_null() || n < 0 || advance.is_null() {
        return E_INVALIDARG;
    }
    let f = match cache_font(hdc, psc) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let mut total = 0;
    for i in 0..n as usize {
        let zw = !visattr.is_null() && *visattr.add(i) & (1 << 6) != 0;
        let a = if zw { 0 } else { f.advance_px(*glyphs.add(i)) };
        *advance.add(i) = a;
        total += a;
        if !goffset.is_null() {
            *goffset.add(i * 2) = 0;
            *goffset.add(i * 2 + 1) = 0;
        }
    }
    if !abc.is_null() {
        *abc = 0;
        *abc.add(1) = total;
        *abc.add(2) = 0;
    }
    S_OK
}

/// Ordre visuel de passages de niveaux donnés (règle L2 d'Unicode).
pub fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let max = levels.iter().copied().max().unwrap_or(0);
    let min_odd = levels
        .iter()
        .copied()
        .filter(|l| l % 2 == 1)
        .min()
        .unwrap_or(max + 1);
    let mut level = max;
    while level >= min_odd && level > 0 {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let start = i;
                while i < order.len() && levels[order[i]] >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
        level -= 1;
    }
    order
}

unsafe extern "win64" fn script_layout(
    n: i32,
    levels: *const u8,
    v2l: *mut i32,
    l2v: *mut i32,
) -> u32 {
    if n < 0 || levels.is_null() {
        return E_INVALIDARG;
    }
    let lv = std::slice::from_raw_parts(levels, n as usize);
    let order = visual_order(lv);
    for (visual, &logical) in order.iter().enumerate() {
        if !v2l.is_null() {
            *v2l.add(visual) = logical as i32;
        }
        if !l2v.is_null() {
            *l2v.add(logical) = visual as i32;
        }
    }
    S_OK
}

/// Idéogrammes et kana : une coupure de ligne est permise avant chacun.
fn breaks_anywhere(c: u32) -> bool {
    matches!(c, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xAC00..=0xD7AF | 0x20000..=0x3FFFF)
}

/// SCRIPT_LOGATTR : fSoftBreak, fWhiteSpace, fCharStop, fWordStop.
pub fn logical_attrs(text: &[u16]) -> Vec<u8> {
    let mut out = vec![0u8; text.len()];
    let cps = code_points(text);
    let mut prev: Option<u32> = None;
    for &(pos, c) in &cps {
        let ch = char::from_u32(c).unwrap_or('\u{FFFD}');
        let white = ch.is_whitespace() && c != 0xA0 && c != 0x202F;
        let mut a = 0x4; // fCharStop
        if white {
            a |= 0x2;
        }
        if let Some(p) = prev {
            let pch = char::from_u32(p).unwrap_or(' ');
            let pwhite = pch.is_whitespace() && p != 0xA0 && p != 0x202F;
            if (pwhite && !white)
                || breaks_anywhere(c)
                || (breaks_anywhere(p) && !white)
                || p == 0x2D && !white
            {
                a |= 0x1; // fSoftBreak
            }
            if pwhite && !white || (!pch.is_alphanumeric() && ch.is_alphanumeric()) {
                a |= 0x8; // fWordStop
            }
        } else if !white {
            a |= 0x8;
        }
        out[pos] = a;
        prev = Some(c);
    }
    out
}

unsafe extern "win64" fn script_break(
    text: *const u16,
    n: i32,
    _sa: *const u16,
    attrs: *mut u8,
) -> u32 {
    if text.is_null() || n <= 0 || attrs.is_null() {
        return E_INVALIDARG;
    }
    let a = logical_attrs(std::slice::from_raw_parts(text, n as usize));
    std::ptr::copy_nonoverlapping(a.as_ptr(), attrs, a.len());
    S_OK
}

/// ScriptGetLogicalWidths : largeur de chaque caractère (celle de sa
/// grappe partagée entre ses caractères).
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn script_get_logical_widths(
    _sa: *const u16,
    n_chars: i32,
    n_glyphs: i32,
    glyph_w: *const i32,
    clusters: *const u16,
    _visattr: *const u16,
    out: *mut i32,
) -> u32 {
    if n_chars < 0 || n_glyphs < 0 || glyph_w.is_null() || clusters.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let cl = std::slice::from_raw_parts(clusters, n_chars as usize);
    let gw = std::slice::from_raw_parts(glyph_w, n_glyphs as usize);
    let mut i = 0;
    while i < cl.len() {
        let g = cl[i];
        let mut j = i;
        while j < cl.len() && cl[j] == g {
            j += 1;
        }
        let w = gw.get(g as usize).copied().unwrap_or(0);
        let k = (j - i) as i32;
        for (m, c) in (i..j).enumerate() {
            *out.add(c) = w / k + if (m as i32) < w % k { 1 } else { 0 };
        }
        i = j;
    }
    S_OK
}

pub fn usp10_exports() -> crate::dlls::Exports {
    crate::exports! {
        "ScriptItemize" => fn script_itemize,
        "ScriptShape" => fn script_shape,
        "ScriptPlace" => fn script_place,
        "ScriptLayout" => fn script_layout,
        "ScriptBreak" => fn script_break,
        "ScriptGetLogicalWidths" => fn script_get_logical_widths,
        "ScriptFreeCache" => fn script_free_cache,
    }
}

pub fn gdi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetGlyphIndicesW" => fn get_glyph_indices_w,
        "GetGlyphOutlineW" => fn get_glyph_outline_w,
        "SetDIBColorTable" => fn set_dib_color_table,
    }
}

/// Remplace GetOutlineTextMetricsW, qui renvoyait toujours 0.
pub fn gdi32_overrides() -> crate::dlls::Exports {
    crate::exports! {
        "GetOutlineTextMetricsW" => fn get_outline_text_metrics_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn passages_bidirectionnels() {
        assert_eq!(itemize(&w("Bonjour"), false), vec![(0, 0)]);
        // « abc » puis de l'hébreu, la ponctuation suit le passage en cours.
        let t = w("abc \u{05D0}\u{05D1}! d");
        assert_eq!(itemize(&t, false), vec![(0, 0), (4, 1), (8, 0)]);
    }

    #[test]
    fn regle_l2_de_reordonnancement() {
        assert_eq!(visual_order(&[0, 1, 1, 0]), vec![0, 2, 1, 3]);
        assert_eq!(visual_order(&[1, 2, 1]), vec![2, 1, 0]);
        assert_eq!(visual_order(&[0, 0]), vec![0, 1]);
    }

    #[test]
    fn coupures_de_ligne() {
        let a = logical_attrs(&w("un deux"));
        assert_eq!(a[2] & 0x2, 0x2, "espace");
        assert_eq!(a[3] & 0x1, 0x1, "coupure avant « deux »");
        assert_eq!(a[1] & 0x1, 0, "pas au milieu d'un mot");
        let j = logical_attrs(&w("日本語"));
        assert_eq!(j[1] & 0x1, 0x1);
    }

    #[test]
    fn largeurs_logiques_par_grappe() {
        let gw = [10, 7];
        let cl = [0u16, 0, 1];
        let mut out = [0i32; 3];
        unsafe {
            script_get_logical_widths(
                std::ptr::null(),
                3,
                2,
                gw.as_ptr(),
                cl.as_ptr(),
                std::ptr::null(),
                out.as_mut_ptr(),
            );
        }
        assert_eq!(out, [5, 5, 7]);
    }

    #[test]
    fn bitmaps_ggo_alignes_sur_quatre_octets() {
        let r = Raster {
            w: 3,
            h: 2,
            origin: (0, 2),
            advance: 4,
            cov: vec![255, 0, 128, 0, 255, 0],
            empty: false,
        };
        let g8 = ggo_bitmap(&r, GGO_GRAY8_BITMAP).unwrap();
        assert_eq!(g8.len(), 8);
        assert_eq!(&g8[0..3], &[64, 0, 32]);
        let b = ggo_bitmap(&r, GGO_BITMAP).unwrap();
        assert_eq!(b.len(), 8);
        assert_eq!(b[0], 0b1010_0000);
        assert_eq!(b[4], 0b0100_0000);
    }

    #[test]
    fn glyphe_rendu_depuis_une_vraie_police() {
        let path = super::super::dwrite::tests::test_font();
        let Some(font) = sfnt::load(&path, 0) else {
            return;
        };
        let f = GdiFont {
            font,
            em: 24.0,
            family: "Test".into(),
            style: "Regular".into(),
            weight: 400,
            italic: false,
            underline: false,
            strikeout: false,
        };
        let gid = f.font.glyph_index('A' as u32); // un carré dans la police de test
        assert_ne!(gid, 0);
        let r = rasterize(&f, gid);
        assert!(!r.empty);
        assert!(r.w > 4 && r.h > 10, "{}x{}", r.w, r.h);
        assert!(
            r.cov.iter().any(|&c| c == 255),
            "le carré a des pixels pleins"
        );
        assert!(r.advance > 0);
        let otm = outline_text_metrics(&f);
        assert_eq!(
            u32::from_le_bytes(otm[0..4].try_into().unwrap()) as usize,
            otm.len()
        );
        let em_square = u32::from_le_bytes(otm[96..100].try_into().unwrap());
        assert_eq!(em_square, f.font.metrics.units_per_em as u32);
    }
}
