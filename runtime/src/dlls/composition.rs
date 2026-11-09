//! Windows.UI.Composition (#1416) : `CompositorController`, `Compositor`,
//! visuels conteneurs et sprites, pinceaux de couleur et de surface,
//! découpe par marges, jeux de propriétés, appareil graphique et surfaces
//! de dessin Direct2D, cible de fenêtre de bureau.
//!
//! Interfaces et identifiants : métadonnées WinRT publiques du SDK, telles
//! que les projette windows-rs (MIT/Apache), et en-têtes d'interopérabilité
//! publics (windows.ui.composition.interop.h).
//!
//! Le rendu est logiciel : `Commit` compose l'arbre de chaque cible
//! (décalages, ancres, échelles, opacité, visibilité, découpes) par-dessus
//! le contenu de la fenêtre et le présente dans son DC. Les animations, les
//! effets et le défilement des surfaces ne sont pas encore pris en charge
//! et le disent au journal.
//!
//! Paint.NET crée son `PdnCompositor` au démarrage (#1416).

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::audio_com::VTable;
use super::comobj::{read_guid, Guid, E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK};

const E_FAIL: i32 = 0x8000_4005u32 as i32;
const RO_E_CLOSED: i32 = 0x8000_0013u32 as i32;

/// GUID écrit « xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx ».
const fn iid(s: &str) -> Guid {
    let b = s.as_bytes();
    let mut digits = [0u8; 32];
    let (mut i, mut n) = (0, 0);
    while i < b.len() {
        let c = b[i];
        let v = match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        };
        if let Some(v) = v {
            digits[n] = v;
            n += 1;
        }
        i += 1;
    }
    assert!(n == 32, "GUID mal formé");
    let mut raw = [0u8; 16];
    let mut k = 0;
    while k < 16 {
        raw[k] = digits[2 * k] << 4 | digits[2 * k + 1];
        k += 1;
    }
    // Data1..3 petit-boutistes en mémoire.
    [
        raw[3], raw[2], raw[1], raw[0], raw[5], raw[4], raw[7], raw[6], raw[8], raw[9], raw[10],
        raw[11], raw[12], raw[13], raw[14], raw[15],
    ]
}

const IID_IINSPECTABLE: Guid = iid("af86e2e0-b12d-4c6a-9c5a-d7aa65101e90");
const IID_IAGILEOBJECT: Guid = iid("94ea2b94-e9cc-49e0-c0ff-ee64ca8f5b90");
const IID_ACTIVATION_FACTORY: Guid = iid("00000035-0000-0000-c000-000000000046");
const IID_CLOSABLE: Guid = iid("30d5a829-7fa4-4026-83bb-d75bae4ea99e");
const IID_ASYNC_INFO: Guid = iid("00000036-0000-0000-c000-000000000046");
const IID_ASYNC_ACTION: Guid = iid("5a648006-843a-4da9-865b-9d26e5dfad7b");

const IID_CONTROLLER: Guid = iid("2d75f35a-70a7-4395-ba2d-cef0b18399f9");
const IID_COMPOSITOR: Guid = iid("b403ca50-7f8c-4e83-985f-cc45060036d8");
const IID_DESKTOP_INTEROP: Guid = iid("29e691fa-4567-4dca-b319-d0f207eb6807");
const IID_COMPOSITOR_INTEROP: Guid = iid("25297d5c-3ad4-4c9c-b5cf-e36a38512330");
const IID_OBJECT: Guid = iid("bcb4ad45-7609-4550-934f-16002a68fded");
const IID_VISUAL: Guid = iid("117e202d-a859-4c89-873b-c2aa566788e3");
const IID_CONTAINER: Guid = iid("02f6bc74-ed20-4773-afe6-d49b4a93db32");
const IID_SPRITE: Guid = iid("08e05581-1ad1-4f97-9757-402d76e4233b");
const IID_COLLECTION: Guid = iid("8b745505-fd3e-4a98-84a8-e949468c6bcb");
const IID_BRUSH: Guid = iid("ab0d7608-30c0-40e9-b568-b60a6bd1fb46");
const IID_COLOR_BRUSH: Guid = iid("2b264c5e-bf35-4831-8642-cf70c20fff2f");
const IID_SURFACE_BRUSH: Guid = iid("ad016d79-1e4c-4c0d-9c29-83338c87c162");
const IID_CLIP: Guid = iid("1ccd2a52-cfc7-4ace-9983-146bb8eb6a3c");
const IID_INSET_CLIP: Guid = iid("1e73e647-84c7-477a-b474-5880e0442e15");
const IID_TARGET: Guid = iid("a1bea8ba-d726-4663-8129-6b5e7927ffa6");
const IID_DESKTOP_TARGET: Guid = iid("6329d6ca-3366-490e-9db3-25312929ac51");
const IID_DEVICE: Guid = iid("fb22c6e1-80a2-4667-9936-dbeaf6eefe95");
const IID_DEVICE2: Guid = iid("0fb8bdf6-c0f0-4bcc-9fb8-084982490d7d");
const IID_DEVICE_INTEROP: Guid = iid("a116ff71-f8bf-4c8a-9c98-70779a32a9c8");
const IID_SURFACE: Guid = iid("1527540d-42c7-47a6-a408-668f79a90dfb");
const IID_DRAWING_SURFACE: Guid = iid("a166c300-fad0-4d11-9e67-e433162ff49e");
const IID_DRAWING_SURFACE2: Guid = iid("fad0e88b-e354-44e8-8e3d-c4880d5a213f");
const IID_SURFACE_INTEROP: Guid = iid("fd04e6e3-fe0c-4c3c-ab19-a07601a576ee");
const IID_PROPERTY_SET: Guid = iid("c9d6d202-5f67-4453-9117-9eadd430d3c2");

const CONTROLLER_CLASS: &str = "Windows.UI.Composition.Core.CompositorController";
const COMPOSITOR_CLASS: &str = "Windows.UI.Composition.Compositor";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------
// Appels vers des objets COM quelconques
// ---------------------------------------------------------------------

unsafe fn vcall<F: Copy>(obj: *mut c_void, slot: usize) -> F {
    let vtbl = *(obj as *const *const usize);
    std::mem::transmute_copy(&*vtbl.add(slot))
}

unsafe fn com_add_ref(p: *mut c_void) {
    if !p.is_null() {
        vcall::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 1)(p);
    }
}

unsafe fn com_release(p: *mut c_void) {
    if !p.is_null() {
        vcall::<unsafe extern "win64" fn(*mut c_void) -> u32>(p, 2)(p);
    }
}

unsafe fn com_qi(p: *mut c_void, iid: &Guid) -> *mut c_void {
    let mut out = std::ptr::null_mut();
    let qi: unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32 =
        vcall(p, 0);
    if qi(p, iid, &mut out) < 0 {
        return std::ptr::null_mut();
    }
    out
}

unsafe fn hstring(h: usize) -> String {
    String::from_utf16_lossy(super::electron_win32::hstring_slice(h))
}

// ---------------------------------------------------------------------
// Objets à plusieurs interfaces
// ---------------------------------------------------------------------

/// Une interface : sa table, puis l'objet qui la porte.
#[repr(C)]
struct Slot {
    vt: *const (),
    owner: *mut Obj,
}

#[repr(C)]
struct Entry {
    slot: Slot,
    iid: Guid,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Controller,
    Compositor,
    ContainerVisual,
    SpriteVisual,
    Collection,
    ColorBrush,
    SurfaceBrush,
    InsetClip,
    Target,
    Device,
    Surface,
    PropertySet,
    Async,
}

impl Kind {
    fn class(self) -> &'static str {
        match self {
            Kind::Controller => CONTROLLER_CLASS,
            Kind::Compositor => COMPOSITOR_CLASS,
            Kind::ContainerVisual => "Windows.UI.Composition.ContainerVisual",
            Kind::SpriteVisual => "Windows.UI.Composition.SpriteVisual",
            Kind::Collection => "Windows.UI.Composition.VisualCollection",
            Kind::ColorBrush => "Windows.UI.Composition.CompositionColorBrush",
            Kind::SurfaceBrush => "Windows.UI.Composition.CompositionSurfaceBrush",
            Kind::InsetClip => "Windows.UI.Composition.InsetClip",
            Kind::Target => "Windows.UI.Composition.Desktop.DesktopWindowTarget",
            Kind::Device => "Windows.UI.Composition.CompositionGraphicsDevice",
            Kind::Surface => "Windows.UI.Composition.CompositionDrawingSurface",
            Kind::PropertySet => "Windows.UI.Composition.CompositionPropertySet",
            Kind::Async => "Windows.Foundation.IAsyncAction",
        }
    }

    /// Interfaces de l'objet, la première étant celle par défaut.
    fn ifaces(self) -> Vec<(Guid, *const ())> {
        let object = [
            (IID_OBJECT, OBJECT_VT.ptr()),
            (IID_CLOSABLE, CLOSABLE_VT.ptr()),
        ];
        let mut v: Vec<(Guid, *const ())> = match self {
            Kind::Controller => {
                return vec![
                    (IID_CONTROLLER, CONTROLLER_VT.ptr()),
                    (IID_CLOSABLE, CLOSABLE_VT.ptr()),
                ]
            }
            Kind::Compositor => {
                return vec![
                    (IID_COMPOSITOR, COMPOSITOR_VT.ptr()),
                    (IID_DESKTOP_INTEROP, DESKTOP_INTEROP_VT.ptr()),
                    (IID_COMPOSITOR_INTEROP, COMPOSITOR_INTEROP_VT.ptr()),
                    (IID_CLOSABLE, CLOSABLE_VT.ptr()),
                ]
            }
            Kind::Async => {
                return vec![
                    (IID_ASYNC_ACTION, ASYNC_ACTION_VT.ptr()),
                    (IID_ASYNC_INFO, ASYNC_INFO_VT.ptr()),
                ]
            }
            Kind::ContainerVisual => vec![
                (IID_CONTAINER, CONTAINER_VT.ptr()),
                (IID_VISUAL, VISUAL_VT.ptr()),
            ],
            Kind::SpriteVisual => vec![
                (IID_SPRITE, SPRITE_VT.ptr()),
                (IID_CONTAINER, CONTAINER_VT.ptr()),
                (IID_VISUAL, VISUAL_VT.ptr()),
            ],
            Kind::Collection => vec![(IID_COLLECTION, COLLECTION_VT.ptr())],
            Kind::ColorBrush => vec![
                (IID_COLOR_BRUSH, COLOR_BRUSH_VT.ptr()),
                (IID_BRUSH, EMPTY_VT.ptr()),
            ],
            Kind::SurfaceBrush => vec![
                (IID_SURFACE_BRUSH, SURFACE_BRUSH_VT.ptr()),
                (IID_BRUSH, EMPTY_VT.ptr()),
            ],
            Kind::InsetClip => vec![
                (IID_INSET_CLIP, INSET_CLIP_VT.ptr()),
                (IID_CLIP, EMPTY_VT.ptr()),
            ],
            Kind::Target => vec![
                (IID_DESKTOP_TARGET, DESKTOP_TARGET_VT.ptr()),
                (IID_TARGET, TARGET_VT.ptr()),
            ],
            Kind::Device => vec![
                (IID_DEVICE, DEVICE_VT.ptr()),
                (IID_DEVICE2, DEVICE2_VT.ptr()),
                (IID_DEVICE_INTEROP, DEVICE_INTEROP_VT.ptr()),
            ],
            Kind::Surface => vec![
                (IID_DRAWING_SURFACE, DRAWING_SURFACE_VT.ptr()),
                (IID_DRAWING_SURFACE2, DRAWING_SURFACE2_VT.ptr()),
                (IID_SURFACE, EMPTY_VT.ptr()),
                (IID_SURFACE_INTEROP, SURFACE_INTEROP_VT.ptr()),
            ],
            Kind::PropertySet => vec![(IID_PROPERTY_SET, PROPERTY_SET_VT.ptr())],
        };
        v.extend_from_slice(&object);
        v
    }
}

struct Obj {
    refs: AtomicU32,
    kind: Kind,
    entries: Box<[Entry]>,
    /// Compositeur propriétaire (tenu), nul pour le compositeur lui-même.
    compositor: *mut Obj,
    st: Mutex<State>,
}

struct State {
    data: Data,
    /// ICompositionObject::Properties, créé à la demande (tenu).
    properties: *mut Obj,
}

#[derive(Clone, Copy, PartialEq)]
struct Visual {
    anchor: [f32; 2],
    backface: i32,
    border: i32,
    center: [f32; 3],
    composite: i32,
    visible: u8,
    offset: [f32; 3],
    opacity: f32,
    orientation: [f32; 4],
    rotation: f32,
    axis: [f32; 3],
    scale: [f32; 3],
    size: [f32; 2],
    matrix: [f32; 16],
}

const IDENTITY4: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

impl Default for Visual {
    fn default() -> Visual {
        Visual {
            anchor: [0.0; 2],
            backface: 0,
            border: 0,
            center: [0.0; 3],
            composite: 0,
            visible: 1,
            offset: [0.0; 3],
            opacity: 1.0,
            orientation: [0.0, 0.0, 0.0, 1.0],
            rotation: 0.0,
            axis: [0.0, 0.0, 1.0],
            scale: [1.0; 3],
            size: [0.0; 2],
            matrix: IDENTITY4,
        }
    }
}

struct VisualData {
    p: Visual,
    clip: *mut c_void,
    brush: *mut c_void,
    /// Enfants de bas en haut (tenus).
    children: Vec<*mut Obj>,
    /// Parent (non tenu : il retire ses enfants en mourant).
    parent: *mut Obj,
}

struct Drawing {
    dc: *mut c_void,
    bitmap: *mut c_void,
    x: u32,
    y: u32,
}

struct SurfaceData {
    w: u32,
    h: u32,
    format: i32,
    alpha: i32,
    /// BGRA prémultiplié.
    px: Vec<u8>,
    device: *mut Obj,
    drawing: Option<Drawing>,
}

enum Data {
    None,
    Compositor {
        /// Cibles de fenêtre (non tenues : elles se retirent en mourant).
        targets: Vec<*mut Obj>,
        /// Contrôleur (non tenu) ; sans lui, chaque changement est rendu.
        controller: *mut Obj,
        handlers: Vec<(i64, usize)>,
        next_token: i64,
        dirty: bool,
    },
    Controller {
        compositor: *mut Obj,
    },
    Visual(Box<VisualData>),
    Collection {
        visual: *mut Obj,
    },
    ColorBrush {
        /// Windows.UI.Color : A, R, G, B.
        color: [u8; 4],
    },
    SurfaceBrush {
        surface: *mut c_void,
        interpolation: i32,
        h_ratio: f32,
        v_ratio: f32,
        stretch: i32,
    },
    InsetClip([f32; 4]),
    Target {
        hwnd: usize,
        topmost: bool,
        root: *mut Obj,
    },
    Device {
        render: *mut c_void,
        d2d: *mut c_void,
        next_token: i64,
    },
    Surface(Box<SurfaceData>),
    PropertySet(HashMap<String, (u8, Vec<u8>)>),
}

unsafe fn new_obj(kind: Kind, compositor: *mut Obj, data: Data) -> *mut Obj {
    let ifaces = kind.ifaces();
    if !compositor.is_null() {
        obj_add_ref(compositor);
    }
    let obj = Box::into_raw(Box::new(Obj {
        refs: AtomicU32::new(1),
        kind,
        entries: Box::new([]),
        compositor,
        st: Mutex::new(State {
            data,
            properties: std::ptr::null_mut(),
        }),
    }));
    (*obj).entries = ifaces
        .into_iter()
        .map(|(iid, vt)| Entry {
            slot: Slot { vt, owner: obj },
            iid,
        })
        .collect();
    obj
}

/// Interface par défaut (premier emplacement).
unsafe fn default_iface(obj: *mut Obj) -> *mut c_void {
    &(*obj).entries[0].slot as *const Slot as *mut c_void
}

unsafe fn iface(obj: *mut Obj, iid: &Guid) -> *mut c_void {
    (*obj)
        .entries
        .iter()
        .find(|e| e.iid == *iid)
        .map_or(std::ptr::null_mut(), |e| {
            &e.slot as *const Slot as *mut c_void
        })
}

unsafe fn obj_add_ref(obj: *mut Obj) {
    (*obj).refs.fetch_add(1, Ordering::AcqRel);
}

/// AddRef sauf si l'objet est déjà en train de mourir.
unsafe fn obj_try_add_ref(obj: *mut Obj) -> bool {
    let refs = &(*obj).refs;
    let mut n = refs.load(Ordering::Acquire);
    while n != 0 {
        match refs.compare_exchange(n, n + 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(m) => n = m,
        }
    }
    false
}

unsafe fn obj_release(obj: *mut Obj) -> u32 {
    let left = (*obj).refs.fetch_sub(1, Ordering::AcqRel).wrapping_sub(1);
    if left == 0 {
        destroy(obj);
    }
    left
}

/// Vrai si `p` est une interface d'un objet de ce module.
unsafe fn ours(p: *mut c_void) -> Option<*mut Obj> {
    if p.is_null() {
        return None;
    }
    let vt = *(p as *const *const ());
    if ALL_VTS.iter().any(|f| f() == vt) {
        Some((*(p as *const Slot)).owner)
    } else {
        None
    }
}

unsafe fn ours_of(p: *mut c_void, kinds: &[Kind]) -> Option<*mut Obj> {
    ours(p).filter(|o| !o.is_null() && kinds.contains(&(**o).kind))
}

unsafe fn destroy(obj: *mut Obj) {
    let (data, properties) = {
        let mut st = lock(&(*obj).st);
        (
            std::mem::replace(&mut st.data, Data::None),
            std::mem::replace(&mut st.properties, std::ptr::null_mut()),
        )
    };
    match data {
        Data::Controller { compositor } => {
            if let Data::Compositor { controller, .. } = &mut lock(&(*compositor).st).data {
                if *controller == obj {
                    *controller = std::ptr::null_mut();
                }
            }
            obj_release(compositor);
        }
        Data::Compositor { handlers, .. } => {
            for (_, h) in handlers {
                com_release(h as *mut c_void);
            }
        }
        Data::Visual(v) => {
            com_release(v.clip);
            com_release(v.brush);
            for c in v.children {
                if let Data::Visual(cv) = &mut lock(&(*c).st).data {
                    cv.parent = std::ptr::null_mut();
                }
                obj_release(c);
            }
        }
        Data::Collection { visual } => {
            obj_release(visual);
        }
        Data::SurfaceBrush { surface, .. } => com_release(surface),
        Data::Target { root, .. } => {
            let c = (*obj).compositor;
            if let Data::Compositor { targets, .. } = &mut lock(&(*c).st).data {
                targets.retain(|t| *t != obj);
            }
            if !root.is_null() {
                obj_release(root);
            }
        }
        Data::Device { render, d2d, .. } => {
            com_release(render);
            com_release(d2d);
        }
        Data::Surface(s) => {
            if let Some(d) = s.drawing {
                com_release(d.dc);
                com_release(d.bitmap);
            }
            if !s.device.is_null() {
                obj_release(s.device);
            }
        }
        _ => {}
    }
    if !properties.is_null() {
        obj_release(properties);
    }
    if !(*obj).compositor.is_null() {
        obj_release((*obj).compositor);
    }
    drop(Box::from_raw(obj));
}

unsafe fn owner(this: *mut Slot) -> *mut Obj {
    (*this).owner
}

unsafe fn put_obj(out: *mut *mut c_void, obj: *mut Obj) -> i32 {
    if out.is_null() {
        obj_release(obj);
        return E_POINTER;
    }
    *out = default_iface(obj);
    S_OK
}

/// Écrit l'interface `iid` d'un objet (AddRef) ou NULL.
unsafe fn put_iface(out: *mut *mut c_void, obj: *mut Obj, iid: &Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if obj.is_null() {
        return S_OK;
    }
    let p = iface(obj, iid);
    if p.is_null() {
        return E_NOINTERFACE;
    }
    obj_add_ref(obj);
    *out = p;
    S_OK
}

unsafe fn put_com(out: *mut *mut c_void, p: *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    com_add_ref(p);
    *out = p;
    S_OK
}

// ---------------------------------------------------------------------
// IUnknown, IInspectable, IClosable
// ---------------------------------------------------------------------

unsafe extern "win64" fn qi(this: *mut Slot, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(g) = read_guid(riid) else {
        return E_INVALIDARG;
    };
    let obj = owner(this);
    let p = if g == IID_IUNKNOWN || g == IID_IINSPECTABLE || g == IID_IAGILEOBJECT {
        default_iface(obj)
    } else {
        iface(obj, &g)
    };
    if p.is_null() {
        super::comobj::log_no_interface((*obj).kind.class(), riid);
        return E_NOINTERFACE;
    }
    obj_add_ref(obj);
    *out = p;
    S_OK
}

unsafe extern "win64" fn add_ref(this: *mut Slot) -> u32 {
    let obj = owner(this);
    (*obj).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn release(this: *mut Slot) -> u32 {
    obj_release(owner(this))
}

unsafe extern "win64" fn get_iids(_: *mut Slot, count: *mut u32, iids: *mut *mut Guid) -> i32 {
    if count.is_null() || iids.is_null() {
        return E_POINTER;
    }
    *count = 0;
    *iids = std::ptr::null_mut();
    S_OK
}

unsafe fn create_hstring(s: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

unsafe extern "win64" fn class_name(this: *mut Slot, out: *mut usize) -> i32 {
    create_hstring((*owner(this)).kind.class(), out)
}

unsafe extern "win64" fn trust_level(_: *mut Slot, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0; // BaseTrust
    S_OK
}

unsafe extern "win64" fn ok(_: *mut Slot) -> i32 {
    S_OK
}

const UNKNOWN: [*const (); 3] = [qi as *const (), add_ref as *const (), release as *const ()];
const INSPECTABLE: [*const (); 6] = [
    qi as *const (),
    add_ref as *const (),
    release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
];

/// Table : `base` puis les méthodes propres à l'interface.
const fn table<const N: usize>(base: &[*const ()], extra: &[*const ()]) -> [*const (); N] {
    assert!(base.len() + extra.len() == N, "taille de table");
    let mut out = [std::ptr::null::<()>(); N];
    let mut i = 0;
    while i < N {
        out[i] = if i < base.len() {
            base[i]
        } else {
            extra[i - base.len()]
        };
        i += 1;
    }
    out
}

static EMPTY_VT: VTable<6> = VTable(INSPECTABLE);
static CLOSABLE_VT: VTable<7> = VTable(table::<7>(&INSPECTABLE, &[ok as *const ()]));

// ---------------------------------------------------------------------
// ICompositionObject
// ---------------------------------------------------------------------

/// Compositeur d'un objet (lui-même pour le compositeur).
unsafe fn compositor_of(obj: *mut Obj) -> *mut Obj {
    match (*obj).kind {
        Kind::Compositor => obj,
        Kind::Controller => match lock(&(*obj).st).data {
            Data::Controller { compositor } => compositor,
            _ => std::ptr::null_mut(),
        },
        _ => (*obj).compositor,
    }
}

unsafe extern "win64" fn object_compositor(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    put_iface(out, compositor_of(owner(this)), &IID_COMPOSITOR)
}

unsafe extern "win64" fn object_dispatcher(_: *mut Slot, out: *mut *mut c_void) -> i32 {
    // CoreDispatcher : aucun sur le bureau.
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    S_OK
}

unsafe extern "win64" fn object_properties(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let obj = owner(this);
    let set = {
        let mut st = lock(&(*obj).st);
        if st.properties.is_null() {
            st.properties = new_obj(
                Kind::PropertySet,
                compositor_of(obj),
                Data::PropertySet(HashMap::new()),
            );
        }
        st.properties
    };
    put_iface(out, set, &IID_PROPERTY_SET)
}

static OBJECT_VT: VTable<11> = VTable(table::<11>(
    &INSPECTABLE,
    &[
        object_compositor as *const (),
        object_dispatcher as *const (),
        object_properties as *const (),
        unsupported!("ICompositionObject::StartAnimation"),
        unsupported!("ICompositionObject::StopAnimation"),
    ],
));

// ---------------------------------------------------------------------
// Fabriques d'activation
// ---------------------------------------------------------------------

#[repr(C)]
struct Factory {
    vt: *const (),
    class: Kind,
}
unsafe impl Sync for Factory {}

static CONTROLLER_FACTORY: Factory = Factory {
    vt: &FACTORY_VT as *const VTable<7> as *const (),
    class: Kind::Controller,
};
static COMPOSITOR_FACTORY: Factory = Factory {
    vt: &FACTORY_VT as *const VTable<7> as *const (),
    class: Kind::Compositor,
};

unsafe extern "win64" fn factory_qi(
    this: *mut Factory,
    riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(g) = read_guid(riid) else {
        return E_INVALIDARG;
    };
    if [
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
        IID_ACTIVATION_FACTORY,
    ]
    .contains(&g)
    {
        *out = this as *mut c_void;
        return S_OK;
    }
    super::comobj::log_no_interface((*this).class.class(), riid);
    E_NOINTERFACE
}

unsafe extern "win64" fn factory_ref(_: *mut Factory) -> u32 {
    1
}

unsafe extern "win64" fn factory_class_name(this: *mut Factory, out: *mut usize) -> i32 {
    create_hstring((*this).class.class(), out)
}

unsafe extern "win64" fn factory_activate(this: *mut Factory, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let obj = match (*this).class {
        Kind::Controller => new_controller(),
        _ => new_compositor(),
    };
    put_obj(out, obj)
}

static FACTORY_VT: VTable<7> = VTable([
    factory_qi as *const (),
    factory_ref as *const (),
    factory_ref as *const (),
    get_iids as *const (),
    factory_class_name as *const (),
    trust_level as *const (),
    factory_activate as *const (),
]);

unsafe fn new_compositor() -> *mut Obj {
    new_obj(
        Kind::Compositor,
        std::ptr::null_mut(),
        Data::Compositor {
            targets: Vec::new(),
            controller: std::ptr::null_mut(),
            handlers: Vec::new(),
            next_token: 1,
            dirty: false,
        },
    )
}

unsafe fn new_controller() -> *mut Obj {
    let compositor = new_compositor();
    let ctl = new_obj(
        Kind::Controller,
        std::ptr::null_mut(),
        Data::Controller { compositor },
    );
    if let Data::Compositor { controller, .. } = &mut lock(&(*compositor).st).data {
        *controller = ctl;
    }
    ctl
}

/// `RoGetActivationFactory` pour les classes de ce module : `None` si la
/// classe n'est pas d'ici.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    let f = match class {
        CONTROLLER_CLASS => &CONTROLLER_FACTORY,
        COMPOSITOR_CLASS => &COMPOSITOR_FACTORY,
        _ => return None,
    };
    Some(factory_qi(
        f as *const Factory as *mut Factory,
        iid,
        out as *mut *mut c_void,
    ))
}

// ---------------------------------------------------------------------
// Changements et validation
// ---------------------------------------------------------------------

/// Un changement visible : le contrôleur l'annonce (CommitNeeded) ; sans
/// contrôleur, le compositeur rend aussitôt.
unsafe fn changed(obj: *mut Obj) {
    let c = compositor_of(obj);
    if c.is_null() {
        return;
    }
    let (notify, sender, auto) = {
        let mut st = lock(&(*c).st);
        let Data::Compositor {
            controller,
            handlers,
            dirty,
            ..
        } = &mut st.data
        else {
            return;
        };
        if controller.is_null() {
            (Vec::new(), std::ptr::null_mut(), true)
        } else if *dirty {
            return;
        } else {
            *dirty = true;
            let hs: Vec<usize> = handlers.iter().map(|(_, h)| *h).collect();
            for h in &hs {
                com_add_ref(*h as *mut c_void);
            }
            let ctl = *controller;
            if !obj_try_add_ref(ctl) {
                for h in &hs {
                    com_release(*h as *mut c_void);
                }
                return;
            }
            (hs, ctl, false)
        }
    };
    if auto {
        commit(c);
        return;
    }
    // ITypedEventHandler<CompositorController, Object>::Invoke (emplacement 3),
    // appelé sans verrou tenu.
    let sender_iface = default_iface(sender);
    for h in notify {
        let f: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void) -> i32 =
            vcall(h as *mut c_void, 3);
        f(h as *mut c_void, sender_iface, std::ptr::null_mut());
        com_release(h as *mut c_void);
    }
    obj_release(sender);
}

/// Rend chaque cible de fenêtre du compositeur.
unsafe fn commit(c: *mut Obj) {
    let targets: Vec<*mut Obj> = {
        let mut st = lock(&(*c).st);
        let Data::Compositor { targets, dirty, .. } = &mut st.data else {
            return;
        };
        *dirty = false;
        targets
            .iter()
            .copied()
            .filter(|t| obj_try_add_ref(*t))
            .collect()
    };
    for t in targets {
        render_target(t);
        obj_release(t);
    }
}

// ---------------------------------------------------------------------
// CompositorController
// ---------------------------------------------------------------------

unsafe fn controller_compositor(this: *mut Slot) -> *mut Obj {
    match lock(&(*owner(this)).st).data {
        Data::Controller { compositor } => compositor,
        _ => std::ptr::null_mut(),
    }
}

unsafe extern "win64" fn controller_get_compositor(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    put_iface(out, controller_compositor(this), &IID_COMPOSITOR)
}

unsafe extern "win64" fn controller_commit(this: *mut Slot) -> i32 {
    let c = controller_compositor(this);
    if c.is_null() {
        return RO_E_CLOSED;
    }
    commit(c);
    S_OK
}

/// Le rendu est synchrone : l'opération rendue est déjà terminée.
unsafe extern "win64" fn controller_ensure_completed(_: *mut Slot, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    put_obj(out, new_obj(Kind::Async, std::ptr::null_mut(), Data::None))
}

unsafe extern "win64" fn controller_add_commit_needed(
    this: *mut Slot,
    handler: *mut c_void,
    token: *mut i64,
) -> i32 {
    if handler.is_null() || token.is_null() {
        return E_INVALIDARG;
    }
    let c = controller_compositor(this);
    let Data::Compositor {
        handlers,
        next_token,
        ..
    } = &mut lock(&(*c).st).data
    else {
        return RO_E_CLOSED;
    };
    com_add_ref(handler);
    handlers.push((*next_token, handler as usize));
    *token = *next_token;
    *next_token += 1;
    S_OK
}

unsafe extern "win64" fn controller_remove_commit_needed(this: *mut Slot, token: i64) -> i32 {
    let c = controller_compositor(this);
    let removed = {
        let Data::Compositor { handlers, .. } = &mut lock(&(*c).st).data else {
            return RO_E_CLOSED;
        };
        let pos = handlers.iter().position(|(t, _)| *t == token);
        pos.map(|i| handlers.remove(i).1)
    };
    if let Some(h) = removed {
        com_release(h as *mut c_void);
    }
    S_OK
}

static CONTROLLER_VT: VTable<11> = VTable(table::<11>(
    &INSPECTABLE,
    &[
        controller_get_compositor as *const (),
        controller_commit as *const (),
        controller_ensure_completed as *const (),
        controller_add_commit_needed as *const (),
        controller_remove_commit_needed as *const (),
    ],
));

// IAsyncAction déjà terminée -------------------------------------------

unsafe extern "win64" fn async_set_completed(this: *mut Slot, handler: *mut c_void) -> i32 {
    if handler.is_null() {
        return S_OK;
    }
    // AsyncActionCompletedHandler::Invoke(asyncInfo, AsyncStatus.Completed).
    let f: unsafe extern "win64" fn(*mut c_void, *mut c_void, i32) -> i32 = vcall(handler, 3);
    f(handler, this as *mut c_void, 1);
    S_OK
}

unsafe extern "win64" fn async_get_completed(_: *mut Slot, out: *mut *mut c_void) -> i32 {
    put_com(out, std::ptr::null_mut())
}

unsafe extern "win64" fn async_zero(_: *mut Slot, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}

unsafe extern "win64" fn async_status(_: *mut Slot, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 1; // Completed
    S_OK
}

static ASYNC_ACTION_VT: VTable<9> = VTable(table::<9>(
    &INSPECTABLE,
    &[
        async_set_completed as *const (),
        async_get_completed as *const (),
        ok as *const (), // GetResults
    ],
));

static ASYNC_INFO_VT: VTable<11> = VTable(table::<11>(
    &INSPECTABLE,
    &[
        async_zero as *const (), // Id
        async_status as *const (),
        async_zero as *const (), // ErrorCode
        ok as *const (),         // Cancel
        ok as *const (),         // Close
    ],
));

// ---------------------------------------------------------------------
// Compositor
// ---------------------------------------------------------------------

unsafe fn new_visual(c: *mut Obj, sprite: bool) -> *mut Obj {
    new_obj(
        if sprite {
            Kind::SpriteVisual
        } else {
            Kind::ContainerVisual
        },
        c,
        Data::Visual(Box::new(VisualData {
            p: Visual::default(),
            clip: std::ptr::null_mut(),
            brush: std::ptr::null_mut(),
            children: Vec::new(),
            parent: std::ptr::null_mut(),
        })),
    )
}

unsafe extern "win64" fn compositor_color_brush(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    // Couleur par défaut : noir opaque.
    compositor_color_brush_with(this, 0x0000_00FF, out)
}

unsafe extern "win64" fn compositor_color_brush_with(
    this: *mut Slot,
    color: u32,
    out: *mut *mut c_void,
) -> i32 {
    let obj = new_obj(
        Kind::ColorBrush,
        owner(this),
        Data::ColorBrush {
            color: color.to_le_bytes(),
        },
    );
    put_obj(out, obj)
}

unsafe extern "win64" fn compositor_container(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    put_obj(out, new_visual(owner(this), false))
}

unsafe extern "win64" fn compositor_sprite(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    put_obj(out, new_visual(owner(this), true))
}

unsafe extern "win64" fn compositor_inset_clip(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    put_obj(
        out,
        new_obj(Kind::InsetClip, owner(this), Data::InsetClip([0.0; 4])),
    )
}

unsafe extern "win64" fn compositor_inset_clip_with(
    this: *mut Slot,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    out: *mut *mut c_void,
) -> i32 {
    put_obj(
        out,
        new_obj(
            Kind::InsetClip,
            owner(this),
            Data::InsetClip([left, top, right, bottom]),
        ),
    )
}

unsafe extern "win64" fn compositor_property_set(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    put_obj(
        out,
        new_obj(
            Kind::PropertySet,
            owner(this),
            Data::PropertySet(HashMap::new()),
        ),
    )
}

unsafe extern "win64" fn compositor_surface_brush(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    compositor_surface_brush_with(this, std::ptr::null_mut(), out)
}

unsafe extern "win64" fn compositor_surface_brush_with(
    this: *mut Slot,
    surface: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    com_add_ref(surface);
    let obj = new_obj(
        Kind::SurfaceBrush,
        owner(this),
        Data::SurfaceBrush {
            surface,
            interpolation: 1,
            h_ratio: 0.5,
            v_ratio: 0.5,
            stretch: 1,
        },
    );
    put_obj(out, obj)
}

static COMPOSITOR_VT: VTable<30> = VTable(table::<30>(
    &INSPECTABLE,
    &[
        unsupported!("ICompositor::CreateColorKeyFrameAnimation"),
        compositor_color_brush as *const (),
        compositor_color_brush_with as *const (),
        compositor_container as *const (),
        unsupported!("ICompositor::CreateCubicBezierEasingFunction"),
        unsupported!("ICompositor::CreateEffectFactory"),
        unsupported!("ICompositor::CreateEffectFactoryWithProperties"),
        unsupported!("ICompositor::CreateExpressionAnimation"),
        unsupported!("ICompositor::CreateExpressionAnimationWithExpression"),
        compositor_inset_clip as *const (),
        compositor_inset_clip_with as *const (),
        unsupported!("ICompositor::CreateLinearEasingFunction"),
        compositor_property_set as *const (),
        unsupported!("ICompositor::CreateQuaternionKeyFrameAnimation"),
        unsupported!("ICompositor::CreateScalarKeyFrameAnimation"),
        unsupported!("ICompositor::CreateScopedBatch"),
        compositor_sprite as *const (),
        compositor_surface_brush as *const (),
        compositor_surface_brush_with as *const (),
        unsupported!("ICompositor::CreateTargetForCurrentView"),
        unsupported!("ICompositor::CreateVector2KeyFrameAnimation"),
        unsupported!("ICompositor::CreateVector3KeyFrameAnimation"),
        unsupported!("ICompositor::CreateVector4KeyFrameAnimation"),
        unsupported!("ICompositor::GetCommitBatch"),
    ],
));

// ICompositorDesktopInterop ---------------------------------------------

unsafe extern "win64" fn create_desktop_window_target(
    this: *mut Slot,
    hwnd: usize,
    topmost: i32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if hwnd == 0 {
        return E_INVALIDARG;
    }
    let c = owner(this);
    let t = new_obj(
        Kind::Target,
        c,
        Data::Target {
            hwnd,
            topmost: topmost != 0,
            root: std::ptr::null_mut(),
        },
    );
    if let Data::Compositor { targets, .. } = &mut lock(&(*c).st).data {
        targets.push(t);
    }
    put_obj(out, t)
}

unsafe extern "win64" fn ensure_on_thread(_: *mut Slot, _thread: u32) -> i32 {
    S_OK
}

static DESKTOP_INTEROP_VT: VTable<5> = VTable(table::<5>(
    &UNKNOWN,
    &[
        create_desktop_window_target as *const (),
        ensure_on_thread as *const (),
    ],
));

// ICompositorInterop ------------------------------------------------------

unsafe extern "win64" fn create_graphics_device(
    this: *mut Slot,
    render: *mut c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if render.is_null() {
        return E_INVALIDARG;
    }
    let d2d = super::d2d1::device_for(render);
    if d2d.is_null() {
        crate::log_warn!(
            "composition",
            "CreateGraphicsDevice : appareil de rendu sans Direct2D"
        );
    }
    com_add_ref(render);
    let obj = new_obj(
        Kind::Device,
        owner(this),
        Data::Device {
            render,
            d2d,
            next_token: 1,
        },
    );
    put_obj(out, obj)
}

static COMPOSITOR_INTEROP_VT: VTable<6> = VTable(table::<6>(
    &UNKNOWN,
    &[
        unsupported!("ICompositorInterop::CreateCompositionSurfaceForHandle"),
        unsupported!("ICompositorInterop::CreateCompositionSurfaceForSwapChain"),
        create_graphics_device as *const (),
    ],
));

// ---------------------------------------------------------------------
// Visuels
// ---------------------------------------------------------------------

/// Lit (`f(&mut v)`) les données du visuel portant `this`.
unsafe fn with_visual<R>(obj: *mut Obj, f: impl FnOnce(&mut VisualData) -> R) -> Option<R> {
    match &mut lock(&(*obj).st).data {
        Data::Visual(v) => Some(f(v)),
        _ => None,
    }
}

macro_rules! visual_prop {
    // Vector2 : 8 octets passés dans un registre.
    ($get:ident, $set:ident, $field:ident, v2) => {
        unsafe extern "win64" fn $get(this: *mut Slot, out: *mut [f32; 2]) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match with_visual(owner(this), |v| v.p.$field) {
                Some(x) => {
                    out.write_unaligned(x);
                    S_OK
                }
                None => E_FAIL,
            }
        }
        unsafe extern "win64" fn $set(this: *mut Slot, value: u64) -> i32 {
            let v = [
                f32::from_bits(value as u32),
                f32::from_bits((value >> 32) as u32),
            ];
            set_visual(this, |p| p.$field = v)
        }
    };
    // Structure de plus de 8 octets : passée par adresse.
    ($get:ident, $set:ident, $field:ident, ref $t:ty) => {
        unsafe extern "win64" fn $get(this: *mut Slot, out: *mut $t) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match with_visual(owner(this), |v| v.p.$field) {
                Some(x) => {
                    out.write_unaligned(x);
                    S_OK
                }
                None => E_FAIL,
            }
        }
        unsafe extern "win64" fn $set(this: *mut Slot, value: *const $t) -> i32 {
            if value.is_null() {
                return E_POINTER;
            }
            let v = value.read_unaligned();
            set_visual(this, |p| p.$field = v)
        }
    };
    // Valeur passée telle quelle (f32, énumération, booléen).
    ($get:ident, $set:ident, $field:ident, $t:ty) => {
        unsafe extern "win64" fn $get(this: *mut Slot, out: *mut $t) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match with_visual(owner(this), |v| v.p.$field) {
                Some(x) => {
                    out.write_unaligned(x);
                    S_OK
                }
                None => E_FAIL,
            }
        }
        unsafe extern "win64" fn $set(this: *mut Slot, value: $t) -> i32 {
            set_visual(this, |p| p.$field = value)
        }
    };
}

unsafe fn set_visual(this: *mut Slot, f: impl FnOnce(&mut Visual)) -> i32 {
    let obj = owner(this);
    let Some(changed_now) = with_visual(obj, |v| {
        let before = v.p;
        f(&mut v.p);
        before != v.p
    }) else {
        return E_FAIL;
    };
    if changed_now {
        changed(obj);
    }
    S_OK
}

visual_prop!(get_anchor, set_anchor, anchor, v2);
visual_prop!(get_backface, set_backface, backface, i32);
visual_prop!(get_border, set_border, border, i32);
visual_prop!(get_center, set_center, center, ref [f32; 3]);
visual_prop!(get_composite, set_composite, composite, i32);
visual_prop!(get_visible, set_visible, visible, u8);
visual_prop!(get_offset, set_offset, offset, ref [f32; 3]);
visual_prop!(get_opacity, set_opacity, opacity, f32);
visual_prop!(get_orientation, set_orientation, orientation, ref [f32; 4]);
visual_prop!(get_rotation, set_rotation, rotation, f32);
visual_prop!(get_axis, set_axis, axis, ref [f32; 3]);
visual_prop!(get_scale, set_scale, scale, ref [f32; 3]);
visual_prop!(get_size, set_size, size, v2);
visual_prop!(get_matrix, set_matrix, matrix, ref [f32; 16]);

unsafe extern "win64" fn get_rotation_degrees(this: *mut Slot, out: *mut f32) -> i32 {
    let hr = get_rotation(this, out);
    if hr == S_OK {
        *out = out.read().to_degrees();
    }
    hr
}

unsafe extern "win64" fn set_rotation_degrees(this: *mut Slot, deg: f32) -> i32 {
    set_rotation(this, deg.to_radians())
}

unsafe extern "win64" fn get_clip(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let clip = with_visual(owner(this), |v| v.clip).unwrap_or(std::ptr::null_mut());
    put_com(out, clip)
}

unsafe extern "win64" fn set_clip(this: *mut Slot, clip: *mut c_void) -> i32 {
    let obj = owner(this);
    com_add_ref(clip);
    let old = with_visual(obj, |v| std::mem::replace(&mut v.clip, clip));
    com_release(old.unwrap_or(clip));
    if old.is_none() {
        return E_FAIL;
    }
    changed(obj);
    S_OK
}

unsafe extern "win64" fn get_parent(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let parent = with_visual(owner(this), |v| v.parent).unwrap_or(std::ptr::null_mut());
    put_iface(out, parent, &IID_CONTAINER)
}

static VISUAL_VT: VTable<39> = VTable(table::<39>(
    &INSPECTABLE,
    &[
        get_anchor as *const (),
        set_anchor as *const (),
        get_backface as *const (),
        set_backface as *const (),
        get_border as *const (),
        set_border as *const (),
        get_center as *const (),
        set_center as *const (),
        get_clip as *const (),
        set_clip as *const (),
        get_composite as *const (),
        set_composite as *const (),
        get_visible as *const (),
        set_visible as *const (),
        get_offset as *const (),
        set_offset as *const (),
        get_opacity as *const (),
        set_opacity as *const (),
        get_orientation as *const (),
        set_orientation as *const (),
        get_parent as *const (),
        get_rotation as *const (),
        set_rotation as *const (),
        get_rotation_degrees as *const (),
        set_rotation_degrees as *const (),
        get_axis as *const (),
        set_axis as *const (),
        get_scale as *const (),
        set_scale as *const (),
        get_size as *const (),
        set_size as *const (),
        get_matrix as *const (),
        set_matrix as *const (),
    ],
));

unsafe extern "win64" fn container_children(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let obj = owner(this);
    obj_add_ref(obj);
    put_obj(
        out,
        new_obj(
            Kind::Collection,
            (*obj).compositor,
            Data::Collection { visual: obj },
        ),
    )
}

static CONTAINER_VT: VTable<7> =
    VTable(table::<7>(&INSPECTABLE, &[container_children as *const ()]));

unsafe extern "win64" fn sprite_get_brush(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let brush = with_visual(owner(this), |v| v.brush).unwrap_or(std::ptr::null_mut());
    put_com(out, brush)
}

unsafe extern "win64" fn sprite_set_brush(this: *mut Slot, brush: *mut c_void) -> i32 {
    let obj = owner(this);
    if !brush.is_null() && ours_of(brush, &[Kind::ColorBrush, Kind::SurfaceBrush]).is_none() {
        crate::log_warn!(
            "composition",
            "SpriteVisual.Brush : pinceau non pris en charge, rien ne sera dessiné"
        );
    }
    com_add_ref(brush);
    let old = with_visual(obj, |v| std::mem::replace(&mut v.brush, brush));
    com_release(old.unwrap_or(brush));
    if old.is_none() {
        return E_FAIL;
    }
    changed(obj);
    S_OK
}

static SPRITE_VT: VTable<8> = VTable(table::<8>(
    &INSPECTABLE,
    &[sprite_get_brush as *const (), sprite_set_brush as *const ()],
));

// VisualCollection --------------------------------------------------------

unsafe fn collection_visual(this: *mut Slot) -> *mut Obj {
    match lock(&(*owner(this)).st).data {
        Data::Collection { visual } => visual,
        _ => std::ptr::null_mut(),
    }
}

unsafe extern "win64" fn collection_count(this: *mut Slot, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_visual(collection_visual(this), |v| v.children.len() as i32).unwrap_or(0);
    S_OK
}

/// Où insérer un enfant : en bas, en haut, au-dessus ou au-dessous d'un frère.
enum At {
    Bottom,
    Top,
    Above(*mut Obj),
    Below(*mut Obj),
}

unsafe fn insert_child(this: *mut Slot, child: *mut c_void, at: At) -> i32 {
    let parent = collection_visual(this);
    let Some(child) = ours_of(child, &[Kind::ContainerVisual, Kind::SpriteVisual]) else {
        return E_INVALIDARG;
    };
    if child == parent {
        return E_INVALIDARG;
    }
    let already = with_visual(child, |v| !v.parent.is_null()).unwrap_or(true);
    if already {
        return E_INVALIDARG;
    }
    let ok_insert = with_visual(parent, |v| {
        let i = match at {
            At::Bottom => 0,
            At::Top => v.children.len(),
            At::Above(s) => match v.children.iter().position(|c| *c == s) {
                Some(i) => i + 1,
                None => return false,
            },
            At::Below(s) => match v.children.iter().position(|c| *c == s) {
                Some(i) => i,
                None => return false,
            },
        };
        v.children.insert(i, child);
        true
    })
    .unwrap_or(false);
    if !ok_insert {
        return E_INVALIDARG;
    }
    obj_add_ref(child);
    with_visual(child, |v| v.parent = parent);
    changed(parent);
    S_OK
}

unsafe extern "win64" fn collection_insert_above(
    this: *mut Slot,
    child: *mut c_void,
    sibling: *mut c_void,
) -> i32 {
    match ours(sibling) {
        Some(s) => insert_child(this, child, At::Above(s)),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn collection_insert_below(
    this: *mut Slot,
    child: *mut c_void,
    sibling: *mut c_void,
) -> i32 {
    match ours(sibling) {
        Some(s) => insert_child(this, child, At::Below(s)),
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn collection_insert_bottom(this: *mut Slot, child: *mut c_void) -> i32 {
    insert_child(this, child, At::Bottom)
}

unsafe extern "win64" fn collection_insert_top(this: *mut Slot, child: *mut c_void) -> i32 {
    insert_child(this, child, At::Top)
}

unsafe fn detach(parent: *mut Obj, children: Vec<*mut Obj>) {
    for c in children {
        with_visual(c, |v| v.parent = std::ptr::null_mut());
        obj_release(c);
    }
    changed(parent);
}

unsafe extern "win64" fn collection_remove(this: *mut Slot, child: *mut c_void) -> i32 {
    let parent = collection_visual(this);
    let Some(child) = ours(child) else {
        return E_INVALIDARG;
    };
    let removed = with_visual(parent, |v| {
        let pos = v.children.iter().position(|c| *c == child);
        pos.map(|i| v.children.remove(i))
    })
    .flatten();
    match removed {
        Some(c) => {
            detach(parent, vec![c]);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn collection_remove_all(this: *mut Slot) -> i32 {
    let parent = collection_visual(this);
    let all = with_visual(parent, |v| std::mem::take(&mut v.children)).unwrap_or_default();
    detach(parent, all);
    S_OK
}

static COLLECTION_VT: VTable<13> = VTable(table::<13>(
    &INSPECTABLE,
    &[
        collection_count as *const (),
        collection_insert_above as *const (),
        collection_insert_bottom as *const (),
        collection_insert_top as *const (),
        collection_insert_below as *const (),
        collection_remove as *const (),
        collection_remove_all as *const (),
    ],
));

// ---------------------------------------------------------------------
// Pinceaux et découpes
// ---------------------------------------------------------------------

unsafe extern "win64" fn color_brush_get(this: *mut Slot, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match lock(&(*owner(this)).st).data {
        Data::ColorBrush { color } => {
            *out = u32::from_le_bytes(color);
            S_OK
        }
        _ => E_FAIL,
    }
}

unsafe extern "win64" fn color_brush_set(this: *mut Slot, value: u32) -> i32 {
    let obj = owner(this);
    if let Data::ColorBrush { color } = &mut lock(&(*obj).st).data {
        *color = value.to_le_bytes();
    }
    changed(obj);
    S_OK
}

static COLOR_BRUSH_VT: VTable<8> = VTable(table::<8>(
    &INSPECTABLE,
    &[color_brush_get as *const (), color_brush_set as *const ()],
));

macro_rules! brush_prop {
    ($get:ident, $set:ident, $field:ident, $t:ty) => {
        unsafe extern "win64" fn $get(this: *mut Slot, out: *mut $t) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match lock(&(*owner(this)).st).data {
                Data::SurfaceBrush { $field, .. } => {
                    *out = $field;
                    S_OK
                }
                _ => E_FAIL,
            }
        }
        unsafe extern "win64" fn $set(this: *mut Slot, value: $t) -> i32 {
            let obj = owner(this);
            if let Data::SurfaceBrush { $field, .. } = &mut lock(&(*obj).st).data {
                *$field = value;
            }
            changed(obj);
            S_OK
        }
    };
}

brush_prop!(brush_get_interp, brush_set_interp, interpolation, i32);
brush_prop!(brush_get_h, brush_set_h, h_ratio, f32);
brush_prop!(brush_get_stretch, brush_set_stretch, stretch, i32);
brush_prop!(brush_get_v, brush_set_v, v_ratio, f32);

unsafe extern "win64" fn brush_get_surface(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let s = match lock(&(*owner(this)).st).data {
        Data::SurfaceBrush { surface, .. } => surface,
        _ => std::ptr::null_mut(),
    };
    put_com(out, s)
}

unsafe extern "win64" fn brush_set_surface(this: *mut Slot, value: *mut c_void) -> i32 {
    let obj = owner(this);
    if !value.is_null() && ours_of(value, &[Kind::Surface]).is_none() {
        crate::log_warn!(
            "composition",
            "CompositionSurfaceBrush.Surface : surface non prise en charge"
        );
    }
    com_add_ref(value);
    let old = match &mut lock(&(*obj).st).data {
        Data::SurfaceBrush { surface, .. } => std::mem::replace(surface, value),
        _ => value,
    };
    com_release(old);
    changed(obj);
    S_OK
}

static SURFACE_BRUSH_VT: VTable<16> = VTable(table::<16>(
    &INSPECTABLE,
    &[
        brush_get_interp as *const (),
        brush_set_interp as *const (),
        brush_get_h as *const (),
        brush_set_h as *const (),
        brush_get_stretch as *const (),
        brush_set_stretch as *const (),
        brush_get_surface as *const (),
        brush_set_surface as *const (),
        brush_get_v as *const (),
        brush_set_v as *const (),
    ],
));

macro_rules! inset_prop {
    ($get:ident, $set:ident, $i:expr) => {
        unsafe extern "win64" fn $get(this: *mut Slot, out: *mut f32) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            match lock(&(*owner(this)).st).data {
                Data::InsetClip(m) => {
                    *out = m[$i];
                    S_OK
                }
                _ => E_FAIL,
            }
        }
        unsafe extern "win64" fn $set(this: *mut Slot, value: f32) -> i32 {
            let obj = owner(this);
            if let Data::InsetClip(m) = &mut lock(&(*obj).st).data {
                m[$i] = value;
            }
            changed(obj);
            S_OK
        }
    };
}

// Marges rangées gauche, haut, droite, bas.
inset_prop!(inset_get_bottom, inset_set_bottom, 3);
inset_prop!(inset_get_left, inset_set_left, 0);
inset_prop!(inset_get_right, inset_set_right, 2);
inset_prop!(inset_get_top, inset_set_top, 1);

static INSET_CLIP_VT: VTable<14> = VTable(table::<14>(
    &INSPECTABLE,
    &[
        inset_get_bottom as *const (),
        inset_set_bottom as *const (),
        inset_get_left as *const (),
        inset_set_left as *const (),
        inset_get_right as *const (),
        inset_set_right as *const (),
        inset_get_top as *const (),
        inset_set_top as *const (),
    ],
));

// ---------------------------------------------------------------------
// Cible de fenêtre
// ---------------------------------------------------------------------

unsafe extern "win64" fn target_topmost(this: *mut Slot, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = match lock(&(*owner(this)).st).data {
        Data::Target { topmost, .. } => topmost as u8,
        _ => 0,
    };
    S_OK
}

unsafe extern "win64" fn target_get_root(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let root = match lock(&(*owner(this)).st).data {
        Data::Target { root, .. } => root,
        _ => std::ptr::null_mut(),
    };
    put_iface(out, root, &IID_VISUAL)
}

unsafe extern "win64" fn target_set_root(this: *mut Slot, value: *mut c_void) -> i32 {
    let obj = owner(this);
    let new_root = if value.is_null() {
        std::ptr::null_mut()
    } else {
        match ours_of(value, &[Kind::ContainerVisual, Kind::SpriteVisual]) {
            Some(v) => v,
            None => return E_INVALIDARG,
        }
    };
    if !new_root.is_null() {
        obj_add_ref(new_root);
    }
    let old = match &mut lock(&(*obj).st).data {
        Data::Target { root, .. } => std::mem::replace(root, new_root),
        _ => std::ptr::null_mut(),
    };
    if !old.is_null() {
        obj_release(old);
    }
    changed(obj);
    S_OK
}

static DESKTOP_TARGET_VT: VTable<7> =
    VTable(table::<7>(&INSPECTABLE, &[target_topmost as *const ()]));

static TARGET_VT: VTable<8> = VTable(table::<8>(
    &INSPECTABLE,
    &[target_get_root as *const (), target_set_root as *const ()],
));

// ---------------------------------------------------------------------
// Appareil graphique et surfaces de dessin
// ---------------------------------------------------------------------

const DXGI_FORMAT_B8G8R8A8_UNORM: i32 = 87;

unsafe fn new_surface(this: *mut Slot, w: u32, h: u32, format: i32, alpha: i32) -> *mut Obj {
    let device = owner(this);
    if format != DXGI_FORMAT_B8G8R8A8_UNORM && format != 0 {
        crate::log_warn!(
            "composition",
            "CreateDrawingSurface : format DXGI {} traité comme BGRA 8 bits",
            format
        );
    }
    obj_add_ref(device);
    new_obj(
        Kind::Surface,
        (*device).compositor,
        Data::Surface(Box::new(SurfaceData {
            w,
            h,
            format,
            alpha,
            px: vec![0; w as usize * h as usize * 4],
            device,
            drawing: None,
        })),
    )
}

/// Taille en pixels d'une Size (deux f32), arrondie à l'entier supérieur.
fn size_px(v: f32) -> Option<u32> {
    if !(0.0..=16384.0).contains(&v) {
        return None;
    }
    Some(v.ceil() as u32)
}

unsafe extern "win64" fn device_create_surface(
    this: *mut Slot,
    size: u64,
    format: i32,
    alpha: i32,
    out: *mut *mut c_void,
) -> i32 {
    let (w, h) = (
        size_px(f32::from_bits(size as u32)),
        size_px(f32::from_bits((size >> 32) as u32)),
    );
    let (Some(w), Some(h)) = (w, h) else {
        return E_INVALIDARG;
    };
    put_obj(out, new_surface(this, w, h, format, alpha))
}

unsafe extern "win64" fn device_create_surface2(
    this: *mut Slot,
    size: u64,
    format: i32,
    alpha: i32,
    out: *mut *mut c_void,
) -> i32 {
    let (w, h) = (size as u32 as i32, (size >> 32) as u32 as i32);
    if !(0..=16384).contains(&w) || !(0..=16384).contains(&h) {
        return E_INVALIDARG;
    }
    put_obj(out, new_surface(this, w as u32, h as u32, format, alpha))
}

unsafe extern "win64" fn device_add_replaced(
    this: *mut Slot,
    _handler: *mut c_void,
    token: *mut i64,
) -> i32 {
    // L'appareil logiciel n'est jamais perdu : le gestionnaire ne sert pas.
    if token.is_null() {
        return E_POINTER;
    }
    if let Data::Device { next_token, .. } = &mut lock(&(*owner(this)).st).data {
        *token = *next_token;
        *next_token += 1;
    }
    S_OK
}

unsafe extern "win64" fn device_remove_replaced(_: *mut Slot, _token: i64) -> i32 {
    S_OK
}

static DEVICE_VT: VTable<9> = VTable(table::<9>(
    &INSPECTABLE,
    &[
        device_create_surface as *const (),
        device_add_replaced as *const (),
        device_remove_replaced as *const (),
    ],
));

static DEVICE2_VT: VTable<8> = VTable(table::<8>(
    &INSPECTABLE,
    &[
        device_create_surface2 as *const (),
        unsupported!("ICompositionGraphicsDevice2::CreateVirtualDrawingSurface"),
    ],
));

unsafe extern "win64" fn device_get_rendering(this: *mut Slot, out: *mut *mut c_void) -> i32 {
    let render = match lock(&(*owner(this)).st).data {
        Data::Device { render, .. } => render,
        _ => std::ptr::null_mut(),
    };
    put_com(out, render)
}

unsafe extern "win64" fn device_set_rendering(this: *mut Slot, value: *mut c_void) -> i32 {
    if value.is_null() {
        return E_INVALIDARG;
    }
    let d2d = super::d2d1::device_for(value);
    com_add_ref(value);
    let old = match &mut lock(&(*owner(this)).st).data {
        Data::Device {
            render, d2d: dev, ..
        } => Some((
            std::mem::replace(render, value),
            std::mem::replace(dev, d2d),
        )),
        _ => None,
    };
    if let Some((r, d)) = old {
        com_release(r);
        com_release(d);
    }
    S_OK
}

static DEVICE_INTEROP_VT: VTable<5> = VTable(table::<5>(
    &UNKNOWN,
    &[
        device_get_rendering as *const (),
        device_set_rendering as *const (),
    ],
));

unsafe fn with_surface<R>(this: *mut Slot, f: impl FnOnce(&mut SurfaceData) -> R) -> Option<R> {
    match &mut lock(&(*owner(this)).st).data {
        Data::Surface(s) => Some(f(s)),
        _ => None,
    }
}

unsafe extern "win64" fn surface_alpha(this: *mut Slot, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_surface(this, |s| s.alpha).unwrap_or(0);
    S_OK
}

unsafe extern "win64" fn surface_format(this: *mut Slot, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_surface(this, |s| s.format).unwrap_or(0);
    S_OK
}

unsafe extern "win64" fn surface_size(this: *mut Slot, out: *mut [f32; 2]) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_surface(this, |s| [s.w as f32, s.h as f32]).unwrap_or([0.0; 2]);
    S_OK
}

unsafe extern "win64" fn surface_size_int(this: *mut Slot, out: *mut [i32; 2]) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = with_surface(this, |s| [s.w as i32, s.h as i32]).unwrap_or([0; 2]);
    S_OK
}

unsafe extern "win64" fn surface_resize(this: *mut Slot, size: u64) -> i32 {
    let (w, h) = (size as u32 as i32, (size >> 32) as u32 as i32);
    if !(0..=16384).contains(&w) || !(0..=16384).contains(&h) {
        return E_INVALIDARG;
    }
    let (w, h) = (w as u32, h as u32);
    let done = with_surface(this, |s| {
        if s.drawing.is_some() {
            return false;
        }
        let mut px = vec![0u8; w as usize * h as usize * 4];
        let row = w.min(s.w) as usize * 4;
        for y in 0..h.min(s.h) as usize {
            let src = y * s.w as usize * 4;
            let dst = y * w as usize * 4;
            px[dst..dst + row].copy_from_slice(&s.px[src..src + row]);
        }
        s.px = px;
        s.w = w;
        s.h = h;
        true
    });
    match done {
        Some(true) => {
            changed(owner(this));
            S_OK
        }
        _ => E_FAIL,
    }
}

static DRAWING_SURFACE_VT: VTable<9> = VTable(table::<9>(
    &INSPECTABLE,
    &[
        surface_alpha as *const (),
        surface_format as *const (),
        surface_size as *const (),
    ],
));

static DRAWING_SURFACE2_VT: VTable<12> = VTable(table::<12>(
    &INSPECTABLE,
    &[
        surface_size_int as *const (),
        surface_resize as *const (),
        unsupported!("ICompositionDrawingSurface2::Scroll"),
        unsupported!("ICompositionDrawingSurface2::ScrollRect"),
        unsupported!("ICompositionDrawingSurface2::ScrollWithClip"),
        unsupported!("ICompositionDrawingSurface2::ScrollRectWithClip"),
    ],
));

/// ICompositionDrawingSurfaceInterop::BeginDraw : un contexte Direct2D
/// dessine dans une bitmap de la taille du rectangle, chargée avec le
/// contenu actuel ; EndDraw recopie la bitmap dans la surface.
unsafe extern "win64" fn interop_begin_draw(
    this: *mut Slot,
    rect: *const [i32; 4],
    riid: *const u8,
    out: *mut *mut c_void,
    offset: *mut [i32; 2],
) -> i32 {
    if out.is_null() || riid.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(want) = read_guid(riid) else {
        return E_INVALIDARG;
    };
    let surface = owner(this);
    // Rectangle, pixels de départ et appareil Direct2D.
    let prep = {
        let st = lock(&(*surface).st);
        let Data::Surface(s) = &st.data else {
            return E_FAIL;
        };
        if s.drawing.is_some() {
            return E_FAIL;
        }
        let [l, t, r, b] = rect
            .as_ref()
            .copied()
            .unwrap_or([0, 0, s.w as i32, s.h as i32]);
        if l < 0 || t < 0 || r <= l || b <= t || r as u32 > s.w || b as u32 > s.h {
            return E_INVALIDARG;
        }
        let (l, t, w, h) = (l as u32, t as u32, (r - l) as u32, (b - t) as u32);
        let mut px = Vec::with_capacity(w as usize * h as usize * 4);
        for y in t..t + h {
            let i = (y as usize * s.w as usize + l as usize) * 4;
            px.extend_from_slice(&s.px[i..i + w as usize * 4]);
        }
        let d2d = match lock(&(*s.device).st).data {
            Data::Device { d2d, .. } => d2d,
            _ => std::ptr::null_mut(),
        };
        com_add_ref(d2d);
        (l, t, w, h, px, d2d)
    };
    let (l, t, w, h, px, d2d) = prep;
    if d2d.is_null() {
        crate::log_warn!(
            "composition",
            "BeginDraw : l'appareil graphique n'a pas d'appareil Direct2D"
        );
        return E_FAIL;
    }
    // ID2D1Device::CreateDeviceContext (4).
    let mut dc = std::ptr::null_mut();
    let hr = vcall::<unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32>(d2d, 4)(
        d2d, 0, &mut dc,
    );
    com_release(d2d);
    if hr < 0 || dc.is_null() {
        return if hr < 0 { hr } else { E_FAIL };
    }
    // ID2D1DeviceContext::CreateBitmap (57) : BGRA prémultiplié, cible.
    #[repr(C)]
    struct BitmapProps1 {
        format: u32,
        alpha: u32,
        dpi_x: f32,
        dpi_y: f32,
        options: u32,
        color_context: *mut c_void,
    }
    let props = BitmapProps1 {
        format: 87,
        alpha: 1,
        dpi_x: 96.0,
        dpi_y: 96.0,
        options: 1, // D2D1_BITMAP_OPTIONS_TARGET
        color_context: std::ptr::null_mut(),
    };
    let mut bitmap = std::ptr::null_mut();
    let hr = vcall::<
        unsafe extern "win64" fn(
            *mut c_void,
            u64,
            *const u8,
            u32,
            *const BitmapProps1,
            *mut *mut c_void,
        ) -> i32,
    >(dc, 57)(
        dc,
        w as u64 | (h as u64) << 32,
        px.as_ptr(),
        w * 4,
        &props,
        &mut bitmap,
    );
    if hr < 0 || bitmap.is_null() {
        com_release(dc);
        return if hr < 0 { hr } else { E_FAIL };
    }
    // SetTarget (74), BeginDraw (48).
    vcall::<unsafe extern "win64" fn(*mut c_void, *mut c_void)>(dc, 74)(dc, bitmap);
    vcall::<unsafe extern "win64" fn(*mut c_void)>(dc, 48)(dc);
    let iface = com_qi(dc, &want);
    if iface.is_null() {
        crate::log_warn!(
            "composition",
            "BeginDraw : interface {} non fournie (seul Direct2D est pris en charge)",
            super::comobj::guid_str(&want)
        );
        com_release(bitmap);
        com_release(dc);
        return E_NOINTERFACE;
    }
    with_surface(this, |s| {
        s.drawing = Some(Drawing {
            dc,
            bitmap,
            x: l,
            y: t,
        })
    });
    if !offset.is_null() {
        *offset = [0, 0];
    }
    *out = iface;
    S_OK
}

unsafe extern "win64" fn interop_end_draw(this: *mut Slot) -> i32 {
    let Some(Some(d)) = with_surface(this, |s| s.drawing.take()) else {
        return E_FAIL;
    };
    // ID2D1RenderTarget::EndDraw (49).
    let hr = vcall::<unsafe extern "win64" fn(*mut c_void, *mut u64, *mut u64) -> i32>(d.dc, 49)(
        d.dc,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    vcall::<unsafe extern "win64" fn(*mut c_void, *mut c_void)>(d.dc, 74)(
        d.dc,
        std::ptr::null_mut(),
    );
    if let Some((bw, bh, px)) = super::d2d1::bitmap_pixels(d.bitmap) {
        with_surface(this, |s| {
            let w = bw.min(s.w.saturating_sub(d.x)) as usize;
            for y in 0..bh.min(s.h.saturating_sub(d.y)) as usize {
                let src = y * bw as usize * 4;
                let dst = ((d.y as usize + y) * s.w as usize + d.x as usize) * 4;
                s.px[dst..dst + w * 4].copy_from_slice(&px[src..src + w * 4]);
            }
        });
    }
    com_release(d.bitmap);
    com_release(d.dc);
    changed(owner(this));
    if hr < 0 {
        hr
    } else {
        S_OK
    }
}

unsafe extern "win64" fn interop_resize(this: *mut Slot, size: u64) -> i32 {
    surface_resize(this, size)
}

static SURFACE_INTEROP_VT: VTable<9> = VTable(table::<9>(
    &UNKNOWN,
    &[
        interop_begin_draw as *const (),
        interop_end_draw as *const (),
        interop_resize as *const (),
        unsupported!("ICompositionDrawingSurfaceInterop::Scroll"),
        ok as *const (), // ResumeDraw
        ok as *const (), // SuspendDraw
    ],
));

// ---------------------------------------------------------------------
// CompositionPropertySet
// ---------------------------------------------------------------------

unsafe fn property_insert(this: *mut Slot, name: usize, tag: u8, bytes: &[u8]) -> i32 {
    let key = hstring(name);
    if key.is_empty() {
        return E_INVALIDARG;
    }
    if let Data::PropertySet(m) = &mut lock(&(*owner(this)).st).data {
        m.insert(key, (tag, bytes.to_vec()));
    }
    S_OK
}

/// CompositionGetValueStatus : Succeeded 0, TypeMismatch 1, NotFound 2.
unsafe fn property_get(
    this: *mut Slot,
    name: usize,
    tag: u8,
    out: *mut u8,
    len: usize,
    status: *mut i32,
) -> i32 {
    if out.is_null() || status.is_null() {
        return E_POINTER;
    }
    let key = hstring(name);
    let st = lock(&(*owner(this)).st);
    let Data::PropertySet(m) = &st.data else {
        return E_FAIL;
    };
    *status = match m.get(&key) {
        None => 2,
        Some((t, _)) if *t != tag => 1,
        Some((_, v)) => {
            std::ptr::copy_nonoverlapping(v.as_ptr(), out, len.min(v.len()));
            0
        }
    };
    S_OK
}

macro_rules! property {
    // Structure de plus de 8 octets passée par adresse.
    ($ins:ident, $get:ident, $tag:expr, ref $n:expr) => {
        unsafe extern "win64" fn $ins(this: *mut Slot, name: usize, value: *const [u8; $n]) -> i32 {
            if value.is_null() {
                return E_POINTER;
            }
            property_insert(this, name, $tag, &value.read_unaligned())
        }
        unsafe extern "win64" fn $get(
            this: *mut Slot,
            name: usize,
            out: *mut [u8; $n],
            status: *mut i32,
        ) -> i32 {
            property_get(this, name, $tag, out as *mut u8, $n, status)
        }
    };
    // Valeur de 4 ou 8 octets passée dans un registre.
    ($ins:ident, $get:ident, $tag:expr, $t:ty) => {
        unsafe extern "win64" fn $ins(this: *mut Slot, name: usize, value: $t) -> i32 {
            property_insert(this, name, $tag, &value.to_le_bytes())
        }
        unsafe extern "win64" fn $get(
            this: *mut Slot,
            name: usize,
            out: *mut $t,
            status: *mut i32,
        ) -> i32 {
            property_get(
                this,
                name,
                $tag,
                out as *mut u8,
                std::mem::size_of::<$t>(),
                status,
            )
        }
    };
}

property!(insert_color, get_color, 0, u32);
property!(insert_matrix3x2, get_matrix3x2, 1, ref 24);
property!(insert_matrix4x4, get_matrix4x4, 2, ref 64);
property!(insert_quaternion, get_quaternion, 3, ref 16);
property!(insert_scalar, get_scalar, 4, f32);
property!(insert_vector2, get_vector2, 5, u64);
property!(insert_vector3, get_vector3, 6, ref 12);
property!(insert_vector4, get_vector4, 7, ref 16);

static PROPERTY_SET_VT: VTable<22> = VTable(table::<22>(
    &INSPECTABLE,
    &[
        insert_color as *const (),
        insert_matrix3x2 as *const (),
        insert_matrix4x4 as *const (),
        insert_quaternion as *const (),
        insert_scalar as *const (),
        insert_vector2 as *const (),
        insert_vector3 as *const (),
        insert_vector4 as *const (),
        get_color as *const (),
        get_matrix3x2 as *const (),
        get_matrix4x4 as *const (),
        get_quaternion as *const (),
        get_scalar as *const (),
        get_vector2 as *const (),
        get_vector3 as *const (),
        get_vector4 as *const (),
    ],
));

/// Toutes les tables de ce module (pour reconnaître nos objets).
static ALL_VTS: [fn() -> *const (); 25] = [
    || EMPTY_VT.ptr(),
    || CLOSABLE_VT.ptr(),
    || OBJECT_VT.ptr(),
    || CONTROLLER_VT.ptr(),
    || ASYNC_ACTION_VT.ptr(),
    || ASYNC_INFO_VT.ptr(),
    || COMPOSITOR_VT.ptr(),
    || DESKTOP_INTEROP_VT.ptr(),
    || COMPOSITOR_INTEROP_VT.ptr(),
    || VISUAL_VT.ptr(),
    || CONTAINER_VT.ptr(),
    || SPRITE_VT.ptr(),
    || COLLECTION_VT.ptr(),
    || COLOR_BRUSH_VT.ptr(),
    || SURFACE_BRUSH_VT.ptr(),
    || INSET_CLIP_VT.ptr(),
    || DESKTOP_TARGET_VT.ptr(),
    || TARGET_VT.ptr(),
    || DEVICE_VT.ptr(),
    || DEVICE2_VT.ptr(),
    || DEVICE_INTEROP_VT.ptr(),
    || DRAWING_SURFACE_VT.ptr(),
    || DRAWING_SURFACE2_VT.ptr(),
    || SURFACE_INTEROP_VT.ptr(),
    || PROPERTY_SET_VT.ptr(),
];

// ---------------------------------------------------------------------
// Rendu logiciel
// ---------------------------------------------------------------------

/// Transformation affine sans rotation : p ↦ o + s·p.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Affine {
    o: [f32; 2],
    s: [f32; 2],
}

impl Affine {
    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.o[0] + self.s[0] * x, self.o[1] + self.s[1] * y)
    }
}

/// Image composée : BGRA prémultiplié.
struct Frame {
    w: usize,
    h: usize,
    px: Vec<u8>,
    drew: bool,
}

impl Frame {
    /// Mélange « source par-dessus » d'un pixel prémultiplié.
    fn blend(&mut self, x: usize, y: usize, src: [f32; 4]) {
        let i = (y * self.w + x) * 4;
        let d = &mut self.px[i..i + 4];
        let inv = 1.0 - src[3];
        for (d, s) in d.iter_mut().zip(src) {
            *d = (s * 255.0 + *d as f32 * inv).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Rectangle [x0, x1) × [y0, y1) en pixels de la fenêtre.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Bounds {
    fn intersect(&self, o: &Bounds) -> Bounds {
        Bounds {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }
}

struct Snapshot {
    p: Visual,
    clip: Option<[f32; 4]>,
    brush: Option<BrushSnap>,
    children: Vec<*mut Obj>,
}

enum BrushSnap {
    Color([u8; 4]),
    Surface {
        w: u32,
        h: u32,
        px: Vec<u8>,
        h_ratio: f32,
        v_ratio: f32,
        stretch: i32,
    },
}

unsafe fn snapshot(v: *mut Obj) -> Option<Snapshot> {
    let (p, clip, brush, children) = {
        let st = lock(&(*v).st);
        let Data::Visual(d) = &st.data else {
            return None;
        };
        for c in &d.children {
            obj_add_ref(*c);
        }
        com_add_ref(d.clip);
        com_add_ref(d.brush);
        (d.p, d.clip, d.brush, d.children.clone())
    };
    let clip_snap = ours_of(clip, &[Kind::InsetClip]).and_then(|c| match lock(&(*c).st).data {
        Data::InsetClip(m) => Some(m),
        _ => None,
    });
    let brush_snap = ours(brush).and_then(|b| brush_snapshot(b));
    com_release(clip);
    com_release(brush);
    Some(Snapshot {
        p,
        clip: clip_snap,
        brush: brush_snap,
        children,
    })
}

unsafe fn brush_snapshot(b: *mut Obj) -> Option<BrushSnap> {
    let (surface, h_ratio, v_ratio, stretch) = match lock(&(*b).st).data {
        Data::ColorBrush { color } => return Some(BrushSnap::Color(color)),
        Data::SurfaceBrush {
            surface,
            h_ratio,
            v_ratio,
            stretch,
            ..
        } => {
            com_add_ref(surface);
            (surface, h_ratio, v_ratio, stretch)
        }
        _ => return None,
    };
    let snap = ours_of(surface, &[Kind::Surface]).and_then(|s| match &lock(&(*s).st).data {
        Data::Surface(d) => Some(BrushSnap::Surface {
            w: d.w,
            h: d.h,
            px: d.px.clone(),
            h_ratio,
            v_ratio,
            stretch,
        }),
        _ => None,
    });
    com_release(surface);
    snap
}

unsafe fn render_visual(
    frame: &mut Frame,
    v: *mut Obj,
    parent: Affine,
    clip: Bounds,
    opacity: f32,
) {
    let Some(snap) = snapshot(v) else { return };
    let p = snap.p;
    let children = snap.children;
    if p.visible != 0 && p.opacity > 0.0 {
        // Position locale : décalage moins l'ancre, échelle autour du centre,
        // puis la part 2D de TransformMatrix (échelle et translation).
        let pos = [
            p.offset[0] - p.anchor[0] * p.size[0],
            p.offset[1] - p.anchor[1] * p.size[1],
        ];
        let m = &p.matrix;
        let local = Affine {
            o: [
                m[0] * (pos[0] + p.center[0] * (1.0 - p.scale[0])) + m[12],
                m[5] * (pos[1] + p.center[1] * (1.0 - p.scale[1])) + m[13],
            ],
            s: [m[0] * p.scale[0], m[5] * p.scale[1]],
        };
        let world = Affine {
            o: [
                parent.o[0] + parent.s[0] * local.o[0],
                parent.o[1] + parent.s[1] * local.o[1],
            ],
            s: [parent.s[0] * local.s[0], parent.s[1] * local.s[1]],
        };
        let mut clip = clip;
        if let Some([l, t, r, b]) = snap.clip {
            let (x0, y0) = world.apply(l, t);
            let (x1, y1) = world.apply(p.size[0] - r, p.size[1] - b);
            clip = clip.intersect(&Bounds {
                x0: x0.min(x1),
                y0: y0.min(y1),
                x1: x0.max(x1),
                y1: y0.max(y1),
            });
        }
        let opacity = opacity * p.opacity.clamp(0.0, 1.0);
        if let Some(brush) = &snap.brush {
            paint(frame, brush, &world, p.size, &clip, opacity);
        }
        for c in &children {
            render_visual(frame, *c, world, clip, opacity);
        }
    }
    for c in children {
        obj_release(c);
    }
}

/// Peint le pinceau sur le rectangle (0, 0, size) du visuel.
fn paint(
    frame: &mut Frame,
    brush: &BrushSnap,
    world: &Affine,
    size: [f32; 2],
    clip: &Bounds,
    opacity: f32,
) {
    let (x0, y0) = world.apply(0.0, 0.0);
    let (x1, y1) = world.apply(size[0], size[1]);
    let area = clip.intersect(&Bounds {
        x0: x0.min(x1),
        y0: y0.min(y1),
        x1: x0.max(x1),
        y1: y0.max(y1),
    });
    let px0 = area.x0.round().max(0.0) as usize;
    let py0 = area.y0.round().max(0.0) as usize;
    let px1 = (area.x1.round().max(0.0) as usize).min(frame.w);
    let py1 = (area.y1.round().max(0.0) as usize).min(frame.h);
    if px0 >= px1 || py0 >= py1 {
        return;
    }
    match brush {
        BrushSnap::Color([a, r, g, b]) => {
            let a = *a as f32 / 255.0 * opacity;
            let src = [
                *b as f32 / 255.0 * a,
                *g as f32 / 255.0 * a,
                *r as f32 / 255.0 * a,
                a,
            ];
            if a <= 0.0 {
                return;
            }
            for y in py0..py1 {
                for x in px0..px1 {
                    frame.blend(x, y, src);
                }
            }
            frame.drew = true;
        }
        BrushSnap::Surface {
            w,
            h,
            px,
            h_ratio,
            v_ratio,
            stretch,
        } => {
            if *w == 0 || *h == 0 {
                return;
            }
            // Taille du contenu dans le visuel selon CompositionStretch :
            // None 0, Fill 1, Uniform 2, UniformToFill 3.
            let (sw, sh) = (*w as f32, *h as f32);
            let (cw, ch) = match stretch {
                0 => (sw, sh),
                2 | 3 => {
                    let k = if *stretch == 2 {
                        (size[0] / sw).min(size[1] / sh)
                    } else {
                        (size[0] / sw).max(size[1] / sh)
                    };
                    (sw * k, sh * k)
                }
                _ => (size[0], size[1]),
            };
            if cw <= 0.0 || ch <= 0.0 {
                return;
            }
            let cx = (size[0] - cw) * h_ratio;
            let cy = (size[1] - ch) * v_ratio;
            for y in py0..py1 {
                let ly = ((y as f32 + 0.5) - world.o[1]) / world.s[1];
                let sy = ((ly - cy) / ch * sh).floor();
                if sy < 0.0 || sy >= sh {
                    continue;
                }
                for x in px0..px1 {
                    let lx = ((x as f32 + 0.5) - world.o[0]) / world.s[0];
                    let sx = ((lx - cx) / cw * sw).floor();
                    if sx < 0.0 || sx >= sw {
                        continue;
                    }
                    let i = (sy as usize * *w as usize + sx as usize) * 4;
                    let s = &px[i..i + 4];
                    if s[3] == 0 {
                        continue;
                    }
                    let src = [
                        s[0] as f32 / 255.0 * opacity,
                        s[1] as f32 / 255.0 * opacity,
                        s[2] as f32 / 255.0 * opacity,
                        s[3] as f32 / 255.0 * opacity,
                    ];
                    frame.blend(x, y, src);
                }
            }
            frame.drew = true;
        }
    }
}

/// Compose l'arbre d'une cible sur un fond donné (BGRA, w × h).
unsafe fn compose(root: *mut Obj, w: usize, h: usize, background: Vec<u8>) -> Frame {
    let mut frame = Frame {
        w,
        h,
        px: background,
        drew: false,
    };
    let all = Bounds {
        x0: 0.0,
        y0: 0.0,
        x1: w as f32,
        y1: h as f32,
    };
    let id = Affine {
        o: [0.0; 2],
        s: [1.0; 2],
    };
    render_visual(&mut frame, root, id, all, 1.0);
    frame
}

unsafe fn render_target(t: *mut Obj) {
    let (hwnd, root) = match lock(&(*t).st).data {
        Data::Target { hwnd, root, .. } => {
            if root.is_null() {
                return;
            }
            obj_add_ref(root);
            (hwnd, root)
        }
        _ => return,
    };
    if super::gui::is_window(hwnd) {
        let mut rc = super::gui::Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        super::user32::get_client_rect(hwnd, &mut rc);
        let (w, h) = (
            (rc.right - rc.left).max(0) as usize,
            (rc.bottom - rc.top).max(0) as usize,
        );
        if w > 0 && h > 0 {
            // Le contenu de la fenêtre sert de fond aux visuels.
            let mut bg = vec![0u8; w * h * 4];
            if let Some(snap) = super::gui::capture_window(
                hwnd,
                super::gui::Rect {
                    left: 0,
                    top: 0,
                    right: w as i32,
                    bottom: h as i32,
                },
            ) {
                if snap.width == w && snap.height == h {
                    for (p, &c) in bg.as_chunks_mut::<4>().0.iter_mut().zip(&snap.pixels) {
                        *p = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
                    }
                }
            }
            let frame = compose(root, w, h, bg);
            if frame.drew {
                present(hwnd, &frame);
            }
        }
    }
    obj_release(root);
}

unsafe fn present(hwnd: usize, frame: &Frame) {
    let mut px = frame.px.clone();
    for p in px.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    // BITMAPINFOHEADER 32 bits, lignes de haut en bas.
    let mut info = [0u8; 40];
    info[0..4].copy_from_slice(&40u32.to_le_bytes());
    info[4..8].copy_from_slice(&(frame.w as i32).to_le_bytes());
    info[8..12].copy_from_slice(&(-(frame.h as i32)).to_le_bytes());
    info[12..14].copy_from_slice(&1u16.to_le_bytes());
    info[14..16].copy_from_slice(&32u16.to_le_bytes());
    let hdc = super::user32::get_dc(hwnd);
    if hdc == 0 {
        return;
    }
    super::gdi32_ext::set_di_bits_to_device(
        hdc,
        0,
        0,
        frame.w as u32,
        frame.h as u32,
        0,
        0,
        0,
        frame.h as u32,
        px.as_ptr() as *const c_void,
        info.as_ptr() as *const c_void,
        0,
    );
    super::user32::release_dc(hwnd, hdc);
}

#[cfg(test)]
mod tests {
    use super::*;

    type Qi = unsafe extern "win64" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;

    unsafe fn call_qi(p: *mut c_void, iid: &Guid) -> *mut c_void {
        let mut out = std::ptr::null_mut();
        let f: Qi = vcall(p, 0);
        assert_eq!(
            f(p, iid, &mut out),
            S_OK,
            "QI {}",
            super::super::comobj::guid_str(iid)
        );
        out
    }

    unsafe fn out0(p: *mut c_void, slot: usize) -> *mut c_void {
        let mut out = std::ptr::null_mut();
        let f: unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32 = vcall(p, slot);
        assert_eq!(f(p, &mut out), S_OK);
        out
    }

    #[test]
    fn identifiants_au_format_windows() {
        assert_eq!(
            iid("6329d6ca-3366-490e-9db3-25312929ac51"),
            super::super::comobj::guid(
                0x6329d6ca,
                0x3366,
                0x490e,
                [0x9d, 0xb3, 0x25, 0x31, 0x29, 0x29, 0xac, 0x51]
            )
        );
    }

    #[test]
    fn controleur_active_et_compositeur() {
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CONTROLLER_CLASS, IID_ACTIVATION_FACTORY.as_ptr(), &mut f),
                Some(S_OK)
            );
            assert!(activation_factory(
                "Windows.UI.Xaml.Window",
                IID_ACTIVATION_FACTORY.as_ptr(),
                &mut f
            )
            .is_none());
            let insp = out0(f as *mut c_void, 6);
            let ctl = call_qi(insp, &IID_CONTROLLER);
            com_release(insp);
            let comp = out0(ctl, 6);
            // Nom de classe du compositeur.
            let mut h = 0usize;
            let name: unsafe extern "win64" fn(*mut c_void, *mut usize) -> i32 = vcall(comp, 4);
            assert_eq!(name(comp, &mut h), S_OK);
            assert_eq!(hstring(h), COMPOSITOR_CLASS);
            // Interop bureau disponible.
            let interop = call_qi(comp, &IID_DESKTOP_INTEROP);
            com_release(interop);
            // Commit et opération déjà terminée.
            let commit_fn: unsafe extern "win64" fn(*mut c_void) -> i32 = vcall(ctl, 7);
            assert_eq!(commit_fn(ctl), S_OK);
            let op = out0(ctl, 8);
            let info = call_qi(op, &IID_ASYNC_INFO);
            let mut status = 0i32;
            let st: unsafe extern "win64" fn(*mut c_void, *mut i32) -> i32 = vcall(info, 7);
            assert_eq!(st(info, &mut status), S_OK);
            assert_eq!(status, 1);
            com_release(info);
            com_release(op);
            com_release(comp);
            com_release(ctl);
        }
    }

    #[repr(C)]
    struct Handler {
        vt: *const VTable<4>,
        refs: AtomicU32,
        calls: AtomicU32,
    }
    unsafe extern "win64" fn h_qi(_: *mut c_void, _: *const u8, _: *mut *mut c_void) -> i32 {
        E_NOINTERFACE
    }
    unsafe extern "win64" fn h_add(this: *mut Handler) -> u32 {
        (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
    }
    unsafe extern "win64" fn h_rel(this: *mut Handler) -> u32 {
        (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1
    }
    unsafe extern "win64" fn h_invoke(this: *mut Handler, _: *mut c_void, _: *mut c_void) -> i32 {
        (*this).calls.fetch_add(1, Ordering::AcqRel);
        S_OK
    }
    static H_VT: VTable<4> = VTable([
        h_qi as *const (),
        h_add as *const (),
        h_rel as *const (),
        h_invoke as *const (),
    ]);

    #[test]
    fn arbre_de_sprites_compose() {
        unsafe {
            let ctl = default_iface(new_controller());
            let mut handler = Handler {
                vt: &H_VT,
                refs: AtomicU32::new(1),
                calls: AtomicU32::new(0),
            };
            let mut token = 0i64;
            let add: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut i64) -> i32 =
                vcall(ctl, 9);
            assert_eq!(
                add(ctl, &mut handler as *mut Handler as *mut c_void, &mut token),
                S_OK
            );
            let comp = out0(ctl, 6);

            // Racine conteneur 100 × 80, sprite rouge 20 × 10 à (5, 6).
            let root = out0(comp, 9);
            let sprite = out0(comp, 22);
            let brush = {
                let mut b = std::ptr::null_mut();
                let f: unsafe extern "win64" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
                    vcall(comp, 8);
                // A, R, G, B
                assert_eq!(f(comp, u32::from_le_bytes([255, 255, 0, 0]), &mut b), S_OK);
                b
            };
            let set_brush: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 =
                vcall(sprite, 7);
            assert_eq!(set_brush(sprite, brush), S_OK);
            assert_eq!(handler.calls.load(Ordering::Acquire), 1, "CommitNeeded");
            let vis = call_qi(sprite, &IID_VISUAL);
            let set_size: unsafe extern "win64" fn(*mut c_void, u64) -> i32 = vcall(vis, 36);
            set_size(vis, (10f32.to_bits() as u64) << 32 | 20f32.to_bits() as u64);
            let set_offset: unsafe extern "win64" fn(*mut c_void, *const [f32; 3]) -> i32 =
                vcall(vis, 21);
            set_offset(vis, &[5.0, 6.0, 0.0]);
            let mut size = [0f32; 2];
            let get_size: unsafe extern "win64" fn(*mut c_void, *mut [f32; 2]) -> i32 =
                vcall(vis, 35);
            get_size(vis, &mut size);
            assert_eq!(size, [20.0, 10.0]);
            // Un seul CommitNeeded tant que rien n'est validé.
            assert_eq!(handler.calls.load(Ordering::Acquire), 1);

            let children = out0(root, 6);
            let insert_top: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 =
                vcall(children, 9);
            assert_eq!(insert_top(children, vis), S_OK);
            assert_eq!(insert_top(children, vis), E_INVALIDARG, "déjà enfant");
            let mut n = 0i32;
            let count: unsafe extern "win64" fn(*mut c_void, *mut i32) -> i32 = vcall(children, 6);
            count(children, &mut n);
            assert_eq!(n, 1);
            let parent = out0(vis, 26);
            assert!(!parent.is_null());
            com_release(parent);

            // Découpe par marges : 2 px à gauche.
            let clip = {
                let mut c = std::ptr::null_mut();
                let f: unsafe extern "win64" fn(
                    *mut c_void,
                    f32,
                    f32,
                    f32,
                    f32,
                    *mut *mut c_void,
                ) -> i32 = vcall(comp, 16);
                assert_eq!(f(comp, 2.0, 0.0, 0.0, 0.0, &mut c), S_OK);
                c
            };
            let set_clip: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 =
                vcall(vis, 15);
            assert_eq!(set_clip(vis, clip), S_OK);

            let frame = compose(ours(root).unwrap(), 40, 30, vec![0; 40 * 30 * 4]);
            assert!(frame.drew);
            let at = |x: usize, y: usize| {
                let i = (y * 40 + x) * 4;
                [
                    frame.px[i],
                    frame.px[i + 1],
                    frame.px[i + 2],
                    frame.px[i + 3],
                ]
            };
            assert_eq!(at(10, 10), [0, 0, 255, 255], "rouge en BGRA");
            assert_eq!(at(6, 10), [0, 0, 0, 0], "découpé");
            assert_eq!(at(7, 10), [0, 0, 255, 255]);
            assert_eq!(at(25, 10), [0, 0, 0, 0], "hors du sprite");
            assert_eq!(at(10, 16), [0, 0, 0, 0]);

            // Opacité 0,5 sur le sprite.
            let set_opacity: unsafe extern "win64" fn(*mut c_void, f32) -> i32 = vcall(vis, 23);
            set_opacity(vis, 0.5);
            let frame = compose(ours(root).unwrap(), 40, 30, vec![0; 40 * 30 * 4]);
            let i = (10 * 40 + 10) * 4;
            assert_eq!(&frame.px[i..i + 4], &[0, 0, 128, 128]);

            let remove: unsafe extern "win64" fn(*mut c_void, i64) -> i32 = vcall(ctl, 10);
            assert_eq!(remove(ctl, token), S_OK);
            assert_eq!(handler.refs.load(Ordering::Acquire), 1);

            for p in [clip, children, vis, brush, sprite, root, comp, ctl] {
                com_release(p);
            }
        }
    }

    #[test]
    fn surface_de_dessin_direct2d() {
        unsafe {
            let comp = default_iface(new_compositor());
            // Appareil Direct2D d'une fabrique 1.1, sur une texture fictive :
            // CreateDevice garde simplement l'appareil DXGI.
            let mut factory = std::ptr::null_mut();
            assert_eq!(
                super::super::d2d1::d2d1_create_factory(
                    0,
                    iid("bb12d362-daee-4b9a-aa1d-14ba401cfa1f").as_ptr() as *const _,
                    std::ptr::null(),
                    &mut factory
                ),
                S_OK
            );
            // Le compositeur sert d'« appareil DXGI » : seul son AddRef compte.
            let mut d2d_device = std::ptr::null_mut();
            let create: unsafe extern "win64" fn(
                *mut c_void,
                *mut c_void,
                *mut *mut c_void,
            ) -> i32 = vcall(factory, 17);
            assert_eq!(create(factory, comp, &mut d2d_device), S_OK);

            let interop = call_qi(comp, &IID_COMPOSITOR_INTEROP);
            let mut device = std::ptr::null_mut();
            let cgd: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32 =
                vcall(interop, 5);
            assert_eq!(cgd(interop, d2d_device, &mut device), S_OK);

            let mut surface = std::ptr::null_mut();
            let cds: unsafe extern "win64" fn(*mut c_void, u64, i32, i32, *mut *mut c_void) -> i32 =
                vcall(device, 6);
            let size = (4f32.to_bits() as u64) << 32 | 8f32.to_bits() as u64;
            assert_eq!(cds(device, size, 87, 1, &mut surface), S_OK);
            let mut sz = [0f32; 2];
            let get_size: unsafe extern "win64" fn(*mut c_void, *mut [f32; 2]) -> i32 =
                vcall(surface, 8);
            get_size(surface, &mut sz);
            assert_eq!(sz, [8.0, 4.0]);

            // BeginDraw sur (2, 1)-(6, 3), Clear vert opaque, EndDraw.
            let si = call_qi(surface, &IID_SURFACE_INTEROP);
            let mut dc = std::ptr::null_mut();
            let mut offset = [9, 9];
            let begin: unsafe extern "win64" fn(
                *mut c_void,
                *const [i32; 4],
                *const u8,
                *mut *mut c_void,
                *mut [i32; 2],
            ) -> i32 = vcall(si, 3);
            let dc_iid = iid("e8f7fe7a-191c-466d-ad95-975678bda998");
            assert_eq!(
                begin(si, &[2, 1, 6, 3], dc_iid.as_ptr(), &mut dc, &mut offset),
                S_OK
            );
            assert_eq!(offset, [0, 0]);
            assert_eq!(
                begin(
                    si,
                    std::ptr::null(),
                    dc_iid.as_ptr(),
                    &mut std::ptr::null_mut(),
                    &mut offset
                ),
                E_FAIL,
                "déjà en cours"
            );
            // ID2D1RenderTarget::Clear (47).
            let clear: unsafe extern "win64" fn(*mut c_void, *const [f32; 4]) = vcall(dc, 47);
            clear(dc, &[0.0, 1.0, 0.0, 1.0]);
            com_release(dc);
            let end: unsafe extern "win64" fn(*mut c_void) -> i32 = vcall(si, 4);
            assert_eq!(end(si), S_OK);

            let s = ours(surface).unwrap();
            let px = match &lock(&(*s).st).data {
                Data::Surface(d) => d.px.clone(),
                _ => unreachable!(),
            };
            let at = |x: usize, y: usize| px[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4].to_vec();
            assert_eq!(at(2, 1), [0, 255, 0, 255]);
            assert_eq!(at(5, 2), [0, 255, 0, 255]);
            assert_eq!(at(1, 1), [0, 0, 0, 0]);
            assert_eq!(at(6, 2), [0, 0, 0, 0]);

            // Pinceau de surface étiré sur un sprite 16 × 8.
            let mut brush = std::ptr::null_mut();
            let sb: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32 =
                vcall(comp, 24);
            assert_eq!(sb(comp, surface, &mut brush), S_OK);
            let sprite = out0(comp, 22);
            let set_brush: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32 =
                vcall(sprite, 7);
            set_brush(sprite, brush);
            let vis = call_qi(sprite, &IID_VISUAL);
            let set_size: unsafe extern "win64" fn(*mut c_void, u64) -> i32 = vcall(vis, 36);
            set_size(vis, (8f32.to_bits() as u64) << 32 | 16f32.to_bits() as u64);
            let frame = compose(ours(sprite).unwrap(), 16, 8, vec![0; 16 * 8 * 4]);
            let i = (3 * 16 + 5) * 4;
            assert_eq!(&frame.px[i..i + 4], &[0, 255, 0, 255]);
            assert_eq!(&frame.px[0..4], &[0, 0, 0, 0]);

            for p in [
                vis, sprite, brush, si, surface, device, interop, d2d_device, factory, comp,
            ] {
                com_release(p);
            }
        }
    }

    #[test]
    fn jeu_de_proprietes() {
        unsafe {
            let comp = default_iface(new_compositor());
            let set = out0(comp, 18);
            let w: Vec<u16> = "Zoom".encode_utf16().collect();
            let mut name = 0usize;
            super::super::electron_win32::WindowsCreateString(
                w.as_ptr(),
                w.len() as u32,
                &mut name,
            );
            let ins: unsafe extern "win64" fn(*mut c_void, usize, f32) -> i32 = vcall(set, 10);
            assert_eq!(ins(set, name, 2.5), S_OK);
            let get: unsafe extern "win64" fn(*mut c_void, usize, *mut f32, *mut i32) -> i32 =
                vcall(set, 18);
            let (mut v, mut status) = (0f32, -1);
            assert_eq!(get(set, name, &mut v, &mut status), S_OK);
            assert_eq!((v, status), (2.5, 0));
            let get_color: unsafe extern "win64" fn(*mut c_void, usize, *mut u32, *mut i32) -> i32 =
                vcall(set, 14);
            let mut c = 0u32;
            get_color(set, name, &mut c, &mut status);
            assert_eq!(status, 1, "type différent");
            get(set, 0, &mut v, &mut status);
            assert_eq!(status, 2, "absent");
            com_release(set);
            com_release(comp);
        }
    }
}
