//! Fonctions de kernel32 (et de l'ensemble api-ms-win-core-path) importées
//! par python312.dll et absentes jusque-là (#1383) : PathCchSkipRoot,
//! PathCchCombineEx, CopyFile2, SetWaitableTimerEx, et les instantanés de
//! processus (Pss*), qui rendent « non pris en charge ».

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER).
const E_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
const ERROR_CALL_NOT_IMPLEMENTED: u32 = 120;

fn hresult_from_win32(e: u32) -> i32 {
    if e == 0 {
        S_OK
    } else {
        (0x8007_0000 | (e & 0xFFFF)) as i32
    }
}

/// Longueur (en unités UTF-16) de la racine de `p` : « C:\ », « C: »,
/// « \ », « \\serveur\partage\ », « \\?\C:\ », « \\?\UNC\serveur\partage\ ».
/// None si le chemin n'a pas de racine.
fn root_len(p: &[u16]) -> Option<usize> {
    let sep = |c: u16| c == b'\\' as u16 || c == b'/' as u16;
    let drive = |s: &[u16]| {
        s.len() >= 2 && s[1] == b':' as u16 && (s[0] as u8).is_ascii_alphabetic() && s[0] < 128
    };
    let after_components = |s: &[u16], start: usize, n: usize| {
        // Fin de `n` composants séparés par « \ » à partir de `start`,
        // séparateur final compris.
        let mut i = start;
        for k in 0..n {
            while i < s.len() && !sep(s[i]) {
                i += 1;
            }
            if i < s.len() && (k + 1 < n || sep(s[i])) {
                i += 1;
            }
        }
        i
    };
    let long = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    if p.starts_with(&long) {
        let rest = &p[4..];
        let unc: Vec<u16> = "UNC\\".encode_utf16().collect();
        if rest.len() >= 4
            && rest[..4]
                .iter()
                .zip(&unc)
                .all(|(a, b)| (*a as u8).eq_ignore_ascii_case(&(*b as u8)))
        {
            return Some(after_components(p, 8, 2));
        }
        if drive(rest) {
            return Some(if rest.len() > 2 && sep(rest[2]) { 7 } else { 6 });
        }
        return None;
    }
    if p.len() >= 2 && sep(p[0]) && sep(p[1]) {
        return Some(after_components(p, 2, 2));
    }
    if drive(p) {
        return Some(if p.len() > 2 && sep(p[2]) { 3 } else { 2 });
    }
    if !p.is_empty() && sep(p[0]) {
        return Some(1);
    }
    None
}

unsafe fn units(p: *const u16) -> Vec<u16> {
    let n = crate::printf::wide_len(p);
    std::slice::from_raw_parts(p, n).to_vec()
}

/// `PathCchSkipRoot(chemin, &fin_de_racine)`.
pub unsafe extern "win64" fn path_cch_skip_root(path: *const u16, root_end: usize) -> i32 {
    if path.is_null() || root_end == 0 {
        return E_INVALIDARG;
    }
    match root_len(&units(path)) {
        Some(n) => {
            crate::wow32::write_ptr(root_end, path.add(n) as usize);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

/// `PathCchCombineEx(sortie, taille, chemin, suite, drapeaux)` : comme
/// PathCombine (résultat canonique), avec la taille du tampon.
pub unsafe extern "win64" fn path_cch_combine_ex(
    out: *mut u16,
    cch: usize,
    path: *const u16,
    more: *const u16,
    _flags: u32,
) -> i32 {
    if out.is_null() || cch == 0 {
        return E_INVALIDARG;
    }
    let buf = super::shlwapi::path_combine_units(path, more);
    let n = buf.len();
    if n + 1 > cch {
        *out = 0;
        return E_INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(buf.as_ptr(), out, n);
    *out.add(n) = 0;
    S_OK
}

/// `CopyFile2(source, cible, paramètres)` : COPYFILE2_EXTENDED_PARAMETERS
/// commence par dwSize puis dwCopyFlags (COPY_FILE_FAIL_IF_EXISTS = 1),
/// mêmes décalages en x86 et en x64.
pub unsafe extern "win64" fn copy_file2(
    src: *const u16,
    dst: *const u16,
    params: *const u32,
) -> i32 {
    let fail_if_exists = !params.is_null() && *params.add(1) & 1 != 0;
    if super::kernel32::copy_file_w(src, dst, i32::from(fail_if_exists)) != 0 {
        S_OK
    } else {
        hresult_from_win32(crate::process::last_error())
    }
}

/// `SetWaitableTimerEx` : la tolérance et le contexte de réveil ne changent
/// rien ici.
pub unsafe extern "win64" fn set_waitable_timer_ex(
    timer: usize,
    due: *const i64,
    period: i32,
    routine: usize,
    arg: usize,
    _wake_context: usize,
    _tolerable_delay: u32,
) -> i32 {
    super::kernel32::set_waitable_timer(timer, due, period, routine, arg, 0)
}

/// `GetVolumePathNamesForVolumeNameW` : les noms de volume « \\?\Volume{…} »
/// n'existent pas ici.
pub unsafe extern "win64" fn get_volume_path_names_for_volume_name_w(
    _volume: *const u16,
    names: *mut u16,
    len: u32,
    returned: *mut u32,
) -> i32 {
    if !names.is_null() && len >= 2 {
        *names = 0;
        *names.add(1) = 0;
    }
    if !returned.is_null() {
        *returned = 0;
    }
    crate::process::set_last_error(2); // ERROR_FILE_NOT_FOUND
    0
}

/// `PssCaptureSnapshot` et suivantes : instantanés de processus non pris
/// en charge (python s'en sert pour lire le parent d'un processus).
pub extern "win64" fn pss_capture_snapshot(
    _p: usize,
    _flags: u32,
    _ctx: u32,
    snapshot: *mut usize,
) -> u32 {
    if !snapshot.is_null() {
        // SAFETY : sortie fournie par l'appelant.
        unsafe { crate::wow32::write_ptr(snapshot as usize, 0) };
    }
    ERROR_CALL_NOT_IMPLEMENTED
}
pub extern "win64" fn pss_query_snapshot(_s: usize, _class: u32, _buf: usize, _len: u32) -> u32 {
    ERROR_CALL_NOT_IMPLEMENTED
}
pub extern "win64" fn pss_free_snapshot(_p: usize, _s: usize) -> u32 {
    ERROR_CALL_NOT_IMPLEMENTED
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PathCchSkipRoot" => fn path_cch_skip_root,
        "PathCchCombineEx" => fn path_cch_combine_ex,
        "CopyFile2" => fn copy_file2,
        "SetWaitableTimerEx" => fn set_waitable_timer_ex,
        "GetVolumePathNamesForVolumeNameW" => fn get_volume_path_names_for_volume_name_w,
        "PssCaptureSnapshot" => fn pss_capture_snapshot,
        "PssQuerySnapshot" => fn pss_query_snapshot,
        "PssFreeSnapshot" => fn pss_free_snapshot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    #[test]
    fn racines_reconnues() {
        let r = |s: &str| root_len(&s.encode_utf16().collect::<Vec<_>>());
        assert_eq!(r("C:\\Windows"), Some(3));
        assert_eq!(r("C:"), Some(2));
        assert_eq!(r("\\dir"), Some(1));
        assert_eq!(r("\\\\srv\\part\\x"), Some(11));
        assert_eq!(r("\\\\?\\C:\\x"), Some(7));
        assert_eq!(r("\\\\?\\UNC\\srv\\part\\x"), Some(17));
        assert_eq!(r("relatif\\x"), None);
    }

    #[test]
    fn skip_root_et_combine() {
        let p = w("C:\\Python\\Lib");
        let mut end: usize = 0;
        unsafe {
            assert_eq!(
                path_cch_skip_root(p.as_ptr(), &mut end as *mut usize as usize),
                S_OK
            );
            assert_eq!(end, p.as_ptr().add(3) as usize);
            let rel = w("x");
            assert_eq!(
                path_cch_skip_root(rel.as_ptr(), &mut end as *mut usize as usize),
                E_INVALIDARG
            );

            let mut out = [0u16; 64];
            let a = w("C:\\Python");
            let b = w("Lib\\..\\DLLs");
            assert_eq!(
                path_cch_combine_ex(out.as_mut_ptr(), 64, a.as_ptr(), b.as_ptr(), 0),
                S_OK
            );
            assert_eq!(crate::dlls::wstr(out.as_ptr()), "C:\\Python\\DLLs");
            assert_eq!(
                path_cch_combine_ex(out.as_mut_ptr(), 4, a.as_ptr(), b.as_ptr(), 0),
                E_INSUFFICIENT_BUFFER
            );
        }
        let mut snap = 7usize;
        assert_eq!(
            pss_capture_snapshot(0, 0, 0, &mut snap),
            ERROR_CALL_NOT_IMPLEMENTED
        );
        assert_eq!(snap, 0);
    }
}
