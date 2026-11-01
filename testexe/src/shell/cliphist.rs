//! Historique du presse-papiers (Win+V) façon Windows 11.
//!
//! Le runtime partage le presse-papiers entre tous les programmes et numérote
//! chaque copie (`GetClipboardSequenceNumber`). Le bureau relève ce numéro
//! deux fois par seconde : chaque nouveau contenu (texte, image CF_DIB ou
//! fichiers CF_HDROP) entre en tête de l'historique. Le panneau permet de
//! rechercher, d'épingler (les épingles survivent au redémarrage), de
//! supprimer, de tout effacer, de régler la limite et de recoller un élément
//! d'un clic dans la fenêtre active.

use crate::gfx::*;
use crate::wm;
use core::ffi::c_void;
use winlux_testexe::*;

pub const CF_TEXT: u32 = 1;
pub const CF_DIB: u32 = 8;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_HDROP: u32 = 15;

const MAX_ITEMS: usize = 100;
const PREVIEW_LEN: usize = 480;
const THUMB_W: i32 = 304;
const THUMB_H: i32 = 96;
const PANEL_W: i32 = 360;
const ANIM_MS: i32 = 180;
const LIMITS: [usize; 4] = [10, 25, 50, 100];
const PIN_FILE: &core::ffi::CStr = c"C:\\.local\\share\\winlux\\clipboard-pinned.bin";
const PIN_MAGIC: &[u8; 8] = b"WLXPIN1\0";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Text,
    Image,
    Files,
}

#[derive(Clone, Copy)]
struct Item {
    kind: Kind,
    format: u32,
    /// Copie privée des données du presse-papiers (HGLOBAL).
    data: Handle,
    size: usize,
    hash: u32,
    pinned: bool,
    time: u32,
    preview: [u8; PREVIEW_LEN],
    preview_len: usize,
    thumb: Handle,
    thumb_w: i32,
    thumb_h: i32,
    img_w: i32,
    img_h: i32,
    files: u16,
}

const EMPTY: Item = Item {
    kind: Kind::Text,
    format: 0,
    data: core::ptr::null_mut(),
    size: 0,
    hash: 0,
    pinned: false,
    time: 0,
    preview: [0; PREVIEW_LEN],
    preview_len: 0,
    thumb: core::ptr::null_mut(),
    thumb_w: 0,
    thumb_h: 0,
    img_w: 0,
    img_h: 0,
    files: 0,
};

static mut ITEMS: [Item; MAX_ITEMS] = [EMPTY; MAX_ITEMS];
static mut COUNT: usize = 0;
static mut LIMIT: usize = 25;
static mut LAST_SEQ: u32 = 0;
static mut LOADED: bool = false;

pub static mut OPEN: bool = false;
static mut ANIM_START: u32 = 0;
static mut SCROLL: i32 = 0;
static mut HOVER: i32 = -1;
static mut HOVER_BTN: i32 = -1; // 0 épingler, 1 supprimer
static mut HOVER_HEADER: i32 = -1; // 0 tout effacer, 1 limite
static mut SELECTED: i32 = 0;
static mut SEARCH: [u8; 64] = [0; 64];
static mut SEARCH_LEN: usize = 0;
static mut MEM_DC: Handle = core::ptr::null_mut();

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn since(t: u32) -> i32 {
    now().wrapping_sub(t).min(1_000_000) as i32
}

/// Libellé localisé (aussi utilisé par snip.rs) : français et anglais
/// donnés à l'appel, espagnol et allemand lus dans `CLIP_TRANSLATIONS`
/// (l'anglais sert si une entrée manque).
pub fn tr(fr: &'static [u8], en: &'static [u8]) -> &'static [u8] {
    match unsafe { crate::locale::CURRENT_LANG } {
        crate::locale::Lang::Fr => fr,
        crate::locale::Lang::En => en,
        lang => CLIP_TRANSLATIONS
            .iter()
            .find(|r| r.0 == fr && r.1 == en)
            .map_or(en, |r| {
                if lang == crate::locale::Lang::Es {
                    r.2
                } else {
                    r.3
                }
            }),
    }
}

// ---------- Données ----------

pub fn fnv_hash(bytes: &[u8]) -> u32 {
    fnv(bytes)
}

fn fnv(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

pub unsafe fn global_copy(src: &[u8]) -> Handle {
    let h = GlobalAlloc(0x0002, src.len().max(1));
    if h.is_null() {
        return h;
    }
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        GlobalFree(h);
        return core::ptr::null_mut();
    }
    core::ptr::copy_nonoverlapping(src.as_ptr(), p, src.len());
    GlobalUnlock(h);
    h
}

unsafe fn with_data<R>(it: &Item, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    if it.data.is_null() {
        return None;
    }
    let p = GlobalLock(it.data) as *const u8;
    if p.is_null() {
        return None;
    }
    let r = f(core::slice::from_raw_parts(p, it.size));
    GlobalUnlock(it.data);
    Some(r)
}

/// Texte UTF-16 → Windows-1252, retours à la ligne gardés (\n).
fn wide_to_preview(src: &[u8], out: &mut [u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i + 1 < src.len() && n < out.len() {
        let u = u16::from_le_bytes([src[i], src[i + 1]]);
        i += 2;
        if u == 0 {
            break;
        }
        let c = match u {
            0x0D => continue,
            0x09 => b' ',
            0x0A => b'\n',
            0x20..=0x7E | 0xA0..=0xFF => u as u8,
            0x20AC => 0x80,
            0x2018 => 0x91,
            0x2019 => 0x92,
            0x201C => 0x93,
            0x201D => 0x94,
            0x2026 => 0x85,
            0x2013 => 0x96,
            0x2014 => 0x97,
            0x0152 => 0x8C,
            0x0153 => 0x9C,
            _ => b'?',
        };
        out[n] = c;
        n += 1;
    }
    n
}

fn ansi_to_preview(src: &[u8], out: &mut [u8]) -> usize {
    let mut n = 0;
    for &c in src {
        if c == 0 || n == out.len() {
            break;
        }
        match c {
            b'\r' => continue,
            b'\t' => out[n] = b' ',
            _ => out[n] = c,
        }
        n += 1;
    }
    n
}

/// Liste des chemins d'un CF_HDROP (DROPFILES + chaînes ANSI ou UTF-16).
/// Écrit les noms de fichiers séparés par « \n » et renvoie (longueur, nombre).
pub fn hdrop_names(src: &[u8], out: &mut [u8], full_paths: bool) -> (usize, u16) {
    if src.len() < 20 {
        return (0, 0);
    }
    let offset = u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as usize;
    let wide = u32::from_le_bytes([src[16], src[17], src[18], src[19]]) != 0;
    let mut i = offset;
    let mut n = 0;
    let mut count = 0u16;
    let mut path = [0u8; 300];
    loop {
        let mut pl = 0;
        if wide {
            while i + 1 < src.len() {
                let u = u16::from_le_bytes([src[i], src[i + 1]]);
                i += 2;
                if u == 0 {
                    break;
                }
                if pl < path.len() {
                    path[pl] = if u < 0x100 { u as u8 } else { b'?' };
                    pl += 1;
                }
            }
        } else {
            while i < src.len() {
                let c = src[i];
                i += 1;
                if c == 0 {
                    break;
                }
                if pl < path.len() {
                    path[pl] = c;
                    pl += 1;
                }
            }
        }
        if pl == 0 {
            break;
        }
        count += 1;
        let name = if full_paths {
            &path[..pl]
        } else {
            let start = path[..pl]
                .iter()
                .rposition(|&c| c == b'\\' || c == b'/')
                .map(|p| p + 1)
                .unwrap_or(0);
            &path[start..pl]
        };
        if n > 0 && n < out.len() {
            out[n] = b'\n';
            n += 1;
        }
        for &c in name {
            if n < out.len() {
                out[n] = c;
                n += 1;
            }
        }
        if i >= src.len() {
            break;
        }
    }
    (n, count)
}

/// Construit un CF_HDROP ANSI à partir de chemins Windows.
pub fn build_hdrop(paths: &[&[u8]], out: &mut [u8]) -> usize {
    if out.len() < 22 {
        return 0;
    }
    out[..20].fill(0);
    out[0] = 20; // pFiles
    let mut n = 20;
    for p in paths {
        if n + p.len() + 2 > out.len() {
            break;
        }
        out[n..n + p.len()].copy_from_slice(p);
        n += p.len();
        out[n] = 0;
        n += 1;
    }
    out[n] = 0;
    n + 1
}

/// Dimensions d'un CF_DIB 24/32 bits, et position des pixels.
fn dib_info(src: &[u8]) -> Option<(i32, i32, u16, usize, bool)> {
    if src.len() < 40 {
        return None;
    }
    let rd = |o: usize| u32::from_le_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]);
    let header = rd(0) as usize;
    let w = rd(4) as i32;
    let h = rd(8) as i32;
    let bpp = u16::from_le_bytes([src[14], src[15]]);
    let compression = rd(16);
    if w <= 0 || h == 0 || !(bpp == 24 || bpp == 32) || header < 40 {
        return None;
    }
    let mut offset = header;
    if compression == 3 && header == 40 {
        offset += 12;
    }
    let top_down = h < 0;
    let h = h.abs();
    let stride = ((w as usize * bpp as usize + 31) / 32) * 4;
    if src.len() < offset + stride * h as usize {
        return None;
    }
    Some((w, h, bpp, offset, top_down))
}

unsafe fn make_thumb(it: &mut Item, src: &[u8]) {
    let Some((w, h, bpp, offset, top_down)) = dib_info(src) else {
        return;
    };
    it.img_w = w;
    it.img_h = h;
    let (mut tw, mut th) = (THUMB_W, THUMB_W * h / w.max(1));
    if th > THUMB_H {
        th = THUMB_H;
        tw = (THUMB_H * w / h.max(1)).max(1);
    }
    let (tw, th) = (tw.clamp(1, THUMB_W), th.clamp(1, THUMB_H));
    let info = BitmapInfoHeader {
        size: 40,
        width: tw,
        height: -th,
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_pels_per_meter: 0,
        y_pels_per_meter: 0,
        colors_used: 0,
        colors_important: 0,
    };
    let mut bits: *mut c_void = core::ptr::null_mut();
    let screen = GetDC(core::ptr::null_mut());
    let bmp = CreateDIBSection(screen, &info, 0, &mut bits, core::ptr::null_mut(), 0);
    if bmp.is_null() || bits.is_null() {
        return;
    }
    let dst = bits as *mut u8;
    let bypp = (bpp / 8) as usize;
    let stride = ((w as usize * bpp as usize + 31) / 32) * 4;
    for y in 0..th {
        let sy = (y * h / th).min(h - 1) as usize;
        let row = if top_down { sy } else { h as usize - 1 - sy };
        for x in 0..tw {
            let sx = (x * w / tw).min(w - 1) as usize;
            let s = offset + row * stride + sx * bypp;
            let d = dst.add((y * tw + x) as usize * 4);
            *d = src[s];
            *d.add(1) = src[s + 1];
            *d.add(2) = src[s + 2];
            *d.add(3) = 0;
        }
    }
    it.thumb = bmp;
    it.thumb_w = tw;
    it.thumb_h = th;
}

unsafe fn free_item(it: &mut Item) {
    if !it.data.is_null() {
        GlobalFree(it.data);
    }
    if !it.thumb.is_null() {
        DeleteObject(it.thumb);
    }
    *it = EMPTY;
}

unsafe fn remove_at(i: usize) {
    if i >= COUNT {
        return;
    }
    free_item(&mut ITEMS[i]);
    for j in i..COUNT - 1 {
        ITEMS[j] = ITEMS[j + 1];
    }
    COUNT -= 1;
    ITEMS[COUNT] = EMPTY;
}

unsafe fn move_to_front(i: usize) {
    if i == 0 || i >= COUNT {
        return;
    }
    let it = ITEMS[i];
    let mut j = i;
    while j > 0 {
        ITEMS[j] = ITEMS[j - 1];
        j -= 1;
    }
    ITEMS[0] = it;
}

/// Retire les éléments non épinglés les plus anciens au-delà de la limite.
unsafe fn trim() {
    loop {
        let unpinned = ITEMS[..COUNT].iter().filter(|i| !i.pinned).count();
        if unpinned <= LIMIT {
            break;
        }
        match ITEMS[..COUNT].iter().rposition(|i| !i.pinned) {
            Some(i) => remove_at(i),
            None => break,
        }
    }
}

/// Ajoute un contenu en tête (ou remonte un doublon). Renvoie l'index 0 si ajouté.
pub unsafe fn add(kind: Kind, format: u32, src: &[u8], pinned: bool) -> bool {
    if src.is_empty() {
        return false;
    }
    let hash = fnv(src);
    if let Some(i) = ITEMS[..COUNT]
        .iter()
        .position(|it| it.hash == hash && it.kind == kind && it.size == src.len())
    {
        ITEMS[i].time = now();
        ITEMS[i].pinned |= pinned;
        move_to_front(i);
        return true;
    }
    if COUNT == MAX_ITEMS {
        match ITEMS[..COUNT].iter().rposition(|i| !i.pinned) {
            Some(i) => remove_at(i),
            None => return false,
        }
    }
    let mut it = EMPTY;
    it.kind = kind;
    it.format = format;
    it.hash = hash;
    it.pinned = pinned;
    it.time = now();
    it.data = global_copy(src);
    if it.data.is_null() {
        return false;
    }
    it.size = src.len();
    match kind {
        Kind::Text => {
            it.preview_len = if format == CF_UNICODETEXT {
                wide_to_preview(src, &mut it.preview)
            } else {
                ansi_to_preview(src, &mut it.preview)
            };
            if it.preview[..it.preview_len]
                .iter()
                .all(|&c| c == b' ' || c == b'\n')
            {
                GlobalFree(it.data);
                return false;
            }
        }
        Kind::Image => {
            make_thumb(&mut it, src);
            if it.img_w == 0 {
                GlobalFree(it.data);
                return false;
            }
        }
        Kind::Files => {
            let (n, c) = hdrop_names(src, &mut it.preview, false);
            it.preview_len = n;
            it.files = c;
            if c == 0 {
                GlobalFree(it.data);
                return false;
            }
        }
    }
    let mut j = COUNT;
    while j > 0 {
        ITEMS[j] = ITEMS[j - 1];
        j -= 1;
    }
    ITEMS[0] = it;
    COUNT += 1;
    trim();
    true
}

/// Lit le presse-papiers courant et l'ajoute à l'historique.
unsafe fn capture_current() -> bool {
    if OpenClipboard(core::ptr::null_mut()) == 0 {
        return false;
    }
    let mut added = false;
    for (format, kind) in [
        (CF_HDROP, Kind::Files),
        (CF_DIB, Kind::Image),
        (CF_UNICODETEXT, Kind::Text),
        (CF_TEXT, Kind::Text),
    ] {
        if IsClipboardFormatAvailable(format) == 0 {
            continue;
        }
        let h = GetClipboardData(format) as Handle;
        if h.is_null() {
            continue;
        }
        let size = GlobalSize(h);
        let p = GlobalLock(h) as *const u8;
        if !p.is_null() && size > 0 {
            let data = core::slice::from_raw_parts(p, size);
            added = add(kind, format, data, false);
            GlobalUnlock(h);
        }
        if added {
            break;
        }
    }
    CloseClipboard();
    added
}

/// Relève le presse-papiers ; vrai si l'historique a changé.
pub unsafe fn poll() -> bool {
    if !LOADED {
        LOADED = true;
        load_pins();
    }
    let seq = GetClipboardSequenceNumber();
    if seq == LAST_SEQ {
        return false;
    }
    LAST_SEQ = seq;
    seq != 0 && capture_current()
}

/// Remet un élément dans le presse-papiers (et en tête de l'historique).
pub unsafe fn restore(i: usize) -> bool {
    if i >= COUNT {
        return false;
    }
    let it = ITEMS[i];
    let Some(h) = with_data(&it, |d| global_copy(d)) else {
        return false;
    };
    if h.is_null() || OpenClipboard(core::ptr::null_mut()) == 0 {
        return false;
    }
    EmptyClipboard();
    let ok = SetClipboardData(it.format, h as usize) != 0;
    CloseClipboard();
    // Notre propre écriture ne doit pas créer de doublon.
    LAST_SEQ = GetClipboardSequenceNumber();
    ITEMS[i].time = now();
    move_to_front(i);
    ok
}

/// Copie un texte Windows-1252 dans le presse-papiers (CF_TEXT).
pub unsafe fn set_text(text: &[u8]) -> bool {
    let h = GlobalAlloc(0x0002, text.len() + 1);
    if h.is_null() {
        return false;
    }
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        return false;
    }
    core::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
    *p.add(text.len()) = 0;
    GlobalUnlock(h);
    if OpenClipboard(core::ptr::null_mut()) == 0 {
        return false;
    }
    EmptyClipboard();
    let ok = SetClipboardData(CF_TEXT, h as usize) != 0;
    CloseClipboard();
    ok
}

/// Lit le texte du presse-papiers en Windows-1252 (CF_TEXT synthétisé).
pub unsafe fn get_text(out: &mut [u8]) -> usize {
    if OpenClipboard(core::ptr::null_mut()) == 0 {
        return 0;
    }
    let mut n = 0;
    let h = GetClipboardData(CF_TEXT) as Handle;
    if !h.is_null() {
        let size = GlobalSize(h);
        let p = GlobalLock(h) as *const u8;
        if !p.is_null() {
            while n < size && n < out.len() && *p.add(n) != 0 {
                out[n] = *p.add(n);
                n += 1;
            }
            GlobalUnlock(h);
        }
    }
    CloseClipboard();
    n
}

/// Format enregistré « Preferred DropEffect » : 2 = couper (déplacer),
/// 5 = copier. Le pont presse-papiers le traduit en « cut »/« copy » pour
/// Thunar et le bureau, et inversement.
unsafe fn drop_effect_format() -> u32 {
    RegisterClipboardFormatA(c"Preferred DropEffect".as_ptr())
}

/// Publie une liste de fichiers (CF_HDROP) pour l'historique et les autres
/// programmes ; `cut` marque un déplacement (Couper).
pub unsafe fn set_files(paths: &[&[u8]], cut: bool) -> bool {
    let mut buf = [0u8; 8192];
    let n = build_hdrop(paths, &mut buf);
    if n == 0 {
        return false;
    }
    let h = global_copy(&buf[..n]);
    if h.is_null() || OpenClipboard(core::ptr::null_mut()) == 0 {
        return false;
    }
    EmptyClipboard();
    let ok = SetClipboardData(CF_HDROP, h as usize) != 0;
    let effect: u32 = if cut { 2 } else { 5 };
    let format = drop_effect_format();
    let e = global_copy(&effect.to_le_bytes());
    if ok && format != 0 && !e.is_null() {
        SetClipboardData(format, e as usize);
    }
    CloseClipboard();
    ok
}

/// Vrai si les fichiers du presse-papiers ont été coupés (déplacement).
pub unsafe fn files_cut() -> bool {
    let format = drop_effect_format();
    if format == 0 || OpenClipboard(core::ptr::null_mut()) == 0 {
        return false;
    }
    let mut effect = 0u32;
    let h = GetClipboardData(format) as Handle;
    if !h.is_null() && GlobalSize(h) >= 4 {
        let p = GlobalLock(h) as *const u8;
        if !p.is_null() {
            effect = u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]);
            GlobalUnlock(h);
        }
    }
    CloseClipboard();
    effect & 2 != 0 && effect & 1 == 0
}

/// Chemins complets du CF_HDROP courant, séparés par « \n ».
pub unsafe fn get_files(out: &mut [u8]) -> usize {
    if OpenClipboard(core::ptr::null_mut()) == 0 {
        return 0;
    }
    let mut n = 0;
    let h = GetClipboardData(CF_HDROP) as Handle;
    if !h.is_null() {
        let size = GlobalSize(h);
        let p = GlobalLock(h) as *const u8;
        if !p.is_null() {
            n = hdrop_names(core::slice::from_raw_parts(p, size), out, true).0;
            GlobalUnlock(h);
        }
    }
    CloseClipboard();
    n
}

pub unsafe fn toggle_pin(i: usize) {
    if i < COUNT {
        ITEMS[i].pinned = !ITEMS[i].pinned;
        save_pins();
    }
}

pub unsafe fn delete(i: usize) {
    let pinned = i < COUNT && ITEMS[i].pinned;
    remove_at(i);
    if pinned {
        save_pins();
    }
}

/// « Tout effacer » : comme Windows, les éléments épinglés restent.
pub unsafe fn clear_unpinned() {
    let mut i = 0;
    while i < COUNT {
        if ITEMS[i].pinned {
            i += 1;
        } else {
            remove_at(i);
        }
    }
    SCROLL = 0;
    SELECTED = 0;
}

pub unsafe fn cycle_limit() {
    let i = LIMITS.iter().position(|&l| l == LIMIT).unwrap_or(1);
    LIMIT = LIMITS[(i + 1) % LIMITS.len()];
    trim();
    save_pins();
}

pub fn count() -> usize {
    unsafe { COUNT }
}
pub fn limit() -> usize {
    unsafe { LIMIT }
}
pub fn is_pinned(i: usize) -> bool {
    unsafe { i < COUNT && ITEMS[i].pinned }
}
pub fn kind(i: usize) -> Option<Kind> {
    unsafe { (i < COUNT).then(|| ITEMS[i].kind) }
}
pub fn preview(i: usize) -> &'static [u8] {
    unsafe {
        if i < COUNT {
            &ITEMS[i].preview[..ITEMS[i].preview_len]
        } else {
            b""
        }
    }
}

// ---------- Épingles persistantes ----------

unsafe fn ensure_dirs() {
    for d in [
        c"C:\\.local".as_ptr(),
        c"C:\\.local\\share".as_ptr(),
        c"C:\\.local\\share\\winlux".as_ptr(),
    ] {
        CreateDirectoryA(d, core::ptr::null_mut());
    }
}

unsafe fn write_all(h: Handle, bytes: &[u8]) -> bool {
    let mut written = 0u32;
    WriteFile(
        h,
        bytes.as_ptr(),
        bytes.len() as u32,
        &mut written,
        core::ptr::null_mut(),
    ) != 0
        && written as usize == bytes.len()
}

unsafe fn save_pins() {
    ensure_dirs();
    let h = CreateFileA(
        PIN_FILE.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0x80,
        core::ptr::null_mut(),
    );
    if h as isize == -1 || h.is_null() {
        return;
    }
    let pinned = ITEMS[..COUNT].iter().filter(|i| i.pinned).count() as u32;
    let mut ok = write_all(h, PIN_MAGIC)
        && write_all(h, &(LIMIT as u32).to_le_bytes())
        && write_all(h, &pinned.to_le_bytes());
    // Ordre inverse : à la relecture, chaque ajout passe en tête.
    for i in (0..COUNT).rev() {
        let it = ITEMS[i];
        if !ok || !it.pinned {
            continue;
        }
        ok = write_all(h, &it.format.to_le_bytes())
            && write_all(h, &(it.size as u32).to_le_bytes())
            && with_data(&it, |d| write_all(h, d)).unwrap_or(false);
    }
    CloseHandle(h);
}

unsafe fn read_exact(h: Handle, out: &mut [u8]) -> bool {
    let mut read = 0u32;
    ReadFile(
        h,
        out.as_mut_ptr(),
        out.len() as u32,
        &mut read,
        core::ptr::null_mut(),
    ) != 0
        && read as usize == out.len()
}

unsafe fn load_pins() {
    let h = CreateFileA(
        PIN_FILE.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0x80,
        core::ptr::null_mut(),
    );
    if h as isize == -1 || h.is_null() {
        return;
    }
    let mut head = [0u8; 16];
    if read_exact(h, &mut head) && &head[..8] == PIN_MAGIC {
        let limit = u32::from_le_bytes([head[8], head[9], head[10], head[11]]) as usize;
        if LIMITS.contains(&limit) {
            LIMIT = limit;
        }
        let count =
            u32::from_le_bytes([head[12], head[13], head[14], head[15]]).min(MAX_ITEMS as u32);
        let heap = GetProcessHeap();
        for _ in 0..count {
            let mut rec = [0u8; 8];
            if !read_exact(h, &mut rec) {
                break;
            }
            let format = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
            let size = u32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]) as usize;
            if size == 0 || size > 64 * 1024 * 1024 {
                break;
            }
            let p = HeapAlloc(heap, 0, size) as *mut u8;
            if p.is_null() {
                break;
            }
            let buf = core::slice::from_raw_parts_mut(p, size);
            if read_exact(h, buf) {
                let kind = match format {
                    CF_DIB => Kind::Image,
                    CF_HDROP => Kind::Files,
                    _ => Kind::Text,
                };
                add(kind, format, buf, true);
            }
            HeapFree(heap, 0, p as *mut c_void);
        }
    }
    CloseHandle(h);
}

// ---------- Panneau ----------

pub unsafe fn set_open(open: bool) {
    if open == OPEN {
        return;
    }
    OPEN = open;
    if open {
        poll();
        ANIM_START = now();
        SCROLL = 0;
        SELECTED = 0;
        SEARCH_LEN = 0;
        HOVER = -1;
    }
}

pub unsafe fn toggle() {
    set_open(!OPEN);
}

pub fn is_open() -> bool {
    unsafe { OPEN }
}

pub fn animating() -> bool {
    unsafe { OPEN && since(ANIM_START) < ANIM_MS + 20 }
}

struct Layout {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    list_top: i32,
    list_bottom: i32,
}

fn layout(screen_w: i32, bar_top: i32) -> Layout {
    let w = PANEL_W.min(screen_w - 24);
    let h = 540.min(bar_top - 24).max(200);
    let x = screen_w - w - 12;
    let y = bar_top - h - 12;
    Layout {
        x,
        y,
        w,
        h,
        list_top: y + 92,
        list_bottom: y + h - 40,
    }
}

fn lower(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' => c + 32,
        0xC0..=0xDE if c != 0xD7 => c + 32,
        _ => c,
    }
}

/// Recherche sans casse dans l'aperçu (texte ou noms de fichiers).
pub fn matches(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    (0..=hay.len() - needle.len()).any(|i| {
        hay[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(&a, &b)| lower(a) == lower(b))
    })
}

unsafe fn visible(out: &mut [usize; MAX_ITEMS]) -> usize {
    let needle = &SEARCH[..SEARCH_LEN];
    let mut n = 0;
    for i in 0..COUNT {
        let it = &ITEMS[i];
        let hay: &[u8] = match it.kind {
            Kind::Image => tr(b"image capture", b"image picture"),
            _ => &it.preview[..it.preview_len],
        };
        if matches(hay, needle) {
            out[n] = i;
            n += 1;
        }
    }
    n
}

fn card_h(k: Kind) -> i32 {
    match k {
        Kind::Image => THUMB_H + 40,
        Kind::Text => 84,
        Kind::Files => 64,
    }
}

fn ago(t: u32, out: &mut [u8; 24]) -> usize {
    let s = since(t) / 1000;
    let mut n = 0;
    let mut push = |b: &[u8], out: &mut [u8; 24]| {
        for &c in b {
            if n < out.len() {
                out[n] = c;
                n += 1;
            }
        }
    };
    let num = |v: i32, buf: &mut [u8; 6]| -> usize {
        let mut tmp = [0u8; 6];
        let mut l = 0;
        let mut v = v.max(0);
        loop {
            tmp[l] = b'0' + (v % 10) as u8;
            l += 1;
            v /= 10;
            if v == 0 || l == 6 {
                break;
            }
        }
        for i in 0..l {
            buf[i] = tmp[l - 1 - i];
        }
        l
    };
    let mut nb = [0u8; 6];
    if s < 60 {
        push(tr(b"\xC0 l'instant", b"Just now"), out);
    } else if s < 3600 {
        let l = num(s / 60, &mut nb);
        push(tr(b"Il y a ", b""), out);
        push(&nb[..l], out);
        push(tr(b" min", b" min ago"), out);
    } else {
        let l = num(s / 3600, &mut nb);
        push(tr(b"Il y a ", b""), out);
        push(&nb[..l], out);
        push(tr(b" h", b" h ago"), out);
    }
    n
}

unsafe fn draw_pin(hdc: Handle, b: &mut Brushes, x: i32, y: i32, color: u32, filled: bool) {
    // Punaise : tête ronde, corps et pointe.
    let c = color;
    if filled {
        ellipse(hdc, b, x + 6, y + 2, x + 18, y + 12, c);
    } else {
        ellipse(hdc, b, x + 6, y + 2, x + 18, y + 12, c);
        ellipse(hdc, b, x + 9, y + 5, x + 15, y + 9, glyph_bg());
    }
    fill(hdc, b, x + 4, y + 11, x + 20, y + 13, c);
    fill(hdc, b, x + 11, y + 13, x + 13, y + 21, c);
}

unsafe fn draw_x(hdc: Handle, b: &mut Brushes, x: i32, y: i32, color: u32) {
    line(hdc, b, x + 7, y + 7, x + 17, y + 17, 2, color);
    line(hdc, b, x + 17, y + 7, x + 7, y + 17, 2, color);
}

fn pin_rect(card: &Rect) -> Rect {
    rect(
        card.right - 60,
        card.top + 6,
        card.right - 34,
        card.top + 30,
    )
}
fn del_rect(card: &Rect) -> Rect {
    rect(card.right - 32, card.top + 6, card.right - 6, card.top + 30)
}
fn clear_rect(l: &Layout) -> Rect {
    let w = text_width(tr(b"Tout effacer", b"Clear all"), 12) + 20;
    rect(l.x + l.w - 16 - w, l.y + 12, l.x + l.w - 16, l.y + 38)
}
fn limit_rect(l: &Layout) -> Rect {
    rect(l.x + 12, l.list_bottom + 8, l.x + 220, l.list_bottom + 32)
}

pub unsafe fn draw(hdc: Handle, b: &mut Brushes, f: &Fonts, screen_w: i32, bar_top: i32) {
    if !OPEN {
        return;
    }
    let p = pal();
    let l0 = layout(screen_w, bar_top);
    let t = since(ANIM_START);
    let dy = if t < ANIM_MS {
        (1000 - ease_out(t, ANIM_MS)) * 28 / 1000
    } else {
        0
    };
    let l = Layout {
        y: l0.y + dy,
        list_top: l0.list_top + dy,
        list_bottom: l0.list_bottom + dy,
        ..l0
    };
    panel(hdc, b, l.x, l.y, l.x + l.w, l.y + l.h, 10, p.mica, p.border);
    set_glyph_bg(Some(p.mica));

    // En-tête : titre et « Tout effacer ».
    text(
        hdc,
        f.semibold,
        p.text,
        l.x + 18,
        l.y + 16,
        tr(b"Presse-papiers", b"Clipboard"),
    );
    if COUNT > 0 {
        let r = clear_rect(&l);
        let bg = if HOVER_HEADER == 0 {
            p.hover
        } else {
            p.surface
        };
        round_rect(hdc, b, r.left, r.top, r.right, r.bottom, 5, bg);
        text_centered(
            hdc,
            f.caption,
            12,
            p.text,
            (r.left + r.right) / 2,
            r.top + 6,
            tr(b"Tout effacer", b"Clear all"),
        );
    }

    // Recherche.
    let sx = l.x + 14;
    let sy = l.y + 48;
    round_rect(hdc, b, sx, sy, l.x + l.w - 14, sy + 32, 6, p.border);
    round_rect(
        hdc,
        b,
        sx + 1,
        sy + 1,
        l.x + l.w - 15,
        sy + 30,
        5,
        p.surface,
    );
    fill(hdc, b, sx + 8, sy + 29, l.x + l.w - 22, sy + 31, p.accent);
    glyph(hdc, b, f, Glyph::Search, sx + 8, sy + 8, 16);
    if SEARCH_LEN == 0 {
        text(
            hdc,
            f.body,
            p.text3,
            sx + 32,
            sy + 8,
            tr(b"Rechercher dans l'historique", b"Search history"),
        );
    } else {
        text_fit(
            hdc,
            f.body,
            13,
            p.text,
            sx + 32,
            sy + 8,
            &SEARCH[..SEARCH_LEN],
            l.w - 70,
        );
    }
    let caret = sx + 32 + text_width(&SEARCH[..SEARCH_LEN], 13).min(l.w - 70);
    if (now() / 530) % 2 == 0 {
        fill(hdc, b, caret, sy + 8, caret + 1, sy + 24, p.text);
    }

    // Liste.
    let mut vis = [0usize; MAX_ITEMS];
    let n = visible(&mut vis);
    if n == 0 {
        let (t1, t2): (&[u8], &[u8]) = if COUNT == 0 {
            (
                tr(
                    b"Vous n'avez encore rien copi\xE9",
                    b"You haven't copied anything yet",
                ),
                tr(
                    b"Copiez du texte, une image ou des fichiers : ils appara\xEEtront ici.",
                    b"Copy text, an image or files and they will show up here.",
                ),
            )
        } else {
            (
                tr(b"Aucun r\xE9sultat", b"No results"),
                tr(b"Essayez un autre mot.", b"Try another word."),
            )
        };
        text_centered(
            hdc,
            f.semibold,
            13,
            p.text,
            l.x + l.w / 2,
            l.list_top + 60,
            t1,
        );
        text_fit(
            hdc,
            f.small,
            11,
            p.text3,
            l.x + 20,
            l.list_top + 86,
            t2,
            l.w - 40,
        );
    }
    let total: i32 = vis[..n].iter().map(|&i| card_h(ITEMS[i].kind) + 8).sum();
    let max_scroll = (total - (l.list_bottom - l.list_top)).max(0);
    SCROLL = SCROLL.clamp(0, max_scroll);
    SELECTED = SELECTED.clamp(0, (n as i32 - 1).max(0));
    let mut y = l.list_top - SCROLL;
    for (vi, &i) in vis[..n].iter().enumerate() {
        let it = ITEMS[i];
        let h = card_h(it.kind);
        let card = rect(l.x + 12, y, l.x + l.w - 12, y + h);
        y += h + 8;
        if card.bottom <= l.list_top || card.top >= l.list_bottom {
            continue;
        }
        // Pas de découpage GDI : une carte partiellement visible est omise
        // en bas, et seule la partie visible du haut est dessinée.
        if card.bottom > l.list_bottom {
            continue;
        }
        let hovered = HOVER == vi as i32;
        let selected = SELECTED == vi as i32;
        let bg = if hovered { p.hover } else { p.surface };
        if selected {
            round_rect(
                hdc,
                b,
                card.left - 2,
                card.top.max(l.list_top) - 2,
                card.right + 2,
                card.bottom + 2,
                8,
                p.accent,
            );
        }
        round_rect(
            hdc,
            b,
            card.left,
            card.top.max(l.list_top),
            card.right,
            card.bottom,
            6,
            bg,
        );
        set_glyph_bg(Some(bg));
        let inner = card.top + 10;
        if card.top >= l.list_top {
            match it.kind {
                Kind::Text => {
                    let pv = &it.preview[..it.preview_len];
                    let mut lines = pv.split(|&c| c == b'\n').filter(|s| !s.is_empty());
                    let mut ly = inner;
                    for _ in 0..3 {
                        if let Some(line) = lines.next() {
                            text_fit(
                                hdc,
                                f.body,
                                13,
                                p.text,
                                card.left + 12,
                                ly,
                                line,
                                card.right - card.left - 80,
                            );
                            ly += 17;
                        }
                    }
                }
                Kind::Image => {
                    if MEM_DC.is_null() {
                        MEM_DC = CreateCompatibleDC(core::ptr::null_mut());
                    }
                    if !it.thumb.is_null() {
                        let old = SelectObject(MEM_DC, it.thumb);
                        BitBlt(
                            hdc,
                            card.left + 12,
                            inner,
                            it.thumb_w,
                            it.thumb_h,
                            MEM_DC,
                            0,
                            0,
                            0x00CC_0020,
                        );
                        SelectObject(MEM_DC, old);
                    }
                }
                Kind::Files => {
                    glyph(hdc, b, f, Glyph::Folder, card.left + 12, inner, 20);
                    let first = it.preview[..it.preview_len]
                        .split(|&c| c == b'\n')
                        .next()
                        .unwrap_or(b"");
                    text_fit(
                        hdc,
                        f.body,
                        13,
                        p.text,
                        card.left + 40,
                        inner + 2,
                        first,
                        card.right - card.left - 110,
                    );
                    if it.files > 1 {
                        let mut buf = [0u8; 40];
                        let mut k = 0;
                        for &c in b"+ " {
                            buf[k] = c;
                            k += 1;
                        }
                        let mut v = it.files - 1;
                        let mut tmp = [0u8; 5];
                        let mut tl = 0;
                        loop {
                            tmp[tl] = b'0' + (v % 10) as u8;
                            tl += 1;
                            v /= 10;
                            if v == 0 {
                                break;
                            }
                        }
                        for j in 0..tl {
                            buf[k] = tmp[tl - 1 - j];
                            k += 1;
                        }
                        for &c in tr(b" autre(s)", b" more") {
                            buf[k] = c;
                            k += 1;
                        }
                        text(hdc, f.small, p.text2, card.left + 40, inner + 20, &buf[..k]);
                    }
                }
            }
            // Pied de carte : type et ancienneté.
            let mut ts = [0u8; 24];
            let tl = ago(it.time, &mut ts);
            let label: &[u8] = match it.kind {
                Kind::Text => tr(b"Texte", b"Text"),
                Kind::Image => tr(b"Image", b"Image"),
                Kind::Files => tr(b"Fichiers", b"Files"),
            };
            text(
                hdc,
                f.small,
                p.text3,
                card.left + 12,
                card.bottom - 18,
                label,
            );
            text(
                hdc,
                f.small,
                p.text3,
                card.left + 12 + text_width(label, 11) + 10,
                card.bottom - 18,
                &ts[..tl],
            );
            // Boutons épingler / supprimer.
            if hovered || it.pinned || selected {
                let pr = pin_rect(&card);
                if hovered && HOVER_BTN == 0 {
                    round_rect(hdc, b, pr.left, pr.top, pr.right, pr.bottom, 4, p.pressed);
                }
                let pc = if it.pinned { p.accent } else { p.text2 };
                draw_pin(hdc, b, pr.left + 1, pr.top + 1, pc, it.pinned);
            }
            if hovered || selected {
                let dr = del_rect(&card);
                if hovered && HOVER_BTN == 1 {
                    round_rect(hdc, b, dr.left, dr.top, dr.right, dr.bottom, 4, p.pressed);
                }
                draw_x(hdc, b, dr.left + 1, dr.top, p.text2);
            }
        }
    }
    // Barre de défilement.
    if max_scroll > 0 {
        let track = l.list_bottom - l.list_top;
        let thumb = (track * track / (total.max(1))).max(24);
        let ty = l.list_top + (track - thumb) * SCROLL / max_scroll.max(1);
        round_rect(
            hdc,
            b,
            l.x + l.w - 7,
            ty,
            l.x + l.w - 4,
            ty + thumb,
            2,
            p.text3,
        );
    }
    set_glyph_bg(None);

    // Pied : limite réglable.
    fill(
        hdc,
        b,
        l.x + 12,
        l.list_bottom + 2,
        l.x + l.w - 12,
        l.list_bottom + 3,
        p.border,
    );
    let lr = limit_rect(&l);
    if HOVER_HEADER == 1 {
        round_rect(hdc, b, lr.left, lr.top, lr.right, lr.bottom, 4, p.hover);
    }
    let mut lb = [0u8; 48];
    let mut k = 0;
    for &c in tr(b"Limite : ", b"Limit: ") {
        lb[k] = c;
        k += 1;
    }
    let mut v = LIMIT;
    let mut tmp = [0u8; 4];
    let mut tl = 0;
    loop {
        tmp[tl] = b'0' + (v % 10) as u8;
        tl += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for j in 0..tl {
        lb[k] = tmp[tl - 1 - j];
        k += 1;
    }
    for &c in tr(b" \xE9l\xE9ments  \x95  changer", b" items  \x95  change") {
        lb[k] = c;
        k += 1;
    }
    text(hdc, f.small, p.text2, lr.left + 6, lr.top + 6, &lb[..k]);
    text(
        hdc,
        f.small,
        p.text3,
        l.x + l.w - 18 - text_width(b"Win+V", 11),
        lr.top + 6,
        b"Win+V",
    );
}

/// Index de carte (dans la liste filtrée) et zone survolée.
unsafe fn hit(l: &Layout, x: i32, y: i32) -> (i32, i32) {
    if y < l.list_top || y >= l.list_bottom || x < l.x || x >= l.x + l.w {
        return (-1, -1);
    }
    let mut vis = [0usize; MAX_ITEMS];
    let n = visible(&mut vis);
    let mut cy = l.list_top - SCROLL;
    for (vi, &i) in vis[..n].iter().enumerate() {
        let h = card_h(ITEMS[i].kind);
        let card = rect(l.x + 12, cy, l.x + l.w - 12, cy + h);
        cy += h + 8;
        if card.bottom > l.list_bottom {
            break;
        }
        if inside(&card, x, y) {
            let btn = if inside(&pin_rect(&card), x, y) {
                0
            } else if inside(&del_rect(&card), x, y) {
                1
            } else {
                -1
            };
            return (vi as i32, btn);
        }
    }
    (-1, -1)
}

unsafe fn item_of(vi: i32) -> Option<usize> {
    let mut vis = [0usize; MAX_ITEMS];
    let n = visible(&mut vis);
    (vi >= 0 && (vi as usize) < n).then(|| vis[vi as usize])
}

/// Recolle l'élément : presse-papiers, fermeture du panneau, collage dans
/// la fenêtre active du bureau.
unsafe fn paste_item(i: usize) {
    let kind = ITEMS[i].kind;
    if !restore(i) {
        return;
    }
    set_open(false);
    match wm::active_app() {
        Some(wm::APP_TERMINAL) => {
            wm::on_key(0, 22);
        }
        Some(wm::APP_NOTEPAD) if kind == Kind::Text => crate::notepad::paste_from_clipboard(),
        Some(wm::APP_EXPLORER) if kind == Kind::Files => crate::explorer::paste_from_clipboard(),
        _ => {}
    }
}

pub unsafe fn on_click(screen_w: i32, bar_top: i32, x: i32, y: i32) -> bool {
    if !OPEN {
        return false;
    }
    let l = layout(screen_w, bar_top);
    if x < l.x || x >= l.x + l.w || y < l.y || y >= l.y + l.h {
        set_open(false);
        return false;
    }
    if COUNT > 0 && inside(&clear_rect(&l), x, y) {
        clear_unpinned();
        return true;
    }
    if inside(&limit_rect(&l), x, y) {
        cycle_limit();
        return true;
    }
    let (vi, btn) = hit(&l, x, y);
    if let Some(i) = item_of(vi) {
        match btn {
            0 => toggle_pin(i),
            1 => delete(i),
            _ => paste_item(i),
        }
    }
    true
}

pub unsafe fn on_mouse_move(screen_w: i32, bar_top: i32, x: i32, y: i32) -> bool {
    if !OPEN {
        return false;
    }
    let l = layout(screen_w, bar_top);
    let (vi, btn) = hit(&l, x, y);
    let hh = if COUNT > 0 && inside(&clear_rect(&l), x, y) {
        0
    } else if inside(&limit_rect(&l), x, y) {
        1
    } else {
        -1
    };
    let changed = vi != HOVER || btn != HOVER_BTN || hh != HOVER_HEADER;
    HOVER = vi;
    HOVER_BTN = btn;
    HOVER_HEADER = hh;
    if vi >= 0 {
        SELECTED = vi;
    }
    changed
}

pub unsafe fn on_wheel(screen_w: i32, bar_top: i32, x: i32, y: i32, delta: i32) -> bool {
    if !OPEN {
        return false;
    }
    let l = layout(screen_w, bar_top);
    if x < l.x || x >= l.x + l.w || y < l.y || y >= l.y + l.h {
        return false;
    }
    SCROLL = (SCROLL - delta / 120 * 60).max(0);
    true
}

/// Clavier quand le panneau est ouvert (il garde le focus comme sous Windows).
pub unsafe fn on_key(vk: u32, ch: u16) -> bool {
    if !OPEN {
        return false;
    }
    let mut vis = [0usize; MAX_ITEMS];
    let n = visible(&mut vis) as i32;
    if ch == 0 {
        match vk {
            0x1B => set_open(false),
            0x26 => SELECTED = (SELECTED - 1).max(0),
            0x28 => SELECTED = (SELECTED + 1).min((n - 1).max(0)),
            0x2E => {
                if let Some(i) = item_of(SELECTED) {
                    delete(i);
                }
            }
            _ => return true,
        }
        return true;
    }
    match ch {
        27 => set_open(false),
        13 => {
            if let Some(i) = item_of(SELECTED) {
                paste_item(i);
            }
        }
        8 => {
            if SEARCH_LEN > 0 {
                SEARCH_LEN -= 1;
            }
            SELECTED = 0;
            SCROLL = 0;
        }
        // Ctrl+P : épingler l'élément sélectionné.
        16 => {
            if let Some(i) = item_of(SELECTED) {
                toggle_pin(i);
            }
        }
        0x20..=0xFF if SEARCH_LEN < SEARCH.len() => {
            SEARCH[SEARCH_LEN] = ch as u8;
            SEARCH_LEN += 1;
            SELECTED = 0;
            SCROLL = 0;
        }
        _ => {}
    }
    true
}

// ---------- Régressions (exécutées sous le runtime en CI) ----------

pub unsafe fn regression_checks() {
    // Formats de fichiers.
    let mut buf = [0u8; 256];
    let n = build_hdrop(&[b"C:\\Documents\\a.txt", b"C:\\Images\\b.png"], &mut buf);
    let mut names = [0u8; 64];
    let (l, c) = hdrop_names(&buf[..n], &mut names, false);
    check!(
        c == 2 && &names[..l] == b"a.txt\nb.png",
        "Historique : CF_HDROP construit puis relu (2 fichiers)"
    );
    check!(
        matches(b"Bonjour \xC9t\xE9", b"\xE9T\xC9") && !matches(b"abc", b"abd"),
        "Historique : recherche sans casse, accents compris"
    );

    // Presse-papiers réel du runtime : chaque copie entre dans l'historique.
    poll();
    let before = COUNT;
    check!(
        set_text(b"Winlux historique 1"),
        "Historique : SetClipboardData(CF_TEXT) accepte"
    );
    let mut back = [0u8; 64];
    let bl = get_text(&mut back);
    check!(
        &back[..bl] == b"Winlux historique 1",
        "Historique : le texte copie se relit depuis le presse-papiers"
    );
    check!(
        poll() && COUNT == before + 1 && preview(0) == b"Winlux historique 1",
        "Historique : nouvelle copie ajoutee en tete"
    );
    set_text(b"Winlux historique 2");
    poll();
    set_text(b"Winlux historique 1");
    poll();
    check!(
        COUNT == before + 2 && preview(0) == b"Winlux historique 1",
        "Historique : un doublon remonte en tete sans etre duplique"
    );
    check!(
        restore(1) && preview(0) == b"Winlux historique 2" && !poll(),
        "Historique : recoller remet l'element en tete sans doublon"
    );
    let saved_limit = LIMIT;
    LIMIT = 10;
    toggle_pin(0);
    for k in 0..12u8 {
        let t = [b'x', b'0' + k / 10, b'0' + k % 10];
        add(Kind::Text, CF_TEXT, &t, false);
    }
    let unpinned = ITEMS[..COUNT].iter().filter(|i| !i.pinned).count();
    check!(
        unpinned == 10 && ITEMS[..COUNT].iter().any(|i| i.pinned),
        "Historique : la limite s'applique sans toucher aux epingles"
    );
    clear_unpinned();
    check!(
        COUNT == 1 && is_pinned(0),
        "Historique : Tout effacer garde les elements epingles"
    );
    toggle_pin(0);
    clear_unpinned();
    LIMIT = saved_limit;
    save_pins();
    check!(
        COUNT == 0,
        "Historique : desepingler puis effacer vide la liste"
    );
}

// ---------- Traductions espagnoles et allemandes ----------

/// `(français, anglais, espagnol, allemand)`, retrouvées par le couple
/// français/anglais passé à `tr` (presse-papiers et Outil Capture).
const CLIP_TRANSLATIONS: &[(&[u8], &[u8], &[u8], &[u8])] = &[
    (b"image capture", b"image picture", b"imagen", b"Bild"),
    (
        b"\xC0 l'instant",
        b"Just now",
        b"Ahora mismo",
        b"Gerade eben",
    ),
    (b"Il y a ", b"", b"Hace ", b"Vor "),
    (b" min", b" min ago", b" min", b" Min."),
    (b" h", b" h ago", b" h", b" Std."),
    (
        b"Tout effacer",
        b"Clear all",
        b"Borrar todo",
        b"Alle l\xF6schen",
    ),
    (
        b"Presse-papiers",
        b"Clipboard",
        b"Portapapeles",
        b"Zwischenablage",
    ),
    (
        b"Rechercher dans l'historique",
        b"Search history",
        b"Buscar en el historial",
        b"Verlauf durchsuchen",
    ),
    (
        b"Vous n'avez encore rien copi\xE9",
        b"You haven't copied anything yet",
        b"A\xFAn no ha copiado nada",
        b"Sie haben noch nichts kopiert",
    ),
    (
        b"Copiez du texte, une image ou des fichiers : ils appara\xEEtront ici.",
        b"Copy text, an image or files and they will show up here.",
        b"Copie texto, una imagen o archivos: aparecer\xE1n aqu\xED.",
        b"Kopieren Sie Text, ein Bild oder Dateien: Sie erscheinen hier.",
    ),
    (
        b"Aucun r\xE9sultat",
        b"No results",
        b"No hay resultados",
        b"Keine Ergebnisse",
    ),
    (
        b"Essayez un autre mot.",
        b"Try another word.",
        b"Pruebe con otra palabra.",
        b"Versuchen Sie ein anderes Wort.",
    ),
    (b" autre(s)", b" more", b" m\xE1s", b" weitere"),
    (b"Texte", b"Text", b"Texto", b"Text"),
    (b"Fichiers", b"Files", b"Archivos", b"Dateien"),
    (b"Limite : ", b"Limit: ", b"L\xEDmite: ", b"Grenze: "),
    (
        b" \xE9l\xE9ments  \x95  changer",
        b" items  \x95  change",
        b" elementos  \x95  cambiar",
        b" Elemente  \x95  \xE4ndern",
    ),
    (
        b"Capture impossible : m\xE9moire insuffisante.",
        b"Capture failed: out of memory.",
        b"Captura imposible: memoria insuficiente.",
        b"Aufnahme fehlgeschlagen: zu wenig Arbeitsspeicher.",
    ),
    (b"Fen\xEAtre", b"Window", b"Ventana", b"Fenster"),
    (
        b"Plein \xE9cran",
        b"Full screen",
        b"Pantalla completa",
        b"Vollbild",
    ),
    (b"Forme libre", b"Freeform", b"Forma libre", b"Freihand"),
    (
        b"Enregistrer une vid\xE9o",
        b"Record video",
        b"Grabar v\xEDdeo",
        b"Video aufnehmen",
    ),
    (
        b"Fermer (\xC9chap)",
        b"Close (Esc)",
        b"Cerrar (Esc)",
        b"Schlie\xDFen (Esc)",
    ),
    (
        b"Choisissez la zone \xE0 enregistrer",
        b"Choose the area to record",
        b"Elija la zona que desea grabar",
        b"W\xE4hlen Sie den aufzunehmenden Bereich",
    ),
    (
        b"Faites glisser pour capturer une zone",
        b"Drag to capture an area",
        b"Arrastre para capturar una zona",
        b"Ziehen Sie, um einen Bereich aufzunehmen",
    ),
    (
        b"Cliquez sur une fen\xEAtre",
        b"Click a window",
        b"Haga clic en una ventana",
        b"Klicken Sie auf ein Fenster",
    ),
    (
        b"Cliquez pour capturer l'\xE9cran",
        b"Click to capture the screen",
        b"Haga clic para capturar la pantalla",
        b"Klicken Sie, um den Bildschirm aufzunehmen",
    ),
    (
        b"Dessinez le contour \xE0 capturer",
        b"Draw the shape to capture",
        b"Dibuje el contorno que desea capturar",
        b"Zeichnen Sie den aufzunehmenden Umriss",
    ),
    (b"Micro : oui", b"Mic: on", b"Micro: s\xED", b"Mikro: ein"),
    (b"Micro : non", b"Mic: off", b"Micro: no", b"Mikro: aus"),
    (
        b"Son syst\xE8me : oui",
        b"System audio: on",
        b"Sonido del sistema: s\xED",
        b"Systemsound: ein",
    ),
    (
        b"Son syst\xE8me : non",
        b"System audio: off",
        b"Sonido del sistema: no",
        b"Systemsound: aus",
    ),
    (b"D\xE9marrer", b"Start", b"Iniciar", b"Starten"),
    (b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"),
    (
        b"Enregistrement impossible : ffmpeg est introuvable sur ce syst\xE8me.",
        b"Cannot record: ffmpeg was not found on this system.",
        b"No se puede grabar: no se encontr\xF3 ffmpeg en este sistema.",
        b"Aufnahme nicht m\xF6glich: ffmpeg wurde auf diesem System nicht gefunden.",
    ),
    (
        b"Enregistrement impossible : aucun affichage X11.",
        b"Cannot record: no X11 display.",
        b"No se puede grabar: no hay pantalla X11.",
        b"Aufnahme nicht m\xF6glich: keine X11-Anzeige.",
    ),
    (
        b"Outil Capture",
        b"Snipping Tool",
        b"Recortes",
        b"Snipping Tool",
    ),
    (
        b"Vid\xE9o enregistr\xE9e : ",
        b"Video saved: ",
        b"V\xEDdeo guardado: ",
        b"Video gespeichert: ",
    ),
    (
        b"Enregistrement interrompu, fichier peut-\xEAtre incomplet : ",
        b"Recording interrupted, file may be incomplete: ",
        b"Grabaci\xF3n interrumpida, puede que el archivo est\xE9 incompleto: ",
        b"Aufnahme unterbrochen, Datei m\xF6glicherweise unvollst\xE4ndig: ",
    ),
    (
        b"Vid\xE9o enregistr\xE9e dans Vid\xE9os",
        b"Video saved to Videos",
        b"V\xEDdeo guardado en V\xEDdeos",
        b"Video in Videos gespeichert",
    ),
    (
        b"Enregistrement interrompu",
        b"Recording interrupted",
        b"Grabaci\xF3n interrumpida",
        b"Aufnahme unterbrochen",
    ),
    (
        b"L'enregistrement s'est arr\xEAt\xE9 (voir screen-recorder.log).",
        b"Recording stopped unexpectedly (see screen-recorder.log).",
        b"La grabaci\xF3n se detuvo (consulte screen-recorder.log).",
        b"Die Aufnahme wurde beendet (siehe screen-recorder.log).",
    ),
    (
        b"Capture copi\xE9e dans le presse-papiers. Annotez-la ou enregistrez-la.",
        b"Snip copied to the clipboard. Annotate or save it.",
        b"Recorte copiado en el portapapeles. An\xF3telo o gu\xE1rdelo.",
        b"Ausschnitt in die Zwischenablage kopiert. Beschriften oder speichern Sie ihn.",
    ),
    (
        b"Capture enregistr\xE9e : ",
        b"Snip saved: ",
        b"Recorte guardado: ",
        b"Ausschnitt gespeichert: ",
    ),
    (
        b"\xC9chec de l'enregistrement : ",
        b"Could not save: ",
        b"No se pudo guardar: ",
        b"Speichern fehlgeschlagen: ",
    ),
    (b"Rogner", b"Crop", b"Recortar", b"Zuschneiden"),
    (b"Stylo", b"Pen", b"Bol\xEDgrafo", b"Stift"),
    (b"Surligneur", b"Highlighter", b"Resaltador", b"Textmarker"),
    (b"Fl\xE8che", b"Arrow", b"Flecha", b"Pfeil"),
    (b"Fin", b"Thin", b"Fino", b"D\xFCnn"),
    (b"Moyen", b"Medium", b"Medio", b"Mittel"),
    (b"\xC9pais", b"Thick", b"Grueso", b"Dick"),
    (
        b"Annuler (Ctrl+Z)",
        b"Undo (Ctrl+Z)",
        b"Deshacer (Ctrl+Z)",
        b"R\xFCckg\xE4ngig (Strg+Z)",
    ),
    (
        b"Copier (Ctrl+C)",
        b"Copy (Ctrl+C)",
        b"Copiar (Ctrl+C)",
        b"Kopieren (Strg+C)",
    ),
    (
        b"Enregistrer (Ctrl+S)",
        b"Save (Ctrl+S)",
        b"Guardar (Ctrl+S)",
        b"Speichern (Strg+S)",
    ),
    (b"Fermer", b"Close", b"Cerrar", b"Schlie\xDFen"),
    (b"Couleur", b"Color", b"Color", b"Farbe"),
    (b"Enregistrer", b"Save", b"Guardar", b"Speichern"),
    (b"Copier", b"Copy", b"Copiar", b"Kopieren"),
    (
        b"Faites glisser pour rogner l'image",
        b"Drag to crop the image",
        b"Arrastre para recortar la imagen",
        b"Ziehen Sie, um das Bild zuzuschneiden",
    ),
    (
        b"Cliquez puis tapez, Entr\xE9e pour valider",
        b"Click then type, Enter to apply",
        b"Haga clic y escriba, Intro para confirmar",
        b"Klicken und tippen, Eingabe zum Best\xE4tigen",
    ),
    (
        b"Dessinez sur la capture. Ctrl+Z annule, \xC9chap ferme.",
        b"Draw on the snip. Ctrl+Z undoes, Esc closes.",
        b"Dibuje sobre el recorte. Ctrl+Z deshace, Esc cierra.",
        b"Zeichnen Sie auf den Ausschnitt. Strg+Z macht r\xFCckg\xE4ngig, Esc schlie\xDFt.",
    ),
    (
        b"Capture copi\xE9e dans le presse-papiers",
        b"Snip copied to the clipboard",
        b"Recorte copiado en el portapapeles",
        b"Ausschnitt in die Zwischenablage kopiert",
    ),
];
