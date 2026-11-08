//! VirtualAllocEx, VirtualProtectEx et VirtualFreeEx dans un enfant figé
//! (#767).
//!
//! Le bac à sable de Chromium prépare son enfant créé suspendu : il y
//! alloue de la mémoire, y copie sa politique puis le relance. Linux n'offre
//! pas d'appel pour projeter de la mémoire dans un autre processus. L'enfant
//! est figé (SIGSTOP) et descend de nous : on l'attache un instant par
//! ptrace, on lui fait exécuter un seul appel système (mmap, mprotect,
//! munmap) à l'adresse où il est arrêté, puis on rétablit ses registres et
//! son code et on le laisse figé, comme avant.
//!
//! Seul un enfant encore suspendu est concerné : un processus qui tourne
//! n'est jamais interrompu ainsi.

use std::collections::HashMap;
use std::io::Error;
use std::sync::Mutex;

/// Régions projetées dans chaque enfant : (PID, adresse) → taille.
static REGIONS: Mutex<Option<HashMap<(i32, usize), usize>>> = Mutex::new(None);

fn regions() -> std::sync::MutexGuard<'static, Option<HashMap<(i32, usize), usize>>> {
    REGIONS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Taille d'une région allouée par `map` dans `pid`.
pub fn region_len(pid: i32, addr: usize) -> Option<usize> {
    regions().as_ref()?.get(&(pid, addr)).copied()
}

const SYS_MMAP: u64 = 9;
const SYS_MPROTECT: u64 = 10;
const SYS_MUNMAP: u64 = 11;

/// Exécute un appel système dans `pid`, figé et descendant de nous.
/// Renvoie la valeur de retour brute (négative = -errno).
unsafe fn remote_syscall(pid: i32, nr: u64, args: [u64; 6]) -> Result<i64, Error> {
    if libc::ptrace(libc::PTRACE_SEIZE, pid, 0usize, 0usize) != 0 {
        return Err(Error::last_os_error());
    }
    let detach = |sig: i32| {
        libc::ptrace(libc::PTRACE_DETACH, pid, 0usize, sig as usize);
    };
    if libc::ptrace(libc::PTRACE_INTERRUPT, pid, 0usize, 0usize) != 0 {
        let e = Error::last_os_error();
        detach(0);
        return Err(e);
    }
    let mut status = 0;
    if libc::waitpid(pid, &mut status, libc::__WALL) != pid {
        let e = Error::last_os_error();
        detach(0);
        return Err(e);
    }
    let mut saved: libc::user_regs_struct = std::mem::zeroed();
    if libc::ptrace(libc::PTRACE_GETREGS, pid, 0usize, &mut saved as *mut _) != 0 {
        let e = Error::last_os_error();
        detach(libc::SIGSTOP);
        return Err(e);
    }
    // Instruction `syscall` (0F 05) posée là où l'enfant est arrêté.
    let at = saved.rip as usize;
    *libc::__errno_location() = 0;
    let code = libc::ptrace(libc::PTRACE_PEEKTEXT, pid, at, 0usize);
    if code == -1 && *libc::__errno_location() != 0 {
        let e = Error::last_os_error();
        detach(libc::SIGSTOP);
        return Err(e);
    }
    let patched = (code as u64 & !0xffff) | 0x050f;
    if libc::ptrace(libc::PTRACE_POKETEXT, pid, at, patched as usize) != 0 {
        let e = Error::last_os_error();
        detach(libc::SIGSTOP);
        return Err(e);
    }
    let mut regs = saved;
    regs.rax = nr;
    // Pas de redémarrage d'un appel système interrompu.
    regs.orig_rax = u64::MAX;
    regs.rdi = args[0];
    regs.rsi = args[1];
    regs.rdx = args[2];
    regs.r10 = args[3];
    regs.r8 = args[4];
    regs.r9 = args[5];
    let mut result = Err(Error::from_raw_os_error(libc::EIO));
    if libc::ptrace(libc::PTRACE_SETREGS, pid, 0usize, &regs as *const _) == 0 {
        // Un arrêt d'un autre type (signal en attente, arrêt de groupe) peut
        // précéder celui du pas à pas : on l'ignore et on recommence, jusqu'à
        // ce que l'instruction ait été exécutée.
        for _ in 0..16 {
            if libc::ptrace(libc::PTRACE_SINGLESTEP, pid, 0usize, 0usize) != 0 {
                break;
            }
            let mut st = 0;
            if libc::waitpid(pid, &mut st, libc::__WALL) != pid || !libc::WIFSTOPPED(st) {
                break;
            }
            let mut after: libc::user_regs_struct = std::mem::zeroed();
            if libc::ptrace(libc::PTRACE_GETREGS, pid, 0usize, &mut after as *mut _) != 0 {
                break;
            }
            if after.rip as usize == at + 2 {
                result = Ok(after.rax as i64);
                break;
            }
        }
    }
    libc::ptrace(libc::PTRACE_POKETEXT, pid, at, code as usize);
    libc::ptrace(libc::PTRACE_SETREGS, pid, 0usize, &saved as *const _);
    // Rendu figé, comme il était : seul ResumeThread le relance.
    detach(libc::SIGSTOP);
    result
}

fn errno_of(r: i64) -> Option<i32> {
    (-4095..0).contains(&r).then_some(-r as i32)
}

/// Code Windows pour une erreur Linux d'une opération dans l'enfant.
fn win_error(e: i32) -> u32 {
    match e {
        libc::ENOMEM => 8,
        libc::EPERM | libc::EACCES => 5,
        libc::ESRCH => 6,
        libc::EEXIST => 487,
        _ => 87,
    }
}

/// mmap anonyme privé dans l'enfant. `fixed` : à cette adresse exacte.
pub unsafe fn map(pid: i32, addr: usize, size: usize, prot: i32) -> Result<usize, u32> {
    let mut flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
    if addr != 0 {
        flags |= libc::MAP_FIXED_NOREPLACE;
    }
    let r = remote_syscall(
        pid,
        SYS_MMAP,
        [
            addr as u64,
            size as u64,
            prot as u64,
            flags as u64,
            u64::MAX,
            0,
        ],
    )
    .map_err(|e| win_error(e.raw_os_error().unwrap_or(libc::EIO)))?;
    if let Some(e) = errno_of(r) {
        return Err(win_error(e));
    }
    if addr != 0 && r as usize != addr {
        let _ = unmap(pid, r as usize, size);
        return Err(487);
    }
    regions()
        .get_or_insert_with(HashMap::new)
        .insert((pid, r as usize), size);
    Ok(r as usize)
}

pub unsafe fn protect(pid: i32, addr: usize, size: usize, prot: i32) -> Result<(), u32> {
    let r = remote_syscall(
        pid,
        SYS_MPROTECT,
        [addr as u64, size as u64, prot as u64, 0, 0, 0],
    )
    .map_err(|e| win_error(e.raw_os_error().unwrap_or(libc::EIO)))?;
    match errno_of(r) {
        Some(e) => Err(win_error(e)),
        None => Ok(()),
    }
}

pub unsafe fn unmap(pid: i32, addr: usize, size: usize) -> Result<(), u32> {
    let r = remote_syscall(pid, SYS_MUNMAP, [addr as u64, size as u64, 0, 0, 0, 0])
        .map_err(|e| win_error(e.raw_os_error().unwrap_or(libc::EIO)))?;
    match errno_of(r) {
        Some(e) => Err(win_error(e)),
        None => {
            if let Some(m) = regions().as_mut() {
                m.remove(&(pid, addr));
            }
            Ok(())
        }
    }
}

/// Protection Linux actuelle de la page qui contient `addr` dans `pid`,
/// lue dans /proc/<pid>/maps.
pub fn current_prot(pid: i32, addr: usize) -> Option<i32> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).ok()?;
    for line in maps.lines() {
        let mut it = line.split_whitespace();
        let range = it.next()?;
        let perms = it.next()?.as_bytes();
        let (a, b) = range.split_once('-')?;
        let (a, b) = (
            usize::from_str_radix(a, 16).ok()?,
            usize::from_str_radix(b, 16).ok()?,
        );
        if addr >= a && addr < b {
            let mut p = 0;
            if perms.first() == Some(&b'r') {
                p |= libc::PROT_READ;
            }
            if perms.get(1) == Some(&b'w') {
                p |= libc::PROT_WRITE;
            }
            if perms.get(2) == Some(&b'x') {
                p |= libc::PROT_EXEC;
            }
            return Some(p);
        }
    }
    None
}

/// Protection Windows équivalente à une protection Linux.
pub fn win_prot_of(p: i32) -> u32 {
    let r = p & libc::PROT_READ != 0;
    let w = p & libc::PROT_WRITE != 0;
    let x = p & libc::PROT_EXEC != 0;
    match (r, w, x) {
        (_, true, true) => 0x40,
        (true, false, true) => 0x20,
        (false, false, true) => 0x10,
        (_, true, false) => 0x04,
        (true, false, false) => 0x02,
        _ => 0x01,
    }
}
