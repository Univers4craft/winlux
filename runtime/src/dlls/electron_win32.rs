//! Fonctions qu'importe directement l'appli de bureau Claude pour Windows
//! (Electron, `claude.exe` 0.14) et que Winlux ne fournissait pas : relevé
//! des imports de l'installateur officiel (Squirrel) le 3 novembre 2026.
//! Une DLL absente empêche le programme de se charger (dhcpcsvc) ; une
//! fonction absente devient un bouchon qui plante au premier appel.
//!
//! Chaque table complète la DLL sans jamais remplacer une fonction qu'elle
//! fournit déjà (voir `add_basics` dans mod.rs).

#![allow(non_snake_case)]

use crate::dlls::{lookup, wstr};
use crate::process::set_last_error;
use std::ffi::c_void;

const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_DATA: u32 = 232;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_IO_PENDING: u32 = 997;
const ERROR_NOT_FOUND: u32 = 1168;
const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;
const STATUS_NOT_IMPLEMENTED: u32 = 0xC000_0002;
const STATUS_PENDING: usize = 0x103;
const E_NOTIMPL: u32 = 0x8000_4001;
const CRYPT_E_NOT_FOUND: u32 = 0x8009_2004;
const WSA_IO_INCOMPLETE: u32 = 996;
const WSASERVICE_NOT_FOUND: u32 = 10108;
const WSA_E_NO_MORE: u32 = 10110;
const SOCKET_ERROR: i32 = -1;

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

/// Vrai pour le pseudo-handle du processus courant ou un handle sur lui.
unsafe fn is_self(process: usize) -> bool {
    if process == usize::MAX {
        return true;
    }
    let pid = api!("kernel32", "GetProcessId", fn(usize) -> u32, process).unwrap_or(0);
    pid == std::process::id()
}

// ============================================================
//  kernel32
// ============================================================

unsafe extern "win64" fn CheckRemoteDebuggerPresent(process: usize, present: *mut i32) -> i32 {
    if process == 0 || present.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *present = 0;
    1
}

/// Fil dans un autre processus : seul le processus courant est accepté.
unsafe extern "win64" fn CreateRemoteThreadEx(
    process: usize,
    sa: *const c_void,
    stack: usize,
    start: usize,
    param: usize,
    flags: u32,
    _attrs: *mut c_void,
    tid: *mut u32,
) -> usize {
    if !is_self(process) {
        set_last_error(ERROR_NOT_SUPPORTED);
        return 0;
    }
    api!(
        "kernel32",
        "CreateThread",
        fn(*const c_void, usize, usize, usize, u32, *mut u32) -> usize,
        sa,
        stack,
        start,
        param,
        flags,
        tid
    )
    .unwrap_or(0)
}

unsafe extern "win64" fn CreateRemoteThread(
    process: usize,
    sa: *const c_void,
    stack: usize,
    start: usize,
    param: usize,
    flags: u32,
    tid: *mut u32,
) -> usize {
    CreateRemoteThreadEx(
        process,
        sa,
        stack,
        start,
        param,
        flags,
        std::ptr::null_mut(),
        tid,
    )
}

/// Mémoire dont le contenu peut être jeté : rendue au noyau.
unsafe extern "win64" fn DiscardVirtualMemory(addr: *mut c_void, size: usize) -> u32 {
    if addr.is_null() || size == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    libc::madvise(addr, size, libc::MADV_DONTNEED);
    ERROR_SUCCESS
}

/// WIN32_MEMORY_RANGE_ENTRY { VirtualAddress, NumberOfBytes }.
unsafe extern "win64" fn PrefetchVirtualMemory(
    _process: usize,
    n: usize,
    ranges: *const [usize; 2],
    _flags: u32,
) -> i32 {
    for i in 0..n {
        let r = *ranges.add(i);
        if r[0] != 0 && r[1] != 0 {
            libc::madvise(r[0] as *mut c_void, r[1], libc::MADV_WILLNEED);
        }
    }
    1
}

/// FindFirstFileExA : les niveaux standard et basique remplissent la même
/// WIN32_FIND_DATAA que FindFirstFileA (le nom court reste vide en basique).
unsafe extern "win64" fn FindFirstFileExA(
    name: *const u8,
    _level: u32,
    data: *mut c_void,
    _op: u32,
    _filter: *mut c_void,
    _flags: u32,
) -> usize {
    api!(
        "kernel32",
        "FindFirstFileA",
        fn(*const u8, *mut c_void) -> usize,
        name,
        data
    )
    .unwrap_or(usize::MAX)
}

/// Pas d'identité de paquet MSIX pour un programme lancé directement.
unsafe extern "win64" fn GetPackageFamilyName(
    _process: usize,
    len: *mut u32,
    _name: *mut u16,
) -> u32 {
    if !len.is_null() {
        *len = 0;
    }
    APPMODEL_ERROR_NO_PACKAGE
}

/// Version large de GetPrivateProfileStringA (conversion UTF-16 ↔ UTF-8).
unsafe extern "win64" fn GetPrivateProfileStringW(
    app: *const u16,
    key: *const u16,
    default: *const u16,
    out: *mut u16,
    size: u32,
    file: *const u16,
) -> u32 {
    let narrow = |p: *const u16| -> Option<std::ffi::CString> {
        (!p.is_null()).then(|| std::ffi::CString::new(wstr(p)).unwrap_or_default())
    };
    let (a, k, d, f) = (narrow(app), narrow(key), narrow(default), narrow(file));
    let ptr = |c: &Option<std::ffi::CString>| {
        c.as_ref()
            .map_or(std::ptr::null(), |c| c.as_ptr() as *const u8)
    };
    let mut buf = vec![0u8; (size as usize).max(1) * 4];
    let n = api!(
        "kernel32",
        "GetPrivateProfileStringA",
        fn(*const u8, *const u8, *const u8, *mut u8, u32, *const u8) -> u32,
        ptr(&a),
        ptr(&k),
        ptr(&d),
        buf.as_mut_ptr(),
        buf.len() as u32,
        ptr(&f)
    )
    .unwrap_or(0) as usize;
    if out.is_null() || size == 0 {
        return 0;
    }
    // Les listes (section ou clé nulle) sont des chaînes séparées par NUL.
    let text = String::from_utf8_lossy(&buf[..n.min(buf.len())]);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let room = size as usize - 1;
    let count = wide.len().min(room);
    std::ptr::copy_nonoverlapping(wide.as_ptr(), out, count);
    *out.add(count) = 0;
    count as u32
}

unsafe extern "win64" fn GetProcessHandleCount(_process: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *count = std::fs::read_dir("/proc/self/fd").map_or(0, |d| d.count()) as u32;
    1
}

/// Un seul tas : celui du processus.
unsafe extern "win64" fn GetProcessHeaps(n: u32, heaps: *mut usize) -> u32 {
    if n >= 1 && !heaps.is_null() {
        *heaps = api!("kernel32", "GetProcessHeap", fn() -> usize).unwrap_or(0);
    }
    1
}

unsafe extern "win64" fn HeapLock(heap: usize) -> i32 {
    i32::from(heap != 0)
}

unsafe extern "win64" fn HeapUnlock(heap: usize) -> i32 {
    i32::from(heap != 0)
}

/// Le parcours d'un tas n'est pas proposé : fin immédiate de la liste.
unsafe extern "win64" fn HeapWalk(_heap: usize, _entry: *mut c_void) -> i32 {
    set_last_error(ERROR_NO_MORE_ITEMS);
    0
}

/// MODULEINFO { lpBaseOfDll, SizeOfImage (u32), EntryPoint }.
unsafe extern "win64" fn K32GetModuleInformation(
    process: usize,
    module: usize,
    info: *mut usize,
    cb: u32,
) -> i32 {
    if info.is_null() || (cb as usize) < 24 {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    if !is_self(process) {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    }
    let m = if module == 0 {
        crate::loader::main_module()
    } else {
        crate::loader::find_module_by_addr(module)
    };
    let Some(m) = m else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    *info = m.base;
    *(info.add(1) as *mut u32) = m.size as u32;
    *info.add(2) = if m.pe.entry_rva == 0 {
        0
    } else {
        m.base + m.pe.entry_rva as usize
    };
    1
}

/// PERFORMANCE_INFORMATION (104 octets), d'après sysinfo(2).
unsafe extern "win64" fn K32GetPerformanceInfo(info: *mut u8, cb: u32) -> i32 {
    if info.is_null() || cb < 104 {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    let mut si: libc::sysinfo = std::mem::zeroed();
    libc::sysinfo(&mut si);
    let page = 4096u64;
    let unit = si.mem_unit.max(1) as u64;
    let pages = |bytes: u64| (bytes / page) as usize;
    let total = si.totalram as u64 * unit;
    let free = (si.freeram as u64 + si.bufferram as u64) * unit;
    let swap = si.totalswap as u64 * unit;
    let vals: [usize; 10] = [
        pages(total - free),               // CommitTotal
        pages(total + swap),               // CommitLimit
        pages(total - free),               // CommitPeak
        pages(total),                      // PhysicalTotal
        pages(free),                       // PhysicalAvailable
        pages(si.bufferram as u64 * unit), // SystemCache
        0,                                 // KernelTotal
        0,                                 // KernelPaged
        0,                                 // KernelNonpaged
        page as usize,                     // PageSize
    ];
    std::ptr::write_bytes(info, 0, 104);
    // Le tampon de l'appli n'est pas forcément aligné : écritures non alignées.
    (info as *mut u32).write_unaligned(104);
    let p = info.add(8) as *mut usize;
    for (i, v) in vals.iter().enumerate() {
        p.add(i).write_unaligned(*v);
    }
    let counts = info.add(88) as *mut u32;
    counts.write_unaligned(0); // HandleCount
    counts.add(1).write_unaligned(1); // ProcessCount
    counts.add(2).write_unaligned(si.procs as u32); // ThreadCount
    1
}

/// PSAPI_WORKING_SET_EX_INFORMATION { VirtualAddress, VirtualAttributes } :
/// une page présente (mincore) est marquée valide (bit 0).
unsafe extern "win64" fn K32QueryWorkingSetEx(_process: usize, buf: *mut usize, cb: u32) -> i32 {
    if buf.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    for i in 0..cb as usize / 16 {
        let addr = *buf.add(i * 2) & !0xFFF;
        let mut present = 0u8;
        let ok = libc::mincore(addr as *mut c_void, 4096, &mut present) == 0;
        *buf.add(i * 2 + 1) = usize::from(ok && present & 1 != 0);
    }
    1
}

fn cpu_time(clock: libc::clockid_t) -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(clock, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// Cycles approchés : nanosecondes de temps processeur × 3 (horloge de
/// 3 GHz supposée). Chromium ne les compare qu'entre eux.
unsafe extern "win64" fn QueryProcessCycleTime(_process: usize, cycles: *mut u64) -> i32 {
    if cycles.is_null() {
        return 0;
    }
    *cycles = cpu_time(libc::CLOCK_PROCESS_CPUTIME_ID) * 3;
    1
}

unsafe extern "win64" fn QueryThreadCycleTime(_thread: usize, cycles: *mut u64) -> i32 {
    if cycles.is_null() {
        return 0;
    }
    *cycles = cpu_time(libc::CLOCK_THREAD_CPUTIME_ID) * 3;
    1
}

/// ReplaceFileW : l'ancien fichier part en sauvegarde (si demandée), le
/// remplaçant prend sa place, par des renommages atomiques.
unsafe extern "win64" fn ReplaceFileW(
    replaced: *const u16,
    replacement: *const u16,
    backup: *const u16,
    _flags: u32,
    _exclude: *mut c_void,
    _reserved: *mut c_void,
) -> i32 {
    if replaced.is_null() || replacement.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let target = crate::paths::to_linux(&wstr(replaced));
    let source = crate::paths::to_linux(&wstr(replacement));
    if !target.exists() {
        set_last_error(2); // ERROR_FILE_NOT_FOUND
        return 0;
    }
    if !backup.is_null() {
        let b = crate::paths::to_linux(&wstr(backup));
        if std::fs::copy(&target, &b).is_err() {
            set_last_error(1177); // ERROR_UNABLE_TO_REMOVE_REPLACED
            return 0;
        }
    }
    match std::fs::rename(&source, &target) {
        Ok(()) => 1,
        Err(_) => {
            set_last_error(1176); // ERROR_UNABLE_TO_MOVE_REPLACEMENT
            0
        }
    }
}

unsafe extern "win64" fn TerminateJobObject(job: usize, _code: u32) -> i32 {
    i32::from(job != 0)
}

unsafe extern "win64" fn VirtualFreeEx(
    process: usize,
    addr: *mut c_void,
    size: usize,
    kind: u32,
) -> i32 {
    if !is_self(process) {
        set_last_error(ERROR_NOT_SUPPORTED);
        return 0;
    }
    api!(
        "kernel32",
        "VirtualFree",
        fn(*mut c_void, usize, u32) -> i32,
        addr,
        size,
        kind
    )
    .unwrap_or(0)
}

/// Même numéro de session que ProcessIdToSessionId.
unsafe extern "win64" fn WTSGetActiveConsoleSessionId() -> u32 {
    1
}

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CheckRemoteDebuggerPresent" => fn CheckRemoteDebuggerPresent,
        "CreateRemoteThread" => fn CreateRemoteThread,
        "CreateRemoteThreadEx" => fn CreateRemoteThreadEx,
        "DiscardVirtualMemory" => fn DiscardVirtualMemory,
        "PrefetchVirtualMemory" => fn PrefetchVirtualMemory,
        "FindFirstFileExA" => fn FindFirstFileExA,
        "GetPackageFamilyName" => fn GetPackageFamilyName,
        "GetPrivateProfileStringW" => fn GetPrivateProfileStringW,
        "GetProcessHandleCount" => fn GetProcessHandleCount,
        "GetProcessHeaps" => fn GetProcessHeaps,
        "HeapLock" => fn HeapLock,
        "HeapUnlock" => fn HeapUnlock,
        "HeapWalk" => fn HeapWalk,
        "K32GetModuleInformation" => fn K32GetModuleInformation,
        "K32GetPerformanceInfo" => fn K32GetPerformanceInfo,
        "K32QueryWorkingSetEx" => fn K32QueryWorkingSetEx,
        "QueryProcessCycleTime" => fn QueryProcessCycleTime,
        "QueryThreadCycleTime" => fn QueryThreadCycleTime,
        "ReplaceFileW" => fn ReplaceFileW,
        "TerminateJobObject" => fn TerminateJobObject,
        "VirtualFreeEx" => fn VirtualFreeEx,
        "WTSGetActiveConsoleSessionId" => fn WTSGetActiveConsoleSessionId,
    }
}

/// psapi : mêmes fonctions sans le préfixe K32 (rangées dans kernel32).
pub fn psapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetModuleInformation" => fn K32GetModuleInformation,
        "GetPerformanceInfo" => fn K32GetPerformanceInfo,
        "QueryWorkingSetEx" => fn K32QueryWorkingSetEx,
    }
}

// ============================================================
//  ntdll
// ============================================================

unsafe extern "win64" fn NtDeleteKey(_key: usize) -> u32 {
    STATUS_NOT_IMPLEMENTED
}

unsafe extern "win64" fn NtQueryObject(
    _h: usize,
    _class: u32,
    _info: *mut c_void,
    _len: u32,
    ret: *mut u32,
) -> u32 {
    if !ret.is_null() {
        *ret = 0;
    }
    STATUS_NOT_IMPLEMENTED
}

unsafe extern "win64" fn RtlGetLastNtStatus() -> u32 {
    0
}

pub fn ntdll_exports() -> crate::dlls::Exports {
    crate::exports! {
        "NtDeleteKey" => fn NtDeleteKey,
        "ZwDeleteKey" => fn NtDeleteKey,
        "NtQueryObject" => fn NtQueryObject,
        "ZwQueryObject" => fn NtQueryObject,
        "RtlGetLastNtStatus" => fn RtlGetLastNtStatus,
        "LdrLockLoaderLock" => fn LdrLockLoaderLock,
        "LdrUnlockLoaderLock" => fn LdrUnlockLoaderLock,
    }
}

/// Le chargeur de Winlux n'a pas de verrou exposé aux applis : on rend un
/// jeton non nul et l'état « verrou pris » (1), ce que Crashpad attend.
unsafe extern "win64" fn LdrLockLoaderLock(
    _flags: u32,
    state: *mut u32,
    cookie: *mut usize,
) -> u32 {
    if !state.is_null() {
        state.write_unaligned(1);
    }
    if !cookie.is_null() {
        cookie.write_unaligned(0x4c44_524c);
    }
    0
}

unsafe extern "win64" fn LdrUnlockLoaderLock(_flags: u32, _cookie: usize) -> u32 {
    0
}

// ============================================================
//  advapi32 : validation d'une ACL
// ============================================================

/// ACL { AclRevision u8, Sbz1 u8, AclSize u16, AceCount u16, Sbz2 u16 },
/// suivie de AceCount ACE { AceType u8, AceFlags u8, AceSize u16, … }.
/// Valide si la révision est 2 à 4 et que chaque ACE tient dans AclSize.
pub(crate) unsafe extern "win64" fn IsValidAcl(acl: *const u8) -> i32 {
    if acl.is_null() {
        return 0;
    }
    let rev = *acl;
    let size = (acl.add(2) as *const u16).read_unaligned() as usize;
    let count = (acl.add(4) as *const u16).read_unaligned() as usize;
    if !(2..=4).contains(&rev) || size < 8 {
        return 0;
    }
    let mut off = 8usize;
    for _ in 0..count {
        if off + 4 > size {
            return 0;
        }
        let ace = (acl.add(off + 2) as *const u16).read_unaligned() as usize;
        if ace < 4 || off + ace > size {
            return 0;
        }
        off += ace;
    }
    1
}

pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "IsValidAcl" => fn IsValidAcl,
    }
}

// ============================================================
//  uxtheme : ordinaux non documentés du mode sombre (Windows 10 1903+)
// ============================================================

/// #133 AllowDarkModeForWindow(hwnd, allow) -> BOOL
unsafe extern "win64" fn AllowDarkModeForWindow(_hwnd: usize, _allow: i32) -> i32 {
    1
}

/// #135 SetPreferredAppMode(mode) -> ancien mode (0 = Default)
unsafe extern "win64" fn SetPreferredAppMode(_mode: u32) -> u32 {
    0
}

/// #132 ShouldAppsUseDarkMode, #138 ShouldSystemUseDarkMode : mode clair.
unsafe extern "win64" fn ShouldUseDarkMode() -> i32 {
    0
}

/// #104 RefreshImmersiveColorPolicyState, #136 FlushMenuThemes
unsafe extern "win64" fn ThemeNoop() {}

/// #137 IsDarkModeAllowedForWindow(hwnd), #139 IsDarkModeAllowedForApp()
unsafe extern "win64" fn IsDarkModeAllowed(_hwnd: usize) -> i32 {
    1
}

// ============================================================
//  user32 : stations de fenêtres et bureaux (bac à sable Chromium)
// ============================================================
//
// Le bac à sable crée sa propre station et son bureau pour isoler les
// processus enfants. Sous Winlux tout s'affiche sur le même écran X11 :
// des handles stables suffisent, et GetUserObjectInformation (user32.rs)
// répond pour eux.

static NEXT_USER_OBJECT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0x5753_0100);

fn new_user_object() -> usize {
    NEXT_USER_OBJECT.fetch_add(4, std::sync::atomic::Ordering::Relaxed)
}

/// Bureau « Default » de la station WinSta0.
const DEFAULT_DESKTOP: usize = 0x4445_0001;

unsafe extern "win64" fn CreateWindowStation(
    _name: usize,
    _flags: u32,
    _access: u32,
    sa: usize,
) -> usize {
    let h = new_user_object();
    crate::dlls::inherit::from_security_attributes(h, sa);
    h
}

unsafe extern "win64" fn OpenWindowStation(_name: usize, _inherit: i32, _access: u32) -> usize {
    new_user_object()
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn CreateDesktop(
    _name: usize,
    _device: usize,
    _devmode: usize,
    _flags: u32,
    _access: u32,
    sa: usize,
    _heap_kb: u32,
    _reserved: usize,
) -> usize {
    let h = new_user_object();
    crate::dlls::inherit::from_security_attributes(h, sa);
    h
}

unsafe extern "win64" fn OpenDesktop(
    _name: usize,
    _flags: u32,
    _inherit: i32,
    _access: u32,
) -> usize {
    DEFAULT_DESKTOP
}

unsafe extern "win64" fn OpenInputDesktop(_flags: u32, _inherit: i32, _access: u32) -> usize {
    DEFAULT_DESKTOP
}

unsafe extern "win64" fn GetThreadDesktop(_tid: u32) -> usize {
    DEFAULT_DESKTOP
}

unsafe extern "win64" fn UserObjectOk(_h: usize) -> i32 {
    1
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateWindowStationW" => fn CreateWindowStation,
        "CreateWindowStationA" => fn CreateWindowStation,
        "OpenWindowStationW" => fn OpenWindowStation,
        "OpenWindowStationA" => fn OpenWindowStation,
        "CloseWindowStation" => fn UserObjectOk,
        "SetProcessWindowStation" => fn UserObjectOk,
        "CreateDesktopW" => fn CreateDesktop,
        "CreateDesktopA" => fn CreateDesktop,
        "CreateDesktopExW" => fn CreateDesktop,
        "CreateDesktopExA" => fn CreateDesktop,
        "OpenDesktopW" => fn OpenDesktop,
        "OpenDesktopA" => fn OpenDesktop,
        "OpenInputDesktop" => fn OpenInputDesktop,
        "GetThreadDesktop" => fn GetThreadDesktop,
        "SetThreadDesktop" => fn UserObjectOk,
        "SwitchDesktop" => fn UserObjectOk,
        "CloseDesktop" => fn UserObjectOk,
    }
}

pub fn uxtheme_exports() -> crate::dlls::Exports {
    crate::exports! {
        "#104" => fn ThemeNoop,
        "#132" => fn ShouldUseDarkMode,
        "#133" => fn AllowDarkModeForWindow,
        "#135" => fn SetPreferredAppMode,
        "#136" => fn ThemeNoop,
        "#137" => fn IsDarkModeAllowed,
        "#138" => fn ShouldUseDarkMode,
        "#139" => fn IsDarkModeAllowed,
    }
}

// ============================================================
//  dhcpcsvc : client DHCP (recherche du proxy par WPAD)
// ============================================================

unsafe extern "win64" fn DhcpCApiInitialize(version: *mut u32) -> u32 {
    if !version.is_null() {
        *version = 2;
    }
    ERROR_SUCCESS
}

unsafe extern "win64" fn DhcpCApiCleanup() {}

/// Le bail DHCP appartient au réseau Linux : aucune option n'est fournie,
/// Chromium passe alors à la découverte du proxy par DNS.
unsafe extern "win64" fn DhcpRequestParams(
    _flags: u32,
    _reserved: *mut c_void,
    _adapter: *const u16,
    _class: *mut c_void,
    // DHCPCAPI_PARAMS_ARRAY (16 octets) : passé par adresse en x64.
    _send: *const c_void,
    _recd: *const c_void,
    _buffer: *mut u8,
    size: *mut u32,
    _app: *const u16,
) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    ERROR_NOT_SUPPORTED
}

unsafe extern "win64" fn DhcpUndoRequestParams(
    _flags: u32,
    _reserved: *mut c_void,
    _adapter: *const u16,
    _app: *const u16,
) -> u32 {
    ERROR_SUCCESS
}

pub fn dhcpcsvc_exports() -> crate::dlls::Exports {
    crate::exports! {
        "DhcpCApiInitialize" => fn DhcpCApiInitialize,
        "DhcpCApiCleanup" => fn DhcpCApiCleanup,
        "DhcpRequestParams" => fn DhcpRequestParams,
        "DhcpUndoRequestParams" => fn DhcpUndoRequestParams,
    }
}

// ============================================================
//  iphlpapi : notifications de changement de réseau
// ============================================================

/// Avec un OVERLAPPED, la demande reste en attente (aucun changement
/// signalé) ; sans, l'appel bloquerait : refusé.
unsafe extern "win64" fn NotifyAddrChange(handle: *mut usize, overlapped: *mut c_void) -> u32 {
    if overlapped.is_null() {
        return ERROR_NOT_SUPPORTED;
    }
    if !handle.is_null() {
        *handle = 0;
    }
    ERROR_IO_PENDING
}

unsafe extern "win64" fn CancelIPChangeNotify(_overlapped: *mut c_void) -> i32 {
    1
}

/// Inscription acceptée ; aucun rappel (le réseau ne change pas sous Winlux
/// du point de vue de l'appli). Le handle sert à CancelMibChangeNotify2.
unsafe extern "win64" fn NotifyIpInterfaceChange(
    _family: u16,
    _cb: usize,
    _ctx: usize,
    _initial: u8,
    handle: *mut usize,
) -> u32 {
    if handle.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *handle = 0x5157_0001;
    ERROR_SUCCESS
}

unsafe extern "win64" fn CancelMibChangeNotify2(_handle: usize) -> u32 {
    ERROR_SUCCESS
}

/// NET_LUID : index dans les bits 24 à 47, type d'interface (6, Ethernet)
/// dans les bits 48 à 63.
unsafe extern "win64" fn ConvertInterfaceIndexToLuid(index: u32, luid: *mut u64) -> u32 {
    if luid.is_null() || index == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    *luid = (6u64 << 48) | ((index as u64 & 0xFF_FFFF) << 24);
    ERROR_SUCCESS
}

unsafe extern "win64" fn ConvertInterfaceLuidToNameW(
    luid: *const u64,
    name: *mut u16,
    len: usize,
) -> u32 {
    if luid.is_null() || name.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let text = format!("ethernet_{}", (*luid >> 24) & 0xFF_FFFF);
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.len() + 1 > len {
        return ERROR_INSUFFICIENT_BUFFER;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), name, wide.len());
    *name.add(wide.len()) = 0;
    ERROR_SUCCESS
}

unsafe extern "win64" fn GetBestRoute2(
    _luid: *const u64,
    _index: u32,
    _src: *const c_void,
    _dst: *const c_void,
    _options: u32,
    _route: *mut c_void,
    _best: *mut c_void,
) -> u32 {
    ERROR_NOT_FOUND
}

/// IP_INTERFACE_INFO sans adaptateur (les adresses passent par
/// GetAdaptersAddresses).
unsafe extern "win64" fn GetInterfaceInfo(info: *mut i32, len: *mut u32) -> u32 {
    if len.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    if info.is_null() || *len < 4 {
        *len = 4;
        return ERROR_INSUFFICIENT_BUFFER;
    }
    *info = 0;
    ERROR_NO_DATA
}

unsafe extern "win64" fn IpReleaseAddress(_adapter: *mut c_void) -> u32 {
    ERROR_NOT_SUPPORTED
}

pub fn iphlpapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "NotifyAddrChange" => fn NotifyAddrChange,
        "CancelIPChangeNotify" => fn CancelIPChangeNotify,
        "NotifyIpInterfaceChange" => fn NotifyIpInterfaceChange,
        "CancelMibChangeNotify2" => fn CancelMibChangeNotify2,
        "ConvertInterfaceIndexToLuid" => fn ConvertInterfaceIndexToLuid,
        "ConvertInterfaceLuidToNameW" => fn ConvertInterfaceLuidToNameW,
        "GetBestRoute2" => fn GetBestRoute2,
        "GetInterfaceInfo" => fn GetInterfaceInfo,
        "IpReleaseAddress" => fn IpReleaseAddress,
        "IpRenewAddress" => fn IpReleaseAddress,
    }
}

// ============================================================
//  ws2_32
// ============================================================

unsafe extern "win64" fn WSAEnumProtocolsW(
    _protocols: *const i32,
    _buf: *mut c_void,
    len: *mut u32,
) -> i32 {
    if !len.is_null() {
        *len = 0;
    }
    0
}

/// OVERLAPPED { Internal (statut), InternalHigh (octets), … }.
unsafe extern "win64" fn WSAGetOverlappedResult(
    _socket: usize,
    ov: *const usize,
    bytes: *mut u32,
    _wait: i32,
    flags: *mut u32,
) -> i32 {
    if ov.is_null() || bytes.is_null() {
        api!("ws2_32", "WSASetLastError", fn(i32) -> (), 10014); // WSAEFAULT
        return 0;
    }
    if *ov == STATUS_PENDING {
        api!(
            "ws2_32",
            "WSASetLastError",
            fn(i32) -> (),
            WSA_IO_INCOMPLETE as i32
        );
        return 0;
    }
    *bytes = *ov.add(1) as u32;
    if !flags.is_null() {
        *flags = 0;
    }
    i32::from(*ov == 0)
}

unsafe extern "win64" fn WSALookupServiceBeginW(
    _q: *mut c_void,
    _flags: u32,
    handle: *mut usize,
) -> i32 {
    if !handle.is_null() {
        *handle = 0;
    }
    api!(
        "ws2_32",
        "WSASetLastError",
        fn(i32) -> (),
        WSASERVICE_NOT_FOUND as i32
    );
    SOCKET_ERROR
}

unsafe extern "win64" fn WSALookupServiceNextW(
    _h: usize,
    _flags: u32,
    _len: *mut u32,
    _out: *mut c_void,
) -> i32 {
    api!(
        "ws2_32",
        "WSASetLastError",
        fn(i32) -> (),
        WSA_E_NO_MORE as i32
    );
    SOCKET_ERROR
}

unsafe extern "win64" fn WSALookupServiceEnd(_h: usize) -> i32 {
    0
}

unsafe extern "win64" fn WSASetServiceW(_info: *mut c_void, _op: u32, _flags: u32) -> i32 {
    api!("ws2_32", "WSASetLastError", fn(i32) -> (), 10045); // WSAEOPNOTSUPP
    SOCKET_ERROR
}

pub fn ws2_32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WSAEnumProtocolsW" => fn WSAEnumProtocolsW,
        "WSAGetOverlappedResult" => fn WSAGetOverlappedResult,
        "WSALookupServiceBeginW" => fn WSALookupServiceBeginW,
        "WSALookupServiceNextW" => fn WSALookupServiceNextW,
        "WSALookupServiceEnd" => fn WSALookupServiceEnd,
        "WSASetServiceW" => fn WSASetServiceW,
    }
}

// ============================================================
//  dbghelp, oleaut32, userenv
// ============================================================

unsafe extern "win64" fn SymGetSearchPathW(_process: usize, path: *mut u16, len: u32) -> i32 {
    if !path.is_null() && len > 0 {
        *path = 0;
    }
    1
}

unsafe extern "win64" fn SymSetSearchPathW(_process: usize, _path: *const u16) -> i32 {
    1
}

pub fn dbghelp_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SymGetSearchPathW" => fn SymGetSearchPathW,
        "SymSetSearchPathW" => fn SymSetSearchPathW,
    }
}

/// BSTR : longueur en octets dans le mot qui précède les caractères.
unsafe fn bstr(p: *const u16) -> Vec<u16> {
    if p.is_null() {
        return Vec::new();
    }
    let bytes = *(p as *const u32).sub(1) as usize;
    std::slice::from_raw_parts(p, bytes / 2).to_vec()
}

/// VARCMP_LT 0, EQ 1, GT 2 ; NORM_IGNORECASE = 1.
unsafe extern "win64" fn VarBstrCmp(a: *const u16, b: *const u16, _lcid: u32, flags: u32) -> u32 {
    let (mut x, mut y) = (bstr(a), bstr(b));
    if flags & 1 != 0 {
        x = String::from_utf16_lossy(&x)
            .to_lowercase()
            .encode_utf16()
            .collect();
        y = String::from_utf16_lossy(&y)
            .to_lowercase()
            .encode_utf16()
            .collect();
    }
    match x.cmp(&y) {
        std::cmp::Ordering::Less => 0,
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Greater => 2,
    }
}

/// DISP_E_TYPEMISMATCH / DISP_E_OVERFLOW.
unsafe extern "win64" fn VarUI4FromStr(
    s: *const u16,
    _lcid: u32,
    _flags: u32,
    out: *mut u32,
) -> u32 {
    if s.is_null() || out.is_null() {
        return 0x8007_0057;
    }
    let text = wstr(s);
    match text.trim().parse::<u64>() {
        Ok(v) if v <= u32::MAX as u64 => {
            *out = v as u32;
            0
        }
        Ok(_) => 0x8002_000A,
        Err(_) => 0x8002_0005,
    }
}

pub fn oleaut32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "VarBstrCmp" => fn VarBstrCmp,
        "VarUI4FromStr" => fn VarUI4FromStr,
    }
}

/// Pas de conteneurs d'applis (AppContainer) sous Winlux.
unsafe extern "win64" fn DeriveAppContainerSidFromAppContainerName(
    _name: *const u16,
    sid: *mut usize,
) -> u32 {
    if !sid.is_null() {
        *sid = 0;
    }
    E_NOTIMPL
}

pub fn userenv_exports() -> crate::dlls::Exports {
    crate::exports! {
        "DeriveAppContainerSidFromAppContainerName" => fn DeriveAppContainerSidFromAppContainerName,
    }
}

// ============================================================
//  crypt32, wintrust : catalogues et usages de certificats
// ============================================================

/// CERT_NAME_BLOB { cbData (u32), pbData } : comparaison octet par octet.
unsafe extern "win64" fn CertCompareCertificateName(
    _enc: u32,
    a: *const usize,
    b: *const usize,
) -> i32 {
    if a.is_null() || b.is_null() {
        return 0;
    }
    let blob = |p: *const usize| {
        let n = *(p as *const u32) as usize;
        let data = *p.add(1) as *const u8;
        if data.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(data, n)
        }
    };
    i32::from(blob(a) == blob(b))
}

unsafe extern "win64" fn CertControlStore(
    _store: usize,
    _flags: u32,
    _ctrl: u32,
    _para: *const c_void,
) -> i32 {
    1
}

unsafe extern "win64" fn CertFindChainInStore(
    _store: usize,
    _enc: u32,
    _flags: u32,
    _ty: u32,
    _para: *const c_void,
    _prev: usize,
) -> usize {
    set_last_error(CRYPT_E_NOT_FOUND);
    0
}

/// Aucune restriction d'usage : tous les usages sont permis.
unsafe extern "win64" fn CertGetEnhancedKeyUsage(
    _cert: usize,
    _flags: u32,
    usage: *mut u32,
    len: *mut u32,
) -> i32 {
    if len.is_null() {
        return 0;
    }
    if usage.is_null() {
        *len = 16;
        return 1;
    }
    if *len < 16 {
        *len = 16;
        set_last_error(234); // ERROR_MORE_DATA
        return 0;
    }
    *usage = 0; // cUsageIdentifier
    *(usage.add(2) as *mut usize) = 0;
    set_last_error(CRYPT_E_NOT_FOUND);
    1
}

unsafe extern "win64" fn CertGetIntendedKeyUsage(
    _enc: u32,
    _info: *const c_void,
    _usage: *mut u8,
    _len: u32,
) -> i32 {
    set_last_error(0);
    0
}

unsafe extern "win64" fn CryptMsgClose(_msg: usize) -> i32 {
    1
}

unsafe extern "win64" fn CryptMsgGetParam(
    _msg: usize,
    _ty: u32,
    _index: u32,
    _data: *mut c_void,
    len: *mut u32,
) -> i32 {
    if !len.is_null() {
        *len = 0;
    }
    set_last_error(0x8009_1010); // CRYPT_E_INVALID_MSG_TYPE
    0
}

unsafe extern "win64" fn CryptVerifyCertificateSignatureEx(
    _prov: usize,
    _enc: u32,
    _subject_ty: u32,
    _subject: *const c_void,
    _issuer_ty: u32,
    _issuer: *const c_void,
    _flags: u32,
    _extra: *mut c_void,
) -> i32 {
    set_last_error(0x8009_0006); // NTE_BAD_SIGNATURE
    0
}

pub fn crypt32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CertCompareCertificateName" => fn CertCompareCertificateName,
        "CertControlStore" => fn CertControlStore,
        "CertFindChainInStore" => fn CertFindChainInStore,
        "CertGetEnhancedKeyUsage" => fn CertGetEnhancedKeyUsage,
        "CertGetIntendedKeyUsage" => fn CertGetIntendedKeyUsage,
        "CryptMsgClose" => fn CryptMsgClose,
        "CryptMsgGetParam" => fn CryptMsgGetParam,
        "CryptVerifyCertificateSignatureEx" => fn CryptVerifyCertificateSignatureEx,
    }
}

/// Catalogues de signatures Windows : aucun n'est installé, l'appelant
/// conclut qu'un fichier n'est pas signé par catalogue.
unsafe extern "win64" fn CryptCATAdminCalcHashFromFileHandle(
    _file: usize,
    len: *mut u32,
    _hash: *mut u8,
    _flags: u32,
) -> i32 {
    if !len.is_null() {
        *len = 0;
    }
    set_last_error(ERROR_NOT_SUPPORTED);
    0
}

unsafe extern "win64" fn CryptCATAdminEnumCatalogFromHash(
    _admin: usize,
    _hash: *const u8,
    _len: u32,
    _flags: u32,
    _prev: *mut usize,
) -> usize {
    set_last_error(ERROR_NOT_FOUND);
    0
}

unsafe extern "win64" fn CryptCATAdminReleaseCatalogContext(
    _admin: usize,
    _cat: usize,
    _flags: u32,
) -> i32 {
    1
}

unsafe extern "win64" fn CryptCATAdminReleaseContext(_admin: usize, _flags: u32) -> i32 {
    1
}

unsafe extern "win64" fn CryptCATCatalogInfoFromContext(
    _cat: usize,
    _info: *mut c_void,
    _flags: u32,
) -> i32 {
    set_last_error(ERROR_NOT_FOUND);
    0
}

pub fn wintrust_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CryptCATAdminCalcHashFromFileHandle" => fn CryptCATAdminCalcHashFromFileHandle,
        "CryptCATAdminCalcHashFromFileHandle2" => fn CryptCATAdminCalcHashFromFileHandle,
        "CryptCATAdminEnumCatalogFromHash" => fn CryptCATAdminEnumCatalogFromHash,
        "CryptCATAdminReleaseCatalogContext" => fn CryptCATAdminReleaseCatalogContext,
        "CryptCATAdminReleaseContext" => fn CryptCATAdminReleaseContext,
        "CryptCATCatalogInfoFromContext" => fn CryptCATCatalogInfoFromContext,
    }
}

/// Toutes les tables, par DLL, pour `add_basics`.
pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        ("kernel32", kernel32_exports()),
        // Noms psapi sans K32 : psapi.dll est servie par kernel32.
        ("kernel32", psapi_exports()),
        ("ntdll", ntdll_exports()),
        ("advapi32", advapi32_exports()),
        ("uxtheme", uxtheme_exports()),
        ("user32", user32_exports()),
        ("iphlpapi", iphlpapi_exports()),
        ("ws2_32", ws2_32_exports()),
        ("dbghelp", dbghelp_exports()),
        ("oleaut32", oleaut32_exports()),
        ("userenv", userenv_exports()),
        ("crypt32", crypt32_exports()),
        ("wintrust", wintrust_exports()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acl_valide_ou_non() {
        unsafe {
            // ACL révision 2, une ACE de 20 octets.
            let mut acl = [0u8; 28];
            acl[0] = 2;
            acl[2..4].copy_from_slice(&28u16.to_le_bytes());
            acl[4..6].copy_from_slice(&1u16.to_le_bytes());
            acl[10..12].copy_from_slice(&20u16.to_le_bytes());
            assert_eq!(IsValidAcl(acl.as_ptr()), 1);
            acl[10..12].copy_from_slice(&40u16.to_le_bytes());
            assert_eq!(IsValidAcl(acl.as_ptr()), 0, "ACE qui déborde");
            acl[10..12].copy_from_slice(&20u16.to_le_bytes());
            acl[0] = 9;
            assert_eq!(IsValidAcl(acl.as_ptr()), 0, "mauvaise révision");
            assert_eq!(IsValidAcl(std::ptr::null()), 0);
            let (mut st, mut ck) = (0u32, 0usize);
            assert_eq!(LdrLockLoaderLock(0, &mut st, &mut ck), 0);
            assert_eq!(st, 1);
            assert_ne!(ck, 0);
            assert!(crate::dlls::lookup("uxtheme", "#135").is_some());
            assert!(crate::dlls::lookup("advapi32", "IsValidAcl").is_some());
            // Le DACL rendu pour la window station (bac à sable Chromium) est valide.
            let (mut d, mut sd) = (std::ptr::null_mut(), std::ptr::null_mut());
            let null = std::ptr::null_mut();
            assert_eq!(
                crate::dlls::security::GetSecurityInfo(
                    0x30, 7, 4, null, null, &mut d, null, &mut sd
                ),
                0
            );
            assert!(d.is_null() || IsValidAcl(d) == 1);
        }
    }

    #[test]
    fn handle_vers_soi_et_arret_immediat() {
        use crate::dlls::kernel32 as k;
        unsafe {
            let me = k::get_current_process();
            let mut dup = 0usize;
            assert_eq!(k::duplicate_handle(me, me, me, &mut dup, 0, 0, 2), 1);
            assert_ne!(
                dup,
                usize::MAX,
                "pas le pseudo-handle (INVALID_HANDLE_VALUE)"
            );
            assert_eq!(k::get_process_id(dup), std::process::id());
            assert!(k::is_current_process_handle(dup));
            k::close_handle(dup);
            // TerminateProcess(GetCurrentProcess()) termine sans rendre la main.
            let pid = libc::fork();
            if pid == 0 {
                k::terminate_process(me, 7);
                libc::_exit(99);
            }
            let mut st = 0;
            assert_eq!(libc::waitpid(pid, &mut st, 0), pid);
            assert!(libc::WIFEXITED(st));
            assert_eq!(libc::WEXITSTATUS(st), 7);
        }
    }

    #[test]
    fn station_bureau_et_securite_crashpad() {
        use crate::dlls::kernel32 as k;
        unsafe {
            let ws = CreateWindowStation(0, 0, 0, 0);
            assert_ne!(ws, 0);
            assert_ne!(CreateDesktop(0, 0, 0, 0, 0, 0, 0, 0), 0);
            assert_eq!(GetThreadDesktop(1), DEFAULT_DESKTOP);
            assert!(crate::dlls::lookup("user32", "CreateWindowStationW").is_some());
            // OpenProcess sur soi : vrai handle héritable, jamais -1.
            let me = k::open_process(0x1fffff, 1, std::process::id());
            assert_ne!(me, usize::MAX);
            assert_ne!(me, 0);
            assert!(k::is_current_process_handle(me));
            assert!(crate::dlls::inherit::is(me));
            // InitializeCriticalSectionEx avec RTL_CRITICAL_SECTION_FLAG_FORCE_DEBUG_INFO.
            let mut cs = [0u64; 5];
            assert_eq!(
                k::initialize_critical_section_ex(cs.as_mut_ptr().cast(), 0, 0x1000_0000),
                1
            );
            assert_eq!(
                k::initialize_critical_section_ex(cs.as_mut_ptr().cast(), 0, 0x1),
                0
            );
            // BuildSecurityDescriptorW (crashpad, tube nommé).
            let f = crate::dlls::lookup("advapi32", "BuildSecurityDescriptorW").unwrap();
            type Build = unsafe extern "win64" fn(
                usize,
                usize,
                u32,
                usize,
                u32,
                usize,
                usize,
                *mut u32,
                *mut usize,
            ) -> u32;
            let build: Build = std::mem::transmute(f);
            let (mut size, mut sd) = (0u32, 0usize);
            assert_eq!(build(0, 0, 0, 0, 0, 0, 0, &mut size, &mut sd), 0);
            assert_eq!(size, 20);
            assert_ne!(sd, 0);
        }
    }

    #[test]
    fn dhcp_sans_option() {
        unsafe {
            let mut v = 0;
            assert_eq!(DhcpCApiInitialize(&mut v), 0);
            assert_eq!(v, 2);
            let mut size = 99;
            let r = DhcpRequestParams(
                0,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut size,
                std::ptr::null(),
            );
            assert_eq!((r, size), (ERROR_NOT_SUPPORTED, 0));
        }
    }

    #[test]
    fn luid_et_nom_d_interface() {
        unsafe {
            let mut luid = 0u64;
            assert_eq!(ConvertInterfaceIndexToLuid(3, &mut luid), 0);
            let mut name = [0u16; 32];
            assert_eq!(ConvertInterfaceLuidToNameW(&luid, name.as_mut_ptr(), 32), 0);
            assert_eq!(String::from_utf16_lossy(&name[..10]), "ethernet_3");
            assert_eq!(
                ConvertInterfaceLuidToNameW(&luid, name.as_mut_ptr(), 4),
                ERROR_INSUFFICIENT_BUFFER
            );
        }
    }

    #[test]
    fn temps_processeur_croissant() {
        unsafe {
            let (mut a, mut b) = (0u64, 0u64);
            assert_eq!(QueryThreadCycleTime(0, &mut a), 1);
            let mut x = 0u64;
            for i in 0..200_000u64 {
                x = x.wrapping_mul(31).wrapping_add(i);
            }
            std::hint::black_box(x);
            assert_eq!(QueryThreadCycleTime(0, &mut b), 1);
            assert!(b > a);
        }
    }

    #[test]
    fn chaines_bstr_et_nombres() {
        // BSTR fabriqués à la main : longueur (octets) puis caractères.
        let mk = |s: &str| {
            let w: Vec<u16> = s.encode_utf16().collect();
            let mut v = vec![0u16; 2];
            v[0] = (w.len() * 2) as u16;
            v.extend(w);
            v.push(0);
            v
        };
        let (a, b) = (mk("Bonjour"), mk("bonjour"));
        unsafe {
            assert_eq!(VarBstrCmp(a.as_ptr().add(2), b.as_ptr().add(2), 0, 0), 0);
            assert_eq!(VarBstrCmp(a.as_ptr().add(2), b.as_ptr().add(2), 0, 1), 1);
            let n: Vec<u16> = "4294967295\0".encode_utf16().collect();
            let mut out = 0;
            assert_eq!(VarUI4FromStr(n.as_ptr(), 0, 0, &mut out), 0);
            assert_eq!(out, u32::MAX);
            let big: Vec<u16> = "4294967296\0".encode_utf16().collect();
            assert_eq!(VarUI4FromStr(big.as_ptr(), 0, 0, &mut out), 0x8002_000A);
        }
    }

    #[test]
    fn performance_et_ensemble_de_travail() {
        unsafe {
            let mut info = [0u8; 104];
            assert_eq!(K32GetPerformanceInfo(info.as_mut_ptr(), 104), 1);
            let page = (info.as_ptr().add(8 + 9 * 8) as *const usize).read_unaligned();
            assert_eq!(page, 4096);
            let local = 7u64;
            let mut q = [&local as *const u64 as usize, 99usize];
            assert_eq!(K32QueryWorkingSetEx(usize::MAX, q.as_mut_ptr(), 16), 1);
            assert_eq!(q[1], 1, "la pile est présente en mémoire");
        }
    }
}
