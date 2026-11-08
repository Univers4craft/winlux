//! Fonctions kernel32 de toujours, encore importées par les programmes
//! Delphi / C++Builder et MSVC 32 bits (WinSCP, #486) : table d'atomes,
//! SetHandleCount, IsBadReadPtr, OpenMutexW, GetSystemTimes…

use std::collections::HashMap;
use std::sync::Mutex;

use crate::process::set_last_error;

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_HANDLE: u32 = 6;
const MAXINTATOM: usize = 0xC000;

// ---------- Atomes ----------

/// Table d'atomes (une seule pour les tables locale et globale : un seul
/// programme par processus). Atome chaîne = 0xC000 + rang.
#[derive(Default)]
struct Atoms {
    names: Vec<Option<(String, u32)>>,
    by_name: HashMap<String, u16>,
}

static ATOMS: Mutex<Option<Atoms>> = Mutex::new(None);

/// `#123` ou MAKEINTATOM(123) : atome entier, rendu tel quel.
fn integer_atom(name: &str) -> Option<u16> {
    let n: u32 = name.strip_prefix('#')?.parse().ok()?;
    (n > 0 && (n as usize) < MAXINTATOM).then_some(n as u16)
}

enum Name {
    Int(u16),
    Text(String),
}

unsafe fn name_of(p: usize, wide: bool) -> Option<Name> {
    if p == 0 {
        return None;
    }
    if p < 0x10000 {
        return (p > 0 && p < MAXINTATOM).then_some(Name::Int(p as u16));
    }
    let s = if wide {
        super::wstr(p as *const u16)
    } else {
        super::cstr(p as *const u8)
    };
    if s.is_empty() || s.chars().count() > 255 {
        return None;
    }
    Some(match integer_atom(&s) {
        Some(a) => Name::Int(a),
        None => Name::Text(s),
    })
}

fn add_atom(p: usize, wide: bool) -> u16 {
    let Some(name) = (unsafe { name_of(p, wide) }) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let text = match name {
        Name::Int(a) => return a,
        Name::Text(t) => t,
    };
    let key = text.to_lowercase();
    let mut g = super::lock(&ATOMS);
    let t = g.get_or_insert_with(Atoms::default);
    if let Some(&a) = t.by_name.get(&key) {
        if let Some(Some((_, count))) = t.names.get_mut(a as usize - MAXINTATOM) {
            *count += 1;
        }
        return a;
    }
    let slot = t
        .names
        .iter()
        .position(Option::is_none)
        .unwrap_or(t.names.len());
    if slot >= 0x3FFF {
        set_last_error(8); // ERROR_NOT_ENOUGH_MEMORY
        return 0;
    }
    if slot == t.names.len() {
        t.names.push(None);
    }
    t.names[slot] = Some((text, 1));
    let atom = (MAXINTATOM + slot) as u16;
    t.by_name.insert(key, atom);
    atom
}

fn find_atom(p: usize, wide: bool) -> u16 {
    let Some(name) = (unsafe { name_of(p, wide) }) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let text = match name {
        Name::Int(a) => return a,
        Name::Text(t) => t,
    };
    let g = super::lock(&ATOMS);
    match g.as_ref().and_then(|t| t.by_name.get(&text.to_lowercase())) {
        Some(&a) => a,
        None => {
            set_last_error(ERROR_FILE_NOT_FOUND);
            0
        }
    }
}

fn delete_atom(atom: u16) -> u16 {
    if (atom as usize) < MAXINTATOM {
        return 0;
    }
    let mut g = super::lock(&ATOMS);
    let Some(t) = g.as_mut() else {
        set_last_error(ERROR_INVALID_HANDLE);
        return atom;
    };
    let i = atom as usize - MAXINTATOM;
    let gone = match t.names.get_mut(i) {
        Some(Some((_, count))) => {
            *count -= 1;
            *count == 0
        }
        _ => {
            set_last_error(ERROR_INVALID_HANDLE);
            return atom;
        }
    };
    if gone {
        if let Some((name, _)) = t.names[i].take() {
            t.by_name.remove(&name.to_lowercase());
        }
    }
    0
}

fn atom_name(atom: u16) -> Option<String> {
    if atom == 0 {
        return None;
    }
    if (atom as usize) < MAXINTATOM {
        return Some(format!("#{atom}"));
    }
    let g = super::lock(&ATOMS);
    g.as_ref()?
        .names
        .get(atom as usize - MAXINTATOM)?
        .as_ref()
        .map(|(n, _)| n.clone())
}

pub extern "win64" fn add_atom_a(name: usize) -> u16 {
    add_atom(name, false)
}
pub extern "win64" fn add_atom_w(name: usize) -> u16 {
    add_atom(name, true)
}
pub extern "win64" fn find_atom_a(name: usize) -> u16 {
    find_atom(name, false)
}
pub extern "win64" fn find_atom_w(name: usize) -> u16 {
    find_atom(name, true)
}
pub extern "win64" fn delete_atom_k(atom: u16) -> u16 {
    delete_atom(atom)
}
/// GetAtomName : longueur copiée (sans le zéro), 0 si l'atome est inconnu.
pub unsafe extern "win64" fn get_atom_name_a(atom: u16, buf: *mut u8, size: i32) -> u32 {
    let Some(name) = atom_name(atom) else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if buf.is_null() || size <= 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    super::put_text_a(&name, buf, size as u32)
}
pub unsafe extern "win64" fn get_atom_name_w(atom: u16, buf: *mut u16, size: i32) -> u32 {
    let Some(name) = atom_name(atom) else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    if buf.is_null() || size <= 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let w: Vec<u16> = name.encode_utf16().collect();
    let n = w.len().min(size as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}
pub extern "win64" fn init_atom_table(_size: u32) -> i32 {
    1
}

// ---------- Divers ----------

/// SetHandleCount : sans effet depuis Win32, rend le nombre demandé.
pub extern "win64" fn set_handle_count(n: u32) -> u32 {
    n
}

/// IsBadReadPtr / IsBadWritePtr : vrai pour un pointeur nul (taille non
/// nulle) ou une plage que le processus ne peut pas lire / écrire.
fn bad_range(p: usize, len: usize, write: bool) -> i32 {
    if len == 0 {
        return 0;
    }
    if p == 0 || p.checked_add(len).is_none() {
        return 1;
    }
    let Ok(maps) = std::fs::read_to_string("/proc/self/maps") else {
        return 0;
    };
    let mut at = p;
    let end = p + len;
    while at < end {
        let region = maps.lines().find_map(|l| {
            let mut f = l.split_whitespace();
            let (a, b) = f.next()?.split_once('-')?;
            let (a, b) = (
                usize::from_str_radix(a, 16).ok()?,
                usize::from_str_radix(b, 16).ok()?,
            );
            let perms = f.next()?.as_bytes();
            (a <= at && at < b).then_some((b, perms[0] == b'r', perms[1] == b'w'))
        });
        match region {
            Some((b, r, w)) if r && (!write || w) => at = b,
            _ => return 1,
        }
    }
    0
}
pub extern "win64" fn is_bad_read_ptr(p: usize, len: usize) -> i32 {
    bad_range(p, len, false)
}
pub extern "win64" fn is_bad_write_ptr(p: usize, len: usize) -> i32 {
    bad_range(p, len, true)
}
pub unsafe extern "win64" fn is_bad_string_ptr_a(p: *const u8, max: usize) -> i32 {
    if p.is_null() {
        return (max != 0) as i32;
    }
    let mut i = 0;
    while i < max {
        if bad_range(p as usize + i, 1, false) != 0 {
            return 1;
        }
        if *p.add(i) == 0 {
            return 0;
        }
        i += 1;
    }
    0
}
pub unsafe extern "win64" fn is_bad_string_ptr_w(p: *const u16, max: usize) -> i32 {
    if p.is_null() {
        return (max != 0) as i32;
    }
    let mut i = 0;
    while i < max {
        if bad_range(p as usize + 2 * i, 2, false) != 0 {
            return 1;
        }
        if *p.add(i) == 0 {
            return 0;
        }
        i += 1;
    }
    0
}

/// OpenMutexW : même chose qu'OpenMutexA, nom converti.
pub unsafe extern "win64" fn open_mutex_w(access: u32, inherit: i32, name: *const u16) -> usize {
    if name.is_null() {
        return super::kernel32::open_mutex_a(access, inherit, std::ptr::null());
    }
    let mut a = super::encode_ansi(&super::wstr(name));
    a.push(0);
    super::kernel32::open_mutex_a(access, inherit, a.as_ptr())
}

/// GetSystemTimes : temps processeur cumulés (inactif, noyau, utilisateur)
/// en unités de 100 ns, lus dans /proc/stat. Comme Windows, le temps noyau
/// inclut le temps inactif.
pub unsafe extern "win64" fn get_system_times(
    idle: *mut u64,
    kernel: *mut u64,
    user: *mut u64,
) -> i32 {
    let text = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let Some(line) = text.lines().find(|l| l.starts_with("cpu ")) else {
        set_last_error(31);
        return 0;
    };
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|x| x.parse().ok())
        .collect();
    let at = |i: usize| v.get(i).copied().unwrap_or(0);
    let hz = libc::sysconf(libc::_SC_CLK_TCK).max(1) as u64;
    let to100ns = |ticks: u64| ticks * 10_000_000 / hz;
    let idle_t = to100ns(at(3) + at(4));
    let sys = to100ns(at(2) + at(5) + at(6));
    let usr = to100ns(at(0) + at(1));
    for (p, val) in [(idle, idle_t), (kernel, sys + idle_t), (user, usr)] {
        if !p.is_null() {
            p.write_unaligned(val);
        }
    }
    1
}

pub fn exports() -> super::Exports {
    crate::exports! {
        "GlobalAddAtomA" => fn add_atom_a,
        "GlobalAddAtomW" => fn add_atom_w,
        "AddAtomA" => fn add_atom_a,
        "AddAtomW" => fn add_atom_w,
        "GlobalFindAtomA" => fn find_atom_a,
        "GlobalFindAtomW" => fn find_atom_w,
        "FindAtomA" => fn find_atom_a,
        "FindAtomW" => fn find_atom_w,
        "GlobalDeleteAtom" => fn delete_atom_k,
        "DeleteAtom" => fn delete_atom_k,
        "GlobalGetAtomNameA" => fn get_atom_name_a,
        "GlobalGetAtomNameW" => fn get_atom_name_w,
        "GetAtomNameA" => fn get_atom_name_a,
        "GetAtomNameW" => fn get_atom_name_w,
        "InitAtomTable" => fn init_atom_table,
        "SetHandleCount" => fn set_handle_count,
        "IsBadReadPtr" => fn is_bad_read_ptr,
        "IsBadWritePtr" => fn is_bad_write_ptr,
        "IsBadCodePtr" => fn is_bad_read_ptr,
        "IsBadStringPtrA" => fn is_bad_string_ptr_a,
        "IsBadStringPtrW" => fn is_bad_string_ptr_w,
        "OpenMutexW" => fn open_mutex_w,
        "OpenJobObjectA" => fn super::jobs::open_job_object_a,
        "OpenJobObjectW" => fn super::jobs::open_job_object_w,
        "GetSystemTimes" => fn get_system_times,
        "GetStringTypeA" => fn super::common_apps::get_string_type_ex_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomes_chaines_et_entiers() {
        let a = add_atom_a(c"WinluxAtomeTest".as_ptr() as usize);
        assert!(a as usize >= MAXINTATOM);
        assert_eq!(add_atom_a(c"winluxatometest".as_ptr() as usize), a);
        assert_eq!(find_atom_a(c"WINLUXATOMETEST".as_ptr() as usize), a);
        let mut buf = [0u8; 32];
        let n = unsafe { get_atom_name_a(a, buf.as_mut_ptr(), 32) };
        assert_eq!(&buf[..n as usize], b"WinluxAtomeTest");
        assert_eq!(delete_atom_k(a), 0);
        assert_eq!(find_atom_a(c"WinluxAtomeTest".as_ptr() as usize), a);
        assert_eq!(delete_atom_k(a), 0);
        assert_eq!(find_atom_a(c"WinluxAtomeTest".as_ptr() as usize), 0);
        assert_eq!(add_atom_a(c"#42".as_ptr() as usize), 42);
        assert_eq!(add_atom_a(42), 42);
    }

    #[test]
    fn pointeurs_douteux() {
        let x = [0u8; 16];
        assert_eq!(is_bad_read_ptr(x.as_ptr() as usize, 16), 0);
        assert_eq!(is_bad_read_ptr(0, 4), 1);
        assert_eq!(is_bad_read_ptr(0, 0), 0);
        assert_eq!(is_bad_write_ptr(is_bad_read_ptr as usize, 1), 1);
    }
}
