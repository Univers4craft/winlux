//! Icônes de l'Explorateur : jeu maison « Prisme » (assets/icons/explorateur,
//! le même que la session native, scripts/gen-explorer-icons.py), composées par
//! AlphaBlend sur le fond réel (survol, sélection, thème clair ou sombre).
//! Sans icône pour un glyphe ou une taille, l'appelant garde ses primitives.

use super::*;

include!("explorer_icons_data.rs");

static ICON_DATA: &[u8; ICON_BYTES] = include_bytes!("explorer_icons.bin");

/// Bitmaps déjà préparées : (ligne de ICON_TABLE, couleur de teinte, DC, bitmap).
const CACHE_MAX: usize = 96;
#[derive(Clone, Copy)]
struct Cached {
    row: u16,
    tint: u32,
    dc: Handle,
    bmp: Handle,
}
static mut CACHE: [Cached; CACHE_MAX] = [Cached { row: u16::MAX, tint: 0, dc: core::ptr::null_mut(), bmp: core::ptr::null_mut() }; CACHE_MAX];
static mut CACHE_NEXT: usize = 0;

/// Glyphe du shell → icône intégrée, quand il y en a une.
pub(super) fn of_glyph(g: Glyph) -> Option<Icon> {
    Some(match g {
        Glyph::Folder => Icon::Folder,
        Glyph::Home => Icon::Home,
        Glyph::Desktop => Icon::Desktop,
        Glyph::Download => Icon::FolderDownload,
        Glyph::Video => Icon::Video,
        Glyph::Music => Icon::Audio,
        Glyph::Computer => Icon::Computer,
        Glyph::Trash => Icon::Trash,
        Glyph::Drive => Icon::Drive,
        Glyph::LinuxDrive => Icon::LinuxDrive,
        Glyph::Removable => Icon::Removable,
        Glyph::DiskImage => Icon::DiskImage,
        Glyph::NetDrive => Icon::Network,
        Glyph::Exe => Icon::Exe,
        Glyph::Image => Icon::Image,
        Glyph::Audio => Icon::Audio,
        Glyph::Archive => Icon::Archive,
        Glyph::Code => Icon::Code,
        Glyph::Pdf => Icon::Pdf,
        Glyph::Text => Icon::Text,
        Glyph::File => Icon::File,
        Glyph::Cut => Icon::Cut,
        Glyph::Copy => Icon::Copy,
        Glyph::Paste => Icon::Paste,
        Glyph::Delete => Icon::Delete,
        Glyph::Rename => Icon::Rename,
        Glyph::Sort => Icon::Sort,
        Glyph::View => Icon::View,
        _ => return None,
    })
}

/// Dossiers d'Accueil (ordre de PLACES) : dossiers jaunes à pictogramme.
pub(super) fn of_place(i: usize) -> Icon {
    match i {
        0 => Icon::Desktop,
        1 => Icon::FolderDownload,
        2 => Icon::FolderDocuments,
        3 => Icon::FolderPictures,
        4 => Icon::FolderMusic,
        5 => Icon::FolderVideo,
        _ => Icon::Folder,
    }
}

/// Ligne de la table pour `icon` à `size` px, dans la variante du thème
/// (`light`) quand elle existe : taille exacte, sinon la plus petite
/// au-dessus (réduite), sinon la plus grande.
fn pick_in(icon: Icon, size: i32, light: bool) -> Option<usize> {
    let has_light = ICON_TABLE.iter().any(|r| r.0 == icon && r.4);
    let want = light && has_light;
    let mut best: Option<usize> = None;
    for (i, &(ic, s, _, _, l)) in ICON_TABLE.iter().enumerate() {
        if ic != icon || l != want {
            continue;
        }
        let s = s as i32;
        if s == size {
            return Some(i);
        }
        best = match best {
            None => Some(i),
            Some(b) => {
                let bs = ICON_TABLE[b].1 as i32;
                let better = if s > size { bs < size || s < bs } else { bs < size && s > bs };
                if better { Some(i) } else { Some(b) }
            }
        };
    }
    best
}

fn pick(icon: Icon, size: i32) -> Option<usize> {
    pick_in(icon, size, unsafe { !crate::gfx::DARK })
}

/// Pixels BGRA prémultipliés de la ligne `row`, teintés si c'est un glyphe.
unsafe fn fill_pixels(row: usize, tint: u32, out: &mut [u32]) {
    let (_, s, off, mask, _) = ICON_TABLE[row];
    let n = (s as usize) * (s as usize);
    let off = off as usize;
    if mask {
        let (r, g, b) = (tint & 0xFF, (tint >> 8) & 0xFF, (tint >> 16) & 0xFF);
        for i in 0..n {
            let a = ICON_DATA[off + i] as u32;
            let pm = |c: u32| (c * a + 127) / 255;
            out[i] = a << 24 | pm(r) << 16 | pm(g) << 8 | pm(b);
        }
    } else {
        for i in 0..n {
            let p = &ICON_DATA[off + i * 4..off + i * 4 + 4];
            out[i] = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
        }
    }
}

unsafe fn bitmap(row: usize, tint: u32) -> Option<Handle> {
    let tint = if ICON_TABLE[row].3 { tint } else { 0 };
    for c in CACHE.iter() {
        if c.row as usize == row && c.tint == tint && !c.dc.is_null() {
            return Some(c.dc);
        }
    }
    let s = ICON_TABLE[row].1 as i32;
    let info = BitmapInfoHeader {
        size: 40,
        width: s,
        height: -s,
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_pels_per_meter: 0,
        y_pels_per_meter: 0,
        colors_used: 0,
        colors_important: 0,
    };
    let slot = &mut CACHE[CACHE_NEXT];
    CACHE_NEXT = (CACHE_NEXT + 1) % CACHE_MAX;
    if !slot.dc.is_null() {
        DeleteDC(slot.dc);
        DeleteObject(slot.bmp);
        *slot = Cached { row: u16::MAX, tint: 0, dc: core::ptr::null_mut(), bmp: core::ptr::null_mut() };
    }
    let dc = CreateCompatibleDC(core::ptr::null_mut());
    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let bmp = CreateDIBSection(dc, &info, 0, &mut bits, core::ptr::null_mut(), 0);
    if dc.is_null() || bmp.is_null() || bits.is_null() {
        if !dc.is_null() {
            DeleteDC(dc);
        }
        return None;
    }
    fill_pixels(row, tint, core::slice::from_raw_parts_mut(bits as *mut u32, (s * s) as usize));
    SelectObject(dc, bmp);
    *slot = Cached { row: row as u16, tint, dc, bmp };
    Some(dc)
}

/// Dessine `icon` en (`x`, `y`) sur `size` px. `tint` : couleur des glyphes
/// monochromes (0x00BBGGRR). Faux si l'icône manque : l'appelant dessine
/// alors ses primitives.
pub(super) unsafe fn draw(hdc: Handle, icon: Icon, x: i32, y: i32, size: i32, tint: u32) -> bool {
    if size < 8 {
        return false;
    }
    let Some(row) = pick(icon, size) else {
        return false;
    };
    let Some(src) = bitmap(row, tint) else {
        return false;
    };
    let s = ICON_TABLE[row].1 as i32;
    GdiAlphaBlend(hdc, x, y, size, size, src, 0, 0, s, s, 0x01FF_0000) != 0
}

/// Vérifications de la table (explorer_regression).
#[doc(hidden)]
pub(super) unsafe fn regression() {
    let mut ok = true;
    for (i, &(_, s, off, mask, _)) in ICON_TABLE.iter().enumerate() {
        let len = (s as usize) * (s as usize) * if mask { 1 } else { 4 };
        let next = ICON_TABLE.get(i + 1).map(|r| r.2 as usize).unwrap_or(ICON_BYTES);
        ok &= off as usize + len == next;
    }
    check!(ok, "Icones de l'Explorateur : table et donnees coherentes");
    check!(
        pick(Icon::Folder, 16).map(|r| ICON_TABLE[r].1) == Some(16)
            && pick(Icon::Folder, 40).map(|r| ICON_TABLE[r].1) == Some(48)
            && pick(Icon::Folder, 64).map(|r| ICON_TABLE[r].1) == Some(48)
            && pick_in(Icon::Folder, 24, true).map(|r| ICON_TABLE[r].4) == Some(true)
            && pick_in(Icon::Folder, 24, false).map(|r| ICON_TABLE[r].4) == Some(false)
            && pick_in(Icon::Text, 24, true).is_some(),
        "Icones de l'Explorateur : taille et variante du theme (clair, sombre)"
    );
    // Composition réelle sur un fond rouge : le dossier jaune apparaît, le
    // coin transparent garde le fond.
    let info = BitmapInfoHeader {
        size: 40,
        width: 32,
        height: -32,
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_pels_per_meter: 0,
        y_pels_per_meter: 0,
        colors_used: 0,
        colors_important: 0,
    };
    let dc = CreateCompatibleDC(core::ptr::null_mut());
    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let bmp = CreateDIBSection(dc, &info, 0, &mut bits, core::ptr::null_mut(), 0);
    if dc.is_null() || bmp.is_null() || bits.is_null() {
        check!(false, "Icones de l'Explorateur : bitmap de test");
        return;
    }
    let old = SelectObject(dc, bmp);
    let px = core::slice::from_raw_parts_mut(bits as *mut u32, 32 * 32);
    px.fill(0x00FF_0000);
    let drawn = draw(dc, Icon::Folder, 0, 0, 32, 0);
    let corner = px[0] & 0xFF_FFFF;
    let mid = px[20 * 32 + 16] & 0xFF_FFFF;
    let (r, g, b) = ((mid >> 16) & 0xFF, (mid >> 8) & 0xFF, mid & 0xFF);
    check!(drawn && corner == 0xFF_0000 && r > 200 && g > 150 && b < 120,
        "Icones de l'Explorateur : dossier jaune compose par AlphaBlend sur le fond");
    SelectObject(dc, old);
    DeleteObject(bmp);
    DeleteDC(dc);
}
