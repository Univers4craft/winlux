//! Media Foundation : objets de base (attributs, types de média,
//! échantillons, tampons), capture webcam V4L2 (voir `mfcapture`) et
//! lecture de fichiers par `MFCreateSourceReaderFromURL` (voir `mfreader`).

use super::mfcapture;
use std::ffi::c_void;

// ---------- HRESULT courants ----------
const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
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

/// MFCreateMediaBufferFromMediaType (#1895) : tampon mémoire assez grand
/// pour `duration` (en 100 ns) du type : MF_MT_SAMPLE_SIZE, sinon le débit
/// audio MF_MT_AUDIO_AVG_BYTES_PER_SECOND, au moins `min_len` octets.
pub unsafe extern "win64" fn mf_create_media_buffer_from_media_type(
    media_type: *mut c_void,
    duration: i64,
    min_len: u32,
    _min_align: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if media_type.is_null() {
        return E_INVALIDARG;
    }
    // IMFAttributes::GetUINT32 est la 8e entrée de la table.
    let get_u32: unsafe extern "win64" fn(*mut c_void, *const u8, *mut u32) -> i32 =
        std::mem::transmute(mfcapture::slot(media_type, 7));
    let mut sample = 0u32;
    let mut size = 0u64;
    if get_u32(
        media_type,
        mfcapture::MF_MT_SAMPLE_SIZE.as_ptr(),
        &mut sample,
    ) >= 0
    {
        size = u64::from(sample);
    } else {
        let mut rate = 0u32;
        if get_u32(
            media_type,
            super::mfreader::MF_MT_AUDIO_AVG_BYTES_PER_SECOND.as_ptr(),
            &mut rate,
        ) >= 0
        {
            size = u64::from(rate) * duration.max(0) as u64 / 10_000_000;
        }
    }
    let size = size.max(u64::from(min_len)).min(u64::from(u32::MAX)) as usize;
    *out = mfcapture::create_memory_buffer(size);
    S_OK
}

/// MFCopyImage : copie `lines` lignes de `width` octets entre deux pas.
pub unsafe extern "win64" fn mf_copy_image(
    dst: *mut u8,
    dst_stride: i32,
    src: *const u8,
    src_stride: i32,
    width: u32,
    lines: u32,
) -> i32 {
    if dst.is_null() || src.is_null() {
        return E_POINTER;
    }
    for y in 0..lines as isize {
        std::ptr::copy(
            src.offset(y * src_stride as isize),
            dst.offset(y * dst_stride as isize),
            width as usize,
        );
    }
    S_OK
}

/// MFCreateAsyncResult : structure MFASYNCRESULT (lot F, #1615).
pub unsafe extern "win64" fn mf_create_async_result(
    unk: *mut c_void,
    cb: *mut c_void,
    state: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    super::lot_f_mfplat::MFCreateAsyncResult(unk, cb, state, out)
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
/// MFCreateSourceReaderFromURL : fichier local décodé par le moteur
/// multimédia de Winlux (voir `mfreader`).
pub unsafe extern "win64" fn mf_create_source_reader_from_url(
    url: *const u16,
    attrs: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() || url.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    match super::mfreader::create_from_url(&super::wstr(url), attrs) {
        Ok(reader) => {
            *out = reader;
            0
        }
        Err(hr) => hr,
    }
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
        "MFCreateAlignedMemoryBuffer"     => fn mf_create_aligned_memory_buffer,
        "MFCreateMediaBufferWrapper"      => fn mfcapture::mf_create_media_buffer_wrapper,
        "MFCreateMediaBufferFromMediaType" => fn mf_create_media_buffer_from_media_type,
        "MFCopyImage"                     => fn mf_copy_image,
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

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call_lock(b: *mut c_void) -> (*mut u8, u32, u32) {
        let f: unsafe extern "win64" fn(*mut c_void, *mut *mut u8, *mut u32, *mut u32) -> i32 =
            std::mem::transmute(mfcapture::slot(b, 3));
        let (mut p, mut max, mut cur) = (std::ptr::null_mut(), 0, 0);
        assert_eq!(f(b, &mut p, &mut max, &mut cur), S_OK);
        let u: unsafe extern "win64" fn(*mut c_void) -> i32 =
            std::mem::transmute(mfcapture::slot(b, 4));
        u(b);
        (p, max, cur)
    }

    /// Discord (#1895) : une tranche d'un tampon, sans copie.
    #[test]
    fn tampon_enveloppe_et_type_de_media() {
        unsafe {
            let mut inner = std::ptr::null_mut();
            assert_eq!(mf_create_memory_buffer(16, &mut inner), S_OK);
            let mut wrap = std::ptr::null_mut();
            assert_eq!(
                mfcapture::mf_create_media_buffer_wrapper(inner, 10, 8, &mut wrap),
                E_INVALIDARG
            );
            assert!(wrap.is_null());
            assert_eq!(
                mfcapture::mf_create_media_buffer_wrapper(inner, 4, 8, &mut wrap),
                S_OK
            );
            let (p, _, _) = call_lock(inner);
            let (q, max, cur) = call_lock(wrap);
            assert_eq!((q as usize - p as usize, max, cur), (4, 8, 8));
            // Le tampon d'origine reste vivant tant que l'enveloppe le tient.
            mfcapture::com_release(inner);
            *q = 7;
            mfcapture::com_release(wrap);

            let mut mt = std::ptr::null_mut();
            assert_eq!(mf_create_media_type(&mut mt), S_OK);
            let set_u32: unsafe extern "win64" fn(*mut c_void, *const u8, u32) -> i32 =
                std::mem::transmute(mfcapture::slot(mt, 21));
            assert_eq!(
                set_u32(mt, mfcapture::MF_MT_SAMPLE_SIZE.as_ptr(), 4096),
                S_OK
            );
            let mut b = std::ptr::null_mut();
            assert_eq!(
                mf_create_media_buffer_from_media_type(mt, 0, 100, 0, &mut b),
                S_OK
            );
            assert_eq!(call_lock(b).1, 4096);
            mfcapture::com_release(b);
            mfcapture::com_release(mt);

            let src = [1u8, 2, 3, 9, 4, 5, 6, 9];
            let mut dst = [0u8; 6];
            assert_eq!(
                mf_copy_image(dst.as_mut_ptr(), 3, src.as_ptr(), 4, 3, 2),
                S_OK
            );
            assert_eq!(dst, [1, 2, 3, 4, 5, 6]);
        }
    }
}
