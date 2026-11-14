//! Trace des fils et de leurs handles (#2161), active avec
//! `WINLUX_APITRACE` (lignes dans le journal d'API) ou, seule, avec
//! `WINLUX_THREAD_TRACE=1` (#2405 : sortie d'erreur et journal, niveau
//! Warn, sans la trace d'API qui change le comportement et décale les
//! horloges). Pour CreateThread, _beginthreadex/_beginthread, ResumeThread,
//! les attentes, GetExitCodeThread, CloseHandle, la fin de chaque fil et les
//! signaux (SetEvent, ResetEvent, ReleaseMutex, ReleaseSemaphore), une ligne
//! `winlux[fil]` donne le fil appelant, le handle, ce qu'il désigne et la
//! valeur rendue. Les attentes sont notées à l'entrée et au retour, pour
//! lire l'ordre des fils. Limite connue : sections critiques, SRW,
//! variables de condition, WaitOnAddress et pool de fils ne sont pas tracés
//! (une ligne par appel noierait le relevé). Sans variable, chaque fonction
//! appelle directement celle de kernel32.

use crate::dlls::kernel32 as k;

/// `WINLUX_THREAD_TRACE` posée (et différente de « 0 ») ?
pub fn thread_trace_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("WINLUX_THREAD_TRACE").is_some_and(|v| !v.is_empty() && v != "0")
    })
}

pub fn active() -> bool {
    thread_trace_on() || crate::thunks::api_trace_active()
}

/// Heure monotone en secondes (CLOCK_MONOTONIC), la même que celle du
/// rapport de faute : les lignes des fils et la faute se comparent.
pub fn monotonic() -> f64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}

fn note(line: std::fmt::Arguments) {
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    if thread_trace_on() {
        crate::log::emit(
            crate::log::Level::Warn,
            "fil",
            &format_args!("{tid} {line} @{:.6}", monotonic()),
        );
    }
    crate::thunks::api_note(&format!(
        "winlux[fil] {tid} {line} @{:.6}",
        crate::thunks::api_clock()
    ));
}

/// Fil créé par la CRT (`_beginthreadex`, `_beginthread`) : même ligne que
/// CreateThread, que la CRT appelle sans passer par l'export (#2405).
pub fn note_created(how: &str, start: usize, param: usize, flags: u32, h: usize) {
    if active() {
        let site = crate::thunks::describe_site(start);
        note(format_args!(
            "{how}(départ {site}, paramètre {param:#x}, drapeaux {flags:#x}) -> {}",
            described(h)
        ));
    }
}

/// Fin du fil courant, par retour de sa fonction ou par ExitThread (#2405).
pub fn note_end(code: u32) {
    if active() {
        note(format_args!("fin du fil, code {code:#x}"));
    }
}

pub extern "win64" fn set_event(h: usize) -> i32 {
    if !active() {
        return k::set_event(h);
    }
    let what = described(h);
    let r = k::set_event(h);
    note(format_args!("SetEvent({what}) -> {r}"));
    r
}

pub extern "win64" fn reset_event(h: usize) -> i32 {
    if !active() {
        return k::reset_event(h);
    }
    let what = described(h);
    let r = k::reset_event(h);
    note(format_args!("ResetEvent({what}) -> {r}"));
    r
}

pub extern "win64" fn release_mutex(h: usize) -> i32 {
    if !active() {
        return k::release_mutex(h);
    }
    let what = described(h);
    let r = k::release_mutex(h);
    note(format_args!("ReleaseMutex({what}) -> {r}"));
    r
}

pub unsafe extern "win64" fn release_semaphore(h: usize, count: i32, prev: *mut i32) -> i32 {
    if !active() {
        return k::release_semaphore(h, count, prev);
    }
    let what = described(h);
    let r = k::release_semaphore(h, count, prev);
    note(format_args!("ReleaseSemaphore({what}, {count}) -> {r}"));
    r
}

fn described(h: usize) -> String {
    format!("{h:#x} ({})", k::describe_handle(h))
}

pub unsafe extern "win64" fn create_thread(
    attrs: usize,
    stack: usize,
    start: usize,
    param: usize,
    flags: u32,
    id: *mut u32,
) -> usize {
    let h = k::create_thread(attrs, stack, start, param, flags, id);
    if active() {
        let site = crate::thunks::describe_site(start);
        note(format_args!(
            "CreateThread(départ {site}, paramètre {param:#x}, drapeaux {flags:#x}) -> {}",
            described(h)
        ));
    }
    h
}

pub extern "win64" fn resume_thread(h: usize) -> u32 {
    if !active() {
        return k::resume_thread(h);
    }
    let what = described(h);
    let r = k::resume_thread(h);
    note(format_args!("ResumeThread({what}) -> {r:#x}"));
    r
}

pub unsafe extern "win64" fn get_exit_code_thread(h: usize, code: *mut u32) -> i32 {
    let r = k::get_exit_code_thread(h, code);
    if active() {
        let c = if r != 0 && !code.is_null() { *code } else { 0 };
        note(format_args!(
            "GetExitCodeThread({}) -> {r}, code {c:#x}",
            described(h)
        ));
    }
    r
}

pub extern "win64" fn close_handle(h: usize) -> i32 {
    if !active() {
        return k::close_handle(h);
    }
    let what = described(h);
    let r = k::close_handle(h);
    note(format_args!("CloseHandle({what}) -> {r}"));
    r
}

pub extern "win64" fn wait_for_single_object(h: usize, ms: u32) -> u32 {
    if !active() {
        return k::wait_for_single_object(h, ms);
    }
    note(format_args!(
        "WaitForSingleObject({}, {ms}) : attente",
        described(h)
    ));
    let r = k::wait_for_single_object(h, ms);
    note(format_args!(
        "WaitForSingleObject({}, {ms}) -> {r:#x}",
        described(h)
    ));
    r
}

pub unsafe extern "win64" fn wait_for_single_object_ex(h: usize, ms: u32, alertable: i32) -> u32 {
    if !active() {
        return k::wait_for_single_object_ex(h, ms, alertable);
    }
    note(format_args!(
        "WaitForSingleObjectEx({}, {ms}, {alertable}) : attente",
        described(h)
    ));
    let r = k::wait_for_single_object_ex(h, ms, alertable);
    note(format_args!(
        "WaitForSingleObjectEx({}, {ms}, {alertable}) -> {r:#x}",
        described(h)
    ));
    r
}

fn list(count: u32, handles: *const usize) -> String {
    if handles.is_null() || count == 0 || count > 64 {
        return "-".into();
    }
    let hs = unsafe { std::slice::from_raw_parts(handles, count as usize) };
    hs.iter()
        .map(|&h| described(h))
        .collect::<Vec<_>>()
        .join(", ")
}

pub unsafe extern "win64" fn wait_for_multiple_objects(
    count: u32,
    handles: *const usize,
    wait_all: i32,
    ms: u32,
) -> u32 {
    if !active() {
        return k::wait_for_multiple_objects(count, handles, wait_all, ms);
    }
    note(format_args!(
        "WaitForMultipleObjects([{}], tous {wait_all}, {ms}) : attente",
        list(count, handles)
    ));
    let r = k::wait_for_multiple_objects(count, handles, wait_all, ms);
    note(format_args!(
        "WaitForMultipleObjects([{}], tous {wait_all}, {ms}) -> {r:#x}",
        list(count, handles)
    ));
    r
}

pub unsafe extern "win64" fn wait_for_multiple_objects_ex(
    count: u32,
    handles: *const usize,
    wait_all: i32,
    ms: u32,
    alertable: i32,
) -> u32 {
    if !active() {
        return crate::dlls::kernel32_ext::wait_for_multiple_objects_ex(
            count, handles, wait_all, ms, alertable,
        );
    }
    note(format_args!(
        "WaitForMultipleObjectsEx([{}], tous {wait_all}, {ms}, {alertable}) : attente",
        list(count, handles)
    ));
    let r = crate::dlls::kernel32_ext::wait_for_multiple_objects_ex(
        count, handles, wait_all, ms, alertable,
    );
    note(format_args!(
        "WaitForMultipleObjectsEx([{}], tous {wait_all}, {ms}, {alertable}) -> {r:#x}",
        list(count, handles)
    ));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "win64" fn fil_court(_p: usize) -> u32 {
        7
    }

    /// Variable qui fait de `trace_fils_enfant` le processus enfant (#2405).
    const ENFANT: &str = "WINLUX_TEST_TRACE_FILS_ENFANT";

    extern "win64" fn fil_signale(ev: usize) -> u32 {
        set_event(ev);
        9
    }

    #[test]
    fn trace_fils_enfant() {
        if std::env::var_os(ENFANT).is_none() {
            return;
        }
        unsafe {
            let ev = k::create_event_w(0, 1, 0, std::ptr::null());
            let h = crate::dlls::crt_runtime::beginthreadex(
                0,
                0,
                fil_signale as *const () as usize,
                ev,
                0,
                std::ptr::null_mut(),
            );
            wait_for_single_object(ev, 10_000);
            wait_for_single_object(h, 10_000);
            reset_event(ev);
        }
        std::process::exit(0);
    }

    /// #2405 : avec WINLUX_THREAD_TRACE seule (sans WINLUX_APITRACE ni
    /// WINLUX_LOG=trace), _beginthreadex, le signal, l'attente et la fin du
    /// fil sortent sur la sortie d'erreur ; sans elle, rien.
    #[test]
    fn trace_des_fils_sans_trace_d_api() {
        let lancer = |on: bool| {
            let dir = std::env::temp_dir().join(format!("winlux-fils-{on}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.args([
                "--exact",
                "dlls::fil_trace::tests::trace_fils_enfant",
                "--test-threads=1",
            ])
            .env(ENFANT, "1")
            .env("WINLUX_JOURNAL_DIR", &dir)
            .env_remove("WINLUX_APITRACE")
            .env_remove("WINLUX_LOG")
            .env_remove("WINLUX_TRACE")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
            if on {
                cmd.env("WINLUX_THREAD_TRACE", "1");
            } else {
                cmd.env_remove("WINLUX_THREAD_TRACE");
            }
            let mut child = cmd.spawn().unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            while child.try_wait().unwrap().is_none() {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("processus enfant bloqué plus de 60 s");
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let mut err = String::new();
            use std::io::Read;
            let _ = child.stderr.take().unwrap().read_to_string(&mut err);
            let _ = std::fs::remove_dir_all(&dir);
            err
        };
        let err = lancer(true);
        let lignes: Vec<&str> = err
            .lines()
            .filter(|l| l.starts_with("winlux[warn/fil]: "))
            .collect();
        let a = |motif: &str| lignes.iter().any(|l| l.contains(motif) && l.contains(" @"));
        assert!(a("_beginthreadex(départ "), "{err}");
        assert!(a("SetEvent(") && a(") -> 1"), "{err}");
        assert!(a("WaitForSingleObject(") && a(" : attente"), "{err}");
        assert!(a("fin du fil, code 0x9"), "{err}");
        assert!(a("ResetEvent("), "{err}");
        assert!(!lancer(false).contains("winlux[warn/fil]"));
    }

    #[test]
    fn handles_de_fil_decrits() {
        let mut tid = 0u32;
        let h = unsafe { k::create_thread(0, 0, fil_court as *const () as usize, 0, 0, &mut tid) };
        assert_ne!(h, 0);
        assert_eq!(k::wait_for_single_object(h, 10_000), 0);
        assert_eq!(k::describe_handle(h), format!("fil {tid} fini (0x7)"));
        let mut code = 0u32;
        assert_eq!(unsafe { get_exit_code_thread(h, &mut code) }, 1);
        assert_eq!(code, 7);
        assert_eq!(close_handle(h), 1);
        assert_eq!(k::describe_handle(h), "autre");
        assert!(k::describe_handle((-2isize) as usize).starts_with("fil courant "));
        assert_eq!(k::describe_handle(usize::MAX), "processus courant");
    }
}
