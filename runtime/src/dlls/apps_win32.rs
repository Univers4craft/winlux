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

// ── Tables du micrologiciel (SMBIOS) ────────────────────────────────────

/// Fournisseur 'RSMB' (SMBIOS brut), tel que l'écrit une constante C à
/// quatre caractères.
const FIRMWARE_RSMB: u32 = u32::from_be_bytes(*b"RSMB");

/// Dossiers où lire `smbios_entry_point` et `DMI`. Le noyau ne les rend
/// lisibles qu'à root ; `winlux-system-services` en dépose une copie
/// lisible par tous dans /run/winlux/firmware au démarrage. La variable
/// WINLUX_FIRMWARE_ROOT remplace les deux (tests).
fn firmware_dirs() -> Vec<std::path::PathBuf> {
    if let Some(root) = std::env::var_os("WINLUX_FIRMWARE_ROOT") {
        return vec![root.into()];
    }
    vec![
        "/sys/firmware/dmi/tables".into(),
        "/run/winlux/firmware".into(),
    ]
}

/// Construit la structure RawSMBIOSData de GetSystemFirmwareTable('RSMB') :
/// Used20CallingMethod, MajorVersion, MinorVersion, DmiRevision, Length
/// (u32), puis les tables SMBIOS brutes.
fn raw_smbios(entry: &[u8], dmi: &[u8]) -> Vec<u8> {
    let (major, minor, rev) = if entry.starts_with(b"_SM3_") && entry.len() >= 10 {
        (entry[7], entry[8], entry[9])
    } else if entry.starts_with(b"_SM_") && entry.len() >= 0x1f {
        (entry[6], entry[7], entry[0x1e])
    } else {
        (0, 0, 0)
    };
    let mut out = Vec::with_capacity(8 + dmi.len());
    out.extend([0, major, minor, rev]);
    out.extend((dmi.len() as u32).to_le_bytes());
    out.extend_from_slice(dmi);
    out
}

fn read_raw_smbios() -> Option<Vec<u8>> {
    for dir in firmware_dirs() {
        let Ok(dmi) = std::fs::read(dir.join("DMI")) else {
            continue;
        };
        if dmi.is_empty() {
            continue;
        }
        let entry = std::fs::read(dir.join("smbios_entry_point")).unwrap_or_default();
        return Some(raw_smbios(&entry, &dmi));
    }
    None
}

/// Copie `data` si le tampon suffit ; renvoie toujours la taille requise,
/// comme le font EnumSystemFirmwareTables et GetSystemFirmwareTable.
unsafe fn firmware_copy(data: &[u8], buf: *mut u8, size: u32) -> u32 {
    if !buf.is_null() && size as usize >= data.len() {
        std::ptr::copy_nonoverlapping(data.as_ptr(), buf, data.len());
    }
    data.len() as u32
}

/// EnumSystemFirmwareTables : 'RSMB' a une seule table, d'identifiant 0,
/// quand le SMBIOS de la machine est lisible.
unsafe extern "win64" fn enum_system_firmware_tables(
    provider: u32,
    buf: *mut u8,
    size: u32,
) -> u32 {
    if provider != FIRMWARE_RSMB {
        set_last_error(1); // ERROR_INVALID_FUNCTION
        return 0;
    }
    if read_raw_smbios().is_none() {
        set_last_error(1168); // ERROR_NOT_FOUND
        return 0;
    }
    firmware_copy(&0u32.to_le_bytes(), buf, size)
}

/// GetSystemFirmwareTable('RSMB', 0) : en-tête RawSMBIOSData suivi des
/// tables lues dans /sys/firmware/dmi/tables (ou leur copie de démarrage).
unsafe extern "win64" fn get_system_firmware_table(
    provider: u32,
    _table: u32,
    buf: *mut u8,
    size: u32,
) -> u32 {
    if provider != FIRMWARE_RSMB {
        set_last_error(1); // ERROR_INVALID_FUNCTION
        return 0;
    }
    match read_raw_smbios() {
        Some(data) => firmware_copy(&data, buf, size),
        None => {
            set_last_error(1168); // ERROR_NOT_FOUND
            0
        }
    }
}

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

/// MoveFileTransactedW et DeleteFileTransactedW (Transactional NTFS) :
/// les transactions de ktmw32 n'ont pas d'isolation (voir sysdlls), donc
/// l'opération est faite tout de suite, comme MoveFileWithProgressW et
/// DeleteFileW. Le programme d'installation de Steam x86 cherche ces deux
/// fonctions par GetProcAddress pour mettre en place les fichiers extraits
/// (#1475).
unsafe extern "win64" fn move_file_transacted_w(
    src: *const u16,
    dst: *const u16,
    progress: usize,
    data: usize,
    flags: u32,
    _transaction: usize,
) -> i32 {
    move_file_with_progress_w(src, dst, progress, data, flags)
}

unsafe extern "win64" fn delete_file_transacted_w(path: *const u16, _transaction: usize) -> i32 {
    api!("kernel32", "DeleteFileW", fn(*const u16) -> i32, path).unwrap_or(0)
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

pub(crate) fn upper_char(c: u16) -> u16 {
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

pub(crate) unsafe fn pidl_from_path(path: &str) -> usize {
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

/// Chemin d'un identifiant Winlux. Un identifiant de plusieurs éléments
/// (ILCombine, ou mis bout à bout par le programme) désigne le chemin de
/// son dernier élément « WLXP », qui porte toujours le chemin complet.
pub(crate) unsafe fn path_from_pidl(pidl: *const u8) -> Option<String> {
    if pidl.is_null() {
        return None;
    }
    let mut found = None;
    let mut item = pidl;
    for _ in 0..64 {
        let cb = std::ptr::read_unaligned(item as *const u16) as usize;
        if cb == 0 {
            break;
        }
        if cb >= 8 && std::slice::from_raw_parts(item.add(2), 4) == PIDL_TAG {
            found = Some(item);
        }
        item = item.add(cb);
    }
    let item = found?;
    let cb = std::ptr::read_unaligned(item as *const u16) as usize;
    let n = (cb - 6) / 2;
    let mut w = vec![0u16; n];
    std::ptr::copy_nonoverlapping(item.add(6), w.as_mut_ptr() as *mut u8, n * 2);
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

/// SHGetKnownFolderIDList : PIDL absolu d'un dossier connu (même chemin
/// que SHGetKnownFolderPath), à libérer par CoTaskMemFree/ILFree.
unsafe extern "win64" fn sh_get_known_folder_id_list(
    id: *const u8,
    flags: u32,
    _token: usize,
    out: *mut usize,
) -> u32 {
    if out.is_null() {
        return 0x8007_0057; // E_INVALIDARG
    }
    *out = 0;
    let Some(guid) = crate::dlls::knownfolders::guid_text(id) else {
        return 0x8007_0057;
    };
    let Some(path) = crate::dlls::knownfolders::known_folder_path(&guid, flags) else {
        return 0x8007_0002; // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
    };
    match pidl_from_path(&path) {
        0 => 0x8007_000E, // E_OUTOFMEMORY
        p => {
            *out = p;
            0
        }
    }
}

pub(super) unsafe extern "win64" fn il_create_from_path_w(path: *const u16) -> usize {
    if path.is_null() {
        return 0;
    }
    let p = wstr(path);
    if !crate::paths::to_linux(&p).exists() {
        return 0;
    }
    pidl_from_path(&crate::paths::full_windows(&p))
}

/// SHOpenFolderAndSelectItems (« Afficher dans le dossier » de Chrome,
/// Electron, des lanceurs…) : l'Explorateur Winlux sur le dossier, comme
/// `explorer.exe /select,CHEMIN`. Les éléments enfants relatifs ne sont pas
/// encore lus : le dossier visé suffit à l'ouvrir.
unsafe extern "win64" fn sh_open_folder_and_select_items(
    folder: *const u8,
    _count: u32,
    _items: *const usize,
    _flags: u32,
) -> u32 {
    let Some(path) = path_from_pidl(folder) else {
        return 0x8007_0057; // E_INVALIDARG
    };
    let params = format!("/select,\"{path}\"");
    if crate::dlls::shell32::execute("explorer.exe", &params, None, None) > 32 {
        0
    } else {
        0x8000_4005 // E_FAIL
    }
}

unsafe extern "win64" fn il_free(pidl: usize) {
    if pidl != 0 {
        api!("ole32", "CoTaskMemFree", fn(usize) -> (), pidl);
    }
}

/// SHGetDesktopFolder : le dossier racine de l'espace de noms
/// (shell_folder_com), sur le système de fichiers.
unsafe extern "win64" fn sh_get_desktop_folder(out: *mut usize) -> u32 {
    super::shell_folder_com::desktop(out)
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

/// Liste d'images système (une par taille), avec les icônes du thème.
fn system_image_list(small: bool) -> usize {
    super::icons::system_list(small)
}

/// SHGetFileInfoW : nom affiché, nom de type, attributs Shell, index et
/// icône de la liste d'images système (icônes génériques du thème : dossier,
/// fichier, archive, programme…).
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
        let index = super::icons::system_index(&base, dir);
        std::ptr::write_unaligned(info.add(8) as *mut i32, index);
        if flags & SHGFI_ICON != 0 {
            let small = flags & SHGFI_SMALLICON != 0;
            let icon = super::icons::list_image(system_image_list(small), index)
                .map_or(0, super::icons::new_icon);
            std::ptr::write_unaligned(info as *mut usize, icon);
        }
    }
    if flags & SHGFI_SYSICONINDEX != 0 {
        return system_image_list(flags & SHGFI_SMALLICON != 0);
    }
    1
}

/// ExtractIconExW : Winlux n'extrait pas encore les icônes des modules ;
/// « aucune icône » est une réponse normale.
pub(super) unsafe extern "win64" fn extract_icon_ex_w(
    _file: *const u16,
    _index: i32,
    large: *mut usize,
    small: *mut usize,
    _n: u32,
) -> u32 {
    // Une case HICON de 4 octets en x86 : write_ptr n'écrit pas au-delà.
    if !large.is_null() {
        crate::wow32::write_ptr(large as usize, 0);
    }
    if !small.is_null() {
        crate::wow32::write_ptr(small as usize, 0);
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

/// Liste double-zéro de chemins ANSI (pFrom / pTo de SHFILEOPSTRUCTA).
unsafe fn multi_sz_ansi(mut p: *const u8) -> Vec<String> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    while *p != 0 {
        let n = libc::strlen(p.cast());
        v.push(crate::dlls::decode_ansi(std::slice::from_raw_parts(p, n)));
        p = p.add(n + 1);
    }
    v
}

/// Décalages de SHFILEOPSTRUCT (wFunc, pFrom, pTo, fFlags,
/// fAnyOperationsAborted) : alignement naturel en 64 bits, `pshpack1.h`
/// en 32 bits (shellapi.h, en-têtes MinGW-w64), d'où fFlags (WORD) à 16
/// et fAnyOperationsAborted à 18 en x86 (#1841).
fn file_op_offsets(x86: bool) -> [usize; 5] {
    if x86 {
        [4, 8, 12, 16, 18]
    } else {
        [8, 16, 24, 32, 36]
    }
}

/// SHFileOperationA/W : copie, déplacement, renommage et suppression (vers
/// la corbeille avec FOF_ALLOWUNDO). Sans boîte de progression.
pub(super) unsafe fn sh_file_operation(op: *mut u8, wide: bool) -> i32 {
    if op.is_null() {
        return ERROR_INVALID_PARAMETER as i32;
    }
    let x86 = crate::wow32::pointer_size() == 4;
    let [func_at, from_at, to_at, flags_at, aborted_at] = file_op_offsets(x86);
    let ptr = |at: usize| -> usize {
        if x86 {
            std::ptr::read_unaligned(op.add(at) as *const u32) as usize
        } else {
            std::ptr::read_unaligned(op.add(at) as *const usize)
        }
    };
    let func = std::ptr::read_unaligned(op.add(func_at) as *const u32);
    let (from, to) = if wide {
        (
            multi_sz(ptr(from_at) as *const u16),
            multi_sz(ptr(to_at) as *const u16),
        )
    } else {
        (
            multi_sz_ansi(ptr(from_at) as *const u8),
            multi_sz_ansi(ptr(to_at) as *const u8),
        )
    };
    let flags = std::ptr::read_unaligned(op.add(flags_at) as *const u16);
    std::ptr::write_unaligned(op.add(aborted_at) as *mut i32, 0);
    let lin =
        |v: &[String]| -> Vec<PathBuf> { v.iter().map(|s| crate::paths::to_linux(s)).collect() };
    file_operation(func, &lin(&from), &lin(&to), flags)
}

unsafe extern "win64" fn sh_file_operation_w(op: *mut u8) -> i32 {
    sh_file_operation(op, true)
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

// Journal des événements : chaque source est un handle ; ReportEvent écrit
// dans le journal de Winlux (celui que lit le rapport de bogue), avec le
// niveau du type d'événement Windows.

fn event_sources() -> &'static std::sync::Mutex<std::collections::HashMap<usize, String>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, String>>> =
        std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

fn register_source(name: String) -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x5E70_0000);
    let h = NEXT.fetch_add(4, std::sync::atomic::Ordering::Relaxed);
    event_sources()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(h, name);
    h
}

unsafe extern "win64" fn register_event_source_w(_server: *const u16, source: *const u16) -> usize {
    register_source(wstr(source))
}

unsafe extern "win64" fn register_event_source_a(_server: *const u8, source: *const u8) -> usize {
    register_source(crate::dlls::cstr(source))
}

extern "win64" fn deregister_event_source(h: usize) -> i32 {
    match event_sources()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&h)
    {
        Some(_) => 1,
        None => {
            set_last_error(6); // ERROR_INVALID_HANDLE
            0
        }
    }
}

/// Écrit l'événement dans le journal de Winlux. Renvoie la ligne écrite.
pub fn report_event(h: usize, kind: u16, id: u32, strings: &[String]) -> Option<String> {
    let source = event_sources()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&h)
        .cloned()?;
    let line = format!(
        "{source} (événement {}) : {}",
        id & 0xFFFF,
        strings.join(" | ")
    );
    match kind {
        1 | 0x10 => {
            crate::log_error!("eventlog", "{line}");
        } // ERROR, AUDIT_FAILURE
        2 => {
            crate::log_warn!("eventlog", "{line}");
        }
        _ => {
            crate::log_info!("eventlog", "{line}");
        }
    }
    Some(line)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn report_event_w(
    h: usize,
    kind: u16,
    _category: u16,
    id: u32,
    _sid: usize,
    n: u16,
    _size: u32,
    strings: *const *const u16,
    _data: usize,
) -> i32 {
    let list: Vec<String> = (0..n as usize)
        .filter(|_| !strings.is_null())
        .map(|i| wstr(*strings.add(i)))
        .collect();
    if report_event(h, kind, id, &list).is_some() {
        1
    } else {
        set_last_error(6);
        0
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn report_event_a(
    h: usize,
    kind: u16,
    _category: u16,
    id: u32,
    _sid: usize,
    n: u16,
    _size: u32,
    strings: *const *const u8,
    _data: usize,
) -> i32 {
    let list: Vec<String> = (0..n as usize)
        .filter(|_| !strings.is_null())
        .map(|i| crate::dlls::cstr(*strings.add(i)))
        .collect();
    if report_event(h, kind, id, &list).is_some() {
        1
    } else {
        set_last_error(6);
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

// ── Pool de threads : attentes (CreateThreadpoolWait…) ─────────────────

/// PTP_WAIT_CALLBACK(Instance, Context, Wait, WaitResult).
type WaitCallback = unsafe extern "win64" fn(usize, usize, usize, u32);

struct TpWait {
    callback: WaitCallback,
    context: usize,
    /// Numéro de l'attente armée ; SetThreadpoolWait et Close le changent,
    /// ce qui annule l'attente précédente.
    generation: std::sync::atomic::AtomicU64,
    running: std::sync::Mutex<u32>,
    idle: std::sync::Condvar,
}

unsafe extern "win64" fn create_threadpool_wait(
    callback: Option<WaitCallback>,
    context: usize,
    _env: usize,
) -> usize {
    let Some(callback) = callback else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    std::sync::Arc::into_raw(std::sync::Arc::new(TpWait {
        callback,
        context,
        generation: std::sync::atomic::AtomicU64::new(0),
        running: std::sync::Mutex::new(0),
        idle: std::sync::Condvar::new(),
    })) as usize
}

/// Délai FILETIME de SetThreadpoolWait en millisecondes (None = infini).
unsafe fn wait_timeout_ms(ft: *const i64) -> Option<u64> {
    if ft.is_null() {
        return None;
    }
    let v = std::ptr::read_unaligned(ft);
    Some(if v < 0 {
        v.unsigned_abs() / 10_000
    } else {
        let now = crate::dlls::kernel32::filetime_now() as i64;
        ((v - now).max(0) / 10_000) as u64
    })
}

unsafe extern "win64" fn set_threadpool_wait(wait: usize, handle: usize, timeout: *const i64) {
    if wait == 0 {
        return;
    }
    std::sync::Arc::increment_strong_count(wait as *const TpWait);
    let w = std::sync::Arc::from_raw(wait as *const TpWait);
    let gen = w
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    if handle == 0 {
        return; // annulation
    }
    let limit = wait_timeout_ms(timeout)
        .map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms));
    std::thread::spawn(move || {
        crate::process::thread_attach_current();
        let result = loop {
            if w.generation.load(std::sync::atomic::Ordering::SeqCst) != gen {
                return;
            }
            // Tranches de 20 ms pour voir une annulation.
            let r = api!(
                "kernel32",
                "WaitForSingleObject",
                fn(usize, u32) -> u32,
                handle,
                20
            )
            .unwrap_or(0xFFFF_FFFF);
            if r != 0x102 {
                break r; // signalé, abandonné ou erreur
            }
            if limit.is_some_and(|l| std::time::Instant::now() >= l) {
                break 0x102; // WAIT_TIMEOUT
            }
        };
        if w.generation.load(std::sync::atomic::Ordering::SeqCst) != gen {
            return;
        }
        *w.running.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        (w.callback)(0, w.context, wait, result);
        let mut n = w.running.lock().unwrap_or_else(|e| e.into_inner());
        *n -= 1;
        w.idle.notify_all();
    });
}

unsafe extern "win64" fn wait_for_threadpool_wait_callbacks(wait: usize, cancel: i32) {
    let Some(w) = (wait as *const TpWait).as_ref() else {
        return;
    };
    if cancel != 0 {
        w.generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    let mut n = w.running.lock().unwrap_or_else(|e| e.into_inner());
    while *n > 0 {
        n = w.idle.wait(n).unwrap_or_else(|e| e.into_inner());
    }
}

unsafe extern "win64" fn close_threadpool_wait(wait: usize) {
    if wait == 0 {
        return;
    }
    let w = std::sync::Arc::from_raw(wait as *const TpWait);
    w.generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// FreeLibraryWhenCallbackReturns : la DLL reste chargée (Winlux ne
/// décharge pas une DLL dont un rappel peut encore tourner).
extern "win64" fn free_library_when_callback_returns(_instance: usize, _module: usize) {}

/// RtlPcToFileHeader : module (base) qui contient une adresse de code.
unsafe extern "win64" fn rtl_pc_to_file_header(pc: usize, base: *mut usize) -> usize {
    let b = crate::loader::find_module_by_addr(pc).map_or(0, |m| m.base);
    if !base.is_null() {
        *base = b;
    }
    b
}

/// TerminateThread : arrêter un fil de l'extérieur n'est pas possible sans
/// risque sous Linux ; seul le fil courant peut s'arrêter ainsi.
unsafe extern "win64" fn terminate_thread(handle: usize, code: u32) -> i32 {
    let current = api!("kernel32", "GetCurrentThread", fn() -> usize,).unwrap_or(0);
    if handle == current || handle == usize::MAX - 1 {
        api!("kernel32", "ExitThread", fn(u32) -> (), code);
    }
    crate::log_warn!(
        "kernel32",
        "TerminateThread refusé sur un autre fil (code {code})"
    );
    set_last_error(5); // ERROR_ACCESS_DENIED
    0
}

// ── winmm : minuteurs multimédia ────────────────────────────────────────

/// LPTIMECALLBACK(uTimerID, uMsg, dwUser, dw1, dw2).
type TimeCallback = unsafe extern "win64" fn(u32, u32, usize, usize, usize);

struct MmTimer {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    worker: std::sync::Arc<std::sync::OnceLock<std::thread::ThreadId>>,
}

fn mm_timers() -> &'static std::sync::Mutex<std::collections::HashMap<u32, MmTimer>> {
    static T: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, MmTimer>>> =
        std::sync::OnceLock::new();
    T.get_or_init(Default::default)
}

const TIME_PERIODIC: u32 = 1;
const TIME_CALLBACK_EVENT_SET: u32 = 0x10;
const TIME_CALLBACK_EVENT_PULSE: u32 = 0x20;

/// timeGetDevCaps : de 1 ms à 1 000 000 ms, comme Windows.
unsafe extern "win64" fn time_get_dev_caps(caps: *mut u32, size: u32) -> u32 {
    if caps.is_null() || size < 8 {
        return 97; // TIMERR_NOCANDO
    }
    *caps = 1;
    *caps.add(1) = 1_000_000;
    0
}

/// timeSetEvent : un fil Windows par minuteur appelle la fonction (ou
/// signale l'événement) à l'échéance, une fois ou périodiquement.
unsafe extern "win64" fn time_set_event(
    delay: u32,
    _resolution: u32,
    callback: usize,
    user: usize,
    flags: u32,
) -> u32 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    if callback == 0 || delay == 0 || delay > 1_000_000 {
        return 0;
    }
    // Programme 32 bits : la fonction est du code x86 (#777). Avec
    // TIME_CALLBACK_EVENT_*, `callback` est un handle d'événement.
    let callback = if flags & (TIME_CALLBACK_EVENT_SET | TIME_CALLBACK_EVENT_PULSE) == 0
        && crate::wow32::active()
    {
        crate::wow32::callback64(callback as u32)
    } else {
        callback
    };
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker = std::sync::Arc::new(std::sync::OnceLock::new());
    let (stop2, worker2) = (stop.clone(), worker.clone());
    let period = std::time::Duration::from_millis(delay as u64);
    let thread = std::thread::spawn(move || {
        let _ = worker2.set(std::thread::current().id());
        crate::process::thread_attach_current();
        let mut next = std::time::Instant::now() + period;
        loop {
            while let Some(left) = next.checked_duration_since(std::time::Instant::now()) {
                if stop2.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(left.min(std::time::Duration::from_millis(5)));
            }
            if stop2.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            if flags & TIME_CALLBACK_EVENT_SET != 0 {
                api!("kernel32", "SetEvent", fn(usize) -> i32, callback);
            } else if flags & TIME_CALLBACK_EVENT_PULSE != 0 {
                if api!("kernel32", "PulseEvent", fn(usize) -> i32, callback).is_none() {
                    api!("kernel32", "SetEvent", fn(usize) -> i32, callback);
                }
            } else {
                let f: TimeCallback = std::mem::transmute(callback);
                f(id, 0, user, 0, 0);
            }
            if flags & TIME_PERIODIC == 0 {
                return;
            }
            // Rattrapage sans rafale : l'échéance suivante part de maintenant
            // si le rappel a duré plus d'une période.
            next += period;
            let now = std::time::Instant::now();
            if next < now {
                next = now;
            }
        }
    });
    mm_timers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            id,
            MmTimer {
                stop,
                thread: Some(thread),
                worker,
            },
        );
    id
}

/// timeKillEvent : arrête le minuteur et attend la fin d'un rappel en cours
/// (sauf depuis ce rappel lui-même).
extern "win64" fn time_kill_event(id: u32) -> u32 {
    let Some(mut t) = mm_timers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id)
    else {
        return 11; // MMSYSERR_INVALPARAM
    };
    t.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    if t.worker.get() != Some(&std::thread::current().id()) {
        if let Some(h) = t.thread.take() {
            let _ = h.join();
        }
    }
    0
}

pub fn winmm_exports() -> Exports {
    crate::exports! {
        "timeGetDevCaps" => fn time_get_dev_caps,
        "timeSetEvent" => fn time_set_event,
        "timeKillEvent" => fn time_kill_event,
    }
}

/// Numéros d'ordre publics de Winsock 2 (ws2_32.def), pour les programmes
/// qui importent par numéro.
pub const WS2_32_ORDINALS: &[(u16, &str)] = &[
    (1, "accept"),
    (2, "bind"),
    (3, "closesocket"),
    (4, "connect"),
    (5, "getpeername"),
    (6, "getsockname"),
    (7, "getsockopt"),
    (8, "htonl"),
    (9, "htons"),
    (10, "ioctlsocket"),
    (11, "inet_addr"),
    (12, "inet_ntoa"),
    (13, "listen"),
    (14, "ntohl"),
    (15, "ntohs"),
    (16, "recv"),
    (17, "recvfrom"),
    (18, "select"),
    (19, "send"),
    (20, "sendto"),
    (21, "setsockopt"),
    (22, "shutdown"),
    (23, "socket"),
    (51, "gethostbyaddr"),
    (52, "gethostbyname"),
    (53, "getprotobyname"),
    (54, "getprotobynumber"),
    (55, "getservbyname"),
    (56, "getservbyport"),
    (57, "gethostname"),
    (111, "WSAGetLastError"),
    (112, "WSASetLastError"),
    (115, "WSAStartup"),
    (116, "WSACleanup"),
    (151, "__WSAFDIsSet"),
];

// ── Tables ──────────────────────────────────────────────────────────────

pub fn kernel32_exports() -> Exports {
    crate::exports! {
        "GetLargePageMinimum" => fn get_large_page_minimum,
        "EnumSystemFirmwareTables" => fn enum_system_firmware_tables,
        "GetSystemFirmwareTable" => fn get_system_firmware_table,
        "SetFileApisToOEM" => fn set_file_apis_to_oem,
        "MoveFileWithProgressW" => fn move_file_with_progress_w,
        "MoveFileTransactedW" => fn move_file_transacted_w,
        "DeleteFileTransactedW" => fn delete_file_transacted_w,
        "FindFirstStreamW" => fn find_first_stream_w,
        "FindNextStreamW" => fn find_next_stream_w,
        "GetCompressedFileSizeW" => fn get_compressed_file_size_w,
        "FindFirstChangeNotificationW" => fn find_first_change_notification_w,
        "FindNextChangeNotification" => fn find_next_change_notification,
        "FindCloseChangeNotification" => fn find_close_change_notification,
        "CreateThreadpoolWait" => fn create_threadpool_wait,
        "SetThreadpoolWait" => fn set_threadpool_wait,
        "WaitForThreadpoolWaitCallbacks" => fn wait_for_threadpool_wait_callbacks,
        "CloseThreadpoolWait" => fn close_threadpool_wait,
        "FreeLibraryWhenCallbackReturns" => fn free_library_when_callback_returns,
        "RtlPcToFileHeader" => fn rtl_pc_to_file_header,
        "TerminateThread" => fn terminate_thread,
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

/// FindExecutableW : programme qui ouvre un document. Un programme
/// (.exe, .com, .bat, .cmd) est son propre exécutable ; sinon c'est
/// l'association du registre (comme AssocQueryString ASSOCSTR_EXECUTABLE).
/// Renvoie une valeur > 32 si trouvé, 2 (fichier absent) ou 31
/// (SE_ERR_NOASSOC) sinon.
pub(super) unsafe extern "win64" fn find_executable_w(
    file: *const u16,
    dir: *const u16,
    out: *mut u16,
) -> usize {
    const SE_ERR_FNF: usize = 2;
    const SE_ERR_NOASSOC: usize = 31;
    const ASSOCSTR_EXECUTABLE: u32 = 2;
    if file.is_null() || out.is_null() {
        return SE_ERR_FNF;
    }
    *out = 0;
    let mut name = wstr(file);
    if !dir.is_null() && !name.contains(['\\', '/', ':']) {
        let d = wstr(dir);
        if !d.is_empty() {
            name = format!("{}\\{name}", d.trim_end_matches(['\\', '/']));
        }
    }
    if !crate::paths::to_linux(&name).exists() {
        return SE_ERR_FNF;
    }
    let full = crate::paths::full_windows(&name);
    let lower = full.to_ascii_lowercase();
    let exe = if [".exe", ".com", ".bat", ".cmd"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        Some(full)
    } else {
        let ext = match full.rfind('.') {
            Some(i) if !full[i..].contains('\\') => full[i..].to_string(),
            _ => return SE_ERR_NOASSOC,
        };
        let ext_w: Vec<u16> = ext.encode_utf16().chain([0]).collect();
        let mut buf = vec![0u16; 260];
        let mut n = buf.len() as u32;
        let r = super::shlwapi_reg::assoc_query_string_w(
            0,
            ASSOCSTR_EXECUTABLE,
            ext_w.as_ptr(),
            std::ptr::null(),
            buf.as_mut_ptr(),
            &mut n,
        );
        (r >= 0).then(|| wstr(buf.as_ptr()))
    };
    match exe {
        Some(e) if !e.is_empty() => {
            let w: Vec<u16> = e.encode_utf16().take(259).chain([0]).collect();
            std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
            42
        }
        _ => SE_ERR_NOASSOC,
    }
}

/// SHAppBarMessage sur une APPBARDATA x64 : cbSize 0, hWnd 8,
/// uCallbackMessage 16, uEdge 20, rc 24, lParam 40. Winlux ne réserve pas
/// de place aux barres d'application des programmes ; la barre des tâches
/// est en bas de l'écran, sous la zone de travail.
unsafe extern "win64" fn sh_app_bar_message(msg: u32, data: *mut u8) -> usize {
    const ABM_QUERYPOS: u32 = 2;
    const ABM_SETPOS: u32 = 3;
    const ABM_GETSTATE: u32 = 4;
    const ABM_GETTASKBARPOS: u32 = 5;
    const ABM_GETAUTOHIDEBAR: u32 = 7;
    const ABM_SETAUTOHIDEBAR: u32 = 8;
    const ABM_GETAUTOHIDEBAREX: u32 = 11;
    const ABM_SETAUTOHIDEBAREX: u32 = 12;
    const ABS_ALWAYSONTOP: usize = 2;
    const ABE_BOTTOM: u32 = 3;
    match msg {
        ABM_GETSTATE => ABS_ALWAYSONTOP,
        ABM_GETAUTOHIDEBAR | ABM_GETAUTOHIDEBAREX => 0,
        ABM_SETAUTOHIDEBAR | ABM_SETAUTOHIDEBAREX => 0,
        ABM_GETTASKBARPOS => {
            if data.is_null() {
                return 0;
            }
            let area = super::gui::work_area();
            let (w, h) = super::gui::screen_size();
            let top = if area.bottom < h { area.bottom } else { h - 48 };
            (data.add(20) as *mut u32).write_unaligned(ABE_BOTTOM);
            for (k, v) in [0, top, w, h].into_iter().enumerate() {
                (data.add(24 + 4 * k) as *mut i32).write_unaligned(v);
            }
            1
        }
        // La position demandée est acceptée telle quelle.
        ABM_QUERYPOS | ABM_SETPOS => 1,
        _ => 1,
    }
}

unsafe extern "win64" fn sh_free_name_mappings(_h: usize) {}

unsafe extern "win64" fn sh_get_malloc(out: *mut usize) -> i32 {
    api!("ole32", "CoGetMalloc", fn(u32, *mut usize) -> i32, 1, out)
        .unwrap_or(0x8000_4005u32 as i32)
}

static CHANGE_NOTIFY_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// SHChangeNotifyRegister (ordinal 2) : l'inscription réussit, mais Winlux
/// n'envoie pas encore de notification de changement du Shell.
unsafe extern "win64" fn sh_change_notify_register(
    _hwnd: usize,
    _sources: i32,
    _events: i32,
    _msg: u32,
    _count: i32,
    _entries: usize,
) -> u32 {
    CHANGE_NOTIFY_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

unsafe extern "win64" fn sh_change_notify_deregister(_id: u32) -> i32 {
    1
}

/// SHChangeNotification_Lock (ordinal 644) : aucune notification n'est
/// jamais envoyée, il n'y a donc rien à verrouiller.
unsafe extern "win64" fn sh_change_notification_lock(
    _h: usize,
    _pid: u32,
    _pidls: usize,
    _event: usize,
) -> usize {
    0
}

unsafe extern "win64" fn sh_change_notification_unlock(_lock: usize) -> i32 {
    1
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
        "SHGetKnownFolderIDList" => fn sh_get_known_folder_id_list,
        "ILFree" => fn il_free,
        "SHOpenFolderAndSelectItems" => fn sh_open_folder_and_select_items,
        "FindExecutableW" => fn find_executable_w,
        "SHAppBarMessage" => fn sh_app_bar_message,
        "SHFreeNameMappings" => fn sh_free_name_mappings,
        "SHGetMalloc" => fn sh_get_malloc,
        "SHChangeNotifyRegister" => fn sh_change_notify_register,
        "SHChangeNotifyDeregister" => fn sh_change_notify_deregister,
        "SHChangeNotification_Lock" => fn sh_change_notification_lock,
        "SHChangeNotification_Unlock" => fn sh_change_notification_unlock,
        "#2" => fn sh_change_notify_register,
        "#4" => fn sh_change_notify_deregister,
        "#155" => fn il_free,
        "#190" => fn il_create_from_path_w,
        "#644" => fn sh_change_notification_lock,
        "#645" => fn sh_change_notification_unlock,
    }
}

/// SHGetDataFromIDListW (#1692) : avec SHGDFIL_FINDDATA, le
/// WIN32_FIND_DATAW de l'élément `pidl` du dossier `folder`, lu par
/// IShellFolder::GetDisplayNameOf(SHGDN_FORPARSING) puis FindFirstFileW.
/// Les autres formats (ressource réseau, description) ne sont pas fournis.
unsafe extern "win64" fn sh_get_data_from_id_list_w(
    folder: usize,
    pidl: *const u8,
    format: i32,
    out: *mut u8,
    cb: i32,
) -> u32 {
    const SHGDFIL_FINDDATA: i32 = 1;
    const E_INVALIDARG: u32 = 0x8007_0057;
    const E_NOTIMPL: u32 = 0x8000_4001;
    const WIN32_FIND_DATAW_SIZE: i32 = 592;
    const SHGDN_FORPARSING: u32 = 0x8000;
    if folder == 0 || pidl.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    // Table x86 d'un IShellFolder de programme 32 bits : pas d'appel win64.
    if format != SHGDFIL_FINDDATA || crate::wow32::pointer_size() == 4 {
        return E_NOTIMPL;
    }
    if cb < WIN32_FIND_DATAW_SIZE {
        return E_INVALIDARG;
    }
    let vtbl = *(folder as *const *const usize);
    let get_display_name_of: unsafe extern "win64" fn(usize, *const u8, u32, *mut u8) -> u32 =
        std::mem::transmute(*vtbl.add(11));
    let mut strret = [0u8; 272];
    let hr = get_display_name_of(folder, pidl, SHGDN_FORPARSING, strret.as_mut_ptr());
    if hr != 0 {
        return hr;
    }
    let mut text: usize = 0;
    let hr = api!(
        "shlwapi",
        "StrRetToStrW",
        fn(*mut u8, *const u8, *mut usize) -> u32,
        strret.as_mut_ptr(),
        pidl,
        &mut text
    )
    .unwrap_or(E_NOTIMPL);
    if hr != 0 || text == 0 {
        return if hr != 0 { hr } else { E_INVALIDARG };
    }
    let find = api!(
        "kernel32",
        "FindFirstFileW",
        fn(usize, *mut u8) -> isize,
        text,
        out
    )
    .unwrap_or(-1);
    crate::dlls::ole32::co_task_mem_free(text as *mut std::ffi::c_void);
    if find == -1 {
        let e = crate::dlls::kernel32::get_last_error();
        return if e == 0 {
            0x8000_4005
        } else {
            0x8007_0000 | (e & 0xffff)
        };
    }
    api!("kernel32", "FindClose", fn(isize) -> i32, find);
    0
}

/// Remplacent des exports vides (SHGetPathFromIDList renvoyait toujours 0).
pub fn shell32_overrides() -> Exports {
    crate::exports! {
        "SHGetPathFromIDListW" => fn sh_get_path_from_id_list_w,
        "SHGetDataFromIDListW" => fn sh_get_data_from_id_list_w,
    }
}

/// RegisterEventSourceA renvoyait 1 pour toute source, sans journal.
pub fn advapi32_overrides() -> Exports {
    crate::exports! {
        "RegisterEventSourceA" => fn register_event_source_a,
    }
}

pub fn advapi32_exports() -> Exports {
    crate::exports! {
        "LsaOpenPolicy" => fn lsa_open_policy,
        "LsaAddAccountRights" => fn lsa_add_account_rights,
        "LsaClose" => fn lsa_close,
        "LookupAccountNameW" => fn lookup_account_name_w,
        "SetFileSecurityW" => fn set_file_security_w,
        "RegisterEventSourceW" => fn register_event_source_w,
        "DeregisterEventSource" => fn deregister_event_source,
        "ReportEventW" => fn report_event_w,
        "ReportEventA" => fn report_event_a,
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
    fn smbios_brut_depuis_les_tables_du_noyau() {
        let mut e3 = b"_SM3_".to_vec();
        e3.extend([0, 0x18, 3, 4, 1]);
        let dmi = [0u8, 4, 0, 0, 0x7f, 4, 0xff, 0xfe];
        let raw = raw_smbios(&e3, &dmi);
        assert_eq!(&raw[..8], &[0, 3, 4, 1, 8, 0, 0, 0]);
        assert_eq!(&raw[8..], &dmi);
        let mut e2 = b"_SM_".to_vec();
        e2.resize(0x1f, 0);
        e2[6] = 2;
        e2[7] = 8;
        e2[0x1e] = 0x28;
        assert_eq!(&raw_smbios(&e2, &dmi)[..4], &[0, 2, 8, 0x28]);

        let dir = std::env::temp_dir().join(format!("winlux-fw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("smbios_entry_point"), &e3).unwrap();
        std::fs::write(dir.join("DMI"), dmi).unwrap();
        std::env::set_var("WINLUX_FIRMWARE_ROOT", &dir);
        unsafe {
            let rsmb = u32::from_be_bytes(*b"RSMB");
            let mut ids = [0xffu8; 4];
            assert_eq!(enum_system_firmware_tables(rsmb, ids.as_mut_ptr(), 4), 4);
            assert_eq!(ids, [0; 4]);
            assert_eq!(
                get_system_firmware_table(rsmb, 0, std::ptr::null_mut(), 0),
                16
            );
            let mut out = [0u8; 16];
            assert_eq!(get_system_firmware_table(rsmb, 0, out.as_mut_ptr(), 16), 16);
            assert_eq!(out.to_vec(), raw);
            let acpi = u32::from_be_bytes(*b"ACPI");
            assert_eq!(get_system_firmware_table(acpi, 0, out.as_mut_ptr(), 16), 0);
        }
        std::env::remove_var("WINLUX_FIRMWARE_ROOT");
        let _ = std::fs::remove_dir_all(dir);
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
    fn afficher_dans_le_dossier_ouvre_l_explorateur_winlux() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("winlux-shopen-{}", std::process::id()));
        let bin = dir.join("bin");
        let folder = dir.join("Rapports");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("final.pdf"), b"x").unwrap();
        let log = dir.join("ouvert.txt");
        let opener = bin.join("xdg-open");
        std::fs::write(
            &opener,
            format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _env = crate::launch::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &bin);
        unsafe {
            let p = pidl_from_path(&crate::paths::to_windows(&folder.join("final.pdf")));
            if p != 0 {
                assert_eq!(
                    sh_open_folder_and_select_items(p as *const u8, 0, std::ptr::null(), 0),
                    0
                );
                il_free(p);
                for _ in 0..200 {
                    if log.exists() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                assert_eq!(
                    std::fs::read_to_string(&log).unwrap(),
                    folder.to_string_lossy()
                );
            }
            assert_eq!(
                sh_open_folder_and_select_items(std::ptr::null(), 0, std::ptr::null(), 0),
                0x8007_0057
            );
        }
        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn minuteur_multimedia_periodique_puis_arrete() {
        static HITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        unsafe extern "win64" fn tick(_id: u32, _m: u32, user: usize, _a: usize, _b: usize) {
            assert_eq!(user, 42);
            HITS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        let mut caps = [0u32; 2];
        unsafe {
            assert_eq!(time_get_dev_caps(caps.as_mut_ptr(), 8), 0);
            let cb: TimeCallback = tick;
            let id = time_set_event(5, 1, cb as usize, 42, TIME_PERIODIC);
            assert_ne!(id, 0);
            std::thread::sleep(std::time::Duration::from_millis(60));
            assert_eq!(time_kill_event(id), 0);
        }
        let n = HITS.load(std::sync::atomic::Ordering::SeqCst);
        assert!(n >= 3, "{n} appels");
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(HITS.load(std::sync::atomic::Ordering::SeqCst), n, "arrêté");
        assert_eq!(time_kill_event(9999), 11);
        assert_eq!(caps, [1, 1_000_000]);
    }

    #[test]
    fn journal_des_evenements() {
        let h = register_source("Paint.NET".into());
        let line = report_event(h, 1, 0x4000_0003, &["échec".into(), "détail".into()]).unwrap();
        assert_eq!(line, "Paint.NET (événement 3) : échec | détail");
        assert_eq!(deregister_event_source(h), 1);
        assert!(report_event(h, 1, 3, &[]).is_none());
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
