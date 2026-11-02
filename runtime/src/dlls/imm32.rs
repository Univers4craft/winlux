//! imm32.dll : gestionnaire des méthodes de saisie (IME) de Windows.
//!
//! Importée par presque tous les logiciels qui ont un champ de texte (PuTTY,
//! Qt, wxWidgets…), même en français. Winlux ne propose pas encore de
//! méthode de saisie Windows : chaque fenêtre a un contexte, toujours fermé
//! et sans composition en cours. Le texte tapé arrive donc normalement par
//! WM_CHAR, comme sous Windows quand aucune IME n'est active.

/// Contexte d'entrée unique (HIMC) : non nul pour que les programmes qui
/// le testent poursuivent normalement.
const HIMC: usize = 0x1C0_0001;

pub extern "win64" fn imm_get_context(_hwnd: usize) -> usize {
    HIMC
}
pub extern "win64" fn imm_release_context(_hwnd: usize, _himc: usize) -> i32 {
    1
}
pub extern "win64" fn imm_create_context() -> usize {
    HIMC
}
pub extern "win64" fn imm_destroy_context(_himc: usize) -> i32 {
    1
}
pub extern "win64" fn imm_associate_context(_hwnd: usize, _himc: usize) -> usize {
    HIMC
}
pub extern "win64" fn imm_associate_context_ex(_hwnd: usize, _himc: usize, _flags: u32) -> i32 {
    1
}
/// ImmGetCompositionString : aucune composition en cours (0 octet).
pub extern "win64" fn imm_get_composition_string(
    _himc: usize,
    _index: u32,
    _buf: *mut u8,
    _len: u32,
) -> i32 {
    0
}
pub extern "win64" fn imm_set_composition_string(
    _himc: usize,
    _index: u32,
    _comp: *const u8,
    _comp_len: u32,
    _read: *const u8,
    _read_len: u32,
) -> i32 {
    0
}
/// Police, position de la composition et des candidats : retenues sans
/// effet, puisque rien n'est affiché.
pub extern "win64" fn imm_set_hint(_himc: usize, _value: *const u8) -> i32 {
    1
}
pub extern "win64" fn imm_get_hint(_himc: usize, _out: *mut u8) -> i32 {
    0
}
pub extern "win64" fn imm_get_candidate_window(_himc: usize, _index: u32, _out: *mut u8) -> i32 {
    0
}
pub extern "win64" fn imm_set_candidate_window(_himc: usize, _form: *const u8) -> i32 {
    1
}
pub extern "win64" fn imm_get_open_status(_himc: usize) -> i32 {
    0
}
pub extern "win64" fn imm_set_open_status(_himc: usize, open: i32) -> i32 {
    // Ouvrir une IME qui n'existe pas échoue ; la fermer réussit.
    (open == 0) as i32
}
pub unsafe extern "win64" fn imm_get_conversion_status(
    _himc: usize,
    conversion: *mut u32,
    sentence: *mut u32,
) -> i32 {
    if !conversion.is_null() {
        *conversion = 0; // IME_CMODE_ALPHANUMERIC
    }
    if !sentence.is_null() {
        *sentence = 0;
    }
    1
}
pub extern "win64" fn imm_set_conversion_status(
    _himc: usize,
    _conversion: u32,
    _sentence: u32,
) -> i32 {
    1
}
pub extern "win64" fn imm_notify_ime(_himc: usize, _action: u32, _index: u32, _value: u32) -> i32 {
    0
}
pub extern "win64" fn imm_is_ime(_hkl: usize) -> i32 {
    0
}
pub extern "win64" fn imm_get_default_ime_wnd(_hwnd: usize) -> usize {
    0
}
/// ImmGetVirtualKey : la touche n'a pas été prise par une IME.
pub extern "win64" fn imm_get_virtual_key(_hwnd: usize) -> u32 {
    0
}
pub extern "win64" fn imm_disable_ime(_thread: u32) -> i32 {
    1
}
pub extern "win64" fn imm_disable_text_frame_service(_thread: u32) -> i32 {
    1
}
pub extern "win64" fn imm_get_text(_hkl: usize, _buf: *mut u8, _len: u32) -> u32 {
    0
}
pub extern "win64" fn imm_escape(_hkl: usize, _himc: usize, _esc: u32, _data: *mut u8) -> isize {
    0
}
pub extern "win64" fn imm_get_property(_hkl: usize, _index: u32) -> u32 {
    0
}
pub extern "win64" fn imm_get_candidate_list(
    _himc: usize,
    _index: u32,
    _out: *mut u8,
    _len: u32,
) -> u32 {
    0
}
pub unsafe extern "win64" fn imm_get_candidate_list_count(_himc: usize, count: *mut u32) -> u32 {
    if !count.is_null() {
        *count = 0;
    }
    0
}
pub extern "win64" fn imm_lock_imc(_himc: usize) -> usize {
    0
}
pub extern "win64" fn imm_unlock_imc(_himc: usize) -> i32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "ImmGetContext" => fn imm_get_context,
        "ImmReleaseContext" => fn imm_release_context,
        "ImmCreateContext" => fn imm_create_context,
        "ImmDestroyContext" => fn imm_destroy_context,
        "ImmAssociateContext" => fn imm_associate_context,
        "ImmAssociateContextEx" => fn imm_associate_context_ex,
        "ImmGetCompositionStringA" => fn imm_get_composition_string,
        "ImmGetCompositionStringW" => fn imm_get_composition_string,
        "ImmSetCompositionStringA" => fn imm_set_composition_string,
        "ImmSetCompositionStringW" => fn imm_set_composition_string,
        "ImmSetCompositionFontA" => fn imm_set_hint,
        "ImmSetCompositionFontW" => fn imm_set_hint,
        "ImmGetCompositionFontA" => fn imm_get_hint,
        "ImmGetCompositionFontW" => fn imm_get_hint,
        "ImmSetCompositionWindow" => fn imm_set_hint,
        "ImmGetCompositionWindow" => fn imm_get_hint,
        "ImmSetStatusWindowPos" => fn imm_set_hint,
        "ImmGetStatusWindowPos" => fn imm_get_hint,
        "ImmGetCandidateWindow" => fn imm_get_candidate_window,
        "ImmSetCandidateWindow" => fn imm_set_candidate_window,
        "ImmGetOpenStatus" => fn imm_get_open_status,
        "ImmSetOpenStatus" => fn imm_set_open_status,
        "ImmGetConversionStatus" => fn imm_get_conversion_status,
        "ImmSetConversionStatus" => fn imm_set_conversion_status,
        "ImmNotifyIME" => fn imm_notify_ime,
        "ImmIsIME" => fn imm_is_ime,
        "ImmGetDefaultIMEWnd" => fn imm_get_default_ime_wnd,
        "ImmGetVirtualKey" => fn imm_get_virtual_key,
        "ImmDisableIME" => fn imm_disable_ime,
        "ImmDisableTextFrameService" => fn imm_disable_text_frame_service,
        "ImmGetIMEFileNameA" => fn imm_get_text,
        "ImmGetIMEFileNameW" => fn imm_get_text,
        "ImmGetDescriptionA" => fn imm_get_text,
        "ImmGetDescriptionW" => fn imm_get_text,
        "ImmEscapeA" => fn imm_escape,
        "ImmEscapeW" => fn imm_escape,
        "ImmGetProperty" => fn imm_get_property,
        "ImmGetCandidateListA" => fn imm_get_candidate_list,
        "ImmGetCandidateListW" => fn imm_get_candidate_list,
        "ImmGetCandidateListCountA" => fn imm_get_candidate_list_count,
        "ImmGetCandidateListCountW" => fn imm_get_candidate_list_count,
        "ImmLockIMC" => fn imm_lock_imc,
        "ImmUnlockIMC" => fn imm_unlock_imc,
    }
}
