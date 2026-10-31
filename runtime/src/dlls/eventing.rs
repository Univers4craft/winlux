//! Process-local ETW provider lifecycle, with no recording sessions enabled.
//! Windows permits EventWriteTransfer to succeed without writing anything when
//! no session records the provider. This does not implement trace controllers,
//! Windows Event Log, or ETL capture.
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct Providers {
    next: u64,
    entries: HashMap<u64, [u8; 16]>,
}

fn providers() -> &'static Mutex<Providers> {
    static PROVIDERS: OnceLock<Mutex<Providers>> = OnceLock::new();
    PROVIDERS.get_or_init(|| Mutex::new(Providers::default()))
}

pub unsafe extern "win64" fn event_register(
    provider: *const [u8; 16],
    _callback: usize,
    _context: *mut c_void,
    handle: *mut u64,
) -> u32 {
    if handle.is_null() {
        return 87;
    }
    *handle = 0;
    if provider.is_null() {
        return 87;
    }
    let mut state = providers()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.entries.len() >= 1024 || state.next == u64::MAX {
        return 8;
    }
    state.next += 1;
    let id = state.next;
    state.entries.insert(id, provider.read_unaligned());
    *handle = id;
    // No enable callback: there are no enabled trace sessions in this backend.
    0
}

pub unsafe extern "win64" fn event_unregister(handle: u64) -> u32 {
    if handle == 0 {
        return 0;
    }
    let mut state = providers()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.entries.remove(&handle).is_some() {
        0
    } else {
        6
    }
}

pub unsafe extern "win64" fn event_write_transfer(
    handle: u64,
    descriptor: *const c_void,
    _activity: *const c_void,
    _related: *const c_void,
    count: u32,
    data: *const c_void,
) -> u32 {
    // Zero handles from a failed registration are explicitly safe to ignore.
    if handle == 0 {
        return 0;
    }
    let state = providers()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !state.entries.contains_key(&handle) {
        return 6;
    }
    if descriptor.is_null() || count > 128 || (count != 0 && data.is_null()) {
        return 87;
    }
    // No session consumes events: do not read the user payload or claim an ETL
    // record was created. This is the documented inactive-provider behavior.
    0
}

/// EventSetInformation : les traits du fournisseur (nom, groupe) ne
/// servent qu'aux sessions ETW, qui n'existent pas ; un fournisseur
/// enregistré accepte donc l'information, comme Windows sans session.
pub extern "win64" fn event_set_information(
    handle: u64,
    _class: u32,
    _info: *const c_void,
    _len: u32,
) -> u32 {
    let state = providers()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if handle != 0 && !state.entries.contains_key(&handle) {
        return 6;
    }
    0
}

/// EventEnabled / EventProviderEnabled : aucune session n'écoute.
pub extern "win64" fn event_enabled(_handle: u64, _descriptor: *const c_void) -> u8 {
    0
}
pub extern "win64" fn event_provider_enabled(_handle: u64, _level: u8, _keyword: u64) -> u8 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "EventSetInformation" => fn event_set_information,
        "EventEnabled" => fn event_enabled,
        "EventProviderEnabled" => fn event_provider_enabled,
        "EventRegister" => fn event_register,
        "EventUnregister" => fn event_unregister,
        "EventWriteTransfer" => fn event_write_transfer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::{null, null_mut};

    #[test]
    fn provider_lifecycle_and_disabled_writes() {
        unsafe {
            let provider = [0x42u8; 16];
            let mut first = 0;
            let mut second = 0;
            assert_eq!(event_register(&provider, 0, null_mut(), &mut first), 0);
            assert_eq!(event_register(&provider, 0, null_mut(), &mut second), 0);
            assert_ne!(first, second);
            let descriptor = [0u8; 16];
            let descriptor = descriptor.as_ptr().cast();
            assert_eq!(
                event_write_transfer(first, descriptor, null(), null(), 0, null()),
                0
            );
            assert_eq!(
                event_write_transfer(first, descriptor, null(), null(), 129, null()),
                87
            );
            assert_eq!(event_unregister(first), 0);
            assert_eq!(event_unregister(first), 6);
            assert_eq!(
                event_write_transfer(first, descriptor, null(), null(), 0, null()),
                6
            );
            assert_eq!(event_unregister(second), 0);
            assert_eq!(
                event_write_transfer(0, null(), null(), null(), 0, null()),
                0
            );
            assert_eq!(event_unregister(0), 0);
            first = 123;
            assert_eq!(event_register(null(), 0, null_mut(), &mut first), 87);
            assert_eq!(first, 0);
            assert_eq!(event_register(&provider, 0, null_mut(), null_mut()), 87);
        }
    }
}
