//! Raccourcis Windows par COM : `CLSID_ShellLink` avec IShellLinkW,
//! IShellLinkA, IPersistFile et IPropertyStore (#1732).
//!
//! Discord et les autres programmes Electron vérifient ou créent leur
//! raccourci au démarrage (`CoCreateInstance(CLSID_ShellLink)`, puis
//! `IPersistFile::Load` / `Save`). Sans cette classe, le module de mise à
//! jour de Discord paniquait et le programme se fermait sans fenêtre.
//!
//! `Save` écrit un vrai fichier .lnk au format public [MS-SHLLINK] : la
//! session Winlux (winlux-applications) en fait un lanceur du menu ou du
//! Bureau, comme pour les raccourcis posés par les installateurs. `Load`
//! relit aussi les .lnk produits ailleurs (liste d'identifiants, bloc
//! LinkInfo, chaînes ANSI ou Unicode).
//!
//! Disposition des tables : shobjidl.h, objidl.h et propsys.h de mingw-w64.
//! Les propriétés (IPropertyStore, dont System.AppUserModel.ID) sont gardées
//! en mémoire pour l'objet ; elles ne sont pas écrites dans le fichier.

use super::audio_com::{guid, read_guid, E_INVALIDARG, E_NOINTERFACE, E_POINTER, S_FALSE, S_OK};
use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

pub const CLSID_SHELL_LINK: Guid = guid(0x0002_1401, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ISHELL_LINK_W: Guid = guid(0x0002_14F9, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ISHELL_LINK_A: Guid = guid(0x0002_14EE, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IPERSIST: Guid = guid(0x0000_010C, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IPERSIST_FILE: Guid = guid(0x0000_010B, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IPROPERTY_STORE: Guid = guid(
    0x886D_8EEB,
    0x8CF2,
    0x4446,
    [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x99],
);

const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const HR_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;
const HR_PATH_NOT_FOUND: i32 = 0x8007_0003u32 as i32;
const HR_INVALID_DATA: i32 = 0x8007_000Du32 as i32;

// ---------- Contenu d'un raccourci ----------

/// Ce que porte un raccourci (chemins Windows).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkData {
    pub target: String,
    pub args: String,
    pub workdir: String,
    pub desc: String,
    pub icon: String,
    pub icon_index: i32,
    /// SW_* (1 : normal).
    pub show_cmd: i32,
    pub hotkey: u16,
    pub relative: String,
}

const HAS_ID_LIST: u32 = 0x01;
const HAS_LINK_INFO: u32 = 0x02;
const HAS_NAME: u32 = 0x04;
const HAS_RELATIVE_PATH: u32 = 0x08;
const HAS_WORKING_DIR: u32 = 0x10;
const HAS_ARGUMENTS: u32 = 0x20;
const HAS_ICON_LOCATION: u32 = 0x40;
const IS_UNICODE: u32 = 0x80;

/// Octets d'un .lnk ([MS-SHLLINK]) : la cible est écrite comme chemin
/// relatif absolu (`C:\…`), lu par Windows comme par winlux-applications.
pub fn link_bytes(l: &LinkData) -> Vec<u8> {
    let mut flags = IS_UNICODE | HAS_RELATIVE_PATH;
    let fields = [
        (HAS_NAME, &l.desc),
        (HAS_WORKING_DIR, &l.workdir),
        (HAS_ARGUMENTS, &l.args),
        (HAS_ICON_LOCATION, &l.icon),
    ];
    for (bit, s) in fields {
        if !s.is_empty() {
            flags |= bit;
        }
    }
    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(&0x4Cu32.to_le_bytes());
    out.extend_from_slice(&[
        0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46,
    ]);
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&0x20u32.to_le_bytes()); // FILE_ATTRIBUTE_ARCHIVE
    out.extend_from_slice(&[0; 24]); // dates
    out.extend_from_slice(&0u32.to_le_bytes()); // taille
    out.extend_from_slice(&l.icon_index.to_le_bytes());
    let show = if l.show_cmd == 0 { 1 } else { l.show_cmd };
    out.extend_from_slice(&(show as u32).to_le_bytes());
    out.extend_from_slice(&l.hotkey.to_le_bytes());
    out.extend_from_slice(&[0; 2 + 4 + 4]);
    let mut put = |s: &str| {
        let units: Vec<u16> = s.encode_utf16().take(u16::MAX as usize).collect();
        out.extend_from_slice(&(units.len() as u16).to_le_bytes());
        for u in units {
            out.extend_from_slice(&u.to_le_bytes());
        }
    };
    if !l.desc.is_empty() {
        put(&l.desc);
    }
    put(if l.target.is_empty() {
        &l.relative
    } else {
        &l.target
    });
    if !l.workdir.is_empty() {
        put(&l.workdir);
    }
    if !l.args.is_empty() {
        put(&l.args);
    }
    if !l.icon.is_empty() {
        put(&l.icon);
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // TerminalBlock
    out
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn ansi(b: &[u8]) -> String {
    // Windows-1252 approché par Latin-1 : les chemins de raccourcis ANSI
    // sont rares et presque toujours en ASCII.
    b.iter().map(|&c| c as char).collect()
}

/// Lit un .lnk ; `None` s'il est tronqué ou n'en est pas un.
pub fn parse_link(d: &[u8]) -> Option<LinkData> {
    if u32_at(d, 0)? != 0x4C || d.len() < 0x4C {
        return None;
    }
    let flags = u32_at(d, 20)?;
    let mut l = LinkData {
        icon_index: u32_at(d, 56)? as i32,
        show_cmd: u32_at(d, 60)? as i32,
        hotkey: u16_at(d, 64)?,
        ..Default::default()
    };
    let mut at = 0x4C;
    if flags & HAS_ID_LIST != 0 {
        at += 2 + u16_at(d, at)? as usize;
    }
    if flags & HAS_LINK_INFO != 0 {
        let size = u32_at(d, at)? as usize;
        let header = u32_at(d, at + 4)? as usize;
        let info = d.get(at..at + size)?;
        let local = u32_at(info, 16).unwrap_or(0) as usize;
        let local_u = if header >= 0x24 {
            u32_at(info, 28).unwrap_or(0) as usize
        } else {
            0
        };
        if local_u != 0 {
            let raw = info.get(local_u..)?;
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&c| c != 0)
                .collect();
            l.target = String::from_utf16_lossy(&units);
        } else if local != 0 {
            let raw = info.get(local..)?;
            let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
            l.target = ansi(&raw[..end]);
        }
        at += size;
    }
    let wide = flags & IS_UNICODE != 0;
    for bit in [
        HAS_NAME,
        HAS_RELATIVE_PATH,
        HAS_WORKING_DIR,
        HAS_ARGUMENTS,
        HAS_ICON_LOCATION,
    ] {
        if flags & bit == 0 {
            continue;
        }
        let n = u16_at(d, at)? as usize;
        at += 2;
        let bytes = if wide { n * 2 } else { n };
        let raw = d.get(at..at + bytes)?;
        at += bytes;
        let s = if wide {
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            ansi(raw)
        };
        match bit {
            HAS_NAME => l.desc = s,
            HAS_RELATIVE_PATH => l.relative = s,
            HAS_WORKING_DIR => l.workdir = s,
            HAS_ARGUMENTS => l.args = s,
            _ => l.icon = s,
        }
    }
    // Sans LinkInfo, un chemin relatif absolu (`C:\…`, écrit par Winlux ou
    // par d'autres outils) désigne la cible.
    if l.target.is_empty() && l.relative.as_bytes().get(1) == Some(&b':') {
        l.target = l.relative.clone();
    }
    Some(l)
}

// ---------- Objet COM ----------

/// Valeur d'IPropertyStore gardée par l'objet : clé (fmtid + pid) et
/// PROPVARIANT (VT_LPWSTR copié, sinon les 16 octets bruts).
struct Prop {
    key: [u8; 20],
    vt: u16,
    raw: [u8; 16],
    text: Option<Vec<u16>>,
}

#[derive(Default)]
struct State {
    link: LinkData,
    file: Option<String>,
    dirty: bool,
    props: Vec<Prop>,
}

/// Une table par interface, comme un objet C++ à héritage multiple : chaque
/// interface reçoit l'adresse de sa propre table et retrouve l'objet.
#[repr(C)]
struct Link {
    vt_w: *const (),
    vt_a: *const (),
    vt_file: *const (),
    vt_props: *const (),
    refs: AtomicU32,
    state: Mutex<State>,
}

const OFF_W: usize = 0;
const OFF_A: usize = 8;
const OFF_FILE: usize = 16;
const OFF_PROPS: usize = 24;

unsafe fn base(this: *mut c_void, off: usize) -> *mut Link {
    (this as *mut u8).sub(off) as *mut Link
}

fn lock(l: &Link) -> std::sync::MutexGuard<'_, State> {
    l.state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Classe ShellLink pour CoCreateInstance.
///
/// # Safety
/// `out` doit être nul ou inscriptible.
pub unsafe fn create_builtin(clsid: &Guid, riid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    // Tables 64 bits seulement : un programme x86 garde l'ancien refus
    // (REGDB_E_CLASSNOTREG) plutôt que d'appeler du code d'une autre ABI.
    if *clsid != CLSID_SHELL_LINK || crate::wow32::active() {
        return None;
    }
    let obj = Box::into_raw(Box::new(Link {
        vt_w: VT_W.0.as_ptr() as *const (),
        vt_a: VT_A.0.as_ptr() as *const (),
        vt_file: VT_FILE.0.as_ptr() as *const (),
        vt_props: VT_PROPS.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        state: Mutex::new(State {
            link: LinkData {
                show_cmd: 1,
                ..Default::default()
            },
            ..Default::default()
        }),
    }));
    let hr = query(obj, riid, out);
    release(obj);
    Some(hr)
}

unsafe fn query(obj: *mut Link, riid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(g) = read_guid(riid) else {
        return E_INVALIDARG;
    };
    let off = if g == IID_IUNKNOWN || g == IID_ISHELL_LINK_W {
        OFF_W
    } else if g == IID_ISHELL_LINK_A {
        OFF_A
    } else if g == IID_IPERSIST_FILE || g == IID_IPERSIST {
        OFF_FILE
    } else if g == IID_IPROPERTY_STORE {
        OFF_PROPS
    } else {
        return E_NOINTERFACE;
    };
    (*obj).refs.fetch_add(1, Ordering::AcqRel);
    *out = (obj as *mut u8).add(off) as *mut c_void;
    S_OK
}

unsafe fn release(obj: *mut Link) -> u32 {
    let left = (*obj).refs.fetch_sub(1, Ordering::AcqRel).wrapping_sub(1);
    if left == 0 {
        drop(Box::from_raw(obj));
    }
    left
}

macro_rules! unknown {
    ($qi:ident, $add:ident, $rel:ident, $off:expr) => {
        unsafe extern "win64" fn $qi(
            this: *mut c_void,
            riid: *const Guid,
            out: *mut *mut c_void,
        ) -> i32 {
            query(base(this, $off), riid, out)
        }
        unsafe extern "win64" fn $add(this: *mut c_void) -> u32 {
            (*base(this, $off)).refs.fetch_add(1, Ordering::AcqRel) + 1
        }
        unsafe extern "win64" fn $rel(this: *mut c_void) -> u32 {
            release(base(this, $off))
        }
    };
}

unknown!(w_qi, w_add_ref, w_release, OFF_W);
unknown!(a_qi, a_add_ref, a_release, OFF_A);
unknown!(f_qi, f_add_ref, f_release, OFF_FILE);
unknown!(p_qi, p_add_ref, p_release, OFF_PROPS);

// ---------- Chaînes ----------

unsafe fn read_w(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| super::wstr(p))
}

unsafe fn read_a(p: *const u8) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let mut n = 0;
    while *p.add(n) != 0 && n < 32 * 1024 {
        n += 1;
    }
    Some(ansi(std::slice::from_raw_parts(p, n)))
}

/// Copie tronquée dans un tampon de `cch` caractères, terminée par zéro.
unsafe fn write_w(s: &str, out: *mut u16, cch: i32) -> i32 {
    if out.is_null() || cch <= 0 {
        return E_INVALIDARG;
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let n = units.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(units.as_ptr(), out, n);
    *out.add(n) = 0;
    S_OK
}

unsafe fn write_a(s: &str, out: *mut u8, cch: i32) -> i32 {
    if out.is_null() || cch <= 0 {
        return E_INVALIDARG;
    }
    let bytes: Vec<u8> = s
        .chars()
        .map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' })
        .collect();
    let n = bytes.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, n);
    *out.add(n) = 0;
    S_OK
}

/// WIN32_FIND_DATAW/A de la cible : attributs et nom (le reste à zéro).
unsafe fn fill_find_data(target: &str, out: *mut u8, wide: bool) {
    if out.is_null() {
        return;
    }
    let size = if wide { 592 } else { 320 };
    std::ptr::write_bytes(out, 0, size);
    let linux = crate::paths::to_linux(target);
    let attrs: u32 = match std::fs::metadata(&linux) {
        Ok(m) if m.is_dir() => 0x10,
        Ok(m) => {
            let len = m.len();
            std::ptr::write_unaligned(out.add(28) as *mut u32, (len >> 32) as u32);
            std::ptr::write_unaligned(out.add(32) as *mut u32, len as u32);
            0x20
        }
        Err(_) => 0x80,
    };
    std::ptr::write_unaligned(out as *mut u32, attrs);
    let name = target.rsplit('\\').next().unwrap_or(target);
    if wide {
        write_w(name, out.add(44) as *mut u16, 260);
    } else {
        write_a(name, out.add(44), 260);
    }
}

fn with_state<R>(obj: *mut Link, f: impl FnOnce(&mut State) -> R) -> R {
    // SAFETY : l'objet est vivant tant qu'une de ses interfaces est appelée.
    let l = unsafe { &*obj };
    f(&mut lock(l))
}

fn set_field(obj: *mut Link, f: impl FnOnce(&mut LinkData)) -> i32 {
    with_state(obj, |s| {
        f(&mut s.link);
        s.dirty = true;
    });
    S_OK
}

// ---------- IShellLinkW ----------

unsafe extern "win64" fn w_get_path(
    this: *mut c_void,
    out: *mut u16,
    cch: i32,
    fd: *mut u8,
    _flags: u32,
) -> i32 {
    let target = with_state(base(this, OFF_W), |s| s.link.target.clone());
    fill_find_data(&target, fd, true);
    if target.is_empty() {
        if !out.is_null() && cch > 0 {
            *out = 0;
        }
        return S_FALSE;
    }
    write_w(&target, out, cch)
}

unsafe extern "win64" fn w_get_id_list(this: *mut c_void, out: *mut usize) -> i32 {
    get_id_list(base(this, OFF_W), out)
}

unsafe fn get_id_list(obj: *mut Link, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let target = with_state(obj, |s| s.link.target.clone());
    if target.is_empty() {
        return S_FALSE;
    }
    match super::apps_win32::pidl_from_path(&target) {
        0 => E_OUTOFMEMORY,
        p => {
            *out = p;
            S_OK
        }
    }
}

unsafe extern "win64" fn w_set_id_list(this: *mut c_void, pidl: *const u8) -> i32 {
    set_id_list(base(this, OFF_W), pidl)
}

unsafe fn set_id_list(obj: *mut Link, pidl: *const u8) -> i32 {
    match super::apps_win32::path_from_pidl(pidl) {
        Some(p) => set_field(obj, |l| l.target = p),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn w_get_description(this: *mut c_void, out: *mut u16, cch: i32) -> i32 {
    let s = with_state(base(this, OFF_W), |s| s.link.desc.clone());
    write_w(&s, out, cch)
}

unsafe extern "win64" fn w_set_description(this: *mut c_void, s: *const u16) -> i32 {
    let v = read_w(s).unwrap_or_default();
    set_field(base(this, OFF_W), |l| l.desc = v)
}

unsafe extern "win64" fn w_get_working_directory(
    this: *mut c_void,
    out: *mut u16,
    cch: i32,
) -> i32 {
    let s = with_state(base(this, OFF_W), |s| s.link.workdir.clone());
    write_w(&s, out, cch)
}

unsafe extern "win64" fn w_set_working_directory(this: *mut c_void, s: *const u16) -> i32 {
    let v = read_w(s).unwrap_or_default();
    set_field(base(this, OFF_W), |l| l.workdir = v)
}

unsafe extern "win64" fn w_get_arguments(this: *mut c_void, out: *mut u16, cch: i32) -> i32 {
    let s = with_state(base(this, OFF_W), |s| s.link.args.clone());
    write_w(&s, out, cch)
}

unsafe extern "win64" fn w_set_arguments(this: *mut c_void, s: *const u16) -> i32 {
    let v = read_w(s).unwrap_or_default();
    set_field(base(this, OFF_W), |l| l.args = v)
}

unsafe extern "win64" fn w_get_hotkey(this: *mut c_void, out: *mut u16) -> i32 {
    get_hotkey(base(this, OFF_W), out)
}

unsafe fn get_hotkey(obj: *mut Link, out: *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_state(obj, |s| s.link.hotkey);
    S_OK
}

unsafe extern "win64" fn w_set_hotkey(this: *mut c_void, key: u16) -> i32 {
    set_field(base(this, OFF_W), |l| l.hotkey = key)
}

unsafe extern "win64" fn w_get_show_cmd(this: *mut c_void, out: *mut i32) -> i32 {
    get_show_cmd(base(this, OFF_W), out)
}

unsafe fn get_show_cmd(obj: *mut Link, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_state(obj, |s| s.link.show_cmd);
    S_OK
}

unsafe extern "win64" fn w_set_show_cmd(this: *mut c_void, cmd: i32) -> i32 {
    set_field(base(this, OFF_W), |l| l.show_cmd = cmd)
}

unsafe extern "win64" fn w_get_icon_location(
    this: *mut c_void,
    out: *mut u16,
    cch: i32,
    index: *mut i32,
) -> i32 {
    let (s, i) = with_state(base(this, OFF_W), |s| {
        (s.link.icon.clone(), s.link.icon_index)
    });
    if !index.is_null() {
        *index = i;
    }
    write_w(&s, out, cch)
}

unsafe extern "win64" fn w_set_icon_location(this: *mut c_void, s: *const u16, i: i32) -> i32 {
    let v = read_w(s).unwrap_or_default();
    set_field(base(this, OFF_W), |l| {
        l.icon = v;
        l.icon_index = i;
    })
}

unsafe extern "win64" fn w_set_relative_path(this: *mut c_void, s: *const u16, _r: u32) -> i32 {
    let v = read_w(s).unwrap_or_default();
    set_field(base(this, OFF_W), |l| l.relative = v)
}

/// IShellLink::Resolve : la cible est un chemin, rien à rechercher.
unsafe extern "win64" fn w_resolve(_this: *mut c_void, _hwnd: usize, _flags: u32) -> i32 {
    S_OK
}

unsafe extern "win64" fn w_set_path(this: *mut c_void, s: *const u16) -> i32 {
    let v = read_w(s).unwrap_or_default();
    set_field(base(this, OFF_W), |l| l.target = v)
}

// ---------- IShellLinkA ----------

unsafe extern "win64" fn a_get_path(
    this: *mut c_void,
    out: *mut u8,
    cch: i32,
    fd: *mut u8,
    _flags: u32,
) -> i32 {
    let target = with_state(base(this, OFF_A), |s| s.link.target.clone());
    fill_find_data(&target, fd, false);
    if target.is_empty() {
        if !out.is_null() && cch > 0 {
            *out = 0;
        }
        return S_FALSE;
    }
    write_a(&target, out, cch)
}

unsafe extern "win64" fn a_get_id_list(this: *mut c_void, out: *mut usize) -> i32 {
    get_id_list(base(this, OFF_A), out)
}

unsafe extern "win64" fn a_set_id_list(this: *mut c_void, pidl: *const u8) -> i32 {
    set_id_list(base(this, OFF_A), pidl)
}

macro_rules! a_text {
    ($get:ident, $set:ident, $field:ident) => {
        unsafe extern "win64" fn $get(this: *mut c_void, out: *mut u8, cch: i32) -> i32 {
            let s = with_state(base(this, OFF_A), |s| s.link.$field.clone());
            write_a(&s, out, cch)
        }
        unsafe extern "win64" fn $set(this: *mut c_void, s: *const u8) -> i32 {
            let v = read_a(s).unwrap_or_default();
            set_field(base(this, OFF_A), |l| l.$field = v)
        }
    };
}

a_text!(a_get_description, a_set_description, desc);
a_text!(a_get_working_directory, a_set_working_directory, workdir);
a_text!(a_get_arguments, a_set_arguments, args);

unsafe extern "win64" fn a_get_hotkey(this: *mut c_void, out: *mut u16) -> i32 {
    get_hotkey(base(this, OFF_A), out)
}

unsafe extern "win64" fn a_set_hotkey(this: *mut c_void, key: u16) -> i32 {
    set_field(base(this, OFF_A), |l| l.hotkey = key)
}

unsafe extern "win64" fn a_get_show_cmd(this: *mut c_void, out: *mut i32) -> i32 {
    get_show_cmd(base(this, OFF_A), out)
}

unsafe extern "win64" fn a_set_show_cmd(this: *mut c_void, cmd: i32) -> i32 {
    set_field(base(this, OFF_A), |l| l.show_cmd = cmd)
}

unsafe extern "win64" fn a_get_icon_location(
    this: *mut c_void,
    out: *mut u8,
    cch: i32,
    index: *mut i32,
) -> i32 {
    let (s, i) = with_state(base(this, OFF_A), |s| {
        (s.link.icon.clone(), s.link.icon_index)
    });
    if !index.is_null() {
        *index = i;
    }
    write_a(&s, out, cch)
}

unsafe extern "win64" fn a_set_icon_location(this: *mut c_void, s: *const u8, i: i32) -> i32 {
    let v = read_a(s).unwrap_or_default();
    set_field(base(this, OFF_A), |l| {
        l.icon = v;
        l.icon_index = i;
    })
}

unsafe extern "win64" fn a_set_relative_path(this: *mut c_void, s: *const u8, _r: u32) -> i32 {
    let v = read_a(s).unwrap_or_default();
    set_field(base(this, OFF_A), |l| l.relative = v)
}

unsafe extern "win64" fn a_set_path(this: *mut c_void, s: *const u8) -> i32 {
    let v = read_a(s).unwrap_or_default();
    set_field(base(this, OFF_A), |l| l.target = v)
}

// ---------- IPersistFile ----------

unsafe extern "win64" fn f_get_class_id(_this: *mut c_void, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    std::ptr::write_unaligned(out, CLSID_SHELL_LINK);
    S_OK
}

unsafe extern "win64" fn f_is_dirty(this: *mut c_void) -> i32 {
    if with_state(base(this, OFF_FILE), |s| s.dirty) {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn f_load(this: *mut c_void, name: *const u16, _mode: u32) -> i32 {
    let Some(name) = read_w(name).filter(|n| !n.is_empty()) else {
        return E_INVALIDARG;
    };
    let path = crate::paths::to_linux(&name);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HR_FILE_NOT_FOUND,
        Err(_) => return E_FAIL,
    };
    let Some(link) = parse_link(&bytes) else {
        return HR_INVALID_DATA;
    };
    with_state(base(this, OFF_FILE), |s| {
        s.link = link;
        s.file = Some(name);
        s.dirty = false;
    });
    S_OK
}

unsafe extern "win64" fn f_save(this: *mut c_void, name: *const u16, remember: i32) -> i32 {
    let obj = base(this, OFF_FILE);
    let (link, current) = with_state(obj, |s| (s.link.clone(), s.file.clone()));
    let Some(win) = read_w(name).filter(|n| !n.is_empty()).or(current) else {
        return E_INVALIDARG;
    };
    let hr = save_link(&win, &link);
    if hr == S_OK {
        with_state(obj, |s| {
            s.dirty = false;
            if remember != 0 || s.file.is_none() {
                s.file = Some(win.clone());
            }
        });
    }
    hr
}

/// Écrit le raccourci : un raccourci posé sur le Bureau est d'abord inscrit
/// dans `.hidden`, la session affiche à sa place un lanceur au nom du
/// programme ; le menu et le Bureau sont ensuite mis à jour.
fn save_link(win: &str, link: &LinkData) -> i32 {
    let path = crate::paths::to_linux(win);
    if !path.parent().is_some_and(|p| p.is_dir()) {
        return HR_PATH_NOT_FOUND;
    }
    crate::setup::hide_desktop_link(win);
    if std::fs::write(&path, link_bytes(link)).is_err() {
        return E_FAIL;
    }
    crate::thunks::api_note(&format!(
        "ShellLink : raccourci {win} → {} {} (#1732)",
        link.target, link.args
    ));
    refresh_launchers();
    S_OK
}

/// Demande à winlux-applications de reprendre les raccourcis, sans attendre.
fn refresh_launchers() {
    let tool = std::path::Path::new("/usr/bin/winlux-applications");
    if !tool.is_file() || std::env::var_os("WINLUX_NO_LAUNCHER_REFRESH").is_some() {
        return;
    }
    if let Ok(mut child) = std::process::Command::new(tool)
        .arg("refresh")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

unsafe extern "win64" fn f_save_completed(_this: *mut c_void, _name: *const u16) -> i32 {
    S_OK
}

unsafe extern "win64" fn f_get_cur_file(this: *mut c_void, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let file = with_state(base(this, OFF_FILE), |s| s.file.clone());
    let Some(file) = file else {
        return S_FALSE;
    };
    co_string(&file, out)
}

unsafe fn co_string(s: &str, out: *mut *mut u16) -> i32 {
    let units: Vec<u16> = s.encode_utf16().chain([0]).collect();
    let p = super::ole32::co_task_mem_alloc(units.len() * 2) as *mut u16;
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(units.as_ptr(), p, units.len());
    *out = p;
    S_OK
}

// ---------- IPropertyStore ----------

const VT_LPWSTR: u16 = 31;

unsafe extern "win64" fn p_get_count(this: *mut c_void, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_state(base(this, OFF_PROPS), |s| s.props.len() as u32);
    S_OK
}

unsafe extern "win64" fn p_get_at(this: *mut c_void, i: u32, key: *mut u8) -> i32 {
    if key.is_null() {
        return E_POINTER;
    }
    match with_state(base(this, OFF_PROPS), |s| {
        s.props.get(i as usize).map(|p| p.key)
    }) {
        Some(k) => {
            std::ptr::copy_nonoverlapping(k.as_ptr(), key, 20);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn p_get_value(this: *mut c_void, key: *const u8, out: *mut u8) -> i32 {
    if key.is_null() || out.is_null() {
        return E_POINTER;
    }
    std::ptr::write_bytes(out, 0, 24);
    let mut k = [0u8; 20];
    std::ptr::copy_nonoverlapping(key, k.as_mut_ptr(), 20);
    let found = with_state(base(this, OFF_PROPS), |s| {
        s.props
            .iter()
            .find(|p| p.key == k)
            .map(|p| (p.vt, p.raw, p.text.clone()))
    });
    let Some((vt, raw, text)) = found else {
        return S_OK; // VT_EMPTY
    };
    std::ptr::write_unaligned(out as *mut u16, vt);
    match text {
        Some(t) => {
            let p = super::ole32::co_task_mem_alloc(t.len() * 2) as *mut u16;
            if p.is_null() {
                return E_OUTOFMEMORY;
            }
            std::ptr::copy_nonoverlapping(t.as_ptr(), p, t.len());
            std::ptr::write_unaligned(out.add(8) as *mut usize, p as usize);
        }
        None => std::ptr::copy_nonoverlapping(raw.as_ptr(), out.add(8), 16),
    }
    S_OK
}

unsafe extern "win64" fn p_set_value(this: *mut c_void, key: *const u8, value: *const u8) -> i32 {
    if key.is_null() || value.is_null() {
        return E_POINTER;
    }
    let mut k = [0u8; 20];
    std::ptr::copy_nonoverlapping(key, k.as_mut_ptr(), 20);
    let vt = std::ptr::read_unaligned(value as *const u16);
    let mut raw = [0u8; 16];
    std::ptr::copy_nonoverlapping(value.add(8), raw.as_mut_ptr(), 16);
    let text = if vt == VT_LPWSTR {
        let p = std::ptr::read_unaligned(value.add(8) as *const usize) as *const u16;
        let s = read_w(p).unwrap_or_default();
        Some(s.encode_utf16().chain([0]).collect())
    } else {
        None
    };
    with_state(base(this, OFF_PROPS), |s| {
        s.props.retain(|p| p.key != k);
        s.props.push(Prop {
            key: k,
            vt,
            raw,
            text,
        });
        s.dirty = true;
    });
    S_OK
}

unsafe extern "win64" fn p_commit(_this: *mut c_void) -> i32 {
    S_OK
}

// ---------- Tables ----------

struct Table<const N: usize>([*const (); N]);
// SAFETY : tables de pointeurs de fonctions, en lecture seule.
unsafe impl<const N: usize> Sync for Table<N> {}

static VT_W: Table<21> = Table([
    w_qi as *const (),
    w_add_ref as *const (),
    w_release as *const (),
    w_get_path as *const (),
    w_get_id_list as *const (),
    w_set_id_list as *const (),
    w_get_description as *const (),
    w_set_description as *const (),
    w_get_working_directory as *const (),
    w_set_working_directory as *const (),
    w_get_arguments as *const (),
    w_set_arguments as *const (),
    w_get_hotkey as *const (),
    w_set_hotkey as *const (),
    w_get_show_cmd as *const (),
    w_set_show_cmd as *const (),
    w_get_icon_location as *const (),
    w_set_icon_location as *const (),
    w_set_relative_path as *const (),
    w_resolve as *const (),
    w_set_path as *const (),
]);

static VT_A: Table<21> = Table([
    a_qi as *const (),
    a_add_ref as *const (),
    a_release as *const (),
    a_get_path as *const (),
    a_get_id_list as *const (),
    a_set_id_list as *const (),
    a_get_description as *const (),
    a_set_description as *const (),
    a_get_working_directory as *const (),
    a_set_working_directory as *const (),
    a_get_arguments as *const (),
    a_set_arguments as *const (),
    a_get_hotkey as *const (),
    a_set_hotkey as *const (),
    a_get_show_cmd as *const (),
    a_set_show_cmd as *const (),
    a_get_icon_location as *const (),
    a_set_icon_location as *const (),
    a_set_relative_path as *const (),
    w_resolve as *const (),
    a_set_path as *const (),
]);

static VT_FILE: Table<9> = Table([
    f_qi as *const (),
    f_add_ref as *const (),
    f_release as *const (),
    f_get_class_id as *const (),
    f_is_dirty as *const (),
    f_load as *const (),
    f_save as *const (),
    f_save_completed as *const (),
    f_get_cur_file as *const (),
]);

static VT_PROPS: Table<8> = Table([
    p_qi as *const (),
    p_add_ref as *const (),
    p_release as *const (),
    p_get_count as *const (),
    p_get_at as *const (),
    p_get_value as *const (),
    p_set_value as *const (),
    p_commit as *const (),
]);

#[cfg(test)]
mod tests;
