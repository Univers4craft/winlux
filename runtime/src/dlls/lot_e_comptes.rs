//! Lot E de #1410 (#1614) : netapi32, wtsapi32 et userenv, surtout
//! importés par pywin32 (win32net, win32ts, win32profile).
//!
//! Winlux est un poste isolé : pas de domaine, pas de serveur SMB ni de
//! service Affichage des messages, une seule session console (n° 1), et
//! les comptes sont ceux de Linux, gérés par les Paramètres. Chaque
//! fonction rend donc ce qu'un poste Windows hors domaine rendrait (nom
//! du poste, groupe de travail, session console, compte courant) ou le
//! code d'erreur documenté (objet introuvable, accès refusé) que les
//! programmes savent traiter. Rien n'est modifié en douce.
//!
//! Sources : documentation Microsoft Learn de chaque fonction et des
//! structures WKSTA_INFO_100/101/102, SERVER_INFO_100/101, USER_INFO_0/1,
//! WKSTA_USER_INFO_0/1, WTS_SESSION_INFOW, WTS_PROCESS_INFOW, PROFILEINFOW,
//! NCB (lmwksta.h, lmserver.h, lmaccess.h, wtsapi32.h, profinfo.h, nb30.h).
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use crate::process::set_last_error;
use crate::wow32::{pointer_size, write_ptr};

const NERR_SUCCESS: u32 = 0;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INVALID_LEVEL: u32 = 124;
const ERROR_PRIVILEGE_NOT_HELD: u32 = 1314;
const ERROR_NO_SUCH_DOMAIN: u32 = 1355;
const ERROR_NO_BROWSER_SERVERS_FOUND: u32 = 6118;
const NERR_GROUP_NOT_FOUND: u32 = 2220;
const NERR_USER_NOT_FOUND: u32 = 2221;
const NERR_USE_NOT_FOUND: u32 = 2250;
const NERR_NAME_NOT_FOUND: u32 = 2273;
const NERR_NET_NAME_NOT_FOUND: u32 = 2310;
const NERR_DEVICE_NOT_SHARED: u32 = 2311;
const NERR_CLIENT_NAME_NOT_FOUND: u32 = 2312;
const NERR_FILE_ID_NOT_FOUND: u32 = 2314;
const NERR_DC_NOT_FOUND: u32 = 2453;
const NERR_INVALID_COMPUTER: u32 = 2351;
/// PLATFORM_ID_NT.
const PLATFORM_ID_NT: u32 = 500;
/// SV_TYPE_WORKSTATION | SV_TYPE_SERVER | SV_TYPE_NT.
const SV_TYPE_POSTE: u32 = 0x0000_1003;

// ============================================================
//  Tampons NetApiBufferAllocate / WTSFreeMemory
// ============================================================

/// Champ d'une structure à empaqueter : DWORD ou pointeur (chaîne UTF-16
/// copiée après le tableau, ou valeur brute).
pub(crate) enum F {
    U32(u32),
    Str(Option<String>),
    Ptr(usize),
}

/// Tableau de structures C (DWORD alignés sur 4, pointeurs sur la taille
/// de pointeur du programme), suivi de leurs chaînes, dans un seul bloc
/// LocalAlloc que NetApiBufferFree ou WTSFreeMemory rendent.
pub(crate) unsafe fn pack(records: &[Vec<F>]) -> usize {
    let total = packed_size(records).max(8);
    let base = crate::dlls::kernel32_misc::local_alloc(0x40, total) as usize;
    if base != 0 {
        pack_at(records, base);
    }
    base
}

/// Décalages des champs, taille brute et alignement d'une structure.
fn layout(r: &[F]) -> (Vec<usize>, usize, usize) {
    let ps = pointer_size();
    let (mut off, mut align, mut offs) = (0usize, 4usize, Vec::new());
    for f in r {
        let a = if matches!(f, F::U32(_)) { 4 } else { ps };
        align = align.max(a);
        off = off.next_multiple_of(a);
        offs.push(off);
        off += a;
    }
    (offs, off, align)
}

fn record_size(records: &[Vec<F>]) -> usize {
    records.first().map_or(0, |r| {
        let (_, raw, align) = layout(r);
        raw.next_multiple_of(align)
    })
}

/// Octets du tableau et de ses chaînes.
pub(crate) fn packed_size(records: &[Vec<F>]) -> usize {
    let strings: usize = records
        .iter()
        .flatten()
        .map(|f| match f {
            F::Str(Some(s)) => (s.encode_utf16().count() + 1) * 2,
            _ => 0,
        })
        .sum();
    record_size(records) * records.len() + strings
}

/// Écrit le tableau et ses chaînes à `base` (packed_size octets).
pub(crate) unsafe fn pack_at(records: &[Vec<F>], base: usize) {
    let size = record_size(records);
    let mut text = base + size * records.len();
    for (i, r) in records.iter().enumerate() {
        let (offs, _, _) = layout(r);
        for (f, off) in r.iter().zip(offs) {
            let at = base + i * size + off;
            match f {
                F::U32(v) => (at as *mut u32).write_unaligned(*v),
                F::Ptr(v) => write_ptr(at, *v),
                F::Str(None) => write_ptr(at, 0),
                F::Str(Some(s)) => {
                    write_ptr(at, text);
                    for c in s.encode_utf16().chain(std::iter::once(0)) {
                        (text as *mut u16).write_unaligned(c);
                        text += 2;
                    }
                }
            }
        }
    }
}

/// Écrit `value` à `out` (pointeur de pointeur du programme), si présent.
unsafe fn out_ptr(out: usize, value: usize) {
    if out != 0 {
        write_ptr(out, value);
    }
}
unsafe fn out_u32(out: *mut u32, value: u32) {
    if !out.is_null() {
        *out = value;
    }
}

/// Énumération réussie : `records` (peut être vide) ; lus = total.
unsafe fn enum_result(records: Vec<Vec<F>>, buf: usize, read: *mut u32, total: *mut u32) -> u32 {
    if buf == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let n = records.len() as u32;
    let p = if records.is_empty() {
        0
    } else {
        pack(&records)
    };
    out_ptr(buf, p);
    out_u32(read, n);
    out_u32(total, n);
    NERR_SUCCESS
}

/// Recherche sans résultat : tampon nul, compteurs à zéro.
unsafe fn not_found(code: u32, buf: usize, read: *mut u32, total: *mut u32) -> u32 {
    out_ptr(buf, 0);
    out_u32(read, 0);
    out_u32(total, 0);
    code
}

pub(crate) fn computer_name() -> String {
    crate::dlls::kernel32::get_hostname_string().to_ascii_uppercase()
}
pub(crate) fn user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "winlux".into())
}
const WORKGROUP: &str = "WORKGROUP";

/// Le compte demandé est-il le compte courant (seul compte que Winlux
/// expose aux programmes) ? Le nom peut porter « POSTE\ ».
unsafe fn is_current_user(name: *const u16) -> bool {
    if name.is_null() {
        return false;
    }
    let n = crate::dlls::wstr(name);
    let n = n.rsplit('\\').next().unwrap_or("");
    n.eq_ignore_ascii_case(&user_name())
}

// ============================================================
//  netapi32 : poste et serveur
// ============================================================

/// Servie par sandbox_dlls (copie unique) ; gardée ici pour les tests.
#[cfg(test)]
unsafe extern "win64" fn NetApiBufferFree(buf: usize) -> u32 {
    if buf != 0 {
        crate::dlls::kernel32_misc::local_free(buf as *mut std::ffi::c_void);
    }
    NERR_SUCCESS
}

unsafe extern "win64" fn NetApiBufferAllocate(size: u32, buf: usize) -> u32 {
    if buf == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let p = crate::dlls::kernel32_misc::local_alloc(0x40, size.max(1) as usize) as usize;
    out_ptr(buf, p);
    if p == 0 {
        8
    } else {
        NERR_SUCCESS
    }
}

/// NetWkstaGetInfo : niveaux 100, 101 et 102 (Windows 10, version 10.0).
unsafe extern "win64" fn NetWkstaGetInfo(_server: usize, level: u32, buf: usize) -> u32 {
    let mut r = vec![
        F::U32(PLATFORM_ID_NT),
        F::Str(Some(computer_name())),
        F::Str(Some(WORKGROUP.into())),
        F::U32(10),
        F::U32(0),
    ];
    match level {
        100 => {}
        101 => r.push(F::Str(Some(String::new()))),
        102 => {
            r.push(F::Str(Some(String::new())));
            r.push(F::U32(1));
        }
        _ => return ERROR_INVALID_LEVEL,
    }
    if buf == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    out_ptr(buf, pack(&[r]));
    NERR_SUCCESS
}

/// NetServerGetInfo : niveaux 100 et 101 (poste de travail NT, sans
/// commentaire).
unsafe extern "win64" fn NetServerGetInfo(_server: usize, level: u32, buf: usize) -> u32 {
    let mut r = vec![F::U32(PLATFORM_ID_NT), F::Str(Some(computer_name()))];
    match level {
        100 => {}
        101 => {
            r.extend([F::U32(10), F::U32(0), F::U32(SV_TYPE_POSTE)]);
            r.push(F::Str(Some(String::new())));
        }
        _ => return ERROR_INVALID_LEVEL,
    }
    if buf == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    out_ptr(buf, pack(&[r]));
    NERR_SUCCESS
}

/// NetWkstaUserEnum : le compte courant, ouvert sur ce poste.
unsafe extern "win64" fn NetWkstaUserEnum(
    _server: usize,
    level: u32,
    buf: usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
    _resume: usize,
) -> u32 {
    let name = Some(user_name());
    let r = match level {
        0 => vec![F::Str(name)],
        1 => vec![
            F::Str(name),
            F::Str(Some(computer_name())),
            F::Str(Some(String::new())),
            F::Str(Some(computer_name())),
        ],
        _ => return ERROR_INVALID_LEVEL,
    };
    enum_result(vec![r], buf, read, total)
}

/// Énumérations sans élément sur un poste isolé (transports, sessions,
/// fichiers ouverts, connexions, groupes globaux, alias de messagerie).
macro_rules! empty_enum {
    ($name:ident, $($arg:ident),*; $buf:ident, $read:ident, $total:ident) => {
        unsafe extern "win64" fn $name($($arg: usize,)* $buf: usize, _max: u32,
            $read: *mut u32, $total: *mut u32, _resume: usize) -> u32 {
            $(let _ = $arg;)*
            enum_result(Vec::new(), $buf, $read, $total)
        }
    };
}
empty_enum!(NetWkstaTransportEnum, server, level; buf, read, total);
empty_enum!(NetGroupEnum, server, level; buf, read, total);
empty_enum!(NetLocalGroupEnum, server, level; buf, read, total);
empty_enum!(NetMessageNameEnum, server, level; buf, read, total);
empty_enum!(NetUseEnum, server, level; buf, read, total);
empty_enum!(NetSessionEnum, server, client, user, level; buf, read, total);
empty_enum!(NetFileEnum, server, base, user, level; buf, read, total);

/// NetServerEnum : aucun service Explorateur d'ordinateurs.
unsafe extern "win64" fn NetServerEnum(
    _server: usize,
    _level: u32,
    buf: usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
    _kind: u32,
    _domain: usize,
    _resume: usize,
) -> u32 {
    not_found(ERROR_NO_BROWSER_SERVERS_FOUND, buf, read, total)
}

/// NetServerDiskEnum (niveau 0) : chaînes de trois caractères « X: ».
unsafe extern "win64" fn NetServerDiskEnum(
    _server: usize,
    level: u32,
    buf: usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
    _resume: usize,
) -> u32 {
    if level != 0 {
        return ERROR_INVALID_LEVEL;
    }
    if buf == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let mask = crate::dlls::kernel32::get_logical_drives();
    let drives: Vec<u16> = (0..26u16)
        .filter(|i| mask & (1 << i) != 0)
        .flat_map(|i| [b'A' as u16 + i, b':' as u16, 0])
        .collect();
    let n = drives.len() / 3;
    let p = crate::dlls::kernel32_misc::local_alloc(0x40, drives.len() * 2 + 2) as *mut u16;
    if p.is_null() {
        return 8;
    }
    std::ptr::copy_nonoverlapping(drives.as_ptr(), p, drives.len());
    out_ptr(buf, p as usize);
    out_u32(read, n as u32);
    out_u32(total, n as u32);
    NERR_SUCCESS
}

/// NetGetJoinInformation : groupe de travail « WORKGROUP »
/// (NetSetupWorkgroupName = 2).
unsafe extern "win64" fn NetGetJoinInformation(_server: usize, name: usize, kind: *mut u32) -> u32 {
    if name == 0 || kind.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let w: Vec<u16> = WORKGROUP.encode_utf16().chain(std::iter::once(0)).collect();
    let p = crate::dlls::kernel32_misc::local_alloc(0x40, w.len() * 2) as *mut u16;
    if p.is_null() {
        return 8;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    out_ptr(name, p as usize);
    *kind = 2;
    NERR_SUCCESS
}

/// NetValidateName : syntaxe d'un nom d'ordinateur ou de groupe de
/// travail ; un domaine n'est jamais joignable.
unsafe extern "win64" fn NetValidateName(
    _server: usize,
    name: *const u16,
    _account: usize,
    _password: usize,
    kind: u32,
) -> u32 {
    if name.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let n = crate::dlls::wstr(name);
    let netbios_ok = |s: &str| {
        !s.is_empty()
            && s.chars().count() <= 15
            && !s
                .chars()
                .any(|c| "\\/:*?\"<>|,~!@#$%^&'(){}".contains(c) || c.is_control())
    };
    match kind {
        // NetSetupMachine, NetSetupWorkgroup
        1 | 2 => {
            if netbios_ok(&n) {
                NERR_SUCCESS
            } else {
                NERR_INVALID_COMPUTER
            }
        }
        // NetSetupDomain
        3 => ERROR_NO_SUCH_DOMAIN,
        // NetSetupNonExistentDomain
        4 => NERR_SUCCESS,
        // NetSetupDnsMachine
        5 => {
            let ok = !n.is_empty()
                && n.len() <= 63
                && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !n.starts_with('-');
            if ok {
                NERR_SUCCESS
            } else {
                NERR_INVALID_COMPUTER
            }
        }
        _ => ERROR_INVALID_PARAMETER,
    }
}

unsafe extern "win64" fn NetGetAnyDCName(_server: usize, _domain: usize, buf: usize) -> u32 {
    out_ptr(buf, 0);
    NERR_DC_NOT_FOUND
}

/// DsGetDcNameW : hors domaine, aucun contrôleur.
unsafe extern "win64" fn DsGetDcNameW(
    _computer: usize,
    _domain: usize,
    _guid: usize,
    _site: usize,
    _flags: u32,
    info: usize,
) -> u32 {
    out_ptr(info, 0);
    ERROR_NO_SUCH_DOMAIN
}

// ============================================================
//  netapi32 : comptes et groupes
// ============================================================

/// NetUserEnum : le compte courant (niveaux 0 et 1 ; USER_PRIV_USER,
/// UF_SCRIPT | UF_NORMAL_ACCOUNT).
unsafe extern "win64" fn NetUserEnum(
    _server: usize,
    level: u32,
    _filter: u32,
    buf: usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
    _resume: usize,
) -> u32 {
    let name = Some(user_name());
    let r = match level {
        0 => vec![F::Str(name)],
        1 => vec![
            F::Str(name),
            F::Str(None),
            F::U32(0),
            F::U32(1),
            F::Str(Some(crate::paths::profile_windows())),
            F::Str(Some(String::new())),
            F::U32(0x0201),
            F::Str(Some(String::new())),
        ],
        _ => return ERROR_INVALID_LEVEL,
    };
    enum_result(vec![r], buf, read, total)
}

/// NetUserGetGroups et NetUserGetLocalGroups : aucun groupe exposé pour le
/// compte courant ; un autre compte n'existe pas.
unsafe extern "win64" fn NetUserGetGroups(
    _server: usize,
    user: *const u16,
    _level: u32,
    buf: usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
) -> u32 {
    if !is_current_user(user) {
        return not_found(NERR_USER_NOT_FOUND, buf, read, total);
    }
    enum_result(Vec::new(), buf, read, total)
}
unsafe extern "win64" fn NetUserGetLocalGroups(
    _server: usize,
    user: *const u16,
    _level: u32,
    _flags: u32,
    buf: usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
) -> u32 {
    if !is_current_user(user) {
        return not_found(NERR_USER_NOT_FOUND, buf, read, total);
    }
    enum_result(Vec::new(), buf, read, total)
}

/// Modifications d'administration (comptes, groupes, partages, nom du
/// poste) : les comptes restent ceux de Linux, gérés par les Paramètres.
unsafe extern "win64" fn denied(_a: usize, _b: usize, _c: usize, _d: usize, _e: usize) -> u32 {
    ERROR_ACCESS_DENIED
}
unsafe extern "win64" fn not_supported(
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
) -> u32 {
    ERROR_NOT_SUPPORTED
}
unsafe extern "win64" fn ok_status(_a: usize) -> u32 {
    NERR_SUCCESS
}

/// Objet nommé introuvable, le pointeur de sortie éventuel (rang
/// `out`, compté à partir de 0) remis à zéro.
macro_rules! missing {
    ($name:ident, $code:expr, $out:expr) => {
        unsafe extern "win64" fn $name(a0: usize, a1: usize, a2: usize, a3: usize) -> u32 {
            let args = [a0, a1, a2, a3];
            if let Some(i) = $out {
                out_ptr(args[i], 0);
            }
            $code
        }
    };
}
missing!(NetGroupDel, NERR_GROUP_NOT_FOUND, None::<usize>);
missing!(NetGroupDelUser, NERR_GROUP_NOT_FOUND, None::<usize>);
missing!(NetGroupGetInfo, NERR_GROUP_NOT_FOUND, Some(3));
missing!(NetGroupSetInfo, NERR_GROUP_NOT_FOUND, None::<usize>);
missing!(NetGroupSetUsers, NERR_GROUP_NOT_FOUND, None::<usize>);
missing!(NetLocalGroupGetInfo, NERR_GROUP_NOT_FOUND, Some(3));
missing!(NetLocalGroupSetInfo, NERR_GROUP_NOT_FOUND, None::<usize>);
missing!(NetLocalGroupSetMembers, NERR_GROUP_NOT_FOUND, None::<usize>);
missing!(NetLocalGroupDelMembers, NERR_GROUP_NOT_FOUND, None::<usize>);
missing!(NetSessionDel, NERR_CLIENT_NAME_NOT_FOUND, None::<usize>);
missing!(NetShareDel, NERR_NET_NAME_NOT_FOUND, None::<usize>);
missing!(NetShareGetInfo, NERR_NET_NAME_NOT_FOUND, Some(3));
missing!(NetShareSetInfo, NERR_NET_NAME_NOT_FOUND, None::<usize>);
missing!(NetShareCheck, NERR_DEVICE_NOT_SHARED, None::<usize>);
missing!(NetFileClose, NERR_FILE_ID_NOT_FOUND, None::<usize>);
missing!(NetFileGetInfo, NERR_FILE_ID_NOT_FOUND, Some(3));
missing!(NetUseDel, NERR_USE_NOT_FOUND, None::<usize>);
missing!(NetUseGetInfo, NERR_USE_NOT_FOUND, Some(3));
missing!(NetMessageBufferSend, NERR_NAME_NOT_FOUND, None::<usize>);
missing!(NetStatisticsGet, ERROR_NOT_SUPPORTED, None::<usize>);

/// NetGroupGetUsers : aucun groupe global.
unsafe extern "win64" fn NetGroupGetUsers(
    _server: usize,
    _group: usize,
    _level: u32,
    buf: usize,
    _max: u32,
    read: *mut u32,
    total: *mut u32,
    _resume: usize,
) -> u32 {
    not_found(NERR_GROUP_NOT_FOUND, buf, read, total)
}

/// NetSessionGetInfo : aucune session réseau sur ce poste.
unsafe extern "win64" fn NetSessionGetInfo(
    _server: usize,
    _client: usize,
    _user: usize,
    _level: u32,
    buf: usize,
) -> u32 {
    out_ptr(buf, 0);
    NERR_CLIENT_NAME_NOT_FOUND
}

/// NetValidatePasswordPolicy : pas de stratégie de mots de passe Windows.
unsafe extern "win64" fn NetValidatePasswordPolicy(
    _server: usize,
    _qualifier: usize,
    _kind: u32,
    _input: usize,
    output: usize,
) -> u32 {
    out_ptr(output, 0);
    ERROR_NOT_SUPPORTED
}

/// Netbios : aucun adaptateur NetBIOS (NRC_BRIDGE, numéro de LANA
/// invalide), réponse écrite dans le NCB comme le veut nb30.h.
unsafe extern "win64" fn Netbios(ncb: *mut u8) -> u8 {
    const NRC_BRIDGE: u8 = 0x23;
    if ncb.is_null() {
        return NRC_BRIDGE;
    }
    // ncb_cmd_cplt : après ncb_lana_num, qui suit ncb_post.
    let cplt = if pointer_size() == 4 { 49 } else { 65 };
    *ncb.add(1) = NRC_BRIDGE;
    *ncb.add(cplt) = NRC_BRIDGE;
    NRC_BRIDGE
}

pub fn netapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "NetApiBufferAllocate" => fn NetApiBufferAllocate,
        "NetWkstaGetInfo" => fn NetWkstaGetInfo,
        "NetWkstaSetInfo" => fn denied,
        "NetWkstaUserEnum" => fn NetWkstaUserEnum,
        "NetWkstaTransportEnum" => fn NetWkstaTransportEnum,
        "NetWkstaTransportAdd" => fn denied,
        "NetWkstaTransportDel" => fn denied,
        "NetServerGetInfo" => fn NetServerGetInfo,
        "NetServerSetInfo" => fn denied,
        "NetServerEnum" => fn NetServerEnum,
        "NetServerDiskEnum" => fn NetServerDiskEnum,
        "NetServerComputerNameAdd" => fn denied,
        "NetServerComputerNameDel" => fn denied,
        "NetGetJoinInformation" => fn NetGetJoinInformation,
        "NetValidateName" => fn NetValidateName,
        "NetGetAnyDCName" => fn NetGetAnyDCName,
        "DsGetDcNameW" => fn DsGetDcNameW,
        "NetUserEnum" => fn NetUserEnum,
        "NetUserGetGroups" => fn NetUserGetGroups,
        "NetUserGetLocalGroups" => fn NetUserGetLocalGroups,
        "NetUserChangePassword" => fn denied,
        "NetUserModalsSet" => fn denied,
        "NetGroupAdd" => fn denied,
        "NetGroupAddUser" => fn denied,
        "NetGroupDel" => fn NetGroupDel,
        "NetGroupDelUser" => fn NetGroupDelUser,
        "NetGroupEnum" => fn NetGroupEnum,
        "NetGroupGetInfo" => fn NetGroupGetInfo,
        "NetGroupGetUsers" => fn NetGroupGetUsers,
        "NetGroupSetInfo" => fn NetGroupSetInfo,
        "NetGroupSetUsers" => fn NetGroupSetUsers,
        "NetLocalGroupEnum" => fn NetLocalGroupEnum,
        "NetLocalGroupGetInfo" => fn NetLocalGroupGetInfo,
        "NetLocalGroupSetInfo" => fn NetLocalGroupSetInfo,
        "NetLocalGroupSetMembers" => fn NetLocalGroupSetMembers,
        "NetLocalGroupDelMembers" => fn NetLocalGroupDelMembers,
        "NetMessageBufferSend" => fn NetMessageBufferSend,
        "NetMessageNameAdd" => fn not_supported,
        "NetMessageNameDel" => fn not_supported,
        "NetMessageNameEnum" => fn NetMessageNameEnum,
        "NetSessionEnum" => fn NetSessionEnum,
        "NetSessionGetInfo" => fn NetSessionGetInfo,
        "NetSessionDel" => fn NetSessionDel,
        "NetShareAdd" => fn denied,
        "NetShareCheck" => fn NetShareCheck,
        "NetShareDel" => fn NetShareDel,
        "NetShareGetInfo" => fn NetShareGetInfo,
        "NetShareSetInfo" => fn NetShareSetInfo,
        "NetFileEnum" => fn NetFileEnum,
        "NetFileGetInfo" => fn NetFileGetInfo,
        "NetFileClose" => fn NetFileClose,
        "NetStatisticsGet" => fn NetStatisticsGet,
        "NetUseAdd" => fn not_supported,
        "NetUseDel" => fn NetUseDel,
        "NetUseEnum" => fn NetUseEnum,
        "NetUseGetInfo" => fn NetUseGetInfo,
        "NetValidatePasswordPolicy" => fn NetValidatePasswordPolicy,
        "NetValidatePasswordPolicyFree" => fn ok_status,
        "Netbios" => fn Netbios,
    }
}

// ============================================================
//  wtsapi32
// ============================================================

/// WTSEnumerateSessionsW : la session console (1, « Console », active).
unsafe extern "win64" fn WTSEnumerateSessionsW(
    _server: usize,
    _reserved: u32,
    version: u32,
    out: usize,
    count: *mut u32,
) -> i32 {
    if version != 1 || out == 0 || count.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let p = pack(&[vec![F::U32(1), F::Str(Some("Console".into())), F::U32(0)]]);
    out_ptr(out, p);
    *count = 1;
    1
}

/// Processus visibles du compte, lus dans /proc (nom = comm Linux).
fn processes() -> Vec<(u32, String)> {
    let uid = unsafe { libc::getuid() };
    let mut v: Vec<(u32, String)> = std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            use std::os::unix::fs::MetadataExt;
            (e.metadata().ok()?.uid() == uid).then_some(())?;
            let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
            Some((pid, comm.trim().to_string()))
        })
        .collect();
    v.sort();
    v
}

/// WTSEnumerateProcessesW : WTS_PROCESS_INFOW, sans SID (pUserSid nul).
unsafe extern "win64" fn WTSEnumerateProcessesW(
    _server: usize,
    _reserved: u32,
    version: u32,
    out: usize,
    count: *mut u32,
) -> i32 {
    if version != 1 || out == 0 || count.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let recs: Vec<Vec<F>> = processes()
        .into_iter()
        .map(|(pid, name)| vec![F::U32(1), F::U32(pid), F::Str(Some(name)), F::Ptr(0)])
        .collect();
    *count = recs.len() as u32;
    out_ptr(out, pack(&recs));
    1
}

/// WTSEnumerateServersW : aucun serveur Terminal Server dans le domaine.
unsafe extern "win64" fn WTSEnumerateServersW(
    _domain: usize,
    _reserved: u32,
    _version: u32,
    out: usize,
    count: *mut u32,
) -> i32 {
    if out == 0 || count.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    out_ptr(out, pack(&[]));
    *count = 0;
    1
}

unsafe extern "win64" fn wts_denied(_a: usize, _b: usize, _c: usize, _d: usize) -> i32 {
    set_last_error(ERROR_ACCESS_DENIED);
    0
}
unsafe extern "win64" fn wts_not_supported(_a: usize, _b: usize, _c: usize, d: usize) -> i32 {
    out_ptr(d, 0);
    set_last_error(ERROR_NOT_SUPPORTED);
    0
}

/// WTSQueryUserToken : réservé aux services (SE_TCB_NAME).
unsafe extern "win64" fn WTSQueryUserToken(_session: u32, token: usize) -> i32 {
    out_ptr(token, 0);
    set_last_error(ERROR_PRIVILEGE_NOT_HELD);
    0
}

/// WTSTerminateProcess : fin d'un processus du compte (SIGKILL).
unsafe extern "win64" fn WTSTerminateProcess(_server: usize, pid: u32, _code: u32) -> i32 {
    if pid == 0 || pid > i32::MAX as u32 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if libc::kill(pid as i32, libc::SIGKILL) == 0 {
        return 1;
    }
    let e = std::io::Error::last_os_error().raw_os_error();
    set_last_error(if e == Some(libc::ESRCH) {
        ERROR_INVALID_PARAMETER
    } else {
        ERROR_ACCESS_DENIED
    });
    0
}

/// Copie d'un texte de longueur donnée en octets, terminé par un nul.
unsafe fn counted_wide(p: *const u16, bytes: u32) -> Vec<u16> {
    let mut v = if p.is_null() {
        Vec::new()
    } else {
        std::slice::from_raw_parts(p, bytes as usize / 2).to_vec()
    };
    if let Some(i) = v.iter().position(|&c| c == 0) {
        v.truncate(i);
    }
    v.push(0);
    v
}

/// WTSSendMessageW : boîte de message dans la session console.
unsafe extern "win64" fn WTSSendMessageW(
    _server: usize,
    _session: u32,
    title: *const u16,
    title_len: u32,
    message: *const u16,
    message_len: u32,
    style: u32,
    _timeout: u32,
    response: *mut u32,
    wait: i32,
) -> i32 {
    let t = counted_wide(title, title_len);
    let m = counted_wide(message, message_len);
    if wait != 0 {
        let r = crate::dlls::dialogs::message_box_w(0, m.as_ptr(), t.as_ptr(), style);
        out_u32(response, r as u32);
    } else {
        std::thread::spawn(move || {
            crate::dlls::dialogs::message_box_w(0, m.as_ptr(), t.as_ptr(), style);
        });
        out_u32(response, 32001); // IDASYNC
    }
    1
}

static WTS_FLUSH: (std::sync::Mutex<u64>, std::sync::Condvar) =
    (std::sync::Mutex::new(0), std::sync::Condvar::new());

/// WTSWaitSystemEvent : la session console ne change jamais ; un appel
/// avec WTS_EVENT_FLUSH libère les attentes en cours.
unsafe extern "win64" fn WTSWaitSystemEvent(_server: usize, mask: u32, flags: *mut u32) -> i32 {
    const WTS_EVENT_FLUSH: u32 = 0x0100_0000;
    if flags.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (lock, cv) = &WTS_FLUSH;
    let mut gen = crate::dlls::lock(lock);
    if mask == WTS_EVENT_FLUSH {
        *gen += 1;
        cv.notify_all();
        *flags = 0;
        return 1;
    }
    let start = *gen;
    while *gen == start {
        gen = cv.wait(gen).unwrap_or_else(|e| e.into_inner());
    }
    *flags = WTS_EVENT_FLUSH;
    1
}

pub fn wtsapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "WTSEnumerateSessionsW" => fn WTSEnumerateSessionsW,
        "WTSEnumerateProcessesW" => fn WTSEnumerateProcessesW,
        "WTSEnumerateServersW" => fn WTSEnumerateServersW,
        "WTSDisconnectSession" => fn wts_denied,
        "WTSLogoffSession" => fn wts_denied,
        "WTSShutdownSystem" => fn wts_denied,
        "WTSSetUserConfigW" => fn wts_denied,
        "WTSQueryUserConfigW" => fn wts_not_supported,
        "WTSQueryUserToken" => fn WTSQueryUserToken,
        "WTSTerminateProcess" => fn WTSTerminateProcess,
        "WTSSendMessageW" => fn WTSSendMessageW,
        "WTSWaitSystemEvent" => fn WTSWaitSystemEvent,
    }
}

// ============================================================
//  userenv
// ============================================================

unsafe extern "win64" fn GetAllUsersProfileDirectoryW(buf: *mut u16, size: *mut u32) -> i32 {
    crate::dlls::knownfolders::profile_out(
        "C:\\ProgramData".encode_utf16().collect(),
        0u16,
        buf,
        size,
    )
}
unsafe extern "win64" fn GetDefaultUserProfileDirectoryW(buf: *mut u16, size: *mut u32) -> i32 {
    crate::dlls::knownfolders::profile_out(
        "C:\\Users\\Default".encode_utf16().collect(),
        0u16,
        buf,
        size,
    )
}

/// ExpandEnvironmentStringsForUserW : l'environnement du compte courant.
unsafe extern "win64" fn ExpandEnvironmentStringsForUserW(
    _token: usize,
    src: *const u16,
    dst: *mut u16,
    size: u32,
) -> i32 {
    if src.is_null() || dst.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let n = crate::dlls::kernel32_fs::expand_environment_strings_w(src, dst, size);
    if n == 0 {
        return 0;
    }
    if n > size {
        set_last_error(122);
        return 0;
    }
    1
}

/// GetProfileType : profil local (0).
unsafe extern "win64" fn GetProfileType(flags: *mut u32) -> i32 {
    if flags.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *flags = 0;
    1
}

/// LoadUserProfileW : le profil du compte courant est déjà chargé ;
/// hProfile = HKEY_CURRENT_USER.
unsafe extern "win64" fn LoadUserProfileW(_token: usize, info: *mut u8) -> i32 {
    if info.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let ps = pointer_size();
    let size = (info as *const u32).read_unaligned() as usize;
    let h_profile = 8 + 5 * ps;
    if size < h_profile + ps {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    write_ptr(info as usize + h_profile, 0x8000_0001);
    1
}
unsafe extern "win64" fn UnloadUserProfile(_token: usize, _profile: usize) -> i32 {
    1
}
unsafe extern "win64" fn DeleteProfileW(_sid: usize, _path: usize, _machine: usize) -> i32 {
    set_last_error(ERROR_ACCESS_DENIED);
    0
}

/// Exports du lot E pour les DLL de comptes et de sessions.
pub fn all() -> Vec<(&'static str, crate::dlls::Exports)> {
    vec![
        ("netapi32", netapi32_exports()),
        ("wtsapi32", wtsapi32_exports()),
        ("userenv", userenv_exports()),
    ]
}

pub fn userenv_exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetAllUsersProfileDirectoryW" => fn GetAllUsersProfileDirectoryW,
        "GetDefaultUserProfileDirectoryW" => fn GetDefaultUserProfileDirectoryW,
        "ExpandEnvironmentStringsForUserW" => fn ExpandEnvironmentStringsForUserW,
        "GetProfileType" => fn GetProfileType,
        "LoadUserProfileW" => fn LoadUserProfileW,
        "UnloadUserProfile" => fn UnloadUserProfile,
        "DeleteProfileW" => fn DeleteProfileW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn wide_at(p: usize) -> String {
        crate::dlls::wstr(p as *const u16)
    }

    #[test]
    fn poste_isole() {
        unsafe {
            let mut buf = 0usize;
            assert_eq!(NetWkstaGetInfo(0, 100, &mut buf as *mut _ as usize), 0);
            assert_eq!((buf as *const u32).read(), PLATFORM_ID_NT);
            assert_eq!(wide_at(*((buf + 8) as *const usize)), computer_name());
            assert_eq!(wide_at(*((buf + 16) as *const usize)), WORKGROUP);
            assert_eq!(*((buf + 24) as *const u32), 10);
            NetApiBufferFree(buf);
            assert_eq!(
                NetWkstaGetInfo(0, 7, &mut buf as *mut _ as usize),
                ERROR_INVALID_LEVEL
            );

            let (mut read, mut total) = (9u32, 9u32);
            assert_eq!(
                NetUserEnum(
                    0,
                    0,
                    0,
                    &mut buf as *mut _ as usize,
                    u32::MAX,
                    &mut read,
                    &mut total,
                    0
                ),
                0
            );
            assert_eq!((read, total), (1, 1));
            assert_eq!(wide_at(*(buf as *const usize)), user_name());
            NetApiBufferFree(buf);

            assert_eq!(
                NetLocalGroupEnum(
                    0,
                    0,
                    &mut buf as *mut _ as usize,
                    u32::MAX,
                    &mut read,
                    &mut total,
                    0
                ),
                0
            );
            assert_eq!((buf, read, total), (0, 0, 0));

            let mut kind = 0u32;
            assert_eq!(
                NetGetJoinInformation(0, &mut buf as *mut _ as usize, &mut kind),
                0
            );
            assert_eq!((wide_at(buf), kind), (WORKGROUP.to_string(), 2));
            NetApiBufferFree(buf);

            let ok: Vec<u16> = "POSTE-1\0".encode_utf16().collect();
            let bad: Vec<u16> = "a:b\0".encode_utf16().collect();
            assert_eq!(NetValidateName(0, ok.as_ptr(), 0, 0, 1), 0);
            assert_eq!(
                NetValidateName(0, bad.as_ptr(), 0, 0, 1),
                NERR_INVALID_COMPUTER
            );
            assert_eq!(
                NetValidateName(0, ok.as_ptr(), 0, 0, 3),
                ERROR_NO_SUCH_DOMAIN
            );

            buf = 1;
            assert_eq!(
                NetShareGetInfo(0, 0, 2, &mut buf as *mut _ as usize),
                NERR_NET_NAME_NOT_FOUND
            );
            assert_eq!(buf, 0);

            let mut ncb = [0u8; 72];
            assert_eq!(Netbios(ncb.as_mut_ptr()), 0x23);
            assert_eq!((ncb[1], ncb[65]), (0x23, 0x23));
        }
    }

    #[test]
    fn session_console_et_profil() {
        unsafe {
            let (mut p, mut n) = (0usize, 0u32);
            assert_eq!(
                WTSEnumerateSessionsW(0, 0, 1, &mut p as *mut _ as usize, &mut n),
                1
            );
            assert_eq!(n, 1);
            assert_eq!(*(p as *const u32), 1);
            assert_eq!(wide_at(*((p + 8) as *const usize)), "Console");
            assert_eq!(*((p + 16) as *const u32), 0);
            crate::dlls::kernel32_misc::local_free(p as *mut std::ffi::c_void);

            assert_eq!(
                WTSEnumerateProcessesW(0, 0, 1, &mut p as *mut _ as usize, &mut n),
                1
            );
            let me = std::process::id();
            let found = (0..n as usize).any(|i| *((p + i * 24 + 4) as *const u32) == me);
            assert!(found, "le processus de test figure dans la liste");
            crate::dlls::kernel32_misc::local_free(p as *mut std::ffi::c_void);

            let mut info = [0u8; 56];
            info[0..4].copy_from_slice(&56u32.to_le_bytes());
            assert_eq!(LoadUserProfileW(0, info.as_mut_ptr()), 1);
            assert_eq!(
                usize::from_le_bytes(info[48..56].try_into().unwrap()),
                0x8000_0001
            );

            let mut buf = [0u16; 32];
            let mut size = 32u32;
            assert_eq!(GetAllUsersProfileDirectoryW(buf.as_mut_ptr(), &mut size), 1);
            assert_eq!(crate::dlls::wstr(buf.as_ptr()), "C:\\ProgramData");
        }
    }
}
