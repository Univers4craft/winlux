//! WindowsCodecs : fonctions exportées `Interface_Méthode_Proxy` (#1063).
//!
//! PresentationCore (WPF) n'appelle pas WIC par les tables de méthodes : il
//! importe ces fonctions, qui appellent la méthode de même nom sur l'objet
//! passé en premier argument. Chaque fonction relaie donc ses arguments à
//! l'emplacement de la table de l'interface. Les numéros d'emplacement
//! suivent l'ordre public des méthodes de chaque interface (IUnknown en
//! 0 à 2, puis les interfaces parentes, comme IWICBitmapSource en 3 à 7).
//! Les deux SetResolution prennent des `double` (registres xmm en x64) et
//! ont leur propre relais.
#![allow(clippy::too_many_arguments)]

use std::ffi::c_void;

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

type Relay =
    unsafe extern "win64" fn(*mut c_void, usize, usize, usize, usize, usize, usize, usize) -> i32;

/// Appelle la méthode `slot` de l'objet COM `this` avec les arguments reçus.
#[allow(clippy::too_many_arguments)]
unsafe fn relay(
    this: *mut c_void,
    slot: usize,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    if this.is_null() {
        return E_POINTER;
    }
    let vt = *(this as *const *const usize);
    let m: Relay = std::mem::transmute(*vt.add(slot));
    m(this, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_ienum_string_next_wic(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 3, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_ienum_string_reset_wic(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iproperty_bag2_write(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_clipper_initialize(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_does_support_animation(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 18, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_does_support_lossless(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 20, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_does_support_multiframe(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 21, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_get_container_format(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 11, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_get_device_manufacturer(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 14, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_get_device_models(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 15, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_get_file_extensions(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 17, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_codec_info_get_mime_types(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 16, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_copy_palette(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 7, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_get_color_contexts(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 10, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_get_decoder_info(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 6, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_get_frame_count(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 12, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_get_frame(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 13, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_get_metadata_query_reader(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_get_preview(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 9, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_decoder_get_thumbnail(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 11, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_encoder_commit(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 11, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_encoder_create_new_frame(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 10, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_encoder_get_encoder_info(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_encoder_get_metadata_query_writer(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 12, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_encoder_initialize(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 3, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_encoder_set_palette(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 7, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_encoder_set_thumbnail(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_flip_rotator_initialize(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_decode_get_color_contexts(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 9, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_decode_get_metadata_query_reader(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_decode_get_thumbnail(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 10, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_encode_commit(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 12, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_encode_get_metadata_query_writer(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 13, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_encode_initialize(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 3, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_encode_set_color_contexts(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 7, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_encode_set_size(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_encode_set_thumbnail(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 9, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_frame_encode_write_source(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 11, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_lock_get_data_pointer_sta(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_lock_get_stride(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_scaler_initialize(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_source_copy_palette(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 6, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_source_copy_pixels(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 7, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_source_get_pixel_format(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_source_get_resolution(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_source_get_size(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 3, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_lock(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicbitmap_set_palette(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 9, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccolor_context_get_exif_color_space(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccolor_context_get_profile_bytes(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 7, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccolor_context_get_type(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 6, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccolor_context_initialize_from_memory(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccolor_transform_initialize(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccomponent_factory_create_metadata_writer_from_reader(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 31, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccomponent_factory_create_query_writer_from_block_writer(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 33, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccomponent_info_get_author(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 6, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccomponent_info_get_clsid(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccomponent_info_get_friendly_name(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 10, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccomponent_info_get_spec_version(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 9, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwiccomponent_info_get_version(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicfast_metadata_encoder_commit(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 3, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicfast_metadata_encoder_get_metadata_query_writer(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicformat_converter_initialize(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap_clipper(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 12, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap_flip_rotator(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 13, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap_from_hbitmap(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 21, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap_from_hicon(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 22, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap_from_memory(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 20, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap_from_source(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 18, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap_scaler(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 11, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_bitmap(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 17, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_component_info(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 6, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_decoder_from_file_handle(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_decoder_from_stream(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_encoder(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_fast_metadata_encoder_from_decoder(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 24, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_fast_metadata_encoder_from_frame_decode(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 25, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_format_converter(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 10, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_palette(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 9, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_query_writer_from_reader(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 27, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_query_writer(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 26, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicimaging_factory_create_stream(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 14, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_block_reader_get_count(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_block_reader_get_reader_by_index(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_query_reader_get_container_format(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 3, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_query_reader_get_enumerator(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 6, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_query_reader_get_location(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_query_reader_get_metadata_by_name(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_query_writer_remove_metadata_by_name(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicmetadata_query_writer_set_metadata_by_name(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 7, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_get_color_count(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 8, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_get_colors(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 9, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_get_type(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 7, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_has_alpha(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 12, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_initialize_custom(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 4, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_initialize_from_bitmap(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 5, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_initialize_from_palette(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 6, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpalette_initialize_predefined(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 3, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpixel_format_info_get_bits_per_pixel(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 13, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpixel_format_info_get_channel_count(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 14, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicpixel_format_info_get_channel_mask(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 15, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicstream_initialize_from_istream(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 14, a, b, c, d, e, f, g)
}

pub unsafe extern "win64" fn p_iwicstream_initialize_from_memory(
    this: *mut c_void,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    f: usize,
    g: usize,
) -> i32 {
    relay(this, 16, a, b, c, d, e, f, g)
}

type SetRes = unsafe extern "win64" fn(*mut c_void, f64, f64) -> i32;

unsafe fn set_resolution(this: *mut c_void, slot: usize, x: f64, y: f64) -> i32 {
    if this.is_null() {
        return E_POINTER;
    }
    let vt = *(this as *const *const usize);
    let m: SetRes = std::mem::transmute(*vt.add(slot));
    m(this, x, y)
}

/// IWICBitmap::SetResolution (emplacement 10).
pub unsafe extern "win64" fn p_iwic_bitmap_set_resolution(
    this: *mut c_void,
    x: f64,
    y: f64,
) -> i32 {
    set_resolution(this, 10, x, y)
}

/// IWICBitmapFrameEncode::SetResolution (emplacement 5).
pub unsafe extern "win64" fn p_iwic_bitmap_frame_encode_set_resolution(
    this: *mut c_void,
    x: f64,
    y: f64,
) -> i32 {
    set_resolution(this, 5, x, y)
}

/// WICCreateColorContext_Proxy(fabrique, sortie) : IWICImagingFactory::CreateColorContext.
pub unsafe extern "win64" fn wic_create_color_context_proxy(
    factory: *mut c_void,
    out: usize,
) -> i32 {
    relay(factory, 15, out, 0, 0, 0, 0, 0, 0)
}

/// WICCreateColorTransform_Proxy(sortie) : pas de transformation de couleurs.
pub unsafe extern "win64" fn wic_create_color_transform_proxy(out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}

/// WICSetEncoderFormat_Proxy : conversion vers un format d'encodeur, non prise en charge.
pub unsafe extern "win64" fn wic_set_encoder_format_proxy(
    _src: usize,
    _palette: usize,
    _frame: usize,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "IEnumString_Next_WIC_Proxy" => fn p_ienum_string_next_wic,
        "IEnumString_Reset_WIC_Proxy" => fn p_ienum_string_reset_wic,
        "IPropertyBag2_Write_Proxy" => fn p_iproperty_bag2_write,
        "IWICBitmapClipper_Initialize_Proxy" => fn p_iwicbitmap_clipper_initialize,
        "IWICBitmapCodecInfo_DoesSupportAnimation_Proxy" => fn p_iwicbitmap_codec_info_does_support_animation,
        "IWICBitmapCodecInfo_DoesSupportLossless_Proxy" => fn p_iwicbitmap_codec_info_does_support_lossless,
        "IWICBitmapCodecInfo_DoesSupportMultiframe_Proxy" => fn p_iwicbitmap_codec_info_does_support_multiframe,
        "IWICBitmapCodecInfo_GetContainerFormat_Proxy" => fn p_iwicbitmap_codec_info_get_container_format,
        "IWICBitmapCodecInfo_GetDeviceManufacturer_Proxy" => fn p_iwicbitmap_codec_info_get_device_manufacturer,
        "IWICBitmapCodecInfo_GetDeviceModels_Proxy" => fn p_iwicbitmap_codec_info_get_device_models,
        "IWICBitmapCodecInfo_GetFileExtensions_Proxy" => fn p_iwicbitmap_codec_info_get_file_extensions,
        "IWICBitmapCodecInfo_GetMimeTypes_Proxy" => fn p_iwicbitmap_codec_info_get_mime_types,
        "IWICBitmapDecoder_CopyPalette_Proxy" => fn p_iwicbitmap_decoder_copy_palette,
        "IWICBitmapDecoder_GetColorContexts_Proxy" => fn p_iwicbitmap_decoder_get_color_contexts,
        "IWICBitmapDecoder_GetDecoderInfo_Proxy" => fn p_iwicbitmap_decoder_get_decoder_info,
        "IWICBitmapDecoder_GetFrameCount_Proxy" => fn p_iwicbitmap_decoder_get_frame_count,
        "IWICBitmapDecoder_GetFrame_Proxy" => fn p_iwicbitmap_decoder_get_frame,
        "IWICBitmapDecoder_GetMetadataQueryReader_Proxy" => fn p_iwicbitmap_decoder_get_metadata_query_reader,
        "IWICBitmapDecoder_GetPreview_Proxy" => fn p_iwicbitmap_decoder_get_preview,
        "IWICBitmapDecoder_GetThumbnail_Proxy" => fn p_iwicbitmap_decoder_get_thumbnail,
        "IWICBitmapEncoder_Commit_Proxy" => fn p_iwicbitmap_encoder_commit,
        "IWICBitmapEncoder_CreateNewFrame_Proxy" => fn p_iwicbitmap_encoder_create_new_frame,
        "IWICBitmapEncoder_GetEncoderInfo_Proxy" => fn p_iwicbitmap_encoder_get_encoder_info,
        "IWICBitmapEncoder_GetMetadataQueryWriter_Proxy" => fn p_iwicbitmap_encoder_get_metadata_query_writer,
        "IWICBitmapEncoder_Initialize_Proxy" => fn p_iwicbitmap_encoder_initialize,
        "IWICBitmapEncoder_SetPalette_Proxy" => fn p_iwicbitmap_encoder_set_palette,
        "IWICBitmapEncoder_SetThumbnail_Proxy" => fn p_iwicbitmap_encoder_set_thumbnail,
        "IWICBitmapFlipRotator_Initialize_Proxy" => fn p_iwicbitmap_flip_rotator_initialize,
        "IWICBitmapFrameDecode_GetColorContexts_Proxy" => fn p_iwicbitmap_frame_decode_get_color_contexts,
        "IWICBitmapFrameDecode_GetMetadataQueryReader_Proxy" => fn p_iwicbitmap_frame_decode_get_metadata_query_reader,
        "IWICBitmapFrameDecode_GetThumbnail_Proxy" => fn p_iwicbitmap_frame_decode_get_thumbnail,
        "IWICBitmapFrameEncode_Commit_Proxy" => fn p_iwicbitmap_frame_encode_commit,
        "IWICBitmapFrameEncode_GetMetadataQueryWriter_Proxy" => fn p_iwicbitmap_frame_encode_get_metadata_query_writer,
        "IWICBitmapFrameEncode_Initialize_Proxy" => fn p_iwicbitmap_frame_encode_initialize,
        "IWICBitmapFrameEncode_SetColorContexts_Proxy" => fn p_iwicbitmap_frame_encode_set_color_contexts,
        "IWICBitmapFrameEncode_SetSize_Proxy" => fn p_iwicbitmap_frame_encode_set_size,
        "IWICBitmapFrameEncode_SetThumbnail_Proxy" => fn p_iwicbitmap_frame_encode_set_thumbnail,
        "IWICBitmapFrameEncode_WriteSource_Proxy" => fn p_iwicbitmap_frame_encode_write_source,
        "IWICBitmapLock_GetDataPointer_STA_Proxy" => fn p_iwicbitmap_lock_get_data_pointer_sta,
        "IWICBitmapLock_GetStride_Proxy" => fn p_iwicbitmap_lock_get_stride,
        "IWICBitmapScaler_Initialize_Proxy" => fn p_iwicbitmap_scaler_initialize,
        "IWICBitmapSource_CopyPalette_Proxy" => fn p_iwicbitmap_source_copy_palette,
        "IWICBitmapSource_CopyPixels_Proxy" => fn p_iwicbitmap_source_copy_pixels,
        "IWICBitmapSource_GetPixelFormat_Proxy" => fn p_iwicbitmap_source_get_pixel_format,
        "IWICBitmapSource_GetResolution_Proxy" => fn p_iwicbitmap_source_get_resolution,
        "IWICBitmapSource_GetSize_Proxy" => fn p_iwicbitmap_source_get_size,
        "IWICBitmap_Lock_Proxy" => fn p_iwicbitmap_lock,
        "IWICBitmap_SetPalette_Proxy" => fn p_iwicbitmap_set_palette,
        "IWICColorContext_GetExifColorSpace_Proxy" => fn p_iwiccolor_context_get_exif_color_space,
        "IWICColorContext_GetProfileBytes_Proxy" => fn p_iwiccolor_context_get_profile_bytes,
        "IWICColorContext_GetType_Proxy" => fn p_iwiccolor_context_get_type,
        "IWICColorContext_InitializeFromMemory_Proxy" => fn p_iwiccolor_context_initialize_from_memory,
        "IWICColorTransform_Initialize_Proxy" => fn p_iwiccolor_transform_initialize,
        "IWICComponentFactory_CreateMetadataWriterFromReader_Proxy" => fn p_iwiccomponent_factory_create_metadata_writer_from_reader,
        "IWICComponentFactory_CreateQueryWriterFromBlockWriter_Proxy" => fn p_iwiccomponent_factory_create_query_writer_from_block_writer,
        "IWICComponentInfo_GetAuthor_Proxy" => fn p_iwiccomponent_info_get_author,
        "IWICComponentInfo_GetCLSID_Proxy" => fn p_iwiccomponent_info_get_clsid,
        "IWICComponentInfo_GetFriendlyName_Proxy" => fn p_iwiccomponent_info_get_friendly_name,
        "IWICComponentInfo_GetSpecVersion_Proxy" => fn p_iwiccomponent_info_get_spec_version,
        "IWICComponentInfo_GetVersion_Proxy" => fn p_iwiccomponent_info_get_version,
        "IWICFastMetadataEncoder_Commit_Proxy" => fn p_iwicfast_metadata_encoder_commit,
        "IWICFastMetadataEncoder_GetMetadataQueryWriter_Proxy" => fn p_iwicfast_metadata_encoder_get_metadata_query_writer,
        "IWICFormatConverter_Initialize_Proxy" => fn p_iwicformat_converter_initialize,
        "IWICImagingFactory_CreateBitmapClipper_Proxy" => fn p_iwicimaging_factory_create_bitmap_clipper,
        "IWICImagingFactory_CreateBitmapFlipRotator_Proxy" => fn p_iwicimaging_factory_create_bitmap_flip_rotator,
        "IWICImagingFactory_CreateBitmapFromHBITMAP_Proxy" => fn p_iwicimaging_factory_create_bitmap_from_hbitmap,
        "IWICImagingFactory_CreateBitmapFromHICON_Proxy" => fn p_iwicimaging_factory_create_bitmap_from_hicon,
        "IWICImagingFactory_CreateBitmapFromMemory_Proxy" => fn p_iwicimaging_factory_create_bitmap_from_memory,
        "IWICImagingFactory_CreateBitmapFromSource_Proxy" => fn p_iwicimaging_factory_create_bitmap_from_source,
        "IWICImagingFactory_CreateBitmapScaler_Proxy" => fn p_iwicimaging_factory_create_bitmap_scaler,
        "IWICImagingFactory_CreateBitmap_Proxy" => fn p_iwicimaging_factory_create_bitmap,
        "IWICImagingFactory_CreateComponentInfo_Proxy" => fn p_iwicimaging_factory_create_component_info,
        "IWICImagingFactory_CreateDecoderFromFileHandle_Proxy" => fn p_iwicimaging_factory_create_decoder_from_file_handle,
        "IWICImagingFactory_CreateDecoderFromStream_Proxy" => fn p_iwicimaging_factory_create_decoder_from_stream,
        "IWICImagingFactory_CreateEncoder_Proxy" => fn p_iwicimaging_factory_create_encoder,
        "IWICImagingFactory_CreateFastMetadataEncoderFromDecoder_Proxy" => fn p_iwicimaging_factory_create_fast_metadata_encoder_from_decoder,
        "IWICImagingFactory_CreateFastMetadataEncoderFromFrameDecode_Proxy" => fn p_iwicimaging_factory_create_fast_metadata_encoder_from_frame_decode,
        "IWICImagingFactory_CreateFormatConverter_Proxy" => fn p_iwicimaging_factory_create_format_converter,
        "IWICImagingFactory_CreatePalette_Proxy" => fn p_iwicimaging_factory_create_palette,
        "IWICImagingFactory_CreateQueryWriterFromReader_Proxy" => fn p_iwicimaging_factory_create_query_writer_from_reader,
        "IWICImagingFactory_CreateQueryWriter_Proxy" => fn p_iwicimaging_factory_create_query_writer,
        "IWICImagingFactory_CreateStream_Proxy" => fn p_iwicimaging_factory_create_stream,
        "IWICMetadataBlockReader_GetCount_Proxy" => fn p_iwicmetadata_block_reader_get_count,
        "IWICMetadataBlockReader_GetReaderByIndex_Proxy" => fn p_iwicmetadata_block_reader_get_reader_by_index,
        "IWICMetadataQueryReader_GetContainerFormat_Proxy" => fn p_iwicmetadata_query_reader_get_container_format,
        "IWICMetadataQueryReader_GetEnumerator_Proxy" => fn p_iwicmetadata_query_reader_get_enumerator,
        "IWICMetadataQueryReader_GetLocation_Proxy" => fn p_iwicmetadata_query_reader_get_location,
        "IWICMetadataQueryReader_GetMetadataByName_Proxy" => fn p_iwicmetadata_query_reader_get_metadata_by_name,
        "IWICMetadataQueryWriter_RemoveMetadataByName_Proxy" => fn p_iwicmetadata_query_writer_remove_metadata_by_name,
        "IWICMetadataQueryWriter_SetMetadataByName_Proxy" => fn p_iwicmetadata_query_writer_set_metadata_by_name,
        "IWICPalette_GetColorCount_Proxy" => fn p_iwicpalette_get_color_count,
        "IWICPalette_GetColors_Proxy" => fn p_iwicpalette_get_colors,
        "IWICPalette_GetType_Proxy" => fn p_iwicpalette_get_type,
        "IWICPalette_HasAlpha_Proxy" => fn p_iwicpalette_has_alpha,
        "IWICPalette_InitializeCustom_Proxy" => fn p_iwicpalette_initialize_custom,
        "IWICPalette_InitializeFromBitmap_Proxy" => fn p_iwicpalette_initialize_from_bitmap,
        "IWICPalette_InitializeFromPalette_Proxy" => fn p_iwicpalette_initialize_from_palette,
        "IWICPalette_InitializePredefined_Proxy" => fn p_iwicpalette_initialize_predefined,
        "IWICPixelFormatInfo_GetBitsPerPixel_Proxy" => fn p_iwicpixel_format_info_get_bits_per_pixel,
        "IWICPixelFormatInfo_GetChannelCount_Proxy" => fn p_iwicpixel_format_info_get_channel_count,
        "IWICPixelFormatInfo_GetChannelMask_Proxy" => fn p_iwicpixel_format_info_get_channel_mask,
        "IWICStream_InitializeFromIStream_Proxy" => fn p_iwicstream_initialize_from_istream,
        "IWICStream_InitializeFromMemory_Proxy" => fn p_iwicstream_initialize_from_memory,
        "IWICBitmap_SetResolution_Proxy" => fn p_iwic_bitmap_set_resolution,
        "IWICBitmapFrameEncode_SetResolution_Proxy" => fn p_iwic_bitmap_frame_encode_set_resolution,
        "WICCreateColorContext_Proxy" => fn wic_create_color_context_proxy,
        "WICCreateColorTransform_Proxy" => fn wic_create_color_transform_proxy,
        "WICSetEncoderFormat_Proxy" => fn wic_set_encoder_format_proxy,
    }
}
