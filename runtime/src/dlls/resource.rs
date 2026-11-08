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
    let base = base & !loader::DATA_HANDLE_BITS;
    let m = {
        let mods = loader::MODULES.lock().unwrap();
        mods.iter().find(|m| m.base == base).cloned()
    }
    .or_else(|| loader::data_image(base))?;
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
    let entry = find_resource_entry(base, type_id, name_id, type_is_str, name_is_str)?;
    // SAFETY: find_resource_entry a vérifié que l'entrée est dans la section.
    unsafe { Some(read_data_entry(entry)) }
}

/// RVA et taille lues dans une IMAGE_RESOURCE_DATA_ENTRY en mémoire.
unsafe fn read_data_entry(entry: usize) -> (u32, u32) {
    let p = entry as *const u32;
    (p.read_unaligned(), p.add(1).read_unaligned())
}

/// Adresse de l'IMAGE_RESOURCE_DATA_ENTRY de la ressource dans l'image :
/// c'est le HRSRC, comme sous Windows. Il tient sur 32 bits pour un
/// programme x86 (image sous 4 Go) ; l'ancien encodage « RVA << 32 |
/// taille » y perdait le RVA, et LoadResource rendait l'en-tête MZ au lieu
/// des données (fiches Delphi illisibles, WinSCP 32 bits, #905).
fn find_resource_entry(
    base: usize,
    type_id: usize,
    name_id: usize,
    type_is_str: bool,
    name_is_str: bool,
) -> Option<usize> {
    let (rsrc_rva, rsrc) = resource_root(base)?;

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

    // IMAGE_RESOURCE_DATA_ENTRY : OffsetToData, Size, CodePage, Reserved.
    rd32(rsrc, data_entry_off + 4)?;
    Some((base & !loader::DATA_HANDLE_BITS) + rsrc_rva + data_entry_off)
}

/// Manifeste d'application (RT_MANIFEST, identifiant 1) d'un module chargé.
pub fn manifest(base: usize) -> Option<String> {
    let (rva, size) = find_resource_data(base, 24, 1, false, false)?;
    // SAFETY: find_resource_data a vérifié que la section est dans l'image.
    let bytes =
        unsafe { std::slice::from_raw_parts((base + rva as usize) as *const u8, size as usize) };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Données d'une ressource d'un module : `name` et `ty` sont un entier
/// (MAKEINTRESOURCE) ou un pointeur vers une chaîne UTF-16.
pub fn resource_bytes(hmod: usize, name: usize, ty: usize) -> Option<&'static [u8]> {
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod & !loader::DATA_HANDLE_BITS
    };
    if base == 0 {
        return None;
    }
    let (rva, size) =
        find_resource_data(base, ty, name, !is_int_resource(ty), !is_int_resource(name))?;
    // SAFETY: find_resource_data a vérifié que la ressource est dans l'image.
    Some(unsafe { std::slice::from_raw_parts((base + rva as usize) as *const u8, size as usize) })
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
        hmod & !loader::DATA_HANDLE_BITS
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

    match find_resource_entry(base, type_id, name_id, type_is_str, name_is_str) {
        Some(entry) => entry,
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
        hmod & !loader::DATA_HANDLE_BITS
    };
    if base == 0 {
        crate::process::set_last_error(6);
        return 0;
    }

    let type_id = ty as usize;
    let name_id = name as usize;
    let type_is_str = !is_int_resource(ty as usize);
    let name_is_str = !is_int_resource(name as usize);

    match find_resource_entry(base, type_id, name_id, type_is_str, name_is_str) {
        Some(entry) => entry,
        None => {
            crate::process::set_last_error(1813);
            0
        }
    }
}

/// HRSRC plausible : l'adresse d'une entrée de données (jamais une petite
/// valeur, 0 compris).
fn data_entry(hrsrc: usize) -> Option<(u32, u32)> {
    if hrsrc < 0x10000 {
        return None;
    }
    // SAFETY: un HRSRC vient de FindResource, qui rend l'adresse d'une
    // IMAGE_RESOURCE_DATA_ENTRY dans une image chargée.
    Some(unsafe { read_data_entry(hrsrc) })
}

/// LoadResource : renvoie un HGLOBAL (pointeur vers les données en mémoire).
pub unsafe extern "win64" fn load_resource(hmod: usize, hrsrc: usize) -> usize {
    let Some((rva, _size)) = data_entry(hrsrc) else {
        return 0;
    };
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod & !loader::DATA_HANDLE_BITS
    };
    base + rva as usize
}

/// SizeofResource
pub unsafe extern "win64" fn sizeof_resource(_hmod: usize, hrsrc: usize) -> u32 {
    data_entry(hrsrc).map_or(0, |(_rva, size)| size)
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
        hmod & !loader::DATA_HANDLE_BITS
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

/// Chaîne `id` de la table RT_STRING : adresse de ses caractères UTF-16
/// dans l'image et longueur (les chaînes sont groupées par 16).
fn string_entry(base: usize, id: u32) -> Option<(usize, usize)> {
    let (rva, size) = find_resource_data(base, 6, (id / 16 + 1) as usize, false, false)?;
    // SAFETY: find_resource_data a vérifié que la section est dans l'image.
    let data =
        unsafe { std::slice::from_raw_parts((base + rva as usize) as *const u8, size as usize) };
    let mut off = 0;
    for i in 0..16 {
        let len = pe::rd16(data, off)? as usize;
        off += 2;
        if i == (id % 16) as usize {
            if len == 0 || off + len * 2 > data.len() {
                return None;
            }
            return Some((base + rva as usize + off, len));
        }
        off += len * 2;
    }
    None
}

/// LoadStringW. Avec `buf_size` = 0, `buf` reçoit un pointeur en lecture
/// seule vers la chaîne dans l'image (non terminée), comme sous Windows.
pub unsafe extern "win64" fn load_string_w(
    hmod: usize,
    id: u32,
    buf: *mut u16,
    buf_size: i32,
) -> i32 {
    if buf.is_null() || buf_size < 0 {
        return 0;
    }
    let base = if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod & !loader::DATA_HANDLE_BITS
    };
    let entry = if base == 0 {
        None
    } else {
        string_entry(base, id)
    };
    if buf_size == 0 {
        let (ptr, len) = entry.unwrap_or((0, 0));
        // Pointeur de 4 octets pour un programme x86.
        if crate::wow32::pointer_size() == 4 {
            (buf as *mut u32).write_unaligned(ptr as u32);
        } else {
            (buf as *mut usize).write_unaligned(ptr);
        }
        return len as i32;
    }
    let Some((ptr, len)) = entry else {
        *buf = 0;
        return 0;
    };
    let n = len.min((buf_size as usize) - 1);
    std::ptr::copy_nonoverlapping(ptr as *const u16, buf, n);
    *buf.add(n) = 0;
    n as i32
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

    /// #905 : le HRSRC est l'adresse de l'IMAGE_RESOURCE_DATA_ENTRY ;
    /// SizeofResource et LoadResource y lisent taille et RVA.
    #[test]
    fn hrsrc_est_l_adresse_de_l_entree() {
        let entry: Box<[u32; 4]> = Box::new([0x1234, 0x5678, 0, 0]);
        let h = entry.as_ptr() as usize;
        assert_eq!(data_entry(h), Some((0x1234, 0x5678)));
        assert_eq!(unsafe { sizeof_resource(0, h) }, 0x5678);
        assert_eq!(unsafe { load_resource(0x40_0000, h) }, 0x40_0000 + 0x1234);
        assert_eq!(data_entry(0), None);
        assert_eq!(unsafe { load_resource(0x40_0000, 0) }, 0);
    }
}
