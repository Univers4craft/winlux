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
const TOKEN_HANDLE: usize = 0x5EC0_0001;

const SE_GROUP_MANDATORY: u32 = 0x1;
const SE_GROUP_ENABLED_BY_DEFAULT: u32 = 0x2;
const SE_GROUP_ENABLED: u32 = 0x4;
const SE_GROUP_OWNER: u32 = 0x8;
const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x10;
const SE_GROUP_INTEGRITY: u32 = 0x20;
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
        2 => {
            // TokenGroups
            let groups = token_groups();
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
        3 => dword(0),  // TokenPrivileges : aucun privilège listé
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

/// Pointeur vers une partie d'un descripteur auto-relatif (0 = absente).
unsafe fn sd_part(sd: *const u8, field: usize) -> *mut u8 {
    let off = std::ptr::read_unaligned(sd.add(field) as *const u32) as usize;
    if off == 0 {
        std::ptr::null_mut()
    } else {
        sd.add(off) as *mut u8
    }
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
        "GetNamedSecurityInfoW" => fn GetNamedSecurityInfoW,
        "GetNamedSecurityInfoA" => fn GetNamedSecurityInfoA,
        "GetSecurityDescriptorOwner" => fn GetSecurityDescriptorOwner,
        "GetSecurityDescriptorGroup" => fn GetSecurityDescriptorGroup,
        "GetSecurityDescriptorDacl" => fn GetSecurityDescriptorDacl,
        "GetSecurityDescriptorLength" => fn GetSecurityDescriptorLength,
        "IsValidSecurityDescriptor" => fn IsValidSecurityDescriptor,
        "GetAce" => fn GetAce,
        "GetAclInformation" => fn GetAclInformation,
    }
}

pub fn shell32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "IsUserAnAdmin" => fn IsUserAnAdmin,
    }
}

#[cfg(test)]
mod tests;
