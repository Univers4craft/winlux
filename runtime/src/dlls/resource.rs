//! Chargement des ressources PE (FindResource, LoadResource, etc.).
//!
//! Les ressources sont lues directement depuis l'image PE en mémoire : la
//! section `.rsrc` a déjà été copiée par le chargeur, il suffit de parcourir
//! l'arbre IMAGE_RESOURCE_DIRECTORY.

use crate::loader;
use crate::pe;

const DIR_RESOURCE: usize = 2;

// ── Structures de l'arbre des ressources ────────────────────────────

fn rd16(b: &[u8], o: usize) -> Option<u16> {
    pe::rd16(b, o)
}
fn rd32(b: &[u8], o: usize) -> Option<u32> {
    pe::rd32(b, o)
}

/// Trouve le répertoire de ressources du module.
fn resource_root(base: usize) -> Option<(usize, &'static [u8])> {
    let mods = loader::MODULES.lock().unwrap();
    let m = mods.iter().find(|m| m.base == base)?;
    let d = m.pe.dir(DIR_RESOURCE);
    if d.rva == 0 || d.size == 0 {
        return None;
    }
    let rva = d.rva as usize;
    let size = d.size as usize;
    if rva + size > m.size {
        return None;
    }
    let data = unsafe { std::slice::from_raw_parts((base + rva) as *const u8, size) };
    Some((rva, data))
}

/// Cherche une entrée dans un répertoire de ressources.
/// `id` est soit un ID numérique (MAKEINTRESOURCE), soit un pointeur vers
/// une chaîne.
fn find_entry(rsrc: &[u8], dir_offset: usize, id: usize, is_string: bool) -> Option<u32> {
    let named = rd16(rsrc, dir_offset + 12)? as usize;
    let id_entries = rd16(rsrc, dir_offset + 14)? as usize;
    let total = named + id_entries;
    let entries_start = dir_offset + 16;

    if is_string {
        // Recherche par nom dans les entrées nommées.
        for i in 0..named {
            let off = entries_start + i * 8;
            let name_off = rd32(rsrc, off)? as usize;
            if name_off & 0x8000_0000 == 0 {
                continue;
            }
            let noff = name_off & 0x7FFF_FFFF;
            if names_match(rsrc, noff, id) {
                return rd32(rsrc, off + 4);
            }
        }
    } else {
        // Recherche par ID dans les entrées non nommées.
        for i in named..total {
            let off = entries_start + i * 8;
            let entry_id = rd32(rsrc, off)? as usize;
            if entry_id == id {
                return rd32(rsrc, off + 4);
            }
        }
    }
    None
}

/// Compare un nom de ressource Unicode (dans l'arbre) avec un pointeur vers
/// une chaîne ANSI large (UTF-16).
fn names_match(rsrc: &[u8], name_offset: usize, ptr: usize) -> bool {
    let len = match rd16(rsrc, name_offset) {
        Some(l) => l as usize,
        None => return false,
    };
    // Le pointeur pointe vers une chaîne UTF-16 terminée par 0 : on la lit
    // unité par unité jusqu'au NUL, sans jamais dépasser sa fin.
    if ptr == 0 || len > 256 {
        return false;
    }
    let user = ptr as *const u16;
    for i in 0..len {
        let c = match rd16(rsrc, name_offset + 2 + i * 2) {
            Some(c) => c,
            None => return false,
        };
        let u = unsafe { std::ptr::read_unaligned(user.add(i)) };
        if u == 0 {
            return false;
        }
        // Comparaison insensible à la casse pour ASCII.
        let a = if c < 128 {
            (c as u8).to_ascii_uppercase() as u16
        } else {
            c
        };
        let b = if u < 128 {
            (u as u8).to_ascii_uppercase() as u16
        } else {
            u
        };
        if a != b {
            return false;
        }
    }
    // L'utilisateur doit aussi avoir fini.
    unsafe { std::ptr::read_unaligned(user.add(len)) == 0 }
}

/// Parcourt l'arbre : type → name → language (on prend le premier langage).
fn find_resource_data(
    base: usize,
    type_id: usize,
    name_id: usize,
    type_is_str: bool,
    name_is_str: bool,
) -> Option<(u32, u32)> {
    let (_rsrc_rva, rsrc) = resource_root(base)?;

    // Niveau 1 : type
    let type_val = find_entry(rsrc, 0, type_id, type_is_str)?;
    if type_val & 0x8000_0000 == 0 {
        return None;
    }
    let name_dir = (type_val & 0x7FFF_FFFF) as usize;

    // Niveau 2 : nom/id
    let name_val = find_entry(rsrc, name_dir, name_id, name_is_str)?;
    if name_val & 0x8000_0000 == 0 {
        return None;
    }
    let lang_dir = (name_val & 0x7FFF_FFFF) as usize;

    // Niveau 3 : on prend la première langue.
    let named = rd16(rsrc, lang_dir + 12)? as usize;
    let id_entries = rd16(rsrc, lang_dir + 14)? as usize;
    if named + id_entries == 0 {
        return None;
    }
    let off = lang_dir + 16;
    let data_entry_off = rd32(rsrc, off + 4)? as usize;
    if data_entry_off & 0x8000_0000 != 0 {
        return None;
    }

    // IMAGE_RESOURCE_DATA_ENTRY
    let data_rva = rd32(rsrc, data_entry_off)?;
    let data_size = rd32(rsrc, data_entry_off + 4)?;
    Some((data_rva, data_size))
}

// ── Fonctions exportées ─────────────────────────────────────────────

fn is_int_resource(p: usize) -> bool {
    p < 0x10000
}

/// FindResourceA : renvoie un HRSRC (encodage du RVA et de la taille).
pub unsafe extern "win64" fn find_resource_a(hmod: usize, name: *const u8, ty: *const u8) -> usize {
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod
    };
    if base == 0 {
        crate::process::set_last_error(6);
        return 0;
    }

    // Les chaînes converties doivent survivre jusqu'à la fin de la recherche :
    // elles sont déclarées ici (et non dans les branches `else`, où elles
    // étaient libérées avant d'être lues).
    let type_w: Vec<u16>;
    let name_w: Vec<u16>;
    let type_id = if is_int_resource(ty as usize) {
        ty as usize
    } else {
        // Convertir la chaîne ANSI en UTF-16 pour la comparaison.
        let s = crate::dlls::cstr(ty);
        type_w = s.encode_utf16().chain(std::iter::once(0)).collect();
        type_w.as_ptr() as usize
    };
    let name_id = if is_int_resource(name as usize) {
        name as usize
    } else {
        let s = crate::dlls::cstr(name);
        name_w = s.encode_utf16().chain(std::iter::once(0)).collect();
        name_w.as_ptr() as usize
    };

    let type_is_str = !is_int_resource(ty as usize);
    let name_is_str = !is_int_resource(name as usize);

    match find_resource_data(base, type_id, name_id, type_is_str, name_is_str) {
        Some((rva, size)) => encode_hrsrc(rva, size),
        None => {
            crate::process::set_last_error(1813);
            0
        }
    }
}

pub unsafe extern "win64" fn find_resource_w(
    hmod: usize,
    name: *const u16,
    ty: *const u16,
) -> usize {
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod
    };
    if base == 0 {
        crate::process::set_last_error(6);
        return 0;
    }

    let type_id = ty as usize;
    let name_id = name as usize;
    let type_is_str = !is_int_resource(ty as usize);
    let name_is_str = !is_int_resource(name as usize);

    match find_resource_data(base, type_id, name_id, type_is_str, name_is_str) {
        Some((rva, size)) => encode_hrsrc(rva, size),
        None => {
            crate::process::set_last_error(1813);
            0
        }
    }
}

fn encode_hrsrc(rva: u32, size: u32) -> usize {
    ((rva as usize) << 32) | size as usize
}
fn decode_hrsrc(h: usize) -> (u32, u32) {
    ((h >> 32) as u32, h as u32)
}

/// LoadResource : renvoie un HGLOBAL (pointeur vers les données en mémoire).
pub unsafe extern "win64" fn load_resource(hmod: usize, hrsrc: usize) -> usize {
    if hrsrc == 0 {
        return 0;
    }
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod
    };
    let (rva, _size) = decode_hrsrc(hrsrc);
    base + rva as usize
}

/// SizeofResource
pub unsafe extern "win64" fn sizeof_resource(_hmod: usize, hrsrc: usize) -> u32 {
    if hrsrc == 0 {
        return 0;
    }
    let (_rva, size) = decode_hrsrc(hrsrc);
    size
}

/// LockResource : sous Windows, c'est un no-op qui renvoie le même pointeur.
pub unsafe extern "win64" fn lock_resource(hglobal: usize) -> usize {
    hglobal
}

/// FindResourceExA avec langage spécifié (on ignore le langage pour l'instant).
pub unsafe extern "win64" fn find_resource_ex_a(
    hmod: usize,
    ty: *const u8,
    name: *const u8,
    _lang: u16,
) -> usize {
    find_resource_a(hmod, name, ty)
}

pub unsafe extern "win64" fn find_resource_ex_w(
    hmod: usize,
    ty: *const u16,
    name: *const u16,
    _lang: u16,
) -> usize {
    find_resource_w(hmod, name, ty)
}

/// LoadStringA : charge une chaîne depuis la table STRINGTABLE du module.
pub unsafe extern "win64" fn load_string_a(
    hmod: usize,
    id: u32,
    buf: *mut u8,
    buf_size: i32,
) -> i32 {
    if buf.is_null() || buf_size <= 0 {
        return 0;
    }
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod
    };
    if base == 0 {
        *buf = 0;
        return 0;
    }
    // Les chaînes sont dans un bloc RT_STRING (type 6), regroupées par 16.
    let block_id = (id / 16 + 1) as usize;
    let index = (id % 16) as usize;
    let (rva, size) = match find_resource_data(base, 6, block_id, false, false) {
        Some(r) => r,
        None => {
            *buf = 0;
            return 0;
        }
    };
    let data = std::slice::from_raw_parts((base + rva as usize) as *const u8, size as usize);
    // Parcourir les 16 entrées : chacune a un u16 de longueur puis la chaîne UTF-16.
    let mut off = 0;
    for i in 0..16 {
        if off + 2 > data.len() {
            *buf = 0;
            return 0;
        }
        let len = pe::rd16(data, off).unwrap_or(0) as usize;
        off += 2;
        if i == index {
            if len == 0 {
                *buf = 0;
                return 0;
            }
            let wchars: Vec<u16> = (0..len)
                .filter_map(|j| pe::rd16(data, off + j * 2))
                .collect();
            let s = String::from_utf16_lossy(&wchars);
            let bytes = crate::dlls::encode_ansi(&s);
            let n = bytes.len().min((buf_size as usize) - 1);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n);
            *buf.add(n) = 0;
            return n as i32;
        }
        off += len * 2;
    }
    *buf = 0;
    0
}

/// LoadStringW
pub unsafe extern "win64" fn load_string_w(
    hmod: usize,
    id: u32,
    buf: *mut u16,
    buf_size: i32,
) -> i32 {
    if buf_size <= 0 {
        return 0;
    }
    if buf.is_null() {
        // Quand buf est null et buf_size == 0, Windows renvoie un pointeur en lecture seule.
        // On ne peut pas supporter cet usage facilement, on renvoie 0.
        return 0;
    }
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod
    };
    if base == 0 {
        *buf = 0;
        return 0;
    }
    let block_id = (id / 16 + 1) as usize;
    let index = (id % 16) as usize;
    let (rva, size) = match find_resource_data(base, 6, block_id, false, false) {
        Some(r) => r,
        None => {
            *buf = 0;
            return 0;
        }
    };
    let data = std::slice::from_raw_parts((base + rva as usize) as *const u8, size as usize);
    let mut off = 0;
    for i in 0..16 {
        if off + 2 > data.len() {
            *buf = 0;
            return 0;
        }
        let len = pe::rd16(data, off).unwrap_or(0) as usize;
        off += 2;
        if i == index {
            if len == 0 {
                *buf = 0;
                return 0;
            }
            let n = len.min((buf_size as usize) - 1);
            for j in 0..n {
                *buf.add(j) = pe::rd16(data, off + j * 2).unwrap_or(0);
            }
            *buf.add(n) = 0;
            return n as i32;
        }
        off += len * 2;
    }
    *buf = 0;
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "FindResourceA" => fn find_resource_a,
        "FindResourceW" => fn find_resource_w,
        "FindResourceExA" => fn find_resource_ex_a,
        "FindResourceExW" => fn find_resource_ex_w,
        "LoadResource" => fn load_resource,
        "SizeofResource" => fn sizeof_resource,
        "LockResource" => fn lock_resource,
        "LoadStringA" => fn load_string_a,
        "LoadStringW" => fn load_string_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_hrsrc() {
        let h = encode_hrsrc(0x1234, 0x5678);
        let (rva, size) = decode_hrsrc(h);
        assert_eq!(rva, 0x1234);
        assert_eq!(size, 0x5678);
    }
}
