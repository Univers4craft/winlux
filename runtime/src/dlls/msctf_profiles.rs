//! Profils de saisie du Text Services Framework : CLSID_TF_InputProcessorProfiles
//! (ITfInputProcessorProfiles et ITfInputProcessorProfileMgr, msctf.h).
//!
//! Winlux n'a pas de processeur de texte (TIP) : la saisie passe par la
//! disposition de clavier X11. L'objet décrit donc une seule « entrée » :
//! la disposition de clavier courante, dans la langue de l'utilisateur.
//! steamwebhelper.exe (CEF 126) le demande au démarrage et s'arrêtait sur
//! un CHECK quand la classe manquait (#1602).
//!
//! Méthodes, ordre et structures : documentation Microsoft Learn de
//! ITfInputProcessorProfiles, ITfInputProcessorProfileMgr,
//! IEnumTfInputProcessorProfiles, IEnumTfLanguageProfiles et
//! TF_INPUTPROCESSORPROFILE, et msctf.idl de MinGW-w64.

use super::comobj::{guid, log_no_interface, read_guid, Guid, VTable};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_FAIL: i32 = 0x8000_4005u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;

pub const CLSID_TF_INPUT_PROCESSOR_PROFILES: Guid = guid(
    0x33C5_3A50,
    0xF456,
    0x4884,
    [0xB0, 0x49, 0x85, 0xFD, 0x64, 0x3E, 0xCF, 0xED],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ITF_INPUT_PROCESSOR_PROFILES: Guid = guid(
    0x1F02_B6C5,
    0x7842,
    0x4EE6,
    [0x8A, 0x0B, 0x9A, 0x24, 0x18, 0x3A, 0x95, 0xCA],
);
const IID_ITF_INPUT_PROCESSOR_PROFILE_MGR: Guid = guid(
    0x71C6_E74C,
    0x0F28,
    0x11D8,
    [0xA8, 0x2A, 0x00, 0x06, 0x5B, 0x84, 0x43, 0x5C],
);
const IID_IENUM_TF_INPUT_PROCESSOR_PROFILES: Guid = guid(
    0x71C6_E74D,
    0x0F28,
    0x11D8,
    [0xA8, 0x2A, 0x00, 0x06, 0x5B, 0x84, 0x43, 0x5C],
);
const IID_IENUM_TF_LANGUAGE_PROFILES: Guid = guid(
    0x3D61_BF11,
    0xAC5F,
    0x42C8,
    [0xA4, 0xCB, 0x93, 0x1B, 0xCC, 0x28, 0xC7, 0x44],
);
const GUID_TFCAT_TIP_KEYBOARD: Guid = guid(
    0x3474_5C63,
    0xB2F0,
    0x4784,
    [0x8B, 0x67, 0x5E, 0x12, 0xC8, 0x70, 0x1A, 0x31],
);

const TF_PROFILETYPE_KEYBOARDLAYOUT: u32 = 2;
const TF_IPP_FLAG_ACTIVE: u32 = 1;
const TF_IPP_FLAG_ENABLED: u32 = 2;
/// sizeof(TF_INPUTPROCESSORPROFILE) en 64 bits.
const PROFILE_SIZE: usize = 88;

fn langid() -> u16 {
    crate::intl::api::get_user_default_lang_id()
}

fn hkl() -> usize {
    crate::intl::api::get_keyboard_layout(0)
}

/// Remplit un TF_INPUTPROCESSORPROFILE pour la disposition de clavier.
unsafe fn fill_profile(p: *mut u8) {
    std::ptr::write_bytes(p, 0, PROFILE_SIZE);
    let w32 = |o: usize, v: u32| (p.add(o) as *mut u32).write_unaligned(v);
    w32(0, TF_PROFILETYPE_KEYBOARDLAYOUT);
    (p.add(4) as *mut u16).write_unaligned(langid());
    // clsid (8) et guidProfile (24) restent nuls pour une disposition.
    std::ptr::copy_nonoverlapping(GUID_TFCAT_TIP_KEYBOARD.as_ptr(), p.add(40), 16);
    // hklSubstitute (56) nul, dwCaps (64) nul.
    (p.add(72) as *mut usize).write_unaligned(hkl());
    w32(80, TF_IPP_FLAG_ACTIVE | TF_IPP_FLAG_ENABLED);
}

// ---------------------------------------------------------------------
// Objet principal : deux faces (Profiles, ProfileMgr) et un compteur.
// ---------------------------------------------------------------------

#[repr(C)]
struct Profiles {
    vt_profiles: *const (),
    vt_mgr: *const (),
    refs: AtomicU32,
}

unsafe fn from_mgr(this: usize) -> usize {
    this - std::mem::size_of::<usize>()
}

unsafe fn qi(obj: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let p = obj as *mut Profiles;
    let face = match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_ITF_INPUT_PROCESSOR_PROFILES => obj,
        Some(g) if g == IID_ITF_INPUT_PROCESSOR_PROFILE_MGR => obj + std::mem::size_of::<usize>(),
        _ => {
            log_no_interface("TF_InputProcessorProfiles", riid);
            return E_NOINTERFACE;
        }
    };
    (*p).refs.fetch_add(1, Ordering::Relaxed);
    *out = face as *mut c_void;
    S_OK
}

unsafe fn release(obj: usize) -> u32 {
    let p = obj as *mut Profiles;
    let n = (*p).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(p));
    }
    n
}

unsafe extern "win64" fn p_qi(this: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    qi(this, riid, out)
}
unsafe extern "win64" fn p_addref(this: usize) -> u32 {
    (*(this as *mut Profiles))
        .refs
        .fetch_add(1, Ordering::Relaxed)
        + 1
}
unsafe extern "win64" fn p_release(this: usize) -> u32 {
    release(this)
}
unsafe extern "win64" fn m_qi(this: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    qi(from_mgr(this), riid, out)
}
unsafe extern "win64" fn m_addref(this: usize) -> u32 {
    p_addref(from_mgr(this))
}
unsafe extern "win64" fn m_release(this: usize) -> u32 {
    release(from_mgr(this))
}

/// Inscription ou retrait d'un TIP : aucun TIP n'est géré.
unsafe extern "win64" fn not_impl_1(_this: usize, _a: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn not_impl_3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn not_impl_5(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
) -> i32 {
    E_NOTIMPL
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn not_impl_8(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
    _h: usize,
) -> i32 {
    E_NOTIMPL
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn not_impl_10(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
    _h: usize,
    _i: usize,
    _j: usize,
) -> i32 {
    E_NOTIMPL
}

/// EnumInputProcessorInfo(ppEnum) : aucun TIP, énumérateur de CLSID vide
/// refusé faute d'IEnumGUID ; le pointeur est mis à zéro.
unsafe extern "win64" fn p_enum_input_processor_info(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    E_NOTIMPL
}
/// GetDefaultLanguageProfile(langid, catid, pclsid, pguidProfile) : pas de
/// TIP par défaut.
unsafe extern "win64" fn p_get_default_language_profile(
    _this: usize,
    _lang: u16,
    _catid: *const u8,
    clsid: *mut u8,
    profile: *mut u8,
) -> i32 {
    if clsid.is_null() || profile.is_null() {
        return E_INVALIDARG;
    }
    std::ptr::write_bytes(clsid, 0, 16);
    std::ptr::write_bytes(profile, 0, 16);
    S_FALSE
}
unsafe extern "win64" fn p_set_default_language_profile(
    _this: usize,
    _lang: u16,
    _clsid: *const u8,
    _profile: *const u8,
) -> i32 {
    E_FAIL
}
unsafe extern "win64" fn p_activate_language_profile(
    _this: usize,
    _clsid: *const u8,
    _lang: u16,
    _profile: *const u8,
) -> i32 {
    E_INVALIDARG
}
/// GetActiveLanguageProfile(rclsid, plangid, pguidProfile) : la langue
/// courante, sans profil de TIP.
unsafe extern "win64" fn p_get_active_language_profile(
    _this: usize,
    _clsid: *const u8,
    lang: *mut u16,
    profile: *mut u8,
) -> i32 {
    if lang.is_null() || profile.is_null() {
        return E_INVALIDARG;
    }
    *lang = langid();
    std::ptr::write_bytes(profile, 0, 16);
    S_FALSE
}
unsafe extern "win64" fn p_get_language_profile_description(
    _this: usize,
    _clsid: *const u8,
    _lang: u16,
    _profile: *const u8,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    E_FAIL
}
/// GetCurrentLanguage(plangid).
unsafe extern "win64" fn p_get_current_language(_this: usize, lang: *mut u16) -> i32 {
    if lang.is_null() {
        return E_INVALIDARG;
    }
    *lang = langid();
    S_OK
}
/// ChangeCurrentLanguage(langid) : une seule langue de saisie.
unsafe extern "win64" fn p_change_current_language(_this: usize, lang: u16) -> i32 {
    if lang == langid() {
        S_OK
    } else {
        E_FAIL
    }
}
/// GetLanguageList(ppLangId, pulCount) : tableau CoTaskMemAlloc d'une langue.
unsafe extern "win64" fn p_get_language_list(
    _this: usize,
    list: *mut usize,
    count: *mut u32,
) -> i32 {
    if list.is_null() || count.is_null() {
        return E_INVALIDARG;
    }
    let p = super::ole32::co_task_mem_alloc(2) as *mut u16;
    if p.is_null() {
        *list = 0;
        *count = 0;
        return E_OUTOFMEMORY;
    }
    *p = langid();
    *list = p as usize;
    *count = 1;
    S_OK
}
/// EnumLanguageProfiles(langid, ppEnum) : aucun profil de TIP.
unsafe extern "win64" fn p_enum_language_profiles(
    _this: usize,
    _lang: u16,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = new_enum(EnumKind::Language, 0);
    S_OK
}
unsafe extern "win64" fn p_enable_language_profile(
    _this: usize,
    _clsid: *const u8,
    _lang: u16,
    _profile: *const u8,
    _enable: i32,
) -> i32 {
    E_INVALIDARG
}
unsafe extern "win64" fn p_is_enabled_language_profile(
    _this: usize,
    _clsid: *const u8,
    _lang: u16,
    _profile: *const u8,
    enabled: *mut i32,
) -> i32 {
    if enabled.is_null() {
        return E_INVALIDARG;
    }
    *enabled = 0;
    E_INVALIDARG
}
unsafe extern "win64" fn p_substitute_keyboard_layout(
    _this: usize,
    _clsid: *const u8,
    _lang: u16,
    _profile: *const u8,
    _hkl: usize,
) -> i32 {
    E_NOTIMPL
}

static PROFILES_VTBL: VTable<19> = VTable([
    p_qi as *const (),
    p_addref as *const (),
    p_release as *const (),
    not_impl_1 as *const (), // Register
    not_impl_1 as *const (), // Unregister
    not_impl_8 as *const (), // AddLanguageProfile
    not_impl_3 as *const (), // RemoveLanguageProfile
    p_enum_input_processor_info as *const (),
    p_get_default_language_profile as *const (),
    p_set_default_language_profile as *const (),
    p_activate_language_profile as *const (),
    p_get_active_language_profile as *const (),
    p_get_language_profile_description as *const (),
    p_get_current_language as *const (),
    p_change_current_language as *const (),
    p_get_language_list as *const (),
    p_enum_language_profiles as *const (),
    p_enable_language_profile as *const (),
    p_is_enabled_language_profile as *const (),
]);
// EnableLanguageProfileByDefault et SubstituteKeyboardLayout suivent.
static PROFILES_VTBL_TAIL: VTable<2> = VTable([
    p_enable_language_profile as *const (),
    p_substitute_keyboard_layout as *const (),
]);

/// ActivateProfile(type, langid, clsid, guidProfile, hkl, flags) : seule la
/// disposition courante existe.
unsafe extern "win64" fn m_activate_profile(
    _this: usize,
    kind: u32,
    _lang: u16,
    _clsid: *const u8,
    _profile: *const u8,
    hkl_wanted: usize,
    _flags: u32,
) -> i32 {
    if kind == TF_PROFILETYPE_KEYBOARDLAYOUT && (hkl_wanted == 0 || hkl_wanted == hkl()) {
        S_OK
    } else {
        E_INVALIDARG
    }
}
/// GetProfile(type, langid, clsid, guidProfile, hkl, pProfile).
unsafe extern "win64" fn m_get_profile(
    _this: usize,
    kind: u32,
    _lang: u16,
    _clsid: *const u8,
    _profile: *const u8,
    _hkl: usize,
    out: *mut u8,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    if kind != TF_PROFILETYPE_KEYBOARDLAYOUT {
        std::ptr::write_bytes(out, 0, PROFILE_SIZE);
        return S_FALSE;
    }
    fill_profile(out);
    S_OK
}
/// EnumProfiles(langid, ppEnum) : la disposition de clavier courante.
unsafe extern "win64" fn m_enum_profiles(_this: usize, _lang: u16, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = new_enum(EnumKind::Profiles, 0);
    S_OK
}
unsafe extern "win64" fn m_release_input_processor(
    _this: usize,
    _clsid: *const u8,
    _f: u32,
) -> i32 {
    S_OK
}
/// GetActiveProfile(catid, pProfile) : la disposition de clavier pour la
/// catégorie clavier, rien pour les autres.
unsafe extern "win64" fn m_get_active_profile(_this: usize, catid: *const u8, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match read_guid(catid) {
        Some(g) if g == GUID_TFCAT_TIP_KEYBOARD => {
            fill_profile(out);
            S_OK
        }
        _ => {
            std::ptr::write_bytes(out, 0, PROFILE_SIZE);
            S_FALSE
        }
    }
}

static MGR_VTBL: VTable<11> = VTable([
    m_qi as *const (),
    m_addref as *const (),
    m_release as *const (),
    m_activate_profile as *const (),
    not_impl_5 as *const (), // DeactivateProfile
    m_get_profile as *const (),
    m_enum_profiles as *const (),
    m_release_input_processor as *const (),
    not_impl_10 as *const (), // RegisterProfile
    not_impl_3 as *const (),  // UnregisterProfile
    m_get_active_profile as *const (),
]);

/// Table complète d'ITfInputProcessorProfiles (18 méthodes après les trois
/// d'IUnknown), construite une fois.
fn profiles_vtbl() -> *const () {
    static TABLE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *TABLE.get_or_init(|| {
        let t: Vec<*const ()> = PROFILES_VTBL
            .0
            .iter()
            .chain(PROFILES_VTBL_TAIL.0.iter())
            .copied()
            .collect();
        Box::leak(t.into_boxed_slice()).as_ptr() as usize
    }) as *const ()
}

fn new_profiles() -> usize {
    Box::into_raw(Box::new(Profiles {
        vt_profiles: profiles_vtbl(),
        vt_mgr: MGR_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
    })) as usize
}

// ---------------------------------------------------------------------
// Énumérateurs : IEnumTfInputProcessorProfiles (une disposition) et
// IEnumTfLanguageProfiles (vide).
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum EnumKind {
    Profiles,
    Language,
}

#[repr(C)]
struct Enum {
    vtbl: *const (),
    refs: AtomicU32,
    kind: EnumKind,
    pos: AtomicU32,
}

impl EnumKind {
    fn count(self) -> u32 {
        match self {
            EnumKind::Profiles => 1,
            EnumKind::Language => 0,
        }
    }
}

fn new_enum(kind: EnumKind, pos: u32) -> usize {
    Box::into_raw(Box::new(Enum {
        vtbl: ENUM_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        kind,
        pos: AtomicU32::new(pos),
    })) as usize
}

unsafe fn en<'a>(this: usize) -> &'a Enum {
    &*(this as *const Enum)
}

unsafe extern "win64" fn e_qi(this: usize, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let want = match en(this).kind {
        EnumKind::Profiles => IID_IENUM_TF_INPUT_PROCESSOR_PROFILES,
        EnumKind::Language => IID_IENUM_TF_LANGUAGE_PROFILES,
    };
    match read_guid(riid) {
        Some(g) if g == IID_IUNKNOWN || g == want => {
            en(this).refs.fetch_add(1, Ordering::Relaxed);
            *out = this as *mut c_void;
            S_OK
        }
        _ => {
            log_no_interface("IEnumTf…Profiles", riid);
            E_NOINTERFACE
        }
    }
}
unsafe extern "win64" fn e_addref(this: usize) -> u32 {
    en(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn e_release(this: usize) -> u32 {
    let n = en(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut Enum));
    }
    n
}
unsafe extern "win64" fn e_clone(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let e = en(this);
    *out = new_enum(e.kind, e.pos.load(Ordering::Relaxed));
    S_OK
}
/// Next(ulCount, rgElements, pcFetched).
unsafe extern "win64" fn e_next(this: usize, count: u32, items: *mut u8, fetched: *mut u32) -> i32 {
    let e = en(this);
    let mut n = 0u32;
    while n < count && e.pos.load(Ordering::Relaxed) < e.kind.count() {
        if items.is_null() {
            return E_INVALIDARG;
        }
        // Seul l'énumérateur de dispositions a des éléments.
        fill_profile(items.add(n as usize * PROFILE_SIZE));
        e.pos.fetch_add(1, Ordering::Relaxed);
        n += 1;
    }
    if !fetched.is_null() {
        *fetched = n;
    }
    if n == count {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn e_reset(this: usize) -> i32 {
    en(this).pos.store(0, Ordering::Relaxed);
    S_OK
}
unsafe extern "win64" fn e_skip(this: usize, count: u32) -> i32 {
    let e = en(this);
    let pos = e.pos.load(Ordering::Relaxed);
    let next = pos.saturating_add(count).min(e.kind.count());
    e.pos.store(next, Ordering::Relaxed);
    if next - pos == count {
        S_OK
    } else {
        S_FALSE
    }
}

static ENUM_VTBL: VTable<7> = VTable([
    e_qi as *const (),
    e_addref as *const (),
    e_release as *const (),
    e_clone as *const (),
    e_next as *const (),
    e_reset as *const (),
    e_skip as *const (),
]);

/// Classe servie par CoCreateInstance.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    if *clsid != CLSID_TF_INPUT_PROCESSOR_PROFILES {
        return None;
    }
    if out.is_null() {
        return Some(E_POINTER);
    }
    let obj = new_profiles();
    let hr = qi(obj, iid as *const _ as *const u8, out);
    release(obj);
    Some(hr)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call<F: Copy>(obj: usize, index: usize) -> F {
        let vt = *(obj as *const *const usize);
        std::mem::transmute_copy(&*vt.add(index))
    }

    #[test]
    fn profils_de_saisie_et_profil_actif() {
        unsafe {
            let clsid = &*(&CLSID_TF_INPUT_PROCESSOR_PROFILES as *const Guid
                as *const super::super::ole32::Guid);
            let iid = &*(&IID_ITF_INPUT_PROCESSOR_PROFILES as *const Guid
                as *const super::super::ole32::Guid);
            let mut p: *mut c_void = std::ptr::null_mut();
            assert_eq!(create_builtin(clsid, iid, &mut p), Some(S_OK));
            let p = p as usize;
            // GetCurrentLanguage (n° 13).
            let mut lang = 0u16;
            let f: unsafe extern "win64" fn(usize, *mut u16) -> i32 = call(p, 13);
            assert_eq!(f(p, &mut lang), S_OK);
            assert_eq!(lang, langid());
            // QI vers ITfInputProcessorProfileMgr puis GetActiveProfile (n° 10).
            let mut m: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                qi(p, IID_ITF_INPUT_PROCESSOR_PROFILE_MGR.as_ptr(), &mut m),
                S_OK
            );
            let m = m as usize;
            let mut prof = [0u8; PROFILE_SIZE];
            let g: unsafe extern "win64" fn(usize, *const u8, *mut u8) -> i32 = call(m, 10);
            assert_eq!(
                g(m, GUID_TFCAT_TIP_KEYBOARD.as_ptr(), prof.as_mut_ptr()),
                S_OK
            );
            assert_eq!(
                u32::from_le_bytes(prof[0..4].try_into().unwrap()),
                TF_PROFILETYPE_KEYBOARDLAYOUT
            );
            assert_eq!(u16::from_le_bytes([prof[4], prof[5]]), lang);
            assert_eq!(prof[40..56], GUID_TFCAT_TIP_KEYBOARD);
            // EnumProfiles (n° 6) : une disposition, puis S_FALSE.
            let mut e = 0usize;
            let h: unsafe extern "win64" fn(usize, u16, *mut usize) -> i32 = call(m, 6);
            assert_eq!(h(m, 0, &mut e), S_OK);
            let next: unsafe extern "win64" fn(usize, u32, *mut u8, *mut u32) -> i32 = call(e, 4);
            let mut got = 0u32;
            assert_eq!(next(e, 1, prof.as_mut_ptr(), &mut got), S_OK);
            assert_eq!(got, 1);
            assert_eq!(next(e, 1, prof.as_mut_ptr(), &mut got), S_FALSE);
            assert_eq!(got, 0);
            e_release(e);
            // Le Release de la face Mgr et celui de la face principale
            // libèrent l'objet sans double libération.
            assert_eq!(m_release(m), 1);
            assert_eq!(p_release(p), 0);
            // Table complète : 21 méthodes, SubstituteKeyboardLayout en dernier.
            let vt = profiles_vtbl() as *const usize;
            assert_eq!(
                *vt.add(20),
                p_substitute_keyboard_layout as *const () as usize
            );
        }
    }
}
