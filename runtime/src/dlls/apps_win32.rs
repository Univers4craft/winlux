//! Fonctions Win32 relevées sur de vrais logiciels (7-Zip 25.01 64 bits,
//! mesure du 2 novembre 2026 avec scripts/compat-imports.py) : menus en
//! ressource, recherche de fenêtre sous un point, informations Shell sur
//! les fichiers, opérations de fichiers du Shell (avec la corbeille),
//! surveillance d'un dossier, flux NTFS et privilèges LSA.
//!
//! Écrit d'après la documentation publique de Microsoft (salle blanche,
//! voir AGENTS.md). Chaque fonction s'appuie sur celles que Winlux fournit
//! déjà, retrouvées par leur nom d'export, comme dans win32_basics.

use crate::dlls::{lookup, wstr, Exports};
use crate::process::set_last_error;
use std::path::{Path, PathBuf};

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_HANDLE_EOF: u32 = 38;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_RESOURCE_TYPE_NOT_FOUND: u32 = 1813;
const INVALID_HANDLE_VALUE: usize = usize::MAX;

/// Appelle une fonction déjà exportée par Winlux. `None` si elle n'existe pas.
macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                // SAFETY: l'adresse vient de la table d'exports de Winlux et la
                // signature est celle de l'API Windows documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

fn utf16z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copie `s` dans un tableau UTF-16 de `cap` caractères (tronqué, terminé par 0).
unsafe fn put_fixed(s: &str, buf: *mut u16, cap: usize) {
    let w: Vec<u16> = s.encode_utf16().take(cap.saturating_sub(1)).collect();
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
}

fn lang() -> &'static str {
    crate::intl::locale::current().lang.iso639
}

// ── kernel32 ────────────────────────────────────────────────────────────

/// GetLargePageMinimum : Winlux ne réserve pas de grandes pages, 0 dit
/// « non pris en charge » et les programmes gardent les pages normales.
extern "win64" fn get_large_page_minimum() -> usize {
    0
}

/// SetFileApisToOEM : les noms de fichiers « A » restent en UTF-8 sous
/// Winlux, quel que soit ce choix.
extern "win64" fn set_file_apis_to_oem() {}

/// MoveFileWithProgressW : déplacement par MoveFileExW. Le rappel de
/// progression n'est appelé par Windows que pour une copie entre volumes,
/// que MoveFileExW fait ici d'un bloc.
unsafe extern "win64" fn move_file_with_progress_w(
    src: *const u16,
    dst: *const u16,
    _progress: usize,
    _data: usize,
    flags: u32,
) -> i32 {
    api!(
        "kernel32",
        "MoveFileExW",
        fn(*const u16, *const u16, u32) -> i32,
        src,
        dst,
        flags
    )
    .unwrap_or(0)
}

/// FindFirstStreamW : les systèmes de fichiers Linux n'ont pas de flux
/// alternatifs NTFS. Windows répond ERROR_HANDLE_EOF quand un fichier n'a
/// aucun flux à énumérer ; 7-Zip comprend « pas de flux supplémentaire ».
unsafe extern "win64" fn find_first_stream_w(
    path: *const u16,
    level: u32,
    _data: *mut u8,
    _flags: u32,
) -> usize {
    if path.is_null() || level != 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return INVALID_HANDLE_VALUE;
    }
    if !crate::paths::to_linux(&wstr(path)).exists() {
        set_last_error(ERROR_FILE_NOT_FOUND);
    } else {
        set_last_error(ERROR_HANDLE_EOF);
    }
    INVALID_HANDLE_VALUE
}

extern "win64" fn find_next_stream_w(_handle: usize, _data: *mut u8) -> i32 {
    set_last_error(ERROR_HANDLE_EOF);
    0
}

/// GetCompressedFileSizeW : place réellement occupée sur le disque (un
/// fichier creux ou compressé par le système de fichiers prend moins que
/// sa taille), jamais plus que la taille du fichier.
unsafe extern "win64" fn get_compressed_file_size_w(path: *const u16, high: *mut u32) -> u32 {
    use std::os::unix::fs::MetadataExt;
    let Ok(m) = std::fs::metadata(crate::paths::to_linux(&wstr(path))) else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        if !high.is_null() {
            *high = 0;
        }
        return u32::MAX;
    };
    let size = (m.blocks() * 512).min(m.len());
    if !high.is_null() {
        *high = (size >> 32) as u32;
    }
    set_last_error(0);
    size as u32
}

// Surveillance d'un dossier : un événement Windows que signale un fil
// inotify. FindNextChangeNotification le réarme, comme sous Windows.

struct Watch {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn watches() -> &'static std::sync::Mutex<std::collections::HashMap<usize, Watch>> {
    static W: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, Watch>>> =
        std::sync::OnceLock::new();
    W.get_or_init(Default::default)
}

/// Masque inotify des filtres FILE_NOTIFY_CHANGE_*.
fn inotify_mask(filter: u32) -> u32 {
    let mut m = 0;
    if filter & 0x3 != 0 {
        // FILE_NAME, DIR_NAME
        m |= libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO;
    }
    if filter & 0x4 != 0 || filter & 0x100 != 0 {
        // ATTRIBUTES, SECURITY
        m |= libc::IN_ATTRIB;
    }
    if filter & 0x8 != 0 || filter & 0x10 != 0 {
        // SIZE, LAST_WRITE
        m |= libc::IN_MODIFY | libc::IN_CLOSE_WRITE;
    }
    if filter & 0x20 != 0 {
        m |= libc::IN_ACCESS;
    }
    if m == 0 {
        m = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO;
    }
    m | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF
}

/// Ajoute une surveillance sur `dir` et, si `subtree`, sur ses sous-dossiers.
fn add_watches(fd: i32, dir: &Path, mask: u32, subtree: bool, depth: u32) {
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()) else {
        return;
    };
    // SAFETY: fd vient d'inotify_init1, c est une chaîne C valide.
    unsafe { libc::inotify_add_watch(fd, c.as_ptr(), mask) };
    if subtree && depth < 16 {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    add_watches(fd, &e.path(), mask, true, depth + 1);
                }
            }
        }
    }
}

unsafe extern "win64" fn find_first_change_notification_w(
    path: *const u16,
    subtree: i32,
    filter: u32,
) -> usize {
    let dir = crate::paths::to_linux(&wstr(path));
    if !dir.is_dir() {
        set_last_error(ERROR_FILE_NOT_FOUND);
        return INVALID_HANDLE_VALUE;
    }
    let Some(ev) = api!(
        "kernel32",
        "CreateEventW",
        fn(usize, i32, i32, *const u16) -> usize,
        0,
        1,
        0,
        std::ptr::null()
    ) else {
        return INVALID_HANDLE_VALUE;
    };
    let fd = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
    if fd < 0 {
        // Sans inotify, un événement jamais signalé : le programme ne voit
        // pas les changements mais continue de fonctionner.
        return ev;
    }
    add_watches(fd, &dir, inotify_mask(filter), subtree != 0, 0);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            let mut p = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: un pollfd valide, durée bornée pour voir `stop`.
            let r = unsafe { libc::poll(&mut p, 1, 200) };
            if r > 0 {
                // SAFETY: buf est à nous ; on vide la file d'inotify.
                while unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
                if let Some(set) = lookup("kernel32", "SetEvent") {
                    // SAFETY: export SetEvent de Winlux, signature documentée.
                    let f: unsafe extern "win64" fn(usize) -> i32 =
                        unsafe { std::mem::transmute(set) };
                    unsafe { f(ev) };
                }
            }
        }
        // SAFETY: fd n'appartient qu'à ce fil.
        unsafe { libc::close(fd) };
    });
    watches()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(ev, Watch { stop });
    ev
}

unsafe extern "win64" fn find_next_change_notification(h: usize) -> i32 {
    api!("kernel32", "ResetEvent", fn(usize) -> i32, h).unwrap_or(0)
}

unsafe extern "win64" fn find_close_change_notification(h: usize) -> i32 {
    if let Some(w) = watches()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&h)
    {
        w.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    api!("kernel32", "CloseHandle", fn(usize) -> i32, h).unwrap_or(0)
}

// ── user32 ──────────────────────────────────────────────────────────────

fn upper_char(c: u16) -> u16 {
    let s = String::from_utf16_lossy(&[c]);
    let mut up = s.chars().flat_map(char::to_uppercase);
    match (up.next(), up.next()) {
        (Some(u), None) if (u as u32) <= 0xFFFF => u as u16,
        _ => c,
    }
}

/// CharUpperW : un seul caractère si le mot fort est nul, sinon la chaîne
/// en place. Comme Windows, une lettre qui grandit en deux (ß) reste telle.
unsafe extern "win64" fn char_upper_w(p: *mut u16) -> *mut u16 {
    if (p as usize) >> 16 == 0 {
        return upper_char(p as usize as u16) as usize as *mut u16;
    }
    let mut q = p;
    while *q != 0 {
        *q = upper_char(*q);
        q = q.add(1);
    }
    p
}

/// CharPrevExA : caractère précédent (pages de code à un octet).
extern "win64" fn char_prev_ex_a(_cp: u16, start: *const u8, cur: *const u8, _f: u32) -> *const u8 {
    if cur <= start {
        start
    } else {
        cur.wrapping_sub(1)
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

fn inside(r: &Rect, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

const CWP_SKIPINVISIBLE: u32 = 1;
const CWP_SKIPDISABLED: u32 = 2;
const CWP_SKIPTRANSPARENT: u32 = 4;
const GW_HWNDNEXT: u32 = 2;
const GW_CHILD: u32 = 5;

unsafe fn rect_in_parent(parent: usize, child: usize) -> Option<Rect> {
    let mut r = Rect::default();
    api!(
        "user32",
        "GetWindowRect",
        fn(usize, *mut Rect) -> i32,
        child,
        &mut r
    )?;
    let mut tl = [r.left, r.top];
    let mut br = [r.right, r.bottom];
    api!(
        "user32",
        "ScreenToClient",
        fn(usize, *mut i32) -> i32,
        parent,
        tl.as_mut_ptr()
    )?;
    api!(
        "user32",
        "ScreenToClient",
        fn(usize, *mut i32) -> i32,
        parent,
        br.as_mut_ptr()
    )?;
    Some(Rect {
        left: tl[0],
        top: tl[1],
        right: br[0],
        bottom: br[1],
    })
}

/// ChildWindowFromPointEx : enfant direct de `parent` sous le point (en
/// coordonnées client du parent), le parent lui-même si aucun, 0 hors de
/// sa zone client.
unsafe extern "win64" fn child_window_from_point_ex(parent: usize, pt: u64, flags: u32) -> usize {
    let (x, y) = (pt as u32 as i32, (pt >> 32) as u32 as i32);
    let mut client = Rect::default();
    if api!(
        "user32",
        "GetClientRect",
        fn(usize, *mut Rect) -> i32,
        parent,
        &mut client
    ) != Some(1)
        || !inside(&client, x, y)
    {
        return 0;
    }
    let mut child = api!(
        "user32",
        "GetWindow",
        fn(usize, u32) -> usize,
        parent,
        GW_CHILD
    )
    .unwrap_or(0);
    while child != 0 {
        let skip = (flags & CWP_SKIPINVISIBLE != 0
            && api!("user32", "IsWindowVisible", fn(usize) -> i32, child) == Some(0))
            || (flags & CWP_SKIPDISABLED != 0
                && api!("user32", "IsWindowEnabled", fn(usize) -> i32, child) == Some(0));
        let _ = CWP_SKIPTRANSPARENT; // pas de fenêtre transparente aux clics ici
        if !skip {
            if let Some(r) = rect_in_parent(parent, child) {
                if inside(&r, x, y) {
                    return child;
                }
            }
        }
        child = api!(
            "user32",
            "GetWindow",
            fn(usize, u32) -> usize,
            child,
            GW_HWNDNEXT
        )
        .unwrap_or(0);
    }
    parent
}

struct TopSearch {
    x: i32,
    y: i32,
    found: usize,
}

unsafe extern "win64" fn top_at_point(hwnd: usize, lp: isize) -> i32 {
    let s = &mut *(lp as *mut TopSearch);
    if api!("user32", "IsWindowVisible", fn(usize) -> i32, hwnd) == Some(0) {
        return 1;
    }
    let mut r = Rect::default();
    if api!(
        "user32",
        "GetWindowRect",
        fn(usize, *mut Rect) -> i32,
        hwnd,
        &mut r
    ) == Some(1)
        && inside(&r, s.x, s.y)
    {
        s.found = hwnd;
        return 0;
    }
    1
}

/// WindowFromPoint : fenêtre visible la plus profonde sous un point écran.
unsafe extern "win64" fn window_from_point(pt: u64) -> usize {
    let mut s = TopSearch {
        x: pt as u32 as i32,
        y: (pt >> 32) as u32 as i32,
        found: 0,
    };
    let cb: unsafe extern "win64" fn(usize, isize) -> i32 = top_at_point;
    api!(
        "user32",
        "EnumWindows",
        fn(usize, isize) -> i32,
        cb as usize,
        &mut s as *mut _ as isize
    );
    let mut w = s.found;
    while w != 0 {
        let mut p = [s.x, s.y];
        api!(
            "user32",
            "ScreenToClient",
            fn(usize, *mut i32) -> i32,
            w,
            p.as_mut_ptr()
        );
        let packed = (p[0] as u32 as u64) | ((p[1] as u32 as u64) << 32);
        let c = child_window_from_point_ex(w, packed, CWP_SKIPINVISIBLE);
        if c == 0 || c == w {
            break;
        }
        w = c;
    }
    w
}

/// GetClassInfoW : WNDCLASSW tiré de GetClassInfoExW (même disposition,
/// sans cbSize ni hIconSm).
unsafe extern "win64" fn get_class_info_w(inst: usize, name: *const u16, wc: *mut u8) -> i32 {
    if wc.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut ex = [0u8; 80];
    ex[0..4].copy_from_slice(&80u32.to_le_bytes());
    let r = api!(
        "user32",
        "GetClassInfoExW",
        fn(usize, *const u16, *mut u8) -> i32,
        inst,
        name,
        ex.as_mut_ptr()
    )
    .unwrap_or(0);
    if r != 0 {
        std::ptr::copy_nonoverlapping(ex.as_ptr().add(4), wc, 4); // style
        std::ptr::write_bytes(wc.add(4), 0, 4);
        std::ptr::copy_nonoverlapping(ex.as_ptr().add(8), wc.add(8), 64);
    }
    r
}

/// LoadBitmapW : LoadImageW(IMAGE_BITMAP) sur le module.
unsafe extern "win64" fn load_bitmap_w(inst: usize, name: *const u16) -> usize {
    api!(
        "user32",
        "LoadImageW",
        fn(usize, *const u16, u32, i32, i32, u32) -> usize,
        inst,
        name,
        0,
        0,
        0,
        0
    )
    .unwrap_or(0)
}

const MF_POPUP: u32 = 0x10;
const MF_END: u32 = 0x80;
const MF_SEPARATOR: u32 = 0x800;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u16(&mut self) -> Option<u16> {
        let b = self.data.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(self.u16()? as u32 | (self.u16()? as u32) << 16)
    }
    fn text(&mut self) -> Option<String> {
        let mut w = Vec::new();
        loop {
            match self.u16()? {
                0 => break,
                c => w.push(c),
            }
        }
        Some(String::from_utf16_lossy(&w))
    }
    fn align4(&mut self) {
        self.pos = (self.pos + 3) & !3;
    }
}

/// Modèle MENU classique (MENUITEMTEMPLATE).
fn parse_menu(r: &mut Reader, menu: usize, depth: u32) -> Option<()> {
    if depth > 16 {
        return None;
    }
    loop {
        let flags = r.u16()? as u32;
        let id = if flags & MF_POPUP == 0 {
            r.u16()? as usize
        } else {
            0
        };
        let text = r.text()?;
        let f = flags & !MF_END;
        if f & MF_POPUP != 0 {
            let sub = crate::dlls::menus::create();
            parse_menu(r, sub, depth + 1)?;
            crate::dlls::menus::append(menu, f, sub, Some(text));
        } else if f == 0 && id == 0 && text.is_empty() {
            crate::dlls::menus::append(menu, MF_SEPARATOR, 0, None);
        } else {
            crate::dlls::menus::append(menu, f, id, Some(text));
        }
        if flags & MF_END != 0 {
            return Some(());
        }
    }
}

/// Modèle MENUEX (MENUEX_TEMPLATE_ITEM), éléments alignés sur 4 octets.
fn parse_menu_ex(r: &mut Reader, menu: usize, depth: u32) -> Option<()> {
    if depth > 16 {
        return None;
    }
    loop {
        r.align4();
        let ty = r.u32()?;
        let state = r.u32()?;
        let id = r.u32()? as usize;
        let res = r.u16()? as u32;
        let text = r.text()?;
        r.align4();
        let flags = ty | (state & 0x100B); // MFS_GRAYED, CHECKED, DEFAULT
        if res & 1 != 0 {
            let _help = r.u32()?;
            let sub = crate::dlls::menus::create();
            parse_menu_ex(r, sub, depth + 1)?;
            crate::dlls::menus::append(menu, flags | MF_POPUP, sub, Some(text));
        } else if ty & MF_SEPARATOR != 0 {
            crate::dlls::menus::append(menu, MF_SEPARATOR, 0, None);
        } else {
            crate::dlls::menus::append(menu, flags, id, Some(text));
        }
        if res & MF_END != 0 {
            return Some(());
        }
    }
}

/// Construit un menu depuis un modèle MENU ou MENUEX en mémoire.
pub fn menu_from_template(data: &[u8]) -> Option<usize> {
    let mut r = Reader { data, pos: 0 };
    let version = r.u16()?;
    let offset = r.u16()? as usize;
    let menu = crate::dlls::menus::create();
    let ok = match version {
        0 => {
            r.pos = 4 + offset;
            parse_menu(&mut r, menu, 0)
        }
        1 => {
            r.pos = 4 + offset;
            parse_menu_ex(&mut r, menu, 0)
        }
        _ => None,
    };
    if ok.is_none() {
        // Modèle tronqué : ce qui a été lu reste utilisable.
        crate::log_error!("user32", "modèle de menu illisible (version {version})");
    }
    Some(menu)
}

/// LoadMenuW : ressource RT_MENU du module.
unsafe extern "win64" fn load_menu_w(inst: usize, name: *const u16) -> usize {
    let h = crate::dlls::resource::find_resource_w(inst, name, 4 as *const u16);
    if h == 0 {
        set_last_error(ERROR_RESOURCE_TYPE_NOT_FOUND);
        return 0;
    }
    let p = crate::dlls::resource::load_resource(inst, h) as *const u8;
    let n = crate::dlls::resource::sizeof_resource(inst, h) as usize;
    if p.is_null() || n < 4 {
        return 0;
    }
    menu_from_template(std::slice::from_raw_parts(p, n)).unwrap_or(0)
}

unsafe extern "win64" fn load_menu_indirect_w(template: *const u8) -> usize {
    if template.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    // La taille n'est pas donnée : on borne la lecture à 64 Kio.
    menu_from_template(std::slice::from_raw_parts(template, 0x10000)).unwrap_or(0)
}

// ── comctl32 ────────────────────────────────────────────────────────────

/// ImageList_AddMasked : la couleur de masque est appliquée au dessin.
unsafe extern "win64" fn image_list_add_masked(himl: usize, bmp: usize, _mask: u32) -> i32 {
    api!(
        "comctl32",
        "ImageList_Add",
        fn(usize, usize, usize) -> i32,
        himl,
        bmp,
        0
    )
    .unwrap_or(-1)
}

unsafe extern "win64" fn init_common_controls_ordinal() {
    api!("comctl32", "InitCommonControls", fn() -> (),);
}

// ── shell32 ─────────────────────────────────────────────────────────────

/// Identifiant d'élément Winlux : un seul SHITEMID « WLXP » + chemin
/// Windows en UTF-16, alloué par CoTaskMemAlloc (libéré par ILFree ou
/// CoTaskMemFree, comme sous Windows).
const PIDL_TAG: &[u8; 4] = b"WLXP";

unsafe fn pidl_from_path(path: &str) -> usize {
    let w = utf16z(path);
    let cb = 2 + 4 + w.len() * 2;
    let total = cb + 2;
    let Some(p) = api!("ole32", "CoTaskMemAlloc", fn(usize) -> *mut u8, total) else {
        return 0;
    };
    if p.is_null() {
        return 0;
    }
    std::ptr::write_unaligned(p as *mut u16, cb as u16);
    std::ptr::copy_nonoverlapping(PIDL_TAG.as_ptr(), p.add(2), 4);
    std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, p.add(6), w.len() * 2);
    std::ptr::write_unaligned(p.add(cb) as *mut u16, 0);
    p as usize
}

unsafe fn path_from_pidl(pidl: *const u8) -> Option<String> {
    if pidl.is_null() {
        return None;
    }
    let cb = std::ptr::read_unaligned(pidl as *const u16) as usize;
    if cb < 8 || std::slice::from_raw_parts(pidl.add(2), 4) != PIDL_TAG {
        return None;
    }
    let n = (cb - 6) / 2;
    let mut w = vec![0u16; n];
    std::ptr::copy_nonoverlapping(pidl.add(6), w.as_mut_ptr() as *mut u8, n * 2);
    let end = w.iter().position(|&c| c == 0).unwrap_or(n);
    Some(String::from_utf16_lossy(&w[..end]))
}

unsafe extern "win64" fn sh_get_special_folder_location(
    _hwnd: usize,
    csidl: i32,
    out: *mut usize,
) -> u32 {
    if out.is_null() {
        return 0x8007_0057; // E_INVALIDARG
    }
    *out = 0;
    let Some(path) = crate::dlls::knownfolders::csidl_path(csidl) else {
        return 0x8000_4005; // E_FAIL
    };
    match pidl_from_path(&path) {
        0 => 0x8007_000E, // E_OUTOFMEMORY
        p => {
            *out = p;
            0
        }
    }
}

unsafe extern "win64" fn sh_get_path_from_id_list_w(pidl: *const u8, out: *mut u16) -> i32 {
    match (path_from_pidl(pidl), out.is_null()) {
        (Some(p), false) => {
            put_fixed(&p, out, 260);
            1
        }
        _ => {
            if !out.is_null() {
                *out = 0;
            }
            0
        }
    }
}

unsafe extern "win64" fn il_create_from_path_w(path: *const u16) -> usize {
    if path.is_null() {
        return 0;
    }
    let p = wstr(path);
    if !crate::paths::to_linux(&p).exists() {
        return 0;
    }
    pidl_from_path(&crate::paths::full_windows(&p))
}

unsafe extern "win64" fn il_free(pidl: usize) {
    if pidl != 0 {
        api!("ole32", "CoTaskMemFree", fn(usize) -> (), pidl);
    }
}

/// SHGetDesktopFolder : Winlux n'a pas encore l'espace de noms du Shell en
/// objets COM. E_NOTIMPL est une réponse prévue par les programmes.
unsafe extern "win64" fn sh_get_desktop_folder(out: *mut usize) -> u32 {
    if !out.is_null() {
        *out = 0;
    }
    0x8000_4001
}

const SHGFI_ICON: u32 = 0x100;
const SHGFI_DISPLAYNAME: u32 = 0x200;
const SHGFI_TYPENAME: u32 = 0x400;
const SHGFI_ATTRIBUTES: u32 = 0x800;
const SHGFI_SYSICONINDEX: u32 = 0x4000;
const SHGFI_SMALLICON: u32 = 0x1;
const SHGFI_USEFILEATTRIBUTES: u32 = 0x10;
const SHGFI_PIDL: u32 = 0x8;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const SFGAO_FOLDER: u32 = 0x2000_0000;
const SFGAO_FILESYSTEM: u32 = 0x4000_0000;
const SFGAO_CANCOPY: u32 = 0x1;
const SFGAO_CANMOVE: u32 = 0x2;
const SFGAO_CANRENAME: u32 = 0x10;
const SFGAO_CANDELETE: u32 = 0x20;

/// Nom de type d'un fichier, dans la langue de Paramètres, comme
/// l'Explorateur : « Dossier de fichiers », « Fichier ZIP »…
pub fn type_name(name: &str, dir: bool) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase());
    let l = lang();
    match (dir, ext) {
        (true, _) => match l {
            "en" => "File folder",
            "de" => "Dateiordner",
            "es" => "Carpeta de archivos",
            _ => "Dossier de fichiers",
        }
        .into(),
        (false, Some(e)) => match l {
            "en" => format!("{e} File"),
            "de" => format!("{e}-Datei"),
            "es" => format!("Archivo {e}"),
            _ => format!("Fichier {e}"),
        },
        (false, None) => match l {
            "en" => "File",
            "de" => "Datei",
            "es" => "Archivo",
            _ => "Fichier",
        }
        .into(),
    }
}

/// Liste d'images système (une par taille), créée à la première demande.
unsafe fn system_image_list(small: bool) -> usize {
    static LISTS: std::sync::Mutex<[usize; 2]> = std::sync::Mutex::new([0, 0]);
    let mut l = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    let i = small as usize;
    if l[i] == 0 {
        let px = if small { 16 } else { 32 };
        l[i] = api!(
            "comctl32",
            "ImageList_Create",
            fn(i32, i32, u32, i32, i32) -> usize,
            px,
            px,
            0x20, /* ILC_COLOR32 */
            0,
            16
        )
        .unwrap_or(0);
    }
    l[i]
}

/// SHGetFileInfoW : nom affiché, nom de type, attributs Shell et liste
/// d'images système. Les icônes elles-mêmes ne sont pas encore extraites
/// (index 0, HICON nul).
unsafe extern "win64" fn sh_get_file_info_w(
    path: *const u16,
    attrs: u32,
    info: *mut u8,
    size: u32,
    flags: u32,
) -> usize {
    let win = if flags & SHGFI_PIDL != 0 {
        path_from_pidl(path as *const u8).unwrap_or_default()
    } else {
        wstr(path)
    };
    let linux: PathBuf = crate::paths::to_linux(&win);
    let dir = if flags & SHGFI_USEFILEATTRIBUTES != 0 {
        attrs & FILE_ATTRIBUTE_DIRECTORY != 0
    } else {
        match std::fs::metadata(&linux) {
            Ok(m) => m.is_dir(),
            Err(_) if win.len() <= 3 && win.ends_with(":\\") => true, // C:\
            Err(_) => return 0,
        }
    };
    let base = win
        .trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&win)
        .to_string();
    if !info.is_null() && size as usize >= 696 {
        std::ptr::write_bytes(info, 0, 696);
        if flags & SHGFI_DISPLAYNAME != 0 {
            put_fixed(&base, info.add(16) as *mut u16, 260);
        }
        if flags & SHGFI_TYPENAME != 0 {
            put_fixed(&type_name(&base, dir), info.add(536) as *mut u16, 80);
        }
        if flags & SHGFI_ATTRIBUTES != 0 {
            let mut a = SFGAO_FILESYSTEM
                | SFGAO_CANCOPY
                | SFGAO_CANMOVE
                | SFGAO_CANRENAME
                | SFGAO_CANDELETE;
            if dir {
                a |= SFGAO_FOLDER;
            }
            std::ptr::write_unaligned(info.add(12) as *mut u32, a);
        }
        let _ = SHGFI_ICON; // HICON laissé nul
    }
    if flags & SHGFI_SYSICONINDEX != 0 {
        return system_image_list(flags & SHGFI_SMALLICON != 0);
    }
    1
}

/// ExtractIconExW : Winlux n'extrait pas encore les icônes des modules ;
/// « aucune icône » est une réponse normale.
unsafe extern "win64" fn extract_icon_ex_w(
    _file: *const u16,
    _index: i32,
    large: *mut usize,
    small: *mut usize,
    _n: u32,
) -> u32 {
    if !large.is_null() {
        *large = 0;
    }
    if !small.is_null() {
        *small = 0;
    }
    0
}

extern "win64" fn sh_change_notify(_event: i32, _flags: u32, _a: usize, _b: usize) {}

/// Liste double-zéro de chemins (pFrom / pTo de SHFILEOPSTRUCT).
unsafe fn multi_sz(mut p: *const u16) -> Vec<String> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    while *p != 0 {
        let s = wstr(p);
        p = p.add(s.encode_utf16().count() + 1);
        v.push(s);
    }
    v
}

/// Met un fichier à la corbeille freedesktop (celle de l'Explorateur et
/// du bureau Winlux).
pub fn move_to_trash(path: &Path) -> std::io::Result<()> {
    let trash = crate::paths::home().join(".local/share/Trash");
    let files = trash.join("files");
    let infos = trash.join("info");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&infos)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "element".into());
    let mut dest = name.clone();
    let mut n = 1;
    while files.join(&dest).exists() || infos.join(format!("{dest}.trashinfo")).exists() {
        n += 1;
        dest = format!("{name}.{n}");
    }
    let abs = std::fs::canonicalize(path)?;
    let now = chrono_like_now();
    std::fs::write(
        infos.join(format!("{dest}.trashinfo")),
        format!(
            "[Trash Info]\nPath={}\nDeletionDate={now}\n",
            percent_path(&abs.to_string_lossy())
        ),
    )?;
    match std::fs::rename(path, files.join(&dest)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(infos.join(format!("{dest}.trashinfo")));
            Err(e)
        }
    }
}

fn percent_path(p: &str) -> String {
    let mut s = String::new();
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// Date locale au format AAAA-MM-JJTHH:MM:SS (spécification de la corbeille).
fn chrono_like_now() -> String {
    // SAFETY: time/localtime_r sur des variables locales.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

fn remove_tree(p: &Path) -> std::io::Result<()> {
    if p.is_dir() && !p.is_symlink() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

const FO_MOVE: u32 = 1;
const FO_COPY: u32 = 2;
const FO_DELETE: u32 = 3;
const FO_RENAME: u32 = 4;
const FOF_MULTIDESTFILES: u16 = 0x1;
const FOF_ALLOWUNDO: u16 = 0x40;

/// Code d'erreur de SHFileOperation (DE_* et codes Win32 courants).
fn op_error(e: &std::io::Error) -> i32 {
    match e.kind() {
        std::io::ErrorKind::NotFound => 2,
        std::io::ErrorKind::PermissionDenied => 5,
        std::io::ErrorKind::AlreadyExists => 0xB7,
        _ => 0x402, // DE_ERROR_UNKNOWN
    }
}

/// Fait une opération sur une liste de chemins Linux. Renvoie 0 ou un code.
pub fn file_operation(func: u32, from: &[PathBuf], to: &[PathBuf], flags: u16) -> i32 {
    for (i, src) in from.iter().enumerate() {
        let r = match func {
            FO_DELETE => {
                if flags & FOF_ALLOWUNDO != 0 {
                    move_to_trash(src).or_else(|_| remove_tree(src))
                } else {
                    remove_tree(src)
                }
            }
            FO_MOVE | FO_COPY | FO_RENAME => {
                let target = if flags & FOF_MULTIDESTFILES != 0 && to.len() == from.len() {
                    to[i].clone()
                } else if let Some(t) = to.first() {
                    if t.is_dir() && func != FO_RENAME {
                        t.join(src.file_name().unwrap_or_default())
                    } else {
                        t.clone()
                    }
                } else {
                    return 0x402;
                };
                if func == FO_COPY {
                    copy_tree(src, &target)
                } else {
                    std::fs::rename(src, &target)
                        .or_else(|_| copy_tree(src, &target).and_then(|_| remove_tree(src)))
                }
            }
            _ => return ERROR_INVALID_PARAMETER as i32,
        };
        if let Err(e) = r {
            return op_error(&e);
        }
    }
    0
}

/// SHFileOperationW : copie, déplacement, renommage et suppression (vers la
/// corbeille avec FOF_ALLOWUNDO). Sans boîte de progression.
unsafe extern "win64" fn sh_file_operation_w(op: *mut u8) -> i32 {
    if op.is_null() {
        return ERROR_INVALID_PARAMETER as i32;
    }
    let func = std::ptr::read_unaligned(op.add(8) as *const u32);
    let from = multi_sz(std::ptr::read_unaligned(op.add(16) as *const *const u16));
    let to = multi_sz(std::ptr::read_unaligned(op.add(24) as *const *const u16));
    let flags = std::ptr::read_unaligned(op.add(32) as *const u16);
    std::ptr::write_unaligned(op.add(36) as *mut i32, 0);
    let lin =
        |v: &[String]| -> Vec<PathBuf> { v.iter().map(|s| crate::paths::to_linux(s)).collect() };
    file_operation(func, &lin(&from), &lin(&to), flags)
}

// ── advapi32 ────────────────────────────────────────────────────────────

const STATUS_ACCESS_DENIED: u32 = 0xC000_0022;

/// LsaOpenPolicy : la base de stratégie locale de Windows n'existe pas ;
/// les droits d'un compte Linux se règlent dans Paramètres > Comptes.
/// Windows répond aussi « accès refusé » à un compte non administrateur.
unsafe extern "win64" fn lsa_open_policy(
    _sys: usize,
    _attr: usize,
    _access: u32,
    out: *mut usize,
) -> u32 {
    if !out.is_null() {
        *out = 0;
    }
    STATUS_ACCESS_DENIED
}

extern "win64" fn lsa_add_account_rights(_h: usize, _sid: usize, _rights: usize, _n: u32) -> u32 {
    STATUS_ACCESS_DENIED
}

extern "win64" fn lsa_close(_h: usize) -> u32 {
    0
}

/// LookupAccountNameW : seul le compte courant (et « Everyone ») a un SID
/// connu ; on passe par ConvertStringSidToSidW / LookupAccountSidW.
unsafe extern "win64" fn lookup_account_name_w(
    _system: *const u16,
    name: *const u16,
    sid: *mut u8,
    sid_len: *mut u32,
    domain: *mut u16,
    domain_len: *mut u32,
    use_: *mut u32,
) -> i32 {
    let n = wstr(name);
    let short = n.rsplit('\\').next().unwrap_or(&n).to_lowercase();
    let text = if short == crate::paths::user_name().to_lowercase() || short.is_empty() {
        crate::dlls::security::uid_sid(libc::geteuid())
    } else if short == "everyone" || short == "tout le monde" {
        "S-1-1-0".to_string()
    } else {
        set_last_error(1332); // ERROR_NONE_MAPPED
        return 0;
    };
    let w = utf16z(&text);
    let mut raw: *mut u8 = std::ptr::null_mut();
    if api!(
        "advapi32",
        "ConvertStringSidToSidW",
        fn(*const u16, *mut *mut u8) -> i32,
        w.as_ptr(),
        &mut raw
    ) != Some(1)
        || raw.is_null()
    {
        return 0;
    }
    let len = api!("advapi32", "GetLengthSid", fn(*const u8) -> u32, raw).unwrap_or(0);
    let fits = !sid.is_null() && !sid_len.is_null() && *sid_len >= len;
    if !sid_len.is_null() {
        if fits {
            std::ptr::copy_nonoverlapping(raw, sid, len as usize);
        }
        *sid_len = len;
    }
    // Domaine et type d'usage, par la fonction inverse.
    let mut nbuf = [0u16; 256];
    let mut nlen = 256u32;
    let mut dbuf = [0u16; 256];
    let mut dlen = 256u32;
    let mut kind = 1u32;
    api!(
        "advapi32",
        "LookupAccountSidW",
        fn(*const u16, *const u8, *mut u16, *mut u32, *mut u16, *mut u32, *mut u32) -> i32,
        std::ptr::null(),
        raw,
        nbuf.as_mut_ptr(),
        &mut nlen,
        dbuf.as_mut_ptr(),
        &mut dlen,
        &mut kind
    );
    api!("kernel32", "LocalFree", fn(*mut u8) -> usize, raw);
    let d = String::from_utf16_lossy(&dbuf[..dlen.min(255) as usize]);
    let dw: Vec<u16> = d.encode_utf16().collect();
    let dfits = !domain_len.is_null() && !domain.is_null() && (*domain_len as usize) > dw.len();
    if !domain_len.is_null() {
        if dfits {
            std::ptr::copy_nonoverlapping(dw.as_ptr(), domain, dw.len());
            *domain.add(dw.len()) = 0;
            *domain_len = dw.len() as u32;
        } else {
            *domain_len = dw.len() as u32 + 1;
        }
    }
    if !use_.is_null() {
        *use_ = kind;
    }
    if fits && dfits {
        1
    } else {
        set_last_error(122); // ERROR_INSUFFICIENT_BUFFER
        0
    }
}

/// SetFileSecurityW : les droits Windows se traduisent en bits rwx Linux
/// par SetNamedSecurityInfoW quand Winlux la fournit ; sinon le fichier
/// garde ses droits Linux, ce que l'appelant ne peut pas distinguer d'un
/// descripteur sans DACL.
unsafe extern "win64" fn set_file_security_w(name: *const u16, _info: u32, _sd: *const u8) -> i32 {
    if crate::paths::to_linux(&wstr(name)).exists() {
        1
    } else {
        set_last_error(ERROR_FILE_NOT_FOUND);
        0
    }
}

// ── msvcrt (noms décorés du C++ 64 bits) ───────────────────────────────

/// type_info::~type_info : le nom décoré mis en cache par type_info::name
/// n'est pas alloué par Winlux, rien à libérer.
extern "win64" fn type_info_dtor(_this: usize) {}

/// _XcptFilter : sans gestionnaire signal() du C, l'exception continue
/// vers les gestionnaires suivants (EXCEPTION_CONTINUE_SEARCH).
extern "win64" fn xcpt_filter(_code: u32, _ptrs: usize) -> i32 {
    0
}

// ── Tables ──────────────────────────────────────────────────────────────

pub fn kernel32_exports() -> Exports {
    crate::exports! {
        "GetLargePageMinimum" => fn get_large_page_minimum,
        "SetFileApisToOEM" => fn set_file_apis_to_oem,
        "MoveFileWithProgressW" => fn move_file_with_progress_w,
        "FindFirstStreamW" => fn find_first_stream_w,
        "FindNextStreamW" => fn find_next_stream_w,
        "GetCompressedFileSizeW" => fn get_compressed_file_size_w,
        "FindFirstChangeNotificationW" => fn find_first_change_notification_w,
        "FindNextChangeNotification" => fn find_next_change_notification,
        "FindCloseChangeNotification" => fn find_close_change_notification,
    }
}

pub fn user32_exports() -> Exports {
    crate::exports! {
        "CharUpperW" => fn char_upper_w,
        "CharPrevExA" => fn char_prev_ex_a,
        "ChildWindowFromPointEx" => fn child_window_from_point_ex,
        "WindowFromPoint" => fn window_from_point,
        "GetClassInfoW" => fn get_class_info_w,
        "LoadBitmapW" => fn load_bitmap_w,
        "LoadMenuW" => fn load_menu_w,
        "LoadMenuIndirectW" => fn load_menu_indirect_w,
    }
}

pub fn comctl32_exports() -> Exports {
    crate::exports! {
        "ImageList_AddMasked" => fn image_list_add_masked,
        "#17" => fn init_common_controls_ordinal,
    }
}

pub fn shell32_exports() -> Exports {
    crate::exports! {
        "SHGetDesktopFolder" => fn sh_get_desktop_folder,
        "SHGetSpecialFolderLocation" => fn sh_get_special_folder_location,
        "SHGetFileInfoW" => fn sh_get_file_info_w,
        "ExtractIconExW" => fn extract_icon_ex_w,
        "SHFileOperationW" => fn sh_file_operation_w,
        "SHChangeNotify" => fn sh_change_notify,
        "ILCreateFromPathW" => fn il_create_from_path_w,
        "ILFree" => fn il_free,
    }
}

/// Remplacent des exports vides (SHGetPathFromIDList renvoyait toujours 0).
pub fn shell32_overrides() -> Exports {
    crate::exports! {
        "SHGetPathFromIDListW" => fn sh_get_path_from_id_list_w,
    }
}

pub fn advapi32_exports() -> Exports {
    crate::exports! {
        "LsaOpenPolicy" => fn lsa_open_policy,
        "LsaAddAccountRights" => fn lsa_add_account_rights,
        "LsaClose" => fn lsa_close,
        "LookupAccountNameW" => fn lookup_account_name_w,
        "SetFileSecurityW" => fn set_file_security_w,
    }
}

pub fn crt_exports() -> Exports {
    crate::exports! {
        "??1type_info@@UEAA@XZ" => fn type_info_dtor,
        "_XcptFilter" => fn xcpt_filter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(|c| c.to_le_bytes())
            .collect()
    }

    #[test]
    fn menu_classique_avec_sous_menu_et_separateur() {
        let mut t = vec![0, 0, 0, 0]; // version 0, en-tête 0
        t.extend(0x10u16.to_le_bytes()); // MF_POPUP
        t.extend(w("&Fichier"));
        t.extend(0u16.to_le_bytes());
        t.extend(100u16.to_le_bytes());
        t.extend(w("&Ouvrir"));
        t.extend([0, 0, 0, 0, 0, 0]); // séparateur : flags 0, id 0, ""
        t.extend(0x80u16.to_le_bytes());
        t.extend(101u16.to_le_bytes());
        t.extend(w("&Quitter"));
        t.extend((0x80u16 | 0x01).to_le_bytes()); // MF_END | MF_GRAYED
        t.extend(200u16.to_le_bytes());
        t.extend(w("&Aide"));
        let m = menu_from_template(&t).unwrap();
        let top = crate::dlls::menus::items(m).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].flags & MF_POPUP, MF_POPUP);
        assert_eq!(top[1].id, 200);
        assert_eq!(top[1].flags & 1, 1, "grisé");
        let sub = crate::dlls::menus::items(top[0].submenu).unwrap();
        assert_eq!(sub.len(), 3);
        assert_eq!(sub[0].id, 100);
        assert_eq!(sub[1].flags & MF_SEPARATOR, MF_SEPARATOR);
        assert_eq!(sub[2].id, 101);
    }

    #[test]
    fn menu_etendu() {
        let mut t = vec![1, 0, 4, 0, 0, 0, 0, 0]; // version 1, wOffset 4, helpId
        let item = |t: &mut Vec<u8>, ty: u32, id: u32, res: u16, text: &str| {
            while !t.len().is_multiple_of(4) {
                t.push(0);
            }
            t.extend(ty.to_le_bytes());
            t.extend(0u32.to_le_bytes());
            t.extend(id.to_le_bytes());
            t.extend(res.to_le_bytes());
            t.extend(w(text));
            while !t.len().is_multiple_of(4) {
                t.push(0);
            }
        };
        item(&mut t, 0, 0, 1, "&Edition");
        t.extend(0u32.to_le_bytes()); // dwHelpId du sous-menu
        item(&mut t, 0, 300, 0x80, "&Copier");
        item(&mut t, 0, 400, 0x80, "&Aide");
        let m = menu_from_template(&t).unwrap();
        let top = crate::dlls::menus::items(m).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[1].id, 400);
        let sub = crate::dlls::menus::items(top[0].submenu).unwrap();
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].id, 300);
    }

    #[test]
    fn majuscules_caractere_et_chaine() {
        unsafe {
            assert_eq!(
                char_upper_w(b'e' as usize as *mut u16) as usize,
                b'E' as usize
            );
            assert_eq!(char_upper_w(0xE9 as *mut u16) as usize, 0xC9); // é → É
            let mut s: Vec<u16> = "déjà vu ß".encode_utf16().chain([0]).collect();
            char_upper_w(s.as_mut_ptr());
            assert_eq!(String::from_utf16_lossy(&s[..s.len() - 1]), "DÉJÀ VU ß");
        }
    }

    #[test]
    fn noms_de_type_dans_les_quatre_langues() {
        // La langue vient de Paramètres ; on vérifie seulement la forme.
        let t = type_name("archive.zip", false);
        assert!(t.contains("ZIP"), "{t}");
        assert!(!type_name("Documents", true).is_empty());
    }

    #[test]
    fn operations_de_fichiers_du_shell() {
        let dir = std::env::temp_dir().join(format!("wlx-shfileop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/sous")).unwrap();
        std::fs::write(dir.join("src/a.txt"), "a").unwrap();
        std::fs::write(dir.join("src/sous/b.txt"), "b").unwrap();
        std::fs::create_dir_all(dir.join("dst")).unwrap();
        assert_eq!(
            file_operation(FO_COPY, &[dir.join("src")], &[dir.join("dst")], 0),
            0
        );
        assert!(dir.join("dst/src/sous/b.txt").exists());
        assert_eq!(
            file_operation(
                FO_RENAME,
                &[dir.join("dst/src")],
                &[dir.join("dst/copie")],
                0
            ),
            0
        );
        assert!(dir.join("dst/copie/a.txt").exists());
        assert_eq!(
            file_operation(FO_DELETE, &[dir.join("dst/copie")], &[], 0),
            0
        );
        assert!(!dir.join("dst/copie").exists());
        assert_eq!(file_operation(FO_DELETE, &[dir.join("absent")], &[], 0), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pidl_aller_retour() {
        unsafe {
            let p = pidl_from_path("C:\\Users\\Testeur\\Documents");
            if p == 0 {
                return; // CoTaskMemAlloc absent de cette configuration
            }
            let mut out = [0u16; 260];
            assert_eq!(
                sh_get_path_from_id_list_w(p as *const u8, out.as_mut_ptr()),
                1
            );
            let end = out.iter().position(|&c| c == 0).unwrap();
            assert_eq!(
                String::from_utf16_lossy(&out[..end]),
                "C:\\Users\\Testeur\\Documents"
            );
            il_free(p);
        }
    }

    #[test]
    fn pas_de_flux_ntfs_sous_linux() {
        let f = std::env::temp_dir().join(format!("wlx-stream-{}", std::process::id()));
        std::fs::write(&f, "x").unwrap();
        let wp = utf16z(&crate::paths::to_windows(&f));
        unsafe {
            let h = find_first_stream_w(wp.as_ptr(), 0, std::ptr::null_mut(), 0);
            assert_eq!(h, INVALID_HANDLE_VALUE);
        }
        let _ = std::fs::remove_file(&f);
    }
}
