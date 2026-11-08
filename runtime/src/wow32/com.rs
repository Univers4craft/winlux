//! COM pour les programmes x86 (#232, étape 6).
//!
//! Un objet COM est un pointeur vers une table de fonctions. Les objets du
//! runtime ont des tables de fonctions win64 (cases de 8 octets), que le
//! code x86 ne peut ni lire ni appeler ; les objets d'un programme x86 ont
//! des tables de fonctions x86 (cases de 4 octets, stdcall). Comme le
//! WOW64 de Windows, chaque objet qui passe la frontière reçoit un
//! mandataire :
//!
//! - **`P32`** : objet du runtime vu par le code x86. Sa table contient une
//!   passerelle x86 par méthode, qui relit les arguments sur la pile 32 bits
//!   d'après la description de l'interface (`Iface`) et appelle la méthode
//!   64 bits.
//! - **`P64`** : objet x86 vu par le runtime (fabrique de classes déposée
//!   par CoRegisterClassObject, IStream passé à CopyTo, IDropTarget…). Sa
//!   table contient une passerelle win64 par méthode, qui appelle la
//!   méthode x86 par `call32`.
//!
//! Un mandataire qui revient de l'autre côté est déballé : l'objet d'origine
//! est rendu, l'identité COM est donc gardée dans les deux sens. Les
//! pointeurs d'interface rendus (`void**`) passent par une case de 8 octets
//! puis sont convertis ; une interface que le runtime ne décrit pas est
//! refusée (E_NOINTERFACE) plutôt que de donner au programme une table
//! qu'il ne saurait pas appeler.

use super::layout::widen;
use super::{call32, rd, wr};
use crate::dlls::comobj::{guid, guid_str, Guid};
use std::collections::HashMap;
use std::sync::Mutex;

const E_NOINTERFACE: u32 = 0x8000_4002;
const E_NOTIMPL: u32 = 0x8000_4001;

/// Nature d'un argument d'une méthode ou d'une fonction, `this` exclu.
#[derive(Clone, Copy)]
pub(super) enum A {
    /// 4 octets : entier, pointeur vers des données sans pointeur, handle.
    I,
    /// 8 octets passés par valeur (LARGE_INTEGER, POINTL).
    Q,
    /// REFIID : type des `OutQ`/`InQ` qui suivent.
    Iid,
    /// `void**` : interface rendue, du type donné par le dernier `Iid`.
    OutQ,
    /// Interface passée, du type donné par le dernier `Iid`.
    InQ,
    /// Interface rendue de type connu.
    Out(&'static Iface),
    /// Interface passée de type connu.
    In(&'static Iface),
    /// Pointeur vers une valeur de la taille d'un pointeur rendue
    /// (LPOLESTR*, HGLOBAL*, BSTR*, ULONG_PTR*).
    OutPtr,
    /// STATSTG* rendu.
    Stat,
    /// VARIANT* lu et écrit (16 octets en x86, 24 en x64).
    Var,
}

/// Description d'une interface : ses méthodes propres (après celles de
/// `parent`). `None` : méthode non décrite.
pub(super) struct Iface {
    pub name: &'static str,
    pub iid: Guid,
    pub parent: Option<&'static Iface>,
    pub methods: &'static [Option<&'static [A]>],
}

impl Iface {
    fn count(&'static self) -> usize {
        self.parent.map_or(0, |p| p.count()) + self.methods.len()
    }

    fn method(&'static self, idx: usize) -> Option<&'static [A]> {
        let base = self.parent.map_or(0, |p| p.count());
        if idx < base {
            self.parent.and_then(|p| p.method(idx))
        } else {
            self.methods.get(idx - base).copied().flatten()
        }
    }

    /// `self` est-elle `other` ou une interface qui en dérive ?
    fn derives(&'static self, other: &'static Iface) -> bool {
        std::ptr::eq(self, other) || self.parent.is_some_and(|p| p.derives(other))
    }
}

const fn ole(d1: u32) -> Guid {
    guid(d1, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46])
}

use A::*;

pub(super) static IUNKNOWN: Iface = Iface {
    name: "IUnknown",
    iid: ole(0),
    parent: None,
    methods: &[Some(&[Iid, OutQ]), Some(&[]), Some(&[])],
};

pub(super) static ICLASSFACTORY: Iface = Iface {
    name: "IClassFactory",
    iid: ole(1),
    parent: Some(&IUNKNOWN),
    methods: &[Some(&[In(&IUNKNOWN), Iid, OutQ]), Some(&[I])],
};

pub(super) static IMALLOC: Iface = Iface {
    name: "IMalloc",
    iid: ole(2),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I]),
        Some(&[I, I]),
        Some(&[I]),
        Some(&[I]),
        Some(&[I]),
        Some(&[]),
    ],
};

pub(super) static ISEQUENTIALSTREAM: Iface = Iface {
    name: "ISequentialStream",
    iid: guid(
        0x0C73_3A30,
        0x2A1C,
        0x11CE,
        [0xAD, 0xE5, 0x00, 0xAA, 0x00, 0x44, 0x77, 0x3D],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[Some(&[I, I, I]), Some(&[I, I, I])],
};

pub(super) static ISTREAM: Iface = Iface {
    name: "IStream",
    iid: ole(0xC),
    parent: Some(&ISEQUENTIALSTREAM),
    methods: &[
        Some(&[Q, I, I]),               // Seek
        Some(&[Q]),                     // SetSize
        Some(&[In(&ISTREAM), Q, I, I]), // CopyTo
        Some(&[I]),                     // Commit
        Some(&[]),                      // Revert
        Some(&[Q, Q, I]),               // LockRegion
        Some(&[Q, Q, I]),               // UnlockRegion
        Some(&[Stat, I]),               // Stat
        Some(&[Out(&ISTREAM)]),         // Clone
    ],
};

pub(super) static IPERSIST: Iface = Iface {
    name: "IPersist",
    iid: ole(0x10C),
    parent: Some(&IUNKNOWN),
    methods: &[Some(&[I])],
};

pub(super) static IPERSISTSTREAM: Iface = Iface {
    name: "IPersistStream",
    iid: ole(0x109),
    parent: Some(&IPERSIST),
    methods: &[
        Some(&[]),
        Some(&[In(&ISTREAM)]),
        Some(&[In(&ISTREAM), I]),
        Some(&[I]),
    ],
};

/// IMoniker : liaison à un IStream, nom affiché et type (moniker d'URL).
pub(super) static IMONIKER: Iface = Iface {
    name: "IMoniker",
    iid: ole(0xF),
    parent: Some(&IPERSISTSTREAM),
    methods: &[
        Some(&[I, I, Iid, OutQ]), // BindToObject
        Some(&[I, I, Iid, OutQ]), // BindToStorage
        None,                     // Reduce
        None,                     // ComposeWith
        None,                     // Enum
        Some(&[In(&IMONIKER)]),   // IsEqual
        Some(&[I]),               // Hash
        None,                     // IsRunning
        None,                     // GetTimeOfLastChange
        None,                     // Inverse
        None,                     // CommonPrefixWith
        None,                     // RelativePathTo
        Some(&[I, I, OutPtr]),    // GetDisplayName
        None,                     // ParseDisplayName
        Some(&[I]),               // IsSystemMoniker
    ],
};

/// IDataObject : méthodes à FORMATETC/STGMEDIUM, pas encore converties.
pub(super) static IDATAOBJECT: Iface = Iface {
    name: "IDataObject",
    iid: ole(0x10E),
    parent: Some(&IUNKNOWN),
    methods: &[None, None, None, None, None, None, None, None, None],
};

pub(super) static IDROPSOURCE: Iface = Iface {
    name: "IDropSource",
    iid: ole(0x121),
    parent: Some(&IUNKNOWN),
    methods: &[Some(&[I, I]), Some(&[I])],
};

pub(super) static IDROPTARGET: Iface = Iface {
    name: "IDropTarget",
    iid: ole(0x122),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[In(&IDATAOBJECT), I, Q, I]),
        Some(&[I, Q, I]),
        Some(&[]),
        Some(&[In(&IDATAOBJECT), I, Q, I]),
    ],
};

pub(super) static IERRORINFO: Iface = Iface {
    name: "IErrorInfo",
    iid: guid(
        0x1CF2_B120,
        0x547D,
        0x101B,
        [0x8E, 0x65, 0x08, 0x00, 0x2B, 0x2B, 0xD1, 0x19],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I]),
        Some(&[OutPtr]),
        Some(&[OutPtr]),
        Some(&[OutPtr]),
        Some(&[I]),
    ],
};

pub(super) static ICREATEERRORINFO: Iface = Iface {
    name: "ICreateErrorInfo",
    iid: guid(
        0x22F0_3340,
        0x547D,
        0x101B,
        [0x8E, 0x65, 0x08, 0x00, 0x2B, 0x2B, 0xD1, 0x19],
    ),
    parent: Some(&IUNKNOWN),
    methods: &[Some(&[I]), Some(&[I]), Some(&[I]), Some(&[I]), Some(&[I])],
};

/// IDispatch et ITypeLib : seules les méthodes d'IUnknown sont décrites
/// (VARIANT et DISPPARAMS par valeur viendront avec l'automation).
pub(super) static IDISPATCH: Iface = Iface {
    name: "IDispatch",
    iid: ole(0x2_0400),
    parent: Some(&IUNKNOWN),
    methods: &[None, None, None, None],
};

pub(super) static ITYPELIB: Iface = Iface {
    name: "ITypeLib",
    iid: ole(0x2_0402),
    parent: Some(&IUNKNOWN),
    methods: &[None, None, None, None, None, None, None, None, None, None],
};

/// IObjectArray : tableau d'objets (éléments retirés d'une liste de
/// raccourcis, tâches de la barre des tâches).
pub(super) static IOBJECTARRAY: Iface = Iface {
    name: "IObjectArray",
    iid: crate::dlls::jumplist_com::IID_IOBJECT_ARRAY,
    parent: Some(&IUNKNOWN),
    methods: &[Some(&[I]), Some(&[I, Iid, OutQ])],
};

/// ICustomDestinationList : listes de raccourcis de la barre des tâches.
pub(super) static ICUSTOMDESTINATIONLIST: Iface = Iface {
    name: "ICustomDestinationList",
    iid: crate::dlls::jumplist_com::IID_ICUSTOM_DESTINATION_LIST,
    parent: Some(&IUNKNOWN),
    methods: &[
        Some(&[I]),                    // SetAppID
        Some(&[I, Iid, OutQ]),         // BeginList
        Some(&[I, In(&IOBJECTARRAY)]), // AppendCategory
        Some(&[I]),                    // AppendKnownCategory
        Some(&[In(&IOBJECTARRAY)]),    // AddUserTasks
        Some(&[]),                     // CommitList
        Some(&[Iid, OutQ]),            // GetRemovedDestinations
        Some(&[I]),                    // DeleteList
        Some(&[]),                     // AbortList
    ],
};

static ALL: &[&Iface] = &[
    &IUNKNOWN,
    &ICLASSFACTORY,
    &IMALLOC,
    &ISEQUENTIALSTREAM,
    &ISTREAM,
    &IPERSIST,
    &IPERSISTSTREAM,
    &IDATAOBJECT,
    &IDROPSOURCE,
    &IDROPTARGET,
    &IERRORINFO,
    &ICREATEERRORINFO,
    &IDISPATCH,
    &ITYPELIB,
    &IOBJECTARRAY,
    &ICUSTOMDESTINATIONLIST,
];

fn by_iid(iid: &Guid) -> Option<&'static Iface> {
    ALL.iter().copied().find(|f| &f.iid == iid)
}

/// Interface d'un objet x86 que le runtime ne décrit pas : seules les
/// méthodes d'IUnknown sont transmises, les autres rendent E_NOTIMPL.
static OPAQUE: Iface = Iface {
    name: "interface non décrite",
    iid: [0; 16],
    parent: Some(&IUNKNOWN),
    methods: &[],
};

// ---------------------------------------------------------------------
// Mandataires
// ---------------------------------------------------------------------

/// Objet du runtime vu par le code x86.
#[repr(C)]
struct P32 {
    vtbl: u32,
    _pad: u32,
    obj: u64,
    iface: &'static Iface,
}

/// Objet x86 vu par le runtime.
#[repr(C)]
struct P64 {
    vtbl: usize,
    obj: u32,
    iface: &'static Iface,
}

/// Méthode `idx` de `iface`, référencée par les passerelles.
struct MRef {
    idx: usize,
    kinds: Option<&'static [A]>,
}

#[derive(Default)]
struct Maps {
    /// (objet 64 bits, interface) → mandataire x86.
    p32: HashMap<(u64, usize), u32>,
    /// Mandataire x86 → objet 64 bits.
    p32_obj: HashMap<u32, u64>,
    /// (objet x86, interface) → mandataire 64 bits.
    p64: HashMap<(u32, usize), usize>,
    /// Mandataire 64 bits → objet x86 et son interface.
    p64_obj: HashMap<usize, (u32, &'static Iface)>,
    /// Tables x86 et 64 bits déjà construites, par interface.
    vt32: HashMap<usize, u32>,
    vt64: HashMap<usize, usize>,
}

static MAPS: Mutex<Option<Maps>> = Mutex::new(None);

fn maps<R>(f: impl FnOnce(&mut Maps) -> R) -> R {
    let mut g = MAPS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Maps::default))
}

fn key(f: &'static Iface) -> usize {
    f as *const Iface as usize
}

/// Méthodes ajoutées en fin de table pour une interface en partie décrite.
const SPARE: usize = 32;

/// Table x86 de `iface` : une passerelle par méthode.
fn vtable32(iface: &'static Iface) -> u32 {
    if let Some(v) = maps(|m| m.vt32.get(&key(iface)).copied()) {
        return v;
    }
    let total = iface.count() + SPARE;
    let mut table: Vec<u32> = Vec::with_capacity(total);
    for idx in 0..total {
        let kinds = if idx < iface.count() {
            iface.method(idx)
        } else {
            None
        };
        let label = format!("{}::méthode n° {idx} (COM)", iface.name);
        let stub = match kinds {
            Some(k) => {
                let m = Box::leak(Box::new(MRef { idx, kinds }));
                super::make_stub(
                    super::Desc {
                        ret: b'i',
                        supported: true,
                        target: method32 as *const () as usize,
                        kinds: Vec::new(),
                        label,
                        fix: super::crt::Fix::None,
                        raw: Some(m as *const MRef as usize),
                        callbacks: 0,
                    },
                    Some(((1 + slots(k)) * 4) as u16),
                )
            }
            // Le nombre d'arguments est inconnu : la passerelle arrête le
            // programme avec un message clair (comme une fonction absente).
            None => super::make_stub(
                super::Desc {
                    ret: b'i',
                    supported: false,
                    target: 0,
                    kinds: Vec::new(),
                    label,
                    fix: super::crt::Fix::None,
                    raw: None,
                    callbacks: 0,
                },
                None,
            ),
        };
        table.push(stub as u32);
    }
    let at = Box::leak(table.into_boxed_slice()).as_ptr() as usize as u32;
    maps(|m| *m.vt32.entry(key(iface)).or_insert(at))
}

fn slots(kinds: &[A]) -> usize {
    kinds
        .iter()
        .map(|k| if matches!(k, Q) { 2 } else { 1 })
        .sum()
}

// Entrée commune des passerelles win64 des mandataires `P64` :
// r10d = méthode (`MRef`). Recopie rcx, rdx, r8, r9 et huit cases de pile.
std::arch::global_asm!(
    r#"
    .text
    .globl winlux_wow_com64
winlux_wow_com64:
    push rbp
    mov rbp, rsp
    sub rsp, 128
    mov qword ptr [rsp + 32], rcx
    mov qword ptr [rsp + 40], rdx
    mov qword ptr [rsp + 48], r8
    mov qword ptr [rsp + 56], r9
    mov rax, qword ptr [rbp + 48]
    mov qword ptr [rsp + 64], rax
    mov rax, qword ptr [rbp + 56]
    mov qword ptr [rsp + 72], rax
    mov rax, qword ptr [rbp + 64]
    mov qword ptr [rsp + 80], rax
    mov rax, qword ptr [rbp + 72]
    mov qword ptr [rsp + 88], rax
    mov rax, qword ptr [rbp + 80]
    mov qword ptr [rsp + 96], rax
    mov rax, qword ptr [rbp + 88]
    mov qword ptr [rsp + 104], rax
    mov rax, qword ptr [rbp + 96]
    mov qword ptr [rsp + 112], rax
    mov rax, qword ptr [rbp + 104]
    mov qword ptr [rsp + 120], rax
    mov ecx, r10d
    lea rdx, [rsp + 32]
    call {dispatch}
    leave
    ret
"#,
    dispatch = sym method64,
);

extern "C" {
    fn winlux_wow_com64();
}

/// Table win64 de `iface` : une passerelle par méthode.
fn vtable64(iface: &'static Iface) -> usize {
    if let Some(v) = maps(|m| m.vt64.get(&key(iface)).copied()) {
        return v;
    }
    let total = iface.count() + SPARE;
    let mut table: Vec<usize> = Vec::with_capacity(total);
    for idx in 0..total {
        let kinds = if idx < iface.count() {
            iface.method(idx)
        } else {
            None
        };
        let m = Box::leak(Box::new(MRef { idx, kinds })) as *const MRef as usize;
        let mut code = Vec::with_capacity(20);
        code.extend_from_slice(&[0x41, 0xba]); // mov r10d, imm32
        code.extend_from_slice(&(m as u32).to_le_bytes());
        code.extend_from_slice(&[0x48, 0xb8]); // mov rax, imm64
        code.extend_from_slice(&(winlux_wow_com64 as *const () as usize as u64).to_le_bytes());
        code.extend_from_slice(&[0xff, 0xe0]); // jmp rax
        table.push(super::alloc_code(&code) as usize);
    }
    let at = Box::leak(table.into_boxed_slice()).as_ptr() as usize;
    maps(|m| *m.vt64.entry(key(iface)).or_insert(at))
}

/// Libère une référence d'un objet 64 bits.
unsafe fn release64(obj: u64) {
    let vt = *(obj as usize as *const *const usize);
    let f: unsafe extern "win64" fn(u64) -> u32 = std::mem::transmute(*vt.add(2));
    f(obj);
}

/// Appelle QueryInterface d'un objet x86 ; rend l'interface ou 0.
fn query32(obj: u32, iid: &Guid) -> u32 {
    let g = Box::new(*iid);
    let out = Box::new(0u32);
    let qi = rd(rd(obj));
    let hr = call32(
        qi as usize,
        &[
            obj,
            &*g as *const Guid as usize as u32,
            &*out as *const u32 as usize as u32,
        ],
    ) as u32;
    if hr as i32 >= 0 {
        *out
    } else {
        0
    }
}

/// Interface 64 bits `v` (référence comprise) rendue au code x86 sous le
/// type `iface`. `None` : interface non décrite, la référence est rendue et
/// le résultat vaut 0.
pub(super) fn to32(v: u64, iface: Option<&'static Iface>, iid: &Guid) -> u32 {
    if v == 0 {
        return 0;
    }
    // Objet « deux ABI » du runtime (DirectShow, DirectSound, Direct3D 9…) :
    // sa table est déjà celle du programme x86 (#778).
    if unsafe { crate::dlls::legacy_com::is_x86_object(v as usize) } {
        return v as u32;
    }
    // Mandataire d'un objet x86 : l'objet lui-même.
    if let Some((x, fx)) = maps(|m| m.p64_obj.get(&(v as usize)).copied()) {
        let fits = match iface {
            Some(f) => fx.derives(f),
            None => &fx.iid == iid,
        };
        if fits || std::ptr::eq(fx, &OPAQUE) {
            return x;
        }
        let q = query32(x, iid);
        // La référence reçue est celle du mandataire : rendue à l'objet x86.
        call32(rd(rd(x) + 8) as usize, &[x]);
        return q;
    }
    let Some(f) = iface else {
        crate::log_warn!(
            "wow32",
            "interface COM {} non décrite pour les programmes 32 bits : refusée",
            guid_str(iid)
        );
        unsafe { release64(v) };
        return 0;
    };
    if let Some(p) = maps(|m| m.p32.get(&(v, key(f))).copied()) {
        return p;
    }
    let vtbl = vtable32(f);
    let p = Box::leak(Box::new(P32 {
        vtbl,
        _pad: 0,
        obj: v,
        iface: f,
    })) as *mut P32 as usize as u32;
    maps(|m| {
        let p = *m.p32.entry((v, key(f))).or_insert(p);
        m.p32_obj.insert(p, v);
        p
    })
}

/// Interface x86 `v` passée au runtime sous le type `iface` (OPAQUE si
/// elle n'est pas décrite).
pub(super) fn to64(v: u32, iface: Option<&'static Iface>) -> u64 {
    if v == 0 {
        return 0;
    }
    if let Some(o) = maps(|m| m.p32_obj.get(&v).copied()) {
        return o;
    }
    let f = iface.unwrap_or(&OPAQUE);
    if let Some(p) = maps(|m| m.p64.get(&(v, key(f))).copied()) {
        return p as u64;
    }
    let vtbl = vtable64(f);
    let p = Box::leak(Box::new(P64 {
        vtbl,
        obj: v,
        iface: f,
    })) as *mut P64 as usize;
    maps(|m| {
        let p = *m.p64.entry((v, key(f))).or_insert(p);
        m.p64_obj.insert(p, (v, f));
        p as u64
    })
}

// ---------------------------------------------------------------------
// VARIANT et STATSTG
// ---------------------------------------------------------------------

const VT_BSTR: u16 = 8;
const VT_DISPATCH: u16 = 9;
const VT_UNKNOWN: u16 = 13;
const VT_PTR: u16 = 26;
const VT_LPSTR: u16 = 30;
const VT_LPWSTR: u16 = 31;
const VT_RECORD: u16 = 36;
const VT_ARRAY: u16 = 0x2000;
const VT_BYREF: u16 = 0x4000;

fn variant_has_pointer(vt: u16) -> bool {
    vt & (VT_ARRAY | VT_BYREF) != 0
        || matches!(
            vt,
            VT_BSTR | VT_DISPATCH | VT_UNKNOWN | VT_PTR | VT_LPSTR | VT_LPWSTR | VT_RECORD
        )
}

/// VARIANT x86 (16 octets) → x64 (24 octets).
unsafe fn variant64(src: u32, dst: *mut u8) {
    std::ptr::copy_nonoverlapping(src as usize as *const u8, dst, 16);
    std::ptr::write_bytes(dst.add(16), 0, 8);
    let vt = *(dst as *const u16);
    if !variant_has_pointer(vt) {
        return;
    }
    let p = rd(src + 8);
    let v = match vt {
        VT_UNKNOWN => to64(p, Some(&IUNKNOWN)),
        VT_DISPATCH => to64(p, Some(&IDISPATCH)),
        _ => p as u64,
    };
    (dst.add(8) as *mut u64).write_unaligned(v);
    if vt == VT_RECORD {
        (dst.add(16) as *mut u64).write_unaligned(rd(src + 12) as u64);
    }
}

/// VARIANT x64 → x86.
unsafe fn variant32(src: *const u8, dst: u32) {
    let vt = *(src as *const u16);
    std::ptr::copy_nonoverlapping(src, dst as usize as *mut u8, 8);
    if !variant_has_pointer(vt) {
        std::ptr::copy_nonoverlapping(src.add(8), (dst + 8) as usize as *mut u8, 8);
        return;
    }
    // Pointeurs sous 4 Go : 32 bits bas seulement (la version 64 bits peut
    // n'écrire que 4 octets, voir wow32::write_ptr).
    let p = (src.add(8) as *const u32).read_unaligned() as u64;
    let v = match vt {
        VT_UNKNOWN | VT_DISPATCH => {
            let f = if vt == VT_UNKNOWN {
                &IUNKNOWN
            } else {
                &IDISPATCH
            };
            to32(p, Some(f), &f.iid)
        }
        _ => p as u32,
    };
    wr(dst + 8, v);
    wr(
        dst + 12,
        if vt == VT_RECORD {
            (src.add(16) as *const u64).read_unaligned() as u32
        } else {
            0
        },
    );
}

/// STATSTG x64 (80 octets) → x86 (72 octets) : le nom est un pointeur,
/// le reste est décalé de 8.
unsafe fn statstg32(src: *const u8, dst: u32) {
    wr(dst, (src as *const u64).read_unaligned() as u32);
    wr(dst + 4, (src.add(8) as *const u32).read_unaligned());
    std::ptr::copy_nonoverlapping(src.add(16), (dst + 8) as usize as *mut u8, 64);
}

unsafe fn statstg64(src: u32, dst: *mut u8) {
    (dst as *mut u64).write_unaligned(rd(src) as u64);
    (dst.add(8) as *mut u32).write_unaligned(rd(src + 4));
    std::ptr::copy_nonoverlapping((src + 8) as usize as *const u8, dst.add(16), 64);
}

// ---------------------------------------------------------------------
// Appels 32 → 64
// ---------------------------------------------------------------------

/// Appelle la fonction win64 `target` avec les arguments x86 lus à `args`
/// (et `this` 64 bits en tête s'il y en a un).
unsafe fn from32(target: usize, this: Option<u64>, args: u32, kinds: &[A]) -> u64 {
    // Cases temporaires : capacité fixée, leurs adresses ne bougent pas.
    let mut temps: Vec<[u64; 10]> = Vec::with_capacity(kinds.len());
    let mut post: Vec<(usize, u32, A, Option<&'static Iface>, Guid)> = Vec::new();
    let mut slots: Vec<u64> = Vec::with_capacity(kinds.len() + 5);
    slots.extend(this);
    let mut p = args;
    let mut iid: Guid = [0; 16];
    for &k in kinds {
        let v = rd(p);
        p += 4;
        match k {
            I => slots.push(widen(v)),
            Q => {
                slots.push(v as u64 | (rd(p) as u64) << 32);
                p += 4;
            }
            Iid => {
                if v != 0 {
                    iid = std::ptr::read_unaligned(v as usize as *const Guid);
                }
                slots.push(v as u64);
            }
            InQ => slots.push(to64(v, by_iid(&iid))),
            In(f) => slots.push(to64(v, Some(f))),
            OutQ | Out(_) | OutPtr | Stat | Var => {
                if v == 0 {
                    slots.push(0);
                    continue;
                }
                temps.push([0; 10]);
                let i = temps.len() - 1;
                let t = temps[i].as_mut_ptr() as *mut u8;
                match k {
                    Stat => statstg64(v, t),
                    Var => variant64(v, t),
                    _ => {}
                }
                slots.push(t as u64);
                let (f, g) = match k {
                    OutQ => (by_iid(&iid), iid),
                    Out(f) => (Some(f), f.iid),
                    _ => (None, iid),
                };
                post.push((i, v, k, f, g));
            }
        }
    }
    let n = slots.len();
    while slots.len() < 4 {
        slots.push(0);
    }
    let mut xmm0 = 0u64;
    let mut r = super::winlux_wow_call64(target, slots.as_ptr(), n, &mut xmm0);
    for (i, dst, k, f, g) in post {
        let t = temps[i].as_ptr() as *const u8;
        match k {
            OutQ | Out(_) => {
                let v = temps[i][0];
                let x = to32(v, f, &g);
                if v != 0 && x == 0 && (r as i32) >= 0 {
                    r = E_NOINTERFACE as u64;
                }
                wr(dst, x);
            }
            OutPtr => wr(dst, temps[i][0] as u32),
            Stat => statstg32(t, dst),
            Var => variant32(t, dst),
            _ => {}
        }
    }
    r
}

/// Passerelle x86 d'une méthode d'un mandataire `P32`.
extern "win64" fn method32(args: u32, aux: usize) -> u64 {
    unsafe {
        let m = &*(aux as *const MRef);
        let this = rd(args);
        let obj = (*(this as usize as *const P32)).obj;
        let vt = *(obj as usize as *const *const usize);
        let target = *vt.add(m.idx);
        from32(target, Some(obj), args + 4, m.kinds.unwrap_or(&[]))
    }
}

/// Fonction exportée dont les arguments contiennent des interfaces.
struct Api {
    target: usize,
    kinds: &'static [A],
    /// L'interface est rendue comme valeur de retour.
    ret: Option<&'static Iface>,
}

extern "win64" fn api32(args: u32, aux: usize) -> u64 {
    unsafe {
        let a = &*(aux as *const Api);
        let r = from32(a.target, None, args, a.kinds);
        match a.ret {
            Some(f) => to32(r, Some(f), &f.iid) as u64,
            None => r,
        }
    }
}

// ---------------------------------------------------------------------
// Appels 64 → 32
// ---------------------------------------------------------------------

/// Passerelle win64 d'une méthode d'un mandataire `P64`.
extern "win64" fn method64(m: u32, args: *const u64) -> u64 {
    if super::teb32_addr() == 0 {
        crate::process::init_thread();
    }
    unsafe {
        let m = &*(m as usize as *const MRef);
        let p = &*(*args as usize as *const P64);
        let Some(kinds) = m.kinds else {
            crate::log_warn!(
                "wow32",
                "{} : méthode n° {} appelée par le runtime, non décrite pour les objets 32 bits",
                p.iface.name,
                m.idx
            );
            return E_NOTIMPL as u64;
        };
        let mut a: Vec<u32> = Vec::with_capacity(kinds.len() * 2 + 1);
        a.push(p.obj);
        // Cases basses (le tas est sous 4 Go) pour les valeurs rendues.
        let mut temps: Vec<[u64; 10]> = Vec::with_capacity(kinds.len());
        let mut post: Vec<(usize, u64, A, Option<&'static Iface>)> = Vec::new();
        let mut iid: Guid = [0; 16];
        for (i, &k) in kinds.iter().enumerate().take(11) {
            let v = *args.add(i + 1);
            match k {
                I => a.push(v as u32),
                Q => {
                    a.push(v as u32);
                    a.push((v >> 32) as u32);
                }
                Iid => {
                    if v != 0 {
                        iid = std::ptr::read_unaligned(v as usize as *const Guid);
                    }
                    // Les IID du runtime sont dans son image, au-dessus de 4 Go.
                    temps.push([0; 10]);
                    let t = temps.last_mut().unwrap().as_mut_ptr() as *mut Guid;
                    *t = iid;
                    a.push(if v == 0 { 0 } else { t as usize as u32 });
                }
                InQ => a.push(to32_borrowed(v, by_iid(&iid), &iid)),
                In(f) => a.push(to32_borrowed(v, Some(f), &f.iid)),
                OutQ | Out(_) | OutPtr | Stat | Var => {
                    if v == 0 {
                        a.push(0);
                        continue;
                    }
                    temps.push([0; 10]);
                    let j = temps.len() - 1;
                    let t = temps[j].as_mut_ptr() as *mut u8;
                    match k {
                        Stat => statstg32(v as usize as *const u8, t as usize as u32),
                        Var => variant32(v as usize as *const u8, t as usize as u32),
                        _ => {}
                    }
                    a.push(t as usize as u32);
                    let f = match k {
                        OutQ => by_iid(&iid),
                        Out(f) => Some(f),
                        _ => None,
                    };
                    post.push((j, v, k, f));
                }
            }
        }
        let target = rd(rd(p.obj) + 4 * m.idx as u32);
        let r = call32(target as usize, &a) as u32;
        for (j, dst, k, f) in post {
            let t = temps[j].as_ptr() as *const u8;
            let d = dst as usize as *mut u8;
            match k {
                OutQ | Out(_) => {
                    let x = *(t as *const u32);
                    (d as *mut u64).write_unaligned(to64(x, f));
                }
                OutPtr => (d as *mut u64).write_unaligned(*(t as *const u32) as u64),
                Stat => statstg64(t as usize as u32, d),
                Var => variant64(t as usize as u32, d),
                _ => {}
            }
        }
        r as u64
    }
}

/// Interface 64 bits prêtée au code x86 le temps d'un appel (sans
/// transfert de référence) : une interface non décrite garde un
/// mandataire réduit à IUnknown plutôt que d'être libérée.
fn to32_borrowed(v: u64, iface: Option<&'static Iface>, iid: &Guid) -> u32 {
    if v == 0 {
        return 0;
    }
    if unsafe { crate::dlls::legacy_com::is_x86_object(v as usize) } {
        return v as u32;
    }
    if let Some((x, _)) = maps(|m| m.p64_obj.get(&(v as usize)).copied()) {
        return x;
    }
    let f = iface.unwrap_or_else(|| {
        crate::log_warn!(
            "wow32",
            "interface COM {} prêtée au code 32 bits sans description",
            guid_str(iid)
        );
        &IUNKNOWN
    });
    if let Some(p) = maps(|m| m.p32.get(&(v, key(f))).copied()) {
        return p;
    }
    // to32 ne libère rien quand l'interface est connue.
    to32(v, Some(f), iid)
}

// ---------------------------------------------------------------------
// Fonctions exportées
// ---------------------------------------------------------------------

/// Arguments des fonctions d'ole32, oleaut32 et shlwapi qui passent des
/// interfaces, des VARIANT ou des pointeurs rendus.
/// Liste d'arguments statique (les statiques peuvent pointer sur les
/// descriptions d'interfaces, pas les temporaires).
macro_rules! k {
    ($($e:expr),*) => {{
        static K: &[A] = &[$($e),*];
        K
    }};
}

fn api_kinds(dll: &str, func: &str) -> Option<(&'static [A], Option<&'static Iface>)> {
    let k: &'static [A] = match (dll, func) {
        ("ole32", "CoCreateInstance") => k!(I, In(&IUNKNOWN), I, Iid, OutQ),
        ("ole32", "CoGetClassObject") => k!(I, I, I, Iid, OutQ),
        ("ole32", "CoRegisterClassObject") => k!(I, In(&ICLASSFACTORY), I, I, I),
        ("ole32", "CreateStreamOnHGlobal") => k!(I, I, Out(&ISTREAM)),
        ("ole32", "GetHGlobalFromStream") => k!(In(&ISTREAM), OutPtr),
        ("ole32", "CoGetMalloc") => k!(I, Out(&IMALLOC)),
        ("ole32", "StringFromCLSID" | "StringFromIID" | "ProgIDFromCLSID") => k!(I, OutPtr),
        ("ole32", "CoCreateFreeThreadedMarshaler") => k!(In(&IUNKNOWN), Out(&IUNKNOWN)),
        ("ole32", "CoMarshalInterThreadInterfaceInStream") => k!(Iid, InQ, Out(&ISTREAM)),
        ("ole32", "CoGetInterfaceAndReleaseStream") => k!(In(&ISTREAM), Iid, OutQ),
        ("ole32", "CoGetContextToken") => k!(OutPtr),
        ("ole32", "OleGetClipboard") => k!(Out(&IDATAOBJECT)),
        ("ole32", "OleSetClipboard") => k!(In(&IDATAOBJECT)),
        ("ole32", "RegisterDragDrop") => k!(I, In(&IDROPTARGET)),
        ("ole32", "DoDragDrop") => k!(In(&IDATAOBJECT), In(&IDROPSOURCE), I, I),
        ("oleaut32", "VariantInit" | "VariantClear" | "#8" | "#9") => k!(Var),
        ("oleaut32", "VariantCopy" | "#10") => k!(Var, Var),
        ("oleaut32", "VariantChangeType" | "#12") => k!(Var, Var, I, I),
        ("oleaut32", "SafeArrayAccessData" | "#23") => k!(I, OutPtr),
        ("oleaut32", "SafeArrayPtrOfIndex" | "#148") => k!(I, I, OutPtr),
        ("oleaut32", "VariantCopyInd" | "#11") => k!(Var, Var),
        ("oleaut32", "GetActiveObject" | "#35") => k!(I, I, Out(&IUNKNOWN)),
        ("ole32", "CreateBindCtx" | "OleRegEnumVerbs") => k!(I, OutPtr),
        ("shell32", "SHGetMalloc") => k!(Out(&IMALLOC)),
        ("comctl32", "ImageList_Read") => k!(In(&ISTREAM)),
        ("comctl32", "ImageList_Write") => k!(I, In(&ISTREAM)),
        ("oleaut32", "GetErrorInfo" | "#200") | ("ole32", "GetErrorInfo") => {
            k!(I, Out(&IERRORINFO))
        }
        ("oleaut32", "SetErrorInfo" | "#201") | ("ole32", "SetErrorInfo") => k!(I, In(&IERRORINFO)),
        ("oleaut32", "CreateErrorInfo" | "#202") | ("ole32", "CreateErrorInfo") => {
            k!(Out(&ICREATEERRORINFO))
        }
        ("oleaut32", "LoadTypeLib" | "#161") => k!(I, Out(&ITYPELIB)),
        ("oleaut32", "LoadTypeLibEx" | "#183") => k!(I, I, Out(&ITYPELIB)),
        ("oleaut32", "LoadRegTypeLib" | "#162") => k!(I, I, I, I, Out(&ITYPELIB)),
        ("shlwapi", "SHCreateStreamOnFileA" | "SHCreateStreamOnFileW") => k!(I, I, Out(&ISTREAM)),
        ("shlwapi", "SHCreateStreamOnFileEx") => k!(I, I, I, I, I, Out(&ISTREAM)),
        ("urlmon", "CreateURLMoniker") => k!(I, I, Out(&IMONIKER)),
        ("urlmon", "CreateURLMonikerEx") => k!(I, I, Out(&IMONIKER), I),
        ("shlwapi", "SHOpenRegStreamA" | "SHOpenRegStreamW") => {
            return Some((k!(I, I, I, I), Some(&ISTREAM)))
        }
        _ => return None,
    };
    Some((k, None))
}

/// Passerelle x86 propre aux fonctions qui passent des interfaces.
pub(super) fn override_for(dll: &str, func: &str) -> Option<super::crt::Override> {
    let (kinds, ret) = api_kinds(dll, func)?;
    let target = crate::dlls::lookup(dll, func)?;
    let api = Box::leak(Box::new(Api { target, kinds, ret }));
    Some(super::crt::Override::Raw {
        target: api32 as *const () as usize,
        aux: api as *const Api as usize,
        pop: Some((slots(kinds) * 4) as u16),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methodes_heritees() {
        assert_eq!(ISTREAM.count(), 14);
        assert!(ISTREAM.method(13).is_some());
        assert!(ISTREAM.derives(&ISEQUENTIALSTREAM));
        assert!(ISTREAM.derives(&IUNKNOWN));
        assert!(!IMALLOC.derives(&ISTREAM));
        assert_eq!(slots(ISTREAM.method(5).unwrap()), 4);
        assert!(by_iid(&ole(0xC)).is_some_and(|f| std::ptr::eq(f, &ISTREAM)));
    }

    #[test]
    fn fonctions_decrites() {
        assert!(api_kinds("ole32", "CoCreateInstance").is_some());
        assert!(api_kinds("ole32", "CoTaskMemAlloc").is_none());
        let (k, r) = api_kinds("shlwapi", "SHOpenRegStreamW").unwrap();
        assert_eq!(slots(k), 4);
        assert!(r.is_some());
    }
}
