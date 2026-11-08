//! Mises à jour A/B (#31) : bascule au démarrage entre le système actif et
//! l'emplacement B (`/.winlux-ab/b`), qui contient soit la mise à jour
//! préparée, soit la seule version précédente gardée en réserve.
//!
//! `winlux-ab prepare` (scripts/winlux-ab.py) installe la nouvelle version
//! dans l'emplacement B pendant que le système tourne, sans jamais toucher
//! au système actif, puis écrit `/.winlux-ab/state`. Au démarrage suivant,
//! `/init` appelle `winlux --ab-boot` avant tout autre programme :
//!
//! - `state=prepared` : échange atomique (renameat2 RENAME_EXCHANGE) de
//!   `/usr`, `/var/lib/dpkg`, `/opt/winlux` et `/init` avec leurs pendants
//!   de B, copie des fichiers de `/etc` du paquet, puis `state=trial` ;
//! - `state=trial` : un démarrage interactif de plus sans validation ; au
//!   `TRIAL_BOOTS`-ième, retour automatique sur la version précédente ;
//! - `state=rollback` (bureau en échec, ou retour demandé dans Paramètres) :
//!   retour sur la version précédente.
//!
//! Chaque échange se reconnaît au numéro d'inode noté par `prepare` : une
//! coupure de courant au milieu de la bascule reprend là où elle s'était
//! arrêtée. Code de sortie 10 : le système a changé, `/init` doit se
//! relancer (le script en cours d'exécution est celui de l'autre version).

use std::collections::BTreeMap;
use std::ffi::CString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Démarrages interactifs non validés avant le retour automatique.
pub const TRIAL_BOOTS: u32 = 3;
pub const SWITCHED: i32 = 10;

/// Chemins échangés, relatifs à la racine et à l'emplacement B.
pub const PAIRS: [&str; 4] = ["usr", "var/lib/dpkg", "opt/winlux", "init"];

pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn dir(&self) -> PathBuf {
        self.root.join(".winlux-ab")
    }
    pub fn slot(&self) -> PathBuf {
        self.dir().join("b")
    }
    pub fn state_path(&self) -> PathBuf {
        self.dir().join("state")
    }
}

pub type State = BTreeMap<String, String>;

pub fn read_state(path: &Path) -> State {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Écrit l'état de façon atomique et durable (fichier temporaire, fsync,
/// renommage, fsync du dossier).
pub fn write_state(path: &Path, state: &State) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let mut text = String::new();
    for (k, v) in state {
        text.push_str(&format!("{k}={v}\n"));
    }
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

fn inode(path: &Path) -> Option<u64> {
    std::fs::symlink_metadata(path).ok().map(|m| m.ino())
}

fn exchange(a: &Path, b: &Path) -> std::io::Result<()> {
    let ca = CString::new(a.as_os_str().as_encoded_bytes()).map_err(std::io::Error::other)?;
    let cb = CString::new(b.as_os_str().as_encoded_bytes()).map_err(std::io::Error::other)?;
    let r = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            ca.as_ptr(),
            libc::AT_FDCWD,
            cb.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if r == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Copie récursive des fichiers ordinaires et liens de `from` dans `to`
/// (fichiers de `/etc` du paquet). Chaque fichier est remplacé par
/// renommage, jamais réécrit sur place.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(from) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&source)?;
        if meta.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_tree(&source, &target)?;
        } else if meta.file_type().is_symlink() {
            let link = std::fs::read_link(&source)?;
            let tmp = target.with_extension("winlux-ab");
            let _ = std::fs::remove_file(&tmp);
            std::os::unix::fs::symlink(link, &tmp)?;
            std::fs::rename(&tmp, &target)?;
        } else if meta.is_file() {
            let tmp = target.with_extension("winlux-ab");
            std::fs::copy(&source, &tmp)?;
            std::os::unix::fs::lchown(&tmp, Some(meta.uid()), Some(meta.gid()))?;
            std::fs::File::open(&tmp)?.sync_all()?;
            std::fs::rename(&tmp, &target)?;
        }
    }
    Ok(())
}

/// Amène le système sur la version dont les inodes sont `<side>_ino.*` :
/// `next` (la mise à jour) ou `current` (la version d'avant la mise à jour).
pub fn switch_to(layout: &Layout, state: &State, side: &str) -> Result<bool, String> {
    let slot = layout.slot();
    let mut changed = false;
    for pair in PAIRS {
        let Some(wanted) = state
            .get(&format!("{side}_ino.{pair}"))
            .and_then(|v| v.parse::<u64>().ok())
        else {
            continue;
        };
        let active = layout.root.join(pair);
        let other = slot.join(pair);
        if inode(&active) == Some(wanted) {
            continue;
        }
        if inode(&other) != Some(wanted) {
            return Err(format!(
                "{pair} : version attendue introuvable dans l'emplacement B"
            ));
        }
        exchange(&active, &other).map_err(|e| format!("{pair} : échange impossible ({e})"))?;
        changed = true;
    }
    // Fichiers de /etc du paquet de la version d'arrivée.
    let etc = slot.join(if side == "next" {
        "etc-next"
    } else {
        "etc-current"
    });
    copy_tree(&etc, &layout.root.join("etc")).map_err(|e| format!("/etc : {e}"))?;
    unsafe { libc::sync() };
    Ok(changed)
}

fn interactive(cmdline: &str) -> bool {
    cmdline
        .split_whitespace()
        .any(|a| a == "winlux.interactive=1")
}

/// Une étape de démarrage ; renvoie le code de sortie.
pub fn boot(layout: &Layout, cmdline: &str, log: &mut dyn FnMut(&str)) -> i32 {
    let path = layout.state_path();
    let mut state = read_state(&path);
    let status = state.get("state").cloned().unwrap_or_default();
    let target = match status.as_str() {
        "prepared" => {
            // Le système actif a changé depuis la préparation (paquet
            // installé entre-temps) : la mise à jour préparée est périmée.
            let dpkg = layout.root.join("var/lib/dpkg/status");
            let stamp = std::fs::metadata(&dpkg)
                .map(|m| format!("{}.{}", m.mtime(), m.mtime_nsec()))
                .unwrap_or_default();
            let active_is_current = state
                .get("current_ino.usr")
                .and_then(|v| v.parse::<u64>().ok())
                == inode(&layout.root.join("usr"));
            if active_is_current && state.get("dpkg_status").map(String::as_str) != Some(&stamp) {
                log("mise à jour préparée périmée (paquets modifiés depuis) : abandonnée");
                state.insert("state".into(), "stale".into());
                let _ = write_state(&path, &state);
                return 0;
            }
            "next"
        }
        "trial" => {
            if !interactive(cmdline) {
                return 0;
            }
            let boots = state
                .get("boots")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0)
                + 1;
            if boots < TRIAL_BOOTS {
                state.insert("boots".into(), boots.to_string());
                let _ = write_state(&path, &state);
                log(&format!("version à l'essai : démarrage {boots}"));
                return 0;
            }
            log("version à l'essai jamais validée : retour à la version précédente");
            state.insert("reason".into(), "boot".into());
            "current"
        }
        "rollback" => "current",
        _ => return 0,
    };
    match switch_to(layout, &state, target) {
        Ok(changed) => {
            let next = if target == "next" {
                state.insert("boots".into(), "0".into());
                "trial"
            } else {
                state
                    .entry("reason".into())
                    .or_insert_with(|| "manual".into());
                "rolled-back"
            };
            state.insert("state".into(), next.into());
            if let Err(e) = write_state(&path, &state) {
                log(&format!("état non enregistré : {e}"));
            }
            log(&format!("bascule terminée : {next}"));
            if changed {
                SWITCHED
            } else {
                0
            }
        }
        Err(e) => {
            // Ne jamais bloquer le démarrage : on reste sur ce qui est en place.
            log(&format!("bascule impossible : {e}"));
            state.insert("error".into(), e);
            let _ = write_state(&path, &state);
            0
        }
    }
}

/// `winlux --ab-boot [RACINE]`.
pub fn cli(args: &[String]) -> i32 {
    let root = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| "/".into());
    let layout = Layout { root };
    if !layout.state_path().is_file() {
        return 0;
    }
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let mut log = |m: &str| eprintln!("winlux-ab : {m}");
    boot(&layout, &cmdline, &mut log)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(root: &Path, version: &str) {
        for dir in ["usr/share/winlux", "var/lib/dpkg", "opt/winlux", "etc"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("usr/share/winlux/version"), version).unwrap();
        std::fs::write(root.join("var/lib/dpkg/status"), version).unwrap();
        std::fs::write(root.join("opt/winlux/v"), version).unwrap();
        std::fs::write(root.join("init"), version).unwrap();
    }

    fn version(root: &Path) -> String {
        let parts: Vec<String> = [
            "usr/share/winlux/version",
            "var/lib/dpkg/status",
            "opt/winlux/v",
            "init",
        ]
        .iter()
        .map(|p| std::fs::read_to_string(root.join(p)).unwrap())
        .collect();
        assert!(
            parts.iter().all(|p| *p == parts[0]),
            "système mélangé : {parts:?}"
        );
        parts[0].clone()
    }

    /// Ce que fait `winlux-ab prepare` côté inodes et /etc.
    fn prepare(layout: &Layout) -> State {
        let slot = layout.slot();
        let _ = std::fs::remove_dir_all(layout.dir());
        tree(&slot, "v2");
        std::fs::create_dir_all(slot.join("etc-next/apt")).unwrap();
        std::fs::write(slot.join("etc-next/apt/winlux"), "v2").unwrap();
        std::fs::create_dir_all(slot.join("etc-current/apt")).unwrap();
        std::fs::write(slot.join("etc-current/apt/winlux"), "v1").unwrap();
        let mut state = State::new();
        for pair in PAIRS {
            state.insert(
                format!("next_ino.{pair}"),
                inode(&slot.join(pair)).unwrap().to_string(),
            );
            state.insert(
                format!("current_ino.{pair}"),
                inode(&layout.root.join(pair)).unwrap().to_string(),
            );
        }
        let m = std::fs::metadata(layout.root.join("var/lib/dpkg/status")).unwrap();
        state.insert(
            "dpkg_status".into(),
            format!("{}.{}", m.mtime(), m.mtime_nsec()),
        );
        state.insert("state".into(), "prepared".into());
        write_state(&layout.state_path(), &state).unwrap();
        state
    }

    fn sandbox(tag: &str) -> Layout {
        let root = std::env::temp_dir().join(format!("winlux-ab-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        tree(&root, "v1");
        Layout { root }
    }

    const BOOT: &str = "quiet winlux.interactive=1";

    #[test]
    fn bascule_essai_validation_et_retour_automatique() {
        let layout = sandbox("cycle");
        let mut log = |_: &str| {};
        prepare(&layout);
        assert_eq!(boot(&layout, BOOT, &mut log), SWITCHED);
        assert_eq!(version(&layout.root), "v2");
        assert_eq!(
            version(&layout.slot()),
            "v1",
            "la version précédente reste en réserve"
        );
        assert_eq!(
            std::fs::read_to_string(layout.root.join("etc/apt/winlux")).unwrap(),
            "v2"
        );
        // Démarrages non validés, puis retour automatique.
        for _ in 1..TRIAL_BOOTS {
            assert_eq!(boot(&layout, BOOT, &mut log), 0);
            assert_eq!(version(&layout.root), "v2");
        }
        assert_eq!(boot(&layout, BOOT, &mut log), SWITCHED);
        assert_eq!(version(&layout.root), "v1");
        assert_eq!(
            std::fs::read_to_string(layout.root.join("etc/apt/winlux")).unwrap(),
            "v1"
        );
        let state = read_state(&layout.state_path());
        assert_eq!(
            (state["state"].as_str(), state["reason"].as_str()),
            ("rolled-back", "boot")
        );
        assert_eq!(boot(&layout, BOOT, &mut log), 0, "plus rien à faire");
        let _ = std::fs::remove_dir_all(&layout.root);
    }

    #[test]
    fn coupure_pendant_la_bascule_puis_reprise() {
        let layout = sandbox("coupure");
        let mut log = |_: &str| {};
        let state = prepare(&layout);
        // Coupure après le premier échange : /usr est neuf, le reste ancien.
        exchange(&layout.root.join("usr"), &layout.slot().join("usr")).unwrap();
        assert_eq!(switch_to(&layout, &state, "next"), Ok(true));
        assert_eq!(version(&layout.root), "v2");
        // Retour demandé (bureau en échec), puis démarrage non interactif.
        let mut s = read_state(&layout.state_path());
        s.insert("state".into(), "rollback".into());
        write_state(&layout.state_path(), &s).unwrap();
        assert_eq!(boot(&layout, "quiet", &mut log), SWITCHED);
        assert_eq!(version(&layout.root), "v1");
        assert_eq!(read_state(&layout.state_path())["reason"], "manual");
        let _ = std::fs::remove_dir_all(&layout.root);
    }

    #[test]
    fn preparation_perimee_et_essai_hors_session() {
        let layout = sandbox("perime");
        let mut log = |_: &str| {};
        prepare(&layout);
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(layout.root.join("var/lib/dpkg/status"), "v1").unwrap();
        assert_eq!(boot(&layout, BOOT, &mut log), 0);
        assert_eq!(version(&layout.root), "v1");
        assert_eq!(read_state(&layout.state_path())["state"], "stale");
        // Démarrage de service (non interactif) : l'essai ne compte pas.
        prepare(&layout);
        assert_eq!(boot(&layout, BOOT, &mut log), SWITCHED);
        for _ in 0..5 {
            assert_eq!(boot(&layout, "console=ttyS0", &mut log), 0);
        }
        assert_eq!(version(&layout.root), "v2");
        let _ = std::fs::remove_dir_all(&layout.root);
    }
}
