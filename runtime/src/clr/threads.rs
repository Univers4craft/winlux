//! Fils managés (étape 5) : `Thread`, `Monitor` (`lock`), `ThreadPool`,
//! `Thread.Sleep`, `BeginInvoke`/`EndInvoke` des délégués.
//!
//! Chaque fil managé est un vrai fil du système avec son propre
//! interpréteur (sa pile d'appels), qui partage avec les autres les types,
//! les statiques et le tas. Un verrou global, pris à tour de rôle dans
//! l'ordre d'arrivée, garantit qu'un seul fil exécute du code managé à la
//! fois. Il change de main aux points sûrs (entre deux instructions, ou
//! jamais pendant un constructeur statique), et il est rendu pendant toute
//! attente (`Sleep`, `Join`, `Monitor`) et pendant les appels natifs
//! (P/Invoke), comme le mode préemptif du .NET Framework. Le
//! ramasse-miettes tourne donc toujours pendant que les autres fils sont
//! arrêtés à un point sûr.
//!
//! `Thread.Suspend` et `Thread.Resume` ne sont pas fournis : s'ils le
//! deviennent, ils passeront par `kernel32::suspend_thread` (#970).

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use super::callbacks;
use super::exceptions::ClrException;
use super::gc;
use super::il::{Interpreter, Value};
use super::metadata::ElementType;
use super::types;

// ── Verrou global ─────────────────────────────────────────────────────────

struct Gil {
    /// (prochain ticket, ticket servi)
    state: Mutex<(u64, u64)>,
    turn: Condvar,
}

static GIL: Gil = Gil {
    state: Mutex::new((0, 0)),
    turn: Condvar::new(),
};

thread_local! {
    /// Profondeur de prise du verrou global par ce fil.
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    /// Identifiant managé de ce fil (`ManagedThreadId`), 0 si inconnu.
    static MANAGED_ID: Cell<i32> = const { Cell::new(0) };
    /// Objet `Thread` de ce fil.
    static CURRENT_THREAD: Cell<usize> = const { Cell::new(0) };
}

/// Prend le verrou global (réentrant).
pub fn gil_acquire() {
    if DEPTH.with(|d| d.get()) > 0 {
        DEPTH.with(|d| d.set(d.get() + 1));
        return;
    }
    let mut st = GIL.state.lock().unwrap();
    let ticket = st.0;
    st.0 += 1;
    while st.1 != ticket {
        st = GIL.turn.wait(st).unwrap();
    }
    drop(st);
    DEPTH.with(|d| d.set(1));
}

/// Rend une prise du verrou global.
pub fn gil_release() {
    let depth = DEPTH.with(|d| d.get());
    if depth == 0 {
        return;
    }
    DEPTH.with(|d| d.set(depth - 1));
    if depth == 1 {
        let mut st = GIL.state.lock().unwrap();
        st.1 += 1;
        drop(st);
        GIL.turn.notify_all();
    }
}

/// Rend entièrement le verrou le temps d'une attente ; à passer ensuite à
/// [`gil_restore`].
pub fn gil_release_all() -> usize {
    let depth = DEPTH.with(|d| d.get());
    if depth > 0 {
        DEPTH.with(|d| d.set(1));
        gil_release();
    }
    depth
}

pub fn gil_restore(depth: usize) {
    if depth > 0 {
        gil_acquire();
        DEPTH.with(|d| d.set(depth));
    }
}

/// Passe la main si un autre fil attend.
pub fn gil_yield() {
    if DEPTH.with(|d| d.get()) == 0 {
        return;
    }
    let waiting = {
        let st = GIL.state.lock().unwrap();
        st.0 > st.1 + 1
    };
    if waiting {
        let depth = gil_release_all();
        gil_restore(depth);
    }
}

// ── Registre des interpréteurs (racines du ramasse-miettes) ───────────────

static INTERPRETERS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

pub fn register(interp: *const Interpreter) {
    INTERPRETERS.lock().unwrap().push(interp as usize);
}

/// Un interpréteur vivant du processus, modèle pour un fil natif qui entre
/// dans le runtime (#2383).
pub fn any_interpreter() -> Option<*const Interpreter> {
    INTERPRETERS
        .lock()
        .unwrap()
        .first()
        .map(|&p| p as *const Interpreter)
}

pub fn unregister(interp: *const Interpreter) {
    INTERPRETERS
        .lock()
        .unwrap()
        .retain(|&p| p != interp as usize);
}

/// Parcourt les interpréteurs des fils vivants. À n'appeler qu'avec le
/// verrou global : les autres fils sont alors arrêtés à un point sûr.
pub fn for_each_interpreter(mut f: impl FnMut(&Interpreter)) {
    let list = INTERPRETERS.lock().unwrap().clone();
    for p in list {
        // SAFETY : un interpréteur se retire du registre avant d'être
        // détruit, et son fil ne le modifie pas sans le verrou global.
        f(unsafe { &*(p as *const Interpreter) });
    }
}

// ── État des fils et des appels asynchrones ───────────────────────────────

/// Mots de l'objet `System.Threading.Thread` (après le mot réservé).
const T_START: usize = 8;
const T_ID: usize = 16;
const T_BACKGROUND: usize = 24;
const T_NAME: usize = 32;

#[derive(Default)]
struct ThreadState {
    started: bool,
    done: bool,
}

#[derive(Default)]
struct AsyncState {
    done: bool,
    result: Option<Value>,
    error: Option<ClrException>,
}

#[derive(Default)]
struct Registry {
    threads: HashMap<usize, ThreadState>,
    asyncs: HashMap<usize, AsyncState>,
    /// Délégués et arguments des fils lancés, gardés en vie.
    held: Vec<usize>,
    next_id: i32,
}

// SAFETY : les pointeurs gardés ne sont lus que sous le verrou global.
unsafe impl Send for Registry {}

static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);
static CHANGED: Condvar = Condvar::new();

fn with_registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    let mut lock = REGISTRY.lock().unwrap();
    f(lock.get_or_insert_with(Registry::default))
}

/// Remet l'état à zéro au lancement d'une assembly ; le fil appelant
/// devient le fil principal (identifiant 1) et prend le verrou global.
pub fn init() {
    *REGISTRY.lock().unwrap() = Some(Registry {
        next_id: 2,
        ..Registry::default()
    });
    MONITORS.lock().unwrap().clear();
    MANAGED_ID.with(|m| m.set(1));
    CURRENT_THREAD.with(|c| c.set(0));
}

/// Racines gardées par ce module : objets `Thread`, délégués lancés,
/// résultats asynchrones pas encore lus.
pub fn roots() -> Vec<usize> {
    with_registry(|r| {
        let mut v: Vec<usize> = r.threads.keys().copied().collect();
        v.extend(r.asyncs.keys().copied());
        v.extend(r.asyncs.values().filter_map(|a| match a.result {
            Some(Value::Ref(p)) => Some(p as usize),
            _ => None,
        }));
        v.extend(
            r.asyncs
                .values()
                .filter_map(|a| a.error.map(|e| e.obj_ptr as usize)),
        );
        v.extend(r.held.iter().copied());
        v
    })
}

/// Attend la fin des fils de premier plan, comme le fait le .NET Framework
/// avant de terminer le processus.
pub fn wait_foreground() {
    let depth = gil_release_all();
    let mut lock = REGISTRY.lock().unwrap();
    loop {
        let pending = lock.as_ref().is_some_and(|r| {
            r.threads.iter().any(|(&obj, s)| {
                s.started && !s.done && unsafe { *((obj + T_BACKGROUND) as *const u64) } == 0
            })
        });
        if !pending {
            break;
        }
        lock = CHANGED.wait(lock).unwrap();
    }
    drop(lock);
    gil_restore(depth);
}

fn managed_id() -> i32 {
    MANAGED_ID.with(|m| {
        if m.get() == 0 {
            let id = with_registry(|r| {
                r.next_id += 1;
                r.next_id - 1
            });
            m.set(id);
        }
        m.get()
    })
}

fn read_word(obj: *mut u8, off: usize) -> u64 {
    unsafe { *(obj.add(off) as *const u64) }
}

fn write_word(obj: *mut u8, off: usize, v: u64) {
    unsafe { *(obj.add(off) as *mut u64) = v }
}

fn timeout_of(ms: i32) -> Option<Duration> {
    (ms >= 0).then(|| Duration::from_millis(ms as u64))
}

fn exception(name: &str, message: &str) -> ClrException {
    ClrException::new("System.Threading", name, message)
}

// ── Monitor ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct MonitorState {
    owner: i32,
    count: u32,
    waiters: u32,
    pulses: u32,
}

static MONITORS: Mutex<Vec<(usize, MonitorState)>> = Mutex::new(Vec::new());
static MONITOR_CHANGED: Condvar = Condvar::new();

fn monitor(list: &mut Vec<(usize, MonitorState)>, obj: usize) -> &mut MonitorState {
    let i = match list.iter().position(|(o, _)| *o == obj) {
        Some(i) => i,
        None => {
            list.push((obj, MonitorState::default()));
            list.len() - 1
        }
    };
    &mut list[i].1
}

fn forget_idle(list: &mut Vec<(usize, MonitorState)>) {
    list.retain(|(_, m)| m.owner != 0 || m.waiters != 0);
}

/// Prend le moniteur de `obj` ; faux si le délai expire.
fn monitor_enter(obj: usize, timeout: Option<Duration>) -> bool {
    let me = managed_id();
    {
        let mut list = MONITORS.lock().unwrap();
        let m = monitor(&mut list, obj);
        if m.owner == 0 || m.owner == me {
            m.owner = me;
            m.count += 1;
            return true;
        }
    }
    if timeout == Some(Duration::ZERO) {
        return false;
    }
    let deadline = timeout.map(|t| Instant::now() + t);
    let depth = gil_release_all();
    let mut list = MONITORS.lock().unwrap();
    let taken = loop {
        let m = monitor(&mut list, obj);
        if m.owner == 0 {
            m.owner = me;
            m.count = 1;
            break true;
        }
        match deadline {
            None => list = MONITOR_CHANGED.wait(list).unwrap(),
            Some(d) => {
                let now = Instant::now();
                if now >= d {
                    break false;
                }
                list = MONITOR_CHANGED.wait_timeout(list, d - now).unwrap().0;
            }
        }
    };
    forget_idle(&mut list);
    drop(list);
    gil_restore(depth);
    taken
}

fn monitor_exit(obj: usize) -> Result<(), ClrException> {
    let me = managed_id();
    let mut list = MONITORS.lock().unwrap();
    let m = monitor(&mut list, obj);
    if m.owner != me {
        forget_idle(&mut list);
        return Err(exception(
            "SynchronizationLockException",
            "Object synchronization method was called from an unsynchronized block of code.",
        ));
    }
    m.count -= 1;
    if m.count == 0 {
        m.owner = 0;
    }
    forget_idle(&mut list);
    drop(list);
    MONITOR_CHANGED.notify_all();
    Ok(())
}

/// `Monitor.Wait` : rend le moniteur, attend un `Pulse`, le reprend.
fn monitor_wait(obj: usize, timeout: Option<Duration>) -> Result<bool, ClrException> {
    let me = managed_id();
    let saved = {
        let mut list = MONITORS.lock().unwrap();
        let m = monitor(&mut list, obj);
        if m.owner != me {
            forget_idle(&mut list);
            return Err(exception(
                "SynchronizationLockException",
                "Object synchronization method was called from an unsynchronized block of code.",
            ));
        }
        let saved = m.count;
        m.owner = 0;
        m.count = 0;
        m.waiters += 1;
        saved
    };
    MONITOR_CHANGED.notify_all();
    let deadline = timeout.map(|t| Instant::now() + t);
    let depth = gil_release_all();
    let mut list = MONITORS.lock().unwrap();
    let signaled = loop {
        let m = monitor(&mut list, obj);
        if m.pulses > 0 {
            m.pulses -= 1;
            break true;
        }
        match deadline {
            None => list = MONITOR_CHANGED.wait(list).unwrap(),
            Some(d) => {
                let now = Instant::now();
                if now >= d {
                    break false;
                }
                list = MONITOR_CHANGED.wait_timeout(list, d - now).unwrap().0;
            }
        }
    };
    monitor(&mut list, obj).waiters -= 1;
    loop {
        let m = monitor(&mut list, obj);
        if m.owner == 0 {
            m.owner = me;
            m.count = saved;
            break;
        }
        list = MONITOR_CHANGED.wait(list).unwrap();
    }
    drop(list);
    gil_restore(depth);
    Ok(signaled)
}

fn monitor_pulse(obj: usize, all: bool) -> Result<(), ClrException> {
    let me = managed_id();
    let mut list = MONITORS.lock().unwrap();
    let m = monitor(&mut list, obj);
    if m.owner != me {
        forget_idle(&mut list);
        return Err(exception(
            "SynchronizationLockException",
            "Object synchronization method was called from an unsynchronized block of code.",
        ));
    }
    if all {
        m.pulses = m.waiters;
    } else if m.waiters > m.pulses {
        m.pulses += 1;
    }
    drop(list);
    MONITOR_CHANGED.notify_all();
    Ok(())
}

// ── Lancement d'un fil ────────────────────────────────────────────────────

struct Launch {
    interp: Box<Interpreter>,
    run: Box<dyn FnOnce(&mut Interpreter)>,
    with_teb: bool,
}

// SAFETY : l'interpréteur et les valeurs capturées ne sont utilisés que
// par le nouveau fil, sous le verrou global.
unsafe impl Send for Launch {}

fn sleep_released(d: Duration) {
    let depth = gil_release_all();
    std::thread::sleep(d);
    gil_restore(depth);
}

impl Interpreter {
    /// Lance `run` sur un nouveau fil, avec son propre interpréteur.
    fn launch(&self, run: Box<dyn FnOnce(&mut Interpreter)>) {
        let launch = Launch {
            interp: Box::new(self.fork()),
            run,
            with_teb: crate::process::teb() != 0,
        };
        std::thread::spawn(move || {
            let launch = launch;
            let Launch {
                mut interp,
                run,
                with_teb,
            } = launch;
            if with_teb && crate::process::teb() == 0 {
                crate::process::init_thread();
            }
            gil_acquire();
            let p: *mut Interpreter = &mut *interp;
            register(p);
            let previous = callbacks::set_current(p);
            run(&mut interp);
            callbacks::set_current(previous);
            unregister(p);
            drop(interp);
            gil_release();
        });
    }

    /// Fin d'un fil sur une exception non gérée : comme le .NET Framework,
    /// le processus s'arrête.
    /// AppDomain.UnhandledException est levé d'abord (#2058).
    fn thread_failed(&mut self, e: ClrException) -> ! {
        self.raise_domain_event("LeverExceptionNonGeree", vec![Value::Ref(e.obj_ptr)]);
        eprintln!("Unhandled exception. {e}");
        crate::log_error!("clr", "exception non gérée dans un fil : {e}");
        std::process::exit(0xE0434352u32 as i32)
    }

    fn start_thread(&mut self, obj: *mut u8, arg: Option<Value>) -> Result<(), ClrException> {
        let key = obj as usize;
        let already = with_registry(|r| {
            let s = r.threads.entry(key).or_default();
            let was = s.started;
            s.started = true;
            if let Some(Value::Ref(a)) = arg {
                r.held.push(a as usize);
            }
            was
        });
        if already {
            return Err(exception(
                "ThreadStateException",
                "Thread is running or terminated; it cannot restart.",
            ));
        }
        let delegate = read_word(obj, T_START) as *mut u8;
        let id = read_word(obj, T_ID) as i32;
        self.launch(Box::new(move |interp| {
            MANAGED_ID.with(|m| m.set(id));
            CURRENT_THREAD.with(|c| c.set(obj as usize));
            let args = arg.map(|a| vec![a]).unwrap_or_default();
            if let Err(e) = interp.invoke_delegate(delegate, args) {
                interp.thread_failed(e);
            }
            with_registry(|r| {
                if let Some(s) = r.threads.get_mut(&key) {
                    s.done = true;
                }
                if let Some(Value::Ref(a)) = arg {
                    if let Some(i) = r.held.iter().position(|&h| h == a as usize) {
                        r.held.remove(i);
                    }
                }
            });
            CHANGED.notify_all();
        }));
        Ok(())
    }

    /// Lance `delegate(args)` sur un fil d'arrière-plan (ThreadPool,
    /// BeginInvoke) ; `done` reçoit le résultat.
    fn run_in_background(
        &mut self,
        delegate: *mut u8,
        args: Vec<Value>,
        async_obj: Option<usize>,
        callback: Option<*mut u8>,
    ) {
        with_registry(|r| {
            r.held.push(delegate as usize);
            r.held.extend(args.iter().filter_map(|a| match a {
                Value::Ref(p) => Some(*p as usize),
                _ => None,
            }));
            if let Some(cb) = callback {
                r.held.push(cb as usize);
            }
        });
        let held: Vec<usize> = std::iter::once(delegate as usize)
            .chain(args.iter().filter_map(|a| match a {
                Value::Ref(p) => Some(*p as usize),
                _ => None,
            }))
            .chain(callback.map(|c| c as usize))
            .collect();
        self.launch(Box::new(move |interp| {
            managed_id();
            let result = interp.invoke_delegate(delegate, args);
            match async_obj {
                Some(ar) => {
                    with_registry(|r| {
                        let a = r.asyncs.entry(ar).or_default();
                        match result {
                            Ok(v) => a.result = Some(v),
                            Err(e) => a.error = Some(e),
                        }
                        a.done = true;
                    });
                    CHANGED.notify_all();
                    if let Some(cb) = callback {
                        if let Err(e) = interp.invoke_delegate(cb, vec![Value::Ref(ar as *mut u8)])
                        {
                            interp.thread_failed(e);
                        }
                    }
                }
                None => {
                    if let Err(e) = result {
                        interp.thread_failed(e);
                    }
                }
            }
            with_registry(|r| {
                for h in &held {
                    if let Some(i) = r.held.iter().position(|x| x == h) {
                        r.held.remove(i);
                    }
                }
            });
        }));
    }

    /// Objet `Thread` du fil courant (créé à la demande).
    fn current_thread_object(&mut self) -> *mut u8 {
        let existing = CURRENT_THREAD.with(|c| c.get());
        if existing != 0 {
            return existing as *mut u8;
        }
        let id = types::with_types(|reg| reg.ensure_external("System.Threading", "Thread"));
        let obj = gc::with_gc(|gc| gc.alloc(id, types::EXTERNAL_OBJECT_SIZE));
        write_word(obj, T_ID, managed_id() as u64);
        // Gardé en vie, mais pas compté parmi les fils de premier plan à
        // attendre : c'est le fil qui attend.
        with_registry(|r| r.held.push(obj as usize));
        CURRENT_THREAD.with(|c| c.set(obj as usize));
        obj
    }

    /// Méthodes de `System.Threading` fournies par le runtime. `None` :
    /// l'appel ne relève pas de ce module.
    pub(super) fn thread_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
        params: &[ElementType],
    ) -> Option<Result<Value, ClrException>> {
        let this = match args.first() {
            Some(Value::Ref(p)) => *p,
            _ => std::ptr::null_mut(),
        };
        let int_arg = |i: usize| args.get(i).map_or(-1, |v| v.as_i32());
        // Les valeurs en transit restent visibles du ramasse-miettes pendant
        // que le verrou global est rendu.
        let pinned = self.pins.len();
        self.pins.extend(args.iter().copied());
        let r = match (namespace, class, name) {
            ("System.Threading", "Thread", ".ctor") => {
                write_word(this, T_START, args.get(1).map_or(0, |v| v.as_i64() as u64));
                let id = with_registry(|r| {
                    r.next_id += 1;
                    r.next_id - 1
                });
                write_word(this, T_ID, id as u64);
                with_registry(|r| {
                    r.threads.insert(this as usize, ThreadState::default());
                });
                Some(Ok(Value::Null))
            }
            ("System.Threading", "Thread", "Start") => Some(
                self.start_thread(this, args.get(1).copied())
                    .map(|_| Value::Null),
            ),
            ("System.Threading", "Thread", "Join") => {
                let timeout = if args.len() > 1 {
                    timeout_of(int_arg(1))
                } else {
                    None
                };
                let key = this as usize;
                let deadline = timeout.map(|t| Instant::now() + t);
                let depth = gil_release_all();
                let mut lock = REGISTRY.lock().unwrap();
                let finished = loop {
                    let state = lock
                        .as_ref()
                        .and_then(|r| r.threads.get(&key))
                        .map(|s| (s.started, s.done));
                    match state {
                        Some((true, true)) | None => break true,
                        Some((false, _)) => break true,
                        _ => {}
                    }
                    match deadline {
                        None => lock = CHANGED.wait(lock).unwrap(),
                        Some(d) => {
                            let now = Instant::now();
                            if now >= d {
                                break false;
                            }
                            lock = CHANGED.wait_timeout(lock, d - now).unwrap().0;
                        }
                    }
                };
                drop(lock);
                gil_restore(depth);
                Some(Ok(if args.len() > 1 {
                    Value::I32(finished as i32)
                } else {
                    Value::Null
                }))
            }
            ("System.Threading", "Thread", "Sleep") => {
                let ms = int_arg(0);
                if ms == 0 {
                    gil_yield();
                } else if let Some(d) = timeout_of(ms) {
                    sleep_released(d);
                } else {
                    // Sleep(Infinite) : le fil ne se réveille jamais.
                    loop {
                        sleep_released(Duration::from_secs(3600));
                    }
                }
                Some(Ok(Value::Null))
            }
            ("System.Threading", "Thread", "Yield") => {
                gil_yield();
                Some(Ok(Value::I32(1)))
            }
            ("System.Threading", "Thread", "SpinWait") => {
                gil_yield();
                Some(Ok(Value::Null))
            }
            ("System.Threading", "Thread", "get_CurrentThread") => {
                Some(Ok(Value::Ref(self.current_thread_object())))
            }
            ("System.Threading", "Thread", "get_ManagedThreadId") => {
                Some(Ok(Value::I32(read_word(this, T_ID) as i32)))
            }
            ("System.Threading", "Thread", "get_IsBackground") => {
                Some(Ok(Value::I32((read_word(this, T_BACKGROUND) != 0) as i32)))
            }
            ("System.Threading", "Thread", "set_IsBackground") => {
                write_word(this, T_BACKGROUND, (int_arg(1) != 0) as u64);
                CHANGED.notify_all();
                Some(Ok(Value::Null))
            }
            ("System.Threading", "Thread", "get_IsAlive") => {
                let alive = with_registry(|r| {
                    r.threads
                        .get(&(this as usize))
                        .is_some_and(|s| s.started && !s.done)
                });
                Some(Ok(Value::I32(alive as i32)))
            }
            ("System.Threading", "Thread", "get_Name") => {
                let n = read_word(this, T_NAME) as *mut u8;
                Some(Ok(if n.is_null() {
                    Value::Null
                } else {
                    Value::Ref(n)
                }))
            }
            ("System.Threading", "Thread", "set_Name") => {
                write_word(this, T_NAME, args.get(1).map_or(0, |v| v.as_i64() as u64));
                Some(Ok(Value::Null))
            }
            ("System.Threading", "Thread", "get_CurrentManagedThreadId")
            | ("System", "Environment", "get_CurrentManagedThreadId") => {
                Some(Ok(Value::I32(managed_id())))
            }
            // Mutex et Semaphore de la mscorlib (#1988) : objets de kernel32,
            // nommés et partagés avec les processus du compte (#20). Un
            // handle rendu négatif est l'opposé du code d'erreur ; à la
            // création, le bit 0 dit si l'objet est neuf.
            ("System.Threading", "Mutex" | "Semaphore", "Creer") => {
                let semaphore = class == "Semaphore";
                let name_at = if semaphore { 2 } else { 1 };
                let name = match args.get(name_at) {
                    Some(Value::Ref(p)) if !p.is_null() => {
                        Some(super::bcl::read_managed_string(*p))
                    }
                    _ => None,
                };
                let (h, err) = if semaphore {
                    crate::dlls::kernel32::clr_semaphore_create(
                        int_arg(0),
                        int_arg(1),
                        name.as_deref(),
                    )
                } else {
                    crate::dlls::kernel32::clr_mutex_create(int_arg(0) != 0, name.as_deref())
                };
                Some(Ok(Value::I64(if h == 0 {
                    -(err.max(1) as i64)
                } else {
                    ((h as i64) << 1) | i64::from(err != 183)
                })))
            }
            ("System.Threading", "Mutex" | "Semaphore", "Ouvrir") => {
                let name = if this.is_null() {
                    String::new()
                } else {
                    super::bcl::read_managed_string(this)
                };
                let (h, err) = crate::dlls::kernel32::clr_sync_open(&name, class == "Semaphore");
                Some(Ok(Value::I64(if h == 0 {
                    -(err.max(1) as i64)
                } else {
                    h as i64
                })))
            }
            ("System.Threading", "Mutex", "Liberer") => {
                let h = args.first().map_or(0, |v| v.as_i64()) as usize;
                Some(Ok(Value::I32(crate::dlls::kernel32::release_mutex(h))))
            }
            ("System.Threading", "Semaphore", "Liberer") => {
                let h = args.first().map_or(0, |v| v.as_i64()) as usize;
                let mut prev = 0i32;
                let ok =
                    unsafe { crate::dlls::kernel32::release_semaphore(h, int_arg(1), &mut prev) };
                Some(Ok(Value::I64(if ok != 0 {
                    prev as i64
                } else {
                    -(crate::process::last_error().max(1) as i64)
                })))
            }
            ("System.Threading", "WaitHandle", "AttendreNoyau") => {
                let h = args.first().map_or(0, |v| v.as_i64()) as usize;
                let ms = int_arg(1);
                let depth = gil_release_all();
                let r = crate::dlls::kernel32::wait_for_single_object(h, ms as u32);
                gil_restore(depth);
                Some(Ok(Value::I32(r as i32)))
            }
            ("System.Threading", "WaitHandle", "FermerNoyau") => {
                let h = args.first().map_or(0, |v| v.as_i64()) as usize;
                crate::dlls::kernel32::close_handle(h);
                Some(Ok(Value::Null))
            }
            ("System.Threading", "ThreadPool", "QueueUserWorkItem") => {
                let cb = this;
                if cb.is_null() {
                    Some(Err(ClrException::new(
                        "System",
                        "ArgumentNullException",
                        "Value cannot be null.\r\nParameter name: callBack",
                    )))
                } else {
                    let state = args.get(1).copied().unwrap_or(Value::Null);
                    self.run_in_background(cb, vec![state], None, None);
                    Some(Ok(Value::I32(1)))
                }
            }
            ("System.Threading", "Monitor", "Enter" | "ReliableEnter") => {
                if this.is_null() {
                    Some(Err(ClrException::null_reference()))
                } else {
                    monitor_enter(this as usize, None);
                    if let Some(Value::Ref(taken)) = args.get(1) {
                        unsafe { **taken = 1 };
                    }
                    Some(Ok(Value::Null))
                }
            }
            ("System.Threading", "Monitor", "TryEnter") => {
                if this.is_null() {
                    Some(Err(ClrException::null_reference()))
                } else {
                    let timeout = match params.get(1) {
                        Some(ElementType::I4) => timeout_of(int_arg(1)),
                        _ => Some(Duration::ZERO),
                    };
                    let taken = monitor_enter(this as usize, timeout);
                    match params.last() {
                        Some(ElementType::ByRef(_)) => {
                            if let Some(Value::Ref(p)) = args.last() {
                                unsafe { **p = taken as u8 };
                            }
                            Some(Ok(Value::Null))
                        }
                        _ => Some(Ok(Value::I32(taken as i32))),
                    }
                }
            }
            ("System.Threading", "Monitor", "Exit") => {
                Some(monitor_exit(this as usize).map(|_| Value::Null))
            }
            ("System.Threading", "Monitor", "IsEntered") => {
                let me = managed_id();
                let mut list = MONITORS.lock().unwrap();
                let owned = monitor(&mut list, this as usize).owner == me;
                forget_idle(&mut list);
                Some(Ok(Value::I32(owned as i32)))
            }
            ("System.Threading", "Monitor", "Wait") => {
                let timeout = if args.len() > 1 {
                    timeout_of(int_arg(1))
                } else {
                    None
                };
                Some(monitor_wait(this as usize, timeout).map(|b| Value::I32(b as i32)))
            }
            ("System.Threading", "Monitor", "Pulse") => {
                Some(monitor_pulse(this as usize, false).map(|_| Value::Null))
            }
            ("System.Threading", "Monitor", "PulseAll") => {
                Some(monitor_pulse(this as usize, true).map(|_| Value::Null))
            }
            // BeginInvoke(args…, AsyncCallback, object) d'un délégué.
            (_, _, "BeginInvoke") if gc::is_delegate(this) && args.len() >= 3 => {
                let n = args.len();
                let callback = match args[n - 2] {
                    Value::Ref(p) => Some(p),
                    _ => None,
                };
                let state = args[n - 1];
                let id = types::with_types(|reg| {
                    reg.ensure_external("System.Runtime.Remoting.Messaging", "AsyncResult")
                });
                let ar = gc::with_gc(|gc| gc.alloc(id, types::EXTERNAL_OBJECT_SIZE));
                write_word(ar, 8, state.as_i64() as u64);
                write_word(ar, 16, this as u64);
                with_registry(|r| {
                    r.asyncs.insert(ar as usize, AsyncState::default());
                });
                self.run_in_background(this, args[1..n - 2].to_vec(), Some(ar as usize), callback);
                Some(Ok(Value::Ref(ar)))
            }
            (_, _, "EndInvoke") if gc::is_delegate(this) => {
                let ar = args.last().map_or(0, |v| v.as_i64() as usize);
                let depth = gil_release_all();
                let mut lock = REGISTRY.lock().unwrap();
                let outcome = loop {
                    let entry = lock.as_mut().and_then(|r| {
                        let done = r.asyncs.get(&ar).is_some_and(|a| a.done);
                        done.then(|| r.asyncs.remove(&ar)).flatten()
                    });
                    if let Some(a) = entry {
                        break Some(a);
                    }
                    if lock.as_ref().is_some_and(|r| !r.asyncs.contains_key(&ar)) {
                        break None;
                    }
                    lock = CHANGED.wait(lock).unwrap();
                };
                drop(lock);
                gil_restore(depth);
                Some(match outcome {
                    Some(AsyncState { error: Some(e), .. }) => Err(e),
                    Some(a) => Ok(a.result.unwrap_or(Value::Null)),
                    None => Err(ClrException::new(
                        "System",
                        "InvalidOperationException",
                        "EndInvoke can only be called once for each asynchronous operation.",
                    )),
                })
            }
            ("System.Runtime.Remoting.Messaging", "AsyncResult", "get_AsyncState")
            | ("System", "IAsyncResult", "get_AsyncState") => {
                let s = read_word(this, 8) as *mut u8;
                Some(Ok(if s.is_null() {
                    Value::Null
                } else {
                    Value::Ref(s)
                }))
            }
            ("System.Runtime.Remoting.Messaging", "AsyncResult", "get_IsCompleted")
            | ("System", "IAsyncResult", "get_IsCompleted") => {
                let done = with_registry(|r| r.asyncs.get(&(this as usize)).is_none_or(|a| a.done));
                Some(Ok(Value::I32(done as i32)))
            }
            ("System.Runtime.Remoting.Messaging", "AsyncResult", "get_CompletedSynchronously")
            | ("System", "IAsyncResult", "get_CompletedSynchronously") => Some(Ok(Value::I32(0))),
            _ => None,
        };
        self.pins.truncate(pinned);
        r
    }
}
