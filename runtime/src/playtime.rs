//! Temps de jeu de l'application Jeux.
//!
//! Quand un programme Direct3D/OpenGL/Vulkan/XInput démarre (`cpu::is_heavy`,
//! le même critère que le mode jeu), le runtime écrit une « séance » dans
//! `~/.local/share/winlux/jeux/sessions/<pid>-<début>.session` puis la
//! rafraîchit toutes les 30 s depuis un fil discret :
//!
//! ```text
//! exe=/home/moi/GOG Games/Mon jeu/jeu.exe
//! start=1762075000
//! last=1762076830
//! ```
//!
//! L'application Jeux (`integration/native/jeux/winlux-games.py`) additionne
//! `last - start` des séances terminées. Le même jeu lancé par double-clic,
//! par le menu Démarrer ou depuis Jeux est donc compté de la même façon.
//! `PLAYTIME=0` dans `~/.config/winlux/jeux.conf` (Paramètres › Jeux) coupe
//! le compteur.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Intervalle entre deux rafraîchissements : au pire 30 s de jeu perdues.
const HEARTBEAT: Duration = Duration::from_secs(30);

fn config_path() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d).join("winlux/jeux.conf"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/winlux/jeux.conf"))
}

/// Le compteur est actif sauf `PLAYTIME=0`.
pub fn enabled_in(text: &str) -> bool {
    !text
        .lines()
        .take(256)
        .filter_map(|line| line.trim().split_once('='))
        .any(|(key, value)| key.trim() == "PLAYTIME" && value.trim() == "0")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Contenu d'une séance ; le chemin ne peut pas casser le format ligne à ligne.
pub fn session_text(exe: &Path, start: u64, last: u64) -> Option<String> {
    let path = exe.to_str()?;
    if path.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(format!("exe={path}\nstart={start}\nlast={last}\n"))
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

/// Démarre le compteur pour le programme courant (jeu ou application 3D).
pub fn start(exe: &Path) {
    let enabled = config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|text| enabled_in(&text))
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let Some(dir) = crate::paths::data_dir().map(|d| d.join("jeux/sessions")) else {
        return;
    };
    let exe = fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let start = now();
    let Some(first) = session_text(&exe, start, start) else {
        return;
    };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join(format!("{}-{start}.session", std::process::id()));
    if write_atomic(&file, &first).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("winlux-temps-de-jeu".into())
        .spawn(move || loop {
            std::thread::sleep(HEARTBEAT);
            match session_text(&exe, start, now()) {
                Some(text) if write_atomic(&file, &text).is_ok() => {}
                _ => return,
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compteur_coupe_seulement_par_playtime_0() {
        assert!(enabled_in(""));
        assert!(enabled_in("PLAYTIME=1\nFOLDER=/jeux\n"));
        assert!(!enabled_in("AUTOSCAN=1\n PLAYTIME = 0 \n"));
        assert!(enabled_in("# PLAYTIME=0 en commentaire\n"));
    }

    #[test]
    fn seance_lisible_ligne_a_ligne() {
        let text = session_text(Path::new("/home/moi/Jeux/Mon jeu/jeu.exe"), 10, 70).unwrap();
        assert_eq!(
            text,
            "exe=/home/moi/Jeux/Mon jeu/jeu.exe\nstart=10\nlast=70\n"
        );
        assert!(session_text(Path::new("/tmp/a\nlast=99999/jeu.exe"), 1, 2).is_none());
    }

    #[test]
    fn seance_ecrite_et_rafraichie() {
        let dir = std::env::temp_dir().join(format!("winlux-temps-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("1-1.session");
        write_atomic(&file, "exe=/a.exe\nstart=1\nlast=1\n").unwrap();
        write_atomic(&file, "exe=/a.exe\nstart=1\nlast=31\n").unwrap();
        assert!(fs::read_to_string(&file).unwrap().contains("last=31"));
        assert!(!file.with_extension("tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
