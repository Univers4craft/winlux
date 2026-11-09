//! Fenêtres visibles des autres processus du même compte (#1228).
//!
//! Sous Windows, un HWND vaut pour tous les processus : le processus GPU de
//! Chromium reçoit du navigateur le HWND de sa fenêtre et ANGLE le vérifie
//! par `IsWindow` avant d'y créer sa chaîne d'échange. Winlux numérotait les
//! fenêtres par processus et ne connaissait que les siennes.
//!
//! Deux choses sont donc partagées, dans le dossier privé du compte des
//! objets nommés (`named_objects::dir`, 0700, jamais lisible d'un autre
//! utilisateur), sous-dossier `fenetres` :
//! - un compteur, pour que deux processus ne donnent jamais le même HWND
//!   (chaque processus en réserve un bloc à la fois) ;
//! - un annuaire : un fichier par fenêtre ayant une fenêtre X11, nommé par
//!   son HWND, qui donne le processus, le fil et la fenêtre X11.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Premier HWND donné, comme avant : en dessous, les valeurs spéciales.
pub const FIRST_HWND: usize = 0x0004_0000;
/// Au-delà, on repart du début : un HWND doit tenir dans 31 bits (wow32).
const LAST_HWND: usize = 0x7fff_0000;
/// HWND réservés d'un coup par processus.
const BLOCK: usize = 256;

/// Fenêtre d'un autre processus, lue dans l'annuaire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Foreign {
    pub pid: u32,
    pub tid: u32,
    pub xid: u64,
    /// Styles WS_* et WS_EX_* : le processus GPU de Chromium choisit son
    /// chemin de peinture d'après WS_EX_LAYERED (#1357).
    pub style: u32,
    pub ex_style: u32,
}

fn dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let d = super::named_objects::dir()?.join("fenetres");
        let _ = std::fs::DirBuilder::new().mode(0o700).create(&d);
        d.is_dir().then_some(d)
    })
    .clone()
}

use std::os::unix::fs::DirBuilderExt;

/// Réserve `BLOCK` HWND dans le compteur partagé de `dir` : rend le premier.
fn reserve_in(dir: &Path) -> Option<usize> {
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join("compteur"))
        .ok()?;
    let fd = f.as_raw_fd();
    if unsafe { libc::flock(fd, libc::LOCK_EX) } != 0 {
        return None;
    }
    let mut buf = [0u8; 8];
    let next = match f.read(&mut buf) {
        Ok(8) => usize::from_le_bytes(buf),
        _ => FIRST_HWND,
    };
    let start = if (FIRST_HWND..LAST_HWND - BLOCK).contains(&next) {
        next
    } else {
        FIRST_HWND
    };
    let ok =
        f.seek(SeekFrom::Start(0)).is_ok() && f.write_all(&(start + BLOCK).to_le_bytes()).is_ok();
    unsafe { libc::flock(fd, libc::LOCK_UN) };
    ok.then_some(start)
}

/// Bloc courant du processus : (prochain, fin exclue).
static LOCAL: Mutex<(usize, usize)> = Mutex::new((0, 0));
/// Numérotation de repli, sans dossier partagé.
static FALLBACK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(FIRST_HWND);

/// Nouveau HWND, unique entre les processus du compte.
pub fn alloc_hwnd() -> usize {
    let mut b = crate::dlls::lock(&LOCAL);
    if b.0 >= b.1 {
        match dir().and_then(|d| reserve_in(&d)) {
            Some(start) => *b = (start, start + BLOCK),
            None => {
                return FALLBACK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    let h = b.0;
    b.0 += 1;
    h
}

fn entry(dir: &Path, hwnd: usize) -> PathBuf {
    dir.join(format!("{hwnd:x}"))
}

fn publish_in(dir: &Path, hwnd: usize, f: Foreign) {
    let tmp = dir.join(format!(".{hwnd:x}-{}", f.pid));
    let text = format!("{} {} {} {} {}\n", f.pid, f.tid, f.xid, f.style, f.ex_style);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| file.write_all(text.as_bytes()));
    if written.is_ok() {
        let _ = std::fs::rename(&tmp, entry(dir, hwnd));
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn withdraw_in(dir: &Path, hwnd: usize, pid: u32) {
    // Seulement si l'entrée est encore la nôtre.
    if read_in(dir, hwnd).is_some_and(|f| f.pid == pid) {
        let _ = std::fs::remove_file(entry(dir, hwnd));
    }
}

fn read_in(dir: &Path, hwnd: usize) -> Option<Foreign> {
    let text = std::fs::read_to_string(entry(dir, hwnd)).ok()?;
    let mut it = text.split_whitespace().map(|s| s.parse::<u64>().ok());
    let pid = it.next()?? as u32;
    let tid = it.next()?? as u32;
    let xid = it.next()??;
    // Styles absents (entrée d'un Winlux antérieur) : 0.
    let style = it.next().flatten().unwrap_or(0) as u32;
    let ex_style = it.next().flatten().unwrap_or(0) as u32;
    Some(Foreign {
        pid,
        tid,
        xid,
        style,
        ex_style,
    })
}

fn alive(pid: u32) -> bool {
    pid != 0
        && (unsafe { libc::kill(pid as i32, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

/// Fenêtre `hwnd` d'un autre processus vivant, vue depuis le processus `me`.
/// Une entrée laissée par un processus mort est effacée.
fn lookup_in(dir: &Path, hwnd: usize, me: u32) -> Option<Foreign> {
    let f = read_in(dir, hwnd)?;
    if f.pid == me {
        return None;
    }
    if !alive(f.pid) {
        let _ = std::fs::remove_file(entry(dir, hwnd));
        return None;
    }
    Some(f)
}

/// Inscrit une fenêtre de ce processus qui a une fenêtre X11, ou met à
/// jour ses styles.
pub fn publish(hwnd: usize, tid: u32, xid: u64, style: u32, ex_style: u32) {
    if let Some(d) = dir() {
        publish_in(
            &d,
            hwnd,
            Foreign {
                pid: std::process::id(),
                tid,
                xid,
                style,
                ex_style,
            },
        );
    }
}

/// Retire une fenêtre détruite de ce processus.
pub fn withdraw(hwnd: usize) {
    if let Some(d) = dir() {
        withdraw_in(&d, hwnd, std::process::id());
    }
}

/// Fenêtre d'un autre processus du compte (`None` pour les siennes).
pub fn foreign(hwnd: usize) -> Option<Foreign> {
    if hwnd < FIRST_HWND {
        return None;
    }
    lookup_in(&dir()?, hwnd, std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("winlux-fenetres-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn deux_processus_n_ont_jamais_le_meme_hwnd() {
        let d = temp_dir("compteur");
        let a = reserve_in(&d).unwrap();
        let b = reserve_in(&d).unwrap();
        assert_eq!(a, FIRST_HWND);
        assert_eq!(b, a + BLOCK);
        // Compteur au bout de la plage : retour au début.
        std::fs::write(d.join("compteur"), (LAST_HWND - 1).to_le_bytes()).unwrap();
        assert_eq!(reserve_in(&d).unwrap(), FIRST_HWND);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn fenetre_d_un_autre_processus_dans_l_annuaire() {
        let d = temp_dir("annuaire");
        // Le processus parent du test joue l'autre processus : il est vivant.
        let other = unsafe { libc::getppid() } as u32;
        let me = std::process::id();
        let f = Foreign {
            pid: other,
            tid: 42,
            xid: 0x1c0_0005,
            ..Default::default()
        };
        publish_in(&d, 0x40123, f);
        assert_eq!(lookup_in(&d, 0x40123, me), Some(f));
        // Vue de son propre processus, ce n'est pas une fenêtre étrangère.
        assert_eq!(lookup_in(&d, 0x40123, other), None);
        // Un autre processus ne peut pas retirer l'entrée.
        withdraw_in(&d, 0x40123, me);
        assert_eq!(lookup_in(&d, 0x40123, me), Some(f));
        withdraw_in(&d, 0x40123, other);
        assert_eq!(lookup_in(&d, 0x40123, me), None);
        // Entrée d'un processus mort : ignorée et effacée.
        publish_in(
            &d,
            0x40124,
            Foreign {
                pid: 0x3fff_fff0,
                ..f
            },
        );
        assert_eq!(lookup_in(&d, 0x40124, me), None);
        assert!(!entry(&d, 0x40124).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn is_window_voit_la_fenetre_d_un_autre_processus() {
        let Some(d) = dir() else { return };
        let other = unsafe { libc::getppid() } as u32;
        let h = alloc_hwnd();
        assert_eq!(unsafe { crate::dlls::user32::is_window(h) }, 0);
        publish_in(
            &d,
            h,
            Foreign {
                pid: other,
                tid: 7,
                xid: 1,
                ..Default::default()
            },
        );
        assert_eq!(unsafe { crate::dlls::user32::is_window(h) }, 1);
        let mut pid = 0u32;
        let tid = unsafe { crate::dlls::user32::get_window_thread_process_id(h, &mut pid) };
        assert_eq!((tid, pid), (7, other));
        withdraw_in(&d, h, other);
        assert_eq!(unsafe { crate::dlls::user32::is_window(h) }, 0);
    }

    /// #1357 : le processus GPU de Chromium lit WS_EX_LAYERED sur la
    /// fenêtre surgissante du navigateur pour choisir UpdateLayeredWindow.
    #[test]
    fn styles_de_la_fenetre_d_un_autre_processus() {
        let Some(d) = dir() else { return };
        let other = unsafe { libc::getppid() } as u32;
        let h = alloc_hwnd();
        let f = Foreign {
            pid: other,
            tid: 7,
            xid: 0x1c0_0043,
            style: 0x8400_0000,    // WS_POPUP | WS_CLIPSIBLINGS
            ex_style: 0x0008_0088, // WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW
        };
        publish_in(&d, h, f);
        assert_eq!(lookup_in(&d, h, std::process::id()), Some(f));
        unsafe {
            assert_eq!(
                crate::dlls::user32::get_window_long_ptr_w(h, -20),
                0x0008_0088
            );
            assert_eq!(
                crate::dlls::user32::get_window_long_w(h, -16),
                0x8400_0000u32 as i32
            );
        }
        // Entrée d'un Winlux antérieur, sans styles : 0.
        std::fs::write(entry(&d, h), format!("{other} 7 5\n")).unwrap();
        assert_eq!(read_in(&d, h).map(|f| (f.style, f.ex_style)), Some((0, 0)));
        withdraw_in(&d, h, other);
    }

    #[test]
    fn direct3d_trouve_la_fenetre_x11_d_un_autre_processus() {
        let Some(d) = dir() else { return };
        let other = unsafe { libc::getppid() } as u32;
        let h = alloc_hwnd();
        publish_in(
            &d,
            h,
            Foreign {
                pid: other,
                tid: 7,
                xid: 0x1c0_0042,
                ..Default::default()
            },
        );
        assert_eq!(crate::dlls::gui::native_window(h), Some(0x1c0_0042));
        withdraw_in(&d, h, other);
        assert_eq!(crate::dlls::gui::native_window(h), None);
    }
}

/// Inscrit dans l'annuaire réel une fenêtre jouée par un autre processus
/// (tests des appels GDI vers une fenêtre étrangère, #1345).
#[cfg(test)]
pub(crate) fn publish_for_test(hwnd: usize, f: Foreign) -> bool {
    match dir() {
        Some(d) => {
            publish_in(&d, hwnd, f);
            true
        }
        None => false,
    }
}

#[cfg(test)]
pub(crate) fn withdraw_for_test(hwnd: usize, pid: u32) {
    if let Some(d) = dir() {
        withdraw_in(&d, hwnd, pid);
    }
}
