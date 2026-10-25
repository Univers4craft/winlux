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

pub const DLL_PROCESS_DETACH: u32 = 0;
pub const DLL_PROCESS_ATTACH: u32 = 1;
pub const DLL_THREAD_ATTACH: u32 = 2;
pub const DLL_THREAD_DETACH: u32 = 3;

static PEB: AtomicUsize = AtomicUsize::new(0);

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

pub fn init_process(exe: &Module, args: &[String]) {
    let exe_win = crate::paths::to_windows(&exe.path);
    let mut all = vec![exe_win.clone()];
    all.extend(args.iter().cloned());
    let cmd = all
        .iter()
        .map(|a| quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ");
    let cmd_w: Vec<u16> = cmd.encode_utf16().collect();
    let mut cmd_a = crate::dlls::encode_ansi(&cmd);
    cmd_a.push(0);

    let peb = alloc_zeroed(0x1000);
    let params = alloc_zeroed(0x1000);
    unsafe {
        *((peb + PEB_IMAGE_BASE) as *mut usize) = exe.base;
        *((peb + PEB_PROCESS_PARAMETERS) as *mut usize) = params;
        *((peb + PEB_PROCESS_HEAP) as *mut usize) = crate::dlls::kernel32::PROCESS_HEAP;
        *((peb + PEB_BEING_DEBUGGED) as *mut u8) = 0;
        // OSMajorVersion / OSMinorVersion / OSBuildNumber : Windows 10 (10.0.19045).
        *((peb + 0x118) as *mut u32) = 10;
        *((peb + 0x11c) as *mut u32) = 0;
        *((peb + 0x120) as *mut u16) = 19045;
        *((peb + 0x124) as *mut u32) = 2; // VER_PLATFORM_WIN32_NT
    }
    let path_w: Vec<u16> = exe_win.encode_utf16().collect();
    unicode_string(params + 0x60, &path_w);
    unicode_string(params + 0x70, &cmd_w);
    let mut command_line_w = cmd_w;
    command_line_w.push(0);
    PEB.store(peb, Ordering::Relaxed);
    let _ = PROCESS.set(ProcessInfo {
        command_line_w,
        command_line_a: cmd_a,
    });
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
    let mut threads = THREAD_TEBS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !threads.contains(&teb) {
        threads.push(teb);
    }
    teb
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
    let mut idx = 0usize;
    for m in mods.iter() {
        if let Some(t) = &m.tls {
            let raw = t.raw_end.saturating_sub(t.raw_start);
            let block = alloc_zeroed((raw + t.zero_fill + 16).max(16));
            unsafe {
                std::ptr::copy_nonoverlapping(t.raw_start as *const u8, block as *mut u8, raw);
                *((array + idx * 8) as *mut usize) = block;
                if t.index_addr != 0 {
                    *(t.index_addr as *mut u32) = idx as u32;
                }
            }
            idx += 1;
        }
    }
    unsafe { *((teb + TEB_TLS_POINTER) as *mut usize) = array };
}

fn run_tls_callbacks(m: &Module, reason: u32) {
    let Some(t) = &m.tls else { return };
    if t.callbacks_addr == 0 {
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

/// Termine l'initialisation Win32 d'une DLL chargée explicitement par LoadLibrary.
pub fn initialize_dynamic_module(m: &Module) -> bool {
    let tls_context = if let Some(tls) = &m.tls {
        let thread_teb = teb();
        if thread_teb == 0 {
            return false;
        }
        let modules = loader::MODULES.lock().unwrap().clone();
        let index = modules
            .iter()
            .take_while(|x| x.base != m.base)
            .filter(|x| x.tls.is_some())
            .count();
        // Le tableau du TEB fait 512 entrées (4 KiB); refuser explicitement
        // plutôt que d'écrire hors limites si l'image dépasse cette capacité.
        if index >= 512 {
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
    if let Some((tls, array, index)) = tls_context {
        let raw = tls.raw_end.saturating_sub(tls.raw_start);
        let block = alloc_zeroed((raw + tls.zero_fill + 16).max(16));
        unsafe {
            std::ptr::copy_nonoverlapping(tls.raw_start as *const u8, block as *mut u8, raw);
            *((array + index * 8) as *mut usize) = block;
            if tls.index_addr != 0 {
                *(tls.index_addr as *mut u32) = index as u32;
            }
        }
    }
    run_tls_callbacks(m, DLL_PROCESS_ATTACH);
    if let Some(entry) = m.entry() {
        let dllmain: extern "win64" fn(usize, u32, usize) -> i32 =
            unsafe { std::mem::transmute(entry) };
        if dllmain(m.base, DLL_PROCESS_ATTACH, 1) == 0 {
            return false;
        }
    }
    true
}

pub fn thread_attach_current() {
    init_thread();
    let mods = loader::MODULES.lock().unwrap().clone();
    for m in mods.into_iter().skip(1) {
        run_tls_callbacks(&m, DLL_THREAD_ATTACH);
        if let Some(e) = m.entry() {
            let dllmain: extern "win64" fn(usize, u32, usize) -> i32 =
                unsafe { std::mem::transmute(e) };
            let _ = dllmain(m.base, DLL_THREAD_ATTACH, 1);
        }
    }
}

pub fn thread_detach_current() {
    let mods = loader::MODULES.lock().unwrap().clone();
    for m in mods.into_iter().skip(1).rev() {
        run_tls_callbacks(&m, DLL_THREAD_DETACH);
        if let Some(e) = m.entry() {
            let dllmain: extern "win64" fn(usize, u32, usize) -> i32 =
                unsafe { std::mem::transmute(e) };
            let _ = dllmain(m.base, DLL_THREAD_DETACH, 1);
        }
    }
}

/// Initialise les DLL réelles (rappels TLS + DllMain) puis lance le programme.
fn start(exe: Module) -> u32 {
    init_thread();
    crate::crash::install();
    let mods = loader::MODULES.lock().unwrap().clone();
    // Les dépendances ont été chargées après le .exe : on initialise en ordre inverse.
    for m in mods.iter().skip(1).rev() {
        if !loader::mark_initialized(m.base) {
            continue;
        }
        run_tls_callbacks(m, DLL_PROCESS_ATTACH);
        if let Some(e) = m.entry() {
            trace!("DllMain de {}", m.name);
            let dllmain: extern "win64" fn(usize, u32, usize) -> i32 =
                unsafe { std::mem::transmute(e) };
            if dllmain(m.base, DLL_PROCESS_ATTACH, 1) == 0 {
                eprintln!("winlux: l'initialisation de {} a échoué", m.name);
                return 0xC000_0142;
            }
        }
    }
    loader::mark_initialized(exe.base);
    run_tls_callbacks(&exe, DLL_PROCESS_ATTACH);
    let entry = exe.entry().expect("le programme n'a pas de point d'entrée");
    trace!("saut vers le point d'entrée 0x{entry:x}");
    let f: extern "win64" fn() -> u32 = unsafe { std::mem::transmute(entry) };
    f()
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
