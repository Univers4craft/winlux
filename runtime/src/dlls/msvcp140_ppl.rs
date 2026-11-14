//! msvcp140.dll maison, suite : l'appui de `<ppltasks.h>` (tâches PPL,
//! `Concurrency::task`) pour un programme de bureau (#2308).
//!
//! Écrit d'après la bibliothèque standard publique de Microsoft (MS STL,
//! licence Apache 2.0 avec exception LLVM) : `stl/src/ppltasks.cpp`, dans
//! sa branche de bureau (ni `_CRT_APP` ni `UNDOCKED_WINDOWS_UCRT`), et
//! `stl/inc/functional` pour `std::function`.
//!
//! - `_TaskEventLogger::_Log*` : sans effet (le traçage ETW des tâches est
//!   vide dans la MS STL).
//! - `_ReportUnobservedException` : arrêt immédiat du programme, comme le
//!   `__fastfail(FAST_FAIL_INVALID_ARG)` de la MS STL (code 0xC0000409).
//! - `platform::GetCurrentThreadId` : l'identifiant Windows du thread.
//! - `_ContextCallback::_Capture` et `_Reset` : sans effet (aucun contexte
//!   COM capturé sur le bureau) ; `_CallInContext` appelle directement la
//!   `std::function<void()>` reçue.
//!
//! `std::function<void()>` (MS STL `<functional>`, x64) : 64 octets, dont
//! le dernier mot (+56) désigne l'objet appelable (`_Getimpl`), nul si la
//! fonction est vide. Cet objet commence par sa table virtuelle de
//! `_Func_base` : `_Copy`, `_Move`, `_Do_call`, `_Target_type`,
//! `_Delete_this(bool)`, `_Get`. L'objet est « local » quand il est rangé
//! dans la `std::function` elle-même.
//!
//! Nos choix, dits :
//!
//! - `_CallInContext` reçoit sa `std::function` par valeur, donc par
//!   adresse en x64, et la détruit comme l'appelé MSVC détruit ses
//!   paramètres (`_Delete_this(!local)`, puis l'appelable effacé).
//! - Une `std::function` vide, pour laquelle la MS STL lève
//!   `bad_function_call`, arrête le programme avec un message, comme nos
//!   autres fonctions `_X*`.
//!
//! `_ExceptionHolder::ReportUnhandledError` : aucune source admissible n'en
//! donne le corps de bureau. Notre choix : arrêt avec message, comme
//! `_ReportUnobservedException`. Sous Windows, une exception de tâche non
//! observée termine aussi le processus ; l'écart porte sur le message, pas
//! sur l'issue de l'appel.
//!
//! `_Schedule_chore` et `_Release_chore` : le corps est connu
//! (`stl/src/taskscheduler.cpp` : `CreateThreadpoolWork`,
//! `SubmitThreadpoolWork`, `CloseThreadpoolWork`, rappel
//! `_M_callback(_M_data)`), mais l'ordre des champs de `_Threadpool_chore`
//! est dans `ppltaskscheduler.h`, absent de la MS STL. Appeler un pointeur
//! de fonction lu au mauvais endroit sauterait n'importe où : nous
//! n'appelons donc rien. À l'appel, nous relevons les trois mots reçus
//! (valeur, et pour chacun le module et la nature de la section où il
//! tombe), puis arrêtons le programme en nommant #2308. Le relevé d'un banc
//! donnera l'ordre mesuré. Lire la mémoire que le programme nous passe
//! n'est pas lire son binaire.

use super::Exports;

/// Les six journaux de `_TaskEventLogger` (thiscall : `this` seul).
extern "win64" fn log_vide(_this: usize) {}

/// `_TaskEventLogger::_LogScheduleTask(bool)`.
extern "win64" fn log_schedule_task(_this: usize, _continuation: bool) {}

const STATUS_STACK_BUFFER_OVERRUN: u32 = 0xC000_0409;

/// `_ReportUnobservedException` : une exception d'une tâche n'a jamais été
/// observée.
extern "win64" fn report_unobserved_exception() {
    eprintln!(
        "winlux: exception d'une tâche PPL jamais observée (_ReportUnobservedException) : arrêt du programme"
    );
    crate::crash::record_failure(
        "FAST_FAIL",
        "Concurrency::details::_ReportUnobservedException",
    );
    crate::dlls::kernel32::flush_and_exit(STATUS_STACK_BUFFER_OVERRUN as i32)
}

/// `platform::GetCurrentThreadId` (rend un `long`).
extern "win64" fn platform_current_thread_id() -> i32 {
    match super::lookup("kernel32", "GetCurrentThreadId") {
        Some(f) => {
            let f: extern "win64" fn() -> u32 = unsafe { std::mem::transmute(f) };
            f() as i32
        }
        None => unsafe { libc::syscall(libc::SYS_gettid) as i32 },
    }
}

/// `_ContextCallback::_Capture` et `_Reset` : rien à capturer ni à rendre.
extern "win64" fn context_vide(_this: usize) {}

/// Indice du mot qui désigne l'appelable dans une `std::function` x64.
const FUNCTION_IMPL: usize = 7;
const DO_CALL: usize = 2;
const DELETE_THIS: usize = 4;

/// `_ContextCallback::_CallInContext(std::function<void()>, bool) const`.
unsafe extern "win64" fn call_in_context(_this: usize, func: *mut usize, _ignore_disconnect: bool) {
    let imp = *func.add(FUNCTION_IMPL);
    if imp == 0 {
        super::msvcp140::fatal("bad_function_call", std::ptr::null());
    }
    let vt = *(imp as *const usize) as *const usize;
    let do_call: unsafe extern "win64" fn(usize) = std::mem::transmute(*vt.add(DO_CALL));
    do_call(imp);
    // Destruction du paramètre par l'appelé (`_Tidy`).
    let imp = *func.add(FUNCTION_IMPL);
    if imp != 0 {
        let vt = *(imp as *const usize) as *const usize;
        let delete_this: unsafe extern "win64" fn(usize, bool) =
            std::mem::transmute(*vt.add(DELETE_THIS));
        let local = imp == func as usize;
        delete_this(imp, !local);
        *func.add(FUNCTION_IMPL) = 0;
    }
}

/// `_ExceptionHolder::ReportUnhandledError` (thiscall : `this` seul).
extern "win64" fn report_unhandled_error(_this: usize) {
    eprintln!(
        "winlux: exception d'une tâche PPL non gérée (_ExceptionHolder::ReportUnhandledError) : arrêt du programme"
    );
    crate::crash::record_failure(
        "FAST_FAIL",
        "Concurrency::details::_ExceptionHolder::ReportUnhandledError",
    );
    crate::dlls::kernel32::flush_and_exit(STATUS_STACK_BUFFER_OVERRUN as i32)
}

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;

/// Où tombe `v` : nul, hors module, ou module+rva et nature de la section.
fn classer(v: usize) -> String {
    if v == 0 {
        return "nul".into();
    }
    let Some(m) = crate::loader::find_module_by_addr(v) else {
        return "hors module".into();
    };
    let rva = v - m.base;
    let sec = m.pe.sections.iter().find(|s| {
        rva >= s.rva as usize && rva < s.rva as usize + s.virtual_size.max(s.raw_size) as usize
    });
    match sec {
        Some(s) if s.characteristics & IMAGE_SCN_MEM_EXECUTE != 0 => {
            format!("{}+{rva:#x}, section exécutable {}", m.name, s.name)
        }
        Some(s) => format!("{}+{rva:#x}, section de données {}", m.name, s.name),
        None => format!("{}+{rva:#x}, hors section", m.name),
    }
}

/// Relevé des trois mots d'un `_Threadpool_chore`.
unsafe fn releve_chore(chore: *const usize) -> String {
    if chore.is_null() {
        return "_Threadpool_chore nul".into();
    }
    (0..3)
        .map(|i| {
            let v = chore.add(i).read_unaligned();
            format!("+{}={v:#x} ({})", i * 8, classer(v))
        })
        .collect::<Vec<_>>()
        .join(" ; ")
}

unsafe fn chore_non_servi(fonction: &str, chore: *const usize) -> ! {
    let releve = releve_chore(chore);
    eprintln!(
        "winlux: Concurrency::details::{fonction} non servi, disposition de _Threadpool_chore non établie (#2308) : {releve} ; arrêt du programme"
    );
    crate::crash::record_failure(
        "ENTRYPOINT_NOT_SERVED",
        &format!("Concurrency::details::{fonction} (#2308) {releve}"),
    );
    crate::dlls::kernel32::flush_and_exit(3)
}

/// `_Schedule_chore(_Threadpool_chore*)` : relevé puis arrêt (voir l'en-tête).
unsafe extern "win64" fn schedule_chore(chore: *const usize) -> i32 {
    chore_non_servi("_Schedule_chore", chore)
}

/// `_Release_chore(_Threadpool_chore*)` : relevé puis arrêt (voir l'en-tête).
unsafe extern "win64" fn release_chore(chore: *const usize) {
    chore_non_servi("_Release_chore", chore)
}

pub fn exports() -> Exports {
    crate::exports! {
        "?_LogScheduleTask@_TaskEventLogger@details@Concurrency@@QEAAX_N@Z" => fn log_schedule_task,
        "?_LogTaskCompleted@_TaskEventLogger@details@Concurrency@@QEAAXXZ" => fn log_vide,
        "?_LogCancelTask@_TaskEventLogger@details@Concurrency@@QEAAXXZ" => fn log_vide,
        "?_LogTaskExecutionCompleted@_TaskEventLogger@details@Concurrency@@QEAAXXZ" => fn log_vide,
        "?_LogWorkItemStarted@_TaskEventLogger@details@Concurrency@@QEAAXXZ" => fn log_vide,
        "?_LogWorkItemCompleted@_TaskEventLogger@details@Concurrency@@QEAAXXZ" => fn log_vide,
        "?_ReportUnobservedException@details@Concurrency@@YAXXZ" => fn report_unobserved_exception,
        "?GetCurrentThreadId@platform@details@Concurrency@@YAJXZ" => fn platform_current_thread_id,
        "?_Capture@_ContextCallback@details@Concurrency@@AEAAXXZ" => fn context_vide,
        "?_Reset@_ContextCallback@details@Concurrency@@AEAAXXZ" => fn context_vide,
        "?_CallInContext@_ContextCallback@details@Concurrency@@QEBAXV?$function@$$A6AXXZ@std@@_N@Z" => fn call_in_context,
        "?ReportUnhandledError@_ExceptionHolder@details@Concurrency@@AEAAXXZ" => fn report_unhandled_error,
        "?_Schedule_chore@details@Concurrency@@YAHPEAU_Threadpool_chore@12@@Z" => fn schedule_chore,
        "?_Release_chore@details@Concurrency@@YAXPEAU_Threadpool_chore@12@@Z" => fn release_chore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static APPELS: Cell<u32> = const { Cell::new(0) };
        static EFFACES: Cell<Option<(usize, bool)>> = const { Cell::new(None) };
    }

    unsafe extern "win64" fn inutile(_: usize) -> usize {
        0
    }
    unsafe extern "win64" fn do_call(_imp: usize) {
        APPELS.with(|a| a.set(a.get() + 1));
    }
    unsafe extern "win64" fn delete_this(imp: usize, dealloc: bool) {
        EFFACES.with(|e| e.set(Some((imp, dealloc))));
    }

    fn table() -> Box<[usize; 6]> {
        Box::new([
            inutile as *const () as usize,
            inutile as *const () as usize,
            do_call as *const () as usize,
            inutile as *const () as usize,
            delete_this as *const () as usize,
            inutile as *const () as usize,
        ])
    }

    #[test]
    fn call_in_context_appelle_puis_detruit_l_appelable_local() {
        let vt = table();
        let mut f = [0usize; 8];
        // Appelable rangé dans la std::function : sa table en tête.
        f[0] = vt.as_ptr() as usize;
        f[FUNCTION_IMPL] = f.as_ptr() as usize;
        let adr = f.as_ptr() as usize;
        unsafe { call_in_context(0, f.as_mut_ptr(), false) };
        assert_eq!(APPELS.with(|a| a.get()), 1);
        assert_eq!(EFFACES.with(|e| e.get()), Some((adr, false)));
        assert_eq!(f[FUNCTION_IMPL], 0);
    }

    #[test]
    fn call_in_context_libere_un_appelable_sur_le_tas() {
        let vt = table();
        let objet = Box::new([vt.as_ptr() as usize, 0]);
        let mut f = [0usize; 8];
        f[FUNCTION_IMPL] = objet.as_ptr() as usize;
        unsafe { call_in_context(0, f.as_mut_ptr(), true) };
        assert_eq!(APPELS.with(|a| a.get()), 1);
        assert_eq!(
            EFFACES.with(|e| e.get()),
            Some((objet.as_ptr() as usize, true))
        );
    }

    #[test]
    fn releve_d_un_chore() {
        let mot = [0usize, 0x10, journaux_et_identifiant as *const () as usize];
        let r = unsafe { releve_chore(mot.as_ptr()) };
        assert!(
            r.starts_with("+0=0x0 (nul) ; +8=0x10 (hors module) ; +16=0x"),
            "{r}"
        );
        assert_eq!(
            unsafe { releve_chore(std::ptr::null()) },
            "_Threadpool_chore nul"
        );
    }

    #[test]
    fn journaux_et_identifiant() {
        log_vide(0);
        log_schedule_task(0, true);
        context_vide(0);
        let tid = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
        let id = platform_current_thread_id();
        assert!(id > 0);
        assert!(super::super::lookup("kernel32", "GetCurrentThreadId").is_some() || id == tid);
        let e = exports();
        assert_eq!(e.len(), 14);
        assert!(e.contains_key("?_ReportUnobservedException@details@Concurrency@@YAXXZ"));
    }
}
