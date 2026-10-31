//! Rendu des commandes GDI d'un DC imprimante en PDF (format accepté par
//! tous les filtres CUPS). Écrit de zéro : polices standard PDF (Helvetica,
//! Times, Courier en WinAnsiEncoding), tracés vectoriels et images RGB
//! compressées en RunLength.

use super::gui::{DrawCmd, FontSpec, RasterOp, Rect};

/// Une page imprimée : commandes GDI en unités de périphérique.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub cmds: Vec<DrawCmd>,
    /// Taille physique de la page en pixels périphérique.
    pub width: i32,
    pub height: i32,
}

/// Métadonnées du document.
pub struct DocInfo<'a> {
    pub title: &'a str,
    /// Résolution du périphérique (points par pouce).
    pub dpi: i32,
    /// Décalage de la zone imprimable (marges physiques) en pixels.
    pub offset_x: i32,
    pub offset_y: i32,
    /// Impression en niveaux de gris (`DMCOLOR_MONOCHROME`).
    pub grayscale: bool,
}

/// Police standard PDF retenue pour une police GDI.
fn base_font(font: &FontSpec) -> &'static str {
    let face = font.face.to_ascii_lowercase();
    let bold = font.weight >= 600;
    let italic = font.italic;
    let family = if face.contains("courier")
        || face.contains("mono")
        || face.contains("consol")
        || face == "fixed"
        || face.contains("terminal")
        || face.contains("lucida console")
    {
        2
    } else if face.contains("times")
        || face.contains("serif") && !face.contains("sans")
        || face.contains("georgia")
        || face.contains("garamond")
        || face.contains("cambria")
        || face.contains("roman")
        || face.contains("book")
    {
        1
    } else {
        0
    };
    match (family, bold, italic) {
        (0, false, false) => "Helvetica",
        (0, true, false) => "Helvetica-Bold",
        (0, false, true) => "Helvetica-Oblique",
        (0, true, true) => "Helvetica-BoldOblique",
        (1, false, false) => "Times-Roman",
        (1, true, false) => "Times-Bold",
        (1, false, true) => "Times-Italic",
        (1, true, true) => "Times-BoldItalic",
        (_, false, false) => "Courier",
        (_, true, false) => "Courier-Bold",
        (_, false, true) => "Courier-Oblique",
        _ => "Courier-BoldOblique",
    }
}

/// Taille d'em (pixels) d'une police GDI : hauteur négative = em, positive =
/// hauteur de cellule (em + interligne interne), nulle = 12 pt.
pub fn em_size(font: &FontSpec, dpi: i32) -> f64 {
    match font.height {
        0 => f64::from(dpi) * 12.0 / 72.0,
        h if h < 0 => f64::from(-h),
        h => f64::from(h) / 1.15,
    }
}

/// Chasse approchée d'un caractère (en em) pour les fonds et soulignements.
fn char_width(family_mono: bool, c: char) -> f64 {
    if family_mono {
        return 0.6;
    }
    match c {
        'i' | 'j' | 'l' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' => 0.25,
        'f' | 't' | 'r' | 'I' | ' ' | '(' | ')' | '[' | ']' => 0.3,
        'm' | 'w' | 'M' | 'W' => 0.85,
        'A'..='Z' => 0.68,
        _ => 0.53,
    }
}

pub fn text_width(font: &FontSpec, dpi: i32, text: &str) -> f64 {
    let mono = base_font(font).starts_with("Courier");
    let em = em_size(font, dpi);
    text.chars().map(|c| char_width(mono, c) * em).sum()
}

/// Encodage WinAnsi d'un texte (caractères absents → « ? »), échappé pour
/// une chaîne littérale PDF.
fn pdf_string(text: &str) -> Vec<u8> {
    const HIGH: [(u16, u8); 27] = [
        (0x20ac, 0x80),
        (0x201a, 0x82),
        (0x0192, 0x83),
        (0x201e, 0x84),
        (0x2026, 0x85),
        (0x2020, 0x86),
        (0x2021, 0x87),
        (0x02c6, 0x88),
        (0x2030, 0x89),
        (0x0160, 0x8a),
        (0x2039, 0x8b),
        (0x0152, 0x8c),
        (0x017d, 0x8e),
        (0x2018, 0x91),
        (0x2019, 0x92),
        (0x201c, 0x93),
        (0x201d, 0x94),
        (0x2022, 0x95),
        (0x2013, 0x96),
        (0x2014, 0x97),
        (0x02dc, 0x98),
        (0x2122, 0x99),
        (0x0161, 0x9a),
        (0x203a, 0x9b),
        (0x0153, 0x9c),
        (0x017e, 0x9e),
        (0x0178, 0x9f),
    ];
    let mut out = vec![b'('];
    for c in text.chars() {
        let code = c as u32;
        let b = if (0x20..0x7f).contains(&code) || (0xa0..=0xff).contains(&code) {
            code as u8
        } else if c == '\t' {
            b' '
        } else {
            HIGH.iter()
                .find(|(u, _)| u32::from(*u) == code)
                .map(|(_, b)| *b)
                .unwrap_or(b'?')
        };
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

/// Compression RunLength (filtre PDF `/RunLengthDecode`).
pub fn run_length(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 4 + 16);
    let mut i = 0;
    while i < data.len() {
        // Plage répétée ?
        let mut run = 1;
        while i + run < data.len() && run < 128 && data[i + run] == data[i] {
            run += 1;
        }
        if run >= 2 {
            out.push((257 - run) as u8);
            out.push(data[i]);
            i += run;
            continue;
        }
        // Plage littérale jusqu'à la prochaine répétition.
        let start = i;
        while i < data.len() && i - start < 128 && !(i + 1 < data.len() && data[i + 1] == data[i]) {
            i += 1;
        }
        if i == start {
            i += 1;
        }
        out.push((i - start - 1) as u8);
        out.extend_from_slice(&data[start..i]);
    }
    out.push(128);
    out
}

struct Writer {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl Writer {
    /// Réserve un numéro d'objet.
    fn reserve(&mut self) -> usize {
        self.offsets.push(0);
        self.offsets.len()
    }

    fn object(&mut self, id: usize, body: &[u8]) {
        self.offsets[id - 1] = self.out.len();
        self.out
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.out.extend_from_slice(body);
        self.out.extend_from_slice(b"\nendobj\n");
    }

    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        let mut body = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.object(id, &body);
    }
}

fn color_ops(color: u32, gray: bool, fill: bool) -> String {
    let r = f64::from(color & 0xff) / 255.0;
    let g = f64::from((color >> 8) & 0xff) / 255.0;
    let b = f64::from((color >> 16) & 0xff) / 255.0;
    if gray {
        let l = 0.299 * r + 0.587 * g + 0.114 * b;
        format!("{l:.3} {}", if fill { "g" } else { "G" })
    } else {
        format!("{r:.3} {g:.3} {b:.3} {}", if fill { "rg" } else { "RG" })
    }
}

/// Contexte de conversion pixels → points PDF pour une page.
struct PageCtx {
    scale: f64,
    height_pt: f64,
    off_x: f64,
    off_y: f64,
}

impl PageCtx {
    fn x(&self, x: f64) -> f64 {
        (x + self.off_x) * self.scale
    }
    fn y(&self, y: f64) -> f64 {
        self.height_pt - (y + self.off_y) * self.scale
    }
    fn rect(&self, r: &Rect) -> String {
        format!(
            "{:.2} {:.2} {:.2} {:.2} re",
            self.x(f64::from(r.left)),
            self.y(f64::from(r.bottom)),
            f64::from(r.right - r.left) * self.scale,
            f64::from(r.bottom - r.top) * self.scale
        )
    }
}

/// Chemin d'une ellipse (4 courbes de Bézier) inscrite dans `r`.
fn ellipse_path(c: &PageCtx, r: &Rect) -> String {
    const K: f64 = 0.552_284_7;
    let cx = c.x(f64::from(r.left + r.right) / 2.0);
    let cy = c.y(f64::from(r.top + r.bottom) / 2.0);
    let rx = f64::from(r.right - r.left) / 2.0 * c.scale;
    let ry = f64::from(r.bottom - r.top) / 2.0 * c.scale;
    let (kx, ky) = (rx * K, ry * K);
    format!(
        "{:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c h",
        cx + rx,
        cy,
        cx + rx,
        cy + ky,
        cx + kx,
        cy + ry,
        cx,
        cy + ry,
        cx - kx,
        cy + ry,
        cx - rx,
        cy + ky,
        cx - rx,
        cy,
        cx - rx,
        cy - ky,
        cx - kx,
        cy - ry,
        cx,
        cy - ry,
        cx + kx,
        cy - ry,
        cx + rx,
        cy - ky,
        cx + rx,
        cy
    )
}

/// Chemin d'un rectangle arrondi.
fn round_rect_path(c: &PageCtx, r: &Rect, rx: i32, ry: i32) -> String {
    const K: f64 = 0.552_284_7;
    let l = c.x(f64::from(r.left));
    let rt = c.x(f64::from(r.right));
    let t = c.y(f64::from(r.top));
    let b = c.y(f64::from(r.bottom));
    let ax = (f64::from(rx) / 2.0 * c.scale).min((rt - l) / 2.0).max(0.0);
    let ay = (f64::from(ry) / 2.0 * c.scale).min((t - b) / 2.0).max(0.0);
    let (kx, ky) = (ax * K, ay * K);
    format!(
        "{:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} l \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c h",
        l + ax,
        b,
        rt - ax,
        b,
        rt - ax + kx,
        b,
        rt,
        b + ay - ky,
        rt,
        b + ay,
        rt,
        t - ay,
        rt,
        t - ay + ky,
        rt - ax + kx,
        t,
        rt - ax,
        t,
        l + ax,
        t,
        l + ax - kx,
        t,
        l,
        t - ay + ky,
        l,
        t - ay,
        l,
        b + ay,
        l,
        b + ay - ky,
        l + ax - kx,
        b,
        l + ax,
        b
    )
}

/// Produit le PDF complet des pages.
pub fn render(pages: &[Page], info: &DocInfo) -> Vec<u8> {
    let dpi = info.dpi.max(1);
    let scale = 72.0 / f64::from(dpi);
    let mut w = Writer {
        out: b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec(),
        offsets: Vec::new(),
    };
    let catalog = w.reserve();
    let pages_id = w.reserve();
    let info_id = w.reserve();
    let mut fonts: Vec<(&'static str, usize)> = Vec::new();
    let mut page_ids = Vec::new();
    let empty = [Page {
        cmds: Vec::new(),
        width: dpi * 210 * 10 / 254,
        height: dpi * 297 * 10 / 254,
    }];
    let pages = if pages.is_empty() { &empty[..] } else { pages };
    for page in pages {
        let width_pt = f64::from(page.width.max(1)) * scale;
        let height_pt = f64::from(page.height.max(1)) * scale;
        let ctx = PageCtx {
            scale,
            height_pt,
            off_x: f64::from(info.offset_x),
            off_y: f64::from(info.offset_y),
        };
        let gray = info.grayscale;
        let mut content = String::new();
        let mut images: Vec<(usize, String)> = Vec::new();
        let mut page_fonts: Vec<(&'static str, usize)> = Vec::new();
        for cmd in &page.cmds {
            match cmd {
                DrawCmd::FillRect { rect, color } => {
                    content += &format!("{} {} f\n", color_ops(*color, gray, true), ctx.rect(rect));
                }
                DrawCmd::PatternRect { rect, color, rop } => {
                    let c = match rop {
                        RasterOp::Clear => Some(0),
                        RasterOp::Set => Some(0x00ff_ffff),
                        RasterOp::Copy | RasterOp::PatCopy | RasterOp::Or => Some(*color),
                        _ => None,
                    };
                    if let Some(c) = c {
                        content += &format!("{} {} f\n", color_ops(c, gray, true), ctx.rect(rect));
                    }
                }
                DrawCmd::FrameRect { rect, color, width } => {
                    let lw = f64::from((*width).max(1));
                    let inset = lw / 2.0;
                    let r = format!(
                        "{:.2} {:.2} {:.2} {:.2} re",
                        ctx.x(f64::from(rect.left) + inset),
                        ctx.y(f64::from(rect.bottom) - inset),
                        (f64::from(rect.right - rect.left) - lw).max(0.0) * scale,
                        (f64::from(rect.bottom - rect.top) - lw).max(0.0) * scale
                    );
                    content += &format!(
                        "{} {:.2} w {} S\n",
                        color_ops(*color, gray, false),
                        lw * scale,
                        r
                    );
                }
                DrawCmd::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color,
                    width,
                } => {
                    content += &format!(
                        "{} {:.2} w 1 J {:.2} {:.2} m {:.2} {:.2} l S\n",
                        color_ops(*color, gray, false),
                        f64::from((*width).max(1)) * scale,
                        ctx.x(f64::from(*x1)),
                        ctx.y(f64::from(*y1)),
                        ctx.x(f64::from(*x2)),
                        ctx.y(f64::from(*y2))
                    );
                }
                DrawCmd::Ellipse { rect, color } => {
                    content += &format!(
                        "{} {} f\n",
                        color_ops(*color, gray, true),
                        ellipse_path(&ctx, rect)
                    );
                }
                DrawCmd::RoundRect {
                    rect,
                    color,
                    rx,
                    ry,
                } => {
                    content += &format!(
                        "{} {} f\n",
                        color_ops(*color, gray, true),
                        round_rect_path(&ctx, rect, *rx, *ry)
                    );
                }
                DrawCmd::Text {
                    x,
                    y,
                    text,
                    color,
                    bk,
                    font,
                } => {
                    if text.is_empty() {
                        continue;
                    }
                    let base = base_font(font);
                    let fid = match page_fonts.iter().find(|(b, _)| *b == base) {
                        Some((_, id)) => *id,
                        None => {
                            let id = match fonts.iter().find(|(b, _)| *b == base) {
                                Some((_, id)) => *id,
                                None => {
                                    let id = w.reserve();
                                    fonts.push((base, id));
                                    id
                                }
                            };
                            page_fonts.push((base, id));
                            id
                        }
                    };
                    let em = em_size(font, dpi);
                    let cell = if font.height > 0 {
                        f64::from(font.height)
                    } else {
                        em * 1.15
                    };
                    let tw = text_width(font, dpi, text);
                    if let Some(bk) = bk {
                        content += &format!(
                            "{} {:.2} {:.2} {:.2} {:.2} re f\n",
                            color_ops(*bk, gray, true),
                            ctx.x(f64::from(*x)),
                            ctx.y(f64::from(*y) + cell),
                            tw * scale,
                            cell * scale
                        );
                    }
                    let baseline = f64::from(*y) + (cell - em) + em * 0.8;
                    let size = em * scale;
                    content += &format!(
                        "BT {} /F{fid} {size:.2} Tf {:.2} {:.2} Td ",
                        color_ops(*color, gray, true),
                        ctx.x(f64::from(*x)),
                        ctx.y(baseline)
                    );
                    // Octets WinAnsi portés en caractères Latin-1 (< 256).
                    content.extend(pdf_string(text).iter().map(|&b| char::from(b)));
                    content += " Tj ET\n";
                    let mut decorate = |offset: f64| {
                        content += &format!(
                            "{} {:.2} w {:.2} {:.2} m {:.2} {:.2} l S\n",
                            color_ops(*color, gray, false),
                            (em * 0.06).max(1.0) * scale,
                            ctx.x(f64::from(*x)),
                            ctx.y(baseline + offset),
                            ctx.x(f64::from(*x) + tw),
                            ctx.y(baseline + offset)
                        );
                    };
                    if font.underline {
                        decorate(em * 0.12);
                    }
                    if font.strikeout {
                        decorate(-em * 0.28);
                    }
                }
                DrawCmd::Bitmap { rect, pixels, .. } => {
                    let iw = (rect.right - rect.left).max(0) as usize;
                    let ih = (rect.bottom - rect.top).max(0) as usize;
                    if iw == 0 || ih == 0 || pixels.len() < iw * ih {
                        continue;
                    }
                    let mut raw = Vec::with_capacity(iw * ih * if gray { 1 } else { 3 });
                    for &p in &pixels[..iw * ih] {
                        let (r, g, b) = (p & 0xff, (p >> 8) & 0xff, (p >> 16) & 0xff);
                        if gray {
                            raw.push(((r * 299 + g * 587 + b * 114) / 1000) as u8);
                        } else {
                            raw.extend_from_slice(&[r as u8, g as u8, b as u8]);
                        }
                    }
                    let id = w.reserve();
                    let name = format!("Im{}", images.len() + 1);
                    w.stream(
                        id,
                        &format!(
                            "/Type /XObject /Subtype /Image /Width {iw} /Height {ih} \
                             /ColorSpace /{} /BitsPerComponent 8 /Filter /RunLengthDecode",
                            if gray { "DeviceGray" } else { "DeviceRGB" }
                        ),
                        &run_length(&raw),
                    );
                    content += &format!(
                        "q {:.2} 0 0 {:.2} {:.2} {:.2} cm /{name} Do Q\n",
                        iw as f64 * scale,
                        ih as f64 * scale,
                        ctx.x(f64::from(rect.left)),
                        ctx.y(f64::from(rect.bottom))
                    );
                    images.push((id, name));
                }
                // Copie depuis une fenêtre à l'écran : non imprimable.
                DrawCmd::Blit { .. } => {}
            }
        }
        // Le texte PDF contient des octets WinAnsi : on les réécrit tels quels.
        let content_bytes: Vec<u8> = content
            .chars()
            .map(|c| {
                if (c as u32) < 256 {
                    c as u32 as u8
                } else {
                    b'?'
                }
            })
            .collect();
        let content_id = w.reserve();
        w.stream(content_id, "", &content_bytes);
        let page_id = w.reserve();
        let mut resources = String::from("/ProcSet [/PDF /Text /ImageB /ImageC]");
        if !page_fonts.is_empty() {
            resources += " /Font <<";
            for (_, id) in &page_fonts {
                resources += &format!(" /F{id} {id} 0 R");
            }
            resources += " >>";
        }
        if !images.is_empty() {
            resources += " /XObject <<";
            for (id, name) in &images {
                resources += &format!(" /{name} {id} 0 R");
            }
            resources += " >>";
        }
        w.object(
            page_id,
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {width_pt:.2} {height_pt:.2}] \
                 /Resources << {resources} >> /Contents {content_id} 0 R >>"
            )
            .as_bytes(),
        );
        page_ids.push(page_id);
    }
    for (base, id) in &fonts {
        w.object(
            *id,
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>"
            )
            .as_bytes(),
        );
    }
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    w.object(
        pages_id,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            page_ids.len()
        )
        .as_bytes(),
    );
    w.object(
        catalog,
        format!("<< /Type /Catalog /Pages {pages_id} 0 R >>").as_bytes(),
    );
    let mut info_body = b"<< /Title ".to_vec();
    info_body.extend_from_slice(&pdf_string(info.title));
    info_body.extend_from_slice(b" /Producer (Winlux) /Creator (Winlux winspool) >>");
    w.object(info_id, &info_body);
    let xref = w.out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", w.offsets.len() + 1);
    for off in &w.offsets {
        tail += &format!("{off:010} 00000 n \n");
    }
    tail += &format!(
        "trailer\n<< /Size {} /Root {catalog} 0 R /Info {info_id} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        w.offsets.len() + 1
    );
    w.out.extend_from_slice(tail.as_bytes());
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_run_length(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let n = data[i];
            i += 1;
            match n {
                128 => break,
                0..=127 => {
                    out.extend_from_slice(&data[i..i + n as usize + 1]);
                    i += n as usize + 1;
                }
                _ => {
                    out.extend(std::iter::repeat_n(data[i], 257 - n as usize));
                    i += 1;
                }
            }
        }
        out
    }

    #[test]
    fn runlength_aller_retour() {
        for data in [
            vec![],
            vec![1],
            vec![1, 2, 3, 4],
            vec![7; 300],
            (0..1000).map(|i| (i % 7 / 3) as u8).collect::<Vec<_>>(),
        ] {
            assert_eq!(decode_run_length(&run_length(&data)), data);
        }
        assert!(run_length(&[255; 10000]).len() < 200);
    }

    /// Vérifie que chaque entrée xref pointe bien sur « N 0 obj ».
    fn check_xref(pdf: &[u8]) -> usize {
        let find = |needle: &[u8], from: usize| {
            pdf[from..]
                .windows(needle.len())
                .position(|w| w == needle)
                .map(|p| p + from)
        };
        let sx = (0..pdf.len())
            .rev()
            .find(|&i| pdf[i..].starts_with(b"startxref\n"))
            .unwrap();
        let tail = String::from_utf8_lossy(&pdf[sx + 10..]).into_owned();
        let off: usize = tail.lines().next().unwrap().parse().unwrap();
        assert!(pdf[off..].starts_with(b"xref\n"));
        let table = String::from_utf8_lossy(&pdf[off..]).into_owned();
        let mut lines = table.lines().skip(1);
        let count: usize = lines
            .next()
            .unwrap()
            .split(' ')
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        lines.next();
        for id in 1..count {
            let entry = lines.next().unwrap();
            let pos: usize = entry[..10].parse().unwrap();
            assert_eq!(
                find(format!("{id} 0 obj").as_bytes(), pos),
                Some(pos),
                "objet {id} mal référencé"
            );
        }
        count - 1
    }

    #[test]
    fn pdf_valide_avec_texte_formes_et_image() {
        let font = FontSpec {
            height: -50,
            weight: 700,
            face: "Arial".into(),
            underline: true,
            ..FontSpec::default()
        };
        let page = Page {
            width: 2480,
            height: 3508,
            cmds: vec![
                DrawCmd::Text {
                    x: 100,
                    y: 100,
                    text: "Été (test) 100 €".into(),
                    color: 0,
                    bk: Some(0x00ff_ffff),
                    font,
                },
                DrawCmd::FillRect {
                    rect: Rect {
                        left: 10,
                        top: 10,
                        right: 110,
                        bottom: 60,
                    },
                    color: 0x0000_00ff,
                },
                DrawCmd::Ellipse {
                    rect: Rect {
                        left: 0,
                        top: 0,
                        right: 300,
                        bottom: 200,
                    },
                    color: 0x00ff_0000,
                },
                DrawCmd::Line {
                    x1: 0,
                    y1: 0,
                    x2: 2480,
                    y2: 3508,
                    color: 0,
                    width: 3,
                },
                DrawCmd::Bitmap {
                    rect: Rect {
                        left: 500,
                        top: 500,
                        right: 504,
                        bottom: 502,
                    },
                    pixels: vec![0x0000_00ff; 8],
                    rop: RasterOp::Copy,
                },
            ],
        };
        let pdf = render(
            &[page.clone(), page],
            &DocInfo {
                title: "Rapport",
                dpi: 300,
                offset_x: 0,
                offset_y: 0,
                grayscale: false,
            },
        );
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 2"));
        assert!(text.contains("/BaseFont /Helvetica-Bold"));
        assert!(text.contains("/MediaBox [0 0 595.20 841.92]"));
        // « É » (0xC9) et « € » (0x80) en WinAnsi, parenthèses échappées.
        assert!(pdf.windows(11).any(|w| w == b"(\xC9t\xE9 \\(test"));
        assert!(pdf.windows(5).any(|w| w == b"100 \x80"));
        // Une seule police partagée par les deux pages.
        assert_eq!(text.matches("/Subtype /Type1").count(), 1);
        assert_eq!(check_xref(&pdf), 3 + 1 + 2 * 3);
    }

    #[test]
    fn pdf_vide_et_niveaux_de_gris() {
        let pdf = render(
            &[],
            &DocInfo {
                title: "",
                dpi: 300,
                offset_x: 0,
                offset_y: 0,
                grayscale: true,
            },
        );
        assert!(String::from_utf8_lossy(&pdf).contains("/Count 1"));
        check_xref(&pdf);
        assert_eq!(color_ops(0x0000_00ff, true, true), "0.299 g");
    }

    #[test]
    fn choix_des_polices() {
        let f = |face: &str, weight, italic| FontSpec {
            face: face.into(),
            weight,
            italic,
            ..FontSpec::default()
        };
        assert_eq!(base_font(&f("Times New Roman", 400, true)), "Times-Italic");
        assert_eq!(base_font(&f("Courier New", 700, false)), "Courier-Bold");
        assert_eq!(base_font(&f("Segoe UI", 400, false)), "Helvetica");
        assert_eq!(base_font(&f("Consolas", 400, false)), "Courier");
    }
}
