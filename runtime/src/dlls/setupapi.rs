//! setupapi.dll — énumération de périphériques, sur l'arbre de
//! `devnodes.rs` : interface et classe HID (manettes, volants, joysticks,
//! via hidraw), classe Ports et interface COMPORT (ports COM), appareils USB
//! parents, processeurs (classe Processor, #1370). Une classe sans appareil
//! donne un ensemble vide, comme sous Windows.
#![allow(non_snake_case)]

use crate::dlls::devnodes;

unsafe fn guid_arg(g: *const u8) -> Option<[u8; 16]> {
    (!g.is_null()).then(|| std::ptr::read_unaligned(g as *const [u8; 16]))
}

pub unsafe extern "win64" fn SetupDiGetClassDevsW(
    guid: *const u8,
    enumerator: *const u16,
    _parent: usize,
    flags: u32,
) -> usize {
    let e = (!enumerator.is_null()).then(|| crate::dlls::wstr(enumerator));
    devnodes::get_class_devs(guid_arg(guid), e.as_deref(), flags).unwrap_or(usize::MAX)
    // INVALID_HANDLE_VALUE
}
pub unsafe extern "win64" fn SetupDiGetClassDevsA(
    guid: *const u8,
    enumerator: *const u8,
    _parent: usize,
    flags: u32,
) -> usize {
    let e = (!enumerator.is_null()).then(|| crate::dlls::cstr(enumerator));
    devnodes::get_class_devs(guid_arg(guid), e.as_deref(), flags).unwrap_or(usize::MAX)
}
pub extern "win64" fn SetupDiDestroyDeviceInfoList(set: usize) -> i32 {
    devnodes::destroy(set);
    1
}
pub unsafe extern "win64" fn SetupDiEnumDeviceInfo(set: usize, idx: u32, data: *mut u8) -> i32 {
    devnodes::enum_device_info(set, idx, data).unwrap_or(false) as i32
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
    let Some((t, value)) = devnodes::registry_property(set, data, prop) else {
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
    data: *const u8,
    guid: *const u8,
    idx: u32,
    iface: *mut u8,
) -> i32 {
    devnodes::enum_interfaces(set, data, guid_arg(guid), idx, iface).unwrap_or(false) as i32
}
pub unsafe extern "win64" fn SetupDiGetDeviceInterfaceDetailW(
    set: usize,
    iface: *const u8,
    detail: *mut u8,
    size: u32,
    required: *mut u32,
    devinfo: *mut u8,
) -> i32 {
    devnodes::interface_detail(set, iface, detail, size, required, devinfo).unwrap_or(false) as i32
}

pub fn exports() -> crate::dlls::Exports {
    let mut e = devnodes::exports();
    e.extend(crate::dlls::drivers::exports());
    e.extend(crate::dlls::setupinf::exports());
    e.extend(crate::exports! {
        "SetupDiGetClassDevsW" => fn SetupDiGetClassDevsW,
        "SetupDiGetClassDevsA" => fn SetupDiGetClassDevsA,
        "SetupDiDestroyDeviceInfoList" => fn SetupDiDestroyDeviceInfoList,
        "SetupDiEnumDeviceInfo" => fn SetupDiEnumDeviceInfo,
        "SetupDiGetDeviceRegistryPropertyW" => fn SetupDiGetDeviceRegistryPropertyW,
        "SetupDiEnumDeviceInterfaces" => fn SetupDiEnumDeviceInterfaces,
        "SetupDiGetDeviceInterfaceDetailW" => fn SetupDiGetDeviceInterfaceDetailW,
    });
    e
}
