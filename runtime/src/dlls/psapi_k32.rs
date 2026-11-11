//! Fonctions « K32… » de kernel32 que psapi.dll fait suivre et qui
//! manquaient (#232, relevées par WinSCP x86) : EnumProcesses,
//! GetModuleBaseNameA, GetModuleFileNameExA, GetMappedFileName(A/W),
//! GetProcessImageFileName(A/W), pilotes de périphérique, ensemble de
//! travail et fichiers d'échange.
//!
//! Les identifiants de processus Windows sont ceux de Linux (GetProcessId
//! les rend tels quels). Un compte standard ne voit pas les pilotes du
//! noyau : la liste est vide. Les chemins « natifs » ont la forme
//! `\Device\HarddiskVolume1\…`, comme QueryFullProcessImageNameW avec
//! PROCESS_NAME_NATIVE.
//!
//! Référence : documentation publique de psapi.h.
#![allow(non_snake_case)]

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_ADDRESS: u32 = 487;
const ERROR_BAD_LENGTH: u32 = 24;

fn set_error(code: u32) {
    crate::process::set_last_error(code);
}

fn is_current(h: usize) -> bool {
    h == usize::MAX || crate::dlls::kernel32::is_current_process_handle(h)
}

/// Identifiants des processus visibles (/proc), par ordre croissant.
fn process_ids() -> Vec<u32> {
    let mut ids: Vec<u32> = std::fs::read_dir("/proc")
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().to_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    ids.sort_unstable();
    ids
}

/// `EnumProcesses(ids, cb, needed)` : *needed* reçoit le nombre d'octets
/// écrits (jamais plus que *cb*), comme sous Windows.
pub unsafe extern "win64" fn K32EnumProcesses(ids: *mut u32, cb: u32, needed: *mut u32) -> i32 {
    if ids.is_null() || needed.is_null() {
        set_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let list = process_ids();
    let n = list.len().min(cb as usize / 4);
    std::ptr::copy_nonoverlapping(list.as_ptr(), ids, n);
    *needed = (n * 4) as u32;
    1
}

/// Copie ANSI terminée par un zéro, tronquée à la taille du tampon ;
/// rend le nombre de caractères copiés.
unsafe fn copy_ansi(text: &str, buf: *mut u8, size: u32) -> u32 {
    if buf.is_null() || size == 0 {
        set_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    let a = crate::dlls::encode_ansi(text);
    let n = a.len().min(size as usize - 1);
    std::ptr::copy_nonoverlapping(a.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

unsafe fn copy_wide(text: &str, buf: *mut u16, size: u32) -> u32 {
    if buf.is_null() || size == 0 {
        set_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    let n = w.len().min(size as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

/// Appelle une fonction « W » à tampon et rend le texte obtenu.
unsafe fn wide_text(f: impl FnOnce(*mut u16, u32) -> u32) -> Option<String> {
    let mut w = vec![0u16; 1024];
    let n = f(w.as_mut_ptr(), w.len() as u32) as usize;
    (n != 0).then(|| String::from_utf16_lossy(&w[..n]))
}

pub unsafe extern "win64" fn K32GetModuleBaseNameA(
    process: usize,
    module: usize,
    buf: *mut u8,
    size: u32,
) -> u32 {
    match wide_text(|p, n| crate::dlls::kernel32_node::K32GetModuleBaseNameW(process, module, p, n))
    {
        Some(t) => copy_ansi(&t, buf, size),
        None => 0,
    }
}

pub unsafe extern "win64" fn K32GetModuleFileNameExA(
    process: usize,
    module: usize,
    buf: *mut u8,
    size: u32,
) -> u32 {
    match wide_text(|p, n| {
        crate::dlls::kernel32_node::K32GetModuleFileNameExW(process, module, p, n)
    }) {
        Some(t) => copy_ansi(&t, buf, size),
        None => 0,
    }
}

/// `C:\dossier\x.dll` → `\Device\HarddiskVolume1\dossier\x.dll`.
fn native_path(win: &str) -> String {
    match win.get(1..2) {
        Some(":") => format!("\\Device\\HarddiskVolume1{}", win.get(2..).unwrap_or("")),
        _ => win.to_string(),
    }
}

/// Fichier projeté à l'adresse `addr` du processus courant, en chemin
/// natif ; None si la page n'est pas une projection de fichier.
fn mapped_file(addr: usize) -> Option<String> {
    // Image PE chargée par Winlux : son chemin Windows.
    if let Some(m) = crate::loader::find_module_by_addr(addr) {
        if !m.path.as_os_str().is_empty() {
            return Some(native_path(&crate::paths::to_windows(&m.path)));
        }
    }
    let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let (lo, hi) = parts.next()?.split_once('-')?;
        let (lo, hi) = (
            usize::from_str_radix(lo, 16).ok()?,
            usize::from_str_radix(hi, 16).ok()?,
        );
        if addr < lo || addr >= hi {
            return None;
        }
        let path = parts.nth(4)?;
        path.starts_with('/')
            .then(|| native_path(&crate::paths::to_windows(std::path::Path::new(path))))
    })
}

pub unsafe extern "win64" fn K32GetMappedFileNameW(
    process: usize,
    addr: usize,
    buf: *mut u16,
    size: u32,
) -> u32 {
    if !is_current(process) {
        set_error(ERROR_ACCESS_DENIED);
        return 0;
    }
    match mapped_file(addr) {
        Some(p) => copy_wide(&p, buf, size),
        None => {
            set_error(ERROR_INVALID_ADDRESS);
            0
        }
    }
}

pub unsafe extern "win64" fn K32GetMappedFileNameA(
    process: usize,
    addr: usize,
    buf: *mut u8,
    size: u32,
) -> u32 {
    match wide_text(|p, n| K32GetMappedFileNameW(process, addr, p, n)) {
        Some(t) => copy_ansi(&t, buf, size),
        None => 0,
    }
}

pub unsafe extern "win64" fn K32GetProcessImageFileNameW(
    process: usize,
    buf: *mut u16,
    size: u32,
) -> u32 {
    let mut w = vec![0u16; 1024];
    let mut n = w.len() as u32;
    if crate::dlls::win32_basics::query_full_process_image_name_w(
        process,
        1,
        w.as_mut_ptr(),
        &mut n,
    ) == 0
    {
        return 0;
    }
    copy_wide(&String::from_utf16_lossy(&w[..n as usize]), buf, size)
}

pub unsafe extern "win64" fn K32GetProcessImageFileNameA(
    process: usize,
    buf: *mut u8,
    size: u32,
) -> u32 {
    match wide_text(|p, n| K32GetProcessImageFileNameW(process, p, n)) {
        Some(t) => copy_ansi(&t, buf, size),
        None => 0,
    }
}

/// Pilotes du noyau : invisibles pour un compte standard, liste vide.
pub unsafe extern "win64" fn K32EnumDeviceDrivers(
    _drivers: *mut usize,
    _cb: u32,
    needed: *mut u32,
) -> i32 {
    if needed.is_null() {
        set_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *needed = 0;
    1
}

pub unsafe extern "win64" fn K32GetDeviceDriverBaseNameW(
    _base: usize,
    _buf: *mut u16,
    _size: u32,
) -> u32 {
    set_error(ERROR_INVALID_PARAMETER);
    0
}

pub unsafe extern "win64" fn K32GetDeviceDriverBaseNameA(
    _base: usize,
    _buf: *mut u8,
    _size: u32,
) -> u32 {
    set_error(ERROR_INVALID_PARAMETER);
    0
}

/// `EmptyWorkingSet` : Linux gère seul la mémoire résidente.
pub unsafe extern "win64" fn K32EmptyWorkingSet(process: usize) -> i32 {
    if !is_current(process) {
        set_error(ERROR_ACCESS_DENIED);
        return 0;
    }
    1
}

pub unsafe extern "win64" fn K32InitializeProcessForWsWatch(process: usize) -> i32 {
    if !is_current(process) {
        set_error(ERROR_ACCESS_DENIED);
        return 0;
    }
    1
}

/// `GetWsChanges` : aucun défaut de page relevé, la liste ne contient que
/// l'entrée finale à zéro (deux pointeurs).
pub unsafe extern "win64" fn K32GetWsChanges(process: usize, info: *mut u8, cb: u32) -> i32 {
    if !is_current(process) {
        set_error(ERROR_ACCESS_DENIED);
        return 0;
    }
    let entry = 2 * crate::wow32::pointer_size();
    if info.is_null() || (cb as usize) < entry {
        set_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::write_bytes(info, 0, entry);
    1
}

/// `GetWsChangesEx` : idem, entrées de trois pointeurs ; *cb* reçoit la
/// taille utilisée.
pub unsafe extern "win64" fn K32GetWsChangesEx(process: usize, info: *mut u8, cb: *mut u32) -> i32 {
    if !is_current(process) {
        set_error(ERROR_ACCESS_DENIED);
        return 0;
    }
    let entry = 3 * crate::wow32::pointer_size();
    if info.is_null() || cb.is_null() || (*cb as usize) < entry {
        if !cb.is_null() {
            *cb = entry as u32;
        }
        set_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::write_bytes(info, 0, entry);
    *cb = entry as u32;
    1
}

/// Pages résidentes du processus (mincore sur chaque projection lisible),
/// sous 4 Go seulement pour un programme x86.
fn resident_pages() -> Vec<usize> {
    let limit = if crate::wow32::pointer_size() == 4 {
        1usize << 32
    } else {
        usize::MAX
    };
    let Ok(maps) = std::fs::read_to_string("/proc/self/maps") else {
        return Vec::new();
    };
    let mut pages = Vec::new();
    for line in maps.lines() {
        let mut parts = line.split_whitespace();
        let Some((lo, hi)) = parts.next().and_then(|r| r.split_once('-')) else {
            continue;
        };
        let (Ok(lo), Ok(hi)) = (usize::from_str_radix(lo, 16), usize::from_str_radix(hi, 16))
        else {
            continue;
        };
        if !parts.next().is_some_and(|perm| perm.starts_with('r')) || hi > limit {
            continue;
        }
        let count = (hi - lo) / 4096;
        let mut vec = vec![0u8; count];
        if unsafe { libc::mincore(lo as *mut libc::c_void, hi - lo, vec.as_mut_ptr()) } != 0 {
            continue;
        }
        pages.extend(
            (0..count)
                .filter(|&i| vec[i] & 1 != 0)
                .map(|i| lo + i * 4096),
        );
    }
    pages
}

/// `QueryWorkingSet` : PSAPI_WORKING_SET_INFORMATION, NumberOfEntries puis
/// une entrée par page résidente (adresse de page, drapeaux à zéro : page
/// privée). Tampon trop petit : NumberOfEntries rempli et ERROR_BAD_LENGTH,
/// comme Windows, pour que l'appelant agrandisse son tampon.
pub unsafe extern "win64" fn K32QueryWorkingSet(process: usize, info: *mut u8, cb: u32) -> i32 {
    if !is_current(process) {
        set_error(ERROR_ACCESS_DENIED);
        return 0;
    }
    let ptr = crate::wow32::pointer_size();
    if info.is_null() || (cb as usize) < ptr {
        set_error(ERROR_BAD_LENGTH);
        return 0;
    }
    let pages = resident_pages();
    crate::wow32::write_ptr(info as usize, pages.len());
    if pages.len() > (cb as usize - ptr) / ptr {
        set_error(ERROR_BAD_LENGTH);
        return 0;
    }
    for (i, &page) in pages.iter().enumerate() {
        crate::wow32::write_ptr(info as usize + ptr * (i + 1), page);
    }
    1
}

/// `EnumPageFilesW/A(rappel, contexte)` : aucun fichier d'échange Windows ;
/// le rappel n'est pas appelé.
pub unsafe extern "win64" fn K32EnumPageFilesW(_callback: usize, _context: usize) -> i32 {
    1
}

pub unsafe extern "win64" fn K32EnumPageFilesA(_callback: usize, _context: usize) -> i32 {
    1
}

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "K32EnumProcesses" => fn K32EnumProcesses,
        "K32GetModuleBaseNameA" => fn K32GetModuleBaseNameA,
        "K32GetModuleFileNameExA" => fn K32GetModuleFileNameExA,
        "K32GetMappedFileNameW" => fn K32GetMappedFileNameW,
        "K32GetMappedFileNameA" => fn K32GetMappedFileNameA,
        "K32GetProcessImageFileNameW" => fn K32GetProcessImageFileNameW,
        "K32GetProcessImageFileNameA" => fn K32GetProcessImageFileNameA,
        "K32EnumDeviceDrivers" => fn K32EnumDeviceDrivers,
        "K32GetDeviceDriverBaseNameW" => fn K32GetDeviceDriverBaseNameW,
        "K32GetDeviceDriverBaseNameA" => fn K32GetDeviceDriverBaseNameA,
        "K32GetDeviceDriverFileNameW" => fn K32GetDeviceDriverBaseNameW,
        "K32GetDeviceDriverFileNameA" => fn K32GetDeviceDriverBaseNameA,
        "K32EmptyWorkingSet" => fn K32EmptyWorkingSet,
        "K32InitializeProcessForWsWatch" => fn K32InitializeProcessForWsWatch,
        "K32GetWsChanges" => fn K32GetWsChanges,
        "K32GetWsChangesEx" => fn K32GetWsChangesEx,
        "K32QueryWorkingSet" => fn K32QueryWorkingSet,
        "K32EnumPageFilesW" => fn K32EnumPageFilesW,
        "K32EnumPageFilesA" => fn K32EnumPageFilesA,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_des_processus_et_chemins() {
        let _teb = crate::process::test_teb();
        unsafe {
            let mut ids = [0u32; 4096];
            let mut needed = 0;
            assert_eq!(K32EnumProcesses(ids.as_mut_ptr(), 4096 * 4, &mut needed), 1);
            let n = needed as usize / 4;
            assert!(n >= 1);
            assert!(ids[..n].contains(&std::process::id()));
            // Tampon trop petit : tronqué, needed borné par cb.
            assert_eq!(K32EnumProcesses(ids.as_mut_ptr(), 4, &mut needed), 1);
            assert_eq!(needed, 4);
            // Le binaire de test est une projection de fichier.
            let mut w = [0u16; 512];
            let addr = liste_des_processus_et_chemins as *const () as usize;
            let len = K32GetMappedFileNameW(usize::MAX, addr, w.as_mut_ptr(), 512);
            assert!(len > 0);
            let p = String::from_utf16_lossy(&w[..len as usize]);
            assert!(p.starts_with("\\Device\\") || p.starts_with("\\\\"), "{p}");
            // Une page anonyme n'est pas un fichier.
            let heap = Box::new([0u8; 64]);
            assert_eq!(
                K32GetMappedFileNameW(usize::MAX, heap.as_ptr() as usize, w.as_mut_ptr(), 512),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_INVALID_ADDRESS);
            assert_eq!(
                native_path("C:\\t\\a.dll"),
                "\\Device\\HarddiskVolume1\\t\\a.dll"
            );
            let mut needed = 99;
            assert_eq!(
                K32EnumDeviceDrivers(std::ptr::null_mut(), 0, &mut needed),
                1
            );
            assert_eq!(needed, 0);
            let mut ws = [0usize; 1];
            assert_eq!(K32QueryWorkingSet(usize::MAX, ws.as_mut_ptr().cast(), 8), 0);
            assert_eq!(crate::process::last_error(), ERROR_BAD_LENGTH);
            assert!(ws[0] > 0);
            // Les autres tests, en parallèle, font grandir l'ensemble de
            // travail entre deux appels : on recommence avec le nombre rendu,
            // comme un programme le fait sur ERROR_BAD_LENGTH (#1693).
            let mut want = ws[0];
            let mut done = false;
            for _ in 0..8 {
                let mut big = vec![0usize; want * 2 + 4096];
                let cb = (big.len() * 8) as u32;
                if K32QueryWorkingSet(usize::MAX, big.as_mut_ptr().cast(), cb) == 1 {
                    assert!(big[0] > 0 && big[1] % 4096 == 0);
                    done = true;
                    break;
                }
                assert_eq!(crate::process::last_error(), ERROR_BAD_LENGTH);
                want = big[0];
            }
            assert!(
                done,
                "ensemble de travail toujours plus grand que le tampon"
            );
        }
    }
}
