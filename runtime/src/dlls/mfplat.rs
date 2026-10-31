//! Media Foundation stubs – suffisent pour que les apps qui tentent MF
//! démarrent sans crash, même si aucune lecture vidéo réelle n'a lieu.

use std::ffi::c_void;

// ---------- HRESULT courants ----------
const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOTIMPL: i32 = 0x80004001u32 as i32;
const MF_E_UNSUPPORTED_SERVICE: i32 = 0xC00D36B4u32 as i32;

// ---------- MFPlat ----------
pub extern "win64" fn mf_startup(_version: u32, _flags: u32) -> i32 {
    S_OK
}
pub extern "win64" fn mf_shutdown() -> i32 {
    S_OK
}
pub unsafe extern "win64" fn mf_create_attributes(out: *mut *mut c_void, _size: u32) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}
pub unsafe extern "win64" fn mf_shutdown_object(_obj: *mut c_void) -> i32 {
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_media_session(
    _config: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_source_resolver(out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_topology(out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_topology_node(
    _node_type: u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_media_type(out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_sample(out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_memory_buffer(_max_len: u32, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_async_result(
    _unk: *mut c_void,
    _cb: *mut c_void,
    _state: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub extern "win64" fn mf_get_system_time() -> u64 {
    0
}

pub unsafe extern "win64" fn mf_get_service(
    _provider: *mut c_void,
    _guid_service: *const u8,
    _riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    MF_E_UNSUPPORTED_SERVICE
}

pub unsafe extern "win64" fn mf_enum_device_sources(
    _attrs: *mut c_void,
    out_devices: *mut *mut *mut c_void,
    out_count: *mut u32,
) -> i32 {
    if !out_devices.is_null() {
        *out_devices = std::ptr::null_mut();
    }
    if !out_count.is_null() {
        *out_count = 0;
    }
    S_OK
}

pub unsafe extern "win64" fn mf_create_device_source(
    _attrs: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

// ---------- MFReadWrite (mfreadwrite.dll alias) ----------
pub unsafe extern "win64" fn mf_create_source_reader_from_url(
    _url: *const u16,
    _attrs: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_source_reader_from_byte_stream(
    _stream: *mut c_void,
    _attrs: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub unsafe extern "win64" fn mf_create_sink_writer_from_url(
    _url: *const u16,
    _stream: *mut c_void,
    _attrs: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

/// Énumération des transformations (décodeurs…) : Winlux n'en fournit pas
/// encore côté Media Foundation, matériel compris. Une liste vide fait
/// choisir à l'application son propre décodeur, comme sur un Windows sans
/// codec ; le décodage matériel passe par DXVA2 (`dxva2.dll`).
pub unsafe extern "win64" fn mft_enum_ex(
    _category: *const c_void,
    _flags: u32,
    _input: *const c_void,
    _output: *const c_void,
    activates: *mut *mut c_void,
    count: *mut u32,
) -> i32 {
    if activates.is_null() || count.is_null() {
        return E_INVALIDARG;
    }
    *activates = std::ptr::null_mut();
    *count = 0;
    S_OK
}

pub unsafe extern "win64" fn mft_enum2(
    category: *const c_void,
    flags: u32,
    input: *const c_void,
    output: *const c_void,
    _attrs: *mut c_void,
    activates: *mut *mut c_void,
    count: *mut u32,
) -> i32 {
    mft_enum_ex(category, flags, input, output, activates, count)
}

pub unsafe extern "win64" fn mft_enum(
    _category: *const c_void, // GUID par valeur = par adresse en x64
    _flags: u32,
    _input: *const c_void,
    _output: *const c_void,
    _attrs: *mut c_void,
    clsids: *mut *mut c_void,
    count: *mut u32,
) -> i32 {
    if clsids.is_null() || count.is_null() {
        return E_INVALIDARG;
    }
    *clsids = std::ptr::null_mut();
    *count = 0;
    S_OK
}

pub unsafe extern "win64" fn mf_create_dxgi_device_manager(
    _token: *mut u32,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "MFTEnumEx"                       => fn mft_enum_ex,
        "MFTEnum2"                        => fn mft_enum2,
        "MFTEnum"                         => fn mft_enum,
        "MFCreateDXGIDeviceManager"       => fn mf_create_dxgi_device_manager,
        "MFStartup"                       => fn mf_startup,
        "MFShutdown"                      => fn mf_shutdown,
        "MFCreateAttributes"              => fn mf_create_attributes,
        "MFShutdownObject"                => fn mf_shutdown_object,
        "MFCreateMediaSession"            => fn mf_create_media_session,
        "MFCreateSourceResolver"          => fn mf_create_source_resolver,
        "MFCreateTopology"                => fn mf_create_topology,
        "MFCreateTopologyNode"            => fn mf_create_topology_node,
        "MFCreateMediaType"               => fn mf_create_media_type,
        "MFCreateSample"                  => fn mf_create_sample,
        "MFCreateMemoryBuffer"            => fn mf_create_memory_buffer,
        "MFCreateAsyncResult"             => fn mf_create_async_result,
        "MFGetSystemTime"                 => fn mf_get_system_time,
        "MFGetService"                    => fn mf_get_service,
        "MFEnumDeviceSources"             => fn mf_enum_device_sources,
        "MFCreateDeviceSource"            => fn mf_create_device_source,
        "MFCreateSourceReaderFromURL"     => fn mf_create_source_reader_from_url,
        "MFCreateSourceReaderFromByteStream" => fn mf_create_source_reader_from_byte_stream,
        "MFCreateSinkWriterFromURL"       => fn mf_create_sink_writer_from_url,
    }
}
