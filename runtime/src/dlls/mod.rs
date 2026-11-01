//! Les « DLL » intégrées de Winlux : du code Rust natif Linux, exposé
//! avec la convention d'appel Windows. Le chargeur relie directement les
//! imports du programme à ces fonctions, sans fabriquer de vraies DLL.

pub mod a11y;
pub mod advapi32;
pub mod advapi32_services;
pub mod audio_com;
pub mod bcrypt;
pub mod clipboard;
pub mod comctl32;
pub mod comdlg32;
pub mod comobj;
pub mod crt;
pub mod crt_runtime;
pub mod crt_scan;
pub mod crt_stdio;
pub mod crt_ucrt;
pub mod crypt32;
pub mod cxx_eh;
pub mod d2d1;
pub mod d2d_raster;
pub mod d3d11;
pub mod d3d12;
pub mod d3d9;
pub mod d3d9_shader;
pub mod d3d_shader;
pub mod dbghelp;
pub mod dinput8;
pub mod dsound;
pub mod dwmapi;
pub mod dwrite;
pub mod dxgi;
pub mod dxva2;
pub mod evdev;
pub mod eventing;
pub mod gdi32;
pub mod gdi32_ext;
pub mod gdi32_fonts;
pub mod gl_bindings;
pub mod gl_legacy;
pub mod gpu_caps;
pub mod gui;
mod gui_cairo;
pub mod gui_x11;
pub mod iphlpapi;
pub mod ipp;
pub mod kernel32;
pub mod kernel32_cpu;
pub mod kernel32_ext;
pub mod kernel32_misc;
pub mod libgcc;
pub mod libstdcpp;
pub mod mfcapture;
pub mod mfplat;
pub mod mmap;
pub mod mmdevapi;
pub mod mscoree;
pub mod msvcrt;
pub mod notifyicon;
pub mod ntdll;
pub mod ole32;
pub mod oleaut32;
pub mod powrprof;
pub mod print_pdf;
pub mod propsys;
pub mod quartz;
pub mod resource;
pub mod rpcrt4;
pub mod sechost;
pub mod seh;
pub mod setupapi;
pub mod sfnt;
pub mod shell32;
pub mod shlwapi;
pub mod shlwapi_reg;
pub mod spirv;
pub mod taskschd;
pub mod user32;
pub mod user32_ext;
pub mod userenv;
pub mod uxtheme;
pub mod v4l2;
pub mod va_bindings;
pub mod version;
pub mod vk_backend;
pub mod vk_bindings;
pub mod windowscodecs;
pub mod winfile;
pub mod winhttp;
pub mod winlux_capture;
pub mod winlux_gpu;
pub mod winlux_media;
pub mod winlux_pty;
pub mod winmm;
pub mod winmm_waveout;
pub mod winspool;
pub mod wintrust;
pub mod ws2_32;
pub mod xaudio2;
pub mod xinput;

use std::collections::HashMap;
use std::sync::OnceLock;

/// Table des exports d'une DLL intégrée : nom → adresse (fonction ou donnée).
pub type Exports = HashMap<&'static str, usize>;

/// Déclare une table d'exports. `fn` pour une fonction, `data` pour une variable exportée.
#[macro_export]
macro_rules! exports {
    ($($name:literal => $kind:ident $val:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut m: $crate::dlls::Exports = std::collections::HashMap::new();
        $( m.insert($name, $crate::exports!(@addr $kind $val)); )*
        m
    }};
    (@addr fn $f:expr) => { $f as *const () as usize };
    (@addr data $v:expr) => { std::ptr::addr_of!($v) as usize };
}

struct Builtin {
    name: &'static str,
    exports: Exports,
    /// Fausse image (une page commençant par « MZ ») servant de HMODULE.
    handle: usize,
}

fn fake_module() -> usize {
    let page = Box::leak(Box::new([0u8; 4096]));
    page[0] = b'M';
    page[1] = b'Z';
    page.as_ptr() as usize
}

fn builtins() -> &'static Vec<Builtin> {
    static B: OnceLock<Vec<Builtin>> = OnceLock::new();
    B.get_or_init(|| {
        let crt = {
            let mut e = msvcrt::exports();
            e.extend(crt::exports());
            e.extend(crt_stdio::exports());
            e.extend(crt_scan::exports());
            e.extend(crt_runtime::exports());
            e.extend(crt_ucrt::exports());
            e.extend(cxx_eh::exports());
            e
        };
        let mut ucrt = crt.clone();
        ucrt.extend(msvcrt::ucrt_exports());
        ucrt.extend(crt::exports());
        ucrt.extend(crt_stdio::exports());
        ucrt.extend(crt_scan::exports());
        ucrt.extend(crt_runtime::exports());
        ucrt.extend(crt_ucrt::exports());
        ucrt.extend(cxx_eh::exports());
        vec![
            Builtin {
                name: "kernel32",
                exports: {
                    let mut e = kernel32::exports();
                    e.extend(winfile::exports());
                    e.extend(kernel32_misc::exports());
                    e.extend(kernel32_cpu::exports());
                    e.extend(resource::exports());
                    e.extend(mmap::exports());
                    e.extend(winlux_pty::exports());
                    e.extend(winlux_capture::exports());
                    e.extend(powrprof::kernel32_exports());
                    e.extend(gpu_caps::exports());
                    e.extend(winlux_media::exports());
                    e.extend(crate::mounts::exports());
                    e.extend(crate::archive::ffi::exports());
                    e.extend(crate::imaging::api::exports());
                    e.extend(winlux_gpu::exports());
                    e.extend(seh::kernel32_exports());
                    e.extend(kernel32_ext::kernel32_exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "ntdll",
                exports: {
                    let mut e = ntdll::exports();
                    e.extend(seh::exports());
                    e.extend(kernel32_ext::ntdll_exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "msvcrt",
                exports: crt,
                handle: fake_module(),
            },
            Builtin {
                name: "ucrtbase",
                exports: ucrt,
                handle: fake_module(),
            },
            Builtin {
                name: "user32",
                exports: {
                    let mut e = user32::exports();
                    e.extend(user32_ext::exports());
                    e.extend(clipboard::exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "advapi32",
                exports: {
                    let mut e = advapi32::exports();
                    e.extend(advapi32_services::exports());
                    e.extend(kernel32_ext::advapi32_exports());
                    e.extend(eventing::exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "rpcrt4",
                exports: rpcrt4::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "winhttp",
                exports: winhttp::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "bcryptprimitives",
                exports: bcrypt::primitives_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "ole32",
                exports: ole32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "bcrypt",
                exports: bcrypt::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "version",
                exports: version::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "shell32",
                exports: shell32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "mscoree",
                exports: mscoree::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "ws2_32",
                exports: ws2_32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "gdi32",
                exports: {
                    let mut e = gdi32::exports();
                    e.extend(gdi32_ext::exports());
                    e.extend(gdi32_fonts::exports());
                    e.extend(winspool::gdi_exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "mfplat",
                exports: mfplat::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "winmm",
                exports: {
                    let mut e = winmm::exports();
                    e.extend(winmm_waveout::exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "dsound",
                exports: dsound::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "mmdevapi",
                exports: mmdevapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "audioses",
                exports: mmdevapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "avrt",
                exports: mmdevapi::avrt_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "xaudio2_9",
                exports: xaudio2::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "xaudio2_8",
                exports: xaudio2::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "xaudio2_7",
                exports: xaudio2::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "comctl32",
                exports: comctl32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "shlwapi",
                exports: shlwapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "oleaut32",
                exports: oleaut32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "quartz",
                exports: quartz::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "mfreadwrite",
                exports: mfplat::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "mf",
                exports: mfplat::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "crypt32",
                exports: crypt32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dxgi",
                exports: dxgi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "d3d11",
                exports: d3d11::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "d3d12",
                exports: d3d12::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dxva2",
                exports: dxva2::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "d3d9",
                exports: d3d9::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dbghelp",
                exports: dbghelp::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "powrprof",
                exports: powrprof::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "setupapi",
                exports: setupapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "wintrust",
                exports: wintrust::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "iphlpapi",
                exports: iphlpapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "userenv",
                exports: userenv::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "windowscodecs",
                exports: windowscodecs::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "d2d1",
                exports: d2d1::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dwrite",
                exports: dwrite::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "uxtheme",
                exports: uxtheme::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dwmapi",
                exports: dwmapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "propsys",
                exports: propsys::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "sechost",
                exports: sechost::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "xinput1_4",
                exports: xinput::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dinput8",
                exports: dinput8::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dinput",
                exports: dinput8::legacy_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "winspool",
                exports: winspool::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "comdlg32",
                exports: comdlg32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "libgcc_s_seh-1",
                exports: libgcc::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "libstdc++-6",
                exports: libstdcpp::exports(),
                handle: fake_module(),
            },
        ]
    })
}

/// Nom de DLL normalisé : minuscules, sans « .dll », avec les alias connus
/// (api-ms-win-crt-* → ucrtbase, kernelbase → kernel32, msvcr120 → msvcrt…).
pub fn normalize(dll: &str) -> String {
    let l = dll.to_ascii_lowercase();
    let l = l.rsplit(['\\', '/']).next().unwrap_or(&l).to_string();
    let base = l.strip_suffix(".dll").unwrap_or(&l);
    if base.starts_with("api-ms-win-crt-") || base == "vcruntime140" || base == "vcruntime140_1" {
        return "ucrtbase".into();
    }
    if base.starts_with("api-ms-win-core-com-l1-") {
        return "ole32".into();
    }
    if base.starts_with("api-ms-win-core-localregistry-l1-") {
        return "advapi32".into();
    }
    if base.starts_with("api-ms-win-core-rtlsupport-l1-") {
        return "ntdll".into();
    }
    // Contrats d'alimentation (PowerGetActiveScheme, PowerSetActiveScheme...).
    if base.starts_with("api-ms-win-power-") {
        return "powrprof".into();
    }
    if base.starts_with("api-ms-win-core-")
        || base.starts_with("api-ms-win-downlevel-kernel32-l2-")
        || base == "kernelbase"
    {
        return "kernel32".into();
    }
    // Security API-set contracts expose the same public security functions
    // implemented by our Advapi32 backend, rather than a separate disk DLL.
    if base.starts_with("api-ms-win-security-base-l1-") {
        return "advapi32".into();
    }
    if base.starts_with("api-ms-win-eventing-provider-l1-") {
        return "advapi32".into();
    }
    if base.starts_with("api-ms-win-core-winrt-") {
        return "ole32".into();
    }
    if base.starts_with("api-ms-win-core-path-l1-") {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-synch-") || base.starts_with("api-ms-win-core-sysinfo-") {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-processthreads-")
        || base.starts_with("api-ms-win-core-heap-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-memory-") || base.starts_with("api-ms-win-core-file-") {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-errorhandling-")
        || base.starts_with("api-ms-win-core-handle-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-libraryloader-")
        || base.starts_with("api-ms-win-core-string-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-console-") || base.starts_with("api-ms-win-core-profile-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-debug-") || base.starts_with("api-ms-win-core-io-") {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-interlocked-")
        || base.starts_with("api-ms-win-core-localization-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-timezone-") || base.starts_with("api-ms-win-core-fibers-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-datetime-")
        || base.starts_with("api-ms-win-core-namedpipe-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-registry-")
        || base.starts_with("api-ms-win-core-realtime-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-psapi-")
        || base.starts_with("api-ms-win-core-processenvironment-")
    {
        return "kernel32".into();
    }
    if base.starts_with("api-ms-win-core-util-") || base.starts_with("api-ms-win-core-wow64-") {
        return "kernel32".into();
    }
    if base.starts_with("msvcr") && base != "msvcrt" {
        return "msvcrt".into();
    }
    // Alias courants pour les DLL de contrôles communs / shell.
    if base == "comctl32" || base == "iertutil" {
        return "comctl32".into();
    }
    // XAudio2 redistribuable (NuGet) et anciennes versions 2.x par COM.
    if base == "xaudio2_9redist" {
        return "xaudio2_9".into();
    }
    if base.starts_with("xaudio2_") && base != "xaudio2_8" && base != "xaudio2_9" {
        return "xaudio2_7".into();
    }
    // winspool.drv (et son API-set) : le spooler Winlux.
    if base == "winspool.drv"
        || base == "winspool"
        || base.starts_with("ext-ms-win-printer-winspool-")
    {
        return "winspool".into();
    }
    if base == "shlwapi" {
        return "shlwapi".into();
    }
    if base == "oleaut32" {
        return "oleaut32".into();
    }
    // Toutes les versions d'XInput partagent la même implémentation.
    if base.starts_with("xinput1_") || base == "xinput9_1_0" {
        return "xinput1_4".into();
    }
    base.to_string()
}

pub fn is_builtin(normalized: &str) -> bool {
    builtins().iter().any(|b| b.name == normalized)
}

pub fn lookup(normalized: &str, func: &str) -> Option<usize> {
    let b = builtins().iter().find(|b| b.name == normalized)?;
    let r = b.exports.get(func).copied();
    // Les API « A/W » d'api-ms-win-core peuvent aussi vivre dans ntdll (Rtl*).
    r.or_else(|| {
        if normalized == "kernel32" {
            // KernelBase exporte aussi les Reg* (api-ms-win-core-registry).
            lookup("ntdll", func).or_else(|| {
                func.starts_with("Reg")
                    .then(|| lookup("advapi32", func))
                    .flatten()
            })
        } else {
            None
        }
    })
}

/// HMODULE d'une DLL intégrée ou d'une DLL réelle déjà chargée.
pub fn module_handle(dll: &str) -> Option<usize> {
    let n = normalize(dll);
    if let Some(b) = builtins().iter().find(|b| b.name == n) {
        return Some(b.handle);
    }
    crate::loader::find_module_by_name(dll).map(|m| m.base)
}

/// Si `h` est le HMODULE d'une DLL intégrée, renvoie son nom normalisé.
pub fn builtin_by_handle(h: usize) -> Option<&'static str> {
    builtins().iter().find(|b| b.handle == h).map(|b| b.name)
}

// ---------- Utilitaires partagés par les DLL ----------

pub(crate) fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub unsafe fn cstr(p: *const u8) -> String {
    if p.is_null() {
        return String::new();
    }
    let n = crate::printf::narrow_len(p);
    decode_ansi(std::slice::from_raw_parts(p, n))
}

const CP1252: [u16; 32] = [
    0x20ac, 0x0081, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008d, 0x017d, 0x008f, 0x0090, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
    0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0x009d, 0x017e, 0x0178,
];

/// Codepage ANSI par défaut de Winlux (Windows-1252), convertie vers UTF-8
/// pour l'interface Linux. Les API Unicode `W` évitent cette conversion.
pub fn decode_ansi(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            let n = if (0x80..=0x9f).contains(&b) {
                CP1252[(b - 0x80) as usize] as u32
            } else {
                b as u32
            };
            char::from_u32(n).unwrap_or('\u{fffd}')
        })
        .collect()
}

pub fn encode_ansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| {
            let n = c as u32;
            if n <= 0x7f || (0xa0..=0xff).contains(&n) {
                return n as u8;
            }
            CP1252
                .iter()
                .position(|&x| x as u32 == n)
                .map(|i| i as u8 + 0x80)
                .unwrap_or(b'?')
        })
        .collect()
}

pub unsafe fn wstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let n = crate::printf::wide_len(p);
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

/// Copie une chaîne dans un tampon Windows « A » de `cap` caractères.
/// Convention Win32 : renvoie la longueur copiée, ou la taille nécessaire (avec le 0) si trop petit.
pub unsafe fn put_a(s: &str, buf: *mut u8, cap: u32) -> u32 {
    let b = s.as_bytes();
    if buf.is_null() || (cap as usize) <= b.len() {
        return b.len() as u32 + 1;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
    *buf.add(b.len()) = 0;
    b.len() as u32
}

pub unsafe fn put_w(s: &str, buf: *mut u16, cap: u32) -> u32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    if buf.is_null() || (cap as usize) <= w.len() {
        return w.len() as u32 + 1;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    w.len() as u32
}

#[cfg(test)]
mod tests {
    use super::{lookup, normalize};

    #[test]
    fn normalise_les_alias_de_dll() {
        assert_eq!(normalize("KERNELBASE.dll"), "kernel32");
        assert_eq!(normalize("api-ms-win-crt-runtime-l1-1-0.dll"), "ucrtbase");
        assert_eq!(normalize("C:\\Windows\\System32\\MSVCR120.DLL"), "msvcrt");
    }

    #[test]
    fn api_sets_reutilisent_leurs_veritables_exports() {
        for (contract, host, symbol) in [
            (
                "api-ms-win-eventing-provider-l1-1-0.dll",
                "advapi32",
                "EventRegister",
            ),
            (
                "api-ms-win-core-localregistry-l1-1-0.dll",
                "advapi32",
                "RegOpenKeyExW",
            ),
            ("api-ms-win-core-com-l1-1-0.dll", "ole32", "CoInitializeEx"),
            (
                "api-ms-win-core-rtlsupport-l1-1-0.dll",
                "ntdll",
                "RtlUnwindEx",
            ),
            (
                "api-ms-win-downlevel-kernel32-l2-1-0.dll",
                "kernel32",
                "GetCurrentProcessId",
            ),
        ] {
            let expected = lookup(host, symbol).expect("backend export must exist");
            assert_eq!(
                lookup(&normalize(contract), symbol),
                Some(expected),
                "{contract}"
            );
        }
        assert_eq!(normalize("api-ms-win-security-base-l1-1-0.dll"), "advapi32");
        assert!(lookup(
            &normalize("api-ms-win-security-base-l1-1-0.dll"),
            "SetSecurityDescriptorDacl"
        )
        .is_none());
        assert_eq!(
            normalize("api-ms-win-security-unknown-l1-1-0.dll"),
            "api-ms-win-security-unknown-l1-1-0"
        );
    }
}
