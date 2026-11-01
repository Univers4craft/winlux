//! Compléments de kernel32 et d'advapi32 demandés au démarrage de coreclr
//! (runtime .NET embarqué par les applis autonomes) : état de la mémoire,
//! attentes alertables, notifications de ressources, fonctionnalités XSAVE
//! et jetons de thread.

use std::ffi::c_void;

use crate::process::set_last_error;

const ERROR_INVALID_PARAMETER: u32 = 87;

/// Lit `/proc/meminfo` : (MemTotal, MemAvailable, SwapTotal, SwapFree) en octets.
fn meminfo() -> (u64, u64, u64, u64) {
    let text = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    let total = field("MemTotal:").unwrap_or(4 << 30);
    let avail = field("MemAvailable:")
        .or_else(|| field("MemFree:"))
        .unwrap_or(total / 2);
    (
        total,
        avail.min(total),
        field("SwapTotal:").unwrap_or(0),
        field("SwapFree:").unwrap_or(0),
    )
}

/// Espace d'adressage utilisateur d'un processus Windows x64 (128 Tio).
const USER_VA: u64 = 0x7FFF_FFFE_FFFF;

#[repr(C)]
pub struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}

/// GlobalMemoryStatusEx : mémoire physique et fichier d'échange de la
/// machine (le « fichier d'échange » de Windows inclut la RAM).
pub unsafe extern "win64" fn global_memory_status_ex(out: *mut MemoryStatusEx) -> i32 {
    if out.is_null() || (*out).length as usize != std::mem::size_of::<MemoryStatusEx>() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (total, avail, swap_total, swap_free) = meminfo();
    let o = &mut *out;
    o.memory_load = ((total - avail) * 100).checked_div(total).unwrap_or(0) as u32;
    o.total_phys = total;
    o.avail_phys = avail;
    o.total_page_file = total + swap_total;
    o.avail_page_file = avail + swap_free;
    o.total_virtual = USER_VA;
    o.avail_virtual = USER_VA;
    o.avail_extended_virtual = 0;
    1
}

/// WaitForMultipleObjectsEx : sans file d'APC, l'attente alertable se
/// comporte comme l'attente ordinaire.
pub unsafe extern "win64" fn wait_for_multiple_objects_ex(
    count: u32,
    handles: *const usize,
    wait_all: i32,
    timeout: u32,
    _alertable: i32,
) -> u32 {
    crate::dlls::kernel32::wait_for_multiple_objects(count, handles, wait_all, timeout)
}

/// OpenEventW : variante large d'OpenEventA.
pub unsafe extern "win64" fn open_event_w(access: u32, inherit: i32, name: *const u16) -> usize {
    if name.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Ok(narrow) = std::ffi::CString::new(crate::dlls::wstr(name)) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    crate::dlls::kernel32::open_event_a(access, inherit, narrow.as_ptr().cast())
}

/// CreateMemoryResourceNotification : un événement que Winlux ne signale
/// pas (pas de surveillance de la pression mémoire pour l'instant) ;
/// QueryMemoryResourceNotification répond donc « état normal ».
pub unsafe extern "win64" fn create_memory_resource_notification(kind: u32) -> usize {
    if kind > 1 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    crate::dlls::kernel32::create_event_a(0, 1, 0, std::ptr::null())
}
pub unsafe extern "win64" fn query_memory_resource_notification(_h: usize, state: *mut i32) -> i32 {
    if state.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *state = 0;
    1
}

/// GetEnabledXStateFeatures : seuls x87 et SSE (XSTATE_MASK_LEGACY) sont
/// exposés dans les CONTEXT de Winlux ; l'état AVX n'est pas sauvegardé.
pub extern "win64" fn get_enabled_xstate_features() -> u64 {
    3
}

/// WerRegisterRuntimeExceptionModule : pas de rapport d'erreur Windows ;
/// l'enregistrement est accepté (S_OK) et sans effet.
pub extern "win64" fn wer_register_runtime_exception_module(
    _dll: *const u16,
    _ctx: *mut c_void,
) -> i32 {
    0
}

/// FlushProcessWriteBuffers : barrière mémoire sur tous les cœurs, via
/// membarrier(2) quand le noyau la propose.
pub extern "win64" fn flush_process_write_buffers() {
    const MEMBARRIER_CMD_GLOBAL: i64 = 1;
    let r = unsafe { libc::syscall(libc::SYS_membarrier, MEMBARRIER_CMD_GLOBAL, 0, 0) };
    if r != 0 {
        std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
    }
}

/// OpenThreadToken : un thread Winlux n'emprunte jamais d'identité, il n'a
/// donc pas de jeton propre (ERROR_NO_TOKEN), comme sous Windows.
pub unsafe extern "win64" fn open_thread_token(
    _thread: usize,
    _access: u32,
    _self: i32,
    token: *mut usize,
) -> i32 {
    if !token.is_null() {
        *token = 0;
    }
    set_last_error(1008);
    0
}

// --- Vérification de version ------------------------------------------------

/// Version annoncée par Winlux partout ailleurs (GetVersionEx, PEB) :
/// Windows 10 22H2, 10.0.19045, poste de travail.
const OS_VERSION: (u32, u32, u32) = (10, 0, 19045);

/// VerSetConditionMask : range la condition (3 bits) de chaque champ
/// VER_* demandé à la position 3 × (rang du bit).
pub extern "win64" fn ver_set_condition_mask(mask: u64, type_mask: u32, condition: u8) -> u64 {
    let mut out = mask;
    for bit in 0..8 {
        if type_mask & (1 << bit) != 0 {
            out &= !(7u64 << (bit * 3));
            out |= u64::from(condition & 7) << (bit * 3);
        }
    }
    out
}

fn compare(cond: u8, actual: u64, wanted: u64) -> bool {
    match cond {
        1 => actual == wanted,
        2 => actual > wanted,
        3 => actual >= wanted,
        4 => actual < wanted,
        5 => actual <= wanted,
        6 => actual & wanted == wanted,
        7 => actual & wanted != 0,
        _ => false,
    }
}

/// VerifyVersionInfoW/A : comme Windows, version majeure, mineure et
/// service pack se comparent ensemble (ordre lexicographique, condition du
/// premier champ demandé) ; build, plate-forme et type de produit à part.
/// Les deux structures OSVERSIONINFOEX ne diffèrent que par la taille de
/// szCSDVersion (128 octets ou 128 caractères larges).
unsafe fn verify_version(info: *const u8, csd_bytes: usize, type_mask: u32, mask: u64) -> i32 {
    const ERROR_OLD_WIN_VERSION: u32 = 1150;
    if info.is_null() || type_mask == 0 || mask == 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let u32_at = |o: usize| (info.add(o) as *const u32).read_unaligned();
    let u16_at = |o: usize| (info.add(o) as *const u16).read_unaligned();
    let tail = 20 + csd_bytes;
    let cond = |bit: u32| ((mask >> (bit * 3)) & 7) as u8;
    let (major, minor, build) = OS_VERSION;
    // (bit, valeur réelle, valeur demandée) dans l'ordre de priorité.
    let ordered = [
        (1, u64::from(major), u64::from(u32_at(4))),
        (0, u64::from(minor), u64::from(u32_at(8))),
        (5, 0, u64::from(u16_at(tail))),
        (4, 0, u64::from(u16_at(tail + 2))),
    ];
    let mut ok = true;
    if let Some(&(first, ..)) = ordered.iter().find(|(b, ..)| type_mask & (1 << b) != 0) {
        let c = cond(first);
        let mut decided = false;
        for &(bit, actual, wanted) in &ordered {
            if type_mask & (1 << bit) == 0 || actual == wanted {
                continue;
            }
            ok = compare(c, actual, wanted);
            decided = true;
            break;
        }
        if !decided {
            ok = matches!(c, 1 | 3 | 5 | 6);
        }
    }
    if ok && type_mask & 4 != 0 {
        ok = compare(cond(2), u64::from(build), u64::from(u32_at(12)));
    }
    if ok && type_mask & 8 != 0 {
        ok = compare(cond(3), 2, u64::from(u32_at(16)));
    }
    if ok && type_mask & 0x80 != 0 {
        ok = compare(cond(7), 1, u64::from(*info.add(tail + 6)));
    }
    if ok {
        1
    } else {
        set_last_error(ERROR_OLD_WIN_VERSION);
        0
    }
}
pub unsafe extern "win64" fn verify_version_info_w(
    info: *const u8,
    type_mask: u32,
    mask: u64,
) -> i32 {
    verify_version(info, 256, type_mask, mask)
}
pub unsafe extern "win64" fn verify_version_info_a(
    info: *const u8,
    type_mask: u32,
    mask: u64,
) -> i32 {
    verify_version(info, 128, type_mask, mask)
}

// --- CONTEXT étendus ----------------------------------------------------------

const CONTEXT_SIZE: usize = crate::dlls::seh::CONTEXT_SIZE;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
/// Bit CONTEXT_XSTATE (sans le bit d'architecture 0x100000).
const CONTEXT_XSTATE_BIT: u32 = 0x40;

/// InitializeContext2 : place un CONTEXT aligné sur 16 octets dans le tampon
/// fourni. Winlux n'expose que l'état x87/SSE (GetEnabledXStateFeatures),
/// qui tient dans le CONTEXT lui-même : la zone XSAVE étendue n'est donc
/// jamais réservée et le bit CONTEXT_XSTATE est retiré, ce que l'appelant
/// lit comme « pas d'état AVX ».
pub unsafe extern "win64" fn initialize_context2(
    buffer: *mut u8,
    flags: u32,
    context: *mut *mut u8,
    length: *mut u32,
    _compaction: u64,
) -> i32 {
    if length.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let needed = (CONTEXT_SIZE + 15) as u32;
    if buffer.is_null() || *length < needed {
        *length = needed;
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    let aligned = ((buffer as usize + 15) & !15) as *mut u8;
    std::ptr::write_bytes(aligned, 0, CONTEXT_SIZE);
    (aligned.add(0x30) as *mut u32).write(flags & !CONTEXT_XSTATE_BIT);
    if !context.is_null() {
        *context = aligned;
    }
    *length = needed;
    1
}
pub unsafe extern "win64" fn initialize_context(
    buffer: *mut u8,
    flags: u32,
    context: *mut *mut u8,
    length: *mut u32,
) -> i32 {
    initialize_context2(buffer, flags, context, length, 0)
}

/// CopyContext : copie le CONTEXT entier ; les drapeaux du résultat sont
/// ceux de la source restreints à ceux demandés.
pub unsafe extern "win64" fn copy_context(dest: *mut u8, flags: u32, src: *const u8) -> i32 {
    if dest.is_null() || src.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let src_flags = (src.add(0x30) as *const u32).read_unaligned();
    std::ptr::copy(src, dest, CONTEXT_SIZE);
    // Les bits d'architecture (0x100000) restent, seules les parties
    // communes aux deux jeux de drapeaux sont déclarées valides.
    (dest.add(0x30) as *mut u32)
        .write_unaligned(src_flags & flags & !CONTEXT_XSTATE_BIT | 0x10_0000);
    1
}

/// LocateXStateFeature : x87 (0) et SSE (1) vivent dans FltSave ; les
/// autres fonctionnalités (AVX…) ne sont pas présentes.
pub unsafe extern "win64" fn locate_xstate_feature(
    ctx: *mut u8,
    feature: u32,
    length: *mut u32,
) -> *mut u8 {
    let (offset, size) = match feature {
        0 => (0x100, 160),
        1 => (crate::dlls::seh::CTX_XMM0, 256),
        _ => return std::ptr::null_mut(),
    };
    if ctx.is_null() {
        return std::ptr::null_mut();
    }
    if !length.is_null() {
        *length = size;
    }
    ctx.add(offset)
}
pub unsafe extern "win64" fn set_xstate_features_mask(_ctx: *mut u8, mask: u64) -> i32 {
    if mask & !3 != 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    1
}
pub unsafe extern "win64" fn get_xstate_features_mask(ctx: *const u8, mask: *mut u64) -> i32 {
    if ctx.is_null() || mask.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *mask = 3;
    1
}

// --- Divers demandés par coreclr ------------------------------------------------

/// IsWow64Process2 : processus 64 bits natif (IMAGE_FILE_MACHINE_UNKNOWN)
/// sur une machine AMD64.
pub unsafe extern "win64" fn is_wow64_process2(
    _process: usize,
    process_machine: *mut u16,
    native: *mut u16,
) -> i32 {
    if process_machine.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *process_machine = 0;
    if !native.is_null() {
        *native = 0x8664;
    }
    1
}

/// SetThreadDescription : nomme le thread Linux (15 caractères au plus,
/// visible dans `top` et les vidages) quand il s'agit du thread courant.
pub unsafe extern "win64" fn set_thread_description(thread: usize, desc: *const u16) -> i32 {
    if desc.is_null() {
        return 0x8007_0057u32 as i32; // E_INVALIDARG
    }
    if thread == crate::dlls::kernel32::get_current_thread() {
        let name: String = crate::dlls::wstr(desc).chars().take(15).collect();
        if let Ok(c) = std::ffi::CString::new(name) {
            libc::prctl(libc::PR_SET_NAME, c.as_ptr());
        }
    }
    0
}

/// GetTempPath2W : même résultat que GetTempPathW pour un processus
/// ordinaire (seul SYSTEM obtient un dossier différent sous Windows).
pub unsafe extern "win64" fn get_temp_path2_w(size: u32, buf: *mut u16) -> u32 {
    crate::dlls::kernel32::get_temp_path_w(size, buf)
}

/// RtlDllShutdownInProgress : faux tant que le processus ne se termine pas.
pub extern "win64" fn rtl_dll_shutdown_in_progress() -> u8 {
    0
}

// Locales nommées (GetLocaleInfoEx, LocaleNameToLCID…) : voir crate::intl::api.

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GlobalMemoryStatusEx" => fn global_memory_status_ex,
        "WaitForMultipleObjectsEx" => fn wait_for_multiple_objects_ex,
        "OpenEventW" => fn open_event_w,
        "CreateMemoryResourceNotification" => fn create_memory_resource_notification,
        "QueryMemoryResourceNotification" => fn query_memory_resource_notification,
        "GetEnabledXStateFeatures" => fn get_enabled_xstate_features,
        "WerRegisterRuntimeExceptionModule" => fn wer_register_runtime_exception_module,
        "FlushProcessWriteBuffers" => fn flush_process_write_buffers,
        "VerSetConditionMask" => fn ver_set_condition_mask,
        "VerifyVersionInfoW" => fn verify_version_info_w,
        "VerifyVersionInfoA" => fn verify_version_info_a,
        "InitializeContext" => fn initialize_context,
        "InitializeContext2" => fn initialize_context2,
        "CopyContext" => fn copy_context,
        "LocateXStateFeature" => fn locate_xstate_feature,
        "SetXStateFeaturesMask" => fn set_xstate_features_mask,
        "GetXStateFeaturesMask" => fn get_xstate_features_mask,
        "IsWow64Process2" => fn is_wow64_process2,
        "SetThreadDescription" => fn set_thread_description,
        "GetTempPath2W" => fn get_temp_path2_w,
    }
}

pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "OpenThreadToken" => fn open_thread_token,
    }
}

pub fn ntdll_exports() -> crate::dlls::Exports {
    crate::exports! {
        "VerSetConditionMask" => fn ver_set_condition_mask,
        "RtlDllShutdownInProgress" => fn rtl_dll_shutdown_in_progress,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etat_de_la_memoire_coherent() {
        let mut m: MemoryStatusEx = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { global_memory_status_ex(&mut m) }, 0);
        m.length = std::mem::size_of::<MemoryStatusEx>() as u32;
        assert_eq!(unsafe { global_memory_status_ex(&mut m) }, 1);
        assert!(m.total_phys > 0);
        assert!(m.avail_phys <= m.total_phys);
        assert!(m.memory_load <= 100);
        assert!(m.total_page_file >= m.total_phys);
        assert_eq!(std::mem::size_of::<MemoryStatusEx>(), 64);
    }

    #[test]
    fn jeton_de_thread_absent() {
        let _teb = crate::process::test_teb();
        let mut t = 5usize;
        assert_eq!(unsafe { open_thread_token(0, 8, 1, &mut t) }, 0);
        assert_eq!(t, 0);
        assert_eq!(crate::process::last_error(), 1008);
        let mut state = 7;
        assert_eq!(
            unsafe { query_memory_resource_notification(0, &mut state) },
            1
        );
        assert_eq!(state, 0);
    }

    #[test]
    fn version_verifiee_comme_windows_10() {
        let _teb = crate::process::test_teb();
        // OSVERSIONINFOEXW : 284 octets.
        let mut info = [0u8; 284];
        info[..4].copy_from_slice(&284u32.to_le_bytes());
        info[4..8].copy_from_slice(&6u32.to_le_bytes()); // Windows 7 = 6.1
        info[8..12].copy_from_slice(&1u32.to_le_bytes());
        let mask = ver_set_condition_mask(ver_set_condition_mask(0, 2, 3), 1, 3);
        assert_eq!(mask, 3 << 3 | 3);
        unsafe {
            assert_eq!(verify_version_info_w(info.as_ptr(), 3, mask), 1);
            info[4..8].copy_from_slice(&11u32.to_le_bytes());
            assert_eq!(verify_version_info_w(info.as_ptr(), 3, mask), 0);
            assert_eq!(crate::process::last_error(), 1150);
            // Même version : « supérieur ou égal » réussit, « supérieur » échoue.
            info[4..8].copy_from_slice(&10u32.to_le_bytes());
            info[8..12].copy_from_slice(&0u32.to_le_bytes());
            assert_eq!(verify_version_info_w(info.as_ptr(), 3, mask), 1);
            let strict = ver_set_condition_mask(ver_set_condition_mask(0, 2, 2), 1, 2);
            assert_eq!(verify_version_info_w(info.as_ptr(), 3, strict), 0);
        }
    }

    #[test]
    fn contexte_initialise_et_aligne() {
        let _teb = crate::process::test_teb();
        let mut len = 0u32;
        unsafe {
            assert_eq!(
                initialize_context(
                    std::ptr::null_mut(),
                    0x10_004B,
                    std::ptr::null_mut(),
                    &mut len
                ),
                0
            );
            assert!(len as usize >= CONTEXT_SIZE);
            let mut buf = vec![0xAAu8; len as usize];
            let mut ctx = std::ptr::null_mut();
            assert_eq!(
                initialize_context(buf.as_mut_ptr(), 0x10_004B, &mut ctx, &mut len),
                1
            );
            assert_eq!(ctx as usize % 16, 0);
            assert_eq!((ctx.add(0x30) as *const u32).read(), 0x10_000B);
            let mut size = 0;
            assert_eq!(locate_xstate_feature(ctx, 1, &mut size), ctx.add(0x1A0));
            assert_eq!(size, 256);
            assert!(locate_xstate_feature(ctx, 2, &mut size).is_null());
        }
    }

    #[test]
    fn locale_et_langues_d_interface() {
        let _teb = crate::process::test_teb();
        let mut buf = [0u16; 16];
        unsafe {
            use crate::dlls::nls::{
                get_locale_info_ex, get_user_default_locale_name, locale_name_to_lcid,
            };
            assert_eq!(get_user_default_locale_name(buf.as_mut_ptr(), 16), 6);
            assert_eq!(String::from_utf16_lossy(&buf[..5]), "fr-FR");
            let name: Vec<u16> = "en-US\0".encode_utf16().collect();
            assert_eq!(locale_name_to_lcid(name.as_ptr(), 0), 0x409);
            assert_eq!(
                get_locale_info_ex(name.as_ptr(), 0x5C, buf.as_mut_ptr(), 16),
                6
            );
            assert_eq!(String::from_utf16_lossy(&buf[..5]), "en-US");
        }
    }
}
