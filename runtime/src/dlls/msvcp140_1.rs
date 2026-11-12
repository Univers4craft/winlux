//! msvcp140_1.dll maison (#2178) : les ressources mémoire de `std::pmr`
//! (`<memory_resource>`) de la bibliothèque C++ de Visual Studio, pour les
//! machines sans redistribuable. Qt 6 (Qt6Core, OBS) en importe
//! `_Aligned_get_default_resource`.
//!
//! D'après la bibliothèque standard publique de Microsoft (MS STL, licence
//! Apache 2.0, `stl/src/memory_resource.cpp` et `<xpolymorphic_allocator.h>`) :
//! sept fonctions `extern "C"` rendent ou posent des pointeurs vers des
//! objets `memory_resource`, dont la table virtuelle MSVC est, dans
//! l'ordre : destructeur (variante « scalar deleting », qui reçoit un
//! drapeau), `do_allocate(octets, alignement)`,
//! `do_deallocate(p, octets, alignement)`, `do_is_equal(autre)`.
//! Les ressources sont statiques : leur destructeur ne libère rien.
//!
//! Quand la vraie msvcp140_1.dll est présente, c'est elle qui est chargée :
//! voir `dlls::prefers_real_file`.

use super::Exports;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Objet C++ : son premier mot pointe sur la table virtuelle.
#[repr(C)]
struct Resource {
    vtable: *const [usize; 4],
}
// Les objets sont immuables et statiques.
unsafe impl Sync for Resource {}
unsafe impl Send for Resource {}

/// Alignement que garantit l'opérateur new par défaut sous Windows x64.
const DEFAULT_ALIGN: usize = 16;

extern "win64" fn dtor(this: *mut Resource, _flags: u32) -> *mut Resource {
    this
}

/// new/delete : bloc aligné sur `align` (au moins 16), rendu par
/// `do_deallocate` de la même ressource. Échec : std::bad_alloc.
extern "win64" fn nd_allocate(_this: *mut Resource, bytes: usize, align: usize) -> *mut c_void {
    let align = align.max(DEFAULT_ALIGN);
    if !align.is_power_of_two() {
        bad_alloc();
    }
    let mut p: *mut c_void = std::ptr::null_mut();
    if unsafe { libc::posix_memalign(&mut p, align, bytes.max(1)) } != 0 {
        bad_alloc();
    }
    p
}

extern "win64" fn nd_deallocate(
    _this: *mut Resource,
    p: *mut c_void,
    _bytes: usize,
    _align: usize,
) {
    unsafe { libc::free(p) };
}

/// Deux ressources sont égales seulement si c'est le même objet.
extern "win64" fn is_equal(this: *const Resource, other: *const Resource) -> bool {
    std::ptr::eq(this, other)
}

/// null_memory_resource : toute allocation lève std::bad_alloc.
extern "win64" fn null_allocate(_this: *mut Resource, _bytes: usize, _align: usize) -> *mut c_void {
    bad_alloc()
}

extern "win64" fn null_deallocate(_this: *mut Resource, _p: *mut c_void, _b: usize, _a: usize) {}

fn bad_alloc() -> ! {
    super::msvcp140::bad_alloc_fatal()
}

fn vtables() -> (&'static [usize; 4], &'static [usize; 4]) {
    static V: std::sync::OnceLock<([usize; 4], [usize; 4])> = std::sync::OnceLock::new();
    let v = V.get_or_init(|| {
        (
            [
                dtor as *const () as usize,
                nd_allocate as *const () as usize,
                nd_deallocate as *const () as usize,
                is_equal as *const () as usize,
            ],
            [
                dtor as *const () as usize,
                null_allocate as *const () as usize,
                null_deallocate as *const () as usize,
                is_equal as *const () as usize,
            ],
        )
    });
    (&v.0, &v.1)
}

fn resources() -> &'static [Resource; 3] {
    static R: std::sync::OnceLock<[Resource; 3]> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        let (nd, null) = vtables();
        [
            // Variante alignée (/Zc:alignedNew, le défaut).
            Resource { vtable: nd },
            // Variante non alignée : même allocateur, alignement au moins 16.
            Resource { vtable: nd },
            Resource { vtable: null },
        ]
    })
}

fn aligned_nd() -> usize {
    &resources()[0] as *const Resource as usize
}
fn unaligned_nd() -> usize {
    &resources()[1] as *const Resource as usize
}

/// Ressource par défaut de chaque variante ; 0 : celle de new/delete.
static ALIGNED_DEFAULT: AtomicUsize = AtomicUsize::new(0);
static UNALIGNED_DEFAULT: AtomicUsize = AtomicUsize::new(0);

fn get(slot: &AtomicUsize, fallback: usize) -> usize {
    match slot.load(Ordering::Acquire) {
        0 => fallback,
        r => r,
    }
}
/// Pose la ressource par défaut (nulle : new/delete) et rend la précédente.
fn set(slot: &AtomicUsize, fallback: usize, r: usize) -> usize {
    match slot.swap(r, Ordering::AcqRel) {
        0 => fallback,
        old => old,
    }
}

extern "win64" fn aligned_get_default_resource() -> usize {
    get(&ALIGNED_DEFAULT, aligned_nd())
}
extern "win64" fn aligned_set_default_resource(r: usize) -> usize {
    set(&ALIGNED_DEFAULT, aligned_nd(), r)
}
extern "win64" fn aligned_new_delete_resource() -> usize {
    aligned_nd()
}
extern "win64" fn unaligned_get_default_resource() -> usize {
    get(&UNALIGNED_DEFAULT, unaligned_nd())
}
extern "win64" fn unaligned_set_default_resource(r: usize) -> usize {
    set(&UNALIGNED_DEFAULT, unaligned_nd(), r)
}
extern "win64" fn unaligned_new_delete_resource() -> usize {
    unaligned_nd()
}
extern "win64" fn null_memory_resource() -> usize {
    &resources()[2] as *const Resource as usize
}

pub fn exports() -> Exports {
    crate::exports! {
        "_Aligned_get_default_resource" => fn aligned_get_default_resource,
        "_Aligned_set_default_resource" => fn aligned_set_default_resource,
        "_Aligned_new_delete_resource" => fn aligned_new_delete_resource,
        "_Unaligned_get_default_resource" => fn unaligned_get_default_resource,
        "_Unaligned_set_default_resource" => fn unaligned_set_default_resource,
        "_Unaligned_new_delete_resource" => fn unaligned_new_delete_resource,
        "null_memory_resource" => fn null_memory_resource,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Alloc = extern "win64" fn(usize, usize, usize) -> *mut c_void;
    type Dealloc = extern "win64" fn(usize, *mut c_void, usize, usize);
    type IsEqual = extern "win64" fn(usize, usize) -> bool;

    unsafe fn slot(r: usize, i: usize) -> usize {
        (*(*(r as *const Resource)).vtable)[i]
    }

    #[test]
    fn ressources_pmr_par_la_table_virtuelle() {
        unsafe {
            let r = aligned_get_default_resource();
            assert_eq!(r, aligned_new_delete_resource());
            let alloc: Alloc = std::mem::transmute(slot(r, 1));
            let dealloc: Dealloc = std::mem::transmute(slot(r, 2));
            let eq: IsEqual = std::mem::transmute(slot(r, 3));
            for align in [1, 8, 16, 64, 4096] {
                let p = alloc(r, 100, align);
                assert!(!p.is_null());
                assert_eq!(p as usize % align.max(16), 0);
                std::ptr::write_bytes(p as *mut u8, 0xAB, 100);
                dealloc(r, p, 100, align);
            }
            assert!(eq(r, r));
            assert!(!eq(r, null_memory_resource()));
            assert!(!eq(r, unaligned_new_delete_resource()));
            // Pose puis rétablissement : set rend la précédente, nul = new/delete.
            let n = null_memory_resource();
            assert_eq!(aligned_set_default_resource(n), r);
            assert_eq!(aligned_get_default_resource(), n);
            assert_eq!(aligned_set_default_resource(0), n);
            assert_eq!(aligned_get_default_resource(), r);
            assert_eq!(
                unaligned_get_default_resource(),
                unaligned_new_delete_resource()
            );
            let d: extern "win64" fn(usize, u32) -> usize = std::mem::transmute(slot(r, 0));
            assert_eq!(d(r, 0), r);
        }
    }
}
