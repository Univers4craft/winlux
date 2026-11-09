//! Requêtes Direct3D 11 (ID3D11Query, ID3D11Predicate) : CreateQuery,
//! Begin, End et GetData selon le type de la requête (#1317).
//!
//! ANGLE (processus GPU de Chromium) mesure son temps GPU par une requête
//! TIMESTAMP_DISJOINT et deux TIMESTAMP, puis divise par la fréquence
//! rendue : des requêtes factices laissaient cette fréquence à 0. Les
//! résultats sont rendus aussitôt (pas de travail GPU différé ici) : une
//! horloge monotone en nanosecondes, une fréquence d'un milliard, et des
//! compteurs qui disent que le dessin a eu lieu.
//!
//! Dispositions : d3d11.h de MinGW-w64 (domaine public).

use super::*;

const D3D11_QUERY_EVENT: u32 = 0;
const D3D11_QUERY_OCCLUSION: u32 = 1;
const D3D11_QUERY_TIMESTAMP: u32 = 2;
const D3D11_QUERY_TIMESTAMP_DISJOINT: u32 = 3;
const D3D11_QUERY_PIPELINE_STATISTICS: u32 = 4;
const D3D11_QUERY_OCCLUSION_PREDICATE: u32 = 5;
const D3D11_QUERY_SO_STATISTICS: u32 = 6;
const D3D11_QUERY_SO_OVERFLOW_PREDICATE: u32 = 7;
const D3D11_QUERY_SO_STATISTICS_STREAM3: u32 = 11;
const D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM3: u32 = 15;

/// Fréquence des horodatages : l'horloge est en nanosecondes.
pub(super) const TIMESTAMP_FREQUENCY: u64 = 1_000_000_000;

/// Table d'ID3D11Query1 (ID3D11Predicate s'arrête à GetDesc).
#[repr(C)]
pub(super) struct QueryVTable {
    query_interface: unsafe extern "win64" fn(*mut Query, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut Query) -> u32,
    release: unsafe extern "win64" fn(*mut Query) -> u32,
    get_device: unsafe extern "win64" fn(*mut Query, *mut *mut c_void),
    get_private_data:
        unsafe extern "win64" fn(*mut Query, *const Guid, *mut u32, *mut c_void) -> i32,
    set_private_data: unsafe extern "win64" fn(*mut Query, *const Guid, u32, *const c_void) -> i32,
    set_private_data_interface:
        unsafe extern "win64" fn(*mut Query, *const Guid, *const c_void) -> i32,
    get_data_size: unsafe extern "win64" fn(*mut Query) -> u32,
    get_desc: unsafe extern "win64" fn(*mut Query, *mut u32),
    get_desc1: unsafe extern "win64" fn(*mut Query, *mut u32),
}

/// Requête : type, drapeaux, type de contexte (Query1) et horodatage pris
/// à End.
#[repr(C)]
pub(super) struct Query {
    vtable: *const QueryVTable,
    ref_count: AtomicU32,
    kind: u32,
    misc: u32,
    context_type: u32,
    ended: u64,
}

fn query_vt() -> *const QueryVTable {
    crate::dlls::dx11_x86::abi(&QUERY_VTABLE, "dlls::d3d11", "QueryVTable")
}

/// Horloge monotone en nanosecondes, jamais nulle.
fn now_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    (ts.tv_sec as u64)
        .wrapping_mul(1_000_000_000)
        .wrapping_add(ts.tv_nsec as u64)
        .max(1)
}

/// Taille du résultat de GetData pour un type de requête.
pub(super) fn data_size(kind: u32) -> u32 {
    match kind {
        D3D11_QUERY_EVENT
        | D3D11_QUERY_OCCLUSION_PREDICATE
        | D3D11_QUERY_SO_OVERFLOW_PREDICATE
        | 12..=D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM3 => 4,
        D3D11_QUERY_OCCLUSION | D3D11_QUERY_TIMESTAMP => 8,
        // D3D11_QUERY_DATA_TIMESTAMP_DISJOINT : UINT64 puis BOOL, aligné.
        D3D11_QUERY_TIMESTAMP_DISJOINT => 16,
        // D3D11_QUERY_DATA_PIPELINE_STATISTICS : onze UINT64.
        D3D11_QUERY_PIPELINE_STATISTICS => 88,
        D3D11_QUERY_SO_STATISTICS | 8..=D3D11_QUERY_SO_STATISTICS_STREAM3 => 16,
        _ => 0,
    }
}

/// CreateQuery, CreatePredicate et CreateQuery1 : `desc` est un
/// D3D11_QUERY_DESC (Query, MiscFlags), suivi de ContextType pour Query1.
pub(super) unsafe fn create(desc: *const u32, with_context: bool, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = std::ptr::null_mut();
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let kind = desc.read_unaligned();
    if data_size(kind) == 0 {
        return E_INVALIDARG;
    }
    let q = Box::into_raw(Box::new(Query {
        vtable: query_vt(),
        ref_count: AtomicU32::new(1),
        kind,
        misc: desc.add(1).read_unaligned(),
        context_type: if with_context {
            desc.add(2).read_unaligned()
        } else {
            0
        },
        ended: 0,
    }));
    *out = q.cast();
    S_OK
}

/// Une de nos requêtes, reconnue à sa table (natif, x86 ou tracée).
unsafe fn as_query(p: *mut c_void) -> Option<*mut Query> {
    if p.is_null() {
        return None;
    }
    let vt = *(p as *const *const QueryVTable);
    (vt == query_vt() || std::ptr::eq(vt, &QUERY_VTABLE)).then_some(p.cast())
}

/// ID3D11DeviceContext::End : l'horodatage est pris ici.
pub(super) unsafe extern "win64" fn ctx_end(_: *mut DeviceContext, q: *mut c_void) {
    if let Some(q) = as_query(q) {
        (*q).ended = now_ns();
    }
}

/// ID3D11DeviceContext::GetData : le résultat est toujours prêt (tampon
/// de l'appelant, aligné ou non).
pub(super) unsafe extern "win64" fn ctx_get_data(
    _: *mut DeviceContext,
    q: *mut c_void,
    data: *mut c_void,
    size: u32,
    _flags: u32,
) -> i32 {
    let Some(q) = as_query(q) else {
        // Objet inconnu (compteur, etc.) : prêt, rien à écrire.
        return S_OK;
    };
    if data.is_null() {
        return S_OK;
    }
    let want = data_size((*q).kind);
    if size != want {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(data.cast::<u8>(), 0, want as usize);
    match (*q).kind {
        // Événement passé ; prédicat d'occlusion : des pixels ont été tracés.
        D3D11_QUERY_EVENT | D3D11_QUERY_OCCLUSION_PREDICATE => {
            data.cast::<u32>().write_unaligned(1)
        }
        D3D11_QUERY_OCCLUSION => data.cast::<u64>().write_unaligned(1),
        D3D11_QUERY_TIMESTAMP => {
            let t = if (*q).ended != 0 {
                (*q).ended
            } else {
                now_ns()
            };
            data.cast::<u64>().write_unaligned(t);
        }
        D3D11_QUERY_TIMESTAMP_DISJOINT => {
            // Frequency ; Disjoint reste FALSE.
            data.cast::<u64>().write_unaligned(TIMESTAMP_FREQUENCY);
        }
        // Statistiques et débordements : zéro.
        _ => {}
    }
    S_OK
}

static QUERY_VTABLE: QueryVTable = QueryVTable {
    query_interface: q_qi,
    add_ref: q_add_ref,
    release: q_release,
    get_device: q_get_device,
    get_private_data: q_get_priv,
    set_private_data: q_set_priv,
    set_private_data_interface: q_set_priv_iface,
    get_data_size: q_get_data_size,
    get_desc: q_get_desc,
    get_desc1: q_get_desc1,
};

unsafe extern "win64" fn q_qi(this: *mut Query, _: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    q_add_ref(this);
    *out = this.cast();
    S_OK
}
unsafe extern "win64" fn q_add_ref(this: *mut Query) -> u32 {
    (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn q_release(this: *mut Query) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        crate::dlls::dxgi_private::forget(this as *const c_void);
        drop(Box::from_raw(this));
        return 0;
    }
    prev - 1
}
unsafe extern "win64" fn q_get_device(_: *mut Query, out: *mut *mut c_void) {
    child_get_device(std::ptr::null_mut(), out);
}
unsafe extern "win64" fn q_get_priv(
    this: *mut Query,
    g: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    crate::dlls::dxgi_private::get(this as *const c_void, g, size, data)
}
unsafe extern "win64" fn q_set_priv(
    this: *mut Query,
    g: *const Guid,
    size: u32,
    data: *const c_void,
) -> i32 {
    crate::dlls::dxgi_private::set(this as *const c_void, g, size, data)
}
unsafe extern "win64" fn q_set_priv_iface(
    this: *mut Query,
    g: *const Guid,
    iface: *const c_void,
) -> i32 {
    crate::dlls::dxgi_private::set_iface(this as *const c_void, g, iface)
}
unsafe extern "win64" fn q_get_data_size(this: *mut Query) -> u32 {
    data_size((*this).kind)
}
unsafe extern "win64" fn q_get_desc(this: *mut Query, out: *mut u32) {
    if !out.is_null() {
        *out = (*this).kind;
        *out.add(1) = (*this).misc;
    }
}
unsafe extern "win64" fn q_get_desc1(this: *mut Query, out: *mut u32) {
    if !out.is_null() {
        q_get_desc(this, out);
        *out.add(2) = (*this).context_type;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requetes_d_horodatage_remplies() {
        unsafe {
            let ctx = std::ptr::null_mut();
            let mut disjoint = std::ptr::null_mut();
            let desc = [D3D11_QUERY_TIMESTAMP_DISJOINT, 0];
            assert_eq!(create(desc.as_ptr(), false, &mut disjoint), S_OK);
            let mut t0 = std::ptr::null_mut();
            let mut t1 = std::ptr::null_mut();
            let desc = [D3D11_QUERY_TIMESTAMP, 0];
            assert_eq!(create(desc.as_ptr(), false, &mut t0), S_OK);
            assert_eq!(create(desc.as_ptr(), false, &mut t1), S_OK);
            // GetDataSize (case 7) et GetDesc (case 8) par la table.
            let m = |o: *mut c_void, i: usize| *(*(o as *const *const usize)).add(i);
            let size: unsafe extern "win64" fn(*mut c_void) -> u32 =
                std::mem::transmute(m(disjoint, 7));
            assert_eq!(size(disjoint), 16);
            let get_desc: unsafe extern "win64" fn(*mut c_void, *mut u32) =
                std::mem::transmute(m(t0, 8));
            let mut d = [9u32; 2];
            get_desc(t0, d.as_mut_ptr());
            assert_eq!(d, [D3D11_QUERY_TIMESTAMP, 0]);

            ctx_end(ctx, t0);
            std::thread::sleep(std::time::Duration::from_millis(2));
            ctx_end(ctx, t1);
            ctx_end(ctx, disjoint);
            // Fréquence non nulle, pas de discontinuité.
            let mut dj = [0xFFu8; 16];
            assert_eq!(
                ctx_get_data(ctx, disjoint, dj.as_mut_ptr().cast(), 16, 0),
                S_OK
            );
            assert_eq!(
                u64::from_le_bytes(dj[..8].try_into().unwrap()),
                TIMESTAMP_FREQUENCY
            );
            assert_eq!(u32::from_le_bytes(dj[8..12].try_into().unwrap()), 0);
            // Horodatages croissants.
            let (mut a, mut b) = (0u64, 0u64);
            assert_eq!(
                ctx_get_data(ctx, t0, (&mut a as *mut u64).cast(), 8, 0),
                S_OK
            );
            assert_eq!(
                ctx_get_data(ctx, t1, (&mut b as *mut u64).cast(), 8, 0),
                S_OK
            );
            assert!(a > 0 && b > a, "{a} {b}");
            // Mauvaise taille refusée ; sondage sans tampon accepté.
            assert_eq!(
                ctx_get_data(ctx, t0, (&mut a as *mut u64).cast(), 4, 0),
                E_INVALIDARG
            );
            assert_eq!(ctx_get_data(ctx, t0, std::ptr::null_mut(), 0, 0), S_OK);
            // Événement : TRUE.
            let mut ev = std::ptr::null_mut();
            assert_eq!(
                create([D3D11_QUERY_EVENT, 0].as_ptr(), false, &mut ev),
                S_OK
            );
            let mut done = 0u32;
            assert_eq!(
                ctx_get_data(ctx, ev, (&mut done as *mut u32).cast(), 4, 0),
                S_OK
            );
            assert_eq!(done, 1);
            // Type inconnu refusé.
            let mut bad = std::ptr::null_mut();
            assert_eq!(create([99u32, 0].as_ptr(), false, &mut bad), E_INVALIDARG);
            assert!(bad.is_null());
            for q in [disjoint, t0, t1, ev] {
                assert_eq!(q_release(q.cast()), 0);
            }
        }
    }
}
