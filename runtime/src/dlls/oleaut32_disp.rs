//! OLE Automation (#16) : table des objets actifs, DispCallFunc, et
//! IDispatch standard construit d'une description d'interface.
//!
//! Écrit d'après la documentation publique Microsoft Learn :
//! `RegisterActiveObject`, `RevokeActiveObject`, `GetActiveObject`,
//! `DispCallFunc`, `CreateDispTypeInfo` (INTERFACEDATA, METHODDATA,
//! PARAMDATA), `CreateStdDispatch`, `DispGetIDsOfNames`, `DispInvoke`,
//! `DispGetParam`, et l'ordre des méthodes d'ITypeInfo et d'IDispatch
//! dans oaidl.h.
//!
//! Disposition x64 : VARIANT 24 octets (`vt` à +0, valeur à +8),
//! DISPPARAMS { rgvarg +0, rgdispidNamedArgs +8, cArgs +16, cNamedArgs +20 },
//! METHODDATA 40 octets, PARAMDATA 16 octets. La table des objets actifs
//! est propre au processus : un objet inscrit par un autre processus n'y
//! est pas visible (serveurs hors processus : suite de #16).

use super::ole32::Guid;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;
const MK_E_UNAVAILABLE: i32 = 0x8004_01e3u32 as i32;
const DISP_E_UNKNOWNNAME: i32 = 0x8002_0006u32 as i32;
const DISP_E_MEMBERNOTFOUND: i32 = 0x8002_0003u32 as i32;
const DISP_E_PARAMNOTFOUND: i32 = 0x8002_0004u32 as i32;
const DISP_E_TYPEMISMATCH: i32 = 0x8002_0005u32 as i32;
const DISP_E_EXCEPTION: i32 = 0x8002_0009u32 as i32;
const DISP_E_BADPARAMCOUNT: i32 = 0x8002_000eu32 as i32;
const DISP_E_BADVARTYPE: i32 = 0x8002_0008u32 as i32;
const TYPE_E_ELEMENTNOTFOUND: i32 = 0x8002_802bu32 as i32;

const VT_EMPTY: u16 = 0;
const VT_R4: u16 = 4;
const VT_R8: u16 = 5;
const VT_DECIMAL: u16 = 14;
const VT_VARIANT: u16 = 12;
const VT_VOID: u16 = 24;
const VT_HRESULT: u16 = 25;
const VT_BYREF: u16 = 0x4000;

const DISPID_UNKNOWN: i32 = -1;
const DISPID_PROPERTYPUT: i32 = -3;

/// Taille d'un VARIANT en 64 bits.
const VARIANT_SIZE: usize = 24;

const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const IID_IDISPATCH: Guid = Guid {
    data1: 0x0002_0400,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const IID_ITYPEINFO: Guid = Guid {
    data1: 0x0002_0401,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};

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

unsafe fn wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| *p.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

// ── Table des objets actifs ────────────────────────────────────────

struct Active {
    cookie: u32,
    clsid: Guid,
    unk: usize,
    strong: bool,
}
static ACTIVE: Mutex<Vec<Active>> = Mutex::new(Vec::new());
static NEXT_ACTIVE: AtomicU32 = AtomicU32::new(1);

/// RegisterActiveObject : ACTIVEOBJECT_STRONG (0) garde une référence,
/// ACTIVEOBJECT_WEAK (1) non.
pub unsafe extern "win64" fn register_active_object(
    unk: usize,
    clsid: *const Guid,
    flags: u32,
    cookie: *mut u32,
) -> i32 {
    if cookie.is_null() {
        return E_POINTER;
    }
    *cookie = 0;
    if unk == 0 || clsid.is_null() || flags > 1 {
        return E_INVALIDARG;
    }
    let strong = flags == 0;
    if strong {
        add_ref(unk);
    }
    let c = NEXT_ACTIVE.fetch_add(1, Ordering::Relaxed);
    ACTIVE.lock().unwrap().push(Active {
        cookie: c,
        clsid: *clsid,
        unk,
        strong,
    });
    *cookie = c;
    S_OK
}

pub unsafe extern "win64" fn revoke_active_object(cookie: u32, _reserved: usize) -> i32 {
    let mut table = ACTIVE.lock().unwrap();
    let Some(i) = table.iter().position(|a| a.cookie == cookie) else {
        return E_INVALIDARG;
    };
    let entry = table.remove(i);
    drop(table);
    if entry.strong {
        release(entry.unk);
    }
    S_OK
}

/// GetActiveObject : le dernier objet inscrit pour cette classe.
pub unsafe extern "win64" fn get_active_object(
    clsid: *const Guid,
    _reserved: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if clsid.is_null() {
        return E_INVALIDARG;
    }
    let table = ACTIVE.lock().unwrap();
    let Some(entry) = table.iter().rev().find(|a| a.clsid == *clsid) else {
        return MK_E_UNAVAILABLE;
    };
    let unk = entry.unk;
    add_ref(unk);
    drop(table);
    *out = unk;
    S_OK
}

// ── DispCallFunc ───────────────────────────────────────────────────

/// Valeur passée pour un argument VARIANT : la valeur elle-même (8 octets
/// à +8), ou l'adresse du VARIANT pour VT_VARIANT et VT_DECIMAL.
unsafe fn argument_slot(vt: u16, var: *const u8) -> u64 {
    if vt == VT_VARIANT || vt == VT_DECIMAL {
        return var as u64;
    }
    let raw = *(var.add(8) as *const u64);
    match vt {
        VT_R4 => raw & 0xffff_ffff,
        _ => raw,
    }
}

/// DispCallFunc : appelle la méthode `vtable_offset` de `instance` (ou la
/// fonction d'adresse `vtable_offset` si `instance` est nul) avec les
/// arguments VARIANT donnés. En x64 il n'y a qu'une convention d'appel.
pub unsafe extern "win64" fn disp_call_func(
    instance: usize,
    vtable_offset: usize,
    _cc: u32,
    vt_return: u16,
    count: u32,
    types: *const u16,
    args: *const *const u8,
    result: *mut u8,
) -> i32 {
    if count > 0 && (types.is_null() || args.is_null()) {
        return E_INVALIDARG;
    }
    // Dispositions x86 (VARIANT de 16 octets…) non traduites ici.
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let target = if instance != 0 {
        if !vtable_offset.is_multiple_of(8) {
            return E_INVALIDARG;
        }
        *(*(instance as *const *const usize)).add(vtable_offset / 8)
    } else {
        vtable_offset
    };
    if target == 0 {
        return E_INVALIDARG;
    }
    let mut slots = Vec::with_capacity(count as usize + 1);
    if instance != 0 {
        slots.push(instance as u64);
    }
    for i in 0..count as usize {
        let var = *args.add(i);
        if var.is_null() {
            return E_INVALIDARG;
        }
        slots.push(argument_slot(*types.add(i), var));
    }
    let raw = crate::clr::pinvoke::call_win64(target, &slots);
    if !result.is_null() {
        std::ptr::write_bytes(result, 0, VARIANT_SIZE);
        match vt_return {
            VT_EMPTY | VT_VOID => {}
            VT_R4 => {
                *(result as *mut u16) = VT_R4;
                *(result.add(8) as *mut u32) = raw.float as u32;
            }
            VT_R8 | 7 => {
                *(result as *mut u16) = vt_return;
                *(result.add(8) as *mut u64) = raw.float;
            }
            vt if vt == VT_DECIMAL || vt == VT_VARIANT => return DISP_E_BADVARTYPE,
            vt => {
                *(result as *mut u16) = vt;
                *(result.add(8) as *mut u64) = raw.int;
            }
        }
    }
    S_OK
}

// ── ITypeInfo d'une description INTERFACEDATA ──────────────────────

/// Méthode décrite pour GetIDsOfNames / Invoke (aussi construite par les
/// ITypeInfo de CreateTypeLib2, oleaut32_typelib.rs).
pub(super) struct Method {
    pub name: String,
    pub params: Vec<(String, u16)>,
    pub dispid: i32,
    pub slot: u32,
    pub cc: u32,
    pub flags: u16,
    pub vt_return: u16,
    /// Paramètre [out, retval] final (type pointé), 0 s'il n'y en a pas : il
    /// n'est pas dans DISPPARAMS, sa valeur devient le résultat d'Invoke.
    pub retval: u16,
}

#[repr(C)]
struct TypeInfo {
    vtbl: *const usize,
    refs: AtomicU32,
    methods: Vec<Method>,
}

/// Table de méthodes statique (pointeurs de fonctions, partagée).
struct Vtbl<const N: usize>([*const (); N]);
unsafe impl<const N: usize> Sync for Vtbl<N> {}

const TYPEINFO_SLOTS: usize = 22;
static TYPEINFO_VTBL: Vtbl<TYPEINFO_SLOTS> = Vtbl([
    ti_query_interface as *const (),
    ti_add_ref as *const (),
    ti_release as *const (),
    ti_not_impl_1 as *const (), // GetTypeAttr
    ti_not_impl_1 as *const (), // GetTypeComp
    ti_not_impl_2 as *const (), // GetFuncDesc
    ti_not_impl_2 as *const (), // GetVarDesc
    ti_get_names as *const (),
    ti_not_impl_2 as *const (), // GetRefTypeOfImplType
    ti_not_impl_2 as *const (), // GetImplTypeFlags
    ti_get_ids_of_names as *const (),
    ti_invoke as *const (),
    ti_not_impl_6 as *const (), // GetDocumentation
    ti_not_impl_6 as *const (), // GetDllEntry
    ti_not_impl_2 as *const (), // GetRefTypeInfo
    ti_not_impl_3 as *const (), // AddressOfMember
    ti_not_impl_3 as *const (), // CreateInstance
    ti_not_impl_2 as *const (), // GetMops
    ti_not_impl_2 as *const (), // GetContainingTypeLib
    ti_void_1 as *const (),     // ReleaseTypeAttr
    ti_void_1 as *const (),     // ReleaseFuncDesc
    ti_void_1 as *const (),     // ReleaseVarDesc
]);

unsafe extern "win64" fn ti_query_interface(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if !iid.is_null() && (*iid == IID_IUNKNOWN || *iid == IID_ITYPEINFO) {
        ti_add_ref(this);
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn ti_add_ref(this: usize) -> u32 {
    (*(this as *const TypeInfo))
        .refs
        .fetch_add(1, Ordering::SeqCst)
        + 1
}
unsafe extern "win64" fn ti_release(this: usize) -> u32 {
    let left = (*(this as *const TypeInfo))
        .refs
        .fetch_sub(1, Ordering::SeqCst)
        - 1;
    if left == 0 {
        drop(Box::from_raw(this as *mut TypeInfo));
    }
    left
}
unsafe extern "win64" fn ti_not_impl_1(_: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn ti_not_impl_2(_: usize, _: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
unsafe extern "win64" fn ti_not_impl_3(_: usize, _: usize, _: usize, _: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn ti_not_impl_6(
    _: usize,
    _: usize,
    _: usize,
    _: usize,
    _: usize,
    _: usize,
) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn ti_void_1(_: usize, _: usize) {}

/// ITypeInfo::GetNames : nom de la méthode puis de ses paramètres.
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
    let info = &*(this as *const TypeInfo);
    names_of(&info.methods, memid, names, max, count)
}

/// GetNames sur une liste de méthodes.
pub(super) unsafe fn names_of(
    methods: &[Method],
    memid: i32,
    names: *mut *mut u16,
    max: u32,
    count: *mut u32,
) -> i32 {
    let Some(m) = methods.iter().find(|m| m.dispid == memid) else {
        return TYPE_E_ELEMENTNOTFOUND;
    };
    let all = std::iter::once(&m.name).chain(m.params.iter().map(|(n, _)| n));
    for (i, name) in all.take(max as usize).enumerate() {
        let w: Vec<u16> = name.encode_utf16().collect();
        *names.add(i) = super::oleaut32::sys_alloc_string_len(w.as_ptr(), w.len() as u32);
        *count = i as u32 + 1;
    }
    S_OK
}

/// ITypeInfo::GetIDsOfNames : le premier nom est le membre, les suivants
/// ses paramètres (leur rang). Comparaison sans casse, comme OLE.
unsafe extern "win64" fn ti_get_ids_of_names(
    this: usize,
    names: *const *const u16,
    count: u32,
    ids: *mut i32,
) -> i32 {
    if names.is_null() || ids.is_null() {
        return E_INVALIDARG;
    }
    let info = &*(this as *const TypeInfo);
    ids_of_names(&info.methods, names, count, ids)
}

/// GetIDsOfNames sur une liste de méthodes.
pub(super) unsafe fn ids_of_names(
    methods: &[Method],
    names: *const *const u16,
    count: u32,
    ids: *mut i32,
) -> i32 {
    if count == 0 {
        return S_OK;
    }
    for i in 0..count as usize {
        *ids.add(i) = DISPID_UNKNOWN;
    }
    let member = wide(*names).to_lowercase();
    let Some(m) = methods.iter().find(|m| m.name.to_lowercase() == member) else {
        return DISP_E_UNKNOWNNAME;
    };
    *ids = m.dispid;
    let mut result = S_OK;
    for i in 1..count as usize {
        let name = wide(*names.add(i)).to_lowercase();
        // Le même nom de propriété peut exister en get et en put.
        let found = methods
            .iter()
            .filter(|o| o.dispid == m.dispid)
            .find_map(|o| o.params.iter().position(|(n, _)| n.to_lowercase() == name));
        match found {
            Some(p) => *ids.add(i) = p as i32,
            None => result = DISP_E_UNKNOWNNAME,
        }
    }
    result
}

/// Rang de paramètre (0 = premier) → VARIANT de DISPPARAMS, ou None.
unsafe fn argument_for(params: *const u8, rank: usize, total: usize) -> Option<usize> {
    let rgvarg = *(params as *const usize);
    let named = *(params.add(8) as *const *const i32);
    let c_args = *(params.add(16) as *const u32) as usize;
    let c_named = *(params.add(20) as *const u32) as usize;
    let positional = c_args - c_named;
    if rank < positional {
        return Some(rgvarg + (c_args - 1 - rank) * VARIANT_SIZE);
    }
    for j in 0..c_named {
        let id = *named.add(j);
        let wanted = if id == DISPID_PROPERTYPUT {
            total.wrapping_sub(1)
        } else {
            id as usize
        };
        if wanted == rank {
            return Some(rgvarg + j * VARIANT_SIZE);
        }
    }
    None
}

/// ITypeInfo::Invoke : convertit les arguments au type déclaré et appelle
/// la méthode `iMeth` de l'objet par DispCallFunc.
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
    let info = &*(this as *const TypeInfo);
    invoke_method(
        &info.methods,
        instance,
        memid,
        flags,
        params,
        result,
        excep,
        arg_err,
    )
}

/// Invoke sur une liste de méthodes : arguments convertis au type déclaré,
/// appel par DispCallFunc à l'entrée `slot` de la table de l'objet.
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn invoke_method(
    methods: &[Method],
    instance: usize,
    memid: i32,
    flags: u16,
    params: *const u8,
    result: *mut u8,
    excep: *mut u8,
    arg_err: *mut u32,
) -> i32 {
    if instance == 0 || params.is_null() {
        return E_INVALIDARG;
    }
    let Some(m) = methods
        .iter()
        .find(|m| m.dispid == memid && m.flags & flags != 0)
    else {
        return DISP_E_MEMBERNOTFOUND;
    };
    let c_args = *(params.add(16) as *const u32) as usize;
    let c_named = *(params.add(20) as *const u32) as usize;
    if c_named > c_args {
        return E_INVALIDARG;
    }
    if c_args != m.params.len() {
        return DISP_E_BADPARAMCOUNT;
    }
    let total = m.params.len();
    let mut temps = vec![[0u8; VARIANT_SIZE]; total];
    let mut types = Vec::with_capacity(total);
    let cleanup = |temps: &mut Vec<[u8; VARIANT_SIZE]>| {
        for t in temps.iter_mut() {
            super::oleaut32::variant_clear(t.as_mut_ptr().cast());
        }
    };
    for (rank, (_, vt)) in m.params.iter().enumerate() {
        let Some(source) = argument_for(params, rank, total) else {
            cleanup(&mut temps);
            return DISP_E_PARAMNOTFOUND;
        };
        let source = source as *const u8;
        let src_vt = *(source as *const u16);
        let hr = if *vt == VT_VARIANT {
            super::oleaut32::variant_copy(temps[rank].as_mut_ptr().cast(), source.cast())
        } else if vt & VT_BYREF != 0 {
            // Une référence doit arriver du bon type ; elle est passée telle
            // quelle et reste à l'appelant (jamais libérée ici).
            if src_vt == *vt {
                std::ptr::copy_nonoverlapping(source, temps[rank].as_mut_ptr(), 16);
                types.push(*vt);
                continue;
            }
            DISP_E_TYPEMISMATCH
        } else {
            super::oleaut32_var::change_type(
                temps[rank].as_mut_ptr().cast(),
                source.cast(),
                0x400,
                0,
                *vt,
            )
        };
        if hr < 0 {
            if !arg_err.is_null() {
                // Rang dans rgvarg (ordre inverse).
                let index = (source as usize - *(params as *const usize)) / VARIANT_SIZE;
                *arg_err = index as u32;
            }
            cleanup(&mut temps);
            return if hr == DISP_E_TYPEMISMATCH || hr == DISP_E_BADVARTYPE {
                DISP_E_TYPEMISMATCH
            } else {
                hr
            };
        }
        types.push(*vt);
    }
    // [out, retval] : référence vers `ret`, qui devient le résultat.
    let mut ret = [0u8; VARIANT_SIZE];
    if m.retval != 0 {
        let target = if m.retval == VT_VARIANT {
            ret.as_mut_ptr()
        } else {
            ret.as_mut_ptr().add(8)
        };
        let mut t = [0u8; VARIANT_SIZE];
        *(t.as_mut_ptr() as *mut u16) = VT_BYREF | m.retval;
        *(t.as_mut_ptr().add(8) as *mut usize) = target as usize;
        temps.push(t);
        types.push(VT_BYREF | m.retval);
    }
    let pointers: Vec<*const u8> = temps.iter().map(|t| t.as_ptr()).collect();
    let mut out = [0u8; VARIANT_SIZE];
    let hr = disp_call_func(
        instance,
        m.slot as usize * 8,
        m.cc,
        m.vt_return,
        types.len() as u32,
        types.as_ptr(),
        pointers.as_ptr(),
        out.as_mut_ptr(),
    );
    // Les références (VT_BYREF) appartiennent à l'appelant.
    for (t, vt) in temps.iter_mut().zip(&types) {
        if vt & VT_BYREF != 0 {
            *t = [0; VARIANT_SIZE];
        }
    }
    cleanup(&mut temps);
    if hr < 0 {
        return hr;
    }
    if m.vt_return == VT_HRESULT {
        let code = *(out.as_ptr().add(8) as *const i32);
        if code < 0 {
            if !excep.is_null() {
                // EXCEPINFO x64 : 64 octets, scode à +56.
                std::ptr::write_bytes(excep, 0, 64);
                *(excep.add(56) as *mut i32) = code;
            }
            return DISP_E_EXCEPTION;
        }
        if !result.is_null() {
            std::ptr::write_bytes(result, 0, VARIANT_SIZE);
            write_retval(m.retval, &ret, result);
        } else if m.retval != 0 {
            super::oleaut32::variant_clear(retval_variant(m.retval, &ret).as_mut_ptr().cast());
        }
        return S_OK;
    }
    if !result.is_null() {
        std::ptr::copy_nonoverlapping(out.as_ptr(), result, VARIANT_SIZE);
    }
    S_OK
}

/// VARIANT complet d'une valeur [out, retval] reçue dans `ret`.
fn retval_variant(vt: u16, ret: &[u8; VARIANT_SIZE]) -> [u8; VARIANT_SIZE] {
    let mut v = *ret;
    if vt != VT_VARIANT {
        v[..2].copy_from_slice(&vt.to_le_bytes());
    }
    v
}

unsafe fn write_retval(vt: u16, ret: &[u8; VARIANT_SIZE], result: *mut u8) {
    if vt == 0 {
        return;
    }
    let v = retval_variant(vt, ret);
    std::ptr::copy_nonoverlapping(v.as_ptr(), result, VARIANT_SIZE);
}

/// CreateDispTypeInfo : ITypeInfo d'une INTERFACEDATA.
pub unsafe extern "win64" fn create_disp_type_info(
    data: *const u8,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if data.is_null() {
        return E_INVALIDARG;
    }
    // Dispositions x86 (VARIANT de 16 octets…) non traduites ici.
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let methods_ptr = *(data as *const *const u8);
    let count = *(data.add(8) as *const u32) as usize;
    if count > 0 && methods_ptr.is_null() {
        return E_INVALIDARG;
    }
    let mut methods = Vec::with_capacity(count);
    for i in 0..count {
        let md = methods_ptr.add(i * 40);
        let pdata = *(md.add(8) as *const *const u8);
        let c_args = *(md.add(28) as *const u32) as usize;
        if c_args > 0 && pdata.is_null() {
            return E_INVALIDARG;
        }
        let params = (0..c_args)
            .map(|p| {
                let pd = pdata.add(p * 16);
                (wide(*(pd as *const *const u16)), *(pd.add(8) as *const u16))
            })
            .collect();
        methods.push(Method {
            name: wide(*(md as *const *const u16)),
            params,
            dispid: *(md.add(16) as *const i32),
            slot: *(md.add(20) as *const u32),
            cc: *(md.add(24) as *const u32),
            flags: *(md.add(32) as *const u16),
            vt_return: *(md.add(34) as *const u16),
            retval: 0,
        });
    }
    let info = Box::new(TypeInfo {
        vtbl: TYPEINFO_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        methods,
    });
    *out = Box::into_raw(info) as usize;
    S_OK
}

// ── IDispatch standard ─────────────────────────────────────────────

/// Objet de CreateStdDispatch : IUnknown non délégant (rendu à
/// l'appelant) et IDispatch dont l'IUnknown délègue à l'extérieur.
#[repr(C)]
struct StdDispatch {
    dispatch: *const usize,
    inner: *const usize,
    refs: AtomicU32,
    outer: usize,
    this: usize,
    tinfo: usize,
}

static DISPATCH_VTBL: Vtbl<7> = Vtbl([
    sd_query_interface as *const (),
    sd_add_ref as *const (),
    sd_release as *const (),
    sd_get_type_info_count as *const (),
    sd_get_type_info as *const (),
    sd_get_ids_of_names as *const (),
    sd_invoke as *const (),
]);
static INNER_VTBL: Vtbl<3> = Vtbl([
    in_query_interface as *const (),
    in_add_ref as *const (),
    in_release as *const (),
]);

unsafe fn from_dispatch(p: usize) -> &'static StdDispatch {
    &*(p as *const StdDispatch)
}
unsafe fn from_inner(p: usize) -> &'static StdDispatch {
    &*((p - 8) as *const StdDispatch)
}

unsafe extern "win64" fn in_query_interface(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let sd = from_inner(this);
    if *iid == IID_IUNKNOWN {
        in_add_ref(this);
        *out = this;
    } else if *iid == IID_IDISPATCH {
        let disp = sd as *const StdDispatch as usize;
        sd_add_ref(disp);
        *out = disp;
    } else {
        return E_NOINTERFACE;
    }
    S_OK
}
unsafe extern "win64" fn in_add_ref(this: usize) -> u32 {
    from_inner(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn in_release(this: usize) -> u32 {
    let sd = from_inner(this);
    let left = sd.refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if left == 0 {
        let tinfo = sd.tinfo;
        drop(Box::from_raw((this - 8) as *mut StdDispatch));
        release(tinfo);
    }
    left
}

unsafe extern "win64" fn sd_query_interface(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    let sd = from_dispatch(this);
    if sd.outer != 0 {
        let f: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
            std::mem::transmute(vcall(sd.outer, 0));
        return f(sd.outer, iid, out);
    }
    in_query_interface(this + 8, iid, out)
}
unsafe extern "win64" fn sd_add_ref(this: usize) -> u32 {
    let sd = from_dispatch(this);
    if sd.outer != 0 {
        return add_ref(sd.outer);
    }
    in_add_ref(this + 8)
}
unsafe extern "win64" fn sd_release(this: usize) -> u32 {
    let sd = from_dispatch(this);
    if sd.outer != 0 {
        return release(sd.outer);
    }
    in_release(this + 8)
}
unsafe extern "win64" fn sd_get_type_info_count(_this: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_INVALIDARG;
    }
    *count = 1;
    S_OK
}
unsafe extern "win64" fn sd_get_type_info(
    this: usize,
    index: u32,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    if index != 0 {
        return 0x8002_000bu32 as i32; // DISP_E_BADINDEX
    }
    let tinfo = from_dispatch(this).tinfo;
    add_ref(tinfo);
    *out = tinfo;
    S_OK
}
unsafe extern "win64" fn sd_get_ids_of_names(
    this: usize,
    _riid: *const Guid,
    names: *const *const u16,
    count: u32,
    _lcid: u32,
    ids: *mut i32,
) -> i32 {
    disp_get_ids_of_names(from_dispatch(this).tinfo, names, count, ids)
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn sd_invoke(
    this: usize,
    dispid: i32,
    _riid: *const Guid,
    _lcid: u32,
    flags: u16,
    params: *const u8,
    result: *mut u8,
    excep: *mut u8,
    arg_err: *mut u32,
) -> i32 {
    let sd = from_dispatch(this);
    disp_invoke(
        sd.this, sd.tinfo, dispid, flags, params, result, excep, arg_err,
    )
}

/// CreateStdDispatch : rend l'IUnknown non délégant de l'objet.
pub unsafe extern "win64" fn create_std_dispatch(
    outer: usize,
    this: usize,
    tinfo: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    if this == 0 || tinfo == 0 {
        return E_INVALIDARG;
    }
    add_ref(tinfo);
    let sd = Box::new(StdDispatch {
        dispatch: DISPATCH_VTBL.0.as_ptr().cast(),
        inner: INNER_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        outer,
        this,
        tinfo,
    });
    let raw = Box::into_raw(sd);
    if raw.is_null() {
        return E_OUTOFMEMORY;
    }
    *out = raw as usize + 8;
    S_OK
}

/// DispGetIDsOfNames : ITypeInfo::GetIDsOfNames.
pub unsafe extern "win64" fn disp_get_ids_of_names(
    tinfo: usize,
    names: *const *const u16,
    count: u32,
    ids: *mut i32,
) -> i32 {
    if tinfo == 0 {
        return E_INVALIDARG;
    }
    let f: unsafe extern "win64" fn(usize, *const *const u16, u32, *mut i32) -> i32 =
        std::mem::transmute(vcall(tinfo, 10));
    f(tinfo, names, count, ids)
}

/// DispInvoke : ITypeInfo::Invoke sur `this`.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn disp_invoke(
    this: usize,
    tinfo: usize,
    dispid: i32,
    flags: u16,
    params: *const u8,
    result: *mut u8,
    excep: *mut u8,
    arg_err: *mut u32,
) -> i32 {
    if tinfo == 0 {
        return E_INVALIDARG;
    }
    type Invoke = unsafe extern "win64" fn(
        usize,
        usize,
        i32,
        u16,
        *const u8,
        *mut u8,
        *mut u8,
        *mut u32,
    ) -> i32;
    let f: Invoke = std::mem::transmute(vcall(tinfo, 11));
    f(tinfo, this, dispid, flags, params, result, excep, arg_err)
}

/// DispGetParam : argument de rang `position` converti au type demandé.
pub unsafe extern "win64" fn disp_get_param(
    params: *const u8,
    position: u32,
    vt: u16,
    result: *mut u8,
    arg_err: *mut u32,
) -> i32 {
    if params.is_null() || result.is_null() {
        return E_INVALIDARG;
    }
    // Dispositions x86 (VARIANT de 16 octets…) non traduites ici.
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let c_args = *(params.add(16) as *const u32) as usize;
    let Some(source) = argument_for(params, position as usize, c_args) else {
        return DISP_E_PARAMNOTFOUND;
    };
    let hr = super::oleaut32_var::change_type(result.cast(), source as *const c_void, 0x400, 0, vt);
    if hr < 0 && !arg_err.is_null() {
        *arg_err = ((source - *(params as *const usize)) / VARIANT_SIZE) as u32;
    }
    if hr == DISP_E_BADVARTYPE {
        return DISP_E_TYPEMISMATCH;
    }
    hr
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "RegisterActiveObject" => fn register_active_object,
        "RevokeActiveObject" => fn revoke_active_object,
        "GetActiveObject" => fn get_active_object,
        "DispCallFunc" => fn disp_call_func,
        "CreateDispTypeInfo" => fn create_disp_type_info,
        "CreateStdDispatch" => fn create_std_dispatch,
        "DispGetIDsOfNames" => fn disp_get_ids_of_names,
        "DispInvoke" => fn disp_invoke,
        "DispGetParam" => fn disp_get_param,
        "#35" => fn get_active_object,
    }
}
