//! Identité visuelle Winlux : le logo « Prisme ».
//!
//! Un losange taillé en quatre facettes, violet à gauche (Linux, la nuit) et
//! cyan à droite (la lumière, « lux »). Le centre optique est remonté pour
//! donner un relief de gemme. Même géométrie et mêmes couleurs que
//! `scripts/render-brand.py` (SVG, PNG, GRUB) : modifier les deux ensemble.

use super::*;

/// Hauteur du centre optique, en millièmes de la taille du logo.
const CY: i32 = 420;

const fn hex(v: u32) -> u32 {
    rgb((v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF)
}

/// Facettes (couleur en haut, couleur en bas) : haut-gauche, bas-gauche,
/// haut-droite, bas-droite.
const FACETS: [(u32, u32); 4] = [
    (hex(0xB69CFF), hex(0x7C4DFF)),
    (hex(0x7C4DFF), hex(0x4B23C8)),
    (hex(0x7DE8FF), hex(0x22B8F0)),
    (hex(0x22B8F0), hex(0x125CD6)),
];

/// Couleur de marque (accent par défaut associé au logo).
pub const ACCENT: u32 = hex(0x7C4DFF);

/// Remplit une ligne `[l, r)` (bornes en 1/256 de pixel). Un bord qui a un
/// fond connu (`lbg`, `rbg`) est lissé ; sinon il est arrondi au pixel près,
/// pour que deux facettes voisines se touchent sans trou.
#[allow(clippy::too_many_arguments)]
unsafe fn span(
    hdc: Handle,
    b: &mut Brushes,
    y: i32,
    l: i32,
    r: i32,
    c: u32,
    lbg: Option<u32>,
    rbg: Option<u32>,
) {
    if r <= l {
        return;
    }
    let li = if lbg.is_some() {
        (l + 255) >> 8
    } else {
        (l + 128) >> 8
    };
    let ri = if rbg.is_some() {
        r >> 8
    } else {
        (r + 128) >> 8
    };
    if ri > li {
        fill(hdc, b, li, y, ri, y + 1, c);
    }
    if let Some(bg) = lbg {
        let cover = (li << 8) - l;
        if cover > 0 {
            fill(hdc, b, li - 1, y, li, y + 1, mix(bg, c, cover, 256));
        }
    }
    if let Some(bg) = rbg {
        let cover = r - (ri << 8);
        if cover > 0 {
            fill(hdc, b, ri, y, ri + 1, y + 1, mix(bg, c, cover, 256));
        }
    }
}

/// Dessine le logo dans le carré `(x, y, s)`. `alpha` (0..1000) fond le logo
/// vers `bg` ; sans `bg`, le logo est opaque et ses bords ne sont pas lissés.
pub unsafe fn logo(
    hdc: Handle,
    b: &mut Brushes,
    x: i32,
    y: i32,
    s: i32,
    alpha: i32,
    bg: Option<u32>,
) {
    if s <= 0 {
        return;
    }
    let base = bg.unwrap_or(0);
    let fade = |c: u32| {
        if bg.is_some() {
            mix(base, c, alpha.clamp(0, 1000), 1000)
        } else {
            c
        }
    };
    // Coordonnées en 1/256 de pixel pour des bords nets à toutes les tailles.
    let cx = (x << 8) + (s << 7);
    let edge_bg = bg.map(|_| fade(base));
    for j in 0..s {
        let v = (2 * j + 1) * 1000 / (2 * s);
        let hw = ((500 - (v - 500).abs()) as i64 * s as i64 * 256 / 1000) as i32;
        // Demi-largeur de la partie basse (du centre vers les sommets latéraux).
        let inner = if v < CY {
            0
        } else if v >= 500 {
            hw
        } else {
            ((v - CY) as i64 * 500 / (500 - CY) as i64 * s as i64 * 256 / 1000) as i32
        }
        .min(hw);
        let col = |k: usize| fade(mix(FACETS[k].0, FACETS[k].1, v, 1000));
        let row = y + j;
        // Gauche : facette haute à l'extérieur, basse à l'intérieur ; droite
        // symétrique. Seuls les bords extérieurs sont lissés.
        if inner < hw {
            span(hdc, b, row, cx - hw, cx - inner, col(0), edge_bg, None);
            span(hdc, b, row, cx + inner, cx + hw, col(2), None, edge_bg);
            span(hdc, b, row, cx - inner, cx, col(1), None, None);
            span(hdc, b, row, cx, cx + inner, col(3), None, None);
        } else {
            span(hdc, b, row, cx - hw, cx, col(1), edge_bg, None);
            span(hdc, b, row, cx, cx + hw, col(3), None, edge_bg);
        }
    }
}

/// Logo, nom « Winlux » et sous-titre éventuel, centrés sur `cx`.
/// Renvoie le bas du bloc.
pub unsafe fn lockup(
    hdc: Handle,
    b: &mut Brushes,
    f: &Fonts,
    cx: i32,
    top: i32,
    s: i32,
    alpha: i32,
    bg: u32,
    fg: u32,
) -> i32 {
    logo(hdc, b, cx - s / 2, top, s, alpha, Some(bg));
    let text_top = top + s + s / 4;
    text_centered(
        hdc,
        f.display,
        28,
        mix(bg, fg, alpha.clamp(0, 1000), 1000),
        cx,
        text_top,
        b"Winlux",
    );
    text_top + 40
}
