//! Suite auto-vérifiante OpenGL (opengl32.dll + WGL) et Vulkan (vulkan-1.dll).
//!
//! Tout passe par LoadLibrary/GetProcAddress, comme le font SDL, GLFW ou
//! volk. Sans affichage (CI) ou sans pilote Vulkan, la partie rendu est
//! sautée proprement ; tout le reste est vérifié partout.
//! Code de sortie = nombre d'échecs.
#![no_std]
#![no_main]

use core::ffi::{c_char, c_void};
use core::ptr::{null, null_mut};
use winlux_testexe::*;

unsafe fn proc(module: Handle, name: &core::ffi::CStr) -> *const c_void {
    if module.is_null() {
        return null();
    }
    GetProcAddress(module, name.as_ptr())
}

macro_rules! f {
    ($m:expr, $name:literal, $ty:ty) => {
        f!($m, $name, $ty, "fonction")
    };
    ($m:expr, $name:literal, $ty:ty, $label:literal) => {{
        let p = proc($m, $name);
        check!(!p.is_null(), concat!($label, " exportée"));
        if p.is_null() {
            None
        } else {
            Some(core::mem::transmute::<*const c_void, $ty>(p))
        }
    }};
}

unsafe extern "system" fn wndproc(h: Handle, m: u32, w: usize, l: isize) -> isize {
    DefWindowProcA(h, m, w, l)
}

const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_VERSION: u32 = 0x1F02;
const GL_MAJOR_VERSION: u32 = 0x821B;
const GL_DEPTH_CLEAR_VALUE: u32 = 0x0B73;

type WglCreate = unsafe extern "system" fn(Handle) -> usize;
type WglMake = unsafe extern "system" fn(Handle, usize) -> i32;
type WglDelete = unsafe extern "system" fn(usize) -> i32;
type WglProc = unsafe extern "system" fn(*const c_char) -> *const c_void;
type CreateAttribs = unsafe extern "system" fn(Handle, usize, *const i32) -> usize;

unsafe fn gl_suite() -> Handle {
    out(b"\n-- OpenGL --\n");
    let gl = LoadLibraryA(c"opengl32.dll".as_ptr());
    check!(!gl.is_null(), "opengl32.dll chargée");
    let gdi = LoadLibraryA(c"gdi32.dll".as_ptr());
    let choose = f!(gdi, c"ChoosePixelFormat", unsafe extern "system" fn(Handle, *const u8) -> i32, "ChoosePixelFormat");
    let describe = f!(gdi, c"DescribePixelFormat", unsafe extern "system" fn(Handle, i32, u32, *mut u8) -> i32, "DescribePixelFormat");
    let set = f!(gdi, c"SetPixelFormat", unsafe extern "system" fn(Handle, i32, *const u8) -> i32, "SetPixelFormat");
    let swap = f!(gdi, c"SwapBuffers", unsafe extern "system" fn(Handle) -> i32, "SwapBuffers");
    let create = f!(gl, c"wglCreateContext", WglCreate, "wglCreateContext");
    let make = f!(gl, c"wglMakeCurrent", WglMake, "wglMakeCurrent");
    let delete = f!(gl, c"wglDeleteContext", WglDelete, "wglDeleteContext");
    let get_proc = f!(gl, c"wglGetProcAddress", WglProc, "wglGetProcAddress");
    let clear_color = f!(gl, c"glClearColor", unsafe extern "system" fn(f32, f32, f32, f32), "glClearColor");
    let clear = f!(gl, c"glClear", unsafe extern "system" fn(u32), "glClear");
    let finish = f!(gl, c"glFinish", unsafe extern "system" fn(), "glFinish");
    let read = f!(gl, c"glReadPixels", unsafe extern "system" fn(i32, i32, i32, i32, u32, u32, *mut u8), "glReadPixels");
    let get_string = f!(gl, c"glGetString", unsafe extern "system" fn(u32) -> *const u8, "glGetString");
    let get_integer = f!(gl, c"glGetIntegerv", unsafe extern "system" fn(u32, *mut i32), "glGetIntegerv");
    let clear_depth = f!(gl, c"glClearDepth", unsafe extern "system" fn(f64), "glClearDepth");
    let get_double = f!(gl, c"glGetDoublev", unsafe extern "system" fn(u32, *mut f64), "glGetDoublev");
    let (
        Some(choose),
        Some(describe),
        Some(set),
        Some(swap),
        Some(create),
        Some(make),
        Some(delete),
        Some(get_proc),
    ) = (choose, describe, set, swap, create, make, delete, get_proc)
    else {
        return null_mut();
    };

    let class = c"WinluxGL";
    let wc = WNDCLASSA {
        style: 0x20, // CS_OWNDC
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: null_mut(),
        hIcon: null_mut(),
        hCursor: null_mut(),
        hbrBackground: 0,
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    RegisterClassA(&wc);
    let hwnd = CreateWindowExA(
        0,
        class.as_ptr(),
        c"Winlux OpenGL".as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        50,
        50,
        64,
        64,
        null_mut(),
        null_mut(),
        null_mut(),
        null_mut(),
    );
    check!(!hwnd.is_null(), "fenêtre créée");
    let hdc = GetDC(hwnd);

    // PIXELFORMATDESCRIPTOR : double tampon, RGBA 32 bits, profondeur 24.
    let mut pfd = [0u8; 40];
    pfd[0] = 40;
    pfd[2] = 1;
    pfd[4] = 0x25; // PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER
    pfd[9] = 32;
    pfd[23] = 24;
    pfd[24] = 8;
    let format = choose(hdc, pfd.as_ptr());
    check!(format >= 1, "ChoosePixelFormat");
    let mut desc = [0u8; 40];
    let count = describe(hdc, format, 40, desc.as_mut_ptr());
    check!(count >= 1 && desc[9] == 32 && desc[23] == 24, "DescribePixelFormat");
    check!(set(hdc, format, pfd.as_ptr()) == 1, "SetPixelFormat");
    check!(get_proc(c"glPasDeTelleFonction".as_ptr()).is_null(), "wglGetProcAddress inconnue → NULL");

    let ctx = create(hdc);
    if ctx == 0 {
        out(b"  --     pas d'affichage OpenGL : rendu saute\n");
        return hwnd;
    }
    check!(make(hdc, ctx) == 1, "wglMakeCurrent");
    if let Some(gs) = get_string {
        let v = gs(GL_VERSION);
        check!(!v.is_null(), "glGetString(GL_VERSION)");
        if !v.is_null() {
            out(b"         ");
            out(core::ffi::CStr::from_ptr(v as *const c_char).to_bytes());
            out(b"\n");
        }
    }
    if let (Some(cc), Some(c), Some(fi), Some(rp)) = (clear_color, clear, finish, read) {
        // Quatre flottants : traverse le pont xmm0–xmm3 → xmm0–xmm3.
        cc(0.25, 0.5, 0.75, 1.0);
        c(GL_COLOR_BUFFER_BIT);
        fi();
        let mut px = [0u8; 4];
        rp(0, 0, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, px.as_mut_ptr());
        let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
        check!(
            near(px[0], 64) && near(px[1], 128) && near(px[2], 191),
            "glClearColor + glReadPixels"
        );
    }
    if let (Some(cd), Some(gd)) = (clear_depth, get_double) {
        cd(0.5);
        let mut v = 0.0f64;
        gd(GL_DEPTH_CLEAR_VALUE, &mut v);
        check!(v == 0.5, "glClearDepth(double)");
    }
    check!(swap(hdc) == 1, "SwapBuffers");
    check!(!get_proc(c"glGenBuffers".as_ptr()).is_null(), "wglGetProcAddress(glGenBuffers)");
    let attribs_fn = get_proc(c"wglCreateContextAttribsARB".as_ptr());
    check!(!attribs_fn.is_null(), "wglCreateContextAttribsARB");
    if !attribs_fn.is_null() {
        let ca: CreateAttribs = core::mem::transmute(attribs_fn);
        // OpenGL 3.3, profil cœur.
        let attrs = [0x2091, 3, 0x2092, 3, 0x9126, 1, 0];
        let core_ctx = ca(hdc, 0, attrs.as_ptr());
        check!(core_ctx != 0, "contexte OpenGL 3.3 cœur");
        if core_ctx != 0 && make(hdc, core_ctx) == 1 {
            if let Some(gi) = get_integer {
                let mut major = 0;
                gi(GL_MAJOR_VERSION, &mut major);
                check!(major >= 3, "GL_MAJOR_VERSION >= 3");
            }
            make(null_mut(), 0);
            delete(core_ctx);
        }
    }
    make(null_mut(), 0);
    check!(delete(ctx) == 1, "wglDeleteContext");
    hwnd
}

// ---------- Vulkan ----------

const VK_SUCCESS: i32 = 0;
const VK_ERROR_INCOMPATIBLE_DRIVER: i32 = -9;

type Gipa = unsafe extern "system" fn(usize, *const c_char) -> *const c_void;

unsafe fn vk<T>(gipa: Gipa, inst: usize, name: &core::ffi::CStr) -> Option<T> {
    let p = gipa(inst, name.as_ptr());
    if p.is_null() {
        None
    } else {
        Some(core::mem::transmute_copy::<*const c_void, T>(&p))
    }
}

#[repr(C)]
struct AppInfo {
    s_type: u32,
    next: *const c_void,
    app: *const c_char,
    app_version: u32,
    engine: *const c_char,
    engine_version: u32,
    api: u32,
}

#[repr(C)]
struct InstanceInfo {
    s_type: u32,
    next: *const c_void,
    flags: u32,
    app: *const AppInfo,
    layers: u32,
    layer_names: *const *const c_char,
    exts: u32,
    ext_names: *const *const c_char,
}

#[repr(C)]
struct QueueInfo {
    s_type: u32,
    next: *const c_void,
    flags: u32,
    family: u32,
    count: u32,
    priorities: *const f32,
}

#[repr(C)]
struct DeviceInfo {
    s_type: u32,
    next: *const c_void,
    flags: u32,
    queue_count: u32,
    queues: *const QueueInfo,
    layers: u32,
    layer_names: *const *const c_char,
    exts: u32,
    ext_names: *const *const c_char,
    features: *const c_void,
}

#[repr(C)]
struct BufferInfo {
    s_type: u32,
    next: *const c_void,
    flags: u32,
    size: u64,
    usage: u32,
    sharing: u32,
    families: u32,
    family_list: *const u32,
}

#[repr(C)]
struct MemReq {
    size: u64,
    align: u64,
    bits: u32,
}

#[repr(C)]
struct AllocInfo {
    s_type: u32,
    next: *const c_void,
    size: u64,
    index: u32,
}

fn has_ext(list: &[[u8; 260]], n: usize, name: &[u8]) -> bool {
    list[..n].iter().any(|e| e.starts_with(name) && e[name.len()] == 0)
}

unsafe fn vk_suite(hwnd: Handle) {
    out(b"\n-- Vulkan --\n");
    let lib = LoadLibraryA(c"vulkan-1.dll".as_ptr());
    check!(!lib.is_null(), "vulkan-1.dll chargée");
    let Some(gipa) = f!(lib, c"vkGetInstanceProcAddr", Gipa, "vkGetInstanceProcAddr") else { return };
    check!(!proc(lib, c"vkCreateWin32SurfaceKHR").is_null(), "vkCreateWin32SurfaceKHR exportée");
    type Enum = unsafe extern "system" fn(*const c_char, *mut u32, *mut [u8; 260]) -> i32;
    let Some(enum_ext) = vk::<Enum>(gipa, 0, c"vkEnumerateInstanceExtensionProperties") else {
        check!(false, "vkEnumerateInstanceExtensionProperties");
        return;
    };
    let mut exts = [[0u8; 260]; 64];
    let mut n = 64u32;
    let r = enum_ext(null(), &mut n, exts.as_mut_ptr());
    if r == -3 {
        out(b"  --     chargeur Vulkan absent : suite Vulkan sautee\n");
        return;
    }
    check!(r == VK_SUCCESS || r == 5, "vkEnumerateInstanceExtensionProperties");
    let n = n as usize;
    check!(!has_ext(&exts, n, b"VK_KHR_xlib_surface"), "extension xlib masquée");
    let win32 = has_ext(&exts, n, b"VK_KHR_win32_surface");
    if has_ext(&exts, n, b"VK_KHR_surface") {
        check!(win32, "VK_KHR_win32_surface annoncée");
    }

    let names = [c"VK_KHR_surface".as_ptr(), c"VK_KHR_win32_surface".as_ptr()];
    let app = AppInfo {
        s_type: 0,
        next: null(),
        app: c"gl_vk_suite".as_ptr(),
        app_version: 1,
        engine: null(),
        engine_version: 0,
        api: 1 << 22,
    };
    let info = InstanceInfo {
        s_type: 1,
        next: null(),
        flags: 0,
        app: &app,
        layers: 0,
        layer_names: null(),
        exts: if win32 { 2 } else { 0 },
        ext_names: names.as_ptr(),
    };
    type CreateInstance = unsafe extern "system" fn(*const InstanceInfo, *const c_void, *mut usize) -> i32;
    let Some(create_instance) = vk::<CreateInstance>(gipa, 0, c"vkCreateInstance") else { return };
    let mut inst = 0usize;
    let r = create_instance(&info, null(), &mut inst);
    if r == VK_ERROR_INCOMPATIBLE_DRIVER {
        out(b"  --     aucun pilote Vulkan : suite Vulkan sautee\n");
        return;
    }
    check!(r == VK_SUCCESS && inst != 0, "vkCreateInstance");
    if inst == 0 {
        return;
    }
    type EnumDev = unsafe extern "system" fn(usize, *mut u32, *mut usize) -> i32;
    let enum_dev: EnumDev = vk(gipa, inst, c"vkEnumeratePhysicalDevices").unwrap();
    let mut devs = [0usize; 8];
    let mut nd = 8u32;
    enum_dev(inst, &mut nd, devs.as_mut_ptr());
    check!(nd >= 1, "vkEnumeratePhysicalDevices");
    if nd >= 1 {
        device_suite(gipa, inst, devs[0]);
    }
    if win32 && nd >= 1 && !hwnd.is_null() {
        surface_suite(gipa, inst, devs[0], hwnd);
    }
    if win32 {
        check!(
            !gipa(inst, c"vkGetPhysicalDeviceWin32PresentationSupportKHR".as_ptr()).is_null(),
            "vkGetPhysicalDeviceWin32PresentationSupportKHR"
        );
    }
    type Destroy = unsafe extern "system" fn(usize, *const c_void);
    let destroy: Destroy = vk(gipa, inst, c"vkDestroyInstance").unwrap();
    destroy(inst, null());
    check!(true, "vkDestroyInstance");
}

/// Surface Win32 sur la fenêtre de test (sautée sans affichage).
unsafe fn surface_suite(gipa: Gipa, inst: usize, phys: usize, hwnd: Handle) {
    #[repr(C)]
    struct Win32Surface {
        s_type: u32,
        next: *const c_void,
        flags: u32,
        hinstance: Handle,
        hwnd: Handle,
    }
    type Create = unsafe extern "system" fn(usize, *const Win32Surface, *const c_void, *mut u64) -> i32;
    let Some(create): Option<Create> = vk(gipa, inst, c"vkCreateWin32SurfaceKHR") else {
        check!(false, "vkCreateWin32SurfaceKHR par vkGetInstanceProcAddr");
        return;
    };
    let info = Win32Surface { s_type: 1_000_009_000, next: null(), flags: 0, hinstance: null_mut(), hwnd };
    let mut surface = 0u64;
    let r = create(inst, &info, null(), &mut surface);
    if r == -3 {
        out(b"  --     pas de fenetre X11 : surface Vulkan sautee\n");
        return;
    }
    check!(r == VK_SUCCESS && surface != 0, "vkCreateWin32SurfaceKHR");
    if surface == 0 {
        return;
    }
    type Support = unsafe extern "system" fn(usize, u32, u64, *mut u32) -> i32;
    let support: Support = vk(gipa, inst, c"vkGetPhysicalDeviceSurfaceSupportKHR").unwrap();
    let mut ok = 0u32;
    check!(support(phys, 0, surface, &mut ok) == VK_SUCCESS && ok == 1, "surface présentable");
    // VkSurfaceCapabilitiesKHR : currentExtent en +8.
    type Caps = unsafe extern "system" fn(usize, u64, *mut [u32; 13]) -> i32;
    let caps: Caps = vk(gipa, inst, c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR").unwrap();
    let mut c = [0u32; 13];
    check!(caps(phys, surface, &mut c) == VK_SUCCESS && c[2] > 0 && c[3] > 0, "capacités de la surface");
    type Destroy = unsafe extern "system" fn(usize, u64, *const c_void);
    let destroy: Destroy = vk(gipa, inst, c"vkDestroySurfaceKHR").unwrap();
    destroy(inst, surface, null());
}

unsafe fn device_suite(gipa: Gipa, inst: usize, phys: usize) {
    let prio = 1.0f32;
    let q = QueueInfo { s_type: 2, next: null(), flags: 0, family: 0, count: 1, priorities: &prio };
    let info = DeviceInfo {
        s_type: 3,
        next: null(),
        flags: 0,
        queue_count: 1,
        queues: &q,
        layers: 0,
        layer_names: null(),
        exts: 0,
        ext_names: null(),
        features: null(),
    };
    type CreateDevice = unsafe extern "system" fn(usize, *const DeviceInfo, *const c_void, *mut usize) -> i32;
    let create_device: CreateDevice = vk(gipa, inst, c"vkCreateDevice").unwrap();
    let mut dev = 0usize;
    check!(create_device(phys, &info, null(), &mut dev) == VK_SUCCESS, "vkCreateDevice");
    if dev == 0 {
        return;
    }
    type Gdpa = unsafe extern "system" fn(usize, *const c_char) -> *const c_void;
    let gdpa: Gdpa = vk(gipa, inst, c"vkGetDeviceProcAddr").unwrap();
    let d = |name: &core::ffi::CStr| gdpa(dev, name.as_ptr());
    let create_buffer: unsafe extern "system" fn(usize, *const BufferInfo, *const c_void, *mut u64) -> i32 =
        core::mem::transmute(d(c"vkCreateBuffer"));
    let reqs: unsafe extern "system" fn(usize, u64, *mut MemReq) = core::mem::transmute(d(c"vkGetBufferMemoryRequirements"));
    let alloc: unsafe extern "system" fn(usize, *const AllocInfo, *const c_void, *mut u64) -> i32 =
        core::mem::transmute(d(c"vkAllocateMemory"));
    let bind: unsafe extern "system" fn(usize, u64, u64, u64) -> i32 = core::mem::transmute(d(c"vkBindBufferMemory"));
    let map: unsafe extern "system" fn(usize, u64, u64, u64, u32, *mut *mut u8) -> i32 =
        core::mem::transmute(d(c"vkMapMemory"));
    let unmap: unsafe extern "system" fn(usize, u64) = core::mem::transmute(d(c"vkUnmapMemory"));
    let free: unsafe extern "system" fn(usize, u64, *const c_void) = core::mem::transmute(d(c"vkFreeMemory"));
    let destroy_buffer: unsafe extern "system" fn(usize, u64, *const c_void) = core::mem::transmute(d(c"vkDestroyBuffer"));
    let destroy_device: unsafe extern "system" fn(usize, *const c_void) = core::mem::transmute(d(c"vkDestroyDevice"));

    // Propriétés mémoire : trouver un type HOST_VISIBLE | HOST_COHERENT.
    let props_fn: unsafe extern "system" fn(usize, *mut u8) =
        vk(gipa, inst, c"vkGetPhysicalDeviceMemoryProperties").unwrap();
    let mut props = [0u8; 520];
    props_fn(phys, props.as_mut_ptr());
    let count = u32::from_ne_bytes([props[0], props[1], props[2], props[3]]);

    let bi = BufferInfo {
        s_type: 12,
        next: null(),
        flags: 0,
        size: 256,
        usage: 0x20, // STORAGE_BUFFER
        sharing: 0,
        families: 0,
        family_list: null(),
    };
    let mut buf = 0u64;
    check!(create_buffer(dev, &bi, null(), &mut buf) == VK_SUCCESS && buf != 0, "vkCreateBuffer");
    let mut mr = MemReq { size: 0, align: 0, bits: 0 };
    reqs(dev, buf, &mut mr);
    check!(mr.size >= 256, "vkGetBufferMemoryRequirements");
    let mut index = u32::MAX;
    for i in 0..count.min(32) {
        let off = 4 + i as usize * 8;
        let flags = u32::from_ne_bytes([props[off], props[off + 1], props[off + 2], props[off + 3]]);
        if mr.bits & (1 << i) != 0 && flags & 6 == 6 {
            index = i;
            break;
        }
    }
    check!(index != u32::MAX, "mémoire visible par le processeur");
    if index != u32::MAX {
        let ai = AllocInfo { s_type: 5, next: null(), size: mr.size, index };
        let mut mem = 0u64;
        check!(alloc(dev, &ai, null(), &mut mem) == VK_SUCCESS, "vkAllocateMemory");
        check!(bind(dev, buf, mem, 0) == VK_SUCCESS, "vkBindBufferMemory");
        let mut p: *mut u8 = null_mut();
        check!(map(dev, mem, 0, 256, 0, &mut p) == VK_SUCCESS && !p.is_null(), "vkMapMemory");
        if !p.is_null() {
            for i in 0..256 {
                *p.add(i) = i as u8;
            }
            check!(*p.add(200) == 200, "écriture dans la mémoire Vulkan");
            unmap(dev, mem);
        }
        destroy_buffer(dev, buf, null());
        free(dev, mem, null());
    }
    destroy_device(dev, null());
    check!(true, "vkDestroyDevice");
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    out(b"Winlux - suite OpenGL / Vulkan\n");
    unsafe {
        let hwnd = gl_suite();
        vk_suite(hwnd);
        if FAILURES == 0 {
            out(b"\nTous les tests OpenGL et Vulkan passent.\n");
        }
    }
    finish()
}
