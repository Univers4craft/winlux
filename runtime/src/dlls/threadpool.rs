//! Pool de threads Windows (API « Threadpool » de Vista) : minuteurs
//! (CreateThreadpoolTimer…), travaux (CreateThreadpoolWork…),
//! TrySubmitThreadpoolCallback et groupes de nettoyage
//! (CreateThreadpoolCleanupGroup…, Teams #2110).
//!
//! Les rappels s'exécutent sur des threads Windows (TEB, notifications
//! DLL_THREAD_ATTACH) créés à la demande et réutilisés ; un thread dédié
//! déclenche les minuteurs à leur échéance.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::dlls::lock;

type Job = Box<dyn FnOnce() + Send>;

struct Pool {
    jobs: Mutex<(VecDeque<Job>, usize)>, // (file, threads inactifs)
    ready: Condvar,
    threads: AtomicUsize,
}

const MAX_THREADS: usize = 128;

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| Pool {
        jobs: Mutex::new((VecDeque::new(), 0)),
        ready: Condvar::new(),
        threads: AtomicUsize::new(0),
    })
}

fn worker() {
    crate::process::thread_attach_current();
    let p = pool();
    loop {
        let job = {
            let mut q = lock(&p.jobs);
            loop {
                if let Some(j) = q.0.pop_front() {
                    break j;
                }
                q.1 += 1;
                q = p
                    .ready
                    .wait(q)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                q.1 -= 1;
            }
        };
        job();
    }
}

/// Met un rappel en file ; un thread de plus est créé si aucun n'est libre.
fn submit(job: Job) {
    let p = pool();
    let spawn = {
        let mut q = lock(&p.jobs);
        q.0.push_back(job);
        q.1 < q.0.len() && p.threads.load(Ordering::Relaxed) < MAX_THREADS
    };
    if spawn {
        let n = p.threads.fetch_add(1, Ordering::Relaxed);
        let _ = std::thread::Builder::new()
            .name(format!("win-pool-{n}"))
            .spawn(worker);
    }
    p.ready.notify_one();
}

/// Rappels en cours ou en file d'un objet, pour les Wait…Callbacks.
#[derive(Default)]
struct InFlight {
    count: Mutex<usize>,
    done: Condvar,
}

impl InFlight {
    fn start(&self) {
        *lock(&self.count) += 1;
    }
    fn finish(&self) {
        let mut c = lock(&self.count);
        *c -= 1;
        if *c == 0 {
            self.done.notify_all();
        }
    }
    fn wait(&self) {
        let mut c = lock(&self.count);
        while *c != 0 {
            c = self
                .done
                .wait(c)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

/// PTP_CALLBACK_INSTANCE : un pointeur non nul propre à chaque appel.
#[repr(C)]
pub struct Instance {
    _reserved: usize,
}

// --- Travaux ------------------------------------------------------------------------

type WorkCallback = unsafe extern "win64" fn(*mut Instance, usize, *const Work);
type SimpleCallback = unsafe extern "win64" fn(*mut Instance, usize);

pub struct Work {
    callback: WorkCallback,
    context: usize,
    flight: InFlight,
}

pub unsafe extern "win64" fn create_threadpool_work(
    callback: Option<WorkCallback>,
    context: usize,
    env: usize,
) -> *const Work {
    let Some(callback) = callback else {
        crate::process::set_last_error(87);
        return std::ptr::null();
    };
    let work = Arc::into_raw(Arc::new(Work {
        callback,
        context,
        flight: InFlight::default(),
    }));
    join_group(env, Member::Work(work as usize));
    work
}

pub unsafe extern "win64" fn submit_threadpool_work(work: *const Work) {
    if work.is_null() {
        return;
    }
    Arc::increment_strong_count(work);
    let w = Arc::from_raw(work);
    w.flight.start();
    let addr = work as usize;
    submit(Box::new(move || {
        let mut inst = Instance { _reserved: 0 };
        (w.callback)(&mut inst, w.context, addr as *const Work);
        w.flight.finish();
    }));
}

pub unsafe extern "win64" fn wait_for_threadpool_work_callbacks(work: *const Work, _cancel: i32) {
    if let Some(w) = work.as_ref() {
        w.flight.wait();
    }
}

pub unsafe extern "win64" fn close_threadpool_work(work: *const Work) {
    if !work.is_null() {
        leave_groups(work as usize);
        drop(Arc::from_raw(work));
    }
}

pub unsafe extern "win64" fn try_submit_threadpool_callback(
    callback: Option<SimpleCallback>,
    context: usize,
    _env: usize,
) -> i32 {
    let Some(callback) = callback else {
        crate::process::set_last_error(87);
        return 0;
    };
    submit(Box::new(move || {
        let mut inst = Instance { _reserved: 0 };
        callback(&mut inst, context);
    }));
    1
}

// --- Minuteurs ------------------------------------------------------------------------

type TimerCallback = unsafe extern "win64" fn(*mut Instance, usize, *const Timer);

pub struct Timer {
    callback: TimerCallback,
    context: usize,
    /// (prochaine échéance, période)
    due: Mutex<(Option<Instant>, Duration)>,
    flight: InFlight,
}

struct Timers {
    list: Mutex<Vec<Arc<Timer>>>,
    changed: Condvar,
}

fn timers() -> &'static Timers {
    static T: OnceLock<Timers> = OnceLock::new();
    T.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("win-pool-timers".into())
            .spawn(timer_thread);
        Timers {
            list: Mutex::new(Vec::new()),
            changed: Condvar::new(),
        }
    })
}

fn timer_thread() {
    let t = timers();
    let mut list = lock(&t.list);
    loop {
        let now = Instant::now();
        let mut next: Option<Instant> = None;
        for timer in list.iter() {
            let mut d = lock(&timer.due);
            let Some(at) = d.0 else { continue };
            if at <= now {
                d.0 = (!d.1.is_zero()).then(|| now + d.1);
                timer.flight.start();
                let tm = Arc::clone(timer);
                submit(Box::new(move || {
                    let mut inst = Instance { _reserved: 0 };
                    unsafe { (tm.callback)(&mut inst, tm.context, Arc::as_ptr(&tm)) };
                    tm.flight.finish();
                }));
            }
            if let Some(at) = d.0 {
                next = Some(next.map_or(at, |n| n.min(at)));
            }
        }
        list = match next {
            Some(at) => {
                let wait = at.saturating_duration_since(Instant::now());
                t.changed
                    .wait_timeout(list, wait)
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0
            }
            None => t
                .changed
                .wait(list)
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        };
    }
}

pub unsafe extern "win64" fn create_threadpool_timer(
    callback: Option<TimerCallback>,
    context: usize,
    env: usize,
) -> *const Timer {
    let Some(callback) = callback else {
        crate::process::set_last_error(87);
        return std::ptr::null();
    };
    let timer = Arc::new(Timer {
        callback,
        context,
        due: Mutex::new((None, Duration::ZERO)),
        flight: InFlight::default(),
    });
    lock(&timers().list).push(Arc::clone(&timer));
    let timer = Arc::into_raw(timer);
    join_group(env, Member::Timer(timer as usize));
    timer
}

/// Échéance FILETIME : négative = relative (unités de 100 ns), positive =
/// date absolue, zéro = tout de suite.
fn due_instant(value: i64) -> Instant {
    let now = Instant::now();
    if value < 0 {
        return now + Duration::from_nanos(value.unsigned_abs().saturating_mul(100));
    }
    let current = super::kernel32::filetime_now() as i64;
    now + Duration::from_nanos((value - current).max(0) as u64 * 100)
}

pub unsafe extern "win64" fn set_threadpool_timer(
    timer: *const Timer,
    due: *const i64,
    period_ms: u32,
    window_ms: u32,
) {
    set_threadpool_timer_ex(timer, due, period_ms, window_ms);
}

/// SetThreadpoolTimerEx : comme SetThreadpoolTimer, et rend TRUE si le
/// minuteur était armé avant l'appel (il ne partira alors plus pour
/// l'ancienne échéance).
pub unsafe extern "win64" fn set_threadpool_timer_ex(
    timer: *const Timer,
    due: *const i64,
    period_ms: u32,
    _window_ms: u32,
) -> i32 {
    let Some(t) = timer.as_ref() else { return 0 };
    // Sous le verrou de la liste, celui de la variable de condition : un
    // réveil émis pendant que le thread des minuteurs recalcule la prochaine
    // échéance (sans la voir) serait sinon perdu et le minuteur ne partirait
    // jamais.
    let _list = lock(&timers().list);
    let was_set = {
        let mut d = lock(&t.due);
        let was_set = d.0.is_some();
        *d = if due.is_null() {
            (None, Duration::ZERO)
        } else {
            (
                Some(due_instant(due.read_unaligned())),
                Duration::from_millis(period_ms.into()),
            )
        };
        was_set
    };
    timers().changed.notify_all();
    i32::from(was_set)
}

pub unsafe extern "win64" fn is_threadpool_timer_set(timer: *const Timer) -> i32 {
    timer
        .as_ref()
        .map_or(0, |t| i32::from(lock(&t.due).0.is_some()))
}

pub unsafe extern "win64" fn wait_for_threadpool_timer_callbacks(timer: *const Timer, cancel: i32) {
    let Some(t) = timer.as_ref() else { return };
    if cancel != 0 {
        lock(&t.due).0 = None;
    }
    t.flight.wait();
}

pub unsafe extern "win64" fn close_threadpool_timer(timer: *const Timer) {
    if timer.is_null() {
        return;
    }
    leave_groups(timer as usize);
    let t = Arc::from_raw(timer);
    lock(&t.due).0 = None;
    lock(&timers().list).retain(|x| !Arc::ptr_eq(x, &t));
}

// --- Groupes de nettoyage ----------------------------------------------------------
//
// Documentation publique : CreateThreadpoolCleanupGroup,
// CloseThreadpoolCleanupGroupMembers, CloseThreadpoolCleanupGroup et
// TP_CALLBACK_ENVIRON (winnt.h : Version, Pool, CleanupGroup,
// CleanupGroupCancelCallback…). Un objet créé avec un environnement qui
// nomme un groupe en devient membre ; le fermer seul l'en retire.

/// Membre d'un groupe : travail, minuteur ou attente (apps_win32).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Member {
    Work(usize),
    Timer(usize),
    Wait(usize),
}

impl Member {
    fn addr(self) -> usize {
        match self {
            Member::Work(a) | Member::Timer(a) | Member::Wait(a) => a,
        }
    }
}

/// PTP_CLEANUP_GROUP_CANCEL_CALLBACK(ObjectContext, CleanupContext).
type CancelCallback = unsafe extern "win64" fn(usize, usize);

/// Groupe → membres, chacun avec la fonction d'annulation de son
/// environnement (0 si aucune).
fn groups() -> &'static Mutex<std::collections::HashMap<usize, Vec<(Member, usize, usize)>>> {
    static G: OnceLock<Mutex<std::collections::HashMap<usize, Vec<(Member, usize, usize)>>>> =
        OnceLock::new();
    G.get_or_init(Default::default)
}

/// Groupe et fonction d'annulation lus dans un TP_CALLBACK_ENVIRON.
/// Disposition x64 : CleanupGroup à 16, CleanupGroupCancelCallback à 24 ;
/// x86 : 8 et 12. La fonction d'annulation d'un programme 32 bits n'est
/// pas appelée (convention d'appel différente).
unsafe fn env_group(env: usize) -> Option<(usize, usize)> {
    if env == 0 {
        return None;
    }
    let version = std::ptr::read_unaligned(env as *const u32);
    if version != 1 && version != 3 {
        return None;
    }
    let (group, cancel) = if crate::wow32::active() {
        (
            std::ptr::read_unaligned((env + 8) as *const u32) as usize,
            0,
        )
    } else {
        (
            std::ptr::read_unaligned((env + 16) as *const usize),
            std::ptr::read_unaligned((env + 24) as *const usize),
        )
    };
    (group != 0).then_some((group, cancel))
}

/// Inscrit un objet tout juste créé dans le groupe de son environnement.
/// `context` est celui de l'objet, rendu à la fonction d'annulation.
pub unsafe fn join_group_with_context(env: usize, member: Member, context: usize) {
    let Some((group, cancel)) = env_group(env) else {
        return;
    };
    if let Some(list) = lock(groups()).get_mut(&group) {
        list.push((member, cancel, context));
    }
}

unsafe fn join_group(env: usize, member: Member) {
    let context = match member {
        Member::Work(a) => (*(a as *const Work)).context,
        Member::Timer(a) => (*(a as *const Timer)).context,
        Member::Wait(_) => 0,
    };
    join_group_with_context(env, member, context);
}

/// Un objet fermé seul quitte son groupe.
pub fn leave_groups(addr: usize) {
    for list in lock(groups()).values_mut() {
        list.retain(|(m, _, _)| m.addr() != addr);
    }
}

pub extern "win64" fn create_threadpool_cleanup_group() -> usize {
    static NEXT: AtomicUsize = AtomicUsize::new(0x7E00_0000);
    let h = NEXT.fetch_add(8, Ordering::Relaxed);
    lock(groups()).insert(h, Vec::new());
    h
}

/// Ferme chaque membre : attend ses rappels en cours, annule ceux qui
/// n'ont pas commencé si `cancel`, puis le libère.
pub unsafe extern "win64" fn close_threadpool_cleanup_group_members(
    group: usize,
    cancel: i32,
    cleanup_context: usize,
) {
    let members = match lock(groups()).get_mut(&group) {
        Some(list) => std::mem::take(list),
        None => return,
    };
    for (member, cancel_cb, context) in members {
        match member {
            Member::Work(a) => {
                let w = a as *const Work;
                wait_for_threadpool_work_callbacks(w, cancel);
                drop(Arc::from_raw(w));
            }
            Member::Timer(a) => {
                let t = a as *const Timer;
                lock(&(*t).due).0 = None;
                wait_for_threadpool_timer_callbacks(t, 1);
                let t = Arc::from_raw(t);
                lock(&timers().list).retain(|x| !Arc::ptr_eq(x, &t));
            }
            Member::Wait(a) => {
                crate::dlls::apps_win32::release_threadpool_wait(a, cancel != 0);
            }
        }
        if cancel != 0 && cancel_cb != 0 {
            let f: CancelCallback = std::mem::transmute(cancel_cb);
            f(context, cleanup_context);
        }
    }
}

pub extern "win64" fn close_threadpool_cleanup_group(group: usize) {
    lock(groups()).remove(&group);
}

/// Pools privés (CreateThreadpool, #1606) : les rappels de Winlux passent
/// tous par le pool partagé ; la table ne garde que les bornes demandées
/// (minimum, maximum). Poignées multiples de 4, sous 4 Gio pour les
/// programmes x86.
fn private_pools() -> &'static Mutex<std::collections::HashMap<usize, (u32, u32)>> {
    static P: OnceLock<Mutex<std::collections::HashMap<usize, (u32, u32)>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

pub extern "win64" fn create_threadpool(_reserved: usize) -> usize {
    static NEXT: AtomicUsize = AtomicUsize::new(0x7D00_0000);
    let h = NEXT.fetch_add(4, Ordering::Relaxed);
    lock(private_pools()).insert(h, (0, MAX_THREADS as u32));
    h
}

pub extern "win64" fn close_threadpool(pool: usize) {
    lock(private_pools()).remove(&pool);
}

pub extern "win64" fn set_threadpool_thread_maximum(pool: usize, max: u32) {
    if let Some(p) = lock(private_pools()).get_mut(&pool) {
        p.1 = max;
    }
}

pub extern "win64" fn set_threadpool_thread_minimum(pool: usize, min: u32) -> i32 {
    match lock(private_pools()).get_mut(&pool) {
        Some(p) => {
            p.0 = min;
            1
        }
        None => {
            crate::process::set_last_error(87);
            0
        }
    }
}

/// CallbackMayRunLong : le pool crée de toute façon un thread de plus au besoin.
pub extern "win64" fn callback_may_run_long(_instance: usize) -> i32 {
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateThreadpoolWork" => fn create_threadpool_work,
        "SubmitThreadpoolWork" => fn submit_threadpool_work,
        "WaitForThreadpoolWorkCallbacks" => fn wait_for_threadpool_work_callbacks,
        "CloseThreadpoolWork" => fn close_threadpool_work,
        "TrySubmitThreadpoolCallback" => fn try_submit_threadpool_callback,
        "CreateThreadpoolTimer" => fn create_threadpool_timer,
        "SetThreadpoolTimer" => fn set_threadpool_timer,
        "SetThreadpoolTimerEx" => fn set_threadpool_timer_ex,
        "CreateThreadpoolCleanupGroup" => fn create_threadpool_cleanup_group,
        "CloseThreadpoolCleanupGroupMembers" => fn close_threadpool_cleanup_group_members,
        "CloseThreadpoolCleanupGroup" => fn close_threadpool_cleanup_group,
        "IsThreadpoolTimerSet" => fn is_threadpool_timer_set,
        "WaitForThreadpoolTimerCallbacks" => fn wait_for_threadpool_timer_callbacks,
        "CloseThreadpoolTimer" => fn close_threadpool_timer,
        "CallbackMayRunLong" => fn callback_may_run_long,
        "CreateThreadpool" => fn create_threadpool,
        "CloseThreadpool" => fn close_threadpool,
        "SetThreadpoolThreadMaximum" => fn set_threadpool_thread_maximum,
        "SetThreadpoolThreadMinimum" => fn set_threadpool_thread_minimum,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn pool_prive() {
        let _teb = crate::process::test_teb();
        let p = create_threadpool(0);
        assert_ne!(p, 0);
        assert_eq!(set_threadpool_thread_minimum(p, 2), 1);
        set_threadpool_thread_maximum(p, 4);
        assert_eq!(lock(private_pools()).get(&p), Some(&(2, 4)));
        close_threadpool(p);
        assert_eq!(set_threadpool_thread_minimum(p, 2), 0);
    }

    unsafe extern "win64" fn count_work(inst: *mut Instance, ctx: usize, _w: *const Work) {
        assert!(!inst.is_null());
        (*(ctx as *const AtomicU32)).fetch_add(1, Ordering::SeqCst);
    }

    unsafe extern "win64" fn count_timer(_inst: *mut Instance, ctx: usize, _t: *const Timer) {
        (*(ctx as *const AtomicU32)).fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn travaux_soumis_et_attendus() {
        let hits = Box::leak(Box::new(AtomicU32::new(0)));
        unsafe {
            let w = create_threadpool_work(Some(count_work), hits as *const _ as usize, 0);
            for _ in 0..5 {
                submit_threadpool_work(w);
            }
            wait_for_threadpool_work_callbacks(w, 0);
            assert_eq!(hits.load(Ordering::SeqCst), 5);
            close_threadpool_work(w);
        }
    }

    #[test]
    fn minuteur_unique_puis_periodique() {
        let hits = Box::leak(Box::new(AtomicU32::new(0)));
        unsafe {
            let t = create_threadpool_timer(Some(count_timer), hits as *const _ as usize, 0);
            assert_eq!(is_threadpool_timer_set(t), 0);
            let due = -10_000i64; // 1 ms, relatif
            set_threadpool_timer(t, &due, 0, 0);
            let start = Instant::now();
            while hits.load(Ordering::SeqCst) == 0 && start.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(1));
            }
            wait_for_threadpool_timer_callbacks(t, 0);
            assert_eq!(hits.load(Ordering::SeqCst), 1);
            assert_eq!(is_threadpool_timer_set(t), 0);
            set_threadpool_timer(t, &due, 2, 0);
            while hits.load(Ordering::SeqCst) < 4 && start.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(hits.load(Ordering::SeqCst) >= 4);
            set_threadpool_timer(t, std::ptr::null(), 0, 0);
            wait_for_threadpool_timer_callbacks(t, 1);
            let after = hits.load(Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            assert_eq!(hits.load(Ordering::SeqCst), after);
            close_threadpool_timer(t);
        }
    }
}
