//! Fonctions que Chromium et Electron vérifient au démarrage (#236).
//!
//! Chromium résout beaucoup de fonctions dynamiquement et arrête le
//! processus (CHECK, `int3`) si GetProcAddress rend NULL ou si l'appel
//! échoue. Relevé sur un vrai Electron 45 lancé sous Winlux :
//! - kernel32 : politiques d'atténuation (`Get/SetProcessMitigationPolicy`),
//!   `Get/SetProcessInformation`, `SetThreadInformation`, `FlsGetValue2`,
//!   `GetFileInformationByName` ;
//! - advapi32 : contrôle et SACL des descripteurs de sécurité,
//!   `MakeSelfRelativeSD`, `MakeAbsoluteSD` ;
//! - user32 : notifications de veille, contextes de DPI, `SetWinEventHook` ;
//! - shcore (`api-ms-win-shcore-scaling-*`) : DPI par écran.
//!
//! Winlux n'applique pas les atténuations de Windows (le noyau Linux a les
//! siennes) : elles sont lues comme désactivées et leur demande est
//! acceptée sans effet, ce que Chromium accepte aussi sous les anciens
//! Windows.

#![allow(non_snake_case)]

use std::sync::atomic::{AtomicIsize, Ordering};

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_DATA: u32 = 13;
const ERROR_INVALID_SECURITY_DESCR: u32 = 1338;
const ERROR_BAD_DESCRIPTOR_FORMAT: u32 = 1340;
const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}

// ============================================================
//  kernel32
// ============================================================

/// `GetProcessMitigationPolicy` : toutes les politiques à zéro (aucune
/// atténuation Windows en vigueur). La taille dépend de la politique ; on
/// remplit ce que l'appelant a prévu.
pub unsafe extern "win64" fn GetProcessMitigationPolicy(
    _process: usize,
    policy: u32,
    buffer: *mut u8,
    length: usize,
) -> i32 {
    if buffer.is_null() || length == 0 || policy > 64 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    std::ptr::write_bytes(buffer, 0, length);
    1
}

pub unsafe extern "win64" fn SetProcessMitigationPolicy(
    policy: u32,
    buffer: *const u8,
    length: usize,
) -> i32 {
    if buffer.is_null() || length == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    crate::trace!("SetProcessMitigationPolicy({policy}) accepté sans effet");
    1
}

/// `GetProcessInformation` : rien de particulier à signaler (zéros).
pub unsafe extern "win64" fn GetProcessInformation(
    _process: usize,
    _class: u32,
    info: *mut u8,
    size: u32,
) -> i32 {
    if info.is_null() || size == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    std::ptr::write_bytes(info, 0, size as usize);
    1
}

/// `SetProcessInformation` / `SetThreadInformation` : priorité mémoire,
/// économie d'énergie… acceptées sans effet.
pub unsafe extern "win64" fn SetProcessInformation(
    _process: usize,
    class: u32,
    info: *const u8,
    _size: u32,
) -> i32 {
    if info.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    crate::trace!("SetProcessInformation({class}) accepté sans effet");
    1
}

pub unsafe extern "win64" fn SetThreadInformation(
    _thread: usize,
    class: u32,
    info: *const u8,
    _size: u32,
) -> i32 {
    if info.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    crate::trace!("SetThreadInformation({class}) accepté sans effet");
    1
}

/// `FlsGetValue2` (Windows 11) : comme FlsGetValue, sans toucher à
/// GetLastError.
pub extern "win64" fn FlsGetValue2(slot: u32) -> usize {
    crate::process::tls_value(slot).unwrap_or(0)
}

/// `GetFileInformationByName` (Windows 11 récent) : non pris en charge ;
/// les appelants retombent sur CreateFile + GetFileInformationByHandleEx.
pub unsafe extern "win64" fn GetFileInformationByName(
    _name: *const u16,
    _class: u32,
    _buffer: *mut u8,
    _size: u32,
) -> i32 {
    fail(ERROR_NOT_SUPPORTED)
}

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetProcessMitigationPolicy" => fn GetProcessMitigationPolicy,
        "SetProcessMitigationPolicy" => fn SetProcessMitigationPolicy,
        "GetProcessInformation" => fn GetProcessInformation,
        "SetProcessInformation" => fn SetProcessInformation,
        "SetThreadInformation" => fn SetThreadInformation,
        "FlsGetValue2" => fn FlsGetValue2,
        "GetFileInformationByName" => fn GetFileInformationByName,
    }
}

// ============================================================
//  advapi32 : descripteurs de sécurité
// ============================================================
//
// Absolu : Revision, Sbz1 (contrôle du gestionnaire de ressources), Control,
// puis 4 pointeurs Owner (8), Group (16), Sacl (24), Dacl (32).
// Auto-relatif (SE_SELF_RELATIVE) : 4 décalages u32 aux octets 4, 8, 12, 16.

const SE_SELF_RELATIVE: u16 = 0x8000;
const SE_SACL_PRESENT: u16 = 0x10;
const SE_SACL_DEFAULTED: u16 = 0x20;
const SE_RM_CONTROL_VALID: u16 = 0x4000;
/// Bits que SetSecurityDescriptorControl peut changer.
const SETTABLE: u16 = 0x0100 | 0x0200 | 0x0400 | 0x0800 | 0x1000 | 0x2000;
const ABSOLUTE_SIZE: usize = 40;
const RELATIVE_SIZE: usize = 20;

unsafe fn control(sd: *const u8) -> u16 {
    std::ptr::read_unaligned(sd.add(2) as *const u16)
}

/// Partie `index` (0 propriétaire, 1 groupe, 2 SACL, 3 DACL) d'un
/// descripteur absolu ou auto-relatif.
unsafe fn part(sd: *const u8, index: usize) -> *mut u8 {
    if control(sd) & SE_SELF_RELATIVE == 0 {
        std::ptr::read_unaligned(sd.add(8 + index * 8) as *const *mut u8)
    } else {
        let off = std::ptr::read_unaligned(sd.add(4 + index * 4) as *const u32) as usize;
        if off == 0 {
            std::ptr::null_mut()
        } else {
            sd.add(off) as *mut u8
        }
    }
}

/// Taille d'un SID (0 si absent ou invalide).
unsafe fn sid_len(p: *const u8) -> usize {
    if p.is_null() || *p != 1 || *p.add(1) > 15 {
        return 0;
    }
    8 + 4 * *p.add(1) as usize
}

/// Taille d'une ACL (AclSize, octets 2-3).
unsafe fn acl_len(p: *const u8) -> usize {
    if p.is_null() {
        0
    } else {
        std::ptr::read_unaligned(p.add(2) as *const u16) as usize
    }
}

unsafe fn part_len(sd: *const u8, index: usize) -> usize {
    let p = part(sd, index);
    if index < 2 {
        sid_len(p)
    } else {
        acl_len(p)
    }
}

unsafe fn valid(sd: *const u8) -> bool {
    !sd.is_null() && *sd == 1
}

pub unsafe extern "win64" fn GetSecurityDescriptorControl(
    sd: *const u8,
    out: *mut u16,
    revision: *mut u32,
) -> i32 {
    if !valid(sd) || out.is_null() || revision.is_null() {
        return fail(ERROR_INVALID_SECURITY_DESCR);
    }
    *out = control(sd);
    *revision = *sd as u32;
    1
}

pub unsafe extern "win64" fn SetSecurityDescriptorControl(
    sd: *mut u8,
    mask: u16,
    bits: u16,
) -> i32 {
    if !valid(sd) {
        return fail(ERROR_INVALID_SECURITY_DESCR);
    }
    if mask & !SETTABLE != 0 || bits & !mask != 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let c = (control(sd) & !mask) | bits;
    std::ptr::write_unaligned(sd.add(2) as *mut u16, c);
    1
}

pub unsafe extern "win64" fn GetSecurityDescriptorSacl(
    sd: *const u8,
    present: *mut i32,
    sacl: *mut *mut u8,
    defaulted: *mut i32,
) -> i32 {
    if !valid(sd) || present.is_null() || sacl.is_null() || defaulted.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let c = control(sd);
    *present = i32::from(c & SE_SACL_PRESENT != 0);
    *sacl = if c & SE_SACL_PRESENT != 0 {
        part(sd, 2)
    } else {
        std::ptr::null_mut()
    };
    *defaulted = i32::from(c & SE_SACL_DEFAULTED != 0);
    1
}

pub unsafe extern "win64" fn SetSecurityDescriptorSacl(
    sd: *mut u8,
    present: i32,
    sacl: *mut u8,
    defaulted: i32,
) -> i32 {
    if !valid(sd) {
        return fail(ERROR_INVALID_SECURITY_DESCR);
    }
    if control(sd) & SE_SELF_RELATIVE != 0 {
        return fail(ERROR_BAD_DESCRIPTOR_FORMAT);
    }
    let mut c = control(sd) & !(SE_SACL_PRESENT | SE_SACL_DEFAULTED);
    if present != 0 {
        c |= SE_SACL_PRESENT;
        if defaulted != 0 {
            c |= SE_SACL_DEFAULTED;
        }
    }
    std::ptr::write_unaligned(sd.add(2) as *mut u16, c);
    std::ptr::write_unaligned(
        sd.add(24) as *mut *mut u8,
        if present != 0 {
            sacl
        } else {
            std::ptr::null_mut()
        },
    );
    1
}

pub unsafe extern "win64" fn GetSecurityDescriptorRMControl(sd: *const u8, rm: *mut u8) -> u32 {
    if !valid(sd) || rm.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    if control(sd) & SE_RM_CONTROL_VALID == 0 {
        return ERROR_INVALID_DATA;
    }
    *rm = *sd.add(1);
    0
}

pub unsafe extern "win64" fn SetSecurityDescriptorRMControl(sd: *mut u8, rm: *const u8) -> u32 {
    if !valid(sd) {
        return ERROR_INVALID_PARAMETER;
    }
    let mut c = control(sd);
    if rm.is_null() {
        c &= !SE_RM_CONTROL_VALID;
        *sd.add(1) = 0;
    } else {
        c |= SE_RM_CONTROL_VALID;
        *sd.add(1) = *rm;
    }
    std::ptr::write_unaligned(sd.add(2) as *mut u16, c);
    0
}

/// `MakeSelfRelativeSD` : copie en un seul bloc (propriétaire, groupe,
/// SACL, DACL à la suite de l'en-tête de 20 octets).
pub unsafe extern "win64" fn MakeSelfRelativeSD(
    abs: *const u8,
    rel: *mut u8,
    len: *mut u32,
) -> i32 {
    if !valid(abs) || len.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if control(abs) & SE_SELF_RELATIVE != 0 {
        return fail(ERROR_BAD_DESCRIPTOR_FORMAT);
    }
    let sizes: Vec<usize> = (0..4).map(|i| part_len(abs, i)).collect();
    let total = RELATIVE_SIZE + sizes.iter().sum::<usize>();
    if rel.is_null() || (*len as usize) < total {
        *len = total as u32;
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::write_bytes(rel, 0, RELATIVE_SIZE);
    *rel = 1;
    *rel.add(1) = *abs.add(1);
    std::ptr::write_unaligned(rel.add(2) as *mut u16, control(abs) | SE_SELF_RELATIVE);
    let mut at = RELATIVE_SIZE;
    for (i, &n) in sizes.iter().enumerate() {
        if n == 0 {
            continue;
        }
        std::ptr::copy_nonoverlapping(part(abs, i), rel.add(at), n);
        std::ptr::write_unaligned(rel.add(4 + i * 4) as *mut u32, at as u32);
        at += n;
    }
    *len = total as u32;
    1
}

/// `MakeAbsoluteSD` : chaque partie dans le tampon fourni par l'appelant ;
/// si l'un d'eux est trop petit, toutes les tailles voulues sont rendues.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn MakeAbsoluteSD(
    rel: *const u8,
    abs: *mut u8,
    abs_size: *mut u32,
    dacl: *mut u8,
    dacl_size: *mut u32,
    sacl: *mut u8,
    sacl_size: *mut u32,
    owner: *mut u8,
    owner_size: *mut u32,
    group: *mut u8,
    group_size: *mut u32,
) -> i32 {
    let sizes = [abs_size, dacl_size, sacl_size, owner_size, group_size];
    if !valid(rel) || sizes.iter().any(|s| s.is_null()) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if control(rel) & SE_SELF_RELATIVE == 0 {
        return fail(ERROR_BAD_DESCRIPTOR_FORMAT);
    }
    // (index de la partie, tampon, taille)
    let parts = [
        (3usize, dacl, dacl_size),
        (2, sacl, sacl_size),
        (0, owner, owner_size),
        (1, group, group_size),
    ];
    let mut short = (*abs_size as usize) < ABSOLUTE_SIZE || abs.is_null();
    for &(i, buf, size) in &parts {
        let need = part_len(rel, i);
        if (*size as usize) < need || (need > 0 && buf.is_null()) {
            short = true;
        }
    }
    if short {
        *abs_size = ABSOLUTE_SIZE as u32;
        for &(i, _, size) in &parts {
            *size = part_len(rel, i) as u32;
        }
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::write_bytes(abs, 0, ABSOLUTE_SIZE);
    *abs = 1;
    *abs.add(1) = *rel.add(1);
    std::ptr::write_unaligned(abs.add(2) as *mut u16, control(rel) & !SE_SELF_RELATIVE);
    for &(i, buf, size) in &parts {
        let need = part_len(rel, i);
        *size = need as u32;
        let p = if need == 0 {
            std::ptr::null_mut()
        } else {
            std::ptr::copy_nonoverlapping(part(rel, i), buf, need);
            buf
        };
        std::ptr::write_unaligned(abs.add(8 + i * 8) as *mut *mut u8, p);
    }
    *abs_size = ABSOLUTE_SIZE as u32;
    1
}

pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetSecurityDescriptorControl" => fn GetSecurityDescriptorControl,
        "SetSecurityDescriptorControl" => fn SetSecurityDescriptorControl,
        "GetSecurityDescriptorSacl" => fn GetSecurityDescriptorSacl,
        "SetSecurityDescriptorSacl" => fn SetSecurityDescriptorSacl,
        "GetSecurityDescriptorRMControl" => fn GetSecurityDescriptorRMControl,
        "SetSecurityDescriptorRMControl" => fn SetSecurityDescriptorRMControl,
        "MakeSelfRelativeSD" => fn MakeSelfRelativeSD,
        "MakeAbsoluteSD" => fn MakeAbsoluteSD,
    }
}

// ============================================================
//  user32 : veille, contextes de DPI, crochets d'accessibilité
// ============================================================

/// `RegisterSuspendResumeNotification` : Winlux n'envoie pas encore
/// WM_POWERBROADCAST ; l'abonnement est accepté.
pub extern "win64" fn RegisterSuspendResumeNotification(recipient: usize, _flags: u32) -> usize {
    if recipient == 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    0x5150_0000 | (recipient & 0xFFFF)
}

pub extern "win64" fn UnregisterSuspendResumeNotification(handle: usize) -> i32 {
    i32::from(handle != 0)
}

// DPI_AWARENESS_CONTEXT : pseudo-handles -1 à -5.
const UNAWARE: isize = -1;
const SYSTEM_AWARE: isize = -2;
const PER_MONITOR: isize = -3;
const PER_MONITOR_V2: isize = -4;
const UNAWARE_GDISCALED: isize = -5;

static PROCESS_CONTEXT: AtomicIsize = AtomicIsize::new(UNAWARE);
thread_local! {
    static THREAD_CONTEXT: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

fn context_valid(c: isize) -> bool {
    (UNAWARE_GDISCALED..=UNAWARE).contains(&c)
}

/// DPI_AWARENESS : 0 non conscient, 1 système, 2 par écran, -1 invalide.
fn awareness(c: isize) -> i32 {
    match c {
        UNAWARE | UNAWARE_GDISCALED => 0,
        SYSTEM_AWARE => 1,
        PER_MONITOR | PER_MONITOR_V2 => 2,
        _ => -1,
    }
}

fn current_context() -> isize {
    match THREAD_CONTEXT.with(|t| t.get()) {
        0 => PROCESS_CONTEXT.load(Ordering::Relaxed),
        c => c,
    }
}

fn system_dpi() -> u32 {
    unsafe { crate::dlls::win32_basics::get_dpi_for_system() }
}

pub extern "win64" fn SetProcessDpiAwarenessContext(ctx: isize) -> i32 {
    if !context_valid(ctx) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    PROCESS_CONTEXT.store(ctx, Ordering::Relaxed);
    1
}

pub extern "win64" fn GetThreadDpiAwarenessContext() -> isize {
    current_context()
}

pub extern "win64" fn SetThreadDpiAwarenessContext(ctx: isize) -> isize {
    if !context_valid(ctx) {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let old = current_context();
    THREAD_CONTEXT.with(|t| t.set(ctx));
    old
}

pub extern "win64" fn GetWindowDpiAwarenessContext(_hwnd: usize) -> isize {
    PROCESS_CONTEXT.load(Ordering::Relaxed)
}

/// IsProcessDPIAware : vrai dès que le processus n'est plus « non conscient »
/// (SetProcessDPIAware, SetProcessDpiAwareness(Context)). WPF le lit (#268).
pub extern "win64" fn IsProcessDPIAware() -> i32 {
    i32::from(awareness(PROCESS_CONTEXT.load(Ordering::Relaxed)) > 0)
}

// DPI_HOSTING_BEHAVIOR : -1 invalide, 0 par défaut, 1 mixte. WinForms .NET 8
// le lit au démarrage par GetProcAddress (#666).
const DPI_HOSTING_DEFAULT: i32 = 0;
const DPI_HOSTING_MIXED: i32 = 1;
thread_local! {
    static THREAD_HOSTING: std::cell::Cell<i32> = const { std::cell::Cell::new(DPI_HOSTING_DEFAULT) };
}

pub extern "win64" fn GetThreadDpiHostingBehavior() -> i32 {
    THREAD_HOSTING.with(|t| t.get())
}

/// Rend l'ancien comportement, ou -1 (invalide) si `value` est inconnu.
pub extern "win64" fn SetThreadDpiHostingBehavior(value: i32) -> i32 {
    if !(DPI_HOSTING_DEFAULT..=DPI_HOSTING_MIXED).contains(&value) {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    THREAD_HOSTING.with(|t| t.replace(value))
}

/// Les fenêtres Winlux ne mémorisent pas le comportement de leur thread de
/// création : on rend celui du thread appelant, presque toujours le même.
pub extern "win64" fn GetWindowDpiHostingBehavior(_hwnd: usize) -> i32 {
    GetThreadDpiHostingBehavior()
}

pub extern "win64" fn GetAwarenessFromDpiAwarenessContext(ctx: isize) -> i32 {
    awareness(ctx)
}

pub extern "win64" fn AreDpiAwarenessContextsEqual(a: isize, b: isize) -> i32 {
    i32::from(context_valid(a) && a == b)
}

pub extern "win64" fn IsValidDpiAwarenessContext(ctx: isize) -> i32 {
    i32::from(context_valid(ctx))
}

pub extern "win64" fn GetDpiFromDpiAwarenessContext(ctx: isize) -> u32 {
    match awareness(ctx) {
        1 | 2 => system_dpi(),
        0 => 96,
        _ => 0,
    }
}

pub extern "win64" fn GetDpiAwarenessContextForProcess(_process: usize) -> isize {
    PROCESS_CONTEXT.load(Ordering::Relaxed)
}

pub extern "win64" fn GetSystemDpiForProcess(_process: usize) -> u32 {
    system_dpi()
}

/// Crochet `SetWinEventHook` hors contexte (WINEVENT_OUTOFCONTEXT) : son
/// rappel est appelé par le thread qui l'a posé, quand il relève ses
/// messages (GetMessage, PeekMessage, MsgWaitForMultipleObjects), comme sous
/// Windows.
struct WinEventHook {
    handle: usize,
    min: u32,
    max: u32,
    proc_: usize,
    thread: std::thread::ThreadId,
}

/// Événement en attente de livraison à un crochet.
struct WinEventCall {
    thread: std::thread::ThreadId,
    handle: usize,
    proc_: usize,
    event: u32,
    hwnd: usize,
    time: u32,
}

static WIN_EVENT_HOOKS: std::sync::Mutex<Vec<WinEventHook>> = std::sync::Mutex::new(Vec::new());
static WIN_EVENT_QUEUE: std::sync::Mutex<Vec<WinEventCall>> = std::sync::Mutex::new(Vec::new());
static NEXT_WIN_EVENT_HOOK: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0x5745_0001);

fn lock_hooks() -> std::sync::MutexGuard<'static, Vec<WinEventHook>> {
    WIN_EVENT_HOOKS.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_win_events() -> std::sync::MutexGuard<'static, Vec<WinEventCall>> {
    WIN_EVENT_QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

pub const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
pub const EVENT_SYSTEM_MINIMIZESTART: u32 = 0x0016;
pub const EVENT_SYSTEM_MINIMIZEEND: u32 = 0x0017;
pub const EVENT_OBJECT_SHOW: u32 = 0x8002;
pub const EVENT_OBJECT_HIDE: u32 = 0x8003;
pub const EVENT_OBJECT_LOCATIONCHANGE: u32 = 0x800B;

/// `SetWinEventHook`. Le suivi d'occultation de Chromium en pose sept
/// (réduction, premier plan, déplacement…) et ne recalcule ce qui est visible
/// qu'à leur réception : sans événement, une fenêtre restaurée ou agrandie
/// restait vide ou figée (#1476, #1498).
pub extern "win64" fn SetWinEventHook(
    min: u32,
    max: u32,
    _module: usize,
    proc_: usize,
    _pid: u32,
    _tid: u32,
    _flags: u32,
) -> usize {
    if proc_ == 0 || min > max {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let handle = NEXT_WIN_EVENT_HOOK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    lock_hooks().push(WinEventHook {
        handle,
        min,
        max,
        proc_,
        thread: std::thread::current().id(),
    });
    handle
}

pub extern "win64" fn UnhookWinEvent(hook: usize) -> i32 {
    let mut hooks = lock_hooks();
    let before = hooks.len();
    hooks.retain(|h| h.handle != hook);
    lock_win_events().retain(|c| c.handle != hook);
    i32::from(hooks.len() != before)
}

/// Émet un événement sur une fenêtre (OBJID_WINDOW, CHILDID_SELF) : il est
/// mis en attente pour chaque crochet concerné et son thread est réveillé.
/// Programmes 64 bits seulement (le rappel d'un programme 32 bits serait à
/// traduire).
pub fn notify_win_event(event: u32, hwnd: usize) {
    if crate::wow32::pointer_size() != 8 {
        return;
    }
    let time = crate::dlls::kernel32::get_tick_count();
    let targets: Vec<(std::thread::ThreadId, usize, usize)> = lock_hooks()
        .iter()
        .filter(|h| (h.min..=h.max).contains(&event))
        .map(|h| (h.thread, h.handle, h.proc_))
        .collect();
    if crate::dlls::user32::window_diag() {
        eprintln!(
            "winlux[fenetre] événement {event:#06x} sur {hwnd:#x} : {} crochet(s)",
            targets.len()
        );
    }
    if targets.is_empty() {
        return;
    }
    let mut queue = lock_win_events();
    for (thread, handle, proc_) in targets {
        // Un même événement encore en attente sur la même fenêtre n'est pas
        // répété (LOCATIONCHANGE arrive à chaque pas d'un glisser).
        if queue
            .iter()
            .any(|c| c.handle == handle && c.event == event && c.hwnd == hwnd)
        {
            continue;
        }
        queue.push(WinEventCall {
            thread,
            handle,
            proc_,
            event,
            hwnd,
            time,
        });
        crate::dlls::gui::wake_thread(thread);
    }
}

#[cfg(test)]
pub fn deliver_win_events_for_test() {
    unsafe { deliver_win_events() }
}

/// Appelle les rappels en attente pour le thread courant.
pub unsafe fn deliver_win_events() {
    let me = std::thread::current().id();
    let calls: Vec<WinEventCall> = {
        let mut queue = lock_win_events();
        if !queue.iter().any(|c| c.thread == me) {
            return;
        }
        let (mine, rest): (Vec<_>, Vec<_>) = queue.drain(..).partition(|c| c.thread == me);
        *queue = rest;
        mine
    };
    type WinEventProc = unsafe extern "win64" fn(usize, u32, usize, i32, i32, u32, u32);
    let tid = crate::dlls::kernel32::get_current_thread_id();
    for c in calls {
        // Décroché entre-temps : plus de rappel.
        if !lock_hooks().iter().any(|h| h.handle == c.handle) {
            continue;
        }
        let f: WinEventProc = std::mem::transmute(c.proc_);
        f(c.handle, c.event, c.hwnd, 0, 0, tid, c.time);
    }
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RegisterSuspendResumeNotification" => fn RegisterSuspendResumeNotification,
        "UnregisterSuspendResumeNotification" => fn UnregisterSuspendResumeNotification,
        "SetProcessDpiAwarenessContext" => fn SetProcessDpiAwarenessContext,
        "GetThreadDpiAwarenessContext" => fn GetThreadDpiAwarenessContext,
        "SetThreadDpiAwarenessContext" => fn SetThreadDpiAwarenessContext,
        "GetWindowDpiAwarenessContext" => fn GetWindowDpiAwarenessContext,
        "GetThreadDpiHostingBehavior" => fn GetThreadDpiHostingBehavior,
        "IsProcessDPIAware" => fn IsProcessDPIAware,
        "SetThreadDpiHostingBehavior" => fn SetThreadDpiHostingBehavior,
        "GetWindowDpiHostingBehavior" => fn GetWindowDpiHostingBehavior,
        "GetAwarenessFromDpiAwarenessContext" => fn GetAwarenessFromDpiAwarenessContext,
        "AreDpiAwarenessContextsEqual" => fn AreDpiAwarenessContextsEqual,
        "IsValidDpiAwarenessContext" => fn IsValidDpiAwarenessContext,
        "GetDpiFromDpiAwarenessContext" => fn GetDpiFromDpiAwarenessContext,
        "GetDpiAwarenessContextForProcess" => fn GetDpiAwarenessContextForProcess,
        "GetSystemDpiForProcess" => fn GetSystemDpiForProcess,
        "SetWinEventHook" => fn SetWinEventHook,
        "UnhookWinEvent" => fn UnhookWinEvent,
    }
}

// ============================================================
//  shcore
// ============================================================

/// PROCESS_DPI_AWARENESS (0 non conscient, 1 système, 2 par écran).
pub extern "win64" fn SetProcessDpiAwareness(value: u32) -> i32 {
    let ctx = match value {
        0 => UNAWARE,
        1 => SYSTEM_AWARE,
        2 => PER_MONITOR,
        _ => return E_INVALIDARG,
    };
    PROCESS_CONTEXT.store(ctx, Ordering::Relaxed);
    S_OK
}

pub unsafe extern "win64" fn GetProcessDpiAwareness(_process: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = awareness(PROCESS_CONTEXT.load(Ordering::Relaxed)).max(0) as u32;
    S_OK
}

/// `GetDpiForMonitor` : le DPI de l'écran (le même pour tous les écrans).
pub unsafe extern "win64" fn GetDpiForMonitor(
    _monitor: usize,
    kind: u32,
    x: *mut u32,
    y: *mut u32,
) -> i32 {
    if x.is_null() || y.is_null() || kind > 2 {
        return E_INVALIDARG;
    }
    let dpi = system_dpi();
    *x = dpi;
    *y = dpi;
    S_OK
}

/// `GetScaleFactorForMonitor` : DEVICE_SCALE_FACTOR en pourcentage.
pub unsafe extern "win64" fn GetScaleFactorForMonitor(_monitor: usize, scale: *mut u32) -> i32 {
    if scale.is_null() {
        return E_INVALIDARG;
    }
    *scale = scale_factor(system_dpi());
    S_OK
}

/// Facteur d'échelle de Windows le plus proche (100, 120, 125, 140, 150…).
fn scale_factor(dpi: u32) -> u32 {
    const STEPS: [u32; 14] = [
        100, 120, 125, 140, 150, 160, 175, 180, 200, 225, 250, 300, 350, 400,
    ];
    let pct = dpi * 100 / 96;
    *STEPS
        .iter()
        .min_by_key(|&&s| s.abs_diff(pct))
        .unwrap_or(&100)
}

pub fn shcore_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetProcessDpiAwareness" => fn SetProcessDpiAwareness,
        "GetProcessDpiAwareness" => fn GetProcessDpiAwareness,
        "GetDpiForMonitor" => fn GetDpiForMonitor,
        "GetScaleFactorForMonitor" => fn GetScaleFactorForMonitor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextes_de_dpi() {
        assert_eq!(awareness(PER_MONITOR_V2), 2);
        assert_eq!(awareness(0), -1);
        assert_eq!(IsValidDpiAwarenessContext(-6), 0);
        assert_eq!(AreDpiAwarenessContextsEqual(-4, -4), 1);
        let old = SetThreadDpiAwarenessContext(PER_MONITOR_V2);
        assert!(context_valid(old));
        assert_eq!(GetThreadDpiAwarenessContext(), PER_MONITOR_V2);
        assert_eq!(GetDpiFromDpiAwarenessContext(UNAWARE), 96);
        assert_eq!(scale_factor(96), 100);
        assert_eq!(scale_factor(144), 150);
        assert_eq!(scale_factor(120), 125);
    }

    #[test]
    fn comportement_d_hebergement_dpi() {
        assert_eq!(GetThreadDpiHostingBehavior(), DPI_HOSTING_DEFAULT);
        assert_eq!(
            SetThreadDpiHostingBehavior(DPI_HOSTING_MIXED),
            DPI_HOSTING_DEFAULT
        );
        assert_eq!(GetThreadDpiHostingBehavior(), DPI_HOSTING_MIXED);
        assert_eq!(GetWindowDpiHostingBehavior(0), DPI_HOSTING_MIXED);
        assert_eq!(SetThreadDpiHostingBehavior(7), -1);
        assert_eq!(
            SetThreadDpiHostingBehavior(DPI_HOSTING_DEFAULT),
            DPI_HOSTING_MIXED
        );
    }

    #[test]
    fn descripteur_auto_relatif_et_absolu() {
        unsafe {
            // SID S-1-5-18 (2 + 6 + 4 octets) et ACL vide de 8 octets.
            let sid: [u8; 12] = [1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0];
            let acl: [u8; 8] = [2, 0, 8, 0, 0, 0, 0, 0];
            let mut abs = [0u8; 40];
            abs[0] = 1;
            std::ptr::write_unaligned(abs.as_mut_ptr().add(8) as *mut *const u8, sid.as_ptr());
            std::ptr::write_unaligned(abs.as_mut_ptr().add(32) as *mut *const u8, acl.as_ptr());
            abs[2] = 0x4; // SE_DACL_PRESENT
            assert_eq!(
                SetSecurityDescriptorSacl(abs.as_mut_ptr(), 1, acl.as_ptr() as *mut u8, 0),
                1
            );
            let (mut c, mut r) = (0u16, 0u32);
            assert_eq!(
                GetSecurityDescriptorControl(abs.as_ptr(), &mut c, &mut r),
                1
            );
            assert_eq!(c, 0x14);
            let mut len = 0u32;
            assert_eq!(
                MakeSelfRelativeSD(abs.as_ptr(), std::ptr::null_mut(), &mut len),
                0
            );
            assert_eq!(len, 20 + 12 + 8 + 8);
            let mut rel = vec![0u8; len as usize];
            assert_eq!(
                MakeSelfRelativeSD(abs.as_ptr(), rel.as_mut_ptr(), &mut len),
                1
            );
            let (mut present, mut defaulted, mut s) = (0, 0, std::ptr::null_mut());
            assert_eq!(
                GetSecurityDescriptorSacl(rel.as_ptr(), &mut present, &mut s, &mut defaulted),
                1
            );
            assert_eq!(present, 1);
            assert_eq!(acl_len(s), 8);
            // Retour en absolu : tailles demandées d'abord.
            let mut sizes = [0u32; 5];
            let [a, d, sa, o, g] = &mut sizes;
            let null = std::ptr::null_mut();
            assert_eq!(
                MakeAbsoluteSD(rel.as_ptr(), null, a, null, d, null, sa, null, o, null, g),
                0
            );
            assert_eq!(sizes, [40, 8, 8, 12, 0]);
            let (mut ab, mut db, mut sb, mut ob) = ([0u8; 40], [0u8; 8], [0u8; 8], [0u8; 12]);
            let [a, d, sa, o, g] = &mut sizes;
            assert_eq!(
                MakeAbsoluteSD(
                    rel.as_ptr(),
                    ab.as_mut_ptr(),
                    a,
                    db.as_mut_ptr(),
                    d,
                    sb.as_mut_ptr(),
                    sa,
                    ob.as_mut_ptr(),
                    o,
                    null,
                    g
                ),
                1
            );
            assert_eq!(ob, sid);
            assert_eq!(control(ab.as_ptr()) & SE_SELF_RELATIVE, 0);
            let mut rm = 0u8;
            assert_eq!(
                GetSecurityDescriptorRMControl(ab.as_ptr(), &mut rm),
                ERROR_INVALID_DATA
            );
        }
    }

    #[test]
    fn attenuations_desactivees() {
        unsafe {
            let mut buf = [0xFFu8; 8];
            assert_eq!(
                GetProcessMitigationPolicy(usize::MAX, 8, buf.as_mut_ptr(), 8),
                1
            );
            assert_eq!(buf, [0; 8]);
        }
    }
}
