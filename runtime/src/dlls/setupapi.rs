//! Stubs pour setupapi.dll — énumération de périphériques.
//! Electron/Chromium cherche les GPU et autres périphériques.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn SetupDiGetClassDevsW(
    _guid: usize, _enumerator: *const u16, _parent: usize, _flags: u32,
) -> usize {
    usize::MAX // INVALID_HANDLE_VALUE
}
pub extern "win64" fn SetupDiDestroyDeviceInfoList(_set: usize) -> i32 { 1 }
pub unsafe extern "win64" fn SetupDiEnumDeviceInfo(
    _set: usize, _idx: u32, _data: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn SetupDiGetDeviceRegistryPropertyW(
    _set: usize, _data: usize, _prop: u32, _type: *mut u32,
    _buf: *mut u8, _size: u32, _required: *mut u32,
) -> i32 { 0 }
pub unsafe extern "win64" fn SetupDiEnumDeviceInterfaces(
    _set: usize, _data: usize, _guid: usize, _idx: u32, _iface: usize,
) -> i32 { 0 }

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetupDiGetClassDevsW" => fn SetupDiGetClassDevsW,
        "SetupDiDestroyDeviceInfoList" => fn SetupDiDestroyDeviceInfoList,
        "SetupDiEnumDeviceInfo" => fn SetupDiEnumDeviceInfo,
        "SetupDiGetDeviceRegistryPropertyW" => fn SetupDiGetDeviceRegistryPropertyW,
        "SetupDiEnumDeviceInterfaces" => fn SetupDiEnumDeviceInterfaces,
    }
}
