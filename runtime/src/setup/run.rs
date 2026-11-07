//! Programmes lancés par un installateur (NSIS `Exec`, `ExecWait`,
//! `ExecShell`, `nsExec::…`, Inno Setup `[Run]` et `[UninstallRun]`).
//!
//! Un installateur Windows lance souvent d'autres programmes : un prérequis
//! qu'il transporte (`vc_redist.x64.exe /quiet`, un second installateur),
//! `msiexec /i paquet.msi`, le désinstalleur de la version précédente, un
//! outil de l'application (`app.exe --register`). Winlux les exécute comme
//! Windows : un `.exe` passe par Winlux lui-même (qui reconnaît à son tour
//! un installateur, un bundle Burn ou un désinstalleur), `msiexec`, `reg` et
//! `regedit` sont ceux intégrés au runtime.
//!
//! Ne sont jamais lancés : les programmes Linux (un `timeout` ou un `net` du
//! `PATH` n'est pas l'outil Windows du même nom), les outils système Windows
//! que Winlux ne fournit pas (`cmd`, `powershell`, `netsh`, `sc`,
//! `schtasks`…), les documents et adresses web ouverts « pour voir » à la
//! fin de l'installation. Ils restent notés dans le journal, et le script
//! continue comme si la commande avait réussi.
//!
//! `WINLUX_SETUP_RUN=0` désactive tout lancement ; `WINLUX_SETUP_RUN_TIMEOUT`
//! (secondes, 600 par défaut) borne l'attente d'un programme.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Profondeur maximale d'installateurs imbriqués (un installateur qui en
/// lance un autre, qui en lance un autre…).
const MAX_DEPTH: u32 = 3;

/// Ce qui sera lancé.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// Programme Windows sur le disque, exécuté par Winlux.
    Pe(PathBuf),
    /// Programme intégré au runtime (`msiexec.exe`, `reg.exe`, `regedit.exe`).
    Builtin(&'static str),
}

/// Résultat d'un lancement.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Terminé avec ce code de sortie.
    Exited(i32),
    /// Lancé sans attendre sa fin.
    Started,
    /// Toujours en cours à la fin du délai : laissé tourner.
    TimedOut,
    /// Pas lancé (raison pour le journal) ; le script continue.
    Skipped(String),
    /// Lancement impossible (programme introuvable, erreur système).
    Failed(String),
}

/// Commande décomposée : programme (chemin Windows) et paramètres.
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub program: String,
    pub params: String,
}

/// Outils système Windows absents de Winlux : jamais cherchés sur le disque
/// (le `PATH` Linux contient des homonymes, `timeout`, `net`, `sc`…).
const SYSTEM_TOOLS: &[&str] = &[
    "attrib",
    "bcdedit",
    "cacls",
    "certutil",
    "cmd",
    "cscript",
    "dism",
    "explorer",
    "icacls",
    "net",
    "net1",
    "netsh",
    "ngen",
    "powershell",
    "pwsh",
    "regsvr32",
    "rundll32",
    "sc",
    "schtasks",
    "sfc",
    "shutdown",
    "takeown",
    "taskkill",
    "timeout",
    "tskill",
    "wevtutil",
    "wmic",
    "wscript",
];

fn base_name(program: &str) -> String {
    let name = program
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".com"))
    {
        Some(n) => n.to_string(),
        None => name,
    }
}

/// Lancements autorisés et profondeur courante.
fn allowed() -> Result<u32, String> {
    if std::env::var("WINLUX_SETUP_RUN").is_ok_and(|v| v == "0") {
        return Err("lancement désactivé (WINLUX_SETUP_RUN=0)".into());
    }
    let depth = std::env::var("WINLUX_SETUP_DEPTH")
        .ok()
        .and_then(|d| d.parse::<u32>().ok())
        .unwrap_or(0);
    if depth >= MAX_DEPTH {
        return Err("trop d'installateurs imbriqués".into());
    }
    Ok(depth)
}

fn timeout() -> Duration {
    let secs = std::env::var("WINLUX_SETUP_RUN_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(600);
    Duration::from_secs(secs.max(1))
}

/// Découpe une ligne de commande `"C:\…\app.exe" /x` ou `C:\Program
/// Files\App\app.exe /x` (sans guillemets : préfixe le plus long qui
/// désigne un fichier, comme `CreateProcess`).
pub fn parse(line: &str, workdir: &str) -> Parsed {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix('"') {
        let (program, params) = rest.split_once('"').unwrap_or((rest, ""));
        return Parsed {
            program: program.to_string(),
            params: params.trim().to_string(),
        };
    }
    let words: Vec<&str> = line.split(' ').collect();
    let mut taken = 1;
    for n in 2..=words.len() {
        let candidate = words[..n].join(" ");
        if crate::launch::looks_like_path(&candidate)
            && crate::launch::find_program(&candidate, Some(&crate::paths::to_linux(workdir)))
                .is_some()
        {
            taken = n;
        }
    }
    Parsed {
        program: words[..taken].join(" "),
        params: words[taken..].join(" ").trim().to_string(),
    }
}

/// Paramètres en arguments. `/D=…` (NSIS) et `_?=…` prennent tout le reste
/// de la ligne, espaces compris, comme chez NSIS.
pub fn split_params(params: &str) -> Vec<String> {
    let padded = format!(" {params}");
    let cut = [" /D=", " _?="]
        .iter()
        .filter_map(|m| padded.find(m))
        .min()
        .unwrap_or(params.len());
    let mut args = crate::dlls::kernel32::split_windows_command_line(&params[..cut]);
    let tail = params[cut..].trim();
    if !tail.is_empty() {
        args.push(tail.to_string());
    }
    args
}

/// Trouve ce qu'il faut lancer, ou dit pourquoi on ne le lance pas.
pub fn resolve(program: &str, workdir: &str) -> Result<Target, String> {
    let program = program.trim();
    let base = base_name(program);
    if base.is_empty() {
        return Err("commande vide".into());
    }
    if base == "msiexec" {
        return Ok(Target::Builtin("msiexec.exe"));
    }
    if base == "reg" {
        return Ok(Target::Builtin("reg.exe"));
    }
    if base == "regedit" || base == "regedt32" {
        return Ok(Target::Builtin("regedit.exe"));
    }
    if SYSTEM_TOOLS.contains(&base.as_str()) {
        return Err(format!(
            "outil système Windows absent de Winlux : {program}"
        ));
    }
    let dir = (!workdir.is_empty()).then(|| crate::paths::to_linux(workdir));
    let found = crate::launch::find_program(program, dir.as_deref())
        .ok_or_else(|| format!("programme introuvable : {program}"))?;
    if crate::launch::is_linux_program(&found) || !crate::launch::is_windows_program(&found) {
        return Err(format!("pas un programme Windows : {}", found.display()));
    }
    Ok(Target::Pe(found))
}

/// `ExecShell` / `[Run] shellexec` : un `.exe` est lancé, un `.msi` est
/// installé par msiexec ; documents et adresses ne sont pas ouverts.
pub fn resolve_shell(file: &str, params: &str, workdir: &str) -> Result<(Target, String), String> {
    let lower = file.trim().to_ascii_lowercase();
    if crate::launch::uri_scheme(file).is_some_and(|s| s.len() > 1) {
        return Err(format!("adresse non ouverte : {file}"));
    }
    if lower.ends_with(".msi") {
        let dir = (!workdir.is_empty()).then(|| crate::paths::to_linux(workdir));
        let path = crate::launch::find_program(file, dir.as_deref())
            .ok_or_else(|| format!("paquet introuvable : {file}"))?;
        let win = crate::paths::to_windows(&path);
        return Ok((
            Target::Builtin("msiexec.exe"),
            format!("/i \"{win}\" {params}").trim().to_string(),
        ));
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if !ext.is_empty() && !matches!(ext, "exe" | "com" | "bat" | "cmd") {
        return Err(format!("document non ouvert : {file}"));
    }
    if matches!(ext, "bat" | "cmd") {
        return Err(format!("script de commandes non lancé : {file}"));
    }
    Ok((resolve(file, workdir)?, params.to_string()))
}

/// Lance `target` avec ses paramètres. `capture` : la sortie est rendue
/// (nsExec::ExecToStack / ExecToLog).
pub fn spawn(
    target: &Target,
    params: &str,
    workdir: &str,
    wait: bool,
    capture: bool,
) -> (Outcome, String) {
    let depth = match allowed() {
        Ok(d) => d,
        Err(e) => return (Outcome::Skipped(e), String::new()),
    };
    let runtime = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return (Outcome::Failed(e.to_string()), String::new()),
    };
    let mut cmd = Command::new(runtime);
    match target {
        Target::Pe(p) => cmd.arg(p),
        Target::Builtin(name) => cmd.arg(name),
    };
    cmd.args(split_params(params));
    let dir = crate::paths::to_linux(workdir);
    if !workdir.is_empty() && dir.is_dir() {
        cmd.current_dir(dir);
    }
    cmd.env("WINLUX_SETUP_DEPTH", (depth + 1).to_string())
        .stdin(Stdio::null());
    if capture {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (Outcome::Failed(e.to_string()), String::new()),
    };
    if !wait {
        // Le processus est récupéré en arrière-plan.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return (Outcome::Started, String::new());
    }
    // Sortie lue pendant l'attente : un tube plein bloquerait le programme.
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|mut r| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            buf
        })
    })
    .collect();
    let deadline = Instant::now() + timeout();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Outcome::Exited(status.code().unwrap_or(1)),
            Ok(None) if Instant::now() >= deadline => break Outcome::TimedOut,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => break Outcome::Failed(e.to_string()),
        }
    };
    if outcome == Outcome::TimedOut {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return (outcome, String::new());
    }
    let mut out = Vec::new();
    for r in readers {
        out.extend(r.join().unwrap_or_default());
    }
    (outcome, String::from_utf8_lossy(&out).into_owned())
}

/// Ligne de commande complète (NSIS `Exec`, `ExecWait`, `nsExec`).
pub fn command_line(line: &str, workdir: &str, wait: bool, capture: bool) -> (Outcome, String) {
    let p = parse(line, workdir);
    match resolve(&p.program, workdir) {
        Ok(t) => spawn(&t, &p.params, workdir, wait, capture),
        Err(e) => (Outcome::Skipped(e), String::new()),
    }
}

/// Programme et paramètres séparés (Inno `[Run]`, NSIS `ExecShell`).
pub fn program(file: &str, params: &str, workdir: &str, wait: bool, shell: bool) -> Outcome {
    let resolved = if shell {
        resolve_shell(file, params, workdir)
    } else {
        resolve(file, workdir).map(|t| (t, params.to_string()))
    };
    match resolved {
        Ok((t, params)) => spawn(&t, &params, workdir, wait, false).0,
        Err(e) => Outcome::Skipped(e),
    }
}

/// Phrase de journal pour une commande et son résultat.
pub fn describe(what: &str, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Exited(c) => format!("programme lancé : {what} (code {c})"),
        Outcome::Started => format!("programme lancé sans attendre : {what}"),
        Outcome::TimedOut => {
            format!("programme toujours en cours, installation poursuivie : {what}")
        }
        Outcome::Skipped(why) => format!("programme ignoré : {what} ({why})"),
        Outcome::Failed(why) => format!("programme non lancé : {what} ({why})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupe_les_lignes_de_commande() {
        let p = parse(
            "\"C:\\Program Files\\App\\uninst.exe\" /S _?=C:\\Program Files\\App",
            "",
        );
        assert_eq!(p.program, "C:\\Program Files\\App\\uninst.exe");
        assert_eq!(p.params, "/S _?=C:\\Program Files\\App");
        let p = parse("msiexec /i \"C:\\x.msi\" /qn", "");
        assert_eq!(p.program, "msiexec");
        assert_eq!(p.params, "/i \"C:\\x.msi\" /qn");
        assert_eq!(
            split_params("/S _?=C:\\Program Files\\App"),
            vec!["/S".to_string(), "_?=C:\\Program Files\\App".to_string()]
        );
        assert_eq!(
            split_params("/i \"C:\\Mes paquets\\x.msi\" /qn"),
            vec!["/i", "C:\\Mes paquets\\x.msi", "/qn"]
        );
        assert_eq!(
            split_params("/S /D=C:\\Program Files\\App"),
            vec!["/S", "/D=C:\\Program Files\\App"]
        );
    }

    #[test]
    fn programmes_integres_et_outils_systeme() {
        assert_eq!(
            resolve("msiexec.exe", ""),
            Ok(Target::Builtin("msiexec.exe"))
        );
        assert_eq!(
            resolve("C:\\windows\\system32\\msiexec.exe", ""),
            Ok(Target::Builtin("msiexec.exe"))
        );
        assert_eq!(resolve("REG", ""), Ok(Target::Builtin("reg.exe")));
        for tool in [
            "cmd.exe",
            "C:\\Windows\\System32\\taskkill.exe",
            "timeout",
            "net",
            "sc",
        ] {
            assert!(resolve(tool, "").is_err(), "{tool} ne doit pas être lancé");
        }
    }

    #[test]
    fn un_programme_linux_nest_jamais_lance() {
        let dir = std::env::temp_dir().join(format!("winlux-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let elf = dir.join("outil.exe");
        std::fs::write(&elf, b"\x7fELF\x02\x01\x01").unwrap();
        let pe = dir.join("aide.exe");
        std::fs::write(&pe, b"MZ\x90\0").unwrap();
        let script = dir.join("lisezmoi.txt");
        std::fs::write(&script, b"bonjour").unwrap();
        let elf_s = elf.to_string_lossy().into_owned();
        assert!(resolve(&elf_s, "").is_err());
        assert_eq!(
            resolve(&pe.to_string_lossy(), ""),
            Ok(Target::Pe(pe.clone()))
        );
        assert!(resolve_shell(&script.to_string_lossy(), "", "").is_err());
        assert!(resolve_shell("https://exemple.org/merci", "", "").is_err());
        assert!(resolve("C:\\nulle-part\\absent.exe", "").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn desactivable() {
        // Pas de variable d'environnement modifiée ici (tests en parallèle) :
        // seule la phrase de journal est vérifiée.
        assert!(describe("x.exe", &Outcome::Skipped("y".into())).contains("ignoré"));
        assert!(describe("x.exe", &Outcome::Exited(3)).contains("code 3"));
    }
}
