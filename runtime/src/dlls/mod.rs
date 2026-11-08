//! Les « DLL » intégrées de Winlux : du code Rust natif Linux, exposé
//! avec la convention d'appel Windows. Le chargeur relie directement les
//! imports du programme à ces fonctions, sans fabriquer de vraies DLL.

/// Méthode absente : rend E_NOTIMPL et le note une fois dans le journal, avec
/// son nom, pour savoir ce qu'une application attend encore.
macro_rules! unsupported {
    ($name:literal) => {{
        unsafe extern "win64" fn f() -> i32 {
            $crate::dlls::d2d1::unsupported_call($name)
        }
        f as *const ()
    }};
}

pub mod a11y;
pub mod absent_devices;
pub mod accel;
pub mod actctx;
pub mod advapi32;
pub mod advapi32_crypto;
pub mod advapi32_rsa;
pub mod advapi32_services;
pub mod apps_win32;
pub mod audio_com;
pub mod bcrypt;
pub mod chromium_win32;
pub mod clipboard;
pub mod comctl32;
pub mod comdlg32;
pub mod comdlg32_files;
pub mod comdlg32_find;
pub mod comdlg32_ui;
pub mod common_apps;
pub mod comobj;
pub mod console_input;
pub mod controls;
pub mod coremessaging;
pub mod credentials;
pub mod crt;
pub mod crt_extra;
pub mod crt_more;
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
pub mod d3d8;
pub mod d3d9;
pub mod d3d9_shader;
pub mod d3d_shader;
pub mod d3dcompiler;
pub mod d3dx9;
pub mod d3dx9_asm;
pub mod d3dx9_draw;
pub mod d3dx9_effect;
pub mod d3dx9_hlsl;
pub mod d3dx9_mesh;
pub mod d3dx9_shader;
pub mod dataobject;
pub mod dbghelp;
pub mod ddeml;
pub mod ddraw;
pub mod devio;
pub mod devnodes;
pub mod dialogs;
pub mod dinput8;
pub mod dirwatch;
pub mod displaymode;
pub mod dpapi;
pub mod dplayx;
pub mod dpnet;
pub mod dsound;
pub mod dwmapi;
pub mod dwrite;
pub mod dwrite_analyzer;
pub mod dwrite_glyphs;
pub mod dxbc;
pub mod dxgi;
pub(crate) mod dxgi_private;
pub mod dxva2;
pub mod electron_win32;
pub mod errorinfo;
pub mod evdev;
pub mod eventing;
pub mod fiber64;
pub mod filedialog_com;
pub mod ftp_apps;
pub mod games_win32;
pub mod gdi32;
pub mod gdi32_ext;
pub mod gdi32_fonts;
pub mod gdi32_more;
pub mod gdi32_palette;
pub mod gdi32_path;
pub mod gdiplus;
pub mod gl_bindings;
pub mod gl_legacy;
pub mod gpu_caps;
pub mod gui;
mod gui_cairo;
pub mod gui_x11;
pub mod handle_broker;
pub mod hid;
pub mod hlsl;
pub mod hostcall;
pub mod hostssl;
pub mod icons;
pub mod image_list_com;
pub mod imm32;
pub mod inherit;
pub mod inifile;
pub mod iphlpapi;
pub mod ipp;
pub(crate) mod jobs;
pub mod kernel32;
pub mod kernel32_cpu;
pub mod kernel32_ext;
pub mod kernel32_fs;
pub mod kernel32_legacy;
pub mod kernel32_misc;
pub mod kernel32_node;
pub mod knownfolders;
pub mod legacy_com;
pub mod legacy_dlls;
pub mod libgcc;
pub mod libstdcpp;
pub mod menus;
pub mod mfcapture;
pub mod mfplat;
pub mod mfreader;
pub mod mmap;
pub mod mmdevapi;
pub mod monitors;
pub mod mouse;
pub mod mpr;
pub mod mscoree;
pub mod msvcp140;
pub mod msvcrt;
pub mod named_pipes;
pub mod ncrypt;
pub mod nls;
pub mod notifyicon;
pub mod ntdll;
pub mod ntdll_file;
pub mod ole32;
pub mod oleaut32;
pub mod oleaut32_var;
pub mod opengl32;
pub mod pdh;
pub mod powrprof;
pub mod print_pdf;
pub mod propsys;
pub mod psapi_k32;
pub mod quartz;
pub mod rawinput;
pub mod resource;
pub mod rpcrt4;
pub mod rtti;
pub mod sandbox_dlls;
pub mod schannel;
pub mod sechost;
pub mod secur32;
pub mod security;
pub mod seh;
pub mod seh_hw;
pub mod sensapi;
pub mod serial;
pub mod setupapi;
pub mod sfnt;
pub mod shell32;
pub mod shlwapi;
pub mod shlwapi_reg;
pub mod spirv;
pub mod taskdialog;
pub mod taskschd;
pub mod thread_ctl;
pub mod threadpool;
pub(crate) mod uianimation;
pub mod uiautomationcore;
pub mod url_moniker;
pub mod user32;
pub mod user32_ext;
pub mod user32_more;
pub mod userenv;
pub mod usp10;
pub mod uxtheme;
pub mod v4l2;
pub mod va_bindings;
pub mod version;
pub mod vk_backend;
pub mod vk_bindings;
pub mod vulkan1;
pub mod webview2;
pub mod webview2_iids;
pub mod win32_basics;
pub mod windowscodecs;
pub mod winfile;
pub mod winhttp;
pub mod wininet;
pub mod winlux_capture;
pub mod winlux_gpu;
pub mod winlux_media;
pub mod winlux_pty;
pub mod winmm;
pub mod winmm_midi;
pub mod winmm_waveout;
pub mod winspool;
pub mod wintrust;
pub mod winusb;
pub mod wpf_native;
pub mod write_watch;
pub mod ws2_32;
pub mod xaudio2;
pub mod xdnd;
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
    (@addr fn $f:expr) => {{
        let a = $f as *const () as usize;
        // Signature relevée par build.rs, pour les programmes 32 bits (#232).
        $crate::wow32::note_export(a, module_path!(), stringify!($f));
        a
    }};
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
            e.extend(rtti::exports());
            e.extend(crt_more::exports());
            e.extend(crate::intl::api::crt_exports());
            e.extend(crt_extra::exports());
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
        ucrt.extend(rtti::exports());
        ucrt.extend(crt_more::exports());
        ucrt.extend(crate::intl::api::crt_exports());
        ucrt.extend(crt_extra::exports());
        let mut all = vec![
            Builtin {
                name: "kernel32",
                exports: {
                    let mut e = kernel32::exports();
                    e.extend(winfile::exports());
                    e.extend(kernel32_misc::exports());
                    e.extend(kernel32_fs::exports());
                    e.extend(kernel32_cpu::exports());
                    e.extend(resource::exports());
                    e.extend(mmap::exports());
                    e.extend(winlux_pty::exports());
                    e.extend(winlux_capture::exports());
                    e.extend(powrprof::kernel32_exports());
                    e.extend(gpu_caps::exports());
                    e.extend(chromium_win32::kernel32_exports());
                    e.extend(winlux_media::exports());
                    e.extend(crate::mounts::exports());
                    e.extend(crate::archive::ffi::exports());
                    e.extend(crate::imaging::api::exports());
                    e.extend(winlux_gpu::exports());
                    e.extend(seh::kernel32_exports());
                    e.extend(kernel32_ext::kernel32_exports());
                    for (name, addr) in kernel32_legacy::exports() {
                        e.entry(name).or_insert(addr);
                    }
                    // Langue, pages de code et heure : priment sur les anciennes versions.
                    e.extend(crate::intl::api::kernel32_exports());
                    // nls couvre les LCTYPE lus par la globalisation NLS de .NET
                    // (formats monétaires, signe négatif…) : il passe après intl.
                    e.extend(nls::exports());
                    e.extend(threadpool::exports());
                    e.extend(kernel32_node::kernel32_exports());
                    e.extend(fiber64::kernel32_exports());
                    e.extend(psapi_k32::kernel32_exports());
                    e.extend(console_input::exports());
                    // Ports série : remplace le QueryDosDeviceW « non géré » de kernel32_node.
                    e.extend(serial::exports());
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
                    e.extend(kernel32_fs::ntdll_exports());
                    e.extend(kernel32_node::ntdll_exports());
                    e.extend(ntdll_file::exports());
                    for (name, addr) in wpf_native::ntdll_exports() {
                        e.entry(name).or_insert(addr);
                    }
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
                    // user32_ext contient surtout des bouchons : il ne doit
                    // compléter user32 que pour les noms absents (sinon ses
                    // stubs écrasaient GetParent, SetCapture, FindWindow,
                    // SetWindowLongPtr(GWLP_WNDPROC)…). Quelques fonctions y
                    // sont meilleures que celles de user32 : elles gagnent.
                    let mut e = user32::exports();
                    for (name, addr) in user32_ext::exports() {
                        let ext_wins = matches!(name, "GetKeyboardLayoutNameA");
                        if ext_wins {
                            e.insert(name, addr);
                        } else {
                            e.entry(name).or_insert(addr);
                        }
                    }
                    e.extend(clipboard::exports());
                    e.extend(rawinput::exports());
                    e.extend(displaymode::user32_exports());
                    e.extend(ddeml::exports());
                    for (name, addr) in user32_more::exports() {
                        e.entry(name).or_insert(addr);
                    }
                    e.extend(crate::intl::api::user32_exports());
                    for (name, addr) in chromium_win32::user32_exports() {
                        e.entry(name).or_insert(addr);
                    }
                    for (name, addr) in wpf_native::user32_exports() {
                        e.entry(name).or_insert(addr);
                    }
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "shcore",
                exports: chromium_win32::shcore_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "advapi32",
                exports: {
                    let mut e = advapi32::exports();
                    e.extend(advapi32_crypto::exports());
                    e.extend(advapi32_services::exports());
                    e.extend(kernel32_ext::advapi32_exports());
                    e.extend(kernel32_fs::advapi32_exports());
                    e.extend(security::advapi32_exports());
                    e.extend(credentials::exports());
                    e.extend(crate::intl::api::advapi32_exports());
                    e.extend(eventing::exports());
                    e.extend(chromium_win32::advapi32_exports());
                    for (name, addr) in wpf_native::advapi32_exports() {
                        e.entry(name).or_insert(addr);
                    }
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
                exports: {
                    let mut e = ole32::exports();
                    e.extend(dataobject::ole32_exports());
                    for (name, addr) in wpf_native::ole32_exports() {
                        e.entry(name).or_insert(addr);
                    }
                    e
                },
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
                exports: {
                    let mut e = shell32::exports();
                    e.extend(knownfolders::shell32_exports());
                    e.extend(security::shell32_exports());
                    e.extend(dataobject::shell32_exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "mscoree",
                exports: mscoree::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "ncrypt",
                // ncrypt.dll exporte aussi les BCrypt* (FileZilla les y
                // importe pour BCryptGenRandom).
                exports: {
                    let mut e = ncrypt::exports();
                    for (name, addr) in bcrypt::exports() {
                        if name.starts_with("BCrypt") {
                            e.entry(name).or_insert(addr);
                        }
                    }
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "uiautomationcore",
                exports: uiautomationcore::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "gdiplus",
                exports: gdiplus::exports(),
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
                    e.extend(gdi32_more::exports());
                    e.extend(gdi32_path::exports());
                    e.extend(winspool::gdi_exports());
                    e.extend(opengl32::gdi32_exports());
                    e.extend(displaymode::gdi32_exports());
                    for (name, addr) in wpf_native::gdi32_exports() {
                        e.entry(name).or_insert(addr);
                    }
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
                    e.extend(winmm_midi::exports());
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
                name: "d3dcompiler_47",
                exports: d3dcompiler::exports(),
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
                exports: {
                    let mut e = oleaut32::exports();
                    e.extend(oleaut32_var::exports());
                    e
                },
                handle: fake_module(),
            },
            Builtin {
                name: "dplayx",
                exports: dplayx::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dplay",
                exports: dplayx::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dpnet",
                exports: dpnet::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dpnaddr",
                exports: dpnet::exports(),
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
                name: "dpapi",
                exports: dpapi::exports(),
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
                name: "ddraw",
                exports: ddraw::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "d3dx9",
                exports: d3dx9::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "d3d8",
                exports: d3d8::exports(),
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
                name: "hid",
                exports: hid::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "webview2loader",
                exports: webview2::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "winusb",
                exports: winusb::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "setupapi",
                exports: setupapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "fwpuclnt",
                exports: sandbox_dlls::fwpuclnt_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "netapi32",
                exports: sandbox_dlls::netapi32_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "wtsapi32",
                exports: sandbox_dlls::wtsapi32_exports(),
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
                exports: {
                    let mut e = userenv::exports();
                    e.extend(knownfolders::userenv_exports());
                    e
                },
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
            // RichEdit : les classes RICHEDIT50W, RichEdit20W/A et RICHEDIT
            // sont fournies par controls::richedit ; ces DLL n'ont qu'à se
            // charger (LoadLibrary de wxWidgets, #795).
            Builtin {
                name: "msftedit",
                exports: Exports::new(),
                handle: fake_module(),
            },
            Builtin {
                name: "riched20",
                exports: Exports::new(),
                handle: fake_module(),
            },
            Builtin {
                name: "riched32",
                exports: Exports::new(),
                handle: fake_module(),
            },
            Builtin {
                name: "sensapi",
                exports: sensapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "pdh",
                exports: pdh::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "winscard",
                exports: absent_devices::winscard_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "bthprops.cpl",
                exports: absent_devices::bthprops_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dcomp",
                exports: absent_devices::dcomp_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "fontsub",
                exports: absent_devices::fontsub_exports(),
                handle: fake_module(),
            },
            // AlphaBlend, GradientFill, TransparentBlt (complétés par common_apps).
            Builtin {
                name: "msimg32",
                exports: Exports::new(),
                handle: fake_module(),
            },
            // Accessibilité (wxWidgets, Audacity), complétée par common_apps.
            Builtin {
                name: "oleacc",
                exports: Exports::new(),
                handle: fake_module(),
            },
            Builtin {
                name: "coremessaging",
                exports: coremessaging::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dwmapi",
                exports: dwmapi::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "mpr",
                exports: mpr::exports(),
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
            Builtin {
                name: "imm32",
                exports: imm32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "secur32",
                exports: secur32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "wininet",
                exports: wininet::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "urlmon",
                exports: wininet::urlmon_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "msvcp140",
                exports: msvcp140::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "opengl32",
                exports: opengl32::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "vulkan-1",
                exports: vulkan1::exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "dnsapi",
                exports: games_win32::dnsapi_exports(),
                handle: fake_module(),
            },
            Builtin {
                name: "usp10",
                exports: usp10::usp10_exports(),
                handle: fake_module(),
            },
            // Client DHCP (WPAD de Chromium), importé par claude.exe.
            Builtin {
                name: "dhcpcsvc",
                exports: electron_win32::dhcpcsvc_exports(),
                handle: fake_module(),
            },
        ];
        add_basics(&mut all);
        add_psapi(&mut all);
        // mpr, msi et shfolder : importées directement par WinSCP et 7-Zip (#486).
        for (name, exports) in [
            ("mpr", legacy_dlls::mpr_exports()),
            ("msi", legacy_dlls::msi_exports()),
            ("shfolder", legacy_dlls::shfolder_exports()),
        ] {
            all.push(Builtin {
                name,
                exports,
                handle: fake_module(),
            });
        }
        all
    })
}

/// Noms publics de psapi.dll. Depuis Windows 7, psapi.dll les fait suivre
/// vers les fonctions « K32… » de kernel32 ; kernel32 n'exporte, lui, que
/// les noms préfixés (#521).
const PSAPI_NAMES: &[&str] = &[
    "EmptyWorkingSet",
    "EnumDeviceDrivers",
    "EnumPageFilesA",
    "EnumPageFilesW",
    "EnumProcessModules",
    "EnumProcessModulesEx",
    "EnumProcesses",
    "GetDeviceDriverBaseNameA",
    "GetDeviceDriverBaseNameW",
    "GetDeviceDriverFileNameA",
    "GetDeviceDriverFileNameW",
    "GetMappedFileNameA",
    "GetMappedFileNameW",
    "GetModuleBaseNameA",
    "GetModuleBaseNameW",
    "GetModuleFileNameExA",
    "GetModuleFileNameExW",
    "GetModuleInformation",
    "GetPerformanceInfo",
    "GetProcessImageFileNameA",
    "GetProcessImageFileNameW",
    "GetProcessMemoryInfo",
    "GetWsChanges",
    "GetWsChangesEx",
    "InitializeProcessForWsWatch",
    "QueryWorkingSet",
    "QueryWorkingSetEx",
];

/// psapi.dll : chaque nom reprend la fonction « K32… » de kernel32 (ou, à
/// défaut, la version sans préfixe qu'un module rangeait dans kernel32).
fn add_psapi(all: &mut Vec<Builtin>) {
    let Some(k32) = all.iter().find(|b| b.name == "kernel32") else {
        return;
    };
    let mut exports = Exports::new();
    for &name in PSAPI_NAMES {
        let prefixed = format!("K32{name}");
        if let Some(&a) = k32.exports.get(prefixed.as_str()).or(k32.exports.get(name)) {
            exports.insert(name, a);
        }
    }
    all.push(Builtin {
        name: "psapi",
        exports,
        handle: fake_module(),
    });
}

/// Fonctions de base (win32_basics) : elles complètent chaque DLL sans
/// jamais remplacer une fonction que la DLL fournit déjà.
fn add_basics(all: &mut [Builtin]) {
    let extra: [(&str, Exports); 9] = [
        ("kernel32", win32_basics::kernel32_exports()),
        ("user32", win32_basics::user32_exports()),
        ("gdi32", win32_basics::gdi32_exports()),
        ("shell32", win32_basics::shell32_exports()),
        ("winmm", win32_basics::winmm_exports()),
        ("ws2_32", win32_basics::ws2_32_exports()),
        ("comctl32", win32_basics::comctl32_exports()),
        ("ucrtbase", win32_basics::crt_exports()),
        ("msvcrt", win32_basics::crt_exports()),
    ];
    // Fonctions relevées sur de vrais jeux (games_win32), elles aussi en complément.
    let games: [(&str, Exports); 15] = [
        ("hid", games_win32::hid_exports()),
        ("kernel32", games_win32::kernel32_exports()),
        ("user32", games_win32::user32_exports()),
        ("gdi32", games_win32::gdi32_exports()),
        ("setupapi", games_win32::setupapi_exports()),
        ("advapi32", games_win32::advapi32_exports()),
        ("crypt32", games_win32::crypt32_exports()),
        ("ws2_32", games_win32::ws2_32_exports()),
        ("dbghelp", games_win32::dbghelp_exports()),
        ("shlwapi", games_win32::shlwapi_exports()),
        ("shell32", games_win32::shell32_exports()),
        ("iphlpapi", games_win32::iphlpapi_exports()),
        ("winhttp", games_win32::winhttp_exports()),
        ("powrprof", games_win32::powrprof_exports()),
        ("rpcrt4", games_win32::rpcrt4_exports()),
    ];
    // Fonctions relevées sur de vrais logiciels (apps_win32), en complément.
    let apps: [(&str, Exports); 11] = [
        ("winmm", apps_win32::winmm_exports()),
        ("gdi32", usp10::gdi32_exports()),
        ("kernel32", apps_win32::kernel32_exports()),
        ("kernel32", seh::exports()),
        ("user32", apps_win32::user32_exports()),
        ("comctl32", apps_win32::comctl32_exports()),
        ("shell32", apps_win32::shell32_exports()),
        ("shell32", image_list_com::shell32_exports()),
        ("advapi32", apps_win32::advapi32_exports()),
        ("msvcrt", apps_win32::crt_exports()),
        ("ucrtbase", apps_win32::crt_exports()),
    ];
    // Imports de l'appli de bureau Claude (electron_win32), en complément.
    let electron = electron_win32::all();
    // Fonctions communes à Notepad++, Audacity et Claude (common_apps).
    let common = common_apps::all();
    // Imports de FileZilla (ftp_apps).
    let ftp = ftp_apps::all();
    for (dll, exports) in extra
        .into_iter()
        .chain(games)
        .chain(apps)
        .chain(electron)
        .chain(common)
        .chain(ftp)
    {
        if let Some(b) = all.iter_mut().find(|b| b.name == dll) {
            for (name, addr) in exports {
                b.exports.entry(name).or_insert(addr);
            }
        }
    }
    // Contextes d'activation (WinForms, styles visuels).
    if let Some(b) = all.iter_mut().find(|b| b.name == "kernel32") {
        for (name, addr) in actctx::kernel32_exports() {
            b.exports.entry(name).or_insert(addr);
        }
    }
    // comctl32 et shell32 importés par numéro d'ordre.
    for &(dll, ord, name) in common_apps::ORDINALS {
        if let Some(b) = all.iter_mut().find(|b| b.name == dll) {
            if let Some(&a) = b.exports.get(name) {
                let key: &'static str = Box::leak(format!("#{ord}").into_boxed_str());
                b.exports.entry(key).or_insert(a);
            }
        }
    }
    // Winsock importé par numéro d'ordre.
    if let Some(b) = all.iter_mut().find(|b| b.name == "ws2_32") {
        for &(ord, name) in apps_win32::WS2_32_ORDINALS {
            if let Some(&a) = b.exports.get(name) {
                let key: &'static str = Box::leak(format!("#{ord}").into_boxed_str());
                b.exports.entry(key).or_insert(a);
            }
        }
    }
    // ?terminate@@YAXXZ est le nom décoré C++ de terminate().
    for crt in ["msvcrt", "ucrtbase"] {
        if let Some(b) = all.iter_mut().find(|b| b.name == crt) {
            if let Some(&t) = b.exports.get("terminate") {
                b.exports.entry("?terminate@@YAXXZ").or_insert(t);
            }
        }
    }
    // Versions réelles qui remplacent d'anciens exports vides.
    if let Some(b) = all.iter_mut().find(|b| b.name == "shell32") {
        for (name, addr) in apps_win32::shell32_overrides() {
            b.exports.insert(name, addr);
        }
    }
    if let Some(b) = all.iter_mut().find(|b| b.name == "gdi32") {
        for (name, addr) in usp10::gdi32_overrides()
            .into_iter()
            .chain(gdi32_palette::gdi32_overrides())
        {
            b.exports.insert(name, addr);
        }
        // En dernier : les tracés passent par le chemin ouvert (BeginPath).
        gdi32_path::hook(&mut b.exports);
    }
    // Fichiers .ini : de vraies lectures et écritures remplacent les
    // anciennes versions qui rendaient toujours la valeur par défaut.
    if let Some(b) = all.iter_mut().find(|b| b.name == "kernel32") {
        for (name, addr) in inifile::kernel32_overrides() {
            b.exports.insert(name, addr);
        }
    }
    if let Some(b) = all.iter_mut().find(|b| b.name == "comctl32") {
        for (name, addr) in icons::comctl32_overrides() {
            b.exports.insert(name, addr);
        }
    }
    if let Some(b) = all.iter_mut().find(|b| b.name == "advapi32") {
        for (name, addr) in apps_win32::advapi32_overrides() {
            b.exports.insert(name, addr);
        }
    }
    if let Some(b) = all.iter_mut().find(|b| b.name == "user32") {
        for (name, addr) in controls::user32_overrides()
            .into_iter()
            .chain(icons::user32_overrides())
            .chain(dialogs::user32_overrides())
            .chain(menus::user32_overrides())
            .chain(accel::user32_overrides())
        {
            b.exports.insert(name, addr);
        }
    }
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
    // Compilateur HLSL : toutes les versions servies par le même d3dcompiler_47.
    if base.starts_with("d3dcompiler_") {
        return "d3dcompiler_47".into();
    }
    // D3DX9 (#834) : d3dx9_24 … d3dx9_43 servies par la même bibliothèque.
    if base.starts_with("d3dx9_") {
        return "d3dx9".into();
    }
    // sspicli porte l'interface SSPI de secur32 depuis Windows 7.
    if base == "sspicli" {
        return "secur32".into();
    }
    // wsock32 (Winsock 1.1) fait suivre ses fonctions vers ws2_32.
    if base == "wsock32" {
        return "ws2_32".into();
    }
    // Gestionnaire de configuration Plug and Play (CM_*) : servi par setupapi.
    if base == "cfgmgr32" || base.starts_with("api-ms-win-devices-config-") {
        return "setupapi".into();
    }
    // combase.dll (COM et Windows Runtime depuis Windows 8) : notre ole32.
    if base == "combase" {
        return "ole32".into();
    }
    // DPI par écran (Chromium, Electron) : shcore.
    if base == "shcore" || base.starts_with("api-ms-win-shcore-") {
        return "shcore".into();
    }
    // Contrats « downlevel » de Windows 7 : la DLL qu'ils nomment.
    for (prefix, dll) in [
        ("api-ms-win-downlevel-shell32-", "shell32"),
        ("api-ms-win-downlevel-user32-", "user32"),
        ("api-ms-win-downlevel-advapi32-", "advapi32"),
        ("api-ms-win-downlevel-ole32-", "ole32"),
        ("api-ms-win-downlevel-shlwapi-", "shlwapi"),
        ("api-ms-win-downlevel-version-", "version"),
    ] {
        if base.starts_with(prefix) {
            return dll.into();
        }
    }
    if base.starts_with("api-ms-win-core-com-l1-") {
        return "ole32".into();
    }
    // Windows Runtime (RoInitialize...) : servi par notre ole32, avant la
    // règle générale api-ms-win-core-* → kernel32.
    if base.starts_with("api-ms-win-core-winrt-") {
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

/// Redistribuables dont Winlux a une version maison partielle : la vraie
/// DLL, quand elle est présente (dossier de l'appli ou System32), passe avant.
pub fn prefers_real_file(normalized: &str) -> bool {
    // Runtime MinGW (libstdc++, libgcc, winpthread) : la copie maison ne
    // couvre que les petits tests C++ ; une appli qui livre la sienne
    // (FileZilla, wxWidgets…) en a besoin en entier (#783).
    matches!(
        normalized,
        "msvcp140" | "libstdc++-6" | "libgcc_s_seh-1" | "libgcc_s_dw2-1" | "libwinpthread-1"
    )
}

pub fn is_builtin(normalized: &str) -> bool {
    builtins().iter().any(|b| b.name == normalized)
}

pub fn lookup(normalized: &str, func: &str) -> Option<usize> {
    let b = builtins().iter().find(|b| b.name == normalized)?;
    let r = b.exports.get(func).copied();
    // Les API « A/W » d'api-ms-win-core peuvent aussi vivre dans ntdll (Rtl*).
    r.or_else(|| {
        // ntdll exporte aussi une partie de la CRT (memcpy, _snprintf,
        // __C_specific_handler…).
        if normalized == "ntdll" {
            return lookup("msvcrt", func);
        }
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

/// Fonctions que Windows place dans KernelBase.dll (et ses API-sets) mais
/// pas dans kernel32.dll : GetProcAddress(kernel32, …) doit y échouer,
/// sinon une appli prend un chemin que Windows ne lui fait jamais prendre
/// (VLC appelait un pointeur resté nul, #495).
pub const KERNELBASE_ONLY: &[&str] = &[
    "WaitOnAddress",
    "WakeByAddressAll",
    "WakeByAddressSingle",
    // Douteux (#521) : user32 les exporte ; on les laisse visibles par
    // KernelBase et les API-sets, jamais par kernel32.
    "LoadStringA",
    "LoadStringW",
];

/// Fonctions rangées dans la table kernel32 de Winlux pour servir des
/// imports (ou par commodité), mais que Windows exporte ailleurs :
/// GetProcAddress sur kernel32 ou KernelBase les refuse, comme Windows (#521).
pub const KERNEL32_FOREIGN: &[(&str, &str)] = &[
    ("EnumProcessModules", "psapi"),
    ("EnumProcessModulesEx", "psapi"),
    ("GetModuleInformation", "psapi"),
    ("GetPerformanceInfo", "psapi"),
    ("GetProcessMemoryInfo", "psapi"),
    ("QueryWorkingSetEx", "psapi"),
    ("GetUserNameA", "advapi32"),
    ("GetUserNameW", "advapi32"),
    ("RtlAddGrowableFunctionTable", "ntdll"),
    ("RtlDeleteGrowableFunctionTable", "ntdll"),
    ("RtlGrowFunctionTable", "ntdll"),
    ("RtlAddVectoredExceptionHandler", "ntdll"),
    ("RtlRemoveVectoredExceptionHandler", "ntdll"),
    ("RtlAddVectoredContinueHandler", "ntdll"),
    ("RtlRemoveVectoredContinueHandler", "ntdll"),
];

/// Les seules fonctions de ntdll que kernel32.dll fait suivre sous leur nom
/// Rtl* (liste de référence docs/compat/kernel32-reference.txt).
pub const KERNEL32_FROM_NTDLL: &[&str] = &[
    "RtlAddFunctionTable",
    "RtlCaptureContext",
    "RtlCaptureStackBackTrace",
    "RtlCompareMemory",
    "RtlCopyMemory",
    "RtlDeleteFunctionTable",
    "RtlFillMemory",
    "RtlInstallFunctionTableCallback",
    "RtlLookupFunctionEntry",
    "RtlMoveMemory",
    "RtlPcToFileHeader",
    "RtlRaiseException",
    "RtlRestoreContext",
    "RtlSecureZeroMemory",
    "RtlUnwind",
    "RtlUnwindEx",
    "RtlVirtualUnwind",
    "RtlZeroMemory",
];

/// Fonctions du registre (contrat api-ms-win-core-registry-l1-1-0) que
/// kernel32.dll réexporte depuis Windows 7 ; les autres Reg* (RegOpenKeyA,
/// RegQueryValueA…) ne sont que dans advapi32.
pub const KERNEL32_REGISTRY: &[&str] = &[
    "RegCloseKey",
    "RegCopyTreeW",
    "RegCreateKeyExA",
    "RegCreateKeyExW",
    "RegDeleteKeyExA",
    "RegDeleteKeyExW",
    "RegDeleteTreeA",
    "RegDeleteTreeW",
    "RegDeleteValueA",
    "RegDeleteValueW",
    "RegDisablePredefinedCacheEx",
    "RegEnumKeyExA",
    "RegEnumKeyExW",
    "RegEnumValueA",
    "RegEnumValueW",
    "RegFlushKey",
    "RegGetKeySecurity",
    "RegGetValueA",
    "RegGetValueW",
    "RegLoadKeyA",
    "RegLoadKeyW",
    "RegLoadMUIStringA",
    "RegLoadMUIStringW",
    "RegNotifyChangeKeyValue",
    "RegOpenCurrentUser",
    "RegOpenKeyExA",
    "RegOpenKeyExW",
    "RegOpenUserClassesRoot",
    "RegQueryInfoKeyA",
    "RegQueryInfoKeyW",
    "RegQueryValueExA",
    "RegQueryValueExW",
    "RegRestoreKeyA",
    "RegRestoreKeyW",
    "RegSaveKeyExA",
    "RegSaveKeyExW",
    "RegSetKeySecurity",
    "RegSetValueExA",
    "RegSetValueExW",
    "RegUnLoadKeyA",
    "RegUnLoadKeyW",
];

/// Ce que GetProcAddress rend sur le HMODULE de kernel32.dll (ou de
/// KernelBase et des API-sets si `kernelbase`). Les imports statiques
/// gardent `lookup`, plus tolérant ; GetProcAddress, lui, sert aux applis
/// à choisir un chemin de code et doit coller à Windows (#495, #521).
pub fn kernel32_proc(symbol: &str, kernelbase: bool) -> Option<usize> {
    if KERNEL32_FOREIGN.iter().any(|&(n, _)| n == symbol) {
        return None;
    }
    if !kernelbase && KERNELBASE_ONLY.contains(&symbol) {
        return None;
    }
    if kernelbase {
        return lookup("kernel32", symbol);
    }
    let b = builtins().iter().find(|b| b.name == "kernel32")?;
    if let Some(&a) = b.exports.get(symbol) {
        return Some(a);
    }
    if KERNEL32_FROM_NTDLL.contains(&symbol) {
        return lookup("ntdll", symbol);
    }
    if KERNEL32_REGISTRY.contains(&symbol) {
        return lookup("advapi32", symbol);
    }
    None
}

/// Noms que GetProcAddress(kernel32, …) résout, triés : la surface figée
/// par scripts/test-kernel32-exports.py (`winlux --exports-kernel32`).
pub fn kernel32_surface() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = builtins()
        .iter()
        .find(|b| b.name == "kernel32")
        .map(|b| b.exports.keys().copied().collect())
        .unwrap_or_default();
    names.extend(KERNEL32_FROM_NTDLL.iter().copied());
    names.extend(KERNEL32_REGISTRY.iter().copied());
    names.retain(|n| !n.starts_with('#') && kernel32_proc(n, false).is_some());
    names.sort_unstable();
    names.dedup();
    names
}

/// HMODULE distinct de KernelBase.dll et des API-sets api-ms-win-core-* :
/// mêmes fonctions que kernel32, plus celles de `KERNELBASE_ONLY`.
pub fn kernelbase_handle() -> usize {
    static H: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *H.get_or_init(fake_module)
}

/// Le nom désigne-t-il KernelBase ou un API-set du noyau (et non kernel32) ?
fn is_kernelbase_name(dll: &str) -> bool {
    let l = dll.to_ascii_lowercase();
    let l = l.rsplit(['\\', '/']).next().unwrap_or(&l).to_string();
    let base = l.strip_suffix(".dll").unwrap_or(&l);
    (base == "kernelbase" || base.starts_with("api-ms-win-core-")) && normalize(dll) == "kernel32"
}

pub fn module_handle(dll: &str) -> Option<usize> {
    if is_kernelbase_name(dll) {
        return Some(kernelbase_handle());
    }
    let n = normalize(dll);
    if prefers_real_file(&n) {
        if let Some(m) = crate::loader::find_module_by_name(dll) {
            return Some(m.base);
        }
    }
    if let Some(b) = builtins().iter().find(|b| b.name == n) {
        return Some(b.handle);
    }
    crate::loader::find_module_by_name(dll).map(|m| m.base)
}

/// Toutes les DLL intégrées et leurs exports, triés : sert à `winlux
/// --exports` et à l'outil de mesure `scripts/compat-imports.py`.
pub fn builtin_exports() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut out: Vec<(&'static str, Vec<&'static str>)> = builtins()
        .iter()
        .map(|b| {
            let mut names: Vec<&'static str> = b.exports.keys().copied().collect();
            names.sort_unstable();
            (b.name, names)
        })
        .collect();
    out.sort_unstable_by_key(|(name, _)| *name);
    out
}

/// Si `h` est le HMODULE d'une DLL intégrée, renvoie son nom normalisé.
pub fn builtin_by_handle(h: usize) -> Option<&'static str> {
    if h == kernelbase_handle() {
        return Some("kernel32");
    }
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

/// Page ANSI de la locale (Windows-1252 en Europe de l'Ouest), convertie
/// vers UTF-8 pour l'interface Linux. Les API Unicode `W` évitent cette
/// conversion.
pub fn decode_ansi(bytes: &[u8]) -> String {
    let cp = crate::intl::codepage::resolve(crate::intl::codepage::CP_ACP);
    crate::intl::codepage::decode(cp, bytes, false).unwrap_or_default()
}

pub fn encode_ansi(text: &str) -> Vec<u8> {
    let cp = crate::intl::codepage::resolve(crate::intl::codepage::CP_ACP);
    crate::intl::codepage::encode(cp, text, b'?', true)
        .map(|e| e.bytes)
        .unwrap_or_default()
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

/// Copie tronquante façon `GetWindowTextA` : le tampon de `cap` caractères
/// reçoit au plus `cap - 1` caractères, est toujours terminé par un NUL, et
/// la fonction renvoie le nombre de caractères copiés (0 si `cap` est nul).
///
/// # Sécurité
/// `buf` doit pointer sur `cap` octets inscriptibles (ou être nul).
pub unsafe fn put_text_a(s: &str, buf: *mut u8, cap: u32) -> u32 {
    if buf.is_null() || cap == 0 {
        return 0;
    }
    let b = s.as_bytes();
    let n = b.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

/// Variante large de [`put_text_a`] (`GetWindowTextW`, `GetClassNameW`…).
///
/// # Sécurité
/// `buf` doit pointer sur `cap` mots de 16 bits inscriptibles (ou être nul).
pub unsafe fn put_text_w(s: &str, buf: *mut u16, cap: u32) -> u32 {
    if buf.is_null() || cap == 0 {
        return 0;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    let n = w.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as u32
}

#[cfg(test)]
mod tests {
    use super::{lookup, normalize, put_text_a, put_text_w};

    #[test]
    fn get_proc_address_kernel32_colle_a_windows() {
        use super::kernel32_proc;
        // Propres à KernelBase (#495) : visibles par KernelBase seulement.
        assert!(kernel32_proc("WaitOnAddress", false).is_none());
        assert!(kernel32_proc("WaitOnAddress", true).is_some());
        // Rangées ailleurs par Windows (#521).
        for name in [
            "GetProcessMemoryInfo",
            "GetUserNameW",
            "LoadStringW",
            "RtlAddVectoredExceptionHandler",
        ] {
            assert!(kernel32_proc(name, false).is_none(), "{name}");
        }
        assert!(lookup("psapi", "GetProcessMemoryInfo").is_some());
        assert!(lookup("psapi", "GetModuleBaseNameW").is_some());
        assert!(lookup("advapi32", "GetUserNameW").is_some());
        assert!(lookup("user32", "LoadStringW").is_some());
        // Premier plan : Flow Launcher (#1074).
        assert!(lookup("user32", "AllowSetForegroundWindow").is_some());
        assert!(lookup("user32", "LockSetForegroundWindow").is_some());
        // Pas de repli large vers ntdll, msvcrt ou advapi32.
        for name in ["NtQueryInformationProcess", "memcpy", "RegOpenKeyA"] {
            assert!(kernel32_proc(name, false).is_none(), "{name}");
        }
        // Mais les vrais exports de kernel32 restent là.
        for name in [
            "K32GetProcessMemoryInfo",
            "AddVectoredExceptionHandler",
            "RtlCaptureContext",
            "RegOpenKeyExW",
            "CreateFileW",
        ] {
            assert!(kernel32_proc(name, false).is_some(), "{name}");
        }
        // Les imports statiques gardent la table complète.
        assert!(lookup("kernel32", "GetUserNameW").is_some());
    }

    #[test]
    fn put_text_tronque_et_termine_toujours_par_nul() {
        let mut a = [0xAAu8; 4];
        assert_eq!(unsafe { put_text_a("Bonjour", a.as_mut_ptr(), 4) }, 3);
        assert_eq!(&a, b"Bon\0");
        let mut a = [0xAAu8; 16];
        assert_eq!(unsafe { put_text_a("Bonjour", a.as_mut_ptr(), 16) }, 7);
        assert_eq!(&a[..8], b"Bonjour\0");
        assert_eq!(unsafe { put_text_a("x", a.as_mut_ptr(), 0) }, 0);
        assert_eq!(unsafe { put_text_a("x", std::ptr::null_mut(), 4) }, 0);
        let mut w = [0xAAAAu16; 3];
        assert_eq!(unsafe { put_text_w("été", w.as_mut_ptr(), 3) }, 2);
        assert_eq!(w, ['é' as u16, 't' as u16, 0]);
        assert_eq!(unsafe { put_text_w("", w.as_mut_ptr(), 1) }, 0);
        assert_eq!(w[0], 0);
    }

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
            ("api-ms-win-core-winrt-l1-1-0.dll", "ole32", "RoInitialize"),
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
        .is_some());
        assert!(lookup(
            &normalize("api-ms-win-security-base-l1-1-0.dll"),
            "AccessCheckByTypeResultList"
        )
        .is_none());
        assert_eq!(
            normalize("api-ms-win-security-unknown-l1-1-0.dll"),
            "api-ms-win-security-unknown-l1-1-0"
        );
    }
}
