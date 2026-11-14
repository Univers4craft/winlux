//! Lot F de #1410 (#1615) : derniers imports isolés (mmdevapi, urlmon,
//! imm32, dxva2, mf, mfreadwrite, d3dcompiler_47, d3d11).
//!
//! Signatures d'après les en-têtes mingw-w64 (mmdeviceapi.h, urlmon.h,
//! imm.h, physicalmonitorenumerationapi.h, mfidl.h, mfreadwrite.h,
//! d3dcompiler.h) et le comportement d'après Microsoft Learn. Les objets
//! qui demandent une chaîne Media Foundation complète (session, rendus) ou
//! l'interopérabilité WinRT de Direct3D ne sont pas encore servis : leur
//! fonction rend E_NOTIMPL, tracé, la sortie à NULL.

#![allow(non_snake_case)]

use super::comobj::{self, guid, Guid, VTable, IID_IUNKNOWN};
use super::mfcapture::{com_addref, com_release, slot};
use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;

// ---------- mmdevapi : ActivateAudioInterfaceAsync (ordinal 17) ----------

pub(crate) const IID_IACTIVATEAUDIOINTERFACEASYNCOPERATION: Guid = guid(
    0x72a22d78,
    0xcde4,
    0x431d,
    [0xb8, 0xcc, 0x84, 0x3a, 0x71, 0x19, 0x9b, 0x6d],
);
pub(crate) const IID_IACTIVATEAUDIOINTERFACECOMPLETIONHANDLER: Guid = guid(
    0x41d949ab,
    0x9862,
    0x444a,
    [0x80, 0xf6, 0xc2, 0x61, 0x33, 0x4d, 0xa5, 0xeb],
);
const DEVINTERFACE_AUDIO_RENDER: Guid = guid(
    0xe6327cad,
    0xdcec,
    0x4949,
    [0xae, 0x8a, 0x99, 0x1e, 0x97, 0x6a, 0x79, 0xd2],
);
const DEVINTERFACE_AUDIO_CAPTURE: Guid = guid(
    0x2eef81be,
    0x33fa,
    0x4800,
    [0x96, 0x70, 0x1c, 0xd4, 0x74, 0x97, 0x2c, 0x3f],
);

/// IActivateAudioInterfaceAsyncOperation : résultat de l'activation.
#[repr(C)]
struct Operation {
    vtbl: *const (),
    refs: AtomicU32,
    hr: i32,
    iface: usize,
    /// Interface demandée à l'activation (type de `iface`).
    riid: Guid,
}

unsafe extern "win64" fn op_qi(
    this: *mut Operation,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match comobj::read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IACTIVATEAUDIOINTERFACEASYNCOPERATION => {
            op_addref(this);
            *out = this as *mut c_void;
            S_OK
        }
        _ => {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn op_addref(this: *mut Operation) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn op_release(this: *mut Operation) -> u32 {
    let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let op = Box::from_raw(this);
        if op.iface != 0 {
            com_release(op.iface as *mut c_void);
        }
    }
    n
}
/// GetActivateResult(HRESULT*, IUnknown**).
unsafe extern "win64" fn op_get_result(
    this: *mut Operation,
    hr: *mut i32,
    out: *mut *mut c_void,
) -> i32 {
    if hr.is_null() || out.is_null() {
        return E_POINTER;
    }
    *hr = (*this).hr;
    *out = (*this).iface as *mut c_void;
    if !(*out).is_null() {
        com_addref(*out);
        // Programme x86 : la méthode est appelée par un mandataire, qui
        // rend la valeur telle quelle (`OutPtr`) ; l'interface est donc
        // convertie ici, sous le type demandé à l'activation (#2428).
        if crate::wow32::active() {
            let x = crate::wow32::iface_to32((*this).iface, &(*this).riid);
            if x == 0 {
                *hr = E_NOINTERFACE;
            }
            *out = x as usize as *mut c_void;
        }
    }
    S_OK
}

static OPERATION_VTBL: VTable<4> = VTable([
    op_qi as *const (),
    op_addref as *const (),
    op_release as *const (),
    op_get_result as *const (),
]);

/// Active `riid` sur le périphérique désigné : un identifiant de
/// périphérique, ou DEVINTERFACE_AUDIO_RENDER / _CAPTURE pour celui par
/// défaut (Microsoft Learn, ActivateAudioInterfaceAsync).
unsafe fn activate(path: &str, riid: *const u8, params: *const u8) -> Result<usize, i32> {
    let mut en = std::ptr::null_mut();
    let hr = super::mmdevapi::create_enumerator(
        &super::mmdevapi::IID_IMMDEVICEENUMERATOR as *const _ as *const _,
        &mut en,
    );
    if hr < 0 {
        return Err(hr);
    }
    let mut dev = std::ptr::null_mut();
    let wanted = super::ole32::parse_guid(path.trim()).map(|g| {
        let mut b = [0u8; 16];
        std::ptr::copy_nonoverlapping(&g as *const _ as *const u8, b.as_mut_ptr(), 16);
        b
    });
    let hr = match wanted {
        Some(g) if g == DEVINTERFACE_AUDIO_RENDER || g == DEVINTERFACE_AUDIO_CAPTURE => {
            let flow = u32::from(g == DEVINTERFACE_AUDIO_CAPTURE);
            let default: unsafe extern "win64" fn(*mut c_void, u32, u32, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(en, 4));
            default(en, flow, 0, &mut dev)
        }
        _ => {
            let w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let get: unsafe extern "win64" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32 =
                std::mem::transmute(slot(en, 5));
            get(en, w.as_ptr(), &mut dev)
        }
    };
    com_release(en);
    if hr < 0 || dev.is_null() {
        return Err(if hr < 0 { hr } else { E_INVALIDARG });
    }
    // IMMDevice::Activate(riid, CLSCTX_ALL, params, out).
    let act: unsafe extern "win64" fn(
        *mut c_void,
        *const u8,
        u32,
        *const u8,
        *mut *mut c_void,
    ) -> i32 = std::mem::transmute(slot(dev, 3));
    let mut out = std::ptr::null_mut();
    let hr = act(dev, riid, 0x17, params, &mut out);
    com_release(dev);
    if hr < 0 {
        Err(hr)
    } else {
        Ok(out as usize)
    }
}

/// ActivateAudioInterfaceAsync : l'activation est faite tout de suite, puis
/// IActivateAudioInterfaceCompletionHandler::ActivateCompleted est appelé
/// sur le fil de l'appelant.
pub unsafe extern "win64" fn ActivateAudioInterfaceAsync(
    path: *const u16,
    riid: *const u8,
    params: *const u8,
    handler: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if path.is_null() || riid.is_null() || handler.is_null() {
        return E_INVALIDARG;
    }
    let path = crate::dlls::wstr(path);
    let (hr, iface) = match activate(&path, riid, params) {
        Ok(p) => (S_OK, p),
        Err(hr) => {
            crate::trace!("mmdevapi!ActivateAudioInterfaceAsync({path}) : {hr:#x}");
            (hr, 0)
        }
    };
    let op = Box::into_raw(Box::new(Operation {
        vtbl: OPERATION_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        hr,
        iface,
        riid: comobj::read_guid(riid).unwrap_or_default(),
    })) as *mut c_void;
    let completed: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 =
        std::mem::transmute(slot(handler, 3));
    completed(handler, op);
    *out = op;
    S_OK
}

// ---------- urlmon : CoInternet*FeatureEnabled ----------

/// FEATURE_ENTRY_COUNT d'urlmon.h.
const FEATURE_ENTRY_COUNT: u32 = 28;

fn features() -> &'static Mutex<HashSet<u32>> {
    static F: std::sync::OnceLock<Mutex<HashSet<u32>>> = std::sync::OnceLock::new();
    F.get_or_init(|| Mutex::new(HashSet::new()))
}

/// CoInternetSetFeatureEnabled : réglage gardé pour le processus.
pub unsafe extern "win64" fn CoInternetSetFeatureEnabled(
    feature: u32,
    _flags: u32,
    enable: i32,
) -> i32 {
    if feature >= FEATURE_ENTRY_COUNT {
        return E_INVALIDARG;
    }
    let mut f = super::lock(features());
    if enable != 0 {
        f.insert(feature);
    } else {
        f.remove(&feature);
    }
    S_OK
}

/// CoInternetIsFeatureEnabled : S_OK si activé, S_FALSE sinon.
pub unsafe extern "win64" fn CoInternetIsFeatureEnabled(feature: u32, _flags: u32) -> i32 {
    if feature >= FEATURE_ENTRY_COUNT {
        return E_INVALIDARG;
    }
    if super::lock(features()).contains(&feature) {
        S_OK
    } else {
        S_FALSE
    }
}

// ---------- imm32 ----------

/// ImmIsUIMessageA : aucune fenêtre d'interface IME n'existe, le message
/// n'est jamais pour elle.
pub unsafe extern "win64" fn ImmIsUIMessageA(_hwnd: usize, _msg: u32, _w: usize, _l: isize) -> i32 {
    0
}

// ---------- dxva2 : configuration du moniteur ----------

/// GetTimingReport : comme les autres fonctions DDC/CI de dxva2, échoue
/// (pas d'accès I²C aux moniteurs) ; la structure est remise à zéro.
pub unsafe extern "win64" fn GetTimingReport(_monitor: usize, report: *mut u8) -> i32 {
    if !report.is_null() {
        // MC_TIMING_REPORT : deux DWORD et un BYTE.
        std::ptr::write_bytes(report, 0, 9);
    }
    crate::process::set_last_error(0xC026_2580); // ERROR_GRAPHICS_I2C_NOT_SUPPORTED
    0
}

// ---------- Objets pas encore servis ----------

macro_rules! absent {
    ($($dll:literal $name:ident($($a:ident),*) -> $out:ident;)*) => {$(
        #[allow(unused_variables)]
        pub unsafe extern "win64" fn $name($($a: usize),*) -> i32 {
            crate::trace!(concat!($dll, "!", stringify!($name), " : pas encore servi (E_NOTIMPL)"));
            if $out != 0 {
                crate::wow32::write_ptr($out, 0);
            }
            E_NOTIMPL
        }
    )*};
}

absent! {
    "mf" MFCreateAggregateSource(collection, out) -> out;
    "mf" MFCreateAudioRenderer(attrs, out) -> out;
    "mf" MFCreateAudioRendererActivate(out) -> out;
    "mf" MFCreateTopoLoader(out) -> out;
    "mf" MFCreateVideoRendererActivate(hwnd, out) -> out;
    "mfreadwrite" MFCreateSinkWriterFromMediaSink(sink, attrs, out) -> out;
    "d3dcompiler_47" D3DCreateLinker(out) -> out;
    "d3dcompiler_47" D3DLoadModule(src, size, out) -> out;
    "d3d11" CreateDirect3D11DeviceFromDXGIDevice(device, out) -> out;
}

pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        (
            "mmdevapi",
            crate::exports! { "ActivateAudioInterfaceAsync" => fn ActivateAudioInterfaceAsync },
        ),
        (
            "urlmon",
            crate::exports! {
                "CoInternetSetFeatureEnabled" => fn CoInternetSetFeatureEnabled,
                "CoInternetIsFeatureEnabled" => fn CoInternetIsFeatureEnabled,
            },
        ),
        (
            "imm32",
            crate::exports! { "ImmIsUIMessageA" => fn ImmIsUIMessageA },
        ),
        (
            "dxva2",
            crate::exports! { "GetTimingReport" => fn GetTimingReport },
        ),
        (
            "mf",
            crate::exports! {
                "MFCreateAggregateSource" => fn MFCreateAggregateSource,
                "MFCreateAudioRenderer" => fn MFCreateAudioRenderer,
                "MFCreateAudioRendererActivate" => fn MFCreateAudioRendererActivate,
                "MFCreateTopoLoader" => fn MFCreateTopoLoader,
                "MFCreateVideoRendererActivate" => fn MFCreateVideoRendererActivate,
            },
        ),
        (
            "mfreadwrite",
            crate::exports! {
                "MFCreateSinkWriterFromMediaSink" => fn MFCreateSinkWriterFromMediaSink,
            },
        ),
        (
            "d3dcompiler_47",
            crate::exports! {
                "D3DCreateLinker" => fn D3DCreateLinker,
                "D3DLoadModule" => fn D3DLoadModule,
            },
        ),
        (
            "d3d11",
            crate::exports! {
                "CreateDirect3D11DeviceFromDXGIDevice" => fn CreateDirect3D11DeviceFromDXGIDevice,
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonctions_urlmon_concrt_et_absentes() {
        unsafe {
            assert_eq!(CoInternetIsFeatureEnabled(21, 2), S_FALSE);
            assert_eq!(CoInternetSetFeatureEnabled(21, 2, 1), S_OK);
            assert_eq!(CoInternetIsFeatureEnabled(21, 2), S_OK);
            assert_eq!(CoInternetSetFeatureEnabled(21, 2, 0), S_OK);
            assert_eq!(CoInternetIsFeatureEnabled(99, 2), E_INVALIDARG);

            let mut out = 1usize;
            assert_eq!(
                MFCreateTopoLoader(&mut out as *mut usize as usize),
                E_NOTIMPL
            );
            assert_eq!(out, 0);

            let mut report = [0xFFu8; 9];
            assert_eq!(GetTimingReport(1, report.as_mut_ptr()), 0);
            assert_eq!(report, [0; 9]);
        }
    }
}
