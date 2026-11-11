//! Fonctions du CRT importées par python312.dll x86 et absentes jusque-là
//! (#1383) : arrondi flottant, console (`_putch`, `_getwch`…), processus
//! (`_wspawnv`, `_cwait`, `_wsystem`…), descripteurs et chaînes.

use super::crt_runtime::errno;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

// --- Flottants ------------------------------------------------------------------------

/// `__fpe_flt_rounds` : valeur de FLT_ROUNDS selon l'arrondi du mot de
/// contrôle (0 vers zéro, 1 au plus proche, 2 vers +∞, 3 vers −∞).
/// python312.dll l'appelle au démarrage : son absence arrêtait les
/// programmes PyInstaller 32 bits (#1383).
pub extern "win64" fn fpe_flt_rounds() -> i32 {
    let word = super::crt_ucrt::controlfp(0, 0);
    match word & 0x300 {
        0x000 => 1, // _RC_NEAR
        0x100 => 3, // _RC_DOWN
        0x200 => 2, // _RC_UP
        _ => 0,     // _RC_CHOP
    }
}

/// `__control87_2(new, mask, x86_cw, sse2_cw)` : un seul mot virtuel ici,
/// rendu pour les deux unités.
pub unsafe extern "win64" fn control87_2(
    new: u32,
    mask: u32,
    x86: *mut u32,
    sse2: *mut u32,
) -> i32 {
    let word = super::crt_ucrt::controlfp(new, mask);
    if !x86.is_null() {
        *x86 = word;
    }
    if !sse2.is_null() {
        *sse2 = word;
    }
    1
}

// --- Console --------------------------------------------------------------------------

/// Caractères rendus par `_ungetch` / `_ungetwch`, relus en premier.
static PUSHBACK: Mutex<Vec<u32>> = Mutex::new(Vec::new());

fn pushed() -> Option<u32> {
    PUSHBACK.lock().unwrap_or_else(|e| e.into_inner()).pop()
}

unsafe fn read_byte() -> Option<u8> {
    let mut b = 0u8;
    (libc::read(0, (&mut b as *mut u8).cast(), 1) == 1).then_some(b)
}

/// Un caractère UTF-8 lu sur l'entrée standard.
unsafe fn read_char() -> Option<u32> {
    let first = read_byte()?;
    let extra = match first {
        0x00..=0x7F => return Some(first as u32),
        0xC0..=0xDF => 1,
        0xE0..=0xEF => 2,
        0xF0..=0xF7 => 3,
        _ => return Some(0xFFFD),
    };
    let mut bytes = vec![first];
    for _ in 0..extra {
        bytes.push(read_byte()?);
    }
    Some(
        std::str::from_utf8(&bytes)
            .ok()
            .and_then(|s| s.chars().next())
            .map_or(0xFFFD, |c| c as u32),
    )
}

unsafe fn write_char(c: u32) -> bool {
    let Some(ch) = char::from_u32(c) else {
        return false;
    };
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    libc::write(1, s.as_ptr().cast(), s.len()) == s.len() as isize
}

pub unsafe extern "win64" fn putch(c: i32) -> i32 {
    let b = c as u8;
    if libc::write(1, (&b as *const u8).cast(), 1) == 1 {
        b as i32
    } else {
        -1
    }
}

pub unsafe extern "win64" fn putwch(c: u16) -> u16 {
    if write_char(c as u32) {
        c
    } else {
        0xFFFF
    }
}

pub unsafe extern "win64" fn getwch() -> u16 {
    pushed()
        .or_else(|| read_char())
        .map_or(0xFFFF, |c| c.min(0xFFFF) as u16)
}

pub unsafe extern "win64" fn getwche() -> u16 {
    let c = getwch();
    if c != 0xFFFF {
        write_char(c as u32);
    }
    c
}

pub unsafe extern "win64" fn getche() -> i32 {
    let c = match pushed() {
        Some(c) => c as i32,
        None => read_byte().map_or(-1, |b| b as i32),
    };
    if c >= 0 {
        putch(c);
    }
    c
}

pub extern "win64" fn ungetch(c: i32) -> i32 {
    if c < 0 {
        return -1;
    }
    PUSHBACK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(c as u32 & 0xFF);
    c
}

pub extern "win64" fn ungetwch(c: u16) -> u16 {
    if c == 0xFFFF {
        return c;
    }
    PUSHBACK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(c as u32);
    c
}

// --- Descripteurs, tas, environnement ---------------------------------------------------

/// `_chsize_s(fd, taille)` : 0, ou le code errno.
pub unsafe extern "win64" fn chsize_s(fd: i32, size: i64) -> i32 {
    if size < 0 {
        *errno() = 22;
        return 22;
    }
    if libc::ftruncate(fd, size) == 0 {
        0
    } else {
        let e = if *libc::__errno_location() == libc::EBADF {
            9
        } else {
            22
        };
        *errno() = e;
        e
    }
}

/// Masque de `_umask` : gardé pour le programme, les droits Linux ne
/// suivent pas les bits _S_IREAD / _S_IWRITE.
static UMASK: AtomicI32 = AtomicI32::new(0);
pub extern "win64" fn umask(mode: i32) -> i32 {
    UMASK.swap(mode & 0x180, Ordering::SeqCst)
}

pub extern "win64" fn heapmin() -> i32 {
    0
}

/// `_resetstkoflw` : la page de garde se rétablit seule ici.
pub extern "win64" fn resetstkoflw() -> i32 {
    1
}

pub unsafe extern "win64" fn wputenv_s(name: *const u16, value: *const u16) -> i32 {
    if name.is_null() || value.is_null() {
        *errno() = 22;
        return 22;
    }
    let k = super::wstr(name);
    if k.is_empty() || k.contains('=') {
        *errno() = 22;
        return 22;
    }
    let v = super::wstr(value);
    if v.is_empty() {
        std::env::remove_var(&k);
    } else {
        std::env::set_var(&k, &v);
    }
    super::crt_runtime::refresh_environ();
    0
}

/// `wcstok_s(chaîne, séparateurs, contexte)`. Le contexte est un pointeur
/// de la taille de ceux du programme (4 octets en x86).
pub unsafe extern "win64" fn wcstok_s(s: *mut u16, delim: *const u16, context: usize) -> *mut u16 {
    use crate::wow32::{read_ptr, write_ptr};
    if context == 0 || delim.is_null() {
        *errno() = 22;
        return std::ptr::null_mut();
    }
    let is_delim = |c: u16| {
        let mut i = 0;
        loop {
            let d = *delim.add(i);
            if d == 0 {
                return false;
            }
            if d == c {
                return true;
            }
            i += 1;
        }
    };
    let mut cur = if s.is_null() {
        read_ptr(context) as *mut u16
    } else {
        s
    };
    if cur.is_null() {
        return std::ptr::null_mut();
    }
    while *cur != 0 && is_delim(*cur) {
        cur = cur.add(1);
    }
    if *cur == 0 {
        write_ptr(context, cur as usize);
        return std::ptr::null_mut();
    }
    let start = cur;
    while *cur != 0 && !is_delim(*cur) {
        cur = cur.add(1);
    }
    if *cur != 0 {
        *cur = 0;
        cur = cur.add(1);
    }
    write_ptr(context, cur as usize);
    start
}

// --- Processus ------------------------------------------------------------------------

const P_WAIT: i32 = 0;
const P_NOWAIT: i32 = 1;
const P_OVERLAY: i32 = 2;
const P_NOWAITO: i32 = 3;
const P_DETACH: i32 = 4;

/// Un argument cité selon les règles de CommandLineToArgvW.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}

/// Tableau de chaînes larges terminé par NULL (pointeurs du programme).
unsafe fn wide_list(list: usize) -> Vec<String> {
    use crate::wow32::{pointer_size, read_ptr};
    let mut out = Vec::new();
    if list == 0 {
        return out;
    }
    let mut at = list;
    loop {
        let p = read_ptr(at);
        if p == 0 {
            break;
        }
        out.push(super::wstr(p as *const u16));
        at += pointer_size();
    }
    out
}

/// Lance `line` (programme `app` s'il est donné) ; rend le handle du
/// processus, ou None (errno posé).
unsafe fn start(app: Option<&str>, line: &str, env: Option<&[String]>) -> Option<usize> {
    use crate::dlls::kernel32::{create_process_w, ProcessInformation, StartupInfoA};
    let app_w: Option<Vec<u16>> = app.map(|a| a.encode_utf16().chain([0]).collect());
    let mut line_w: Vec<u16> = line.encode_utf16().chain([0]).collect();
    // Bloc d'environnement Unicode : « nom=valeur\0 » … « \0 ».
    let env_w: Option<Vec<u16>> = env.map(|vars| {
        let mut b: Vec<u16> = Vec::new();
        for v in vars {
            b.extend(v.encode_utf16());
            b.push(0);
        }
        b.push(0);
        b
    });
    let startup: StartupInfoA = std::mem::zeroed();
    let mut info: ProcessInformation = std::mem::zeroed();
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x400;
    let ok = create_process_w(
        app_w.as_ref().map_or(std::ptr::null(), |a| a.as_ptr()),
        line_w.as_mut_ptr(),
        0,
        0,
        1,
        if env_w.is_some() {
            CREATE_UNICODE_ENVIRONMENT
        } else {
            0
        },
        env_w
            .as_ref()
            .map_or(std::ptr::null(), |e| e.as_ptr().cast()),
        std::ptr::null(),
        &startup,
        &mut info,
    );
    if ok == 0 {
        *errno() = 2; // ENOENT
        return None;
    }
    crate::dlls::kernel32::close_handle(info.thread);
    Some(info.process)
}

/// Attend la fin du processus et rend son code de sortie.
unsafe fn wait_exit(process: usize) -> i32 {
    crate::dlls::kernel32::wait_for_single_object(process, u32::MAX);
    let mut code = 0u32;
    crate::dlls::kernel32::get_exit_code_process(process, &mut code);
    code as i32
}

unsafe fn spawn(mode: i32, path: *const u16, argv: usize, envp: usize) -> isize {
    if path.is_null() || argv == 0 {
        *errno() = 22;
        return -1;
    }
    let app = super::wstr(path);
    let args = wide_list(argv);
    let line = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
    let env = (envp != 0).then(|| wide_list(envp));
    let Some(process) = start(Some(&app), &line, env.as_deref()) else {
        return -1;
    };
    match mode {
        P_WAIT => {
            let code = wait_exit(process);
            crate::dlls::kernel32::close_handle(process);
            code as isize
        }
        P_OVERLAY => {
            let code = wait_exit(process);
            crate::dlls::kernel32::flush_and_exit(code)
        }
        P_DETACH => {
            crate::dlls::kernel32::close_handle(process);
            0
        }
        P_NOWAIT | P_NOWAITO => process as isize,
        _ => {
            *errno() = 22;
            -1
        }
    }
}

pub unsafe extern "win64" fn wspawnv(mode: i32, path: *const u16, argv: usize) -> isize {
    spawn(mode, path, argv, 0)
}

pub unsafe extern "win64" fn wspawnve(
    mode: i32,
    path: *const u16,
    argv: usize,
    envp: usize,
) -> isize {
    spawn(mode, path, argv, envp)
}

/// `_wexecv` : le programme est remplacé ; ici on attend l'enfant et on
/// sort avec son code.
pub unsafe extern "win64" fn wexecv(path: *const u16, argv: usize) -> isize {
    spawn(P_OVERLAY, path, argv, 0)
}

pub unsafe extern "win64" fn wexecve(path: *const u16, argv: usize, envp: usize) -> isize {
    spawn(P_OVERLAY, path, argv, envp)
}

/// `_cwait(&code, handle, action)` : attend un processus lancé en P_NOWAIT.
pub unsafe extern "win64" fn cwait(termstat: *mut i32, process: isize, _action: i32) -> isize {
    if !matches!(
        crate::dlls::kernel32::lookup_sync(process as usize),
        Some(crate::dlls::kernel32::SyncHandle::Process(_))
    ) {
        *errno() = 10; // ECHILD
        return -1;
    }
    let code = wait_exit(process as usize);
    if !termstat.is_null() {
        *termstat = code;
    }
    process
}

/// `_wsystem(commande)` : la commande passe à `cmd.exe /c`. Sans commande,
/// rend non nul si un interpréteur existe.
pub unsafe extern "win64" fn wsystem(command: *const u16) -> i32 {
    if command.is_null() {
        // Sonder l'interpréteur : `cmd.exe /c exit 0`.
        return match start(None, "cmd.exe /c exit 0", None) {
            Some(p) => {
                wait_exit(p);
                crate::dlls::kernel32::close_handle(p);
                1
            }
            None => 0,
        };
    }
    let line = format!("cmd.exe /c {}", super::wstr(command));
    match start(None, &line, None) {
        Some(p) => {
            let code = wait_exit(p);
            crate::dlls::kernel32::close_handle(p);
            code
        }
        None => -1,
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "__fpe_flt_rounds" => fn fpe_flt_rounds, "__control87_2" => fn control87_2,
        "_putch" => fn putch, "_putwch" => fn putwch, "_getwch" => fn getwch,
        "_getche" => fn getche, "_getwche" => fn getwche,
        "_ungetch" => fn ungetch, "_ungetwch" => fn ungetwch,
        "_chsize_s" => fn chsize_s, "_umask" => fn umask, "_heapmin" => fn heapmin,
        "_resetstkoflw" => fn resetstkoflw, "_wputenv_s" => fn wputenv_s, "wcstok_s" => fn wcstok_s,
        "_wspawnv" => fn wspawnv, "_wspawnve" => fn wspawnve,
        "_wexecv" => fn wexecv, "_wexecve" => fn wexecve,
        "_cwait" => fn cwait, "_wsystem" => fn wsystem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrondi_flottant_selon_le_mot_de_controle() {
        // Mot par défaut : arrondi au plus proche (FLT_ROUNDS = 1).
        assert_eq!(fpe_flt_rounds(), 1);
        let mut x86 = 0;
        let mut sse = 0;
        assert_eq!(unsafe { control87_2(0, 0, &mut x86, &mut sse) }, 1);
        assert_eq!(x86, sse);
    }

    #[test]
    fn citation_des_arguments() {
        assert_eq!(quote("simple"), "simple");
        assert_eq!(quote("deux mots"), "\"deux mots\"");
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("C:\\dir\\"), "C:\\dir\\");
        assert_eq!(quote("C:\\mon dir\\"), "\"C:\\mon dir\\\\\"");
    }

    #[test]
    fn decoupage_wcstok_s() {
        let mut s: Vec<u16> = "a,,bc;d".encode_utf16().chain([0]).collect();
        let delim: Vec<u16> = ",;".encode_utf16().chain([0]).collect();
        let mut ctx: usize = 0;
        let ctx_at = &mut ctx as *mut usize as usize;
        let mut got = Vec::new();
        unsafe {
            let mut t = wcstok_s(s.as_mut_ptr(), delim.as_ptr(), ctx_at);
            while !t.is_null() {
                got.push(super::super::wstr(t));
                t = wcstok_s(std::ptr::null_mut(), delim.as_ptr(), ctx_at);
            }
        }
        assert_eq!(got, ["a", "bc", "d"]);
    }

    #[test]
    fn chsize_umask_et_console() {
        let path = std::env::temp_dir().join(format!("winlux-chsize-{}", std::process::id()));
        let f = std::fs::File::create(&path).unwrap();
        use std::os::fd::AsRawFd;
        assert_eq!(unsafe { chsize_s(f.as_raw_fd(), 1234) }, 0);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 1234);
        assert_eq!(unsafe { chsize_s(-1, 10) }, 9);
        drop(f);
        let _ = std::fs::remove_file(&path);
        let old = umask(0x80);
        assert_eq!(umask(old), 0x80);
        assert_eq!(ungetwch(0x00E9), 0x00E9);
        assert_eq!(unsafe { getwch() }, 0x00E9);
        assert_eq!(heapmin(), 0);
        assert_eq!(resetstkoflw(), 1);
    }
}
