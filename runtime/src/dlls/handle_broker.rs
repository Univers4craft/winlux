//! DuplicateHandle entre deux processus Windows (#236).
//!
//! Sous Windows, `DuplicateHandle(src_process, h, dst_process, …)` copie un
//! handle d'un processus dans un autre. Chromium et Electron en vivent :
//! le navigateur (le « broker » de Mojo) pousse dans un renderer, un
//! processus GPU ou un utilitaire les mémoires partagées, événements et
//! fichiers qu'il lui donne, et reprend (DUPLICATE_CLOSE_SOURCE) ceux que
//! l'enfant lui envoie par valeur.
//!
//! Ici, chaque processus Windows est un processus Linux avec sa propre
//! table de handles. Chacun peut donc ouvrir un « guichet » : une socket
//! Unix `SOCK_SEQPACKET` nommée par son PID, dans un dossier privé de
//! l'utilisateur (`$XDG_RUNTIME_DIR/winlux-handles`, droits 0700). Un
//! autre processus du même utilisateur s'y connecte et demande :
//!
//! - `PUT <héritable> <numéro> <description>` + descripteurs : recrée le
//!   handle chez le destinataire et répond `OK <numéro>` ;
//! - `GET <handle> <droits> <options>` : le détenteur en fait une copie aux
//!   droits demandés, la décrit et la joint (`OK <description>`), et ferme
//!   l'original si DUPLICATE_CLOSE_SOURCE ;
//! - `CLOSE <handle>` : ferme un handle chez lui.
//!
//! Les descriptions sont celles de l'héritage (`inherit::describe`) ; les
//! descripteurs voyagent par SCM_RIGHTS.
//!
//! Un enfant lancé par CreateProcess reçoit son guichet déjà ouvert par le
//! parent (descripteur hérité, `WINLUX_GUICHET_HANDLES`) : le parent peut y
//! déposer des handles avant même que l'enfant ne démarre, ce que fait
//! Chromium avec un enfant créé suspendu. Dans ce cas, le parent choisit le
//! numéro du handle dans une plage réservée et n'attend pas de réponse ;
//! l'enfant lit ces dépôts au démarrage, avant le programme.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// Descripteur hérité du guichet ouvert par le parent.
pub(crate) const ENV: &str = "WINLUX_GUICHET_HANDLES";
const DUPLICATE_CLOSE_SOURCE: u32 = 1;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_PROCESS_ABORTED: u32 = 1067;
/// Plus grand message : une description de handle tient en quelques
/// centaines d'octets (nom de tube en hexadécimal compris).
const MAX_MESSAGE: usize = 8192;
const MAX_FDS: usize = 4;

/// Numéros choisis par un parent pour un enfant encore suspendu : une plage
/// que kernel32 n'atteint jamais (ses handles partent de 0x10000).
static RESERVED: AtomicUsize = AtomicUsize::new(0x5A00_0000);
static LISTENING: OnceLock<bool> = OnceLock::new();
static TEMP: AtomicUsize = AtomicUsize::new(0);

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/winlux-{}", unsafe { libc::getuid() })));
    let d = base.join("winlux-handles");
    std::fs::create_dir_all(&d).ok()?;
    let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    Some(d)
}

fn address(path: &std::path::Path) -> Option<(libc::sockaddr_un, libc::socklen_t)> {
    use std::os::unix::ffi::OsStrExt;
    let mut a: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    a.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let b = path.as_os_str().as_bytes();
    if b.len() >= a.sun_path.len() {
        return None;
    }
    for (d, s) in a.sun_path.iter_mut().zip(b) {
        *d = *s as libc::c_char;
    }
    Some((
        a,
        std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
    ))
}

fn socket() -> Option<OwnedFd> {
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Socket d'écoute liée à `path` (remplace un reste d'un ancien processus).
fn listen_at(path: &std::path::Path) -> Option<OwnedFd> {
    let s = socket()?;
    let (a, len) = address(path)?;
    let _ = std::fs::remove_file(path);
    unsafe {
        if libc::bind(s.as_raw_fd(), &a as *const _ as *const libc::sockaddr, len) != 0
            || libc::listen(s.as_raw_fd(), 64) != 0
        {
            return None;
        }
    }
    Some(s)
}

// ============================================================
//  Messages
// ============================================================

fn send(fd: RawFd, text: &str, fds: &[OwnedFd]) -> bool {
    let fds: Vec<RawFd> = fds.iter().map(|f| f.as_raw_fd()).collect();
    let mut iov = libc::iovec {
        iov_base: text.as_ptr() as *mut libc::c_void,
        iov_len: text.len(),
    };
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of_val(fds.as_slice()) as u32) } as usize;
    let mut control = vec![0u8; space.max(1)];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    if !fds.is_empty() {
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        unsafe {
            let c = libc::CMSG_FIRSTHDR(&msg);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN(std::mem::size_of_val(fds.as_slice()) as u32) as _;
            std::ptr::copy_nonoverlapping(
                fds.as_ptr(),
                libc::CMSG_DATA(c) as *mut RawFd,
                fds.len(),
            );
        }
    }
    loop {
        let r = unsafe { libc::sendmsg(fd, &msg, libc::MSG_NOSIGNAL) };
        if r >= 0 {
            return r as usize == text.len();
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return false;
        }
    }
}

fn receive(fd: RawFd) -> Option<(String, Vec<OwnedFd>)> {
    let mut buf = vec![0u8; MAX_MESSAGE];
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    let space =
        unsafe { libc::CMSG_SPACE((MAX_FDS * std::mem::size_of::<RawFd>()) as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = space as _;
    let n = loop {
        let r = unsafe { libc::recvmsg(fd, &mut msg, libc::MSG_CMSG_CLOEXEC) };
        if r >= 0 {
            break r as usize;
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return None;
        }
    };
    let mut fds = Vec::new();
    unsafe {
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(c) as *const RawFd;
                let count = ((*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize)
                    / std::mem::size_of::<RawFd>();
                for i in 0..count {
                    fds.push(OwnedFd::from_raw_fd(*data.add(i)));
                }
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
    }
    if n == 0 && fds.is_empty() {
        return None;
    }
    let text = String::from_utf8(buf[..n].to_vec()).ok()?;
    Some((text, fds))
}

/// Le `#i` d'une description pris dans les descripteurs reçus.
fn take_fd(fds: &mut [Option<OwnedFd>], field: &str) -> Option<OwnedFd> {
    let i: usize = field.strip_prefix('#')?.parse().ok()?;
    fds.get_mut(i)?.take()
}

// ============================================================
//  Côté guichet (le processus qui reçoit les demandes)
// ============================================================

/// Copie locale de `h` aux droits demandés, sans fermer l'original.
fn local_copy(h: usize, access: u32, options: u32) -> Result<usize, u32> {
    let me = crate::dlls::kernel32::get_current_process();
    let mut copy = 0usize;
    let ok = unsafe {
        crate::dlls::kernel32::duplicate_handle(
            me,
            h,
            me,
            &mut copy,
            access,
            0,
            options & !DUPLICATE_CLOSE_SOURCE,
        )
    };
    if ok == 0 {
        Err(match crate::process::last_error() {
            0 => ERROR_ACCESS_DENIED,
            e => e,
        })
    } else {
        Ok(copy)
    }
}

/// Description transmissible d'une copie de `h` (droits `access`).
fn export(h: usize, access: u32, options: u32) -> Result<(String, Vec<OwnedFd>), u32> {
    let copy = local_copy(h, access, options)?;
    // Le pseudo-handle du processus courant devient un vrai handle vers
    // nous ; les autres copies sont des handles neufs, fermés après envoi.
    let described = crate::dlls::inherit::describe(copy, false);
    if copy != h {
        crate::dlls::kernel32::close_handle(copy);
    }
    described.ok_or(ERROR_NOT_SUPPORTED)
}

fn hex(v: &str) -> Option<usize> {
    usize::from_str_radix(v.trim_start_matches("0x"), 16).ok()
}

/// Traite une demande ; rend la réponse et ses descripteurs.
fn answer(text: &str, fds: Vec<OwnedFd>) -> (String, Vec<OwnedFd>) {
    let mut words = text.splitn(4, ' ');
    let err = |code: u32| (format!("ERR {code}"), Vec::new());
    match words.next() {
        Some("PUT") => {
            let inherit = words.next() == Some("1");
            let at = words.next().and_then(hex).unwrap_or(0);
            let Some(desc) = words.next() else {
                return err(ERROR_INVALID_HANDLE);
            };
            let mut slots: Vec<Option<OwnedFd>> = fds.into_iter().map(Some).collect();
            // `at` vaut 0 : un numéro neuf, choisi ici.
            let desc = zero_handle(desc);
            match crate::dlls::inherit::adopt_one(&desc, at, |f| take_fd(&mut slots, f)) {
                Some(h) => {
                    crate::dlls::inherit::set(h, inherit);
                    crate::trace!("guichet : handle reçu {desc} → {h:#x}");
                    (format!("OK {h:x}"), Vec::new())
                }
                None => err(ERROR_NOT_SUPPORTED),
            }
        }
        Some("GET") => {
            let h = words.next().and_then(hex);
            let access = words.next().and_then(hex).unwrap_or(0) as u32;
            let options = words.next().and_then(hex).unwrap_or(0) as u32;
            let Some(h) = h else {
                return err(ERROR_INVALID_HANDLE);
            };
            match export(h, access, options) {
                Ok((desc, fds)) => {
                    if options & DUPLICATE_CLOSE_SOURCE != 0 {
                        crate::dlls::kernel32::close_handle(h);
                    }
                    crate::trace!("guichet : handle {h:#x} cédé ({desc})");
                    (format!("OK {desc}"), fds)
                }
                Err(code) => {
                    // DUPLICATE_CLOSE_SOURCE ferme l'original même en cas
                    // d'échec, comme sous Windows.
                    if options & DUPLICATE_CLOSE_SOURCE != 0 {
                        crate::dlls::kernel32::close_handle(h);
                    }
                    err(code)
                }
            }
        }
        Some("CLOSE") => match words.next().and_then(hex) {
            Some(h) if crate::dlls::kernel32::close_handle(h) != 0 => ("OK 0".into(), Vec::new()),
            _ => err(ERROR_INVALID_HANDLE),
        },
        _ => err(ERROR_NOT_SUPPORTED),
    }
}

/// Une description reçue garde le numéro qu'avait le handle chez son
/// expéditeur : il ne veut rien dire ici, on le remet à 0.
fn zero_handle(desc: &str) -> String {
    let mut f: Vec<&str> = desc.split(',').collect();
    if f.len() > 1 {
        f[1] = "0";
    }
    f.join(",")
}

/// Même utilisateur que nous ? (le dossier est déjà privé ; double
/// vérification par SO_PEERCRED).
fn same_user(fd: RawFd) -> bool {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    r == 0 && cred.uid == unsafe { libc::getuid() }
}

fn serve_one(conn: OwnedFd) {
    if !same_user(conn.as_raw_fd()) {
        return;
    }
    let Some((text, fds)) = receive(conn.as_raw_fd()) else {
        return;
    };
    let (reply, out) = answer(&text, fds);
    // L'expéditeur d'un dépôt pour un enfant suspendu ne lit pas la
    // réponse : un échec d'envoi ici est normal.
    send(conn.as_raw_fd(), &reply, &out);
}

fn accept(listener: RawFd) -> Option<OwnedFd> {
    let c = unsafe {
        libc::accept4(
            listener,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            libc::SOCK_CLOEXEC,
        )
    };
    (c >= 0).then(|| unsafe { OwnedFd::from_raw_fd(c) })
}

fn serve(listener: OwnedFd) {
    let _ = std::thread::Builder::new()
        .name("winlux-guichet".into())
        .spawn(move || loop {
            match accept(listener.as_raw_fd()) {
                Some(conn) => serve_one(conn),
                None => {
                    let e = std::io::Error::last_os_error().raw_os_error();
                    if !matches!(e, Some(libc::EINTR | libc::ECONNABORTED)) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
            }
        });
}

/// Ouvre le guichet de ce processus s'il ne l'est pas encore.
pub(crate) fn ensure_listening() -> bool {
    *LISTENING.get_or_init(|| {
        let Some(path) = dir().map(|d| d.join(std::process::id().to_string())) else {
            return false;
        };
        match listen_at(&path) {
            Some(l) => {
                serve(l);
                true
            }
            None => false,
        }
    })
}

/// Fin du processus : son guichet disparaît avec lui.
pub(crate) fn forget() {
    if LISTENING.get() == Some(&true) {
        if let Some(d) = dir() {
            let _ = std::fs::remove_file(d.join(std::process::id().to_string()));
        }
    }
}

/// Au démarrage d'un enfant : reprend le guichet ouvert par le parent, lit
/// tout de suite les handles déjà déposés (avant que le programme ne les
/// cherche), puis le sert en tâche de fond.
pub(crate) fn adopt() {
    let Some(fd) = std::env::var(ENV)
        .ok()
        .and_then(|v| v.parse::<RawFd>().ok())
    else {
        return;
    };
    std::env::remove_var(ENV);
    unsafe {
        if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) != 0 {
            return;
        }
    }
    let listener = unsafe { OwnedFd::from_raw_fd(fd) };
    // Dépôts en attente : lecture sans bloquer.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        while let Some(conn) = accept(fd) {
            let cf = libc::fcntl(conn.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(conn.as_raw_fd(), libc::F_SETFL, cf & !libc::O_NONBLOCK);
            serve_one(conn);
        }
        libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
    }
    let _ = LISTENING.set(true);
    serve(listener);
}

/// Guichet préparé par le parent pour un enfant pas encore lancé.
pub(crate) struct ChildDesk {
    path: PathBuf,
    /// Copie héritable (sans FD_CLOEXEC), fermée chez le parent après le
    /// lancement.
    inheritable: OwnedFd,
}

impl ChildDesk {
    /// Valeur de `WINLUX_GUICHET_HANDLES` pour l'enfant.
    pub(crate) fn env(&self) -> String {
        self.inheritable.as_raw_fd().to_string()
    }

    /// L'enfant est lancé : son guichet prend le nom de son PID. Le
    /// parent ouvre aussi le sien, pour que l'enfant puisse lui reprendre
    /// des handles.
    pub(crate) fn launched(self, pid: u32) {
        if let Some(d) = dir() {
            let _ = std::fs::rename(&self.path, d.join(pid.to_string()));
        }
        ensure_listening();
    }
}

impl Drop for ChildDesk {
    fn drop(&mut self) {
        // Échec du lancement : le nom provisoire n'a pas été renommé.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Prépare le guichet d'un enfant Windows (avant CreateProcess).
pub(crate) fn prepare_child() -> Option<ChildDesk> {
    let d = dir()?;
    let path = d.join(format!(
        "nouveau-{}-{}",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = listen_at(&path)?;
    let c = unsafe { libc::fcntl(listener.as_raw_fd(), libc::F_DUPFD, 3) };
    if c < 0 {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(ChildDesk {
        path,
        inheritable: unsafe { OwnedFd::from_raw_fd(c) },
    })
}

// ============================================================
//  Côté demandeur
// ============================================================

fn connect(pid: i32) -> Result<OwnedFd, u32> {
    let path = dir().ok_or(ERROR_ACCESS_DENIED)?.join(pid.to_string());
    let (a, len) = address(&path).ok_or(ERROR_ACCESS_DENIED)?;
    let s = socket().ok_or(ERROR_ACCESS_DENIED)?;
    let r = unsafe { libc::connect(s.as_raw_fd(), &a as *const _ as *const libc::sockaddr, len) };
    if r != 0 {
        // Pas de guichet : un processus qui n'est pas un programme Windows
        // de Winlux, ou qui est déjà parti.
        crate::trace!("guichet du processus {pid} injoignable");
        return Err(ERROR_ACCESS_DENIED);
    }
    // Un processus figé (SIGSTOP) ne répondrait jamais.
    let tv = libc::timeval {
        tv_sec: 10,
        tv_usec: 0,
    };
    unsafe {
        libc::setsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );
    }
    Ok(s)
}

fn request(pid: i32, text: &str, fds: &[OwnedFd]) -> Result<(String, Vec<OwnedFd>), u32> {
    let s = connect(pid)?;
    if !send(s.as_raw_fd(), text, fds) {
        return Err(ERROR_PROCESS_ABORTED);
    }
    let (reply, out) = receive(s.as_raw_fd()).ok_or(ERROR_PROCESS_ABORTED)?;
    match reply.split_once(' ') {
        Some(("OK", rest)) => Ok((rest.to_string(), out)),
        Some(("ERR", code)) => Err(code.trim().parse().unwrap_or(ERROR_NOT_SUPPORTED)),
        _ => Err(ERROR_PROCESS_ABORTED),
    }
}

/// Copie le handle local `h` (droits `access`/`options` déjà appliqués par
/// l'appelant) dans le processus `pid`. `suspended` : enfant pas encore
/// démarré, on choisit son numéro et on dépose sans attendre.
pub(crate) fn put(pid: i32, h: usize, inherit: bool, suspended: bool) -> Result<usize, u32> {
    let (desc, fds) = crate::dlls::inherit::describe(h, false).ok_or(ERROR_NOT_SUPPORTED)?;
    put_described(pid, &desc, fds, inherit, suspended)
}

fn put_described(
    pid: i32,
    desc: &str,
    fds: Vec<OwnedFd>,
    inherit: bool,
    suspended: bool,
) -> Result<usize, u32> {
    if suspended {
        let at = RESERVED.fetch_add(4, Ordering::Relaxed);
        let s = connect(pid)?;
        let text = format!("PUT {} {at:x} {desc}", u8::from(inherit));
        return if send(s.as_raw_fd(), &text, &fds) {
            Ok(at)
        } else {
            Err(ERROR_PROCESS_ABORTED)
        };
    }
    let text = format!("PUT {} 0 {desc}", u8::from(inherit));
    let (reply, _) = request(pid, &text, &fds)?;
    hex(reply.trim()).ok_or(ERROR_PROCESS_ABORTED)
}

/// Reprend chez nous une copie du handle `h` du processus `pid` (et l'y
/// ferme si DUPLICATE_CLOSE_SOURCE). Rend le numéro local.
pub(crate) fn get(
    pid: i32,
    h: usize,
    access: u32,
    options: u32,
    inherit: bool,
) -> Result<usize, u32> {
    let (desc, fds) = request(pid, &format!("GET {h:x} {access:x} {options:x}"), &[])?;
    let mut slots: Vec<Option<OwnedFd>> = fds.into_iter().map(Some).collect();
    let local = crate::dlls::inherit::adopt_one(&zero_handle(&desc), 0, |f| take_fd(&mut slots, f))
        .ok_or(ERROR_NOT_SUPPORTED)?;
    crate::dlls::inherit::set(local, inherit);
    Ok(local)
}

/// Copie le handle `h` du processus `src` dans le processus `dst`, sans
/// passer par notre table (Chromium : d'un enfant à un autre).
pub(crate) fn forward(
    src: i32,
    h: usize,
    dst: i32,
    access: u32,
    options: u32,
    inherit: bool,
    dst_suspended: bool,
) -> Result<usize, u32> {
    let (desc, fds) = request(src, &format!("GET {h:x} {access:x} {options:x}"), &[])?;
    put_described(dst, &desc, fds, inherit, dst_suspended)
}

/// Ferme le handle `h` dans le processus `pid` (DuplicateHandle avec
/// DUPLICATE_CLOSE_SOURCE et sans destination).
pub(crate) fn close_remote(pid: i32, h: usize) -> Result<(), u32> {
    request(pid, &format!("CLOSE {h:x}"), &[]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numero_de_l_expediteur_efface() {
        assert_eq!(zero_handle("m,1a2b,#0,4096,f001f"), "m,0,#0,4096,f001f");
        assert_eq!(zero_handle("p,10004,77"), "p,0,77");
    }

    #[test]
    fn descripteurs_par_scm_rights() {
        let mut pair = [0; 2];
        assert_eq!(
            unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_SEQPACKET, 0, pair.as_mut_ptr()) },
            0
        );
        let (a, b) = unsafe { (OwnedFd::from_raw_fd(pair[0]), OwnedFd::from_raw_fd(pair[1])) };
        let mut p = [0; 2];
        assert_eq!(unsafe { libc::pipe(p.as_mut_ptr()) }, 0);
        let (r, w) = unsafe { (OwnedFd::from_raw_fd(p[0]), OwnedFd::from_raw_fd(p[1])) };
        assert!(send(a.as_raw_fd(), "PUT 0 0 f,0,#0,0", &[w]));
        let (text, fds) = receive(b.as_raw_fd()).unwrap();
        assert_eq!(text, "PUT 0 0 f,0,#0,0");
        assert_eq!(fds.len(), 1);
        // Le descripteur reçu écrit bien dans le même tube.
        assert_eq!(
            unsafe { libc::write(fds[0].as_raw_fd(), b"x".as_ptr().cast(), 1) },
            1
        );
        let mut c = 0u8;
        assert_eq!(
            unsafe { libc::read(r.as_raw_fd(), (&mut c as *mut u8).cast(), 1) },
            1
        );
        assert_eq!(c, b'x');
        let mut slots: Vec<Option<OwnedFd>> = fds.into_iter().map(Some).collect();
        assert!(take_fd(&mut slots, "#0").is_some());
        assert!(take_fd(&mut slots, "#0").is_none());
    }
}
