//! Outils système facultatifs (7-Zip, unrar, xz, zstd…). Un format qui en
//! dépend n'est proposé que si l'outil est présent sur la machine.

use super::util::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

/// Cherche un exécutable dans le PATH (et les dossiers système usuels).
pub fn find(names: &[&str]) -> Option<PathBuf> {
    if std::env::var_os("WINLUX_NO_ARCHIVE_TOOLS").is_some() {
        return None;
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in ["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
        dirs.push(PathBuf::from(d));
    }
    for n in names {
        for d in &dirs {
            let p = d.join(n);
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

pub fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn sevenzip() -> Option<PathBuf> {
    find(&["7zz", "7z", "7za"])
}
pub fn unrar() -> Option<PathBuf> {
    find(&["unrar"])
}
pub fn unar() -> Option<PathBuf> {
    find(&["unar"])
}
pub fn bsdtar() -> Option<PathBuf> {
    find(&["bsdtar"])
}

/// Décompresse `archive` à travers un filtre (`xz -dc`…) ; la lecture du
/// fichier source alimente la progression.
pub fn filter_read(
    tool: &Path,
    args: &[&str],
    archive: &Path,
    progress: &Progress,
) -> Res<(Child, ChildStdout, std::thread::JoinHandle<()>)> {
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut f = std::fs::File::open(archive)?;
    let p = progress.clone();
    let feeder = std::thread::spawn(move || {
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = match f.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if p.add(n as u64).is_err() || stdin.write_all(&buf[..n]).is_err() {
                break;
            }
        }
    });
    Ok((child, stdout, feeder))
}

/// Compresse vers `out` à travers un filtre (`xz -c`…).
pub fn filter_write(tool: &Path, args: &[&str], out: &Path) -> Res<Child> {
    let f = std::fs::File::create(out)?;
    Ok(Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(f))
        .stderr(Stdio::null())
        .spawn()?)
}

/// Lance un outil d'archive, suit les pourcentages affichés et reconnaît
/// les erreurs de mot de passe.
pub fn run_archiver(mut cmd: Command, has_password: bool, progress: &Progress) -> Res<()> {
    progress.set_total(100);
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut err = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let mut out_text = String::new();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut chunk = Vec::new();
    loop {
        chunk.clear();
        // 7-Zip réécrit la ligne de progression avec des retours arrière.
        let n = reader.read_until(b'\x08', &mut chunk)?;
        if n == 0 {
            break;
        }
        let s = String::from_utf8_lossy(&chunk);
        if let Some(pct) = s
            .split(|c: char| !c.is_ascii_digit() && c != '%')
            .filter_map(|w| w.strip_suffix('%'))
            .filter_map(|w| w.parse::<u64>().ok())
            .next_back()
        {
            let done = progress.done.load(std::sync::atomic::Ordering::Relaxed);
            if pct > done && progress.add(pct - done).is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ArcError::Cancelled);
            }
        }
        if out_text.len() < 64 * 1024 {
            out_text.push_str(&s);
        }
    }
    let status = child.wait()?;
    let err_text = err_thread.join().unwrap_or_default();
    let all = format!("{out_text}\n{err_text}").to_lowercase();
    if all.contains("wrong password")
        || all.contains("incorrect password")
        || all.contains("password is incorrect")
        || all.contains("enter password")
        || all.contains("encrypted")
            && (all.contains("can not open") || all.contains("cannot open"))
        || all.contains("passphrase")
    {
        return Err(if has_password {
            ArcError::BadPassword
        } else {
            ArcError::NeedPassword
        });
    }
    if !status.success() {
        let last = err_text
            .lines()
            .chain(out_text.lines())
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("échec de l'outil d'archive")
            .trim()
            .to_string();
        return Err(ArcError::Io(std::io::Error::other(last)));
    }
    Ok(())
}
