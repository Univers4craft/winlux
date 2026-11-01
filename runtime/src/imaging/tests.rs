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
    assert!(
        max <= max_tol && mean <= mean_tol,
        "{name}: écart max {max}, moyen {mean:.3}"
    );
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
    for f in [
        "t.jpg",
        "t_prog.jpg",
        "t_444.jpg",
        "t_gray.jpg",
        "t_jpeg.tif",
    ] {
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
    let get = |k: &str| {
        img.meta
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
    };
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
    let v = View {
        zoom: 1.0,
        cx: 20.0,
        cy: 15.0,
        ..Default::default()
    };
    l.render(0, &v, 40, 30, 160, &mut out, [128, 128, 128], false);
    let px = |x: usize, y: usize| {
        let o = (y * 40 + x) * 4;
        [out[o + 2], out[o + 1], out[o]]
    };
    assert!(
        px(1, 7)[0] > 200 && px(1, 7)[2] < 60,
        "début du dégradé rouge {:?}",
        px(1, 7)
    );
    assert!(
        px(18, 7)[2] > 200 && px(18, 7)[0] < 60,
        "fin du dégradé bleue {:?}",
        px(18, 7)
    );
    assert_eq!(px(30, 22), [0, 255, 0], "cercle vert (classe CSS)");
    assert_eq!(px(30, 5), [255, 255, 255], "fond blanc");
    assert_eq!(px(10, 28), [0, 0, 0], "triangle noir");
    // zoom x8 : le bord du cercle reste net (re-rastérisation)
    let mut big = vec![0u8; 64 * 64 * 4];
    let v = View {
        zoom: 8.0,
        cx: 30.0,
        cy: 22.5,
        ..Default::default()
    };
    l.render(0, &v, 64, 64, 256, &mut big, [0, 0, 0], false);
    let intermediate = big
        .chunks(4)
        .filter(|p| p[1] > 20 && p[1] < 235 && p[0] < 20)
        .count();
    assert!(
        intermediate < 64 * 3,
        "bord flou : {intermediate} pixels intermédiaires"
    );
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
    assert_eq!(
        decode_file(&dir().join("t.heic")).unwrap_err(),
        Error::Unsupported("HEIC")
    );
    for e in ["heic", "avif", "jxl", "jp2", "pdf"] {
        assert!(!supported_ext(e), "{e}");
    }
    for e in [
        "png", "jpg", "webp", "cr2", "nef", "psd", "svg", "exr", "stl", "glb",
    ] {
        assert!(supported_ext(e), "{e}");
    }
}

#[test]
fn rendu_rotation_miroir_et_vignette() {
    let img = decode("ref.png");
    let mut l = Loaded::new(img);
    let (w, h) = (40usize, 30usize);
    // rotation d'un quart de tour : la sortie 30x40 contient l'image tournée
    let v = View {
        zoom: 1.0,
        cx: 20.0,
        cy: 15.0,
        rot: 1,
        ..Default::default()
    };
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

// --- Robustesse sur en-têtes malformés (#213) ------------------------------
//
// Le profil release compile avec `panic = "abort"` : chaque décodeur doit
// rendre une erreur propre, jamais paniquer ni allouer sur foi du fichier.

fn malformed(name: &str, patch: impl FnOnce(&mut Vec<u8>)) -> Result<Image, Error> {
    let mut d = std::fs::read(dir().join(name)).unwrap();
    patch(&mut d);
    decode_bytes(&d, &ext_of(name))
}

#[test]
fn png_profondeur_invalide() {
    // IHDR : profondeur à l'octet 24 (0 = division par zéro, 7 = décalage
    // hors octet, 3 = non prévue par le format).
    for depth in [0u8, 3, 7, 32, 255] {
        let r = malformed("ref.png", |d| d[24] = depth);
        assert!(
            matches!(r, Err(Error::Corrupt)),
            "profondeur {depth}: {r:?}"
        );
    }
    // Entrelacement inconnu.
    assert!(malformed("ref.png", |d| d[28] = 2).is_err());
}

#[test]
fn apng_trame_hors_canevas() {
    // APNG synthétique 2×2 : fcTL annonçant une trame 60000×60000.
    fn chunk(kind: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = (body.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v.extend_from_slice(&[0, 0, 0, 0]);
        v
    }
    let mut d = b"\x89PNG\r\n\x1a\n".to_vec();
    d.extend(chunk(b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0, 0, 0]));
    d.extend(chunk(b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]));
    let mut fctl = vec![0, 0, 0, 0];
    fctl.extend_from_slice(&60000u32.to_be_bytes());
    fctl.extend_from_slice(&60000u32.to_be_bytes());
    fctl.extend_from_slice(&[0; 8]);
    fctl.extend_from_slice(&[0, 1, 0, 10, 0, 0]);
    d.extend(chunk(b"fcTL", &fctl));
    // IDAT : zlib « stored » de 2 lignes (1 filtre + 8 octets).
    let raw = [0u8; 18];
    let mut z = vec![0x78, 0x01, 0x01, 18, 0, 237, 255];
    z.extend_from_slice(&raw);
    z.extend_from_slice(&[0, 0, 0, 1]);
    d.extend(chunk(b"IDAT", &z));
    d.extend(chunk(b"fcTL", &fctl));
    d.extend(chunk(b"fdAT", &[0, 0, 0, 1, 1, 2, 3]));
    d.extend(chunk(b"IEND", &[]));
    let img = decode_bytes(&d, "png").expect("l'image de base reste lisible");
    assert_eq!((raster(&img, 0).w, raster(&img, 0).h), (2, 2));
}

#[test]
fn gif_trame_demesuree() {
    // Descripteur d'image 65535×65535 sur un canevas 2×2 : trame ignorée.
    let mut d = b"GIF89a\x02\x00\x02\x00\x00\x00\x00".to_vec();
    d.extend_from_slice(&[0x2C, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0, 2, 0, 0x3B]);
    assert!(matches!(decode_bytes(&d, "gif"), Err(Error::Corrupt)));
    // Trame vide (0×0).
    let mut d = b"GIF89a\x02\x00\x02\x00\x00\x00\x00".to_vec();
    d.extend_from_slice(&[0x2C, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0x3B]);
    assert!(matches!(decode_bytes(&d, "gif"), Err(Error::Corrupt)));
}

#[test]
fn jpeg_composantes_et_sos_tronque() {
    // SOF0 avec 5 composantes.
    let mut d = vec![0xFF, 0xD8];
    d.extend_from_slice(&[0xFF, 0xC0, 0, 23, 8, 0, 1, 0, 1, 5]);
    for i in 0..5u8 {
        d.extend_from_slice(&[i + 1, 0x11, 0]);
    }
    d.extend_from_slice(&[0xFF, 0xD9]);
    assert!(matches!(decode_bytes(&d, "jpg"), Err(Error::Corrupt)));
    // SOF0 correct puis SOS vide (longueur 2) et SOS trop court.
    for sos in [&[0xFF, 0xDA, 0, 2][..], &[0xFF, 0xDA, 0, 4, 1, 1][..]] {
        let mut d = vec![0xFF, 0xD8];
        d.extend_from_slice(&[0xFF, 0xC0, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0]);
        d.extend_from_slice(sos);
        d.extend_from_slice(&[0xFF, 0xD9]);
        assert!(matches!(decode_bytes(&d, "jpg"), Err(Error::Corrupt)));
    }
}

#[test]
fn tiff_tuile_de_taille_nulle() {
    // IFD little-endian 4×4 avec TileWidth/TileLength = 0.
    fn tiff(entries: &[(u16, u32)]) -> Vec<u8> {
        let mut d = b"II*\0\x08\0\0\0".to_vec();
        d.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for &(tag, v) in entries {
            d.extend_from_slice(&tag.to_le_bytes());
            d.extend_from_slice(&4u16.to_le_bytes());
            d.extend_from_slice(&1u32.to_le_bytes());
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&[0; 4]);
        d.resize(d.len() + 64, 0x80);
        d
    }
    let base = [(256, 4), (257, 4), (258, 8), (259, 1), (262, 1), (277, 1)];
    let mut e = base.to_vec();
    e.extend_from_slice(&[(322, 0), (323, 0), (324, 200), (325, 16)]);
    assert!(matches!(
        decode_bytes(&tiff(&e), "tif"),
        Err(Error::Corrupt)
    ));
    // Profondeur 0 bit et profondeur 200 bits.
    for bps in [0, 200] {
        let mut e = base.to_vec();
        e[2] = (258, bps);
        e.extend_from_slice(&[(273, 200), (278, 4), (279, 16)]);
        assert!(decode_bytes(&tiff(&e), "tif").is_err());
    }
    // Le même fichier avec une profondeur valide se décode.
    let mut e = base.to_vec();
    e.extend_from_slice(&[(273, 200), (278, 4), (279, 16)]);
    assert!(decode_bytes(&tiff(&e), "tif").is_ok());
}

#[test]
fn psd_canaux_et_profondeur() {
    // 0 canal, 300 canaux, profondeur 3 bits.
    assert!(malformed("t.psd", |d| d[13] = 0).is_err());
    assert!(malformed("t.psd", |d| d[12] = 1).is_err());
    assert!(malformed("t.psd", |d| d[23] = 3).is_err());
    assert!(malformed("t.psd", |_| {}).is_ok());
}

#[test]
fn exr_compression_vide() {
    let mut d = vec![0x76, 0x2F, 0x31, 0x01, 2, 0, 0, 0];
    d.extend_from_slice(b"compression\0compression\0");
    d.extend_from_slice(&[0, 0, 0, 0, 0]);
    assert!(matches!(decode_bytes(&d, "exr"), Err(Error::Corrupt)));
}

#[test]
fn xcf_hierarchie_demesuree() {
    // Calque 2×2 dont la hiérarchie annonce 4 milliards de pixels.
    let mut d = b"gimp xcf file\0".to_vec();
    d.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 0]);
    d.extend_from_slice(&[0; 8]); // fin des propriétés
    let layer_at = (d.len() + 12) as u32;
    d.extend_from_slice(&layer_at.to_be_bytes());
    d.extend_from_slice(&[0; 8]); // fin des calques, fin des canaux
                                  // calque
    d.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, b'a', 0]);
    d.extend_from_slice(&[0; 8]); // fin des propriétés
    let hier_at = (d.len() + 8) as u32;
    d.extend_from_slice(&hier_at.to_be_bytes());
    d.extend_from_slice(&[0; 4]); // masque
    d.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 3]);
    let lvl = (d.len() + 4) as u32;
    d.extend_from_slice(&lvl.to_be_bytes());
    d.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]);
    let img = decode_bytes(&d, "xcf").expect("composite décodable");
    assert_eq!(raster(&img, 0).w, 2);
}

#[test]
fn eps_apercu_bits_invalides() {
    let d = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BeginPreview: 4 4 3 1\n%0F\n%%EndPreview\n";
    assert!(matches!(decode_bytes(d, "eps"), Err(Error::Variant(_))));
    let d = b"%!PS-Adobe-3.0 EPSF-3.0\n%%BeginPreview: 4 4 1 1\n%0F\n%%EndPreview\n";
    assert!(decode_bytes(d, "eps").is_ok());
}

#[test]
fn gltf_accesseur_demesure() {
    let j = br#"{"asset":{"version":"2.0"},
        "buffers":[{"byteLength":4,"uri":"data:application/octet-stream;base64,AAAAAA=="}],
        "bufferViews":[{"buffer":0,"byteLength":4}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":100000000,"type":"VEC3"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
        "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}]}"#;
    assert!(matches!(decode_bytes(j, "gltf"), Err(Error::Corrupt)));
}
