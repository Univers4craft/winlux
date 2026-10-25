//! Flux de fichiers de la CRT (`FILE*`) : `fopen`, `fread`, `fwrite`,
//! `fprintf`, `fgets`, `fseek`, `feof`…
//!
//! Un `FILE*` est ici un pointeur opaque vers une entrée du registre local;
//! les trois flux standard sont reliés aux entrées/sorties du processus.

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Disk,
    Stdin,
    Stdout,
    Stderr,
}

struct Stream {
    kind: Kind,
    file: Mutex<Option<File>>,
    eof: AtomicBool,
    err: AtomicBool,
}

fn registry() -> &'static Mutex<HashMap<usize, Arc<Stream>>> {
    static R: OnceLock<Mutex<HashMap<usize, Arc<Stream>>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lookup(p: *mut c_void) -> Option<Arc<Stream>> {
    std_stream(0);
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&(p as usize))
        .cloned()
}

/// Réserve une valeur de pointeur opaque pour un nouveau flux.
fn new_slot() -> *mut c_void {
    Box::into_raw(Box::new(0u64)) as *mut c_void
}

fn register(kind: Kind, file: Option<File>) -> *mut c_void {
    let p = new_slot();
    let s = Arc::new(Stream {
        kind,
        file: Mutex::new(file),
        eof: AtomicBool::new(false),
        err: AtomicBool::new(false),
    });
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(p as usize, s);
    p
}

fn free_slot(p: *mut c_void) {
    if !p.is_null() {
        unsafe { drop(Box::from_raw(p as *mut u64)) };
    }
}

/// Taille d'un `FILE` de msvcrt.dll : le code compilé avec MinGW calcule
/// `stdout` comme `&__iob_func()[1]`, donc les trois flux standard doivent
/// être contigus avec ce pas.
const MSVCRT_FILE_SIZE: usize = 48;
static mut IOB: [u64; 3 * MSVCRT_FILE_SIZE / 8] = [0; 3 * MSVCRT_FILE_SIZE / 8];

fn std_stream(index: usize) -> *mut c_void {
    static STD: OnceLock<[usize; 3]> = OnceLock::new();
    let handles = STD.get_or_init(|| {
        let base = std::ptr::addr_of_mut!(IOB) as usize;
        let mut reg = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut out = [0usize; 3];
        for (i, kind) in [Kind::Stdin, Kind::Stdout, Kind::Stderr]
            .into_iter()
            .enumerate()
        {
            out[i] = base + i * MSVCRT_FILE_SIZE;
            reg.insert(
                out[i],
                Arc::new(Stream {
                    kind,
                    file: Mutex::new(None),
                    eof: AtomicBool::new(false),
                    err: AtomicBool::new(false),
                }),
            );
        }
        out
    });
    if index < 3 {
        handles[index] as *mut c_void
    } else {
        std::ptr::null_mut()
    }
}

pub unsafe extern "win64" fn iob_func() -> *mut c_void {
    std_stream(0)
}

fn parse_mode(mode: &str) -> Option<(bool, OpenOptions)> {
    let m = mode.as_bytes();
    let base = *m.first()?;
    let mut plus = false;
    for &c in m {
        if c == b'+' {
            plus = true;
        }
    }
    let mut o = OpenOptions::new();
    match base {
        b'r' => {
            o.read(true);
            if plus {
                o.write(true);
            }
        }
        b'w' => {
            o.write(true).create(true).truncate(true);
            if plus {
                o.read(true);
            }
        }
        b'a' => {
            o.write(true).create(true).append(true);
            if plus {
                o.read(true);
            }
        }
        _ => return None,
    }
    Some((plus, o))
}

unsafe fn cstr_bytes(p: *const u8) -> Vec<u8> {
    std::slice::from_raw_parts(p, crate::printf::narrow_len(p)).to_vec()
}

fn read_into(s: &Stream, buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    let n = match s.kind {
        Kind::Disk => {
            let mut g = s
                .file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match g.as_mut() {
                Some(f) => f.read(buf).unwrap_or(0),
                None => 0,
            }
        }
        Kind::Stdin => std::io::stdin().read(buf).unwrap_or(0),
        _ => 0,
    };
    if n == 0 {
        s.eof.store(true, Ordering::Release);
    }
    n
}

fn write_from(s: &Stream, buf: &[u8]) -> usize {
    match s.kind {
        Kind::Disk => {
            let mut g = s
                .file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match g.as_mut() {
                Some(f) => f.write(buf).unwrap_or(0),
                None => 0,
            }
        }
        Kind::Stdout => std::io::stdout().write(buf).unwrap_or(0),
        Kind::Stderr => std::io::stderr().write(buf).unwrap_or(0),
        Kind::Stdin => 0,
    }
}

fn flush_stream(s: &Stream) -> bool {
    let mut ok = true;
    if s.kind == Kind::Disk {
        let mut g = s
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(f) = g.as_mut() {
            ok = f.flush().is_ok();
        }
    } else if s.kind == Kind::Stdout {
        ok = std::io::stdout().flush().is_ok();
    } else if s.kind == Kind::Stderr {
        ok = std::io::stderr().flush().is_ok();
    }
    if !ok {
        s.err.store(true, Ordering::Release);
    }
    ok
}

// ---------- Ouverture / fermeture ----------

pub unsafe extern "win64" fn fopen(path: *const u8, mode: *const u8) -> *mut c_void {
    if path.is_null() || mode.is_null() {
        return std::ptr::null_mut();
    }
    let path = match String::from_utf8(cstr_bytes(path)) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let mode = String::from_utf8_lossy(&cstr_bytes(mode)).to_string();
    let file = match parse_mode(&mode) {
        Some((_, o)) => o.open(crate::paths::to_linux(&path)),
        None => return std::ptr::null_mut(),
    };
    match file {
        Ok(f) => register(Kind::Disk, Some(f)),
        Err(_) => std::ptr::null_mut(),
    }
}

pub unsafe extern "win64" fn wfopen(path: *const u16, mode: *const u16) -> *mut c_void {
    if path.is_null() || mode.is_null() {
        return std::ptr::null_mut();
    }
    let p = crate::dlls::wstr(path);
    let m = crate::dlls::wstr(mode);
    let file = match parse_mode(&m) {
        Some((_, o)) => o.open(crate::paths::to_linux(&p)),
        None => return std::ptr::null_mut(),
    };
    match file {
        Ok(f) => register(Kind::Disk, Some(f)),
        Err(_) => std::ptr::null_mut(),
    }
}

pub unsafe extern "win64" fn fopen_s(
    result: *mut *mut c_void,
    path: *const u8,
    mode: *const u8,
) -> i32 {
    if result.is_null() {
        return 22;
    }
    let f = fopen(path, mode);
    if f.is_null() {
        *result = std::ptr::null_mut();
        22
    } else {
        *result = f;
        0
    }
}

pub unsafe extern "win64" fn fclose(p: *mut c_void) -> i32 {
    let s = match lookup(p) {
        Some(s) => s,
        None => return -1,
    };
    let ok = flush_stream(&s);
    // Les flux standard vivent dans `_iob` : on les vide sans les libérer.
    if s.kind != Kind::Disk {
        return if ok { 0 } else { -1 };
    }
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&(p as usize));
    *s.file
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    free_slot(p);
    if ok {
        0
    } else {
        -1
    }
}

// ---------- Lecture / écriture ----------

pub unsafe extern "win64" fn fread(
    ptr: *mut u8,
    size: usize,
    count: usize,
    p: *mut c_void,
) -> usize {
    let s = match lookup(p) {
        Some(s) => s,
        None => return 0,
    };
    if size == 0 || count == 0 || ptr.is_null() {
        return 0;
    }
    let total = size.saturating_mul(count);
    let buf = std::slice::from_raw_parts_mut(ptr, total);
    let mut read = 0;
    while read < total {
        let n = read_into(&s, &mut buf[read..]);
        if n == 0 {
            break;
        }
        read += n;
    }
    read / size
}

pub unsafe extern "win64" fn fwrite(
    ptr: *const u8,
    size: usize,
    count: usize,
    p: *mut c_void,
) -> usize {
    let s = match lookup(p) {
        Some(s) => s,
        None => return 0,
    };
    if size == 0 || count == 0 || ptr.is_null() {
        return 0;
    }
    let total = size.saturating_mul(count);
    let buf = std::slice::from_raw_parts(ptr, total);
    let mut written = 0;
    while written < total {
        let n = write_from(&s, &buf[written..]);
        if n == 0 {
            s.err.store(true, Ordering::Release);
            break;
        }
        written += n;
    }
    written / size
}

pub unsafe extern "win64" fn fputc(c: i32, p: *mut c_void) -> i32 {
    let s = match lookup(p) {
        Some(s) => s,
        None => return -1,
    };
    let b = [(c as u8)];
    if write_from(&s, &b) == 1 {
        (c as u8) as i32
    } else {
        s.err.store(true, Ordering::Release);
        -1
    }
}

pub unsafe extern "win64" fn fgetc(p: *mut c_void) -> i32 {
    let s = match lookup(p) {
        Some(s) => s,
        None => return -1,
    };
    let mut b = [0u8; 1];
    match read_into(&s, &mut b) {
        0 => -1,
        _ => b[0] as i32,
    }
}

pub unsafe extern "win64" fn fgets(buf: *mut u8, n: i32, p: *mut c_void) -> *mut u8 {
    if buf.is_null() || n <= 0 {
        return std::ptr::null_mut();
    }
    let s = match lookup(p) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let cap = n as usize;
    let mut i = 0;
    let mut one = [0u8; 1];
    while i + 1 < cap {
        let got = read_into(&s, &mut one);
        if got == 0 {
            break;
        }
        *buf.add(i) = one[0];
        i += 1;
        if one[0] == b'\n' {
            break;
        }
    }
    if i == 0 {
        return std::ptr::null_mut();
    }
    *buf.add(i) = 0;
    buf
}

pub unsafe extern "win64" fn fputs(text: *const u8, p: *mut c_void) -> i32 {
    let s = match lookup(p) {
        Some(s) => s,
        None => return -1,
    };
    let b = cstr_bytes(text);
    let n = write_from(&s, &b);
    if n == b.len() {
        0
    } else {
        s.err.store(true, Ordering::Release);
        -1
    }
}

/// Lit tout le contenu restant d'un flux ; utilisé par `fscanf`.
pub unsafe fn read_to_end(p: *mut c_void) -> Vec<u8> {
    let s = match lookup(p) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = read_into(&s, &mut buf);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    data
}

// ---------- Positionnement et état ----------
pub unsafe extern "win64" fn fseek(p: *mut c_void, offset: i64, origin: i32) -> i32 {
    let s = match lookup(p) {
        Some(s) => s,
        None => return -1,
    };
    if s.kind != Kind::Disk {
        return -1;
    }
    let from = match origin {
        0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return -1,
    };
    let mut g = s
        .file
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match g.as_mut() {
        Some(f) => {
            if f.seek(from).is_ok() {
                s.eof.store(false, Ordering::Release);
                0
            } else {
                s.err.store(true, Ordering::Release);
                -1
            }
        }
        None => -1,
    }
}

pub unsafe extern "win64" fn ftell(p: *mut c_void) -> i64 {
    let s = match lookup(p) {
        Some(s) => s,
        None => return -1,
    };
    let mut g = s
        .file
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match g.as_mut() {
        Some(f) => f.stream_position().map(|n| n as i64).unwrap_or(-1),
        None => -1,
    }
}

pub unsafe extern "win64" fn rewind(p: *mut c_void) {
    fseek(p, 0, 0);
    s_clearerr(p);
}
unsafe fn s_clearerr(p: *mut c_void) {
    if let Some(s) = lookup(p) {
        s.eof.store(false, Ordering::Release);
        s.err.store(false, Ordering::Release);
    }
}

pub unsafe extern "win64" fn feof(p: *mut c_void) -> i32 {
    lookup(p)
        .map(|s| s.eof.load(Ordering::Acquire) as i32)
        .unwrap_or(0)
}
pub unsafe extern "win64" fn ferror(p: *mut c_void) -> i32 {
    lookup(p)
        .map(|s| s.err.load(Ordering::Acquire) as i32)
        .unwrap_or(0)
}
pub unsafe extern "win64" fn clearerr(p: *mut c_void) {
    s_clearerr(p)
}

pub unsafe extern "win64" fn fflush(p: *mut c_void) -> i32 {
    if p.is_null() {
        let all: Vec<Arc<Stream>> = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        return if all.iter().all(|s| flush_stream(s)) {
            0
        } else {
            -1
        };
    }
    match lookup(p) {
        Some(s) if flush_stream(&s) => 0,
        _ => -1,
    }
}

pub unsafe extern "win64" fn setvbuf(
    _p: *mut c_void,
    _buf: *mut u8,
    _mode: i32,
    _size: usize,
) -> i32 {
    0
}

pub unsafe extern "win64" fn fileno(p: *mut c_void) -> i32 {
    match lookup(p) {
        Some(s) => match s.kind {
            Kind::Stdin => 0,
            Kind::Stdout => 1,
            Kind::Stderr => 2,
            Kind::Disk => -1,
        },
        None => -1,
    }
}

pub unsafe extern "win64" fn acrt_iob_func(index: u32) -> *mut c_void {
    std_stream(index as usize)
}

pub unsafe extern "win64" fn filelength(p: *mut c_void) -> i32 {
    let s = match lookup(p) {
        Some(s) => s,
        None => return -1,
    };
    let g = s
        .file
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match g.as_ref() {
        Some(f) => f.metadata().map(|m| m.len() as i32).unwrap_or(-1),
        None => -1,
    }
}

pub unsafe extern "win64" fn remove_file(path: *const u8) -> i32 {
    if path.is_null() {
        return -1;
    }
    match std::fs::remove_file(crate::paths::to_linux(&String::from_utf8_lossy(
        &cstr_bytes(path),
    ))) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

pub unsafe extern "win64" fn rename_file(old: *const u8, new: *const u8) -> i32 {
    if old.is_null() || new.is_null() {
        return -1;
    }
    let o = String::from_utf8_lossy(&cstr_bytes(old)).to_string();
    let n = String::from_utf8_lossy(&cstr_bytes(new)).to_string();
    match std::fs::rename(crate::paths::to_linux(&o), crate::paths::to_linux(&n)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

// ---------- Sortie formatée sur un flux ----------

unsafe fn format_narrow(fmt: *const u8, va: &mut crate::thunks::VaList) -> Vec<u8> {
    crate::printf::format(
        std::slice::from_raw_parts(fmt, crate::printf::narrow_len(fmt)),
        va,
        crate::printf::MSVCRT,
    )
}

unsafe extern "win64" fn imp_fprintf(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let p = v.next_u64() as *mut c_void;
    let fmt = v.next_ptr::<u8>();
    let b = format_narrow(fmt, &mut v);
    match lookup(p) {
        Some(s) if write_from(&s, &b) == b.len() => b.len() as i32,
        _ => -1,
    }
}
unsafe extern "win64" fn imp_vfprintf(args: *const u64) -> i32 {
    let p = *args as *mut c_void;
    let fmt = *args.add(1) as *const u8;
    let va = *args.add(2) as *const u64;
    let mut v = crate::thunks::VaList::new(va);
    let b = format_narrow(fmt, &mut v);
    match lookup(p) {
        Some(s) if write_from(&s, &b) == b.len() => b.len() as i32,
        _ => -1,
    }
}
unsafe extern "win64" fn imp_vsprintf(args: *const u64) -> i32 {
    let dst = *args as *mut u8;
    let fmt = *args.add(1) as *const u8;
    let va = *args.add(2) as *const u64;
    let mut v = crate::thunks::VaList::new(va);
    let b = format_narrow(fmt, &mut v);
    std::ptr::copy_nonoverlapping(b.as_ptr(), dst, b.len());
    *dst.add(b.len()) = 0;
    b.len() as i32
}

// Ponts variadiques pour la sortie formatée sur flux.

pub unsafe extern "win64" fn fgetpos(p: *mut c_void, pos: *mut i64) -> i32 {
    if p.is_null() || pos.is_null() {
        return -1;
    }
    let off = ftell(p);
    if off < 0 {
        return -1;
    }
    *pos = off;
    0
}
pub unsafe extern "win64" fn fsetpos(p: *mut c_void, pos: *const i64) -> i32 {
    if p.is_null() || pos.is_null() {
        return -1;
    }
    fseek(p, *pos, 0)
}

crate::varargs_thunk!(fprintf_thunk => imp_fprintf);
crate::varargs_thunk!(vfprintf_thunk => imp_vfprintf);
crate::varargs_thunk!(vsprintf_thunk => imp_vsprintf);

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "fopen" => fn fopen, "_wfopen" => fn wfopen, "fopen_s" => fn fopen_s, "fclose" => fn fclose,
        "fread" => fn fread, "fwrite" => fn fwrite, "fgets" => fn fgets, "fputs" => fn fputs,
        "fgetc" => fn fgetc, "fputc" => fn fputc, "getc" => fn fgetc, "putc" => fn fputc,
        "fseek" => fn fseek, "ftell" => fn ftell, "rewind" => fn rewind, "fgetpos" => fn fgetpos, "fsetpos" => fn fsetpos,
        "_fseeki64" => fn fseek, "_ftelli64" => fn ftell,
        "feof" => fn feof, "ferror" => fn ferror, "clearerr" => fn clearerr,
        "fflush" => fn fflush, "setvbuf" => fn setvbuf, "setbuf" => fn setvbuf, "_fileno" => fn fileno,
        "__acrt_iob_func" => fn acrt_iob_func, "__iob_func" => fn iob_func, "_iob" => data IOB, "_filelength" => fn filelength,
        "remove" => fn remove_file, "rename" => fn rename_file,
        "fprintf" => fn fprintf_thunk,
        "vfprintf" => fn vfprintf_thunk, "vsprintf" => fn vsprintf_thunk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flux_ecrit_relit_et_repositionne() {
        unsafe {
            let base =
                std::env::temp_dir().join(format!("winlux-crt-stream-{}.txt", std::process::id()));
            let path = format!("{}\0", base.display());
            let f = fopen(path.as_ptr(), b"w+\0".as_ptr());
            assert!(!f.is_null());
            let payload = b"ligne1\nligne2\n";
            assert_eq!(fwrite(payload.as_ptr(), 1, payload.len(), f), payload.len());
            assert_eq!(fflush(f), 0);
            assert_eq!(ftell(f), payload.len() as i64);
            let mut saved = -1;
            assert_eq!(fgetpos(f, &mut saved), 0);
            assert_eq!(saved, payload.len() as i64);
            assert_eq!(fseek(f, 0, 0), 0);
            assert_eq!(fsetpos(f, &saved), 0);
            assert_eq!(ftell(f), saved);
            assert_eq!(fgetpos(f, std::ptr::null_mut()), -1);
            assert_eq!(fsetpos(f, std::ptr::null()), -1);
            assert_eq!(fseek(f, 0, 0), 0);
            let mut buf = [0u8; 32];
            let read = fread(buf.as_mut_ptr(), 1, 7, f);
            assert_eq!(read, 7);
            assert_eq!(&buf[..7], b"ligne1\n");
            assert_eq!(
                fgets(buf.as_mut_ptr(), 32, f) as usize,
                buf.as_ptr() as usize
            );
            assert_eq!(
                std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8).to_bytes(),
                b"ligne2\n"
            );
            assert_eq!(fgets(buf.as_mut_ptr(), 32, f) as usize, 0);
            assert_eq!(feof(f), 1);
            assert_eq!(filelength(f), payload.len() as i32);
            assert_eq!(fclose(f), 0);
            assert_eq!(std::fs::read(&base).unwrap(), payload);
            assert_eq!(remove_file(path.as_ptr()), 0);
        }
    }

    #[test]
    fn flux_standard_et_erreurs() {
        unsafe {
            assert!(!std_stream(0).is_null());
            assert_eq!(fileno(std_stream(1)), 1);
            assert_eq!(fileno(std_stream(2)), 2);
            assert!(std_stream(3).is_null());
            assert!(fopen(b"\0".as_ptr(), b"z\0".as_ptr()).is_null());
            assert_eq!(ferror(std_stream(1)), 0);
            assert_eq!(fseek(std_stream(0), 0, 0), -1);
        }
    }
}
