//! Espaces de noms privés et descripteurs de frontière (#1606) :
//! CreateBoundaryDescriptorA/W, AddSIDToBoundaryDescriptor,
//! AddIntegrityLabelToBoundaryDescriptor, DeleteBoundaryDescriptor,
//! CreatePrivateNamespaceA/W, OpenPrivateNamespaceA/W et
//! ClosePrivateNamespace. Un espace porte le nom du descripteur et un
//! alias ; les objets nommés « alias\nom » qu'un programme y crée sont des
//! objets nommés ordinaires de Winlux, dont le nom commence par l'alias.
//!
//! Sources : documentation Microsoft Learn de ces fonctions ; noms
//! vérifiés dans kernel32.Lib du SDK Windows public (noms seulement).

use crate::process::set_last_error;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const ERROR_PATH_NOT_FOUND: u32 = 3;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_ALREADY_EXISTS: u32 = 183;

enum Object {
    /// Descripteur de frontière : son nom (les SID ajoutés ne changent rien
    /// sous Winlux, un seul compte voit les objets du processus).
    Boundary(String),
    /// Espace ouvert : (nom du descripteur, alias).
    Namespace(String, String),
}

struct State {
    objects: HashMap<usize, Object>,
    /// Espaces créés et non détruits : (descripteur, alias) → créateurs.
    created: HashMap<(String, String), usize>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(State {
            objects: HashMap::new(),
            created: HashMap::new(),
        })
    })
}

/// Poignées multiples de 4, sous 4 Gio pour les programmes x86.
fn new_handle(o: Object) -> usize {
    static NEXT: AtomicUsize = AtomicUsize::new(0x7E00_0000);
    let h = NEXT.fetch_add(4, Ordering::Relaxed);
    super::lock(state()).objects.insert(h, o);
    h
}

unsafe fn text_a(p: *const u8) -> Option<String> {
    (!p.is_null()).then(|| super::cstr(p))
}

unsafe fn text_w(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| super::wstr(p))
}

fn create_boundary(name: Option<String>) -> usize {
    match name {
        Some(n) => new_handle(Object::Boundary(n)),
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

pub unsafe extern "win64" fn create_boundary_descriptor_w(name: *const u16, _flags: u32) -> usize {
    create_boundary(text_w(name))
}

pub unsafe extern "win64" fn create_boundary_descriptor_a(name: *const u8, _flags: u32) -> usize {
    create_boundary(text_a(name))
}

/// AddSIDToBoundaryDescriptor / AddIntegrityLabelToBoundaryDescriptor :
/// la poignée est passée par adresse (elle pourrait être réallouée).
pub unsafe extern "win64" fn add_sid_to_boundary_descriptor(desc: *mut usize, sid: usize) -> i32 {
    if desc.is_null() || sid == 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let h = crate::wow32::read_ptr(desc as usize);
    if matches!(
        super::lock(state()).objects.get(&h),
        Some(Object::Boundary(_))
    ) {
        1
    } else {
        set_last_error(ERROR_INVALID_HANDLE);
        0
    }
}

pub unsafe extern "win64" fn delete_boundary_descriptor(desc: usize) {
    let mut s = super::lock(state());
    if matches!(s.objects.get(&desc), Some(Object::Boundary(_))) {
        s.objects.remove(&desc);
    }
}

fn namespace(boundary: usize, alias: Option<String>, create: bool) -> usize {
    let Some(alias) = alias else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let mut s = super::lock(state());
    let Some(Object::Boundary(name)) = s.objects.get(&boundary) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let key = (name.to_lowercase(), alias.to_lowercase());
    if create {
        if s.created.contains_key(&key) {
            set_last_error(ERROR_ALREADY_EXISTS);
            return 0;
        }
        s.created.insert(key.clone(), 1);
    } else {
        match s.created.get_mut(&key) {
            Some(n) => *n += 1,
            None => {
                set_last_error(ERROR_PATH_NOT_FOUND);
                return 0;
            }
        }
    }
    drop(s);
    new_handle(Object::Namespace(key.0, key.1))
}

pub unsafe extern "win64" fn create_private_namespace_w(
    _attrs: usize,
    boundary: usize,
    alias: *const u16,
) -> usize {
    namespace(boundary, text_w(alias), true)
}

pub unsafe extern "win64" fn create_private_namespace_a(
    _attrs: usize,
    boundary: usize,
    alias: *const u8,
) -> usize {
    namespace(boundary, text_a(alias), true)
}

pub unsafe extern "win64" fn open_private_namespace_w(boundary: usize, alias: *const u16) -> usize {
    namespace(boundary, text_w(alias), false)
}

pub unsafe extern "win64" fn open_private_namespace_a(boundary: usize, alias: *const u8) -> usize {
    namespace(boundary, text_a(alias), false)
}

/// ClosePrivateNamespace : PRIVATE_NAMESPACE_FLAG_DESTROY (1) retire
/// l'espace, sinon il disparaît avec sa dernière poignée.
pub extern "win64" fn close_private_namespace(handle: usize, flags: u32) -> u8 {
    let mut s = super::lock(state());
    let Some(Object::Namespace(b, a)) = s.objects.remove(&handle) else {
        set_last_error(ERROR_INVALID_HANDLE);
        return 0;
    };
    let key = (b, a);
    let last = match s.created.get_mut(&key) {
        Some(n) => {
            *n = n.saturating_sub(1);
            *n == 0
        }
        None => false,
    };
    if flags & 1 != 0 || last {
        s.created.remove(&key);
    }
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateBoundaryDescriptorW" => fn create_boundary_descriptor_w,
        "CreateBoundaryDescriptorA" => fn create_boundary_descriptor_a,
        "AddSIDToBoundaryDescriptor" => fn add_sid_to_boundary_descriptor,
        "AddIntegrityLabelToBoundaryDescriptor" => fn add_sid_to_boundary_descriptor,
        "DeleteBoundaryDescriptor" => fn delete_boundary_descriptor,
        "CreatePrivateNamespaceW" => fn create_private_namespace_w,
        "CreatePrivateNamespaceA" => fn create_private_namespace_a,
        "OpenPrivateNamespaceW" => fn open_private_namespace_w,
        "OpenPrivateNamespaceA" => fn open_private_namespace_a,
        "ClosePrivateNamespace" => fn close_private_namespace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn espaces_de_noms_prives() {
        let _teb = crate::process::test_teb();
        unsafe {
            let name: Vec<u16> = "WinluxFrontiere\0".encode_utf16().collect();
            let alias: Vec<u16> = "Alias\0".encode_utf16().collect();
            let mut b = create_boundary_descriptor_w(name.as_ptr(), 0);
            assert_ne!(b, 0);
            let sid = [1u8, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];
            assert_eq!(
                add_sid_to_boundary_descriptor(&mut b, sid.as_ptr() as usize),
                1
            );
            // Absent : OpenPrivateNamespace échoue.
            assert_eq!(open_private_namespace_w(b, alias.as_ptr()), 0);
            assert_eq!(crate::process::last_error(), ERROR_PATH_NOT_FOUND);
            let ns = create_private_namespace_w(0, b, alias.as_ptr());
            assert_ne!(ns, 0);
            assert_eq!(create_private_namespace_w(0, b, alias.as_ptr()), 0);
            assert_eq!(crate::process::last_error(), ERROR_ALREADY_EXISTS);
            let opened = open_private_namespace_a(b, c"alias".as_ptr().cast());
            assert_ne!(opened, 0);
            assert_eq!(close_private_namespace(opened, 0), 1);
            assert_eq!(close_private_namespace(ns, 1), 1);
            assert_eq!(close_private_namespace(ns, 1), 0);
            assert_eq!(open_private_namespace_w(b, alias.as_ptr()), 0);
            delete_boundary_descriptor(b);
            assert_eq!(create_private_namespace_w(0, b, alias.as_ptr()), 0);
        }
    }
}
