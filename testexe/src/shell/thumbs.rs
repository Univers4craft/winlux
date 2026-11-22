//! Miniatures de l'Explorateur : images (décodeurs maison du runtime,
//! `WinluxImageThumbnail`) et vidéos/audio (`WinluxMediaThumbnail`).
//!
//! Le runtime calcule les vignettes en arrière-plan et les garde sur disque
//! (`~/.cache/winlux/thumbnails`) ; ce module garde en mémoire les dernières
//! vignettes affichées et redemande l'affichage tant qu'une vignette est en
//! cours de calcul. Sans vignette (format non décodé, fichier illisible),
//! l'appelant garde l'icône du type de fichier.

use winlux_testexe::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Image,
    Media,
}

const SLOTS: usize = 96;
const MAX_SIZE: usize = 128;

#[derive(Clone, Copy)]
struct Slot {
    key: u64,
    size: u32,
    /// 1 prête, 0 en cours, -1 aucune, 2 vide.
    state: i8,
    used: u32,
    retry_at: u32,
    pixels: *mut u8,
}

const EMPTY: Slot = Slot {
    key: 0,
    size: 0,
    state: 2,
    used: 0,
    retry_at: 0,
    pixels: core::ptr::null_mut(),
};

static mut CACHE: [Slot; SLOTS] = [EMPTY; SLOTS];
static mut CLOCK: u32 = 0;
static mut PENDING: bool = false;
static mut SCRATCH: *mut u8 = core::ptr::null_mut();

fn hash(path: &[u8], size: u32, src: Source) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &c in path {
        h ^= c.to_ascii_lowercase() as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= (size as u64) << 32 | (src == Source::Media) as u64;
    h.wrapping_mul(0x0000_0100_0000_01b3) | 1
}

unsafe fn buffer() -> *mut u8 {
    VirtualAlloc(core::ptr::null_mut(), MAX_SIZE * MAX_SIZE * 4, 0x3000, 4) as *mut u8
}

unsafe fn slot_for(key: u64, size: u32) -> &'static mut Slot {
    CLOCK = CLOCK.wrapping_add(1);
    let cache = &mut *core::ptr::addr_of_mut!(CACHE);
    let mut victim = 0;
    for (i, s) in cache.iter().enumerate() {
        if s.key == key && s.size == size {
            cache[i].used = CLOCK;
            return &mut cache[i];
        }
        if s.state == 2 || s.used < cache[victim].used {
            victim = i;
        }
    }
    let s = &mut cache[victim];
    let pixels = if s.pixels.is_null() {
        buffer()
    } else {
        s.pixels
    };
    *s = Slot {
        key,
        size,
        state: 0,
        used: CLOCK,
        retry_at: 0,
        pixels,
    };
    s.state = if pixels.is_null() { -1 } else { 0 };
    s
}

/// Vrai si une vignette était encore en calcul au dernier affichage : la
/// boucle de l'Explorateur doit repeindre pour la faire apparaître.
pub unsafe fn pending() -> bool {
    let p = PENDING;
    PENDING = false;
    p
}

/// Dessine la vignette de `path` (chemin Windows en Windows-1252) dans le
/// carré `size` en (`x`, `y`), composée sur `bg` (0x00BBGGRR). Renvoie
/// `false` si aucune vignette n'est (encore) disponible.
pub unsafe fn draw_thumb(
    hdc: Handle,
    path: &[u8],
    src: Source,
    x: i32,
    y: i32,
    size: i32,
    bg: u32,
) -> bool {
    if !crate::gfx::ENABLE_THUMBNAILS || size < 16 || path.is_empty() || path.len() >= 1024 {
        return false;
    }
    let size = (size as u32).min(MAX_SIZE as u32);
    let key = hash(path, size, src);
    let slot = slot_for(key, size);
    if slot.state == 0 {
        let now = GetTickCount();
        if slot.retry_at != 0 && (now.wrapping_sub(slot.retry_at) as i32) < 0 {
            PENDING = true;
            return false;
        }
        let mut wide = [0u16; 1024];
        for (i, &c) in path.iter().enumerate() {
            wide[i] = c as u16;
        }
        let r = match src {
            Source::Image => WinluxImageThumbnail(wide.as_ptr(), size, slot.pixels),
            Source::Media => WinluxMediaThumbnail(wide.as_ptr(), size, slot.pixels),
        };
        slot.state = r.clamp(-1, 1) as i8;
        if slot.state == 0 {
            slot.retry_at = now.wrapping_add(120);
            PENDING = true;
        }
    }
    if slot.state != 1 {
        return false;
    }
    if SCRATCH.is_null() {
        SCRATCH = buffer();
        if SCRATCH.is_null() {
            return false;
        }
    }
    // Vignette BGRA à alpha droit, composée sur la couleur du fond.
    let (br, bgc, bb) = (bg & 0xFF, (bg >> 8) & 0xFF, (bg >> 16) & 0xFF);
    let n = (size * size) as usize;
    for i in 0..n {
        let s = slot.pixels.add(i * 4);
        let d = SCRATCH.add(i * 4);
        let a = *s.add(3) as u32;
        let mixc = |c: u8, b: u32| ((c as u32 * a + b * (255 - a) + 127) / 255) as u8;
        *d = mixc(*s, bb);
        *d.add(1) = mixc(*s.add(1), bgc);
        *d.add(2) = mixc(*s.add(2), br);
        *d.add(3) = 255;
    }
    let info = BitmapInfoHeader {
        size: 40,
        width: size as i32,
        height: -(size as i32),
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_pels_per_meter: 0,
        y_pels_per_meter: 0,
        colors_used: 0,
        colors_important: 0,
    };
    SetDIBitsToDevice(
        hdc,
        x,
        y,
        size,
        size,
        0,
        0,
        0,
        size,
        SCRATCH as *const core::ffi::c_void,
        &info,
        0,
    ) != 0
}
