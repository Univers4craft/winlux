//! Sécurité Windows tirée des droits Linux : jeton du processus, SID,
//! appartenance aux groupes, élévation (UAC) et descripteurs de sécurité
//! des fichiers.
//!
//! Correspondance :
//! - utilisateur Linux d'uid N → `S-1-5-21-<machine>-N` (N ≥ 1000, sinon
//!   N + 0x40000), root → `S-1-5-18` (SYSTEM) ;
//! - groupe Linux de gid N → `S-1-5-21-<machine>-(N + 0x30000)` ;
//! - « autres » → `S-1-1-0` (Tout le monde) ;
//! - bits rwx → droits FILE_GENERIC_READ / WRITE / EXECUTE ; le propriétaire
//!   reçoit en plus READ_CONTROL, WRITE_DAC, WRITE_OWNER et DELETE, comme
//!   le « contrôle total » de Windows quand il a rwx.
//!
//! Le jeton est « administrateur » (élevé) seulement après une demande UAC
//! acceptée (`WINLUX_ELEVATED=1`, posé par `uac.rs`) ou pour root. Un compte
//! membre de sudo/wheel/admin a, avant élévation, un jeton « limité »
//! (TokenElevationTypeLimited), exactement comme un administrateur Windows.
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::os::unix::fs::MetadataExt;

const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NONE_MAPPED: u32 = 1332;

/// Pseudo-handle de jeton : CloseHandle l'ignore sans dommage.
pub const TOKEN_HANDLE: usize = 0x5EC0_0001;

const SE_GROUP_MANDATORY: u32 = 0x1;
const SE_GROUP_ENABLED_BY_DEFAULT: u32 = 0x2;
const SE_GROUP_ENABLED: u32 = 0x4;
const SE_GROUP_OWNER: u32 = 0x8;
const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x10;
const SE_GROUP_INTEGRITY: u32 = 0x20;
const SE_GROUP_LOGON_ID: u32 = 0xC000_0000;
const GROUP_ON: u32 = SE_GROUP_MANDATORY | SE_GROUP_ENABLED_BY_DEFAULT | SE_GROUP_ENABLED;

// ---------- SID ----------

/// SID sous forme lisible (`S-1-5-32-544`) → octets binaires.
pub fn sid_bytes(text: &str) -> Option<Vec<u8>> {
    let mut parts = text.strip_prefix("S-")?.split('-');
    let rev: u8 = parts.next()?.parse().ok()?;
    let auth: u64 = parts.next()?.parse().ok()?;
    let subs: Vec<u32> = parts.map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if rev != 1 || subs.len() > 15 || auth >= 1 << 48 {
        return None;
    }
    let mut b = vec![1u8, subs.len() as u8];
    b.extend_from_slice(&auth.to_be_bytes()[2..]);
    for s in subs {
        b.extend_from_slice(&s.to_le_bytes());
    }
    Some(b)
}

/// Octets d'un SID → texte. `None` si la structure est invalide.
pub fn sid_text(b: &[u8]) -> Option<String> {
    if b.len() < 8 || b[0] != 1 || b[1] > 15 || b.len() < 8 + 4 * b[1] as usize {
        return None;
    }
    let mut auth = [0u8; 8];
    auth[2..].copy_from_slice(&b[2..8]);
    let mut s = format!("S-1-{}", u64::from_be_bytes(auth));
    for i in 0..b[1] as usize {
        let o = 8 + i * 4;
        s.push_str(&format!(
            "-{}",
            u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
        ));
    }
    Some(s)
}

/// Lit un SID en mémoire (taille déduite de son en-tête).
unsafe fn sid_at<'a>(p: *const u8) -> Option<&'a [u8]> {
    if p.is_null() || *p != 1 || *p.add(1) > 15 {
        return None;
    }
    Some(std::slice::from_raw_parts(p, 8 + 4 * *p.add(1) as usize))
}

/// Trois sous-autorités propres à la machine, stables (machine-id).
fn machine_part() -> String {
    let id = std::fs::read_to_string("/etc/machine-id").unwrap_or_else(|_| "winlux".into());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.trim().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    let a = (h as u32) | 0x1000_0000;
    let b = ((h >> 32) as u32) | 0x1000_0000;
    let c = (h.rotate_left(17) as u32) | 0x1000_0000;
    format!("S-1-5-21-{a}-{b}-{c}")
}

pub fn uid_sid(uid: u32) -> String {
    if uid == 0 {
        return "S-1-5-18".into();
    }
    let rid = if uid >= 1000 { uid } else { uid + 0x40000 };
    format!("{}-{rid}", machine_part())
}

/// SID du compte qui fait tourner le processus (TokenUser).
pub fn user_sid() -> String {
    uid_sid(euid())
}

pub fn gid_sid(gid: u32) -> String {
    if gid == 0 {
        return "S-1-5-32-544".into();
    }
    format!("{}-{}", machine_part(), gid + 0x30000)
}

const EVERYONE: &str = "S-1-1-0";
const ADMINISTRATORS: &str = "S-1-5-32-544";
const USERS: &str = "S-1-5-32-545";

// ---------- Compte et élévation ----------

fn euid() -> u32 {
    // SAFETY: appel système sans argument.
    unsafe { libc::geteuid() }
}

/// Vrai si le processus a été élevé (UAC accepté) ou tourne en root.
pub fn elevated() -> bool {
    euid() == 0 || std::env::var("WINLUX_ELEVATED").is_ok_and(|v| v == "1")
}

/// Groupes Linux qui font d'un compte un administrateur.
const ADMIN_GROUPS: [&str; 4] = ["sudo", "wheel", "admin", "adm"];

/// Le compte courant peut-il devenir administrateur (comme un membre du
/// groupe Administrateurs sous Windows) ?
pub fn admin_capable() -> bool {
    if euid() == 0 {
        return true;
    }
    let user = crate::paths::user_name();
    if let Ok(text) = std::fs::read_to_string("/etc/winlux/users.conf") {
        for line in text.lines() {
            let mut f = line.splitn(3, ':');
            if f.next() == Some(user.as_str()) {
                if let Some(role) = f.next() {
                    if matches!(role, "root" | "admin" | "administrateur") {
                        return true;
                    }
                }
            }
        }
    }
    let Ok(groups) = std::fs::read_to_string("/etc/group") else {
        return false;
    };
    groups.lines().any(|l| {
        let f: Vec<&str> = l.split(':').collect();
        f.len() >= 4 && ADMIN_GROUPS.contains(&f[0]) && f[3].split(',').any(|m| m == user)
    })
}

/// Identifiant (LUID, partie basse) de la session d'ouverture : stable
/// pour la session Linux (getsid).
pub fn logon_luid() -> u32 {
    // SAFETY: getsid(0) est un appel système sans pointeur.
    0x0001_0000 + unsafe { libc::getsid(0) }.max(0) as u32
}

/// Groupes du jeton : (SID, attributs).
fn token_groups() -> Vec<(String, u32)> {
    let mut g = vec![
        (EVERYONE.to_string(), GROUP_ON),
        ("S-1-5-4".to_string(), GROUP_ON),  // INTERACTIF
        ("S-1-5-11".to_string(), GROUP_ON), // Utilisateurs authentifiés
        (USERS.to_string(), GROUP_ON),
    ];
    if elevated() {
        g.push((ADMINISTRATORS.into(), GROUP_ON | SE_GROUP_OWNER));
    } else if admin_capable() {
        g.push((ADMINISTRATORS.into(), SE_GROUP_USE_FOR_DENY_ONLY));
    }
    // SAFETY: getgid est un appel système sans argument.
    g.push((gid_sid(unsafe { libc::getegid() }), GROUP_ON));
    // SID de la session d'ouverture (S-1-5-5-0-n) : le bac à sable de
    // Chromium le cherche pour restreindre les jetons de ses enfants.
    // SAFETY: getsid(0) est un appel système sans pointeur.
    let session = unsafe { libc::getsid(0) }.max(0) as u32;
    g.push((format!("S-1-5-5-0-{session}"), GROUP_ON | SE_GROUP_LOGON_ID));
    g
}

// ---------- Jeton ----------

pub unsafe extern "win64" fn OpenProcessToken(
    _process: usize,
    _access: u32,
    token: *mut usize,
) -> i32 {
    if token.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *token = TOKEN_HANDLE;
    1
}

/// Données d'une classe de jeton : (octets, liste des décalages où écrire
/// l'adresse absolue d'un SID : (position du pointeur, position du SID)).
fn token_info(class: u32) -> Option<(Vec<u8>, Vec<(usize, usize)>)> {
    let user = sid_bytes(&uid_sid(euid()))?;
    let dword = |v: u32| Some((v.to_le_bytes().to_vec(), vec![]));
    // Structure « un SID_AND_ATTRIBUTES ou un PSID suivi du SID ».
    let one = |sid: Vec<u8>, attrs: Option<u32>| {
        let head = if attrs.is_some() { 16 } else { 8 };
        let mut b = vec![0u8; head];
        if let Some(a) = attrs {
            b[8..12].copy_from_slice(&a.to_le_bytes());
        }
        b.extend_from_slice(&sid);
        Some((b, vec![(0, head)]))
    };
    match class {
        1 => one(user, Some(0)), // TokenUser
        4 => one(user, None),    // TokenOwner
        5 => one(sid_bytes(&gid_sid(unsafe { libc::getegid() }))?, None), // TokenPrimaryGroup
        2 | 28 => {
            // TokenGroups, ou TokenLogonSid (le seul SID de session) : le bac
            // à sable de Chromium lit ce dernier pour verrouiller la DACL par
            // défaut des jetons de ses enfants ; une erreur autre que
            // ERROR_NOT_FOUND y faisait échouer tout lancement (#747).
            let mut groups = token_groups();
            if class == 28 {
                groups.retain(|g| g.1 & SE_GROUP_LOGON_ID != 0);
            }
            let head = 8 + 16 * groups.len();
            let mut b = vec![0u8; head];
            b[0..4].copy_from_slice(&(groups.len() as u32).to_le_bytes());
            let mut fix = vec![];
            for (i, (sid, attrs)) in groups.iter().enumerate() {
                let entry = 8 + 16 * i;
                b[entry + 8..entry + 12].copy_from_slice(&attrs.to_le_bytes());
                fix.push((entry, b.len()));
                b.extend_from_slice(&sid_bytes(sid)?);
            }
            Some((b, fix))
        }
        3 => {
            // TokenPrivileges : ceux d'un compte standard de Windows
            // (SeShutdown, SeChangeNotify activé, SeUndock,
            // SeIncreaseWorkingSet, SeTimeZone). Le courtier du bac à sable
            // de Chromium refuse une liste plus courte qu'un
            // TOKEN_PRIVILEGES (16 octets) et n'a alors aucun renderer (#1352).
            const PRIVS: [(u32, u32); 5] = [(19, 0), (23, 3), (25, 0), (33, 0), (34, 0)];
            let mut b = (PRIVS.len() as u32).to_le_bytes().to_vec();
            for (luid, attrs) in PRIVS {
                b.extend_from_slice(&luid.to_le_bytes());
                b.extend_from_slice(&0u32.to_le_bytes());
                b.extend_from_slice(&attrs.to_le_bytes());
            }
            Some((b, vec![]))
        }
        6 => {
            // TokenDefaultDacl : pointeur vers une ACL vide (révision 2).
            let mut b = vec![0u8; 8];
            b.extend_from_slice(&[2, 0, 8, 0, 0, 0, 0, 0]);
            Some((b, vec![(0, 8)]))
        }
        9 => dword(2), // TokenImpersonationLevel : SecurityImpersonation
        10 => {
            // TokenStatistics (56 octets) : jeton primaire, aucun compteur,
            // la session d'ouverture en AuthenticationId.
            let mut b = vec![0u8; 56];
            b[8..12].copy_from_slice(&logon_luid().to_le_bytes());
            b[32..36].copy_from_slice(&1u32.to_le_bytes());
            Some((b, vec![]))
        }
        11 => dword(0).map(|(mut b, f)| {
            b.extend_from_slice(&[0; 4]); // TokenRestrictedSids : aucun
            (b, f)
        }),
        29 => dword(0), // TokenIsAppContainer
        42 => dword(0), // TokenIsRestricted
        8 => dword(1),  // TokenType : primaire
        12 => dword(1), // TokenSessionId
        18 => dword(if elevated() {
            2 // TokenElevationTypeFull
        } else if admin_capable() {
            3 // TokenElevationTypeLimited
        } else {
            1 // TokenElevationTypeDefault
        }),
        20 => dword(elevated() as u32), // TokenElevation
        25 => {
            // TokenIntegrityLevel : élevé (0x3000) ou moyen (0x2000).
            let level = if elevated() {
                "S-1-16-12288"
            } else {
                "S-1-16-8192"
            };
            one(sid_bytes(level)?, Some(SE_GROUP_INTEGRITY))
        }
        _ => None,
    }
}

pub unsafe extern "win64" fn GetTokenInformation(
    _token: usize,
    class: u32,
    buf: *mut u8,
    len: u32,
    ret: *mut u32,
) -> i32 {
    let Some((bytes, fix)) = token_info(class) else {
        crate::trace!("GetTokenInformation : classe {class} non prise en charge");
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if !ret.is_null() {
        *ret = bytes.len() as u32;
    }
    if buf.is_null() || (len as usize) < bytes.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    for (ptr_at, sid_at) in fix {
        std::ptr::write_unaligned(buf.add(ptr_at) as *mut usize, buf as usize + sid_at);
    }
    1
}

pub unsafe extern "win64" fn CheckTokenMembership(
    _token: usize,
    sid: *const u8,
    member: *mut i32,
) -> i32 {
    let Some(text) = sid_at(sid).and_then(sid_text) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if member.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let me = uid_sid(euid());
    *member = (text == me
        || token_groups()
            .iter()
            .any(|(g, a)| *g == text && a & SE_GROUP_ENABLED != 0)) as i32;
    1
}

/// `IsUserAnAdmin` (shell32) : vrai seulement une fois élevé.
pub extern "win64" fn IsUserAnAdmin() -> i32 {
    elevated() as i32
}

/// Privilèges : Winlux n'en restreint aucun, on répond « tout accordé ».
pub unsafe extern "win64" fn AdjustTokenPrivileges(
    _token: usize,
    _disable_all: i32,
    _new: *const c_void,
    _len: u32,
    _prev: *mut c_void,
    ret: *mut u32,
) -> i32 {
    if !ret.is_null() {
        *ret = 0;
    }
    crate::process::set_last_error(0);
    1
}

const PRIVILEGES: [&str; 8] = [
    "SeCreateTokenPrivilege",
    "SeShutdownPrivilege",
    "SeBackupPrivilege",
    "SeRestorePrivilege",
    "SeDebugPrivilege",
    "SeChangeNotifyPrivilege",
    "SeIncreaseQuotaPrivilege",
    "SeSecurityPrivilege",
];

fn privilege_luid(name: &str) -> Option<u32> {
    // Valeurs LUID de winnt.h.
    let luid = [2, 19, 17, 18, 20, 23, 5, 8];
    PRIVILEGES
        .iter()
        .position(|p| p.eq_ignore_ascii_case(name))
        .map(|i| luid[i])
}

unsafe fn lookup_privilege(name: String, luid: *mut u32) -> i32 {
    match privilege_luid(&name) {
        Some(v) if !luid.is_null() => {
            *luid = v;
            *luid.add(1) = 0;
            1
        }
        _ => {
            crate::process::set_last_error(1313); // ERROR_NO_SUCH_PRIVILEGE
            0
        }
    }
}

pub unsafe extern "win64" fn LookupPrivilegeValueW(
    _sys: *const u16,
    name: *const u16,
    luid: *mut u32,
) -> i32 {
    lookup_privilege(crate::dlls::wstr(name), luid)
}

pub unsafe extern "win64" fn LookupPrivilegeValueA(
    _sys: *const u8,
    name: *const u8,
    luid: *mut u32,
) -> i32 {
    lookup_privilege(crate::dlls::cstr(name), luid)
}

// ---------- Fonctions SID ----------

/// Mémoire libérable par `FreeSid` / `LocalFree` (tas Winlux).
/// Disposition d'EXPLICIT_ACCESS et de son TRUSTEE selon la taille de
/// pointeur du programme (#1915). x64 : TRUSTEE à 16 (forme 28, nom 40),
/// 48 octets ; x86 : TRUSTEE à 12 (forme 20, nom 28), 32 octets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AccessLayout {
    pub size: usize,
    pub form: usize,
    pub kind: usize,
    pub name: usize,
}

pub(crate) fn access_layout(ptr: usize) -> AccessLayout {
    let (t, l) = trustee_layout(ptr);
    AccessLayout {
        size: t + l.size,
        form: t + l.form,
        kind: t + l.kind,
        name: t + l.name,
    }
}

/// (décalage du TRUSTEE dans EXPLICIT_ACCESS, disposition du TRUSTEE seul).
pub(crate) fn trustee_layout(ptr: usize) -> (usize, AccessLayout) {
    if ptr == 4 {
        (
            12,
            AccessLayout {
                size: 20,
                form: 8,
                kind: 12,
                name: 16,
            },
        )
    } else {
        (
            16,
            AccessLayout {
                size: 32,
                form: 12,
                kind: 16,
                name: 24,
            },
        )
    }
}

/// Lit une case pointeur de `ptr` octets.
unsafe fn read_ptr_of(at: *const u8, ptr: usize) -> usize {
    if ptr == 4 {
        std::ptr::read_unaligned(at as *const u32) as usize
    } else {
        std::ptr::read_unaligned(at as *const usize)
    }
}

unsafe fn heap_copy(b: &[u8]) -> *mut u8 {
    let p = crate::dlls::kernel32::heap_alloc(0, 0, b.len()) as *mut u8;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
    }
    p
}

pub unsafe extern "win64" fn AllocateAndInitializeSid(
    auth: *const u8,
    count: u8,
    s0: u32,
    s1: u32,
    s2: u32,
    s3: u32,
    s4: u32,
    s5: u32,
    s6: u32,
    s7: u32,
    out: *mut *mut u8,
) -> i32 {
    if auth.is_null() || out.is_null() || count > 8 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut b = vec![1u8, count];
    b.extend_from_slice(std::slice::from_raw_parts(auth, 6));
    for s in [s0, s1, s2, s3, s4, s5, s6, s7].iter().take(count as usize) {
        b.extend_from_slice(&s.to_le_bytes());
    }
    *out = heap_copy(&b);
    (!(*out).is_null()) as i32
}

pub unsafe extern "win64" fn FreeSid(sid: *mut c_void) -> *mut c_void {
    if !sid.is_null() {
        crate::dlls::kernel32::heap_free(0, 0, sid);
    }
    std::ptr::null_mut()
}

pub unsafe extern "win64" fn IsValidSid(sid: *const u8) -> i32 {
    sid_at(sid).is_some() as i32
}

pub unsafe extern "win64" fn GetLengthSid(sid: *const u8) -> u32 {
    sid_at(sid).map_or(0, |s| s.len() as u32)
}

pub unsafe extern "win64" fn EqualSid(a: *const u8, b: *const u8) -> i32 {
    match (sid_at(a), sid_at(b)) {
        (Some(x), Some(y)) => (x == y) as i32,
        _ => 0,
    }
}

pub unsafe extern "win64" fn CopySid(len: u32, dst: *mut u8, src: *const u8) -> i32 {
    let Some(s) = sid_at(src) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if dst.is_null() || (len as usize) < s.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len());
    1
}

pub unsafe extern "win64" fn GetSidSubAuthorityCount(sid: *mut u8) -> *mut u8 {
    sid.add(1)
}

pub unsafe extern "win64" fn GetSidSubAuthority(sid: *mut u8, index: u32) -> *mut u32 {
    sid.add(8 + 4 * index as usize) as *mut u32
}

pub unsafe extern "win64" fn GetSidIdentifierAuthority(sid: *mut u8) -> *mut u8 {
    sid.add(2)
}

pub unsafe extern "win64" fn ConvertSidToStringSidW(sid: *const u8, out: *mut *mut u16) -> i32 {
    let Some(text) = sid_at(sid).and_then(sid_text) else {
        crate::process::set_last_error(1337); // ERROR_INVALID_SID
        return 0;
    };
    let w: Vec<u8> = text
        .encode_utf16()
        .chain([0])
        .flat_map(|c| c.to_le_bytes())
        .collect();
    *out = heap_copy(&w) as *mut u16;
    1
}

pub unsafe extern "win64" fn ConvertSidToStringSidA(sid: *const u8, out: *mut *mut u8) -> i32 {
    let Some(text) = sid_at(sid).and_then(sid_text) else {
        crate::process::set_last_error(1337);
        return 0;
    };
    let mut b = text.into_bytes();
    b.push(0);
    *out = heap_copy(&b);
    1
}

unsafe fn string_to_sid(text: &str, out: *mut *mut u8) -> i32 {
    let alias = match text {
        "WD" => EVERYONE,
        "BA" => ADMINISTRATORS,
        "BU" => USERS,
        "SY" => "S-1-5-18",
        "AU" => "S-1-5-11",
        t => t,
    };
    match sid_bytes(alias) {
        Some(b) if !out.is_null() => {
            *out = heap_copy(&b);
            1
        }
        _ => {
            crate::process::set_last_error(1337);
            0
        }
    }
}

pub unsafe extern "win64" fn ConvertStringSidToSidW(text: *const u16, out: *mut *mut u8) -> i32 {
    string_to_sid(&crate::dlls::wstr(text), out)
}

pub unsafe extern "win64" fn ConvertStringSidToSidA(text: *const u8, out: *mut *mut u8) -> i32 {
    string_to_sid(&crate::dlls::cstr(text), out)
}

/// Nom de compte d'un SID : (nom, domaine, type SID_NAME_USE).
fn account_of(sid: &str) -> Option<(String, String, u32)> {
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_uppercase())
        .unwrap_or_else(|_| "WINLUX".into());
    Some(match sid {
        EVERYONE => ("Everyone".into(), String::new(), 5),
        ADMINISTRATORS => ("Administrators".into(), "BUILTIN".into(), 4),
        USERS => ("Users".into(), "BUILTIN".into(), 4),
        "S-1-5-18" => ("SYSTEM".into(), "NT AUTHORITY".into(), 5),
        // Comptes et groupes bien connus (documentation publique « Well-known
        // SIDs ») : un installeur les résout avant d'écrire une ACL (#1639).
        // SidTypeWellKnownGroup = 5, SidTypeAlias = 4.
        "S-1-5-4" => ("INTERACTIVE".into(), "NT AUTHORITY".into(), 5),
        "S-1-5-6" => ("SERVICE".into(), "NT AUTHORITY".into(), 5),
        "S-1-5-11" => ("Authenticated Users".into(), "NT AUTHORITY".into(), 5),
        "S-1-5-19" => ("LOCAL SERVICE".into(), "NT AUTHORITY".into(), 5),
        "S-1-5-20" => ("NETWORK SERVICE".into(), "NT AUTHORITY".into(), 5),
        "S-1-5-32-546" => ("Guests".into(), "BUILTIN".into(), 4),
        "S-1-5-32-547" => ("Power Users".into(), "BUILTIN".into(), 4),
        "S-1-5-32-555" => ("Remote Desktop Users".into(), "BUILTIN".into(), 4),
        "S-1-5-32-558" => ("Performance Monitor Users".into(), "BUILTIN".into(), 4),
        "S-1-5-32-559" => ("Performance Log Users".into(), "BUILTIN".into(), 4),
        s if s == uid_sid(euid()) => (crate::paths::user_name(), host, 1),
        _ => return None,
    })
}

unsafe fn put_name_w(s: &str, buf: *mut u16, len: *mut u32) -> bool {
    let w: Vec<u16> = s.encode_utf16().collect();
    let fits = !buf.is_null() && (*len as usize) > w.len();
    if fits {
        std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
        *buf.add(w.len()) = 0;
        *len = w.len() as u32;
    } else {
        *len = w.len() as u32 + 1;
    }
    fits
}

pub unsafe extern "win64" fn LookupAccountSidW(
    _system: *const u16,
    sid: *const u8,
    name: *mut u16,
    name_len: *mut u32,
    domain: *mut u16,
    domain_len: *mut u32,
    use_: *mut u32,
) -> i32 {
    let Some((n, d, kind)) = sid_at(sid).and_then(sid_text).and_then(|t| account_of(&t)) else {
        crate::process::set_last_error(ERROR_NONE_MAPPED);
        return 0;
    };
    if name_len.is_null() || domain_len.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let a = put_name_w(&n, name, name_len);
    let b = put_name_w(&d, domain, domain_len);
    if !use_.is_null() {
        *use_ = kind;
    }
    if a && b {
        1
    } else {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        0
    }
}

// ---------- Descripteurs de sécurité des fichiers ----------

const FILE_GENERIC_READ: u32 = 0x0012_0089;
const FILE_GENERIC_WRITE: u32 = 0x0012_0116;
const FILE_GENERIC_EXECUTE: u32 = 0x0012_00A0;
const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
const DELETE: u32 = 0x0001_0000;
const READ_CONTROL: u32 = 0x0002_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const FILE_DELETE_CHILD: u32 = 0x40;

/// Droits Windows d'un triplet rwx Linux.
pub fn rights_of(bits: u32, owner: bool, dir: bool) -> u32 {
    if owner && bits & 7 == 7 {
        return FILE_ALL_ACCESS;
    }
    let mut m = 0;
    if bits & 4 != 0 {
        m |= FILE_GENERIC_READ;
    }
    if bits & 2 != 0 {
        m |= FILE_GENERIC_WRITE;
        if dir && bits & 1 != 0 {
            m |= FILE_DELETE_CHILD | DELETE;
        }
    }
    if bits & 1 != 0 {
        m |= FILE_GENERIC_EXECUTE;
    }
    if owner {
        m |= READ_CONTROL | WRITE_DAC | WRITE_OWNER | DELETE;
    }
    m
}

fn ace(mask: u32, sid: &[u8], dir: bool) -> Vec<u8> {
    let size = 8 + sid.len();
    // Sur un dossier, les entrées s'héritent aux fichiers et sous-dossiers.
    let flags = if dir { 0x3 } else { 0 };
    let mut b = vec![0u8, flags];
    b.extend_from_slice(&(size as u16).to_le_bytes());
    b.extend_from_slice(&mask.to_le_bytes());
    b.extend_from_slice(sid);
    b
}

/// Descripteur auto-relatif décrivant un fichier Linux.
/// `info` : OWNER (1), GROUP (2), DACL (4) demandés.
pub fn file_descriptor(path: &std::path::Path, info: u32) -> std::io::Result<Vec<u8>> {
    let m = std::fs::metadata(path)?;
    let dir = m.is_dir();
    let mode = m.mode();
    let owner = sid_bytes(&uid_sid(m.uid())).unwrap_or_default();
    let group = sid_bytes(&gid_sid(m.gid())).unwrap_or_default();
    let mut aces: Vec<Vec<u8>> = vec![];
    for (bits, sid, is_owner) in [
        ((mode >> 6) & 7, owner.clone(), true),
        ((mode >> 3) & 7, group.clone(), false),
        (mode & 7, sid_bytes(EVERYONE).unwrap_or_default(), false),
    ] {
        let mask = rights_of(bits, is_owner, dir);
        if mask != 0 {
            aces.push(ace(mask, &sid, dir));
        }
    }
    // root peut tout faire sous Linux : SYSTEM a le contrôle total.
    if m.uid() != 0 {
        aces.push(ace(
            FILE_ALL_ACCESS,
            &sid_bytes("S-1-5-18").unwrap_or_default(),
            dir,
        ));
    }
    let mut out = vec![0u8; 20];
    out[0] = 1;
    let mut control: u16 = 0x8000; // SE_SELF_RELATIVE
    if info & 1 != 0 {
        let off = out.len() as u32;
        out[4..8].copy_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&owner);
    }
    if info & 2 != 0 {
        let off = out.len() as u32;
        out[8..12].copy_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&group);
    }
    if info & 4 != 0 {
        control |= 0x4; // SE_DACL_PRESENT
        let off = out.len() as u32;
        out[16..20].copy_from_slice(&off.to_le_bytes());
        let size: usize = 8 + aces.iter().map(Vec::len).sum::<usize>();
        out.extend_from_slice(&[2, 0]);
        out.extend_from_slice(&(size as u16).to_le_bytes());
        out.extend_from_slice(&(aces.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        for a in aces {
            out.extend_from_slice(&a);
        }
    }
    out[2..4].copy_from_slice(&control.to_le_bytes());
    Ok(out)
}

unsafe fn get_file_security(name: &str, info: u32, sd: *mut u8, len: u32, needed: *mut u32) -> i32 {
    let d = match file_descriptor(&crate::paths::to_linux(name), info) {
        Ok(d) => d,
        Err(e) => {
            crate::process::set_last_error(crate::dlls::kernel32::io_error_code(&e));
            return 0;
        }
    };
    if !needed.is_null() {
        *needed = d.len() as u32;
    }
    if sd.is_null() || (len as usize) < d.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(d.as_ptr(), sd, d.len());
    1
}

pub unsafe extern "win64" fn GetFileSecurityW(
    name: *const u16,
    info: u32,
    sd: *mut u8,
    len: u32,
    needed: *mut u32,
) -> i32 {
    get_file_security(&crate::dlls::wstr(name), info, sd, len, needed)
}

pub unsafe extern "win64" fn GetFileSecurityA(
    name: *const u8,
    info: u32,
    sd: *mut u8,
    len: u32,
    needed: *mut u32,
) -> i32 {
    get_file_security(&crate::dlls::cstr(name), info, sd, len, needed)
}

/// Pointeur vers une partie d'un descripteur (0 = absente). `field` est
/// le décalage auto-relatif (4 propriétaire, 8 groupe, 12 SACL, 16 DACL) ;
/// un descripteur absolu (InitializeSecurityDescriptor, sans
/// SE_SELF_RELATIVE) range des pointeurs à 8, 16, 24 et 32.
unsafe fn sd_part(sd: *const u8, field: usize) -> *mut u8 {
    let control = std::ptr::read_unaligned(sd.add(2) as *const u16);
    if control & SE_SELF_RELATIVE == 0 {
        return read_abs_slot(sd, field);
    }
    let off = std::ptr::read_unaligned(sd.add(field) as *const u32) as usize;
    if off == 0 {
        std::ptr::null_mut()
    } else {
        sd.add(off) as *mut u8
    }
}

/// Parties d'un descripteur (absolu ou auto-relatif) : propriétaire,
/// groupe, SACL et DACL, copiées.
pub(crate) unsafe fn sd_parts(sd: *const u8) -> [Option<Vec<u8>>; 4] {
    let part = |field: usize| -> Option<Vec<u8>> {
        let p = sd_part(sd, field);
        if p.is_null() {
            return None;
        }
        if field == 12 || field == 16 {
            let len = std::ptr::read_unaligned(p.add(2) as *const u16) as usize;
            (len >= 8).then(|| std::slice::from_raw_parts(p, len).to_vec())
        } else {
            sid_at(p).map(<[u8]>::to_vec)
        }
    };
    [part(4), part(8), part(12), part(16)]
}

/// Descripteur auto-relatif d'après ses parties (ordre de `sd_parts`) et
/// les bits de contrôle hors SE_SELF_RELATIVE et *_PRESENT.
pub(crate) fn self_relative_sd(parts: &[Option<Vec<u8>>; 4], control: u16) -> Vec<u8> {
    let mut sd = vec![0u8; 20];
    sd[0] = 1;
    let mut c = (control & !(SE_SELF_RELATIVE | SE_DACL_PRESENT | 0x10)) | SE_SELF_RELATIVE;
    for (i, field) in [4usize, 8, 12, 16].into_iter().enumerate() {
        if let Some(bytes) = &parts[i] {
            let off = sd.len() as u32;
            sd[field..field + 4].copy_from_slice(&off.to_le_bytes());
            sd.extend_from_slice(bytes);
            match field {
                12 => c |= 0x10, // SE_SACL_PRESENT
                16 => c |= SE_DACL_PRESENT,
                _ => {}
            }
        }
    }
    sd[2..4].copy_from_slice(&c.to_le_bytes());
    sd
}

unsafe fn named_security_info(
    name: &str,
    kind: u32,
    info: u32,
    owner: *mut *mut u8,
    group: *mut *mut u8,
    dacl: *mut *mut u8,
    sacl: *mut *mut u8,
    sd: *mut *mut u8,
) -> u32 {
    if kind != 1 {
        return ERROR_INVALID_PARAMETER; // seul SE_FILE_OBJECT est géré
    }
    let d = match file_descriptor(&crate::paths::to_linux(name), info) {
        Ok(d) => d,
        Err(e) => return crate::dlls::kernel32::io_error_code(&e),
    };
    let p = heap_copy(&d);
    if p.is_null() {
        return 8; // ERROR_NOT_ENOUGH_MEMORY
    }
    for (out, field) in [(owner, 4), (group, 8), (dacl, 16)] {
        if !out.is_null() {
            *out = sd_part(p, field);
        }
    }
    if !sacl.is_null() {
        *sacl = std::ptr::null_mut();
    }
    if sd.is_null() {
        // L'appelant ne pourra pas libérer : les pointeurs restent valides
        // pour la durée du processus, comme une fuite côté Windows.
    } else {
        *sd = p;
    }
    0
}

pub unsafe extern "win64" fn GetNamedSecurityInfoW(
    name: *const u16,
    kind: u32,
    info: u32,
    owner: *mut *mut u8,
    group: *mut *mut u8,
    dacl: *mut *mut u8,
    sacl: *mut *mut u8,
    sd: *mut *mut u8,
) -> u32 {
    named_security_info(
        &crate::dlls::wstr(name),
        kind,
        info,
        owner,
        group,
        dacl,
        sacl,
        sd,
    )
}

pub unsafe extern "win64" fn GetNamedSecurityInfoA(
    name: *const u8,
    kind: u32,
    info: u32,
    owner: *mut *mut u8,
    group: *mut *mut u8,
    dacl: *mut *mut u8,
    sacl: *mut *mut u8,
    sd: *mut *mut u8,
) -> u32 {
    named_security_info(
        &crate::dlls::cstr(name),
        kind,
        info,
        owner,
        group,
        dacl,
        sacl,
        sd,
    )
}

pub unsafe extern "win64" fn GetSecurityDescriptorOwner(
    sd: *const u8,
    owner: *mut *mut u8,
    defaulted: *mut i32,
) -> i32 {
    if sd.is_null() || owner.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *owner = sd_part(sd, 4);
    if !defaulted.is_null() {
        *defaulted = 0;
    }
    1
}

pub unsafe extern "win64" fn GetSecurityDescriptorGroup(
    sd: *const u8,
    group: *mut *mut u8,
    defaulted: *mut i32,
) -> i32 {
    if sd.is_null() || group.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *group = sd_part(sd, 8);
    if !defaulted.is_null() {
        *defaulted = 0;
    }
    1
}

pub unsafe extern "win64" fn GetSecurityDescriptorDacl(
    sd: *const u8,
    present: *mut i32,
    dacl: *mut *mut u8,
    defaulted: *mut i32,
) -> i32 {
    if sd.is_null() || present.is_null() || dacl.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let control = std::ptr::read_unaligned(sd.add(2) as *const u16);
    *present = (control & 0x4 != 0) as i32;
    *dacl = sd_part(sd, 16);
    if !defaulted.is_null() {
        *defaulted = 0;
    }
    1
}

pub unsafe extern "win64" fn GetSecurityDescriptorLength(sd: *const u8) -> u32 {
    if sd.is_null() {
        return 0;
    }
    let mut end = 20usize;
    for field in [4usize, 8, 16] {
        let off = std::ptr::read_unaligned(sd.add(field) as *const u32) as usize;
        if off == 0 {
            continue;
        }
        let len = if field == 16 {
            std::ptr::read_unaligned(sd.add(off + 2) as *const u16) as usize
        } else {
            sid_at(sd.add(off)).map_or(0, |s| s.len())
        };
        end = end.max(off + len);
    }
    end as u32
}

pub unsafe extern "win64" fn IsValidSecurityDescriptor(sd: *const u8) -> i32 {
    (!sd.is_null() && *sd == 1) as i32
}

pub unsafe extern "win64" fn GetAce(acl: *const u8, index: u32, out: *mut *mut u8) -> i32 {
    if acl.is_null() || out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let count = std::ptr::read_unaligned(acl.add(4) as *const u16) as u32;
    if index >= count {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut p = acl.add(8);
    for _ in 0..index {
        p = p.add(std::ptr::read_unaligned(p.add(2) as *const u16) as usize);
    }
    *out = p as *mut u8;
    1
}

pub unsafe extern "win64" fn GetAclInformation(
    acl: *const u8,
    out: *mut u32,
    len: u32,
    class: u32,
) -> i32 {
    if acl.is_null() || out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let size = std::ptr::read_unaligned(acl.add(2) as *const u16) as u32;
    let count = std::ptr::read_unaligned(acl.add(4) as *const u16) as u32;
    match class {
        1 if len >= 4 => *out = 2, // AclRevisionInformation
        2 if len >= 12 => {
            *out = count;
            *out.add(1) = size;
            *out.add(2) = 0;
        }
        _ => {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
    }
    1
}

const SE_SELF_RELATIVE: u16 = 0x8000;
const SE_DACL_PRESENT: u16 = 0x4;
const SE_DACL_DEFAULTED: u16 = 0x8;
const SE_OWNER_DEFAULTED: u16 = 0x1;
/// Taille d'un SECURITY_DESCRIPTOR absolu : 40 octets en x64, 20 pour un
/// programme x86 (pointeurs sur 4 octets). Écrire 40 octets dans la
/// structure x86, souvent sur la pile, écrasait les variables voisines :
/// SteamService.exe abandonnait l'installation du service (#1881).
fn absolute_sd_size(ptr: usize) -> usize {
    if ptr == 4 {
        20
    } else {
        40
    }
}

/// Case d'un descripteur absolu pour la partie `field` (décalage
/// auto-relatif : 4 propriétaire, 8 groupe, 12 SACL, 16 DACL) : x64 aux
/// décalages 8, 16, 24, 32 ; x86 aux mêmes décalages que l'auto-relatif.
fn abs_slot(field: usize, ptr: usize) -> usize {
    if ptr == 4 {
        field
    } else {
        field * 2
    }
}

pub(crate) unsafe fn read_abs_slot(sd: *const u8, field: usize) -> *mut u8 {
    let ptr = crate::wow32::pointer_size();
    let at = sd.add(abs_slot(field, ptr));
    if ptr == 4 {
        std::ptr::read_unaligned(at as *const u32) as usize as *mut u8
    } else {
        std::ptr::read_unaligned(at as *const *mut u8)
    }
}

pub(crate) unsafe fn write_abs_slot(sd: *mut u8, field: usize, value: *mut u8) {
    write_abs_slot_for(sd, field, value, crate::wow32::pointer_size());
}

unsafe fn write_abs_slot_for(sd: *mut u8, field: usize, value: *mut u8, ptr: usize) {
    let at = sd.add(abs_slot(field, ptr));
    if ptr == 4 {
        std::ptr::write_unaligned(at as *mut u32, value as usize as u32);
    } else {
        std::ptr::write_unaligned(at as *mut *mut u8, value);
    }
}

/// InitializeSecurityDescriptor : descripteur absolu vide (révision 1).
pub unsafe extern "win64" fn InitializeSecurityDescriptor(sd: *mut u8, revision: u32) -> i32 {
    if sd.is_null() || revision != 1 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    std::ptr::write_bytes(sd, 0, absolute_sd_size(crate::wow32::pointer_size()));
    *sd = 1;
    1
}

unsafe fn absolute_sd(sd: *mut u8) -> Option<*mut u16> {
    if sd.is_null() || *sd != 1 {
        crate::process::set_last_error(1338); // ERROR_INVALID_SECURITY_DESCR
        return None;
    }
    let control = sd.add(2) as *mut u16;
    if std::ptr::read_unaligned(control) & SE_SELF_RELATIVE != 0 {
        crate::process::set_last_error(1340); // ERROR_BAD_DESCRIPTOR_FORMAT
        return None;
    }
    Some(control)
}

pub unsafe extern "win64" fn SetSecurityDescriptorDacl(
    sd: *mut u8,
    present: i32,
    dacl: *mut u8,
    defaulted: i32,
) -> i32 {
    let Some(control) = absolute_sd(sd) else {
        return 0;
    };
    let mut c = std::ptr::read_unaligned(control) & !(SE_DACL_PRESENT | SE_DACL_DEFAULTED);
    if present != 0 {
        c |= SE_DACL_PRESENT;
        if defaulted != 0 {
            c |= SE_DACL_DEFAULTED;
        }
    }
    std::ptr::write_unaligned(control, c);
    write_abs_slot(
        sd,
        16,
        if present != 0 {
            dacl
        } else {
            std::ptr::null_mut()
        },
    );
    1
}

pub unsafe extern "win64" fn SetSecurityDescriptorOwner(
    sd: *mut u8,
    owner: *mut u8,
    defaulted: i32,
) -> i32 {
    let Some(control) = absolute_sd(sd) else {
        return 0;
    };
    let mut c = std::ptr::read_unaligned(control) & !SE_OWNER_DEFAULTED;
    if defaulted != 0 {
        c |= SE_OWNER_DEFAULTED;
    }
    std::ptr::write_unaligned(control, c);
    write_abs_slot(sd, 4, owner);
    1
}

pub unsafe extern "win64" fn SetSecurityDescriptorGroup(
    sd: *mut u8,
    group: *mut u8,
    _defaulted: i32,
) -> i32 {
    if absolute_sd(sd).is_none() {
        return 0;
    }
    write_abs_slot(sd, 8, group);
    1
}

/// InitializeAcl : en-tête d'une ACL vide (révision 2 ou 4).
pub unsafe extern "win64" fn InitializeAcl(acl: *mut u8, len: u32, revision: u32) -> i32 {
    if acl.is_null() || len < 8 || !(2..=4).contains(&revision) {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    std::ptr::write_bytes(acl, 0, 8);
    *acl = revision as u8;
    std::ptr::write_unaligned(acl.add(2) as *mut u16, len.min(0xFFFF) as u16);
    1
}

unsafe fn add_ace(acl: *mut u8, kind: u8, flags: u8, mask: u32, sid: *const u8) -> i32 {
    let Some(sid) = sid_at(sid) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if acl.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let size = std::ptr::read_unaligned(acl.add(2) as *const u16) as usize;
    let count = std::ptr::read_unaligned(acl.add(4) as *const u16) as usize;
    let mut end = 8usize;
    for _ in 0..count {
        end += std::ptr::read_unaligned(acl.add(end + 2) as *const u16) as usize;
    }
    let len = 8 + sid.len();
    if end + len > size {
        crate::process::set_last_error(1344); // ERROR_ALLOTTED_SPACE_EXCEEDED
        return 0;
    }
    let p = acl.add(end);
    *p = kind;
    *p.add(1) = flags;
    std::ptr::write_unaligned(p.add(2) as *mut u16, len as u16);
    std::ptr::write_unaligned(p.add(4) as *mut u32, mask);
    std::ptr::copy_nonoverlapping(sid.as_ptr(), p.add(8), sid.len());
    std::ptr::write_unaligned(acl.add(4) as *mut u16, (count + 1) as u16);
    1
}

pub unsafe extern "win64" fn AddAccessAllowedAce(
    acl: *mut u8,
    _rev: u32,
    mask: u32,
    sid: *const u8,
) -> i32 {
    add_ace(acl, 0, 0, mask, sid)
}

pub unsafe extern "win64" fn AddAccessAllowedAceEx(
    acl: *mut u8,
    _rev: u32,
    flags: u32,
    mask: u32,
    sid: *const u8,
) -> i32 {
    add_ace(acl, 0, flags as u8, mask, sid)
}

pub unsafe extern "win64" fn AddAccessDeniedAce(
    acl: *mut u8,
    _rev: u32,
    mask: u32,
    sid: *const u8,
) -> i32 {
    add_ace(acl, 1, 0, mask, sid)
}

/// AddMandatoryAce : étiquette d'intégrité (SYSTEM_MANDATORY_LABEL_ACE,
/// type 0x11), le masque portant la politique (NO_WRITE_UP…). Le bac à
/// sable de Chromium en pose une sur l'objet de ses renderers (#1352).
pub unsafe extern "win64" fn AddMandatoryAce(
    acl: *mut u8,
    _rev: u32,
    flags: u32,
    policy: u32,
    sid: *const u8,
) -> i32 {
    add_ace(acl, 0x11, flags as u8, policy, sid)
}

/// SetEntriesInAcl : nouvelle ACL = entrées de l'ancienne + EXPLICIT_ACCESS
/// (GRANT/SET → autorisation, DENY → refus ; REVOKE retire le SID). Seuls
/// les administrés désignés par SID (TRUSTEE_IS_SID) ou par nom de compte
/// connu sont pris en compte. Libérée par LocalFree.
unsafe fn set_entries_in_acl(
    count: u32,
    entries: *const u8,
    old: *const u8,
    out: *mut *mut u8,
    wide: bool,
) -> u32 {
    set_entries_in_acl_for(count, entries, old, out, wide, crate::wow32::pointer_size())
}

/// SetEntriesInAcl pour un programme dont les pointeurs font `ptr` octets.
unsafe fn set_entries_in_acl_for(
    count: u32,
    entries: *const u8,
    old: *const u8,
    out: *mut *mut u8,
    wide: bool,
    ptr: usize,
) -> u32 {
    if out.is_null() || (count > 0 && entries.is_null()) {
        return ERROR_INVALID_PARAMETER;
    }
    let mut aces: Vec<(u8, u8, u32, Vec<u8>)> = Vec::new();
    if !old.is_null() {
        let n = std::ptr::read_unaligned(old.add(4) as *const u16) as usize;
        let mut off = 8usize;
        for _ in 0..n {
            let p = old.add(off);
            let len = std::ptr::read_unaligned(p.add(2) as *const u16) as usize;
            if let Some(sid) = sid_at(p.add(8)) {
                aces.push((
                    *p,
                    *p.add(1),
                    std::ptr::read_unaligned(p.add(4) as *const u32),
                    sid.to_vec(),
                ));
            }
            off += len.max(8);
        }
    }
    let lay = access_layout(ptr);
    for i in 0..count as usize {
        let e = entries.add(i * lay.size);
        let mask = std::ptr::read_unaligned(e as *const u32);
        let mode = std::ptr::read_unaligned(e.add(4) as *const u32);
        let inherit = std::ptr::read_unaligned(e.add(8) as *const u32) as u8;
        let form = std::ptr::read_unaligned(e.add(lay.form) as *const u32);
        let name = read_ptr_of(e.add(lay.name), ptr) as *const u8;
        let sid = match form {
            0 => sid_at(name).map(<[u8]>::to_vec),
            1 => {
                let text = if wide {
                    crate::dlls::wstr(name as *const u16)
                } else {
                    crate::dlls::cstr(name)
                };
                trustee_sid(&text)
            }
            _ => None,
        };
        let Some(sid) = sid else {
            crate::trace!(
                "SetEntriesInAcl : administré {i} (forme {form}) introuvable → ERROR_NONE_MAPPED"
            );
            return 1332; // ERROR_NONE_MAPPED
        };
        match mode {
            1 => aces.push((0, inherit, mask, sid)),
            2 => {
                aces.retain(|a| a.3 != sid);
                aces.push((0, inherit, mask, sid));
            }
            3 => aces.insert(0, (1, inherit, mask, sid)),
            4 => aces.retain(|a| a.3 != sid),
            _ => {}
        }
    }
    let size = 8 + aces.iter().map(|a| 8 + a.3.len()).sum::<usize>();
    let mut acl = vec![2u8, 0];
    acl.extend_from_slice(&(size as u16).to_le_bytes());
    acl.extend_from_slice(&(aces.len() as u16).to_le_bytes());
    acl.extend_from_slice(&[0, 0]);
    for (kind, flags, mask, sid) in aces {
        acl.extend_from_slice(&[kind, flags]);
        acl.extend_from_slice(&((8 + sid.len()) as u16).to_le_bytes());
        acl.extend_from_slice(&mask.to_le_bytes());
        acl.extend_from_slice(&sid);
    }
    let p = heap_copy(&acl);
    if p.is_null() {
        return 8;
    }
    *out = p;
    0
}

/// SID d'un nom d'administré : « CURRENT_USER », un compte Linux ou un
/// SID écrit « S-1-… ».
fn trustee_sid(name: &str) -> Option<Vec<u8>> {
    if name.starts_with("S-1-") {
        return sid_bytes(name);
    }
    if name.eq_ignore_ascii_case("CURRENT_USER") {
        return sid_bytes(&uid_sid(euid()));
    }
    let account = name.rsplit('\\').next().unwrap_or(name);
    if account.eq_ignore_ascii_case("Everyone") || account.eq_ignore_ascii_case("Tout le monde") {
        return sid_bytes(EVERYONE);
    }
    // Comptes et groupes bien connus (noms anglais de l'API, #1915).
    let known = match account.to_ascii_lowercase().as_str() {
        "system" | "local system" => Some("S-1-5-18"),
        "administrators" | "administrateurs" => Some("S-1-5-32-544"),
        "users" | "utilisateurs" => Some("S-1-5-32-545"),
        "authenticated users" => Some("S-1-5-11"),
        _ => None,
    };
    if let Some(sid) = known {
        return sid_bytes(sid);
    }
    let c = std::ffi::CString::new(account).ok()?;
    let pw = unsafe { libc::getpwnam(c.as_ptr()) };
    if pw.is_null() {
        return None;
    }
    sid_bytes(&uid_sid(unsafe { (*pw).pw_uid }))
}

pub unsafe extern "win64" fn SetEntriesInAclA(
    count: u32,
    entries: *const u8,
    old: *const u8,
    out: *mut *mut u8,
) -> u32 {
    set_entries_in_acl(count, entries, old, out, false)
}

pub unsafe extern "win64" fn SetEntriesInAclW(
    count: u32,
    entries: *const u8,
    old: *const u8,
    out: *mut *mut u8,
) -> u32 {
    set_entries_in_acl(count, entries, old, out, true)
}

/// GetSecurityInfo : descripteur d'un objet ouvert. Un fichier est décrit
/// d'après ses droits Linux ; un objet du noyau (processus, canal, jeton…)
/// appartient au compte qui fait tourner le programme.
pub unsafe extern "win64" fn GetSecurityInfo(
    handle: usize,
    kind: u32,
    info: u32,
    owner: *mut *mut u8,
    group: *mut *mut u8,
    dacl: *mut *mut u8,
    sacl: *mut *mut u8,
    sd: *mut *mut u8,
) -> u32 {
    let fd = crate::dlls::kernel32::handle_to_fd(handle);
    let path = if kind == 1 && fd >= 0 {
        std::fs::read_link(format!("/proc/self/fd/{fd}")).unwrap_or_else(|_| "/proc/self".into())
    } else {
        std::path::PathBuf::from("/proc/self")
    };
    let d = match file_descriptor(&path, info & 7) {
        Ok(d) => d,
        Err(e) => return crate::dlls::kernel32::io_error_code(&e),
    };
    let p = heap_copy(&d);
    if p.is_null() {
        return 8;
    }
    for (out, field) in [(owner, 4), (group, 8), (dacl, 16)] {
        if !out.is_null() {
            *out = sd_part(p, field);
        }
    }
    if !sacl.is_null() {
        *sacl = std::ptr::null_mut();
    }
    if !sd.is_null() {
        *sd = p;
    }
    0
}

/// SetSecurityInfo / SetNamedSecurityInfo : les droits Windows ne sont pas
/// reportés sur les objets Linux (un canal ou un processus n'est de toute
/// façon accessible qu'au compte qui l'a créé). Accepté sans effet.
pub unsafe extern "win64" fn SetSecurityInfo(
    _handle: usize,
    _kind: u32,
    _info: u32,
    _owner: *mut u8,
    _group: *mut u8,
    _dacl: *mut u8,
    _sacl: *mut u8,
) -> u32 {
    0
}

// ---------- Jetons du bac à sable de Chromium ----------
//
// Winlux n'a qu'un jeton par compte : les copies et les jetons restreints
// que le bac à sable fabrique avant de lancer un moteur de rendu sont ce
// même jeton (TOKEN_HANDLE), que CloseHandle accepte toujours. La
// séparation réelle des processus vient des comptes Linux, pas du jeton.

unsafe fn give_token(out: *mut usize) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *out = TOKEN_HANDLE;
    crate::process::set_last_error(0);
    1
}

pub unsafe extern "win64" fn DuplicateTokenEx(
    _token: usize,
    _access: u32,
    _sa: usize,
    _level: u32,
    _kind: u32,
    out: *mut usize,
) -> i32 {
    give_token(out)
}

pub unsafe extern "win64" fn DuplicateToken(_token: usize, _level: u32, out: *mut usize) -> i32 {
    give_token(out)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn CreateRestrictedToken(
    _token: usize,
    _flags: u32,
    _n_disable: u32,
    _disable: usize,
    _n_delete: u32,
    _delete: usize,
    _n_restrict: u32,
    _restrict: usize,
    out: *mut usize,
) -> i32 {
    give_token(out)
}

/// SetTokenInformation (niveau d'intégrité, DACL par défaut…) : accepté.
pub unsafe extern "win64" fn SetTokenInformation(
    _token: usize,
    _class: u32,
    _info: usize,
    _len: u32,
) -> i32 {
    1
}

/// Threads qui empruntent une identité (SetThreadToken, ImpersonateSelf…).
/// Clé : la poignée de thread reçue, ou `THIS_THREAD | tid` pour le thread
/// courant. Le jeton lui-même reste l'unique jeton Winlux ; seul compte le
/// fait que le thread en ait un, ce que vérifie le courtier du bac à sable
/// de Chromium juste après avoir créé un enfant (#767).
static IMPERSONATING: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
const THIS_THREAD: usize = 1 << 62;
const CURRENT_THREAD_PSEUDO: usize = usize::MAX - 1;

fn thread_key(thread: usize) -> usize {
    if thread == 0 || thread == CURRENT_THREAD_PSEUDO {
        THIS_THREAD | unsafe { libc::syscall(libc::SYS_gettid) } as usize
    } else {
        thread
    }
}

fn set_impersonation(thread: usize, on: bool) {
    let key = thread_key(thread);
    let mut list = IMPERSONATING.lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|&k| k != key);
    if on {
        list.push(key);
    }
}

/// Le thread désigné (poignée, ou 0 / pseudo-poignée pour le thread
/// courant) a-t-il reçu un jeton d'emprunt ?
pub fn thread_has_token(thread: usize) -> bool {
    let key = thread_key(thread);
    IMPERSONATING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&key)
}

/// OpenThreadToken côté advapi32/kernel32 : un jeton seulement si le thread
/// en a reçu un, sinon ERROR_NO_TOKEN comme sous Windows.
pub unsafe fn open_thread_token(thread: usize, token: *mut usize) -> i32 {
    if thread_has_token(thread) {
        return give_token(token);
    }
    if !token.is_null() {
        *token = 0;
    }
    crate::process::set_last_error(1008);
    0
}

pub unsafe extern "win64" fn ImpersonateSelf(_level: u32) -> i32 {
    set_impersonation(0, true);
    1
}

pub unsafe extern "win64" fn ImpersonateLoggedOnUser(_token: usize) -> i32 {
    set_impersonation(0, true);
    1
}

pub unsafe extern "win64" fn RevertToSelf() -> i32 {
    set_impersonation(0, false);
    1
}

/// SetThreadToken(PHANDLE thread, HANDLE token) : `thread` NULL désigne le
/// thread courant ; un jeton NULL met fin à l'emprunt.
pub unsafe extern "win64" fn SetThreadToken(thread: *const usize, token: usize) -> i32 {
    let target = if thread.is_null() { 0 } else { *thread };
    set_impersonation(target, token != 0);
    crate::process::set_last_error(0);
    1
}

pub unsafe extern "win64" fn IsTokenRestricted(_token: usize) -> i32 {
    crate::process::set_last_error(0);
    0
}

/// SID bien connus de `CreateWellKnownSid` (WELL_KNOWN_SID_TYPE).
fn well_known_sid(kind: u32) -> Option<&'static str> {
    Some(match kind {
        0 => "S-1-0-0",
        1 => "S-1-1-0",
        2 => "S-1-2-0",
        3 => "S-1-3-0",
        4 => "S-1-3-1",
        7 => "S-1-5",
        9 => "S-1-5-2",
        10 => "S-1-5-3",
        11 => "S-1-5-4",
        12 => "S-1-5-6",
        13 => "S-1-5-7",
        16 => "S-1-5-10",
        17 => "S-1-5-11",
        18 => "S-1-5-12",
        19 => "S-1-5-13",
        22 => "S-1-5-18",
        23 => "S-1-5-19",
        24 => "S-1-5-20",
        25 => "S-1-5-32",
        26 => "S-1-5-32-544",
        27 => "S-1-5-32-545",
        28 => "S-1-5-32-546",
        29 => "S-1-5-32-547",
        36 => "S-1-5-32-555",
        // WinBuiltinPerfMonitoringUsersSid, WinBuiltinPerfLoggingUsersSid
        // (WELL_KNOWN_SID_TYPE de winnt.h, en-têtes mingw-w64) (#1639).
        57 => "S-1-5-32-558",
        58 => "S-1-5-32-559",
        65 => "S-1-16-0",
        66 => "S-1-16-4096",
        67 => "S-1-16-8192",
        68 => "S-1-16-12288",
        69 => "S-1-16-16384",
        _ => return None,
    })
}

pub unsafe extern "win64" fn CreateWellKnownSid(
    kind: u32,
    _domain: *const u8,
    sid: *mut u8,
    size: *mut u32,
) -> i32 {
    let Some(bytes) = well_known_sid(kind).and_then(sid_bytes) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if size.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let room = *size as usize;
    *size = bytes.len() as u32;
    if sid.is_null() || room < bytes.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), sid, bytes.len());
    1
}

const SACL_SECURITY_INFORMATION: u32 = 0x8;
const LABEL_SECURITY_INFORMATION: u32 = 0x10;

/// Ajoute au descripteur auto-relatif `sd` une SACL portant l'étiquette
/// d'intégrité du processus (SYSTEM_MANDATORY_LABEL_ACE, NO_WRITE_UP), comme
/// TokenIntegrityLevel. Un jeton ou un processus Windows en a toujours une ;
/// Acrobat lit AceCount de cette SACL sans tester le pointeur (#1828).
fn with_integrity_label(mut sd: Vec<u8>) -> Vec<u8> {
    let level = if elevated() {
        "S-1-16-12288"
    } else {
        "S-1-16-8192"
    };
    let sid = sid_bytes(level).unwrap_or_default();
    let ace_size = 8 + sid.len();
    let off = sd.len() as u32;
    sd[12..16].copy_from_slice(&off.to_le_bytes());
    sd.extend_from_slice(&[2, 0]);
    sd.extend_from_slice(&((8 + ace_size) as u16).to_le_bytes());
    sd.extend_from_slice(&1u16.to_le_bytes());
    sd.extend_from_slice(&[0, 0]);
    // SYSTEM_MANDATORY_LABEL_ACE_TYPE 0x11, SYSTEM_MANDATORY_LABEL_NO_WRITE_UP.
    sd.extend_from_slice(&[0x11, 0]);
    sd.extend_from_slice(&(ace_size as u16).to_le_bytes());
    sd.extend_from_slice(&1u32.to_le_bytes());
    sd.extend_from_slice(&sid);
    let control = u16::from_le_bytes([sd[2], sd[3]]) | 0x10; // SE_SACL_PRESENT
    sd[2..4].copy_from_slice(&control.to_le_bytes());
    sd
}

/// GetKernelObjectSecurity : descripteur de l'objet (au nom du compte).
pub unsafe extern "win64" fn GetKernelObjectSecurity(
    _handle: usize,
    info: u32,
    buf: *mut u8,
    len: u32,
    needed: *mut u32,
) -> i32 {
    let d = match file_descriptor(std::path::Path::new("/proc/self"), info & 7) {
        Ok(d) if info & (SACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION) != 0 => {
            with_integrity_label(d)
        }
        Ok(d) => d,
        Err(e) => {
            crate::process::set_last_error(crate::dlls::kernel32::io_error_code(&e));
            return 0;
        }
    };
    if !needed.is_null() {
        *needed = d.len() as u32;
    }
    if buf.is_null() || (len as usize) < d.len() {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(d.as_ptr(), buf, d.len());
    1
}

pub unsafe extern "win64" fn SetKernelObjectSecurity(_h: usize, _info: u32, _sd: usize) -> i32 {
    1
}

/// SetNamedSecurityInfoW/A : comme SetSecurityInfo, sans effet sur Linux.
pub unsafe extern "win64" fn SetNamedSecurityInfo(
    _name: usize,
    _kind: u32,
    _info: u32,
    _owner: usize,
    _group: usize,
    _dacl: usize,
    _sacl: usize,
) -> u32 {
    0
}

/// CreateProcessAsUserW : même compte Linux, donc CreateProcessW.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn CreateProcessAsUserW(
    _token: usize,
    application: *const u16,
    command_line: *mut u16,
    process_attributes: usize,
    thread_attributes: usize,
    inherit: i32,
    flags: u32,
    environment: *const c_void,
    current_directory: *const u16,
    startup: *const crate::dlls::kernel32::StartupInfoA,
    info: *mut crate::dlls::kernel32::ProcessInformation,
) -> i32 {
    crate::dlls::kernel32::create_process_w(
        application,
        command_line,
        process_attributes,
        thread_attributes,
        inherit,
        flags,
        environment,
        current_directory,
        startup,
        info,
    )
}

// ---------- Lot D du relevé (#1512) : ACE et ACL ----------

/// Fin des ACE d'une ACL (décalage) et leur nombre.
unsafe fn acl_end(acl: *const u8) -> (usize, usize) {
    let count = std::ptr::read_unaligned(acl.add(4) as *const u16) as usize;
    let mut end = 8usize;
    for _ in 0..count {
        end += std::ptr::read_unaligned(acl.add(end + 2) as *const u16) as usize;
    }
    (end, count)
}

/// Insère des ACE brutes (`raw`, `n` entrées) à la position `index`
/// (u32::MAX ou au-delà : à la fin).
unsafe fn insert_aces(acl: *mut u8, index: u32, raw: &[u8], n: usize) -> i32 {
    if acl.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let size = std::ptr::read_unaligned(acl.add(2) as *const u16) as usize;
    let (end, count) = acl_end(acl);
    if end + raw.len() > size {
        crate::process::set_last_error(1344); // ERROR_ALLOTTED_SPACE_EXCEEDED
        return 0;
    }
    let mut at = 8usize;
    for _ in 0..(index as usize).min(count) {
        at += std::ptr::read_unaligned(acl.add(at + 2) as *const u16) as usize;
    }
    std::ptr::copy(acl.add(at), acl.add(at + raw.len()), end - at);
    std::ptr::copy_nonoverlapping(raw.as_ptr(), acl.add(at), raw.len());
    std::ptr::write_unaligned(acl.add(4) as *mut u16, (count + n) as u16);
    1
}

pub unsafe extern "win64" fn AddAccessDeniedAceEx(
    acl: *mut u8,
    _rev: u32,
    flags: u32,
    mask: u32,
    sid: *const u8,
) -> i32 {
    add_ace(acl, 1, flags as u8, mask, sid)
}

/// SYSTEM_AUDIT_ACE : SUCCESSFUL_ACCESS_ACE_FLAG (0x40) et
/// FAILED_ACCESS_ACE_FLAG (0x80) selon les deux booléens.
pub unsafe extern "win64" fn AddAuditAccessAce(
    acl: *mut u8,
    _rev: u32,
    mask: u32,
    sid: *const u8,
    success: i32,
    failure: i32,
) -> i32 {
    let flags = if success != 0 { 0x40 } else { 0 } | if failure != 0 { 0x80 } else { 0 };
    add_ace(acl, 2, flags, mask, sid)
}

pub unsafe extern "win64" fn AddAuditAccessAceEx(
    acl: *mut u8,
    _rev: u32,
    flags: u32,
    mask: u32,
    sid: *const u8,
    success: i32,
    failure: i32,
) -> i32 {
    let audit = if success != 0 { 0x40 } else { 0 } | if failure != 0 { 0x80 } else { 0 };
    add_ace(acl, 2, (flags as u8 & 0x1F) | audit, mask, sid)
}

/// ACE « objet » (types 5, 6, 7) : masque, drapeaux de présence, GUID du
/// type d'objet et du type hérité s'ils sont donnés, puis le SID.
unsafe fn add_object_ace(
    acl: *mut u8,
    kind: u8,
    flags: u8,
    mask: u32,
    object: *const u8,
    inherited: *const u8,
    sid: *const u8,
) -> i32 {
    let Some(sid) = sid_at(sid) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let mut present = 0u32;
    let mut guids = Vec::new();
    if !object.is_null() {
        present |= 1;
        guids.extend_from_slice(std::slice::from_raw_parts(object, 16));
    }
    if !inherited.is_null() {
        present |= 2;
        guids.extend_from_slice(std::slice::from_raw_parts(inherited, 16));
    }
    let len = 12 + guids.len() + sid.len();
    let mut raw = vec![kind, flags];
    raw.extend_from_slice(&(len as u16).to_le_bytes());
    raw.extend_from_slice(&mask.to_le_bytes());
    raw.extend_from_slice(&present.to_le_bytes());
    raw.extend_from_slice(&guids);
    raw.extend_from_slice(sid);
    insert_aces(acl, u32::MAX, &raw, 1)
}

pub unsafe extern "win64" fn AddAccessAllowedObjectAce(
    acl: *mut u8,
    _rev: u32,
    flags: u32,
    mask: u32,
    object: *const u8,
    inherited: *const u8,
    sid: *const u8,
) -> i32 {
    add_object_ace(acl, 5, flags as u8, mask, object, inherited, sid)
}

pub unsafe extern "win64" fn AddAccessDeniedObjectAce(
    acl: *mut u8,
    _rev: u32,
    flags: u32,
    mask: u32,
    object: *const u8,
    inherited: *const u8,
    sid: *const u8,
) -> i32 {
    add_object_ace(acl, 6, flags as u8, mask, object, inherited, sid)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn AddAuditAccessObjectAce(
    acl: *mut u8,
    _rev: u32,
    flags: u32,
    mask: u32,
    object: *const u8,
    inherited: *const u8,
    sid: *const u8,
    success: i32,
    failure: i32,
) -> i32 {
    let audit = if success != 0 { 0x40 } else { 0 } | if failure != 0 { 0x80 } else { 0 };
    add_object_ace(
        acl,
        7,
        (flags as u8 & 0x1F) | audit,
        mask,
        object,
        inherited,
        sid,
    )
}

/// AddAce : ACE déjà formées, insérées à `start` (MAXDWORD : à la fin).
pub unsafe extern "win64" fn AddAce(
    acl: *mut u8,
    _rev: u32,
    start: u32,
    list: *const u8,
    len: u32,
) -> i32 {
    if list.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let raw = std::slice::from_raw_parts(list, len as usize);
    let mut n = 0;
    let mut at = 0usize;
    while at + 4 <= raw.len() {
        let size = u16::from_le_bytes([raw[at + 2], raw[at + 3]]) as usize;
        if size < 4 || at + size > raw.len() {
            crate::process::set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
        at += size;
        n += 1;
    }
    insert_aces(acl, start, &raw[..at], n)
}

pub unsafe extern "win64" fn DeleteAce(acl: *mut u8, index: u32) -> i32 {
    if acl.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (end, count) = acl_end(acl);
    if index as usize >= count {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut at = 8usize;
    for _ in 0..index {
        at += std::ptr::read_unaligned(acl.add(at + 2) as *const u16) as usize;
    }
    let size = std::ptr::read_unaligned(acl.add(at + 2) as *const u16) as usize;
    std::ptr::copy(acl.add(at + size), acl.add(at), end - at - size);
    std::ptr::write_bytes(acl.add(end - size), 0, size);
    std::ptr::write_unaligned(acl.add(4) as *mut u16, (count - 1) as u16);
    1
}

/// InitializeSid : révision 1, autorité et sous-autorités à zéro.
pub unsafe extern "win64" fn InitializeSid(sid: *mut u8, auth: *const u8, count: u8) -> i32 {
    if sid.is_null() || auth.is_null() || count > 15 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *sid = 1;
    *sid.add(1) = count;
    std::ptr::copy_nonoverlapping(auth, sid.add(2), 6);
    std::ptr::write_bytes(sid.add(8), 0, 4 * count as usize);
    1
}

/// ACE d'une ACL : (type, drapeaux, masque, SID).
pub unsafe fn acl_entries(acl: *const u8) -> Vec<(u8, u8, u32, Vec<u8>)> {
    let mut out = Vec::new();
    if acl.is_null() {
        return out;
    }
    let (_, count) = acl_end(acl);
    let mut at = 8usize;
    for _ in 0..count {
        let p = acl.add(at);
        let size = std::ptr::read_unaligned(p.add(2) as *const u16) as usize;
        let kind = *p;
        let mask = std::ptr::read_unaligned(p.add(4) as *const u32);
        let sid_at_off = match kind {
            5..=7 => {
                let present = std::ptr::read_unaligned(p.add(8) as *const u32);
                12 + if present & 1 != 0 { 16 } else { 0 } + if present & 2 != 0 { 16 } else { 0 }
            }
            _ => 8,
        };
        if let Some(sid) = sid_at(p.add(sid_at_off)) {
            out.push((kind, *p.add(1), mask, sid.to_vec()));
        }
        at += size.max(8);
    }
    out
}

/// SID d'un TRUSTEE_W (par SID ou par nom).
unsafe fn trustee_of(t: *const u8) -> Option<Vec<u8>> {
    if t.is_null() {
        return None;
    }
    let form = std::ptr::read_unaligned(t.add(12) as *const u32);
    let name = std::ptr::read_unaligned(t.add(24) as *const *const u8);
    match form {
        0 => sid_at(name).map(<[u8]>::to_vec),
        1 => trustee_sid(&crate::dlls::wstr(name as *const u16)),
        _ => None,
    }
}

/// SID qui valent pour l'administré : lui-même, « Tout le monde », et les
/// groupes du jeton si c'est l'utilisateur courant.
fn trustee_set(sid: &[u8]) -> Vec<Vec<u8>> {
    let mut set = vec![sid.to_vec()];
    set.extend(sid_bytes(EVERYONE));
    if sid_text(sid).as_deref() == Some(uid_sid(euid()).as_str()) {
        for (g, attrs) in token_groups() {
            if attrs & SE_GROUP_ENABLED != 0 {
                set.extend(sid_bytes(&g));
            }
        }
    }
    set
}

/// Droits effectifs : ce qu'accordent les ACE d'autorisation, moins ce
/// qu'un refus antérieur a retiré (l'ordre des ACE compte).
pub fn effective_rights(aces: &[(u8, u8, u32, Vec<u8>)], who: &[Vec<u8>]) -> u32 {
    let mut granted = 0u32;
    let mut denied = 0u32;
    for (kind, flags, mask, sid) in aces {
        if flags & 0x8 != 0 || !who.contains(sid) {
            continue; // INHERIT_ONLY_ACE
        }
        match kind {
            0 | 5 => granted |= mask & !denied,
            1 | 6 => denied |= mask & !granted,
            _ => {}
        }
    }
    granted
}

pub unsafe extern "win64" fn GetEffectiveRightsFromAclW(
    acl: *const u8,
    trustee: *const u8,
    out: *mut u32,
) -> u32 {
    let Some(sid) = trustee_of(trustee) else {
        return ERROR_NONE_MAPPED;
    };
    if out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    // Une DACL absente accorde tout.
    *out = if acl.is_null() {
        0x1F_FFFF
    } else {
        effective_rights(&acl_entries(acl), &trustee_set(&sid))
    };
    0
}

pub unsafe extern "win64" fn GetAuditedPermissionsFromAclW(
    acl: *const u8,
    trustee: *const u8,
    success: *mut u32,
    failure: *mut u32,
) -> u32 {
    let Some(sid) = trustee_of(trustee) else {
        return ERROR_NONE_MAPPED;
    };
    if success.is_null() || failure.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let who = trustee_set(&sid);
    let (mut ok, mut ko) = (0, 0);
    for (kind, flags, mask, s) in acl_entries(acl) {
        if (kind == 2 || kind == 7) && flags & 0x8 == 0 && who.contains(&s) {
            if flags & 0x40 != 0 {
                ok |= mask;
            }
            if flags & 0x80 != 0 {
                ko |= mask;
            }
        }
    }
    *success = ok;
    *failure = ko;
    0
}

/// GetExplicitEntriesFromAclW : un tableau EXPLICIT_ACCESS_W (48 octets
/// chacun, 32 pour un programme x86) suivi des SID, en un seul bloc LocalAlloc.
pub unsafe extern "win64" fn GetExplicitEntriesFromAclW(
    acl: *const u8,
    count: *mut u32,
    out: *mut *mut u8,
) -> u32 {
    if count.is_null() || out.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *count = 0;
    *out = std::ptr::null_mut();
    let ptr = crate::wow32::pointer_size();
    let lay = access_layout(ptr);
    let entries: Vec<_> = acl_entries(acl)
        .into_iter()
        .filter(|e| e.1 & 0x10 == 0 && matches!(e.0, 0..=2)) // pas INHERITED_ACE
        .collect();
    if entries.is_empty() {
        return 0;
    }
    let head = entries.len() * lay.size;
    let total = head + entries.iter().map(|e| e.3.len()).sum::<usize>();
    let mut block = vec![0u8; total];
    let base = heap_copy(&block);
    if base.is_null() {
        return 8; // ERROR_NOT_ENOUGH_MEMORY
    }
    let mut at = head;
    for (i, (kind, flags, mask, sid)) in entries.iter().enumerate() {
        let e = i * lay.size;
        let mode: u32 = match (kind, flags & 0xC0) {
            (0, _) => 1,    // GRANT_ACCESS
            (1, _) => 3,    // DENY_ACCESS
            (_, 0x80) => 6, // SET_AUDIT_FAILURE
            _ => 5,         // SET_AUDIT_SUCCESS
        };
        block[e..e + 4].copy_from_slice(&mask.to_le_bytes());
        block[e + 4..e + 8].copy_from_slice(&mode.to_le_bytes());
        block[e + 8..e + 12].copy_from_slice(&u32::from(flags & 0x0F).to_le_bytes());
        // TRUSTEE_W : forme TRUSTEE_IS_SID (0), type inconnu (0).
        let sid_at = (base as usize + at).to_le_bytes();
        block[e + lay.name..e + lay.name + ptr].copy_from_slice(&sid_at[..ptr]);
        block[at..at + sid.len()].copy_from_slice(sid);
        at += sid.len();
    }
    std::ptr::copy_nonoverlapping(block.as_ptr(), base, total);
    *count = entries.len() as u32;
    *out = base;
    0
}

/// SDDL d'un SID : alias connu ou forme « S-1-… ».
fn sddl_sid(sid: &[u8]) -> String {
    let text = sid_text(sid).unwrap_or_default();
    let alias = match text.as_str() {
        "S-1-1-0" => "WD",
        "S-1-3-0" => "CO",
        "S-1-3-1" => "CG",
        "S-1-5-11" => "AU",
        "S-1-5-18" => "SY",
        "S-1-5-32-544" => "BA",
        "S-1-5-32-545" => "BU",
        "S-1-5-4" => "IU",
        _ => return text,
    };
    alias.to_string()
}

fn sddl_acl(prefix: &str, control: u16, aces: &[(u8, u8, u32, Vec<u8>)], protected: u16) -> String {
    let mut s = String::from(prefix);
    if control & protected != 0 {
        s.push('P');
    }
    for (kind, flags, mask, sid) in aces {
        let k = match kind {
            0 => "A",
            1 => "D",
            2 => "AU",
            5 => "OA",
            6 => "OD",
            7 => "OU",
            0x11 => "ML",
            _ => continue,
        };
        let mut f = String::new();
        for (bit, name) in [
            (0x1, "OI"),
            (0x2, "CI"),
            (0x4, "NP"),
            (0x8, "IO"),
            (0x10, "ID"),
            (0x40, "SA"),
            (0x80, "FA"),
        ] {
            if flags & bit != 0 {
                f.push_str(name);
            }
        }
        let m = if *mask == 0x1F_01FF {
            "FA".to_string()
        } else {
            format!("0x{mask:x}")
        };
        s.push_str(&format!("({k};{f};{m};;;{})", sddl_sid(sid)));
    }
    s
}

/// SDDL d'un descripteur de sécurité (autonome ou absolu).
pub unsafe fn sddl_of(sd: *const u8, info: u32) -> String {
    let control = std::ptr::read_unaligned(sd.add(2) as *const u16);
    let mut s = String::new();
    if info & 1 != 0 {
        if let Some(o) = sid_at(sd_part(sd, 4)) {
            s.push_str(&format!("O:{}", sddl_sid(o)));
        }
    }
    if info & 2 != 0 {
        if let Some(g) = sid_at(sd_part(sd, 8)) {
            s.push_str(&format!("G:{}", sddl_sid(g)));
        }
    }
    if info & 4 != 0 && control & 0x4 != 0 {
        let dacl = sd_part(sd, 16);
        if dacl.is_null() {
            s.push_str("D:NO_ACCESS_CONTROL");
        } else {
            s.push_str(&sddl_acl("D:", control, &acl_entries(dacl), 0x1000));
        }
    }
    if info & 8 != 0 && control & 0x10 != 0 {
        let sacl = sd_part(sd, 12);
        if !sacl.is_null() {
            s.push_str(&sddl_acl("S:", control, &acl_entries(sacl), 0x2000));
        }
    }
    s
}

pub unsafe extern "win64" fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
    sd: *const u8,
    rev: u32,
    info: u32,
    out: *mut *mut u16,
    len: *mut u32,
) -> i32 {
    if sd.is_null() || out.is_null() || rev != 1 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let text = sddl_of(sd, info);
    let w: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let bytes: Vec<u8> = w.iter().flat_map(|c| c.to_le_bytes()).collect();
    let p = heap_copy(&bytes);
    if p.is_null() {
        crate::process::set_last_error(8);
        return 0;
    }
    *out = p as *mut u16;
    if !len.is_null() {
        *len = w.len() as u32;
    }
    1
}

/// AdjustTokenGroups : les groupes du jeton Winlux sont obligatoires ; on
/// peut les « activer » (ils le sont) ou revenir aux valeurs par défaut,
/// pas les désactiver.
pub unsafe extern "win64" fn AdjustTokenGroups(
    _token: usize,
    reset: i32,
    groups: *const u8,
    _len: u32,
    _prev: *mut u8,
    ret_len: *mut u32,
) -> i32 {
    if !ret_len.is_null() {
        *ret_len = 0;
    }
    if reset != 0 {
        return 1;
    }
    if groups.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let n = std::ptr::read_unaligned(groups as *const u32) as usize;
    for i in 0..n {
        // TOKEN_GROUPS : GroupCount (puis alignement), SID_AND_ATTRIBUTES de 16 octets.
        let attrs = std::ptr::read_unaligned(groups.add(8 + i * 16 + 8) as *const u32);
        if attrs & SE_GROUP_ENABLED == 0 {
            crate::process::set_last_error(1310); // ERROR_CANT_DISABLE_MANDATORY
            return 0;
        }
    }
    1
}

const PRIVILEGE_DISPLAY: [&str; 8] = [
    "Create a token object",
    "Shut down the system",
    "Back up files and directories",
    "Restore files and directories",
    "Debug programs",
    "Bypass traverse checking",
    "Adjust memory quotas for a process",
    "Manage auditing and security log",
];

pub unsafe extern "win64" fn LookupPrivilegeNameW(
    _system: *const u16,
    luid: *const u32,
    name: *mut u16,
    len: *mut u32,
) -> i32 {
    if luid.is_null() || len.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let low = std::ptr::read_unaligned(luid);
    let Some(p) = PRIVILEGES.iter().find(|p| privilege_luid(p) == Some(low)) else {
        crate::process::set_last_error(1313); // ERROR_NO_SUCH_PRIVILEGE
        return 0;
    };
    if put_name_w(p, name, len) {
        1
    } else {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        0
    }
}

pub unsafe extern "win64" fn LookupPrivilegeDisplayNameW(
    _system: *const u16,
    name: *const u16,
    out: *mut u16,
    len: *mut u32,
    lang: *mut u32,
) -> i32 {
    if len.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let n = crate::dlls::wstr(name);
    let Some(i) = PRIVILEGES.iter().position(|p| p.eq_ignore_ascii_case(&n)) else {
        crate::process::set_last_error(1313);
        return 0;
    };
    if !lang.is_null() {
        *lang = 0x0409;
    }
    if put_name_w(PRIVILEGE_DISPLAY[i], out, len) {
        1
    } else {
        crate::process::set_last_error(ERROR_INSUFFICIENT_BUFFER);
        0
    }
}

pub fn releve_exports() -> crate::dlls::Exports {
    crate::exports! {
        "AddAccessDeniedAceEx" => fn AddAccessDeniedAceEx,
        "AddAuditAccessAce" => fn AddAuditAccessAce,
        "AddAuditAccessAceEx" => fn AddAuditAccessAceEx,
        "AddAccessAllowedObjectAce" => fn AddAccessAllowedObjectAce,
        "AddAccessDeniedObjectAce" => fn AddAccessDeniedObjectAce,
        "AddAuditAccessObjectAce" => fn AddAuditAccessObjectAce,
        "AddAce" => fn AddAce,
        "DeleteAce" => fn DeleteAce,
        "InitializeSid" => fn InitializeSid,
        "GetEffectiveRightsFromAclW" => fn GetEffectiveRightsFromAclW,
        "GetAuditedPermissionsFromAclW" => fn GetAuditedPermissionsFromAclW,
        "GetExplicitEntriesFromAclW" => fn GetExplicitEntriesFromAclW,
        // TRUSTEE_IS_SID : ptstrName est un SID, même disposition en A (#2029).
        "GetExplicitEntriesFromAclA" => fn GetExplicitEntriesFromAclW,
        "ConvertSecurityDescriptorToStringSecurityDescriptorW" => fn ConvertSecurityDescriptorToStringSecurityDescriptorW,
        "AdjustTokenGroups" => fn AdjustTokenGroups,
        "LookupPrivilegeNameW" => fn LookupPrivilegeNameW,
        "LookupPrivilegeDisplayNameW" => fn LookupPrivilegeDisplayNameW,
    }
}

pub fn advapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "OpenProcessToken" => fn OpenProcessToken,
        "GetTokenInformation" => fn GetTokenInformation,
        "CheckTokenMembership" => fn CheckTokenMembership,
        "AdjustTokenPrivileges" => fn AdjustTokenPrivileges,
        "LookupPrivilegeValueW" => fn LookupPrivilegeValueW,
        "LookupPrivilegeValueA" => fn LookupPrivilegeValueA,
        "AllocateAndInitializeSid" => fn AllocateAndInitializeSid,
        "FreeSid" => fn FreeSid,
        "IsValidSid" => fn IsValidSid,
        "GetLengthSid" => fn GetLengthSid,
        "EqualSid" => fn EqualSid,
        "CopySid" => fn CopySid,
        "GetSidSubAuthority" => fn GetSidSubAuthority,
        "GetSidSubAuthorityCount" => fn GetSidSubAuthorityCount,
        "GetSidIdentifierAuthority" => fn GetSidIdentifierAuthority,
        "ConvertSidToStringSidW" => fn ConvertSidToStringSidW,
        "ConvertSidToStringSidA" => fn ConvertSidToStringSidA,
        "ConvertStringSidToSidW" => fn ConvertStringSidToSidW,
        "ConvertStringSidToSidA" => fn ConvertStringSidToSidA,
        "LookupAccountSidW" => fn LookupAccountSidW,
        "GetFileSecurityW" => fn GetFileSecurityW,
        "GetFileSecurityA" => fn GetFileSecurityA,
        "InitializeSecurityDescriptor" => fn InitializeSecurityDescriptor,
        "SetSecurityDescriptorDacl" => fn SetSecurityDescriptorDacl,
        "SetSecurityDescriptorOwner" => fn SetSecurityDescriptorOwner,
        "SetSecurityDescriptorGroup" => fn SetSecurityDescriptorGroup,
        "InitializeAcl" => fn InitializeAcl,
        "AddAccessAllowedAce" => fn AddAccessAllowedAce,
        "AddAccessAllowedAceEx" => fn AddAccessAllowedAceEx,
        "AddAccessDeniedAce" => fn AddAccessDeniedAce,
        "AddMandatoryAce" => fn AddMandatoryAce,
        "SetEntriesInAclA" => fn SetEntriesInAclA,
        "SetEntriesInAclW" => fn SetEntriesInAclW,
        "GetSecurityInfo" => fn GetSecurityInfo,
        "SetSecurityInfo" => fn SetSecurityInfo,
        "GetNamedSecurityInfoW" => fn GetNamedSecurityInfoW,
        "GetNamedSecurityInfoA" => fn GetNamedSecurityInfoA,
        "GetSecurityDescriptorOwner" => fn GetSecurityDescriptorOwner,
        "GetSecurityDescriptorGroup" => fn GetSecurityDescriptorGroup,
        "GetSecurityDescriptorDacl" => fn GetSecurityDescriptorDacl,
        "GetSecurityDescriptorLength" => fn GetSecurityDescriptorLength,
        "IsValidSecurityDescriptor" => fn IsValidSecurityDescriptor,
        "GetAce" => fn GetAce,
        "GetAclInformation" => fn GetAclInformation,
        "DuplicateTokenEx" => fn DuplicateTokenEx,
        "DuplicateToken" => fn DuplicateToken,
        "CreateRestrictedToken" => fn CreateRestrictedToken,
        "SetTokenInformation" => fn SetTokenInformation,
        "ImpersonateLoggedOnUser" => fn ImpersonateLoggedOnUser,
        "RevertToSelf" => fn RevertToSelf,
        "ImpersonateSelf" => fn ImpersonateSelf,
        "SetThreadToken" => fn SetThreadToken,
        "IsTokenRestricted" => fn IsTokenRestricted,
        "CreateWellKnownSid" => fn CreateWellKnownSid,
        "GetKernelObjectSecurity" => fn GetKernelObjectSecurity,
        "SetKernelObjectSecurity" => fn SetKernelObjectSecurity,
        "SetNamedSecurityInfoW" => fn SetNamedSecurityInfo,
        "SetNamedSecurityInfoA" => fn SetNamedSecurityInfo,
        "CreateProcessAsUserW" => fn CreateProcessAsUserW,
    }
}

pub fn shell32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "IsUserAnAdmin" => fn IsUserAnAdmin,
        // Ordinal documenté de IsUserAnAdmin ; Steam.exe l'importe ainsi (#1380).
        "#680" => fn IsUserAnAdmin,
    }
}

#[cfg(test)]
mod tests;
