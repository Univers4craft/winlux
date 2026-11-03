//! Palettes logiques de GDI (CreatePalette, SelectPalette, Get/SetPaletteEntries…)
//! et GetFontResourceInfoW.
//!
//! L'écran de Winlux est toujours en couleurs vraies : une palette ne change
//! pas l'affichage, mais elle doit exister, garder ses couleurs et se laisser
//! choisir dans un DC. OpenTTD s'arrête sur « CreatePalette failed! » quand
//! CreatePalette renvoie NULL.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use super::{gdi32, gui, lock, Exports};

const DEFAULT_PALETTE: usize = 16; // GetStockObject(15) renvoie index + 1
const OBJ_PAL: u32 = 5;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const CLR_INVALID: u32 = 0xFFFF_FFFF;

/// Couleurs d'une palette : PALETTEENTRY (rouge, vert, bleu, drapeaux).
fn palettes() -> &'static Mutex<HashMap<usize, Vec<[u8; 4]>>> {
    static P: OnceLock<Mutex<HashMap<usize, Vec<[u8; 4]>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Palette choisie dans chaque DC.
fn selected() -> &'static Mutex<HashMap<usize, usize>> {
    static S: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Les 20 couleurs réservées du système (palette par défaut).
const SYSTEM_COLORS: [[u8; 3]; 20] = [
    [0, 0, 0],
    [128, 0, 0],
    [0, 128, 0],
    [128, 128, 0],
    [0, 0, 128],
    [128, 0, 128],
    [0, 128, 128],
    [192, 192, 192],
    [192, 220, 192],
    [166, 202, 240],
    [255, 251, 240],
    [160, 160, 164],
    [128, 128, 128],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [0, 0, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

fn entries_of(pal: usize) -> Option<Vec<[u8; 4]>> {
    if pal == DEFAULT_PALETTE {
        return Some(
            SYSTEM_COLORS
                .iter()
                .map(|c| [c[0], c[1], c[2], 0])
                .collect(),
        );
    }
    lock(palettes()).get(&pal).cloned()
}

pub fn is_palette(h: usize) -> bool {
    h == DEFAULT_PALETTE || lock(palettes()).contains_key(&h)
}

/// CreatePalette(LOGPALETTE*) : palVersion, palNumEntries, PALETTEENTRY[].
unsafe extern "win64" fn create_palette(log: *const u8) -> usize {
    if log.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let count = std::ptr::read_unaligned(log.add(2) as *const u16) as usize;
    if count == 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let entries: Vec<[u8; 4]> = (0..count)
        .map(|i| std::ptr::read_unaligned(log.add(4 + i * 4) as *const [u8; 4]))
        .collect();
    let h = gui::next_handle(0x0021_0000);
    lock(palettes()).insert(h, entries);
    h
}

unsafe extern "win64" fn create_halftone_palette(_hdc: usize) -> usize {
    // 6×6×6 niveaux, comme la palette demi-teinte d'un écran 256 couleurs.
    let mut entries = Vec::with_capacity(256);
    for r in 0..6u8 {
        for g in 0..6u8 {
            for b in 0..6u8 {
                entries.push([r * 51, g * 51, b * 51, 0]);
            }
        }
    }
    for c in SYSTEM_COLORS.iter().take(256 - entries.len()) {
        entries.push([c[0], c[1], c[2], 0]);
    }
    let h = gui::next_handle(0x0021_0000);
    lock(palettes()).insert(h, entries);
    h
}

unsafe extern "win64" fn get_palette_entries(pal: usize, start: u32, n: u32, out: *mut u8) -> u32 {
    let Some(entries) = entries_of(pal) else {
        return 0;
    };
    if out.is_null() {
        return entries.len() as u32;
    }
    let start = start as usize;
    let mut copied = 0;
    for (i, e) in entries.iter().skip(start).take(n as usize).enumerate() {
        std::ptr::write_unaligned(out.add(i * 4) as *mut [u8; 4], *e);
        copied += 1;
    }
    copied
}

unsafe fn write_entries(pal: usize, start: u32, n: u32, src: *const u8) -> u32 {
    if src.is_null() {
        return 0;
    }
    let mut map = lock(palettes());
    let Some(entries) = map.get_mut(&pal) else {
        return 0;
    };
    let start = start as usize;
    let mut done = 0;
    for i in 0..n as usize {
        let Some(slot) = entries.get_mut(start + i) else {
            break;
        };
        *slot = std::ptr::read_unaligned(src.add(i * 4) as *const [u8; 4]);
        done += 1;
    }
    done
}

unsafe extern "win64" fn set_palette_entries(
    pal: usize,
    start: u32,
    n: u32,
    src: *const u8,
) -> u32 {
    write_entries(pal, start, n, src)
}

unsafe extern "win64" fn animate_palette(pal: usize, start: u32, n: u32, src: *const u8) -> i32 {
    (write_entries(pal, start, n, src) > 0) as i32
}

unsafe extern "win64" fn resize_palette(pal: usize, n: u32) -> i32 {
    match lock(palettes()).get_mut(&pal) {
        Some(e) if n > 0 => {
            e.resize(n as usize, [0, 0, 0, 0]);
            1
        }
        _ => 0,
    }
}

unsafe extern "win64" fn get_nearest_palette_index(pal: usize, color: u32) -> u32 {
    let Some(entries) = entries_of(pal) else {
        return CLR_INVALID;
    };
    let (r, g, b) = (
        (color & 0xFF) as i32,
        ((color >> 8) & 0xFF) as i32,
        ((color >> 16) & 0xFF) as i32,
    );
    entries
        .iter()
        .enumerate()
        .min_by_key(|(_, e)| {
            let (dr, dg, db) = (e[0] as i32 - r, e[1] as i32 - g, e[2] as i32 - b);
            dr * dr + dg * dg + db * db
        })
        .map_or(CLR_INVALID, |(i, _)| i as u32)
}

unsafe extern "win64" fn select_palette(hdc: usize, pal: usize, _background: i32) -> usize {
    if hdc == 0 || !is_palette(pal) {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    lock(selected()).insert(hdc, pal).unwrap_or(DEFAULT_PALETTE)
}

/// Écran en couleurs vraies : aucune entrée à reporter dans la palette système.
unsafe extern "win64" fn realize_palette(_hdc: usize) -> u32 {
    0
}

unsafe extern "win64" fn get_system_palette_entries(
    _hdc: usize,
    start: u32,
    n: u32,
    out: *mut u8,
) -> u32 {
    // Comme Windows sur un écran sans palette : rien n'est renvoyé.
    let _ = (start, n, out);
    0
}

unsafe extern "win64" fn get_system_palette_use(_hdc: usize) -> u32 {
    1 // SYSPAL_STATIC
}

unsafe extern "win64" fn set_system_palette_use(_hdc: usize, _use: u32) -> u32 {
    1
}

unsafe extern "win64" fn delete_object(obj: usize) -> i32 {
    if obj == DEFAULT_PALETTE {
        return 1; // objet stock
    }
    if lock(palettes()).remove(&obj).is_some() {
        lock(selected()).retain(|_, &mut p| p != obj);
        return 1;
    }
    gdi32::delete_object(obj)
}

unsafe extern "win64" fn get_object_type(obj: usize) -> u32 {
    if is_palette(obj) {
        OBJ_PAL
    } else {
        gdi32::get_object_type(obj)
    }
}

unsafe fn get_object(obj: usize, size: i32, out: *mut c_void, wide: bool) -> i32 {
    if let Some(entries) = entries_of(obj).map(|e| e.len()) {
        // Une palette se décrit par son nombre d'entrées (WORD).
        if out.is_null() {
            return 2;
        }
        if size < 2 {
            return 0;
        }
        std::ptr::write_unaligned(out as *mut u16, entries as u16);
        return 2;
    }
    if wide {
        gdi32::get_object_w(obj, size, out)
    } else {
        gdi32::get_object_a(obj, size, out)
    }
}

unsafe extern "win64" fn get_object_a(obj: usize, size: i32, out: *mut c_void) -> i32 {
    get_object(obj, size, out, false)
}

unsafe extern "win64" fn get_object_w(obj: usize, size: i32, out: *mut c_void) -> i32 {
    get_object(obj, size, out, true)
}

/// GetFontResourceInfoW (non documentée, utilisée par OpenTTD après
/// AddFontResourceEx) : 0 = nombre de faces (DWORD), 1 = nom de la police.
/// Le nom renvoyé est la famille, celle que CreateFont retrouve ensuite.
unsafe extern "win64" fn get_font_resource_info_w(
    file: *const u16,
    size: *mut u32,
    buffer: *mut c_void,
    query: u32,
) -> i32 {
    if file.is_null() || size.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let path = super::gdi32_fonts::resolve_font_path(&super::wstr(file));
    let faces = super::gdi32_fonts::parse_font_file(&path);
    if faces.is_empty() {
        crate::process::set_last_error(2); // ERROR_FILE_NOT_FOUND
        return 0;
    }
    match query {
        0 => {
            if buffer.is_null() || *size < 4 {
                *size = 4;
                crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
                return buffer.is_null() as i32;
            }
            *(buffer as *mut u32) = faces.len() as u32;
            *size = 4;
            1
        }
        1 => {
            let mut names: Vec<String> = Vec::new();
            for f in &faces {
                if !names.contains(&f.family) {
                    names.push(f.family.clone());
                }
            }
            let text: Vec<u16> = names.join("|").encode_utf16().chain([0]).collect();
            let needed = (text.len() * 2) as u32;
            if buffer.is_null() {
                *size = needed;
                return 1;
            }
            if *size < needed {
                *size = needed;
                crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
                return 0;
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), buffer as *mut u16, text.len());
            *size = needed;
            1
        }
        _ => {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

/// Remplacent les versions vides de gdi32 (CreatePalette renvoyait NULL).
pub fn gdi32_overrides() -> Exports {
    crate::exports! {
        "CreatePalette" => fn create_palette,
        "CreateHalftonePalette" => fn create_halftone_palette,
        "GetPaletteEntries" => fn get_palette_entries,
        "SetPaletteEntries" => fn set_palette_entries,
        "AnimatePalette" => fn animate_palette,
        "ResizePalette" => fn resize_palette,
        "GetNearestPaletteIndex" => fn get_nearest_palette_index,
        "SelectPalette" => fn select_palette,
        "RealizePalette" => fn realize_palette,
        "GetSystemPaletteEntries" => fn get_system_palette_entries,
        "GetSystemPaletteUse" => fn get_system_palette_use,
        "SetSystemPaletteUse" => fn set_system_palette_use,
        "DeleteObject" => fn delete_object,
        "GetObjectType" => fn get_object_type,
        "GetObjectA" => fn get_object_a,
        "GetObjectW" => fn get_object_w,
        "GetFontResourceInfoW" => fn get_font_resource_info_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_creee_lue_modifiee_et_choisie() {
        let mut log = vec![0u8; 4 + 4 * 3];
        log[0..2].copy_from_slice(&0x300u16.to_le_bytes());
        log[2..4].copy_from_slice(&3u16.to_le_bytes());
        log[4..8].copy_from_slice(&[255, 0, 0, 0]);
        log[8..12].copy_from_slice(&[0, 255, 0, 0]);
        log[12..16].copy_from_slice(&[0, 0, 255, 0]);
        unsafe {
            let pal = create_palette(log.as_ptr());
            assert!(pal != 0, "CreatePalette");
            assert_eq!(get_object_type(pal), OBJ_PAL);
            let mut n = 0u16;
            assert_eq!(get_object_w(pal, 2, (&mut n as *mut u16).cast()), 2);
            assert_eq!(n, 3);
            assert_eq!(get_nearest_palette_index(pal, 0x00F0_1010), 2, "bleu");
            let green = [10u8, 200, 10, 0];
            assert_eq!(set_palette_entries(pal, 1, 1, green.as_ptr()), 1);
            let mut out = [0u8; 12];
            assert_eq!(get_palette_entries(pal, 0, 3, out.as_mut_ptr()), 3);
            assert_eq!(&out[4..8], &green);
            assert_eq!(get_palette_entries(pal, 0, 0, std::ptr::null_mut()), 3);
            let hdc = 0x7777_0001;
            assert_eq!(select_palette(hdc, pal, 0), DEFAULT_PALETTE);
            assert_eq!(select_palette(hdc, DEFAULT_PALETTE, 0), pal);
            assert_eq!(delete_object(pal), 1);
            assert_eq!(get_object_type(pal), 0);
            let half = create_halftone_palette(0);
            assert_eq!(get_palette_entries(half, 0, 0, std::ptr::null_mut()), 236);
            assert_eq!(delete_object(half), 1);
        }
    }
}
