//! IDWriteTextFormat1 à 3 et IDWriteTextLayout1 à 4 (#1118).
//!
//! Paint.NET 5.1 (et tout programme passé par TerraFX) demande au format de
//! texte et à la mise en page leurs versions récentes. Les réglages ajoutés
//! (orientation verticale, dernière ligne, alignement optique, police de
//! repli, interligne étendu, axes de police, crénage par paires, espacement)
//! sont gardés et relus ; la mise en page n'utilise encore que ceux qu'elle
//! connaissait.
//!
//! Ordre des méthodes : `dwrite_1.h`, `dwrite_2.h` et `dwrite_3.h`
//! (MinGW-w64, domaine public).

use super::{
    iid, invalidate, layout_line_metrics, layout_metrics, method, text, Guid, LineMetrics,
    TextMetrics, TextObj, E_INVALIDARG, E_POINTER, S_OK,
};
use std::ffi::c_void;
use std::sync::Arc;

pub(super) const IID_TEXT_FORMAT1: Guid = iid(
    0x5f174b49,
    0x0d8b,
    0x4cfb,
    [0x8b, 0xca, 0xf1, 0xcc, 0xe9, 0xd0, 0x6c, 0x67],
);
pub(super) const IID_TEXT_FORMAT2: Guid = iid(
    0xf67e0edd,
    0x9e3d,
    0x4ecc,
    [0x8c, 0x32, 0x41, 0x83, 0x25, 0x3d, 0xfe, 0x70],
);
pub(super) const IID_TEXT_FORMAT3: Guid = iid(
    0x6d3b5641,
    0xe550,
    0x430d,
    [0xa8, 0x5b, 0xb7, 0xbf, 0x48, 0xa9, 0x34, 0x27],
);
pub(super) const IID_TEXT_LAYOUT1: Guid = iid(
    0x9064d822,
    0x80a7,
    0x465c,
    [0xa9, 0x86, 0xdf, 0x65, 0xf7, 0x8b, 0x8f, 0xeb],
);
pub(super) const IID_TEXT_LAYOUT2: Guid = iid(
    0x1093c18f,
    0x8d5e,
    0x43f0,
    [0xb0, 0x64, 0x09, 0x17, 0x31, 0x1b, 0x52, 0x5e],
);
pub(super) const IID_TEXT_LAYOUT3: Guid = iid(
    0x07ddcd52,
    0x020e,
    0x4de8,
    [0xac, 0x33, 0x6c, 0x95, 0x3d, 0x83, 0xf9, 0x2d],
);
pub(super) const IID_TEXT_LAYOUT4: Guid = iid(
    0x05a9bf42,
    0x223f,
    0x4441,
    [0xb5, 0xfb, 0x82, 0x63, 0x68, 0x5f, 0x55, 0xe9],
);

/// Interface tenue (AddRef à la pose, Release quand plus personne ne la garde).
pub(super) struct Held(usize);

impl Held {
    unsafe fn new(p: *mut c_void) -> Option<Arc<Held>> {
        if p.is_null() {
            return None;
        }
        method::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 1)(p);
        Some(Arc::new(Held(p as usize)))
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let p = self.0 as *mut c_void;
        unsafe { method::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 2)(p) };
    }
}

// Seules les données du programme y circulent, sous le verrou du format.
unsafe impl Send for Held {}
unsafe impl Sync for Held {}

/// DWRITE_FONT_AXIS_VALUE : étiquette OpenType et valeur.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct AxisValue {
    tag: u32,
    value: f32,
}

/// Réglages des versions 1 à 3 du format de texte.
#[derive(Clone)]
pub(super) struct FormatExt {
    vertical: u32,
    last_line_wrapping: bool,
    optical: u32,
    fallback: Option<Arc<Held>>,
    leading_before: f32,
    gap_usage: u32,
    axes: Vec<AxisValue>,
    automatic_axes: u32,
    pair_kerning: bool,
    spacing: [f32; 3],
}

impl Default for FormatExt {
    fn default() -> Self {
        FormatExt {
            vertical: 0,
            last_line_wrapping: true,
            optical: 0,
            fallback: None,
            leading_before: 0.0,
            gap_usage: 0,
            axes: Vec::new(),
            automatic_axes: 0,
            pair_kerning: false,
            spacing: [0.0; 3],
        }
    }
}

impl std::fmt::Debug for FormatExt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormatExt")
            .field("vertical", &self.vertical)
            .field("last_line_wrapping", &self.last_line_wrapping)
            .field("optical", &self.optical)
            .field("fallback", &self.fallback.as_ref().map(|h| h.0))
            .field("axes", &self.axes)
            .finish()
    }
}

/// DWRITE_LINE_SPACING (IDWriteTextFormat2, IDWriteTextLayout3).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LineSpacing {
    method: u32,
    height: f32,
    baseline: f32,
    leading_before: f32,
    gap_usage: u32,
}

/// DWRITE_TEXT_RANGE passé par valeur : position et longueur dans un registre.
fn whole_range(out: *mut [u32; 2]) {
    if !out.is_null() {
        unsafe { *out = [0, u32::MAX] };
    }
}

// --- Réglages communs au format (28-42) et à la mise en page (72-79) -------------

pub(super) unsafe extern "win64" fn set_vertical(this: *mut TextObj, v: u32) -> i32 {
    if v > 1 {
        return E_INVALIDARG;
    }
    text(this).fmt.ext.vertical = v;
    S_OK
}
pub(super) unsafe extern "win64" fn get_vertical(this: *mut TextObj) -> u32 {
    text(this).fmt.ext.vertical
}
pub(super) unsafe extern "win64" fn set_last_line(this: *mut TextObj, on: i32) -> i32 {
    text(this).fmt.ext.last_line_wrapping = on != 0;
    S_OK
}
pub(super) unsafe extern "win64" fn get_last_line(this: *mut TextObj) -> i32 {
    text(this).fmt.ext.last_line_wrapping as i32
}
pub(super) unsafe extern "win64" fn set_optical(this: *mut TextObj, v: u32) -> i32 {
    if v > 1 {
        return E_INVALIDARG;
    }
    text(this).fmt.ext.optical = v;
    S_OK
}
pub(super) unsafe extern "win64" fn get_optical(this: *mut TextObj) -> u32 {
    text(this).fmt.ext.optical
}
pub(super) unsafe extern "win64" fn set_fallback(this: *mut TextObj, fb: *mut c_void) -> i32 {
    let held = Held::new(fb);
    text(this).fmt.ext.fallback = held;
    S_OK
}
pub(super) unsafe extern "win64" fn get_fallback(this: *mut TextObj, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let p = text(this)
        .fmt
        .ext
        .fallback
        .as_ref()
        .map_or(std::ptr::null_mut(), |h| h.0 as *mut c_void);
    if !p.is_null() {
        method::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 1)(p);
    }
    *out = p;
    S_OK
}

/// SetLineSpacing(const DWRITE_LINE_SPACING*).
pub(super) unsafe extern "win64" fn set_spacing2(this: *mut TextObj, s: *const LineSpacing) -> i32 {
    let Some(s) = s.as_ref() else {
        return E_INVALIDARG;
    };
    if s.method > 2 || s.height < 0.0 || s.gap_usage > 2 {
        return E_INVALIDARG;
    }
    let mut t = text(this);
    t.fmt.line_spacing = (s.method, s.height, s.baseline);
    t.fmt.ext.leading_before = s.leading_before;
    t.fmt.ext.gap_usage = s.gap_usage;
    invalidate(&mut t);
    S_OK
}

/// GetLineSpacing(DWRITE_LINE_SPACING*).
pub(super) unsafe extern "win64" fn get_spacing2(this: *mut TextObj, s: *mut LineSpacing) -> i32 {
    if s.is_null() {
        return E_POINTER;
    }
    let t = text(this);
    let (method, height, baseline) = t.fmt.line_spacing;
    *s = LineSpacing {
        method,
        height,
        baseline,
        leading_before: t.fmt.ext.leading_before,
        gap_usage: t.fmt.ext.gap_usage,
    };
    S_OK
}

unsafe fn put_axes(this: *mut TextObj, v: *const AxisValue, n: u32) -> i32 {
    if v.is_null() && n != 0 {
        return E_INVALIDARG;
    }
    let axes = if n == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(v, n.min(64) as usize).to_vec()
    };
    let mut t = text(this);
    // L'axe de graisse choisit aussi la graisse demandée.
    if let Some(w) = axes.iter().find(|a| a.tag == u32::from_le_bytes(*b"wght")) {
        t.fmt.weight = w.value.clamp(1.0, 999.0) as u32;
    }
    t.fmt.ext.axes = axes;
    invalidate(&mut t);
    S_OK
}

unsafe fn copy_axes(this: *mut TextObj, v: *mut AxisValue, n: u32) -> i32 {
    let axes = text(this).fmt.ext.axes.clone();
    if (n as usize) < axes.len() {
        return E_INVALIDARG;
    }
    if !axes.is_empty() {
        if v.is_null() {
            return E_POINTER;
        }
        std::ptr::copy_nonoverlapping(axes.as_ptr(), v, axes.len());
    }
    S_OK
}

pub(super) unsafe extern "win64" fn format_set_axes(
    this: *mut TextObj,
    v: *const AxisValue,
    n: u32,
) -> i32 {
    put_axes(this, v, n)
}
pub(super) unsafe extern "win64" fn format_axis_count(this: *mut TextObj) -> u32 {
    text(this).fmt.ext.axes.len() as u32
}
pub(super) unsafe extern "win64" fn format_get_axes(
    this: *mut TextObj,
    v: *mut AxisValue,
    n: u32,
) -> i32 {
    copy_axes(this, v, n)
}
pub(super) unsafe extern "win64" fn get_automatic_axes(this: *mut TextObj) -> u32 {
    text(this).fmt.ext.automatic_axes
}
pub(super) unsafe extern "win64" fn set_automatic_axes(this: *mut TextObj, v: u32) -> i32 {
    if v > 1 {
        return E_INVALIDARG;
    }
    text(this).fmt.ext.automatic_axes = v;
    S_OK
}

// --- IDWriteTextLayout1 à 4 (67-88) -------------------------------------------------

pub(super) unsafe extern "win64" fn set_pair_kerning(
    this: *mut TextObj,
    on: i32,
    _range: u64,
) -> i32 {
    text(this).fmt.ext.pair_kerning = on != 0;
    S_OK
}
pub(super) unsafe extern "win64" fn get_pair_kerning(
    this: *mut TextObj,
    _pos: u32,
    on: *mut i32,
    range: *mut [u32; 2],
) -> i32 {
    if on.is_null() {
        return E_POINTER;
    }
    *on = text(this).fmt.ext.pair_kerning as i32;
    whole_range(range);
    S_OK
}
pub(super) unsafe extern "win64" fn set_char_spacing(
    this: *mut TextObj,
    leading: f32,
    trailing: f32,
    min_advance: f32,
    _range: u64,
) -> i32 {
    if min_advance < 0.0 {
        return E_INVALIDARG;
    }
    text(this).fmt.ext.spacing = [leading, trailing, min_advance];
    S_OK
}
pub(super) unsafe extern "win64" fn get_char_spacing(
    this: *mut TextObj,
    _pos: u32,
    leading: *mut f32,
    trailing: *mut f32,
    min_advance: *mut f32,
    range: *mut [u32; 2],
) -> i32 {
    if leading.is_null() || trailing.is_null() || min_advance.is_null() {
        return E_POINTER;
    }
    let [l, t, m] = text(this).fmt.ext.spacing;
    (*leading, *trailing, *min_advance) = (l, t, m);
    whole_range(range);
    S_OK
}

/// DWRITE_TEXT_METRICS1 : DWRITE_TEXT_METRICS puis heightIncludingTrailingWhitespace.
#[repr(C)]
pub(super) struct TextMetrics1 {
    base: TextMetrics,
    height_ws: f32,
}

pub(super) unsafe extern "win64" fn layout_metrics1(
    this: *mut TextObj,
    out: *mut TextMetrics1,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let hr = layout_metrics(this, &mut (*out).base);
    if hr >= 0 {
        (*out).height_ws = (*out).base.height;
    }
    hr
}

pub(super) unsafe extern "win64" fn invalidate_layout(this: *mut TextObj) -> i32 {
    invalidate(&mut text(this));
    S_OK
}

/// DWRITE_LINE_METRICS1 : DWRITE_LINE_METRICS puis leadingBefore et leadingAfter.
#[repr(C)]
pub(super) struct LineMetrics1 {
    base: LineMetrics,
    leading_before: f32,
    leading_after: f32,
}

pub(super) unsafe extern "win64" fn layout_line_metrics1(
    this: *mut TextObj,
    out: *mut LineMetrics1,
    max: u32,
    actual: *mut u32,
) -> i32 {
    let mut count = 0u32;
    let hr = layout_line_metrics(this, std::ptr::null_mut(), 0, &mut count);
    if !actual.is_null() {
        *actual = count;
    }
    if hr < 0 && count == 0 {
        return hr;
    }
    let mut v: Vec<LineMetrics> = (0..count)
        .map(|_| LineMetrics {
            length: 0,
            trailing_ws: 0,
            newline: 0,
            height: 0.0,
            baseline: 0.0,
            trimmed: 0,
        })
        .collect();
    let hr = layout_line_metrics(this, v.as_mut_ptr(), count, &mut count);
    if hr < 0 {
        return hr;
    }
    if (max as usize) < v.len() || out.is_null() {
        return super::E_NOT_SUFFICIENT_BUFFER;
    }
    for (i, m) in v.into_iter().enumerate() {
        out.add(i).write(LineMetrics1 {
            base: m,
            leading_before: 0.0,
            leading_after: 0.0,
        });
    }
    S_OK
}

pub(super) unsafe extern "win64" fn layout_set_axes(
    this: *mut TextObj,
    v: *const AxisValue,
    n: u32,
    _range: u64,
) -> i32 {
    put_axes(this, v, n)
}
pub(super) unsafe extern "win64" fn layout_axis_count(this: *mut TextObj, _pos: u32) -> u32 {
    text(this).fmt.ext.axes.len() as u32
}
pub(super) unsafe extern "win64" fn layout_get_axes(
    this: *mut TextObj,
    _pos: u32,
    v: *mut AxisValue,
    n: u32,
    range: *mut [u32; 2],
) -> i32 {
    whole_range(range);
    copy_axes(this, v, n)
}

#[cfg(test)]
mod tests {
    use super::super::{create_text_format, create_text_layout, FORMAT_VT, LAYOUT_VT};
    use super::*;

    type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
    type Unary = unsafe extern "win64" fn(*mut c_void) -> u32;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    #[test]
    fn tables_have_windows_sizes() {
        // IDWriteTextFormat3 : 43 méthodes ; IDWriteTextLayout4 : 89.
        assert_eq!(FORMAT_VT.0.len(), 43);
        assert_eq!(LAYOUT_VT.0.len(), 89);
        assert_eq!(std::mem::size_of::<LineSpacing>(), 20);
        assert_eq!(std::mem::size_of::<TextMetrics1>(), 40);
        assert_eq!(std::mem::size_of::<LineMetrics1>(), 32);
        assert_eq!(std::mem::size_of::<AxisValue>(), 8);
    }

    #[test]
    fn format3_and_layout4_answer_and_keep_their_settings() {
        unsafe {
            let family = wide("Segoe UI");
            let locale = wide("fr-fr");
            let mut fmt = std::ptr::null_mut();
            assert_eq!(
                create_text_format(
                    std::ptr::null_mut(),
                    family.as_ptr(),
                    std::ptr::null_mut(),
                    400,
                    0,
                    5,
                    12.0,
                    locale.as_ptr(),
                    &mut fmt
                ),
                S_OK
            );
            for iid in [IID_TEXT_FORMAT1, IID_TEXT_FORMAT2, IID_TEXT_FORMAT3] {
                let mut p = std::ptr::null_mut();
                assert_eq!(method::<Qi>(fmt, 0)(fmt, &iid, &mut p), S_OK);
                assert_eq!(p, fmt);
                method::<Unary>(p, 2)(p);
            }
            // IDWriteTextFormat2::SetLineSpacing puis GetLineSpacing.
            let s = LineSpacing {
                method: 1,
                height: 20.0,
                baseline: 16.0,
                leading_before: 0.5,
                gap_usage: 1,
            };
            type SetS = unsafe extern "win64" fn(*mut c_void, *const LineSpacing) -> i32;
            type GetS = unsafe extern "win64" fn(*mut c_void, *mut LineSpacing) -> i32;
            assert_eq!(method::<SetS>(fmt, 36)(fmt, &s), S_OK);
            let mut g = LineSpacing::default();
            assert_eq!(method::<GetS>(fmt, 37)(fmt, &mut g), S_OK);
            assert_eq!(
                (g.method, g.height, g.leading_before, g.gap_usage),
                (1, 20.0, 0.5, 1)
            );
            // IDWriteTextFormat3 : l'axe de graisse choisit la graisse.
            let axes = [AxisValue {
                tag: u32::from_le_bytes(*b"wght"),
                value: 700.0,
            }];
            type SetA = unsafe extern "win64" fn(*mut c_void, *const AxisValue, u32) -> i32;
            assert_eq!(method::<SetA>(fmt, 38)(fmt, axes.as_ptr(), 1), S_OK);
            assert_eq!(method::<Unary>(fmt, 39)(fmt), 1);
            type GetW = unsafe extern "win64" fn(*mut c_void) -> u32;
            assert_eq!(method::<GetW>(fmt, 22)(fmt), 700);

            let txt: Vec<u16> = "Couleurs".encode_utf16().collect();
            let mut lay = std::ptr::null_mut();
            assert_eq!(
                create_text_layout(
                    std::ptr::null_mut(),
                    txt.as_ptr(),
                    txt.len() as u32,
                    fmt,
                    200.0,
                    50.0,
                    &mut lay
                ),
                S_OK
            );
            for iid in [
                IID_TEXT_LAYOUT1,
                IID_TEXT_LAYOUT2,
                IID_TEXT_LAYOUT3,
                IID_TEXT_LAYOUT4,
            ] {
                let mut p = std::ptr::null_mut();
                assert_eq!(method::<Qi>(lay, 0)(lay, &iid, &mut p), S_OK);
                method::<Unary>(p, 2)(p);
            }
            // IDWriteTextFormat1-3 d'une mise en page : une vue dont les
            // emplacements sont ceux de IDWriteTextFormat3 (#1158, Paint.NET).
            let mut view = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(lay, 0)(lay, &IID_TEXT_FORMAT3, &mut view),
                S_OK
            );
            assert!(!view.is_null() && view != lay);
            let mut again = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(lay, 0)(lay, &IID_TEXT_FORMAT1, &mut again),
                S_OK
            );
            assert_eq!(again, view);
            method::<Unary>(again, 2)(again);
            let mut g = LineSpacing::default();
            assert_eq!(method::<GetS>(view, 37)(view, &mut g), S_OK);
            assert_eq!(method::<GetW>(view, 22)(view), 700);
            assert_eq!(method::<Unary>(view, 39)(view), 1);
            let mut back = std::ptr::null_mut();
            assert_eq!(
                method::<Qi>(view, 0)(view, &IID_TEXT_LAYOUT4, &mut back),
                S_OK
            );
            assert_eq!(back, lay);
            method::<Unary>(back, 2)(back);
            method::<Unary>(view, 2)(view);
            // IDWriteTextLayout2::GetMetrics et IDWriteTextLayout3::GetLineMetrics.
            let mut m: TextMetrics1 = std::mem::zeroed();
            type GetM = unsafe extern "win64" fn(*mut c_void, *mut TextMetrics1) -> i32;
            assert_eq!(method::<GetM>(lay, 71)(lay, &mut m), S_OK);
            assert_eq!(m.base.line_count, 1);
            assert!(m.base.width > 0.0 && m.height_ws == m.base.height);
            let mut lines: [LineMetrics1; 2] = std::mem::zeroed();
            let mut n = 0u32;
            type GetL =
                unsafe extern "win64" fn(*mut c_void, *mut LineMetrics1, u32, *mut u32) -> i32;
            assert_eq!(
                method::<GetL>(lay, 83)(lay, lines.as_mut_ptr(), 2, &mut n),
                S_OK
            );
            assert_eq!(n, 1);
            assert_eq!(lines[0].base.length, txt.len() as u32);
            // Réglage hérité du format, relu par la mise en page.
            assert_eq!(method::<GetW>(lay, 73)(lay), 0);
            assert_eq!(method::<Unary>(lay, 85)(lay), 1);
            method::<Unary>(lay, 2)(lay);
            method::<Unary>(fmt, 2)(fmt);
        }
    }
}
