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
//! - un annuaire : un fichier par fenêtre, nommé par son HWND, qui donne le
//!   processus, le fil et la fenêtre X11. Les fenêtres de premier niveau y
//!   sont toutes, même cachées ou sans fenêtre X11, avec leur classe et leur
//!   titre : `EnumWindows` voit ainsi celles des autres programmes (#2046).
//!   Les contrôles enfants n'y sont que s'ils ont une fenêtre X11, sans
//!   titre (un champ de saisie réécrirait son entrée à chaque touche).
//!
//! Ce qui bouge sans cesse (position, taille, état réduit) n'est pas publié :
//! il se lit sur le serveur X par la fenêtre X11.
//!
//! Sécurité (#2046) : le dossier est privé au compte, un autre compte (et un
//! processus lancé en root, qui a son propre dossier) n'y voit rien. Un
//! autre processus du compte peut lire ces métadonnées, comme sous Windows ;
//! le serveur X les montre déjà à tous ses clients. Il ne peut ni modifier
//! la fenêtre, ni lui envoyer de messages, ni en lire le contenu. Une
//! entrée est lue avec méfiance : taille et nombre bornés, champs mal
//! formés ignorés.

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
    /// Fenêtre X11 gérée par le gestionnaire de fenêtres (cadre de la barre
    /// de menus, sinon `xid`) : clé de l'ordre Z du serveur (#2046).
    pub top: u64,
    /// Parent (contrôle) ou propriétaire (premier niveau), 0 sans.
    pub parent: usize,
    /// `FLAG_*`.
    pub flags: u32,
    /// Démarrage du processus (champ 22 de /proc/PID/stat) : un PID
    /// réutilisé ne ressuscite pas la fenêtre d'un mort. 0 : inconnu.
    pub start: u64,
}

/// Le drapeau « visible » de la fenêtre (WS_VISIBLE).
pub const FLAG_VISIBLE: u32 = 1;
/// Fenêtre de premier niveau (ni contrôle enfant, ni message-only).
pub const FLAG_TOP_LEVEL: u32 = 2;

/// Fenêtre d'un autre processus avec ses noms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub hwnd: usize,
    pub f: Foreign,
    pub class: String,
    pub title: String,
}

/// Bornes de lecture : une entrée forgée ne peut ni épuiser la mémoire ni
/// bloquer le lecteur.
const MAX_ENTRY: u64 = 4096;
const MAX_ENTRIES: usize = 4096;
const MAX_NAME: usize = 256;

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

fn clean(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_NAME)
        .collect()
}

fn entry_text(f: &Foreign, class: &str, title: &str) -> String {
    format!(
        "{} {} {} {} {} {} {} {} {}\n{}\n{}\n",
        f.pid,
        f.tid,
        f.xid,
        f.style,
        f.ex_style,
        f.top,
        f.parent,
        f.flags,
        f.start,
        clean(class),
        clean(title)
    )
}

#[cfg(test)]
fn publish_in(dir: &Path, hwnd: usize, f: Foreign) {
    publish_named_in(dir, hwnd, f, "", "");
}

#[cfg(test)]
fn publish_named_in(dir: &Path, hwnd: usize, f: Foreign, class: &str, title: &str) {
    write_entry(dir, hwnd, f.pid, &entry_text(&f, class, title));
}

fn write_entry(dir: &Path, hwnd: usize, pid: u32, text: &str) {
    let tmp = dir.join(format!(".{hwnd:x}-{pid}"));
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
    read_full_in(dir, hwnd).map(|(f, _, _)| f)
}

/// Entrée complète : numéros, classe, titre. Une entrée trop grosse ou mal
/// formée est ignorée.
fn read_full_in(dir: &Path, hwnd: usize) -> Option<(Foreign, String, String)> {
    let mut text = String::new();
    std::fs::File::open(entry(dir, hwnd))
        .ok()?
        .take(MAX_ENTRY)
        .read_to_string(&mut text)
        .ok()?;
    parse_entry(&text)
}

fn parse_entry(text: &str) -> Option<(Foreign, String, String)> {
    let mut lines = text.split('\n');
    let mut it = lines
        .next()?
        .split_whitespace()
        .map(|s| s.parse::<u64>().ok());
    let pid = u32::try_from(it.next()??).ok()?;
    let tid = u32::try_from(it.next()??).ok()?;
    let xid = it.next()??;
    // Champs absents (entrée d'un Winlux antérieur) : 0.
    let mut more = || it.next().flatten().unwrap_or(0);
    let style = more() as u32;
    let ex_style = more() as u32;
    let top = more();
    let parent = more() as usize;
    let flags = more() as u32;
    let start = more();
    let class = clean(lines.next().unwrap_or(""));
    let title = clean(lines.next().unwrap_or(""));
    Some((
        Foreign {
            pid,
            tid,
            xid,
            style,
            ex_style,
            top,
            parent,
            flags,
            start,
        },
        class,
        title,
    ))
}

/// Démarrage du processus `pid`, en tops d'horloge depuis l'amorçage.
pub fn start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Le nom (champ 2) est entre parenthèses et peut contenir des espaces.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn alive(pid: u32) -> bool {
    pid != 0
        && (unsafe { libc::kill(pid as i32, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

/// Le processus de l'entrée vit-il encore, et est-ce bien le même ?
fn owner_alive(f: &Foreign) -> bool {
    alive(f.pid) && (f.start == 0 || start_time(f.pid).is_none_or(|s| s == f.start))
}

/// Fenêtre `hwnd` d'un autre processus vivant, vue depuis le processus `me`.
/// Une entrée laissée par un processus mort est effacée.
fn lookup_in(dir: &Path, hwnd: usize, me: u32) -> Option<Foreign> {
    lookup_full_in(dir, hwnd, me).map(|l| l.f)
}

fn lookup_full_in(dir: &Path, hwnd: usize, me: u32) -> Option<Listed> {
    let (f, class, title) = read_full_in(dir, hwnd)?;
    if f.pid == me {
        return None;
    }
    if !owner_alive(&f) {
        let _ = std::fs::remove_file(entry(dir, hwnd));
        return None;
    }
    Some(Listed {
        hwnd,
        f,
        class,
        title,
    })
}

/// Entrées lues : relues seulement quand le dossier change (création,
/// renommage, effacement changent sa date). Le suivi d'occultation de
/// Chromium appelle `EnumWindows` à chaque déplacement de fenêtre.
type Cache = Option<(PathBuf, std::time::SystemTime, Vec<Listed>)>;
static CACHE: Mutex<Cache> = Mutex::new(None);

fn read_all_in(dir: &Path) -> Vec<Listed> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut hwnds: Vec<usize> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_str()?;
            if name.starts_with('.') || name.len() > 16 {
                return None;
            }
            usize::from_str_radix(name, 16).ok()
        })
        .filter(|&h| (FIRST_HWND..LAST_HWND).contains(&h))
        .take(MAX_ENTRIES)
        .collect();
    hwnds.sort_unstable();
    hwnds
        .into_iter()
        .filter_map(|hwnd| {
            let (f, class, title) = read_full_in(dir, hwnd)?;
            Some(Listed {
                hwnd,
                f,
                class,
                title,
            })
        })
        .collect()
}

/// Fenêtres des autres processus vivants du compte, par HWND croissant.
/// Les entrées des morts sont effacées au passage.
fn list_in(dir: &Path, me: u32) -> Vec<Listed> {
    let mtime = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
    let all = {
        let mut cache = crate::dlls::lock(&CACHE);
        match (&*cache, mtime) {
            // Une date trop récente ne dit rien : deux changements dans le
            // même tic d'horloge du noyau la laissent identique.
            (Some((d, t, list)), Some(m)) if d == dir && *t == m => list.clone(),
            _ => {
                let list = read_all_in(dir);
                // Une date trop récente ne garantit rien : deux changements
                // dans le même tic d'horloge du noyau la laissent identique.
                let settled = |m: &std::time::SystemTime| {
                    m.elapsed()
                        .is_ok_and(|e| e > std::time::Duration::from_millis(100))
                };
                *cache = mtime
                    .filter(settled)
                    .map(|m| (dir.to_path_buf(), m, list.clone()));
                list
            }
        }
    };
    // Un seul contrôle par processus.
    let mut alive_pids: std::collections::HashMap<(u32, u64), bool> = Default::default();
    all.into_iter()
        .filter(|l| l.f.pid != me)
        .filter(|l| {
            let ok = *alive_pids
                .entry((l.f.pid, l.f.start))
                .or_insert_with(|| owner_alive(&l.f));
            if !ok {
                let _ = std::fs::remove_file(entry(dir, l.hwnd));
            }
            ok
        })
        .collect()
}

/// Dernier texte écrit par HWND de ce processus : une entrée inchangée
/// n'est pas réécrite.
fn written() -> &'static Mutex<std::collections::HashMap<usize, String>> {
    static W: std::sync::OnceLock<Mutex<std::collections::HashMap<usize, String>>> =
        std::sync::OnceLock::new();
    W.get_or_init(Default::default)
}

/// Démarrage de ce processus, lu une fois.
fn my_start() -> u64 {
    static S: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *S.get_or_init(|| start_time(std::process::id()).unwrap_or(0))
}

/// Inscrit (ou met à jour) une fenêtre de ce processus. `f.pid` et
/// `f.start` sont remplis ici.
pub fn publish(hwnd: usize, f: Foreign, class: &str, title: &str) {
    let Some(d) = dir() else {
        return;
    };
    let f = Foreign {
        pid: std::process::id(),
        start: my_start(),
        ..f
    };
    let text = entry_text(&f, class, title);
    let mut w = crate::dlls::lock(written());
    if w.get(&hwnd) == Some(&text) {
        return;
    }
    write_entry(&d, hwnd, f.pid, &text);
    w.insert(hwnd, text);
}

/// Retire une fenêtre détruite de ce processus.
pub fn withdraw(hwnd: usize) {
    crate::dlls::lock(written()).remove(&hwnd);
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

/// Fenêtre d'un autre processus du compte, avec sa classe et son titre.
pub fn foreign_named(hwnd: usize) -> Option<Listed> {
    if hwnd < FIRST_HWND {
        return None;
    }
    lookup_full_in(&dir()?, hwnd, std::process::id())
}

/// Fenêtres de premier niveau des autres processus du compte (#2046).
pub fn foreign_top_levels() -> Vec<Listed> {
    let Some(d) = dir() else {
        return Vec::new();
    };
    list_in(&d, std::process::id())
        .into_iter()
        .filter(|l| l.f.flags & FLAG_TOP_LEVEL != 0)
        .collect()
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

    /// #2046 : classe et titre relus, nettoyés et bornés ; une entrée d'un
    /// Winlux antérieur se lit encore, une entrée mal formée est ignorée.
    #[test]
    fn entree_complete_relue_et_bornee() {
        let d = temp_dir("entree");
        let other = unsafe { libc::getppid() } as u32;
        let f = Foreign {
            pid: other,
            tid: 9,
            xid: 0x1c0_0007,
            style: 0x10cf_0000,
            ex_style: 0x100,
            top: 0x1c0_0006,
            parent: 0x40200,
            flags: FLAG_VISIBLE | FLAG_TOP_LEVEL,
            start: 0,
        };
        let long = "é".repeat(1000);
        publish_named_in(&d, 0x40300, f, "Notepad++", &format!("a\nb\r{long}"));
        let (g, class, title) = read_full_in(&d, 0x40300).unwrap();
        assert_eq!(g, f);
        assert_eq!(class, "Notepad++");
        assert!(title.starts_with("a b "), "{title}");
        assert_eq!(title.chars().count(), MAX_NAME);
        // Entrée d'avant #2046 : cinq nombres, sans noms.
        std::fs::write(entry(&d, 0x40301), format!("{other} 7 5 1 2\n")).unwrap();
        let (old, class, title) = read_full_in(&d, 0x40301).unwrap();
        assert_eq!((old.tid, old.style, old.ex_style, old.flags), (7, 1, 2, 0));
        assert_eq!((class.as_str(), title.as_str()), ("", ""));
        // Mal formées : ignorées sans panique.
        for (h, text) in [
            (0x40302, "abc 7 5\n".to_string()),
            (0x40303, "99999999999 1 2\n".to_string()),
            (0x40304, String::new()),
            (
                0x40305,
                format!("{other} 1 2 3 4 5 6 7 8\n{}", "x".repeat(10_000)),
            ),
        ] {
            std::fs::write(entry(&d, h), text).unwrap();
            let r = read_full_in(&d, h);
            if h == 0x40305 {
                // Fichier géant : lu jusqu'à la borne, nom tronqué.
                assert!(r.is_some_and(|(_, c, _)| c.len() == MAX_NAME));
            } else {
                assert!(r.is_none(), "{h:#x}");
            }
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// #2046 : un PID réutilisé par un autre processus ne ressuscite pas
    /// la fenêtre d'un mort.
    #[test]
    fn pid_reutilise_ne_ressuscite_pas() {
        let d = temp_dir("pid");
        let other = unsafe { libc::getppid() } as u32;
        let start = start_time(other).expect("/proc lisible");
        let me = std::process::id();
        let f = Foreign {
            pid: other,
            start,
            flags: FLAG_TOP_LEVEL,
            ..Default::default()
        };
        publish_in(&d, 0x40400, f);
        assert_eq!(lookup_in(&d, 0x40400, me), Some(f));
        publish_in(
            &d,
            0x40401,
            Foreign {
                start: start + 1,
                ..f
            },
        );
        assert_eq!(lookup_in(&d, 0x40401, me), None);
        assert!(!entry(&d, 0x40401).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// #2046 : la liste rend les fenêtres des autres processus vivants,
    /// jamais les siennes, et efface celles des morts.
    #[test]
    fn liste_des_fenetres_des_autres() {
        let d = temp_dir("liste");
        let other = unsafe { libc::getppid() } as u32;
        let me = std::process::id();
        let top = Foreign {
            pid: other,
            flags: FLAG_TOP_LEVEL | FLAG_VISIBLE,
            ..Default::default()
        };
        publish_named_in(&d, 0x40500, top, "A", "Fenêtre A");
        publish_named_in(
            &d,
            0x40501,
            Foreign {
                flags: 0,
                xid: 3,
                ..top
            },
            "Edit",
            "",
        );
        publish_named_in(&d, 0x40502, Foreign { pid: me, ..top }, "Moi", "");
        publish_named_in(
            &d,
            0x40503,
            Foreign {
                pid: 0x3fff_fff0,
                ..top
            },
            "Mort",
            "",
        );
        // Fichiers étrangers à l'annuaire : ignorés.
        std::fs::write(d.join("compteur"), [0u8; 8]).unwrap();
        std::fs::write(d.join(".40504-1"), "temporaire").unwrap();
        std::fs::write(d.join("1"), format!("{other} 1 1\n")).unwrap();
        let list = list_in(&d, me);
        let hwnds: Vec<usize> = list.iter().map(|l| l.hwnd).collect();
        assert_eq!(hwnds, vec![0x40500, 0x40501]);
        assert_eq!(list[0].title, "Fenêtre A");
        assert!(!entry(&d, 0x40503).exists());
        // Une nouvelle entrée se voit tout de suite.
        publish_named_in(&d, 0x40506, top, "B", "Fenêtre B");
        assert!(list_in(&d, me).iter().any(|l| l.hwnd == 0x40506));
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
            ..Default::default()
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
