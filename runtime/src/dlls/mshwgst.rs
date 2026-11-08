//! mshwgst.dll : reconnaissance de gestes de l'encre (API Recognizer de
//! recapis.h, documentée sur Microsoft Learn).
//!
//! WPF ne déclare la reconnaissance de gestes disponible
//! (`System.Windows.Ink.GestureRecognizer`) que si la valeur
//! `Recognizer dll` de `HKLM\Software\Microsoft\TPG\System Recognizers\
//! {BED9A940-7D48-48E3-9A68-F4887A5A1B2E}` désigne une DLL chargeable. Sinon,
//! un `InkCanvas` qui active des gestes lève « Aucune reconnaissance de
//! mouvement n'est disponible sur le système » dès sa construction
//! (Éditeur de ScreenToGif, #1132).
//!
//! Winlux n'a pas de moteur de reconnaissance : le contexte accepte les
//! traits et rend toujours une liste d'alternatives vide. Pour WPF, aucun
//! geste n'est reconnu (comme un trait qui ne ressemble à rien) et le trait
//! reste de l'encre ordinaire.

use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const S_OK: i32 = 0;
const E_POINTER: i32 = 0x8000_4003_u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;
const E_FAIL: i32 = 0x8000_4005_u32 as i32;
const REGDB_E_CLASSNOTREG: i32 = 0x8004_0154_u32 as i32;

/// CLSID du moteur de gestes ({BED9A940-7D48-48E3-9A68-F4887A5A1B2E}).
pub const GESTURE_RECOGNIZER_CLSID: [u8; 16] = [
    0x40, 0xA9, 0xD9, 0xBE, 0x48, 0x7D, 0xE3, 0x48, 0x9A, 0x68, 0xF4, 0x88, 0x7A, 0x5A, 0x1B, 0x2E,
];

/// Clé du registre où WPF lit le chemin de la DLL de gestes.
pub const REGISTRY_KEY: &str =
    "hklm\\Software\\Microsoft\\TPG\\System Recognizers\\{BED9A940-7D48-48E3-9A68-F4887A5A1B2E}";
pub const REGISTRY_VALUE: &str = "Recognizer dll";
pub const DLL_PATH: &str = "C:\\Windows\\System32\\mshwgst.dll";

static NEXT: AtomicUsize = AtomicUsize::new(0x5747_0000);
static RECOGNIZERS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
static CONTEXTS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

fn add(set: &Mutex<Option<HashSet<usize>>>) -> usize {
    let h = NEXT.fetch_add(4, Ordering::Relaxed);
    set.lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashSet::new)
        .insert(h);
    h
}

fn known(set: &Mutex<Option<HashSet<usize>>>, h: usize) -> bool {
    set.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|s| s.contains(&h))
}

fn remove(set: &Mutex<Option<HashSet<usize>>>, h: usize) -> bool {
    set.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .is_some_and(|s| s.remove(&h))
}

fn context_status(ctx: usize) -> i32 {
    if known(&CONTEXTS, ctx) {
        S_OK
    } else {
        E_INVALIDARG
    }
}

/// CreateRecognizer : seul le moteur de gestes est fourni.
pub unsafe extern "win64" fn create_recognizer(clsid: *const [u8; 16], out: *mut usize) -> i32 {
    if clsid.is_null() || out.is_null() {
        return E_POINTER;
    }
    if *clsid != GESTURE_RECOGNIZER_CLSID {
        *out = 0;
        return REGDB_E_CLASSNOTREG;
    }
    *out = add(&RECOGNIZERS);
    S_OK
}

pub extern "win64" fn destroy_recognizer(rec: usize) -> i32 {
    if remove(&RECOGNIZERS, rec) {
        S_OK
    } else {
        E_INVALIDARG
    }
}

pub unsafe extern "win64" fn create_context(rec: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if !known(&RECOGNIZERS, rec) {
        *out = 0;
        return E_INVALIDARG;
    }
    *out = add(&CONTEXTS);
    S_OK
}

pub extern "win64" fn destroy_context(ctx: usize) -> i32 {
    if remove(&CONTEXTS, ctx) {
        S_OK
    } else {
        E_INVALIDARG
    }
}

/// AddStroke : le trait est accepté (il n'est pas analysé).
pub extern "win64" fn add_stroke(
    ctx: usize,
    _packet_desc: *const c_void,
    _size: u32,
    _packets: *const u8,
    _xform: *const c_void,
) -> i32 {
    context_status(ctx)
}

pub extern "win64" fn set_enabled_unicode_ranges(
    ctx: usize,
    count: u32,
    ranges: *const c_void,
) -> i32 {
    if count != 0 && ranges.is_null() {
        return E_POINTER;
    }
    context_status(ctx)
}

pub extern "win64" fn end_ink_input(ctx: usize) -> i32 {
    context_status(ctx)
}

pub extern "win64" fn reset_context(ctx: usize) -> i32 {
    context_status(ctx)
}

/// Process : l'analyse est terminée d'emblée (pas de traitement partiel).
pub unsafe extern "win64" fn process(ctx: usize, partial: *mut i32) -> i32 {
    if partial.is_null() {
        return E_POINTER;
    }
    *partial = 0;
    context_status(ctx)
}

/// GetAlternateList : aucune alternative, aucun geste reconnu.
pub unsafe extern "win64" fn get_alternate_list(
    ctx: usize,
    _range: *mut c_void,
    count: *mut u32,
    _alternates: *mut usize,
    _breaks: u32,
) -> i32 {
    if count.is_null() {
        return E_POINTER;
    }
    *count = 0;
    context_status(ctx)
}

/// GetLatticePtr : treillis vide (aucune colonne, aucun meilleur résultat).
pub unsafe extern "win64" fn get_lattice_ptr(ctx: usize, out: *mut *const c_void) -> i32 {
    static EMPTY_LATTICE: [usize; 8] = [0; 8];
    if out.is_null() {
        return E_POINTER;
    }
    *out = EMPTY_LATTICE.as_ptr() as *const c_void;
    context_status(ctx)
}

/// Les alternatives n'existent jamais : leurs fonctions échouent.
pub extern "win64" fn destroy_alternate(_alt: usize) -> i32 {
    E_INVALIDARG
}

pub extern "win64" fn get_string(
    _alt: usize,
    _range: *mut c_void,
    _size: *mut u32,
    _text: *mut u16,
) -> i32 {
    E_FAIL
}

pub extern "win64" fn get_confidence_level(
    _alt: usize,
    _range: *mut c_void,
    _level: *mut u32,
) -> i32 {
    E_FAIL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateRecognizer" => fn create_recognizer,
        "DestroyRecognizer" => fn destroy_recognizer,
        "CreateContext" => fn create_context,
        "DestroyContext" => fn destroy_context,
        "AddStroke" => fn add_stroke,
        "SetEnabledUnicodeRanges" => fn set_enabled_unicode_ranges,
        "EndInkInput" => fn end_ink_input,
        "ResetContext" => fn reset_context,
        "Process" => fn process,
        "GetAlternateList" => fn get_alternate_list,
        "GetLatticePtr" => fn get_lattice_ptr,
        "DestroyAlternate" => fn destroy_alternate,
        "GetString" => fn get_string,
        "GetConfidenceLevel" => fn get_confidence_level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnaissance_de_gestes_disponible_sans_geste_reconnu() {
        // #1132 : le chemin suivi par WPF (GestureRecognizer, InkCanvas).
        let mut rec = 0usize;
        let other = [0u8; 16];
        unsafe {
            assert_eq!(create_recognizer(&other, &mut rec), REGDB_E_CLASSNOTREG);
            assert_eq!(create_recognizer(&GESTURE_RECOGNIZER_CLSID, &mut rec), S_OK);
        }
        assert_ne!(rec, 0);
        let mut ctx = 0usize;
        assert_eq!(unsafe { create_context(rec, &mut ctx) }, S_OK);
        let ranges = [0xF000u32, 1];
        assert_eq!(
            set_enabled_unicode_ranges(ctx, 1, ranges.as_ptr() as *const c_void),
            S_OK
        );
        assert_eq!(reset_context(ctx), S_OK);
        assert_eq!(
            add_stroke(ctx, std::ptr::null(), 0, std::ptr::null(), std::ptr::null()),
            S_OK
        );
        assert_eq!(end_ink_input(ctx), S_OK);
        let mut partial = 1i32;
        assert_eq!(unsafe { process(ctx, &mut partial) }, S_OK);
        assert_eq!(partial, 0);
        let mut count = 10u32;
        let mut alts = [0usize; 10];
        let r = unsafe {
            get_alternate_list(ctx, std::ptr::null_mut(), &mut count, alts.as_mut_ptr(), 0)
        };
        assert_eq!((r, count), (S_OK, 0));
        let mut lattice: *const c_void = std::ptr::null();
        assert_eq!(unsafe { get_lattice_ptr(ctx, &mut lattice) }, S_OK);
        // ulColumnCount et ulBestResultColumnCount nuls.
        assert!(
            unsafe { std::slice::from_raw_parts(lattice as *const usize, 8) }
                .iter()
                .all(|&v| v == 0)
        );
        assert_eq!(destroy_context(ctx), S_OK);
        assert_eq!(destroy_context(ctx), E_INVALIDARG);
        assert_eq!(reset_context(ctx), E_INVALIDARG);
        assert_eq!(destroy_recognizer(rec), S_OK);
    }
}
