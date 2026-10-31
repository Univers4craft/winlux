//! Détection, à l'exécution, de ce que la machine sait faire en graphisme et
//! en vidéo, et choix du backend de rendu.
//!
//! Garde-fous : chaque capacité (Vulkan, version et extensions, OpenGL,
//! VA-API, NVDEC) est sondée par `dlopen` sans dépendance de compilation.
//! Une capacité absente donne simplement `None`/`false` : la traduction D3D
//! se replie sur OpenGL, le décodage vidéo sur le logiciel, et les
//! Paramètres masquent les réglages qui n'ont pas de sens sur la machine.
//!
//! Préférences utilisateur : `~/.config/winlux/graphics.ini` (modifiable
//! depuis Paramètres > Graphismes et vidéo), surchargées par
//! `WINLUX_GFX_BACKEND=auto|vulkan|opengl` et
//! `WINLUX_VIDEO_DECODE=auto|hardware|software`.
#![allow(dead_code)] // champs exposés au rapport et aux tests

use crate::dlls::va_bindings as va;
use crate::dlls::vk_bindings as vk;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::OnceLock;

/// Version minimale de Vulkan exigée par le backend (rendu dynamique et état
/// dynamique étendu sont dans le cœur 1.3).
pub(crate) const VULKAN_MIN_VERSION: u32 = vk::VK_API_VERSION_1_3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VulkanDevice {
    pub index: usize,
    pub name: String,
    pub api_version: u32,
    pub driver_version: u32,
    pub vendor_id: u32,
    pub device_type: u32,
    pub has_swapchain: bool,
    pub has_graphics_queue: bool,
}

impl VulkanDevice {
    pub fn is_hardware(&self) -> bool {
        self.device_type != vk::VK_PHYSICAL_DEVICE_TYPE_CPU
    }
    pub fn type_label(&self) -> &'static str {
        match self.device_type {
            vk::VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU => "dedie",
            vk::VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU => "integre",
            vk::VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU => "virtuel",
            vk::VK_PHYSICAL_DEVICE_TYPE_CPU => "logiciel",
            _ => "autre",
        }
    }
    /// Le backend Winlux peut-il tourner sur ce périphérique ?
    pub fn usable(&self, has_xlib_surface: bool) -> bool {
        self.api_version >= VULKAN_MIN_VERSION
            && self.has_swapchain
            && self.has_graphics_queue
            && has_xlib_surface
    }
    fn rank(&self) -> u32 {
        match self.device_type {
            vk::VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU => 4,
            vk::VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU => 3,
            vk::VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU => 2,
            vk::VK_PHYSICAL_DEVICE_TYPE_CPU => 1,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VulkanCaps {
    pub instance_version: u32,
    pub has_surface: bool,
    pub has_xlib_surface: bool,
    pub devices: Vec<VulkanDevice>,
}

/// Codecs vidéo que DXVA2 peut proposer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Codec {
    Mpeg2,
    H264,
    Vc1,
    Hevc,
    Hevc10,
    Vp9,
    Vp9_10,
    Av1,
}

impl Codec {
    pub const ALL: [Codec; 8] = [
        Codec::Mpeg2,
        Codec::H264,
        Codec::Vc1,
        Codec::Hevc,
        Codec::Hevc10,
        Codec::Vp9,
        Codec::Vp9_10,
        Codec::Av1,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Codec::Mpeg2 => "MPEG-2",
            Codec::H264 => "H.264",
            Codec::Vc1 => "VC-1",
            Codec::Hevc => "HEVC",
            Codec::Hevc10 => "HEVC 10 bits",
            Codec::Vp9 => "VP9",
            Codec::Vp9_10 => "VP9 10 bits",
            Codec::Av1 => "AV1",
        }
    }
    pub fn from_va_profile(p: c_int) -> Option<Codec> {
        Some(match p {
            va::VA_PROFILE_MPEG2_SIMPLE | va::VA_PROFILE_MPEG2_MAIN => Codec::Mpeg2,
            va::VA_PROFILE_H264_MAIN
            | va::VA_PROFILE_H264_HIGH
            | va::VA_PROFILE_H264_CONSTRAINED_BASELINE => Codec::H264,
            va::VA_PROFILE_VC1_SIMPLE | va::VA_PROFILE_VC1_MAIN | va::VA_PROFILE_VC1_ADVANCED => {
                Codec::Vc1
            }
            va::VA_PROFILE_HEVC_MAIN => Codec::Hevc,
            va::VA_PROFILE_HEVC_MAIN10 => Codec::Hevc10,
            va::VA_PROFILE_VP9_PROFILE0 => Codec::Vp9,
            va::VA_PROFILE_VP9_PROFILE2 => Codec::Vp9_10,
            va::VA_PROFILE_AV1_PROFILE0 => Codec::Av1,
            _ => return None,
        })
    }
    /// Codec CUVID (`cudaVideoCodec`) et profondeur, pour sonder NVDEC.
    fn cuvid(self) -> (u32, u32) {
        match self {
            Codec::Mpeg2 => (1, 0),
            Codec::Vc1 => (3, 0),
            Codec::H264 => (4, 0),
            Codec::Hevc => (8, 0),
            Codec::Hevc10 => (8, 2),
            Codec::Vp9 => (10, 0),
            Codec::Vp9_10 => (10, 2),
            Codec::Av1 => (11, 0),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct VaapiCaps {
    pub node: String,
    pub vendor: String,
    pub codecs: Vec<Codec>,
    /// Le pilote VA est `nvidia-vaapi-driver`, qui s'appuie sur NVDEC.
    pub backed_by_nvdec: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NvdecCaps {
    pub devices: Vec<String>,
    pub codecs: Vec<Codec>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GpuCaps {
    pub opengl: bool,
    pub vulkan: Option<VulkanCaps>,
    pub vaapi: Option<VaapiCaps>,
    pub nvdec: Option<NvdecCaps>,
}

// ── Préférences ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackendPref {
    Auto,
    Vulkan,
    OpenGl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoPref {
    Auto,
    Hardware,
    Software,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Prefs {
    pub backend: BackendPref,
    pub video: VideoPref,
    /// Index du périphérique Vulkan choisi, `None` = automatique.
    pub vulkan_device: Option<usize>,
    pub vsync: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            backend: BackendPref::Auto,
            video: VideoPref::Auto,
            vulkan_device: None,
            vsync: true,
        }
    }
}

impl Prefs {
    pub fn parse(text: &str) -> Prefs {
        let mut p = Prefs::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            p.apply(k.trim(), v.trim());
        }
        p
    }

    /// Applique une valeur ; `false` si la clé ou la valeur est invalide.
    pub fn apply(&mut self, key: &str, value: &str) -> bool {
        match key {
            "backend" => {
                self.backend = match value {
                    "auto" => BackendPref::Auto,
                    "vulkan" => BackendPref::Vulkan,
                    "opengl" => BackendPref::OpenGl,
                    _ => return false,
                }
            }
            "video_decode" => {
                self.video = match value {
                    "auto" => VideoPref::Auto,
                    "hardware" => VideoPref::Hardware,
                    "software" => VideoPref::Software,
                    _ => return false,
                }
            }
            "vulkan_device" => {
                self.vulkan_device = match value {
                    "auto" => None,
                    n => match n.parse::<usize>() {
                        Ok(i) if i < 16 => Some(i),
                        _ => return false,
                    },
                }
            }
            "vsync" => {
                self.vsync = match value {
                    "1" | "on" => true,
                    "0" | "off" => false,
                    _ => return false,
                }
            }
            _ => return false,
        }
        true
    }

    pub fn serialize(&self) -> String {
        format!(
            "# Réglages graphiques Winlux (Paramètres > Graphismes et vidéo)\nbackend={}\nvideo_decode={}\nvulkan_device={}\nvsync={}\n",
            match self.backend {
                BackendPref::Auto => "auto",
                BackendPref::Vulkan => "vulkan",
                BackendPref::OpenGl => "opengl",
            },
            match self.video {
                VideoPref::Auto => "auto",
                VideoPref::Hardware => "hardware",
                VideoPref::Software => "software",
            },
            self.vulkan_device
                .map(|i| i.to_string())
                .unwrap_or_else(|| "auto".into()),
            if self.vsync { 1 } else { 0 },
        )
    }
}

fn prefs_path() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("WINLUX_GRAPHICS_CONFIG") {
        return Some(p.into());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("winlux").join("graphics.ini"))
}

/// Préférences effectives : fichier puis variables d'environnement.
pub(crate) fn prefs() -> Prefs {
    let mut p = prefs_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|t| Prefs::parse(&t))
        .unwrap_or_default();
    if let Ok(v) = std::env::var("WINLUX_GFX_BACKEND") {
        p.apply("backend", &v);
    }
    if let Ok(v) = std::env::var("WINLUX_VIDEO_DECODE") {
        p.apply("video_decode", &v);
    }
    p
}

pub(crate) fn save_pref(key: &str, value: &str) -> bool {
    let Some(path) = prefs_path() else {
        return false;
    };
    let mut p = std::fs::read_to_string(&path)
        .map(|t| Prefs::parse(&t))
        .unwrap_or_default();
    if !p.apply(key, value) {
        return false;
    }
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    let tmp = path.with_extension("ini.tmp");
    std::fs::write(&tmp, p.serialize()).is_ok() && std::fs::rename(&tmp, &path).is_ok()
}

// ── Sondes ─────────────────────────────────────────────────────────────────

fn probe_opengl() -> bool {
    unsafe {
        let lib = libc::dlopen(c"libGL.so.1".as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            return false;
        }
        !libc::dlsym(lib, c"glXCreateContext".as_ptr()).is_null()
    }
}

fn probe_vulkan() -> Option<VulkanCaps> {
    let loader = vk::loader()?;
    let global = vk::global_fns()?;
    let instance_version = vk::instance_version();
    let inst_exts = vk::instance_extensions();
    let has_surface = inst_exts.iter().any(|e| e == "VK_KHR_surface");
    let has_xlib_surface = has_surface && inst_exts.iter().any(|e| e == "VK_KHR_xlib_surface");

    unsafe {
        let app = vk::VkApplicationInfo {
            sType: vk::VK_STRUCTURE_TYPE_APPLICATION_INFO,
            pNext: std::ptr::null(),
            pApplicationName: c"winlux-probe".as_ptr(),
            applicationVersion: 1,
            pEngineName: c"winlux".as_ptr(),
            engineVersion: 1,
            apiVersion: instance_version.max(vk::VK_API_VERSION_1_0),
        };
        let info = vk::VkInstanceCreateInfo {
            sType: vk::VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
            pNext: std::ptr::null(),
            flags: 0,
            pApplicationInfo: &app,
            enabledLayerCount: 0,
            ppEnabledLayerNames: std::ptr::null(),
            enabledExtensionCount: 0,
            ppEnabledExtensionNames: std::ptr::null(),
        };
        let mut instance: vk::VkInstance = std::ptr::null_mut();
        if (global.create_instance)(&info, std::ptr::null(), &mut instance) != vk::VK_SUCCESS
            || instance.is_null()
        {
            crate::log_info!(
                "vulkan",
                "aucun pilote Vulkan utilisable (création d'instance refusée)"
            );
            return None;
        }
        let gipa = loader.get_instance_proc_addr;
        let fns = vk::VkInstanceFns::load(instance, &|i, n| gipa(i, n))?;
        let mut n = 0u32;
        (fns.enumerate_physical_devices)(instance, &mut n, std::ptr::null_mut());
        let mut pds = vec![std::ptr::null_mut(); n as usize];
        (fns.enumerate_physical_devices)(instance, &mut n, pds.as_mut_ptr());
        pds.truncate(n as usize);
        let mut devices = Vec::new();
        for (index, &pd) in pds.iter().enumerate() {
            let mut props = vk::VkPhysicalDeviceProperties::zeroed();
            (fns.get_physical_device_properties)(pd, &mut props);
            let exts = vk::device_extensions(&fns, pd);
            let mut qn = 0u32;
            (fns.get_physical_device_queue_family_properties)(pd, &mut qn, std::ptr::null_mut());
            let mut qf = vec![vk::VkQueueFamilyProperties::default(); qn as usize];
            (fns.get_physical_device_queue_family_properties)(pd, &mut qn, qf.as_mut_ptr());
            devices.push(VulkanDevice {
                index,
                name: props.name(),
                api_version: props.apiVersion,
                driver_version: props.driverVersion,
                vendor_id: props.vendorID,
                device_type: props.deviceType,
                has_swapchain: exts.iter().any(|e| e == "VK_KHR_swapchain"),
                has_graphics_queue: qf
                    .iter()
                    .take(qn as usize)
                    .any(|q| q.queueFlags & vk::VK_QUEUE_GRAPHICS_BIT != 0 && q.queueCount > 0),
            });
        }
        (fns.destroy_instance)(instance, std::ptr::null());
        Some(VulkanCaps {
            instance_version,
            has_surface,
            has_xlib_surface,
            devices,
        })
    }
}

fn probe_vaapi() -> Option<VaapiCaps> {
    let session = va::VaSession::open()?;
    let mut codecs: Vec<Codec> = session
        .decode_profiles()
        .into_iter()
        .filter_map(Codec::from_va_profile)
        .collect();
    codecs.sort_by_key(|c| Codec::ALL.iter().position(|x| x == c));
    codecs.dedup();
    let lower = session.vendor.to_lowercase();
    Some(VaapiCaps {
        node: session.node.clone(),
        vendor: session.vendor.clone(),
        codecs,
        backed_by_nvdec: lower.contains("nvdec") || lower.contains("nvidia"),
    })
}

/// Sonde NVDEC (CUDA + libnvcuvid). Garde-fou : chaque symbole et chaque
/// appel est vérifié, un pilote absent ou trop ancien donne `None`.
fn probe_nvdec() -> Option<NvdecCaps> {
    if std::env::var_os("WINLUX_NO_NVDEC").is_some_and(|v| v == "1") {
        return None;
    }
    unsafe {
        let cuda = libc::dlopen(c"libcuda.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if cuda.is_null() {
            return None;
        }
        let cuvid = libc::dlopen(
            c"libnvcuvid.so.1".as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        );
        if cuvid.is_null() {
            return None;
        }
        let sym = |lib: *mut c_void, name: &CStr| {
            let p = libc::dlsym(lib, name.as_ptr());
            (!p.is_null()).then_some(p)
        };
        type CuInit = unsafe extern "C" fn(u32) -> c_int;
        type CuCount = unsafe extern "C" fn(*mut c_int) -> c_int;
        type CuGet = unsafe extern "C" fn(*mut c_int, c_int) -> c_int;
        type CuName = unsafe extern "C" fn(*mut c_char, c_int, c_int) -> c_int;
        type CuCtxCreate = unsafe extern "C" fn(*mut *mut c_void, u32, c_int) -> c_int;
        type CuCtxDestroy = unsafe extern "C" fn(*mut c_void) -> c_int;
        type CuvidCaps = unsafe extern "C" fn(*mut u8) -> c_int;
        let init: CuInit = std::mem::transmute(sym(cuda, c"cuInit")?);
        let count: CuCount = std::mem::transmute(sym(cuda, c"cuDeviceGetCount")?);
        let get: CuGet = std::mem::transmute(sym(cuda, c"cuDeviceGet")?);
        let name: CuName = std::mem::transmute(sym(cuda, c"cuDeviceGetName")?);
        let ctx_create: Option<CuCtxCreate> =
            sym(cuda, c"cuCtxCreate_v2").map(|p| std::mem::transmute(p));
        let ctx_destroy: Option<CuCtxDestroy> =
            sym(cuda, c"cuCtxDestroy_v2").map(|p| std::mem::transmute(p));
        let caps_fn: Option<CuvidCaps> =
            sym(cuvid, c"cuvidGetDecoderCaps").map(|p| std::mem::transmute(p));
        if init(0) != 0 {
            return None;
        }
        let mut n = 0;
        if count(&mut n) != 0 || n <= 0 {
            return None;
        }
        let mut devices = Vec::new();
        let mut codecs = Vec::new();
        for i in 0..n.min(8) {
            let mut dev = 0;
            if get(&mut dev, i) != 0 {
                continue;
            }
            let mut buf = [0 as c_char; 128];
            if name(buf.as_mut_ptr(), buf.len() as c_int, dev) == 0 {
                devices.push(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned());
            }
            // Les capacités par codec demandent un contexte CUDA.
            if let (Some(create), Some(destroy), Some(caps_fn)) = (ctx_create, ctx_destroy, caps_fn)
            {
                let mut ctx = std::ptr::null_mut();
                if create(&mut ctx, 0, dev) == 0 && !ctx.is_null() {
                    for codec in Codec::ALL {
                        let (cu, depth) = codec.cuvid();
                        // CUVIDDECODECAPS : 88 octets, réservé large par prudence.
                        let mut caps = [0u8; 256];
                        caps[0..4].copy_from_slice(&cu.to_le_bytes());
                        caps[4..8].copy_from_slice(&1u32.to_le_bytes()); // 4:2:0
                        caps[8..12].copy_from_slice(&depth.to_le_bytes());
                        if caps_fn(caps.as_mut_ptr()) == 0
                            && caps[24] != 0
                            && !codecs.contains(&codec)
                        {
                            codecs.push(codec);
                        }
                    }
                    destroy(ctx);
                }
            }
        }
        if devices.is_empty() {
            return None;
        }
        Some(NvdecCaps { devices, codecs })
    }
}

fn probe() -> GpuCaps {
    let caps = GpuCaps {
        opengl: probe_opengl(),
        vulkan: probe_vulkan(),
        vaapi: probe_vaapi(),
        nvdec: probe_nvdec(),
    };
    crate::log_info!(
        "gpu",
        "capacités : OpenGL={} Vulkan={} VA-API={} NVDEC={}",
        caps.opengl,
        caps.vulkan
            .as_ref()
            .map(|v| format_version(v.instance_version))
            .unwrap_or_else(|| "non".into()),
        caps.vaapi
            .as_ref()
            .map(|v| v.vendor.as_str())
            .unwrap_or("non"),
        caps.nvdec.is_some()
    );
    caps
}

/// Capacités de la machine, sondées une fois par processus.
pub(crate) fn caps() -> &'static GpuCaps {
    static C: OnceLock<GpuCaps> = OnceLock::new();
    C.get_or_init(probe)
}

pub(crate) fn format_version(v: u32) -> String {
    format!(
        "{}.{}.{}",
        vk::version_major(v),
        vk::version_minor(v),
        vk::version_patch(v)
    )
}

// ── Choix du backend ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Backend {
    Vulkan,
    OpenGl,
    /// Ni Vulkan ni OpenGL : les appareils D3D se créent mais ne dessinent rien.
    Null,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Vulkan => "vulkan",
            Backend::OpenGl => "opengl",
            Backend::Null => "aucun",
        }
    }
}

/// Périphérique Vulkan retenu pour le backend selon les préférences, ou
/// `None` si Vulkan ne convient pas (absent, < 1.3, pas de swapchain…).
/// En automatique, un Vulkan purement logiciel (llvmpipe) n'est pas retenu :
/// OpenGL fait aussi bien sur ces machines.
pub(crate) fn pick_vulkan_device(caps: &GpuCaps, prefs: &Prefs) -> Option<VulkanDevice> {
    let v = caps.vulkan.as_ref()?;
    let usable: Vec<&VulkanDevice> = v
        .devices
        .iter()
        .filter(|d| d.usable(v.has_xlib_surface))
        .collect();
    if let Some(i) = prefs.vulkan_device {
        if let Some(d) = usable.iter().find(|d| d.index == i) {
            return Some((*d).clone());
        }
    }
    let best = usable.iter().max_by_key(|d| (d.rank(), d.api_version))?;
    if prefs.backend == BackendPref::Auto && !best.is_hardware() {
        return None;
    }
    Some((*best).clone())
}

/// Backend utilisé par Direct3D 12 (le plus proche sémantiquement de Vulkan).
pub(crate) fn select_backend(caps: &GpuCaps, prefs: &Prefs) -> Backend {
    let vulkan = || pick_vulkan_device(caps, prefs).is_some();
    match prefs.backend {
        BackendPref::OpenGl if caps.opengl => Backend::OpenGl,
        BackendPref::OpenGl | BackendPref::Vulkan | BackendPref::Auto if vulkan() => {
            Backend::Vulkan
        }
        _ if caps.opengl => Backend::OpenGl,
        _ => Backend::Null,
    }
}

/// Voie de décodage matériel effectivement disponible pour un codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecodePath {
    Vaapi,
    Software,
}

pub(crate) fn decode_path(caps: &GpuCaps, prefs: &Prefs, codec: Codec) -> DecodePath {
    if prefs.video == VideoPref::Software {
        return DecodePath::Software;
    }
    match &caps.vaapi {
        Some(v) if v.codecs.contains(&codec) => DecodePath::Vaapi,
        _ => DecodePath::Software,
    }
}

/// Codecs que DXVA2 annonce aux applications : seulement ceux que la
/// machine décode vraiment en matériel. Liste vide = l'application prend son
/// décodeur logiciel, comme sous Windows sans accélération.
pub(crate) fn hardware_codecs(caps: &GpuCaps, prefs: &Prefs) -> Vec<Codec> {
    Codec::ALL
        .into_iter()
        .filter(|&c| decode_path(caps, prefs, c) == DecodePath::Vaapi)
        .filter(|&c| crate::dlls::dxva2::bridge_supports(c))
        .collect()
}

// ── Rapport pour les Paramètres ────────────────────────────────────────────

/// Rapport `clé=valeur` lu par Paramètres. Les clés absentes signifient
/// « capacité absente » : la page masque alors le réglage correspondant.
pub(crate) fn report(caps: &GpuCaps, prefs: &Prefs) -> String {
    let mut out = String::new();
    let mut kv = |k: &str, v: &str| {
        out.push_str(k);
        out.push('=');
        out.push_str(&v.replace('\n', " "));
        out.push('\n');
    };
    kv("opengl", if caps.opengl { "1" } else { "0" });
    match &caps.vulkan {
        Some(v) => {
            kv("vulkan", "1");
            kv("vulkan_version", &format_version(v.instance_version));
            kv("vulkan_devices", &v.devices.len().to_string());
            for d in &v.devices {
                kv(
                    &format!("vulkan_device{}", d.index),
                    &format!(
                        "{}|{}|{}|{}",
                        d.name,
                        d.type_label(),
                        format_version(d.api_version),
                        if d.usable(v.has_xlib_surface) {
                            "ok"
                        } else {
                            "insuffisant"
                        }
                    ),
                );
            }
            let usable = v
                .devices
                .iter()
                .filter(|d| d.usable(v.has_xlib_surface))
                .count();
            kv("vulkan_usable", &usable.to_string());
        }
        None => kv("vulkan", "0"),
    }
    let chosen = pick_vulkan_device(caps, prefs);
    if let Some(d) = &chosen {
        kv("vulkan_selected", &d.index.to_string());
    }
    kv(
        "backend_pref",
        match prefs.backend {
            BackendPref::Auto => "auto",
            BackendPref::Vulkan => "vulkan",
            BackendPref::OpenGl => "opengl",
        },
    );
    kv("backend_d3d12", select_backend(caps, prefs).label());
    kv(
        "backend_d3d11",
        if caps.opengl { "opengl" } else { "aucun" },
    );
    kv("backend_d3d9", if caps.opengl { "opengl" } else { "aucun" });
    kv("vsync", if prefs.vsync { "1" } else { "0" });
    if let Some(v) = &caps.vaapi {
        kv("vaapi", "1");
        kv("vaapi_vendor", &v.vendor);
        let codecs: Vec<&str> = v.codecs.iter().map(|c| c.label()).collect();
        kv("vaapi_codecs", &codecs.join(", "));
        if v.backed_by_nvdec {
            kv("vaapi_nvdec", "1");
        }
    } else {
        kv("vaapi", "0");
    }
    if let Some(n) = &caps.nvdec {
        kv("nvdec", "1");
        kv("nvdec_devices", &n.devices.join(", "));
        let codecs: Vec<&str> = n.codecs.iter().map(|c| c.label()).collect();
        kv("nvdec_codecs", &codecs.join(", "));
    } else {
        kv("nvdec", "0");
    }
    kv(
        "video_pref",
        match prefs.video {
            VideoPref::Auto => "auto",
            VideoPref::Hardware => "hardware",
            VideoPref::Software => "software",
        },
    );
    let hw: Vec<&str> = hardware_codecs(caps, prefs)
        .iter()
        .map(|c| c.label())
        .collect();
    kv("video_hw_codecs", &hw.join(", "));
    kv(
        "video_active",
        if hw.is_empty() { "software" } else { "vaapi" },
    );
    out
}

// ── Exports pour le shell (kernel32) ───────────────────────────────────────

/// `WinluxGraphicsInfo(buf, cap)` : copie le rapport (ASCII/Windows-1252,
/// terminé par NUL) et renvoie la taille nécessaire sans le NUL.
pub unsafe extern "win64" fn winlux_graphics_info(buf: *mut u8, cap: u32) -> u32 {
    let text = report(caps(), &prefs());
    let bytes: Vec<u8> = text
        .chars()
        .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
        .collect();
    if !buf.is_null() && cap > 0 {
        let n = bytes.len().min(cap as usize - 1);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n);
        *buf.add(n) = 0;
    }
    bytes.len() as u32
}

/// `WinluxSetGraphicsOption(key, value)` : 1 si enregistré, 0 sinon.
pub unsafe extern "win64" fn winlux_set_graphics_option(
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    if key.is_null() || value.is_null() {
        return 0;
    }
    let (Ok(k), Ok(v)) = (CStr::from_ptr(key).to_str(), CStr::from_ptr(value).to_str()) else {
        return 0;
    };
    let ok = save_pref(k, v);
    crate::log_info!(
        "gpu",
        "réglage {}={} : {}",
        k,
        v,
        if ok { "enregistré" } else { "refusé" }
    );
    ok as i32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxGraphicsInfo" => fn winlux_graphics_info,
        "WinluxSetGraphicsOption" => fn winlux_set_graphics_option,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(index: usize, ty: u32, api: u32) -> VulkanDevice {
        VulkanDevice {
            index,
            name: format!("gpu{index}"),
            api_version: api,
            driver_version: 0,
            vendor_id: 0,
            device_type: ty,
            has_swapchain: true,
            has_graphics_queue: true,
        }
    }

    fn caps_with(devices: Vec<VulkanDevice>) -> GpuCaps {
        GpuCaps {
            opengl: true,
            vulkan: Some(VulkanCaps {
                instance_version: vk::VK_API_VERSION_1_3,
                has_surface: true,
                has_xlib_surface: true,
                devices,
            }),
            vaapi: None,
            nvdec: None,
        }
    }

    #[test]
    fn sans_vulkan_repli_opengl() {
        let caps = GpuCaps {
            opengl: true,
            ..Default::default()
        };
        assert_eq!(select_backend(&caps, &Prefs::default()), Backend::OpenGl);
        let forced = Prefs {
            backend: BackendPref::Vulkan,
            ..Default::default()
        };
        assert_eq!(select_backend(&caps, &forced), Backend::OpenGl);
    }

    #[test]
    fn rien_du_tout_backend_nul() {
        assert_eq!(
            select_backend(&GpuCaps::default(), &Prefs::default()),
            Backend::Null
        );
    }

    #[test]
    fn vulkan_trop_ancien_refuse() {
        let caps = caps_with(vec![dev(
            0,
            vk::VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU,
            vk::VK_API_VERSION_1_2,
        )]);
        assert_eq!(select_backend(&caps, &Prefs::default()), Backend::OpenGl);
    }

    #[test]
    fn vulkan_sans_swapchain_refuse() {
        let mut d = dev(
            0,
            vk::VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU,
            vk::VK_API_VERSION_1_3,
        );
        d.has_swapchain = false;
        assert_eq!(
            select_backend(&caps_with(vec![d]), &Prefs::default()),
            Backend::OpenGl
        );
    }

    #[test]
    fn vulkan_logiciel_seulement_si_force() {
        let caps = caps_with(vec![dev(
            0,
            vk::VK_PHYSICAL_DEVICE_TYPE_CPU,
            vk::VK_API_VERSION_1_3,
        )]);
        assert_eq!(select_backend(&caps, &Prefs::default()), Backend::OpenGl);
        let forced = Prefs {
            backend: BackendPref::Vulkan,
            ..Default::default()
        };
        assert_eq!(select_backend(&caps, &forced), Backend::Vulkan);
    }

    #[test]
    fn gpu_dedie_prefere_et_choix_manuel() {
        let caps = caps_with(vec![
            dev(
                0,
                vk::VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU,
                vk::VK_API_VERSION_1_3,
            ),
            dev(
                1,
                vk::VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU,
                vk::VK_API_VERSION_1_3,
            ),
        ]);
        assert_eq!(
            pick_vulkan_device(&caps, &Prefs::default()).unwrap().index,
            1
        );
        let manual = Prefs {
            vulkan_device: Some(0),
            ..Default::default()
        };
        assert_eq!(pick_vulkan_device(&caps, &manual).unwrap().index, 0);
        // Un index qui n'existe plus (carte retirée) retombe sur l'automatique.
        let stale = Prefs {
            vulkan_device: Some(7),
            ..Default::default()
        };
        assert_eq!(pick_vulkan_device(&caps, &stale).unwrap().index, 1);
    }

    #[test]
    fn opengl_force_respecte() {
        let caps = caps_with(vec![dev(
            0,
            vk::VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU,
            vk::VK_API_VERSION_1_3,
        )]);
        let p = Prefs {
            backend: BackendPref::OpenGl,
            ..Default::default()
        };
        assert_eq!(select_backend(&caps, &p), Backend::OpenGl);
    }

    #[test]
    fn preferences_aller_retour() {
        let mut p = Prefs::default();
        assert!(p.apply("backend", "vulkan"));
        assert!(p.apply("video_decode", "software"));
        assert!(p.apply("vulkan_device", "1"));
        assert!(p.apply("vsync", "0"));
        assert!(!p.apply("backend", "directx"));
        assert!(!p.apply("inconnu", "1"));
        assert_eq!(Prefs::parse(&p.serialize()), p);
    }

    #[test]
    fn video_sans_vaapi_logiciel() {
        let caps = GpuCaps::default();
        assert_eq!(
            decode_path(&caps, &Prefs::default(), Codec::H264),
            DecodePath::Software
        );
        assert!(hardware_codecs(&caps, &Prefs::default()).is_empty());
        let r = report(&caps, &Prefs::default());
        assert!(r.contains("vaapi=0\n"));
        assert!(r.contains("video_active=software\n"));
        assert!(!r.contains("vaapi_codecs"));
    }

    #[test]
    fn video_vaapi_et_preference_logicielle() {
        let caps = GpuCaps {
            vaapi: Some(VaapiCaps {
                node: "/dev/dri/renderD128".into(),
                vendor: "Mesa".into(),
                codecs: vec![Codec::H264, Codec::Hevc],
                backed_by_nvdec: false,
            }),
            ..Default::default()
        };
        assert_eq!(
            decode_path(&caps, &Prefs::default(), Codec::H264),
            DecodePath::Vaapi
        );
        assert_eq!(
            decode_path(&caps, &Prefs::default(), Codec::Av1),
            DecodePath::Software
        );
        let soft = Prefs {
            video: VideoPref::Software,
            ..Default::default()
        };
        assert_eq!(decode_path(&caps, &soft, Codec::H264), DecodePath::Software);
        assert!(hardware_codecs(&caps, &Prefs::default()).contains(&Codec::H264));
    }

    #[test]
    fn rapport_masque_vulkan_absent() {
        let r = report(&GpuCaps::default(), &Prefs::default());
        assert!(r.contains("vulkan=0\n"));
        assert!(!r.contains("vulkan_device0"));
        assert!(r.contains("backend_d3d12=aucun\n"));
    }

    #[test]
    fn sonde_reelle_sans_plantage() {
        // Doit fonctionner sur toute machine, avec ou sans GPU ni pilote.
        let c = probe();
        let _ = report(&c, &Prefs::default());
    }

    #[test]
    fn export_info_tronque_proprement() {
        let mut small = [0xffu8; 8];
        let need = unsafe { winlux_graphics_info(small.as_mut_ptr(), small.len() as u32) };
        assert!(need as usize > small.len());
        assert_eq!(small[7], 0);
    }
}
