//! cabinet.dll (décompression FDI) et lz32.dll (fichiers SZDD de
//! COMPRESS.EXE), importées directement par des installeurs et de vieux
//! utilitaires (#26). Sans elles, le programme ne démarrait pas.
//!
//! - FDI : FDICreate, FDIIsCabinet, FDICopy, FDIDestroy. Le cabinet est lu
//!   par les rappels de l'application (pfnopen/pfnread…), comme sous
//!   Windows ; le décodage (stocké, MSZIP, LZX) est celui des paquets .msi
//!   (`msi::cab`). Les cabinets répartis sur plusieurs disques et la
//!   compression Quantum sont refusés avec le code FDI documenté.
//! - LZ : LZOpenFileA/W, LZInit, LZRead, LZSeek, LZCopy, CopyLZFile, LZClose,
//!   GetExpandedNameA/W. Un fichier non compressé est lu tel quel.
//!
//! Formats d'après leur documentation publique (en-têtes mingw-w64 pour
//! les structures).

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

use crate::msi::cab::Cabinet;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match crate::dlls::lookup($dll, $name) {
            Some(addr) => {
                // SAFETY : adresse de la table d'exports de Winlux, signature
                // de l'API Windows documentée.
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

// --- cabinet.dll : FDI ------------------------------------------------------------------

type PfnAlloc = unsafe extern "win64" fn(u32) -> *mut c_void;
type PfnFree = unsafe extern "win64" fn(*mut c_void);
type PfnOpen = unsafe extern "win64" fn(*const u8, i32, i32) -> isize;
type PfnRead = unsafe extern "win64" fn(isize, *mut c_void, u32) -> u32;
type PfnWrite = unsafe extern "win64" fn(isize, *const c_void, u32) -> u32;
type PfnClose = unsafe extern "win64" fn(isize) -> i32;
type PfnSeek = unsafe extern "win64" fn(isize, i32, i32) -> i32;
type PfnNotify = unsafe extern "win64" fn(i32, *mut FdiNotification) -> isize;

/// ERF : erfOper (code FDIERROR_*), erfType, fError.
#[repr(C)]
pub struct Erf {
    oper: i32,
    kind: i32,
    error: i32,
}

/// FDINOTIFICATION (64 octets en 64 bits).
#[repr(C)]
pub struct FdiNotification {
    cb: i32,
    psz1: *const u8,
    psz2: *const u8,
    psz3: *const u8,
    pv: *mut c_void,
    hf: isize,
    date: u16,
    time: u16,
    attribs: u16,
    set_id: u16,
    i_cabinet: u16,
    i_folder: u16,
    fdie: i32,
}

/// FDICABINETINFO.
#[repr(C)]
pub struct FdiCabinetInfo {
    cb_cabinet: i32,
    c_folders: u16,
    c_files: u16,
    set_id: u16,
    i_cabinet: u16,
    reserve: i32,
    has_prev: i32,
    has_next: i32,
}

/// Les tampons de FDI sont des Vec de Winlux : pfnalloc et pfnfree ne
/// servent pas.
struct Fdi {
    open: PfnOpen,
    read: PfnRead,
    write: PfnWrite,
    close: PfnClose,
    erf: *mut Erf,
}

const FDIERROR_CABINET_NOT_FOUND: i32 = 1;
const FDIERROR_NOT_A_CABINET: i32 = 2;
const FDIERROR_CORRUPT_CABINET: i32 = 4;
const FDIERROR_TARGET_FILE: i32 = 8;
const FDIERROR_USER_ABORT: i32 = 11;
/// FDINOTIFICATIONTYPE (fdi.h de mingw-w64) : fdintCABINET_INFO vaut 0,
/// fdintPARTIAL_FILE 1. L'ODT recevait 1 en premier, croyait à un fichier
/// commencé dans un cabinet précédent et abandonnait l'extraction (#1856).
const FDINT_CABINET_INFO: i32 = 0;
const FDINT_COPY_FILE: i32 = 2;
const FDINT_CLOSE_FILE_INFO: i32 = 3;
/// _O_BINARY | _O_RDONLY | _O_SEQUENTIAL.
const OPEN_READ: i32 = 0x8000 | 0x0020;

unsafe fn set_erf(erf: *mut Erf, oper: i32) {
    if !erf.is_null() {
        (*erf).oper = oper;
        (*erf).kind = 0;
        (*erf).error = i32::from(oper != 0);
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn fdi_create(
    alloc: Option<PfnAlloc>,
    free: Option<PfnFree>,
    open: Option<PfnOpen>,
    read: Option<PfnRead>,
    write: Option<PfnWrite>,
    close: Option<PfnClose>,
    _seek: Option<PfnSeek>,
    _cpu: i32,
    erf: *mut Erf,
) -> usize {
    let (Some(alloc), Some(free), Some(open), Some(read), Some(write), Some(close)) =
        (alloc, free, open, read, write, close)
    else {
        set_erf(erf, 5); // FDIERROR_ALLOC_FAIL
        return 0;
    };
    set_erf(erf, 0);
    let _ = (alloc, free);
    Box::into_raw(Box::new(Fdi {
        open,
        read,
        write,
        close,
        erf,
    })) as usize
}

unsafe extern "win64" fn fdi_destroy(h: usize) -> i32 {
    if h == 0 {
        return 0;
    }
    drop(Box::from_raw(h as *mut Fdi));
    1
}

/// Lit tout ce que rend `hf` par le rappel de l'application.
unsafe fn read_all(fdi: &Fdi, hf: isize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = (fdi.read)(hf, buf.as_mut_ptr().cast(), buf.len() as u32);
        if n == 0 || n == u32::MAX {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    out
}

fn header_info(d: &[u8]) -> Option<FdiCabinetInfo> {
    if d.get(..4) != Some(b"MSCF") {
        return None;
    }
    let u16_at = |o: usize| Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?));
    let flags = u16_at(30)?;
    Some(FdiCabinetInfo {
        cb_cabinet: i32::from_le_bytes(d.get(8..12)?.try_into().ok()?),
        c_folders: u16_at(26)?,
        c_files: u16_at(28)?,
        set_id: u16_at(32)?,
        i_cabinet: u16_at(34)?,
        reserve: i32::from(flags & 4 != 0),
        has_prev: i32::from(flags & 1 != 0),
        has_next: i32::from(flags & 2 != 0),
    })
}

unsafe extern "win64" fn fdi_is_cabinet(h: usize, hf: isize, info: *mut FdiCabinetInfo) -> i32 {
    if h == 0 {
        return 0;
    }
    let fdi = &*(h as *const Fdi);
    let mut head = [0u8; 36];
    let n = (fdi.read)(hf, head.as_mut_ptr().cast(), 36);
    match (n == 36).then(|| header_info(&head)).flatten() {
        Some(i) => {
            if !info.is_null() {
                *info = i;
            }
            set_erf(fdi.erf, 0);
            1
        }
        None => {
            set_erf(fdi.erf, FDIERROR_NOT_A_CABINET);
            0
        }
    }
}

/// Appelle le rappel de FDICopy avec la FDINOTIFICATION de l'appelant :
/// pour un programme x86, la disposition 32 bits (40 octets ; cb 0, psz1 4,
/// psz2 8, psz3 12, pv 16, hf 20, date 24, time 26, attribs 28, setID 30,
/// iCabinet 32, iFolder 34, fdie 36), d'après fdi.h de mingw-w64.
unsafe fn call_notify(notify: PfnNotify, kind: i32, n: &mut FdiNotification) -> isize {
    if crate::wow32::pointer_size() != 4 {
        return notify(kind, n);
    }
    let mut b = x86_notification(n);
    notify(kind, b.as_mut_ptr().cast())
}

/// FDINOTIFICATION à la disposition x86.
fn x86_notification(n: &FdiNotification) -> [u8; 40] {
    let mut b = [0u8; 40];
    let mut put = |at: usize, v: &[u8]| b[at..at + v.len()].copy_from_slice(v);
    put(0, &n.cb.to_le_bytes());
    put(4, &(n.psz1 as usize as u32).to_le_bytes());
    put(8, &(n.psz2 as usize as u32).to_le_bytes());
    put(12, &(n.psz3 as usize as u32).to_le_bytes());
    put(16, &(n.pv as usize as u32).to_le_bytes());
    put(20, &(n.hf as u32).to_le_bytes());
    put(24, &n.date.to_le_bytes());
    put(26, &n.time.to_le_bytes());
    put(28, &n.attribs.to_le_bytes());
    put(30, &n.set_id.to_le_bytes());
    put(32, &n.i_cabinet.to_le_bytes());
    put(34, &n.i_folder.to_le_bytes());
    put(36, &n.fdie.to_le_bytes());
    b
}

unsafe extern "win64" fn fdi_copy(
    h: usize,
    cabinet: *const u8,
    path: *const u8,
    _flags: i32,
    notify: Option<PfnNotify>,
    _decrypt: usize,
    pv: *mut c_void,
) -> i32 {
    if h == 0 {
        return 0;
    }
    let fdi = &*(h as *const Fdi);
    let Some(notify) = notify else {
        set_erf(fdi.erf, FDIERROR_USER_ABORT);
        return 0;
    };
    // Chemin complet : répertoire (avec sa barre finale) puis nom.
    let mut full: Vec<u8> = Vec::new();
    if !path.is_null() {
        full.extend_from_slice(std::ffi::CStr::from_ptr(path.cast()).to_bytes());
    }
    if !cabinet.is_null() {
        full.extend_from_slice(std::ffi::CStr::from_ptr(cabinet.cast()).to_bytes());
    }
    full.push(0);
    let hf = (fdi.open)(full.as_ptr(), OPEN_READ, 0);
    if hf == -1 {
        set_erf(fdi.erf, FDIERROR_CABINET_NOT_FOUND);
        return 0;
    }
    let data = read_all(fdi, hf);
    (fdi.close)(hf);
    let Some(info) = header_info(&data) else {
        set_erf(fdi.erf, FDIERROR_NOT_A_CABINET);
        return 0;
    };
    let cab = match Cabinet::parse(&data) {
        Ok(c) => c,
        Err(e) => {
            crate::trace!("FDICopy : {e}");
            set_erf(fdi.erf, FDIERROR_CORRUPT_CABINET);
            return 0;
        }
    };
    // Sur la pile (sous 4 Go pour un programme x86), pas dans l'image.
    let empty = [0u8; 1];
    let mut n: FdiNotification = std::mem::zeroed();
    n.psz1 = empty.as_ptr();
    n.psz2 = empty.as_ptr();
    n.psz3 = if path.is_null() { empty.as_ptr() } else { path };
    n.pv = pv;
    n.set_id = info.set_id;
    n.i_cabinet = info.i_cabinet;
    if call_notify(notify, FDINT_CABINET_INFO, &mut n) == -1 {
        set_erf(fdi.erf, FDIERROR_USER_ABORT);
        return 0;
    }
    let meta: HashMap<String, (u16, u16, u16, usize)> = cab
        .files
        .iter()
        .map(|f| (f.name.clone(), (f.date, f.time, f.attribs, f.size)))
        .collect();
    let mut failure = 0;
    let result = cab.extract(&mut |name, bytes| {
        let (date, time, attribs, _) = meta.get(name).copied().unwrap_or_default();
        let mut raw = crate::dlls::encode_ansi(name);
        raw.push(0);
        let mut n: FdiNotification = std::mem::zeroed();
        n.cb = bytes.len() as i32;
        n.psz1 = raw.as_ptr();
        n.pv = pv;
        n.date = date;
        n.time = time;
        n.attribs = attribs;
        n.set_id = info.set_id;
        n.i_cabinet = info.i_cabinet;
        let out = call_notify(notify, FDINT_COPY_FILE, &mut n);
        if out == -1 {
            failure = FDIERROR_USER_ABORT;
            return Err("abandon".into());
        }
        if out == 0 {
            return Ok(()); // fichier ignoré par l'application
        }
        for chunk in bytes.chunks(32 * 1024) {
            if (fdi.write)(out, chunk.as_ptr().cast(), chunk.len() as u32) != chunk.len() as u32 {
                failure = FDIERROR_TARGET_FILE;
                return Err("écriture".into());
            }
        }
        n.hf = out;
        n.cb = 0;
        if call_notify(notify, FDINT_CLOSE_FILE_INFO, &mut n) == 0 {
            failure = FDIERROR_TARGET_FILE;
            return Err("fermeture".into());
        }
        Ok(())
    });
    match result {
        Ok(()) => {
            set_erf(fdi.erf, 0);
            1
        }
        Err(e) => {
            crate::trace!("FDICopy : {e}");
            set_erf(
                fdi.erf,
                if failure != 0 {
                    failure
                } else {
                    FDIERROR_CORRUPT_CABINET
                },
            );
            0
        }
    }
}

pub fn cabinet_exports() -> crate::dlls::Exports {
    let mut e = super::cabinet_fci::exports();
    e.extend(crate::exports! {
        "FDICreate" => fn fdi_create,
        "FDIIsCabinet" => fn fdi_is_cabinet,
        "FDICopy" => fn fdi_copy,
        "FDIDestroy" => fn fdi_destroy,
    });
    e
}

// --- lz32.dll : fichiers SZDD ----------------------------------------------------------------

const LZERROR_BADINHANDLE: i32 = -1;
const LZERROR_BADOUTHANDLE: i32 = -2;
const LZERROR_READ: i32 = -3;
const LZERROR_WRITE: i32 = -4;
const LZERROR_BADVALUE: i32 = -7;
const LZERROR_UNKNOWNALG: i32 = -8;
/// Les handles LZ commencent à LZ_MIN_HANDLE, comme sous Windows.
const LZ_MIN_HANDLE: i32 = 0x400;
const SZDD_MAGIC: &[u8; 8] = b"SZDD\x88\xf0\x27\x33";

struct LzFile {
    data: Vec<u8>,
    pos: usize,
    /// Handle de fichier d'origine, fermé par LZClose.
    source: usize,
}

static LZ: Mutex<Option<HashMap<i32, LzFile>>> = Mutex::new(None);

fn lz() -> std::sync::MutexGuard<'static, Option<HashMap<i32, LzFile>>> {
    LZ.lock().unwrap_or_else(|e| e.into_inner())
}

/// Décompresse un fichier SZDD (LZSS, fenêtre de 4 Kio remplie d'espaces).
pub fn szdd_expand(d: &[u8]) -> Result<Vec<u8>, i32> {
    if d.get(..8) != Some(SZDD_MAGIC) {
        return Err(LZERROR_BADVALUE);
    }
    if d.get(8) != Some(&b'A') {
        return Err(LZERROR_UNKNOWNALG);
    }
    let size = u32::from_le_bytes(d.get(10..14).ok_or(LZERROR_READ)?.try_into().unwrap()) as usize;
    let mut window = [b' '; 4096];
    let mut wpos = 4096 - 16;
    let mut out = Vec::with_capacity(size);
    let mut i = 14;
    while i < d.len() && out.len() < size {
        let flags = d[i];
        i += 1;
        for bit in 0..8 {
            if i >= d.len() || out.len() >= size {
                break;
            }
            if flags & (1 << bit) != 0 {
                let c = d[i];
                i += 1;
                out.push(c);
                window[wpos] = c;
                wpos = (wpos + 1) & 4095;
            } else {
                let (Some(&a), Some(&b)) = (d.get(i), d.get(i + 1)) else {
                    break;
                };
                i += 2;
                let mut from = a as usize | ((b as usize & 0xF0) << 4);
                let len = (b as usize & 0x0F) + 3;
                for _ in 0..len {
                    let c = window[from];
                    from = (from + 1) & 4095;
                    out.push(c);
                    window[wpos] = c;
                    wpos = (wpos + 1) & 4095;
                }
            }
        }
    }
    out.truncate(size);
    Ok(out)
}

/// Nom développé : la lettre gardée dans l'en-tête remplace le « _ » final.
fn expanded_name(name: &str, header: &[u8]) -> String {
    if header.get(..8) != Some(SZDD_MAGIC) {
        return name.to_string();
    }
    let missing = header.get(9).copied().unwrap_or(0);
    let mut s = name.to_string();
    if s.ends_with('_') {
        s.pop();
        if missing != 0 {
            let c = missing as char;
            let upper = s
                .rsplit(['\\', '/'])
                .next()
                .is_some_and(|b| b == b.to_uppercase());
            s.push(if upper {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            });
        }
    }
    s
}

unsafe fn read_handle(h: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut n = 0u32;
        let ok = api!(
            "kernel32",
            "ReadFile",
            fn(usize, *mut u8, u32, *mut u32, usize) -> i32,
            h,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut n,
            0
        )?;
        if ok == 0 {
            return None;
        }
        if n == 0 {
            return Some(out);
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}

unsafe fn write_handle(h: usize, data: &[u8]) -> bool {
    let mut n = 0u32;
    api!(
        "kernel32",
        "WriteFile",
        fn(usize, *const u8, u32, *mut u32, usize) -> i32,
        h,
        data.as_ptr(),
        data.len() as u32,
        &mut n,
        0
    )
    .is_some_and(|ok| ok != 0 && n as usize == data.len())
}

unsafe fn close_handle(h: usize) {
    api!("kernel32", "CloseHandle", fn(usize) -> i32, h);
}

/// LZInit : lit tout le fichier source, décompressé s'il est au format SZDD.
unsafe extern "win64" fn lz_init(source: i32) -> i32 {
    let h = source as u32 as usize;
    let Some(raw) = read_handle(h) else {
        return LZERROR_BADINHANDLE;
    };
    let data = if raw.get(..8) == Some(SZDD_MAGIC) {
        match szdd_expand(&raw) {
            Ok(d) => d,
            Err(e) => return e,
        }
    } else {
        raw
    };
    let mut g = lz();
    let map = g.get_or_insert_with(HashMap::new);
    let id = (LZ_MIN_HANDLE..LZ_MIN_HANDLE + 0x400)
        .find(|i| !map.contains_key(i))
        .unwrap_or(LZ_MIN_HANDLE);
    map.insert(
        id,
        LzFile {
            data,
            pos: 0,
            source: h,
        },
    );
    id
}

unsafe fn open_file(path: &str, ofs: *mut u8, style: u16) -> i32 {
    const OF_CREATE: u16 = 0x1000;
    const OF_WRITE: u16 = 0x0001;
    const OF_READWRITE: u16 = 0x0002;
    let write = style & (OF_CREATE | OF_WRITE | OF_READWRITE) != 0;
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let h = api!(
        "kernel32",
        "CreateFileW",
        fn(*const u16, u32, u32, usize, u32, u32, usize) -> usize,
        wide.as_ptr(),
        if write { 0x4000_0000 } else { 0x8000_0000 },
        1,
        0,
        if style & OF_CREATE != 0 { 2 } else { 3 },
        0x80,
        0
    )
    .unwrap_or(usize::MAX);
    if h == usize::MAX {
        return LZERROR_BADINHANDLE;
    }
    // OFSTRUCT : cBytes, fFixedDisk, nErrCode, Reserved1/2, szPathName[128].
    if !ofs.is_null() {
        std::ptr::write_bytes(ofs, 0, 136);
        *ofs = 136;
        let name = crate::dlls::encode_ansi(path);
        let n = name.len().min(127);
        std::ptr::copy_nonoverlapping(name.as_ptr(), ofs.add(8), n);
    }
    if write {
        return h as i32;
    }
    lz_init(h as i32)
}

unsafe extern "win64" fn lz_open_file_a(name: *const u8, ofs: *mut u8, style: u16) -> i32 {
    open_file(&crate::dlls::cstr(name), ofs, style)
}

unsafe extern "win64" fn lz_open_file_w(name: *const u16, ofs: *mut u8, style: u16) -> i32 {
    open_file(&crate::dlls::wstr(name), ofs, style)
}

unsafe extern "win64" fn lz_read(h: i32, buf: *mut u8, count: i32) -> i32 {
    if count < 0 || buf.is_null() {
        return LZERROR_BADVALUE;
    }
    let mut g = lz();
    let Some(f) = g.as_mut().and_then(|m| m.get_mut(&h)) else {
        return LZERROR_BADINHANDLE;
    };
    let n = (count as usize).min(f.data.len().saturating_sub(f.pos));
    std::ptr::copy_nonoverlapping(f.data.as_ptr().add(f.pos), buf, n);
    f.pos += n;
    n as i32
}

unsafe extern "win64" fn lz_seek(h: i32, offset: i32, origin: i32) -> i32 {
    let mut g = lz();
    let Some(f) = g.as_mut().and_then(|m| m.get_mut(&h)) else {
        return LZERROR_BADINHANDLE;
    };
    let base = match origin {
        0 => 0i64,
        1 => f.pos as i64,
        2 => f.data.len() as i64,
        _ => return LZERROR_BADVALUE,
    };
    let to = base + offset as i64;
    if to < 0 || to > f.data.len() as i64 {
        return LZERROR_BADVALUE;
    }
    f.pos = to as usize;
    to as i32
}

unsafe extern "win64" fn lz_close(h: i32) {
    let removed = lz().as_mut().and_then(|m| m.remove(&h));
    match removed {
        Some(f) => close_handle(f.source),
        None => close_handle(h as u32 as usize),
    }
}

unsafe extern "win64" fn lz_copy(src: i32, dst: i32) -> i32 {
    let data = {
        let g = lz();
        g.as_ref().and_then(|m| m.get(&src)).map(|f| f.data.clone())
    };
    let data = match data {
        Some(d) => d,
        None => match read_handle(src as u32 as usize) {
            Some(raw) if raw.get(..8) == Some(SZDD_MAGIC) => match szdd_expand(&raw) {
                Ok(d) => d,
                Err(e) => return e,
            },
            Some(raw) => raw,
            None => return LZERROR_READ,
        },
    };
    if dst <= 0 {
        return LZERROR_BADOUTHANDLE;
    }
    if !write_handle(dst as u32 as usize, &data) {
        return LZERROR_WRITE;
    }
    data.len() as i32
}

unsafe extern "win64" fn lz_start() -> i32 {
    1
}

unsafe extern "win64" fn lz_done() {}

unsafe fn expanded_name_of(path: &str) -> String {
    let header = std::fs::File::open(crate::paths::to_linux(path))
        .ok()
        .and_then(|mut f| {
            use std::io::Read;
            let mut h = [0u8; 14];
            f.read_exact(&mut h).ok().map(|()| h)
        });
    expanded_name(path, header.as_ref().map_or(&[][..], |h| &h[..]))
}

unsafe extern "win64" fn get_expanded_name_a(src: *const u8, dst: *mut u8) -> i32 {
    if src.is_null() || dst.is_null() {
        return LZERROR_BADVALUE;
    }
    let name = expanded_name_of(&crate::dlls::cstr(src));
    let raw = crate::dlls::encode_ansi(&name);
    std::ptr::copy_nonoverlapping(raw.as_ptr(), dst, raw.len());
    *dst.add(raw.len()) = 0;
    1
}

unsafe extern "win64" fn get_expanded_name_w(src: *const u16, dst: *mut u16) -> i32 {
    if src.is_null() || dst.is_null() {
        return LZERROR_BADVALUE;
    }
    let name = expanded_name_of(&crate::dlls::wstr(src));
    let w: Vec<u16> = name.encode_utf16().collect();
    std::ptr::copy_nonoverlapping(w.as_ptr(), dst, w.len());
    *dst.add(w.len()) = 0;
    1
}

pub fn lz32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "LZOpenFileA" => fn lz_open_file_a,
        "LZOpenFileW" => fn lz_open_file_w,
        "LZInit" => fn lz_init,
        "LZRead" => fn lz_read,
        "LZSeek" => fn lz_seek,
        "LZCopy" => fn lz_copy,
        "CopyLZFile" => fn lz_copy,
        "LZClose" => fn lz_close,
        "LZStart" => fn lz_start,
        "LZDone" => fn lz_done,
        "GetExpandedNameA" => fn get_expanded_name_a,
        "GetExpandedNameW" => fn get_expanded_name_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FDINOTIFICATION x86 : 40 octets aux décalages de fdi.h (#1758).
    #[test]
    fn notification_x86() {
        let mut n: FdiNotification = unsafe { std::mem::zeroed() };
        n.cb = 7;
        n.psz1 = 0x1234_5678 as *const u8;
        n.pv = 0x0040_0000 as *mut c_void;
        n.hf = 9;
        n.date = 0x1111;
        n.attribs = 0x20;
        n.i_folder = 3;
        n.fdie = 5;
        let b = x86_notification(&n);
        assert_eq!(&b[0..4], &7i32.to_le_bytes());
        assert_eq!(&b[4..8], &0x1234_5678u32.to_le_bytes());
        assert_eq!(&b[16..20], &0x0040_0000u32.to_le_bytes());
        assert_eq!(&b[20..24], &9u32.to_le_bytes());
        assert_eq!(&b[24..26], &0x1111u16.to_le_bytes());
        assert_eq!(&b[28..30], &0x20u16.to_le_bytes());
        assert_eq!(&b[34..36], &3u16.to_le_bytes());
        assert_eq!(&b[36..40], &5i32.to_le_bytes());
    }

    /// Fichier SZDD construit à la main : « ABCABCABC » = 3 littéraux puis
    /// une référence de 6 octets vers ce qu'on vient d'écrire.
    #[test]
    fn szdd_decompresse_litteraux_et_references() {
        let mut f = SZDD_MAGIC.to_vec();
        f.extend_from_slice(b"At");
        f.extend_from_slice(&9u32.to_le_bytes());
        // Drapeaux : bits 0..2 littéraux, bit 3 référence.
        f.push(0b0000_0111);
        f.extend_from_slice(b"ABC");
        // Position 4096-16 = 0xFF0, longueur 6 → (6-3)=3.
        f.push(0xF0);
        f.push(0xF3);
        assert_eq!(szdd_expand(&f).unwrap(), b"ABCABCABC");
        assert_eq!(expanded_name("SETUP.EX_", &f), "SETUP.EXT");
        assert_eq!(expanded_name("lisez.tx_", &f), "lisez.txt");
        assert_eq!(expanded_name("plain.txt", b"rien"), "plain.txt");
    }

    #[test]
    fn fdi_copy_annonce_le_cabinet_puis_les_fichiers() {
        use std::cell::RefCell;
        thread_local! {
            static CAB: RefCell<(Vec<u8>, usize)> = const { RefCell::new((Vec::new(), 0)) };
            static KINDS: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
            static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        }
        unsafe extern "win64" fn alloc(n: u32) -> *mut c_void {
            libc::malloc(n as usize)
        }
        unsafe extern "win64" fn free(p: *mut c_void) {
            libc::free(p)
        }
        unsafe extern "win64" fn open(_p: *const u8, _f: i32, _m: i32) -> isize {
            CAB.with(|c| c.borrow_mut().1 = 0);
            1
        }
        unsafe extern "win64" fn read(_h: isize, buf: *mut c_void, n: u32) -> u32 {
            CAB.with(|c| {
                let mut c = c.borrow_mut();
                let at = c.1;
                let k = (n as usize).min(c.0.len() - at);
                std::ptr::copy_nonoverlapping(c.0[at..].as_ptr(), buf.cast(), k);
                c.1 += k;
                k as u32
            })
        }
        unsafe extern "win64" fn write(_h: isize, buf: *const c_void, n: u32) -> u32 {
            let b = std::slice::from_raw_parts(buf.cast::<u8>(), n as usize);
            OUT.with(|o| o.borrow_mut().extend_from_slice(b));
            n
        }
        unsafe extern "win64" fn close(_h: isize) -> i32 {
            0
        }
        unsafe extern "win64" fn seek(_h: isize, _d: i32, _t: i32) -> i32 {
            0
        }
        unsafe extern "win64" fn notify(kind: i32, _n: *mut FdiNotification) -> isize {
            KINDS.with(|k| k.borrow_mut().push(kind));
            match kind {
                FDINT_COPY_FILE => 7,
                _ => 1,
            }
        }
        let cab = crate::msi::cab::tests::build_mszip(&[("a.txt", b"bonjour")]);
        CAB.with(|c| *c.borrow_mut() = (cab, 0));
        let mut erf = Erf {
            oper: 0,
            kind: 0,
            error: 0,
        };
        unsafe {
            let h = fdi_create(
                Some(alloc),
                Some(free),
                Some(open),
                Some(read),
                Some(write),
                Some(close),
                Some(seek),
                1,
                &mut erf,
            );
            assert_ne!(h, 0);
            let name = b"a.cab\0";
            let ok = fdi_copy(
                h,
                name.as_ptr(),
                std::ptr::null(),
                0,
                Some(notify),
                0,
                std::ptr::null_mut(),
            );
            fdi_destroy(h);
            assert_eq!(ok, 1);
        }
        // fdintCABINET_INFO (0), fdintCOPY_FILE (2), fdintCLOSE_FILE_INFO (3).
        assert_eq!(KINDS.with(|k| k.borrow().clone()), vec![0, 2, 3]);
        assert_eq!(OUT.with(|o| o.borrow().clone()), b"bonjour");
    }

    #[test]
    fn fdi_lit_l_en_tete_du_cabinet() {
        let cab = crate::msi::cab::tests::build_mszip(&[("a.txt", b"bonjour")]);
        let info = header_info(&cab).unwrap();
        assert_eq!(info.c_files, 1);
        assert_eq!(info.c_folders, 1);
        assert_eq!(info.cb_cabinet as usize, cab.len());
        assert!(header_info(b"pas un cabinet").is_none());
    }
}
