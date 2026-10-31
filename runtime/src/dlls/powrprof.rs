//! powrprof.dll et API d'alimentation de kernel32, branchées sur le module
//! `power` (sysfs Linux) : état batterie, modes d'alimentation, veille et
//! inhibitions de mise en veille demandées par les applications.
#![allow(non_snake_case)]

use crate::power::{self, Profile};
use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};

const ERROR_SUCCESS: u32 = 0;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NOT_SUPPORTED: u32 = 50;

pub extern "win64" fn PowerDeterminePlatformRoleEx(_version: u32) -> u32 {
    // PlatformRoleMobile (2) avec batterie, PlatformRoleDesktop (1) sinon.
    if power::read_status().battery.is_some() {
        2
    } else {
        1
    }
}
pub extern "win64" fn PowerDeterminePlatformRole() -> u32 {
    PowerDeterminePlatformRoleEx(1)
}
pub unsafe extern "win64" fn PowerRegisterSuspendResumeNotification(
    _flags: u32,
    _recipient: usize,
    handle: *mut usize,
) -> u32 {
    if !handle.is_null() {
        *handle = 0xA001;
    }
    0
}
pub extern "win64" fn PowerUnregisterSuspendResumeNotification(_h: usize) -> u32 {
    0
}
pub unsafe extern "win64" fn PowerSettingRegisterNotification(
    _guid: usize,
    _flags: u32,
    _recipient: usize,
    handle: *mut usize,
) -> u32 {
    if !handle.is_null() {
        *handle = 0xA002;
    }
    0
}
pub extern "win64" fn PowerSettingUnregisterNotification(_h: usize) -> u32 {
    0
}

/// Mode actif ; renvoie un GUID alloué à libérer avec `LocalFree`.
pub unsafe extern "win64" fn PowerGetActiveScheme(_key: usize, out: *mut *mut u8) -> u32 {
    if out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let guid = power::Config::load().profile.guid();
    let p = crate::dlls::kernel32::heap_alloc(0, 0, 16) as *mut u8;
    if p.is_null() {
        return 8; // ERROR_NOT_ENOUGH_MEMORY
    }
    std::ptr::copy_nonoverlapping(guid.as_ptr(), p, 16);
    *out = p;
    ERROR_SUCCESS
}

/// Change de mode : enregistre la préférence puis règle gouverneur,
/// luminosité et périphériques.
pub unsafe extern "win64" fn PowerSetActiveScheme(_key: usize, guid: *const [u8; 16]) -> u32 {
    if guid.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let Some(profile) = Profile::from_guid(&*guid) else {
        return ERROR_FILE_NOT_FOUND;
    };
    let root = power::root();
    let mut config = power::Config::load_at(&root);
    config.profile = profile;
    if config.save_at(&root).is_ok() {
        power::apply_profile_at(&root, &config);
        return ERROR_SUCCESS;
    }
    // Processus sans droit d'écriture sur /etc : passer par le service.
    let action = format!("power-profile-{}", profile.key());
    match std::process::Command::new("winlux-control")
        .arg(action)
        .status()
    {
        Ok(s) if s.success() => ERROR_SUCCESS,
        _ => ERROR_ACCESS_DENIED,
    }
}

/// Copie le nom lisible d'un mode (UTF-16) ; taille en octets.
pub unsafe extern "win64" fn PowerReadFriendlyName(
    _key: usize,
    scheme: *const [u8; 16],
    _sub: usize,
    _setting: usize,
    buffer: *mut u16,
    size: *mut u32,
) -> u32 {
    if scheme.is_null() || size.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let Some(profile) = Profile::from_guid(&*scheme) else {
        return ERROR_FILE_NOT_FOUND;
    };
    let name: Vec<u16> = profile.label().encode_utf16().chain([0]).collect();
    let bytes = (name.len() * 2) as u32;
    if buffer.is_null() {
        *size = bytes;
        return ERROR_SUCCESS;
    }
    if *size < bytes {
        *size = bytes;
        return 234; // ERROR_MORE_DATA
    }
    std::ptr::copy_nonoverlapping(name.as_ptr(), buffer, name.len());
    *size = bytes;
    ERROR_SUCCESS
}

/// Veille ou veille prolongée ; rend la main après la reprise.
pub extern "win64" fn SetSuspendState(hibernate: u8, _force: u8, _wake_disabled: u8) -> u8 {
    let root = power::root();
    let result = if hibernate != 0 {
        power::sleep::hibernate_at(&root)
    } else {
        power::sleep::suspend_at(&root)
    };
    match result {
        Ok(()) => 1,
        Err(e) => {
            crate::log_warn!("power", "SetSuspendState : {e}");
            crate::process::set_last_error(ERROR_NOT_SUPPORTED);
            0
        }
    }
}
pub extern "win64" fn IsPwrSuspendAllowed() -> u8 {
    u8::from(power::sleep::capabilities_at(&power::root()).suspend)
}
pub extern "win64" fn IsPwrHibernateAllowed() -> u8 {
    u8::from(power::sleep::capabilities_at(&power::root()).hibernate)
}
pub extern "win64" fn IsPwrShutdownAllowed() -> u8 {
    1
}

/// `SYSTEM_POWER_CAPABILITIES` (76 octets).
pub unsafe extern "win64" fn GetPwrCapabilities(out: *mut u8) -> u8 {
    if out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let root = power::root();
    let caps = power::sleep::capabilities_at(&root);
    let battery = power::read_status_at(&root).battery.is_some();
    let lid = root.join("proc/acpi/button/lid").exists();
    let buf = std::slice::from_raw_parts_mut(out, 76);
    buf.fill(0);
    buf[0] = 1; // PowerButtonPresent
    buf[2] = u8::from(lid); // LidPresent
    buf[5] = u8::from(caps.suspend); // SystemS3
    buf[6] = u8::from(caps.hibernate); // SystemS4
    buf[7] = 1; // SystemS5
    buf[8] = u8::from(caps.hibernate); // HiberFilePresent
    buf[30] = u8::from(battery); // SystemBatteriesPresent
    1
}

// ─────────────────────────── kernel32 ───────────────────────────

/// `GetSystemPowerStatus` : secteur, pourcentage, autonomie.
pub unsafe extern "win64" fn GetSystemPowerStatus(out: *mut power::SystemPowerStatus) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let root = power::root();
    let saver = power::Config::load_at(&root).profile == Profile::Saver;
    *out = power::to_system_power_status(&power::read_status_at(&root), saver);
    1
}

thread_local! {
    static EXECUTION_STATE: Cell<u32> = const { Cell::new(0) };
}

fn thread_key() -> String {
    // SAFETY: gettid n'a pas de précondition.
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    format!("{}-t{tid}", std::process::id())
}

/// `SetThreadExecutionState` : empêche la veille ou l'extinction de l'écran
/// (lecteurs vidéo, présentations, téléchargements).
pub extern "win64" fn SetThreadExecutionState(flags: u32) -> u32 {
    let previous = EXECUTION_STATE.with(Cell::get);
    let root = power::root();
    if flags & power::ES_CONTINUOUS != 0 {
        let state = flags & !power::ES_CONTINUOUS;
        power::set_inhibit_at(&root, &thread_key(), state);
        EXECUTION_STATE.with(|s| s.set(if state == 0 { 0 } else { flags }));
    } else if flags != 0 {
        // Sans ES_CONTINUOUS : simple remise à zéro des délais d'inactivité.
        power::touch_activity_at(&root);
    }
    // Windows renvoie au moins ES_CONTINUOUS pour signaler le succès.
    previous | power::ES_CONTINUOUS
}

static NEXT_REQUEST: AtomicU32 = AtomicU32::new(1);
const REQUEST_BASE: usize = 0x5057_0000;

/// `PowerCreateRequest` : renvoie un handle de demande d'éveil.
pub extern "win64" fn PowerCreateRequest(_reason: usize) -> usize {
    REQUEST_BASE + NEXT_REQUEST.fetch_add(1, Ordering::Relaxed) as usize
}

fn request_key(h: usize) -> Option<String> {
    let id = h
        .checked_sub(REQUEST_BASE)
        .filter(|i| *i > 0 && *i < 0x1_0000)?;
    Some(format!("{}-r{id}", std::process::id()))
}

/// Types de demande : 0 écran, 1 système, 2 absence, 3 exécution.
fn request_flag(kind: u32) -> Option<u32> {
    match kind {
        0 => Some(power::ES_DISPLAY_REQUIRED),
        1 | 3 => Some(power::ES_SYSTEM_REQUIRED),
        2 => Some(power::ES_AWAYMODE_REQUIRED),
        _ => None,
    }
}

fn update_request(h: usize, kind: u32, set: bool) -> i32 {
    let (Some(key), Some(flag)) = (request_key(h), request_flag(kind)) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let root = power::root();
    let path = power::run_dir_at(&root).join("power-inhibit").join(&key);
    let current = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| u32::from_str_radix(t.trim().trim_start_matches("0x"), 16).ok())
        .unwrap_or(0);
    let next = if set { current | flag } else { current & !flag };
    power::set_inhibit_at(&root, &key, next);
    1
}

pub extern "win64" fn PowerSetRequest(h: usize, kind: u32) -> i32 {
    update_request(h, kind, true)
}
pub extern "win64" fn PowerClearRequest(h: usize, kind: u32) -> i32 {
    update_request(h, kind, false)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PowerDeterminePlatformRole" => fn PowerDeterminePlatformRole,
        "PowerDeterminePlatformRoleEx" => fn PowerDeterminePlatformRoleEx,
        "PowerRegisterSuspendResumeNotification" => fn PowerRegisterSuspendResumeNotification,
        "PowerUnregisterSuspendResumeNotification" => fn PowerUnregisterSuspendResumeNotification,
        "PowerSettingRegisterNotification" => fn PowerSettingRegisterNotification,
        "PowerSettingUnregisterNotification" => fn PowerSettingUnregisterNotification,
        "PowerGetActiveScheme" => fn PowerGetActiveScheme,
        "PowerSetActiveScheme" => fn PowerSetActiveScheme,
        "PowerReadFriendlyName" => fn PowerReadFriendlyName,
        "SetSuspendState" => fn SetSuspendState,
        "IsPwrSuspendAllowed" => fn IsPwrSuspendAllowed,
        "IsPwrHibernateAllowed" => fn IsPwrHibernateAllowed,
        "IsPwrShutdownAllowed" => fn IsPwrShutdownAllowed,
        "GetPwrCapabilities" => fn GetPwrCapabilities,
    }
}

/// Exports ajoutés à kernel32.
pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetSystemPowerStatus" => fn GetSystemPowerStatus,
        "SetThreadExecutionState" => fn SetThreadExecutionState,
        "PowerCreateRequest" => fn PowerCreateRequest,
        "PowerSetRequest" => fn PowerSetRequest,
        "PowerClearRequest" => fn PowerClearRequest,
    }
}
