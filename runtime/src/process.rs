//! Environnement de processus Windows : TEB, PEB, registre GS, TLS
//! statique, initialisation des modules et lancement du point d'entrée.
//!
//! Disposition des structures d'après la documentation publique (NT_TIB,
//! TEB/PEB documentés dans winternl.h) et nos propres tests.

use crate::loader::{self, Module};
use crate::trace;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

pub const TEB_SIZE: usize = 0x2000;
pub const TEB_STACK_BASE: usize = 0x08;
pub const TEB_STACK_LIMIT: usize = 0x10;
pub const TEB_SELF: usize = 0x30;
pub const TEB_PID: usize = 0x40;
pub const TEB_TID: usize = 0x48;
pub const TEB_TLS_POINTER: usize = 0x58;
pub const TEB_PEB: usize = 0x60;
pub const TEB_LAST_ERROR: usize = 0x68;
pub const TEB_TLS_SLOTS: usize = 0x1480;
pub const TLS_SLOTS: usize = 64;
pub const TEB_TLS_EXPANSION_POINTER: usize = 0x1780;
pub const TLS_EXPANSION_SLOTS: usize = 1024;

static THREAD_TEBS: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();

pub const PEB_BEING_DEBUGGED: usize = 0x02;
pub const PEB_IMAGE_BASE: usize = 0x10;
pub const PEB_PROCESS_PARAMETERS: usize = 0x20;
pub const PEB_PROCESS_HEAP: usize = 0x30;

#[allow(dead_code)]
pub const DLL_PROCESS_DETACH: u32 = 0;
pub const DLL_PROCESS_ATTACH: u32 = 1;
pub const DLL_THREAD_ATTACH: u32 = 2;
pub const DLL_THREAD_DETACH: u32 = 3;

static PEB: AtomicUsize = AtomicUsize::new(0);

/// Adresse fixe du PEB dans chaque processus Windows de Winlux : un autre
/// processus (bac à sable de Chromium, crashpad) la reçoit de
/// NtQueryInformationProcess et y lit l'image du programme.
pub const PEB_FIXED: usize = 0x5FFE_0000_0000;

/// Variable posée par CreateProcess(CREATE_SUSPENDED) : le processus se
/// fige (SIGSTOP) une fois chargé, avant tout code du programme.
pub const SUSPENDED_ENV: &str = "WINLUX_SUSPENDU";

/// PEB à son adresse fixe, ou ailleurs si elle est prise.
fn alloc_peb() -> usize {
    unsafe {
        let p = libc::mmap(
            PEB_FIXED as *mut libc::c_void,
            0x1000,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED_NOREPLACE,
            -1,
            0,
        );
        if p != libc::MAP_FAILED && p as usize == PEB_FIXED {
            return PEB_FIXED;
        }
    }
    alloc_zeroed(0x1000)
}

/// CREATE_SUSPENDED : se figer jusqu'au ResumeThread du parent.
pub fn stop_if_suspended() {
    if std::env::var_os(SUSPENDED_ENV).is_some() {
        std::env::remove_var(SUSPENDED_ENV);
        trace!("processus créé suspendu : attente de ResumeThread");
        unsafe { libc::raise(libc::SIGSTOP) };
    }
}

pub struct ProcessInfo {
    pub command_line_w: Vec<u16>,
    pub command_line_a: Vec<u8>,
}

pub static PROCESS: OnceLock<ProcessInfo> = OnceLock::new();

pub fn peb() -> usize {
    PEB.load(Ordering::Relaxed)
}

/// Adresse du TEB du thread courant (lue via GS, comme le ferait un programme Windows).
#[inline]
pub fn teb() -> usize {
    // Ne pas lire GS:[0x30] avant l'initialisation du TEB : le processus hôte
    // Linux a généralement un GS nul, et une lecture absolue à 0x30 segfault.
    // ARCH_GET_GS vérifie d'abord qu'un TEB Winlux a effectivement été installé.
    let mut base = 0usize;
    let rc = unsafe { libc::syscall(libc::SYS_arch_prctl, 0x1004, &mut base as *mut usize) };
    if rc != 0 {
        return 0;
    }
    base
}

pub fn set_last_error(code: u32) {
    let t = teb();
    if t != 0 {
        unsafe { *((t + TEB_LAST_ERROR) as *mut u32) = code };
    }
}

pub fn last_error() -> u32 {
    let t = teb();
    if t == 0 {
        return 0;
    }
    unsafe { *((t + TEB_LAST_ERROR) as *const u32) }
}

/// Per-thread minimal TEB for ABI regression tests. Restore GS before freeing
/// its backing memory, including when an assertion unwinds.
#[cfg(test)]
pub struct TestTeb {
    previous: usize,
    _bytes: Box<[u8; 4096]>,
}
#[cfg(test)]
impl Drop for TestTeb {
    fn drop(&mut self) {
        assert_eq!(
            unsafe { libc::syscall(libc::SYS_arch_prctl, 0x1001, self.previous) },
            0
        );
    }
}
#[cfg(test)]
pub fn test_teb() -> TestTeb {
    let previous = teb();
    let mut bytes = Box::new([0u8; 4096]);
    let base = bytes.as_mut_ptr() as usize;
    unsafe {
        *((base + 0x30) as *mut usize) = base;
        assert_eq!(libc::syscall(libc::SYS_arch_prctl, 0x1001, base), 0);
    }
    TestTeb {
        previous,
        _bytes: bytes,
    }
}

fn alloc_zeroed(size: usize) -> usize {
    unsafe {
        let p = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        assert!(p != libc::MAP_FAILED);
        p as usize
    }
}

/// Construit la ligne de commande Windows à partir des arguments (règles de guillemets de CommandLineToArgvW).
pub fn quote_arg(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '\t', '"']) {
        return a.to_string();
    }
    let mut s = String::from("\"");
    let mut bs = 0;
    for c in a.chars() {
        match c {
            '\\' => bs += 1,
            '"' => {
                s.extend(std::iter::repeat_n('\\', bs * 2 + 1));
                s.push('"');
                bs = 0;
            }
            _ => {
                s.extend(std::iter::repeat_n('\\', bs));
                s.push(c);
                bs = 0;
            }
        }
    }
    s.extend(std::iter::repeat_n('\\', bs * 2));
    s.push('"');
    s
}

#[cfg(test)]
mod tests {
    use super::quote_arg;

    #[test]
    fn quote_les_arguments_necessitant_des_guillemets() {
        assert_eq!(quote_arg("simple"), "simple");
        assert_eq!(quote_arg("deux mots"), "\"deux mots\"");
        assert_eq!(quote_arg("a b\\"), "\"a b\\\\\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn parent_de_chromium_vivant_ou_mort() {
        assert!(super::stat_alive("42 (claude.exe) S 1 42"));
        assert!(super::stat_alive("42 (a) b) R 1"));
        assert!(!super::stat_alive("42 (claude.exe) Z 1 42"));
        assert!(!super::stat_alive("42 (claude.exe) X 1"));
        assert!(super::process_alive(std::process::id() as i32));
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id() as i32;
        // Mort mais pas encore ramassé : zombie, donc plus vivant.
        let mut alive = true;
        for _ in 0..50 {
            alive = super::process_alive(pid);
            if !alive {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!alive);
        child.wait().unwrap();
        assert!(!super::process_alive(pid));
    }
}

fn unicode_string(at: usize, s: &[u16]) {
    let buf = Box::leak(
        s.iter()
            .copied()
            .chain([0])
            .collect::<Vec<u16>>()
            .into_boxed_slice(),
    );
    unsafe {
        *(at as *mut u16) = (s.len() * 2) as u16;
        *((at + 2) as *mut u16) = (s.len() * 2 + 2) as u16;
        *((at + 8) as *mut usize) = buf.as_ptr() as usize;
    }
}

/// Processus enfant de Chromium (moteur de rendu, GPU, réseau, crashpad) :
/// sous Windows il s'arrête quand son application disparaît. Ici, quand le processus parent meurt (même
/// tué), l'enfant est rattaché à un autre parent : il s'arrête alors aussi,
/// au lieu de rester seul en mémoire (450 Mo après un plantage de Claude).
fn exit_with_parent(windows_parent: Option<i32>) {
    let parent = unsafe { libc::getppid() };
    if parent <= 1 && windows_parent.is_none() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("winlux-parent-watch".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let ppid = unsafe { libc::getppid() };
            let gone = (parent > 1 && (ppid != parent || !process_alive(parent)))
                || windows_parent.is_some_and(|p| !process_alive(p));
            if gone {
                crate::trace!("parent {parent} disparu : arrêt de l'enfant de Chromium");
                unsafe { libc::_exit(0) };
            }
        });
}

/// Le processus `pid` tourne-t-il encore ? Un zombie (Z) ou un processus
/// mort (X) ne compte plus : son navigateur est arrêté même s'il n'a pas
/// encore été ramassé.
fn process_alive(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat_alive(&stat),
        Err(_) => false,
    }
}

fn stat_alive(stat: &str) -> bool {
    // « pid (nom) état … » : le nom peut contenir des parenthèses.
    let state = stat
        .rfind(')')
        .and_then(|i| stat[i + 1..].trim_start().chars().next());
    !matches!(state, Some('Z' | 'X' | 'x') | None)
}

pub fn init_process(exe: &Module, args: &[String]) {
    // Identité de l'application pour ses autorisations (caméra, micro…).
    crate::permissions::init(&exe.path, args);
    let exe_win = crate::paths::to_windows(&exe.path);
    // Processus Windows qui nous a lancés (avant que la variable ne prenne
    // notre propre numéro pour nos enfants).
    let windows_parent = std::env::var("WINLUX_PARENT_PID")
        .ok()
        .and_then(|p| p.parse::<i32>().ok())
        .filter(|&p| p > 1 && p != std::process::id() as i32);
    crate::permissions::ask_files_before_start(&exe_win);
    // Enfants de Chromium/Electron (--type=gpu-process, renderer, utility,
    // crashpad-handler…) : Windows les tue avec leur navigateur (objet Job
    // KILL_ON_JOB_CLOSE), ils s'arrêtent donc avec leur parent.
    let chromium_child = args.iter().any(|a| a.starts_with("--type="))
        || std::env::var("WINLUX_COMMAND_LINE").is_ok_and(|l| l.contains(" --type="));
    if chromium_child {
        exit_with_parent(windows_parent);
    }
    let mut all = vec![exe_win.clone()];
    all.extend(args.iter().cloned());
    let mut cmd = all
        .iter()
        .map(|a| quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ");
    // Ligne exacte transmise par un parent Windows (CreateProcess) : elle
    // garde son premier mot, que certains programmes lisent (argv[0]).
    if let Some(line) = std::env::var("WINLUX_COMMAND_LINE")
        .ok()
        .filter(|l| !l.trim().is_empty())
    {
        std::env::remove_var("WINLUX_COMMAND_LINE");
        cmd = line;
    }
    if !chromium_child && chromium_sandbox_refused(&exe_win, &cmd) {
        cmd.push_str(" --no-sandbox");
    }
    // Bac à sable refusé en cours de route (appli non reconnue au départ,
    // #236) : l'appli repart sans, comme l'utilisateur l'a accepté.
    crate::sandbox_fallback::apply_at_start(&mut cmd, chromium_child);
    crate::launch::provide_git_bash();
    let cmd_w: Vec<u16> = cmd.encode_utf16().collect();
    let mut cmd_a = crate::dlls::encode_ansi(&cmd);
    cmd_a.push(0);

    // SharedUserData!SystemCall avant tout code du programme : les relais
    // Nt* de ntdll en dépendent (#767).
    crate::dlls::pe_image::map_shared_user_data();
    let peb = alloc_peb();
    let params = alloc_zeroed(0x1000);
    unsafe {
        *((peb + PEB_IMAGE_BASE) as *mut usize) = exe.base;
        *((peb + PEB_PROCESS_PARAMETERS) as *mut usize) = params;
        *((peb + PEB_PROCESS_HEAP) as *mut usize) = crate::dlls::kernel32::process_heap();
        *((peb + PEB_BEING_DEBUGGED) as *mut u8) = 0;
        // OSMajorVersion / OSMinorVersion / OSBuildNumber : Windows 10 (10.0.19045).
        // LoaderLock (RTL_CRITICAL_SECTION libre) : du code lit son OwningThread
        // pour savoir s'il tourne sous le verrou du chargeur (Warzone 2100).
        let loader_lock = alloc_zeroed(64);
        *((loader_lock + 8) as *mut i32) = -1; // LockCount (OwningThread à +0x10 reste 0)
        *((peb + 0x110) as *mut usize) = loader_lock;
        *((peb + 0x118) as *mut u32) = 10;
        *((peb + 0x11c) as *mut u32) = 0;
        *((peb + 0x120) as *mut u16) = 19045;
        *((peb + 0x124) as *mut u32) = 2; // VER_PLATFORM_WIN32_NT
    }
    let path_w: Vec<u16> = exe_win.encode_utf16().collect();
    // StandardInput/Output/Error de RTL_USER_PROCESS_PARAMETERS : lus
    // directement par certains runtimes (Bun) au lieu de GetStdHandle.
    unsafe {
        *((params + 0x20) as *mut usize) = crate::dlls::kernel32::STD_INPUT_REAL;
        *((params + 0x28) as *mut usize) = crate::dlls::kernel32::STD_OUTPUT_REAL;
        *((params + 0x30) as *mut usize) = crate::dlls::kernel32::STD_ERROR_REAL;
    }
    unicode_string(params + 0x60, &path_w);
    unicode_string(params + 0x70, &cmd_w);
    let mut command_line_w = cmd_w;
    command_line_w.push(0);
    PEB.store(peb, Ordering::Relaxed);
    if crate::wow32::active() {
        crate::wow32::init_peb32(exe.base);
    }
    let _ = PROCESS.set(ProcessInfo {
        command_line_w,
        command_line_a: cmd_a,
    });
    // Variables _acmdln/_wcmdln de la CRT, lues directement par les programmes.
    crate::dlls::crt_extra::process_started();
    crate::wow32::fill_crt_data();
}

/// Crée le TEB du thread courant et fait pointer GS dessus.
pub fn init_thread() -> usize {
    let teb = alloc_zeroed(TEB_SIZE);
    unsafe {
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        let (mut addr, mut size) = (std::ptr::null_mut(), 0usize);
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) == 0 {
            libc::pthread_attr_getstack(&attr, &mut addr, &mut size);
            libc::pthread_attr_destroy(&mut attr);
        }
        *((teb + TEB_STACK_BASE) as *mut usize) = addr as usize + size;
        *((teb + TEB_STACK_LIMIT) as *mut usize) = addr as usize;
        *((teb + TEB_SELF) as *mut usize) = teb;
        *((teb + TEB_PID) as *mut usize) = libc::getpid() as usize;
        *((teb + TEB_TID) as *mut usize) = libc::gettid() as usize;
        *((teb + TEB_PEB) as *mut usize) = peb();
        // ARCH_SET_GS : sous Linux x86-64, glibc utilise FS pour son TLS ; GS est libre.
        if libc::syscall(libc::SYS_arch_prctl, 0x1001, teb) != 0 {
            panic!("arch_prctl(ARCH_SET_GS) a échoué");
        }
    }
    setup_static_tls(teb);
    install_signal_stack();
    // Programme x86 : TEB 32 bits sur FS (#232).
    crate::wow32::init_thread(teb);
    let mut threads = THREAD_TEBS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !threads.contains(&teb) {
        threads.push(teb);
    }
    teb
}

const SIGNAL_STACK_SIZE: usize = 64 * 1024;

/// Pile de signal alternative du thread courant : le gestionnaire de
/// plantage (SA_ONSTACK) doit pouvoir s'exécuter même quand la pile du
/// thread Windows est épuisée, sur chaque thread et pas seulement le premier.
fn install_signal_stack() {
    unsafe {
        let mut current: libc::stack_t = std::mem::zeroed();
        if libc::sigaltstack(std::ptr::null(), &mut current) == 0
            && current.ss_flags & libc::SS_DISABLE == 0
            && current.ss_size != 0
        {
            // Pile déjà installée (par la bibliothèque standard de Rust sur
            // ses threads, par exemple) : elle appartient à son auteur.
            return;
        }
        let stack = libc::mmap(
            std::ptr::null_mut(),
            SIGNAL_STACK_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        if stack == libc::MAP_FAILED {
            return;
        }
        let ss = libc::stack_t {
            ss_sp: stack,
            ss_flags: 0,
            ss_size: SIGNAL_STACK_SIZE,
        };
        if libc::sigaltstack(&ss, std::ptr::null_mut()) != 0 {
            libc::munmap(stack, SIGNAL_STACK_SIZE);
            return;
        }
        OWN_SIGNAL_STACK.with(|own| own.set(stack as usize));
    }
}

thread_local! {
    /// Pile de signal installée par `install_signal_stack` sur ce thread
    /// (0 si c'est quelqu'un d'autre qui l'a installée). Seule celle-ci est
    /// libérée à la fin du thread : la bibliothèque standard de Rust libère
    /// elle-même la sienne, et un second `munmap` de la même plage aurait
    /// pu retirer une page déjà réutilisée (tas, TEB d'un autre thread).
    static OWN_SIGNAL_STACK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Retire la pile de signal du thread courant et la libère, seulement si
/// c'est `install_signal_stack` qui l'a posée.
fn remove_signal_stack() {
    let own = OWN_SIGNAL_STACK.with(|own| own.replace(0));
    if own == 0 {
        return;
    }
    unsafe {
        let mut current: libc::stack_t = std::mem::zeroed();
        if libc::sigaltstack(std::ptr::null(), &mut current) != 0
            || current.ss_flags & libc::SS_ONSTACK != 0
            || current.ss_sp as usize != own
        {
            return;
        }
        let off = libc::stack_t {
            ss_sp: std::ptr::null_mut(),
            ss_flags: libc::SS_DISABLE,
            ss_size: 0,
        };
        if libc::sigaltstack(&off, std::ptr::null_mut()) == 0 {
            libc::munmap(own as *mut libc::c_void, SIGNAL_STACK_SIZE);
        }
    }
}

/// Libère le TEB du thread courant (blocs TLS statiques, tableau
/// d'expansion, pile de signal) et détache GS : plus aucun code Windows ne
/// tourne sur ce thread.
fn free_current_teb() {
    let t = teb();
    if t == 0 {
        return;
    }
    {
        let mut threads = THREAD_TEBS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        threads.retain(|&x| x != t);
    }
    // Programme x86 : TEB 32 bits et entrée de LDT du thread (#232).
    crate::wow32::free_thread(t);
    unsafe {
        // GS d'abord : un gestionnaire de signal ne doit pas lire un TEB libéré.
        libc::syscall(libc::SYS_arch_prctl, 0x1001, 0usize);
        let array = *((t + TEB_TLS_POINTER) as *const usize);
        if array != 0 {
            let mods = loader::MODULES.lock().unwrap().clone();
            for t in mods.iter().filter_map(|m| m.tls.as_ref()) {
                if t.slot >= loader::TLS_MODULE_SLOTS {
                    continue;
                }
                let block = *((array + t.slot * 8) as *const usize);
                if block != 0 {
                    libc::munmap(block as *mut libc::c_void, tls_block_size(t));
                }
            }
            libc::munmap(array as *mut libc::c_void, 4096);
        }
        let expansion = *((t + TEB_TLS_EXPANSION_POINTER) as *const usize);
        if expansion != 0 {
            libc::munmap(expansion as *mut libc::c_void, TLS_EXPANSION_SLOTS * 8);
        }
        libc::munmap(t as *mut libc::c_void, TEB_SIZE);
    }
    remove_signal_stack();
}

/// Lit une valeur TLS Win32 du TEB courant.
pub fn tls_value(index: u32) -> Option<usize> {
    let t = teb();
    if t == 0 {
        return None;
    }
    let i = index as usize;
    unsafe {
        if i < TLS_SLOTS {
            Some(*((t + TEB_TLS_SLOTS + i * 8) as *const usize))
        } else if i < TLS_SLOTS + TLS_EXPANSION_SLOTS {
            let p = *((t + TEB_TLS_EXPANSION_POINTER) as *const usize);
            if p == 0 {
                Some(0)
            } else {
                Some(*((p + (i - TLS_SLOTS) * 8) as *const usize))
            }
        } else {
            None
        }
    }
}

/// Écrit une valeur TLS Win32 du TEB courant, allouant le tableau d'expansion si nécessaire.
pub fn set_tls_value(index: u32, value: usize) -> bool {
    let t = teb();
    if t == 0 {
        return false;
    }
    let i = index as usize;
    unsafe {
        if i < TLS_SLOTS {
            *((t + TEB_TLS_SLOTS + i * 8) as *mut usize) = value;
            true
        } else if i < TLS_SLOTS + TLS_EXPANSION_SLOTS {
            let slot = t + TEB_TLS_EXPANSION_POINTER;
            let mut p = *(slot as *const usize);
            if p == 0 {
                p = alloc_zeroed(TLS_EXPANSION_SLOTS * 8);
                *(slot as *mut usize) = p;
            }
            *((p + (i - TLS_SLOTS) * 8) as *mut usize) = value;
            true
        } else {
            false
        }
    }
}

/// `TlsFree` vide l'emplacement sur les TEB de tous les threads connus avant réutilisation.
pub fn clear_tls_value_all_threads(index: u32) -> bool {
    let i = index as usize;
    if i >= TLS_SLOTS + TLS_EXPANSION_SLOTS {
        return false;
    }
    let threads = THREAD_TEBS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    unsafe {
        for &t in threads.iter() {
            if i < TLS_SLOTS {
                *((t + TEB_TLS_SLOTS + i * 8) as *mut usize) = 0;
            } else {
                let p = *((t + TEB_TLS_EXPANSION_POINTER) as *const usize);
                if p != 0 {
                    *((p + (i - TLS_SLOTS) * 8) as *mut usize) = 0;
                }
            }
        }
    }
    true
}

/// TLS statique (__declspec(thread)) : un bloc par module, copié depuis l'image.
fn setup_static_tls(teb: usize) {
    let mods = loader::MODULES.lock().unwrap().clone();
    let array = alloc_zeroed(4096);
    for m in mods.iter() {
        if let Some(t) = &m.tls {
            let idx = t.slot;
            if idx >= loader::TLS_MODULE_SLOTS {
                continue;
            }
            let raw = t.raw_end.saturating_sub(t.raw_start);
            let block = alloc_zeroed(tls_block_size(t));
            unsafe {
                std::ptr::copy_nonoverlapping(t.raw_start as *const u8, block as *mut u8, raw);
                *((array + idx * 8) as *mut usize) = block;
                if t.index_addr != 0 {
                    *(t.index_addr as *mut u32) = idx as u32;
                }
            }
        }
    }
    unsafe { *((teb + TEB_TLS_POINTER) as *mut usize) = array };
}

fn run_tls_callbacks(m: &Module, reason: u32) {
    let Some(t) = &m.tls else { return };
    if t.callbacks_addr == 0 {
        return;
    }
    if m.pe.x86 {
        let mut p = t.callbacks_addr as u32;
        while crate::wow32::rd(p) != 0 {
            let cb = crate::wow32::rd(p);
            trace!("rappel TLS x86 0x{cb:x} de {}", m.name);
            crate::wow32::call32(cb as usize, &[m.base as u32, reason, 0]);
            p += 4;
        }
        return;
    }
    let mut p = t.callbacks_addr as *const usize;
    unsafe {
        while *p != 0 {
            trace!("rappel TLS 0x{:x} de {}", *p, m.name);
            let cb: extern "win64" fn(usize, u32, usize) = std::mem::transmute(*p);
            cb(m.base, reason, 0);
            p = p.add(1);
        }
    }
}

/// Taille d'un bloc TLS statique (données, zéros, marge).
fn tls_block_size(t: &loader::TlsInfo) -> usize {
    (t.raw_end.saturating_sub(t.raw_start) + t.zero_fill + 16).max(16)
}

/// Déchargement d'une DLL (#24) : ses rappels TLS reçoivent
/// DLL_PROCESS_DETACH, son bloc TLS est libéré dans chaque thread et son
/// index TLS redevient libre pour une prochaine DLL.
pub fn release_module_tls(m: &Module) {
    NO_THREAD_CALLS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|&b| b != m.base);
    let Some(tls) = &m.tls else { return };
    run_tls_callbacks(m, 0);
    if tls.slot >= loader::TLS_MODULE_SLOTS {
        return;
    }
    let tebs = THREAD_TEBS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for t in &tebs {
        unsafe {
            let array = *((*t + TEB_TLS_POINTER) as *const usize);
            if array == 0 {
                continue;
            }
            let slot = (array + tls.slot * 8) as *mut usize;
            if *slot != 0 {
                libc::munmap(*slot as *mut libc::c_void, tls_block_size(tls));
                *slot = 0;
            }
            crate::wow32::mirror_tls_slot(*t, tls.slot, 0);
        }
    }
    loader::free_tls_slot(tls.slot);
}

/// Alloue et copie un bloc de données TLS statique pour un module.
unsafe fn alloc_tls_block(tls: &loader::TlsInfo) -> usize {
    let raw = tls.raw_end.saturating_sub(tls.raw_start);
    let block = alloc_zeroed((raw + tls.zero_fill + 16).max(16));
    std::ptr::copy_nonoverlapping(tls.raw_start as *const u8, block as *mut u8, raw);
    block
}

/// Termine l'initialisation Win32 d'une DLL chargée explicitement par LoadLibrary.
pub fn initialize_dynamic_module(m: &Module) -> bool {
    let tls_context = if let Some(tls) = &m.tls {
        let thread_teb = teb();
        if thread_teb == 0 {
            return false;
        }
        let index = tls.slot;
        // Le tableau du TEB fait 512 entrées (4 KiB); refuser explicitement
        // plutôt que d'écrire hors limites si l'image dépasse cette capacité.
        if index >= loader::TLS_MODULE_SLOTS {
            return false;
        }
        let array = unsafe { *((thread_teb + TEB_TLS_POINTER) as *const usize) };
        if array == 0 {
            return false;
        }
        Some((tls.clone(), array, index))
    } else {
        None
    };
    if !loader::mark_initialized(m.base) {
        return true;
    }
    if let Some((tls, _array, index)) = tls_context {
        let tebs = THREAD_TEBS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for t in &tebs {
            let array = unsafe { *((*t + TEB_TLS_POINTER) as *const usize) };
            if array == 0 {
                continue;
            }
            unsafe {
                let block = alloc_tls_block(&tls);
                *((array + index * 8) as *mut usize) = block;
                crate::wow32::mirror_tls_slot(*t, index, block);
            }
        }
        if tls.index_addr != 0 {
            unsafe {
                *(tls.index_addr as *mut u32) = index as u32;
            }
        }
    }
    run_tls_callbacks(m, DLL_PROCESS_ATTACH);
    if let Some(entry) = m.entry() {
        trace!("DllMain de {} (chargement dynamique)", m.name);
        let dllmain =
            |base: usize, reason: u32, reserved: usize| call_dllmain(entry, base, reason, reserved);
        if dllmain(m.base, DLL_PROCESS_ATTACH, 0) == 0 {
            return false;
        }
    }
    true
}

/// Modules qui ont appelé `DisableThreadLibraryCalls` : leur DllMain ne
/// reçoit plus DLL_THREAD_ATTACH/DETACH (les rappels TLS, si).
static NO_THREAD_CALLS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

/// `DisableThreadLibraryCalls(hModule)`.
pub fn disable_thread_library_calls(module: usize) -> bool {
    if !loader::MODULES
        .lock()
        .unwrap()
        .iter()
        .any(|m| m.base == module)
    {
        return false;
    }
    let mut list = NO_THREAD_CALLS.lock().unwrap_or_else(|e| e.into_inner());
    if !list.contains(&module) {
        list.push(module);
    }
    true
}

fn thread_calls_enabled(module: usize) -> bool {
    !NO_THREAD_CALLS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&module)
}

pub fn thread_attach_current() {
    init_thread();
    let mods = loader::MODULES.lock().unwrap().clone();
    for m in mods.into_iter().skip(1) {
        run_tls_callbacks(&m, DLL_THREAD_ATTACH);
        if let Some(e) = m.entry().filter(|_| thread_calls_enabled(m.base)) {
            let dllmain =
                |base: usize, reason: u32, reserved: usize| call_dllmain(e, base, reason, reserved);
            let _ = dllmain(m.base, DLL_THREAD_ATTACH, 0);
        }
    }
}

/// Fin d'un thread Windows : rappels DLL_THREAD_DETACH puis libération du
/// TEB (et de ses blocs TLS) créé par `thread_attach_current`.
pub fn thread_detach_current() {
    if teb() == 0 {
        return;
    }
    let mods = loader::MODULES.lock().unwrap().clone();
    for m in mods.into_iter().skip(1).rev() {
        run_tls_callbacks(&m, DLL_THREAD_DETACH);
        if let Some(e) = m.entry().filter(|_| thread_calls_enabled(m.base)) {
            let dllmain =
                |base: usize, reason: u32, reserved: usize| call_dllmain(e, base, reason, reserved);
            let _ = dllmain(m.base, DLL_THREAD_DETACH, 0);
        }
    }
    free_current_teb();
}

/// Initialise les DLL réelles (rappels TLS + DllMain) puis lance le programme.
fn start(exe: Module) -> u32 {
    init_thread();
    crate::crash::install();
    crate::quality::init();
    crate::wow32::wrap_fault_handlers();
    let mods = loader::MODULES.lock().unwrap().clone();
    // Les dépendances ont été chargées après le .exe : on initialise en ordre inverse.
    for m in mods.iter().skip(1).rev() {
        if !loader::mark_initialized(m.base) {
            continue;
        }
        run_tls_callbacks(m, DLL_PROCESS_ATTACH);
        if let Some(e) = m.entry() {
            trace!("DllMain de {}", m.name);
            let dllmain =
                |base: usize, reason: u32, reserved: usize| call_dllmain(e, base, reason, reserved);
            if dllmain(m.base, DLL_PROCESS_ATTACH, 1) == 0 {
                eprintln!(
                    "winlux: l'initialisation de {} a échoué (LastError={})",
                    m.name,
                    last_error()
                );
                return 0xC000_0142;
            }
        }
    }
    loader::mark_initialized(exe.base);
    run_tls_callbacks(&exe, DLL_PROCESS_ATTACH);
    let entry = exe.entry().expect("le programme n'a pas de point d'entrée");
    trace!("saut vers le point d'entrée 0x{entry:x}");
    if crate::wow32::active() {
        return crate::wow32::call32(entry, &[]) as u32;
    }
    // Thread principal : objet de thread Win32 (OpenThread, suspension par
    // le ramasse-miettes de .NET) et racine de pile Windows.
    let _ = crate::dlls::kernel32::current_thread_object();
    crate::dlls::thread_ctl::root_call(entry, 0)
}

/// Action personnalisée DLL de Windows Installer (#833) : la DLL est le
/// module principal ; ses dépendances puis elle-même sont initialisées
/// (rappels TLS, DllMain), puis `UINT Entree(MSIHANDLE)` est appelée.
pub fn run_dll_export(dll: Module, export: usize, arg: u32) -> ! {
    let h = std::thread::Builder::new()
        .name("win-main".into())
        .stack_size(8 << 20)
        .spawn(move || {
            init_thread();
            crate::crash::install();
            crate::wow32::wrap_fault_handlers();
            let mods = loader::MODULES.lock().unwrap().clone();
            for m in mods.iter().rev() {
                if !loader::mark_initialized(m.base) {
                    continue;
                }
                run_tls_callbacks(m, DLL_PROCESS_ATTACH);
                if let Some(e) = m.entry() {
                    if call_dllmain(e, m.base, DLL_PROCESS_ATTACH, 0) == 0 {
                        eprintln!("winlux: l'initialisation de {} a échoué", m.name);
                        return 1603;
                    }
                }
            }
            let _ = &dll;
            if crate::wow32::active() {
                return crate::wow32::call32(export, &[arg]) as u32;
            }
            let f: extern "win64" fn(u32) -> u32 = unsafe { std::mem::transmute(export) };
            f(arg)
        })
        .expect("création du thread principal");
    let code = h.join().unwrap_or(1603);
    crate::msi::session::save(code);
    crate::dlls::kernel32::flush_and_exit(code as i32)
}

/// DllMain d'un module réel, x86 (wow32) ou x64.
pub fn call_dllmain(entry: usize, base: usize, reason: u32, reserved: usize) -> i32 {
    if crate::wow32::active() {
        return crate::wow32::call32(entry, &[base as u32, reason, reserved as u32]) as i32;
    }
    let f: extern "win64" fn(usize, u32, usize) -> i32 = unsafe { std::mem::transmute(entry) };
    f(base, reason, reserved)
}

pub fn run(exe: Module) -> ! {
    let reserve = (exe.pe.stack_reserve as usize).clamp(1 << 20, 256 << 20);
    let h = std::thread::Builder::new()
        .name("win-main".into())
        .stack_size(reserve)
        .spawn(move || start(exe))
        .expect("création du thread principal");
    let code = h.join().unwrap_or(0xC000_0005);
    crate::dlls::kernel32::flush_and_exit(code as i32)
}

/// Programme Chromium ou Electron (Claude, Discord, VS Code…) : son bac à
/// sable Windows exige d'écrire dans la mémoire de l'enfant et de lui
/// confier des poignées avant son démarrage, ce que Winlux ne sait pas
/// encore faire ; le courtier tue alors chaque enfant (#767). Winlux pose
/// la question à l'utilisateur (capacité « sandbox ») et n'ajoute
/// `--no-sandbox` qu'après un « Autoriser » explicite, révocable dans
/// Paramètres > Confidentialité et sécurité.
fn chromium_sandbox_refused(exe_win: &str, cmd: &str) -> bool {
    if cmd.contains("--no-sandbox") {
        return false;
    }
    let exe = crate::paths::to_linux(exe_win);
    let is_chromium = std::path::Path::new(&exe)
        .parent()
        .is_some_and(is_chromium_dir);
    is_chromium && crate::permissions::allowed(crate::permissions::Capability::Sandbox)
}

/// Dossier d'un programme Chromium : données ICU et instantané V8 côte à côte.
fn is_chromium_dir(dir: &std::path::Path) -> bool {
    dir.join("icudtl.dat").is_file()
        && (dir.join("v8_context_snapshot.bin").is_file()
            || dir.join("snapshot_blob.bin").is_file())
}
