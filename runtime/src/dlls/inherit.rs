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

    pub(crate) fn map(fd: OwnedFd) -> Option<SharedWord> {
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
    /// Mot `i` de la page (0 : l'état ; les suivants servent aux objets
    /// nommés, voir `kernel32::shared_sync`).
    pub(crate) fn slot(&self, i: usize) -> &AtomicU32 {
        assert!(i < 1024);
        unsafe { &*self.ptr.add(i) }
    }
    /// Attend au plus `ms` millisecondes que le mot 0 quitte `value`.
    pub(crate) fn wait_while(&self, value: u32, ms: u32) {
        let ts = libc::timespec {
            tv_sec: (ms / 1000) as libc::time_t,
            tv_nsec: ((ms % 1000) * 1_000_000) as libc::c_long,
        };
        unsafe {
            libc::syscall(
                libc::SYS_futex,
                self.ptr,
                libc::FUTEX_WAIT,
                value,
                &ts as *const libc::timespec,
                0,
                0,
            );
        }
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
        self.wait_while(0, ms);
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

/// Description texte d'un handle transmissible à un autre processus, et
/// les descripteurs à joindre. Dans le texte, le i-ème descripteur est noté
/// `#i` ; l'appelant le remplace par son numéro (héritage) ou l'envoie par
/// SCM_RIGHTS (DuplicateHandle vers un autre processus, `handle_broker`).
///
/// `move_pipe` : une instance de tube nommé quitte ce processus (héritage,
/// DUPLICATE_CLOSE_SOURCE) ; sinon elle part comme simple descripteur.
pub(crate) fn describe(h: usize, move_pipe: bool) -> Option<(String, Vec<OwnedFd>)> {
    let dup = |fd: i32| -> Option<OwnedFd> {
        let c = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
        (c >= 0).then(|| unsafe { OwnedFd::from_raw_fd(c) })
    };
    let ov = u8::from(crate::dlls::kernel32::is_overlapped_handle(h));
    let pipe = if move_pipe {
        crate::dlls::named_pipes::export(h)
    } else {
        None
    };
    if let Some((name, client, connected)) = pipe {
        // Instance de tube nommé : l'autre processus la reprend.
        let server = dup(crate::dlls::kernel32::fd_of_handle(h)?)?;
        let mut fds = vec![server];
        let client = match client.and_then(dup) {
            Some(c) => {
                fds.push(c);
                "#1".to_string()
            }
            None => "-".to_string(),
        };
        crate::dlls::named_pipes::release(h);
        return Some((
            format!(
                "n,{h:x},#0,{client},{},{ov},{}",
                u8::from(connected),
                hex(&name)
            ),
            fds,
        ));
    }
    if let Some(fd) = crate::dlls::kernel32::fd_of_handle(h) {
        return Some((format!("f,{h:x},#0,{ov}"), vec![dup(fd)?]));
    }
    if let Some((fd, manual)) = crate::dlls::kernel32::share_event(h) {
        return Some((format!("e,{h:x},#0,{}", u8::from(manual)), vec![dup(fd)?]));
    }
    if let Some((fd, size, access)) = crate::dlls::mmap::export(h) {
        return Some((format!("m,{h:x},#0,{size},{access:x}"), vec![dup(fd)?]));
    }
    if let Some(pid) = crate::dlls::kernel32::inheritable_process_pid(h) {
        return Some((format!("p,{h:x},{pid}"), Vec::new()));
    }
    None
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
        let Some((text, own)) = describe(h, true) else {
            crate::trace!("handle {h:#x} héritable mais pas transmissible à l'enfant");
            continue;
        };
        // Chaque `#i` devient le numéro d'une copie héritable.
        let mut numbers = Vec::new();
        for fd in &own {
            match inheritable_copy(fd.as_raw_fd(), &mut fds) {
                Some(n) => numbers.push(n),
                None => break,
            }
        }
        if numbers.len() != own.len() {
            continue;
        }
        parts.push(substitute(&text, |i| numbers.get(i).map(|n| n.to_string())));
    }
    Passed {
        env: parts.join(";"),
        _fds: fds,
    }
}

/// Remplace les champs `#i` d'une description.
pub(crate) fn substitute(text: &str, mut f: impl FnMut(usize) -> Option<String>) -> String {
    text.split(',')
        .map(|field| {
            field
                .strip_prefix('#')
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(&mut f)
                .unwrap_or_else(|| field.to_string())
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Recrée un handle décrit par `describe`. `fd` donne le descripteur d'un
/// champ ; `at` impose le numéro du handle (0 : celui de la description,
/// ou un numéro neuf si elle porte 0). Rend le numéro du handle créé.
pub(crate) fn adopt_one(
    part: &str,
    at: usize,
    mut fd: impl FnMut(&str) -> Option<OwnedFd>,
) -> Option<usize> {
    let f: Vec<&str> = part.split(',').collect();
    let described = f.get(1).and_then(|h| usize::from_str_radix(h, 16).ok())?;
    let h = match (at, described) {
        (0, 0) => crate::dlls::kernel32::new_handle_number(),
        (0, d) => d,
        (a, _) => a,
    };
    let flag = |i: usize| f.get(i) == Some(&"1");
    let mut field = |i: usize| f.get(i).and_then(|s| fd(s));
    match f[0] {
        "f" => {
            let file = field(2)?;
            crate::dlls::kernel32::insert_file_at(h, std::fs::File::from(file), flag(3));
        }
        "n" => {
            let server = field(2)?;
            let client = field(3);
            let name = unhex(f.get(6)?)?;
            crate::dlls::kernel32::insert_file_at(h, std::fs::File::from(server), flag(5));
            crate::dlls::named_pipes::adopt(h, &name, client, flag(4));
        }
        "e" => {
            let w = SharedWord::map(field(2)?)?;
            crate::dlls::kernel32::insert_sync_at(
                h,
                crate::dlls::kernel32::EventObject::adopt(flag(3), w),
            );
        }
        "m" => {
            let file = field(2)?;
            let size = f.get(3)?.parse::<u64>().ok()?;
            let access = f
                .get(4)
                .and_then(|a| u32::from_str_radix(a, 16).ok())
                .unwrap_or(crate::dlls::mmap::SECTION_ALL_ACCESS);
            crate::dlls::mmap::adopt(h, file, size, access);
        }
        "p" => {
            let pid = f.get(2)?.parse::<i32>().ok()?;
            crate::dlls::kernel32::insert_sync_at(h, crate::dlls::kernel32::foreign_process(pid));
        }
        _ => return None,
    }
    Some(h)
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
        let inherited = |s: &str| -> Option<OwnedFd> {
            let n: i32 = s.parse().ok()?;
            unsafe {
                if libc::fcntl(n, libc::F_SETFD, libc::FD_CLOEXEC) != 0 {
                    return None;
                }
                Some(OwnedFd::from_raw_fd(n))
            }
        };
        match adopt_one(part, 0, inherited) {
            Some(h) => {
                set(h, true);
                highest = highest.max(h);
                crate::trace!("handle hérité {part}");
            }
            None => crate::log_warn!("process", "handle hérité illisible : {part}"),
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
    fn descripteurs_numerotes() {
        assert_eq!(
            substitute("n,1c,#0,#1,1,0,6162", |i| Some((40 + i).to_string())),
            "n,1c,40,41,1,0,6162"
        );
        assert_eq!(substitute("p,8,1234", |_| None), "p,8,1234");
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
