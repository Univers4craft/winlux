//! Explorer : l'explorateur de fichiers de Winlux, au style Windows 11 et à
//! l'ergonomie de Nemo (Linux Mint).
//!
//! - onglets dans la barre de titre (Ctrl+T, Ctrl+W, clic milieu sur un
//!   dossier) et double volet dans un même onglet (F3) : plusieurs dossiers
//!   dans une seule fenêtre ;
//! - volet de navigation : Accueil, dossiers personnels, Ce PC (C: = dossier
//!   personnel Linux, Z: = racine du système), Corbeille ;
//! - modes d'affichage : très grandes, grandes, moyennes et petites icônes,
//!   liste, détails, mosaïques ; option compacte, fichiers cachés, extensions ;
//! - tri par nom, date, type ou taille ; recherche instantanée dans le dossier,
//!   Entrée pour chercher aussi dans les sous-dossiers ;
//! - sélection au clic, Ctrl, Maj ou au lasso ; couper, copier, coller,
//!   renommer, nouveau dossier, suppression vers la Corbeille ;
//! - double-clic : dossier ouvert sur place, `.exe` lancé dans Winlux, autre
//!   fichier ouvert avec l'application Linux associée.
//!
//! Tout passe par les API Win32 Unicode du runtime (`FindFirstFileW`, `MoveFileW`…) :
//! Explorer est lui-même un programme Windows qui s'exécute dans Winlux.

use crate::gfx::*;
use crate::locale::*;
use crate::wm::{self, Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = true;

const MAX_ENTRIES: usize = 1024;
const PATH_LEN: usize = 260;
// FIND_DATA fournit jusqu'à 259 octets : le libellé affiché doit rester le
// même nom que celui utilisé par les opérations sur les fichiers.
const NAME_LEN: usize = PATH_LEN;
const MAX_TABS: usize = 6;
const HISTORY: usize = 12;
const CLIP_MAX: usize = 64;

const TOOLBAR_H: i32 = 48;
const COMMAND_H: i32 = 46;
const STATUS_H: i32 = 28;
const NAV_W: i32 = 228;
const TAB_W: i32 = 220;
const SCROLL_W: i32 = 10;

// ---------- Données ----------

#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
enum Kind {
    Folder,
    Exe,
    Image,
    Audio,
    Video,
    Archive,
    Code,
    Pdf,
    Text,
    File,
    Drive,
    LinuxDrive,
    /// Fichier image disque (iso, vhd, qcow2…), montable comme un lecteur.
    DiskImage,
    /// Image disque montée (lecteur dynamique).
    Removable,
    /// Lecteur réseau (FTP, SFTP, SMB…).
    Network,
}

#[derive(Clone, Copy)]
struct Entry {
    name: [u8; NAME_LEN],
    len: u16,
    dir: bool,
    hidden: bool,
    kind: Kind,
    size: u64,
    mtime: u64,
}

const EMPTY_ENTRY: Entry = Entry {
    name: [0; NAME_LEN],
    len: 0,
    dir: false,
    hidden: false,
    kind: Kind::Folder,
    size: 0,
    mtime: 0,
};

impl Entry {
    fn name(&self) -> &[u8] {
        &self.name[..self.len as usize]
    }
}

#[derive(Clone, Copy)]
pub struct Path {
    buf: [u8; PATH_LEN],
    len: u16,
}
const EMPTY_PATH: Path = Path {
    buf: [0; PATH_LEN],
    len: 0,
};
impl Path {
    pub fn get(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }
    fn set(&mut self, s: &[u8]) -> bool {
        if !valid_path(s) {
            return false;
        }
        self.buf[..s.len()].copy_from_slice(s);
        self.len = s.len() as u16;
        true
    }
    fn is(&self, s: &[u8]) -> bool {
        self.get() == s
    }
}

/// Champ de saisie d'une ligne (recherche, adresse, renommage).
#[derive(Clone, Copy)]
struct Field {
    buf: [u8; PATH_LEN],
    len: usize,
}
const EMPTY_FIELD: Field = Field {
    buf: [0; PATH_LEN],
    len: 0,
};
impl Field {
    fn get(&self) -> &[u8] {
        &self.buf[..self.len]
    }
    fn set(&mut self, s: &[u8]) {
        let n = s.len().min(PATH_LEN - 1);
        self.buf[..n].copy_from_slice(&s[..n]);
        self.len = n;
    }
    fn push(&mut self, c: u8) {
        if self.len < PATH_LEN - 1 {
            self.buf[self.len] = c;
            self.len += 1;
        }
    }
    fn pop(&mut self) {
        if self.len > 0 {
            self.len -= 1;
        }
    }
}

struct Pane {
    loc: Path,
    back: [Path; HISTORY],
    nback: usize,
    fwd: [Path; HISTORY],
    nfwd: usize,
    entries: [Entry; MAX_ENTRIES],
    count: usize,
    /// Indices filtrés et triés réellement affichés.
    view: [u16; MAX_ENTRIES],
    nview: usize,
    sel: [u64; MAX_ENTRIES / 64],
    anchor: usize,
    scroll: i32,
    scroll_target: i32,
    scroll_x: i32,
    search: Field,
    /// Résultats d'une recherche dans les sous-dossiers (noms = chemins relatifs).
    deep: bool,
    truncated: bool,
    missing: bool,
}

const EMPTY_PANE: Pane = Pane {
    loc: EMPTY_PATH,
    back: [EMPTY_PATH; HISTORY],
    nback: 0,
    fwd: [EMPTY_PATH; HISTORY],
    nfwd: 0,
    entries: [EMPTY_ENTRY; MAX_ENTRIES],
    count: 0,
    view: [0; MAX_ENTRIES],
    nview: 0,
    sel: [0; MAX_ENTRIES / 64],
    anchor: 0,
    scroll: 0,
    scroll_target: 0,
    scroll_x: 0,
    search: EMPTY_FIELD,
    deep: false,
    truncated: false,
    missing: false,
};

struct Tab {
    panes: [Pane; 2],
    dual: bool,
    focus: usize,
}
const EMPTY_TAB: Tab = Tab {
    panes: [EMPTY_PANE, EMPTY_PANE],
    dual: false,
    focus: 0,
};

static mut TABS: [Tab; MAX_TABS] = [
    EMPTY_TAB, EMPTY_TAB, EMPTY_TAB, EMPTY_TAB, EMPTY_TAB, EMPTY_TAB,
];
static mut NTABS: usize = 0;
static mut CUR: usize = 0;

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    XLarge,
    Large,
    Medium,
    Small,
    List,
    Details,
    Tiles,
}
#[derive(Clone, Copy, PartialEq)]
enum SortKey {
    Name,
    Date,
    Kind,
    Size,
}

static mut VIEW: View = View::Details;
static mut COMPACT: bool = false;
static mut SHOW_HIDDEN: bool = false;
static mut SHOW_EXT: bool = true;
static mut SHOW_NAV: bool = true;
static mut SORT: SortKey = SortKey::Name;
static mut SORT_DESC: bool = false;

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    List,
    Search,
    Address,
    Rename,
}
static mut FOCUS: Focus = Focus::List;
static mut ADDRESS: Field = EMPTY_FIELD;
static mut ADDRESS_SELECTED: bool = false;
static mut RENAME: Field = EMPTY_FIELD;
static mut RENAME_INDEX: usize = usize::MAX;
/// Nom entièrement sélectionné au début d'un renommage : la première frappe le remplace.
static mut RENAME_FRESH: bool = false;
static mut RENAME_TAIL: usize = 0;

#[derive(Clone, Copy, PartialEq)]
enum Menu {
    None,
    View,
    Sort,
    New,
    Context { x: i32, y: i32, on_item: bool },
}
static mut MENU: Menu = Menu::None;
static mut HOVER: i32 = -1;
static mut HOVER_ZONE: u32 = 0;
static mut LASSO: Option<(i32, i32, i32, i32)> = None;
static mut SCROLL_DRAG: Option<(i32, i32)> = None;
static mut FILE_DRAG_START: Option<(i32, i32)> = None;
static mut FILE_DRAG_ACTIVE: bool = false;
static mut MESSAGE: Field = EMPTY_FIELD;
static mut MESSAGE_UNTIL: u32 = 0;

static mut CLIP: [Path; CLIP_MAX] = [EMPTY_PATH; CLIP_MAX];
static mut NCLIP: usize = 0;
static mut CLIP_CUT: bool = false;
/// Numéro du presse-papiers système correspondant à `CLIP`.
static mut CLIP_SEQ: u32 = 0;

const HOME: &[u8] = b"::home";
const PC: &[u8] = b"::pc";

// ---------- Chaînes et chemins ----------

fn lower(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' => c + 32,
        0xC0..=0xDE if c != 0xD7 => c + 32,
        _ => c,
    }
}
fn cmp_ci(a: &[u8], b: &[u8]) -> core::cmp::Ordering {
    for i in 0..a.len().min(b.len()) {
        let (x, y) = (lower(a[i]), lower(b[i]));
        if x != y {
            return x.cmp(&y);
        }
    }
    a.len().cmp(&b.len())
}
fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    (0..=hay.len() - needle.len())
        .any(|i| (0..needle.len()).all(|k| lower(hay[i + k]) == lower(needle[k])))
}
fn ends_ci(s: &[u8], suffix: &[u8]) -> bool {
    s.len() >= suffix.len() && cmp_ci(&s[s.len() - suffix.len()..], suffix).is_eq()
}

fn extension(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|&c| c == b'.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => b"",
    }
}

/// Construit `dir\name` dans `out` et renvoie sa longueur.
/// Zéro signifie chemin non représentable : jamais de chemin tronqué.
fn join(out: &mut [u8; PATH_LEN + NAME_LEN + 2], dir: &[u8], name: &[u8]) -> usize {
    out[0] = 0;
    let sep = !dir.is_empty() && !matches!(dir.last(), Some(b'\\' | b'/'));
    let n = dir.len() + usize::from(sep) + name.len();
    if n == 0 || n >= PATH_LEN || dir.contains(&0) || name.contains(&0) {
        return 0;
    }
    out[..dir.len()].copy_from_slice(dir);
    let at = dir.len() + usize::from(sep);
    if sep {
        out[dir.len()] = b'\\';
    }
    out[at..n].copy_from_slice(name);
    out[n] = 0;
    n
}

fn valid_path(s: &[u8]) -> bool {
    s.len() < PATH_LEN && !s.contains(&0)
}

fn valid_component(name: &[u8]) -> bool {
    !name.is_empty()
        && valid_path(name)
        && !name.iter().any(|&c| {
            c < 32
                || matches!(
                    c,
                    b'\\' | b'/' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|'
                )
        })
}

/// Copie en chaîne C terminée par zéro.
fn cstr<'a>(buf: &'a mut [u8; PATH_LEN + NAME_LEN + 2], s: &[u8]) -> *const core::ffi::c_char {
    if !valid_path(s) || s.is_empty() {
        buf[0] = 0;
        return core::ptr::null();
    }
    buf[..s.len()].copy_from_slice(s);
    buf[s.len()] = 0;
    buf.as_ptr() as *const core::ffi::c_char
}

/// Convertit un chemin interne (Windows-1252 / ASCII) en chaîne wide (UTF-16)
/// terminée par zéro, pour les API Win32 W.
fn wstr(buf: &mut [u16; PATH_LEN + NAME_LEN + 2], s: &[u8]) -> *const u16 {
    if !valid_path(s) || s.is_empty() {
        buf[0] = 0;
        return core::ptr::null();
    }
    for (i, &c) in s.iter().enumerate() {
        buf[i] = c as u16;
    }
    buf[s.len()] = 0;
    buf.as_ptr()
}

/// Construit `dir\name` en wide (UTF-16) dans `buf` et renvoie la longueur.
fn wjoin(buf: &mut [u16; PATH_LEN + NAME_LEN + 2], dir: &[u8], name: &[u8]) -> usize {
    let sep = !dir.is_empty() && !matches!(dir.last(), Some(b'\\' | b'/'));
    let n = dir.len() + usize::from(sep) + name.len();
    if n == 0 || n >= PATH_LEN || dir.contains(&0) || name.contains(&0) {
        buf[0] = 0;
        return 0;
    }
    for (i, &c) in dir.iter().enumerate() {
        buf[i] = c as u16;
    }
    let at = dir.len() + usize::from(sep);
    if sep {
        buf[dir.len()] = b'\\' as u16;
    }
    for (i, &c) in name.iter().enumerate() {
        buf[at + i] = c as u16;
    }
    buf[n] = 0;
    n
}

/// Lit le nom de fichier depuis Win32FindDataW (UTF-16 → Windows-1252).
fn find_name_w(d: &Win32FindDataW) -> &[u8] {
    static mut NAME_BUF: [u8; 260] = [0; 260];
    let raw = &d.file_name;
    let n = raw.iter().position(|&c| c == 0).unwrap_or(260);
    unsafe {
        for i in 0..n {
            NAME_BUF[i] = if raw[i] <= 0xFF { raw[i] as u8 } else { b'?' };
        }
        &NAME_BUF[..n]
    }
}

fn parent(path: &[u8]) -> Option<&[u8]> {
    if path.len() <= 3 {
        return None;
    }
    let i = path.iter().rposition(|&c| c == b'\\')?;
    Some(if i <= 2 { &path[..3] } else { &path[..i] })
}

fn last_component(path: &[u8]) -> &[u8] {
    if path == HOME {
        return tr(S_HOME);
    }
    if path == PC {
        return tr(S_THIS_PC);
    }
    if path.len() <= 3 {
        let letter = path.first().copied().unwrap_or(b'C');
        if letter != b'C' && letter != b'Z' {
            unsafe {
                for i in 0..NDRIVES {
                    if DRIVES[i].letter == letter {
                        return drive_label(i);
                    }
                }
            }
        }
        return if path.first() == Some(&b'Z') {
            tr4(&[b"Syst\xE8me Linux (Z:)", b"Linux system (Z:)", b"Sistema Linux (Z:)", b"Linux-System (Z:)"])
        } else {
            tr4(&[b"Dossier personnel (C:)", b"Home folder (C:)", b"Carpeta personal (C:)", b"Pers\xF6nlicher Ordner (C:)"])
        };
    }
    match path.iter().rposition(|&c| c == b'\\') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

fn exists_dir(path: &[u8]) -> bool {
    let mut buf = [0u16; PATH_LEN + NAME_LEN + 2];
    let a = unsafe { GetFileAttributesW(wstr(&mut buf, path)) };
    a != u32::MAX && a & 0x10 != 0
}

/// Dossier personnel XDG : nom français, sinon anglais (Ubuntu les crée selon
/// la langue de la session).
fn user_folder(out: &mut Path, fr: &[u8], en: &[u8]) {
    let mut buf = [0u8; PATH_LEN + NAME_LEN + 2];
    let n = join(&mut buf, b"C:", en);
    if exists_dir(&buf[..n]) {
        out.set(&buf[..n]);
        return;
    }
    let n = join(&mut buf, b"C:", fr);
    out.set(&buf[..n]);
}

#[derive(Clone, Copy)]
struct Place {
    name: &'static [u8],
    fr: &'static [u8],
    en: &'static [u8],
    glyph: Glyph,
}
const PLACES: [Place; 6] = [
    Place {
        name: b"Bureau",
        fr: b"Bureau",
        en: b"Desktop",
        glyph: Glyph::Desktop,
    },
    Place {
        name: b"T\xE9l\xE9chargements",
        fr: b"T\xE9l\xE9chargements",
        en: b"Downloads",
        glyph: Glyph::Download,
    },
    Place {
        name: b"Documents",
        fr: b"Documents",
        en: b"Documents",
        glyph: Glyph::Text,
    },
    Place {
        name: b"Images",
        fr: b"Images",
        en: b"Pictures",
        glyph: Glyph::Image,
    },
    Place {
        name: b"Musique",
        fr: b"Musique",
        en: b"Music",
        glyph: Glyph::Music,
    },
    Place {
        name: b"Vid\xE9os",
        fr: b"Vid\xE9os",
        en: b"Videos",
        glyph: Glyph::Video,
    },
];
const TRASH: &[u8] = b"C:\\.local\\share\\Trash\\files";

fn place_label(i: usize) -> &'static [u8] {
    match i {
        0 => tr(S_DESKTOP_FOLDER),
        1 => tr(S_DOWNLOADS),
        2 => tr(S_DOCUMENTS),
        3 => tr(S_IMAGES),
        4 => tr(S_MUSIC),
        5 => tr(S_VIDEOS),
        _ => PLACES[i].name,
    }
}

fn place_path(i: usize) -> Path {
    let mut p = EMPTY_PATH;
    user_folder(&mut p, PLACES[i].fr, PLACES[i].en);
    p
}

fn kind_of(name: &[u8], dir: bool) -> Kind {
    if dir {
        return Kind::Folder;
    }
    let e = extension(name);
    let any = |list: &[&[u8]]| list.iter().any(|x| cmp_ci(e, x).is_eq());
    if any(&[b"exe", b"com", b"msi", b"bat", b"cmd", b"appimage", b"sh"]) {
        Kind::Exe
    } else if any(&[
        b"png", b"jpg", b"jpeg", b"gif", b"bmp", b"webp", b"svg", b"ico", b"tif", b"tiff", b"heic",
        b"heif", b"avif", b"jxl", b"apng", b"jpe", b"jfif", b"dib", b"cur", b"qoi", b"svgz", b"psd",
        b"psb", b"xcf", b"ai", b"eps", b"tga", b"pcx", b"pbm", b"pgm", b"ppm", b"pnm", b"pam",
        b"pfm", b"dds", b"exr", b"hdr", b"icns", b"sgi", b"ras", b"xbm", b"xpm", b"wbmp", b"jp2",
        b"cr2", b"cr3", b"crw", b"nef", b"nrw", b"arw", b"srf", b"sr2", b"dng", b"orf", b"rw2",
        b"raf", b"pef", b"srw", b"x3f", b"3fr", b"iiq", b"kdc", b"dcr", b"mrw", b"erf", b"rwl",
        b"mef",
    ]) {
        Kind::Image
    } else if any(&[
        b"mp3", b"flac", b"ogg", b"oga", b"wav", b"wave", b"m4a", b"m4b", b"aac", b"opus", b"wma",
        b"alac", b"ape", b"ac3", b"eac3", b"dts", b"thd", b"mka", b"wv", b"aif", b"aiff", b"aifc",
        b"caf", b"amr", b"au", b"mp2", b"mpc", b"tta",
    ]) {
        Kind::Audio
    } else if any(&[
        b"mp4", b"m4v", b"mkv", b"webm", b"avi", b"mov", b"qt", b"flv", b"f4v", b"wmv", b"asf",
        b"mpeg", b"mpg", b"mpe", b"m1v", b"m2v", b"vob", b"ts", b"m2ts", b"mts", b"m2t", b"ogv",
        b"3gp", b"3g2", b"mxf", b"divx", b"rm", b"rmvb", b"dv", b"y4m",
    ]) {
        Kind::Video
    } else if any(&[
        b"zip", b"7z", b"rar", b"tar", b"gz", b"tgz", b"xz", b"txz", b"bz2", b"tbz2", b"zst",
        b"tzst", b"deb", b"rpm", b"cab", b"lzh", b"arj",
    ]) {
        Kind::Archive
    } else if any(&[
        b"iso", b"img", b"vhd", b"vhdx", b"qcow2", b"qcow", b"vmdk", b"vdi", b"dmg",
    ]) {
        Kind::DiskImage
    } else if any(&[
        b"rs", b"c", b"h", b"cpp", b"py", b"js", b"ts", b"html", b"css", b"json", b"toml", b"yml",
        b"yaml", b"xml", b"java", b"go", b"cs", b"ps1",
    ]) {
        Kind::Code
    } else if any(&[b"pdf"]) {
        Kind::Pdf
    } else if any(&[
        b"txt", b"md", b"log", b"ini", b"cfg", b"conf", b"csv", b"doc", b"docx", b"odt", b"rtf",
    ]) {
        Kind::Text
    } else {
        Kind::File
    }
}

fn glyph_of(k: Kind) -> Glyph {
    match k {
        Kind::Folder => Glyph::Folder,
        Kind::Exe => Glyph::Exe,
        Kind::Image => Glyph::Image,
        Kind::Audio => Glyph::Audio,
        Kind::Video => Glyph::Video,
        Kind::Archive => Glyph::Archive,
        Kind::Code => Glyph::Code,
        Kind::Pdf => Glyph::Pdf,
        Kind::Text => Glyph::Text,
        Kind::File => Glyph::File,
        Kind::Drive => Glyph::Drive,
        Kind::LinuxDrive => Glyph::LinuxDrive,
        Kind::DiskImage => Glyph::DiskImage,
        Kind::Removable => Glyph::Removable,
        Kind::Network => Glyph::NetDrive,
    }
}

/// Nom du type pour la colonne « Type ».
fn type_name<'a>(e: &Entry, out: &'a mut [u8; 48]) -> &'a [u8] {
    let fixed: &[u8] = match e.kind {
        Kind::Folder => tr4(&[b"Dossier de fichiers", b"File folder", b"Carpeta de archivos", b"Dateiordner"]),
        Kind::Drive | Kind::LinuxDrive => tr4(&[b"Disque local", b"Local disk", b"Disco local", b"Lokaler Datentr\xE4ger"]),
        Kind::Removable => t4([
            b"Image disque mont\xE9e",
            b"Mounted disc image",
            b"Imagen de disco montada",
            b"Eingebundenes Disk-Image",
        ]),
        Kind::Network => t4([
            b"Lecteur r\xE9seau",
            b"Network drive",
            b"Unidad de red",
            b"Netzlaufwerk",
        ]),
        Kind::Exe if ends_ci(e.name(), b".exe") => tr4(&[b"Application Windows", b"Windows application", b"Aplicaci\xF3n de Windows", b"Windows-Anwendung"]),
        Kind::Exe => tr4(&[b"Programme", b"Program", b"Programa", b"Programm"]),
        _ => b"",
    };
    if !fixed.is_empty() {
        out[..fixed.len()].copy_from_slice(fixed);
        return &out[..fixed.len()];
    }
    let ext = extension(e.name());
    if ext.is_empty() {
        let file = tr4(&[b"Fichier", b"File", b"Archivo", b"Datei"]);
        out[..file.len()].copy_from_slice(file);
        return &out[..file.len()];
    }
    let prefix: &[u8] = match e.kind {
        Kind::Image => tr4(&[b"Image ", b"Image ", b"Imagen ", b"Bild "]),
        Kind::Audio => tr4(&[b"Audio ", b"Audio ", b"Audio ", b"Audio "]),
        Kind::Video => tr4(&[b"Vid\xE9o ", b"Video ", b"V\xEDdeo ", b"Video "]),
        Kind::Archive => tr4(&[b"Archive ", b"Archive ", b"Archivo comprimido ", b"Archiv "]),
        Kind::DiskImage => t4([b"Image disque ", b"Disc image ", b"Imagen ", b"Disk-Image "]),
        Kind::Pdf => tr4(&[b"Document ", b"Document ", b"Documento ", b"Dokument "]),
        _ => tr4(&[b"Fichier ", b"File ", b"Archivo ", b"Datei "]),
    };
    let mut n = prefix.len();
    out[..n].copy_from_slice(prefix);
    for &c in ext.iter().take(40 - n) {
        out[n] = if c.is_ascii_lowercase() { c - 32 } else { c };
        n += 1;
    }
    &out[..n]
}

/// Taille lisible : « 12,4 Ko », « 3,1 Mo »…
fn size_text(size: u64, out: &mut [u8; 24]) -> usize {
    let units: [&[u8]; 5] = [b" octets", b" Ko", b" Mo", b" Go", b" To"];
    let mut u = 0;
    let mut whole = size;
    let mut tenth = 0;
    while whole >= 1024 && u < 4 {
        tenth = (whole % 1024) * 10 / 1024;
        whole /= 1024;
        u += 1;
    }
    let mut n = write_num(out, 0, whole);
    if u > 0 && whole < 100 {
        out[n] = b',';
        out[n + 1] = b'0' + tenth as u8;
        n += 2;
    }
    for &c in units[u] {
        out[n] = c;
        n += 1;
    }
    n
}

fn write_num(out: &mut [u8], at: usize, mut v: u64) -> usize {
    let mut tmp = [0u8; 20];
    let mut k = 0;
    loop {
        tmp[k] = b'0' + (v % 10) as u8;
        k += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for i in 0..k {
        out[at + i] = tmp[k - 1 - i];
    }
    at + k
}

static mut TZ_OFFSET: i64 = i64::MIN;

/// FILETIME (UTC) → « JJ/MM/AAAA HH:MM » en heure locale.
fn date_text(ft: u64, out: &mut [u8; 24]) -> usize {
    if ft == 0 {
        return 0;
    }
    unsafe {
        if TZ_OFFSET == i64::MIN {
            let mut utc = 0u64;
            GetSystemTimeAsFileTime(&mut utc);
            let mut st = SystemTime::default();
            GetLocalTime(&mut st);
            let local_days = days_from_civil(st.year as i64, st.month as i64, st.day as i64);
            let local_secs = local_days * 86400
                + st.hour as i64 * 3600
                + st.minute as i64 * 60
                + st.second as i64;
            let utc_secs = (utc / 10_000_000) as i64 - 11_644_473_600;
            TZ_OFFSET = ((local_secs - utc_secs) as f64 / 900.0 + 0.5) as i64 * 900;
        }
        let secs = (ft / 10_000_000) as i64 - 11_644_473_600 + TZ_OFFSET;
        let (y, m, d) = civil_from_days(secs.div_euclid(86400));
        let rem = secs.rem_euclid(86400);
        let parts = [d as u64, m as u64];
        let mut n = 0;
        for p in parts {
            out[n] = b'0' + (p / 10) as u8;
            out[n + 1] = b'0' + (p % 10) as u8;
            out[n + 2] = b'/';
            n += 3;
        }
        n = write_num(out, n, y as u64);
        out[n] = b' ';
        let (h, mi) = (rem / 3600, rem / 60 % 60);
        out[n + 1] = b'0' + (h / 10) as u8;
        out[n + 2] = b'0' + (h % 10) as u8;
        out[n + 3] = b':';
        out[n + 4] = b'0' + (mi / 10) as u8;
        out[n + 5] = b'0' + (mi % 10) as u8;
        n + 6
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ---------- Chargement des dossiers ----------

unsafe fn tab() -> &'static mut Tab {
    &mut TABS[CUR]
}
unsafe fn pane() -> &'static mut Pane {
    let t = tab();
    &mut t.panes[t.focus]
}

unsafe fn add_entry(
    p: &mut Pane,
    name: &[u8],
    dir: bool,
    hidden: bool,
    size: u64,
    mtime: u64,
    kind: Option<Kind>,
) {
    if p.count >= MAX_ENTRIES || !valid_path(name) || name.is_empty() {
        p.truncated = true;
        return;
    }
    let e = &mut p.entries[p.count];
    let n = name.len();
    e.name[..n].copy_from_slice(&name[..n]);
    e.len = n as u16;
    e.dir = dir;
    e.hidden = hidden;
    e.size = size;
    e.mtime = mtime;
    e.kind = kind.unwrap_or_else(|| kind_of(name, dir));
    p.count += 1;
}

/// Lit un dossier : `prefix` est ajouté devant chaque nom (recherche profonde).
unsafe fn read_dir(p: &mut Pane, dir: &[u8], prefix: &[u8], filter: &[u8], depth: u32) {
    let mut wpattern = [0u16; PATH_LEN + NAME_LEN + 2];
    let n = wjoin(&mut wpattern, dir, b"*");
    if n == 0 {
        p.truncated = true;
        return;
    }
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(wpattern.as_ptr(), &mut data);
    if h as isize == -1 {
        if prefix.is_empty() {
            p.missing = !exists_dir(dir);
        }
        return;
    }
    let mut subdirs = [[0u8; NAME_LEN]; 48];
    let mut sublen = [0u16; 48];
    let mut nsub = 0;
    loop {
        let name = find_name_w(&data);
        if name != b"." && name != b".." && !valid_component(name) {
            p.truncated = true;
        } else if name != b"." && name != b".." && !name.is_empty() {
            let dir_flag = data.attributes & 0x10 != 0;
            let hidden = data.attributes & 0x2 != 0 || name[0] == b'.';
            let size = ((data.size_high as u64) << 32) | data.size_low as u64;
            if filter.is_empty() {
                add_entry(p, name, dir_flag, hidden, size, data.write_time, None);
            } else {
                if contains_ci(name, filter) && (SHOW_HIDDEN || !hidden) {
                    let mut full = [0u8; PATH_LEN + NAME_LEN + 2];
                    let m = if prefix.is_empty() && valid_path(name) {
                        name.len()
                    } else {
                        join(&mut full, prefix, name)
                    };
                    if prefix.is_empty() {
                        full[..m].copy_from_slice(&name[..m]);
                    }
                    if m != 0 {
                        add_entry(
                            p,
                            &full[..m],
                            dir_flag,
                            hidden,
                            size,
                            data.write_time,
                            Some(kind_of(name, dir_flag)),
                        );
                    } else {
                        p.truncated = true;
                    }
                }
                if dir_flag && !hidden && nsub < subdirs.len() {
                    let m = name.len();
                    if m >= NAME_LEN {
                        p.truncated = true;
                        if FindNextFileW(h, &mut data) == 0 {
                            break;
                        }
                        continue;
                    }
                    subdirs[nsub][..m].copy_from_slice(&name[..m]);
                    sublen[nsub] = m as u16;
                    nsub += 1;
                }
            }
        }
        if FindNextFileW(h, &mut data) == 0 || p.count >= MAX_ENTRIES {
            break;
        }
    }
    FindClose(h);
    if depth > 0 {
        for i in 0..nsub {
            if p.count >= MAX_ENTRIES {
                break;
            }
            let name = &subdirs[i][..sublen[i] as usize];
            let mut sub = [0u8; PATH_LEN + NAME_LEN + 2];
            let sn = join(&mut sub, dir, name);
            let mut pre = [0u8; PATH_LEN + NAME_LEN + 2];
            let pn = if prefix.is_empty() {
                pre[..name.len()].copy_from_slice(name);
                name.len()
            } else {
                join(&mut pre, prefix, name)
            };
            let (sub_s, pre_s) = (sub, pre);
            if sn != 0 && pn != 0 {
                read_dir(p, &sub_s[..sn], &pre_s[..pn], filter, depth - 1);
            } else {
                p.truncated = true;
            }
        }
    }
}

unsafe fn reload(p: &mut Pane) {
    p.count = 0;
    p.truncated = false;
    p.missing = false;
    p.deep = false;
    p.sel = [0; MAX_ENTRIES / 64];
    let loc = p.loc;
    if !loc.is(HOME) && !loc.is(PC) {
        maybe_commit(loc.get());
    }
    if loc.is(HOME) {
        for i in 0..PLACES.len() {
            let pp = place_path(i);
            add_entry(p, place_label(i), true, false, 0, 0, Some(Kind::Folder));
            let _ = pp;
        }
    } else if loc.is(PC) {
        add_entry(
            p,
            tr4(&[b"Dossier personnel (C:)", b"Home folder (C:)", b"Carpeta personal (C:)", b"Pers\xF6nlicher Ordner (C:)"]),
            true,
            false,
            0,
            0,
            Some(Kind::Drive),
        );
        add_entry(
            p,
            tr4(&[b"Syst\xE8me Linux (Z:)", b"Linux system (Z:)", b"Sistema Linux (Z:)", b"Linux-System (Z:)"]),
            true,
            false,
            0,
            0,
            Some(Kind::LinuxDrive),
        );
        refresh_drives();
        for i in 0..NDRIVES {
            let kind = if DRIVES[i].network {
                Kind::Network
            } else {
                Kind::Removable
            };
            add_entry(p, drive_label(i), true, false, 0, 0, Some(kind));
        }
    } else {
        read_dir(p, loc.get(), b"", b"", 0);
    }
    rebuild_view(p);
}

fn sort_less(a: &Entry, b: &Entry) -> bool {
    use core::cmp::Ordering::*;
    if a.dir != b.dir {
        return a.dir;
    }
    let by_name = cmp_ci(a.name(), b.name());
    let o = unsafe {
        match SORT {
            SortKey::Name => by_name,
            SortKey::Date => a.mtime.cmp(&b.mtime).then(by_name),
            SortKey::Size => a.size.cmp(&b.size).then(by_name),
            SortKey::Kind => cmp_ci(extension(a.name()), extension(b.name())).then(by_name),
        }
    };
    let o = if unsafe { SORT_DESC } { o.reverse() } else { o };
    o == Less
}

unsafe fn rebuild_view(p: &mut Pane) {
    let filter = p.search.get();
    p.nview = 0;
    for i in 0..p.count {
        let e = &p.entries[i];
        if e.hidden && !SHOW_HIDDEN {
            continue;
        }
        if !p.deep && !filter.is_empty() && !contains_ci(e.name(), filter) {
            continue;
        }
        p.view[p.nview] = i as u16;
        p.nview += 1;
    }
    // O(n log n), without allocation. Original indices preserve equal-key order.
    let entries = &p.entries;
    p.view[..p.nview].sort_unstable_by(|&a, &b| {
        if sort_less(&entries[a as usize], &entries[b as usize]) {
            core::cmp::Ordering::Less
        } else if sort_less(&entries[b as usize], &entries[a as usize]) {
            core::cmp::Ordering::Greater
        } else {
            a.cmp(&b)
        }
    });
    p.scroll = p.scroll.min(max_scroll_hint(p)).max(0);
}

fn max_scroll_hint(_p: &Pane) -> i32 {
    i32::MAX
}

unsafe fn navigate_pane(p: &mut Pane, target: &[u8], record: bool) {
    if !valid_path(target) || target.is_empty() {
        notify(tr4(&[b"Chemin trop long : aucun nom n'a \xE9t\xE9 tronqu\xE9.", b"Path too long: no name was truncated.", b"Ruta demasiado larga: no se ha truncado ning\xFAn nombre.", b"Pfad zu lang: Kein Name wurde gek\xFCrzt."]));
        return;
    }
    if record && !p.loc.get().is_empty() && !p.loc.is(target) {
        if p.nback == HISTORY {
            for i in 1..HISTORY {
                p.back[i - 1] = p.back[i];
            }
            p.nback -= 1;
        }
        p.back[p.nback] = p.loc;
        p.nback += 1;
        p.nfwd = 0;
    }
    p.loc.set(target);
    p.search.len = 0;
    p.scroll = 0;
    p.scroll_target = 0;
    p.scroll_x = 0;
    reload(p);
}

unsafe fn navigate(target: &[u8]) {
    FOCUS = Focus::List;
    navigate_pane(pane(), target, true);
}

unsafe fn go_back() {
    let p = pane();
    if p.nback == 0 {
        return;
    }
    p.nback -= 1;
    if p.nfwd < HISTORY {
        p.fwd[p.nfwd] = p.loc;
        p.nfwd += 1;
    }
    let target = p.back[p.nback];
    navigate_pane(p, target.get(), false);
}
unsafe fn go_forward() {
    let p = pane();
    if p.nfwd == 0 {
        return;
    }
    p.nfwd -= 1;
    if p.nback < HISTORY {
        p.back[p.nback] = p.loc;
        p.nback += 1;
    }
    let target = p.fwd[p.nfwd];
    navigate_pane(p, target.get(), false);
}
unsafe fn go_up() {
    let loc = pane().loc;
    if loc.is(HOME) || loc.is(PC) {
        return;
    }
    match parent(loc.get()) {
        Some(up) => {
            let mut t = EMPTY_PATH;
            t.set(up);
            navigate(t.get());
        }
        None => navigate(PC),
    }
}

/// Chemin Windows complet de l'élément `index` (indice dans `entries`).
unsafe fn item_path(p: &Pane, index: usize, out: &mut [u8; PATH_LEN + NAME_LEN + 2]) -> usize {
    let e = &p.entries[index];
    if p.loc.is(HOME) {
        let k = PLACES
            .iter()
            .position(|pl| pl.name == e.name())
            .unwrap_or(0);
        let pp = place_path(k);
        out[..pp.len as usize].copy_from_slice(pp.get());
        out[pp.len as usize] = 0;
        return pp.len as usize;
    }
    if p.loc.is(PC) {
        if let (Kind::Removable | Kind::Network, Some(l)) = (e.kind, drive_letter_in(e.name())) {
            out[..3].copy_from_slice(&[l, b':', b'\\']);
            out[3] = 0;
            return 3;
        }
        let s: &[u8] = if e.kind == Kind::LinuxDrive {
            b"Z:\\"
        } else {
            b"C:\\"
        };
        out[..3].copy_from_slice(s);
        out[3] = 0;
        return 3;
    }
    join(out, p.loc.get(), e.name())
}

unsafe fn open_item(p: &mut Pane, index: usize, new_tab: bool) {
    let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
    let n = item_path(p, index, &mut path);
    if n == 0 {
        notify(tr4(&[b"Chemin trop long pour cette version d'Explorer.", b"Path too long for this version of Explorer.", b"Ruta demasiado larga para esta versi\xF3n del Explorador.", b"Pfad zu lang f\xFCr diese Version des Explorers."]));
        return;
    }
    if p.entries[index].kind == Kind::Network {
        if let Some(d) = find_drive(path[0]).filter(|d| !d.online) {
            // Lecteur réseau hors ligne : on demande la reconnexion.
            let mut args = [0u16; 16];
            for (i, &c) in b"connect X".iter().enumerate() {
                args[i] = if c == b'X' { d.letter as u16 } else { c as u16 };
            }
            run_drives_tool(&args);
            return;
        }
    }
    if !p.entries[index].dir && open_storage_item(&path[..n], p.entries[index].kind) {
        return;
    }
    if p.entries[index].dir {
        if new_tab {
            open_tab(&path[..n]);
        } else {
            navigate(&path[..n]);
        }
        return;
    }
    // Open text files in integrated editor
    if ends_ci(&path[..n], b".txt")
        || ends_ci(&path[..n], b".rs")
        || ends_ci(&path[..n], b".c")
        || ends_ci(&path[..n], b".h")
        || ends_ci(&path[..n], b".py")
        || ends_ci(&path[..n], b".js")
        || ends_ci(&path[..n], b".sh")
        || ends_ci(&path[..n], b".toml")
        || ends_ci(&path[..n], b".json")
        || ends_ci(&path[..n], b".md")
        || ends_ci(&path[..n], b".cfg")
        || ends_ci(&path[..n], b".conf")
        || ends_ci(&path[..n], b".log")
    {
        crate::notepad::open_new();
        use winlux_testexe::*;
        let mut wide = [0u16; PATH_LEN + NAME_LEN + 2];
        wstr(&mut wide, &path[..n]);
        let h = CreateFileW(
            wide.as_ptr(),
            0x80000000,
            1,
            core::ptr::null_mut(),
            3,
            0,
            core::ptr::null_mut(),
        );
        if !h.is_null() && (h as isize) != -1 {
            let mut buf = [0u8; 32768];
            let mut got = 0u32;
            ReadFile(
                h,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut got,
                core::ptr::null_mut(),
            );
            CloseHandle(h);
            // Extract filename from path
            let mut fname_start = 0;
            let mut k = 0;
            while k < n {
                if path[k] == b'\\' || path[k] == b'/' {
                    fname_start = k + 1;
                }
                k += 1;
            }
            unsafe {
                crate::notepad::load_into_active(&buf[..got as usize], &path[fname_start..n]);
            }
        }
        wm::open(wm::APP_NOTEPAD);
        return;
    }
    // Audio et vidéo : lecteur multimédia intégré (repli : association Linux).
    if matches!(p.entries[index].kind, Kind::Audio | Kind::Video) && open_media(&path[..n]) {
        return;
    }
    let mut wpath = [0u16; PATH_LEN + NAME_LEN + 2];
    wstr(&mut wpath, &path[..n]);
    let wopen: [u16; 5] = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
    let r = ShellExecuteW(
        core::ptr::null_mut(),
        wopen.as_ptr(),
        wpath.as_ptr(),
        core::ptr::null(),
        core::ptr::null(),
        1,
    );
    if (r as isize) <= 32 {
        notify(tr4(&[b"Aucune application ne sait ouvrir ce fichier.", b"No app can open this file.", b"Ninguna aplicaci\xF3n puede abrir este archivo.", b"Keine App kann diese Datei \xF6ffnen."]));
    }
}

/// Ouvre un fichier audio/vidéo dans le lecteur multimédia de Winlux.
unsafe fn open_media(path: &[u8]) -> bool {
    let mut params = [0u16; PATH_LEN + NAME_LEN + 4];
    if path.len() + 3 > params.len() || !valid_path(path) {
        return false;
    }
    params[0] = b'"' as u16;
    for (i, &c) in path.iter().enumerate() {
        params[i + 1] = c as u16;
    }
    params[path.len() + 1] = b'"' as u16;
    let mut program = [0u16; 24];
    for (i, &c) in b"winlux-mediaplayer".iter().enumerate() {
        program[i] = c as u16;
    }
    let wopen: [u16; 5] = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
    let r = ShellExecuteW(
        core::ptr::null_mut(),
        wopen.as_ptr(),
        program.as_ptr(),
        params.as_ptr(),
        core::ptr::null(),
        1,
    );
    (r as isize) > 32
}

unsafe fn notify(msg: &[u8]) {
    MESSAGE.set(msg);
    MESSAGE_UNTIL = GetTickCount().wrapping_add(4000);
    if !wm::is_visible(wm::APP_EXPLORER) {
        crate::desktop::toast(msg);
    }
}

// ---------- Onglets ----------

unsafe fn open_tab(path: &[u8]) {
    if NTABS == MAX_TABS {
        notify(tr4(&[b"Nombre maximal d'onglets atteint.", b"Maximum number of tabs reached.", b"Se alcanz\xF3 el n\xFAmero m\xE1ximo de pesta\xF1as.", b"Maximale Anzahl an Registerkarten erreicht."]));
        return;
    }
    let t = &mut TABS[NTABS];
    t.dual = false;
    t.focus = 0;
    for p in t.panes.iter_mut() {
        p.nback = 0;
        p.nfwd = 0;
        p.loc.len = 0;
    }
    navigate_pane(&mut t.panes[0], path, false);
    CUR = NTABS;
    NTABS += 1;
    FOCUS = Focus::List;
}

unsafe fn close_tab(i: usize) {
    if NTABS <= 1 {
        wm::close(wm::APP_EXPLORER);
        NTABS = 0;
        return;
    }
    for k in i..NTABS - 1 {
        core::ptr::swap(&mut TABS[k] as *mut Tab, &mut TABS[k + 1] as *mut Tab);
    }
    NTABS -= 1;
    if CUR >= NTABS {
        CUR = NTABS - 1;
    } else if CUR > i {
        CUR -= 1;
    }
}

unsafe fn toggle_dual() {
    let t = tab();
    t.dual = !t.dual;
    if t.dual && t.panes[1].loc.len == 0 {
        let loc = t.panes[0].loc;
        navigate_pane(&mut t.panes[1], loc.get(), false);
    }
    if !t.dual {
        t.focus = 0;
    }
}

/// Ouvre Explorer sur un dossier (appelé par le bureau et le menu Démarrer).
pub unsafe fn open_at(path: &[u8]) {
    if wm::native_explorer(path) {
        return;
    }
    if !wm::is_open(wm::APP_EXPLORER) || NTABS == 0 {
        load_folder_options();
        NTABS = 0;
        CUR = 0;
        open_tab(path);
    } else if !pane().loc.is(path) {
        open_tab(path);
    }
    wm::open(wm::APP_EXPLORER);
}
pub unsafe fn open_home() {
    open_at(HOME);
}
/// Prépare le premier onglet pendant l'écran de démarrage.
pub unsafe fn preload() {
    if NTABS == 0 {
        load_folder_options();
        open_tab(HOME);
    }
    for i in 0..PLACES.len() {
        let _ = place_path(i);
    }
}
pub unsafe fn open_pc() {
    open_at(PC);
}
pub unsafe fn open_documents() {
    open_at(place_path(2).get());
}
pub unsafe fn open_downloads() {
    open_at(place_path(1).get());
}
pub unsafe fn open_trash() {
    open_at(TRASH);
}

pub unsafe fn current_view() -> u8 {
    match VIEW {
        View::XLarge => 0,
        View::Large => 1,
        View::Medium => 2,
        View::Small => 3,
        View::List => 4,
        View::Details => 5,
        View::Tiles => 6,
    }
}
pub unsafe fn set_view(v: u8) {
    VIEW = match v {
        0 => View::XLarge,
        1 => View::Large,
        2 => View::Medium,
        3 => View::Small,
        4 => View::List,
        5 => View::Details,
        _ => View::Tiles,
    };
}
pub unsafe fn show_hidden() -> bool {
    SHOW_HIDDEN
}
pub unsafe fn set_show_hidden(v: bool) {
    SHOW_HIDDEN = v;
}
pub unsafe fn show_ext() -> bool {
    SHOW_EXT
}
pub unsafe fn set_show_ext(v: bool) {
    SHOW_EXT = v;
}

// ---------- Options des dossiers (registre, comme Windows) ----------
//
// Paramètres > Gestionnaire de fichiers écrit ces valeurs ; les programmes
// Windows lisent les mêmes (Hidden 1 = afficher, 2 = masquer ; HideFileExt).

const KEY_ADVANCED: &core::ffi::CStr =
    c"Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
const KEY_WINLUX: &core::ffi::CStr = c"Software\\Winlux\\Explorer";
const HKEY_CURRENT_USER: usize = 0x8000_0001;

unsafe fn reg_dword(key: &core::ffi::CStr, name: &core::ffi::CStr) -> Option<u32> {
    let mut h: Handle = core::ptr::null_mut();
    if RegOpenKeyExA(HKEY_CURRENT_USER as Handle, key.as_ptr(), 0, 0x2_0019, &mut h) != 0 {
        return None;
    }
    let mut value = 0u32;
    let mut kind = 0u32;
    let mut size = 4u32;
    let r = RegQueryValueExA(
        h,
        name.as_ptr(),
        core::ptr::null_mut(),
        &mut kind,
        (&mut value as *mut u32).cast(),
        &mut size,
    );
    RegCloseKey(h);
    (r == 0 && kind == 4 && size == 4).then_some(value)
}

unsafe fn reg_set_dword(key: &core::ffi::CStr, name: &core::ffi::CStr, value: u32) {
    let mut h: Handle = core::ptr::null_mut();
    if RegCreateKeyExA(
        HKEY_CURRENT_USER as Handle,
        key.as_ptr(),
        0,
        core::ptr::null(),
        0,
        0x2_0006,
        core::ptr::null_mut(),
        &mut h,
        core::ptr::null_mut(),
    ) != 0
    {
        return;
    }
    RegSetValueExA(h, name.as_ptr(), 0, 4, (&value as *const u32).cast(), 4);
    RegCloseKey(h);
}

/// Relu à chaque nouvelle fenêtre : les Paramètres ont pu changer entre-temps.
unsafe fn load_folder_options() {
    if let Some(v) = reg_dword(KEY_ADVANCED, c"Hidden") {
        SHOW_HIDDEN = v == 1;
    }
    if let Some(v) = reg_dword(KEY_ADVANCED, c"HideFileExt") {
        SHOW_EXT = v == 0;
    }
    if let Some(v) = reg_dword(KEY_WINLUX, c"DefaultView") {
        if v <= 6 {
            set_view(v as u8);
        }
    }
}

unsafe fn save_folder_options() {
    reg_set_dword(KEY_ADVANCED, c"Hidden", if SHOW_HIDDEN { 1 } else { 2 });
    reg_set_dword(KEY_ADVANCED, c"HideFileExt", u32::from(!SHOW_EXT));
}

// ---------- Sélection ----------

fn is_sel(p: &Pane, i: usize) -> bool {
    p.sel[i / 64] & (1 << (i % 64)) != 0
}
fn set_sel(p: &mut Pane, i: usize, on: bool) {
    if on {
        p.sel[i / 64] |= 1 << (i % 64);
    } else {
        p.sel[i / 64] &= !(1 << (i % 64));
    }
}
fn sel_count(p: &Pane) -> usize {
    p.sel.iter().map(|w| w.count_ones() as usize).sum()
}
fn first_sel(p: &Pane) -> Option<usize> {
    (0..p.nview)
        .find(|&v| is_sel(p, p.view[v] as usize))
        .map(|v| p.view[v] as usize)
}
fn ctrl() -> bool {
    unsafe { crate::gfx::key_state(0x11) as u16 & 0x8000 != 0 }
}
fn shift() -> bool {
    unsafe { crate::gfx::key_state(0x10) as u16 & 0x8000 != 0 }
}

// ---------- Géométrie ----------

#[derive(Clone, Copy)]
struct Geo {
    toolbar: Rect,
    command: Rect,
    nav: Rect,
    body: Rect,
    status: Rect,
    panes: [Rect; 2],
}

unsafe fn geo(f: &Frame) -> Geo {
    let c = f.client;
    let toolbar = rect(c.left, c.top, c.right, c.top + TOOLBAR_H);
    let command = rect(c.left, toolbar.bottom, c.right, toolbar.bottom + COMMAND_H);
    let status = rect(c.left, c.bottom - STATUS_H, c.right, c.bottom);
    let nav_w = if SHOW_NAV && c.right - c.left > 700 {
        NAV_W
    } else {
        0
    };
    let nav = rect(c.left, command.bottom, c.left + nav_w, status.top);
    let body = rect(nav.right, command.bottom, c.right, status.top);
    let mut panes = [body, body];
    if tab().dual {
        let mid = (body.left + body.right) / 2;
        panes[0].right = mid - 1;
        panes[1].left = mid + 1;
    }
    Geo {
        toolbar,
        command,
        nav,
        body,
        status,
        panes,
    }
}

#[derive(Clone, Copy)]
struct Cells {
    cw: i32,
    ch: i32,
    cols: i32,
    icon: i32,
    header: i32,
}

unsafe fn cells(r: &Rect) -> Cells {
    let w = r.right - r.left - SCROLL_W - 16;
    let compact = COMPACT;
    let (cw, ch, icon) = match VIEW {
        View::XLarge => (156, 176, 96),
        View::Large => (120, 132, 64),
        View::Medium => (96, 100, 40),
        View::Small => (220, if compact { 24 } else { 30 }, 16),
        View::List => (260, if compact { 24 } else { 30 }, 16),
        View::Details => (w, if compact { 26 } else { 34 }, 16),
        View::Tiles => (280, if compact { 60 } else { 72 }, 44),
    };
    let (cw, ch) = if compact && matches!(VIEW, View::XLarge | View::Large | View::Medium) {
        (cw - 12, ch - 14)
    } else {
        (cw, ch)
    };
    let cols = if VIEW == View::Details {
        1
    } else {
        (w / cw).max(1)
    };
    let cw = if VIEW == View::Details { cw } else { w / cols };
    Cells {
        cw,
        ch,
        cols,
        icon,
        header: if VIEW == View::Details { 32 } else { 0 },
    }
}

unsafe fn content_height(p: &Pane, c: &Cells) -> i32 {
    ((p.nview as i32 + c.cols - 1) / c.cols) * c.ch + c.header + 16
}

unsafe fn clamp_scroll(p: &mut Pane, r: &Rect) {
    let c = cells(r);
    let max = (content_height(p, &c) - (r.bottom - r.top)).max(0);
    p.scroll = p.scroll.clamp(0, max);
    p.scroll_target = p.scroll_target.clamp(0, max);
}

unsafe fn item_rect(r: &Rect, c: &Cells, v: usize, scroll: i32) -> Rect {
    let (col, row) = (v as i32 % c.cols, v as i32 / c.cols);
    let x = r.left + 8 + col * c.cw;
    let y = r.top + 8 + c.header + row * c.ch - scroll;
    rect(
        x,
        y,
        x + c.cw - if VIEW == View::Details { 0 } else { 4 },
        y + c.ch - if VIEW == View::Details { 0 } else { 4 },
    )
}

unsafe fn item_at(p: &Pane, r: &Rect, x: i32, y: i32) -> Option<usize> {
    if !inside(r, x, y) || y < r.top + 8 + cells(r).header {
        return None;
    }
    let c = cells(r);
    let col = (x - r.left - 8) / c.cw;
    let row = (y - r.top - 8 - c.header + p.scroll) / c.ch;
    if col < 0 || col >= c.cols || row < 0 {
        return None;
    }
    let v = (row * c.cols + col) as usize;
    if v >= p.nview {
        return None;
    }
    let ir = item_rect(r, &c, v, p.scroll);
    if !inside(&ir, x, y) {
        return None;
    }
    if VIEW == View::Details && x > ir.left + detail_cols(&ir).1 {
        return None;
    }
    Some(v)
}

/// Colonnes de la vue Détails : (x du nom, largeur du nom, x date, x type, x taille).
fn detail_cols(r: &Rect) -> (i32, i32, i32, i32, i32) {
    let w = r.right - r.left;
    let name_w = (w * 45 / 100).max(200);
    let date_x = r.left + name_w + 12;
    let type_x = date_x + 150;
    let size_x = type_x + (w - name_w - 150 - 120).max(120);
    (
        r.left + 36,
        name_w,
        date_x,
        type_x,
        size_x.min(r.right - 110),
    )
}

// ---------- Dessin ----------

unsafe fn draw_tabs(g: &mut Gfx, f: &Frame) {
    let p = pal();
    let t = f.title;
    let mut x = t.left + 8;
    let w = ((t.right - t.left - 60) / NTABS.max(1) as i32)
        .min(TAB_W)
        .max(90);
    for i in 0..NTABS {
        let r = rect(x, t.top + 6, x + w, t.bottom);
        let active = i == CUR;
        let hover = !active && inside(&r, wm::MOUSE_X, wm::MOUSE_Y);
        if active {
            round_rect(
                g.hdc,
                g.b,
                r.left,
                r.top,
                r.right,
                r.bottom + 6,
                7,
                p.window,
            );
        } else if hover {
            round_rect(
                g.hdc,
                g.b,
                r.left + 2,
                r.top + 2,
                r.right - 2,
                r.bottom - 4,
                5,
                p.hover,
            );
        } else if i + 1 != CUR && i + 1 < NTABS {
            fill(
                g.hdc,
                g.b,
                r.right - 1,
                r.top + 10,
                r.right,
                r.bottom - 10,
                p.border,
            );
        }
        let loc = TABS[i].panes[TABS[i].focus].loc;
        let name = last_component(loc.get());
        let gl = if loc.is(HOME) {
            Glyph::Home
        } else if loc.is(PC) {
            Glyph::Computer
        } else {
            Glyph::Folder
        };
        glyph(g.hdc, g.b, g.f, gl, r.left + 12, r.top + 9, 16);
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            if active { p.text } else { p.text2 },
            r.left + 36,
            r.top + 10,
            name,
            w - 70,
        );
        let close = rect(r.right - 28, r.top + 6, r.right - 8, r.top + 26);
        if active || hover {
            if inside(&close, wm::MOUSE_X, wm::MOUSE_Y) {
                round_rect(
                    g.hdc,
                    g.b,
                    close.left,
                    close.top,
                    close.right,
                    close.bottom,
                    4,
                    p.hover,
                );
            }
            set_glyph_bg(Some(if inside(&close, wm::MOUSE_X, wm::MOUSE_Y) {
                p.hover
            } else if active {
                p.window
            } else {
                p.hover
            }));
            glyph(
                g.hdc,
                g.b,
                g.f,
                Glyph::Close,
                close.left + 5,
                close.top + 5,
                10,
            );
            set_glyph_bg(None);
        }
        x += w;
    }
    let plus = rect(x + 4, t.top + 8, x + 36, t.bottom - 4);
    if inside(&plus, wm::MOUSE_X, wm::MOUSE_Y) {
        round_rect(
            g.hdc,
            g.b,
            plus.left,
            plus.top,
            plus.right,
            plus.bottom,
            4,
            p.hover,
        );
    }
    glyph(
        g.hdc,
        g.b,
        g.f,
        Glyph::Plus,
        plus.left + 9,
        plus.top + 7,
        14,
    );
}

fn hovered(r: &Rect) -> bool {
    unsafe { inside(r, wm::MOUSE_X, wm::MOUSE_Y) }
}

unsafe fn icon_button(g: &mut Gfx, r: &Rect, gl: Glyph, enabled: bool, on: bool) {
    let p = pal();
    let bg = if on {
        p.selected
    } else if enabled && hovered(r) {
        p.hover
    } else {
        p.window
    };
    if bg != p.window {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, bg);
    }
    set_glyph_bg(Some(bg));
    if !enabled {
        set_glyph_fg(Some(p.text3));
    }
    let s = 16;
    glyph(
        g.hdc,
        g.b,
        g.f,
        gl,
        (r.left + r.right - s) / 2,
        (r.top + r.bottom - s) / 2,
        s,
    );
    set_glyph_fg(None);
    set_glyph_bg(None);
}

/// Boutons de la barre d'outils, dans l'ordre : précédent, suivant, haut, actualiser.
fn nav_buttons(g: &Geo) -> [Rect; 4] {
    let y = g.toolbar.top + 8;
    let mut out = [rect(0, 0, 0, 0); 4];
    for i in 0..4 {
        let x = g.toolbar.left + 10 + i as i32 * 38;
        out[i] = rect(x, y, x + 34, y + 32);
    }
    out
}
fn address_rect(g: &Geo) -> Rect {
    let search_w = 280.min((g.toolbar.right - g.toolbar.left) / 4);
    rect(
        g.toolbar.left + 168,
        g.toolbar.top + 8,
        g.toolbar.right - search_w - 22,
        g.toolbar.top + 40,
    )
}
fn search_rect(g: &Geo) -> Rect {
    let a = address_rect(g);
    rect(a.right + 10, a.top, g.toolbar.right - 12, a.bottom)
}

#[derive(Clone, Copy, PartialEq)]
enum Cmd {
    New,
    Cut,
    Copy,
    Paste,
    Rename,
    Delete,
    Sort,
    View,
    Dual,
    Nav,
    Terminal,
}
const COMMANDS: [(Cmd, Glyph, Tr4, i32); 11] = [
    (Cmd::New, Glyph::Plus, [b"Nouveau", b"New", b"Nuevo", b"Neu"], 112),
    (Cmd::Cut, Glyph::Cut, [b"", b"", b"", b""], 38),
    (Cmd::Copy, Glyph::Copy, [b"", b"", b"", b""], 38),
    (Cmd::Paste, Glyph::Paste, [b"", b"", b"", b""], 38),
    (Cmd::Rename, Glyph::Rename, [b"", b"", b"", b""], 38),
    (Cmd::Delete, Glyph::Delete, [b"", b"", b"", b""], 38),
    (Cmd::Sort, Glyph::Sort, [b"Trier", b"Sort", b"Ordenar", b"Sortieren"], 84),
    (Cmd::View, Glyph::View, [b"Afficher", b"View", b"Ver", b"Anzeigen"], 104),
    (Cmd::Dual, Glyph::Split, [b"Double volet", b"Dual pane", b"Doble panel", b"Doppelansicht"], 130),
    (Cmd::Nav, Glyph::Details, [b"Volet", b"Pane", b"Panel", b"Bereich"], 76),
    (Cmd::Terminal, Glyph::Terminal, [b"Terminal", b"Terminal", b"Terminal", b"Terminal"], 102),
];

fn command_rects(g: &Geo) -> [Rect; 11] {
    let mut out = [rect(0, 0, 0, 0); 11];
    let mut x = g.command.left + 10;
    for (i, c) in COMMANDS.iter().enumerate() {
        if i == 1 || i == 6 {
            x += 12;
        }
        out[i] = rect(x, g.command.top + 7, x + c.3, g.command.top + 39);
        x += c.3 + 4;
    }
    out
}

unsafe fn draw_toolbar(g: &mut Gfx, geo: &Geo) {
    let p = pal();
    fill(
        g.hdc,
        g.b,
        geo.toolbar.left,
        geo.toolbar.top,
        geo.toolbar.right,
        geo.command.bottom,
        p.window,
    );
    let pn = pane();
    let nb = nav_buttons(geo);
    icon_button(g, &nb[0], Glyph::Back, pn.nback > 0, false);
    icon_button(g, &nb[1], Glyph::Forward, pn.nfwd > 0, false);
    icon_button(
        g,
        &nb[2],
        Glyph::Up,
        !pn.loc.is(HOME) && !pn.loc.is(PC),
        false,
    );
    icon_button(g, &nb[3], Glyph::Refresh, true, false);

    // Barre d'adresse : fil d'Ariane, ou texte modifiable après un clic.
    let a = address_rect(geo);
    let editing = FOCUS == Focus::Address;
    round_rect(
        g.hdc,
        g.b,
        a.left,
        a.top,
        a.right,
        a.bottom,
        5,
        if editing { p.accent } else { p.border },
    );
    round_rect(
        g.hdc,
        g.b,
        a.left + 1,
        a.top + 1,
        a.right - 1,
        a.bottom - (if editing { 2 } else { 1 }),
        4,
        if hovered(&a) && !editing {
            p.hover
        } else {
            p.surface
        },
    );
    if editing {
        if ADDRESS_SELECTED {
            let w = text_width(ADDRESS.get(), 13).min(a.right - a.left - 24);
            fill(
                g.hdc,
                g.b,
                a.left + 10,
                a.top + 5,
                a.left + 14 + w,
                a.bottom - 5,
                p.selected,
            );
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            a.left + 12,
            a.top + 8,
            ADDRESS.get(),
            a.right - a.left - 24,
        );
        let cx = a.left + 12 + text_width(ADDRESS.get(), 13).min(a.right - a.left - 30);
        if GetTickCount() / 530 % 2 == 0 {
            fill(g.hdc, g.b, cx, a.top + 7, cx + crate::gfx::caret_w(1), a.bottom - 7, p.text);
        }
    } else {
        draw_breadcrumb(g, &a, pn.loc.get());
    }

    let s = search_rect(geo);
    let active = FOCUS == Focus::Search;
    round_rect(
        g.hdc,
        g.b,
        s.left,
        s.top,
        s.right,
        s.bottom,
        5,
        if active { p.accent } else { p.border },
    );
    round_rect(
        g.hdc,
        g.b,
        s.left + 1,
        s.top + 1,
        s.right - 1,
        s.bottom - (if active { 2 } else { 1 }),
        4,
        p.surface,
    );
    set_glyph_bg(Some(p.surface));
    glyph(g.hdc, g.b, g.f, Glyph::Search, s.right - 28, s.top + 9, 14);
    set_glyph_bg(None);
    if pn.search.len == 0 {
        let mut label = [0u8; 80];
        let pre = tr4(&[b"Rechercher dans ", b"Search ", b"Buscar en ", b"Suchen in "]);
        label[..pre.len()].copy_from_slice(pre);
        let name = last_component(pn.loc.get());
        let n = name.len().min(60);
        label[pre.len()..pre.len() + n].copy_from_slice(&name[..n]);
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text3,
            s.left + 12,
            s.top + 8,
            &label[..pre.len() + n],
            s.right - s.left - 48,
        );
    } else {
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            s.left + 12,
            s.top + 8,
            pn.search.get(),
            s.right - s.left - 48,
        );
    }
    if active && GetTickCount() / 530 % 2 == 0 {
        let cx = s.left + 12 + text_width(pn.search.get(), 13).min(s.right - s.left - 52);
        fill(g.hdc, g.b, cx, s.top + 7, cx + crate::gfx::caret_w(1), s.bottom - 7, p.text);
    }

    // Barre de commandes.
    fill(
        g.hdc,
        g.b,
        geo.command.left,
        geo.command.top,
        geo.command.right,
        geo.command.top + 1,
        p.border,
    );
    let has_sel = sel_count(pn) > 0 && !pn.loc.is(HOME) && !pn.loc.is(PC);
    let rects = command_rects(geo);
    for (i, (cmd, gl, label, _)) in COMMANDS.iter().enumerate() {
        let label = tr4(label);
        let r = rects[i];
        if r.right > geo.command.right - 8 {
            break;
        }
        let enabled = match cmd {
            Cmd::Cut | Cmd::Copy | Cmd::Rename | Cmd::Delete => has_sel,
            Cmd::Paste => NCLIP > 0 && !pn.loc.is(HOME) && !pn.loc.is(PC),
            Cmd::New => !pn.loc.is(HOME) && !pn.loc.is(PC),
            _ => true,
        };
        let on = match cmd {
            Cmd::Dual => tab().dual,
            Cmd::Nav => SHOW_NAV,
            Cmd::View => MENU == Menu::View,
            Cmd::Sort => MENU == Menu::Sort,
            _ => false,
        };
        if label.is_empty() {
            icon_button(g, &r, *gl, enabled, on);
            continue;
        }
        let bg = if on {
            p.selected
        } else if hovered(&r) && enabled {
            p.hover
        } else {
            p.window
        };
        if bg != p.window {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, bg);
        }
        set_glyph_bg(Some(bg));
        glyph(g.hdc, g.b, g.f, *gl, r.left + 10, r.top + 8, 16);
        set_glyph_bg(None);
        text(
            g.hdc,
            g.f.caption,
            if enabled { p.text } else { p.text3 },
            r.left + 34,
            r.top + 9,
            label,
        );
        if matches!(cmd, Cmd::Sort | Cmd::View | Cmd::New) && r.right - r.left > 80 {
            set_glyph_bg(Some(bg));
            glyph(
                g.hdc,
                g.b,
                g.f,
                Glyph::ChevronDown,
                r.right - 18,
                r.top + 12,
                9,
            );
            set_glyph_bg(None);
        }
    }
    fill(
        g.hdc,
        g.b,
        geo.command.left,
        geo.command.bottom - 1,
        geo.command.right,
        geo.command.bottom,
        p.border,
    );
}

unsafe fn draw_breadcrumb(g: &mut Gfx, a: &Rect, loc: &[u8]) {
    let p = pal();
    let mut x = a.left + 10;
    let gl = if loc == HOME {
        Glyph::Home
    } else if loc == PC || loc.len() <= 3 {
        Glyph::Computer
    } else {
        Glyph::Folder
    };
    set_glyph_bg(Some(if hovered(a) { p.hover } else { p.surface }));
    glyph(g.hdc, g.b, g.f, gl, x, a.top + 8, 16);
    x += 26;
    // Segments : Ce PC > lecteur > dossiers…
    let mut segs: [(&[u8], usize); 24] = [(b"", 0); 24];
    let mut n;
    if loc == HOME {
        segs[0] = (tr(S_HOME), 0);
        n = 1;
    } else if loc == PC {
        segs[0] = (tr(S_THIS_PC), 0);
        n = 1;
    } else {
        segs[0] = (tr(S_THIS_PC), 0);
        n = 1;
        segs[1] = (last_component(&loc[..3.min(loc.len())]), 3);
        n += 1;
        let mut start = 3;
        for i in 3..=loc.len() {
            if i == loc.len() || loc[i] == b'\\' {
                if i > start && n < segs.len() {
                    segs[n] = (&loc[start..i], i);
                    n += 1;
                }
                start = i + 1;
            }
        }
    }
    // Si tout ne tient pas, on garde les derniers segments.
    let avail = a.right - x - 10;
    let mut first = 0;
    loop {
        let w: i32 = segs[first..n]
            .iter()
            .map(|s| text_width(s.0, 13) + 22)
            .sum();
        if w <= avail || first + 1 >= n {
            break;
        }
        first += 1;
    }
    for i in first..n {
        if i > first {
            glyph(g.hdc, g.b, g.f, Glyph::Chevron, x - 2, a.top + 11, 10);
            x += 12;
        }
        let w = text_width(segs[i].0, 13).min(a.right - x - 12);
        if w <= 0 {
            break;
        }
        text_fit(
            g.hdc,
            g.f.body,
            13,
            if i + 1 == n { p.text } else { p.text2 },
            x,
            a.top + 8,
            segs[i].0,
            w + 4,
        );
        x += w + 10;
    }
    set_glyph_bg(None);
}

/// Entrées du volet de navigation : (libellé, icône, chemin, retrait).
const NAV_MAX: usize = 11 + MAX_DRIVES;

unsafe fn nav_items(out: &mut [(&'static [u8], Glyph, Path, i32); NAV_MAX]) -> usize {
    let mut n = 0;
    let mut push = |label: &'static [u8], gl: Glyph, path: &[u8], indent: i32| {
        let mut p = EMPTY_PATH;
        p.set(path);
        out[n] = (label, gl, p, indent);
        n += 1;
    };
    push(tr(S_HOME), Glyph::Home, HOME, 0);
    for i in 0..PLACES.len() {
        let pp = place_path(i);
        push(place_label(i), PLACES[i].glyph, pp.get(), 0);
    }
    push(tr(S_THIS_PC), Glyph::Computer, PC, 0);
    push(tr4(&[b"Dossier personnel (C:)", b"Home folder (C:)", b"Carpeta personal (C:)", b"Pers\xF6nlicher Ordner (C:)"]), Glyph::Drive, b"C:\\", 1);
    push(tr4(&[b"Syst\xE8me Linux (Z:)", b"Linux system (Z:)", b"Sistema Linux (Z:)", b"Linux-System (Z:)"]), Glyph::LinuxDrive, b"Z:\\", 1);
    for i in 0..NDRIVES {
        let d = DRIVES[i];
        let gl = if d.network {
            Glyph::NetDrive
        } else {
            Glyph::Removable
        };
        push(drive_label(i), gl, &[d.letter, b':', b'\\'], 1);
    }
    push(tr(S_TRASH), Glyph::Trash, TRASH, 0);
    n
}
fn nav_row(geo: &Geo, i: usize) -> Rect {
    // Séparateurs après Accueil et après les dossiers personnels.
    let trash = 10 + unsafe { NDRIVES };
    let gap =
        if i >= 1 { 13 } else { 0 } + if i >= 7 { 13 } else { 0 } + if i >= trash { 13 } else { 0 };
    let y = geo.nav.top + 10 + i as i32 * 32 + gap;
    rect(geo.nav.left + 6, y, geo.nav.right - 8, y + 30)
}

unsafe fn draw_nav(g: &mut Gfx, geo: &Geo) {
    if geo.nav.right <= geo.nav.left {
        return;
    }
    let p = pal();
    fill(
        g.hdc,
        g.b,
        geo.nav.left,
        geo.nav.top,
        geo.nav.right,
        geo.nav.bottom,
        p.window,
    );
    fill(
        g.hdc,
        g.b,
        geo.nav.right - 1,
        geo.nav.top,
        geo.nav.right,
        geo.nav.bottom,
        p.border,
    );
    let mut items = [(&b""[..], Glyph::Folder, EMPTY_PATH, 0); NAV_MAX];
    let n = nav_items(&mut items);
    let loc = pane().loc;
    for i in 0..n {
        let r = nav_row(geo, i);
        if r.bottom > geo.nav.bottom {
            break;
        }
        let (label, gl, path, indent) = items[i];
        let current = loc.get() == path.get();
        let bg = if current {
            p.selected
        } else if hovered(&r) {
            p.hover
        } else {
            p.window
        };
        if bg != p.window {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, bg);
        }
        if current {
            round_rect(
                g.hdc,
                g.b,
                r.left,
                r.top + 8,
                r.left + 3,
                r.bottom - 8,
                1,
                p.accent,
            );
        }
        let x = r.left + 14 + indent * 18;
        set_glyph_bg(Some(bg));
        glyph(g.hdc, g.b, g.f, gl, x, r.top + 7, 16);
        set_glyph_bg(None);
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text,
            x + 26,
            r.top + 8,
            label,
            r.right - x - 34,
        );
        if i == 0 || i == 6 || i == 9 + NDRIVES {
            let sy = r.bottom + 6;
            fill(
                g.hdc,
                g.b,
                geo.nav.left + 14,
                sy,
                geo.nav.right - 16,
                sy + 1,
                p.border,
            );
        }
    }
}

/// Vignette du fichier (images, vidéos, audio) à la place de l'icône dans
/// les affichages en icônes et en mosaïques ; `false` garde l'icône.
unsafe fn draw_thumbnail(g: &mut Gfx, pn: &Pane, idx: usize, x: i32, y: i32, s: i32, bg: u32) -> bool {
    let e = &pn.entries[idx];
    let src = match e.kind {
        _ if e.dir => return false,
        Kind::Image | Kind::File => crate::thumbs::Source::Image,
        Kind::Audio | Kind::Video => crate::thumbs::Source::Media,
        _ => return false,
    };
    let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
    let n = item_path(pn, idx, &mut path);
    n != 0 && crate::thumbs::draw_thumb(g.hdc, &path[..n], src, x, y, s, bg)
}

unsafe fn draw_pane(g: &mut Gfx, r: &Rect, which: usize) {
    let p = pal();
    let t = tab();
    let pn = &mut t.panes[which];
    let focused = !t.dual || t.focus == which;
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.window);
    if t.dual {
        // Petit en-tête du volet : chemin courant, accent sur le volet actif.
        let name = last_component(pn.loc.get());
        fill(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.top + 3,
            if focused { p.accent } else { p.border },
        );
        text_fit(
            g.hdc,
            g.f.small,
            11,
            if focused { p.text } else { p.text3 },
            r.left + 12,
            r.top + 7,
            name,
            r.right - r.left - 24,
        );
    }
    let r = &if t.dual {
        rect(r.left, r.top + 22, r.right, r.bottom)
    } else {
        *r
    };
    clamp_scroll(pn, r);
    let c = cells(r);

    if pn.nview == 0 {
        let msg: &[u8] = if pn.missing {
            tr4(&[b"Ce dossier est introuvable.", b"This folder can't be found.", b"No se encuentra esta carpeta.", b"Dieser Ordner wurde nicht gefunden."])
        } else if pn.search.len > 0 {
            tr4(&[b"Aucun \xE9l\xE9ment ne correspond \xE0 votre recherche.", b"No items match your search.", b"Ning\xFAn elemento coincide con la b\xFAsqueda.", b"Keine Elemente entsprechen Ihrer Suche."])
        } else {
            tr4(&[b"Ce dossier est vide.", b"This folder is empty.", b"Esta carpeta est\xE1 vac\xEDa.", b"Dieser Ordner ist leer."])
        };
        text_centered(
            g.hdc,
            g.f.body,
            13,
            p.text3,
            (r.left + r.right) / 2,
            r.top + 60,
            msg,
        );
        return;
    }

    if VIEW == View::Details {
        let sx_off = pn.scroll_x;
        let hr = rect(
            r.left + 8 - sx_off,
            r.top + 4,
            r.right - SCROLL_W - 8,
            r.top + 4 + c.header,
        );
        let (nx, _, dx, tx, sx) = detail_cols(&hr);
        let heads: [(&[u8], i32, SortKey); 4] = [
            (tr(S_NAME), nx - 28, SortKey::Name),
            (tr(S_DATE), dx, SortKey::Date),
            (tr(S_TYPE), tx, SortKey::Kind),
            (tr(S_SIZE), sx, SortKey::Size),
        ];
        for (label, x, key) in heads {
            text(g.hdc, g.f.caption, p.text2, x, hr.top + 8, label);
            if SORT == key {
                let ax = x + text_width(label, 12) + 8;
                set_glyph_bg(Some(p.window));
                glyph(
                    g.hdc,
                    g.b,
                    g.f,
                    if SORT_DESC {
                        Glyph::ChevronDown
                    } else {
                        Glyph::Chevron
                    },
                    ax,
                    hr.top + 10,
                    9,
                );
                set_glyph_bg(None);
            }
            if x > nx {
                fill(
                    g.hdc,
                    g.b,
                    x - 8,
                    hr.top + 6,
                    x - 7,
                    hr.bottom - 6,
                    p.border,
                );
            }
        }
        fill(
            g.hdc,
            g.b,
            hr.left,
            hr.bottom - 1,
            hr.right,
            hr.bottom,
            p.border,
        );
    }

    let first_row = (pn.scroll - c.header).max(0) / c.ch;
    let rows_visible = (r.bottom - r.top) / c.ch + 2;
    let start = (first_row * c.cols) as usize;
    let end = (((first_row + rows_visible) * c.cols) as usize).min(pn.nview);
    let mut tbuf = [0u8; 48];
    let mut sbuf = [0u8; 24];
    for v in start..end {
        let idx = pn.view[v] as usize;
        let e = pn.entries[idx];
        let mut ir = item_rect(r, &c, v, pn.scroll);
        if VIEW == View::Details {
            ir.left -= pn.scroll_x;
            ir.right -= pn.scroll_x;
        }
        if ir.bottom < r.top + c.header || ir.top > r.bottom {
            continue;
        }
        let selected = is_sel(pn, idx);
        let hover = HOVER == v as i32 && HOVER_ZONE == which as u32 + 1;
        let bg = if selected {
            if focused {
                p.selected
            } else {
                mix(p.selected, p.window, 1, 2)
            }
        } else if hover {
            p.hover
        } else {
            p.window
        };
        if bg != p.window {
            round_rect(g.hdc, g.b, ir.left, ir.top, ir.right, ir.bottom, 4, bg);
        }
        let name = display_name(&e);
        let renaming = FOCUS == Focus::Rename && RENAME_INDEX == idx && which == t.focus;
        set_glyph_bg(Some(bg));
        match VIEW {
            View::XLarge | View::Large | View::Medium => {
                let s = c.icon;
                let gx = (ir.left + ir.right - s) / 2;
                if !draw_thumbnail(g, pn, idx, gx, ir.top + 8, s, bg) {
                    glyph(g.hdc, g.b, g.f, glyph_of(e.kind), gx, ir.top + 8, s);
                    format_badge(g, &e, gx, ir.top + 8, s);
                }
                let ty = ir.top + 14 + s;
                if renaming {
                    draw_rename(g, rect(ir.left + 4, ty - 3, ir.right - 4, ty + 19));
                } else {
                    center_fit(
                        g,
                        (ir.left + ir.right) / 2,
                        ty,
                        name,
                        ir.right - ir.left - 10,
                        p.text,
                    );
                }
            }
            View::Small | View::List => {
                glyph(
                    g.hdc,
                    g.b,
                    g.f,
                    glyph_of(e.kind),
                    ir.left + 8,
                    ir.top + (c.ch - 16) / 2,
                    16,
                );
                let ty = ir.top + (c.ch - 14) / 2;
                if renaming {
                    draw_rename(g, rect(ir.left + 30, ty - 3, ir.right - 6, ty + 19));
                } else {
                    text_fit(
                        g.hdc,
                        g.f.caption,
                        12,
                        p.text,
                        ir.left + 32,
                        ty,
                        name,
                        ir.right - ir.left - 40,
                    );
                }
            }
            View::Tiles => {
                let gy = ir.top + (c.ch - c.icon) / 2;
                if !draw_thumbnail(g, pn, idx, ir.left + 10, gy, c.icon, bg) {
                    glyph(g.hdc, g.b, g.f, glyph_of(e.kind), ir.left + 10, gy, c.icon);
                    format_badge(g, &e, ir.left + 10, gy, c.icon);
                }
                let tx = ir.left + 20 + c.icon;
                let ty = ir.top + c.ch / 2 - 18;
                if renaming {
                    draw_rename(g, rect(tx - 4, ty - 3, ir.right - 6, ty + 19));
                } else {
                    text_fit(
                        g.hdc,
                        g.f.caption,
                        12,
                        p.text,
                        tx,
                        ty,
                        name,
                        ir.right - tx - 8,
                    );
                }
                text_fit(
                    g.hdc,
                    g.f.small,
                    11,
                    p.text3,
                    tx,
                    ty + 18,
                    type_name(&e, &mut tbuf),
                    ir.right - tx - 8,
                );
                if !e.dir {
                    let n = size_text(e.size, &mut sbuf);
                    text(g.hdc, g.f.small, p.text3, tx, ty + 32, &sbuf[..n]);
                }
            }
            View::Details => {
                let (nx, nw, dx, tx, sx) = detail_cols(&ir);
                glyph(
                    g.hdc,
                    g.b,
                    g.f,
                    glyph_of(e.kind),
                    ir.left + 10,
                    ir.top + (c.ch - 16) / 2,
                    16,
                );
                let ty = ir.top + (c.ch - 14) / 2;
                if renaming {
                    draw_rename(g, rect(nx - 4, ty - 3, ir.left + nw, ty + 19));
                } else {
                    text_fit(g.hdc, g.f.caption, 12, p.text, nx, ty, name, nw - 40);
                }
                let mut dbuf = [0u8; 24];
                let dn = date_text(e.mtime, &mut dbuf);
                text(g.hdc, g.f.caption, p.text2, dx, ty, &dbuf[..dn]);
                text_fit(
                    g.hdc,
                    g.f.caption,
                    12,
                    p.text2,
                    tx,
                    ty,
                    type_name(&e, &mut tbuf),
                    sx - tx - 12,
                );
                if !e.dir {
                    let n = size_text(e.size, &mut sbuf);
                    let w = text_width(&sbuf[..n], 12);
                    text(
                        g.hdc,
                        g.f.caption,
                        p.text2,
                        (ir.right - 16 - w).max(sx),
                        ty,
                        &sbuf[..n],
                    );
                }
            }
        }
        set_glyph_bg(None);
    }
    // Barre de défilement fine, comme sous Windows 11.
    let total = content_height(pn, &c);
    let view_h = r.bottom - r.top;
    if total > view_h {
        let track = rect(r.right - SCROLL_W, r.top + 4, r.right - 3, r.bottom - 4);
        let th = ((view_h * (track.bottom - track.top)) / total).max(30);
        let ty =
            track.top + (pn.scroll * (track.bottom - track.top - th)) / (total - view_h).max(1);
        let wide = hovered(&rect(
            track.left - 6,
            track.top,
            track.right + 3,
            track.bottom,
        )) || SCROLL_DRAG.is_some();
        let (l, rr) = if wide {
            (track.left, track.right)
        } else {
            (track.right - 3, track.right)
        };
        round_rect(
            g.hdc,
            g.b,
            l,
            ty,
            rr,
            ty + th,
            2,
            if wide {
                p.text3
            } else {
                mix(p.text3, p.window, 1, 2)
            },
        );
    }
    if let Some((x0, y0, x1, y1)) = LASSO {
        if which == t.focus {
            let (l, tp, rr, b) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
            fill(g.hdc, g.b, l, tp, rr, b, mix(p.window, p.accent, 2, 10));
            FrameRect(g.hdc, &rect(l, tp, rr, b), g.b.get(p.accent));
        }
    }
}

fn display_name(e: &Entry) -> &[u8] {
    let n = e.name();
    if unsafe { SHOW_EXT } || e.dir {
        return n;
    }
    match n.iter().rposition(|&c| c == b'.') {
        Some(i) if i > 0 => &n[..i],
        _ => n,
    }
}

unsafe fn center_fit(g: &mut Gfx, cx: i32, y: i32, s: &[u8], max_w: i32, color: u32) {
    let w = text_width(s, 12).min(max_w);
    text_fit(g.hdc, g.f.caption, 12, color, cx - w / 2, y, s, max_w);
}

unsafe fn draw_rename(g: &mut Gfx, r: Rect) {
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.accent);
    fill(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        p.surface,
    );
    if RENAME_FRESH {
        let w = text_width(RENAME.get(), 12).min(r.right - r.left - 10);
        fill(
            g.hdc,
            g.b,
            r.left + 4,
            r.top + 2,
            r.left + 6 + w,
            r.bottom - 2,
            mix(p.surface, p.accent, 5, 10),
        );
    }
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        p.text,
        r.left + 5,
        r.top + 3,
        RENAME.get(),
        r.right - r.left - 12,
    );
    let cx = r.left + 5 + text_width(RENAME.get(), 12).min(r.right - r.left - 12);
    if GetTickCount() / 530 % 2 == 0 {
        fill(g.hdc, g.b, cx, r.top + 3, cx + crate::gfx::caret_w(1), r.bottom - 3, p.text);
    }
}

unsafe fn draw_status(g: &mut Gfx, geo: &Geo) {
    let p = pal();
    let s = geo.status;
    fill(g.hdc, g.b, s.left, s.top, s.right, s.bottom, p.window);
    fill(g.hdc, g.b, s.left, s.top, s.right, s.top + 1, p.border);
    let pn = pane();
    let mut buf = [0u8; 256];
    let mut n = write_num(&mut buf, 0, pn.nview as u64);
    buf[n] = b' ';
    n += 1;
    let label = tr(S_ITEMS);
    buf[n..n + label.len()].copy_from_slice(label);
    n += label.len();
    let sc = sel_count(pn);
    if sc > 0 {
        buf[n..n + 5].copy_from_slice(b"   |  "[..5].as_ref());
        n += 5;
        n = write_num(&mut buf, n, sc as u64);
        buf[n] = b' ';
        n += 1;
        let l2 = tr(S_SELECTED);
        buf[n..n + l2.len()].copy_from_slice(l2);
        n += l2.len();
        if sc == 1 {
            if let Some(i) = first_sel(pn) {
                if !pn.entries[i].dir {
                    buf[n..n + 4].copy_from_slice(b"  |  "[..4].as_ref());
                    n += 4;
                    let mut sb = [0u8; 24];
                    let m = size_text(pn.entries[i].size, &mut sb);
                    buf[n..n + m].copy_from_slice(&sb[..m]);
                    n += m;
                }
            }
        }
    }
    if pn.truncated {
        let l3 = tr4(&[b"  (limite atteinte ou noms non repr\xE9sentables masqu\xE9s)", b"  (limit reached or unrepresentable names hidden)", b"  (l\xEDmite alcanzado o nombres no representables ocultos)", b"  (Grenze erreicht oder nicht darstellbare Namen ausgeblendet)"]);
        buf[n..n + l3.len()].copy_from_slice(l3);
        n += l3.len();
    }
    text(g.hdc, g.f.small, p.text2, s.left + 14, s.top + 8, &buf[..n]);
    if (MESSAGE_UNTIL.wrapping_sub(GetTickCount()) as i32) > 0 {
        let w = text_width(MESSAGE.get(), 11);
        text(
            g.hdc,
            g.f.small,
            p.accent,
            (s.left + s.right - w) / 2,
            s.top + 8,
            MESSAGE.get(),
        );
    }
    draw_job(g, &s);
    // Bascule rapide Détails / Grandes icônes, en bas à droite.
    let d = rect(s.right - 64, s.top + 3, s.right - 38, s.bottom - 3);
    let l = rect(s.right - 34, s.top + 3, s.right - 8, s.bottom - 3);
    for (r, gl, on) in [
        (d, Glyph::Details, VIEW == View::Details),
        (l, Glyph::Grid, VIEW == View::Large),
    ] {
        let bg = if on {
            p.selected
        } else if hovered(&r) {
            p.hover
        } else {
            p.window
        };
        if bg != p.window {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 3, bg);
        }
        set_glyph_bg(Some(bg));
        glyph(g.hdc, g.b, g.f, gl, r.left + 6, r.top + 4, 14);
        set_glyph_bg(None);
    }
}

// ---------- Menus déroulants ----------

#[derive(Clone, Copy, PartialEq)]
enum Item {
    Sep,
    View(View),
    Compact,
    Hidden,
    Ext,
    Nav,
    Sort(SortKey),
    Desc,
    Asc,
    NewFolder,
    NewText,
    Open,
    OpenTab,
    ExeSettings,
    Cut,
    Copy,
    Paste,
    Rename,
    Delete,
    Refresh,
    Terminal,
    CopyPath,
    ExtractAll,
    ExtractHere,
    Mount,
    Eject,
    CompressZip,
    Compress7z,
    CompressTarGz,
    CompressZipPw,
    ConnectNet,
}

fn item_label(i: Item) -> &'static [u8] {
    match i {
        Item::Sep => b"",
        Item::View(View::XLarge) => tr4(&[b"Tr\xE8s grandes ic\xF4nes", b"Extra large icons", b"Iconos muy grandes", b"Extra gro\xDFe Symbole"]),
        Item::View(View::Large) => tr4(&[b"Grandes ic\xF4nes", b"Large icons", b"Iconos grandes", b"Gro\xDFe Symbole"]),
        Item::View(View::Medium) => tr4(&[b"Ic\xF4nes moyennes", b"Medium icons", b"Iconos medianos", b"Mittelgro\xDFe Symbole"]),
        Item::View(View::Small) => tr4(&[b"Petites ic\xF4nes", b"Small icons", b"Iconos peque\xF1os", b"Kleine Symbole"]),
        Item::View(View::List) => tr4(&[b"Liste", b"List", b"Lista", b"Liste"]),
        Item::View(View::Details) => tr4(&[b"D\xE9tails", b"Details", b"Detalles", b"Details"]),
        Item::View(View::Tiles) => tr4(&[b"Mosa\xEFques", b"Tiles", b"Mosaicos", b"Kacheln"]),
        Item::Compact => tr4(&[b"Mode compact", b"Compact view", b"Vista compacta", b"Kompakte Ansicht"]),
        Item::Hidden => tr4(&[b"\xC9l\xE9ments masqu\xE9s", b"Hidden items", b"Elementos ocultos", b"Ausgeblendete Elemente"]),
        Item::Ext => tr4(&[b"Extensions de noms de fichiers", b"File name extensions", b"Extensiones de nombre de archivo", b"Dateinamenerweiterungen"]),
        Item::Nav => tr4(&[b"Volet de navigation", b"Navigation pane", b"Panel de navegaci\xF3n", b"Navigationsbereich"]),
        Item::Sort(SortKey::Name) => tr(S_NAME),
        Item::Sort(SortKey::Date) => tr(S_DATE),
        Item::Sort(SortKey::Kind) => tr(S_TYPE),
        Item::Sort(SortKey::Size) => tr(S_SIZE),
        Item::Asc => tr4(&[b"Croissant", b"Ascending", b"Ascendente", b"Aufsteigend"]),
        Item::Desc => tr4(&[b"D\xE9croissant", b"Descending", b"Descendente", b"Absteigend"]),
        Item::NewFolder => tr4(&[b"Dossier", b"Folder", b"Carpeta", b"Ordner"]),
        Item::NewText => tr4(&[b"Document texte", b"Text document", b"Documento de texto", b"Textdokument"]),
        Item::Open => tr(S_OPEN),
        Item::OpenTab => tr4(&[b"Ouvrir dans un nouvel onglet", b"Open in new tab", b"Abrir en una pesta\xF1a nueva", b"In neuer Registerkarte \xF6ffnen"]),
        Item::ExeSettings => tr4(&[b"R\xE9glages de l'ex\xE9cutable", b"Executable settings", b"Ajustes del ejecutable", b"Einstellungen der ausf\xFChrbaren Datei"]),
        Item::Cut => tr(S_CUT),
        Item::Copy => tr(S_COPY),
        Item::Paste => tr(S_PASTE),
        Item::Rename => tr(S_RENAME),
        Item::Delete => tr(S_DELETE),
        Item::Refresh => tr4(&[b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"]),
        Item::Terminal => tr4(&[b"Ouvrir dans le terminal", b"Open in terminal", b"Abrir en el terminal", b"Im Terminal \xF6ffnen"]),
        Item::CopyPath => tr4(&[b"Copier le chemin d'acc\xE8s", b"Copy as path", b"Copiar como ruta de acceso", b"Als Pfad kopieren"]),
        Item::ExtractAll => t4([
            b"Extraire tout...",
            b"Extract all...",
            b"Extraer todo...",
            b"Alle extrahieren...",
        ]),
        Item::ExtractHere => t4([
            b"Extraire ici",
            b"Extract here",
            b"Extraer aqu\xED",
            b"Hier entpacken",
        ]),
        Item::Mount => t4([b"Monter", b"Mount", b"Montar", b"Einbinden"]),
        Item::Eject => {
            let network = unsafe {
                first_sel(pane()).is_some_and(|i| pane().entries[i].kind == Kind::Network)
            };
            if network {
                t4([b"D\xE9connecter", b"Disconnect", b"Desconectar", b"Trennen"])
            } else {
                t4([b"\xC9jecter", b"Eject", b"Expulsar", b"Auswerfen"])
            }
        }
        Item::CompressZip => t4([
            b"Compresser en fichier ZIP",
            b"Compress to ZIP file",
            b"Comprimir en archivo ZIP",
            b"In ZIP-Datei komprimieren",
        ]),
        Item::Compress7z => t4([
            b"Compresser en fichier 7z",
            b"Compress to 7z file",
            b"Comprimir en archivo 7z",
            b"In 7z-Datei komprimieren",
        ]),
        Item::CompressTarGz => t4([
            b"Compresser en fichier TAR.GZ",
            b"Compress to TAR.GZ file",
            b"Comprimir en archivo TAR.GZ",
            b"In TAR.GZ-Datei komprimieren",
        ]),
        Item::CompressZipPw => t4([
            b"Compresser en ZIP avec mot de passe...",
            b"Compress to ZIP with password...",
            b"Comprimir en ZIP con contrase\xF1a...",
            b"In ZIP mit Kennwort komprimieren...",
        ]),
        Item::ConnectNet => t4([
            b"Connecter un lecteur r\xE9seau...",
            b"Map network drive...",
            b"Conectar unidad de red...",
            b"Netzlaufwerk verbinden...",
        ]),
    }
}

fn item_checked(i: Item) -> bool {
    unsafe {
        match i {
            Item::View(v) => VIEW == v,
            Item::Compact => COMPACT,
            Item::Hidden => SHOW_HIDDEN,
            Item::Ext => SHOW_EXT,
            Item::Nav => SHOW_NAV,
            Item::Sort(k) => SORT == k,
            Item::Desc => SORT_DESC,
            Item::Asc => !SORT_DESC,
            _ => false,
        }
    }
}

const MENU_MAX: usize = 28;

unsafe fn menu_items(out: &mut [Item; MENU_MAX]) -> usize {
    let list: &[Item] = match MENU {
        Menu::View => &[
            Item::View(View::XLarge),
            Item::View(View::Large),
            Item::View(View::Medium),
            Item::View(View::Small),
            Item::View(View::List),
            Item::View(View::Details),
            Item::View(View::Tiles),
            Item::Sep,
            Item::Compact,
            Item::Nav,
            Item::Hidden,
            Item::Ext,
        ],
        Menu::Sort => &[
            Item::Sort(SortKey::Name),
            Item::Sort(SortKey::Date),
            Item::Sort(SortKey::Kind),
            Item::Sort(SortKey::Size),
            Item::Sep,
            Item::Asc,
            Item::Desc,
        ],
        Menu::New => &[Item::NewFolder, Item::NewText],
        Menu::Context { on_item: true, .. } => &[
            Item::Open,
            Item::OpenTab,
            Item::ExeSettings,
            Item::ExtractAll,
            Item::ExtractHere,
            Item::Mount,
            Item::Eject,
            Item::Sep,
            Item::Cut,
            Item::Copy,
            Item::Paste,
            Item::Sep,
            Item::Rename,
            Item::Delete,
            Item::Sep,
            Item::CopyPath,
            Item::Sep,
            Item::CompressZip,
            Item::Compress7z,
            Item::CompressTarGz,
            Item::CompressZipPw,
        ],
        Menu::Context { .. } => &[
            Item::View(View::Large),
            Item::View(View::Details),
            Item::Sep,
            Item::Refresh,
            Item::Paste,
            Item::NewFolder,
            Item::NewText,
            Item::Sep,
            Item::Hidden,
            Item::Terminal,
            Item::ConnectNet,
        ],
        Menu::None => &[],
    };
    let p = pane();
    let executable =
        first_sel(p).is_some_and(|i| !p.entries[i].dir && ends_ci(p.entries[i].name(), b".exe"));
    let in_pc = p.loc.is(PC);
    let in_home = p.loc.is(HOME);
    let first_kind = first_sel(p).map(|i| p.entries[i].kind);
    let formats = if CREATE_FORMATS == u32::MAX {
        WinluxArchiveFormats()
    } else {
        CREATE_FORMATS
    };
    CREATE_FORMATS = formats;
    let mut count = 0;
    let mut last_sep = true;
    for &item in list {
        if item == Item::ExeSettings && !executable {
            continue;
        }
        let keep = match item {
            Item::ExtractAll | Item::ExtractHere => SEL_FLAGS & CAN_READ != 0,
            Item::Mount => SEL_FLAGS & CAN_MOUNT != 0,
            Item::Eject => in_pc && matches!(first_kind, Some(Kind::Removable | Kind::Network)),
            Item::CompressZip | Item::CompressZipPw => !in_pc && !in_home && formats & 1 != 0,
            Item::Compress7z => !in_pc && !in_home && formats & 2 != 0,
            Item::CompressTarGz => !in_pc && !in_home && formats & 8 != 0,
            Item::ConnectNet => in_pc,
            _ => true,
        };
        if !keep {
            continue;
        }
        // Pas de séparateur en double ni en fin de menu.
        if item == Item::Sep {
            if last_sep {
                continue;
            }
            last_sep = true;
        } else {
            last_sep = false;
        }
        out[count] = item;
        count += 1;
    }
    if count > 0 && out[count - 1] == Item::Sep {
        count -= 1;
    }
    count
}

unsafe fn menu_origin(geo: &Geo) -> (i32, i32) {
    let rects = command_rects(geo);
    match MENU {
        Menu::View => (rects[7].left, rects[7].bottom + 4),
        Menu::Sort => (rects[6].left, rects[6].bottom + 4),
        Menu::New => (rects[0].left, rects[0].bottom + 4),
        Menu::Context { x, y, .. } => (x, y),
        Menu::None => (0, 0),
    }
}
const MENU_W: i32 = 270;
fn menu_row_h(i: Item) -> i32 {
    if i == Item::Sep {
        9
    } else {
        32
    }
}

unsafe fn menu_box(geo: &Geo, f: &Frame) -> Rect {
    let mut items = [Item::Sep; MENU_MAX];
    let n = menu_items(&mut items);
    let h: i32 = items[..n].iter().map(|&i| menu_row_h(i)).sum::<i32>() + 8;
    let (mut x, mut y) = menu_origin(geo);
    x = x.min(f.client.right - MENU_W - 6);
    if y + h > f.client.bottom - 4 {
        y = (y - h).max(f.client.top + 4);
    }
    rect(x, y, x + MENU_W, y + h)
}

unsafe fn draw_menu(g: &mut Gfx, geo: &Geo, f: &Frame) {
    if MENU == Menu::None {
        return;
    }
    let p = pal();
    let r = menu_box(geo, f);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.border);
    round_rect(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        7,
        p.surface,
    );
    let mut items = [Item::Sep; MENU_MAX];
    let n = menu_items(&mut items);
    let mut y = r.top + 4;
    for &it in items[..n].iter() {
        let h = menu_row_h(it);
        if it == Item::Sep {
            fill(g.hdc, g.b, r.left + 1, y + 4, r.right - 1, y + 5, p.border);
            y += h;
            continue;
        }
        let row = rect(r.left + 4, y, r.right - 4, y + h);
        let bg = if hovered(&row) { p.hover } else { p.surface };
        if bg != p.surface {
            round_rect(g.hdc, g.b, row.left, row.top, row.right, row.bottom, 4, bg);
        }
        if item_checked(it) {
            let is_radio = matches!(it, Item::View(_) | Item::Sort(_) | Item::Asc | Item::Desc);
            if is_radio {
                ellipse(
                    g.hdc,
                    g.b,
                    row.left + 12,
                    row.top + 12,
                    row.left + 20,
                    row.top + 20,
                    p.accent,
                );
            } else {
                set_glyph_bg(Some(bg));
                fill(
                    g.hdc,
                    g.b,
                    row.left + 10,
                    row.top + 15,
                    row.left + 14,
                    row.top + 19,
                    p.text,
                );
                line(
                    g.hdc,
                    g.b,
                    row.left + 13,
                    row.top + 18,
                    row.left + 21,
                    row.top + 10,
                    2,
                    p.text,
                );
                set_glyph_bg(None);
            }
        }
        let gl = match it {
            Item::View(View::Details) => Some(Glyph::Details),
            Item::View(View::List) => Some(Glyph::List),
            Item::View(View::Tiles) => Some(Glyph::Tiles),
            Item::View(_) => Some(Glyph::Grid),
            Item::Cut => Some(Glyph::Cut),
            Item::Copy => Some(Glyph::Copy),
            Item::Paste => Some(Glyph::Paste),
            Item::Rename => Some(Glyph::Rename),
            Item::Delete => Some(Glyph::Delete),
            Item::Refresh => Some(Glyph::Refresh),
            Item::NewFolder => Some(Glyph::Folder),
            Item::NewText => Some(Glyph::Text),
            Item::Terminal => Some(Glyph::Terminal),
            Item::Open => Some(Glyph::Forward),
            Item::OpenTab => Some(Glyph::Plus),
            Item::Nav => Some(Glyph::Split),
            Item::ExtractAll | Item::ExtractHere => Some(Glyph::Archive),
            Item::CompressZip | Item::Compress7z | Item::CompressTarGz | Item::CompressZipPw => {
                Some(Glyph::Archive)
            }
            Item::Mount => Some(Glyph::DiskImage),
            Item::Eject => Some(Glyph::Eject),
            Item::ConnectNet => Some(Glyph::NetDrive),
            _ => None,
        };
        if let Some(gl) = gl {
            set_glyph_bg(Some(bg));
            glyph(g.hdc, g.b, g.f, gl, row.left + 32, row.top + 8, 16);
            set_glyph_bg(None);
        }
        text(
            g.hdc,
            g.f.caption,
            p.text,
            row.left + 60,
            row.top + 9,
            item_label(it),
        );
        y += h;
    }
}

unsafe fn menu_hit(geo: &Geo, f: &Frame, x: i32, y: i32) -> Option<Option<Item>> {
    if MENU == Menu::None {
        return None;
    }
    let r = menu_box(geo, f);
    if !inside(&r, x, y) {
        return Some(None);
    }
    let mut items = [Item::Sep; MENU_MAX];
    let n = menu_items(&mut items);
    let mut top = r.top + 4;
    for &it in items[..n].iter() {
        let h = menu_row_h(it);
        if y >= top && y < top + h {
            return Some(if it == Item::Sep { None } else { Some(it) });
        }
        top += h;
    }
    Some(None)
}

// ---------- Actions sur les fichiers ----------

unsafe fn unique_name(
    dir: &[u8],
    base: &[u8],
    ext: &[u8],
    out: &mut [u8; PATH_LEN + NAME_LEN + 2],
) -> usize {
    for k in 1..100u64 {
        let mut name = [0u8; NAME_LEN];
        let suffix_len = if k > 1 {
            if k < 10 {
                4
            } else {
                5
            }
        } else {
            0
        };
        if base.len() + ext.len() + suffix_len >= NAME_LEN {
            return 0;
        }
        let mut n = base.len();
        name[..n].copy_from_slice(base);
        if k > 1 {
            name[n] = b' ';
            name[n + 1] = b'(';
            n = write_num(&mut name, n + 2, k);
            name[n] = b')';
            n += 1;
        }
        name[n..n + ext.len()].copy_from_slice(ext);
        n += ext.len();
        let len = join(out, dir, &name[..n]);
        if len == 0 {
            return 0;
        }
        let mut wb = [0u16; PATH_LEN + NAME_LEN + 2];
        let a = GetFileAttributesW(wstr(&mut wb, &out[..len]));
        if a == u32::MAX {
            return len;
        }
    }
    0
}

unsafe fn select_named(p: &mut Pane, name: &[u8]) -> Option<usize> {
    p.sel = [0; MAX_ENTRIES / 64];
    let i = (0..p.count).find(|&i| p.entries[i].name() == name)?;
    set_sel(p, i, true);
    p.anchor = i;
    Some(i)
}

unsafe fn new_item(folder: bool) {
    let p = pane();
    if p.loc.is(HOME) || p.loc.is(PC) {
        return;
    }
    let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
    let n = if folder {
        unique_name(p.loc.get(), tr(S_NEW_FOLDER), b"", &mut path)
    } else {
        unique_name(p.loc.get(), tr4(&[b"Nouveau document texte", b"New text document", b"Nuevo documento de texto", b"Neues Textdokument"]), b".txt", &mut path)
    };
    if n == 0 {
        return;
    }
    let mut wp = [0u16; PATH_LEN + NAME_LEN + 2];
    wstr(&mut wp, &path[..n]);
    let ok = if folder {
        CreateDirectoryW(wp.as_ptr(), core::ptr::null_mut()) != 0
    } else {
        let h = CreateFileW(
            wp.as_ptr(),
            0x4000_0000,
            0,
            core::ptr::null_mut(),
            1,
            0x80,
            core::ptr::null_mut(),
        );
        if h as isize != -1 {
            CloseHandle(h);
            true
        } else {
            false
        }
    };
    if !ok {
        notify(tr4(&[b"Impossible de cr\xE9er l'\xE9l\xE9ment ici (droits insuffisants ?).", b"Can't create the item here (insufficient rights?).", b"No se puede crear el elemento aqu\xED (\xBFpermisos insuficientes?).", b"Element kann hier nicht erstellt werden (fehlende Rechte?)."]));
        return;
    }
    reload(p);
    let name_start = path[..n]
        .iter()
        .rposition(|&c| c == b'\\')
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut name = [0u8; NAME_LEN];
    let nl = (n - name_start).min(NAME_LEN);
    name[..nl].copy_from_slice(&path[name_start..name_start + nl]);
    if let Some(i) = select_named(p, &name[..nl]) {
        start_rename(i);
    }
}

unsafe fn start_rename(index: usize) {
    let p = pane();
    RENAME.set(p.entries[index].name());
    RENAME_INDEX = index;
    RENAME_FRESH = true;
    RENAME_TAIL = 0;
    FOCUS = Focus::Rename;
}

unsafe fn commit_rename() {
    FOCUS = Focus::List;
    let p = pane();
    if RENAME_INDEX >= p.count || RENAME.len == 0 {
        return;
    }
    let old_name = p.entries[RENAME_INDEX].name();
    if old_name == RENAME.get() {
        return;
    }
    if RENAME.get().iter().any(|&c| {
        matches!(
            c,
            b'\\' | b'/' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|'
        )
    }) {
        notify(b"Un nom ne peut pas contenir \\ / : * ? \" < > |");
        return;
    }
    let mut from = [0u8; PATH_LEN + NAME_LEN + 2];
    let mut to = [0u8; PATH_LEN + NAME_LEN + 2];
    if join(&mut from, p.loc.get(), old_name) == 0 || join(&mut to, p.loc.get(), RENAME.get()) == 0
    {
        notify(tr4(&[b"Chemin trop long : renommage annul\xE9.", b"Path too long: rename cancelled.", b"Ruta demasiado larga: cambio de nombre cancelado.", b"Pfad zu lang: Umbenennen abgebrochen."]));
        return;
    }
    let mut wfrom = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut wto = [0u16; PATH_LEN + NAME_LEN + 2];
    wstr(
        &mut wfrom,
        &from[..from.iter().position(|&c| c == 0).unwrap_or(from.len())],
    );
    wstr(
        &mut wto,
        &to[..to.iter().position(|&c| c == 0).unwrap_or(to.len())],
    );
    if MoveFileW(wfrom.as_ptr(), wto.as_ptr()) == 0 {
        notify(tr4(&[b"Renommage impossible.", b"Can't rename.", b"No se puede cambiar el nombre.", b"Umbenennen nicht m\xF6glich."]));
        return;
    }
    let new_name = RENAME;
    reload(p);
    select_named(p, new_name.get());
}

unsafe fn copy_selection(cut: bool) {
    let p = pane();
    NCLIP = 0;
    CLIP_CUT = cut;
    for v in 0..p.nview {
        let i = p.view[v] as usize;
        if !is_sel(p, i) || NCLIP == CLIP_MAX {
            continue;
        }
        let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
        let n = item_path(p, i, &mut path);
        if n == 0 || !CLIP[NCLIP].set(&path[..n]) {
            notify(tr4(&[b"Chemin trop long : \xE9l\xE9ment ignor\xE9.", b"Path too long: item skipped.", b"Ruta demasiado larga: elemento omitido.", b"Pfad zu lang: Element \xFCbersprungen."]));
            continue;
        }
        NCLIP += 1;
    }
    if NCLIP > 0 {
        // Publie aussi la sélection (CF_HDROP) : historique Win+V et autres programmes.
        let mut list: [&[u8]; CLIP_MAX] = [b""; CLIP_MAX];
        for (k, slot) in list.iter_mut().enumerate().take(NCLIP) {
            *slot = CLIP[k].get();
        }
        crate::cliphist::set_files(&list[..NCLIP]);
        CLIP_SEQ = GetClipboardSequenceNumber();
        notify(if cut {
            tr4(&[b"\xC9l\xE9ments coup\xE9s : collez-les dans un autre dossier.", b"Items cut: paste them into another folder.", b"Elementos cortados: p\xE9guelos en otra carpeta.", b"Elemente ausgeschnitten: F\xFCgen Sie sie in einen anderen Ordner ein."])
        } else {
            tr4(&[b"\xC9l\xE9ments copi\xE9s.", b"Items copied.", b"Elementos copiados.", b"Elemente kopiert."])
        });
    }
}

/// GetFullPathNameW résout aussi les alias C:/Z: et les liens symboliques
/// existants dans Winlux. La destination est reconstruite depuis son parent
/// existant avant toute création.
unsafe fn canonical_path(path: &[u8]) -> Option<Path> {
    let mut wsrc = [0u16; PATH_LEN + NAME_LEN + 2];
    wstr(&mut wsrc, path);
    let mut wout = [0u16; PATH_LEN];
    let n = GetFullPathNameW(
        wsrc.as_ptr(),
        PATH_LEN as u32,
        wout.as_mut_ptr(),
        core::ptr::null_mut(),
    ) as usize;
    if n == 0 || n >= PATH_LEN {
        return None;
    }
    let mut result = EMPTY_PATH;
    for i in 0..n {
        result.buf[i] = if wout[i] <= 0xFF { wout[i] as u8 } else { b'?' };
    }
    result.len = n as u16;
    Some(result)
}

fn same_or_descendant(path: &[u8], ancestor: &[u8]) -> bool {
    let trim = |s: &[u8]| {
        let mut n = s.len();
        while n > 3 && matches!(s[n - 1], b'\\' | b'/') {
            n -= 1;
        }
        n
    };
    let (pn, an) = (trim(path), trim(ancestor));
    an != 0
        && pn >= an
        && cmp_ci(&path[..an], &ancestor[..an]).is_eq()
        && (pn == an
            || matches!(ancestor[an - 1], b'\\' | b'/')
            || matches!(path[an], b'\\' | b'/'))
}

unsafe fn safe_destination(from: &[u8], destination_dir: &[u8]) -> bool {
    match (canonical_path(from), canonical_path(destination_dir)) {
        (Some(src), Some(dst)) => !same_or_descendant(dst.get(), src.get()),
        _ => false,
    }
}

/// Première passe en lecture seule : longueur de tous les chemins, droits de
/// lecture et cycles de liens contrôlés avant de créer la destination.
unsafe fn preflight_tree(from: &[u8], to: &[u8], ancestors: &mut [Path; 17], depth: usize) -> bool {
    if depth >= ancestors.len() || !valid_path(from) || !valid_path(to) {
        return false;
    }
    let Some(canonical) = canonical_path(from) else {
        return false;
    };
    if ancestors[..depth]
        .iter()
        .any(|p| cmp_ci(p.get(), canonical.get()).is_eq())
    {
        return false;
    }
    let mut wb = [0u16; PATH_LEN + NAME_LEN + 2];
    let a = GetFileAttributesW(wstr(&mut wb, from));
    if a == u32::MAX {
        return false;
    }
    if a & 0x10 == 0 {
        return true;
    }
    ancestors[depth] = canonical;
    let mut wpattern = [0u16; PATH_LEN + NAME_LEN + 2];
    if wjoin(&mut wpattern, from, b"*") == 0 {
        return false;
    }
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(wpattern.as_ptr(), &mut data);
    if h as isize == -1 {
        return GetLastError() == 2 || GetLastError() == 18;
    }
    let ok = loop {
        let name = find_name_w(&data);
        if name != b"." && name != b".." && !name.is_empty() {
            if !valid_component(name) {
                break false;
            }
            let mut f2 = [0u8; PATH_LEN + NAME_LEN + 2];
            let mut t2 = [0u8; PATH_LEN + NAME_LEN + 2];
            let fnl = join(&mut f2, from, name);
            let tnl = join(&mut t2, to, name);
            if fnl == 0 || tnl == 0 || !preflight_tree(&f2[..fnl], &t2[..tnl], ancestors, depth + 1)
            {
                break false;
            }
        }
        if FindNextFileW(h, &mut data) == 0 {
            break GetLastError() == 2 || GetLastError() == 18;
        }
    };
    FindClose(h);
    ok
}

/// Ne retire que l'arbre de destination créé par cette copie, jamais la source.
unsafe fn rollback_copy(path: &[u8], depth: usize) -> bool {
    if depth > 17 {
        return false;
    }
    let mut wb = [0u16; PATH_LEN + NAME_LEN + 2];
    let a = GetFileAttributesW(wstr(&mut wb, path));
    if a == u32::MAX {
        return true;
    }
    if a & 0x10 == 0 {
        let mut wd = [0u16; PATH_LEN + NAME_LEN + 2];
        return DeleteFileW(wstr(&mut wd, path)) != 0;
    }
    let mut wpattern = [0u16; PATH_LEN + NAME_LEN + 2];
    if wjoin(&mut wpattern, path, b"*") == 0 {
        return false;
    }
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(wpattern.as_ptr(), &mut data);
    let mut ok = true;
    if h as isize != -1 {
        loop {
            let name = find_name_w(&data);
            if name != b"." && name != b".." && !name.is_empty() {
                if !valid_component(name) {
                    ok = false;
                    if FindNextFileW(h, &mut data) == 0 {
                        break;
                    }
                    continue;
                }
                let mut child = [0u8; PATH_LEN + NAME_LEN + 2];
                let n = join(&mut child, path, name);
                if n == 0 || !rollback_copy(&child[..n], depth + 1) {
                    ok = false;
                }
            }
            if FindNextFileW(h, &mut data) == 0 {
                break;
            }
        }
        FindClose(h);
    } else if GetLastError() != 2 && GetLastError() != 18 {
        return false;
    }
    let mut wr = [0u16; PATH_LEN + NAME_LEN + 2];
    ok && RemoveDirectoryW(wstr(&mut wr, path)) != 0
}

/// CREATE_NEW réserve le nom de façon atomique. Un échec de lecture/écriture
/// ne supprime que le fichier dont cette fonction a créé le handle.
unsafe fn copy_file_new(from: &[u8], to: &[u8]) -> bool {
    let mut wfb = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut wtb = [0u16; PATH_LEN + NAME_LEN + 2];
    let source = CreateFileW(
        wstr(&mut wfb, from),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0x80,
        core::ptr::null_mut(),
    );
    if source as isize == -1 {
        return false;
    }
    let target = CreateFileW(
        wstr(&mut wtb, to),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        1,
        0x80,
        core::ptr::null_mut(),
    );
    if target as isize == -1 {
        CloseHandle(source);
        return false;
    }
    let mut block = [0u8; 32768];
    let ok = loop {
        let mut read = 0;
        if ReadFile(
            source,
            block.as_mut_ptr(),
            block.len() as u32,
            &mut read,
            core::ptr::null_mut(),
        ) == 0
        {
            break false;
        }
        if read == 0 {
            break true;
        }
        let mut at = 0;
        while at < read {
            let mut written = 0;
            if WriteFile(
                target,
                block.as_ptr().add(at as usize),
                read - at,
                &mut written,
                core::ptr::null_mut(),
            ) == 0
                || written == 0
            {
                break;
            }
            at += written;
        }
        if at != read {
            break false;
        }
    };
    CloseHandle(source);
    let ok = CloseHandle(target) != 0 && ok;
    if !ok {
        let mut wd = [0u16; PATH_LEN + NAME_LEN + 2];
        if DeleteFileW(wstr(&mut wd, to)) == 0 {
            notify(tr4(&[b"Copie interrompue : le fichier cible partiel reste accessible.", b"Copy interrupted: the partial target file is still available.", b"Copia interrumpida: el archivo de destino parcial sigue disponible.", b"Kopieren unterbrochen: Die unvollst\xE4ndige Zieldatei bleibt verf\xFCgbar."]));
        }
    }
    ok
}

unsafe fn copy_tree_inner(from: &[u8], to: &[u8], depth: u32, created: &mut bool) -> bool {
    if !valid_path(from) || !valid_path(to) {
        return false;
    }
    let mut wb = [0u16; PATH_LEN + NAME_LEN + 2];
    let a = GetFileAttributesW(wstr(&mut wb, from));
    if a == u32::MAX {
        return false;
    }
    if a & 0x10 == 0 {
        let ok = copy_file_new(from, to);
        *created = ok;
        return ok;
    }
    let mut wt = [0u16; PATH_LEN + NAME_LEN + 2];
    if depth > 16 || CreateDirectoryW(wstr(&mut wt, to), core::ptr::null_mut()) == 0 {
        return false;
    }
    *created = true;
    let mut wpattern = [0u16; PATH_LEN + NAME_LEN + 2];
    if wjoin(&mut wpattern, from, b"*") == 0 {
        return false;
    }
    let mut data: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(wpattern.as_ptr(), &mut data);
    if h as isize == -1 {
        return GetLastError() == 2 || GetLastError() == 18;
    }
    let ok = loop {
        let name = find_name_w(&data);
        if name != b"." && name != b".." && !name.is_empty() {
            if !valid_component(name) {
                break false;
            }
            let mut f2 = [0u8; PATH_LEN + NAME_LEN + 2];
            let mut t2 = [0u8; PATH_LEN + NAME_LEN + 2];
            let fnl = join(&mut f2, from, name);
            let tnl = join(&mut t2, to, name);
            let mut child_created = false;
            if fnl == 0
                || tnl == 0
                || !copy_tree_inner(&f2[..fnl], &t2[..tnl], depth + 1, &mut child_created)
            {
                break false;
            }
        }
        if FindNextFileW(h, &mut data) == 0 {
            break GetLastError() == 2 || GetLastError() == 18;
        }
    };
    FindClose(h);
    ok
}

unsafe fn copy_tree(from: &[u8], to: &[u8]) -> bool {
    let Some(dir) = parent(to) else {
        return false;
    };
    if !safe_destination(from, dir) {
        return false;
    }
    let mut ancestors = [EMPTY_PATH; 17];
    if !preflight_tree(from, to, &mut ancestors, 0) {
        return false;
    }
    let mut wb = [0u16; PATH_LEN + NAME_LEN + 2];
    if GetFileAttributesW(wstr(&mut wb, to)) != u32::MAX {
        return false;
    }
    let mut created = false;
    if copy_tree_inner(from, to, 0, &mut created) {
        return true;
    }
    if created && !rollback_copy(to, 0) {
        notify(tr4(&[b"Copie interrompue : une copie partielle reste dans le dossier cible.", b"Copy interrupted: a partial copy remains in the target folder.", b"Copia interrumpida: queda una copia parcial en la carpeta de destino.", b"Kopieren unterbrochen: Eine Teilkopie bleibt im Zielordner."]));
    }
    false
}

/// Charge les fichiers du presse-papiers système (copiés ailleurs ou recollés
/// depuis l'historique Win+V) puis les colle dans le dossier courant.
pub unsafe fn paste_from_clipboard() {
    let mut buf = [0u8; 8192];
    let n = crate::cliphist::get_files(&mut buf);
    if n == 0 {
        return;
    }
    NCLIP = 0;
    CLIP_CUT = false;
    for path in buf[..n].split(|&c| c == b'\n') {
        if NCLIP < CLIP_MAX && !path.is_empty() && CLIP[NCLIP].set(path) {
            NCLIP += 1;
        }
    }
    CLIP_SEQ = GetClipboardSequenceNumber();
    paste();
}

unsafe fn paste() {
    let p = pane();
    if NCLIP == 0 || p.loc.is(HOME) || p.loc.is(PC) {
        return;
    }
    let mut failed = 0;
    let mut sources_kept = 0;
    let mut clip_remaining = 0;
    for k in 0..NCLIP {
        let src = CLIP[k];
        if CLIP_CUT {
            if let Some(source_dir) = parent(src.get()) {
                if let (Some(a), Some(b)) =
                    (canonical_path(source_dir), canonical_path(p.loc.get()))
                {
                    // Couper puis coller au même endroit ne doit ni renommer
                    // la source en « (2) » ni la recopier.
                    if cmp_ci(a.get(), b.get()).is_eq() {
                        continue;
                    }
                }
            }
        }
        if !safe_destination(src.get(), p.loc.get()) {
            failed += 1;
            if CLIP_CUT {
                CLIP[clip_remaining] = src;
                clip_remaining += 1;
            }
            continue;
        }
        let name = last_component(src.get());
        let (base, ext) = match name.iter().rposition(|&c| c == b'.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name, &b""[..]),
        };
        let mut dst = [0u8; PATH_LEN + NAME_LEN + 2];
        let mut direct = [0u8; PATH_LEN + NAME_LEN + 2];
        let dn = join(&mut direct, p.loc.get(), name);
        if dn == 0 {
            failed += 1;
            if CLIP_CUT {
                CLIP[clip_remaining] = src;
                clip_remaining += 1;
            }
            continue;
        }
        let mut wd = [0u16; PATH_LEN + NAME_LEN + 2];
        let n = if GetFileAttributesW(wstr(&mut wd, &direct[..dn])) == u32::MAX {
            dst[..dn + 1].copy_from_slice(&direct[..dn + 1]);
            dn
        } else {
            unique_name(p.loc.get(), base, ext, &mut dst)
        };
        if n == 0 {
            failed += 1;
            if CLIP_CUT {
                CLIP[clip_remaining] = src;
                clip_remaining += 1;
            }
            continue;
        }
        let mut wsrc_b = [0u16; PATH_LEN + NAME_LEN + 2];
        let mut wdst_b = [0u16; PATH_LEN + NAME_LEN + 2];
        let moved =
            CLIP_CUT && MoveFileW(wstr(&mut wsrc_b, src.get()), wstr(&mut wdst_b, &dst[..n])) != 0;
        let ok = moved || copy_tree(src.get(), &dst[..n]);
        if !ok {
            failed += 1;
        }
        if CLIP_CUT && !moved {
            CLIP[clip_remaining] = src;
            clip_remaining += 1;
            if ok {
                sources_kept += 1;
            }
        }
    }
    if CLIP_CUT {
        NCLIP = clip_remaining;
    }
    reload(p);
    if failed > 0 {
        notify(tr4(&[b"Collage refus\xE9 ou incomplet : chemin trop long, cycle ou droits. Sources conserv\xE9es.", b"Paste refused or incomplete: path too long, loop or rights. Sources kept.", b"Pegado rechazado o incompleto: ruta demasiado larga, ciclo o permisos. Se conservan los originales.", b"Einf\xFCgen abgelehnt oder unvollst\xE4ndig: Pfad zu lang, Schleife oder Rechte. Quellen bleiben erhalten."]));
    } else if sources_kept > 0 {
        notify(tr4(&[b"Copie termin\xE9e. D\xE9placement indisponible : sources conserv\xE9es.", b"Copy finished. Move unavailable: sources kept.", b"Copia terminada. No se puede mover: se conservan los originales.", b"Kopieren abgeschlossen. Verschieben nicht m\xF6glich: Quellen bleiben erhalten."]));
    }
    // L'autre volet affiche peut-être le dossier source ou cible.
    let t = tab();
    if t.dual {
        let other = &mut t.panes[1 - t.focus];
        reload(other);
    }
}

// ---------- Drag & drop de fichiers ----------

pub unsafe fn file_drag_active() -> bool {
    FILE_DRAG_ACTIVE
}

pub unsafe fn cancel_file_drag() {
    FILE_DRAG_START = None;
    FILE_DRAG_ACTIVE = false;
}

pub unsafe fn drop_files_to(dest: &[u8], do_copy: bool) {
    let p = pane();
    if p.loc.is(HOME) || p.loc.is(PC) {
        cancel_file_drag();
        return;
    }
    let mut failed = 0u32;
    for v in 0..p.nview {
        let i = p.view[v] as usize;
        if !is_sel(p, i) {
            continue;
        }
        let mut src = [0u8; PATH_LEN + NAME_LEN + 2];
        let sn = item_path(p, i, &mut src);
        if sn == 0 {
            failed += 1;
            continue;
        }
        if !safe_destination(&src[..sn], dest) {
            failed += 1;
            continue;
        }
        let name = p.entries[i].name();
        let (base, ext) = match name.iter().rposition(|&c| c == b'.') {
            Some(k) if k > 0 => (&name[..k], &name[k..]),
            _ => (name, &b""[..]),
        };
        let mut dst = [0u8; PATH_LEN + NAME_LEN + 2];
        let mut direct = [0u8; PATH_LEN + NAME_LEN + 2];
        let dn = join(&mut direct, dest, name);
        if dn == 0 {
            failed += 1;
            continue;
        }
        let mut wd = [0u16; PATH_LEN + NAME_LEN + 2];
        let n = if GetFileAttributesW(wstr(&mut wd, &direct[..dn])) == u32::MAX {
            dst[..dn + 1].copy_from_slice(&direct[..dn + 1]);
            dn
        } else {
            unique_name(dest, base, ext, &mut dst)
        };
        if n == 0 {
            failed += 1;
            continue;
        }
        let mut wsrc_b = [0u16; PATH_LEN + NAME_LEN + 2];
        let mut wdst_b = [0u16; PATH_LEN + NAME_LEN + 2];
        let moved =
            !do_copy && MoveFileW(wstr(&mut wsrc_b, &src[..sn]), wstr(&mut wdst_b, &dst[..n])) != 0;
        let ok = moved || copy_tree(&src[..sn], &dst[..n]);
        if !ok {
            failed += 1;
        }
    }
    if failed > 0 {
        notify(if do_copy {
            tr4(&[b"Certains fichiers n'ont pas pu \xEAtre copi\xE9s.", b"Some files couldn't be copied.", b"Algunos archivos no se pudieron copiar.", b"Einige Dateien konnten nicht kopiert werden."])
        } else {
            tr4(&[b"Certains fichiers n'ont pas pu \xEAtre d\xE9plac\xE9s.", b"Some files couldn't be moved.", b"Algunos archivos no se pudieron mover.", b"Einige Dateien konnten nicht verschoben werden."])
        });
    }
    reload(p);
    let t = tab();
    if t.dual {
        let other = &mut t.panes[1 - t.focus];
        reload(other);
    }
    cancel_file_drag();
}

pub unsafe fn paste_to(dest: &[u8]) {
    if NCLIP == 0 {
        return;
    }
    let mut failed = 0u32;
    for k in 0..NCLIP {
        let src = CLIP[k];
        if !safe_destination(src.get(), dest) {
            failed += 1;
            continue;
        }
        let name = last_component(src.get());
        let (base, ext) = match name.iter().rposition(|&c| c == b'.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name, &b""[..]),
        };
        let mut dst = [0u8; PATH_LEN + NAME_LEN + 2];
        let mut direct = [0u8; PATH_LEN + NAME_LEN + 2];
        let dn = join(&mut direct, dest, name);
        if dn == 0 {
            failed += 1;
            continue;
        }
        let mut wd = [0u16; PATH_LEN + NAME_LEN + 2];
        let n = if GetFileAttributesW(wstr(&mut wd, &direct[..dn])) == u32::MAX {
            dst[..dn + 1].copy_from_slice(&direct[..dn + 1]);
            dn
        } else {
            unique_name(dest, base, ext, &mut dst)
        };
        if n == 0 {
            failed += 1;
            continue;
        }
        let mut wsrc_b = [0u16; PATH_LEN + NAME_LEN + 2];
        let mut wdst_b = [0u16; PATH_LEN + NAME_LEN + 2];
        let moved =
            CLIP_CUT && MoveFileW(wstr(&mut wsrc_b, src.get()), wstr(&mut wdst_b, &dst[..n])) != 0;
        let ok = moved || copy_tree(src.get(), &dst[..n]);
        if !ok {
            failed += 1;
        }
    }
    if CLIP_CUT {
        NCLIP = 0;
    }
    if failed > 0 {
        notify(tr4(&[b"Collage refus\xE9 ou incomplet.", b"Paste refused or incomplete.", b"Pegado rechazado o incompleto.", b"Einf\xFCgen abgelehnt oder unvollst\xE4ndig."]));
    }
}

pub fn desktop_folder_path() -> Path {
    place_path(0)
}

pub unsafe fn drag_glyph() -> Glyph {
    let p = pane();
    if let Some(i) = first_sel(p) {
        glyph_of(p.entries[i].kind)
    } else {
        Glyph::File
    }
}

/// Chemins des éléments sélectionnés (glisser-déposer vers une application).
pub unsafe fn for_each_selected(f: &mut dyn FnMut(&[u8])) {
    let p = pane();
    if p.loc.is(HOME) || p.loc.is(PC) {
        return;
    }
    for v in 0..p.nview {
        let i = p.view[v] as usize;
        if !is_sel(p, i) {
            continue;
        }
        let mut src = [0u8; PATH_LEN + NAME_LEN + 2];
        let n = item_path(p, i, &mut src);
        if n > 0 {
            f(&src[..n]);
        }
    }
}

pub unsafe fn drag_count() -> usize {
    sel_count(pane())
}

/// Envoie la sélection dans la Corbeille (format freedesktop.org, partagé
/// avec les gestionnaires de fichiers Linux).
unsafe fn delete_selection() {
    let p = pane();
    if p.loc.is(HOME) || p.loc.is(PC) {
        return;
    }
    let in_trash = p.loc.is(TRASH);
    let mut w1 = [0u16; PATH_LEN + NAME_LEN + 2];
    CreateDirectoryW(
        wstr(&mut w1, b"C:\\.local\\share\\Trash"),
        core::ptr::null_mut(),
    );
    CreateDirectoryW(wstr(&mut w1, TRASH), core::ptr::null_mut());
    CreateDirectoryW(
        wstr(&mut w1, b"C:\\.local\\share\\Trash\\info"),
        core::ptr::null_mut(),
    );
    let mut failed = 0;
    for v in 0..p.nview {
        let i = p.view[v] as usize;
        if !is_sel(p, i) {
            continue;
        }
        let mut src = [0u8; PATH_LEN + NAME_LEN + 2];
        let sn = item_path(p, i, &mut src);
        if sn == 0 {
            failed += 1;
            continue;
        }
        let name = p.entries[i].name();
        if in_trash {
            let mut ws = [0u16; PATH_LEN + NAME_LEN + 2];
            wstr(&mut ws, &src[..sn]);
            let ok = if p.entries[i].dir {
                RemoveDirectoryW(ws.as_ptr()) != 0
            } else {
                DeleteFileW(ws.as_ptr()) != 0
            };
            if !ok {
                failed += 1;
            }
            continue;
        }
        let (base, ext) = match name.iter().rposition(|&c| c == b'.') {
            Some(k) if k > 0 && !p.entries[i].dir => (&name[..k], &name[k..]),
            _ => (name, &b""[..]),
        };
        let mut dst = [0u8; PATH_LEN + NAME_LEN + 2];
        let dn = unique_name(TRASH, base, ext, &mut dst);
        if dn == 0 {
            failed += 1;
            continue;
        }
        // Fichier .trashinfo : chemin d'origine et date de suppression.
        let trashed = &dst[TRASH.len() + 1..dn];
        let mut info = [0u8; PATH_LEN + NAME_LEN + 2];
        let mut nm = [0u8; NAME_LEN + 12];
        let tl = trashed.len();
        nm[..tl].copy_from_slice(&trashed[..tl]);
        nm[tl..tl + 10].copy_from_slice(b".trashinfo");
        if join(&mut info, b"C:\\.local\\share\\Trash\\info", &nm[..tl + 10]) == 0 {
            failed += 1;
            continue;
        }
        let mut wsrc2 = [0u16; PATH_LEN + NAME_LEN + 2];
        let mut wdst2 = [0u16; PATH_LEN + NAME_LEN + 2];
        if MoveFileW(wstr(&mut wsrc2, &src[..sn]), wstr(&mut wdst2, &dst[..dn])) == 0 {
            failed += 1;
            continue;
        }
        let mut winfo = [0u16; PATH_LEN + NAME_LEN + 2];
        let info_len = info.iter().position(|&c| c == 0).unwrap_or(info.len());
        let h = CreateFileW(
            wstr(&mut winfo, &info[..info_len]),
            0x4000_0000,
            0,
            core::ptr::null_mut(),
            2,
            0x80,
            core::ptr::null_mut(),
        );
        if h as isize != -1 {
            let mut body = [0u8; 700];
            let head = b"[Trash Info]\nPath=";
            let mut n = head.len();
            body[..n].copy_from_slice(head);
            let mut full = [0u8; 600];
            let fl = GetFullPathLinux(&src[..sn], &mut full);
            body[n..n + fl].copy_from_slice(&full[..fl]);
            n += fl;
            let mut st = SystemTime::default();
            GetLocalTime(&mut st);
            let d = b"\nDeletionDate=";
            body[n..n + d.len()].copy_from_slice(d);
            n += d.len();
            n = write_num(&mut body, n, st.year as u64);
            for (sep, v) in [
                (b'-', st.month),
                (b'-', st.day),
                (b'T', st.hour),
                (b':', st.minute),
                (b':', st.second),
            ] {
                body[n] = sep;
                body[n + 1] = b'0' + (v / 10) as u8;
                body[n + 2] = b'0' + (v % 10) as u8;
                n += 3;
            }
            body[n] = b'\n';
            n += 1;
            let mut written = 0u32;
            let ok = WriteFile(
                h,
                body.as_ptr(),
                n as u32,
                &mut written,
                core::ptr::null_mut(),
            ) != 0
                && written as usize == n;
            CloseHandle(h);
            if !ok {
                let mut wdi = [0u16; PATH_LEN + NAME_LEN + 2];
                DeleteFileW(wstr(&mut wdi, &info[..info_len]));
                let mut wd2 = [0u16; PATH_LEN + NAME_LEN + 2];
                let mut ws2 = [0u16; PATH_LEN + NAME_LEN + 2];
                if MoveFileW(wstr(&mut wd2, &dst[..dn]), wstr(&mut ws2, &src[..sn])) == 0 {
                    notify(tr4(&[b"Corbeille incompl\xE8te : l'\xE9l\xE9ment reste accessible dans la Corbeille.", b"Recycle Bin incomplete: the item is still available in the Recycle Bin.", b"Papelera incompleta: el elemento sigue disponible en la Papelera.", b"Papierkorb unvollst\xE4ndig: Das Element bleibt im Papierkorb verf\xFCgbar."]));
                }
                failed += 1;
            }
        } else {
            let mut wd3 = [0u16; PATH_LEN + NAME_LEN + 2];
            let mut ws3 = [0u16; PATH_LEN + NAME_LEN + 2];
            if MoveFileW(wstr(&mut wd3, &dst[..dn]), wstr(&mut ws3, &src[..sn])) == 0 {
                notify(tr4(&[b"Corbeille incompl\xE8te : l'\xE9l\xE9ment reste accessible dans la Corbeille.", b"Recycle Bin incomplete: the item is still available in the Recycle Bin.", b"Papelera incompleta: el elemento sigue disponible en la Papelera.", b"Papierkorb unvollst\xE4ndig: Das Element bleibt im Papierkorb verf\xFCgbar."]));
            }
            failed += 1;
        }
    }
    reload(p);
    if failed > 0 {
        notify(tr4(&[b"Certains \xE9l\xE9ments n'ont pas pu \xEAtre supprim\xE9s.", b"Some items couldn't be deleted.", b"Algunos elementos no se pudieron eliminar.", b"Einige Elemente konnten nicht gel\xF6scht werden."]));
    } else if !in_trash {
        notify(tr4(&[b"\xC9l\xE9ments d\xE9plac\xE9s dans la Corbeille.", b"Items moved to the Recycle Bin.", b"Elementos movidos a la Papelera.", b"Elemente in den Papierkorb verschoben."]));
    }
}

/// Chemin Linux (UTF-8 approché, encodé en %XX pour la Corbeille) d'un chemin Windows.
#[allow(non_snake_case)]
fn GetFullPathLinux(win: &[u8], out: &mut [u8; 600]) -> usize {
    let mut n = 0;
    let mut push = |c: u8, n: &mut usize| {
        if *n < out.len() {
            out[*n] = c;
            *n += 1;
        }
    };
    let rest: &[u8] = if win.len() >= 2 && win[1] == b':' {
        &win[2..]
    } else {
        win
    };
    if win.first().map(|c| c.to_ascii_uppercase()) == Some(b'C') {
        let mut home = [0i8; 200];
        let hn =
            unsafe { GetEnvironmentVariableA(c"HOME".as_ptr(), home.as_mut_ptr(), 200) } as usize;
        for &c in home[..hn.min(199)].iter() {
            push(c as u8, &mut n);
        }
    }
    for &c in rest {
        let c = if c == b'\\' { b'/' } else { c };
        if c.is_ascii_alphanumeric() || matches!(c, b'/' | b'-' | b'_' | b'.' | b'~') {
            push(c, &mut n);
        } else {
            // Windows-1252 → UTF-8 pour les accents courants, puis %XX.
            let mut utf = [0u8; 3];
            let len = if c < 0x80 {
                utf[0] = c;
                1
            } else if c >= 0xA0 {
                utf[0] = 0xC0 | (c >> 6);
                utf[1] = 0x80 | (c & 0x3F);
                2
            } else {
                utf[0] = b'_';
                1
            };
            for &u in &utf[..len] {
                const H: &[u8] = b"0123456789ABCDEF";
                push(b'%', &mut n);
                push(H[(u >> 4) as usize], &mut n);
                push(H[(u & 15) as usize], &mut n);
            }
        }
    }
    n
}

unsafe fn run_item(it: Item, geo: &Geo) {
    let p = pane();
    match it {
        Item::View(v) => VIEW = v,
        Item::Compact => COMPACT = !COMPACT,
        Item::Hidden => {
            SHOW_HIDDEN = !SHOW_HIDDEN;
            save_folder_options();
            for t in TABS[..NTABS].iter_mut() {
                for pn in t.panes.iter_mut() {
                    rebuild_view(pn);
                }
            }
        }
        Item::Ext => {
            SHOW_EXT = !SHOW_EXT;
            save_folder_options();
        }
        Item::Nav => SHOW_NAV = !SHOW_NAV,
        Item::Sort(k) => {
            SORT = k;
            resort_all();
        }
        Item::Asc => {
            SORT_DESC = false;
            resort_all();
        }
        Item::Desc => {
            SORT_DESC = true;
            resort_all();
        }
        Item::NewFolder => new_item(true),
        Item::NewText => new_item(false),
        Item::Open => {
            if let Some(i) = first_sel(p) {
                open_item(p, i, false)
            }
        }
        Item::OpenTab => {
            if let Some(i) = first_sel(p) {
                if p.entries[i].dir {
                    open_item(p, i, true)
                }
            }
        }
        Item::ExeSettings => {
            if let Some(i) = first_sel(p) {
                let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
                let n = item_path(p, i, &mut path);
                if n > 0 && ends_ci(&path[..n], b".exe") {
                    let mut wide = [0u16; PATH_LEN + NAME_LEN + 2];
                    wstr(&mut wide, &path[..n]);
                    crate::settings::open_exe_settings(&wide);
                }
            }
        }
        Item::Cut => copy_selection(true),
        Item::Copy => copy_selection(false),
        Item::Paste => paste(),
        Item::Rename => {
            if let Some(i) = first_sel(p) {
                start_rename(i)
            }
        }
        Item::Delete => delete_selection(),
        Item::Refresh => reload(p),
        Item::Terminal => open_terminal(),
        Item::CopyPath => {
            if let Some(i) = first_sel(p) {
                let mut b = [0u8; PATH_LEN + NAME_LEN + 2];
                let n = item_path(p, i, &mut b);
                ADDRESS.set(&b[..n]);
                notify(tr4(&[b"Chemin affich\xE9 dans la barre d'adresse.", b"Path shown in the address bar.", b"Ruta mostrada en la barra de direcciones.", b"Pfad in der Adressleiste angezeigt."]));
                FOCUS = Focus::Address;
            }
        }
        Item::ExtractAll | Item::ExtractHere | Item::Mount => {
            if let Some(i) = first_sel(p) {
                let mut b = [0u8; PATH_LEN + NAME_LEN + 2];
                let n = item_path(p, i, &mut b);
                if n > 0 {
                    match it {
                        Item::Mount => mount_image(&b[..n]),
                        _ => extract_archive(&b[..n], it == Item::ExtractHere, None),
                    }
                }
            }
        }
        Item::Eject => {
            if let Some(l) = first_sel(p).and_then(|i| drive_letter_in(p.entries[i].name())) {
                eject_drive(l);
            }
        }
        Item::CompressZip => compress_selection(b"zip", None),
        Item::Compress7z => compress_selection(b"7z", None),
        Item::CompressTarGz => compress_selection(b"tar.gz", None),
        Item::CompressZipPw => {
            let loc = p.loc;
            JOB_ARG = loc;
            ask_password(JobKind::Create, false);
        }
        Item::ConnectNet => crate::settings::open_page(crate::settings::PAGE_NETDRIVES),
        Item::Sep => {}
    }
    let _ = geo;
}

unsafe fn resort_all() {
    for t in TABS[..NTABS].iter_mut() {
        for pn in t.panes.iter_mut() {
            rebuild_view(pn);
        }
    }
}

unsafe fn open_terminal() {
    let p = pane();
    let dir: &[u8] = if p.loc.is(HOME) || p.loc.is(PC) {
        b"C:\\"
    } else {
        p.loc.get()
    };
    crate::terminal::init_at(dir);
    crate::wm::open(crate::wm::APP_TERMINAL);
}

unsafe fn run_cmd(cmd: Cmd, geo: &Geo) {
    MENU = match (cmd, MENU) {
        (Cmd::View, Menu::View) | (Cmd::Sort, Menu::Sort) | (Cmd::New, Menu::New) => Menu::None,
        (Cmd::View, _) => Menu::View,
        (Cmd::Sort, _) => Menu::Sort,
        (Cmd::New, _) => Menu::New,
        _ => Menu::None,
    };
    match cmd {
        Cmd::Cut => copy_selection(true),
        Cmd::Copy => copy_selection(false),
        Cmd::Paste => paste(),
        Cmd::Rename => {
            if let Some(i) = first_sel(pane()) {
                start_rename(i)
            }
        }
        Cmd::Delete => delete_selection(),
        Cmd::Dual => toggle_dual(),
        Cmd::Nav => SHOW_NAV = !SHOW_NAV,
        Cmd::Terminal => open_terminal(),
        _ => {}
    }
    let _ = geo;
}

// ---------- Archives, images disque et lecteurs dynamiques ----------
//
// Les archives s'ouvrent comme des dossiers : le runtime les déploie dans un
// cache puis résout les chemins `...\photos.zip\...` vers ce cache. Après
// chaque modification (rechargement du dossier), un « commit » reconstruit
// l'archive. Les images disque reçoivent une lettre de lecteur.

const MAX_DRIVES: usize = 8;
const LABEL_LEN: usize = 72;

#[derive(Clone, Copy)]
struct DynDrive {
    letter: u8,
    network: bool,
    online: bool,
    label: [u8; LABEL_LEN],
    len: usize,
}
const EMPTY_DRIVE: DynDrive = DynDrive {
    letter: 0,
    network: false,
    online: true,
    label: [0; LABEL_LEN],
    len: 0,
};
static mut DRIVES: [DynDrive; MAX_DRIVES] = [EMPTY_DRIVE; MAX_DRIVES];
static mut NDRIVES: usize = 0;

/// Langue de l'interface (0 fr, 1 en, 2 es, 3 de), réglée dans les Paramètres.
fn lang() -> usize {
    unsafe { crate::locale::CURRENT_LANG as usize }
}

/// Chaîne localisée : français, anglais, espagnol, allemand (Windows-1252).
fn t4(s: [&'static [u8]; 4]) -> &'static [u8] {
    s[lang()]
}

fn ansi_from_wide(w: &[u16], out: &mut [u8]) -> usize {
    let mut n = 0;
    for &c in w {
        if c == 0 || n == out.len() {
            break;
        }
        out[n] = if c < 256 { c as u8 } else { b'?' };
        n += 1;
    }
    n
}

unsafe fn refresh_drives() {
    let mut buf = [0u16; 4096];
    NDRIVES = 0;
    let n = WinluxMountList(buf.as_mut_ptr(), buf.len() as u32);
    if n <= 0 {
        return;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    for line in buf[..len].split(|&c| c == b'\n' as u16) {
        if NDRIVES == MAX_DRIVES {
            break;
        }
        let mut cols = line.split(|&c| c == b'\t' as u16);
        let (Some(l), Some(kind), Some(label), Some(_ro), Some(state)) = (
            cols.next(),
            cols.next(),
            cols.next(),
            cols.next(),
            cols.next(),
        ) else {
            continue;
        };
        let Some(&letter) = l.first() else {
            continue;
        };
        let d = &mut DRIVES[NDRIVES];
        d.letter = letter as u8;
        d.network = kind.iter().map(|&c| c as u8).eq(b"network".iter().copied());
        d.online = state.is_empty() || state.iter().map(|&c| c as u8).eq(b"ok".iter().copied());
        let mut n = ansi_from_wide(label, &mut d.label[..LABEL_LEN - 20]);
        if n == 0 {
            let def = if d.network {
                t4([
                    b"Lecteur r\xE9seau",
                    b"Network drive",
                    b"Unidad de red",
                    b"Netzlaufwerk",
                ])
            } else {
                t4([
                    b"Image disque",
                    b"Disc image",
                    b"Imagen de disco",
                    b"Disk-Image",
                ])
            };
            d.label[..def.len()].copy_from_slice(def);
            n = def.len();
        }
        for &c in b" (" {
            d.label[n] = c;
            n += 1;
        }
        d.label[n] = d.letter;
        d.label[n + 1] = b':';
        d.label[n + 2] = b')';
        n += 3;
        if !d.online {
            let off = t4([
                b" (hors ligne)",
                b" (offline)",
                b" (sin conexi\xF3n)",
                b" (offline)",
            ]);
            d.label[n..n + off.len()].copy_from_slice(off);
            n += off.len();
        }
        d.len = n;
        NDRIVES += 1;
    }
}

unsafe fn drive_label(i: usize) -> &'static [u8] {
    let d = &*core::ptr::addr_of!(DRIVES[i]);
    &d.label[..d.len]
}

/// Lettre d'un lecteur dynamique d'après son libellé « Nom (E:) ».
fn drive_letter_in(name: &[u8]) -> Option<u8> {
    name.windows(4)
        .rev()
        .find(|w| w[0] == b'(' && w[1].is_ascii_uppercase() && w[2] == b':' && w[3] == b')')
        .map(|w| w[1])
}

unsafe fn find_drive(letter: u8) -> Option<DynDrive> {
    (0..NDRIVES).map(|i| DRIVES[i]).find(|d| d.letter == letter)
}

/// Le chemin traverse-t-il une archive (pour le commit après modification) ?
fn path_in_archive(path: &[u8]) -> bool {
    path.split(|&c| c == b'\\')
        .skip(1)
        .any(|c| kind_of(c, false) == Kind::Archive)
}

const CAN_READ: u32 = 1;
const CAN_WRITE: u32 = 2;
const CAN_MOUNT: u32 = 4;
const IS_OPEN: u32 = 16;

unsafe fn support_of(path: &[u8]) -> u32 {
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let p = wstr(&mut w, path);
    if p.is_null() {
        return 0;
    }
    WinluxArchiveSupport(p)
}

#[derive(Clone, Copy, PartialEq)]
enum JobKind {
    Open,
    ExtractAll,
    ExtractHere,
    Create,
    Commit,
    Mount,
    Eject,
}

static mut JOB: u32 = 0;
static mut JOB_KIND: JobKind = JobKind::Open;
static mut JOB_PROGRESS: u32 = 0;
static mut JOB_START: u32 = 0;
/// Archive, image ou destination concernée par la tâche en cours.
static mut JOB_ARG: Path = EMPTY_PATH;
static mut JOB_DEST: Path = EMPTY_PATH;
static mut JOB_WITH_PASSWORD: bool = false;
static mut COMMIT_PENDING: Path = EMPTY_PATH;
/// Possibilités (WinluxArchiveSupport) de l'élément sous le menu contextuel.
static mut SEL_FLAGS: u32 = 0;
/// Formats de création disponibles (WinluxArchiveFormats).
static mut CREATE_FORMATS: u32 = u32::MAX;

/// Boîte de saisie du mot de passe.
static mut PASSWORD_OPEN: bool = false;
static mut PASSWORD: Field = EMPTY_FIELD;
static mut PASSWORD_ERROR: bool = false;
/// Action à relancer avec le mot de passe saisi.
static mut PASSWORD_FOR: JobKind = JobKind::Open;

unsafe fn start_job(kind: JobKind, id: u32, arg: &[u8]) {
    if id == 0 {
        notify(t4([
            b"Op\xE9ration impossible sur cet \xE9l\xE9ment.",
            b"This item cannot be processed.",
            b"No se puede procesar este elemento.",
            b"Dieses Element kann nicht verarbeitet werden.",
        ]));
        return;
    }
    JOB = id;
    JOB_KIND = kind;
    JOB_PROGRESS = 0;
    JOB_START = GetTickCount();
    JOB_ARG.set(arg);
}

fn busy_message() -> &'static [u8] {
    t4([
        b"Une op\xE9ration est d\xE9j\xE0 en cours.",
        b"Another operation is in progress.",
        b"Ya hay una operaci\xF3n en curso.",
        b"Es l\xE4uft bereits ein Vorgang.",
    ])
}

/// Ouvre une archive (déploiement en arrière-plan), puis y entre.
unsafe fn open_archive(path: &[u8], password: Option<&[u8]>) {
    if JOB != 0 {
        notify(busy_message());
        return;
    }
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut pw = [0u16; PATH_LEN + NAME_LEN + 2];
    let pp = match password {
        Some(s) => {
            for (i, &c) in s.iter().enumerate() {
                pw[i] = c as u16;
            }
            pw.as_ptr()
        }
        None => core::ptr::null(),
    };
    JOB_WITH_PASSWORD = password.is_some();
    start_job(
        JobKind::Open,
        WinluxArchiveOpen(wstr(&mut w, path), pp),
        path,
    );
}

unsafe fn extract_archive(path: &[u8], here: bool, password: Option<&[u8]>) {
    if JOB != 0 {
        notify(busy_message());
        return;
    }
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut d = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut pw = [0u16; PATH_LEN + NAME_LEN + 2];
    let dest = if here {
        match parent(path) {
            Some(dir) => wstr(&mut d, dir),
            None => core::ptr::null(),
        }
    } else {
        core::ptr::null()
    };
    let pp = match password {
        Some(s) => {
            for (i, &c) in s.iter().enumerate() {
                pw[i] = c as u16;
            }
            pw.as_ptr()
        }
        None => core::ptr::null(),
    };
    JOB_WITH_PASSWORD = password.is_some();
    let kind = if here {
        JobKind::ExtractHere
    } else {
        JobKind::ExtractAll
    };
    start_job(
        kind,
        WinluxArchiveExtract(wstr(&mut w, path), dest, pp),
        path,
    );
}

unsafe fn mount_image(path: &[u8]) {
    if JOB != 0 {
        notify(busy_message());
        return;
    }
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    start_job(JobKind::Mount, WinluxImageMount(wstr(&mut w, path)), path);
}

unsafe fn eject_drive(letter: u8) {
    if let Some(d) = find_drive(letter) {
        if d.network {
            // Lecteurs réseau : gérés par l'outil winlux-drives.
            let mut args = [0u16; 16];
            for (i, &c) in b"disconnect X".iter().enumerate() {
                args[i] = if c == b'X' { letter as u16 } else { c as u16 };
            }
            run_drives_tool(&args);
            refresh_drives();
            for t in TABS[..NTABS].iter_mut() {
                for pn in t.panes.iter_mut() {
                    if pn.loc.is(PC) {
                        reload(pn);
                    }
                }
            }
            return;
        }
    }
    if JOB != 0 {
        notify(busy_message());
        return;
    }
    start_job(
        JobKind::Eject,
        WinluxImageEject(letter as u32),
        &[letter, b':', b'\\'],
    );
}

unsafe fn run_drives_tool(args: &[u16]) {
    let open: [u16; 5] = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
    let mut prog = [0u16; 16];
    for (i, &c) in b"winlux-drives".iter().enumerate() {
        prog[i] = c as u16;
    }
    let r = ShellExecuteW(
        core::ptr::null_mut(),
        open.as_ptr(),
        prog.as_ptr(),
        args.as_ptr(),
        core::ptr::null(),
        0,
    );
    if (r as isize) <= 32 {
        notify(t4([
            b"Gestionnaire de lecteurs r\xE9seau indisponible.",
            b"Network drive manager unavailable.",
            b"Gestor de unidades de red no disponible.",
            b"Netzlaufwerk-Verwaltung nicht verf\xFCgbar.",
        ]));
    }
}

/// Compresse la sélection : `ext` = « zip », « 7z », « tar.gz »…
unsafe fn compress_selection(ext: &[u8], password: Option<&[u8]>) {
    if JOB != 0 {
        notify(busy_message());
        return;
    }
    let p = pane();
    if p.loc.is(HOME) || p.loc.is(PC) {
        return;
    }
    let mut sources = [0u16; 8192];
    let mut at = 0;
    let mut count = 0;
    let mut first = [0u8; NAME_LEN];
    let mut first_len = 0;
    for v in 0..p.nview {
        let i = p.view[v] as usize;
        if !is_sel(p, i) {
            continue;
        }
        let mut path = [0u8; PATH_LEN + NAME_LEN + 2];
        let n = item_path(p, i, &mut path);
        if n == 0 || at + n + 2 >= sources.len() {
            continue;
        }
        for &c in &path[..n] {
            sources[at] = c as u16;
            at += 1;
        }
        sources[at] = 0;
        at += 1;
        if count == 0 {
            let name = p.entries[i].name();
            // Nom de l'archive : celui de l'élément, sans son extension.
            let stem = if p.entries[i].dir {
                name
            } else {
                match name.iter().rposition(|&c| c == b'.') {
                    Some(k) if k > 0 => &name[..k],
                    _ => name,
                }
            };
            first_len = stem.len().min(NAME_LEN - 16);
            first[..first_len].copy_from_slice(&stem[..first_len]);
        }
        count += 1;
    }
    if count == 0 {
        return;
    }
    sources[at] = 0;
    let base: &[u8] = if count == 1 {
        &first[..first_len]
    } else {
        last_component(p.loc.get())
    };
    let mut name = [0u8; NAME_LEN];
    let bl = base.len().min(NAME_LEN - 16);
    name[..bl].copy_from_slice(&base[..bl]);
    name[bl] = b'.';
    name[bl + 1..bl + 1 + ext.len()].copy_from_slice(ext);
    let mut out = [0u8; PATH_LEN + NAME_LEN + 2];
    let on = join(&mut out, p.loc.get(), &name[..bl + 1 + ext.len()]);
    if on == 0 {
        notify(tr4(&[b"Chemin trop long pour cette version d'Explorer.", b"Path too long for this version of Explorer.", b"Ruta demasiado larga para esta versi\xF3n del Explorador.", b"Pfad zu lang f\xFCr diese Version des Explorers."]));
        return;
    }
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut pw = [0u16; PATH_LEN + NAME_LEN + 2];
    let pp = match password {
        Some(s) => {
            for (i, &c) in s.iter().enumerate() {
                pw[i] = c as u16;
            }
            pw.as_ptr()
        }
        None => core::ptr::null(),
    };
    let loc = p.loc;
    JOB_DEST = loc;
    start_job(
        JobKind::Create,
        WinluxArchiveCreate(wstr(&mut w, &out[..on]), sources.as_ptr(), pp),
        &out[..on],
    );
}

/// Après une modification dans une archive ouverte : on la reconstruit.
unsafe fn maybe_commit(loc: &[u8]) {
    if !path_in_archive(loc) {
        return;
    }
    if JOB != 0 {
        COMMIT_PENDING.set(loc);
        return;
    }
    let mut w = [0u16; PATH_LEN + NAME_LEN + 2];
    let id = WinluxArchiveCommit(wstr(&mut w, loc));
    if id != 0 {
        start_job(JobKind::Commit, id, loc);
    }
}

fn job_label(kind: JobKind) -> &'static [u8] {
    match kind {
        JobKind::Open => t4([
            b"Ouverture de l'archive...",
            b"Opening archive...",
            b"Abriendo el archivo...",
            b"Archiv wird ge\xF6ffnet...",
        ]),
        JobKind::ExtractAll | JobKind::ExtractHere => t4([
            b"Extraction...",
            b"Extracting...",
            b"Extrayendo...",
            b"Entpacken...",
        ]),
        JobKind::Create => t4([
            b"Compression...",
            b"Compressing...",
            b"Comprimiendo...",
            b"Komprimieren...",
        ]),
        JobKind::Commit => t4([
            b"Enregistrement de l'archive...",
            b"Saving archive...",
            b"Guardando el archivo...",
            b"Archiv wird gespeichert...",
        ]),
        JobKind::Mount => t4([
            b"Montage de l'image...",
            b"Mounting image...",
            b"Montando la imagen...",
            b"Image wird eingebunden...",
        ]),
        JobKind::Eject => t4([
            b"\xC9jection...",
            b"Ejecting...",
            b"Expulsando...",
            b"Auswerfen...",
        ]),
    }
}

unsafe fn reload_all() {
    refresh_drives();
    for t in TABS[..NTABS].iter_mut() {
        for pn in t.panes.iter_mut() {
            if !pn.loc.get().is_empty() {
                reload(pn);
            }
        }
    }
}

unsafe fn notify_with(prefix: &[u8], msg: &[u8]) {
    let mut b = [0u8; 200];
    let n = prefix.len().min(120);
    b[..n].copy_from_slice(&prefix[..n]);
    let m = msg.len().min(b.len() - n);
    b[n..n + m].copy_from_slice(&msg[..m]);
    notify(&b[..n + m]);
}

unsafe fn ask_password(kind: JobKind, wrong: bool) {
    PASSWORD_OPEN = true;
    PASSWORD_ERROR = wrong;
    PASSWORD_FOR = kind;
    PASSWORD.len = 0;
    MENU = Menu::None;
}

/// Fin d'une tâche de fond : on enchaîne l'action visible pour l'utilisateur.
unsafe fn finish_job(status: u32, msg: &[u8]) {
    let kind = JOB_KIND;
    let arg = JOB_ARG;
    JOB = 0;
    match status {
        1 => match kind {
            JobKind::Open => navigate(arg.get()),
            JobKind::ExtractAll => {
                reload_all();
                navigate(msg);
            }
            JobKind::ExtractHere => {
                reload_all();
                notify(t4([
                    b"Extraction termin\xE9e.",
                    b"Extraction complete.",
                    b"Extracci\xF3n completada.",
                    b"Entpacken abgeschlossen.",
                ]));
            }
            JobKind::Create => {
                reload_all();
                let created = last_component(msg);
                let mut nb = [0u8; NAME_LEN];
                let nl = created.len().min(NAME_LEN);
                nb[..nl].copy_from_slice(&created[..nl]);
                let p = pane();
                p.sel = [0; MAX_ENTRIES / 64];
                let _ = select_named(p, &nb[..nl]);
                notify_with(
                    t4([
                        b"Archive cr\xE9\xE9e : ",
                        b"Archive created: ",
                        b"Archivo creado: ",
                        b"Archiv erstellt: ",
                    ]),
                    &nb[..nl],
                );
            }
            JobKind::Commit => {}
            JobKind::Mount => {
                refresh_drives();
                if msg.len() >= 2 && msg[1] == b':' {
                    navigate(&[msg[0], b':', b'\\']);
                    notify_with(
                        t4([
                            b"Image mont\xE9e sur ",
                            b"Image mounted as ",
                            b"Imagen montada en ",
                            b"Image eingebunden als ",
                        ]),
                        msg,
                    );
                }
            }
            JobKind::Eject => {
                refresh_drives();
                let letter = arg.get().first().copied().unwrap_or(0);
                for t in TABS[..NTABS].iter_mut() {
                    for pn in t.panes.iter_mut() {
                        if pn.loc.get().first() == Some(&letter)
                            && pn.loc.get().get(1) == Some(&b':')
                        {
                            navigate_pane(pn, PC, false);
                        }
                    }
                }
                reload_all();
                notify(t4([
                    b"Image \xE9ject\xE9e : vous pouvez la d\xE9placer ou la supprimer.",
                    b"Image ejected: it is now safe to move or delete it.",
                    b"Imagen expulsada: ya puede moverla o eliminarla.",
                    b"Image ausgeworfen: sie kann jetzt verschoben werden.",
                ]));
            }
        },
        3 | 4
            if matches!(
                kind,
                JobKind::Open | JobKind::ExtractAll | JobKind::ExtractHere
            ) =>
        {
            // On garde l'archive visée pour relancer avec le mot de passe.
            JOB_ARG = arg;
            ask_password(kind, status == 4 || JOB_WITH_PASSWORD);
        }
        5 => {
            reload_all();
            notify(t4([
                b"Archive en lecture seule : la modification n'a pas \xE9t\xE9 enregistr\xE9e.",
                b"Read-only archive: the change was not saved.",
                b"Archivo de solo lectura: el cambio no se guard\xF3.",
                b"Schreibgesch\xFCtztes Archiv: \xC4nderung nicht gespeichert.",
            ]));
        }
        6 => notify_with(
            t4([
                b"Non pris en charge : ",
                b"Not supported: ",
                b"No compatible: ",
                b"Nicht unterst\xFCtzt: ",
            ]),
            msg,
        ),
        7 => notify(t4([
            b"Op\xE9ration annul\xE9e.",
            b"Operation cancelled.",
            b"Operaci\xF3n cancelada.",
            b"Vorgang abgebrochen.",
        ])),
        _ => notify_with(
            t4([b"\xC9chec : ", b"Failed: ", b"Error: ", b"Fehler: "]),
            msg,
        ),
    }
    if JOB == 0 && !COMMIT_PENDING.get().is_empty() {
        let pending = COMMIT_PENDING;
        COMMIT_PENDING.len = 0;
        maybe_commit(pending.get());
    }
}

/// Suivi de la tâche en cours ; renvoie vrai tant qu'il faut redessiner.
unsafe fn poll_job() -> bool {
    if JOB == 0 {
        return false;
    }
    let mut prog = 0u32;
    let mut msg = [0u16; 512];
    let st = WinluxJobPoll(JOB, &mut prog, msg.as_mut_ptr(), msg.len() as u32);
    JOB_PROGRESS = prog;
    if st == 0 {
        return true;
    }
    WinluxJobClose(JOB);
    let mut m = [0u8; 512];
    let n = ansi_from_wide(&msg, &mut m);
    finish_job(st, &m[..n]);
    true
}

unsafe fn cancel_job() {
    if JOB != 0 {
        WinluxJobCancel(JOB);
    }
}

/// Barre de progression dans la barre d'état (après 300 ms d'activité).
unsafe fn draw_job(g: &mut Gfx, s: &Rect) {
    if JOB == 0 || GetTickCount().wrapping_sub(JOB_START) < 300 {
        return;
    }
    let p = pal();
    let label = job_label(JOB_KIND);
    let right = s.right - 76;
    let bar = rect(right - 160, s.top + 11, right, s.top + 17);
    let lw = text_width(label, 11);
    text(
        g.hdc,
        g.f.small,
        p.text2,
        bar.left - lw - 10,
        s.top + 8,
        label,
    );
    round_rect(
        g.hdc, g.b, bar.left, bar.top, bar.right, bar.bottom, 3, p.border,
    );
    let w = (bar.right - bar.left) * JOB_PROGRESS.min(1000) as i32 / 1000;
    if w > 0 {
        round_rect(
            g.hdc,
            g.b,
            bar.left,
            bar.top,
            bar.left + w.max(6),
            bar.bottom,
            3,
            p.accent,
        );
    }
}

fn password_box(f: &Frame) -> Rect {
    let (w, h) = (420, 196);
    let cx = (f.client.left + f.client.right) / 2;
    let cy = (f.client.top + f.client.bottom) / 2;
    rect(cx - w / 2, cy - h / 2, cx + w / 2, cy + h / 2)
}

fn password_buttons(r: &Rect) -> (Rect, Rect) {
    let ok = rect(r.right - 216, r.bottom - 48, r.right - 116, r.bottom - 16);
    let cancel = rect(r.right - 108, r.bottom - 48, r.right - 16, r.bottom - 16);
    (ok, cancel)
}

unsafe fn draw_password(g: &mut Gfx, f: &Frame) {
    if !PASSWORD_OPEN {
        return;
    }
    let p = pal();
    let r = password_box(f);
    round_rect(
        g.hdc,
        g.b,
        r.left - 1,
        r.top - 1,
        r.right + 1,
        r.bottom + 1,
        9,
        p.border,
    );
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 8, p.surface);
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        r.left + 20,
        r.top + 18,
        t4([
            b"Archive prot\xE9g\xE9e par mot de passe",
            b"Password-protected archive",
            b"Archivo protegido con contrase\xF1a",
            b"Kennwortgesch\xFCtztes Archiv",
        ]),
    );
    text_fit(
        g.hdc,
        g.f.small,
        11,
        p.text2,
        r.left + 20,
        r.top + 44,
        last_component(JOB_ARG.get()),
        r.right - r.left - 40,
    );
    let field = rect(r.left + 20, r.top + 70, r.right - 20, r.top + 102);
    round_rect(
        g.hdc,
        g.b,
        field.left,
        field.top,
        field.right,
        field.bottom,
        4,
        p.accent,
    );
    round_rect(
        g.hdc,
        g.b,
        field.left + 1,
        field.top + 1,
        field.right - 1,
        field.bottom - 2,
        4,
        p.window,
    );
    let mut dots = [b'*'; 64];
    let n = PASSWORD.len.min(dots.len());
    let caret_x = field.left + 10 + text_width(&dots[..n], 12);
    dots[n] = b' ';
    text(
        g.hdc,
        g.f.caption,
        p.text,
        field.left + 10,
        field.top + 8,
        &dots[..n],
    );
    if GetTickCount() / 500 % 2 == 0 {
        fill(
            g.hdc,
            g.b,
            caret_x,
            field.top + 7,
            caret_x + 1,
            field.bottom - 8,
            p.text,
        );
    }
    if PASSWORD_ERROR {
        text(
            g.hdc,
            g.f.small,
            rgb(0xD1, 0x34, 0x38),
            r.left + 20,
            r.top + 110,
            t4([
                b"Mot de passe incorrect, r\xE9essayez.",
                b"Wrong password, try again.",
                b"Contrase\xF1a incorrecta, int\xE9ntelo de nuevo.",
                b"Falsches Kennwort, bitte erneut versuchen.",
            ]),
        );
    }
    let (ok, cancel) = password_buttons(&r);
    round_rect(
        g.hdc, g.b, ok.left, ok.top, ok.right, ok.bottom, 4, p.accent,
    );
    let okl: &[u8] = b"OK";
    text(
        g.hdc,
        g.f.caption,
        rgb(0xFF, 0xFF, 0xFF),
        (ok.left + ok.right - text_width(okl, 12)) / 2,
        ok.top + 8,
        okl,
    );
    let bg = if hovered(&cancel) { p.hover } else { p.window };
    round_rect(
        g.hdc,
        g.b,
        cancel.left,
        cancel.top,
        cancel.right,
        cancel.bottom,
        4,
        p.border,
    );
    round_rect(
        g.hdc,
        g.b,
        cancel.left + 1,
        cancel.top + 1,
        cancel.right - 1,
        cancel.bottom - 1,
        4,
        bg,
    );
    let cl = t4([b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"]);
    text(
        g.hdc,
        g.f.caption,
        p.text,
        (cancel.left + cancel.right - text_width(cl, 12)) / 2,
        cancel.top + 8,
        cl,
    );
}

unsafe fn password_submit() {
    PASSWORD_OPEN = false;
    let arg = JOB_ARG;
    let pw = PASSWORD;
    match PASSWORD_FOR {
        JobKind::Open => open_archive(arg.get(), Some(pw.get())),
        JobKind::ExtractAll => extract_archive(arg.get(), false, Some(pw.get())),
        JobKind::ExtractHere => extract_archive(arg.get(), true, Some(pw.get())),
        JobKind::Create => compress_selection(b"zip", Some(pw.get())),
        _ => {}
    }
    PASSWORD.buf.fill(0);
    PASSWORD.len = 0;
}

unsafe fn password_cancel() {
    PASSWORD_OPEN = false;
    PASSWORD.buf.fill(0);
    PASSWORD.len = 0;
}

/// Clavier pendant la saisie du mot de passe (boîte modale).
unsafe fn password_key(vk: u32, ch: u16) -> bool {
    match ch {
        13 => password_submit(),
        27 => password_cancel(),
        8 => PASSWORD.pop(),
        c if c >= 32 => match ansi_char(c) {
            Some(a) => PASSWORD.push(a),
            None => notify(tr4(&[b"Caract\xE8re non pris en charge.", b"Unsupported character.", b"Car\xE1cter no admitido.", b"Nicht unterst\xFCtztes Zeichen."])),
        },
        _ => {
            let _ = vk;
        }
    }
    true
}

unsafe fn password_mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if ev != Mouse::Down {
        return true;
    }
    let r = password_box(f);
    let (ok, cancel) = password_buttons(&r);
    if inside(&ok, x, y) {
        password_submit();
    } else if inside(&cancel, x, y) {
        password_cancel();
    }
    true
}

/// Pastille de format (ZIP, 7Z, ISO, VHDX…) sur les grandes icônes.
unsafe fn format_badge(g: &mut Gfx, e: &Entry, x: i32, y: i32, s: i32) {
    if s < 40 || e.dir || !matches!(e.kind, Kind::Archive | Kind::DiskImage) {
        return;
    }
    let ext = extension(e.name());
    if ext.is_empty() || ext.len() > 5 {
        return;
    }
    let mut up = [0u8; 5];
    for (i, &c) in ext.iter().enumerate() {
        up[i] = c.to_ascii_uppercase();
    }
    let label = &up[..ext.len()];
    let color = match label {
        b"ZIP" => rgb(0xE0, 0x9E, 0x2A),
        b"7Z" => rgb(0x2E, 0x7D, 0x32),
        b"RAR" => rgb(0x8E, 0x24, 0xAA),
        b"ISO" | b"IMG" | b"DMG" => rgb(0x45, 0x5A, 0x64),
        b"VHD" | b"VHDX" | b"VMDK" | b"VDI" | b"QCOW2" | b"QCOW" => rgb(0x15, 0x65, 0xC0),
        _ => rgb(0x6D, 0x4C, 0x41),
    };
    let w = text_width(label, 11) + 8;
    let bx = x + s - w;
    let by = y + s - 18;
    round_rect(g.hdc, g.b, bx, by, bx + w, by + 16, 3, color);
    text(
        g.hdc,
        g.f.small,
        rgb(0xFF, 0xFF, 0xFF),
        bx + 4,
        by + 1,
        label,
    );
}

/// Double-clic sur un fichier : archive → on entre dedans ; image → montage.
/// Renvoie vrai si l'élément a été pris en charge.
unsafe fn open_storage_item(path: &[u8], kind: Kind) -> bool {
    if !matches!(kind, Kind::Archive | Kind::DiskImage) {
        return false;
    }
    let flags = support_of(path);
    if flags & IS_OPEN != 0 {
        navigate(path);
        true
    } else if flags & CAN_READ != 0 {
        open_archive(path, None);
        true
    } else if flags & CAN_MOUNT != 0 {
        mount_image(path);
        true
    } else {
        false
    }
}

// ---------- Entrées ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    if NTABS == 0 {
        open_tab(HOME);
    }
    let geo = geo(f);
    draw_tabs(g, f);
    draw_toolbar(g, &geo);
    draw_nav(g, &geo);
    draw_pane(g, &geo.panes[0], 0);
    if tab().dual {
        fill(
            g.hdc,
            g.b,
            geo.panes[0].right,
            geo.body.top,
            geo.panes[1].left,
            geo.body.bottom,
            pal().border,
        );
        draw_pane(g, &geo.panes[1], 1);
    }
    draw_status(g, &geo);
    draw_menu(g, &geo, f);
    draw_password(g, f);
}

unsafe fn pane_rect_inner(geo: &Geo, which: usize) -> Rect {
    let r = geo.panes[which];
    if tab().dual {
        rect(r.left, r.top + 22, r.right, r.bottom)
    } else {
        r
    }
}

unsafe fn update_hover(geo: &Geo, x: i32, y: i32) -> bool {
    let (mut h, mut zone) = (-1, 0u32);
    for which in 0..if tab().dual { 2 } else { 1 } {
        let r = pane_rect_inner(geo, which);
        if let Some(v) = item_at(&tab().panes[which], &r, x, y) {
            h = v as i32;
            zone = which as u32 + 1;
        }
    }
    let changed = h != HOVER || zone != HOVER_ZONE;
    HOVER = h;
    HOVER_ZONE = zone;
    changed
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    if NTABS == 0 {
        return false;
    }
    if PASSWORD_OPEN {
        return password_mouse(ev, x, y, f);
    }
    let geo = geo(f);
    // Menu ouvert : il capte le clic.
    if matches!(ev, Mouse::Down | Mouse::DoubleClick | Mouse::RightDown) {
        if let Some(hit) = menu_hit(&geo, f, x, y) {
            let menu = MENU;
            MENU = Menu::None;
            if let Some(it) = hit {
                run_item(it, &geo);
                // Les cases à cocher laissent le menu ouvert, comme sous Windows.
                if matches!(
                    it,
                    Item::View(_)
                        | Item::Compact
                        | Item::Hidden
                        | Item::Ext
                        | Item::Nav
                        | Item::Sort(_)
                        | Item::Asc
                        | Item::Desc
                ) && !matches!(menu, Menu::Context { .. })
                {
                    MENU = menu;
                }
                return true;
            }
            if ev != Mouse::RightDown {
                return true;
            }
        }
    }
    match ev {
        Mouse::Move => {
            if let Some((_, grab)) = SCROLL_DRAG {
                let t = tab();
                let which = t.focus;
                let r = pane_rect_inner(&geo, which);
                let pn = &mut t.panes[which];
                let c = cells(&r);
                let total = content_height(pn, &c);
                let view_h = r.bottom - r.top;
                let track_h = view_h - 8;
                let th = ((view_h * track_h) / total.max(1)).max(30);
                pn.scroll = ((y - grab - r.top - 4) * (total - view_h)) / (track_h - th).max(1);
                pn.scroll_target = pn.scroll;
                clamp_scroll(pn, &r);
                return true;
            }
            if let Some((x0, y0, _, _)) = LASSO {
                LASSO = Some((x0, y0, x, y));
                let t = tab();
                let r = pane_rect_inner(&geo, t.focus);
                let pn = &mut t.panes[t.focus];
                let c = cells(&r);
                let (l, tp, rr, b) = (x0.min(x), y0.min(y), x0.max(x), y0.max(y));
                for v in 0..pn.nview {
                    let ir = item_rect(&r, &c, v, pn.scroll);
                    let hit = ir.left < rr && ir.right > l && ir.top < b && ir.bottom > tp;
                    let idx = pn.view[v] as usize;
                    set_sel(pn, idx, hit);
                }
                return true;
            }
            if let Some((sx, sy)) = FILE_DRAG_START {
                let dx = (x - sx).abs();
                let dy = (y - sy).abs();
                if dx > 5 || dy > 5 {
                    FILE_DRAG_ACTIVE = true;
                    FILE_DRAG_START = None;
                }
            }
            if FILE_DRAG_ACTIVE {
                return true;
            }
            update_hover(&geo, x, y)
                || inside(&f.title, x, y)
                || inside(&geo.toolbar, x, y)
                || inside(&geo.command, x, y)
                || inside(&geo.nav, x, y)
                || inside(&geo.status, x, y)
                || MENU != Menu::None
        }
        Mouse::Up => {
            let was_file_drag = FILE_DRAG_ACTIVE;
            let was = LASSO.is_some() || SCROLL_DRAG.is_some();
            LASSO = None;
            SCROLL_DRAG = None;
            FILE_DRAG_START = None;
            if was_file_drag {
                let t = tab();
                if t.dual {
                    let other = 1 - t.focus;
                    let r = geo.panes[other];
                    if inside(&r, x, y) {
                        let dest_loc = t.panes[other].loc;
                        if !dest_loc.is(HOME) && !dest_loc.is(PC) {
                            let do_copy = ctrl();
                            drop_files_to(dest_loc.get(), do_copy);
                        }
                    }
                }
                cancel_file_drag();
                return true;
            }
            was
        }
        Mouse::Wheel(delta) => {
            let t = tab();
            for which in 0..if t.dual { 2 } else { 1 } {
                let r = pane_rect_inner(&geo, which);
                if inside(&r, x, y) {
                    let pn = &mut t.panes[which];
                    if shift() {
                        pn.scroll_x -= delta / 120 * 60;
                        let content_w = if VIEW == View::Details {
                            let (_, _, _, _, sx) = detail_cols(&r);
                            sx + 120 - r.left
                        } else {
                            r.right - r.left
                        };
                        let max_x = (content_w - (r.right - r.left - SCROLL_W)).max(0);
                        pn.scroll_x = pn.scroll_x.clamp(0, max_x);
                    } else {
                        let step = cells(&r).ch.max(40);
                        pn.scroll_target -= delta / 120 * step;
                        let c = cells(&r);
                        let max = (content_height(pn, &c) - (r.bottom - r.top)).max(0);
                        pn.scroll_target = pn.scroll_target.clamp(0, max);
                    }
                    update_hover(&geo, x, y);
                    return true;
                }
            }
            false
        }
        Mouse::Down | Mouse::DoubleClick | Mouse::RightDown | Mouse::MiddleDown => {
            click(ev, x, y, f, &geo)
        }
    }
}

unsafe fn click(ev: Mouse, x: i32, y: i32, f: &Frame, geo: &Geo) -> bool {
    if FOCUS == Focus::Rename {
        commit_rename();
    }
    // Onglets.
    if inside(&f.title, x, y) {
        let t = f.title;
        let w = ((t.right - t.left - 60) / NTABS.max(1) as i32)
            .min(TAB_W)
            .max(90);
        let i = ((x - t.left - 8) / w) as usize;
        if x >= t.left + 8 && i < NTABS {
            let r = rect(
                t.left + 8 + i as i32 * w,
                t.top + 6,
                t.left + 8 + (i as i32 + 1) * w,
                t.bottom,
            );
            let close = rect(r.right - 28, r.top + 6, r.right - 8, r.top + 26);
            if inside(&close, x, y) || ev == Mouse::MiddleDown {
                close_tab(i);
            } else {
                CUR = i;
                FOCUS = Focus::List;
            }
            return true;
        }
        let plus_x = t.left + 8 + NTABS as i32 * w;
        if x >= plus_x && x < plus_x + 40 {
            open_tab(HOME);
            return true;
        }
        return false;
    }
    let nb = nav_buttons(geo);
    for (i, r) in nb.iter().enumerate() {
        if inside(r, x, y) {
            match i {
                0 => go_back(),
                1 => go_forward(),
                2 => go_up(),
                _ => reload(pane()),
            }
            return true;
        }
    }
    if inside(&address_rect(geo), x, y) {
        let loc = pane().loc;
        if loc.is(HOME) {
            ADDRESS.set(tr(S_HOME));
        } else if loc.is(PC) {
            ADDRESS.set(tr(S_THIS_PC));
        } else {
            ADDRESS.set(loc.get());
        }
        FOCUS = Focus::Address;
        ADDRESS_SELECTED = true;
        return true;
    }
    if inside(&search_rect(geo), x, y) {
        FOCUS = Focus::Search;
        return true;
    }
    if FOCUS != Focus::List {
        FOCUS = Focus::List;
    }
    let rects = command_rects(geo);
    for (i, r) in rects.iter().enumerate() {
        if inside(r, x, y) && r.right <= geo.command.right - 8 {
            run_cmd(COMMANDS[i].0, geo);
            return true;
        }
    }
    MENU = Menu::None;
    // Volet de navigation.
    if inside(&geo.nav, x, y) {
        let mut items = [(&b""[..], Glyph::Folder, EMPTY_PATH, 0); NAV_MAX];
        let n = nav_items(&mut items);
        for i in 0..n {
            if inside(&nav_row(geo, i), x, y) {
                let target = items[i].2;
                if ev == Mouse::MiddleDown {
                    open_tab(target.get());
                } else {
                    navigate(target.get());
                }
                return true;
            }
        }
        return false;
    }
    // Barre d'état : bascules d'affichage.
    if inside(&geo.status, x, y) {
        if x >= geo.status.right - 64 && x < geo.status.right - 38 {
            VIEW = View::Details;
            return true;
        }
        if x >= geo.status.right - 34 && x < geo.status.right - 8 {
            VIEW = View::Large;
            return true;
        }
        return false;
    }
    // Volets de fichiers.
    let t = tab();
    for which in 0..if t.dual { 2 } else { 1 } {
        if !inside(&geo.panes[which], x, y) {
            continue;
        }
        t.focus = which;
        let r = pane_rect_inner(geo, which);
        let pn = &mut t.panes[which];
        // Barre de défilement.
        if x >= r.right - SCROLL_W - 4 && ev == Mouse::Down {
            let c = cells(&r);
            let total = content_height(pn, &c);
            let view_h = r.bottom - r.top;
            if total > view_h {
                let track_h = view_h - 8;
                let th = ((view_h * track_h) / total).max(30);
                let ty = r.top + 4 + (pn.scroll * (track_h - th)) / (total - view_h).max(1);
                if y >= ty && y < ty + th {
                    SCROLL_DRAG = Some((0, y - ty));
                } else {
                    pn.scroll_target += if y < ty { -view_h } else { view_h };
                    clamp_scroll(pn, &r);
                }
                return true;
            }
        }
        // En-têtes de la vue Détails : tri.
        if VIEW == View::Details && y < r.top + 4 + cells(&r).header && ev == Mouse::Down {
            let hr = rect(r.left + 8, r.top + 4, r.right - SCROLL_W - 8, r.top + 36);
            let (_, _, dx, tx, sx) = detail_cols(&hr);
            let key = if x >= sx - 8 {
                SortKey::Size
            } else if x >= tx - 8 {
                SortKey::Kind
            } else if x >= dx - 8 {
                SortKey::Date
            } else {
                SortKey::Name
            };
            if SORT == key {
                SORT_DESC = !SORT_DESC;
            } else {
                SORT = key;
                SORT_DESC = false;
            }
            resort_all();
            return true;
        }
        match item_at(pn, &r, x, y) {
            Some(v) => {
                let idx = pn.view[v] as usize;
                match ev {
                    Mouse::DoubleClick => {
                        open_item(pn, idx, false);
                    }
                    Mouse::MiddleDown => {
                        if pn.entries[idx].dir {
                            open_item(pn, idx, true);
                        }
                    }
                    Mouse::RightDown => {
                        if !is_sel(pn, idx) {
                            pn.sel = [0; MAX_ENTRIES / 64];
                            set_sel(pn, idx, true);
                            pn.anchor = idx;
                        }
                        SEL_FLAGS = 0;
                        let e = pn.entries[idx];
                        if !e.dir && matches!(e.kind, Kind::Archive | Kind::DiskImage) {
                            let mut b = [0u8; PATH_LEN + NAME_LEN + 2];
                            let n = item_path(pn, idx, &mut b);
                            SEL_FLAGS = support_of(&b[..n]);
                        }
                        MENU = Menu::Context {
                            x,
                            y,
                            on_item: true,
                        };
                    }
                    _ => {
                        if ctrl() {
                            let s = is_sel(pn, idx);
                            set_sel(pn, idx, !s);
                            pn.anchor = idx;
                        } else if shift() {
                            let a = (0..pn.nview)
                                .find(|&k| pn.view[k] as usize == pn.anchor)
                                .unwrap_or(v);
                            pn.sel = [0; MAX_ENTRIES / 64];
                            for k in a.min(v)..=a.max(v) {
                                let i2 = pn.view[k] as usize;
                                set_sel(pn, i2, true);
                            }
                        } else {
                            pn.sel = [0; MAX_ENTRIES / 64];
                            set_sel(pn, idx, true);
                            pn.anchor = idx;
                        }
                        FILE_DRAG_START = Some((x, y));
                    }
                }
            }
            None => {
                if !ctrl() {
                    pn.sel = [0; MAX_ENTRIES / 64];
                }
                if ev == Mouse::RightDown {
                    MENU = Menu::Context {
                        x,
                        y,
                        on_item: false,
                    };
                } else if ev == Mouse::Down {
                    LASSO = Some((x, y, x, y));
                }
            }
        }
        return true;
    }
    false
}

unsafe fn p_is_dir_renamed() -> bool {
    let p = pane();
    RENAME_INDEX < p.count && p.entries[RENAME_INDEX].dir
}

fn ansi_char(ch: u16) -> Option<u8> {
    if ch < 256 {
        return Some(ch as u8);
    }
    const CP1252: [u16; 32] = [
        0x20ac, 0x0081, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x008d, 0x017d, 0x008f, 0x0090, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022,
        0x2013, 0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0x009d, 0x017e, 0x0178,
    ];
    CP1252.iter().position(|&c| c == ch).map(|i| i as u8 + 0x80)
}

unsafe fn focus_address() {
    let loc = pane().loc;
    ADDRESS.set(if loc.is(HOME) {
        tr(S_HOME)
    } else if loc.is(PC) {
        tr(S_THIS_PC)
    } else {
        loc.get()
    });
    ADDRESS_SELECTED = true;
    FOCUS = Focus::Address;
}

unsafe fn window_shortcut(vk: u32, control: bool) -> bool {
    match vk {
        0x72 => toggle_dual(),
        0x54 if control => {
            let loc = pane().loc;
            open_tab(loc.get());
        }
        0x57 if control => close_tab(CUR),
        0x4C if control => focus_address(),
        0x46 if control => FOCUS = Focus::Search,
        0x41 if control && FOCUS == Focus::Address => ADDRESS_SELECTED = true,
        _ => return false,
    }
    true
}

/// Clavier : `ch` pour un caractère tapé, sinon `vk` (touche virtuelle).
pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if NTABS == 0 {
        return false;
    }
    if PASSWORD_OPEN {
        return password_key(vk, ch);
    }
    if ch == 27 && JOB != 0 && FOCUS == Focus::List && MENU == Menu::None {
        cancel_job();
        return true;
    }
    // Les raccourcis de fenêtre fonctionnent aussi lorsqu'un champ est actif.
    if ch == 0 && window_shortcut(vk, ctrl()) {
        return true;
    }
    let field: Option<&mut Field> = match FOCUS {
        Focus::Search => Some(&mut pane().search),
        Focus::Address => Some(&mut *core::ptr::addr_of_mut!(ADDRESS)),
        Focus::Rename => Some(&mut *core::ptr::addr_of_mut!(RENAME)),
        Focus::List => None,
    };
    if let Some(fl) = field {
        if FOCUS == Focus::Address && ADDRESS_SELECTED && ch != 0 && ch != 13 && ch != 27 {
            fl.len = 0;
            ADDRESS_SELECTED = false;
            if ch == 8 {
                return true;
            }
        }
        if ch >= 32 && ansi_char(ch).is_none() {
            notify(tr4(&[b"Ce caract\xE8re n'est pas encore repr\xE9sentable dans les chemins ANSI d'Explorer.", b"This character can't be represented in Explorer's ANSI paths yet.", b"Este car\xE1cter a\xFAn no se puede representar en las rutas ANSI del Explorador.", b"Dieses Zeichen ist in den ANSI-Pfaden des Explorers noch nicht darstellbar."]));
            return true;
        }
        if FOCUS == Focus::Rename && RENAME_FRESH && ch != 0 && ch != 13 && ch != 27 {
            RENAME_FRESH = false;
            // Comme sous Windows, l'extension est conservée si on retape le nom.
            let ext_start = if p_is_dir_renamed() {
                fl.len
            } else {
                fl.get()
                    .iter()
                    .rposition(|&c| c == b'.')
                    .filter(|&i| i > 0)
                    .unwrap_or(fl.len)
            };
            let mut ext = [0u8; PATH_LEN];
            let el = fl.len - ext_start;
            ext[..el].copy_from_slice(&fl.buf[ext_start..fl.len]);
            fl.len = 0;
            if ch == 8 {
                return true;
            }
            fl.push(ansi_char(ch).unwrap_or_default());
            for &c in &ext[..el] {
                fl.push(c);
            }
            RENAME_TAIL = el;
            return true;
        }
        if FOCUS == Focus::Rename && RENAME_TAIL > 0 && ch >= 32 {
            // Insertion avant l'extension conservée.
            let tail = RENAME_TAIL.min(fl.len);
            let at = fl.len - tail;
            if fl.len < PATH_LEN - 1 {
                for k in (at..fl.len).rev() {
                    fl.buf[k + 1] = fl.buf[k];
                }
                fl.buf[at] = ansi_char(ch).unwrap_or_default();
                fl.len += 1;
            }
            return true;
        }
        if FOCUS == Focus::Rename && RENAME_TAIL > 0 && ch == 8 {
            let tail = RENAME_TAIL.min(fl.len);
            let at = fl.len - tail;
            if at > 0 {
                for k in at..fl.len {
                    fl.buf[k - 1] = fl.buf[k];
                }
                fl.len -= 1;
            }
            return true;
        }
        if ch != 0 {
            match ch {
                8 => fl.pop(),
                13 => {
                    match FOCUS {
                        Focus::Address => {
                            let a = ADDRESS;
                            let s = a.get();
                            if cmp_ci(s, tr(S_HOME)).is_eq() {
                                navigate(HOME);
                            } else if cmp_ci(s, tr(S_THIS_PC)).is_eq() {
                                navigate(PC);
                            } else if s.starts_with(b"http") || s.starts_with(b"www.") {
                                let mut wb = [0u16; PATH_LEN + NAME_LEN + 2];
                                let wopen: [u16; 5] =
                                    [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
                                ShellExecuteW(
                                    core::ptr::null_mut(),
                                    wopen.as_ptr(),
                                    wstr(&mut wb, s),
                                    core::ptr::null(),
                                    core::ptr::null(),
                                    1,
                                );
                                FOCUS = Focus::List;
                            } else {
                                // Accepte aussi un chemin Linux (/home/…) : Z:\ est la racine.
                                let mut t = EMPTY_PATH;
                                if s.first() == Some(&b'/') {
                                    if s.len() + 2 >= PATH_LEN {
                                        notify(tr4(&[b"Chemin trop long : adresse conserv\xE9e sans modification.", b"Path too long: address kept unchanged.", b"Ruta demasiado larga: direcci\xF3n conservada sin cambios.", b"Pfad zu lang: Adresse unver\xE4ndert beibehalten."]));
                                        return true;
                                    }
                                    let mut b2 = [0u8; PATH_LEN];
                                    b2[..2].copy_from_slice(b"Z:");
                                    for (k, &c) in s.iter().enumerate() {
                                        b2[2 + k] = if c == b'/' { b'\\' } else { c };
                                    }
                                    t.set(&b2[..s.len() + 2]);
                                } else {
                                    t.set(s);
                                }
                                let mut tl = t.len as usize;
                                while tl > 3 && t.buf[tl - 1] == b'\\' {
                                    tl -= 1;
                                }
                                t.len = tl as u16;
                                if exists_dir(t.get()) {
                                    navigate(t.get());
                                } else {
                                    let mut wb = [0u16; PATH_LEN + NAME_LEN + 2];
                                    let wopen: [u16; 5] =
                                        [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];
                                    let r = ShellExecuteW(
                                        core::ptr::null_mut(),
                                        wopen.as_ptr(),
                                        wstr(&mut wb, t.get()),
                                        core::ptr::null(),
                                        core::ptr::null(),
                                        1,
                                    );
                                    if (r as isize) <= 32 {
                                        notify(tr4(&[b"Winlux ne trouve pas cet emplacement.", b"Winlux can't find this location.", b"Winlux no encuentra esta ubicaci\xF3n.", b"Winlux findet diesen Speicherort nicht."]));
                                    } else {
                                        FOCUS = Focus::List;
                                    }
                                }
                            }
                        }
                        Focus::Rename => commit_rename(),
                        Focus::Search => {
                            // Entrée : recherche aussi dans les sous-dossiers (3 niveaux).
                            let p = pane();
                            if !p.loc.is(HOME) && !p.loc.is(PC) && p.search.len > 0 {
                                let filter = p.search;
                                let loc = p.loc;
                                p.count = 0;
                                p.truncated = false;
                                p.sel = [0; MAX_ENTRIES / 64];
                                read_dir(p, loc.get(), b"", filter.get(), 3);
                                p.deep = true;
                                rebuild_view(p);
                                notify(tr4(&[b"Recherche termin\xE9e dans les sous-dossiers.", b"Search finished in subfolders.", b"B\xFAsqueda terminada en las subcarpetas.", b"Suche in Unterordnern abgeschlossen."]));
                            }
                        }
                        Focus::List => {}
                    }
                    return true;
                }
                27 => {
                    if FOCUS == Focus::Search {
                        pane().search.len = 0;
                        reload(pane());
                    }
                    FOCUS = Focus::List;
                    return true;
                }
                c if c >= 32 => fl.push(ansi_char(c).unwrap_or_default()),
                _ => return false,
            }
            if FOCUS == Focus::Search {
                let p = pane();
                if p.deep {
                    reload(p);
                }
                rebuild_view(p);
                p.scroll = 0;
                p.scroll_target = 0;
            }
            return true;
        }
        return matches!(vk, 0x25..=0x28);
    }
    // Raccourcis de la liste.
    let p = pane();
    let c = ctrl();
    if ch != 0 {
        // Ctrl+lettre arrive aussi en caractère de contrôle : traité via vk.
        if ch < 32 {
            return false;
        }
        if c {
            return false;
        }
        // Saisie directe : saute au premier élément commençant par la lettre.
        let b = lower(ch.min(255) as u8);
        if let Some(v) = (0..p.nview).find(|&v| {
            p.entries[p.view[v] as usize]
                .name()
                .first()
                .map(|&x| lower(x))
                == Some(b)
        }) {
            let i = p.view[v] as usize;
            p.sel = [0; MAX_ENTRIES / 64];
            set_sel(p, i, true);
            p.anchor = i;
        }
        return true;
    }
    let cur_v = (0..p.nview).find(|&v| p.view[v] as usize == p.anchor && is_sel(p, p.anchor));
    let cols = cells(&rect(0, 0, 1000, 600)).cols.max(1) as usize;
    let move_to = |p: &mut Pane, v: usize| {
        let i = p.view[v] as usize;
        p.sel = [0; MAX_ENTRIES / 64];
        set_sel(p, i, true);
        p.anchor = i;
    };
    match vk {
        0x74 => reload(p), // F5
        0x71 => {
            if let Some(i) = first_sel(p) {
                start_rename(i)
            }
        } // F2
        0x2E => delete_selection(), // Suppr
        0x08 => go_back(), // Retour arrière
        0x0D => {
            if let Some(i) = first_sel(p) {
                open_item(p, i, false)
            }
        }
        0x09 => {
            let t = tab();
            if t.dual {
                t.focus = 1 - t.focus;
            }
        } // Tab : change de volet
        0x26 | 0x28 | 0x25 | 0x27 if p.nview > 0 => {
            let step = if VIEW == View::Details {
                1
            } else if matches!(vk, 0x25 | 0x27) {
                1
            } else {
                cols
            };
            let v = match (cur_v, vk) {
                (None, _) => 0,
                (Some(v), 0x26) | (Some(v), 0x25) => v.saturating_sub(step),
                (Some(v), _) => (v + step).min(p.nview - 1),
            };
            move_to(p, v);
        }
        0x24 if p.nview > 0 => move_to(p, 0), // Origine
        0x23 if p.nview > 0 => move_to(p, p.nview - 1), // Fin
        0x41 if c => {
            for v in 0..p.nview {
                let i = p.view[v] as usize;
                set_sel(p, i, true);
            }
        }
        0x43 if c => copy_selection(false),
        0x58 if c => copy_selection(true),
        0x56 if c => {
            if GetClipboardSequenceNumber() != CLIP_SEQ && IsClipboardFormatAvailable(15) != 0 {
                paste_from_clipboard();
            } else {
                paste();
            }
        }
        0x4E if c && shift() => new_item(true),
        _ => return false,
    }
    true
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let mut animating = poll_job() || PASSWORD_OPEN || crate::thumbs::pending();
    for i in 0..NTABS {
        for which in 0..2 {
            let pn = &mut TABS[i].panes[which];
            if pn.scroll != pn.scroll_target {
                let diff = pn.scroll_target - pn.scroll;
                let step = diff / 4;
                if step == 0 {
                    pn.scroll = pn.scroll_target;
                } else {
                    pn.scroll += step;
                }
                animating = true;
            }
        }
    }
    animating
        || FOCUS != Focus::List
        || MESSAGE_UNTIL != 0 && {
            let alive = (MESSAGE_UNTIL.wrapping_sub(GetTickCount()) as i32) > -1000;
            if !alive {
                MESSAGE_UNTIL = 0;
            }
            alive
        }
}

/// Exécuté uniquement par le programme PE `explorer_regression.exe` ; le
/// linker retire cette fonction inutilisée de l'exécutable du bureau.
#[doc(hidden)]
pub unsafe fn regression_checks() {
    let mut scratch = [0u8; PATH_LEN + NAME_LEN + 2];
    let long_name = [b'a'; 200];
    let mut path = EMPTY_PATH;
    path.set(b"C:\\original");
    check!(
        !path.set(&[b'x'; PATH_LEN]) && path.is(b"C:\\original"),
        "Explorer refuse un chemin trop long sans le tronquer"
    );
    let n = join(&mut scratch, b"C:\\test", &long_name);
    check!(
        n == 208 && &scratch[8..n] == &long_name,
        "Explorer conserve les noms de plus de 128 octets"
    );
    check!(
        join(&mut scratch, &[b'x'; 200], &[b'y'; 80]) == 0 && scratch[0] == 0,
        "Explorer refuse les chemins joints hors buffer"
    );
    check!(
        !valid_component(b"?.txt")
            && !valid_component(b"literal\\name")
            && valid_component(&long_name),
        "Explorer refuse les noms ANSI ambigus avant une operation"
    );
    check!(
        same_or_descendant(b"C:\\source\\child", b"c:\\source")
            && !same_or_descendant(b"C:\\source2", b"C:\\source"),
        "Explorer compare les limites de composants des descendants"
    );

    let mut raw = [0i8; PATH_LEN];
    sprintf(
        raw.as_mut_ptr(),
        c"Z:\\tmp\\wlexp-%u".as_ptr(),
        GetCurrentProcessId(),
    );
    let rn = strlen(raw.as_ptr());
    let root = core::slice::from_raw_parts(raw.as_ptr().cast::<u8>(), rn);
    let mut wr = [0u16; PATH_LEN + NAME_LEN + 2];
    for i in 0..rn {
        wr[i] = raw[i] as u16;
    }
    wr[rn] = 0;
    let created = CreateDirectoryW(wr.as_ptr(), core::ptr::null_mut()) != 0;
    check!(created, "Explorer cree son dossier de regression isole");
    if !created {
        return;
    }
    let mut src = [0u8; PATH_LEN + NAME_LEN + 2];
    let sn = join(&mut src, root, b"source");
    let mut ws = [0u16; PATH_LEN + NAME_LEN + 2];
    CreateDirectoryW(wstr(&mut ws, &src[..sn]), core::ptr::null_mut());
    let mut child = [0u8; PATH_LEN + NAME_LEN + 2];
    let cn = join(&mut child, &src[..sn], b"child");
    CreateDirectoryW(wstr(&mut ws, &child[..cn]), core::ptr::null_mut());
    let mut dst = [0u8; PATH_LEN + NAME_LEN + 2];
    let dn = join(&mut dst, root, b"copied");
    CreateDirectoryW(wstr(&mut ws, &dst[..dn]), core::ptr::null_mut());
    let mut file = [0u8; PATH_LEN + NAME_LEN + 2];
    let fnl = join(&mut file, &src[..sn], &long_name);
    let h = CreateFileW(
        wstr(&mut ws, &file[..fnl]),
        0x4000_0000,
        7,
        core::ptr::null_mut(),
        1,
        0x80,
        core::ptr::null_mut(),
    );
    let mut written = 0;
    let wrote = h as isize != -1
        && WriteFile(
            h,
            b"original".as_ptr(),
            8,
            &mut written,
            core::ptr::null_mut(),
        ) != 0;
    if h as isize != -1 {
        CloseHandle(h);
    }
    check!(
        wrote && written == 8,
        "Explorer prepare un vrai fichier au nom long"
    );
    check!(
        !copy_file_new(&file[..fnl], &file[..fnl]) && {
            let mut wc = [0u16; PATH_LEN + NAME_LEN + 2];
            GetFileAttributesW(wstr(&mut wc, &file[..fnl])) != u32::MAX
        },
        "Une collision de copie ne supprime jamais un fichier existant"
    );
    NTABS = 1;
    CUR = 0;
    navigate_pane(&mut TABS[0].panes[0], &src[..sn], false);
    let i = select_named(pane(), &long_name);
    check!(
        i.is_some(),
        "Explorer lit le nom long integral depuis FindFirstFileW"
    );
    if i.is_some() {
        copy_selection(true);
        paste();
        check!(
            NCLIP == 0
                && {
                    let mut wc = [0u16; PATH_LEN + NAME_LEN + 2];
                    GetFileAttributesW(wstr(&mut wc, &file[..fnl])) != u32::MAX
                }
                && select_named(pane(), &long_name).is_some(),
            "Couper-coller dans le meme dossier ne renomme pas la source"
        );
        copy_selection(false);
        check!(
            NCLIP == 1 && CLIP[0].get() == &file[..fnl],
            "Explorer copie le chemin integral dans le presse-papiers"
        );
        navigate_pane(pane(), &dst[..dn], false);
        paste();
        let n = join(&mut scratch, &dst[..dn], &long_name);
        check!(
            n != 0
                && {
                    let mut wc = [0u16; PATH_LEN + NAME_LEN + 2];
                    GetFileAttributesW(wstr(&mut wc, &scratch[..n])) != u32::MAX
                }
                && {
                    let mut wc2 = [0u16; PATH_LEN + NAME_LEN + 2];
                    GetFileAttributesW(wstr(&mut wc2, &file[..fnl])) != u32::MAX
                },
            "Explorer colle le nom exact et conserve la source"
        );
    }
    let mut recursive = [0u8; PATH_LEN + NAME_LEN + 2];
    let recn = join(&mut recursive, &child[..cn], b"source");
    check!(
        !copy_tree(&src[..sn], &recursive[..recn]) && {
            let mut wc = [0u16; PATH_LEN + NAME_LEN + 2];
            GetFileAttributesW(wstr(&mut wc, &recursive[..recn])) == u32::MAX
        },
        "Explorer interdit une copie recursive dans son sous-dossier avant creation"
    );
    let mut alias = [0u8; PATH_LEN + NAME_LEN + 2];
    let an = join(&mut alias, &src[..sn], b"child\\..\\child");
    check!(
        !safe_destination(&src[..sn], &alias[..an]),
        "Explorer resout les alias avec .. avant de copier"
    );
    let mut deep_target = [0u8; PATH_LEN + NAME_LEN + 2];
    let tn = join(&mut deep_target, root, &[b'd'; 100]);
    CreateDirectoryW(wstr(&mut ws, &deep_target[..tn]), core::ptr::null_mut());
    let mut too_long = [0u8; PATH_LEN + NAME_LEN + 2];
    let ln = join(&mut too_long, &deep_target[..tn], b"copy");
    check!(
        !copy_tree(&src[..sn], &too_long[..ln]) && {
            let mut wc = [0u16; PATH_LEN + NAME_LEN + 2];
            GetFileAttributesW(wstr(&mut wc, &too_long[..ln])) == u32::MAX
        },
        "Une copie dont les enfants depassent MAX_PATH est refusee sans arbre partiel"
    );

    navigate_pane(pane(), &src[..sn], false);
    focus_address();
    key(0, b'Z' as u16);
    check!(
        ADDRESS.get() == b"Z" && !ADDRESS_SELECTED,
        "Ctrl+L selectionne et la frappe remplace toute l'adresse"
    );
    FOCUS = Focus::Address;
    let dual = tab().dual;
    check!(
        window_shortcut(0x72, false) && tab().dual != dual,
        "F3 fonctionne pendant la saisie d'une adresse"
    );
    FOCUS = Focus::Address;
    let tabs = NTABS;
    check!(
        window_shortcut(0x54, true) && NTABS == tabs + 1,
        "Ctrl+T fonctionne pendant la saisie d'une adresse"
    );
    check!(
        ansi_char(0x20AC) == Some(0x80) && ansi_char(0x03A9).is_none(),
        "Explorer preserve Euro et refuse les conversions ANSI destructrices"
    );
    let mut program = [0u8; PATH_LEN + NAME_LEN + 2];
    let program_len = join(&mut program, root, b"Mon programme.EXE");
    let mut wide_program = [0u16; PATH_LEN + NAME_LEN + 2];
    wstr(&mut wide_program, &program[..program_len]);
    crate::settings::regression_exe_settings(&wide_program);
    let p = pane();
    p.count = 0;
    p.sel = [0; MAX_ENTRIES / 64];
    add_entry(p, b"Mon programme.EXE", false, false, 0, 0, Some(Kind::Exe));
    set_sel(p, 0, true);
    MENU = Menu::Context {
        x: 0,
        y: 0,
        on_item: true,
    };
    let mut items = [Item::Sep; MENU_MAX];
    let count = menu_items(&mut items);
    check!(
        items[..count].contains(&Item::ExeSettings)
            && items[..count].contains(&Item::Delete)
            && items[..count].contains(&Item::Rename),
        "Clic droit EXE : reglages facultatifs, renommer et supprimer restent disponibles"
    );
    pane().entries[0].dir = true;
    let count = menu_items(&mut items);
    check!(
        !items[..count].contains(&Item::ExeSettings),
        "Un dossier nomme .EXE ne propose pas de profil executable"
    );
    MENU = Menu::None;
    archive_regression(root);
    check!(
        rollback_copy(root, 0),
        "Explorer nettoie ses seuls fichiers de regression"
    );
}

#[doc(hidden)]
unsafe fn wait_job(job: u32) -> u32 {
    if job == 0 {
        return u32::MAX;
    }
    for _ in 0..3000 {
        let mut pr = 0u32;
        let st = WinluxJobPoll(job, &mut pr, core::ptr::null_mut(), 0);
        if st != 0 {
            WinluxJobClose(job);
            return st;
        }
        Sleep(10);
    }
    u32::MAX
}

/// Archives : création, ouverture comme un dossier, ajout d'un fichier,
/// reconstruction puis extraction de contrôle.
#[doc(hidden)]
unsafe fn archive_regression(root: &[u8]) {
    check!(
        drive_letter_in(b"Ubuntu (E:)") == Some(b'E')
            && drive_letter_in(b"Serveur (Y:) (hors ligne)") == Some(b'Y')
            && drive_letter_in(b"Dossier (1)").is_none(),
        "Explorer retrouve la lettre d'un lecteur dynamique"
    );
    check!(
        path_in_archive(b"C:\\a\\b.zip\\c")
            && path_in_archive(b"C:\\a\\b.tar.gz")
            && !path_in_archive(b"C:\\a\\zip"),
        "Explorer reconnait un chemin a l'interieur d'une archive"
    );
    check!(
        kind_of(b"w.VHDX", false) == Kind::DiskImage
            && kind_of(b"u.iso", false) == Kind::DiskImage
            && kind_of(b"a.tgz", false) == Kind::Archive,
        "Explorer distingue archives et images disque"
    );
    let mut src = [0u8; PATH_LEN + NAME_LEN + 2];
    let sn = join(&mut src, root, b"source");
    let mut zip = [0u8; PATH_LEN + NAME_LEN + 2];
    let zn = join(&mut zip, root, b"paquet.zip");
    let mut wz = [0u16; PATH_LEN + NAME_LEN + 2];
    let mut sources = [0u16; PATH_LEN + NAME_LEN + 4];
    for (i, &c) in src[..sn].iter().enumerate() {
        sources[i] = c as u16;
    }
    let st = wait_job(WinluxArchiveCreate(
        wstr(&mut wz, &zip[..zn]),
        sources.as_ptr(),
        core::ptr::null(),
    ));
    check!(
        st == 1 && support_of(&zip[..zn]) & (CAN_READ | CAN_WRITE) == CAN_READ | CAN_WRITE,
        "Explorer compresse un dossier en ZIP"
    );
    let st = wait_job(WinluxArchiveOpen(
        wstr(&mut wz, &zip[..zn]),
        core::ptr::null(),
    ));
    let mut inner = [0u8; PATH_LEN + NAME_LEN + 2];
    let inn = join(&mut inner, &zip[..zn], b"source");
    check!(
        st == 1 && exists_dir(&inner[..inn]) && support_of(&zip[..zn]) & IS_OPEN != 0,
        "Explorer ouvre une archive ZIP comme un dossier"
    );
    let mut added = [0u8; PATH_LEN + NAME_LEN + 2];
    let an = join(&mut added, &inner[..inn], b"ajout.txt");
    let h = CreateFileW(
        wstr(&mut wz, &added[..an]),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        1,
        0x80,
        core::ptr::null_mut(),
    );
    let mut written = 0u32;
    let ok = !h.is_null() && h as isize != -1 && {
        WriteFile(
            h,
            b"winlux".as_ptr(),
            6,
            &mut written,
            core::ptr::null_mut(),
        );
        CloseHandle(h);
        true
    };
    let st = wait_job(WinluxArchiveCommit(wstr(&mut wz, &inner[..inn])));
    check!(ok, "Explorer ajoute un fichier dans une archive ouverte");
    check!(st == 1, "Explorer enregistre l'archive modifiee");
    let mut verif = [0u8; PATH_LEN + NAME_LEN + 2];
    let vn = join(&mut verif, root, b"verif");
    let mut wv = [0u16; PATH_LEN + NAME_LEN + 2];
    let st = wait_job(WinluxArchiveExtract(
        wstr(&mut wz, &zip[..zn]),
        wstr(&mut wv, &verif[..vn]),
        core::ptr::null(),
    ));
    let mut sub = [0u8; PATH_LEN + NAME_LEN + 2];
    let subn = join(&mut sub, &verif[..vn], b"source");
    let mut out = [0u8; PATH_LEN + NAME_LEN + 2];
    let on = join(&mut out, &sub[..subn], b"ajout.txt");
    check!(
        st == 1 && GetFileAttributesW(wstr(&mut wv, &out[..on])) != u32::MAX,
        "Explorer extrait l'archive modifiee"
    );
}
