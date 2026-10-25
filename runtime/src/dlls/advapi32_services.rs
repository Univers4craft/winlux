//! Sous-ensemble SCM local, non privilégié. Les définitions sont persistées
//! dans le registre Winlux; StartService lance réellement le binaire invité.

use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

const ERROR_SUCCESS: i32 = 0;
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_INVALID_HANDLE: i32 = 6;
const ERROR_INVALID_PARAMETER: i32 = 87;
const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
const ERROR_CALL_NOT_IMPLEMENTED: i32 = 120;
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
const ERROR_SERVICE_ALREADY_RUNNING: i32 = 1056;
const ERROR_SERVICE_DISABLED: i32 = 1058;
const ERROR_SERVICE_NOT_ACTIVE: i32 = 1062;
const ERROR_SERVICE_EXISTS: i32 = 1073;
const SERVICE_STOPPED: u32 = 1;
const SERVICE_START_PENDING: u32 = 2;
const SERVICE_STOP_PENDING: u32 = 3;
const SERVICE_RUNNING: u32 = 4;
const SERVICE_CONTROL_STOP: u32 = 1;
const SERVICE_CONTROL_INTERROGATE: u32 = 4;
const HKEY_LOCAL_MACHINE: usize = 0x8000_0002;

struct ServiceRecord {
    name: String,
    display: String,
    command: String,
    service_type: u32,
    start_type: u32,
    state: u32,
    win32_exit_code: u32,
    process_id: u32,
    child: Option<Child>,
}
enum ScmHandle {
    Manager,
    Service(String),
}
static SERVICES: OnceLock<Mutex<HashMap<String, ServiceRecord>>> = OnceLock::new();
static HANDLES: OnceLock<Mutex<HashMap<usize, ScmHandle>>> = OnceLock::new();
static NEXT: AtomicUsize = AtomicUsize::new(0x5000_0000);
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn services() -> &'static Mutex<HashMap<String, ServiceRecord>> {
    SERVICES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn handles() -> &'static Mutex<HashMap<usize, ScmHandle>> {
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn new_handle(v: ScmHandle) -> usize {
    let h = NEXT.fetch_add(1, Ordering::Relaxed);
    lock(handles()).insert(h, v);
    h
}
fn handle(h: usize) -> Option<ScmHandle> {
    lock(handles()).get(&h).map(|x| match x {
        ScmHandle::Manager => ScmHandle::Manager,
        ScmHandle::Service(n) => ScmHandle::Service(n.clone()),
    })
}
fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}
fn reg_path(name: &str) -> String {
    format!("SYSTEM\\CurrentControlSet\\Services\\{name}")
}

fn persist(record: &ServiceRecord) -> Result<(), i32> {
    let sub =
        std::ffi::CString::new(reg_path(&record.name)).map_err(|_| ERROR_INVALID_PARAMETER)?;
    let mut key = 0usize;
    let mut disposition = 0u32;
    let status = unsafe {
        crate::dlls::advapi32::reg_create_key_ex_a(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr() as *const u8,
            0,
            std::ptr::null(),
            0,
            0xf003f,
            0,
            &mut key,
            &mut disposition,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(status);
    }
    let image =
        std::ffi::CString::new(record.command.clone()).map_err(|_| ERROR_INVALID_PARAMETER)?;
    let display =
        std::ffi::CString::new(record.display.clone()).map_err(|_| ERROR_INVALID_PARAMETER)?;
    let service_type = record.service_type.to_le_bytes();
    let start_type = record.start_type.to_le_bytes();
    let mut ok = true;
    unsafe {
        for (name, kind, data) in [
            ("ImagePath", 1u32, image.as_bytes_with_nul()),
            ("DisplayName", 1u32, display.as_bytes_with_nul()),
            ("Type", 4u32, service_type.as_slice()),
            ("Start", 4u32, start_type.as_slice()),
        ] {
            let value_name = std::ffi::CString::new(name).unwrap();
            if crate::dlls::advapi32::reg_set_value_ex_a(
                key,
                value_name.as_ptr() as *const u8,
                0,
                kind,
                data.as_ptr(),
                data.len() as u32,
            ) != ERROR_SUCCESS
            {
                ok = false;
            }
        }
        crate::dlls::advapi32::reg_close_key(key);
    }
    if ok {
        Ok(())
    } else {
        Err(ERROR_ACCESS_DENIED)
    }
}

fn read_reg_string(key: usize, name: &str) -> Option<String> {
    let value = std::ffi::CString::new(name).ok()?;
    let mut kind = 0u32;
    let mut size = 0u32;
    let status = unsafe {
        crate::dlls::advapi32::reg_query_value_ex_a(
            key,
            value.as_ptr() as *const u8,
            std::ptr::null_mut(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != 0 || kind != 1 || size > 1_048_576 {
        return None;
    }
    let mut b = vec![0u8; size as usize];
    let status = unsafe {
        crate::dlls::advapi32::reg_query_value_ex_a(
            key,
            value.as_ptr() as *const u8,
            std::ptr::null_mut(),
            &mut kind,
            b.as_mut_ptr(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    while b.last() == Some(&0) {
        b.pop();
    }
    Some(String::from_utf8_lossy(&b).into_owned())
}
fn read_reg_dword(key: usize, name: &str) -> Option<u32> {
    let v = std::ffi::CString::new(name).ok()?;
    let mut kind = 0;
    let mut size = 4;
    let mut n = 0u32;
    let rc = unsafe {
        crate::dlls::advapi32::reg_query_value_ex_a(
            key,
            v.as_ptr() as *const u8,
            std::ptr::null_mut(),
            &mut kind,
            &mut n as *mut u32 as *mut u8,
            &mut size,
        )
    };
    if rc == 0 && kind == 4 && size == 4 {
        Some(n)
    } else {
        None
    }
}
fn load_persisted(name: &str) -> Option<ServiceRecord> {
    let sub = std::ffi::CString::new(reg_path(name)).ok()?;
    let mut key = 0usize;
    if unsafe {
        crate::dlls::advapi32::reg_open_key_ex_a(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr() as *const u8,
            0,
            0,
            &mut key,
        )
    } != 0
    {
        return None;
    }
    let command = read_reg_string(key, "ImagePath");
    let display = read_reg_string(key, "DisplayName").unwrap_or_else(|| name.into());
    let service_type = read_reg_dword(key, "Type").unwrap_or(0x10);
    let start_type = read_reg_dword(key, "Start").unwrap_or(3);
    unsafe {
        crate::dlls::advapi32::reg_close_key(key);
    }
    Some(ServiceRecord {
        name: name.into(),
        display,
        command: command?,
        service_type,
        start_type,
        state: SERVICE_STOPPED,
        win32_exit_code: 0,
        process_id: 0,
        child: None,
    })
}

fn parse_command_line(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let mut count = 1;
            while chars.peek() == Some(&'\\') {
                chars.next();
                count += 1;
            }
            if chars.peek() == Some(&'"') {
                chars.next();
                cur.extend(std::iter::repeat_n('\\', count / 2));
                if count % 2 == 1 {
                    cur.push('"')
                } else {
                    quoted = !quoted;
                }
            } else {
                cur.extend(std::iter::repeat_n('\\', count));
            }
            continue;
        }
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if c.is_whitespace() && !quoted {
            if !cur.is_empty() {
                args.push(std::mem::take(&mut cur));
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

fn spawn_service(record: &mut ServiceRecord, args: &[*const u8]) -> Result<(), u32> {
    if record.start_type == 4 {
        return Err(ERROR_SERVICE_DISABLED as u32);
    }
    if record.state == SERVICE_RUNNING || record.state == SERVICE_START_PENDING {
        return Err(ERROR_SERVICE_ALREADY_RUNNING as u32);
    }
    let mut argv = parse_command_line(&record.command);
    if argv.is_empty() {
        return Err(ERROR_INVALID_PARAMETER as u32);
    }
    for p in args {
        if !p.is_null() {
            argv.push(unsafe { crate::dlls::cstr(*p) });
        }
    }
    let exe = argv.remove(0);
    let mut path = crate::paths::to_linux(&exe);
    if !path.exists() && path.extension().is_none() {
        let mut with = path.as_os_str().to_os_string();
        with.push(".exe");
        let q = std::path::PathBuf::from(with);
        if q.exists() {
            path = q;
        }
    }
    let windows_exe = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("exe"));
    let mut command = if windows_exe {
        let runtime = std::env::current_exe().map_err(|_| ERROR_ACCESS_DENIED as u32)?;
        let mut c = Command::new(runtime);
        c.arg(&path);
        c
    } else {
        Command::new(&path)
    };
    command.args(argv.iter());
    command.env("WINLUX_SERVICE_NAME", &record.name);
    record.state = SERVICE_START_PENDING;
    let child = command.spawn().map_err(|e| {
        record.state = SERVICE_STOPPED;
        match e.kind() {
            std::io::ErrorKind::NotFound => 2u32,
            std::io::ErrorKind::PermissionDenied => 5u32,
            _ => 31u32,
        }
    })?;
    record.process_id = child.id();
    record.child = Some(child);
    record.state = SERVICE_RUNNING;
    record.win32_exit_code = 0;
    Ok(())
}

fn refresh(record: &mut ServiceRecord) {
    if let Some(c) = record.child.as_mut() {
        if let Ok(Some(status)) = c.try_wait() {
            record.state = SERVICE_STOPPED;
            record.win32_exit_code = status.code().unwrap_or(1) as u32;
            record.process_id = 0;
            record.child = None;
        }
    }
}
fn write_status(record: &ServiceRecord, out: *mut u8) {
    unsafe {
        std::ptr::write_bytes(out, 0, 36);
        std::ptr::write_unaligned(out as *mut u32, record.service_type);
        std::ptr::write_unaligned(out.add(4) as *mut u32, record.state);
        std::ptr::write_unaligned(out.add(12) as *mut u32, record.win32_exit_code);
        std::ptr::write_unaligned(out.add(28) as *mut u32, record.process_id);
    }
}
fn write_status_basic(record: &ServiceRecord, out: *mut u8) {
    unsafe {
        std::ptr::write_bytes(out, 0, 28);
        std::ptr::write_unaligned(out as *mut u32, record.service_type);
        std::ptr::write_unaligned(out.add(4) as *mut u32, record.state);
        std::ptr::write_unaligned(out.add(12) as *mut u32, record.win32_exit_code);
    }
}

pub unsafe extern "win64" fn open_sc_manager_a(
    machine: *const u8,
    db: *const u8,
    _access: u32,
) -> usize {
    if !machine.is_null() {
        let m = crate::dlls::cstr(machine);
        let local = std::env::var("HOSTNAME").unwrap_or_default();
        if m != "." && !m.eq_ignore_ascii_case("localhost") && !m.eq_ignore_ascii_case(&local) {
            crate::process::set_last_error(5);
            return 0;
        }
    }
    if !db.is_null() {
        let d = crate::dlls::cstr(db);
        if !d.eq_ignore_ascii_case("ServicesActive") {
            crate::process::set_last_error(5);
            return 0;
        }
    }
    new_handle(ScmHandle::Manager)
}
pub unsafe extern "win64" fn create_service_a(
    manager: usize,
    name: *const u8,
    display: *const u8,
    _access: u32,
    service_type: u32,
    start_type: u32,
    _error: u32,
    path: *const u8,
    _group: *const u8,
    tag: *mut u32,
    _deps: *const u8,
    _user: *const u8,
    _password: *const u8,
) -> usize {
    if !matches!(handle(manager), Some(ScmHandle::Manager)) {
        crate::process::set_last_error(6);
        return 0;
    }
    if name.is_null() || path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let raw = crate::dlls::cstr(name);
    if raw.is_empty() {
        crate::process::set_last_error(87);
        return 0;
    }
    let key = raw.to_lowercase();
    let display = if display.is_null() {
        raw.clone()
    } else {
        crate::dlls::cstr(display)
    };
    let command = crate::dlls::cstr(path);
    if lock(services()).contains_key(&key) {
        crate::process::set_last_error(ERROR_SERVICE_EXISTS as u32);
        return 0;
    }
    let record = ServiceRecord {
        name: key.clone(),
        display,
        command,
        service_type,
        start_type,
        state: SERVICE_STOPPED,
        win32_exit_code: 0,
        process_id: 0,
        child: None,
    };
    if let Err(e) = persist(&record) {
        crate::process::set_last_error(e as u32);
        return 0;
    }
    lock(services()).insert(key.clone(), record);
    if !tag.is_null() {
        *tag = 0;
    }
    new_handle(ScmHandle::Service(key))
}
pub unsafe extern "win64" fn open_service_a(
    manager: usize,
    name: *const u8,
    _access: u32,
) -> usize {
    if !matches!(handle(manager), Some(ScmHandle::Manager)) {
        crate::process::set_last_error(6);
        return 0;
    }
    if name.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let key = crate::dlls::cstr(name).to_lowercase();
    if !lock(services()).contains_key(&key) {
        if let Some(r) = load_persisted(&key) {
            lock(services()).insert(key.clone(), r);
        } else {
            crate::process::set_last_error(ERROR_SERVICE_DOES_NOT_EXIST as u32);
            return 0;
        }
    }
    new_handle(ScmHandle::Service(key))
}
pub unsafe extern "win64" fn start_service_a(
    service: usize,
    argc: u32,
    argv: *const *const u8,
) -> i32 {
    let Some(ScmHandle::Service(name)) = handle(service) else {
        return fail(ERROR_INVALID_HANDLE as u32);
    };
    if argc != 0 && argv.is_null() {
        return fail(ERROR_INVALID_PARAMETER as u32);
    }
    let extra = if argc == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(argv, argc as usize)
    };
    let mut all = lock(services());
    let Some(r) = all.get_mut(&name) else {
        return fail(ERROR_SERVICE_DOES_NOT_EXIST as u32);
    };
    refresh(r);
    match spawn_service(r, extra) {
        Ok(()) => 1,
        Err(e) => fail(e),
    }
}
pub unsafe extern "win64" fn control_service(service: usize, control: u32, status: *mut u8) -> i32 {
    let Some(ScmHandle::Service(name)) = handle(service) else {
        return fail(ERROR_INVALID_HANDLE as u32);
    };
    let mut all = lock(services());
    let Some(r) = all.get_mut(&name) else {
        return fail(ERROR_SERVICE_DOES_NOT_EXIST as u32);
    };
    refresh(r);
    match control {
        SERVICE_CONTROL_STOP => {
            if r.state != SERVICE_RUNNING {
                return fail(ERROR_SERVICE_NOT_ACTIVE as u32);
            }
            r.state = SERVICE_STOP_PENDING;
            if let Some(mut c) = r.child.take() {
                match c.try_wait() {
                    Ok(Some(exit)) => {
                        r.win32_exit_code = exit.code().unwrap_or(1) as u32;
                    }
                    Ok(None) => {
                        if c.kill().is_err() {
                            r.child = Some(c);
                            r.state = SERVICE_RUNNING;
                            return fail(ERROR_ACCESS_DENIED as u32);
                        }
                        if let Ok(exit) = c.wait() {
                            r.win32_exit_code = exit.code().unwrap_or(1) as u32;
                        }
                    }
                    Err(_) => {
                        r.child = Some(c);
                        r.state = SERVICE_RUNNING;
                        return fail(ERROR_ACCESS_DENIED as u32);
                    }
                }
            }
            r.state = SERVICE_STOPPED;
            r.process_id = 0;
        }
        SERVICE_CONTROL_INTERROGATE => {}
        _ => return fail(ERROR_CALL_NOT_IMPLEMENTED as u32),
    }
    if !status.is_null() {
        write_status_basic(r, status);
    }
    1
}
pub unsafe extern "win64" fn query_service_status_ex(
    service: usize,
    level: u32,
    status: *mut u8,
    size: u32,
    needed: *mut u32,
) -> i32 {
    if level != 0 {
        return fail(ERROR_INVALID_PARAMETER as u32);
    }
    if !needed.is_null() {
        *needed = 36;
    }
    if status.is_null() || size < 36 {
        return fail(ERROR_INSUFFICIENT_BUFFER as u32);
    }
    let Some(ScmHandle::Service(name)) = handle(service) else {
        return fail(ERROR_INVALID_HANDLE as u32);
    };
    let mut all = lock(services());
    let Some(r) = all.get_mut(&name) else {
        return fail(ERROR_SERVICE_DOES_NOT_EXIST as u32);
    };
    refresh(r);
    write_status(r, status);
    1
}
pub unsafe extern "win64" fn close_service_handle(handle_value: usize) -> i32 {
    if lock(handles()).remove(&handle_value).is_some() {
        1
    } else {
        crate::process::set_last_error(6);
        0
    }
}
pub unsafe extern "win64" fn delete_service(service: usize) -> i32 {
    let Some(ScmHandle::Service(name)) = handle(service) else {
        return fail(ERROR_INVALID_HANDLE as u32);
    };
    let mut all = lock(services());
    let Some(r) = all.get_mut(&name) else {
        return fail(ERROR_SERVICE_DOES_NOT_EXIST as u32);
    };
    refresh(r);
    if r.state == SERVICE_RUNNING {
        return fail(1072);
    }
    let sub = std::ffi::CString::new(reg_path(&name)).unwrap();
    let rc = crate::dlls::advapi32::reg_delete_key_a(HKEY_LOCAL_MACHINE, sub.as_ptr() as *const u8);
    if rc != 0 && rc != 2 {
        crate::process::set_last_error(rc as u32);
        return 0;
    }
    all.remove(&name);
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {"OpenSCManagerA"=>fn open_sc_manager_a,"CreateServiceA"=>fn create_service_a,"OpenServiceA"=>fn open_service_a,"StartServiceA"=>fn start_service_a,"ControlService"=>fn control_service,"QueryServiceStatusEx"=>fn query_service_status_ex,"CloseServiceHandle"=>fn close_service_handle,"DeleteService"=>fn delete_service}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    #[test]
    fn decoupe_la_ligne_de_commande_de_service_et_gere_les_guillemets() {
        assert_eq!(
            parse_command_line(r#""C:\Program Files\agent.exe" --service "two words""#),
            vec![r#"C:\Program Files\agent.exe"#, "--service", "two words"]
        );
        assert_eq!(
            parse_command_line(r#"agent.exe a\"b"#),
            vec!["agent.exe", "a\"b"]
        );
    }

    #[test]
    fn scm_lance_un_processus_local_et_peut_larreter() {
        let _serial = crate::dlls::advapi32::REGISTRY_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let name = CString::new(format!("winlux-test-service-{}", std::process::id())).unwrap();
        let command = CString::new("/bin/sleep 30").unwrap();
        let manager = unsafe { open_sc_manager_a(std::ptr::null(), std::ptr::null(), 0xf003f) };
        assert_ne!(manager, 0);
        let service = unsafe {
            create_service_a(
                manager,
                name.as_ptr() as *const u8,
                std::ptr::null(),
                0xf01ff,
                0x10,
                3,
                0,
                command.as_ptr() as *const u8,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_ne!(service, 0);
        assert_eq!(unsafe { start_service_a(service, 0, std::ptr::null()) }, 1);
        let mut status = [0u8; 36];
        let mut needed = 0;
        assert_eq!(
            unsafe { query_service_status_ex(service, 0, status.as_mut_ptr(), 36, &mut needed) },
            1
        );
        assert_eq!(
            unsafe { std::ptr::read_unaligned(status.as_ptr().add(4) as *const u32) },
            SERVICE_RUNNING
        );
        assert_eq!(
            unsafe { control_service(service, SERVICE_CONTROL_STOP, status.as_mut_ptr()) },
            1
        );
        assert_eq!(unsafe { delete_service(service) }, 1);
        assert_eq!(unsafe { close_service_handle(service) }, 1);
        assert_eq!(unsafe { close_service_handle(manager) }, 1);
    }
}
