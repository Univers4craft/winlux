//! DLL de périphériques et de composition que des programmes importent
//! directement sans toujours s'en servir (#26) : winscard (cartes à puce),
//! bthprops.cpl (Bluetooth classique), dcomp (DirectComposition) et fontsub
//! (sous-ensembles de polices pour l'impression). Sans elles, le programme
//! ne démarrait pas du tout (import absent). Chaque fonction rend le code
//! documenté de Windows quand le service ou le matériel manque : aucun
//! lecteur de carte, aucune radio Bluetooth, composition indisponible.
//! Les applications (Qt, outils réseau) prévoient ces réponses.

const SCARD_S_SUCCESS: u32 = 0;
const SCARD_E_INVALID_HANDLE: u32 = 0x8010_0003;
const SCARD_E_NO_SERVICE: u32 = 0x8010_001D;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_NOT_FOUND: u32 = 1168;
const ERROR_INVALID_HANDLE: u32 = 6;
const E_NOTIMPL: u32 = 0x8000_4001;
/// fontsub : ERR_GENERIC (1000), « échec de traitement de la police ».
const FONTSUB_ERR_GENERIC: u32 = 1000;

fn last_error(code: u32) {
    crate::process::set_last_error(code);
}

// --- winscard.dll -------------------------------------------------------------------------

/// SCARD_IO_REQUEST { dwProtocol, cbPciLength } exportés en données.
#[repr(C)]
pub struct ScardIoRequest {
    protocol: u32,
    length: u32,
}
pub static G_RG_SCARD_T0_PCI: ScardIoRequest = ScardIoRequest {
    protocol: 1,
    length: 8,
};
pub static G_RG_SCARD_T1_PCI: ScardIoRequest = ScardIoRequest {
    protocol: 2,
    length: 8,
};
pub static G_RG_SCARD_RAW_PCI: ScardIoRequest = ScardIoRequest {
    protocol: 0x10000,
    length: 8,
};

unsafe extern "win64" fn scard_establish_context(
    _scope: u32,
    _r1: usize,
    _r2: usize,
    ctx: *mut usize,
) -> u32 {
    if !ctx.is_null() {
        *ctx = 0;
    }
    SCARD_E_NO_SERVICE
}
unsafe extern "win64" fn scard_release_context(_ctx: usize) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_is_valid_context(_ctx: usize) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_list_readers(
    _ctx: usize,
    _groups: usize,
    _readers: usize,
    len: *mut u32,
) -> u32 {
    if !len.is_null() {
        *len = 0;
    }
    SCARD_E_NO_SERVICE
}
unsafe extern "win64" fn scard_free_memory(_ctx: usize, _mem: usize) -> u32 {
    SCARD_S_SUCCESS
}
unsafe extern "win64" fn scard_get_status_change(
    _ctx: usize,
    _timeout: u32,
    _states: usize,
    _n: u32,
) -> u32 {
    SCARD_E_NO_SERVICE
}
unsafe extern "win64" fn scard_connect(
    _ctx: usize,
    _reader: usize,
    _share: u32,
    _protocols: u32,
    card: *mut usize,
    _active: *mut u32,
) -> u32 {
    if !card.is_null() {
        *card = 0;
    }
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_disconnect(_card: usize, _disposition: u32) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_begin_transaction(_card: usize) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_end_transaction(_card: usize, _disposition: u32) -> u32 {
    SCARD_E_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn scard_transmit(
    _card: usize,
    _send_pci: usize,
    _send: usize,
    _send_len: u32,
    _recv_pci: usize,
    _recv: usize,
    _recv_len: *mut u32,
) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_status(
    _card: usize,
    _names: usize,
    _names_len: *mut u32,
    _state: *mut u32,
    _protocol: *mut u32,
    _atr: usize,
    _atr_len: *mut u32,
) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_cancel(_ctx: usize) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_reconnect(
    _card: usize,
    _share: u32,
    _pref: u32,
    _init: u32,
    _active: *mut u32,
) -> u32 {
    SCARD_E_INVALID_HANDLE
}
unsafe extern "win64" fn scard_get_attrib(
    _card: usize,
    _attr: u32,
    _buf: usize,
    _len: *mut u32,
) -> u32 {
    SCARD_E_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn scard_control(
    _card: usize,
    _code: u32,
    _in: usize,
    _in_len: u32,
    _out: usize,
    _out_len: u32,
    _returned: *mut u32,
) -> u32 {
    SCARD_E_INVALID_HANDLE
}
/// Événement « service démarré » : aucun service, aucun événement.
unsafe extern "win64" fn scard_access_started_event() -> usize {
    0
}
unsafe extern "win64" fn scard_release_started_event() {}

pub fn winscard_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SCardEstablishContext" => fn scard_establish_context,
        "SCardReleaseContext" => fn scard_release_context,
        "SCardIsValidContext" => fn scard_is_valid_context,
        "SCardListReadersA" => fn scard_list_readers, "SCardListReadersW" => fn scard_list_readers,
        "SCardFreeMemory" => fn scard_free_memory,
        "SCardGetStatusChangeA" => fn scard_get_status_change,
        "SCardGetStatusChangeW" => fn scard_get_status_change,
        "SCardConnectA" => fn scard_connect, "SCardConnectW" => fn scard_connect,
        "SCardDisconnect" => fn scard_disconnect, "SCardReconnect" => fn scard_reconnect,
        "SCardBeginTransaction" => fn scard_begin_transaction,
        "SCardEndTransaction" => fn scard_end_transaction,
        "SCardTransmit" => fn scard_transmit,
        "SCardStatusA" => fn scard_status, "SCardStatusW" => fn scard_status,
        "SCardCancel" => fn scard_cancel, "SCardGetAttrib" => fn scard_get_attrib,
        "SCardControl" => fn scard_control,
        "SCardAccessStartedEvent" => fn scard_access_started_event,
        "SCardReleaseStartedEvent" => fn scard_release_started_event,
        "g_rgSCardT0Pci" => data G_RG_SCARD_T0_PCI,
        "g_rgSCardT1Pci" => data G_RG_SCARD_T1_PCI,
        "g_rgSCardRawPci" => data G_RG_SCARD_RAW_PCI,
    }
}

// --- bthprops.cpl (Bluetooth classique) ---------------------------------------------------

/// BluetoothFindFirstRadio / FirstDevice : aucune radio visible par l'API
/// Windows (le Bluetooth de Winlux se règle dans Paramètres, côté BlueZ).
unsafe extern "win64" fn bt_find_first(_params: usize, handle: *mut usize) -> usize {
    if !handle.is_null() {
        *handle = 0;
    }
    last_error(ERROR_NO_MORE_ITEMS);
    0
}
unsafe extern "win64" fn bt_find_next(_find: usize, item: *mut usize) -> i32 {
    if !item.is_null() {
        *item = 0;
    }
    last_error(ERROR_NO_MORE_ITEMS);
    0
}
unsafe extern "win64" fn bt_find_close(_find: usize) -> i32 {
    last_error(ERROR_INVALID_HANDLE);
    0
}
unsafe extern "win64" fn bt_get_info(_handle: usize, _info: usize) -> u32 {
    ERROR_NOT_FOUND
}
unsafe extern "win64" fn bt_is_state(_radio: usize) -> i32 {
    0
}
unsafe extern "win64" fn bt_enable(_radio: usize, _enable: i32) -> i32 {
    0
}
unsafe extern "win64" fn bt_remove_device(_address: usize) -> u32 {
    ERROR_NOT_FOUND
}
unsafe extern "win64" fn bt_authenticate_ex(
    _parent: usize,
    _radio: usize,
    _device: usize,
    _oob: usize,
    _req: u32,
) -> u32 {
    ERROR_NOT_FOUND
}
unsafe extern "win64" fn bt_register_auth(
    _device: usize,
    handle: *mut usize,
    _callback: usize,
    _param: usize,
) -> u32 {
    if !handle.is_null() {
        *handle = 0;
    }
    ERROR_NOT_FOUND
}
unsafe extern "win64" fn bt_unregister_auth(_handle: usize) -> i32 {
    last_error(ERROR_INVALID_HANDLE);
    0
}
unsafe extern "win64" fn bt_send_auth_response_ex(_radio: usize, _response: usize) -> u32 {
    ERROR_NOT_FOUND
}
unsafe extern "win64" fn bt_set_service_state(
    _radio: usize,
    _device: usize,
    _guid: usize,
    _flags: u32,
) -> u32 {
    ERROR_NOT_FOUND
}
unsafe extern "win64" fn bt_enumerate_installed_services(
    _radio: usize,
    _device: usize,
    count: *mut u32,
    _guids: usize,
) -> u32 {
    if !count.is_null() {
        *count = 0;
    }
    ERROR_NOT_FOUND
}

pub fn bthprops_exports() -> crate::dlls::Exports {
    crate::exports! {
        "BluetoothFindFirstRadio" => fn bt_find_first, "BluetoothFindNextRadio" => fn bt_find_next,
        "BluetoothFindRadioClose" => fn bt_find_close, "BluetoothGetRadioInfo" => fn bt_get_info,
        "BluetoothFindFirstDevice" => fn bt_find_first, "BluetoothFindNextDevice" => fn bt_find_next,
        "BluetoothFindDeviceClose" => fn bt_find_close, "BluetoothGetDeviceInfo" => fn bt_get_info,
        "BluetoothIsConnectable" => fn bt_is_state, "BluetoothIsDiscoverable" => fn bt_is_state,
        "BluetoothEnableDiscovery" => fn bt_enable, "BluetoothEnableIncomingConnections" => fn bt_enable,
        "BluetoothRemoveDevice" => fn bt_remove_device,
        "BluetoothAuthenticateDeviceEx" => fn bt_authenticate_ex,
        "BluetoothRegisterForAuthenticationEx" => fn bt_register_auth,
        "BluetoothUnregisterAuthentication" => fn bt_unregister_auth,
        "BluetoothSendAuthenticationResponseEx" => fn bt_send_auth_response_ex,
        "BluetoothSetServiceState" => fn bt_set_service_state,
        "BluetoothEnumerateInstalledServices" => fn bt_enumerate_installed_services,
    }
}

// --- dcomp.dll ----------------------------------------------------------------------------

/// DCompositionCreateDevice(2/3) : composition indisponible ; les applis
/// (Chromium, Qt, ANGLE) reviennent alors à la présentation DXGI classique.
unsafe extern "win64" fn dcomp_create_device(_device: usize, _iid: usize, out: *mut usize) -> u32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn dcomp_create_surface_handle(
    _access: u32,
    _attrs: usize,
    out: *mut usize,
) -> u32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn dcomp_wait_for_compositor_clock(
    _count: u32,
    _handles: usize,
    _timeout: u32,
) -> u32 {
    // WAIT_FAILED : pas d'horloge de composition.
    0xFFFF_FFFF
}

pub fn dcomp_exports() -> crate::dlls::Exports {
    crate::exports! {
        "DCompositionCreateDevice" => fn dcomp_create_device,
        "DCompositionCreateDevice2" => fn dcomp_create_device,
        "DCompositionCreateDevice3" => fn dcomp_create_device,
        "DCompositionCreateSurfaceHandle" => fn dcomp_create_surface_handle,
        "DCompositionWaitForCompositorClock" => fn dcomp_wait_for_compositor_clock,
    }
}

// --- fontsub.dll --------------------------------------------------------------------------

/// CreateFontPackage / MergeFontPackage : pas de sous-ensemble ; Qt et les
/// pilotes d'impression incorporent alors la police entière.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn create_font_package(
    _src: usize,
    _src_size: u32,
    out: *mut usize,
    out_size: *mut u32,
    written: *mut u32,
    _flags: u16,
    _ttc_index: u16,
    _format: u16,
    _lang: u16,
    _platform: u16,
    _encoding: u16,
    _keep: usize,
    _keep_count: u16,
    _alloc: usize,
    _realloc: usize,
    _free: usize,
    _reserved: usize,
) -> u32 {
    for p in [out_size, written] {
        if !p.is_null() {
            *p = 0;
        }
    }
    if !out.is_null() {
        *out = 0;
    }
    FONTSUB_ERR_GENERIC
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn merge_font_package(
    _font1: usize,
    _font1_size: u32,
    _font2: usize,
    _font2_size: u32,
    out: *mut usize,
    out_size: *mut u32,
    written: *mut u32,
    _mode: u16,
    _alloc: usize,
    _realloc: usize,
    _free: usize,
    _reserved: usize,
) -> u32 {
    for p in [out_size, written] {
        if !p.is_null() {
            *p = 0;
        }
    }
    if !out.is_null() {
        *out = 0;
    }
    FONTSUB_ERR_GENERIC
}

pub fn fontsub_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateFontPackage" => fn create_font_package,
        "MergeFontPackage" => fn merge_font_package,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_documentes_sans_materiel() {
        unsafe {
            let mut ctx = 1usize;
            assert_eq!(
                scard_establish_context(2, 0, 0, &mut ctx),
                SCARD_E_NO_SERVICE
            );
            assert_eq!(ctx, 0);
            let mut h = 1usize;
            assert_eq!(bt_find_first(0, &mut h), 0);
            assert_eq!(h, 0);
            let mut dev = 1usize;
            assert_eq!(dcomp_create_device(0, 0, &mut dev), E_NOTIMPL);
            assert_eq!(dev, 0);
        }
    }
}
