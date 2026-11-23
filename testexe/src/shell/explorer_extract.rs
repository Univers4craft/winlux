//! Boîte « Extraire tout… » (#3246) : les points de suspension promettent un
//! choix avant l'action et la possibilité d'annuler (guide public des menus,
//! « Using ellipses »). Destination modifiable, préremplie avec le dossier au
//! nom de l'archive, « Parcourir… », case « Afficher les fichiers extraits
//! une fois l'opération terminée » cochée, Extraire / Annuler.
//! Le contenu exact de la boîte est un choix de produit (aucune source
//! publique ne le décrit), soumis à damien.

use super::*;

static mut OPEN: bool = false;
static mut ARCHIVE: Field = EMPTY_FIELD;
static mut DEST: Field = EMPTY_FIELD;
/// Case « Afficher les fichiers extraits… » (cochée par défaut).
static mut SHOW: bool = true;
/// Destination choisie pour l'extraction en cours (relue si l'archive
/// demande un mot de passe), vide hors d'une extraction lancée par la boîte.
pub(super) static mut CHOSEN: Field = EMPTY_FIELD;
/// Faut-il entrer dans le dossier une fois l'extraction finie ?
pub(super) static mut SHOW_AFTER: bool = true;

pub(super) fn dialog_open() -> bool {
    unsafe { OPEN }
}

/// Nom de dossier proposé : `photos.tar.gz` → `photos`, `a.zip` → `a`.
pub(super) fn folder_name(name: &[u8]) -> &[u8] {
    for ext in [b".tar.gz" as &[u8], b".tar.xz", b".tar.bz2", b".tar.zst"] {
        if name.len() > ext.len() && name[name.len() - ext.len()..].eq_ignore_ascii_case(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    match name.iter().rposition(|&c| c == b'.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// Ouvre la boîte pour `path` (archive, chemin Windows).
pub(super) unsafe fn open(path: &[u8]) {
    if JOB != 0 {
        notify(busy_message());
        return;
    }
    let Some(dir) = parent(path) else {
        return;
    };
    ARCHIVE.set(path);
    let mut d = [0u8; PATH_LEN];
    let mut n = 0;
    for part in [dir, b"\\" as &[u8], folder_name(last_component(path))] {
        if dir.ends_with(b"\\") && part == b"\\" {
            continue;
        }
        let k = part.len().min(d.len() - n);
        d[n..n + k].copy_from_slice(&part[..k]);
        n += k;
    }
    DEST.set(&d[..n]);
    SHOW = true;
    OPEN = true;
    MENU = Menu::None;
}

unsafe fn submit() {
    let dest = DEST;
    let d = trim(dest.get());
    if d.len() < 3 || d[1] != b':' {
        notify(t4([
            b"Indiquez un dossier de destination complet, par exemple C:\\Dossier.",
            b"Enter a full destination folder, for example C:\\Folder.",
            b"Indique una carpeta de destino completa, por ejemplo C:\\Carpeta.",
            b"Geben Sie einen vollst\xE4ndigen Zielordner an, zum Beispiel C:\\Ordner.",
        ]));
        return;
    }
    OPEN = false;
    CHOSEN.set(d);
    SHOW_AFTER = SHOW;
    let a = ARCHIVE;
    extract_archive(a.get(), false, None);
}

unsafe fn cancel() {
    OPEN = false;
}

fn trim(s: &[u8]) -> &[u8] {
    let mut s = s;
    while let [b' ' | b'"', rest @ ..] = s {
        s = rest;
    }
    while let [rest @ .., b' ' | b'"' | b'\\'] = s {
        if rest.len() <= 2 {
            break;
        }
        s = rest;
    }
    s
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    fn SHBrowseForFolderW(bi: *mut BrowseInfoW) -> usize;
    fn SHGetPathFromIDListW(pidl: usize, path: *mut u16) -> i32;
}
#[link(name = "ole32", kind = "raw-dylib")]
extern "system" {
    fn CoTaskMemFree(p: usize);
}

/// BROWSEINFOW (shlobj_core.h, x64).
#[repr(C)]
struct BrowseInfoW {
    owner: usize,
    root: usize,
    display: *mut u16,
    title: *const u16,
    flags: u32,
    callback: usize,
    lparam: isize,
    image: i32,
}

/// « Parcourir… » : boîte de choix de dossier de Winlux (SHBrowseForFolderW).
unsafe fn browse() {
    let mut display = [0u16; 260];
    let mut title = [0u16; 96];
    let t = t4([
        b"S\xE9lectionner une destination",
        b"Select a destination",
        b"Seleccionar un destino",
        b"Ziel ausw\xE4hlen",
    ]);
    for (i, &c) in t.iter().take(title.len() - 1).enumerate() {
        title[i] = c as u16;
    }
    let mut bi = BrowseInfoW {
        owner: 0,
        root: 0,
        display: display.as_mut_ptr(),
        title: title.as_ptr(),
        flags: 0x41, // BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE
        callback: 0,
        lparam: 0,
        image: 0,
    };
    let pidl = SHBrowseForFolderW(&mut bi);
    if pidl == 0 {
        return;
    }
    let mut out = [0u16; 260];
    if SHGetPathFromIDListW(pidl, out.as_mut_ptr()) != 0 {
        let n = out.iter().position(|&c| c == 0).unwrap_or(0);
        let mut b = [0u8; PATH_LEN];
        let n = n.min(b.len());
        for i in 0..n {
            b[i] = ansi_char(out[i]).unwrap_or(b'?');
        }
        DEST.set(&b[..n]);
    }
    CoTaskMemFree(pidl);
}

fn dialog_box(f: &Frame) -> Rect {
    let (w, h) = (520, 236);
    let cx = (f.client.left + f.client.right) / 2;
    let cy = (f.client.top + f.client.bottom) / 2;
    rect(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
}

struct Parts {
    field: Rect,
    browse: Rect,
    check: Rect,
    extract: Rect,
    cancel: Rect,
}

fn parts(r: &Rect) -> Parts {
    Parts {
        field: rect(r.left + 20, r.top + 78, r.right - 132, r.top + 110),
        browse: rect(r.right - 122, r.top + 78, r.right - 20, r.top + 110),
        check: rect(r.left + 20, r.top + 128, r.right - 20, r.top + 152),
        extract: rect(r.right - 216, r.bottom - 48, r.right - 116, r.bottom - 16),
        cancel: rect(r.right - 108, r.bottom - 48, r.right - 16, r.bottom - 16),
    }
}

unsafe fn button(g: &mut Gfx, r: &Rect, label: &[u8], primary: bool) {
    let p = pal();
    if primary {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.accent);
    } else {
        let bg = if hovered(r) { p.hover } else { p.window };
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.border);
        round_rect(g.hdc, g.b, r.left + 1, r.top + 1, r.right - 1, r.bottom - 1, 4, bg);
    }
    text(
        g.hdc,
        g.f.caption,
        if primary { p.on_accent } else { p.text },
        (r.left + r.right - text_width(label, 12)) / 2,
        r.top + 8,
        label,
    );
}

pub(super) unsafe fn draw(g: &mut Gfx, f: &Frame) {
    if !OPEN {
        return;
    }
    let p = pal();
    let r = dialog_box(f);
    round_rect(g.hdc, g.b, r.left - 1, r.top - 1, r.right + 1, r.bottom + 1, 9, p.border);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        r.left + 20,
        r.top + 18,
        t4([b"Extraire tout", b"Extract all", b"Extraer todo", b"Alle extrahieren"]),
    );
    let a = ARCHIVE;
    text_fit(g.hdc, g.f.small, 11, p.text2, r.left + 20, r.top + 42, last_component(a.get()), r.right - r.left - 40);
    text(
        g.hdc,
        g.f.small,
        p.text,
        r.left + 20,
        r.top + 60,
        t4([
            b"Les fichiers seront extraits dans ce dossier :",
            b"Files will be extracted to this folder:",
            b"Los archivos se extraer\xE1n en esta carpeta:",
            b"Dateien werden in diesen Ordner extrahiert:",
        ]),
    );
    let pp = parts(&r);
    let fl = &pp.field;
    round_rect(g.hdc, g.b, fl.left, fl.top, fl.right, fl.bottom, 4, p.accent);
    round_rect(g.hdc, g.b, fl.left + 1, fl.top + 1, fl.right - 1, fl.bottom - 2, 4, p.window);
    let d = DEST;
    // Fin du chemin visible quand il dépasse : c'est là qu'on tape.
    let room = fl.right - fl.left - 20;
    let mut s = d.get();
    while s.len() > 1 && text_width(s, 12) > room {
        s = &s[1..];
    }
    text(g.hdc, g.f.caption, p.text, fl.left + 10, fl.top + 8, s);
    if GetTickCount() / 500 % 2 == 0 {
        let x = fl.left + 10 + text_width(s, 12);
        fill(g.hdc, g.b, x, fl.top + 7, x + 1, fl.bottom - 8, p.text);
    }
    button(
        g,
        &pp.browse,
        t4([b"Parcourir...", b"Browse...", b"Examinar...", b"Durchsuchen..."]),
        false,
    );
    let c = &pp.check;
    panel(
        g.hdc,
        g.b,
        c.left,
        c.top + 2,
        c.left + 20,
        c.top + 22,
        4,
        if SHOW { p.accent } else { p.window },
        if SHOW || hovered(c) { p.accent } else { p.text3 },
    );
    if SHOW {
        text(g.hdc, g.f.small, p.on_accent, c.left + 5, c.top + 4, b"\xD7");
    }
    text(
        g.hdc,
        g.f.body,
        p.text,
        c.left + 28,
        c.top + 3,
        t4([
            b"Afficher les fichiers extraits une fois l'op\xE9ration termin\xE9e",
            b"Show extracted files when complete",
            b"Mostrar los archivos extra\xEDdos al terminar",
            b"Dateien nach Extrahierung anzeigen",
        ]),
    );
    button(
        g,
        &pp.extract,
        t4([b"Extraire", b"Extract", b"Extraer", b"Extrahieren"]),
        true,
    );
    button(
        g,
        &pp.cancel,
        t4([b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]),
        false,
    );
}

/// Clavier : Entrée extrait, Échap annule, Espace sans texte n'est qu'un
/// caractère du chemin, Retour arrière efface.
pub(super) unsafe fn key(_vk: u32, ch: u16) -> bool {
    match ch {
        13 => submit(),
        27 => cancel(),
        8 => DEST.pop(),
        c if c >= 32 => match ansi_char(c) {
            Some(a) => DEST.push(a),
            None => notify(tr4(&[
                b"Caract\xE8re non pris en charge.",
                b"Unsupported character.",
                b"Car\xE1cter no admitido.",
                b"Nicht unterst\xFCtztes Zeichen.",
            ])),
        },
        _ => {}
    }
    true
}

pub(super) unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if ev != Mouse::Down {
        return true;
    }
    let pp = parts(&dialog_box(f));
    if inside(&pp.extract, x, y) {
        submit();
    } else if inside(&pp.cancel, x, y) {
        cancel();
    } else if inside(&pp.browse, x, y) {
        browse();
    } else if inside(&pp.check, x, y) {
        SHOW = !SHOW;
    }
    true
}

/// Contrôles sans fenêtre (#3246) : nom de dossier proposé, boîte ouverte
/// avant toute extraction avec destination préremplie et case cochée,
/// saisie, Échap ferme sans rien lancer.
pub(super) unsafe fn regression() {
    check!(
        folder_name(b"photos.zip") == b"photos"
            && folder_name(b"Sauvegarde.TAR.GZ") == b"Sauvegarde"
            && folder_name(b"src.tar.zst") == b"src"
            && folder_name(b"v1.2.7z") == b"v1.2"
            && folder_name(b".zip") == b".zip",
        "Extraire tout : dossier propose au nom de l'archive (#3246)"
    );
    if JOB != 0 {
        return;
    }
    open(b"C:\\Users\\d\\Downloads\\photos.tar.gz");
    let d = DEST;
    check!(
        OPEN && SHOW && JOB == 0 && d.get() == b"C:\\Users\\d\\Downloads\\photos",
        "Extraire tout... : boite ouverte, rien d'extrait, destination preremplie, case cochee (#3246)"
    );
    key(0, 8);
    key(0, b'X' as u16);
    let d = DEST;
    check!(d.get() == b"C:\\Users\\d\\Downloads\\photoX", "Extraire tout... : destination modifiable (#3246)");
    key(0, 27);
    check!(!OPEN && JOB == 0 && CHOSEN.len == 0, "Extraire tout... : Echap annule sans rien lancer (#3246)");
    open(b"C:\\photos.zip");
    let d = DEST;
    check!(d.get() == b"C:\\photos", "Extraire tout... : archive a la racine d'un lecteur (#3246)");
    cancel();
}
