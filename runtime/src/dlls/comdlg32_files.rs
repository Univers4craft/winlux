//! `comdlg32` : boîtes « Ouvrir » et « Enregistrer sous »
//! (`GetOpenFileName`, `GetSaveFileName`), construites avec les contrôles de
//! Winlux : emplacements à gauche, liste à colonnes du dossier, nom du
//! fichier et type en bas. Les chemins rendus au programme sont des chemins
//! Windows (`C:\Users\…`) ; la boîte parcourt les dossiers Linux qu'ils
//! désignent.
//!
//! Sans affichage, `WINLUX_FILEDLG=<nom ou chemin>` remplit le champ « Nom
//! du fichier » et valide la boîte, avec les mêmes vérifications qu'un clic
//! sur « Ouvrir » ; sans cette variable, la boîte est annulée.

use super::comdlg32_ui::*;
use super::{dialogs, gui, user32};
use crate::paths;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

// Identifiants (ceux de Windows quand ils existent : edt1, cmb1, lst1).
const ID_PATH: usize = 0x47C;
const ID_UP: usize = 0x47D;
const ID_PLACES: usize = 0x47E;
const ID_NEW_FOLDER: usize = 0x47F;
const ID_LIST: usize = 0x460;
const ID_NAME: usize = 0x480;
const ID_TYPE: usize = 0x470;

const OFN_OVERWRITEPROMPT: u32 = 0x2;
const OFN_ALLOWMULTISELECT: u32 = 0x200;
const OFN_PATHMUSTEXIST: u32 = 0x800;
const OFN_FILEMUSTEXIST: u32 = 0x1000;
const OFN_CREATEPROMPT: u32 = 0x2000;

const FNERR_BUFFERTOOSMALL: u32 = 0x3003;

const W: i32 = 760;
const H: i32 = 480;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Clone, Default)]
struct FileState {
    save: bool,
    flags: u32,
    filters: Vec<(String, String)>,
    filter: usize,
    /// Motif tapé dans « Nom du fichier » (`*.log`), prioritaire sur le type.
    typed_filter: Option<String>,
    def_ext: Option<String>,
    dir: PathBuf,
    entries: Vec<Entry>,
    places: Vec<PathBuf>,
    result: Vec<String>,
    initial_name: String,
}

thread_local! {
    static STATE: RefCell<Option<FileState>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut FileState) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

// ---------- Logique pure (testée sans affichage) ----------

/// Filtre `lpstrFilter` : paires « libellé\0motifs\0 », fin par « \0\0 ».
pub(crate) unsafe fn parse_filter(ptr: usize, wide: bool) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if ptr == 0 {
        return out;
    }
    let mut p = ptr;
    loop {
        let (label, next) = read_one(p, wide);
        if label.is_empty() {
            break;
        }
        let (pattern, next2) = read_one(next, wide);
        out.push((label, pattern));
        p = next2;
        if out.len() > 256 {
            break;
        }
    }
    out
}

/// Une chaîne terminée par un zéro et l'adresse qui la suit.
unsafe fn read_one(p: usize, wide: bool) -> (String, usize) {
    if wide {
        let mut n = 0;
        while *(p as *const u16).add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p as *const u16, n));
        (s, p + (n + 1) * 2)
    } else {
        let s = super::cstr(p as *const u8);
        let mut n = 0;
        while *(p as *const u8).add(n) != 0 {
            n += 1;
        }
        (s, p + n + 1)
    }
}

/// Correspondance d'un nom avec un motif `*`/`?`, sans tenir compte de la
/// casse. `*.*` accepte aussi les noms sans point, comme Windows.
pub(crate) fn wildcard(pattern: &str, name: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern == "*.*" || pattern == "*" {
        return true;
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut i, mut j) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while j < n.len() {
        if i < p.len() && (p[i] == '?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

pub(crate) fn matches_any(patterns: &str, name: &str) -> bool {
    patterns.split(';').any(|p| wildcard(p, name))
}

/// Contenu d'un dossier : sous-dossiers d'abord, puis fichiers qui
/// correspondent aux motifs ; noms cachés (« . ») omis ; tri sans casse.
pub(crate) fn list_dir(dir: &Path, patterns: &str) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = std::fs::metadata(e.path()) else {
            continue;
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        let entry = Entry {
            is_dir: meta.is_dir(),
            size: meta.len(),
            mtime,
            name,
        };
        if entry.is_dir {
            dirs.push(entry);
        } else if matches_any(patterns, &entry.name) {
            files.push(entry);
        }
    }
    let key = |e: &Entry| e.name.to_lowercase();
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    dirs.extend(files);
    dirs
}

/// Noms tapés dans « Nom du fichier » : un seul, ou plusieurs entre
/// guillemets (`"a.txt" "b.txt"`) en sélection multiple.
pub(crate) fn split_names(text: &str) -> Vec<String> {
    let t = text.trim();
    if !t.starts_with('"') {
        return if t.is_empty() {
            Vec::new()
        } else {
            vec![t.to_string()]
        };
    }
    t.split('"')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Chemin Windows complet d'un nom tapé, relatif au dossier affiché.
pub(crate) fn resolve(dir_windows: &str, name: &str) -> String {
    let name = name.replace('/', "\\");
    let b = name.as_bytes();
    let absolute =
        (b.len() >= 2 && b[1] == b':') || name.starts_with("\\\\") || name.starts_with('\\');
    if absolute {
        if name.starts_with('\\') && !name.starts_with("\\\\") {
            // Racine du lecteur courant.
            return format!("{}{}", &dir_windows[..2.min(dir_windows.len())], name);
        }
        return name;
    }
    let mut out = dir_windows.trim_end_matches('\\').to_string();
    for part in name.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                if let Some(i) = out.rfind('\\') {
                    if i > 2 {
                        out.truncate(i);
                    } else {
                        out.truncate(3.min(out.len()));
                    }
                }
            }
            p => {
                if !out.ends_with('\\') {
                    out.push('\\');
                }
                out.push_str(p);
            }
        }
    }
    if out.len() == 2 {
        out.push('\\');
    }
    out
}

/// Extension par défaut ajoutée à un nom qui n'en a pas (enregistrement).
pub(crate) fn with_default_ext(path: &str, def_ext: Option<&str>) -> String {
    let Some(ext) = def_ext.filter(|e| !e.is_empty()) else {
        return path.to_string();
    };
    let file = path.rsplit('\\').next().unwrap_or(path);
    if file.contains('.') {
        path.to_string()
    } else {
        format!("{path}.{}", ext.trim_start_matches('.'))
    }
}

/// Résultat écrit dans `lpstrFile` : chemin complet, ou dossier puis noms
/// séparés par des zéros en sélection multiple. Renvoie (texte, offset du
/// nom, offset de l'extension).
pub(crate) fn result_text(paths_win: &[String]) -> (Vec<String>, u16, u16) {
    if paths_win.len() == 1 {
        let p = &paths_win[0];
        let off = p.rfind('\\').map_or(0, |i| i + 1);
        let file = &p[off..];
        let ext = file.rfind('.').map_or(0, |i| {
            p[..off].chars().count() + file[..i].chars().count() + 1
        });
        let off = p[..off].chars().count();
        return (vec![p.clone()], off as u16, ext as u16);
    }
    let dir = paths_win[0]
        .rfind('\\')
        .map_or(String::new(), |i| paths_win[0][..i].to_string());
    let mut parts = vec![dir.clone()];
    for p in paths_win {
        parts.push(p.rsplit('\\').next().unwrap_or(p).to_string());
    }
    (parts, (dir.chars().count() + 1) as u16, 0)
}

fn size_text(bytes: u64) -> String {
    let unit = tr("Ko", "KB", "KB", "KB");
    let kb = bytes.div_ceil(1024);
    let digits = kb.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(if lang() == "en" { ',' } else { '\u{202F}' });
        }
        out.push(c);
    }
    format!("{out} {unit}")
}

fn date_text(secs: i64) -> String {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = secs as libc::time_t;
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return String::new();
    }
    let (d, m, y) = (tm.tm_mday, tm.tm_mon + 1, tm.tm_year + 1900);
    let (hh, mm) = (tm.tm_hour, tm.tm_min);
    match lang().as_str() {
        "en" => format!("{m}/{d}/{y} {hh}:{mm:02}"),
        "de" => format!("{d:02}.{m:02}.{y} {hh:02}:{mm:02}"),
        _ => format!("{d:02}/{m:02}/{y} {hh:02}:{mm:02}"),
    }
}

fn type_text(e: &Entry) -> String {
    if e.is_dir {
        return tr(
            "Dossier de fichiers",
            "File folder",
            "Dateiordner",
            "Carpeta de archivos",
        )
        .to_string();
    }
    match e.name.rfind('.').map(|i| e.name[i + 1..].to_uppercase()) {
        Some(ext) if !ext.is_empty() => match lang().as_str() {
            "en" => format!("{ext} File"),
            "de" => format!("{ext}-Datei"),
            "es" => format!("Archivo {ext}"),
            _ => format!("Fichier {ext}"),
        },
        _ => tr("Fichier", "File", "Datei", "Archivo").to_string(),
    }
}

/// Emplacements de la colonne de gauche : (libellé, dossier Linux).
fn places() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let known = [
        ("Desktop", tr("Bureau", "Desktop", "Desktop", "Escritorio")),
        (
            "Documents",
            tr("Documents", "Documents", "Dokumente", "Documentos"),
        ),
        (
            "Downloads",
            tr("Téléchargements", "Downloads", "Downloads", "Descargas"),
        ),
        ("Pictures", tr("Images", "Pictures", "Bilder", "Imágenes")),
        ("Music", tr("Musique", "Music", "Musik", "Música")),
        ("Videos", tr("Vidéos", "Videos", "Videos", "Vídeos")),
    ];
    for (key, label) in known {
        if let Some(p) = paths::known_folder(key).filter(|p| p.is_dir()) {
            out.push((label.to_string(), p));
        }
    }
    out.push((paths::user_name(), paths::home()));
    out.push((
        tr(
            "Disque local (C:)",
            "Local Disk (C:)",
            "Lokaler Datenträger (C:)",
            "Disco local (C:)",
        )
        .to_string(),
        paths::to_linux("C:\\"),
    ));
    for m in crate::mounts::list() {
        let label = if m.label.is_empty() {
            format!("({}:)", m.letter)
        } else {
            format!("{} ({}:)", m.label, m.letter)
        };
        out.push((label, paths::to_linux(&format!("{}:\\", m.letter))));
    }
    out
}

// ---------- Boîte ----------

unsafe fn fill_list(dlg: usize) {
    let Some((dir, patterns)) = with(|s| {
        let patterns = s.typed_filter.clone().unwrap_or_else(|| {
            s.filters
                .get(s.filter)
                .map_or("*.*".to_string(), |f| f.1.clone())
        });
        (s.dir.clone(), patterns)
    }) else {
        return;
    };
    let entries = list_dir(&dir, &patterns);
    let list = user32::get_dlg_item(dlg, ID_LIST as i32);
    send(list, 0x1009, 0, 0); // LVM_DELETEALLITEMS
    for (i, e) in entries.iter().enumerate() {
        let name = wide(&e.name);
        let mut it = LvItemW {
            mask: 0x1 | 0x4,
            item: i as i32,
            text: name.as_ptr() as usize,
            param: i as isize,
            ..Default::default()
        };
        send(list, 0x104D, 0, &mut it as *mut LvItemW as isize);
        let cols = [
            date_text(e.mtime),
            type_text(e),
            if e.is_dir {
                String::new()
            } else {
                size_text(e.size)
            },
        ];
        for (k, text) in cols.iter().enumerate() {
            let t = wide(text);
            let mut sub = LvItemW {
                mask: 0x1,
                item: i as i32,
                sub_item: k as i32 + 1,
                text: t.as_ptr() as usize,
                ..Default::default()
            };
            send(list, 0x104C, 0, &mut sub as *mut LvItemW as isize);
        }
    }
    with(|s| s.entries = entries);
    set_text(
        user32::get_dlg_item(dlg, ID_PATH as i32),
        &paths::to_windows(&dir),
    );
}

unsafe fn navigate(dlg: usize, dir: PathBuf) {
    if !dir.is_dir() {
        return;
    }
    with(|s| {
        s.dir = dir;
    });
    fill_list(dlg);
}

fn selected_rows(list: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i: isize = -1;
    loop {
        i = send(list, 0x100C, i as usize, 0x2); // LVM_GETNEXTITEM, LVNI_SELECTED
        if i < 0 || out.len() > 10_000 {
            break;
        }
        out.push(i as usize);
    }
    out
}

/// Sélection dans la liste → champ « Nom du fichier ».
unsafe fn selection_changed(dlg: usize) {
    let list = user32::get_dlg_item(dlg, ID_LIST as i32);
    let rows = selected_rows(list);
    let Some(names) = with(|s| {
        rows.iter()
            .filter_map(|&r| s.entries.get(r))
            .filter(|e| !e.is_dir)
            .map(|e| e.name.clone())
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    let text = match names.len() {
        0 => return,
        1 => names[0].clone(),
        _ => names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(" "),
    };
    set_text(user32::get_dlg_item(dlg, ID_NAME as i32), &text);
}

/// Double-clic ou Entrée sur une ligne : ouvre le dossier, ou valide.
unsafe fn activate(dlg: usize) -> bool {
    let list = user32::get_dlg_item(dlg, ID_LIST as i32);
    let rows = selected_rows(list);
    let Some(target) = with(|s| {
        rows.first()
            .and_then(|&r| s.entries.get(r))
            .map(|e| (e.is_dir, s.dir.join(&e.name)))
    })
    .flatten() else {
        return false;
    };
    if target.0 {
        navigate(dlg, target.1);
        set_text(user32::get_dlg_item(dlg, ID_NAME as i32), "");
        return true;
    }
    false
}

/// Bouton « Ouvrir » / « Enregistrer » (ou Entrée).
unsafe fn on_ok(dlg: usize) {
    let focus = user32::get_focus();
    let path_edit = user32::get_dlg_item(dlg, ID_PATH as i32);
    if focus == path_edit {
        let typed = get_text(path_edit);
        let target = paths::to_linux(&typed);
        if target.is_dir() {
            navigate(dlg, target);
        } else {
            message(dlg, &not_found_path(&typed), 0x30);
        }
        return;
    }
    let list = user32::get_dlg_item(dlg, ID_LIST as i32);
    if focus == list && activate(dlg) {
        return;
    }
    let name_edit = user32::get_dlg_item(dlg, ID_NAME as i32);
    let text = get_text(name_edit);
    if text.trim().is_empty() {
        activate(dlg);
        return;
    }
    if text.contains('*') || text.contains('?') {
        with(|s| s.typed_filter = Some(text.trim().to_string()));
        fill_list(dlg);
        return;
    }
    let Some((dir, flags, save, def_ext)) =
        with(|s| (s.dir.clone(), s.flags, s.save, s.def_ext.clone()))
    else {
        return;
    };
    let dir_w = paths::to_windows(&dir);
    let mut names = split_names(&text);
    if flags & OFN_ALLOWMULTISELECT == 0 || save {
        names.truncate(1);
        names = vec![text.trim().trim_matches('"').to_string()];
    }
    let mut out = Vec::new();
    for name in &names {
        let full = resolve(&dir_w, name);
        let linux = paths::to_linux(&full);
        if linux.is_dir() {
            if names.len() == 1 {
                navigate(dlg, linux);
                set_text(name_edit, "");
                return;
            }
            continue;
        }
        let full = if save {
            with_default_ext(&full, def_ext.as_deref())
        } else {
            full
        };
        let linux = paths::to_linux(&full);
        let parent_ok = linux.parent().is_some_and(|p| p.is_dir());
        if !parent_ok && (flags & (OFN_PATHMUSTEXIST | OFN_FILEMUSTEXIST) != 0 || save) {
            message(dlg, &not_found_path(&full), 0x30);
            return;
        }
        if !save && !linux.exists() {
            if flags & OFN_FILEMUSTEXIST != 0 {
                message(dlg, &not_found_file(name), 0x30);
                return;
            }
            if flags & OFN_CREATEPROMPT != 0 && message(dlg, &create_prompt(name), 0x34) != 6 {
                return;
            }
        }
        if save && linux.exists() {
            if flags & OFN_OVERWRITEPROMPT != 0 && message(dlg, &overwrite_prompt(name), 0x34) != 6
            {
                return;
            }
        } else if save
            && flags & OFN_CREATEPROMPT != 0
            && message(dlg, &create_prompt(name), 0x34) != 6
        {
            return;
        }
        out.push(full);
    }
    if out.is_empty() {
        return;
    }
    with(|s| s.result = out);
    dialogs::end_dialog(dlg, 1);
}

unsafe fn message(dlg: usize, text: &str, kind: u32) -> i32 {
    let caption = get_text(dlg);
    dialogs::message_box(dlg, text, &caption, kind)
}

fn not_found_file(name: &str) -> String {
    format!(
        "{name}\n{}",
        tr(
            "Fichier introuvable.\nVérifiez le nom du fichier et réessayez.",
            "File not found.\nCheck the file name and try again.",
            "Datei nicht gefunden.\nÜberprüfen Sie den Dateinamen, und wiederholen Sie den Vorgang.",
            "No se encuentra el archivo.\nCompruebe el nombre del archivo e inténtelo de nuevo.",
        )
    )
}

fn not_found_path(path: &str) -> String {
    format!(
        "{path}\n{}",
        tr(
            "Chemin introuvable.",
            "Path does not exist.",
            "Der Pfad ist nicht vorhanden.",
            "La ruta no existe.",
        )
    )
}

fn overwrite_prompt(name: &str) -> String {
    format!(
        "{name} {}",
        tr(
            "existe déjà.\nVoulez-vous le remplacer ?",
            "already exists.\nDo you want to replace it?",
            "ist bereits vorhanden.\nMöchten Sie sie ersetzen?",
            "ya existe.\n¿Desea reemplazarlo?",
        )
    )
}

fn create_prompt(name: &str) -> String {
    format!(
        "{name}\n{}",
        tr(
            "Ce fichier n'existe pas.\nVoulez-vous le créer ?",
            "This file doesn't exist.\nDo you want to create it?",
            "Diese Datei ist nicht vorhanden.\nMöchten Sie sie erstellen?",
            "Este archivo no existe.\n¿Desea crearlo?",
        )
    )
}

unsafe fn new_folder(dlg: usize) {
    let Some(dir) = with(|s| s.dir.clone()) else {
        return;
    };
    let base = tr(
        "Nouveau dossier",
        "New folder",
        "Neuer Ordner",
        "Nueva carpeta",
    );
    let mut name = base.to_string();
    let mut n = 2;
    while dir.join(&name).exists() && n < 1000 {
        name = format!("{base} ({n})");
        n += 1;
    }
    if std::fs::create_dir(dir.join(&name)).is_ok() {
        fill_list(dlg);
        set_text(user32::get_dlg_item(dlg, ID_NAME as i32), &name);
    }
}

unsafe fn init_dialog(dlg: usize) {
    let Some(st) = STATE.with(|s| s.borrow().clone()) else {
        return;
    };
    let x0 = 16;
    let right = W - 16;
    dialogs::add_control(dlg, "BUTTON", "↑", ID_UP, 0x0001_0000, 0, (x0, 14, 34, 28));
    dialogs::add_control(
        dlg,
        "EDIT",
        "",
        ID_PATH,
        0x0001_0000 | 0x80,
        0x200,
        (x0 + 42, 14, right - x0 - 42, 28),
    );
    let places_lb = dialogs::add_control(
        dlg,
        "LISTBOX",
        "",
        ID_PLACES,
        0x0001_0000 | 0x1 | 0x0020_0000, // LBS_NOTIFY | WS_VSCROLL
        0x200,
        (x0, 54, 190, 318),
    );
    let places = places();
    for (label, _) in &places {
        let t = wide(label);
        send(places_lb, 0x180, 0, t.as_ptr() as isize);
    }
    with(|s| s.places = places.iter().map(|p| p.1.clone()).collect());
    let multi = st.flags & OFN_ALLOWMULTISELECT != 0 && !st.save;
    let list = dialogs::add_control(
        dlg,
        "SysListView32",
        "",
        ID_LIST,
        0x0001_0000 | 0x1 | 0x8 | if multi { 0 } else { 0x4 },
        0x200,
        (x0 + 200, 54, right - x0 - 200, 318),
    );
    let cols = [
        (tr("Nom", "Name", "Name", "Nombre"), 230),
        (
            tr(
                "Modifié le",
                "Date modified",
                "Änderungsdatum",
                "Fecha de modificación",
            ),
            140,
        ),
        (tr("Type", "Type", "Typ", "Tipo"), 120),
        (tr("Taille", "Size", "Größe", "Tamaño"), 80),
    ];
    for (i, (label, cx)) in cols.iter().enumerate() {
        let t = wide(label);
        let mut col = LvColumnW {
            mask: 0x2 | 0x4,
            cx: *cx,
            text: t.as_ptr() as usize,
            ..Default::default()
        };
        send(list, 0x1061, i, &mut col as *mut LvColumnW as isize);
    }
    let y = 388;
    dialogs::add_control(
        dlg,
        "STATIC",
        tr(
            "&Nom du fichier :",
            "File &name:",
            "Datei&name:",
            "&Nombre:",
        ),
        0xFFFF,
        0,
        0,
        (x0, y + 5, 140, 20),
    );
    let name_edit = dialogs::add_control(
        dlg,
        "EDIT",
        &st.initial_name,
        ID_NAME,
        0x0001_0000 | 0x80,
        0x200,
        (x0 + 146, y, 330, 28),
    );
    let combo = dialogs::add_control(
        dlg,
        "COMBOBOX",
        "",
        ID_TYPE,
        0x0001_0000 | 0x3 | 0x0020_0000, // CBS_DROPDOWNLIST
        0,
        (x0 + 486, y, right - x0 - 486, 200),
    );
    for (label, _) in &st.filters {
        let t = wide(label);
        send(combo, 0x143, 0, t.as_ptr() as isize);
    }
    send(combo, 0x14E, st.filter, 0);
    let by = H - 46;
    if st.save {
        dialogs::add_control(
            dlg,
            "BUTTON",
            tr(
                "Nouveau dossier",
                "New folder",
                "Neuer Ordner",
                "Nueva carpeta",
            ),
            ID_NEW_FOLDER,
            0x0001_0000,
            0,
            (x0, by, 150, 30),
        );
    }
    let ok_label = if st.save {
        tr("&Enregistrer", "&Save", "&Speichern", "&Guardar")
    } else {
        tr("&Ouvrir", "&Open", "Ö&ffnen", "&Abrir")
    };
    dialogs::add_control(
        dlg,
        "BUTTON",
        ok_label,
        1,
        0x0001_0000 | 0x1,
        0,
        (right - 200, by, 96, 30),
    );
    dialogs::add_control(
        dlg,
        "BUTTON",
        cancel_label(),
        2,
        0x0001_0000,
        0,
        (right - 96, by, 96, 30),
    );
    fill_list(dlg);
    user32::set_focus(name_edit);
    if !gui::backend_is_interactive() {
        // Sans affichage : WINLUX_FILEDLG fait le choix de l'utilisateur.
        if let Ok(v) = std::env::var("WINLUX_FILEDLG") {
            set_text(name_edit, &v);
            user32::post_message_w(dlg, 0x0111, 1, 0);
        }
    }
}

unsafe extern "win64" fn file_proc(dlg: usize, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        0x0110 => {
            init_dialog(dlg);
            0 // le focus est déjà placé
        }
        0x0111 => {
            let id = wp & 0xFFFF;
            let code = (wp >> 16) & 0xFFFF;
            match (id, code) {
                (1, _) => on_ok(dlg),
                (2, _) => {
                    dialogs::end_dialog(dlg, 0);
                }
                (ID_UP, 0) => {
                    if let Some(parent) = with(|s| s.dir.parent().map(Path::to_path_buf)).flatten()
                    {
                        navigate(dlg, parent);
                    }
                }
                (ID_NEW_FOLDER, 0) => new_folder(dlg),
                (ID_PLACES, 1) => {
                    let lb = user32::get_dlg_item(dlg, ID_PLACES as i32);
                    let i = send(lb, 0x188, 0, 0);
                    if let Some(p) = with(|s| s.places.get(i as usize).cloned()).flatten() {
                        navigate(dlg, p);
                    }
                }
                (ID_TYPE, 1) => {
                    let cb = user32::get_dlg_item(dlg, ID_TYPE as i32);
                    let i = send(cb, 0x147, 0, 0).max(0) as usize;
                    with(|s| {
                        s.filter = i;
                        s.typed_filter = None;
                    });
                    fill_list(dlg);
                }
                _ => {}
            }
            1
        }
        0x004E => {
            let code = *((lp as *const u8).add(16) as *const i32);
            let from = *((lp as *const u8).add(8) as *const usize);
            if from == ID_LIST {
                match code {
                    -101 => {
                        // LVN_ITEMCHANGED : seulement quand la sélection change.
                        let changed = *((lp as *const u8).add(32) as *const u32)
                            ^ *((lp as *const u8).add(36) as *const u32);
                        if changed & 0x2 != 0 {
                            selection_changed(dlg);
                        }
                    }
                    -114 if !activate(dlg) => on_ok(dlg),
                    _ => {}
                }
            }
            1
        }
        _ => 0,
    }
}

// ---------- Fonctions exportées ----------

/// Décalages d'OPENFILENAME (64 bits ; A et W ont la même disposition).
mod ofn {
    pub const SIZE: usize = 0;
    pub const OWNER: usize = 8;
    pub const FILTER: usize = 24;
    pub const FILTER_INDEX: usize = 44;
    pub const FILE: usize = 48;
    pub const MAX_FILE: usize = 56;
    pub const FILE_TITLE: usize = 64;
    pub const MAX_FILE_TITLE: usize = 72;
    pub const INITIAL_DIR: usize = 80;
    pub const TITLE: usize = 88;
    pub const FLAGS: usize = 96;
    pub const FILE_OFFSET: usize = 100;
    pub const FILE_EXT: usize = 102;
    pub const DEF_EXT: usize = 104;
}

// ---------- L'Explorateur Winlux comme boîte Ouvrir / Enregistrer ----------
//
// Un seul Explorateur dans Winlux (#598) : avec un affichage, la boîte
// « Ouvrir » ou « Enregistrer sous » d'un programme .exe est l'Explorateur
// Winlux lui-même (`winlux-shell-app explorer`), avec en bas le champ
// « Nom du fichier », le type et les boutons Ouvrir/Enregistrer et Annuler.
// Il vérifie le nom comme la boîte intégrée (fichier introuvable,
// remplacement, extension par défaut) puis écrit son choix dans un fichier :
// première ligne le numéro du type (à partir de 1), puis un chemin Windows
// par ligne, en Latin-1 comme les chemins de l'Explorateur.
//
// La boîte intégrée reste le repli : sans affichage, sans l'Explorateur
// installé, avec `WINLUX_FILEDLG` (tests) ou `WINLUX_FILEDLG_BUILTIN=1`.

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PickRequest {
    pub save: bool,
    /// Choix d'un dossier (IFileDialog, FOS_PICKFOLDERS).
    pub folder: bool,
    pub flags: u32,
    pub title: String,
    pub filters: Vec<(String, String)>,
    pub filter: usize,
    pub dir: String,
    pub name: String,
    pub def_ext: Option<String>,
}

/// Variables d'environnement passées à l'Explorateur.
pub(crate) fn pick_env(r: &PickRequest, result: &str) -> Vec<(&'static str, String)> {
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    let filters = r
        .filters
        .iter()
        .map(|(d, p)| format!("{}\t{}", clean(d), clean(p)))
        .collect::<Vec<_>>()
        .join("\n");
    vec![
        (
            "WINLUX_PICK",
            if r.folder {
                "folder"
            } else if r.save {
                "save"
            } else {
                "open"
            }
            .to_string(),
        ),
        ("WINLUX_PICK_TITLE", clean(&r.title)),
        ("WINLUX_PICK_FILTERS", filters),
        ("WINLUX_PICK_FILTER", (r.filter + 1).to_string()),
        ("WINLUX_PICK_FLAGS", r.flags.to_string()),
        ("WINLUX_PICK_NAME", clean(&r.name)),
        (
            "WINLUX_PICK_DEFEXT",
            r.def_ext.as_deref().map(clean).unwrap_or_default(),
        ),
        ("WINLUX_PICK_RESULT", result.to_string()),
    ]
}

/// Choix rendu par l'Explorateur : (chemins Windows, type choisi à partir
/// de 0). `None` : annulé.
pub(crate) fn parse_pick_result(bytes: &[u8], filters: usize) -> Option<(Vec<String>, usize)> {
    // Latin-1 : chaque octet est un caractère.
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let mut lines = text.lines().map(str::trim_end).filter(|l| !l.is_empty());
    let filter = lines.next()?.trim().parse::<usize>().ok()?;
    let paths: Vec<String> = lines.map(String::from).collect();
    if paths.is_empty() || paths.iter().any(|p| p.len() < 3 || p.as_bytes()[1] != b':') {
        return None;
    }
    Some((
        paths,
        filter.saturating_sub(1).min(filters.saturating_sub(1)),
    ))
}

pub(crate) fn explorer_app() -> Option<PathBuf> {
    if std::env::var_os("WINLUX_FILEDLG").is_some()
        || std::env::var("WINLUX_FILEDLG_BUILTIN").is_ok_and(|v| v == "1")
        || !gui::backend_is_interactive()
    {
        return None;
    }
    let app = std::env::var_os("WINLUX_SHELL_APP_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/bin/winlux-shell-app"));
    app.is_file().then_some(app)
}

/// `None` : Explorateur indisponible, la boîte intégrée prend le relais.
/// `Some(None)` : annulé. `Some(Some(..))` : choix validé.
pub(crate) unsafe fn pick_with_explorer(
    owner: usize,
    request: &PickRequest,
    app: &Path,
) -> Option<Option<(Vec<String>, usize)>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let result = std::env::temp_dir().join(format!(
        "winlux-pick-{}-{}.txt",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&result);
    let mut cmd = std::process::Command::new(app);
    cmd.arg("explorer").arg(&request.dir);
    for (k, v) in pick_env(request, &paths::to_windows(&result)) {
        cmd.env(k, v);
    }
    let child = cmd.spawn().ok()?;
    crate::thunks::api_note(&format!(
        "boîte de fichiers : Explorateur lancé (pid {}), propriétaire {owner:#x} (#1288)",
        child.id()
    ));
    // Boîte modale : le programme attend, sa fenêtre reste dessinée.
    let owner_enabled = owner != 0 && user32::is_window_enabled(owner) != 0;
    if owner_enabled {
        user32::enable_window(owner, 0);
    }
    // La fin de l'Explorateur est guettée par un fil à part, qui réveille
    // ensuite la file de messages de ce fil. Sans ce réveil, une boucle de
    // messages imbriquée du programme, ouverte pendant un message que la
    // boucle ci-dessous distribue (Qt de VLC), attendait dans
    // MsgWaitForMultipleObjectsEx un message qui ne venait jamais : la
    // fenêtre propriétaire est désactivée, la souris n'y produit rien, et
    // la boucle ci-dessous ne reprenait jamais la main (#1288).
    let child = std::sync::Arc::new(std::sync::Mutex::new(child));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let (child, done) = (child.clone(), done.clone());
        let tid = std::thread::current().id();
        std::thread::spawn(move || loop {
            let exited = !matches!(
                child.lock().unwrap_or_else(|e| e.into_inner()).try_wait(),
                Ok(None)
            );
            if exited {
                done.store(true, std::sync::atomic::Ordering::SeqCst);
                gui::post_to_thread(tid, gui::Msg::new(0, 0, 0, 0)); // WM_NULL
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        });
    }
    let mut msg: gui::Msg = std::mem::zeroed();
    loop {
        if done.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let mut quit = None;
        while user32::peek_message_w(&mut msg, 0, 0, 0, 1) != 0 {
            if msg.message == 0x0012 {
                quit = Some(msg.wparam as i32);
                break;
            }
            user32::translate_message(&msg);
            user32::dispatch_message_w(&msg);
        }
        if let Some(code) = quit {
            let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
            let _ = child.kill();
            let _ = child.wait();
            user32::post_quit_message(code);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    crate::thunks::api_note("boîte de fichiers : Explorateur fermé, choix relu (#1288)");
    if owner_enabled {
        user32::enable_window(owner, 1);
    }
    if owner != 0 && gui::is_window(owner) {
        user32::set_focus(owner);
    }
    let bytes = std::fs::read(&result).unwrap_or_default();
    let _ = std::fs::remove_file(&result);
    Some(parse_pick_result(&bytes, request.filters.len()))
}

pub(crate) unsafe fn file_dialog(ofn_ptr: *mut u8, wide: bool, save: bool) -> i32 {
    set_error(0);
    if ofn_ptr.is_null() {
        set_error(CDERR_INITIALIZATION);
        return 0;
    }
    let size = rd::<u32>(ofn_ptr, ofn::SIZE);
    if size != 136 && size != 152 && size != 76 && size != 88 {
        set_error(CDERR_STRUCTSIZE);
        return 0;
    }
    let owner = rd::<usize>(ofn_ptr, ofn::OWNER);
    let flags = rd::<u32>(ofn_ptr, ofn::FLAGS);
    let mut filters = parse_filter(rd::<usize>(ofn_ptr, ofn::FILTER), wide);
    if filters.is_empty() {
        filters.push((
            tr(
                "Tous les fichiers (*.*)",
                "All files (*.*)",
                "Alle Dateien (*.*)",
                "Todos los archivos (*.*)",
            )
            .to_string(),
            "*.*".to_string(),
        ));
    }
    let filter = (rd::<u32>(ofn_ptr, ofn::FILTER_INDEX) as usize)
        .saturating_sub(1)
        .min(filters.len() - 1);
    let file_buf = rd::<usize>(ofn_ptr, ofn::FILE);
    let max_file = rd::<u32>(ofn_ptr, ofn::MAX_FILE);
    let initial = read_opt(file_buf, wide).unwrap_or_default();
    let initial_dir = read_opt(rd::<usize>(ofn_ptr, ofn::INITIAL_DIR), wide);
    let def_ext = read_opt(rd::<usize>(ofn_ptr, ofn::DEF_EXT), wide);
    let title = read_opt(rd::<usize>(ofn_ptr, ofn::TITLE), wide).unwrap_or_else(|| {
        if save {
            tr(
                "Enregistrer sous",
                "Save As",
                "Speichern unter",
                "Guardar como",
            )
        } else {
            tr("Ouvrir", "Open", "Öffnen", "Abrir")
        }
        .to_string()
    });

    // Dossier de départ : celui du nom proposé, lpstrInitialDir, sinon
    // Documents.
    let (mut dir, mut initial_name) = (None, initial.clone());
    if initial.contains('\\') || initial.contains('/') {
        let w = initial.replace('/', "\\");
        let cut = w.rfind('\\').unwrap_or(0);
        let d = paths::to_linux(&w[..cut.max(2)]);
        if d.is_dir() {
            dir = Some(d);
            initial_name = w[cut + 1..].to_string();
        }
    }
    if dir.is_none() {
        dir = initial_dir
            .map(|d| paths::to_linux(&d))
            .filter(|d| d.is_dir());
    }
    let dir = dir
        .or_else(|| paths::known_folder("Documents").filter(|d| d.is_dir()))
        .unwrap_or_else(paths::home);

    let request = PickRequest {
        save,
        folder: false,
        flags,
        title: title.clone(),
        filters: filters.clone(),
        filter,
        dir: paths::to_windows(&dir),
        name: initial_name.clone(),
        def_ext: def_ext.clone(),
    };
    let picked = explorer_app().and_then(|app| pick_with_explorer(owner, &request, &app));
    let (result, filter) = match picked {
        Some(Some(chosen)) => chosen,
        Some(None) => return 0,
        None => {
            STATE.with(|s| {
                *s.borrow_mut() = Some(FileState {
                    save,
                    flags,
                    filters,
                    filter,
                    def_ext,
                    dir,
                    initial_name,
                    ..Default::default()
                })
            });
            let r = dialogs::run_builtin(owner, &title, W, H, file_proc as *const () as usize, 0);
            let st = STATE.with(|s| s.borrow_mut().take()).unwrap_or_default();
            if r != 1 || st.result.is_empty() {
                return 0;
            }
            (st.result, st.filter)
        }
    };
    let st = FileState {
        result,
        filter,
        ..Default::default()
    };
    let (parts, off, ext) = result_text(&st.result);
    let need: usize = parts.iter().map(|p| p.chars().count() + 1).sum::<usize>() + 1;
    if need > max_file as usize {
        if file_buf != 0 && max_file >= 2 {
            std::ptr::write_unaligned(file_buf as *mut u16, need.min(0xFFFF) as u16);
        }
        set_error(FNERR_BUFFERTOOSMALL);
        return 0;
    }
    // Chaînes séparées par des zéros, double zéro final.
    let mut pos = 0usize;
    for p in &parts {
        let n = p.chars().count();
        if wide {
            super::put_text_w(p, (file_buf as *mut u16).add(pos), (n + 1) as u32);
        } else {
            super::put_text_a(p, (file_buf as *mut u8).add(pos), (n + 1) as u32);
        }
        pos += n + 1;
    }
    if wide {
        *(file_buf as *mut u16).add(pos) = 0;
    } else {
        *(file_buf as *mut u8).add(pos) = 0;
    }
    wr::<u16>(ofn_ptr, ofn::FILE_OFFSET, off);
    wr::<u16>(ofn_ptr, ofn::FILE_EXT, ext);
    wr::<u32>(ofn_ptr, ofn::FILTER_INDEX, st.filter as u32 + 1);
    let title_buf = rd::<usize>(ofn_ptr, ofn::FILE_TITLE);
    let max_title = rd::<u32>(ofn_ptr, ofn::MAX_FILE_TITLE);
    if title_buf != 0 && max_title > 0 && st.result.len() == 1 {
        let name = st.result[0].rsplit('\\').next().unwrap_or("").to_string();
        if wide {
            super::put_text_w(&name, title_buf as *mut u16, max_title);
        } else {
            super::put_text_a(&name, title_buf as *mut u8, max_title);
        }
    }
    1
}

pub unsafe extern "win64" fn get_open_file_name_a(ofn: *mut u8) -> i32 {
    file_dialog(ofn, false, false)
}
pub unsafe extern "win64" fn get_open_file_name_w(ofn: *mut u8) -> i32 {
    file_dialog(ofn, true, false)
}
pub unsafe extern "win64" fn get_save_file_name_a(ofn: *mut u8) -> i32 {
    file_dialog(ofn, false, true)
}
pub unsafe extern "win64" fn get_save_file_name_w(ofn: *mut u8) -> i32 {
    file_dialog(ofn, true, true)
}

/// GetFileTitle : nom affiché d'un chemin (sans le dossier).
pub unsafe extern "win64" fn get_file_title_a(path: *const u8, buf: *mut u8, cap: u16) -> i16 {
    if path.is_null() {
        return -1;
    }
    let p = super::cstr(path);
    let name = p.rsplit(['\\', '/']).next().unwrap_or(&p).to_string();
    let need = name.len() + 1;
    if buf.is_null() || (cap as usize) < need {
        return need as i16;
    }
    super::put_text_a(&name, buf, cap as u32);
    0
}
pub unsafe extern "win64" fn get_file_title_w(path: *const u16, buf: *mut u16, cap: u16) -> i16 {
    if path.is_null() {
        return -1;
    }
    let p = super::wstr(path);
    let name = p.rsplit(['\\', '/']).next().unwrap_or(&p).to_string();
    let need = name.encode_utf16().count() + 1;
    if buf.is_null() || (cap as usize) < need {
        return need as i16;
    }
    super::put_text_w(&name, buf, cap as u32);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motifs_sans_casse_et_etoile_point_etoile() {
        assert!(wildcard("*.txt", "Notes.TXT"));
        assert!(!wildcard("*.txt", "notes.txt.bak"));
        assert!(wildcard("*.*", "Makefile"));
        assert!(wildcard("putty?.log", "putty1.log"));
        assert!(matches_any("*.ppk;*.pem", "cle.PEM"));
        assert!(!matches_any("*.ppk;*.pem", "cle.pub"));
    }

    #[test]
    fn filtre_double_zero() {
        let raw: Vec<u16> = "Clés (*.ppk)\0*.ppk\0Tous (*.*)\0*.*\0\0"
            .encode_utf16()
            .collect();
        let f = unsafe { parse_filter(raw.as_ptr() as usize, true) };
        assert_eq!(f.len(), 2);
        assert_eq!(f[0], ("Clés (*.ppk)".to_string(), "*.ppk".to_string()));
        let raw = b"Text\0*.txt\0\0";
        let f = unsafe { parse_filter(raw.as_ptr() as usize, false) };
        assert_eq!(f, vec![("Text".to_string(), "*.txt".to_string())]);
    }

    #[test]
    fn chemins_tapes_et_extension_par_defaut() {
        let d = "C:\\Users\\damien\\Documents";
        assert_eq!(resolve(d, "a.txt"), "C:\\Users\\damien\\Documents\\a.txt");
        assert_eq!(resolve(d, "..\\Bureau"), "C:\\Users\\damien\\Bureau");
        assert_eq!(resolve(d, "D:\\x.log"), "D:\\x.log");
        assert_eq!(resolve(d, "\\Temp"), "C:\\Temp");
        assert_eq!(resolve("C:\\", ".."), "C:\\");
        assert_eq!(
            with_default_ext("C:\\a\\cle", Some("ppk")),
            "C:\\a\\cle.ppk"
        );
        assert_eq!(
            with_default_ext("C:\\a\\cle.txt", Some("ppk")),
            "C:\\a\\cle.txt"
        );
        assert_eq!(
            split_names("\"a.txt\" \"b c.txt\""),
            vec!["a.txt", "b c.txt"]
        );
        assert_eq!(split_names("simple.txt"), vec!["simple.txt"]);
    }

    #[test]
    fn resultat_simple_et_multiple() {
        let (p, off, ext) = result_text(&["C:\\Users\\d\\cle.ppk".to_string()]);
        assert_eq!(p, vec!["C:\\Users\\d\\cle.ppk"]);
        assert_eq!((off, ext), (11, 15));
        let (p, off, _) = result_text(&["C:\\d\\a.txt".to_string(), "C:\\d\\b.txt".to_string()]);
        assert_eq!(p, vec!["C:\\d", "a.txt", "b.txt"]);
        assert_eq!(off, 5);
    }

    #[test]
    fn explorateur_comme_boite_ouvrir_enregistrer() {
        let r = PickRequest {
            save: true,
            folder: false,
            flags: OFN_OVERWRITEPROMPT,
            title: "Enregistrer sous".into(),
            filters: vec![
                ("Texte (*.txt)".into(), "*.txt".into()),
                ("Tous\tles fichiers".into(), "*.*".into()),
            ],
            filter: 1,
            dir: "C:\\Users\\d\\Documents".into(),
            name: "note.txt".into(),
            def_ext: Some("txt".into()),
        };
        let env = pick_env(&r, "Z:\\tmp\\r.txt");
        let get = |k: &str| {
            env.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(get("WINLUX_PICK"), "save");
        assert_eq!(get("WINLUX_PICK_FILTER"), "2");
        assert_eq!(
            get("WINLUX_PICK_FILTERS"),
            "Texte (*.txt)\t*.txt\nTous les fichiers\t*.*"
        );
        assert_eq!(get("WINLUX_PICK_FLAGS"), "2");
        assert_eq!(get("WINLUX_PICK_DEFEXT"), "txt");
        assert_eq!(get("WINLUX_PICK_RESULT"), "Z:\\tmp\\r.txt");
        assert_eq!(
            parse_pick_result(b"2\nC:\\d\\caf\xE9.txt\n", 2),
            Some((vec!["C:\\d\\caf\u{e9}.txt".to_string()], 1))
        );
        assert_eq!(
            parse_pick_result(b"1\r\nC:\\d\\a.txt\r\nC:\\d\\b.txt\r\n", 2),
            Some((
                vec!["C:\\d\\a.txt".to_string(), "C:\\d\\b.txt".to_string()],
                0
            ))
        );
        assert_eq!(parse_pick_result(b"9\nC:\\a", 2).map(|r| r.1), Some(1));
        assert_eq!(parse_pick_result(b"", 2), None);
        assert_eq!(parse_pick_result(b"1\n", 2), None);
        assert_eq!(parse_pick_result(b"1\nrelatif.txt\n", 2), None);
    }

    /// #1288 : une boucle de messages imbriquée du programme (Qt de VLC),
    /// ouverte pendant que la boîte attend l'Explorateur, est réveillée à
    /// la fermeture de l'Explorateur ; avant, VLC restait figé.
    #[test]
    fn boucle_imbriquee_reveillee_a_la_fin_de_l_explorateur() {
        use std::os::unix::fs::PermissionsExt;
        unsafe extern "win64" fn imbrique(h: usize, m: u32, w: usize, l: isize) -> isize {
            if m == 0x8001 {
                // Attente sans délai, comme QEventDispatcherWin32.
                user32::msg_wait_for_multiple_objects(0, std::ptr::null(), 0, u32::MAX, 0x04FF);
                return 1;
            }
            gui::def_window_proc(h, m, w, l)
        }
        let dir = std::env::temp_dir().join(format!("winlux-pick-imbrique-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("winlux-shell-app");
        std::fs::write(&app, "#!/bin/sh\nsleep 0.3\n").unwrap();
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let app2 = app.clone();
        std::thread::spawn(move || unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = user32::create_window_ex_w(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                40,
                40,
                0,
                0,
                0,
                std::ptr::null_mut(),
            );
            user32::set_window_long_ptr_w(hwnd, -4, imbrique as *const () as isize);
            user32::post_message_w(hwnd, 0x8001, 0, 0);
            let r = PickRequest {
                save: false,
                folder: false,
                flags: 0,
                title: "Ouvrir".into(),
                filters: vec![],
                filter: 0,
                dir: "C:\\Users\\d".into(),
                name: String::new(),
                def_ext: None,
            };
            let got = pick_with_explorer(0, &r, &app2);
            user32::destroy_window(hwnd);
            let _ = tx.send(got);
        });
        let got = rx.recv_timeout(std::time::Duration::from_secs(10));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            got,
            Ok(Some(None)),
            "la boîte d'ouverture est restée bloquée"
        );
    }

    #[test]
    fn l_explorateur_rend_le_fichier_choisi() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("winlux-pick-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Faux Explorateur : vérifie ses arguments puis choisit un fichier.
        let app = dir.join("winlux-shell-app");
        std::fs::write(
            &app,
            "#!/bin/sh\n\
             test \"$1\" = explorer || exit 1\n\
             out=$(printf '%s' \"$WINLUX_PICK_RESULT\" | sed 's/^[Zz]://; s@\\\\@/@g')\n\
             if test \"$WINLUX_PICK\" = save; then\n\
               printf '2\\n%s\\\\%s.txt\\n' \"$2\" \"$WINLUX_PICK_NAME\" > \"$out\"\n\
             fi\n",
        )
        .unwrap();
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut r = PickRequest {
            save: true,
            folder: false,
            flags: 0,
            title: "Enregistrer sous".into(),
            filters: vec![("A".into(), "*.a".into()), ("Texte".into(), "*.txt".into())],
            filter: 0,
            dir: "C:\\Users\\d\\Documents".into(),
            name: "note".into(),
            def_ext: None,
        };
        let got = unsafe { pick_with_explorer(0, &r, &app) };
        assert_eq!(
            got,
            Some(Some((
                vec!["C:\\Users\\d\\Documents\\note.txt".to_string()],
                1
            )))
        );
        // Fenêtre fermée sans choix : annulé, pas de repli sur l'autre boîte.
        r.save = false;
        assert_eq!(unsafe { pick_with_explorer(0, &r, &app) }, Some(None));
        // Explorateur absent : la boîte intégrée prend le relais.
        assert_eq!(
            unsafe { pick_with_explorer(0, &r, &dir.join("absent")) },
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dossiers_avant_fichiers_et_caches_omis() {
        let dir = std::env::temp_dir().join(format!("winlux-filedlg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Zeta")).unwrap();
        std::fs::write(dir.join("b.txt"), "x").unwrap();
        std::fs::write(dir.join("A.TXT"), "x").unwrap();
        std::fs::write(dir.join("c.log"), "x").unwrap();
        std::fs::write(dir.join(".cache"), "x").unwrap();
        let names: Vec<String> = list_dir(&dir, "*.txt")
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, vec!["Zeta", "A.TXT", "b.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
