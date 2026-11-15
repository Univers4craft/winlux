//! Bac à sable de Chromium et d'Electron qui ne démarre pas (#236).
//!
//! Le navigateur crée chaque enfant (renderer, GPU…) suspendu sous un jeton
//! restreint, prépare son bac à sable, puis le relance. Si la préparation
//! échoue, il tue l'enfant encore suspendu et l'application s'arrête sans
//! fenêtre. Ce cas est sans ambiguïté : Chromium ne tue jamais autrement un
//! enfant `--type=…` qu'il n'a pas encore laissé démarrer.
//!
//! Winlux le détecte alors et relance l'application sans son bac à sable
//! (`--no-sandbox`), sans poser de question (#1733) : ses enfants sont
//! confinés par Winlux. Le choix est consigné dans le journal, mémorisé et
//! visible dans Paramètres > Confidentialité et sécurité, où un refus est
//! respecté ; aux lancements suivants, `--no-sandbox` est ajouté d'office.
//!
//! Le repli ne vaut que pour un navigateur dont les enfants sont son propre
//! exécutable (Chromium, Electron) : lui seul lit `--no-sandbox`. Un
//! programme qui héberge un moteur web dans un autre exécutable (Acrobat et
//! ses enfants CEF) prendrait l'option pour un document à ouvrir (#2696).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::permissions::{Capability, Decision};

const FLAG: &str = "--no-sandbox";

/// Enfants Chromium créés suspendus par ce processus : leur `--type=` et
/// s'ils sont notre propre exécutable.
static SUSPENDED_CHILDREN: Mutex<Option<HashMap<i32, (String, bool)>>> = Mutex::new(None);
static ASKED: AtomicBool = AtomicBool::new(false);

fn lock() -> std::sync::MutexGuard<'static, Option<HashMap<i32, (String, bool)>>> {
    SUSPENDED_CHILDREN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ligne de commande d'un enfant de Chromium (renderer, GPU, utilitaire…).
pub fn is_chromium_child_line(line: &str) -> bool {
    line.contains(" --type=") || line.starts_with("--type=")
}

/// CreateProcess vient de lancer `pid` suspendu avec cette ligne ;
/// `same_image` : l'enfant est notre propre exécutable.
pub fn note_child(pid: u32, line: &str, same_image: bool) {
    if is_chromium_child_line(line) {
        lock()
            .get_or_insert_with(HashMap::new)
            .insert(pid as i32, (child_type(line).to_string(), same_image));
    }
}

/// TerminateProcess sur `pid` ; `still_suspended` : jamais relancé.
pub fn on_terminate(pid: i32, still_suspended: bool) {
    let child = lock().as_mut().and_then(|s| s.remove(&pid));
    let Some((kind, same_image)) = child.filter(|_| still_suspended) else {
        return;
    };
    let cause = cause(pid, &kind, std::process::id());
    if same_image {
        sandbox_failed(&cause);
    } else {
        // Moteur web hébergé dans un autre exécutable : `--no-sandbox` ne le
        // concerne pas et l'hôte le lirait comme un document (#2696).
        crate::log_info!(
            "process",
            "bac à sable d'un moteur web hébergé ({cause}) : l'enfant n'est pas notre exécutable, pas de relance sans bac à sable (#2696)"
        );
    }
}

/// Valeur de `--type=` d'une ligne d'enfant (« renderer », « gpu-process »…).
fn child_type(line: &str) -> &str {
    line.split_whitespace()
        .find_map(|w| w.strip_prefix("--type="))
        .unwrap_or("?")
}

/// Ce qui a déclenché le repli, pour le journal (#2562) : l'enfant terminé
/// encore suspendu, son type et le processus qui l'a terminé.
fn cause(pid: i32, kind: &str, requester: u32) -> String {
    format!(
        "l'enfant {pid} (--type={kind}) a été terminé avant d'avoir démarré, \
         par le processus {requester} (TerminateProcess)"
    )
}

fn own_line_has_flag() -> bool {
    std::env::args().any(|a| a == FLAG)
}

/// Le bac à sable n'a pas pu démarrer : proposer de relancer sans.
fn sandbox_failed(cause: &str) {
    if ASKED.swap(true, Ordering::SeqCst) || own_line_has_flag() {
        return;
    }
    crate::log_warn!(
        "process",
        "bac à sable de Chromium impossible ({cause}) : relance sans, enfants confinés par Winlux (#1733)"
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
        note_child(4242, "app.exe --type=gpu-process", true);
        // Déjà relancé (pas suspendu) : rien.
        on_terminate(4242, false);
        assert!(!lock().as_ref().unwrap().contains_key(&4242));
        assert!(!ASKED.load(Ordering::SeqCst));
    }

    #[test]
    fn moteur_heberge_dans_un_autre_executable_sans_relance_2696() {
        // Acrobat.exe tue un enfant AcroCEF.exe --type=renderer encore
        // suspendu : pas de relance, rien de mémorisé.
        note_child(4343, "\"C:\\a\\AcroCEF.exe\" --type=renderer", false);
        on_terminate(4343, true);
        assert!(!lock().as_ref().unwrap().contains_key(&4343));
        assert!(!ASKED.load(Ordering::SeqCst));
    }

    #[test]
    fn cause_du_repli_journalisee() {
        assert_eq!(
            child_type("\"C:\\a\\b.exe\" --type=renderer --lang=fr"),
            "renderer"
        );
        assert_eq!(child_type("--type=gpu-process"), "gpu-process");
        let c = cause(6788, "renderer", 6756);
        assert!(c.contains("6788"), "{c}");
        assert!(c.contains("--type=renderer"), "{c}");
        assert!(c.contains("6756"), "{c}");
        assert!(c.contains("TerminateProcess"), "{c}");
    }
}
