//! Lancement croisé entre les deux mondes.
//!
//! - Un programme Windows lance une appli Linux (`CreateProcess`,
//!   `ShellExecute`) : le nom est cherché comme sous Windows puis dans le
//!   `PATH` Linux, un lanceur `.desktop` est exécuté d'après sa ligne `Exec=`,
//!   et les arguments qui sont des chemins Windows (`C:\…`) deviennent des
//!   chemins Linux pour que l'appli Linux puisse les ouvrir.
//! - Un script Linux lance un `.exe` (binfmt ou `winlux app.exe …`) : les
//!   arguments qui sont des chemins Linux existants (`/home/…`) deviennent des
//!   chemins Windows (`C:\…`, `Z:\…`) que le programme Windows comprend.
//!   `WINLUX_KEEP_LINUX_PATHS=1` désactive cette conversion.

use std::path::{Path, PathBuf};

/// Dossiers du `PATH` Linux (avec des valeurs par défaut s'il est vide).
fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .filter(|p| !p.is_empty())
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["/usr/local/bin".into(), "/usr/bin".into(), "/bin".into()])
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
}

/// Programme Linux exécutable trouvé dans le `PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    path_dirs()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|p| is_executable(p))
}

/// Le nom ressemble-t-il à un chemin (et non à un simple nom de programme) ?
pub fn looks_like_path(name: &str) -> bool {
    name.contains('\\')
        || name.contains('/')
        || (name.len() >= 2
            && name.as_bytes()[1] == b':'
            && name.as_bytes()[0].is_ascii_alphabetic())
}

/// Dossiers où chercher les lanceurs `.desktop` (spécification XDG).
fn applications_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(d));
    } else if let Some(h) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(h).join(".local/share"));
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(
        system
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    dirs.into_iter().map(|d| d.join("applications")).collect()
}

/// Trouve le programme que `CreateProcess` doit lancer, dans l'ordre de
/// Windows : chemin donné (avec `.exe` ajouté au besoin), dossier du
/// programme en cours, dossier courant, puis le `PATH` Linux (où un nom
/// comme `firefox` ou `notepad.exe` est cherché tel quel puis avec `.exe`).
/// Un nom `xxx.desktop` est aussi cherché dans les dossiers d'applications.
pub fn find_program(name: &str, app_dir: Option<&Path>) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let with_exe = |p: &Path| -> Option<PathBuf> {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        if p.extension().is_none() {
            let mut x = p.as_os_str().to_os_string();
            x.push(".exe");
            let q = PathBuf::from(x);
            if q.is_file() {
                return Some(q);
            }
        }
        None
    };
    if looks_like_path(name) {
        return with_exe(&crate::paths::to_linux(name));
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".desktop") {
        return applications_dirs()
            .into_iter()
            .map(|d| d.join(name))
            .find(|p| p.is_file());
    }
    // Noms Windows (`app.exe` ou `app`) : dossier du programme puis dossier courant.
    let windows_name = if lower.ends_with(".exe") || lower.ends_with(".com") {
        name.to_string()
    } else {
        format!("{name}.exe")
    };
    for dir in app_dir
        .into_iter()
        .map(Path::to_path_buf)
        .chain(std::env::current_dir().ok())
    {
        let p = crate::paths::to_linux(&dir.join(&windows_name).to_string_lossy());
        if p.is_file() {
            return Some(p);
        }
    }
    for dir in path_dirs() {
        let exact = dir.join(name);
        if is_executable(&exact) {
            return Some(exact);
        }
        let exe = dir.join(&windows_name);
        if exe.is_file() {
            return Some(exe);
        }
    }
    None
}

/// Est-ce un programme Windows (relancé dans Winlux) ?
pub fn is_windows_program(p: &Path) -> bool {
    if has_ext(p, "exe") || has_ext(p, "com") || has_ext(p, "scr") {
        return true;
    }
    // Sans extension connue : on regarde la signature MZ, sauf pour un
    // programme Linux exécutable (ELF, script).
    use std::io::Read;
    let mut head = [0u8; 2];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head == b"MZ"
}

/// Programme Linux exécutable (ELF ou script « #! »), quel que soit son
/// nom : un `bash.exe` qui pointe vers `/usr/bin/bash` en est un.
pub fn is_linux_program(p: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 4];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && (&head == b"\x7fELF" || head.starts_with(b"#!"))
}

pub fn is_desktop_entry(p: &Path) -> bool {
    has_ext(p, "desktop")
}

/// Supprime les échappements d'une valeur de chaîne `.desktop` (`\s`, `\n`…).
fn unescape_value(v: &str) -> String {
    let mut out = String::new();
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Découpe une ligne `Exec=` (guillemets doubles, `\"`, `\\`, `\$`, `` \` ``).
fn split_exec(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut arg = String::new();
    let mut has_arg = false;
    let mut quoted = false;
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                quoted = !quoted;
                has_arg = true;
            }
            '\\' if quoted => {
                if let Some(n) = it.next() {
                    arg.push(n);
                }
            }
            c if c.is_whitespace() && !quoted => {
                if has_arg || !arg.is_empty() {
                    out.push(std::mem::take(&mut arg));
                    has_arg = false;
                }
            }
            c => arg.push(c),
        }
    }
    if has_arg || !arg.is_empty() {
        out.push(arg);
    }
    out
}

/// Commande d'un lanceur `.desktop` : son `Exec=` dont les codes `%f %F %u
/// %U` reçoivent les fichiers donnés, et s'il demande un terminal.
pub fn desktop_command(path: &Path, files: &[String]) -> Option<(Vec<String>, bool)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut exec = None;
    let mut terminal = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(v) = line.strip_prefix("Exec=") {
            exec = Some(unescape_value(v.trim()));
        } else if let Some(v) = line.strip_prefix("Terminal=") {
            terminal = v.trim().eq_ignore_ascii_case("true");
        }
    }
    let exec = exec?;
    let mut argv = Vec::new();
    for word in split_exec(&exec) {
        match word.as_str() {
            "%f" | "%u" => {
                argv.extend(files.first().cloned());
            }
            "%F" | "%U" => {
                argv.extend(files.iter().cloned());
            }
            "%i" | "%c" | "%k" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            _ => argv.push(word.replace("%%", "%")),
        }
    }
    if argv.is_empty() {
        return None;
    }
    Some((argv, terminal))
}

/// `C:\Users\…` → `/home/…` pour un argument destiné à une appli Linux.
/// Seuls les chemins absolus avec lettre de lecteur sont convertis ; les
/// options (`/S`, `-v`) et le texte restent intacts.
pub fn linux_arg(arg: &str) -> String {
    let b = arg.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    let unc = arg.starts_with("\\\\?\\") || arg.starts_with("\\??\\");
    if drive || unc {
        crate::paths::to_linux(arg).to_string_lossy().into_owned()
    } else {
        arg.to_string()
    }
}

/// `/home/moi/doc.txt` → `C:\doc.txt` pour un argument donné à un `.exe`
/// depuis Linux. Seuls les chemins absolus existants d'au moins deux
/// niveaux (ou un fichier) sont convertis, pour ne pas toucher aux options
/// Windows comme `/S` ou `/quiet`.
pub fn windows_arg(arg: &str) -> String {
    if !arg.starts_with('/') || arg.starts_with("//") {
        return arg.to_string();
    }
    let p = Path::new(arg);
    let deep = arg.trim_end_matches('/')[1..].contains('/');
    if p.exists() && (deep || p.is_file()) {
        crate::paths::to_windows(p)
    } else {
        arg.to_string()
    }
}

/// Arguments donnés à un `.exe` lancé depuis Linux.
pub fn windows_args(args: &[String]) -> Vec<String> {
    if std::env::var_os("WINLUX_KEEP_LINUX_PATHS").is_some_and(|v| v != "0") {
        return args.to_vec();
    }
    args.iter().map(|a| windows_arg(a)).collect()
}

/// Arguments donnés à une appli Linux lancée depuis un `.exe`.
pub fn linux_args(args: &[String]) -> Vec<String> {
    args.iter().map(|a| linux_arg(a)).collect()
}

/// Lit un lien `file:///…` en chemin Linux (`%20` décodés).
pub fn file_uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri
        .strip_prefix("file://")
        .or_else(|| uri.strip_prefix("FILE://"))?;
    // file://localhost/chemin ou file:///chemin ; file:///C:/… devient C:/…
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8(out).ok()?;
    let t = s.trim_start_matches('/');
    let tb = t.as_bytes();
    if tb.len() >= 2 && tb[1] == b':' && tb[0].is_ascii_alphabetic() {
        Some(crate::paths::to_linux(t))
    } else {
        Some(PathBuf::from(s))
    }
}

/// Schéma d'une adresse (`mailto`, `steam`, `magnet`…), sauf une lettre de
/// lecteur (`C:`).
pub fn uri_scheme(s: &str) -> Option<String> {
    let i = s.find(':')?;
    let scheme = &s[..i];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if scheme.len() < 2
        || !first.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    Some(scheme.to_ascii_lowercase())
}

/// Script donné à un shell Linux qui se fait passer pour Git Bash
/// (`bash.exe -c "…"`) : les chemins Windows (`C:\Users\x`, `C:/x`) et
/// MSYS (`/c/Users/x`, `/cygdrive/c/x`) qu'il contient deviennent des
/// chemins Linux, comme le ferait la couche MSYS de Git pour Windows.
/// Claude Code écrit ainsi l'emplacement de son instantané de shell et du
/// fichier qui rapporte le dossier courant.
pub fn msys_script(s: &str) -> String {
    // `pwd -P >| fichier` : Claude Code relit ce fichier pour suivre le
    // dossier courant et attend un chemin MSYS (`/c/projet`).
    let s = if s.contains("pwd -P >|") {
        let c = crate::paths::drive_c();
        let c = shell_quote(&c.to_string_lossy());
        s.replace(
            "pwd -P >|",
            &format!(
                "{{ __wl=$(pwd -P); case \"$__wl\" in {c}|{c}/*) __wl=\"/c${{__wl#{c}}}\";; *) __wl=\"/z$__wl\";; esac; printf '%s\\n' \"$__wl\"; }} >|"
            ),
        )
    } else {
        s.to_string()
    };
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let prev = if i == 0 { None } else { Some(chars[i - 1]) };
        let boundary = prev.is_none_or(|p| !(p.is_alphanumeric() || p == '_' || p == '.' || p == '/' || p == '\\' || p == '-' || p == '$'));
        if boundary {
            if let Some((len, linux)) = path_token(&chars[i..], prev) {
                out.push_str(&linux);
                i += len;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Mot entre apostrophes pour le shell.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Fin d'un chemin dans un script : guillemet fermant s'il a commencé
/// après un guillemet, sinon espace ou opérateur du shell.
fn path_end(rest: &[char], quote: Option<char>, colon_ends: bool) -> usize {
    rest.iter()
        .position(|&c| match quote {
            Some(q) => c == q || c == '\n',
            None => {
                c.is_whitespace()
                    || matches!(c, ';' | '|' | '&' | '(' | ')' | '<' | '>' | '`' | '"' | '\'')
                    || (colon_ends && c == ':')
            }
        })
        .unwrap_or(rest.len())
}

fn path_token(rest: &[char], prev: Option<char>) -> Option<(usize, String)> {
    let quote = prev.filter(|&p| p == '\'' || p == '"');
    // C:\… ou C:/…
    if rest.len() >= 3
        && rest[0].is_ascii_alphabetic()
        && rest[1] == ':'
        && (rest[2] == '\\' || rest[2] == '/')
    {
        let len = path_end(rest, quote, false);
        let win: String = rest[..len].iter().collect();
        return Some((len, crate::paths::to_linux(&win).to_string_lossy().into_owned()));
    }
    // /cygdrive/c/… ou /c/…
    let letter_at = if rest.starts_with(&['/', 'c', 'y', 'g', 'd', 'r', 'i', 'v', 'e', '/']) {
        10
    } else {
        1
    };
    if rest.first() != Some(&'/') || rest.len() <= letter_at {
        return None;
    }
    let letter = rest[letter_at];
    let after = rest.get(letter_at + 1).copied();
    if !letter.is_ascii_alphabetic()
        || !after.is_none_or(|c| c == '/' || c.is_whitespace() || c == '\'' || c == '"' || c == ';' || c == ':')
    {
        return None;
    }
    // Un vrai dossier Linux d'une lettre (/a, /x…) reste tel quel.
    if letter_at == 1 && Path::new(&format!("/{letter}")).exists() {
        return None;
    }
    let len = path_end(rest, quote, true);
    let tail: String = rest[letter_at + 1..len].iter().collect();
    let win = format!("{}:/{}", letter.to_ascii_uppercase(), tail.trim_start_matches('/'));
    Some((len, crate::paths::to_linux(&win).to_string_lossy().into_owned()))
}

/// Un programme Linux déguisé en `.exe` (lien `bash.exe` → `/usr/bin/bash`)
/// attend les chemins Windows qu'on donnerait à son équivalent Windows.
pub fn is_disguised_linux_tool(p: &Path) -> bool {
    has_ext(p, "exe") && is_linux_program(p)
}

/// Les tests qui modifient `PATH` passent l'un après l'autre.
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("winlux-launch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lit_la_ligne_exec_dun_lanceur_desktop() {
        let d = tmp("desktop");
        let f = d.join("lecteur.desktop");
        std::fs::write(
            &f,
            "[Desktop Entry]\nName=Lecteur\nExec=env A=1 \"/opt/mon lecteur/bin\" --plein-ecran %U %i\nTerminal=false\n[Desktop Action x]\nExec=autre\n",
        )
        .unwrap();
        let (argv, term) =
            desktop_command(&f, &["/a.mkv".to_string(), "/b.mkv".to_string()]).unwrap();
        assert_eq!(
            argv,
            vec![
                "env",
                "A=1",
                "/opt/mon lecteur/bin",
                "--plein-ecran",
                "/a.mkv",
                "/b.mkv"
            ]
        );
        assert!(!term);
        std::fs::write(&f, "[Desktop Entry]\nExec=htop\nTerminal=true\n").unwrap();
        assert_eq!(
            desktop_command(&f, &[]).unwrap(),
            (vec!["htop".to_string()], true)
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn les_chemins_changent_de_monde_mais_pas_les_options() {
        let d = tmp("args");
        let file = d.join("doc.txt");
        std::fs::write(&file, b"x").unwrap();
        let linux = file.to_string_lossy().into_owned();
        let win = windows_arg(&linux);
        assert!(win.starts_with("C:\\") || win.starts_with("Z:\\"), "{win}");
        assert!(win.ends_with("\\doc.txt"));
        assert_eq!(linux_arg(&win), linux);
        for keep in [
            "/S",
            "/quiet",
            "-v",
            "--x=1",
            "texte libre",
            "/chemin/introuvable/x",
        ] {
            assert_eq!(windows_arg(keep), keep);
            assert_eq!(linux_arg(keep), keep);
        }
        assert_eq!(linux_arg("Z:\\etc\\hostname"), "/etc/hostname");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn schemas_et_liens_file() {
        assert_eq!(uri_scheme("mailto:a@b.fr").as_deref(), Some("mailto"));
        assert_eq!(uri_scheme("steam://run/1").as_deref(), Some("steam"));
        assert_eq!(uri_scheme("C:\\x"), None);
        assert_eq!(uri_scheme("pas une adresse"), None);
        assert_eq!(
            file_uri_path("file:///tmp/mon%20fichier.txt").unwrap(),
            PathBuf::from("/tmp/mon fichier.txt")
        );
        assert_eq!(
            file_uri_path("file:///Z:/etc/hostname").unwrap(),
            PathBuf::from("/etc/hostname")
        );
    }

    #[test]
    fn cherche_les_programmes_comme_windows_puis_dans_le_path() {
        let d = tmp("which");
        let bin = d.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("outil-linux");
        std::fs::write(&tool, b"#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(bin.join("Jeu.exe"), b"MZ").unwrap();
        let app = d.join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("aide.exe"), b"MZ").unwrap();
        let _env = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("PATH");
        std::env::set_var("PATH", &bin);
        assert_eq!(find_program("outil-linux", None), Some(tool.clone()));
        assert_eq!(find_program("Jeu", None), Some(bin.join("Jeu.exe")));
        assert_eq!(find_program("aide", Some(&app)), Some(app.join("aide.exe")));
        assert_eq!(find_program("absent-de-partout", None), None);
        match old {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        assert!(is_windows_program(&bin.join("Jeu.exe")));
        assert!(!is_windows_program(&tool));
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn traduit_les_chemins_windows_et_msys_dun_script() {
        let _g = TEST_ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let saved = std::env::var_os("WINLUX_DRIVE_C");
        std::env::set_var("WINLUX_DRIVE_C", "/srv/lecteur-c");
        let s = msys_script("source 'C:\\Windows\\temp\\a b.sh' && cat >| /c/Windows/temp/x-cwd; ls /usr/bin; echo a/c/b");
        assert_eq!(
            s,
            "source '/srv/lecteur-c/Windows/temp/a b.sh' && cat >| /srv/lecteur-c/Windows/temp/x-cwd; ls /usr/bin; echo a/c/b"
        );
        assert_eq!(msys_script("cd /cygdrive/c/projet"), "cd /srv/lecteur-c/projet");
        let pwd = msys_script("pwd -P >| /c/t/cwd");
        assert!(pwd.contains("__wl=\"/c${__wl#'/srv/lecteur-c'}\""), "{pwd}");
        assert!(pwd.ends_with(">| /srv/lecteur-c/t/cwd"), "{pwd}");
        assert_eq!(msys_script("echo https://x.org/c/y"), "echo https://x.org/c/y");
        match saved {
            Some(v) => std::env::set_var("WINLUX_DRIVE_C", v),
            None => std::env::remove_var("WINLUX_DRIVE_C"),
        }
    }

}
