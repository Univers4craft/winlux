//! Stubs pour sechost.dll — hôte de services de sécurité.
//! Electron/Node.js interrogent le registre d'événements ETW.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn EventRegister(
    _provider: usize,
    _callback: usize,
    _context: usize,
    handle: *mut u64,
) -> u32 {
    if !handle.is_null() {
        *handle = 0;
    }
    0
}
pub unsafe extern "win64" fn EventUnregister(_handle: u64) -> u32 {
    0
}
pub unsafe extern "win64" fn EventWrite(
    _handle: u64,
    _descriptor: usize,
    _count: u32,
    _data: usize,
) -> u32 {
    0
}
pub unsafe extern "win64" fn EventSetInformation(
    _handle: u64,
    _class: u32,
    _info: usize,
    _len: u32,
) -> u32 {
    0
}
pub unsafe extern "win64" fn EventActivityIdControl(_code: u32, _guid: usize) -> u32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "EventRegister" => fn EventRegister,
        "EventUnregister" => fn EventUnregister,
        "EventWrite" => fn EventWrite,
        "EventSetInformation" => fn EventSetInformation,
        "EventActivityIdControl" => fn EventActivityIdControl,
    }
}
