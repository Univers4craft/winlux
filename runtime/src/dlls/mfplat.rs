//! Media Foundation : objets de base (attributs, types de média,
//! échantillons, tampons) et capture webcam V4L2 (voir `mfcapture`). La
//! lecture de fichiers (MFCreateSourceReaderFromURL, sessions) reste à faire.

use super::mfcapture;
use std::ffi::c_void;

// ---------- HRESULT courants ----------
const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x80004001u32 as i32;
const MF_E_UNSUPPORTED_SERVICE: i32 = 0xC00D36BAu32 as i32;
const E_POINTER: i32 = 0x80004003u32 as i32;

// ---------- MFPlat ----------
pub extern "win64" fn mf_startup(_version: u32, _flags: u32) -> i32 {
    S_OK
}
pub extern "win64" fn mf_shutdown() -> i32 {
    S_OK
}
pub unsafe extern "win64" fn mf_create_attributes(out: *mut *mut c_void, _size: u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = mfcapture::create_attributes();
    S_OK
}
pub unsafe extern "win64" fn mf_shutdown_object(_obj: *mut c_void) -> i32 {
    S_OK
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
    if out.is_null() {
        return E_POINTER;
    }
    *out = mfcapture::create_media_type();
    S_OK
}

pub unsafe extern "win64" fn mf_create_sample(out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = mfcapture::create_sample();
    S_OK
}

pub unsafe extern "win64" fn mf_create_memory_buffer(max_len: u32, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = mfcapture::create_memory_buffer(max_len as usize);
    S_OK
}

pub unsafe extern "win64" fn mf_create_aligned_memory_buffer(
    max_len: u32,
    _align: u32,
    out: *mut *mut c_void,
) -> i32 {
    mf_create_memory_buffer(max_len, out)
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

/// Horloge système MF en unités de 100 ns (monotone).
pub extern "win64" fn mf_get_system_time() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 10_000_000 + ts.tv_nsec as u64 / 100
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

/// Caméras V4L2 (source VIDCAP). Les micros passent par waveIn/WASAPI.
pub unsafe extern "win64" fn mf_enum_device_sources(
    attrs: *mut c_void,
    out_devices: *mut *mut *mut c_void,
    out_count: *mut u32,
) -> i32 {
    if out_devices.is_null() || out_count.is_null() {
        return E_POINTER;
    }
    *out_devices = std::ptr::null_mut();
    *out_count = 0;
    let video = mfcapture::requested_source_type(attrs)
        == Some(mfcapture::MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID);
    if !video {
        return S_OK;
    }
    let list = mfcapture::camera_activates();
    if list.is_empty() {
        return S_OK;
    }
    let array = super::ole32::co_task_mem_alloc(list.len() * 8) as *mut *mut c_void;
    if array.is_null() {
        return 0x8007000Eu32 as i32; // E_OUTOFMEMORY
    }
    for (i, a) in list.iter().enumerate() {
        *array.add(i) = *a;
    }
    *out_devices = array;
    *out_count = list.len() as u32;
    S_OK
}

pub unsafe extern "win64" fn mf_create_device_source(
    attrs: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let link = mfcapture::requested_symbolic_link(attrs);
    match mfcapture::camera_source(link.as_deref()) {
        Some(src) => {
            *out = src;
            S_OK
        }
        None => mfcapture::MF_E_NOT_FOUND,
    }
}

pub unsafe extern "win64" fn mf_create_source_reader_from_media_source(
    source: *mut c_void,
    attrs: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match mfcapture::create_source_reader(source, attrs) {
        Ok(r) => {
            *out = r;
            S_OK
        }
        Err(e) => {
            *out = std::ptr::null_mut();
            e
        }
    }
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

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
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
        "MFCreateAlignedMemoryBuffer"     => fn mf_create_aligned_memory_buffer,
        "MFCreateAsyncResult"             => fn mf_create_async_result,
        "MFGetSystemTime"                 => fn mf_get_system_time,
        "MFGetService"                    => fn mf_get_service,
        "MFEnumDeviceSources"             => fn mf_enum_device_sources,
        "MFCreateDeviceSource"            => fn mf_create_device_source,
        "MFCreateSourceReaderFromMediaSource" => fn mf_create_source_reader_from_media_source,
        "MFCreateSourceReaderFromURL"     => fn mf_create_source_reader_from_url,
        "MFCreateSourceReaderFromByteStream" => fn mf_create_source_reader_from_byte_stream,
        "MFCreateSinkWriterFromURL"       => fn mf_create_sink_writer_from_url,
    }
}
