//! Primitives de processus de la mscorlib Winlux (#2050) : la classe
//! interne `System.Diagnostics.ProcessusNatif` appelle ici kernel32
//! (OpenProcess, GetExitCodeProcess, TerminateProcess, CreateProcessW…) et
//! la liste des processus que sert déjà Toolhelp (`system_processes`). Un
//! programme .NET voit donc les mêmes processus, noms et codes de sortie
//! qu'un programme natif. Toute la logique de `Process` est en C#.
//!
//! Convention : une primitive qui échoue rend le code d'erreur Win32 en
//! négatif.

use super::bcl::{alloc_managed_string, alloc_utf16, managed_utf16};
use super::exceptions::ClrException;
use super::gc;
use super::il::{Interpreter, Value};
use super::types;
use crate::dlls::kernel32::{self, ProcessInformation, StartupInfoA};

/// Le tableau managé : longueur à +0, données à +8.
const DATA: usize = 8;

fn arg_ref(args: &[Value], i: usize) -> *mut u8 {
    match args.get(i) {
        Some(Value::Ref(p)) => *p,
        _ => std::ptr::null_mut(),
    }
}

fn arg_i64(args: &[Value], i: usize) -> i64 {
    args.get(i).map_or(0, |v| v.as_i64())
}

fn last_error() -> i64 {
    match crate::process::last_error() {
        0 => -31,
        e => -i64::from(e),
    }
}

fn long_array(values: &[i64]) -> Value {
    let arr = gc::with_gc(|gc| gc.alloc_array_of(types::TYPE_INT64, 8, values.len()));
    for (i, v) in values.iter().enumerate() {
        unsafe { *(arr.add(DATA + i * 8) as *mut i64) = *v };
    }
    Value::Ref(arr)
}

/// Chaîne managée en UTF-16 terminé par un zéro ; `None` pour null.
fn wide_opt(args: &[Value], i: usize) -> Option<Vec<u16>> {
    let p = arg_ref(args, i);
    if p.is_null() {
        return None;
    }
    let mut w = managed_utf16(p);
    w.push(0);
    Some(w)
}

/// Processus vivant au sens de Windows : un zombie est fini (#1914).
fn alive(pid: u32) -> bool {
    !kernel32::os_process_zombie(pid as i32)
}

/// Le processus appartient-il au compte courant ? Comme pour
/// QueryFullProcessImageNameW, on n'ouvre pas le processus d'un autre
/// compte : Windows refuse l'accès (ERROR_ACCESS_DENIED). Un pid absent
/// passe : OpenProcess rend alors ERROR_INVALID_PARAMETER.
fn same_account(pid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    let me = unsafe { libc::geteuid() };
    me == 0 || std::fs::metadata(format!("/proc/{pid}")).map_or(true, |m| m.uid() == me)
}

impl Interpreter {
    pub(super) fn process_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, ClrException>> {
        if (namespace, class) != ("System.Diagnostics", "ProcessusNatif") {
            return None;
        }
        // Le programme .NET lancé directement n'a pas de module principal
        // dans le chargeur PE : son image est le fichier de l'assembly
        // d'entrée (Assembly.Location).
        if name == "Image"
            && kernel32::get_process_id(arg_i64(args, 0) as usize) == std::process::id()
        {
            if let Some(path) = self.domain.first().and_then(|d| d.path.as_deref()) {
                return Some(Ok(alloc_managed_string(&crate::paths::to_windows(path))));
            }
        }
        Some(Ok(unsafe { natif(name, args) }?))
    }
}

/// # Safety
/// Les arguments viennent des déclarations `extern` de `ProcessusNatif`.
unsafe fn natif(name: &str, args: &[Value]) -> Option<Value> {
    Some(match name {
        "Courant" => Value::I32(kernel32::get_current_process_id() as i32),
        // Liste() : [pid, nombre de fils, début en tops d'horloge]… à plat.
        "Liste" => {
            let mut flat = Vec::new();
            for p in crate::dlls::kernel32_node::system_processes()
                .into_iter()
                .filter(|p| alive(p.pid))
            {
                flat.push(i64::from(p.pid));
                flat.push(p.threads.len() as i64);
                flat.push(p.start_ticks as i64);
            }
            long_array(&flat)
        }
        // Nom(pid) : nom Windows du programme (« iCUE.exe »), ou null si le
        // processus n'existe plus.
        "Nom" => {
            let pid = arg_i64(args, 0) as u32;
            match crate::dlls::kernel32_node::system_processes()
                .into_iter()
                .find(|p| p.pid == pid && alive(pid))
            {
                Some(p) => alloc_managed_string(&p.name),
                None => Value::Null,
            }
        }
        // Ouvrir(pid) : handle de processus ou -erreur (87 : pas de tel pid).
        "Ouvrir" => {
            let pid = arg_i64(args, 0) as u32;
            if !same_account(pid) {
                return Some(Value::I64(-5));
            }
            crate::process::set_last_error(0);
            let h = kernel32::open_process(0x1F_FFFF, 0, pid);
            Value::I64(if h == 0 { last_error() } else { h as i64 })
        }
        // CodeSortie(h) : code de sortie (259 = STILL_ACTIVE) ou -erreur.
        "CodeSortie" => {
            let mut code = 0u32;
            if kernel32::get_exit_code_process(arg_i64(args, 0) as usize, &mut code) == 0 {
                Value::I64(last_error())
            } else {
                Value::I64(i64::from(code))
            }
        }
        "Terminer" => {
            let ok =
                kernel32::terminate_process(arg_i64(args, 0) as usize, arg_i64(args, 1) as u32);
            Value::I32(if ok != 0 { 0 } else { last_error() as i32 })
        }
        // Attendre(h, ms) : 0 signalé, 258 délai écoulé ; les autres fils
        // gérés continuent pendant l'attente.
        "Attendre" => {
            let h = arg_i64(args, 0) as usize;
            let ms = arg_i64(args, 1) as i32;
            let depth = super::threads::gil_release_all();
            let r = kernel32::wait_for_single_object(h, ms as u32);
            super::threads::gil_restore(depth);
            Value::I32(r as i32)
        }
        // Image(h) : chemin Windows du programme, ou null (accès refusé).
        // Le processus courant se lit par GetModuleFileNameW ; les autres
        // passent par QueryFullProcessImageNameW et sa règle de comptes.
        "Image" => {
            let h = arg_i64(args, 0) as usize;
            let mut buf = vec![0u16; 1024];
            let mut size = buf.len() as u32;
            let ok = if kernel32::get_process_id(h) == kernel32::get_current_process_id() {
                size = kernel32::get_module_file_name_w(0, buf.as_mut_ptr(), size);
                i32::from(size != 0)
            } else {
                crate::dlls::win32_basics::query_full_process_image_name_w(
                    h,
                    0,
                    buf.as_mut_ptr(),
                    &mut size,
                )
            };
            if ok == 0 {
                Value::Null
            } else {
                alloc_utf16(&buf[..(size as usize).min(buf.len())])
            }
        }
        // Lancer(ligne de commande, dossier, redirections, sans fenêtre) :
        // [handle du processus, pid, entrée, sortie, erreurs] (0 pour un
        // flux non redirigé), ou [-erreur].
        "Lancer" => {
            let Some(mut command) = wide_opt(args, 0) else {
                return Some(long_array(&[-87]));
            };
            let dir = wide_opt(args, 1);
            let redirect = arg_i64(args, 2) as u32;
            let no_window = arg_i64(args, 3) != 0;
            let mut startup: StartupInfoA = std::mem::zeroed();
            startup.cb = std::mem::size_of::<StartupInfoA>() as u32;
            // Tubes : le bout de l'enfant est héritable, le nôtre non.
            let mut ours = [0usize; 3];
            let mut theirs = [0usize; 3];
            for (i, slot) in ours.iter_mut().enumerate() {
                if redirect & (1 << i) == 0 {
                    continue;
                }
                let (mut r, mut w) = (0usize, 0usize);
                if kernel32::create_pipe(&mut r, &mut w, std::ptr::null(), 0) == 0 {
                    return Some(long_array(&[last_error()]));
                }
                // Entrée : l'enfant lit, nous écrivons ; sorties : l'inverse.
                let (child, mine) = if i == 0 { (r, w) } else { (w, r) };
                crate::dlls::inherit::set(child, true);
                theirs[i] = child;
                *slot = mine;
            }
            if redirect != 0 {
                startup.flags |= 0x100; // STARTF_USESTDHANDLES
                startup.std_input = theirs[0];
                startup.std_output = theirs[1];
                startup.std_error = theirs[2];
            }
            if no_window {
                startup.flags |= 0x1; // STARTF_USESHOWWINDOW
                startup.show_window = 0;
            }
            let mut info: ProcessInformation = std::mem::zeroed();
            let flags = if no_window { 0x0800_0000 } else { 0 }; // CREATE_NO_WINDOW
            let ok = kernel32::create_process_w(
                std::ptr::null(),
                command.as_mut_ptr(),
                0,
                0,
                i32::from(redirect != 0),
                flags,
                std::ptr::null(),
                dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
                &startup,
                &mut info,
            );
            let err = last_error();
            for h in theirs.into_iter().filter(|&h| h != 0) {
                kernel32::close_handle(h);
            }
            if ok == 0 {
                for h in ours.into_iter().filter(|&h| h != 0) {
                    kernel32::close_handle(h);
                }
                return Some(long_array(&[err]));
            }
            if info.thread != 0 {
                kernel32::close_handle(info.thread);
            }
            long_array(&[
                info.process as i64,
                i64::from(info.process_id),
                ours[0] as i64,
                ours[1] as i64,
                ours[2] as i64,
            ])
        }
        // Ouvrir avec le shell (document, adresse, dossier) : 0 ou -erreur.
        "Shell" => {
            let file = wide_opt(args, 0).unwrap_or_else(|| vec![0]);
            let params = wide_opt(args, 1);
            let dir = wide_opt(args, 2);
            let verb = wide_opt(args, 3);
            let r = crate::dlls::shell32::shell_execute_w(
                0,
                verb.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                file.as_ptr(),
                params.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
                dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
                1,
            );
            Value::I32(if r > 32 { 0 } else { -(r as i32) })
        }
        _ => return None,
    })
}
