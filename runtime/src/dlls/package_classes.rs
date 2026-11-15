//! Classes activables déclarées par le paquet MSIX du programme (#2657).
//!
//! Le manifeste d'un paquet peut déclarer, par l'extension
//! `windows.activatableClass.inProcessServer`, une DLL du paquet et les
//! classes Windows Runtime qu'elle fournit (Microsoft Learn, schéma du
//! manifeste de paquet, élément `InProcessServer`). Pour une telle classe,
//! l'activation charge la DLL et appelle son export
//! `DllGetActivationFactory(HSTRING, IActivationFactory **)` (Microsoft
//! Learn, « DllGetActivationFactory entry point »).
//!
//! Inférences, faute de texte public : le chemin `Path` est relatif au
//! dossier du paquet (l'exemple de Learn ne donne qu'un nom de fichier), et
//! `RoGetActivationFactory` rend l'interface `iid` de la fabrique, obtenue
//! par QueryInterface, comme le suggère sa signature `(HSTRING, REFIID,
//! void **)`.

const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const CO_E_DLLNOTFOUND: i32 = 0x8004_01f8u32 as i32;
const CO_E_ERRORINDLL: i32 = 0x8004_01f9u32 as i32;

type DllGetActivationFactory = unsafe extern "win64" fn(usize, *mut usize) -> i32;
type QueryInterface = unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32;
type Release = unsafe extern "win64" fn(usize) -> u32;

/// Fabrique d'une classe déclarée par le paquet du programme courant.
/// `None` : le programme n'est pas empaqueté ou son paquet ne déclare pas
/// la classe ; l'appelant garde son propre code d'erreur.
pub unsafe fn activation_factory(
    class: usize,
    name: &str,
    iid: *const u8,
    out: *mut usize,
) -> Option<i32> {
    // Le code x86 d'une DLL 32 bits ne s'appelle pas d'ici.
    if crate::wow32::active() {
        return None;
    }
    let (pkg, dir) = super::kernel32_misc::current_package()?;
    let (_, rel) = pkg.activatable_classes.iter().find(|(c, _)| c == name)?;
    let path = format!("{dir}\\{}", rel.replace('/', "\\"));
    let Some(module) = super::com_registry::load(&path) else {
        crate::log_warn!(
            "com",
            "RoGetActivationFactory : {path} (classe « {name} » du paquet) introuvable (CO_E_DLLNOTFOUND)"
        );
        return Some(CO_E_DLLNOTFOUND);
    };
    let entry =
        super::kernel32::get_proc_address(module, c"DllGetActivationFactory".as_ptr().cast());
    if entry == 0 {
        crate::log_warn!(
            "com",
            "RoGetActivationFactory : {path} n'exporte pas DllGetActivationFactory (CO_E_ERRORINDLL)"
        );
        return Some(CO_E_ERRORINDLL);
    }
    let hr = factory_from_entry(entry, class, iid, out);
    if hr < 0 {
        crate::log_warn!(
            "com",
            "RoGetActivationFactory : DllGetActivationFactory de {path} refuse « {name} » (0x{:08x})",
            hr as u32
        );
    }
    Some(hr)
}

/// Appelle `DllGetActivationFactory` puis demande l'interface `iid` à la
/// fabrique rendue ; la référence de la fabrique elle-même est rendue.
unsafe fn factory_from_entry(entry: usize, class: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() || iid.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    let get: DllGetActivationFactory = std::mem::transmute(entry);
    let mut factory = 0usize;
    let hr = get(class, &mut factory);
    if hr < 0 {
        return hr;
    }
    if factory == 0 {
        return E_NOINTERFACE;
    }
    let vtbl = *(factory as *const *const usize);
    let qi: QueryInterface = std::mem::transmute(*vtbl);
    let release: Release = std::mem::transmute(*vtbl.add(2));
    let hr = qi(factory, iid, out);
    release(factory);
    if hr < 0 {
        *out = 0;
    }
    hr
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// IID_IActivationFactory {00000035-0000-0000-C000-000000000046}.
    const IID_ACTIVATION_FACTORY: [u8; 16] =
        [0x35, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
    const IID_AUTRE: [u8; 16] = [7; 16];

    static REFS: AtomicU32 = AtomicU32::new(0);
    static CLASSE_VUE: AtomicU32 = AtomicU32::new(0);

    #[repr(C)]
    struct Fabrique {
        vtbl: *const usize,
    }

    unsafe extern "win64" fn qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
        let iid = std::slice::from_raw_parts(iid, 16);
        if iid == IID_ACTIVATION_FACTORY {
            REFS.fetch_add(1, Ordering::SeqCst);
            *out = this;
            0
        } else {
            *out = 0;
            E_NOINTERFACE
        }
    }
    unsafe extern "win64" fn add_ref(_: usize) -> u32 {
        REFS.fetch_add(1, Ordering::SeqCst) + 1
    }
    unsafe extern "win64" fn release(_: usize) -> u32 {
        REFS.fetch_sub(1, Ordering::SeqCst) - 1
    }

    fn fabrique() -> usize {
        static F: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        *F.get_or_init(|| {
            let vtbl: &'static [usize; 3] =
                Box::leak(Box::new([qi as usize, add_ref as usize, release as usize]));
            Box::leak(Box::new(Fabrique {
                vtbl: vtbl.as_ptr(),
            })) as *const Fabrique as usize
        })
    }

    unsafe extern "win64" fn dll_get(class: usize, out: *mut usize) -> i32 {
        CLASSE_VUE.store(class as u32, Ordering::SeqCst);
        REFS.fetch_add(1, Ordering::SeqCst);
        *out = fabrique();
        0
    }

    #[test]
    fn fabrique_du_paquet_par_dll_get_activation_factory_2657() {
        unsafe {
            REFS.store(0, Ordering::SeqCst);
            let mut out = 0usize;
            let hr = factory_from_entry(
                dll_get as usize,
                0x1234,
                IID_ACTIVATION_FACTORY.as_ptr(),
                &mut out,
            );
            assert_eq!(hr, 0);
            assert_eq!(out, fabrique());
            assert_eq!(CLASSE_VUE.load(Ordering::SeqCst), 0x1234);
            // Une référence pour l'appelant, celle de DllGetActivationFactory rendue.
            assert_eq!(REFS.load(Ordering::SeqCst), 1);

            REFS.store(0, Ordering::SeqCst);
            let mut out = 0x55usize;
            let hr = factory_from_entry(dll_get as usize, 0x1234, IID_AUTRE.as_ptr(), &mut out);
            assert_eq!(hr, E_NOINTERFACE);
            assert_eq!(out, 0);
            assert_eq!(REFS.load(Ordering::SeqCst), 0);
        }
    }
}
