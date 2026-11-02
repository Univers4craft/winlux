//! Surveillance de dossiers sur inotify : le moteur de `ReadDirectoryChangesW`,
//! `ReadDirectoryChangesExW` et `FindFirstChangeNotification`.
//!
//! Un [`Watcher`] suit un dossier (et ses sous-dossiers si demandé : une
//! surveillance inotify par dossier, ajoutée quand un sous-dossier apparaît),
//! traduit les événements Linux en actions Windows (`FILE_ACTION_*`), apparie
//! les renommages (`IN_MOVED_FROM`/`IN_MOVED_TO` de même cookie → `OLD_NAME`
//! puis `NEW_NAME`) et les garde en attente entre deux appels, comme Windows.
//! Les noms sont relatifs au dossier surveillé, séparés par `\`.

use std::collections::{HashMap, VecDeque};
use std::ffi::{CString, OsStr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub const FILE_ACTION_ADDED: u32 = 1;
pub const FILE_ACTION_REMOVED: u32 = 2;
pub const FILE_ACTION_MODIFIED: u32 = 3;
pub const FILE_ACTION_RENAMED_OLD_NAME: u32 = 4;
pub const FILE_ACTION_RENAMED_NEW_NAME: u32 = 5;

pub const FILTER_FILE_NAME: u32 = 0x1;
pub const FILTER_DIR_NAME: u32 = 0x2;
pub const FILTER_ATTRIBUTES: u32 = 0x4;
pub const FILTER_SIZE: u32 = 0x8;
pub const FILTER_LAST_WRITE: u32 = 0x10;
pub const FILTER_LAST_ACCESS: u32 = 0x20;
pub const FILTER_CREATION: u32 = 0x40;
pub const FILTER_SECURITY: u32 = 0x100;
pub const FILTER_ALL: u32 = 0x17f;

/// Au-delà, les changements les plus anciens sont remplacés par un
/// débordement (`ERROR_NOTIFY_ENUM_DIR` : l'appli relit le dossier).
const MAX_PENDING: usize = 16384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub action: u32,
    /// Chemin relatif au dossier surveillé, séparé par `\`.
    pub name: String,
}

/// Résultat d'une lecture : des enregistrements, ou un débordement.
#[derive(Debug, PartialEq, Eq)]
pub enum Batch {
    Records(Vec<u8>),
    Overflow,
}

pub struct Watcher {
    fd: OwnedFd,
    root: PathBuf,
    subtree: bool,
    filter: u32,
    /// Descripteur de surveillance → dossier relatif (vide = la racine).
    dirs: HashMap<i32, PathBuf>,
    pending: VecDeque<Change>,
    /// Renommages dont on n'a vu que l'ancien nom (cookie → nom, dossier ?).
    moves: Vec<(u32, PathBuf, bool)>,
    overflow: bool,
}

fn mask_for(filter: u32) -> u32 {
    let mut mask = libc::IN_CREATE
        | libc::IN_DELETE
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO
        | libc::IN_MODIFY
        | libc::IN_ATTRIB
        | libc::IN_ONLYDIR
        | libc::IN_EXCL_UNLINK;
    if filter & FILTER_LAST_ACCESS != 0 {
        mask |= libc::IN_ACCESS;
    }
    mask
}

fn windows_name(relative: &Path) -> String {
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\\")
}

impl Watcher {
    pub fn new(root: &Path, subtree: bool, filter: u32) -> std::io::Result<Self> {
        if !root.is_dir() {
            return Err(std::io::Error::from_raw_os_error(libc::ENOTDIR));
        }
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut watcher = Watcher {
            fd: unsafe { OwnedFd::from_raw_fd(raw) },
            root: root.to_path_buf(),
            subtree,
            filter,
            dirs: HashMap::new(),
            pending: VecDeque::new(),
            moves: Vec::new(),
            overflow: false,
        };
        watcher.watch(Path::new(""))?;
        Ok(watcher)
    }

    /// Les appels suivants peuvent changer le filtre (comme Windows).
    pub fn set_filter(&mut self, filter: u32) {
        if filter != self.filter {
            self.filter = filter;
            for relative in self.dirs.values().cloned().collect::<Vec<_>>() {
                let _ = self.add_one(&relative);
            }
        }
    }

    fn add_one(&mut self, relative: &Path) -> std::io::Result<i32> {
        let full = self.root.join(relative);
        let path = CString::new(full.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
        let wd = unsafe {
            libc::inotify_add_watch(self.fd.as_raw_fd(), path.as_ptr(), mask_for(self.filter))
        };
        if wd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        self.dirs.insert(wd, relative.to_path_buf());
        Ok(wd)
    }

    /// Surveille un dossier, puis ses sous-dossiers (sans suivre les liens).
    fn watch(&mut self, relative: &Path) -> std::io::Result<()> {
        self.add_one(relative)?;
        if !self.subtree {
            return Ok(());
        }
        let mut stack = vec![relative.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(self.root.join(&dir)) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    let child = dir.join(entry.file_name());
                    if self.add_one(&child).is_ok() {
                        stack.push(child);
                    }
                }
            }
        }
        Ok(())
    }

    fn forget_under(&mut self, relative: &Path) {
        let gone: Vec<i32> = self
            .dirs
            .iter()
            .filter(|(_, d)| d.starts_with(relative) && !relative.as_os_str().is_empty())
            .map(|(wd, _)| *wd)
            .collect();
        for wd in gone {
            unsafe { libc::inotify_rm_watch(self.fd.as_raw_fd(), wd) };
            self.dirs.remove(&wd);
        }
    }

    fn rename_under(&mut self, from: &Path, to: &Path) {
        for dir in self.dirs.values_mut() {
            if let Ok(rest) = dir.strip_prefix(from) {
                *dir = to.join(rest);
            }
        }
    }

    fn push(&mut self, action: u32, relative: &Path) {
        let name = windows_name(relative);
        // Plusieurs écritures de suite sur un même fichier : une seule ligne.
        if action == FILE_ACTION_MODIFIED
            && self
                .pending
                .back()
                .is_some_and(|c| c.action == action && c.name == name)
        {
            return;
        }
        if self.pending.len() >= MAX_PENDING {
            self.pending.clear();
            self.overflow = true;
            return;
        }
        self.pending.push_back(Change { action, name });
    }

    fn name_filter(&self, is_dir: bool) -> bool {
        self.filter
            & if is_dir {
                FILTER_DIR_NAME
            } else {
                FILTER_FILE_NAME
            }
            != 0
    }

    fn handle(&mut self, wd: i32, mask: u32, cookie: u32, name: &OsStr) {
        if mask & libc::IN_Q_OVERFLOW != 0 {
            self.pending.clear();
            self.overflow = true;
            return;
        }
        if mask & libc::IN_IGNORED != 0 {
            self.dirs.remove(&wd);
            return;
        }
        let Some(dir) = self.dirs.get(&wd).cloned() else {
            return;
        };
        if name.is_empty() {
            return; // événement sur le dossier lui-même : déjà vu par son parent
        }
        let relative = dir.join(name);
        let is_dir = mask & libc::IN_ISDIR != 0;
        if mask & libc::IN_CREATE != 0 {
            if is_dir && self.subtree {
                let _ = self.watch(&relative);
            }
            if self.name_filter(is_dir) {
                self.push(FILE_ACTION_ADDED, &relative);
            }
        } else if mask & libc::IN_DELETE != 0 {
            if self.name_filter(is_dir) {
                self.push(FILE_ACTION_REMOVED, &relative);
            }
        } else if mask & libc::IN_MOVED_FROM != 0 {
            self.moves.push((cookie, relative, is_dir));
        } else if mask & libc::IN_MOVED_TO != 0 {
            if let Some(index) = self
                .moves
                .iter()
                .position(|(c, _, _)| *c == cookie && cookie != 0)
            {
                let (_, from, _) = self.moves.remove(index);
                if is_dir {
                    self.rename_under(&from, &relative);
                }
                if self.name_filter(is_dir) {
                    self.push(FILE_ACTION_RENAMED_OLD_NAME, &from);
                    self.push(FILE_ACTION_RENAMED_NEW_NAME, &relative);
                }
            } else {
                // Arrivé d'ailleurs : un ajout, avec son contenu surveillé.
                if is_dir && self.subtree {
                    let _ = self.watch(&relative);
                }
                if self.name_filter(is_dir) {
                    self.push(FILE_ACTION_ADDED, &relative);
                }
            }
        } else if mask & libc::IN_MODIFY != 0 {
            if self.filter & (FILTER_SIZE | FILTER_LAST_WRITE) != 0 {
                self.push(FILE_ACTION_MODIFIED, &relative);
            }
        } else if mask & libc::IN_ATTRIB != 0 {
            if self.filter
                & (FILTER_ATTRIBUTES | FILTER_SECURITY | FILTER_CREATION | FILTER_LAST_WRITE)
                != 0
            {
                self.push(FILE_ACTION_MODIFIED, &relative);
            }
        } else if mask & libc::IN_ACCESS != 0 && self.filter & FILTER_LAST_ACCESS != 0 {
            self.push(FILE_ACTION_MODIFIED, &relative);
        }
    }

    /// Les renommages restés sans destination sont sortis du dossier :
    /// des suppressions pour Windows.
    fn settle_moves(&mut self) {
        for (_, from, is_dir) in std::mem::take(&mut self.moves) {
            if is_dir {
                self.forget_under(&from);
            }
            if self.name_filter(is_dir) {
                self.push(FILE_ACTION_REMOVED, &from);
            }
        }
    }

    /// Lit tout ce qu'inotify a en réserve, sans attendre.
    fn drain(&mut self) {
        let mut buffer = [0u8; 16384];
        loop {
            let n = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };
            if n <= 0 {
                break;
            }
            let mut offset = 0usize;
            let header = std::mem::size_of::<libc::inotify_event>();
            while offset + header <= n as usize {
                let event: libc::inotify_event =
                    unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset) as *const _) };
                let start = offset + header;
                let end = (start + event.len as usize).min(n as usize);
                let raw = &buffer[start..end];
                let name = &raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())];
                self.handle(event.wd, event.mask, event.cookie, OsStr::from_bytes(name));
                offset = end;
            }
        }
    }

    /// Attend un changement au plus `timeout_ms` (-1 : sans limite).
    /// Vrai s'il y a quelque chose à rendre (changements ou débordement).
    pub fn wait(&mut self, timeout_ms: i32) -> bool {
        if self.ready() {
            return true;
        }
        let mut poll = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, timeout_ms) } > 0 {
            self.drain();
            // Un renommage arrive en deux événements : on laisse 2 ms au second.
            if !self.moves.is_empty() {
                let mut again = libc::pollfd {
                    fd: self.fd.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                if unsafe { libc::poll(&mut again, 1, 2) } > 0 {
                    self.drain();
                }
            }
            self.settle_moves();
        }
        self.ready()
    }

    pub fn ready(&self) -> bool {
        self.overflow || !self.pending.is_empty()
    }

    /// Vide les changements en attente (FindNextChangeNotification).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.overflow = false;
    }

    /// Enregistrements `FILE_NOTIFY_INFORMATION` (ou, `extended`,
    /// `FILE_NOTIFY_EXTENDED_INFORMATION`) tenant dans `capacity` octets.
    /// Le reste attend l'appel suivant ; si même le premier ne tient pas, ou
    /// après un débordement, Windows répond `ERROR_NOTIFY_ENUM_DIR`.
    pub fn take(&mut self, capacity: usize, extended: bool) -> Batch {
        if self.overflow {
            self.clear();
            return Batch::Overflow;
        }
        let mut out = Vec::new();
        let mut last = None;
        while let Some(change) = self.pending.front() {
            let record = if extended {
                encode_extended(change, &self.root)
            } else {
                encode(change)
            };
            if out.len() + record.len() > capacity {
                break;
            }
            last = Some(out.len());
            out.extend_from_slice(&record);
            self.pending.pop_front();
        }
        let Some(last) = last else {
            if self.pending.is_empty() {
                return Batch::Records(Vec::new());
            }
            self.clear();
            return Batch::Overflow;
        };
        // Le dernier enregistrement n'a pas de suivant.
        out[last..last + 4].copy_from_slice(&0u32.to_le_bytes());
        Batch::Records(out)
    }
}

fn utf16(name: &str) -> Vec<u8> {
    name.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// `FILE_NOTIFY_INFORMATION` : suivant, action, longueur du nom (octets),
/// nom UTF-16 ; aligné sur 4 octets.
pub fn encode(change: &Change) -> Vec<u8> {
    let name = utf16(&change.name);
    let size = (12 + name.len()).div_ceil(4) * 4;
    let mut record = Vec::with_capacity(size);
    record.extend_from_slice(&(size as u32).to_le_bytes());
    record.extend_from_slice(&change.action.to_le_bytes());
    record.extend_from_slice(&(name.len() as u32).to_le_bytes());
    record.extend_from_slice(&name);
    record.resize(size, 0);
    record
}

fn filetime(seconds: i64, nanos: i64) -> i64 {
    (seconds + 11_644_473_600) * 10_000_000 + nanos / 100
}

/// `FILE_NOTIFY_EXTENDED_INFORMATION` : dates, tailles, attributs et
/// identifiants du fichier en plus (lus après coup ; zéro s'il a disparu).
pub fn encode_extended(change: &Change, root: &Path) -> Vec<u8> {
    use std::os::unix::fs::MetadataExt;
    let name = utf16(&change.name);
    let size = (84 + name.len()).div_ceil(8) * 8;
    let mut record = vec![0u8; size];
    record[0..4].copy_from_slice(&(size as u32).to_le_bytes());
    record[4..8].copy_from_slice(&change.action.to_le_bytes());
    let relative: PathBuf = change.name.split('\\').collect();
    let full = root.join(&relative);
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        let created = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| filetime(d.as_secs() as i64, d.subsec_nanos() as i64))
            .unwrap_or_else(|| filetime(meta.mtime(), meta.mtime_nsec()));
        let fields: [i64; 6] = [
            created,
            filetime(meta.mtime(), meta.mtime_nsec()),
            filetime(meta.ctime(), meta.ctime_nsec()),
            filetime(meta.atime(), meta.atime_nsec()),
            (meta.blocks() * 512) as i64,
            meta.len() as i64,
        ];
        for (index, value) in fields.iter().enumerate() {
            record[8 + index * 8..16 + index * 8].copy_from_slice(&value.to_le_bytes());
        }
        let mut attributes = if meta.is_dir() { 0x10u32 } else { 0x20 };
        if meta.permissions().readonly() {
            attributes |= 1;
        }
        record[56..60].copy_from_slice(&attributes.to_le_bytes());
        record[64..72].copy_from_slice(&(meta.ino() as i64).to_le_bytes());
    }
    if let Some(parent) = full.parent().and_then(|p| std::fs::metadata(p).ok()) {
        record[72..80].copy_from_slice(&(parent.ino() as i64).to_le_bytes());
    }
    record[80..84].copy_from_slice(&(name.len() as u32).to_le_bytes());
    record[84..84 + name.len()].copy_from_slice(&name);
    record
}

/// Relit des enregistrements `FILE_NOTIFY_INFORMATION` (tests).
#[cfg(test)]
pub fn decode(records: &[u8]) -> Vec<Change> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset + 12 <= records.len() {
        let field = |at: usize| u32::from_le_bytes(records[at..at + 4].try_into().unwrap());
        let next = field(offset) as usize;
        let action = field(offset + 4);
        let length = field(offset + 8) as usize;
        let start = offset + 12;
        let end = (start + length).min(records.len());
        let units: Vec<u16> = records[start..end]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        out.push(Change {
            action,
            name: String::from_utf16_lossy(&units),
        });
        if next == 0 {
            break;
        }
        offset += next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("winlux-dirwatch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn collect(watcher: &mut Watcher) -> Vec<Change> {
        let mut all = Vec::new();
        while watcher.wait(150) {
            match watcher.take(65536, false) {
                Batch::Records(r) => all.extend(decode(&r)),
                Batch::Overflow => panic!("débordement inattendu"),
            }
        }
        all
    }

    fn change(action: u32, name: &str) -> Change {
        Change {
            action,
            name: name.into(),
        }
    }

    #[test]
    fn creation_modification_renommage_suppression() {
        let dir = temp("base");
        let mut w = Watcher::new(&dir, false, FILTER_ALL).unwrap();
        std::fs::write(dir.join("a.txt"), b"1").unwrap();
        std::fs::write(dir.join("a.txt"), b"22").unwrap();
        std::fs::rename(dir.join("a.txt"), dir.join("été.txt")).unwrap();
        std::fs::remove_file(dir.join("été.txt")).unwrap();
        let got = collect(&mut w);
        assert_eq!(got.first(), Some(&change(FILE_ACTION_ADDED, "a.txt")));
        assert!(got.contains(&change(FILE_ACTION_MODIFIED, "a.txt")));
        let old = got
            .iter()
            .position(|c| *c == change(FILE_ACTION_RENAMED_OLD_NAME, "a.txt"))
            .expect("ancien nom");
        assert_eq!(
            got[old + 1],
            change(FILE_ACTION_RENAMED_NEW_NAME, "été.txt")
        );
        assert_eq!(got.last(), Some(&change(FILE_ACTION_REMOVED, "été.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sous_arbre_et_filtre() {
        let dir = temp("subtree");
        std::fs::create_dir(dir.join("src")).unwrap();
        let mut w = Watcher::new(&dir, true, FILTER_FILE_NAME).unwrap();
        std::fs::write(dir.join("src/main.rs"), b"fn main(){}").unwrap();
        std::fs::create_dir(dir.join("src/nouveau")).unwrap();
        // Laisse inotify voir le dossier pour qu'il soit surveillé à son tour.
        let mut got = collect(&mut w);
        std::fs::write(dir.join("src/nouveau/b.rs"), b"").unwrap();
        got.extend(collect(&mut w));
        assert_eq!(
            got,
            vec![
                change(FILE_ACTION_ADDED, "src\\main.rs"),
                change(FILE_ACTION_ADDED, "src\\nouveau\\b.rs"),
            ],
            "FILE_NAME seul : ni dossiers ni modifications"
        );
        // Sans sous-arbre : rien de ce qui se passe dans src.
        let mut flat = Watcher::new(&dir, false, FILTER_ALL).unwrap();
        std::fs::write(dir.join("src/autre.rs"), b"").unwrap();
        assert!(collect(&mut flat).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sortie_du_dossier_et_petit_tampon() {
        let dir = temp("small");
        let outside = temp("outside");
        let mut w = Watcher::new(&dir, false, FILTER_ALL).unwrap();
        std::fs::write(dir.join("x"), b"").unwrap();
        std::fs::rename(dir.join("x"), outside.join("x")).unwrap();
        assert!(w.wait(200));
        // Un tampon de 16 octets tient un enregistrement d'un caractère.
        let Batch::Records(first) = w.take(16, false) else {
            panic!()
        };
        assert_eq!(decode(&first), vec![change(FILE_ACTION_ADDED, "x")]);
        let Batch::Records(second) = w.take(16, false) else {
            panic!()
        };
        assert_eq!(decode(&second), vec![change(FILE_ACTION_REMOVED, "x")]);
        // Trop petit même pour un seul : débordement, comme Windows.
        std::fs::write(dir.join("long-nom.txt"), b"").unwrap();
        assert!(w.wait(200));
        assert_eq!(w.take(8, false), Batch::Overflow);
        assert!(!w.ready());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn enregistrement_etendu() {
        let dir = temp("extended");
        std::fs::write(dir.join("f"), b"12345").unwrap();
        let record = encode_extended(&change(FILE_ACTION_MODIFIED, "f"), &dir);
        assert_eq!(record.len() % 8, 0);
        assert_eq!(u32::from_le_bytes(record[4..8].try_into().unwrap()), 3);
        assert_eq!(i64::from_le_bytes(record[48..56].try_into().unwrap()), 5);
        assert_eq!(
            u32::from_le_bytes(record[56..60].try_into().unwrap()) & 0x20,
            0x20
        );
        assert_eq!(u32::from_le_bytes(record[80..84].try_into().unwrap()), 2);
        assert_eq!(&record[84..86], &[b'f', 0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
