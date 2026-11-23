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

/// Commande d'un outil d'archive (7-Zip, unrar, unar, bsdtar) sous une
/// locale UTF-8 fixe. Ces outils convertissent les noms d'entrée selon la
/// locale héritée : sous C, bsdtar écrit « été » en caractères de
/// remplacement sans erreur et ne relit plus aucun nom accentué (#3282).
/// C.UTF-8 est intégrée à la glibc depuis 2.35, sans fichier de locale.
/// Les messages restent en anglais, ce que `run_archiver` reconnaît.
pub fn archiver(tool: &Path) -> Command {
    let mut c = Command::new(tool);
    c.env("LC_ALL", "C.UTF-8");
    c
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
        if only_refused_links(&err_text) {
            // 7-Zip a refusé lui-même les liens qui sortent du dossier et
            // extrait tout le reste : c'est le résultat voulu (#3325, #3405).
            return Ok(());
        }
        // L'erreur est sur stderr ; stdout ne porte que la progression.
        let last_line = |t: &str| {
            t.lines()
                .rfind(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        };
        let last = last_line(&err_text)
            .or_else(|| last_line(&out_text))
            .unwrap_or_else(|| "échec de l'outil d'archive".into());
        return Err(ArcError::Io(std::io::Error::other(last)));
    }
    Ok(())
}

/// Les seules erreurs de l'outil sont des entrées dangereuses qu'il a
/// refusées en extrayant le reste :
/// - 7-Zip 23, code 2 : « ERROR: Dangerous link path was ignored : nom : cible » (#3405) ;
/// - bsdtar, code 1 : « nom: Path contains '..' », puis « bsdtar: Error exit
///   delayed from previous errors. » (#3411).
fn only_refused_links(err_text: &str) -> bool {
    let lines: Vec<&str> = err_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let refused = |l: &str| {
        l.starts_with("ERROR: Dangerous link path was ignored")
            || l.contains(": Path contains '..'")
    };
    lines.iter().any(|l| refused(l))
        && lines
            .iter()
            .all(|l| refused(l) || *l == "bsdtar: Error exit delayed from previous errors.")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #3405 : la sortie exacte de 7-Zip 23.01 sur un 7z qui contient des
    /// liens vers /etc et « ../../.. ».
    #[test]
    fn liens_refuses_par_7zip_ne_sont_pas_un_echec_3405() {
        let err = "ERROR: Dangerous link path was ignored : ./remonte : ../../..\n\
                   ERROR: Dangerous link path was ignored : ./absolu : /etc\n";
        assert!(only_refused_links(err));
        assert!(!only_refused_links(""));
        assert!(!only_refused_links(
            "ERROR: Dangerous link path was ignored : ./a : /etc\nERROR: Data Error : f.txt\n"
        ));
        assert!(!only_refused_links(
            "ERROR: Can not open the file as archive\n"
        ));
        // #3411 : sortie exacte de bsdtar 3.7 sur un 7z avec « ../evade.txt ».
        assert!(only_refused_links(
            "../evade.txt: Path contains '..': Unknown error -1\n\
             bsdtar: Error exit delayed from previous errors.\n"
        ));
        assert!(!only_refused_links(
            "bsdtar: Error exit delayed from previous errors.\n"
        ));
        assert!(!only_refused_links(
            "../e: Path contains '..': Unknown error -1\nf.txt: Truncated input file\n\
             bsdtar: Error exit delayed from previous errors.\n"
        ));
    }

    /// #3411 : un 7z dont une entrée remonte (« ../evade.txt ») : l'outil la refuse
    /// et extrait le reste ; l'extraction réussit sans elle.
    #[test]
    fn entree_qui_remonte_refusee_sans_echec_total_3411() {
        let Some(b) = bsdtar() else {
            eprintln!("bsdtar absent : test sauté");
            return;
        };
        let d = std::env::temp_dir().join(format!("winlux-pp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("ok.txt"), "ok").unwrap();
        std::fs::write(d.join("ev.txt"), "x").unwrap();
        let arc = d.join("pp.7z");
        let st = archiver(&b)
            .current_dir(&d)
            .args([
                "--format",
                "7zip",
                "-P",
                "-cf",
                "pp.7z",
                "-s",
                ",^ev.txt$,../evade.txt,",
                "ok.txt",
                "ev.txt",
            ])
            .status()
            .unwrap();
        assert!(st.success());
        let out = d.join("sous/out");
        let r = super::super::extract_to(&arc, &out, None, &Progress::default());
        eprintln!("extraction : {:?}", r.as_ref().map_err(|e| e.to_string()));
        assert!(!d.join("sous/evade.txt").exists(), "écrit hors du dossier");
        r.unwrap();
        assert_eq!(std::fs::read_to_string(out.join("ok.txt")).unwrap(), "ok");
        let _ = std::fs::remove_dir_all(&d);
    }
}
