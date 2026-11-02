//! setupapi.dll — énumération de périphériques.
//! L'interface HID (manettes, volants, joysticks) est servie par
//! `hid.rs` à partir de hidraw ; les autres classes restent vides
//! (Electron/Chromium s'en contente pour les GPU).
#![allow(non_snake_case)]

use crate::dlls::hid;

pub unsafe extern "win64" fn SetupDiGetClassDevsW(
    guid: *const u8,
    _enumerator: *const u16,
    _parent: usize,
    flags: u32,
) -> usize {
    hid::get_class_devs(guid, flags).unwrap_or(usize::MAX) // INVALID_HANDLE_VALUE
}
pub unsafe extern "win64" fn SetupDiGetClassDevsA(
    guid: *const u8,
    _enumerator: *const u8,
    _parent: usize,
    flags: u32,
) -> usize {
    hid::get_class_devs(guid, flags).unwrap_or(usize::MAX)
}
pub extern "win64" fn SetupDiDestroyDeviceInfoList(set: usize) -> i32 {
    hid::destroy(set);
    1
}
pub unsafe extern "win64" fn SetupDiEnumDeviceInfo(set: usize, idx: u32, data: *mut u8) -> i32 {
    hid::enum_device_info(set, idx, data).unwrap_or(false) as i32
}
pub unsafe extern "win64" fn SetupDiGetDeviceRegistryPropertyW(
    set: usize,
    data: *const u8,
    prop: u32,
    kind: *mut u32,
    buf: *mut u8,
    size: u32,
    required: *mut u32,
) -> i32 {
    let Some((t, value)) = hid::registry_property(set, data, prop) else {
        crate::process::set_last_error(13); // ERROR_INVALID_DATA
        return 0;
    };
    let bytes = value.len() as u32 * 2;
    if !kind.is_null() {
        *kind = t;
    }
    if !required.is_null() {
        *required = bytes;
    }
    if buf.is_null() || size < bytes {
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(value.as_ptr() as *const u8, buf, bytes as usize);
    1
}
pub unsafe extern "win64" fn SetupDiEnumDeviceInterfaces(
    set: usize,
    _data: usize,
    _guid: usize,
    idx: u32,
    iface: *mut u8,
) -> i32 {
    hid::enum_interfaces(set, idx, iface).unwrap_or(false) as i32
}
pub unsafe extern "win64" fn SetupDiGetDeviceInterfaceDetailW(
    set: usize,
    iface: *const u8,
    detail: *mut u8,
    size: u32,
    required: *mut u32,
    devinfo: *mut u8,
) -> i32 {
    hid::interface_detail(set, iface, detail, size, required, devinfo).unwrap_or(false) as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetupDiGetClassDevsW" => fn SetupDiGetClassDevsW,
        "SetupDiGetClassDevsA" => fn SetupDiGetClassDevsA,
        "SetupDiDestroyDeviceInfoList" => fn SetupDiDestroyDeviceInfoList,
        "SetupDiEnumDeviceInfo" => fn SetupDiEnumDeviceInfo,
        "SetupDiGetDeviceRegistryPropertyW" => fn SetupDiGetDeviceRegistryPropertyW,
        "SetupDiEnumDeviceInterfaces" => fn SetupDiEnumDeviceInterfaces,
        "SetupDiGetDeviceInterfaceDetailW" => fn SetupDiGetDeviceInterfaceDetailW,
    }
}
