//! Boîte « Propriétés » de l'Explorateur (Alt+Entrée ou clic droit >
//! Propriétés), onglet Général de Windows 11 : nom, type, emplacement,
//! taille (et taille exacte en octets), contenu d'un dossier, dates de
//! création, de modification et d'accès, attribut Lecture seule.

use super::*;

const MAX_WALK: u32 = 50_000;
const MAX_DEPTH: u32 = 32;

#[derive(Clone, Copy)]
struct Props {
    open: bool,
    name: [u8; NAME_LEN],
    name_len: usize,
    path: [u8; PATH_LEN + NAME_LEN + 2],
    path_len: usize,
    kind: Kind,
    dir: bool,
    size: u64,
    files: u32,
    dirs: u32,
    partial: bool,
    created: u64,
    modified: u64,
    accessed: u64,
    readonly: bool,
    hidden: bool,
    readonly_new: bool,
}

static mut P: Props = Props {
    open: false,
    name: [0; NAME_LEN],
    name_len: 0,
    path: [0; PATH_LEN + NAME_LEN + 2],
    path_len: 0,
    kind: Kind::File,
    dir: false,
    size: 0,
    files: 0,
    dirs: 0,
    partial: false,
    created: 0,
    modified: 0,
    accessed: 0,
    readonly: false,
    hidden: false,
    readonly_new: false,
};

pub(super) unsafe fn dialog_open() -> bool {
    P.open
}

fn ft(t: FileTime) -> u64 {
    (t.high as u64) << 32 | t.low as u64
}

/// Taille et nombre d'éléments d'un dossier, borné (50 000 éléments).
pub(super) unsafe fn walk(path: &[u8], depth: u32, size: &mut u64, files: &mut u32, dirs: &mut u32, partial: &mut bool) {
    if depth > MAX_DEPTH || *files + *dirs >= MAX_WALK {
        *partial = true;
        return;
    }
    let mut pat = [0u8; PATH_LEN + NAME_LEN + 2];
    let n = join(&mut pat, path, b"*");
    if n == 0 {
        *partial = true;
        return;
    }
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(wstr(&mut w, &pat[..n]), &mut data);
    if h as isize == -1 {
        return;
    }
    loop {
        let name = find_name_w(&data);
        if !name.is_empty() && name != b"." && name != b".." {
            if data.attributes & 0x10 != 0 {
                *dirs += 1;
                // Pas de lien symbolique suivi (boucles, autre disque).
                if data.attributes & 0x400 == 0 {
                    let mut sub = [0u8; PATH_LEN + NAME_LEN + 2];
                    let m = join(&mut sub, path, name);
                    if m > 0 {
                        walk(&sub[..m], depth + 1, size, files, dirs, partial);
                    } else {
                        *partial = true;
                    }
                }
            } else {
                *files += 1;
                *size = size.saturating_add((data.size_high as u64) << 32 | data.size_low as u64);
            }
            if *files + *dirs >= MAX_WALK {
                *partial = true;
                break;
            }
        }
        if FindNextFileW(h, &mut data) == 0 {
            break;
        }
    }
    FindClose(h);
}

/// Ouvre la boîte pour l'élément `index` du volet.
pub(super) unsafe fn open(p: &Pane, index: usize) {
    let e = p.entries[index];
    let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
    let n = item_path(p, index, &mut path);
    if n == 0 {
        return;
    }
    let mut props = Props {
        open: true,
        name: [0; NAME_LEN],
        name_len: e.name().len().min(NAME_LEN),
        path,
        path_len: n,
        kind: e.kind,
        dir: e.dir,
        size: e.size,
        files: 0,
        dirs: 0,
        partial: false,
        created: 0,
        modified: e.mtime,
        accessed: 0,
        readonly: false,
        hidden: e.hidden,
        readonly_new: false,
    };
    props.name[..props.name_len].copy_from_slice(&e.name()[..props.name_len]);
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut data: Win32FileAttributeData = core::mem::zeroed();
    if GetFileAttributesExW(wstr(&mut w, &path[..n]), 0, &mut data) != 0 {
        props.created = ft(data.creation_time);
        props.modified = ft(data.write_time);
        props.accessed = ft(data.access_time);
        props.readonly = data.attributes & 1 != 0;
        props.hidden |= data.attributes & 2 != 0;
        if !props.dir {
            props.size = (data.size_high as u64) << 32 | data.size_low as u64;
        }
    }
    if props.dir {
        let mut base = [0u8; PATH_LEN + NAME_LEN + 2];
        let mut m = n;
        base[..n].copy_from_slice(&path[..n]);
        if base[m - 1] != b'\\' {
            base[m] = b'\\';
            m += 1;
        }
        let (mut size, mut files, mut dirs, mut partial) = (0, 0, 0, false);
        walk(&base[..m], 0, &mut size, &mut files, &mut dirs, &mut partial);
        props.size = size;
        props.files = files;
        props.dirs = dirs;
        props.partial = partial;
    }
    props.readonly_new = props.readonly;
    P = props;
}

unsafe fn close(apply: bool) {
    if apply && !P.dir && P.readonly_new != P.readonly {
        let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
        let path = wstr(&mut w, &P.path[..P.path_len]);
        let attrs = GetFileAttributesW(path);
        if attrs != u32::MAX {
            let new = if P.readonly_new { attrs | 1 } else { attrs & !1 };
            if SetFileAttributesW(path, new & !0x80) == 0 {
                notify(t4([
                    b"Impossible de modifier l'attribut Lecture seule.",
                    b"Couldn't change the Read-only attribute.",
                    b"No se pudo cambiar el atributo de solo lectura.",
                    b"Das Attribut \x84Schreibgesch\xFCtzt\x93 konnte nicht ge\xE4ndert werden.",
                ]));
            }
        }
        reload(pane());
    }
    P.open = false;
}

fn t4(s: [&'static [u8]; 4]) -> &'static [u8] {
    tr4(&s)
}

// ---------- Géométrie ----------

fn box_rect(f: &Frame) -> Rect {
    let w = 420.min(f.client.right - f.client.left - 24);
    let h = 420.min(f.client.bottom - f.client.top - 24);
    let cx = (f.client.left + f.client.right) / 2;
    let cy = (f.client.top + f.client.bottom) / 2;
    rect(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
}

fn buttons(r: &Rect) -> (Rect, Rect) {
    let cancel = rect(r.right - 108, r.bottom - 44, r.right - 16, r.bottom - 14);
    let ok = rect(cancel.left - 100, cancel.top, cancel.left - 8, cancel.bottom);
    (ok, cancel)
}

fn readonly_box(r: &Rect) -> Rect {
    rect(r.left + 130, r.bottom - 92, r.left + 146, r.bottom - 76)
}

// ---------- Dessin ----------

/// « 3,0 Mo (3 145 728 octets) »
fn size_line(size: u64, out: &mut [u8; 96]) -> usize {
    let mut s = [0u8; 24];
    let m = size_text(size, &mut s);
    let mut n = 0;
    for &c in &s[..m] {
        out[n] = c;
        n += 1;
    }
    if size >= 1024 {
        out[n] = b' ';
        out[n + 1] = b'(';
        n += 2;
        // Milliers séparés par une espace, comme Windows en français.
        let mut digits = [0u8; 20];
        let mut k = 0;
        let mut v = size;
        loop {
            digits[k] = b'0' + (v % 10) as u8;
            k += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        for i in (0..k).rev() {
            out[n] = digits[i];
            n += 1;
            if i > 0 && i % 3 == 0 {
                out[n] = b' ';
                n += 1;
            }
        }
        for &c in tr4(&[b" octets)", b" bytes)", b" bytes)", b" Byte)"]) {
            out[n] = c;
            n += 1;
        }
    }
    n
}

pub(super) unsafe fn draw(g: &mut Gfx, f: &Frame) {
    if !P.open {
        return;
    }
    let p = pal();
    let r = box_rect(f);
    round_rect(g.hdc, g.b, r.left - 1, r.top - 1, r.right + 1, r.bottom + 1, 9, p.border);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    // Titre : « Propriétés de note.txt »
    let mut title = [0u8; NAME_LEN + 32];
    let mut n = 0;
    for &c in tr4(&[b"Propri\xE9t\xE9s de ", b"Properties of ", b"Propiedades de ", b"Eigenschaften von "]).iter().chain(&P.name[..P.name_len]) {
        if n < title.len() {
            title[n] = c;
            n += 1;
        }
    }
    text_fit(g.hdc, g.f.semibold, 14, p.text, r.left + 20, r.top + 16, &title[..n], r.right - r.left - 40);
    // Icône et nom.
    glyph(g.hdc, g.b, g.f, glyph_of(P.kind), r.left + 20, r.top + 50, 32);
    text_fit(g.hdc, g.f.caption, 12, p.text, r.left + 70, r.top + 59, &P.name[..P.name_len], r.right - r.left - 90);
    fill(g.hdc, g.b, r.left + 20, r.top + 94, r.right - 20, r.top + 95, p.border);
    let label_x = r.left + 20;
    let value_x = r.left + 130;
    let value_w = r.right - value_x - 20;
    let mut y = r.top + 106;
    let row = |g: &mut Gfx, y: &mut i32, label: &[u8], value: &[u8]| {
        text(g.hdc, g.f.caption, p.text2, label_x, *y, label);
        text_fit(g.hdc, g.f.caption, 12, p.text, value_x, *y, value, value_w);
        *y += 24;
    };
    // Type
    let e = Entry { kind: P.kind, dir: P.dir, ..EMPTY_ENTRY };
    let mut probe = e;
    probe.name[..P.name_len].copy_from_slice(&P.name[..P.name_len]);
    probe.len = P.name_len as u16;
    let mut tbuf = [0u8; 48];
    row(g, &mut y, tr4(&[b"Type :", b"Type:", b"Tipo:", b"Typ:"]), type_name(&probe, &mut tbuf));
    // Emplacement : dossier parent.
    let loc = parent(&P.path[..P.path_len]).unwrap_or(&P.path[..P.path_len]);
    row(g, &mut y, tr4(&[b"Emplacement :", b"Location:", b"Ubicaci\xF3n:", b"Ort:"]), loc);
    // Taille
    let mut sbuf = [0u8; 96];
    let sn = size_line(P.size, &mut sbuf);
    row(g, &mut y, tr4(&[b"Taille :", b"Size:", b"Tama\xF1o:", b"Gr\xF6\xDFe:"]), &sbuf[..sn]);
    if P.dir {
        // « 12 fichiers, 3 dossiers », au singulier quand il le faut.
        let mut c = [0u8; 96];
        let mut m = content_text(P.files, P.dirs, &mut c);
        if P.partial {
            for &ch in tr4(&[b" (au moins)", b" (at least)", b" (al menos)", b" (mindestens)"]) {
                c[m] = ch;
                m += 1;
            }
        }
        row(g, &mut y, tr4(&[b"Contenu :", b"Contains:", b"Contiene:", b"Inhalt:"]), &c[..m]);
    }
    y += 6;
    fill(g.hdc, g.b, r.left + 20, y - 6, r.right - 20, y - 5, p.border);
    let mut d = [0u8; 24];
    for (label, t) in [
        (tr4(&[b"Cr\xE9\xE9 le :", b"Created:", b"Creado:", b"Erstellt:"]), P.created),
        (tr4(&[b"Modifi\xE9 le :", b"Modified:", b"Modificado:", b"Ge\xE4ndert:"]), P.modified),
        (tr4(&[b"Dernier acc\xE8s :", b"Accessed:", b"\xDAltimo acceso:", b"Letzter Zugriff:"]), P.accessed),
    ] {
        let dn = date_text(t, &mut d);
        row(g, &mut y, label, &d[..dn]);
    }
    // Attributs
    let rb = readonly_box(&r);
    text(g.hdc, g.f.caption, p.text2, label_x, rb.top, tr4(&[b"Attributs :", b"Attributes:", b"Atributos:", b"Attribute:"]));
    let check = |g: &mut Gfx, b: Rect, on: bool, enabled: bool, label: &[u8]| {
        round_rect(g.hdc, g.b, b.left, b.top, b.right, b.bottom, 3, if on { p.accent } else { p.text3 });
        if !on {
            round_rect(g.hdc, g.b, b.left + 1, b.top + 1, b.right - 1, b.bottom - 1, 3, p.surface);
        } else {
            line(g.hdc, g.b, b.left + 3, b.top + 8, b.left + 6, b.top + 11, 2, p.on_accent);
            line(g.hdc, g.b, b.left + 6, b.top + 11, b.left + 12, b.top + 4, 2, p.on_accent);
        }
        text(g.hdc, g.f.caption, if enabled { p.text } else { p.text3 }, b.right + 8, b.top, label);
    };
    check(
        g,
        rb,
        P.readonly_new,
        !P.dir,
        tr4(&[b"Lecture seule", b"Read-only", b"Solo lectura", b"Schreibgesch\xFCtzt"]),
    );
    let hb = rect(rb.left + 150, rb.top, rb.left + 166, rb.bottom);
    check(g, hb, P.hidden, false, tr4(&[b"Cach\xE9", b"Hidden", b"Oculto", b"Versteckt"]));
    let (ok, cancel) = buttons(&r);
    super::copy::button(g, &ok, b"OK", true);
    super::copy::button(g, &cancel, tr4(&[b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]), false);
}

// ---------- Entrées ----------

pub(super) unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if !P.open {
        return false;
    }
    if ev == Mouse::Down {
        let r = box_rect(f);
        let (ok, cancel) = buttons(&r);
        let rb = readonly_box(&r);
        let label = rect(rb.left, rb.top - 2, rb.right + 120, rb.bottom + 2);
        if inside(&ok, x, y) {
            close(true);
        } else if inside(&cancel, x, y) {
            close(false);
        } else if !P.dir && inside(&label, x, y) {
            P.readonly_new = !P.readonly_new;
        }
    }
    true
}

pub(super) unsafe fn key(vk: u32, _ch: u16) -> bool {
    // Touches virtuelles seulement : Alt+Entrée ouvre la boîte sur l'appui,
    // le caractère 13 qui suit ne doit pas la refermer aussitôt.
    match vk {
        0x0D => close(true),
        0x1B => close(false),
        0x20 if !P.dir => P.readonly_new = !P.readonly_new,
        _ => {}
    }
    true
}

/// « 1 fichier, 0 dossier », « 12 files, 1 folder »… Le français met le
/// singulier à 0 et 1, les autres langues seulement à 1.
fn content_text(files: u32, dirs: u32, c: &mut [u8; 96]) -> usize {
    let one = |n: u32| if ui_lang() == 0 { n <= 1 } else { n == 1 };
    let mut m = write_num(c, 0, files as u64);
    let f = if one(files) {
        t4([b" fichier, ", b" file, ", b" archivo, ", b" Datei, "])
    } else {
        t4([b" fichiers, ", b" files, ", b" archivos, ", b" Dateien, "])
    };
    c[m..m + f.len()].copy_from_slice(f);
    m += f.len();
    m = write_num(c, m, dirs as u64);
    let d = if one(dirs) {
        t4([b" dossier", b" folder", b" carpeta", b" Ordner"])
    } else {
        t4([b" dossiers", b" folders", b" carpetas", b" Ordner"])
    };
    c[m..m + d.len()].copy_from_slice(d);
    m + d.len()
}

// ---------- Régression ----------

#[doc(hidden)]
pub(super) unsafe fn regression(root: &[u8]) {
    // Fichier de 3 000 octets puis dossier qui le contient.
    let mut dir = [0u8; PATH_LEN + NAME_LEN + 2];
    let dn = join(&mut dir, root, b"proprietes");
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    CreateDirectoryW(wstr(&mut w, &dir[..dn]), core::ptr::null_mut());
    let mut file = [0u8; PATH_LEN + NAME_LEN + 2];
    let fnl = join(&mut file, &dir[..dn], b"note.txt");
    let h = CreateFileW(wstr(&mut w, &file[..fnl]), 0x4000_0000, 0, core::ptr::null_mut(), 2, 0x80, core::ptr::null_mut());
    let data = [b'a'; 3000];
    let mut wr = 0u32;
    WriteFile(h, data.as_ptr(), data.len() as u32, &mut wr, core::ptr::null_mut());
    CloseHandle(h);
    let mut sub = [0u8; PATH_LEN + NAME_LEN + 2];
    let sn = join(&mut sub, &dir[..dn], b"sous-dossier");
    CreateDirectoryW(wstr(&mut w, &sub[..sn]), core::ptr::null_mut());
    navigate(&dir[..dn]);
    let p = pane();
    let fi = (0..p.count).find(|&i| p.entries[i].name() == b"note.txt");
    check!(fi.is_some(), "Proprietes : le fichier de test est liste");
    let Some(fi) = fi else { return };
    open(p, fi);
    check!(
        P.open && P.size == 3000 && !P.dir && P.modified != 0 && !P.readonly,
        "Proprietes d'un fichier : taille exacte, date de modification, pas en lecture seule"
    );
    let mut sb = [0u8; 96];
    let n = size_line(3000, &mut sb);
    check!(&sb[..n] == b"2,9 Ko (3 000 octets)" || sb[..n].ends_with(b"(3 000 octets)") || sb[..n].ends_with(b"(3 000 bytes)"),
        "Proprietes : taille affichee avec les octets exacts (3 000)");
    // Cocher Lecture seule puis OK : l'attribut est posé sur le fichier.
    key(0x20, 0);
    key(0x0D, 0);
    let attrs = GetFileAttributesW(wstr(&mut w, &file[..fnl]));
    check!(!P.open && attrs != u32::MAX && attrs & 1 != 0, "Proprietes : Lecture seule + OK protege le fichier");
    // Puis la décocher : le fichier redevient modifiable.
    let p = pane();
    if let Some(fi) = (0..p.count).find(|&i| p.entries[i].name() == b"note.txt") {
        open(p, fi);
        check!(P.readonly, "Proprietes : la case Lecture seule reflete l'etat du fichier");
        key(0x20, 0);
        key(0x0D, 0);
    }
    let attrs = GetFileAttributesW(wstr(&mut w, &file[..fnl]));
    check!(attrs != u32::MAX && attrs & 1 == 0, "Proprietes : decocher Lecture seule rend le fichier modifiable");
    // Annuler ne change rien.
    let p = pane();
    if let Some(fi) = (0..p.count).find(|&i| p.entries[i].name() == b"note.txt") {
        open(p, fi);
        key(0x20, 0);
        key(0x1B, 0);
    }
    let attrs = GetFileAttributesW(wstr(&mut w, &file[..fnl]));
    check!(!P.open && attrs & 1 == 0, "Proprietes : Annuler n'applique rien");
    // Dossier : contenu compté.
    navigate(root);
    let p = pane();
    if let Some(di) = (0..p.count).find(|&i| p.entries[i].name() == b"proprietes") {
        open(p, di);
        check!(
            P.dir && P.files == 1 && P.dirs == 1 && P.size == 3000 && !P.partial,
            "Proprietes d'un dossier : 1 fichier, 1 dossier, 3 000 octets"
        );
        close(false);
    } else {
        check!(false, "Proprietes d'un dossier : 1 fichier, 1 dossier, 3 000 octets");
    }
    DeleteFileW(wstr(&mut w, &file[..fnl]));
    RemoveDirectoryW(wstr(&mut w, &sub[..sn]));
    RemoveDirectoryW(wstr(&mut w, &dir[..dn]));
    // Pluriels du Contenu dans les 4 langues.
    let saved = ui_lang();
    let mut c = [0u8; 96];
    let mut all = true;
    for (l, a, b) in [
        (crate::locale::Lang::Fr, &b"1 fichier, 1 dossier"[..], &b"2 fichiers, 0 dossier"[..]),
        (crate::locale::Lang::En, b"1 file, 1 folder", b"2 files, 0 folders"),
        (crate::locale::Lang::Es, b"1 archivo, 1 carpeta", b"2 archivos, 0 carpetas"),
        (crate::locale::Lang::De, b"1 Datei, 1 Ordner", b"2 Dateien, 0 Ordner"),
    ] {
        set_ui_lang(l as usize);
        let n = content_text(1, 1, &mut c);
        all &= &c[..n] == a;
        let n = content_text(2, 0, &mut c);
        all &= &c[..n] == b;
    }
    set_ui_lang(saved);
    check!(all, "Proprietes : singulier et pluriel du Contenu en fr, en, es et de");
}
