//! `PEB->Ldr` (#1314) : PEB_LDR_DATA et une LDR_DATA_TABLE_ENTRY par
//! module PE chargé, disposition x64 documentée dans winternl.h.
//!
//! Un autre processus les lit avec ReadProcessMemory : le crashpad de
//! Chromium parcourt les trois listes pour écrire le dump d'un enfant
//! planté, et abandonnait sur `Ldr` nul (erreur 299). Les entrées sont
//! donc tenues à jour à chaque chargement et déchargement, sans attendre
//! que le programme les demande.

use std::collections::HashMap;
use std::sync::Mutex;

/// Décalage de `Ldr` dans le PEB x64.
pub const PEB_LDR: usize = 0x18;
/// Taille de PEB_LDR_DATA (champ `Length`).
const LDR_DATA_SIZE: usize = 0x58;
/// InLoadOrderModuleList, InMemoryOrderModuleList,
/// InInitializationOrderModuleList dans PEB_LDR_DATA.
const LDR_LISTS: usize = 0x10;
/// Taille réservée pour une LDR_DATA_TABLE_ENTRY (0x120 sous Windows 10).
const ENTRY_SIZE: usize = 0x120;
// Champs de LDR_DATA_TABLE_ENTRY (x64).
const E_DLL_BASE: usize = 0x30;
const E_ENTRY_POINT: usize = 0x38;
const E_SIZE_OF_IMAGE: usize = 0x40;
const E_FULL_NAME: usize = 0x48;
const E_BASE_NAME: usize = 0x58;
const E_FLAGS: usize = 0x68;
const E_LOAD_COUNT: usize = 0x6c;
const E_HASH_LINKS: usize = 0x70;
const E_TIME_DATE_STAMP: usize = 0x80;

struct Ldr {
    data: usize,
    /// Adresse de base du module → son entrée (jamais libérée : un lecteur
    /// d'un autre processus peut encore la parcourir).
    entries: HashMap<usize, usize>,
}

static LDR: Mutex<Option<Ldr>> = Mutex::new(None);

/// Un module tel que l'entrée le décrit.
pub struct Image {
    pub base: usize,
    pub size: usize,
    pub entry: usize,
    pub full_name: String,
}

fn leak_zeroed(size: usize) -> usize {
    Box::leak(vec![0u64; size.div_ceil(8)].into_boxed_slice()).as_mut_ptr() as usize
}

/// UNICODE_STRING (Length, MaximumLength, Buffer) vers une copie terminée par 0.
unsafe fn unicode_string(at: usize, s: &str) {
    let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let len = (w.len() - 1) * 2;
    let buf = Box::leak(w.into_boxed_slice());
    *(at as *mut u16) = len.min(0xfffc) as u16;
    *((at + 2) as *mut u16) = (len + 2).min(0xfffe) as u16;
    *((at + 8) as *mut usize) = buf.as_ptr() as usize;
}

fn time_date_stamp(base: usize, size: usize) -> u32 {
    // e_lfanew puis IMAGE_FILE_HEADER.TimeDateStamp (signature + 4).
    unsafe {
        if size < 0x40 || *(base as *const u16) != 0x5a4d {
            return 0;
        }
        let pe = *((base + 0x3c) as *const u32) as usize;
        if pe + 12 > size {
            return 0;
        }
        *((base + pe + 8) as *const u32)
    }
}

unsafe fn new_entry(m: &Image) -> usize {
    let e = leak_zeroed(ENTRY_SIZE);
    *((e + E_DLL_BASE) as *mut usize) = m.base;
    *((e + E_ENTRY_POINT) as *mut usize) = m.entry;
    *((e + E_SIZE_OF_IMAGE) as *mut u32) = m.size as u32;
    unicode_string(e + E_FULL_NAME, &m.full_name);
    let base_name = m.full_name.rsplit(['\\', '/']).next().unwrap_or("");
    unicode_string(e + E_BASE_NAME, base_name);
    // LDRP_IMAGE_DLL (0x4) sauf pour l'exécutable, LDRP_ENTRY_PROCESSED (0x4000).
    *((e + E_FLAGS) as *mut u32) = 0x4000;
    *((e + E_LOAD_COUNT) as *mut u16) = 0xffff;
    // HashLinks vide : pointe sur lui-même.
    *((e + E_HASH_LINKS) as *mut usize) = e + E_HASH_LINKS;
    *((e + E_HASH_LINKS + 8) as *mut usize) = e + E_HASH_LINKS;
    *((e + E_TIME_DATE_STAMP) as *mut u32) = time_date_stamp(m.base, m.size);
    e
}

/// Relie en anneau les nœuds (Flink, Blink) d'une liste LIST_ENTRY.
unsafe fn link(nodes: &[usize]) {
    let n = nodes.len();
    for (i, &node) in nodes.iter().enumerate() {
        *(node as *mut usize) = nodes[(i + 1) % n];
        *((node + 8) as *mut usize) = nodes[(i + n - 1) % n];
    }
}

/// Reconstruit les trois listes dans l'ordre des modules donnés
/// (l'exécutable en premier, comme sous Windows).
fn rebuild(ldr: &mut Ldr, modules: &[Image]) {
    let live: Vec<usize> = modules
        .iter()
        .map(|m| {
            *ldr.entries
                .entry(m.base)
                .or_insert_with(|| unsafe { new_entry(m) })
        })
        .collect();
    if let Some(&exe) = live.first() {
        unsafe { *((exe + E_FLAGS) as *mut u32) = 0x4000 };
    }
    for &e in live.iter().skip(1) {
        unsafe { *((e + E_FLAGS) as *mut u32) = 0x4004 };
    }
    for list in 0..3 {
        let k = list * 0x10;
        let nodes: Vec<usize> = std::iter::once(ldr.data + LDR_LISTS + k)
            .chain(live.iter().map(|&e| e + k))
            .collect();
        unsafe { link(&nodes) };
    }
}

fn loaded_images() -> Vec<Image> {
    crate::loader::MODULES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|m| Image {
            base: m.base,
            size: m.size,
            entry: m.entry().unwrap_or(0),
            full_name: crate::paths::to_windows(&m.path),
        })
        .collect()
}

/// Pose `PEB->Ldr` et y inscrit les modules déjà chargés.
pub fn install(peb: usize) {
    let data = leak_zeroed(LDR_DATA_SIZE);
    unsafe {
        *(data as *mut u32) = LDR_DATA_SIZE as u32;
        *((data + 4) as *mut u8) = 1; // Initialized
        *((peb + PEB_LDR) as *mut usize) = data;
    }
    let mut ldr = Ldr {
        data,
        entries: HashMap::new(),
    };
    rebuild(&mut ldr, &loaded_images());
    *LDR.lock().unwrap_or_else(|e| e.into_inner()) = Some(ldr);
}

/// Après un chargement ou un déchargement : listes à jour. Sans effet
/// tant que le PEB n'est pas posé (il inscrit alors tout d'un coup).
pub fn sync() {
    let images = loaded_images();
    if let Some(ldr) = LDR.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        rebuild(ldr, &images);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn walk(head: usize, k: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut node = *(head as *const usize);
        while node != head && out.len() < 16 {
            out.push(*((node - k + E_DLL_BASE) as *const usize));
            node = *(node as *const usize);
        }
        out
    }

    #[test]
    fn peb_ldr_liste_les_modules_dans_l_ordre_de_chargement() {
        let peb = leak_zeroed(0x400);
        let image = leak_zeroed(0x1000);
        unsafe {
            *(image as *mut u16) = 0x5a4d;
            *((image + 0x3c) as *mut u32) = 0x80;
            *((image + 0x88) as *mut u32) = 0x1234_5678;
        }
        let data = leak_zeroed(LDR_DATA_SIZE);
        unsafe {
            *(data as *mut u32) = LDR_DATA_SIZE as u32;
            *((peb + PEB_LDR) as *mut usize) = data;
        }
        let mut ldr = Ldr {
            data,
            entries: HashMap::new(),
        };
        let exe = Image {
            base: image,
            size: 0x1000,
            entry: image + 0x10,
            full_name: "C:\\Programmes\\app.exe".into(),
        };
        let dll = |base| Image {
            base,
            size: 0x100,
            entry: 0,
            full_name: "C:\\Programmes\\outil.dll".into(),
        };
        let (dll1, dll2) = (leak_zeroed(0x100), leak_zeroed(0x100));
        rebuild(&mut ldr, &[exe, dll(dll1)]);
        unsafe {
            let data = *((peb + PEB_LDR) as *const usize);
            assert_eq!(*(data as *const u32), 0x58);
            for list in 0..3 {
                let k = list * 0x10;
                assert_eq!(walk(data + LDR_LISTS + k, k), vec![image, dll1]);
            }
            let first = *((data + LDR_LISTS) as *const usize);
            assert_eq!(*((first + E_ENTRY_POINT) as *const usize), image + 0x10);
            assert_eq!(*((first + E_SIZE_OF_IMAGE) as *const u32), 0x1000);
            assert_eq!(*((first + E_TIME_DATE_STAMP) as *const u32), 0x1234_5678);
            let name = *((first + E_BASE_NAME + 8) as *const usize) as *const u16;
            let len = *((first + E_BASE_NAME) as *const u16) as usize / 2;
            assert_eq!(
                String::from_utf16_lossy(std::slice::from_raw_parts(name, len)),
                "app.exe"
            );
        }
        // Déchargement de la DLL puis nouvelle DLL : l'anneau suit.
        let exe = Image {
            base: image,
            size: 0x1000,
            entry: image + 0x10,
            full_name: "C:\\Programmes\\app.exe".into(),
        };
        rebuild(&mut ldr, &[exe, dll(dll2)]);
        unsafe {
            for list in 0..3 {
                let k = list * 0x10;
                assert_eq!(walk(data + LDR_LISTS + k, k), vec![image, dll2]);
            }
        }
    }
}
