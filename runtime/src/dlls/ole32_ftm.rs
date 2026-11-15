//! Marshaleur libre (free-threaded marshaler) de
//! `CoCreateFreeThreadedMarshaler` (#2674).
//!
//! Microsoft Learn, CoCreateFreeThreadedMarshaler : la fonction crée un
//! marshaleur agrégeable, l'agrège à `punkOuter` et rend dans
//! `ppunkMarshal` son IUnknown ; l'objet qui l'agrège lui délègue les
//! QueryInterface de IID_IMarshal. Pour MSHCTX_INPROC, le marshaleur
//! « copies the interface pointer into the marshaling stream » ; pour les
//! autres contextes, il délègue au marshaleur standard.
//!
//! Winlux n'a qu'un processus par programme et un seul format de bloc de
//! marshaling (`ole32_lot_c`), qui transporte déjà le pointeur dans le
//! même processus : les méthodes d'IMarshal passent par
//! `CoGetMarshalSizeMax`, `CoMarshalInterface`, `CoUnmarshalInterface` et
//! `CoReleaseMarshalData`.
//!
//! Valeurs prises dans winapi-rs (MIT/Apache-2.0, `src/um/objidlbase.rs`
//! et `src/um/cguid.rs`) : IID_IMarshal {00000003-0000-0000-C000-000000000046},
//! CLSID_InProcFreeMarshaler {0000033A-0000-0000-C000-000000000046}, et
//! l'ordre de la vtable d'IMarshal (GetUnmarshalClass, GetMarshalSizeMax,
//! MarshalInterface, UnmarshalInterface, ReleaseMarshalData,
//! DisconnectObject).

use super::comobj::{guid, Guid, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK};
use std::sync::atomic::{AtomicU32, Ordering};

const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;

pub const IID_IMARSHAL: Guid = guid(0x0000_0003, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const CLSID_IN_PROC_FREE_MARSHALER: Guid =
    guid(0x0000_033A, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// Objet agrégeable : IUnknown non délégant en tête, IMarshal ensuite.
#[repr(C)]
struct Ftm {
    inner: *const usize,
    marshal: *const usize,
    refs: AtomicU32,
    /// IUnknown de contrôle (punkOuter), ou 0 sans agrégation.
    outer: usize,
}

unsafe fn of_marshal(this: usize) -> *mut Ftm {
    (this - std::mem::size_of::<usize>()) as *mut Ftm
}

// ---------- IUnknown non délégant ----------

unsafe extern "win64" fn inner_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(g) = super::ole32_lot_c::read_guid(iid) else {
        return E_POINTER;
    };
    let ftm = this as *mut Ftm;
    if g == IID_IUNKNOWN {
        inner_add_ref(this);
        *out = this;
        S_OK
    } else if g == IID_IMARSHAL {
        let m = std::ptr::addr_of!((*ftm).marshal) as usize;
        marshal_add_ref(m);
        *out = m;
        S_OK
    } else {
        E_NOINTERFACE
    }
}

unsafe extern "win64" fn inner_add_ref(this: usize) -> u32 {
    (*(this as *mut Ftm)).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "win64" fn inner_release(this: usize) -> u32 {
    let ftm = this as *mut Ftm;
    let n = (*ftm).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(ftm));
    }
    n
}

// ---------- IMarshal (IUnknown délégué à l'objet qui agrège) ----------

/// IUnknown auquel IMarshal délègue : punkOuter, sinon l'IUnknown interne.
unsafe fn controlling(this: usize) -> usize {
    let ftm = of_marshal(this);
    if (*ftm).outer != 0 {
        (*ftm).outer
    } else {
        ftm as usize
    }
}

unsafe extern "win64" fn marshal_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    super::ole32_lot_c::query(controlling(this), iid, out)
}

unsafe extern "win64" fn marshal_add_ref(this: usize) -> u32 {
    super::ole32_lot_c::add_ref(controlling(this))
}

unsafe extern "win64" fn marshal_release(this: usize) -> u32 {
    super::ole32_lot_c::release(controlling(this))
}

unsafe extern "win64" fn get_unmarshal_class(
    _this: usize,
    _iid: *const Guid,
    _pv: usize,
    _dest: u32,
    _dest_data: usize,
    _flags: u32,
    cid: *mut Guid,
) -> i32 {
    if cid.is_null() {
        return E_POINTER;
    }
    std::ptr::write_unaligned(cid, CLSID_IN_PROC_FREE_MARSHALER);
    S_OK
}

unsafe extern "win64" fn get_marshal_size_max(
    _this: usize,
    iid: *const Guid,
    pv: usize,
    dest: u32,
    dest_data: usize,
    flags: u32,
    size: *mut u32,
) -> i32 {
    super::ole32_lot_c::co_get_marshal_size_max(size, iid, pv, dest, dest_data, flags)
}

unsafe extern "win64" fn marshal_interface(
    _this: usize,
    stm: usize,
    iid: *const Guid,
    pv: usize,
    dest: u32,
    dest_data: usize,
    flags: u32,
) -> i32 {
    super::ole32_lot_c::co_marshal_interface(stm, iid, pv, dest, dest_data, flags)
}

unsafe extern "win64" fn unmarshal_interface(
    _this: usize,
    stm: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    super::ole32_lot_c::co_unmarshal_interface(stm, iid, out)
}

unsafe extern "win64" fn release_marshal_data(_this: usize, stm: usize) -> i32 {
    super::ole32_lot_c::co_release_marshal_data(stm)
}

unsafe extern "win64" fn disconnect_object(_this: usize, _reserved: u32) -> i32 {
    S_OK
}

fn inner_vt() -> *const usize {
    static VT: std::sync::OnceLock<[usize; 3]> = std::sync::OnceLock::new();
    VT.get_or_init(|| {
        [
            inner_qi as *const () as usize,
            inner_add_ref as *const () as usize,
            inner_release as *const () as usize,
        ]
    })
    .as_ptr()
}

fn marshal_vt() -> *const usize {
    static VT: std::sync::OnceLock<[usize; 9]> = std::sync::OnceLock::new();
    VT.get_or_init(|| {
        [
            marshal_qi as *const () as usize,
            marshal_add_ref as *const () as usize,
            marshal_release as *const () as usize,
            get_unmarshal_class as *const () as usize,
            get_marshal_size_max as *const () as usize,
            marshal_interface as *const () as usize,
            unmarshal_interface as *const () as usize,
            release_marshal_data as *const () as usize,
            disconnect_object as *const () as usize,
        ]
    })
    .as_ptr()
}

/// CoCreateFreeThreadedMarshaler : IUnknown interne d'un marshaleur neuf
/// (une référence), agrégé à `outer` s'il n'est pas nul.
pub unsafe extern "win64" fn co_create_free_threaded_marshaler(
    outer: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let ftm = Box::new(Ftm {
        inner: inner_vt(),
        marshal: marshal_vt(),
        refs: AtomicU32::new(1),
        outer,
    });
    let p = Box::into_raw(ftm);
    if p.is_null() {
        *out = 0;
        return E_OUTOFMEMORY;
    }
    *out = p as usize;
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    static OUTER_REFS: AtomicI32 = AtomicI32::new(0);

    unsafe extern "win64" fn outer_qi(_: usize, _: *const Guid, out: *mut usize) -> i32 {
        *out = 0;
        E_NOINTERFACE
    }
    unsafe extern "win64" fn outer_add_ref(_: usize) -> u32 {
        (OUTER_REFS.fetch_add(1, Ordering::SeqCst) + 1) as u32
    }
    unsafe extern "win64" fn outer_release(_: usize) -> u32 {
        (OUTER_REFS.fetch_sub(1, Ordering::SeqCst) - 1) as u32
    }

    #[test]
    fn marshaleur_libre_cree_et_agrege_2674() {
        unsafe {
            // Objet qui agrège : son premier mot pointe sur sa vtable.
            let vt: &'static [usize; 3] = Box::leak(Box::new([
                outer_qi as usize,
                outer_add_ref as usize,
                outer_release as usize,
            ]));
            let outer = Box::leak(Box::new(vt.as_ptr() as usize)) as *const usize as usize;

            let mut unk = 0usize;
            assert_eq!(co_create_free_threaded_marshaler(outer, &mut unk), S_OK);
            assert_ne!(unk, 0, "S_OK doit rendre un marshaleur");

            // L'IUnknown interne sert IMarshal ; AddRef d'IMarshal va à l'agrégat.
            let mut m = 0usize;
            assert_eq!(inner_qi(unk, &IID_IMARSHAL, &mut m), S_OK);
            assert_ne!(m, 0);
            assert_eq!(OUTER_REFS.load(Ordering::SeqCst), 1);
            let mut cid = [0u8; 16];
            let f: unsafe extern "win64" fn(
                usize,
                *const Guid,
                usize,
                u32,
                usize,
                u32,
                *mut Guid,
            ) -> i32 = std::mem::transmute(*(*(m as *const *const usize)).add(3));
            assert_eq!(f(m, &IID_IUNKNOWN, 0, 0, 0, 0, &mut cid), S_OK);
            assert_eq!(cid, CLSID_IN_PROC_FREE_MARSHALER);
            marshal_release(m);
            assert_eq!(OUTER_REFS.load(Ordering::SeqCst), 0);

            // Une interface inconnue est refusée ; IUnknown rend l'objet interne.
            let mut x = 1usize;
            assert_eq!(inner_qi(unk, &[9u8; 16], &mut x), E_NOINTERFACE);
            assert_eq!(x, 0);
            assert_eq!(inner_qi(unk, &IID_IUNKNOWN, &mut x), S_OK);
            assert_eq!(x, unk);
            assert_eq!(inner_release(unk), 1);
            assert_eq!(inner_release(unk), 0);

            // Sans agrégation, IMarshal se rapporte à l'objet interne.
            let mut seul = 0usize;
            assert_eq!(co_create_free_threaded_marshaler(0, &mut seul), S_OK);
            let mut m = 0usize;
            assert_eq!(inner_qi(seul, &IID_IMARSHAL, &mut m), S_OK);
            assert_eq!(marshal_release(m), 1);
            assert_eq!(inner_release(seul), 0);
            assert_eq!(
                co_create_free_threaded_marshaler(0, std::ptr::null_mut()),
                E_POINTER
            );
        }
    }
}
