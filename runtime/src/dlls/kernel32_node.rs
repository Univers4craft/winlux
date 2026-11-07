//! Fonctions de kernel32/ntdll demandées par les runtimes JavaScript
//! compilés pour Windows (Bun, Node.js/libuv), et en premier lieu par
//! l'exécutable Windows de Claude Code (`claude.exe`, un binaire Bun).
//!
//! Chaque fonction suit la documentation publique de Microsoft et
//! s'appuie sur l'équivalent Linux : futex pour WaitOnAddress, la pile
//! décrite dans le TEB pour GetCurrentThreadStackLimits, etc.
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

/// TEB->DeallocationStack : bas de la réservation de la pile.
const TEB_DEALLOCATION_STACK: usize = 0x1478;

/// `GetCurrentThreadStackLimits(PULONG_PTR low, PULONG_PTR high)`.
/// JavaScriptCore borne sa pile avec ces valeurs : des zéros lui font
/// croire à un débordement de pile immédiat.
pub unsafe extern "win64" fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize) {
    let teb = crate::process::teb();
    let (mut lo, mut hi) = (0usize, 0usize);
    if teb != 0 {
        hi = *((teb + crate::process::TEB_STACK_BASE) as *const usize);
        lo = *((teb + TEB_DEALLOCATION_STACK) as *const usize);
        if lo == 0 {
            lo = *((teb + crate::process::TEB_STACK_LIMIT) as *const usize);
        }
    }
    if hi == 0 {
        // Pas de TEB : on décrit la pile pthread courante.
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        let (mut addr, mut size) = (std::ptr::null_mut(), 0usize);
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) == 0 {
            libc::pthread_attr_getstack(&attr, &mut addr, &mut size);
            libc::pthread_attr_destroy(&mut attr);
        }
        lo = addr as usize;
        hi = lo + size;
    }
    if !low.is_null() {
        *low = lo;
    }
    if !high.is_null() {
        *high = hi;
    }
}

/// Winlux n'implémente pas les fibres en mode « thread converti » : aucun
/// thread n'est une fibre tant que ConvertThreadToFiber n'a pas réussi.
pub unsafe extern "win64" fn IsThreadAFiber() -> i32 {
    0
}

/// `DebugBreak` : lève EXCEPTION_BREAKPOINT comme sous Windows. Sans
/// débogueur, le gestionnaire de plantage de Winlux la rapporte.
pub unsafe extern "win64" fn DebugBreak() {
    eprintln!("winlux: le programme a appelé DebugBreak (point d'arrêt explicite)");
    std::arch::asm!("int3");
}

// ---- WaitOnAddress / WakeByAddress* (synch-l1-2-0) -----------------------

fn futex_word(addr: usize) -> (usize, u32, u32) {
    // Le futex Linux travaille sur des mots de 32 bits alignés ; on attend
    // sur le mot qui contient l'adresse, et on compare la valeur exacte
    // (1, 2, 4 ou 8 octets) avant chaque attente.
    let word = addr & !3;
    (word, (addr - word) as u32, 0)
}

unsafe fn same_value(addr: usize, compare: usize, size: usize) -> bool {
    match size {
        1 => *(addr as *const u8) == *(compare as *const u8),
        2 => (addr as *const u16).read_unaligned() == (compare as *const u16).read_unaligned(),
        4 => (addr as *const u32).read_unaligned() == (compare as *const u32).read_unaligned(),
        8 => (addr as *const u64).read_unaligned() == (compare as *const u64).read_unaligned(),
        _ => false,
    }
}

/// `WaitOnAddress(addr, compare, size, ms)` : TRUE au réveil, FALSE avec
/// ERROR_TIMEOUT à l'expiration. Les réveils parasites sont permis par la
/// documentation (l'appelant revérifie toujours sa condition).
pub unsafe extern "win64" fn WaitOnAddress(
    addr: usize,
    compare: usize,
    size: usize,
    ms: u32,
) -> i32 {
    if addr == 0 || compare == 0 || !matches!(size, 1 | 2 | 4 | 8) {
        crate::process::set_last_error(87);
        return 0;
    }
    if !same_value(addr, compare, size) {
        return 1;
    }
    let (word, _, _) = futex_word(addr);
    let current = (*(word as *const AtomicU32)).load(Ordering::SeqCst);
    // Revérifie après la lecture du mot pour ne pas rater un changement.
    if !same_value(addr, compare, size) {
        return 1;
    }
    let ts;
    let tsp = if ms == u32::MAX {
        std::ptr::null()
    } else {
        ts = libc::timespec {
            tv_sec: (ms / 1000) as libc::time_t,
            tv_nsec: ((ms % 1000) as libc::c_long) * 1_000_000,
        };
        &ts as *const libc::timespec
    };
    let rc = libc::syscall(
        libc::SYS_futex,
        word as *const u32,
        libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
        current,
        tsp,
    );
    if rc != 0 && *libc::__errno_location() == libc::ETIMEDOUT {
        crate::process::set_last_error(1460); // ERROR_TIMEOUT
        return 0;
    }
    1
}

unsafe fn wake(addr: usize, count: i32) {
    if addr == 0 {
        return;
    }
    let (word, _, _) = futex_word(addr);
    libc::syscall(
        libc::SYS_futex,
        word as *const u32,
        libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
        count,
    );
}

pub unsafe extern "win64" fn WakeByAddressSingle(addr: usize) {
    // Plusieurs adresses peuvent partager un mot de 32 bits : réveiller
    // tous les attentifs de ce mot reste correct (réveils parasites admis).
    wake(addr, i32::MAX);
}
pub unsafe extern "win64" fn WakeByAddressAll(addr: usize) {
    wake(addr, i32::MAX);
}

/// `RtlWaitOnAddress(addr, compare, size, PLARGE_INTEGER timeout)` :
/// NTSTATUS, délai NT (négatif = relatif en unités de 100 ns, NULL = infini).
pub unsafe extern "win64" fn RtlWaitOnAddress(
    addr: usize,
    compare: usize,
    size: usize,
    timeout: *const i64,
) -> u32 {
    let ms = if timeout.is_null() {
        u32::MAX
    } else {
        let t = *timeout;
        if t < 0 {
            ((-t) / 10_000).min(u32::MAX as i64 - 1) as u32
        } else {
            0
        }
    };
    if WaitOnAddress(addr, compare, size, ms) != 0 {
        0
    } else {
        0x102 // STATUS_TIMEOUT
    }
}

// ---- Mémoire --------------------------------------------------------------

/// `VirtualUnlock` : comme sous Windows, déverrouiller des pages qui ne
/// sont pas verrouillées échoue avec ERROR_NOT_LOCKED. JavaScriptCore s'en
/// sert après MEM_RESET pour rendre des pages au système : on les libère
/// réellement avec madvise(MADV_DONTNEED) n'est pas sûr (contenu perdu
/// alors que l'appelant peut les relire) ; on se contente du code d'erreur.
pub unsafe extern "win64" fn VirtualUnlock(addr: usize, size: usize) -> i32 {
    if addr != 0 && size != 0 && libc::munlock(addr as *const c_void, size) == 0 {
        return 1;
    }
    crate::process::set_last_error(158); // ERROR_NOT_LOCKED
    0
}

// ---- Fichiers -------------------------------------------------------------

const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;

/// `DeviceIoControl` : FSCTL_GET_REPARSE_POINT (cible d'un lien
/// symbolique) ; les autres codes n'ont pas d'équivalent Linux.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn DeviceIoControl(
    h: usize,
    code: u32,
    inb: *const u8,
    inl: u32,
    outb: *mut u8,
    outl: u32,
    returned: *mut u32,
    ov: *mut c_void,
) -> i32 {
    use crate::dlls::devio;
    if !returned.is_null() {
        *returned = 0;
    }
    // Fin immédiate : OVERLAPPED.Internal / InternalHigh, puis l'événement.
    let finish = |n: u32, status: usize| {
        if !returned.is_null() {
            *returned = n;
        }
        if !ov.is_null() {
            (ov as *mut usize).write(status);
            (ov as *mut usize).add(1).write(n as usize);
            let ev = (ov as *const usize).add(3).read();
            if ev != 0 {
                crate::dlls::kernel32::set_event(ev);
            }
        }
    };
    // Rapports HID par code de contrôle (hidapi, outils de manettes).
    match code {
        devio::IOCTL_HID_GET_FEATURE | devio::IOCTL_HID_GET_INPUT_REPORT => {
            let f = if code == devio::IOCTL_HID_GET_FEATURE {
                crate::dlls::hid::HidD_GetFeature
            } else {
                crate::dlls::hid::HidD_GetInputReport
            };
            if f(h, outb, outl) == 0 {
                return 0;
            }
            finish(outl, 0);
            return 1;
        }
        devio::IOCTL_HID_SET_FEATURE | devio::IOCTL_HID_SET_OUTPUT_REPORT => {
            let f = if code == devio::IOCTL_HID_SET_FEATURE {
                crate::dlls::hid::HidD_SetFeature
            } else {
                crate::dlls::hid::HidD_SetOutputReport
            };
            if f(h, inb as *mut u8, inl) == 0 {
                return 0;
            }
            finish(0, 0);
            return 1;
        }
        _ => {}
    }
    let Some(fd) = crate::dlls::kernel32::fd_of_handle(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if code == FSCTL_GET_REPARSE_POINT {
        let mut iosb = [0u64; 2];
        let st = crate::dlls::ntdll_file::reparse_point_of_fd(
            fd,
            outb,
            outl,
            iosb.as_mut_ptr() as *mut c_void,
        );
        if st == 0 {
            finish(iosb[1] as u32, 0);
            return 1;
        }
        crate::process::set_last_error(if st == 0xC000_0023 { 122 } else { 4390 });
        return 0;
    }
    let input = if inb.is_null() || inl == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(inb, inl as usize)
    };
    let (data, error) = match devio::control(fd, code, input, outl as usize) {
        devio::Reply::Data(d) => (d, 0),
        devio::Reply::Header(d) => (d, devio::header_error(code)),
        devio::Reply::Error(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    if data.len() > outl as usize || (!data.is_empty() && outb.is_null()) {
        crate::process::set_last_error(devio::ERROR_BUFFER);
        return 0;
    }
    if !data.is_empty() {
        std::ptr::copy_nonoverlapping(data.as_ptr(), outb, data.len());
    }
    if error != 0 {
        // ERROR_MORE_DATA : réponse partielle, STATUS_BUFFER_OVERFLOW.
        finish(data.len() as u32, 0x8000_0005);
        crate::process::set_last_error(error);
        return 0;
    }
    finish(data.len() as u32, 0);
    crate::process::set_last_error(0);
    1
}

/// `ReOpenFile` : nouveau handle sur le même fichier, éventuellement en
/// mode asynchrone (FILE_FLAG_OVERLAPPED), utilisé par libuv pour ses tubes.
pub unsafe extern "win64" fn ReOpenFile(h: usize, _access: u32, _share: u32, flags: u32) -> usize {
    let h = crate::dlls::kernel32::canon_std(h);
    let copy = match h as isize {
        -12..=-10 => {
            let fd = libc::fcntl((-(h as isize) - 10) as i32, libc::F_DUPFD_CLOEXEC, 3);
            if fd < 0 {
                None
            } else {
                Some(crate::dlls::kernel32::insert_file(
                    std::os::unix::io::FromRawFd::from_raw_fd(fd),
                ))
            }
        }
        _ => crate::dlls::kernel32::duplicate_handle_impl(h),
    };
    match copy {
        Some(n) => {
            if flags & 0x4000_0000 != 0 {
                crate::dlls::kernel32::mark_overlapped_handle(n);
            }
            n
        }
        None => {
            crate::process::set_last_error(6);
            usize::MAX
        }
    }
}

/// `SetFileCompletionNotificationModes` : non pris en charge ; libuv
/// attend alors exactement ERROR_INVALID_FUNCTION pour continuer sans.
pub unsafe extern "win64" fn SetFileCompletionNotificationModes(_h: usize, _flags: u8) -> i32 {
    crate::process::set_last_error(1);
    0
}

pub unsafe extern "win64" fn CancelSynchronousIo(_thread: usize) -> i32 {
    crate::process::set_last_error(1168); // ERROR_NOT_FOUND
    0
}

/// `LockFileEx` / `UnlockFileEx` : verrous de plage POSIX (fcntl).
pub unsafe extern "win64" fn LockFileEx(
    h: usize,
    flags: u32,
    _reserved: u32,
    len_lo: u32,
    len_hi: u32,
    ov: *mut c_void,
) -> i32 {
    file_lock(h, flags, len_lo, len_hi, ov, true)
}
pub unsafe extern "win64" fn UnlockFileEx(
    h: usize,
    _reserved: u32,
    len_lo: u32,
    len_hi: u32,
    ov: *mut c_void,
) -> i32 {
    file_lock(h, 0, len_lo, len_hi, ov, false)
}
unsafe fn file_lock(
    h: usize,
    flags: u32,
    len_lo: u32,
    len_hi: u32,
    ov: *mut c_void,
    lock: bool,
) -> i32 {
    let Some(fd) = crate::dlls::kernel32::fd_of_handle(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let offset = if ov.is_null() {
        0
    } else {
        let lo = (ov as *const u32).add(4).read() as u64;
        let hi = (ov as *const u32).add(5).read() as u64;
        (hi << 32) | lo
    };
    let len = ((len_hi as u64) << 32) | len_lo as u64;
    let mut fl: libc::flock = std::mem::zeroed();
    fl.l_type = if !lock {
        libc::F_UNLCK
    } else if flags & 2 != 0 {
        libc::F_WRLCK
    } else {
        libc::F_RDLCK
    } as i16;
    fl.l_whence = libc::SEEK_SET as i16;
    fl.l_start = offset.min(i64::MAX as u64) as i64;
    fl.l_len = if len >= i64::MAX as u64 {
        0
    } else {
        len as i64
    };
    let cmd = if lock && flags & 1 == 0 {
        libc::F_OFD_SETLKW
    } else {
        libc::F_OFD_SETLK
    };
    if libc::fcntl(fd, cmd, &fl) != 0 {
        let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if e == libc::EBADF && lock && flags & 2 != 0 {
            // Verrou exclusif sur un fichier ouvert en lecture seule :
            // Windows l'accepte, POSIX non ; on prend un verrou partagé.
            fl.l_type = libc::F_RDLCK as i16;
            if libc::fcntl(fd, cmd, &fl) == 0 {
                return 1;
            }
        }
        crate::process::set_last_error(if lock { 33 } else { 158 }); // LOCK_VIOLATION / NOT_LOCKED
        return 0;
    }
    if !ov.is_null() {
        (ov as *mut usize).write(0);
    }
    1
}

/// `SetFileInformationByHandle` : classes Win32 → classes NT équivalentes.
pub unsafe extern "win64" fn SetFileInformationByHandle(
    h: usize,
    class: u32,
    buf: *const u8,
    len: u32,
) -> i32 {
    let nt = match class {
        0 => 4,   // FileBasicInfo
        3 => 10,  // FileRenameInfo
        4 => 13,  // FileDispositionInfo
        5 => 19,  // FileAllocationInfo
        6 => 20,  // FileEndOfFileInfo
        21 => 64, // FileDispositionInfoEx
        22 => 65, // FileRenameInfoEx
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    let mut iosb = [0u64; 2];
    let st = crate::dlls::ntdll_file::NtSetInformationFile(
        h,
        iosb.as_mut_ptr() as *mut c_void,
        buf,
        len,
        nt,
    );
    if st != 0 {
        crate::process::set_last_error(crate::dlls::ntdll::rtl_nt_status_to_dos_error(st as i32));
        return 0;
    }
    1
}

pub unsafe extern "win64" fn CreateDirectoryExW(
    _template: *const u16,
    path: *const u16,
    sa: usize,
) -> i32 {
    crate::dlls::kernel32::create_directory_w(path, sa)
}

pub unsafe extern "win64" fn GetDiskFreeSpaceW(
    root: *const u16,
    sectors_per_cluster: *mut u32,
    bytes_per_sector: *mut u32,
    free_clusters: *mut u32,
    total_clusters: *mut u32,
) -> i32 {
    let path = if root.is_null() {
        "C:\\".to_string()
    } else {
        crate::dlls::wstr(root)
    };
    let Some((avail, total, _)) = crate::dlls::kernel32::disk_free(&path) else {
        crate::process::set_last_error(3);
        return 0;
    };
    let cluster = 4096u64;
    let put = |p: *mut u32, v: u64| {
        if !p.is_null() {
            *p = v.min(u32::MAX as u64) as u32;
        }
    };
    put(sectors_per_cluster, 8);
    put(bytes_per_sector, 512);
    put(free_clusters, avail / cluster);
    put(total_clusters, total / cluster);
    1
}
pub unsafe extern "win64" fn GetDiskFreeSpaceA(
    root: *const u8,
    spc: *mut u32,
    bps: *mut u32,
    free: *mut u32,
    total: *mut u32,
) -> i32 {
    let w: Vec<u16> = if root.is_null() {
        vec![0]
    } else {
        crate::dlls::cstr(root)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    };
    GetDiskFreeSpaceW(
        if root.is_null() {
            std::ptr::null()
        } else {
            w.as_ptr()
        },
        spc,
        bps,
        free,
        total,
    )
}

// ---- Attentes enregistrées et file de travail ------------------------------

struct RegisteredWait {
    cancelled: std::sync::atomic::AtomicBool,
    done: std::sync::Mutex<bool>,
    finished: std::sync::Condvar,
}

static WAITS: std::sync::Mutex<
    Option<std::collections::HashMap<usize, std::sync::Arc<RegisteredWait>>>,
> = std::sync::Mutex::new(None);
static NEXT_WAIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x5741_0000);

/// Un thread Winlux prêt à exécuter du code Windows (TEB, GS).
fn spawn_windows_thread(f: impl FnOnce() + Send + 'static) -> bool {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            crate::process::init_thread();
            f();
        })
        .is_ok()
}

/// `RegisterWaitForSingleObject` : libuv l'utilise pour apprendre la fin
/// de chaque processus enfant. Un thread attend l'objet puis appelle
/// `callback(context, timed_out)`.
pub unsafe extern "win64" fn RegisterWaitForSingleObject(
    out: *mut usize,
    object: usize,
    callback: usize,
    context: usize,
    ms: u32,
    flags: u32,
) -> i32 {
    if out.is_null() || callback == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let id = NEXT_WAIT.fetch_add(4, Ordering::Relaxed);
    let w = std::sync::Arc::new(RegisteredWait {
        cancelled: std::sync::atomic::AtomicBool::new(false),
        done: std::sync::Mutex::new(false),
        finished: std::sync::Condvar::new(),
    });
    crate::dlls::lock(&WAITS)
        .get_or_insert_with(std::collections::HashMap::new)
        .insert(id, w.clone());
    let once = flags & 0x8 != 0; // WT_EXECUTEONLYONCE
    let ok = spawn_windows_thread(move || {
        let cb: extern "win64" fn(usize, u8) = std::mem::transmute(callback);
        loop {
            // Attente par tranches pour pouvoir être annulée.
            let mut waited = 0u32;
            let result = loop {
                if w.cancelled.load(Ordering::Acquire) {
                    break None;
                }
                let slice = if ms == u32::MAX {
                    100
                } else {
                    (ms - waited).min(100)
                };
                let r = crate::dlls::kernel32::wait_for_single_object(object, slice);
                if r != 0x102 {
                    break Some(r);
                }
                waited = waited.saturating_add(slice);
                if ms != u32::MAX && waited >= ms {
                    break Some(0x102);
                }
            };
            let Some(r) = result else { break };
            if r == u32::MAX {
                break;
            }
            cb(context, (r == 0x102) as u8);
            if once || w.cancelled.load(Ordering::Acquire) {
                break;
            }
        }
        *crate::dlls::lock(&w.done) = true;
        w.finished.notify_all();
    });
    if !ok {
        crate::process::set_last_error(8);
        return 0;
    }
    *out = id;
    1
}

unsafe fn unregister(id: usize, completion: usize) -> i32 {
    let Some(w) = crate::dlls::lock(&WAITS)
        .as_mut()
        .and_then(|m| m.remove(&id))
    else {
        crate::process::set_last_error(6);
        return 0;
    };
    w.cancelled.store(true, Ordering::Release);
    if completion == usize::MAX {
        // INVALID_HANDLE_VALUE : attendre la fin d'un rappel en cours.
        let mut done = crate::dlls::lock(&w.done);
        while !*done {
            done = w
                .finished
                .wait(done)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    } else if completion != 0 {
        crate::dlls::kernel32::set_event(completion);
    }
    1
}
pub unsafe extern "win64" fn UnregisterWait(id: usize) -> i32 {
    unregister(id, 0)
}
pub unsafe extern "win64" fn UnregisterWaitEx(id: usize, completion: usize) -> i32 {
    unregister(id, completion)
}

/// `QueueUserWorkItem(fn, context, flags)`.
pub unsafe extern "win64" fn QueueUserWorkItem(func: usize, context: usize, _flags: u32) -> i32 {
    if func == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let ok = spawn_windows_thread(move || {
        let f: extern "win64" fn(usize) -> u32 = std::mem::transmute(func);
        f(context);
    });
    if !ok {
        crate::process::set_last_error(8);
        return 0;
    }
    1
}

// ---- Objets job -------------------------------------------------------------

/// Les objets job (libuv en crée un pour tuer ses enfants à sa sortie)
/// sont acceptés sans effet : sous Linux, chaque enfant reste attaché à
/// son groupe de processus et Winlux le termine avec la session.
static NEXT_JOB: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x4A4F_0000);
static JOBS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
pub unsafe extern "win64" fn CreateJobObjectW(_sa: usize, _name: *const u16) -> usize {
    let job = NEXT_JOB.fetch_add(4, Ordering::Relaxed);
    crate::dlls::lock(&JOBS).push(job);
    job
}
/// CloseHandle d'un objet Job : doit réussir, Chromium vérifie la fermeture
/// de chaque poignée (CHECK de ScopedHandle, #767).
pub(crate) fn close_job(h: usize) -> bool {
    let mut jobs = crate::dlls::lock(&JOBS);
    let before = jobs.len();
    jobs.retain(|&j| j != h);
    jobs.len() != before
}
pub unsafe extern "win64" fn job_ok2(_a: usize, _b: usize) -> i32 {
    1
}
pub unsafe extern "win64" fn SetInformationJobObject(
    _job: usize,
    _class: u32,
    _info: usize,
    _len: u32,
) -> i32 {
    1
}

// ---- Processus -----------------------------------------------------------

pub unsafe extern "win64" fn ProcessIdToSessionId(_pid: u32, session: *mut u32) -> i32 {
    if session.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *session = 1;
    1
}

/// PROCESS_MEMORY_COUNTERS(_EX) depuis /proc/self/status.
pub unsafe extern "win64" fn K32GetProcessMemoryInfo(
    _process: usize,
    counters: *mut u8,
    cb: u32,
) -> i32 {
    if counters.is_null() || cb < 72 {
        crate::process::set_last_error(122);
        return 0;
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| -> usize {
        status
            .lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
            * 1024
    };
    let (rss, peak, vm) = (field("VmRSS:"), field("VmHWM:"), field("VmSize:"));
    std::ptr::write_bytes(counters, 0, cb as usize);
    let put = |off: usize, v: usize| (counters.add(off) as *mut usize).write_unaligned(v);
    (counters as *mut u32).write_unaligned(cb.min(80));
    put(8, peak); // PeakWorkingSetSize
    put(16, rss); // WorkingSetSize
    put(56, vm); // PagefileUsage
    put(64, vm); // PeakPagefileUsage
    if cb >= 80 {
        put(72, vm); // PrivateUsage
    }
    1
}

pub unsafe extern "win64" fn GetProcessIoCounters(_process: usize, counters: *mut u8) -> i32 {
    if counters.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    std::ptr::write_bytes(counters, 0, 48);
    let io = std::fs::read_to_string("/proc/self/io").unwrap_or_default();
    let get = |k: &str| {
        io.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let put = |off: usize, v: u64| (counters.add(off) as *mut u64).write_unaligned(v);
    put(0, get("syscr:"));
    put(8, get("syscw:"));
    put(24, get("rchar:"));
    put(32, get("wchar:"));
    1
}

/// Instantané des processus (CreateToolhelp32Snapshot / Process32*W) lu
/// dans /proc : sert à retrouver et terminer l'arbre des processus enfants.
struct Snapshot {
    procs: Vec<(u32, u32, u32, String)>, // pid, ppid, threads, nom
    pos: usize,
}
static SNAPSHOTS: std::sync::Mutex<Option<std::collections::HashMap<usize, Snapshot>>> =
    std::sync::Mutex::new(None);
static NEXT_SNAP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x534E_0000);

pub unsafe extern "win64" fn CreateToolhelp32Snapshot(flags: u32, _pid: u32) -> usize {
    if flags & 0x2 == 0 {
        // Seuls les processus (TH32CS_SNAPPROCESS) sont décrits.
        crate::process::set_last_error(50);
        return usize::MAX;
    }
    let mut procs = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
                continue;
            };
            let name = stat[open + 1..close].to_string();
            let rest: Vec<&str> = stat[close + 1..].split_whitespace().collect();
            let ppid = rest.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            let threads = rest.get(17).and_then(|v| v.parse().ok()).unwrap_or(1);
            let exe = if name.to_ascii_lowercase().ends_with(".exe") {
                name
            } else {
                format!("{name}.exe")
            };
            procs.push((pid, ppid, threads, exe));
        }
    }
    let id = NEXT_SNAP.fetch_add(4, Ordering::Relaxed);
    crate::dlls::lock(&SNAPSHOTS)
        .get_or_insert_with(std::collections::HashMap::new)
        .insert(id, Snapshot { procs, pos: 0 });
    id
}

unsafe fn process_entry(snap: usize, entry: *mut u8, first: bool) -> i32 {
    let mut g = crate::dlls::lock(&SNAPSHOTS);
    let Some(s) = g.as_mut().and_then(|m| m.get_mut(&snap)) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if first {
        s.pos = 0;
    }
    if entry.is_null() || (entry as *const u32).read_unaligned() < 304 {
        crate::process::set_last_error(24); // ERROR_BAD_LENGTH
        return 0;
    }
    let Some((pid, ppid, threads, name)) = s.procs.get(s.pos).cloned() else {
        crate::process::set_last_error(18); // ERROR_NO_MORE_FILES
        return 0;
    };
    s.pos += 1;
    // PROCESSENTRY32W : dwSize, cntUsage, th32ProcessID, th32DefaultHeapID,
    // th32ModuleID, cntThreads, th32ParentProcessID, pcPriClassBase,
    // dwFlags, szExeFile[260].
    std::ptr::write_bytes(entry.add(4), 0, 300);
    (entry.add(8) as *mut u32).write_unaligned(pid);
    (entry.add(28) as *mut u32).write_unaligned(threads);
    (entry.add(32) as *mut u32).write_unaligned(ppid);
    (entry.add(36) as *mut i32).write_unaligned(8);
    let w: Vec<u16> = name.encode_utf16().take(259).collect();
    std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, entry.add(44), w.len() * 2);
    1
}
pub unsafe extern "win64" fn Process32FirstW(snap: usize, entry: *mut u8) -> i32 {
    process_entry(snap, entry, true)
}
pub unsafe extern "win64" fn Process32NextW(snap: usize, entry: *mut u8) -> i32 {
    process_entry(snap, entry, false)
}
/// Fermeture d'un instantané (appelée par CloseHandle).
pub(crate) fn close_snapshot(h: usize) -> bool {
    crate::dlls::lock(&SNAPSHOTS)
        .as_mut()
        .is_some_and(|m| m.remove(&h).is_some())
}

pub unsafe extern "win64" fn GetThreadDescription(_thread: usize, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return 0x8007_0057u32 as i32;
    }
    let p = crate::dlls::kernel32_misc::local_alloc(0x40, 2) as *mut u16;
    *out = p;
    0
}

pub unsafe extern "win64" fn NeedCurrentDirectoryForExePathW(_name: *const u16) -> i32 {
    1
}

/// `GetComputerNameExW` : nom d'hôte Linux pour tous les formats (pas de
/// domaine Active Directory).
pub unsafe extern "win64" fn GetComputerNameExW(kind: u32, buf: *mut u16, size: *mut u32) -> i32 {
    if size.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let name = match kind {
        2 | 6 => String::new(), // DnsDomain / PhysicalDnsDomain
        _ => crate::dlls::kernel32::get_hostname_string(),
    };
    let w: Vec<u16> = name.encode_utf16().collect();
    if buf.is_null() || (*size as usize) <= w.len() {
        *size = w.len() as u32 + 1;
        crate::process::set_last_error(234); // ERROR_MORE_DATA
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    *size = w.len() as u32;
    1
}

pub unsafe extern "win64" fn HeapCompact(_heap: usize, _flags: u32) -> usize {
    1 << 20
}
pub unsafe extern "win64" fn HeapQueryInformation(
    _heap: usize,
    class: u32,
    info: *mut u32,
    len: usize,
    ret: *mut usize,
) -> i32 {
    if class != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if !ret.is_null() {
        *ret = 4;
    }
    if info.is_null() || len < 4 {
        crate::process::set_last_error(24);
        return 0;
    }
    *info = 2; // tas à faible fragmentation
    1
}

pub unsafe extern "win64" fn GetNamedPipeProcessId(_pipe: usize, pid: *mut u32) -> i32 {
    if pid.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *pid = libc::getpid() as u32;
    1
}

pub unsafe extern "win64" fn fail_not_supported() -> i32 {
    crate::process::set_last_error(50);
    0
}
pub unsafe extern "win64" fn fail_suspend() -> u32 {
    crate::process::set_last_error(50);
    u32::MAX
}
pub unsafe extern "win64" fn fail_no_more_files() -> usize {
    crate::process::set_last_error(18);
    usize::MAX
}

/// `ProcThreadAttributeList` : liste maison (en-tête « WLXA », capacité,
/// nombre, puis triplets attribut/valeur/taille). CreateProcess y lit la
/// pseudo-console (PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE) et les handles
/// hérités (PROC_THREAD_ATTRIBUTE_HANDLE_LIST) ; les autres attributs
/// (parent…) sont gardés et ignorés.
const ATTRIBUTE_MAGIC: u32 = 0x4158_4C57; // « WLXA »
const ATTRIBUTE_ENTRY: usize = 24;
pub const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;
pub const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x0002_0002;

fn attribute_list_size(count: u32) -> usize {
    16 + (count.max(1) as usize) * ATTRIBUTE_ENTRY
}

pub unsafe extern "win64" fn InitializeProcThreadAttributeList(
    list: *mut u8,
    count: u32,
    _flags: u32,
    size: *mut usize,
) -> i32 {
    if size.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let needed = attribute_list_size(count).max(64);
    if list.is_null() || *size < needed {
        *size = needed;
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::write_bytes(list, 0, needed);
    let capacity = ((needed - 16) / ATTRIBUTE_ENTRY) as u32;
    std::ptr::write_unaligned(list as *mut u32, ATTRIBUTE_MAGIC);
    std::ptr::write_unaligned(list.add(4) as *mut u32, capacity);
    1
}
pub unsafe extern "win64" fn UpdateProcThreadAttribute(
    list: usize,
    _flags: u32,
    attr: usize,
    value: usize,
    size: usize,
    _prev: usize,
    _ret: usize,
) -> i32 {
    let list = list as *mut u8;
    if list.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    if std::ptr::read_unaligned(list as *const u32) != ATTRIBUTE_MAGIC {
        crate::process::set_last_error(87);
        return 0;
    }
    let capacity = std::ptr::read_unaligned(list.add(4) as *const u32);
    let used = std::ptr::read_unaligned(list.add(8) as *const u32);
    if used >= capacity {
        crate::process::set_last_error(122);
        return 0;
    }
    // PSEUDOCONSOLE : la valeur est le HPCON lui-même ; ailleurs un pointeur.
    let entry = list.add(16 + used as usize * ATTRIBUTE_ENTRY);
    std::ptr::write_unaligned(entry as *mut usize, attr);
    std::ptr::write_unaligned(entry.add(8) as *mut usize, value);
    std::ptr::write_unaligned(entry.add(16) as *mut usize, size);
    std::ptr::write_unaligned(list.add(8) as *mut u32, used + 1);
    1
}

/// (valeur, taille) d'un attribut d'une liste remplie par UpdateProcThreadAttribute.
unsafe fn attribute_entry(list: usize, attr: usize) -> Option<(usize, usize)> {
    let list = list as *const u8;
    if list.is_null() || std::ptr::read_unaligned(list as *const u32) != ATTRIBUTE_MAGIC {
        return None;
    }
    let used = std::ptr::read_unaligned(list.add(8) as *const u32) as usize;
    (0..used).find_map(|index| {
        let entry = list.add(16 + index * ATTRIBUTE_ENTRY);
        (std::ptr::read_unaligned(entry as *const usize) == attr).then(|| {
            (
                std::ptr::read_unaligned(entry.add(8) as *const usize),
                std::ptr::read_unaligned(entry.add(16) as *const usize),
            )
        })
    })
}

/// Valeur d'un attribut d'une liste remplie par UpdateProcThreadAttribute.
pub(crate) unsafe fn proc_thread_attribute(list: usize, attr: usize) -> Option<usize> {
    attribute_entry(list, attr).map(|(value, _)| value)
}

/// Handles de `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`, s'il y en a.
pub(crate) unsafe fn attribute_handle_list(list: usize) -> Option<Vec<usize>> {
    let (ptr, size) = attribute_entry(list, PROC_THREAD_ATTRIBUTE_HANDLE_LIST)?;
    let n = size / 8;
    if ptr == 0 || n > 4096 {
        return None;
    }
    Some(
        (0..n)
            .map(|i| std::ptr::read_unaligned((ptr + i * 8) as *const usize))
            .collect(),
    )
}
pub unsafe extern "win64" fn DeleteProcThreadAttributeList(_list: usize) {}

pub unsafe extern "win64" fn FreeLibraryAndExitThread(module: usize, code: u32) {
    crate::dlls::kernel32::free_library(module);
    crate::dlls::kernel32::exit_current_thread(code);
}

/// `GenerateConsoleCtrlEvent(CTRL_C/CTRL_BREAK, groupe)`.
pub unsafe extern "win64" fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32 {
    let sig = if event == 0 {
        libc::SIGINT
    } else {
        libc::SIGQUIT
    };
    let target = if group == 0 { 0 } else { -(group as i32) };
    (libc::kill(target, sig) == 0) as i32
}

// ---------- Fonctions encore importées par Claude Code (Bun) ----------

/// `GetModuleHandleExA` : comme la variante W.
pub unsafe extern "win64" fn GetModuleHandleExA(
    flags: u32,
    module: *const u8,
    out: *mut usize,
) -> i32 {
    if flags & 4 != 0 || module.is_null() {
        return crate::dlls::kernel32::get_module_handle_ex_w(flags, module as *const u16, out);
    }
    let w: Vec<u16> = crate::dlls::cstr(module)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    crate::dlls::kernel32::get_module_handle_ex_w(flags, w.as_ptr(), out)
}

fn is_current_process(h: usize) -> bool {
    h == usize::MAX || crate::dlls::kernel32::is_current_process_handle(h)
}

/// `K32EnumProcessModules` (processus courant) : base de chaque module.
pub unsafe extern "win64" fn K32EnumProcessModules(
    process: usize,
    modules: *mut usize,
    cb: u32,
    needed: *mut u32,
) -> i32 {
    if !is_current_process(process) {
        crate::process::set_last_error(5);
        return 0;
    }
    let list = crate::loader::list_modules();
    let size = std::mem::size_of::<usize>();
    if !needed.is_null() {
        *needed = (list.len() * size) as u32;
    }
    if !modules.is_null() {
        for (i, m) in list.iter().take(cb as usize / size).enumerate() {
            *modules.add(i) = m.base;
        }
    }
    1
}

unsafe fn copy_wide(text: &str, buf: *mut u16, size: u32) -> u32 {
    if buf.is_null() || size == 0 {
        crate::process::set_last_error(122);
        return 0;
    }
    let w: Vec<u16> = text.encode_utf16().collect();
    let n = w.len().min(size as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

/// `K32GetModuleBaseNameW(processus, module, tampon, taille)`.
pub unsafe extern "win64" fn K32GetModuleBaseNameW(
    process: usize,
    module: usize,
    buf: *mut u16,
    size: u32,
) -> u32 {
    if !is_current_process(process) {
        crate::process::set_last_error(5);
        return 0;
    }
    let m = if module == 0 {
        crate::loader::main_module()
    } else {
        crate::loader::find_module_by_addr(module)
    };
    match m {
        Some(m) => copy_wide(&m.name, buf, size),
        None => {
            crate::process::set_last_error(126);
            0
        }
    }
}

/// `K32GetModuleFileNameExW(processus, module, tampon, taille)`.
pub unsafe extern "win64" fn K32GetModuleFileNameExW(
    process: usize,
    module: usize,
    buf: *mut u16,
    size: u32,
) -> u32 {
    if !is_current_process(process) {
        crate::process::set_last_error(5);
        return 0;
    }
    crate::dlls::kernel32::get_module_file_name_w(module, buf, size)
}

/// `GetNamedPipeHandleStateA` : tube en mode octets, bloquant, une instance.
pub unsafe extern "win64" fn GetNamedPipeHandleStateA(
    _pipe: usize,
    state: *mut u32,
    instances: *mut u32,
    max_collection: *mut u32,
    timeout: *mut u32,
    user: *mut u8,
    user_size: u32,
) -> i32 {
    for (p, v) in [
        (state, 0u32),
        (instances, 1),
        (max_collection, 0),
        (timeout, 0),
    ] {
        if !p.is_null() {
            *p = v;
        }
    }
    if !user.is_null() && user_size > 0 {
        *user = 0;
    }
    1
}

static CONSOLE_TITLE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// `GetConsoleTitleW(tampon, taille)`.
pub unsafe extern "win64" fn GetConsoleTitleW(buf: *mut u16, size: u32) -> u32 {
    let t = crate::dlls::lock(&CONSOLE_TITLE).clone();
    let t = if t.is_empty() {
        "Winlux".to_string()
    } else {
        t
    };
    copy_wide(&t, buf, size)
}

/// `FreeResource` : obsolète, ne fait rien et rend FALSE comme Windows.
pub extern "win64" fn FreeResource(_h: usize) -> i32 {
    0
}

/// Énumération des ressources : aucune ressource de ce type
/// (ERROR_RESOURCE_TYPE_NOT_FOUND), les appelants passent au repli.
pub extern "win64" fn EnumResourceNamesW(_m: usize, _t: usize, _f: usize, _p: isize) -> i32 {
    crate::process::set_last_error(1813);
    0
}

/// `RtlFillMemory(destination, longueur, octet)`.
pub unsafe extern "win64" fn RtlFillMemory(dest: *mut u8, len: usize, fill: u8) {
    if !dest.is_null() {
        std::ptr::write_bytes(dest, fill, len);
    }
}

/// `RtlExitUserProcess(code)`.
pub extern "win64" fn RtlExitUserProcess(code: u32) -> ! {
    crate::dlls::kernel32::flush_and_exit(code as i32)
}

/// `RtlCaptureStackBackTrace` : pas de pile remontée (0 trame), ce que
/// les appelants (diagnostics) acceptent.
pub unsafe extern "win64" fn RtlCaptureStackBackTrace(
    _skip: u32,
    _count: u32,
    _frames: *mut usize,
    hash: *mut u32,
) -> u16 {
    if !hash.is_null() {
        *hash = 0;
    }
    0
}

/// Objets du noyau sans équivalent : STATUS_NOT_SUPPORTED.
pub extern "win64" fn nt_not_supported() -> u32 {
    0xC000_00BB
}

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetCurrentThreadStackLimits" => fn GetCurrentThreadStackLimits,
        "IsThreadAFiber" => fn IsThreadAFiber,
        "DebugBreak" => fn DebugBreak,
        "WaitOnAddress" => fn WaitOnAddress,
        "WakeByAddressSingle" => fn WakeByAddressSingle,
        "WakeByAddressAll" => fn WakeByAddressAll,
        "VirtualUnlock" => fn VirtualUnlock,
        "DeviceIoControl" => fn DeviceIoControl,
        "ReOpenFile" => fn ReOpenFile,
        "SetFileCompletionNotificationModes" => fn SetFileCompletionNotificationModes,
        "CancelSynchronousIo" => fn CancelSynchronousIo,
        "LockFileEx" => fn LockFileEx,
        "UnlockFileEx" => fn UnlockFileEx,
        "SetFileInformationByHandle" => fn SetFileInformationByHandle,
        "CreateDirectoryExW" => fn CreateDirectoryExW,
        "GetDiskFreeSpaceW" => fn GetDiskFreeSpaceW,
        "GetDiskFreeSpaceA" => fn GetDiskFreeSpaceA,
        "RegisterWaitForSingleObject" => fn RegisterWaitForSingleObject,
        "UnregisterWait" => fn UnregisterWait,
        "UnregisterWaitEx" => fn UnregisterWaitEx,
        "QueueUserWorkItem" => fn QueueUserWorkItem,
        "CreateJobObjectW" => fn CreateJobObjectW,
        "CreateJobObjectA" => fn CreateJobObjectW,
        "AssignProcessToJobObject" => fn job_ok2,
        "SetInformationJobObject" => fn SetInformationJobObject,
        "ProcessIdToSessionId" => fn ProcessIdToSessionId,
        "K32GetProcessMemoryInfo" => fn K32GetProcessMemoryInfo,
        "GetProcessMemoryInfo" => fn K32GetProcessMemoryInfo,
        "GetProcessIoCounters" => fn GetProcessIoCounters,
        "CreateToolhelp32Snapshot" => fn CreateToolhelp32Snapshot,
        "Process32FirstW" => fn Process32FirstW,
        "Process32NextW" => fn Process32NextW,
        "GetThreadDescription" => fn GetThreadDescription,
        "NeedCurrentDirectoryForExePathW" => fn NeedCurrentDirectoryForExePathW,
        "GetComputerNameExW" => fn GetComputerNameExW,
        "HeapCompact" => fn HeapCompact,
        "HeapQueryInformation" => fn HeapQueryInformation,
        "GetNamedPipeClientProcessId" => fn GetNamedPipeProcessId,
        "GetNamedPipeServerProcessId" => fn GetNamedPipeProcessId,
        "InitializeProcThreadAttributeList" => fn InitializeProcThreadAttributeList,
        "UpdateProcThreadAttribute" => fn UpdateProcThreadAttribute,
        "DeleteProcThreadAttributeList" => fn DeleteProcThreadAttributeList,
        "FreeLibraryAndExitThread" => fn FreeLibraryAndExitThread,
        "GenerateConsoleCtrlEvent" => fn GenerateConsoleCtrlEvent,
        "ReadFileEx" => fn fail_not_supported,
        "WriteFileEx" => fn fail_not_supported,
        "OpenFileById" => fn fail_no_more_files,
        "FindFirstVolumeW" => fn fail_no_more_files,
        "FindNextVolumeW" => fn fail_not_supported,
        "FindVolumeClose" => fn job_ok2,
        "QueryDosDeviceW" => fn fail_not_supported,
        "WaitNamedPipeW" => fn crate::dlls::named_pipes::wait_named_pipe_w,
        "Thread32First" => fn fail_not_supported,
        "Thread32Next" => fn fail_not_supported,
        "OpenThread" => fn fail_not_supported,
        "SuspendThread" => fn fail_suspend,
        "GetThreadContext" => fn fail_not_supported,
        "BeginUpdateResourceW" => fn fail_not_supported,
        "UpdateResourceW" => fn fail_not_supported,
        "EndUpdateResourceW" => fn fail_not_supported,
        "GetModuleHandleExA" => fn GetModuleHandleExA,
        "K32EnumProcessModules" => fn K32EnumProcessModules,
        "EnumProcessModules" => fn K32EnumProcessModules,
        "K32GetModuleBaseNameW" => fn K32GetModuleBaseNameW,
        "K32GetModuleFileNameExW" => fn K32GetModuleFileNameExW,
        "GetNamedPipeHandleStateA" => fn GetNamedPipeHandleStateA,
        "GetNamedPipeHandleStateW" => fn GetNamedPipeHandleStateA,
        "GetConsoleTitleW" => fn GetConsoleTitleW,
        "FreeResource" => fn FreeResource,
        "EnumResourceNamesW" => fn EnumResourceNamesW,
        "EnumResourceLanguagesW" => fn EnumResourceNamesW,
        "RtlCaptureStackBackTrace" => fn RtlCaptureStackBackTrace,
    }
}

pub fn ntdll_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RtlWaitOnAddress" => fn RtlWaitOnAddress,
        "RtlWakeAddressSingle" => fn WakeByAddressSingle,
        "RtlWakeAddressAll" => fn WakeByAddressAll,
        "DbgBreakPoint" => fn DebugBreak,
        "RtlFillMemory" => fn RtlFillMemory,
        "RtlExitUserProcess" => fn RtlExitUserProcess,
        "RtlCaptureStackBackTrace" => fn RtlCaptureStackBackTrace,
        "NtQuerySecurityObject" => fn nt_not_supported,
        "NtSetSecurityObject" => fn nt_not_supported,
        "NtOpenDirectoryObject" => fn nt_not_supported,
        "NtCreateNamedPipeFile" => fn nt_not_supported,
    }
}
