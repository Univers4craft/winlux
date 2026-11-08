//! Bibliothèques de types en mémoire (#1108) : `CreateTypeLib2`,
//! `ICreateTypeLib2` / `ICreateTypeInfo2`, et les `ITypeLib` / `ITypeInfo`
//! qu'elles produisent.
//!
//! Écrit d'après la documentation publique Microsoft Learn de ces
//! interfaces et l'ordre de leurs méthodes dans oaidl.h (en-têtes
//! mingw-w64). Le format binaire des fichiers .tlb n'a pas de
//! spécification publique : il n'est ni lu ni écrit. `SaveAllChanges`
//! le dit (E_NOTIMPL), mais la bibliothèque reste utilisable en mémoire,
//! et `RegisterTypeLib` l'inscrit pour le processus : `LoadRegTypeLib` et
//! `LoadTypeLib` (par son nom de fichier) la retrouvent ensuite.
//!
//! La bibliothèque standard « stdole » (IUnknown, IDispatch) est fournie,
//! construite par ces mêmes structures depuis leurs déclarations publiques
//! (unknwn.h, oaidl.h) : c'est elle que les créateurs de bibliothèques
//! chargent pour faire hériter leurs interfaces d'IDispatch. GUID, DISPPARAMS
//! et EXCEPINFO y sont décrits comme des pointeurs sans type (VT_PTR vers
//! VT_VOID).
//!
//! Dispositions x64 vérifiées avec mingw-w64 (offsetof) : TYPEDESC 16,
//! ELEMDESC 32, FUNCDESC 88, VARDESC 64, TYPEATTR 96, TLIBATTR 32,
//! ARRAYDESC 20 + 8 par dimension, PARAMDESCEX 32. Programmes 32 bits :
//! E_NOTIMPL (dispositions non traduites).

use super::ole32::Guid;
use super::oleaut32_disp::Method;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const TYPE_E_ELEMENTNOTFOUND: i32 = 0x8002_802Bu32 as i32;
const TYPE_E_NAMECONFLICT: i32 = 0x8002_8C83u32 as i32;
const TYPE_E_LIBNOTREGISTERED: i32 = 0x8002_801Du32 as i32;
const TYPE_E_CANTLOADLIBRARY: i32 = 0x8002_9C4Au32 as i32;
const TYPE_E_BADMODULEKIND: i32 = 0x8002_80BDu32 as i32;
const DISP_E_MEMBERNOTFOUND: i32 = 0x8002_0003u32 as i32;

const MEMBERID_NIL: i32 = -1;

// TYPEKIND
const TKIND_ENUM: u32 = 0;
const TKIND_RECORD: u32 = 1;
const TKIND_INTERFACE: u32 = 3;
const TKIND_DISPATCH: u32 = 4;
const TKIND_MAX: u32 = 8;

// VARTYPE
const VT_I4: u16 = 3;
const VT_UI2: u16 = 18;
const VT_UI4: u16 = 19;
const VT_INT: u16 = 22;
const VT_UINT: u16 = 23;
const VT_VOID: u16 = 24;
const VT_HRESULT: u16 = 25;
const VT_PTR: u16 = 26;
const VT_SAFEARRAY: u16 = 27;
const VT_CARRAY: u16 = 28;
const VT_USERDEFINED: u16 = 29;
const VT_ARRAY: u16 = 0x2000;
const VT_BYREF: u16 = 0x4000;

const PARAMFLAG_FRETVAL: u16 = 0x8;
const PARAMFLAG_FHASDEFAULT: u16 = 0x20;

const FUNC_VIRTUAL: u32 = 0;
const FUNC_PUREVIRTUAL: u32 = 1;
const FUNCFLAG_FRESTRICTED: u16 = 1;
const VAR_CONST: u32 = 2;
const TYPEFLAG_FHIDDEN: u16 = 0x10;
const TYPEFLAG_FDUAL: u16 = 0x40;
const TYPEFLAG_FDISPATCHABLE: u16 = 0x1000;
const SYS_WIN64: u32 = 3;

const VARIANT_SIZE: usize = 24;

const fn ole_guid(data1: u32) -> Guid {
    Guid {
        data1,
        data2: 0,
        data3: 0,
        data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
    }
}
const IID_IUNKNOWN: Guid = ole_guid(0);
const IID_IDISPATCH: Guid = ole_guid(0x0002_0400);
const IID_ITYPEINFO: Guid = ole_guid(0x0002_0401);
const IID_ITYPELIB: Guid = ole_guid(0x0002_0402);
const IID_ICREATETYPEINFO: Guid = ole_guid(0x0002_0405);
const IID_ICREATETYPELIB: Guid = ole_guid(0x0002_0406);
const IID_ICREATETYPEINFO2: Guid = ole_guid(0x0002_040E);
const IID_ICREATETYPELIB2: Guid = ole_guid(0x0002_040F);
/// Bibliothèque standard OLE Automation (stdole2.tlb).
pub const LIBID_STDOLE: Guid = ole_guid(0x0002_0430);

// ── Données ────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct TypeDesc {
    vt: u16,
    inner: Option<Box<TypeDesc>>,
    href: u32,
    dims: Vec<(u32, i32)>,
}

impl TypeDesc {
    fn plain(vt: u16) -> Self {
        TypeDesc {
            vt,
            ..Default::default()
        }
    }
    fn ptr(to: TypeDesc) -> Self {
        TypeDesc {
            vt: VT_PTR,
            inner: Some(Box::new(to)),
            ..Default::default()
        }
    }
}

#[derive(Clone, Default)]
struct Elem {
    td: TypeDesc,
    flags: u16,
    default: Option<[u8; VARIANT_SIZE]>,
}

#[derive(Clone, Default)]
struct Func {
    memid: i32,
    funckind: u32,
    invkind: u32,
    callconv: u32,
    params: Vec<Elem>,
    ret: Elem,
    opt: i16,
    ovft: i16,
    flags: u16,
    scodes: Vec<i32>,
    names: Vec<String>,
    doc: String,
    help: u32,
}

#[derive(Clone, Default)]
struct Var {
    memid: i32,
    kind: u32,
    elem: Elem,
    value: Option<[u8; VARIANT_SIZE]>,
    oinst: u32,
    flags: u16,
    name: String,
    doc: String,
    help: u32,
}

#[derive(Default)]
struct InfoData {
    name: String,
    kind: u32,
    guid: Guid,
    flags: u16,
    doc: String,
    help: u32,
    major: u16,
    minor: u16,
    align: u16,
    alias: TypeDesc,
    funcs: Vec<Func>,
    vars: Vec<Var>,
    /// (HREFTYPE, IMPLTYPEFLAGS)
    impls: Vec<(u32, i32)>,
    /// Types référencés : ITypeInfo (référence tenue sauf pour un type de
    /// la même bibliothèque, qui vit autant qu'elle).
    refs: Vec<(usize, bool)>,
    vft: u16,
}

/// Objet d'un type : ICreateTypeInfo2 à +0, ITypeInfo à +8. Son compteur
/// est celui de la bibliothèque, qui libère tout à zéro.
#[repr(C)]
struct InfoObj {
    create_vtbl: *const usize,
    info_vtbl: *const usize,
    lib: *const LibObj,
    id: u32,
}

struct InfoEntry {
    obj: Box<InfoObj>,
    d: InfoData,
}

#[derive(Default)]
struct LibData {
    name: String,
    guid: Guid,
    lcid: u32,
    major: u16,
    minor: u16,
    flags: u16,
    doc: String,
    help: u32,
    help_file: String,
    file: String,
    infos: Vec<InfoEntry>,
    next_id: u32,
}

/// Objet bibliothèque : ICreateTypeLib2 à +0, ITypeLib à +8.
#[repr(C)]
struct LibObj {
    create_vtbl: *const usize,
    lib_vtbl: *const usize,
    refs: AtomicU32,
    data: Mutex<LibData>,
}

struct Vtbl<const N: usize>([*const (); N]);
unsafe impl<const N: usize> Sync for Vtbl<N> {}

fn lock(lib: &LibObj) -> MutexGuard<'_, LibData> {
    lib.data.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn lib_of_create(this: usize) -> &'static LibObj {
    &*(this as *const LibObj)
}
unsafe fn lib_of_tlib(this: usize) -> &'static LibObj {
    &*((this - 8) as *const LibObj)
}
unsafe fn info_of_create(this: usize) -> &'static InfoObj {
    &*(this as *const InfoObj)
}
unsafe fn info_of_tinfo(this: usize) -> &'static InfoObj {
    &*((this - 8) as *const InfoObj)
}

fn entry(d: &mut LibData, id: u32) -> Option<&mut InfoEntry> {
    d.infos.iter_mut().find(|e| e.obj.id == id)
}

fn lib_add_ref(lib: &LibObj) -> u32 {
    lib.refs.fetch_add(1, Ordering::SeqCst) + 1
}

unsafe fn lib_release(lib: *const LibObj) -> u32 {
    let left = (*lib).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if left == 0 {
        let boxed = Box::from_raw(lib as *mut LibObj);
        let mut d = boxed.data.into_inner().unwrap_or_else(|e| e.into_inner());
        for e in d.infos.drain(..) {
            free_info_data(e.d);
        }
    }
    left
}

unsafe fn free_info_data(d: InfoData) {
    for (p, held) in d.refs {
        if held {
            release(p);
        }
    }
    for f in d.funcs {
        for e in f.params {
            clear_default(e);
        }
    }
    for v in d.vars {
        clear_default(v.elem);
        if let Some(mut val) = v.value {
            super::oleaut32::variant_clear(val.as_mut_ptr().cast());
        }
    }
}

unsafe fn clear_default(e: Elem) {
    if let Some(mut v) = e.default {
        super::oleaut32::variant_clear(v.as_mut_ptr().cast());
    }
}

unsafe fn vcall(obj: usize, slot: usize) -> usize {
    *(*(obj as *const *const usize)).add(slot)
}
unsafe fn add_ref(obj: usize) -> u32 {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(vcall(obj, 1));
    f(obj)
}
unsafe fn release(obj: usize) -> u32 {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(vcall(obj, 2));
    f(obj)
}

pub(super) unsafe fn wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| *p.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

unsafe fn bstr(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().collect();
    super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32)
}

unsafe fn put_bstr(out: *mut *mut u16, s: &str) {
    if !out.is_null() {
        *out = bstr(s);
    }
}

// ── Lecture des descriptions de l'appelant ─────────────────────────

unsafe fn read_tdesc(p: *const u8, depth: u32) -> Option<TypeDesc> {
    if p.is_null() || depth > 16 {
        return None;
    }
    let vt = *(p.add(8) as *const u16);
    let mut td = TypeDesc::plain(vt);
    match vt {
        VT_PTR | VT_SAFEARRAY => {
            td.inner = Some(Box::new(read_tdesc(*(p as *const *const u8), depth + 1)?));
        }
        VT_CARRAY => {
            let a = *(p as *const *const u8);
            if a.is_null() {
                return None;
            }
            td.inner = Some(Box::new(read_tdesc(a, depth + 1)?));
            let n = *(a.add(16) as *const u16) as usize;
            for i in 0..n.min(64) {
                let b = a.add(20 + i * 8);
                td.dims
                    .push((*(b as *const u32), *(b.add(4) as *const i32)));
            }
        }
        VT_USERDEFINED => td.href = *(p as *const u32),
        _ => {}
    }
    Some(td)
}

unsafe fn read_elem(p: *const u8) -> Option<Elem> {
    let td = read_tdesc(p, 0)?;
    let flags = *(p.add(24) as *const u16);
    let ex = *(p.add(16) as *const *const u8);
    let mut default = None;
    if flags & PARAMFLAG_FHASDEFAULT != 0 && !ex.is_null() {
        let mut v = [0u8; VARIANT_SIZE];
        super::oleaut32::variant_copy(v.as_mut_ptr().cast(), ex.add(8).cast());
        default = Some(v);
    }
    Some(Elem { td, flags, default })
}

// ── Écriture des descriptions rendues ──────────────────────────────

/// Blocs rendus par GetTypeAttr / GetFuncDesc / GetVarDesc / GetLibAttr,
/// libérés par les Release* correspondants : (blocs, VARIANT à vider).
type Block = (Vec<Box<[u64]>>, Vec<usize>);
fn outstanding() -> &'static Mutex<HashMap<usize, Block>> {
    static M: OnceLock<Mutex<HashMap<usize, Block>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

struct Out {
    blocks: Vec<Box<[u64]>>,
    variants: Vec<usize>,
}

impl Out {
    fn new() -> Self {
        Out {
            blocks: Vec::new(),
            variants: Vec::new(),
        }
    }
    fn alloc(&mut self, bytes: usize) -> *mut u8 {
        let mut b = vec![0u64; bytes.div_ceil(8).max(1)].into_boxed_slice();
        let p = b.as_mut_ptr() as *mut u8;
        self.blocks.push(b);
        p
    }
    unsafe fn variant(&mut self, v: &[u8; VARIANT_SIZE], dst: *mut u8) {
        super::oleaut32::variant_copy(dst.cast(), v.as_ptr().cast());
        self.variants.push(dst as usize);
    }
    unsafe fn tdesc(&mut self, td: &TypeDesc, dst: *mut u8) {
        *(dst.add(8) as *mut u16) = td.vt;
        match td.vt {
            VT_PTR | VT_SAFEARRAY => {
                if let Some(inner) = &td.inner {
                    let p = self.alloc(16);
                    self.tdesc(inner, p);
                    *(dst as *mut usize) = p as usize;
                }
            }
            VT_CARRAY => {
                let a = self.alloc(20 + 8 * td.dims.len().max(1));
                if let Some(inner) = &td.inner {
                    self.tdesc(inner, a);
                }
                *(a.add(16) as *mut u16) = td.dims.len() as u16;
                for (i, (n, lb)) in td.dims.iter().enumerate() {
                    *(a.add(20 + i * 8) as *mut u32) = *n;
                    *(a.add(24 + i * 8) as *mut i32) = *lb;
                }
                *(dst as *mut usize) = a as usize;
            }
            VT_USERDEFINED => *(dst as *mut u32) = td.href,
            _ => {}
        }
    }
    unsafe fn elem(&mut self, e: &Elem, dst: *mut u8) {
        self.tdesc(&e.td, dst);
        *(dst.add(24) as *mut u16) = e.flags;
        if let Some(v) = &e.default {
            let ex = self.alloc(32);
            *(ex as *mut u32) = 32;
            self.variant(v, ex.add(8));
            *(dst.add(16) as *mut usize) = ex as usize;
        }
    }
    /// Garde les blocs jusqu'au Release* et rend le bloc principal.
    fn finish(mut self, main: *mut u8) -> *mut u8 {
        let blocks = std::mem::take(&mut self.blocks);
        let variants = std::mem::take(&mut self.variants);
        outstanding()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(main as usize, (blocks, variants));
        main
    }
}

unsafe fn release_block(p: usize) {
    let taken = outstanding()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&p);
    if let Some((_, variants)) = taken {
        for v in variants {
            super::oleaut32::variant_clear(v as *mut _);
        }
    }
}

// ── Mise en page de la table des méthodes ──────────────────────────

/// Taille de la table de méthodes d'un type : celle de l'interface de base
/// plus 8 octets par fonction virtuelle.
unsafe fn vft_size(d: &LibData, id: u32, depth: u32) -> u16 {
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return 0;
    };
    let base = base_vft(d, &e.d, depth);
    let own =
        e.d.funcs
            .iter()
            .filter(|f| matches!(f.funckind, FUNC_VIRTUAL | FUNC_PUREVIRTUAL))
            .count() as u16;
    base + own * 8
}

unsafe fn base_vft(d: &LibData, info: &InfoData, depth: u32) -> u16 {
    if depth > 32 || !matches!(info.kind, TKIND_INTERFACE | TKIND_DISPATCH) {
        return 0;
    }
    let Some(&(href, _)) = info.impls.first() else {
        return 0;
    };
    let Some(&(p, held)) = info.refs.get(href_index(href)) else {
        return 0;
    };
    if !held {
        // Type de la même bibliothèque (verrou déjà tenu).
        return vft_size(d, info_of_tinfo(p).id, depth + 1);
    }
    // Type d'une autre bibliothèque : son TYPEATTR.
    let mut attr = 0usize;
    let get: unsafe extern "win64" fn(usize, *mut usize) -> i32 = std::mem::transmute(vcall(p, 3));
    if get(p, &mut attr) < 0 || attr == 0 {
        return 0;
    }
    let size = *((attr + 54) as *const u16);
    let rel: unsafe extern "win64" fn(usize, usize) = std::mem::transmute(vcall(p, 19));
    rel(p, attr);
    size
}

const HREF_BASE: u32 = 0x100;
fn href_index(href: u32) -> usize {
    (href.wrapping_sub(HREF_BASE) / 4) as usize
}

/// LayOut : place les fonctions virtuelles à la suite de la table de base.
unsafe fn layout(d: &mut LibData, id: u32) {
    let size = vft_size(d, id, 0);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return;
    };
    let base = base_vft(d, &e.d, 0);
    let e = entry(d, id).unwrap();
    let mut slot = base as i16;
    for f in e.d.funcs.iter_mut() {
        if matches!(f.funckind, FUNC_VIRTUAL | FUNC_PUREVIRTUAL) {
            f.ovft = slot;
            slot += 8;
        }
    }
    e.d.vft = size;
}

// ── Objets ─────────────────────────────────────────────────────────

fn new_lib(d: LibData) -> *const LibObj {
    Box::into_raw(Box::new(LibObj {
        create_vtbl: CREATE_LIB_VTBL.0.as_ptr().cast(),
        lib_vtbl: TYPELIB_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        data: Mutex::new(d),
    }))
}

/// Ajoute un type à la bibliothèque et rend son identifiant.
fn push_info(lib: &LibObj, d: &mut LibData, data: InfoData) -> u32 {
    d.next_id += 1;
    let id = d.next_id;
    d.infos.push(InfoEntry {
        obj: Box::new(InfoObj {
            create_vtbl: CREATE_INFO_VTBL.0.as_ptr().cast(),
            info_vtbl: TYPEINFO_VTBL.0.as_ptr().cast(),
            lib,
            id,
        }),
        d: data,
    });
    id
}

fn tinfo_ptr(e: &InfoEntry) -> usize {
    &*e.obj as *const InfoObj as usize + 8
}

/// Vrai si `p` est une ITypeInfo de cette bibliothèque.
unsafe fn same_lib(lib: &LibObj, p: usize) -> bool {
    *(p as *const *const usize) == TYPEINFO_VTBL.0.as_ptr().cast()
        && std::ptr::eq(info_of_tinfo(p).lib, lib)
}

/// CreateTypeLib2 : nouvelle bibliothèque vide (SYS_WIN32 ou SYS_WIN64).
pub unsafe extern "win64" fn create_type_lib2(
    syskind: u32,
    file: *const u16,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if file.is_null() || syskind > SYS_WIN64 {
        return E_INVALIDARG;
    }
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let lib = new_lib(LibData {
        file: wide(file),
        ..Default::default()
    });
    *out = lib as usize;
    S_OK
}

/// CreateTypeLib (ICreateTypeLib) : même objet.
pub unsafe extern "win64" fn create_type_lib(
    syskind: u32,
    file: *const u16,
    out: *mut usize,
) -> i32 {
    create_type_lib2(syskind, file, out)
}

// ── ICreateTypeLib2 ────────────────────────────────────────────────

static CREATE_LIB_VTBL: Vtbl<17> = Vtbl([
    ctl_query_interface as *const (),
    ctl_add_ref as *const (),
    ctl_release as *const (),
    ctl_create_type_info as *const (),
    ctl_set_name as *const (),
    ctl_set_version as *const (),
    ctl_set_guid as *const (),
    ctl_set_doc_string as *const (),
    ctl_set_help_file_name as *const (),
    ctl_set_help_context as *const (),
    ctl_set_lcid as *const (),
    ctl_set_lib_flags as *const (),
    ctl_save_all_changes as *const (),
    ctl_delete_type_info as *const (),
    ctl_set_cust_data as *const (),
    ctl_set_help_context as *const (), // SetHelpStringContext
    ctl_set_name as *const (),         // SetHelpStringDll (gardé, sans effet)
]);

unsafe fn lib_query(lib: &LibObj, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let base = lib as *const LibObj as usize;
    let p = match *iid {
        g if g == IID_IUNKNOWN || g == IID_ICREATETYPELIB || g == IID_ICREATETYPELIB2 => base,
        g if g == IID_ITYPELIB => base + 8,
        _ => return E_NOINTERFACE,
    };
    lib_add_ref(lib);
    *out = p;
    S_OK
}

unsafe extern "win64" fn ctl_query_interface(
    this: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    lib_query(lib_of_create(this), iid, out)
}
unsafe extern "win64" fn ctl_add_ref(this: usize) -> u32 {
    lib_add_ref(lib_of_create(this))
}
unsafe extern "win64" fn ctl_release(this: usize) -> u32 {
    lib_release(lib_of_create(this))
}

unsafe extern "win64" fn ctl_create_type_info(
    this: usize,
    name: *const u16,
    kind: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if name.is_null() || kind >= TKIND_MAX {
        return E_INVALIDARG;
    }
    let lib = lib_of_create(this);
    let name = wide(name);
    let mut d = lock(lib);
    if d.infos.iter().any(|e| e.d.name.eq_ignore_ascii_case(&name)) {
        return TYPE_E_NAMECONFLICT;
    }
    let align = if matches!(kind, TKIND_INTERFACE | TKIND_DISPATCH) {
        8
    } else {
        4
    };
    let id = push_info(
        lib,
        &mut d,
        InfoData {
            name,
            kind,
            align,
            ..Default::default()
        },
    );
    lib_add_ref(lib);
    *out = &*entry(&mut d, id).unwrap().obj as *const InfoObj as usize;
    S_OK
}

unsafe extern "win64" fn ctl_set_name(this: usize, name: *const u16) -> i32 {
    if name.is_null() {
        return E_INVALIDARG;
    }
    lock(lib_of_create(this)).name = wide(name);
    S_OK
}
unsafe extern "win64" fn ctl_set_version(this: usize, major: u16, minor: u16) -> i32 {
    let mut d = lock(lib_of_create(this));
    d.major = major;
    d.minor = minor;
    S_OK
}
unsafe extern "win64" fn ctl_set_guid(this: usize, guid: *const Guid) -> i32 {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    lock(lib_of_create(this)).guid = *guid;
    S_OK
}
unsafe extern "win64" fn ctl_set_doc_string(this: usize, doc: *const u16) -> i32 {
    lock(lib_of_create(this)).doc = wide(doc);
    S_OK
}
unsafe extern "win64" fn ctl_set_help_file_name(this: usize, file: *const u16) -> i32 {
    lock(lib_of_create(this)).help_file = wide(file);
    S_OK
}
unsafe extern "win64" fn ctl_set_help_context(this: usize, ctx: u32) -> i32 {
    lock(lib_of_create(this)).help = ctx;
    S_OK
}
unsafe extern "win64" fn ctl_set_lcid(this: usize, lcid: u32) -> i32 {
    lock(lib_of_create(this)).lcid = lcid;
    S_OK
}
unsafe extern "win64" fn ctl_set_lib_flags(this: usize, flags: u32) -> i32 {
    lock(lib_of_create(this)).flags = flags as u16;
    S_OK
}
/// SaveAllChanges : le format .tlb n'est pas publié, rien n'est écrit.
unsafe extern "win64" fn ctl_save_all_changes(this: usize) -> i32 {
    let file = lock(lib_of_create(this)).file.clone();
    crate::log_warn!(
        "oleaut32",
        "SaveAllChanges : bibliothèque de types « {file} » gardée en mémoire, format .tlb non publié (#1108)"
    );
    E_NOTIMPL
}
unsafe extern "win64" fn ctl_delete_type_info(this: usize, name: *const u16) -> i32 {
    let name = wide(name);
    let mut d = lock(lib_of_create(this));
    let Some(i) = d
        .infos
        .iter()
        .position(|e| e.d.name.eq_ignore_ascii_case(&name))
    else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    // L'objet peut encore être tenu par l'appelant : on ne garde que lui.
    let e = d.infos.remove(i);
    std::mem::forget(e.obj);
    free_info_data(e.d);
    S_OK
}
/// Données personnalisées : acceptées, non conservées.
unsafe extern "win64" fn ctl_set_cust_data(_this: usize, guid: *const Guid, var: *const u8) -> i32 {
    if guid.is_null() || var.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}

// ── ITypeLib ───────────────────────────────────────────────────────

static TYPELIB_VTBL: Vtbl<13> = Vtbl([
    tl_query_interface as *const (),
    tl_add_ref as *const (),
    tl_release as *const (),
    tl_get_type_info_count as *const (),
    tl_get_type_info as *const (),
    tl_get_type_info_type as *const (),
    tl_get_type_info_of_guid as *const (),
    tl_get_lib_attr as *const (),
    tl_get_type_comp as *const (),
    tl_get_documentation as *const (),
    tl_is_name as *const (),
    tl_find_name as *const (),
    tl_release_tlib_attr as *const (),
]);

unsafe extern "win64" fn tl_query_interface(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    lib_query(lib_of_tlib(this), iid, out)
}
unsafe extern "win64" fn tl_add_ref(this: usize) -> u32 {
    lib_add_ref(lib_of_tlib(this))
}
unsafe extern "win64" fn tl_release(this: usize) -> u32 {
    lib_release(lib_of_tlib(this))
}
unsafe extern "win64" fn tl_get_type_info_count(this: usize) -> u32 {
    lock(lib_of_tlib(this)).infos.len() as u32
}
unsafe extern "win64" fn tl_get_type_info(this: usize, index: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let lib = lib_of_tlib(this);
    let d = lock(lib);
    let Some(e) = d.infos.get(index as usize) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    *out = tinfo_ptr(e);
    lib_add_ref(lib);
    S_OK
}
unsafe extern "win64" fn tl_get_type_info_type(this: usize, index: u32, kind: *mut u32) -> i32 {
    if kind.is_null() {
        return E_INVALIDARG;
    }
    let d = lock(lib_of_tlib(this));
    let Some(e) = d.infos.get(index as usize) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    *kind = e.d.kind;
    S_OK
}
unsafe extern "win64" fn tl_get_type_info_of_guid(
    this: usize,
    guid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() || guid.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let lib = lib_of_tlib(this);
    let d = lock(lib);
    let Some(e) = d.infos.iter().find(|e| e.d.guid == *guid) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    *out = tinfo_ptr(e);
    lib_add_ref(lib);
    S_OK
}
unsafe extern "win64" fn tl_get_lib_attr(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let d = lock(lib_of_tlib(this));
    let mut o = Out::new();
    let a = o.alloc(32);
    *(a as *mut Guid) = d.guid;
    *(a.add(16) as *mut u32) = d.lcid;
    *(a.add(20) as *mut u32) = SYS_WIN64;
    *(a.add(24) as *mut u16) = d.major;
    *(a.add(26) as *mut u16) = d.minor;
    *(a.add(28) as *mut u16) = d.flags;
    *out = o.finish(a) as usize;
    S_OK
}
unsafe extern "win64" fn tl_get_type_comp(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
/// GetDocumentation : index -1 pour la bibliothèque elle-même.
unsafe extern "win64" fn tl_get_documentation(
    this: usize,
    index: i32,
    name: *mut *mut u16,
    doc: *mut *mut u16,
    help: *mut u32,
    help_file: *mut *mut u16,
) -> i32 {
    let d = lock(lib_of_tlib(this));
    let (n, s, h) = if index == -1 {
        (d.name.clone(), d.doc.clone(), d.help)
    } else {
        let Some(e) = d.infos.get(index as usize) else {
            return TYPE_E_ELEMENTNOTFOUND;
        };
        (e.d.name.clone(), e.d.doc.clone(), e.d.help)
    };
    put_bstr(name, &n);
    put_bstr(doc, &s);
    if !help.is_null() {
        *help = h;
    }
    put_bstr(help_file, &d.help_file);
    S_OK
}

/// Noms d'un type et de ses membres.
fn names_in(e: &InfoEntry) -> impl Iterator<Item = (&str, i32)> {
    std::iter::once((e.d.name.as_str(), MEMBERID_NIL))
        .chain(
            e.d.funcs
                .iter()
                .filter_map(|f| f.names.first().map(|n| (n.as_str(), f.memid))),
        )
        .chain(e.d.vars.iter().map(|v| (v.name.as_str(), v.memid)))
}

/// IsName : le nom existe-t-il (sans casse) ? Sa casse exacte est recopiée.
unsafe extern "win64" fn tl_is_name(
    this: usize,
    name: *mut u16,
    _hash: u32,
    found: *mut i32,
) -> i32 {
    if name.is_null() || found.is_null() {
        return E_INVALIDARG;
    }
    *found = 0;
    let wanted = wide(name);
    let d = lock(lib_of_tlib(this));
    for e in &d.infos {
        if let Some((exact, _)) = names_in(e).find(|(n, _)| n.eq_ignore_ascii_case(&wanted)) {
            let w: Vec<u16> = exact.encode_utf16().collect();
            if w.len() == wanted.encode_utf16().count() {
                std::ptr::copy_nonoverlapping(w.as_ptr(), name, w.len());
            }
            *found = 1;
            break;
        }
    }
    S_OK
}

/// FindName : jusqu'à `*count` types contenant le nom.
unsafe extern "win64" fn tl_find_name(
    this: usize,
    name: *const u16,
    _hash: u32,
    infos: *mut usize,
    memids: *mut i32,
    count: *mut u16,
) -> i32 {
    if name.is_null() || infos.is_null() || memids.is_null() || count.is_null() {
        return E_INVALIDARG;
    }
    let wanted = wide(name);
    let lib = lib_of_tlib(this);
    let d = lock(lib);
    let max = *count as usize;
    let mut n = 0usize;
    for e in &d.infos {
        if n >= max {
            break;
        }
        if let Some((_, memid)) = names_in(e).find(|(x, _)| x.eq_ignore_ascii_case(&wanted)) {
            *infos.add(n) = tinfo_ptr(e);
            *memids.add(n) = memid;
            lib_add_ref(lib);
            n += 1;
        }
    }
    *count = n as u16;
    S_OK
}
unsafe extern "win64" fn tl_release_tlib_attr(_this: usize, attr: usize) {
    release_block(attr);
}

// ── ICreateTypeInfo2 ───────────────────────────────────────────────

static CREATE_INFO_VTBL: Vtbl<41> = Vtbl([
    cti_query_interface as *const (),
    cti_add_ref as *const (),
    cti_release as *const (),
    cti_set_guid as *const (),
    cti_set_type_flags as *const (),
    cti_set_doc_string as *const (),
    cti_set_help_context as *const (),
    cti_set_version as *const (),
    cti_add_ref_type_info as *const (),
    cti_add_func_desc as *const (),
    cti_add_impl_type as *const (),
    cti_set_impl_type_flags as *const (),
    cti_set_alignment as *const (),
    cti_set_schema as *const (),
    cti_add_var_desc as *const (),
    cti_set_func_and_param_names as *const (),
    cti_set_var_name as *const (),
    cti_set_type_desc_alias as *const (),
    cti_define_func_as_dll_entry as *const (),
    cti_set_func_doc_string as *const (),
    cti_set_var_doc_string as *const (),
    cti_set_func_help_context as *const (),
    cti_set_var_help_context as *const (),
    cti_set_mops as *const (),
    cti_set_type_idldesc as *const (),
    cti_lay_out as *const (),
    cti_delete_func_desc as *const (),
    cti_delete_func_desc_by_mem_id as *const (),
    cti_delete_var_desc as *const (),
    cti_delete_var_desc_by_mem_id as *const (),
    cti_delete_impl_type as *const (),
    cti_set_cust_data as *const (),
    cti_set_index_cust_data as *const (), // SetFuncCustData
    cti_set_param_cust_data as *const (),
    cti_set_index_cust_data as *const (), // SetVarCustData
    cti_set_index_cust_data as *const (), // SetImplTypeCustData
    cti_set_help_string_context as *const (),
    cti_set_index_help_string_context as *const (), // SetFuncHelpStringContext
    cti_set_index_help_string_context as *const (), // SetVarHelpStringContext
    cti_invalidate as *const (),
    cti_set_name as *const (),
]);

unsafe fn info_query(info: &InfoObj, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let base = info as *const InfoObj as usize;
    let p = match *iid {
        g if g == IID_IUNKNOWN || g == IID_ICREATETYPEINFO || g == IID_ICREATETYPEINFO2 => base,
        g if g == IID_ITYPEINFO => base + 8,
        _ => return E_NOINTERFACE,
    };
    lib_add_ref(&*info.lib);
    *out = p;
    S_OK
}

/// Applique `f` aux données du type, sous le verrou de sa bibliothèque.
unsafe fn with_info<R>(info: &InfoObj, f: impl FnOnce(&mut InfoData) -> R) -> Option<R> {
    let mut d = lock(&*info.lib);
    entry(&mut d, info.id).map(|e| f(&mut e.d))
}

unsafe fn edit(this: usize, f: impl FnOnce(&mut InfoData) -> i32) -> i32 {
    with_info(info_of_create(this), f).unwrap_or(TYPE_E_ELEMENTNOTFOUND)
}

unsafe extern "win64" fn cti_query_interface(
    this: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    info_query(info_of_create(this), iid, out)
}
unsafe extern "win64" fn cti_add_ref(this: usize) -> u32 {
    lib_add_ref(&*info_of_create(this).lib)
}
unsafe extern "win64" fn cti_release(this: usize) -> u32 {
    lib_release(info_of_create(this).lib)
}
unsafe extern "win64" fn cti_set_guid(this: usize, guid: *const Guid) -> i32 {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    edit(this, |d| {
        d.guid = *guid;
        S_OK
    })
}
unsafe extern "win64" fn cti_set_type_flags(this: usize, flags: u32) -> i32 {
    edit(this, |d| {
        d.flags = flags as u16;
        S_OK
    })
}
unsafe extern "win64" fn cti_set_doc_string(this: usize, doc: *const u16) -> i32 {
    let doc = wide(doc);
    edit(this, |d| {
        d.doc = doc;
        S_OK
    })
}
unsafe extern "win64" fn cti_set_help_context(this: usize, ctx: u32) -> i32 {
    edit(this, |d| {
        d.help = ctx;
        S_OK
    })
}
unsafe extern "win64" fn cti_set_version(this: usize, major: u16, minor: u16) -> i32 {
    edit(this, |d| {
        d.major = major;
        d.minor = minor;
        S_OK
    })
}
/// AddRefTypeInfo : HREFTYPE d'un type référencé (même type → même valeur).
unsafe extern "win64" fn cti_add_ref_type_info(this: usize, tinfo: usize, out: *mut u32) -> i32 {
    if tinfo == 0 || out.is_null() {
        return E_INVALIDARG;
    }
    let info = info_of_create(this);
    let lib = &*info.lib;
    let same = same_lib(lib, tinfo);
    let mut d = lock(lib);
    let Some(e) = entry(&mut d, info.id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let i = match e.d.refs.iter().position(|(p, _)| *p == tinfo) {
        Some(i) => i,
        None => {
            if !same {
                add_ref(tinfo);
            }
            e.d.refs.push((tinfo, !same));
            e.d.refs.len() - 1
        }
    };
    *out = HREF_BASE + i as u32 * 4;
    S_OK
}
unsafe extern "win64" fn cti_add_func_desc(this: usize, index: u32, fd: *const u8) -> i32 {
    if fd.is_null() {
        return E_INVALIDARG;
    }
    let count = (*(fd.add(36) as *const i16)).max(0) as usize;
    let pparams = *(fd.add(16) as *const *const u8);
    if count > 0 && pparams.is_null() {
        return E_INVALIDARG;
    }
    let mut params = Vec::with_capacity(count);
    for i in 0..count {
        let Some(e) = read_elem(pparams.add(i * 32)) else {
            return E_INVALIDARG;
        };
        params.push(e);
    }
    let Some(ret) = read_elem(fd.add(48)) else {
        return E_INVALIDARG;
    };
    let nscodes = (*(fd.add(42) as *const i16)).max(0) as usize;
    let pscodes = *(fd.add(8) as *const *const i32);
    let scodes = if pscodes.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(pscodes, nscodes).to_vec()
    };
    let func = Func {
        memid: *(fd as *const i32),
        funckind: *(fd.add(24) as *const u32),
        invkind: *(fd.add(28) as *const u32),
        callconv: *(fd.add(32) as *const u32),
        params,
        ret,
        opt: *(fd.add(38) as *const i16),
        ovft: *(fd.add(40) as *const i16),
        flags: *(fd.add(80) as *const u16),
        scodes,
        ..Default::default()
    };
    edit(this, |d| {
        if index as usize > d.funcs.len() {
            return TYPE_E_ELEMENTNOTFOUND;
        }
        let mut func = func;
        if func.memid == MEMBERID_NIL {
            func.memid = 0x6000_0000 + d.funcs.len() as i32;
        }
        d.funcs.insert(index as usize, func);
        S_OK
    })
}
unsafe extern "win64" fn cti_add_impl_type(this: usize, index: u32, href: u32) -> i32 {
    edit(this, |d| {
        if index as usize > d.impls.len() || href_index(href) >= d.refs.len() {
            return TYPE_E_ELEMENTNOTFOUND;
        }
        d.impls.insert(index as usize, (href, 0));
        S_OK
    })
}
unsafe extern "win64" fn cti_set_impl_type_flags(this: usize, index: u32, flags: i32) -> i32 {
    edit(this, |d| match d.impls.get_mut(index as usize) {
        Some(i) => {
            i.1 = flags;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    })
}
unsafe extern "win64" fn cti_set_alignment(this: usize, align: u16) -> i32 {
    edit(this, |d| {
        d.align = align;
        S_OK
    })
}
unsafe extern "win64" fn cti_set_schema(_this: usize, _schema: *const u16) -> i32 {
    S_OK
}
unsafe extern "win64" fn cti_add_var_desc(this: usize, index: u32, vd: *const u8) -> i32 {
    if vd.is_null() {
        return E_INVALIDARG;
    }
    let Some(elem) = read_elem(vd.add(24)) else {
        return E_INVALIDARG;
    };
    let kind = *(vd.add(60) as *const u32);
    let mut value = None;
    let mut oinst = 0;
    if kind == VAR_CONST {
        let p = *(vd.add(16) as *const *const u8);
        if p.is_null() {
            return E_INVALIDARG;
        }
        let mut v = [0u8; VARIANT_SIZE];
        super::oleaut32::variant_copy(v.as_mut_ptr().cast(), p.cast());
        value = Some(v);
    } else {
        oinst = *(vd.add(16) as *const u32);
    }
    let var = Var {
        memid: *(vd as *const i32),
        kind,
        elem,
        value,
        oinst,
        flags: *(vd.add(56) as *const u16),
        ..Default::default()
    };
    edit(this, |d| {
        if index as usize > d.vars.len() {
            return TYPE_E_ELEMENTNOTFOUND;
        }
        let mut var = var;
        if var.memid == MEMBERID_NIL {
            var.memid = 0x4000_0000 + d.vars.len() as i32;
        }
        d.vars.insert(index as usize, var);
        S_OK
    })
}
unsafe extern "win64" fn cti_set_func_and_param_names(
    this: usize,
    index: u32,
    names: *const *const u16,
    count: u32,
) -> i32 {
    if names.is_null() || count == 0 {
        return E_INVALIDARG;
    }
    let list: Vec<String> = (0..count as usize).map(|i| wide(*names.add(i))).collect();
    edit(this, |d| match d.funcs.get_mut(index as usize) {
        Some(f) => {
            if list.len() > f.params.len() + 1 {
                return E_INVALIDARG;
            }
            f.names = list;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    })
}
unsafe extern "win64" fn cti_set_var_name(this: usize, index: u32, name: *const u16) -> i32 {
    let name = wide(name);
    edit(this, |d| match d.vars.get_mut(index as usize) {
        Some(v) => {
            v.name = name;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    })
}
unsafe extern "win64" fn cti_set_type_desc_alias(this: usize, td: *const u8) -> i32 {
    let Some(td) = read_tdesc(td, 0) else {
        return E_INVALIDARG;
    };
    edit(this, |d| {
        d.alias = td;
        S_OK
    })
}
unsafe extern "win64" fn cti_define_func_as_dll_entry(
    _this: usize,
    _index: u32,
    _dll: *const u16,
    _proc: *const u16,
) -> i32 {
    TYPE_E_BADMODULEKIND
}
unsafe extern "win64" fn cti_set_func_doc_string(this: usize, index: u32, doc: *const u16) -> i32 {
    let doc = wide(doc);
    edit(this, |d| match d.funcs.get_mut(index as usize) {
        Some(f) => {
            f.doc = doc;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    })
}
unsafe extern "win64" fn cti_set_var_doc_string(this: usize, index: u32, doc: *const u16) -> i32 {
    let doc = wide(doc);
    edit(this, |d| match d.vars.get_mut(index as usize) {
        Some(v) => {
            v.doc = doc;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    })
}
unsafe extern "win64" fn cti_set_func_help_context(this: usize, index: u32, ctx: u32) -> i32 {
    edit(this, |d| match d.funcs.get_mut(index as usize) {
        Some(f) => {
            f.help = ctx;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    })
}
unsafe extern "win64" fn cti_set_var_help_context(this: usize, index: u32, ctx: u32) -> i32 {
    edit(this, |d| match d.vars.get_mut(index as usize) {
        Some(v) => {
            v.help = ctx;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    })
}
unsafe extern "win64" fn cti_set_mops(_this: usize, _index: u32, _mops: *const u16) -> i32 {
    S_OK
}
unsafe extern "win64" fn cti_set_type_idldesc(_this: usize, _idl: *const u8) -> i32 {
    S_OK
}
unsafe extern "win64" fn cti_lay_out(this: usize) -> i32 {
    let info = info_of_create(this);
    let mut d = lock(&*info.lib);
    if entry(&mut d, info.id).is_none() {
        return TYPE_E_ELEMENTNOTFOUND;
    }
    layout(&mut d, info.id);
    S_OK
}
unsafe extern "win64" fn cti_delete_func_desc(this: usize, index: u32) -> i32 {
    edit(this, |d| {
        if index as usize >= d.funcs.len() {
            return TYPE_E_ELEMENTNOTFOUND;
        }
        let f = d.funcs.remove(index as usize);
        for e in f.params {
            clear_default(e);
        }
        S_OK
    })
}
unsafe extern "win64" fn cti_delete_func_desc_by_mem_id(
    this: usize,
    memid: i32,
    invkind: u32,
) -> i32 {
    edit(this, |d| {
        let Some(i) = d
            .funcs
            .iter()
            .position(|f| f.memid == memid && f.invkind == invkind)
        else {
            return TYPE_E_ELEMENTNOTFOUND;
        };
        let f = d.funcs.remove(i);
        for e in f.params {
            clear_default(e);
        }
        S_OK
    })
}
unsafe extern "win64" fn cti_delete_var_desc(this: usize, index: u32) -> i32 {
    edit(this, |d| {
        if index as usize >= d.vars.len() {
            return TYPE_E_ELEMENTNOTFOUND;
        }
        let v = d.vars.remove(index as usize);
        clear_default(v.elem);
        if let Some(mut val) = v.value {
            super::oleaut32::variant_clear(val.as_mut_ptr().cast());
        }
        S_OK
    })
}
unsafe extern "win64" fn cti_delete_var_desc_by_mem_id(this: usize, memid: i32) -> i32 {
    let index = with_info(info_of_create(this), |d| {
        d.vars.iter().position(|v| v.memid == memid)
    })
    .flatten();
    match index {
        Some(i) => cti_delete_var_desc(this, i as u32),
        None => TYPE_E_ELEMENTNOTFOUND,
    }
}
unsafe extern "win64" fn cti_delete_impl_type(this: usize, index: u32) -> i32 {
    edit(this, |d| {
        if index as usize >= d.impls.len() {
            return TYPE_E_ELEMENTNOTFOUND;
        }
        d.impls.remove(index as usize);
        S_OK
    })
}
unsafe extern "win64" fn cti_set_cust_data(_this: usize, guid: *const Guid, var: *const u8) -> i32 {
    if guid.is_null() || var.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}
unsafe extern "win64" fn cti_set_index_cust_data(
    _this: usize,
    _index: u32,
    guid: *const Guid,
    var: *const u8,
) -> i32 {
    if guid.is_null() || var.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}
unsafe extern "win64" fn cti_set_param_cust_data(
    _this: usize,
    _func: u32,
    _param: u32,
    guid: *const Guid,
    var: *const u8,
) -> i32 {
    if guid.is_null() || var.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}
unsafe extern "win64" fn cti_set_help_string_context(_this: usize, _ctx: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn cti_set_index_help_string_context(
    _this: usize,
    _index: u32,
    _ctx: u32,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn cti_invalidate(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn cti_set_name(this: usize, name: *const u16) -> i32 {
    if name.is_null() {
        return E_INVALIDARG;
    }
    let name = wide(name);
    edit(this, |d| {
        d.name = name;
        S_OK
    })
}

// ── ITypeInfo ──────────────────────────────────────────────────────

static TYPEINFO_VTBL: Vtbl<22> = Vtbl([
    ti_query_interface as *const (),
    ti_add_ref as *const (),
    ti_release as *const (),
    ti_get_type_attr as *const (),
    ti_get_type_comp as *const (),
    ti_get_func_desc as *const (),
    ti_get_var_desc as *const (),
    ti_get_names as *const (),
    ti_get_ref_type_of_impl_type as *const (),
    ti_get_impl_type_flags as *const (),
    ti_get_ids_of_names as *const (),
    ti_invoke as *const (),
    ti_get_documentation as *const (),
    ti_get_dll_entry as *const (),
    ti_get_ref_type_info as *const (),
    ti_address_of_member as *const (),
    ti_create_instance as *const (),
    ti_get_mops as *const (),
    ti_get_containing_type_lib as *const (),
    ti_release_block as *const (), // ReleaseTypeAttr
    ti_release_block as *const (), // ReleaseFuncDesc
    ti_release_block as *const (), // ReleaseVarDesc
]);

unsafe fn read(this: usize) -> MutexGuard<'static, LibData> {
    lock(&*info_of_tinfo(this).lib)
}

unsafe extern "win64" fn ti_query_interface(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    info_query(info_of_tinfo(this), iid, out)
}
unsafe extern "win64" fn ti_add_ref(this: usize) -> u32 {
    lib_add_ref(&*info_of_tinfo(this).lib)
}
unsafe extern "win64" fn ti_release(this: usize) -> u32 {
    lib_release(info_of_tinfo(this).lib)
}

unsafe extern "win64" fn ti_get_type_attr(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let id = info_of_tinfo(this).id;
    let d = read(this);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let vft = if e.d.vft != 0 {
        e.d.vft
    } else {
        vft_size(&d, id, 0)
    };
    let i = &e.d;
    let mut o = Out::new();
    let a = o.alloc(96);
    *(a as *mut Guid) = i.guid;
    *(a.add(16) as *mut u32) = d.lcid;
    *(a.add(24) as *mut i32) = MEMBERID_NIL;
    *(a.add(28) as *mut i32) = MEMBERID_NIL;
    let instance = match i.kind {
        TKIND_INTERFACE | TKIND_DISPATCH => 8,
        TKIND_ENUM => 4,
        TKIND_RECORD => i.vars.iter().map(|v| v.oinst + 8).max().unwrap_or(0),
        _ => 0,
    };
    *(a.add(40) as *mut u32) = instance;
    *(a.add(44) as *mut u32) = i.kind;
    *(a.add(48) as *mut u16) = i.funcs.len() as u16;
    *(a.add(50) as *mut u16) = i.vars.len() as u16;
    *(a.add(52) as *mut u16) = i.impls.len() as u16;
    *(a.add(54) as *mut u16) = vft;
    *(a.add(56) as *mut u16) = i.align;
    *(a.add(58) as *mut u16) = i.flags;
    *(a.add(60) as *mut u16) = i.major;
    *(a.add(62) as *mut u16) = i.minor;
    o.tdesc(&i.alias, a.add(64));
    *out = o.finish(a) as usize;
    S_OK
}
unsafe extern "win64" fn ti_get_type_comp(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn ti_get_func_desc(this: usize, index: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let id = info_of_tinfo(this).id;
    let d = read(this);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let Some(f) = e.d.funcs.get(index as usize) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let mut o = Out::new();
    let a = o.alloc(88);
    *(a as *mut i32) = f.memid;
    if !f.scodes.is_empty() {
        let s = o.alloc(4 * f.scodes.len());
        std::ptr::copy_nonoverlapping(f.scodes.as_ptr(), s as *mut i32, f.scodes.len());
        *(a.add(8) as *mut usize) = s as usize;
    }
    if !f.params.is_empty() {
        let p = o.alloc(32 * f.params.len());
        for (k, e) in f.params.iter().enumerate() {
            o.elem(e, p.add(32 * k));
        }
        *(a.add(16) as *mut usize) = p as usize;
    }
    *(a.add(24) as *mut u32) = f.funckind;
    *(a.add(28) as *mut u32) = f.invkind;
    *(a.add(32) as *mut u32) = f.callconv;
    *(a.add(36) as *mut i16) = f.params.len() as i16;
    *(a.add(38) as *mut i16) = f.opt;
    *(a.add(40) as *mut i16) = f.ovft;
    *(a.add(42) as *mut i16) = f.scodes.len() as i16;
    o.elem(&f.ret, a.add(48));
    *(a.add(80) as *mut u16) = f.flags;
    *out = o.finish(a) as usize;
    S_OK
}
unsafe extern "win64" fn ti_get_var_desc(this: usize, index: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let id = info_of_tinfo(this).id;
    let d = read(this);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let Some(v) = e.d.vars.get(index as usize) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let mut o = Out::new();
    let a = o.alloc(64);
    *(a as *mut i32) = v.memid;
    match &v.value {
        Some(val) => {
            let p = o.alloc(VARIANT_SIZE);
            o.variant(val, p);
            *(a.add(16) as *mut usize) = p as usize;
        }
        None => *(a.add(16) as *mut u32) = v.oinst,
    }
    o.elem(&v.elem, a.add(24));
    *(a.add(56) as *mut u16) = v.flags;
    *(a.add(60) as *mut u32) = v.kind;
    *out = o.finish(a) as usize;
    S_OK
}

/// Méthodes appelables (GetIDsOfNames, Invoke) d'un type.
fn methods(i: &InfoData) -> Vec<Method> {
    let dispatchable = i.kind == TKIND_INTERFACE || i.flags & TYPEFLAG_FDUAL != 0;
    let mut out: Vec<Method> = i
        .funcs
        .iter()
        .map(|f| {
            let mut params: Vec<(String, u16)> = f
                .params
                .iter()
                .enumerate()
                .map(|(k, e)| {
                    (
                        f.names.get(k + 1).cloned().unwrap_or_default(),
                        invoke_vt(&e.td),
                    )
                })
                .collect();
            let mut retval = 0;
            if f.params
                .last()
                .is_some_and(|e| e.flags & PARAMFLAG_FRETVAL != 0)
            {
                params.pop();
                let last = &f.params[f.params.len() - 1].td;
                retval = match (&last.vt, &last.inner) {
                    (&VT_PTR, Some(inner)) => invoke_vt(inner),
                    _ => invoke_vt(last),
                };
            }
            let vtable = dispatchable && matches!(f.funckind, FUNC_VIRTUAL | FUNC_PUREVIRTUAL);
            Method {
                name: f.names.first().cloned().unwrap_or_default(),
                params,
                dispid: f.memid,
                slot: (f.ovft.max(0) / 8) as u32,
                cc: f.callconv,
                // Sans table de méthodes (dispinterface pure), Invoke ne
                // trouvera rien à appeler.
                flags: if vtable { f.invkind as u16 } else { 0 },
                vt_return: invoke_vt(&f.ret.td),
                retval,
            }
        })
        .collect();
    // Les variables d'une dispinterface ne sont pas appelables ici.
    out.retain(|m| !m.name.is_empty() || m.flags != 0);
    out
}

/// Type passé à DispCallFunc pour un TYPEDESC : un pointeur devient une
/// référence (VT_BYREF), un type utilisateur (énumération) un entier.
fn invoke_vt(td: &TypeDesc) -> u16 {
    match (td.vt, &td.inner) {
        (VT_PTR, Some(inner)) => match inner.vt {
            VT_PTR | VT_USERDEFINED | VT_VOID | VT_CARRAY => VT_BYREF | 13, // IUnknown*
            vt => VT_BYREF | vt,
        },
        (VT_SAFEARRAY, Some(inner)) => VT_ARRAY | inner.vt,
        (VT_USERDEFINED, _) => VT_I4,
        (VT_INT, _) => VT_I4,
        (VT_UINT, _) => VT_UI4,
        (vt, _) => vt,
    }
}

unsafe extern "win64" fn ti_get_names(
    this: usize,
    memid: i32,
    names: *mut *mut u16,
    max: u32,
    count: *mut u32,
) -> i32 {
    if names.is_null() || count.is_null() {
        return E_INVALIDARG;
    }
    *count = 0;
    let id = info_of_tinfo(this).id;
    let d = read(this);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let list: Vec<String> = if let Some(f) = e.d.funcs.iter().find(|f| f.memid == memid) {
        f.names.clone()
    } else if let Some(v) = e.d.vars.iter().find(|v| v.memid == memid) {
        vec![v.name.clone()]
    } else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    for (k, n) in list.iter().take(max as usize).enumerate() {
        *names.add(k) = bstr(n);
        *count = k as u32 + 1;
    }
    S_OK
}
unsafe extern "win64" fn ti_get_ref_type_of_impl_type(
    this: usize,
    index: u32,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let id = info_of_tinfo(this).id;
    let d = read(this);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    match e.d.impls.get(index as usize) {
        Some(&(href, _)) => {
            *out = href;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    }
}
unsafe extern "win64" fn ti_get_impl_type_flags(this: usize, index: u32, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let id = info_of_tinfo(this).id;
    let d = read(this);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    match e.d.impls.get(index as usize) {
        Some(&(_, flags)) => {
            *out = flags;
            S_OK
        }
        None => TYPE_E_ELEMENTNOTFOUND,
    }
}
unsafe extern "win64" fn ti_get_ids_of_names(
    this: usize,
    names: *const *const u16,
    count: u32,
    ids: *mut i32,
) -> i32 {
    if names.is_null() || ids.is_null() {
        return E_INVALIDARG;
    }
    let id = info_of_tinfo(this).id;
    let list = {
        let d = read(this);
        let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
            return TYPE_E_ELEMENTNOTFOUND;
        };
        let mut all = methods(&e.d);
        // Les noms des fonctions d'une dispinterface restent trouvables.
        for m in all.iter_mut() {
            m.flags = 1;
        }
        all
    };
    super::oleaut32_disp::ids_of_names(&list, names, count, ids)
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn ti_invoke(
    this: usize,
    instance: usize,
    memid: i32,
    flags: u16,
    params: *const u8,
    result: *mut u8,
    excep: *mut u8,
    arg_err: *mut u32,
) -> i32 {
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let id = info_of_tinfo(this).id;
    let list = {
        let d = read(this);
        let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
            return TYPE_E_ELEMENTNOTFOUND;
        };
        methods(&e.d)
    };
    if list.iter().all(|m| m.flags == 0) {
        return DISP_E_MEMBERNOTFOUND;
    }
    super::oleaut32_disp::invoke_method(
        &list, instance, memid, flags, params, result, excep, arg_err,
    )
}
/// GetDocumentation : MEMBERID_NIL pour le type lui-même.
unsafe extern "win64" fn ti_get_documentation(
    this: usize,
    memid: i32,
    name: *mut *mut u16,
    doc: *mut *mut u16,
    help: *mut u32,
    help_file: *mut *mut u16,
) -> i32 {
    let id = info_of_tinfo(this).id;
    let d = read(this);
    let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let (n, s, h) = if memid == MEMBERID_NIL {
        (e.d.name.clone(), e.d.doc.clone(), e.d.help)
    } else if let Some(f) = e.d.funcs.iter().find(|f| f.memid == memid) {
        (
            f.names.first().cloned().unwrap_or_default(),
            f.doc.clone(),
            f.help,
        )
    } else if let Some(v) = e.d.vars.iter().find(|v| v.memid == memid) {
        (v.name.clone(), v.doc.clone(), v.help)
    } else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    put_bstr(name, &n);
    put_bstr(doc, &s);
    if !help.is_null() {
        *help = h;
    }
    put_bstr(help_file, &d.help_file);
    S_OK
}
unsafe extern "win64" fn ti_get_dll_entry(
    _this: usize,
    _memid: i32,
    _invkind: u32,
    dll: *mut *mut u16,
    name: *mut *mut u16,
    ordinal: *mut u16,
) -> i32 {
    for p in [dll, name] {
        if !p.is_null() {
            *p = std::ptr::null_mut();
        }
    }
    if !ordinal.is_null() {
        *ordinal = 0;
    }
    TYPE_E_BADMODULEKIND
}
unsafe extern "win64" fn ti_get_ref_type_info(this: usize, href: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let id = info_of_tinfo(this).id;
    let p = {
        let d = read(this);
        let Some(e) = d.infos.iter().find(|e| e.obj.id == id) else {
            return TYPE_E_ELEMENTNOTFOUND;
        };
        match e.d.refs.get(href_index(href)) {
            Some(&(p, _)) if href >= HREF_BASE => p,
            _ => return TYPE_E_ELEMENTNOTFOUND,
        }
    };
    add_ref(p);
    *out = p;
    S_OK
}
unsafe extern "win64" fn ti_address_of_member(
    _this: usize,
    _memid: i32,
    _invkind: u32,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    TYPE_E_BADMODULEKIND
}
unsafe extern "win64" fn ti_create_instance(
    _this: usize,
    _outer: usize,
    _iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn ti_get_mops(_this: usize, _memid: i32, out: *mut *mut u16) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    S_OK
}
unsafe extern "win64" fn ti_get_containing_type_lib(
    this: usize,
    out: *mut usize,
    index: *mut u32,
) -> i32 {
    let info = info_of_tinfo(this);
    let lib = &*info.lib;
    if !index.is_null() {
        let d = lock(lib);
        match d.infos.iter().position(|e| e.obj.id == info.id) {
            Some(i) => *index = i as u32,
            None => return TYPE_E_ELEMENTNOTFOUND,
        }
    }
    if !out.is_null() {
        lib_add_ref(lib);
        *out = lib as *const LibObj as usize + 8;
    }
    S_OK
}
unsafe extern "win64" fn ti_release_block(_this: usize, p: usize) {
    release_block(p);
}

// ── stdole et bibliothèques inscrites dans le processus ────────────

fn func(name: &str, params: &[(&str, TypeDesc)], ret: u16, memid: i32) -> Func {
    let mut names = vec![name.to_string()];
    names.extend(params.iter().map(|(n, _)| n.to_string()));
    Func {
        memid,
        funckind: FUNC_PUREVIRTUAL,
        invkind: 1,
        callconv: 4, // CC_STDCALL
        params: params
            .iter()
            .map(|(_, td)| Elem {
                td: td.clone(),
                flags: 1, // PARAMFLAG_FIN
                default: None,
            })
            .collect(),
        ret: Elem {
            td: TypeDesc::plain(ret),
            ..Default::default()
        },
        flags: FUNCFLAG_FRESTRICTED,
        names,
        ..Default::default()
    }
}

fn void_ptr(levels: usize) -> TypeDesc {
    let mut td = TypeDesc::plain(VT_VOID);
    for _ in 0..levels {
        td = TypeDesc::ptr(td);
    }
    td
}

/// stdole 2.0 : IUnknown et IDispatch (unknwn.h, oaidl.h).
fn stdole() -> usize {
    static LIB: OnceLock<usize> = OnceLock::new();
    *LIB.get_or_init(|| unsafe {
        let lib = new_lib(LibData {
            name: "stdole".into(),
            guid: LIBID_STDOLE,
            major: 2,
            minor: 0,
            doc: "OLE Automation".into(),
            file: "C:\\Windows\\System32\\stdole2.tlb".into(),
            ..Default::default()
        });
        let l = &*lib;
        let mut d = lock(l);
        let base = 0x6000_0000;
        let unk = push_info(
            l,
            &mut d,
            InfoData {
                name: "IUnknown".into(),
                kind: TKIND_INTERFACE,
                guid: IID_IUNKNOWN,
                flags: TYPEFLAG_FHIDDEN,
                align: 8,
                funcs: vec![
                    func(
                        "QueryInterface",
                        &[("riid", void_ptr(1)), ("ppvObj", void_ptr(2))],
                        VT_HRESULT,
                        base,
                    ),
                    func("AddRef", &[], VT_UI4, base + 1),
                    func("Release", &[], VT_UI4, base + 2),
                ],
                ..Default::default()
            },
        );
        let unk_ptr = tinfo_ptr(d.infos.iter().find(|e| e.obj.id == unk).unwrap());
        let disp_base = 0x6001_0000;
        let disp = push_info(
            l,
            &mut d,
            InfoData {
                name: "IDispatch".into(),
                kind: TKIND_INTERFACE,
                guid: IID_IDISPATCH,
                flags: TYPEFLAG_FDISPATCHABLE,
                align: 8,
                funcs: vec![
                    func(
                        "GetTypeInfoCount",
                        &[("pctinfo", TypeDesc::ptr(TypeDesc::plain(VT_UINT)))],
                        VT_HRESULT,
                        disp_base,
                    ),
                    func(
                        "GetTypeInfo",
                        &[
                            ("itinfo", TypeDesc::plain(VT_UINT)),
                            ("lcid", TypeDesc::plain(VT_UI4)),
                            ("pptinfo", void_ptr(2)),
                        ],
                        VT_HRESULT,
                        disp_base + 1,
                    ),
                    func(
                        "GetIDsOfNames",
                        &[
                            ("riid", void_ptr(1)),
                            ("rgszNames", void_ptr(2)),
                            ("cNames", TypeDesc::plain(VT_UINT)),
                            ("lcid", TypeDesc::plain(VT_UI4)),
                            ("rgdispid", TypeDesc::ptr(TypeDesc::plain(VT_I4))),
                        ],
                        VT_HRESULT,
                        disp_base + 2,
                    ),
                    func(
                        "Invoke",
                        &[
                            ("dispidMember", TypeDesc::plain(VT_I4)),
                            ("riid", void_ptr(1)),
                            ("lcid", TypeDesc::plain(VT_UI4)),
                            ("wFlags", TypeDesc::plain(VT_UI2)),
                            ("pdispparams", void_ptr(1)),
                            ("pvarResult", void_ptr(1)),
                            ("pexcepinfo", void_ptr(1)),
                            ("puArgErr", TypeDesc::ptr(TypeDesc::plain(VT_UINT))),
                        ],
                        VT_HRESULT,
                        disp_base + 3,
                    ),
                ],
                refs: vec![(unk_ptr, false)],
                impls: vec![(HREF_BASE, 0)],
                ..Default::default()
            },
        );
        layout(&mut d, unk);
        layout(&mut d, disp);
        drop(d);
        lib as usize
    })
}

/// Bibliothèques inscrites par RegisterTypeLib (processus seulement).
fn registered() -> &'static Mutex<Vec<(Guid, u16, u16, String, usize)>> {
    static R: Mutex<Vec<(Guid, u16, u16, String, usize)>> = Mutex::new(Vec::new());
    &R
}

fn base_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
}

/// ITypeLib de stdole ou d'une bibliothèque créée dans ce processus, d'après
/// le nom de fichier ; None sinon.
pub unsafe fn load_by_path(path: &str) -> Option<usize> {
    let base = base_name(path);
    if base == "stdole2.tlb" || base == "stdole32.tlb" {
        let lib = stdole();
        lib_add_ref(&*(lib as *const LibObj));
        return Some(lib + 8);
    }
    let r = registered().lock().unwrap_or_else(|e| e.into_inner());
    let &(_, _, _, _, lib) = r
        .iter()
        .find(|(_, _, _, file, _)| file.eq_ignore_ascii_case(path) || base_name(file) == base)?;
    lib_add_ref(&*(lib as *const LibObj));
    Some(lib + 8)
}

/// LoadRegTypeLib : stdole (1.0 ou 2.0) ou une bibliothèque inscrite.
pub unsafe fn load_registered(guid: &Guid, major: u16) -> Option<usize> {
    if *guid == LIBID_STDOLE && (major == 1 || major == 2) {
        let lib = stdole();
        lib_add_ref(&*(lib as *const LibObj));
        return Some(lib + 8);
    }
    let r = registered().lock().unwrap_or_else(|e| e.into_inner());
    let &(_, _, _, _, lib) = r
        .iter()
        .rev()
        .find(|(g, maj, _, _, _)| g == guid && *maj == major)?;
    lib_add_ref(&*(lib as *const LibObj));
    Some(lib + 8)
}

/// Chemin d'une bibliothèque connue (QueryPathOfRegTypeLib).
pub fn registered_path(guid: &Guid, major: u16) -> Option<String> {
    if *guid == LIBID_STDOLE && (major == 1 || major == 2) {
        return Some("C:\\Windows\\System32\\stdole2.tlb".into());
    }
    let r = registered().lock().unwrap_or_else(|e| e.into_inner());
    r.iter()
        .rev()
        .find(|(g, maj, _, _, _)| g == guid && *maj == major)
        .map(|(_, _, _, f, _)| f.clone())
}

/// RegisterTypeLib d'une bibliothèque créée ici : inscrite pour le
/// processus (S_OK). Une autre ITypeLib : None (rien à lire de son format).
pub unsafe fn register(tlib: usize, path: &str) -> Option<i32> {
    if tlib == 0 || *(tlib as *const *const usize) != TYPELIB_VTBL.0.as_ptr().cast() {
        return None;
    }
    let lib = lib_of_tlib(tlib);
    let (guid, major, minor, file) = {
        let d = lock(lib);
        let file = if path.is_empty() {
            d.file.clone()
        } else {
            path.to_string()
        };
        (d.guid, d.major, d.minor, file)
    };
    lib_add_ref(lib);
    registered()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((guid, major, minor, file, lib as *const LibObj as usize));
    Some(S_OK)
}

/// Résultat de LoadTypeLib quand rien n'est trouvé.
pub const NOT_LOADED: i32 = TYPE_E_CANTLOADLIBRARY;
pub const NOT_REGISTERED: i32 = TYPE_E_LIBNOTREGISTERED;

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateTypeLib2" => fn create_type_lib2,
        "CreateTypeLib" => fn create_type_lib,
        "#180" => fn create_type_lib2,
        "#160" => fn create_type_lib,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    type Hr0 = unsafe extern "win64" fn(usize) -> i32;
    type HrP = unsafe extern "win64" fn(usize, usize) -> i32;
    type HrPP = unsafe extern "win64" fn(usize, usize, usize) -> i32;

    unsafe fn slot<F: Copy>(obj: usize, n: usize) -> F {
        let p = vcall(obj, n);
        std::mem::transmute_copy(&p)
    }

    const LIBID_ESSAI: Guid = Guid {
        data1: 0x1108_0001,
        data2: 0x57a1,
        data3: 0x4e11,
        data4: [0x80, 1, 2, 3, 4, 5, 6, 7],
    };
    const IID_ICALCUL: Guid = Guid {
        data1: 0x1108_0002,
        data2: 0x57a1,
        data3: 0x4e11,
        data4: [0x80, 1, 2, 3, 4, 5, 6, 8],
    };

    unsafe extern "win64" fn nul(_: usize) -> i32 {
        0
    }
    unsafe extern "win64" fn ajoute(_this: usize, a: i32, b: i32, r: *mut i32) -> i32 {
        *r = a + b;
        0
    }

    /// Bibliothèque complète : interface double héritant d'IDispatch (stdole),
    /// méthode à [out, retval], disposition, lecture, inscription et Invoke.
    #[test]
    fn bibliotheque_de_types_creee_inscrite_et_appelee() {
        unsafe {
            let file = w("C:\\Essai\\essai1108.tlb");
            let mut ctl = 0usize;
            assert_eq!(create_type_lib2(3, file.as_ptr(), &mut ctl), S_OK);
            assert_eq!(
                slot::<HrP>(ctl, 6)(ctl, &LIBID_ESSAI as *const _ as usize),
                S_OK
            );
            let set_version: unsafe extern "win64" fn(usize, u16, u16) -> i32 = slot(ctl, 5);
            assert_eq!(set_version(ctl, 1, 0), S_OK);
            let nom = w("Essai1108");
            assert_eq!(slot::<HrP>(ctl, 4)(ctl, nom.as_ptr() as usize), S_OK);

            // IDispatch de stdole, par LoadRegTypeLib.
            let mut stdole_tl = 0usize;
            assert_eq!(
                super::super::oleaut32::load_reg_type_lib(
                    &LIBID_STDOLE as *const _ as *const u8,
                    2,
                    0,
                    0,
                    &mut stdole_tl
                ),
                S_OK
            );
            let mut disp_ti = 0usize;
            assert_eq!(
                slot::<HrPP>(stdole_tl, 6)(
                    stdole_tl,
                    &IID_IDISPATCH as *const _ as usize,
                    &mut disp_ti as *mut _ as usize
                ),
                S_OK
            );

            let iname = w("ICalcul");
            let create_ti: unsafe extern "win64" fn(usize, *const u16, u32, *mut usize) -> i32 =
                slot(ctl, 3);
            let mut cti = 0usize;
            assert_eq!(
                create_ti(ctl, iname.as_ptr(), TKIND_INTERFACE, &mut cti),
                S_OK
            );
            assert_eq!(
                slot::<HrP>(cti, 3)(cti, &IID_ICALCUL as *const _ as usize),
                S_OK
            );
            let mut href = 0u32;
            assert_eq!(
                slot::<HrPP>(cti, 8)(cti, disp_ti, &mut href as *mut _ as usize),
                S_OK
            );
            let add_impl: unsafe extern "win64" fn(usize, u32, u32) -> i32 = slot(cti, 10);
            assert_eq!(add_impl(cti, 0, href), S_OK);

            // HRESULT Ajoute([in] long a, [in] long b, [out, retval] long* r)
            let pointe = [0u64, VT_I4 as u64]; // TYPEDESC { lptdesc, vt }
            let mut params = [[0u64; 4]; 3];
            *(params[0].as_mut_ptr().cast::<u8>().add(8) as *mut u16) = VT_I4;
            *(params[1].as_mut_ptr().cast::<u8>().add(8) as *mut u16) = VT_I4;
            *(params[2].as_mut_ptr().cast::<u8>() as *mut usize) = pointe.as_ptr() as usize;
            *(params[2].as_mut_ptr().cast::<u8>().add(8) as *mut u16) = VT_PTR;
            *(params[2].as_mut_ptr().cast::<u8>().add(24) as *mut u16) = 2 | PARAMFLAG_FRETVAL;
            let mut fd = [0u64; 11];
            *(fd.as_mut_ptr().cast::<u8>() as *mut i32) = 1;
            *(fd.as_mut_ptr().cast::<u8>().add(16) as *mut usize) = params.as_ptr() as usize;
            *(fd.as_mut_ptr().cast::<u8>().add(24) as *mut u32) = FUNC_PUREVIRTUAL;
            *(fd.as_mut_ptr().cast::<u8>().add(28) as *mut u32) = 1; // INVOKE_FUNC
            *(fd.as_mut_ptr().cast::<u8>().add(32) as *mut u32) = 4; // CC_STDCALL
            *(fd.as_mut_ptr().cast::<u8>().add(36) as *mut i16) = 3;
            *(fd.as_mut_ptr().cast::<u8>().add(56) as *mut u16) = VT_HRESULT;
            let add_func: unsafe extern "win64" fn(usize, u32, *const u8) -> i32 = slot(cti, 9);
            assert_eq!(add_func(cti, 0, fd.as_ptr().cast()), S_OK);
            let noms = [w("Ajoute"), w("a"), w("b"), w("r")];
            let ptrs: Vec<*const u16> = noms.iter().map(|n| n.as_ptr()).collect();
            let set_names: unsafe extern "win64" fn(usize, u32, *const *const u16, u32) -> i32 =
                slot(cti, 15);
            assert_eq!(set_names(cti, 0, ptrs.as_ptr(), 4), S_OK);
            assert_eq!(slot::<Hr0>(cti, 25)(cti), S_OK); // LayOut

            let mut ti = 0usize;
            assert_eq!(
                slot::<HrPP>(cti, 0)(
                    cti,
                    &IID_ITYPEINFO as *const _ as usize,
                    &mut ti as *mut _ as usize
                ),
                S_OK
            );
            let mut attr = 0usize;
            assert_eq!(slot::<HrP>(ti, 3)(ti, &mut attr as *mut _ as usize), S_OK);
            let a = attr as *const u8;
            assert_eq!(*(a.add(44) as *const u32), TKIND_INTERFACE);
            assert_eq!(*(a.add(48) as *const u16), 1, "cFuncs");
            assert_eq!(*(a.add(52) as *const u16), 1, "cImplTypes");
            assert_eq!(
                *(a.add(54) as *const u16),
                64,
                "cbSizeVft = IDispatch (56) + 8"
            );
            assert_eq!(*(a as *const Guid), IID_ICALCUL);
            slot::<HrP>(ti, 19)(ti, attr);
            let mut pfd = 0usize;
            let get_fd: unsafe extern "win64" fn(usize, u32, *mut usize) -> i32 = slot(ti, 5);
            assert_eq!(get_fd(ti, 0, &mut pfd), S_OK);
            assert_eq!(*((pfd + 40) as *const i16), 56, "oVft après IDispatch");
            assert_eq!(*((pfd + 36) as *const i16), 3);
            slot::<HrP>(ti, 20)(ti, pfd);

            let mut lus = [std::ptr::null_mut::<u16>(); 4];
            let mut n = 0u32;
            let get_names: unsafe extern "win64" fn(
                usize,
                i32,
                *mut *mut u16,
                u32,
                *mut u32,
            ) -> i32 = slot(ti, 7);
            assert_eq!(get_names(ti, 1, lus.as_mut_ptr(), 4, &mut n), S_OK);
            assert_eq!(n, 4);
            assert_eq!(wide(lus[0]), "Ajoute");
            assert_eq!(wide(lus[3]), "r");
            for p in lus {
                super::super::oleaut32::sys_free_string(p);
            }
            let mut id = 0i32;
            let cherche = w("Ajoute");
            let p = [cherche.as_ptr()];
            let ids_of: unsafe extern "win64" fn(usize, *const *const u16, u32, *mut i32) -> i32 =
                slot(ti, 10);
            assert_eq!(ids_of(ti, p.as_ptr(), 1, &mut id), S_OK);
            assert_eq!(id, 1);

            // Invoke sur un objet dont l'entrée 7 (oVft 56) est Ajoute.
            let mut table = [nul as *const () as usize; 8];
            table[7] = ajoute as *const () as usize;
            let objet = [table.as_ptr() as usize];
            let mut args = [[0u64; 3]; 2]; // ordre inverse : b puis a
            args[0][0] = VT_I4 as u64;
            args[0][1] = 5;
            args[1][0] = VT_I4 as u64;
            args[1][1] = 37;
            let mut dp = [0u64; 3];
            *(dp.as_mut_ptr().cast::<u8>() as *mut usize) = args.as_ptr() as usize;
            *(dp.as_mut_ptr().cast::<u8>().add(16) as *mut u32) = 2;
            let mut res = [0u64; 3];
            let invoke: unsafe extern "win64" fn(
                usize,
                usize,
                i32,
                u16,
                *const u8,
                *mut u8,
                *mut u8,
                *mut u32,
            ) -> i32 = slot(ti, 11);
            assert_eq!(
                invoke(
                    ti,
                    objet.as_ptr() as usize,
                    1,
                    1,
                    dp.as_ptr().cast(),
                    res.as_mut_ptr().cast(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut()
                ),
                S_OK
            );
            assert_eq!(res[0] as u16, VT_I4);
            assert_eq!(res[1] as i32, 42);

            // ITypeLib : nombre, recherche par GUID, nom.
            let mut tl = 0usize;
            assert_eq!(
                slot::<HrPP>(ctl, 0)(
                    ctl,
                    &IID_ITYPELIB as *const _ as usize,
                    &mut tl as *mut _ as usize
                ),
                S_OK
            );
            assert_eq!(slot::<Hr0>(tl, 3)(tl), 1);
            let mut t2 = 0usize;
            assert_eq!(
                slot::<HrPP>(tl, 6)(
                    tl,
                    &IID_ICALCUL as *const _ as usize,
                    &mut t2 as *mut _ as usize
                ),
                S_OK
            );
            assert_eq!(t2, ti);
            slot::<Hr0>(t2, 2)(t2);
            let mut doc = std::ptr::null_mut::<u16>();
            let get_doc: unsafe extern "win64" fn(
                usize,
                i32,
                *mut *mut u16,
                usize,
                usize,
                usize,
            ) -> i32 = slot(tl, 9);
            assert_eq!(get_doc(tl, -1, &mut doc, 0, 0, 0), S_OK);
            assert_eq!(wide(doc), "Essai1108");
            super::super::oleaut32::sys_free_string(doc);

            // RegisterTypeLib → QueryPathOfRegTypeLib, LoadRegTypeLib, LoadTypeLib.
            let chemin = w("C:\\Essai\\essai1108.tlb");
            assert_eq!(
                super::super::oleaut32::register_type_lib(tl, chemin.as_ptr(), 0),
                S_OK
            );
            let mut bpath = 0usize;
            assert_eq!(
                super::super::oleaut32::query_path_of_reg_type_lib(
                    &LIBID_ESSAI as *const _ as *const u8,
                    1,
                    0,
                    0,
                    &mut bpath
                ),
                S_OK
            );
            assert_eq!(wide(bpath as *const u16), "C:\\Essai\\essai1108.tlb");
            super::super::oleaut32::sys_free_string(bpath as *mut u16);
            let mut tl2 = 0usize;
            assert_eq!(
                super::super::oleaut32::load_reg_type_lib(
                    &LIBID_ESSAI as *const _ as *const u8,
                    1,
                    0,
                    0,
                    &mut tl2
                ),
                S_OK
            );
            assert_eq!(tl2, tl);
            slot::<Hr0>(tl2, 2)(tl2);
            let mut tl3 = 0usize;
            let court = w("essai1108.tlb");
            assert_eq!(
                super::super::oleaut32::load_type_lib(court.as_ptr(), &mut tl3),
                S_OK
            );
            assert_eq!(tl3, tl);
            slot::<Hr0>(tl3, 2)(tl3);

            let inconnu = w("C:\\Essai\\absente.tlb");
            assert_eq!(
                super::super::oleaut32::load_type_lib(inconnu.as_ptr(), &mut tl3),
                NOT_LOADED
            );
            assert_eq!(tl3, 0);
            let mut st = 0usize;
            let stdole2 = w("stdole2.tlb");
            assert_eq!(
                super::super::oleaut32::load_type_lib(stdole2.as_ptr(), &mut st),
                S_OK
            );
            assert_eq!(st, stdole_tl);
            slot::<Hr0>(st, 2)(st);

            slot::<Hr0>(ti, 2)(ti);
            slot::<Hr0>(cti, 2)(cti);
            slot::<Hr0>(tl, 2)(tl);
            slot::<Hr0>(ctl, 2)(ctl);
            slot::<Hr0>(disp_ti, 2)(disp_ti);
            slot::<Hr0>(stdole_tl, 2)(stdole_tl);
        }
    }
}
