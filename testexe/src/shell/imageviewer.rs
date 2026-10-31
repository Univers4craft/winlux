//! Visionneuse d'images de Winlux, façon « Photos » de Windows 11.
//!
//! Le décodage et le rendu sont faits par le runtime (décodeurs maison,
//! `runtime/src/imaging`) via les exports `WinluxImage*` ; cette application
//! gère l'interface : zoom (molette, ajusté, 100 %), panoramique, rotation,
//! miroir, pages/calques/images d'une animation, navigation dans le dossier,
//! diaporama, panneau d'informations (EXIF/IPTC/XMP), comparaison avant/après
//! pour les photos RAW, aperçu tournant des modèles 3D et impression.
//!
//! Garde-fou : un fichier qu'aucun décodeur ne sait lire affiche un panneau
//! d'erreur clair, jamais une fenêtre vide.

use crate::gfx::*;
use crate::locale::{Lang, CURRENT_LANG};
use winlux_testexe::*;

const TOOLBAR_H: i32 = 48;
const BOTTOM_H: i32 = 40;
const PANEL_W: i32 = 300;
const MAX_FILES: usize = 4096;
const NAME_CAP: usize = 260;
const PATH_CAP: usize = 1024;

const FLAG_ANIMATED: u32 = 1;
const FLAG_LAYERED: u32 = 2;
const FLAG_RAW: u32 = 4;
const FLAG_VECTOR: u32 = 32;
const FLAG_MODEL: u32 = 64;

const TIMER_SLIDESHOW: usize = 1;
const TIMER_ANIM: usize = 2;
const TIMER_SPIN: usize = 3;

// ---------- Textes (français, anglais, espagnol, allemand) ----------

fn t(s: [&'static [u8]; 4]) -> &'static [u8] {
    s[match unsafe { CURRENT_LANG } {
        Lang::Fr => 0,
        Lang::En => 1,
        Lang::Es => 2,
        Lang::De => 3,
    }]
}

const TITLE: [&[u8]; 4] = [
    b"Visionneuse d'images",
    b"Image Viewer",
    b"Visor de im\xE1genes",
    b"Bildanzeige",
];
const FIT: [&[u8]; 4] = [b"Ajuster", b"Fit", b"Ajustar", b"Einpassen"];
const ROTATE: [&[u8]; 4] = [b"Pivoter", b"Rotate", b"Girar", b"Drehen"];
const MIRROR: [&[u8]; 4] = [b"Miroir", b"Mirror", b"Espejo", b"Spiegeln"];
const SLIDESHOW: [&[u8]; 4] = [b"Diaporama", b"Slideshow", b"Presentaci\xF3n", b"Diashow"];
const PRINT: [&[u8]; 4] = [b"Imprimer", b"Print", b"Imprimir", b"Drucken"];
const INFO: [&[u8]; 4] = [b"Infos", b"Info", b"Info", b"Info"];
const COMPARE: [&[u8]; 4] = [b"Comparer", b"Compare", b"Comparar", b"Vergleichen"];
const DETAILS: [&[u8]; 4] = [b"Informations", b"Details", b"Detalles", b"Details"];
const PAGE: [&[u8]; 4] = [b"Page", b"Page", b"P\xE1gina", b"Seite"];
const LAYER: [&[u8]; 4] = [b"Calque", b"Layer", b"Capa", b"Ebene"];
const FRAME: [&[u8]; 4] = [b"Image", b"Frame", b"Fotograma", b"Bild"];
const PLAY: [&[u8]; 4] = [b"Lecture", b"Play", b"Reproducir", b"Abspielen"];
const PAUSE: [&[u8]; 4] = [b"Pause", b"Pause", b"Pausa", b"Pause"];
const EXPOSURE: [&[u8]; 4] = [b"Exposition", b"Exposure", b"Exposici\xF3n", b"Belichtung"];
const AUTO_LEVELS: [&[u8]; 4] = [
    b"Niveaux auto",
    b"Auto levels",
    b"Niveles auto",
    b"Auto-Tonwert",
];
const BEFORE_AFTER: [&[u8]; 4] = [
    b"Avant  |  Apr\xE8s",
    b"Before  |  After",
    b"Antes  |  Despu\xE9s",
    b"Vorher  |  Nachher",
];
const DRAG_ROTATE: [&[u8]; 4] = [
    b"Glissez pour tourner le mod\xE8le",
    b"Drag to rotate the model",
    b"Arrastra para girar el modelo",
    b"Ziehen, um das Modell zu drehen",
];
const NO_FILE: [&[u8]; 4] = [
    b"Ouvrez une image depuis l'Explorateur",
    b"Open a picture from File Explorer",
    b"Abre una imagen desde el Explorador",
    b"\xD6ffnen Sie ein Bild im Explorer",
];
const SUPPORTED: [&[u8]; 4] = [
    b"Formats pris en charge :",
    b"Supported formats:",
    b"Formatos compatibles:",
    b"Unterst\xFCtzte Formate:",
];
const CANT_OPEN: [&[u8]; 4] = [
    b"Impossible d'ouvrir cette image",
    b"Can't open this picture",
    b"No se puede abrir esta imagen",
    b"Dieses Bild kann nicht ge\xF6ffnet werden",
];
const ERR_UNSUPPORTED: [&[u8]; 4] = [
    b"Ce format n'est pas encore pris en charge par Winlux.",
    b"This format isn't supported by Winlux yet.",
    b"Winlux a\xFAn no admite este formato.",
    b"Dieses Format wird von Winlux noch nicht unterst\xFCtzt.",
];
const ERR_VARIANT: [&[u8]; 4] = [
    b"Cette variante du format n'est pas encore prise en charge :",
    b"This variant of the format isn't supported yet:",
    b"Esta variante del formato a\xFAn no es compatible:",
    b"Diese Formatvariante wird noch nicht unterst\xFCtzt:",
];
const ERR_CORRUPT: [&[u8]; 4] = [
    b"Le fichier est endommag\xE9 ou incomplet.",
    b"The file is damaged or incomplete.",
    b"El archivo est\xE1 da\xF1ado o incompleto.",
    b"Die Datei ist besch\xE4digt oder unvollst\xE4ndig.",
];
const ERR_TOO_LARGE: [&[u8]; 4] = [
    b"L'image est trop grande pour \xEAtre affich\xE9e.",
    b"The picture is too large to display.",
    b"La imagen es demasiado grande para mostrarla.",
    b"Das Bild ist zu gro\xDF f\xFCr die Anzeige.",
];
const ERR_IO: [&[u8]; 4] = [
    b"Le fichier est introuvable ou illisible.",
    b"The file can't be found or read.",
    b"No se encuentra o no se puede leer el archivo.",
    b"Die Datei wurde nicht gefunden oder ist nicht lesbar.",
];
const L_NAME: [&[u8]; 4] = [b"Nom", b"Name", b"Nombre", b"Name"];
const L_FORMAT: [&[u8]; 4] = [b"Format", b"Format", b"Formato", b"Format"];
const L_DIMENSIONS: [&[u8]; 4] = [b"Dimensions", b"Dimensions", b"Dimensiones", b"Abmessungen"];
const L_SIZE: [&[u8]; 4] = [b"Taille", b"Size", b"Tama\xF1o", b"Gr\xF6\xDFe"];
const L_PAGES: [&[u8]; 4] = [b"Pages", b"Pages", b"P\xE1ginas", b"Seiten"];
const L_FOLDER: [&[u8]; 4] = [
    b"Dans le dossier",
    b"In folder",
    b"En la carpeta",
    b"Im Ordner",
];
const L_NONE: [&[u8]; 4] = [
    b"Aucune m\xE9tadonn\xE9e dans ce fichier.",
    b"No metadata in this file.",
    b"Este archivo no tiene metadatos.",
    b"Keine Metadaten in dieser Datei.",
];
const PRINT_FAILED: [&[u8]; 4] = [
    b"L'impression a \xE9chou\xE9.",
    b"Printing failed.",
    b"Error al imprimir.",
    b"Drucken fehlgeschlagen.",
];

/// Libellé lisible d'une clé de métadonnée stable du runtime.
fn meta_label(key: &[u8]) -> &[u8] {
    let l: [&'static [u8]; 4] = match key {
        b"camera.make" => [b"Fabricant", b"Camera maker", b"Fabricante", b"Hersteller"],
        b"camera.model" => [b"Appareil", b"Camera model", b"Modelo", b"Kameramodell"],
        b"lens" => [b"Objectif", b"Lens", b"Objetivo", b"Objektiv"],
        b"date.taken" => [
            b"Prise de vue",
            b"Date taken",
            b"Fecha de captura",
            b"Aufnahmedatum",
        ],
        b"date" => [b"Date", b"Date", b"Fecha", b"Datum"],
        b"exposure" => [
            b"Exposition",
            b"Exposure time",
            b"Exposici\xF3n",
            b"Belichtungszeit",
        ],
        b"aperture" => [b"Ouverture", b"Aperture", b"Apertura", b"Blende"],
        b"iso" => [b"Sensibilit\xE9", b"ISO speed", b"ISO", b"ISO"],
        b"focal" => [
            b"Focale",
            b"Focal length",
            b"Distancia focal",
            b"Brennweite",
        ],
        b"exposure.bias" => [
            b"Correction",
            b"Exposure bias",
            b"Compensaci\xF3n",
            b"Belichtungskorrektur",
        ],
        b"flash" => [b"Flash", b"Flash", b"Flash", b"Blitz"],
        b"gps" => [
            b"Position GPS",
            b"GPS location",
            b"Ubicaci\xF3n GPS",
            b"GPS-Position",
        ],
        b"artist" => [b"Auteur", b"Author", b"Autor", b"Autor"],
        b"copyright" => [b"Copyright", b"Copyright", b"Copyright", b"Copyright"],
        b"software" => [b"Logiciel", b"Software", b"Software", b"Software"],
        b"description" => [
            b"Description",
            b"Description",
            b"Descripci\xF3n",
            b"Beschreibung",
        ],
        b"rating" => [b"Note", b"Rating", b"Valoraci\xF3n", b"Bewertung"],
        b"iptc.title" => [b"Titre", b"Title", b"T\xEDtulo", b"Titel"],
        b"iptc.headline" => [
            b"Titre principal",
            b"Headline",
            b"Titular",
            b"\xDCberschrift",
        ],
        b"iptc.keywords" => [
            b"Mots-cl\xE9s",
            b"Keywords",
            b"Palabras clave",
            b"Stichw\xF6rter",
        ],
        b"iptc.city" => [b"Ville", b"City", b"Ciudad", b"Stadt"],
        b"iptc.country" => [b"Pays", b"Country", b"Pa\xEDs", b"Land"],
        b"model.triangles" => [b"Triangles", b"Triangles", b"Tri\xE1ngulos", b"Dreiecke"],
        _ => return key,
    };
    t(l)
}

// ---------- État ----------

struct State {
    hwnd: Handle,
    path: [u16; PATH_CAP],
    path_len: usize,
    img: usize,
    info: WinluxImageInfoC,
    page: u32,
    view: WinluxViewC,
    fit: bool,
    error: i32,
    error_detail: [u8; 160],
    buf: *mut u8,
    buf_cap: usize,
    files: *mut [u16; NAME_CAP],
    sizes: [u64; MAX_FILES],
    nfiles: usize,
    cur: usize,
    /// Le fichier ouvert fait partie de la liste du dossier.
    found: bool,
    dir_len: usize,
    slideshow: bool,
    panel: bool,
    compare: bool,
    playing: bool,
    spinning: bool,
    drag: u8,
    last_x: i32,
    last_y: i32,
    down_x: i32,
    down_y: i32,
    hover: i32,
    buttons: [(Rect, u8); 32],
    nbuttons: usize,
    can_print: bool,
    message: &'static [u8],
}

static mut S: State = State {
    hwnd: core::ptr::null_mut(),
    path: [0; PATH_CAP],
    path_len: 0,
    img: 0,
    info: WinluxImageInfoC {
        width: 0,
        height: 0,
        pages: 0,
        flags: 0,
        delay_ms: 0,
        meta_count: 0,
        format: [0; 32],
    },
    page: 0,
    view: DEFAULT_VIEW,
    fit: true,
    error: -1,
    error_detail: [0; 160],
    buf: core::ptr::null_mut(),
    buf_cap: 0,
    files: core::ptr::null_mut(),
    sizes: [0; MAX_FILES],
    nfiles: 0,
    cur: 0,
    found: false,
    dir_len: 0,
    slideshow: false,
    panel: false,
    compare: false,
    playing: true,
    spinning: true,
    drag: 0,
    last_x: 0,
    last_y: 0,
    down_x: 0,
    down_y: 0,
    hover: -1,
    buttons: [(rect(0, 0, 0, 0), 0); 32],
    nbuttons: 0,
    can_print: false,
    message: b"",
};

const DEFAULT_VIEW: WinluxViewC = WinluxViewC {
    zoom: 1.0,
    cx: 0.0,
    cy: 0.0,
    rot: 0,
    mirror: 0,
    yaw: 0.6,
    pitch: 0.35,
    exposure: 0.0,
    auto_levels: 0,
    split: -1.0,
};

#[allow(static_mut_refs)]
unsafe fn st() -> &'static mut State {
    &mut *core::ptr::addr_of_mut!(S)
}

// Identifiants des boutons.
const B_PREV: u8 = 1;
const B_NEXT: u8 = 2;
const B_ZOOM_OUT: u8 = 3;
const B_ZOOM_IN: u8 = 4;
const B_FIT: u8 = 5;
const B_ACTUAL: u8 = 6;
const B_ROTATE: u8 = 7;
const B_MIRROR: u8 = 8;
const B_SLIDESHOW: u8 = 9;
const B_PRINT: u8 = 10;
const B_INFO: u8 = 11;
const B_COMPARE: u8 = 12;
const B_PAGE_PREV: u8 = 13;
const B_PAGE_NEXT: u8 = 14;
const B_PLAY: u8 = 15;
const B_EXP_DOWN: u8 = 16;
const B_EXP_UP: u8 = 17;
const B_LEVELS: u8 = 18;

// ---------- Petits utilitaires ----------

struct Line {
    b: [u8; 160],
    n: usize,
}

impl Line {
    fn new() -> Self {
        Line { b: [0; 160], n: 0 }
    }
    fn s(&mut self, s: &[u8]) -> &mut Self {
        for &c in s {
            if self.n < self.b.len() {
                self.b[self.n] = c;
                self.n += 1;
            }
        }
        self
    }
    fn num(&mut self, mut v: u64) -> &mut Self {
        let mut d = [0u8; 20];
        let mut i = 0;
        loop {
            d[i] = b'0' + (v % 10) as u8;
            v /= 10;
            i += 1;
            if v == 0 {
                break;
            }
        }
        while i > 0 {
            i -= 1;
            self.s(&[d[i]]);
        }
        self
    }
    fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

fn cstr(b: &[u8]) -> &[u8] {
    &b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]
}

/// Couleur GDI (0x00BBGGRR) vers le 0xRRGGBB attendu par le runtime.
fn to_rgb(c: u32) -> u32 {
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}

fn image_bg() -> u32 {
    if unsafe { st().slideshow } {
        rgb(0, 0, 0)
    } else if unsafe { DARK } {
        rgb(0x1C, 0x1C, 0x1C)
    } else {
        rgb(0xF3, 0xF3, 0xF3)
    }
}

unsafe fn file_name(s: &State) -> [u8; NAME_CAP] {
    let mut out = [0u8; NAME_CAP];
    let mut start = 0;
    for i in 0..s.path_len {
        if s.path[i] == b'\\' as u16 || s.path[i] == b'/' as u16 {
            start = i + 1;
        }
    }
    for (k, &c) in s.path[start..s.path_len]
        .iter()
        .take(NAME_CAP - 1)
        .enumerate()
    {
        out[k] = if c < 256 { c as u8 } else { b'?' };
    }
    out
}

fn wlen(w: &[u16]) -> usize {
    w.iter().position(|&c| c == 0).unwrap_or(w.len())
}

fn lower(c: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&c) {
        c + 32
    } else {
        c
    }
}

fn name_cmp(a: &[u16], b: &[u16]) -> core::cmp::Ordering {
    let (la, lb) = (wlen(a), wlen(b));
    for i in 0..la.min(lb) {
        let (x, y) = (lower(a[i]), lower(b[i]));
        if x != y {
            return x.cmp(&y);
        }
    }
    la.cmp(&lb)
}

// ---------- Préférences du bureau ----------

unsafe fn read_small_file(path: &core::ffi::CStr, out: &mut [u8]) -> usize {
    let h = CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        return 0;
    }
    let mut got = 0u32;
    ReadFile(
        h,
        out.as_mut_ptr(),
        out.len() as u32,
        &mut got,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    (got as usize).min(out.len())
}

/// Thème et langue du bureau : `theme=` du fichier d'état de winlux-control,
/// la langue choisie dans Paramètres, sinon `LANG`.
unsafe fn load_preferences() {
    let mut buf = [0u8; 4096];
    let n = read_small_file(c"Z:\\run\\winlux\\control.txt", &mut buf);
    for line in buf[..n].split(|&c| c == b'\n') {
        match line {
            b"theme=light" => DARK = false,
            b"theme=dark" => DARK = true,
            _ => {}
        }
    }
    let mut conf = [0u8; 8];
    if read_small_file(c"Z:\\etc\\winlux\\locale.conf", &mut conf) > 0 {
        crate::locale::load_lang();
        return;
    }
    let mut lang = [0i8; 16];
    if GetEnvironmentVariableA(c"LANG".as_ptr(), lang.as_mut_ptr(), 16) >= 2 {
        let code = [lang[0] as u8, lang[1] as u8];
        CURRENT_LANG = crate::locale::lang_from_code(&code);
    }
}

// ---------- Ouverture et dossier ----------

unsafe fn close_image(s: &mut State) {
    if s.img != 0 {
        WinluxImageClose(s.img);
        s.img = 0;
    }
    KillTimer(s.hwnd, TIMER_ANIM);
    KillTimer(s.hwnd, TIMER_SPIN);
}

unsafe fn open_current(s: &mut State) {
    close_image(s);
    s.page = 0;
    s.fit = true;
    s.compare = false;
    s.view = DEFAULT_VIEW;
    s.error_detail = [0; 160];
    s.path[s.path_len] = 0;
    s.img = WinluxImageOpen(s.path.as_ptr());
    if s.img == 0 || WinluxImageInfo(s.img, &mut s.info) == 0 || s.info.pages == 0 {
        s.error = WinluxImageLastError(s.error_detail.as_mut_ptr(), 160).max(1);
        if s.img != 0 {
            WinluxImageClose(s.img);
            s.img = 0;
        }
    } else {
        s.error = 0;
        s.playing = true;
        s.spinning = true;
        start_animation(s);
    }
    let mut title = [0u16; PATH_CAP + 40];
    let name = file_name(s);
    let mut k = 0;
    for &c in cstr(&name) {
        title[k] = c as u16;
        k += 1;
    }
    if k > 0 {
        for &c in b" - " {
            title[k] = c as u16;
            k += 1;
        }
    }
    for &c in t(TITLE) {
        title[k] = c as u16;
        k += 1;
    }
    SetWindowTextW(s.hwnd, title.as_ptr());
    InvalidateRect(s.hwnd, core::ptr::null(), 0);
}

unsafe fn start_animation(s: &mut State) {
    KillTimer(s.hwnd, TIMER_ANIM);
    KillTimer(s.hwnd, TIMER_SPIN);
    if s.img == 0 {
        return;
    }
    if s.info.flags & FLAG_MODEL != 0 && s.spinning {
        SetTimer(s.hwnd, TIMER_SPIN, 33, 0);
    } else if s.info.flags & FLAG_ANIMATED != 0 && s.info.pages > 1 && s.playing {
        let mut pi: WinluxPageInfoC = core::mem::zeroed();
        WinluxImagePage(s.img, s.page, &mut pi);
        let d = if pi.delay_ms < 20 { 100 } else { pi.delay_ms };
        SetTimer(s.hwnd, TIMER_ANIM, d, 0);
    }
}

/// Liste les images du dossier du fichier ouvert (ordre alphabétique) pour
/// la navigation précédent/suivant et le diaporama.
unsafe fn scan_folder(s: &mut State) {
    s.nfiles = 0;
    s.cur = 0;
    s.found = false;
    if s.files.is_null() {
        s.files = VirtualAlloc(core::ptr::null_mut(), MAX_FILES * NAME_CAP * 2, 0x3000, 4)
            as *mut [u16; NAME_CAP];
        if s.files.is_null() {
            return;
        }
    }
    let mut dir = 0;
    for i in 0..s.path_len {
        if s.path[i] == b'\\' as u16 || s.path[i] == b'/' as u16 {
            dir = i + 1;
        }
    }
    s.dir_len = dir;
    let mut pattern = [0u16; PATH_CAP + 2];
    pattern[..dir].copy_from_slice(&s.path[..dir]);
    pattern[dir] = b'*' as u16;
    let mut fd: Win32FindDataW = core::mem::zeroed();
    let h = FindFirstFileW(pattern.as_ptr(), &mut fd);
    if h.is_null() || h as isize == -1 {
        return;
    }
    let mut full = [0u16; PATH_CAP + NAME_CAP];
    full[..dir].copy_from_slice(&s.path[..dir]);
    loop {
        let n = wlen(&fd.file_name);
        if fd.attributes & 0x10 == 0 && n > 0 && dir + n < PATH_CAP && s.nfiles < MAX_FILES {
            full[dir..dir + n].copy_from_slice(&fd.file_name[..n]);
            full[dir + n] = 0;
            if WinluxImageSupported(full.as_ptr()) != 0 {
                let slot = &mut *s.files.add(s.nfiles);
                *slot = [0; NAME_CAP];
                slot[..n.min(NAME_CAP - 1)].copy_from_slice(&fd.file_name[..n.min(NAME_CAP - 1)]);
                s.sizes[s.nfiles] = ((fd.size_high as u64) << 32) | fd.size_low as u64;
                s.nfiles += 1;
            }
        }
        if FindNextFileW(h, &mut fd) == 0 {
            break;
        }
    }
    FindClose(h);
    // Tri par insertion (dossiers de quelques milliers d'images au plus).
    for i in 1..s.nfiles {
        let mut j = i;
        while j > 0 && name_cmp(&*s.files.add(j - 1), &*s.files.add(j)).is_gt() {
            core::ptr::swap(s.files.add(j - 1), s.files.add(j));
            s.sizes.swap(j - 1, j);
            j -= 1;
        }
    }
    let me = &s.path[dir..s.path_len];
    for i in 0..s.nfiles {
        let f = &*s.files.add(i);
        let n = wlen(f);
        if n == me.len() && name_cmp(&f[..n], me).is_eq() {
            s.cur = i;
            s.found = true;
        }
    }
}

unsafe fn go(s: &mut State, delta: i32) {
    if s.nfiles < 2 {
        return;
    }
    let n = s.nfiles as i32;
    s.cur = if s.found {
        (((s.cur as i32 + delta) % n + n) % n) as usize
    } else if delta > 0 {
        0
    } else {
        s.nfiles - 1
    };
    s.found = true;
    let f = &*s.files.add(s.cur);
    let len = wlen(f);
    if s.dir_len + len >= PATH_CAP {
        return;
    }
    s.path[s.dir_len..s.dir_len + len].copy_from_slice(&f[..len]);
    s.path_len = s.dir_len + len;
    open_current(s);
}

/// Chemin passé sur la ligne de commande (premier argument après le programme).
unsafe fn command_line_path(s: &mut State) {
    let cl = GetCommandLineW();
    if cl.is_null() {
        return;
    }
    let mut i = 0usize;
    let at = |k: usize| *cl.add(k);
    if at(0) == b'"' as u16 {
        i = 1;
        while at(i) != 0 && at(i) != b'"' as u16 {
            i += 1;
        }
        if at(i) != 0 {
            i += 1;
        }
    } else {
        while at(i) != 0 && at(i) != b' ' as u16 {
            i += 1;
        }
    }
    while at(i) == b' ' as u16 {
        i += 1;
    }
    let quoted = at(i) == b'"' as u16;
    if quoted {
        i += 1;
    }
    let mut n = 0;
    while at(i) != 0 && n < PATH_CAP - 1 {
        let c = at(i);
        if quoted && c == b'"' as u16 {
            break;
        }
        s.path[n] = c;
        n += 1;
        i += 1;
    }
    while n > 0 && s.path[n - 1] == b' ' as u16 {
        n -= 1;
    }
    s.path_len = n;
}

pub unsafe fn init(hwnd: Handle) {
    let s = st();
    s.hwnd = hwnd;
    load_preferences();
    let comdlg = LoadLibraryA(c"comdlg32.dll".as_ptr());
    let gdi = GetModuleHandleA(c"gdi32.dll".as_ptr());
    s.can_print = !comdlg.is_null()
        && !GetProcAddress(comdlg, c"PrintDlgW".as_ptr()).is_null()
        && !gdi.is_null()
        && !GetProcAddress(gdi, c"StartDocW".as_ptr()).is_null();
    command_line_path(s);
    if s.path_len > 0 {
        open_current(s);
        scan_folder(s);
    } else {
        SetWindowTextW(hwnd, wide_title().as_ptr());
    }
}

fn wide_title() -> [u16; 40] {
    let mut w = [0u16; 40];
    for (i, &c) in t(TITLE).iter().take(39).enumerate() {
        w[i] = c as u16;
    }
    w
}

// ---------- Géométrie de la vue ----------

fn image_area(s: &State, client: &Rect) -> Rect {
    if s.slideshow {
        return *client;
    }
    let mut r = rect(
        client.left,
        client.top + TOOLBAR_H,
        client.right,
        client.bottom,
    );
    if s.panel {
        r.right = (r.right - PANEL_W).max(r.left + 1);
    }
    if has_bottom(s) {
        r.bottom = (r.bottom - BOTTOM_H).max(r.top + 1);
    }
    r
}

fn has_bottom(s: &State) -> bool {
    s.img != 0 && (s.info.pages > 1 || s.compare || s.info.flags & FLAG_MODEL != 0) && !s.slideshow
}

unsafe fn page_size(s: &State) -> (f32, f32) {
    let mut pi: WinluxPageInfoC = core::mem::zeroed();
    if s.img == 0 || WinluxImagePage(s.img, s.page, &mut pi) == 0 {
        return (1.0, 1.0);
    }
    (pi.width.max(1) as f32, pi.height.max(1) as f32)
}

unsafe fn fit_zoom(s: &State, area: &Rect) -> f32 {
    let (pw, ph) = page_size(s);
    let (pw, ph) = if s.view.rot % 2 == 1 {
        (ph, pw)
    } else {
        (pw, ph)
    };
    let aw = (area.right - area.left - 16).max(1) as f32;
    let ah = (area.bottom - area.top - 16).max(1) as f32;
    let z = (aw / pw).min(ah / ph);
    // Une petite image bitmap reste à 100 % ; vectoriel et 3D remplissent.
    if s.info.flags & (FLAG_VECTOR | FLAG_MODEL) != 0 {
        z
    } else {
        z.min(1.0)
    }
}

/// Déplacement à l'écran → déplacement en coordonnées image.
fn screen_to_image_delta(v: &WinluxViewC, dx: f32, dy: f32) -> (f32, f32) {
    let (mut x, mut y) = (if v.mirror != 0 { -dx } else { dx }, dy);
    for _ in 0..(v.rot % 4) {
        let (nx, ny) = (y, -x);
        x = nx;
        y = ny;
    }
    (x / v.zoom, y / v.zoom)
}

unsafe fn apply_fit(s: &mut State, area: &Rect) {
    if s.fit {
        let (pw, ph) = page_size(s);
        s.view.zoom = fit_zoom(s, area);
        s.view.cx = pw / 2.0;
        s.view.cy = ph / 2.0;
    }
}

unsafe fn set_zoom(s: &mut State, area: &Rect, zoom: f32, mx: i32, my: i32) {
    apply_fit(s, area);
    let zoom = zoom.clamp(0.01, 64.0);
    let dx = (mx - (area.left + area.right) / 2) as f32;
    let dy = (my - (area.top + area.bottom) / 2) as f32;
    let (ix, iy) = screen_to_image_delta(&s.view, dx, dy);
    let (px, py) = (s.view.cx + ix, s.view.cy + iy);
    s.view.zoom = zoom;
    let (nx, ny) = screen_to_image_delta(&s.view, dx, dy);
    s.view.cx = px - nx;
    s.view.cy = py - ny;
    s.fit = false;
    clamp_center(s);
}

unsafe fn clamp_center(s: &mut State) {
    let (pw, ph) = page_size(s);
    s.view.cx = s.view.cx.clamp(0.0, pw);
    s.view.cy = s.view.cy.clamp(0.0, ph);
}

// ---------- Dessin ----------

unsafe fn ensure_buffer(s: &mut State, w: i32, h: i32) -> bool {
    let need = (w.max(1) as usize) * (h.max(1) as usize) * 4;
    if need > s.buf_cap {
        if !s.buf.is_null() {
            VirtualFree(s.buf as *mut core::ffi::c_void, 0, 0x8000);
        }
        s.buf = VirtualAlloc(core::ptr::null_mut(), need, 0x3000, 4) as *mut u8;
        s.buf_cap = if s.buf.is_null() { 0 } else { need };
    }
    !s.buf.is_null()
}

/// Copie un tampon BGRA haut-bas à l'écran, par bandes (limite GDI).
unsafe fn blit(hdc: Handle, x: i32, y: i32, w: i32, h: i32, bits: *const u8) {
    let band = (4_000_000 / w.max(1)).clamp(1, h.max(1));
    let mut row = 0;
    while row < h {
        let rows = band.min(h - row);
        let info = BitmapInfoHeader {
            size: 40,
            width: w,
            height: -rows,
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        };
        SetDIBitsToDevice(
            hdc,
            x,
            y + row,
            w as u32,
            rows as u32,
            0,
            0,
            0,
            rows as u32,
            bits.add(row as usize * w as usize * 4) as *const core::ffi::c_void,
            &info,
            0,
        );
        row += rows;
    }
}

unsafe fn add_button(s: &mut State, r: Rect, id: u8) {
    if s.nbuttons < s.buttons.len() {
        s.buttons[s.nbuttons] = (r, id);
        s.nbuttons += 1;
    }
}

/// Bouton texte de barre d'outils ; renvoie sa largeur.
unsafe fn tool_button(
    g: &mut crate::wm::Gfx,
    s: &mut State,
    x: i32,
    y: i32,
    label: &[u8],
    id: u8,
    active: bool,
) -> i32 {
    let p = pal();
    let w = text_width(label, 13) + 20;
    let r = rect(x, y, x + w, y + 32);
    let hover = s.hover == id as i32;
    if active {
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.selected);
    } else {
        flat_button(g.hdc, g.b, &r, hover, false);
    }
    text(
        g.hdc,
        g.f.body,
        if active { p.accent } else { p.text },
        x + 10,
        y + 9,
        label,
    );
    add_button(s, r, id);
    w
}

unsafe fn draw_toolbar(g: &mut crate::wm::Gfx, s: &mut State, c: &Rect) {
    let p = pal();
    fill(
        g.hdc,
        g.b,
        c.left,
        c.top,
        c.right,
        c.top + TOOLBAR_H,
        p.mica,
    );
    fill(
        g.hdc,
        g.b,
        c.left,
        c.top + TOOLBAR_H - 1,
        c.right,
        c.top + TOOLBAR_H,
        p.border,
    );
    let y = c.top + 8;
    // À droite : actions, de droite à gauche.
    let mut right = c.right - 8;
    let place = |g: &mut crate::wm::Gfx,
                 s: &mut State,
                 right: &mut i32,
                 label: &[u8],
                 id: u8,
                 active: bool| {
        let w = text_width(label, 13) + 20;
        *right -= w + 2;
        tool_button(g, s, *right, y, label, id, active);
    };
    if s.img != 0 {
        place(g, s, &mut right, t(INFO), B_INFO, s.panel);
        if s.can_print {
            place(g, s, &mut right, t(PRINT), B_PRINT, false);
        }
        if s.nfiles > 1 {
            place(g, s, &mut right, t(SLIDESHOW), B_SLIDESHOW, false);
        }
        if s.info.flags & FLAG_RAW != 0 {
            place(g, s, &mut right, t(COMPARE), B_COMPARE, s.compare);
        }
        if s.info.flags & FLAG_MODEL == 0 {
            place(g, s, &mut right, t(MIRROR), B_MIRROR, s.view.mirror != 0);
            place(g, s, &mut right, t(ROTATE), B_ROTATE, false);
        }
        place(
            g,
            s,
            &mut right,
            b"1:1",
            B_ACTUAL,
            !s.fit && (s.view.zoom - 1.0).abs() < 1e-3,
        );
        place(g, s, &mut right, t(FIT), B_FIT, s.fit);
        place(g, s, &mut right, b"+", B_ZOOM_IN, false);
        let mut z = Line::new();
        z.num((s.view.zoom * 100.0 + 0.5) as u64).s(b" %");
        let zw = 56;
        right -= zw;
        text_centered(g.hdc, g.f.body, 13, p.text2, right + zw / 2, y + 9, z.get());
        place(g, s, &mut right, b"-", B_ZOOM_OUT, false);
    }
    // À gauche : précédent/suivant, nom du fichier et position dans le dossier.
    let mut x = c.left + 8;
    if s.nfiles > 1 {
        x += tool_button(g, s, x, y, b"<", B_PREV, false) + 2;
        x += tool_button(g, s, x, y, b">", B_NEXT, false) + 8;
    }
    let name = file_name(s);
    let mut line = Line::new();
    line.s(cstr(&name));
    if s.nfiles > 1 && s.found {
        line.s(b"   ")
            .num(s.cur as u64 + 1)
            .s(b" / ")
            .num(s.nfiles as u64);
    }
    let label: &[u8] = if s.path_len == 0 {
        t(TITLE)
    } else {
        line.get()
    };
    text_fit(
        g.hdc,
        g.f.semibold,
        13,
        p.text,
        x + 4,
        y + 9,
        label,
        (right - x - 16).max(0),
    );
}

unsafe fn draw_bottom(g: &mut crate::wm::Gfx, s: &mut State, c: &Rect, area: &Rect) {
    let p = pal();
    let top = area.bottom;
    fill(g.hdc, g.b, c.left, top, area.right, c.bottom, p.mica);
    fill(g.hdc, g.b, c.left, top, area.right, top + 1, p.border);
    let y = top + 4;
    let mut x = c.left + 8;
    if s.compare {
        let mut l = Line::new();
        l.s(t(EXPOSURE)).s(b" ");
        let e = s.view.exposure;
        l.s(if e < 0.0 { b"-" } else { b"+" });
        let tenths = (e.abs() * 10.0 + 0.5) as u64;
        l.num(tenths / 10).s(b",").num(tenths % 10).s(b" IL");
        x += tool_button(g, s, x, y, b"-", B_EXP_DOWN, false) + 4;
        text(g.hdc, g.f.body, p.text, x, y + 9, l.get());
        x += text_width(l.get(), 13) + 8;
        x += tool_button(g, s, x, y, b"+", B_EXP_UP, false) + 12;
        x += tool_button(
            g,
            s,
            x,
            y,
            t(AUTO_LEVELS),
            B_LEVELS,
            s.view.auto_levels != 0,
        ) + 16;
        text(g.hdc, g.f.caption, p.text3, x, y + 10, t(BEFORE_AFTER));
        return;
    }
    if s.info.flags & FLAG_MODEL != 0 {
        x += tool_button(
            g,
            s,
            x,
            y,
            if s.spinning { t(PAUSE) } else { t(PLAY) },
            B_PLAY,
            false,
        ) + 12;
        text(g.hdc, g.f.caption, p.text3, x, y + 10, t(DRAG_ROTATE));
        return;
    }
    let animated = s.info.flags & FLAG_ANIMATED != 0;
    if animated {
        x += tool_button(
            g,
            s,
            x,
            y,
            if s.playing { t(PAUSE) } else { t(PLAY) },
            B_PLAY,
            false,
        ) + 8;
    }
    x += tool_button(g, s, x, y, b"<", B_PAGE_PREV, false) + 4;
    let kind = if animated {
        t(FRAME)
    } else if s.info.flags & FLAG_LAYERED != 0 {
        t(LAYER)
    } else {
        t(PAGE)
    };
    let mut l = Line::new();
    l.s(kind)
        .s(b" ")
        .num(s.page as u64 + 1)
        .s(b" / ")
        .num(s.info.pages as u64);
    let mut pi: WinluxPageInfoC = core::mem::zeroed();
    if WinluxImagePage(s.img, s.page, &mut pi) != 0 && pi.name[0] != 0 {
        l.s(b"  -  ").s(cstr(&pi.name));
    }
    let w = text_width(l.get(), 13).max(120);
    text(g.hdc, g.f.body, p.text, x + 4, y + 9, l.get());
    x += w + 12;
    tool_button(g, s, x, y, b">", B_PAGE_NEXT, false);
}

unsafe fn draw_panel(g: &mut crate::wm::Gfx, s: &mut State, c: &Rect) {
    let p = pal();
    let l = c.right - PANEL_W;
    let top = c.top + TOOLBAR_H;
    fill(g.hdc, g.b, l, top, c.right, c.bottom, p.surface);
    fill(g.hdc, g.b, l, top, l + 1, c.bottom, p.border);
    let x = l + 16;
    let w = PANEL_W - 32;
    let mut y = top + 16;
    text(g.hdc, g.f.title, p.text, x, y, t(DETAILS));
    y += 32;
    let row = |g: &mut crate::wm::Gfx, y: &mut i32, label: &[u8], value: &[u8]| {
        if *y > c.bottom - 30 {
            return;
        }
        text_fit(g.hdc, g.f.small, 11, p.text3, x, *y, label, w);
        text_fit(g.hdc, g.f.body, 13, p.text, x, *y + 15, value, w);
        *y += 40;
    };
    let name = file_name(s);
    row(g, &mut y, t(L_NAME), cstr(&name));
    row(g, &mut y, t(L_FORMAT), cstr(&s.info.format));
    let mut d = Line::new();
    d.num(s.info.width as u64)
        .s(b" x ")
        .num(s.info.height as u64)
        .s(b" px");
    if s.info.width as u64 * s.info.height as u64 >= 1_000_000 {
        let mp = s.info.width as u64 * s.info.height as u64 / 100_000;
        d.s(b"  (").num(mp / 10).s(b",").num(mp % 10).s(b" Mpx)");
    }
    row(g, &mut y, t(L_DIMENSIONS), d.get());
    if s.found {
        let bytes = s.sizes[s.cur];
        let mut z = Line::new();
        if bytes >= 1 << 20 {
            let tenth = bytes * 10 / (1 << 20);
            z.num(tenth / 10).s(b",").num(tenth % 10).s(b" Mo");
        } else {
            z.num(bytes.div_ceil(1024)).s(b" Ko");
        }
        row(g, &mut y, t(L_SIZE), z.get());
    }
    if s.info.pages > 1 {
        let mut z = Line::new();
        z.num(s.info.pages as u64);
        row(g, &mut y, t(L_PAGES), z.get());
    }
    if s.nfiles > 1 && s.found {
        let mut z = Line::new();
        z.num(s.cur as u64 + 1).s(b" / ").num(s.nfiles as u64);
        row(g, &mut y, t(L_FOLDER), z.get());
    }
    y += 6;
    fill(g.hdc, g.b, x, y, x + w, y + 1, p.border);
    y += 12;
    if s.info.meta_count == 0 {
        text_fit(g.hdc, g.f.caption, 12, p.text3, x, y, t(L_NONE), w);
    }
    for i in 0..s.info.meta_count {
        let mut k = [0u8; 64];
        let mut v = [0u8; 160];
        if WinluxImageMeta(s.img, i, k.as_mut_ptr(), 64, v.as_mut_ptr(), 160) == 0 {
            break;
        }
        row(g, &mut y, meta_label(cstr(&k)), cstr(&v));
    }
}

unsafe fn draw_nav_arrows(g: &mut crate::wm::Gfx, s: &mut State, area: &Rect) {
    if s.nfiles < 2 || s.slideshow {
        return;
    }
    let p = pal();
    let cy = (area.top + area.bottom) / 2;
    for (x, label, id) in [
        (area.left + 12, b"<", B_PREV),
        (area.right - 52, b">", B_NEXT),
    ] {
        let r = rect(x, cy - 20, x + 40, cy + 20);
        let hover = s.hover == id as i32;
        ellipse(
            g.hdc,
            g.b,
            r.left,
            r.top,
            r.right,
            r.bottom,
            if hover { p.hover } else { p.surface },
        );
        text_centered(g.hdc, g.f.large, 22, p.text, x + 20, cy - 13, label);
        add_button(s, r, id);
    }
}

unsafe fn draw_message(g: &mut crate::wm::Gfx, s: &State, area: &Rect) {
    let p = pal();
    if s.path_len == 0 {
        let cx = (area.left + area.right) / 2;
        let mut y = (area.top + area.bottom) / 2 - 80;
        glyph(g.hdc, g.b, g.f, Glyph::Photos, cx - 32, y, 64);
        y += 84;
        text_centered(g.hdc, g.f.title, 15, p.text, cx, y, t(NO_FILE));
        y += 30;
        text_centered(g.hdc, g.f.caption, 12, p.text3, cx, y, t(SUPPORTED));
        y += 20;
        // Liste réelle des formats annoncés par le runtime (garde-fou).
        let mut buf = [0u8; 2048];
        let n = (WinluxImageFormats(buf.as_mut_ptr(), 2048) as usize).min(2047);
        for line in buf[..n].split(|&c| c == b'\n') {
            if line.is_empty() || y > area.bottom - 20 {
                continue;
            }
            text_fit(
                g.hdc,
                g.f.small,
                11,
                p.text2,
                area.left + 24,
                y,
                line,
                area.right - area.left - 48,
            );
            y += 16;
        }
        return;
    }
    let cx = (area.left + area.right) / 2;
    let mut y = (area.top + area.bottom) / 2 - 70;
    glyph(g.hdc, g.b, g.f, Glyph::Image, cx - 28, y, 56);
    y += 74;
    text_centered(g.hdc, g.f.title, 15, p.text, cx, y, t(CANT_OPEN));
    y += 28;
    let reason = match s.error {
        1 => t(ERR_UNSUPPORTED),
        2 => t(ERR_VARIANT),
        3 => t(ERR_CORRUPT),
        4 => t(ERR_TOO_LARGE),
        _ => t(ERR_IO),
    };
    text_centered(g.hdc, g.f.body, 13, p.text2, cx, y, reason);
    let detail = cstr(&s.error_detail);
    if !detail.is_empty() {
        y += 20;
        text_centered(g.hdc, g.f.caption, 12, p.text3, cx, y, detail);
    }
    y += 24;
    let name = file_name(s);
    text_centered(g.hdc, g.f.caption, 12, p.text3, cx, y, cstr(&name));
}

pub unsafe fn paint(g: &mut crate::wm::Gfx, client: &Rect) {
    let s = st();
    let p = pal();
    s.nbuttons = 0;
    let area = image_area(s, client);
    let bg = image_bg();
    if s.img != 0 {
        apply_fit(s, &area);
        let w = area.right - area.left;
        let h = area.bottom - area.top;
        if w > 0 && h > 0 && ensure_buffer(s, w, h) {
            if s.compare {
                s.view.split = 0.5;
            } else {
                s.view.split = -1.0;
                s.view.exposure = 0.0;
                s.view.auto_levels = 0;
            }
            WinluxImageRender(
                s.img,
                s.page,
                &s.view,
                s.buf,
                w as u32,
                h as u32,
                w as u32 * 4,
                to_rgb(bg),
                // Damier sous la transparence, sauf pour les modèles 3D.
                (s.info.flags & FLAG_MODEL == 0) as u32,
            );
            blit(g.hdc, area.left, area.top, w, h, s.buf);
        }
    } else {
        fill(g.hdc, g.b, area.left, area.top, area.right, area.bottom, bg);
        draw_message(g, s, &area);
    }
    if !s.slideshow {
        draw_nav_arrows(g, s, &area);
        draw_toolbar(g, s, client);
        if has_bottom(s) {
            draw_bottom(g, s, client, &area);
        }
        if s.panel && s.img != 0 {
            draw_panel(g, s, client);
        }
    }
    if !s.message.is_empty() {
        let w = text_width(s.message, 13) + 32;
        let cx = (area.left + area.right) / 2;
        let r = rect(cx - w / 2, area.bottom - 56, cx + w / 2, area.bottom - 20);
        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 6, p.surface2);
        text(g.hdc, g.f.body, p.text, r.left + 16, r.top + 11, s.message);
    }
}

// ---------- Actions ----------

unsafe fn client_rect(s: &State) -> Rect {
    let mut r: Rect = core::mem::zeroed();
    GetClientRect(s.hwnd, &mut r);
    r
}

unsafe fn redraw(s: &State) {
    InvalidateRect(s.hwnd, core::ptr::null(), 0);
}

unsafe fn zoom_step(s: &mut State, factor: f32) {
    let c = client_rect(s);
    let area = image_area(s, &c);
    apply_fit(s, &area);
    let z = s.view.zoom * factor;
    set_zoom(
        s,
        &area,
        z,
        (area.left + area.right) / 2,
        (area.top + area.bottom) / 2,
    );
}

unsafe fn toggle_slideshow(s: &mut State) {
    s.slideshow = !s.slideshow && s.nfiles > 1;
    if s.slideshow {
        s.fit = true;
        SetTimer(s.hwnd, TIMER_SLIDESHOW, 4000, 0);
    } else {
        KillTimer(s.hwnd, TIMER_SLIDESHOW);
    }
}

unsafe fn set_page(s: &mut State, page: i32) {
    if s.img == 0 || s.info.pages == 0 {
        return;
    }
    let n = s.info.pages as i32;
    s.page = ((page % n + n) % n) as u32;
    if s.info.flags & FLAG_ANIMATED == 0 {
        s.fit = true;
    }
}

unsafe fn action(s: &mut State, id: u8) {
    s.message = b"";
    match id {
        B_PREV => go(s, -1),
        B_NEXT => go(s, 1),
        B_ZOOM_IN => zoom_step(s, 1.25),
        B_ZOOM_OUT => zoom_step(s, 0.8),
        B_FIT => s.fit = true,
        B_ACTUAL => {
            let c = client_rect(s);
            let area = image_area(s, &c);
            set_zoom(
                s,
                &area,
                1.0,
                (area.left + area.right) / 2,
                (area.top + area.bottom) / 2,
            );
        }
        B_ROTATE => s.view.rot = (s.view.rot + 1) % 4,
        B_MIRROR => s.view.mirror ^= 1,
        B_SLIDESHOW => toggle_slideshow(s),
        B_PRINT => print(s),
        B_INFO => s.panel = !s.panel,
        B_COMPARE => {
            s.compare = !s.compare;
            if s.compare && s.view.exposure == 0.0 && s.view.auto_levels == 0 {
                s.view.auto_levels = 1;
            }
        }
        B_PAGE_PREV => set_page(s, s.page as i32 - 1),
        B_PAGE_NEXT => set_page(s, s.page as i32 + 1),
        B_PLAY => {
            if s.info.flags & FLAG_MODEL != 0 {
                s.spinning = !s.spinning;
            } else {
                s.playing = !s.playing;
            }
            start_animation(s);
        }
        B_EXP_DOWN => s.view.exposure = (s.view.exposure - 0.3).max(-5.0),
        B_EXP_UP => s.view.exposure = (s.view.exposure + 0.3).min(5.0),
        B_LEVELS => s.view.auto_levels ^= 1,
        _ => {}
    }
    redraw(s);
}

fn hit(s: &State, x: i32, y: i32) -> i32 {
    for i in (0..s.nbuttons).rev() {
        if inside(&s.buttons[i].0, x, y) {
            return s.buttons[i].1 as i32;
        }
    }
    -1
}

pub unsafe fn mouse_down(x: i32, y: i32) {
    let s = st();
    s.down_x = x;
    s.down_y = y;
    s.last_x = x;
    s.last_y = y;
    if s.slideshow {
        toggle_slideshow(s);
        redraw(s);
        return;
    }
    let b = hit(s, x, y);
    if b > 0 {
        action(s, b as u8);
        return;
    }
    let c = client_rect(s);
    let area = image_area(s, &c);
    if s.img == 0 || !inside(&area, x, y) {
        return;
    }
    let split_x = area.left + (area.right - area.left) / 2;
    s.drag = if s.info.flags & FLAG_MODEL != 0 {
        3
    } else if s.compare && (x - split_x).abs() < 8 {
        2
    } else {
        1
    };
    SetCapture(s.hwnd);
}

pub unsafe fn mouse_move(x: i32, y: i32) {
    let s = st();
    let dx = (x - s.last_x) as f32;
    let dy = (y - s.last_y) as f32;
    s.last_x = x;
    s.last_y = y;
    match s.drag {
        1 => {
            let c = client_rect(s);
            let area = image_area(s, &c);
            apply_fit(s, &area);
            s.fit = false;
            let (ix, iy) = screen_to_image_delta(&s.view, dx, dy);
            s.view.cx -= ix;
            s.view.cy -= iy;
            clamp_center(s);
            redraw(s);
        }
        3 => {
            s.view.yaw += dx * 0.012;
            s.view.pitch = (s.view.pitch + dy * 0.012).clamp(-1.5, 1.5);
            redraw(s);
        }
        _ => {
            let h = hit(s, x, y);
            if h != s.hover {
                s.hover = h;
                redraw(s);
            }
        }
    }
}

pub unsafe fn mouse_up() {
    let s = st();
    if s.drag != 0 {
        s.drag = 0;
        ReleaseCapture();
    }
}

pub unsafe fn double_click(x: i32, y: i32) {
    let s = st();
    let c = client_rect(s);
    let area = image_area(s, &c);
    if s.img == 0 || !inside(&area, x, y) || hit(s, x, y) > 0 {
        return;
    }
    if s.fit {
        set_zoom(s, &area, 1.0, x, y);
    } else {
        s.fit = true;
    }
    redraw(s);
}

/// Molette : zoom centré sur le pointeur (coordonnées client).
pub unsafe fn wheel(delta: i32, x: i32, y: i32) {
    let s = st();
    if s.img == 0 {
        return;
    }
    let c = client_rect(s);
    let area = image_area(s, &c);
    apply_fit(s, &area);
    let steps = delta as f32 / 120.0;
    let mut f = 1.0f32;
    let k = if steps > 0.0 { 1.2 } else { 1.0 / 1.2 };
    for _ in 0..((steps.abs() + 0.5) as i32).max(1) {
        f *= k;
    }
    let z = s.view.zoom * f;
    set_zoom(s, &area, z, x, y);
    redraw(s);
}

pub unsafe fn key(vk: u32) {
    let s = st();
    let ctrl = GetKeyState(0x11) < 0;
    match vk {
        0x25 | 0x26 => go(s, -1),
        0x27 | 0x28 | 0x20 => go(s, 1),
        0x24 => {
            if s.nfiles > 1 {
                go(s, -(s.cur as i32));
            }
        }
        0x23 => {
            if s.nfiles > 1 {
                go(s, (s.nfiles - 1 - s.cur) as i32);
            }
        }
        0x21 => action(s, B_PAGE_PREV),
        0x22 => action(s, B_PAGE_NEXT),
        0x6B | 0xBB => action(s, B_ZOOM_IN),
        0x6D | 0xBD => action(s, B_ZOOM_OUT),
        0x30 | 0x60 => action(s, B_FIT),
        0x31 | 0x61 => action(s, B_ACTUAL),
        0x52 => action(s, B_ROTATE),
        0x4D => action(s, B_MIRROR),
        0x49 => action(s, B_INFO),
        0x43 if s.info.flags & FLAG_RAW != 0 => action(s, B_COMPARE),
        0x50 if ctrl && s.can_print => action(s, B_PRINT),
        0x74 => action(s, B_SLIDESHOW),
        0x1B => {
            if s.slideshow {
                toggle_slideshow(s);
            } else {
                PostQuitMessage(0);
            }
        }
        _ => return,
    }
    redraw(s);
}

pub unsafe fn timer(id: usize) {
    let s = st();
    match id {
        TIMER_SLIDESHOW => go(s, 1),
        TIMER_ANIM => {
            if s.img != 0 && s.info.pages > 1 {
                s.page = (s.page + 1) % s.info.pages;
                start_animation(s);
            }
        }
        TIMER_SPIN => {
            if s.drag != 3 {
                s.view.yaw += 0.03;
            }
        }
        _ => return,
    }
    redraw(s);
}

pub unsafe fn shutdown() {
    close_image(st());
}

// ---------- Impression ----------

type PrintDlgW = unsafe extern "system" fn(*mut u8) -> i32;
type StartDocW = unsafe extern "system" fn(Handle, *const u8) -> i32;
type DcFn = unsafe extern "system" fn(Handle) -> i32;

/// Imprime la page affichée, ajustée à la feuille, via la boîte « Imprimer »
/// du système (`PrintDlgW`) et le pilote d'impression Winlux.
unsafe fn print(s: &mut State) {
    if s.img == 0 || !s.can_print {
        return;
    }
    let comdlg = LoadLibraryA(c"comdlg32.dll".as_ptr());
    let gdi = GetModuleHandleA(c"gdi32.dll".as_ptr());
    let pd_fn = GetProcAddress(comdlg, c"PrintDlgW".as_ptr());
    let sd_fn = GetProcAddress(gdi, c"StartDocW".as_ptr());
    let sp_fn = GetProcAddress(gdi, c"StartPage".as_ptr());
    let ep_fn = GetProcAddress(gdi, c"EndPage".as_ptr());
    let ed_fn = GetProcAddress(gdi, c"EndDoc".as_ptr());
    if pd_fn.is_null() || sd_fn.is_null() || sp_fn.is_null() || ep_fn.is_null() || ed_fn.is_null() {
        return;
    }
    let print_dlg: PrintDlgW = core::mem::transmute(pd_fn);
    let start_doc: StartDocW = core::mem::transmute(sd_fn);
    let start_page: DcFn = core::mem::transmute(sp_fn);
    let end_page: DcFn = core::mem::transmute(ep_fn);
    let end_doc: DcFn = core::mem::transmute(ed_fn);
    // PRINTDLGW x64 (120 octets) : PD_RETURNDC | PD_NOSELECTION | PD_NOPAGENUMS.
    let mut pd = [0u8; 120];
    pd[0..4].copy_from_slice(&120u32.to_le_bytes());
    pd[8..16].copy_from_slice(&(s.hwnd as usize).to_le_bytes());
    pd[40..44].copy_from_slice(&(0x100u32 | 0x4 | 0x8).to_le_bytes());
    pd[52..54].copy_from_slice(&1u16.to_le_bytes());
    if print_dlg(pd.as_mut_ptr()) == 0 {
        return;
    }
    let hdc = usize::from_le_bytes([
        pd[32], pd[33], pd[34], pd[35], pd[36], pd[37], pd[38], pd[39],
    ]) as Handle;
    if hdc.is_null() {
        s.message = t(PRINT_FAILED);
        return;
    }
    let pw = GetDeviceCaps(hdc, 8).max(1);
    let ph = GetDeviceCaps(hdc, 10).max(1);
    let (iw, ih) = page_size(s);
    let (iw, ih) = if s.view.rot % 2 == 1 {
        (ih, iw)
    } else {
        (iw, ih)
    };
    let scale = (pw as f32 / iw).min(ph as f32 / ih);
    let dw = ((iw * scale) as i32).max(1);
    let dh = ((ih * scale) as i32).max(1);
    // Rendu à au plus 2400 px de côté, étiré par le pilote à la taille finale.
    let r = (2400.0 / dw.max(dh) as f32).min(1.0);
    let bw = ((dw as f32 * r) as i32).max(1);
    let bh = ((dh as f32 * r) as i32).max(1);
    let bits = VirtualAlloc(core::ptr::null_mut(), (bw * bh * 4) as usize, 0x3000, 4) as *mut u8;
    let mut ok = !bits.is_null();
    if ok {
        let mut v = s.view;
        v.zoom = bw as f32 / if s.view.rot % 2 == 1 { ih } else { iw }.max(1.0);
        v.zoom = v
            .zoom
            .min(bh as f32 / if s.view.rot % 2 == 1 { iw } else { ih }.max(1.0));
        let (ow, oh) = page_size(s);
        v.cx = ow / 2.0;
        v.cy = oh / 2.0;
        v.split = -1.0;
        WinluxImageRender(
            s.img,
            s.page,
            &v,
            bits,
            bw as u32,
            bh as u32,
            bw as u32 * 4,
            0xFF_FFFF,
            0,
        );
        let mut name = [0u16; NAME_CAP];
        let n = file_name(s);
        for (i, &c) in cstr(&n).iter().take(NAME_CAP - 1).enumerate() {
            name[i] = c as u16;
        }
        // DOCINFOW x64 : cbSize, lpszDocName, lpszOutput, lpszDatatype, fwType.
        let mut di = [0u8; 40];
        di[0..4].copy_from_slice(&40i32.to_le_bytes());
        di[8..16].copy_from_slice(&(name.as_ptr() as usize).to_le_bytes());
        let info = BitmapInfoHeader {
            size: 40,
            width: bw,
            height: -bh,
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        };
        ok = start_doc(hdc, di.as_ptr()) > 0 && start_page(hdc) > 0;
        if ok {
            StretchDIBits(
                hdc,
                (pw - dw) / 2,
                (ph - dh) / 2,
                dw,
                dh,
                0,
                0,
                bw,
                bh,
                bits as *const core::ffi::c_void,
                &info,
                0,
                0x00CC_0020,
            );
            ok = end_page(hdc) > 0;
            ok &= end_doc(hdc) > 0;
        }
        VirtualFree(bits as *mut core::ffi::c_void, 0, 0x8000);
    }
    DeleteDC(hdc);
    if !ok {
        s.message = t(PRINT_FAILED);
    }
}
