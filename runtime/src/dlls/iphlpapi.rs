//! Stubs pour iphlpapi.dll — informations réseau.
//! Electron interroge les adaptateurs pour détecter la connectivité.
#![allow(non_snake_case)]

const ERROR_BUFFER_OVERFLOW: u32 = 111;

pub unsafe extern "win64" fn GetAdaptersAddresses(
    _family: u32,
    _flags: u32,
    _reserved: usize,
    _addrs: usize,
    size: *mut u32,
) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    ERROR_BUFFER_OVERFLOW
}
pub unsafe extern "win64" fn GetAdaptersInfo(_info: usize, size: *mut u32) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    ERROR_BUFFER_OVERFLOW
}
pub unsafe extern "win64" fn GetNetworkParams(_info: usize, size: *mut u32) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    ERROR_BUFFER_OVERFLOW
}
pub unsafe extern "win64" fn GetBestRoute(_dest: u32, _src: u32, _route: usize) -> u32 {
    0
}
pub unsafe extern "win64" fn GetIfTable(_table: usize, size: *mut u32, _order: i32) -> u32 {
    if !size.is_null() {
        *size = 0;
    }
    122 // ERROR_INSUFFICIENT_BUFFER
}
pub unsafe extern "win64" fn GetBestInterfaceEx(_addr: usize, _idx: *mut u32) -> u32 {
    0
}
pub unsafe extern "win64" fn GetNumberOfInterfaces(num: *mut u32) -> u32 {
    if !num.is_null() {
        *num = 1;
    }
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetAdaptersAddresses" => fn GetAdaptersAddresses,
        "GetAdaptersInfo" => fn GetAdaptersInfo,
        "GetNetworkParams" => fn GetNetworkParams,
        "GetBestRoute" => fn GetBestRoute,
        "GetIfTable" => fn GetIfTable,
        "GetBestInterfaceEx" => fn GetBestInterfaceEx,
        "GetNumberOfInterfaces" => fn GetNumberOfInterfaces,
    }
}
