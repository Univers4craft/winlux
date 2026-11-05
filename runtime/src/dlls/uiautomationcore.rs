//! UIAutomationCore.dll : le minimum pour les programmes qui exposent leur
//! contenu à l'accessibilité (fonctions `Uia*`, d'après Microsoft Learn).
//! Winlux n'a pas encore de client UI Automation : aucun lecteur d'écran
//! n'écoute, les événements sont acceptés sans suite.

const S_OK: u32 = 0;
const E_NOINTERFACE: u32 = 0x8000_4002;
const E_NOTIMPL: u32 = 0x8000_4001;
const E_INVALIDARG: u32 = 0x8007_0057;

#[repr(C)]
struct Vtbl {
    query_interface: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> u32,
    add_ref: unsafe extern "win64" fn(usize) -> u32,
    release: unsafe extern "win64" fn(usize) -> u32,
}

#[repr(C)]
struct Reserved {
    vtbl: &'static Vtbl,
}

unsafe extern "win64" fn qi(this: usize, iid: *const u8, out: *mut usize) -> u32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // IID_IUnknown : 00000000-0000-0000-C000-000000000046.
    const IUNKNOWN: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
    if !iid.is_null() && std::slice::from_raw_parts(iid, 16) == IUNKNOWN {
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}

unsafe extern "win64" fn static_ref(_this: usize) -> u32 {
    1
}

static VTBL: Vtbl = Vtbl {
    query_interface: qi,
    add_ref: static_ref,
    release: static_ref,
};
static NOT_SUPPORTED: Reserved = Reserved { vtbl: &VTBL };
static MIXED: Reserved = Reserved { vtbl: &VTBL };

unsafe extern "win64" fn return_raw_element_provider(
    _hwnd: usize,
    _wp: usize,
    _lp: isize,
    _el: usize,
) -> isize {
    0
}

unsafe extern "win64" fn host_provider_from_hwnd(_hwnd: usize, out: *mut usize) -> u32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    E_NOTIMPL
}

unsafe extern "win64" fn ok_stub(_a: usize, _b: usize, _c: usize, _d: usize) -> u32 {
    S_OK
}

unsafe extern "win64" fn clients_are_listening() -> i32 {
    0
}

unsafe extern "win64" fn not_supported_value(out: *mut usize) -> u32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = &NOT_SUPPORTED as *const Reserved as usize;
    S_OK
}

unsafe extern "win64" fn mixed_attribute_value(out: *mut usize) -> u32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = &MIXED as *const Reserved as usize;
    S_OK
}

unsafe extern "win64" fn lookup_id(_kind: i32, _guid: *const u8) -> i32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "UiaReturnRawElementProvider" => fn return_raw_element_provider,
        "UiaHostProviderFromHwnd" => fn host_provider_from_hwnd,
        "UiaRaiseAutomationEvent" => fn ok_stub,
        "UiaRaiseAutomationPropertyChangedEvent" => fn ok_stub,
        "UiaRaiseStructureChangedEvent" => fn ok_stub,
        "UiaRaiseNotificationEvent" => fn ok_stub,
        "UiaRaiseAsyncContentLoadedEvent" => fn ok_stub,
        "UiaRaiseTextEditTextChangedEvent" => fn ok_stub,
        "UiaRaiseActiveTextPositionChangedEvent" => fn ok_stub,
        "UiaRaiseChangesEvent" => fn ok_stub,
        "UiaDisconnectProvider" => fn ok_stub,
        "UiaDisconnectAllProviders" => fn ok_stub,
        "UiaClientsAreListening" => fn clients_are_listening,
        "UiaGetReservedNotSupportedValue" => fn not_supported_value,
        "UiaGetReservedMixedAttributeValue" => fn mixed_attribute_value,
        "UiaLookupId" => fn lookup_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_reservees_et_iunknown() {
        unsafe {
            let mut p = 0usize;
            assert_eq!(not_supported_value(&mut p), S_OK);
            assert_ne!(p, 0);
            let obj = &*(p as *const Reserved);
            let iid = [0u8, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
            let mut q = 0usize;
            assert_eq!((obj.vtbl.query_interface)(p, iid.as_ptr(), &mut q), S_OK);
            assert_eq!(q, p);
            assert_eq!((obj.vtbl.release)(p), 1);
            let mut m = 0usize;
            mixed_attribute_value(&mut m);
            assert_ne!(m, p);
            assert_eq!(clients_are_listening(), 0);
        }
    }
}
