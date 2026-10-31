//! shell32 : dossiers spéciaux et `ShellExecute`, qui ouvre un fichier, un
//! dossier ou une adresse web avec le bon programme du système Linux.
//!
//! - une adresse web (`https://…`, `www.…`, `microsoft-edge:…`) s'ouvre dans
//!   le navigateur par défaut de Winlux, Microsoft Edge (voir `browser_candidates`) ;
//! - un exécutable Windows (`.exe`, `.com`) est relancé dans Winlux ;
//! - `shell:terminal` ouvre un terminal Linux ;
//! - un nom de programme sans chemin est cherché dans le `PATH` Linux ;
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

pub unsafe extern "win64" fn sh_get_folder_path_a(
    _owner: usize,
    _csidl: i32,
    _token: usize,
    _flags: u32,
    path: *mut u8,
) -> i32 {
    if path.is_null() {
        return -1;
    }
    crate::dlls::put_a(
        &std::env::var("HOME").unwrap_or_else(|_| "/".into()),
        path,
        4096,
    );
    0
}

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
    if let Ok(b) = std::env::var("WINLUX_BROWSER") {
        if !b.is_empty() {
            list.push(b);
        }
    }
    for name in [
        "winlux-browser",
        "microsoft-edge-stable",
        "microsoft-edge",
        "microsoft-edge-beta",
        "microsoft-edge-dev",
        "chromium",
        "chromium-browser",
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
        || lower.starts_with("mailto:")
        || lower.starts_with("ftp://")
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
pub fn resolve(file: &str, params: &str) -> Result<(PathBuf, Vec<String>), isize> {
    let file = file.trim();
    if file.is_empty() {
        return Err(SE_ERR_FNF);
    }
    let args = split_params(params);
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
    if file.eq_ignore_ascii_case("shell:terminal") {
        for candidate in terminal_candidates() {
            if let Some(program) = which(&candidate) {
                return Ok((program, args));
            }
        }
        return Err(SE_ERR_NOASSOC);
    }
    let looks_like_path = file.contains('\\')
        || file.contains('/')
        || (file.len() >= 2 && file.as_bytes()[1] == b':');
    let path = crate::paths::to_linux(file);
    if looks_like_path || path.exists() {
        if !path.exists() {
            return Err(SE_ERR_FNF);
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
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
    which(file).map(|p| (p, args)).ok_or(SE_ERR_FNF)
}

/// Lance la commande détachée ; un fil récupère le processus à sa fin.
fn spawn(program: &Path, args: &[String], dir: Option<PathBuf>) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
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
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

fn execute(file: &str, params: &str, dir: Option<String>) -> isize {
    match resolve(file, params) {
        Ok((program, args)) => {
            let dir = dir
                .filter(|d| !d.is_empty())
                .map(|d| crate::paths::to_linux(&d));
            if spawn(&program, &args, dir) {
                SUCCESS
            } else {
                SE_ERR_FNF
            }
        }
        Err(code) => code,
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
    _verb: *const u8,
    file: *const u8,
    params: *const u8,
    dir: *const u8,
    _show: i32,
) -> isize {
    let Some(file) = opt_a(file) else {
        return SE_ERR_FNF;
    };
    execute(&file, &opt_a(params).unwrap_or_default(), opt_a(dir))
}

pub unsafe extern "win64" fn shell_execute_w(
    _hwnd: usize,
    _verb: *const u16,
    file: *const u16,
    params: *const u16,
    dir: *const u16,
    _show: i32,
) -> isize {
    let Some(file) = opt_w(file) else {
        return SE_ERR_FNF;
    };
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
    let code = execute(
        &file,
        &text(i.parameters).unwrap_or_default(),
        text(i.directory),
    );
    i.inst_app = code;
    if code > 32 {
        1
    } else {
        crate::process::set_last_error(if code == SE_ERR_NOASSOC { 1155 } else { 2 });
        0
    }
}

pub unsafe extern "win64" fn shell_execute_ex_a(info: *mut ShellExecuteInfo) -> i32 {
    execute_ex(info, false)
}
pub unsafe extern "win64" fn shell_execute_ex_w(info: *mut ShellExecuteInfo) -> i32 {
    execute_ex(info, true)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SHGetFolderPathA"=>fn sh_get_folder_path_a,
        "CommandLineToArgvW"=>fn command_line_to_argv_w,
        "ShellExecuteA"=>fn shell_execute_a,"ShellExecuteW"=>fn shell_execute_w,
        "ShellExecuteExA"=>fn shell_execute_ex_a,"ShellExecuteExW"=>fn shell_execute_ex_w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let edge = bin.join("microsoft-edge-stable");
        std::fs::write(&edge, b"#!/bin/sh\n").unwrap();
        let old_path = std::env::var_os("PATH");
        let mut paths = vec![bin.clone()];
        if let Some(ref old) = old_path {
            paths.extend(std::env::split_paths(old));
        }
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        let launcher = bin.join("winlux-open");
        std::fs::write(&launcher, b"#!/bin/sh\n").unwrap();
        assert_eq!(
            resolve(exe.to_str().unwrap(), "-a").unwrap(),
            (
                launcher,
                vec![exe.to_string_lossy().into_owned(), "-a".into()]
            )
        );
        let (program, args) = resolve("www.bing.com", "").unwrap();
        assert_eq!(program, edge);
        assert_eq!(args, vec!["https://www.bing.com".to_string()]);
        assert_eq!(
            resolve("shell:browser", "").unwrap(),
            (edge.clone(), vec![])
        );
        assert_eq!(resolve("C:\\fichier-introuvable.txt", ""), Err(SE_ERR_FNF));
        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
