//! Direct3D 12 (`d3d12.dll`) maison, exécuté sur Vulkan quand le matériel
//! le permet, sinon sur OpenGL, sinon sur le processeur.
//!
//! Couvre `ID3D12Device`, files de commandes, allocateurs, listes de
//! commandes graphiques (enregistrement puis rejeu), tas de descripteurs
//! (poignées CPU/GPU réelles), signatures racines, PSO, ressources
//! « committed »/« placed » avec mémoire CPU (Map/Unmap), barrières,
//! barrières de clôture (`ID3D12Fence` + évènements Win32) et la swapchain
//! DXGI propre à D3D12 (`IDXGISwapChain3/4`, `GetCurrentBackBufferIndex`).
//!
//! Choix d'implémentation :
//! - chaque ressource a une copie CPU ; les cibles de rendu ont en plus une
//!   image GPU (Vulkan ou texture GL). Un drapeau indique quelle copie est
//!   la plus récente, la synchronisation se fait à la demande ;
//! - `ExecuteCommandLists` rejoue les commandes de façon synchrone : une
//!   barrière de clôture signalée par la file est donc atteinte dès le
//!   retour, ce qui rend `SetEventOnCompletion` immédiat ;
//! - les shaders DXBC/DXIL ne sont pas encore traduits : les dessins
//!   utilisent des shaders passe-plat (position et couleur de sommet),
//!   comme le chemin Direct3D 11 ;
//! - garde-fous : sans Vulkan utilisable, OpenGL prend le relais ; sans
//!   OpenGL (pas d'affichage), l'appareil fonctionne sur le processeur
//!   (effacements, copies, lectures) sans rien afficher. Aucun cas ne fait
//!   échouer `D3D12CreateDevice` ni ne plante.
#![allow(non_snake_case, dead_code, clippy::missing_safety_doc)]

use crate::dlls::dxgi::{DxgiModeDesc, DxgiSampleDesc, DxgiSwapChainDesc, GlContext};
use crate::dlls::gl_bindings as gl;
use crate::dlls::gpu_caps::{self, Backend};
use crate::dlls::ole32::Guid;
use crate::dlls::vk_backend::{dxgi_to_vk, PipelineKey, VkContext, VkImageAlloc, VkPresenter};
use crate::dlls::vk_bindings::{
    VkViewport, VK_FORMAT_UNDEFINED, VK_INDEX_TYPE_UINT16, VK_INDEX_TYPE_UINT32,
    VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST, VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP,
};
use crate::{log_info, log_warn};
use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

// ── HRESULT ────────────────────────────────────────────────────────────────
const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const DXGI_ERROR_NOT_FOUND: i32 = 0x887a_0002u32 as i32;
const DXGI_ERROR_INVALID_CALL: i32 = 0x887a_0001u32 as i32;

// ── Constantes D3D12 ───────────────────────────────────────────────────────
const DIM_BUFFER: u32 = 1;
const DIM_TEX1D: u32 = 2;
const DIM_TEX2D: u32 = 3;
const DIM_TEX3D: u32 = 4;
const FLAG_RENDER_TARGET: u32 = 0x1;
const FLAG_DEPTH_STENCIL: u32 = 0x2;
const HEAP_DEFAULT: u32 = 1;
const HEAP_UPLOAD: u32 = 2;
const HEAP_READBACK: u32 = 3;
const DESC_SIZE: u32 = 32;
const PITCH_ALIGN: u64 = 256;
const PLACEMENT_ALIGN: u64 = 512;
const MAX_FEATURE_LEVEL: u32 = 0xc100; // 12_1
const LUID: u64 = 0x0001_0000;

const DESC_NONE: u32 = 0;
const DESC_CBV: u32 = 1;
const DESC_SRV: u32 = 2;
const DESC_UAV: u32 = 3;
const DESC_RTV: u32 = 4;
const DESC_DSV: u32 = 5;
const DESC_SAMPLER: u32 = 6;

// ── Structures du SDK (dispositions x64 vérifiées dans les tests) ──────────

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ResourceDesc {
    pub dimension: u32,
    pub alignment: u64,
    pub width: u64,
    pub height: u32,
    pub depth_or_array: u16,
    pub mip_levels: u16,
    pub format: u32,
    pub sample_count: u32,
    pub sample_quality: u32,
    pub layout: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct HeapProperties {
    ty: u32,
    cpu_page: u32,
    pool: u32,
    creation_mask: u32,
    visible_mask: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct QueueDesc {
    ty: u32,
    priority: i32,
    flags: u32,
    node_mask: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct HeapDesc {
    ty: u32,
    count: u32,
    flags: u32,
    node_mask: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Footprint {
    format: u32,
    width: u32,
    height: u32,
    depth: u32,
    row_pitch: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct PlacedFootprint {
    offset: u64,
    footprint: Footprint,
}

#[repr(C)]
struct CopyLocation {
    resource: *mut Obj,
    ty: u32,
    // Union : index de sous-ressource (u32) ou PlacedFootprint.
    placed: PlacedFootprint,
}

#[repr(C)]
struct Barrier {
    ty: u32,
    flags: u32,
    resource: *mut Obj,
    subresource: u32,
    before: u32,
    after: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VertexBufferView {
    location: u64,
    size: u32,
    stride: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IndexBufferView {
    location: u64,
    size: u32,
    format: u32,
}

#[repr(C)]
struct InputElement {
    semantic: *const i8,
    semantic_index: u32,
    format: u32,
    slot: u32,
    offset: u32,
    class: u32,
    step: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct SwapDesc1 {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub stereo: i32,
    pub sample: DxgiSampleDesc,
    pub usage: u32,
    pub count: u32,
    pub scaling: u32,
    pub effect: u32,
    pub alpha: u32,
    pub flags: u32,
}

// Décalages dans D3D12_GRAPHICS_PIPELINE_STATE_DESC (656 octets en x64).
const PSO_INPUT_LAYOUT: usize = 552;
const PSO_TOPOLOGY_TYPE: usize = 572;
const PSO_NUM_RT: usize = 576;
const PSO_RT_FORMATS: usize = 580;

// ── Formats ────────────────────────────────────────────────────────────────

/// (octets par bloc, côté du bloc en pixels) d'un format DXGI.
pub(crate) fn fmt_block(f: u32) -> Option<(u32, u32)> {
    Some(match f {
        1..=4 => (16, 1),
        5..=8 => (12, 1),
        9..=22 => (8, 1),
        23..=47 | 67..=69 | 87..=93 | 100 => (4, 1),
        48..=59 | 85 | 86 | 115 => (2, 1),
        60..=65 => (1, 1),
        70..=72 | 79..=81 => (8, 4),
        73..=78 | 82..=84 | 94..=99 => (16, 4),
        _ => return None,
    })
}

fn blocks(n: u32, dim: u32) -> u32 {
    n.max(1).div_ceil(dim)
}

/// Disposition d'une sous-ressource dans la copie CPU (lignes serrées).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SubLayout {
    pub offset: u64,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub row_bytes: u32,
    /// Lignes de blocs par tranche.
    pub rows: u32,
}

impl SubLayout {
    fn size(&self) -> u64 {
        self.row_bytes as u64 * self.rows as u64 * self.depth as u64
    }
}

fn mip_count(d: &ResourceDesc) -> u32 {
    if d.dimension == DIM_BUFFER {
        return 1;
    }
    if d.mip_levels != 0 {
        return d.mip_levels as u32;
    }
    let largest = (d.width as u32)
        .max(d.height)
        .max(if d.dimension == DIM_TEX3D {
            d.depth_or_array as u32
        } else {
            1
        });
    32 - largest.max(1).leading_zeros()
}

fn layer_count(d: &ResourceDesc) -> u32 {
    if d.dimension == DIM_TEX3D || d.dimension == DIM_BUFFER {
        1
    } else {
        (d.depth_or_array as u32).max(1)
    }
}

/// Sous-ressources dans l'ordre D3D12 (mip + couche × nombre de mips).
pub(crate) fn sub_layouts(d: &ResourceDesc) -> Option<Vec<SubLayout>> {
    if d.dimension == DIM_BUFFER {
        return Some(vec![SubLayout {
            offset: 0,
            width: d.width as u32,
            height: 1,
            depth: 1,
            row_bytes: d.width as u32,
            rows: 1,
        }]);
    }
    let (bytes, dim) = fmt_block(d.format)?;
    let mut out = Vec::new();
    let mut offset = 0u64;
    for _layer in 0..layer_count(d) {
        for mip in 0..mip_count(d) {
            let w = ((d.width as u32) >> mip).max(1);
            let h = if d.dimension == DIM_TEX1D {
                1
            } else {
                (d.height >> mip).max(1)
            };
            let depth = if d.dimension == DIM_TEX3D {
                ((d.depth_or_array as u32) >> mip).max(1)
            } else {
                1
            };
            let l = SubLayout {
                offset,
                width: w,
                height: h,
                depth,
                row_bytes: blocks(w, dim) * bytes,
                rows: blocks(h, dim),
            };
            offset += l.size();
            out.push(l);
        }
    }
    Some(out)
}

fn resource_size(d: &ResourceDesc) -> Option<u64> {
    if d.dimension == DIM_BUFFER {
        return Some(d.width);
    }
    sub_layouts(d).map(|v| v.last().map(|l| l.offset + l.size()).unwrap_or(0))
}

fn align(v: u64, a: u64) -> u64 {
    v.div_ceil(a) * a
}

fn f32_to_f16(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp <= 0 {
        sign
    } else if exp >= 31 {
        sign | 0x7c00
    } else {
        sign | ((exp as u16) << 10) | (man >> 13) as u16
    }
}

/// Encode une couleur RVBA flottante dans le format d'un texel.
pub(crate) fn encode_color(format: u32, c: [f32; 4]) -> Option<Vec<u8>> {
    let u8c = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Some(match format {
        27..=32 => vec![u8c(c[0]), u8c(c[1]), u8c(c[2]), u8c(c[3])],
        87 | 90 | 91 => vec![u8c(c[2]), u8c(c[1]), u8c(c[0]), u8c(c[3])],
        88 | 92 | 93 => vec![u8c(c[2]), u8c(c[1]), u8c(c[0]), 255],
        1 | 2 => c.iter().flat_map(|v| v.to_le_bytes()).collect(),
        9 | 10 => c
            .iter()
            .flat_map(|v| f32_to_f16(*v).to_le_bytes())
            .collect(),
        23 | 24 => {
            let q = |v: f32, m: f32| (v.clamp(0.0, 1.0) * m + 0.5) as u32;
            let p = q(c[0], 1023.0)
                | (q(c[1], 1023.0) << 10)
                | (q(c[2], 1023.0) << 20)
                | (q(c[3], 3.0) << 30);
            p.to_le_bytes().to_vec()
        }
        41 => c[0].to_le_bytes().to_vec(),
        61 => vec![u8c(c[0])],
        49 => vec![u8c(c[0]), u8c(c[1])],
        _ => return None,
    })
}

// ── Objets COM ─────────────────────────────────────────────────────────────

#[repr(C)]
pub(crate) struct Obj {
    vtbl: *const usize,
    refs: AtomicU32,
    /// Appareil parent (référence tenue) ; pour une swapchain : la file.
    device: *mut Obj,
    kind: Kind,
}

unsafe impl Send for Obj {}
unsafe impl Sync for Obj {}

enum Kind {
    Device(Box<Device>),
    Queue(QueueDesc),
    Allocator(u32),
    List(Box<Mutex<List>>),
    Fence(Box<Fence>),
    DescHeap(Box<DescHeap>),
    RootSig(Vec<u8>),
    Pso(Box<Pso>),
    Resource(Box<Res>),
    Heap(u64, HeapProperties),
    QueryHeap,
    CmdSig,
    Blob(Vec<u8>),
    SwapChain(Box<Mutex<SwapChain>>),
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) struct Device {
    gpu: Gpu,
}

enum Gpu {
    Vk(Arc<VkContext>),
    Gl(Mutex<GlBackend>),
    Cpu,
}

impl Gpu {
    fn label(&self) -> &'static str {
        match self {
            Gpu::Vk(_) => "vulkan",
            Gpu::Gl(_) => "opengl",
            Gpu::Cpu => "processeur",
        }
    }
}

pub(crate) struct Res {
    desc: ResourceDesc,
    heap: HeapProperties,
    va: u64,
    cpu: *mut u8,
    len: usize,
    layouts: Vec<SubLayout>,
    data: Mutex<ResData>,
}

#[derive(Default)]
struct ResData {
    gpu: Option<GpuImage>,
    /// La copie CPU est plus récente que l'image GPU.
    cpu_newer: bool,
    /// L'image GPU est plus récente que la copie CPU.
    gpu_newer: bool,
}

enum GpuImage {
    Vk(VkImageAlloc),
    Gl {
        tex: u32,
        fbo: u32,
        format: u32,
        ty: u32,
    },
}

impl Res {
    fn bytes(&self) -> &mut [u8] {
        if self.cpu.is_null() {
            return &mut [];
        }
        unsafe { std::slice::from_raw_parts_mut(self.cpu, self.len) }
    }
}

struct Fence {
    value: AtomicU64,
    waiters: Mutex<Vec<(u64, usize)>>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Descriptor {
    kind: u32,
    format: u32,
    resource: *mut Obj,
    va: u64,
    size: u32,
    _pad: u32,
}

struct DescHeap {
    desc: HeapDesc,
    mem: Box<[Descriptor]>,
    gpu_base: u64,
}

struct Pso {
    compute: bool,
    /// (location, format DXGI, décalage) : 0 = position, 1 = couleur.
    layout: Vec<(u32, u32, u32)>,
    rt_format: u32,
    topology_type: u32,
}

#[derive(Clone, Copy, Default)]
struct ListState {
    pso: usize,
    rt: usize,
    dsv: usize,
    viewport: Option<[f32; 6]>,
    topology: u32,
    vb: Option<(u64, u32, u32)>,
    ib: Option<(u64, u32, u32)>,
}

struct List {
    ty: u32,
    open: bool,
    cmds: Vec<Cmd>,
    st: ListState,
}

#[derive(Clone, Copy)]
enum Loc {
    Sub(*mut Obj, u32),
    Placed(*mut Obj, PlacedFootprint),
}

struct DrawCall {
    target: *mut Obj,
    viewport: [f32; 6],
    topology: u32,
    layout: Vec<(u32, u32, u32)>,
    vb: (u64, u32, u32),
    ib: Option<(u64, u32, u32)>,
    count: u32,
    instances: u32,
    first: u32,
    base: i32,
    first_instance: u32,
}

enum Cmd {
    ClearRt(*mut Obj, [f32; 4], Vec<[i32; 4]>),
    ClearDs(*mut Obj, f32, u8, u32),
    CopyBuffer(*mut Obj, u64, *mut Obj, u64, u64),
    CopyResource(*mut Obj, *mut Obj),
    CopyTexture(Loc, [u32; 3], Loc, Option<[u32; 6]>),
    Draw(Box<DrawCall>),
    Barrier,
}

unsafe impl Send for List {}

pub(crate) struct SwapChain {
    desc: SwapDesc1,
    hwnd: usize,
    buffers: Vec<*mut Obj>,
    index: u32,
    presents: u32,
    presenter: Option<VkPresenter>,
    gl_window: Option<u64>,
    latency: u32,
    waitable: usize,
    warned: bool,
}

unsafe impl Send for SwapChain {}

// ── Registres : adresses GPU et tas de descripteurs ────────────────────────

static NEXT_VA: AtomicU64 = AtomicU64::new(0x0010_0000_0000);

fn va_registry() -> &'static Mutex<Vec<(u64, u64, usize)>> {
    static R: OnceLock<Mutex<Vec<(u64, u64, usize)>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Vec::new()))
}

fn va_alloc(size: u64) -> u64 {
    let size = align(size.max(1), 0x1_0000);
    NEXT_VA.fetch_add(size, Ordering::Relaxed)
}

/// Ressource et décalage correspondant à une adresse GPU virtuelle.
fn va_lookup(addr: u64) -> Option<(*mut Obj, u64)> {
    let r = lock(va_registry());
    r.iter()
        .find(|(b, s, _)| addr >= *b && addr < b + s.max(&1))
        .map(|(b, _, o)| (*o as *mut Obj, addr - b))
}

fn heap_registry() -> &'static Mutex<Vec<(usize, usize)>> {
    static R: OnceLock<Mutex<Vec<(usize, usize)>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Vec::new()))
}

/// Descripteur pointé par une poignée CPU, après vérification qu'elle
/// tombe dans un tas connu (une poignée invalide ne fait pas planter).
fn descriptor_at(handle: usize) -> Option<*mut Descriptor> {
    let r = lock(heap_registry());
    r.iter()
        .find(|(s, e)| handle >= *s && handle < *e && (handle - s) % DESC_SIZE as usize == 0)
        .map(|_| handle as *mut Descriptor)
}

fn descriptor_resource(handle: usize) -> *mut Obj {
    match descriptor_at(handle) {
        Some(d) => unsafe { (*d).resource },
        None => std::ptr::null_mut(),
    }
}

// ── Vtables ────────────────────────────────────────────────────────────────

unsafe extern "win64" fn stub_ok() -> i32 {
    S_OK
}
unsafe extern "win64" fn stub_notimpl() -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn stub_zero() -> u32 {
    0
}

macro_rules! f {
    ($f:expr) => {
        $f as *const () as usize
    };
}

fn build(n: usize, default: usize, entries: &[(usize, usize)]) -> &'static [usize] {
    let mut v = vec![default; n];
    for &(i, p) in entries {
        v[i] = p;
    }
    Box::leak(v.into_boxed_slice())
}

/// Entrées communes : IUnknown + ID3D12Object (+ GetDevice en 7).
fn common(with_get_device: bool) -> Vec<(usize, usize)> {
    let mut v = vec![
        (0, f!(obj_qi)),
        (1, f!(obj_addref)),
        (2, f!(obj_release)),
        (3, f!(obj_get_private)),
        (4, f!(stub_ok)),
        (5, f!(stub_ok)),
        (6, f!(stub_ok)),
    ];
    if with_get_device {
        v.push((7, f!(obj_get_device)));
    }
    v
}

macro_rules! vtable {
    ($name:ident, $n:expr, $default:expr, [$($e:expr),* $(,)?]) => {
        fn $name() -> &'static [usize] {
            static V: OnceLock<usize> = OnceLock::new();
            let p = *V.get_or_init(|| {
                let mut e = common(true);
                let extra: &[(usize, usize)] = &[$($e),*];
                e.extend_from_slice(extra);
                build($n, $default, &e).as_ptr() as usize
            });
            unsafe { std::slice::from_raw_parts(p as *const usize, $n) }
        }
    };
}

fn device_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        let mut e = common(false);
        e.extend([
            (7, f!(dev_node_count)),
            (8, f!(dev_create_queue)),
            (9, f!(dev_create_allocator)),
            (10, f!(dev_create_gfx_pso)),
            (11, f!(dev_create_compute_pso)),
            (12, f!(dev_create_list)),
            (13, f!(dev_check_feature)),
            (14, f!(dev_create_desc_heap)),
            (15, f!(dev_increment)),
            (16, f!(dev_create_root_sig)),
            (17, f!(dev_create_cbv)),
            (18, f!(dev_create_srv)),
            (19, f!(dev_create_uav)),
            (20, f!(dev_create_rtv)),
            (21, f!(dev_create_dsv)),
            (22, f!(dev_create_sampler)),
            (23, f!(dev_copy_descriptors)),
            (24, f!(dev_copy_descriptors_simple)),
            (25, f!(dev_alloc_info)),
            (26, f!(dev_custom_heap_props)),
            (27, f!(dev_create_committed)),
            (28, f!(dev_create_heap)),
            (29, f!(dev_create_placed)),
            (30, f!(stub_notimpl)),
            (34, f!(stub_ok)),
            (35, f!(stub_ok)),
            (36, f!(dev_create_fence)),
            (37, f!(stub_ok)),
            (38, f!(dev_footprints)),
            (39, f!(dev_create_query_heap)),
            (40, f!(stub_ok)),
            (41, f!(dev_create_cmd_sig)),
            (42, f!(stub_ok)),
            (43, f!(dev_luid)),
            (45, f!(dev_event_on_multiple)),
            (46, f!(stub_ok)),
        ]);
        build(60, f!(stub_notimpl), &e).as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 60) }
}

vtable!(
    queue_vtable,
    19,
    f!(stub_ok),
    [
        (8, f!(stub_ok)),
        (9, f!(stub_ok)),
        (10, f!(queue_execute)),
        (14, f!(queue_signal)),
        (15, f!(queue_wait)),
        (16, f!(queue_ts_freq)),
        (17, f!(queue_clock)),
        (18, f!(queue_desc)),
    ]
);
vtable!(allocator_vtable, 9, f!(stub_ok), [(8, f!(stub_ok))]);
vtable!(
    list_vtable,
    70,
    f!(stub_ok),
    [
        (8, f!(list_type)),
        (9, f!(list_close)),
        (10, f!(list_reset)),
        (11, f!(list_clear_state)),
        (12, f!(list_draw)),
        (13, f!(list_draw_indexed)),
        (15, f!(list_copy_buffer)),
        (16, f!(list_copy_texture)),
        (17, f!(list_copy_resource)),
        (19, f!(list_resolve)),
        (20, f!(list_topology)),
        (21, f!(list_viewports)),
        (25, f!(list_set_pso)),
        (26, f!(list_barrier)),
        (43, f!(list_ib)),
        (44, f!(list_vbs)),
        (46, f!(list_om_rts)),
        (47, f!(list_clear_dsv)),
        (48, f!(list_clear_rtv)),
    ]
);
vtable!(
    fence_vtable,
    11,
    f!(stub_ok),
    [
        (8, f!(fence_completed)),
        (9, f!(fence_event)),
        (10, f!(fence_cpu_signal)),
    ]
);
vtable!(
    desc_heap_vtable,
    11,
    f!(stub_ok),
    [
        (8, f!(heap_desc)),
        (9, f!(heap_cpu_start)),
        (10, f!(heap_gpu_start)),
    ]
);
vtable!(root_sig_vtable, 8, f!(stub_ok), []);
vtable!(pso_vtable, 9, f!(stub_ok), [(8, f!(stub_notimpl))]);
vtable!(
    resource_vtable,
    15,
    f!(stub_ok),
    [
        (8, f!(res_map)),
        (9, f!(res_unmap)),
        (10, f!(res_desc)),
        (11, f!(res_va)),
        (12, f!(res_write)),
        (13, f!(res_read)),
        (14, f!(res_heap_props)),
    ]
);
vtable!(heap_vtable, 9, f!(stub_ok), [(8, f!(memheap_desc))]);
vtable!(query_heap_vtable, 8, f!(stub_ok), []);
vtable!(cmd_sig_vtable, 8, f!(stub_ok), []);

fn blob_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            5,
            f!(stub_zero),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (3, f!(blob_ptr)),
                (4, f!(blob_size)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 5) }
}

fn swapchain_vtable() -> &'static [usize] {
    static V: OnceLock<usize> = OnceLock::new();
    let p = *V.get_or_init(|| {
        build(
            41,
            f!(stub_ok),
            &[
                (0, f!(obj_qi)),
                (1, f!(obj_addref)),
                (2, f!(obj_release)),
                (5, f!(obj_get_private)),
                (6, f!(sc_get_parent)),
                (7, f!(sc_get_device)),
                (8, f!(sc_present)),
                (9, f!(sc_get_buffer)),
                (10, f!(stub_ok)),
                (11, f!(sc_get_fullscreen)),
                (12, f!(sc_get_desc)),
                (13, f!(sc_resize)),
                (15, f!(sc_no_output)),
                (16, f!(stub_notimpl)),
                (17, f!(sc_present_count)),
                (18, f!(sc_get_desc1)),
                (19, f!(sc_get_fs_desc)),
                (20, f!(sc_get_hwnd)),
                (21, f!(stub_notimpl)),
                (22, f!(sc_present1)),
                (23, f!(stub_zero)),
                (24, f!(sc_no_output)),
                (28, f!(sc_get_rotation)),
                (30, f!(sc_get_source_size)),
                (31, f!(sc_set_latency)),
                (32, f!(sc_get_latency)),
                (33, f!(sc_waitable)),
                (35, f!(stub_notimpl)),
                (36, f!(sc_current_index)),
                (37, f!(sc_color_space_support)),
                (38, f!(stub_ok)),
                (39, f!(sc_resize1)),
                (40, f!(stub_ok)),
            ],
        )
        .as_ptr() as usize
    });
    unsafe { std::slice::from_raw_parts(p as *const usize, 41) }
}

// ── IUnknown / ID3D12Object ────────────────────────────────────────────────

fn iid_d1(iid: *const Guid) -> Option<u32> {
    if iid.is_null() {
        None
    } else {
        Some(unsafe { (*iid).data1 })
    }
}

const IID_UNKNOWN: u32 = 0x0000_0000;
const IID_OBJECT: u32 = 0xc4fe_c28f;
const IID_DEVICE_CHILD: u32 = 0x905d_b94b;
const IID_PAGEABLE: u32 = 0x63ee_58fb;

fn supports(kind: &Kind, d1: u32) -> bool {
    if d1 == IID_UNKNOWN {
        return true;
    }
    let child = [IID_OBJECT, IID_DEVICE_CHILD, IID_PAGEABLE];
    let own: &[u32] = match kind {
        Kind::Device(_) => return [IID_OBJECT, 0x1898_19f1, 0x77ac_ce80].contains(&d1),
        Kind::Queue(_) => &[0x0ec8_70a6],
        Kind::Allocator(_) => &[0x6102_dee4],
        Kind::List(_) => &[0x7116_d91c, 0x5b16_0d0f],
        Kind::Fence(_) => &[0x0a75_3dcf],
        Kind::DescHeap(_) => &[0x8efb_471d],
        Kind::RootSig(_) => &[0xc54a_6b66],
        Kind::Pso(_) => &[0x765a_30f3],
        Kind::Resource(_) => &[0x6964_42be],
        Kind::Heap(..) => &[0x6b3b_2502],
        Kind::QueryHeap => &[0x0d96_58ae],
        Kind::CmdSig => &[0xc36a_797c],
        Kind::Blob(_) => return d1 == 0x8ba5_fb08,
        Kind::SwapChain(_) => {
            return [
                0xaec2_2fb8, // IDXGIObject
                0x3d3e_0379, // IDXGIDeviceSubObject
                0x310d_36a0, // IDXGISwapChain
                0x790a_45f7, // 1
                0xa8be_2ac4, // 2
                0x94d9_9bdb, // 3
                0x3d58_5d5a, // 4
            ]
            .contains(&d1);
        }
    };
    child.contains(&d1) || own.contains(&d1)
}

unsafe extern "win64" fn obj_qi(this: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match iid_d1(iid) {
        Some(d1) if supports(&(*this).kind, d1) => {
            obj_addref(this);
            *out = this.cast();
            S_OK
        }
        _ => {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}

unsafe extern "win64" fn obj_addref(this: *mut Obj) -> u32 {
    (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "win64" fn obj_release(this: *mut Obj) -> u32 {
    let prev = (*this).refs.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        destroy(this);
        return 0;
    }
    prev.saturating_sub(1)
}

unsafe extern "win64" fn obj_get_private(
    _: *mut Obj,
    _: *const Guid,
    size: *mut u32,
    _: *mut c_void,
) -> i32 {
    if !size.is_null() {
        *size = 0;
    }
    DXGI_ERROR_NOT_FOUND
}

unsafe extern "win64" fn obj_get_device(
    this: *mut Obj,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let dev = (*this).device;
    if dev.is_null() || out.is_null() {
        return E_FAIL;
    }
    obj_qi(dev, iid, out)
}

fn new_obj(vtbl: &'static [usize], device: *mut Obj, kind: Kind) -> *mut Obj {
    if !device.is_null() {
        unsafe { obj_addref(device) };
    }
    Box::into_raw(Box::new(Obj {
        vtbl: vtbl.as_ptr(),
        refs: AtomicU32::new(1),
        device,
        kind,
    }))
}

/// Remet l'objet à l'appelant via l'IID demandé (référence transférée).
unsafe fn hand_out(obj: *mut Obj, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        obj_release(obj);
        return S_FALSE;
    }
    let hr = obj_qi(obj, iid, out);
    obj_release(obj);
    hr
}

unsafe fn destroy(this: *mut Obj) {
    let obj = Box::from_raw(this);
    match &obj.kind {
        Kind::Resource(r) => {
            lock(va_registry()).retain(|e| e.2 != this as usize);
            let gpu = lock(&r.data).gpu.take();
            if let Some(img) = gpu {
                free_image(obj.device, img);
            }
            if !r.cpu.is_null() {
                drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                    r.cpu, r.len,
                )));
            }
        }
        Kind::DescHeap(h) => {
            let start = h.mem.as_ptr() as usize;
            lock(heap_registry()).retain(|e| e.0 != start);
        }
        Kind::SwapChain(sc) => {
            let mut s = lock(sc);
            s.presenter = None;
            for b in std::mem::take(&mut s.buffers) {
                obj_release(b);
            }
            if s.waitable != 0 {
                crate::dlls::kernel32::close_handle(s.waitable);
            }
        }
        _ => {}
    }
    let parent = obj.device;
    drop(obj);
    if !parent.is_null() {
        obj_release(parent);
    }
}

unsafe fn device_of(obj: *mut Obj) -> Option<&'static Device> {
    let mut p = obj;
    // Remonte la swapchain → file → appareil.
    for _ in 0..3 {
        if p.is_null() {
            return None;
        }
        if let Kind::Device(d) = &(*p).kind {
            return Some(&*(d.as_ref() as *const Device));
        }
        p = (*p).device;
    }
    None
}

unsafe fn res_of(obj: *mut Obj) -> Option<&'static Res> {
    if obj.is_null() {
        return None;
    }
    match &(*obj).kind {
        Kind::Resource(r) => Some(&*(r.as_ref() as *const Res)),
        _ => None,
    }
}

// ── Blob ───────────────────────────────────────────────────────────────────

unsafe extern "win64" fn blob_ptr(this: *mut Obj) -> *mut u8 {
    match &mut (*this).kind {
        Kind::Blob(b) => b.as_mut_ptr(),
        _ => std::ptr::null_mut(),
    }
}
unsafe extern "win64" fn blob_size(this: *mut Obj) -> usize {
    match &(*this).kind {
        Kind::Blob(b) => b.len(),
        _ => 0,
    }
}

pub(crate) fn new_blob(bytes: Vec<u8>) -> *mut Obj {
    new_obj(blob_vtable(), std::ptr::null_mut(), Kind::Blob(bytes))
}

// ── ID3D12Device ───────────────────────────────────────────────────────────

unsafe fn dev_ptr(this: *mut Obj) -> Option<&'static Device> {
    match &(*this).kind {
        Kind::Device(d) => Some(&*(d.as_ref() as *const Device)),
        _ => None,
    }
}

unsafe extern "win64" fn dev_node_count(_: *mut Obj) -> u32 {
    1
}

unsafe extern "win64" fn dev_create_queue(
    this: *mut Obj,
    desc: *const QueueDesc,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    hand_out(new_obj(queue_vtable(), this, Kind::Queue(*desc)), iid, out)
}

unsafe extern "win64" fn dev_create_allocator(
    this: *mut Obj,
    ty: u32,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    hand_out(
        new_obj(allocator_vtable(), this, Kind::Allocator(ty)),
        iid,
        out,
    )
}

unsafe fn semantic(e: &InputElement) -> String {
    if e.semantic.is_null() {
        return String::new();
    }
    CStr::from_ptr(e.semantic)
        .to_string_lossy()
        .to_ascii_uppercase()
}

/// Disposition des sommets retenue pour les shaders passe-plat.
unsafe fn parse_input_layout(elems: *const InputElement, n: u32) -> Vec<(u32, u32, u32)> {
    let mut out = Vec::new();
    if elems.is_null() {
        return out;
    }
    let mut running = 0u32;
    let mut first: Option<(u32, u32)> = None;
    for i in 0..n.min(32) as usize {
        let e = &*elems.add(i);
        let offset = if e.offset == 0xffff_ffff {
            running
        } else {
            e.offset
        };
        running = offset + fmt_block(e.format).map(|b| b.0).unwrap_or(0);
        if e.slot != 0 {
            continue;
        }
        let name = semantic(e);
        if first.is_none() {
            first = Some((e.format, offset));
        }
        if (name == "POSITION" || name == "SV_POSITION")
            && !out.iter().any(|a: &(u32, u32, u32)| a.0 == 0)
        {
            out.push((0, e.format, offset));
        } else if name == "COLOR" && e.semantic_index == 0 && !out.iter().any(|a| a.0 == 1) {
            out.push((1, e.format, offset));
        }
    }
    if !out.iter().any(|a| a.0 == 0) {
        if let Some((f, o)) = first {
            out.push((0, f, o));
        }
    }
    out.sort();
    out
}

unsafe extern "win64" fn dev_create_gfx_pso(
    this: *mut Obj,
    desc: *const u8,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let rd = |o: usize| std::ptr::read_unaligned(desc.add(o) as *const u32);
    let elems = std::ptr::read_unaligned(desc.add(PSO_INPUT_LAYOUT) as *const *const InputElement);
    let count = rd(PSO_INPUT_LAYOUT + 8);
    let pso = Pso {
        compute: false,
        layout: parse_input_layout(elems, count),
        rt_format: if rd(PSO_NUM_RT) > 0 {
            rd(PSO_RT_FORMATS)
        } else {
            0
        },
        topology_type: rd(PSO_TOPOLOGY_TYPE),
    };
    hand_out(
        new_obj(pso_vtable(), this, Kind::Pso(Box::new(pso))),
        iid,
        out,
    )
}

unsafe extern "win64" fn dev_create_compute_pso(
    this: *mut Obj,
    desc: *const u8,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let pso = Pso {
        compute: true,
        layout: Vec::new(),
        rt_format: 0,
        topology_type: 0,
    };
    hand_out(
        new_obj(pso_vtable(), this, Kind::Pso(Box::new(pso))),
        iid,
        out,
    )
}

unsafe extern "win64" fn dev_create_list(
    this: *mut Obj,
    _node: u32,
    ty: u32,
    _alloc: *mut Obj,
    pso: *mut Obj,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let list = List {
        ty,
        open: true,
        cmds: Vec::new(),
        st: ListState {
            pso: pso as usize,
            topology: 0,
            ..Default::default()
        },
    };
    hand_out(
        new_obj(list_vtable(), this, Kind::List(Box::new(Mutex::new(list)))),
        iid,
        out,
    )
}

unsafe extern "win64" fn dev_check_feature(
    _: *mut Obj,
    feature: u32,
    data: *mut u8,
    size: u32,
) -> i32 {
    if data.is_null() || size == 0 {
        return E_INVALIDARG;
    }
    let w32 = |o: usize, v: u32| {
        if o + 4 <= size as usize {
            std::ptr::write_unaligned(data.add(o) as *mut u32, v);
        }
    };
    let r32 = |o: usize| -> u32 {
        if o + 4 <= size as usize {
            std::ptr::read_unaligned(data.add(o) as *const u32)
        } else {
            0
        }
    };
    match feature {
        // FEATURE_LEVELS : entrée/sortie, on garde la demande.
        2 => {
            if size < 24 {
                return E_INVALIDARG;
            }
            let n = r32(0);
            let levels = std::ptr::read_unaligned(data.add(8) as *const *const u32);
            let mut best = 0;
            if !levels.is_null() {
                for i in 0..n.min(16) as usize {
                    let l = *levels.add(i);
                    if l <= MAX_FEATURE_LEVEL && l > best {
                        best = l;
                    }
                }
            }
            w32(16, best);
            return S_OK;
        }
        3 => {
            let f = r32(0);
            let known = fmt_block(f).is_some();
            let rt = dxgi_to_vk(f) != VK_FORMAT_UNDEFINED;
            let mut s1 = 0;
            if known {
                s1 = 0x1 | 0x2 | 0x10 | 0x20 | 0x40 | 0x80 | 0x100 | 0x200 | 0x1000;
                if matches!(f, 42 | 57) {
                    s1 |= 0x4;
                }
                if rt {
                    s1 |= 0x4000 | 0x8000 | 0x8_0000;
                }
            }
            w32(4, s1);
            w32(8, 0);
            return if known { S_OK } else { E_FAIL };
        }
        4 => {
            let count = r32(4);
            w32(8, 0);
            w32(12, u32::from(count == 1));
            return S_OK;
        }
        5 => {
            let f = r32(0);
            if size >= 5 {
                *data.add(4) = if matches!(f, 20 | 45 | 103 | 104) {
                    2
                } else {
                    1
                };
            }
            return S_OK;
        }
        7 => {
            let wanted = r32(0);
            w32(0, wanted.min(0x51));
            return S_OK;
        }
        12 => {
            let wanted = r32(0);
            w32(0, wanted.clamp(1, 2));
            return S_OK;
        }
        _ => {}
    }
    // Options et capacités : zéro = « non pris en charge », valeur sûre.
    std::ptr::write_bytes(data, 0, size as usize);
    match feature {
        0 => {
            w32(16, 1); // ResourceBindingTier 1
            w32(36, 40); // MaxGPUVirtualAddressBitsPerResource
            w32(56, 1); // ResourceHeapTier 1
        }
        6 => {
            w32(0, 40);
            w32(4, 40);
        }
        _ => {}
    }
    S_OK
}

unsafe extern "win64" fn dev_create_desc_heap(
    this: *mut Obj,
    desc: *const HeapDesc,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() || (*desc).count == 0 || (*desc).count > 1_000_000 {
        return E_INVALIDARG;
    }
    let empty = Descriptor {
        kind: DESC_NONE,
        format: 0,
        resource: std::ptr::null_mut(),
        va: 0,
        size: 0,
        _pad: 0,
    };
    let mem = vec![empty; (*desc).count as usize].into_boxed_slice();
    let start = mem.as_ptr() as usize;
    let end = start + mem.len() * DESC_SIZE as usize;
    lock(heap_registry()).push((start, end));
    let heap = DescHeap {
        desc: *desc,
        mem,
        gpu_base: va_alloc((*desc).count as u64 * DESC_SIZE as u64),
    };
    hand_out(
        new_obj(desc_heap_vtable(), this, Kind::DescHeap(Box::new(heap))),
        iid,
        out,
    )
}

unsafe extern "win64" fn dev_increment(_: *mut Obj, _ty: u32) -> u32 {
    DESC_SIZE
}

unsafe extern "win64" fn dev_create_root_sig(
    this: *mut Obj,
    _node: u32,
    blob: *const u8,
    len: usize,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if blob.is_null() || len == 0 {
        return E_INVALIDARG;
    }
    let bytes = std::slice::from_raw_parts(blob, len.min(1 << 20)).to_vec();
    hand_out(
        new_obj(root_sig_vtable(), this, Kind::RootSig(bytes)),
        iid,
        out,
    )
}

unsafe fn write_descriptor(handle: usize, d: Descriptor) {
    if let Some(p) = descriptor_at(handle) {
        *p = d;
    } else {
        log_warn!(
            "d3d12",
            "poignée de descripteur inconnue 0x{:x} : ignorée",
            handle
        );
    }
}

fn descriptor(kind: u32, resource: *mut Obj, format: u32) -> Descriptor {
    Descriptor {
        kind,
        format,
        resource,
        va: 0,
        size: 0,
        _pad: 0,
    }
}

unsafe fn view_format(desc: *const u32, res: *mut Obj) -> u32 {
    if !desc.is_null() && *desc != 0 {
        return *desc;
    }
    res_of(res).map(|r| r.desc.format).unwrap_or(0)
}

unsafe extern "win64" fn dev_create_cbv(_: *mut Obj, desc: *const u64, handle: usize) {
    if desc.is_null() {
        return;
    }
    let mut d = descriptor(DESC_CBV, std::ptr::null_mut(), 0);
    d.va = *desc;
    d.size = *(desc.add(1) as *const u32);
    d.resource = va_lookup(d.va).map(|r| r.0).unwrap_or(std::ptr::null_mut());
    write_descriptor(handle, d);
}
unsafe extern "win64" fn dev_create_srv(
    _: *mut Obj,
    res: *mut Obj,
    desc: *const u32,
    handle: usize,
) {
    write_descriptor(handle, descriptor(DESC_SRV, res, view_format(desc, res)));
}
unsafe extern "win64" fn dev_create_uav(
    _: *mut Obj,
    res: *mut Obj,
    _counter: *mut Obj,
    desc: *const u32,
    handle: usize,
) {
    write_descriptor(handle, descriptor(DESC_UAV, res, view_format(desc, res)));
}
unsafe extern "win64" fn dev_create_rtv(
    _: *mut Obj,
    res: *mut Obj,
    desc: *const u32,
    handle: usize,
) {
    write_descriptor(handle, descriptor(DESC_RTV, res, view_format(desc, res)));
}
unsafe extern "win64" fn dev_create_dsv(
    _: *mut Obj,
    res: *mut Obj,
    desc: *const u32,
    handle: usize,
) {
    write_descriptor(handle, descriptor(DESC_DSV, res, view_format(desc, res)));
}
unsafe extern "win64" fn dev_create_sampler(_: *mut Obj, _desc: *const u8, handle: usize) {
    write_descriptor(handle, descriptor(DESC_SAMPLER, std::ptr::null_mut(), 0));
}

unsafe fn copy_range(dst: usize, src: usize, n: u32) {
    for i in 0..n as usize {
        let off = i * DESC_SIZE as usize;
        if let (Some(d), Some(s)) = (descriptor_at(dst + off), descriptor_at(src + off)) {
            *d = *s;
        }
    }
}

unsafe extern "win64" fn dev_copy_descriptors_simple(
    _: *mut Obj,
    n: u32,
    dst: usize,
    src: usize,
    _ty: u32,
) {
    copy_range(dst, src, n);
}

unsafe extern "win64" fn dev_copy_descriptors(
    _: *mut Obj,
    ndst: u32,
    dst_starts: *const usize,
    dst_sizes: *const u32,
    nsrc: u32,
    src_starts: *const usize,
    src_sizes: *const u32,
    _ty: u32,
) {
    if dst_starts.is_null() || src_starts.is_null() {
        return;
    }
    let mut srcs = Vec::new();
    for i in 0..nsrc as usize {
        let n = if src_sizes.is_null() {
            1
        } else {
            *src_sizes.add(i)
        };
        for k in 0..n as usize {
            srcs.push(*src_starts.add(i) + k * DESC_SIZE as usize);
        }
    }
    let mut it = srcs.into_iter();
    for i in 0..ndst as usize {
        let n = if dst_sizes.is_null() {
            1
        } else {
            *dst_sizes.add(i)
        };
        for k in 0..n as usize {
            let Some(s) = it.next() else { return };
            copy_range(*dst_starts.add(i) + k * DESC_SIZE as usize, s, 1);
        }
    }
}

fn normalized(d: &ResourceDesc) -> ResourceDesc {
    let mut n = *d;
    if n.dimension != DIM_BUFFER && n.mip_levels == 0 {
        n.mip_levels = mip_count(d) as u16;
    }
    if n.dimension == DIM_BUFFER {
        n.mip_levels = 1;
        n.height = 1;
        n.depth_or_array = 1;
    }
    if n.depth_or_array == 0 {
        n.depth_or_array = 1;
    }
    if n.sample_count == 0 {
        n.sample_count = 1;
    }
    n
}

unsafe extern "win64" fn dev_alloc_info(
    _: *mut Obj,
    out: *mut [u64; 2],
    _mask: u32,
    n: u32,
    descs: *const ResourceDesc,
) -> *mut [u64; 2] {
    let mut total = 0u64;
    for i in 0..n as usize {
        if descs.is_null() {
            break;
        }
        match resource_size(&normalized(&*descs.add(i))) {
            Some(s) => total = align(total, 0x1_0000) + align(s.max(1), 0x1_0000),
            None => {
                total = u64::MAX;
                break;
            }
        }
    }
    *out = [total, 0x1_0000];
    out
}

unsafe extern "win64" fn dev_custom_heap_props(
    _: *mut Obj,
    out: *mut HeapProperties,
    _node: u32,
    ty: u32,
) -> *mut HeapProperties {
    // CPU_PAGE_PROPERTY : 1 = non disponible, 3 = écriture différée.
    // MEMORY_POOL : 1 = L0 (système), 2 = L1 (vidéo).
    let (page, pool) = match ty {
        HEAP_UPLOAD | HEAP_READBACK => (3, 1),
        _ => (1, 2),
    };
    *out = HeapProperties {
        ty: 4,
        cpu_page: page,
        pool,
        creation_mask: 1,
        visible_mask: 1,
    };
    out
}

unsafe fn create_resource(
    dev: *mut Obj,
    heap: HeapProperties,
    desc: &ResourceDesc,
) -> Result<*mut Obj, i32> {
    if !(DIM_BUFFER..=DIM_TEX3D).contains(&desc.dimension) || desc.width == 0 {
        return Err(E_INVALIDARG);
    }
    let desc = normalized(desc);
    let layouts = sub_layouts(&desc).ok_or(E_INVALIDARG)?;
    let size = resource_size(&desc).ok_or(E_INVALIDARG)?;
    if size > (4u64 << 30) {
        return Err(E_OUTOFMEMORY);
    }
    let len = size as usize;
    let mut v: Vec<u8> = Vec::new();
    if v.try_reserve_exact(len.max(1)).is_err() {
        return Err(E_OUTOFMEMORY);
    }
    v.resize(len.max(1), 0);
    let cpu = Box::into_raw(v.into_boxed_slice()) as *mut u8;
    let va = if desc.dimension == DIM_BUFFER {
        va_alloc(size)
    } else {
        0
    };
    let gpu = if desc.dimension == DIM_TEX2D && desc.flags & FLAG_RENDER_TARGET != 0 {
        device_of(dev).and_then(|d| create_image(d, &desc))
    } else {
        None
    };
    let res = Res {
        desc,
        heap,
        va,
        cpu,
        len: len.max(1),
        layouts,
        data: Mutex::new(ResData {
            gpu,
            cpu_newer: false,
            gpu_newer: false,
        }),
    };
    let obj = new_obj(resource_vtable(), dev, Kind::Resource(Box::new(res)));
    if va != 0 {
        lock(va_registry()).push((va, size, obj as usize));
    }
    Ok(obj)
}

unsafe extern "win64" fn dev_create_committed(
    this: *mut Obj,
    props: *const HeapProperties,
    _flags: u32,
    desc: *const ResourceDesc,
    _state: u32,
    _clear: *const c_void,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if props.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    match create_resource(this, *props, &*desc) {
        Ok(o) => hand_out(o, iid, out),
        Err(e) => e,
    }
}

unsafe extern "win64" fn dev_create_heap(
    this: *mut Obj,
    desc: *const u8,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let size = std::ptr::read_unaligned(desc as *const u64);
    let props = std::ptr::read_unaligned(desc.add(8) as *const HeapProperties);
    hand_out(
        new_obj(heap_vtable(), this, Kind::Heap(size, props)),
        iid,
        out,
    )
}

/// Ressource « placed » : mémoire propre (le recouvrement d'alias entre
/// ressources d'un même tas n'est pas émulé).
unsafe extern "win64" fn dev_create_placed(
    this: *mut Obj,
    heap: *mut Obj,
    _offset: u64,
    desc: *const ResourceDesc,
    _state: u32,
    _clear: *const c_void,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if heap.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    let props = match &(*heap).kind {
        Kind::Heap(_, p) => *p,
        _ => return E_INVALIDARG,
    };
    match create_resource(this, props, &*desc) {
        Ok(o) => hand_out(o, iid, out),
        Err(e) => e,
    }
}

unsafe extern "win64" fn dev_create_fence(
    this: *mut Obj,
    initial: u64,
    _flags: u32,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let f = Fence {
        value: AtomicU64::new(initial),
        waiters: Mutex::new(Vec::new()),
    };
    hand_out(
        new_obj(fence_vtable(), this, Kind::Fence(Box::new(f))),
        iid,
        out,
    )
}

unsafe extern "win64" fn dev_footprints(
    _: *mut Obj,
    desc: *const ResourceDesc,
    first: u32,
    num: u32,
    base: u64,
    layouts: *mut PlacedFootprint,
    num_rows: *mut u32,
    row_sizes: *mut u64,
    total: *mut u64,
) {
    if desc.is_null() {
        return;
    }
    let d = normalized(&*desc);
    let Some(subs) = sub_layouts(&d) else {
        if !total.is_null() {
            *total = u64::MAX;
        }
        return;
    };
    let mut cur = base;
    let mut sum = 0u64;
    for i in 0..num as usize {
        let Some(l) = subs.get(first as usize + i) else {
            break;
        };
        let pitch = if d.dimension == DIM_BUFFER {
            align(d.width, PITCH_ALIGN)
        } else {
            align(l.row_bytes as u64, PITCH_ALIGN)
        };
        let off = if i == 0 {
            cur
        } else {
            align(cur, PLACEMENT_ALIGN)
        };
        if !layouts.is_null() {
            *layouts.add(i) = PlacedFootprint {
                offset: off,
                footprint: Footprint {
                    format: d.format,
                    width: l.width,
                    height: l.height,
                    depth: l.depth,
                    row_pitch: pitch as u32,
                },
            };
        }
        if !num_rows.is_null() {
            *num_rows.add(i) = l.rows;
        }
        if !row_sizes.is_null() {
            *row_sizes.add(i) = l.row_bytes as u64;
        }
        let end = off + pitch * (l.rows as u64 * l.depth as u64 - 1) + l.row_bytes as u64;
        sum = end - base;
        cur = end;
    }
    if !total.is_null() {
        *total = sum;
    }
}

unsafe extern "win64" fn dev_create_query_heap(
    this: *mut Obj,
    _desc: *const u8,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    hand_out(
        new_obj(query_heap_vtable(), this, Kind::QueryHeap),
        iid,
        out,
    )
}

unsafe extern "win64" fn dev_create_cmd_sig(
    this: *mut Obj,
    _desc: *const u8,
    _rs: *mut Obj,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    hand_out(new_obj(cmd_sig_vtable(), this, Kind::CmdSig), iid, out)
}

unsafe extern "win64" fn dev_luid(_: *mut Obj, out: *mut u64) -> *mut u64 {
    *out = LUID;
    out
}

/// Toutes les exécutions sont synchrones : il suffit de regarder les valeurs.
unsafe extern "win64" fn dev_event_on_multiple(
    _: *mut Obj,
    fences: *const *mut Obj,
    values: *const u64,
    n: u32,
    flags: u32,
    event: usize,
) -> i32 {
    if fences.is_null() || values.is_null() {
        return E_INVALIDARG;
    }
    let mut all = true;
    let mut any = false;
    for i in 0..n as usize {
        let f = *fences.add(i);
        let done = match (!f.is_null()).then(|| &(*f).kind) {
            Some(Kind::Fence(fe)) => fe.value.load(Ordering::Acquire) >= *values.add(i),
            _ => false,
        };
        all &= done;
        any |= done;
    }
    let ready = if flags & 1 != 0 { any } else { all };
    if ready && event != 0 {
        crate::dlls::kernel32::set_event(event);
    }
    S_OK
}

// ── File de commandes ──────────────────────────────────────────────────────

unsafe extern "win64" fn queue_execute(this: *mut Obj, n: u32, lists: *const *mut Obj) {
    if lists.is_null() {
        return;
    }
    let Some(dev) = device_of(this) else { return };
    for i in 0..n as usize {
        let l = *lists.add(i);
        if l.is_null() {
            continue;
        }
        let Kind::List(m) = &(*l).kind else { continue };
        let list = lock(m);
        if list.open {
            log_warn!("d3d12", "ExecuteCommandLists : liste non fermée, ignorée");
            continue;
        }
        for c in &list.cmds {
            execute(dev, c);
        }
    }
}

unsafe fn fence_signal(fence: *mut Obj, value: u64) -> i32 {
    if fence.is_null() {
        return E_INVALIDARG;
    }
    let Kind::Fence(f) = &(*fence).kind else {
        return E_INVALIDARG;
    };
    f.value.store(value, Ordering::Release);
    let ready: Vec<usize> = {
        let mut w = lock(&f.waiters);
        let (done, keep): (Vec<_>, Vec<_>) = w.drain(..).partition(|(v, _)| *v <= value);
        *w = keep;
        done.into_iter().map(|(_, h)| h).collect()
    };
    for h in ready {
        crate::dlls::kernel32::set_event(h);
    }
    S_OK
}

unsafe extern "win64" fn queue_signal(_: *mut Obj, fence: *mut Obj, value: u64) -> i32 {
    fence_signal(fence, value)
}
unsafe extern "win64" fn queue_wait(_: *mut Obj, _fence: *mut Obj, _value: u64) -> i32 {
    S_OK
}
unsafe extern "win64" fn queue_ts_freq(_: *mut Obj, out: *mut u64) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 1_000_000_000;
    S_OK
}
fn now_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}
unsafe extern "win64" fn queue_clock(_: *mut Obj, gpu: *mut u64, cpu: *mut u64) -> i32 {
    let t = now_ns();
    if !gpu.is_null() {
        *gpu = t;
    }
    if !cpu.is_null() {
        *cpu = t;
    }
    S_OK
}
unsafe extern "win64" fn queue_desc(this: *mut Obj, out: *mut QueueDesc) -> *mut QueueDesc {
    if let Kind::Queue(d) = &(*this).kind {
        *out = *d;
    }
    out
}

pub(crate) unsafe fn is_queue(p: *mut c_void) -> bool {
    !p.is_null() && *(p as *const *const usize) == queue_vtable().as_ptr()
}

// ── Barrière de clôture ────────────────────────────────────────────────────

unsafe extern "win64" fn fence_completed(this: *mut Obj) -> u64 {
    match &(*this).kind {
        Kind::Fence(f) => f.value.load(Ordering::Acquire),
        _ => 0,
    }
}

unsafe extern "win64" fn fence_event(this: *mut Obj, value: u64, event: usize) -> i32 {
    let Kind::Fence(f) = &(*this).kind else {
        return E_INVALIDARG;
    };
    if f.value.load(Ordering::Acquire) >= value {
        if event != 0 {
            crate::dlls::kernel32::set_event(event);
        }
        return S_OK;
    }
    if event == 0 {
        // Attente bloquante demandée : rien n'avance plus tard côté GPU
        // (tout est synchrone), seul un Signal CPU d'un autre fil le peut.
        for _ in 0..2000 {
            if f.value.load(Ordering::Acquire) >= value {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        return S_OK;
    }
    lock(&f.waiters).push((value, event));
    S_OK
}

unsafe extern "win64" fn fence_cpu_signal(this: *mut Obj, value: u64) -> i32 {
    fence_signal(this, value)
}

// ── Tas de descripteurs ────────────────────────────────────────────────────

unsafe extern "win64" fn heap_desc(this: *mut Obj, out: *mut HeapDesc) -> *mut HeapDesc {
    if let Kind::DescHeap(h) = &(*this).kind {
        *out = h.desc;
    }
    out
}
unsafe extern "win64" fn heap_cpu_start(this: *mut Obj, out: *mut usize) -> *mut usize {
    *out = match &(*this).kind {
        Kind::DescHeap(h) => h.mem.as_ptr() as usize,
        _ => 0,
    };
    out
}
unsafe extern "win64" fn heap_gpu_start(this: *mut Obj, out: *mut u64) -> *mut u64 {
    *out = match &(*this).kind {
        // Seuls les tas visibles des shaders ont une adresse GPU.
        Kind::DescHeap(h) if h.desc.flags & 1 != 0 => h.gpu_base,
        _ => 0,
    };
    out
}
unsafe extern "win64" fn memheap_desc(this: *mut Obj, out: *mut u8) -> *mut u8 {
    if let Kind::Heap(size, props) = &(*this).kind {
        std::ptr::write_bytes(out, 0, 48);
        std::ptr::write_unaligned(out as *mut u64, *size);
        std::ptr::write_unaligned(out.add(8) as *mut HeapProperties, *props);
        std::ptr::write_unaligned(out.add(32) as *mut u64, 0x1_0000);
    }
    out
}

// ── Ressource ──────────────────────────────────────────────────────────────

unsafe extern "win64" fn res_map(
    this: *mut Obj,
    sub: u32,
    _range: *const usize,
    out: *mut *mut u8,
) -> i32 {
    let Some(r) = res_of(this) else {
        return E_INVALIDARG;
    };
    let Some(l) = r.layouts.get(sub as usize) else {
        return E_INVALIDARG;
    };
    if let Some(d) = device_of(this) {
        ensure_cpu(d, r);
    }
    if !out.is_null() {
        *out = r.cpu.add(l.offset as usize);
    }
    S_OK
}
unsafe extern "win64" fn res_unmap(this: *mut Obj, _sub: u32, written: *const [usize; 2]) {
    let Some(r) = res_of(this) else { return };
    let wrote = written.is_null() || (*written)[1] > (*written)[0];
    let mut d = lock(&r.data);
    if wrote && d.gpu.is_some() {
        d.cpu_newer = true;
        d.gpu_newer = false;
    }
}
unsafe extern "win64" fn res_desc(this: *mut Obj, out: *mut ResourceDesc) -> *mut ResourceDesc {
    if let Some(r) = res_of(this) {
        *out = r.desc;
    }
    out
}
unsafe extern "win64" fn res_va(this: *mut Obj) -> u64 {
    res_of(this).map(|r| r.va).unwrap_or(0)
}
unsafe extern "win64" fn res_heap_props(
    this: *mut Obj,
    props: *mut HeapProperties,
    flags: *mut u32,
) -> i32 {
    let Some(r) = res_of(this) else {
        return E_INVALIDARG;
    };
    if !props.is_null() {
        *props = r.heap;
    }
    if !flags.is_null() {
        *flags = 0;
    }
    S_OK
}

/// Copie entre la sous-ressource `sub` et une mémoire de l'application.
unsafe fn sub_copy(
    this: *mut Obj,
    sub: u32,
    bx: *const [u32; 6],
    app: *mut u8,
    row: u32,
    slice: u32,
    write: bool,
) -> i32 {
    let Some(r) = res_of(this) else {
        return E_INVALIDARG;
    };
    let Some(l) = r.layouts.get(sub as usize).copied() else {
        return E_INVALIDARG;
    };
    if app.is_null() {
        return E_INVALIDARG;
    }
    let dim = fmt_block(r.desc.format).map(|b| b.1).unwrap_or(1);
    let bpb = if r.desc.dimension == DIM_BUFFER {
        1
    } else {
        l.row_bytes / blocks(l.width, dim)
    };
    let b = if bx.is_null() {
        [0, 0, 0, l.width, l.height, l.depth]
    } else {
        *bx
    };
    if b[3] <= b[0]
        || b[4] <= b[1]
        || b[5] <= b[2]
        || b[3] > l.width
        || b[4] > l.height
        || b[5] > l.depth
    {
        return E_INVALIDARG;
    }
    if let Some(d) = device_of(this) {
        ensure_cpu(d, r);
    }
    let bytes = r.bytes();
    let w = blocks(b[3] - b[0], dim) * bpb;
    let rows = blocks(b[4] - b[1], dim);
    for z in 0..(b[5] - b[2]) {
        for y in 0..rows {
            let off = l.offset as usize
                + ((b[2] + z) as usize * l.rows as usize + (b[1] / dim + y) as usize)
                    * l.row_bytes as usize
                + (b[0] / dim * bpb) as usize;
            let a = app.add(z as usize * slice as usize + y as usize * row as usize);
            let Some(cell) = bytes.get_mut(off..off + w as usize) else {
                return E_INVALIDARG;
            };
            if write {
                std::ptr::copy_nonoverlapping(a, cell.as_mut_ptr(), w as usize);
            } else {
                std::ptr::copy_nonoverlapping(cell.as_ptr(), a, w as usize);
            }
        }
    }
    if write {
        mark_cpu(r);
    }
    S_OK
}

unsafe extern "win64" fn res_write(
    this: *mut Obj,
    sub: u32,
    bx: *const [u32; 6],
    src: *const u8,
    row: u32,
    slice: u32,
) -> i32 {
    sub_copy(this, sub, bx, src as *mut u8, row, slice, true)
}
unsafe extern "win64" fn res_read(
    this: *mut Obj,
    dst: *mut u8,
    row: u32,
    slice: u32,
    sub: u32,
    bx: *const [u32; 6],
) -> i32 {
    sub_copy(this, sub, bx, dst, row, slice, false)
}

fn mark_cpu(r: &Res) {
    let mut d = lock(&r.data);
    if d.gpu.is_some() {
        d.cpu_newer = true;
        d.gpu_newer = false;
    }
}

// ── Liste de commandes graphiques ──────────────────────────────────────────

unsafe fn list_of<'a>(this: *mut Obj) -> Option<MutexGuard<'a, List>> {
    match &(*this).kind {
        Kind::List(m) => Some(lock(&*(m.as_ref() as *const Mutex<List>))),
        _ => None,
    }
}

unsafe fn record(this: *mut Obj, c: Cmd) {
    if let Some(mut l) = list_of(this) {
        if l.open {
            l.cmds.push(c);
        }
    }
}

unsafe extern "win64" fn list_type(this: *mut Obj) -> u32 {
    list_of(this).map(|l| l.ty).unwrap_or(0)
}
unsafe extern "win64" fn list_close(this: *mut Obj) -> i32 {
    match list_of(this) {
        Some(mut l) if l.open => {
            l.open = false;
            S_OK
        }
        _ => E_FAIL,
    }
}
unsafe extern "win64" fn list_reset(this: *mut Obj, _alloc: *mut Obj, pso: *mut Obj) -> i32 {
    let Some(mut l) = list_of(this) else {
        return E_INVALIDARG;
    };
    l.cmds.clear();
    l.open = true;
    l.st = ListState {
        pso: pso as usize,
        ..Default::default()
    };
    S_OK
}
unsafe extern "win64" fn list_clear_state(this: *mut Obj, pso: *mut Obj) {
    if let Some(mut l) = list_of(this) {
        l.st = ListState {
            pso: pso as usize,
            ..Default::default()
        };
    }
}

unsafe fn push_draw(
    this: *mut Obj,
    indexed: bool,
    count: u32,
    instances: u32,
    first: u32,
    base: i32,
    first_instance: u32,
) {
    let Some(mut l) = list_of(this) else { return };
    if !l.open || l.st.rt == 0 || count == 0 || instances == 0 {
        return;
    }
    let st = l.st;
    let Some(vb) = st.vb else { return };
    if indexed && st.ib.is_none() {
        return;
    }
    let pso = st.pso as *mut Obj;
    let layout = match (!pso.is_null()).then(|| &(*pso).kind) {
        Some(Kind::Pso(p)) if !p.compute => p.layout.clone(),
        _ => return,
    };
    let target = st.rt as *mut Obj;
    let Some(r) = res_of(target) else { return };
    let viewport = st.viewport.unwrap_or([
        0.0,
        0.0,
        r.desc.width as f32,
        r.desc.height as f32,
        0.0,
        1.0,
    ]);
    l.cmds.push(Cmd::Draw(Box::new(DrawCall {
        target,
        viewport,
        topology: st.topology,
        layout,
        vb,
        ib: if indexed { st.ib } else { None },
        count,
        instances,
        first,
        base,
        first_instance,
    })));
}

unsafe extern "win64" fn list_draw(
    this: *mut Obj,
    count: u32,
    inst: u32,
    start: u32,
    start_inst: u32,
) {
    push_draw(this, false, count, inst, start, 0, start_inst);
}
unsafe extern "win64" fn list_draw_indexed(
    this: *mut Obj,
    count: u32,
    inst: u32,
    start: u32,
    base: i32,
    start_inst: u32,
) {
    push_draw(this, true, count, inst, start, base, start_inst);
}
unsafe extern "win64" fn list_copy_buffer(
    this: *mut Obj,
    dst: *mut Obj,
    doff: u64,
    src: *mut Obj,
    soff: u64,
    n: u64,
) {
    record(this, Cmd::CopyBuffer(dst, doff, src, soff, n));
}

unsafe fn parse_loc(l: *const CopyLocation) -> Option<Loc> {
    if l.is_null() || (*l).resource.is_null() {
        return None;
    }
    Some(match (*l).ty {
        0 => Loc::Sub(
            (*l).resource,
            *(&(*l).placed as *const PlacedFootprint as *const u32),
        ),
        _ => Loc::Placed((*l).resource, (*l).placed),
    })
}

unsafe extern "win64" fn list_copy_texture(
    this: *mut Obj,
    dst: *const CopyLocation,
    x: u32,
    y: u32,
    z: u32,
    src: *const CopyLocation,
    bx: *const [u32; 6],
) {
    let (Some(d), Some(s)) = (parse_loc(dst), parse_loc(src)) else {
        return;
    };
    record(
        this,
        Cmd::CopyTexture(d, [x, y, z], s, (!bx.is_null()).then(|| *bx)),
    );
}
unsafe extern "win64" fn list_copy_resource(this: *mut Obj, dst: *mut Obj, src: *mut Obj) {
    record(this, Cmd::CopyResource(dst, src));
}
unsafe extern "win64" fn list_resolve(
    this: *mut Obj,
    dst: *mut Obj,
    _ds: u32,
    src: *mut Obj,
    _ss: u32,
    _f: u32,
) {
    record(this, Cmd::CopyResource(dst, src));
}
unsafe extern "win64" fn list_topology(this: *mut Obj, t: u32) {
    if let Some(mut l) = list_of(this) {
        l.st.topology = t;
    }
}
unsafe extern "win64" fn list_viewports(this: *mut Obj, n: u32, vp: *const [f32; 6]) {
    if let Some(mut l) = list_of(this) {
        l.st.viewport = (n > 0 && !vp.is_null()).then(|| *vp);
    }
}
unsafe extern "win64" fn list_set_pso(this: *mut Obj, pso: *mut Obj) {
    if let Some(mut l) = list_of(this) {
        l.st.pso = pso as usize;
    }
}
unsafe extern "win64" fn list_barrier(this: *mut Obj, n: u32, _b: *const Barrier) {
    if n > 0 {
        record(this, Cmd::Barrier);
    }
}
unsafe extern "win64" fn list_ib(this: *mut Obj, view: *const IndexBufferView) {
    if let Some(mut l) = list_of(this) {
        l.st.ib = (!view.is_null()).then(|| ((*view).location, (*view).size, (*view).format));
    }
}
unsafe extern "win64" fn list_vbs(
    this: *mut Obj,
    start: u32,
    n: u32,
    views: *const VertexBufferView,
) {
    if let Some(mut l) = list_of(this) {
        if start == 0 && n > 0 {
            l.st.vb =
                (!views.is_null()).then(|| ((*views).location, (*views).size, (*views).stride));
        }
    }
}
unsafe extern "win64" fn list_om_rts(
    this: *mut Obj,
    n: u32,
    handles: *const usize,
    _single: i32,
    dsv: *const usize,
) {
    if let Some(mut l) = list_of(this) {
        l.st.rt = if n > 0 && !handles.is_null() {
            descriptor_resource(*handles) as usize
        } else {
            0
        };
        l.st.dsv = if dsv.is_null() {
            0
        } else {
            descriptor_resource(*dsv) as usize
        };
    }
}
unsafe fn rects(n: u32, r: *const [i32; 4]) -> Vec<[i32; 4]> {
    if r.is_null() {
        return Vec::new();
    }
    (0..n.min(256) as usize).map(|i| *r.add(i)).collect()
}
unsafe extern "win64" fn list_clear_dsv(
    this: *mut Obj,
    handle: usize,
    flags: u32,
    depth: f32,
    stencil: u8,
    _n: u32,
    _r: *const [i32; 4],
) {
    let res = descriptor_resource(handle);
    if !res.is_null() {
        record(this, Cmd::ClearDs(res, depth, stencil, flags));
    }
}
unsafe extern "win64" fn list_clear_rtv(
    this: *mut Obj,
    handle: usize,
    color: *const [f32; 4],
    n: u32,
    r: *const [i32; 4],
) {
    let res = descriptor_resource(handle);
    if res.is_null() || color.is_null() {
        return;
    }
    record(this, Cmd::ClearRt(res, *color, rects(n, r)));
}

// ── Exécution ──────────────────────────────────────────────────────────────

fn warn_once(flag: &AtomicU32, msg: &str) {
    if flag.swap(1, Ordering::Relaxed) == 0 {
        log_warn!("d3d12", "{}", msg);
    }
}

unsafe fn execute(dev: &Device, c: &Cmd) {
    match c {
        Cmd::ClearRt(res, color, r) => clear_rt(dev, *res, *color, r),
        Cmd::ClearDs(res, depth, stencil, flags) => clear_ds(dev, *res, *depth, *stencil, *flags),
        Cmd::CopyBuffer(dst, doff, src, soff, n) => {
            let (Some(d), Some(s)) = (res_of(*dst), res_of(*src)) else {
                return;
            };
            ensure_cpu(dev, s);
            let (doff, soff, n) = (*doff as usize, *soff as usize, *n as usize);
            if soff.saturating_add(n) <= s.len && doff.saturating_add(n) <= d.len {
                std::ptr::copy(s.cpu.add(soff), d.cpu.add(doff), n);
                mark_cpu(d);
            }
        }
        Cmd::CopyResource(dst, src) => {
            let (Some(d), Some(s)) = (res_of(*dst), res_of(*src)) else {
                return;
            };
            if std::ptr::eq(d, s) {
                return;
            }
            ensure_cpu(dev, s);
            std::ptr::copy_nonoverlapping(s.cpu, d.cpu, s.len.min(d.len));
            mark_cpu(d);
        }
        Cmd::CopyTexture(d, xyz, s, bx) => copy_texture(dev, *d, *xyz, *s, *bx),
        Cmd::Draw(dc) => draw(dev, dc),
        // Chaque soumission Vulkan attend sa barrière de clôture et GL
        // ordonne ses commandes : rien à faire de plus.
        Cmd::Barrier => {}
    }
}

/// Vue CPU d'un emplacement de copie.
struct View {
    r: &'static Res,
    base: usize,
    row: usize,
    slice: usize,
    width: u32,
    height: u32,
    depth: u32,
    bpb: u32,
    dim: u32,
}

unsafe fn loc_view(l: Loc) -> Option<View> {
    match l {
        Loc::Sub(o, s) => {
            let r = res_of(o)?;
            let lay = r.layouts.get(s as usize)?;
            let (bpb, dim) = fmt_block(r.desc.format)?;
            Some(View {
                r,
                base: lay.offset as usize,
                row: lay.row_bytes as usize,
                slice: lay.row_bytes as usize * lay.rows as usize,
                width: lay.width,
                height: lay.height,
                depth: lay.depth,
                bpb,
                dim,
            })
        }
        Loc::Placed(o, p) => {
            let r = res_of(o)?;
            let f = p.footprint;
            let (bpb, dim) = fmt_block(f.format)?;
            Some(View {
                r,
                base: p.offset as usize,
                row: f.row_pitch as usize,
                slice: f.row_pitch as usize * blocks(f.height, dim) as usize,
                width: f.width,
                height: f.height,
                depth: f.depth.max(1),
                bpb,
                dim,
            })
        }
    }
}

unsafe fn copy_texture(dev: &Device, d: Loc, xyz: [u32; 3], s: Loc, bx: Option<[u32; 6]>) {
    let (Some(dv), Some(sv)) = (loc_view(d), loc_view(s)) else {
        return;
    };
    ensure_cpu(dev, sv.r);
    ensure_cpu(dev, dv.r);
    let mut b = bx.unwrap_or([0, 0, 0, sv.width, sv.height, sv.depth]);
    b[3] = b[3].min(sv.width);
    b[4] = b[4].min(sv.height);
    b[5] = b[5].min(sv.depth);
    if b[3] <= b[0] || b[4] <= b[1] || b[5] <= b[2] {
        return;
    }
    let dim = sv.dim;
    let cols = blocks(b[3] - b[0], dim) as usize;
    let rows = blocks(b[4] - b[1], dim) as usize;
    let n = cols * sv.bpb as usize;
    for z in 0..(b[5] - b[2]) as usize {
        for y in 0..rows {
            let so = sv.base
                + (b[2] as usize + z) * sv.slice
                + (b[1] / dim) as usize * sv.row
                + y * sv.row
                + (b[0] / dim) as usize * sv.bpb as usize;
            let dz = xyz[2] as usize + z;
            let dy = (xyz[1] / dv.dim) as usize + y;
            let dx = (xyz[0] / dv.dim) as usize * dv.bpb as usize;
            if dz >= dv.depth as usize || dy >= blocks(dv.height, dv.dim) as usize {
                continue;
            }
            let dofs = dv.base + dz * dv.slice + dy * dv.row + dx;
            if so + n <= sv.r.len && dofs + n <= dv.r.len {
                std::ptr::copy(sv.r.cpu.add(so), dv.r.cpu.add(dofs), n);
            }
        }
    }
    mark_cpu(dv.r);
}

unsafe fn clear_rt(dev: &Device, res: *mut Obj, color: [f32; 4], rects: &[[i32; 4]]) {
    let Some(r) = res_of(res) else { return };
    if rects.is_empty() {
        let mut d = lock(&r.data);
        let done = match (&dev.gpu, d.gpu.as_ref()) {
            (Gpu::Vk(ctx), Some(GpuImage::Vk(img))) => ctx.submit(|cmd| {
                ctx.clear_color(cmd, img, color);
                ctx.memory_barrier(cmd);
            }),
            (Gpu::Gl(m), Some(GpuImage::Gl { fbo, .. })) => {
                let mut b = lock(m);
                b.current() && gl_clear(*fbo, color)
            }
            _ => false,
        };
        if done {
            d.gpu_newer = true;
            d.cpu_newer = false;
            return;
        }
    }
    static WARN: AtomicU32 = AtomicU32::new(0);
    let Some(px) = encode_color(r.desc.format, color) else {
        warn_once(
            &WARN,
            "effacement : format de cible non géré côté processeur",
        );
        return;
    };
    ensure_cpu(dev, r);
    let l = r.layouts[0];
    let full = [[0, 0, l.width as i32, l.height as i32]];
    let list = if rects.is_empty() { &full[..] } else { rects };
    let bytes = r.bytes();
    for rc in list {
        let x0 = rc[0].clamp(0, l.width as i32) as usize;
        let x1 = rc[2].clamp(0, l.width as i32) as usize;
        let y0 = rc[1].clamp(0, l.height as i32) as usize;
        let y1 = rc[3].clamp(0, l.height as i32) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let o = l.offset as usize + y * l.row_bytes as usize + x * px.len();
                if let Some(cell) = bytes.get_mut(o..o + px.len()) {
                    cell.copy_from_slice(&px);
                }
            }
        }
    }
    mark_cpu(r);
}

unsafe fn clear_ds(dev: &Device, res: *mut Obj, depth: f32, stencil: u8, flags: u32) {
    let Some(r) = res_of(res) else { return };
    ensure_cpu(dev, r);
    let l = r.layouts[0];
    let n = l.width as usize * l.height as usize;
    let bytes = r.bytes();
    match r.desc.format {
        40 if flags & 1 != 0 => {
            for i in 0..n {
                if let Some(c) =
                    bytes.get_mut(l.offset as usize + i * 4..l.offset as usize + i * 4 + 4)
                {
                    c.copy_from_slice(&depth.to_le_bytes());
                }
            }
        }
        44 | 45 => {
            let dq = (depth.clamp(0.0, 1.0) * 16_777_215.0 + 0.5) as u32;
            for i in 0..n {
                let o = l.offset as usize + i * 4;
                let Some(c) = bytes.get_mut(o..o + 4) else {
                    break;
                };
                let mut v = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                if flags & 1 != 0 {
                    v = (v & 0xff00_0000) | dq;
                }
                if flags & 2 != 0 {
                    v = (v & 0x00ff_ffff) | ((stencil as u32) << 24);
                }
                c.copy_from_slice(&v.to_le_bytes());
            }
        }
        _ => {}
    }
}

/// Rapatrie l'image GPU dans la copie CPU si elle est plus récente.
unsafe fn ensure_cpu(dev: &Device, r: &Res) {
    let mut d = lock(&r.data);
    if !d.gpu_newer {
        return;
    }
    let l = r.layouts[0];
    let size = l.row_bytes as usize * l.rows as usize;
    let ok = match (&dev.gpu, d.gpu.as_ref()) {
        (Gpu::Vk(ctx), Some(GpuImage::Vk(img))) => (|| {
            let st = ctx.create_buffer(size as u64, true)?;
            let ok = ctx.submit(|cmd| {
                ctx.memory_barrier(cmd);
                ctx.copy_image_to_buffer(cmd, img, st.buffer, 0, l.width);
            });
            if ok {
                std::ptr::copy_nonoverlapping(
                    st.mapped,
                    r.cpu.add(l.offset as usize),
                    size.min(r.len),
                );
            }
            ctx.destroy_buffer(&st);
            Some(ok)
        })()
        .unwrap_or(false),
        (
            Gpu::Gl(m),
            Some(GpuImage::Gl {
                fbo, format, ty, ..
            }),
        ) => {
            let mut b = lock(m);
            b.current()
                && gl_read(
                    *fbo,
                    l.width,
                    l.height,
                    *format,
                    *ty,
                    r.cpu.add(l.offset as usize),
                )
        }
        _ => false,
    };
    if ok {
        d.gpu_newer = false;
    }
}

/// Envoie la copie CPU vers l'image GPU si elle est plus récente.
unsafe fn ensure_gpu(dev: &Device, r: &Res) {
    let mut d = lock(&r.data);
    if !d.cpu_newer {
        return;
    }
    let l = r.layouts[0];
    let size = l.row_bytes as usize * l.rows as usize;
    let ok = match (&dev.gpu, d.gpu.as_ref()) {
        (Gpu::Vk(ctx), Some(GpuImage::Vk(img))) => (|| {
            let st = ctx.create_buffer(size as u64, true)?;
            std::ptr::copy_nonoverlapping(r.cpu.add(l.offset as usize), st.mapped, size.min(r.len));
            let ok = ctx.submit(|cmd| {
                ctx.copy_buffer_to_image(cmd, st.buffer, 0, l.width, img);
                ctx.memory_barrier(cmd);
            });
            ctx.destroy_buffer(&st);
            Some(ok)
        })()
        .unwrap_or(false),
        (
            Gpu::Gl(m),
            Some(GpuImage::Gl {
                tex, format, ty, ..
            }),
        ) => {
            let mut b = lock(m);
            b.current()
                && gl_upload(
                    *tex,
                    l.width,
                    l.height,
                    *format,
                    *ty,
                    r.cpu.add(l.offset as usize),
                )
        }
        _ => false,
    };
    if ok {
        d.cpu_newer = false;
    }
}

unsafe fn bounded(addr: u64, size: u32) -> Option<&'static [u8]> {
    let (o, off) = va_lookup(addr)?;
    let r = res_of(o)?;
    let start = off as usize;
    let end = (start + size as usize).min(r.len);
    (start < end).then(|| std::slice::from_raw_parts(r.cpu.add(start), end - start))
}

unsafe fn draw(dev: &Device, dc: &DrawCall) {
    static WARN: AtomicU32 = AtomicU32::new(0);
    let Some(t) = res_of(dc.target) else { return };
    let Some(vb) = bounded(dc.vb.0, dc.vb.1) else {
        return;
    };
    let ib = match dc.ib {
        Some((a, s, f)) => match bounded(a, s) {
            Some(b) => Some((b, f)),
            None => return,
        },
        None => None,
    };
    if !dc.layout.iter().any(|a| a.0 == 0) {
        return;
    }
    ensure_gpu(dev, t);
    let mut d = lock(&t.data);
    let done = match (&dev.gpu, d.gpu.as_ref()) {
        (Gpu::Vk(ctx), Some(GpuImage::Vk(img))) => vk_draw(ctx, img, dc, vb, ib),
        (Gpu::Gl(m), Some(GpuImage::Gl { fbo, .. })) => {
            let mut b = lock(m);
            b.current() && b.draw(*fbo, dc, vb, ib)
        }
        _ => {
            warn_once(
                &WARN,
                "dessin ignoré : aucun GPU utilisable (mode processeur)",
            );
            false
        }
    };
    if done {
        d.gpu_newer = true;
        d.cpu_newer = false;
    }
}

fn vk_draw(
    ctx: &VkContext,
    img: &VkImageAlloc,
    dc: &DrawCall,
    vb: &[u8],
    ib: Option<(&[u8], u32)>,
) -> bool {
    static WARN: AtomicU32 = AtomicU32::new(0);
    let topology = match dc.topology {
        4 => VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
        5 => VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP,
        _ => {
            warn_once(
                &WARN,
                "Vulkan : seules les topologies triangle sont dessinées",
            );
            return false;
        }
    };
    let attributes: Vec<(u32, u32, u32)> = dc
        .layout
        .iter()
        .map(|&(l, f, o)| (l, dxgi_to_vk(f), o))
        .filter(|a| a.1 != VK_FORMAT_UNDEFINED)
        .collect();
    if !attributes.iter().any(|a| a.0 == 0) {
        return false;
    }
    let key = PipelineKey {
        color_format: img.format,
        with_color: attributes.iter().any(|a| a.0 == 1),
        attributes,
    };
    let Some(pipe) = ctx.pipeline(&key) else {
        return false;
    };
    let Some(vbuf) = ctx.create_buffer(vb.len() as u64, true) else {
        return false;
    };
    unsafe { std::ptr::copy_nonoverlapping(vb.as_ptr(), vbuf.mapped, vb.len()) };
    let ibuf = ib.and_then(|(b, f)| {
        let buf = ctx.create_buffer(b.len() as u64, true)?;
        unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), buf.mapped, b.len()) };
        Some((
            buf,
            if f == 42 {
                VK_INDEX_TYPE_UINT32
            } else {
                VK_INDEX_TYPE_UINT16
            },
        ))
    });
    if ib.is_some() && ibuf.is_none() {
        ctx.destroy_buffer(&vbuf);
        return false;
    }
    let vp = dc.viewport;
    // Hauteur négative : l'axe Y de Direct3D (vers le haut) est conservé.
    let viewport = VkViewport {
        x: vp[0],
        y: vp[1] + vp[3],
        width: vp[2],
        height: -vp[3],
        minDepth: vp[4],
        maxDepth: vp[5],
    };
    let ok = ctx.submit(|cmd| {
        ctx.draw(
            cmd,
            img,
            pipe,
            topology,
            viewport,
            (vbuf.buffer, 0, dc.vb.2 as u64),
            ibuf.as_ref().map(|(b, t)| (b.buffer, 0, *t)),
            (dc.count, dc.instances, dc.first, dc.base, dc.first_instance),
        );
        ctx.memory_barrier(cmd);
    });
    ctx.destroy_buffer(&vbuf);
    if let Some((b, _)) = ibuf {
        ctx.destroy_buffer(&b);
    }
    ok
}

unsafe fn create_image(dev: &Device, desc: &ResourceDesc) -> Option<GpuImage> {
    let (w, h) = (desc.width as u32, desc.height);
    match &dev.gpu {
        Gpu::Vk(ctx) => {
            let f = dxgi_to_vk(desc.format);
            if f == VK_FORMAT_UNDEFINED {
                return None;
            }
            ctx.create_image(w, h, f).map(GpuImage::Vk)
        }
        Gpu::Gl(m) => {
            let (internal, format, ty) = gl_format(desc.format)?;
            let mut b = lock(m);
            if !b.current() {
                return None;
            }
            let g = gl::gl()?;
            let (mut tex, mut fbo) = (0, 0);
            (g.GenTextures)(1, &mut tex);
            (g.BindTexture)(gl::GL_TEXTURE_2D, tex);
            (g.TexImage2D)(
                gl::GL_TEXTURE_2D,
                0,
                internal as i32,
                w as i32,
                h as i32,
                0,
                format,
                ty,
                std::ptr::null(),
            );
            (g.TexParameteri)(gl::GL_TEXTURE_2D, gl::GL_TEXTURE_MIN_FILTER, gl::GL_NEAREST);
            (g.GenFramebuffers)(1, &mut fbo);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
            (g.FramebufferTexture2D)(
                gl::GL_FRAMEBUFFER,
                gl::GL_COLOR_ATTACHMENT0,
                gl::GL_TEXTURE_2D,
                tex,
                0,
            );
            let complete =
                (g.CheckFramebufferStatus)(gl::GL_FRAMEBUFFER) == gl::GL_FRAMEBUFFER_COMPLETE;
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
            if !complete {
                (g.DeleteFramebuffers)(1, &fbo);
                (g.DeleteTextures)(1, &tex);
                return None;
            }
            Some(GpuImage::Gl {
                tex,
                fbo,
                format,
                ty,
            })
        }
        Gpu::Cpu => None,
    }
}

unsafe fn free_image(dev_obj: *mut Obj, img: GpuImage) {
    let Some(dev) = device_of(dev_obj) else {
        return;
    };
    match (&dev.gpu, img) {
        (Gpu::Vk(ctx), GpuImage::Vk(i)) => ctx.destroy_image(&i),
        (Gpu::Gl(m), GpuImage::Gl { tex, fbo, .. }) => {
            let mut b = lock(m);
            if let (true, Some(g)) = (b.current(), gl::gl()) {
                (g.DeleteFramebuffers)(1, &fbo);
                (g.DeleteTextures)(1, &tex);
            }
        }
        _ => {}
    }
}

// ── Backend OpenGL ─────────────────────────────────────────────────────────

fn gl_format(f: u32) -> Option<(u32, u32, u32)> {
    Some(match f {
        27..=32 => (gl::GL_RGBA8, gl::GL_RGBA, gl::GL_UNSIGNED_BYTE),
        87 | 90 | 91 => (gl::GL_RGBA8, gl::GL_BGRA, gl::GL_UNSIGNED_BYTE),
        1 | 2 => (gl::GL_RGBA32F, gl::GL_RGBA, gl::GL_FLOAT),
        9 | 10 => (gl::GL_RGBA16F, gl::GL_RGBA, gl::GL_HALF_FLOAT),
        _ => return None,
    })
}

/// (composantes, type GL, normalisé) d'un attribut de sommet.
fn gl_attr(f: u32) -> Option<(i32, u32, u8)> {
    Some(match f {
        2 => (4, gl::GL_FLOAT, 0),
        6 => (3, gl::GL_FLOAT, 0),
        16 => (2, gl::GL_FLOAT, 0),
        41 => (1, gl::GL_FLOAT, 0),
        28 | 87 => (4, gl::GL_UNSIGNED_BYTE, 1),
        _ => return None,
    })
}

struct GlBackend {
    ctx: GlContext,
    owner: Option<std::thread::ThreadId>,
    drawable: u64,
    programs: [u32; 2],
    vbo: u32,
    ibo: u32,
}

unsafe impl Send for GlBackend {}

const GL_VS: &str = "#version 120\nattribute vec4 a_pos;\nattribute vec4 a_col;\nvarying vec4 v_col;\n\
void main() {\n  vec4 p = a_pos;\n  gl_Position = vec4(p.x, -p.y, 2.0 * p.z - p.w, p.w);\n  v_col = a_col;\n}\n";
const GL_FS_COLOR: &str =
    "#version 120\nvarying vec4 v_col;\nvoid main() { gl_FragColor = v_col; }\n";
const GL_FS_WHITE: &str = "#version 120\nvoid main() { gl_FragColor = vec4(1.0); }\n";

impl GlBackend {
    fn create() -> Option<GlBackend> {
        let ctx = GlContext::create_for_window(0)?;
        let drawable = ctx.drawable;
        Some(GlBackend {
            ctx,
            owner: Some(std::thread::current().id()),
            drawable,
            programs: [0; 2],
            vbo: 0,
            ibo: 0,
        })
    }

    unsafe fn bind(&mut self, drawable: u64) -> bool {
        let me = std::thread::current().id();
        if self.owner == Some(me) && self.drawable == drawable {
            return true;
        }
        let Some(glx) = gl::glx() else { return false };
        if (glx.MakeCurrent)(self.ctx.display, drawable, self.ctx.context) == 0 {
            return false;
        }
        self.owner = Some(me);
        self.drawable = drawable;
        true
    }

    unsafe fn current(&mut self) -> bool {
        let d = self.drawable;
        self.bind(d)
    }

    unsafe fn program(&mut self, with_color: bool) -> Option<u32> {
        let i = with_color as usize;
        if self.programs[i] != 0 {
            return Some(self.programs[i]);
        }
        let g = gl::gl()?;
        let l = crate::dlls::gl_legacy::legacy()?;
        let compile = |ty: u32, src: &str| -> Option<u32> {
            let s = (g.CreateShader)(ty);
            let c = std::ffi::CString::new(src).ok()?;
            let p = c.as_ptr();
            (g.ShaderSource)(s, 1, &p, std::ptr::null());
            (g.CompileShader)(s);
            let mut ok = 0;
            (g.GetShaderiv)(s, gl::GL_COMPILE_STATUS, &mut ok);
            (ok != 0).then_some(s)
        };
        let vs = compile(gl::GL_VERTEX_SHADER, GL_VS)?;
        let fs = compile(
            gl::GL_FRAGMENT_SHADER,
            if with_color { GL_FS_COLOR } else { GL_FS_WHITE },
        )?;
        let p = (g.CreateProgram)();
        (g.AttachShader)(p, vs);
        (g.AttachShader)(p, fs);
        (l.BindAttribLocation)(p, 0, c"a_pos".as_ptr());
        (l.BindAttribLocation)(p, 1, c"a_col".as_ptr());
        (g.LinkProgram)(p);
        let mut ok = 0;
        (g.GetProgramiv)(p, gl::GL_LINK_STATUS, &mut ok);
        (g.DeleteShader)(vs);
        (g.DeleteShader)(fs);
        if ok == 0 {
            log_warn!("d3d12", "programme GL passe-plat refusé par le pilote");
            (g.DeleteProgram)(p);
            return None;
        }
        self.programs[i] = p;
        Some(p)
    }

    unsafe fn draw(
        &mut self,
        fbo: u32,
        dc: &DrawCall,
        vb: &[u8],
        ib: Option<(&[u8], u32)>,
    ) -> bool {
        let mode = match dc.topology {
            1 => gl::GL_POINTS,
            2 => gl::GL_LINES,
            3 => gl::GL_LINE_STRIP,
            4 => gl::GL_TRIANGLES,
            5 => gl::GL_TRIANGLE_STRIP,
            _ => return false,
        };
        let with_color = dc.layout.iter().any(|a| a.0 == 1 && gl_attr(a.1).is_some());
        let Some(prog) = self.program(with_color) else {
            return false;
        };
        let Some(g) = gl::gl() else { return false };
        if self.vbo == 0 {
            (g.GenBuffers)(1, &mut self.vbo);
            (g.GenBuffers)(1, &mut self.ibo);
        }
        let vp = dc.viewport;
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
        (g.Viewport)(vp[0] as i32, vp[1] as i32, vp[2] as i32, vp[3] as i32);
        for cap in [
            gl::GL_DEPTH_TEST,
            gl::GL_CULL_FACE,
            gl::GL_BLEND,
            gl::GL_SCISSOR_TEST,
        ] {
            (g.Disable)(cap);
        }
        (g.UseProgram)(prog);
        (g.BindBuffer)(gl::GL_ARRAY_BUFFER, self.vbo);
        (g.BufferData)(
            gl::GL_ARRAY_BUFFER,
            vb.len() as isize,
            vb.as_ptr().cast(),
            gl::GL_STREAM_DRAW,
        );
        let base = if ib.is_some() && dc.base > 0 {
            dc.base as usize * dc.vb.2 as usize
        } else {
            0
        };
        let mut enabled = Vec::new();
        for &(loc, f, off) in &dc.layout {
            let Some((n, ty, norm)) = gl_attr(f) else {
                continue;
            };
            (g.EnableVertexAttribArray)(loc);
            (g.VertexAttribPointer)(
                loc,
                n,
                ty,
                norm,
                dc.vb.2 as i32,
                (off as usize + base) as *const c_void,
            );
            enabled.push(loc);
        }
        match ib {
            Some((bytes, f)) => {
                let (ty, isz) = if f == 42 {
                    (gl::GL_UNSIGNED_INT, 4)
                } else {
                    (gl::GL_UNSIGNED_SHORT, 2)
                };
                (g.BindBuffer)(gl::GL_ELEMENT_ARRAY_BUFFER, self.ibo);
                (g.BufferData)(
                    gl::GL_ELEMENT_ARRAY_BUFFER,
                    bytes.len() as isize,
                    bytes.as_ptr().cast(),
                    gl::GL_STREAM_DRAW,
                );
                (g.DrawElementsInstanced)(
                    mode,
                    dc.count as i32,
                    ty,
                    (dc.first as usize * isz) as *const c_void,
                    dc.instances as i32,
                );
                (g.BindBuffer)(gl::GL_ELEMENT_ARRAY_BUFFER, 0);
            }
            None => {
                (g.DrawArraysInstanced)(mode, dc.first as i32, dc.count as i32, dc.instances as i32)
            }
        }
        for loc in enabled {
            (g.DisableVertexAttribArray)(loc);
        }
        (g.BindBuffer)(gl::GL_ARRAY_BUFFER, 0);
        (g.UseProgram)(0);
        (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
        true
    }
}

impl Drop for GlBackend {
    fn drop(&mut self) {
        if let Some(g) = gl::gl() {
            unsafe {
                if self.current() {
                    for p in self.programs {
                        if p != 0 {
                            (g.DeleteProgram)(p);
                        }
                    }
                    if self.vbo != 0 {
                        (g.DeleteBuffers)(1, &self.vbo);
                        (g.DeleteBuffers)(1, &self.ibo);
                    }
                }
            }
        }
    }
}

unsafe fn gl_clear(fbo: u32, c: [f32; 4]) -> bool {
    let Some(g) = gl::gl() else { return false };
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, fbo);
    (g.Disable)(gl::GL_SCISSOR_TEST);
    (g.ColorMask)(1, 1, 1, 1);
    (g.ClearColor)(c[0], c[1], c[2], c[3]);
    (g.Clear)(gl::GL_COLOR_BUFFER_BIT);
    (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
    true
}

unsafe fn gl_read(fbo: u32, w: u32, h: u32, format: u32, ty: u32, dst: *mut u8) -> bool {
    let Some(g) = gl::gl() else { return false };
    (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, fbo);
    (g.PixelStorei)(gl::GL_PACK_ALIGNMENT, 1);
    (g.ReadPixels)(0, 0, w as i32, h as i32, format, ty, dst.cast());
    (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, 0);
    true
}

unsafe fn gl_upload(tex: u32, w: u32, h: u32, format: u32, ty: u32, src: *const u8) -> bool {
    let Some(g) = gl::gl() else { return false };
    (g.BindTexture)(gl::GL_TEXTURE_2D, tex);
    (g.PixelStorei)(gl::GL_UNPACK_ALIGNMENT, 1);
    (g.TexSubImage2D)(
        gl::GL_TEXTURE_2D,
        0,
        0,
        0,
        w as i32,
        h as i32,
        format,
        ty,
        src.cast(),
    );
    true
}

// ── Swapchain DXGI pour Direct3D 12 ────────────────────────────────────────

unsafe fn sc_of<'a>(this: *mut Obj) -> Option<MutexGuard<'a, SwapChain>> {
    match &(*this).kind {
        Kind::SwapChain(m) => Some(lock(&*(m.as_ref() as *const Mutex<SwapChain>))),
        _ => None,
    }
}

unsafe fn make_buffers(dev_obj: *mut Obj, d: &SwapDesc1) -> Result<Vec<*mut Obj>, i32> {
    let desc = ResourceDesc {
        dimension: DIM_TEX2D,
        alignment: 0,
        width: d.width as u64,
        height: d.height,
        depth_or_array: 1,
        mip_levels: 1,
        format: d.format,
        sample_count: 1,
        sample_quality: 0,
        layout: 0,
        flags: FLAG_RENDER_TARGET,
    };
    let heap = HeapProperties {
        ty: HEAP_DEFAULT,
        ..Default::default()
    };
    let mut out = Vec::new();
    for _ in 0..d.count {
        match create_resource(dev_obj, heap, &desc) {
            Ok(o) => out.push(o),
            Err(e) => {
                for o in out {
                    obj_release(o);
                }
                return Err(e);
            }
        }
    }
    Ok(out)
}

/// Appelée par la fabrique DXGI quand le « device » passé est une file D3D12.
pub(crate) unsafe fn create_swap_chain(
    queue: *mut c_void,
    hwnd: usize,
    desc: &SwapDesc1,
) -> Result<*mut c_void, i32> {
    let queue = queue as *mut Obj;
    let Some(dev) = device_of(queue) else {
        return Err(E_INVALIDARG);
    };
    let mut d = *desc;
    if d.width == 0 || d.height == 0 {
        d.width = 1280;
        d.height = 720;
    }
    if d.format == 0 {
        d.format = 28;
    }
    d.count = d.count.clamp(1, 16);
    let buffers = make_buffers((*queue).device, &d)?;
    let window = crate::dlls::gui::native_window(hwnd);
    let presenter = match (&dev.gpu, window) {
        (Gpu::Vk(ctx), Some(xid)) => {
            let p =
                VkPresenter::create(ctx.clone(), xid, d.width, d.height, gpu_caps::prefs().vsync);
            if p.is_none() {
                log_warn!(
                    "d3d12",
                    "présentation Vulkan impossible pour cette fenêtre : rien ne s'affichera"
                );
            }
            p
        }
        _ => None,
    };
    let gl_window = match &dev.gpu {
        Gpu::Gl(_) => window,
        _ => None,
    };
    let waitable = crate::dlls::kernel32::create_event_a(0, 1, 1, std::ptr::null());
    log_info!(
        "d3d12",
        "swapchain {}x{} ×{} (format {}) sur {}",
        d.width,
        d.height,
        d.count,
        d.format,
        dev.gpu.label()
    );
    let sc = SwapChain {
        desc: d,
        hwnd,
        buffers,
        index: 0,
        presents: 0,
        presenter,
        gl_window,
        latency: 3,
        waitable,
        warned: false,
    };
    Ok(new_obj(
        swapchain_vtable(),
        queue,
        Kind::SwapChain(Box::new(Mutex::new(sc))),
    )
    .cast())
}

unsafe fn present_buffer(dev: &Device, s: &mut SwapChain, buf: *mut Obj) {
    let Some(r) = res_of(buf) else { return };
    match &dev.gpu {
        Gpu::Vk(_) => {
            ensure_gpu(dev, r);
            let d = lock(&r.data);
            if let (Some(p), Some(GpuImage::Vk(img))) = (s.presenter.as_mut(), d.gpu.as_ref()) {
                if !p.present(img) && !s.warned {
                    s.warned = true;
                    log_warn!("d3d12", "échec de présentation Vulkan");
                }
            }
        }
        Gpu::Gl(m) => {
            let Some(win) = s.gl_window else { return };
            ensure_gpu(dev, r);
            let d = lock(&r.data);
            let Some(GpuImage::Gl { fbo, .. }) = d.gpu.as_ref() else {
                return;
            };
            let mut b = lock(m);
            let (Some(g), Some(glx)) = (gl::gl(), gl::glx()) else {
                return;
            };
            if !b.bind(win) {
                if !s.warned {
                    s.warned = true;
                    log_warn!(
                        "d3d12",
                        "fenêtre incompatible avec le contexte GL : rien ne s'affichera"
                    );
                }
                return;
            }
            let (w, h) = (s.desc.width as i32, s.desc.height as i32);
            (g.BindFramebuffer)(gl::GL_READ_FRAMEBUFFER, *fbo);
            (g.BindFramebuffer)(gl::GL_DRAW_FRAMEBUFFER, 0);
            // Lignes stockées du haut vers le bas : on retourne à l'affichage.
            (g.BlitFramebuffer)(
                0,
                0,
                w,
                h,
                0,
                h,
                w,
                0,
                gl::GL_COLOR_BUFFER_BIT,
                gl::GL_NEAREST as u32,
            );
            (glx.SwapBuffers)(b.ctx.display, win);
            (g.BindFramebuffer)(gl::GL_FRAMEBUFFER, 0);
        }
        Gpu::Cpu => {}
    }
}

unsafe extern "win64" fn sc_present(this: *mut Obj, _sync: u32, flags: u32) -> i32 {
    let Some(dev) = device_of(this) else {
        return DXGI_ERROR_INVALID_CALL;
    };
    let Some(mut s) = sc_of(this) else {
        return DXGI_ERROR_INVALID_CALL;
    };
    if flags & 1 != 0 {
        return S_OK; // DXGI_PRESENT_TEST
    }
    if let Some(&buf) = s.buffers.get(s.index as usize) {
        present_buffer(dev, &mut s, buf);
    }
    let n = s.buffers.len().max(1) as u32;
    s.index = (s.index + 1) % n;
    s.presents += 1;
    S_OK
}
unsafe extern "win64" fn sc_present1(
    this: *mut Obj,
    sync: u32,
    flags: u32,
    _p: *const c_void,
) -> i32 {
    sc_present(this, sync, flags)
}
unsafe extern "win64" fn sc_get_buffer(
    this: *mut Obj,
    i: u32,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let Some(s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    match s.buffers.get(i as usize) {
        Some(&b) => obj_qi(b, iid, out),
        None => DXGI_ERROR_INVALID_CALL,
    }
}
unsafe extern "win64" fn sc_get_parent(
    _: *mut Obj,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    crate::dlls::dxgi::factory_for(iid, out)
}
unsafe extern "win64" fn sc_get_device(
    this: *mut Obj,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    let q = (*this).device;
    if q.is_null() {
        return E_FAIL;
    }
    if obj_qi(q, iid, out) == S_OK {
        return S_OK;
    }
    obj_get_device(q, iid, out)
}
unsafe extern "win64" fn sc_get_fullscreen(
    _: *mut Obj,
    fs: *mut i32,
    out: *mut *mut c_void,
) -> i32 {
    if !fs.is_null() {
        *fs = 0;
    }
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    S_OK
}
unsafe extern "win64" fn sc_get_desc(this: *mut Obj, out: *mut DxgiSwapChainDesc) -> i32 {
    let Some(s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    if out.is_null() {
        return E_INVALIDARG;
    }
    let d = s.desc;
    *out = DxgiSwapChainDesc {
        buffer_desc: DxgiModeDesc {
            width: d.width,
            height: d.height,
            refresh_rate_numerator: 60,
            refresh_rate_denominator: 1,
            format: d.format,
            scanline_ordering: 0,
            scaling: 0,
        },
        sample_desc: d.sample,
        buffer_usage: d.usage,
        buffer_count: d.count,
        output_window: s.hwnd,
        windowed: 1,
        swap_effect: d.effect,
        flags: d.flags,
    };
    S_OK
}
unsafe extern "win64" fn sc_get_desc1(this: *mut Obj, out: *mut SwapDesc1) -> i32 {
    let Some(s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = s.desc;
    S_OK
}
unsafe extern "win64" fn sc_get_fs_desc(_: *mut Obj, out: *mut [u32; 5]) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = [60, 1, 0, 0, 1];
    S_OK
}
unsafe extern "win64" fn sc_get_hwnd(this: *mut Obj, out: *mut usize) -> i32 {
    let Some(s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = s.hwnd;
    S_OK
}
unsafe extern "win64" fn sc_no_output(_: *mut Obj, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    DXGI_ERROR_NOT_FOUND
}
unsafe extern "win64" fn sc_present_count(this: *mut Obj, out: *mut u32) -> i32 {
    let Some(s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    if !out.is_null() {
        *out = s.presents;
    }
    S_OK
}
unsafe extern "win64" fn sc_get_rotation(_: *mut Obj, out: *mut u32) -> i32 {
    if !out.is_null() {
        *out = 1; // DXGI_MODE_ROTATION_IDENTITY
    }
    S_OK
}
unsafe extern "win64" fn sc_get_source_size(this: *mut Obj, w: *mut u32, h: *mut u32) -> i32 {
    let Some(s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    if !w.is_null() {
        *w = s.desc.width;
    }
    if !h.is_null() {
        *h = s.desc.height;
    }
    S_OK
}
unsafe extern "win64" fn sc_set_latency(this: *mut Obj, n: u32) -> i32 {
    match sc_of(this) {
        Some(mut s) if (1..=16).contains(&n) => {
            s.latency = n;
            S_OK
        }
        _ => DXGI_ERROR_INVALID_CALL,
    }
}
unsafe extern "win64" fn sc_get_latency(this: *mut Obj, out: *mut u32) -> i32 {
    let Some(s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    if !out.is_null() {
        *out = s.latency;
    }
    S_OK
}
/// Évènement à réinitialisation manuelle toujours signalé : la file est
/// synchrone, l'image suivante peut toujours être préparée.
unsafe extern "win64" fn sc_waitable(this: *mut Obj) -> usize {
    sc_of(this).map(|s| s.waitable).unwrap_or(0)
}
unsafe extern "win64" fn sc_current_index(this: *mut Obj) -> u32 {
    sc_of(this).map(|s| s.index).unwrap_or(0)
}
unsafe extern "win64" fn sc_color_space_support(_: *mut Obj, cs: u32, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = u32::from(cs == 0);
    S_OK
}

unsafe fn resize(this: *mut Obj, count: u32, w: u32, h: u32, fmt: u32) -> i32 {
    let Some(mut s) = sc_of(this) else {
        return E_INVALIDARG;
    };
    // Comme DXGI : refus tant que l'application garde des tampons.
    if s.buffers
        .iter()
        .any(|&b| (*b).refs.load(Ordering::Acquire) > 1)
    {
        return DXGI_ERROR_INVALID_CALL;
    }
    let mut d = s.desc;
    if count != 0 {
        d.count = count.clamp(1, 16);
    }
    if w != 0 && h != 0 {
        d.width = w;
        d.height = h;
    }
    if fmt != 0 {
        d.format = fmt;
    }
    let queue = (*this).device;
    let fresh = match make_buffers((*queue).device, &d) {
        Ok(b) => b,
        Err(e) => return e,
    };
    for b in std::mem::replace(&mut s.buffers, fresh) {
        obj_release(b);
    }
    s.desc = d;
    s.index = 0;
    if let Some(p) = s.presenter.as_mut() {
        p.rebuild(d.width, d.height);
    }
    S_OK
}
unsafe extern "win64" fn sc_resize(
    this: *mut Obj,
    count: u32,
    w: u32,
    h: u32,
    fmt: u32,
    _flags: u32,
) -> i32 {
    resize(this, count, w, h, fmt)
}
unsafe extern "win64" fn sc_resize1(
    this: *mut Obj,
    count: u32,
    w: u32,
    h: u32,
    fmt: u32,
    _flags: u32,
    _masks: *const u32,
    _queues: *const *mut c_void,
) -> i32 {
    resize(this, count, w, h, fmt)
}

// ── Création de l'appareil ─────────────────────────────────────────────────

fn make_gpu() -> Gpu {
    let caps = gpu_caps::caps();
    let prefs = gpu_caps::prefs();
    if gpu_caps::select_backend(caps, &prefs) == Backend::Vulkan {
        if let Some(ctx) = VkContext::create() {
            return Gpu::Vk(ctx);
        }
        log_warn!(
            "d3d12",
            "Vulkan annoncé mais non initialisable : repli sur OpenGL"
        );
    }
    if caps.opengl {
        if let Some(b) = GlBackend::create() {
            return Gpu::Gl(Mutex::new(b));
        }
    }
    log_warn!(
        "d3d12",
        "ni Vulkan ni OpenGL : appareil D3D12 sur processeur (rien ne s'affiche)"
    );
    Gpu::Cpu
}

fn new_device(gpu: Gpu) -> *mut Obj {
    log_info!("d3d12", "appareil Direct3D 12 sur {}", gpu.label());
    new_obj(
        device_vtable(),
        std::ptr::null_mut(),
        Kind::Device(Box::new(Device { gpu })),
    )
}

pub unsafe extern "win64" fn D3D12CreateDevice(
    _adapter: *mut c_void,
    min_level: u32,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if min_level < 0x9100 || min_level > MAX_FEATURE_LEVEL {
        return E_INVALIDARG;
    }
    if out.is_null() {
        // Simple test de prise en charge : aucun contexte GPU n'est créé.
        return S_FALSE;
    }
    hand_out(new_device(make_gpu()), iid, out)
}

pub unsafe extern "win64" fn D3D12GetDebugInterface(
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOINTERFACE
}

pub unsafe extern "win64" fn D3D12EnableExperimentalFeatures(
    _n: u32,
    _iids: *const Guid,
    _cfg: *mut c_void,
    _sizes: *mut u32,
) -> i32 {
    E_NOINTERFACE
}

pub unsafe extern "win64" fn D3D12GetInterface(
    _clsid: *const Guid,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOINTERFACE
}

/// Sérialise une signature racine dans un format maison (« WLRS ») : elle
/// n'est relue que par `CreateRootSignature`, qui accepte aussi les blobs
/// produits par le compilateur Microsoft.
fn root_blob(version: u32, params: u32) -> Vec<u8> {
    let mut v = b"WLRS".to_vec();
    v.extend(version.to_le_bytes());
    v.extend(params.to_le_bytes());
    v
}

pub unsafe extern "win64" fn D3D12SerializeRootSignature(
    desc: *const u32,
    version: u32,
    blob: *mut *mut Obj,
    err: *mut *mut Obj,
) -> i32 {
    if !err.is_null() {
        *err = std::ptr::null_mut();
    }
    if desc.is_null() || blob.is_null() {
        return E_INVALIDARG;
    }
    *blob = new_blob(root_blob(version, *desc));
    S_OK
}

pub unsafe extern "win64" fn D3D12SerializeVersionedRootSignature(
    desc: *const u32,
    blob: *mut *mut Obj,
    err: *mut *mut Obj,
) -> i32 {
    if !err.is_null() {
        *err = std::ptr::null_mut();
    }
    if desc.is_null() || blob.is_null() {
        return E_INVALIDARG;
    }
    *blob = new_blob(root_blob(*desc, *desc.add(2)));
    S_OK
}

pub unsafe extern "win64" fn D3D12CreateRootSignatureDeserializer(
    _p: *const c_void,
    _n: usize,
    _iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3D12CreateDevice" => fn D3D12CreateDevice,
        "D3D12GetDebugInterface" => fn D3D12GetDebugInterface,
        "D3D12EnableExperimentalFeatures" => fn D3D12EnableExperimentalFeatures,
        "D3D12GetInterface" => fn D3D12GetInterface,
        "D3D12SerializeRootSignature" => fn D3D12SerializeRootSignature,
        "D3D12SerializeVersionedRootSignature" => fn D3D12SerializeVersionedRootSignature,
        "D3D12CreateRootSignatureDeserializer" => fn D3D12CreateRootSignatureDeserializer,
        "D3D12CreateVersionedRootSignatureDeserializer" => fn D3D12CreateRootSignatureDeserializer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iid(d1: u32) -> Guid {
        Guid {
            data1: d1,
            data2: 0,
            data3: 0,
            data4: [0; 8],
        }
    }

    #[test]
    fn dispositions_abi() {
        use std::mem::size_of;
        assert_eq!(size_of::<ResourceDesc>(), 56);
        assert_eq!(size_of::<HeapProperties>(), 20);
        assert_eq!(size_of::<CopyLocation>(), 48);
        assert_eq!(size_of::<Barrier>(), 32);
        assert_eq!(size_of::<InputElement>(), 32);
        assert_eq!(size_of::<PlacedFootprint>(), 32);
        assert_eq!(size_of::<SwapDesc1>(), 48);
        assert_eq!(size_of::<Descriptor>(), DESC_SIZE as usize);
        assert_eq!(std::mem::offset_of!(ResourceDesc, format), 32);
        assert_eq!(std::mem::offset_of!(CopyLocation, placed), 16);
    }

    fn tex(w: u64, h: u32, mips: u16, format: u32) -> ResourceDesc {
        ResourceDesc {
            dimension: DIM_TEX2D,
            width: w,
            height: h,
            depth_or_array: 1,
            mip_levels: mips,
            format,
            sample_count: 1,
            ..Default::default()
        }
    }

    fn buffer(size: u64) -> ResourceDesc {
        ResourceDesc {
            dimension: DIM_BUFFER,
            width: size,
            height: 1,
            depth_or_array: 1,
            mip_levels: 1,
            sample_count: 1,
            layout: 1,
            ..Default::default()
        }
    }

    #[test]
    fn sous_ressources_et_empreintes() {
        let d = normalized(&tex(64, 64, 0, 28));
        assert_eq!(d.mip_levels, 7);
        let l = sub_layouts(&d).unwrap();
        assert_eq!(l[0].row_bytes, 256);
        assert_eq!(l[1].offset, 64 * 64 * 4);
        assert_eq!(l[6].width, 1);
        let bc = sub_layouts(&tex(16, 16, 1, 71)).unwrap();
        assert_eq!((bc[0].row_bytes, bc[0].rows), (32, 4));
        let mut fp = [PlacedFootprint::default(); 2];
        let mut rows = [0u32; 2];
        let mut total = 0u64;
        unsafe {
            dev_footprints(
                std::ptr::null_mut(),
                &tex(8, 8, 2, 28),
                0,
                2,
                0,
                fp.as_mut_ptr(),
                rows.as_mut_ptr(),
                std::ptr::null_mut(),
                &mut total,
            )
        };
        assert_eq!(fp[0].footprint.row_pitch, 256);
        assert_eq!(fp[1].offset % 512, 0);
        assert_eq!(rows, [8, 4]);
        assert_eq!(total, fp[1].offset + 256 * 3 + 16);
        assert!(sub_layouts(&tex(4, 4, 1, 9999)).is_none());
    }

    #[test]
    fn couleurs_encodees() {
        assert_eq!(
            encode_color(28, [1.0, 0.0, 0.5, 1.0]).unwrap(),
            vec![255, 0, 128, 255]
        );
        assert_eq!(
            encode_color(87, [1.0, 0.0, 0.0, 1.0]).unwrap(),
            vec![0, 0, 255, 255]
        );
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert!(encode_color(71, [0.0; 4]).is_none());
    }

    unsafe fn create<T>(f: impl FnOnce(*const Guid, *mut *mut c_void) -> i32, d1: u32) -> *mut T {
        let g = iid(d1);
        let mut out = std::ptr::null_mut();
        assert_eq!(f(&g, &mut out), S_OK, "création 0x{d1:08x}");
        out.cast()
    }

    /// Scénario complet : effacement bleu, triangle sur la moitié haute,
    /// copie vers un tampon de relecture, barrière de clôture et évènement.
    unsafe fn scenario(gpu: Gpu) -> Vec<u8> {
        let dev = new_device(gpu);
        let q: *mut Obj = create(
            |i, o| dev_create_queue(dev, &QueueDesc::default(), i, o),
            0x0ec8_70a6,
        );
        let alloc: *mut Obj = create(|i, o| dev_create_allocator(dev, 0, i, o), 0x6102_dee4);
        let list: *mut Obj = create(
            |i, o| dev_create_list(dev, 0, 0, alloc, std::ptr::null_mut(), i, o),
            0x5b16_0d0f,
        );
        let heap_desc = HeapDesc {
            ty: 2,
            count: 2,
            flags: 0,
            node_mask: 0,
        };
        let heap: *mut Obj = create(
            |i, o| dev_create_desc_heap(dev, &heap_desc, i, o),
            0x8efb_471d,
        );
        let mut handle = 0usize;
        heap_cpu_start(heap, &mut handle);
        let def = HeapProperties {
            ty: HEAP_DEFAULT,
            ..Default::default()
        };
        let mut rt_desc = tex(8, 8, 1, 28);
        rt_desc.flags = FLAG_RENDER_TARGET;
        let rt: *mut Obj = create(
            |i, o| dev_create_committed(dev, &def, 0, &rt_desc, 0, std::ptr::null(), i, o),
            0x6964_42be,
        );
        dev_create_rtv(dev, rt, std::ptr::null(), handle);
        let up = HeapProperties {
            ty: HEAP_UPLOAD,
            ..Default::default()
        };
        let vb: *mut Obj = create(
            |i, o| dev_create_committed(dev, &up, 0, &buffer(36), 0, std::ptr::null(), i, o),
            0x6964_42be,
        );
        let verts: [f32; 9] = [-1.0, 0.0, 0.0, 3.0, 0.0, 0.0, -1.0, 4.0, 0.0];
        let mut p: *mut u8 = std::ptr::null_mut();
        assert_eq!(res_map(vb, 0, std::ptr::null(), &mut p), S_OK);
        std::ptr::copy_nonoverlapping(verts.as_ptr() as *const u8, p, 36);
        res_unmap(vb, 0, std::ptr::null());
        let rbp = HeapProperties {
            ty: HEAP_READBACK,
            ..Default::default()
        };
        let rb: *mut Obj = create(
            |i, o| dev_create_committed(dev, &rbp, 0, &buffer(256 * 8), 0, std::ptr::null(), i, o),
            0x6964_42be,
        );

        let elem = InputElement {
            semantic: c"POSITION".as_ptr(),
            semantic_index: 0,
            format: 6,
            slot: 0,
            offset: 0,
            class: 0,
            step: 0,
        };
        let mut pso_desc = [0u8; 656];
        std::ptr::write_unaligned(
            pso_desc.as_mut_ptr().add(PSO_INPUT_LAYOUT) as *mut *const InputElement,
            &elem,
        );
        std::ptr::write_unaligned(
            pso_desc.as_mut_ptr().add(PSO_INPUT_LAYOUT + 8) as *mut u32,
            1,
        );
        std::ptr::write_unaligned(pso_desc.as_mut_ptr().add(PSO_TOPOLOGY_TYPE) as *mut u32, 3);
        std::ptr::write_unaligned(pso_desc.as_mut_ptr().add(PSO_NUM_RT) as *mut u32, 1);
        std::ptr::write_unaligned(pso_desc.as_mut_ptr().add(PSO_RT_FORMATS) as *mut u32, 28);
        let pso: *mut Obj = create(
            |i, o| dev_create_gfx_pso(dev, pso_desc.as_ptr(), i, o),
            0x765a_30f3,
        );

        list_om_rts(list, 1, &handle, 0, std::ptr::null());
        list_clear_rtv(list, handle, &[0.0, 0.0, 1.0, 1.0], 0, std::ptr::null());
        // Poignée inconnue : ignorée sans planter.
        list_clear_rtv(list, 0xdead_0000, &[1.0; 4], 0, std::ptr::null());
        list_set_pso(list, pso);
        list_topology(list, 4);
        list_viewports(list, 1, &[0.0, 0.0, 8.0, 8.0, 0.0, 1.0]);
        let view = VertexBufferView {
            location: res_va(vb),
            size: 36,
            stride: 12,
        };
        list_vbs(list, 0, 1, &view);
        list_draw(list, 3, 1, 0, 0);
        list_barrier(list, 1, std::ptr::null());
        let dst = CopyLocation {
            resource: rb,
            ty: 1,
            placed: PlacedFootprint {
                offset: 0,
                footprint: Footprint {
                    format: 28,
                    width: 8,
                    height: 8,
                    depth: 1,
                    row_pitch: 256,
                },
            },
        };
        let src = CopyLocation {
            resource: rt,
            ty: 0,
            placed: PlacedFootprint::default(),
        };
        list_copy_texture(list, &dst, 0, 0, 0, &src, std::ptr::null());
        assert_eq!(list_close(list), S_OK);
        queue_execute(q, 1, &list);

        let fence: *mut Obj = create(|i, o| dev_create_fence(dev, 0, 0, i, o), 0x0a75_3dcf);
        let ev = crate::dlls::kernel32::create_event_a(0, 1, 0, std::ptr::null());
        assert_eq!(fence_event(fence, 1, ev), S_OK);
        assert_eq!(
            crate::dlls::kernel32::wait_for_single_object(ev, 0),
            258,
            "pas encore signalé"
        );
        assert_eq!(queue_signal(q, fence, 1), S_OK);
        assert_eq!(fence_completed(fence), 1);
        assert_eq!(
            crate::dlls::kernel32::wait_for_single_object(ev, 0),
            0,
            "évènement signalé"
        );
        crate::dlls::kernel32::close_handle(ev);

        let mut out: *mut u8 = std::ptr::null_mut();
        assert_eq!(res_map(rb, 0, std::ptr::null(), &mut out), S_OK);
        let pixels = std::slice::from_raw_parts(out, 256 * 8).to_vec();
        for o in [fence, pso, rb, vb, rt, heap, list, alloc, q] {
            obj_release(o);
        }
        assert_eq!(obj_release(dev), 0, "toutes les références rendues");
        pixels
    }

    fn px(p: &[u8], x: usize, y: usize) -> &[u8] {
        &p[y * 256 + x * 4..y * 256 + x * 4 + 4]
    }

    #[test]
    fn rendu_processeur() {
        let p = unsafe { scenario(Gpu::Cpu) };
        // Sans GPU : l'effacement et la copie fonctionnent, le dessin est ignoré.
        assert_eq!(px(&p, 0, 0), [0, 0, 255, 255]);
        assert_eq!(px(&p, 7, 7), [0, 0, 255, 255]);
    }

    #[test]
    fn rendu_vulkan_si_disponible() {
        std::env::set_var("WINLUX_GFX_BACKEND", "vulkan");
        let Some(ctx) = VkContext::create() else {
            return;
        };
        let p = unsafe { scenario(Gpu::Vk(ctx)) };
        // Triangle sur la moitié haute (Y vers le haut comme Direct3D).
        assert_eq!(px(&p, 3, 0), [255, 255, 255, 255]);
        assert_eq!(px(&p, 3, 7), [0, 0, 255, 255]);
    }

    #[test]
    fn rendu_opengl_si_disponible() {
        if !gpu_caps::caps().opengl {
            return;
        }
        let Some(b) = GlBackend::create() else { return };
        let p = unsafe { scenario(Gpu::Gl(Mutex::new(b))) };
        assert_eq!(px(&p, 3, 0), [255, 255, 255, 255]);
        assert_eq!(px(&p, 3, 7), [0, 0, 255, 255]);
    }

    #[test]
    fn capacites_et_sondage() {
        unsafe {
            assert_eq!(
                D3D12CreateDevice(
                    std::ptr::null_mut(),
                    0xb000,
                    &iid(0x1898_19f1),
                    std::ptr::null_mut()
                ),
                S_FALSE
            );
            assert_eq!(
                D3D12CreateDevice(
                    std::ptr::null_mut(),
                    0xd000,
                    &iid(0x1898_19f1),
                    std::ptr::null_mut()
                ),
                E_INVALIDARG
            );
            let dev = new_device(Gpu::Cpu);
            let levels = [0xb000u32, 0xc000, 0xc100, 0xc200];
            let mut fl = [0u8; 24];
            std::ptr::write_unaligned(fl.as_mut_ptr() as *mut u32, 4);
            std::ptr::write_unaligned(fl.as_mut_ptr().add(8) as *mut *const u32, levels.as_ptr());
            assert_eq!(dev_check_feature(dev, 2, fl.as_mut_ptr(), 24), S_OK);
            assert_eq!(
                std::ptr::read_unaligned(fl.as_ptr().add(16) as *const u32),
                0xc100
            );
            let mut opts = [0xffu8; 60];
            assert_eq!(dev_check_feature(dev, 0, opts.as_mut_ptr(), 60), S_OK);
            assert_eq!(opts[16], 1);
            assert_eq!(opts[0], 0);
            // Raytracing (OPTIONS5) : zéro = absent.
            let mut o5 = [0xffu8; 12];
            assert_eq!(dev_check_feature(dev, 27, o5.as_mut_ptr(), 12), S_OK);
            assert!(o5.iter().all(|&b| b == 0));
            let mut sm = 0x65u32;
            dev_check_feature(dev, 7, &mut sm as *mut u32 as *mut u8, 4);
            assert_eq!(sm, 0x51);
            let mut blob: *mut Obj = std::ptr::null_mut();
            let rs = [0u32; 10];
            assert_eq!(
                D3D12SerializeRootSignature(rs.as_ptr(), 1, &mut blob, std::ptr::null_mut()),
                S_OK
            );
            assert_eq!(blob_size(blob), 12);
            let sig: *mut Obj = create(
                |i, o| dev_create_root_sig(dev, 0, blob_ptr(blob), blob_size(blob), i, o),
                0xc54a_6b66,
            );
            obj_release(sig);
            obj_release(blob);
            let mut q = std::ptr::null_mut();
            assert_eq!(obj_qi(dev, &iid(0x1234_5678), &mut q), E_NOINTERFACE);
            assert!(q.is_null());
            assert_eq!(obj_release(dev), 0);
        }
    }

    #[test]
    fn swapchain_d3d12() {
        unsafe {
            let dev = new_device(Gpu::Cpu);
            let q: *mut Obj = create(
                |i, o| dev_create_queue(dev, &QueueDesc::default(), i, o),
                0x0ec8_70a6,
            );
            assert!(is_queue(q.cast()));
            let desc = SwapDesc1 {
                width: 64,
                height: 32,
                format: 28,
                count: 3,
                ..Default::default()
            };
            let sc = create_swap_chain(q.cast(), 0, &desc).expect("swapchain") as *mut Obj;
            assert_eq!(sc_current_index(sc), 0);
            let mut buf = std::ptr::null_mut();
            assert_eq!(sc_get_buffer(sc, 1, &iid(0x6964_42be), &mut buf), S_OK);
            let mut rd = ResourceDesc::default();
            res_desc(buf as *mut Obj, &mut rd);
            assert_eq!((rd.width, rd.height, rd.format), (64, 32, 28));
            assert_eq!(sc_present(sc, 1, 0), S_OK);
            assert_eq!(sc_present(sc, 1, 0), S_OK);
            assert_eq!(sc_current_index(sc), 2);
            // Tampon encore tenu : ResizeBuffers refuse, comme DXGI.
            assert_eq!(sc_resize(sc, 0, 128, 64, 0, 0), DXGI_ERROR_INVALID_CALL);
            obj_release(buf as *mut Obj);
            assert_eq!(sc_resize(sc, 2, 128, 64, 0, 0), S_OK);
            let mut d1 = SwapDesc1::default();
            sc_get_desc1(sc, &mut d1);
            assert_eq!((d1.width, d1.height, d1.count), (128, 64, 2));
            assert_eq!(sc_current_index(sc), 0);
            let ev = sc_waitable(sc);
            assert_eq!(crate::dlls::kernel32::wait_for_single_object(ev, 0), 0);
            obj_release(sc);
            obj_release(q);
            assert_eq!(obj_release(dev), 0);
        }
    }
}
