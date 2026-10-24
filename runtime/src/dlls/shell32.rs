//! shell32 : dossiers spéciaux et `ShellExecute`, qui ouvre un fichier, un
//! dossier ou une adresse web avec le bon programme du système Linux.
//!
//! - une adresse web (`https://…`, `www.…`, `microsoft-edge:…`) s'ouvre dans
//!   le navigateur par défaut de Winlux, Microsoft Edge (voir `browser_candidates`) ;
//! - un exécutable Windows (`.exe`, `.com`) est relancé dans Winlux ;
//! - `shell:terminal` ouvre un terminal Linux ;
//! - un nom de programme sans chemin est cherché dans le `PATH` Linux ;
//! - tout autre fichier ou dossier passe par `xdg-open`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SE_ERR_FNF: isize = 2;
const SE_ERR_NOASSOC: isize = 31;
const SUCCESS: isize = 42;

pub unsafe extern "win64" fn sh_get_folder_path_a(_owner:usize,_csidl:i32,_token:usize,_flags:u32,path:*mut u8)->i32{if path.is_null(){return -1;}crate::dlls::put_a(&std::env::var("HOME").unwrap_or_else(|_|"/".into()),path,4096);0}

/// Programme Linux trouvé dans le `PATH`.
fn which(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_else(|| vec!["/usr/local/bin".into(), "/usr/bin".into(), "/bin".into()])
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Navigateurs essayés dans l'ordre : `WINLUX_BROWSER`, le lanceur Winlux,
/// puis Microsoft Edge (canaux stable, bêta, dev) et des solutions de repli.
pub fn browser_candidates() -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(b) = std::env::var("WINLUX_BROWSER") { if !b.is_empty() { list.push(b); } }
    for name in ["winlux-browser", "microsoft-edge-stable", "microsoft-edge", "microsoft-edge-beta", "microsoft-edge-dev", "chromium", "chromium-browser", "firefox", "xdg-open"] {
        list.push(name.to_string());
    }
    list
}

fn terminal_candidates() -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(t) = std::env::var("WINLUX_TERMINAL") { if !t.is_empty() { list.push(t); } }
    for name in ["x-terminal-emulator", "konsole", "gnome-terminal", "kgx", "xfce4-terminal", "xterm"] { list.push(name.to_string()); }
    list
}

fn is_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
        || lower.starts_with("microsoft-edge:") || lower.starts_with("mailto:") || lower.starts_with("ftp://")
}

/// Découpe les paramètres comme la ligne de commande Windows.
fn split_params(params: &str) -> Vec<String> {
    if params.trim().is_empty() { Vec::new() } else { crate::dlls::kernel32::split_windows_command_line(params) }
}

/// Commande Linux qui réalise `ShellExecute(file, params)`, ou le code
/// d'erreur Win32 (`SE_ERR_*`) si rien ne convient.
pub fn resolve(file: &str, params: &str) -> Result<(PathBuf, Vec<String>), isize> {
    let file = file.trim();
    if file.is_empty() { return Err(SE_ERR_FNF); }
    let args = split_params(params);
    if file.eq_ignore_ascii_case("shell:browser") || is_url(file) {
        let mut target = file.to_string();
        if let Some(rest) = target.strip_prefix("microsoft-edge:") { target = rest.to_string(); }
        if target.to_ascii_lowercase().starts_with("www.") { target = format!("https://{target}"); }
        for candidate in browser_candidates() {
            if let Some(program) = which(&candidate) {
                let mut a = Vec::new();
                if !file.eq_ignore_ascii_case("shell:browser") && !target.is_empty() { a.push(target.clone()); }
                return Ok((program, a));
            }
        }
        return Err(SE_ERR_NOASSOC);
    }
    if file.eq_ignore_ascii_case("shell:terminal") {
        for candidate in terminal_candidates() {
            if let Some(program) = which(&candidate) { return Ok((program, args)); }
        }
        return Err(SE_ERR_NOASSOC);
    }
    let looks_like_path = file.contains('\\') || file.contains('/') || (file.len() >= 2 && file.as_bytes()[1] == b':');
    let path = crate::paths::to_linux(file);
    if looks_like_path || path.exists() {
        if !path.exists() { return Err(SE_ERR_FNF); }
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if path.is_file() && matches!(ext.as_str(), "exe" | "com" | "scr") {
            // The desktop launcher opens a terminal for console PEs and gives
            // graphical programs an error dialog when execution fails.
            let me = which("winlux-open").or_else(|| std::env::current_exe().ok()).ok_or(SE_ERR_FNF)?;
            let mut a = vec![path.to_string_lossy().into_owned()];
            a.extend(args);
            return Ok((me, a));
        }
        let opener = which("xdg-open").ok_or(SE_ERR_NOASSOC)?;
        return Ok((opener, vec![path.to_string_lossy().into_owned()]));
    }
    which(file).map(|p| (p, args)).ok_or(SE_ERR_FNF)
}

/// Lance la commande détachée ; un fil récupère le processus à sa fin.
fn spawn(program: &Path, args: &[String], dir: Option<PathBuf>) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    if let Some(d) = dir.filter(|d| d.is_dir()) { cmd.current_dir(d); }
    match cmd.spawn() {
        Ok(mut child) => { std::thread::spawn(move || { let _ = child.wait(); }); true }
        Err(_) => false,
    }
}

fn execute(file: &str, params: &str, dir: Option<String>) -> isize {
    match resolve(file, params) {
        Ok((program, args)) => {
            let dir = dir.filter(|d| !d.is_empty()).map(|d| crate::paths::to_linux(&d));
            if spawn(&program, &args, dir) { SUCCESS } else { SE_ERR_FNF }
        }
        Err(code) => code,
    }
}

unsafe fn opt_a(p: *const u8) -> Option<String> { if p.is_null() { None } else { Some(crate::dlls::cstr(p)) } }
unsafe fn opt_w(p: *const u16) -> Option<String> {
    if p.is_null() { None } else { Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, crate::printf::wide_len(p)))) }
}

pub unsafe extern "win64" fn shell_execute_a(_hwnd: usize, _verb: *const u8, file: *const u8, params: *const u8, dir: *const u8, _show: i32) -> isize {
    let Some(file) = opt_a(file) else { return SE_ERR_FNF };
    execute(&file, &opt_a(params).unwrap_or_default(), opt_a(dir))
}

pub unsafe extern "win64" fn shell_execute_w(_hwnd: usize, _verb: *const u16, file: *const u16, params: *const u16, dir: *const u16, _show: i32) -> isize {
    let Some(file) = opt_w(file) else { return SE_ERR_FNF };
    execute(&file, &opt_w(params).unwrap_or_default(), opt_w(dir))
}

/// `SHELLEXECUTEINFOA` (x64) : cbSize, fMask, hwnd, lpVerb, lpFile,
/// lpParameters, lpDirectory, nShow, hInstApp…
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
}

unsafe fn execute_ex(info: *mut ShellExecuteInfo, wide: bool) -> i32 {
    if info.is_null() { crate::process::set_last_error(87); return 0; }
    let i = &mut *info;
    let text = |p: usize| if wide { opt_w(p as *const u16) } else { opt_a(p as *const u8) };
    let Some(file) = text(i.file) else { crate::process::set_last_error(2); return 0; };
    let code = execute(&file, &text(i.parameters).unwrap_or_default(), text(i.directory));
    i.inst_app = code;
    if code > 32 { 1 } else { crate::process::set_last_error(if code == SE_ERR_NOASSOC { 1155 } else { 2 }); 0 }
}

pub unsafe extern "win64" fn shell_execute_ex_a(info: *mut ShellExecuteInfo) -> i32 { execute_ex(info, false) }
pub unsafe extern "win64" fn shell_execute_ex_w(info: *mut ShellExecuteInfo) -> i32 { execute_ex(info, true) }

pub fn exports()->crate::dlls::Exports{crate::exports!{
    "SHGetFolderPathA"=>fn sh_get_folder_path_a,
    "ShellExecuteA"=>fn shell_execute_a,"ShellExecuteW"=>fn shell_execute_w,
    "ShellExecuteExA"=>fn shell_execute_ex_a,"ShellExecuteExW"=>fn shell_execute_ex_w
}}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_exe_sont_relances_dans_winlux_et_les_urls_vont_au_navigateur() {
        let dir = std::env::temp_dir().join(format!("winlux-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Jeu.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let (program, args) = resolve(exe.to_str().unwrap(), "-a \"b c\"").unwrap();
        assert_eq!(program, std::env::current_exe().unwrap());
        assert_eq!(args, vec![exe.to_string_lossy().into_owned(), "-a".into(), "b c".into()]);

        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let edge = bin.join("microsoft-edge-stable");
        std::fs::write(&edge, b"#!/bin/sh\n").unwrap();
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &bin);
        let launcher = bin.join("winlux-open");
        std::fs::write(&launcher, b"#!/bin/sh\n").unwrap();
        assert_eq!(resolve(exe.to_str().unwrap(), "-a").unwrap(),
            (launcher, vec![exe.to_string_lossy().into_owned(), "-a".into()]));
        let (program, args) = resolve("www.bing.com", "").unwrap();
        assert_eq!(program, edge);
        assert_eq!(args, vec!["https://www.bing.com".to_string()]);
        assert_eq!(resolve("shell:browser", "").unwrap(), (edge.clone(), vec![]));
        assert_eq!(resolve("C:\\fichier-introuvable.txt", ""), Err(SE_ERR_FNF));
        match old_path { Some(p) => std::env::set_var("PATH", p), None => std::env::remove_var("PATH") }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
