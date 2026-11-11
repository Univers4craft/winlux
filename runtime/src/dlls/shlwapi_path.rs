//! shlwapi.dll — le reste de la famille Path* (#1631).
//!
//! Pythonwin s'arrêtait sur PathStripToRootW ; plutôt qu'une fonction par
//! passe du banc, les fonctions Path* documentées de shlwapi qui manquaient
//! sont servies ici, en A et en W, d'après les pages publiques de
//! learn.microsoft.com (shlwapi.h). Les chemins sont traités comme des
//! suites d'unités (octets pour A, UTF-16 pour W), avec MAX_PATH (260)
//! comme taille de tampon quand la fonction modifie le chemin sur place.

#![allow(non_snake_case)]

use super::shlwapi::{canonicalize_units, root_len};

const BS: u32 = b'\\' as u32;
const MAX_PATH: usize = 260;

fn is_sep(c: u32) -> bool {
    c == BS || c == b'/' as u32
}

fn lower(c: u32) -> u32 {
    if (b'A' as u32..=b'Z' as u32).contains(&c) {
        c + 32
    } else {
        c
    }
}

fn units(s: &str) -> Vec<u32> {
    s.encode_utf16().map(u32::from).collect()
}

fn text(v: &[u32]) -> String {
    String::from_utf16_lossy(&v.iter().map(|&c| c as u16).collect::<Vec<_>>())
}

// ── Lecture et écriture des chaînes A et W ──────────────────────────────

trait Unit: Copy {
    fn get(self) -> u32;
    fn from(c: u32) -> Self;
}
impl Unit for u8 {
    fn get(self) -> u32 {
        self as u32
    }
    fn from(c: u32) -> Self {
        c as u8
    }
}
impl Unit for u16 {
    fn get(self) -> u32 {
        self as u32
    }
    fn from(c: u32) -> Self {
        c as u16
    }
}

unsafe fn read<T: Unit>(p: *const T) -> Vec<u32> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    let mut i = 0;
    loop {
        let c = (*p.add(i)).get();
        if c == 0 {
            return v;
        }
        v.push(c);
        i += 1;
    }
}

/// Écrit `v` et le zéro final si la place suffit (`cap` unités).
unsafe fn write<T: Unit>(p: *mut T, v: &[u32], cap: usize) -> bool {
    if p.is_null() || v.len() >= cap {
        return false;
    }
    for (i, &c) in v.iter().enumerate() {
        *p.add(i) = T::from(c);
    }
    *p.add(v.len()) = T::from(0);
    true
}

// ── Opérations sur les unités ───────────────────────────────────────────

fn is_root(p: &[u32]) -> bool {
    if p == [BS] || (p.len() == 3 && p[1] == b':' as u32 && p[2] == BS) {
        return true;
    }
    if p.len() > 2 && p[0] == BS && p[1] == BS {
        let inner = p[2..].strip_suffix(&[BS]).unwrap_or(&p[2..]);
        return inner.iter().filter(|&&c| c == BS).count() <= 1 && !inner.is_empty();
    }
    false
}

/// PathStripToRoot : garde « C:\ », « \ » ou « \\serveur\partage ».
fn strip_to_root(v: &mut Vec<u32>) -> bool {
    let r = root_len(v);
    if r == 0 {
        v.clear();
        return false;
    }
    v.truncate(r);
    if v.len() > 2 && v[0] == BS && v[1] == BS && v.last() == Some(&BS) {
        v.pop();
    }
    true
}

/// Début du dernier élément (« dir2\ » pour « c:\dir1\dir2\ »).
fn file_part(v: &[u32]) -> usize {
    let end = if v.last() == Some(&BS) {
        v.len() - 1
    } else {
        v.len()
    };
    v[..end]
        .iter()
        .rposition(|&c| is_sep(c) || c == b':' as u32)
        .map_or(0, |i| i + 1)
}

fn strip_path(v: &mut Vec<u32>) {
    if is_root(v) {
        return;
    }
    let cut = file_part(v);
    v.drain(..cut);
}

fn skip_root(v: &[u32]) -> Option<usize> {
    if v.len() >= 2 && v[1] == b':' as u32 && v.get(2) != Some(&BS) {
        return None;
    }
    match root_len(v) {
        0 => None,
        n => Some(n),
    }
}

fn is_unc(v: &[u32]) -> bool {
    if v.len() < 2 || v[0] != BS || v[1] != BS {
        return false;
    }
    if v.get(2) == Some(&(b'?' as u32)) {
        let unc = units("\\\\?\\UNC\\");
        return v.len() >= unc.len()
            && v[..unc.len()]
                .iter()
                .map(|&c| lower(c))
                .eq(unc.iter().map(|&c| lower(c)));
    }
    true
}

fn is_unc_server(v: &[u32]) -> bool {
    is_unc(v) && !v[2..].contains(&BS)
}

fn is_unc_server_share(v: &[u32]) -> bool {
    if !is_unc(v) || v.get(2) == Some(&(b'?' as u32)) {
        return false;
    }
    let parts: Vec<&[u32]> = v[2..].split(|&c| c == BS).collect();
    parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty()
}

fn is_file_spec(v: &[u32]) -> bool {
    !v.iter().any(|&c| is_sep(c) || c == b':' as u32)
}

/// PathCommonPrefix : longueur du préfixe commun, élément par élément,
/// sans tenir compte de la casse.
fn common_prefix(a: &[u32], b: &[u32]) -> usize {
    let mut i = 0;
    let mut last = 0;
    loop {
        let ca = a.get(i).copied();
        let cb = b.get(i).copied();
        let end_a = ca.is_none_or(|c| c == BS);
        let end_b = cb.is_none_or(|c| c == BS);
        if end_a && end_b {
            last = i;
            if ca.is_none() || cb.is_none() {
                break;
            }
        } else if ca.map(lower) != cb.map(lower) {
            break;
        }
        i += 1;
    }
    // « C: » commun : la racine « C:\ » l'est aussi.
    if last == 2
        && a.get(1) == Some(&(b':' as u32))
        && a.get(2) == Some(&BS)
        && b.get(2) == Some(&BS)
    {
        return 3;
    }
    // « \\ » seul ne fait pas un préfixe UNC.
    if last <= 2 && a.starts_with(&[BS, BS]) {
        return 0;
    }
    last
}

fn is_prefix(prefix: &[u32], path: &[u32]) -> bool {
    !prefix.is_empty() && common_prefix(prefix, path) == prefix.len()
}

fn is_same_root(a: &[u32], b: &[u32]) -> bool {
    let ra = root_len(a);
    let rb = root_len(b);
    ra > 0
        && ra == rb
        && a[..ra]
            .iter()
            .map(|&c| lower(c))
            .eq(b[..rb].iter().map(|&c| lower(c)))
}

/// PathIsURL : un schéma d'au moins deux caractères suivi de « : ».
fn is_url(v: &[u32]) -> bool {
    let Some(colon) = v.iter().position(|&c| c == b':' as u32) else {
        return false;
    };
    colon >= 2
        && v[..colon]
            .iter()
            .all(|&c| c < 128 && ((c as u8).is_ascii_alphanumeric() || b"+-.".contains(&(c as u8))))
}

/// Position du premier espace hors guillemets (début des arguments).
fn args_start(v: &[u32]) -> Option<usize> {
    let mut quoted = false;
    for (i, &c) in v.iter().enumerate() {
        if c == b'"' as u32 {
            quoted = !quoted;
        } else if c == b' ' as u32 && !quoted {
            return Some(i);
        }
    }
    None
}

fn remove_args(v: &mut Vec<u32>) {
    if let Some(i) = args_start(v) {
        v.truncate(i);
    }
}

fn remove_blanks(v: &mut Vec<u32>) {
    while v.last() == Some(&(b' ' as u32)) {
        v.pop();
    }
    let lead = v.iter().take_while(|&&c| c == b' ' as u32).count();
    v.drain(..lead);
}

fn quote_spaces(v: &mut Vec<u32>) -> bool {
    if !v.contains(&(b' ' as u32)) || v.first() == Some(&(b'"' as u32)) {
        return false;
    }
    if v.len() + 2 >= MAX_PATH {
        return false;
    }
    v.insert(0, b'"' as u32);
    v.push(b'"' as u32);
    true
}

fn unquote_spaces(v: &mut Vec<u32>) -> bool {
    if v.len() >= 2 && v[0] == b'"' as u32 && v[v.len() - 1] == b'"' as u32 {
        v.pop();
        v.remove(0);
        return true;
    }
    false
}

/// Position du « . » de l'extension dans le dernier élément.
fn extension(v: &[u32]) -> Option<usize> {
    let start = v.iter().rposition(|&c| is_sep(c)).map_or(0, |i| i + 1);
    v[start..]
        .iter()
        .rposition(|&c| c == b'.' as u32)
        .map(|i| start + i)
}

/// PathUndecorate : « nom[1].ext » devient « nom.ext ».
fn undecorate(v: &mut Vec<u32>) {
    let dot = extension(v).unwrap_or(v.len());
    if dot < 3 || v[dot - 1] != b']' as u32 {
        return;
    }
    let Some(open) = v[..dot - 1].iter().rposition(|&c| c == b'[' as u32) else {
        return;
    };
    let inner = &v[open + 1..dot - 1];
    if !inner.is_empty()
        && inner
            .iter()
            .all(|&c| (b'0' as u32..=b'9' as u32).contains(&c))
    {
        v.drain(open..dot);
    }
}

fn rename_extension(v: &mut Vec<u32>, ext: &[u32]) -> bool {
    let base = extension(v).unwrap_or(v.len());
    if base + ext.len() >= MAX_PATH {
        return false;
    }
    v.truncate(base);
    v.extend_from_slice(ext);
    true
}

fn add_extension(v: &mut Vec<u32>, ext: Option<Vec<u32>>) -> bool {
    if extension(v).is_some() {
        return true;
    }
    let ext = ext.unwrap_or_else(|| units(".exe"));
    if v.len() + ext.len() >= MAX_PATH {
        return false;
    }
    v.extend_from_slice(&ext);
    true
}

/// PathFindNextComponent : après le prochain séparateur ; `None` si le
/// chemin est déjà épuisé.
fn next_component(v: &[u32]) -> Option<usize> {
    if v.is_empty() {
        return None;
    }
    Some(v.iter().position(|&c| c == BS).map_or(v.len(), |i| i + 1))
}

fn drive_number(v: &[u32]) -> i32 {
    let p = if v.starts_with(&units("\\\\?\\")) {
        &v[4..]
    } else {
        v
    };
    match p {
        [c, b, ..] if *b == b':' as u32 && *c < 128 && (*c as u8).is_ascii_alphabetic() => {
            ((*c as u8 | 0x20) - b'a') as i32
        }
        _ => -1,
    }
}

/// PathGetCharType : GCT_INVALID 0, GCT_LFNCHAR 1, GCT_SHORTCHAR 2,
/// GCT_WILD 4, GCT_SEPARATOR 8.
fn char_type(c: u32) -> u32 {
    match c {
        0x2A | 0x3F => 4,
        0x5C | 0x2F | 0x3A => 8,
        0..=0x1F | 0x22 | 0x3C | 0x3E | 0x7C => 0,
        0x20 | 0x2B | 0x2C | 0x3B | 0x3D | 0x5B | 0x5D => 1,
        _ => 3,
    }
}

/// PathIsLFNFileSpec : vrai si le nom dépasse la forme 8.3.
fn is_lfn_file_spec(v: &[u32]) -> bool {
    let name = &v[file_part(v)..];
    let parts: Vec<&[u32]> = name.split(|&c| c == b'.' as u32).collect();
    if parts.len() > 2 || name.iter().any(|&c| char_type(c) == 1) {
        return true;
    }
    parts[0].len() > 8 || parts.get(1).is_some_and(|e| e.len() > 3)
}

/// PathMakePretty : un chemin tout en capitales passe en minuscules.
fn make_pretty(v: &mut [u32]) -> bool {
    let upper = v.iter().any(|&c| (b'A' as u32..=b'Z' as u32).contains(&c));
    let lower_found = v.iter().any(|&c| (b'a' as u32..=b'z' as u32).contains(&c));
    if !upper || lower_found {
        return false;
    }
    for c in v.iter_mut() {
        *c = lower(*c);
    }
    true
}

/// PathParseIconLocation : retire « ,index » et les guillemets, rend
/// l'index.
fn parse_icon_location(v: &mut Vec<u32>) -> i32 {
    let mut index = 0;
    if let Some(comma) = v.iter().rposition(|&c| c == b',' as u32) {
        index = text(&v[comma + 1..]).trim().parse().unwrap_or(0);
        v.truncate(comma);
    }
    remove_blanks(v);
    unquote_spaces(v);
    index
}

fn relative_path_to(from: &[u32], from_dir: bool, to: &[u32], to_dir: bool) -> Option<Vec<u32>> {
    let mut from = from.to_vec();
    if !from_dir {
        let cut = file_part(&from);
        from.truncate(cut.saturating_sub(1).max(root_len(&from)));
    }
    let mut to = to.to_vec();
    let _ = to_dir;
    if !is_same_root(&from, &to) {
        return None;
    }
    while from.len() > root_len(&from) && from.last() == Some(&BS) {
        from.pop();
    }
    let mut common = common_prefix(&from, &to);
    if common < from.len() && from.get(common) != Some(&BS) && common != root_len(&from) {
        common = 0;
    }
    let ups = from[common..]
        .split(|&c| c == BS)
        .filter(|s| !s.is_empty())
        .count();
    let rest: Vec<u32> = to.split_off(common.min(to.len()));
    let rest: &[u32] = rest.strip_prefix(&[BS]).unwrap_or(&rest);
    let mut out = Vec::new();
    if ups == 0 {
        out.extend(units(".\\"));
    }
    for _ in 0..ups {
        out.extend(units("..\\"));
    }
    out.extend_from_slice(rest);
    if out.last() == Some(&BS) && !rest.is_empty() {
        out.pop();
    }
    if rest.is_empty() && out.len() > 1 {
        out.pop();
    }
    Some(out)
}

fn find_suffix(v: &[u32], suffixes: &[Vec<u32>]) -> Option<usize> {
    suffixes.iter().find_map(|s| {
        (v.len() >= s.len()
            && v[v.len() - s.len()..]
                .iter()
                .map(|&c| lower(c))
                .eq(s.iter().map(|&c| lower(c))))
        .then(|| v.len() - s.len())
    })
}

fn env(name: &str) -> Option<Vec<u32>> {
    let n: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut buf = vec![0u16; 32768];
    let len = unsafe {
        crate::dlls::kernel32::get_environment_variable_w(n.as_ptr(), buf.as_mut_ptr(), 32768)
    };
    (len > 0 && (len as usize) < buf.len())
        .then(|| buf[..len as usize].iter().map(|&c| c as u32).collect())
}

/// PathUnExpandEnvStrings : remplace le plus long préfixe connu par sa
/// variable (%USERPROFILE%, %ProgramFiles%…).
fn unexpand_env(v: &[u32]) -> Option<Vec<u32>> {
    let names = [
        "ALLUSERSPROFILE",
        "APPDATA",
        "USERPROFILE",
        "ProgramFiles",
        "SystemRoot",
        "SystemDrive",
    ];
    let mut best: Option<(usize, &str)> = None;
    for name in names {
        let Some(val) = env(name) else { continue };
        if val.is_empty() || !is_prefix(&val, v) {
            continue;
        }
        if best.is_none_or(|(n, _)| val.len() > n) {
            best = Some((val.len(), name));
        }
    }
    let (n, name) = best?;
    let mut out = units(&format!("%{name}%"));
    out.extend_from_slice(&v[n..]);
    Some(out)
}

fn full_path(v: &[u32]) -> Option<Vec<u32>> {
    let w: Vec<u16> = v.iter().map(|&c| c as u16).chain(Some(0)).collect();
    let mut buf = vec![0u16; 32768];
    let n = unsafe {
        crate::dlls::kernel32::get_full_path_name_w(
            w.as_ptr(),
            32768,
            buf.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    };
    (n > 0 && (n as usize) < buf.len()).then(|| {
        canonicalize_units(
            &buf[..n as usize]
                .iter()
                .map(|&c| c as u32)
                .collect::<Vec<_>>(),
        )
    })
}

fn exists(v: &[u32]) -> bool {
    crate::paths::to_linux(&text(v)).exists()
}

fn find_on_path(file: &[u32], dirs: &[Vec<u32>]) -> Option<Vec<u32>> {
    let mut all: Vec<Vec<u32>> = dirs.to_vec();
    if let Some(path) = env("PATH") {
        all.extend(
            path.split(|&c| c == b';' as u32)
                .filter(|d| !d.is_empty())
                .map(<[u32]>::to_vec),
        );
    }
    all.into_iter().find_map(|mut d| {
        if d.last() != Some(&BS) {
            d.push(BS);
        }
        d.extend_from_slice(file);
        exists(&d).then_some(d)
    })
}

// ── Enveloppes A et W ───────────────────────────────────────────────────

/// Fonction qui modifie le chemin sur place et rend un BOOL.
macro_rules! in_place {
    ($a:ident, $w:ident, |$v:ident| $body:expr) => {
        unsafe extern "win64" fn $a(path: *mut u8) -> i32 {
            in_place_impl(path, |$v| $body)
        }
        unsafe extern "win64" fn $w(path: *mut u16) -> i32 {
            in_place_impl(path, |$v| $body)
        }
    };
}
unsafe fn in_place_impl<T: Unit>(path: *mut T, f: impl FnOnce(&mut Vec<u32>) -> bool) -> i32 {
    if path.is_null() {
        return 0;
    }
    let mut v = read(path);
    let r = f(&mut v);
    write(path, &v, MAX_PATH.max(v.len() + 1));
    r as i32
}

/// Prédicat sur un chemin.
macro_rules! query {
    ($a:ident, $w:ident, |$v:ident| $body:expr) => {
        unsafe extern "win64" fn $a(path: *const u8) -> i32 {
            let $v = read(path);
            ($body) as i32
        }
        unsafe extern "win64" fn $w(path: *const u16) -> i32 {
            let $v = read(path);
            ($body) as i32
        }
    };
}

/// Fonction qui rend un pointeur dans le chemin (ou NULL).
macro_rules! pointer {
    ($a:ident, $w:ident, |$v:ident| $body:expr) => {
        unsafe extern "win64" fn $a(path: *const u8) -> *const u8 {
            if path.is_null() {
                return std::ptr::null();
            }
            let $v = read(path);
            let at: Option<usize> = $body;
            at.map_or(std::ptr::null(), |i| path.add(i))
        }
        unsafe extern "win64" fn $w(path: *const u16) -> *const u16 {
            if path.is_null() {
                return std::ptr::null();
            }
            let $v = read(path);
            let at: Option<usize> = $body;
            at.map_or(std::ptr::null(), |i| path.add(i))
        }
    };
}

in_place!(PathStripToRootA, PathStripToRootW, |v| strip_to_root(v));
unsafe extern "win64" fn PathStripPathA(path: *mut u8) -> i32 {
    in_place_impl(path, |v| {
        strip_path(v);
        true
    })
}
in_place!(PathRemoveArgsA, PathRemoveArgsW, |v| {
    remove_args(v);
    true
});
in_place!(PathRemoveBlanksA, PathRemoveBlanksW, |v| {
    remove_blanks(v);
    true
});
in_place!(PathQuoteSpacesA, PathQuoteSpacesW, |v| quote_spaces(v));
in_place!(PathUnquoteSpacesA, PathUnquoteSpacesW, |v| unquote_spaces(
    v
));
in_place!(PathUndecorateA, PathUndecorateW, |v| {
    undecorate(v);
    true
});
in_place!(PathMakePrettyA, PathMakePrettyW, |v| make_pretty(v));

query!(PathIsUNCA, PathIsUNCW, |v| is_unc(&v));
query!(PathIsUNCServerA, PathIsUNCServerW, |v| is_unc_server(&v));
query!(PathIsUNCServerShareA, PathIsUNCServerShareW, |v| {
    is_unc_server_share(&v)
});
query!(PathIsFileSpecA, PathIsFileSpecW, |v| is_file_spec(&v));
query!(PathIsURLA, PathIsURLW, |v| is_url(&v));
query!(
    PathIsLFNFileSpecA,
    PathIsLFNFileSpecW,
    |v| is_lfn_file_spec(&v)
);
unsafe extern "win64" fn PathIsNetworkPathA(path: *const u8) -> i32 {
    read(path).starts_with(&[BS, BS]) as i32
}
unsafe extern "win64" fn PathGetDriveNumberA(path: *const u8) -> i32 {
    drive_number(&read(path))
}
query!(PathIsDirectoryEmptyA, PathIsDirectoryEmptyW, |v| {
    std::fs::read_dir(crate::paths::to_linux(&text(&v)))
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
});
// Aucun dossier n'est marqué « système » par Winlux.
query!(PathIsSystemFolderA, PathIsSystemFolderW, |v| {
    let _ = v;
    false
});

unsafe extern "win64" fn PathSkipRootA(path: *const u8) -> *const u8 {
    if path.is_null() {
        return std::ptr::null();
    }
    skip_root(&read(path)).map_or(std::ptr::null(), |i| path.add(i))
}
pointer!(PathGetArgsA, PathGetArgsW, |v| Some(
    args_start(&v).map_or(v.len(), |i| i + 1)
));
pointer!(PathFindNextComponentA, PathFindNextComponentW, |v| {
    next_component(&v)
});

/// PathParseIconLocation rend l'index, pas un BOOL : enveloppes dédiées.
unsafe extern "win64" fn path_parse_icon_location_a(path: *mut u8) -> i32 {
    parse_icon(path)
}
unsafe extern "win64" fn path_parse_icon_location_w(path: *mut u16) -> i32 {
    parse_icon(path)
}
unsafe fn parse_icon<T: Unit>(path: *mut T) -> i32 {
    if path.is_null() {
        return 0;
    }
    let mut v = read(path);
    let i = parse_icon_location(&mut v);
    write(path, &v, v.len() + 1);
    i
}

unsafe extern "win64" fn PathGetCharTypeA(c: u8) -> u32 {
    char_type(c as u32)
}
unsafe extern "win64" fn PathGetCharTypeW(c: u16) -> u32 {
    char_type(c as u32)
}

unsafe fn two<T: Unit>(a: *const T, b: *const T, f: impl FnOnce(&[u32], &[u32]) -> bool) -> i32 {
    f(&read(a), &read(b)) as i32
}
unsafe extern "win64" fn PathIsPrefixA(prefix: *const u8, path: *const u8) -> i32 {
    two(prefix, path, is_prefix)
}
unsafe extern "win64" fn PathIsPrefixW(prefix: *const u16, path: *const u16) -> i32 {
    two(prefix, path, is_prefix)
}
unsafe extern "win64" fn PathIsSameRootA(a: *const u8, b: *const u8) -> i32 {
    two(a, b, is_same_root)
}
unsafe extern "win64" fn PathIsSameRootW(a: *const u16, b: *const u16) -> i32 {
    two(a, b, is_same_root)
}
/// PathIsContentType : le type MIME déclaré pour l'extension sous
/// HKCR\.ext « Content Type ».
unsafe fn content_type<T: Unit>(path: *const T, ty: *const T) -> i32 {
    let v = read(path);
    let Some(dot) = extension(&v) else { return 0 };
    let key = text(&v[dot..]);
    let want = text(&read(ty));
    let sub: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = "Content Type".encode_utf16().chain(Some(0)).collect();
    let mut buf = [0u16; 128];
    let mut size = 256u32;
    let mut kind = 0u32;
    const HKEY_CLASSES_ROOT: usize = 0x8000_0000;
    let r = crate::dlls::shlwapi_reg::sh_get_value_w(
        HKEY_CLASSES_ROOT,
        sub.as_ptr(),
        name.as_ptr(),
        &mut kind,
        buf.as_mut_ptr() as *mut u8,
        &mut size,
    );
    (r == 0 && crate::dlls::wstr(buf.as_ptr()).eq_ignore_ascii_case(&want)) as i32
}
unsafe extern "win64" fn PathIsContentTypeA(path: *const u8, ty: *const u8) -> i32 {
    content_type(path, ty)
}
unsafe extern "win64" fn PathIsContentTypeW(path: *const u16, ty: *const u16) -> i32 {
    content_type(path, ty)
}

unsafe fn common<T: Unit>(a: *const T, b: *const T, out: *mut T) -> i32 {
    let (a, b) = (read(a), read(b));
    let n = common_prefix(&a, &b);
    if !out.is_null() {
        write(out, &a[..n], MAX_PATH);
    }
    n as i32
}
unsafe extern "win64" fn PathCommonPrefixA(a: *const u8, b: *const u8, out: *mut u8) -> i32 {
    common(a, b, out)
}
unsafe extern "win64" fn PathCommonPrefixW(a: *const u16, b: *const u16, out: *mut u16) -> i32 {
    common(a, b, out)
}

unsafe fn build_root<T: Unit>(out: *mut T, drive: i32) -> *mut T {
    if !out.is_null() {
        if (0..26).contains(&drive) {
            write(out, &[b'A' as u32 + drive as u32, b':' as u32, BS], 4);
        } else {
            *out = T::from(0);
        }
    }
    out
}
unsafe extern "win64" fn PathBuildRootA(out: *mut u8, drive: i32) -> *mut u8 {
    build_root(out, drive)
}
unsafe extern "win64" fn PathBuildRootW(out: *mut u16, drive: i32) -> *mut u16 {
    build_root(out, drive)
}

unsafe fn rename_ext<T: Unit>(path: *mut T, ext: *const T) -> i32 {
    if path.is_null() || ext.is_null() {
        return 0;
    }
    let mut v = read(path);
    if !rename_extension(&mut v, &read(ext)) {
        return 0;
    }
    write(path, &v, MAX_PATH) as i32
}
unsafe extern "win64" fn PathRenameExtensionA(path: *mut u8, ext: *const u8) -> i32 {
    rename_ext(path, ext)
}
unsafe extern "win64" fn PathRenameExtensionW(path: *mut u16, ext: *const u16) -> i32 {
    rename_ext(path, ext)
}

unsafe extern "win64" fn PathAddExtensionA(path: *mut u8, ext: *const u8) -> i32 {
    if path.is_null() {
        return 0;
    }
    let mut v = read(path);
    let e = (!ext.is_null()).then(|| read(ext));
    if !add_extension(&mut v, e) {
        return 0;
    }
    write(path, &v, MAX_PATH) as i32
}

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
unsafe fn relative<T: Unit>(out: *mut T, from: *const T, fa: u32, to: *const T, ta: u32) -> i32 {
    if out.is_null() || from.is_null() || to.is_null() {
        return 0;
    }
    match relative_path_to(
        &read(from),
        fa & FILE_ATTRIBUTE_DIRECTORY != 0,
        &read(to),
        ta & FILE_ATTRIBUTE_DIRECTORY != 0,
    ) {
        Some(r) => write(out, &r, MAX_PATH) as i32,
        None => {
            *out = T::from(0);
            0
        }
    }
}
unsafe extern "win64" fn PathRelativePathToA(
    out: *mut u8,
    from: *const u8,
    fa: u32,
    to: *const u8,
    ta: u32,
) -> i32 {
    relative(out, from, fa, to, ta)
}
unsafe extern "win64" fn PathRelativePathToW(
    out: *mut u16,
    from: *const u16,
    fa: u32,
    to: *const u16,
    ta: u32,
) -> i32 {
    relative(out, from, fa, to, ta)
}

unsafe fn qualify<T: Unit>(path: *const T, out: *mut T, cch: u32) -> i32 {
    if path.is_null() || out.is_null() {
        return 0;
    }
    let v = read(path);
    let full = if is_url(&v) && v.get(1) != Some(&(b':' as u32)) {
        Some(v)
    } else {
        full_path(&v)
    };
    match full {
        Some(f) => write(out, &f, cch as usize) as i32,
        None => 0,
    }
}
unsafe extern "win64" fn PathSearchAndQualifyA(path: *const u8, out: *mut u8, cch: u32) -> i32 {
    qualify(path, out, cch)
}
unsafe extern "win64" fn PathSearchAndQualifyW(path: *const u16, out: *mut u16, cch: u32) -> i32 {
    qualify(path, out, cch)
}

unsafe fn on_path<T: Unit>(file: *mut T, dirs: usize) -> i32 {
    if file.is_null() {
        return 0;
    }
    let mut list = Vec::new();
    let ps = crate::wow32::pointer_size();
    let mut at = dirs;
    while at != 0 {
        let p = if ps == 4 {
            std::ptr::read_unaligned(at as *const u32) as usize
        } else {
            std::ptr::read_unaligned(at as *const u64) as usize
        };
        if p == 0 {
            break;
        }
        list.push(read(p as *const T));
        at += ps;
    }
    match find_on_path(&read(file), &list) {
        Some(found) => write(file, &found, MAX_PATH) as i32,
        None => 0,
    }
}
unsafe extern "win64" fn PathFindOnPathA(file: *mut u8, dirs: usize) -> i32 {
    on_path(file, dirs)
}
unsafe extern "win64" fn PathFindOnPathW(file: *mut u16, dirs: usize) -> i32 {
    on_path(file, dirs)
}

unsafe fn suffix<T: Unit>(path: *const T, array: usize, count: i32) -> *const T {
    if path.is_null() || array == 0 || count <= 0 {
        return std::ptr::null();
    }
    let ps = crate::wow32::pointer_size();
    let list: Vec<Vec<u32>> = (0..count as usize)
        .map(|i| {
            let at = array + i * ps;
            let p = if ps == 4 {
                std::ptr::read_unaligned(at as *const u32) as usize
            } else {
                std::ptr::read_unaligned(at as *const u64) as usize
            };
            read(p as *const T)
        })
        .collect();
    find_suffix(&read(path), &list).map_or(std::ptr::null(), |i| path.add(i))
}
unsafe extern "win64" fn PathFindSuffixArrayA(path: *const u8, array: usize, n: i32) -> *const u8 {
    suffix(path, array, n)
}
unsafe extern "win64" fn PathFindSuffixArrayW(
    path: *const u16,
    array: usize,
    n: i32,
) -> *const u16 {
    suffix(path, array, n)
}

unsafe fn unexpand<T: Unit>(path: *const T, out: *mut T, cch: u32) -> i32 {
    if path.is_null() || out.is_null() {
        return 0;
    }
    match unexpand_env(&read(path)) {
        Some(r) => write(out, &r, cch as usize) as i32,
        None => 0,
    }
}
unsafe extern "win64" fn PathUnExpandEnvStringsA(path: *const u8, out: *mut u8, cch: u32) -> i32 {
    unexpand(path, out, cch)
}
unsafe extern "win64" fn PathUnExpandEnvStringsW(path: *const u16, out: *mut u16, cch: u32) -> i32 {
    unexpand(path, out, cch)
}

/// PathCompactPath : sans mesure de texte, la largeur en pixels est
/// ramenée à une largeur moyenne de 7 pixels par caractère.
unsafe fn compact<T: Unit>(path: *mut T, dx: u32) -> i32 {
    if path.is_null() {
        return 0;
    }
    let v: Vec<u16> = read(path).iter().map(|&c| c as u16).collect();
    let max = (dx as usize / 7).max(4) + 1;
    let out: Vec<u32> = super::common_apps::compact_path(&v, max)
        .iter()
        .map(|&c| c as u32)
        .collect();
    write(path, &out, v.len() + 1) as i32
}
unsafe extern "win64" fn PathCompactPathA(_hdc: usize, path: *mut u8, dx: u32) -> i32 {
    compact(path, dx)
}
unsafe extern "win64" fn PathCompactPathW(_hdc: usize, path: *mut u16, dx: u32) -> i32 {
    compact(path, dx)
}
unsafe extern "win64" fn PathCompactPathExA(
    out: *mut u8,
    src: *const u8,
    max: u32,
    _f: u32,
) -> i32 {
    if out.is_null() || src.is_null() || max == 0 {
        return 0;
    }
    let v: Vec<u16> = read(src).iter().map(|&c| c as u16).collect();
    let r: Vec<u32> = super::common_apps::compact_path(&v, max as usize)
        .iter()
        .map(|&c| c as u32)
        .collect();
    write(out, &r, max as usize) as i32
}

/// PathSetDlgItemPath : le chemin raccourci à la largeur du contrôle,
/// ramenée à 7 pixels par caractère.
unsafe extern "win64" fn PathSetDlgItemPathW(dlg: usize, id: i32, path: *const u16) {
    let mut v: Vec<u16> = read(path).iter().map(|&c| c as u16).collect();
    let item = crate::dlls::user32::get_dlg_item(dlg, id);
    let mut rc = [0i32; 4];
    if item != 0
        && crate::dlls::user32::get_client_rect(
            item,
            rc.as_mut_ptr() as *mut crate::dlls::gui::Rect,
        ) != 0
    {
        let max = ((rc[2] - rc[0]).max(0) as usize / 7).max(4) + 1;
        v = super::common_apps::compact_path(&v, max);
    }
    v.push(0);
    crate::dlls::user32::set_dlg_item_text_w(dlg, id, v.as_ptr());
}
unsafe extern "win64" fn PathSetDlgItemPathA(dlg: usize, id: i32, path: *const u8) {
    let w: Vec<u16> = read(path)
        .iter()
        .map(|&c| c as u16)
        .chain(Some(0))
        .collect();
    PathSetDlgItemPathW(dlg, id, w.as_ptr());
}

// Winlux ne gère pas de dossiers « système » (desktop.ini en lecture seule) :
// marquer ou démarquer réussit sans effet.
unsafe extern "win64" fn PathMakeSystemFolderA(path: *const u8) -> i32 {
    exists(&read(path)) as i32
}
unsafe extern "win64" fn PathMakeSystemFolderW(path: *const u16) -> i32 {
    exists(&read(path)) as i32
}
unsafe extern "win64" fn PathUnmakeSystemFolderA(path: *const u8) -> i32 {
    exists(&read(path)) as i32
}
unsafe extern "win64" fn PathUnmakeSystemFolderW(path: *const u16) -> i32 {
    exists(&read(path)) as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PathStripToRootA" => fn PathStripToRootA, "PathStripToRootW" => fn PathStripToRootW,
        "PathStripPathA" => fn PathStripPathA,
        "PathRemoveArgsA" => fn PathRemoveArgsA, "PathRemoveArgsW" => fn PathRemoveArgsW,
        "PathRemoveBlanksA" => fn PathRemoveBlanksA, "PathRemoveBlanksW" => fn PathRemoveBlanksW,
        "PathQuoteSpacesA" => fn PathQuoteSpacesA, "PathQuoteSpacesW" => fn PathQuoteSpacesW,
        "PathUnquoteSpacesA" => fn PathUnquoteSpacesA, "PathUnquoteSpacesW" => fn PathUnquoteSpacesW,
        "PathUndecorateA" => fn PathUndecorateA, "PathUndecorateW" => fn PathUndecorateW,
        "PathMakePrettyA" => fn PathMakePrettyA, "PathMakePrettyW" => fn PathMakePrettyW,
        "PathParseIconLocationA" => fn path_parse_icon_location_a,
        "PathParseIconLocationW" => fn path_parse_icon_location_w,
        "PathIsUNCA" => fn PathIsUNCA, "PathIsUNCW" => fn PathIsUNCW,
        "PathIsUNCServerA" => fn PathIsUNCServerA, "PathIsUNCServerW" => fn PathIsUNCServerW,
        "PathIsUNCServerShareA" => fn PathIsUNCServerShareA,
        "PathIsUNCServerShareW" => fn PathIsUNCServerShareW,
        "PathIsFileSpecA" => fn PathIsFileSpecA, "PathIsFileSpecW" => fn PathIsFileSpecW,
        "PathIsURLA" => fn PathIsURLA, "PathIsURLW" => fn PathIsURLW,
        "PathIsLFNFileSpecA" => fn PathIsLFNFileSpecA, "PathIsLFNFileSpecW" => fn PathIsLFNFileSpecW,
        "PathIsNetworkPathA" => fn PathIsNetworkPathA,
        "PathGetDriveNumberA" => fn PathGetDriveNumberA,
        "PathIsDirectoryEmptyA" => fn PathIsDirectoryEmptyA,
        "PathIsDirectoryEmptyW" => fn PathIsDirectoryEmptyW,
        "PathIsSystemFolderA" => fn PathIsSystemFolderA, "PathIsSystemFolderW" => fn PathIsSystemFolderW,
        "PathSkipRootA" => fn PathSkipRootA,
        "PathGetArgsA" => fn PathGetArgsA, "PathGetArgsW" => fn PathGetArgsW,
        "PathFindNextComponentA" => fn PathFindNextComponentA,
        "PathFindNextComponentW" => fn PathFindNextComponentW,
        "PathGetCharTypeA" => fn PathGetCharTypeA, "PathGetCharTypeW" => fn PathGetCharTypeW,
        "PathIsPrefixA" => fn PathIsPrefixA, "PathIsPrefixW" => fn PathIsPrefixW,
        "PathIsSameRootA" => fn PathIsSameRootA, "PathIsSameRootW" => fn PathIsSameRootW,
        "PathIsContentTypeA" => fn PathIsContentTypeA, "PathIsContentTypeW" => fn PathIsContentTypeW,
        "PathCommonPrefixA" => fn PathCommonPrefixA, "PathCommonPrefixW" => fn PathCommonPrefixW,
        "PathBuildRootA" => fn PathBuildRootA, "PathBuildRootW" => fn PathBuildRootW,
        "PathRenameExtensionA" => fn PathRenameExtensionA,
        "PathRenameExtensionW" => fn PathRenameExtensionW,
        "PathAddExtensionA" => fn PathAddExtensionA,
        "PathRelativePathToA" => fn PathRelativePathToA, "PathRelativePathToW" => fn PathRelativePathToW,
        "PathSearchAndQualifyA" => fn PathSearchAndQualifyA,
        "PathSearchAndQualifyW" => fn PathSearchAndQualifyW,
        "PathFindOnPathA" => fn PathFindOnPathA, "PathFindOnPathW" => fn PathFindOnPathW,
        "PathFindSuffixArrayA" => fn PathFindSuffixArrayA,
        "PathFindSuffixArrayW" => fn PathFindSuffixArrayW,
        "PathUnExpandEnvStringsA" => fn PathUnExpandEnvStringsA,
        "PathUnExpandEnvStringsW" => fn PathUnExpandEnvStringsW,
        "PathCompactPathA" => fn PathCompactPathA, "PathCompactPathW" => fn PathCompactPathW,
        "PathCompactPathExA" => fn PathCompactPathExA,
        "PathSetDlgItemPathA" => fn PathSetDlgItemPathA, "PathSetDlgItemPathW" => fn PathSetDlgItemPathW,
        "PathMakeSystemFolderA" => fn PathMakeSystemFolderA,
        "PathMakeSystemFolderW" => fn PathMakeSystemFolderW,
        "PathUnmakeSystemFolderA" => fn PathUnmakeSystemFolderA,
        "PathUnmakeSystemFolderW" => fn PathUnmakeSystemFolderW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u32> {
        units(s)
    }
    fn t(v: &[u32]) -> String {
        text(v)
    }

    #[test]
    fn racines_et_elements() {
        for (p, root, ok) in [
            ("C:\\Python\\Lib\\x.py", "C:\\", true),
            ("\\\\srv\\part\\d\\e", "\\\\srv\\part", true),
            ("\\dossier", "\\", true),
            ("relatif\\x", "", false),
        ] {
            let mut v = u(p);
            assert_eq!(strip_to_root(&mut v), ok, "{p}");
            assert_eq!(t(&v), root, "{p}");
        }
        let mut v = u("c:\\dir1\\dir2\\");
        strip_path(&mut v);
        assert_eq!(t(&v), "dir2\\");
        assert!(is_unc(&u("\\\\srv\\x")) && !is_unc(&u("\\\\?\\C:\\x")));
        assert!(is_unc(&u("\\\\?\\UNC\\srv\\x")));
        assert!(is_unc_server(&u("\\\\srv")) && !is_unc_server(&u("\\\\srv\\p")));
        assert!(is_unc_server_share(&u("\\\\srv\\p")) && !is_unc_server_share(&u("\\\\srv\\p\\d")));
        assert!(is_file_spec(&u("a.txt")) && !is_file_spec(&u("c:a.txt")));
        assert_eq!(skip_root(&u("C:\\x")), Some(3));
        assert_eq!(skip_root(&u("C:x")), None);
        assert_eq!(next_component(&u("c:\\a\\b")), Some(3));
        assert_eq!(next_component(&u("b")), Some(1));
        assert_eq!(drive_number(&u("d:\\x")), 3);
    }

    #[test]
    fn prefixes_et_chemins_relatifs() {
        let a = u("C:\\win\\desktop\\temp.txt");
        let b = u("c:\\win\\tray\\sample.txt");
        assert_eq!(common_prefix(&a, &b), 6);
        assert_eq!(common_prefix(&u("C:\\a"), &u("C:\\b")), 3);
        assert_eq!(common_prefix(&u("C:\\ab"), &u("C:\\abc")), 3);
        assert!(is_prefix(&u("C:\\win"), &u("c:\\WIN\\x")));
        assert!(!is_prefix(&u("C:\\wi"), &u("c:\\win\\x")));
        assert!(is_same_root(&u("C:\\a"), &u("c:\\b")));
        assert!(!is_same_root(&u("C:\\a"), &u("D:\\a")));
        let r = relative_path_to(
            &u("C:\\FolderA\\FolderB\\FolderC"),
            true,
            &u("C:\\FolderA\\FolderD\\FolderE"),
            true,
        );
        assert_eq!(t(&r.unwrap()), "..\\..\\FolderD\\FolderE");
        let r = relative_path_to(&u("C:\\a\\f.txt"), false, &u("C:\\a\\b\\g.txt"), false);
        assert_eq!(t(&r.unwrap()), ".\\b\\g.txt");
        assert!(relative_path_to(&u("C:\\a"), true, &u("D:\\a"), true).is_none());
    }

    #[test]
    fn arguments_guillemets_et_extensions() {
        let v = u("\"C:\\Program Files\\x.exe\" -a -b");
        assert_eq!(t(&v[args_start(&v).unwrap() + 1..]), "-a -b");
        let mut v = u("notepad.exe fichier.txt");
        remove_args(&mut v);
        assert_eq!(t(&v), "notepad.exe");
        let mut v = u("  a b  ");
        remove_blanks(&mut v);
        assert_eq!(t(&v), "a b");
        let mut v = u("C:\\a b\\c");
        assert!(quote_spaces(&mut v));
        assert_eq!(t(&v), "\"C:\\a b\\c\"");
        assert!(unquote_spaces(&mut v));
        assert_eq!(t(&v), "C:\\a b\\c");
        let mut v = u("C:\\x\\page[12].htm");
        undecorate(&mut v);
        assert_eq!(t(&v), "C:\\x\\page.htm");
        let mut v = u("C:\\x.y\\f.txt");
        assert!(rename_extension(&mut v, &u(".doc")));
        assert_eq!(t(&v), "C:\\x.y\\f.doc");
        let mut v = u("C:\\x.y\\f");
        assert!(add_extension(&mut v, None));
        assert_eq!(t(&v), "C:\\x.y\\f.exe");
        let mut v = u("\"C:\\w\\shell32.dll\",-3");
        assert_eq!(parse_icon_location(&mut v), -3);
        assert_eq!(t(&v), "C:\\w\\shell32.dll");
        assert!(is_url(&u("https://x")) && !is_url(&u("C:\\x")));
        assert!(is_lfn_file_spec(&u("un long nom.txt")) && !is_lfn_file_spec(&u("CONFIG.SYS")));
        let mut v = u("C:\\WINDOWS\\X");
        assert!(make_pretty(&mut v));
        assert_eq!(t(&v), "c:\\windows\\x");
        assert_eq!(char_type(b'*' as u32), 4);
        assert_eq!(char_type(b'\\' as u32), 8);
        assert_eq!(
            find_suffix(&u("C:\\a\\b.TXT"), &[u(".doc"), u("b.txt")]),
            Some(5)
        );
    }

    #[test]
    fn enveloppes_a_et_w() {
        unsafe {
            let mut w: Vec<u16> = "C:\\Python312\\Lib\\os.py"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            w.resize(260, 0);
            assert_eq!(PathStripToRootW(w.as_mut_ptr()), 1);
            assert_eq!(crate::dlls::wstr(w.as_ptr()), "C:\\");
            let mut a = b"c:\\dir\\file.txt\0".to_vec();
            a.resize(260, 0);
            assert_eq!(PathStripPathA(a.as_mut_ptr()), 1);
            assert_eq!(crate::dlls::cstr(a.as_ptr()), "file.txt");
            let mut root = [0u16; 4];
            PathBuildRootW(root.as_mut_ptr(), 2);
            assert_eq!(crate::dlls::wstr(root.as_ptr()), "C:\\");
            let p = b"C:\\a\\b\0";
            let r = PathSkipRootA(p.as_ptr());
            assert_eq!(crate::dlls::cstr(r), "a\\b");
        }
    }
}
