//! Fonctions natives demandées par les DLL de WPF livrées avec une appli .NET
//! autonome (`wpfgfx_cor3.dll`, `PresentationNative_cor3.dll`), #268.
//!
//! - ntdll : bitmaps `RTL_BITMAP` (`RtlInitializeBitMap`, `RtlSetBits`…),
//!   `DbgPrintEx`, `DbgPrompt` ;
//! - advapi32 : traçage ETW « classique » (`RegisterTraceGuidsW`,
//!   `TraceEvent`…), sans session de trace : rien n'est jamais activé ;
//!   `AllocateLocallyUniqueId` ;
//! - gdi32 : `GetRandomRgn` ;
//! - ole32 : `PropVariantCopy` ;
//! - user32 : `GetGuiResources`.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// ntdll : RTL_BITMAP { ULONG SizeOfBitMap; PULONG Buffer; }
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct RtlBitmap {
    size: u32,
    buffer: *mut u32,
}

unsafe fn bit(map: &RtlBitmap, i: u32) -> bool {
    *map.buffer.add((i / 32) as usize) & (1 << (i % 32)) != 0
}

unsafe fn set_bit(map: &RtlBitmap, i: u32, on: bool) {
    let word = map.buffer.add((i / 32) as usize);
    if on {
        *word |= 1 << (i % 32);
    } else {
        *word &= !(1 << (i % 32));
    }
}

pub unsafe extern "win64" fn rtl_initialize_bitmap(
    map: *mut RtlBitmap,
    buffer: *mut u32,
    size: u32,
) {
    if map.is_null() {
        return;
    }
    (*map).size = size;
    (*map).buffer = buffer;
}

unsafe fn fill_bits(map: *const RtlBitmap, start: u32, count: u32, on: bool) {
    if map.is_null() || (*map).buffer.is_null() {
        return;
    }
    let m = &*map;
    let end = start.saturating_add(count).min(m.size);
    for i in start..end {
        set_bit(m, i, on);
    }
}

pub unsafe extern "win64" fn rtl_set_bits(map: *const RtlBitmap, start: u32, count: u32) {
    fill_bits(map, start, count, true);
}

pub unsafe extern "win64" fn rtl_clear_bits(map: *const RtlBitmap, start: u32, count: u32) {
    fill_bits(map, start, count, false);
}

pub unsafe extern "win64" fn rtl_set_all_bits(map: *const RtlBitmap) {
    if !map.is_null() {
        fill_bits(map, 0, (*map).size, true);
    }
}

pub unsafe extern "win64" fn rtl_clear_all_bits(map: *const RtlBitmap) {
    if !map.is_null() {
        fill_bits(map, 0, (*map).size, false);
    }
}

/// Premier indice d'une suite de `count` bits à `want`, en commençant à
/// `hint` puis en reprenant au début ; `u32::MAX` (0xFFFFFFFF) sinon.
unsafe fn find_run(map: *const RtlBitmap, count: u32, hint: u32, want: bool) -> u32 {
    if map.is_null() || (*map).buffer.is_null() {
        return u32::MAX;
    }
    let m = &*map;
    if count == 0 {
        return if hint < m.size { hint } else { 0 };
    }
    if count > m.size {
        return u32::MAX;
    }
    let last = m.size - count;
    let hint = if hint > last { 0 } else { hint };
    let starts = (hint..=last).chain(0..hint);
    for s in starts {
        if (s..s + count).all(|i| bit(m, i) == want) {
            return s;
        }
    }
    u32::MAX
}

pub unsafe extern "win64" fn rtl_find_clear_bits(
    map: *const RtlBitmap,
    count: u32,
    hint: u32,
) -> u32 {
    find_run(map, count, hint, false)
}

pub unsafe extern "win64" fn rtl_find_set_bits(
    map: *const RtlBitmap,
    count: u32,
    hint: u32,
) -> u32 {
    find_run(map, count, hint, true)
}

pub unsafe extern "win64" fn rtl_find_clear_bits_and_set(
    map: *const RtlBitmap,
    count: u32,
    hint: u32,
) -> u32 {
    let at = find_run(map, count, hint, false);
    if at != u32::MAX {
        fill_bits(map, at, count, true);
    }
    at
}

pub unsafe extern "win64" fn rtl_find_set_bits_and_clear(
    map: *const RtlBitmap,
    count: u32,
    hint: u32,
) -> u32 {
    let at = find_run(map, count, hint, true);
    if at != u32::MAX {
        fill_bits(map, at, count, false);
    }
    at
}

unsafe fn all_bits(map: *const RtlBitmap, start: u32, count: u32, want: bool) -> u8 {
    if map.is_null() || (*map).buffer.is_null() {
        return 0;
    }
    let m = &*map;
    match start.checked_add(count) {
        Some(end) if end <= m.size => u8::from((start..end).all(|i| bit(m, i) == want)),
        _ => 0,
    }
}

pub unsafe extern "win64" fn rtl_are_bits_set(map: *const RtlBitmap, start: u32, count: u32) -> u8 {
    all_bits(map, start, count, true)
}

pub unsafe extern "win64" fn rtl_are_bits_clear(
    map: *const RtlBitmap,
    start: u32,
    count: u32,
) -> u8 {
    all_bits(map, start, count, false)
}

pub unsafe extern "win64" fn rtl_number_of_set_bits(map: *const RtlBitmap) -> u32 {
    if map.is_null() || (*map).buffer.is_null() {
        return 0;
    }
    let m = &*map;
    (0..m.size).filter(|&i| bit(m, i)).count() as u32
}

/// DbgPrintEx : message de débogage, écarté sans débogueur (STATUS_SUCCESS).
pub extern "win64" fn dbg_print_ex(_component: u32, _level: u32, _format: *const u8) -> u32 {
    0
}

/// DbgPrompt : aucun débogueur noyau pour répondre, réponse vide.
pub extern "win64" fn dbg_prompt(_prompt: *const u8, _response: *mut u8, _len: u32) -> u32 {
    0
}

/// RtlVerifyVersionInfo : VerifyVersionInfoW en NTSTATUS (0, ou
/// STATUS_REVISION_MISMATCH quand la condition n'est pas remplie).
/// wpfgfx s'en sert pour choisir son chemin de rendu.
pub unsafe extern "win64" fn rtl_verify_version_info(
    info: *const u8,
    type_mask: u32,
    mask: u64,
) -> u32 {
    const STATUS_INVALID_PARAMETER: u32 = 0xC000_000D;
    const STATUS_REVISION_MISMATCH: u32 = 0xC000_0059;
    if info.is_null() || type_mask == 0 {
        return STATUS_INVALID_PARAMETER;
    }
    if crate::dlls::kernel32_ext::verify_version_info_w(info, type_mask, mask) != 0 {
        0
    } else {
        STATUS_REVISION_MISMATCH
    }
}

// ---------------------------------------------------------------------------
// advapi32 : traçage ETW classique, toujours désactivé
// ---------------------------------------------------------------------------

static NEXT_TRACE: AtomicU64 = AtomicU64::new(0x5741_0001);

/// RegisterTraceGuidsW : enregistre le fournisseur, sans jamais l'activer
/// (le rappel n'est pas appelé, aucune session n'existe).
pub unsafe extern "win64" fn register_trace_guids_w(
    _callback: usize,
    _context: usize,
    _control_guid: *const u8,
    _count: u32,
    _guids: usize,
    _mof_path: *const u16,
    _mof_resource: *const u16,
    handle: *mut u64,
) -> u32 {
    if handle.is_null() {
        return 87;
    }
    *handle = NEXT_TRACE.fetch_add(1, Ordering::Relaxed);
    0
}

pub unsafe extern "win64" fn register_trace_guids_a(
    callback: usize,
    context: usize,
    control_guid: *const u8,
    count: u32,
    guids: usize,
    _mof_path: *const u8,
    _mof_resource: *const u8,
    handle: *mut u64,
) -> u32 {
    register_trace_guids_w(
        callback,
        context,
        control_guid,
        count,
        guids,
        std::ptr::null(),
        std::ptr::null(),
        handle,
    )
}

/// EventWrite : aucune session ETW n'écoute, l'événement est écarté.
pub extern "win64" fn event_write(
    _handle: u64,
    _descriptor: usize,
    _count: u32,
    _data: usize,
) -> u32 {
    0
}

pub extern "win64" fn event_write_string(
    _handle: u64,
    _level: u8,
    _keyword: u64,
    _text: *const u16,
) -> u32 {
    0
}

pub extern "win64" fn unregister_trace_guids(_handle: u64) -> u32 {
    0
}

/// TraceEvent / TraceMessage sur une session inexistante : ERROR_INVALID_HANDLE.
pub extern "win64" fn trace_event(_session: u64, _header: usize) -> u32 {
    6
}

pub extern "win64" fn trace_message(_session: u64, _flags: u32, _guid: usize, _number: u16) -> u32 {
    6
}

/// GetTraceLoggerHandle : appelé depuis le rappel d'activation, jamais
/// appelé ici ; 0 (INVALID_PROCESSTRACE_HANDLE côté appelant) avec l'erreur.
pub extern "win64" fn get_trace_logger_handle(_buffer: usize) -> u64 {
    crate::process::set_last_error(6);
    0
}

pub extern "win64" fn get_trace_enable_flags(_session: u64) -> u32 {
    0
}

pub extern "win64" fn get_trace_enable_level(_session: u64) -> u8 {
    0
}

/// AllocateLocallyUniqueId : LUID unique pour la durée du processus.
pub unsafe extern "win64" fn allocate_locally_unique_id(luid: *mut u64) -> i32 {
    static NEXT: AtomicU64 = AtomicU64::new(0x1_0000);
    if luid.is_null() {
        crate::process::set_last_error(998);
        return 0;
    }
    luid.write_unaligned(NEXT.fetch_add(1, Ordering::Relaxed));
    1
}

// ---------------------------------------------------------------------------
// gdi32 : GetRandomRgn
// ---------------------------------------------------------------------------

/// GetRandomRgn(hdc, hrgn, SYSRGN) : zone visible de la fenêtre du DC, en
/// coordonnées écran. 1 si la région est remplie, 0 si le DC n'a pas de
/// fenêtre (DC mémoire), -1 en cas d'erreur.
pub unsafe extern "win64" fn get_random_rgn(hdc: usize, rgn: usize, which: i32) -> i32 {
    const SYSRGN: i32 = 4;
    if which != SYSRGN || rgn == 0 {
        return -1;
    }
    let Some(hwnd) = crate::dlls::gui::dc_window(hdc) else {
        return -1;
    };
    if hwnd == 0 {
        return 0;
    }
    let mut r = crate::dlls::gui::Rect::default();
    if crate::dlls::user32::get_window_rect(hwnd, &mut r) == 0 {
        return -1;
    }
    crate::dlls::gdi32::set_rect_rgn(rgn, r.left, r.top, r.right, r.bottom);
    1
}

/// GdiEntry13 (DdQueryDisplaySettingsUniqueness) : compteur qui change à
/// chaque changement de mode d'écran. wpfgfx l'obtient par GetProcAddress et
/// l'appelle sans vérifier le pointeur : son absence plantait le rendu.
pub extern "win64" fn dd_query_display_settings_uniqueness() -> u32 {
    // Dérivé de la taille de l'écran : il change quand le mode change.
    let (w, h) = crate::dlls::gui::screen_size();
    ((w as u32) << 16 ^ h as u32) | 1
}

// ---------------------------------------------------------------------------
// ole32 : PropVariantCopy
// ---------------------------------------------------------------------------

/// PropVariantCopy : copie profonde. Les chaînes VT_LPSTR / VT_LPWSTR
/// (propres aux PROPVARIANT) sont dupliquées par CoTaskMemAlloc ; le reste
/// suit VariantCopy.
pub unsafe extern "win64" fn prop_variant_copy(dest: *mut u8, src: *const u8) -> i32 {
    const VT_LPSTR: u16 = 30;
    const VT_LPWSTR: u16 = 31;
    if dest.is_null() || src.is_null() {
        return 0x8007_0057u32 as i32; // E_INVALIDARG
    }
    let vt = src.cast::<u16>().read_unaligned();
    if vt == VT_LPSTR || vt == VT_LPWSTR {
        std::ptr::copy_nonoverlapping(src, dest, 24);
        let s = src.add(8).cast::<*const u8>().read_unaligned();
        if s.is_null() {
            return 0;
        }
        let unit = if vt == VT_LPWSTR { 2 } else { 1 };
        let mut len = 0usize;
        while (0..unit).any(|b| *s.add(len * unit + b) != 0) {
            len += 1;
        }
        let bytes = (len + 1) * unit;
        let copy = crate::dlls::ole32::co_task_mem_alloc(bytes).cast::<u8>();
        if copy.is_null() {
            return 0x8007_000Eu32 as i32; // E_OUTOFMEMORY
        }
        std::ptr::copy_nonoverlapping(s, copy, bytes);
        dest.add(8).cast::<*mut u8>().write_unaligned(copy);
        return 0;
    }
    crate::dlls::oleaut32::variant_copy(dest.cast::<c_void>(), src.cast::<c_void>())
}

// ---------------------------------------------------------------------------
// user32 : GetGuiResources
// ---------------------------------------------------------------------------

/// GetGuiResources : nombre d'objets GDI ou USER du processus. Winlux ne
/// les compte pas : une petite valeur plausible (WPF n'en fait qu'un
/// diagnostic).
pub extern "win64" fn get_gui_resources(_process: usize, flags: u32) -> u32 {
    match flags {
        0 | 2 => 64, // GR_GDIOBJECTS, GR_GDIOBJECTS_PEAK
        1 | 4 => 32, // GR_USEROBJECTS, GR_USEROBJECTS_PEAK
        _ => 0,
    }
}

/// ChangeWindowMessageFilter : pas de niveaux d'intégrité entre fenêtres
/// Winlux, tout message passe déjà.
pub extern "win64" fn change_window_message_filter(_message: u32, _flag: u32) -> i32 {
    1
}

pub fn ntdll_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RtlInitializeBitMap" => fn rtl_initialize_bitmap,
        "RtlSetBits" => fn rtl_set_bits,
        "RtlClearBits" => fn rtl_clear_bits,
        "RtlSetAllBits" => fn rtl_set_all_bits,
        "RtlClearAllBits" => fn rtl_clear_all_bits,
        "RtlFindClearBits" => fn rtl_find_clear_bits,
        "RtlFindSetBits" => fn rtl_find_set_bits,
        "RtlFindClearBitsAndSet" => fn rtl_find_clear_bits_and_set,
        "RtlFindSetBitsAndClear" => fn rtl_find_set_bits_and_clear,
        "RtlAreBitsSet" => fn rtl_are_bits_set,
        "RtlAreBitsClear" => fn rtl_are_bits_clear,
        "RtlNumberOfSetBits" => fn rtl_number_of_set_bits,
        "DbgPrintEx" => fn dbg_print_ex,
        "DbgPrompt" => fn dbg_prompt,
        "RtlVerifyVersionInfo" => fn rtl_verify_version_info,
    }
}

pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RegisterTraceGuidsW" => fn register_trace_guids_w,
        "RegisterTraceGuidsA" => fn register_trace_guids_a,
        "UnregisterTraceGuids" => fn unregister_trace_guids,
        "TraceEvent" => fn trace_event,
        "TraceMessage" => fn trace_message,
        "GetTraceLoggerHandle" => fn get_trace_logger_handle,
        "GetTraceEnableFlags" => fn get_trace_enable_flags,
        "GetTraceEnableLevel" => fn get_trace_enable_level,
        "AllocateLocallyUniqueId" => fn allocate_locally_unique_id,
        "EventWrite" => fn event_write,
        "EventWriteString" => fn event_write_string,
    }
}

pub fn gdi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetRandomRgn" => fn get_random_rgn,
        "GdiEntry13" => fn dd_query_display_settings_uniqueness,
    }
}

pub fn ole32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "PropVariantCopy" => fn prop_variant_copy,
    }
}

pub fn user32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetGuiResources" => fn get_gui_resources,
        "ChangeWindowMessageFilter" => fn change_window_message_filter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_rtl() {
        let mut words = [0u32; 2];
        let mut map = RtlBitmap {
            size: 0,
            buffer: std::ptr::null_mut(),
        };
        unsafe {
            rtl_initialize_bitmap(&mut map, words.as_mut_ptr(), 40);
            assert_eq!(rtl_find_clear_bits_and_set(&map, 3, 0), 0);
            assert_eq!(rtl_find_clear_bits_and_set(&map, 3, 0), 3);
            assert_eq!(rtl_number_of_set_bits(&map), 6);
            rtl_clear_bits(&map, 1, 1);
            assert_eq!(rtl_find_clear_bits(&map, 1, 0), 1);
            assert_eq!(rtl_find_clear_bits(&map, 2, 0), 6);
            assert_eq!(rtl_are_bits_set(&map, 3, 3), 1);
            rtl_set_bits(&map, 30, 4);
            assert_eq!(words[0] >> 30, 0b11);
            assert_eq!(words[1] & 0b11, 0b11);
            assert_eq!(rtl_find_clear_bits(&map, 41, 0), u32::MAX);
            rtl_set_all_bits(&map);
            assert_eq!(rtl_find_clear_bits_and_set(&map, 1, 5), u32::MAX);
        }
    }

    #[test]
    fn trace_classique_jamais_active() {
        let mut h = 0u64;
        unsafe {
            assert_eq!(
                register_trace_guids_w(
                    0,
                    0,
                    std::ptr::null(),
                    0,
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                    &mut h
                ),
                0
            );
        }
        assert_ne!(h, 0);
        assert_eq!(get_trace_enable_flags(h), 0);
        assert_eq!(trace_event(0, 0), 6);
        assert_eq!(unregister_trace_guids(h), 0);
        let (mut a, mut b) = (0u64, 0u64);
        unsafe {
            assert_eq!(allocate_locally_unique_id(&mut a), 1);
            assert_eq!(allocate_locally_unique_id(&mut b), 1);
        }
        assert_ne!(a, b);
    }

    #[test]
    fn copie_de_propvariant_chaine() {
        let s: Vec<u16> = "Bonjour".encode_utf16().chain([0]).collect();
        let mut src = [0u8; 24];
        src[0..2].copy_from_slice(&31u16.to_le_bytes());
        src[8..16].copy_from_slice(&(s.as_ptr() as usize).to_le_bytes());
        let mut dst = [0u8; 24];
        unsafe {
            assert_eq!(prop_variant_copy(dst.as_mut_ptr(), src.as_ptr()), 0);
            let p = usize::from_le_bytes(dst[8..16].try_into().unwrap()) as *const u16;
            assert_ne!(p, s.as_ptr());
            assert_eq!(crate::dlls::wstr(p), "Bonjour");
        }
    }
}
