//! Fonctions Win32 demandées par de vrais jeux et leurs bibliothèques
//! (SDL2/SDL3, OpenAL, libcurl, ANGLE, crashpad, libwinpthread), relevées
//! sur SuperTuxKart 1.5 et Warzone 2100 4.7 : voir docs/VRAIS-JEUX.md.
//!
//! Elles complètent kernel32, user32, gdi32, setupapi, advapi32, crypt32,
//! ws2_32, dbghelp, shlwapi, shell32, iphlpapi, winhttp, powrprof et rpcrt4
//! (seulement pour les noms absents), et fournissent hid.dll et dnsapi.dll.

#![allow(non_snake_case, clippy::missing_safety_doc)]

use crate::dlls::{lock, Exports};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

fn set_error(e: u32) {
    crate::process::set_last_error(e);
}
fn is_current_process(h: usize) -> bool {
    h == usize::MAX || h == 0 || crate::dlls::kernel32::is_current_process_handle(h)
}
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x7A10_0000);
fn fake_handle() -> usize {
    NEXT_HANDLE.fetch_add(4, Ordering::Relaxed)
}

// =====================================================================
// kernel32
// =====================================================================

unsafe extern "win64" fn CompareFileTime(a: *const u64, b: *const u64) -> i32 {
    let (x, y) = (std::ptr::read_unaligned(a), std::ptr::read_unaligned(b));
    x.cmp(&y) as i32
}

const LOCALES: [&str; 8] = [
    "00000409", "0000040C", "00000407", "00000410", "00000C0A", "00000816", "00000413", "00000809",
];
unsafe extern "win64" fn EnumSystemLocalesA(cb: usize, _flags: u32) -> i32 {
    if cb == 0 {
        set_error(87);
        return 0;
    }
    let f: extern "win64" fn(*const u8) -> i32 = std::mem::transmute(cb);
    for l in LOCALES {
        let c = format!("{l}\0");
        if f(c.as_ptr()) == 0 {
            break;
        }
    }
    1
}
unsafe extern "win64" fn EnumSystemLocalesW(cb: usize, _flags: u32) -> i32 {
    if cb == 0 {
        set_error(87);
        return 0;
    }
    let f: extern "win64" fn(*const u16) -> i32 = std::mem::transmute(cb);
    for l in LOCALES {
        let w: Vec<u16> = l.encode_utf16().chain([0]).collect();
        if f(w.as_ptr()) == 0 {
            break;
        }
    }
    1
}
/// Identifiant du thread d'un handle : le thread courant pour le
/// pseudo-handle, sinon celui qu'enregistre kernel32.
extern "win64" fn GetThreadId(h: usize) -> u32 {
    if h == usize::MAX - 1 || h == 0 {
        return crate::dlls::kernel32::get_current_thread_id();
    }
    match crate::dlls::kernel32::thread_id_of_handle(h) {
        Some(t) => t,
        None => {
            set_error(6);
            0
        }
    }
}
unsafe extern "win64" fn K32EnumProcessModulesEx(
    p: usize,
    m: *mut usize,
    cb: u32,
    needed: *mut u32,
    _filter: u32,
) -> i32 {
    crate::dlls::kernel32_node::K32EnumProcessModules(p, m, cb, needed)
}
/// TH32CS_SNAPMODULE n'est pas fourni par CreateToolhelp32Snapshot : la liste est vide.
extern "win64" fn Module32FirstW(_snap: usize, _entry: *mut c_void) -> i32 {
    set_error(18);
    0
}
/// Read/WriteProcessMemory dans un autre processus (#236) : par
/// /proc/<pid>/mem, qui comme Windows écrit aussi dans une page protégée
/// en écriture (code, données en lecture seule). Linux l'autorise envers un
/// descendant (le bac à sable de Chromium et son enfant suspendu) ou un
/// processus qui nous a désignés (crashpad, voir `allow_crash_handler`).
unsafe fn foreign_memory(
    p: usize,
    addr: usize,
    buf: *mut u8,
    n: usize,
    done: *mut usize,
    write: bool,
) -> i32 {
    use std::os::unix::fs::FileExt;
    if !done.is_null() {
        *done = 0;
    }
    let Some(pid) = crate::dlls::kernel32::process_os_pid(p) else {
        set_error(6);
        return 0;
    };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(write)
        .open(format!("/proc/{pid}/mem"));
    let file = match file {
        Ok(f) => f,
        Err(e) => {
            crate::trace!("mémoire du processus {pid} inaccessible : {e}");
            set_error(if e.kind() == std::io::ErrorKind::NotFound {
                6
            } else {
                5
            });
            return 0;
        }
    };
    let slice = std::slice::from_raw_parts_mut(buf, n);
    let mut total = 0usize;
    while total < n {
        let r = if write {
            file.write_at(&slice[total..], (addr + total) as u64)
        } else {
            file.read_at(&mut slice[total..], (addr + total) as u64)
        };
        match r {
            Ok(0) | Err(_) => break,
            Ok(k) => total += k,
        }
    }
    if !done.is_null() {
        *done = total;
    }
    crate::trace!(
        "{} du processus {pid} à {addr:#x} : {total}/{n} octets",
        if write { "écriture" } else { "lecture" }
    );
    if total == n {
        1
    } else {
        set_error(299); // ERROR_PARTIAL_COPY
        0
    }
}
unsafe extern "win64" fn ReadProcessMemory(
    p: usize,
    addr: *const u8,
    buf: *mut u8,
    n: usize,
    done: *mut usize,
) -> i32 {
    if !is_current_process(p) {
        return foreign_memory(p, addr as usize, buf, n, done, false);
    }
    // process_vm_readv refuse proprement une adresse non lisible.
    let local = libc::iovec {
        iov_base: buf.cast(),
        iov_len: n,
    };
    let remote = libc::iovec {
        iov_base: addr as *mut c_void,
        iov_len: n,
    };
    let r = libc::process_vm_readv(libc::getpid(), &local, 1, &remote, 1, 0);
    if !done.is_null() {
        *done = r.max(0) as usize;
    }
    if r as usize == n {
        1
    } else {
        set_error(299); // ERROR_PARTIAL_COPY
        0
    }
}
unsafe extern "win64" fn WriteProcessMemory(
    p: usize,
    addr: *mut u8,
    buf: *const u8,
    n: usize,
    done: *mut usize,
) -> i32 {
    if !is_current_process(p) {
        return foreign_memory(p, addr as usize, buf as *mut u8, n, done, true);
    }
    let local = libc::iovec {
        iov_base: buf as *mut c_void,
        iov_len: n,
    };
    let remote = libc::iovec {
        iov_base: addr.cast(),
        iov_len: n,
    };
    let r = libc::process_vm_writev(libc::getpid(), &local, 1, &remote, 1, 0);
    if r as usize == n {
        if !done.is_null() {
            *done = n;
        }
        return 1;
    }
    // Page protégée en écriture (code) : WriteProcessMemory la débloque.
    let mut old = 0u32;
    if crate::dlls::kernel32::virtual_protect(addr.cast(), n, 0x40, &mut old) != 0 {
        std::ptr::copy(buf, addr, n);
        let mut tmp = 0u32;
        crate::dlls::kernel32::virtual_protect(addr.cast(), n, old, &mut tmp);
        if !done.is_null() {
            *done = n;
        }
        return 1;
    }
    set_error(299);
    0
}
extern "win64" fn ok1() -> i32 {
    1
}
extern "win64" fn not_supported() -> i32 {
    set_error(50);
    0
}
extern "win64" fn privilege_not_held() -> i32 {
    set_error(1314);
    0
}
/// VirtualAllocEx dans un enfant encore suspendu (bac à sable de Chromium,
/// #767) ; refusé pour tout autre processus.
unsafe fn foreign_alloc(p: usize, addr: usize, size: usize, kind: u32, prot: u32) -> *mut c_void {
    use crate::dlls::{kernel32 as k, remote_vm};
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    if size == 0 || !k::valid_page_prot(prot) || kind & (MEM_COMMIT | MEM_RESERVE) == 0 {
        set_error(87);
        return std::ptr::null_mut();
    }
    let page = 4096usize;
    let r = k::with_suspended_child(p, |pid| {
        if kind & MEM_RESERVE == 0 && addr != 0 {
            // Pages déjà réservées : seule la protection change.
            let base = addr & !(page - 1);
            let end = (addr + size + page - 1) & !(page - 1);
            remote_vm::protect(pid, base, end - base, k::linux_prot(prot)).map(|()| base)
        } else {
            let base = addr & !0xffff;
            let len = (size + (addr - base) + page - 1) & !(page - 1);
            // MEM_RESERVE seul : pages inaccessibles jusqu'au MEM_COMMIT.
            let lp = if kind & MEM_COMMIT == 0 {
                libc::PROT_NONE
            } else {
                k::linux_prot(prot)
            };
            remote_vm::map(pid, base, len, lp)
        }
    });
    match r {
        Some(Ok(a)) => {
            crate::trace!("VirtualAllocEx dans l'enfant suspendu : {a:#x} ({size} octets)");
            a as *mut c_void
        }
        Some(Err(code)) => {
            set_error(code);
            std::ptr::null_mut()
        }
        None => {
            set_error(5);
            std::ptr::null_mut()
        }
    }
}
unsafe fn foreign_protect(p: usize, addr: usize, size: usize, prot: u32, old: *mut u32) -> i32 {
    use crate::dlls::{kernel32 as k, remote_vm};
    if size == 0 || !k::valid_page_prot(prot) {
        set_error(87);
        return 0;
    }
    let page = 4096usize;
    let base = addr & !(page - 1);
    let end = (addr + size + page - 1) & !(page - 1);
    let r = k::with_suspended_child(p, |pid| {
        let before = remote_vm::current_prot(pid, base);
        remote_vm::protect(pid, base, end - base, k::linux_prot(prot)).map(|()| before)
    });
    match r {
        Some(Ok(before)) => {
            if !old.is_null() {
                *old = before.map_or(0x01, remote_vm::win_prot_of);
            }
            1
        }
        Some(Err(code)) => {
            set_error(code);
            0
        }
        None => {
            set_error(5);
            0
        }
    }
}
/// VirtualFreeEx(MEM_RELEASE) dans un enfant encore suspendu (#767).
pub(crate) unsafe fn foreign_free(p: usize, addr: usize, size: usize, kind: u32) -> Option<i32> {
    use crate::dlls::remote_vm;
    const MEM_DECOMMIT: u32 = 0x4000;
    const MEM_RELEASE: u32 = 0x8000;
    crate::dlls::kernel32::with_suspended_child(p, |pid| {
        let r = if kind == MEM_RELEASE {
            // Taille de la région : celle de la projection dans /proc/<pid>/maps.
            remote_vm::region_len(pid, addr)
                .map_or(Err(487), |len| remote_vm::unmap(pid, addr, len))
        } else if kind == MEM_DECOMMIT && size != 0 {
            remote_vm::protect(pid, addr & !4095, (size + 4095) & !4095, libc::PROT_NONE)
        } else {
            Err(87)
        };
        match r {
            Ok(()) => 1,
            Err(code) => {
                set_error(code);
                0
            }
        }
    })
}
unsafe extern "win64" fn VirtualAllocEx(
    p: usize,
    addr: *mut c_void,
    size: usize,
    kind: u32,
    prot: u32,
) -> *mut c_void {
    if !is_current_process(p) {
        return foreign_alloc(p, addr as usize, size, kind, prot);
    }
    crate::dlls::kernel32::virtual_alloc(addr, size, kind, prot)
}
unsafe extern "win64" fn VirtualProtectEx(
    p: usize,
    addr: *mut c_void,
    size: usize,
    prot: u32,
    old: *mut u32,
) -> i32 {
    if !is_current_process(p) {
        return foreign_protect(p, addr as usize, size, prot, old);
    }
    crate::dlls::kernel32::virtual_protect(addr, size, prot, old)
}
unsafe extern "win64" fn VirtualLock(addr: *mut c_void, size: usize) -> i32 {
    // mlock peut être refusé (limite RLIMIT_MEMLOCK) : la mémoire reste utilisable.
    let _ = libc::mlock(addr, size);
    1
}
unsafe extern "win64" fn VirtualUnlock(addr: *mut c_void, size: usize) -> i32 {
    let _ = libc::munlock(addr, size);
    1
}
unsafe extern "win64" fn CopyFileExW(
    src: *const u16,
    dst: *const u16,
    _progress: usize,
    _data: usize,
    cancel: *const i32,
    flags: u32,
) -> i32 {
    if !cancel.is_null() && *cancel != 0 {
        set_error(1235); // ERROR_REQUEST_ABORTED
        return 0;
    }
    crate::dlls::kernel32::copy_file_w(src, dst, (flags & 1) as i32)
}
unsafe extern "win64" fn GetProcessShutdownParameters(level: *mut u32, flags: *mut u32) -> i32 {
    if !level.is_null() {
        *level = 0x280;
    }
    if !flags.is_null() {
        *flags = 0;
    }
    1
}
/// Pas d'ajustement d'horloge : incrément de 15,625 ms, comme Windows.
unsafe extern "win64" fn GetSystemTimeAdjustment(
    adj: *mut u32,
    inc: *mut u32,
    disabled: *mut i32,
) -> i32 {
    if !adj.is_null() {
        *adj = 156_250;
    }
    if !inc.is_null() {
        *inc = 156_250;
    }
    if !disabled.is_null() {
        *disabled = 1;
    }
    1
}

pub fn kernel32_exports() -> Exports {
    crate::exports! {
        "CompareFileTime" => fn CompareFileTime,
        "EnumSystemLocalesA" => fn EnumSystemLocalesA, "EnumSystemLocalesW" => fn EnumSystemLocalesW,
        "GetThreadId" => fn GetThreadId,
        "K32EnumProcessModulesEx" => fn K32EnumProcessModulesEx, "EnumProcessModulesEx" => fn K32EnumProcessModulesEx,
        "Module32FirstW" => fn Module32FirstW, "Module32NextW" => fn Module32FirstW,
        "Module32First" => fn Module32FirstW, "Module32Next" => fn Module32FirstW,
        "ReadProcessMemory" => fn ReadProcessMemory, "WriteProcessMemory" => fn WriteProcessMemory,
        "SetConsoleScreenBufferSize" => fn ok1,
        "VirtualAllocEx" => fn VirtualAllocEx, "VirtualProtectEx" => fn VirtualProtectEx,
        "VirtualLock" => fn VirtualLock, "VirtualUnlock" => fn VirtualUnlock,
        "CopyFileExW" => fn CopyFileExW,
        "SetProcessShutdownParameters" => fn ok1, "GetProcessShutdownParameters" => fn GetProcessShutdownParameters,
        "Wow64GetThreadContext" => fn not_supported, "Wow64SetThreadContext" => fn not_supported,
        "SetThreadContext" => fn crate::dlls::kernel32::set_thread_context,
        "GetSystemTimeAdjustment" => fn GetSystemTimeAdjustment,
        "SetSystemTime" => fn privilege_not_held, "SetLocalTime" => fn privilege_not_held,
        "SetSystemTimeAdjustment" => fn privilege_not_held,
    }
}

// =====================================================================
// user32 / gdi32
// =====================================================================

use crate::dlls::gui::Rect;

unsafe fn rect(p: *const Rect) -> Option<Rect> {
    (!p.is_null()).then(|| std::ptr::read_unaligned(p))
}
unsafe fn put_rect(p: *mut Rect, r: Rect) -> i32 {
    if p.is_null() {
        return 0;
    }
    std::ptr::write_unaligned(p, r);
    1
}
fn empty(r: &Rect) -> bool {
    r.right <= r.left || r.bottom <= r.top
}
unsafe extern "win64" fn IntersectRect(out: *mut Rect, a: *const Rect, b: *const Rect) -> i32 {
    let (Some(a), Some(b)) = (rect(a), rect(b)) else {
        return 0;
    };
    let r = Rect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    if empty(&r) {
        put_rect(out, Rect::default());
        0
    } else {
        put_rect(out, r)
    }
}
unsafe extern "win64" fn UnionRect(out: *mut Rect, a: *const Rect, b: *const Rect) -> i32 {
    let (Some(a), Some(b)) = (rect(a), rect(b)) else {
        return 0;
    };
    let r = match (empty(&a), empty(&b)) {
        (true, true) => Rect::default(),
        (true, false) => b,
        (false, true) => a,
        _ => Rect {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        },
    };
    put_rect(out, r);
    (!empty(&r)) as i32
}
unsafe extern "win64" fn SetRectEmpty(r: *mut Rect) -> i32 {
    put_rect(r, Rect::default())
}
unsafe extern "win64" fn SetRect(r: *mut Rect, l: i32, t: i32, rr: i32, b: i32) -> i32 {
    put_rect(
        r,
        Rect {
            left: l,
            top: t,
            right: rr,
            bottom: b,
        },
    )
}
unsafe extern "win64" fn CopyRect(d: *mut Rect, s: *const Rect) -> i32 {
    match rect(s) {
        Some(r) => put_rect(d, r),
        None => 0,
    }
}
unsafe extern "win64" fn IsRectEmpty(r: *const Rect) -> i32 {
    rect(r).is_none_or(|r| empty(&r)) as i32
}
unsafe extern "win64" fn EqualRect(a: *const Rect, b: *const Rect) -> i32 {
    matches!((rect(a), rect(b)), (Some(x), Some(y)) if x == y) as i32
}
unsafe extern "win64" fn InflateRect(r: *mut Rect, dx: i32, dy: i32) -> i32 {
    match rect(r) {
        Some(x) => put_rect(
            r,
            Rect {
                left: x.left - dx,
                top: x.top - dy,
                right: x.right + dx,
                bottom: x.bottom + dy,
            },
        ),
        None => 0,
    }
}
unsafe extern "win64" fn SubtractRect(out: *mut Rect, a: *const Rect, b: *const Rect) -> i32 {
    let (Some(mut r), Some(s)) = (rect(a), rect(b)) else {
        return 0;
    };
    // Le résultat n'est réduit que si `s` couvre tout un côté de `r`.
    if s.top <= r.top && s.bottom >= r.bottom {
        if s.left <= r.left && s.right > r.left {
            r.left = s.right.min(r.right);
        } else if s.right >= r.right && s.left < r.right {
            r.right = s.left.max(r.left);
        }
    } else if s.left <= r.left && s.right >= r.right {
        if s.top <= r.top && s.bottom > r.top {
            r.top = s.bottom.min(r.bottom);
        } else if s.bottom >= r.bottom && s.top < r.bottom {
            r.bottom = s.top.max(r.top);
        }
    }
    put_rect(out, r);
    (!empty(&r)) as i32
}

/// DISP_CHANGE_SUCCESSFUL : le jeu garde la résolution du bureau, la
/// fenêtre plein écran est mise à l'échelle par le compositeur.
extern "win64" fn ChangeDisplaySettingsEx(
    _name: usize,
    _mode: usize,
    _hwnd: usize,
    _flags: u32,
    _param: usize,
) -> i32 {
    0
}
extern "win64" fn ChangeDisplaySettings(_mode: usize, _flags: u32) -> i32 {
    0
}
extern "win64" fn CopyImage(h: usize, _ty: u32, _cx: i32, _cy: i32, _flags: u32) -> usize {
    h
}
extern "win64" fn new_icon() -> usize {
    fake_handle()
}
unsafe extern "win64" fn GetClipCursor(r: *mut Rect) -> i32 {
    let w = crate::dlls::user32::get_system_metrics(0);
    let h = crate::dlls::user32::get_system_metrics(1);
    put_rect(
        r,
        Rect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        },
    )
}
extern "win64" fn zero() -> usize {
    0
}
/// Entrée brute : aucun périphérique n'est déclaré, WM_INPUT n'arrive jamais ;
/// SDL se replace alors sur les messages souris/clavier classiques.
unsafe extern "win64" fn GetRawInputDeviceList(
    list: *mut c_void,
    num: *mut u32,
    _size: u32,
) -> u32 {
    if !num.is_null() {
        if list.is_null() {
            *num = 0;
            return 0;
        }
        *num = 0;
    }
    0
}
extern "win64" fn GetRawInputDeviceInfo(
    _dev: usize,
    _cmd: u32,
    _data: *mut c_void,
    _size: *mut u32,
) -> u32 {
    set_error(6);
    u32::MAX
}
unsafe extern "win64" fn GetRawInputData(
    _h: usize,
    _cmd: u32,
    _data: *mut c_void,
    size: *mut u32,
    _hdr: u32,
) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    set_error(6);
    u32::MAX
}
unsafe extern "win64" fn GetRawInputBuffer(_data: *mut c_void, size: *mut u32, _hdr: u32) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    0
}
unsafe extern "win64" fn GetRegisteredRawInputDevices(
    _list: *mut c_void,
    num: *mut u32,
    _size: u32,
) -> u32 {
    if !num.is_null() {
        *num = 0;
    }
    0
}
unsafe extern "win64" fn PostThreadMessage(
    tid: u32,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> i32 {
    let Some(thread) = crate::dlls::gui::thread_of_win32(tid) else {
        set_error(1444); // ERROR_INVALID_THREAD_ID
        return 0;
    };
    let pt = crate::dlls::gui::Point::default();
    let m = crate::dlls::gui::Msg {
        hwnd: 0,
        message,
        wparam,
        lparam,
        time: 0,
        pt,
        l_private: 0,
    };
    crate::dlls::gui::post_to_thread(thread, m) as i32
}
unsafe extern "win64" fn wvsprintfA(dst: *mut u8, fmt: *const u8, va: *const u64) -> i32 {
    if dst.is_null() || fmt.is_null() {
        return 0;
    }
    let mut v = crate::thunks::VaList::new(va);
    let mut b = crate::dlls::crt_stdio::format_narrow(fmt, &mut v);
    b.truncate(1023);
    std::ptr::copy_nonoverlapping(b.as_ptr(), dst, b.len());
    *dst.add(b.len()) = 0;
    b.len() as i32
}
unsafe extern "win64" fn wvsprintfW(dst: *mut u16, fmt: *const u16, va: *const u64) -> i32 {
    if dst.is_null() || fmt.is_null() {
        return 0;
    }
    let mut v = crate::thunks::VaList::new(va);
    let mut w = crate::dlls::crt::format_wide(fmt, &mut v);
    w.truncate(1023);
    std::ptr::copy_nonoverlapping(w.as_ptr(), dst, w.len());
    *dst.add(w.len()) = 0;
    w.len() as i32
}
extern "win64" fn GetShellWindow() -> usize {
    crate::dlls::user32::get_desktop_window()
}
/// ANGLE (libEGL) retrouve la fenêtre d'un HDC de GetDC, et refuse un
/// affichage dont le DC d'écran ne rend aucune fenêtre (#996 : Chromium lui
/// passe GetDC(NULL), qui désigne le bureau).
extern "win64" fn WindowFromDC(hdc: usize) -> usize {
    crate::dlls::gui::window_from_dc(hdc)
}

pub fn user32_exports() -> Exports {
    crate::exports! {
        "AttachThreadInput" => fn ok1,
        "ChangeDisplaySettingsExW" => fn ChangeDisplaySettingsEx, "ChangeDisplaySettingsExA" => fn ChangeDisplaySettingsEx,
        "ChangeDisplaySettingsW" => fn ChangeDisplaySettings, "ChangeDisplaySettingsA" => fn ChangeDisplaySettings,
        "CopyImage" => fn CopyImage,
        "CreateIconFromResource" => fn new_icon, "CreateIconFromResourceEx" => fn new_icon, "CreateIconIndirect" => fn new_icon,
        "CreateIcon" => fn new_icon, "CreateCursor" => fn new_icon,
        "GetClipCursor" => fn GetClipCursor,
        "RegisterRawInputDevices" => fn ok1, "GetRegisteredRawInputDevices" => fn GetRegisteredRawInputDevices,
        "GetRawInputDeviceList" => fn GetRawInputDeviceList,
        "GetRawInputDeviceInfoA" => fn GetRawInputDeviceInfo, "GetRawInputDeviceInfoW" => fn GetRawInputDeviceInfo,
        "GetRawInputData" => fn GetRawInputData, "GetRawInputBuffer" => fn GetRawInputBuffer,
        "DefRawInputProc" => fn zero,
        "IntersectRect" => fn IntersectRect, "UnionRect" => fn UnionRect, "SetRectEmpty" => fn SetRectEmpty,
        "SetRect" => fn SetRect, "CopyRect" => fn CopyRect, "IsRectEmpty" => fn IsRectEmpty,
        "EqualRect" => fn EqualRect, "InflateRect" => fn InflateRect, "SubtractRect" => fn SubtractRect,
        "PostThreadMessageW" => fn PostThreadMessage, "PostThreadMessageA" => fn PostThreadMessage,
        "RegisterDeviceNotificationW" => fn crate::dlls::devnotify::RegisterDeviceNotificationW,
        "RegisterDeviceNotificationA" => fn crate::dlls::devnotify::RegisterDeviceNotificationA,
        "UnregisterDeviceNotification" => fn crate::dlls::devnotify::UnregisterDeviceNotification,
        "SetWindowRgn" => fn ok1,
        // SetWindowRgn est sans effet : aucune fenêtre n'a de région, donc
        // GetWindowRgn(Box) répond ERROR (0), comme Windows dans ce cas (#747).
        "GetWindowRgn" => fn zero, "GetWindowRgnBox" => fn zero,
        "wvsprintfA" => fn wvsprintfA, "wvsprintfW" => fn wvsprintfW,
        "GetShellWindow" => fn GetShellWindow, "WindowFromDC" => fn WindowFromDC,
    }
}

/// Rampe gamma et profil de couleur : non gérés (SDL le tolère).
extern "win64" fn gamma_unsupported() -> i32 {
    0
}
pub fn gdi32_exports() -> Exports {
    crate::exports! {
        "GetDeviceGammaRamp" => fn gamma_unsupported, "SetDeviceGammaRamp" => fn gamma_unsupported,
        "GetICMProfileW" => fn gamma_unsupported, "GetICMProfileA" => fn gamma_unsupported,
    }
}

// =====================================================================
// setupapi, cfgmgr32, hid
// =====================================================================

/// Variantes ANSI de setupapi (SDL2) : appel de la version W, chemins
/// et chaînes convertis dans la page ANSI.
unsafe extern "win64" fn SetupDiGetDeviceInterfaceDetailA(
    set: usize,
    iface: *const u8,
    detail: *mut u8,
    size: u32,
    required: *mut u32,
    devinfo: *mut u8,
) -> i32 {
    let mut need = 0u32;
    crate::dlls::setupapi::SetupDiGetDeviceInterfaceDetailW(
        set,
        iface,
        std::ptr::null_mut(),
        0,
        &mut need,
        std::ptr::null_mut(),
    );
    if need < 8 {
        return 0;
    }
    let mut wide = vec![0u8; need as usize + 8];
    std::ptr::write_unaligned(wide.as_mut_ptr() as *mut u32, 8);
    if crate::dlls::setupapi::SetupDiGetDeviceInterfaceDetailW(
        set,
        iface,
        wide.as_mut_ptr(),
        wide.len() as u32,
        &mut need,
        devinfo,
    ) == 0
    {
        return 0;
    }
    let units: Vec<u16> = wide[4..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&c| c != 0)
        .collect();
    let mut path = crate::dlls::encode_ansi(&String::from_utf16_lossy(&units));
    path.push(0);
    let total = 4 + path.len() as u32;
    if !required.is_null() {
        *required = total.max(5);
    }
    if detail.is_null() || size < total {
        set_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(path.as_ptr(), detail.add(4), path.len());
    1
}
unsafe extern "win64" fn SetupDiGetDeviceRegistryPropertyA(
    set: usize,
    data: *const u8,
    prop: u32,
    kind: *mut u32,
    buf: *mut u8,
    size: u32,
    required: *mut u32,
) -> i32 {
    let mut need = 0u32;
    let mut t = 0u32;
    crate::dlls::setupapi::SetupDiGetDeviceRegistryPropertyW(
        set,
        data,
        prop,
        &mut t,
        std::ptr::null_mut(),
        0,
        &mut need,
    );
    if need == 0 {
        return 0;
    }
    let mut wide = vec![0u8; need as usize];
    if crate::dlls::setupapi::SetupDiGetDeviceRegistryPropertyW(
        set,
        data,
        prop,
        &mut t,
        wide.as_mut_ptr(),
        need,
        &mut need,
    ) == 0
    {
        return 0;
    }
    let units: Vec<u16> = wide
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    // Chaînes (REG_SZ, REG_MULTI_SZ) converties ; données binaires recopiées.
    let out: Vec<u8> = if matches!(t, 1 | 2 | 7) {
        crate::dlls::encode_ansi(&String::from_utf16_lossy(&units))
    } else {
        wide.clone()
    };
    if !kind.is_null() {
        *kind = t;
    }
    if !required.is_null() {
        *required = out.len() as u32;
    }
    if buf.is_null() || size < out.len() as u32 {
        set_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(out.as_ptr(), buf, out.len());
    1
}
extern "win64" fn no_more_items() -> i32 {
    set_error(259);
    0
}
/// CR_NO_SUCH_DEVNODE : pas d'arbre de périphériques Plug and Play.
extern "win64" fn cr_no_such_devnode() -> u32 {
    0x0D
}
pub fn setupapi_exports() -> Exports {
    crate::exports! {
        "SetupDiGetDeviceInterfaceDetailA" => fn SetupDiGetDeviceInterfaceDetailA,
        "SetupDiGetDeviceRegistryPropertyA" => fn SetupDiGetDeviceRegistryPropertyA,
        "SetupDiGetDeviceInstanceIdA" => fn no_more_items, "SetupDiGetDeviceInstanceIdW" => fn no_more_items,
        "SetupDiOpenDevRegKey" => fn no_more_items,
        "CM_Get_Device_IDA" => fn cr_no_such_devnode, "CM_Get_Device_IDW" => fn cr_no_such_devnode,
        "CM_Get_Parent" => fn cr_no_such_devnode, "CM_Get_Child" => fn cr_no_such_devnode, "CM_Get_Sibling" => fn cr_no_such_devnode,
        "CM_Locate_DevNodeA" => fn cr_no_such_devnode, "CM_Locate_DevNodeW" => fn cr_no_such_devnode,
        "CM_Get_DevNode_Status" => fn cr_no_such_devnode,
    }
}

/// Plus aucun complément : hid.rs et hid_report.rs servent tout hid.dll (#1523).
pub fn hid_exports() -> Exports {
    crate::exports! {}
}

// =====================================================================
// dnsapi : requêtes DNS (SRV, TXT, A…) par le résolveur de glibc
// =====================================================================

mod resolv {
    extern "C" {
        pub fn res_query(
            name: *const libc::c_char,
            class: i32,
            ty: i32,
            answer: *mut u8,
            len: i32,
        ) -> i32;
    }
}

/// Lit un nom DNS compressé à `pos` ; renvoie le nom et la position suivante.
fn dns_name(msg: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    let mut jumps = 0;
    loop {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            let target = ((len & 0x3F) << 8) | *msg.get(pos + 1)? as usize;
            if end.is_none() {
                end = Some(pos + 2);
            }
            jumps += 1;
            if jumps > 32 {
                return None;
            }
            pos = target;
            continue;
        }
        labels.push(String::from_utf8_lossy(msg.get(pos + 1..pos + 1 + len)?).to_string());
        pos += 1 + len;
    }
    Some((labels.join("."), end.unwrap_or(pos)))
}

struct DnsRecord {
    name: String,
    ty: u16,
    ttl: u32,
    data: Vec<u8>,
    rdata_off: usize,
}
fn parse_answers(msg: &[u8]) -> Vec<DnsRecord> {
    let mut out = Vec::new();
    if msg.len() < 12 {
        return out;
    }
    let qd = u16::from_be_bytes([msg[4], msg[5]]) as usize;
    let an = u16::from_be_bytes([msg[6], msg[7]]) as usize;
    let mut pos = 12;
    for _ in 0..qd {
        let Some((_, p)) = dns_name(msg, pos) else {
            return out;
        };
        pos = p + 4;
    }
    for _ in 0..an {
        let Some((name, p)) = dns_name(msg, pos) else {
            break;
        };
        let Some(h) = msg.get(p..p + 10) else { break };
        let ty = u16::from_be_bytes([h[0], h[1]]);
        let ttl = u32::from_be_bytes([h[4], h[5], h[6], h[7]]);
        let len = u16::from_be_bytes([h[8], h[9]]) as usize;
        let Some(data) = msg.get(p + 10..p + 10 + len) else {
            break;
        };
        out.push(DnsRecord {
            name,
            ty,
            ttl,
            data: data.to_vec(),
            rdata_off: p + 10,
        });
        pos = p + 10 + len;
    }
    out
}

/// Bloc alloué pour un enregistrement DNS_RECORDW (x64) : pNext 0, pName 8,
/// wType 16, wDataLength 18, Flags 20, dwTtl 24, dwReserved 28, Data 32.
unsafe fn build_record(msg: &[u8], r: &DnsRecord) -> Option<*mut u8> {
    let wide = |s: &str| -> *mut u16 {
        let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
        let p = libc::malloc(w.len() * 2) as *mut u16;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        }
        p
    };
    let (data, data_len): (Vec<u8>, u16) = match r.ty {
        1 if r.data.len() == 4 => (r.data.clone(), 4), // A : IP4_ADDRESS
        28 if r.data.len() == 16 => (r.data.clone(), 16), // AAAA
        5 | 12 | 2 => {
            // CNAME, PTR, NS : pointeur vers le nom.
            let (n, _) = dns_name(msg, r.rdata_off)?;
            ((wide(&n) as usize).to_le_bytes().to_vec(), 8)
        }
        33 if r.data.len() >= 7 => {
            // SRV : pNameTarget, wPriority, wWeight, wPort, Pad.
            let (n, _) = dns_name(msg, r.rdata_off + 6)?;
            let mut d = (wide(&n) as usize).to_le_bytes().to_vec();
            d.extend_from_slice(&u16::from_be_bytes([r.data[0], r.data[1]]).to_le_bytes());
            d.extend_from_slice(&u16::from_be_bytes([r.data[2], r.data[3]]).to_le_bytes());
            d.extend_from_slice(&u16::from_be_bytes([r.data[4], r.data[5]]).to_le_bytes());
            d.extend_from_slice(&[0, 0]);
            (d, 16)
        }
        16 => {
            // TXT : dwStringCount puis un tableau de pointeurs.
            let mut strings = Vec::new();
            let mut i = 0;
            while i < r.data.len() {
                let n = r.data[i] as usize;
                let s = String::from_utf8_lossy(r.data.get(i + 1..i + 1 + n).unwrap_or(&[]))
                    .to_string();
                strings.push(wide(&s) as usize);
                i += 1 + n;
            }
            let mut d = (strings.len() as u32).to_le_bytes().to_vec();
            d.extend_from_slice(&[0; 4]);
            for p in &strings {
                d.extend_from_slice(&p.to_le_bytes());
            }
            let len = d.len() as u16;
            (d, len)
        }
        _ => return None,
    };
    let block = libc::calloc(1, 32 + data.len().max(16)) as *mut u8;
    if block.is_null() {
        return None;
    }
    std::ptr::write_unaligned(block.add(8) as *mut usize, wide(&r.name) as usize);
    std::ptr::write_unaligned(block.add(16) as *mut u16, r.ty);
    std::ptr::write_unaligned(block.add(18) as *mut u16, data_len);
    std::ptr::write_unaligned(block.add(20) as *mut u32, 0x2001); // DnsSectionAnswer, jeu de caractères Unicode
    std::ptr::write_unaligned(block.add(24) as *mut u32, r.ttl);
    std::ptr::copy_nonoverlapping(data.as_ptr(), block.add(32), data.len());
    Some(block)
}

fn dns_records() -> &'static Mutex<HashMap<usize, Vec<usize>>> {
    static R: std::sync::OnceLock<Mutex<HashMap<usize, Vec<usize>>>> = std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

unsafe fn dns_query(name: &str, ty: u16, results: *mut usize) -> i32 {
    const DNS_ERROR_RCODE_NAME_ERROR: i32 = 9003;
    const DNS_INFO_NO_RECORDS: i32 = 9501;
    if results.is_null() {
        return 87;
    }
    *results = 0;
    let Ok(c) = std::ffi::CString::new(name) else {
        return 123;
    };
    let mut answer = vec![0u8; 8192];
    let n = resolv::res_query(
        c.as_ptr(),
        1,
        ty as i32,
        answer.as_mut_ptr(),
        answer.len() as i32,
    );
    if n < 0 {
        return DNS_ERROR_RCODE_NAME_ERROR;
    }
    let msg = &answer[..(n as usize).min(answer.len())];
    let blocks: Vec<*mut u8> = parse_answers(msg)
        .iter()
        .filter(|r| r.ty == ty || ty == 255)
        .filter_map(|r| build_record(msg, r))
        .collect();
    if blocks.is_empty() {
        return DNS_INFO_NO_RECORDS;
    }
    for w in blocks.windows(2) {
        std::ptr::write_unaligned(w[0] as *mut usize, w[1] as usize);
    }
    lock(dns_records()).insert(
        blocks[0] as usize,
        blocks.iter().map(|b| *b as usize).collect(),
    );
    *results = blocks[0] as usize;
    0
}
unsafe extern "win64" fn DnsQuery_W(
    name: *const u16,
    ty: u16,
    _opt: u32,
    _extra: usize,
    results: *mut usize,
    _res: usize,
) -> i32 {
    if name.is_null() {
        return 87;
    }
    dns_query(&crate::dlls::wstr(name), ty, results)
}
unsafe extern "win64" fn DnsQuery_A(
    name: *const u8,
    ty: u16,
    _opt: u32,
    _extra: usize,
    results: *mut usize,
    _res: usize,
) -> i32 {
    if name.is_null() {
        return 87;
    }
    dns_query(&crate::dlls::cstr(name), ty, results)
}
/// Libère la liste (les chaînes des enregistrements restent : quelques octets).
unsafe extern "win64" fn DnsFree(p: usize, _ty: u32) {
    if let Some(list) = lock(dns_records()).remove(&p) {
        for b in list {
            libc::free(b as *mut c_void);
        }
    }
}
unsafe extern "win64" fn DnsRecordListFree(p: usize, ty: u32) {
    DnsFree(p, ty)
}
pub fn dnsapi_exports() -> Exports {
    crate::exports! {
        "DnsQuery_W" => fn DnsQuery_W, "DnsQuery_A" => fn DnsQuery_A, "DnsQuery_UTF8" => fn DnsQuery_A,
        "DnsFree" => fn DnsFree, "DnsRecordListFree" => fn DnsRecordListFree,
    }
}

// =====================================================================
// advapi32 : hachage CryptoAPI, aléa, descripteurs de sécurité
// =====================================================================

const CALG_MD5: u32 = 0x8003;
const CALG_SHA1: u32 = 0x8004;
const CALG_SHA_256: u32 = 0x800C;

/// MD5 (RFC 1321), pour CryptCreateHash et BCrypt.
pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    let (mut a0, mut b0, mut c0, mut d0) = (
        0x6745_2301u32,
        0xefcd_ab89u32,
        0x98ba_dcfeu32,
        0x1032_5476u32,
    );
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk
            .chunks(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, v) in [a0, b0, c0, d0].iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

struct Hash {
    alg: u32,
    data: Vec<u8>,
}
fn hashes() -> &'static Mutex<HashMap<usize, Hash>> {
    static H: std::sync::OnceLock<Mutex<HashMap<usize, Hash>>> = std::sync::OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}
fn digest(alg: u32, data: &[u8]) -> Vec<u8> {
    match alg {
        CALG_MD5 => md5(data).to_vec(),
        CALG_SHA1 => crate::dlls::bcrypt::crypto::sha1(&[data]).to_vec(),
        _ => crate::dlls::bcrypt::crypto::sha256(&[data]).to_vec(),
    }
}
unsafe extern "win64" fn CryptAcquireContextW(
    ph: *mut usize,
    _c: *const u16,
    _p: *const u16,
    _t: u32,
    _f: u32,
) -> i32 {
    crate::dlls::advapi32::crypt_acquire_context_a(ph, std::ptr::null(), std::ptr::null(), 0, 0)
}
unsafe extern "win64" fn CryptCreateHash(
    _prov: usize,
    alg: u32,
    key: usize,
    _flags: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() || key != 0 {
        set_error(87);
        return 0;
    }
    if !matches!(alg, CALG_MD5 | CALG_SHA1 | CALG_SHA_256) {
        set_error(0x8009_0008); // NTE_BAD_ALGID
        return 0;
    }
    let h = fake_handle();
    lock(hashes()).insert(
        h,
        Hash {
            alg,
            data: Vec::new(),
        },
    );
    *out = h;
    1
}
unsafe extern "win64" fn CryptHashData(h: usize, data: *const u8, len: u32, _flags: u32) -> i32 {
    match lock(hashes()).get_mut(&h) {
        Some(hash) => {
            if len > 0 && !data.is_null() {
                hash.data
                    .extend_from_slice(std::slice::from_raw_parts(data, len as usize));
            }
            1
        }
        None => {
            set_error(6);
            0
        }
    }
}
unsafe extern "win64" fn CryptGetHashParam(
    h: usize,
    param: u32,
    out: *mut u8,
    len: *mut u32,
    _flags: u32,
) -> i32 {
    let map = lock(hashes());
    let Some(hash) = map.get(&h) else {
        set_error(6);
        return 0;
    };
    let value: Vec<u8> = match param {
        1 => hash.alg.to_le_bytes().to_vec(), // HP_ALGID
        2 => digest(hash.alg, &hash.data),    // HP_HASHVAL
        4 => (digest(hash.alg, &[]).len() as u32).to_le_bytes().to_vec(), // HP_HASHSIZE
        _ => {
            set_error(87);
            return 0;
        }
    };
    if len.is_null() {
        set_error(87);
        return 0;
    }
    if out.is_null() {
        *len = value.len() as u32;
        return 1;
    }
    if (*len as usize) < value.len() {
        *len = value.len() as u32;
        set_error(234); // ERROR_MORE_DATA
        return 0;
    }
    std::ptr::copy_nonoverlapping(value.as_ptr(), out, value.len());
    *len = value.len() as u32;
    1
}
extern "win64" fn CryptDestroyHash(h: usize) -> i32 {
    lock(hashes()).remove(&h).is_some() as i32
}
extern "win64" fn bad_key() -> i32 {
    set_error(0x8009_0008);
    0
}
/// RtlGenRandom.
unsafe extern "win64" fn SystemFunction036(buf: *mut u8, len: u32) -> u8 {
    if buf.is_null() {
        return 0;
    }
    crate::dlls::bcrypt::crypto::random_bytes(std::slice::from_raw_parts_mut(buf, len as usize));
    1
}
unsafe extern "win64" fn BuildExplicitAccessWithNameW(
    ea: *mut u8,
    _name: *const u16,
    perms: u32,
    mode: u32,
    inherit: u32,
) {
    if ea.is_null() {
        return;
    }
    // EXPLICIT_ACCESS_W : grfAccessPermissions, grfAccessMode, grfInheritance, TRUSTEE_W.
    std::ptr::write_bytes(ea, 0, 48);
    std::ptr::write_unaligned(ea as *mut u32, perms);
    std::ptr::write_unaligned(ea.add(4) as *mut u32, mode);
    std::ptr::write_unaligned(ea.add(8) as *mut u32, inherit);
}
/// BuildSecurityDescriptorW(owner, group, nacc, acc, naud, aud, old, size,
/// out) : descripteur relatif minimal (DACL nulle), comme
/// ConvertStringSecurityDescriptorToSecurityDescriptorW. Winlux applique les
/// droits Linux ; crashpad s'en sert pour son tube nommé.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn BuildSecurityDescriptorW(
    _owner: usize,
    _group: usize,
    _n_access: u32,
    _access: usize,
    _n_audit: u32,
    _audit: usize,
    _old: usize,
    size: *mut u32,
    out: *mut usize,
) -> u32 {
    if out.is_null() || size.is_null() {
        return 87;
    }
    let p = crate::dlls::kernel32_misc::local_alloc(0x40, 20) as *mut u8;
    if p.is_null() {
        return 8;
    }
    *p = 1; // SECURITY_DESCRIPTOR_REVISION
    std::ptr::write_unaligned(p.add(2) as *mut u16, 0x8004); // SE_SELF_RELATIVE | SE_DACL_PRESENT
    *out = p as usize;
    *size = 20;
    0
}
/// Descripteur relatif minimal (DACL nulle : accès à tous) : Winlux
/// applique les droits Linux, pas les ACL de la chaîne SDDL.
unsafe extern "win64" fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
    _s: *const u16,
    _rev: u32,
    out: *mut usize,
    len: *mut u32,
) -> i32 {
    if out.is_null() {
        set_error(87);
        return 0;
    }
    let p = crate::dlls::kernel32_misc::local_alloc(0x40, 20) as *mut u8;
    if p.is_null() {
        set_error(8);
        return 0;
    }
    *p = 1; // SECURITY_DESCRIPTOR_REVISION
    std::ptr::write_unaligned(p.add(2) as *mut u16, 0x8004); // SE_SELF_RELATIVE | SE_DACL_PRESENT
    *out = p as usize;
    if !len.is_null() {
        *len = 20;
    }
    1
}
pub fn advapi32_exports() -> Exports {
    crate::exports! {
        "CryptAcquireContextW" => fn CryptAcquireContextW,
        "CryptCreateHash" => fn CryptCreateHash, "CryptHashData" => fn CryptHashData,
        "CryptGetHashParam" => fn CryptGetHashParam, "CryptDestroyHash" => fn CryptDestroyHash,
        "CryptImportKey" => fn bad_key, "CryptEncrypt" => fn bad_key, "CryptDecrypt" => fn bad_key,
        "CryptDestroyKey" => fn ok1,
        "SystemFunction036" => fn SystemFunction036,
        "BuildExplicitAccessWithNameW" => fn BuildExplicitAccessWithNameW,
        "BuildSecurityDescriptorW" => fn BuildSecurityDescriptorW,
        "ConvertStringSecurityDescriptorToSecurityDescriptorW" => fn ConvertStringSecurityDescriptorToSecurityDescriptorW,
        "ImpersonateNamedPipeClient" => fn ok1,
    }
}

// =====================================================================
// crypt32 : chaînes de certificats (Schannel de libcurl) — absentes
// =====================================================================

/// CRYPT_E_NOT_FOUND : libcurl renonce à la vérification Schannel.
extern "win64" fn crypt_not_found() -> i32 {
    set_error(0x8009_2004);
    0
}
extern "win64" fn null_ptr() -> usize {
    0
}
pub fn crypt32_exports() -> Exports {
    crate::exports! {
        "CertAddCertificateContextToStore" => fn ok1,
        "CertCreateCertificateChainEngine" => fn crypt_not_found,
        "CertFindExtension" => fn null_ptr,
        "CertFreeCRLContext" => fn ok1, "CertFreeCTLContext" => fn ok1,
        "CertFreeCertificateChain" => fn zero, "CertFreeCertificateChainEngine" => fn zero,
        "CryptDecodeObjectEx" => fn crypt_not_found, "CryptQueryObject" => fn crypt_not_found,
    }
}

// =====================================================================
// ws2_32 : envois et réceptions à tampons multiples
// =====================================================================

#[repr(C)]
struct WsaBuf {
    len: u32,
    buf: *mut u8,
}
unsafe fn gather(bufs: *const WsaBuf, count: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..count as usize {
        let b = &*bufs.add(i);
        if b.len > 0 && !b.buf.is_null() {
            out.extend_from_slice(std::slice::from_raw_parts(b.buf, b.len as usize));
        }
    }
    out
}
unsafe extern "win64" fn WSASendTo(
    s: usize,
    bufs: *const WsaBuf,
    count: u32,
    sent: *mut u32,
    flags: u32,
    to: *const u8,
    tolen: i32,
    _ovl: usize,
    _cb: usize,
) -> i32 {
    if bufs.is_null() {
        crate::dlls::ws2_32::wsa_set_last_error(10014);
        return -1;
    }
    let data = gather(bufs, count);
    let r = if to.is_null() {
        crate::dlls::ws2_32::send(s, data.as_ptr(), data.len() as i32, flags as i32)
    } else {
        crate::dlls::ws2_32::send_to(s, data.as_ptr(), data.len() as i32, flags as i32, to, tolen)
    };
    if r < 0 {
        return -1;
    }
    if !sent.is_null() {
        *sent = r as u32;
    }
    0
}
unsafe extern "win64" fn WSARecvFrom(
    s: usize,
    bufs: *const WsaBuf,
    count: u32,
    recvd: *mut u32,
    flags: *mut u32,
    from: *mut u8,
    fromlen: *mut i32,
    _ovl: usize,
    _cb: usize,
) -> i32 {
    if bufs.is_null() || count == 0 {
        crate::dlls::ws2_32::wsa_set_last_error(10014);
        return -1;
    }
    let total: usize = (0..count as usize)
        .map(|i| (*bufs.add(i)).len as usize)
        .sum();
    let mut tmp = vec![0u8; total];
    let f = if flags.is_null() { 0 } else { *flags as i32 };
    let r = crate::dlls::ws2_32::recv_from(s, tmp.as_mut_ptr(), total as i32, f, from, fromlen);
    if r < 0 {
        return -1;
    }
    let mut left = &tmp[..r as usize];
    for i in 0..count as usize {
        let b = &*bufs.add(i);
        let n = (b.len as usize).min(left.len());
        std::ptr::copy_nonoverlapping(left.as_ptr(), b.buf, n);
        left = &left[n..];
    }
    if !recvd.is_null() {
        *recvd = r as u32;
    }
    if !flags.is_null() {
        *flags = 0;
    }
    0
}
/// WSASendMsg : aussi fonction d'extension, servie par ws2_32 (#1629).
unsafe extern "win64" fn WSASendMsg(
    s: usize,
    msg: usize,
    flags: u32,
    sent: *mut u32,
    ovl: usize,
    cb: usize,
) -> i32 {
    crate::dlls::ws2_32::wsa_send_msg(s, msg, flags, sent, ovl, cb)
}
pub fn ws2_32_exports() -> Exports {
    crate::exports! {
        "WSASendTo" => fn WSASendTo, "WSARecvFrom" => fn WSARecvFrom, "WSASendMsg" => fn WSASendMsg,
    }
}

// =====================================================================
// dbghelp, shlwapi, shell32, iphlpapi, winhttp, powrprof, rpcrt4
// =====================================================================

extern "win64" fn SymGetModuleBase64(_p: usize, addr: u64) -> u64 {
    crate::loader::list_modules()
        .iter()
        .find(|m| (m.base as u64..m.base as u64 + m.size as u64).contains(&addr))
        .map_or(0, |m| m.base as u64)
}
pub fn dbghelp_exports() -> Exports {
    crate::exports! {
        "SymFromAddrW" => fn not_supported,
        "SymFunctionTableAccess64" => fn null_ptr,
        "SymGetModuleBase64" => fn SymGetModuleBase64,
    }
}

unsafe extern "win64" fn PathAppendW(path: *mut u16, more: *const u16) -> i32 {
    if path.is_null() {
        return 0;
    }
    let base = crate::dlls::wstr(path);
    let extra = if more.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(more)
    };
    let extra = extra.trim_start_matches(['\\', '/']);
    let joined = if extra.is_empty() {
        base
    } else if base.is_empty() {
        extra.to_string()
    } else {
        format!("{}\\{}", base.trim_end_matches(['\\', '/']), extra)
    };
    let w: Vec<u16> = joined.encode_utf16().take(259).chain([0]).collect();
    std::ptr::copy_nonoverlapping(w.as_ptr(), path, w.len());
    1
}
/// Recopie l'URL (déjà canonique pour les usages des jeux : http(s)://…).
unsafe extern "win64" fn UrlCanonicalizeW(
    url: *const u16,
    out: *mut u16,
    len: *mut u32,
    _flags: u32,
) -> u32 {
    if url.is_null() || out.is_null() || len.is_null() {
        return 0x8007_0057;
    }
    let w: Vec<u16> = crate::dlls::wstr(url)
        .replace(' ', "%20")
        .encode_utf16()
        .collect();
    if (*len as usize) <= w.len() {
        *len = w.len() as u32 + 1;
        return 0x8000_4003; // E_POINTER
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
    *out.add(w.len()) = 0;
    *len = w.len() as u32;
    0
}
pub fn shlwapi_exports() -> Exports {
    crate::exports! { "PathAppendW" => fn PathAppendW, "UrlCanonicalizeW" => fn UrlCanonicalizeW }
}

unsafe fn create_directory_tree(path: &str) -> i32 {
    let linux = crate::paths::to_linux(path);
    if linux.is_dir() {
        return 183; // ERROR_ALREADY_EXISTS
    }
    match std::fs::create_dir_all(&linux) {
        Ok(()) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => 5,
        Err(_) => 3, // ERROR_PATH_NOT_FOUND
    }
}
unsafe extern "win64" fn SHCreateDirectoryExW(_hwnd: usize, path: *const u16, _sa: usize) -> i32 {
    if path.is_null() {
        return 161; // ERROR_BAD_PATHNAME
    }
    create_directory_tree(&crate::dlls::wstr(path))
}
unsafe extern "win64" fn SHCreateDirectoryExA(_hwnd: usize, path: *const u8, _sa: usize) -> i32 {
    if path.is_null() {
        return 161;
    }
    create_directory_tree(&crate::dlls::cstr(path))
}
unsafe extern "win64" fn SHCreateDirectory(hwnd: usize, path: *const u16) -> i32 {
    SHCreateDirectoryExW(hwnd, path, 0)
}
pub fn shell32_exports() -> Exports {
    crate::exports! {
        "SHCreateDirectoryExW" => fn SHCreateDirectoryExW, "SHCreateDirectoryExA" => fn SHCreateDirectoryExA,
        "SHCreateDirectory" => fn SHCreateDirectory,
        // Sélecteur de dossier : pas encore de boîte, l'appel équivaut à « Annuler ».
        "SHBrowseForFolderW" => fn null_ptr, "SHBrowseForFolderA" => fn null_ptr,
        "SHGetPathFromIDListW" => fn zero, "SHGetPathFromIDListA" => fn zero,
    }
}

unsafe extern "win64" fn FreeMibTable(p: *mut c_void) {
    libc::free(p);
}
/// Table de routage vide (NumEntries = 0) : la passerelle n'est pas exposée.
unsafe extern "win64" fn GetIpForwardTable2(_family: u16, out: *mut *mut c_void) -> u32 {
    if out.is_null() {
        return 87;
    }
    *out = libc::calloc(1, 16);
    0
}
unsafe extern "win64" fn if_nametoindex(name: *const libc::c_char) -> u32 {
    if name.is_null() {
        return 0;
    }
    libc::if_nametoindex(name)
}
unsafe extern "win64" fn if_indextoname(index: u32, out: *mut libc::c_char) -> *mut libc::c_char {
    libc::if_indextoname(index, out)
}
pub fn iphlpapi_exports() -> Exports {
    crate::exports! {
        "FreeMibTable" => fn FreeMibTable, "GetIpForwardTable2" => fn GetIpForwardTable2,
        "if_nametoindex" => fn if_nametoindex, "if_indextoname" => fn if_indextoname,
    }
}

/// URL_COMPONENTSW (x64). Pour chaque partie : pointeur + longueur.
const PARTS: [(usize, usize); 6] = [(8, 16), (24, 32), (40, 48), (56, 64), (72, 80), (88, 96)];
unsafe extern "win64" fn WinHttpCrackUrl(
    url: *const u16,
    len: u32,
    _flags: u32,
    comps: *mut u8,
) -> i32 {
    if url.is_null() || comps.is_null() {
        set_error(87);
        return 0;
    }
    let n = if len == 0 {
        crate::printf::wide_len(url)
    } else {
        len as usize
    };
    let w = std::slice::from_raw_parts(url, n);
    let text = String::from_utf16_lossy(w);
    let Some(sep) = text.find("://") else {
        set_error(12006); // ERROR_WINHTTP_UNRECOGNIZED_SCHEME
        return 0;
    };
    let scheme = &text[..sep];
    let (n_scheme, default_port) = match scheme.to_ascii_lowercase().as_str() {
        "http" => (1, 80u16),
        "https" => (2, 443u16),
        _ => {
            set_error(12006);
            return 0;
        }
    };
    let rest_start = sep + 3;
    let rest = &text[rest_start..];
    let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..auth_end];
    let (userinfo, hostport, host_off) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..], i + 1),
        None => (None, authority, 0),
    };
    let (host, port) = match hostport.rfind(':') {
        Some(i)
            if !hostport[i + 1..].is_empty()
                && hostport[i + 1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            (
                &hostport[..i],
                hostport[i + 1..].parse().unwrap_or(default_port),
            )
        }
        _ => (hostport, default_port),
    };
    let tail = &rest[auth_end..];
    let q = tail.find(['?', '#']).unwrap_or(tail.len());
    let (user, pass) = match userinfo {
        Some(u) => match u.find(':') {
            Some(i) => (&u[..i], &u[i + 1..]),
            None => (u, ""),
        },
        None => ("", ""),
    };
    // Positions (en unités UTF-16) dans l'URL d'origine.
    let off = |s: &str| {
        text[..(s.as_ptr() as usize - text.as_ptr() as usize)]
            .encode_utf16()
            .count()
    };
    let ulen = |s: &str| s.encode_utf16().count();
    let parts: [(&str, usize); 6] = [
        (scheme, 0),
        (host, rest_start + host_off),
        (user, 0),
        (pass, 0),
        (&tail[..q], 0),
        (&tail[q..], 0),
    ];
    let _ = parts[1].1;
    for (i, (part, _)) in parts.iter().enumerate() {
        let (po, lo) = PARTS[i];
        let ptr = std::ptr::read_unaligned(comps.add(po) as *const usize) as *mut u16;
        let cap = std::ptr::read_unaligned(comps.add(lo) as *const u32);
        let plen = ulen(part);
        if ptr.is_null() {
            if cap != 0 {
                let start = if part.is_empty() { 0 } else { off(part) };
                std::ptr::write_unaligned(
                    comps.add(po) as *mut usize,
                    if part.is_empty() {
                        0
                    } else {
                        url.add(start) as usize
                    },
                );
                std::ptr::write_unaligned(comps.add(lo) as *mut u32, plen as u32);
            }
        } else {
            if (cap as usize) <= plen {
                std::ptr::write_unaligned(comps.add(lo) as *mut u32, plen as u32 + 1);
                set_error(122); // ERROR_INSUFFICIENT_BUFFER
                return 0;
            }
            for (j, c) in part.encode_utf16().enumerate() {
                *ptr.add(j) = c;
            }
            *ptr.add(plen) = 0;
            std::ptr::write_unaligned(comps.add(lo) as *mut u32, plen as u32);
        }
    }
    std::ptr::write_unaligned(comps.add(20) as *mut i32, n_scheme);
    std::ptr::write_unaligned(comps.add(36) as *mut u16, port);
    1
}
pub fn winhttp_exports() -> Exports {
    crate::exports! { "WinHttpCrackUrl" => fn WinHttpCrackUrl, "WinHttpSetCredentials" => fn ok1 }
}

/// CallNtPowerInformation : ProcessorInformation (11) décrit chaque cœur.
unsafe extern "win64" fn CallNtPowerInformation(
    level: u32,
    _input: *const c_void,
    _inlen: u32,
    out: *mut u8,
    outlen: u32,
) -> u32 {
    if out.is_null() {
        return 0xC000_000D; // STATUS_INVALID_PARAMETER
    }
    std::ptr::write_bytes(out, 0, outlen as usize);
    if level == 11 {
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
        if (outlen as usize) < cpus * 24 {
            return 0xC000_0023; // STATUS_BUFFER_TOO_SMALL
        }
        let mhz = |cpu: usize, file: &str| -> u32 {
            std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/{file}"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .map_or(0, |khz| khz / 1000)
        };
        for i in 0..cpus {
            let p = out.add(i * 24) as *mut u32;
            let max = mhz(i, "cpuinfo_max_freq");
            let cur = mhz(i, "scaling_cur_freq");
            *p = i as u32;
            *p.add(1) = max;
            *p.add(2) = if cur == 0 { max } else { cur };
            *p.add(3) = max;
        }
    }
    0
}
pub fn powrprof_exports() -> Exports {
    crate::exports! { "CallNtPowerInformation" => fn CallNtPowerInformation }
}
pub fn rpcrt4_exports() -> Exports {
    crate::exports! { "UuidCreateSequential" => fn crate::dlls::rpcrt4::uuid_create }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_vecteurs_rfc1321() {
        let hex = |b: [u8; 16]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(md5(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
    }

    #[test]
    fn rectangles() {
        let a = Rect {
            left: 0,
            top: 0,
            right: 10,
            bottom: 10,
        };
        let b = Rect {
            left: 5,
            top: 5,
            right: 20,
            bottom: 20,
        };
        let mut r = Rect::default();
        assert_eq!(unsafe { IntersectRect(&mut r, &a, &b) }, 1);
        assert_eq!(
            r,
            Rect {
                left: 5,
                top: 5,
                right: 10,
                bottom: 10
            }
        );
        assert_eq!(unsafe { UnionRect(&mut r, &a, &b) }, 1);
        assert_eq!(
            r,
            Rect {
                left: 0,
                top: 0,
                right: 20,
                bottom: 20
            }
        );
        let c = Rect {
            left: 30,
            top: 30,
            right: 40,
            bottom: 40,
        };
        assert_eq!(unsafe { IntersectRect(&mut r, &a, &c) }, 0);
        assert_eq!(unsafe { IsRectEmpty(&r) }, 1);
    }

    #[test]
    fn crack_url_decoupe_les_parties() {
        let url: Vec<u16> = "https://user:pw@example.org:8443/a/b?x=1"
            .encode_utf16()
            .chain([0])
            .collect();
        let mut comps = [0u8; 104];
        comps[0..4].copy_from_slice(&104u32.to_le_bytes());
        for (_, lo) in PARTS {
            comps[lo..lo + 4].copy_from_slice(&1u32.to_le_bytes());
        }
        assert_eq!(
            unsafe { WinHttpCrackUrl(url.as_ptr(), 0, 0, comps.as_mut_ptr()) },
            1
        );
        let get = |i: usize| -> String {
            let (po, lo) = PARTS[i];
            let p = usize::from_le_bytes(comps[po..po + 8].try_into().unwrap()) as *const u16;
            let n = u32::from_le_bytes(comps[lo..lo + 4].try_into().unwrap()) as usize;
            if p.is_null() {
                String::new()
            } else {
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
            }
        };
        assert_eq!(get(0), "https");
        assert_eq!(get(1), "example.org");
        assert_eq!(get(2), "user");
        assert_eq!(get(3), "pw");
        assert_eq!(get(4), "/a/b");
        assert_eq!(get(5), "?x=1");
        assert_eq!(u16::from_le_bytes([comps[36], comps[37]]), 8443);
        assert_eq!(i32::from_le_bytes(comps[20..24].try_into().unwrap()), 2);
    }

    #[test]
    fn reponse_dns_srv() {
        // En-tête, question « _s._udp.x » SRV, une réponse SRV vers « h.x » port 2759.
        let mut m = vec![0u8, 1, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        let q = [2, b'_', b's', 4, b'_', b'u', b'd', b'p', 1, b'x', 0];
        m.extend_from_slice(&q);
        m.extend_from_slice(&[0, 33, 0, 1]);
        m.extend_from_slice(&[0xC0, 12, 0, 33, 0, 1, 0, 0, 0, 60]);
        let target = [1, b'h', 0xC0, 20];
        let mut rdata = vec![0, 10, 0, 5, 0x0A, 0xC7];
        rdata.extend_from_slice(&target);
        m.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        m.extend_from_slice(&rdata);
        let recs = parse_answers(&m);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].name, "_s._udp.x");
        assert_eq!(recs[0].ty, 33);
        assert_eq!(dns_name(&m, recs[0].rdata_off + 6).unwrap().0, "h.x");
    }
}
