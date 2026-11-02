//! Héritage des handles entre processus Windows (`bInheritHandles`).
//!
//! Sous Windows, un handle marqué héritable (`SECURITY_ATTRIBUTES`,
//! `SetHandleInformation`, `DuplicateHandle`) existe dans l'enfant lancé par
//! `CreateProcess(…, bInheritHandles = TRUE, …)` avec **la même valeur** :
//! le parent la lui transmet souvent en texte sur la ligne de commande.
//! crashpad (Chromium, Electron, Sentry) passe ainsi à son gestionnaire de
//! plantage ses événements, sa première instance de tube nommé et un handle
//! vers lui-même ; `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` restreint la liste.
//!
//! Ici, chaque processus Windows est un processus Linux qui a sa propre
//! table de handles. Le parent passe donc à l'enfant :
//! - les fichiers, tubes et sockets comme descripteurs Linux hérités ;
//! - les événements comme une page de mémoire partagée (memfd) qui porte
//!   leur état, attendue et réveillée par futex entre processus ;
//! - les processus comme leur PID ;
//! - les instances de tube nommé non encore connectées, que l'enfant
//!   reprend (c'est lui qui attendra les clients) ;
//!
//! et la description de tout cela dans la variable `WINLUX_HANDLES_HERITES`,
//! que l'enfant lit au démarrage pour recréer chaque handle à sa valeur.

use std::collections::HashSet;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

pub(crate) const ENV: &str = "WINLUX_HANDLES_HERITES";
const HANDLE_FLAG_INHERIT: u32 = 1;

fn inheritable() -> &'static Mutex<HashSet<usize>> {
    static S: std::sync::OnceLock<Mutex<HashSet<usize>>> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn set(h: usize, on: bool) {
    let mut s = crate::dlls::lock(inheritable());
    if on {
        s.insert(h);
    } else {
        s.remove(&h);
    }
}

pub(crate) fn is(h: usize) -> bool {
    crate::dlls::lock(inheritable()).contains(&h)
}

/// `SECURITY_ATTRIBUTES.bInheritHandle` (décalage 16 en 64 bits).
pub(crate) unsafe fn from_security_attributes(h: usize, sa: usize) {
    if sa != 0
        && h != 0
        && h != usize::MAX
        && std::ptr::read_unaligned((sa + 16) as *const i32) != 0
    {
        set(h, true);
    }
}

/// `SetHandleInformation(h, mask, flags)`.
pub(crate) fn set_information(h: usize, mask: u32, flags: u32) {
    if mask & HANDLE_FLAG_INHERIT != 0 {
        set(h, flags & HANDLE_FLAG_INHERIT != 0);
    }
}

/// `GetHandleInformation` : drapeaux du handle.
pub(crate) fn information(h: usize) -> u32 {
    if is(h) {
        HANDLE_FLAG_INHERIT
    } else {
        0
    }
}

// ============================================================
//  Page partagée d'un événement
// ============================================================

/// Mot de 32 bits dans une page memfd partagée entre processus : 1 =
/// signalé. Les attentes passent par futex (sans FUTEX_PRIVATE_FLAG, donc
/// valables d'un processus à l'autre).
pub(crate) struct SharedWord {
    ptr: *mut AtomicU32,
    fd: OwnedFd,
}

unsafe impl Send for SharedWord {}
unsafe impl Sync for SharedWord {}

impl SharedWord {
    pub(crate) fn create(initial: bool) -> Option<SharedWord> {
        let fd = unsafe { libc::memfd_create(c"evenement-partage".as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::ftruncate(fd.as_raw_fd(), 4096) } != 0 {
            return None;
        }
        let w = SharedWord::map(fd)?;
        w.set(initial);
        Some(w)
    }

    fn map(fd: OwnedFd) -> Option<SharedWord> {
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if p == libc::MAP_FAILED {
            return None;
        }
        Some(SharedWord { ptr: p.cast(), fd })
    }

    fn word(&self) -> &AtomicU32 {
        unsafe { &*self.ptr }
    }
    pub(crate) fn fd(&self) -> i32 {
        self.fd.as_raw_fd()
    }
    pub(crate) fn get(&self) -> u32 {
        self.word().load(Ordering::Acquire)
    }
    pub(crate) fn set(&self, v: bool) {
        self.word().store(u32::from(v), Ordering::Release);
    }
    /// Prend le signal d'un événement automatique.
    pub(crate) fn take(&self) -> bool {
        self.word()
            .compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub(crate) fn wake(&self) {
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                self.ptr,
                libc::FUTEX_WAKE,
                i32::MAX,
                0,
                0,
                0,
            );
        }
    }
    /// Attend au plus `ms` millisecondes que le mot quitte 0.
    pub(crate) fn wait(&self, ms: u32) {
        let ts = libc::timespec {
            tv_sec: (ms / 1000) as libc::time_t,
            tv_nsec: ((ms % 1000) * 1_000_000) as libc::c_long,
        };
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                self.ptr,
                libc::FUTEX_WAIT,
                0,
                &ts as *const libc::timespec,
                0,
                0,
            );
        }
    }
}

impl Drop for SharedWord {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.cast(), 4096);
        }
    }
}

// ============================================================
//  Transmission à l'enfant
// ============================================================

/// Ce que le parent passe à un enfant : la description (variable
/// d'environnement) et les descripteurs hérités, à fermer après le
/// lancement.
pub(crate) struct Passed {
    pub(crate) env: String,
    _fds: Vec<OwnedFd>,
}

/// Copie héritable (sans FD_CLOEXEC) d'un descripteur.
fn inheritable_copy(fd: i32, fds: &mut Vec<OwnedFd>) -> Option<i32> {
    let c = unsafe { libc::fcntl(fd, libc::F_DUPFD, 3) };
    if c < 0 {
        return None;
    }
    fds.push(unsafe { OwnedFd::from_raw_fd(c) });
    Some(c)
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = b
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect();
    String::from_utf8(bytes?).ok()
}

/// Handles à transmettre : tous les héritables, ou seulement ceux de
/// `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` (qui doivent eux aussi l'être).
pub(crate) fn prepare(list: Option<&[usize]>) -> Passed {
    let mut handles: Vec<usize> = crate::dlls::lock(inheritable()).iter().copied().collect();
    if let Some(l) = list {
        handles.retain(|h| l.contains(h));
    }
    handles.sort_unstable();
    let mut fds = Vec::new();
    let mut parts = Vec::new();
    for h in handles {
        let ov = u8::from(crate::dlls::kernel32::is_overlapped_handle(h));
        if let Some((name, client, connected)) = crate::dlls::named_pipes::export(h) {
            // Instance de tube nommé : l'enfant la reprend.
            let Some(fd) = crate::dlls::kernel32::fd_of_handle(h)
                .and_then(|fd| inheritable_copy(fd, &mut fds))
            else {
                continue;
            };
            let client = client
                .and_then(|c| inheritable_copy(c, &mut fds))
                .map_or("-".to_string(), |c| c.to_string());
            parts.push(format!(
                "n,{h:x},{fd},{client},{},{ov},{}",
                u8::from(connected),
                hex(&name)
            ));
            crate::dlls::named_pipes::release(h);
        } else if let Some(fd) = crate::dlls::kernel32::fd_of_handle(h) {
            if let Some(c) = inheritable_copy(fd, &mut fds) {
                parts.push(format!("f,{h:x},{c},{ov}"));
            }
        } else if let Some((fd, manual)) = crate::dlls::kernel32::share_event(h) {
            if let Some(c) = inheritable_copy(fd, &mut fds) {
                parts.push(format!("e,{h:x},{c},{}", u8::from(manual)));
            }
        } else if let Some(pid) = crate::dlls::kernel32::inheritable_process_pid(h) {
            parts.push(format!("p,{h:x},{pid}"));
        } else {
            crate::trace!("handle {h:#x} héritable mais pas transmissible à l'enfant");
        }
    }
    Passed {
        env: parts.join(";"),
        _fds: fds,
    }
}

/// Au démarrage d'un processus Windows : recrée les handles hérités.
pub(crate) fn adopt() {
    let Ok(text) = std::env::var(ENV) else {
        return;
    };
    // L'enfant de l'enfant ne doit pas les recevoir une seconde fois.
    std::env::remove_var(ENV);
    let mut highest = 0usize;
    for part in text.split(';').filter(|p| !p.is_empty()) {
        let f: Vec<&str> = part.split(',').collect();
        let Some(h) = f.get(1).and_then(|h| usize::from_str_radix(h, 16).ok()) else {
            continue;
        };
        let fd = |i: usize| -> Option<OwnedFd> {
            let n: i32 = f.get(i)?.parse().ok()?;
            unsafe {
                if libc::fcntl(n, libc::F_SETFD, libc::FD_CLOEXEC) != 0 {
                    return None;
                }
                Some(OwnedFd::from_raw_fd(n))
            }
        };
        let flag = |i: usize| f.get(i) == Some(&"1");
        let ok = match f[0] {
            "f" => fd(2).map(|fd| {
                crate::dlls::kernel32::insert_file_at(h, std::fs::File::from(fd), flag(3));
            }),
            "n" => fd(2).and_then(|server| {
                let client = fd(3);
                let name = unhex(f.get(6)?)?;
                crate::dlls::kernel32::insert_file_at(h, std::fs::File::from(server), flag(5));
                crate::dlls::named_pipes::adopt(h, &name, client, flag(4));
                Some(())
            }),
            "e" => fd(2).and_then(SharedWord::map).map(|w| {
                crate::dlls::kernel32::insert_sync_at(
                    h,
                    crate::dlls::kernel32::EventObject::adopt(flag(3), w),
                );
            }),
            "p" => f.get(2).and_then(|p| p.parse::<i32>().ok()).map(|pid| {
                crate::dlls::kernel32::insert_sync_at(
                    h,
                    crate::dlls::kernel32::foreign_process(pid),
                );
            }),
            _ => None,
        };
        if ok.is_some() {
            set(h, true);
            highest = highest.max(h);
            crate::trace!("handle hérité {part}");
        } else {
            crate::log_warn!("process", "handle hérité illisible : {part}");
        }
    }
    crate::dlls::kernel32::reserve_handles_up_to(highest);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_en_hexadecimal() {
        let n = "crashpad_42_ABC\\é";
        assert_eq!(unhex(&hex(n)).as_deref(), Some(n));
        assert_eq!(unhex("4"), None);
    }

    #[test]
    fn mot_partage() {
        let w = SharedWord::create(false).unwrap();
        assert_eq!(w.get(), 0);
        w.set(true);
        assert!(w.take());
        assert!(!w.take());
        // Même page vue par un second mappage (comme dans l'enfant).
        let dup = unsafe { OwnedFd::from_raw_fd(libc::dup(w.fd())) };
        let other = SharedWord::map(dup).unwrap();
        other.set(true);
        assert_eq!(w.get(), 1);
        let t = std::time::Instant::now();
        w.set(false);
        w.wait(30);
        assert!(t.elapsed() >= std::time::Duration::from_millis(20));
    }
}
