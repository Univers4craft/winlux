//! Files de minuteurs (CreateTimerQueue, CreateTimerQueueTimer…), d'après
//! la documentation publique de Microsoft Learn. Chaque minuteur a son
//! thread Windows qui attend l'échéance puis appelle
//! `callback(parameter, TRUE)` ; la période relance le minuteur. La
//! suppression empêche tout rappel qui n'a pas commencé et, selon
//! l'événement de fin donné, attend ou signale la fin du rappel en cours.
//! L'installeur web de Creative Cloud s'arrête si elles manquent (#1226).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::dlls::lock;
use crate::process::set_last_error;

const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_IO_PENDING: u32 = 997;
const WT_EXECUTEONLYONCE: u32 = 0x8;
const INVALID_HANDLE_VALUE: usize = usize::MAX;

#[derive(Default)]
struct State {
    cancelled: bool,
    /// Un rappel est en cours.
    running: bool,
    /// Prochaine échéance ; `None` : minuteur unique déjà parti.
    due: Option<Instant>,
    period: u32,
    thread: Option<std::thread::ThreadId>,
}

#[derive(Default)]
struct Timer {
    state: Mutex<State>,
    cv: Condvar,
}

impl Timer {
    /// Attend la fin du rappel en cours, sauf depuis ce rappel lui-même.
    fn wait_idle(&self) {
        let mut s = lock(&self.state);
        if s.thread == Some(std::thread::current().id()) {
            return;
        }
        while s.running {
            s = self
                .cv
                .wait(s)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Thread du minuteur.
    unsafe fn run(&self, callback: usize, parameter: usize, once: bool) {
        let cb: extern "win64" fn(usize, u8) = std::mem::transmute(callback);
        lock(&self.state).thread = Some(std::thread::current().id());
        loop {
            {
                let mut s = lock(&self.state);
                loop {
                    if s.cancelled {
                        return;
                    }
                    match s.due {
                        Some(due) => {
                            let now = Instant::now();
                            if now >= due {
                                break;
                            }
                            s = self
                                .cv
                                .wait_timeout(s, due - now)
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .0;
                        }
                        None => {
                            if once {
                                return;
                            }
                            s = self
                                .cv
                                .wait(s)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                    }
                }
                s.running = true;
                s.due = match s.period {
                    0 => None,
                    p => Some(Instant::now() + Duration::from_millis(p as u64)),
                };
            }
            cb(parameter, 1);
            let mut s = lock(&self.state);
            s.running = false;
            self.cv.notify_all();
        }
    }
}

/// Minuteurs par handle, avec leur file (0 : file par défaut).
struct Tables {
    timers: HashMap<usize, (usize, Arc<Timer>)>,
    queues: HashMap<usize, ()>,
}

static TABLES: Mutex<Option<Tables>> = Mutex::new(None);
static NEXT: AtomicUsize = AtomicUsize::new(0x544D_0000);

fn with_tables<R>(f: impl FnOnce(&mut Tables) -> R) -> R {
    let mut g = lock(&TABLES);
    f(g.get_or_insert_with(|| Tables {
        timers: HashMap::new(),
        queues: HashMap::new(),
    }))
}

fn new_handle() -> usize {
    NEXT.fetch_add(4, Ordering::Relaxed)
}

/// CreateTimerQueue : une file vide.
pub extern "win64" fn create_timer_queue() -> usize {
    let h = new_handle();
    with_tables(|t| t.queues.insert(h, ()));
    h
}

unsafe fn create_timer(
    queue: usize,
    callback: usize,
    parameter: usize,
    due: u32,
    period: u32,
    flags: u32,
) -> Option<usize> {
    if callback == 0 || (queue != 0 && !with_tables(|t| t.queues.contains_key(&queue))) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return None;
    }
    let once = flags & WT_EXECUTEONLYONCE != 0;
    let timer = Arc::new(Timer::default());
    {
        let mut s = lock(&timer.state);
        s.due = Some(Instant::now() + Duration::from_millis(due as u64));
        s.period = if once { 0 } else { period };
    }
    let h = new_handle();
    with_tables(|t| t.timers.insert(h, (queue, timer.clone())));
    let ok = super::kernel32_node::spawn_windows_thread(move || unsafe {
        timer.run(callback, parameter, once)
    });
    if !ok {
        with_tables(|t| t.timers.remove(&h));
        set_last_error(ERROR_NOT_ENOUGH_MEMORY);
        return None;
    }
    Some(h)
}

/// CreateTimerQueueTimer(phNewTimer, TimerQueue, Callback, Parameter,
/// DueTime, Period, Flags).
pub unsafe extern "win64" fn create_timer_queue_timer(
    out: *mut usize,
    queue: usize,
    callback: usize,
    parameter: usize,
    due: u32,
    period: u32,
    flags: u32,
) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    match create_timer(queue, callback, parameter, due, period, flags) {
        Some(h) => {
            *out = h;
            1
        }
        None => 0,
    }
}

/// SetTimerQueueTimer (ancienne forme) : rend le handle du minuteur.
pub unsafe extern "win64" fn set_timer_queue_timer(
    queue: usize,
    callback: usize,
    parameter: usize,
    due: u32,
    period: u32,
    _prefer_io: i32,
) -> usize {
    create_timer(queue, callback, parameter, due, period, 0).unwrap_or(0)
}

/// ChangeTimerQueueTimer : nouvelle échéance et nouvelle période.
pub extern "win64" fn change_timer_queue_timer(
    _queue: usize,
    timer: usize,
    due: u32,
    period: u32,
) -> i32 {
    let Some((_, t)) = with_tables(|tb| tb.timers.get(&timer).cloned()) else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    let mut s = lock(&t.state);
    s.due = Some(Instant::now() + Duration::from_millis(due as u64));
    s.period = period;
    t.cv.notify_all();
    1
}

/// Annule `timers` puis, selon `completion` : INVALID_HANDLE_VALUE, attend
/// la fin des rappels en cours ; un événement, le signale quand ils sont
/// finis ; NULL, rend la main (ERROR_IO_PENDING si un rappel tourne).
fn cancel(timers: Vec<Arc<Timer>>, completion: usize) -> i32 {
    let mut running = Vec::new();
    for t in timers {
        let mut s = lock(&t.state);
        s.cancelled = true;
        t.cv.notify_all();
        if s.running {
            drop(s);
            running.push(t);
        }
    }
    if completion == INVALID_HANDLE_VALUE {
        for t in &running {
            t.wait_idle();
        }
        return 1;
    }
    if completion != 0 {
        if running.is_empty() {
            crate::dlls::kernel32::set_event(completion);
        } else {
            let signaled = super::kernel32_node::spawn_windows_thread(move || {
                for t in &running {
                    t.wait_idle();
                }
                crate::dlls::kernel32::set_event(completion);
            });
            if !signaled {
                crate::dlls::kernel32::set_event(completion);
            }
        }
        return 1;
    }
    if running.is_empty() {
        1
    } else {
        set_last_error(ERROR_IO_PENDING);
        0
    }
}

/// DeleteTimerQueueTimer(TimerQueue, Timer, CompletionEvent).
pub extern "win64" fn delete_timer_queue_timer(
    _queue: usize,
    timer: usize,
    completion: usize,
) -> i32 {
    let Some((_, t)) = with_tables(|tb| tb.timers.remove(&timer)) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    cancel(vec![t], completion)
}

/// CancelTimerQueueTimer (ancienne forme) : suppression sans attente.
pub extern "win64" fn cancel_timer_queue_timer(queue: usize, timer: usize) -> i32 {
    match delete_timer_queue_timer(queue, timer, 0) {
        0 if crate::process::last_error() == ERROR_IO_PENDING => 1,
        r => r,
    }
}

/// DeleteTimerQueueEx(TimerQueue, CompletionEvent) : supprime la file et
/// tous ses minuteurs.
pub extern "win64" fn delete_timer_queue_ex(queue: usize, completion: usize) -> i32 {
    let timers = with_tables(|tb| {
        tb.queues.remove(&queue)?;
        let ids: Vec<usize> = tb
            .timers
            .iter()
            .filter(|(_, (q, _))| *q == queue)
            .map(|(h, _)| *h)
            .collect();
        Some(
            ids.iter()
                .filter_map(|h| tb.timers.remove(h).map(|(_, t)| t))
                .collect::<Vec<_>>(),
        )
    });
    let Some(timers) = timers else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    cancel(timers, completion)
}

/// DeleteTimerQueue : DeleteTimerQueueEx sans événement de fin.
pub extern "win64" fn delete_timer_queue(queue: usize) -> i32 {
    match delete_timer_queue_ex(queue, 0) {
        0 if crate::process::last_error() == ERROR_IO_PENDING => 1,
        r => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    static FIRED: AtomicU32 = AtomicU32::new(0);
    static PARAM: AtomicUsize = AtomicUsize::new(0);

    extern "win64" fn tick(param: usize, fired: u8) {
        assert_eq!(fired, 1);
        PARAM.store(param, Ordering::SeqCst);
        FIRED.fetch_add(1, Ordering::SeqCst);
    }

    fn wait_for(n: u32) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if FIRED.load(Ordering::SeqCst) >= n {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn file_de_minuteurs_unique_periodique_et_suppression() {
        unsafe {
            let q = create_timer_queue();
            assert_ne!(q, 0);
            let mut h = 0usize;
            assert_eq!(
                create_timer_queue_timer(&mut h, q, 0, 0, 0, 0, 0),
                0,
                "rappel nul refusé"
            );
            // Unique, 10 ms.
            assert_eq!(
                create_timer_queue_timer(
                    &mut h,
                    q,
                    tick as usize,
                    0x1205,
                    10,
                    0,
                    WT_EXECUTEONLYONCE
                ),
                1
            );
            assert!(wait_for(1));
            assert_eq!(PARAM.load(Ordering::SeqCst), 0x1205);
            std::thread::sleep(Duration::from_millis(40));
            assert_eq!(FIRED.load(Ordering::SeqCst), 1, "un seul départ");
            assert_eq!(delete_timer_queue_timer(q, h, INVALID_HANDLE_VALUE), 1);
            // Périodique dans la file par défaut, puis échéance repoussée.
            let mut p = 0usize;
            assert_eq!(
                create_timer_queue_timer(&mut p, 0, tick as usize, 7, 0, 5, 0),
                1
            );
            assert!(wait_for(4), "le minuteur périodique repart");
            assert_eq!(change_timer_queue_timer(0, p, 60_000, 0), 1);
            std::thread::sleep(Duration::from_millis(30));
            let n = FIRED.load(Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            assert_eq!(FIRED.load(Ordering::SeqCst), n, "échéance repoussée");
            assert_eq!(delete_timer_queue_timer(0, p, INVALID_HANDLE_VALUE), 1);
            assert_eq!(delete_timer_queue_timer(0, p, 0), 0, "déjà supprimé");
            // DeleteTimerQueueEx emporte les minuteurs de la file.
            let mut r = 0usize;
            assert_eq!(
                create_timer_queue_timer(&mut r, q, tick as usize, 0, 60_000, 0, 0),
                1
            );
            assert_eq!(delete_timer_queue_ex(q, INVALID_HANDLE_VALUE), 1);
            assert_eq!(change_timer_queue_timer(q, r, 1, 0), 0);
            assert_eq!(delete_timer_queue(q), 0, "file déjà supprimée");
        }
    }
}
