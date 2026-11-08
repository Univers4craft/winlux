//! IDWriteGlyphRunAnalysis écrit pour Winlux : couverture d'une suite de
//! glyphes en pixels, pour les programmes qui dessinent eux-mêmes leur
//! texte à partir des « textures alpha » de DirectWrite (le rendu logiciel
//! de WPF, #847).
//!
//! D'après la documentation publique de DirectWrite :
//! - `IDWriteFactory::CreateGlyphRunAnalysis(run, pixelsPerDip, transform,
//!   renderingMode, measuringMode, baselineOriginX, baselineOriginY, out)` ;
//! - le mode ALIASED ne donne que la texture `DWRITE_TEXTURE_ALIASED_1x1`
//!   (un octet par pixel), les autres modes ne donnent que
//!   `DWRITE_TEXTURE_CLEARTYPE_3x1` (trois octets par pixel), le mode
//!   OUTLINE est refusé ;
//! - les bornes d'un type de texture non fourni sont vides et sa texture
//!   est remplie de zéros.
//!
//! Les contours viennent des polices de Winlux (`dwrite::glyph_run_outline`)
//! et sont remplis par le rastériseur anticrénelé de Direct2D. La texture
//! ClearType reçoit la même couverture sur ses trois sous-pixels : un texte
//! en niveaux de gris, net et sans franges colorées.
#![allow(clippy::missing_safety_doc)]

use super::audio_com::{guid, put, Com, VTable, E_INVALIDARG, E_POINTER, S_OK};
use super::d2d1::{add_ref, method, qi_list, release};
use super::d2d_raster::{Canvas, FillRule, Matrix, Path};
use super::dwrite::GlyphRun;
use super::ole32::Guid;
use std::ffi::c_void;

const IID_GLYPH_RUN_ANALYSIS: Guid = guid(
    0x7d97_dbf7,
    0xe085,
    0x42d4,
    [0x81, 0xe3, 0x6a, 0x88, 0x3b, 0xde, 0xd1, 0x18],
);

pub const RENDERING_MODE_ALIASED: u32 = 1;
pub const RENDERING_MODE_OUTLINE: u32 = 6;
pub const TEXTURE_ALIASED_1X1: u32 = 0;
pub const TEXTURE_CLEARTYPE_3X1: u32 = 1;

/// DWRITE_MATRIX (même disposition que D2D1_MATRIX_3X2_F).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DwMatrix {
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub dx: f32,
    pub dy: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
    fn width(&self) -> usize {
        (self.right - self.left).max(0) as usize
    }
    fn height(&self) -> usize {
        (self.bottom - self.top).max(0) as usize
    }
}

/// Couverture calculée une fois : un octet (0..255) par pixel de `bounds`.
pub struct Analysis {
    /// Texture d'un octet par pixel : mode ALIASED, ou anticrénelage en
    /// niveaux de gris demandé à IDWriteFactory2/3.
    one_byte: bool,
    bounds: Rect,
    coverage: Vec<u8>,
}

impl Analysis {
    /// Rastérise un chemin déjà en pixels.
    #[cfg(test)]
    pub fn from_polys(polys: &[Vec<(f32, f32)>], aliased: bool) -> Analysis {
        Self::from_polys_gray(polys, aliased, false)
    }

    /// Comme `from_polys`, avec la couverture en niveaux de gris dans la
    /// texture DWRITE_TEXTURE_ALIASED_1x1 quand `gray` est vrai
    /// (DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE).
    pub fn from_polys_gray(polys: &[Vec<(f32, f32)>], aliased: bool, gray: bool) -> Analysis {
        let one_byte = aliased || gray;
        let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in polys.iter().flatten() {
            if p.0.is_finite() && p.1.is_finite() {
                l = l.min(p.0);
                t = t.min(p.1);
                r = r.max(p.0);
                b = b.max(p.1);
            }
        }
        // Garde-fou : une suite de glyphes de plus de 16 384 pixels de côté
        // est tronquée (taille maximale d'une texture Direct3D).
        const MAX: f32 = 16384.0;
        let empty = Analysis {
            one_byte,
            bounds: Rect::default(),
            coverage: Vec::new(),
        };
        if l >= r || t >= b {
            return empty;
        }
        let bounds = Rect {
            left: l.floor().max(-MAX) as i32,
            top: t.floor().max(-MAX) as i32,
            right: r.ceil().min(MAX) as i32,
            bottom: b.ceil().min(MAX) as i32,
        };
        if bounds.is_empty() || bounds.width() * bounds.height() > (1 << 26) {
            return empty;
        }
        let (w, h) = (bounds.width(), bounds.height());
        let mut px = vec![0u8; w * h * 4];
        let shifted: Vec<Vec<(f32, f32)>> = polys
            .iter()
            .map(|p| {
                p.iter()
                    .map(|q| (q.0 - bounds.left as f32, q.1 - bounds.top as f32))
                    .collect()
            })
            .collect();
        let mut canvas = Canvas {
            w: w as u32,
            h: h as u32,
            stride: w * 4,
            px: &mut px,
        };
        canvas.fill(&shifted, FillRule::NonZero, None, &|_, _| {
            [1.0, 1.0, 1.0, 1.0]
        });
        let coverage = px
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| {
                if aliased {
                    if c[3] >= 128 {
                        255
                    } else {
                        0
                    }
                } else {
                    c[3]
                }
            })
            .collect();
        Analysis {
            one_byte,
            bounds,
            coverage,
        }
    }

    pub fn bounds(&self, texture: u32) -> Rect {
        let wanted = if self.one_byte {
            TEXTURE_ALIASED_1X1
        } else {
            TEXTURE_CLEARTYPE_3X1
        };
        if texture == wanted {
            self.bounds
        } else {
            Rect::default()
        }
    }

    /// Copie la couverture de `area` dans `out` (1 ou 3 octets par pixel) ;
    /// ce qui sort des bornes, ou un type de texture non fourni, vaut 0.
    pub fn fill(&self, texture: u32, area: Rect, out: &mut [u8]) {
        out.iter_mut().for_each(|v| *v = 0);
        let bpp = if texture == TEXTURE_ALIASED_1X1 { 1 } else { 3 };
        if self.bounds(texture).is_empty() {
            return;
        }
        let w = area.width();
        for y in area.top..area.bottom {
            if y < self.bounds.top || y >= self.bounds.bottom {
                continue;
            }
            let src_row = (y - self.bounds.top) as usize * self.bounds.width();
            let dst_row = (y - area.top) as usize * w;
            for x in area.left.max(self.bounds.left)..area.right.min(self.bounds.right) {
                let v = self.coverage[src_row + (x - self.bounds.left) as usize];
                let d = (dst_row + (x - area.left) as usize) * bpp;
                out[d..d + bpp].iter_mut().for_each(|o| *o = v);
            }
        }
    }
}

type AnalysisObj = Com<Analysis>;

/// IDWriteFactory::CreateGlyphRunAnalysis.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn create_glyph_run_analysis(
    this: *mut c_void,
    run: *const GlyphRun,
    pixels_per_dip: f32,
    transform: *const DwMatrix,
    rendering_mode: u32,
    measuring_mode: u32,
    origin_x: f32,
    origin_y: f32,
    out: *mut *mut c_void,
) -> i32 {
    create_glyph_run_analysis_aa(
        this,
        run,
        pixels_per_dip,
        transform,
        rendering_mode,
        measuring_mode,
        origin_x,
        origin_y,
        false,
        out,
    )
}

/// Même chose, avec l'anticrénelage en niveaux de gris des versions 2 et 3
/// de la fabrique (`gray`).
#[allow(clippy::too_many_arguments)]
pub unsafe fn create_glyph_run_analysis_aa(
    _this: *mut c_void,
    run: *const GlyphRun,
    pixels_per_dip: f32,
    transform: *const DwMatrix,
    rendering_mode: u32,
    _measuring_mode: u32,
    origin_x: f32,
    origin_y: f32,
    gray: bool,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if run.is_null() || rendering_mode == RENDERING_MODE_OUTLINE || rendering_mode > 6 {
        return E_INVALIDARG;
    }
    let ppd = if pixels_per_dip > 0.0 && pixels_per_dip.is_finite() {
        pixels_per_dip
    } else {
        1.0
    };
    // Transformation en DIP, puis passage en pixels.
    let m = transform.as_ref().map_or(Matrix::IDENTITY, |t| Matrix {
        m11: t.m11,
        m12: t.m12,
        m21: t.m21,
        m22: t.m22,
        dx: t.dx,
        dy: t.dy,
    });
    let to_px = m.then(&Matrix {
        m11: ppd,
        m12: 0.0,
        m21: 0.0,
        m22: ppd,
        dx: 0.0,
        dy: 0.0,
    });
    let mut path = Path::default();
    let polys =
        if super::dwrite::glyph_run_outline(run, (origin_x, origin_y), &mut path, to_px.scale()) {
            path.fill_polys(&to_px)
        } else {
            Vec::new()
        };
    let a = Analysis::from_polys_gray(&polys, rendering_mode == RENDERING_MODE_ALIASED, gray);
    put(out, Com::create(ANALYSIS_VT.ptr(), a))
}

unsafe extern "win64" fn analysis_qi(
    this: *mut AnalysisObj,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    qi_list(this, riid, out, &[IID_GLYPH_RUN_ANALYSIS])
}

unsafe extern "win64" fn analysis_bounds(
    this: *mut AnalysisObj,
    texture: u32,
    out: *mut Rect,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if texture > TEXTURE_CLEARTYPE_3X1 {
        return E_INVALIDARG;
    }
    *out = Com::get(this).bounds(texture);
    S_OK
}

unsafe extern "win64" fn analysis_texture(
    this: *mut AnalysisObj,
    texture: u32,
    area: *const Rect,
    out: *mut u8,
    size: u32,
) -> i32 {
    if texture > TEXTURE_CLEARTYPE_3X1 || area.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let area = *area;
    let bpp = if texture == TEXTURE_ALIASED_1X1 { 1 } else { 3 };
    let need = area.width() * area.height() * bpp;
    if (size as usize) < need {
        return 0x8007_007Au32 as i32; // HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER)
    }
    let buf = std::slice::from_raw_parts_mut(out, need);
    Com::get(this).fill(texture, area, buf);
    S_OK
}

unsafe extern "win64" fn analysis_blend_params(
    _this: *mut AnalysisObj,
    params: *mut c_void,
    gamma: *mut f32,
    contrast: *mut f32,
    cleartype: *mut f32,
) -> i32 {
    if gamma.is_null() || contrast.is_null() || cleartype.is_null() {
        return E_INVALIDARG;
    }
    // Gamma et contraste des paramètres de rendu fournis (méthodes 3 et 4
    // de IDWriteRenderingParams), sinon ceux de Windows par défaut.
    if params.is_null() {
        *gamma = 1.8;
        *contrast = 0.5;
    } else {
        *gamma = method::<unsafe extern "win64" fn(*mut c_void) -> f32>(params, 3)(params);
        *contrast = method::<unsafe extern "win64" fn(*mut c_void) -> f32>(params, 4)(params);
    }
    // Couverture identique sur les trois sous-pixels : pas de ClearType.
    *cleartype = 0.0;
    S_OK
}

static ANALYSIS_VT: VTable<6> = VTable([
    analysis_qi as *const (),
    add_ref::<Analysis> as *const (),
    release::<Analysis> as *const (),
    analysis_bounds as *const (),
    analysis_texture as *const (),
    analysis_blend_params as *const (),
]);

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Vec<(f32, f32)>> {
        vec![vec![(2.0, 3.0), (6.0, 3.0), (6.0, 5.5), (2.0, 5.5)]]
    }

    #[test]
    fn bornes_et_couverture_cleartype() {
        let a = Analysis::from_polys(&square(), false);
        let b = a.bounds(TEXTURE_CLEARTYPE_3X1);
        assert_eq!(
            b,
            Rect {
                left: 2,
                top: 3,
                right: 6,
                bottom: 6
            }
        );
        // Le type aliasé n'est pas fourni hors du mode ALIASED.
        assert!(a.bounds(TEXTURE_ALIASED_1X1).is_empty());
        let mut out = vec![0u8; b.width() * b.height() * 3];
        a.fill(TEXTURE_CLEARTYPE_3X1, b, &mut out);
        // Ligne pleine puis demi-ligne (y de 5 à 5,5).
        assert_eq!(&out[0..3], &[255, 255, 255]);
        let last = (2 * b.width()) * 3;
        assert!((120..=136).contains(&out[last]), "{}", out[last]);
        assert_eq!(out[last], out[last + 1]);
    }

    #[test]
    fn mode_aliase_seuil_et_texture_vide_sinon() {
        let a = Analysis::from_polys(&square(), true);
        let b = a.bounds(TEXTURE_ALIASED_1X1);
        assert!(!b.is_empty());
        assert!(a.bounds(TEXTURE_CLEARTYPE_3X1).is_empty());
        let mut out = vec![7u8; b.width() * b.height()];
        a.fill(TEXTURE_ALIASED_1X1, b, &mut out);
        assert!(out.iter().all(|&v| v == 0 || v == 255));
        let mut ct = vec![7u8; 12];
        a.fill(TEXTURE_CLEARTYPE_3X1, b, &mut ct);
        assert!(ct.iter().all(|&v| v == 0));
    }

    #[test]
    fn zone_demandee_plus_grande_que_les_bornes() {
        let a = Analysis::from_polys(&square(), false);
        let area = Rect {
            left: 0,
            top: 0,
            right: 8,
            bottom: 8,
        };
        let mut out = vec![9u8; 8 * 8 * 3];
        a.fill(TEXTURE_CLEARTYPE_3X1, area, &mut out);
        assert_eq!(out[0], 0);
        assert_eq!(out[(3 * 8 + 2) * 3], 255);
    }

    #[test]
    fn analyse_d_une_police_truetype() {
        unsafe {
            let run = crate::dlls::dwrite::tests::sample_run();
            let mut obj: *mut c_void = std::ptr::null_mut();
            let hr = create_glyph_run_analysis(
                std::ptr::null_mut(),
                &run.run,
                1.0,
                std::ptr::null(),
                5,
                0,
                10.0,
                20.0,
                &mut obj,
            );
            assert_eq!(hr, S_OK);
            let mut r = Rect::default();
            assert_eq!(
                analysis_bounds(obj.cast(), TEXTURE_CLEARTYPE_3X1, &mut r),
                S_OK
            );
            // Carré (1..6, 0..6 au-dessus de la ligne de base) puis losange
            // décalé de l'avance 7, posés en (10, 20).
            assert_eq!(
                r,
                Rect {
                    left: 11,
                    top: 14,
                    right: 23,
                    bottom: 20
                }
            );
            let mut buf = vec![0u8; r.width() * r.height() * 3];
            assert_eq!(
                analysis_texture(
                    obj.cast(),
                    TEXTURE_CLEARTYPE_3X1,
                    &r,
                    buf.as_mut_ptr(),
                    buf.len() as u32
                ),
                S_OK
            );
            assert!(buf.iter().any(|&v| v == 255));
            // Mode OUTLINE refusé, comme sous Windows.
            let mut o2: *mut c_void = std::ptr::null_mut();
            let hr = create_glyph_run_analysis(
                std::ptr::null_mut(),
                &run.run,
                1.0,
                std::ptr::null(),
                RENDERING_MODE_OUTLINE,
                0,
                0.0,
                0.0,
                &mut o2,
            );
            assert_eq!(hr, E_INVALIDARG);
            release::<Analysis>(obj.cast());
        }
    }
}
