//! GDI+ : familles de polices, polices, formats de chaîne, dessin et mesure
//! du texte (`GdipDrawString`, `GdipMeasureString`,
//! `GdipMeasureCharacterRanges`). D'après la documentation publique de
//! Microsoft Learn ; les glyphes viennent du rastériseur d'Uniscribe maison.

use super::super::d2d_raster::Clip;
use super::super::usp10;
use super::super::{gui, wstr, Exports};
use super::{
    add, draw_glyphs, font, font_em, font_spec, get, graphics, out, remove, string_format, with,
    Font, Graphics, Obj, StringFormat, GENERIC_ERROR, INVALID_PARAMETER, OK,
};

const FONT_FAMILY_NOT_FOUND: i32 = 14;
const LF_FACESIZE: usize = 32;

// Drapeaux de StringFormat.
const SF_RTL: i32 = 0x0001;
const SF_NO_WRAP: i32 = 0x1000;
const SF_LINE_LIMIT: i32 = 0x2000;
const SF_NO_CLIP: i32 = 0x4000;
const SF_TRAILING_SPACES: i32 = 0x0800;

// --- Familles ---------------------------------------------------------------------------

fn family(h: usize) -> Option<String> {
    match get(h)? {
        Obj::Family(s) => Some(s),
        _ => None,
    }
}

/// Métriques de dessin d'une famille (unités de conception).
fn design_metrics(name: &str, style: i32) -> Option<(u16, u16, u16, u16)> {
    let f = Font {
        family: name.to_string(),
        size: 12.0,
        style,
        unit: 2,
    };
    let g = usp10::spec_font(&font_spec(&f, 64.0))?;
    let m = &g.font.metrics;
    let spacing =
        (m.ascent as i32 + m.descent as i32 + (m.line_gap as i32).max(0)).min(u16::MAX as i32);
    Some((m.units_per_em, m.ascent, m.descent, spacing as u16))
}

unsafe extern "win64" fn create_font_family_from_name(
    name: *const u16,
    _coll: usize,
    fam: *mut usize,
) -> i32 {
    if name.is_null() {
        return INVALID_PARAMETER;
    }
    let n = wstr(name);
    if n.trim().is_empty() {
        return FONT_FAMILY_NOT_FOUND;
    }
    out(fam, add(Obj::Family(n)))
}

unsafe extern "win64" fn generic_sans(fam: *mut usize) -> i32 {
    out(fam, add(Obj::Family("Microsoft Sans Serif".into())))
}
unsafe extern "win64" fn generic_serif(fam: *mut usize) -> i32 {
    out(fam, add(Obj::Family("Times New Roman".into())))
}
unsafe extern "win64" fn generic_mono(fam: *mut usize) -> i32 {
    out(fam, add(Obj::Family("Courier New".into())))
}

unsafe extern "win64" fn delete_font_family(fam: usize) -> i32 {
    match family(fam) {
        Some(_) => remove(fam),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn clone_font_family(fam: usize, c: *mut usize) -> i32 {
    match family(fam) {
        Some(s) => out(c, add(Obj::Family(s))),
        None => INVALID_PARAMETER,
    }
}

/// Copie un nom dans un tampon LF_FACESIZE.
unsafe fn write_face(dst: *mut u16, name: &str) {
    let mut n = 0;
    for u in name.encode_utf16().take(LF_FACESIZE - 1) {
        *dst.add(n) = u;
        n += 1;
    }
    *dst.add(n) = 0;
}

unsafe extern "win64" fn get_family_name(fam: usize, name: *mut u16, _lang: u16) -> i32 {
    match family(fam) {
        Some(s) if !name.is_null() => {
            write_face(name, &s);
            OK
        }
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn is_style_available(fam: usize, _style: i32, avail: *mut i32) -> i32 {
    match family(fam) {
        Some(_) => out(avail, 1),
        None => INVALID_PARAMETER,
    }
}

unsafe fn metric(
    fam: usize,
    style: i32,
    v: *mut u16,
    pick: fn((u16, u16, u16, u16)) -> u16,
) -> i32 {
    let Some(s) = family(fam) else {
        return INVALID_PARAMETER;
    };
    match design_metrics(&s, style) {
        Some(m) => out(v, pick(m)),
        None => GENERIC_ERROR,
    }
}

unsafe extern "win64" fn get_em_height(fam: usize, style: i32, v: *mut u16) -> i32 {
    metric(fam, style, v, |m| m.0)
}
unsafe extern "win64" fn get_cell_ascent(fam: usize, style: i32, v: *mut u16) -> i32 {
    metric(fam, style, v, |m| m.1)
}
unsafe extern "win64" fn get_cell_descent(fam: usize, style: i32, v: *mut u16) -> i32 {
    metric(fam, style, v, |m| m.2)
}
unsafe extern "win64" fn get_line_spacing(fam: usize, style: i32, v: *mut u16) -> i32 {
    metric(fam, style, v, |m| m.3)
}

// --- Collections ------------------------------------------------------------------------

const GENERIC_FAMILIES: [&str; 3] = ["Microsoft Sans Serif", "Times New Roman", "Courier New"];

unsafe extern "win64" fn new_font_collection(c: *mut usize) -> i32 {
    out(c, add(Obj::Collection))
}

unsafe extern "win64" fn delete_private_collection(c: *mut usize) -> i32 {
    if c.is_null() {
        return INVALID_PARAMETER;
    }
    let st = remove(*c);
    *c = 0;
    st
}

unsafe extern "win64" fn collection_family_count(c: usize, n: *mut i32) -> i32 {
    match get(c) {
        Some(Obj::Collection) => out(n, GENERIC_FAMILIES.len() as i32),
        _ => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn collection_family_list(
    c: usize,
    want: i32,
    list: *mut usize,
    found: *mut i32,
) -> i32 {
    if !matches!(get(c), Some(Obj::Collection)) || list.is_null() {
        return INVALID_PARAMETER;
    }
    let n = (want.max(0) as usize).min(GENERIC_FAMILIES.len());
    for (i, f) in GENERIC_FAMILIES.iter().take(n).enumerate() {
        *list.add(i) = add(Obj::Family((*f).into()));
    }
    out(found, n as i32)
}

unsafe extern "win64" fn private_add_font_file(c: usize, path: *const u16) -> i32 {
    if !matches!(get(c), Some(Obj::Collection)) || path.is_null() {
        return INVALID_PARAMETER;
    }
    let p = super::super::gdi32_fonts::resolve_font_path(&wstr(path));
    if !p.exists() {
        return 10; // FileNotFound
    }
    if super::super::gdi32_fonts::add_font_file(&p) > 0 {
        OK
    } else {
        GENERIC_ERROR
    }
}

unsafe extern "win64" fn private_add_memory_font(c: usize, mem: *const u8, len: i32) -> i32 {
    if !matches!(get(c), Some(Obj::Collection)) || mem.is_null() || len <= 0 {
        return INVALID_PARAMETER;
    }
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let data = std::slice::from_raw_parts(mem, len as usize);
    let dir = std::env::temp_dir().join(format!("winlux-gdiplus-{}", std::process::id()));
    if std::fs::create_dir_all(&dir).is_err() {
        return GENERIC_ERROR;
    }
    let p = dir.join(format!("police-{}.ttf", N.fetch_add(1, Ordering::Relaxed)));
    if std::fs::write(&p, data).is_err() {
        return GENERIC_ERROR;
    }
    if super::super::gdi32_fonts::add_font_file(&p) > 0 {
        OK
    } else {
        GENERIC_ERROR
    }
}

// --- Polices ----------------------------------------------------------------------------

unsafe extern "win64" fn create_font(
    fam: usize,
    size: f32,
    style: i32,
    unit: i32,
    f: *mut usize,
) -> i32 {
    let Some(name) = family(fam) else {
        return INVALID_PARAMETER;
    };
    if size.is_nan() || size <= 0.0 || !(0..=6).contains(&unit) || unit == 1 {
        return INVALID_PARAMETER;
    }
    out(
        f,
        add(Obj::Font(Font {
            family: name,
            size,
            style,
            unit,
        })),
    )
}

/// Taille em en pixels d'une hauteur GDI (lfHeight).
fn em_of_height(h: i32, face: &str, weight: i32) -> f32 {
    match h {
        0 => 12.0,
        h if h < 0 => (-h) as f32,
        h => {
            let (upem, asc, desc, _) = design_metrics(face, if weight >= 600 { 1 } else { 0 })
                .unwrap_or((2048, 1854, 434, 2355));
            h as f32 * upem as f32 / (asc as f32 + desc as f32).max(1.0)
        }
    }
}

fn font_from_spec(spec: &gui::FontSpec) -> Font {
    let face = if spec.face.is_empty() {
        "Microsoft Sans Serif".to_string()
    } else {
        spec.face.clone()
    };
    let mut style = 0;
    if spec.weight >= 600 {
        style |= 1;
    }
    if spec.italic {
        style |= 2;
    }
    if spec.underline {
        style |= 4;
    }
    if spec.strikeout {
        style |= 8;
    }
    Font {
        size: em_of_height(spec.height, &face, spec.weight),
        family: face,
        style,
        unit: 2,
    }
}

unsafe extern "win64" fn create_font_from_dc(hdc: usize, f: *mut usize) -> i32 {
    let spec = gui::font_spec(gui::dc_font(hdc)).unwrap_or_default();
    out(f, add(Obj::Font(font_from_spec(&spec))))
}

/// Lit un LOGFONT (W : nom en UTF-16 ; A : nom en ANSI).
unsafe fn read_logfont(lf: *const u8, wide: bool) -> gui::FontSpec {
    let rd = |o: usize| (lf.add(o) as *const i32).read_unaligned();
    let face = if wide {
        let p = lf.add(28) as *const u16;
        let n = (0..LF_FACESIZE)
            .find(|&i| p.add(i).read_unaligned() == 0)
            .unwrap_or(LF_FACESIZE);
        String::from_utf16_lossy(
            &(0..n)
                .map(|i| p.add(i).read_unaligned())
                .collect::<Vec<_>>(),
        )
    } else {
        let p = lf.add(28);
        let n = (0..LF_FACESIZE)
            .find(|&i| *p.add(i) == 0)
            .unwrap_or(LF_FACESIZE);
        std::slice::from_raw_parts(p, n)
            .iter()
            .map(|&b| b as char)
            .collect()
    };
    gui::FontSpec {
        height: rd(0),
        width: rd(4),
        weight: rd(16),
        italic: *lf.add(20) != 0,
        underline: *lf.add(21) != 0,
        strikeout: *lf.add(22) != 0,
        face,
    }
}

unsafe extern "win64" fn create_font_from_logfont_w(
    _hdc: usize,
    lf: *const u8,
    f: *mut usize,
) -> i32 {
    if lf.is_null() {
        return INVALID_PARAMETER;
    }
    out(f, add(Obj::Font(font_from_spec(&read_logfont(lf, true)))))
}

unsafe extern "win64" fn create_font_from_logfont_a(
    _hdc: usize,
    lf: *const u8,
    f: *mut usize,
) -> i32 {
    if lf.is_null() {
        return INVALID_PARAMETER;
    }
    out(f, add(Obj::Font(font_from_spec(&read_logfont(lf, false)))))
}

unsafe extern "win64" fn delete_font(f: usize) -> i32 {
    match font(f) {
        Some(_) => remove(f),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn clone_font(f: usize, c: *mut usize) -> i32 {
    match font(f) {
        Some(v) => out(c, add(Obj::Font(v))),
        None => INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn get_font_size(f: usize, v: *mut f32) -> i32 {
    match font(f) {
        Some(x) => out(v, x.size),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn get_font_style(f: usize, v: *mut i32) -> i32 {
    match font(f) {
        Some(x) => out(v, x.style),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn get_font_unit(f: usize, v: *mut i32) -> i32 {
    match font(f) {
        Some(x) => out(v, x.unit),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn get_family(f: usize, fam: *mut usize) -> i32 {
    match font(f) {
        Some(x) => out(fam, add(Obj::Family(x.family))),
        None => INVALID_PARAMETER,
    }
}

/// Interligne rapporté à l'em.
fn spacing_ratio(f: &Font) -> f32 {
    match design_metrics(&f.family, f.style) {
        Some((upem, _, _, sp)) => sp as f32 / upem.max(1) as f32,
        None => 1.15,
    }
}

unsafe extern "win64" fn get_font_height(f: usize, g: usize, v: *mut f32) -> i32 {
    let Some(x) = font(f) else {
        return INVALID_PARAMETER;
    };
    let gr = graphics(g);
    out(v, font_em(&x, gr.as_ref()) * spacing_ratio(&x))
}

unsafe extern "win64" fn get_font_height_given_dpi(f: usize, dpi: f32, v: *mut f32) -> i32 {
    let Some(x) = font(f) else {
        return INVALID_PARAMETER;
    };
    let px = match x.unit {
        3 => x.size * dpi / 72.0,
        4 => x.size * dpi,
        5 => x.size * dpi / 300.0,
        6 => x.size * dpi / 25.4,
        _ => x.size,
    };
    out(v, px * spacing_ratio(&x))
}

unsafe fn write_logfont(f: &Font, gr: Option<&Graphics>, lf: *mut u8, wide: bool) {
    let size = if wide { 92 } else { 60 };
    std::ptr::write_bytes(lf, 0, size);
    let em = font_em(f, gr) * gr.map_or(1.0, device_scale);
    let wr = |o: usize, v: i32| (lf.add(o) as *mut i32).write_unaligned(v);
    wr(0, -(em.round() as i32).max(1));
    wr(16, if f.style & 1 != 0 { 700 } else { 400 });
    *lf.add(20) = (f.style & 2 != 0) as u8;
    *lf.add(21) = (f.style & 4 != 0) as u8;
    *lf.add(22) = (f.style & 8 != 0) as u8;
    *lf.add(23) = 1; // DEFAULT_CHARSET
    *lf.add(26) = 5; // CLEARTYPE_QUALITY
    if wide {
        let p = lf.add(28) as *mut u16;
        for (i, u) in f.family.encode_utf16().take(LF_FACESIZE - 1).enumerate() {
            p.add(i).write_unaligned(u);
        }
    } else {
        for (i, c) in f.family.chars().take(LF_FACESIZE - 1).enumerate() {
            *lf.add(28 + i) = if (c as u32) < 256 { c as u8 } else { b'?' };
        }
    }
}

unsafe extern "win64" fn get_log_font_w(f: usize, g: usize, lf: *mut u8) -> i32 {
    let Some(x) = font(f) else {
        return INVALID_PARAMETER;
    };
    if lf.is_null() {
        return INVALID_PARAMETER;
    }
    write_logfont(&x, graphics(g).as_ref(), lf, true);
    OK
}

unsafe extern "win64" fn get_log_font_a(f: usize, g: usize, lf: *mut u8) -> i32 {
    let Some(x) = font(f) else {
        return INVALID_PARAMETER;
    };
    if lf.is_null() {
        return INVALID_PARAMETER;
    }
    write_logfont(&x, graphics(g).as_ref(), lf, false);
    OK
}

// --- Formats de chaîne ------------------------------------------------------------------

fn default_format() -> StringFormat {
    StringFormat {
        trimming: 1,
        ..Default::default()
    }
}

fn typographic_format() -> StringFormat {
    StringFormat {
        flags: 0x0004 | SF_LINE_LIMIT | SF_NO_CLIP,
        typographic: true,
        ..Default::default()
    }
}

unsafe extern "win64" fn create_string_format(flags: i32, _lang: u16, f: *mut usize) -> i32 {
    out(
        f,
        add(Obj::Format(StringFormat {
            flags,
            ..default_format()
        })),
    )
}
unsafe extern "win64" fn generic_default(f: *mut usize) -> i32 {
    out(f, add(Obj::Format(default_format())))
}
unsafe extern "win64" fn generic_typographic(f: *mut usize) -> i32 {
    out(f, add(Obj::Format(typographic_format())))
}
unsafe extern "win64" fn delete_string_format(f: usize) -> i32 {
    match string_format(f) {
        Some(_) => remove(f),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn clone_string_format(f: usize, c: *mut usize) -> i32 {
    match string_format(f) {
        Some(v) => out(c, add(Obj::Format(v))),
        None => INVALID_PARAMETER,
    }
}

fn with_format(f: usize, op: impl FnOnce(&mut StringFormat)) -> i32 {
    with(f, |o| match o {
        Obj::Format(s) => {
            op(s);
            OK
        }
        _ => INVALID_PARAMETER,
    })
    .unwrap_or(INVALID_PARAMETER)
}

macro_rules! format_field {
    ($set:ident, $get:ident, $field:ident) => {
        unsafe extern "win64" fn $set(f: usize, v: i32) -> i32 {
            with_format(f, |s| s.$field = v)
        }
        unsafe extern "win64" fn $get(f: usize, v: *mut i32) -> i32 {
            match string_format(f) {
                Some(s) => out(v, s.$field),
                None => INVALID_PARAMETER,
            }
        }
    };
}

format_field!(set_align, get_align, align);
format_field!(set_line_align, get_line_align, line_align);
format_field!(set_flags, get_flags, flags);
format_field!(set_trimming, get_trimming, trimming);
format_field!(set_hotkey, get_hotkey, hotkey);

unsafe extern "win64" fn set_tab_stops(f: usize, _first: f32, _n: i32, _tabs: *const f32) -> i32 {
    with_format(f, |_| {})
}
unsafe extern "win64" fn get_tab_stop_count(f: usize, n: *mut i32) -> i32 {
    match string_format(f) {
        Some(_) => out(n, 0),
        None => INVALID_PARAMETER,
    }
}
unsafe extern "win64" fn set_digit_substitution(f: usize, _lang: u16, _sub: i32) -> i32 {
    with_format(f, |_| {})
}

unsafe extern "win64" fn set_ranges(f: usize, n: i32, r: *const i32) -> i32 {
    if n < 0 || (n > 0 && r.is_null()) || n > 32 {
        return INVALID_PARAMETER;
    }
    let v: Vec<(i32, i32)> = (0..n as usize)
        .map(|i| (*r.add(2 * i), *r.add(2 * i + 1)))
        .collect();
    with_format(f, |s| s.ranges = v)
}
unsafe extern "win64" fn get_range_count(f: usize, n: *mut i32) -> i32 {
    match string_format(f) {
        Some(s) => out(n, s.ranges.len() as i32),
        None => INVALID_PARAMETER,
    }
}

// --- Mise en page -----------------------------------------------------------------------

/// Un caractère placé : indice UTF-16 dans le texte d'origine, caractère,
/// abscisse et avance en pixels dans la ligne.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placed {
    pub(crate) idx: usize,
    pub(crate) ch: char,
    pub(crate) x: f32,
    pub(crate) adv: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Line {
    pub(crate) chars: Vec<Placed>,
    /// Largeur mesurée (sans les espaces finaux, sauf MeasureTrailingSpaces).
    pub(crate) width: f32,
    /// Décalage horizontal de la ligne dans la boîte, en pixels.
    pub(crate) x: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    pub(crate) lines: Vec<Line>,
    pub(crate) line_h: f32,
    /// Décalage vertical du bloc dans la boîte.
    pub(crate) y: f32,
    pub(crate) pad: f32,
    /// Unités UTF-16 du texte d'origine couvertes par les lignes visibles.
    pub(crate) fitted: usize,
    /// Caractère souligné par le préfixe `&` (HotkeyPrefixShow).
    pub(crate) hotkey: Option<usize>,
}

impl Layout {
    /// Boîte englobante (x, y, l, h) en pixels, relative à l'origine.
    pub(crate) fn bounds(&self) -> (f32, f32, f32, f32) {
        if self.lines.is_empty() {
            return (0.0, self.y, 0.0, 0.0);
        }
        let l = self
            .lines
            .iter()
            .map(|ln| ln.x - self.pad)
            .fold(f32::MAX, f32::min);
        let r = self
            .lines
            .iter()
            .map(|ln| ln.x + ln.width + self.pad)
            .fold(f32::MIN, f32::max);
        (l, self.y, r - l, self.lines.len() as f32 * self.line_h)
    }
}

/// Coupe et place le texte. `adv` donne l'avance d'un caractère en pixels ;
/// `bw`/`bh` : taille de la boîte en pixels (0 : non bornée).
pub(crate) fn layout(
    text: &[u16],
    fmt: &StringFormat,
    em: f32,
    line_h: f32,
    bw: f32,
    bh: f32,
    adv: &dyn Fn(char) -> f32,
) -> Layout {
    let pad = if fmt.typographic { 0.0 } else { em / 6.0 };
    // 1. Décodage et préfixes `&`.
    let mut chars: Vec<(usize, char)> = Vec::new();
    let mut hotkey = None;
    let mut it = char::decode_utf16(text.iter().copied()).map(|r| r.unwrap_or('\u{FFFD}'));
    let mut idx = 0usize;
    while let Some(c) = it.next() {
        let here = idx;
        idx += c.len_utf16();
        if c == '&' && fmt.hotkey != 0 {
            if let Some(n) = it.next() {
                let at = idx;
                idx += n.len_utf16();
                if n != '&' && fmt.hotkey == 1 && hotkey.is_none() {
                    hotkey = Some(at);
                }
                chars.push((at, n));
            }
            continue;
        }
        chars.push((here, c));
    }
    let total = idx;
    // 2. Paragraphes et coupure en lignes : (début, fin, fin consommée).
    let wrap = fmt.flags & SF_NO_WRAP == 0 && bw > 0.0;
    let avail = (bw - 2.0 * pad).max(0.0);
    let mut lines: Vec<(Vec<Placed>, usize)> = Vec::new();
    let mut para: Vec<Placed> = Vec::new();
    let flush = |para: &mut Vec<Placed>, end: usize, lines: &mut Vec<(Vec<Placed>, usize)>| {
        if !wrap {
            let mut x = 0.0f32;
            for p in para.iter_mut() {
                p.x = x;
                x += p.adv;
            }
            lines.push((std::mem::take(para), end));
            return;
        }
        let mut cur: Vec<Placed> = Vec::new();
        let mut x = 0.0f32;
        let mut brk: Option<usize> = None;
        for p in para.drain(..) {
            let fits = x + p.adv <= avail + 0.01 || p.ch == ' ';
            if !fits && !cur.is_empty() {
                let cut = brk.unwrap_or(cur.len());
                let rest: Vec<Placed> = cur.split_off(cut);
                let consumed = rest.first().map_or(p.idx, |r| r.idx);
                lines.push((std::mem::take(&mut cur), consumed));
                x = 0.0;
                for mut r in rest {
                    r.x = x;
                    x += r.adv;
                    cur.push(r);
                }
                brk = None;
            }
            let mut p = p;
            p.x = x;
            x += p.adv;
            let space = p.ch == ' ' || p.ch == '\t';
            cur.push(p);
            if space {
                brk = Some(cur.len());
            }
        }
        lines.push((cur, end));
    };
    for (i, c) in chars {
        match c {
            '\r' => {}
            '\n' => flush(&mut para, i + 1, &mut lines),
            c => para.push(Placed {
                idx: i,
                ch: c,
                x: 0.0,
                adv: adv(c),
            }),
        }
    }
    if !para.is_empty() || lines.is_empty() || text.last() == Some(&(b'\n' as u16)) {
        flush(&mut para, total, &mut lines);
    }
    if text.is_empty() {
        lines.clear();
    }
    // 3. Hauteur disponible.
    let mut visible = lines.len();
    let mut truncated = false;
    if bh > 0.0 && line_h > 0.0 {
        let n = if fmt.flags & SF_LINE_LIMIT != 0 {
            (bh / line_h + 0.001).floor() as usize
        } else {
            (bh / line_h - 0.001).ceil().max(1.0) as usize
        };
        if n < visible {
            visible = n;
            truncated = true;
        }
    }
    lines.truncate(visible);
    let fitted = if truncated {
        lines.last().map_or(0, |l| l.1)
    } else {
        total
    };
    // 4. Points de suspension.
    let ell = adv('\u{2026}');
    let mut out: Vec<Line> = Vec::new();
    let n = lines.len();
    for (k, (mut cs, _)) in lines.into_iter().enumerate() {
        let measure = |cs: &[Placed]| -> f32 {
            let end = if fmt.flags & SF_TRAILING_SPACES != 0 {
                cs.len()
            } else {
                cs.iter()
                    .rposition(|p| p.ch != ' ' && p.ch != '\t')
                    .map_or(0, |i| i + 1)
            };
            cs[..end].last().map_or(0.0, |p| p.x + p.adv)
        };
        let mut w = measure(&cs);
        let overflow = bw > 0.0 && w > avail + 0.01;
        if fmt.trimming >= 3 && (overflow || (truncated && k + 1 == n)) {
            while cs
                .last()
                .is_some_and(|p| p.x + p.adv + ell > avail + 0.01 || p.ch == ' ')
            {
                cs.pop();
            }
            let x = cs.last().map_or(0.0, |p| p.x + p.adv);
            let idx = cs.last().map_or(0, |p| p.idx);
            cs.push(Placed {
                idx,
                ch: '\u{2026}',
                x,
                adv: ell,
            });
            w = x + ell;
        } else if overflow && fmt.trimming == 2 {
            // Coupe au mot.
            if let Some(sp) = cs.iter().rposition(|p| p.ch == ' ' && p.x <= avail) {
                cs.truncate(sp);
                w = measure(&cs);
            }
        }
        out.push(Line {
            chars: cs,
            width: w,
            x: 0.0,
        });
    }
    // 5. Alignements (proche, centre, loin ; inversés en RTL).
    let mut align = fmt.align;
    if fmt.flags & SF_RTL != 0 && align != 1 {
        align = 2 - align;
    }
    for ln in &mut out {
        ln.x = match (align, bw > 0.0) {
            (1, true) => pad + (avail - ln.width) / 2.0,
            (2, true) => bw - pad - ln.width,
            (1, false) => -ln.width / 2.0,
            (2, false) => -ln.width - pad,
            _ => pad,
        };
    }
    let th = out.len() as f32 * line_h;
    let y = match (fmt.line_align, bh > 0.0) {
        (1, true) => (bh - th) / 2.0,
        (2, true) => bh - th,
        (1, false) => -th / 2.0,
        (2, false) => -th,
        _ => 0.0,
    };
    Layout {
        lines: out,
        line_h,
        y,
        pad,
        fitted,
        hotkey,
    }
}

/// Facteur monde → pixels du Graphics (moyenne géométrique des axes).
fn device_scale(g: &Graphics) -> f32 {
    let m = g.device();
    (m.m11 * m.m22 - m.m12 * m.m21).abs().sqrt().max(1e-6)
}

struct Prepared {
    spec: gui::FontSpec,
    lay: Layout,
    /// Pixels par unité monde.
    s: f32,
    em_px: f32,
    ascent: f32,
}

unsafe fn prepare(
    g: &Graphics,
    text: *const u16,
    len: i32,
    f: &Font,
    fmt: &StringFormat,
    rect: [f32; 4],
) -> Option<Prepared> {
    if text.is_null() {
        return None;
    }
    let n = if len < 0 {
        crate::printf::wide_len(text)
    } else {
        len as usize
    };
    let text = std::slice::from_raw_parts(text, n);
    let s = device_scale(g);
    let em_px = font_em(f, Some(g)) * s;
    let spec = font_spec(f, em_px);
    let gf = usp10::spec_font(&spec)?;
    let m = &gf.font.metrics;
    let upem = m.units_per_em.max(1) as f32;
    let k = gf.em / upem;
    let line_h = (m.ascent as f32 + m.descent as f32 + (m.line_gap as f32).max(0.0)) * k;
    let ascent = (m.ascent as f32 * k).ceil();
    let adv = |c: char| gf.advance_px(gf.font.glyph_index(c as u32)) as f32;
    let lay = layout(
        text,
        fmt,
        em_px,
        line_h.max(1.0),
        rect[2] * s,
        rect[3] * s,
        &adv,
    );
    Some(Prepared {
        spec,
        lay,
        s,
        em_px,
        ascent,
    })
}

unsafe fn read_rect(r: *const f32) -> [f32; 4] {
    if r.is_null() {
        return [0.0; 4];
    }
    [*r, *r.add(1), (*r.add(2)).max(0.0), (*r.add(3)).max(0.0)]
}

unsafe extern "win64" fn draw_string(
    g: usize,
    text: *const u16,
    len: i32,
    f: usize,
    rect: *const f32,
    fmt: usize,
    b: usize,
) -> i32 {
    let (Some(gr), Some(fo)) = (graphics(g), font(f)) else {
        return INVALID_PARAMETER;
    };
    if rect.is_null() {
        return INVALID_PARAMETER;
    }
    let sf = if fmt == 0 {
        default_format()
    } else {
        string_format(fmt).unwrap_or_else(default_format)
    };
    let r = read_rect(rect);
    let Some(p) = prepare(&gr, text, len, &fo, &sf, r) else {
        return if text.is_null() {
            INVALID_PARAMETER
        } else {
            OK
        };
    };
    let mut glyphs = Vec::new();
    let mut rects = Vec::new();
    let thick = (p.em_px / 14.0).max(1.0);
    for (i, ln) in p.lay.lines.iter().enumerate() {
        let top = p.lay.y + i as f32 * p.lay.line_h;
        let line: String = ln.chars.iter().map(|c| c.ch).collect();
        if let Some((gs, _, _)) = usp10::layout_pixels(&p.spec, &line) {
            for (gx, gy, ras) in gs {
                glyphs.push((
                    r[0] + (ln.x + gx as f32) / p.s,
                    r[1] + (top + gy as f32) / p.s,
                    ras,
                ));
            }
        }
        let base = top + p.ascent;
        let mut deco = |x: f32, w: f32, y: f32| {
            rects.push((r[0] + x / p.s, r[1] + y / p.s, w / p.s, thick / p.s));
        };
        if fo.style & 4 != 0 && ln.width > 0.0 {
            deco(ln.x, ln.width, base + thick);
        }
        if fo.style & 8 != 0 && ln.width > 0.0 {
            deco(ln.x, ln.width, base - p.em_px * 0.3);
        }
        if let Some(h) = p.lay.hotkey {
            if let Some(c) = ln.chars.iter().find(|c| c.idx == h && c.ch != '\u{2026}') {
                deco(ln.x + c.x, c.adv, base + thick);
            }
        }
    }
    let clip = (sf.flags & SF_NO_CLIP == 0 && r[2] > 0.0 && r[3] > 0.0).then_some(r);
    draw_glyphs(g, b, &glyphs, &rects, clip)
}

unsafe extern "win64" fn measure_string(
    g: usize,
    text: *const u16,
    len: i32,
    f: usize,
    rect: *const f32,
    fmt: usize,
    bbox: *mut f32,
    fitted: *mut i32,
    lines: *mut i32,
) -> i32 {
    let (Some(gr), Some(fo)) = (graphics(g), font(f)) else {
        return INVALID_PARAMETER;
    };
    if rect.is_null() || bbox.is_null() || text.is_null() {
        return INVALID_PARAMETER;
    }
    let sf = if fmt == 0 {
        default_format()
    } else {
        string_format(fmt).unwrap_or_else(default_format)
    };
    let r = read_rect(rect);
    let Some(p) = prepare(&gr, text, len, &fo, &sf, r) else {
        return GENERIC_ERROR;
    };
    let (x, y, w, h) = p.lay.bounds();
    *bbox = r[0] + x / p.s;
    *bbox.add(1) = r[1] + y / p.s;
    *bbox.add(2) = w / p.s;
    *bbox.add(3) = h / p.s;
    if !fitted.is_null() {
        *fitted = p.lay.fitted as i32;
    }
    if !lines.is_null() {
        *lines = p.lay.lines.len() as i32;
    }
    OK
}

/// Rectangle (pixels, relatif à l'origine) couvert par une plage de caractères.
pub(crate) fn range_box(lay: &Layout, first: i32, length: i32) -> Option<Clip> {
    let (a, b) = if length >= 0 {
        (first, first + length)
    } else {
        (first + length, first)
    };
    let (a, b) = (a.max(0) as usize, b.max(0) as usize);
    let mut bb: Option<Clip> = None;
    for (i, ln) in lay.lines.iter().enumerate() {
        let top = lay.y + i as f32 * lay.line_h;
        for c in ln
            .chars
            .iter()
            .filter(|c| c.idx >= a && c.idx < b && c.ch != '\u{2026}')
        {
            let cl = Clip {
                l: ln.x + c.x,
                t: top,
                r: ln.x + c.x + c.adv,
                b: top + lay.line_h,
            };
            bb = Some(match bb {
                Some(o) => Clip {
                    l: o.l.min(cl.l),
                    t: o.t.min(cl.t),
                    r: o.r.max(cl.r),
                    b: o.b.max(cl.b),
                },
                None => cl,
            });
        }
    }
    bb
}

unsafe extern "win64" fn measure_character_ranges(
    g: usize,
    text: *const u16,
    len: i32,
    f: usize,
    rect: *const f32,
    fmt: usize,
    count: i32,
    regions: *const usize,
) -> i32 {
    let (Some(gr), Some(fo), Some(sf)) = (graphics(g), font(f), string_format(fmt)) else {
        return INVALID_PARAMETER;
    };
    if rect.is_null() || text.is_null() || count < 0 || (count > 0 && regions.is_null()) {
        return INVALID_PARAMETER;
    }
    if (count as usize) < sf.ranges.len() {
        return INVALID_PARAMETER;
    }
    let r = read_rect(rect);
    let Some(p) = prepare(&gr, text, len, &fo, &sf, r) else {
        return GENERIC_ERROR;
    };
    for (i, &(first, length)) in sf.ranges.iter().enumerate() {
        let reg = *regions.add(i);
        let v = range_box(&p.lay, first, length).map(|c| Clip {
            l: r[0] + c.l / p.s,
            t: r[1] + c.t / p.s,
            r: r[0] + c.r / p.s,
            b: r[1] + c.b / p.s,
        });
        // Plage vide : région vide.
        let v = v.unwrap_or(Clip {
            l: 0.0,
            t: 0.0,
            r: 0.0,
            b: 0.0,
        });
        let ok = with(reg, |o| match o {
            Obj::Region(x) => {
                *x = Some(v);
                true
            }
            _ => false,
        });
        if ok != Some(true) {
            return INVALID_PARAMETER;
        }
    }
    OK
}

pub(super) fn exports() -> Exports {
    crate::exports! {
        "GdipCreateFontFamilyFromName" => fn create_font_family_from_name,
        "GdipGetGenericFontFamilySansSerif" => fn generic_sans,
        "GdipGetGenericFontFamilySerif" => fn generic_serif,
        "GdipGetGenericFontFamilyMonospace" => fn generic_mono,
        "GdipDeleteFontFamily" => fn delete_font_family,
        "GdipCloneFontFamily" => fn clone_font_family,
        "GdipGetFamilyName" => fn get_family_name,
        "GdipIsStyleAvailable" => fn is_style_available,
        "GdipGetEmHeight" => fn get_em_height,
        "GdipGetCellAscent" => fn get_cell_ascent,
        "GdipGetCellDescent" => fn get_cell_descent,
        "GdipGetLineSpacing" => fn get_line_spacing,
        "GdipNewInstalledFontCollection" => fn new_font_collection,
        "GdipNewPrivateFontCollection" => fn new_font_collection,
        "GdipDeletePrivateFontCollection" => fn delete_private_collection,
        "GdipGetFontCollectionFamilyCount" => fn collection_family_count,
        "GdipGetFontCollectionFamilyList" => fn collection_family_list,
        "GdipPrivateAddFontFile" => fn private_add_font_file,
        "GdipPrivateAddMemoryFont" => fn private_add_memory_font,
        "GdipCreateFont" => fn create_font,
        "GdipCreateFontFromDC" => fn create_font_from_dc,
        "GdipCreateFontFromLogfontW" => fn create_font_from_logfont_w,
        "GdipCreateFontFromLogfontA" => fn create_font_from_logfont_a,
        "GdipDeleteFont" => fn delete_font,
        "GdipCloneFont" => fn clone_font,
        "GdipGetFontSize" => fn get_font_size,
        "GdipGetFontStyle" => fn get_font_style,
        "GdipGetFontUnit" => fn get_font_unit,
        "GdipGetFamily" => fn get_family,
        "GdipGetFontHeight" => fn get_font_height,
        "GdipGetFontHeightGivenDPI" => fn get_font_height_given_dpi,
        "GdipGetLogFontW" => fn get_log_font_w,
        "GdipGetLogFontA" => fn get_log_font_a,
        "GdipCreateStringFormat" => fn create_string_format,
        "GdipStringFormatGetGenericDefault" => fn generic_default,
        "GdipStringFormatGetGenericTypographic" => fn generic_typographic,
        "GdipDeleteStringFormat" => fn delete_string_format,
        "GdipCloneStringFormat" => fn clone_string_format,
        "GdipSetStringFormatAlign" => fn set_align,
        "GdipGetStringFormatAlign" => fn get_align,
        "GdipSetStringFormatLineAlign" => fn set_line_align,
        "GdipGetStringFormatLineAlign" => fn get_line_align,
        "GdipSetStringFormatFlags" => fn set_flags,
        "GdipGetStringFormatFlags" => fn get_flags,
        "GdipSetStringFormatTrimming" => fn set_trimming,
        "GdipGetStringFormatTrimming" => fn get_trimming,
        "GdipSetStringFormatHotkeyPrefix" => fn set_hotkey,
        "GdipGetStringFormatHotkeyPrefix" => fn get_hotkey,
        "GdipSetStringFormatTabStops" => fn set_tab_stops,
        "GdipGetStringFormatTabStopCount" => fn get_tab_stop_count,
        "GdipSetStringFormatDigitSubstitution" => fn set_digit_substitution,
        "GdipSetStringFormatMeasurableCharacterRanges" => fn set_ranges,
        "GdipGetStringFormatMeasurableCharacterRangeCount" => fn get_range_count,
        "GdipDrawString" => fn draw_string,
        "GdipMeasureString" => fn measure_string,
        "GdipMeasureCharacterRanges" => fn measure_character_ranges,
    }
}
