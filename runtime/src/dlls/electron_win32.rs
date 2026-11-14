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
const E_NOTIMPL: u32 = 0x8000_4001;
const CRYPT_E_NOT_FOUND: u32 = 0x8009_2004;
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
/// `QueryWorkingSetEx` : drapeau « valide » de chaque adresse demandée ;
/// pour un autre processus, d'après son `/proc/<pid>/pagemap` (#1729).
unsafe extern "win64" fn K32QueryWorkingSetEx(process: usize, buf: *mut usize, cb: u32) -> i32 {
    if buf.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let pagemap =
        if process == usize::MAX || crate::dlls::kernel32::is_current_process_handle(process) {
            None
        } else {
            let Some(pid) = crate::dlls::kernel32::process_os_pid(process) else {
                set_last_error(6); // ERROR_INVALID_HANDLE
                return 0;
            };
            match std::fs::File::open(format!("/proc/{pid}/pagemap")) {
                Ok(f) => Some(f),
                Err(_) => {
                    set_last_error(5); // ERROR_ACCESS_DENIED
                    return 0;
                }
            }
        };
    // PSAPI_WORKING_SET_EX_INFORMATION : adresse et attributs, deux
    // pointeurs (8 octets en x86, 16 en x64).
    let ps = crate::wow32::pointer_size();
    for i in 0..cb as usize / (2 * ps) {
        let entry = buf as usize + i * 2 * ps;
        let addr = crate::wow32::read_ptr(entry) & !0xFFF;
        let valid = match &pagemap {
            Some(f) => crate::dlls::psapi_k32::pagemap_present(f, addr),
            None => {
                let mut present = 0u8;
                libc::mincore(addr as *mut c_void, 4096, &mut present) == 0 && present & 1 != 0
            }
        };
        crate::wow32::write_ptr(entry + ps, usize::from(valid));
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

unsafe extern "win64" fn VirtualFreeEx(
    process: usize,
    addr: *mut c_void,
    size: usize,
    kind: u32,
) -> i32 {
    if !is_self(process) {
        if let Some(r) = crate::dlls::games_win32::foreign_free(process, addr as usize, size, kind)
        {
            return r;
        }
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

/// TransactNamedPipe : écrit un message sur le tube puis lit la réponse
/// (client crashpad → gestionnaire de plantage). Avec OVERLAPPED,
/// l'opération est faite tout de suite et l'OVERLAPPED est terminé.
unsafe extern "win64" fn TransactNamedPipe(
    pipe: usize,
    input: *const u8,
    in_len: u32,
    output: *mut u8,
    out_len: u32,
    read: *mut u32,
    ov: usize,
) -> i32 {
    use crate::dlls::kernel32 as k;
    let mut written = 0u32;
    if k::write_file(pipe, input, in_len, &mut written, 0) == 0 {
        return 0;
    }
    let mut got = 0u32;
    let ok = k::read_file(pipe, output, out_len, &mut got, 0);
    if !read.is_null() {
        *read = got;
    }
    if ov != 0 {
        let err = if ok == 0 { k::get_last_error() } else { 0 };
        k::complete_overlapped(pipe, ov, got, err);
    }
    ok
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
        "TerminateJobObject" => fn super::jobs::terminate_job_object,
        "VirtualFreeEx" => fn VirtualFreeEx,
        "TransactNamedPipe" => fn TransactNamedPipe,
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

unsafe extern "win64" fn NtDeleteKey(key: usize) -> u32 {
    crate::dlls::ntdll::nt_delete_key(key) as u32
}

/// `NtQueryObject` : informations de base (0), nom (1), type (2) et
/// indicateurs (4) d'un handle. Chromium vérifie ainsi le type des handles
/// qu'un enfant reçoit par héritage (canal Mojo : « File », mémoire
/// partagée : « Section ») et s'arrêtait sur un CHECK tant que la fonction
/// répondait « non implémentée ».
/// Droits d'un handle de fichier d'après le mode d'ouverture de son
/// descripteur : FILE_GENERIC_READ, FILE_GENERIC_WRITE ou les deux.
fn file_access(h: usize, kind: &str) -> Option<u32> {
    const FILE_GENERIC_READ: u32 = 0x0012_0089;
    const FILE_GENERIC_WRITE: u32 = 0x0012_0116;
    if kind != "File" {
        return None;
    }
    let fd = crate::dlls::kernel32::fd_of_handle(h)?;
    // SAFETY : simple lecture des drapeaux d'un descripteur.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return None;
    }
    Some(match flags & libc::O_ACCMODE {
        libc::O_RDONLY => FILE_GENERIC_READ,
        libc::O_WRONLY => FILE_GENERIC_WRITE,
        _ => FILE_GENERIC_READ | FILE_GENERIC_WRITE,
    })
}

unsafe extern "win64" fn NtQueryObject(
    h: usize,
    class: u32,
    info: *mut c_void,
    len: u32,
    ret: *mut u32,
) -> u32 {
    const STATUS_INVALID_HANDLE: u32 = 0xC000_0008;
    const STATUS_INFO_LENGTH_MISMATCH: u32 = 0xC000_0004;
    const STATUS_INVALID_INFO_CLASS: u32 = 0xC000_0003;
    let Some(kind) = crate::dlls::kernel32::object_type_name(h) else {
        if !ret.is_null() {
            *ret = 0;
        }
        return STATUS_INVALID_HANDLE;
    };
    let inherit = crate::dlls::inherit::is(h);
    // Programme x86 : UNICODE_STRING de 8 octets (Buffer à 4) et
    // OBJECT_TYPE_INFORMATION de 0x60 octets (TypeName + 22 ULONG,
    // PUBLIC_OBJECT_TYPE_INFORMATION de winternl.h), comme pour
    // NtQuerySection (#1650).
    let x86 = crate::dlls::legacy_com::x86();
    let us = if x86 { 8 } else { 16 };
    let mut bytes: Vec<u8>;
    // (position d'un UNICODE_STRING, position de son texte dans `bytes`)
    let mut string: Option<(usize, usize, usize)> = None;
    match class {
        0 => {
            // OBJECT_BASIC_INFORMATION (56 octets).
            bytes = vec![0u8; 56];
            bytes[0..4].copy_from_slice(&(if inherit { 2u32 } else { 0 }).to_le_bytes());
            // GrantedAccess : les vrais droits d'une section ; pour un
            // fichier, ceux de son mode d'ouverture (Chromium vérifie qu'un
            // fichier de police transmis au moteur de rendu est en lecture
            // seule, #996) ; sinon tous.
            let granted = crate::dlls::mmap::mapping_access(h)
                .or_else(|| file_access(h, kind))
                .unwrap_or(0x001F_01FF);
            bytes[4..8].copy_from_slice(&granted.to_le_bytes());
            bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
            bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
        }
        1 | 2 => {
            // UNICODE_STRING (16 octets) suivi, pour le type, du reste
            // d'OBJECT_TYPE_INFORMATION (0x68 octets en tout), puis le texte.
            let text = if class == 2 {
                kind.to_string()
            } else if kind == "File" {
                crate::dlls::kernel32::fd_of_handle(h)
                    .and_then(|fd| std::fs::read_link(format!("/proc/self/fd/{fd}")).ok())
                    .filter(|p| p.is_absolute())
                    .map(|p| {
                        let w = crate::paths::to_windows(&p);
                        format!("\\??\\{w}")
                    })
                    .unwrap_or_default()
            } else if kind == "Key" {
                crate::registry::key_nt_name(h).unwrap_or_default()
            } else {
                String::new()
            };
            let head = if class == 2 { us + 88 } else { us };
            bytes = vec![0u8; head];
            let wide: Vec<u8> = text
                .encode_utf16()
                .chain(Some(0))
                .flat_map(|c| c.to_le_bytes())
                .collect();
            let at = bytes.len();
            let chars = wide.len() / 2 - 1;
            bytes.extend_from_slice(&wide);
            string = Some((0, at, chars));
            if class == 2 {
                // ValidAccessMask, PoolType (Paged = 1).
                bytes[us + 64..us + 68].copy_from_slice(&0x001F_01FFu32.to_le_bytes());
                bytes[us + 76..us + 80].copy_from_slice(&1u32.to_le_bytes());
            }
        }
        4 => {
            // OBJECT_HANDLE_FLAG_INFORMATION : Inherit, ProtectFromClose.
            bytes = vec![u8::from(inherit), 0];
        }
        _ => {
            if !ret.is_null() {
                *ret = 0;
            }
            return STATUS_INVALID_INFO_CLASS;
        }
    }
    if !ret.is_null() {
        *ret = bytes.len() as u32;
    }
    if info.is_null() || (len as usize) < bytes.len() {
        return STATUS_INFO_LENGTH_MISMATCH;
    }
    let out = info as *mut u8;
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    if let Some((at, text, chars)) = string {
        let n = (chars * 2) as u16;
        (out.add(at) as *mut u16).write_unaligned(n);
        (out.add(at + 2) as *mut u16).write_unaligned(n + 2);
        if x86 {
            (out.add(at + 4) as *mut u32).write_unaligned((out as usize + text) as u32);
        } else {
            (out.add(at + 8) as *mut usize).write_unaligned(out as usize + text);
        }
    }
    0
}

unsafe extern "win64" fn RtlGetLastNtStatus() -> u32 {
    0
}

/// `NtQuerySection` : SectionBasicInformation (classe 0, 24 octets :
/// BaseAddress, AllocationAttributes, MaximumSize) pour les mappings de
/// CreateFileMapping. Chromium (PlatformSharedMemoryRegion) lit ainsi la
/// taille d'une mémoire partagée reçue et appelle la fonction sans vérifier
/// qu'elle existe : son absence faisait sauter Claude en 0.
unsafe extern "win64" fn NtQuerySection(
    h: usize,
    class: u32,
    info: *mut u8,
    len: usize,
    ret: *mut usize,
) -> u32 {
    query_section(h, class, info, len, ret, crate::dlls::legacy_com::x86())
}

unsafe fn query_section(
    h: usize,
    class: u32,
    info: *mut u8,
    len: usize,
    ret: *mut usize,
    x86: bool,
) -> u32 {
    const STATUS_INVALID_HANDLE: u32 = 0xC000_0008;
    const STATUS_INFO_LENGTH_MISMATCH: u32 = 0xC000_0004;
    const STATUS_SECTION_NOT_IMAGE: u32 = 0xC000_0049;
    const STATUS_INVALID_INFO_CLASS: u32 = 0xC000_0003;
    const SEC_COMMIT: u32 = 0x0800_0000;
    let Some((_, size, _)) = crate::dlls::mmap::export(h) else {
        crate::trace!("NtQuerySection(0x{h:x}) : handle inconnu");
        return STATUS_INVALID_HANDLE;
    };
    crate::trace!("NtQuerySection(0x{h:x}, classe {class}, {len} octets) : taille {size}");
    // Programme x86 : SECTION_BASIC_INFORMATION fait 16 octets (pointeur de
    // 4 octets, MaximumSize à 8) et le SIZE_T rendu en fait 4. steam.exe y
    // lit la taille du flux partagé avec steamwebhelper ; avec la
    // disposition 64 bits il obtenait STATUS_INFO_LENGTH_MISMATCH et une
    // taille 0, et l'écriture de 8 octets débordait de son SIZE_T (#1650).
    let (total, attrs_at, size_at) = if x86 { (16, 4, 8) } else { (24, 8, 16) };
    match class {
        0 => {
            if !ret.is_null() {
                if total == 16 {
                    (ret as *mut u32).write_unaligned(16);
                } else {
                    ret.write_unaligned(24);
                }
            }
            if info.is_null() || len < total {
                return STATUS_INFO_LENGTH_MISMATCH;
            }
            std::ptr::write_bytes(info, 0, total);
            put_u32(info, attrs_at, SEC_COMMIT);
            (info.add(size_at) as *mut u64).write_unaligned(size);
            0
        }
        1 => STATUS_SECTION_NOT_IMAGE,
        _ => STATUS_INVALID_INFO_CLASS,
    }
}

pub fn ntdll_exports() -> crate::dlls::Exports {
    crate::exports! {
        "NtQuerySection" => fn NtQuerySection,
        "ZwQuerySection" => fn NtQuerySection,
        "NtDeleteKey" => fn NtDeleteKey,
        "ZwDeleteKey" => fn NtDeleteKey,
        "NtQueryObject" => fn NtQueryObject,
        "ZwQueryObject" => fn NtQueryObject,
        "RtlGetLastNtStatus" => fn RtlGetLastNtStatus,
        "NtSuspendProcess" => fn NtSuspendProcess,
        "NtResumeProcess" => fn NtResumeProcess,
        "RtlGetUnloadEventTraceEx" => fn RtlGetUnloadEventTraceEx,
        "LdrLockLoaderLock" => fn LdrLockLoaderLock,
        "LdrUnlockLoaderLock" => fn LdrUnlockLoaderLock,
    }
}

/// NtSuspendProcess / NtResumeProcess : SIGSTOP / SIGCONT au processus
/// Linux (Crashpad fige ainsi un processus avant de lire sa mémoire).
unsafe fn signal_process(h: usize, sig: i32) -> u32 {
    match crate::dlls::kernel32::process_os_pid(h) {
        Some(pid) if libc::kill(pid, sig) == 0 => 0,
        Some(_) => 0xC000_0022, // STATUS_ACCESS_DENIED
        None => 0xC000_0008,    // STATUS_INVALID_HANDLE
    }
}

/// Processus figés par ce processus (NtSuspendProcess) et pas encore
/// relancés. Crashpad fige le programme qu'il vide ; s'il plante pendant
/// le vidage, le programme restait figé pour toujours (état T, enfants
/// Chromium bloqués, « Quitter » sans effet, #1787) : le gestionnaire de
/// plantage les relance (`resume_suspended`). Tableau fixe d'atomiques,
/// lisible depuis un gestionnaire de signal.
use std::sync::atomic::{AtomicI32, Ordering};

static SUSPENDED: [AtomicI32; 16] = [const { AtomicI32::new(0) }; 16];

unsafe extern "win64" fn NtSuspendProcess(h: usize) -> u32 {
    let r = signal_process(h, libc::SIGSTOP);
    if r == 0 {
        if let Some(pid) = crate::dlls::kernel32::process_os_pid(h) {
            let _ = SUSPENDED.iter().find(|s| {
                s.compare_exchange(0, pid, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
            });
        }
    }
    r
}

unsafe extern "win64" fn NtResumeProcess(h: usize) -> u32 {
    let r = signal_process(h, libc::SIGCONT);
    if r == 0 {
        if let Some(pid) = crate::dlls::kernel32::process_os_pid(h) {
            let _ = SUSPENDED.iter().find(|s| {
                s.compare_exchange(pid, 0, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
            });
        }
    }
    r
}

/// Trace des modules déchargés (RTL_UNLOAD_EVENT_TRACE, 0x68 octets en
/// 64 bits) : Winlux n'en tient pas, la trace est vide. Crashpad obtient
/// cette fonction par GetProcAddress sans vérifier le résultat et appelait
/// une adresse nulle en vidant un processus (#1797). Signature : Microsoft
/// Learn, « RtlGetUnloadEventTraceEx ».
static UNLOAD_TRACE_SIZE: u32 = 0x68;
static UNLOAD_TRACE_COUNT: u32 = 0;
static UNLOAD_TRACE_ARRAY: [u8; 0x68] = [0; 0x68];
/// Pointeur vers le tableau : Crashpad lit ce pointeur, puis le tableau.
static UNLOAD_TRACE: &[u8; 0x68] = &UNLOAD_TRACE_ARRAY;

unsafe extern "win64" fn RtlGetUnloadEventTraceEx(
    element_size: *mut *const u32,
    element_count: *mut *const u32,
    event_trace: *mut *const c_void,
) {
    if !element_size.is_null() {
        element_size.write_unaligned(&UNLOAD_TRACE_SIZE);
    }
    if !element_count.is_null() {
        element_count.write_unaligned(&UNLOAD_TRACE_COUNT);
    }
    if !event_trace.is_null() {
        event_trace.write_unaligned(&UNLOAD_TRACE as *const _ as *const c_void);
    }
}

/// Relance (SIGCONT) les processus que ce processus a figés sans les
/// relancer. Appelé par le gestionnaire de plantage : uniquement des
/// opérations sûres dans un signal (atomiques, kill). Rend leur nombre.
pub(crate) fn resume_suspended() -> usize {
    let mut n = 0;
    for slot in &SUSPENDED {
        let pid = slot.swap(0, Ordering::AcqRel);
        if pid > 0 {
            // SAFETY : kill est sûr dans un gestionnaire de signal.
            unsafe { libc::kill(pid, libc::SIGCONT) };
            n += 1;
        }
    }
    n
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

/// TRUSTEE_W { pMultipleTrustee, MultipleTrusteeOperation, TrusteeForm,
/// TrusteeType, ptstrName } : forme 0 = SID, 1 = nom.
unsafe fn build_trustee(trustee: *mut u8, form: u32, value: usize) {
    if trustee.is_null() {
        return;
    }
    // 20 octets en x86, 32 en x64 (#1915).
    let (_, lay) = crate::dlls::security::trustee_layout(crate::wow32::pointer_size());
    std::ptr::write_bytes(trustee, 0, lay.size);
    (trustee.add(lay.form) as *mut u32).write_unaligned(form);
    if lay.size == 20 {
        (trustee.add(lay.name) as *mut u32).write_unaligned(value as u32);
    } else {
        (trustee.add(lay.name) as *mut usize).write_unaligned(value);
    }
}

unsafe extern "win64" fn BuildTrusteeWithSid(trustee: *mut u8, sid: usize) {
    build_trustee(trustee, 0, sid);
}

unsafe extern "win64" fn BuildTrusteeWithName(trustee: *mut u8, name: usize) {
    build_trustee(trustee, 1, name);
}

pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "MapGenericMask" => fn MapGenericMask,
        "AccessCheck" => fn AccessCheck,
        "BuildTrusteeWithSidW" => fn BuildTrusteeWithSid,
        "BuildTrusteeWithSidA" => fn BuildTrusteeWithSid,
        "BuildTrusteeWithNameW" => fn BuildTrusteeWithName,
        "BuildTrusteeWithNameA" => fn BuildTrusteeWithName,
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
/// Station « WinSta0 » (GetProcessWindowStation, user32.rs).
const WINSTA0: usize = 0x5753_0001;

/// DuplicateHandle d'un handle de station ou de bureau (#2443) : la page
/// de DuplicateHandle compte « Desktop » et « Window station » parmi les
/// objets duplicables. WinSta0 et le bureau Default sont des constantes,
/// rendues telles quelles ; un objet créé reçoit un nouveau numéro, que
/// la fermeture de l'original n'invalide pas. None : pas un tel handle.
pub(crate) fn duplicate_user_object(h: usize) -> Option<usize> {
    if h == WINSTA0 || h == DEFAULT_DESKTOP {
        return Some(h);
    }
    let next = NEXT_USER_OBJECT.load(std::sync::atomic::Ordering::Relaxed);
    (h >= 0x5753_0100 && h < next && (h - 0x5753_0100).is_multiple_of(4)).then(new_user_object)
}

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

/// `GetPointerDevices` : aucun écran tactile ni stylet déclaré au sens de
/// Windows Pointer (le toucher arrive par la souris d'X11).
unsafe extern "win64" fn GetPointerDevices(count: *mut u32, _devices: *mut u8) -> i32 {
    if count.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *count = 0;
    1
}

// ============================================================
//  wtsapi32 : notifications de changement de session
// ============================================================
//
// Chromium (WtsSessionChangeObserver) inscrit sa fenêtre de messages et
// vérifie le résultat par un CHECK. Winlux n'a qu'une session console :
// l'inscription réussit et aucun WM_WTSSESSION_CHANGE n'est émis pour
// l'instant (le verrouillage de session pourra les envoyer plus tard).

unsafe extern "win64" fn WTSRegisterSessionNotification(hwnd: usize, _flags: u32) -> i32 {
    if hwnd == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    1
}

unsafe extern "win64" fn WTSUnRegisterSessionNotification(_hwnd: usize) -> i32 {
    1
}

unsafe extern "win64" fn WTSRegisterSessionNotificationEx(
    _server: usize,
    hwnd: usize,
    flags: u32,
) -> i32 {
    WTSRegisterSessionNotification(hwnd, flags)
}

unsafe extern "win64" fn WTSUnRegisterSessionNotificationEx(_server: usize, hwnd: usize) -> i32 {
    WTSUnRegisterSessionNotification(hwnd)
}

pub fn wtsapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WTSRegisterSessionNotification" => fn WTSRegisterSessionNotification,
        "WTSUnRegisterSessionNotification" => fn WTSUnRegisterSessionNotification,
        "WTSRegisterSessionNotificationEx" => fn WTSRegisterSessionNotificationEx,
        "WTSUnRegisterSessionNotificationEx" => fn WTSUnRegisterSessionNotificationEx,
    }
}

// ============================================================
//  user32 : API de configuration d'affichage (QueryDisplayConfig)
// ============================================================
//
// Chromium (display::win::ScreenWin) lit ainsi le nom et la fréquence de
// chaque écran et s'arrête sur un CHECK quand GetDisplayConfigBufferSizes
// manque. Un chemin par écran de EnumDisplayMonitors, dans le même ordre
// (\\.\DISPLAY1 = principal), avec un mode source et un mode cible chacun.

const DC_ADAPTER_LUID: u32 = 0x0057_4c58; // « WLX »
const DC_TARGET_BASE: u32 = 0x100;
const DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER: u32 = 0xFFFF_FFFF;
const ERROR_INSUFFICIENT_BUFFER_L: i32 = 122;
const ERROR_INVALID_PARAMETER_L: i32 = 87;
const ERROR_NOT_SUPPORTED_L: i32 = 50;

struct DcScreen {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    hz: u32,
}

fn dc_screens() -> Vec<DcScreen> {
    let monitors = crate::dlls::monitors::list();
    let modes = crate::dlls::displaymode::ordered(crate::dlls::displaymode::screens());
    monitors
        .iter()
        .enumerate()
        .map(|(i, m)| DcScreen {
            left: m.rect.left,
            top: m.rect.top,
            width: (m.rect.right - m.rect.left).max(1) as u32,
            height: (m.rect.bottom - m.rect.top).max(1) as u32,
            hz: modes
                .get(i)
                .and_then(crate::dlls::displaymode::current)
                .map(|(_, _, hz)| hz)
                .filter(|hz| *hz > 0)
                .unwrap_or(60),
        })
        .collect()
}

unsafe fn put_u32(p: *mut u8, off: usize, v: u32) {
    (p.add(off) as *mut u32).write_unaligned(v);
}

unsafe fn put_wide(p: *mut u8, off: usize, chars: usize, text: &str) {
    let dst = p.add(off) as *mut u16;
    let w: Vec<u16> = text.encode_utf16().take(chars - 1).collect();
    for i in 0..chars {
        dst.add(i).write_unaligned(*w.get(i).unwrap_or(&0));
    }
}

/// `GetDisplayConfigBufferSizes` : un chemin et deux modes par écran.
unsafe extern "win64" fn GetDisplayConfigBufferSizes(
    _flags: u32,
    paths: *mut u32,
    modes: *mut u32,
) -> i32 {
    if paths.is_null() || modes.is_null() {
        return ERROR_INVALID_PARAMETER_L;
    }
    let n = crate::dlls::monitors::list().len() as u32;
    *paths = n;
    *modes = 2 * n;
    0
}

/// `QueryDisplayConfig` : DISPLAYCONFIG_PATH_INFO (72 octets) et
/// DISPLAYCONFIG_MODE_INFO (64 octets), mode source 2i, mode cible 2i+1.
unsafe extern "win64" fn QueryDisplayConfig(
    _flags: u32,
    npaths: *mut u32,
    paths: *mut u8,
    nmodes: *mut u32,
    modes: *mut u8,
    topology: *mut u32,
) -> i32 {
    if npaths.is_null() || nmodes.is_null() || paths.is_null() || modes.is_null() {
        return ERROR_INVALID_PARAMETER_L;
    }
    let list = dc_screens();
    let n = list.len() as u32;
    if *npaths < n || *nmodes < 2 * n {
        return ERROR_INSUFFICIENT_BUFFER_L;
    }
    std::ptr::write_bytes(paths, 0, 72 * n as usize);
    std::ptr::write_bytes(modes, 0, 64 * 2 * n as usize);
    for (i, sc) in list.iter().enumerate() {
        let i32_ = i as u32;
        let p = paths.add(72 * i);
        // sourceInfo
        put_u32(p, 0, DC_ADAPTER_LUID);
        put_u32(p, 8, i32_);
        put_u32(p, 12, 2 * i32_);
        put_u32(p, 16, 1); // DISPLAYCONFIG_SOURCE_IN_USE
                           // targetInfo
        put_u32(p, 20, DC_ADAPTER_LUID);
        put_u32(p, 28, DC_TARGET_BASE + i32_);
        put_u32(p, 32, 2 * i32_ + 1);
        put_u32(p, 36, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER);
        put_u32(p, 40, 1); // DISPLAYCONFIG_ROTATION_IDENTITY
        put_u32(p, 44, 1); // DISPLAYCONFIG_SCALING_IDENTITY
        put_u32(p, 48, sc.hz);
        put_u32(p, 52, 1);
        put_u32(p, 56, 1); // DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE
        put_u32(p, 60, 1); // targetAvailable
        put_u32(p, 64, 1); // DISPLAYCONFIG_TARGET_IN_USE
        put_u32(p, 68, 1); // DISPLAYCONFIG_PATH_ACTIVE
                           // Mode source : définition, 32 bits par pixel, position.
        let m = modes.add(64 * 2 * i);
        put_u32(m, 0, 1); // DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
        put_u32(m, 4, i32_);
        put_u32(m, 8, DC_ADAPTER_LUID);
        put_u32(m, 16, sc.width);
        put_u32(m, 20, sc.height);
        put_u32(m, 24, 4); // DISPLAYCONFIG_PIXELFORMAT_32BPP
        put_u32(m, 28, sc.left as u32);
        put_u32(m, 32, sc.top as u32);
        // Mode cible : signal vidéo.
        let t = modes.add(64 * (2 * i + 1));
        put_u32(t, 0, 2); // DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
        put_u32(t, 4, DC_TARGET_BASE + i32_);
        put_u32(t, 8, DC_ADAPTER_LUID);
        let pixel_rate = u64::from(sc.width) * u64::from(sc.height) * u64::from(sc.hz);
        (t.add(16) as *mut u64).write_unaligned(pixel_rate);
        put_u32(t, 24, sc.height * sc.hz);
        put_u32(t, 28, 1);
        put_u32(t, 32, sc.hz);
        put_u32(t, 36, 1);
        put_u32(t, 40, sc.width);
        put_u32(t, 44, sc.height);
        put_u32(t, 48, sc.width);
        put_u32(t, 52, sc.height);
        put_u32(t, 56, 255); // D3DKMDT_VSS_OTHER
        put_u32(t, 60, 1);
    }
    *npaths = n;
    *nmodes = 2 * n;
    if !topology.is_null() {
        *topology = 1; // DISPLAYCONFIG_TOPOLOGY_INTERNAL
    }
    0
}

/// `DisplayConfigGetDeviceInfo` : noms des sources (\\.\DISPLAYn) et des
/// cibles, couleur standard (pas de HDR), blanc SDR de 80 nits.
unsafe extern "win64" fn DisplayConfigGetDeviceInfo(hdr: *mut u8) -> i32 {
    if hdr.is_null() {
        return ERROR_INVALID_PARAMETER_L;
    }
    let kind = (hdr as *const u32).read_unaligned();
    let size = (hdr.add(4) as *const u32).read_unaligned() as usize;
    let id = (hdr.add(16) as *const u32).read_unaligned();
    let n = crate::dlls::monitors::list().len() as u32;
    let need = match kind {
        1 => 84,
        2 => 420,
        4 => 276,
        6 => 24,
        9 => 32,
        11 => 24,
        _ => return ERROR_NOT_SUPPORTED_L,
    };
    if size < need {
        return ERROR_INVALID_PARAMETER_L;
    }
    let index = match kind {
        1 => id,
        4 => 0,
        _ => id.wrapping_sub(DC_TARGET_BASE),
    };
    if index >= n.max(1) {
        return ERROR_INVALID_PARAMETER_L;
    }
    std::ptr::write_bytes(hdr.add(20), 0, need - 20);
    let number = index + 1;
    match kind {
        1 => put_wide(hdr, 20, 32, &format!("\\\\.\\DISPLAY{number}")),
        2 => {
            put_u32(hdr, 24, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER);
            put_u32(hdr, 32, number);
            put_wide(hdr, 36, 64, &format!("Winlux {number}"));
            put_wide(
                hdr,
                164,
                128,
                &format!(
                    "\\\\?\\DISPLAY#WLX0000#{number}&0#{{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}}"
                ),
            );
        }
        4 => put_wide(hdr, 20, 128, "\\\\?\\WINLUX#DISPLAY#0"),
        6 => put_u32(hdr, 20, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_OTHER),
        9 => put_u32(hdr, 28, 8),     // 8 bits par canal, ni HDR ni WCG
        11 => put_u32(hdr, 20, 1000), // 1000 = 80 nits
        _ => {}
    }
    0
}

unsafe extern "win64" fn SetDisplayConfig(
    _np: u32,
    _paths: usize,
    _nm: u32,
    _modes: usize,
    _flags: u32,
) -> i32 {
    ERROR_NOT_SUPPORTED_L
}

unsafe extern "win64" fn DisplayConfigSetDeviceInfo(_hdr: usize) -> i32 {
    ERROR_NOT_SUPPORTED_L
}

// Fonctions de fenêtre de Windows 8 à 11 que Chromium appelle au démarrage
// du navigateur : réponses neutres d'un poste à un seul utilisateur, sans
// écran tactile déclaré.

static POWER_NOTIFY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x5000);

/// `RegisterPowerSettingNotification` : inscription acceptée (handle
/// propre à chaque appel), aucun WM_POWERBROADCAST de réglage émis.
unsafe extern "win64" fn RegisterPowerSettingNotification(
    recipient: usize,
    _guid: usize,
    _flags: u32,
) -> usize {
    if recipient == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    POWER_NOTIFY.fetch_add(4, std::sync::atomic::Ordering::Relaxed)
}

unsafe extern "win64" fn UnregisterPowerSettingNotification(_h: usize) -> i32 {
    1
}

unsafe extern "win64" fn WindowOk(_hwnd: usize) -> i32 {
    1
}

unsafe extern "win64" fn WindowOk2(_hwnd: usize, _arg: u32) -> i32 {
    1
}

/// Réglages d'affichage posés par `SetWindowDisplayAffinity`, par HWND.
/// Un HWND n'est jamais réattribué (`gui::alloc_hwnd`), une entrée d'une
/// fenêtre détruite est retirée à la lecture suivante.
fn affinites() -> &'static std::sync::Mutex<std::collections::HashMap<usize, u32>> {
    static A: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, u32>>> =
        std::sync::OnceLock::new();
    A.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

const WDA_NONE: u32 = 0;
const WDA_MONITOR: u32 = 1;
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
const ERROR_INVALID_WINDOW_HANDLE: u32 = 1400;

/// `SetWindowDisplayAffinity` (winuser.h, Windows 7) : le réglage est
/// retenu et relu par `GetWindowDisplayAffinity`. Winlux ne propose aucune
/// capture d'écran qui devrait le respecter : il ne change rien à l'affichage.
unsafe extern "win64" fn SetWindowDisplayAffinity(hwnd: usize, affinite: u32) -> i32 {
    if !crate::dlls::gui::is_window(hwnd) {
        crate::process::set_last_error(ERROR_INVALID_WINDOW_HANDLE);
        return 0;
    }
    if !matches!(affinite, WDA_NONE | WDA_MONITOR | WDA_EXCLUDEFROMCAPTURE) {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return 0;
    }
    let mut a = super::lock(affinites());
    if affinite == WDA_NONE {
        a.remove(&hwnd);
    } else {
        a.insert(hwnd, affinite);
    }
    1
}

/// `GetWindowDisplayAffinity` (winuser.h, Windows 7) : rend le réglage posé
/// par `SetWindowDisplayAffinity`, `WDA_NONE` par défaut et pour une fenêtre
/// d'un autre processus du compte. Choix Winlux : l'appel réussit même pour
/// une fenêtre non superposée, le compositeur étant toujours actif.
unsafe extern "win64" fn GetWindowDisplayAffinity(hwnd: usize, sortie: *mut u32) -> i32 {
    let locale = crate::dlls::gui::is_window(hwnd);
    if !locale && crate::dlls::gui::foreign_window(hwnd).is_none() {
        super::lock(affinites()).remove(&hwnd);
        crate::process::set_last_error(ERROR_INVALID_WINDOW_HANDLE);
        return 0;
    }
    if sortie.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let valeur = if locale {
        super::lock(affinites())
            .get(&hwnd)
            .copied()
            .unwrap_or(WDA_NONE)
    } else {
        WDA_NONE
    };
    *sortie = valeur;
    1
}

/// GetGestureInfo : Winlux n'envoie jamais WM_GESTURE, aucun handle n'est valide.
unsafe extern "win64" fn GetGestureInfo(_handle: usize, _info: *mut u8) -> i32 {
    crate::process::set_last_error(6); // ERROR_INVALID_HANDLE
    0
}

/// SetGestureConfig / CloseGestureInfoHandle : acceptés sans effet.
unsafe extern "win64" fn GestureOk(_a: usize, _b: usize, _c: usize, _d: usize, _e: usize) -> i32 {
    1
}

/// Réglages de retour visuel tactile par fenêtre (FEEDBACK_TYPE 1 à 11,
/// valeur BOOL) posés par `SetWindowFeedbackSetting` (#1431). Winlux ne
/// dessine pas ces retours ; les réglages sont gardés pour les relire.
static FEEDBACK: std::sync::Mutex<Vec<(usize, u32, i32)>> = std::sync::Mutex::new(Vec::new());

const GWFS_INCLUDE_ANCESTORS: u32 = 1;

fn feedback_args_ok(hwnd: usize, kind: u32) -> bool {
    if !crate::dlls::gui::is_window(hwnd) {
        crate::process::set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
        return false;
    }
    if !(1..=11).contains(&kind) {
        crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
        return false;
    }
    true
}

/// SetWindowFeedbackSetting(hwnd, type, flags, size, config) : config nul
/// et size 0 rendent le réglage par défaut ; sinon un BOOL (4 octets).
unsafe extern "win64" fn SetWindowFeedbackSetting(
    hwnd: usize,
    kind: u32,
    _flags: u32,
    size: u32,
    config: *const i32,
) -> i32 {
    if !feedback_args_ok(hwnd, kind) {
        return 0;
    }
    let mut map = crate::dlls::lock(&FEEDBACK);
    map.retain(|&(h, k, _)| !(h == hwnd && k == kind));
    if config.is_null() {
        if size != 0 {
            crate::process::set_last_error(87);
            return 0;
        }
        return 1;
    }
    if size != 4 {
        crate::process::set_last_error(87);
        return 0;
    }
    map.push((hwnd, kind, (config.read_unaligned() != 0) as i32));
    1
}

/// GetWindowFeedbackSetting(hwnd, type, flags, *size, config) : le réglage
/// de la fenêtre, sinon (GWFS_INCLUDE_ANCESTORS) celui d'un parent ou le
/// défaut du système (retour activé).
unsafe extern "win64" fn GetWindowFeedbackSetting(
    hwnd: usize,
    kind: u32,
    flags: u32,
    size: *mut u32,
    config: *mut i32,
) -> i32 {
    if !feedback_args_ok(hwnd, kind) {
        return 0;
    }
    if size.is_null() || config.is_null() || *size < 4 {
        crate::process::set_last_error(87);
        return 0;
    }
    let found = {
        let map = crate::dlls::lock(&FEEDBACK);
        let mut h = hwnd;
        let mut found = None;
        while h != 0 {
            if let Some(&(_, _, v)) = map.iter().find(|&&(w, k, _)| w == h && k == kind) {
                found = Some(v);
                break;
            }
            if flags & GWFS_INCLUDE_ANCESTORS == 0 {
                break;
            }
            h = crate::dlls::user32::get_parent(h);
        }
        found
    };
    let value = match found {
        Some(v) => v,
        None if flags & GWFS_INCLUDE_ANCESTORS != 0 => 1,
        None => {
            *size = 0;
            return 0;
        }
    };
    config.write_unaligned(value);
    *size = 4;
    1
}

unsafe extern "win64" fn IsWindowArranged(_hwnd: usize) -> i32 {
    0
}

/// `ChangeWindowMessageFilterEx` : pas de niveaux d'intégrité entre
/// fenêtres sous Winlux ; CHANGEFILTERSTRUCT.ExtStatus = MSGFLTINFO_NONE.
unsafe extern "win64" fn ChangeWindowMessageFilterEx(
    _hwnd: usize,
    _msg: u32,
    _action: u32,
    info: *mut u32,
) -> i32 {
    if !info.is_null() && *info >= 8 {
        *info.add(1) = 0;
    }
    1
}

/// `GetPointerType` : tout pointeur est la souris (PT_MOUSE).
unsafe extern "win64" fn GetPointerType(_id: u32, kind: *mut u32) -> i32 {
    if kind.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *kind = 4;
    1
}

/// `SystemParametersInfoForDpi` : mêmes valeurs que SystemParametersInfoW
/// (les métriques sont déjà celles de l'échelle de la session).
/// RegisterHotKey(hwnd, id, modifiers, vk) : la combinaison est prise sur
/// tout l'écran et rend WM_HOTKEY ; refusée (ERROR_HOTKEY_ALREADY_REGISTERED)
/// si le gestionnaire de fenêtres ou un autre programme la tient déjà (#1168).
unsafe extern "win64" fn RegisterHotKey(hwnd: usize, id: i32, mods: u32, vk: u32) -> i32 {
    match crate::dlls::gui::register_hotkey(hwnd, id, mods, vk) {
        Ok(()) => 1,
        Err(code) => {
            crate::process::set_last_error(code);
            0
        }
    }
}

unsafe extern "win64" fn UnregisterHotKey(hwnd: usize, id: i32) -> i32 {
    match crate::dlls::gui::unregister_hotkey(hwnd, id) {
        Ok(()) => 1,
        Err(code) => {
            crate::process::set_last_error(code);
            0
        }
    }
}

unsafe extern "win64" fn SystemParametersInfoForDpi(
    action: u32,
    param: u32,
    data: *mut std::ffi::c_void,
    winini: u32,
    _dpi: u32,
) -> i32 {
    crate::dlls::user32::system_parameters_info_w(action, param, data, winini)
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RegisterPowerSettingNotification" => fn RegisterPowerSettingNotification,
        "UnregisterPowerSettingNotification" => fn UnregisterPowerSettingNotification,
        "EnableNonClientDpiScaling" => fn WindowOk,
        "RegisterTouchWindow" => fn WindowOk2,
        "GetGestureInfo" => fn GetGestureInfo,
        "SetGestureConfig" => fn GestureOk,
        "CloseGestureInfoHandle" => fn GestureOk,
        "UnregisterTouchWindow" => fn WindowOk,
        "SetWindowDisplayAffinity" => fn SetWindowDisplayAffinity,
        "GetWindowDisplayAffinity" => fn GetWindowDisplayAffinity,
        "RegisterHotKey" => fn RegisterHotKey,
        "UnregisterHotKey" => fn UnregisterHotKey,
        "RegisterPointerDeviceNotifications" => fn WindowOk2,
        "IsWindowArranged" => fn IsWindowArranged,
        "SetWindowFeedbackSetting" => fn SetWindowFeedbackSetting,
        "GetWindowFeedbackSetting" => fn GetWindowFeedbackSetting,
        "ChangeWindowMessageFilterEx" => fn ChangeWindowMessageFilterEx,
        "GetPointerType" => fn GetPointerType,
        "SystemParametersInfoForDpi" => fn SystemParametersInfoForDpi,
        "GetDisplayConfigBufferSizes" => fn GetDisplayConfigBufferSizes,
        "QueryDisplayConfig" => fn QueryDisplayConfig,
        "DisplayConfigGetDeviceInfo" => fn DisplayConfigGetDeviceInfo,
        "DisplayConfigSetDeviceInfo" => fn DisplayConfigSetDeviceInfo,
        "SetDisplayConfig" => fn SetDisplayConfig,
        "GetPointerDevices" => fn GetPointerDevices,
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

// ============================================================
//  combase / ole32 : chaînes HSTRING et activation Windows Runtime
// ============================================================
//
// Disposition commune des HSTRING (#1712), celle que C++/WinRT (base.h,
// `hstring_header` / `shared_hstring_header`) et windows-rs
// (`HStringHeader`, licence MIT) écrivent eux-mêmes sans passer par
// WindowsCreateString : { drapeaux u32, longueur u32, réservé u32,
// réservé u32, texte *const u16 } (20 octets en x86, 24 en x64, la taille
// de HSTRING_HEADER). Le drapeau 1 marque une référence rapide, dont
// l'en-tête est fourni par l'appelant. Une chaîne allouée continue par un
// compteur de références u32 puis le texte terminé par 0 ; elle vient du
// tas du processus, que ces bibliothèques libèrent par HeapFree, et
// WindowsDuplicateString ne fait qu'incrémenter son compteur.

const HSTRING_REFERENCE: u32 = 1;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const REGDB_E_CLASSNOTREG: i32 = 0x8004_0154u32 as i32;

/// Taille de l'en-tête, sur la taille d'un pointeur du programme.
fn header_size() -> usize {
    16 + crate::wow32::pointer_size()
}

/// Compteur de références d'une chaîne allouée, juste après l'en-tête.
unsafe fn hstring_count<'a>(h: usize) -> &'a std::sync::atomic::AtomicU32 {
    &*((h + header_size()) as *const std::sync::atomic::AtomicU32)
}

/// (texte, longueur) d'un HSTRING ; NULL est la chaîne vide.
unsafe fn hstring_parts(h: usize) -> Option<(*const u16, u32)> {
    if h == 0 {
        return Some((std::ptr::null(), 0));
    }
    let len = ((h + 4) as *const u32).read_unaligned();
    let text = header_text(h);
    if text.is_null() && len != 0 {
        return None;
    }
    Some((text, len))
}

/// Texte de l'en-tête, à +16, sur la taille d'un pointeur du programme :
/// le HSTRING_HEADER d'un programme x86 n'a que 20 octets (#1630).
unsafe fn header_text(h: usize) -> *const u16 {
    if crate::wow32::pointer_size() == 4 {
        ((h + 16) as *const u32).read_unaligned() as usize as *const u16
    } else {
        ((h + 16) as *const usize).read_unaligned() as *const u16
    }
}

unsafe fn set_header_text(h: usize, text: usize) {
    if crate::wow32::pointer_size() == 4 {
        ((h + 16) as *mut u32).write_unaligned(text as u32);
    } else {
        ((h + 16) as *mut usize).write_unaligned(text);
    }
}

unsafe fn hstring_alloc(src: *const u16, len: u32) -> usize {
    if len == 0 {
        return 0;
    }
    let text_at = header_size() + 4;
    let size = text_at + (len as usize + 1) * 2;
    // HEAP_ZERO_MEMORY : en-tête réservé et 0 final.
    let p = crate::dlls::kernel32::heap_alloc(0, 8, size) as *mut u8;
    if p.is_null() {
        return 0;
    }
    let text = p.add(text_at) as *mut u16;
    std::ptr::copy_nonoverlapping(src, text, len as usize);
    (p.add(4) as *mut u32).write_unaligned(len);
    set_header_text(p as usize, text as usize);
    hstring_count(p as usize).store(1, std::sync::atomic::Ordering::Relaxed);
    p as usize
}

/// Une référence rapide (en-tête fourni par l'appelant) ?
unsafe fn is_reference(h: usize) -> bool {
    (h as *const u32).read_unaligned() & HSTRING_REFERENCE != 0
}

pub(crate) unsafe extern "win64" fn WindowsCreateString(
    src: *const u16,
    len: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    crate::wow32::write_ptr(out as usize, 0);
    if len == 0 {
        return 0;
    }
    if src.is_null() {
        return E_POINTER;
    }
    let h = hstring_alloc(src, len);
    if h == 0 {
        return E_OUTOFMEMORY;
    }
    crate::wow32::write_ptr(out as usize, h);
    0
}

unsafe extern "win64" fn WindowsCreateStringReference(
    src: *const u16,
    len: u32,
    header: *mut u8,
    out: *mut usize,
) -> i32 {
    if out.is_null() || header.is_null() {
        return E_INVALIDARG;
    }
    crate::wow32::write_ptr(out as usize, 0);
    if len == 0 {
        return 0;
    }
    if src.is_null() || *src.add(len as usize) != 0 {
        return E_INVALIDARG;
    }
    // HSTRING_HEADER : 20 octets en x86, 24 en x64 (#1630).
    std::ptr::write_bytes(header, 0, header_size());
    (header as *mut u32).write_unaligned(HSTRING_REFERENCE);
    (header.add(4) as *mut u32).write_unaligned(len);
    set_header_text(header as usize, src as usize);
    crate::wow32::write_ptr(out as usize, header as usize);
    0
}

unsafe extern "win64" fn WindowsDeleteString(h: usize) -> i32 {
    if h == 0 || is_reference(h) {
        return 0;
    }
    if hstring_count(h).fetch_sub(1, std::sync::atomic::Ordering::AcqRel) == 1 {
        crate::dlls::kernel32::heap_free(0, 0, h as *mut std::ffi::c_void);
    }
    0
}

unsafe extern "win64" fn WindowsDuplicateString(h: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    crate::wow32::write_ptr(out as usize, 0);
    if h == 0 {
        return 0;
    }
    if !is_reference(h) {
        hstring_count(h).fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        crate::wow32::write_ptr(out as usize, h);
        return 0;
    }
    let Some((text, len)) = hstring_parts(h) else {
        return E_INVALIDARG;
    };
    let copy = hstring_alloc(text, len);
    if copy == 0 && len != 0 {
        return E_OUTOFMEMORY;
    }
    crate::wow32::write_ptr(out as usize, copy);
    0
}

static EMPTY_WIDE: [u16; 1] = [0];

unsafe extern "win64" fn WindowsGetStringRawBuffer(h: usize, len: *mut u32) -> *const u16 {
    let (text, n) = hstring_parts(h).unwrap_or((std::ptr::null(), 0));
    if !len.is_null() {
        *len = n;
    }
    if text.is_null() {
        EMPTY_WIDE.as_ptr()
    } else {
        text
    }
}

unsafe extern "win64" fn WindowsGetStringLen(h: usize) -> u32 {
    hstring_parts(h).map_or(0, |(_, n)| n)
}

unsafe extern "win64" fn WindowsIsStringEmpty(h: usize) -> i32 {
    (WindowsGetStringLen(h) == 0) as i32
}

unsafe extern "win64" fn WindowsStringHasEmbeddedNull(h: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let (text, n) = hstring_parts(h).unwrap_or((std::ptr::null(), 0));
    *out = (0..n as usize).any(|i| *text.add(i) == 0) as i32;
    0
}

pub(crate) unsafe fn hstring_slice<'a>(h: usize) -> &'a [u16] {
    match hstring_parts(h) {
        Some((text, n)) if !text.is_null() => std::slice::from_raw_parts(text, n as usize),
        _ => &[],
    }
}

unsafe extern "win64" fn WindowsCompareStringOrdinal(a: usize, b: usize, result: *mut i32) -> i32 {
    if result.is_null() {
        return E_INVALIDARG;
    }
    *result = match hstring_slice(a).cmp(hstring_slice(b)) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    0
}

unsafe extern "win64" fn WindowsConcatString(a: usize, b: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let mut v = hstring_slice(a).to_vec();
    v.extend_from_slice(hstring_slice(b));
    WindowsCreateString(v.as_ptr(), v.len() as u32, out)
}

unsafe extern "win64" fn WindowsSubstringWithSpecifiedLength(
    h: usize,
    start: u32,
    len: u32,
    out: *mut usize,
) -> i32 {
    let s = hstring_slice(h);
    let end = start as usize + len as usize;
    if out.is_null() || end > s.len() {
        return E_INVALIDARG;
    }
    WindowsCreateString(s[start as usize..end].as_ptr(), len, out)
}

unsafe extern "win64" fn WindowsSubstring(h: usize, start: u32, out: *mut usize) -> i32 {
    let n = WindowsGetStringLen(h);
    if start > n {
        return E_INVALIDARG;
    }
    WindowsSubstringWithSpecifiedLength(h, start, n - start, out)
}

/// Sont inscrites `Windows.System.DispatcherQueue` (#1355), les classes
/// d'activation de Windows.UI.Composition (#1416) et
/// `Windows.Storage.ApplicationData` (#2122). Pour les autres classes, Chromium (notifications, réglages d'accessibilité, UI
/// Automation) se rabat sur l'API Win32.
unsafe extern "win64" fn RoGetActivationFactory(class: usize, iid: usize, out: *mut usize) -> i32 {
    let name = String::from_utf16_lossy(hstring_slice(class));
    // Fabriques inscrites par le programme (RoRegisterActivationFactories).
    if let Some(hr) =
        super::ole32_lot_c::registered_activation_factory(class, &name, iid as *const u8, out)
    {
        return hr;
    }
    // Windows.System.Profile.WindowsIntegrityPolicy (#2217).
    if let Some(hr) = super::integrity_policy::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    if let Some(hr) = super::coremessaging::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    // Windows.UI.ViewManagement.UISettings (#2206).
    if let Some(hr) = super::ui_settings::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    if let Some(hr) = super::composition::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    // Windows.Networking.Connectivity.NetworkInformation (#2215).
    if let Some(hr) = super::network_information::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    if let Some(hr) = super::winrt_ui::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    // Windows.System.UserProfile.GlobalizationPreferences (#2172).
    if let Some(hr) = super::globalization_prefs::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    // Windows.Storage.ApplicationData d'un programme empaqueté (#2122).
    if let Some(hr) = super::storage_appdata::activation_factory(&name, iid as *const u8, out) {
        return hr;
    }
    if !out.is_null() {
        crate::wow32::write_ptr(out as usize, 0);
    }
    // Classe et interface demandées, pour savoir ce qu'il faudrait servir
    // (#1659), comme le journal de CoCreateInstance.
    let iid_text = super::comobj::read_guid(iid as *const u8)
        .map_or_else(|| "NULL".to_string(), |g| super::comobj::guid_str(&g));
    crate::log_warn!(
        "com",
        "RoGetActivationFactory : classe « {name} » (interface {iid_text}) inconnue (REGDB_E_CLASSNOTREG)"
    );
    REGDB_E_CLASSNOTREG
}

unsafe extern "win64" fn RoActivateInstance(_class: usize, out: *mut usize) -> i32 {
    RoGetActivationFactory(0, 0, out)
}

unsafe extern "win64" fn RoOriginateError(_hr: i32, _message: usize) -> i32 {
    0
}

unsafe extern "win64" fn RoTransformError(_old: i32, _new: i32, _message: usize) -> i32 {
    0
}

/// Erreur posée par SetRestrictedErrorInfo sur ce thread, cédée à
/// l'appelant ; S_FALSE quand il n'y en a pas.
unsafe extern "win64" fn GetRestrictedErrorInfo(out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let info = super::ole32_lot_c::take_restricted_error_info();
    crate::wow32::write_ptr(out as usize, info);
    if info == 0 {
        1
    } else {
        0
    }
}

pub fn ole32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WindowsCreateString" => fn WindowsCreateString,
        "WindowsCreateStringReference" => fn WindowsCreateStringReference,
        "WindowsDeleteString" => fn WindowsDeleteString,
        "WindowsDuplicateString" => fn WindowsDuplicateString,
        "WindowsGetStringRawBuffer" => fn WindowsGetStringRawBuffer,
        "WindowsGetStringLen" => fn WindowsGetStringLen,
        "WindowsIsStringEmpty" => fn WindowsIsStringEmpty,
        "WindowsStringHasEmbeddedNull" => fn WindowsStringHasEmbeddedNull,
        "WindowsCompareStringOrdinal" => fn WindowsCompareStringOrdinal,
        "WindowsConcatString" => fn WindowsConcatString,
        "WindowsSubstring" => fn WindowsSubstring,
        "WindowsSubstringWithSpecifiedLength" => fn WindowsSubstringWithSpecifiedLength,
        "RoGetActivationFactory" => fn RoGetActivationFactory,
        "RoActivateInstance" => fn RoActivateInstance,
        "RoOriginateError" => fn RoOriginateError,
        "RoOriginateErrorW" => fn RoOriginateError,
        "RoTransformError" => fn RoTransformError,
        "RoTransformErrorW" => fn RoTransformError,
        "GetRestrictedErrorInfo" => fn GetRestrictedErrorInfo,
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

/// WSAEnumNameSpaceProviders(A|W|ExA|ExW) : aucun fournisseur d'espace de
/// noms n'est enregistré (la résolution passe par getaddrinfo et les
/// requêtes NLA ci-dessous) ; 0 entrée, longueur requise nulle (#2000).
unsafe extern "win64" fn WSAEnumNameSpaceProviders(len: *mut u32, _buf: *mut c_void) -> i32 {
    if len.is_null() {
        return wsa_error(10014); // WSAEFAULT
    }
    *len = 0;
    0
}

/// NS_NLA : l'emplacement réseau (Network Location Awareness), que
/// Chromium interroge pour savoir si la machine est connectée.
const NS_NLA: u32 = 15;
const WSAQUERYSETW_SIZE: usize = 120;

type NlaQueries = std::sync::Mutex<std::collections::HashMap<usize, bool>>;

fn nla_queries() -> &'static NlaQueries {
    static Q: std::sync::OnceLock<NlaQueries> = std::sync::OnceLock::new();
    Q.get_or_init(Default::default)
}

/// Une interface autre que la boucle locale est-elle active, avec une
/// adresse ?
pub fn network_connected() -> bool {
    unsafe {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut list) != 0 {
            return false;
        }
        let mut found = false;
        let mut at = list;
        while !at.is_null() {
            let i = &*at;
            let up = libc::IFF_UP as u32 | libc::IFF_RUNNING as u32;
            if i.ifa_flags & up == up
                && i.ifa_flags & libc::IFF_LOOPBACK as u32 == 0
                && !i.ifa_addr.is_null()
            {
                let family = (*i.ifa_addr).sa_family as i32;
                if family == libc::AF_INET || family == libc::AF_INET6 {
                    found = true;
                    break;
                }
            }
            at = i.ifa_next;
        }
        libc::freeifaddrs(list);
        found
    }
}

fn wsa_error(code: u32) -> i32 {
    unsafe { api!("ws2_32", "WSASetLastError", fn(i32) -> (), code as i32) };
    SOCKET_ERROR
}

unsafe extern "win64" fn WSALookupServiceBeginW(
    q: *mut c_void,
    _flags: u32,
    handle: *mut usize,
) -> i32 {
    if handle.is_null() || q.is_null() {
        return wsa_error(10014); // WSAEFAULT
    }
    *handle = 0;
    let namespace = ((q as *const u8).add(40) as *const u32).read_unaligned();
    if namespace != NS_NLA {
        return wsa_error(WSASERVICE_NOT_FOUND);
    }
    let h = crate::dlls::kernel32::new_handle_number();
    nla_queries()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(h, false);
    *handle = h;
    0
}

unsafe extern "win64" fn WSALookupServiceNextW(
    h: usize,
    _flags: u32,
    len: *mut u32,
    out: *mut c_void,
) -> i32 {
    let mut queries = nla_queries().lock().unwrap_or_else(|e| e.into_inner());
    let Some(done) = queries.get_mut(&h) else {
        return wsa_error(6); // WSA_INVALID_HANDLE
    };
    // Un seul emplacement, « Réseau », quand une interface est active.
    if *done || !network_connected() {
        return wsa_error(WSA_E_NO_MORE);
    }
    let name: Vec<u16> = "Réseau".encode_utf16().chain(Some(0)).collect();
    let need = WSAQUERYSETW_SIZE + name.len() * 2;
    if len.is_null() || out.is_null() || (*len as usize) < need {
        if !len.is_null() {
            *len = need as u32;
        }
        return wsa_error(10014); // WSAEFAULT : tampon trop petit
    }
    let base = out as *mut u8;
    std::ptr::write_bytes(base, 0, WSAQUERYSETW_SIZE);
    (base as *mut u32).write_unaligned(WSAQUERYSETW_SIZE as u32);
    let text = base.add(WSAQUERYSETW_SIZE) as *mut u16;
    std::ptr::copy_nonoverlapping(name.as_ptr(), text, name.len());
    (base.add(8) as *mut usize).write_unaligned(text as usize);
    (base.add(40) as *mut u32).write_unaligned(NS_NLA);
    *done = true;
    0
}

unsafe extern "win64" fn WSALookupServiceEnd(h: usize) -> i32 {
    nla_queries()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&h);
    0
}

unsafe extern "win64" fn WSASetServiceW(_info: *mut c_void, _op: u32, _flags: u32) -> i32 {
    api!("ws2_32", "WSASetLastError", fn(i32) -> (), 10045); // WSAEOPNOTSUPP
    SOCKET_ERROR
}

pub fn ws2_32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WSAEnumProtocolsW" => fn WSAEnumProtocolsW,
        "WSAEnumNameSpaceProvidersA" => fn WSAEnumNameSpaceProviders,
        "WSAEnumNameSpaceProvidersW" => fn WSAEnumNameSpaceProviders,
        "WSAEnumNameSpaceProvidersExA" => fn WSAEnumNameSpaceProviders,
        "WSAEnumNameSpaceProvidersExW" => fn WSAEnumNameSpaceProviders,
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
        // Ordinal d'après OleAut32.Lib du SDK Windows public (#1606).
        "#277" => fn VarUI4FromStr,
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
    // CERT_ENHKEY_USAGE { cUsageIdentifier, rgpszUsageIdentifier } : 8 octets
    // en x86, 16 en x64 (#1667).
    let ps = crate::wow32::pointer_size();
    let size = 2 * ps as u32;
    if usage.is_null() {
        *len = size;
        return 1;
    }
    if *len < size {
        *len = size;
        set_last_error(234); // ERROR_MORE_DATA
        return 0;
    }
    *usage = 0; // cUsageIdentifier
    crate::wow32::write_ptr(usage as usize + ps, 0);
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

unsafe extern "win64" fn CryptMsgClose(msg: usize) -> i32 {
    // Message de CryptMsgOpenToDecode (#1820) : libéré.
    super::discord_win32::close_msg(msg);
    1
}

unsafe extern "win64" fn CryptMsgGetParam(
    msg: usize,
    ty: u32,
    index: u32,
    data: *mut c_void,
    len: *mut u32,
) -> i32 {
    if super::discord_win32::is_decode_msg(msg) {
        return super::discord_win32::get_param(msg, ty, index, data, len);
    }
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

// ============================================================
//  shlwapi : PathMatchSpec ; advapi32 : MapGenericMask
// ============================================================

/// Motif MS-DOS (`*`, `?`) sur un texte, sans casse.
fn dos_match(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            mark = t;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// Un nom correspond-il à l'une des spécifications (« *.txt; *.md ») ?
pub fn path_match_spec(file: &str, spec: &str, multiple: bool, strip: bool) -> bool {
    let lower = |s: &str| {
        s.chars()
            .flat_map(char::to_lowercase)
            .collect::<Vec<char>>()
    };
    let name = lower(file);
    let one = |p: &str| {
        let p = if strip { p.trim_start() } else { p };
        // « *.* » accepte aussi les noms sans point, comme sous Windows.
        p == "*.*" || dos_match(&lower(p), &name)
    };
    if multiple {
        spec.split(';').any(one)
    } else {
        one(spec)
    }
}

unsafe extern "win64" fn PathMatchSpecW(file: *const u16, spec: *const u16) -> i32 {
    if file.is_null() || spec.is_null() {
        return 0;
    }
    path_match_spec(
        &crate::dlls::wstr(file),
        &crate::dlls::wstr(spec),
        true,
        true,
    ) as i32
}

unsafe extern "win64" fn PathMatchSpecA(file: *const u8, spec: *const u8) -> i32 {
    if file.is_null() || spec.is_null() {
        return 0;
    }
    path_match_spec(
        &crate::dlls::cstr(file),
        &crate::dlls::cstr(spec),
        true,
        true,
    ) as i32
}

/// PathMatchSpecEx : S_OK si correspondance, S_FALSE sinon ;
/// PMSF_MULTIPLE (1) pour une liste, PMSF_DONT_STRIP_SPACES (0x10000).
unsafe extern "win64" fn PathMatchSpecExW(file: *const u16, spec: *const u16, flags: u32) -> u32 {
    if file.is_null() || spec.is_null() {
        return 0x8007_0057;
    }
    let ok = path_match_spec(
        &crate::dlls::wstr(file),
        &crate::dlls::wstr(spec),
        flags & 1 != 0,
        flags & 0x1_0000 == 0,
    );
    u32::from(!ok)
}

unsafe extern "win64" fn PathMatchSpecExA(file: *const u8, spec: *const u8, flags: u32) -> u32 {
    if file.is_null() || spec.is_null() {
        return 0x8007_0057;
    }
    let ok = path_match_spec(
        &crate::dlls::cstr(file),
        &crate::dlls::cstr(spec),
        flags & 1 != 0,
        flags & 0x1_0000 == 0,
    );
    u32::from(!ok)
}

pub fn shlwapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "PathMatchSpecW" => fn PathMatchSpecW,
        "PathMatchSpecA" => fn PathMatchSpecA,
        "PathMatchSpecExW" => fn PathMatchSpecExW,
        "PathMatchSpecExA" => fn PathMatchSpecExA,
    }
}

/// `MapGenericMask` : GENERIC_READ/WRITE/EXECUTE/ALL remplacés par les
/// droits de la GENERIC_MAPPING, bits génériques effacés.
pub fn map_generic_mask(mask: u32, mapping: [u32; 4]) -> u32 {
    let mut m = mask;
    for (bit, rights) in [
        (0x8000_0000u32, mapping[0]),
        (0x4000_0000, mapping[1]),
        (0x2000_0000, mapping[2]),
        (0x1000_0000, mapping[3]),
    ] {
        if mask & bit != 0 {
            m |= rights;
        }
    }
    m & !0xF000_0000
}

unsafe extern "win64" fn MapGenericMask(mask: *mut u32, mapping: *const u32) {
    if mask.is_null() || mapping.is_null() {
        return;
    }
    let map = [
        mapping.read_unaligned(),
        mapping.add(1).read_unaligned(),
        mapping.add(2).read_unaligned(),
        mapping.add(3).read_unaligned(),
    ];
    mask.write_unaligned(map_generic_mask(mask.read_unaligned(), map));
}

/// Droits accordés par AccessCheck : le jeton unique de Winlux est celui de
/// l'utilisateur, propriétaire de ses objets ; il reçoit ce qu'il demande
/// (MAXIMUM_ALLOWED donne GENERIC_ALL traduit).
pub fn access_check_granted(desired: u32, mapping: [u32; 4]) -> u32 {
    const MAXIMUM_ALLOWED: u32 = 0x0200_0000;
    let mut want = desired;
    if want & MAXIMUM_ALLOWED != 0 {
        want = (want & !MAXIMUM_ALLOWED) | 0x1000_0000;
    }
    map_generic_mask(want, mapping)
}

/// AccessCheck(sd, token, desired, mapping, privileges, privileges_len,
/// granted, status) -> BOOL
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn AccessCheck(
    sd: usize,
    _token: usize,
    desired: u32,
    mapping: *const u32,
    privileges: *mut u32,
    privileges_len: *mut u32,
    granted: *mut u32,
    status: *mut i32,
) -> i32 {
    if sd == 0 || mapping.is_null() || granted.is_null() || status.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    // PRIVILEGE_SET vide : PrivilegeCount = 0, Control = 0 (8 octets).
    if !privileges_len.is_null() {
        if privileges_len.read_unaligned() < 8 {
            privileges_len.write_unaligned(8);
            crate::process::set_last_error(122);
            return 0;
        }
        if !privileges.is_null() {
            privileges.write_unaligned(0);
            privileges.add(1).write_unaligned(0);
        }
    }
    let map = [
        mapping.read_unaligned(),
        mapping.add(1).read_unaligned(),
        mapping.add(2).read_unaligned(),
        mapping.add(3).read_unaligned(),
    ];
    granted.write_unaligned(access_check_granted(desired, map));
    status.write_unaligned(1);
    1
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
        ("ole32", ole32_exports()),
        ("wtsapi32", wtsapi32_exports()),
        ("shlwapi", shlwapi_exports()),
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
    fn affinite_d_affichage_retenue_et_relue() {
        let _teb = crate::process::test_teb();
        unsafe {
            let mut v = 0xFFu32;
            crate::process::set_last_error(0);
            assert_eq!(GetWindowDisplayAffinity(0xDEAD_0000, &mut v), 0);
            assert_eq!(crate::process::last_error(), ERROR_INVALID_WINDOW_HANDLE);
            assert_eq!(SetWindowDisplayAffinity(0xDEAD_0000, WDA_MONITOR), 0);
            assert_eq!(crate::process::last_error(), ERROR_INVALID_WINDOW_HANDLE);

            use crate::dlls::gui;
            extern "win64" fn inerte(_h: usize, _m: u32, _w: usize, _l: isize) -> isize {
                0
            }
            let proc_addr: extern "win64" fn(usize, u32, usize, isize) -> isize = inerte;
            let hwnd = gui::alloc_hwnd();
            gui::insert_window(
                hwnd,
                gui::Window {
                    class: "Test".to_string(),
                    wnd_proc: proc_addr as usize,
                    background: 0,
                    title: "test".to_string(),
                    rect: gui::Rect::from_size(0, 0, 200, 100),
                    visible: false,
                    parent: 0,
                    owner: std::thread::current().id(),
                    needs_paint: false,
                    paint_queued: false,
                    style: 0,
                    ex_style: 0,
                    pending: Vec::new(),
                    menu_bar: 0,
                    painting: false,
                },
            );
            assert_eq!(GetWindowDisplayAffinity(hwnd, &mut v), 1);
            assert_eq!(v, WDA_NONE);
            assert_eq!(SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE), 1);
            assert_eq!(GetWindowDisplayAffinity(hwnd, &mut v), 1);
            assert_eq!(v, WDA_EXCLUDEFROMCAPTURE);
            assert_eq!(SetWindowDisplayAffinity(hwnd, 7), 0);
            assert_eq!(crate::process::last_error(), 87);
            assert_eq!(GetWindowDisplayAffinity(hwnd, std::ptr::null_mut()), 0);
            assert_eq!(SetWindowDisplayAffinity(hwnd, WDA_NONE), 1);
            assert_eq!(GetWindowDisplayAffinity(hwnd, &mut v), 1);
            assert_eq!(v, WDA_NONE);
            let e = all();
            assert!(e.iter().any(|(dll, t)| dll.eq_ignore_ascii_case("user32")
                && t.contains_key("GetWindowDisplayAffinity")));
            gui::destroy_window(hwnd);
        }
    }

    /// Steam, utility storage (#2000) : aucun fournisseur, longueur nulle.
    #[test]
    fn aucun_fournisseur_d_espace_de_noms() {
        unsafe {
            let mut len = 77u32;
            assert_eq!(WSAEnumNameSpaceProviders(&mut len, std::ptr::null_mut()), 0);
            assert_eq!(len, 0);
            let e = ws2_32_exports();
            for n in ["A", "W", "ExA", "ExW"] {
                let nom = format!("WSAEnumNameSpaceProviders{n}");
                assert!(e.contains_key(nom.as_str()), "{nom} absent");
            }
        }
    }

    fn etat(pid: u32) -> char {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        stat[stat.rfind(')').unwrap() + 1..]
            .trim_start()
            .chars()
            .next()
            .unwrap()
    }

    /// #1797 : trace des modules déchargés vide mais bien formée.
    #[test]
    fn trace_des_modules_decharges_vide() {
        let (mut taille, mut nombre, mut trace) =
            (std::ptr::null(), std::ptr::null(), std::ptr::null());
        unsafe {
            RtlGetUnloadEventTraceEx(&mut taille, &mut nombre, &mut trace);
            assert_eq!(*taille, 0x68);
            assert_eq!(*nombre, 0);
            let tableau = *(trace as *const *const u8);
            assert_eq!(tableau, UNLOAD_TRACE_ARRAY.as_ptr());
            RtlGetUnloadEventTraceEx(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        let exports = crate::dlls::builtin_exports();
        let ntdll = exports.iter().find(|(n, _)| *n == "ntdll").unwrap();
        assert!(ntdll.1.contains(&"RtlGetUnloadEventTraceEx"));
    }

    /// #1787 : un processus figé par NtSuspendProcess puis laissé ainsi
    /// (Crashpad planté en plein vidage) est relancé par le gestionnaire de
    /// plantage, et une relance normale libère sa place.
    #[test]
    fn processus_fige_relance_apres_plantage() {
        let mut enfant = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = enfant.id();
        unsafe { libc::kill(pid as i32, libc::SIGSTOP) };
        let place = SUSPENDED.iter().find(|s| {
            s.compare_exchange(0, pid as i32, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        });
        assert!(place.is_some());
        let debut = std::time::Instant::now();
        while etat(pid) != 'T' && debut.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::yield_now();
        }
        assert_eq!(etat(pid), 'T');
        assert!(resume_suspended() >= 1);
        let debut = std::time::Instant::now();
        while etat(pid) == 'T' && debut.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::yield_now();
        }
        assert_ne!(etat(pid), 'T');
        assert!(SUSPENDED
            .iter()
            .all(|s| s.load(Ordering::Relaxed) != pid as i32));
        let _ = enfant.kill();
        let _ = enfant.wait();
    }

    fn fenetre(parent: usize) -> usize {
        let hwnd = crate::dlls::gui::alloc_hwnd();
        crate::dlls::gui::insert_window(
            hwnd,
            crate::dlls::gui::Window {
                class: "Test".to_string(),
                wnd_proc: 0,
                background: 0,
                title: String::new(),
                rect: crate::dlls::gui::Rect::from_size(0, 0, 10, 10),
                visible: false,
                parent,
                owner: std::thread::current().id(),
                needs_paint: false,
                paint_queued: false,
                style: if parent != 0 { 0x4000_0000 } else { 0 },
                ex_style: 0,
                pending: Vec::new(),
                menu_bar: 0,
                painting: false,
            },
        );
        hwnd
    }

    #[test]
    fn retour_tactile_par_fenetre() {
        unsafe {
            let parent = fenetre(0);
            let child = fenetre(parent);
            let off = 0i32;
            // FEEDBACK_GESTURE_PRESSANDTAP (11) coupé sur le parent.
            assert_eq!(SetWindowFeedbackSetting(parent, 11, 0, 4, &off), 1);
            let (mut size, mut v) = (4u32, -1i32);
            assert_eq!(
                GetWindowFeedbackSetting(parent, 11, 0, &mut size, &mut v),
                1
            );
            assert_eq!((size, v), (4, 0));
            // L'enfant n'a rien à lui ; avec les ancêtres, il hérite.
            assert_eq!(GetWindowFeedbackSetting(child, 11, 0, &mut size, &mut v), 0);
            size = 4;
            assert_eq!(GetWindowFeedbackSetting(child, 11, 1, &mut size, &mut v), 1);
            assert_eq!(v, 0);
            // Autre type : défaut du système, activé.
            assert_eq!(GetWindowFeedbackSetting(child, 2, 1, &mut size, &mut v), 1);
            assert_eq!(v, 1);
            // Retour au défaut, puis paramètres refusés.
            assert_eq!(
                SetWindowFeedbackSetting(parent, 11, 0, 0, std::ptr::null()),
                1
            );
            assert_eq!(
                GetWindowFeedbackSetting(parent, 11, 0, &mut size, &mut v),
                0
            );
            assert_eq!(SetWindowFeedbackSetting(parent, 12, 0, 4, &off), 0);
            assert_eq!(SetWindowFeedbackSetting(parent, 11, 0, 2, &off), 0);
            assert_eq!(SetWindowFeedbackSetting(0x7fff_0000, 11, 0, 4, &off), 0);
        }
    }

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
        }
        // TerminateProcess(GetCurrentProcess()) termine sans rendre la main.
        // Vérifié dans un processus neuf (ce même test relancé seul) : un
        // fork() de l'exécuteur multithread pouvait hériter d'un verrou tenu
        // par un autre test et rester bloqué pour toujours (#1982).
        const ENFANT: &str = "WINLUX_TEST_ARRET_IMMEDIAT";
        if std::env::var_os(ENFANT).is_some() {
            unsafe {
                k::terminate_process(k::get_current_process(), 7);
                libc::_exit(99);
            }
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "dlls::electron_win32::tests::handle_vers_soi_et_arret_immediat",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(ENFANT, "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let fin = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if std::time::Instant::now() > fin {
                let _ = child.kill();
                let _ = child.wait();
                panic!("TerminateProcess(GetCurrentProcess()) n'a pas terminé en 60 s");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(status.code(), Some(7));
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
    fn tube_nomme_transaction_et_fin() {
        use crate::dlls::kernel32 as k;
        let name = format!("\\\\.\\pipe\\winlux-transact-{}", std::process::id());
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe {
            // PIPE_ACCESS_DUPLEX, PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE.
            let server = k::create_named_pipe_w(wide.as_ptr(), 3, 6, 1, 512, 512, 0, 0);
            assert_ne!(server, usize::MAX);
            let srv = server;
            let t = std::thread::spawn(move || {
                let _teb = crate::process::test_teb();
                k::connect_named_pipe(srv, 0);
                let mut buf = [0u8; 16];
                let mut n = 0u32;
                assert_eq!(k::read_file(srv, buf.as_mut_ptr(), 16, &mut n, 0), 1);
                assert_eq!(&buf[..n as usize], b"ping");
                assert_eq!(k::write_file(srv, b"pong!".as_ptr(), 5, &mut n, 0), 1);
                // Le client ferme : la lecture suivante rend ERROR_BROKEN_PIPE,
                // jamais un succès à 0 octet (crashpad bouclait dessus).
                assert_eq!(k::read_file(srv, buf.as_mut_ptr(), 16, &mut n, 0), 0);
                assert_eq!(k::get_last_error(), 109);
            });
            let mut client = usize::MAX;
            for _ in 0..200 {
                client = k::create_file_w(wide.as_ptr(), 0xC000_0000, 0, 0, 3, 0, 0);
                if client != usize::MAX {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert_ne!(client, usize::MAX, "client du tube");
            let mut out = [0u8; 16];
            let mut got = 0u32;
            assert_eq!(
                TransactNamedPipe(
                    client,
                    b"ping".as_ptr(),
                    4,
                    out.as_mut_ptr(),
                    16,
                    &mut got,
                    0
                ),
                1
            );
            assert_eq!(&out[..got as usize], b"pong!");
            k::close_handle(client);
            t.join().unwrap();
            k::close_handle(server);
            let mut trustee = [0u8; 32];
            BuildTrusteeWithSid(trustee.as_mut_ptr(), 0x1234);
            assert_eq!(u32::from_le_bytes(trustee[12..16].try_into().unwrap()), 0);
            assert_eq!(
                usize::from_le_bytes(trustee[24..32].try_into().unwrap()),
                0x1234
            );
        }
    }

    #[test]
    fn tube_nomme_deconnexion_puis_nouveau_client() {
        use crate::dlls::kernel32 as k;
        let _teb = crate::process::test_teb();
        let name = format!("\\\\.\\pipe\\winlux-disconnect-{}", std::process::id());
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let open = |w: &Vec<u16>| unsafe {
            for _ in 0..200 {
                let c = k::create_file_w(w.as_ptr(), 0xC000_0000, 0, 0, 3, 0, 0);
                if c != usize::MAX {
                    return c;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            usize::MAX
        };
        unsafe {
            let server = k::create_named_pipe_w(wide.as_ptr(), 3, 0, 1, 512, 512, 0, 0);
            assert_ne!(server, usize::MAX);
            let srv = server;
            let t = std::thread::spawn(move || {
                let _teb = crate::process::test_teb();
                for word in [b"un", b"de"] {
                    let r = k::connect_named_pipe(srv, 0);
                    assert!(r == 1 || k::get_last_error() == 535);
                    let mut buf = [0u8; 8];
                    let mut n = 0u32;
                    assert_eq!(k::read_file(srv, buf.as_mut_ptr(), 8, &mut n, 0), 1);
                    assert_eq!(&buf[..n as usize], word);
                    // Le serveur libère l'instance pour le client suivant,
                    // comme le serveur d'exceptions de crashpad.
                    assert_eq!(k::disconnect_named_pipe(srv), 1);
                }
            });
            let mut n = 0u32;
            let c1 = open(&wide);
            assert_ne!(c1, usize::MAX);
            assert_eq!(k::write_file(c1, b"un".as_ptr(), 2, &mut n, 0), 1);
            // Après DisconnectNamedPipe, le premier client lit la fin du tube.
            let mut buf = [0u8; 4];
            assert_eq!(k::read_file(c1, buf.as_mut_ptr(), 4, &mut n, 0), 0);
            let c2 = open(&wide);
            assert_ne!(c2, usize::MAX, "second client après la déconnexion");
            assert_eq!(k::write_file(c2, b"de".as_ptr(), 2, &mut n, 0), 1);
            t.join().unwrap();
            k::close_handle(c1);
            k::close_handle(c2);
            k::close_handle(server);
            let mut count = 7u32;
            assert_eq!(GetPointerDevices(&mut count, std::ptr::null_mut()), 1);
            assert_eq!(count, 0);
        }
    }

    #[test]
    fn configuration_d_affichage() {
        unsafe {
            let (mut np, mut nm) = (0u32, 0u32);
            assert_eq!(GetDisplayConfigBufferSizes(2, &mut np, &mut nm), 0);
            assert!(np >= 1);
            assert_eq!(nm, 2 * np);
            let mut paths = vec![0u8; 72 * np as usize];
            let mut modes = vec![0u8; 64 * nm as usize];
            let mut small = 0u32;
            assert_eq!(
                QueryDisplayConfig(
                    2,
                    &mut small,
                    paths.as_mut_ptr(),
                    &mut nm,
                    modes.as_mut_ptr(),
                    std::ptr::null_mut()
                ),
                122
            );
            assert_eq!(
                QueryDisplayConfig(
                    2,
                    &mut np,
                    paths.as_mut_ptr(),
                    &mut nm,
                    modes.as_mut_ptr(),
                    std::ptr::null_mut()
                ),
                0
            );
            let rd = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
            // Le mode cible du premier chemin porte la fréquence et la taille.
            let t = 64 * rd(&paths, 32) as usize;
            assert_eq!(rd(&modes, t), 2);
            assert!(rd(&modes, t + 32) > 0);
            assert_eq!(
                rd(&modes, t + 40),
                rd(&modes, 64 * rd(&paths, 12) as usize + 16)
            );
            // Nom de la source : celui de GetMonitorInfo.
            let mut src = [0u8; 84];
            src[0..4].copy_from_slice(&1u32.to_le_bytes());
            src[4..8].copy_from_slice(&84u32.to_le_bytes());
            src[16..20].copy_from_slice(&paths[8..12]);
            assert_eq!(DisplayConfigGetDeviceInfo(src.as_mut_ptr()), 0);
            let name: Vec<u16> = src[20..84]
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|c| *c != 0)
                .collect();
            assert_eq!(String::from_utf16_lossy(&name), "\\\\.\\DISPLAY1");
            let mut tgt = [0u8; 420];
            tgt[0..4].copy_from_slice(&2u32.to_le_bytes());
            tgt[4..8].copy_from_slice(&420u32.to_le_bytes());
            tgt[16..20].copy_from_slice(&paths[28..32]);
            assert_eq!(DisplayConfigGetDeviceInfo(tgt.as_mut_ptr()), 0);
            assert_ne!(tgt[36], 0);
            // Taille trop courte refusée.
            tgt[4..8].copy_from_slice(&40u32.to_le_bytes());
            assert_eq!(DisplayConfigGetDeviceInfo(tgt.as_mut_ptr()), 87);
        }
    }

    #[test]
    fn notifications_de_session() {
        let _teb = crate::process::test_teb();
        unsafe {
            assert_eq!(WTSRegisterSessionNotification(0x1234, 0), 1);
            assert_eq!(WTSRegisterSessionNotificationEx(0, 0x1234, 1), 1);
            assert_eq!(WTSUnRegisterSessionNotification(0x1234), 1);
            assert_eq!(WTSRegisterSessionNotification(0, 0), 0);
            let a = RegisterPowerSettingNotification(0x1234, 0, 0);
            let b = RegisterPowerSettingNotification(0x1234, 0, 0);
            assert!(a != 0 && b != 0 && a != b);
            let mut kind = 0u32;
            assert_eq!(GetPointerType(1, &mut kind), 1);
            assert_eq!(kind, 4);
            let mut filter = [8u32, 9];
            assert_eq!(
                ChangeWindowMessageFilterEx(0x1234, 0x4a, 1, filter.as_mut_ptr()),
                1
            );
            assert_eq!(filter[1], 0);
        }
        // Trouvées par GetProcAddress à côté de celles de sandbox_dlls.
        let all = all();
        let wts: Vec<_> = all.iter().filter(|(n, _)| *n == "wtsapi32").collect();
        assert!(wts
            .iter()
            .any(|(_, e)| e.contains_key("WTSRegisterSessionNotification")));
    }

    #[test]
    fn taille_d_une_section() {
        let _teb = crate::process::test_teb();
        unsafe {
            let h = crate::dlls::mmap::create_file_mapping_w(
                usize::MAX,
                0,
                4,
                0,
                0x40000,
                std::ptr::null(),
            );
            assert_ne!(h, 0);
            let mut info = [0u8; 24];
            let mut got = 0usize;
            assert_eq!(NtQuerySection(h, 0, info.as_mut_ptr(), 24, &mut got), 0);
            assert_eq!(got, 24);
            assert_eq!(
                u64::from_le_bytes(info[16..24].try_into().unwrap()),
                0x40000
            );
            assert_eq!(
                NtQuerySection(h, 0, info.as_mut_ptr(), 8, &mut got),
                0xC000_0004
            );
            assert_eq!(
                NtQuerySection(0x99_9999, 0, info.as_mut_ptr(), 24, &mut got),
                0xC000_0008
            );
            // Disposition x86 (#1650) : 16 octets, MaximumSize à 8, SIZE_T
            // rendu sur 4 octets sans toucher aux 4 suivants.
            let mut small = [0u8; 16];
            let mut got32 = [0u32, 0xDEAD_BEEF];
            assert_eq!(
                query_section(
                    h,
                    0,
                    small.as_mut_ptr(),
                    16,
                    got32.as_mut_ptr().cast(),
                    true
                ),
                0
            );
            assert_eq!(got32, [16, 0xDEAD_BEEF]);
            assert_eq!(
                u64::from_le_bytes(small[8..16].try_into().unwrap()),
                0x40000
            );
            assert_eq!(
                u32::from_le_bytes(small[4..8].try_into().unwrap()),
                0x0800_0000
            );
            crate::dlls::kernel32::close_handle(h);
        }
    }

    /// Une clé ouverte est un objet « Key » nommé \\REGISTRY\\… ; un tampon
    /// trop court rend la taille utile (#2413).
    /// Station et bureau du bac à sable : DuplicateHandle les copie (#2443).
    #[test]
    fn duplicatehandle_copie_station_et_bureau() {
        let _teb = crate::process::test_teb();
        unsafe {
            let station = CreateWindowStation(0, 0, 0x37f, 0);
            let desktop = CreateDesktop(0, 0, 0, 0, 0x1ff, 0, 0, 0);
            let me = crate::dlls::kernel32::get_current_process();
            for h in [station, desktop, WINSTA0, DEFAULT_DESKTOP] {
                let mut copy = 0usize;
                assert_eq!(
                    crate::dlls::kernel32::duplicate_handle(me, h, me, &mut copy, 0, 0, 2),
                    1,
                    "{h:#x}"
                );
                assert_ne!(copy, 0);
            }
            // Un numéro jamais servi reste refusé.
            let mut copy = 0usize;
            assert_eq!(
                crate::dlls::kernel32::duplicate_handle(me, 0x5753_00f0, me, &mut copy, 0, 0, 2),
                0
            );
        }
    }

    #[test]
    fn nom_nt_d_une_cle_du_registre() {
        let _teb = crate::process::test_teb();
        let hkcu = crate::registry::new_handle("hkcu\\Software\\Essai2413".into());
        let hklm = crate::registry::new_handle("hklm".into());
        let read = |h: usize, class: u32| unsafe {
            let mut buf = [0u64; 64];
            let mut got = 0u32;
            assert_eq!(
                NtQueryObject(h, class, buf.as_mut_ptr().cast(), 512, &mut got),
                0
            );
            let b = buf.as_ptr() as *const u8;
            let n = (b as *const u16).read_unaligned() as usize / 2;
            let p = (b.add(8) as *const usize).read_unaligned() as *const u16;
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        };
        assert_eq!(read(hkcu, 2), "Key");
        assert_eq!(
            read(hkcu, 1),
            format!(
                "\\REGISTRY\\USER\\{}\\Software\\Essai2413",
                crate::dlls::security::user_sid()
            )
        );
        assert_eq!(read(hklm, 1), "\\REGISTRY\\MACHINE");
        unsafe {
            let mut small = [0u64; 2];
            let mut got = 0u32;
            assert_eq!(
                NtQueryObject(hkcu, 1, small.as_mut_ptr().cast(), 16, &mut got),
                0xC000_0004
            );
            assert!(got > 16);
            // Racine prédéfinie : pas un handle d'objet.
            assert_eq!(
                NtQueryObject(0x8000_0001, 1, small.as_mut_ptr().cast(), 16, &mut got),
                0xC000_0008
            );
        }
        crate::registry::close_handle(hkcu);
        crate::registry::close_handle(hklm);
        unsafe {
            let mut small = [0u64; 2];
            let mut got = 0u32;
            assert_eq!(
                NtQueryObject(hkcu, 1, small.as_mut_ptr().cast(), 16, &mut got),
                0xC000_0008
            );
        }
    }

    #[test]
    fn type_des_handles() {
        let _teb = crate::process::test_teb();
        unsafe {
            let section = crate::dlls::mmap::create_file_mapping_w(
                usize::MAX,
                0,
                4,
                0,
                4096,
                std::ptr::null(),
            );
            let name = |h: usize| {
                let mut buf = [0u64; 32];
                let mut got = 0u32;
                assert_eq!(
                    NtQueryObject(h, 2, buf.as_mut_ptr().cast(), 256, &mut got),
                    0
                );
                let b = buf.as_ptr() as *const u8;
                let n = (b as *const u16).read_unaligned() as usize / 2;
                let p = (b.add(8) as *const usize).read_unaligned() as *const u16;
                String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
            };
            assert_eq!(name(section), "Section");
            let path: Vec<u16> = "/proc/self/exe".encode_utf16().chain(Some(0)).collect();
            let file =
                crate::dlls::kernel32::create_file_w(path.as_ptr(), 0x8000_0000, 1, 0, 3, 0, 0);
            assert_ne!(file, usize::MAX);
            assert_eq!(name(file), "File");
            assert_eq!(name(crate::dlls::security::TOKEN_HANDLE), "Token");
            let mut basic = [0u8; 56];
            let mut got = 0u32;
            assert_eq!(
                NtQueryObject(file, 0, basic.as_mut_ptr().cast(), 56, &mut got),
                0
            );
            assert_eq!(got, 56);
            assert_eq!(
                NtQueryObject(file, 0, basic.as_mut_ptr().cast(), 8, &mut got),
                0xC000_0004
            );
            assert_eq!(
                NtQueryObject(0x7777_7777, 2, basic.as_mut_ptr().cast(), 56, &mut got),
                0xC000_0008
            );
            crate::dlls::kernel32::close_handle(file);
            crate::dlls::kernel32::close_handle(section);
            // Jetons du bac à sable : copie, jeton restreint, fermeture.
            let mut t = 0usize;
            assert_eq!(
                crate::dlls::security::DuplicateTokenEx(
                    crate::dlls::security::TOKEN_HANDLE,
                    0,
                    0,
                    2,
                    1,
                    &mut t
                ),
                1
            );
            assert_eq!(crate::dlls::kernel32::close_handle(t), 1);
            let mut size = 68u32;
            let mut sid = [0u8; 68];
            assert_eq!(
                crate::dlls::security::CreateWellKnownSid(
                    66,
                    std::ptr::null(),
                    sid.as_mut_ptr(),
                    &mut size
                ),
                1
            );
            assert_eq!(
                crate::dlls::security::sid_text(&sid[..size as usize]).as_deref(),
                Some("S-1-16-4096")
            );
        }
    }

    #[test]
    fn filtres_de_noms_et_droits_generiques() {
        assert!(path_match_spec(
            "C:\\Docs\\Rapport.TXT",
            "*.txt",
            true,
            true
        ));
        assert!(path_match_spec("photo.jpeg", "*.png; *.JPEG", true, true));
        assert!(!path_match_spec("photo.jpeg", "*.png; *.JPEG", false, true));
        assert!(path_match_spec("LISEZMOI", "*.*", true, true));
        assert!(path_match_spec("été.md", "?té.*", true, true));
        assert!(!path_match_spec("a.txt", "*.md", true, true));
        unsafe {
            let f: Vec<u16> = "Notes.Log".encode_utf16().chain(Some(0)).collect();
            let p: Vec<u16> = "*.txt;*.log".encode_utf16().chain(Some(0)).collect();
            assert_eq!(PathMatchSpecW(f.as_ptr(), p.as_ptr()), 1);
            assert_eq!(PathMatchSpecExW(f.as_ptr(), p.as_ptr(), 1), 0);
            assert_eq!(PathMatchSpecExW(f.as_ptr(), p.as_ptr(), 0), 1);
            let mut mask = 0x8000_0001u32; // GENERIC_READ | 1
            let mapping = [0x10u32, 0x20, 0x40, 0x1FF];
            MapGenericMask(&mut mask, mapping.as_ptr());
            assert_eq!(mask, 0x11);
        }
        assert_eq!(map_generic_mask(0x1000_0000, [1, 2, 4, 0x1F]), 0x1F);
    }

    #[test]
    fn emplacement_reseau_nla() {
        let _teb = crate::process::test_teb();
        unsafe {
            let mut q = [0u8; WSAQUERYSETW_SIZE];
            q[40] = NS_NLA as u8;
            let mut h = 0usize;
            assert_eq!(WSALookupServiceBeginW(q.as_mut_ptr().cast(), 0, &mut h), 0);
            assert_ne!(h, 0);
            let mut out = [0u8; 256];
            let mut len = 8u32;
            let connected = network_connected();
            let r = WSALookupServiceNextW(h, 0, &mut len, out.as_mut_ptr().cast());
            assert_eq!(r, SOCKET_ERROR);
            if connected {
                assert!(len as usize > WSAQUERYSETW_SIZE);
                assert_eq!(
                    WSALookupServiceNextW(h, 0, &mut len, out.as_mut_ptr().cast()),
                    0
                );
                assert_eq!(u32::from_le_bytes(out[40..44].try_into().unwrap()), NS_NLA);
                assert_ne!(usize::from_le_bytes(out[8..16].try_into().unwrap()), 0);
            }
            // Un seul emplacement : ensuite, plus rien.
            assert_eq!(
                WSALookupServiceNextW(h, 0, &mut len, out.as_mut_ptr().cast()),
                SOCKET_ERROR
            );
            assert_eq!(WSALookupServiceEnd(h), 0);
            // Autre espace de noms : introuvable, comme avant.
            q[40] = 12;
            assert_eq!(
                WSALookupServiceBeginW(q.as_mut_ptr().cast(), 0, &mut h),
                SOCKET_ERROR
            );
        }
    }

    #[test]
    fn verification_d_acces_et_raccourcis() {
        let _teb = crate::process::test_teb();
        let mapping = [0x10u32, 0x20, 0x40, 0x1FF];
        assert_eq!(access_check_granted(0x8000_0000, mapping), 0x10);
        assert_eq!(access_check_granted(0x0200_0000, mapping), 0x1FF);
        unsafe {
            let sd = [0u8; 20];
            let mut privs = [0xFFFF_FFFFu32; 5];
            let mut len = 20u32;
            let (mut granted, mut ok) = (0u32, 0i32);
            assert_eq!(
                AccessCheck(
                    sd.as_ptr() as usize,
                    crate::dlls::security::TOKEN_HANDLE,
                    0x4000_0000,
                    mapping.as_ptr(),
                    privs.as_mut_ptr(),
                    &mut len,
                    &mut granted,
                    &mut ok,
                ),
                1
            );
            assert_eq!((granted, ok, privs[0]), (0x20, 1, 0));
            let mut small = 4u32;
            assert_eq!(
                AccessCheck(
                    sd.as_ptr() as usize,
                    0,
                    1,
                    mapping.as_ptr(),
                    privs.as_mut_ptr(),
                    &mut small,
                    &mut granted,
                    &mut ok,
                ),
                0
            );
            assert_eq!(small, 8);
            assert_eq!(RegisterHotKey(0, 1, 2, 0x41), 1);
            assert_eq!(UnregisterHotKey(0, 1), 1);
            assert_eq!(UnregisterHotKey(0, 1), 0);
        }
    }

    /// Chaîne bâtie par le programme lui-même, comme C++/WinRT et
    /// windows-rs : { drapeaux, longueur, 0, 0, texte, compteur, texte }.
    #[test]
    fn chaine_winrt_batie_par_le_programme() {
        unsafe {
            #[repr(C)]
            struct Header {
                flags: u32,
                len: u32,
                r0: u32,
                r1: u32,
                data: *const u16,
                count: u32,
                buffer: [u16; 32],
            }
            let nom: Vec<u16> = "Windows.Foundation.Uri".encode_utf16().collect();
            let mut b = Box::new(Header {
                flags: 0,
                len: nom.len() as u32,
                r0: 0,
                r1: 0,
                data: std::ptr::null(),
                count: 1,
                buffer: [0; 32],
            });
            b.buffer[..nom.len()].copy_from_slice(&nom);
            b.data = b.buffer.as_ptr();
            let h = Box::into_raw(b) as usize;
            let count =
                || std::ptr::read_volatile(std::ptr::addr_of!((*(h as *const Header)).count));
            assert_eq!(hstring_slice(h), &nom[..]);
            assert_eq!(WindowsGetStringLen(h), nom.len() as u32);
            // Dupliquer une chaîne allouée ne fait que compter une référence.
            let mut dup = 0usize;
            assert_eq!(WindowsDuplicateString(h, &mut dup), 0);
            assert_eq!(dup, h);
            assert_eq!(count(), 2);
            assert_eq!(WindowsDeleteString(dup), 0);
            assert_eq!(count(), 1);
            // Et nos chaînes suivent la même disposition.
            let mut ours = 0usize;
            assert_eq!(
                WindowsCreateString(nom.as_ptr(), nom.len() as u32, &mut ours),
                0
            );
            let o = &*(ours as *const Header);
            assert_eq!((o.flags, o.len, o.count), (0, nom.len() as u32, 1));
            assert_eq!(
                std::slice::from_raw_parts(o.data, nom.len() + 1)[nom.len()],
                0
            );
            assert_eq!(WindowsDuplicateString(ours, &mut dup), 0);
            assert_eq!(dup, ours);
            assert_eq!(WindowsDeleteString(ours), 0);
            assert_eq!(WindowsDeleteString(dup), 0);
            drop(Box::from_raw(h as *mut Header));
        }
    }

    #[test]
    fn chaines_winrt() {
        unsafe {
            let a: Vec<u16> = "Claude".encode_utf16().collect();
            let mut h = 0usize;
            assert_eq!(WindowsCreateString(a.as_ptr(), 6, &mut h), 0);
            assert_ne!(h, 0);
            let mut n = 0u32;
            let p = WindowsGetStringRawBuffer(h, &mut n);
            assert_eq!(n, 6);
            assert_eq!(std::slice::from_raw_parts(p, 7), [&a[..], &[0]].concat());
            let z: Vec<u16> = "Claude\0".encode_utf16().collect();
            let mut header = [0u64; 3];
            let mut r = 0usize;
            assert_eq!(
                WindowsCreateStringReference(z.as_ptr(), 6, header.as_mut_ptr().cast(), &mut r),
                0
            );
            let mut cmp = 9;
            assert_eq!(WindowsCompareStringOrdinal(h, r, &mut cmp), 0);
            assert_eq!(cmp, 0);
            let mut both = 0usize;
            assert_eq!(WindowsConcatString(h, r, &mut both), 0);
            assert_eq!(WindowsGetStringLen(both), 12);
            let mut sub = 0usize;
            assert_eq!(WindowsSubstring(both, 6, &mut sub), 0);
            assert_eq!(WindowsCompareStringOrdinal(sub, h, &mut cmp), 0);
            assert_eq!(cmp, 0);
            let mut dup = 0usize;
            assert_eq!(WindowsDuplicateString(r, &mut dup), 0);
            assert_ne!(dup, r, "une référence se copie en vraie chaîne");
            for x in [h, r, both, sub, dup, 0] {
                assert_eq!(WindowsDeleteString(x), 0);
            }
            assert_eq!(WindowsIsStringEmpty(0), 1);
            assert_eq!(*WindowsGetStringRawBuffer(0, &mut n), 0);
            let mut f = 1usize;
            assert_eq!(RoGetActivationFactory(h, 0, &mut f), REGDB_E_CLASSNOTREG);
            assert_eq!(f, 0);
            assert!(crate::dlls::lookup("ole32", "WindowsCreateString").is_some());
            assert!(crate::dlls::lookup(
                &crate::dlls::normalize("combase.dll"),
                "WindowsCreateString"
            )
            .is_some());
            // NtSuspendProcess / NtResumeProcess sur un enfant.
            let mut child = std::process::Command::new("sleep")
                .arg("5")
                .spawn()
                .unwrap();
            let hp = crate::dlls::kernel32::open_process(0x1fffff, 0, child.id());
            assert_eq!(NtSuspendProcess(hp), 0);
            assert_eq!(NtResumeProcess(hp), 0);
            let _ = child.kill();
            let _ = child.wait();
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
