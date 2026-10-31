//! Métadonnées : EXIF (TIFF/IFD), IPTC (blocs Photoshop 8BIM) et XMP.
//! Les clés sont des identifiants stables ; la visionneuse les traduit.

use super::tiff::{find, Entry, Tiff};
use super::Image;

/// Bloc EXIF brut (en-tête TIFF « II*\0 » ou « MM\0* »).
pub fn exif(data: &[u8], img: &mut Image) {
    if let Some((t, first)) = Tiff::parse(data) {
        if let Some((ifd0, _)) = t.ifd(first) {
            exif_ifd(&t, &ifd0, img);
        }
    }
}

fn push(img: &mut Image, key: &str, val: String) {
    if !val.is_empty() && !img.meta.iter().any(|(k, _)| k == key) {
        img.meta.push((key.to_string(), val));
    }
}

pub fn exif_ifd(t: &Tiff, ifd0: &[Entry], img: &mut Image) {
    let texts = [
        (0x10F, "camera.make"),
        (0x110, "camera.model"),
        (0x131, "software"),
        (0x132, "date"),
        (0x13B, "artist"),
        (0x8298, "copyright"),
        (0x10E, "description"),
    ];
    for (tag, key) in texts {
        if let Some(s) = find(ifd0, tag).and_then(|e| t.text(e)) {
            push(img, key, s);
        }
    }
    if let Some(o) = find(ifd0, 0x112).and_then(|e| t.val(e, 0)) {
        if (1..=8).contains(&o) && img.orientation == 0 {
            img.orientation = o as u8;
        }
    }
    if let Some(off) = find(ifd0, 0x8769).and_then(|e| t.val(e, 0)) {
        if let Some((ex, _)) = t.ifd(off as usize) {
            if let Some(s) = find(&ex, 0x9003).and_then(|e| t.text(e)) {
                push(img, "date.taken", s);
            }
            if let Some(v) = find(&ex, 0x829A).and_then(|e| t.rational(e, 0)) {
                let s = if v > 0.0 && v < 1.0 {
                    format!("1/{} s", (1.0 / v).round())
                } else {
                    format!("{v} s")
                };
                push(img, "exposure", s);
            }
            if let Some(v) = find(&ex, 0x829D).and_then(|e| t.rational(e, 0)) {
                push(img, "aperture", format!("f/{v:.1}"));
            }
            if let Some(v) = find(&ex, 0x8827).and_then(|e| t.val(e, 0)) {
                push(img, "iso", format!("ISO {v}"));
            }
            if let Some(v) = find(&ex, 0x920A).and_then(|e| t.rational(e, 0)) {
                push(img, "focal", format!("{v:.0} mm"));
            }
            if let Some(v) = find(&ex, 0x9204).and_then(|e| t.rational(e, 0)) {
                push(img, "exposure.bias", format!("{v:+.1} EV"));
            }
            if let Some(v) = find(&ex, 0x9209).and_then(|e| t.val(e, 0)) {
                push(img, "flash", if v & 1 == 1 { "1" } else { "0" }.to_string());
            }
            if let Some(s) = find(&ex, 0xA434).and_then(|e| t.text(e)) {
                push(img, "lens", s);
            }
        }
    }
    if let Some(off) = find(ifd0, 0x8825).and_then(|e| t.val(e, 0)) {
        if let Some((gps, _)) = t.ifd(off as usize) {
            let coord = |tag: u16, reft: u16| -> Option<f64> {
                let e = find(&gps, tag)?;
                let d = t.rational(e, 0)? + t.rational(e, 1)? / 60.0 + t.rational(e, 2)? / 3600.0;
                let r = find(&gps, reft).and_then(|e| t.text(e)).unwrap_or_default();
                Some(if r == "S" || r == "W" { -d } else { d })
            };
            if let (Some(la), Some(lo)) = (coord(2, 1), coord(4, 3)) {
                push(img, "gps", format!("{la:.6}, {lo:.6}"));
            }
        }
    }
}

/// Ressources Photoshop (APP13 JPEG, PSD) : IPTC-NAA (0x0404).
pub fn photoshop_irb(seg: &[u8], img: &mut Image) {
    let start = seg
        .windows(4)
        .position(|w| w == b"8BIM")
        .unwrap_or(seg.len());
    irb(&seg[start..], img);
}

pub fn irb(d: &[u8], img: &mut Image) {
    let mut p = 0;
    while p + 12 <= d.len() && &d[p..p + 4] == b"8BIM" {
        let id = super::be16(d, p + 4);
        let nlen = d[p + 6] as usize;
        let mut q = p + 7 + nlen;
        if (nlen + 1) % 2 == 1 {
            q += 1;
        }
        if q + 4 > d.len() {
            break;
        }
        let size = super::be32(d, q) as usize;
        let body = d.get(q + 4..q + 4 + size).unwrap_or(&[]);
        if id == 0x0404 {
            iptc(body, img);
        }
        p = q + 4 + size + (size & 1);
    }
}

fn iptc(d: &[u8], img: &mut Image) {
    let mut p = 0;
    let mut keywords: Vec<String> = Vec::new();
    while p + 5 <= d.len() && d[p] == 0x1C {
        let rec = d[p + 1];
        let ds = d[p + 2];
        let len = super::be16(d, p + 3) as usize;
        let v = d.get(p + 5..p + 5 + len).unwrap_or(&[]);
        let s = String::from_utf8(v.to_vec())
            .unwrap_or_else(|_| v.iter().map(|&c| c as char).collect());
        if rec == 2 {
            match ds {
                5 => push(img, "iptc.title", s),
                25 => keywords.push(s),
                80 => push(img, "artist", s),
                90 => push(img, "iptc.city", s),
                101 => push(img, "iptc.country", s),
                105 => push(img, "iptc.headline", s),
                116 => push(img, "copyright", s),
                120 => push(img, "description", s),
                _ => {}
            }
        }
        p += 5 + len;
    }
    if !keywords.is_empty() {
        push(img, "iptc.keywords", keywords.join(", "));
    }
}

/// Paquet XMP : titre, créateur, droits, note.
pub fn xmp(seg: &[u8], img: &mut Image) {
    let s = String::from_utf8_lossy(seg);
    for (tag, key) in [
        ("dc:title", "iptc.title"),
        ("dc:creator", "artist"),
        ("dc:rights", "copyright"),
        ("dc:description", "description"),
        ("xmp:CreatorTool", "software"),
    ] {
        if let Some(v) = xml_text(&s, tag) {
            push(img, key, v);
        }
    }
    if let Some(v) = attr(&s, "xmp:Rating") {
        push(img, "rating", v);
    }
}

fn xml_text(s: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let a = s.find(&open)?;
    let close = format!("</{tag}>");
    let b = a + s[a..].find(&close)?;
    let inner = &s[a..b];
    // Prend le texte le plus profond (rdf:li).
    let mut out = String::new();
    let mut depth_text = false;
    for part in inner.split('<') {
        if let Some(i) = part.find('>') {
            let t = part[i + 1..].trim();
            if !t.is_empty() {
                out = t.to_string();
                depth_text = true;
            }
        }
    }
    if depth_text {
        Some(super::svg::unescape(&out))
    } else {
        None
    }
}

pub fn attr(s: &str, name: &str) -> Option<String> {
    let k = format!("{name}=\"");
    let a = s.find(&k)? + k.len();
    let b = a + s[a..].find('"')?;
    Some(s[a..b].to_string())
}
