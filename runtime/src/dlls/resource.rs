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
    lookup_resource(base, type_id, name_id, type_is_str, name_is_str, None).ok()
}

/// Comme `find_resource_entry`, mais l'échec porte le code que FindResource
/// laisse dans GetLastError : 1812 si le module n'a pas de ressources, 1813
/// si le type manque, 1814 si le nom manque, 1815 si la langue manque
/// (#2394). `lang` : voir `walk_resource_tree`.
fn lookup_resource(
    base: usize,
    type_id: usize,
    name_id: usize,
    type_is_str: bool,
    name_is_str: bool,
    lang: Option<u16>,
) -> Result<usize, u32> {
    let (rsrc_rva, rsrc) = resource_root(base).ok_or(ERROR_RESOURCE_DATA_NOT_FOUND)?;
    let data_entry_off =
        walk_resource_tree(rsrc, type_id, name_id, type_is_str, name_is_str, lang)?;
    Ok((base & !loader::DATA_HANDLE_BITS) + rsrc_rva + data_entry_off)
}

/// Parcourt l'arbre type → nom → langue d'une section .rsrc et rend
/// l'offset de l'IMAGE_RESOURCE_DATA_ENTRY, ou le code d'erreur du niveau
/// où la recherche s'arrête.
///
/// `lang` (#2396) : `None` pour FindResource, qui prend la première langue ;
/// `Some(l)` pour FindResourceEx, voir `choose_language`.
fn walk_resource_tree(
    rsrc: &[u8],
    type_id: usize,
    name_id: usize,
    type_is_str: bool,
    name_is_str: bool,
    lang: Option<u16>,
) -> Result<usize, u32> {
    // Niveau 1 : type
    let type_val =
        find_entry(rsrc, 0, type_id, type_is_str).ok_or(ERROR_RESOURCE_TYPE_NOT_FOUND)?;
    if type_val & 0x8000_0000 == 0 {
        return Err(ERROR_RESOURCE_TYPE_NOT_FOUND);
    }
    let name_dir = (type_val & 0x7FFF_FFFF) as usize;

    // Niveau 2 : nom/id
    let name_val =
        find_entry(rsrc, name_dir, name_id, name_is_str).ok_or(ERROR_RESOURCE_NAME_NOT_FOUND)?;
    if name_val & 0x8000_0000 == 0 {
        return Err(ERROR_RESOURCE_NAME_NOT_FOUND);
    }
    let lang_dir = (name_val & 0x7FFF_FFFF) as usize;

    // Niveau 3 : la langue.
    let lang_missing = ERROR_RESOURCE_LANG_NOT_FOUND;
    let named = rd16(rsrc, lang_dir + 12).ok_or(lang_missing)? as usize;
    let id_entries = rd16(rsrc, lang_dir + 14).ok_or(lang_missing)? as usize;
    if named + id_entries == 0 {
        return Err(lang_missing);
    }
    let first = lang_dir + 16;
    let ids: Vec<(u16, usize)> = (named..named + id_entries)
        .filter_map(|i| {
            let off = first + i * 8;
            Some((rd32(rsrc, off)? as u16, off))
        })
        .collect();
    let off = match lang {
        None => first,
        Some(l) => choose_language(&ids, l, crate::intl::api::get_thread_ui_language())
            .ok_or(lang_missing)?,
    };
    let data_entry_off = rd32(rsrc, off + 4).ok_or(lang_missing)? as usize;
    if data_entry_off & 0x8000_0000 != 0 {
        return Err(lang_missing);
    }

    // IMAGE_RESOURCE_DATA_ENTRY : OffsetToData, Size, CodePage, Reserved.
    rd32(rsrc, data_entry_off + 4).ok_or(ERROR_RESOURCE_DATA_NOT_FOUND)?;
    Ok(data_entry_off)
}

/// Entrée de langue retenue par FindResourceEx parmi `ids` (langue, offset
/// de l'entrée), #2396. La page FindResourceExW dit seulement : « If this
/// parameter is MAKELANGID(LANG_NEUTRAL, SUBLANG_NEUTRAL), the current
/// language associated with the calling thread is used. » Elle ne décrit
/// aucun repli ; choix retenus :
/// - langue précise : cette langue seule, sinon aucune (1815, dont c'est
///   le libellé) ;
/// - langue neutre (0) : celle du thread, sinon l'entrée neutre, sinon la
///   première, comme FindResource, pour ne pas refuser une ressource qui
///   n'existe que dans une autre langue.
fn choose_language(ids: &[(u16, usize)], lang: u16, thread_lang: u16) -> Option<usize> {
    let find = |l: u16| ids.iter().find(|(id, _)| *id == l).map(|(_, off)| *off);
    if lang != 0 {
        return find(lang);
    }
    find(thread_lang)
        .or_else(|| find(0))
        .or_else(|| ids.first().map(|(_, off)| *off))
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

/// Recherche commune de FindResourceA/W. `name` et `ty` sont un entier
/// (MAKEINTRESOURCE), une chaîne, ou une chaîne « #NNN » : la documentation
/// de FindResource dit que le nombre décimal qui suit « # » est alors
/// l'identifiant entier (#2395).
unsafe fn find_resource_args(
    hmod: usize,
    name: usize,
    ty: usize,
    ansi: bool,
    lang: Option<u16>,
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
    // Les noms convertis vivent jusqu'à la fin de la recherche.
    let ty_key = arg_key(ty, ansi);
    let name_key = arg_key(name, ansi);
    let lookup_arg = |k: &ResKey, buf: &mut Vec<u16>| -> (usize, bool) {
        match k {
            ResKey::Id(n) => (*n as usize, false),
            ResKey::Name(w) => {
                buf.extend_from_slice(w);
                buf.push(0);
                (buf.as_ptr() as usize, true)
            }
        }
    };
    let mut ty_buf = Vec::new();
    let mut name_buf = Vec::new();
    let (type_id, type_is_str) = lookup_arg(&ty_key, &mut ty_buf);
    let (name_id, name_is_str) = lookup_arg(&name_key, &mut name_buf);
    match lookup_resource(base, type_id, name_id, type_is_str, name_is_str, lang) {
        Ok(entry) => entry,
        Err(code) => {
            crate::process::set_last_error(code);
            0
        }
    }
}

/// FindResourceA : renvoie un HRSRC, l'adresse de l'entrée de données.
pub unsafe extern "win64" fn find_resource_a(hmod: usize, name: *const u8, ty: *const u8) -> usize {
    find_resource_args(hmod, name as usize, ty as usize, true, None)
}

pub unsafe extern "win64" fn find_resource_w(
    hmod: usize,
    name: *const u16,
    ty: *const u16,
) -> usize {
    find_resource_args(hmod, name as usize, ty as usize, false, None)
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

/// FindResourceExA : comme FindResourceA, dans la langue demandée (#2396).
pub unsafe extern "win64" fn find_resource_ex_a(
    hmod: usize,
    ty: *const u8,
    name: *const u8,
    lang: u16,
) -> usize {
    find_resource_args(hmod, name as usize, ty as usize, true, Some(lang))
}

pub unsafe extern "win64" fn find_resource_ex_w(
    hmod: usize,
    ty: *const u16,
    name: *const u16,
    lang: u16,
) -> usize {
    find_resource_args(hmod, name as usize, ty as usize, false, Some(lang))
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

// ── Énumération (EnumResourceTypes/Names/Languages) ─────────────────

/// Clé d'une entrée de répertoire : identifiant ou nom UTF-16.
#[derive(Clone, Debug, PartialEq)]
enum ResKey {
    Id(u16),
    Name(Vec<u16>),
}

/// Entrées d'un répertoire : clé et valeur (sous-répertoire ou données).
fn dir_list(rsrc: &[u8], dir: usize) -> Option<Vec<(ResKey, u32)>> {
    let named = rd16(rsrc, dir + 12)? as usize;
    let ids = rd16(rsrc, dir + 14)? as usize;
    let mut out = Vec::with_capacity(named + ids);
    for i in 0..named + ids {
        let off = dir + 16 + i * 8;
        let key = rd32(rsrc, off)?;
        let val = rd32(rsrc, off + 4)?;
        let k = if key & 0x8000_0000 != 0 {
            let noff = (key & 0x7FFF_FFFF) as usize;
            let len = rd16(rsrc, noff)? as usize;
            let mut name = Vec::with_capacity(len);
            for j in 0..len {
                name.push(rd16(rsrc, noff + 2 + j * 2)?);
            }
            ResKey::Name(name)
        } else {
            ResKey::Id(key as u16)
        };
        out.push((k, val));
    }
    Some(out)
}

/// Copie d'une clé pour un rappel : MAKEINTRESOURCE ou chaîne terminée
/// sur le tas (sous 4 Go pour un programme x86), libérée après l'appel.
fn with_key<T>(k: &ResKey, ansi: bool, f: impl FnOnce(usize) -> T) -> T {
    match k {
        ResKey::Id(id) => f(*id as usize),
        ResKey::Name(n) => {
            let bytes: Vec<u8> = if ansi {
                let mut b = crate::dlls::encode_ansi(&String::from_utf16_lossy(n));
                b.push(0);
                b
            } else {
                n.iter()
                    .chain(std::iter::once(&0))
                    .flat_map(|c| c.to_le_bytes())
                    .collect()
            };
            // SAFETY : bloc alloué, rempli puis libéré ici.
            unsafe {
                let p = libc::malloc(bytes.len()) as *mut u8;
                if p.is_null() {
                    return f(0);
                }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                let r = f(p as usize);
                libc::free(p.cast());
                r
            }
        }
    }
}

/// Clé désignée par un argument : entier, ou chaîne (ANSI ou large).
/// Une chaîne « #123 » vaut l'identifiant 123, comme sous Windows.
unsafe fn arg_key(p: usize, ansi: bool) -> ResKey {
    if is_int_resource(p) {
        return ResKey::Id(p as u16);
    }
    let s = if ansi {
        crate::dlls::cstr(p as *const u8)
    } else {
        crate::dlls::wstr(p as *const u16)
    };
    if let Some(n) = s.strip_prefix('#').and_then(|d| d.parse::<u16>().ok()) {
        return ResKey::Id(n);
    }
    ResKey::Name(s.encode_utf16().collect())
}

fn key_eq(a: &ResKey, b: &ResKey) -> bool {
    match (a, b) {
        (ResKey::Id(x), ResKey::Id(y)) => x == y,
        (ResKey::Name(x), ResKey::Name(y)) => {
            let up = |c: &u16| {
                if *c < 128 {
                    (*c as u8).to_ascii_uppercase() as u16
                } else {
                    *c
                }
            };
            x.len() == y.len() && x.iter().map(up).eq(y.iter().map(up))
        }
        _ => false,
    }
}

const ERROR_RESOURCE_DATA_NOT_FOUND: u32 = 1812;
const ERROR_RESOURCE_TYPE_NOT_FOUND: u32 = 1813;
const ERROR_RESOURCE_NAME_NOT_FOUND: u32 = 1814;
const ERROR_RESOURCE_LANG_NOT_FOUND: u32 = 1815;
const ERROR_RESOURCE_ENUM_USER_STOP: u32 = 15106;

fn enum_base(hmod: usize) -> usize {
    if hmod == 0 {
        loader::main_module().map(|m| m.base).unwrap_or(0)
    } else {
        hmod & !loader::DATA_HANDLE_BITS
    }
}

/// Sous-répertoire de `key` dans le répertoire `dir`.
fn sub_dir(rsrc: &[u8], dir: usize, key: &ResKey) -> Option<usize> {
    dir_list(rsrc, dir)?
        .into_iter()
        .find(|(k, v)| key_eq(k, key) && v & 0x8000_0000 != 0)
        .map(|(_, v)| (v & 0x7FFF_FFFF) as usize)
}

fn fail(e: u32) -> i32 {
    crate::process::set_last_error(e);
    0
}

type TypeProc = unsafe extern "win64" fn(usize, usize, isize) -> i32;
type NameProc = unsafe extern "win64" fn(usize, usize, usize, isize) -> i32;
type LangProc = unsafe extern "win64" fn(usize, usize, usize, u16, isize) -> i32;

unsafe fn enum_types(hmod: usize, proc_: usize, param: isize, ansi: bool) -> i32 {
    let Some((_, rsrc)) = resource_root(enum_base(hmod)) else {
        return fail(ERROR_RESOURCE_DATA_NOT_FOUND);
    };
    if proc_ == 0 {
        return fail(87);
    }
    let f: TypeProc = std::mem::transmute(proc_);
    let list = dir_list(rsrc, 0).unwrap_or_default();
    if list.is_empty() {
        return fail(ERROR_RESOURCE_TYPE_NOT_FOUND);
    }
    for (k, _) in &list {
        if with_key(k, ansi, |t| f(hmod, t, param)) == 0 {
            return fail(ERROR_RESOURCE_ENUM_USER_STOP);
        }
    }
    1
}

unsafe fn enum_names(hmod: usize, ty: usize, proc_: usize, param: isize, ansi: bool) -> i32 {
    let Some((_, rsrc)) = resource_root(enum_base(hmod)) else {
        return fail(ERROR_RESOURCE_DATA_NOT_FOUND);
    };
    if proc_ == 0 {
        return fail(87);
    }
    let tk = arg_key(ty, ansi);
    let Some(tdir) = sub_dir(rsrc, 0, &tk) else {
        return fail(ERROR_RESOURCE_TYPE_NOT_FOUND);
    };
    let f: NameProc = std::mem::transmute(proc_);
    let list = dir_list(rsrc, tdir).unwrap_or_default();
    if list.is_empty() {
        return fail(ERROR_RESOURCE_NAME_NOT_FOUND);
    }
    for (k, _) in &list {
        if with_key(&tk, ansi, |t| with_key(k, ansi, |n| f(hmod, t, n, param))) == 0 {
            return fail(ERROR_RESOURCE_ENUM_USER_STOP);
        }
    }
    1
}

unsafe fn enum_langs(
    hmod: usize,
    ty: usize,
    name: usize,
    proc_: usize,
    param: isize,
    ansi: bool,
) -> i32 {
    let Some((_, rsrc)) = resource_root(enum_base(hmod)) else {
        return fail(ERROR_RESOURCE_DATA_NOT_FOUND);
    };
    if proc_ == 0 {
        return fail(87);
    }
    let (tk, nk) = (arg_key(ty, ansi), arg_key(name, ansi));
    let Some(tdir) = sub_dir(rsrc, 0, &tk) else {
        return fail(ERROR_RESOURCE_TYPE_NOT_FOUND);
    };
    let Some(ndir) = sub_dir(rsrc, tdir, &nk) else {
        return fail(ERROR_RESOURCE_NAME_NOT_FOUND);
    };
    let f: LangProc = std::mem::transmute(proc_);
    let list = dir_list(rsrc, ndir).unwrap_or_default();
    if list.is_empty() {
        return fail(ERROR_RESOURCE_LANG_NOT_FOUND);
    }
    for (k, _) in &list {
        let lang = match k {
            ResKey::Id(l) => *l,
            ResKey::Name(_) => continue,
        };
        let r = with_key(&tk, ansi, |t| {
            with_key(&nk, ansi, |n| f(hmod, t, n, lang, param))
        });
        if r == 0 {
            return fail(ERROR_RESOURCE_ENUM_USER_STOP);
        }
    }
    1
}

/// EnumResourceTypesW(module, rappel, lParam).
pub unsafe extern "win64" fn enum_resource_types_w(hmod: usize, f: usize, p: isize) -> i32 {
    enum_types(hmod, f, p, false)
}
pub unsafe extern "win64" fn enum_resource_types_a(hmod: usize, f: usize, p: isize) -> i32 {
    enum_types(hmod, f, p, true)
}
/// EnumResourceNamesW(module, type, rappel, lParam).
pub unsafe extern "win64" fn enum_resource_names_w(
    hmod: usize,
    ty: usize,
    f: usize,
    p: isize,
) -> i32 {
    enum_names(hmod, ty, f, p, false)
}
pub unsafe extern "win64" fn enum_resource_names_a(
    hmod: usize,
    ty: usize,
    f: usize,
    p: isize,
) -> i32 {
    enum_names(hmod, ty, f, p, true)
}
/// EnumResourceLanguagesW(module, type, nom, rappel, lParam).
pub unsafe extern "win64" fn enum_resource_languages_w(
    hmod: usize,
    ty: usize,
    name: usize,
    f: usize,
    p: isize,
) -> i32 {
    enum_langs(hmod, ty, name, f, p, false)
}
pub unsafe extern "win64" fn enum_resource_languages_a(
    hmod: usize,
    ty: usize,
    name: usize,
    f: usize,
    p: isize,
) -> i32 {
    enum_langs(hmod, ty, name, f, p, true)
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
        "EnumResourceTypesA" => fn enum_resource_types_a,
        "EnumResourceTypesW" => fn enum_resource_types_w,
        "EnumResourceNamesA" => fn enum_resource_names_a,
        "EnumResourceNamesW" => fn enum_resource_names_w,
        "EnumResourceLanguagesA" => fn enum_resource_languages_a,
        "EnumResourceLanguagesW" => fn enum_resource_languages_w,
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

    /// Section .rsrc minimale : type 10 → noms 5 (une langue, 0x040c) et
    /// 6 (répertoire de langues vide).
    fn section_rsrc() -> Vec<u8> {
        let mut b = vec![0u8; 0x70];
        let w16 = |b: &mut Vec<u8>, o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let w32 = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        // Racine : une entrée par ID, type 10.
        w16(&mut b, 14, 1);
        w32(&mut b, 16, 10);
        w32(&mut b, 20, 0x8000_0018);
        // Noms : 5 et 6.
        w16(&mut b, 0x18 + 14, 2);
        w32(&mut b, 0x28, 5);
        w32(&mut b, 0x2c, 0x8000_0038);
        w32(&mut b, 0x30, 6);
        w32(&mut b, 0x34, 0x8000_0050);
        // Langues du nom 5 : 0x040c → entrée de données en 0x60.
        w16(&mut b, 0x38 + 14, 1);
        w32(&mut b, 0x48, 0x040c);
        w32(&mut b, 0x4c, 0x60);
        // 0x50 : répertoire de langues vide du nom 6.
        // 0x60 : IMAGE_RESOURCE_DATA_ENTRY.
        w32(&mut b, 0x60, 0x1000);
        w32(&mut b, 0x64, 4);
        b
    }

    /// Image PE32+ minimale dont la seule section, .rsrc (RVA 0x1000), contient
    /// `rsrc`, chargée comme données ; rend le handle.
    fn image_de_donnees_avec(rsrc: &[u8], nom: &str) -> usize {
        assert!(rsrc.len() <= 0x200);
        let mut f = vec![0u8; 0x400];
        let w16 = |f: &mut Vec<u8>, o: usize, v: u16| f[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let w32 = |f: &mut Vec<u8>, o: usize, v: u32| f[o..o + 4].copy_from_slice(&v.to_le_bytes());
        f[0..2].copy_from_slice(b"MZ");
        w32(&mut f, 0x3c, 0x40);
        f[0x40..0x44].copy_from_slice(b"PE\0\0");
        let coff = 0x44;
        w16(&mut f, coff, 0x8664);
        w16(&mut f, coff + 2, 1);
        w16(&mut f, coff + 16, 0xf0);
        w16(&mut f, coff + 18, 0x2022);
        let opt = coff + 20;
        w16(&mut f, opt, 0x20b);
        w32(&mut f, opt + 32, 0x1000);
        w32(&mut f, opt + 36, 0x200);
        w32(&mut f, opt + 56, 0x2000);
        w32(&mut f, opt + 60, 0x200);
        w16(&mut f, opt + 68, 2);
        w32(&mut f, opt + 108, 16);
        w32(&mut f, opt + 108 + 4 + DIR_RESOURCE * 8, 0x1000);
        w32(&mut f, opt + 108 + 8 + DIR_RESOURCE * 8, rsrc.len() as u32);
        let sec = opt + 0xf0;
        f[sec..sec + 5].copy_from_slice(b".rsrc");
        w32(&mut f, sec + 8, rsrc.len() as u32);
        w32(&mut f, sec + 12, 0x1000);
        w32(&mut f, sec + 16, 0x200);
        w32(&mut f, sec + 20, 0x200);
        f[0x200..0x200 + rsrc.len()].copy_from_slice(rsrc);
        let dir = std::env::temp_dir().join(format!(
            "winlux-test-ressources-{}-{nom}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("module.dll");
        std::fs::write(&path, &f).unwrap();
        loader::load_data(&path, 2).unwrap()
    }

    /// #2394 : chaque niveau de l'arbre manqué donne son propre code.
    #[test]
    fn find_resource_code_d_erreur_par_niveau() {
        let r = section_rsrc();
        assert_eq!(walk_resource_tree(&r, 10, 5, false, false, None), Ok(0x60));
        assert_eq!(
            walk_resource_tree(&r, 3, 5, false, false, None),
            Err(ERROR_RESOURCE_TYPE_NOT_FOUND)
        );
        assert_eq!(
            walk_resource_tree(&r, 10, 7, false, false, None),
            Err(ERROR_RESOURCE_NAME_NOT_FOUND)
        );
        assert_eq!(
            walk_resource_tree(&r, 10, 6, false, false, None),
            Err(ERROR_RESOURCE_LANG_NOT_FOUND)
        );
    }

    /// #2395 : « #5 » (A comme W) désigne l'identifiant entier 5 ; « #x » et
    /// « #70000 » restent des noms.
    #[test]
    fn find_resource_forme_diese_nnn() {
        let w: Vec<u16> = "#5\0".encode_utf16().collect();
        let a = b"#5\0";
        assert!(matches!(
            unsafe { arg_key(w.as_ptr() as usize, false) },
            ResKey::Id(5)
        ));
        assert!(matches!(
            unsafe { arg_key(a.as_ptr() as usize, true) },
            ResKey::Id(5)
        ));
        let x: Vec<u16> = "#x\0".encode_utf16().collect();
        assert!(matches!(
            unsafe { arg_key(x.as_ptr() as usize, false) },
            ResKey::Name(_)
        ));
        let big = b"#70000\0";
        assert!(matches!(
            unsafe { arg_key(big.as_ptr() as usize, true) },
            ResKey::Name(_)
        ));
        // De bout en bout : « #5 » trouve la ressource d'identifiant 5 d'une
        // image chargée comme données, « #7 » rend « nom absent ».
        let _teb = crate::process::test_teb();
        let h = image_de_donnees_avec(&section_rsrc(), "diese");
        let base = h & !loader::DATA_HANDLE_BITS;
        let w7: Vec<u16> = "#7\0".encode_utf16().collect();
        let r = unsafe { find_resource_w(h, w.as_ptr(), 10 as *const u16) };
        assert_eq!(r, base + 0x1000 + 0x60);
        let r = unsafe { find_resource_a(h, a.as_ptr(), 10 as *const u8) };
        assert_eq!(r, base + 0x1000 + 0x60);
        assert_eq!(
            unsafe { find_resource_w(h, w7.as_ptr(), 10 as *const u16) },
            0
        );
        assert_eq!(crate::process::last_error(), ERROR_RESOURCE_NAME_NOT_FOUND);
    }

    /// Type 10, nom 5, en deux langues rangées en-US (0x409, données en
    /// 0x50) puis fr-FR (0x40c, données en 0x60).
    fn section_rsrc_deux_langues() -> Vec<u8> {
        let mut b = vec![0u8; 0x70];
        let w16 = |b: &mut Vec<u8>, o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let w32 = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        w16(&mut b, 14, 1);
        w32(&mut b, 16, 10);
        w32(&mut b, 20, 0x8000_0018);
        w16(&mut b, 0x18 + 14, 1);
        w32(&mut b, 0x28, 5);
        w32(&mut b, 0x2c, 0x8000_0030);
        w16(&mut b, 0x30 + 14, 2);
        w32(&mut b, 0x40, 0x409);
        w32(&mut b, 0x44, 0x50);
        w32(&mut b, 0x48, 0x40c);
        w32(&mut b, 0x4c, 0x60);
        w32(&mut b, 0x50, 0x1000);
        w32(&mut b, 0x54, 2);
        w32(&mut b, 0x60, 0x1010);
        w32(&mut b, 0x64, 3);
        b
    }

    /// #2396 : FindResourceEx rend la langue demandée, 1815 si elle manque ;
    /// la langue neutre suit celle du thread. FindResource garde la première.
    #[test]
    fn find_resource_ex_langue_demandee() {
        let r = section_rsrc_deux_langues();
        assert_eq!(walk_resource_tree(&r, 10, 5, false, false, None), Ok(0x50));
        assert_eq!(
            walk_resource_tree(&r, 10, 5, false, false, Some(0x40c)),
            Ok(0x60)
        );
        assert_eq!(
            walk_resource_tree(&r, 10, 5, false, false, Some(0x409)),
            Ok(0x50)
        );
        assert_eq!(
            walk_resource_tree(&r, 10, 5, false, false, Some(0x407)),
            Err(ERROR_RESOURCE_LANG_NOT_FOUND)
        );
        let ids = [(0x409u16, 1usize), (0x40c, 2)];
        assert_eq!(choose_language(&ids, 0, 0x40c), Some(2));
        assert_eq!(choose_language(&ids, 0, 0x407), Some(1));
        assert_eq!(choose_language(&[(0x409, 1), (0, 3)], 0, 0x407), Some(3));
        assert_eq!(choose_language(&ids, 0x40c, 0x409), Some(2));
        assert_eq!(choose_language(&ids, 0x410, 0x409), None);

        // De bout en bout, sur une image chargée comme données.
        let _teb = crate::process::test_teb();
        let h = image_de_donnees_avec(&r, "langues");
        let base = h & !loader::DATA_HANDLE_BITS;
        let fr = unsafe { find_resource_ex_w(h, 10 as *const u16, 5 as *const u16, 0x40c) };
        assert_eq!(fr, base + 0x1000 + 0x60);
        assert_eq!(unsafe { sizeof_resource(h, fr) }, 3);
        let en = unsafe { find_resource_ex_a(h, 10 as *const u8, 5 as *const u8, 0x409) };
        assert_eq!(en, base + 0x1000 + 0x50);
        let de = unsafe { find_resource_ex_w(h, 10 as *const u16, 5 as *const u16, 0x407) };
        assert_eq!(de, 0);
        assert_eq!(crate::process::last_error(), ERROR_RESOURCE_LANG_NOT_FOUND);
    }

    /// #2394 : un module sans section de ressources rend 1812, et
    /// FindResourceW le laisse dans GetLastError.
    #[test]
    fn find_resource_sans_section_rend_1812() {
        let _teb = crate::process::test_teb();
        let base = 0x7ff0_0000_0000usize;
        assert_eq!(
            lookup_resource(base, 10, 5, false, false, None),
            Err(ERROR_RESOURCE_DATA_NOT_FOUND)
        );
        let h = unsafe { find_resource_w(base, 5 as *const u16, 10 as *const u16) };
        assert_eq!(h, 0);
        assert_eq!(crate::process::last_error(), ERROR_RESOURCE_DATA_NOT_FOUND);
    }
}
