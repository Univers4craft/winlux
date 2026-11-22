//! Famille PathCch de pathcch.h (api-ms-win-core-path-l1-1-0, KernelBase) :
//! manipulations de chemins qui acceptent les préfixes « \\ », « \\?\ » et
//! « \\?\UNC\ ». Paint.NET 5.2 importe PathCchRemoveFileSpec (#2921).
//!
//! Source : documentation publique de Microsoft (sdk-api, pages
//! nf-pathcch-*) ; PATHCCH_MAX_CCH et VOLUME_PREFIX : pathcch.h de mingw-w64
//! (domaine public). Les comportements que la documentation ne fixe pas sont
//! marqués « inférence » à l'endroit où ils sont choisis.
//!
//! PathCchSkipRoot et PathCchCombineEx restent dans kernel32_path.rs (#1383).
//! PathCchCanonicalize(Ex), PathCchCombine, PathCchAppend(Ex) et
//! PathAlloc* ne sont pas servies ici : leur règle de construction
//! automatique des chemins « \\?\ » longs n'est pas documentée.

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
/// Inférence : la page de PathCchAddExtension nomme PATHCCH_E_FILENAME_TOO_LONG
/// sans en donner la valeur ; nous prenons
/// HRESULT_FROM_WIN32(ERROR_FILENAME_EXCED_RANGE = 206, winerror.h).
const PATHCCH_E_FILENAME_TOO_LONG: i32 = 0x8007_00CEu32 as i32;
/// Inférence : PathCchAddBackslash ne nomme pas son erreur de tampon trop
/// petit ; nous rendons celle de strsafe (ERROR_INSUFFICIENT_BUFFER).
const E_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
/// pathcch.h (mingw-w64).
const PATHCCH_MAX_CCH: usize = 0x8000;

const BS: u16 = b'\\' as u16;
const DOT: u16 = b'.' as u16;

fn is_drive(s: &[u16]) -> bool {
    s.len() >= 2 && s[1] == b':' as u16 && s[0] < 128 && (s[0] as u8).is_ascii_alphabetic()
}

fn starts_with_ci(s: &[u16], prefix: &str) -> bool {
    let p: Vec<u16> = prefix.encode_utf16().collect();
    s.len() >= p.len()
        && s[..p.len()]
            .iter()
            .zip(&p)
            .all(|(a, b)| *a < 128 && (*a as u8).eq_ignore_ascii_case(&(*b as u8)))
}

/// Fin d'au plus deux composants « serveur\partage » à partir de `start`,
/// sans la barre qui suit : « \\serveur\partage\x » a pour racine
/// « \\serveur\partage » (tables de PathCchStripToRoot et PathCchIsRoot).
fn unc_root_end(p: &[u16], start: usize) -> usize {
    let mut i = start;
    while i < p.len() && p[i] != BS {
        i += 1;
    }
    // Un second composant vide (« \\serveur\ ») n'appartient pas à la racine.
    if i + 1 < p.len() && p[i] == BS && p[i + 1] != BS {
        i += 1;
        while i < p.len() && p[i] != BS {
            i += 1;
        }
    }
    i
}

/// Racine d'un chemin : (longueur, racine complète au sens de PathCchIsRoot).
/// Seule la barre inverse sépare les éléments : la documentation de
/// PathCchCanonicalize précise que la famille ne convertit pas « / »
/// (inférence : « / » n'est donc pas un séparateur ici).
fn root(p: &[u16]) -> Option<(usize, bool)> {
    if starts_with_ci(p, "\\\\?\\UNC\\") {
        return Some((unc_root_end(p, 8), true));
    }
    if starts_with_ci(p, "\\\\?\\Volume") {
        // « \\?\Volume{guid}\ » est une racine, « \\?\Volume{guid} » non.
        let mut i = 10;
        while i < p.len() && p[i] != BS {
            i += 1;
        }
        return Some(if i < p.len() {
            (i + 1, true)
        } else {
            (i, false)
        });
    }
    if starts_with_ci(p, "\\\\?\\") {
        let rest = &p[4..];
        if is_drive(rest) {
            return Some(if rest.get(2) == Some(&BS) {
                (7, true)
            } else {
                (6, false)
            });
        }
        return None;
    }
    if p.len() >= 2 && p[0] == BS && p[1] == BS {
        return Some((unc_root_end(p, 2), true));
    }
    if is_drive(p) {
        return Some(if p.get(2) == Some(&BS) {
            (3, true)
        } else {
            (2, false)
        });
    }
    if p.first() == Some(&BS) {
        return Some((1, true));
    }
    None
}

/// Le chemin en place, s'il tient dans `cch` caractères nul compris.
/// Inférence : une chaîne sans zéro dans son tampon est un argument invalide.
unsafe fn buffer<'a>(path: *mut u16, cch: usize) -> Result<&'a mut [u16], i32> {
    if path.is_null() || cch == 0 || cch > PATHCCH_MAX_CCH {
        return Err(E_INVALIDARG);
    }
    let all = std::slice::from_raw_parts_mut(path, cch);
    if all.contains(&0) {
        Ok(all)
    } else {
        Err(E_INVALIDARG)
    }
}

fn len(buf: &[u16]) -> usize {
    buf.iter().position(|&c| c == 0).unwrap_or(buf.len())
}

/// Position du « . » de l'extension du dernier élément, ou la fin.
fn extension_at(p: &[u16]) -> usize {
    let start = root(p).map_or(0, |(r, _)| r);
    let last = p[start..]
        .iter()
        .rposition(|&c| c == BS)
        .map_or(start, |i| start + i + 1);
    p[last..]
        .iter()
        .rposition(|&c| c == DOT)
        .map_or(p.len(), |i| last + i)
}

unsafe fn write_out(at: usize, v: usize) {
    if at != 0 {
        crate::wow32::write_ptr(at, v);
    }
}

/// `PathCchRemoveFileSpec(chemin, taille)` : retire le dernier élément et
/// la barre qui le précède, jamais la racine ; une barre finale seule est
/// retirée seule. S_FALSE s'il n'y a rien à retirer.
pub unsafe extern "win64" fn path_cch_remove_file_spec(path: *mut u16, cch: usize) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let n = len(buf);
    let r = root(&buf[..n]).map_or(0, |(r, _)| r);
    if n == r {
        return S_FALSE;
    }
    let cut = if buf[n - 1] == BS {
        n - 1
    } else {
        // Inférence : un chemin relatif d'un seul élément devient vide.
        buf[r..n]
            .iter()
            .rposition(|&c| c == BS)
            .map_or(r, |i| r + i)
    };
    buf[cut] = 0;
    S_OK
}

/// `PathCchRemoveBackslashEx(chemin, taille, &fin, &reste)`.
pub unsafe extern "win64" fn path_cch_remove_backslash_ex(
    path: *mut u16,
    cch: usize,
    end: usize,
    remaining: usize,
) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let n = len(buf);
    let is_root = root(&buf[..n]).is_some_and(|(r, _)| r == n);
    let (hr, at) = if n > 0 && buf[n - 1] == BS {
        if is_root {
            // La barre d'une racine reste ; fin et reste la désignent.
            (S_FALSE, n - 1)
        } else {
            buf[n - 1] = 0;
            (S_OK, n - 1)
        }
    } else {
        (S_FALSE, n)
    };
    write_out(end, path.add(at) as usize);
    write_out(remaining, cch - at);
    hr
}

/// `PathCchRemoveBackslash(chemin, taille)`.
pub unsafe extern "win64" fn path_cch_remove_backslash(path: *mut u16, cch: usize) -> i32 {
    path_cch_remove_backslash_ex(path, cch, 0, 0)
}

/// `PathCchAddBackslashEx(chemin, taille, &fin, &reste)`.
pub unsafe extern "win64" fn path_cch_add_backslash_ex(
    path: *mut u16,
    cch: usize,
    end: usize,
    remaining: usize,
) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let mut n = len(buf);
    let hr = if n > 0 && buf[n - 1] == BS {
        S_FALSE
    } else if n + 2 > cch {
        return E_INSUFFICIENT_BUFFER;
    } else {
        buf[n] = BS;
        buf[n + 1] = 0;
        n += 1;
        S_OK
    };
    write_out(end, path.add(n) as usize);
    write_out(remaining, cch - n);
    hr
}

/// `PathCchAddBackslash(chemin, taille)`.
pub unsafe extern "win64" fn path_cch_add_backslash(path: *mut u16, cch: usize) -> i32 {
    path_cch_add_backslash_ex(path, cch, 0, 0)
}

/// `PathCchStripToRoot(chemin, taille)` : S_FALSE si le chemin n'est déjà
/// qu'une racine. Inférence : un chemin sans racine est refusé
/// (E_INVALIDARG), comme PathCchSkipRoot le fait.
pub unsafe extern "win64" fn path_cch_strip_to_root(path: *mut u16, cch: usize) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let n = len(buf);
    match root(&buf[..n]) {
        None => E_INVALIDARG,
        Some((r, _)) if r == n => S_FALSE,
        Some((r, _)) => {
            buf[r] = 0;
            S_OK
        }
    }
}

/// `PathCchIsRoot(chemin)`. La table de la page rend TRUE pour « \path1 »,
/// mais ses remarques ne citent que « \ » : nous suivons les remarques et
/// PathCchStripToRoot (« \path1 » a pour racine « \ »), inférence.
pub unsafe extern "win64" fn path_cch_is_root(path: *const u16) -> i32 {
    if path.is_null() {
        return 0;
    }
    let p = std::slice::from_raw_parts(path, crate::printf::wide_len(path));
    i32::from(!p.is_empty() && root(p) == Some((p.len(), true)))
}

/// `PathCchFindExtension(chemin, taille, &extension)`.
pub unsafe extern "win64" fn path_cch_find_extension(
    path: *const u16,
    cch: usize,
    ext: usize,
) -> i32 {
    if ext == 0 {
        return E_INVALIDARG;
    }
    let buf = match buffer(path as *mut u16, cch) {
        Ok(b) => b,
        Err(e) => {
            write_out(ext, 0);
            return e;
        }
    };
    let n = len(buf);
    write_out(ext, path.add(extension_at(&buf[..n])) as usize);
    S_OK
}

/// `PathCchRemoveExtension(chemin, taille)` : S_FALSE sans extension.
pub unsafe extern "win64" fn path_cch_remove_extension(path: *mut u16, cch: usize) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let n = len(buf);
    let at = extension_at(&buf[..n]);
    if at == n {
        return S_FALSE;
    }
    buf[at] = 0;
    S_OK
}

/// Extension sans son point, ou None si elle est invalide. Inférence : la
/// documentation parle de « caractères illégaux » sans les lister ; nous
/// refusons ceux qu'un nom de fichier Windows refuse, plus « . » et
/// l'espace, qui feraient une extension introuvable ensuite.
unsafe fn extension_arg(ext: *const u16) -> Option<Vec<u16>> {
    if ext.is_null() {
        return None;
    }
    let s = std::slice::from_raw_parts(ext, crate::printf::wide_len(ext));
    let s = s.strip_prefix(&[DOT]).unwrap_or(s);
    let bad = |c: u16| c < 32 || "\\/:*?\"<>|. ".encode_utf16().any(|b| b == c);
    if s.iter().any(|&c| bad(c)) {
        return None;
    }
    Some(s.to_vec())
}

/// `PathCchAddExtension(chemin, taille, extension)` : S_FALSE si le chemin
/// a déjà une extension ; une extension vide laisse le chemin intact.
pub unsafe extern "win64" fn path_cch_add_extension(
    path: *mut u16,
    cch: usize,
    ext: *const u16,
) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let Some(e) = extension_arg(ext) else {
        return E_INVALIDARG;
    };
    let n = len(buf);
    if extension_at(&buf[..n]) != n {
        return S_FALSE;
    }
    if e.is_empty() {
        return S_OK;
    }
    if n + 1 + e.len() + 1 > cch {
        return PATHCCH_E_FILENAME_TOO_LONG;
    }
    buf[n] = DOT;
    buf[n + 1..n + 1 + e.len()].copy_from_slice(&e);
    buf[n + 1 + e.len()] = 0;
    S_OK
}

/// `PathCchRenameExtension(chemin, taille, extension)` : remplace (ou
/// ajoute) l'extension ; une extension vide retire l'existante. Le chemin
/// reste intact en cas d'échec.
pub unsafe extern "win64" fn path_cch_rename_extension(
    path: *mut u16,
    cch: usize,
    ext: *const u16,
) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let Some(e) = extension_arg(ext) else {
        return E_INVALIDARG;
    };
    let n = len(buf);
    let at = extension_at(&buf[..n]);
    if e.is_empty() {
        buf[at] = 0;
        return S_OK;
    }
    if at + 1 + e.len() + 1 > cch {
        return PATHCCH_E_FILENAME_TOO_LONG;
    }
    buf[at] = DOT;
    buf[at + 1..at + 1 + e.len()].copy_from_slice(&e);
    buf[at + 1 + e.len()] = 0;
    S_OK
}

/// `PathCchStripPrefix(chemin, taille)` : « \\?\C:\x » → « C:\x » et,
/// inférence, « \\?\UNC\serveur\x » → « \\serveur\x » (retirer les seuls
/// quatre caractères laisserait « UNC\… », qui n'est plus un chemin UNC).
/// Les autres formes (« \\?\Volume{…} ») n'ont pas d'équivalent sans
/// préfixe : S_FALSE.
pub unsafe extern "win64" fn path_cch_strip_prefix(path: *mut u16, cch: usize) -> i32 {
    let buf = match buffer(path, cch) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let n = len(buf);
    let (from, to) = if starts_with_ci(&buf[..n], "\\\\?\\UNC\\") {
        (8, 2)
    } else if starts_with_ci(&buf[..n], "\\\\?\\") && is_drive(&buf[4..n]) {
        (4, 0)
    } else {
        return S_FALSE;
    };
    buf.copy_within(from..=n, to);
    S_OK
}

/// `PathIsUNCEx(chemin, &serveur)` : « \\serveur… » ou « \\?\UNC\serveur… ».
/// Inférence : `serveur` n'est écrit qu'en cas de succès.
pub unsafe extern "win64" fn path_is_unc_ex(path: *const u16, server: usize) -> i32 {
    if path.is_null() {
        return 0;
    }
    let p = std::slice::from_raw_parts(path, crate::printf::wide_len(path));
    let at = if starts_with_ci(p, "\\\\?\\UNC\\") {
        8
    } else if starts_with_ci(p, "\\\\?\\") {
        return 0;
    } else if p.len() >= 2 && p[0] == BS && p[1] == BS {
        2
    } else {
        return 0;
    };
    write_out(server, path.add(at) as usize);
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PathCchRemoveFileSpec" => fn path_cch_remove_file_spec,
        "PathCchRemoveBackslash" => fn path_cch_remove_backslash,
        "PathCchRemoveBackslashEx" => fn path_cch_remove_backslash_ex,
        "PathCchAddBackslash" => fn path_cch_add_backslash,
        "PathCchAddBackslashEx" => fn path_cch_add_backslash_ex,
        "PathCchStripToRoot" => fn path_cch_strip_to_root,
        "PathCchIsRoot" => fn path_cch_is_root,
        "PathCchFindExtension" => fn path_cch_find_extension,
        "PathCchRemoveExtension" => fn path_cch_remove_extension,
        "PathCchAddExtension" => fn path_cch_add_extension,
        "PathCchRenameExtension" => fn path_cch_rename_extension,
        "PathCchStripPrefix" => fn path_cch_strip_prefix,
        "PathIsUNCEx" => fn path_is_unc_ex,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str, cch: usize) -> Vec<u16> {
        let mut v: Vec<u16> = s.encode_utf16().collect();
        v.resize(cch, 0);
        v
    }

    fn text(v: &[u16]) -> String {
        String::from_utf16_lossy(&v[..len(v)])
    }

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Table de la page PathCchRemoveFileSpec, préfixes longs et cas
    /// « rien à retirer ».
    #[test]
    fn retire_le_dernier_element_2921() {
        for (avant, apres, hr) in [
            (r"C:\path1", r"C:\", S_OK),
            (r"C:\path1\path2", r"C:\path1", S_OK),
            (r"C:\path1\", r"C:\path1", S_OK),
            (r"\\path1\path2\path3", r"\\path1\path2", S_OK),
            (r"\path1", r"\", S_OK),
            (r"\\?\C:\a\b", r"\\?\C:\a", S_OK),
            (r"\\?\C:\a", r"\\?\C:\", S_OK),
            (r"\\?\UNC\srv\share\dir", r"\\?\UNC\srv\share", S_OK),
            (r"C:\", r"C:\", S_FALSE),
            (r"\", r"\", S_FALSE),
            (r"\\srv\share", r"\\srv\share", S_FALSE),
            (r"\\?\UNC\srv\share", r"\\?\UNC\srv\share", S_FALSE),
            ("", "", S_FALSE),
            (r"dossier\fichier.txt", "dossier", S_OK),
        ] {
            let mut b = buf(avant, 64);
            unsafe {
                assert_eq!(path_cch_remove_file_spec(b.as_mut_ptr(), 64), hr, "{avant}");
            }
            assert_eq!(text(&b), apres, "{avant}");
        }
        let mut b = buf(r"C:\a", 4); // pas de zéro dans le tampon
        unsafe {
            assert_eq!(path_cch_remove_file_spec(b.as_mut_ptr(), 4), E_INVALIDARG);
            assert_eq!(
                path_cch_remove_file_spec(std::ptr::null_mut(), 4),
                E_INVALIDARG
            );
            assert_eq!(path_cch_remove_file_spec(b.as_mut_ptr(), 0), E_INVALIDARG);
        }
    }

    /// Import statique par api-ms-win-core-path (Paint.NET) ; GetProcAddress
    /// ne les voit que sur KernelBase, comme sous Windows (#1383).
    #[test]
    fn servies_par_le_contrat_path_2921() {
        let contrat = crate::dlls::normalize("api-ms-win-core-path-l1-1-0.dll");
        for (nom, _) in exports() {
            assert!(crate::dlls::lookup(&contrat, nom).is_some(), "{nom}");
            assert!(crate::dlls::kernel32_proc(nom, true).is_some(), "{nom}");
            assert!(crate::dlls::kernel32_proc(nom, false).is_none(), "{nom}");
        }
    }

    /// Table de la page PathCchIsRoot (sauf « \path1 », voir la fonction).
    #[test]
    fn racines_de_la_documentation_2921() {
        for (p, attendu) in [
            (r"c:\", true),
            ("c:", false),
            (r"c:\path1", false),
            ("path1", false),
            (r"\\path1\path2", true),
            (r"\\path1\path2\", false),
            (r"\\path1\path2\path3", false),
            (r"\\path1", true),
            (r"\\path1\", false),
            (r"\\", true),
            (r"\\?\UNC\", true),
            (r"\\?\UNC\path1\path2", true),
            (r"\\?\UNC\path1\path2\", false),
            (r"\\?\UNC\path1", true),
            (r"\\?\UNC\path1\", false),
            (r"\\?\c:\", true),
            (r"\\?\c:", false),
            (r"\\?\c:\path1", false),
            (r"\\?\Volume{guid}\", true),
            (r"\\?\Volume{guid}", false),
            (r"\\?\Volume{guid}\path1", false),
            ("", false),
            (r"\", true),
        ] {
            let v = w(p);
            assert_eq!(unsafe { path_cch_is_root(v.as_ptr()) } != 0, attendu, "{p}");
        }
        assert_eq!(unsafe { path_cch_is_root(std::ptr::null()) }, 0);
    }

    #[test]
    fn barres_racine_et_prefixe_2921() {
        unsafe {
            let mut b = buf(r"C:\a\", 16);
            let (mut fin, mut reste) = (0usize, 0usize);
            let (pf, pr) = (
                &mut fin as *mut usize as usize,
                &mut reste as *mut usize as usize,
            );
            assert_eq!(
                path_cch_remove_backslash_ex(b.as_mut_ptr(), 16, pf, pr),
                S_OK
            );
            assert_eq!(
                (text(&b).as_str(), fin, reste),
                (r"C:\a", b.as_ptr().add(4) as usize, 12)
            );
            assert_eq!(path_cch_remove_backslash(b.as_mut_ptr(), 16), S_FALSE);
            let mut r = buf(r"C:\", 16);
            assert_eq!(
                path_cch_remove_backslash_ex(r.as_mut_ptr(), 16, pf, pr),
                S_FALSE
            );
            assert_eq!(
                (text(&r).as_str(), fin, reste),
                (r"C:\", r.as_ptr().add(2) as usize, 14)
            );

            assert_eq!(path_cch_add_backslash_ex(b.as_mut_ptr(), 16, pf, pr), S_OK);
            assert_eq!(
                (text(&b).as_str(), fin, reste),
                (r"C:\a\", b.as_ptr().add(5) as usize, 11)
            );
            assert_eq!(path_cch_add_backslash(b.as_mut_ptr(), 16), S_FALSE);
            let mut plein = buf("abc", 4);
            assert_eq!(
                path_cch_add_backslash(plein.as_mut_ptr(), 4),
                E_INSUFFICIENT_BUFFER
            );
            assert_eq!(text(&plein), "abc");

            for (avant, apres, hr) in [
                (r"C:\path1\path2\file", r"C:\", S_OK),
                (r"\\path1\path2\path3", r"\\path1\path2", S_OK),
                (r"\path1", r"\", S_OK),
                (r"C:\", r"C:\", S_FALSE),
                (r"\\?\UNC\a\b\c", r"\\?\UNC\a\b", S_OK),
            ] {
                let mut b = buf(avant, 64);
                assert_eq!(path_cch_strip_to_root(b.as_mut_ptr(), 64), hr, "{avant}");
                assert_eq!(text(&b), apres, "{avant}");
            }
            let mut rel = buf("relatif", 16);
            assert_eq!(path_cch_strip_to_root(rel.as_mut_ptr(), 16), E_INVALIDARG);

            for (avant, apres, hr) in [
                (r"\\?\C:\x", r"C:\x", S_OK),
                (r"\\?\UNC\srv\share", r"\\srv\share", S_OK),
                (r"C:\x", r"C:\x", S_FALSE),
                (r"\\?\Volume{g}\x", r"\\?\Volume{g}\x", S_FALSE),
            ] {
                let mut b = buf(avant, 64);
                assert_eq!(path_cch_strip_prefix(b.as_mut_ptr(), 64), hr, "{avant}");
                assert_eq!(text(&b), apres, "{avant}");
            }

            let mut serveur = 0usize;
            let ps = &mut serveur as *mut usize as usize;
            let u = w(r"\\srv\share");
            assert_eq!(path_is_unc_ex(u.as_ptr(), ps), 1);
            assert_eq!(serveur, u.as_ptr().add(2) as usize);
            let u = w(r"\\?\UNC\srv\share");
            assert_eq!(path_is_unc_ex(u.as_ptr(), ps), 1);
            assert_eq!(serveur, u.as_ptr().add(8) as usize);
            assert_eq!(path_is_unc_ex(w(r"\\?\C:\x").as_ptr(), 0), 0);
            assert_eq!(path_is_unc_ex(w(r"C:\x").as_ptr(), 0), 0);
        }
    }

    #[test]
    fn extensions_2921() {
        unsafe {
            let p = buf(r"C:\a.b\fichier.txt", 32);
            let mut ext = 0usize;
            let pe = &mut ext as *mut usize as usize;
            assert_eq!(path_cch_find_extension(p.as_ptr(), 32, pe), S_OK);
            assert_eq!(ext, p.as_ptr().add(14) as usize);
            let q = buf(r"C:\a.b\fichier", 32);
            assert_eq!(path_cch_find_extension(q.as_ptr(), 32, pe), S_OK);
            assert_eq!(ext, q.as_ptr().add(14) as usize, "sans extension : la fin");
            assert_eq!(path_cch_find_extension(q.as_ptr(), 32, 0), E_INVALIDARG);

            let mut b = buf(r"C:\a.b\fichier.txt", 32);
            assert_eq!(path_cch_remove_extension(b.as_mut_ptr(), 32), S_OK);
            assert_eq!(text(&b), r"C:\a.b\fichier");
            assert_eq!(path_cch_remove_extension(b.as_mut_ptr(), 32), S_FALSE);

            assert_eq!(
                path_cch_add_extension(b.as_mut_ptr(), 32, w(".png").as_ptr()),
                S_OK
            );
            assert_eq!(text(&b), r"C:\a.b\fichier.png");
            assert_eq!(
                path_cch_add_extension(b.as_mut_ptr(), 32, w("jpg").as_ptr()),
                S_FALSE
            );
            assert_eq!(
                path_cch_add_extension(b.as_mut_ptr(), 32, w("a*b").as_ptr()),
                E_INVALIDARG
            );
            assert_eq!(
                path_cch_add_extension(b.as_mut_ptr(), 32, std::ptr::null()),
                E_INVALIDARG
            );

            assert_eq!(
                path_cch_rename_extension(b.as_mut_ptr(), 32, w("jpeg").as_ptr()),
                S_OK
            );
            assert_eq!(text(&b), r"C:\a.b\fichier.jpeg");
            assert_eq!(
                path_cch_rename_extension(b.as_mut_ptr(), 32, w("").as_ptr()),
                S_OK
            );
            assert_eq!(text(&b), r"C:\a.b\fichier");
            assert_eq!(
                path_cch_add_extension(b.as_mut_ptr(), 32, w(".").as_ptr()),
                S_OK
            );
            assert_eq!(
                text(&b),
                r"C:\a.b\fichier",
                "extension vide : chemin intact"
            );

            let mut court = buf("f", 4);
            assert_eq!(
                path_cch_add_extension(court.as_mut_ptr(), 4, w("txt").as_ptr()),
                PATHCCH_E_FILENAME_TOO_LONG
            );
            assert_eq!(text(&court), "f");
        }
    }
}
