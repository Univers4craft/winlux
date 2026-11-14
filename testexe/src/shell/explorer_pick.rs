//! L'Explorateur comme boîte « Ouvrir » / « Enregistrer sous » des
//! programmes .exe : un seul Explorateur dans Winlux (#598).
//!
//! `GetOpenFileName` / `GetSaveFileName` (runtime, comdlg32_files.rs)
//! lancent `winlux-shell-app explorer <dossier>` avec les variables
//! `WINLUX_PICK*`. L'Explorateur garde tout ce qu'il sait faire (volet de
//! navigation, onglets, recherche, affichages, menus) et ajoute en bas, à la
//! place de la barre d'état, le champ « Nom du fichier », le type de fichier
//! et les boutons Ouvrir (ou Enregistrer) et Annuler, comme sous Windows.
//! Le choix est écrit dans `WINLUX_PICK_RESULT` : le numéro du type (à
//! partir de 1), puis un chemin Windows par ligne.

use super::*;

/// Types de fichiers retenus : IrfanView en passe plus de 16, avec des
/// motifs de plus de 1 000 caractères (« Tous les formats ») (#1690).
const MAX_FILTERS: usize = 96;
const DESC_LEN: usize = 128;
const PAT_LEN: usize = 2048;
/// Lignes visibles de la liste des types ; au-delà elle défile.
const LIST_ROWS: usize = 12;
pub(super) const PANEL_H: i32 = 96;
const ROW_H: i32 = 28;

#[derive(Clone, Copy)]
struct Filter {
    desc: [u8; DESC_LEN],
    dl: usize,
    pat: [u8; PAT_LEN],
    pl: usize,
}
const EMPTY_FILTER: Filter = Filter {
    desc: [0; DESC_LEN],
    dl: 0,
    pat: [0; PAT_LEN],
    pl: 0,
};

const OFN_OVERWRITEPROMPT: u32 = 0x2;
const OFN_ALLOWMULTISELECT: u32 = 0x200;
const OFN_PATHMUSTEXIST: u32 = 0x800;
const OFN_FILEMUSTEXIST: u32 = 0x1000;
const OFN_CREATEPROMPT: u32 = 0x2000;

static mut ACTIVE: bool = false;
static mut SAVE: bool = false;
/// FOS_PICKFOLDERS : la boîte choisit un dossier, pas un fichier.
static mut FOLDER: bool = false;
static mut FLAGS: u32 = 0;
/// Titre du programme (lpstrTitle, SetTitle) ou consigne de SHBrowseForFolder
/// (lpszTitle), qui peut tenir une phrase entière.
static mut TITLE: [u8; 260] = [0; 260];
static mut TITLE_LEN: usize = 0;
static mut FILTERS: [Filter; MAX_FILTERS] = [EMPTY_FILTER; MAX_FILTERS];
static mut NFILTERS: usize = 0;
static mut CUR: usize = 0;
/// Motif tapé dans « Nom du fichier » (`*.log`), prioritaire sur le type.
static mut TYPED: Field = EMPTY_FIELD;
pub(super) static mut NAME: Field = EMPTY_FIELD;
static mut DEFEXT: [u8; 32] = [0; 32];
static mut DEFEXT_LEN: usize = 0;
static mut RESULT: [u16; PATH_LEN + NAME_LEN + 2] = [0; PATH_LEN + NAME_LEN + 2];
static mut SIG: (usize, usize, u16, u8) = (usize::MAX, 0, 0, 0);
static mut SIG_READY: bool = false;
static mut LIST_OPEN: bool = false;
/// Premier type affiché dans la liste ouverte.
static mut LIST_TOP: usize = 0;
/// Une variable d'environnement tient jusqu'à 32 767 caractères.
static mut ENV_W: [u16; 32768] = [0; 32768];
static mut FILTERS_TEXT: [u8; 32768] = [0; 32768];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    None,
    /// Message d'erreur, bouton OK.
    Info,
    /// Question Oui / Non avant de rendre `PENDING`.
    Confirm,
}
static mut ASK: Ask = Ask::None;
static mut ASK_TEXT: [u8; 600] = [0; 600];
static mut ASK_LEN: usize = 0;
static mut PENDING: Path = EMPTY_PATH;

pub(super) fn active() -> bool {
    unsafe { ACTIVE }
}

/// Lit une variable d'environnement (UTF-16) en Windows-1252/Latin-1.
unsafe fn env(name: &core::ffi::CStr, out: &mut [u8]) -> Option<usize> {
    let mut wname = [0u16; 32];
    for (i, &c) in name.to_bytes().iter().enumerate().take(31) {
        wname[i] = c as u16;
    }
    // Tampon statique de 32 768 caractères : la liste des types d'un
    // programme comme IrfanView dépasse largement 2 048 caractères, et elle
    // était alors ignorée au profit de « Tous les fichiers » (#1690).
    let w = &mut *core::ptr::addr_of_mut!(ENV_W);
    let n = GetEnvironmentVariableW(wname.as_ptr(), w.as_mut_ptr(), w.len() as u32) as usize;
    if n == 0 || n >= w.len() {
        return None;
    }
    let n = n.min(out.len());
    for i in 0..n {
        out[i] = ansi_char(w[i]).unwrap_or(b'?');
    }
    Some(n)
}

fn num(s: &[u8]) -> u32 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u32, |a, &c| a.saturating_mul(10).saturating_add((c - b'0') as u32))
}

/// Lit `WINLUX_PICK*` : rien à faire hors d'une boîte Ouvrir/Enregistrer.
pub(super) unsafe fn init() {
    let mut mode = [0u8; 8];
    let Some(n) = env(c"WINLUX_PICK", &mut mode) else {
        return;
    };
    let mut result = [0u8; PATH_LEN];
    let Some(rn) = env(c"WINLUX_PICK_RESULT", &mut result).filter(|&n| n > 2) else {
        return;
    };
    for (i, &c) in result[..rn].iter().enumerate() {
        RESULT[i] = c as u16;
    }
    RESULT[rn] = 0;
    ACTIVE = true;
    SAVE = &mode[..n] == b"save";
    FOLDER = &mode[..n] == b"folder";
    let mut buf = [0u8; 2048];
    if let Some(n) = env(c"WINLUX_PICK_FLAGS", &mut buf) {
        FLAGS = num(&buf[..n]);
    }
    if let Some(n) = env(c"WINLUX_PICK_TITLE", &mut buf) {
        TITLE_LEN = n.min(TITLE.len());
        TITLE[..TITLE_LEN].copy_from_slice(&buf[..TITLE_LEN]);
    }
    if let Some(n) = env(c"WINLUX_PICK_NAME", &mut buf) {
        NAME.set(&buf[..n]);
    }
    if let Some(n) = env(c"WINLUX_PICK_DEFEXT", &mut buf) {
        let e = trim(&buf[..n]);
        let e = e.strip_prefix(b".").unwrap_or(e);
        DEFEXT_LEN = e.len().min(DEFEXT.len());
        DEFEXT[..DEFEXT_LEN].copy_from_slice(&e[..DEFEXT_LEN]);
    }
    NFILTERS = 0;
    let text = &mut *core::ptr::addr_of_mut!(FILTERS_TEXT);
    if let Some(n) = env(c"WINLUX_PICK_FILTERS", text) {
        for line in text[..n].split(|&c| c == b'\n') {
            if NFILTERS == MAX_FILTERS {
                break;
            }
            let Some(tab) = line.iter().position(|&c| c == b'\t') else {
                continue;
            };
            let f = &mut FILTERS[NFILTERS];
            f.dl = tab.min(DESC_LEN);
            f.desc[..f.dl].copy_from_slice(&line[..f.dl]);
            let pat = &line[tab + 1..];
            f.pl = pat.len().min(PAT_LEN);
            f.pat[..f.pl].copy_from_slice(&pat[..f.pl]);
            NFILTERS += 1;
        }
    }
    if NFILTERS == 0 {
        let f = &mut FILTERS[0];
        let d = t4([b"Tous les fichiers (*.*)", b"All files (*.*)", b"Todos los archivos (*.*)", b"Alle Dateien (*.*)"]);
        f.dl = d.len();
        f.desc[..f.dl].copy_from_slice(d);
        f.pl = 3;
        f.pat[..3].copy_from_slice(b"*.*");
        NFILTERS = 1;
    }
    CUR = 0;
    if let Some(n) = env(c"WINLUX_PICK_FILTER", &mut buf) {
        CUR = (num(&buf[..n]) as usize).saturating_sub(1).min(NFILTERS - 1);
    }
    LIST_TOP = 0;
    keep_visible();
    FOCUS = Focus::FileName;
    // Les programmes lancés depuis la boîte (Nouvelle fenêtre, Terminal…)
    // ne doivent pas devenir à leur tour une boîte Ouvrir.
    for name in [
        c"WINLUX_PICK",
        c"WINLUX_PICK_RESULT",
        c"WINLUX_PICK_FLAGS",
        c"WINLUX_PICK_TITLE",
        c"WINLUX_PICK_NAME",
        c"WINLUX_PICK_DEFEXT",
        c"WINLUX_PICK_FILTERS",
        c"WINLUX_PICK_FILTER",
    ] {
        SetEnvironmentVariableA(name.as_ptr(), core::ptr::null());
    }
}

/// Titre de la fenêtre : « Ouvrir », « Enregistrer sous » ou celui du programme.
pub(super) fn title() -> Option<&'static [u8]> {
    unsafe {
        if !ACTIVE {
            return None;
        }
        if TITLE_LEN > 0 {
            return Some(&*core::ptr::slice_from_raw_parts(core::ptr::addr_of!(TITLE) as *const u8, TITLE_LEN));
        }
        Some(if FOLDER {
            t4([b"S\xE9lectionner un dossier", b"Select Folder", b"Seleccionar carpeta", b"Ordner ausw\xE4hlen"])
        } else if SAVE {
            t4([b"Enregistrer sous", b"Save As", b"Guardar como", b"Speichern unter"])
        } else {
            t4([b"Ouvrir", b"Open", b"Abrir", b"\xD6ffnen"])
        })
    }
}

fn trim(s: &[u8]) -> &[u8] {
    let a = s.iter().position(|&c| c != b' ').unwrap_or(s.len());
    let b = s.iter().rposition(|&c| c != b' ').map_or(a, |i| i + 1);
    &s[a..b]
}

fn lower(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' | 0xC0..=0xD6 | 0xD8..=0xDE => c + 32,
        _ => c,
    }
}

/// Joker Windows (`*`, `?`), sans casse. `*.*` prend aussi les noms sans point.
pub(super) fn wildcard(pat: &[u8], name: &[u8]) -> bool {
    if pat == b"*.*" || pat == b"*" {
        return true;
    }
    let (mut p, mut n) = (0, 0);
    let (mut star, mut mark) = (usize::MAX, 0);
    while n < name.len() {
        if p < pat.len() && (pat[p] == b'?' || lower(pat[p]) == lower(name[n])) {
            p += 1;
            n += 1;
        } else if p < pat.len() && pat[p] == b'*' {
            star = p;
            p += 1;
            mark = n;
        } else if star != usize::MAX {
            p = star + 1;
            mark += 1;
            n = mark;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == b'*' {
        p += 1;
    }
    // « *. » : nom sans extension.
    p == pat.len() || (pat[p..] == *b"." && !name.contains(&b'.'))
}

pub(super) fn matches_any(patterns: &[u8], name: &[u8]) -> bool {
    patterns
        .split(|&c| c == b';')
        .map(trim)
        .filter(|p| !p.is_empty())
        .any(|p| wildcard(p, name))
}

/// Fichier affiché par la boîte : les dossiers restent toujours visibles.
pub(super) unsafe fn shows(name: &[u8], dir: bool) -> bool {
    if !ACTIVE || dir {
        return true;
    }
    if FOLDER {
        // Choix d'un dossier : les fichiers sont masqués, comme sous Windows.
        return false;
    }
    if TYPED.len > 0 {
        return matches_any(TYPED.get(), name);
    }
    let f = &FILTERS[CUR];
    matches_any(&f.pat[..f.pl], name)
}

/// Chemin complet d'un nom tapé, relatif au dossier affiché.
pub(super) fn resolve(dir: &[u8], name: &[u8], out: &mut Path) -> bool {
    let mut buf = [0u8; PATH_LEN + 4];
    let mut n = 0;
    let put = |buf: &mut [u8; PATH_LEN + 4], n: &mut usize, c: u8| {
        if *n < PATH_LEN {
            buf[*n] = if c == b'/' { b'\\' } else { c };
            *n += 1;
        }
    };
    let abs = (name.len() >= 2 && name[1] == b':') || name.starts_with(b"\\\\");
    let rooted = !abs && matches!(name.first(), Some(b'\\' | b'/'));
    if !abs && (dir.len() < 2 || dir[1] != b':' || !dir[0].is_ascii_alphabetic()) {
        return false; // Accueil, Ce PC : un nom seul ne désigne rien.
    }
    let rest: &[u8] = if abs {
        for &c in name {
            put(&mut buf, &mut n, c);
        }
        &[]
    } else if rooted {
        put(&mut buf, &mut n, dir[0]);
        put(&mut buf, &mut n, b':');
        name
    } else {
        for &c in dir {
            put(&mut buf, &mut n, c);
        }
        name
    };
    for part in rest.split(|&c| c == b'\\' || c == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                let floor = 3;
                while n > floor && buf[n - 1] != b'\\' {
                    n -= 1;
                }
                if n > floor {
                    n -= 1;
                }
            }
            p => {
                if n == 0 || buf[n - 1] != b'\\' {
                    put(&mut buf, &mut n, b'\\');
                }
                for &c in p {
                    put(&mut buf, &mut n, c);
                }
            }
        }
    }
    if n == 2 {
        put(&mut buf, &mut n, b'\\');
    }
    while n > 3 && buf[n - 1] == b'\\' {
        n -= 1;
    }
    n < PATH_LEN && out.set(&buf[..n])
}

fn exists(path: &[u8]) -> bool {
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    unsafe { GetFileAttributesW(wstr(&mut w, path)) != u32::MAX }
}

unsafe fn sync_from_selection() {
    let p = pane();
    let first = first_sel(p).unwrap_or(usize::MAX);
    let sig = (first, sel_count(p), p.loc.len, p.loc.get().last().copied().unwrap_or(0));
    if !SIG_READY {
        // Ouverture : la saisie va au nom du fichier, comme sous Windows.
        SIG = sig;
        SIG_READY = true;
        FOCUS = Focus::FileName;
        return;
    }
    if sig == SIG {
        return;
    }
    SIG = sig;
    if FOLDER {
        // Le dossier sélectionné devient le choix ; aucun, le dossier affiché.
        NAME = EMPTY_FIELD;
        if sel_count(p) == 1 && p.entries[first].dir {
            NAME.set(p.entries[first].name());
        }
        return;
    }
    let multi = !SAVE && FLAGS & OFN_ALLOWMULTISELECT != 0;
    let mut files = 0;
    for i in 0..p.count {
        if is_sel(p, i) && !p.entries[i].dir {
            files += 1;
        }
    }
    if files == 0 {
        return;
    }
    let mut f = EMPTY_FIELD;
    for i in 0..p.count {
        if !is_sel(p, i) || p.entries[i].dir {
            continue;
        }
        let name = p.entries[i].name();
        if files == 1 || !multi {
            f.set(name);
            break;
        }
        if f.len > 0 {
            f.push(b' ');
        }
        f.push(b'"');
        for &c in name {
            f.push(c);
        }
        f.push(b'"');
    }
    NAME = f;
}

fn push(buf: &mut [u8], n: &mut usize, s: &[u8]) {
    let k = s.len().min(buf.len().saturating_sub(*n));
    buf[*n..*n + k].copy_from_slice(&s[..k]);
    *n += k;
}

unsafe fn ask(kind: Ask, parts: &[&[u8]]) {
    ASK_LEN = 0;
    for part in parts {
        let mut n = ASK_LEN;
        push(&mut *core::ptr::addr_of_mut!(ASK_TEXT), &mut n, part);
        ASK_LEN = n;
    }
    ASK = kind;
    LIST_OPEN = false;
}

/// Bouton Ouvrir / Enregistrer, Entrée, double-clic sur un fichier.
pub(super) unsafe fn accept() {
    LIST_OPEN = false;
    if FOLDER {
        accept_folder();
        return;
    }
    let text = NAME;
    let text = trim(text.get());
    let p = pane();
    if text.is_empty() {
        if sel_count(p) == 1 {
            if let Some(i) = first_sel(p).filter(|&i| p.entries[i].dir) {
                open_item(p, i, false);
            }
        }
        return;
    }
    if text.contains(&b'*') || text.contains(&b'?') {
        TYPED.set(text);
        NAME.len = 0;
        rebuild_view(p);
        p.scroll = 0;
        return;
    }
    let multi = !SAVE && FLAGS & OFN_ALLOWMULTISELECT != 0 && text.first() == Some(&b'"');
    let loc = p.loc;
    let mut out = [EMPTY_PATH; 64];
    let mut nout = 0;
    let mut names = [&b""[..]; 64];
    let mut nnames = 0;
    if multi {
        for part in text.split(|&c| c == b'"') {
            let part = trim(part);
            if !part.is_empty() && nnames < names.len() {
                names[nnames] = part;
                nnames += 1;
            }
        }
    } else {
        let t = text.strip_prefix(b"\"").unwrap_or(text);
        names[0] = t.strip_suffix(b"\"").unwrap_or(t);
        nnames = 1;
    }
    for &name in &names[..nnames] {
        let mut full = EMPTY_PATH;
        if !resolve(loc.get(), name, &mut full) {
            ask(Ask::Info, &[name, b"\n", t4([
                b"Choisissez d'abord un dossier, par exemple Documents.",
                b"Choose a folder first, for example Documents.",
                b"Elija primero una carpeta, por ejemplo Documentos.",
                b"W\xE4hlen Sie zuerst einen Ordner, zum Beispiel Dokumente.",
            ])]);
            return;
        }
        if exists_dir(full.get()) {
            if nnames == 1 {
                NAME.len = 0;
                navigate(full.get());
                return;
            }
            continue;
        }
        let file = last_component(full.get());
        if SAVE && DEFEXT_LEN > 0 && !file.contains(&b'.') {
            let mut b = [0u8; PATH_LEN + 40];
            let mut n = 0;
            push(&mut b, &mut n, full.get());
            push(&mut b, &mut n, b".");
            push(&mut b, &mut n, &DEFEXT[..DEFEXT_LEN]);
            if !full.set(&b[..n]) {
                return;
            }
        }
        let parent_ok = parent(full.get()).is_some_and(exists_dir);
        if !parent_ok && (SAVE || FLAGS & (OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST) != 0) {
            ask(Ask::Info, &[full.get(), b"\n", t4([
                b"Chemin introuvable. V\xE9rifiez le chemin et r\xE9essayez.",
                b"Path not found. Check the path and try again.",
                b"No se encuentra la ruta. Compruebe la ruta e int\xE9ntelo de nuevo.",
                b"Pfad nicht gefunden. \xDCberpr\xFCfen Sie den Pfad, und versuchen Sie es erneut.",
            ])]);
            return;
        }
        let here = exists(full.get());
        if !SAVE && !here && FLAGS & OFN_FILEMUSTEXIST != 0 {
            ask(Ask::Info, &[name, b"\n", t4([
                b"Fichier introuvable. V\xE9rifiez le nom du fichier et r\xE9essayez.",
                b"File not found. Check the file name and try again.",
                b"No se encuentra el archivo. Compruebe el nombre e int\xE9ntelo de nuevo.",
                b"Datei nicht gefunden. \xDCberpr\xFCfen Sie den Dateinamen, und versuchen Sie es erneut.",
            ])]);
            return;
        }
        if nnames == 1 {
            let file = last_component(full.get());
            let question: Option<&[u8]> = if SAVE && here && FLAGS & OFN_OVERWRITEPROMPT != 0 {
                Some(t4([
                    b" existe d\xE9j\xE0.\nVoulez-vous le remplacer ?",
                    b" already exists.\nDo you want to replace it?",
                    b" ya existe.\n\xBFDesea reemplazarlo?",
                    b" ist bereits vorhanden.\nM\xF6chten Sie sie ersetzen?",
                ]))
            } else if !here && FLAGS & OFN_CREATEPROMPT != 0 {
                Some(t4([
                    b" n'existe pas.\nVoulez-vous le cr\xE9er ?",
                    b" doesn't exist.\nDo you want to create it?",
                    b" no existe.\n\xBFDesea crearlo?",
                    b" ist nicht vorhanden.\nM\xF6chten Sie sie erstellen?",
                ]))
            } else {
                None
            };
            if let Some(q) = question {
                PENDING = full;
                ask(Ask::Confirm, &[file, q]);
                return;
            }
        }
        if nout < out.len() {
            out[nout] = full;
            nout += 1;
        }
    }
    if nout > 0 {
        finish(&out[..nout]);
    }
}

/// « Sélectionner un dossier » : le dossier nommé (relatif au dossier
/// affiché), ou le dossier affiché lui-même quand le champ est vide.
unsafe fn accept_folder() {
    let text = NAME;
    let text = trim(text.get());
    let t = text.strip_prefix(b"\"").unwrap_or(text);
    let t = t.strip_suffix(b"\"").unwrap_or(t);
    let loc = pane().loc;
    let mut full = EMPTY_PATH;
    if !resolve(loc.get(), if t.is_empty() { b"." } else { t }, &mut full) {
        ask(Ask::Info, &[t4([
            b"Choisissez d'abord un dossier, par exemple Documents.",
            b"Choose a folder first, for example Documents.",
            b"Elija primero una carpeta, por ejemplo Documentos.",
            b"W\xE4hlen Sie zuerst einen Ordner, zum Beispiel Dokumente.",
        ])]);
        return;
    }
    if !exists_dir(full.get()) {
        ask(Ask::Info, &[full.get(), b"\n", t4([
            b"Chemin introuvable. V\xE9rifiez le chemin et r\xE9essayez.",
            b"Path not found. Check the path and try again.",
            b"No se encuentra la ruta. Compruebe la ruta e int\xE9ntelo de nuevo.",
            b"Pfad nicht gefunden. \xDCberpr\xFCfen Sie den Pfad, und versuchen Sie es erneut.",
        ])]);
        return;
    }
    finish(&[full]);
}

/// Écrit le choix (UTF-16 → Latin-1, comme les chemins de l'Explorateur) et
/// ferme la boîte.
unsafe fn finish(paths: &[Path]) -> ! {
    let h = CreateFileW(
        core::ptr::addr_of!(RESULT) as *const u16,
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0x80,
        core::ptr::null_mut(),
    );
    if h as isize != -1 && !h.is_null() {
        let mut line = [0u8; PATH_LEN + 2];
        let mut n = write_num(&mut line, 0, CUR as u64 + 1);
        line[n] = b'\n';
        n += 1;
        let mut done = 0u32;
        WriteFile(h, line.as_ptr(), n as u32, &mut done, core::ptr::null_mut());
        for p in paths {
            let s = p.get();
            line[..s.len()].copy_from_slice(s);
            line[s.len()] = b'\n';
            WriteFile(h, line.as_ptr(), s.len() as u32 + 1, &mut done, core::ptr::null_mut());
        }
        CloseHandle(h);
    }
    ExitProcess(0)
}

/// Annuler, Échap, ou fermeture de la fenêtre : aucun fichier.
pub(super) unsafe fn cancel() -> ! {
    ExitProcess(0)
}

/// Double-clic ou Entrée sur un fichier de la liste.
pub(super) unsafe fn choose(p: &Pane, index: usize) {
    if !is_sel(p, index) {
        NAME.set(p.entries[index].name());
    } else {
        SIG_READY = true;
        SIG = (usize::MAX, 0, 0, 0);
        sync_from_selection();
    }
    accept();
}

// ---------- Dessin ----------

struct Layout {
    label_x: i32,
    name: Rect,
    kind: Rect,
    ok: Rect,
    cancel: Rect,
}

fn layout(s: &Rect) -> Layout {
    let top = s.top + 12;
    // Choix d'un dossier : pas de liste des types, le champ prend la place.
    let kind = if unsafe { FOLDER } {
        rect(s.right + 4096, top, s.right + 4096, top + 32)
    } else {
        rect(s.right - 300, top, s.right - 16, top + 32)
    };
    let name = rect(s.left + 150, top, kind.left.min(s.right - 4) - 12, top + 32);
    let cancel = rect(s.right - 116, s.bottom - 44, s.right - 16, s.bottom - 12);
    let ok_w = if unsafe { FOLDER } { 200 } else { 100 };
    let ok = rect(cancel.left - 8 - ok_w, cancel.top, cancel.left - 8, cancel.bottom);
    Layout {
        label_x: s.left + 16,
        name,
        kind,
        ok,
        cancel,
    }
}

/// Lignes de la liste ouverte.
fn list_rows() -> usize {
    unsafe { NFILTERS }.min(LIST_ROWS)
}

/// Fait défiler la liste pour que le type courant soit visible.
unsafe fn keep_visible() {
    let rows = list_rows();
    if CUR < LIST_TOP {
        LIST_TOP = CUR;
    } else if CUR >= LIST_TOP + rows {
        LIST_TOP = CUR + 1 - rows;
    }
    LIST_TOP = LIST_TOP.min(NFILTERS - rows);
}

fn list_rect(l: &Layout) -> Rect {
    let h = list_rows() as i32 * ROW_H + 8;
    rect(l.kind.left, l.kind.top - h - 2, l.kind.right, l.kind.top - 2)
}

fn ok_label() -> &'static [u8] {
    if unsafe { FOLDER } {
        t4([b"S\xE9lectionner un dossier", b"Select Folder", b"Seleccionar carpeta", b"Ordner ausw\xE4hlen"])
    } else if unsafe { SAVE } {
        t4([b"Enregistrer", b"Save", b"Guardar", b"Speichern"])
    } else {
        t4([b"Ouvrir", b"Open", b"Abrir", b"\xD6ffnen"])
    }
}

unsafe fn field_box(g: &mut Gfx, r: &Rect, active: bool) {
    let p = pal();
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 5, if active { p.accent } else { p.border });
    round_rect(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - if active { 2 } else { 1 },
        4,
        p.surface,
    );
}

/// Bas de la fenêtre, à la place de la barre d'état.
pub(super) unsafe fn draw(g: &mut Gfx, s: &Rect) {
    sync_from_selection();
    let p = pal();
    fill(g.hdc, g.b, s.left, s.top, s.right, s.bottom, p.window);
    fill(g.hdc, g.b, s.left, s.top, s.right, s.top + 1, p.border);
    let l = layout(s);
    let label = if FOLDER {
        t4([b"Dossier :", b"Folder:", b"Carpeta:", b"Ordner:"])
    } else {
        t4([b"Nom du fichier :", b"File name:", b"Nombre:", b"Dateiname:"])
    };
    text_fit(g.hdc, g.f.body, 13, p.text, l.label_x, l.name.top + 8, label, l.name.left - l.label_x - 8);
    let active = FOCUS == Focus::FileName;
    field_box(g, &l.name, active);
    let w = l.name.right - l.name.left - 24;
    text_fit(g.hdc, g.f.body, 13, p.text, l.name.left + 12, l.name.top + 8, NAME.get(), w);
    if active && GetTickCount() / 530 % 2 == 0 {
        let cx = l.name.left + 12 + text_width(NAME.get(), 13).min(w);
        fill(g.hdc, g.b, cx, l.name.top + 7, cx + crate::gfx::caret_w(1), l.name.bottom - 7, p.text);
    }
    // Ce que le programme demande (son titre, ou la consigne lpszTitle de
    // SHBrowseForFolder) : la fenêtre n'a pas de barre de titre, la boîte
    // l'affiche sur la ligne des boutons (#2400).
    if let Some(t) = title() {
        let w = l.ok.left - l.label_x - 16;
        if w > 40 {
            text_fit(g.hdc, g.f.body, 13, p.text, l.label_x, l.ok.top + 8, t, w);
        }
    }
    if FOLDER {
        copy::button(g, &l.ok, ok_label(), true);
        copy::button(g, &l.cancel, t4([b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]), false);
        return;
    }
    // Type de fichier : liste déroulante.
    field_box(g, &l.kind, LIST_OPEN);
    let kind: &[u8] = if TYPED.len > 0 {
        TYPED.get()
    } else {
        let f = &FILTERS[CUR];
        &f.desc[..f.dl]
    };
    text_fit(g.hdc, g.f.body, 13, p.text, l.kind.left + 12, l.kind.top + 8, kind, l.kind.right - l.kind.left - 44);
    set_glyph_bg(Some(p.surface));
    glyph(g.hdc, g.b, g.f, Glyph::ChevronDown, l.kind.right - 26, l.kind.top + 10, 12);
    set_glyph_bg(None);
    copy::button(g, &l.ok, ok_label(), true);
    copy::button(g, &l.cancel, t4([b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]), false);
    if LIST_OPEN {
        let r = list_rect(&l);
        round_rect(g.hdc, g.b, r.left - 1, r.top - 1, r.right + 1, r.bottom + 1, 7, p.border);
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, p.surface);
        for k in 0..list_rows() {
            let i = LIST_TOP + k;
            let row = rect(r.left + 4, r.top + 4 + k as i32 * ROW_H, r.right - 4, r.top + 4 + (k as i32 + 1) * ROW_H);
            if i == CUR && TYPED.len == 0 {
                round_rect(g.hdc, g.b, row.left, row.top, row.right, row.bottom, 4, p.selected);
            } else if hovered(&row) {
                round_rect(g.hdc, g.b, row.left, row.top, row.right, row.bottom, 4, p.hover);
            }
            let f = &FILTERS[i];
            text_fit(g.hdc, g.f.body, 13, p.text, row.left + 10, row.top + 6, &f.desc[..f.dl], row.right - row.left - 20);
        }
    }
}

fn ask_box(f: &Frame) -> Rect {
    let (w, h) = (460, 176);
    let cx = (f.client.left + f.client.right) / 2;
    let cy = (f.client.top + f.client.bottom) / 2;
    rect(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
}

fn ask_buttons(r: &Rect) -> (Rect, Rect) {
    let no = rect(r.right - 108, r.bottom - 44, r.right - 16, r.bottom - 14);
    let yes = rect(no.left - 100, no.top, no.left - 8, no.bottom);
    (yes, no)
}

/// Message ou question au-dessus de la fenêtre (remplace la MessageBox).
pub(super) unsafe fn draw_overlay(g: &mut Gfx, f: &Frame) {
    if !ACTIVE || ASK == Ask::None {
        return;
    }
    let p = pal();
    let r = ask_box(f);
    round_rect(g.hdc, g.b, r.left - 1, r.top - 1, r.right + 1, r.bottom + 1, 9, p.border);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    let caption = title().unwrap_or(b"");
    text(g.hdc, g.f.semibold, p.text, r.left + 20, r.top + 18, caption);
    let msg = &*core::ptr::slice_from_raw_parts(core::ptr::addr_of!(ASK_TEXT) as *const u8, ASK_LEN);
    for (i, line) in msg.split(|&c| c == b'\n').take(3).enumerate() {
        text_fit(g.hdc, g.f.caption, 12, p.text, r.left + 20, r.top + 52 + i as i32 * 22, line, r.right - r.left - 40);
    }
    let (yes, no) = ask_buttons(&r);
    if ASK == Ask::Confirm {
        copy::button(g, &yes, t4([b"Oui", b"Yes", b"S\xED", b"Ja"]), true);
        copy::button(g, &no, t4([b"Non", b"No", b"No", b"Nein"]), false);
    } else {
        copy::button(g, &no, b"OK", true);
    }
}

unsafe fn answer(yes: bool) {
    let was = ASK;
    ASK = Ask::None;
    if was == Ask::Confirm && yes {
        let one = [PENDING];
        finish(&one);
    }
}

/// Souris : rend vrai si le clic est pris par la boîte.
pub(super) unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame, status: &Rect) -> bool {
    if !ACTIVE {
        return false;
    }
    let down = matches!(ev, Mouse::Down | Mouse::DoubleClick);
    if ASK != Ask::None {
        if down {
            let (yes, no) = ask_buttons(&ask_box(f));
            if inside(&no, x, y) {
                answer(ASK == Ask::Info);
            } else if ASK == Ask::Confirm && inside(&yes, x, y) {
                answer(true);
            }
        }
        return true;
    }
    let l = layout(status);
    // Molette sur la liste ouverte : elle défile (#1690).
    if let (true, Mouse::Wheel(d)) = (LIST_OPEN, ev) {
        let max = NFILTERS - list_rows();
        LIST_TOP = if d > 0 { LIST_TOP.saturating_sub(3) } else { (LIST_TOP + 3).min(max) };
        return true;
    }
    if LIST_OPEN && down {
        let r = list_rect(&l);
        LIST_OPEN = false;
        if inside(&r, x, y) {
            let k = ((y - r.top - 4) / ROW_H).clamp(0, list_rows() as i32 - 1) as usize;
            CUR = (LIST_TOP + k).min(NFILTERS - 1);
            TYPED.len = 0;
            let p = pane();
            rebuild_view(p);
            p.scroll = 0;
            return true;
        }
        if inside(&l.kind, x, y) {
            return true;
        }
    }
    if !inside(status, x, y) {
        return false;
    }
    if down {
        if inside(&l.name, x, y) {
            FOCUS = Focus::FileName;
        } else if inside(&l.kind, x, y) {
            LIST_OPEN = true;
        } else if inside(&l.ok, x, y) {
            accept();
        } else if inside(&l.cancel, x, y) {
            cancel();
        }
    }
    true
}

/// Clavier : Entrée valide, Échap annule (ou ferme la liste / le message).
pub(super) unsafe fn key(vk: u32, ch: u16) -> bool {
    if !ACTIVE {
        return false;
    }
    if ASK != Ask::None {
        match ch {
            13 => answer(true),
            27 => answer(false),
            _ => {}
        }
        return true;
    }
    if LIST_OPEN {
        match vk {
            0x26 => CUR = CUR.saturating_sub(1),
            0x28 => CUR = (CUR + 1).min(NFILTERS - 1),
            _ => {}
        }
        keep_visible();
        if matches!(vk, 0x26 | 0x28) {
            TYPED.len = 0;
            rebuild_view(pane());
            return true;
        }
        if matches!(ch, 13 | 27) {
            LIST_OPEN = false;
            return true;
        }
    }
    if FOCUS == Focus::FileName {
        match ch {
            13 => {
                accept();
                return true;
            }
            27 => cancel(),
            _ => {}
        }
        // Flèches : retour à la liste, comme la touche Tab de Windows.
        if ch == 0 && matches!(vk, 0x26 | 0x28) {
            FOCUS = Focus::List;
            return false;
        }
    }
    if FOCUS == Focus::List && ch == 27 && MENU == Menu::None {
        cancel();
    }
    // Tab : du nom à la liste et retour.
    if ch == 9 {
        FOCUS = if FOCUS == Focus::FileName { Focus::List } else { Focus::FileName };
        return true;
    }
    false
}

pub(super) unsafe fn regression() {
    check!(
        wildcard(b"*.txt", b"NOTE.TXT") && !wildcard(b"*.txt", b"note.txt.bak"),
        "Ouvrir : joker *.txt sans casse"
    );
    check!(wildcard(b"*.*", b"LISEZMOI"), "Ouvrir : *.* prend aussi un nom sans point");
    check!(
        matches_any(b"*.png; *.jpg", b"photo.JPG") && !matches_any(b"*.png;*.jpg", b"a.gif"),
        "Ouvrir : plusieurs motifs separes par ;"
    );
    check!(
        wildcard(b"a?c.*", b"abc.d") && !wildcard(b"a?c.*", b"ac.d"),
        "Ouvrir : ? remplace un caractere"
    );
    let mut out = EMPTY_PATH;
    check!(
        resolve(b"C:\\Users\\d\\Documents", b"a.txt", &mut out)
            && out.is(b"C:\\Users\\d\\Documents\\a.txt"),
        "Ouvrir : nom relatif au dossier affiche"
    );
    check!(
        resolve(b"C:\\Users\\d\\Documents", b"..\\Bureau\\b.txt", &mut out)
            && out.is(b"C:\\Users\\d\\Bureau\\b.txt"),
        "Ouvrir : .. remonte d'un dossier"
    );
    check!(
        resolve(b"C:\\a", b"D:/x/y.log", &mut out) && out.is(b"D:\\x\\y.log"),
        "Ouvrir : chemin absolu garde"
    );
    check!(
        resolve(b"C:\\a\\b", b"\\Temp", &mut out) && out.is(b"C:\\Temp"),
        "Ouvrir : \\ part de la racine du lecteur"
    );
    check!(
        resolve(b"C:\\", b"..", &mut out) && out.is(b"C:\\"),
        "Ouvrir : .. s'arrete a la racine"
    );
    check!(
        !resolve(HOME, b"a.txt", &mut out),
        "Ouvrir : un nom seul dans Accueil ne designe rien"
    );
    check!(!active(), "Explorateur normal : pas de boite Ouvrir sans WINLUX_PICK");
    FOLDER = true;
    ACTIVE = true;
    check!(
        !shows(b"a.txt", false) && shows(b"Documents", true),
        "Choisir un dossier : fichiers masques, dossiers visibles"
    );
    check!(
        resolve(b"C:\\Users\\d\\Documents", b".", &mut out) && out.is(b"C:\\Users\\d\\Documents"),
        "Choisir un dossier : champ vide = dossier affiche"
    );
    FOLDER = false;
    ACTIVE = false;
}
