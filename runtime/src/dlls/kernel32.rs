use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::ThreadId;

static PROCESS_HEAP: AtomicUsize = AtomicUsize::new(0);
struct VmRegion {
    size: usize,
    /// Protection Windows de chaque page, 0 pour une page réservée mais non
    /// engagée.
    pages: PageMap,
    allocation_protection: u32,
    kind: u32,
}

/// Protections des pages d'une région, rangées par plages contiguës de même
/// valeur : le ramasse-miettes de .NET réserve 256 Gio d'un coup, soit 67
/// millions de pages qu'un tableau par page faisait payer ~335 Mio de RAM
/// (#665).
#[derive(Clone)]
pub(crate) struct PageMap {
    /// Début de chaque plage -> valeur ; plages voisines toujours distinctes.
    runs: std::collections::BTreeMap<usize, u32>,
    len: usize,
}

impl PageMap {
    pub(crate) fn new(len: usize, value: u32) -> Self {
        let mut runs = std::collections::BTreeMap::new();
        runs.insert(0, value);
        PageMap { runs, len }
    }
    fn from_pages(values: &[u32]) -> Self {
        let mut runs = std::collections::BTreeMap::new();
        let mut last = None;
        for (i, &v) in values.iter().enumerate() {
            if last != Some(v) {
                runs.insert(i, v);
                last = Some(v);
            }
        }
        if runs.is_empty() {
            runs.insert(0, 0);
        }
        PageMap {
            runs,
            len: values.len(),
        }
    }
    pub(crate) fn get(&self, page: usize) -> u32 {
        self.runs.range(..=page).next_back().map_or(0, |(_, &v)| v)
    }
    fn committed(&self, page: usize) -> bool {
        self.get(page) != 0
    }
    /// Toutes les pages de [first, first + count) sont-elles engagées ?
    fn all_committed(&self, first: usize, count: usize) -> bool {
        if count == 0 {
            return true;
        }
        self.get(first) != 0
            && self
                .runs
                .range(first + 1..first + count)
                .all(|(_, &v)| v != 0)
    }
    /// Donne la valeur `value` aux pages [first, first + count).
    pub(crate) fn fill(&mut self, first: usize, count: usize, value: u32) {
        let end = (first + count).min(self.len);
        if first >= end {
            return;
        }
        let after = self.get(end);
        let inner: Vec<usize> = self.runs.range(first..end).map(|(&k, _)| k).collect();
        for k in inner {
            self.runs.remove(&k);
        }
        if end < self.len {
            self.runs.insert(end, after);
        }
        self.runs.insert(first, value);
        // Fusion avec les voisines de même valeur.
        if end < self.len && after == value {
            self.runs.remove(&end);
        }
        if first > 0 && self.get(first - 1) == value {
            self.runs.remove(&first);
        }
    }
    /// Plage [début, fin) de même valeur qui contient `page`.
    fn run_bounds(&self, page: usize) -> (usize, usize) {
        let start = self.runs.range(..=page).next_back().map_or(0, |(&k, _)| k);
        let end = self
            .runs
            .range(page + 1..)
            .next()
            .map_or(self.len, |(&k, _)| k);
        (start, end)
    }
}

/// PE mappings belong to the loader, but must be visible to the Windows VM APIs.
pub(crate) fn register_image(base: usize, protection: Vec<u32>) {
    let pages = protection.len();
    allocs().insert(
        base,
        VmRegion {
            size: pages * 4096,
            pages: PageMap::from_pages(&protection),
            allocation_protection: 0x80,
            kind: 0x1000000,
        },
    );
}
static ALLOCS: OnceLock<Mutex<HashMap<usize, VmRegion>>> = OnceLock::new();

/// Table des réservations VirtualAlloc, verrouillée : le thread qui la
/// tient ne peut pas être arrêté par SuspendThread (#2065), sinon le
/// thread qui suspend resterait bloqué dans son `VirtualQuery` suivant.
/// Les champs se libèrent dans l'ordre : le verrou, puis la garde.
struct AllocsGuard {
    maps: std::sync::MutexGuard<'static, HashMap<usize, VmRegion>>,
    _hold: super::thread_ctl::NoPark,
}

impl std::ops::Deref for AllocsGuard {
    type Target = HashMap<usize, VmRegion>;
    fn deref(&self) -> &Self::Target {
        &self.maps
    }
}

impl std::ops::DerefMut for AllocsGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.maps
    }
}

fn allocs() -> AllocsGuard {
    let hold = super::thread_ctl::no_park();
    AllocsGuard {
        maps: lock(ALLOCS.get_or_init(|| Mutex::new(HashMap::new()))),
        _hold: hold,
    }
}
static HEAP_ALLOCS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
pub(crate) struct GlobalMemory {
    pub(crate) bytes: Vec<u8>,
    locks: u32,
    flags: u32,
}
static GLOBAL_MEMORY: OnceLock<Mutex<HashMap<usize, Arc<Mutex<GlobalMemory>>>>> = OnceLock::new();
enum FileHandle {
    File(Arc<Mutex<File>>),
}
static HANDLES: OnceLock<Mutex<HashMap<usize, FileHandle>>> = OnceLock::new();
/// Handles ouverts avec FILE_FLAG_OVERLAPPED : seuls ceux-là rendent
/// ReadFile/WriteFile asynchrones quand un OVERLAPPED est fourni.
static OVERLAPPED_HANDLES: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
fn overlapped_handles() -> &'static Mutex<HashSet<usize>> {
    OVERLAPPED_HANDLES.get_or_init(|| Mutex::new(HashSet::new()))
}
pub(crate) fn mark_overlapped_handle(h: usize) {
    lock(overlapped_handles()).insert(h);
}
pub(crate) fn is_overlapped_handle(h: usize) -> bool {
    lock(overlapped_handles()).contains(&h)
}
struct IoResult {
    done: bool,
    bytes: u32,
    error: u32,
}
struct IoOperation {
    file_handle: usize,
    issuing_thread: ThreadId,
    state: Mutex<IoResult>,
    changed: Condvar,
    cancelled: std::sync::atomic::AtomicBool,
}
static IO_OPERATIONS: OnceLock<Mutex<HashMap<usize, Arc<IoOperation>>>> = OnceLock::new();
#[derive(Clone, Copy)]
struct CompletionPacket {
    bytes: u32,
    key: usize,
    overlapped: usize,
    error: u32,
}
struct CompletionPortState {
    queue: VecDeque<CompletionPacket>,
    /// Fils qui ont retiré un paquet, avec leur numéro de fil Linux.
    active: HashMap<ThreadId, i32>,
}
pub(crate) struct CompletionPort {
    state: Mutex<CompletionPortState>,
    changed: Condvar,
    max_concurrency: usize,
}
static IOCP_ASSOCIATIONS: OnceLock<Mutex<HashMap<usize, (Arc<CompletionPort>, usize)>>> =
    OnceLock::new();
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x1_0000);
static TEMP_FILE_SEQUENCE: AtomicU32 = AtomicU32::new(1);

/// Sémaphore à compteur (CreateSemaphore) : (compteur, maximum).
pub(crate) struct SemaphoreObject {
    count: Mutex<(i32, i32)>,
    changed: Condvar,
    /// Sémaphore nommé partagé avec d'autres processus (#20) : compteur et
    /// maximum dans la page partagée, qui fait foi à la place de `count`.
    shared: std::sync::OnceLock<crate::dlls::inherit::SharedWord>,
}
pub(crate) struct EventObject {
    manual_reset: bool,
    signaled: Mutex<bool>,
    changed: Condvar,
    /// État partagé avec d'autres processus (événement hérité, voir
    /// `inherit`) : quand il est là, il fait foi à la place de `signaled`.
    shared: std::sync::OnceLock<crate::dlls::inherit::SharedWord>,
}
impl EventObject {
    fn new(manual_reset: bool, initial: bool) -> EventObject {
        EventObject {
            manual_reset,
            signaled: Mutex::new(initial),
            changed: Condvar::new(),
            shared: std::sync::OnceLock::new(),
        }
    }
    /// Signalé ? (sans le consommer)
    fn is_set(&self, local: &bool) -> bool {
        match self.shared.get() {
            Some(w) => w.get() != 0,
            None => *local,
        }
    }
    fn store(&self, local: &mut bool, v: bool) {
        match self.shared.get() {
            Some(w) => w.set(v),
            None => *local = v,
        }
    }
    /// Prend le signal (remis à zéro pour un événement automatique).
    fn take(&self, local: &mut bool) -> bool {
        match self.shared.get() {
            Some(w) if self.manual_reset => w.get() != 0,
            Some(w) => w.take(),
            None => {
                let was = *local;
                if was && !self.manual_reset {
                    *local = false;
                }
                was
            }
        }
    }
    /// Rend l'événement partageable avec un processus enfant : descripteur
    /// de la page partagée qui porte son état.
    pub(crate) fn share(&self) -> Option<i32> {
        let _gate = sync_gate();
        let local = lock(&self.signaled);
        if self.shared.get().is_none() {
            let w = crate::dlls::inherit::SharedWord::create(*local)?;
            let _ = self.shared.set(w);
        }
        self.shared.get().map(|w| w.fd())
    }
    /// Événement dont l'état vit dans une page partagée héritée.
    pub(crate) fn adopt(manual_reset: bool, word: crate::dlls::inherit::SharedWord) -> SyncHandle {
        let e = EventObject::new(manual_reset, false);
        let _ = e.shared.set(word);
        SyncHandle::Event(Arc::new(e))
    }
}
struct MutexState {
    owner: Option<ThreadId>,
    /// Tid Linux du propriétaire, pour passer le mutex dans une page
    /// partagée (DuplicateHandle vers un autre processus, #767).
    owner_os: u32,
    recursion: usize,
}
pub(crate) struct MutexObject {
    state: Mutex<MutexState>,
    changed: Condvar,
    /// Mutex nommé partagé avec d'autres processus (#20) : propriétaire (tid
    /// Linux) et récursion dans la page partagée, à la place de `state`.
    shared: std::sync::OnceLock<crate::dlls::inherit::SharedWord>,
}

// Page d'état d'un objet nommé partagé entre processus (#20), en mots de
// 32 bits : 0 = état (événement signalé, tid du propriétaire d'un mutex,
// compteur d'un sémaphore), 1 = récursion du mutex ou maximum du sémaphore,
// 2 = type d'objet, 3 = réinitialisation manuelle d'un événement.
const SHARED_KIND_EVENT: u32 = 1;
const SHARED_KIND_MUTEX: u32 = 2;
const SHARED_KIND_SEMAPHORE: u32 = 3;

fn os_thread_id() -> u32 {
    unsafe { libc::syscall(libc::SYS_gettid) as u32 }
}

impl MutexObject {
    fn new(owned: bool) -> MutexObject {
        MutexObject {
            state: Mutex::new(MutexState {
                owner: owned.then(|| std::thread::current().id()),
                owner_os: if owned { os_thread_id() } else { 0 },
                recursion: usize::from(owned),
            }),
            changed: Condvar::new(),
            shared: std::sync::OnceLock::new(),
        }
    }
    /// Mutex partagé : essaie de le prendre. Some(abandonné) si pris.
    fn shared_try(w: &crate::dlls::inherit::SharedWord) -> Option<bool> {
        let me = os_thread_id();
        let owner = w.slot(0).load(Ordering::Acquire);
        if owner == me {
            w.slot(1).fetch_add(1, Ordering::AcqRel);
            return Some(false);
        }
        // Propriétaire disparu (thread ou processus terminé) : le mutex est
        // abandonné, le suivant le prend avec WAIT_ABANDONED.
        let abandoned = owner != 0 && !os_process_alive(owner as i32);
        if (owner == 0 || abandoned)
            && w.slot(0)
                .compare_exchange(owner, me, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            w.slot(1).store(1, Ordering::Release);
            return Some(abandoned);
        }
        None
    }
    /// Rend le mutex partageable avec un autre processus (DuplicateHandle
    /// du mutex « vivant » du bac à sable de Chromium, #767) : son état
    /// passe dans une page partagée, comme celui d'un mutex nommé (#20).
    pub(crate) fn share(&self) -> Option<i32> {
        let _gate = sync_gate();
        let st = lock(&self.state);
        if self.shared.get().is_none() {
            let w = crate::dlls::inherit::SharedWord::create(false)?;
            let (owner, depth) = match st.owner {
                Some(_) => (st.owner_os, st.recursion as u32),
                None => (0, 0),
            };
            w.slot(0).store(owner, Ordering::Release);
            w.slot(1).store(depth, Ordering::Release);
            w.slot(2).store(SHARED_KIND_MUTEX, Ordering::Release);
            let _ = self.shared.set(w);
            // Les attentes locales en cours repartent sur la page partagée.
            self.changed.notify_all();
        }
        self.shared.get().map(|w| w.fd())
    }
    /// Mutex dont l'état vit dans une page partagée reçue d'un autre
    /// processus.
    pub(crate) fn adopt(word: crate::dlls::inherit::SharedWord) -> SyncHandle {
        let m = MutexObject::new(false);
        let _ = m.shared.set(word);
        SyncHandle::Mutex(Arc::new(m))
    }
    fn shared_ready(w: &crate::dlls::inherit::SharedWord) -> bool {
        let owner = w.slot(0).load(Ordering::Acquire);
        owner == 0 || owner == os_thread_id() || !os_process_alive(owner as i32)
    }
}

impl SemaphoreObject {
    fn new(initial: i32, maximum: i32) -> SemaphoreObject {
        SemaphoreObject {
            count: Mutex::new((initial, maximum)),
            changed: Condvar::new(),
            shared: std::sync::OnceLock::new(),
        }
    }
    /// Rend le sémaphore partageable avec un autre processus : compteur et
    /// maximum passent dans une page partagée.
    pub(crate) fn share(&self) -> Option<i32> {
        let _gate = sync_gate();
        let c = lock(&self.count);
        if self.shared.get().is_none() {
            let w = crate::dlls::inherit::SharedWord::create(false)?;
            w.slot(0).store(c.0.max(0) as u32, Ordering::Release);
            w.slot(1).store(c.1.max(0) as u32, Ordering::Release);
            w.slot(2).store(SHARED_KIND_SEMAPHORE, Ordering::Release);
            let _ = self.shared.set(w);
            self.changed.notify_all();
        }
        self.shared.get().map(|w| w.fd())
    }
    pub(crate) fn adopt(word: crate::dlls::inherit::SharedWord) -> SyncHandle {
        let max = word.slot(1).load(Ordering::Acquire) as i32;
        let sem = SemaphoreObject::new(0, max);
        let _ = sem.shared.set(word);
        SyncHandle::Semaphore(Arc::new(sem))
    }
    fn shared_try(w: &crate::dlls::inherit::SharedWord) -> bool {
        let c = w.slot(0);
        let mut v = c.load(Ordering::Acquire);
        while v > 0 {
            match c.compare_exchange(v, v - 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return true,
                Err(now) => v = now,
            }
        }
        false
    }
}
struct ThreadState {
    finished: bool,
    exit_code: u32,
    suspend_count: usize,
    /// Identifiant Win32 du thread (son tid Linux, celui que lit
    /// GetCurrentThreadId dans le thread) ; 0 tant que le thread n'a pas pris
    /// la main une première fois.
    win_tid: u32,
    /// Le thread a quitté son attente de démarrage : la suspension passe
    /// désormais par `slot` (signal), plus par `suspend_count`.
    running: bool,
}
pub(crate) struct ThreadObject {
    state: Mutex<ThreadState>,
    changed: Condvar,
    /// Identifiant Linux du thread (0 tant qu'il n'a pas démarré).
    pub(crate) os_tid: AtomicI32,
    /// Affinité ou priorité demandée avant le démarrage (thread suspendu).
    pub(crate) pending_cpus: Mutex<Option<Vec<usize>>>,
    pub(crate) priority: AtomicI32,
    /// Contexte de retour vers le pont de thread (setjmp maison) : ExitThread
    /// y saute pour terminer le thread proprement sans dérouler les trames
    /// Windows, qui n'ont pas d'informations de déroulement Linux.
    exit_jump: Mutex<Box<[u64; 8]>>,
    /// Suspension d'un thread démarré (SuspendThread, GetThreadContext…).
    pub(crate) slot: super::thread_ctl::SuspendSlot,
    /// Thread Rust correspondant (file d'APC de QueueUserAPC).
    pub(crate) rust_id: OnceLock<ThreadId>,
}

impl ThreadObject {
    fn new(suspended: bool) -> Self {
        ThreadObject {
            state: Mutex::new(ThreadState {
                finished: false,
                exit_code: 259,
                suspend_count: usize::from(suspended),
                win_tid: 0,
                running: false,
            }),
            changed: Condvar::new(),
            os_tid: AtomicI32::new(0),
            pending_cpus: Mutex::new(None),
            priority: AtomicI32::new(0),
            exit_jump: Mutex::new(Box::new([0; 8])),
            slot: super::thread_ctl::SuspendSlot::new(),
            rust_id: OnceLock::new(),
        }
    }
}

core::arch::global_asm!(
    ".globl winlux_thread_setjmp",
    ".p2align 4",
    "winlux_thread_setjmp:",
    // RCX = tampon de 8 mots : rbx, rbp, r12-r15, rsp après retour, rip.
    "mov [rcx], rbx",
    "mov [rcx+8], rbp",
    "mov [rcx+16], r12",
    "mov [rcx+24], r13",
    "mov [rcx+32], r14",
    "mov [rcx+40], r15",
    "lea rax, [rsp+8]",
    "mov [rcx+48], rax",
    "mov rax, [rsp]",
    "mov [rcx+56], rax",
    "xor eax, eax",
    "ret",
    ".globl winlux_thread_longjmp",
    ".p2align 4",
    "winlux_thread_longjmp:",
    // RCX = tampon, RDX = valeur rendue par le second retour de setjmp.
    "mov rbx, [rcx]",
    "mov rbp, [rcx+8]",
    "mov r12, [rcx+16]",
    "mov r13, [rcx+24]",
    "mov r14, [rcx+32]",
    "mov r15, [rcx+40]",
    "mov rsp, [rcx+48]",
    "mov rax, rdx",
    "jmp qword ptr [rcx+56]",
);
extern "win64" {
    fn winlux_thread_setjmp(buf: *mut u64) -> u64;
    fn winlux_thread_longjmp(buf: *mut u64, value: u64) -> !;
}

/// Cible Linux d'un handle de thread Win32.
pub(crate) enum ThreadTarget {
    Running(i32),
    NotStarted(Arc<ThreadObject>),
}

/// Identifiant Win32 du thread d'un handle (GetThreadId).
pub(crate) fn thread_id_of_handle(h: usize) -> Option<u32> {
    match lookup_sync(h)? {
        SyncHandle::Thread(t) => {
            let st = t
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Some(if st.win_tid != 0 {
                st.win_tid
            } else {
                t.os_tid.load(Ordering::Acquire) as u32
            })
        }
        _ => None,
    }
}

pub(crate) fn thread_target(h: usize) -> Option<ThreadTarget> {
    if h == get_current_thread() {
        return Some(ThreadTarget::Running(unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }));
    }
    match lookup_sync(h)? {
        SyncHandle::Thread(t) => match t.os_tid.load(Ordering::Acquire) {
            0 => Some(ThreadTarget::NotStarted(t)),
            tid => Some(ThreadTarget::Running(tid)),
        },
        _ => None,
    }
}

/// PID Linux d'un handle de processus (le processus courant ou un enfant).
/// Vrai pour le pseudo-handle du processus courant ou un vrai handle vers
/// lui (DuplicateHandle de GetCurrentProcess, OpenProcess sur son PID).
pub(crate) fn is_current_process_handle(h: usize) -> bool {
    h == get_current_process() || process_os_pid(h) == Some(std::process::id() as i32)
}

pub(crate) fn process_os_pid(h: usize) -> Option<i32> {
    if h == get_current_process() {
        return Some(std::process::id() as i32);
    }
    match lookup_sync(h)? {
        SyncHandle::Process(p) => {
            let s = lock(&p.state);
            s.child.as_ref().map(|c| c.id() as i32).or(s.foreign)
        }
        _ => None,
    }
}
/// Appelle `f` avec le PID d'un enfant encore suspendu (CREATE_SUSPENDED),
/// son état verrouillé : personne ne l'attend ni ne le relance pendant
/// l'opération (`remote_vm`, #767).
pub(crate) fn with_suspended_child<R>(h: usize, f: impl FnOnce(i32) -> R) -> Option<R> {
    match lookup_sync(h)? {
        SyncHandle::Process(p) => {
            let s = lock(&p.state);
            match s.child.as_ref() {
                Some(c) if s.suspended => Some(f(c.id() as i32)),
                _ => None,
            }
        }
        _ => None,
    }
}
/// Protection Linux d'une protection Windows (PAGE_*).
pub(crate) fn linux_prot(p: u32) -> i32 {
    win_prot(p)
}
pub(crate) fn valid_page_prot(p: u32) -> bool {
    valid_win_prot(p)
}
struct ProcessState {
    child: Option<Child>,
    /// Processus ouvert par `OpenProcess` sans en être le parent (Gestionnaire
    /// des tâches, outils de diagnostic) : seul son PID Linux est connu.
    foreign: Option<i32>,
    exit_code: u32,
    suspended: bool,
}
pub(crate) struct ProcessObject {
    state: Mutex<ProcessState>,
    changed: Condvar,
}
#[derive(Clone)]
pub(crate) enum SyncHandle {
    Event(Arc<EventObject>),
    Mutex(Arc<MutexObject>),
    Thread(Arc<ThreadObject>),
    Process(Arc<ProcessObject>),
    CompletionPort(Arc<CompletionPort>),
    Semaphore(Arc<SemaphoreObject>),
}
static SYNCS: OnceLock<Mutex<HashMap<usize, SyncHandle>>> = OnceLock::new();
static SYNC_NAMES: OnceLock<Mutex<HashMap<String, (SyncHandle, usize)>>> = OnceLock::new();
static SYNC_GATE: OnceLock<Mutex<()>> = OnceLock::new();
static CRITICALS: OnceLock<Mutex<HashMap<usize, Arc<MutexObject>>>> = OnceLock::new();
const TLS_LIMIT: usize = crate::process::TLS_SLOTS + crate::process::TLS_EXPANSION_SLOTS;
static TLS_GENERATION: AtomicUsize = AtomicUsize::new(1);
static TLS_SLOTS: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
static CLOCK_START: OnceLock<std::time::Instant> = OnceLock::new();

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SystemInfo {
    pub processor_architecture: u16,
    pub reserved: u16,
    pub page_size: u32,
    pub minimum_application_address: *mut c_void,
    pub maximum_application_address: *mut c_void,
    pub active_processor_mask: usize,
    pub number_of_processors: u32,
    pub processor_type: u32,
    pub allocation_granularity: u32,
    pub processor_level: u16,
    pub processor_revision: u16,
}

/// Layout-compatible with Win32 `SYSTEMTIME`.  Keeping this API in kernel32
/// lets guest applications present the host's local clock without scraping a
/// Linux-specific source themselves.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SystemTime {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryBasicInformation {
    pub base_address: *mut c_void,
    pub allocation_base: *mut c_void,
    pub allocation_protect: u32,
    pub partition_id: u16,
    pub region_size: usize,
    pub state: u32,
    pub protect: u32,
    pub kind: u32,
}

#[repr(C)]
pub struct StartupInfoA {
    pub cb: u32,
    pub reserved: *mut u8,
    pub desktop: *mut u8,
    pub title: *mut u8,
    pub x: u32,
    pub y: u32,
    pub x_size: u32,
    pub y_size: u32,
    pub x_chars: u32,
    pub y_chars: u32,
    pub fill_attribute: u32,
    pub flags: u32,
    pub show_window: u16,
    pub reserved2_size: u16,
    pub reserved2: *mut u8,
    pub std_input: usize,
    pub std_output: usize,
    pub std_error: usize,
}
#[repr(C)]
pub struct ProcessInformation {
    pub process: usize,
    pub thread: usize,
    pub process_id: u32,
    pub thread_id: u32,
}

#[repr(C)]
pub struct Overlapped {
    pub internal: usize,
    pub internal_high: usize,
    pub offset: u32,
    pub offset_high: u32,
    pub event: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OverlappedEntry {
    pub completion_key: usize,
    pub overlapped: *mut c_void,
    pub internal: usize,
    pub bytes_transferred: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FileTime {
    pub low: u32,
    pub high: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Win32FileAttributeData {
    pub attributes: u32,
    pub creation_time: FileTime,
    pub access_time: FileTime,
    pub write_time: FileTime,
    pub size_high: u32,
    pub size_low: u32,
}

#[repr(C)]
struct CriticalSection {
    debug_info: *mut c_void,
    lock_count: i32,
    recursion_count: i32,
    owning_thread: usize,
    lock_semaphore: usize,
    spin_count: usize,
}

pub fn process_heap() -> usize {
    PROCESS_HEAP.load(Ordering::Relaxed)
}

pub fn init() {
    PROCESS_HEAP
        .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
        .ok();
    let _ = ALLOCS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let _ = HEAP_ALLOCS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = HANDLES.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = SYNCS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = SYNC_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = CRITICALS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = TLS_SLOTS.get_or_init(|| Mutex::new(vec![0; TLS_LIMIT]));
    let _ = CLOCK_START.get_or_init(std::time::Instant::now);
}

/// Valeurs des handles d'entrée, de sortie et d'erreur standard rendues
/// par GetStdHandle. Comme sous Windows, ce sont de vrais handles (petits
/// multiples de 4) et non les constantes STD_*_HANDLE : certains runtimes
/// (Bun, donc Claude Code) rangent un handle sur 63 bits et perdaient le bit
/// de signe des anciennes valeurs -10/-11/-12. Elles restent au-dessus des
/// descripteurs de sockets et sous les handles de fichiers (0x10000).
pub(crate) const STD_INPUT_REAL: usize = 0xFFF0;
pub(crate) const STD_OUTPUT_REAL: usize = 0xFFF4;
pub(crate) const STD_ERROR_REAL: usize = 0xFFF8;
static STD_OVERRIDE: [AtomicUsize; 3] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

/// Ramène toutes les formes d'un handle standard (réel, constante STD_*,
/// constante tronquée à 63 bits) à la constante STD_* historique, que le
/// reste de kernel32 reconnaît.
pub(crate) fn canon_std(h: usize) -> usize {
    match h {
        STD_INPUT_REAL | 0x7FFF_FFFF_FFFF_FFF6 => (-10isize) as usize,
        STD_OUTPUT_REAL | 0x7FFF_FFFF_FFFF_FFF5 => (-11isize) as usize,
        STD_ERROR_REAL | 0x7FFF_FFFF_FFFF_FFF4 => (-12isize) as usize,
        // Tampon d'écran de CreateConsoleScreenBuffer : le même terminal.
        _ if crate::dlls::kernel32_releve::is_screen_buffer(h) => (-11isize) as usize,
        _ => h,
    }
}

pub extern "win64" fn get_std_handle(which: u32) -> usize {
    let slot = match which as i32 {
        -10 => 0,
        -11 => 1,
        -12 => 2,
        _ => {
            crate::process::set_last_error(6);
            return usize::MAX;
        }
    };
    match STD_OVERRIDE[slot].load(Ordering::Relaxed) {
        0 => [STD_INPUT_REAL, STD_OUTPUT_REAL, STD_ERROR_REAL][slot],
        h => h,
    }
}

/// `SetStdHandle` : redirige l'entrée, la sortie ou l'erreur standard vers
/// un autre handle (fichier, tube) pour les GetStdHandle suivants.
pub extern "win64" fn set_std_handle(which: u32, h: usize) -> i32 {
    let slot = match which as i32 {
        -10 => 0,
        -11 => 1,
        -12 => 2,
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    let real = [STD_INPUT_REAL, STD_OUTPUT_REAL, STD_ERROR_REAL][slot];
    let value = if canon_std(h) == canon_std(real) {
        0
    } else {
        h
    };
    STD_OVERRIDE[slot].store(value, Ordering::Relaxed);
    1
}

/// Comme sous Windows, un handle est un multiple de 4 : le bit 0 de
/// `OVERLAPPED.hEvent` dit « pas de paquet au port de complétion ». Avec un
/// numéro impair, la fin d'une E/S signalait le handle voisin et l'événement
/// attendu ne l'était jamais (#1360).
fn next_handle() -> usize {
    NEXT_HANDLE.fetch_add(4, Ordering::Relaxed)
}
/// Numéro de handle neuf pour les objets tenus hors de kernel32 (mappings).
pub(crate) fn new_handle_number() -> usize {
    next_handle()
}
/// Handle hérité (voir `inherit`) : même valeur que chez le parent.
pub(crate) fn insert_file_at(h: usize, file: File, overlapped: bool) {
    lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())))
        .insert(h, FileHandle::File(Arc::new(Mutex::new(file))));
    if overlapped {
        mark_overlapped_handle(h);
    }
}
pub(crate) fn insert_sync_at(h: usize, obj: SyncHandle) {
    lock(SYNCS.get_or_init(|| Mutex::new(HashMap::new()))).insert(h, obj);
}
/// Les prochains handles seront au-dessus de `h` (handles hérités).
pub(crate) fn reserve_handles_up_to(h: usize) {
    NEXT_HANDLE.fetch_max((h + 4) & !3, Ordering::Relaxed);
}
/// Événement transmissible à un enfant : (descripteur de sa page partagée,
/// réinitialisation manuelle).
pub(crate) fn share_event(h: usize) -> Option<(i32, bool)> {
    match lookup_sync(h)? {
        SyncHandle::Event(e) => Some((e.share()?, e.manual_reset)),
        _ => None,
    }
}
/// Gestionnaire de plantage de crashpad lancé par ce processus ou désigné
/// par notre parent (variable `WINLUX_CRASH_HANDLER_PID`).
static CRASH_HANDLER: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
pub(crate) const CRASH_HANDLER_ENV: &str = "WINLUX_CRASH_HANDLER_PID";
fn crash_handler_pid() -> Option<i32> {
    Some(CRASH_HANDLER.load(Ordering::Relaxed)).filter(|&p| p > 1)
}
/// Au démarrage : les enfants de Chromium (GPU, réseau, rendu) ne sont pas
/// des descendants du gestionnaire de crashpad, que Linux (Yama) empêche
/// alors de lire leur mémoire ; sous Windows, il l'ouvre comme tout
/// processus du même compte (#767). L'enfant le désigne donc lui-même.
pub(crate) fn adopt_crash_handler() {
    let Some(pid) = std::env::var(CRASH_HANDLER_ENV)
        .ok()
        .and_then(|p| p.parse::<i32>().ok())
        .filter(|&p| p > 1 && p != std::process::id() as i32)
    else {
        return;
    };
    CRASH_HANDLER.store(pid, Ordering::Relaxed);
    unsafe { libc::prctl(libc::PR_SET_PTRACER, pid as libc::c_ulong, 0, 0, 0) };
}
/// Mutex ou sémaphore transmissible à un autre processus : (descripteur
/// de sa page partagée, vrai pour un sémaphore).
pub(crate) fn share_sync(h: usize) -> Option<(i32, bool)> {
    match lookup_sync(h)? {
        SyncHandle::Mutex(m) => Some((m.share()?, false)),
        SyncHandle::Semaphore(s) => Some((s.share()?, true)),
        _ => None,
    }
}
/// PID d'un handle de processus réel (pas le pseudo-handle courant).
pub(crate) fn inheritable_process_pid(h: usize) -> Option<i32> {
    match lookup_sync(h)? {
        SyncHandle::Process(_) => process_os_pid(h),
        _ => None,
    }
}
/// Processus connu par son seul PID (OpenProcess, handle hérité).
pub(crate) fn foreign_process(pid: i32) -> SyncHandle {
    SyncHandle::Process(Arc::new(ProcessObject {
        state: Mutex::new(ProcessState {
            child: None,
            foreign: Some(pid),
            exit_code: 259,
            suspended: false,
        }),
        changed: Condvar::new(),
    }))
}
pub(crate) fn insert_file(file: File) -> usize {
    let h = next_handle();
    lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())))
        .insert(h, FileHandle::File(Arc::new(Mutex::new(file))));
    h
}
pub(crate) fn duplicate_handle_impl(handle: usize) -> Option<usize> {
    use std::os::unix::io::AsRawFd;
    let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
    let fh = handles.get(&handle)?;
    let FileHandle::File(arc) = fh;
    let fd = {
        let f = lock(arc);
        f.as_raw_fd()
    };
    let new_fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if new_fd < 0 {
        return None;
    }
    drop(handles);
    let new_file = unsafe { std::os::unix::io::FromRawFd::from_raw_fd(new_fd) };
    let new_h = next_handle();
    lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())))
        .insert(new_h, FileHandle::File(Arc::new(Mutex::new(new_file))));
    if is_overlapped_handle(handle) {
        lock(overlapped_handles()).insert(new_h);
    }
    Some(new_h)
}
fn post_iocp(port: &CompletionPort, packet: CompletionPacket) {
    lock(&port.state).queue.push_back(packet);
    port.changed.notify_all();
}
fn iocp_is_signaled(port: &CompletionPort) -> bool {
    !lock(&port.state).queue.is_empty()
}
fn wait_iocp(port: &CompletionPort, timeout: u32) -> u32 {
    let start = std::time::Instant::now();
    let mut state = lock(&port.state);
    loop {
        if !state.queue.is_empty() {
            return 0;
        }
        if timeout == 0 {
            return 258;
        }
        let left = if timeout == u32::MAX {
            u32::MAX
        } else {
            let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
            if elapsed >= timeout {
                return 258;
            }
            timeout - elapsed
        };
        let (next, timed) = wait_condvar(&port.changed, state, left);
        state = next;
        if timed && state.queue.is_empty() {
            return 258;
        }
    }
}
pub(crate) fn release_iocp_slots_for_current_thread() {
    let tid = std::thread::current().id();
    let mut ports: Vec<Arc<CompletionPort>> = Vec::new();
    let mut seen = HashSet::new();
    for obj in lock(SYNCS.get_or_init(|| Mutex::new(HashMap::new()))).values() {
        if let SyncHandle::CompletionPort(p) = obj {
            let key = Arc::as_ptr(p) as usize;
            if seen.insert(key) {
                ports.push(p.clone());
            }
        }
    }
    for (p, _) in lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new()))).values() {
        let key = Arc::as_ptr(p) as usize;
        if seen.insert(key) {
            ports.push(p.clone());
        }
    }
    for p in ports {
        let mut state = lock(&p.state);
        if state.active.remove(&tid).is_some() {
            p.changed.notify_all();
        }
    }
}
pub(crate) fn io_error_code(e: &std::io::Error) -> u32 {
    match e.raw_os_error() {
        Some(libc::ENAMETOOLONG) => return 206, // ERROR_FILENAME_EXCED_RANGE
        Some(libc::ELOOP) => return 1921,       // ERROR_CANT_RESOLVE_FILENAME
        Some(libc::EROFS) => return 19,         // ERROR_WRITE_PROTECT
        Some(libc::ENOSPC) | Some(libc::EDQUOT) => return 112, // ERROR_DISK_FULL
        Some(libc::EXDEV) => return 17,         // ERROR_NOT_SAME_DEVICE
        Some(libc::ENOTEMPTY) => return 145,    // ERROR_DIR_NOT_EMPTY
        Some(libc::EISDIR) | Some(libc::EPERM) => return 5, // ERROR_ACCESS_DENIED
        Some(libc::EBUSY) | Some(libc::ETXTBSY) => return 32, // ERROR_SHARING_VIOLATION
        Some(libc::EFAULT) => return 998,       // ERROR_NOACCESS : tampon invalide (#18)
        _ => {}
    }
    match e.kind() {
        std::io::ErrorKind::NotFound => 2,
        std::io::ErrorKind::PermissionDenied => 5,
        std::io::ErrorKind::AlreadyExists => 183,
        std::io::ErrorKind::InvalidInput => 87,
        std::io::ErrorKind::NotADirectory => 267,
        _ => 31,
    }
}
unsafe fn bytes<'a>(p: *const u8, len: usize) -> Option<&'a [u8]> {
    if len != 0 && p.is_null() {
        None
    } else if len == 0 {
        Some(&[])
    } else {
        Some(std::slice::from_raw_parts(p, len))
    }
}
unsafe fn bytes_mut<'a>(p: *mut u8, len: usize) -> Option<&'a mut [u8]> {
    if len != 0 && p.is_null() {
        None
    } else if len == 0 {
        Some(std::slice::from_raw_parts_mut(
            std::ptr::NonNull::<u8>::dangling().as_ptr(),
            0,
        ))
    } else {
        Some(std::slice::from_raw_parts_mut(p, len))
    }
}
use super::lock;
fn sync_gate() -> std::sync::MutexGuard<'static, ()> {
    lock(SYNC_GATE.get_or_init(|| Mutex::new(())))
}

/// ReadFile/WriteFile avec un OVERLAPPED sur un handle ouvert sans
/// FILE_FLAG_OVERLAPPED : l'opération est synchrone, à la position donnée
/// par l'OVERLAPPED (Windows ignore alors le pointeur de fichier courant).
unsafe fn sync_io_at_overlapped(
    h: usize,
    buffer: usize,
    len: u32,
    done: *mut u32,
    overlapped: usize,
    is_write: bool,
) -> i32 {
    if !overlapped.is_multiple_of(std::mem::align_of::<Overlapped>()) {
        crate::process::set_last_error(87);
        return 0;
    }
    if len != 0 && buffer == 0 {
        crate::process::set_last_error(998); // ERROR_NOACCESS, comme Windows (#18)
        return 0;
    }
    let file = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let ov = overlapped as *mut Overlapped;
    let offset = ((*ov).offset_high as u64) << 32 | (*ov).offset as u64;
    let mut f = lock(&file);
    // Un tube n'a pas de position : on lit ou écrit simplement dans l'ordre.
    let seekable = f.stream_position().is_ok();
    if seekable {
        if let Err(e) = f.seek(SeekFrom::Start(offset)) {
            (*ov).internal = 0xC000_000D; // STATUS_INVALID_PARAMETER
            crate::process::set_last_error(io_error_code(&e));
            return 0;
        }
    }
    let result = if is_write {
        f.write(std::slice::from_raw_parts(
            buffer as *const u8,
            len as usize,
        ))
    } else {
        f.read(std::slice::from_raw_parts_mut(
            buffer as *mut u8,
            len as usize,
        ))
    };
    drop(f);
    match result {
        Ok(n) => {
            (*ov).internal = 0;
            (*ov).internal_high = n;
            if !done.is_null() {
                *done = n as u32;
            }
            if (*ov).event != 0 {
                set_event((*ov).event);
            }
            1
        }
        Err(e) => {
            let code = io_error_code(&e);
            (*ov).internal = 0xC000_0001; // STATUS_UNSUCCESSFUL
            (*ov).internal_high = 0;
            if (*ov).event != 0 {
                set_event((*ov).event);
            }
            crate::process::set_last_error(code);
            0
        }
    }
}

/// NTSTATUS rangé dans OVERLAPPED.Internal pour une erreur Win32 :
/// libuv le relit avec RtlNtStatusToDosError (fin de tube = EOF, etc.).
pub(crate) fn ntstatus_of_win32(error: u32) -> u32 {
    match error {
        0 => 0,
        5 => 0xC000_0022,     // ACCESS_DENIED
        6 => 0xC000_0008,     // INVALID_HANDLE
        38 => 0xC000_0011,    // HANDLE_EOF → END_OF_FILE
        87 => 0xC000_000D,    // INVALID_PARAMETER
        109 => 0xC000_014B,   // BROKEN_PIPE → PIPE_BROKEN
        112 => 0xC000_007F,   // DISK_FULL
        232 => 0xC000_00B1,   // NO_DATA → PIPE_CLOSING
        995 => 0xC000_0120,   // OPERATION_ABORTED → CANCELLED
        1236 => 0xC000_0241,  // CONNECTION_ABORTED
        10054 => 0xC000_020D, // WSAECONNRESET → CONNECTION_RESET
        10060 => 0xC000_00B5, // WSAETIMEDOUT → IO_TIMEOUT
        10061 => 0xC000_0236, // WSAECONNREFUSED → CONNECTION_REFUSED
        10051 => 0xC000_023C, // WSAENETUNREACH → NETWORK_UNREACHABLE
        10065 => 0xC000_023D, // WSAEHOSTUNREACH → HOST_UNREACHABLE
        10053 => 0xC000_0241, // WSAECONNABORTED → CONNECTION_ABORTED
        _ => 0xC000_0001,
    }
}

/// Erreur Win32 d'un NTSTATUS produit par [`ntstatus_of_win32`].
fn win32_of_ntstatus(status: u32) -> u32 {
    crate::dlls::ntdll::rtl_nt_status_to_dos_error(status as i32)
}

/// Lecture immédiate d'un tube ou d'une socket : Some(Ok(n)) si des
/// données étaient prêtes, Some(Err(109)) si l'autre bout est fermé,
/// None s'il faut attendre.
fn try_stream_read_now(fd: i32, buffer: usize, len: u32) -> Option<Result<u32, u32>> {
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut p, 1, 0) } <= 0 {
        return None;
    }
    let n = unsafe {
        libc::recv(
            fd,
            buffer as *mut libc::c_void,
            len as usize,
            libc::MSG_DONTWAIT,
        )
    };
    let n = if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOTSOCK) {
        let mut avail: libc::c_int = 0;
        unsafe { libc::ioctl(fd, libc::FIONREAD, &mut avail) };
        if avail <= 0 {
            if p.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Some(Err(109));
            }
            return None;
        }
        unsafe {
            libc::read(
                fd,
                buffer as *mut libc::c_void,
                (len as usize).min(avail as usize),
            )
        }
    } else {
        n
    };
    if n < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EAGAIN) | Some(libc::EINTR) => None,
            Some(libc::ECONNRESET) => Some(Err(109)),
            _ => Some(Err(109)),
        };
    }
    if n == 0 {
        return Some(Err(109));
    }
    Some(Ok(n as u32))
}

/// E/S d'un tube ou d'une socket pour une opération asynchrone.
fn stream_io(
    fd: i32,
    buffer: usize,
    len: u32,
    write: Option<&[u8]>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<u32, u32> {
    if let Some(data) = write {
        let mut done = 0usize;
        while done < data.len() {
            if cancelled.load(Ordering::Acquire) {
                return Err(995);
            }
            let n = unsafe {
                libc::send(
                    fd,
                    data[done..].as_ptr() as *const libc::c_void,
                    data.len() - done,
                    libc::MSG_NOSIGNAL,
                )
            };
            let n = if n < 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOTSOCK)
            {
                unsafe {
                    libc::write(
                        fd,
                        data[done..].as_ptr() as *const libc::c_void,
                        data.len() - done,
                    )
                }
            } else {
                n
            };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                match e.raw_os_error() {
                    Some(libc::EINTR) => continue,
                    Some(libc::EAGAIN) => {
                        let mut p = libc::pollfd {
                            fd,
                            events: libc::POLLOUT,
                            revents: 0,
                        };
                        unsafe { libc::poll(&mut p, 1, 100) };
                        continue;
                    }
                    Some(libc::EPIPE) | Some(libc::ECONNRESET) => return Err(232),
                    _ => return Err(io_error_code(&e)),
                }
            }
            done += n as usize;
        }
        return Ok(done as u32);
    }
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(995);
        }
        let mut p = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let r = unsafe { libc::poll(&mut p, 1, 100) };
        if r == 0 || (r < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR))
        {
            continue;
        }
        let mut avail: libc::c_int = 0;
        unsafe { libc::ioctl(fd, libc::FIONREAD, &mut avail) };
        if len == 0 {
            if avail == 0 && p.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Err(109);
            }
            return Ok(0);
        }
        let n = unsafe { libc::read(fd, buffer as *mut libc::c_void, len as usize) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            match e.raw_os_error() {
                Some(libc::EINTR) | Some(libc::EAGAIN) => continue,
                Some(libc::ECONNRESET) => return Err(109),
                _ => return Err(io_error_code(&e)),
            }
        }
        if n == 0 {
            return Err(109); // l'autre bout est fermé : ERROR_BROKEN_PIPE
        }
        return Ok(n as u32);
    }
}

/// Termine une opération asynchrone sans fil d'E/S (ConnectNamedPipe) :
/// OVERLAPPED, événement et port de complétion associé au handle.
/// Opération réussie tout de suite sur un handle en
/// FILE_SKIP_COMPLETION_PORT_ON_SUCCESS : OVERLAPPED rempli, événement
/// signalé si `signal`, aucun paquet au port de complétion.
/// Écrit Internal et InternalHigh d'un OVERLAPPED, en disposition x86
/// (champs de 4 octets) pour un programme 32 bits (#1408).
///
/// # Safety
/// `ov` désigne un OVERLAPPED valable.
pub(crate) unsafe fn ov_set(ov: usize, internal: usize, high: usize) {
    if crate::wow32::pointer_size() == 4 {
        std::ptr::write_volatile(ov as *mut u32, internal as u32);
        std::ptr::write_volatile((ov + 4) as *mut u32, high as u32);
    } else {
        let p = ov as *mut Overlapped;
        std::ptr::write_volatile(&mut (*p).internal, internal);
        std::ptr::write_volatile(&mut (*p).internal_high, high);
    }
}

/// hEvent d'un OVERLAPPED (décalage 16 en x86, 24 en x64).
///
/// # Safety
/// `ov` désigne un OVERLAPPED valable.
pub(crate) unsafe fn ov_event(ov: usize) -> usize {
    if crate::wow32::pointer_size() == 4 {
        std::ptr::read_unaligned((ov + 16) as *const u32) as usize
    } else {
        (*(ov as *const Overlapped)).event
    }
}

pub(crate) fn complete_overlapped_inline(overlapped: usize, bytes: u32, signal: bool) {
    unsafe {
        ov_set(overlapped, 0, bytes as usize);
        let raw = ov_event(overlapped);
        if signal && raw & !1 != 0 {
            let _ = set_event(raw & !1);
        }
    }
}

/// Opération asynchrone hors ReadFile/WriteFile (ConnectNamedPipe, #2219)
/// inscrite comme les autres : GetOverlappedResult la connaît et peut
/// l'attendre quand `bWait` vaut TRUE.
pub(crate) struct PendingIo {
    h: usize,
    overlapped: usize,
    op: Arc<IoOperation>,
}

/// Début d'une E/S asynchrone sur `h` : `Internal` = STATUS_PENDING et
/// hEvent remis à non signalé (« the function that started the operation
/// resets the hEvent member », page GetOverlappedResult). `None` si une
/// opération est déjà en cours sur ce même OVERLAPPED (ERROR_IO_PENDING posé).
///
/// # Safety
/// `overlapped` désigne un OVERLAPPED valable.
pub(crate) unsafe fn begin_overlapped(h: usize, overlapped: usize) -> Option<PendingIo> {
    let op = Arc::new(IoOperation {
        file_handle: h,
        issuing_thread: std::thread::current().id(),
        state: Mutex::new(IoResult {
            done: false,
            bytes: 0,
            error: 0,
        }),
        changed: Condvar::new(),
        cancelled: std::sync::atomic::AtomicBool::new(false),
    });
    {
        let mut ops = lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())));
        if ops.get(&overlapped).is_some_and(|o| !lock(&o.state).done) {
            crate::process::set_last_error(997);
            return None;
        }
        ops.insert(overlapped, op.clone());
    }
    ov_set(overlapped, 0x103, 0);
    let event = ov_event(overlapped) & !1;
    if event != 0 {
        let _ = reset_event(event);
    }
    Some(PendingIo { h, overlapped, op })
}

impl PendingIo {
    /// L'opération est-elle annulée (CancelIo/CancelIoEx) ?
    pub(crate) fn cancelled(&self) -> bool {
        self.op.cancelled.load(Ordering::Acquire)
    }
    /// Fin de l'opération : résultat noté pour GetOverlappedResult, puis
    /// `Internal`, événement et port de complétion comme `complete_overlapped`.
    pub(crate) fn finish(self, bytes: u32, error: u32) {
        {
            let mut state = lock(&self.op.state);
            state.bytes = bytes;
            state.error = error;
            state.done = true;
            self.op.changed.notify_all();
        }
        complete_overlapped(self.h, self.overlapped, bytes, error);
    }
    /// Le fil n'a pas pu partir : l'opération est oubliée.
    pub(crate) fn abandon(self) {
        let mut ops = lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())));
        if ops
            .get(&self.overlapped)
            .is_some_and(|o| Arc::ptr_eq(o, &self.op))
        {
            ops.remove(&self.overlapped);
        }
    }
}

pub(crate) fn complete_overlapped(h: usize, overlapped: usize, bytes: u32, error: u32) {
    unsafe {
        let raw = ov_event(overlapped);
        ov_set(
            overlapped,
            ntstatus_of_win32(error) as usize,
            bytes as usize,
        );
        if raw & !1 != 0 {
            let _ = set_event(raw & !1);
        }
        if raw & 1 != 0 {
            return;
        }
    }
    let completion = lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new())))
        .get(&h)
        .cloned();
    if let Some((port, key)) = completion {
        post_iocp(
            &port,
            CompletionPacket {
                bytes,
                key,
                overlapped,
                error,
            },
        );
    }
}

/// Paquet au port lié à `h`, sans toucher à `overlapped` : la valeur est
/// celle que l'appelant a confiée (ApcContext d'un IOCTL AFD, #1766).
pub(crate) fn post_completion(h: usize, overlapped: usize, bytes: u32, error: u32) -> bool {
    let completion = lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new())))
        .get(&h)
        .cloned();
    let Some((port, key)) = completion else {
        return false;
    };
    post_iocp(
        &port,
        CompletionPacket {
            bytes,
            key,
            overlapped,
            error,
        },
    );
    true
}

fn start_overlapped_io(
    h: usize,
    buffer: usize,
    len: u32,
    overlapped: usize,
    is_write: bool,
    done_out: *mut u32,
) -> i32 {
    const ERROR_IO_PENDING: u32 = 997;
    const STATUS_PENDING: usize = 0x103;
    if overlapped == 0
        || !overlapped.is_multiple_of(std::mem::align_of::<Overlapped>())
        || (len != 0 && buffer == 0)
    {
        crate::process::set_last_error(87);
        return 0;
    }
    let file = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let ov = overlapped as *mut Overlapped;
    let offset = unsafe { ((*ov).offset_high as u64) << 32 | (*ov).offset as u64 };
    // hEvent avec le bit 0 levé : signaler l'événement mais ne rien poster
    // au port de complétion (libuv s'en sert pour ses lectures d'essai).
    let raw_event = unsafe { (*ov).event };
    let event = raw_event & !1;
    // ReadFile et WriteFile remettent l'événement de l'OVERLAPPED à non
    // signalé au début de l'E/S (documentation Learn, « Synchronization and
    // File Position ») : resté signalé d'une E/S précédente, il réveillait
    // l'attente avant la fin de celle-ci (#2055).
    if event != 0 {
        let _ = reset_event(event);
    }
    let completion = if raw_event & 1 != 0 {
        None
    } else {
        lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new())))
            .get(&h)
            .cloned()
    };
    // Tube ou socket avec des données déjà là (ou fermé) : la lecture se
    // termine tout de suite, comme sous Windows. libuv lit ainsi la sortie
    // des processus enfants : ReadFile puis CancelIoEx si c'est en attente.
    if !is_write && len != 0 {
        use std::os::unix::io::AsRawFd;
        let fd = lock(&file).as_raw_fd();
        if file_type_of_fd(fd) == 3 {
            match try_stream_read_now(fd, buffer, len) {
                Some(Ok(n)) => {
                    let op = Arc::new(IoOperation {
                        file_handle: h,
                        issuing_thread: std::thread::current().id(),
                        state: Mutex::new(IoResult {
                            done: true,
                            bytes: n,
                            error: 0,
                        }),
                        changed: Condvar::new(),
                        cancelled: std::sync::atomic::AtomicBool::new(false),
                    });
                    lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())))
                        .insert(overlapped, op);
                    unsafe {
                        (*ov).internal = 0;
                        (*ov).internal_high = n as usize;
                        if !done_out.is_null() {
                            *done_out = n;
                        }
                    }
                    if event != 0 {
                        let _ = set_event(event);
                    }
                    if let Some((port, key)) = completion {
                        post_iocp(
                            &port,
                            CompletionPacket {
                                bytes: n,
                                key,
                                overlapped,
                                error: 0,
                            },
                        );
                    }
                    crate::process::set_last_error(0);
                    return 1;
                }
                Some(Err(e)) => {
                    lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())))
                        .remove(&overlapped);
                    unsafe {
                        (*ov).internal = ntstatus_of_win32(e) as usize;
                        (*ov).internal_high = 0;
                    }
                    crate::process::set_last_error(e);
                    return 0;
                }
                None => {}
            }
        }
    }
    let write_data = if is_write && len != 0 {
        Some(unsafe { std::slice::from_raw_parts(buffer as *const u8, len as usize).to_vec() })
    } else {
        None
    };
    let operation = Arc::new(IoOperation {
        file_handle: h,
        issuing_thread: std::thread::current().id(),
        state: Mutex::new(IoResult {
            done: false,
            bytes: 0,
            error: 0,
        }),
        changed: Condvar::new(),
        cancelled: std::sync::atomic::AtomicBool::new(false),
    });
    {
        let mut ops = lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())));
        if ops.get(&overlapped).is_some_and(|o| !lock(&o.state).done) {
            crate::process::set_last_error(997);
            return 0;
        }
        ops.insert(overlapped, operation.clone());
    }
    unsafe {
        (*ov).internal = STATUS_PENDING;
        (*ov).internal_high = 0;
    }
    let op = operation.clone();
    if std::thread::Builder::new()
        .name("winlux-overlapped-io".into())
        .spawn(move || {
            use std::os::unix::fs::FileExt;
            use std::os::unix::io::AsRawFd;
            let stream_fd = {
                let f = lock(&file);
                let fd = f.as_raw_fd();
                (file_type_of_fd(fd) == 3).then_some(fd)
            };
            let result = if let Some(fd) = stream_fd {
                // Tube ou socket : pas de position (read/write et non
                // pread/pwrite), lecture annulable, et lecture de zéro octet
                // = « attendre des données » (lecture d'amorçage de libuv).
                stream_io(fd, buffer, len, write_data.as_deref(), &op.cancelled)
            } else {
                let file = lock(&file);
                if op.cancelled.load(Ordering::Acquire) {
                    Err(995)
                } else if let Some(data) = write_data {
                    file.write_at(&data, offset)
                        .map(|n| n as u32)
                        .map_err(|e| io_error_code(&e))
                } else {
                    let mut data = vec![0u8; len as usize];
                    match file.read_at(&mut data, offset) {
                        Ok(n) => {
                            if n > 0 {
                                unsafe {
                                    std::ptr::copy_nonoverlapping(
                                        data.as_ptr(),
                                        buffer as *mut u8,
                                        n,
                                    );
                                }
                            }
                            Ok(n as u32)
                        }
                        Err(e) => Err(io_error_code(&e)),
                    }
                }
            };
            let (mut bytes, mut error) = (0u32, 0u32);
            match result {
                Ok(n) => bytes = n,
                Err(e) => error = e,
            }
            {
                let mut state = lock(&op.state);
                state.bytes = bytes;
                state.error = error;
                state.done = true;
                op.changed.notify_all();
            }
            unsafe {
                let p = overlapped as *mut Overlapped;
                (*p).internal = ntstatus_of_win32(error) as usize;
                (*p).internal_high = bytes as usize;
            }
            if event != 0 {
                let _ = set_event(event);
            }
            if let Some((port, key)) = completion {
                post_iocp(
                    &port,
                    CompletionPacket {
                        bytes,
                        key,
                        overlapped,
                        error,
                    },
                );
            }
        })
        .is_err()
    {
        lock(IO_OPERATIONS.get().unwrap()).remove(&overlapped);
        unsafe {
            (*ov).internal = 0;
        }
        crate::process::set_last_error(8);
        return 0;
    }
    crate::process::set_last_error(ERROR_IO_PENDING);
    0
}

pub unsafe extern "win64" fn get_overlapped_result(
    h: usize,
    overlapped: *mut Overlapped,
    transferred: *mut u32,
    wait: i32,
) -> i32 {
    if overlapped.is_null() || transferred.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let key = overlapped as usize;
    let operation = {
        let ops = lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())));
        ops.get(&key).cloned()
    };
    let Some(op) = operation else {
        // Opération déjà relevée (port de complétion) : Windows relit
        // simplement OVERLAPPED.Internal / InternalHigh.
        let status = (*overlapped).internal;
        if status == 0x103 {
            crate::process::set_last_error(996);
            return 0;
        }
        *transferred = (*overlapped).internal_high as u32;
        if status == 0 {
            return 1;
        }
        crate::process::set_last_error(win32_of_ntstatus(status as u32));
        return 0;
    };
    if op.file_handle != h {
        crate::process::set_last_error(6);
        return 0;
    }
    let mut state = lock(&op.state);
    if !state.done && wait == 0 {
        *transferred = 0;
        crate::process::set_last_error(996);
        return 0;
    }
    while !state.done {
        state = op
            .changed
            .wait(state)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    *transferred = state.bytes;
    let error = state.error;
    drop(state);
    lock(IO_OPERATIONS.get().unwrap()).remove(&key);
    if error != 0 {
        crate::process::set_last_error(error);
        0
    } else {
        1
    }
}
/// Fin de vie d'un handle hors kernel32 (socket fermée) : son numéro peut
/// resservir, il ne doit pas hériter du port de complétion.
pub(crate) fn forget_completion_port(h: usize) {
    lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&h);
}
/// CancelIo(Ex) sur une socket : ses opérations en cours sont annulées.
fn cancel_socket_io(h: usize, overlapped: usize) -> i32 {
    if crate::dlls::ws2_32::cancel_overlapped(h, overlapped) {
        1
    } else {
        crate::process::set_last_error(1168); // ERROR_NOT_FOUND
        0
    }
}
pub extern "win64" fn cancel_io(h: usize) -> i32 {
    if crate::dlls::ws2_32::is_socket(h) {
        return cancel_socket_io(h, 0);
    }
    if !lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new()))).contains_key(&h) {
        crate::process::set_last_error(6);
        return 0;
    }
    let current = std::thread::current().id();
    let ops = lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())));
    let mut cancelled = 0;
    for op in ops.values() {
        if op.file_handle == h && op.issuing_thread == current && !lock(&op.state).done {
            op.cancelled.store(true, Ordering::Release);
            cancelled += 1;
        }
    }
    if cancelled == 0 {
        crate::process::set_last_error(1168);
        0
    } else {
        1
    }
}
pub unsafe extern "win64" fn cancel_io_ex(h: usize, overlapped: *mut Overlapped) -> i32 {
    if crate::dlls::ws2_32::is_socket(h) {
        return cancel_socket_io(h, overlapped as usize);
    }
    if !lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new()))).contains_key(&h) {
        crate::process::set_last_error(6);
        return 0;
    }
    let ops = lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())));
    let mut cancelled = 0;
    for (key, op) in ops.iter() {
        if op.file_handle == h
            && (overlapped.is_null() || *key == overlapped as usize)
            && !lock(&op.state).done
        {
            op.cancelled.store(true, Ordering::Release);
            cancelled += 1;
        }
    }
    if cancelled == 0 {
        crate::process::set_last_error(1168);
        0
    } else {
        1
    }
}
/// Fils actifs d'un port qui tournent vraiment. Comme sous Windows, un fil
/// qui a retiré un paquet puis s'est bloqué ailleurs (WaitFor*, Sleep, E/S
/// synchrone, verrou) ne compte plus dans la limite de concurrence : il
/// laisse sa place à un autre fil qui attend le port (#2055). Un fil est en
/// cours s'il est « R » dans /proc/self/task/TID/stat.
fn running_threads(active: &HashMap<ThreadId, i32>) -> usize {
    active
        .values()
        .filter(|&&tid| {
            std::fs::read_to_string(format!("/proc/self/task/{tid}/stat"))
                .ok()
                .and_then(|st| {
                    let rest = &st[st.rfind(')')? + 1..];
                    rest.split_whitespace().next().map(|c| c == "R")
                })
                // Fil disparu : il ne tourne plus.
                .unwrap_or(false)
        })
        .count()
}
/// Délai entre deux examens quand la file attend parce que la limite de
/// concurrence est atteinte : un fil qui se bloque ailleurs ne prévient
/// pas le port.
const IOCP_CONCURRENCY_RECHECK_MS: u32 = 10;
fn dequeue_iocp(
    port: &CompletionPort,
    max_packets: usize,
    timeout: u32,
) -> Result<Vec<CompletionPacket>, u32> {
    let start = std::time::Instant::now();
    let current = std::thread::current().id();
    let mut state = lock(&port.state);
    if state.active.remove(&current).is_some() {
        port.changed.notify_all();
    }
    let mut noted = false;
    loop {
        let room = |state: &CompletionPortState| {
            state.active.len() < port.max_concurrency
                || running_threads(&state.active) < port.max_concurrency
        };
        let gated = !state.queue.is_empty() && !room(&state);
        if !state.queue.is_empty() && !gated {
            if state.active.len() >= port.max_concurrency {
                // Avant #2055, ce paquet serait resté en file.
                crate::trace!(
                    "GetQueuedCompletionStatus : place d'un fil bloqué ailleurs reprise ({} actif(s), limite {}, {} paquet(s))",
                    state.active.len(),
                    port.max_concurrency,
                    state.queue.len()
                );
            }
            let n = max_packets.min(state.queue.len());
            let packets = state.queue.drain(..n).collect();
            state.active.insert(current, unsafe { libc::gettid() });
            return Ok(packets);
        }
        if gated && !noted {
            noted = true;
            crate::trace!(
                "GetQueuedCompletionStatus : {} paquet(s) en attente, limite de concurrence {} atteinte par des fils qui tournent",
                state.queue.len(),
                port.max_concurrency
            );
        }
        if timeout == 0 {
            return Err(258);
        }
        let left = if timeout == u32::MAX {
            u32::MAX
        } else {
            let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
            if elapsed >= timeout {
                return Err(258);
            }
            timeout - elapsed
        };
        let wait = if gated {
            left.min(IOCP_CONCURRENCY_RECHECK_MS)
        } else {
            left
        };
        let (next, timed) = wait_condvar(&port.changed, state, wait);
        state = next;
        if timed && wait == left && (state.queue.is_empty() || !room(&state)) {
            return Err(258);
        }
    }
}
pub extern "win64" fn create_io_completion_port(
    file: usize,
    existing: usize,
    key: usize,
    _concurrency: u32,
) -> usize {
    let (port_handle, port) = if existing == 0 {
        let concurrency = if _concurrency == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        } else {
            _concurrency as usize
        };
        let p = Arc::new(CompletionPort {
            state: Mutex::new(CompletionPortState {
                queue: VecDeque::new(),
                active: HashMap::new(),
            }),
            changed: Condvar::new(),
            max_concurrency: concurrency.max(1),
        });
        let h = add_sync(SyncHandle::CompletionPort(p.clone()), None);
        (h, p)
    } else {
        let Some(SyncHandle::CompletionPort(p)) = lookup_sync(existing) else {
            crate::process::set_last_error(6);
            return 0;
        };
        (existing, p)
    };
    if file == usize::MAX {
        return port_handle;
    }
    // Les sockets (ws2_32) se lient aussi à un port : .NET y attend la fin
    // de ConnectEx, WSASend et WSARecv (#1146).
    if !lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new()))).contains_key(&file)
        && !crate::dlls::ws2_32::is_socket(file)
    {
        if existing == 0 {
            remove_sync(port_handle);
        }
        crate::process::set_last_error(6);
        return 0;
    }
    let mut associations = lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new())));
    if let Some((old, _)) = associations.get(&file) {
        if !Arc::ptr_eq(old, &port) {
            drop(associations);
            if existing == 0 {
                remove_sync(port_handle);
            }
            crate::process::set_last_error(87);
            return 0;
        }
    } else {
        associations.insert(file, (port, key));
    }
    port_handle
}
pub unsafe extern "win64" fn post_queued_completion_status(
    port: usize,
    bytes: u32,
    key: usize,
    overlapped: *mut c_void,
) -> i32 {
    let Some(SyncHandle::CompletionPort(p)) = lookup_sync(port) else {
        crate::process::set_last_error(6);
        return 0;
    };
    post_iocp(
        &p,
        CompletionPacket {
            bytes,
            key,
            overlapped: overlapped as usize,
            error: 0,
        },
    );
    1
}
pub unsafe extern "win64" fn get_queued_completion_status(
    port: usize,
    bytes: *mut u32,
    key: *mut usize,
    overlapped: *mut *mut c_void,
    timeout: u32,
) -> i32 {
    if bytes.is_null() || key.is_null() || overlapped.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(SyncHandle::CompletionPort(p)) = lookup_sync(port) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let packet = match dequeue_iocp(&p, 1, timeout) {
        Ok(mut packets) => packets.remove(0),
        Err(e) => {
            *bytes = 0;
            *key = 0;
            *overlapped = std::ptr::null_mut();
            crate::process::set_last_error(e);
            return 0;
        }
    };
    *bytes = packet.bytes;
    *key = packet.key;
    *overlapped = packet.overlapped as *mut c_void;
    if packet.error != 0 {
        crate::process::set_last_error(packet.error);
        0
    } else {
        1
    }
}
pub unsafe extern "win64" fn get_queued_completion_status_ex(
    port: usize,
    entries: *mut OverlappedEntry,
    count: u32,
    removed: *mut u32,
    timeout: u32,
    alertable: i32,
) -> i32 {
    if entries.is_null() || removed.is_null() || count == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if alertable != 0 {
        crate::process::set_last_error(120);
        return 0;
    }
    let Some(SyncHandle::CompletionPort(p)) = lookup_sync(port) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let packets = match dequeue_iocp(&p, count as usize, timeout) {
        Ok(p) => p,
        Err(e) => {
            *removed = 0;
            crate::process::set_last_error(e);
            return 0;
        }
    };
    for (i, packet) in packets.iter().enumerate() {
        let internal = if packet.error == 0 { 0 } else { 0xC000_0001 };
        std::ptr::write(
            entries.add(i),
            OverlappedEntry {
                completion_key: packet.key,
                overlapped: packet.overlapped as *mut c_void,
                internal,
                bytes_transferred: packet.bytes,
                reserved: 0,
            },
        );
    }
    *removed = packets.len() as u32;
    1
}

/// Avec `WINLUX_APITRACE`, chaque message que l'appli écrit sur stderr est
/// noté avec la pile de l'appelant (au plus 200 par processus) : un
/// « bad_variant_access was thrown in -fno-exceptions mode » de libc++,
/// suivi d'un arrêt dans le CRT statique de l'appli, dit ainsi d'où il
/// vient.
fn note_stderr_write(text: &[u8]) {
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    if !crate::thunks::api_trace_active()
        || COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 200
    {
        return;
    }
    let shown: String = String::from_utf8_lossy(text)
        .trim_end()
        .chars()
        .take(200)
        .collect();
    let calls = crate::dlls::seh::caller_stack(12);
    crate::thunks::api_note(&format!(
        "winlux[stderr] « {shown} » écrit par {}",
        calls.join(" ")
    ));
}

pub unsafe extern "win64" fn write_file(
    h: usize,
    buf: *const u8,
    len: u32,
    written: *mut u32,
    ov: usize,
) -> i32 {
    let h = canon_std(h);
    if !written.is_null() {
        *written = 0;
    }
    // Appareil virtuel (pilote remplacé, observation #1655).
    if let Some(r) = crate::dlls::vdev::write_file(h, buf, len, written, ov) {
        return r;
    }
    if ov != 0 {
        if is_overlapped_handle(h) {
            return start_overlapped_io(h, buf as usize, len, ov, true, written);
        }
        return sync_io_at_overlapped(h, buf as usize, len, written, ov, true);
    }
    let Some(s) = bytes(buf, len as usize) else {
        crate::process::set_last_error(998); // ERROR_NOACCESS (#18)
        return 0;
    };
    if h == (-11isize) as usize {
        return match std::io::stdout().write_all(s) {
            Ok(()) => {
                if !written.is_null() {
                    *written = len;
                }
                1
            }
            Err(e) => {
                crate::process::set_last_error(io_error_code(&e));
                0
            }
        };
    }
    if h == (-12isize) as usize {
        note_stderr_write(s);
        return match std::io::stderr().write_all(s) {
            Ok(()) => {
                if !written.is_null() {
                    *written = len;
                }
                1
            }
            Err(e) => {
                crate::process::set_last_error(io_error_code(&e));
                0
            }
        };
    }
    if h == (-10isize) as usize {
        crate::process::set_last_error(6);
        return 0;
    }
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let result = { lock(&f).write(s) };
    match result {
        Ok(n) => {
            if !written.is_null() {
                *written = n as u32;
            }
            1
        }
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn read_file(
    h: usize,
    buf: *mut u8,
    len: u32,
    read: *mut u32,
    ov: usize,
) -> i32 {
    let h = canon_std(h);
    if !read.is_null() {
        *read = 0;
    }
    if ov != 0 {
        if is_overlapped_handle(h) {
            return start_overlapped_io(h, buf as usize, len, ov, false, read);
        }
        return sync_io_at_overlapped(h, buf as usize, len, read, ov, false);
    }
    let Some(b) = bytes_mut(buf, len as usize) else {
        crate::process::set_last_error(998); // ERROR_NOACCESS (#18)
        return 0;
    };
    // Lecture de zéro octet sur un tube : Windows attend que des données
    // arrivent (ou la fin du tube). libuv (Node, Bun) s'en sert pour guetter
    // un tube sans tampon ; rendue tout de suite, elle relançait sa lecture
    // en boucle et `claude.exe auth login` prenait 300 Mo par seconde (#1042).
    if b.is_empty() {
        if let Some(fd) = fd_of_handle(h).filter(|&fd| fd_is_pipe(fd)) {
            if !wait_pipe_readable(fd) {
                crate::process::set_last_error(109);
                return 0;
            }
            return 1;
        }
    }
    if h == (-10isize) as usize {
        // Les invites écrites sans retour à la ligne doivent apparaître
        // avant que le programme n'attende la saisie.
        crate::dlls::crt_stdio::flush_console_output();
        return match std::io::stdin().read(b) {
            Ok(0) if !b.is_empty() && fd_is_pipe(0) => {
                crate::process::set_last_error(109);
                0
            }
            Ok(n) => {
                if !read.is_null() {
                    *read = n as u32;
                }
                1
            }
            Err(e) => {
                crate::process::set_last_error(io_error_code(&e));
                0
            }
        };
    }
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let result = { lock(&f).read(b) };
    match result {
        Ok(0) if !b.is_empty() && fd_of_handle(h).is_some_and(fd_is_pipe) => {
            // Fin d'un tube : Windows répond ERROR_BROKEN_PIPE.
            crate::process::set_last_error(109);
            0
        }
        Ok(n) => {
            if !read.is_null() {
                *read = n as u32;
            }
            1
        }
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn create_file_a(
    path: *const u8,
    access: u32,
    _share: u32,
    _sec: usize,
    creation: u32,
    _flags: u32,
    _template: usize,
) -> usize {
    if path.is_null() {
        crate::process::set_last_error(87);
        return usize::MAX;
    }
    create_file_str(
        &crate::dlls::cstr(path),
        access,
        _share,
        _sec,
        creation,
        _flags,
        _template,
    )
}
/// Corps commun de `create_file_a` et `create_file_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
fn want_write_access(access: u32) -> bool {
    // GENERIC_WRITE, GENERIC_ALL, FILE_WRITE_DATA, FILE_APPEND_DATA.
    access & (0x4000_0000 | 0x1000_0000 | 0x2 | 0x4) != 0
}
unsafe fn create_file_str(
    path: &str,
    access: u32,
    share: u32,
    sec: usize,
    creation: u32,
    flags: u32,
    template: usize,
) -> usize {
    let h = create_file_opened(path, access, share, sec, creation, flags, template);
    crate::dlls::inherit::from_security_attributes(h, sec);
    h
}
unsafe fn create_file_opened(
    path: &str,
    access: u32,
    _share: u32,
    _sec: usize,
    creation: u32,
    _flags: u32,
    _template: usize,
) -> usize {
    if let Some(name) = crate::dlls::named_pipes::pipe_name(path) {
        return crate::dlls::named_pipes::open_client(&name, _flags);
    }
    // \\?\usb#vid_…&mi_…#…#{86e0d1e0-…} : l'interface d'un port COM
    // (CM_Get_Device_Interface_List), ouverte comme \\.\COMn.
    let com_alias;
    let path = match crate::dlls::devnodes::comport_alias(path) {
        Some(c) => {
            com_alias = c;
            com_alias.as_str()
        }
        None => path,
    };
    // \\?\hid#vid_…#hidrawN#{…} : un périphérique HID (manette, volant…).
    // Appareil virtuel d'un pilote constructeur (#1533) : \\.\NomDuPilote.
    if crate::dlls::vdev::is_virtual(path) {
        return match crate::dlls::vdev::open(path) {
            Ok(f) => {
                crate::process::set_last_error(0);
                let h = insert_file(f);
                if _flags & 0x4000_0000 != 0 {
                    mark_overlapped_handle(h);
                }
                h
            }
            Err(_) => {
                crate::process::set_last_error(2);
                usize::MAX
            }
        };
    }
    if crate::dlls::hid::parse_path(path).is_some() {
        return match crate::dlls::hid::open_path(path, access) {
            Ok(f) => {
                crate::process::set_last_error(0);
                let h = insert_file(f);
                if _flags & 0x4000_0000 != 0 {
                    mark_overlapped_handle(h);
                }
                h
            }
            Err(e) => {
                crate::process::set_last_error(
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        5
                    } else {
                        2
                    },
                );
                usize::MAX
            }
        };
    }
    // \\?\usb#vid_…#…#{a5dcbf10-…} : un appareil USB pour winusb (usbfs).
    if let Some(dir) = crate::dlls::devnodes::usb_device_target(path) {
        return match crate::dlls::winusb::open(&dir) {
            Ok(f) => {
                crate::process::set_last_error(0);
                let h = insert_file(f);
                if _flags & 0x4000_0000 != 0 {
                    mark_overlapped_handle(h);
                }
                h
            }
            Err(_) => {
                crate::process::set_last_error(5);
                usize::MAX
            }
        };
    }
    // \\.\C: et \\.\PhysicalDriveN : un handle pour DeviceIoControl, posé
    // sur la racine du lecteur ou l'entrée /sys/block du disque.
    if let Some(dir) = crate::dlls::devio::device_target(path) {
        return match std::fs::File::open(&dir) {
            Ok(f) => {
                crate::process::set_last_error(0);
                insert_file(f)
            }
            Err(_) => {
                crate::process::set_last_error(5);
                usize::MAX
            }
        };
    }
    // CONIN$ / CONOUT$ : la console, c'est-à-dire le terminal ; sans
    // terminal (sortie redirigée, appli graphique), l'entrée ou la sortie
    // standard, et surtout pas un fichier nommé « CONOUT$ ».
    let device = path.trim_start_matches("\\\\.\\").to_ascii_uppercase();
    if device == "CONIN$" {
        crate::process::set_last_error(0);
        return STD_INPUT_REAL;
    }
    if device == "CONOUT$" {
        crate::process::set_last_error(0);
        return STD_OUTPUT_REAL;
    }
    let p = crate::paths::to_linux(path);
    let existed = p.exists();
    // Fichier en lecture seule : refusé en écriture même pour root, comme
    // l'attribut FILE_ATTRIBUTE_READONLY de Windows.
    if want_write_access(access) {
        if let Ok(m) = std::fs::metadata(&p) {
            if m.is_file() && crate::dlls::winfile::no_write_bits(&m) {
                crate::process::set_last_error(5);
                return usize::MAX;
            }
        }
    }
    // .exe ou DLL chargé par un processus vivant : ni écriture ni
    // troncature (CREATE_ALWAYS 2, TRUNCATE_EXISTING 5), comme sous
    // Windows (#1917).
    if existed && (want_write_access(access) || matches!(creation, 2 | 5)) {
        if let Some(code) = crate::image_lock::refuse_write(&p, "CreateFile") {
            crate::process::set_last_error(code);
            return usize::MAX;
        }
    }
    let want_read = access & (0x8000_0000 | 0x1000_0000 | 0x0200_0000 | 1) != 0;
    // GENERIC_WRITE, GENERIC_ALL, MAXIMUM_ALLOWED ou FILE_WRITE_DATA.
    let want_write = access & (0x4000_0000 | 0x1000_0000 | 0x0200_0000 | 2) != 0;
    // FILE_APPEND_DATA seul (sans FILE_WRITE_DATA) : chaque écriture va en
    // fin de fichier, comme O_APPEND. C'est ce qu'utilise appendFile() de
    // Node/Bun (journaux, transcriptions de session de Claude Code).
    let append_only = access & 4 != 0 && !want_write;
    let mut o = OpenOptions::new();
    o.read(want_read || !(want_write || append_only))
        .write(want_write);
    if append_only {
        o.append(true);
    }
    use std::os::unix::fs::OpenOptionsExt;
    // FILE_FLAG_OPEN_REPARSE_POINT : on ouvre le lien symbolique lui-même
    // (lstat() de Node/Bun, lecture de la cible avec DeviceIoControl).
    if _flags & 0x0020_0000 != 0
        && p.symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        && !want_write
        && !append_only
    {
        o.custom_flags(libc::O_PATH | libc::O_NOFOLLOW);
    }
    // Un dossier s'ouvre avec FILE_FLAG_BACKUP_SEMANTICS, y compris pour
    // en modifier les attributs ou les dates : sous Linux, en lecture seule.
    let is_dir = p.is_dir();
    if is_dir {
        if _flags & 0x0200_0000 == 0 {
            crate::process::set_last_error(5);
            return usize::MAX;
        }
        if matches!(creation, 1) {
            crate::process::set_last_error(80); // ERROR_FILE_EXISTS
            return usize::MAX;
        }
        o.read(true)
            .write(false)
            .append(false)
            .custom_flags(libc::O_DIRECTORY);
    }
    // Rust exige `write` pour créer ou tronquer : CreateFile(GENERIC_READ,
    // OPEN_ALWAYS) sur un fichier absent échouait avec 87.
    let needs_write = match creation {
        1 | 4 => !existed,
        2 | 5 => true,
        _ => false,
    };
    if needs_write && !want_write && !append_only && !is_dir {
        o.write(true);
    }
    match creation {
        1 => {
            o.create_new(true);
        }
        2 => {
            o.create(true).truncate(true);
        }
        3 => {}
        4 => {
            o.create(true);
        }
        5 => {
            o.truncate(true);
        }
        _ => {
            crate::process::set_last_error(87);
            return usize::MAX;
        }
    }
    let f = match o.open(&p) {
        Ok(f) => f,
        // CREATE_NEW sur un fichier existant : ERROR_FILE_EXISTS (80), pas
        // ERROR_ALREADY_EXISTS (183), réservé à CREATE_ALWAYS/OPEN_ALWAYS
        // (documentation de CreateFile). Sumatra PDF portable en dépend
        // pour savoir si son dossier est inscriptible (#1252).
        Err(e) if creation == 1 && e.kind() == std::io::ErrorKind::AlreadyExists => {
            crate::process::set_last_error(80);
            return usize::MAX;
        }
        Err(e) => {
            // Dossier parent absent : ERROR_PATH_NOT_FOUND (3), comme
            // Windows, et non ERROR_FILE_NOT_FOUND (2) (#1674).
            let code = if e.kind() == std::io::ErrorKind::NotFound
                && p.parent()
                    .is_some_and(|d| !d.as_os_str().is_empty() && !d.is_dir())
            {
                3
            } else {
                io_error_code(&e)
            };
            // Sous WINLUX_TRACE : quel fichier, pour quel accès (#1663).
            crate::trace!(
                "CreateFile({path} → {}, disposition {creation}) : erreur {code} ({e})",
                p.display()
            );
            crate::process::set_last_error(code);
            return usize::MAX;
        }
    };
    if !existed {
        crate::trace!("CreateFile({path} → {}) : fichier créé", p.display());
    }
    // CREATE_ALWAYS / OPEN_ALWAYS : ERROR_ALREADY_EXISTS si le fichier
    // existait, 0 s'il vient d'être créé (documentation de CreateFile) ;
    // l'erreur d'une tentative précédente ne doit pas rester (#905). Les
    // autres dispositions réussies rendent 0 aussi (#1315).
    crate::process::set_last_error(if (creation == 2 || creation == 4) && existed {
        183
    } else {
        0
    });
    let h = next_handle();
    lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())))
        .insert(h, FileHandle::File(Arc::new(Mutex::new(f))));
    if _flags & 0x4000_0000 != 0 {
        lock(overlapped_handles()).insert(h);
    }
    if _flags & FILE_FLAG_DELETE_ON_CLOSE != 0 {
        crate::trace!("CreateFile({path}) : effacé à la fermeture");
        crate::dlls::doomed::register(h, p);
    }
    h
}

/// FILE_FLAG_DELETE_ON_CLOSE : le fichier disparaît à la fermeture de son
/// handle (documentation de CreateFile, #1252).
const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;

/// Nom actuel et identité (st_dev, st_ino) d'un fichier ouvert avec
/// effacement à la fermeture. Rien si le fichier n'a plus de nom
/// (déjà effacé : /proc rend « … (deleted) »), #1885.
pub(crate) fn doomed_on_close(fd: i32) -> Option<(std::path::PathBuf, u64, u64)> {
    if fd < 0 {
        return None;
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 || st.st_nlink == 0 {
        return None;
    }
    let path = crate::dlls::ntdll_file::path_of_fd(fd)?;
    if path.to_string_lossy().ends_with(" (deleted)") {
        return None;
    }
    Some((path, st.st_dev, st.st_ino))
}

pub(crate) fn delete_on_close() -> &'static Mutex<HashMap<usize, std::path::PathBuf>> {
    static S: OnceLock<Mutex<HashMap<usize, std::path::PathBuf>>> = OnceLock::new();
    S.get_or_init(Default::default)
}
pub unsafe extern "win64" fn get_file_size(h: usize, high: *mut u32) -> u32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return u32::MAX;
        };
        f.clone()
    };
    let result = { lock(&f).metadata() };
    match result {
        Ok(m) => {
            let n = m.len();
            if !high.is_null() {
                *high = (n >> 32) as u32;
            }
            n as u32
        }
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            u32::MAX
        }
    }
}
pub unsafe extern "win64" fn set_end_of_file(h: usize) -> i32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let mut file = lock(&f);
    let position = match file.stream_position() {
        Ok(p) => p,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            return 0;
        }
    };
    match file.set_len(position) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
fn system_time_to_filetime(t: std::time::SystemTime) -> u64 {
    const EPOCH: u64 = 116_444_736_000_000_000;
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    EPOCH
        .saturating_add(d.as_secs().saturating_mul(10_000_000))
        .saturating_add((d.subsec_nanos() / 100) as u64)
}
fn filetime_parts(t: u64) -> FileTime {
    FileTime {
        low: t as u32,
        high: (t >> 32) as u32,
    }
}
fn metadata_to_find_data(attributes: u32, metadata: &std::fs::Metadata) -> Win32FileAttributeData {
    let zero = std::time::UNIX_EPOCH;
    Win32FileAttributeData {
        attributes,
        creation_time: filetime_parts(system_time_to_filetime(
            metadata
                .created()
                .unwrap_or_else(|_| metadata.modified().unwrap_or(zero)),
        )),
        access_time: filetime_parts(system_time_to_filetime(metadata.accessed().unwrap_or(zero))),
        write_time: filetime_parts(system_time_to_filetime(metadata.modified().unwrap_or(zero))),
        size_high: if metadata.is_dir() {
            0
        } else {
            (metadata.len() >> 32) as u32
        },
        size_low: if metadata.is_dir() {
            0
        } else {
            metadata.len() as u32
        },
    }
}
pub unsafe extern "win64" fn get_file_attributes_ex_a(
    path: *const u8,
    level: u32,
    data: *mut Win32FileAttributeData,
) -> i32 {
    if path.is_null() || data.is_null() || level != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let win = crate::dlls::cstr(path);
    let p = crate::paths::to_linux(&win);
    match crate::dlls::winfile::attributes(&p) {
        Ok((a, m)) => {
            std::ptr::write_unaligned(data, metadata_to_find_data(a, &m));
            1
        }
        Err(e) if builtin_attribute_data(&e, &win, data) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn get_file_attributes_ex_w(
    path: *const u16,
    level: u32,
    data: *mut Win32FileAttributeData,
) -> i32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let s = match String::from_utf16(std::slice::from_raw_parts(
        path,
        crate::printf::wide_len(path),
    )) {
        Ok(s) => s,
        Err(_) => {
            crate::process::set_last_error(1113);
            return 0;
        }
    };
    let p = crate::paths::to_linux(&s);
    if data.is_null() || level != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    match crate::dlls::winfile::attributes(&p) {
        Ok((a, m)) => {
            std::ptr::write_unaligned(data, metadata_to_find_data(a, &m));
            1
        }
        Err(e) if builtin_attribute_data(&e, &s, data) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
fn get_temp_file_name(directory: &str, prefix: &str, unique: u32) -> Result<(String, u32), u32> {
    if prefix.len() > 3 || prefix.contains(['\\', '/']) {
        return Err(87);
    }
    let dir = crate::paths::to_linux(directory);
    if !dir.is_dir() {
        return Err(3);
    }
    let attempts = if unique == 0 { 65535 } else { 1 };
    for _ in 0..attempts {
        let id = if unique == 0 {
            TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed).max(1)
        } else {
            unique
        };
        let name = format!("{prefix}{id:04X}.tmp");
        let path = dir.join(&name);
        if unique == 0 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok((crate::paths::to_windows(&path), id)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error_code(&e)),
            }
        } else {
            return Ok((crate::paths::to_windows(&path), id));
        }
    }
    Err(80)
}
pub unsafe extern "win64" fn get_temp_file_name_a(
    directory: *const u8,
    prefix: *const u8,
    unique: u32,
    out: *mut u8,
) -> u32 {
    if directory.is_null() || out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let dir = crate::dlls::cstr(directory);
    let prefix = if prefix.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(prefix)
    };
    let (path, id) = match get_temp_file_name(&dir, &prefix, unique) {
        Ok(x) => x,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let b = crate::dlls::encode_ansi(&path);
    if b.len() >= 260 {
        if unique == 0 {
            let _ = std::fs::remove_file(crate::paths::to_linux(&path));
        }
        crate::process::set_last_error(206);
        return 0;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), out, b.len());
    *out.add(b.len()) = 0;
    id
}
pub unsafe extern "win64" fn get_temp_file_name_w(
    directory: *const u16,
    prefix: *const u16,
    unique: u32,
    out: *mut u16,
) -> u32 {
    if directory.is_null() || out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let d = match String::from_utf16(std::slice::from_raw_parts(
        directory,
        crate::printf::wide_len(directory),
    )) {
        Ok(s) => s,
        Err(_) => {
            crate::process::set_last_error(1113);
            return 0;
        }
    };
    let p = if prefix.is_null() {
        String::new()
    } else {
        match String::from_utf16(std::slice::from_raw_parts(
            prefix,
            crate::printf::wide_len(prefix),
        )) {
            Ok(s) => s,
            Err(_) => {
                crate::process::set_last_error(1113);
                return 0;
            }
        }
    };
    let (path, id) = match get_temp_file_name(&d, &p, unique) {
        Ok(x) => x,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let w: Vec<u16> = path.encode_utf16().collect();
    if w.len() >= 260 {
        if unique == 0 {
            let _ = std::fs::remove_file(crate::paths::to_linux(&path));
        }
        crate::process::set_last_error(206);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
    *out.add(w.len()) = 0;
    id
}
pub unsafe extern "win64" fn set_file_pointer(
    h: usize,
    distance: i32,
    high: *mut i32,
    method: u32,
) -> u32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return u32::MAX;
        };
        f.clone()
    };
    let mut f = lock(&f);
    let d = if high.is_null() {
        distance as i64
    } else {
        (((*high as u32 as u64) << 32) | (distance as u32 as u64)) as i64
    };
    let from = match method {
        0 if d >= 0 => SeekFrom::Start(d as u64),
        0 => {
            crate::process::set_last_error(ERROR_NEGATIVE_SEEK);
            return u32::MAX;
        }
        1 => SeekFrom::Current(d),
        2 => SeekFrom::End(d),
        _ => {
            crate::process::set_last_error(87);
            return u32::MAX;
        }
    };
    match f.seek(from) {
        Ok(n) => {
            if !high.is_null() {
                *high = (n >> 32) as i32;
            }
            n as u32
        }
        Err(e) => {
            crate::process::set_last_error(seek_error_code(&e));
            u32::MAX
        }
    }
}

/// ERROR_NEGATIVE_SEEK : d'après la documentation de SetFilePointer(Ex),
/// une position résultante négative échoue avec ce code. lseek() rend
/// EINVAL dans ce cas ; le CRT de Steam x86, qui sonde la fin d'un fichier
/// neuf à -1, ne tolère que 131 et fermait son manifeste vide (#1439).
const ERROR_NEGATIVE_SEEK: u32 = 131;
fn seek_error_code(e: &std::io::Error) -> u32 {
    if e.raw_os_error() == Some(libc::EINVAL) {
        ERROR_NEGATIVE_SEEK
    } else {
        io_error_code(e)
    }
}
pub extern "win64" fn flush_file_buffers(h: usize) -> i32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let result = { lock(&f).sync_all() };
    match result {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
// ---------- Démarrage, fichiers, volumes ----------

/// `STARTUPINFOA/W` : même disposition sur 64 bits, 104 octets.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StartupInfo {
    pub cb: u32,
    pub reserved: u32,
    pub lp_reserved: usize,
    pub lp_desktop: usize,
    pub lp_title: usize,
    pub x: u32,
    pub y: u32,
    pub x_size: u32,
    pub y_size: u32,
    pub x_count: u32,
    pub y_count: u32,
    pub fill_attribute: u32,
    pub flags: u32,
    pub show_window: u16,
    pub cb_reserved2: u16,
    pub pad: u32,
    pub lp_reserved2: usize,
    pub std_input: usize,
    pub std_output: usize,
    pub std_error: usize,
}

pub(crate) fn startup_info() -> StartupInfo {
    StartupInfo {
        cb: std::mem::size_of::<StartupInfo>() as u32,
        reserved: 0,
        lp_reserved: 0,
        lp_desktop: 0,
        lp_title: 0,
        x: 0,
        y: 0,
        x_size: 0,
        y_size: 0,
        x_count: 0,
        y_count: 0,
        fill_attribute: 0,
        flags: 0,
        show_window: 0,
        cb_reserved2: 0,
        pad: 0,
        lp_reserved2: 0,
        std_input: get_std_handle(-10i32 as u32),
        std_output: get_std_handle(-11i32 as u32),
        std_error: get_std_handle(-12i32 as u32),
    }
}

unsafe fn write_startup_info(info: *mut u8) {
    if info.is_null() {
        crate::process::set_last_error(87);
        return;
    }
    std::ptr::write_unaligned(info as *mut StartupInfo, startup_info());
}

pub unsafe extern "win64" fn get_startup_info_a(info: *mut u8) {
    write_startup_info(info);
}
pub unsafe extern "win64" fn get_startup_info_w(info: *mut u8) {
    write_startup_info(info);
}

pub unsafe extern "win64" fn set_file_pointer_ex(
    h: usize,
    distance: i64,
    new_position: *mut i64,
    method: u32,
) -> i32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let from = match method {
        0 if distance >= 0 => SeekFrom::Start(distance as u64),
        0 => {
            crate::process::set_last_error(ERROR_NEGATIVE_SEEK);
            return 0;
        }
        1 => SeekFrom::Current(distance),
        2 => SeekFrom::End(distance),
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    let mut guard = lock(&f);
    let result = guard.seek(from);
    match result {
        Ok(n) => {
            if !new_position.is_null() {
                *new_position = n as i64;
            }
            1
        }
        Err(e) => {
            crate::process::set_last_error(seek_error_code(&e));
            0
        }
    }
}

pub unsafe extern "win64" fn get_file_size_ex(h: usize, out: *mut i64) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let result = lock(&f).metadata();
    match result {
        Ok(m) => {
            *out = m.len() as i64;
            1
        }
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}

/// FILE_TYPE_DISK (1), FILE_TYPE_CHAR (2) ou FILE_TYPE_PIPE (3) d'après le
/// descripteur Linux : une console est un caractère, un tube ou une socket
/// un tube, le reste un disque.
fn file_type_of_fd(fd: i32) -> u32 {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return 0;
    }
    match st.st_mode & libc::S_IFMT {
        libc::S_IFCHR => 2,
        libc::S_IFIFO | libc::S_IFSOCK => 3,
        _ => 1,
    }
}
pub extern "win64" fn get_file_type(h: usize) -> u32 {
    let h = canon_std(h);
    let std_fd = match h as isize {
        -10 => Some(0),
        -11 => Some(1),
        -12 => Some(2),
        _ => None,
    };
    let fd = std_fd.or_else(|| fd_of_handle(h));
    match fd.map(file_type_of_fd) {
        Some(0) | None => {
            crate::process::set_last_error(6);
            0
        }
        Some(kind) => {
            crate::process::set_last_error(0);
            kind
        }
    }
}

pub unsafe extern "win64" fn set_file_attributes_a(path: *const u8, attributes: u32) -> i32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    set_file_attributes_str(&crate::dlls::cstr(path), attributes)
}
/// Corps commun de `set_file_attributes_a` et `set_file_attributes_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
unsafe fn set_file_attributes_str(path: &str, attributes: u32) -> i32 {
    let p = crate::paths::to_linux(path);
    let m = match std::fs::metadata(&p) {
        Ok(m) => m,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            return 0;
        }
    };
    // Windows ignore la lecture seule sur un dossier : ne pas le verrouiller.
    if m.is_dir() {
        return 1;
    }
    // Lecture seule : on retire tous les droits d'écriture. Sinon on rend
    // l'écriture au seul propriétaire (jamais « écriture pour tous », que
    // ferait `set_readonly(false)`).
    use std::os::unix::fs::PermissionsExt;
    let mode = m.permissions().mode();
    let new_mode = if attributes & 0x1 != 0 {
        mode & !0o222
    } else if mode & 0o222 == 0 {
        mode | 0o200
    } else {
        mode
    };
    if new_mode == mode {
        return 1;
    }
    match std::fs::set_permissions(&p, std::fs::Permissions::from_mode(new_mode)) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}

pub unsafe extern "win64" fn set_file_attributes_w(path: *const u16, attributes: u32) -> i32 {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    set_file_attributes_str(&s.to_string_lossy(), attributes)
}

pub unsafe extern "win64" fn get_file_time(
    h: usize,
    creation: *mut FileTime,
    access: *mut FileTime,
    write: *mut FileTime,
) -> i32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let result = lock(&f).metadata();
    match result {
        Ok(m) => {
            let zero = std::time::UNIX_EPOCH;
            if !creation.is_null() {
                std::ptr::write_unaligned(
                    creation,
                    filetime_parts(system_time_to_filetime(m.created().unwrap_or(zero))),
                );
            }
            if !access.is_null() {
                std::ptr::write_unaligned(
                    access,
                    filetime_parts(system_time_to_filetime(m.accessed().unwrap_or(zero))),
                );
            }
            if !write.is_null() {
                std::ptr::write_unaligned(
                    write,
                    filetime_parts(system_time_to_filetime(m.modified().unwrap_or(zero))),
                );
            }
            1
        }
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}

pub unsafe extern "win64" fn set_file_time(
    h: usize,
    _creation: *const FileTime,
    _access: *const FileTime,
    write: *const FileTime,
) -> i32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let Some(ft) = filetime_value(write) else {
        return 1;
    };
    let when = system_time_from_filetime(ft);
    let file = lock(&f);
    let result = file.set_modified(when);
    match result {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}

fn filetime_value(ft: *const FileTime) -> Option<u64> {
    if ft.is_null() {
        return None;
    }
    Some(unsafe { ((*ft).high as u64) << 32 | (*ft).low as u64 })
}

const FILETIME_EPOCH: u64 = 116_444_736_000_000_000;

fn system_time_from_filetime(t: u64) -> std::time::SystemTime {
    if t >= FILETIME_EPOCH {
        std::time::UNIX_EPOCH
            + std::time::Duration::from_nanos(t.saturating_sub(FILETIME_EPOCH).saturating_mul(100))
    } else {
        std::time::UNIX_EPOCH
    }
}

pub unsafe extern "win64" fn lock_file(
    h: usize,
    _offset_low: u32,
    _offset_high: u32,
    _length_low: u32,
    _length_high: u32,
) -> i32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&*lock(&f));
    if libc::flock(fd, libc::LOCK_EX) == 0 {
        1
    } else {
        crate::process::set_last_error(33);
        0
    }
}

pub unsafe extern "win64" fn unlock_file(
    h: usize,
    _offset_low: u32,
    _offset_high: u32,
    _length_low: u32,
    _length_high: u32,
) -> i32 {
    let f = {
        let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
        let Some(FileHandle::File(f)) = handles.get(&h) else {
            crate::process::set_last_error(6);
            return 0;
        };
        f.clone()
    };
    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&*lock(&f));
    if libc::flock(fd, libc::LOCK_UN) == 0 {
        1
    } else {
        crate::process::set_last_error(33);
        0
    }
}

pub(crate) fn disk_free(path: &str) -> Option<(u64, u64, u64)> {
    let linux = crate::paths::to_linux(path);
    let text = linux.to_string_lossy().into_owned();
    let c = std::ffi::CString::new(text).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let fragment = if s.f_frsize != 0 {
        s.f_frsize
    } else {
        s.f_bsize
    };
    Some((
        s.f_bavail as u64 * fragment as u64,
        s.f_blocks as u64 * fragment as u64,
        s.f_bfree as u64 * fragment as u64,
    ))
}

pub unsafe extern "win64" fn get_disk_free_space_ex_a(
    path: *const u8,
    available: *mut i64,
    total: *mut i64,
    free: *mut i64,
) -> i32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    get_disk_free_space_ex_str(&crate::dlls::cstr(path), available, total, free)
}
/// Corps commun de `get_disk_free_space_ex_a` et `get_disk_free_space_ex_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
unsafe fn get_disk_free_space_ex_str(
    path: &str,
    available: *mut i64,
    total: *mut i64,
    free: *mut i64,
) -> i32 {
    match disk_free(path) {
        Some((a, t, f)) => {
            if !available.is_null() {
                *available = a as i64;
            }
            if !total.is_null() {
                *total = t as i64;
            }
            if !free.is_null() {
                *free = f as i64;
            }
            1
        }
        None => {
            crate::process::set_last_error(3);
            0
        }
    }
}

pub unsafe extern "win64" fn get_disk_free_space_ex_w(
    path: *const u16,
    available: *mut i64,
    total: *mut i64,
    free: *mut i64,
) -> i32 {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    get_disk_free_space_ex_str(&s.to_string_lossy(), available, total, free)
}

unsafe fn write_volume_info_a(
    label: *mut u8,
    label_size: u32,
    serial: *mut u32,
    max_component: *mut u32,
    fs_flags: *mut u32,
    fs_name: *mut u8,
    fs_size: u32,
) {
    if !label.is_null() {
        let _ = write_a_text("WINLUX", label, label_size);
    }
    if !serial.is_null() {
        *serial = 0x5731_4E4C;
    }
    if !max_component.is_null() {
        *max_component = 255;
    }
    if !fs_flags.is_null() {
        *fs_flags = 0;
    }
    if !fs_name.is_null() {
        let _ = write_a_text("WINLUXFS", fs_name, fs_size);
    }
}

pub unsafe extern "win64" fn get_volume_information_a(
    root: *const u8,
    label: *mut u8,
    label_size: u32,
    serial: *mut u32,
    max_component: *mut u32,
    flags: *mut u32,
    fs: *mut u8,
    fs_size: u32,
) -> i32 {
    if root.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let p = crate::paths::to_linux(&crate::dlls::cstr(root));
    if !p.exists() {
        crate::process::set_last_error(3);
        return 0;
    }
    write_volume_info_a(label, label_size, serial, max_component, flags, fs, fs_size);
    1
}

pub unsafe extern "win64" fn get_volume_information_w(
    root: *const u16,
    label: *mut u16,
    label_size: u32,
    serial: *mut u32,
    max_component: *mut u32,
    flags: *mut u32,
    fs: *mut u16,
    fs_size: u32,
) -> i32 {
    let s = match cstring_from_wide(root) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let p = crate::paths::to_linux(&s.to_string_lossy());
    if !p.exists() {
        crate::process::set_last_error(3);
        return 0;
    }
    if !label.is_null() {
        let _ = write_w_text("WINLUX", label, label_size);
    }
    if !serial.is_null() {
        *serial = 0x5731_4E4C;
    }
    if !max_component.is_null() {
        *max_component = 255;
    }
    if !flags.is_null() {
        *flags = 0;
    }
    if !fs.is_null() {
        let _ = write_w_text("WINLUXFS", fs, fs_size);
    }
    1
}

pub unsafe extern "win64" fn get_drive_type_a(root: *const u8) -> u32 {
    if root.is_null() {
        crate::process::set_last_error(87);
        return 1;
    }
    crate::mounts::drive_type((*root as char).to_ascii_uppercase()).unwrap_or(3)
}
pub unsafe extern "win64" fn get_drive_type_w(root: *const u16) -> u32 {
    if root.is_null() {
        crate::process::set_last_error(87);
        return 1;
    }
    let c = char::from_u32(*root as u32).unwrap_or('C');
    crate::mounts::drive_type(c.to_ascii_uppercase()).unwrap_or(3)
}

fn move_file_ex_impl(src: &str, dst: &str, flags: u32) -> i32 {
    let r = move_file_ex_inner(src, dst, flags);
    crate::trace!(
        "MoveFileEx({src} -> {dst}, {flags:#x}) = {r} (erreur {})",
        if r == 0 {
            crate::process::last_error()
        } else {
            0
        }
    );
    r
}

fn move_file_ex_inner(src: &str, dst: &str, flags: u32) -> i32 {
    let s = crate::paths::to_linux(src);
    let d = crate::paths::to_linux(dst);
    if d.exists() {
        if flags & 0x1 == 0 {
            // Sans MOVEFILE_REPLACE_EXISTING, Windows refuse (183) : rename(2)
            // écraserait silencieusement la destination.
            crate::process::set_last_error(183);
            return 0;
        }
        let _ = std::fs::remove_file(&d);
    }
    match std::fs::rename(&s, &d) {
        Ok(()) => 1,
        Err(_) if flags & 0x2 != 0 => match copy_keeping_write_time(&s, &d) {
            Ok(_) => {
                let _ = std::fs::remove_file(&s);
                1
            }
            Err(e) => {
                crate::process::set_last_error(io_error_code(&e));
                0
            }
        },
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}

pub unsafe extern "win64" fn move_file_ex_a(src: *const u8, dst: *const u8, flags: u32) -> i32 {
    if src.is_null() || dst.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    move_file_ex_impl(&crate::dlls::cstr(src), &crate::dlls::cstr(dst), flags)
}

pub unsafe extern "win64" fn move_file_ex_w(src: *const u16, dst: *const u16, flags: u32) -> i32 {
    let s = match cstring_from_wide(src) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let d = match cstring_from_wide(dst) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    move_file_ex_impl(&s.to_string_lossy(), &d.to_string_lossy(), flags)
}

pub extern "win64" fn exit_process(code: u32) -> ! {
    flush_and_exit(code as i32)
}
pub extern "win64" fn get_command_line_a() -> usize {
    crate::process::PROCESS
        .get()
        .map(|p| p.command_line_a.as_ptr() as usize)
        .unwrap_or(0)
}
pub extern "win64" fn get_command_line_w() -> usize {
    crate::process::PROCESS
        .get()
        .map(|p| p.command_line_w.as_ptr() as usize)
        .unwrap_or(0)
}
pub unsafe extern "win64" fn get_module_handle_a(name: *const u8) -> usize {
    if name.is_null() {
        return crate::loader::main_module().map(|m| m.base).unwrap_or(0);
    }
    get_module_handle_str(&crate::dlls::cstr(name))
}
/// Corps commun de `get_module_handle_a` et `get_module_handle_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
unsafe fn get_module_handle_str(name: &str) -> usize {
    let n = name;
    crate::dlls::module_handle(n).unwrap_or(0)
}
pub unsafe extern "win64" fn get_module_handle_w(name: *const u16) -> usize {
    if name.is_null() {
        return get_module_handle_a(std::ptr::null());
    }
    let s = match cstring_from_wide(name) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    get_module_handle_str(&s.to_string_lossy())
}
pub unsafe extern "win64" fn get_proc_address(module: usize, name: *const u8) -> usize {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let by_ordinal = (name as usize) <= 0xffff;
    if let Some(dll) = crate::dlls::builtin_by_handle(module) {
        if by_ordinal {
            let symbol = format!("#{}", name as usize);
            if let Some(a) = crate::dlls::lookup(dll, &symbol) {
                return crate::thunks::api_trace(dll, &symbol, a);
            }
            crate::thunks::api_trace_missing(dll, &symbol);
            crate::process::set_last_error(127);
            return 0;
        }
        let symbol = crate::dlls::cstr(name);
        // kernel32.dll n'exporte ni les fonctions propres à KernelBase, ni
        // celles de psapi, advapi32 ou ntdll rangées dans sa table (#521).
        let found = if dll == "kernel32" {
            crate::dlls::kernel32_proc(&symbol, module == crate::dlls::kernelbase_handle())
        } else {
            crate::dlls::lookup(dll, &symbol)
        };
        return match found {
            Some(a) => crate::thunks::api_trace(dll, &symbol, a),
            None => {
                crate::trace!("GetProcAddress: {dll}!{symbol} non disponible");
                crate::thunks::api_trace_missing(dll, &symbol);
                if let Some(stub) = crate::thunks::getproc_stub(dll, &symbol) {
                    return stub;
                }
                crate::process::set_last_error(127);
                0
            }
        };
    }
    let Some(m) = crate::loader::find_module_by_addr(module).filter(|m| m.base == module) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let found = if by_ordinal {
        crate::loader::export_by_ordinal(&m, name as usize as u32)
    } else {
        crate::loader::export_by_name(&m, &crate::dlls::cstr(name))
    };
    found.unwrap_or_else(|| {
        crate::process::set_last_error(127);
        0
    })
}
pub unsafe extern "win64" fn load_library_a(name: *const u8) -> usize {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    load_library_str(&crate::dlls::cstr(name))
}
/// Programme qui charge lui-même OpenGL, Vulkan ou Direct3D (SDL lié en
/// statique, comme Xonotic) : il dessine sa fenêtre, qui reste donc à
/// l'échelle 1, comme pour un import direct de ces DLL (#1421). SDL charge
/// opengl32 avant de créer sa fenêtre.
fn note_renders_itself(name: &str) {
    let base = name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    let base = if base.contains('.') {
        base
    } else {
        format!("{base}.dll")
    };
    if crate::dlls::gui_x11::renders_itself(&[base]) {
        crate::dlls::gui_x11::RENDERS_ITSELF.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
/// Corps commun de `load_library_a` et `load_library_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
unsafe fn load_library_str(name: &str) -> usize {
    let n = name;
    if n.is_empty() {
        crate::process::set_last_error(87);
        return 0;
    }
    note_renders_itself(n);
    let real_first = crate::dlls::prefers_real_file(&crate::dlls::normalize(n))
        && crate::loader::main_module()
            .is_some_and(|m| crate::loader::find_dll_file(n, &m.path).is_some());
    if let Some(h) = crate::dlls::module_handle(n).filter(|_| !real_first) {
        // Une DLL réelle libérée par FreeLibrary (DLL_PROCESS_DETACH déjà
        // reçu) puis rechargée : Windows la reprojette et rappelle son
        // DllMain ; ses objets globaux C++ détruits sont reconstruits.
        let reattach = {
            let mut mods = crate::loader::MODULES.lock().unwrap();
            mods.iter_mut().find(|m| m.base == h).and_then(|m| {
                m.refcount = m.refcount.saturating_add(1);
                (!m.initialized).then(|| m.clone())
            })
        };
        if let Some(m) = reattach {
            if !initialize_with_dependencies(&m, &[]) {
                crate::process::set_last_error(1114);
                return 0;
            }
        }
        return h;
    }
    let Some(importer) = crate::loader::main_module() else {
        crate::process::set_last_error(126);
        return 0;
    };
    let Some(path) = crate::loader::find_dll_file(n, &importer.path) else {
        crate::process::set_last_error(126);
        return 0;
    };
    let before: Vec<usize> = crate::loader::MODULES
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.base)
        .collect();
    match crate::loader::load(&path, false) {
        Ok(m) => {
            if initialize_with_dependencies(&m, &before) {
                m.base
            } else {
                crate::log_warn!(
                    "loader",
                    "LoadLibrary {} : DllMain ou une dépendance a refusé l'initialisation (1114)",
                    path.display()
                );
                crate::process::set_last_error(1114);
                0
            }
        }
        Err(e) => {
            // La raison était perdue : seul le code 126 remontait (Spotify,
            // #1809). Elle va au journal, comme les imports manquants.
            crate::log_warn!("loader", "LoadLibrary {} : {e} (126)", path.display());
            crate::process::set_last_error(126);
            0
        }
    }
}
/// DllMain (et rappels TLS) de `m` et des DLL réelles que son chargement a
/// amenées, dépendances d'abord, comme le chargeur de Windows. Sans cela, une
/// DLL chargée seulement comme dépendance d'un LoadLibrary n'était jamais
/// initialisée : ses objets globaux C++ restaient à zéro. C'était le
/// plantage d'Audacity au premier lancement (#691) : la clé de
/// ProjectCloudExtension, globale de lib-cloud-audiocom.dll amenée par
/// mod-cloud-audiocom.dll, valait 0 et désignait un autre objet du projet.
fn initialize_with_dependencies(m: &crate::loader::Module, before: &[usize]) -> bool {
    let pending: Vec<crate::loader::Module> = crate::loader::MODULES
        .lock()
        .unwrap()
        .iter()
        .filter(|x| x.base != m.base && !x.initialized && x.refcount > 0)
        .filter(|x| !before.is_empty() && !before.contains(&x.base))
        .cloned()
        .collect();
    for dep in dependency_order(m, &pending) {
        if !crate::process::initialize_dynamic_module(dep) {
            crate::trace!(
                "DllMain de {} (dépendance de {}) a échoué",
                dep.name,
                m.name
            );
            return false;
        }
    }
    crate::process::initialize_dynamic_module(m)
}

/// Ordre d'initialisation des dépendances `pending` de `m` : chacune après
/// les DLL qu'elle importe, comme le chargeur de Windows (parcours en
/// profondeur des imports, suffixe). L'ordre de projection ne suffit pas :
/// steamui.dll importe tier0_s64.dll puis vstdlib_s64.dll, qui importe
/// aussi tier0 ; à rebours de la projection, vstdlib passait avant tier0 et
/// son DllMain lisait une variable de tier0 encore à zéro (#1584). Les DLL
/// qu'aucun import n'atteint (transferts, chargements faits par un DllMain)
/// restent dans l'ordre inverse de leur projection.
pub(crate) fn dependency_order<'a>(
    m: &crate::loader::Module,
    pending: &'a [crate::loader::Module],
) -> Vec<&'a crate::loader::Module> {
    let names: Vec<String> = pending.iter().map(|x| x.name.clone()).collect();
    init_order(&m.import_names(), &names, &|i| pending[i].import_names())
        .into_iter()
        .map(|i| &pending[i])
        .collect()
}

/// Cœur de `dependency_order`, sur les noms : `roots` = imports du module
/// chargé, `names[i]` = nom de la i-ème DLL à initialiser (dans l'ordre de
/// projection), `imports(i)` = ses imports. Rend les indices dans l'ordre
/// des DllMain.
fn init_order(
    roots: &[String],
    names: &[String],
    imports: &dyn Fn(usize) -> Vec<String>,
) -> Vec<usize> {
    let keys: Vec<String> = names.iter().map(|n| crate::dlls::normalize(n)).collect();
    let find = |name: &str| {
        let n = crate::dlls::normalize(name);
        keys.iter().position(|k| *k == n)
    };
    let mut seen = vec![false; names.len()];
    let mut order = Vec::with_capacity(names.len());
    // Pile explicite (pas de récursion) : (indice, imports déjà parcourus).
    let visit = |start: usize, seen: &mut Vec<bool>, order: &mut Vec<usize>| {
        if seen[start] {
            return;
        }
        seen[start] = true;
        let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
        // À rebours : `pop` rend alors les imports dans leur ordre.
        let deps =
            |i: usize| -> Vec<usize> { imports(i).iter().rev().filter_map(|n| find(n)).collect() };
        stack.push((start, deps(start)));
        while let Some((i, todo)) = stack.last_mut() {
            if let Some(j) = todo.pop() {
                if !seen[j] {
                    seen[j] = true;
                    let d = deps(j);
                    stack.push((j, d));
                }
            } else {
                order.push(*i);
                stack.pop();
            }
        }
    };
    for name in roots {
        if let Some(j) = find(name) {
            visit(j, &mut seen, &mut order);
        }
    }
    for j in (0..names.len()).rev() {
        visit(j, &mut seen, &mut order);
    }
    order
}
pub unsafe extern "win64" fn load_library_w(name: *const u16) -> usize {
    let s = match cstring_from_wide(name) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    load_library_str(&s.to_string_lossy())
}
pub unsafe extern "win64" fn load_library_ex_a(name: *const u8, file: usize, flags: u32) -> usize {
    let h = load_library_ex_inner(name, file, flags);
    if h == 0 && !name.is_null() {
        crate::trace!(
            "LoadLibraryEx({}, 0x{flags:x}) : échec {}",
            crate::dlls::cstr(name),
            crate::process::last_error()
        );
    }
    h
}
unsafe fn load_library_ex_inner(name: *const u8, file: usize, flags: u32) -> usize {
    if name.is_null() || file != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let value = crate::dlls::cstr(name);
    if value.is_empty() {
        crate::process::set_last_error(87);
        return 0;
    }
    // Nom et drapeaux demandés, pour WINLUX_TRACE=1 (#1019).
    crate::trace!("LoadLibraryEx({value}, 0x{flags:x})");
    // LOAD_IGNORE_CODE_AUTHZ_LEVEL : sans effet.
    let flags = flags & !0x10;
    if flags == 0 {
        return load_library_a(name);
    }
    // LOAD_LIBRARY_AS_DATAFILE(_EXCLUSIVE) et LOAD_LIBRARY_AS_IMAGE_RESOURCE :
    // ressources seulement (mscorrc.dll de .NET, fichiers .mui, icônes).
    const AS_DATA: u32 = 0x2 | 0x20 | 0x40;
    if flags & AS_DATA != 0 {
        return load_as_data(&value, flags);
    }
    // DONT_RESOLVE_DLL_REFERENCES : image projetée sans imports ni DllMain,
    // mais vue comme un module (GetModuleFileName, ressources). Le
    // lanceur CEF de Spotify charge ainsi Spotify.dll pour vérifier son
    // emplacement et sa signature avant le vrai chargement (#1809).
    if flags & 0x1 != 0 {
        return load_as_data(&value, 0);
    }
    // Never execute DLL entry points for resource/data-only loading requests.
    // Signature flags are not supported; return an error rather than
    // searching an unsafe fallback path. 0x400 = AddDllDirectory dirs.
    const SEARCH: u32 = 0x100 | 0x200 | 0x400 | 0x800 | 0x1000;
    if flags & !(SEARCH | 8) != 0 {
        crate::process::set_last_error(50);
        return 0;
    }
    if flags & 8 != 0 && flags & SEARCH != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut directories = Vec::new();
    if flags & (8 | 0x100) != 0 {
        // Converti seulement ici, où le nom doit être un chemin absolu : un
        // simple nom de DLL (« api-ms-win-core-synch-l1-2-0 » demandé par la
        // CRT avec LOAD_LIBRARY_SEARCH_SYSTEM32) devenait un chemin relatif
        // au dossier courant. Lancé depuis le Bureau, claude.exe attendait
        // alors l'autorisation « fichiers personnels » avant même son main
        // (#1019).
        let is_path = value.contains(['\\', '/']) || value.as_bytes().get(1) == Some(&b':');
        let path = is_path.then(|| crate::paths::to_linux(&value));
        match path {
            Some(path) if path.is_absolute() => {
                if let Some(parent) = path.parent() {
                    directories.push(parent.to_path_buf());
                }
            }
            // LOAD_WITH_ALTERED_SEARCH_PATH avec un simple nom : recherche
            // standard, comme Windows.
            None if flags & 0x100 == 0 => {}
            _ => {
                crate::process::set_last_error(87);
                return 0;
            }
        }
    }
    if flags & (8 | 0x200 | 0x1000) != 0 {
        if let Some(module) = crate::loader::main_module() {
            if let Some(parent) = module.path.parent() {
                directories.push(parent.to_path_buf());
            }
        }
    }
    if flags & (0x400 | 0x1000) != 0 {
        directories.extend(crate::loader::user_dll_directories());
    }
    if flags & (8 | 0x800 | 0x1000) != 0 {
        directories.push(crate::paths::to_linux("C:\\Windows\\System32"));
    }
    crate::loader::with_dll_search(directories, || load_library_a(name))
}
/// LoadLibraryEx en mode données : l'image est projetée en lecture seule,
/// sans imports ni DllMain ; le handle porte le bit 1 (fichier de données)
/// ou 2 (image de ressources) et sert aux fonctions de ressources.
fn load_as_data(value: &str, flags: u32) -> usize {
    // Une DLL déjà chargée normalement est renvoyée telle quelle.
    if let Some(h) = crate::dlls::module_handle(value) {
        crate::loader::increment_ref(h);
        return h;
    }
    let importer = crate::loader::main_module()
        .map(|m| m.path)
        .unwrap_or_else(|| crate::paths::to_linux("C:\\Windows\\System32\\winlux.exe"));
    let path = crate::loader::find_dll_file(value, &importer).or_else(|| {
        let system = crate::paths::to_linux("C:\\Windows\\System32");
        crate::loader::with_dll_search(vec![system], || {
            crate::loader::find_dll_file(value, &importer)
        })
    });
    let Some(path) = path else {
        crate::process::set_last_error(2);
        return 0;
    };
    // 0 : DONT_RESOLVE_DLL_REFERENCES, handle sans marque, comme une base
    // d'image ordinaire.
    let tag = if flags == 0 {
        0
    } else if flags & 0x20 != 0 {
        2
    } else {
        1
    };
    match crate::loader::load_data(&path, tag) {
        Ok(handle) => handle,
        Err(error) => {
            crate::trace!("LoadLibraryEx (données) {} : {error}", path.display());
            crate::process::set_last_error(193);
            0
        }
    }
}
pub unsafe extern "win64" fn load_library_ex_w(name: *const u16, file: usize, flags: u32) -> usize {
    let name = match cstring_from_wide(name) {
        Ok(value) => value,
        Err(error) => {
            crate::process::set_last_error(error);
            return 0;
        }
    };
    load_library_ex_a(name.as_ptr().cast(), file, flags)
}
pub extern "win64" fn free_library(module: usize) -> i32 {
    if module & crate::loader::DATA_HANDLE_BITS != 0 {
        if crate::loader::free_data(module).is_some() {
            return 1;
        }
        crate::process::set_last_error(6);
        return 0;
    }
    if crate::dlls::builtin_by_handle(module).is_some() {
        return 1;
    }
    // Image DONT_RESOLVE_DLL_REFERENCES : handle sans marque (#1809).
    if crate::loader::data_image(module).is_some_and(|m| m.base == module)
        && crate::loader::free_data(module).is_some()
    {
        return 1;
    }
    if module == crate::loader::main_module().map(|m| m.base).unwrap_or(0) {
        crate::process::set_last_error(5);
        return 0;
    }
    // Le compteur est décrémenté sous MODULES, mais DllMain(DLL_PROCESS_DETACH)
    // est appelé verrou relâché : il peut lui-même charger ou libérer des
    // modules, ou créer des threads qui lisent la liste.
    let (last, detach) = {
        let mut mods = crate::loader::MODULES.lock().unwrap();
        let Some(m) = mods.iter_mut().find(|m| m.base == module) else {
            crate::process::set_last_error(6);
            return 0;
        };
        if m.refcount == 0 {
            crate::process::set_last_error(5);
            return 0;
        }
        // Module épinglé (GET_MODULE_HANDLE_EX_FLAG_PIN) : jamais déchargé.
        if m.refcount == u32::MAX {
            return 1;
        }
        m.refcount -= 1;
        let detach = if m.refcount == 0 && m.initialized {
            m.initialized = false;
            m.entry().map(|entry| (entry, m.base))
        } else {
            None
        };
        (m.refcount == 0, detach)
    };
    if let Some((entry, base)) = detach {
        // DllMain x86 ou x64 (#232).
        let _ = crate::process::call_dllmain(entry, base, 0, 0);
    }
    if last {
        // Vrai déchargement (#24) : rappels TLS et blocs TLS de chaque
        // thread, retrait de la liste, démappage, puis une référence de
        // moins sur chaque DLL que celle-ci importait. Un LoadLibrary pendant
        // DllMain a pu reprendre le module : il reste alors chargé.
        let still_unused = crate::loader::MODULES
            .lock()
            .unwrap()
            .iter()
            .any(|m| m.base == module && m.refcount == 0);
        if still_unused {
            if let Some(m) = crate::loader::find_module_by_addr(module) {
                crate::process::release_module_tls(&m);
            }
            if crate::loader::unload(module).is_some() {
                for dep in crate::loader::take_dependencies(module) {
                    free_library(dep);
                }
            }
        }
    }
    1
}
pub extern "win64" fn get_current_process() -> usize {
    (-1isize) as usize
}
pub extern "win64" fn disable_thread_library_calls(module: usize) -> i32 {
    if crate::process::disable_thread_library_calls(module) {
        1
    } else {
        crate::process::set_last_error(6);
        0
    }
}
pub extern "win64" fn get_current_thread() -> usize {
    (-2isize) as usize
}
pub extern "win64" fn get_current_thread_id() -> u32 {
    let t = crate::process::teb();
    if t != 0 {
        unsafe { *((t + crate::process::TEB_TID) as *const usize) as u32 }
    } else {
        unsafe { libc::syscall(libc::SYS_gettid) as u32 }
    }
}
pub unsafe extern "win64" fn get_system_info(out: *mut SystemInfo) {
    if out.is_null() {
        crate::process::set_last_error(87);
        return;
    }
    let n = std::thread::available_parallelism()
        .map(|x| x.get())
        .unwrap_or(1)
        .max(1);
    let mask = if n >= usize::BITS as usize {
        usize::MAX
    } else {
        (1usize << n) - 1
    };
    std::ptr::write(
        out,
        SystemInfo {
            processor_architecture: 9,
            reserved: 0,
            page_size: 4096,
            minimum_application_address: 0x10000usize as *mut c_void,
            maximum_application_address: 0x0000_7fff_ffff_efffu64 as usize as *mut c_void,
            active_processor_mask: mask,
            number_of_processors: n as u32,
            processor_type: 586,
            allocation_granularity: 65536,
            processor_level: 6,
            processor_revision: 0x3a09,
        },
    );
}
pub unsafe extern "win64" fn get_native_system_info(out: *mut SystemInfo) {
    get_system_info(out)
}
pub(crate) fn filetime_now() -> u64 {
    const EPOCH: u64 = 116_444_736_000_000_000;
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    EPOCH
        .saturating_add(d.as_secs().saturating_mul(10_000_000))
        .saturating_add((d.subsec_nanos() / 100) as u64)
}
pub unsafe extern "win64" fn get_system_time_as_file_time(out: *mut u64) {
    if out.is_null() {
        crate::process::set_last_error(87);
        return;
    }
    *out = filetime_now()
}
pub unsafe extern "win64" fn get_local_time(out: *mut SystemTime) {
    if out.is_null() {
        crate::process::set_last_error(87);
        return;
    }
    let seconds = libc::time(std::ptr::null_mut());
    let mut local: libc::tm = std::mem::zeroed();
    if seconds == (-1 as libc::time_t) || libc::localtime_r(&seconds, &mut local).is_null() {
        crate::process::set_last_error(31);
        return;
    }
    std::ptr::write(
        out,
        SystemTime {
            year: (local.tm_year + 1900).clamp(0, u16::MAX as i32) as u16,
            month: (local.tm_mon + 1).clamp(0, u16::MAX as i32) as u16,
            day_of_week: local.tm_wday.clamp(0, u16::MAX as i32) as u16,
            day: local.tm_mday.clamp(0, u16::MAX as i32) as u16,
            hour: local.tm_hour.clamp(0, u16::MAX as i32) as u16,
            minute: local.tm_min.clamp(0, u16::MAX as i32) as u16,
            second: local.tm_sec.clamp(0, u16::MAX as i32) as u16,
            milliseconds: 0,
        },
    );
}
pub extern "win64" fn get_version() -> u32 {
    10u32 | (19045u32 << 16)
}
pub unsafe extern "win64" fn get_version_ex_a(info: *mut u8) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let size = std::ptr::read_unaligned(info as *const u32);
    if size < 148 {
        crate::process::set_last_error(87);
        return 0;
    }
    std::ptr::write_unaligned(info.add(4) as *mut u32, 10);
    std::ptr::write_unaligned(info.add(8) as *mut u32, 0);
    std::ptr::write_unaligned(info.add(12) as *mut u32, 19045);
    std::ptr::write_unaligned(info.add(16) as *mut u32, 2);
    std::ptr::write_bytes(info.add(20), 0, 128);
    1
}
pub unsafe extern "win64" fn get_version_ex_w(info: *mut u16) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let size = std::ptr::read_unaligned(info as *const u32);
    if size < 276 {
        crate::process::set_last_error(87);
        return 0;
    }
    std::ptr::write_unaligned(info.add(2) as *mut u32, 10);
    std::ptr::write_unaligned(info.add(4) as *mut u32, 0);
    std::ptr::write_unaligned(info.add(6) as *mut u32, 19045);
    std::ptr::write_unaligned(info.add(8) as *mut u32, 2);
    std::ptr::write_bytes(info.add(10), 0, 128);
    1
}
pub extern "win64" fn get_last_error() -> u32 {
    crate::process::last_error()
}
pub extern "win64" fn set_last_error(code: u32) {
    crate::process::set_last_error(code)
}
pub extern "win64" fn get_process_heap() -> usize {
    process_heap()
}
/// Diagnostic `WINLUX_HEAP_DIAG` (#2052, pour #1818) : `trace` journalise chaque
/// HeapAlloc et HeapFree (pointeur, taille) ; `nofree` garde les blocs
/// libérés au lieu de les rendre à glibc, dont `free` écrit ses chaînages
/// dans les 16 premiers octets du bloc. Les deux se combinent (`trace,nofree`).
/// Sert à établir en boîte noire qu'un programme se sert d'un objet après
/// l'avoir libéré, ce que le tas de Windows laisse souvent passer.
fn heap_diag() -> (bool, bool) {
    let d = heap_diag_all();
    (d.0, d.1)
}
/// `lfh` (#2067) : comme le tas à fragmentation réduite de Windows, un bloc
/// libéré de moins de 16 Kio est gardé par classe de taille (multiple de 16)
/// et rendu tel quel, contenu intact, au prochain HeapAlloc de la même
/// classe ; glibc, elle, réécrit le début du bloc en le libérant.
fn heap_lfh() -> bool {
    heap_diag_all().2
}
fn heap_diag_all() -> (bool, bool, bool) {
    static DIAG: OnceLock<(bool, bool, bool)> = OnceLock::new();
    *DIAG.get_or_init(|| heap_diag_parse(&std::env::var("WINLUX_HEAP_DIAG").unwrap_or_default()))
}
fn heap_diag_parse(v: &str) -> (bool, bool, bool) {
    let has = |w: &str| v.split(',').any(|x| x.trim() == w);
    (has("trace"), has("nofree"), has("lfh"))
}
/// `fill` (#2156) : un bloc rendu par HeapAlloc sans HEAP_ZERO_MEMORY, ou
/// la partie agrandie par HeapReAlloc, est rempli de [`HEAP_FILL_PATTERN`].
/// Un programme qui suit un pointeur lu dans un champ jamais initialisé
/// fait alors une faute avec ce motif dans le registre fautif (Acrobat,
/// #1818). Jamais actif par défaut.
fn heap_fill() -> bool {
    static FILL: OnceLock<bool> = OnceLock::new();
    *FILL.get_or_init(|| heap_fill_parse(&std::env::var("WINLUX_HEAP_DIAG").unwrap_or_default()))
}
fn heap_fill_parse(v: &str) -> bool {
    v.split(',').any(|x| x.trim() == "fill")
}
/// Motif de `WINLUX_HEAP_DIAG=fill` : quadmot non canonique, distinct de
/// celui de WINLUX_STACK_SCRUB (0xF1F15C2B…).
pub(crate) const HEAP_FILL_PATTERN: u64 = 0xF2F2_5A11_F2F2_5A11;
/// Remplit `len` octets à `p` du motif, dans l'ordre de la mémoire (petit
/// boutiste), y compris un reste de moins de 8 octets.
unsafe fn heap_fill_bytes(p: *mut u8, len: usize) {
    let pat = HEAP_FILL_PATTERN.to_le_bytes();
    for i in 0..len {
        *p.add(i) = pat[i % 8];
    }
}
const LFH_LIMIT: usize = 16 * 1024;
const LFH_PER_CLASS: usize = 4096;
fn lfh_class(size: usize) -> usize {
    size.max(1).div_ceil(16) * 16
}
static LFH_CACHE: OnceLock<Mutex<HashMap<usize, Vec<usize>>>> = OnceLock::new();
/// Bloc d'au moins `size` octets : un bloc libéré de la même classe s'il y
/// en a un (contenu intact), sinon un bloc neuf de la taille de la classe.
unsafe fn lfh_alloc(size: usize) -> *mut c_void {
    let class = lfh_class(size);
    if class <= LFH_LIMIT {
        let reused = lock(LFH_CACHE.get_or_init(|| Mutex::new(HashMap::new())))
            .get_mut(&class)
            .and_then(|v| v.pop());
        if let Some(p) = reused {
            return p as *mut c_void;
        }
        return libc::malloc(class);
    }
    libc::malloc(size.max(1))
}
/// Libère un bloc de `size` octets demandés : gardé pour sa classe, sans
/// toucher à son contenu, ou rendu à glibc au-delà de la limite.
unsafe fn lfh_free(p: *mut c_void, size: usize) {
    let class = lfh_class(size);
    if class <= LFH_LIMIT {
        let mut cache = lock(LFH_CACHE.get_or_init(|| Mutex::new(HashMap::new())));
        let v = cache.entry(class).or_default();
        if v.len() < LFH_PER_CLASS {
            v.push(p as usize);
            return;
        }
    }
    libc::free(p);
}
#[cfg(test)]
mod tests_diagnostic_tas {
    #[test]
    fn valeurs_de_winlux_heap_diag() {
        assert_eq!(super::heap_diag_parse(""), (false, false, false));
        assert_eq!(super::heap_diag_parse("trace"), (true, false, false));
        assert_eq!(super::heap_diag_parse("nofree"), (false, true, false));
        assert_eq!(super::heap_diag_parse("trace, nofree"), (true, true, false));
        assert_eq!(super::heap_diag_parse("lfh,trace"), (true, false, true));
        assert_eq!(super::heap_diag_parse("1"), (false, false, false));
    }

    /// #2067 : un bloc libéré revient, contenu intact, au HeapAlloc suivant
    /// de la même classe de taille.
    #[test]
    fn lfh_rend_le_bloc_libere_intact() {
        unsafe {
            let p = super::lfh_alloc(40) as *mut u64;
            *p = 0x1122_3344_5566_7788;
            *p.add(1) = 0x99;
            super::lfh_free(p as *mut _, 40);
            let q = super::lfh_alloc(33) as *mut u64;
            assert_eq!(q, p, "même classe (48) : même bloc");
            assert_eq!(*q, 0x1122_3344_5566_7788);
            assert_eq!(*q.add(1), 0x99);
            super::lfh_free(q as *mut _, 33);
            let big = super::lfh_alloc(20_000);
            assert!(!big.is_null());
            super::lfh_free(big, 20_000);
        }
        assert_eq!(super::lfh_class(0), 16);
        assert_eq!(super::lfh_class(17), 32);
    }

    /// WINLUX_HEAP_DIAG=fill (#2156) : motif reconnaissable, octet par
    /// octet, y compris un reste de moins de 8 octets.
    #[test]
    fn remplissage_du_tas() {
        assert!(super::heap_fill_parse("trace,fill"));
        assert!(!super::heap_fill_parse("trace,lfh"));
        assert!(!super::heap_fill_parse(""));
        let mut b = [0u8; 21];
        unsafe { super::heap_fill_bytes(b.as_mut_ptr(), 19) };
        let pat = super::HEAP_FILL_PATTERN.to_le_bytes();
        for (i, x) in b.iter().enumerate().take(19) {
            assert_eq!(*x, pat[i % 8]);
        }
        assert_eq!(&b[19..], &[0, 0]);
        assert_eq!(
            u64::from_le_bytes(b[8..16].try_into().unwrap()),
            0xF2F2_5A11_F2F2_5A11
        );
    }
}

pub unsafe extern "win64" fn heap_alloc(_heap: usize, flags: u32, size: usize) -> *mut c_void {
    let p = if super::tas_page::enabled() {
        // WINLUX_HEAP_DIAG=page (#2158) : bloc sur ses pages, page de garde.
        super::tas_page::alloc(size).unwrap_or_else(|| libc::malloc(size.max(1)))
    } else if heap_lfh() {
        lfh_alloc(size)
    } else {
        libc::malloc(size.max(1))
    };
    if !p.is_null() {
        if flags & 8 != 0 {
            std::ptr::write_bytes(p, 0, size);
        } else if heap_fill() {
            heap_fill_bytes(p as *mut u8, size);
        }
        lock(HEAP_ALLOCS.get_or_init(|| Mutex::new(HashMap::new())))
            .insert(p as usize, size.max(1));
    }
    if heap_diag().0 {
        crate::log::emit(
            crate::log::Level::Warn,
            "tas",
            &format_args!(
                "HeapAlloc({size}, drapeaux {flags:#x}) = {p:p} (fil {})",
                libc::syscall(libc::SYS_gettid)
            ),
        );
    }
    p
}
pub unsafe extern "win64" fn heap_realloc(
    _heap: usize,
    flags: u32,
    p: *mut c_void,
    size: usize,
) -> *mut c_void {
    if p.is_null() {
        return heap_alloc(0, flags, size);
    }
    if flags & 0x10 != 0 {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    }
    // Tout sous le verrou du tas : un HeapFree/HeapSize concurrent sur le
    // même bloc ne voit jamais l'ancien pointeur après le realloc.
    let mut allocations = lock(HEAP_ALLOCS.get_or_init(|| Mutex::new(HashMap::new())));
    let Some(old) = allocations.get(&(p as usize)).copied() else {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    };
    let (trace, nofree) = heap_diag();
    let q = if super::tas_page::enabled() {
        // Mode page (#2158) : nouveau bloc, l'ancien part en quarantaine
        // inaccessible ; une lecture par l'ancien pointeur fait une faute.
        let q = super::tas_page::alloc(size).unwrap_or_else(|| libc::malloc(size.max(1)));
        if !q.is_null() {
            std::ptr::copy_nonoverlapping(p as *const u8, q as *mut u8, old.min(size));
            if !super::tas_page::free(p) && !nofree {
                libc::free(p);
            }
        }
        q
    } else if heap_lfh() {
        // Mode lfh : nouveau bloc de la bonne classe, l'ancien est gardé
        // pour sa classe avec son contenu.
        let q = lfh_alloc(size);
        if !q.is_null() {
            std::ptr::copy_nonoverlapping(p as *const u8, q as *mut u8, old.min(size));
            lfh_free(p, old);
        }
        q
    } else if nofree {
        // WINLUX_HEAP_DIAG=nofree : l'ancien bloc reste intact.
        let q = libc::malloc(size.max(1));
        if !q.is_null() {
            std::ptr::copy_nonoverlapping(p as *const u8, q as *mut u8, old.min(size));
        }
        q
    } else {
        libc::realloc(p, size.max(1))
    };
    if trace {
        crate::log::emit(
            crate::log::Level::Warn,
            "tas",
            &format_args!(
                "HeapReAlloc({p:p}, {size}, drapeaux {flags:#x}) = {q:p} (fil {})",
                libc::syscall(libc::SYS_gettid)
            ),
        );
    }
    if q.is_null() {
        crate::process::set_last_error(8);
        return q;
    }
    allocations.remove(&(p as usize));
    allocations.insert(q as usize, size.max(1));
    if flags & 8 != 0 && size > old {
        std::ptr::write_bytes((q as *mut u8).add(old), 0, size - old);
    } else if heap_fill() && size > old {
        heap_fill_bytes((q as *mut u8).add(old), size - old);
    }
    q
}
pub unsafe extern "win64" fn heap_free(_heap: usize, _flags: u32, p: *mut c_void) -> i32 {
    if p.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(old) =
        lock(HEAP_ALLOCS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&(p as usize))
    else {
        crate::process::set_last_error(87);
        return 0;
    };
    let (trace, nofree) = heap_diag();
    if trace {
        crate::log::emit(
            crate::log::Level::Warn,
            "tas",
            &format_args!("HeapFree({p:p}) (fil {})", libc::syscall(libc::SYS_gettid)),
        );
    }
    if super::tas_page::enabled() && super::tas_page::free(p) {
        // Mode page (#2158) : bloc rendu inaccessible, en quarantaine.
    } else if heap_lfh() {
        lfh_free(p, old);
    } else if !nofree {
        libc::free(p);
    }
    1
}
pub(crate) fn create_global_memory(
    size: usize,
    flags: u32,
) -> Option<(usize, Arc<Mutex<GlobalMemory>>)> {
    let mut bytes = Vec::new();
    // Au moins un octet de capacité : le pointeur d'un bloc GMEM_FIXED sert
    // de handle et doit être unique même pour une taille nulle.
    bytes.try_reserve_exact(size.max(1)).ok()?;
    bytes.resize(size, 0);
    // GMEM_FIXED (bit GMEM_MOVEABLE = 0x2 absent) : le handle est le
    // pointeur lui-même, utilisable directement et rendu tel quel par
    // GlobalLock. GMEM_MOVEABLE garde un numéro de handle opaque.
    let handle = if flags & 0x2 == 0 {
        bytes.as_ptr() as usize
    } else {
        next_handle()
    };
    let memory = Arc::new(Mutex::new(GlobalMemory {
        bytes,
        locks: 0,
        flags,
    }));
    lock(GLOBAL_MEMORY.get_or_init(|| Mutex::new(HashMap::new())))
        .insert(handle, Arc::clone(&memory));
    Some((handle, memory))
}
pub(crate) fn lookup_global_memory(handle: usize) -> Option<Arc<Mutex<GlobalMemory>>> {
    lock(GLOBAL_MEMORY.get_or_init(|| Mutex::new(HashMap::new())))
        .get(&handle)
        .cloned()
}
/// Handle d'un bloc GMEM_MOVEABLE à partir du pointeur que rend GlobalLock.
pub(crate) fn global_handle_of_pointer(ptr: usize) -> Option<usize> {
    lock(GLOBAL_MEMORY.get_or_init(|| Mutex::new(HashMap::new())))
        .iter()
        .find(|(_, m)| lock(m).bytes.as_ptr() as usize == ptr)
        .map(|(h, _)| *h)
}
pub(crate) fn free_global_memory(handle: usize) -> bool {
    lock(GLOBAL_MEMORY.get_or_init(|| Mutex::new(HashMap::new())))
        .remove(&handle)
        .is_some()
}
pub extern "win64" fn global_alloc(flags: u32, size: usize) -> usize {
    match create_global_memory(size, flags) {
        Some((handle, _)) => handle,
        None => {
            crate::process::set_last_error(8);
            0
        }
    }
}
pub extern "win64" fn global_realloc(handle: usize, size: usize, flags: u32) -> usize {
    let Some(memory) = lookup_global_memory(handle) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let mut memory_guard = lock(&memory);
    let old = memory_guard.bytes.len();
    if size > old && memory_guard.bytes.try_reserve_exact(size - old).is_err() {
        crate::process::set_last_error(8);
        return 0;
    }
    memory_guard.bytes.resize(size, 0);
    // GMEM_MODIFY (0x80) change seulement les attributs.
    if flags & 0x80 == 0 {
        memory_guard.flags = (memory_guard.flags & 0x2) | (flags & !0x2);
    }
    let fixed = memory_guard.flags & 0x2 == 0;
    let new_ptr = memory_guard.bytes.as_ptr() as usize;
    drop(memory_guard);
    if fixed && new_ptr != handle {
        // Bloc fixe déplacé : le nouveau pointeur devient le handle.
        let mut table = lock(GLOBAL_MEMORY.get_or_init(|| Mutex::new(HashMap::new())));
        table.remove(&handle);
        table.insert(new_ptr, memory);
        return new_ptr;
    }
    handle
}
pub extern "win64" fn global_lock(handle: usize) -> *mut c_void {
    let Some(memory) = lookup_global_memory(handle) else {
        crate::process::set_last_error(6);
        return std::ptr::null_mut();
    };
    let mut memory = lock(&memory);
    memory.locks = memory.locks.saturating_add(1);
    memory.bytes.as_mut_ptr().cast()
}
pub extern "win64" fn global_unlock(handle: usize) -> i32 {
    let Some(memory) = lookup_global_memory(handle) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let mut memory = lock(&memory);
    if memory.locks == 0 {
        crate::process::set_last_error(158);
        return 0;
    }
    memory.locks -= 1;
    crate::process::set_last_error(0);
    i32::from(memory.locks != 0)
}
pub extern "win64" fn global_size(handle: usize) -> usize {
    let Some(memory) = lookup_global_memory(handle) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let size = lock(&memory).bytes.len();
    size
}
pub extern "win64" fn global_flags(handle: usize) -> u32 {
    let Some(memory) = lookup_global_memory(handle) else {
        crate::process::set_last_error(6);
        return u32::MAX;
    };
    let memory = lock(&memory);
    (memory.flags & 0x7f00) | memory.locks.min(0xff)
}
pub extern "win64" fn global_free(handle: usize) -> usize {
    if handle == 0 {
        return 0;
    }
    if free_global_memory(handle) {
        0
    } else {
        crate::process::set_last_error(6);
        handle
    }
}
pub unsafe extern "win64" fn virtual_alloc(
    addr: *mut c_void,
    size: usize,
    _kind: u32,
    prot: u32,
) -> *mut c_void {
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_RESET: u32 = 0x80000;
    const MEM_WRITE_WATCH: u32 = 0x20_0000;
    const PAGE: usize = 4096;
    const GRANULARITY: usize = 65536;
    // MEM_WRITE_WATCH : seulement avec MEM_RESERVE, comme Windows (#965).
    let watch = _kind & MEM_WRITE_WATCH != 0;
    if watch && _kind & MEM_RESERVE == 0 {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    }
    let _kind = _kind & !MEM_WRITE_WATCH;
    if size == 0
        || !valid_win_prot(prot)
        || _kind & !(MEM_COMMIT | MEM_RESERVE | MEM_RESET) != 0
        || _kind & (MEM_COMMIT | MEM_RESERVE | MEM_RESET) == 0
        || (_kind & MEM_RESET != 0 && _kind != MEM_RESET)
    {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    }
    let Some(request_end) = (addr as usize).checked_add(size) else {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    };
    if _kind == MEM_RESET {
        let Some(end0) = request_end.checked_add(PAGE - 1) else {
            crate::process::set_last_error(87);
            return std::ptr::null_mut();
        };
        let base = addr as usize & !(PAGE - 1);
        let end = end0 & !(PAGE - 1);
        let mut maps = allocs();
        let Some((&region_base, _r)) = maps
            .iter_mut()
            .find(|(b, r)| r.kind == 0x20000 && base >= **b && end <= **b + r.size)
        else {
            crate::process::set_last_error(487);
            return std::ptr::null_mut();
        };
        if libc::madvise(base as *mut _, end - base, libc::MADV_DONTNEED) != 0 {
            crate::process::set_last_error(8);
            return std::ptr::null_mut();
        }
        let _ = region_base;
        return base as *mut c_void;
    }
    if _kind & MEM_RESERVE == 0 {
        if addr.is_null() {
            return virtual_alloc(std::ptr::null_mut(), size, MEM_RESERVE | MEM_COMMIT, prot);
        }
        let Some(page_end0) = request_end.checked_add(PAGE - 1) else {
            crate::process::set_last_error(87);
            return std::ptr::null_mut();
        };
        let page_base = addr as usize & !(PAGE - 1);
        let page_end = page_end0 & !(PAGE - 1);
        let mut maps = allocs();
        let Some((&region_base, r)) = maps
            .iter_mut()
            .find(|(b, r)| r.kind == 0x20000 && page_base >= **b && page_end <= **b + r.size)
        else {
            drop(maps);
            // Pages d'une vue MapViewOfFile(Ex) d'une section SEC_RESERVE :
            // Windows les valide par VirtualAlloc(MEM_COMMIT). Le memfd
            // fournit déjà les pages ; on pose la protection (#665).
            return match crate::dlls::mmap::protect_view_pages(
                page_base,
                page_end,
                prot,
                win_prot(prot),
            ) {
                Some(Ok(_)) => page_base as *mut c_void,
                Some(Err(code)) => {
                    crate::process::set_last_error(code);
                    std::ptr::null_mut()
                }
                None => {
                    crate::process::set_last_error(487);
                    std::ptr::null_mut()
                }
            };
        };
        let start = (page_base - region_base) / PAGE;
        let count = (page_end - page_base) / PAGE;
        if libc::mprotect(page_base as *mut _, page_end - page_base, win_prot(prot)) != 0 {
            crate::process::set_last_error(8);
            return std::ptr::null_mut();
        }
        r.pages.fill(start, count, prot);
        return page_base as *mut c_void;
    }
    let page_base = if addr.is_null() {
        0
    } else {
        (addr as usize) & !(GRANULARITY - 1)
    };
    let offset = if addr.is_null() {
        0
    } else {
        (addr as usize) - page_base
    };
    let Some(size) = size
        .checked_add(offset)
        .and_then(|n| n.checked_add(PAGE - 1))
        .map(|n| n & !(PAGE - 1))
    else {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    };
    let initial_prot = if _kind & MEM_COMMIT != 0 {
        win_prot(prot)
    } else {
        libc::PROT_NONE
    };
    let (p, mapped_size) = if addr.is_null() {
        let Some(reserve) = size.checked_add(GRANULARITY) else {
            crate::process::set_last_error(8);
            return std::ptr::null_mut();
        };
        let raw = libc::mmap(
            std::ptr::null_mut(),
            reserve,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        if raw == libc::MAP_FAILED {
            crate::process::set_last_error(8);
            return std::ptr::null_mut();
        }
        let aligned = ((raw as usize) + GRANULARITY - 1) & !(GRANULARITY - 1);
        let prefix = aligned - raw as usize;
        let suffix = reserve - prefix - size;
        if prefix > 0 {
            libc::munmap(raw, prefix);
        }
        if suffix > 0 {
            libc::munmap((aligned + size) as *mut c_void, suffix);
        }
        (aligned, size)
    } else {
        let p = libc::mmap(
            page_base as *mut c_void,
            size,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED_NOREPLACE,
            -1,
            0,
        );
        if p == libc::MAP_FAILED {
            crate::process::set_last_error(487);
            return std::ptr::null_mut();
        }
        (p as usize, size)
    };
    if _kind & MEM_COMMIT != 0 && libc::mprotect(p as *mut c_void, mapped_size, initial_prot) != 0 {
        libc::munmap(p as *mut c_void, mapped_size);
        crate::process::set_last_error(8);
        return std::ptr::null_mut();
    }
    let pages = mapped_size / PAGE;
    allocs().insert(
        p,
        VmRegion {
            size: mapped_size,
            pages: PageMap::new(pages, if _kind & MEM_COMMIT != 0 { prot } else { 0 }),
            allocation_protection: prot,
            kind: 0x20000,
        },
    );
    if watch {
        crate::dlls::write_watch::register(p, mapped_size);
    }
    p as *mut c_void
}

/// Pages engagées (indices) de la région réservée à `base`, dans
/// [first, end) : pour GetWriteWatch.
pub(crate) fn committed_pages(base: usize, first: usize, end: usize) -> Vec<usize> {
    let maps = allocs();
    let Some(r) = maps.get(&base) else {
        return Vec::new();
    };
    let end = end.min(r.size / 4096);
    let mut out = Vec::new();
    let mut page = first;
    while page < end {
        let (_, run_end) = r.pages.run_bounds(page);
        let stop = run_end.min(end);
        if r.pages.committed(page) {
            out.extend(page..stop);
        }
        page = stop;
    }
    out
}
pub unsafe extern "win64" fn virtual_free(addr: *mut c_void, size: usize, kind: u32) -> i32 {
    const MEM_DECOMMIT: u32 = 0x4000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE: usize = 4096;
    if addr.is_null() {
        crate::process::set_last_error(487);
        return 0;
    }
    let mut maps = allocs();
    let p = addr as usize;
    if kind == MEM_RELEASE {
        if size != 0 {
            crate::process::set_last_error(87);
            return 0;
        }
        let Some(region) = maps.get(&p) else {
            crate::process::set_last_error(487);
            return 0;
        };
        if region.kind != 0x20000 {
            crate::process::set_last_error(487);
            return 0;
        }
        if libc::munmap(p as *mut c_void, region.size) != 0 {
            crate::process::set_last_error(8);
            return 0;
        }
        maps.remove(&p);
        drop(maps);
        crate::dlls::write_watch::unregister(p);
        return 1;
    }
    if kind == MEM_DECOMMIT {
        if size == 0 {
            crate::process::set_last_error(87);
            return 0;
        }
        let start = p & !(PAGE - 1);
        let Some(end_raw) = p.checked_add(size) else {
            crate::process::set_last_error(87);
            return 0;
        };
        let Some(end0) = end_raw.checked_add(PAGE - 1) else {
            crate::process::set_last_error(87);
            return 0;
        };
        let end = end0 & !(PAGE - 1);
        let Some((&base, region)) = maps
            .iter_mut()
            .find(|(b, r)| r.kind == 0x20000 && start >= **b && end <= **b + r.size)
        else {
            crate::process::set_last_error(487);
            return 0;
        };
        if libc::mprotect(start as *mut c_void, end - start, libc::PROT_NONE) != 0
            || libc::madvise(start as *mut c_void, end - start, libc::MADV_DONTNEED) != 0
        {
            crate::process::set_last_error(8);
            return 0;
        }
        let first = (start - base) / PAGE;
        let count = (end - start) / PAGE;
        region.pages.fill(first, count, 0);
        return 1;
    }
    crate::process::set_last_error(87);
    0
}
/// PAGE_GUARD, PAGE_NOCACHE et PAGE_WRITECOMBINE modifient une protection
/// de base ; PAGE_GUARD ne s'applique pas à PAGE_NOACCESS.
const PAGE_GUARD: u32 = 0x100;
const PAGE_MODIFIERS: u32 = 0x100 | 0x200 | 0x400;
fn valid_win_prot(p: u32) -> bool {
    p & !(0xFF | PAGE_MODIFIERS) == 0
        && matches!(
            p & 0xFF,
            0x01 | 0x02 | 0x04 | 0x08 | 0x10 | 0x20 | 0x40 | 0x80
        )
        && !(p & 0xFF == 0x01 && p & PAGE_MODIFIERS != 0)
}
fn win_prot(p: u32) -> i32 {
    // Page de garde : inaccessible jusqu'au premier accès, qui lève
    // STATUS_GUARD_PAGE_VIOLATION et rend la protection de base (seh_hw).
    if p & PAGE_GUARD != 0 {
        return libc::PROT_NONE;
    }
    match p & 0xFF {
        0x01 => libc::PROT_NONE,
        0x02 => libc::PROT_READ,
        0x04 | 0x08 => libc::PROT_READ | libc::PROT_WRITE,
        0x10 => libc::PROT_EXEC,
        0x20 => libc::PROT_EXEC | libc::PROT_READ,
        0x40 | 0x80 => libc::PROT_EXEC | libc::PROT_READ | libc::PROT_WRITE,
        _ => libc::PROT_NONE,
    }
}
pub unsafe extern "win64" fn virtual_protect(
    addr: *mut c_void,
    size: usize,
    prot: u32,
    old: *mut u32,
) -> i32 {
    const PAGE: usize = 4096;
    if addr.is_null() || size == 0 || old.is_null() || !valid_win_prot(prot) {
        crate::process::set_last_error(87);
        return 0;
    }
    let start = addr as usize & !(PAGE - 1);
    let Some(end_raw) = (addr as usize).checked_add(size) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let Some(end0) = end_raw.checked_add(PAGE - 1) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let end = end0 & !(PAGE - 1);
    let mut maps = allocs();
    let Some((&base, r)) = maps
        .iter_mut()
        .find(|(b, r)| start >= **b && end <= **b + r.size)
    else {
        drop(maps);
        // Vue d'un mappage de fichier ou de section (code double-mappé de
        // coreclr, #665).
        return match crate::dlls::mmap::protect_view_pages(start, end, prot, win_prot(prot)) {
            Some(Ok(previous)) => {
                *old = previous;
                1
            }
            Some(Err(code)) => {
                crate::process::set_last_error(code);
                0
            }
            None => {
                crate::process::set_last_error(487);
                0
            }
        };
    };
    let first = (start - base) / PAGE;
    let count = (end - start) / PAGE;
    if !r.pages.all_committed(first, count) {
        crate::process::set_last_error(487);
        return 0;
    }
    let previous = r.pages.get(first);
    if libc::mprotect(start as *mut c_void, end - start, win_prot(prot)) != 0 {
        crate::process::set_last_error(8);
        return 0;
    }
    *old = previous;
    r.pages.fill(first, count, prot);
    1
}
/// Pour le gestionnaire de fautes (contexte de signal : jamais de verrou
/// bloquant) : `Some(true)` si `addr` est dans une page VirtualAlloc
/// exécutable (code d'un JIT), `Some(false)` si elle n'y est pas, `None` si
/// la table est occupée.
pub(crate) fn exec_region_try(addr: usize) -> Option<bool> {
    const PAGE: usize = 4096;
    let Some(table) = ALLOCS.get() else {
        return Some(false);
    };
    let maps = table.try_lock().ok()?;
    Some(maps.iter().any(|(&b, r)| {
        addr >= b
            && addr < b + r.size
            && matches!(
                r.pages.get((addr - b) / PAGE) & 0xFF,
                0x10 | 0x20 | 0x40 | 0x80
            )
    }))
}

/// Page de garde touchée en `addr` : retire PAGE_GUARD de cette page (une
/// seule levée, comme Windows) et renvoie vrai. Faux si `addr` n'est pas
/// dans une page de garde VirtualAlloc. Hors contexte de signal.
pub(crate) fn clear_guard_page(addr: usize) -> bool {
    const PAGE: usize = 4096;
    if ALLOCS.get().is_none() {
        return false;
    }
    let mut maps = allocs();
    let Some((&base, r)) = maps
        .iter_mut()
        .find(|(&b, r)| addr >= b && addr < b + r.size)
    else {
        return false;
    };
    let index = (addr - base) / PAGE;
    if !r.pages.committed(index) || r.pages.get(index) & PAGE_GUARD == 0 {
        return false;
    }
    let prot = r.pages.get(index) & !PAGE_GUARD;
    let page = base + index * PAGE;
    if unsafe { libc::mprotect(page as *mut c_void, PAGE, win_prot(prot)) } != 0 {
        return false;
    }
    r.pages.fill(index, 1, prot);
    true
}

/// Variante sans verrou bloquant de `clear_guard_page`, pour savoir dans le
/// gestionnaire de signal si la faute vise une page de garde.
pub(crate) fn is_guard_page_try(addr: usize) -> Option<bool> {
    const PAGE: usize = 4096;
    let Some(table) = ALLOCS.get() else {
        return Some(false);
    };
    let maps = table.try_lock().ok()?;
    Some(maps.iter().any(|(&b, r)| {
        addr >= b && addr < b + r.size && r.pages.get((addr - b) / PAGE) & PAGE_GUARD != 0
    }))
}

/// Région Linux (hors VirtualAlloc) qui contient `address` : la pile du
/// thread courant, décrite comme une pile Windows (AllocationBase au bas de
/// la réservation, ce que lit coreclr pour borner sa pile), ou une
/// projection de `/proc/self/maps` (image, tas, piles des autres threads).
/// Pile du thread vue par `VirtualQuery`, découpée comme sous Windows : en
/// bas une page réservée, puis une page de garde (PAGE_GUARD), puis la partie
/// validée jusqu'au sommet. coreclr cherche cette page de garde quand une
/// exception n'est pas gérée : sans elle, il conclut à un débordement de
/// pile et s'arrête sur « Stack overflow. » au lieu d'afficher l'exception.
fn stack_region(address: usize, limit: usize, base: usize) -> MemoryBasicInformation {
    const PAGE: usize = 4096;
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_PRIVATE: u32 = 0x20000;
    const PAGE_READWRITE: u32 = 4;
    const PAGE_GUARD: u32 = 0x100;
    let page = address & !(PAGE - 1);
    let guard = limit + PAGE;
    let committed = (limit + 2 * PAGE).min(base);
    let (lo, hi, state, protect) = if page < guard {
        (limit, guard, MEM_RESERVE, 0)
    } else if page < committed {
        (guard, committed, MEM_COMMIT, PAGE_READWRITE | PAGE_GUARD)
    } else {
        (committed, base, MEM_COMMIT, PAGE_READWRITE)
    };
    MemoryBasicInformation {
        base_address: page.max(lo) as *mut c_void,
        allocation_base: limit as *mut c_void,
        allocation_protect: PAGE_READWRITE,
        partition_id: 0,
        region_size: hi - page.max(lo),
        state,
        protect,
        kind: MEM_PRIVATE,
    }
}

fn linux_region(address: usize) -> Option<MemoryBasicInformation> {
    let teb = crate::process::teb();
    if teb != 0 {
        let (base, limit) = unsafe {
            (
                *((teb + crate::process::TEB_STACK_BASE) as *const usize),
                *((teb + crate::process::TEB_STACK_LIMIT) as *const usize),
            )
        };
        if limit < base && address >= limit && address < base {
            return Some(stack_region(address, limit, base));
        }
    }
    let text = std::fs::read_to_string("/proc/self/maps").ok()?;
    maps_region(&text, address)
}

/// Bornes (début, fin) de chaque ligne de `/proc/<pid>/maps`.
fn maps_ranges(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    text.lines().filter_map(|line| {
        let (lo, hi) = line.split_whitespace().next()?.split_once('-')?;
        Some((
            usize::from_str_radix(lo, 16).ok()?,
            usize::from_str_radix(hi, 16).ok()?,
        ))
    })
}

/// Région d'un texte `/proc/<pid>/maps` qui contient `address`.
fn maps_region(text: &str, address: usize) -> Option<MemoryBasicInformation> {
    const PAGE: usize = 4096;
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_PRIVATE: u32 = 0x20000;
    const MEM_MAPPED: u32 = 0x40000;
    let page = address & !(PAGE - 1);
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (range, perms) = (parts.next()?, parts.next()?);
        let (lo, hi) = range.split_once('-')?;
        let (lo, hi) = (
            usize::from_str_radix(lo, 16).ok()?,
            usize::from_str_radix(hi, 16).ok()?,
        );
        if address < lo || address >= hi {
            continue;
        }
        let p = perms.as_bytes();
        let (r, w, x) = (p[0] == b'r', p[1] == b'w', p[2] == b'x');
        let protect = match (r, w, x) {
            (false, false, false) => 1,
            (true, false, false) => 2,
            (true, true, false) | (false, true, false) => 4,
            (_, false, true) => 0x20,
            (_, true, true) => 0x40,
        };
        let inode = parts.nth(2).unwrap_or("0");
        return Some(MemoryBasicInformation {
            base_address: page as *mut c_void,
            allocation_base: lo as *mut c_void,
            allocation_protect: protect,
            partition_id: 0,
            region_size: hi - page,
            state: MEM_COMMIT,
            protect,
            kind: if inode == "0" {
                MEM_PRIVATE
            } else {
                MEM_MAPPED
            },
        });
    }
    None
}
pub unsafe extern "win64" fn virtual_query(
    addr: *const c_void,
    info: *mut MemoryBasicInformation,
    length: usize,
) -> usize {
    const PAGE: usize = 4096;
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_FREE: u32 = 0x10000;
    let required = std::mem::size_of::<MemoryBasicInformation>();
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    if length < required {
        crate::process::set_last_error(122);
        return 0;
    }
    let address = addr as usize;
    let page = address & !(PAGE - 1);
    let maps = allocs();
    let region = maps
        .iter()
        .find(|(base, r)| address >= **base && address < **base + r.size);
    let result = if let Some((&base, r)) = region {
        let idx = (page - base) / PAGE;
        let committed = r.pages.committed(idx);
        let protect = if committed { r.pages.get(idx) } else { 1 };
        let (first, last) = r.pages.run_bounds(idx);
        MemoryBasicInformation {
            base_address: (base + first * PAGE) as *mut c_void,
            allocation_base: base as *mut c_void,
            allocation_protect: r.allocation_protection,
            partition_id: 0,
            region_size: (last - first) * PAGE,
            state: if committed { MEM_COMMIT } else { MEM_RESERVE },
            protect,
            kind: r.kind,
        }
    } else if let Some(r) = linux_region(address) {
        r
    } else {
        let next = maps
            .keys()
            .copied()
            .filter(|b| *b > page)
            .min()
            .unwrap_or(0x0000_7fff_ffff_ffffusize);
        let len = next.saturating_sub(page).max(PAGE);
        MemoryBasicInformation {
            base_address: page as *mut c_void,
            allocation_base: std::ptr::null_mut(),
            allocation_protect: 0,
            partition_id: 0,
            region_size: len,
            state: MEM_FREE,
            protect: 1,
            kind: 0,
        }
    };
    if crate::log::enabled(crate::log::Level::Trace) {
        if let Some(n) = crate::dlls::mmap::named_view_at(address) {
            crate::trace!(
                "VirtualQuery(0x{address:x}, vue de {n}) : base 0x{:x}, taille {}, état 0x{:x}",
                result.base_address as usize,
                result.region_size,
                result.state
            );
        }
    }
    std::ptr::write_unaligned(info, result);
    required
}
/// Plus haute adresse d'un programme Windows 64 bits (0x7FFFFFFEFFFF).
const USER_SPACE_END: usize = 0x7FFF_FFFF_0000;

/// VirtualQueryEx : pour un autre processus (enfant, ou processus qui nous a
/// désignés comme le gestionnaire de plantage de Chromium, #1729), les
/// régions viennent de son `/proc/<pid>/maps` ; un trou entre deux régions
/// est MEM_FREE, et au-delà de l'espace utilisateur ERROR_INVALID_PARAMETER
/// arrête le parcours, comme sous Windows.
pub unsafe extern "win64" fn virtual_query_ex(
    process: usize,
    addr: *const c_void,
    info: *mut MemoryBasicInformation,
    length: usize,
) -> usize {
    if is_current_process_handle(process) {
        return virtual_query(addr, info, length);
    }
    let Some(pid) = process_os_pid(process) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let required = std::mem::size_of::<MemoryBasicInformation>();
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    if length < required {
        crate::process::set_last_error(24); // ERROR_BAD_LENGTH
        return 0;
    }
    let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/maps")) else {
        crate::process::set_last_error(5);
        return 0;
    };
    match foreign_region(&text, addr as usize) {
        Some(r) => {
            std::ptr::write_unaligned(info, r);
            required
        }
        None => {
            crate::process::set_last_error(87);
            0
        }
    }
}

/// Région d'un autre processus d'après son `maps` ; None au-delà de
/// l'espace utilisateur.
fn foreign_region(text: &str, address: usize) -> Option<MemoryBasicInformation> {
    const PAGE: usize = 4096;
    if address >= USER_SPACE_END {
        return None;
    }
    if let Some(r) = maps_region(text, address) {
        let end = (r.base_address as usize + r.region_size).min(USER_SPACE_END);
        return Some(MemoryBasicInformation {
            region_size: end - r.base_address as usize,
            ..r
        });
    }
    let page = address & !(PAGE - 1);
    let next = maps_ranges(text)
        .map(|(lo, _)| lo)
        .filter(|&lo| lo > page)
        .min()
        .unwrap_or(USER_SPACE_END)
        .min(USER_SPACE_END);
    Some(MemoryBasicInformation {
        base_address: page as *mut c_void,
        allocation_base: std::ptr::null_mut(),
        allocation_protect: 0,
        partition_id: 0,
        region_size: next - page,
        state: 0x10000, // MEM_FREE
        protect: 1,
        kind: 0,
    })
}
pub extern "win64" fn flush_instruction_cache(
    _process: usize,
    _addr: *const c_void,
    _size: usize,
) -> i32 {
    1
}
/// Millisecondes écoulées depuis le démarrage du système, veille comprise,
/// comme GetTickCount sous Windows (#534). CLOCK_BOOTTIME est monotone ;
/// l'ancienne version comptait depuis le premier appel et rendait 0.
pub fn boot_millis() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) } == 0 {
        return ts.tv_sec as u64 * 1000 + ts.tv_nsec as u64 / 1_000_000;
    }
    CLOCK_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}
pub extern "win64" fn get_tick_count() -> u32 {
    boot_millis() as u32
}
pub extern "win64" fn get_tick_count64() -> u64 {
    boot_millis()
}
pub unsafe extern "win64" fn get_current_process_id() -> u32 {
    libc::getpid() as u32
}
fn same_sync(a: &SyncHandle, b: &SyncHandle) -> bool {
    match (a, b) {
        (SyncHandle::Event(x), SyncHandle::Event(y)) => Arc::ptr_eq(x, y),
        (SyncHandle::Mutex(x), SyncHandle::Mutex(y)) => Arc::ptr_eq(x, y),
        (SyncHandle::Thread(x), SyncHandle::Thread(y)) => Arc::ptr_eq(x, y),
        (SyncHandle::Process(x), SyncHandle::Process(y)) => Arc::ptr_eq(x, y),
        (SyncHandle::CompletionPort(x), SyncHandle::CompletionPort(y)) => Arc::ptr_eq(x, y),
        (SyncHandle::Semaphore(x), SyncHandle::Semaphore(y)) => Arc::ptr_eq(x, y),
        _ => false,
    }
}
fn sync_kind(obj: &SyncHandle) -> u32 {
    match obj {
        SyncHandle::Event(_) => SHARED_KIND_EVENT,
        SyncHandle::Mutex(_) => SHARED_KIND_MUTEX,
        SyncHandle::Semaphore(_) => SHARED_KIND_SEMAPHORE,
        _ => 0,
    }
}

/// Objet nommé `key` partagé avec les autres processus du compte (#20).
/// `obj` : l'objet à publier si le nom est libre (Create*), None pour
/// seulement ouvrir (Open*). Rend l'objet à utiliser et vrai s'il existait ;
/// Err(6) si le nom désigne un objet d'un autre type ; None : pas d'objet
/// de ce nom ailleurs (ou partage indisponible).
fn share_named(
    key: &str,
    want: u32,
    obj: Option<&SyncHandle>,
) -> Option<Result<(SyncHandle, bool), u32>> {
    use crate::dlls::inherit::SharedWord;
    use crate::dlls::named_objects as named;
    let mut made: Option<SharedWord> = None;
    let opened = {
        let mut init = |f: &std::fs::File, _: &std::path::Path| -> bool {
            let Some(obj) = obj else {
                return false;
            };
            // Page réservée avant la projection : sur un tmpfs plein, la
            // première écriture plantait au lieu d'échouer (#1905).
            if !crate::dlls::inherit::reserve_page(std::os::fd::AsRawFd::as_raw_fd(f)) {
                return false;
            }
            let Some(w) = named::dup(f).and_then(SharedWord::map) else {
                return false;
            };
            let (state, aux, manual) = match obj {
                SyncHandle::Event(e) => (
                    u32::from(e.is_set(&lock(&e.signaled))),
                    0,
                    u32::from(e.manual_reset),
                ),
                SyncHandle::Mutex(m) => {
                    let st = lock(&m.state);
                    match st.owner {
                        Some(_) => (os_thread_id(), st.recursion as u32, 0),
                        None => (0, 0, 0),
                    }
                }
                SyncHandle::Semaphore(sem) => {
                    let c = lock(&sem.count);
                    (c.0 as u32, c.1 as u32, 0)
                }
                _ => return false,
            };
            w.slot(0).store(state, Ordering::Release);
            w.slot(1).store(aux, Ordering::Release);
            w.slot(3).store(manual, Ordering::Release);
            w.slot(2).store(want, Ordering::Release);
            made = Some(w);
            true
        };
        let create: Option<&mut dyn FnMut(&std::fs::File, &std::path::Path) -> bool> =
            if obj.is_some() { Some(&mut init) } else { None };
        named::open(named::SYNC, key, create)?
    };
    if opened.created {
        let (obj, w) = (obj?.clone(), made?);
        let attached = match &obj {
            SyncHandle::Event(e) => e.shared.set(w).is_ok(),
            SyncHandle::Mutex(m) => m.shared.set(w).is_ok(),
            SyncHandle::Semaphore(sem) => sem.shared.set(w).is_ok(),
            _ => false,
        };
        return attached.then_some(Ok((obj, false)));
    }
    let Some(w) = SharedWord::map(opened.fd) else {
        named::release(named::SYNC, key);
        return None;
    };
    if w.slot(2).load(Ordering::Acquire) != want {
        named::release(named::SYNC, key);
        return Some(Err(6));
    }
    let found = match want {
        SHARED_KIND_EVENT => {
            let manual = w.slot(3).load(Ordering::Acquire) != 0;
            EventObject::adopt(manual, w)
        }
        SHARED_KIND_MUTEX => MutexObject::adopt(w),
        _ => SemaphoreObject::adopt(w),
    };
    Some(Ok((found, true)))
}

pub(crate) fn add_sync(obj: SyncHandle, name: Option<String>) -> usize {
    let Some(name) = name else {
        return add_sync_named(obj, None);
    };
    let kind = kind_label(sync_kind(&obj));
    let h = add_sync_named(obj, Some(name.clone()));
    let err = crate::process::last_error();
    remember_name(h, &name);
    crate::trace!(
        "Create{kind}({name}) = 0x{h:x}, clé {:?}, {} (erreur {err})",
        crate::dlls::named_objects::name_key(&name),
        match (h, err) {
            (0, _) => "refusé",
            (_, 183) => "existait déjà",
            _ => "créé",
        }
    );
    h
}

fn add_sync_named(obj: SyncHandle, name: Option<String>) -> usize {
    let obj = if let Some(name) = name {
        let key = crate::dlls::named_objects::name_key(&name);
        let mut names = lock(SYNC_NAMES.get_or_init(|| Mutex::new(HashMap::new())));
        if let Some((old, count)) = names.get_mut(&key) {
            let compatible = matches!(
                (&obj, &*old),
                (SyncHandle::Event(_), SyncHandle::Event(_))
                    | (SyncHandle::Mutex(_), SyncHandle::Mutex(_))
                    | (SyncHandle::Semaphore(_), SyncHandle::Semaphore(_))
            );
            if !compatible {
                crate::process::set_last_error(6);
                return 0;
            }
            *count += 1;
            crate::process::set_last_error(183);
            old.clone()
        } else {
            // Même nom dans un autre processus du compte (#20) ?
            let obj = match share_named(&key, sync_kind(&obj), Some(&obj)) {
                Some(Ok((shared, existed))) => {
                    if existed {
                        crate::process::set_last_error(183);
                    }
                    shared
                }
                Some(Err(e)) => {
                    crate::process::set_last_error(e);
                    return 0;
                }
                None => {
                    crate::trace!(
                        "objet nommé {name} : partage entre processus indisponible, objet local"
                    );
                    obj
                }
            };
            crate::dlls::named_objects::retain(crate::dlls::named_objects::SYNC, &key);
            names.insert(key, (obj.clone(), 1));
            obj
        }
    } else {
        obj
    };
    let h = next_handle();
    lock(SYNCS.get_or_init(|| Mutex::new(HashMap::new()))).insert(h, obj);
    h
}
/// Type d'objet du noyau d'un handle, tel que le nomme
/// NtQueryObject(ObjectTypeInformation), ou None s'il n'existe pas.
pub(crate) fn object_type_name(h: usize) -> Option<&'static str> {
    if h == crate::dlls::security::TOKEN_HANDLE || matches!(h as isize, -6..=-4) {
        return Some("Token");
    }
    if h == get_current_process() {
        return Some("Process");
    }
    if h == get_current_thread() {
        return Some("Thread");
    }
    if fd_of_handle(h).is_some() {
        return Some("File");
    }
    if crate::dlls::mmap::export(h).is_some() {
        return Some("Section");
    }
    let Some(sync) = lookup_sync(h) else {
        // Clé du registre ouverte (#2413).
        return crate::registry::key_nt_name(h).map(|_| "Key");
    };
    Some(match sync {
        SyncHandle::Event(_) => "Event",
        SyncHandle::Mutex(_) => "Mutant",
        SyncHandle::Thread(_) => "Thread",
        SyncHandle::Process(_) => "Process",
        SyncHandle::CompletionPort(_) => "IoCompletion",
        SyncHandle::Semaphore(_) => "Semaphore",
    })
}
/// Le handle est-il un événement de CreateEvent ?
pub(crate) fn is_event_handle(h: usize) -> bool {
    matches!(lookup_sync(h), Some(SyncHandle::Event(_)))
}
pub(crate) fn lookup_sync(h: usize) -> Option<SyncHandle> {
    lock(SYNCS.get_or_init(|| Mutex::new(HashMap::new())))
        .get(&h)
        .cloned()
}
fn remove_sync(h: usize) -> bool {
    forget_name(h);
    let Some(obj) = lock(SYNCS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&h) else {
        return false;
    };
    let mut names = lock(SYNC_NAMES.get_or_init(|| Mutex::new(HashMap::new())));
    let mut dead = None;
    for (name, (named, count)) in names.iter_mut() {
        if same_sync(&obj, named) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                dead = Some(name.clone());
            }
            break;
        }
    }
    if let Some(name) = dead {
        names.remove(&name);
        crate::dlls::named_objects::release(crate::dlls::named_objects::SYNC, &name);
    }
    true
}
/// WINLUX_TRACE (#2072) : nom des handles d'objets nommés, pour tracer
/// leurs signalements et leurs attentes entre processus (connexion de
/// steam.exe au webhelper, #1995). Rempli seulement quand la trace est
/// active.
static TRACED_NAMES: Mutex<Option<HashMap<usize, String>>> = Mutex::new(None);

fn trace_on() -> bool {
    crate::log::enabled(crate::log::Level::Trace)
}

fn remember_name(h: usize, name: &str) {
    if h != 0 && trace_on() {
        lock(&TRACED_NAMES)
            .get_or_insert_with(HashMap::new)
            .insert(h, name.to_string());
    }
}

fn forget_name(h: usize) {
    if trace_on() {
        if let Some(m) = lock(&TRACED_NAMES).as_mut() {
            m.remove(&h);
        }
    }
}

/// Nom de l'objet du handle `h` s'il est nommé et la trace active.
fn traced_name(h: usize) -> Option<String> {
    if !trace_on() {
        return None;
    }
    lock(&TRACED_NAMES).as_ref()?.get(&h).cloned()
}

fn kind_label(kind: u32) -> &'static str {
    match kind {
        SHARED_KIND_EVENT => "Event",
        SHARED_KIND_MUTEX => "Mutex",
        SHARED_KIND_SEMAPHORE => "Semaphore",
        _ => "objet",
    }
}

fn named_event(name: *const u8) -> Option<String> {
    if name.is_null() {
        None
    } else {
        let s = unsafe { crate::dlls::cstr(name) };
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}
fn wait_condvar<'a, T>(
    cv: &Condvar,
    guard: std::sync::MutexGuard<'a, T>,
    timeout: u32,
) -> (std::sync::MutexGuard<'a, T>, bool) {
    if timeout == u32::MAX {
        (
            cv.wait(guard)
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            false,
        )
    } else {
        let (g, r) = cv
            .wait_timeout(guard, std::time::Duration::from_millis(timeout as u64))
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (g, r.timed_out())
    }
}
fn wait_event(e: &EventObject, timeout: u32) -> u32 {
    let start = std::time::Instant::now();
    loop {
        let gate = sync_gate();
        let mut state = lock(&e.signaled);
        if e.take(&mut state) {
            return 0;
        }
        if timeout == 0 {
            return 258;
        }
        let left = if timeout == u32::MAX {
            u32::MAX
        } else {
            let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
            if elapsed >= timeout {
                return 258;
            }
            timeout - elapsed
        };
        drop(gate);
        if let Some(w) = e.shared.get() {
            // Signalé par un autre processus : attente sur la page partagée.
            drop(state);
            w.wait(left.min(50));
            continue;
        }
        let (next, _) = wait_condvar(&e.changed, state, left);
        drop(next);
    }
}
fn wait_semaphore(sem: &SemaphoreObject, timeout: u32) -> u32 {
    let start = std::time::Instant::now();
    if let Some(w) = sem.shared.get() {
        loop {
            if SemaphoreObject::shared_try(w) {
                return 0;
            }
            let Some(left) = time_left(start, timeout) else {
                return 258;
            };
            w.wait_while(0, left.min(50));
        }
    }
    loop {
        let gate = sync_gate();
        let mut state = lock(&sem.count);
        if sem.shared.get().is_some() {
            drop(state);
            drop(gate);
            return wait_semaphore(sem, time_left(start, timeout).unwrap_or(0));
        }
        if state.0 > 0 {
            state.0 -= 1;
            return 0;
        }
        if timeout == 0 {
            return 258;
        }
        let left = if timeout == u32::MAX {
            u32::MAX
        } else {
            let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
            if elapsed >= timeout {
                return 258;
            }
            timeout - elapsed
        };
        drop(gate);
        let (next, _) = wait_condvar(&sem.changed, state, left);
        drop(next);
    }
}
/// Temps restant d'une attente commencée à `start` (None : écoulé).
fn time_left(start: std::time::Instant, timeout: u32) -> Option<u32> {
    if timeout == u32::MAX {
        return Some(u32::MAX);
    }
    let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
    (elapsed < timeout).then(|| timeout - elapsed)
}
fn wait_mutex(m: &MutexObject, timeout: u32) -> u32 {
    let me = std::thread::current().id();
    let start = std::time::Instant::now();
    if let Some(w) = m.shared.get() {
        loop {
            if let Some(abandoned) = MutexObject::shared_try(w) {
                return if abandoned { 0x80 } else { 0 };
            }
            let Some(left) = time_left(start, timeout) else {
                return 258;
            };
            let owner = w.slot(0).load(Ordering::Acquire);
            if owner != 0 {
                w.wait_while(owner, left.min(50));
            }
        }
    }
    loop {
        let gate = sync_gate();
        let mut s = lock(&m.state);
        if m.shared.get().is_some() {
            // Passé dans une page partagée pendant l'attente (#767).
            drop(s);
            drop(gate);
            return wait_mutex(m, time_left(start, timeout).unwrap_or(0));
        }
        if s.owner.as_ref() == Some(&me) {
            s.recursion = s.recursion.saturating_add(1);
            return 0;
        }
        if s.owner.is_none() {
            s.owner = Some(me);
            s.owner_os = os_thread_id();
            s.recursion = 1;
            return 0;
        }
        if timeout == 0 {
            return 258;
        }
        let left = if timeout == u32::MAX {
            u32::MAX
        } else {
            let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
            if elapsed >= timeout {
                return 258;
            }
            timeout - elapsed
        };
        drop(gate);
        let (next, _) = wait_condvar(&m.changed, s, left);
        drop(next);
    }
}
fn wait_thread(t: &ThreadObject, timeout: u32) -> u32 {
    let start = std::time::Instant::now();
    loop {
        let gate = sync_gate();
        let s = lock(&t.state);
        if s.finished {
            return 0;
        }
        if timeout == 0 {
            return 258;
        }
        let left = if timeout == u32::MAX {
            u32::MAX
        } else {
            let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
            if elapsed >= timeout {
                return 258;
            }
            timeout - elapsed
        };
        drop(gate);
        let (next, _) = wait_condvar(&t.changed, s, left);
        drop(next);
    }
}
fn poll_process(p: &ProcessObject) -> bool {
    let mut s = lock(&p.state);
    if let Some(pid) = s.foreign {
        // Un processus terminé que son parent n'a pas encore réclamé reste
        // un zombie : il est fini, son handle doit être signalé (#1859).
        if !os_process_alive(pid) || os_process_zombie(pid) {
            s.foreign = None;
            s.exit_code = 0;
            p.changed.notify_all();
        }
        return s.foreign.is_none();
    }
    if let Some(child) = s.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                s.exit_code = status.code().unwrap_or(1) as u32;
                s.child = None;
                p.changed.notify_all();
            }
            Ok(None) => {}
            Err(_) => {
                s.exit_code = 1;
                s.child = None;
                p.changed.notify_all();
            }
        }
    }
    s.child.is_none()
}
fn wait_process(p: &ProcessObject, timeout: u32) -> u32 {
    let start = std::time::Instant::now();
    loop {
        let gate = sync_gate();
        if poll_process(p) {
            return 0;
        }
        if timeout == 0 {
            return 258;
        }
        if timeout != u32::MAX && start.elapsed().as_millis() >= timeout as u128 {
            return 258;
        }
        drop(gate);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn try_mutex(m: &MutexObject) -> bool {
    let _gate = sync_gate();
    let me = std::thread::current().id();
    let mut s = lock(&m.state);
    if s.owner.as_ref() == Some(&me) {
        s.recursion = s.recursion.saturating_add(1);
        true
    } else if s.owner.is_none() {
        s.owner = Some(me);
        s.owner_os = os_thread_id();
        s.recursion = 1;
        true
    } else {
        false
    }
}
fn leave_mutex(m: &MutexObject) -> bool {
    if let Some(w) = m.shared.get() {
        if w.slot(0).load(Ordering::Acquire) != os_thread_id() {
            crate::process::set_last_error(288);
            return false;
        }
        let depth = w.slot(1).load(Ordering::Acquire);
        if depth > 1 {
            w.slot(1).store(depth - 1, Ordering::Release);
        } else {
            w.slot(1).store(0, Ordering::Release);
            w.slot(0).store(0, Ordering::Release);
            w.wake();
        }
        return true;
    }
    let gate = sync_gate();
    let me = std::thread::current().id();
    let mut s = lock(&m.state);
    if m.shared.get().is_some() {
        drop(s);
        drop(gate);
        return leave_mutex(m);
    }
    if s.owner.as_ref() != Some(&me) {
        crate::process::set_last_error(288);
        return false;
    }
    if s.recursion > 1 {
        s.recursion -= 1;
    } else {
        s.recursion = 0;
        s.owner = None;
        m.changed.notify_one();
    }
    true
}
fn critical_object(cs: usize) -> Option<Arc<MutexObject>> {
    lock(CRITICALS.get_or_init(|| Mutex::new(HashMap::new())))
        .get(&cs)
        .cloned()
}
unsafe fn update_critical_fields(cs: *mut c_void, m: &MutexObject) {
    let s = lock(&m.state);
    let lock_count = if s.owner.is_some() { 0 } else { -1 };
    let recursion = s.recursion.min(i32::MAX as usize) as i32;
    let owner = s
        .owner
        .as_ref()
        .map_or(0, |_| get_current_thread_id() as usize);
    if crate::wow32::active() {
        // RTL_CRITICAL_SECTION x86 : 24 octets, champs de 4 octets.
        let p = cs as *mut u32;
        p.add(1).write_unaligned(lock_count as u32);
        p.add(2).write_unaligned(recursion as u32);
        p.add(3).write_unaligned(owner as u32);
        return;
    }
    let p = cs as *mut CriticalSection;
    (*p).lock_count = lock_count;
    (*p).recursion_count = recursion;
    (*p).owning_thread = owner;
}
/// Remet à zéro une CRITICAL_SECTION (40 octets en 64 bits, 24 en x86 :
/// écrire la forme 64 bits déborderait sur les données voisines).
unsafe fn reset_critical_fields(cs: *mut c_void, lock_count: i32, spin: usize) {
    if crate::wow32::active() {
        let p = cs as *mut u32;
        for (i, v) in [0, lock_count as u32, 0, 0, 0, spin as u32]
            .into_iter()
            .enumerate()
        {
            p.add(i).write_unaligned(v);
        }
        return;
    }
    std::ptr::write(
        cs as *mut CriticalSection,
        CriticalSection {
            debug_info: std::ptr::null_mut(),
            lock_count,
            recursion_count: 0,
            owning_thread: 0,
            lock_semaphore: 0,
            spin_count: spin,
        },
    );
}
pub unsafe extern "win64" fn initialize_critical_section(cs: *mut c_void) {
    if cs.is_null() {
        crate::process::set_last_error(87);
        return;
    }
    reset_critical_fields(cs, -1, 0);
    lock(CRITICALS.get_or_init(|| Mutex::new(HashMap::new())))
        .insert(cs as usize, Arc::new(MutexObject::new(false)));
}
pub unsafe extern "win64" fn initialize_critical_section_and_spin_count(
    cs: *mut c_void,
    spin: u32,
) -> i32 {
    initialize_critical_section(cs);
    if cs.is_null() {
        return 0;
    }
    reset_critical_fields(cs, -1, spin as usize);
    1
}
pub unsafe extern "win64" fn initialize_critical_section_ex(
    cs: *mut c_void,
    spin: u32,
    flags: u32,
) -> i32 {
    // Drapeaux RTL_CRITICAL_SECTION_FLAG_* (octet haut : NO_DEBUG_INFO,
    // DYNAMIC_SPIN, STATIC_INIT, RESOURCE_TYPE, FORCE_DEBUG_INFO…) : aucun
    // ne change le comportement ici ; le reste est réservé.
    if flags & !0xFF00_0000 != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    initialize_critical_section_and_spin_count(cs, spin & 0x00FF_FFFF)
}
pub unsafe extern "win64" fn enter_critical_section(cs: *mut c_void) {
    let Some(m) = critical_object(cs as usize) else {
        crate::process::set_last_error(87);
        return;
    };
    let _ = wait_mutex(&m, u32::MAX);
    update_critical_fields(cs, &m);
}
pub unsafe extern "win64" fn try_enter_critical_section(cs: *mut c_void) -> i32 {
    let Some(m) = critical_object(cs as usize) else {
        crate::process::set_last_error(87);
        return 0;
    };
    if try_mutex(&m) {
        update_critical_fields(cs, &m);
        1
    } else {
        0
    }
}
pub unsafe extern "win64" fn leave_critical_section(cs: *mut c_void) {
    let Some(m) = critical_object(cs as usize) else {
        crate::process::set_last_error(87);
        return;
    };
    if leave_mutex(&m) {
        update_critical_fields(cs, &m);
    }
}
pub unsafe extern "win64" fn delete_critical_section(cs: *mut c_void) {
    if cs.is_null() {
        return;
    }
    lock(CRITICALS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&(cs as usize));
    reset_critical_fields(cs, 0, 0);
}
pub unsafe extern "win64" fn create_thread(
    _attrs: usize,
    stack: usize,
    start: usize,
    param: usize,
    flags: u32,
    id: *mut u32,
) -> usize {
    static NEXT_THREAD: AtomicU32 = AtomicU32::new(1);
    // CREATE_SUSPENDED (4) et STACK_SIZE_PARAM_IS_A_RESERVATION (0x10000) :
    // la pile d'un thread Linux est de toute façon réservée, pas engagée.
    if start == 0 || flags & !0x10004 != 0 {
        crate::process::set_last_error(if start == 0 { 87 } else { 50 });
        return 0;
    }
    let seq = NEXT_THREAD.fetch_add(1, Ordering::Relaxed);
    let state = Arc::new(ThreadObject::new(flags & 4 != 0));
    let handle = add_sync(SyncHandle::Thread(state.clone()), None);
    let mut builder = std::thread::Builder::new().name(format!("win-thread-{seq}"));
    if stack != 0 {
        builder = builder.stack_size(stack.max(64 * 1024));
    }
    let thread = state.clone();
    if let Err(e) = builder.spawn(move || {
        let os_tid = libc::syscall(libc::SYS_gettid) as i32;
        let _ = thread.rust_id.set(std::thread::current().id());
        super::thread_ctl::register(os_tid, &thread.slot);
        register_thread_object(os_tid, &thread);
        let mut s = lock(&thread.state);
        // L'identifiant rendu par CreateThread est celui que
        // GetCurrentThreadId donnera dans le thread : le créateur attend
        // qu'il soit connu, même pour un thread créé suspendu.
        s.win_tid = os_tid as u32;
        thread.changed.notify_all();
        while s.suspend_count != 0 {
            s = thread
                .changed
                .wait(s)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        s.running = true;
        drop(s);
        {
            // Même verrou que SetThreadAffinityMask/SetThreadPriority :
            // une demande faite pendant le démarrage n'est jamais perdue.
            let mut pending = lock(&thread.pending_cpus);
            if let Some(cpus) = pending.take() {
                crate::cpu::set_affinity(os_tid, &cpus);
            }
            super::kernel32_cpu::thread_started(os_tid, thread.priority.load(Ordering::Relaxed));
            thread.os_tid.store(os_tid, Ordering::Release);
        }
        CURRENT_THREAD.with(|c| *c.borrow_mut() = Some(thread.clone()));
        crate::process::thread_attach_current();
        let jump: *mut u64 = lock(&thread.exit_jump).as_mut_ptr();
        // Second retour (valeur 1) : ExitThread a déjà enregistré le code
        // et signalé le handle ; le thread se termine simplement ici.
        if winlux_thread_setjmp(jump) == 0 {
            let code = super::thread_ctl::root_call(start, param);
            finish_current_thread(code);
        }
        std::hint::black_box(&thread);
    }) {
        // Toujours journalisé, avec les limites en cause (#767).
        crate::log_warn!(
            "process",
            "CreateThread refusé ({e}) : {}",
            crate::crash::thread_limits_report()
        );
        remove_sync(handle);
        crate::process::set_last_error(8);
        return 0;
    }
    let tid = {
        let mut s = lock(&state.state);
        while s.win_tid == 0 {
            s = state
                .changed
                .wait(s)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        s.win_tid
    };
    if !id.is_null() {
        *id = tid;
    }
    handle
}

thread_local! {
    /// Objet de thread Win32 du thread courant (None pour le thread
    /// principal et les threads Rust internes).
    static CURRENT_THREAD: std::cell::RefCell<Option<Arc<ThreadObject>>> =
        const { std::cell::RefCell::new(None) };
    /// Objet de thread créé pour un thread qui ne vient pas de CreateThread
    /// (thread principal, threads de Winlux qui exécutent du code invité) :
    /// marqué terminé à la fin du thread.
    static ADOPTED: AdoptedThread = const { AdoptedThread(std::cell::RefCell::new(None)) };
}

struct AdoptedThread(std::cell::RefCell<Option<Arc<ThreadObject>>>);
impl Drop for AdoptedThread {
    fn drop(&mut self) {
        if let Some(thread) = self.0.borrow_mut().take() {
            let tid = thread.os_tid.load(Ordering::Acquire);
            super::thread_ctl::unregister(tid, &thread.slot);
            unregister_thread_object(tid);
            let mut s = lock(&thread.state);
            s.exit_code = 0;
            s.finished = true;
            thread.changed.notify_all();
        }
    }
}

/// Threads Win32 vivants par tid Linux (OpenThread).
static THREADS_BY_TID: OnceLock<Mutex<HashMap<i32, std::sync::Weak<ThreadObject>>>> =
    OnceLock::new();
fn register_thread_object(tid: i32, t: &Arc<ThreadObject>) {
    lock(THREADS_BY_TID.get_or_init(|| Mutex::new(HashMap::new()))).insert(tid, Arc::downgrade(t));
}
fn unregister_thread_object(tid: i32) {
    lock(THREADS_BY_TID.get_or_init(|| Mutex::new(HashMap::new()))).remove(&tid);
}
/// Threads Windows vivants du processus (WINLUX_THREAD_DUMP, #2147).
pub(crate) fn thread_slots() -> Vec<(i32, Arc<ThreadObject>)> {
    let mut v: Vec<(i32, Arc<ThreadObject>)> =
        lock(THREADS_BY_TID.get_or_init(|| Mutex::new(HashMap::new())))
            .iter()
            .filter_map(|(&t, w)| Some((t, w.upgrade()?)))
            .collect();
    v.sort_by_key(|(t, _)| *t);
    v
}
fn thread_object_by_tid(tid: i32) -> Option<Arc<ThreadObject>> {
    lock(THREADS_BY_TID.get_or_init(|| Mutex::new(HashMap::new())))
        .get(&tid)
        .and_then(std::sync::Weak::upgrade)
}

/// Objet de thread Win32 du thread courant, créé au besoin (thread
/// principal, threads internes) : DuplicateHandle(GetCurrentThread()) en
/// donne un vrai handle, que coreclr passe ensuite à SuspendThread depuis
/// le thread du ramasse-miettes.
pub(crate) fn current_thread_object() -> Arc<ThreadObject> {
    if let Some(t) = CURRENT_THREAD
        .try_with(|c| c.borrow().clone())
        .ok()
        .flatten()
    {
        return t;
    }
    if let Some(t) = ADOPTED.try_with(|a| a.0.borrow().clone()).ok().flatten() {
        return t;
    }
    let tid = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
    let t = Arc::new(ThreadObject::new(false));
    {
        let mut s = lock(&t.state);
        s.win_tid = tid as u32;
        s.running = true;
    }
    t.os_tid.store(tid, Ordering::Release);
    let _ = t.rust_id.set(std::thread::current().id());
    if ADOPTED
        .try_with(|a| *a.0.borrow_mut() = Some(t.clone()))
        .is_ok()
    {
        super::thread_ctl::register(tid, &t.slot);
        register_thread_object(tid, &t);
    }
    t
}

/// Thread visé par un handle : pseudo-handle, handle de thread, ou rien.
fn thread_object_of(h: usize) -> Option<Arc<ThreadObject>> {
    if h == get_current_thread() {
        return Some(current_thread_object());
    }
    match lookup_sync(h)? {
        SyncHandle::Thread(t) => Some(t),
        _ => None,
    }
}

fn is_current(t: &ThreadObject) -> bool {
    t.os_tid.load(Ordering::Acquire) == unsafe { libc::syscall(libc::SYS_gettid) as i32 }
}

/// tid d'un thread démarré (None s'il attend encore son démarrage).
fn running_tid(t: &ThreadObject) -> Option<i32> {
    let s = lock(&t.state);
    s.running.then_some(s.win_tid as i32)
}

/// `GetThreadIOPendingFlag` : les E/S de Winlux sont synchrones vues du
/// thread (les E/S « overlapped » se terminent dans l'appel ou sur un autre
/// thread), donc aucun thread n'a d'E/S en attente. Le pool de threads de
/// .NET 9 la consulte depuis chaque thread de travail (#1072).
pub unsafe extern "win64" fn get_thread_io_pending_flag(_thread: usize, pending: *mut i32) -> i32 {
    if pending.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *pending = 0;
    1
}

/// `SuspendThread`.
pub extern "win64" fn suspend_thread(h: usize) -> u32 {
    let Some(t) = thread_object_of(h) else {
        crate::process::set_last_error(6);
        return u32::MAX;
    };
    {
        let mut s = lock(&t.state);
        if s.finished {
            crate::process::set_last_error(5);
            return u32::MAX;
        }
        if !s.running {
            // Pas encore démarré : il attend que le compte retombe à 0.
            let prev = s.suspend_count as u32;
            if prev >= super::thread_ctl::MAXIMUM_SUSPEND_COUNT {
                crate::process::set_last_error(super::thread_ctl::ERROR_SIGNAL_REFUSED);
                return u32::MAX;
            }
            s.suspend_count += 1;
            return prev;
        }
    }
    let Some(tid) = running_tid(&t) else {
        return 0;
    };
    match super::thread_ctl::suspend(&t.slot, tid) {
        Ok(prev) => prev,
        Err(e) => {
            crate::process::set_last_error(e);
            u32::MAX
        }
    }
}

/// `GetThreadContext`.
pub unsafe extern "win64" fn get_thread_context(h: usize, ctx: *mut u8) -> i32 {
    if ctx.is_null() {
        crate::process::set_last_error(998);
        return 0;
    }
    let Some(t) = thread_object_of(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if is_current(&t) {
        super::thread_ctl::get_own_context(ctx);
        return 1;
    }
    let Some(tid) = running_tid(&t) else {
        // Créé suspendu, jamais démarré : pas encore de registres.
        crate::process::set_last_error(31);
        return 0;
    };
    if super::thread_ctl::get_context(&t.slot, tid, ctx) {
        1
    } else {
        crate::process::set_last_error(5);
        0
    }
}

/// `SetThreadContext`.
pub unsafe extern "win64" fn set_thread_context(h: usize, ctx: *const u8) -> i32 {
    if ctx.is_null() {
        crate::process::set_last_error(998);
        return 0;
    }
    let Some(t) = thread_object_of(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if is_current(&t) {
        let flags = (ctx.add(0x30) as *const u32).read_unaligned();
        if flags & 0x1 != 0 {
            let mut copy = seh_context_copy(ctx);
            super::seh::rtl_restore_context(copy.as_mut_ptr(), std::ptr::null_mut());
        }
        return 1;
    }
    let Some(tid) = running_tid(&t) else {
        crate::process::set_last_error(31);
        return 0;
    };
    if super::thread_ctl::set_context(&t.slot, tid, ctx) {
        1
    } else {
        crate::process::set_last_error(5);
        0
    }
}

unsafe fn seh_context_copy(ctx: *const u8) -> Vec<u8> {
    std::slice::from_raw_parts(ctx, super::seh::CONTEXT_SIZE).to_vec()
}

/// `OpenThread` : handle vers un thread de ce processus par son identifiant.
pub extern "win64" fn open_thread(_access: u32, inherit: i32, tid: u32) -> usize {
    let me = unsafe { libc::syscall(libc::SYS_gettid) as u32 };
    let t = if tid == me {
        Some(current_thread_object())
    } else {
        thread_object_by_tid(tid as i32)
    };
    let Some(t) = t else {
        crate::process::set_last_error(87);
        return 0;
    };
    let h = add_sync(SyncHandle::Thread(t), None);
    crate::dlls::inherit::set(h, inherit != 0);
    h
}

/// Termine le thread Win32 courant : rappels DLL_THREAD_DETACH, libération
/// du TEB, puis signalement du handle avec son code de sortie. Appelé à la
/// fin naturelle de la procédure du thread et par ExitThread.
/// `ExitThread` : enregistre le code, signale le handle puis revient au pont
/// de thread par le contexte sauvé à son démarrage. Sur un thread qui n'a
/// pas été créé par CreateThread (thread principal, threads internes), on
/// retombe sur pthread_exit.
pub(crate) fn exit_current_thread(code: u32) -> ! {
    let jump = CURRENT_THREAD
        .try_with(|c| c.borrow().as_ref().map(|t| lock(&t.exit_jump).as_mut_ptr()))
        .ok()
        .flatten();
    finish_current_thread(code);
    match jump {
        Some(jump) => unsafe { winlux_thread_longjmp(jump, 1) },
        None => unsafe { libc::pthread_exit(code as usize as *mut c_void) },
    }
}

pub(crate) fn finish_current_thread(code: u32) {
    // WINLUX_THREAD_TRACE / WINLUX_APITRACE (#2405) : fin du fil.
    super::fil_trace::note_end(code);
    crate::process::thread_detach_current();
    release_iocp_slots_for_current_thread();
    let thread = CURRENT_THREAD
        .try_with(|c| c.borrow_mut().take())
        .ok()
        .flatten();
    if let Some(thread) = thread {
        let tid = thread.os_tid.load(Ordering::Acquire);
        super::thread_ctl::unregister(tid, &thread.slot);
        unregister_thread_object(tid);
        let _gate = sync_gate();
        let mut s = lock(&thread.state);
        s.exit_code = code;
        s.finished = true;
        thread.changed.notify_all();
    }
}
pub(crate) fn split_windows_command_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut arg = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            let mut slashes = 1usize;
            while chars.peek() == Some(&'\\') {
                chars.next();
                slashes += 1;
            }
            if chars.peek() == Some(&'"') {
                chars.next();
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                if slashes % 2 == 1 {
                    arg.push('"')
                } else {
                    quoted = !quoted;
                }
            } else {
                arg.extend(std::iter::repeat_n('\\', slashes));
            }
            continue;
        }
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if c.is_whitespace() && !quoted {
            if !arg.is_empty() {
                out.push(std::mem::take(&mut arg));
            }
            continue;
        }
        arg.push(c);
    }
    if !arg.is_empty() {
        out.push(arg);
    }
    out
}
fn process_error(e: &std::io::Error) -> u32 {
    io_error_code(e)
}
/// Variables de l'hôte Linux qu'un programme Windows ne transmet pas de
/// lui-même dans un bloc d'environnement : affichage, session, bus, et la
/// configuration de Winlux. Elles passent sous les valeurs du bloc.
fn linux_plumbing_env() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    const KEEP: &[&str] = &[
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "XDG_SESSION_TYPE",
        "DBUS_SESSION_BUS_ADDRESS",
        "PULSE_SERVER",
    ];
    std::env::vars_os()
        .filter(|(k, _)| {
            let k = k.to_string_lossy();
            KEEP.contains(&k.as_ref())
                || (k.starts_with("WINLUX_")
                    && !matches!(k.as_ref(), "WINLUX_COMMAND_LINE" | "WINLUX_SERVICE_NAME"))
        })
        .collect()
}

unsafe fn parse_environment_block(
    block: *const c_void,
    unicode: bool,
) -> Result<Option<Vec<(String, String)>>, u32> {
    if block.is_null() {
        return Ok(None);
    }
    let mut env = HashMap::<String, (String, String)>::new();
    if unicode {
        let p = block as *const u16;
        let mut off = 0usize;
        loop {
            if off >= 524_288 {
                return Err(87);
            }
            let mut end = off;
            while end < 524_288 && *p.add(end) != 0 {
                end += 1;
            }
            if end == 524_288 {
                return Err(87);
            }
            if end == off {
                break;
            }
            let entry = String::from_utf16(std::slice::from_raw_parts(p.add(off), end - off))
                .map_err(|_| 1113u32)?;
            off = end + 1;
            insert_env_entry(&entry, &mut env);
        }
    } else {
        let p = block as *const u8;
        let mut off = 0usize;
        loop {
            if off >= 1_048_576 {
                return Err(87);
            }
            let mut end = off;
            while end < 1_048_576 && *p.add(end) != 0 {
                end += 1;
            }
            if end == 1_048_576 {
                return Err(87);
            }
            if end == off {
                break;
            }
            let entry = crate::dlls::decode_ansi(std::slice::from_raw_parts(p.add(off), end - off));
            off = end + 1;
            insert_env_entry(&entry, &mut env);
        }
    }
    Ok(Some(env.into_values().collect()))
}
fn insert_env_entry(entry: &str, env: &mut HashMap<String, (String, String)>) {
    let start = if entry.starts_with('=') { 1 } else { 0 };
    if let Some(i) = entry[start..].find('=') {
        let split = start + i;
        let name = &entry[..split];
        let value = &entry[split + 1..];
        if !name.is_empty() {
            env.insert(name.to_lowercase(), (name.to_string(), value.to_string()));
        }
    }
}
/// Descripteur Linux d'un handle de fichier ou d'un handle standard.
pub(crate) fn fd_of_handle(h: usize) -> Option<i32> {
    use std::os::unix::io::AsRawFd;
    let h = canon_std(h);
    match h as isize {
        -10 => return Some(0),
        -11 => return Some(1),
        -12 => return Some(2),
        _ => {}
    }
    let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
    match handles.get(&h)? {
        FileHandle::File(f) => Some(lock(f).as_raw_fd()),
    }
}
/// Le descripteur est-il un tube (CreatePipe, `|` du shell) ?
/// Tube anonyme (FIFO) ou tube nommé (socket Unix) : une lecture de 0 octet
/// y signifie que l'autre bout est fermé.
/// Attend qu'un tube ait des données à lire. Faux : tube fermé à l'autre
/// bout sans rien à lire (ERROR_BROKEN_PIPE).
fn wait_pipe_readable(fd: i32) -> bool {
    loop {
        let mut p = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let r = unsafe { libc::poll(&mut p, 1, -1) };
        if r < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return false;
        }
        if p.revents & libc::POLLIN != 0 {
            return true;
        }
        if p.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            return false;
        }
    }
}
fn fd_is_pipe(fd: i32) -> bool {
    file_type_of_fd(fd) == 3
}
/// Entrée/sortie standard d'un enfant d'après un handle de STARTUPINFO :
/// 0 ou INVALID_HANDLE_VALUE → rien (/dev/null), sinon une copie du fd.
fn stdio_for_handle(h: usize, _which: i32) -> Option<std::process::Stdio> {
    if h == 0 || h == usize::MAX {
        return Some(std::process::Stdio::null());
    }
    let fd = fd_of_handle(h)?;
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if copy < 0 {
        return None;
    }
    let owned = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(copy) };
    Some(std::process::Stdio::from(owned))
}
/// Handle de processus Windows pour un enfant lancé ailleurs (ShellExecuteEx).
pub(crate) fn add_child_process(child: Child) -> usize {
    let object = Arc::new(ProcessObject {
        state: Mutex::new(ProcessState {
            child: Some(child),
            foreign: None,
            exit_code: 259,
            suspended: false,
        }),
        changed: Condvar::new(),
    });
    add_sync(SyncHandle::Process(object), None)
}
#[repr(C)]
pub struct SecurityAttributes {
    pub length: u32,
    pub descriptor: *mut c_void,
    pub inherit: i32,
}
/// `CreatePipe` : tube anonyme Linux, lu et écrit par ReadFile/WriteFile.
pub unsafe extern "win64" fn create_pipe(
    read: *mut usize,
    write: *mut usize,
    sa: *const SecurityAttributes,
    _size: u32,
) -> i32 {
    if read.is_null() || write.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut fds = [0i32; 2];
    if libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) != 0 {
        crate::process::set_last_error(8);
        return 0;
    }
    let r: File = std::os::unix::io::FromRawFd::from_raw_fd(fds[0]);
    let w: File = std::os::unix::io::FromRawFd::from_raw_fd(fds[1]);
    *read = insert_file(r);
    *write = insert_file(w);
    crate::dlls::inherit::from_security_attributes(*read, sa as usize);
    crate::dlls::inherit::from_security_attributes(*write, sa as usize);
    1
}

/// Nom de programme Windows (`.exe`, `.com`) : sans fichier, CreateProcess échoue.
fn is_windows_program_name(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe") || e.eq_ignore_ascii_case("com"))
}

/// ERROR_PATH_NOT_FOUND si le dossier du programme manque, sinon
/// ERROR_FILE_NOT_FOUND.
fn missing_program_error(path: &std::path::Path) -> u32 {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() && !dir.is_dir() => 3,
        _ => 2,
    }
}

unsafe fn create_process_common(
    application: Option<String>,
    command_line: *const u8,
    inherit_handles: i32,
    creation_flags: u32,
    environment: Option<Vec<(String, String)>>,
    current_directory: *const u8,
    startup: *const StartupInfoA,
    info: *mut ProcessInformation,
) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    // Le débogage (DEBUG_PROCESS, DEBUG_ONLY_THIS_PROCESS) n'est pas pris en
    // charge ; les classes de priorité, consoles, groupes et
    // STARTUPINFOEX sont acceptés (sans effet ou traduits plus bas).
    if creation_flags & (0x1 | 0x2) != 0 {
        crate::process::set_last_error(50);
        return 0;
    }
    if !startup.is_null() && (*startup).cb < std::mem::size_of::<StartupInfoA>() as u32 {
        crate::process::set_last_error(87);
        return 0;
    }
    let line = if command_line.is_null() {
        String::new()
    } else {
        crate::dlls::cstr(command_line)
    };
    crate::trace!(
        "CreateProcess : application {application:?}, ligne {line:?}, options {creation_flags:#x}"
    );
    let mut args = split_windows_command_line(&line);
    let app_dir =
        crate::loader::main_module().and_then(|m| m.path.parent().map(|d| d.to_path_buf()));
    let exe = if let Some(app) = application {
        if !args.is_empty() {
            args.remove(0);
        }
        app
    } else if !args.is_empty() {
        // `C:\Program Files\App\app.exe arg` sans guillemets : comme Windows,
        // on essaie les préfixes de plus en plus longs.
        let mut taken = 1;
        if !line.trim_start().starts_with('"') {
            for n in 2..=args.len() {
                let candidate = args[..n].join(" ");
                if crate::launch::looks_like_path(&candidate)
                    && crate::launch::find_program(&candidate, app_dir.as_deref()).is_some()
                {
                    taken = n;
                }
            }
        }
        args.drain(..taken).collect::<Vec<_>>().join(" ")
    } else {
        crate::process::set_last_error(87);
        return 0;
    };
    let mut host_path = crate::launch::find_program(&exe, app_dir.as_deref())
        .unwrap_or_else(|| crate::paths::to_linux(&exe));
    if !host_path.exists() && crate::dlls::shell32::is_explorer(&exe) {
        // `explorer.exe [/select,]CHEMIN` : l'Explorateur Winlux de la session,
        // comme par ShellExecute (une seule version de l'Explorateur).
        if let Some(Ok((opener, argv))) = crate::dlls::shell32::explorer(&args.join(" ")) {
            host_path = opener;
            args = argv;
        }
    }
    // msiexec.exe, reg.exe et regedit.exe intégrés : pas de fichier sur disque.
    let mut builtin_tool = false;
    if !host_path.exists() && crate::msi::is_msiexec(&host_path) {
        // msiexec.exe est intégré au runtime : `winlux msiexec.exe …`.
        host_path = std::path::PathBuf::from("msiexec.exe");
        builtin_tool = true;
    }
    if !host_path.exists()
        && (crate::registry::cli::is_reg(&host_path)
            || crate::registry::cli::is_regedit(&host_path))
    {
        // reg.exe et regedit.exe sont intégrés au runtime eux aussi.
        let name = host_path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let name = if name.ends_with(".exe") {
            name
        } else {
            format!("{name}.exe")
        };
        host_path = std::path::PathBuf::from(name);
        builtin_tool = true;
    }
    // Programme Windows introuvable (Powershell.exe lancé par ScreenToGif) :
    // échec rendu à l'appelant, comme sous Windows, au lieu de lancer un
    // runtime qui écrit « lecture impossible » sur stderr (#1133).
    if !builtin_tool && !host_path.exists() && is_windows_program_name(&host_path) {
        crate::trace!("CreateProcess : {exe:?} introuvable");
        crate::process::set_last_error(missing_program_error(&host_path));
        return 0;
    }
    // Un « .exe » qui est en réalité un programme Linux (ELF ou script
    // « #! », par exemple un lien bash.exe → /usr/bin/bash) s'exécute
    // directement : c'est ainsi que les outils Linux s'offrent aux
    // programmes Windows qui cherchent un .exe (Git Bash de Claude Code).
    let is_pe = !crate::launch::is_linux_program(&host_path)
        && (host_path
            .extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("exe"))
            || (host_path.is_file() && crate::launch::is_windows_program(&host_path)));
    let suspended = creation_flags & 4 != 0;
    let mut target = Vec::<std::ffi::OsString>::new();
    if is_pe {
        let runtime = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                crate::process::set_last_error(process_error(&e));
                return 0;
            }
        };
        target.push(runtime.into_os_string());
        target.push(host_path.into_os_string());
        target.extend(args.iter().map(std::ffi::OsString::from));
    } else if crate::launch::is_desktop_entry(&host_path) {
        // Lanceur d'appli Linux : on exécute sa ligne `Exec=`.
        let files = crate::launch::linux_args(&args);
        let Some((argv, _terminal)) = crate::launch::desktop_command(&host_path, &files) else {
            crate::process::set_last_error(193);
            return 0;
        };
        target.extend(argv.into_iter().map(std::ffi::OsString::from));
    } else {
        // Appli Linux : les chemins Windows des arguments deviennent Linux.
        // Un outil Linux déguisé en .exe (bash.exe pour Git Bash) reçoit en
        // plus ses scripts avec les chemins Windows et MSYS traduits.
        let disguised = crate::launch::is_disguised_linux_tool(&host_path);
        target.push(host_path.into_os_string());
        target.extend(
            crate::launch::linux_args(&args)
                .into_iter()
                .map(|a| {
                    if disguised {
                        crate::launch::msys_script(&a)
                    } else {
                        a
                    }
                })
                .map(std::ffi::OsString::from),
        );
    }
    // Même .exe que nous (enfants de Chromium) : même adresse d'image (#767).
    let same_base = if is_pe {
        target
            .get(1)
            .and_then(|p| crate::loader::base_for_child(std::path::Path::new(p)))
    } else {
        None
    };
    // Programme Windows suspendu : il se fige lui-même une fois chargé
    // (image, DLL et PEB en place, aucun code du programme exécuté),
    // comme sous Windows. Le bac à sable de Chromium lit puis écrit alors
    // sa mémoire (ReadProcessMemory, WriteProcessMemory) avant ResumeThread.
    let mut cmd = if suspended && !is_pe {
        let mut c = Command::new("/bin/sh");
        c.arg("-c")
            .arg("kill -STOP $$; exec \"$@\"")
            .arg("winlux-suspended")
            .args(target.iter());
        c
    } else {
        let mut i = target.into_iter();
        let exe = i.next().unwrap();
        let mut c = Command::new(exe);
        c.args(i);
        c
    };
    if !current_directory.is_null() {
        let cwd = crate::paths::to_linux(&crate::dlls::cstr(current_directory));
        cmd.current_dir(cwd);
    }
    // EXTENDED_STARTUPINFO_PRESENT + PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE :
    // le programme s'attache à la pseudo-console (pty) de CreatePseudoConsole.
    let mut pseudo_console = false;
    if creation_flags & 0x0008_0000 != 0
        && !startup.is_null()
        && (*startup).cb as usize >= std::mem::size_of::<StartupInfoA>() + 8
    {
        let list = std::ptr::read_unaligned(
            (startup as *const u8).add(std::mem::size_of::<StartupInfoA>()) as *const usize,
        );
        if let Some(console) = crate::dlls::kernel32_node::proc_thread_attribute(
            list,
            crate::dlls::kernel32_node::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
        ) {
            if !crate::dlls::console_input::attach_pseudo_console(&mut cmd, console) {
                crate::process::set_last_error(6);
                return 0;
            }
            pseudo_console = true;
        }
    }
    // STARTF_USESTDHANDLES : entrée, sortie et erreur du programme lancé
    // (tubes de CreatePipe, fichiers, ou handles standard du parent).
    if !pseudo_console && !startup.is_null() && (*startup).flags & 0x100 != 0 {
        let s = &*startup;
        match (
            stdio_for_handle(s.std_input, 0),
            stdio_for_handle(s.std_output, 1),
            stdio_for_handle(s.std_error, 2),
        ) {
            (Some(i), Some(o), Some(e)) => {
                cmd.stdin(i).stdout(o).stderr(e);
            }
            _ => {
                crate::process::set_last_error(6);
                return 0;
            }
        }
    }
    // Handles hérités par un programme Windows (voir `inherit`) :
    // STARTUPINFOEX.lpAttributeList (décalage 104) peut en restreindre la
    // liste (PROC_THREAD_ATTRIBUTE_HANDLE_LIST).
    let passed = if is_pe && inherit_handles != 0 {
        let list = if creation_flags & 0x80000 != 0 && !startup.is_null() && (*startup).cb >= 112 {
            crate::dlls::kernel32_node::attribute_handle_list(std::ptr::read_unaligned(
                (startup as *const u8).add(104) as *const usize,
            ))
        } else {
            None
        };
        Some(crate::dlls::inherit::prepare(list.as_deref()))
    } else {
        None
    };
    // DETACHED_PROCESS / CREATE_NEW_PROCESS_GROUP : nouveau groupe de processus.
    if !pseudo_console && creation_flags & (0x8 | 0x200) != 0 {
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    }
    if let Some(env) = environment {
        cmd.env_clear();
        if is_pe {
            // Un bloc d'environnement Windows ne connaît pas l'affichage ni
            // la configuration de Winlux : Chromium en construit un sans
            // DISPLAY pour son processus GPU, qui basculait alors sans
            // fenêtre et échouait en boucle (#996).
            for (key, value) in linux_plumbing_env() {
                cmd.env(key, value);
            }
        }
        for (key, value) in env {
            cmd.env(key, value);
        }
    }
    // Programme Windows : il reçoit la ligne de commande exacte de
    // l'appelant, premier mot compris (Claude Code se relance sous le nom
    // « rg » pour servir de ripgrep).
    if is_pe && !line.trim().is_empty() {
        cmd.env("WINLUX_COMMAND_LINE", &line);
    }
    cmd.env_remove(crate::dlls::inherit::ENV);
    cmd.env_remove(crate::process::SUSPENDED_ENV);
    cmd.env_remove(crate::loader::SAME_BASE_ENV);
    if let Some(b) = same_base.as_ref() {
        cmd.env(crate::loader::SAME_BASE_ENV, b);
    }
    if suspended && is_pe {
        cmd.env(crate::process::SUSPENDED_ENV, "1");
    }
    if let Some(p) = passed.as_ref().filter(|p| !p.env.is_empty()) {
        cmd.env(crate::dlls::inherit::ENV, &p.env);
    }
    // Guichet de handles de l'enfant (DuplicateHandle vers lui, même
    // suspendu) : ouvert ici, hérité, renommé d'après son PID.
    cmd.env_remove(crate::dlls::handle_broker::ENV);
    // Gestionnaire de crashpad connu : l'enfant le désigne à son tour pour
    // qu'il puisse lire sa mémoire (#767).
    cmd.env_remove(CRASH_HANDLER_ENV);
    if let Some(pid) = crash_handler_pid() {
        cmd.env(CRASH_HANDLER_ENV, pid.to_string());
    }
    let desk = if is_pe {
        crate::dlls::handle_broker::prepare_child()
    } else {
        None
    };
    if let Some(d) = desk.as_ref() {
        cmd.env(crate::dlls::handle_broker::ENV, d.env());
    }
    let spawned = cmd.spawn();
    // Les copies héritables restent chez l'enfant ; le parent les ferme.
    drop(passed);
    if let (Some(d), Ok(c)) = (desk, spawned.as_ref()) {
        d.launched(c.id());
    }
    if let (true, Ok(c)) = (suspended && is_pe, spawned.as_ref()) {
        crate::sandbox_fallback::note_child(c.id(), &line);
        // Sous Windows, CreateProcess rend la main avec l'enfant déjà figé.
        // Ici l'enfant se fige lui-même (SIGSTOP) : attendre qu'il le soit,
        // sinon un ResumeThread trop rapide envoie SIGCONT avant le SIGSTOP
        // et l'enfant reste figé pour toujours.
        wait_until_stopped(c.id());
    }
    // Gestionnaire de plantage de crashpad (Chromium, Electron) : il lit
    // notre mémoire pour écrire son rapport. Linux (Yama) ne laisse un
    // enfant lire son parent que si celui-ci l'a désigné.
    if let Ok(c) = spawned.as_ref() {
        if is_pe && line.contains("--type=crashpad-handler") {
            libc::prctl(libc::PR_SET_PTRACER, c.id() as libc::c_ulong, 0, 0, 0);
            CRASH_HANDLER.store(c.id() as i32, Ordering::Relaxed);
        }
    }
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            crate::trace!("CreateProcess({:?}) : échec {e}", cmd.get_program());
            crate::process::set_last_error(process_error(&e));
            return 0;
        }
    };
    let pid = child.id();
    crate::trace!(
        "CreateProcess({:?} {:?}) = pid {pid}{}",
        cmd.get_program(),
        cmd.get_args().collect::<Vec<_>>(),
        if suspended { " (suspendu)" } else { "" }
    );
    if suspended {
        let mut status = 0;
        let waited = loop {
            let r = libc::waitpid(pid as i32, &mut status, libc::WUNTRACED);
            if r >= 0 {
                break r;
            }
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            let _ = child.kill();
            let _ = child.wait();
            crate::process::set_last_error(31);
            return 0;
        };
        if waited != pid as i32 || (status & 0xff) != 0x7f {
            let _ = child.kill();
            let _ = child.wait();
            crate::process::set_last_error(31);
            return 0;
        }
    }
    let object = Arc::new(ProcessObject {
        state: Mutex::new(ProcessState {
            child: Some(child),
            foreign: None,
            exit_code: 259,
            suspended,
        }),
        changed: Condvar::new(),
    });
    let process_handle = add_sync(SyncHandle::Process(object.clone()), None);
    let thread_handle = add_sync(SyncHandle::Process(object), None);
    // Lancé par un membre d'un job : l'enfant le rejoint (#19).
    crate::dlls::jobs::on_child_created(pid as i32, process_handle, creation_flags);
    std::ptr::write(
        info,
        ProcessInformation {
            process: process_handle,
            thread: thread_handle,
            process_id: pid,
            thread_id: pid,
        },
    );
    1
}
pub unsafe extern "win64" fn create_process_a(
    application: *const u8,
    command_line: *const u8,
    process_attributes: usize,
    thread_attributes: usize,
    inherit: i32,
    flags: u32,
    environment: *const c_void,
    current_directory: *const u8,
    startup: *const StartupInfoA,
    info: *mut ProcessInformation,
) -> i32 {
    if application.is_null() && command_line.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let app = if application.is_null() {
        None
    } else {
        Some(crate::dlls::cstr(application))
    };
    let env = match parse_environment_block(environment, flags & 0x400 != 0) {
        Ok(e) => e,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let _ = (process_attributes, thread_attributes);
    create_process_common(
        app,
        command_line,
        inherit,
        flags,
        env,
        current_directory,
        startup,
        info,
    )
}
pub unsafe extern "win64" fn create_process_w(
    application: *const u16,
    command_line: *mut u16,
    process_attributes: usize,
    thread_attributes: usize,
    inherit: i32,
    flags: u32,
    environment: *const c_void,
    current_directory: *const u16,
    startup: *const StartupInfoA,
    info: *mut ProcessInformation,
) -> i32 {
    let app = if application.is_null() {
        None
    } else {
        let s = match cstring_from_wide(application) {
            Ok(s) => s,
            Err(e) => {
                crate::process::set_last_error(e);
                return 0;
            }
        };
        Some(s.to_string_lossy().into_owned())
    };
    let line = if command_line.is_null() {
        None
    } else {
        let n = crate::printf::wide_len(command_line);
        let s = match String::from_utf16(std::slice::from_raw_parts(command_line, n)) {
            Ok(s) => s,
            Err(_) => {
                crate::process::set_last_error(1113);
                return 0;
            }
        };
        let bytes = crate::dlls::encode_ansi(&s);
        match std::ffi::CString::new(bytes) {
            Ok(c) => Some(c),
            Err(_) => {
                crate::process::set_last_error(87);
                return 0;
            }
        }
    };
    crate::log_debug!("process", "CreateProcessW {app:?} {line:?}");
    if app.is_none() && line.is_none() {
        crate::process::set_last_error(87);
        return 0;
    }
    let cwd = if current_directory.is_null() {
        None
    } else {
        match cstring_from_wide(current_directory) {
            Ok(s) => Some(s),
            Err(e) => {
                crate::process::set_last_error(e);
                return 0;
            }
        }
    };
    let env = match parse_environment_block(environment, flags & 0x400 != 0) {
        Ok(e) => e,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let _ = (process_attributes, thread_attributes);
    create_process_common(
        app,
        line.as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr() as *const u8),
        inherit,
        flags,
        env,
        cwd.as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr() as *const u8),
        startup,
        info,
    )
}
pub unsafe extern "win64" fn open_process(_access: u32, inherit: i32, pid: u32) -> usize {
    if pid == 0 || pid > i32::MAX as u32 {
        crate::process::set_last_error(87);
        return 0;
    }
    if pid == libc::getpid() as u32 {
        // Vrai handle vers soi (jamais -1, qui vaut INVALID_HANDLE_VALUE) :
        // crashpad ouvre ainsi son propre processus, héritable, et passe la
        // valeur à son gestionnaire sur la ligne de commande.
        let h = add_sync(foreign_process(pid as i32), None);
        crate::dlls::inherit::set(h, inherit != 0);
        return h;
    }
    // Les PID Windows sont les PID Linux : tout processus vivant s'ouvre, les
    // droits réels sont vérifiés par le noyau au moment d'agir (kill, nice).
    if !os_process_alive(pid as i32) {
        crate::process::set_last_error(87);
        return 0;
    }
    let object = Arc::new(ProcessObject {
        state: Mutex::new(ProcessState {
            child: None,
            foreign: Some(pid as i32),
            exit_code: 259,
            suspended: false,
        }),
        changed: Condvar::new(),
    });
    add_sync(SyncHandle::Process(object), None)
}
/// Vrai tant que le processus existe (zombie compris : il n'est pas réclamé).
fn os_process_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
/// Processus terminé mais pas encore réclamé par son parent (zombie). Un
/// meneur de groupe sorti seul (ExitThread du fil principal) apparaît aussi
/// « Z » tant que ses autres fils tournent : il n'est fini que s'il ne reste
/// plus que lui dans /proc/<pid>/task.
pub(crate) fn os_process_zombie(pid: i32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let state = stat
        .rfind(')')
        .and_then(|i| stat[i + 1..].trim_start().chars().next());
    if !matches!(state, Some('Z' | 'X')) {
        return false;
    }
    std::fs::read_dir(format!("/proc/{pid}/task")).map_or(true, |d| d.count() <= 1)
}
pub unsafe extern "win64" fn get_exit_code_process(handle: usize, code: *mut u32) -> i32 {
    if code.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(SyncHandle::Process(p)) = lookup_sync(handle) else {
        crate::process::set_last_error(6);
        return 0;
    };
    poll_process(&p);
    *code = lock(&p.state).exit_code;
    1
}
pub unsafe extern "win64" fn terminate_process(handle: usize, code: u32) -> i32 {
    if is_current_process_handle(handle) {
        // TerminateProcess(GetCurrentProcess()) : le processus s'arrête tout de
        // suite, sans rendre la main ni lancer les destructeurs (Crashpad,
        // Chromium et les CRT s'en servent comme d'un abort).
        terminate_self(code);
    }
    let Some(SyncHandle::Process(p)) = lookup_sync(handle) else {
        crate::process::set_last_error(6);
        return 0;
    };
    // Enfant de Chromium tué avant d'avoir démarré : son bac à sable n'a
    // pas pu être préparé (voir sandbox_fallback). La question est posée
    // avant de verrouiller le processus.
    let (child_pid, still_suspended) = {
        let s = lock(&p.state);
        (s.child.as_ref().map(|c| c.id() as i32), s.suspended)
    };
    if let Some(pid) = child_pid {
        crate::sandbox_fallback::on_terminate(pid, still_suspended);
    }
    let mut s = lock(&p.state);
    let target = s
        .foreign
        .or_else(|| s.child.as_ref().map(|c| c.id() as i32));
    if let Some(pid) = target {
        // Crashpad arrête ainsi le processus qui a planté : sans cette
        // ligne, l'enfant disparaissait sans laisser de trace.
        let line = format!(
            "winlux: TerminateProcess du processus {pid} (code {code:#x}) demandé par le processus {}",
            std::process::id()
        );
        eprintln!("{line}");
        crate::thunks::api_note(&line);
    }
    if let Some(pid) = s.foreign {
        // TerminateProcess est immédiat sous Windows : SIGKILL.
        if libc::kill(pid, libc::SIGKILL) != 0 {
            let gone = std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            crate::process::set_last_error(if gone { 87 } else { 5 });
            return 0;
        }
        s.exit_code = code;
        return 1;
    }
    let Some(child) = s.child.as_mut() else {
        crate::process::set_last_error(5);
        return 0;
    };
    crate::dlls::handle_broker::forget_pid(child.id() as i32);
    if child.kill().is_err() {
        crate::process::set_last_error(5);
        return 0;
    }
    if let Some(mut child) = s.child.take() {
        let _ = child.wait();
    }
    s.exit_code = code;
    p.changed.notify_all();
    1
}
/// Ce que désigne un handle, pour la trace des fils (#2161) : « fil 1234
/// vivant », « fil 1234 fini (0) », « événement »… Lecture seule.
pub(crate) fn describe_handle(h: usize) -> String {
    if h == (-2isize) as usize {
        return format!("fil courant {}", unsafe { libc::syscall(libc::SYS_gettid) });
    }
    if h == usize::MAX {
        return "processus courant".into();
    }
    match lookup_sync(h) {
        Some(SyncHandle::Thread(t)) => {
            let s = lock(&t.state);
            if s.finished {
                format!("fil {} fini ({:#x})", s.win_tid, s.exit_code)
            } else {
                format!("fil {} vivant", s.win_tid)
            }
        }
        Some(SyncHandle::Process(_)) => "processus".into(),
        Some(SyncHandle::Event(_)) => "événement".into(),
        Some(SyncHandle::Mutex(_)) => "mutex".into(),
        Some(SyncHandle::Semaphore(_)) => "sémaphore".into(),
        Some(SyncHandle::CompletionPort(_)) => "port de complétion".into(),
        None => "autre".into(),
    }
}
pub unsafe extern "win64" fn get_exit_code_thread(handle: usize, code: *mut u32) -> i32 {
    if code.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    match lookup_sync(handle) {
        Some(SyncHandle::Thread(t)) => {
            let s = lock(&t.state);
            *code = if s.finished { s.exit_code } else { 259 };
            1
        }
        Some(SyncHandle::Process(p)) => {
            poll_process(&p);
            *code = lock(&p.state).exit_code;
            1
        }
        _ => {
            crate::process::set_last_error(6);
            0
        }
    }
}
pub extern "win64" fn resume_thread(handle: usize) -> u32 {
    let Some(obj) = lookup_sync(handle) else {
        crate::process::set_last_error(6);
        return u32::MAX;
    };
    match obj {
        SyncHandle::Thread(t) => {
            let _gate = sync_gate();
            let mut s = lock(&t.state);
            if s.finished {
                crate::process::set_last_error(6);
                return u32::MAX;
            }
            if s.running {
                drop(s);
                return super::thread_ctl::resume(&t.slot);
            }
            let old = s.suspend_count;
            if old > 0 {
                s.suspend_count -= 1;
                if s.suspend_count == 0 {
                    t.changed.notify_all();
                }
            }
            old.min(u32::MAX as usize) as u32
        }
        SyncHandle::Process(p) => {
            let mut s = lock(&p.state);
            if !s.suspended {
                return 0;
            }
            let Some(c) = s.child.as_mut() else {
                crate::process::set_last_error(6);
                return u32::MAX;
            };
            if unsafe { libc::kill(c.id() as i32, libc::SIGCONT) } != 0 {
                crate::process::set_last_error(5);
                return u32::MAX;
            }
            s.suspended = false;
            1
        }
        _ => {
            crate::process::set_last_error(6);
            u32::MAX
        }
    }
}
/// Extrait le descripteur de fichier Linux (fd) depuis un handle Windows.
/// Utilisé par le module mmap pour CreateFileMappingA.
pub fn handle_to_fd(h: usize) -> i32 {
    use std::os::unix::io::AsRawFd;
    let h = canon_std(h);
    match h as isize {
        -10 => return 0,
        -11 => return 1,
        -12 => return 2,
        _ => {}
    }
    let handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
    match handles.get(&h) {
        Some(FileHandle::File(f)) => lock(f).as_raw_fd(),
        _ => -1,
    }
}
pub extern "win64" fn close_handle(handle: usize) -> i32 {
    // Pseudo-handles (processus -1, thread -2, jetons -4 à -6) : fermer ne
    // fait rien et réussit, comme sous Windows (crashpad de Warzone 2100).
    if matches!(handle as isize, -6..=-1) {
        return 1;
    }
    // Événement de WSACreateEvent fermé par CloseHandle (#1360).
    if lookup_sync(handle).is_none() && crate::dlls::ws2_32::is_wsa_event(handle) {
        return crate::dlls::ws2_32::wsa_close_event(handle);
    }
    // \Device\Afd (mio, #1766) : ses sondages en cours sont annulés.
    crate::dlls::afd::closed(handle);
    if matches!(handle, STD_INPUT_REAL | STD_OUTPUT_REAL | STD_ERROR_REAL) {
        // Fermer un handle standard ne ferme pas le terminal de Winlux.
        return 1;
    }
    // Jeton d'accès (OpenProcessToken) : partagé, rien à libérer. Chromium
    // (Electron) vérifie que sa fermeture réussit.
    if handle == crate::dlls::security::TOKEN_HANDLE {
        return 1;
    }
    if handle == 1
        || handle == 2
        || handle == (-10isize) as usize
        || handle == (-11isize) as usize
        || handle == (-12isize) as usize
    {
        crate::process::set_last_error(6);
        return 0;
    }
    crate::dlls::ntdll_file::forget_dir_cursor(handle);
    crate::dlls::named_pipes::close_server(handle);
    crate::dlls::inherit::set(handle, false);
    if crate::dlls::kernel32_node::close_snapshot(handle)
        || crate::dlls::jobs::close(handle)
        || crate::dlls::kernel32_releve::close(handle)
    {
        return 1;
    }
    // Effacement à la fermeture : au nom actuel du fichier, qui a pu être
    // renommé depuis son ouverture (#1885).
    let doomed = lock(delete_on_close()).remove(&handle).and_then(|_| {
        crate::dlls::doomed::forget(handle);
        doomed_on_close(handle_to_fd(handle))
    });
    if lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())))
        .remove(&handle)
        .is_some()
    {
        lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&handle);
        lock(overlapped_handles()).remove(&handle);
        forget_dir_watcher(handle);
        if let Some((path, dev, ino)) = doomed {
            // Le nom doit toujours désigner le même fichier : jamais un
            // autre qui l'aurait pris entre-temps.
            let same = std::fs::symlink_metadata(&path).is_ok_and(|m| {
                use std::os::unix::fs::MetadataExt;
                m.dev() == dev && m.ino() == ino
            });
            let removed = if !same {
                Err(std::io::Error::other("nom repris par un autre fichier"))
            } else if path.is_dir() {
                std::fs::remove_dir(&path)
            } else {
                std::fs::remove_file(&path)
            };
            crate::trace!(
                "CloseHandle({handle:#x}) : effacement à la fermeture de {} = {}",
                path.display(),
                removed.map_or_else(|e| e.to_string(), |_| "ok".into())
            );
        }
        1
    } else if crate::dlls::mmap::close_mapping(handle) || remove_sync(handle) {
        remove_waitable_timer(handle);
        1
    } else {
        crate::trace!("CloseHandle({handle:#x}) : handle inconnu");
        crate::process::set_last_error(6);
        0
    }
}
pub unsafe extern "win64" fn create_event_a(
    _sec: usize,
    manual: i32,
    initial: i32,
    name: *const u8,
) -> usize {
    let obj = SyncHandle::Event(Arc::new(EventObject::new(manual != 0, initial != 0)));
    let h = create_sync(obj, named_event(name));
    crate::dlls::inherit::from_security_attributes(h, _sec);
    h
}

/// Create{Event,Mutex,Semaphore,WaitableTimer} : GetLastError est remis à 0
/// avant la création, puis vaut ERROR_ALREADY_EXISTS (183) seulement si
/// l'objet nommé existait, comme Windows (#2079). Un IPCWrapper (flux maître
/// de Steam) teste GetLastError() pour savoir s'il a créé l'objet : nous
/// laissions la valeur d'un appel précédent (122, ou un 183 resté d'un
/// CreateFileMapping qui faisait croire qu'un canal neuf existait déjà).
fn create_sync(obj: SyncHandle, name: Option<String>) -> usize {
    crate::process::set_last_error(0);
    add_sync(obj, name)
}
pub extern "win64" fn set_event(h: usize) -> i32 {
    let Some(SyncHandle::Event(v)) = lookup_sync(h) else {
        // Événement de WSACreateEvent : hEvent d'un WSASend de Chromium,
        // signalé à la fin de l'E/S (#1360).
        if crate::dlls::ws2_32::is_wsa_event(h) {
            return crate::dlls::ws2_32::wsa_set_event(h);
        }
        crate::process::set_last_error(6);
        return 0;
    };
    if let Some(n) = traced_name(h) {
        crate::trace!("SetEvent({n}, 0x{h:x})");
    }
    let _gate = sync_gate();
    v.store(&mut lock(&v.signaled), true);
    if let Some(w) = v.shared.get() {
        w.wake();
    }
    if v.manual_reset {
        v.changed.notify_all()
    } else {
        v.changed.notify_one()
    }
    1
}
pub extern "win64" fn reset_event(h: usize) -> i32 {
    let Some(SyncHandle::Event(v)) = lookup_sync(h) else {
        if crate::dlls::ws2_32::is_wsa_event(h) {
            return crate::dlls::ws2_32::wsa_reset_event(h);
        }
        crate::process::set_last_error(6);
        return 0;
    };
    if let Some(n) = traced_name(h) {
        crate::trace!("ResetEvent({n}, 0x{h:x})");
    }
    let _gate = sync_gate();
    v.store(&mut lock(&v.signaled), false);
    1
}
pub unsafe extern "win64" fn create_mutex_a(_sec: usize, initial: i32, name: *const u8) -> usize {
    let obj = SyncHandle::Mutex(Arc::new(MutexObject::new(initial != 0)));
    create_sync(obj, named_event(name))
}
pub extern "win64" fn release_mutex(h: usize) -> i32 {
    let Some(SyncHandle::Mutex(m)) = lookup_sync(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let ok = leave_mutex(&m);
    if let Some(n) = traced_name(h) {
        crate::trace!("ReleaseMutex({n}, 0x{h:x}) = {ok}");
    }
    i32::from(ok)
}

/// System.Threading.Mutex de la mscorlib Winlux (#1988) : le mutex .NET est
/// le mutex nommé de kernel32, donc partagé avec les autres processus du
/// compte (#20). Rend le handle (0 en cas d'échec) et le dernier code
/// d'erreur : 183 quand le nom existait déjà.
pub(crate) fn clr_mutex_create(owned: bool, name: Option<&str>) -> (usize, u32) {
    crate::process::set_last_error(0);
    let c = name
        .filter(|n| !n.is_empty())
        .and_then(|n| std::ffi::CString::new(n).ok());
    let h = unsafe {
        create_mutex_a(
            0,
            owned as i32,
            c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr().cast()),
        )
    };
    (h, crate::process::last_error())
}
/// Semaphore de la mscorlib (#1988) : sémaphore de kernel32, nommé ou non.
pub(crate) fn clr_semaphore_create(initial: i32, maximum: i32, name: Option<&str>) -> (usize, u32) {
    crate::process::set_last_error(0);
    let c = name
        .filter(|n| !n.is_empty())
        .and_then(|n| std::ffi::CString::new(n).ok());
    let h = unsafe {
        create_semaphore_a(
            0,
            initial,
            maximum,
            c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr().cast()),
        )
    };
    (h, crate::process::last_error())
}
/// Mutex.OpenExisting et Semaphore.OpenExisting (#1988) : 0 et le code
/// d'erreur (2 = introuvable, 6 = le nom désigne un autre type d'objet).
pub(crate) fn clr_sync_open(name: &str, semaphore: bool) -> (usize, u32) {
    crate::process::set_last_error(0);
    let kind = if semaphore {
        SHARED_KIND_SEMAPHORE
    } else {
        SHARED_KIND_MUTEX
    };
    let h = open_sync_by_name(name, kind);
    (h, crate::process::last_error())
}

unsafe fn open_named_sync(name: *const u8, event: bool) -> usize {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let kind = if event {
        SHARED_KIND_EVENT
    } else {
        SHARED_KIND_MUTEX
    };
    open_sync_by_name(&crate::dlls::cstr(name), kind)
}
/// Open{Event,Mutex,Semaphore} : objet nommé de ce processus, ou d'un autre
/// processus du même compte (#20).
fn open_sync_by_name(name: &str, kind: u32) -> usize {
    let h = open_sync_by_name_inner(name, kind);
    remember_name(h, name);
    crate::trace!(
        "Open{}({name}) = 0x{h:x} (erreur {})",
        kind_label(kind),
        if h == 0 {
            crate::process::last_error()
        } else {
            0
        }
    );
    h
}

fn open_sync_by_name_inner(name: &str, kind: u32) -> usize {
    let key = crate::dlls::named_objects::name_key(name);
    let mut names = lock(SYNC_NAMES.get_or_init(|| Mutex::new(HashMap::new())));
    if !names.contains_key(&key) {
        match share_named(&key, kind, None) {
            Some(Ok((obj, _))) => {
                crate::dlls::named_objects::retain(crate::dlls::named_objects::SYNC, &key);
                names.insert(key.clone(), (obj, 0));
            }
            Some(Err(e)) => {
                crate::process::set_last_error(e);
                return 0;
            }
            None => {
                crate::process::set_last_error(2);
                return 0;
            }
        }
    }
    let Some((object, count)) = names.get_mut(&key) else {
        crate::process::set_last_error(2);
        return 0;
    };
    if sync_kind(object) != kind {
        crate::process::set_last_error(6);
        return 0;
    }
    let h = next_handle();
    lock(SYNCS.get_or_init(|| Mutex::new(HashMap::new()))).insert(h, object.clone());
    *count += 1;
    h
}
/// OpenSemaphoreW
pub unsafe extern "win64" fn open_semaphore_w(
    _access: u32,
    inherit: i32,
    name: *const u16,
) -> usize {
    if inherit != 0 {
        crate::process::set_last_error(50);
        return 0;
    }
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    open_sync_by_name(&crate::dlls::wstr(name), SHARED_KIND_SEMAPHORE)
}
/// OpenSemaphoreA
pub unsafe extern "win64" fn open_semaphore_a(
    _access: u32,
    inherit: i32,
    name: *const u8,
) -> usize {
    if inherit != 0 {
        crate::process::set_last_error(50);
        return 0;
    }
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    open_sync_by_name(&crate::dlls::cstr(name), SHARED_KIND_SEMAPHORE)
}
#[cfg(test)]
mod reviewed_named_sync_tests {
    use super::*;
    #[test]
    fn opened_event_keeps_name_alive_until_last_close() {
        unsafe {
            let name = b"winlux-reviewed-event\0";
            let first = create_event_a(0, 1, 1, name.as_ptr());
            let second = open_event_a(0, 0, name.as_ptr());
            assert_ne!(second, 0);
            assert_eq!(open_mutex_a(0, 0, name.as_ptr()), 0);
            assert_eq!(close_handle(first), 1);
            let third = open_event_a(0, 0, name.as_ptr());
            assert_ne!(third, 0);
            assert_eq!(wait_for_single_object(third, 0), 0);
            assert_eq!(close_handle(second), 1);
            assert_eq!(close_handle(third), 1);
            assert_eq!(open_event_a(0, 0, name.as_ptr()), 0);
            let mutex = create_mutex_a(0, 0, name.as_ptr());
            let opened = open_mutex_a(0, 0, name.as_ptr());
            assert_ne!(opened, 0);
            assert_eq!(close_handle(mutex), 1);
            assert_eq!(close_handle(opened), 1);
        }
    }
}
#[cfg(test)]
mod created_last_error_tests {
    use super::*;

    /// Création réussie : GetLastError vaut 0, ou 183 si le nom existait,
    /// jamais la valeur d'un appel précédent (#2079).
    #[test]
    fn creation_reussie_remet_last_error_a_zero() {
        let _teb = crate::process::test_teb();
        unsafe {
            let name = format!("winlux-2079-{}\0", std::process::id());
            let n = name.as_ptr();
            for named in [false, true] {
                let p = if named { n } else { std::ptr::null() };
                // 183 resté d'un appel précédent : pas « existait déjà ».
                crate::process::set_last_error(183);
                let ev = create_event_a(0, 1, 0, p);
                assert_ne!(ev, 0);
                assert_eq!(crate::process::last_error(), 0, "CreateEvent nommé={named}");
                crate::process::set_last_error(122);
                let again = create_event_a(0, 1, 0, p);
                assert_eq!(
                    crate::process::last_error(),
                    if named { 183 } else { 0 },
                    "second CreateEvent nommé={named}"
                );
                close_handle(again);
                close_handle(ev);
            }
            let m = format!("winlux-2079-m-{}\0", std::process::id());
            crate::process::set_last_error(122);
            let mx = create_mutex_a(0, 0, m.as_ptr());
            assert_ne!(mx, 0);
            assert_eq!(crate::process::last_error(), 0, "CreateMutex");
            close_handle(mx);
            crate::process::set_last_error(122);
            let sem = create_semaphore_a(0, 0, 1, std::ptr::null());
            assert_ne!(sem, 0);
            assert_eq!(crate::process::last_error(), 0, "CreateSemaphore");
            close_handle(sem);
            crate::process::set_last_error(122);
            let t = create_waitable_timer_a(0, 1, std::ptr::null());
            assert_ne!(t, 0);
            assert_eq!(crate::process::last_error(), 0, "CreateWaitableTimer");
            close_handle(t);
        }
    }
}
pub unsafe extern "win64" fn open_event_a(_access: u32, _inherit: i32, name: *const u8) -> usize {
    if _inherit != 0 {
        crate::process::set_last_error(50);
        return 0;
    }
    open_named_sync(name, true)
}
pub unsafe extern "win64" fn open_mutex_a(_access: u32, _inherit: i32, name: *const u8) -> usize {
    if _inherit != 0 {
        crate::process::set_last_error(50);
        return 0;
    }
    open_named_sync(name, false)
}
pub extern "win64" fn wait_for_single_object(h: usize, timeout: u32) -> u32 {
    if !trace_on() {
        return wait_for_single_object_inner(h, timeout);
    }
    let start = std::time::Instant::now();
    let r = wait_for_single_object_inner(h, timeout);
    if let Some(n) = traced_name(h) {
        crate::trace!("WaitForSingleObject({n}, 0x{h:x}, {timeout} ms) = 0x{r:x}");
    } else {
        trace_long_wait("WaitForSingleObject", &[h], timeout, r, start);
    }
    r
}

/// WINLUX_TRACE (#2072) : attente d'au moins 200 ms sur des handles
/// anonymes, avec le type de chaque objet, pour savoir où un processus
/// s'est arrêté sans attente nommée visible.
fn trace_long_wait(api: &str, hs: &[usize], timeout: u32, r: u32, start: std::time::Instant) {
    let took = start.elapsed();
    if took < std::time::Duration::from_millis(200) {
        return;
    }
    let what: Vec<String> = hs
        .iter()
        .map(|&h| format!("{} 0x{h:x}", object_type_name(h).unwrap_or("?")))
        .collect();
    crate::trace!(
        "{api}([{}], {timeout} ms) = 0x{r:x} après {} ms (fil {})",
        what.join(", "),
        took.as_millis(),
        os_thread_id()
    );
}
fn wait_for_single_object_inner(h: usize, timeout: u32) -> u32 {
    // Entrée de la console (terminal) : signalée quand une touche attend.
    if crate::dlls::console_input::is_console_input(h) {
        return if crate::dlls::console_input::wait(timeout) {
            0
        } else {
            258
        };
    }
    let Some(obj) = lookup_sync(h) else {
        // Événement de WSACreateEvent : Chromium (serveur DevTools) l'attend
        // par ObjectWatcher, donc par WaitForSingleObject (#1351).
        if crate::dlls::ws2_32::is_wsa_event(h) {
            return wait_wsa_events(&[h], timeout);
        }
        crate::process::set_last_error(6);
        return u32::MAX;
    };
    match obj {
        // Événement associé à une socket par WSAEventSelect : la pile réseau
        // de Windows le signale ; ici, l'attente interroge la socket.
        SyncHandle::Event(e) if crate::dlls::ws2_32::has_socket_assoc(h) => {
            let start = std::time::Instant::now();
            loop {
                if crate::dlls::ws2_32::handle_ready(h) || wait_event(&e, 0) == 0 {
                    return 0;
                }
                if timeout != u32::MAX && start.elapsed().as_millis() >= timeout as u128 {
                    return 258;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        SyncHandle::Event(e) => wait_event(&e, timeout),
        SyncHandle::Mutex(m) => wait_mutex(&m, timeout),
        SyncHandle::Thread(t) => wait_thread(&t, timeout),
        SyncHandle::Process(p) => wait_process(&p, timeout),
        SyncHandle::CompletionPort(p) => wait_iocp(&p, timeout),
        SyncHandle::Semaphore(sem) => wait_semaphore(&sem, timeout),
    }
}
/// Attente « un seul parmi » qui contient des événements de WSACreateEvent
/// (#1351) : ceux-ci, et les autres handles, sont interrogés sans attente
/// jusqu'au premier prêt. Un événement WSA est à réinitialisation manuelle :
/// l'interroger ne le consomme pas.
fn wait_wsa_events(hs: &[usize], timeout: u32) -> u32 {
    let start = std::time::Instant::now();
    loop {
        for (i, &h) in hs.iter().enumerate() {
            let ready = if lookup_sync(h).is_none() {
                if !crate::dlls::ws2_32::is_wsa_event(h) {
                    crate::process::set_last_error(6);
                    return u32::MAX;
                }
                crate::dlls::ws2_32::handle_ready(h)
            } else {
                match wait_for_single_object(h, 0) {
                    0 => true,
                    0x80 => return 0x80 + i as u32,
                    u32::MAX => return u32::MAX,
                    _ => false,
                }
            };
            if ready {
                return i as u32;
            }
        }
        if timeout != u32::MAX && start.elapsed().as_millis() >= timeout as u128 {
            return 258;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
pub unsafe extern "win64" fn wait_for_multiple_objects(
    count: u32,
    handles: *const usize,
    wait_all: i32,
    timeout: u32,
) -> u32 {
    let names: Vec<String> = if trace_on() && !handles.is_null() && (1..=64).contains(&count) {
        std::slice::from_raw_parts(handles, count as usize)
            .iter()
            .map(|&h| traced_name(h).unwrap_or_else(|| format!("0x{h:x}")))
            .collect()
    } else {
        Vec::new()
    };
    let start = std::time::Instant::now();
    let r = wait_for_multiple_objects_inner(count, handles, wait_all, timeout);
    if names.len() != count as usize {
        return r;
    }
    if names.iter().all(|n| n.starts_with("0x")) {
        trace_long_wait(
            "WaitForMultipleObjects",
            std::slice::from_raw_parts(handles, count as usize),
            timeout,
            r,
            start,
        );
        return r;
    }
    crate::trace!(
        "WaitForMultipleObjects([{}], tous {}, {timeout} ms) = 0x{r:x}",
        names.join(", "),
        wait_all != 0
    );
    r
}
unsafe fn wait_for_multiple_objects_inner(
    count: u32,
    handles: *const usize,
    wait_all: i32,
    timeout: u32,
) -> u32 {
    if count == 0 || count > 64 || handles.is_null() {
        crate::process::set_last_error(87);
        return u32::MAX;
    }
    let hs = std::slice::from_raw_parts(handles, count as usize);
    if wait_all == 0
        && hs
            .iter()
            .any(|&h| lookup_sync(h).is_none() && crate::dlls::ws2_32::is_wsa_event(h))
    {
        return wait_wsa_events(hs, timeout);
    }
    let mut objects = Vec::with_capacity(hs.len());
    for &h in hs {
        let Some(o) = lookup_sync(h) else {
            crate::process::set_last_error(6);
            return u32::MAX;
        };
        objects.push(o);
    }
    let start = std::time::Instant::now();
    if wait_all != 0 {
        for i in 0..objects.len() {
            if objects[..i].iter().any(|o| same_sync(o, &objects[i])) {
                crate::process::set_last_error(87);
                return u32::MAX;
            }
        }
        loop {
            let gate = sync_gate();
            let me = std::thread::current().id();
            let ready = objects.iter().zip(hs).all(|(o, &h)| match o {
                SyncHandle::Event(e) => {
                    e.is_set(&lock(&e.signaled)) || crate::dlls::ws2_32::handle_ready(h)
                }
                SyncHandle::Mutex(m) => match m.shared.get() {
                    Some(w) => MutexObject::shared_ready(w),
                    None => {
                        let s = lock(&m.state);
                        s.owner.is_none() || s.owner.as_ref() == Some(&me)
                    }
                },
                SyncHandle::Thread(t) => lock(&t.state).finished,
                SyncHandle::Process(p) => poll_process(p),
                SyncHandle::CompletionPort(p) => iocp_is_signaled(p),
                SyncHandle::Semaphore(sem) => match sem.shared.get() {
                    Some(w) => w.slot(0).load(Ordering::Acquire) > 0,
                    None => lock(&sem.count).0 > 0,
                },
            });
            if ready {
                for o in &objects {
                    match o {
                        SyncHandle::Event(e) => {
                            e.take(&mut lock(&e.signaled));
                        }
                        SyncHandle::Mutex(m) => {
                            if let Some(w) = m.shared.get() {
                                MutexObject::shared_try(w);
                                continue;
                            }
                            let mut s = lock(&m.state);
                            if s.owner.as_ref() == Some(&me) {
                                s.recursion = s.recursion.saturating_add(1);
                            } else {
                                s.owner = Some(me);
                                s.owner_os = os_thread_id();
                                s.recursion = 1;
                            }
                        }
                        SyncHandle::Semaphore(sem) => match sem.shared.get() {
                            Some(w) => {
                                SemaphoreObject::shared_try(w);
                            }
                            None => lock(&sem.count).0 -= 1,
                        },
                        SyncHandle::Thread(_)
                        | SyncHandle::Process(_)
                        | SyncHandle::CompletionPort(_) => {}
                    }
                }
                return 0;
            }
            drop(gate);
            if timeout == 0 {
                return 258;
            }
            if timeout != u32::MAX && start.elapsed().as_millis() >= timeout as u128 {
                return 258;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    loop {
        for (i, obj) in objects.iter().enumerate() {
            let ready = match obj {
                // WinSCP (PuTTY) attend ainsi ses sockets (#1340).
                SyncHandle::Event(e) => {
                    wait_event(e, 0) == 0 || crate::dlls::ws2_32::handle_ready(hs[i])
                }
                SyncHandle::Mutex(m) => match wait_mutex(m, 0) {
                    0 => true,
                    0x80 => return 0x80 + i as u32,
                    _ => false,
                },
                SyncHandle::Thread(t) => wait_thread(t, 0) == 0,
                SyncHandle::Process(p) => poll_process(p),
                SyncHandle::CompletionPort(p) => iocp_is_signaled(p),
                SyncHandle::Semaphore(sem) => wait_semaphore(sem, 0) == 0,
            };
            if ready {
                return i as u32;
            }
        }
        if timeout == 0 {
            return 258;
        }
        if timeout != u32::MAX && start.elapsed().as_millis() >= timeout as u128 {
            return 258;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
/// Variables cachées « =C: » (dossier courant de chaque lecteur) : Windows
/// les admet en tête de nom, l'environnement Linux non. `_wchdir` du CRT
/// Microsoft écrit « =C: » après SetCurrentDirectoryW et échoue si
/// SetEnvironmentVariableW refuse (Steam, #1380).
fn hidden_env() -> std::sync::MutexGuard<'static, Vec<(String, String)>> {
    static V: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
    V.lock().unwrap_or_else(|e| e.into_inner())
}
fn is_hidden_env_name(name: &str) -> bool {
    name.len() > 1 && name.starts_with('=') && !name[1..].contains('=')
}
pub(crate) fn env_value(name: &str) -> Option<String> {
    if is_hidden_env_name(name) {
        return hidden_env()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone());
    }
    std::env::var_os(name)
        .or_else(|| {
            std::env::vars_os()
                .find(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
        })
        .map(|v| v.to_string_lossy().into_owned())
}
fn set_env_value(name: &str, value: Option<&str>) -> Result<(), u32> {
    if is_hidden_env_name(name) {
        let mut v = hidden_env();
        v.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        if let Some(val) = value {
            v.push((name.to_string(), val.to_string()));
        }
        return Ok(());
    }
    if name.is_empty() || name.contains('=') || name.contains('\0') {
        return Err(87);
    }
    if let Some(v) = value {
        std::env::set_var(name, v);
    } else {
        std::env::remove_var(name);
    }
    Ok(())
}
fn write_a_text(s: &str, buf: *mut u8, size: u32) -> u32 {
    let b = crate::dlls::encode_ansi(s);
    if buf.is_null() || size as usize <= b.len() {
        b.len().saturating_add(1).min(u32::MAX as usize) as u32
    } else {
        unsafe {
            std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
            *buf.add(b.len()) = 0;
        }
        b.len() as u32
    }
}
fn write_w_text(s: &str, buf: *mut u16, size: u32) -> u32 {
    unsafe { crate::dlls::put_w(s, buf, size) }
}
unsafe fn cstring_from_wide(s: *const u16) -> Result<std::ffi::CString, u32> {
    if s.is_null() {
        return Err(87);
    }
    let n = crate::printf::wide_len(s);
    let text = String::from_utf16(std::slice::from_raw_parts(s, n)).map_err(|_| 1113u32)?;
    std::ffi::CString::new(text).map_err(|_| 87)
}
fn module_path(module: usize) -> Option<String> {
    if module == 0 {
        crate::loader::main_module().map(|m| crate::paths::to_windows(&m.path))
    } else {
        crate::loader::find_module_by_addr(module)
            .map(|m| crate::paths::to_windows(&m.path))
            // DLL système intégrée : son chemin sous Windows (#1113).
            .or_else(|| crate::dlls::builtin_file_path(module))
            // Image chargée par DONT_RESOLVE_DLL_REFERENCES (#1809).
            .or_else(|| {
                (module & crate::loader::DATA_HANDLE_BITS == 0)
                    .then(|| crate::loader::data_image(module))
                    .flatten()
                    .filter(|m| m.base == module)
                    .map(|m| crate::paths::to_windows(&m.path))
            })
    }
}
pub unsafe extern "win64" fn get_environment_variable_a(
    name: *const u8,
    buf: *mut u8,
    size: u32,
) -> u32 {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let n = crate::dlls::cstr(name);
    let Some(v) = env_value(&n) else {
        crate::process::set_last_error(203);
        return 0;
    };
    if v.is_empty() {
        if !buf.is_null() && size > 0 {
            *buf = 0;
        }
        return 0;
    }
    let ret = write_a_text(&v, buf, size);
    if ret > size {
        crate::process::set_last_error(122);
        ret
    } else {
        ret
    }
}
pub unsafe extern "win64" fn set_environment_variable_a(name: *const u8, value: *const u8) -> i32 {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let n = crate::dlls::cstr(name);
    let v = if value.is_null() {
        None
    } else {
        Some(crate::dlls::cstr(value))
    };
    match set_env_value(&n, v.as_deref()) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(e);
            0
        }
    }
}
pub unsafe extern "win64" fn get_current_directory_a(size: u32, buf: *mut u8) -> u32 {
    let Ok(p) = std::env::current_dir() else {
        crate::process::set_last_error(31);
        return 0;
    };
    let ret = write_a_text(&crate::paths::to_windows(&p), buf, size);
    if ret > size {
        crate::process::set_last_error(122);
    }
    ret
}
pub unsafe extern "win64" fn set_current_directory_a(path: *const u8) -> i32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    set_current_directory_str(&crate::dlls::cstr(path))
}
/// Corps commun de `set_current_directory_a` et `set_current_directory_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
unsafe fn set_current_directory_str(path: &str) -> i32 {
    match std::env::set_current_dir(crate::paths::to_linux(path)) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
/// GetTempPath : TMP, puis TEMP, sinon le dossier temporaire du profil
/// (`%LOCALAPPDATA%\\Temp`), comme le documente Microsoft. Jamais /tmp par
/// défaut : c'est un tmpfs, et un installateur qui y décompresse des
/// centaines de Mo épuise la mémoire (#2326).
pub(crate) fn temp_path() -> String {
    let p = ["TMP", "TEMP"]
        .iter()
        .find_map(|k| env_value(k).filter(|v| !v.trim().is_empty()))
        .map(|v| crate::paths::to_linux(&v))
        .filter(|p| p.is_absolute())
        .unwrap_or_else(crate::paths::profile_temp);
    let _ = std::fs::create_dir_all(&p);
    let mut s = crate::paths::to_windows(&p);
    if !s.ends_with('\\') {
        s.push('\\');
    }
    s
}
pub unsafe extern "win64" fn get_temp_path_a(size: u32, buf: *mut u8) -> u32 {
    let ret = write_a_text(&temp_path(), buf, size);
    if ret > size {
        crate::process::set_last_error(122);
    }
    ret
}
pub unsafe extern "win64" fn get_module_file_name_a(module: usize, buf: *mut u8, size: u32) -> u32 {
    let Some(s) = module_path(module) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if buf.is_null() || size == 0 {
        crate::process::set_last_error(122);
        return 0;
    }
    let b = crate::dlls::encode_ansi(&s);
    if b.len() >= size as usize {
        // Tampon trop petit : chemin tronqué, terminé, nSize renvoyé et
        // ERROR_INSUFFICIENT_BUFFER (comportement Windows XP et suivants).
        let n = size as usize - 1;
        std::ptr::copy_nonoverlapping(b.as_ptr(), buf, n);
        *buf.add(n) = 0;
        crate::process::set_last_error(122);
        return size;
    }
    write_a_text(&s, buf, size)
}
pub unsafe extern "win64" fn get_file_attributes_a(path: *const u8) -> u32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return u32::MAX;
    }
    get_file_attributes_str(&crate::dlls::cstr(path))
}
/// Corps commun de `get_file_attributes_a` et `get_file_attributes_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
/// Attributs d'une DLL système intégrée, présente pour le programme mais
/// sans fichier sur le disque (#1113).
const BUILTIN_FILE_ATTRIBUTES: u32 = 0x20; // FILE_ATTRIBUTE_ARCHIVE

/// Fichier introuvable mais DLL système intégrée : remplit `data` comme
/// pour un fichier présent (taille et dates nulles).
unsafe fn builtin_attribute_data(
    e: &std::io::Error,
    win_path: &str,
    data: *mut Win32FileAttributeData,
) -> bool {
    if e.kind() != std::io::ErrorKind::NotFound || !crate::dlls::is_builtin_system_file(win_path) {
        return false;
    }
    std::ptr::write_bytes(
        data as *mut u8,
        0,
        std::mem::size_of::<Win32FileAttributeData>(),
    );
    (*data).attributes = BUILTIN_FILE_ATTRIBUTES;
    true
}

unsafe fn get_file_attributes_str(path: &str) -> u32 {
    let p = crate::paths::to_linux(path);
    match crate::dlls::winfile::attributes(&p) {
        Ok((a, _)) => a,
        Err(e)
            if e.kind() == std::io::ErrorKind::NotFound
                && crate::dlls::is_builtin_system_file(path) =>
        {
            BUILTIN_FILE_ATTRIBUTES
        }
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            u32::MAX
        }
    }
}
pub unsafe extern "win64" fn delete_file_a(path: *const u8) -> i32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    delete_file_str(&crate::dlls::cstr(path))
}
/// Corps commun de `delete_file_a` et `delete_file_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
unsafe fn delete_file_str(path: &str) -> i32 {
    let r = delete_file_str_inner(path);
    crate::trace!(
        "DeleteFile({path}) = {r} (erreur {})",
        if r == 0 {
            crate::process::last_error()
        } else {
            0
        }
    );
    r
}
unsafe fn delete_file_str_inner(path: &str) -> i32 {
    match std::fs::remove_file(crate::paths::to_linux(path)) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn create_directory_a(path: *const u8, _sec: usize) -> i32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    create_directory_str(&crate::dlls::cstr(path), _sec)
}
/// Corps commun de `create_directory_a` et `create_directory_w` : le chemin est déjà décodé
/// (ANSI pour « A », UTF-16 pour « W »), sans repasser par la page de code.
unsafe fn create_directory_str(path: &str, _sec: usize) -> i32 {
    let r = create_directory_str_inner(path, _sec);
    crate::trace!(
        "CreateDirectory({path}) = {r} (erreur {})",
        if r == 0 {
            crate::process::last_error()
        } else {
            0
        }
    );
    r
}
unsafe fn create_directory_str_inner(path: &str, _sec: usize) -> i32 {
    match std::fs::create_dir(crate::paths::to_linux(path)) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn create_file_w(
    path: *const u16,
    access: u32,
    share: u32,
    sec: usize,
    creation: u32,
    flags: u32,
    template: usize,
) -> usize {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return usize::MAX;
        }
    };
    let h = create_file_str(
        &s.to_string_lossy(),
        access,
        share,
        sec,
        creation,
        flags,
        template,
    );
    crate::trace!(
        "CreateFileW({}, accès {access:#x}, disposition {creation}, options {flags:#x}) = {h:#x} (erreur {})",
        s.to_string_lossy(),
        crate::process::last_error()
    );
    h
}
pub unsafe extern "win64" fn get_environment_variable_w(
    name: *const u16,
    buf: *mut u16,
    size: u32,
) -> u32 {
    let n = match cstring_from_wide(name) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let Some(v) = env_value(&n.to_string_lossy()) else {
        crate::process::set_last_error(203);
        return 0;
    };
    if v.is_empty() {
        if !buf.is_null() && size > 0 {
            *buf = 0;
        }
        return 0;
    }
    let ret = write_w_text(&v, buf, size);
    if ret > size {
        crate::process::set_last_error(122);
    }
    ret
}
pub unsafe extern "win64" fn set_environment_variable_w(
    name: *const u16,
    value: *const u16,
) -> i32 {
    let n = match cstring_from_wide(name) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let v = if value.is_null() {
        None
    } else {
        match cstring_from_wide(value) {
            Ok(s) => Some(s),
            Err(e) => {
                crate::process::set_last_error(e);
                return 0;
            }
        }
    };
    let value = v.map(|s| s.to_string_lossy().into_owned());
    match set_env_value(&n.to_string_lossy(), value.as_deref()) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(e);
            0
        }
    }
}
pub unsafe extern "win64" fn get_current_directory_w(size: u32, buf: *mut u16) -> u32 {
    let Ok(p) = std::env::current_dir() else {
        crate::process::set_last_error(31);
        return 0;
    };
    let ret = write_w_text(&crate::paths::to_windows(&p), buf, size);
    if ret > size {
        crate::process::set_last_error(122);
    }
    ret
}
pub unsafe extern "win64" fn set_current_directory_w(path: *const u16) -> i32 {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    set_current_directory_str(&s.to_string_lossy())
}
pub unsafe extern "win64" fn get_temp_path_w(size: u32, buf: *mut u16) -> u32 {
    let ret = write_w_text(&temp_path(), buf, size);
    if ret > size {
        crate::process::set_last_error(122);
    }
    ret
}
pub unsafe extern "win64" fn get_module_file_name_w(
    module: usize,
    buf: *mut u16,
    size: u32,
) -> u32 {
    let Some(s) = module_path(module) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if buf.is_null() || size == 0 {
        crate::process::set_last_error(122);
        return 0;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    if w.len() >= size as usize {
        let n = size as usize - 1;
        std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
        *buf.add(n) = 0;
        crate::process::set_last_error(122);
        return size;
    }
    write_w_text(&s, buf, size)
}
pub unsafe extern "win64" fn get_file_attributes_w(path: *const u16) -> u32 {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return u32::MAX;
        }
    };
    let path = s.to_string_lossy();
    let r = get_file_attributes_str(&path);
    crate::trace!("GetFileAttributesW({path}) = {r:#x}");
    r
}
pub unsafe extern "win64" fn delete_file_w(path: *const u16) -> i32 {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    delete_file_str(&s.to_string_lossy())
}
pub unsafe extern "win64" fn create_directory_w(path: *const u16, sec: usize) -> i32 {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    create_directory_str(&s.to_string_lossy(), sec)
}
/// `GetFullPathName` travaille sur le texte, comme Windows : pas d'accès
/// disque, liens non suivis, `C:\Users\<nom>\Documents` reste tel quel.
fn full_windows_path(name: &str) -> Result<String, u32> {
    if name.is_empty() {
        return Err(87);
    }
    Ok(crate::paths::full_windows(name))
}
pub unsafe extern "win64" fn get_full_path_name_a(
    name: *const u8,
    size: u32,
    buf: *mut u8,
    file_part: *mut *mut u8,
) -> u32 {
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let path = match full_windows_path(&crate::dlls::cstr(name)) {
        Ok(p) => p,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let bytes = crate::dlls::encode_ansi(&path);
    let needed = bytes.len().saturating_add(1);
    if needed > u32::MAX as usize {
        crate::process::set_last_error(87);
        return 0;
    }
    if buf.is_null() || (size as usize) < needed {
        if !file_part.is_null() {
            *file_part = std::ptr::null_mut();
        }
        return needed as u32;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    *buf.add(bytes.len()) = 0;
    if !file_part.is_null() {
        let offset = bytes
            .iter()
            .rposition(|b| *b == b'\\' || *b == b'/')
            .map_or(0, |i| i + 1);
        *file_part = buf.add(offset);
    }
    bytes.len() as u32
}
pub unsafe extern "win64" fn get_full_path_name_w(
    name: *const u16,
    size: u32,
    buf: *mut u16,
    file_part: *mut *mut u16,
) -> u32 {
    let wide = match cstring_from_wide(name) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let path = match full_windows_path(&wide.to_string_lossy()) {
        Ok(p) => p,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let text: Vec<u16> = path.encode_utf16().collect();
    let needed = text.len().saturating_add(1);
    if needed > u32::MAX as usize {
        crate::process::set_last_error(87);
        return 0;
    }
    if buf.is_null() || (size as usize) < needed {
        if !file_part.is_null() {
            *file_part = std::ptr::null_mut();
        }
        return needed as u32;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), buf, text.len());
    *buf.add(text.len()) = 0;
    if !file_part.is_null() {
        let offset = text
            .iter()
            .rposition(|c| *c == (b'\\' as u16) || *c == (b'/' as u16))
            .map_or(0, |i| i + 1);
        *file_part = buf.add(offset);
    }
    text.len() as u32
}
/// Copie comme CopyFile : contenu et attributs, et **date de dernière
/// écriture conservée** (documentation de CopyFile). `std::fs::copy` mettait
/// l'heure courante : SteamService comparait les dates de ses deux copies,
/// se croyait périmé à chaque démarrage et relançait sa mise à jour sans
/// fin (#1949).
fn copy_keeping_write_time(
    source: &std::path::Path,
    target: &std::path::Path,
) -> std::io::Result<u64> {
    let n = std::fs::copy(source, target)?;
    if let Ok(modified) = std::fs::metadata(source).and_then(|m| m.modified()) {
        if let Ok(f) = std::fs::File::open(target) {
            let _ = f.set_modified(modified);
        }
    }
    Ok(n)
}
pub unsafe extern "win64" fn copy_file_a(
    src: *const u8,
    dst: *const u8,
    fail_if_exists: i32,
) -> i32 {
    if src.is_null() || dst.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let source = crate::paths::to_linux(&crate::dlls::cstr(src));
    let target = crate::paths::to_linux(&crate::dlls::cstr(dst));
    if fail_if_exists != 0 && target.exists() {
        crate::process::set_last_error(80);
        return 0;
    }
    if let Some(code) = crate::image_lock::refuse_write(&target, "CopyFile") {
        crate::process::set_last_error(code);
        return 0;
    }
    match copy_keeping_write_time(&source, &target) {
        Ok(_) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn copy_file_w(
    src: *const u16,
    dst: *const u16,
    fail_if_exists: i32,
) -> i32 {
    let s = match cstring_from_wide(src) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let d = match cstring_from_wide(dst) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let source = crate::paths::to_linux(&s.to_string_lossy());
    let target = crate::paths::to_linux(&d.to_string_lossy());
    if fail_if_exists != 0 && target.exists() {
        crate::process::set_last_error(80);
        return 0;
    }
    if let Some(code) = crate::image_lock::refuse_write(&target, "CopyFile") {
        crate::process::set_last_error(code);
        return 0;
    }
    match copy_keeping_write_time(&source, &target) {
        Ok(_) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn move_file_a(src: *const u8, dst: *const u8) -> i32 {
    if src.is_null() || dst.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let s = crate::paths::to_linux(&crate::dlls::cstr(src));
    let d = crate::paths::to_linux(&crate::dlls::cstr(dst));
    if d.exists() {
        crate::process::set_last_error(183); // ERROR_ALREADY_EXISTS
        return 0;
    }
    match std::fs::rename(s, d) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn move_file_w(src: *const u16, dst: *const u16) -> i32 {
    let s = match cstring_from_wide(src) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let d = match cstring_from_wide(dst) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let s = crate::paths::to_linux(&s.to_string_lossy());
    let d = crate::paths::to_linux(&d.to_string_lossy());
    if d.exists() {
        crate::process::set_last_error(80);
        return 0;
    }
    match std::fs::rename(s, d) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
/// `RemoveDirectory` sur un lien vers un dossier supprime le lien, jamais
/// le contenu de la cible (comportement Windows pour les liens et jonctions).
fn remove_dir_or_link(p: &std::path::Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.file_type().is_symlink() => {
            if p.is_dir() {
                std::fs::remove_file(p)
            } else {
                Err(std::io::Error::from_raw_os_error(libc::ENOTDIR))
            }
        }
        _ => std::fs::remove_dir(p),
    }
}
pub unsafe extern "win64" fn remove_directory_a(path: *const u8) -> i32 {
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    match remove_dir_or_link(&crate::paths::to_linux(&crate::dlls::cstr(path))) {
        Ok(()) => 1,
        Err(e) => {
            crate::process::set_last_error(io_error_code(&e));
            0
        }
    }
}
pub unsafe extern "win64" fn remove_directory_w(path: *const u16) -> i32 {
    let s = match cstring_from_wide(path) {
        Ok(s) => s,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    let path = s.to_string_lossy();
    match remove_dir_or_link(&crate::paths::to_linux(&path)) {
        Ok(()) => {
            crate::trace!("RemoveDirectory({path}) = 1");
            1
        }
        Err(e) => {
            let code = io_error_code(&e);
            crate::trace!("RemoveDirectory({path}) = 0 (erreur {code})");
            crate::process::set_last_error(code);
            0
        }
    }
}
pub extern "win64" fn sleep(ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}
pub extern "win64" fn tls_alloc() -> u32 {
    let mut slots = lock(TLS_SLOTS.get_or_init(|| Mutex::new(vec![0; TLS_LIMIT])));
    let Some((i, slot)) = slots.iter_mut().enumerate().find(|(_, g)| **g == 0) else {
        crate::process::set_last_error(8);
        return u32::MAX;
    };
    let _ = crate::process::clear_tls_value_all_threads(i as u32);
    let generation = TLS_GENERATION.fetch_add(1, Ordering::Relaxed).max(1);
    *slot = generation;
    crate::process::set_last_error(0);
    i as u32
}
pub extern "win64" fn tls_free(i: u32) -> i32 {
    let mut slots = lock(TLS_SLOTS.get_or_init(|| Mutex::new(vec![0; TLS_LIMIT])));
    let Some(slot) = slots.get_mut(i as usize) else {
        crate::process::set_last_error(87);
        return 0;
    };
    if *slot == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let _ = crate::process::clear_tls_value_all_threads(i);
    *slot = 0;
    crate::process::set_last_error(0);
    1
}
pub unsafe extern "win64" fn tls_set_value(i: u32, v: *mut c_void) -> i32 {
    let slots = lock(TLS_SLOTS.get_or_init(|| Mutex::new(vec![0; TLS_LIMIT])));
    if slots.get(i as usize).copied().unwrap_or(0) == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if !crate::process::set_tls_value(i, v as usize) {
        crate::process::set_last_error(87);
        return 0;
    }
    crate::process::set_last_error(0);
    1
}
pub extern "win64" fn tls_get_value(i: u32) -> *mut c_void {
    let slots = lock(TLS_SLOTS.get_or_init(|| Mutex::new(vec![0; TLS_LIMIT])));
    if slots.get(i as usize).copied().unwrap_or(0) == 0 {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    }
    let value = crate::process::tls_value(i).unwrap_or(0);
    crate::process::set_last_error(0);
    value as *mut c_void
}
pub unsafe extern "win64" fn write_console_w(
    _h: usize,
    buf: *const u16,
    n: u32,
    written: *mut u32,
    _r: usize,
) -> i32 {
    if n != 0 && buf.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let units = if n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(buf, n as usize)
    };
    let x = String::from_utf16_lossy(units);
    let ok = if canon_std(_h) == (-12isize) as usize {
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::io::Write::write_all(&mut std::io::stderr(), x.as_bytes()).is_ok()
    } else {
        let mut out = std::io::stdout();
        std::io::Write::write_all(&mut out, x.as_bytes())
            .and_then(|_| std::io::Write::flush(&mut out))
            .is_ok()
    };
    if !ok {
        return 0;
    }
    if !written.is_null() {
        *written = n;
    }
    1
}
pub extern "win64" fn query_performance_frequency(out: *mut i64) -> i32 {
    unsafe {
        if out.is_null() {
            return 0;
        }
        *out = 1_000_000_000;
    }
    1
}
/// QueryPerformanceCounter : nanosecondes de CLOCK_MONOTONIC, la même
/// horloge que ntdll!RtlQueryPerformanceCounter et NtQueryPerformanceCounter.
/// Sous Windows, le compteur a une origine commune à tous les processus.
/// L'ancienne version comptait depuis le démarrage du processus : les
/// processus de Chromium vivaient alors dans des temps décalés de plusieurs
/// minutes, et le renderer de Discord attendait des échéances de viz
/// placées loin dans le futur (page noire, #1886).
pub extern "win64" fn query_performance_counter(out: *mut i64) -> i32 {
    if out.is_null() {
        return 0;
    }
    unsafe {
        crate::dlls::ntdll::rtl_query_performance_counter(out);
    }
    1
}
/// QueryUnbiasedInterruptTime : temps depuis le démarrage en unités de
/// 100 ns, hors veille (horloge CLOCK_MONOTONIC de Linux, qui s'arrête
/// pendant la suspension). Utilisé par .NET (Flow Launcher, #953).
pub extern "win64" fn query_unbiased_interrupt_time(out: *mut u64) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
        *out = ts.tv_sec as u64 * 10_000_000 + ts.tv_nsec as u64 / 100;
    }
    1
}
/// Fichier où écrire le code de sortie Windows complet (`WINLUX_EXIT_CODE_FILE`,
/// posé par winlux-open, #1205) : le statut de sortie Linux n'en garde que
/// l'octet de poids faible (0x780148E0 affiché « code 224 »).
static EXIT_CODE_FILE: OnceLock<std::ffi::CString> = OnceLock::new();

/// Lu une fois au démarrage, tant qu'il n'y a qu'un fil, puis retiré de
/// l'environnement : un programme lancé par celui-ci n'y écrit pas son code.
pub fn exit_code_file_init() {
    if let Some(path) = std::env::var_os("WINLUX_EXIT_CODE_FILE") {
        // Appelé au début de main, avant tout autre fil.
        std::env::remove_var("WINLUX_EXIT_CODE_FILE");
        use std::os::unix::ffi::OsStrExt;
        if let Ok(path) = std::ffi::CString::new(path.as_bytes()) {
            if !path.as_bytes().is_empty() {
                let _ = EXIT_CODE_FILE.set(path);
            }
        }
    }
}

/// Texte écrit pour un code : « 0x780148E0 ». Sans allocation.
fn exit_code_text(code: u32) -> [u8; 11] {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut text = *b"0x00000000\n";
    for i in 0..8 {
        text[9 - i] = HEX[((code >> (4 * i)) & 0xF) as usize];
    }
    text
}

/// Écrit le code complet ; sûr dans un gestionnaire de signal (open/write).
pub fn record_exit_code(code: u32) {
    let Some(path) = EXIT_CODE_FILE.get() else {
        return;
    };
    let text = exit_code_text(code);
    // SAFETY: chemin terminé par NUL, tampon local ; appels sûrs en signal.
    unsafe {
        let fd = libc::open(
            path.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_CLOEXEC,
            0o600,
        );
        if fd >= 0 {
            libc::write(fd, text.as_ptr().cast(), text.len());
            libc::close(fd);
        }
    }
}

/// Arrêt immédiat du processus courant : vide la sortie puis `_exit`, sans
/// atexit ni destructeurs, comme TerminateProcess sous Windows.
pub fn terminate_self(code: u32) -> ! {
    crate::trace!("TerminateProcess(processus courant) : fin, code {code:#x}");
    // WINLUX_WOW32_HISTORY (#2368) : historique écrit à toute fin.
    crate::wow32::dump_history_at_exit(&format!("TerminateProcess (code {code:#x})"));
    record_exit_code(code);
    crate::dlls::jobs::on_exit();
    crate::dlls::named_objects::on_exit();
    crate::dlls::doomed::on_exit();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = std::io::Write::flush(&mut std::io::stderr());
    crate::dlls::console_input::restore();
    // SAFETY: _exit termine le processus sans revenir.
    unsafe { libc::_exit(code as i32) }
}

pub fn flush_and_exit(code: i32) -> ! {
    // WINLUX_WOW32_HISTORY (#2368) : ExitProcess, fail-fast, _invoke_watson.
    crate::wow32::dump_history_at_exit(&format!("fin du processus (code {:#x})", code as u32));
    // Les jobs KILL_ON_JOB_CLOSE du processus se ferment avec lui (#19).
    crate::dlls::jobs::on_exit();
    crate::dlls::named_objects::on_exit();
    crate::dlls::doomed::on_exit();
    let _ = std::io::Write::flush(&mut std::io::stdout());
    crate::dlls::console_input::restore();
    crate::dlls::handle_broker::forget();
    crate::log::journal_event("session", &format!("fin du programme (code {code})"));
    crate::trace!("ExitProcess : fin, code {:#x}", code as u32);
    record_exit_code(code as u32);
    let _ = std::io::Write::flush(&mut std::io::stderr());
    // ExitProcess ne rend jamais la main au code de l'invité : pas de
    // `exit()` de la glibc, dont les gestionnaires atexit et destructeurs TLS
    // tournaient pendant que les autres threads x86 continuaient. Steam et
    // PyInstaller x86 y plantaient en lisant [0+8] après « fin » (#1440).
    // SAFETY : sorties vidées ci-dessus, _exit termine tout le processus.
    unsafe {
        libc::fflush(std::ptr::null_mut());
        libc::_exit(code)
    }
}

// ---------- Fonctions supplémentaires kernel32 ----------

// --- HeapCreate / HeapDestroy / HeapSize ---
static HEAPS: OnceLock<Mutex<HashMap<usize, ()>>> = OnceLock::new();
pub unsafe extern "win64" fn heap_create(_options: u32, _initial: usize, _max: usize) -> usize {
    let h = next_handle();
    lock(HEAPS.get_or_init(|| Mutex::new(HashMap::new()))).insert(h, ());
    h
}
pub unsafe extern "win64" fn heap_destroy(heap: usize) -> i32 {
    lock(HEAPS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&heap);
    1
}
pub unsafe extern "win64" fn heap_size(_heap: usize, _flags: u32, p: *const c_void) -> usize {
    if p.is_null() {
        return usize::MAX;
    }
    let allocs = lock(HEAP_ALLOCS.get().unwrap());
    allocs.get(&(p as usize)).copied().unwrap_or(usize::MAX)
}
pub unsafe extern "win64" fn heap_validate(_heap: usize, _flags: u32, _p: *const c_void) -> i32 {
    1
}
pub unsafe extern "win64" fn heap_set_information(
    _heap: usize,
    _class: u32,
    _info: *const c_void,
    _size: usize,
) -> i32 {
    1
}

// --- FindFirstFileA/W / FindNextFileA/W / FindClose ---
#[repr(C)]
pub struct Win32FindDataA {
    pub attributes: u32,
    pub creation_time: [u32; 2],
    pub last_access_time: [u32; 2],
    pub last_write_time: [u32; 2],
    pub file_size_high: u32,
    pub file_size_low: u32,
    pub reserved0: u32,
    pub reserved1: u32,
    pub file_name: [u8; 260],
    pub alternate_name: [u8; 14],
}

struct FindState {
    entries: Vec<(String, std::fs::Metadata)>,
    pos: usize,
}
static FIND_HANDLES: OnceLock<Mutex<HashMap<usize, FindState>>> = OnceLock::new();

fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" || pattern == "*.*" {
        return true;
    }
    let pl = pattern.to_ascii_lowercase();
    let nl = name.to_ascii_lowercase();
    let mut pi = pl.as_bytes().iter().peekable();
    let mut ni = nl.as_bytes().iter().peekable();
    fn m(
        p: &mut std::iter::Peekable<std::slice::Iter<u8>>,
        n: &mut std::iter::Peekable<std::slice::Iter<u8>>,
    ) -> bool {
        loop {
            match p.peek() {
                None => return n.peek().is_none(),
                Some(&&b'*') => {
                    p.next();
                    if p.peek().is_none() {
                        return true;
                    }
                    while n.peek().is_some() {
                        let mut p2 = p.clone();
                        let mut n2 = n.clone();
                        if m(&mut p2, &mut n2) {
                            return true;
                        }
                        n.next();
                    }
                    return false;
                }
                Some(&&b'?') => {
                    p.next();
                    if n.next().is_none() {
                        return false;
                    }
                }
                Some(&&c) => {
                    if n.next() != Some(&c) {
                        return false;
                    }
                    p.next();
                }
            }
        }
    }
    m(&mut pi, &mut ni)
}

unsafe fn fill_find_data_a(data: *mut Win32FindDataA, name: &str, meta: &std::fs::Metadata) {
    std::ptr::write_bytes(data, 0, 1);
    let d = &mut *data;
    d.attributes = if meta.is_dir() { 0x10 } else { 0x80 }; // FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_NORMAL
    if name.starts_with('.') {
        d.attributes |= 0x02;
    } // FILE_ATTRIBUTE_HIDDEN
    let size = meta.len();
    d.file_size_low = size as u32;
    d.file_size_high = (size >> 32) as u32;
    // Write the name
    let bytes = crate::dlls::encode_ansi(name);
    let n = bytes.len().min(259);
    d.file_name[..n].copy_from_slice(&bytes[..n]);
}

pub unsafe extern "win64" fn find_first_file_a(
    pattern: *const u8,
    data: *mut Win32FindDataA,
) -> usize {
    if pattern.is_null() || data.is_null() {
        crate::process::set_last_error(87);
        return usize::MAX;
    }
    let pat_str = crate::dlls::cstr(pattern);
    let path = crate::paths::to_linux(&pat_str);
    let (dir, file_pattern) = match path.parent() {
        Some(d) if d.as_os_str().is_empty() => (
            std::path::PathBuf::from("."),
            path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "*".into()),
        ),
        Some(d) => (
            d.to_path_buf(),
            path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "*".into()),
        ),
        None => {
            crate::process::set_last_error(3);
            return usize::MAX;
        }
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        crate::process::set_last_error(3);
        return usize::MAX;
    };
    let mut entries = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if glob_match(&file_pattern, &name) {
            if let Ok(meta) = entry.metadata() {
                entries.push((name, meta));
            }
        }
    }
    if entries.is_empty() {
        crate::process::set_last_error(18);
        return usize::MAX;
    } // ERROR_NO_MORE_FILES
    fill_find_data_a(data, &entries[0].0, &entries[0].1);
    let h = next_handle();
    lock(FIND_HANDLES.get_or_init(|| Mutex::new(HashMap::new())))
        .insert(h, FindState { entries, pos: 1 });
    h
}

pub unsafe extern "win64" fn find_next_file_a(handle: usize, data: *mut Win32FindDataA) -> i32 {
    if data.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut handles = lock(FIND_HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
    let Some(state) = handles.get_mut(&handle) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if state.pos >= state.entries.len() {
        crate::process::set_last_error(18);
        return 0;
    }
    let (name, meta) = &state.entries[state.pos];
    fill_find_data_a(data, name, meta);
    state.pos += 1;
    1
}

pub extern "win64" fn find_close(handle: usize) -> i32 {
    lock(FIND_HANDLES.get_or_init(|| Mutex::new(HashMap::new()))).remove(&handle);
    1
}

// --- SRW Locks (Slim Reader/Writer) ---
// Un SRWLOCK Windows est un seul mot dont on ne touche pas le contenu : l'état
// réel vit dans une table indexée par l'adresse du verrou, partagée avec les
// RtlAcquireSRWLock* de ntdll. L'attente est bloquante (Mutex + Condvar),
// pas un spin : un thread qui attend un verrou tenu longtemps ne consomme pas
// de processeur.
struct SrwInner {
    /// Un écrivain détient le verrou.
    exclusive: bool,
    /// Nombre de lecteurs qui le détiennent.
    shared: u32,
    /// Écrivains en attente : les nouveaux lecteurs patientent pour ne pas
    /// les affamer.
    writers_waiting: u32,
}
pub(crate) struct SrwLockState {
    inner: Mutex<SrwInner>,
    changed: Condvar,
}
static SRW_LOCKS: OnceLock<Mutex<HashMap<usize, Arc<SrwLockState>>>> = OnceLock::new();

pub(crate) fn get_srw(p: usize) -> Arc<SrwLockState> {
    let mut map = lock(SRW_LOCKS.get_or_init(|| Mutex::new(HashMap::new())));
    map.entry(p)
        .or_insert_with(|| {
            Arc::new(SrwLockState {
                inner: Mutex::new(SrwInner {
                    exclusive: false,
                    shared: 0,
                    writers_waiting: 0,
                }),
                changed: Condvar::new(),
            })
        })
        .clone()
}

pub extern "win64" fn initialize_srw_lock(p: usize) {
    if p != 0 {
        unsafe {
            std::ptr::write_bytes(p as *mut u8, 0, crate::wow32::pointer_size());
        }
    }
}
pub extern "win64" fn acquire_srw_lock_exclusive(p: usize) {
    let srw = get_srw(p);
    let mut s = lock(&srw.inner);
    s.writers_waiting += 1;
    while s.exclusive || s.shared != 0 {
        s = srw
            .changed
            .wait(s)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    s.writers_waiting -= 1;
    s.exclusive = true;
}
pub extern "win64" fn release_srw_lock_exclusive(p: usize) {
    let srw = get_srw(p);
    let mut s = lock(&srw.inner);
    s.exclusive = false;
    srw.changed.notify_all();
}
pub extern "win64" fn acquire_srw_lock_shared(p: usize) {
    let srw = get_srw(p);
    let mut s = lock(&srw.inner);
    while s.exclusive || s.writers_waiting != 0 {
        s = srw
            .changed
            .wait(s)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    s.shared += 1;
}
pub extern "win64" fn release_srw_lock_shared(p: usize) {
    let srw = get_srw(p);
    let mut s = lock(&srw.inner);
    s.shared = s.shared.saturating_sub(1);
    if s.shared == 0 {
        srw.changed.notify_all();
    }
}
pub extern "win64" fn try_acquire_srw_lock_exclusive(p: usize) -> u8 {
    let srw = get_srw(p);
    let mut s = lock(&srw.inner);
    if s.exclusive || s.shared != 0 {
        0
    } else {
        s.exclusive = true;
        1
    }
}
pub extern "win64" fn try_acquire_srw_lock_shared(p: usize) -> u8 {
    let srw = get_srw(p);
    let mut s = lock(&srw.inner);
    if s.exclusive {
        0
    } else {
        s.shared += 1;
        1
    }
}

// --- Condition Variables ---
// Même table pour kernel32 et les RtlSleepConditionVariable* de ntdll.
pub(crate) struct CondVarState {
    cv: Condvar,
    mu: Mutex<()>,
}
static COND_VARS: OnceLock<Mutex<HashMap<usize, Arc<CondVarState>>>> = OnceLock::new();

pub(crate) fn get_condvar(p: usize) -> Arc<CondVarState> {
    let mut map = lock(COND_VARS.get_or_init(|| Mutex::new(HashMap::new())));
    map.entry(p)
        .or_insert_with(|| {
            Arc::new(CondVarState {
                cv: Condvar::new(),
                mu: Mutex::new(()),
            })
        })
        .clone()
}

/// Attente sur une variable de condition : `release` relâche le verrou
/// applicatif une fois le mutex interne pris (un réveil émis entre les deux
/// n'est donc jamais perdu), `reacquire` le reprend avant de rendre la main.
/// Renvoie vrai si le délai a expiré.
pub(crate) fn condvar_sleep(
    cv: usize,
    ms: u32,
    release: impl FnOnce(),
    reacquire: impl FnOnce(),
) -> bool {
    let cond = get_condvar(cv);
    let guard = cond.mu.lock().unwrap_or_else(|e| e.into_inner());
    release();
    let timed_out = if ms == u32::MAX {
        drop(
            cond.cv
                .wait(guard)
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        false
    } else {
        let (g, result) = cond
            .cv
            .wait_timeout(guard, std::time::Duration::from_millis(ms as u64))
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(g);
        result.timed_out()
    };
    reacquire();
    timed_out
}

pub extern "win64" fn initialize_condition_variable(p: usize) {
    if p != 0 {
        unsafe {
            std::ptr::write_bytes(p as *mut u8, 0, crate::wow32::pointer_size());
        }
    }
}
pub extern "win64" fn sleep_condition_variable_srw(
    cv: usize,
    srw: usize,
    ms: u32,
    flags: u32,
) -> i32 {
    // CONDITION_VARIABLE_LOCKMODE_SHARED (1) : le SRW est tenu en partagé.
    let shared = flags & 1 != 0;
    let timed_out = condvar_sleep(
        cv,
        ms,
        || {
            if shared {
                release_srw_lock_shared(srw)
            } else {
                release_srw_lock_exclusive(srw)
            }
        },
        || {
            if shared {
                acquire_srw_lock_shared(srw)
            } else {
                acquire_srw_lock_exclusive(srw)
            }
        },
    );
    if timed_out {
        crate::process::set_last_error(1460); // ERROR_TIMEOUT (libc++ le teste)
        0
    } else {
        1
    }
}
pub extern "win64" fn sleep_condition_variable_cs(cv: usize, cs: usize, ms: u32) -> i32 {
    let timed_out = condvar_sleep(
        cv,
        ms,
        || unsafe { leave_critical_section(cs as *mut c_void) },
        || unsafe { enter_critical_section(cs as *mut c_void) },
    );
    if timed_out {
        crate::process::set_last_error(1460); // ERROR_TIMEOUT (libc++ le teste)
        0
    } else {
        1
    }
}
pub extern "win64" fn wake_condition_variable(cv: usize) {
    let cond = get_condvar(cv);
    // Prendre `mu` : un dormeur qui a relâché le verrou applicatif mais n'est
    // pas encore dans `wait` ne doit pas rater le réveil.
    let _g = cond.mu.lock().unwrap_or_else(|e| e.into_inner());
    cond.cv.notify_one();
}
pub extern "win64" fn wake_all_condition_variable(cv: usize) {
    let cond = get_condvar(cv);
    let _g = cond.mu.lock().unwrap_or_else(|e| e.into_inner());
    cond.cv.notify_all();
}

// --- InitOnce ---
// Le mot INIT_ONCE porte l'état dans ses deux bits bas (0 : jamais commencé,
// 1 : initialisation en cours, 2 : terminée) et le contexte dans les bits
// hauts, comme sous Windows. Les prétendants attendent sous un verrou global
// (Mutex + Condvar) que le premier termine : une seule initialisation, sans
// course entre deux threads qui lisent « pas encore fait » en même temps.
const INIT_ONCE_CHECK_ONLY: u32 = 1;
const INIT_ONCE_ASYNC: u32 = 2;
const INIT_ONCE_INIT_FAILED: u32 = 4;
static INIT_ONCE_GATE: OnceLock<(Mutex<()>, Condvar)> = OnceLock::new();
fn init_once_gate() -> &'static (Mutex<()>, Condvar) {
    INIT_ONCE_GATE.get_or_init(|| (Mutex::new(()), Condvar::new()))
}
pub extern "win64" fn init_once_begin_initialize(
    init_once: usize,
    flags: u32,
    pending: *mut i32,
    context: *mut usize,
) -> i32 {
    if init_once == 0 || pending.is_null() || flags & !(INIT_ONCE_CHECK_ONLY | INIT_ONCE_ASYNC) != 0
    {
        crate::process::set_last_error(87);
        return 0;
    }
    let (mu, cv) = init_once_gate();
    let mut guard = mu.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let value = unsafe { crate::wow32::read_ptr(init_once) };
        match value & 3 {
            2 => {
                unsafe {
                    *pending = 0;
                    if !context.is_null() {
                        *context = value & !3;
                    }
                }
                return 1;
            }
            0 if flags & INIT_ONCE_CHECK_ONLY != 0 => {
                crate::process::set_last_error(1001); // ERROR_GEN_FAILURE : rien n'a encore été fait
                unsafe { *pending = 1 };
                return 0;
            }
            0 => {
                // Mode asynchrone : plusieurs initialisations concurrentes sont
                // permises, seule la première InitOnceComplete est retenue.
                if flags & INIT_ONCE_ASYNC == 0 {
                    unsafe { crate::wow32::write_ptr(init_once, 1) };
                }
                unsafe { *pending = 1 };
                return 1;
            }
            _ => {
                if flags & INIT_ONCE_CHECK_ONLY != 0 {
                    crate::process::set_last_error(1001);
                    unsafe { *pending = 1 };
                    return 0;
                }
                if flags & INIT_ONCE_ASYNC != 0 {
                    unsafe { *pending = 1 };
                    return 1;
                }
                guard = cv.wait(guard).unwrap_or_else(|e| e.into_inner());
            }
        }
    }
}
pub extern "win64" fn init_once_complete(init_once: usize, flags: u32, context: usize) -> i32 {
    if init_once == 0 || context & 3 != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let (mu, cv) = init_once_gate();
    let _guard = mu.lock().unwrap_or_else(|e| e.into_inner());
    let value = unsafe { crate::wow32::read_ptr(init_once) };
    if value & 3 == 2 {
        // Déjà terminée par un autre initialiseur (mode asynchrone).
        crate::process::set_last_error(1001);
        return 0;
    }
    let new = if flags & INIT_ONCE_INIT_FAILED != 0 {
        0
    } else {
        context | 2
    };
    unsafe { crate::wow32::write_ptr(init_once, new) };
    cv.notify_all();
    1
}
pub extern "win64" fn init_once_initialize(init_once: usize) {
    if init_once != 0 {
        unsafe {
            std::ptr::write_bytes(init_once as *mut u8, 0, crate::wow32::pointer_size());
        }
    }
}
/// `InitOnceExecuteOnce` : exécute `callback(init_once, parameter, context)`
/// une seule fois ; les autres threads attendent qu'il ait terminé.
pub unsafe extern "win64" fn init_once_execute_once(
    init_once: usize,
    callback: usize,
    parameter: usize,
    context: *mut usize,
) -> i32 {
    if init_once == 0 || callback == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut pending = 0;
    if init_once_begin_initialize(init_once, 0, &mut pending, context) == 0 {
        return 0;
    }
    if pending == 0 {
        return 1;
    }
    let f: extern "win64" fn(usize, usize, *mut usize) -> i32 = std::mem::transmute(callback);
    let mut produced = 0usize;
    let ok = f(init_once, parameter, &mut produced) != 0;
    if !context.is_null() {
        *context = produced;
    }
    init_once_complete(
        init_once,
        if ok { 0 } else { INIT_ONCE_INIT_FAILED },
        if ok { produced & !3 } else { 0 },
    );
    i32::from(ok)
}
// --- Fiber Local Storage (FLS) ---
static FLS_CALLBACKS: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();

pub extern "win64" fn fls_alloc(callback: usize) -> u32 {
    // FlsGetValue/FlsSetValue lisent les mêmes cases TEB que TlsGetValue :
    // l'index doit donc venir de la même table que TlsAlloc, sinon le premier
    // FlsAlloc (CRT) et le premier TlsAlloc (application) partagent la case 0.
    let slot = tls_alloc();
    if slot != u32::MAX && callback != 0 {
        lock(FLS_CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))).insert(slot, callback);
    }
    slot
}
pub extern "win64" fn fls_free(slot: u32) -> i32 {
    lock(FLS_CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&slot);
    tls_free(slot)
}
pub extern "win64" fn fls_get_value(slot: u32) -> *mut c_void {
    crate::process::tls_value(slot).unwrap_or(0) as *mut c_void
}
pub unsafe extern "win64" fn fls_set_value(slot: u32, data: *mut c_void) -> i32 {
    crate::process::set_tls_value(slot, data as usize);
    1
}

// --- Interlocked SList ---
// Même implémentation que RtlInterlockedPushEntrySList & co. (ntdll) : une
// seule table et un verrou global, l'en-tête de 16 octets ne pouvant pas être
// mis à jour atomiquement sans CMPXCHG16B.
pub extern "win64" fn initialize_slist_head(head: usize) {
    unsafe { super::ntdll::rtl_initialize_slist_head(head as *mut u8) }
}
pub unsafe extern "win64" fn interlocked_push_entry_slist(head: usize, entry: usize) -> usize {
    super::ntdll::rtl_interlocked_push_entry_slist(head as *mut u8, entry as *mut u8) as usize
}
pub unsafe extern "win64" fn interlocked_pop_entry_slist(head: usize) -> usize {
    super::ntdll::rtl_interlocked_pop_entry_slist(head as *mut u8) as usize
}
pub unsafe extern "win64" fn interlocked_flush_slist(head: usize) -> usize {
    super::ntdll::rtl_interlocked_flush_slist(head as *mut u8) as usize
}
pub unsafe extern "win64" fn query_depth_slist(head: usize) -> u16 {
    super::ntdll::rtl_query_depth_slist(head as *const u8)
}
// --- Encode/Decode Pointer ---
pub extern "win64" fn encode_pointer(ptr: usize) -> usize {
    pointer_xor(ptr)
}
pub extern "win64" fn decode_pointer(ptr: usize) -> usize {
    pointer_xor(ptr)
}
/// Programme x86 : valeur codée sur 32 bits, comme un pointeur de ce
/// programme ; sinon la passerelle la signale « au-dessus de 4 Go » (#1642).
fn pointer_xor(ptr: usize) -> usize {
    let v = ptr ^ 0xDEAD_BEEF_CAFE_BABE;
    if crate::wow32::active() {
        v as u32 as usize
    } else {
        v
    }
}

// --- Helpers (hostname, date/time without chrono/nix) ---
pub(crate) fn get_hostname_string() -> String {
    let mut buf = [0u8; 256];
    if unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0 {
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..len]).into_owned()
    } else {
        "WINLUX".into()
    }
}

// --- GetLogicalDrives, GetComputerName, GetUserName ---
pub extern "win64" fn get_logical_drives() -> u32 {
    // C:, Z: et les lecteurs dynamiques (images montées, lecteurs réseau).
    0x04 | 0x0200_0000 | crate::mounts::drive_mask()
}

pub unsafe extern "win64" fn get_computer_name_a(buf: *mut u8, size: *mut u32) -> i32 {
    if buf.is_null() || size.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let name = get_hostname_string();
    let upper = name.to_ascii_uppercase();
    let n = upper.len();
    if (*size as usize) <= n {
        *size = (n + 1) as u32;
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(upper.as_bytes().as_ptr(), buf, n);
    *buf.add(n) = 0;
    *size = n as u32;
    1
}

pub unsafe extern "win64" fn get_computer_name_w(buf: *mut u16, size: *mut u32) -> i32 {
    if buf.is_null() || size.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let name = get_hostname_string();
    let upper = name.to_ascii_uppercase();
    let n = upper.encode_utf16().count() as u32;
    if *size <= n {
        *size = n + 1;
        crate::process::set_last_error(111);
        return 0;
    }
    crate::dlls::put_w(&upper, buf, *size);
    *size = n;
    1
}

// --- GetLocaleInfoA/W ---

// --- GetDateFormatA/W, GetTimeFormatA/W (basic) ---

// --- GetNumberOfProcessors ---
pub extern "win64" fn get_active_processor_count(_group: u16) -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

// --- Waitable Timer (stubs) ---
// --- Minuteurs (CreateWaitableTimer) ---
// Un minuteur est un événement de la table des handles (donc attendable par
// WaitForSingleObject/WaitForMultipleObjects) ; SetWaitableTimer programme un
// thread qui le signale à l'échéance puis à chaque période. La génération
// invalide les programmations précédentes (CancelWaitableTimer, nouveau
// SetWaitableTimer, CloseHandle).
static WAITABLE_TIMERS: OnceLock<Mutex<HashMap<usize, Arc<AtomicUsize>>>> = OnceLock::new();
fn waitable_timers() -> &'static Mutex<HashMap<usize, Arc<AtomicUsize>>> {
    WAITABLE_TIMERS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn remove_waitable_timer(h: usize) -> bool {
    match lock(waitable_timers()).remove(&h) {
        Some(generation) => {
            generation.fetch_add(1, Ordering::AcqRel);
            true
        }
        None => false,
    }
}
pub unsafe extern "win64" fn create_waitable_timer_a(
    _attr: usize,
    manual: i32,
    name: *const u8,
) -> usize {
    let obj = SyncHandle::Event(Arc::new(EventObject::new(manual != 0, false)));
    let h = create_sync(obj, named_event(name));
    if h != 0 {
        lock(waitable_timers()).insert(h, Arc::new(AtomicUsize::new(0)));
    }
    h
}
pub unsafe extern "win64" fn create_waitable_timer_w(
    attr: usize,
    manual: i32,
    name: *const u16,
) -> usize {
    let n = sync_name_w(name);
    create_waitable_timer_a(
        attr,
        manual,
        n.as_ref().map_or(std::ptr::null(), |c| c.as_ptr().cast()),
    )
}
/// OpenWaitableTimerW : minuteur nommé de ce processus. Le handle ouvert
/// partage le compteur d'armement du minuteur d'origine.
pub unsafe extern "win64" fn open_waitable_timer_w(
    access: u32,
    inherit: i32,
    name: *const u16,
) -> usize {
    let Some(n) = sync_name_w(name) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let h = open_event_a(access, inherit, n.as_ptr().cast());
    if h == 0 {
        return 0;
    }
    let Some(SyncHandle::Event(obj)) = lookup_sync(h) else {
        return h;
    };
    let timers: Vec<(usize, Arc<AtomicUsize>)> = lock(waitable_timers())
        .iter()
        .map(|(&t, g)| (t, g.clone()))
        .collect();
    let generation = timers.into_iter().find_map(|(t, g)| match lookup_sync(t) {
        Some(SyncHandle::Event(o)) if Arc::ptr_eq(&o, &obj) => Some(g),
        _ => None,
    });
    match generation {
        Some(g) => {
            lock(waitable_timers()).insert(h, g);
            h
        }
        None => {
            // Le nom désigne un événement, pas un minuteur.
            close_handle(h);
            crate::process::set_last_error(6);
            0
        }
    }
}
pub unsafe extern "win64" fn create_waitable_timer_ex_w(
    attr: usize,
    name: *const u16,
    flags: u32,
    _access: u32,
) -> usize {
    // CREATE_WAITABLE_TIMER_MANUAL_RESET = 1.
    create_waitable_timer_w(attr, i32::from(flags & 1 != 0), name)
}
pub unsafe extern "win64" fn set_waitable_timer(
    h: usize,
    due: *const i64,
    period: i32,
    _routine: usize,
    _arg: usize,
    _resume: i32,
) -> i32 {
    let Some(generation) = lock(waitable_timers()).get(&h).cloned() else {
        crate::process::set_last_error(6);
        return 0;
    };
    let Some(SyncHandle::Event(event)) = lookup_sync(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if due.is_null() || period < 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let due = due.read_unaligned();
    // Échéance : relative (négative, en unités de 100 ns) ou absolue (FILETIME).
    let first = if due <= 0 {
        std::time::Duration::from_nanos((-due) as u64 * 100)
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| (d.as_nanos() / 100) as i64)
            + crate::intl::tz::FILETIME_UNIX_EPOCH;
        std::time::Duration::from_nanos((due - now).max(0) as u64 * 100)
    };
    let my_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
    event.store(&mut lock(&event.signaled), false);
    let period = std::time::Duration::from_millis(period as u64);
    let spawned = std::thread::Builder::new()
        .name("winlux-waitable-timer".into())
        .spawn(move || {
            let mut wait = first;
            loop {
                std::thread::sleep(wait);
                if generation.load(Ordering::Acquire) != my_generation {
                    return;
                }
                {
                    let _gate = sync_gate();
                    event.store(&mut lock(&event.signaled), true);
                    if let Some(w) = event.shared.get() {
                        w.wake();
                    }
                    event.changed.notify_all();
                }
                if period.is_zero() {
                    return;
                }
                wait = period;
            }
        });
    if spawned.is_err() {
        crate::process::set_last_error(8);
        return 0;
    }
    1
}
pub extern "win64" fn cancel_waitable_timer(h: usize) -> i32 {
    match lock(waitable_timers()).get(&h) {
        Some(generation) => {
            generation.fetch_add(1, Ordering::AcqRel);
            1
        }
        None => {
            crate::process::set_last_error(6);
            0
        }
    }
}
// --- GetFileInformationByHandle ---
#[repr(C)]
pub struct ByHandleFileInformation {
    pub attributes: u32,
    pub creation_time: [u32; 2],
    pub last_access_time: [u32; 2],
    pub last_write_time: [u32; 2],
    pub volume_serial: u32,
    pub file_size_high: u32,
    pub file_size_low: u32,
    pub number_of_links: u32,
    pub file_index_high: u32,
    pub file_index_low: u32,
}

/// GetFileInformationByHandle, lu par fstat sur le descripteur du handle :
/// fichier, dossier, mais aussi handle standard redirigé vers un fichier,
/// /dev/null ou un terminal (Python 3.12 x86, #1436). Seuls les
/// fichiers ouverts par CreateFile étaient reconnus.
pub unsafe extern "win64" fn get_file_information_by_handle(
    handle: usize,
    info: *mut ByHandleFileInformation,
) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let fd = handle_to_fd(handle);
    if fd < 0 {
        crate::process::set_last_error(6);
        return 0;
    }
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) != 0 {
        crate::process::set_last_error(6);
        return 0;
    }
    let is_dir = st.st_mode & libc::S_IFMT == libc::S_IFDIR;
    let mut attrs = if is_dir { 0x10 } else { 0 };
    if st.st_mode & libc::S_IFMT == libc::S_IFREG && st.st_mode & 0o222 == 0 {
        attrs |= 0x1;
    }
    if attrs == 0 {
        attrs = 0x80;
    }
    let ft = |t: i64| -> [u32; 2] { [t as u32, (t >> 32) as u32] };
    let modified = unix_to_filetime(st.st_mtime, st.st_mtime_nsec);
    let changed = unix_to_filetime(st.st_ctime, st.st_ctime_nsec);
    let size = if st.st_mode & libc::S_IFMT == libc::S_IFREG {
        st.st_size as u64
    } else {
        0
    };
    std::ptr::write_bytes(info, 0, 1);
    let i = &mut *info;
    i.attributes = attrs;
    i.creation_time = ft(modified.min(changed));
    i.last_access_time = ft(unix_to_filetime(st.st_atime, st.st_atime_nsec));
    i.last_write_time = ft(modified);
    i.volume_serial = st.st_dev as u32;
    i.file_size_high = (size >> 32) as u32;
    i.file_size_low = size as u32;
    i.number_of_links = (st.st_nlink as u32).max(1);
    i.file_index_high = (st.st_ino >> 32) as u32;
    i.file_index_low = st.st_ino as u32;
    1
}

/// Date Unix (secondes, nanosecondes) → FILETIME.
fn unix_to_filetime(secs: i64, nsecs: i64) -> i64 {
    secs * 10_000_000 + nsecs / 100 + 116_444_736_000_000_000
}

/// GetFileInformationByHandleEx : FileBasicInfo (0), FileStandardInfo (1),
/// FileNameInfo (2), FileAttributeTagInfo (9) et FileIdInfo (18), lus par
/// fstat sur le descripteur du handle (fichier ou dossier).
pub unsafe extern "win64" fn get_file_information_by_handle_ex(
    handle: usize,
    class: u32,
    out: *mut u8,
    size: u32,
) -> i32 {
    let fd = handle_to_fd(handle);
    if fd < 0 {
        crate::process::set_last_error(6);
        return 0;
    }
    // Listes de dossier (FileIdBothDirectory(Restart)Info,
    // FileFullDirectory(Restart)Info) : celles de NtQueryDirectoryFile.
    // `remove_dir_all` de la bibliothèque Rust parcourt ainsi le dossier à
    // effacer ; sans elles le dossier restait (rm-* de Discord, #1885).
    if let Some((nt, restart)) = match class {
        10 => Some((37, false)),
        11 => Some((37, true)),
        14 => Some((2, false)),
        15 => Some((2, true)),
        _ => None,
    } {
        let mut iosb = [0u64; 2];
        let st = crate::dlls::ntdll_file::NtQueryDirectoryFile(
            handle,
            0,
            0,
            0,
            iosb.as_mut_ptr() as *mut std::ffi::c_void,
            out,
            size,
            nt,
            0,
            std::ptr::null(),
            u8::from(restart),
        );
        if st != 0 {
            crate::process::set_last_error(match st {
                0x8000_0006 => 18,  // STATUS_NO_MORE_FILES → ERROR_NO_MORE_FILES
                0x8000_0005 => 234, // STATUS_BUFFER_OVERFLOW → ERROR_MORE_DATA
                _ => crate::dlls::ntdll::rtl_nt_status_to_dos_error(st as i32),
            });
            return 0;
        }
        return 1;
    }
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat(fd, &mut st) != 0 {
        crate::process::set_last_error(6);
        return 0;
    }
    let is_dir = st.st_mode & libc::S_IFMT == libc::S_IFDIR;
    let path = std::fs::read_link(format!("/proc/self/fd/{fd}")).ok();
    let hidden = path
        .as_ref()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n.to_string_lossy().starts_with('.'));
    let mut attrs = if is_dir { 0x10 } else { 0 };
    if st.st_mode & 0o222 == 0 {
        attrs |= 0x1;
    }
    if hidden {
        attrs |= 0x2;
    }
    if attrs == 0 {
        attrs = 0x80;
    }
    let need = |n: u32| -> bool {
        if out.is_null() || size < n {
            crate::process::set_last_error(24); // ERROR_BAD_LENGTH
            false
        } else {
            std::ptr::write_bytes(out, 0, n as usize);
            true
        }
    };
    let put64 = |o: usize, v: i64| (out.add(o) as *mut i64).write_unaligned(v);
    let put32 = |o: usize, v: u32| (out.add(o) as *mut u32).write_unaligned(v);
    match class {
        0 => {
            if !need(40) {
                return 0;
            }
            let modified = unix_to_filetime(st.st_mtime, st.st_mtime_nsec);
            put64(
                0,
                modified.min(unix_to_filetime(st.st_ctime, st.st_ctime_nsec)),
            );
            put64(8, unix_to_filetime(st.st_atime, st.st_atime_nsec));
            put64(16, modified);
            put64(24, unix_to_filetime(st.st_ctime, st.st_ctime_nsec));
            put32(32, attrs);
        }
        1 => {
            if !need(24) {
                return 0;
            }
            let len = if is_dir { 0 } else { st.st_size };
            put64(0, st.st_blocks * 512);
            put64(8, len);
            put32(16, st.st_nlink as u32);
            *out.add(21) = u8::from(is_dir);
        }
        2 => {
            let wide: Vec<u16> = if let Some(name) = crate::dlls::named_pipes::file_name(handle) {
                name
            } else {
                let Some(p) = path else {
                    crate::process::set_last_error(6);
                    return 0;
                };
                let win = crate::paths::to_windows(&p);
                let rest = win.get(2..).unwrap_or(&win);
                rest.encode_utf16().collect()
            };
            if out.is_null() || size < 4 {
                crate::process::set_last_error(24);
                return 0;
            }
            put32(0, (wide.len() * 2) as u32);
            let room = (size as usize - 4) / 2;
            let n = wide.len().min(room);
            std::ptr::copy_nonoverlapping(wide.as_ptr(), out.add(4) as *mut u16, n);
            if n < wide.len() {
                crate::process::set_last_error(234); // ERROR_MORE_DATA
                return 0;
            }
        }
        9 => {
            if !need(8) {
                return 0;
            }
            put32(0, attrs);
        }
        18 => {
            if !need(24) {
                return 0;
            }
            put64(0, st.st_dev as i64);
            put64(8, st.st_ino as i64);
        }
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    }
    1
}

// --- Surveillance de dossiers : ReadDirectoryChangesW et FindFirstChangeNotification ---
//
// Moteur inotify dans `dirwatch.rs`. Un dossier ouvert par CreateFile garde
// son surveillant jusqu'à CloseHandle : les changements survenus entre deux
// appels sont rendus au suivant, comme sous Windows.

type SharedWatcher = Arc<Mutex<crate::dlls::dirwatch::Watcher>>;
static DIR_WATCHERS: OnceLock<Mutex<HashMap<usize, SharedWatcher>>> = OnceLock::new();
fn dir_watchers() -> &'static Mutex<HashMap<usize, SharedWatcher>> {
    DIR_WATCHERS.get_or_init(|| Mutex::new(HashMap::new()))
}
const ERROR_NOTIFY_ENUM_DIR: u32 = 1022;
const ERROR_OPERATION_ABORTED: u32 = 995;

/// Chemin Linux d'un handle de fichier (lien /proc/self/fd).
fn handle_path(h: usize) -> Option<std::path::PathBuf> {
    let fd = fd_of_handle(h)?;
    std::fs::read_link(format!("/proc/self/fd/{fd}")).ok()
}

fn watcher_for(h: usize, subtree: bool, filter: u32) -> Result<SharedWatcher, u32> {
    if let Some(found) = lock(dir_watchers()).get(&h).cloned() {
        lock(&found).set_filter(filter);
        return Ok(found);
    }
    let Some(path) = handle_path(h) else {
        return Err(6);
    };
    if !path.is_dir() {
        return Err(1); // ERROR_INVALID_FUNCTION : pas un dossier
    }
    let watcher = crate::dlls::dirwatch::Watcher::new(&path, subtree, filter).map_err(|e| {
        if e.raw_os_error() == Some(libc::ENOSPC) || e.raw_os_error() == Some(libc::EMFILE) {
            8 // plus de surveillances inotify disponibles
        } else {
            io_error_code(&e)
        }
    })?;
    crate::trace!("ReadDirectoryChangesW : surveillance de {path:?} (sous-arbre {subtree}, filtre {filter:#x})");
    let shared = Arc::new(Mutex::new(watcher));
    lock(dir_watchers()).insert(h, shared.clone());
    Ok(shared)
}

/// Oublie le surveillant d'un handle fermé (appelé par CloseHandle).
pub(crate) fn forget_dir_watcher(h: usize) {
    lock(dir_watchers()).remove(&h);
}

// File d'APC par thread : routines d'achèvement (ReadDirectoryChangesW,
// ReadFileEx…) et QueueUserAPC, exécutées pendant une attente « alertable ».
#[derive(Clone, Copy)]
enum Apc {
    /// PAPCFUNC(param)
    User(usize, usize),
    /// LPOVERLAPPED_COMPLETION_ROUTINE(erreur, octets, overlapped)
    Completion(usize, u32, u32, usize),
}
static APC_QUEUES: OnceLock<(Mutex<HashMap<ThreadId, VecDeque<Apc>>>, Condvar)> = OnceLock::new();
fn apc_queues() -> &'static (Mutex<HashMap<ThreadId, VecDeque<Apc>>>, Condvar) {
    APC_QUEUES.get_or_init(|| (Mutex::new(HashMap::new()), Condvar::new()))
}
fn queue_apc(thread: ThreadId, apc: Apc) {
    let (queues, changed) = apc_queues();
    lock(queues).entry(thread).or_default().push_back(apc);
    changed.notify_all();
}
fn apc_pending() -> bool {
    let me = std::thread::current().id();
    lock(&apc_queues().0)
        .get(&me)
        .is_some_and(|q| !q.is_empty())
}
/// Exécute les APC du thread courant ; vrai si au moins une a tourné.
pub(crate) fn run_apcs() -> bool {
    let me = std::thread::current().id();
    let mut ran = false;
    loop {
        let next = lock(&apc_queues().0)
            .get_mut(&me)
            .and_then(VecDeque::pop_front);
        let Some(apc) = next else {
            return ran;
        };
        ran = true;
        unsafe {
            match apc {
                Apc::User(func, param) => {
                    let f: extern "win64" fn(usize) = std::mem::transmute(func);
                    f(param);
                }
                Apc::Completion(func, error, bytes, overlapped) => {
                    let f: extern "win64" fn(u32, u32, usize) = std::mem::transmute(func);
                    f(error, bytes, overlapped);
                }
            }
        }
    }
}
/// Attente alertable : APC en attente → elles tournent et on rend
/// WAIT_IO_COMPLETION (0xC0) ; sinon `wait(tranche)` jusqu'au délai.
fn alertable_wait(ms: u32, mut wait: impl FnMut(u32) -> u32) -> u32 {
    const WAIT_IO_COMPLETION: u32 = 0xC0;
    const WAIT_TIMEOUT: u32 = 258;
    if run_apcs() {
        return WAIT_IO_COMPLETION;
    }
    let start = std::time::Instant::now();
    loop {
        let elapsed = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
        let left = if ms == u32::MAX {
            u32::MAX
        } else {
            ms.saturating_sub(elapsed)
        };
        let result = wait(left.min(15));
        if result != WAIT_TIMEOUT {
            return result;
        }
        if run_apcs() {
            return WAIT_IO_COMPLETION;
        }
        if left <= 15 {
            return WAIT_TIMEOUT;
        }
    }
}

/// `QueueUserAPC` : pour le thread courant (pseudo-handle GetCurrentThread).
pub unsafe extern "win64" fn queue_user_apc(func: usize, thread: usize, param: usize) -> u32 {
    if func == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if thread == get_current_thread() {
        queue_apc(std::thread::current().id(), Apc::User(func, param));
        return 1;
    }
    // Autre thread : l'APC tourne à sa prochaine attente alertable.
    if let Some(SyncHandle::Thread(t)) = lookup_sync(thread) {
        if let Some(id) = t.rust_id.get() {
            if !lock(&t.state).finished {
                queue_apc(*id, Apc::User(func, param));
                return 1;
            }
        }
    }
    crate::process::set_last_error(6);
    0
}

/// Copie un lot de changements dans le tampon de l'appelant.
unsafe fn deliver_changes(
    watcher: &SharedWatcher,
    buf: *mut c_void,
    len: u32,
    extended: bool,
) -> (u32, u32) {
    match lock(watcher).take(len as usize, extended) {
        crate::dlls::dirwatch::Batch::Records(data) => {
            std::ptr::copy_nonoverlapping(data.as_ptr(), buf as *mut u8, data.len());
            (data.len() as u32, 0)
        }
        crate::dlls::dirwatch::Batch::Overflow => (0, ERROR_NOTIFY_ENUM_DIR),
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn read_directory_changes(
    h: usize,
    buf: *mut c_void,
    len: u32,
    subtree: i32,
    filter: u32,
    bytes: *mut u32,
    ov: usize,
    routine: usize,
    extended: bool,
) -> i32 {
    if buf.is_null() || filter == 0 || filter & !crate::dlls::dirwatch::FILTER_ALL != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let watcher = match watcher_for(h, subtree != 0, filter) {
        Ok(w) => w,
        Err(e) => {
            crate::process::set_last_error(e);
            return 0;
        }
    };
    if ov == 0 {
        // Synchrone : attend le premier changement.
        loop {
            let ready = lock(&watcher).wait(200);
            if ready {
                break;
            }
            if !lock(dir_watchers()).contains_key(&h) {
                crate::process::set_last_error(ERROR_OPERATION_ABORTED);
                return 0;
            }
        }
        let (n, error) = deliver_changes(&watcher, buf, len, extended);
        if !bytes.is_null() {
            *bytes = n;
        }
        crate::process::set_last_error(error);
        return 1;
    }
    if !ov.is_multiple_of(std::mem::align_of::<Overlapped>()) {
        crate::process::set_last_error(87);
        return 0;
    }
    // Asynchrone : un fil attend, puis remplit le tampon et signale
    // l'événement, le port de complétion ou la routine d'achèvement.
    let operation = Arc::new(IoOperation {
        file_handle: h,
        issuing_thread: std::thread::current().id(),
        state: Mutex::new(IoResult {
            done: false,
            bytes: 0,
            error: 0,
        }),
        changed: Condvar::new(),
        cancelled: std::sync::atomic::AtomicBool::new(false),
    });
    {
        let mut ops = lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new())));
        if ops.get(&ov).is_some_and(|op| !lock(&op.state).done) {
            crate::process::set_last_error(997);
            return 0;
        }
        ops.insert(ov, operation.clone());
    }
    let overlapped = ov as *mut Overlapped;
    (*overlapped).internal = 0x103; // STATUS_PENDING
    (*overlapped).internal_high = 0;
    // Les handles de Winlux se suivent : le bit bas n'est pas un drapeau ici.
    let event = (*overlapped).event;
    if event != 0 {
        let _ = reset_event(event);
    }
    let completion = lock(IOCP_ASSOCIATIONS.get_or_init(|| Mutex::new(HashMap::new())))
        .get(&h)
        .cloned();
    let issuer = std::thread::current().id();
    let (buf, op) = (buf as usize, operation.clone());
    let spawned = std::thread::Builder::new()
        .name("winlux-dirwatch".into())
        .spawn(move || {
            let error = loop {
                if op.cancelled.load(Ordering::Acquire) || !lock(dir_watchers()).contains_key(&h) {
                    break ERROR_OPERATION_ABORTED;
                }
                if lock(&watcher).wait(100) {
                    break 0;
                }
            };
            let (count, error) = if error == 0 {
                // Débordement : succès avec 0 octet, l'appli relit le dossier.
                let (count, _) =
                    unsafe { deliver_changes(&watcher, buf as *mut c_void, len, extended) };
                (count, 0)
            } else {
                (0, error)
            };
            unsafe {
                (*(ov as *mut Overlapped)).internal = match error {
                    0 if count == 0 => 0x10C, // STATUS_NOTIFY_ENUM_DIR
                    0 => 0,
                    _ => 0xC000_0120, // STATUS_CANCELLED
                };
                (*(ov as *mut Overlapped)).internal_high = count as usize;
            }
            {
                let mut state = lock(&op.state);
                state.bytes = count;
                state.error = error;
                state.done = true;
                op.changed.notify_all();
            }
            if event != 0 {
                let _ = set_event(event);
            }
            if let Some((port, key)) = completion {
                post_iocp(
                    &port,
                    CompletionPacket {
                        bytes: count,
                        key,
                        overlapped: ov,
                        error,
                    },
                );
            }
            if routine != 0 {
                queue_apc(issuer, Apc::Completion(routine, error, count, ov));
            }
        });
    if spawned.is_err() {
        lock(IO_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&ov);
        crate::process::set_last_error(8);
        return 0;
    }
    if !bytes.is_null() {
        *bytes = 0;
    }
    crate::process::set_last_error(997);
    1
}

/// `ReadDirectoryChangesW` : synchrone, OVERLAPPED + événement, port de
/// complétion ou routine d'achèvement (APC du thread appelant).
pub unsafe extern "win64" fn read_directory_changes_w(
    h: usize,
    buf: *mut c_void,
    len: u32,
    subtree: i32,
    filter: u32,
    bytes: *mut u32,
    ov: usize,
    routine: usize,
) -> i32 {
    read_directory_changes(h, buf, len, subtree, filter, bytes, ov, routine, false)
}

/// `ReadDirectoryChangesExW` : classe 1 (FILE_NOTIFY_INFORMATION) ou
/// 2 (FILE_NOTIFY_EXTENDED_INFORMATION : dates, taille, attributs, id).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn read_directory_changes_ex_w(
    h: usize,
    buf: *mut c_void,
    len: u32,
    subtree: i32,
    filter: u32,
    bytes: *mut u32,
    ov: usize,
    routine: usize,
    class: u32,
) -> i32 {
    match class {
        1 => read_directory_changes(h, buf, len, subtree, filter, bytes, ov, routine, false),
        2 => read_directory_changes(h, buf, len, subtree, filter, bytes, ov, routine, true),
        _ => {
            crate::process::set_last_error(87);
            0
        }
    }
}

// FindFirstChangeNotification : un événement Windows qu'un fil signale à
// chaque changement ; FindNextChangeNotification le réarme.
static CHANGE_NOTIFICATIONS: OnceLock<Mutex<HashMap<usize, Arc<std::sync::atomic::AtomicBool>>>> =
    OnceLock::new();
fn change_notifications() -> &'static Mutex<HashMap<usize, Arc<std::sync::atomic::AtomicBool>>> {
    CHANGE_NOTIFICATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Code Windows d'un échec de FindFirstChangeNotification : dossier refusé
/// à l'application (ERROR_ACCESS_DENIED), absent (ERROR_PATH_NOT_FOUND),
/// ou fichier qui n'est pas un dossier (ERROR_DIRECTORY). 7-Zip affiche ce
/// code quand il ne peut pas ouvrir un dossier (#483).
fn change_notification_error(linux: &std::path::Path, e: &std::io::Error) -> u32 {
    if crate::permissions::is_denied_path(linux) {
        5
    } else if e.raw_os_error() == Some(libc::ENOTDIR) {
        if linux.exists() {
            267
        } else {
            3
        }
    } else {
        io_error_code(e)
    }
}

fn find_first_change_notification(path: &str, subtree: bool, filter: u32) -> usize {
    if filter == 0 || filter & !crate::dlls::dirwatch::FILTER_ALL != 0 {
        crate::process::set_last_error(87);
        return usize::MAX;
    }
    let linux = crate::paths::to_linux(path);
    let mut watcher = match crate::dlls::dirwatch::Watcher::new(&linux, subtree, filter) {
        Ok(w) => w,
        Err(e) => {
            crate::process::set_last_error(change_notification_error(&linux, &e));
            return usize::MAX;
        }
    };
    let event = add_sync(
        SyncHandle::Event(Arc::new(EventObject::new(true, false))),
        None,
    );
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    lock(change_notifications()).insert(event, stop.clone());
    let spawned = std::thread::Builder::new()
        .name("winlux-change-notify".into())
        .spawn(move || {
            while !stop.load(Ordering::Acquire) {
                if watcher.wait(150) {
                    watcher.clear();
                    let _ = set_event(event);
                }
            }
        });
    if spawned.is_err() {
        lock(change_notifications()).remove(&event);
        remove_sync(event);
        crate::process::set_last_error(8);
        return usize::MAX;
    }
    event
}

pub unsafe extern "win64" fn find_first_change_notification_w(
    path: *const u16,
    subtree: i32,
    filter: u32,
) -> usize {
    if path.is_null() {
        crate::process::set_last_error(87);
        return usize::MAX;
    }
    find_first_change_notification(&crate::dlls::wstr(path), subtree != 0, filter)
}

pub unsafe extern "win64" fn find_first_change_notification_a(
    path: *const u8,
    subtree: i32,
    filter: u32,
) -> usize {
    if path.is_null() {
        crate::process::set_last_error(87);
        return usize::MAX;
    }
    find_first_change_notification(&crate::dlls::cstr(path), subtree != 0, filter)
}

pub extern "win64" fn find_next_change_notification(h: usize) -> i32 {
    if !lock(change_notifications()).contains_key(&h) {
        crate::process::set_last_error(6);
        return 0;
    }
    reset_event(h)
}

pub extern "win64" fn find_close_change_notification(h: usize) -> i32 {
    let Some(stop) = lock(change_notifications()).remove(&h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    stop.store(true, Ordering::Release);
    remove_sync(h);
    1
}

// --- CreateNamedPipeA / ConnectNamedPipe (stubs) ---
pub unsafe extern "win64" fn create_named_pipe_a(
    name: *const u8,
    open_mode: u32,
    pipe_mode: u32,
    max: u32,
    _out_size: u32,
    _in_size: u32,
    _timeout: u32,
    _sa: usize,
) -> usize {
    if name.is_null() {
        crate::process::set_last_error(123);
        return usize::MAX;
    }
    let h = crate::dlls::named_pipes::create(&crate::dlls::cstr(name), open_mode, pipe_mode, max);
    crate::dlls::inherit::from_security_attributes(h, _sa);
    h
}
pub extern "win64" fn connect_named_pipe(h: usize, ov: usize) -> i32 {
    crate::dlls::named_pipes::connect(h, ov)
}
pub extern "win64" fn disconnect_named_pipe(h: usize) -> i32 {
    crate::dlls::named_pipes::disconnect(h)
}

// --- CreateFileMappingA/W, MapViewOfFile, UnmapViewOfFile (stubs) ---
pub unsafe extern "win64" fn create_file_mapping_a(
    _file: usize,
    _sa: usize,
    _protect: u32,
    _high: u32,
    _low: u32,
    _name: *const u8,
) -> usize {
    next_handle()
}
pub unsafe extern "win64" fn create_file_mapping_w(
    _file: usize,
    _sa: usize,
    _protect: u32,
    _high: u32,
    _low: u32,
    _name: *const u16,
) -> usize {
    next_handle()
}

static MAPPINGS: OnceLock<Mutex<HashMap<usize, Vec<u8>>>> = OnceLock::new();

pub unsafe extern "win64" fn map_view_of_file(
    _mapping: usize,
    _access: u32,
    _high: u32,
    _low: u32,
    bytes: usize,
) -> *mut c_void {
    let size = if bytes == 0 { 4096 } else { bytes };
    let mem = vec![0u8; size];
    let ptr = mem.as_ptr() as *mut c_void;
    lock(MAPPINGS.get_or_init(|| Mutex::new(HashMap::new()))).insert(ptr as usize, mem);
    ptr
}
pub unsafe extern "win64" fn unmap_view_of_file(base: *const c_void) -> i32 {
    lock(MAPPINGS.get_or_init(|| Mutex::new(HashMap::new()))).remove(&(base as usize));
    1
}

// --- GetUserNameA/W ---
pub unsafe extern "win64" fn get_user_name_a(buf: *mut u8, size: *mut u32) -> i32 {
    if buf.is_null() || size.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let name = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let n = name.len();
    if (*size as usize) <= n {
        *size = (n + 1) as u32;
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(name.as_bytes().as_ptr(), buf, n);
    *buf.add(n) = 0;
    *size = (n + 1) as u32;
    1
}

// --- GetPrivateProfileStringA/W, WritePrivateProfileStringA ---
pub unsafe extern "win64" fn get_private_profile_string_a(
    _section: *const u8,
    _key: *const u8,
    default: *const u8,
    buf: *mut u8,
    size: u32,
    _file: *const u8,
) -> u32 {
    // Stub: always return the default
    if buf.is_null() || size == 0 {
        return 0;
    }
    if default.is_null() {
        *buf = 0;
        return 0;
    }
    crate::dlls::put_a(&crate::dlls::cstr(default), buf, size)
}
pub unsafe extern "win64" fn write_private_profile_string_a(
    _section: *const u8,
    _key: *const u8,
    _value: *const u8,
    _file: *const u8,
) -> i32 {
    1
}

// --- GetLongPathNameA/W, GetShortPathNameA/W ---
pub unsafe extern "win64" fn get_long_path_name_a(short: *const u8, buf: *mut u8, cap: u32) -> u32 {
    if short.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let s = crate::dlls::cstr(short);
    crate::dlls::put_a(&s, buf, cap)
}
pub unsafe extern "win64" fn get_long_path_name_w(
    short: *const u16,
    buf: *mut u16,
    cap: u32,
) -> u32 {
    if short.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let s = crate::dlls::wstr(short);
    crate::dlls::put_w(&s, buf, cap)
}
pub unsafe extern "win64" fn get_short_path_name_a(long: *const u8, buf: *mut u8, cap: u32) -> u32 {
    get_long_path_name_a(long, buf, cap)
}
pub unsafe extern "win64" fn get_short_path_name_w(
    long: *const u16,
    buf: *mut u16,
    cap: u32,
) -> u32 {
    get_long_path_name_w(long, buf, cap)
}

// --- CompareStringA/W, CompareStringOrdinal ---
pub unsafe extern "win64" fn compare_string_a(
    _locale: u32,
    flags: u32,
    s1: *const u8,
    n1: i32,
    s2: *const u8,
    n2: i32,
) -> i32 {
    let a = if n1 < 0 {
        crate::dlls::cstr(s1)
    } else {
        crate::dlls::decode_ansi(std::slice::from_raw_parts(s1, n1 as usize))
    };
    let b = if n2 < 0 {
        crate::dlls::cstr(s2)
    } else {
        crate::dlls::decode_ansi(std::slice::from_raw_parts(s2, n2 as usize))
    };
    let (a, b): (Vec<u16>, Vec<u16>) = (a.encode_utf16().collect(), b.encode_utf16().collect());
    compare_result(&a, &b, flags)
}

/// CompareStringA/W appliquent `dwCmpFlags` comme CompareStringEx (#2255) :
/// sans NORM_IGNORECASE, deux chaînes qui ne diffèrent que par la casse ne
/// sont pas égales.
fn compare_result(a: &[u16], b: &[u16], flags: u32) -> i32 {
    match super::nls::compare(a, b, flags) {
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => 2,
        std::cmp::Ordering::Greater => 3,
    }
}

pub unsafe extern "win64" fn compare_string_w(
    _locale: u32,
    flags: u32,
    s1: *const u16,
    n1: i32,
    s2: *const u16,
    n2: i32,
) -> i32 {
    if s1.is_null() || s2.is_null() {
        set_last_error(87);
        return 0;
    }
    let len = |p: *const u16, n: i32| {
        if n < 0 {
            crate::printf::wide_len(p)
        } else {
            n as usize
        }
    };
    let a = std::slice::from_raw_parts(s1, len(s1, n1));
    let b = std::slice::from_raw_parts(s2, len(s2, n2));
    compare_result(a, b, flags)
}
pub unsafe extern "win64" fn compare_string_ordinal(
    s1: *const u16,
    n1: i32,
    s2: *const u16,
    n2: i32,
    ignore_case: i32,
) -> i32 {
    let a = if n1 < 0 {
        crate::dlls::wstr(s1)
    } else {
        String::from_utf16_lossy(std::slice::from_raw_parts(s1, n1 as usize))
    };
    let b = if n2 < 0 {
        crate::dlls::wstr(s2)
    } else {
        String::from_utf16_lossy(std::slice::from_raw_parts(s2, n2 as usize))
    };
    let (aa, bb) = if ignore_case != 0 {
        (a.to_lowercase(), b.to_lowercase())
    } else {
        (a, b)
    };
    match aa.cmp(&bb) {
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => 2,
        std::cmp::Ordering::Greater => 3,
    }
}

// --- GetStringTypeA/W ---
pub unsafe extern "win64" fn get_string_type_w(
    _info_type: u32,
    src: *const u16,
    count: i32,
    char_type: *mut u16,
) -> i32 {
    if src.is_null() || char_type.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let n = if count < 0 {
        crate::printf::wide_len(src)
    } else {
        count as usize
    };
    for i in 0..n {
        let ch = char::from_u32(*src.add(i) as u32).unwrap_or('\0');
        let mut t: u16 = 0;
        if ch.is_uppercase() {
            t |= 0x01;
        } // C1_UPPER
        if ch.is_lowercase() {
            t |= 0x02;
        } // C1_LOWER
        if ch.is_ascii_digit() {
            t |= 0x04;
        } // C1_DIGIT
        if ch.is_whitespace() {
            t |= 0x08;
        } // C1_SPACE
        if ch.is_ascii_punctuation() {
            t |= 0x10;
        } // C1_PUNCT
        if ch.is_control() {
            t |= 0x20;
        } // C1_CNTRL
        if ch.is_ascii_hexdigit() {
            t |= 0x80;
        } // C1_XDIGIT
        if ch.is_alphabetic() {
            t |= 0x100;
        } // C1_ALPHA
        *char_type.add(i) = t;
    }
    1
}

// --- LCMapStringA/W (basic) ---
pub unsafe extern "win64" fn lc_map_string_w(
    _locale: u32,
    flags: u32,
    src: *const u16,
    src_count: i32,
    dst: *mut u16,
    dst_count: i32,
) -> i32 {
    if src.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let n = if src_count < 0 {
        crate::printf::wide_len(src) + 1
    } else {
        src_count as usize
    };
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(src, n));
    let transformed = if flags & 0x100 != 0 {
        s.to_lowercase()
    }
    // LCMAP_LOWERCASE
    else if flags & 0x200 != 0 {
        s.to_uppercase()
    }
    // LCMAP_UPPERCASE
    else {
        s
    };
    let w: Vec<u16> = transformed.encode_utf16().collect();
    if dst.is_null() || dst_count == 0 {
        return w.len() as i32;
    }
    let copy = w.len().min(dst_count as usize);
    std::ptr::copy_nonoverlapping(w.as_ptr(), dst, copy);
    copy as i32
}

// ---------- API supplémentaires pour Electron / Chromium ----------

pub unsafe extern "win64" fn get_product_info(
    _major: u32,
    _minor: u32,
    _sp_major: u32,
    _sp_minor: u32,
    product_type: *mut u32,
) -> i32 {
    if !product_type.is_null() {
        *product_type = 0x30;
    } // PRODUCT_PROFESSIONAL
    1
}

pub unsafe extern "win64" fn get_system_time(st: *mut u16) {
    if st.is_null() {
        return;
    }
    let now = chrono_utc_fields();
    let fields: [u16; 8] = [now.0, now.1, now.2, now.3, now.4, now.5, now.6, now.7];
    std::ptr::copy_nonoverlapping(fields.as_ptr(), st, 8);
}

fn chrono_utc_fields() -> (u16, u16, u16, u16, u16, u16, u16, u16) {
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    unsafe {
        libc::gettimeofday(&mut tv, std::ptr::null_mut());
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::gmtime_r(&tv.tv_sec, &mut tm);
    }
    (
        (tm.tm_year + 1900) as u16,
        (tm.tm_mon + 1) as u16,
        tm.tm_wday as u16,
        tm.tm_mday as u16,
        tm.tm_hour as u16,
        tm.tm_min as u16,
        tm.tm_sec as u16,
        (tv.tv_usec / 1000) as u16,
    )
}

pub unsafe extern "win64" fn get_process_times(
    _proc: usize,
    creation: *mut u64,
    exit: *mut u64,
    kernel: *mut u64,
    user: *mut u64,
) -> i32 {
    process_or_thread_times(libc::RUSAGE_SELF, creation, exit, kernel, user)
}

/// Temps processeur réels (getrusage) en unités de 100 ns. GetProcessTimes
/// rendait le temps écoulé depuis le premier GetTickCount comme temps
/// « utilisateur » : faux dès que GetTickCount compte depuis le démarrage.
unsafe fn process_or_thread_times(
    who: i32,
    creation: *mut u64,
    exit: *mut u64,
    kernel: *mut u64,
    user: *mut u64,
) -> i32 {
    let mut usage: libc::rusage = core::mem::zeroed();
    if libc::getrusage(who, &mut usage) != 0 {
        usage = core::mem::zeroed();
    }
    let ticks = |t: libc::timeval| t.tv_sec as u64 * 10_000_000 + t.tv_usec as u64 * 10;
    if !creation.is_null() {
        *creation = 132_000_000_000_000_000;
    }
    if !exit.is_null() {
        *exit = 0;
    }
    if !kernel.is_null() {
        *kernel = ticks(usage.ru_stime);
    }
    if !user.is_null() {
        *user = ticks(usage.ru_utime);
    }
    1
}

pub unsafe extern "win64" fn get_thread_times(
    _thread: usize,
    creation: *mut u64,
    exit: *mut u64,
    kernel: *mut u64,
    user: *mut u64,
) -> i32 {
    process_or_thread_times(libc::RUSAGE_THREAD, creation, exit, kernel, user)
}

pub unsafe extern "win64" fn set_handle_information(h: usize, mask: u32, flags: u32) -> i32 {
    crate::dlls::inherit::set_information(h, mask, flags);
    1
}
pub unsafe extern "win64" fn get_handle_information(h: usize, flags: *mut u32) -> i32 {
    if !flags.is_null() {
        *flags = crate::dlls::inherit::information(h);
    }
    1
}
/// Processus désigné par un handle, vu de DuplicateHandle : nous-mêmes, ou
/// un autre processus (PID Linux, et s'il est encore suspendu).
enum DupSide {
    Me,
    Other(i32, bool),
    Invalid,
}
fn dup_side(process: usize) -> DupSide {
    // 0 : toléré comme le processus courant (anciens appelants internes).
    if process == 0 || is_current_process_handle(process) {
        return DupSide::Me;
    }
    match lookup_sync(process) {
        Some(SyncHandle::Process(p)) => {
            let s = lock(&p.state);
            match s.child.as_ref().map(|c| c.id() as i32).or(s.foreign) {
                Some(pid) => DupSide::Other(pid, s.suspended),
                None => DupSide::Invalid,
            }
        }
        _ => DupSide::Invalid,
    }
}

/// DuplicateHandle : copie d'un handle d'un processus dans un autre (#236).
/// Entre deux processus distincts, la copie passe par leur guichet
/// (`handle_broker`) ; Chromium et Electron s'en servent pour donner
/// mémoire partagée, événements et fichiers à leurs enfants.
pub unsafe extern "win64" fn duplicate_handle(
    src_process: usize,
    src_handle: usize,
    dst_process: usize,
    dst_handle: *mut usize,
    access: u32,
    inherit: i32,
    options: u32,
) -> i32 {
    let src = dup_side(src_process);
    // Sans destination, seul DUPLICATE_CLOSE_SOURCE a un sens : fermer le
    // handle chez la source.
    let dst = if dst_process == 0 && options & 1 != 0 {
        None
    } else {
        Some(dup_side(dst_process))
    };
    let fail = |code: u32| {
        if !dst_handle.is_null() {
            *dst_handle = 0;
        }
        crate::process::set_last_error(code);
        0
    };
    let done = |h: usize| {
        if !dst_handle.is_null() {
            *dst_handle = h;
        }
        1
    };
    use crate::dlls::handle_broker as broker;
    match (src, dst) {
        (DupSide::Invalid, _) | (_, Some(DupSide::Invalid)) => fail(6),
        (DupSide::Me, Some(DupSide::Me)) => {
            duplicate_local(src_handle, dst_handle, access, inherit, options)
        }
        (DupSide::Me, None) => {
            close_handle(src_handle);
            1
        }
        (DupSide::Other(pid, _), None) => match broker::close_remote(pid, src_handle) {
            Ok(()) => 1,
            Err(code) => fail(code),
        },
        (DupSide::Me, Some(DupSide::Other(pid, suspended))) => {
            // Copie locale aux droits demandés, envoyée puis fermée.
            let mut copy = 0usize;
            if duplicate_local(src_handle, &mut copy, access, 0, options & !1) == 0 {
                let code = crate::process::last_error();
                if options & 1 != 0 {
                    close_handle(src_handle);
                }
                return fail(code);
            }
            let sent = broker::put(pid, copy, inherit != 0, suspended);
            if copy != src_handle {
                close_handle(copy);
            }
            if options & 1 != 0 {
                close_handle(src_handle);
            }
            crate::trace!("DuplicateHandle({src_handle:#x}) vers le processus {pid} : {sent:?}");
            match sent {
                Ok(h) => done(h),
                Err(code) => fail(code),
            }
        }
        (DupSide::Other(pid, _), Some(DupSide::Me)) => {
            let got = broker::get(pid, src_handle, access, options, inherit != 0);
            crate::trace!("DuplicateHandle({src_handle:#x}) depuis le processus {pid} : {got:?}");
            match got {
                Ok(h) => done(h),
                Err(code) => fail(code),
            }
        }
        (DupSide::Other(from, _), Some(DupSide::Other(to, suspended))) => {
            let r = if from == to {
                // Copie à l'intérieur d'un même autre processus.
                broker::forward(from, src_handle, to, access, options, inherit != 0, false)
            } else {
                broker::forward(
                    from,
                    src_handle,
                    to,
                    access,
                    options,
                    inherit != 0,
                    suspended,
                )
            };
            match r {
                Ok(h) => done(h),
                Err(code) => fail(code),
            }
        }
    }
}

/// DuplicateHandle à l'intérieur du processus courant.
unsafe fn duplicate_local(
    src_handle: usize,
    dst_handle: *mut usize,
    access: u32,
    inherit: i32,
    options: u32,
) -> i32 {
    // Mapping (mémoire partagée de Chromium) : même mémoire, nouveau handle,
    // avec des droits jamais plus larges que ceux de l'original.
    if let Some(r) = crate::dlls::mmap::duplicate_mapping(src_handle, access, options) {
        return match r {
            Ok(h) => {
                if options & 1 != 0 {
                    close_handle(src_handle);
                }
                crate::dlls::inherit::set(h, inherit != 0);
                if !dst_handle.is_null() {
                    *dst_handle = h;
                }
                1
            }
            Err(code) => {
                if !dst_handle.is_null() {
                    *dst_handle = 0;
                }
                // DUPLICATE_CLOSE_SOURCE ferme l'original même en cas d'échec.
                if options & 1 != 0 {
                    close_handle(src_handle);
                }
                crate::process::set_last_error(code);
                0
            }
        };
    }
    // Un fichier ou un tube reçoit un vrai double (son propre fd) : fermer
    // l'original ne ferme pas la copie, comme sous Windows.
    let std_fd = match canon_std(src_handle) as isize {
        -10 => Some(0),
        -11 => Some(1),
        -12 => Some(2),
        _ => None,
    };
    // Pseudo-handles (processus et thread courants) : Windows rend un vrai
    // handle vers le même objet ; ici il reste utilisable tel quel.
    let pseudo = src_handle == get_current_process() || src_handle == get_current_thread();
    let copy = if src_handle == get_current_process() {
        // Vrai handle vers soi-même, désigné par le PID : Chromium refuse le
        // pseudo-handle (-1, égal à INVALID_HANDLE_VALUE) dans un ScopedHandle,
        // et crashpad passe la copie héritable à son gestionnaire.
        let h = next_handle();
        insert_sync_at(h, foreign_process(std::process::id() as i32));
        Some(h)
    } else if src_handle == get_current_thread() {
        // Vrai handle vers le thread courant : coreclr le garde pour
        // suspendre ce thread depuis le ramasse-miettes ; le pseudo-handle
        // y désignerait le thread du ramasse-miettes lui-même.
        Some(add_sync(SyncHandle::Thread(current_thread_object()), None))
    } else if pseudo {
        Some(src_handle)
    } else if let Some(fd) = std_fd.filter(|&fd| libc::isatty(fd) == 1) {
        // La console (terminal) reste la console : la copie est le handle
        // standard réel, que les fonctions Console reconnaissent.
        Some([STD_INPUT_REAL, STD_OUTPUT_REAL, STD_ERROR_REAL][fd as usize])
    } else if let Some(fd) = std_fd {
        let new_fd = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3);
        (new_fd >= 0).then(|| insert_file(std::os::unix::io::FromRawFd::from_raw_fd(new_fd)))
    } else if let Some(h) = duplicate_handle_impl(src_handle) {
        Some(h)
    } else if src_handle == crate::dlls::security::TOKEN_HANDLE {
        // Jeton unique du compte (voir security.rs) : la copie est lui-même.
        Some(src_handle)
    } else if let Some(h) = crate::dlls::electron_win32::duplicate_user_object(src_handle) {
        // Station de fenêtres ou bureau (#2443).
        Some(h)
    } else {
        // Objet de synchronisation (événement, mutex, sémaphore, thread,
        // processus, port de complétion) : nouveau numéro de handle vers le
        // même objet partagé par comptage de références (Arc). Fermer la
        // copie ne ferme pas l'original ; un objet nommé compte un
        // utilisateur de plus.
        lookup_sync(src_handle).map(add_sync_reference)
    };
    let Some(result) = copy else {
        if !dst_handle.is_null() {
            *dst_handle = 0;
        }
        crate::process::set_last_error(6);
        return 0;
    };
    if options & 1 != 0 && std_fd.is_none() && !pseudo {
        close_handle(src_handle);
    }
    if result != src_handle || !pseudo {
        crate::dlls::inherit::set(result, inherit != 0);
    }
    if !dst_handle.is_null() {
        *dst_handle = result;
    }
    1
}

/// Nouveau handle vers un objet de synchronisation existant (DuplicateHandle).
fn add_sync_reference(obj: SyncHandle) -> usize {
    {
        let mut names = lock(SYNC_NAMES.get_or_init(|| Mutex::new(HashMap::new())));
        for (named, count) in names.values_mut() {
            if same_sync(&obj, named) {
                *count += 1;
                break;
            }
        }
    }
    let h = next_handle();
    lock(SYNCS.get_or_init(|| Mutex::new(HashMap::new()))).insert(h, obj);
    h
}

/// Nom large d'objet de synchronisation converti pour les variantes A
/// (None pour un objet anonyme).
unsafe fn sync_name_w(name: *const u16) -> Option<std::ffi::CString> {
    if name.is_null() {
        None
    } else {
        std::ffi::CString::new(crate::dlls::wstr(name)).ok()
    }
}
pub unsafe extern "win64" fn create_event_w(
    _sa: usize,
    manual: i32,
    initial: i32,
    name: *const u16,
) -> usize {
    let n = sync_name_w(name);
    create_event_a(
        _sa,
        manual,
        initial,
        n.as_ref().map_or(std::ptr::null(), |c| c.as_ptr().cast()),
    )
}
pub unsafe extern "win64" fn create_mutex_w(_sa: usize, owner: i32, name: *const u16) -> usize {
    let n = sync_name_w(name);
    create_mutex_a(
        _sa,
        owner,
        n.as_ref().map_or(std::ptr::null(), |c| c.as_ptr().cast()),
    )
}

pub extern "win64" fn get_process_id(process: usize) -> u32 {
    process_os_pid(process).map_or(0, |pid| pid as u32)
}

pub unsafe extern "win64" fn get_module_handle_ex_w(
    flags: u32,
    module: *const u16,
    handle: *mut usize,
) -> i32 {
    if handle.is_null() {
        return 0;
    }
    if flags & 4 != 0 {
        // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS : `module` est une adresse,
        // jamais un nom. Sans module à cette adresse : ERROR_MOD_NOT_FOUND.
        let address = module as usize;
        if let Some(m) = crate::loader::find_module_by_addr(address) {
            module_handle_ex_ref(flags, m.base);
            *handle = m.base;
            return 1;
        }
        *handle = 0;
        crate::process::set_last_error(126);
        return 0;
    }
    let name = if module.is_null() {
        String::new()
    } else {
        crate::dlls::wstr(module)
    };
    if name.is_empty() {
        if let Some(m) = crate::loader::main_module() {
            *handle = m.base;
            return 1;
        }
    }
    if let Some(h) = crate::dlls::module_handle(&name) {
        module_handle_ex_ref(flags, h);
        *handle = h;
        return 1;
    }
    *handle = 0;
    crate::process::set_last_error(126);
    0
}

/// Référence prise par GetModuleHandleEx sur une DLL réelle : une de plus
/// (à rendre par FreeLibrary), aucune avec UNCHANGED_REFCOUNT (2), ou
/// l'épinglage définitif avec PIN (1).
fn module_handle_ex_ref(flags: u32, base: usize) {
    if flags & 1 != 0 {
        crate::loader::pin(base);
    } else if flags & 2 == 0 {
        crate::loader::increment_ref(base);
    }
}

/// Variantes « Ex » (Vista) : drapeaux au lieu de booléens, droits d'accès ignorés.
pub unsafe extern "win64" fn create_event_ex_w(
    sa: usize,
    name: *const u16,
    flags: u32,
    _access: u32,
) -> usize {
    create_event_w(
        sa,
        i32::from(flags & 1 != 0),
        i32::from(flags & 2 != 0),
        name,
    )
}
pub unsafe extern "win64" fn create_mutex_ex_w(
    sa: usize,
    name: *const u16,
    flags: u32,
    _access: u32,
) -> usize {
    create_mutex_w(sa, i32::from(flags & 1 != 0), name)
}
pub unsafe extern "win64" fn create_semaphore_ex_w(
    sa: usize,
    initial: i32,
    maximum: i32,
    name: *const u16,
    _flags: u32,
    _access: u32,
) -> usize {
    create_semaphore_w(sa, initial, maximum, name)
}
/// AreFileApisANSI : les API de fichiers « A » utilisent la page ANSI.
pub extern "win64" fn are_file_apis_ansi() -> i32 {
    1
}
pub unsafe extern "win64" fn create_semaphore_w(
    _sa: usize,
    initial: i32,
    maximum: i32,
    _name: *const u16,
) -> usize {
    if initial < 0 || maximum <= 0 || initial > maximum {
        crate::process::set_last_error(87);
        return 0;
    }
    let obj = SyncHandle::Semaphore(Arc::new(SemaphoreObject::new(initial, maximum)));
    let name = (!_name.is_null())
        .then(|| crate::dlls::wstr(_name))
        .filter(|n| !n.is_empty());
    create_sync(obj, name)
}
pub unsafe extern "win64" fn create_semaphore_a(
    sa: usize,
    initial: i32,
    maximum: i32,
    name: *const u8,
) -> usize {
    let wide: Option<Vec<u16>> =
        named_event(name).map(|n| n.encode_utf16().chain(Some(0)).collect());
    create_semaphore_w(
        sa,
        initial,
        maximum,
        wide.as_ref().map_or(std::ptr::null(), |w| w.as_ptr()),
    )
}
/// ReleaseSemaphore : ajoute `count` au compteur sans dépasser le maximum
/// (ERROR_TOO_MANY_POSTS sinon), renvoie l'ancienne valeur dans `prev`.
pub unsafe extern "win64" fn release_semaphore(h: usize, count: i32, prev: *mut i32) -> i32 {
    if let Some(n) = traced_name(h) {
        crate::trace!("ReleaseSemaphore({n}, 0x{h:x}, {count})");
    }
    let Some(SyncHandle::Semaphore(sem)) = lookup_sync(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if count <= 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if let Some(w) = sem.shared.get() {
        let c = w.slot(0);
        let max = w.slot(1).load(Ordering::Acquire);
        let mut v = c.load(Ordering::Acquire);
        loop {
            if v.checked_add(count as u32).is_none_or(|n| n > max) {
                crate::process::set_last_error(298);
                return 0;
            }
            match c.compare_exchange(v, v + count as u32, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(now) => v = now,
            }
        }
        if !prev.is_null() {
            *prev = v as i32;
        }
        w.wake();
        return 1;
    }
    let gate = sync_gate();
    let mut state = lock(&sem.count);
    if sem.shared.get().is_some() {
        drop(state);
        drop(gate);
        return release_semaphore(h, count, prev);
    }
    if state.0.checked_add(count).is_none_or(|n| n > state.1) {
        crate::process::set_last_error(298);
        return 0;
    }
    if !prev.is_null() {
        *prev = state.0;
    }
    state.0 += count;
    drop(state);
    for _ in 0..count {
        sem.changed.notify_one();
    }
    1
}

pub unsafe extern "win64" fn sleep_ex(ms: u32, alertable: i32) -> u32 {
    if alertable == 0 {
        sleep(ms);
        return 0;
    }
    // Réveillé par une APC (routine d'achèvement, QueueUserAPC).
    match alertable_wait(ms, |slice| {
        let (queues, changed) = apc_queues();
        let me = std::thread::current().id();
        let guard = lock(queues);
        if guard.get(&me).is_some_and(|q| !q.is_empty()) {
            return 0;
        }
        let _ = changed.wait_timeout(guard, std::time::Duration::from_millis(slice as u64));
        if apc_pending() {
            0
        } else {
            258
        }
    }) {
        0xC0 => 0xC0,
        0 if run_apcs() => 0xC0,
        _ => 0,
    }
}

pub unsafe extern "win64" fn wait_for_single_object_ex(h: usize, ms: u32, alertable: i32) -> u32 {
    if alertable == 0 {
        return wait_for_single_object(h, ms);
    }
    alertable_wait(ms, |slice| wait_for_single_object(h, slice))
}

pub unsafe extern "win64" fn signal_object_and_wait(
    to_signal: usize,
    to_wait: usize,
    ms: u32,
    _alertable: i32,
) -> u32 {
    set_event(to_signal);
    wait_for_single_object(to_wait, ms)
}

pub unsafe extern "win64" fn create_named_pipe_w(
    name: *const u16,
    open_mode: u32,
    pipe_mode: u32,
    max_instances: u32,
    out_size: u32,
    in_size: u32,
    timeout: u32,
    sa: usize,
) -> usize {
    let n = crate::dlls::wstr(name);
    let a = std::ffi::CString::new(n.as_bytes()).unwrap_or_default();
    create_named_pipe_a(
        a.as_ptr() as *const u8,
        open_mode,
        pipe_mode,
        max_instances,
        out_size,
        in_size,
        timeout,
        sa,
    )
}

pub unsafe extern "win64" fn set_named_pipe_handle_state(
    _h: usize,
    _mode: *const u32,
    _max_collect: *const u32,
    _timeout: *const u32,
) -> i32 {
    1
}

/// `PeekNamedPipe` : octets en attente dans un tube (sans les consommer).
/// Un tube vide dont l'écrivain est fermé répond ERROR_BROKEN_PIPE.
pub unsafe extern "win64" fn peek_named_pipe(
    h: usize,
    _buf: *mut u8,
    _size: u32,
    read: *mut u32,
    total: *mut u32,
    left: *mut u32,
) -> i32 {
    let Some(fd) = fd_of_handle(h) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let mut avail: libc::c_int = 0;
    if libc::ioctl(fd, libc::FIONREAD, &mut avail) != 0 {
        crate::process::set_last_error(1);
        return 0;
    }
    if avail == 0 {
        let mut p = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        if libc::poll(&mut p, 1, 0) == 1 && p.revents & libc::POLLHUP != 0 {
            crate::process::set_last_error(109);
            return 0;
        }
    }
    if !read.is_null() {
        *read = 0;
    }
    if !total.is_null() {
        *total = avail.max(0) as u32;
    }
    if !left.is_null() {
        *left = 0;
    }
    1
}

pub unsafe extern "win64" fn get_overlapped_result_ex(
    h: usize,
    ov: *mut Overlapped,
    transferred: *mut u32,
    ms: u32,
    _alertable: i32,
) -> i32 {
    get_overlapped_result(h, ov, transferred, if ms > 0 { 1 } else { 0 })
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "GetStdHandle"=>fn get_std_handle,"SetStdHandle"=>fn set_std_handle, "WriteFile"=>fn write_file, "ReadFile"=>fn read_file,"GetOverlappedResult"=>fn get_overlapped_result,"GetOverlappedResultEx"=>fn get_overlapped_result_ex,"CancelIo"=>fn cancel_io,"CancelIoEx"=>fn cancel_io_ex,"CreateIoCompletionPort"=>fn create_io_completion_port,"GetQueuedCompletionStatus"=>fn get_queued_completion_status,"GetQueuedCompletionStatusEx"=>fn get_queued_completion_status_ex,"PostQueuedCompletionStatus"=>fn post_queued_completion_status,"CreateFileA"=>fn create_file_a, "GetFileSize"=>fn get_file_size, "SetFilePointer"=>fn set_file_pointer, "FlushFileBuffers"=>fn flush_file_buffers, "ExitProcess"=>fn exit_process,
     "CreateEventA"=>fn create_event_a,"SetEvent"=>fn super::fil_trace::set_event,"ResetEvent"=>fn super::fil_trace::reset_event,"CreateMutexA"=>fn create_mutex_a,"ReleaseMutex"=>fn super::fil_trace::release_mutex,"WaitForSingleObject"=>fn super::fil_trace::wait_for_single_object,"WaitForMultipleObjects"=>fn super::fil_trace::wait_for_multiple_objects,"GetExitCodeThread"=>fn super::fil_trace::get_exit_code_thread,"ResumeThread"=>fn super::fil_trace::resume_thread,
     "InitializeCriticalSection"=>fn initialize_critical_section,"InitializeCriticalSectionAndSpinCount"=>fn initialize_critical_section_and_spin_count,"InitializeCriticalSectionEx"=>fn initialize_critical_section_ex,"EnterCriticalSection"=>fn enter_critical_section,"TryEnterCriticalSection"=>fn try_enter_critical_section,"LeaveCriticalSection"=>fn leave_critical_section,"DeleteCriticalSection"=>fn delete_critical_section,
     "GetCommandLineA"=>fn get_command_line_a,"GetCommandLineW"=>fn get_command_line_w,"GetModuleHandleA"=>fn get_module_handle_a,"GetModuleHandleW"=>fn get_module_handle_w,"GetProcAddress"=>fn get_proc_address,"LoadLibraryA"=>fn load_library_a,"LoadLibraryW"=>fn load_library_w,"FreeLibrary"=>fn free_library,"CreateProcessA"=>fn create_process_a,"CreatePipe"=>fn create_pipe,"CreateProcessW"=>fn create_process_w,"OpenProcess"=>fn open_process,"TerminateProcess"=>fn terminate_process,"GetExitCodeProcess"=>fn get_exit_code_process,"GetCurrentProcess"=>fn get_current_process,"GetCurrentThread"=>fn get_current_thread,"DisableThreadLibraryCalls"=>fn disable_thread_library_calls,"GetCurrentThreadId"=>fn get_current_thread_id,
    "GetLastError"=>fn get_last_error, "SetLastError"=>fn set_last_error, "GetProcessHeap"=>fn get_process_heap,
      "HeapAlloc"=>fn heap_alloc,"HeapReAlloc"=>fn heap_realloc,"HeapFree"=>fn heap_free,"GlobalAlloc"=>fn global_alloc,"GlobalReAlloc"=>fn global_realloc,"GlobalLock"=>fn global_lock,"GlobalUnlock"=>fn global_unlock,"GlobalSize"=>fn global_size,"GlobalFlags"=>fn global_flags,"GlobalFree"=>fn global_free,"VirtualAlloc"=>fn virtual_alloc,"VirtualFree"=>fn virtual_free,"GetWriteWatch"=>fn crate::dlls::write_watch::get_write_watch,"ResetWriteWatch"=>fn crate::dlls::write_watch::reset_write_watch,"VirtualProtect"=>fn virtual_protect,"VirtualQuery"=>fn virtual_query,"VirtualQueryEx"=>fn virtual_query_ex,"FlushInstructionCache"=>fn flush_instruction_cache,
     "GetTickCount"=>fn get_tick_count,"GetTickCount64"=>fn get_tick_count64,"GetCurrentProcessId"=>fn get_current_process_id,"CreateThread"=>fn super::fil_trace::create_thread,"CloseHandle"=>fn super::fil_trace::close_handle,"GetEnvironmentVariableA"=>fn get_environment_variable_a,"SetEnvironmentVariableA"=>fn set_environment_variable_a,
     "GetSystemInfo"=>fn get_system_info,"GetNativeSystemInfo"=>fn get_native_system_info,"GetSystemTimeAsFileTime"=>fn get_system_time_as_file_time,"GetSystemTimePreciseAsFileTime"=>fn get_system_time_as_file_time,"GetLocalTime"=>fn get_local_time,"GetVersion"=>fn get_version,"GetVersionExA"=>fn get_version_ex_a,"GetVersionExW"=>fn get_version_ex_w,
      "GetCurrentDirectoryA"=>fn get_current_directory_a,"SetCurrentDirectoryA"=>fn set_current_directory_a,"GetTempPathA"=>fn get_temp_path_a,"GetTempFileNameA"=>fn get_temp_file_name_a,"GetModuleFileNameA"=>fn get_module_file_name_a,"GetFileAttributesA"=>fn get_file_attributes_a,"GetFileAttributesExA"=>fn get_file_attributes_ex_a,"DeleteFileA"=>fn delete_file_a,"CreateDirectoryA"=>fn create_directory_a,"CreateFileW"=>fn create_file_w,"GetFullPathNameA"=>fn get_full_path_name_a,"CopyFileA"=>fn copy_file_a,"MoveFileA"=>fn move_file_a,"RemoveDirectoryA"=>fn remove_directory_a,
      "GetEnvironmentVariableW"=>fn get_environment_variable_w,"SetEnvironmentVariableW"=>fn set_environment_variable_w,"GetCurrentDirectoryW"=>fn get_current_directory_w,"SetCurrentDirectoryW"=>fn set_current_directory_w,
      "GetTempPathW"=>fn get_temp_path_w,"GetTempFileNameW"=>fn get_temp_file_name_w,"GetModuleFileNameW"=>fn get_module_file_name_w,"GetFileAttributesW"=>fn get_file_attributes_w,"GetFileAttributesExW"=>fn get_file_attributes_ex_w,"DeleteFileW"=>fn delete_file_w,"CreateDirectoryW"=>fn create_directory_w,"GetFullPathNameW"=>fn get_full_path_name_w,"CopyFileW"=>fn copy_file_w,"MoveFileW"=>fn move_file_w,"RemoveDirectoryW"=>fn remove_directory_w,"SetEndOfFile"=>fn set_end_of_file,"Sleep"=>fn sleep,
     "TlsAlloc"=>fn tls_alloc, "TlsFree"=>fn tls_free, "TlsSetValue"=>fn tls_set_value, "TlsGetValue"=>fn tls_get_value,
     "WriteConsoleW"=>fn write_console_w,
    "QueryPerformanceFrequency"=>fn query_performance_frequency, "QueryPerformanceCounter"=>fn query_performance_counter,
     "QueryUnbiasedInterruptTime"=>fn query_unbiased_interrupt_time,
     "GetStartupInfoA"=>fn get_startup_info_a,"GetStartupInfoW"=>fn get_startup_info_w,"SetFilePointerEx"=>fn set_file_pointer_ex,"GetFileSizeEx"=>fn get_file_size_ex,"GetFileType"=>fn get_file_type,
     "SetFileAttributesA"=>fn set_file_attributes_a,"SetFileAttributesW"=>fn set_file_attributes_w,"GetFileTime"=>fn get_file_time,"SetFileTime"=>fn set_file_time,"LockFile"=>fn lock_file,"UnlockFile"=>fn unlock_file,
     "GetDiskFreeSpaceExA"=>fn get_disk_free_space_ex_a,"GetDiskFreeSpaceExW"=>fn get_disk_free_space_ex_w,"GetVolumeInformationA"=>fn get_volume_information_a,"GetVolumeInformationW"=>fn get_volume_information_w,"GetDriveTypeA"=>fn get_drive_type_a,"GetDriveTypeW"=>fn get_drive_type_w,"MoveFileExA"=>fn move_file_ex_a,"MoveFileExW"=>fn move_file_ex_w,
     // --- Nouvelles fonctions supplémentaires ---
     "HeapCreate"=>fn heap_create,"HeapDestroy"=>fn heap_destroy,"HeapSize"=>fn heap_size,"HeapValidate"=>fn heap_validate,"HeapSetInformation"=>fn heap_set_information,
     "FindFirstFileA"=>fn find_first_file_a,"FindNextFileA"=>fn find_next_file_a,"FindClose"=>fn find_close,
     "InitializeSRWLock"=>fn initialize_srw_lock,"AcquireSRWLockExclusive"=>fn acquire_srw_lock_exclusive,"ReleaseSRWLockExclusive"=>fn release_srw_lock_exclusive,"AcquireSRWLockShared"=>fn acquire_srw_lock_shared,"ReleaseSRWLockShared"=>fn release_srw_lock_shared,"TryAcquireSRWLockExclusive"=>fn try_acquire_srw_lock_exclusive,"TryAcquireSRWLockShared"=>fn try_acquire_srw_lock_shared,
     "InitializeConditionVariable"=>fn initialize_condition_variable,"SleepConditionVariableSRW"=>fn sleep_condition_variable_srw,"SleepConditionVariableCS"=>fn sleep_condition_variable_cs,"WakeConditionVariable"=>fn wake_condition_variable,"WakeAllConditionVariable"=>fn wake_all_condition_variable,
     "InitOnceBeginInitialize"=>fn init_once_begin_initialize,"InitOnceComplete"=>fn init_once_complete,"InitOnceInitialize"=>fn init_once_initialize,"InitOnceExecuteOnce"=>fn init_once_execute_once,
     "FlsAlloc"=>fn fls_alloc,"FlsFree"=>fn fls_free,"FlsGetValue"=>fn fls_get_value,"FlsSetValue"=>fn fls_set_value,
     "InitializeSListHead"=>fn initialize_slist_head,"InterlockedPushEntrySList"=>fn interlocked_push_entry_slist,"InterlockedPopEntrySList"=>fn interlocked_pop_entry_slist,"InterlockedFlushSList"=>fn interlocked_flush_slist,"QueryDepthSList"=>fn query_depth_slist,
     "EncodePointer"=>fn encode_pointer,"DecodePointer"=>fn decode_pointer,
     "GetLogicalDrives"=>fn get_logical_drives,"GetComputerNameA"=>fn get_computer_name_a,"GetComputerNameW"=>fn get_computer_name_w,
     "GetActiveProcessorCount"=>fn get_active_processor_count,
     "CreateWaitableTimerA"=>fn create_waitable_timer_a,"CreateWaitableTimerW"=>fn create_waitable_timer_w,"CreateWaitableTimerExW"=>fn create_waitable_timer_ex_w,"SetWaitableTimer"=>fn set_waitable_timer,"CancelWaitableTimer"=>fn cancel_waitable_timer,
     "GetFileInformationByHandle"=>fn get_file_information_by_handle,"GetFileInformationByHandleEx"=>fn get_file_information_by_handle_ex,"ReadDirectoryChangesW"=>fn read_directory_changes_w,"ReadDirectoryChangesExW"=>fn read_directory_changes_ex_w,"FindFirstChangeNotificationW"=>fn find_first_change_notification_w,"FindFirstChangeNotificationA"=>fn find_first_change_notification_a,"FindNextChangeNotification"=>fn find_next_change_notification,"FindCloseChangeNotification"=>fn find_close_change_notification,"QueueUserAPC"=>fn queue_user_apc,"GetThreadIOPendingFlag"=>fn get_thread_io_pending_flag,
     "CreateNamedPipeA"=>fn create_named_pipe_a,"ConnectNamedPipe"=>fn connect_named_pipe,"DisconnectNamedPipe"=>fn disconnect_named_pipe,
     "CreateFileMappingA"=>fn create_file_mapping_a,"CreateFileMappingW"=>fn create_file_mapping_w,"MapViewOfFile"=>fn map_view_of_file,"UnmapViewOfFile"=>fn unmap_view_of_file,
     "GetUserNameA"=>fn get_user_name_a,
     "GetPrivateProfileStringA"=>fn get_private_profile_string_a,"WritePrivateProfileStringA"=>fn write_private_profile_string_a,
     "GetLongPathNameA"=>fn get_long_path_name_a,"GetLongPathNameW"=>fn get_long_path_name_w,"GetShortPathNameA"=>fn get_short_path_name_a,"GetShortPathNameW"=>fn get_short_path_name_w,
     "CompareStringA"=>fn compare_string_a,"CompareStringW"=>fn compare_string_w,"CompareStringOrdinal"=>fn compare_string_ordinal,
     "GetStringTypeW"=>fn get_string_type_w,
     "LCMapStringW"=>fn lc_map_string_w,
     "OpenEventA"=>fn open_event_a,"OpenMutexA"=>fn open_mutex_a,"OpenSemaphoreW"=>fn open_semaphore_w,"OpenSemaphoreA"=>fn open_semaphore_a,
     "LoadLibraryExA"=>fn load_library_ex_a,"LoadLibraryExW"=>fn load_library_ex_w,
     // --- API Electron / Chromium ---
     "GetProductInfo"=>fn get_product_info,
     "GetSystemTime"=>fn get_system_time,
     "GetProcessTimes"=>fn get_process_times,"GetThreadTimes"=>fn get_thread_times,
     "SetHandleInformation"=>fn set_handle_information,"GetHandleInformation"=>fn get_handle_information,"DuplicateHandle"=>fn duplicate_handle,
     "CreateEventW"=>fn create_event_w,"CreateMutexW"=>fn create_mutex_w,
     "GetProcessId"=>fn get_process_id,"GetModuleHandleExW"=>fn get_module_handle_ex_w,
     "CreateSemaphoreW"=>fn create_semaphore_w,"CreateSemaphoreA"=>fn create_semaphore_a,"CreateSemaphoreExW"=>fn create_semaphore_ex_w,"CreateEventExW"=>fn create_event_ex_w,"CreateMutexExW"=>fn create_mutex_ex_w,"AreFileApisANSI"=>fn are_file_apis_ansi,"ReleaseSemaphore"=>fn super::fil_trace::release_semaphore,
     "SleepEx"=>fn sleep_ex,"WaitForSingleObjectEx"=>fn super::fil_trace::wait_for_single_object_ex,"SignalObjectAndWait"=>fn signal_object_and_wait,
     "CreateNamedPipeW"=>fn create_named_pipe_w,"SetNamedPipeHandleState"=>fn set_named_pipe_handle_state,"PeekNamedPipe"=>fn peek_named_pipe
     }
}

#[cfg(test)]
mod tests {
    use super::copy_keeping_write_time;

    #[test]
    fn compare_string_respecte_la_casse_sans_ignorecase() {
        // #2255 : sans NORM_IGNORECASE, « Abc » et « abc » ne sont pas égales.
        let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (maj, min) = (w("Abc"), w("abc"));
        unsafe {
            let cw = |f, a: &[u16], b: &[u16]| {
                super::compare_string_w(0x400, f, a.as_ptr(), -1, b.as_ptr(), -1)
            };
            assert_ne!(cw(0, &maj, &min), 2);
            assert_eq!(cw(1, &maj, &min), 2); // NORM_IGNORECASE
            assert_eq!(cw(0, &min, &min), 2);
            assert_eq!(cw(0, &w("abc"), &w("abd")), 1);
            assert_eq!(cw(0x2, &w("été"), &w("ete")), 2); // NORM_IGNORENONSPACE
            assert_eq!(
                super::compare_string_w(0x400, 0, maj.as_ptr(), 3, min.as_ptr(), 2),
                3
            );
            let ca = |f, a: &[u8], b: &[u8]| {
                super::compare_string_a(0x400, f, a.as_ptr(), -1, b.as_ptr(), -1)
            };
            assert_ne!(ca(0, b"Abc\0", b"abc\0"), 2);
            assert_eq!(ca(1, b"Abc\0", b"abc\0"), 2);
        }
    }

    #[test]
    fn copie_garde_la_date_de_derniere_ecriture() {
        let dir = std::env::temp_dir().join(format!("winlux-copie-date-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("SteamService.exe");
        let dst = dir.join("Copie.exe");
        std::fs::write(&src, b"MZ").unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_761_091_200);
        std::fs::File::open(&src)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(copy_keeping_write_time(&src, &dst).unwrap(), 2);
        assert_eq!(std::fs::metadata(&dst).unwrap().modified().unwrap(), old);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dllmain_des_dependances_dans_l_ordre_des_imports() {
        // #1584 : steamui importe libavcodec, SDL3, tier0, video64 et
        // vstdlib ; vstdlib importe tier0, libavcodec importe libavutil.
        // Projection : steamui (déjà à part), puis dans cet ordre.
        let names: Vec<String> = [
            "libavcodec.dll",
            "libavutil.dll",
            "SDL3.dll",
            "tier0_s64.dll",
            "video64.dll",
            "vstdlib_s64.dll",
            "orpheline.dll",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let imports = |i: usize| -> Vec<String> {
            let v: &[&str] = match i {
                0 => &["libavutil.dll", "KERNEL32.dll"],
                4 => &["libavcodec.dll", "TIER0_S64.DLL"],
                5 => &["tier0_s64.dll"],
                _ => &[],
            };
            v.iter().map(|s| s.to_string()).collect()
        };
        let roots: Vec<String> = [
            "libavcodec.dll",
            "SDL3.dll",
            "tier0_s64.dll",
            "video64.dll",
            "vstdlib_s64.dll",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let order = super::init_order(&roots, &names, &imports);
        let pos = |n: &str| order.iter().position(|&i| names[i] == n).unwrap();
        assert_eq!(order.len(), names.len());
        assert!(pos("tier0_s64.dll") < pos("vstdlib_s64.dll"));
        assert!(pos("tier0_s64.dll") < pos("video64.dll"));
        assert!(pos("libavutil.dll") < pos("libavcodec.dll"));
        assert!(pos("libavcodec.dll") < pos("video64.dll"));
        // Ordre des imports de steamui respecté entre DLL indépendantes.
        assert!(pos("libavcodec.dll") < pos("SDL3.dll"));
        assert!(pos("SDL3.dll") < pos("tier0_s64.dll"));
        // DLL qu'aucun import n'atteint : initialisée aussi.
        assert!(order.iter().any(|&i| names[i] == "orpheline.dll"));
    }
    /// #767 : un enfant de Chromium désigne le gestionnaire de crashpad
    /// reçu de son parent, et le transmet à ses propres enfants.
    #[test]
    fn gestionnaire_de_crashpad_designe_par_l_enfant() {
        let parent = unsafe { libc::getppid() };
        std::env::set_var(super::CRASH_HANDLER_ENV, parent.to_string());
        super::adopt_crash_handler();
        std::env::remove_var(super::CRASH_HANDLER_ENV);
        assert_eq!(super::crash_handler_pid(), Some(parent));
        let report = crate::crash::thread_limits_report();
        assert!(
            report.contains("threads ") && report.contains("RLIMIT_NPROC"),
            "{report}"
        );
    }

    #[test]
    fn code_de_sortie_complet_en_hexadecimal() {
        // #1205 : 0x780148E0, pas l'octet de poids faible (224).
        assert_eq!(&super::exit_code_text(0x7801_48E0), b"0x780148E0\n");
        assert_eq!(&super::exit_code_text(0), b"0x00000000\n");
        assert_eq!(&super::exit_code_text(0xC000_0005), b"0xC0000005\n");
    }

    #[test]
    fn programme_windows_absent_echoue_sans_lancer_de_runtime() {
        // #1133 : ScreenToGif lance Powershell.exe ; CreateProcess doit
        // rendre FALSE avec ERROR_FILE_NOT_FOUND, sans processus enfant.
        let _teb = crate::process::test_teb();
        for (line, code) in [
            ("Programme_absent_1133.exe -NoProfile\0", 2),
            ("C:\\dossier_absent_1133\\outil.exe\0", 3),
        ] {
            let mut info = ProcessInformation {
                process: 0,
                thread: 0,
                process_id: 0,
                thread_id: 0,
            };
            set_last_error(0);
            let ok = unsafe {
                create_process_a(
                    std::ptr::null(),
                    line.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    &mut info,
                )
            };
            assert_eq!(ok, 0, "{line}");
            assert_eq!(get_last_error(), code, "{line}");
            assert_eq!(info.process, 0);
        }
    }
    /// GetTickCount64 compte depuis le démarrage du système, pas depuis son
    /// premier appel (#534) : la valeur suit /proc/uptime dès le départ.
    #[test]
    fn tick_count_depuis_le_demarrage() {
        let uptime: f64 = std::fs::read_to_string("/proc/uptime")
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let ticks = super::get_tick_count64();
        let expected = (uptime * 1000.0) as u64;
        assert!(ticks + 2000 >= expected, "{ticks} ms, uptime {expected} ms");
    }

    /// GetProcessTimes rend le vrai temps processeur, bien inférieur au temps
    /// écoulé depuis le démarrage du système.
    #[test]
    fn process_times_temps_processeur() {
        let (mut c, mut e, mut k, mut u) = (0u64, 0u64, 0u64, 0u64);
        let mut x = 0u64;
        for i in 0..2_000_000u64 {
            x = x.wrapping_mul(31).wrapping_add(i);
        }
        std::hint::black_box(x);
        assert_eq!(
            unsafe { super::get_process_times(0, &mut c, &mut e, &mut k, &mut u) },
            1
        );
        assert!(u > 0, "temps utilisateur nul");
        assert!(
            u / 10_000 < super::get_tick_count64(),
            "temps utilisateur = durée depuis le démarrage"
        );
    }

    /// WaitOnAddress n'est pas dans kernel32.dll sous Windows, seulement dans
    /// KernelBase et api-ms-win-core-synch-l1-2-0 (VLC choisit sa branche
    /// selon ce GetProcAddress, #495).
    #[test]
    fn wait_on_address_absente_de_kernel32_seul() {
        let k32 = crate::dlls::module_handle("KERNEL32.dll").unwrap();
        let kb = crate::dlls::module_handle("kernelbase.dll").unwrap();
        let synch = crate::dlls::module_handle("api-ms-win-core-synch-l1-2-0.dll").unwrap();
        assert_ne!(k32, kb);
        assert_eq!(kb, synch);
        unsafe {
            for f in [
                c"WaitOnAddress",
                c"WakeByAddressAll",
                c"WakeByAddressSingle",
            ] {
                let n = f.as_ptr() as *const u8;
                assert_eq!(super::get_proc_address(k32, n), 0);
                assert_ne!(super::get_proc_address(kb, n), 0);
                assert_ne!(super::get_proc_address(synch, n), 0);
            }
            assert_ne!(
                super::get_proc_address(k32, c"Sleep".as_ptr() as *const u8),
                0
            );
            assert_ne!(
                super::get_proc_address(kb, c"Sleep".as_ptr() as *const u8),
                0
            );
        }
    }

    /// Fermer le jeton d'OpenProcessToken réussit (Chromium le vérifie).
    #[test]
    fn fermer_le_jeton_du_processus_reussit() {
        assert_eq!(super::close_handle(crate::dlls::security::TOKEN_HANDLE), 1);
    }

    #[test]
    fn open_process_ouvre_un_processus_etranger_et_le_termine() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep");
        let pid = child.id();
        unsafe {
            let h = super::open_process(0x1, 0, pid);
            assert_ne!(h, 0);
            assert_eq!(super::get_process_id(h), pid);
            assert_eq!(super::super::kernel32_cpu::set_priority_class(h, 0x4000), 1);
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
            let nice: i32 = stat
                .rsplit(')')
                .next()
                .unwrap()
                .split_whitespace()
                .nth(16)
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(nice, 10);
            assert_eq!(super::terminate_process(h, 1), 1);
            assert!(child.wait().is_ok());
            assert_eq!(super::open_process(0x1, 0, pid), 0);
            super::close_handle(h);
        }
    }

    use super::*;

    #[test]
    fn variable_cachee_dossier_du_lecteur() {
        // `_wchdir` (CRT Microsoft) : SetCurrentDirectoryW puis « =C: » (#1380).
        let name: Vec<u16> = "=C:".encode_utf16().chain([0]).collect();
        let val: Vec<u16> = "C:\\Program Files (x86)\\Steam"
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            assert_eq!(set_environment_variable_w(name.as_ptr(), val.as_ptr()), 1);
            let mut out = [0u16; 64];
            let n = get_environment_variable_w(name.as_ptr(), out.as_mut_ptr(), 64);
            assert_eq!(
                String::from_utf16_lossy(&out[..n as usize]),
                "C:\\Program Files (x86)\\Steam"
            );
            assert!(std::env::var_os("=C:").is_none());
            assert_eq!(
                set_environment_variable_w(name.as_ptr(), std::ptr::null()),
                1
            );
            assert_eq!(
                get_environment_variable_w(name.as_ptr(), out.as_mut_ptr(), 64),
                0
            );
            let bad: Vec<u16> = "A=B".encode_utf16().chain([0]).collect();
            assert_eq!(set_environment_variable_w(bad.as_ptr(), val.as_ptr()), 0);
        }
    }
    use std::ffi::CString;

    #[test]
    fn load_library_ex_handles_reserved_flags_and_builtin_system_libraries() {
        let _teb = crate::process::test_teb();
        let name: Vec<u16> = "kernel32.dll".encode_utf16().chain(Some(0)).collect();
        let expected = crate::dlls::module_handle("kernel32.dll").unwrap();
        unsafe {
            assert_eq!(load_library_ex_w(name.as_ptr(), 0, 0x800), expected);
            assert_eq!(load_library_ex_w(name.as_ptr(), 0, 0x1000), expected);
            assert_eq!(load_library_ex_w(name.as_ptr(), 1, 0), 0);
            assert_eq!(crate::process::last_error(), 87);
            // Mode données sur une DLL intégrée : son handle, sans rien exécuter.
            assert_eq!(load_library_ex_w(name.as_ptr(), 0, 2), expected);
            assert_eq!(load_library_ex_w(name.as_ptr(), 0, 8 | 0x800), 0);
            assert_eq!(crate::process::last_error(), 87);
            let relative = b"winlux-test-relative.dll\0";
            assert_eq!(load_library_ex_a(relative.as_ptr(), 0, 0x100), 0);
            assert_eq!(crate::process::last_error(), 87);
        }
    }

    /// DLL de ressources minimale (PE32+, une section .rsrc) dont la table
    /// de chaînes contient « Prisme » à l'identifiant 3.
    fn resource_only_dll() -> Vec<u8> {
        let mut f = vec![0u8; 0x400];
        f[0..2].copy_from_slice(b"MZ");
        f[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        f[0x40..0x44].copy_from_slice(b"PE\0\0");
        let w16 = |f: &mut Vec<u8>, o: usize, v: u16| f[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let w32 = |f: &mut Vec<u8>, o: usize, v: u32| f[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let coff = 0x44;
        w16(&mut f, coff, 0x8664);
        w16(&mut f, coff + 2, 1);
        w16(&mut f, coff + 16, 240);
        w16(&mut f, coff + 18, 0x2022);
        let opt = coff + 20;
        w16(&mut f, opt, 0x20b);
        f[opt + 24..opt + 32].copy_from_slice(&0x1_8000_0000u64.to_le_bytes());
        w32(&mut f, opt + 32, 0x1000);
        w32(&mut f, opt + 36, 0x200);
        w32(&mut f, opt + 56, 0x2000);
        w32(&mut f, opt + 60, 0x200);
        w16(&mut f, opt + 68, 2);
        w32(&mut f, opt + 108, 16);
        // Répertoire 2 : ressources.
        let mut rsrc = vec![0u8; 0x58];
        let dir = |r: &mut Vec<u8>, at: usize, id: u32, target: u32| {
            r[at + 14..at + 16].copy_from_slice(&1u16.to_le_bytes());
            r[at + 16..at + 20].copy_from_slice(&id.to_le_bytes());
            r[at + 20..at + 24].copy_from_slice(&target.to_le_bytes());
        };
        dir(&mut rsrc, 0, 6, 0x8000_0018);
        dir(&mut rsrc, 0x18, 1, 0x8000_0030);
        dir(&mut rsrc, 0x30, 0x409, 0x48);
        let mut table = Vec::new();
        for i in 0..16u16 {
            if i == 3 {
                table.extend_from_slice(&6u16.to_le_bytes());
                for u in "Prisme".encode_utf16() {
                    table.extend_from_slice(&u.to_le_bytes());
                }
            } else {
                table.extend_from_slice(&0u16.to_le_bytes());
            }
        }
        rsrc[0x48..0x4c].copy_from_slice(&(0x1000u32 + 0x58).to_le_bytes());
        rsrc[0x4c..0x50].copy_from_slice(&(table.len() as u32).to_le_bytes());
        rsrc.extend_from_slice(&table);
        w32(&mut f, opt + 112 + 2 * 8, 0x1000);
        w32(&mut f, opt + 112 + 2 * 8 + 4, rsrc.len() as u32);
        let sec = opt + 240;
        f[sec..sec + 5].copy_from_slice(b".rsrc");
        w32(&mut f, sec + 8, rsrc.len() as u32);
        w32(&mut f, sec + 12, 0x1000);
        w32(&mut f, sec + 16, 0x200);
        w32(&mut f, sec + 20, 0x200);
        w32(&mut f, sec + 36, 0x4000_0040);
        f[0x200..0x200 + rsrc.len()].copy_from_slice(&rsrc);
        f
    }

    #[test]
    fn load_library_ex_en_mode_donnees_pour_les_ressources() {
        let _teb = crate::process::test_teb();
        let name = format!("winlux-donnees-{}.dll", std::process::id());
        let linux = std::path::Path::new("/tmp").join(&name);
        std::fs::write(&linux, resource_only_dll()).unwrap();
        let path = CString::new(format!("Z:\\tmp\\{name}")).unwrap();
        unsafe {
            let h = load_library_ex_a(path.as_ptr().cast(), 0, 0x2);
            assert_ne!(
                h,
                0,
                "fichier de données chargé : {}",
                crate::process::last_error()
            );
            assert_eq!(h & 3, 1, "bit 1 : LOAD_LIBRARY_AS_DATAFILE");
            let mut buf = [0u16; 16];
            let n = crate::dlls::user32::load_string_w(h, 3, buf.as_mut_ptr(), 16);
            assert_eq!(String::from_utf16_lossy(&buf[..n as usize]), "Prisme");
            // LoadStringW(…, 0) : pointeur vers la chaîne dans l'image.
            let mut ptr = [0u16; 4];
            assert_eq!(
                crate::dlls::resource::load_string_w(h, 3, ptr.as_mut_ptr(), 0),
                6
            );
            let found = crate::dlls::resource::find_resource_w(h, 1 as *const u16, 6 as *const u16);
            assert_ne!(found, 0, "FindResource sur le handle de données");
            assert!(
                crate::dlls::module_handle(&name).is_none(),
                "invisible pour GetModuleHandle"
            );
            // Deuxième chargement en image de ressources : même image, bit 2.
            let h2 = load_library_ex_a(path.as_ptr().cast(), 0, 0x20);
            assert_eq!(h2 & !3, h & !3);
            assert_eq!(h2 & 3, 2);
            assert_eq!(free_library(h2), 1);
            assert_eq!(free_library(h), 1);
            assert!(crate::loader::data_image(h).is_none(), "image libérée");
            assert_eq!(free_library(h), 0);
            let missing = b"winlux-introuvable-xyz.dll\0";
            assert_eq!(load_library_ex_a(missing.as_ptr(), 0, 0x2), 0);
            assert_eq!(crate::process::last_error(), 2);
        }
        let _ = std::fs::remove_file(linux);
    }

    /// #1809 : DONT_RESOLVE_DLL_REFERENCES (lanceur CEF de Spotify) rend
    /// une base d'image sans marque, que GetModuleFileName, FindResource et
    /// FreeLibrary reconnaissent.
    #[test]
    fn load_library_ex_sans_resoudre_les_references() {
        let _teb = crate::process::test_teb();
        let name = format!("winlux-nonresolue-{}.dll", std::process::id());
        let linux = std::path::Path::new("/tmp").join(&name);
        std::fs::write(&linux, resource_only_dll()).unwrap();
        let windows = format!("Z:\\tmp\\{name}");
        let wide: Vec<u16> = windows.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let h = load_library_ex_w(wide.as_ptr(), 0, 0x1 | 0x8);
            assert_ne!(h, 0, "erreur {}", crate::process::last_error());
            assert_eq!(h & 3, 0, "base d'image sans marque");
            let mut buf = [0u16; 260];
            let n = get_module_file_name_w(h, buf.as_mut_ptr(), 260);
            assert_ne!(n, 0);
            let path = String::from_utf16_lossy(&buf[..n as usize]);
            assert!(path.ends_with(&name), "{path}");
            let found = crate::dlls::resource::find_resource_w(h, 1 as *const u16, 6 as *const u16);
            assert_ne!(found, 0, "FindResource sur l'image non résolue");
            assert_eq!(free_library(h), 1);
            assert!(crate::loader::data_image(h).is_none(), "image libérée");
        }
        let _ = std::fs::remove_file(linux);
    }

    #[test]
    fn create_file_reussi_remet_l_erreur_a_zero() {
        // #1315 : un CreateFileW réussi ne laisse pas l'erreur d'un appel
        // précédent (Claude pour Windows : 122 puis 50 sur config.json).
        init();
        let _teb = crate::process::test_teb();
        let path = format!(
            "Z:\\tmp\\winlux-k32err-{}-{}.json",
            std::process::id(),
            next_handle()
        );
        let path_c = CString::new(path).unwrap();
        let p = path_c.as_ptr() as *const u8;
        let h = unsafe { create_file_a(p, 0xC000_0000, 0, 0, 2, 0, 0) };
        assert_ne!(h, usize::MAX);
        unsafe { close_handle(h) };
        for (creation, flags, expected) in
            [(3, 0x80, 0), (5, 0x80, 0), (4, 0x80, 183), (2, 0x80, 183)]
        {
            crate::process::set_last_error(122);
            let h = unsafe { create_file_a(p, 0xC000_0000, 0, 0, creation, flags, 0) };
            assert_ne!(h, usize::MAX, "disposition {creation}");
            assert_eq!(
                crate::process::last_error(),
                expected,
                "disposition {creation}"
            );
            unsafe { close_handle(h) };
        }
        let dir = CString::new("Z:\\tmp").unwrap();
        crate::process::set_last_error(50);
        let h = unsafe { create_file_a(dir.as_ptr() as *const u8, 0x80, 7, 0, 3, 0x0200_0000, 0) };
        assert_ne!(h, usize::MAX);
        assert_eq!(
            crate::process::last_error(),
            0,
            "dossier (FILE_FLAG_BACKUP_SEMANTICS)"
        );
        unsafe { close_handle(h) };
        crate::process::set_last_error(0);
        let h = unsafe { create_file_a(p, 0xC000_0000, 0, 0, 1, 0x80, 0) };
        assert_eq!(h, usize::MAX);
        assert_eq!(
            crate::process::last_error(),
            80,
            "CREATE_NEW sur un fichier existant"
        );
        unsafe { delete_file_a(p) };
    }
    #[test]
    fn informations_etendues_d_un_handle_de_fichier() {
        init();
        let path = format!(
            "Z:\\tmp\\winlux-k32ex-{}-{}.bin",
            std::process::id(),
            next_handle()
        );
        let path_c = CString::new(path.clone()).unwrap();
        let h = unsafe { create_file_a(path_c.as_ptr() as *const u8, 0xC000_0000, 0, 0, 2, 0, 0) };
        assert_ne!(h, usize::MAX);
        let mut done = 0;
        unsafe { write_file(h, b"Prisme".as_ptr(), 6, &mut done, 0) };
        let mut standard = [0u8; 24];
        assert_eq!(
            unsafe { get_file_information_by_handle_ex(h, 1, standard.as_mut_ptr(), 24) },
            1
        );
        assert_eq!(i64::from_le_bytes(standard[8..16].try_into().unwrap()), 6);
        assert_eq!(standard[21], 0); // pas un dossier
        let mut basic = [0u8; 40];
        assert_eq!(
            unsafe { get_file_information_by_handle_ex(h, 0, basic.as_mut_ptr(), 40) },
            1
        );
        assert_eq!(u32::from_le_bytes(basic[32..36].try_into().unwrap()), 0x80);
        assert!(i64::from_le_bytes(basic[16..24].try_into().unwrap()) > 116_444_736_000_000_000);
        let mut name = [0u8; 600];
        assert_eq!(
            unsafe { get_file_information_by_handle_ex(h, 2, name.as_mut_ptr(), 600) },
            1
        );
        let bytes = u32::from_le_bytes(name[..4].try_into().unwrap()) as usize;
        let wide: Vec<u16> = name[4..4 + bytes]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert!(String::from_utf16_lossy(&wide).ends_with(".bin"));
        assert!(String::from_utf16_lossy(&wide).starts_with('\\'));
        let mut small = [0u8; 8];
        assert_eq!(
            unsafe { get_file_information_by_handle_ex(h, 1, small.as_mut_ptr(), 8) },
            0
        );
        assert_eq!(
            unsafe { get_file_information_by_handle_ex(h, 77, small.as_mut_ptr(), 8) },
            0
        );
        close_handle(h);
        let _ = std::fs::remove_file(crate::paths::to_linux(&path));
    }

    #[test]
    fn objets_de_synchronisation_ex() {
        init();
        unsafe {
            // CREATE_EVENT_MANUAL_RESET | CREATE_EVENT_INITIAL_SET : reste signalé.
            let e = create_event_ex_w(0, std::ptr::null(), 3, 0);
            assert_eq!(wait_for_single_object(e, 0), 0);
            assert_eq!(wait_for_single_object(e, 0), 0);
            let s = create_semaphore_ex_w(0, 1, 2, std::ptr::null(), 0, 0);
            assert_eq!(wait_for_single_object(s, 0), 0);
            assert_eq!(wait_for_single_object(s, 0), 0x102);
            let mut prev = -1;
            assert_eq!(release_semaphore(s, 2, &mut prev), 1);
            assert_eq!(prev, 0);
            assert_eq!(release_semaphore(s, 1, &mut prev), 0); // dépasserait le maximum (2)
            assert_eq!(wait_for_single_object(s, 0), 0);
            assert_eq!(wait_for_single_object(s, 0), 0);
            assert_eq!(wait_for_single_object(s, 0), 0x102);
            let m = create_mutex_ex_w(0, std::ptr::null(), 0, 0);
            assert_eq!(wait_for_single_object(m, 0), 0);
            assert_eq!(are_file_apis_ansi(), 1);
            for h in [e, s, m] {
                close_handle(h);
            }
        }
    }

    #[test]
    fn fichiers_win32_font_des_allers_retours_sans_perte() {
        init();
        let path = format!(
            "Z:\\tmp\\winlux-k32-{}-{}.bin",
            std::process::id(),
            next_handle()
        );
        let path_c = CString::new(path).unwrap();
        let h = unsafe { create_file_a(path_c.as_ptr() as *const u8, 0xC000_0000, 0, 0, 2, 0, 0) };
        assert_ne!(h, usize::MAX);
        let payload = [0x00, 0xff, b'W', b'X'];
        let mut done = 99;
        assert_eq!(
            unsafe { write_file(h, payload.as_ptr(), payload.len() as u32, &mut done, 0) },
            1
        );
        assert_eq!(done, payload.len() as u32);
        let mut high = 0;
        assert_eq!(unsafe { get_file_size(h, &mut high) }, payload.len() as u32);
        assert_eq!(high, 0);
        assert_eq!(
            unsafe { set_file_pointer(h, 0, std::ptr::null_mut(), 0) },
            0
        );
        let mut output = [0u8; 4];
        assert_eq!(
            unsafe { read_file(h, output.as_mut_ptr(), output.len() as u32, &mut done, 0) },
            1
        );
        assert_eq!(done, payload.len() as u32);
        assert_eq!(output, payload);
        assert_eq!(close_handle(h), 1);
        assert_eq!(unsafe { delete_file_a(path_c.as_ptr() as *const u8) }, 1);
    }

    #[test]
    fn lecture_ecriture_overlapped_renvoie_io_pending_puis_resultat() {
        init();
        crate::process::init_thread();
        let path = format!(
            "Z:\\tmp\\winlux-overlapped-{}-{}.bin",
            std::process::id(),
            next_handle()
        );
        let c = CString::new(path).unwrap();
        let h =
            unsafe { create_file_a(c.as_ptr() as *const u8, 0xC0000000, 7, 0, 2, 0x4000_0000, 0) };
        assert_ne!(h, usize::MAX);
        let payload = *b"async";
        let mut ov = Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 7,
            offset_high: 0,
            event: 0,
        };
        let mut transferred = 0;
        assert_eq!(
            unsafe {
                write_file(
                    h,
                    payload.as_ptr(),
                    payload.len() as u32,
                    &mut transferred,
                    &mut ov as *mut _ as usize,
                )
            },
            0
        );
        assert_eq!(crate::process::last_error(), 997);
        assert_eq!(
            unsafe { get_overlapped_result(h, &mut ov, &mut transferred, 1) },
            1
        );
        assert_eq!(transferred, 5);
        let mut output = [0u8; 5];
        ov = Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 7,
            offset_high: 0,
            event: 0,
        };
        assert_eq!(
            unsafe {
                read_file(
                    h,
                    output.as_mut_ptr(),
                    output.len() as u32,
                    &mut transferred,
                    &mut ov as *mut _ as usize,
                )
            },
            0
        );
        assert_eq!(
            unsafe { get_overlapped_result(h, &mut ov, &mut transferred, 1) },
            1
        );
        assert_eq!(transferred, 5);
        assert_eq!(output, payload);
        assert_eq!(close_handle(h), 1);
        assert_eq!(unsafe { delete_file_a(c.as_ptr() as *const u8) }, 1);
    }

    #[test]
    fn cancel_io_ex_annule_une_operation_en_file_dattente() {
        init();
        crate::process::init_thread();
        let path = format!(
            "Z:\\tmp\\winlux-cancel-{}-{}.bin",
            std::process::id(),
            next_handle()
        );
        let c = CString::new(path).unwrap();
        let h =
            unsafe { create_file_a(c.as_ptr() as *const u8, 0xC0000000, 7, 0, 2, 0x4000_0000, 0) };
        assert_ne!(h, usize::MAX);
        let file = {
            let handles = lock(HANDLES.get().unwrap());
            let Some(FileHandle::File(f)) = handles.get(&h) else {
                panic!("file handle absent")
            };
            f.clone()
        };
        let file_guard = lock(&file);
        let mut ov = Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            event: 0,
        };
        let mut written = 0;
        assert_eq!(
            unsafe {
                write_file(
                    h,
                    b"cancel".as_ptr(),
                    6,
                    &mut written,
                    &mut ov as *mut _ as usize,
                )
            },
            0
        );
        assert_eq!(unsafe { cancel_io_ex(h, &mut ov) }, 1);
        drop(file_guard);
        assert_eq!(
            unsafe { get_overlapped_result(h, &mut ov, &mut written, 1) },
            0
        );
        assert_eq!(crate::process::last_error(), 995);
        assert_eq!(close_handle(h), 1);
        assert_eq!(unsafe { delete_file_a(c.as_ptr() as *const u8) }, 1);
    }

    #[test]
    fn iocp_est_waitable_et_dequeue_des_paquets_en_lot() {
        init();
        let port = create_io_completion_port(usize::MAX, 0, 0, 0);
        assert_ne!(port, 0);
        unsafe {
            assert_eq!(
                post_queued_completion_status(port, 12, 0xabc, std::ptr::null_mut()),
                1
            );
            assert_eq!(
                post_queued_completion_status(port, 34, 0xdef, 0x1234usize as *mut c_void),
                1
            );
        }
        assert_eq!(wait_for_single_object(port, 0), 0);
        let mut entries = [OverlappedEntry {
            completion_key: 0,
            overlapped: std::ptr::null_mut(),
            internal: 0,
            bytes_transferred: 0,
            reserved: 0,
        }; 2];
        let mut count = 0;
        assert_eq!(
            unsafe {
                get_queued_completion_status_ex(port, entries.as_mut_ptr(), 2, &mut count, 0, 0)
            },
            1
        );
        assert_eq!(count, 2);
        assert_eq!(entries[0].completion_key, 0xabc);
        assert_eq!(entries[0].bytes_transferred, 12);
        assert_eq!(entries[1].completion_key, 0xdef);
        assert_eq!(entries[1].bytes_transferred, 34);
        assert_eq!(wait_for_single_object(port, 0), 258);
        assert_eq!(close_handle(port), 1);
    }

    #[test]
    fn iocp_limite_les_threads_actifs_selon_le_concurrency_max() {
        let port = create_io_completion_port(usize::MAX, 0, 0, 1);
        assert_ne!(port, 0);
        unsafe {
            post_queued_completion_status(port, 1, 1, std::ptr::null_mut());
        }
        let (mut bytes, mut key) = (0u32, 0usize);
        let mut ov = std::ptr::null_mut();
        assert_eq!(
            unsafe { get_queued_completion_status(port, &mut bytes, &mut key, &mut ov, 0) },
            1
        );
        assert_eq!(key, 1);
        unsafe {
            post_queued_completion_status(port, 2, 2, std::ptr::null_mut());
        }
        // Le premier fil tourne (boucle active) pendant que le second essaie :
        // il garde sa place, comme sous Windows.
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let got = Arc::new(std::sync::atomic::AtomicI32::new(-1));
        let (d, g) = (done.clone(), got.clone());
        let t = std::thread::spawn(move || {
            let (mut bytes, mut key) = (0u32, 0usize);
            let mut ov = std::ptr::null_mut();
            let r = unsafe { get_queued_completion_status(port, &mut bytes, &mut key, &mut ov, 0) };
            g.store(r, Ordering::SeqCst);
            d.store(true, Ordering::SeqCst);
        });
        while !done.load(Ordering::SeqCst) {
            std::hint::spin_loop();
        }
        t.join().unwrap();
        assert_eq!(got.load(Ordering::SeqCst), 0); // le premier thread tourne et détient le slot
        assert_eq!(
            unsafe { get_queued_completion_status(port, &mut bytes, &mut key, &mut ov, 0) },
            1
        );
        assert_eq!(key, 2);
        assert_eq!(close_handle(port), 1);
    }

    /// #2055 : un fil qui a retiré un paquet puis se bloque ailleurs (ici
    /// dans join) laisse sa place, comme sous Windows.
    #[test]
    fn iocp_place_d_un_fil_bloque_reprise_par_un_autre() {
        let port = create_io_completion_port(usize::MAX, 0, 0, 1);
        unsafe {
            post_queued_completion_status(port, 1, 1, std::ptr::null_mut());
        }
        let (mut bytes, mut key) = (0u32, 0usize);
        let mut ov = std::ptr::null_mut();
        assert_eq!(
            unsafe { get_queued_completion_status(port, &mut bytes, &mut key, &mut ov, 0) },
            1
        );
        unsafe {
            post_queued_completion_status(port, 2, 2, std::ptr::null_mut());
        }
        let second = std::thread::spawn(move || {
            let (mut bytes, mut key) = (0u32, 0usize);
            let mut ov = std::ptr::null_mut();
            let r =
                unsafe { get_queued_completion_status(port, &mut bytes, &mut key, &mut ov, 5000) };
            (r, key)
        })
        .join()
        .unwrap();
        assert_eq!(
            second,
            (1, 2),
            "paquet retiré pendant que le premier fil attend"
        );
        assert_eq!(close_handle(port), 1);
    }

    /// #2055 : ReadFile avec chevauchement remet l'événement à non signalé
    /// au début de l'E/S.
    #[test]
    fn readfile_avec_chevauchement_remet_l_evenement_a_zero() {
        init();
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let h = next_handle();
        lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new()))).insert(
            h,
            FileHandle::File(Arc::new(Mutex::new(unsafe {
                <std::fs::File as std::os::unix::io::FromRawFd>::from_raw_fd(fds[0])
            }))),
        );
        lock(overlapped_handles()).insert(h);
        let ev = unsafe { create_event_a(0, 1, 1, std::ptr::null()) };
        assert_eq!(wait_for_single_object(ev, 0), 0, "signalé avant l'E/S");
        let mut ov: Overlapped = unsafe { std::mem::zeroed() };
        ov.event = ev;
        let mut buf = [0u8; 4];
        let r = unsafe {
            read_file(
                h,
                buf.as_mut_ptr(),
                4,
                std::ptr::null_mut(),
                &mut ov as *mut Overlapped as usize,
            )
        };
        assert_eq!((r, ov.internal), (0, 0x103), "E/S en attente");
        assert_eq!(wait_for_single_object(ev, 0), 258, "remis à zéro au début");
        assert_eq!(
            unsafe { libc::write(fds[1], b"abcd".as_ptr().cast(), 4) },
            4
        );
        assert_eq!(wait_for_single_object(ev, 5000), 0, "signalé à la fin");
        assert_eq!(&buf, b"abcd");
        unsafe { libc::close(fds[1]) };
        close_handle(ev);
        close_handle(h);
    }

    extern "win64" fn iocp_one_packet_worker(port: usize) -> u32 {
        let (mut bytes, mut key) = (0u32, 0usize);
        let mut ov = std::ptr::null_mut();
        unsafe { get_queued_completion_status(port, &mut bytes, &mut key, &mut ov, 5000) as u32 }
    }

    #[test]
    fn iocp_libere_le_slot_quand_un_worker_win32_se_termine() {
        init();
        let port = create_io_completion_port(usize::MAX, 0, 0, 1);
        assert_ne!(port, 0);
        unsafe {
            post_queued_completion_status(port, 1, 0x11, std::ptr::null_mut());
        }
        let worker = unsafe {
            create_thread(
                0,
                0,
                iocp_one_packet_worker as *const () as usize,
                port,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(worker, 0);
        assert_eq!(wait_for_single_object(worker, 5000), 0);
        let mut worker_code = 0;
        assert_eq!(unsafe { get_exit_code_thread(worker, &mut worker_code) }, 1);
        assert_eq!(worker_code, 1);
        unsafe {
            post_queued_completion_status(port, 2, 0x22, std::ptr::null_mut());
        }
        let (mut bytes, mut key) = (0u32, 0usize);
        let mut ov = std::ptr::null_mut();
        assert_eq!(
            unsafe { get_queued_completion_status(port, &mut bytes, &mut key, &mut ov, 0) },
            1
        );
        assert_eq!(key, 0x22);
        assert_eq!(bytes, 2);
        close_handle(worker);
        close_handle(port);
    }

    #[test]
    fn evenements_auto_et_manuel_et_mutex_recursif() {
        init();
        let auto = unsafe { create_event_a(0, 0, 0, std::ptr::null()) };
        assert_eq!(wait_for_single_object(auto, 0), 258);
        assert_eq!(set_event(auto), 1);
        assert_eq!(wait_for_single_object(auto, 0), 0);
        assert_eq!(wait_for_single_object(auto, 0), 258); // auto-reset consomme le signal
        close_handle(auto);

        let manual = unsafe { create_event_a(0, 1, 0, std::ptr::null()) };
        assert_eq!(set_event(manual), 1);
        assert_eq!(wait_for_single_object(manual, 0), 0);
        assert_eq!(wait_for_single_object(manual, 0), 0); // le signal reste actif
        assert_eq!(reset_event(manual), 1);
        assert_eq!(wait_for_single_object(manual, 0), 258);
        close_handle(manual);

        let mutex = unsafe { create_mutex_a(0, 0, std::ptr::null()) };
        assert_eq!(wait_for_single_object(mutex, 0), 0);
        assert_eq!(wait_for_single_object(mutex, 0), 0); // mutex Win32 récursif
        assert_eq!(release_mutex(mutex), 1);
        assert_eq!(release_mutex(mutex), 1);
        assert_eq!(release_mutex(mutex), 0); // un mutex libre n'appartient pas au thread
        close_handle(mutex);
    }

    #[test]
    fn wait_for_multiple_objects_any_retourne_le_premier_handle_signale() {
        init();
        let a = unsafe { create_event_a(0, 0, 0, std::ptr::null()) };
        let b = unsafe { create_event_a(0, 0, 0, std::ptr::null()) };
        assert_eq!(set_event(b), 1);
        let handles = [a, b];
        assert_eq!(
            unsafe { wait_for_multiple_objects(2, handles.as_ptr(), 0, 0) },
            1
        );
        assert_eq!(
            unsafe { wait_for_multiple_objects(2, handles.as_ptr(), 1, 0) },
            258
        );
        close_handle(a);
        close_handle(b);
    }

    #[test]
    fn wait_all_ne_consomme_aucun_evenement_avant_que_tous_soient_signales() {
        init();
        let a = unsafe { create_event_a(0, 0, 0, std::ptr::null()) };
        let b = unsafe { create_event_a(0, 0, 0, std::ptr::null()) };
        let handles = [a, b];
        set_event(b);
        assert_eq!(
            unsafe { wait_for_multiple_objects(2, handles.as_ptr(), 1, 0) },
            258
        );
        assert_eq!(wait_for_single_object(b, 0), 0);
        set_event(b);
        set_event(a);
        assert_eq!(
            unsafe { wait_for_multiple_objects(2, handles.as_ptr(), 1, 0) },
            0
        );
        assert_eq!(wait_for_single_object(a, 0), 258);
        assert_eq!(wait_for_single_object(b, 0), 258);
        close_handle(a);
        close_handle(b);
    }

    #[test]
    fn wait_all_prend_le_mutex_seulement_quand_tout_est_signale() {
        init();
        let mutex = unsafe { create_mutex_a(0, 0, std::ptr::null()) };
        let event = unsafe { create_event_a(0, 1, 0, std::ptr::null()) };
        let handles = [mutex, event];
        assert_eq!(wait_for_single_object(mutex, 0), 0);
        assert_eq!(
            unsafe { wait_for_multiple_objects(2, handles.as_ptr(), 1, 0) },
            258
        );
        let Some(SyncHandle::Mutex(m)) = lookup_sync(mutex) else {
            panic!("mutex absent")
        };
        assert_eq!(release_mutex(mutex), 1);
        assert!(std::thread::spawn(move || {
            let acquired = wait_mutex(&m, 0) == 0;
            if acquired {
                leave_mutex(&m)
            } else {
                false
            }
        })
        .join()
        .unwrap());
        set_event(event);
        assert_eq!(
            unsafe { wait_for_multiple_objects(2, handles.as_ptr(), 1, 0) },
            0
        );
        assert_eq!(release_mutex(mutex), 1);
        assert_eq!(close_handle(mutex), 1);
        assert_eq!(close_handle(event), 1);
    }

    #[test]
    fn section_critique_est_recursive_et_supprimee_proprement() {
        init();
        let mut storage = [0usize; 5];
        let cs = storage.as_mut_ptr() as *mut c_void;
        unsafe {
            initialize_critical_section(cs);
            enter_critical_section(cs);
            enter_critical_section(cs);
            assert_eq!(try_enter_critical_section(cs), 1);
            leave_critical_section(cs);
            leave_critical_section(cs);
            leave_critical_section(cs);
            delete_critical_section(cs);
        }
        assert!(critical_object(cs as usize).is_none());
    }

    #[test]
    fn tls_valide_les_indices_et_invalide_les_valeurs_apres_tls_free() {
        init();
        crate::process::init_thread();
        let i = tls_alloc();
        assert_ne!(i, u32::MAX);
        assert_eq!(unsafe { tls_set_value(i, 0x1234usize as *mut c_void) }, 1);
        assert_eq!(tls_get_value(i) as usize, 0x1234);
        assert_eq!(tls_free(i), 1);
        assert!(tls_get_value(i).is_null());
        assert_eq!(unsafe { tls_set_value(i, std::ptr::null_mut()) }, 0);
        let reused = tls_alloc();
        assert_eq!(reused, i);
        assert!(tls_get_value(reused).is_null());
        tls_free(reused);
    }

    #[test]
    fn api_environnement_wide_sert_a_lire_et_ecrire_les_variables() {
        let key = format!("WINLUX_TEST_WIDE_{}", std::process::id());
        let value = "Ete été";
        let k: Vec<u16> = key.encode_utf16().chain([0]).collect();
        let v: Vec<u16> = value.encode_utf16().chain([0]).collect();
        unsafe {
            assert_eq!(set_environment_variable_w(k.as_ptr(), v.as_ptr()), 1);
            let mut out = [0u16; 32];
            assert_eq!(
                get_environment_variable_w(k.as_ptr(), out.as_mut_ptr(), out.len() as u32),
                value.encode_utf16().count() as u32
            );
            assert_eq!(
                String::from_utf16(&out[..value.encode_utf16().count()]).unwrap(),
                value
            );
            assert_eq!(set_environment_variable_w(k.as_ptr(), std::ptr::null()), 1);
            assert_eq!(
                get_environment_variable_w(k.as_ptr(), out.as_mut_ptr(), out.len() as u32),
                0
            );
        }
    }

    extern "win64" fn thread_returns_42(_param: usize) -> u32 {
        42
    }

    #[test]
    fn create_thread_retourne_un_handle_attendable_et_un_code_de_sortie() {
        init();
        let mut tid = 0;
        let h = unsafe {
            create_thread(
                0,
                0,
                thread_returns_42 as *const () as usize,
                0,
                0,
                &mut tid,
            )
        };
        assert_ne!(h, 0);
        assert_ne!(tid, 0);
        assert_eq!(wait_for_single_object(h, 5_000), 0);
        let mut code = 259;
        assert_eq!(unsafe { get_exit_code_thread(h, &mut code) }, 1);
        assert_eq!(code, 42);
        assert_eq!(close_handle(h), 1);
    }

    #[test]
    fn create_thread_suspended_attend_resume_thread() {
        init();
        let h = unsafe {
            create_thread(
                0,
                0,
                thread_returns_42 as *const () as usize,
                0,
                4,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(h, 0);
        assert_eq!(wait_for_single_object(h, 0), 258);
        assert_eq!(resume_thread(h), 1);
        assert_eq!(wait_for_single_object(h, 5_000), 0);
        close_handle(h);
    }

    #[test]
    fn create_process_execute_un_processus_local_et_retourne_son_code() {
        let app = std::ffi::CString::new("/bin/sh").unwrap();
        let line = std::ffi::CString::new("sh -c \"exit 42\"").unwrap();
        let startup = StartupInfoA {
            cb: std::mem::size_of::<StartupInfoA>() as u32,
            reserved: std::ptr::null_mut(),
            desktop: std::ptr::null_mut(),
            title: std::ptr::null_mut(),
            x: 0,
            y: 0,
            x_size: 0,
            y_size: 0,
            x_chars: 0,
            y_chars: 0,
            fill_attribute: 0,
            flags: 0,
            show_window: 0,
            reserved2_size: 0,
            reserved2: std::ptr::null_mut(),
            std_input: 0,
            std_output: 0,
            std_error: 0,
        };
        let mut info = ProcessInformation {
            process: 0,
            thread: 0,
            process_id: 0,
            thread_id: 0,
        };
        assert_eq!(
            unsafe {
                create_process_a(
                    app.as_ptr() as *const u8,
                    line.as_ptr() as *const u8,
                    0,
                    0,
                    0,
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup,
                    &mut info,
                )
            },
            1
        );
        assert_eq!(wait_for_single_object(info.process, 5000), 0);
        let mut code = 0;
        assert_eq!(unsafe { get_exit_code_process(info.process, &mut code) }, 1);
        assert_eq!(code, 42);
        assert_eq!(unsafe { get_exit_code_thread(info.thread, &mut code) }, 1);
        assert_eq!(code, 42);
        close_handle(info.thread);
        close_handle(info.process);
    }

    #[test]
    fn create_process_suspended_ne_demarre_quapres_resume_thread() {
        let app = std::ffi::CString::new("/bin/sh").unwrap();
        let line = std::ffi::CString::new("sh -c \"exit 42\"").unwrap();
        let startup = StartupInfoA {
            cb: std::mem::size_of::<StartupInfoA>() as u32,
            reserved: std::ptr::null_mut(),
            desktop: std::ptr::null_mut(),
            title: std::ptr::null_mut(),
            x: 0,
            y: 0,
            x_size: 0,
            y_size: 0,
            x_chars: 0,
            y_chars: 0,
            fill_attribute: 0,
            flags: 0,
            show_window: 0,
            reserved2_size: 0,
            reserved2: std::ptr::null_mut(),
            std_input: 0,
            std_output: 0,
            std_error: 0,
        };
        let mut info = ProcessInformation {
            process: 0,
            thread: 0,
            process_id: 0,
            thread_id: 0,
        };
        assert_eq!(
            unsafe {
                create_process_a(
                    app.as_ptr() as *const u8,
                    line.as_ptr() as *const u8,
                    0,
                    0,
                    0,
                    4,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup,
                    &mut info,
                )
            },
            1
        );
        assert_eq!(wait_for_single_object(info.process, 0), 258);
        let mut code = 0;
        assert_eq!(unsafe { get_exit_code_process(info.process, &mut code) }, 1);
        assert_eq!(code, 259);
        assert_eq!(resume_thread(info.thread), 1);
        assert_eq!(wait_for_single_object(info.process, 5000), 0);
        assert_eq!(unsafe { get_exit_code_process(info.process, &mut code) }, 1);
        assert_eq!(code, 42);
        close_handle(info.thread);
        close_handle(info.process);
    }

    /// #1042 : une lecture de zéro octet sur un tube attend des données
    /// (libuv guette ainsi stdin) ; tube fermé et vide : ERROR_BROKEN_PIPE.
    #[test]
    fn zero_byte_read_on_a_pipe_waits_for_data() {
        let _teb = crate::process::test_teb();
        let (mut r, mut w) = (0usize, 0usize);
        unsafe {
            assert_eq!(create_pipe(&mut r, &mut w, std::ptr::null(), 0), 1);
        }
        let writer = w;
        let start = std::time::Instant::now();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let mut n = 0u32;
            unsafe { write_file(writer, b"x".as_ptr(), 1, &mut n, 0) };
        });
        let mut buf = [0u8; 4];
        let mut n = 7u32;
        unsafe {
            assert_eq!(read_file(r, buf.as_mut_ptr(), 0, &mut n, 0), 1);
        }
        assert!(
            start.elapsed() >= std::time::Duration::from_millis(250),
            "rendue sans attendre"
        );
        assert_eq!(n, 0);
        t.join().unwrap();
        // Les données sont toujours là pour la vraie lecture.
        unsafe {
            assert_eq!(read_file(r, buf.as_mut_ptr(), 4, &mut n, 0), 1);
        }
        assert_eq!((n, buf[0]), (1, b'x'));
        close_handle(w);
        unsafe {
            assert_eq!(read_file(r, buf.as_mut_ptr(), 0, &mut n, 0), 0);
        }
        assert_eq!(crate::process::last_error(), 109);
        close_handle(r);
    }

    #[test]
    fn create_process_relie_les_tubes_dentree_et_de_sortie_dune_appli_linux() {
        let _teb = crate::process::test_teb();
        let mut out_r = 0usize;
        let mut out_w = 0usize;
        let mut in_r = 0usize;
        let mut in_w = 0usize;
        unsafe {
            assert_eq!(create_pipe(&mut out_r, &mut out_w, std::ptr::null(), 0), 1);
            assert_eq!(create_pipe(&mut in_r, &mut in_w, std::ptr::null(), 0), 1);
        }
        // Copie du bout de lecture, puis fermeture de l'original (comme Python).
        let mut out_copy = 0usize;
        unsafe {
            assert_eq!(
                duplicate_handle(usize::MAX, out_r, usize::MAX, &mut out_copy, 0, 0, 1),
                1
            );
        }
        assert_ne!(out_copy, out_r);
        let line = std::ffi::CString::new(
            "sh -c \"read l; echo \\\"recu:$l:$1\\\"; echo err >&2; exit 7\" x Z:\\etc",
        )
        .unwrap();
        let startup = StartupInfoA {
            cb: std::mem::size_of::<StartupInfoA>() as u32,
            reserved: std::ptr::null_mut(),
            desktop: std::ptr::null_mut(),
            title: std::ptr::null_mut(),
            x: 0,
            y: 0,
            x_size: 0,
            y_size: 0,
            x_chars: 0,
            y_chars: 0,
            fill_attribute: 0,
            flags: 0x100,
            show_window: 0,
            reserved2_size: 0,
            reserved2: std::ptr::null_mut(),
            std_input: in_r,
            std_output: out_w,
            std_error: out_w,
        };
        let mut info = ProcessInformation {
            process: 0,
            thread: 0,
            process_id: 0,
            thread_id: 0,
        };
        let ok = unsafe {
            create_process_a(
                std::ptr::null(),
                line.as_ptr() as *const u8,
                0,
                0,
                1,
                0x0800_0000 | 0x20,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut info,
            )
        };
        assert_eq!(ok, 1, "erreur {}", get_last_error());
        close_handle(in_r);
        close_handle(out_w);
        let mut n = 0u32;
        let msg = b"bonjour\n";
        unsafe {
            assert_eq!(
                write_file(in_w, msg.as_ptr(), msg.len() as u32, &mut n, 0),
                1
            );
        }
        close_handle(in_w);
        assert_eq!(wait_for_single_object(info.process, 5000), 0);
        let mut code = 0;
        assert_eq!(unsafe { get_exit_code_process(info.process, &mut code) }, 1);
        assert_eq!(code, 7);
        let mut avail = 0u32;
        unsafe {
            assert_eq!(
                peek_named_pipe(
                    out_copy,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut avail,
                    std::ptr::null_mut()
                ),
                1
            );
        }
        assert!(avail > 0);
        let mut all = Vec::new();
        let mut buf = [0u8; 64];
        loop {
            let mut got = 0u32;
            let r = unsafe { read_file(out_copy, buf.as_mut_ptr(), 64, &mut got, 0) };
            if r == 0 {
                assert_eq!(get_last_error(), 109);
                break;
            }
            all.extend_from_slice(&buf[..got as usize]);
        }
        // Le chemin Windows `Z:\etc` est arrivé en `/etc` dans l'appli Linux.
        assert_eq!(String::from_utf8(all).unwrap(), "recu:bonjour:/etc\nerr\n");
        close_handle(out_copy);
        close_handle(info.thread);
        close_handle(info.process);
    }

    #[test]
    fn create_process_lit_les_blocs_denvironnement_ansi_et_unicode() {
        let ansi = b"PATH=/bin\0WINLUX_CHILD=ansi\0\0";
        let parsed = unsafe { parse_environment_block(ansi.as_ptr() as *const c_void, false) }
            .unwrap()
            .unwrap();
        assert!(parsed.iter().any(|(k, v)| k == "PATH" && v == "/bin"));
        assert!(parsed
            .iter()
            .any(|(k, v)| k == "WINLUX_CHILD" && v == "ansi"));
        let wide: Vec<u16> = "PATH=/bin\0WINLUX_CHILD=unicode\0\0"
            .encode_utf16()
            .collect();
        let parsed = unsafe { parse_environment_block(wide.as_ptr() as *const c_void, true) }
            .unwrap()
            .unwrap();
        assert!(parsed.iter().any(|(k, v)| k == "PATH" && v == "/bin"));
        assert!(parsed
            .iter()
            .any(|(k, v)| k == "WINLUX_CHILD" && v == "unicode"));
        assert!(
            unsafe { parse_environment_block(b"BAD\0\0".as_ptr() as *const c_void, false) }
                .unwrap()
                .unwrap()
                .is_empty()
        );
    }
}

/// Tests de l'audit kernel32/ntdll (#211) : threads, handles, mémoire
/// globale, SList, SRW, InitOnce, horodatages, locale et chemins de module.
#[cfg(test)]
mod audit_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    extern "win64" fn thread_records_its_id_then_exits(param: usize) -> u32 {
        unsafe {
            *(param as *mut u32) = get_current_thread_id();
        }
        super::super::kernel32_misc::exit_thread(7)
    }

    #[test]
    fn fichier_supprime_a_la_fermeture_et_create_new_sur_existant() {
        // #1252 : Sumatra PDF portable teste son dossier avec CREATE_NEW et
        // FILE_FLAG_DELETE_ON_CLOSE ; le fichier doit disparaître à la
        // fermeture, et un CREATE_NEW sur un fichier existant rend 80.
        init();
        crate::process::init_thread();
        let path = format!(
            "Z:\\tmp\\winlux-delete-on-close-{}-{}.tmp",
            std::process::id(),
            next_handle()
        );
        let host = crate::paths::to_linux(&path);
        let c = std::ffi::CString::new(path).unwrap();
        let open = |flags: u32| unsafe {
            create_file_a(c.as_ptr() as *const u8, 0xC000_0000, 0, 0, 1, flags, 0)
        };
        let h = open(0x0400_0080);
        assert_ne!(h, usize::MAX);
        assert!(host.exists(), "visible tant que le handle est ouvert");
        assert_eq!(close_handle(h), 1);
        assert!(!host.exists(), "supprimé à la fermeture du handle");
        let h = open(0x80);
        assert_ne!(h, usize::MAX);
        assert_eq!(close_handle(h), 1);
        assert_eq!(open(0x80), usize::MAX);
        assert_eq!(crate::process::last_error(), 80, "ERROR_FILE_EXISTS");
        let _ = std::fs::remove_file(host);
    }

    #[test]
    fn create_thread_rend_l_id_du_thread_et_exit_thread_signale_le_handle() {
        init();
        let mut seen = 0u32;
        let mut tid = 0u32;
        let h = unsafe {
            create_thread(
                0,
                0,
                thread_records_its_id_then_exits as *const () as usize,
                &mut seen as *mut u32 as usize,
                0,
                &mut tid,
            )
        };
        assert_ne!(h, 0);
        assert_ne!(tid, 0);
        // ExitThread signale le handle : WaitForSingleObject rend la main…
        assert_eq!(wait_for_single_object(h, 5_000), 0);
        // … GetExitCodeThread lit le code passé à ExitThread…
        let mut code = 0;
        assert_eq!(unsafe { get_exit_code_thread(h, &mut code) }, 1);
        assert_eq!(code, 7);
        // … et l'id rendu par CreateThread est celui de GetCurrentThreadId.
        assert_eq!(seen, tid);
        assert_eq!(close_handle(h), 1);
    }

    #[test]
    fn duplicate_handle_copie_un_objet_de_synchronisation() {
        init();
        let event = unsafe { create_event_a(0, 1, 0, std::ptr::null()) };
        let mut copy = 0usize;
        assert_eq!(
            unsafe { duplicate_handle(usize::MAX, event, usize::MAX, &mut copy, 0, 0, 2) },
            1
        );
        assert_ne!(copy, 0);
        assert_ne!(copy, event);
        // Même objet : signaler la copie réveille l'original.
        assert_eq!(set_event(copy), 1);
        assert_eq!(wait_for_single_object(event, 0), 0);
        // Fermer la copie ne ferme pas l'original.
        assert_eq!(close_handle(copy), 1);
        assert_eq!(reset_event(event), 1);
        assert_eq!(wait_for_single_object(event, 0), 258);
        // DUPLICATE_CLOSE_SOURCE ferme l'original, la copie survit.
        let mut second = 0usize;
        assert_eq!(
            unsafe { duplicate_handle(usize::MAX, event, usize::MAX, &mut second, 0, 0, 1) },
            1
        );
        assert_eq!(close_handle(event), 0);
        assert_eq!(set_event(second), 1);
        assert_eq!(close_handle(second), 1);
        // Handle inconnu : échec ERROR_INVALID_HANDLE.
        crate::process::init_thread();
        let mut none = 1usize;
        assert_eq!(
            unsafe { duplicate_handle(usize::MAX, 0xdead_0000, usize::MAX, &mut none, 0, 0, 0) },
            0
        );
        assert_eq!(none, 0);
        assert_eq!(crate::process::last_error(), 6);
    }

    #[test]
    fn global_alloc_fixe_rend_un_pointeur_direct() {
        init();
        // GMEM_FIXED : le handle est le pointeur, GlobalLock le rend tel quel.
        let fixed = global_alloc(0x40, 16);
        assert_ne!(fixed, 0);
        assert_eq!(global_lock(fixed) as usize, fixed);
        unsafe {
            *(fixed as *mut u8) = 0x5a;
        }
        assert_eq!(global_size(fixed), 16);
        let grown = global_realloc(fixed, 4096, 0);
        assert_ne!(grown, 0);
        assert_eq!(global_lock(grown) as usize, grown);
        assert_eq!(unsafe { *(grown as *const u8) }, 0x5a);
        assert_eq!(global_size(grown), 4096);
        assert_eq!(global_free(grown), 0);
        // GMEM_MOVEABLE : handle opaque, distinct du pointeur verrouillé.
        let moveable = global_alloc(0x42, 8);
        assert_ne!(moveable, 0);
        let p = global_lock(moveable) as usize;
        assert_ne!(p, moveable);
        assert_eq!(global_unlock(moveable), 0);
        assert_eq!(global_free(moveable), 0);
        // Un bloc fixe de taille nulle garde un handle unique.
        let a = global_alloc(0, 0);
        let b = global_alloc(0, 0);
        assert_ne!(a, b);
        assert_eq!(global_free(a), 0);
        assert_eq!(global_free(b), 0);
    }

    #[test]
    fn slist_empile_depile_et_vide_sous_verrou() {
        #[repr(C, align(16))]
        struct Entry {
            next: usize,
            value: u32,
        }
        #[repr(C, align(16))]
        struct Head([u8; 16]);
        let mut head = Head([0; 16]);
        let mut entries = [
            Entry { next: 0, value: 1 },
            Entry { next: 0, value: 2 },
            Entry { next: 0, value: 3 },
        ];
        let h = &mut head as *mut Head as usize;
        unsafe {
            initialize_slist_head(h);
            assert_eq!(query_depth_slist(h), 0);
            for e in entries.iter_mut() {
                interlocked_push_entry_slist(h, e as *mut Entry as usize);
            }
            assert_eq!(query_depth_slist(h), 3);
            // Même table que ntdll : RtlQueryDepthSList voit la même pile.
            assert_eq!(
                super::super::ntdll::rtl_query_depth_slist(h as *const u8),
                3
            );
            let top = interlocked_pop_entry_slist(h) as *const Entry;
            assert_eq!((*top).value, 3);
            assert_eq!(query_depth_slist(h), 2);
            let first = interlocked_flush_slist(h) as *const Entry;
            assert_eq!((*first).value, 2);
            assert_eq!(((*first).next as *const Entry).read().value, 1);
            assert_eq!(query_depth_slist(h), 0);
            assert_eq!(interlocked_pop_entry_slist(h), 0);
        }
    }

    #[test]
    fn srw_lock_bloque_un_second_thread_sans_spin() {
        static ORDER: AtomicU32 = AtomicU32::new(0);
        let lock_word = Box::leak(Box::new(0usize)) as *mut usize as usize;
        initialize_srw_lock(lock_word);
        acquire_srw_lock_exclusive(lock_word);
        assert_eq!(try_acquire_srw_lock_exclusive(lock_word), 0);
        assert_eq!(try_acquire_srw_lock_shared(lock_word), 0);
        let waiter = std::thread::spawn(move || {
            // Le ntdll Rtl* partage le même état que kernel32.
            unsafe {
                super::super::ntdll::rtl_acquire_srw_lock_exclusive(
                    lock_word as *mut std::ffi::c_void,
                )
            };
            let seen = ORDER.fetch_add(1, Ordering::SeqCst);
            release_srw_lock_exclusive(lock_word);
            seen
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        // Le second thread n'a pas pu entrer tant que le verrou était tenu.
        assert_eq!(ORDER.load(Ordering::SeqCst), 0);
        ORDER.fetch_add(1, Ordering::SeqCst);
        release_srw_lock_exclusive(lock_word);
        assert_eq!(waiter.join().unwrap(), 1);
        // Deux lecteurs entrent ensemble ; un écrivain attend qu'ils sortent.
        acquire_srw_lock_shared(lock_word);
        assert_eq!(try_acquire_srw_lock_shared(lock_word), 1);
        assert_eq!(try_acquire_srw_lock_exclusive(lock_word), 0);
        release_srw_lock_shared(lock_word);
        release_srw_lock_shared(lock_word);
        assert_eq!(try_acquire_srw_lock_exclusive(lock_word), 1);
        release_srw_lock_exclusive(lock_word);
    }

    static INIT_ONCE_RUNS: AtomicUsize = AtomicUsize::new(0);
    extern "win64" fn init_once_callback(_once: usize, param: usize, context: *mut usize) -> i32 {
        INIT_ONCE_RUNS.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));
        unsafe {
            *context = param;
        }
        1
    }

    #[test]
    fn init_once_execute_once_n_initialise_qu_une_fois_sous_concurrence() {
        let once = Box::leak(Box::new(0usize)) as *mut usize as usize;
        init_once_initialize(once);
        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(move || {
                    let mut context = 0usize;
                    let ok = unsafe {
                        init_once_execute_once(
                            once,
                            init_once_callback as *const () as usize,
                            0x1230,
                            &mut context,
                        )
                    };
                    (ok, context)
                })
            })
            .collect();
        for t in threads {
            // Tous reçoivent le contexte produit par l'unique initialisation.
            assert_eq!(t.join().unwrap(), (1, 0x1230));
        }
        assert_eq!(INIT_ONCE_RUNS.load(Ordering::SeqCst), 1);
        // Le mot garde l'état « terminé » et le contexte.
        let mut pending = 1;
        let mut context = 0usize;
        assert_eq!(
            init_once_begin_initialize(once, 1, &mut pending, &mut context),
            1
        );
        assert_eq!((pending, context), (0, 0x1230));
    }

    #[test]
    fn system_time_et_file_time_font_l_aller_retour() {
        use crate::intl::api::{file_time_to_system_time, system_time_to_file_time};
        let dates = [
            (1601, 1, 1, 0, 0, 0, 0, 1u16),    // origine FILETIME, lundi
            (1970, 1, 1, 0, 0, 0, 0, 4),       // origine Unix, jeudi
            (2000, 2, 29, 12, 30, 45, 500, 2), // bissextile (divisible par 400)
            (2024, 2, 29, 23, 59, 59, 999, 4), // bissextile
            (2026, 11, 1, 9, 15, 0, 0, 0),     // date de l'audit, dimanche
            (2100, 3, 1, 0, 0, 0, 0, 1),       // 2100 n'est pas bissextile
        ];
        for (year, month, day, hour, minute, second, millis, weekday) in dates {
            let st = crate::intl::tz::SystemTime {
                year,
                month,
                day_of_week: 0,
                day,
                hour,
                minute,
                second,
                millis,
            };
            let mut ft = 0i64;
            assert_eq!(unsafe { system_time_to_file_time(&st, &mut ft) }, 1);
            let mut back = crate::intl::tz::SystemTime::default();
            assert_eq!(unsafe { file_time_to_system_time(&ft, &mut back) }, 1);
            assert_eq!(
                (
                    back.year,
                    back.month,
                    back.day,
                    back.hour,
                    back.minute,
                    back.second,
                    back.millis,
                    back.day_of_week
                ),
                (year, month, day, hour, minute, second, millis, weekday),
                "{year}-{month:02}-{day:02}"
            );
        }
        // Valeurs de référence : 1970-01-01 = 116444736000000000 ;
        // 2026-11-01 09:15:00 UTC = 1793524500 s depuis 1970.
        let unix = crate::intl::tz::SystemTime {
            year: 1970,
            month: 1,
            day: 1,
            ..Default::default()
        };
        let mut ft = 0i64;
        assert_eq!(unsafe { system_time_to_file_time(&unix, &mut ft) }, 1);
        assert_eq!(ft, 116_444_736_000_000_000);
        let audit = crate::intl::tz::SystemTime {
            year: 2026,
            month: 11,
            day: 1,
            hour: 9,
            minute: 15,
            ..Default::default()
        };
        assert_eq!(unsafe { system_time_to_file_time(&audit, &mut ft) }, 1);
        assert_eq!(ft, 116_444_736_000_000_000 + 1_793_524_500 * 10_000_000);
        // 29 février d'une année non bissextile : refusé.
        let bad = crate::intl::tz::SystemTime {
            year: 2023,
            month: 2,
            day: 29,
            ..Default::default()
        };
        assert_eq!(unsafe { system_time_to_file_time(&bad, &mut ft) }, 0);
    }

    #[test]
    fn get_locale_info_w_garde_le_dernier_caractere() {
        let _teb = crate::process::test_teb();
        const LOCALE_SENGLISHLANGUAGENAME: u32 = 0x1001;
        unsafe {
            let need = super::super::nls::get_locale_info_w(
                0x0409,
                LOCALE_SENGLISHLANGUAGENAME,
                std::ptr::null_mut(),
                0,
            );
            assert_eq!(need, 8); // "English" + NUL
            let mut buf = [0xffffu16; 8];
            assert_eq!(
                super::super::nls::get_locale_info_w(
                    0x0409,
                    LOCALE_SENGLISHLANGUAGENAME,
                    buf.as_mut_ptr(),
                    buf.len() as i32
                ),
                8
            );
            assert_eq!(String::from_utf16(&buf[..7]).unwrap(), "English");
            assert_eq!(buf[7], 0);
            // Un caractère de moins : refus, pas de troncature silencieuse.
            let mut short = [0xffffu16; 7];
            assert_eq!(
                super::super::nls::get_locale_info_w(
                    0x0409,
                    LOCALE_SENGLISHLANGUAGENAME,
                    short.as_mut_ptr(),
                    short.len() as i32
                ),
                0
            );
            assert_eq!(crate::process::last_error(), 122);
        }
    }

    #[test]
    fn get_module_file_name_a_tronque_et_signale_le_tampon_insuffisant() {
        let _teb = crate::process::test_teb();
        let image: &'static mut [u8] = Box::leak(vec![0u8; 8192].into_boxed_slice());
        let base = image.as_ptr() as usize;
        let module = crate::loader::Module {
            name: "winlux-audit-trunc.dll".into(),
            path: "/tmp/winlux-audit-trunc.dll".into(),
            base,
            size: image.len(),
            pe: crate::pe::PeImage {
                characteristics: 0,
                entry_rva: 0,
                image_base: 0,
                section_alignment: 0x1000,
                size_of_image: image.len() as u32,
                size_of_headers: 0,
                subsystem: 3,
                dll_characteristics: 0,
                stack_reserve: 0,
                stack_commit: 0,
                dirs: vec![crate::pe::DataDir::default(); 16],
                sections: vec![],
                il_only32: false,
                x86: false,
            },
            tls: None,
            initialized: true,
            refcount: 1,
        };
        crate::loader::MODULES.lock().unwrap().push(module);
        let expected = "Z:\\tmp\\winlux-audit-trunc.dll";
        unsafe {
            let mut full = [0u8; 64];
            let n = get_module_file_name_a(base, full.as_mut_ptr(), full.len() as u32);
            assert_eq!(n as usize, expected.len());
            assert_eq!(std::str::from_utf8(&full[..n as usize]).unwrap(), expected);
            // Tampon trop petit : nSize renvoyé, chaîne tronquée terminée,
            // ERROR_INSUFFICIENT_BUFFER.
            let mut small = [0xaau8; 10];
            crate::process::set_last_error(0);
            assert_eq!(get_module_file_name_a(base, small.as_mut_ptr(), 10), 10);
            assert_eq!(crate::process::last_error(), 122);
            assert_eq!(&small[..9], &expected.as_bytes()[..9]);
            assert_eq!(small[9], 0);
            // Idem en large.
            let mut small_w = [0xaaaau16; 6];
            assert_eq!(get_module_file_name_w(base, small_w.as_mut_ptr(), 6), 6);
            assert_eq!(crate::process::last_error(), 122);
            assert_eq!(String::from_utf16(&small_w[..5]).unwrap(), "Z:\\tm");
            assert_eq!(small_w[5], 0);
        }
        crate::loader::remove_module(base);
    }

    #[test]
    fn read_file_avec_overlapped_est_synchrone_sans_file_flag_overlapped() {
        init();
        crate::process::init_thread();
        let path = format!(
            "Z:\\tmp\\winlux-sync-ov-{}-{}.bin",
            std::process::id(),
            next_handle()
        );
        let c = std::ffi::CString::new(path).unwrap();
        let h = unsafe { create_file_a(c.as_ptr() as *const u8, 0xC0000000, 7, 0, 2, 0, 0) };
        assert_ne!(h, usize::MAX);
        let mut ov = Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 4,
            offset_high: 0,
            event: 0,
        };
        let mut done = 0;
        // Écriture à l'offset 4 de l'OVERLAPPED, terminée au retour.
        assert_eq!(
            unsafe {
                write_file(
                    h,
                    b"sync".as_ptr(),
                    4,
                    &mut done,
                    &mut ov as *mut _ as usize,
                )
            },
            1
        );
        assert_eq!(done, 4);
        assert_eq!(ov.internal, 0);
        let mut out = [0u8; 4];
        ov.offset = 4;
        assert_eq!(
            unsafe {
                read_file(
                    h,
                    out.as_mut_ptr(),
                    4,
                    &mut done,
                    &mut ov as *mut _ as usize,
                )
            },
            1
        );
        assert_eq!((&out, done), (b"sync", 4));
        assert_eq!(unsafe { get_file_size(h, std::ptr::null_mut()) }, 8);
        assert_eq!(close_handle(h), 1);
        assert_eq!(unsafe { delete_file_a(c.as_ptr() as *const u8) }, 1);
    }

    #[test]
    fn get_file_type_distingue_disque_tube_et_caractere() {
        init();
        let mut r = 0usize;
        let mut w = 0usize;
        assert_eq!(
            unsafe { create_pipe(&mut r, &mut w, std::ptr::null(), 0) },
            1
        );
        assert_eq!(get_file_type(r), 3);
        close_handle(r);
        close_handle(w);
        let dev = std::ffi::CString::new("Z:\\dev\\null").unwrap();
        let h = unsafe { create_file_a(dev.as_ptr() as *const u8, 0x80000000, 7, 0, 3, 0, 0) };
        assert_ne!(h, usize::MAX);
        assert_eq!(get_file_type(h), 2);
        close_handle(h);
        let path = format!("Z:\\tmp\\winlux-ftype-{}.bin", std::process::id());
        let c = std::ffi::CString::new(path).unwrap();
        let h = unsafe { create_file_a(c.as_ptr() as *const u8, 0xC0000000, 7, 0, 2, 0, 0) };
        assert_eq!(get_file_type(h), 1);
        close_handle(h);
        unsafe { delete_file_a(c.as_ptr() as *const u8) };
    }

    #[test]
    fn position_negative_rend_error_negative_seek() {
        // #1439 : sonde à -1 depuis la fin d'un fichier vide (CRT de Steam).
        init();
        let _teb = crate::process::test_teb();
        let path = format!("Z:\\tmp\\winlux-negseek-{}.bin", std::process::id());
        let c = std::ffi::CString::new(path).unwrap();
        let h = unsafe { create_file_a(c.as_ptr() as *const u8, 0xC0000000, 7, 0, 2, 0x80, 0) };
        assert_ne!(h, usize::MAX);
        let mut pos = 7i64;
        assert_eq!(unsafe { set_file_pointer_ex(h, -1, &mut pos, 2) }, 0);
        assert_eq!(crate::process::last_error(), 131);
        assert_eq!(unsafe { set_file_pointer_ex(h, -1, &mut pos, 0) }, 0);
        assert_eq!(crate::process::last_error(), 131);
        assert_eq!(
            unsafe { set_file_pointer(h, -1, std::ptr::null_mut(), 1) },
            u32::MAX
        );
        assert_eq!(crate::process::last_error(), 131);
        assert_eq!(unsafe { set_file_pointer_ex(h, 0, &mut pos, 2) }, 1);
        assert_eq!(pos, 0);
        close_handle(h);
        unsafe { delete_file_a(c.as_ptr() as *const u8) };
    }

    #[test]
    fn waitable_timer_est_attendable_et_annulable() {
        init();
        let h = unsafe { create_waitable_timer_a(0, 1, std::ptr::null()) };
        assert_ne!(h, 0);
        let due = -200_000i64; // 20 ms
        assert_eq!(unsafe { set_waitable_timer(h, &due, 0, 0, 0, 0) }, 1);
        assert_eq!(wait_for_single_object(h, 0), 258);
        assert_eq!(wait_for_single_object(h, 5_000), 0);
        let periodic = unsafe { create_waitable_timer_a(0, 0, std::ptr::null()) };
        assert_eq!(
            unsafe { set_waitable_timer(periodic, &due, 20, 0, 0, 0) },
            1
        );
        assert_eq!(wait_for_single_object(periodic, 5_000), 0);
        assert_eq!(wait_for_single_object(periodic, 5_000), 0);
        assert_eq!(cancel_waitable_timer(periodic), 1);
        assert_eq!(close_handle(periodic), 1);
        assert_eq!(close_handle(h), 1);
        assert_eq!(cancel_waitable_timer(h), 0);
    }

    #[test]
    fn format_message_from_string_sans_arguments_copie_les_insertions() {
        let _teb = crate::process::test_teb();
        use super::super::kernel32_misc::format_message_a;
        let template = b"Fichier %1 : %2!d! octets%n%%%0ignore\0";
        let mut out = [0u8; 64];
        // Sans arguments : %1 et %2!d! restent tels quels, pas de plantage.
        let n = unsafe {
            format_message_a(
                0x400,
                template.as_ptr() as usize,
                0,
                0,
                out.as_mut_ptr(),
                64,
                0,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..n as usize]).unwrap(),
            "Fichier %1 : %2!d! octets\r\n%"
        );
        let name = b"a.txt\0";
        let args: [usize; 2] = [name.as_ptr() as usize, 42];
        let n = unsafe {
            format_message_a(
                0x400 | 0x2000,
                template.as_ptr() as usize,
                0,
                0,
                out.as_mut_ptr(),
                64,
                args.as_ptr() as usize,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..n as usize]).unwrap(),
            "Fichier a.txt : 42 octets\r\n%"
        );
    }
}

#[cfg(test)]
mod surveillance_de_dossiers {
    use super::*;
    use crate::dlls::dirwatch::{decode, FILE_ACTION_ADDED};
    use std::sync::atomic::AtomicU32;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("winlux-rdcw-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn later(path: std::path::PathBuf) {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::fs::write(path, b"x").unwrap();
        });
    }

    #[test]
    fn synchrone_puis_overlapped_avec_evenement() {
        let dir = temp("sync");
        let h = insert_file(File::open(&dir).unwrap());
        let mut buffer = vec![0u8; 4096];
        let mut bytes = 0u32;
        later(dir.join("un.txt"));
        let ok = unsafe {
            read_directory_changes_w(
                h,
                buffer.as_mut_ptr() as *mut c_void,
                4096,
                0,
                0x1,
                &mut bytes,
                0,
                0,
            )
        };
        assert_eq!(ok, 1);
        let changes = decode(&buffer[..bytes as usize]);
        assert_eq!(changes[0].action, FILE_ACTION_ADDED);
        assert_eq!(changes[0].name, "un.txt");
        // OVERLAPPED + événement : l'appel rend la main tout de suite.
        let event = unsafe { create_event_a(0, 1, 0, std::ptr::null()) };
        let mut ov: Overlapped = unsafe { std::mem::zeroed() };
        ov.event = event;
        let ok = unsafe {
            read_directory_changes_w(
                h,
                buffer.as_mut_ptr() as *mut c_void,
                4096,
                0,
                0x1,
                std::ptr::null_mut(),
                &mut ov as *mut Overlapped as usize,
                0,
            )
        };
        assert_eq!(ok, 1);
        later(dir.join("deux.txt"));
        assert_eq!(wait_for_single_object(event, 5000), 0);
        let mut got = 0u32;
        assert_eq!(unsafe { get_overlapped_result(h, &mut ov, &mut got, 1) }, 1);
        assert_eq!(decode(&buffer[..got as usize])[0].name, "deux.txt");
        // Fermer le dossier abandonne une attente en cours.
        let ok = unsafe {
            read_directory_changes_w(
                h,
                buffer.as_mut_ptr() as *mut c_void,
                4096,
                0,
                0x1,
                std::ptr::null_mut(),
                &mut ov as *mut Overlapped as usize,
                0,
            )
        };
        assert_eq!(ok, 1);
        assert_eq!(close_handle(h), 1);
        assert_eq!(wait_for_single_object(event, 5000), 0);
        assert_eq!(unsafe { get_overlapped_result(h, &mut ov, &mut got, 1) }, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    static ROUTINE_BYTES: AtomicU32 = AtomicU32::new(0);
    extern "win64" fn routine(error: u32, bytes: u32, _ov: usize) {
        ROUTINE_BYTES.store(if error == 0 { bytes } else { u32::MAX }, Ordering::SeqCst);
    }

    #[test]
    fn routine_d_achevement_pendant_sleepex() {
        let dir = temp("apc");
        let h = insert_file(File::open(&dir).unwrap());
        let mut buffer = vec![0u8; 4096];
        let mut ov: Overlapped = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            read_directory_changes_w(
                h,
                buffer.as_mut_ptr() as *mut c_void,
                4096,
                1,
                0x17f,
                std::ptr::null_mut(),
                &mut ov as *mut Overlapped as usize,
                routine as *const () as usize,
            )
        };
        assert_eq!(ok, 1);
        later(dir.join("trois.txt"));
        // Attente alertable : la routine tourne sur ce thread, SleepEx rend 0xC0.
        assert_eq!(unsafe { sleep_ex(5000, 1) }, 0xC0);
        let bytes = ROUTINE_BYTES.load(Ordering::SeqCst);
        assert!(bytes > 0 && bytes != u32::MAX);
        assert_eq!(decode(&buffer[..bytes as usize])[0].name, "trois.txt");
        // Sans APC, une attente alertable expire normalement.
        assert_eq!(unsafe { sleep_ex(30, 1) }, 0);
        close_handle(h);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_first_change_notification() {
        let dir = temp("ffcn");
        let path: Vec<u16> = dir
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let h = unsafe { find_first_change_notification_w(path.as_ptr(), 0, 0x1) };
        assert_ne!(h, usize::MAX);
        assert_eq!(wait_for_single_object(h, 200), 258);
        later(dir.join("quatre.txt"));
        assert_eq!(wait_for_single_object(h, 5000), 0);
        assert_eq!(find_next_change_notification(h), 1);
        assert_eq!(wait_for_single_object(h, 200), 258);
        assert_eq!(find_close_change_notification(h), 1);
        let missing: Vec<u16> = "/nulle/part".encode_utf16().chain(Some(0)).collect();
        assert_eq!(
            unsafe { find_first_change_notification_w(missing.as_ptr(), 0, 0x1) },
            usize::MAX
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod pile_virtual_query {
    #[test]
    fn la_pile_a_une_page_de_garde_comme_sous_windows() {
        let (limit, base) = (0x10_0000usize, 0x20_0000usize);
        let bas = super::stack_region(limit + 10, limit, base);
        assert_eq!((bas.state, bas.allocation_base as usize), (0x2000, limit));
        let garde = super::stack_region(limit + 4096, limit, base);
        assert_eq!(garde.state, 0x1000);
        assert_eq!(garde.protect, 0x104, "PAGE_READWRITE | PAGE_GUARD");
        assert_eq!(garde.region_size, 4096);
        let haut = super::stack_region(base - 8, limit, base);
        assert_eq!((haut.state, haut.protect), (0x1000, 4));
        assert_eq!(haut.base_address as usize + haut.region_size, base);
        assert_eq!(haut.allocation_base as usize, limit);
    }
}

/// Attend (au plus 30 s) que le processus `pid` soit arrêté par SIGSTOP,
/// ou qu'il ait disparu. Lit l'état dans /proc/<pid>/stat sans le récolter.
fn wait_until_stopped(pid: u32) {
    let path = format!("/proc/{pid}/stat");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let state = std::fs::read_to_string(&path).ok().and_then(|t| {
            t.rsplit_once(')')
                .and_then(|(_, r)| r.trim_start().chars().next())
        });
        match state {
            Some('T') | Some('t') | Some('Z') | Some('X') | None => return,
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            crate::trace!("CreateProcess suspendu : l'enfant {pid} ne s'est pas figé");
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[cfg(test)]
mod interrupt_time_tests {
    use super::{query_performance_counter, query_performance_frequency};
    #[test]
    fn qpc_a_la_meme_origine_que_ntdll_et_le_systeme() {
        // #1886 : QueryPerformanceCounter et RtlQueryPerformanceCounter
        // concordent, et l'origine est celle du système (CLOCK_MONOTONIC),
        // commune à tous les processus, pas le démarrage du processus.
        let (mut freq, mut a, mut rtl, mut b) = (0i64, 0i64, 0i64, 0i64);
        assert_eq!(query_performance_frequency(&mut freq), 1);
        assert_eq!(freq, 1_000_000_000);
        assert_eq!(query_performance_counter(&mut a), 1);
        unsafe { crate::dlls::ntdll::rtl_query_performance_counter(&mut rtl) };
        assert_eq!(query_performance_counter(&mut b), 1);
        assert!(a <= rtl && rtl <= b, "{a} {rtl} {b}");
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        let sys = ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64;
        assert!(b <= sys && sys - b < 1_000_000_000, "{b} {sys}");
    }

    #[test]
    fn query_unbiased_interrupt_time_avance_en_100_ns() {
        let (mut a, mut b) = (0u64, 0u64);
        assert_eq!(super::query_unbiased_interrupt_time(&mut a), 1);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(super::query_unbiased_interrupt_time(&mut b), 1);
        assert!(b - a >= 150_000 && b - a < 50_000_000, "{a} {b}");
        assert_eq!(
            super::query_unbiased_interrupt_time(std::ptr::null_mut()),
            0
        );
    }
}

#[cfg(test)]
mod tests_downlevel {
    /// Acrobat (#1818) : LoadLibraryEx d'un contrat « downlevel » avec
    /// LOAD_LIBRARY_SEARCH_SYSTEM32, puis GetProcAddress.
    #[test]
    fn contrat_downlevel_charge_avec_search_system32() {
        unsafe {
            let h = super::load_library_ex_a(
                c"api-ms-win-downlevel-shell32-l1-1-0.dll".as_ptr().cast(),
                0,
                0x800,
            );
            assert_ne!(h, 0);
            for f in [
                c"CommandLineToArgvW",
                c"SHGetKnownFolderPath",
                c"SHGetFolderPathW",
                c"ShellExecuteExW",
            ] {
                assert_ne!(super::get_proc_address(h, f.as_ptr().cast()), 0, "{f:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests_processus_zombie {
    use super::*;

    #[test]
    fn processus_etranger_fini_mais_non_reclame_est_signale() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id() as i32;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !os_process_zombie(pid) {
            assert!(std::time::Instant::now() < deadline, "jamais zombie");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Zombie : encore « vivant » pour kill(0), mais fini pour Windows.
        assert!(os_process_alive(pid));
        let h = unsafe { open_process(0x0010_0000, 0, pid as u32) };
        assert_ne!(h, 0);
        assert_eq!(wait_for_single_object(h, 2000), 0);
        assert!(!os_process_zombie(std::process::id() as i32));
        unsafe { close_handle(h) };
        child.wait().unwrap();
    }
}
