//! Pool de threads Windows (API « Threadpool » de Vista) : minuteurs
//! (CreateThreadpoolTimer…), travaux (CreateThreadpoolWork…) et
//! TrySubmitThreadpoolCallback.
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
    _env: usize,
) -> *const Work {
    let Some(callback) = callback else {
        crate::process::set_last_error(87);
        return std::ptr::null();
    };
    Arc::into_raw(Arc::new(Work {
        callback,
        context,
        flight: InFlight::default(),
    }))
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
    _env: usize,
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
    Arc::into_raw(timer)
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
    _window_ms: u32,
) {
    let Some(t) = timer.as_ref() else { return };
    {
        let mut d = lock(&t.due);
        *d = if due.is_null() {
            (None, Duration::ZERO)
        } else {
            (
                Some(due_instant(due.read_unaligned())),
                Duration::from_millis(period_ms.into()),
            )
        };
    }
    timers().changed.notify_all();
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
    let t = Arc::from_raw(timer);
    lock(&t.due).0 = None;
    lock(&timers().list).retain(|x| !Arc::ptr_eq(x, &t));
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
        "IsThreadpoolTimerSet" => fn is_threadpool_timer_set,
        "WaitForThreadpoolTimerCallbacks" => fn wait_for_threadpool_timer_callbacks,
        "CloseThreadpoolTimer" => fn close_threadpool_timer,
        "CallbackMayRunLong" => fn callback_may_run_long,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

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
