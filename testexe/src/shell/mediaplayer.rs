//! Lecteur multimédia natif (vidéo et audio), style Windows 11.
//!
//! Le décodage est fait par le moteur multimédia du runtime
//! (`WinluxMedia*`, voir runtime/src/dlls/winlux_media.rs) ; ce module ne
//! gère que l'interface : barre de progression avec aperçu au survol,
//! volume, vitesse, plein écran, pistes audio, sous-titres intégrés ou
//! externes, chapitres, liste de lecture et reprise de lecture.

#![allow(static_mut_refs)]

use crate::gfx::*;
use winlux_testexe::*;

// ---------------------------------------------------------------------------
// Textes localisés (français, anglais, allemand, espagnol ; Windows-1252)
// ---------------------------------------------------------------------------

const S_TITLE: usize = 0;
const S_EMPTY: usize = 1;
const S_FORMATS: usize = 2;
const S_NO_ENGINE: usize = 3;
const S_HW: usize = 4;
const S_SOFTWARE: usize = 5;
const S_UNSUPPORTED: usize = 6;
const S_MISSING: usize = 7;
const S_UNREADABLE: usize = 8;
const S_RESUMED: usize = 9;
const S_SPEED: usize = 10;
const S_AUDIO: usize = 11;
const S_SUBS: usize = 12;
const S_OFF: usize = 13;
const S_CHAPTERS: usize = 14;
const S_PLAYLIST: usize = 15;
const S_ERROR: usize = 16;
const S_NO_AUDIO: usize = 17;
const S_TRACK: usize = 18;
const S_CHAPTER: usize = 19;
const S_KEYS: usize = 20;
const S_EMPTY_LIST: usize = 21;

const STRINGS: [[&[u8]; 4]; 22] = [
    [
        b"Lecteur multim\xE9dia",
        b"Media Player",
        b"Medienwiedergabe",
        b"Reproductor multimedia",
    ],
    [
        b"Ouvrez une vid\xE9o ou un morceau depuis l'Explorateur de fichiers",
        b"Open a video or a song from File Explorer",
        b"\xD6ffnen Sie ein Video oder einen Titel im Explorer",
        b"Abra un v\xEDdeo o una canci\xF3n desde el Explorador de archivos",
    ],
    [
        b"Formats d\xE9codables sur cet ordinateur",
        b"Formats this PC can decode",
        b"Auf diesem PC dekodierbare Formate",
        b"Formatos que este equipo puede decodificar",
    ],
    [
        b"Moteur multim\xE9dia absent : installez le paquet ffmpeg",
        b"Media engine missing: install the ffmpeg package",
        b"Medien-Engine fehlt: Paket ffmpeg installieren",
        b"Falta el motor multimedia: instale el paquete ffmpeg",
    ],
    [
        b"D\xE9codage mat\xE9riel : ",
        b"Hardware decoding: ",
        b"Hardware-Dekodierung: ",
        b"Decodificaci\xF3n por hardware: ",
    ],
    [
        b"indisponible (d\xE9codage logiciel)",
        b"unavailable (software decoding)",
        b"nicht verf\xFCgbar (Software-Dekodierung)",
        b"no disponible (decodificaci\xF3n por software)",
    ],
    [
        b"Format non pris en charge sur cet ordinateur",
        b"This format is not supported on this PC",
        b"Dieses Format wird auf diesem PC nicht unterst\xFCtzt",
        b"Este formato no es compatible con este equipo",
    ],
    [
        b"D\xE9codeur manquant : ",
        b"Missing decoder: ",
        b"Fehlender Decoder: ",
        b"Falta el decodificador: ",
    ],
    [
        b"Fichier illisible ou endommag\xE9",
        b"Unreadable or damaged file",
        b"Datei unlesbar oder besch\xE4\x64igt",
        b"Archivo ilegible o da\xF1\x61\x64o",
    ],
    [
        b"Lecture reprise \xE0 ",
        b"Resumed at ",
        b"Fortgesetzt bei ",
        b"Reanudado en ",
    ],
    [b"Vitesse", b"Speed", b"Geschwindigkeit", b"Velocidad"],
    [b"Piste audio", b"Audio track", b"Audiospur", b"Pista de audio"],
    [b"Sous-titres", b"Subtitles", b"Untertitel", b"Subt\xEDtulos"],
    [b"D\xE9sactiv\xE9s", b"Off", b"Aus", b"Desactivados"],
    [b"Chapitres", b"Chapters", b"Kapitel", b"Cap\xEDtulos"],
    [
        b"Liste de lecture",
        b"Playlist",
        b"Wiedergabeliste",
        b"Lista de reproducci\xF3n",
    ],
    [
        b"Erreur de lecture : ",
        b"Playback error: ",
        b"Wiedergabefehler: ",
        b"Error de reproducci\xF3n: ",
    ],
    [
        b"Sortie audio indisponible : lecture sans son",
        b"Audio output unavailable: playing without sound",
        b"Audioausgabe nicht verf\xFCgbar: Wiedergabe ohne Ton",
        b"Salida de audio no disponible: reproducci\xF3n sin sonido",
    ],
    [b"Piste ", b"Track ", b"Spur ", b"Pista "],
    [b"Chapitre ", b"Chapter ", b"Kapitel ", b"Cap\xEDtulo "],
    [
        b"Espace : lecture/pause \xB7 F : plein \xE9\x63ran \xB7 Fl\xE8\x63hes : 5 s / volume \xB7 M : muet",
        b"Space: play/pause \xB7 F: full screen \xB7 Arrows: 5 s / volume \xB7 M: mute",
        b"Leertaste: Wiedergabe/Pause \xB7 F: Vollbild \xB7 Pfeile: 5 s / Lautst\xE4rke \xB7 M: stumm",
        b"Espacio: reproducir/pausa \xB7 F: pantalla completa \xB7 Flechas: 5 s / volumen \xB7 M: silencio",
    ],
    [
        b"Aucun autre fichier dans ce dossier",
        b"No other file in this folder",
        b"Keine weiteren Dateien in diesem Ordner",
        b"No hay otros archivos en esta carpeta",
    ],
];

static mut LANG: usize = 0;

fn tr(id: usize) -> &'static [u8] {
    unsafe { STRINGS[id][LANG.min(3)] }
}

// ---------------------------------------------------------------------------
// Petits utilitaires sans allocation
// ---------------------------------------------------------------------------

/// Tampon de texte fixe.
#[derive(Clone, Copy)]
pub struct Buf<const N: usize> {
    pub b: [u8; N],
    pub n: usize,
}

impl<const N: usize> Buf<N> {
    pub const fn new() -> Self {
        Buf { b: [0; N], n: 0 }
    }
    pub fn clear(&mut self) {
        self.n = 0;
    }
    pub fn push(&mut self, s: &[u8]) -> &mut Self {
        for &c in s {
            if self.n < N {
                self.b[self.n] = c;
                self.n += 1;
            }
        }
        self
    }
    pub fn num(&mut self, mut v: u32) -> &mut Self {
        let mut tmp = [0u8; 10];
        let mut i = tmp.len();
        loop {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.push(&tmp[i..])
    }
    pub fn time(&mut self, ms: u32) -> &mut Self {
        let s = ms / 1000;
        let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
        if h > 0 {
            self.num(h).push(b":");
            if m < 10 {
                self.push(b"0");
            }
        }
        self.num(m).push(b":");
        if s < 10 {
            self.push(b"0");
        }
        self.num(s)
    }
    pub fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

/// Chemin Windows en UTF-16, terminé par zéro.
#[derive(Clone, Copy)]
pub struct WPath {
    pub w: [u16; 512],
    pub n: usize,
}

impl WPath {
    pub const fn empty() -> Self {
        WPath { w: [0; 512], n: 0 }
    }
    /// Depuis des octets Windows-1252 (code page du shell).
    pub fn from_bytes(s: &[u8]) -> Self {
        let mut p = WPath::empty();
        for &c in s.iter().take(511) {
            p.w[p.n] = c as u16;
            p.n += 1;
        }
        p.w[p.n] = 0;
        p
    }
    pub fn from_wide(s: &[u16]) -> Self {
        let mut p = WPath::empty();
        for &c in s.iter().take_while(|&&c| c != 0).take(511) {
            p.w[p.n] = c;
            p.n += 1;
        }
        p.w[p.n] = 0;
        p
    }
    pub fn ptr(&self) -> *const u16 {
        self.w.as_ptr()
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    /// Nom de fichier affichable (Windows-1252, « ? » hors plage).
    pub fn file_name(&self, out: &mut Buf<160>) {
        out.clear();
        let start = self.w[..self.n]
            .iter()
            .rposition(|&c| c == b'\\' as u16 || c == b'/' as u16)
            .map(|i| i + 1)
            .unwrap_or(0);
        for &c in &self.w[start..self.n] {
            out.push(&[if c < 256 { c as u8 } else { b'?' }]);
        }
    }
    /// Égalité de chemins Windows : casse et séparateurs ignorés.
    pub fn eq(&self, o: &WPath) -> bool {
        let norm = |c: u16| {
            if c == b'/' as u16 {
                b'\\' as u16
            } else if (b'A' as u16..=b'Z' as u16).contains(&c) {
                c + 32
            } else {
                c
            }
        };
        self.n == o.n
            && self.w[..self.n]
                .iter()
                .zip(o.w[..o.n].iter())
                .all(|(&a, &b)| norm(a) == norm(b))
    }
}

fn split_tab(line: &[u8], i: usize) -> &[u8] {
    line.split(|&c| c == b'\t').nth(i).unwrap_or(&[])
}

fn parse_u32(s: &[u8]) -> u32 {
    s.iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u32, |n, &c| {
            n.saturating_mul(10).saturating_add((c - b'0') as u32)
        })
}

// ---------------------------------------------------------------------------
// État
// ---------------------------------------------------------------------------

const MAX_TRACKS: usize = 16;
const MAX_SUBS: usize = 24;
const MAX_EXT_SUBS: usize = 8;
const MAX_CHAPTERS: usize = 128;
const MAX_PLAYLIST: usize = 256;
const FRAME_CAP: usize = 1920 * 1080 * 4;
const PREVIEW_W: u32 = 192;
const PREVIEW_H: u32 = 108;
const COVER: u32 = 256;

#[derive(Clone, Copy)]
struct Track {
    ordinal: i32,
    label: Buf<64>,
    ok: bool,
}
const NO_TRACK: Track = Track {
    ordinal: -1,
    label: Buf::new(),
    ok: false,
};

#[derive(Clone, Copy)]
struct Chapter {
    ms: u32,
    title: Buf<48>,
}
const NO_CHAPTER: Chapter = Chapter {
    ms: 0,
    title: Buf::new(),
};

#[derive(Clone, Copy, PartialEq)]
enum SubSel {
    Off,
    Embedded(i32),
    External(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum Menu {
    None,
    Speed,
    Audio,
    Subs,
    Chapters,
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    None,
    Seek,
    Volume,
    Play,
    Prev,
    Next,
    Back10,
    Fwd30,
    Speed,
    Audio,
    Subs,
    Chapters,
    Mute,
    Fullscreen,
    Playlist,
    MenuItem(usize),
    ListItem(usize),
}

static mut HWND: Handle = core::ptr::null_mut();
static mut PATH: WPath = WPath::empty();
static mut LOADED: bool = false;
static mut ENGINE: bool = true;
static mut FORMATS: Buf<2048> = Buf::new();

// Informations du fichier.
static mut DURATION: u32 = 0;
static mut HAS_VIDEO: bool = false;
static mut HAS_COVER: bool = false;
static mut PLAYABLE: bool = false;
static mut MISSING: Buf<128> = Buf::new();
static mut TITLE: Buf<160> = Buf::new();
static mut ARTIST: Buf<128> = Buf::new();
static mut VIDEO_LABEL: Buf<64> = Buf::new();
static mut AUDIO_TRACKS: [Track; MAX_TRACKS] = [NO_TRACK; MAX_TRACKS];
static mut AUDIO_COUNT: usize = 0;
static mut SUB_TRACKS: [Track; MAX_SUBS] = [NO_TRACK; MAX_SUBS];
static mut SUB_COUNT: usize = 0;
static mut EXT_SUBS: [WPath; MAX_EXT_SUBS] = [WPath::empty(); MAX_EXT_SUBS];
static mut EXT_COUNT: usize = 0;
static mut CHAPTERS: [Chapter; MAX_CHAPTERS] = [NO_CHAPTER; MAX_CHAPTERS];
static mut CHAPTER_COUNT: usize = 0;
static mut PLAYLIST: [WPath; MAX_PLAYLIST] = [WPath::empty(); MAX_PLAYLIST];
static mut PLAYLIST_COUNT: usize = 0;
static mut CURRENT: usize = 0;

// Lecture.
static mut SESSION: usize = 0;
static mut PLAYING: bool = false;
/// Session lancée seulement pour afficher une image (pause après recherche).
static mut STILL: bool = false;
static mut POSITION: u32 = 0;
static mut VOLUME: u32 = 80;
static mut MUTED: bool = false;
static mut SPEED: u32 = 100;
static mut AUDIO_SEL: i32 = -1;
static mut SUB_SEL: SubSel = SubSel::Off;
static mut STATE: MediaState = MediaState {
    position_ms: 0,
    status: 0,
    frame_seq: 0,
    width: 0,
    height: 0,
    hw_active: 0,
    audio_active: 0,
    reserved: 0,
};
static mut FRAME: [u8; FRAME_CAP] = [0; FRAME_CAP];
static mut FRAME_W: u32 = 0;
static mut FRAME_H: u32 = 0;
static mut FRAME_SEQ: u32 = 0;
static mut DECODE_W: u32 = 0;
static mut DECODE_H: u32 = 0;
static mut LAST_SAVE: u32 = 0;
static mut PENDING_RESTART: u32 = 0;
static mut RESIZED_AT: u32 = 0;
static mut AUDIO_WARNED: bool = false;

// Interface.
static mut HOVER: Hit = Hit::None;
static mut PRESSED: Hit = Hit::None;
static mut MOUSE_X: i32 = -1;
static mut MOUSE_Y: i32 = -1;
static mut MENU: Menu = Menu::None;
static mut SHOW_LIST: bool = false;
static mut LIST_SCROLL: usize = 0;
static mut FULLSCREEN: bool = false;
static mut SAVED_STYLE: i32 = 0;
static mut SAVED_RECT: Rect = Rect {
    left: 0,
    top: 0,
    right: 0,
    bottom: 0,
};
static mut LAST_MOVE: u32 = 0;
static mut LAST_CLICK: u32 = 0;
static mut TOAST: Buf<200> = Buf::new();
static mut TOAST_UNTIL: u32 = 0;
static mut PREVIEW: [u8; (PREVIEW_W * PREVIEW_H * 4) as usize] =
    [0; (PREVIEW_W * PREVIEW_H * 4) as usize];
static mut PREVIEW_AT: u32 = u32::MAX;
static mut PREVIEW_READY: bool = false;
static mut COVER_IMG: [u8; (COVER * COVER * 4) as usize] = [0; (COVER * COVER * 4) as usize];
static mut COVER_STATE: i32 = -1;
static mut LAST_PREFS: u32 = 0;

const SPEEDS: [u32; 8] = [25, 50, 75, 100, 125, 150, 200, 300];
const BAR_H: i32 = 96;
const LIST_W: i32 = 300;

// ---------------------------------------------------------------------------
// Préférences du bureau (thème, langue)
// ---------------------------------------------------------------------------

unsafe fn read_prefs() {
    let h = CreateFileA(
        c"Z:\\run\\winlux\\control.txt".as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        return;
    }
    let mut buf = [0u8; 8192];
    let mut got = 0u32;
    ReadFile(
        h,
        buf.as_mut_ptr(),
        buf.len() as u32,
        &mut got,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    for line in buf[..got as usize].split(|&c| c == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(v) = line.strip_prefix(b"theme=") {
            match v {
                b"light" => set_theme(false),
                b"dark" => set_theme(true),
                _ => {}
            }
        } else if let Some(v) = line.strip_prefix(b"locale_lang=") {
            LANG = match v {
                b"en" => 1,
                b"de" => 2,
                b"es" => 3,
                _ => 0,
            };
        }
    }
    check_auto_theme();
}

// ---------------------------------------------------------------------------
// Chargement d'un fichier
// ---------------------------------------------------------------------------

unsafe fn toast(a: &[u8], b: &[u8]) {
    TOAST.clear();
    TOAST.push(a).push(b);
    TOAST_UNTIL = GetTickCount().wrapping_add(4000);
}

unsafe fn load_formats() {
    let mut raw = [0u8; 2048];
    let n = WinluxMediaFormats(raw.as_mut_ptr(), raw.len() as u32);
    FORMATS.clear();
    if n > 0 {
        FORMATS.push(&raw[..n as usize]);
    }
    ENGINE = FORMATS
        .get()
        .split(|&c| c == b'\n')
        .any(|l| l == b"engine\t1");
}

unsafe fn parse_info(text: &[u8]) {
    DURATION = 0;
    HAS_VIDEO = false;
    HAS_COVER = false;
    PLAYABLE = false;
    MISSING.clear();
    TITLE.clear();
    ARTIST.clear();
    VIDEO_LABEL.clear();
    AUDIO_COUNT = 0;
    SUB_COUNT = 0;
    CHAPTER_COUNT = 0;
    for line in text.split(|&c| c == b'\n') {
        let key = split_tab(line, 0);
        let f = |i| split_tab(line, i);
        match key {
            b"duration" => DURATION = parse_u32(f(1)),
            b"playable" => PLAYABLE = f(1) == b"1",
            b"missing" => {
                MISSING.push(f(1));
            }
            b"title" => {
                TITLE.push(f(1));
            }
            b"artist" => {
                ARTIST.push(f(1));
            }
            b"cover" => HAS_COVER = f(1) == b"1",
            b"video" => {
                HAS_VIDEO = f(5) == b"1";
                VIDEO_LABEL
                    .push(f(1))
                    .push(b" \xB7 ")
                    .push(f(2))
                    .push(b"\xD7")
                    .push(f(3));
            }
            b"audio" if AUDIO_COUNT < MAX_TRACKS => {
                let t = &mut AUDIO_TRACKS[AUDIO_COUNT];
                t.ordinal = parse_u32(f(1)) as i32;
                t.ok = f(5) == b"1";
                t.label.clear();
                t.label.push(tr(S_TRACK)).num(t.ordinal as u32 + 1);
                if !f(6).is_empty() {
                    t.label.push(b" \xB7 ").push(f(6));
                }
                if !f(4).is_empty() {
                    t.label.push(b" \xB7 ").push(f(4));
                }
                t.label.push(b" \xB7 ").push(f(2));
                let ch = parse_u32(f(3));
                if ch == 6 {
                    t.label.push(b" 5.1");
                } else if ch == 8 {
                    t.label.push(b" 7.1");
                } else if ch == 1 {
                    t.label.push(b" mono");
                }
                AUDIO_COUNT += 1;
            }
            b"sub" if SUB_COUNT < MAX_SUBS => {
                let t = &mut SUB_TRACKS[SUB_COUNT];
                t.ordinal = parse_u32(f(1)) as i32;
                t.ok = true;
                t.label.clear();
                t.label.push(tr(S_TRACK)).num(t.ordinal as u32 + 1);
                if !f(5).is_empty() {
                    t.label.push(b" \xB7 ").push(f(5));
                }
                if !f(3).is_empty() {
                    t.label.push(b" \xB7 ").push(f(3));
                }
                t.label.push(b" \xB7 ").push(f(2));
                SUB_COUNT += 1;
            }
            b"chapter" if CHAPTER_COUNT < MAX_CHAPTERS => {
                let c = &mut CHAPTERS[CHAPTER_COUNT];
                c.ms = parse_u32(f(1));
                c.title.clear();
                if f(2).is_empty() {
                    c.title.push(tr(S_CHAPTER)).num(CHAPTER_COUNT as u32 + 1);
                } else {
                    c.title.push(f(2));
                }
                CHAPTER_COUNT += 1;
            }
            _ => {}
        }
    }
}

/// Lit une liste de chemins (un par ligne) renvoyée par le runtime.
unsafe fn read_paths(
    f: unsafe extern "C" fn(*const u16, *mut u16, u32) -> i32,
    out: &mut [WPath],
) -> usize {
    static mut RAW: [u16; 65536] = [0; 65536];
    let raw = &mut *core::ptr::addr_of_mut!(RAW);
    let n = f(PATH.ptr(), raw.as_mut_ptr(), raw.len() as u32);
    if n <= 0 {
        return 0;
    }
    let mut count = 0;
    for line in raw[..n as usize].split(|&c| c == b'\n' as u16) {
        if line.is_empty() || count >= out.len() {
            continue;
        }
        out[count] = WPath::from_wide(line);
        count += 1;
    }
    count
}

unsafe extern "C" fn siblings_fn(p: *const u16, o: *mut u16, c: u32) -> i32 {
    WinluxMediaSiblings(p, o, c)
}
unsafe extern "C" fn subtitles_fn(p: *const u16, o: *mut u16, c: u32) -> i32 {
    WinluxMediaSubtitles(p, o, c)
}

/// Ouvre un fichier et démarre la lecture (reprise à la dernière position).
pub unsafe fn open_file(path: &WPath, refresh_list: bool) {
    stop_session();
    save_resume();
    // Chemin canonique (C:\… ou Z:\…) : la liste de lecture et la reprise
    // reconnaissent le même fichier quelle que soit la forme reçue.
    let mut canon = [0u16; 512];
    let n = WinluxMediaCanonical(path.ptr(), canon.as_mut_ptr(), canon.len() as u32);
    PATH = if n > 0 {
        WPath::from_wide(&canon[..n as usize])
    } else {
        *path
    };
    LOADED = true;
    PLAYING = false;
    FRAME_W = 0;
    FRAME_H = 0;
    POSITION = 0;
    AUDIO_SEL = -1;
    SUB_SEL = SubSel::Off;
    MENU = Menu::None;
    PREVIEW_AT = u32::MAX;
    COVER_STATE = -1;
    AUDIO_WARNED = false;
    let mut raw = [0u8; 16384];
    let n = WinluxMediaProbe(PATH.ptr(), raw.as_mut_ptr(), raw.len() as u32);
    let mut name = Buf::<160>::new();
    PATH.file_name(&mut name);
    set_window_title(name.get());
    if n < 0 {
        parse_info(b"");
        toast(tr(S_UNREADABLE), b"");
    } else {
        parse_info(&raw[..n as usize]);
    }
    if TITLE.n == 0 {
        TITLE.push(name.get());
    }
    EXT_COUNT = read_paths(subtitles_fn, &mut *core::ptr::addr_of_mut!(EXT_SUBS));
    if refresh_list {
        PLAYLIST_COUNT = read_paths(siblings_fn, &mut *core::ptr::addr_of_mut!(PLAYLIST));
    }
    CURRENT = (0..PLAYLIST_COUNT)
        .find(|&i| PLAYLIST[i].eq(&PATH))
        .unwrap_or(0);
    if !PLAYABLE {
        return;
    }
    // Sous-titres : piste « par défaut » non activée d'office, comme Windows.
    let resume = WinluxMediaGetResume(PATH.ptr());
    if resume > 0 && (DURATION == 0 || resume < DURATION) {
        let mut b = Buf::<16>::new();
        b.time(resume);
        toast(tr(S_RESUMED), b.get());
    }
    start(resume, false);
}

unsafe fn set_window_title(name: &[u8]) {
    if HWND.is_null() {
        return;
    }
    let mut w = [0u16; 260];
    let mut n = 0;
    for &c in name.iter().chain(b" - ".iter()).chain(tr(S_TITLE).iter()) {
        if n < w.len() - 1 {
            w[n] = c as u16;
            n += 1;
        }
    }
    SetWindowTextW(HWND, w.as_ptr());
}

// ---------------------------------------------------------------------------
// Sessions de lecture
// ---------------------------------------------------------------------------

unsafe fn stop_session() {
    if SESSION != 0 {
        WinluxMediaClose(SESSION);
        SESSION = 0;
    }
}

unsafe fn current_position() -> u32 {
    if SESSION != 0 && !STILL {
        STATE.position_ms
    } else {
        POSITION
    }
}

unsafe fn save_resume() {
    if LOADED && !PATH.is_empty() && DURATION > 0 {
        WinluxMediaSetResume(PATH.ptr(), current_position(), DURATION);
    }
}

/// Démarre (ou redémarre) le décodage à `pos`. `still` : une seule image.
unsafe fn start(pos: u32, still: bool) {
    stop_session();
    let pos = if DURATION > 0 {
        pos.min(DURATION.saturating_sub(250))
    } else {
        pos
    };
    POSITION = pos;
    STILL = still;
    let area = video_area_now();
    DECODE_W = ((area.right - area.left).max(64) as u32).min(1920);
    DECODE_H = ((area.bottom - area.top).max(64) as u32).min(1080);
    let ext = match SUB_SEL {
        SubSel::External(i) if i < EXT_COUNT => EXT_SUBS[i].ptr(),
        _ => core::ptr::null(),
    };
    let opts = MediaOpenOptions {
        start_ms: pos,
        width: DECODE_W,
        height: DECODE_H,
        volume: if MUTED { 0 } else { VOLUME },
        speed: SPEED,
        audio_track: if still { -2 } else { AUDIO_SEL },
        sub_track: match SUB_SEL {
            SubSel::Embedded(n) => n,
            _ => -1,
        },
        flags: if HAS_VIDEO { 0 } else { 1 },
        sub_path: ext,
    };
    SESSION = WinluxMediaOpen(PATH.ptr(), &opts);
    STATE = MediaState::default();
    STATE.position_ms = pos;
    PLAYING = SESSION != 0 && !still;
    if SESSION == 0 && !still {
        toast(tr(S_ERROR), tr(S_UNREADABLE));
    }
    LAST_SAVE = GetTickCount();
}

unsafe fn play_pause() {
    if !LOADED || !PLAYABLE {
        return;
    }
    if PLAYING {
        POSITION = current_position();
        stop_session();
        PLAYING = false;
        save_resume();
    } else {
        let pos = if DURATION > 0 && POSITION + 500 >= DURATION {
            0
        } else {
            POSITION
        };
        start(pos, false);
    }
}

unsafe fn seek(ms: u32) {
    if !LOADED || !PLAYABLE {
        return;
    }
    if PLAYING {
        start(ms, false);
    } else {
        // En pause : on affiche l'image demandée sans relancer le son.
        start(ms, HAS_VIDEO);
        if !HAS_VIDEO {
            stop_session();
            POSITION = ms;
        }
    }
}

unsafe fn restart_soon() {
    if PLAYING {
        PENDING_RESTART = GetTickCount().wrapping_add(250);
    }
}

unsafe fn next(delta: i32) {
    if PLAYLIST_COUNT == 0 {
        return;
    }
    if delta < 0 && current_position() > 3000 {
        seek(0);
        return;
    }
    let i = CURRENT as i32 + delta;
    if i < 0 || i as usize >= PLAYLIST_COUNT {
        return;
    }
    let p = PLAYLIST[i as usize];
    open_file(&p, false);
}

/// Appelé ~60 fois par seconde ; vrai s'il faut repeindre.
pub unsafe fn tick() -> bool {
    let now = GetTickCount();
    let mut dirty = false;
    if LAST_PREFS == 0 || now.wrapping_sub(LAST_PREFS) > 2000 {
        LAST_PREFS = now;
        read_prefs();
    }
    if theme_animating() {
        dirty = true;
    }
    if PENDING_RESTART != 0 && (now.wrapping_sub(PENDING_RESTART) as i32) >= 0 {
        PENDING_RESTART = 0;
        if PLAYING {
            start(current_position(), false);
        }
    }
    // Redimensionnement important : on redécode à la nouvelle taille.
    if RESIZED_AT != 0 && now.wrapping_sub(RESIZED_AT) > 500 {
        RESIZED_AT = 0;
        let a = video_area_now();
        let (w, h) = ((a.right - a.left) as u32, (a.bottom - a.top) as u32);
        if PLAYING && HAS_VIDEO && (w > DECODE_W * 5 / 4 || h > DECODE_H * 5 / 4) && DECODE_W < 1920
        {
            start(current_position(), false);
        }
    }
    if SESSION != 0 {
        let mut st = MediaState::default();
        if WinluxMediaState(SESSION, &mut st) != 0 {
            let prev = STATE.position_ms / 1000;
            STATE = st;
            if st.position_ms / 1000 != prev {
                dirty = true;
            }
            if st.frame_seq != FRAME_SEQ && st.width > 0 {
                let need = (st.width * st.height * 4) as usize;
                if need <= FRAME_CAP {
                    let seq = WinluxMediaFrame(
                        SESSION,
                        core::ptr::addr_of_mut!(FRAME) as *mut u8,
                        FRAME_CAP as u32,
                    );
                    if seq != 0 {
                        FRAME_SEQ = st.frame_seq;
                        FRAME_W = st.width;
                        FRAME_H = st.height;
                        dirty = true;
                        if STILL {
                            stop_session();
                        }
                    }
                }
            }
            if SESSION != 0 && st.status == 1 && st.audio_active == 0 && !STILL && !AUDIO_WARNED {
                let has_audio = (0..AUDIO_COUNT).any(|i| AUDIO_TRACKS[i].ok);
                if has_audio && AUDIO_SEL != -2 {
                    AUDIO_WARNED = true;
                    toast(tr(S_NO_AUDIO), b"");
                }
            }
            if SESSION != 0 && st.status >= 2 {
                let pos = st.position_ms;
                let failed = st.status == 3;
                if failed {
                    let mut e = [0u8; 160];
                    let n = WinluxMediaError(SESSION, e.as_mut_ptr(), e.len() as u32).max(0);
                    toast(tr(S_ERROR), &e[..n as usize]);
                }
                stop_session();
                if STILL {
                    STILL = false;
                } else {
                    PLAYING = false;
                    POSITION = if failed { pos } else { DURATION.max(pos) };
                    save_resume();
                    if !failed && CURRENT + 1 < PLAYLIST_COUNT {
                        next(1);
                    }
                }
                dirty = true;
            }
        }
        if PLAYING && now.wrapping_sub(LAST_SAVE) > 5000 {
            LAST_SAVE = now;
            save_resume();
        }
    }
    if PLAYING {
        dirty = true;
    }
    // Aperçu de survol en cours de génération.
    if HOVER == Hit::Seek && !PREVIEW_READY && PREVIEW_AT != u32::MAX {
        dirty = true;
    }
    if LOADED && HAS_COVER && !HAS_VIDEO && COVER_STATE == 0 {
        dirty = true;
    }
    if TOAST_UNTIL != 0 && (now.wrapping_sub(TOAST_UNTIL) as i32) >= 0 {
        TOAST_UNTIL = 0;
        dirty = true;
    }
    if FULLSCREEN
        && PLAYING
        && now.wrapping_sub(LAST_MOVE) > 2500
        && now.wrapping_sub(LAST_MOVE) < 2600
    {
        dirty = true;
    }
    dirty
}

// ---------------------------------------------------------------------------
// Mise en page
// ---------------------------------------------------------------------------

unsafe fn client() -> Rect {
    let mut r = Rect::default();
    if !HWND.is_null() {
        GetClientRect(HWND, &mut r);
    }
    r
}

unsafe fn bar_hidden() -> bool {
    FULLSCREEN && PLAYING && MENU == Menu::None && GetTickCount().wrapping_sub(LAST_MOVE) > 2500
}

unsafe fn list_rect(r: &Rect) -> Option<Rect> {
    (SHOW_LIST && !FULLSCREEN).then(|| rect(r.right - LIST_W, r.top, r.right, r.bottom))
}

unsafe fn content_rect(r: &Rect) -> Rect {
    match list_rect(r) {
        Some(l) => rect(r.left, r.top, l.left, r.bottom),
        None => *r,
    }
}

unsafe fn bar_rect(r: &Rect) -> Rect {
    let c = content_rect(r);
    rect(c.left, c.bottom - BAR_H, c.right, c.bottom)
}

unsafe fn video_area(r: &Rect) -> Rect {
    let c = content_rect(r);
    if bar_hidden() {
        c
    } else {
        rect(c.left, c.top, c.right, c.bottom - BAR_H)
    }
}

unsafe fn video_area_now() -> Rect {
    let r = client();
    if r.right <= r.left {
        return rect(0, 0, 960, 540);
    }
    let c = content_rect(&r);
    if FULLSCREEN {
        c
    } else {
        rect(c.left, c.top, c.right, c.bottom - BAR_H)
    }
}

fn seek_track(b: &Rect) -> Rect {
    rect(b.left + 76, b.top + 14, b.right - 76, b.top + 30)
}

/// Boutons de la barre : (cible, rectangle). Le groupe central se décale,
/// puis perd ses boutons -10/+30, quand la fenêtre est étroite.
unsafe fn buttons(b: &Rect) -> [(Hit, Rect); 14] {
    let y = b.top + 44;
    let s = 40;
    let btn = |x: i32| rect(x, y, x + s, y + s);
    let r = b.right - 12;
    let right_left = r - 3 * s - 104 - s - 64;
    let left_right = b.left + 104;
    let wide = right_left - left_right >= 4 * s + 120;
    let half = if wide { 2 * s + 30 } else { s + 30 };
    let cx = ((b.left + b.right) / 2).clamp(
        left_right + half,
        (right_left - half).max(left_right + half),
    );
    let none = rect(0, 0, 0, 0);
    [
        (Hit::Playlist, btn(b.left + 12)),
        (Hit::Chapters, rect(b.left + 56, y, b.left + 56 + 44, y + s)),
        (
            Hit::Prev,
            if wide {
                btn(cx - 2 * s - 28)
            } else {
                btn(cx - s - 28)
            },
        ),
        (Hit::Back10, if wide { btn(cx - s - 24) } else { none }),
        (Hit::Play, rect(cx - 24, y - 4, cx + 24, y + s + 4)),
        (Hit::Fwd30, if wide { btn(cx + 24 + 4) } else { none }),
        (
            Hit::Next,
            if wide { btn(cx + s + 32) } else { btn(cx + 28) },
        ),
        (Hit::Fullscreen, btn(r - s)),
        (Hit::Volume, rect(r - s - 104, y + 14, r - s - 8, y + 26)),
        (Hit::Mute, btn(r - s - 104 - s)),
        (Hit::Subs, btn(r - 2 * s - 104 - s - 4)),
        (Hit::Audio, btn(r - 3 * s - 104 - s - 8)),
        (
            Hit::Speed,
            rect(right_left, y, r - 3 * s - 104 - s - 12, y + s),
        ),
        (Hit::None, none),
    ]
}

unsafe fn menu_items(out: &mut [Buf<64>; 32]) -> usize {
    let mut n = 0;
    let mut add = |s: &[u8], out: &mut [Buf<64>; 32]| {
        if n < out.len() {
            out[n].clear();
            out[n].push(s);
            n += 1;
        }
    };
    match MENU {
        Menu::Speed => {
            for sp in SPEEDS {
                let mut b = Buf::<64>::new();
                b.num(sp / 100);
                if sp % 100 != 0 {
                    b.push(b",")
                        .num((sp % 100) / if sp % 10 == 0 { 10 } else { 1 });
                }
                b.push(b"\xD7");
                add(b.get(), out);
            }
        }
        Menu::Audio => {
            for i in 0..AUDIO_COUNT {
                let l = AUDIO_TRACKS[i].label;
                add(l.get(), out);
            }
        }
        Menu::Subs => {
            add(tr(S_OFF), out);
            for i in 0..SUB_COUNT {
                let l = SUB_TRACKS[i].label;
                add(l.get(), out);
            }
            for i in 0..EXT_COUNT {
                let mut name = Buf::<160>::new();
                EXT_SUBS[i].file_name(&mut name);
                add(name.get(), out);
            }
        }
        Menu::Chapters => {
            for i in 0..CHAPTER_COUNT.min(32) {
                let mut b = Buf::<64>::new();
                b.time(CHAPTERS[i].ms)
                    .push(b"  ")
                    .push(CHAPTERS[i].title.get());
                add(b.get(), out);
            }
        }
        Menu::None => {}
    }
    n
}

unsafe fn menu_selected(i: usize) -> bool {
    match MENU {
        Menu::Speed => SPEEDS[i] == SPEED,
        Menu::Audio => {
            let cur = if AUDIO_SEL >= 0 {
                AUDIO_SEL
            } else {
                (0..AUDIO_COUNT)
                    .find(|&k| AUDIO_TRACKS[k].ok)
                    .map(|k| AUDIO_TRACKS[k].ordinal)
                    .unwrap_or(-1)
            };
            AUDIO_TRACKS[i].ordinal == cur
        }
        Menu::Subs => match SUB_SEL {
            SubSel::Off => i == 0,
            SubSel::Embedded(n) => i >= 1 && i <= SUB_COUNT && SUB_TRACKS[i - 1].ordinal == n,
            SubSel::External(k) => i == 1 + SUB_COUNT + k,
        },
        Menu::Chapters => current_chapter() == Some(i),
        Menu::None => false,
    }
}

unsafe fn current_chapter() -> Option<usize> {
    let pos = current_position();
    (0..CHAPTER_COUNT).rev().find(|&i| CHAPTERS[i].ms <= pos)
}

unsafe fn menu_anchor(b: &Rect) -> i32 {
    let target = match MENU {
        Menu::Speed => Hit::Speed,
        Menu::Audio => Hit::Audio,
        Menu::Subs => Hit::Subs,
        Menu::Chapters => Hit::Chapters,
        Menu::None => return 0,
    };
    buttons(b)
        .iter()
        .find(|(h, _)| *h == target)
        .map(|(_, r)| r.left)
        .unwrap_or(b.left)
}

unsafe fn menu_rect(r: &Rect, count: usize) -> Rect {
    let b = bar_rect(r);
    let w = 300;
    let h = count as i32 * 32 + 12;
    let x = menu_anchor(&b).min(b.right - w - 8).max(b.left + 8);
    let top = (b.top - 8 - h).max(r.top + 8);
    rect(x, top, x + w, b.top - 8)
}

unsafe fn hit_test(x: i32, y: i32) -> Hit {
    let r = client();
    if MENU != Menu::None {
        let mut items = [Buf::<64>::new(); 32];
        let n = menu_items(&mut items);
        let m = menu_rect(&r, n);
        if inside(&m, x, y) {
            let i = ((y - m.top - 6) / 32).max(0) as usize;
            return if i < n { Hit::MenuItem(i) } else { Hit::None };
        }
    }
    if let Some(l) = list_rect(&r) {
        if inside(&l, x, y) {
            let i = ((y - l.top - 52) / 36).max(0) as usize + LIST_SCROLL;
            return if y > l.top + 52 && i < PLAYLIST_COUNT {
                Hit::ListItem(i)
            } else {
                Hit::None
            };
        }
    }
    if bar_hidden() {
        return Hit::None;
    }
    let b = bar_rect(&r);
    if !inside(&b, x, y) {
        return Hit::None;
    }
    let t = seek_track(&b);
    if y >= t.top - 6 && y <= t.bottom + 2 && x >= t.left - 6 && x <= t.right + 6 {
        return Hit::Seek;
    }
    for (h, br) in buttons(&b) {
        if h != Hit::None
            && br.right > br.left
            && inside(&rect(br.left, br.top - 6, br.right, br.bottom + 6), x, y)
        {
            return h;
        }
    }
    Hit::None
}

unsafe fn seek_ms_at(x: i32) -> u32 {
    let b = bar_rect(&client());
    let t = seek_track(&b);
    let w = (t.right - t.left).max(1);
    let fx = (x - t.left).clamp(0, w);
    (DURATION as u64 * fx as u64 / w as u64) as u32
}

unsafe fn volume_at(x: i32) -> u32 {
    let b = bar_rect(&client());
    let v = buttons(&b)
        .iter()
        .find(|(h, _)| *h == Hit::Volume)
        .map(|(_, r)| *r)
        .unwrap_or_default();
    let w = (v.right - v.left).max(1);
    ((x - v.left).clamp(0, w) * 100 / w) as u32
}

// ---------------------------------------------------------------------------
// Entrées
// ---------------------------------------------------------------------------

pub unsafe fn on_mouse_move(x: i32, y: i32, left_down: bool) -> bool {
    let was_hidden = bar_hidden();
    MOUSE_X = x;
    MOUSE_Y = y;
    LAST_MOVE = GetTickCount();
    let h = hit_test(x, y);
    let mut dirty = h != HOVER || was_hidden || h == Hit::Seek;
    HOVER = h;
    if left_down {
        match PRESSED {
            Hit::Volume => {
                VOLUME = volume_at(x);
                MUTED = false;
                dirty = true;
            }
            Hit::Seek => dirty = true,
            _ => {}
        }
    }
    dirty
}

pub unsafe fn on_mouse_down(x: i32, y: i32) -> bool {
    LAST_MOVE = GetTickCount();
    let h = hit_test(x, y);
    PRESSED = h;
    let now = GetTickCount();
    let double = now.wrapping_sub(LAST_CLICK) < GetDoubleClickTime().clamp(200, 600);
    LAST_CLICK = now;
    if h == Hit::None {
        let r = client();
        if MENU != Menu::None {
            MENU = Menu::None;
            return true;
        }
        if inside(&video_area(&r), x, y) && LOADED {
            if double {
                toggle_fullscreen();
                // Le premier clic a basculé lecture/pause : on l'annule.
                play_pause();
            } else {
                play_pause();
            }
            return true;
        }
        return false;
    }
    if h == Hit::Volume {
        VOLUME = volume_at(x);
        MUTED = false;
    }
    true
}

pub unsafe fn on_mouse_up(x: i32, y: i32) -> bool {
    let pressed = PRESSED;
    PRESSED = Hit::None;
    let h = hit_test(x, y);
    match pressed {
        Hit::Seek => {
            if DURATION > 0 {
                seek(seek_ms_at(x));
            }
            return true;
        }
        Hit::Volume => {
            restart_soon();
            return true;
        }
        _ => {}
    }
    if h != pressed {
        return true;
    }
    let toggle_menu = |m: Menu| {
        MENU = if MENU == m { Menu::None } else { m };
    };
    match h {
        Hit::Play => play_pause(),
        Hit::Prev => next(-1),
        Hit::Next => next(1),
        Hit::Back10 => seek(current_position().saturating_sub(10_000)),
        Hit::Fwd30 => seek(current_position().saturating_add(30_000)),
        Hit::Speed => toggle_menu(Menu::Speed),
        Hit::Audio if AUDIO_COUNT > 1 => toggle_menu(Menu::Audio),
        Hit::Subs if HAS_VIDEO && SUB_COUNT + EXT_COUNT > 0 => toggle_menu(Menu::Subs),
        Hit::Chapters if CHAPTER_COUNT > 0 => toggle_menu(Menu::Chapters),
        Hit::Mute => {
            MUTED = !MUTED;
            restart_soon();
        }
        Hit::Fullscreen => toggle_fullscreen(),
        Hit::Playlist => {
            SHOW_LIST = !SHOW_LIST;
            RESIZED_AT = GetTickCount();
        }
        Hit::MenuItem(i) => {
            choose_menu(i);
        }
        Hit::ListItem(i) if i < PLAYLIST_COUNT && i != CURRENT => {
            let p = PLAYLIST[i];
            open_file(&p, false);
        }
        _ => {}
    }
    true
}

unsafe fn choose_menu(i: usize) {
    let menu = MENU;
    MENU = Menu::None;
    match menu {
        Menu::Speed if i < SPEEDS.len() => {
            SPEED = SPEEDS[i];
            if PLAYING {
                start(current_position(), false);
            }
        }
        Menu::Audio if i < AUDIO_COUNT => {
            if !AUDIO_TRACKS[i].ok {
                toast(tr(S_MISSING), AUDIO_TRACKS[i].label.get());
                return;
            }
            AUDIO_SEL = AUDIO_TRACKS[i].ordinal;
            if PLAYING {
                start(current_position(), false);
            }
        }
        Menu::Subs => {
            SUB_SEL = if i == 0 {
                SubSel::Off
            } else if i <= SUB_COUNT {
                SubSel::Embedded(SUB_TRACKS[i - 1].ordinal)
            } else {
                SubSel::External(i - 1 - SUB_COUNT)
            };
            let pos = current_position();
            if PLAYING {
                start(pos, false);
            } else {
                start(pos, true);
            }
        }
        Menu::Chapters if i < CHAPTER_COUNT => seek(CHAPTERS[i].ms),
        _ => {}
    }
}

pub unsafe fn on_wheel(delta: i32) -> bool {
    let r = client();
    if let Some(l) = list_rect(&r) {
        if inside(&l, MOUSE_X, MOUSE_Y) {
            if delta < 0 && LIST_SCROLL + 1 < PLAYLIST_COUNT {
                LIST_SCROLL += 3.min(PLAYLIST_COUNT - 1 - LIST_SCROLL);
            } else if delta > 0 {
                LIST_SCROLL = LIST_SCROLL.saturating_sub(3);
            }
            return true;
        }
    }
    change_volume(if delta > 0 { 5 } else { -5 });
    true
}

unsafe fn change_volume(d: i32) {
    VOLUME = (VOLUME as i32 + d).clamp(0, 100) as u32;
    MUTED = false;
    restart_soon();
}

pub unsafe fn on_key(vk: u32) -> bool {
    LAST_MOVE = GetTickCount();
    match vk {
        0x20 | 0xB3 => play_pause(), // Espace, lecture/pause
        0x25 => seek(current_position().saturating_sub(5_000)), // ←
        0x27 => seek(current_position().saturating_add(5_000)), // →
        0x26 => change_volume(5),    // ↑
        0x28 => change_volume(-5),   // ↓
        0x46 | 0x7A => toggle_fullscreen(), // F, F11
        0x1B if MENU != Menu::None => MENU = Menu::None, // Échap
        0x1B if FULLSCREEN => toggle_fullscreen(),
        0x4D | 0xAD => {
            MUTED = !MUTED;
            restart_soon();
        }
        0x4E | 0xB0 => next(1),  // N, piste suivante
        0x50 | 0xB1 => next(-1), // P, piste précédente
        0x4C => SHOW_LIST = !SHOW_LIST,
        0x24 => seek(0),                   // Début
        0xDD | 0x6B => step_speed(1),      // ] ou +
        0xDB | 0x6D => step_speed(-1),     // [ ou -
        0x53 if HAS_VIDEO => cycle_subs(), // S
        0x41 if AUDIO_COUNT > 1 => {
            // A : piste audio suivante
            let cur = (0..AUDIO_COUNT)
                .find(|&i| AUDIO_TRACKS[i].ordinal == AUDIO_SEL)
                .unwrap_or(0);
            let nxt = (1..=AUDIO_COUNT)
                .map(|d| (cur + d) % AUDIO_COUNT)
                .find(|&i| AUDIO_TRACKS[i].ok);
            if let Some(i) = nxt {
                AUDIO_SEL = AUDIO_TRACKS[i].ordinal;
                toast(tr(S_AUDIO), b"");
                TOAST.push(b" : ").push(AUDIO_TRACKS[i].label.get());
                if PLAYING {
                    start(current_position(), false);
                }
            }
        }
        _ => return false,
    }
    true
}

unsafe fn step_speed(d: i32) {
    let i = SPEEDS.iter().position(|&s| s == SPEED).unwrap_or(3) as i32;
    let j = (i + d).clamp(0, SPEEDS.len() as i32 - 1) as usize;
    MENU = Menu::Speed;
    choose_menu(j);
}

unsafe fn cycle_subs() {
    let total = 1 + SUB_COUNT + EXT_COUNT;
    let cur = match SUB_SEL {
        SubSel::Off => 0,
        SubSel::Embedded(n) => {
            1 + (0..SUB_COUNT)
                .find(|&i| SUB_TRACKS[i].ordinal == n)
                .unwrap_or(0)
        }
        SubSel::External(k) => 1 + SUB_COUNT + k,
    };
    MENU = Menu::Subs;
    choose_menu((cur + 1) % total);
}

pub unsafe fn on_resize() {
    RESIZED_AT = GetTickCount();
}

unsafe fn toggle_fullscreen() {
    if HWND.is_null() {
        return;
    }
    if !FULLSCREEN {
        SAVED_STYLE = GetWindowLongA(HWND, -16);
        GetWindowRect(HWND, core::ptr::addr_of_mut!(SAVED_RECT));
        SetWindowLongA(HWND, -16, (0x8000_0000u32 | 0x1000_0000) as i32);
        SetWindowPos(
            HWND,
            core::ptr::null_mut(),
            0,
            0,
            GetSystemMetrics(0),
            GetSystemMetrics(1),
            0x0020 | 0x0004,
        );
        FULLSCREEN = true;
    } else {
        SetWindowLongA(HWND, -16, SAVED_STYLE);
        let r = SAVED_RECT;
        SetWindowPos(
            HWND,
            core::ptr::null_mut(),
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            0x0020 | 0x0004,
        );
        FULLSCREEN = false;
    }
    RESIZED_AT = GetTickCount();
}

/// Fermeture de la fenêtre : on mémorise la position.
pub unsafe fn on_close() {
    save_resume();
    stop_session();
}

// ---------------------------------------------------------------------------
// Dessin
// ---------------------------------------------------------------------------

unsafe fn blit_bgra(hdc: Handle, bits: *const u8, w: u32, h: u32, dst: &Rect) {
    let info = BitmapInfoHeader {
        size: 40,
        width: w as i32,
        height: -(h as i32),
        planes: 1,
        bit_count: 32,
        compression: 0,
        ..Default::default()
    };
    StretchDIBits(
        hdc,
        dst.left,
        dst.top,
        dst.right - dst.left,
        dst.bottom - dst.top,
        0,
        0,
        w as i32,
        h as i32,
        bits as *const core::ffi::c_void,
        &info,
        0,
        0x00CC_0020,
    );
}

/// Rectangle de taille `w×h` ajusté et centré dans `area`.
fn fit(area: &Rect, w: u32, h: u32) -> Rect {
    let aw = (area.right - area.left).max(1) as i64;
    let ah = (area.bottom - area.top).max(1) as i64;
    let (w, h) = (w.max(1) as i64, h.max(1) as i64);
    let (dw, dh) = if aw * h > ah * w {
        (ah * w / h, ah)
    } else {
        (aw, aw * h / w)
    };
    let x = area.left + ((aw - dw) / 2) as i32;
    let y = area.top + ((ah - dh) / 2) as i32;
    rect(x, y, x + dw as i32, y + dh as i32)
}

/// Triangle plein (lecture) pointant à droite ou à gauche.
unsafe fn triangle(hdc: Handle, b: &mut Brushes, x: i32, y: i32, s: i32, right: bool, c: u32) {
    for dy in 0..s {
        let half = s / 2;
        let w = (half - (dy - half).abs()).max(0) * 2 * 7 / 8;
        if right {
            fill(hdc, b, x, y + dy, x + w.max(1), y + dy + 1, c);
        } else {
            fill(
                hdc,
                b,
                x + s * 7 / 8 - w.max(1),
                y + dy,
                x + s * 7 / 8,
                y + dy + 1,
                c,
            );
        }
    }
}

unsafe fn icon(g: &mut Brushes, hdc: Handle, f: &Fonts, h: Hit, r: &Rect, c: u32) {
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    match h {
        Hit::Play => {
            if PLAYING {
                fill(hdc, g, cx - 7, cy - 9, cx - 2, cy + 9, c);
                fill(hdc, g, cx + 2, cy - 9, cx + 7, cy + 9, c);
            } else {
                triangle(hdc, g, cx - 6, cy - 10, 20, true, c);
            }
        }
        Hit::Prev => {
            fill(hdc, g, cx - 8, cy - 7, cx - 6, cy + 7, c);
            triangle(hdc, g, cx - 6, cy - 7, 14, false, c);
        }
        Hit::Next => {
            triangle(hdc, g, cx - 6, cy - 7, 14, true, c);
            fill(hdc, g, cx + 6, cy - 7, cx + 8, cy + 7, c);
        }
        Hit::Back10 | Hit::Fwd30 => {
            let s: &[u8] = if h == Hit::Back10 { b"-10" } else { b"+30" };
            text_centered(hdc, f.semibold, 13, c, cx, cy - 8, s);
        }
        Hit::Fullscreen => {
            let (l, t, rr, bb) = (cx - 8, cy - 7, cx + 8, cy + 7);
            let full = FULLSCREEN;
            for (x, y, dx, dy) in [
                (l, t, 1, 1),
                (rr, t, -1, 1),
                (l, bb, 1, -1),
                (rr, bb, -1, -1),
            ] {
                let (x, y) = if full {
                    (x + dx * 5, y + dy * 5)
                } else {
                    (x, y)
                };
                let sx = if full { -dx } else { dx };
                let sy = if full { -dy } else { dy };
                let x2 = x + sx * 5;
                let y2 = y + sy * 5;
                fill(
                    hdc,
                    g,
                    x.min(x2),
                    y.min(y + sy * 2),
                    x.max(x2) + 1,
                    y.max(y + sy * 2) + 1,
                    c,
                );
                fill(
                    hdc,
                    g,
                    x.min(x + sx * 2),
                    y.min(y2),
                    x.max(x + sx * 2) + 1,
                    y.max(y2) + 1,
                    c,
                );
            }
        }
        Hit::Mute => {
            fill(hdc, g, cx - 9, cy - 3, cx - 5, cy + 3, c);
            triangle(hdc, g, cx - 9, cy - 7, 14, false, c);
            if MUTED || VOLUME == 0 {
                line(hdc, g, cx + 2, cy - 4, cx + 9, cy + 4, 2, c);
                line(hdc, g, cx + 2, cy + 4, cx + 9, cy - 4, 2, c);
            } else {
                fill(hdc, g, cx + 2, cy - 2, cx + 4, cy + 2, c);
                if VOLUME > 33 {
                    fill(hdc, g, cx + 6, cy - 4, cx + 8, cy + 4, c);
                }
                if VOLUME > 66 {
                    fill(hdc, g, cx + 10, cy - 6, cx + 12, cy + 6, c);
                }
            }
        }
        Hit::Playlist => {
            for k in 0..3 {
                fill(hdc, g, cx - 8, cy - 6 + k * 6, cx + 8, cy - 4 + k * 6, c);
            }
        }
        Hit::Audio => {
            // Note de musique.
            ellipse(hdc, g, cx - 8, cy + 2, cx - 1, cy + 8, c);
            fill(hdc, g, cx - 2, cy - 9, cx, cy + 5, c);
            fill(hdc, g, cx - 2, cy - 9, cx + 7, cy - 6, c);
        }
        Hit::Subs => {
            round_rect(hdc, g, cx - 10, cy - 7, cx + 10, cy + 7, 3, c);
            let inner = if DARK {
                rgb(0x20, 0x20, 0x20)
            } else {
                rgb(0xF3, 0xF3, 0xF3)
            };
            round_rect(hdc, g, cx - 8, cy - 5, cx + 8, cy + 5, 2, inner);
            text_centered(hdc, f.small, 11, c, cx, cy - 7, b"CC");
        }
        Hit::Chapters => {
            text_centered(hdc, f.small, 11, c, cx, cy - 7, tr(S_CHAPTERS));
        }
        Hit::Speed => {
            let mut b = Buf::<16>::new();
            b.num(SPEED / 100);
            if SPEED % 100 != 0 {
                b.push(b",")
                    .num((SPEED % 100) / if SPEED % 10 == 0 { 10 } else { 1 });
            }
            b.push(b"\xD7");
            text_centered(hdc, f.semibold, 13, c, cx, cy - 8, b.get());
        }
        _ => {}
    }
}

unsafe fn draw_empty(g: &mut Brushes, hdc: Handle, f: &Fonts, r: &Rect) {
    let p = pal();
    fill(hdc, g, r.left, r.top, r.right, r.bottom, p.mica);
    let cx = (r.left + r.right) / 2;
    glyph(hdc, g, f, Glyph::Video, cx - 32, r.top + 36, 64);
    text_centered(hdc, f.large, 22, p.text, cx, r.top + 116, tr(S_TITLE));
    text_centered(hdc, f.body, 13, p.text2, cx, r.top + 152, tr(S_EMPTY));
    text_centered(hdc, f.small, 11, p.text3, cx, r.top + 176, tr(S_KEYS));
    let top = r.top + 212;
    if !ENGINE {
        text_centered(
            hdc,
            f.semibold,
            13,
            rgb(0xE8, 0x48, 0x48),
            cx,
            top,
            tr(S_NO_ENGINE),
        );
        return;
    }
    // Formats réellement décodables ici (jamais de promesse en l'air).
    let mut hw = Buf::<96>::new();
    hw.push(tr(S_HW));
    let mut label: &[u8] = b"";
    for line in FORMATS.get().split(|&c| c == b'\n') {
        if split_tab(line, 0) == b"hw" {
            label = split_tab(line, 1);
        }
    }
    hw.push(if label.is_empty() {
        tr(S_SOFTWARE)
    } else {
        label
    });
    text_centered(hdc, f.body, 13, p.text2, cx, top, hw.get());
    text_centered(hdc, f.semibold, 13, p.text, cx, top + 28, tr(S_FORMATS));
    let col_w = 230;
    let x0 = cx - col_w;
    let (mut yv, mut ya) = (top + 54, top + 54);
    text(hdc, f.semibold, p.text2, x0, yv, b"Vid\xE9o");
    text(hdc, f.semibold, p.text2, x0 + col_w + 20, ya, b"Audio");
    yv += 20;
    ya += 20;
    for line in FORMATS.get().split(|&c| c == b'\n') {
        let kind = split_tab(line, 0);
        if kind != b"video" && kind != b"audio" {
            continue;
        }
        let ok = split_tab(line, 2) == b"1";
        let (x, y) = if kind == b"video" {
            (x0, &mut yv)
        } else {
            (x0 + col_w + 20, &mut ya)
        };
        if *y > r.bottom - 18 {
            continue;
        }
        let mark_c = if ok { p.accent } else { p.text3 };
        if ok {
            fill(hdc, g, x, *y + 6, x + 8, *y + 10, mark_c);
        } else {
            fill(hdc, g, x, *y + 7, x + 8, *y + 9, mark_c);
        }
        text(
            hdc,
            f.caption,
            if ok { p.text } else { p.text3 },
            x + 16,
            *y,
            split_tab(line, 1),
        );
        *y += 18;
    }
}

unsafe fn draw_audio_art(g: &mut Brushes, hdc: Handle, f: &Fonts, area: &Rect) {
    let p = pal();
    fill(hdc, g, area.left, area.top, area.right, area.bottom, p.mica);
    let cx = (area.left + area.right) / 2;
    let size = ((area.bottom - area.top) - 120).clamp(64, COVER as i32);
    let top = area.top + ((area.bottom - area.top) - size - 70).max(8) / 2;
    if HAS_COVER && COVER_STATE != 1 && COVER_STATE != -2 {
        COVER_STATE = WinluxMediaThumbnail(
            PATH.ptr(),
            COVER,
            core::ptr::addr_of_mut!(COVER_IMG) as *mut u8,
        );
        if COVER_STATE < 0 {
            COVER_STATE = -2;
        }
    }
    let art = rect(cx - size / 2, top, cx + size / 2, top + size);
    if COVER_STATE == 1 {
        panel(
            hdc,
            g,
            art.left - 1,
            art.top - 1,
            art.right + 1,
            art.bottom + 1,
            6,
            p.surface,
            p.border,
        );
        blit_bgra(
            hdc,
            core::ptr::addr_of!(COVER_IMG) as *const u8,
            COVER,
            COVER,
            &art,
        );
    } else {
        panel(
            hdc, g, art.left, art.top, art.right, art.bottom, 8, p.surface2, p.border,
        );
        glyph(
            hdc,
            g,
            f,
            Glyph::Music,
            cx - size / 4,
            top + size / 4,
            size / 2,
        );
    }
    let y = art.bottom + 16;
    let mut t = Buf::<160>::new();
    t.push(TITLE.get());
    text_fit(
        hdc,
        f.title,
        p.text,
        cx - (area.right - area.left) / 2 + 24,
        y,
        area.right - area.left - 48,
        t.get(),
        true,
    );
    if ARTIST.n > 0 {
        text_centered(hdc, f.body, 13, p.text2, cx, y + 24, ARTIST.get());
    }
}

unsafe fn draw_video(g: &mut Brushes, hdc: Handle, f: &Fonts, area: &Rect) {
    fill(
        hdc,
        g,
        area.left,
        area.top,
        area.right,
        area.bottom,
        rgb(0, 0, 0),
    );
    if FRAME_W > 0 && FRAME_H > 0 {
        let dst = fit(area, FRAME_W, FRAME_H);
        blit_bgra(
            hdc,
            core::ptr::addr_of!(FRAME) as *const u8,
            FRAME_W,
            FRAME_H,
            &dst,
        );
    } else if SESSION != 0 {
        let cx = (area.left + area.right) / 2;
        let cy = (area.top + area.bottom) / 2;
        // Indicateur de chargement : points en rotation.
        let t = (GetTickCount() / 3 % 360) as i32;
        for k in 0..6 {
            let a = t + k * 24;
            let x = cx + 18 * cos_milli(a) / 1000;
            let y = cy + 18 * sin_milli(a) / 1000;
            let c = mix(rgb(0x30, 0x30, 0x30), rgb(0xE0, 0xE0, 0xE0), k + 1, 6);
            ellipse(hdc, g, x - 3, y - 3, x + 3, y + 3, c);
        }
    } else {
        let _ = f;
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn text_fit(
    hdc: Handle,
    font: Handle,
    color: u32,
    x: i32,
    y: i32,
    w: i32,
    s: &[u8],
    center: bool,
) {
    let mut n = s.len();
    while n > 0 && text_width(&s[..n], 15) > w {
        n -= 1;
    }
    let mut b = Buf::<200>::new();
    b.push(&s[..n]);
    if n < s.len() && n > 3 {
        b.n = n - 2;
        b.push(b"\x85");
    }
    let tw = text_width(b.get(), 15);
    let x = if center { x + (w - tw) / 2 } else { x };
    text(hdc, font, color, x, y, b.get());
}

unsafe fn draw_seek(g: &mut Brushes, hdc: Handle, f: &Fonts, b: &Rect) {
    let p = pal();
    let t = seek_track(b);
    let pos = if PRESSED == Hit::Seek {
        seek_ms_at(MOUSE_X)
    } else {
        current_position()
    };
    let mut l = Buf::<16>::new();
    l.time(pos);
    text(hdc, f.caption, p.text2, b.left + 16, t.top - 1, l.get());
    let mut d = Buf::<16>::new();
    d.time(DURATION);
    text(
        hdc,
        f.caption,
        p.text2,
        b.right - 16 - text_width(d.get(), 12),
        t.top - 1,
        d.get(),
    );
    let mid = (t.top + t.bottom) / 2;
    let hover = HOVER == Hit::Seek || PRESSED == Hit::Seek;
    let th = if hover { 3 } else { 2 };
    round_rect(hdc, g, t.left, mid - th, t.right, mid + th, th, p.border);
    let w = t.right - t.left;
    let fx = if DURATION > 0 {
        (pos as i64 * w as i64 / DURATION as i64) as i32
    } else {
        0
    }
    .clamp(0, w);
    if fx > 0 {
        round_rect(
            hdc,
            g,
            t.left,
            mid - th,
            t.left + fx,
            mid + th,
            th,
            p.accent,
        );
    }
    // Repères de chapitres.
    for i in 1..CHAPTER_COUNT {
        if DURATION == 0 {
            break;
        }
        let x = t.left + (CHAPTERS[i].ms as i64 * w as i64 / DURATION as i64) as i32;
        fill(hdc, g, x - 1, mid - th, x + 1, mid + th, p.surface);
    }
    let kx = t.left + fx;
    let kr = if hover { 8 } else { 6 };
    ellipse(hdc, g, kx - kr, mid - kr, kx + kr, mid + kr, p.surface);
    ellipse(
        hdc,
        g,
        kx - kr + 3,
        mid - kr + 3,
        kx + kr - 3,
        mid + kr - 3,
        p.accent,
    );
}

unsafe fn draw_seek_preview(g: &mut Brushes, hdc: Handle, f: &Fonts, b: &Rect) {
    if !(HOVER == Hit::Seek || PRESSED == Hit::Seek) || DURATION == 0 {
        return;
    }
    let p = pal();
    let t = seek_track(b);
    let at = seek_ms_at(MOUSE_X);
    // Pas d'aperçu : 1/100 de la durée (au moins 2 s) pour profiter du cache.
    let step = (DURATION / 100).max(2000);
    let key = at / step * step;
    let with_image = HAS_VIDEO;
    if with_image && key != PREVIEW_AT {
        PREVIEW_AT = key;
        PREVIEW_READY = false;
    }
    if with_image && !PREVIEW_READY {
        let r = WinluxMediaPreview(
            PATH.ptr(),
            key,
            PREVIEW_W,
            PREVIEW_H,
            core::ptr::addr_of_mut!(PREVIEW) as *mut u8,
        );
        PREVIEW_READY = r == 1;
        if r < 0 {
            PREVIEW_AT = u32::MAX - 1;
        }
    }
    let mut label = Buf::<80>::new();
    label.time(at);
    let ch = (0..CHAPTER_COUNT).rev().find(|&i| CHAPTERS[i].ms <= at);
    if let Some(i) = ch {
        label.push(b" \xB7 ").push(CHAPTERS[i].title.get());
    }
    let tw = text_width(label.get(), 12) + 20;
    let w = if with_image {
        (PREVIEW_W as i32 + 8).max(tw)
    } else {
        tw
    };
    let h = if with_image {
        PREVIEW_H as i32 + 32
    } else {
        28
    };
    let x = (MOUSE_X - w / 2).clamp(b.left + 4, b.right - w - 4);
    let y = t.top - 12 - h;
    panel(hdc, g, x, y, x + w, y + h, 8, p.surface, p.border);
    if with_image {
        let img = rect(
            x + (w - PREVIEW_W as i32) / 2,
            y + 4,
            x + (w + PREVIEW_W as i32) / 2,
            y + 4 + PREVIEW_H as i32,
        );
        if PREVIEW_READY {
            blit_bgra(
                hdc,
                core::ptr::addr_of!(PREVIEW) as *const u8,
                PREVIEW_W,
                PREVIEW_H,
                &img,
            );
        } else {
            fill(
                hdc,
                g,
                img.left,
                img.top,
                img.right,
                img.bottom,
                rgb(0x10, 0x10, 0x10),
            );
        }
    }
    text_centered(
        hdc,
        f.caption,
        12,
        p.text,
        x + w / 2,
        y + h - 22,
        label.get(),
    );
}

unsafe fn draw_bar(g: &mut Brushes, hdc: Handle, f: &Fonts, r: &Rect) {
    let p = pal();
    let b = bar_rect(r);
    fill(hdc, g, b.left, b.top, b.right, b.bottom, p.mica);
    fill(hdc, g, b.left, b.top, b.right, b.top + 1, p.border);
    draw_seek(g, hdc, f, &b);
    for (h, br) in buttons(&b) {
        if h == Hit::None || br.right <= br.left {
            continue;
        }
        let enabled = match h {
            Hit::Audio => AUDIO_COUNT > 1,
            Hit::Subs => HAS_VIDEO && SUB_COUNT + EXT_COUNT > 0,
            Hit::Chapters => CHAPTER_COUNT > 0,
            Hit::Prev => LOADED,
            Hit::Next => CURRENT + 1 < PLAYLIST_COUNT,
            _ => {
                LOADED && PLAYABLE
                    || matches!(h, Hit::Fullscreen | Hit::Playlist | Hit::Volume | Hit::Mute)
            }
        };
        let color = if enabled { p.text } else { p.text3 };
        if h == Hit::Volume {
            let mid = (br.top + br.bottom) / 2;
            round_rect(hdc, g, br.left, mid - 2, br.right, mid + 2, 2, p.border);
            let v = if MUTED { 0 } else { VOLUME as i32 };
            let fx = br.left + (br.right - br.left) * v / 100;
            round_rect(
                hdc,
                g,
                br.left,
                mid - 2,
                fx.max(br.left + 2),
                mid + 2,
                2,
                p.accent,
            );
            ellipse(hdc, g, fx - 6, mid - 6, fx + 6, mid + 6, p.surface);
            ellipse(hdc, g, fx - 3, mid - 3, fx + 3, mid + 3, p.accent);
            continue;
        }
        let active = (h == Hit::Playlist && SHOW_LIST)
            || (h == Hit::Subs && SUB_SEL != SubSel::Off)
            || (h == Hit::Speed && SPEED != 100);
        if h == Hit::Play {
            ellipse(
                hdc,
                g,
                br.left,
                br.top,
                br.right,
                br.bottom,
                if enabled { p.accent } else { p.border },
            );
            icon(g, hdc, f, h, &br, p.on_accent);
            continue;
        }
        if enabled && (HOVER == h || active) {
            let bg = if PRESSED == h { p.pressed } else { p.hover };
            round_rect(hdc, g, br.left, br.top, br.right, br.bottom, 4, bg);
        }
        icon(g, hdc, f, h, &br, if active { p.accent } else { color });
    }
    // Ligne d'état : accélération matérielle, codec.
    if SESSION != 0 && STATE.hw_active != 0 && !STILL {
        text(hdc, f.small, p.text3, b.left + 112, b.top + 56, b"GPU");
    }
}

unsafe fn draw_menu(g: &mut Brushes, hdc: Handle, f: &Fonts, r: &Rect) {
    if MENU == Menu::None {
        return;
    }
    let p = pal();
    let mut items = [Buf::<64>::new(); 32];
    let n = menu_items(&mut items);
    let m = menu_rect(r, n);
    panel(
        hdc, g, m.left, m.top, m.right, m.bottom, 8, p.surface, p.border,
    );
    for (i, item) in items.iter().enumerate().take(n) {
        let y = m.top + 6 + i as i32 * 32;
        if y + 32 > m.bottom {
            break;
        }
        if HOVER == Hit::MenuItem(i) {
            round_rect(hdc, g, m.left + 4, y, m.right - 4, y + 30, 4, p.hover);
        }
        if menu_selected(i) {
            round_rect(hdc, g, m.left + 6, y + 9, m.left + 9, y + 21, 1, p.accent);
        }
        let ok = match MENU {
            Menu::Audio => AUDIO_TRACKS[i].ok,
            _ => true,
        };
        text(
            hdc,
            f.body,
            if ok { p.text } else { p.text3 },
            m.left + 18,
            y + 7,
            item.get(),
        );
    }
}

unsafe fn draw_list(g: &mut Brushes, hdc: Handle, f: &Fonts, r: &Rect) {
    let Some(l) = list_rect(r) else { return };
    let p = pal();
    fill(hdc, g, l.left, l.top, l.right, l.bottom, p.surface);
    fill(hdc, g, l.left, l.top, l.left + 1, l.bottom, p.border);
    let mut h = Buf::<64>::new();
    h.push(tr(S_PLAYLIST))
        .push(b" (")
        .num(PLAYLIST_COUNT as u32)
        .push(b")");
    text(hdc, f.title, p.text, l.left + 16, l.top + 16, h.get());
    if PLAYLIST_COUNT <= 1 {
        text(
            hdc,
            f.caption,
            p.text3,
            l.left + 16,
            l.top + 60,
            tr(S_EMPTY_LIST),
        );
    }
    let mut y = l.top + 52;
    let mut i = LIST_SCROLL;
    while i < PLAYLIST_COUNT && y + 36 <= l.bottom {
        let row = rect(l.left + 6, y, l.right - 6, y + 34);
        if i == CURRENT {
            round_rect(
                hdc, g, row.left, row.top, row.right, row.bottom, 4, p.selected,
            );
            round_rect(
                hdc,
                g,
                row.left + 2,
                row.top + 10,
                row.left + 5,
                row.bottom - 10,
                1,
                p.accent,
            );
        } else if HOVER == Hit::ListItem(i) {
            round_rect(hdc, g, row.left, row.top, row.right, row.bottom, 4, p.hover);
        }
        let kind = WinluxMediaKind(PLAYLIST[i].ptr());
        glyph(
            hdc,
            g,
            f,
            if kind == 2 {
                Glyph::Video
            } else {
                Glyph::Music
            },
            row.left + 12,
            row.top + 9,
            16,
        );
        let mut name = Buf::<160>::new();
        PLAYLIST[i].file_name(&mut name);
        text_fit(
            hdc,
            f.body,
            if i == CURRENT { p.text } else { p.text2 },
            row.left + 38,
            row.top + 9,
            row.right - row.left - 46,
            name.get(),
            false,
        );
        y += 36;
        i += 1;
    }
}

unsafe fn draw_toast(g: &mut Brushes, hdc: Handle, f: &Fonts, area: &Rect) {
    if TOAST_UNTIL == 0 {
        return;
    }
    let p = pal();
    let w = (text_width(TOAST.get(), 13) + 32).min(area.right - area.left - 32);
    let cx = (area.left + area.right) / 2;
    let y = area.top + 20;
    panel(
        hdc,
        g,
        cx - w / 2,
        y,
        cx + w / 2,
        y + 36,
        8,
        p.surface,
        p.border,
    );
    text_fit(
        hdc,
        f.body,
        p.text,
        cx - w / 2 + 16,
        y + 10,
        w - 32,
        TOAST.get(),
        true,
    );
}

unsafe fn draw_unsupported(g: &mut Brushes, hdc: Handle, f: &Fonts, area: &Rect) {
    let p = pal();
    fill(hdc, g, area.left, area.top, area.right, area.bottom, p.mica);
    let cx = (area.left + area.right) / 2;
    let cy = (area.top + area.bottom) / 2;
    glyph(hdc, g, f, Glyph::File, cx - 24, cy - 80, 48);
    text_centered(hdc, f.title, 15, p.text, cx, cy - 16, tr(S_UNSUPPORTED));
    let mut m = Buf::<200>::new();
    if MISSING.n > 0 {
        m.push(tr(S_MISSING)).push(MISSING.get());
    } else if !ENGINE {
        m.push(tr(S_NO_ENGINE));
    } else {
        m.push(tr(S_UNREADABLE));
    }
    text_centered(hdc, f.body, 13, p.text2, cx, cy + 12, m.get());
}

pub unsafe fn draw(hdc: Handle) {
    let r = client();
    let fonts = Fonts::new();
    let mut brushes = Brushes::new();
    SetBkMode(hdc, 1);
    let (g, f) = (&mut brushes, &fonts);
    if !LOADED {
        draw_empty(g, hdc, f, &r);
    } else {
        let area = video_area(&r);
        if !PLAYABLE {
            draw_unsupported(g, hdc, f, &area);
        } else if HAS_VIDEO {
            draw_video(g, hdc, f, &area);
        } else {
            draw_audio_art(g, hdc, f, &area);
        }
        draw_list(g, hdc, f, &r);
        if !bar_hidden() {
            draw_bar(g, hdc, f, &r);
            draw_seek_preview(g, hdc, f, &bar_rect(&r));
        }
        draw_menu(g, hdc, f, &r);
        if MENU == Menu::None && PRESSED == Hit::None {
            let tip: &[u8] = match HOVER {
                Hit::Speed => tr(S_SPEED),
                Hit::Audio => tr(S_AUDIO),
                Hit::Subs => tr(S_SUBS),
                Hit::Chapters => tr(S_CHAPTERS),
                Hit::Playlist => tr(S_PLAYLIST),
                _ => b"",
            };
            draw_tooltip(hdc, g, f, tip, MOUSE_X, MOUSE_Y - 8, r.right);
        }
        draw_toast(g, hdc, f, &area);
    }
    brushes.release();
    fonts.release();
}

/// Initialisation : fenêtre, préférences et fichier passé en argument.
pub unsafe fn init(hwnd: Handle, path: &WPath) {
    HWND = hwnd;
    read_prefs();
    LAST_PREFS = GetTickCount();
    load_formats();
    if !path.is_empty() {
        open_file(path, true);
    } else {
        set_window_title(b"");
    }
}

/// Premier argument de la ligne de commande (chemin du fichier à lire).
pub unsafe fn command_line_path() -> WPath {
    let cl = GetCommandLineW();
    if cl.is_null() {
        return WPath::empty();
    }
    let mut n = 0;
    while *cl.add(n) != 0 && n < 4096 {
        n += 1;
    }
    let s = core::slice::from_raw_parts(cl, n);
    // Saute le nom du programme (entre guillemets ou jusqu'à l'espace).
    let mut i = 0;
    if s.first() == Some(&(b'"' as u16)) {
        i = 1;
        while i < s.len() && s[i] != b'"' as u16 {
            i += 1;
        }
        i += 1;
    } else {
        while i < s.len() && s[i] != b' ' as u16 {
            i += 1;
        }
    }
    while i < s.len() && s[i] == b' ' as u16 {
        i += 1;
    }
    let mut rest = &s[i.min(s.len())..];
    while rest.last() == Some(&(b' ' as u16)) {
        rest = &rest[..rest.len() - 1];
    }
    if rest.len() >= 2 && rest[0] == b'"' as u16 {
        let end = rest[1..]
            .iter()
            .position(|&c| c == b'"' as u16)
            .map(|p| p + 1)
            .unwrap_or(rest.len());
        rest = &rest[1..end];
    }
    WPath::from_wide(rest)
}

// ---------------------------------------------------------------------------
// Fenêtre
// ---------------------------------------------------------------------------

/// Fenêtre autonome du lecteur (`mediaplayer.exe`, `audioplayer.exe`).
pub unsafe fn run(class_name: &core::ffi::CStr) -> ! {
    let class = WNDCLASSA {
        style: 0,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: GetModuleHandleA(core::ptr::null()),
        hIcon: core::ptr::null_mut(),
        hCursor: LoadCursorA(core::ptr::null_mut(), 32512),
        hbrBackground: 0,
        lpszMenuName: core::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    RegisterClassA(&class);
    let path = command_line_path();
    let hwnd = CreateWindowExA(
        0,
        class_name.as_ptr(),
        tr4(&[
            b"Lecteur multim\xE9dia\0",
            b"Media Player\0",
            b"Reproductor multimedia\0",
            b"Medienwiedergabe\0",
        ])
        .as_ptr()
        .cast(),
        0x00CF_0000,
        80,
        60,
        1100,
        700,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        GetModuleHandleA(core::ptr::null()),
        core::ptr::null_mut(),
    );
    init(hwnd, &path);
    ShowWindow(hwnd, 1);
    UpdateWindow(hwnd);
    SetTimer(hwnd, 1, 16, 0);
    let mut msg: MSG = core::mem::zeroed();
    while GetMessageA(&mut msg, core::ptr::null_mut(), 0, 0) != 0 {
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
    }
    ExitProcess(msg.wparam as u32)
}

unsafe extern "system" fn wndproc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    let x = (lparam & 0xFFFF) as i16 as i32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
    let redraw = |dirty: bool| {
        if dirty {
            InvalidateRect(hwnd, core::ptr::null(), 0);
        }
    };
    match msg {
        0x0002 => {
            // WM_DESTROY
            KillTimer(hwnd, 1);
            on_close();
            PostQuitMessage(0);
            0
        }
        0x0005 => {
            // WM_SIZE
            on_resize();
            redraw(true);
            0
        }
        0x0014 => 1, // WM_ERASEBKGND : tout est repeint par draw()
        0x000F => {
            let mut ps: PAINTSTRUCT = core::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            draw(hdc);
            EndPaint(hwnd, &ps);
            0
        }
        0x0113 => {
            redraw(tick());
            0
        }
        0x0200 => {
            redraw(on_mouse_move(x, y, wparam & 1 != 0));
            0
        }
        0x0201 | 0x0203 => {
            redraw(on_mouse_down(x, y));
            0
        }
        0x0202 => {
            redraw(on_mouse_up(x, y));
            0
        }
        0x020A => {
            let delta = ((wparam >> 16) & 0xFFFF) as i16 as i32;
            redraw(on_wheel(delta));
            0
        }
        0x0100 => {
            if on_key(wparam as u32) {
                redraw(true);
                0
            } else {
                DefWindowProcA(hwnd, msg, wparam, lparam)
            }
        }
        _ => DefWindowProcA(hwnd, msg, wparam, lparam),
    }
}
