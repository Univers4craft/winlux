//! Veille (suspend-to-RAM) et hibernation (suspend-to-disk) par
//! `/sys/power/state`, sans systemd ni outil externe.
//!
//! Reprise propre : avant la transition on prévient le bureau
//! (`/run/winlux/power-event`) et on vide les tampons disque ; au retour on
//! réapplique le profil (le micrologiciel remet parfois la luminosité et le
//! gouverneur à zéro) et on remet le compteur d'inactivité à zéro.
//!
//! Fiabilité : on suit le protocole `/sys/power/wakeup_count` du noyau
//! (un évènement de réveil arrivé pendant la préparation annule la veille au
//! lieu de la perdre), on réessaie quand le noyau répond « occupé », et les
//! scripts de `/usr/lib/winlux/sleep.d` puis `/etc/winlux/sleep.d` sont
//! lancés avant (`pre suspend|hibernate`) et après (`post ...`) la veille :
//! c'est là que les écrans et le pavé tactile sont réappliqués.
//!
//! Hibernation : le noyau reprend l'image avant de monter la racine quand la
//! ligne de commande contient `resume=` et `resume_offset=`. `winlux --power
//! hibernate-setup` prépare donc un fichier d'échange dédié, calcule son
//! décalage physique (FIEMAP) et complète les entrées GRUB installées.

use super::{
    apply_profile_at, read_trim, run_dir_at, set_backlight_power_at, touch_activity_at,
    write_existing, Config,
};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub suspend: bool,
    pub hibernate: bool,
}

/// Ce que le noyau accepte réellement. L'hibernation exige en plus une
/// zone de reprise configurée (`/sys/power/resume` différent de 0:0).
pub fn capabilities_at(root: &Path) -> Capabilities {
    let states = read_trim(&root.join("sys/power/state")).unwrap_or_default();
    let has = |s: &str| states.split_whitespace().any(|v| v == s);
    let resume = read_trim(&root.join("sys/power/resume")).unwrap_or_default();
    Capabilities {
        suspend: has("mem") || has("freeze"),
        hibernate: has("disk") && !resume.is_empty() && resume != "0:0",
    }
}

pub(crate) fn event(root: &Path, what: &str) {
    let dir = run_dir_at(root);
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(
            dir.join("power-event"),
            format!("{what} {}\n", super::now_secs()),
        );
    }
}

/// Scripts de veille, triés par nom ; un script de `/etc` remplace celui de
/// même nom livré dans `/usr/lib`.
pub fn hooks_at(root: &Path) -> Vec<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let mut by_name = std::collections::BTreeMap::new();
    for dir in ["usr/lib/winlux/sleep.d", "etc/winlux/sleep.d"] {
        let Ok(entries) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        for e in entries.flatten() {
            let executable = e
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
            if executable {
                by_name.insert(e.file_name(), e.path());
            }
        }
    }
    by_name.into_values().collect()
}

/// Lance les scripts (`pre`/`post`, `suspend`/`hibernate`), dix secondes au
/// plus chacun : un script bloqué ne doit jamais empêcher la veille.
pub fn run_hooks(root: &Path, phase: &str, kind: &str) -> usize {
    let mut ran = 0;
    for hook in hooks_at(root) {
        let Ok(mut child) = std::process::Command::new(&hook)
            .args([phase, kind])
            .env("WINLUX_POWER_ROOT", root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            continue;
        };
        ran += 1;
        for _ in 0..100 {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    ran
}

/// Arme `/sys/power/wakeup_count`. `Ok(false)` : un réveil est arrivé
/// entre-temps, la veille doit être annulée. Sans ce fichier (noyau sans
/// PM_SLEEP ou faux sysfs incomplet), on continue normalement.
pub fn arm_wakeup_count(root: &Path) -> bool {
    let path = root.join("sys/power/wakeup_count");
    let Some(count) = read_trim(&path) else {
        return true;
    };
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .and_then(|mut f| f.write_all(count.as_bytes()))
        .is_ok()
}

fn write_state(root: &Path, state: &str) -> std::io::Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .open(root.join("sys/power/state"))
        .and_then(|mut f| f.write_all(state.as_bytes()))
}

fn transition(root: &Path, state: &str) -> Result<(), String> {
    let kind = if state == "disk" {
        "hibernate"
    } else {
        "suspend"
    };
    event(root, kind);
    run_hooks(root, "pre", kind);
    // SAFETY: sync(2) n'a pas de précondition.
    unsafe { libc::sync() };
    let mut result = Err(
        "un \u{e9}v\u{e8}nement de r\u{e9}veil est arriv\u{e9} pendant la pr\u{e9}paration"
            .to_string(),
    );
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        if !arm_wakeup_count(root) {
            continue;
        }
        match write_state(root, state) {
            Ok(()) => {
                result = Ok(());
                break;
            }
            // EBUSY : un pilote n'était pas prêt ; EAGAIN/EINVAL après
            // armement : un réveil vient d'arriver. On réessaie.
            Err(e) if matches!(e.raw_os_error(), Some(libc::EBUSY | libc::EAGAIN)) => {
                result = Err(format!("le noyau est occup\u{e9} ({e})"));
            }
            Err(e) => {
                result = Err(format!("le noyau a refus\u{e9} la transition ({e})"));
                break;
            }
        }
    }
    // Retour de veille (ou échec) : rétablir l'écran et le profil.
    set_backlight_power_at(root, true);
    apply_profile_at(root, &Config::load_at(root));
    touch_activity_at(root);
    run_hooks(root, "post", kind);
    event(root, if result.is_ok() { "resume" } else { "failed" });
    result
}

pub fn suspend_at(root: &Path) -> Result<(), String> {
    let states = read_trim(&root.join("sys/power/state")).unwrap_or_default();
    let has = |s: &str| states.split_whitespace().any(|v| v == s);
    if has("mem") {
        transition(root, "mem")
    } else if has("freeze") {
        // Veille logicielle (s2idle) quand le micrologiciel n'a pas de S3.
        transition(root, "freeze")
    } else {
        Err("Ce mat\u{e9}riel n'expose aucune veille dans /sys/power/state".into())
    }
}

pub fn hibernate_at(root: &Path) -> Result<(), String> {
    let caps = capabilities_at(root);
    if !caps.hibernate {
        if !disk_root(root) {
            return Err("Veille prolong\u{e9}e indisponible en session Live".into());
        }
        return Err(
            "Veille prolong\u{e9}e non pr\u{e9}par\u{e9}e : lancez \u{ab} winlux --power hibernate-setup \u{bb}"
                .into(),
        );
    }
    // « platform » laisse l'ACPI couper l'alimentation, sinon « shutdown ».
    let modes = read_trim(&root.join("sys/power/disk")).unwrap_or_default();
    if modes.contains("platform") {
        write_existing(&root.join("sys/power/disk"), "platform");
    } else if modes.contains("shutdown") {
        write_existing(&root.join("sys/power/disk"), "shutdown");
    }
    transition(root, "disk")
}

// ───────────────────── Préparation de l'hibernation ─────────────────────

const PAGE: u64 = 4096;

/// En-tête d'échange Linux version 1 (« SWAPSPACE2 »), sans `mkswap`.
pub fn swap_header(pages: u64) -> Vec<u8> {
    let mut h = vec![0u8; PAGE as usize];
    h[1024..1028].copy_from_slice(&1u32.to_le_bytes()); // version
    let last = u32::try_from(pages.saturating_sub(1)).unwrap_or(u32::MAX);
    h[1028..1032].copy_from_slice(&last.to_le_bytes()); // last_page
                                                        // nr_badpages = 0 ; uuid pseudo-aléatoire pour distinguer les fichiers.
    let seed = super::now_secs() ^ (u64::from(std::process::id()) << 32);
    h[1036..1044].copy_from_slice(&seed.to_le_bytes());
    h[1044..1052].copy_from_slice(&(!seed).to_le_bytes());
    h[1052..1063].copy_from_slice(b"winlux-hibr");
    h[PAGE as usize - 10..].copy_from_slice(b"SWAPSPACE2");
    h
}

/// Décalage physique (en pages) du premier bloc du fichier, via FIEMAP.
fn physical_offset(file: &fs::File) -> Result<u64, String> {
    use std::os::fd::AsRawFd;
    const FS_IOC_FIEMAP: libc::c_ulong = 0xC020_660B;
    const FIEMAP_FLAG_SYNC: u32 = 1;
    #[repr(C)]
    #[derive(Default)]
    struct Extent {
        logical: u64,
        physical: u64,
        length: u64,
        reserved64: [u64; 2],
        flags: u32,
        reserved: [u32; 3],
    }
    #[repr(C)]
    #[derive(Default)]
    struct Fiemap {
        start: u64,
        length: u64,
        flags: u32,
        mapped: u32,
        count: u32,
        reserved: u32,
        extent: Extent,
    }
    let mut map = Fiemap {
        length: PAGE,
        flags: FIEMAP_FLAG_SYNC,
        count: 1,
        ..Default::default()
    };
    // SAFETY: `map` est une structure fiemap valide suivie d'une extension.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), FS_IOC_FIEMAP, &mut map) };
    if rc != 0 || map.mapped == 0 {
        return Err(format!(
            "FIEMAP indisponible sur ce syst\u{e8}me de fichiers ({})",
            std::io::Error::last_os_error()
        ));
    }
    Ok(map.extent.physical / PAGE)
}

/// Ajoute `resume=` et `resume_offset=` aux entrées Winlux de GRUB. Le
/// périphérique de reprise est celui de `root=` (le fichier est sur la
/// racine). Idempotent ; les entrées de secours ne reprennent jamais.
pub fn patch_grub(cfg: &str, offset: u64) -> String {
    let mut out = String::with_capacity(cfg.len() + 128);
    for line in cfg.lines() {
        let trimmed = line.trim_start();
        let is_kernel = trimmed.starts_with("linux ") && trimmed.contains("init=/init");
        if !is_kernel || trimmed.contains("winlux.recovery=1") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let words: Vec<&str> = line
            .split(' ')
            .filter(|w| !w.starts_with("resume=") && !w.starts_with("resume_offset="))
            .collect();
        let mut new = words.join(" ");
        if let Some(root) = words.iter().find_map(|w| w.strip_prefix("root=")) {
            new.push_str(&format!(" resume={root} resume_offset={offset}"));
        }
        out.push_str(&new);
        out.push('\n');
    }
    out
}

/// Vrai si la racine est sur un disque capable d'héberger un fichier
/// d'échange (et non en mémoire comme la session Live).
pub fn disk_root(root: &Path) -> bool {
    let mounts = fs::read_to_string(root.join("proc/mounts")).unwrap_or_default();
    mounts
        .lines()
        .rev()
        .find_map(|l| {
            let mut w = l.split_whitespace();
            let (_, point, kind) = (w.next(), w.next()?, w.next()?);
            (point == "/").then_some(kind)
        })
        .is_some_and(|k| matches!(k, "ext4" | "ext3" | "xfs" | "btrfs" | "f2fs"))
}

fn mem_total_bytes(root: &Path) -> Option<u64> {
    let text = fs::read_to_string(root.join("proc/meminfo")).ok()?;
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// Prépare l'hibernation sur un système installé. Renvoie un résumé.
pub fn hibernate_setup_at(root: &Path) -> Result<String, String> {
    if !disk_root(root) {
        return Err(
            "Veille prolong\u{e9}e indisponible en session Live (racine en m\u{e9}moire)".into(),
        );
    }
    let mem = mem_total_bytes(root).ok_or("Taille de la m\u{e9}moire inconnue")?;
    // L'image est compressée : la taille de la RAM suffit toujours.
    let size = mem.div_ceil(PAGE) * PAGE;
    let path = root.join("var/lib/winlux/hibernate.swap");
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let dir = path.parent().unwrap_or(root);
    let c_dir =
        std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: statvfs est zéroïsable et rempli par le noyau.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: chemin C valide, structure de sortie valide.
    if unsafe { libc::statvfs(c_dir.as_ptr(), &mut st) } == 0 {
        let free = st.f_bavail as u64 * st.f_frsize as u64;
        if free < size + (1 << 30) {
            return Err(format!(
                "Espace libre insuffisant : {} Mio n\u{e9}cessaires",
                (size + (1 << 30)) >> 20
            ));
        }
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("{} : {e}", path.display()))?;
    use std::os::fd::AsRawFd;
    // Réserver tous les blocs : un fichier d'échange ne peut pas avoir de trous.
    // SAFETY: descripteur valide, plage positive.
    let rc = unsafe { libc::fallocate(file.as_raw_fd(), 0, 0, size as libc::off_t) };
    if rc != 0 {
        let zeros = vec![0u8; 1 << 20];
        let mut w = &file;
        let mut done = 0;
        while done < size {
            let n = zeros.len().min((size - done) as usize);
            w.write_all(&zeros[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
        }
    }
    use std::os::unix::fs::FileExt;
    file.write_all_at(&swap_header(size / PAGE), 0)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    let offset = physical_offset(&file)?;
    let dev = fs::metadata(&path).map_err(|e| e.to_string())?.dev();
    let (major, minor) = (libc::major(dev), libc::minor(dev));
    activate_at(root, &path)?;
    write_existing(&root.join("sys/power/resume_offset"), &offset.to_string());
    write_existing(&root.join("sys/power/resume"), &format!("{major}:{minor}"));
    let grub = root.join("boot/grub/grub.cfg");
    let mut grub_note = "GRUB introuvable : ajoutez resume= et resume_offset= au noyau";
    if let Ok(cfg) = fs::read_to_string(&grub) {
        let patched = patch_grub(&cfg, offset);
        let tmp = grub.with_extension("cfg.winlux");
        if fs::write(&tmp, &patched).is_ok() && fs::rename(&tmp, &grub).is_ok() {
            grub_note = "entr\u{e9}es GRUB mises \u{e0} jour";
        }
    }
    let mut config = Config::load_at(root);
    config.hibernate_file = Some("/var/lib/winlux/hibernate.swap".into());
    config.save_at(root).map_err(|e| e.to_string())?;
    Ok(format!(
        "Fichier d'\u{e9}change {} Mio pr\u{ea}t (p\u{e9}riph\u{e9}rique {major}:{minor}, d\u{e9}calage {offset}) ; {grub_note}.",
        size >> 20
    ))
}

/// Active le fichier d'échange d'hibernation (au démarrage par l'init).
pub fn activate_at(root: &Path, path: &Path) -> Result<(), String> {
    let swaps = fs::read_to_string(root.join("proc/swaps")).unwrap_or_default();
    let real = path.to_string_lossy();
    if swaps.lines().any(|l| l.starts_with(real.as_ref())) {
        return Ok(());
    }
    let c =
        std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: chemin C valide. Sans priorité explicite, le noyau donne la
    // plus basse : le fichier ne sert de swap courant qu'en dernier recours.
    if unsafe { libc::swapon(c.as_ptr(), 0) } != 0 {
        return Err(format!(
            "swapon {} : {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
