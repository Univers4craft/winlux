//! vulkan-1.dll : Vulkan pour les programmes Windows, transmis au chargeur
//! Vulkan de l'hôte (libvulkan.so.1, puis Mesa ou le pilote du GPU).
//!
//! Chaque commande traverse le pont générique de [`super::hostcall`] avec sa
//! signature tirée de vk.xml. Ce qui change entre Windows et Linux :
//! - `VK_KHR_win32_surface` est annoncée à la place de `VK_KHR_xlib_surface`
//!   et `vkCreateWin32SurfaceKHR` crée une surface Xlib sur la fenêtre X11
//!   du HWND ;
//! - `vkGetInstanceProcAddr` et `vkGetDeviceProcAddr` renvoient des ponts ;
//! - les rappels de débogage (VK_EXT_debug_utils, VK_EXT_debug_report) sont
//!   enveloppés, puisque le chargeur les appelle en convention System V ;
//! - les `VkAllocationCallbacks` du programme sont remplacés par NULL :
//!   le pilote utilise son propre allocateur.

use super::hostcall::{self, Resolver};
use super::Exports;
use std::ffi::{c_char, c_void, CStr};

type VkResult = i32;
const VK_SUCCESS: VkResult = 0;
const VK_INCOMPLETE: VkResult = 5;
const VK_ERROR_INITIALIZATION_FAILED: VkResult = -3;
const VK_ERROR_EXTENSION_NOT_PRESENT: VkResult = -7;

const STYPE_XLIB_SURFACE: u32 = 1_000_004_000;
const STYPE_DEBUG_REPORT: u32 = 1_000_011_000;
const STYPE_DEBUG_UTILS_MESSENGER: u32 = 1_000_128_004;

/// Commandes WSI que la vulkan-1.dll de Windows exporte aussi.
const WSI_EXPORTS: &[&str] = &[
    "vkDestroySurfaceKHR",
    "vkGetPhysicalDeviceSurfaceSupportKHR",
    "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
    "vkGetPhysicalDeviceSurfaceFormatsKHR",
    "vkGetPhysicalDeviceSurfacePresentModesKHR",
    "vkCreateSwapchainKHR",
    "vkDestroySwapchainKHR",
    "vkGetSwapchainImagesKHR",
    "vkAcquireNextImageKHR",
    "vkQueuePresentKHR",
    "vkGetDeviceGroupPresentCapabilitiesKHR",
    "vkGetDeviceGroupSurfacePresentModesKHR",
    "vkGetPhysicalDevicePresentRectanglesKHR",
    "vkAcquireNextImage2KHR",
    "vkGetPhysicalDeviceDisplayPropertiesKHR",
    "vkGetPhysicalDeviceDisplayPlanePropertiesKHR",
];

/// Nom d'extension d'instance vu par le programme → nom réel chez l'hôte.
/// Les extensions de surface propres à Linux sont masquées.
pub fn host_extension(name: &str) -> &str {
    match name {
        "VK_KHR_win32_surface" => "VK_KHR_xlib_surface",
        other => other,
    }
}

/// Nom d'extension de l'hôte → nom montré au programme (None : masquée).
pub fn guest_extension(name: &str) -> Option<&str> {
    match name {
        "VK_KHR_xlib_surface" => Some("VK_KHR_win32_surface"),
        "VK_KHR_xcb_surface" | "VK_KHR_wayland_surface" | "VK_EXT_acquire_xlib_display" => None,
        other => Some(other),
    }
}

fn gipa() -> Option<unsafe extern "C" fn(usize, *const c_char) -> usize> {
    let p = hostcall::vk_sym(b"vkGetInstanceProcAddr\0");
    (p != 0).then(|| unsafe {
        std::mem::transmute::<usize, unsafe extern "C" fn(usize, *const c_char) -> usize>(p)
    })
}

fn instance_proc(instance: usize, name: &CStr) -> usize {
    gipa()
        .map(|f| unsafe { f(instance, name.as_ptr()) })
        .unwrap_or(0)
}

/// Commandes réécrites par Winlux.
fn special(name: &str) -> Option<usize> {
    let f: usize = match name {
        "vkGetInstanceProcAddr" => vk_get_instance_proc_addr as *const () as usize,
        "vkGetDeviceProcAddr" => vk_get_device_proc_addr as *const () as usize,
        "vkCreateInstance" => vk_create_instance as *const () as usize,
        "vkEnumerateInstanceExtensionProperties" => {
            vk_enumerate_instance_extensions as *const () as usize
        }
        "vkCreateWin32SurfaceKHR" => vk_create_win32_surface as *const () as usize,
        "vkGetPhysicalDeviceWin32PresentationSupportKHR" => {
            vk_win32_presentation_support as *const () as usize
        }
        "vkCreateDebugUtilsMessengerEXT" => vk_create_debug_utils_messenger as *const () as usize,
        "vkCreateDebugReportCallbackEXT" => vk_create_debug_report_callback as *const () as usize,
        _ => return None,
    };
    Some(f)
}

fn bridge(name: &str, host: usize) -> usize {
    if host == 0 {
        return 0;
    }
    match hostcall::vk_signatures().get(name) {
        Some(kinds) => hostcall::bridge_for(name, kinds, host),
        None => {
            crate::log_warn!("vulkan-1", "{name} : signature inconnue, non transmise");
            0
        }
    }
}

extern "win64" fn vk_get_instance_proc_addr(instance: usize, name: *const c_char) -> usize {
    if name.is_null() {
        return 0;
    }
    let cname = unsafe { CStr::from_ptr(name) };
    let n = cname.to_string_lossy();
    if let Some(f) = special(&n) {
        // Les commandes d'extension ne sont servies que si l'hôte les a.
        let host_name = match &*n {
            "vkCreateWin32SurfaceKHR" => Some(c"vkCreateXlibSurfaceKHR"),
            "vkGetPhysicalDeviceWin32PresentationSupportKHR" => {
                Some(c"vkGetPhysicalDeviceXlibPresentationSupportKHR")
            }
            "vkCreateDebugUtilsMessengerEXT" | "vkCreateDebugReportCallbackEXT" => Some(cname),
            _ => None,
        };
        if let Some(h) = host_name {
            if instance != 0 && instance_proc(instance, h) == 0 {
                return 0;
            }
        }
        return f;
    }
    bridge(&n, instance_proc(instance, cname))
}

extern "win64" fn vk_get_device_proc_addr(device: usize, name: *const c_char) -> usize {
    if name.is_null() {
        return 0;
    }
    let cname = unsafe { CStr::from_ptr(name) };
    let n = cname.to_string_lossy();
    if n == "vkGetDeviceProcAddr" {
        return vk_get_device_proc_addr as *const () as usize;
    }
    let gdpa = hostcall::vk_sym(b"vkGetDeviceProcAddr\0");
    if gdpa == 0 {
        return 0;
    }
    let f: unsafe extern "C" fn(usize, *const c_char) -> usize =
        unsafe { std::mem::transmute(gdpa) };
    bridge(&n, unsafe { f(device, name) })
}

/// Liste d'extensions de l'hôte, traduite pour le programme.
fn guest_extensions(layer: *const c_char) -> Result<Vec<[u8; 260]>, VkResult> {
    let p = instance_proc(0, c"vkEnumerateInstanceExtensionProperties");
    if p == 0 {
        return Err(VK_ERROR_INITIALIZATION_FAILED);
    }
    let f: unsafe extern "C" fn(*const c_char, *mut u32, *mut [u8; 260]) -> VkResult =
        unsafe { std::mem::transmute(p) };
    let mut n = 0u32;
    let r = unsafe { f(layer, &mut n, std::ptr::null_mut()) };
    if r != VK_SUCCESS {
        return Err(r);
    }
    let mut list = vec![[0u8; 260]; n as usize];
    let r = unsafe { f(layer, &mut n, list.as_mut_ptr()) };
    if r < 0 {
        return Err(r);
    }
    list.truncate(n as usize);
    let mut out = Vec::new();
    for e in list {
        let end = e[..256].iter().position(|&b| b == 0).unwrap_or(256);
        let name = String::from_utf8_lossy(&e[..end]).into_owned();
        if let Some(g) = guest_extension(&name) {
            let mut x = e;
            x[..256].fill(0);
            x[..g.len()].copy_from_slice(g.as_bytes());
            out.push(x);
        }
    }
    Ok(out)
}

extern "win64" fn vk_enumerate_instance_extensions(
    layer: *const c_char,
    count: *mut u32,
    props: *mut [u8; 260],
) -> VkResult {
    if count.is_null() {
        return VK_ERROR_INITIALIZATION_FAILED;
    }
    let list = match guest_extensions(layer) {
        Ok(l) => l,
        Err(e) => return e,
    };
    unsafe {
        if props.is_null() {
            *count = list.len() as u32;
            return VK_SUCCESS;
        }
        let n = (*count as usize).min(list.len());
        std::ptr::copy_nonoverlapping(list.as_ptr(), props, n);
        *count = n as u32;
        if n < list.len() {
            VK_INCOMPLETE
        } else {
            VK_SUCCESS
        }
    }
}

// ---------- Rappels de débogage ----------

#[repr(C)]
struct Callback {
    func: usize,
    user: usize,
}

extern "C" fn utils_tramp(sev: u32, types: u32, data: *const c_void, user: *const Callback) -> u32 {
    let c = unsafe { &*user };
    let f: extern "win64" fn(u32, u32, *const c_void, usize) -> u32 =
        unsafe { std::mem::transmute(c.func) };
    f(sev, types, data, c.user)
}

#[allow(clippy::too_many_arguments)]
extern "C" fn report_tramp(
    flags: u32,
    obj_type: u32,
    obj: u64,
    location: usize,
    code: i32,
    prefix: *const c_char,
    msg: *const c_char,
    user: *const Callback,
) -> u32 {
    let c = unsafe { &*user };
    let f: extern "win64" fn(
        u32,
        u32,
        u64,
        usize,
        i32,
        *const c_char,
        *const c_char,
        usize,
    ) -> u32 = unsafe { std::mem::transmute(c.func) };
    f(flags, obj_type, obj, location, code, prefix, msg, c.user)
}

/// Remplace sur place le rappel d'une structure de débogage (VkDebugUtils
/// MessengerCreateInfoEXT : rappel en +32 ; VkDebugReportCallback
/// CreateInfoEXT : en +24) ; renvoie de quoi la remettre en état.
unsafe fn wrap_callback(node: *mut u8) -> Option<(*mut usize, usize, usize)> {
    let stype = *(node as *const u32);
    let (off, tramp) = match stype {
        STYPE_DEBUG_UTILS_MESSENGER => (32, utils_tramp as *const () as usize),
        STYPE_DEBUG_REPORT => (24, report_tramp as *const () as usize),
        _ => return None,
    };
    let fp = node.add(off) as *mut usize;
    let up = node.add(off + 8) as *mut usize;
    let (func, user) = (*fp, *up);
    if func == 0 {
        return None;
    }
    let cb = Box::leak(Box::new(Callback { func, user })) as *const Callback as usize;
    *fp = tramp;
    *up = cb;
    Some((fp, func, user))
}

unsafe fn restore(saved: Vec<(*mut usize, usize, usize)>) {
    for (fp, func, user) in saved {
        *fp = func;
        *fp.add(1) = user;
    }
}

/// Rappels chaînés dans `pNext` (messages pendant vkCreateInstance).
unsafe fn wrap_chain(mut next: *mut u8) -> Vec<(*mut usize, usize, usize)> {
    let mut saved = Vec::new();
    let mut guard = 0;
    while !next.is_null() && guard < 64 {
        if let Some(s) = wrap_callback(next) {
            saved.push(s);
        }
        next = *(next.add(8) as *const *mut u8);
        guard += 1;
    }
    saved
}

extern "win64" fn vk_create_instance(info: *const u8, _alloc: usize, out: *mut usize) -> VkResult {
    if info.is_null() || out.is_null() {
        return VK_ERROR_INITIALIZATION_FAILED;
    }
    let p = instance_proc(0, c"vkCreateInstance");
    if p == 0 {
        return VK_ERROR_INITIALIZATION_FAILED;
    }
    let f: unsafe extern "C" fn(*const u8, usize, *mut usize) -> VkResult =
        unsafe { std::mem::transmute(p) };
    unsafe {
        // VkInstanceCreateInfo : enabledExtensionCount en +48, noms en +56.
        let mut copy = [0u8; 64];
        std::ptr::copy_nonoverlapping(info, copy.as_mut_ptr(), 64);
        let n = *(info.add(48) as *const u32) as usize;
        let names = *(info.add(56) as *const *const *const c_char);
        let mut owned: Vec<std::ffi::CString> = Vec::with_capacity(n);
        for i in 0..n {
            let name = CStr::from_ptr(*names.add(i)).to_string_lossy();
            let host = host_extension(&name);
            owned.push(std::ffi::CString::new(host).unwrap_or_default());
        }
        let ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        copy[56..64].copy_from_slice(&(ptrs.as_ptr() as usize).to_ne_bytes());
        let saved = wrap_chain(*(info.add(8) as *const *mut u8));
        let r = f(copy.as_ptr(), 0, out);
        restore(saved);
        r
    }
}

extern "win64" fn vk_create_win32_surface(
    instance: usize,
    info: *const u8,
    _alloc: usize,
    out: *mut u64,
) -> VkResult {
    if info.is_null() || out.is_null() {
        return VK_ERROR_INITIALIZATION_FAILED;
    }
    let hwnd = unsafe { *(info.add(32) as *const usize) };
    let dpy = hostcall::x_display();
    let Some(window) = super::gui::native_window(hwnd) else {
        crate::log_warn!(
            "vulkan-1",
            "vkCreateWin32SurfaceKHR : HWND {hwnd:#x} sans fenêtre X11"
        );
        return VK_ERROR_INITIALIZATION_FAILED;
    };
    let p = instance_proc(instance, c"vkCreateXlibSurfaceKHR");
    if p == 0 || dpy == 0 {
        return VK_ERROR_EXTENSION_NOT_PRESENT;
    }
    // VkXlibSurfaceCreateInfoKHR : sType, pNext, flags, Display*, Window.
    #[repr(C)]
    struct Xlib {
        stype: u32,
        next: usize,
        flags: u32,
        dpy: usize,
        window: u64,
    }
    let x = Xlib {
        stype: STYPE_XLIB_SURFACE,
        next: 0,
        flags: 0,
        dpy,
        window,
    };
    let f: unsafe extern "C" fn(usize, *const Xlib, usize, *mut u64) -> VkResult =
        unsafe { std::mem::transmute(p) };
    unsafe { f(instance, &x, 0, out) }
}

extern "win64" fn vk_win32_presentation_support(physical: usize, family: u32) -> u32 {
    let p = hostcall::vk_sym(b"vkGetPhysicalDeviceXlibPresentationSupportKHR\0");
    let dpy = hostcall::x_display();
    let x = hostcall::libx11();
    let (dv, vid, ds) = (
        hostcall::sym(x, b"XDefaultVisual\0"),
        hostcall::sym(x, b"XVisualIDFromVisual\0"),
        hostcall::sym(x, b"XDefaultScreen\0"),
    );
    if p == 0 || dpy == 0 || dv == 0 || vid == 0 || ds == 0 {
        return 0;
    }
    unsafe {
        let ds: unsafe extern "C" fn(usize) -> i32 = std::mem::transmute(ds);
        let dv: unsafe extern "C" fn(usize, i32) -> usize = std::mem::transmute(dv);
        let vid: unsafe extern "C" fn(usize) -> u64 = std::mem::transmute(vid);
        let f: unsafe extern "C" fn(usize, u32, usize, u64) -> u32 = std::mem::transmute(p);
        f(physical, family, dpy, vid(dv(dpy, ds(dpy))))
    }
}

unsafe fn create_debug(
    instance: usize,
    name: &CStr,
    info: *const u8,
    size: usize,
    out: *mut u64,
) -> VkResult {
    let p = instance_proc(instance, name);
    if p == 0 || info.is_null() {
        return VK_ERROR_EXTENSION_NOT_PRESENT;
    }
    let mut copy = vec![0u8; size];
    std::ptr::copy_nonoverlapping(info, copy.as_mut_ptr(), size);
    wrap_callback(copy.as_mut_ptr());
    let f: unsafe extern "C" fn(usize, *const u8, usize, *mut u64) -> VkResult =
        std::mem::transmute(p);
    f(instance, copy.as_ptr(), 0, out)
}

extern "win64" fn vk_create_debug_utils_messenger(
    instance: usize,
    info: *const u8,
    _alloc: usize,
    out: *mut u64,
) -> VkResult {
    unsafe { create_debug(instance, c"vkCreateDebugUtilsMessengerEXT", info, 48, out) }
}

extern "win64" fn vk_create_debug_report_callback(
    instance: usize,
    info: *const u8,
    _alloc: usize,
    out: *mut u64,
) -> VkResult {
    unsafe { create_debug(instance, c"vkCreateDebugReportCallbackEXT", info, 40, out) }
}

pub fn exports() -> Exports {
    let mut e = Exports::new();
    let sigs = hostcall::vk_signatures();
    for (name, kinds) in sigs.core() {
        e.insert(name, hostcall::lazy_bridge(name, kinds, Resolver::Vulkan));
    }
    for &name in WSI_EXPORTS {
        if let Some(kinds) = sigs.get(name) {
            e.insert(name, hostcall::lazy_bridge(name, kinds, Resolver::Vulkan));
        }
    }
    for name in [
        "vkGetInstanceProcAddr",
        "vkGetDeviceProcAddr",
        "vkCreateInstance",
        "vkEnumerateInstanceExtensionProperties",
        "vkCreateWin32SurfaceKHR",
        "vkGetPhysicalDeviceWin32PresentationSupportKHR",
    ] {
        if let Some(f) = special(name) {
            e.insert(name, f);
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_traduites() {
        assert_eq!(
            host_extension("VK_KHR_win32_surface"),
            "VK_KHR_xlib_surface"
        );
        assert_eq!(host_extension("VK_KHR_surface"), "VK_KHR_surface");
        assert_eq!(
            guest_extension("VK_KHR_xlib_surface"),
            Some("VK_KHR_win32_surface")
        );
        assert_eq!(guest_extension("VK_KHR_wayland_surface"), None);
    }

    #[test]
    fn exports_de_vulkan_1() {
        let e = exports();
        for n in [
            "vkCreateInstance",
            "vkCreateDevice",
            "vkQueueSubmit",
            "vkCmdDraw",
            "vkCreateWin32SurfaceKHR",
            "vkQueuePresentKHR",
            "vkEnumerateInstanceVersion",
        ] {
            assert!(e.contains_key(n), "{n}");
        }
    }

    #[test]
    fn rappel_enveloppe_puis_restaure() {
        let mut node = [0u8; 48];
        node[..4].copy_from_slice(&STYPE_DEBUG_UTILS_MESSENGER.to_ne_bytes());
        node[32..40].copy_from_slice(&0x1234usize.to_ne_bytes());
        node[40..48].copy_from_slice(&0x99usize.to_ne_bytes());
        let saved = unsafe { wrap_chain(node.as_mut_ptr()) };
        assert_eq!(saved.len(), 1);
        assert_eq!(
            usize::from_ne_bytes(node[32..40].try_into().unwrap()),
            utils_tramp as *const () as usize
        );
        unsafe { restore(saved) };
        assert_eq!(
            usize::from_ne_bytes(node[32..40].try_into().unwrap()),
            0x1234
        );
        assert_eq!(usize::from_ne_bytes(node[40..48].try_into().unwrap()), 0x99);
    }
}
