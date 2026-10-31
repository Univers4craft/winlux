//! Stubs pour powrprof.dll — gestion de l'alimentation.
//! Electron s'enregistre pour les notifications de veille/réveil.
#![allow(non_snake_case)]

pub extern "win64" fn PowerDeterminePlatformRoleEx(_version: u32) -> u32 {
    1
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

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PowerDeterminePlatformRoleEx" => fn PowerDeterminePlatformRoleEx,
        "PowerRegisterSuspendResumeNotification" => fn PowerRegisterSuspendResumeNotification,
        "PowerUnregisterSuspendResumeNotification" => fn PowerUnregisterSuspendResumeNotification,
        "PowerSettingRegisterNotification" => fn PowerSettingRegisterNotification,
        "PowerSettingUnregisterNotification" => fn PowerSettingUnregisterNotification,
    }
}
