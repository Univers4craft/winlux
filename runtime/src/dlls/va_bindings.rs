//! Chargement dynamique de VA-API (`libva.so.2` + `libva-drm.so.2`).
//!
//! VA-API est l'interface Linux de décodage vidéo matériel (Intel, AMD, et
//! NVIDIA via `nvidia-vaapi-driver`). Tout est optionnel : sans la
//! bibliothèque, sans nœud `/dev/dri/renderD*` ou sans pilote, les fonctions
//! renvoient `None` et le décodage logiciel de l'application prend le relais.
#![allow(non_snake_case, dead_code)]

use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::OnceLock;

pub(crate) type VADisplay = *mut c_void;
pub(crate) type VAStatus = c_int;
pub(crate) type VAConfigID = u32;
pub(crate) type VAContextID = u32;
pub(crate) type VASurfaceID = u32;
pub(crate) type VABufferID = u32;

pub(crate) const VA_STATUS_SUCCESS: VAStatus = 0;
pub(crate) const VA_INVALID_ID: u32 = 0xffff_ffff;
pub(crate) const VA_INVALID_SURFACE: u32 = 0xffff_ffff;

pub(crate) const VA_ENTRYPOINT_VLD: c_int = 1;
pub(crate) const VA_RT_FORMAT_YUV420: u32 = 0x1;
pub(crate) const VA_RT_FORMAT_YUV420_10: u32 = 0x100;
pub(crate) const VA_PROGRESSIVE: c_int = 0x1;

pub(crate) const VA_PICTURE_PARAMETER_BUFFER: c_int = 0;
pub(crate) const VA_IQ_MATRIX_BUFFER: c_int = 1;
pub(crate) const VA_SLICE_PARAMETER_BUFFER: c_int = 4;
pub(crate) const VA_SLICE_DATA_BUFFER: c_int = 5;

// Profils VA (va.h).
pub(crate) const VA_PROFILE_MPEG2_SIMPLE: c_int = 0;
pub(crate) const VA_PROFILE_MPEG2_MAIN: c_int = 1;
pub(crate) const VA_PROFILE_H264_MAIN: c_int = 6;
pub(crate) const VA_PROFILE_H264_HIGH: c_int = 7;
pub(crate) const VA_PROFILE_VC1_SIMPLE: c_int = 8;
pub(crate) const VA_PROFILE_VC1_MAIN: c_int = 9;
pub(crate) const VA_PROFILE_VC1_ADVANCED: c_int = 10;
pub(crate) const VA_PROFILE_H264_CONSTRAINED_BASELINE: c_int = 13;
pub(crate) const VA_PROFILE_HEVC_MAIN: c_int = 17;
pub(crate) const VA_PROFILE_HEVC_MAIN10: c_int = 18;
pub(crate) const VA_PROFILE_VP9_PROFILE0: c_int = 19;
pub(crate) const VA_PROFILE_VP9_PROFILE2: c_int = 21;
pub(crate) const VA_PROFILE_AV1_PROFILE0: c_int = 32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VAImageFormat {
    pub fourcc: u32,
    pub byte_order: u32,
    pub bits_per_pixel: u32,
    pub depth: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub alpha_mask: u32,
    pub va_reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct VAImage {
    pub image_id: u32,
    pub format: VAImageFormat,
    pub buf: VABufferID,
    pub width: u16,
    pub height: u16,
    pub data_size: u32,
    pub num_planes: u32,
    pub pitches: [u32; 3],
    pub offsets: [u32; 3],
    pub num_palette_entries: i32,
    pub entry_bytes: i32,
    pub component_order: [i8; 4],
    pub va_reserved: [u32; 4],
}

macro_rules! va_table {
    ($table:ident { $( $field:ident : $sym:literal => fn($($arg:ty),*) $(-> $ret:ty)? ),* $(,)? }) => {
        pub(crate) struct $table {
            $( pub $field: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )*
        }
        unsafe impl Send for $table {}
        unsafe impl Sync for $table {}
        impl $table {
            unsafe fn load(lib: *mut c_void) -> Option<Self> {
                Some(Self {
                    $( $field: {
                        let p = libc::dlsym(lib, concat!($sym, "\0").as_ptr().cast());
                        if p.is_null() {
                            crate::log_debug!("vaapi", "symbole absent : {}", $sym);
                            return None;
                        }
                        std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p)
                    }, )*
                })
            }
        }
    };
}

va_table!(VaFns {
    initialize: "vaInitialize" => fn(VADisplay, *mut c_int, *mut c_int) -> VAStatus,
    terminate: "vaTerminate" => fn(VADisplay) -> VAStatus,
    query_vendor_string: "vaQueryVendorString" => fn(VADisplay) -> *const c_char,
    max_num_profiles: "vaMaxNumProfiles" => fn(VADisplay) -> c_int,
    max_num_entrypoints: "vaMaxNumEntrypoints" => fn(VADisplay) -> c_int,
    query_config_profiles: "vaQueryConfigProfiles" => fn(VADisplay, *mut c_int, *mut c_int) -> VAStatus,
    query_config_entrypoints: "vaQueryConfigEntrypoints" => fn(VADisplay, c_int, *mut c_int, *mut c_int) -> VAStatus,
    create_config: "vaCreateConfig" => fn(VADisplay, c_int, c_int, *mut c_void, c_int, *mut VAConfigID) -> VAStatus,
    destroy_config: "vaDestroyConfig" => fn(VADisplay, VAConfigID) -> VAStatus,
    create_surfaces: "vaCreateSurfaces" => fn(VADisplay, u32, u32, u32, *mut VASurfaceID, u32, *mut c_void, u32) -> VAStatus,
    destroy_surfaces: "vaDestroySurfaces" => fn(VADisplay, *mut VASurfaceID, c_int) -> VAStatus,
    create_context: "vaCreateContext" => fn(VADisplay, VAConfigID, c_int, c_int, c_int, *mut VASurfaceID, c_int, *mut VAContextID) -> VAStatus,
    destroy_context: "vaDestroyContext" => fn(VADisplay, VAContextID) -> VAStatus,
    create_buffer: "vaCreateBuffer" => fn(VADisplay, VAContextID, c_int, u32, u32, *mut c_void, *mut VABufferID) -> VAStatus,
    destroy_buffer: "vaDestroyBuffer" => fn(VADisplay, VABufferID) -> VAStatus,
    begin_picture: "vaBeginPicture" => fn(VADisplay, VAContextID, VASurfaceID) -> VAStatus,
    render_picture: "vaRenderPicture" => fn(VADisplay, VAContextID, *mut VABufferID, c_int) -> VAStatus,
    end_picture: "vaEndPicture" => fn(VADisplay, VAContextID) -> VAStatus,
    sync_surface: "vaSyncSurface" => fn(VADisplay, VASurfaceID) -> VAStatus,
    derive_image: "vaDeriveImage" => fn(VADisplay, VASurfaceID, *mut VAImage) -> VAStatus,
    destroy_image: "vaDestroyImage" => fn(VADisplay, u32) -> VAStatus,
    map_buffer: "vaMapBuffer" => fn(VADisplay, VABufferID, *mut *mut c_void) -> VAStatus,
    unmap_buffer: "vaUnmapBuffer" => fn(VADisplay, VABufferID) -> VAStatus,
    error_str: "vaErrorStr" => fn(VAStatus) -> *const c_char,
});

va_table!(VaDrmFns {
    get_display_drm: "vaGetDisplayDRM" => fn(c_int) -> VADisplay,
});

pub(crate) struct VaLib {
    pub va: VaFns,
    pub drm: VaDrmFns,
}

/// Charge libva une seule fois. `WINLUX_NO_VAAPI=1` simule son absence.
pub(crate) fn lib() -> Option<&'static VaLib> {
    static L: OnceLock<Option<VaLib>> = OnceLock::new();
    L.get_or_init(|| unsafe {
        if std::env::var_os("WINLUX_NO_VAAPI").is_some_and(|v| v == "1") {
            return None;
        }
        let va = libc::dlopen(c"libva.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if va.is_null() {
            crate::log_info!("vaapi", "libva absente : décodage vidéo logiciel");
            return None;
        }
        let drm = libc::dlopen(
            c"libva-drm.so.2".as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        );
        if drm.is_null() {
            crate::log_info!("vaapi", "libva-drm absente : décodage vidéo logiciel");
            return None;
        }
        Some(VaLib {
            va: VaFns::load(va)?,
            drm: VaDrmFns::load(drm)?,
        })
    })
    .as_ref()
}

/// Un affichage VA ouvert sur un nœud de rendu DRM ; fermé au `Drop`.
pub(crate) struct VaSession {
    pub display: VADisplay,
    pub fd: c_int,
    pub node: String,
    pub vendor: String,
}

unsafe impl Send for VaSession {}

impl VaSession {
    /// Ouvre le premier nœud `/dev/dri/renderD128..135` dont le pilote VA
    /// s'initialise. Aucun nœud ou aucun pilote : `None`.
    pub fn open() -> Option<Self> {
        let l = lib()?;
        for minor in 128..136 {
            let node = format!("/dev/dri/renderD{minor}");
            let Ok(path) = std::ffi::CString::new(node.clone()) else {
                continue;
            };
            unsafe {
                let fd = libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
                if fd < 0 {
                    continue;
                }
                let display = (l.drm.get_display_drm)(fd);
                if display.is_null() {
                    libc::close(fd);
                    continue;
                }
                let (mut major, mut minor_v) = (0, 0);
                if (l.va.initialize)(display, &mut major, &mut minor_v) != VA_STATUS_SUCCESS {
                    (l.va.terminate)(display);
                    libc::close(fd);
                    continue;
                }
                let v = (l.va.query_vendor_string)(display);
                let vendor = if v.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(v).to_string_lossy().into_owned()
                };
                crate::log_info!(
                    "vaapi",
                    "{} : VA-API {}.{} « {} »",
                    node,
                    major,
                    minor_v,
                    vendor
                );
                return Some(VaSession {
                    display,
                    fd,
                    node,
                    vendor,
                });
            }
        }
        None
    }

    /// Profils pour lesquels le pilote propose le point d'entrée de décodage (VLD).
    pub fn decode_profiles(&self) -> Vec<c_int> {
        let Some(l) = lib() else { return Vec::new() };
        unsafe {
            let max = (l.va.max_num_profiles)(self.display).max(0) as usize;
            let mut profiles = vec![0 as c_int; max.max(1)];
            let mut n: c_int = 0;
            if (l.va.query_config_profiles)(self.display, profiles.as_mut_ptr(), &mut n)
                != VA_STATUS_SUCCESS
            {
                return Vec::new();
            }
            profiles.truncate((n.max(0) as usize).min(max));
            let max_ep = (l.va.max_num_entrypoints)(self.display).max(1) as usize;
            profiles
                .into_iter()
                .filter(|&p| {
                    let mut eps = vec![0 as c_int; max_ep];
                    let mut ne: c_int = 0;
                    (l.va.query_config_entrypoints)(self.display, p, eps.as_mut_ptr(), &mut ne)
                        == VA_STATUS_SUCCESS
                        && eps[..(ne.max(0) as usize).min(max_ep)].contains(&VA_ENTRYPOINT_VLD)
                })
                .collect()
        }
    }
}

impl Drop for VaSession {
    fn drop(&mut self) {
        if let Some(l) = lib() {
            unsafe {
                (l.va.terminate)(self.display);
            }
        }
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taille_vaimage() {
        // sizeof(VAImage) avec va.h 1.20 sur x86-64.
        assert_eq!(std::mem::size_of::<VAImage>(), 120);
    }

    #[test]
    fn ouverture_sans_materiel_ne_plante_pas() {
        // Sur une machine sans GPU, l'ouverture échoue proprement.
        let _ = VaSession::open().map(|s| s.decode_profiles());
    }
}
