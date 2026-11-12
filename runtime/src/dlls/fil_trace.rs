//! Trace des fils et de leurs handles (#2161), active seulement avec
//! `WINLUX_APITRACE` : pour CreateThread, ResumeThread, les attentes,
//! GetExitCodeThread et CloseHandle, le journal d'API reçoit une ligne
//! `winlux[fil]` avec le fil appelant, le handle, ce qu'il désigne (fil et
//! tid, événement…) et la valeur rendue. Les attentes sont notées à
//! l'entrée et au retour, pour lire l'ordre des fils. Sans la variable,
//! chaque fonction appelle directement celle de kernel32.

use crate::dlls::kernel32 as k;

fn active() -> bool {
    crate::thunks::api_trace_active()
}

fn note(line: std::fmt::Arguments) {
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    crate::thunks::api_note(&format!(
        "winlux[fil] {tid} {line} @{:.6}",
        crate::thunks::api_clock()
    ));
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
