//! Images disque montées comme un lecteur : un double-clic sur un `.iso`,
//! `.img`, `.vhd(x)`, `.qcow2`, `.vmdk`, `.vdi` ou `.dmg` l'attache (boucle
//! `loop` du noyau ou `qemu-nbd`), monte chaque partition reconnue et lui
//! donne une lettre de lecteur dans « Ce PC », avec un bouton Éjecter.
//!
//! Garde-fous : un format n'est proposé que si l'outil et le module noyau
//! nécessaires sont présents ; lecture seule pour ISO et DMG, pour une image
//! non inscriptible, ou si le système de fichiers refuse l'écriture.

use crate::archive::tools::find;
use crate::archive::util::{ArcError, Progress, Res};
use crate::mounts::{self, Mount};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Périphérique boucle du noyau (image brute : iso, img).
    Loop,
    /// `qemu-nbd` avec le format qemu indiqué.
    Nbd(&'static str),
    /// Image Apple : qemu-nbd (pilote dmg) ou conversion dmg2img.
    Dmg,
}

pub fn backend_for(name: &str) -> Option<Backend> {
    let n = name.to_lowercase();
    let ext = n.rsplit('.').next()?;
    Some(match ext {
        "iso" | "img" | "raw" | "ima" => Backend::Loop,
        "vhd" => Backend::Nbd("vpc"),
        "vhdx" => Backend::Nbd("vhdx"),
        "qcow2" => Backend::Nbd("qcow2"),
        "qcow" => Backend::Nbd("qcow"),
        "vmdk" => Backend::Nbd("vmdk"),
        "vdi" => Backend::Nbd("vdi"),
        "dmg" => Backend::Dmg,
        _ => return None,
    })
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn module_ready(name: &str, dev_hint: &str, args: &[&str]) -> bool {
    if Path::new(&format!("/sys/module/{name}")).exists() || Path::new(dev_hint).exists() {
        return true;
    }
    if !is_root() {
        return false;
    }
    find(&["modprobe"]).is_some_and(|m| {
        Command::new(m)
            .arg(name)
            .args(args)
            .status()
            .is_ok_and(|s| s.success())
    })
}

fn loop_ready() -> bool {
    if is_root() {
        find(&["losetup"]).is_some()
            && find(&["mount"]).is_some()
            && module_ready("loop", "/dev/loop-control", &[])
    } else {
        find(&["udisksctl"]).is_some()
    }
}

fn nbd_ready() -> bool {
    is_root() && find(&["qemu-nbd"]).is_some() && module_ready("nbd", "/dev/nbd0", &["max_part=16"])
}

/// Possibilités de montage : `None` si impossible sur cette machine,
/// sinon `Some(lecture_écriture_possible)`.
pub fn support(path: &Path) -> Option<bool> {
    let b = backend_for(&path.to_string_lossy())?;
    let ok = match b {
        Backend::Loop => loop_ready(),
        Backend::Nbd(_) => nbd_ready(),
        Backend::Dmg => nbd_ready() || (find(&["dmg2img"]).is_some() && loop_ready()),
    };
    if !ok {
        return None;
    }
    let iso = path.to_string_lossy().to_lowercase().ends_with(".iso");
    Some(!iso && b != Backend::Dmg && writable(path))
}

fn writable(p: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

fn run(cmd: &mut Command) -> Res<String> {
    let o = cmd.output()?;
    if !o.status.success() {
        let e = String::from_utf8_lossy(&o.stderr).trim().to_string();
        return Err(ArcError::Io(std::io::Error::other(if e.is_empty() {
            format!("{cmd:?} a échoué")
        } else {
            e
        })));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn sys_block() -> PathBuf {
    std::env::var_os("WINLUX_SYS_BLOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/sys/block"))
}

/// Partitions d'un périphérique (`loop0` → `/dev/loop0p1`…), triées.
pub fn partitions(dev: &str) -> Vec<String> {
    let base = dev.trim_start_matches("/dev/");
    let mut parts: Vec<(u32, String)> = std::fs::read_dir(sys_block().join(base))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    let num = n.strip_prefix(base)?.trim_start_matches('p');
                    let k: u32 = num.parse().ok()?;
                    Some((k, format!("/dev/{n}")))
                })
                .collect()
        })
        .unwrap_or_default();
    parts.sort();
    parts.into_iter().map(|p| p.1).collect()
}

fn settle() {
    if let Some(u) = find(&["udevadm"]) {
        let _ = Command::new(u).args(["settle", "--timeout=5"]).status();
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
}

fn free_nbd() -> Option<String> {
    (0..16).find_map(|i| {
        let d = sys_block().join(format!("nbd{i}"));
        let size = std::fs::read_to_string(d.join("size")).ok()?;
        (size.trim() == "0" && !d.join("pid").exists()).then(|| format!("/dev/nbd{i}"))
    })
}

fn label_of(dev: &str) -> Option<String> {
    let b = find(&["blkid"])?;
    let o = Command::new(b)
        .args(["-o", "value", "-s", "LABEL", dev])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn media_dir() -> PathBuf {
    mounts::state_dir().join("media")
}

fn safe_dir_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect();
    if t.is_empty() || t == "." || t == ".." {
        "image".into()
    } else {
        t
    }
}

/// Analyse « Mapped file X as /dev/loop0. » et « Mounted /dev/x at /media/y ».
pub fn parse_udisks(out: &str, key: &str) -> Option<String> {
    let i = out.find(key)? + key.len();
    let line = out[i..].lines().next()?.trim().trim_end_matches('.');
    (!line.is_empty()).then(|| line.to_string())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Image".into())
}

/// Monte l'image et renvoie les lettres attribuées (« E: F: »).
pub fn mount(path: &Path, progress: &Progress) -> Res<String> {
    let path = std::fs::canonicalize(path)?;
    let src = path.to_string_lossy().into_owned();
    prune_stale();
    let already: Vec<String> = mounts::list()
        .into_iter()
        .filter(|m| m.kind == "image" && m.source == src)
        .map(|m| format!("{}:", m.letter))
        .collect();
    if !already.is_empty() {
        return Ok(already.join(" "));
    }
    let backend =
        backend_for(&src).ok_or_else(|| ArcError::Unsupported("image inconnue".into()))?;
    let rw = support(&path)
        .ok_or_else(|| ArcError::Unsupported("outil ou module noyau de montage absent".into()))?;
    progress.set_total(4);
    let use_udisks = !is_root() && backend == Backend::Loop;
    // 1. Attacher l'image à un périphérique bloc.
    let (dev, kind) = if use_udisks {
        let mut c = Command::new(find(&["udisksctl"]).unwrap());
        c.args(["loop-setup", "--no-user-interaction", "-f"])
            .arg(&path);
        if !rw {
            c.arg("-r");
        }
        let out = run(&mut c)?;
        let dev = parse_udisks(&out, " as ")
            .ok_or_else(|| ArcError::Io(std::io::Error::other(out.clone())))?;
        (dev, "udisks")
    } else {
        match backend {
            Backend::Loop => {
                let mut c = Command::new(find(&["losetup"]).unwrap());
                c.args(["-f", "-P", "--show"]);
                if !rw {
                    c.arg("-r");
                }
                (run(c.arg(&path))?, "loop")
            }
            Backend::Nbd(fmt) => attach_nbd(&path, fmt, rw)?,
            Backend::Dmg => {
                if nbd_ready() {
                    attach_nbd(&path, "dmg", false)?
                } else {
                    let img = crate::paths::data_dir()
                        .unwrap_or_else(std::env::temp_dir)
                        .join("dmg")
                        .join(format!("{}.img", stem(&path)));
                    if let Some(p) = img.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    run(Command::new(find(&["dmg2img"]).unwrap())
                        .arg("-s")
                        .arg("-i")
                        .arg(&path)
                        .arg("-o")
                        .arg(&img))?;
                    let mut c = Command::new(find(&["losetup"]).unwrap());
                    (run(c.args(["-f", "-P", "--show", "-r"]).arg(&img))?, "loop")
                }
            }
        }
    };
    progress.add(1)?;
    settle();
    progress.add(1)?;
    // 2. Monter chaque partition (ou le périphérique entier).
    let mut parts = partitions(&dev);
    if parts.is_empty() {
        parts.push(dev.clone());
    }
    let multi = parts.len() > 1;
    let mut letters = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let label = label_of(part).unwrap_or_else(|| {
            if multi {
                format!("{} ({})", stem(&path), i + 1)
            } else {
                stem(&path)
            }
        });
        let (mp, ro) = if use_udisks {
            let mut c = Command::new(find(&["udisksctl"]).unwrap());
            c.args(["mount", "--no-user-interaction", "-b", part]);
            match run(&mut c).ok().and_then(|o| parse_udisks(&o, " at ")) {
                Some(mp) => (PathBuf::from(mp), !rw),
                None => continue,
            }
        } else {
            let base = media_dir();
            std::fs::create_dir_all(&base)?;
            let mut mp = base.join(safe_dir_name(&label));
            let mut k = 2;
            while mp.exists() {
                mp = base.join(format!("{} {k}", safe_dir_name(&label)));
                k += 1;
            }
            std::fs::create_dir_all(&mp)?;
            let mount = find(&["mount"]).unwrap();
            let mut ro = !rw;
            let mut ok = false;
            for attempt_ro in [ro, true] {
                let mut c = Command::new(&mount);
                if attempt_ro {
                    c.args(["-o", "ro"]);
                }
                if run(c.arg(part).arg(&mp)).is_ok() {
                    ok = true;
                    ro = attempt_ro;
                    break;
                }
                if attempt_ro {
                    break;
                }
            }
            if !ok {
                let _ = std::fs::remove_dir(&mp);
                continue;
            }
            (mp, ro)
        };
        let m = Mount {
            letter: 'A',
            kind: "image".into(),
            mountpoint: mp.clone(),
            label,
            read_only: ro,
            source: src.clone(),
            device: part.clone(),
            backend: format!("{kind}:{dev}"),
            state: String::new(),
        };
        match mounts::add_auto(m) {
            Ok(l) => letters.push(format!("{l}:")),
            Err(e) => {
                let _ = unmount_point(kind, part, &mp);
                detach(kind, &dev);
                return Err(e.into());
            }
        }
    }
    progress.add(2)?;
    if letters.is_empty() {
        detach(kind, &dev);
        return Err(ArcError::Unsupported(
            "aucun système de fichiers reconnu dans cette image".into(),
        ));
    }
    Ok(letters.join(" "))
}

fn attach_nbd(path: &Path, fmt: &str, rw: bool) -> Res<(String, &'static str)> {
    let dev = free_nbd()
        .ok_or_else(|| ArcError::Io(std::io::Error::other("aucun périphérique nbd libre")))?;
    let mut c = Command::new(find(&["qemu-nbd"]).unwrap());
    c.arg("-c").arg(&dev).arg("-f").arg(fmt);
    if !rw {
        c.arg("-r");
    }
    run(c.arg(path))?;
    // qemu-nbd rend la main avant que le noyau connaisse la taille.
    for _ in 0..50 {
        let size = std::fs::read_to_string(
            sys_block()
                .join(dev.trim_start_matches("/dev/"))
                .join("size"),
        )
        .unwrap_or_default();
        if size.trim() != "0" && !size.trim().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok((dev, "nbd"))
}

fn unmount_point(kind: &str, part: &str, mp: &Path) -> Res<()> {
    if kind == "udisks" {
        run(Command::new(find(&["udisksctl"]).unwrap()).args([
            "unmount",
            "--no-user-interaction",
            "-b",
            part,
        ]))?;
        return Ok(());
    }
    let umount = find(&["umount"]).ok_or(ArcError::Unsupported("umount absent".into()))?;
    if let Err(e) = run(Command::new(umount).arg(mp)) {
        return Err(ArcError::Io(std::io::Error::other(format!(
            "lecteur en cours d'utilisation ({e})"
        ))));
    }
    let _ = std::fs::remove_dir(mp);
    Ok(())
}

fn detach(kind: &str, dev: &str) {
    let _ = match kind {
        "loop" => find(&["losetup"]).map(|l| Command::new(l).arg("-d").arg(dev).status()),
        "nbd" => find(&["qemu-nbd"]).map(|q| Command::new(q).arg("-d").arg(dev).status()),
        "udisks" => find(&["udisksctl"]).map(|u| {
            Command::new(u)
                .args(["loop-delete", "--no-user-interaction", "-b", dev])
                .status()
        }),
        _ => None,
    };
}

fn split_backend(b: &str) -> (&str, &str) {
    b.split_once(':').unwrap_or((b, ""))
}

/// Éjecte l'image dont fait partie le lecteur `letter` (toutes ses partitions).
pub fn eject(letter: char) -> Res<String> {
    let m = mounts::get(letter)
        .ok_or_else(|| ArcError::Io(std::io::Error::other("lecteur inconnu")))?;
    // Disques branchés (clé USB, NTFS…) : même bouton Éjecter.
    if m.kind == "disk" {
        return crate::volumes::eject(letter).map_err(|e| ArcError::Io(std::io::Error::other(e)));
    }
    if m.kind != "image" {
        return Err(ArcError::Unsupported(
            "ce lecteur n'est pas une image".into(),
        ));
    }
    let siblings: Vec<Mount> = mounts::list()
        .into_iter()
        .filter(|x| x.kind == "image" && x.backend == m.backend)
        .collect();
    let (kind, dev) = split_backend(&m.backend);
    for s in &siblings {
        if is_mounted(&s.mountpoint) {
            unmount_point(kind, &s.device, &s.mountpoint)?;
        }
        mounts::remove(s.letter)?;
    }
    detach(kind, dev);
    Ok(siblings
        .iter()
        .map(|s| format!("{}:", s.letter))
        .collect::<Vec<_>>()
        .join(" "))
}

fn is_mounted(mp: &Path) -> bool {
    let target = mp.to_string_lossy().replace(' ', "\\040");
    std::fs::read_to_string("/proc/self/mounts")
        .map(|t| {
            t.lines()
                .any(|l| l.split(' ').nth(1) == Some(target.as_str()))
        })
        .unwrap_or(true)
}

/// Retire du registre les images qui ne sont plus montées (redémarrage…).
pub fn prune_stale() {
    for m in mounts::list() {
        if m.kind == "image" && !is_mounted(&m.mountpoint) {
            let _ = mounts::remove(m.letter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backends() {
        assert_eq!(backend_for("Ubuntu.ISO"), Some(Backend::Loop));
        assert_eq!(backend_for("w.vhdx"), Some(Backend::Nbd("vhdx")));
        assert_eq!(backend_for("w.vhd"), Some(Backend::Nbd("vpc")));
        assert_eq!(backend_for("mac.dmg"), Some(Backend::Dmg));
        assert_eq!(backend_for("a.zip"), None);
    }

    #[test]
    fn udisks_output() {
        assert_eq!(
            parse_udisks("Mapped file /home/u/a.iso as /dev/loop7.", " as ").as_deref(),
            Some("/dev/loop7")
        );
        assert_eq!(
            parse_udisks("Mounted /dev/loop7 at /media/u/Ubuntu 26.04", " at ").as_deref(),
            Some("/media/u/Ubuntu 26.04")
        );
    }

    #[test]
    fn partition_listing() {
        let _g = mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("winlux-sys-{}", std::process::id()));
        for p in [
            "loop3/loop3p2",
            "loop3/loop3p10",
            "loop3/loop3p1",
            "loop3/queue",
        ] {
            std::fs::create_dir_all(d.join(p)).unwrap();
        }
        std::env::set_var("WINLUX_SYS_BLOCK", &d);
        assert_eq!(
            partitions("/dev/loop3"),
            vec!["/dev/loop3p1", "/dev/loop3p2", "/dev/loop3p10"]
        );
        assert!(partitions("/dev/loop9").is_empty());
        std::env::remove_var("WINLUX_SYS_BLOCK");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Montage réel (root + module loop requis) : `WINLUX_TEST_MOUNT=1`.
    #[test]
    fn real_loop_mount_and_eject() {
        if std::env::var_os("WINLUX_TEST_MOUNT").is_none() || !loop_ready() {
            return;
        }
        let _g = mounts::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("winlux-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("WINLUX_STATE_DIR", d.join("state"));
        let img = d.join("Disque test.img");
        let f = std::fs::File::create(&img).unwrap();
        f.set_len(16 << 20).unwrap();
        drop(f);
        assert!(Command::new("mkfs.ext4")
            .args(["-q", "-L", "WTEST"])
            .arg(&img)
            .status()
            .unwrap()
            .success());
        assert_eq!(support(&img), Some(true));
        let letters = mount(&img, &Progress::default()).unwrap();
        assert_eq!(letters, "D:");
        // Second double-clic : même lettre, pas de second montage.
        assert_eq!(mount(&img, &Progress::default()).unwrap(), "D:");
        let m = mounts::get('D').unwrap();
        assert_eq!(m.label, "WTEST");
        assert!(!m.read_only);
        std::fs::write(m.mountpoint.join("bonjour.txt"), "ok").unwrap();
        assert_eq!(
            crate::paths::to_linux("D:\\bonjour.txt"),
            m.mountpoint.join("bonjour.txt")
        );
        assert_eq!(eject('D').unwrap(), "D:");
        assert!(mounts::get('D').is_none());
        assert!(!m.mountpoint.exists());
        std::env::remove_var("WINLUX_STATE_DIR");
        let _ = std::fs::remove_dir_all(&d);
    }
}
