//! Tests des décodeurs sur de petits fichiers de référence
//! (`runtime/tests/fixtures/imaging`, références RGBA décodées par une
//! bibliothèque tierce uniquement pour la génération des fixtures).

use super::*;
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/imaging")
}

fn reference(name: &str) -> (u32, u32, Vec<u8>) {
    let d = std::fs::read(dir().join(name)).unwrap();
    (le32(&d, 0), le32(&d, 4), d[8..].to_vec())
}

fn decode(name: &str) -> Image {
    let p = dir().join(name);
    decode_file(&p).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn raster(img: &Image, page: usize) -> &Raster {
    match &img.pages[page].content {
        Content::Raster(r) => r,
        _ => panic!("pas une image matricielle"),
    }
}

/// (écart max, écart moyen) en ignorant la couleur des pixels transparents.
fn diff(r: &Raster, w: u32, h: u32, px: &[u8]) -> (u32, f64) {
    assert_eq!((r.w, r.h), (w, h), "dimensions");
    let mut max = 0u32;
    let mut sum = 0u64;
    let mut n = 0u64;
    for i in 0..(w * h) as usize {
        let a = &r.px[i * 4..i * 4 + 4];
        let b = &px[i * 4..i * 4 + 4];
        let ch = if a[3] == 0 && b[3] == 0 { 3..4 } else { 0..4 };
        for k in ch {
            let d = (a[k] as i32 - b[k] as i32).unsigned_abs();
            max = max.max(d);
            sum += d as u64;
            n += 1;
        }
    }
    (max, sum as f64 / n.max(1) as f64)
}

fn check(name: &str, refname: &str, max_tol: u32, mean_tol: f64) {
    let img = decode(name);
    let (w, h, px) = reference(refname);
    let (max, mean) = diff(raster(&img, 0), w, h, &px);
    assert!(max <= max_tol && mean <= mean_tol, "{name}: écart max {max}, moyen {mean:.3}");
}

#[test]
fn formats_sans_perte_identiques_a_la_reference() {
    for f in [
        "t_rgba.png",
        "t_rgb_interlace.png",
        "t_pal.png",
        "t.bmp",
        "t32.bmp",
        "t.tga",
        "t_rle.tga",
        "t.pcx",
        "t.ppm",
        "t.pgm",
        "t_lzw.tif",
        "t_deflate.tif",
        "t_packbits.tif",
        "t_lossless.webp",
        "t.gif",
        "t.psd",
        "t.sgi",
        "t.xbm",
        "t.qoi",
        "t_raw.dds",
        "t.xpm",
        "t.wbmp",
        "t.pam",
    ] {
        check(f, &format!("{f}.rgba"), 1, 0.01);
    }
    for f in ["t.qoi", "t.ff", "t.xcf", "t.sun"] {
        check(f, "ref.rgba", 0, 0.0);
    }
}

#[test]
fn formats_avec_perte_proches_de_la_reference() {
    for f in ["t.jpg", "t_prog.jpg", "t_444.jpg", "t_gray.jpg", "t_jpeg.tif"] {
        check(f, &format!("{f}.rgba"), 40, 1.5);
    }
    for f in ["t_lossy.webp", "t_lossy_alpha.webp"] {
        check(f, &format!("{f}.rgba"), 6, 0.2);
    }
    for f in ["t_dxt1.dds", "t_dxt5.dds"] {
        check(f, &format!("{f}.rgba"), 1, 0.3);
    }
}

#[test]
fn animations_gif_et_webp() {
    for f in ["t_anim.gif", "t_anim.webp"] {
        let img = decode(f);
        assert!(img.animated, "{f}");
        assert_eq!(img.pages.len(), 3, "{f}");
        assert!(img.pages.iter().all(|p| p.delay_ms == 100), "{f}");
        let (w, h, px) = reference(&format!("{f}.rgba"));
        let (max, _) = diff(raster(&img, 0), w, h, &px);
        assert!(max <= 1, "{f}: {max}");
    }
}

#[test]
fn icones_et_hdr() {
    let ico = decode("t.ico");
    assert!(!ico.pages.is_empty());
    for f in ["t.hdr", "t_zip.exr", "t_none.exr", "t.pfm"] {
        let img = decode(f);
        let r = raster(&img, 0);
        assert_eq!((r.w, r.h), (40, 30), "{f}");
        // le cercle rouge reste rouge après tone mapping
        let c = r.get(12, 12);
        assert!(c[0] > 150 && c[1] < 60 && c[2] < 60, "{f}: {c:?}");
    }
    let zip = decode("t_zip.exr");
    let none = decode("t_none.exr");
    assert_eq!(raster(&zip, 0).px, raster(&none, 0).px);
}

#[test]
fn raw_affiche_l_apercu_jpeg_et_les_exif() {
    let img = decode("t.nef");
    assert!(img.raw && img.embedded_preview);
    let (w, h, px) = reference("t.jpg.rgba");
    let (max, mean) = diff(raster(&img, 0), w, h, &px);
    assert!(max <= 40 && mean <= 1.5, "{max} {mean}");
    let get = |k: &str| img.meta.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(get("camera.make").as_deref(), Some("WinluxCam"));
    assert_eq!(get("camera.model").as_deref(), Some("Testeur 3000"));
    assert_eq!(get("exposure").as_deref(), Some("1/250 s"));
    assert_eq!(get("iso").as_deref(), Some("ISO 200"));
}

#[test]
fn svg_vectoriel_et_zoom_sans_perte() {
    let img = decode("t.svg");
    assert_eq!(img.pages[0].size(), (40, 30));
    let mut l = Loaded::new(img);
    let mut out = vec![0u8; 40 * 30 * 4];
    let v = View { zoom: 1.0, cx: 20.0, cy: 15.0, ..Default::default() };
    l.render(0, &v, 40, 30, 160, &mut out, [128, 128, 128], false);
    let px = |x: usize, y: usize| {
        let o = (y * 40 + x) * 4;
        [out[o + 2], out[o + 1], out[o]]
    };
    assert!(px(1, 7)[0] > 200 && px(1, 7)[2] < 60, "début du dégradé rouge {:?}", px(1, 7));
    assert!(px(18, 7)[2] > 200 && px(18, 7)[0] < 60, "fin du dégradé bleue {:?}", px(18, 7));
    assert_eq!(px(30, 22), [0, 255, 0], "cercle vert (classe CSS)");
    assert_eq!(px(30, 5), [255, 255, 255], "fond blanc");
    assert_eq!(px(10, 28), [0, 0, 0], "triangle noir");
    // zoom x8 : le bord du cercle reste net (re-rastérisation)
    let mut big = vec![0u8; 64 * 64 * 4];
    let v = View { zoom: 8.0, cx: 30.0, cy: 22.5, ..Default::default() };
    l.render(0, &v, 64, 64, 256, &mut big, [0, 0, 0], false);
    let intermediate = big.chunks(4).filter(|p| p[1] > 20 && p[1] < 235 && p[0] < 20).count();
    assert!(intermediate < 64 * 3, "bord flou : {intermediate} pixels intermédiaires");
}

#[test]
fn modeles_3d_aperçu() {
    for f in ["t.obj", "t.stl", "t.ply", "t.glb"] {
        let img = decode(f);
        let Content::Model(m) = &img.pages[0].content else {
            panic!("{f}");
        };
        assert_eq!(m.tris.len(), 4, "{f}");
        let mut l = Loaded::new(img);
        let t = thumbnail(&mut l, 48);
        let opaque = t.chunks(4).filter(|p| p[3] == 255).count();
        assert!(opaque > 200, "{f}: {opaque}");
    }
}

#[test]
fn formats_sans_decodeur_jamais_annonces() {
    assert_eq!(decode_file(&dir().join("t.heic")).unwrap_err(), Error::Unsupported("HEIC"));
    for e in ["heic", "avif", "jxl", "jp2", "pdf"] {
        assert!(!supported_ext(e), "{e}");
    }
    for e in ["png", "jpg", "webp", "cr2", "nef", "psd", "svg", "exr", "stl", "glb"] {
        assert!(supported_ext(e), "{e}");
    }
}

#[test]
fn rendu_rotation_miroir_et_vignette() {
    let img = decode("ref.png");
    let mut l = Loaded::new(img);
    let (w, h) = (40usize, 30usize);
    // rotation d'un quart de tour : la sortie 30x40 contient l'image tournée
    let v = View { zoom: 1.0, cx: 20.0, cy: 15.0, rot: 1, ..Default::default() };
    let mut out = vec![0u8; h * w * 4];
    l.render(0, &v, h, w, h * 4, &mut out, [0, 0, 0], false);
    let src = |x: u32, y: u32| raster(&l.img, 0).get(x, y);
    // pixel source (5,5) -> écran (h-1-5, 5)
    let o = (5 * h + (h - 1 - 5)) * 4;
    let s = src(5, 5);
    assert!((out[o + 2] as i32 - s[0] as i32).abs() <= 2);
    let t = thumbnail(&mut l, 64);
    assert_eq!(t.len(), 64 * 64 * 4);
    // l'image (40x30) est centrée : les bandes haut/bas sont transparentes
    assert_eq!(t[3], 0);
    assert!(t[(32 * 64 + 20) * 4 + 3] > 0);
}

#[test]
fn svgz_compresse_par_gzip() {
    let img = decode("t.svgz");
    assert!(matches!(img.pages[0].content, Content::Vector(_)));
}
