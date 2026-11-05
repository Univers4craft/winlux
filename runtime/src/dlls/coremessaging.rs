//! coremessaging.dll : `CreateDispatcherQueueController` et la file
//! `Windows.System.DispatcherQueue` (interfaces WinRT publiques du SDK,
//! dispatcherqueue.h et windows.system.h).
//!
//! Chaque file est rattachée à un thread. Elle s'appuie sur une fenêtre
//! cachée de ce thread : `TryEnqueue` range le délégué puis poste un
//! message à la fenêtre, et la boucle de messages du programme (WinForms,
//! WPF…) l'exécute sur le bon thread. Les minuteurs de la file
//! (`CreateTimer`) utilisent `SetTimer` sur la même fenêtre.
//!
//! Paint.NET crée sa file au démarrage (`InitializeDispatcherQueueController`).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, IID_IUNKNOWN,
    S_OK,
};

const IID_IINSPECTABLE: Guid = guid(
    0xAF86_E2E0,
    0xB12D,
    0x4C6A,
    [0x9C, 0x5A, 0xD7, 0xAA, 0x65, 0x10, 0x1E, 0x90],
);
const IID_IAGILEOBJECT: Guid = guid(
    0x94EA_2B94,
    0xE9CC,
    0x49E0,
    [0xC0, 0xFF, 0xEE, 0x64, 0xCA, 0x8F, 0x5B, 0x90],
);
const IID_CONTROLLER: Guid = guid(
    0x22F3_4E66,
    0x50DB,
    0x4E36,
    [0xA9, 0x8D, 0x61, 0xC0, 0x1B, 0x38, 0x4D, 0x20],
);
const IID_QUEUE: Guid = guid(
    0x603E_88E4,
    0xA338,
    0x4FFE,
    [0xA4, 0x57, 0xA5, 0xCF, 0xB9, 0xCE, 0xB8, 0x99],
);
const IID_QUEUE2: Guid = guid(
    0xC822_C647,
    0x30EF,
    0x506E,
    [0xBD, 0x1E, 0xA6, 0x47, 0xAE, 0x66, 0x75, 0xFF],
);
const IID_TIMER: Guid = guid(
    0x5FEA_BB1D,
    0xA31C,
    0x4727,
    [0xB1, 0xAC, 0x37, 0x45, 0x46, 0x49, 0xD5, 0x6A],
);

const DQTYPE_THREAD_DEDICATED: u32 = 1;
const DQTYPE_THREAD_CURRENT: u32 = 2;
const RPC_E_WRONG_THREAD: i32 = 0x8001_010Eu32 as i32;

/// Message privé de la fenêtre cachée : « des délégués attendent ».
const WM_DQ_RUN: u32 = 0x8000 + 0x2A1;
const CLASS: &str = "WinluxDispatcherQueue";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn tid() -> u32 {
    unsafe { libc::syscall(libc::SYS_gettid) as u32 }
}

// ---------------------------------------------------------------------
// Objets COM invités : appels de méthodes
// ---------------------------------------------------------------------

unsafe fn vcall(obj: usize, slot: usize) -> usize {
    let vtbl = *(obj as *const *const usize);
    *vtbl.add(slot)
}

unsafe fn add_ref(obj: usize) {
    if obj != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(vcall(obj, 1));
        f(obj);
    }
}

unsafe fn release(obj: usize) {
    if obj != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(vcall(obj, 2));
        f(obj);
    }
}

/// IDispatcherQueueHandler::Invoke (emplacement 3).
unsafe fn invoke_handler(h: usize) {
    let f: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(vcall(h, 3));
    let hr = f(h);
    if hr < 0 {
        crate::trace!("DispatcherQueue : délégué terminé par {hr:#x}");
    }
}

/// ITypedEventHandler<sender, args>::Invoke (emplacement 3).
unsafe fn invoke_typed(h: usize, sender: usize, args: usize) {
    let f: unsafe extern "win64" fn(usize, usize, usize) -> i32 = std::mem::transmute(vcall(h, 3));
    f(h, sender, args);
}

// ---------------------------------------------------------------------
// La file
// ---------------------------------------------------------------------

#[repr(C)]
struct Queue {
    vtbl: *const VTable<15>,
    vtbl2: *const VTable<7>,
    hwnd: usize,
    thread: u32,
    pending: Mutex<VecDeque<usize>>,
    events: Mutex<Vec<(i64, usize)>>,
}

#[repr(C)]
struct Controller {
    vtbl: *const VTable<8>,
    queue: *mut Queue,
}

#[repr(C)]
struct Timer {
    vtbl: *const VTable<15>,
    refs: AtomicU32,
    queue: *mut Queue,
    id: usize,
    interval_ms: u32,
    repeating: bool,
    running: bool,
    ticks: Vec<(i64, usize)>,
}

static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(1);
static NEXT_TIMER_ID: AtomicUsize = AtomicUsize::new(0x5D00);

/// Files par fenêtre cachée, et minuteurs par identifiant.
fn by_hwnd() -> &'static Mutex<HashMap<usize, usize>> {
    static M: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}
fn by_thread() -> &'static Mutex<HashMap<u32, usize>> {
    static M: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}
fn timers() -> &'static Mutex<HashMap<usize, usize>> {
    static M: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

unsafe extern "win64" fn window_proc(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        WM_DQ_RUN => {
            if let Some(&q) = lock(by_hwnd()).get(&hwnd) {
                run_pending(q as *mut Queue);
            }
            0
        }
        0x0113 => {
            // WM_TIMER
            if let Some(&t) = lock(timers()).get(&wp) {
                timer_fire(t as *mut Timer);
            }
            0
        }
        _ => super::gui::def_window_proc(hwnd, msg, wp, lp),
    }
}

unsafe fn run_pending(q: *mut Queue) {
    // Un délégué peut en remettre d'autres dans la file : on la vide par lots.
    loop {
        let batch: Vec<usize> = lock(&(*q).pending).drain(..).collect();
        if batch.is_empty() {
            break;
        }
        for h in batch {
            invoke_handler(h);
            release(h);
        }
    }
}

/// Crée la file du thread courant (une seule par thread).
unsafe fn queue_for_current_thread() -> *mut Queue {
    let me = tid();
    if let Some(&q) = lock(by_thread()).get(&me) {
        return q as *mut Queue;
    }
    super::gui::register_class(
        CLASS,
        super::gui::ClassInfo {
            wnd_proc: window_proc as *const () as usize,
            background: 0,
            instance: 0,
        },
    );
    let class: Vec<u16> = CLASS.encode_utf16().chain([0]).collect();
    let hwnd = super::user32::create_window_ex_w(
        0,
        class.as_ptr(),
        class.as_ptr(),
        0x8000_0000, // WS_POPUP, jamais affichée
        0,
        0,
        1,
        1,
        0,
        0,
        0,
        std::ptr::null_mut(),
    );
    let q = Box::into_raw(Box::new(Queue {
        vtbl: &QUEUE_VTBL,
        vtbl2: &QUEUE2_VTBL,
        hwnd,
        thread: me,
        pending: Mutex::new(VecDeque::new()),
        events: Mutex::new(Vec::new()),
    }));
    lock(by_hwnd()).insert(hwnd, q as usize);
    lock(by_thread()).insert(me, q as usize);
    q
}

unsafe fn enqueue(q: *mut Queue, handler: usize) -> bool {
    if handler == 0 || (*q).hwnd == 0 {
        return false;
    }
    add_ref(handler);
    lock(&(*q).pending).push_back(handler);
    super::user32::post_message_w((*q).hwnd, WM_DQ_RUN, 0, 0) != 0
}

// ---------------------------------------------------------------------
// IUnknown / IInspectable communs
// ---------------------------------------------------------------------

unsafe extern "win64" fn static_add_ref(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn static_release(_this: usize) -> u32 {
    1
}
unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn get_trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}
unsafe fn class_name(out: *mut usize, name: &str) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = name.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

fn is_base(iid: &Guid) -> bool {
    *iid == IID_IUNKNOWN || *iid == IID_IINSPECTABLE || *iid == IID_IAGILEOBJECT
}

fn log_unknown(what: &str, iid: &Guid) {
    crate::log_warn!(
        "coremessaging",
        "{what} : interface {:02x?} non prise en charge",
        iid
    );
}

// ---------------------------------------------------------------------
// IDispatcherQueueController
// ---------------------------------------------------------------------

unsafe extern "win64" fn ctl_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    if is_base(&iid) || iid == IID_CONTROLLER {
        *out = this;
        return S_OK;
    }
    log_unknown("DispatcherQueueController", &iid);
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn ctl_class(_this: usize, out: *mut usize) -> i32 {
    class_name(out, "Windows.System.DispatcherQueueController")
}
unsafe extern "win64" fn ctl_get_queue(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (*(this as *const Controller)).queue as usize;
    S_OK
}
unsafe extern "win64" fn not_impl(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

static CONTROLLER_VTBL: VTable<8> = VTable([
    ctl_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    get_iids as *const (),
    ctl_class as *const (),
    get_trust_level as *const (),
    ctl_get_queue as *const (),
    not_impl as *const (), // ShutdownQueueAsync
]);

// ---------------------------------------------------------------------
// IDispatcherQueue (+ IDispatcherQueue2 au décalage 8)
// ---------------------------------------------------------------------

unsafe fn queue_qi(q: *mut Queue, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    if is_base(&iid) || iid == IID_QUEUE {
        *out = q as usize;
        return S_OK;
    }
    if iid == IID_QUEUE2 {
        *out = std::ptr::addr_of!((*q).vtbl2) as usize;
        return S_OK;
    }
    log_unknown("DispatcherQueue", &iid);
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn q_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    queue_qi(this as *mut Queue, iid, out)
}
unsafe extern "win64" fn q_class(_this: usize, out: *mut usize) -> i32 {
    class_name(out, "Windows.System.DispatcherQueue")
}
unsafe extern "win64" fn q_create_timer(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let t = Box::into_raw(Box::new(Timer {
        vtbl: &TIMER_VTBL,
        refs: AtomicU32::new(1),
        queue: this as *mut Queue,
        id: NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed),
        interval_ms: 0,
        repeating: true,
        running: false,
        ticks: Vec::new(),
    }));
    *out = t as usize;
    S_OK
}
unsafe extern "win64" fn q_try_enqueue(this: usize, handler: usize, result: *mut u8) -> i32 {
    let ok = enqueue(this as *mut Queue, handler);
    if !result.is_null() {
        *result = ok as u8;
    }
    S_OK
}
unsafe extern "win64" fn q_try_enqueue_priority(
    this: usize,
    _priority: i32,
    handler: usize,
    result: *mut u8,
) -> i32 {
    q_try_enqueue(this, handler, result)
}
unsafe extern "win64" fn q_add_event(this: usize, handler: usize, token: *mut i64) -> i32 {
    if token.is_null() {
        return E_POINTER;
    }
    let t = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed) as i64;
    add_ref(handler);
    lock(&(*(this as *mut Queue)).events).push((t, handler));
    *token = t;
    S_OK
}
unsafe extern "win64" fn q_remove_event(this: usize, token: i64) -> i32 {
    let mut ev = lock(&(*(this as *mut Queue)).events);
    if let Some(i) = ev.iter().position(|(t, _)| *t == token) {
        let (_, h) = ev.remove(i);
        drop(ev);
        release(h);
    }
    S_OK
}

static QUEUE_VTBL: VTable<15> = VTable([
    q_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    get_iids as *const (),
    q_class as *const (),
    get_trust_level as *const (),
    q_create_timer as *const (),
    q_try_enqueue as *const (),
    q_try_enqueue_priority as *const (),
    q_add_event as *const (), // add_ShutdownStarting
    q_remove_event as *const (),
    q_add_event as *const (), // add_ShutdownCompleted
    q_remove_event as *const (),
    std::ptr::null(),
    std::ptr::null(),
]);

unsafe fn from_q2(this: usize) -> *mut Queue {
    (this - std::mem::size_of::<usize>()) as *mut Queue
}
unsafe extern "win64" fn q2_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    queue_qi(from_q2(this), iid, out)
}
unsafe extern "win64" fn q2_has_thread_access(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = ((*from_q2(this)).thread == tid()) as u8;
    S_OK
}

static QUEUE2_VTBL: VTable<7> = VTable([
    q2_qi as *const (),
    static_add_ref as *const (),
    static_release as *const (),
    get_iids as *const (),
    q_class as *const (),
    get_trust_level as *const (),
    q2_has_thread_access as *const (),
]);

// ---------------------------------------------------------------------
// IDispatcherQueueTimer
// ---------------------------------------------------------------------

unsafe extern "win64" fn t_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    if is_base(&iid) || iid == IID_TIMER {
        t_add_ref(this);
        *out = this;
        return S_OK;
    }
    log_unknown("DispatcherQueueTimer", &iid);
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn t_add_ref(this: usize) -> u32 {
    (*(this as *mut Timer)).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn t_release(this: usize) -> u32 {
    let t = this as *mut Timer;
    let left = (*t).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if left == 0 {
        t_stop(this);
        let b = Box::from_raw(t);
        for (_, h) in &b.ticks {
            release(*h);
        }
    }
    left
}
unsafe extern "win64" fn t_class(_this: usize, out: *mut usize) -> i32 {
    class_name(out, "Windows.System.DispatcherQueueTimer")
}
unsafe extern "win64" fn t_get_interval(this: usize, out: *mut i64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (*(this as *mut Timer)).interval_ms as i64 * 10_000;
    S_OK
}
unsafe extern "win64" fn t_put_interval(this: usize, span: i64) -> i32 {
    let t = this as *mut Timer;
    (*t).interval_ms = (span / 10_000).clamp(0, u32::MAX as i64) as u32;
    if (*t).running {
        t_start(this);
    }
    S_OK
}
unsafe extern "win64" fn t_get_running(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (*(this as *mut Timer)).running as u8;
    S_OK
}
unsafe extern "win64" fn t_get_repeating(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = (*(this as *mut Timer)).repeating as u8;
    S_OK
}
unsafe extern "win64" fn t_put_repeating(this: usize, v: u8) -> i32 {
    (*(this as *mut Timer)).repeating = v != 0;
    S_OK
}
unsafe extern "win64" fn t_start(this: usize) -> i32 {
    let t = this as *mut Timer;
    let hwnd = (*(*t).queue).hwnd;
    if (*(*t).queue).thread != tid() {
        return RPC_E_WRONG_THREAD;
    }
    lock(timers()).insert((*t).id, t as usize);
    super::user32::set_timer(hwnd, (*t).id, (*t).interval_ms.max(1), 0);
    (*t).running = true;
    S_OK
}
unsafe extern "win64" fn t_stop(this: usize) -> i32 {
    let t = this as *mut Timer;
    if (*t).running {
        super::user32::kill_timer((*(*t).queue).hwnd, (*t).id);
        (*t).running = false;
    }
    lock(timers()).remove(&(*t).id);
    S_OK
}
unsafe extern "win64" fn t_add_tick(this: usize, handler: usize, token: *mut i64) -> i32 {
    if token.is_null() {
        return E_POINTER;
    }
    let tok = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed) as i64;
    add_ref(handler);
    (*(this as *mut Timer)).ticks.push((tok, handler));
    *token = tok;
    S_OK
}
unsafe extern "win64" fn t_remove_tick(this: usize, token: i64) -> i32 {
    let t = this as *mut Timer;
    if let Some(i) = (*t).ticks.iter().position(|(k, _)| *k == token) {
        let (_, h) = (*t).ticks.remove(i);
        release(h);
    }
    S_OK
}

unsafe fn timer_fire(t: *mut Timer) {
    if !(*t).repeating {
        t_stop(t as usize);
    }
    t_add_ref(t as usize);
    let handlers: Vec<usize> = (*t).ticks.iter().map(|(_, h)| *h).collect();
    for h in handlers {
        invoke_typed(h, t as usize, 0);
    }
    t_release(t as usize);
}

static TIMER_VTBL: VTable<15> = VTable([
    t_qi as *const (),
    t_add_ref as *const (),
    t_release as *const (),
    get_iids as *const (),
    t_class as *const (),
    get_trust_level as *const (),
    t_get_interval as *const (),
    t_put_interval as *const (),
    t_get_running as *const (),
    t_get_repeating as *const (),
    t_put_repeating as *const (),
    t_start as *const (),
    t_stop as *const (),
    t_add_tick as *const (),
    t_remove_tick as *const (),
]);

// ---------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------

/// DispatcherQueueOptions : dwSize, threadType, apartmentType.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Options {
    size: u32,
    thread_type: u32,
    apartment: u32,
}

/// Thread dédié : il crée sa file puis fait tourner sa boucle de messages.
unsafe extern "win64" fn dedicated_thread(param: usize) -> u32 {
    let slot = param as *const AtomicUsize;
    let q = queue_for_current_thread();
    (*slot).store(q as usize, Ordering::Release);
    let mut msg: super::gui::Msg = std::mem::zeroed();
    while super::user32::get_message_w(&mut msg, 0, 0, 0) > 0 {
        super::user32::translate_message(&msg);
        super::user32::dispatch_message_w(&msg);
    }
    0
}

/// La structure de 12 octets est passée par adresse (convention x64).
pub unsafe extern "win64" fn create_dispatcher_queue_controller(
    options: *const Options,
    out: *mut usize,
) -> i32 {
    if out.is_null() || options.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let options = std::ptr::read_unaligned(options);
    if options.size as usize != std::mem::size_of::<Options>() {
        return E_INVALIDARG;
    }
    let q = match options.thread_type {
        DQTYPE_THREAD_CURRENT => {
            let me = tid();
            if lock(by_thread()).contains_key(&me) {
                return RPC_E_WRONG_THREAD; // une file existe déjà sur ce thread
            }
            queue_for_current_thread()
        }
        DQTYPE_THREAD_DEDICATED => {
            let slot: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
            let h = super::kernel32::create_thread(
                0,
                0,
                dedicated_thread as *const () as usize,
                slot as *const AtomicUsize as usize,
                0,
                std::ptr::null_mut(),
            );
            if h == 0 {
                return E_INVALIDARG;
            }
            let mut waited = 0;
            while slot.load(Ordering::Acquire) == 0 && waited < 5000 {
                std::thread::sleep(std::time::Duration::from_millis(1));
                waited += 1;
            }
            slot.load(Ordering::Acquire) as *mut Queue
        }
        _ => return E_INVALIDARG,
    };
    if q.is_null() || (*q).hwnd == 0 {
        return E_INVALIDARG;
    }
    let c = Box::into_raw(Box::new(Controller {
        vtbl: &CONTROLLER_VTBL,
        queue: q,
    }));
    *out = c as usize;
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateDispatcherQueueController" => fn create_dispatcher_queue_controller,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct Handler {
        vtbl: *const VTable<4>,
        refs: AtomicU32,
        calls: AtomicU32,
    }
    unsafe extern "win64" fn h_qi(_: usize, _: *const u8, _: *mut usize) -> i32 {
        E_NOINTERFACE
    }
    unsafe extern "win64" fn h_add(this: usize) -> u32 {
        (*(this as *mut Handler))
            .refs
            .fetch_add(1, Ordering::AcqRel)
            + 1
    }
    unsafe extern "win64" fn h_rel(this: usize) -> u32 {
        (*(this as *mut Handler))
            .refs
            .fetch_sub(1, Ordering::AcqRel)
            - 1
    }
    unsafe extern "win64" fn h_invoke(this: usize) -> i32 {
        (*(this as *mut Handler))
            .calls
            .fetch_add(1, Ordering::AcqRel);
        S_OK
    }
    static H_VTBL: VTable<4> = VTable([
        h_qi as *const (),
        h_add as *const (),
        h_rel as *const (),
        h_invoke as *const (),
    ]);

    #[test]
    fn file_du_thread_courant_execute_les_delegues() {
        std::thread::spawn(|| unsafe {
            let mut c = 0usize;
            let opts = Options {
                size: 12,
                thread_type: DQTYPE_THREAD_CURRENT,
                apartment: 0,
            };
            assert_eq!(create_dispatcher_queue_controller(&opts, &mut c), S_OK);
            assert_eq!(
                create_dispatcher_queue_controller(&opts, &mut 0),
                RPC_E_WRONG_THREAD
            );
            let mut q = 0usize;
            assert_eq!(ctl_get_queue(c, &mut q), S_OK);
            let mut q2 = 0usize;
            assert_eq!(q_qi(q, IID_QUEUE2.as_ptr(), &mut q2), S_OK);
            let mut access = 0u8;
            q2_has_thread_access(q2, &mut access);
            assert_eq!(access, 1);
            let h = Handler {
                vtbl: &H_VTBL,
                refs: AtomicU32::new(1),
                calls: AtomicU32::new(0),
            };
            let mut ok = 0u8;
            q_try_enqueue(q, &h as *const Handler as usize, &mut ok);
            assert_eq!(ok, 1);
            assert_eq!(h.refs.load(Ordering::Acquire), 2);
            // La boucle de messages du thread exécute le délégué.
            let mut msg: super::super::gui::Msg = std::mem::zeroed();
            while super::super::user32::peek_message_w(&mut msg, 0, 0, 0, 1) != 0 {
                super::super::user32::dispatch_message_w(&msg);
            }
            assert_eq!(h.calls.load(Ordering::Acquire), 1);
            assert_eq!(h.refs.load(Ordering::Acquire), 1);
        })
        .join()
        .unwrap();
    }
}
