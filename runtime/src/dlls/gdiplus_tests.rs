use super::text::{layout, range_box};
use super::*;

const W: i32 = 16;

unsafe fn new_bitmap(w: i32, h: i32) -> usize {
    let mut b = 0usize;
    assert_eq!(create_bitmap_from_scan0(w, h, 0, PF_ARGB32, 0, &mut b), OK);
    b
}

unsafe fn pixel(b: usize, x: i32, y: i32) -> u32 {
    let mut c = 0u32;
    assert_eq!(get_pixel(b, x, y, &mut c), OK);
    c
}

unsafe fn graphics_of(b: usize) -> usize {
    let mut g = 0usize;
    assert_eq!(image_graphics(b, &mut g), OK);
    g
}

unsafe fn solid(c: u32) -> usize {
    let mut br = 0usize;
    assert_eq!(create_solid_fill(c, &mut br), OK);
    br
}

#[test]
fn demarrage_et_bitmap_transparente() {
    unsafe {
        let input = [1u32, 0, 0, 0, 0, 0];
        let mut token = 0usize;
        assert_eq!(
            startup(
                &mut token,
                input.as_ptr() as *const u8,
                std::ptr::null_mut()
            ),
            OK
        );
        assert_ne!(token, 0);
        let b = new_bitmap(W, W);
        let mut w = 0u32;
        assert_eq!(get_image_width(b, &mut w), OK);
        assert_eq!(w, W as u32);
        assert_eq!(pixel(b, 3, 3), 0);
        assert_eq!(set_pixel(b, 3, 3, 0xFF12_3456), OK);
        assert_eq!(pixel(b, 3, 3), 0xFF12_3456);
        assert_eq!(dispose_image(b), OK);
        assert_eq!(get_image_width(b, &mut w), INVALID_PARAMETER);
    }
}

#[test]
fn rectangle_plein_sans_anticrenelage() {
    unsafe {
        let b = new_bitmap(W, W);
        let g = graphics_of(b);
        assert_eq!(graphics_clear(g, 0xFFFF_FFFF), OK);
        let br = solid(0xFFFF_0000);
        assert_eq!(fill_rectangle(g, br, 2.0, 2.0, 4.0, 4.0), OK);
        assert_eq!(pixel(b, 2, 2), 0xFFFF_0000);
        assert_eq!(pixel(b, 5, 5), 0xFFFF_0000);
        assert_eq!(pixel(b, 6, 6), 0xFFFF_FFFF);
        assert_eq!(pixel(b, 1, 2), 0xFFFF_FFFF);
        // Translation du monde.
        assert_eq!(translate_world(g, 8.0, 0.0, 0), OK);
        assert_eq!(fill_rectangle(g, br, 2.0, 8.0, 2.0, 2.0), OK);
        assert_eq!(pixel(b, 10, 8), 0xFFFF_0000);
        assert_eq!(pixel(b, 2, 8), 0xFFFF_FFFF);
        delete_brush(br);
        delete_graphics(g);
        dispose_image(b);
    }
}

#[test]
fn ellipse_anticrenelee_a_des_bords_partiels() {
    unsafe {
        let b = new_bitmap(W, W);
        let g = graphics_of(b);
        assert_eq!(set_smoothing(g, 4), OK);
        let br = solid(0xFF00_00FF);
        assert_eq!(fill_ellipse(g, br, 0.0, 0.0, 16.0, 16.0), OK);
        assert_eq!(pixel(b, 8, 8), 0xFF00_00FF);
        assert_eq!(pixel(b, 0, 0) >> 24, 0);
        let edge = pixel(b, 2, 3) >> 24;
        assert!(edge > 0 && edge < 255, "bord : alpha {edge}");
        delete_brush(br);
        delete_graphics(g);
        dispose_image(b);
    }
}

#[test]
fn ligne_d_un_pixel() {
    unsafe {
        let b = new_bitmap(W, W);
        let g = graphics_of(b);
        let mut p = 0usize;
        assert_eq!(create_pen1(0xFF00_FF00, 1.0, 0, &mut p), OK);
        assert_eq!(draw_line(g, p, 1.0, 4.0, 12.0, 4.0), OK);
        assert_eq!(pixel(b, 6, 4) & 0x00FF_FFFF, 0x0000_FF00);
        assert_eq!(pixel(b, 6, 7), 0);
        delete_graphics(g);
        dispose_image(b);
    }
}

#[test]
fn verrouillage_des_pixels() {
    unsafe {
        let b = new_bitmap(4, 2);
        set_pixel(b, 1, 1, 0xFF01_0203);
        let mut data = [0u8; 32];
        let rect = [0i32, 0, 4, 2];
        assert_eq!(
            lock_bits(b, rect.as_ptr(), 3, PF_ARGB32, data.as_mut_ptr()),
            OK
        );
        let stride = (data.as_ptr().add(8) as *const i32).read_unaligned();
        let scan0 = (data.as_ptr().add(16) as *const usize).read_unaligned() as *mut u32;
        assert_eq!(stride, 16);
        assert_eq!(*scan0.add(5), 0xFF01_0203);
        *scan0.add(2) = 0xFF0A_0B0C;
        assert_eq!(unlock_bits(b, data.as_mut_ptr()), OK);
        assert_eq!(pixel(b, 2, 0), 0xFF0A_0B0C);
        dispose_image(b);
    }
}

#[test]
fn encodeurs_png_et_bmp() {
    unsafe {
        let (mut n, mut size) = (0u32, 0u32);
        assert_eq!(encoders_size(&mut n, &mut size), OK);
        assert!(n >= 2);
        let mut buf = vec![0u8; size as usize];
        assert_eq!(get_encoders(n, size, buf.as_mut_ptr()), OK);
        // MimeType (décalage 64) du premier codec.
        let mime = (buf.as_ptr().add(64) as *const usize).read_unaligned() as *const u16;
        let m = wstr(mime);
        assert!(m.starts_with("image/"), "{m}");
    }
}

#[test]
fn enregistrement_et_relecture_png() {
    unsafe {
        let b = new_bitmap(3, 3);
        set_pixel(b, 1, 1, 0xFF80_4020);
        let dir = std::env::temp_dir().join(format!("winlux-gdiplus-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.png");
        let name: Vec<u16> = file.to_string_lossy().encode_utf16().chain([0]).collect();
        let png = codec_clsid(6);
        assert_eq!(save_image_to_file(b, name.as_ptr(), &png, 0), OK);
        let mut back = 0usize;
        assert_eq!(load_image_from_file(name.as_ptr(), &mut back), OK);
        assert_eq!(pixel(back, 1, 1), 0xFF80_4020);
        assert_eq!(pixel(back, 0, 0) >> 24, 0);
        dispose_image(b);
        dispose_image(back);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn matrice_inversee() {
    unsafe {
        let mut m = 0usize;
        assert_eq!(create_matrix2(2.0, 0.0, 0.0, 4.0, 10.0, 20.0, &mut m), OK);
        let mut pts = [1.0f32, 1.0];
        assert_eq!(transform_matrix_points(m, pts.as_mut_ptr(), 1), OK);
        assert_eq!(pts, [12.0, 24.0]);
        assert_eq!(invert_matrix(m), OK);
        assert_eq!(transform_matrix_points(m, pts.as_mut_ptr(), 1), OK);
        assert!((pts[0] - 1.0).abs() < 1e-5 && (pts[1] - 1.0).abs() < 1e-5);
    }
}

#[test]
fn hachures() {
    assert!(hatch_on(0, 3, 0) && !hatch_on(0, 3, 1));
    assert!(hatch_on(1, 0, 5) && !hatch_on(1, 1, 5));
    assert!(hatch_on(4, 8, 3) && hatch_on(4, 3, 16));
    let dense = (0..64).filter(|i| hatch_on(17, i % 8, i / 8)).count();
    let thin = (0..64).filter(|i| hatch_on(6, i % 8, i / 8)).count();
    assert!(thin < 8 && dense > 50, "{thin} {dense}");
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn fmt() -> StringFormat {
    StringFormat {
        trimming: 1,
        ..Default::default()
    }
}

/// Avances fixes : 10 px par caractère.
fn ten(_: char) -> f32 {
    10.0
}

#[test]
fn texte_coupe_aux_mots() {
    // em 12 : marge 2 px de chaque côté ; place pour 10 caractères.
    let l = layout(&w("abc def ghij"), &fmt(), 12.0, 14.0, 104.0, 0.0, &ten);
    let lines: Vec<String> = l
        .lines
        .iter()
        .map(|ln| ln.chars.iter().map(|c| c.ch).collect())
        .collect();
    assert_eq!(lines, ["abc def ", "ghij"]);
    assert_eq!(l.lines[0].width, 70.0);
    assert_eq!(l.lines[0].x, 2.0);
    assert_eq!(l.fitted, 12);
    let (x, y, bw, bh) = l.bounds();
    assert_eq!((x, y, bw, bh), (0.0, 0.0, 74.0, 28.0));
}

#[test]
fn texte_sans_coupure_et_retours() {
    let mut f = fmt();
    f.flags = 0x1000;
    let l = layout(&w("ab cd\r\nef"), &f, 12.0, 14.0, 30.0, 0.0, &ten);
    assert_eq!(l.lines.len(), 2);
    assert_eq!(l.lines[0].width, 50.0);
    assert_eq!(l.lines[1].chars[0].idx, 7);
}

#[test]
fn texte_centre_et_aligne_a_droite() {
    let mut f = fmt();
    f.align = 1;
    f.line_align = 2;
    let l = layout(&w("ab"), &f, 12.0, 14.0, 104.0, 50.0, &ten);
    assert_eq!(l.lines[0].x, 2.0 + (100.0 - 20.0) / 2.0);
    assert_eq!(l.y, 36.0);
    f.align = 2;
    let l = layout(&w("ab"), &f, 12.0, 14.0, 0.0, 0.0, &ten);
    assert_eq!(l.lines[0].x, -22.0);
}

#[test]
fn texte_limite_en_hauteur_avec_points_de_suspension() {
    let mut f = fmt();
    f.trimming = 3;
    f.flags = 0x2000;
    let l = layout(&w("aaaa bbbb cccc"), &f, 12.0, 14.0, 64.0, 20.0, &ten);
    assert_eq!(l.lines.len(), 1);
    let s: String = l.lines[0].chars.iter().map(|c| c.ch).collect();
    assert_eq!(s, "aaaa\u{2026}");
    assert_eq!(l.fitted, 5);
}

#[test]
fn prefixe_et_commercial() {
    let mut f = fmt();
    f.hotkey = 1;
    let l = layout(&w("&Fichier && co"), &f, 12.0, 14.0, 0.0, 0.0, &ten);
    let s: String = l.lines[0].chars.iter().map(|c| c.ch).collect();
    assert_eq!(s, "Fichier & co");
    assert_eq!(l.hotkey, Some(1));
}

#[test]
fn plages_de_caracteres() {
    let l = layout(&w("abc def ghij"), &fmt(), 12.0, 14.0, 104.0, 0.0, &ten);
    let r = range_box(&l, 8, 2).unwrap();
    assert_eq!((r.l, r.t, r.r, r.b), (2.0, 14.0, 22.0, 28.0));
    let r = range_box(&l, 1, 1).unwrap();
    assert_eq!((r.l, r.r), (12.0, 22.0));
    assert!(range_box(&l, 40, 2).is_none());
}

#[test]
fn texte_vide() {
    let l = layout(&[], &fmt(), 12.0, 14.0, 100.0, 0.0, &ten);
    assert!(l.lines.is_empty());
    assert_eq!(l.bounds().2, 0.0);
}

#[test]
fn dessin_et_mesure_d_une_chaine() {
    unsafe {
        let probe = Font {
            family: "Microsoft Sans Serif".into(),
            size: 12.0,
            style: 0,
            unit: 2,
        };
        if crate::dlls::usp10::spec_font(&font_spec(&probe, 16.0)).is_none() {
            eprintln!("aucune police TrueType : test sauté");
            return;
        }
        let ex = exports();
        type Fam = unsafe extern "win64" fn(*const u16, usize, *mut usize) -> i32;
        type Mk = unsafe extern "win64" fn(usize, f32, i32, i32, *mut usize) -> i32;
        type Draw = unsafe extern "win64" fn(
            usize,
            *const u16,
            i32,
            usize,
            *const f32,
            usize,
            usize,
        ) -> i32;
        type Meas = unsafe extern "win64" fn(
            usize,
            *const u16,
            i32,
            usize,
            *const f32,
            usize,
            *mut f32,
            *mut i32,
            *mut i32,
        ) -> i32;
        let fam_fn: Fam = std::mem::transmute(ex["GdipCreateFontFamilyFromName"]);
        let mk: Mk = std::mem::transmute(ex["GdipCreateFont"]);
        let draw: Draw = std::mem::transmute(ex["GdipDrawString"]);
        let meas: Meas = std::mem::transmute(ex["GdipMeasureString"]);
        let name = w("Microsoft Sans Serif\0");
        let (mut fam, mut f) = (0usize, 0usize);
        assert_eq!(fam_fn(name.as_ptr(), 0, &mut fam), OK);
        assert_eq!(mk(fam, 16.0, 0, 2, &mut f), OK);
        let b = new_bitmap(120, 40);
        let g = graphics_of(b);
        let br = solid(0xFF00_0000);
        let text = w("Winlux");
        let rect = [0.0f32, 0.0, 120.0, 40.0];
        assert_eq!(
            draw(g, text.as_ptr(), text.len() as i32, f, rect.as_ptr(), 0, br),
            OK
        );
        let mut inked = 0;
        for y in 0..40 {
            for x in 0..120 {
                if pixel(b, x, y) >> 24 > 128 {
                    inked += 1;
                }
            }
        }
        assert!(inked > 30, "{inked} pixels encrés");
        let mut bbox = [0f32; 4];
        let (mut fitted, mut lines) = (0, 0);
        assert_eq!(
            meas(
                g,
                text.as_ptr(),
                text.len() as i32,
                f,
                rect.as_ptr(),
                0,
                bbox.as_mut_ptr(),
                &mut fitted,
                &mut lines
            ),
            OK
        );
        assert_eq!((fitted, lines), (6, 1));
        assert!(
            bbox[2] > 30.0 && bbox[2] < 120.0 && bbox[3] > 14.0 && bbox[3] < 30.0,
            "{bbox:?}"
        );
        delete_graphics(g);
        dispose_image(b);
    }
}

/// wxWidgets lit le point courant d'un chemin par GdipGetPathLastPoint
/// (rectangles arrondis des boutons d'Audacity, #1054).
#[test]
fn dernier_point_et_points_entiers_du_chemin() {
    unsafe {
        let mut p = 0usize;
        assert_eq!(create_path(0, &mut p), OK);
        let mut last = [0f32; 2];
        assert_eq!(get_path_last_point(p, last.as_mut_ptr()), INVALID_PARAMETER);
        assert_eq!(add_path_line(p, 1.4, 2.6, 10.0, 2.0), OK);
        assert_eq!(get_path_last_point(p, last.as_mut_ptr()), OK);
        assert_eq!(last, [10.0, 2.0]);
        let mut pts = [0i32; 4];
        assert_eq!(get_path_points_i(p, pts.as_mut_ptr(), 4), OK);
        assert_eq!(pts, [1, 3, 10, 2]);
        assert_eq!(
            get_path_points_i(p, pts.as_mut_ptr(), 1),
            INSUFFICIENT_BUFFER
        );
        delete_path(p);
    }
}

/// #1486 : GdipGetNearestColor rend la couleur telle quelle en 32 bits.
#[test]
fn couleur_la_plus_proche() {
    unsafe {
        let b = new_bitmap(4, 4);
        let g = graphics_of(b);
        let mut c = 0x80C0_A060u32;
        assert_eq!(get_nearest_color(g, &mut c), OK);
        assert_eq!(c, 0x80C0_A060);
        assert_eq!(
            get_nearest_color(g, std::ptr::null_mut()),
            INVALID_PARAMETER
        );
        assert_eq!(get_nearest_color(0, &mut c), INVALID_PARAMETER);
        delete_graphics(g);
        dispose_image(b);
    }
}
