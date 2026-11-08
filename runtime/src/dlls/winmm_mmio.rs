//! Entrées-sorties multimédia de winmm : mmioOpen, mmioRead, mmioWrite,
//! mmioSeek, mmioDescend, mmioAscend, mmioCreateChunk… (#1221).
//!
//! Elles lisent et écrivent les fichiers RIFF (WAV, AVI) par blocs
//! (« chunks »). System.Media.SoundPlayer de .NET les appelle pour vérifier
//! un WAV avant de le jouer ; des jeux et des lecteurs les emploient aussi.
//! Fichiers disque et fichiers en mémoire (FOURCC_MEM) sont servis, sans
//! tampon d'E/S : mmioGetInfo rend un MMIOINFO sans tampon et mmioAdvance
//! répond MMIOERR_UNBUFFERED, comme pour un fichier ouvert sans
//! MMIO_ALLOCBUF. Constantes et dispositions : mmsystem.h / mmiscapi.h de
//! MinGW-w64 et documentation Microsoft Learn.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Mutex;

const MMSYSERR_NOERROR: u32 = 0;
const MMSYSERR_INVALHANDLE: u32 = 5;
const MMSYSERR_INVALPARAM: u32 = 11;
const MMIOERR_FILENOTFOUND: u32 = 257;
const MMIOERR_CANNOTOPEN: u32 = 259;
const MMIOERR_CANNOTWRITE: u32 = 262;
const MMIOERR_CANNOTSEEK: u32 = 263;
const MMIOERR_CHUNKNOTFOUND: u32 = 265;
const MMIOERR_UNBUFFERED: u32 = 266;
const MMIOERR_ACCESSDENIED: u32 = 268;

const MMIO_WRITE: u32 = 0x1;
const MMIO_READWRITE: u32 = 0x2;
const MMIO_PARSE: u32 = 0x100;
const MMIO_DELETE: u32 = 0x200;
const MMIO_CREATE: u32 = 0x1000;
const MMIO_EXIST: u32 = 0x4000;
const MMIO_TOUPPER: u32 = 0x10;
const MMIO_FINDCHUNK: u32 = 0x10;
const MMIO_FINDRIFF: u32 = 0x20;
const MMIO_FINDLIST: u32 = 0x40;
const MMIO_CREATERIFF: u32 = 0x20;
const MMIO_CREATELIST: u32 = 0x40;
const MMIO_DIRTY: u32 = 0x1000_0000;

pub const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*s)
}
const FOURCC_RIFF: u32 = fourcc(b"RIFF");
const FOURCC_LIST: u32 = fourcc(b"LIST");
const FOURCC_MEM: u32 = fourcc(b"MEM ");

enum Backing {
    File(std::fs::File),
    /// Tampon du programme (FOURCC_MEM) : adresse, longueur, position.
    Mem {
        base: usize,
        len: usize,
        pos: usize,
    },
}

struct Mmio {
    io: Backing,
    writable: bool,
}

impl Mmio {
    fn seek(&mut self, to: SeekFrom) -> Option<u64> {
        match &mut self.io {
            Backing::File(f) => f.seek(to).ok(),
            Backing::Mem { len, pos, .. } => {
                let target = match to {
                    SeekFrom::Start(n) => n as i64,
                    SeekFrom::Current(d) => *pos as i64 + d,
                    SeekFrom::End(d) => *len as i64 + d,
                };
                if target < 0 || target as usize > *len {
                    return None;
                }
                *pos = target as usize;
                Some(target as u64)
            }
        }
    }

    fn pos(&mut self) -> u64 {
        self.seek(SeekFrom::Current(0)).unwrap_or(0)
    }

    fn read(&mut self, out: &mut [u8]) -> Option<usize> {
        match &mut self.io {
            Backing::File(f) => {
                let mut done = 0;
                while done < out.len() {
                    match f.read(&mut out[done..]) {
                        Ok(0) => break,
                        Ok(n) => done += n,
                        Err(_) => return None,
                    }
                }
                Some(done)
            }
            Backing::Mem { base, len, pos } => {
                let n = out.len().min(*len - *pos);
                // SAFETY : tampon déclaré par le programme à mmioOpen.
                unsafe {
                    std::ptr::copy_nonoverlapping((*base + *pos) as *const u8, out.as_mut_ptr(), n)
                };
                *pos += n;
                Some(n)
            }
        }
    }

    fn write(&mut self, data: &[u8]) -> Option<usize> {
        if !self.writable {
            return None;
        }
        match &mut self.io {
            Backing::File(f) => f.write_all(data).ok().map(|_| data.len()),
            Backing::Mem { base, len, pos } => {
                let n = data.len().min(*len - *pos);
                // SAFETY : tampon déclaré par le programme à mmioOpen.
                unsafe {
                    std::ptr::copy_nonoverlapping(data.as_ptr(), (*base + *pos) as *mut u8, n)
                };
                *pos += n;
                (n == data.len()).then_some(n)
            }
        }
    }

    fn read_u32(&mut self) -> Option<u32> {
        let mut b = [0u8; 4];
        (self.read(&mut b)? == 4).then(|| u32::from_le_bytes(b))
    }
}

static FILES: Mutex<Option<HashMap<usize, Mmio>>> = Mutex::new(None);
static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x4D4D_0010);

fn with<R>(h: usize, f: impl FnOnce(&mut Mmio) -> R) -> Option<R> {
    let mut g = FILES.lock().unwrap_or_else(|e| e.into_inner());
    g.as_mut()?.get_mut(&h).map(f)
}

fn wow32() -> bool {
    crate::wow32::active()
}

/// MMIOINFO : (wErrorRet, fccIOProc, cchBuffer, pchBuffer, hmmio).
/// x64 : 16, 4, 32, 40, 104 ; x86 : 12, 4, 20, 24, 68.
fn info_offsets() -> (usize, usize, usize, usize, usize) {
    if wow32() {
        (12, 4, 20, 24, 68)
    } else {
        (16, 4, 32, 40, 104)
    }
}

unsafe fn ptr_at(p: *const u8, off: usize) -> usize {
    if wow32() {
        (p.add(off) as *const u32).read_unaligned() as usize
    } else {
        (p.add(off) as *const usize).read_unaligned()
    }
}

unsafe fn set_error(info: *mut u8, code: u32) {
    if !info.is_null() {
        (info.add(info_offsets().0) as *mut u32).write_unaligned(code);
    }
}

fn error_of(e: &std::io::Error) -> u32 {
    match e.kind() {
        std::io::ErrorKind::NotFound => MMIOERR_FILENOTFOUND,
        std::io::ErrorKind::PermissionDenied => MMIOERR_ACCESSDENIED,
        _ => MMIOERR_CANNOTOPEN,
    }
}

unsafe fn open(name: Option<String>, info: *mut u8, flags: u32) -> usize {
    let (_, fcc_off, cch_off, buf_off, h_off) = info_offsets();
    if !info.is_null() {
        set_error(info, 0);
        let fcc = (info.add(fcc_off) as *const u32).read_unaligned();
        if fcc == FOURCC_MEM {
            let len = (info.add(cch_off) as *const i32).read_unaligned();
            let base = ptr_at(info, buf_off);
            if base == 0 || len <= 0 {
                set_error(info, MMSYSERR_INVALPARAM);
                return 0;
            }
            let h = NEXT.fetch_add(4, std::sync::atomic::Ordering::Relaxed);
            FILES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(HashMap::new)
                .insert(
                    h,
                    Mmio {
                        io: Backing::Mem {
                            base,
                            len: len as usize,
                            pos: 0,
                        },
                        writable: flags & (MMIO_WRITE | MMIO_READWRITE) != 0,
                    },
                );
            return h;
        }
    }
    let Some(name) = name else {
        set_error(info, MMIOERR_FILENOTFOUND);
        return 0;
    };
    let path = crate::paths::to_linux(&name);
    if flags & MMIO_PARSE != 0 {
        // Nom complet rendu dans le tampon du programme : non servi ici,
        // l'appel réussit sans ouvrir de fichier (comme un nom déjà complet).
        return 1;
    }
    if flags & MMIO_DELETE != 0 {
        return match std::fs::remove_file(&path) {
            Ok(()) => 1,
            Err(e) => {
                set_error(info, error_of(&e));
                0
            }
        };
    }
    if flags & MMIO_EXIST != 0 {
        if path.exists() {
            return 1;
        }
        set_error(info, MMIOERR_FILENOTFOUND);
        return 0;
    }
    let write = flags & (MMIO_WRITE | MMIO_READWRITE) != 0;
    let mut opts = std::fs::OpenOptions::new();
    opts.read(flags & MMIO_WRITE == 0 || flags & MMIO_READWRITE != 0)
        .write(write);
    if flags & MMIO_CREATE != 0 {
        opts.create(true).truncate(true).write(true);
    }
    match opts.open(&path) {
        Ok(f) => {
            let h = NEXT.fetch_add(4, std::sync::atomic::Ordering::Relaxed);
            FILES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(HashMap::new)
                .insert(
                    h,
                    Mmio {
                        io: Backing::File(f),
                        writable: write || flags & MMIO_CREATE != 0,
                    },
                );
            if !info.is_null() {
                if wow32() {
                    (info.add(h_off) as *mut u32).write_unaligned(h as u32);
                } else {
                    (info.add(h_off) as *mut usize).write_unaligned(h);
                }
            }
            h
        }
        Err(e) => {
            crate::trace!("mmioOpen {name} : {e}");
            set_error(info, error_of(&e));
            0
        }
    }
}

pub unsafe extern "win64" fn mmio_open_w(name: *const u16, info: *mut u8, flags: u32) -> usize {
    let name = (!name.is_null()).then(|| crate::dlls::wstr(name));
    open(name, info, flags)
}

pub unsafe extern "win64" fn mmio_open_a(name: *const u8, info: *mut u8, flags: u32) -> usize {
    let name = (!name.is_null()).then(|| crate::dlls::cstr(name));
    open(name, info, flags)
}

pub extern "win64" fn mmio_close(h: usize, _flags: u32) -> u32 {
    let mut g = FILES.lock().unwrap_or_else(|e| e.into_inner());
    match g.as_mut().and_then(|m| m.remove(&h)) {
        Some(_) => MMSYSERR_NOERROR,
        None => MMSYSERR_INVALHANDLE,
    }
}

pub unsafe extern "win64" fn mmio_read(h: usize, buf: *mut u8, len: i32) -> i32 {
    if len < 0 || (buf.is_null() && len > 0) {
        return -1;
    }
    let out = std::slice::from_raw_parts_mut(buf, len as usize);
    with(h, |m| m.read(out).map_or(-1, |n| n as i32)).unwrap_or(-1)
}

pub unsafe extern "win64" fn mmio_write(h: usize, buf: *const u8, len: i32) -> i32 {
    if len < 0 || (buf.is_null() && len > 0) {
        return -1;
    }
    let data = std::slice::from_raw_parts(buf, len as usize);
    with(h, |m| m.write(data).map_or(-1, |n| n as i32)).unwrap_or(-1)
}

pub extern "win64" fn mmio_seek(h: usize, offset: i32, origin: i32) -> i32 {
    let to = match origin {
        0 if offset >= 0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset as i64),
        2 => SeekFrom::End(offset as i64),
        _ => return -1,
    };
    with(h, |m| m.seek(to).map_or(-1, |p| p as i32)).unwrap_or(-1)
}

/// MMCKINFO : ckid, cksize, fccType, dwDataOffset, dwFlags (20 octets).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CkInfo {
    ckid: u32,
    cksize: u32,
    fcc_type: u32,
    data_offset: u32,
    flags: u32,
}

unsafe fn ck_read(p: *const u8) -> CkInfo {
    (p as *const CkInfo).read_unaligned()
}

unsafe fn ck_write(p: *mut u8, c: CkInfo) {
    (p as *mut CkInfo).write_unaligned(c)
}

pub unsafe extern "win64" fn mmio_descend(
    h: usize,
    ck: *mut u8,
    parent: *const u8,
    flags: u32,
) -> u32 {
    if ck.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let want = ck_read(ck);
    let end = (!parent.is_null()).then(|| {
        let p = ck_read(parent);
        p.data_offset as u64 + p.cksize as u64
    });
    let found = with(h, |m| loop {
        let start = m.pos();
        if end.is_some_and(|e| start + 8 > e) {
            return Err(MMIOERR_CHUNKNOTFOUND);
        }
        let (Some(id), Some(size)) = (m.read_u32(), m.read_u32()) else {
            return Err(MMIOERR_CHUNKNOTFOUND);
        };
        let data = start + 8;
        let mut c = CkInfo {
            ckid: id,
            cksize: size,
            fcc_type: 0,
            data_offset: data as u32,
            flags: 0,
        };
        if id == FOURCC_RIFF || id == FOURCC_LIST {
            c.fcc_type = m.read_u32().ok_or(MMIOERR_CHUNKNOTFOUND)?;
        }
        let hit = match flags & (MMIO_FINDCHUNK | MMIO_FINDRIFF | MMIO_FINDLIST) {
            MMIO_FINDCHUNK => id == want.ckid,
            MMIO_FINDRIFF => id == FOURCC_RIFF && c.fcc_type == want.fcc_type,
            MMIO_FINDLIST => id == FOURCC_LIST && c.fcc_type == want.fcc_type,
            _ => true,
        };
        if hit {
            return Ok(c);
        }
        let next = data + size as u64 + (size & 1) as u64;
        if m.seek(SeekFrom::Start(next)).is_none() {
            return Err(MMIOERR_CHUNKNOTFOUND);
        }
    });
    match found {
        None => MMSYSERR_INVALHANDLE,
        Some(Err(e)) => e,
        Some(Ok(c)) => {
            ck_write(ck, c);
            MMSYSERR_NOERROR
        }
    }
}

pub unsafe extern "win64" fn mmio_ascend(h: usize, ck: *mut u8, _flags: u32) -> u32 {
    if ck.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let mut c = ck_read(ck);
    let r = with(h, |m| {
        if c.flags & MMIO_DIRTY != 0 {
            // Bloc créé par mmioCreateChunk : sa taille est celle écrite.
            let here = m.pos();
            c.cksize = (here - c.data_offset as u64) as u32;
            m.seek(SeekFrom::Start(c.data_offset as u64 - 4))
                .ok_or(MMIOERR_CANNOTSEEK)?;
            m.write(&c.cksize.to_le_bytes())
                .ok_or(MMIOERR_CANNOTWRITE)?;
            m.seek(SeekFrom::Start(here)).ok_or(MMIOERR_CANNOTSEEK)?;
            if c.cksize & 1 != 0 {
                m.write(&[0]).ok_or(MMIOERR_CANNOTWRITE)?;
            }
            c.flags &= !MMIO_DIRTY;
            return Ok(());
        }
        let next = c.data_offset as u64 + c.cksize as u64 + (c.cksize & 1) as u64;
        m.seek(SeekFrom::Start(next))
            .map(|_| ())
            .ok_or(MMIOERR_CANNOTSEEK)
    });
    match r {
        None => MMSYSERR_INVALHANDLE,
        Some(Err(e)) => e,
        Some(Ok(())) => {
            ck_write(ck, c);
            MMSYSERR_NOERROR
        }
    }
}

pub unsafe extern "win64" fn mmio_create_chunk(h: usize, ck: *mut u8, flags: u32) -> u32 {
    if ck.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let mut c = ck_read(ck);
    if flags & MMIO_CREATERIFF != 0 {
        c.ckid = FOURCC_RIFF;
    } else if flags & MMIO_CREATELIST != 0 {
        c.ckid = FOURCC_LIST;
    }
    let r = with(h, |m| {
        let start = m.pos();
        let mut head = Vec::with_capacity(12);
        head.extend_from_slice(&c.ckid.to_le_bytes());
        head.extend_from_slice(&c.cksize.to_le_bytes());
        if c.ckid == FOURCC_RIFF || c.ckid == FOURCC_LIST {
            head.extend_from_slice(&c.fcc_type.to_le_bytes());
        }
        m.write(&head).ok_or(MMIOERR_CANNOTWRITE)?;
        c.data_offset = (start + 8) as u32;
        c.flags = MMIO_DIRTY;
        Ok(())
    });
    match r {
        None => MMSYSERR_INVALHANDLE,
        Some(Err(e)) => e,
        Some(Ok(())) => {
            ck_write(ck, c);
            MMSYSERR_NOERROR
        }
    }
}

fn to_fourcc(s: &str, flags: u32) -> u32 {
    let mut b = [b' '; 4];
    for (i, ch) in s.bytes().take(4).enumerate() {
        b[i] = if flags & MMIO_TOUPPER != 0 {
            ch.to_ascii_uppercase()
        } else {
            ch
        };
    }
    u32::from_le_bytes(b)
}

pub unsafe extern "win64" fn mmio_string_to_fourcc_a(s: *const u8, flags: u32) -> u32 {
    if s.is_null() {
        return 0;
    }
    to_fourcc(&crate::dlls::cstr(s), flags)
}

pub unsafe extern "win64" fn mmio_string_to_fourcc_w(s: *const u16, flags: u32) -> u32 {
    if s.is_null() {
        return 0;
    }
    to_fourcc(&crate::dlls::wstr(s), flags)
}

/// mmioGetInfo : MMIOINFO sans tampon d'E/S (cchBuffer 0).
pub unsafe extern "win64" fn mmio_get_info(h: usize, info: *mut u8, _flags: u32) -> u32 {
    if info.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    if with(h, |_| ()).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    let size = if wow32() { 72 } else { 112 };
    std::ptr::write_bytes(info, 0, size);
    let h_off = info_offsets().4;
    if wow32() {
        (info.add(h_off) as *mut u32).write_unaligned(h as u32);
    } else {
        (info.add(h_off) as *mut usize).write_unaligned(h);
    }
    MMSYSERR_NOERROR
}

pub extern "win64" fn mmio_set_info(h: usize, _info: usize, _flags: u32) -> u32 {
    if with(h, |_| ()).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    MMSYSERR_NOERROR
}

pub extern "win64" fn mmio_advance(h: usize, _info: usize, _flags: u32) -> u32 {
    if with(h, |_| ()).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    MMIOERR_UNBUFFERED
}

/// mmioSetBuffer : accepté, les E/S restent directes.
pub extern "win64" fn mmio_set_buffer(h: usize, _buf: usize, _len: i32, _flags: u32) -> u32 {
    if with(h, |_| ()).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    MMSYSERR_NOERROR
}

pub extern "win64" fn mmio_flush(h: usize, _flags: u32) -> u32 {
    with(h, |m| {
        if let Backing::File(f) = &mut m.io {
            let _ = f.flush();
        }
    })
    .map_or(MMSYSERR_INVALHANDLE, |_| MMSYSERR_NOERROR)
}

/// Procédures d'E/S personnalisées : non prises en charge (NULL).
pub extern "win64" fn mmio_install_io_proc(_fcc: u32, _proc: usize, _flags: u32) -> usize {
    0
}

pub extern "win64" fn mmio_send_message(_h: usize, _msg: u32, _p1: isize, _p2: isize) -> isize {
    0
}

unsafe fn rename(from: String, to: String) -> u32 {
    match std::fs::rename(crate::paths::to_linux(&from), crate::paths::to_linux(&to)) {
        Ok(()) => MMSYSERR_NOERROR,
        Err(e) => error_of(&e),
    }
}

pub unsafe extern "win64" fn mmio_rename_w(
    from: *const u16,
    to: *const u16,
    _info: usize,
    _flags: u32,
) -> u32 {
    if from.is_null() || to.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    rename(crate::dlls::wstr(from), crate::dlls::wstr(to))
}

pub unsafe extern "win64" fn mmio_rename_a(
    from: *const u8,
    to: *const u8,
    _info: usize,
    _flags: u32,
) -> u32 {
    if from.is_null() || to.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    rename(crate::dlls::cstr(from), crate::dlls::cstr(to))
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "mmioOpenW" => fn mmio_open_w, "mmioOpenA" => fn mmio_open_a,
        "mmioClose" => fn mmio_close, "mmioRead" => fn mmio_read,
        "mmioWrite" => fn mmio_write, "mmioSeek" => fn mmio_seek,
        "mmioDescend" => fn mmio_descend, "mmioAscend" => fn mmio_ascend,
        "mmioCreateChunk" => fn mmio_create_chunk,
        "mmioStringToFOURCCA" => fn mmio_string_to_fourcc_a,
        "mmioStringToFOURCCW" => fn mmio_string_to_fourcc_w,
        "mmioGetInfo" => fn mmio_get_info, "mmioSetInfo" => fn mmio_set_info,
        "mmioAdvance" => fn mmio_advance, "mmioSetBuffer" => fn mmio_set_buffer,
        "mmioFlush" => fn mmio_flush,
        "mmioInstallIOProcA" => fn mmio_install_io_proc,
        "mmioInstallIOProcW" => fn mmio_install_io_proc,
        "mmioSendMessage" => fn mmio_send_message,
        "mmioRenameA" => fn mmio_rename_a, "mmioRenameW" => fn mmio_rename_w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le parcours de SoundPlayer : RIFF/WAVE, bloc « fmt », lecture,
    /// remontée, bloc « data » ; puis écriture d'un fichier RIFF.
    #[test]
    fn mmio_lit_et_ecrit_un_wav() {
        let dir = std::env::temp_dir().join(format!("winlux-mmio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("son.wav");
        let win = |p: &std::path::Path| -> Vec<u16> {
            crate::paths::to_windows(p)
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect()
        };
        unsafe {
            // Écriture : RIFF WAVE { fmt (16 octets), data (3 octets) }.
            let name = win(&path);
            let h = mmio_open_w(
                name.as_ptr(),
                std::ptr::null_mut(),
                MMIO_CREATE | MMIO_WRITE,
            );
            assert_ne!(h, 0);
            let mut riff = [0u8; 20];
            (riff.as_mut_ptr().add(8) as *mut u32).write_unaligned(fourcc(b"WAVE"));
            assert_eq!(mmio_create_chunk(h, riff.as_mut_ptr(), MMIO_CREATERIFF), 0);
            let mut fmt = [0u8; 20];
            (fmt.as_mut_ptr() as *mut u32).write_unaligned(fourcc(b"fmt "));
            assert_eq!(mmio_create_chunk(h, fmt.as_mut_ptr(), 0), 0);
            let format: [u8; 16] = [1, 0, 1, 0, 0x44, 0xAC, 0, 0, 0x88, 0x58, 1, 0, 2, 0, 16, 0];
            assert_eq!(mmio_write(h, format.as_ptr(), 16), 16);
            assert_eq!(mmio_ascend(h, fmt.as_mut_ptr(), 0), 0);
            let mut data = [0u8; 20];
            (data.as_mut_ptr() as *mut u32).write_unaligned(fourcc(b"data"));
            assert_eq!(mmio_create_chunk(h, data.as_mut_ptr(), 0), 0);
            assert_eq!(mmio_write(h, [1u8, 2, 3].as_ptr(), 3), 3);
            assert_eq!(mmio_ascend(h, data.as_mut_ptr(), 0), 0);
            assert_eq!(mmio_ascend(h, riff.as_mut_ptr(), 0), 0);
            assert_eq!(mmio_close(h, 0), 0);
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(&bytes[..4], b"RIFF");
            // 4 (WAVE) + 8 + 16 + 8 + 3 + 1 (alignement)
            assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 40);
            assert_eq!(bytes.len(), 48);

            // Lecture comme SoundPlayer.ValidateSoundFile.
            let h = mmio_open_w(name.as_ptr(), std::ptr::null_mut(), 0x10000);
            assert_ne!(h, 0);
            let mut riff = CkInfo {
                fcc_type: fourcc(b"WAVE"),
                ..CkInfo::default()
            };
            let rp = &mut riff as *mut CkInfo as *mut u8;
            assert_eq!(mmio_descend(h, rp, std::ptr::null(), MMIO_FINDRIFF), 0);
            assert_eq!((riff.cksize, riff.data_offset), (40, 8));
            let mut ck = CkInfo {
                ckid: fourcc(b"data"),
                ..CkInfo::default()
            };
            let cp = &mut ck as *mut CkInfo as *mut u8;
            // « data » est trouvé en sautant « fmt ».
            assert_eq!(mmio_descend(h, cp, rp, MMIO_FINDCHUNK), 0);
            assert_eq!(ck.cksize, 3);
            let mut buf = [0u8; 8];
            assert_eq!(mmio_read(h, buf.as_mut_ptr(), 8), 4); // 3 octets + alignement, puis fin
            assert_eq!(&buf[..3], &[1, 2, 3]);
            assert_eq!(mmio_seek(h, 12, 0), 12);
            (*(cp as *mut CkInfo)).ckid = fourcc(b"fmt ");
            assert_eq!(mmio_descend(h, cp, rp, MMIO_FINDCHUNK), 0);
            assert_eq!(ck.cksize, 16);
            assert_eq!(mmio_ascend(h, cp, 0), 0);
            assert_eq!(mmio_seek(h, 0, 1), 36);
            (*(cp as *mut CkInfo)).ckid = fourcc(b"zzzz");
            assert_eq!(
                mmio_descend(h, cp, rp, MMIO_FINDCHUNK),
                MMIOERR_CHUNKNOTFOUND
            );
            assert_eq!(mmio_close(h, 0), 0);
            assert_eq!(mmio_close(h, 0), MMSYSERR_INVALHANDLE);

            let missing = win(&dir.join("absent.wav"));
            assert_eq!(mmio_open_w(missing.as_ptr(), std::ptr::null_mut(), 0), 0);
            assert_eq!(to_fourcc("wave", MMIO_TOUPPER), fourcc(b"WAVE"));
            assert_eq!(to_fourcc("ab", 0), fourcc(b"ab  "));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fichier en mémoire (FOURCC_MEM) décrit par MMIOINFO.
    #[test]
    fn mmio_fichier_en_memoire() {
        let mut mem = *b"RIFF\x04\x00\x00\x00WAVE";
        let mut info = [0u8; 112];
        unsafe {
            (info.as_mut_ptr().add(4) as *mut u32).write_unaligned(FOURCC_MEM);
            (info.as_mut_ptr().add(32) as *mut i32).write_unaligned(mem.len() as i32);
            (info.as_mut_ptr().add(40) as *mut usize).write_unaligned(mem.as_mut_ptr() as usize);
            let h = mmio_open_w(std::ptr::null(), info.as_mut_ptr(), 0);
            assert_ne!(h, 0);
            let mut riff = CkInfo {
                fcc_type: fourcc(b"WAVE"),
                ..CkInfo::default()
            };
            assert_eq!(
                mmio_descend(
                    h,
                    &mut riff as *mut CkInfo as *mut u8,
                    std::ptr::null(),
                    MMIO_FINDRIFF
                ),
                0
            );
            assert_eq!(riff.cksize, 4);
            assert_eq!(mmio_close(h, 0), 0);
        }
    }
}
