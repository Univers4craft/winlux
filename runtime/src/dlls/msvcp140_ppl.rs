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
//! En suspens dans #2308, faute de source admissible pour leur disposition
//! en mémoire : `_Schedule_chore`, `_Release_chore`, le constructeur de
//! `task_continuation_context` et `_ExceptionHolder::ReportUnhandledError`.

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
    fn journaux_et_identifiant() {
        log_vide(0);
        log_schedule_task(0, true);
        context_vide(0);
        let tid = unsafe { libc::syscall(libc::SYS_gettid) as i32 };
        let id = platform_current_thread_id();
        assert!(id > 0);
        assert!(super::super::lookup("kernel32", "GetCurrentThreadId").is_some() || id == tid);
        let e = exports();
        assert_eq!(e.len(), 11);
        assert!(e.contains_key("?_ReportUnobservedException@details@Concurrency@@YAXXZ"));
    }
}
