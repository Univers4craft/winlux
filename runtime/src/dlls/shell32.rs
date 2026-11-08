//! shell32 : dossiers spéciaux et `ShellExecute`, qui ouvre un fichier, un
//! dossier ou une adresse web avec le bon programme du système Linux.
//!
//! - une adresse web (`https://…`, `www.…`, `microsoft-edge:…`) s'ouvre dans
//!   le navigateur choisi par l'utilisateur (voir `browser_candidates`) ;
//! - `mailto:` ouvre le client mail Linux (`xdg-email`), un autre protocole
//!   (`steam:`, `magnet:`, `tel:`…) l'appli Linux qui le gère (`xdg-open`),
//!   et `file:///…` le fichier désigné ;
//! - un lanceur Linux `.desktop` est exécuté d'après sa ligne `Exec=` ;
//! - un exécutable Windows (`.exe`, `.com`) est relancé dans Winlux ;
//! - `shell:terminal` ouvre un terminal Linux ;
//! - un nom de programme sans chemin est cherché dans le `PATH` Linux, et
//!   les chemins Windows de ses paramètres deviennent des chemins Linux ;
//! - une image d'un format réellement décodé par Winlux (`imaging`) s'ouvre
//!   dans la visionneuse maison `imageviewer.exe` ;
//! - tout autre fichier ou dossier passe par `xdg-open`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SE_ERR_FNF: isize = 2;
const SE_ERR_NOASSOC: isize = 31;
const SUCCESS: isize = 42;

fn split_argv_wide(line: &[u16]) -> Vec<Vec<u16>> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let mut arg = Vec::new();
        let mut quoted = false;
        loop {
            let start = i;
            while i < line.len() && line[i] == b'\\' as u16 {
                i += 1;
            }
            let slashes = i - start;
            if i < line.len() && line[i] == b'"' as u16 {
                arg.extend(std::iter::repeat_n(b'\\' as u16, slashes / 2));
                if slashes % 2 == 1 {
                    arg.push(b'"' as u16);
                    i += 1;
                } else if quoted && line.get(i + 1) == Some(&(b'"' as u16)) {
                    arg.push(b'"' as u16);
                    i += 2;
                } else {
                    quoted = !quoted;
                    i += 1;
                }
                continue;
            }
            arg.extend(std::iter::repeat_n(b'\\' as u16, slashes));
            if i == line.len() || !quoted && matches!(line[i], 9 | 32) {
                break;
            }
            arg.push(line[i]);
            i += 1;
        }
        result.push(arg);
        while i < line.len() && matches!(line[i], 9 | 32) {
            i += 1;
        }
    }
    result
}

/// One LocalFree-compatible allocation contains both pointers and UTF-16 data.
pub unsafe extern "win64" fn command_line_to_argv_w(
    line: *const u16,
    count: *mut i32,
) -> *mut *mut u16 {
    if line.is_null() || count.is_null() {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    }
    *count = 0;
    let mut length = 0;
    while length < 32768 && *line.add(length) != 0 {
        length += 1;
    }
    if length == 32768 {
        crate::process::set_last_error(87);
        return std::ptr::null_mut();
    }
    let args = if length == 0 {
        let Some(module) = crate::loader::main_module() else {
            crate::process::set_last_error(2);
            return std::ptr::null_mut();
        };
        vec![crate::paths::to_windows(&module.path)
            .encode_utf16()
            .collect()]
    } else {
        split_argv_wide(std::slice::from_raw_parts(line, length))
    };
    let pointers = (args.len() + 1) * std::mem::size_of::<*mut u16>();
    let bytes = pointers + args.iter().map(|arg| (arg.len() + 1) * 2).sum::<usize>();
    let block = crate::dlls::kernel32::heap_alloc(0, 0, bytes) as *mut u8;
    if block.is_null() {
        crate::process::set_last_error(8);
        return std::ptr::null_mut();
    }
    let argv = block.cast::<*mut u16>();
    *argv.add(args.len()) = std::ptr::null_mut();
    let mut cursor = block.add(pointers).cast::<u16>();
    for (index, arg) in args.iter().enumerate() {
        *argv.add(index) = cursor;
        std::ptr::copy_nonoverlapping(arg.as_ptr(), cursor, arg.len());
        *cursor.add(arg.len()) = 0;
        cursor = cursor.add(arg.len() + 1);
    }
    *count = args.len() as i32;
    argv
}

/// Programme Linux trouvé dans le `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    crate::launch::which(name)
}

/// Navigateurs essayés dans l'ordre : `WINLUX_BROWSER`, puis le lanceur Winlux
/// (le navigateur choisi par l'utilisateur, docs/BROWSERS.md). Sans lanceur,
/// un navigateur présent, sans en favoriser aucun (ordre alphabétique de
/// l'éditeur), puis `xdg-open`.
pub fn browser_candidates() -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(b) = std::env::var("WINLUX_BROWSER") {
        if !b.is_empty() {
            list.push(b);
        }
    }
    for name in [
        "winlux-browser",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "microsoft-edge-stable",
        "microsoft-edge",
        "firefox",
        "xdg-open",
    ] {
        list.push(name.to_string());
    }
    list
}

fn terminal_candidates() -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(t) = std::env::var("WINLUX_TERMINAL") {
        if !t.is_empty() {
            list.push(t);
        }
    }
    for name in [
        "winlux-terminal",
        "x-terminal-emulator",
        "konsole",
        "gnome-terminal",
        "kgx",
        "xfce4-terminal",
        "xterm",
    ] {
        list.push(name.to_string());
    }
    list
}

fn is_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("www.")
        || lower.starts_with("microsoft-edge:")
        || lower.starts_with("ftp://")
}

/// Commande d'un lanceur `.desktop`, dans un terminal s'il le demande.
fn desktop_entry(path: &Path, args: &[String]) -> Result<(PathBuf, Vec<String>), isize> {
    let files = crate::launch::linux_args(args);
    let (mut argv, terminal) =
        crate::launch::desktop_command(path, &files).ok_or(SE_ERR_NOASSOC)?;
    if terminal {
        let term = terminal_candidates()
            .iter()
            .find_map(|t| which(t))
            .ok_or(SE_ERR_NOASSOC)?;
        argv.insert(0, "-e".into());
        return Ok((term, argv));
    }
    let program = argv.remove(0);
    let program = which(&program).ok_or(SE_ERR_FNF)?;
    Ok((program, argv))
}

/// Protocole autre que le web : le client mail pour `mailto:`, sinon
/// l'appli Linux déclarée pour `x-scheme-handler/…` (via `xdg-open`).
fn protocol(target: &str, scheme: &str) -> Result<(PathBuf, Vec<String>), isize> {
    if scheme == "mailto" {
        if let Some(mail) = which("xdg-email") {
            return Ok((mail, vec![target.to_string()]));
        }
    }
    if let Some(opener) = which("xdg-open") {
        return Ok((opener, vec![target.to_string()]));
    }
    if scheme == "mailto" {
        // Repli : le navigateur sait souvent ouvrir un webmail.
        for candidate in browser_candidates() {
            if let Some(program) = which(&candidate) {
                return Ok((program, vec![target.to_string()]));
            }
        }
    }
    Err(SE_ERR_NOASSOC)
}

/// Découpe les paramètres comme la ligne de commande Windows.
fn split_params(params: &str) -> Vec<String> {
    if params.trim().is_empty() {
        Vec::new()
    } else {
        crate::dlls::kernel32::split_windows_command_line(params)
    }
}

/// Visionneuse d'images Winlux : `WINLUX_IMAGE_VIEWER`, à côté du programme
/// en cours, puis les dossiers d'installation du live ISO.
fn image_viewer() -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("WINLUX_IMAGE_VIEWER") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(dir) =
        crate::loader::main_module().and_then(|m| m.path.parent().map(Path::to_path_buf))
    {
        dirs.push(dir);
    }
    dirs.push("/opt/winlux/apps".into());
    dirs.push("/opt/winlux/tests".into());
    dirs.into_iter()
        .map(|d| d.join("imageviewer.exe"))
        .find(|p| p.is_file())
}

/// Commande Linux qui réalise `ShellExecute(file, params)`, ou le code
/// d'erreur Win32 (`SE_ERR_*`) si rien ne convient.
/// `explorer.exe` (avec ou sans chemin) : l'Explorateur de Windows.
pub(crate) fn is_explorer(file: &str) -> bool {
    let base = file.rsplit(['\\', '/']).next().unwrap_or(file);
    base.eq_ignore_ascii_case("explorer") || base.eq_ignore_ascii_case("explorer.exe")
}

/// Paramètres d'`explorer.exe` : `/select,CHEMIN` (dossier de l'élément),
/// `/e,` `/n,` `/root,` puis un dossier. Renvoie (chemin, sélection).
fn explorer_target(params: &str) -> (String, bool) {
    let mut rest = params.trim();
    let mut select = false;
    loop {
        rest = rest.trim_start_matches([' ', ',']);
        if !rest.starts_with('/') {
            break;
        }
        let end = rest.find([',', ' ']).unwrap_or(rest.len());
        let switch = &rest[..end];
        select |= switch.eq_ignore_ascii_case("/select");
        rest = &rest[end..];
    }
    (rest.trim().trim_matches('"').to_string(), select)
}

/// `explorer.exe [/select,]CHEMIN` → le gestionnaire de dossiers de la session
/// (`xdg-open`, soit l'Explorateur Winlux) sur le dossier voulu. Sans chemin :
/// le dossier personnel.
pub(crate) fn explorer(params: &str) -> Option<Result<(PathBuf, Vec<String>), isize>> {
    let opener = which("xdg-open")?;
    let (target, select) = explorer_target(params);
    let mut folder = if target.is_empty() {
        PathBuf::from(std::env::var_os("HOME")?)
    } else {
        let path = crate::paths::to_linux(&target);
        if select || !path.is_dir() {
            path.parent().map(Path::to_path_buf).unwrap_or(path)
        } else {
            path
        }
    };
    while !folder.is_dir() {
        folder = folder.parent()?.to_path_buf();
    }
    Some(Ok((opener, vec![folder.to_string_lossy().into_owned()])))
}

pub fn resolve(file: &str, params: &str) -> Result<(PathBuf, Vec<String>), isize> {
    let file = file.trim();
    if file.is_empty() {
        return Err(SE_ERR_FNF);
    }
    let args = split_params(params);
    if is_explorer(file) {
        if let Some(opened) = explorer(params) {
            return opened;
        }
    }
    if file.eq_ignore_ascii_case("shell:browser") || is_url(file) {
        let mut target = file.to_string();
        if let Some(rest) = target.strip_prefix("microsoft-edge:") {
            target = rest.to_string();
        }
        if target.to_ascii_lowercase().starts_with("www.") {
            target = format!("https://{target}");
        }
        for candidate in browser_candidates() {
            if let Some(program) = which(&candidate) {
                let mut a = Vec::new();
                if !file.eq_ignore_ascii_case("shell:browser") && !target.is_empty() {
                    a.push(target.clone());
                }
                return Ok((program, a));
            }
        }
        return Err(SE_ERR_NOASSOC);
    }
    if let Some(path) = crate::launch::file_uri_path(file) {
        let as_windows = crate::paths::to_windows(&path);
        return resolve(&as_windows, params);
    }
    if file.eq_ignore_ascii_case("shell:terminal") {
        for candidate in terminal_candidates() {
            if let Some(program) = which(&candidate) {
                return Ok((program, args));
            }
        }
        return Err(SE_ERR_NOASSOC);
    }
    if let Some(scheme) = crate::launch::uri_scheme(file).filter(|s| s != "shell") {
        return protocol(file, &scheme);
    }
    let looks_like_path = crate::launch::looks_like_path(file);
    let path = crate::paths::to_linux(file);
    if looks_like_path || path.exists() {
        if !path.exists() {
            return Err(SE_ERR_FNF);
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if path.is_file() && ext == "desktop" {
            return desktop_entry(&path, &args);
        }
        if path.is_file() && matches!(ext.as_str(), "exe" | "com" | "scr") {
            // The desktop launcher opens a terminal for console PEs and gives
            // graphical programs an error dialog when execution fails.
            let me = which("winlux-open")
                .or_else(|| std::env::current_exe().ok())
                .ok_or(SE_ERR_FNF)?;
            let mut a = vec![path.to_string_lossy().into_owned()];
            a.extend(args);
            return Ok((me, a));
        }
        if path.is_file() && crate::imaging::supported_ext(&ext) {
            if let Some(viewer) = image_viewer() {
                let me = std::env::current_exe().map_err(|_| SE_ERR_FNF)?;
                return Ok((
                    me,
                    vec![
                        viewer.to_string_lossy().into_owned(),
                        crate::paths::to_windows(&path),
                    ],
                ));
            }
        }
        let opener = which("xdg-open").ok_or(SE_ERR_NOASSOC)?;
        return Ok((opener, vec![path.to_string_lossy().into_owned()]));
    }
    if file.to_ascii_lowercase().ends_with(".desktop") {
        if let Some(p) = crate::launch::find_program(file, None) {
            return desktop_entry(&p, &args);
        }
    }
    if let Some(p) = which(file) {
        if !crate::launch::is_windows_program(&p) {
            return Ok((p, crate::launch::linux_args(&args)));
        }
    }
    // `notepad`, `outil.exe` : programme Windows du PATH, relancé dans Winlux.
    match crate::launch::find_program(file, None) {
        Some(p) if crate::launch::is_windows_program(&p) => {
            let me = std::env::current_exe().map_err(|_| SE_ERR_FNF)?;
            let mut a = vec![p.to_string_lossy().into_owned()];
            a.extend(args);
            Ok((me, a))
        }
        Some(p) => Ok((p, crate::launch::linux_args(&args))),
        None => Err(SE_ERR_FNF),
    }
}

/// Lance la commande. Sans `keep`, un fil récupère le processus à sa fin ;
/// avec `keep`, le processus est rendu pour devenir un handle Windows.
fn spawn(
    program: &Path,
    args: &[String],
    dir: Option<PathBuf>,
    keep: bool,
    elevate: bool,
) -> Result<Option<std::process::Child>, ()> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    if elevate {
        // Verbe « runas » : le .exe lancé affiche la demande UAC (uac.rs).
        cmd.env("WINLUX_ELEVATE", "1");
    }
    if program
        .file_name()
        .is_some_and(|name| name == "winlux-open")
    {
        // PID 1 can leave the graphical shell attached to /dev/console.
        // That inherited TTY is not a visible terminal for the launched app.
        cmd.env("WINLUX_DESKTOP_LAUNCH", "1");
    }
    if let Some(d) = dir.filter(|d| d.is_dir()) {
        cmd.current_dir(d);
    }
    match cmd.spawn() {
        Ok(child) if keep => Ok(Some(child)),
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(None)
        }
        Err(_) => Err(()),
    }
}

pub(crate) fn execute(
    file: &str,
    params: &str,
    dir: Option<String>,
    verb: Option<String>,
) -> isize {
    execute_keep(file, params, dir, false, is_runas(verb.as_deref())).0
}

/// Verbe « runas » : exécuter en tant qu'administrateur.
fn is_runas(verb: Option<&str>) -> bool {
    verb.is_some_and(|v| v.eq_ignore_ascii_case("runas"))
}

fn execute_keep(
    file: &str,
    params: &str,
    dir: Option<String>,
    keep: bool,
    elevate: bool,
) -> (isize, Option<std::process::Child>) {
    match resolve(file, params) {
        Ok((program, args)) => {
            let dir = dir
                .filter(|d| !d.is_empty())
                .map(|d| crate::paths::to_linux(&d));
            match spawn(&program, &args, dir, keep, elevate) {
                Ok(child) => (SUCCESS, child),
                Err(()) => (SE_ERR_FNF, None),
            }
        }
        Err(code) => (code, None),
    }
}

unsafe fn opt_a(p: *const u8) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(crate::dlls::cstr(p))
    }
}
unsafe fn opt_w(p: *const u16) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            p,
            crate::printf::wide_len(p),
        )))
    }
}

pub unsafe extern "win64" fn shell_execute_a(
    _hwnd: usize,
    verb: *const u8,
    file: *const u8,
    params: *const u8,
    dir: *const u8,
    _show: i32,
) -> isize {
    let Some(file) = opt_a(file) else {
        return SE_ERR_FNF;
    };
    execute(
        &file,
        &opt_a(params).unwrap_or_default(),
        opt_a(dir),
        opt_a(verb),
    )
}

pub unsafe extern "win64" fn shell_execute_w(
    _hwnd: usize,
    verb: *const u16,
    file: *const u16,
    params: *const u16,
    dir: *const u16,
    _show: i32,
) -> isize {
    let Some(file) = opt_w(file) else {
        return SE_ERR_FNF;
    };
    execute(
        &file,
        &opt_w(params).unwrap_or_default(),
        opt_w(dir),
        opt_w(verb),
    )
}

/// `SHELLEXECUTEINFOA/W` (x64) : cbSize, fMask, hwnd, lpVerb, lpFile,
/// lpParameters, lpDirectory, nShow, hInstApp, lpIDList, lpClass,
/// hkeyClass, dwHotKey, hIcon/hMonitor, hProcess.
#[repr(C)]
pub struct ShellExecuteInfo {
    size: u32,
    mask: u32,
    hwnd: usize,
    verb: usize,
    file: usize,
    parameters: usize,
    directory: usize,
    show: i32,
    inst_app: isize,
    id_list: usize,
    class: usize,
    hkey_class: usize,
    hot_key: u32,
    icon: usize,
    process: usize,
}

/// SEE_MASK_NOCLOSEPROCESS : l'appelant veut un handle pour attendre la fin.
const SEE_MASK_NOCLOSEPROCESS: u32 = 0x40;
/// Taille d'une structure qui contient le champ hProcess.
const SEI_WITH_PROCESS: u32 = std::mem::size_of::<ShellExecuteInfo>() as u32;

unsafe fn execute_ex(info: *mut ShellExecuteInfo, wide: bool) -> i32 {
    if info.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let i = &mut *info;
    let text = |p: usize| {
        if wide {
            opt_w(p as *const u16)
        } else {
            opt_a(p as *const u8)
        }
    };
    let Some(file) = text(i.file) else {
        crate::process::set_last_error(2);
        return 0;
    };
    let wants_process = i.mask & SEE_MASK_NOCLOSEPROCESS != 0 && i.size >= SEI_WITH_PROCESS;
    if wants_process {
        i.process = 0;
    }
    let (code, child) = execute_keep(
        &file,
        &text(i.parameters).unwrap_or_default(),
        text(i.directory),
        wants_process,
        is_runas(text(i.verb).as_deref()),
    );
    i.inst_app = code;
    if let Some(child) = child {
        i.process = crate::dlls::kernel32::add_child_process(child);
    }
    if code > 32 {
        1
    } else {
        crate::process::set_last_error(if code == SE_ERR_NOASSOC { 1155 } else { 2 });
        0
    }
}

pub unsafe extern "win64" fn shell_execute_ex_a(info: *mut ShellExecuteInfo) -> i32 {
    execute_ex_any(info, false)
}
pub unsafe extern "win64" fn shell_execute_ex_w(info: *mut ShellExecuteInfo) -> i32 {
    execute_ex_any(info, true)
}

/// `SHELLEXECUTEINFO` d'un programme x86 : 60 octets, toutes les cases sur
/// 4 octets (cbSize 0, fMask 4, hwnd 8, lpVerb 12, lpFile 16,
/// lpParameters 20, lpDirectory 24, nShow 28, hInstApp 32, lpIDList 36,
/// lpClass 40, hkeyClass 44, dwHotKey 48, hIcon 52, hProcess 56).
const SEI32_SIZE: u32 = 60;

unsafe fn execute_ex_any(info: *mut ShellExecuteInfo, wide: bool) -> i32 {
    if info.is_null() || crate::wow32::pointer_size() != 4 {
        return execute_ex(info, wide);
    }
    let f = |i: usize| (info as *const u32).add(i).read_unaligned();
    let size = f(0);
    let mut wide_info = ShellExecuteInfo {
        // hProcess n'est demandé que si la structure x86 le contient.
        size: if size >= SEI32_SIZE {
            SEI_WITH_PROCESS
        } else {
            size
        },
        mask: f(1),
        hwnd: f(2) as usize,
        verb: f(3) as usize,
        file: f(4) as usize,
        parameters: f(5) as usize,
        directory: f(6) as usize,
        show: f(7) as i32,
        inst_app: 0,
        id_list: f(9) as usize,
        class: f(10) as usize,
        hkey_class: f(11) as usize,
        hot_key: f(12),
        icon: f(13) as usize,
        process: 0,
    };
    let r = execute_ex(&mut wide_info, wide);
    let o = info as *mut u32;
    o.add(8).write_unaligned(wide_info.inst_app as u32);
    if size >= SEI32_SIZE && wide_info.mask & SEE_MASK_NOCLOSEPROCESS != 0 {
        o.add(14).write_unaligned(wide_info.process as u32);
    }
    r
}

/// SHAddToRecentDocs(uFlags, pv) : ajoute un fichier aux documents récents
/// de Windows. Winlux n'a pas encore de liste « Récents » propre aux
/// programmes Windows ; l'appel est accepté sans effet (VLC quittait sur
/// cette fonction absente, #849).
pub unsafe extern "win64" fn sh_add_to_recent_docs(_flags: u32, _item: *const std::ffi::c_void) {}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SHAddToRecentDocs"=>fn sh_add_to_recent_docs,
        "CommandLineToArgvW"=>fn command_line_to_argv_w,
        "ShellExecuteA"=>fn shell_execute_a,"ShellExecuteW"=>fn shell_execute_w,
        "ShellExecuteExA"=>fn shell_execute_ex_a,"ShellExecuteExW"=>fn shell_execute_ex_w,
        "Shell_NotifyIconA"=>fn crate::dlls::notifyicon::shell_notify_icon_a,
        "Shell_NotifyIconW"=>fn crate::dlls::notifyicon::shell_notify_icon_w,
        "Shell_NotifyIconGetRect"=>fn crate::dlls::notifyicon::shell_notify_icon_get_rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_executable(p: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn explorer_select_ouvre_le_dossier_de_l_element() {
        assert_eq!(
            explorer_target("/select,\"C:\\Users\\a b\\x.txt\""),
            ("C:\\Users\\a b\\x.txt".to_string(), true)
        );
        assert_eq!(
            explorer_target("/e, /root,D:\\"),
            ("D:\\".to_string(), false)
        );
        assert_eq!(explorer_target(""), (String::new(), false));
        assert!(is_explorer("explorer.exe") && is_explorer("C:\\Windows\\EXPLORER.EXE"));
        assert!(!is_explorer("iexplorer.exe"));
        let dir = std::env::temp_dir().join(format!("winlux-explorer-{}", std::process::id()));
        let bin = dir.join("bin");
        let folder = dir.join("Rapports");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("final.pdf"), b"x").unwrap();
        std::fs::write(bin.join("xdg-open"), b"#!/bin/sh\n").unwrap();
        make_executable(&bin.join("xdg-open"));
        let item = crate::paths::to_windows(&folder.join("final.pdf"));
        let _env = crate::launch::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &bin);
        let opened = resolve("explorer.exe", &format!("/select,\"{item}\"")).unwrap();
        let folder_too = resolve(
            "C:\\Windows\\explorer.exe",
            &crate::paths::to_windows(&folder),
        )
        .unwrap();
        let gone = resolve(
            "explorer",
            &format!("/select,{}\\parti.txt", crate::paths::to_windows(&folder)),
        )
        .unwrap();
        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let expected = (
            bin.join("xdg-open"),
            vec![folder.to_string_lossy().into_owned()],
        );
        assert_eq!(opened, expected);
        assert_eq!(folder_too, expected);
        assert_eq!(gone, expected);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn protocoles_desktop_et_programmes_linux_du_path() {
        let dir = std::env::temp_dir().join(format!("winlux-proto-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for name in ["xdg-email", "xdg-open", "lecteur-linux", "xterm"] {
            std::fs::write(bin.join(name), b"#!/bin/sh\n").unwrap();
            make_executable(&bin.join(name));
        }
        let entry = dir.join("Lecteur.desktop");
        std::fs::write(&entry, "[Desktop Entry]\nExec=lecteur-linux --ouvrir %F\n").unwrap();
        let film = dir.join("film.mkv");
        std::fs::write(&film, b"x").unwrap();
        let film_win = crate::paths::to_windows(&film);
        let _env = crate::launch::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &bin);

        assert_eq!(
            resolve("mailto:moi@exemple.fr?subject=Salut", "").unwrap(),
            (
                bin.join("xdg-email"),
                vec!["mailto:moi@exemple.fr?subject=Salut".into()]
            )
        );
        assert_eq!(
            resolve("steam://run/440", "").unwrap(),
            (bin.join("xdg-open"), vec!["steam://run/440".into()])
        );
        // Lanceur .desktop : Exec= avec le fichier converti en chemin Linux.
        let (program, args) = resolve(entry.to_str().unwrap(), &format!("\"{film_win}\"")).unwrap();
        assert_eq!(program, bin.join("lecteur-linux"));
        assert_eq!(
            args,
            vec!["--ouvrir".to_string(), film.to_string_lossy().into_owned()]
        );
        // Programme Linux du PATH : les chemins Windows deviennent Linux.
        let (program, args) = resolve("lecteur-linux", &format!("-v \"{film_win}\"")).unwrap();
        assert_eq!(program, bin.join("lecteur-linux"));
        assert_eq!(
            args,
            vec!["-v".to_string(), film.to_string_lossy().into_owned()]
        );
        // Terminal=true : ouvert dans un terminal.
        std::fs::write(
            &entry,
            "[Desktop Entry]\nExec=lecteur-linux\nTerminal=true\n",
        )
        .unwrap();
        let (program, args) = resolve(entry.to_str().unwrap(), "").unwrap();
        assert_eq!(program, bin.join("xterm"));
        assert_eq!(args, vec!["-e".to_string(), "lecteur-linux".into()]);

        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decoded_images_open_in_the_winlux_viewer_and_others_do_not() {
        let dir = std::env::temp_dir().join(format!("winlux-viewer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let viewer = dir.join("imageviewer.exe");
        std::fs::write(&viewer, b"MZ").unwrap();
        let photo = dir.join("photo.JPG");
        let heic = dir.join("photo.heic");
        std::fs::write(&photo, b"x").unwrap();
        std::fs::write(&heic, b"x").unwrap();
        std::env::set_var("WINLUX_IMAGE_VIEWER", &viewer);
        let (program, args) = resolve(photo.to_str().unwrap(), "").unwrap();
        assert_eq!(program, std::env::current_exe().unwrap());
        assert_eq!(args[0], viewer.to_string_lossy());
        assert!(args[1].ends_with("photo.JPG"), "{args:?}");
        // HEIC n'a pas de décodeur : jamais envoyé à la visionneuse.
        if let Ok((_, args)) = resolve(heic.to_str().unwrap(), "") {
            assert!(!args.iter().any(|a| a.ends_with("imageviewer.exe")));
        }
        std::env::remove_var("WINLUX_IMAGE_VIEWER");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn argv_preserves_empty_unicode_and_escaped_arguments_and_localfree_ownership() {
        let _teb = crate::process::test_teb();
        let expected = [
            "C:\\Program Files\\ChatGPT.exe",
            "",
            "deux mots",
            "écran 😀",
            "quote\"inside",
            "trail \\",
        ];
        let line = expected
            .iter()
            .map(|value| crate::process::quote_arg(value))
            .collect::<Vec<_>>()
            .join(" ");
        let wide: Vec<u16> = line.encode_utf16().chain([0]).collect();
        let mut count = 0;
        unsafe {
            let argv = command_line_to_argv_w(wide.as_ptr(), &mut count);
            assert!(!argv.is_null());
            assert_eq!(count as usize, expected.len());
            assert!((*argv.add(expected.len())).is_null());
            for (index, expected) in expected.iter().enumerate() {
                let mut size = 0;
                while *(*argv.add(index)).add(size) != 0 {
                    size += 1;
                }
                assert_eq!(
                    String::from_utf16(std::slice::from_raw_parts(*argv.add(index), size)).unwrap(),
                    *expected
                );
            }
            assert!(crate::dlls::kernel32_misc::local_free(argv.cast()).is_null());
        }
        assert_eq!(
            split_argv_wide(&"  app.exe".encode_utf16().collect::<Vec<_>>()),
            vec![vec![], "app.exe".encode_utf16().collect()]
        );
    }

    #[test]
    fn les_exe_sont_relances_dans_winlux_et_les_urls_vont_au_navigateur() {
        let dir = std::env::temp_dir().join(format!("winlux-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Jeu.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let (program, args) = resolve(exe.to_str().unwrap(), "-a \"b c\"").unwrap();
        assert_eq!(program, std::env::current_exe().unwrap());
        assert_eq!(
            args,
            vec![
                exe.to_string_lossy().into_owned(),
                "-a".into(),
                "b c".into()
            ]
        );

        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        // Le lanceur Winlux passe avant tout navigateur installé sur l'hôte.
        let browser = bin.join("winlux-browser");
        std::fs::write(&browser, b"#!/bin/sh\n").unwrap();
        make_executable(&browser);
        let _env = crate::launch::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let old_path = std::env::var_os("PATH");
        let mut paths = vec![bin.clone()];
        if let Some(ref old) = old_path {
            paths.extend(std::env::split_paths(old));
        }
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        let launcher = bin.join("winlux-open");
        std::fs::write(&launcher, b"#!/bin/sh\n").unwrap();
        make_executable(&launcher);
        assert_eq!(
            resolve(exe.to_str().unwrap(), "-a").unwrap(),
            (
                launcher,
                vec![exe.to_string_lossy().into_owned(), "-a".into()]
            )
        );
        let (program, args) = resolve("www.bing.com", "").unwrap();
        assert_eq!(program, browser);
        assert_eq!(args, vec!["https://www.bing.com".to_string()]);
        assert_eq!(
            resolve("shell:browser", "").unwrap(),
            (browser.clone(), vec![])
        );
        assert_eq!(resolve("C:\\fichier-introuvable.txt", ""), Err(SE_ERR_FNF));
        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
