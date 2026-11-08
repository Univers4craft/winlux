//! Bac à sable de Chromium et d'Electron qui ne démarre pas (#236).
//!
//! Le navigateur crée chaque enfant (renderer, GPU…) suspendu sous un jeton
//! restreint, prépare son bac à sable, puis le relance. Si la préparation
//! échoue, il tue l'enfant encore suspendu et l'application s'arrête sans
//! fenêtre. Ce cas est sans ambiguïté : Chromium ne tue jamais autrement un
//! enfant `--type=…` qu'il n'a pas encore laissé démarrer.
//!
//! Winlux le détecte alors et propose clairement (fr/en/de/es, fenêtre
//! `winlux-permission ask sandbox`) de relancer l'application sans son bac
//! à sable (`--no-sandbox`). La réponse est mémorisée comme les autres
//! autorisations et reste visible et modifiable dans Paramètres >
//! Confidentialité et sécurité : aux lancements suivants, `--no-sandbox` est
//! ajouté d'office. Rien n'est désactivé en silence.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::permissions::{Capability, Decision};

const FLAG: &str = "--no-sandbox";

/// Enfants Chromium créés suspendus par ce processus.
static SUSPENDED_CHILDREN: Mutex<Option<HashSet<i32>>> = Mutex::new(None);
static ASKED: AtomicBool = AtomicBool::new(false);

fn lock() -> std::sync::MutexGuard<'static, Option<HashSet<i32>>> {
    SUSPENDED_CHILDREN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ligne de commande d'un enfant de Chromium (renderer, GPU, utilitaire…).
pub fn is_chromium_child_line(line: &str) -> bool {
    line.contains(" --type=") || line.starts_with("--type=")
}

/// CreateProcess vient de lancer `pid` suspendu avec cette ligne.
pub fn note_child(pid: u32, line: &str) {
    if is_chromium_child_line(line) {
        lock().get_or_insert_with(HashSet::new).insert(pid as i32);
    }
}

/// TerminateProcess sur `pid` ; `still_suspended` : jamais relancé.
pub fn on_terminate(pid: i32, still_suspended: bool) {
    let known = lock().as_mut().is_some_and(|s| s.remove(&pid));
    if known && still_suspended {
        sandbox_failed();
    }
}

fn own_line_has_flag() -> bool {
    std::env::args().any(|a| a == FLAG)
}

/// Le bac à sable n'a pas pu démarrer : proposer de relancer sans.
fn sandbox_failed() {
    if ASKED.swap(true, Ordering::SeqCst) || own_line_has_flag() {
        return;
    }
    crate::log_warn!(
        "process",
        "bac à sable de Chromium impossible : proposition de relancer sans"
    );
    if !crate::permissions::allowed(Capability::Sandbox) {
        return;
    }
    relaunch();
}

/// Relance l'application à la place de ce processus (même PID), après
/// avoir arrêté ses enfants (crashpad, GPU) : un second exemplaire verrait
/// le premier encore là et s'arrêterait aussitôt.
fn relaunch() {
    use std::os::unix::ffi::OsStrExt;
    let me = std::process::id().to_string();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for entry in dir.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() else {
                continue;
            };
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            let ppid = stat
                .rfind(')')
                .and_then(|i| stat[i + 1..].split_whitespace().nth(1).map(str::to_string));
            if ppid.as_deref() == Some(me.as_str()) {
                unsafe { libc::kill(pid, libc::SIGKILL) };
                crate::dlls::handle_broker::forget_pid(pid);
            }
        }
    }
    let mut argv: Vec<std::ffi::CString> = std::env::args_os()
        .filter_map(|a| std::ffi::CString::new(a.as_bytes()).ok())
        .collect();
    if argv.is_empty() {
        return;
    }
    argv.push(std::ffi::CString::new(FLAG).unwrap());
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(path) = std::ffi::CString::new(exe.as_os_str().as_bytes()) else {
        return;
    };
    let mut ptrs: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    crate::dlls::handle_broker::forget();
    eprintln!("winlux: relance de l'application sans bac à sable ({FLAG})");
    unsafe { libc::execv(path.as_ptr(), ptrs.as_ptr()) };
}

/// Au lancement d'un programme Windows : ajoute `--no-sandbox` si
/// l'utilisateur l'a accepté pour ce programme (navigateur, pas ses enfants).
pub fn apply_at_start(cmd: &mut String, chromium_child: bool) {
    if chromium_child || cmd.split_whitespace().any(|w| w == FLAG) {
        return;
    }
    if crate::permissions::decided(Capability::Sandbox) == Some(Decision::Allow) {
        cmd.push(' ');
        cmd.push_str(FLAG);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enfants_de_chromium() {
        assert!(is_chromium_child_line(
            "\"C:\\a\\claude.exe\" --type=renderer --lang=fr"
        ));
        assert!(!is_chromium_child_line(
            "\"C:\\a\\claude.exe\" --no-sandbox"
        ));
    }

    #[test]
    fn enfant_inconnu_ignore() {
        note_child(4242, "app.exe --type=gpu-process");
        // Déjà relancé (pas suspendu) : rien.
        on_terminate(4242, false);
        assert!(!lock().as_ref().unwrap().contains(&4242));
        assert!(!ASKED.load(Ordering::SeqCst));
    }
}
