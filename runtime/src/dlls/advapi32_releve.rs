//! advapi32 et secur32 : les fonctions du lot D du relevé d'imports
//! (#1512) qui ne relèvent ni des ACL (security.rs), ni des services
//! (advapi32_services.rs), ni des identifiants (credentials.rs).
//!
//! Importées surtout par pywin32 (win32security, win32evtlog, win32file,
//! win32net…), jdk4py, Qt 6 et sharp. Chacune suit la documentation
//! publique. Quand Linux n'a pas l'équivalent (chiffrement EFS, traces
//! ETW, stratégie LSA, journal d'événements Windows), la fonction répond
//! comme un Windows où la fonction existe mais où il n'y a rien : journal
//! vide, fichier non chiffré, aucune session de trace, droit refusé à un
//! compte sans privilège.

#![allow(clippy::missing_safety_doc)]

use crate::dlls::{cstr, lookup, wstr, Exports};
use crate::process::set_last_error;
use std::collections::HashSet;
use std::sync::Mutex;

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),* $(,)?) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_HANDLE_EOF: u32 = 38;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_SHUTDOWN_IN_PROGRESS: u32 = 1116;
const ERROR_PRIVILEGE_NOT_HELD: u32 = 1314;
const ERROR_NO_SUCH_DOMAIN: u32 = 1355;
const ERROR_RPC_SERVER_UNAVAILABLE: u32 = 1722;
const ERROR_WMI_INSTANCE_NOT_FOUND: u32 = 4201;
const ERROR_FILE_NOT_ENCRYPTED: u32 = 6007;

const STATUS_SUCCESS: u32 = 0;
const STATUS_INVALID_HANDLE: u32 = 0xC000_0008;
const STATUS_NO_SUCH_LOGON_SESSION: u32 = 0xC000_005F;
const STATUS_PRIVILEGE_NOT_HELD: u32 = 0xC000_0061;
const STATUS_NO_SUCH_PACKAGE: u32 = 0xC000_00FE;
const STATUS_INVALID_PARAMETER: u32 = 0xC000_000D;
const STATUS_NO_MEMORY: u32 = 0xC000_0017;

fn fail(e: u32) -> i32 {
    set_last_error(e);
    0
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_of_ansi(p: *const u8) -> Option<Vec<u16>> {
    (!p.is_null()).then(|| wide(&unsafe { cstr(p) }))
}

fn ptr_or_null(v: &Option<Vec<u16>>) -> *const u16 {
    v.as_ref().map_or(std::ptr::null(), |w| w.as_ptr())
}

// ── LSA (advapi32) ──────────────────────────────────────────────────────

/// LsaNtStatusToWinError : la correspondance des codes NTSTATUS courants
/// (ntstatus.h / winerror.h).
pub fn nt_status_to_win_error(status: u32) -> u32 {
    match status {
        0 => 0,
        0xC000_0001 => 31,   // STATUS_UNSUCCESSFUL → ERROR_GEN_FAILURE
        0xC000_0002 => 1,    // STATUS_NOT_IMPLEMENTED → ERROR_INVALID_FUNCTION
        0xC000_0005 => 998,  // ACCESS_VIOLATION → ERROR_NOACCESS
        0xC000_0008 => 6,    // INVALID_HANDLE
        0xC000_000D => 87,   // INVALID_PARAMETER
        0xC000_000F => 2,    // NO_SUCH_FILE
        0xC000_0017 => 8,    // NO_MEMORY → ERROR_NOT_ENOUGH_MEMORY
        0xC000_0022 => 5,    // ACCESS_DENIED
        0xC000_0023 => 122,  // BUFFER_TOO_SMALL → ERROR_INSUFFICIENT_BUFFER
        0xC000_0034 => 2,    // OBJECT_NAME_NOT_FOUND → ERROR_FILE_NOT_FOUND
        0xC000_0035 => 183,  // OBJECT_NAME_COLLISION → ERROR_ALREADY_EXISTS
        0xC000_003A => 3,    // OBJECT_PATH_NOT_FOUND
        0xC000_005F => 1312, // NO_SUCH_LOGON_SESSION
        0xC000_0060 => 1313, // NO_SUCH_PRIVILEGE
        0xC000_0061 => 1314, // PRIVILEGE_NOT_HELD
        0xC000_0064 => 1317, // NO_SUCH_USER
        0xC000_006A => 86,   // WRONG_PASSWORD → ERROR_INVALID_PASSWORD
        0xC000_006D => 1326, // LOGON_FAILURE
        0xC000_0073 => 1332, // NONE_MAPPED
        0xC000_00BB => 50,   // NOT_SUPPORTED
        0xC000_00FE => 1364, // NO_SUCH_PACKAGE
        0x8000_0005 => 234,  // BUFFER_OVERFLOW → ERROR_MORE_DATA
        0x8000_001A => 259,  // NO_MORE_ENTRIES → ERROR_NO_MORE_ITEMS
        0x0000_0103 => 997,  // PENDING → ERROR_IO_PENDING
        _ => 317,            // ERROR_MR_MID_NOT_FOUND, comme Windows
    }
}

extern "win64" fn lsa_nt_status_to_win_error(status: u32) -> u32 {
    nt_status_to_win_error(status)
}

/// Les tampons LSA de Winlux viennent de malloc.
unsafe extern "win64" fn lsa_free_memory(p: *mut u8) -> u32 {
    if !p.is_null() {
        libc::free(p.cast());
    }
    STATUS_SUCCESS
}

/// LsaOpenPolicy refuse l'accès à la stratégie locale (aucun compte n'est
/// administrateur de la base LSA) : aucun handle de stratégie n'existe.
extern "win64" fn lsa_no_policy(_a: usize, _b: usize, _c: usize, _d: usize) -> u32 {
    STATUS_INVALID_HANDLE
}

// ── LSA (secur32) ───────────────────────────────────────────────────────

fn lsa_handles() -> &'static Mutex<HashSet<usize>> {
    static H: std::sync::OnceLock<Mutex<HashSet<usize>>> = std::sync::OnceLock::new();
    H.get_or_init(Default::default)
}

static NEXT_LSA: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x15A0_0000);

/// LsaConnectUntrusted : connexion non privilégiée, toujours permise.
unsafe extern "win64" fn lsa_connect_untrusted(out: *mut usize) -> u32 {
    if out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let h = NEXT_LSA.fetch_add(4, std::sync::atomic::Ordering::Relaxed);
    lock(lsa_handles()).insert(h);
    *out = h;
    STATUS_SUCCESS
}

extern "win64" fn lsa_deregister_logon_process(h: usize) -> u32 {
    if lock(lsa_handles()).remove(&h) {
        STATUS_SUCCESS
    } else {
        STATUS_INVALID_HANDLE
    }
}

/// LsaRegisterLogonProcess demande SeTcbPrivilege.
extern "win64" fn lsa_register_logon_process(_name: usize, _out: usize, _mode: usize) -> u32 {
    STATUS_PRIVILEGE_NOT_HELD
}

/// Paquets d'authentification connus (numéros stables).
const PACKAGES: [&str; 3] = [
    "Negotiate",
    "Kerberos",
    "MICROSOFT_AUTHENTICATION_PACKAGE_V1_0",
];

/// LSA_STRING (x64) : Length, MaximumLength, puis Buffer aligné sur 8.
unsafe fn lsa_string(p: *const u8) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let len = std::ptr::read_unaligned(p as *const u16) as usize;
    let buf = std::ptr::read_unaligned(p.add(8) as *const *const u8);
    if buf.is_null() {
        return Some(String::new());
    }
    Some(String::from_utf8_lossy(std::slice::from_raw_parts(buf, len)).into_owned())
}

unsafe extern "win64" fn lsa_lookup_authentication_package(
    h: usize,
    name: *const u8,
    out: *mut u32,
) -> u32 {
    if !lock(lsa_handles()).contains(&h) {
        return STATUS_INVALID_HANDLE;
    }
    if out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(n) = lsa_string(name) else {
        return STATUS_INVALID_PARAMETER;
    };
    match PACKAGES.iter().position(|p| p.eq_ignore_ascii_case(&n)) {
        Some(i) => {
            *out = i as u32;
            STATUS_SUCCESS
        }
        None => STATUS_NO_SUCH_PACKAGE,
    }
}

/// LsaCallAuthenticationPackage : la session n'a ni ticket Kerberos ni
/// cache NTLM ; chaque requête au paquet le dit
/// (STATUS_NO_SUCH_LOGON_SESSION dans ProtocolStatus).
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn lsa_call_authentication_package(
    h: usize,
    package: u32,
    _buf: usize,
    _len: u32,
    out: *mut usize,
    out_len: *mut u32,
    protocol: *mut u32,
) -> u32 {
    if !out.is_null() {
        *out = 0;
    }
    if !out_len.is_null() {
        *out_len = 0;
    }
    if !lock(lsa_handles()).contains(&h) {
        return STATUS_INVALID_HANDLE;
    }
    if package as usize >= PACKAGES.len() {
        return STATUS_NO_SUCH_PACKAGE;
    }
    if !protocol.is_null() {
        *protocol = STATUS_NO_SUCH_LOGON_SESSION;
    }
    STATUS_SUCCESS
}

unsafe extern "win64" fn lsa_free_return_buffer(p: *mut u8) -> u32 {
    lsa_free_memory(p)
}

/// LsaEnumerateLogonSessions : la session de l'utilisateur.
unsafe extern "win64" fn lsa_enumerate_logon_sessions(count: *mut u32, out: *mut *mut u8) -> u32 {
    if count.is_null() || out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let p = libc::calloc(1, 8) as *mut u8;
    if p.is_null() {
        return STATUS_NO_MEMORY;
    }
    std::ptr::write_unaligned(p as *mut u32, crate::dlls::security::logon_luid());
    *count = 1;
    *out = p;
    STATUS_SUCCESS
}

/// SECURITY_LOGON_SESSION_DATA dans sa première forme (Size = 136) :
/// utilisateur, domaine (la machine), paquet, type interactif, SID.
pub fn logon_session_data(luid: u32, user: &str, host: &str, sid: &[u8], at: u64) -> Vec<u8> {
    let strings = [user, host, "Negotiate", "", "", ""];
    let mut block = vec![0u8; 136];
    block[..4].copy_from_slice(&136u32.to_le_bytes());
    block[4..8].copy_from_slice(&luid.to_le_bytes());
    block[64..68].copy_from_slice(&2u32.to_le_bytes()); // Interactive
    block[68..72].copy_from_slice(&1u32.to_le_bytes()); // Session
    block[80..88].copy_from_slice(&at.to_le_bytes());
    // Les chaînes et le SID suivent la structure ; les pointeurs sont
    // posés en relatif ici, puis corrigés à l'adresse du bloc.
    let offsets = [16usize, 32, 48, 88, 104, 120];
    for (s, off) in strings.iter().zip(offsets) {
        let w: Vec<u8> = s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        let pos = block.len();
        block[off..off + 2].copy_from_slice(&(w.len() as u16).to_le_bytes());
        block[off + 2..off + 4].copy_from_slice(&((w.len() + 2) as u16).to_le_bytes());
        block[off + 8..off + 16].copy_from_slice(&(pos as u64).to_le_bytes());
        block.extend_from_slice(&w);
        block.extend_from_slice(&[0, 0]);
    }
    while !block.len().is_multiple_of(4) {
        block.push(0);
    }
    let pos = block.len();
    block[72..80].copy_from_slice(&(pos as u64).to_le_bytes());
    block.extend_from_slice(sid);
    block
}

unsafe extern "win64" fn lsa_get_logon_session_data(luid: *const u32, out: *mut *mut u8) -> u32 {
    if luid.is_null() || out.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    *out = std::ptr::null_mut();
    let want = std::ptr::read_unaligned(luid);
    let mine = crate::dlls::security::logon_luid();
    if want != mine || std::ptr::read_unaligned(luid.add(1)) != 0 {
        return STATUS_NO_SUCH_LOGON_SESSION;
    }
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|h| h.trim().to_uppercase())
        .unwrap_or_else(|_| "WINLUX".into());
    let sid = crate::dlls::security::sid_bytes(&crate::dlls::security::uid_sid(libc::geteuid()))
        .unwrap_or_default();
    // Heure d'ouverture : début de la session (démarrage du chef de session).
    let at = std::fs::metadata(format!("/proc/{}", libc::getsid(0)))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64 / 100 + 116_444_736_000_000_000)
        .unwrap_or(0);
    let mut block = logon_session_data(mine, &crate::paths::user_name(), &host, &sid, at);
    let p = libc::calloc(1, block.len()) as *mut u8;
    if p.is_null() {
        return STATUS_NO_MEMORY;
    }
    for off in [24usize, 40, 56, 72, 96, 112, 128] {
        let rel = u64::from_le_bytes(block[off..off + 8].try_into().unwrap_or_default());
        block[off..off + 8].copy_from_slice(&(p as u64 + rel).to_le_bytes());
    }
    std::ptr::copy_nonoverlapping(block.as_ptr(), p, block.len());
    *out = p;
    STATUS_SUCCESS
}

/// La stratégie locale ne change jamais sous Winlux : l'inscription est
/// acceptée et l'événement ne sera jamais signalé.
extern "win64" fn lsa_policy_change_notification(_class: u32, _event: usize) -> u32 {
    STATUS_SUCCESS
}

/// SetContextAttributesA : aucun attribut de contexte ne se règle après
/// coup sur les contextes Schannel de Winlux.
extern "win64" fn set_context_attributes_a(_ctx: usize, _attr: u32, _buf: usize, _len: u32) -> i32 {
    0x8009_0302_u32 as i32 // SEC_E_UNSUPPORTED_FUNCTION
}

/// TranslateNameW : les formats de nom d'annuaire supposent un domaine.
extern "win64" fn translate_name_w(
    _name: usize,
    _from: u32,
    _to: u32,
    _out: usize,
    _len: usize,
) -> i32 {
    fail(ERROR_NO_SUCH_DOMAIN)
}

// ── Journal d'événements ────────────────────────────────────────────────

fn event_logs() -> &'static Mutex<HashSet<usize>> {
    static L: std::sync::OnceLock<Mutex<HashSet<usize>>> = std::sync::OnceLock::new();
    L.get_or_init(Default::default)
}

static NEXT_LOG: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x0E70_0000);

fn local_server(server: *const u16) -> bool {
    if server.is_null() {
        return true;
    }
    let s = unsafe { wstr(server) };
    let s = s.trim_start_matches('\\');
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    s.is_empty()
        || s == "."
        || s.eq_ignore_ascii_case(host.trim())
        || s.eq_ignore_ascii_case("localhost")
}

/// OpenEventLogW : un journal Windows vide. Les événements des programmes
/// (ReportEvent) vont au journal de Winlux, que lit le rapport de bogue.
unsafe extern "win64" fn open_event_log_w(server: *const u16, source: *const u16) -> usize {
    if !local_server(server) {
        set_last_error(ERROR_RPC_SERVER_UNAVAILABLE);
        return 0;
    }
    if source.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let h = NEXT_LOG.fetch_add(4, std::sync::atomic::Ordering::Relaxed);
    lock(event_logs()).insert(h);
    h
}

/// OpenBackupEventLogW : Winlux ne lit pas les fichiers .evt.
unsafe extern "win64" fn open_backup_event_log_w(server: *const u16, file: *const u16) -> usize {
    if !local_server(server) {
        set_last_error(ERROR_RPC_SERVER_UNAVAILABLE);
        return 0;
    }
    if file.is_null() || !crate::paths::to_linux(&wstr(file)).exists() {
        set_last_error(ERROR_FILE_NOT_FOUND);
        return 0;
    }
    set_last_error(ERROR_NOT_SUPPORTED);
    0
}

fn is_log(h: usize) -> bool {
    lock(event_logs()).contains(&h)
}

extern "win64" fn close_event_log(h: usize) -> i32 {
    if lock(event_logs()).remove(&h) {
        1
    } else {
        fail(ERROR_INVALID_HANDLE)
    }
}

unsafe extern "win64" fn get_number_of_event_log_records(h: usize, out: *mut u32) -> i32 {
    if !is_log(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    *out = 0;
    1
}

unsafe extern "win64" fn get_oldest_event_log_record(h: usize, out: *mut u32) -> i32 {
    get_number_of_event_log_records(h, out)
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn read_event_log_w(
    h: usize,
    _flags: u32,
    _offset: u32,
    _buf: usize,
    _len: u32,
    read: *mut u32,
    needed: *mut u32,
) -> i32 {
    if !is_log(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    for p in [read, needed] {
        if !p.is_null() {
            *p = 0;
        }
    }
    fail(ERROR_HANDLE_EOF)
}

/// Vider un journal vide réussit ; une copie de sauvegarde demandée
/// ailleurs n'est pas écrite (format .evt non pris en charge).
unsafe extern "win64" fn clear_event_log_w(h: usize, backup: *const u16) -> i32 {
    if !is_log(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if !backup.is_null() && *backup != 0 {
        return fail(ERROR_NOT_SUPPORTED);
    }
    1
}

unsafe extern "win64" fn backup_event_log_w(h: usize, _file: *const u16) -> i32 {
    if !is_log(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    fail(ERROR_NOT_SUPPORTED)
}

/// Le journal vide ne change jamais : l'événement ne sera pas signalé.
extern "win64" fn notify_change_event_log(h: usize, _event: usize) -> i32 {
    if is_log(h) {
        1
    } else {
        fail(ERROR_INVALID_HANDLE)
    }
}

// ── Chiffrement de fichiers (EFS) ───────────────────────────────────────

fn file_exists(p: *const u16) -> bool {
    !p.is_null() && crate::paths::to_linux(&unsafe { wstr(p) }).exists()
}

/// FileEncryptionStatusW : FILE_SYSTEM_NOT_SUPPORT (6), les systèmes de
/// fichiers Linux n'ont pas EFS.
unsafe extern "win64" fn file_encryption_status_w(file: *const u16, status: *mut u32) -> i32 {
    if status.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if !file_exists(file) {
        return fail(ERROR_FILE_NOT_FOUND);
    }
    *status = 6;
    1
}

unsafe extern "win64" fn encrypt_file_w(file: *const u16) -> i32 {
    if !file_exists(file) {
        return fail(ERROR_FILE_NOT_FOUND);
    }
    fail(ERROR_NOT_SUPPORTED)
}

/// DecryptFileW sur un fichier en clair réussit sans rien faire.
unsafe extern "win64" fn decrypt_file_w(file: *const u16, _reserved: u32) -> i32 {
    if !file_exists(file) {
        return fail(ERROR_FILE_NOT_FOUND);
    }
    1
}

/// EncryptionDisable : le chiffrement est déjà indisponible partout.
unsafe extern "win64" fn encryption_disable(dir: *const u16, _disable: i32) -> i32 {
    if !file_exists(dir) {
        return fail(ERROR_FILE_NOT_FOUND);
    }
    1
}

/// Fonctions qui rendent un code d'erreur : aucun fichier n'est chiffré.
unsafe extern "win64" fn not_encrypted(file: *const u16, _b: usize, _c: usize) -> u32 {
    if !file_exists(file) {
        return ERROR_FILE_NOT_FOUND;
    }
    ERROR_FILE_NOT_ENCRYPTED
}

unsafe extern "win64" fn duplicate_encryption_info_file(
    src: *const u16,
    _dst: *const u16,
    _a: u32,
    _b: u32,
    _c: usize,
) -> u32 {
    not_encrypted(src, 0, 0)
}

extern "win64" fn free_encryption_certificate_hash_list(_list: usize) {}

unsafe extern "win64" fn open_encrypted_file_raw_w(
    file: *const u16,
    _flags: u32,
    ctx: *mut usize,
) -> u32 {
    if !ctx.is_null() {
        *ctx = 0;
    }
    if !file_exists(file) {
        return ERROR_FILE_NOT_FOUND;
    }
    ERROR_NOT_SUPPORTED
}

/// Aucun contexte brut n'est jamais ouvert.
extern "win64" fn encrypted_raw_io(_cb: usize, _ctx: usize, _c: usize) -> u32 {
    ERROR_INVALID_PARAMETER
}

extern "win64" fn close_encrypted_file_raw(_ctx: usize) {}

// ── Traces ETW ──────────────────────────────────────────────────────────

/// StartTraceW : un compte ordinaire n'est pas dans « Performance Log
/// Users » ; Winlux n'a pas de collecteur ETW.
extern "win64" fn start_trace_w(_h: usize, _name: usize, _props: usize) -> u32 {
    ERROR_ACCESS_DENIED
}

/// ControlTraceW : aucune session de trace n'existe.
extern "win64" fn control_trace_w(_h: u64, _name: usize, _props: usize, _code: u32) -> u32 {
    ERROR_WMI_INSTANCE_NOT_FOUND
}

/// OpenTraceW : INVALID_PROCESSTRACE_HANDLE, aucune session ni fichier .etl
/// lisible.
extern "win64" fn open_trace_w(_log: usize) -> u64 {
    set_last_error(ERROR_WMI_INSTANCE_NOT_FOUND);
    u64::MAX
}

extern "win64" fn process_trace(_handles: usize, _count: u32, _start: usize, _end: usize) -> u32 {
    ERROR_INVALID_HANDLE
}

extern "win64" fn close_trace(_h: u64) -> u32 {
    ERROR_INVALID_HANDLE
}

/// StartTraceA : comme StartTraceW.
extern "win64" fn start_trace_a(h: usize, name: usize, props: usize) -> u32 {
    start_trace_w(h, name, props)
}

/// StopTraceW, FlushTraceW, QueryTraceW, UpdateTraceW (et leurs formes A,
/// ControlTraceA) : formes de ControlTrace, aucune session n'existe.
extern "win64" fn trace_session_op(_h: u64, _name: usize, _props: usize) -> u32 {
    ERROR_WMI_INSTANCE_NOT_FOUND
}

extern "win64" fn control_trace_a(h: u64, name: usize, props: usize, code: u32) -> u32 {
    control_trace_w(h, name, props, code)
}

extern "win64" fn open_trace_a(log: usize) -> u64 {
    open_trace_w(log)
}

/// EnableTrace, EnableTraceEx, EnableTraceEx2 : activer un fournisseur
/// demande d'être administrateur ou dans « Performance Log Users ».
extern "win64" fn enable_trace(_a: u64, _b: usize, _c: usize) -> u32 {
    ERROR_ACCESS_DENIED
}

/// QueryAllTracesW/A(PropertyArray, PropertyArrayCount, *LoggerCount) :
/// aucune session de trace n'est en cours.
unsafe extern "win64" fn query_all_traces(
    _array: usize,
    _count: u32,
    logger_count: *mut u32,
) -> u32 {
    if logger_count.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *logger_count = 0;
    0
}

/// EnumerateTraceGuidsEx(class, in, in_size, out, out_size, *needed) :
/// aucun fournisseur inscrit, liste vide.
unsafe extern "win64" fn enumerate_trace_guids_ex(
    _class: u32,
    _inb: usize,
    _in_size: u32,
    _outb: usize,
    _out_size: u32,
    needed: *mut u32,
) -> u32 {
    if needed.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *needed = 0;
    0
}

/// TraceSetInformation : sans session ni droit de profilage.
extern "win64" fn trace_set_information(_h: u64, _class: u32, _info: usize, _len: u32) -> u32 {
    ERROR_ACCESS_DENIED
}

/// TraceQueryInformation : aucune session dont lire les réglages.
extern "win64" fn trace_query_information(
    _h: u64,
    _class: u32,
    _info: usize,
    _len: u32,
    _ret: usize,
) -> u32 {
    ERROR_WMI_INSTANCE_NOT_FOUND
}

// ── Fournisseurs de chiffrement ─────────────────────────────────────────

/// Fournisseurs que CryptAcquireContext accepte : (nom, type).
const PROVIDERS: [(&str, u32); 2] = [
    ("Winlux Software Cryptographic Provider", 1), // PROV_RSA_FULL
    ("Winlux RSA and AES Cryptographic Provider", 24), // PROV_RSA_AES
];
const PROVIDER_TYPES: [(&str, u32); 2] = [
    ("RSA Full (Signature and Key Exchange)", 1),
    ("RSA Full and AES", 24),
];
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_MORE_DATA: u32 = 234;

/// Copie `text` (UTF-16, avec zéro) ; `len` est en octets.
unsafe fn put_bytes_w(text: &str, out: *mut u16, len: *mut u32) -> i32 {
    if len.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let w = wide(text);
    let need = (w.len() * 2) as u32;
    if out.is_null() {
        *len = need;
        return 1;
    }
    if *len < need {
        *len = need;
        return fail(ERROR_MORE_DATA);
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), out, w.len());
    *len = need;
    1
}

unsafe fn enum_pairs(
    list: &[(&str, u32)],
    index: u32,
    kind: *mut u32,
    name: *mut u16,
    len: *mut u32,
) -> i32 {
    let Some((n, t)) = list.get(index as usize) else {
        return fail(ERROR_NO_MORE_ITEMS);
    };
    if !kind.is_null() {
        *kind = *t;
    }
    put_bytes_w(n, name, len)
}

unsafe extern "win64" fn crypt_enum_providers_w(
    index: u32,
    _reserved: usize,
    _flags: u32,
    kind: *mut u32,
    name: *mut u16,
    len: *mut u32,
) -> i32 {
    enum_pairs(&PROVIDERS, index, kind, name, len)
}

unsafe extern "win64" fn crypt_enum_provider_types_w(
    index: u32,
    _reserved: usize,
    _flags: u32,
    kind: *mut u32,
    name: *mut u16,
    len: *mut u32,
) -> i32 {
    enum_pairs(&PROVIDER_TYPES, index, kind, name, len)
}

unsafe extern "win64" fn crypt_get_default_provider_w(
    kind: u32,
    _reserved: usize,
    _flags: u32,
    name: *mut u16,
    len: *mut u32,
) -> i32 {
    match PROVIDERS.iter().find(|p| p.1 == kind) {
        Some((n, _)) => put_bytes_w(n, name, len),
        None => fail(0x8009_0017), // NTE_PROV_TYPE_NOT_DEF
    }
}

/// CryptSetProviderExW : un seul fournisseur par type, déjà celui par
/// défaut ; CRYPT_DELETE_DEFAULT (4) n'a rien à retirer.
unsafe extern "win64" fn crypt_set_provider_ex_w(
    name: *const u16,
    kind: u32,
    _reserved: usize,
    flags: u32,
) -> i32 {
    if flags & 4 != 0 {
        return 1;
    }
    let n = wstr(name);
    if PROVIDERS
        .iter()
        .any(|p| p.1 == kind && p.0.eq_ignore_ascii_case(&n))
    {
        1
    } else {
        fail(0x8009_0013) // NTE_BAD_PROV_TYPE
    }
}

// ── Divers ──────────────────────────────────────────────────────────────

/// Arrêt de la machine : les programmes n'ont pas SeShutdownPrivilege
/// (le menu Démarrer de la session s'en charge).
extern "win64" fn initiate_system_shutdown_w(
    _m: usize,
    _msg: usize,
    _t: u32,
    _force: i32,
    _reboot: i32,
) -> i32 {
    fail(ERROR_PRIVILEGE_NOT_HELD)
}

/// InitiateSystemShutdownExW : comme la variante sans raison, le droit
/// d'arrêter la machine n'est pas accordé aux programmes ; l'arrêt passe
/// par le menu de Winlux.
extern "win64" fn initiate_system_shutdown_ex_w(
    m: usize,
    msg: usize,
    t: u32,
    force: i32,
    reboot: i32,
    _reason: u32,
) -> i32 {
    initiate_system_shutdown_w(m, msg, t, force, reboot)
}

extern "win64" fn abort_system_shutdown_w(_machine: usize) -> i32 {
    fail(ERROR_NO_SHUTDOWN_IN_PROGRESS)
}

/// HW_PROFILE_INFOW : poste non amarré, un GUID stable tiré de
/// /etc/machine-id, « Profile 1 ».
pub fn hw_profile_guid(machine_id: &str) -> String {
    let hex: String = machine_id
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(32)
        .collect();
    let hex = format!("{hex:0<32}");
    format!(
        "{{{}-{}-{}-{}-{}}}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

unsafe extern "win64" fn get_current_hw_profile_w(out: *mut u8) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    std::ptr::write_bytes(out, 0, 244);
    std::ptr::write_unaligned(out as *mut u32, 1); // DOCKINFO_UNDOCKED
    let id = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
    let g = wide(&hw_profile_guid(&id));
    std::ptr::copy_nonoverlapping(g.as_ptr() as *const u8, out.add(4), g.len().min(39) * 2);
    let n = wide("Profile 1");
    std::ptr::copy_nonoverlapping(n.as_ptr() as *const u8, out.add(82), n.len() * 2);
    1
}

/// ImpersonateAnonymousToken : l'emprunt d'identité de Winlux ne change
/// pas l'utilisateur Linux ; il est noté comme ImpersonateSelf
/// (SecurityAnonymous).
unsafe extern "win64" fn impersonate_anonymous_token(_thread: usize) -> i32 {
    api!("advapi32", "ImpersonateSelf", fn(u32) -> i32, 0).unwrap_or(0)
}

/// LogonUserW : Winlux ne vérifie pas de mot de passe de compte pour un
/// programme.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn logon_user_w(
    _user: usize,
    _domain: usize,
    _password: usize,
    _kind: u32,
    _provider: u32,
    token: *mut usize,
) -> i32 {
    if !token.is_null() {
        *token = 0;
    }
    fail(ERROR_NOT_SUPPORTED)
}

/// Copie une chaîne W (sans zéro) vers un tampon A de `*len` caractères,
/// convention de LookupAccount* (taille nécessaire avec le zéro).
unsafe fn narrow_into(w: &[u16], out: *mut u8, len: *mut u32) -> bool {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    let a = crate::dlls::encode_ansi(&String::from_utf16_lossy(&w[..end]));
    if out.is_null() || (*len as usize) <= a.len() {
        *len = a.len() as u32 + 1;
        return false;
    }
    std::ptr::copy_nonoverlapping(a.as_ptr(), out, a.len());
    *out.add(a.len()) = 0;
    *len = a.len() as u32;
    true
}

type LookupW = unsafe extern "win64" fn(
    *const u16,
    usize,
    *mut u8,
    *mut u32,
    *mut u16,
    *mut u32,
    *mut u32,
) -> i32;

/// Appelle la version W avec des tampons larges, puis convertit.
#[allow(clippy::too_many_arguments)]
unsafe fn lookup_a(
    name_w: &'static str,
    system: *const u8,
    key: usize,
    name_out: *mut u8,
    name_len: *mut u32,
    domain: *mut u8,
    domain_len: *mut u32,
    use_: *mut u32,
    sid_mode: bool,
) -> i32 {
    let Some(addr) = lookup("advapi32", name_w) else {
        return fail(ERROR_NOT_SUPPORTED);
    };
    let f: LookupW = std::mem::transmute(addr);
    if domain_len.is_null() || name_len.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let sys = wide_of_ansi(system);
    let mut dom = vec![0u16; 256];
    let mut dl = 256u32;
    if sid_mode {
        // LookupAccountSidA : `key` est le SID, `name_out` reçoit le nom.
        let mut nm = vec![0u16; 256];
        let mut nl = 256u32;
        if f(
            ptr_or_null(&sys),
            key,
            nm.as_mut_ptr() as *mut u8,
            &mut nl,
            dom.as_mut_ptr(),
            &mut dl,
            use_,
        ) == 0
        {
            return 0;
        }
        let a = narrow_into(&nm, name_out, name_len);
        let b = narrow_into(&dom, domain, domain_len);
        return if a && b {
            1
        } else {
            fail(ERROR_INSUFFICIENT_BUFFER)
        };
    }
    // LookupAccountNameA : `key` est le nom, `name_out` reçoit le SID.
    let nm = wide_of_ansi(key as *const u8);
    let ok = f(
        ptr_or_null(&sys),
        ptr_or_null(&nm) as usize,
        name_out,
        name_len,
        dom.as_mut_ptr(),
        &mut dl,
        use_,
    );
    let b = narrow_into(&dom, domain, domain_len);
    if ok == 0 {
        return 0;
    }
    if b {
        1
    } else {
        fail(ERROR_INSUFFICIENT_BUFFER)
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn lookup_account_name_a(
    system: *const u8,
    name: *const u8,
    sid: *mut u8,
    sid_len: *mut u32,
    domain: *mut u8,
    domain_len: *mut u32,
    use_: *mut u32,
) -> i32 {
    lookup_a(
        "LookupAccountNameW",
        system,
        name as usize,
        sid,
        sid_len,
        domain,
        domain_len,
        use_,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn lookup_account_sid_a(
    system: *const u8,
    sid: *const u8,
    name: *mut u8,
    name_len: *mut u32,
    domain: *mut u8,
    domain_len: *mut u32,
    use_: *mut u32,
) -> i32 {
    lookup_a(
        "LookupAccountSidW",
        system,
        sid as usize,
        name,
        name_len,
        domain,
        domain_len,
        use_,
        true,
    )
}

unsafe extern "win64" fn set_file_security_a(file: *const u8, info: u32, sd: usize) -> i32 {
    let w = wide_of_ansi(file);
    api!(
        "advapi32",
        "SetFileSecurityW",
        fn(*const u16, u32, usize) -> i32,
        ptr_or_null(&w),
        info,
        sd
    )
    .unwrap_or_else(|| fail(ERROR_NOT_SUPPORTED))
}

unsafe extern "win64" fn convert_string_sd_to_sd_a(
    text: *const u8,
    rev: u32,
    out: usize,
    len: *mut u32,
) -> i32 {
    let w = wide_of_ansi(text);
    api!(
        "advapi32",
        "ConvertStringSecurityDescriptorToSecurityDescriptorW",
        fn(*const u16, u32, usize, *mut u32) -> i32,
        ptr_or_null(&w),
        rev,
        out,
        len
    )
    .unwrap_or_else(|| fail(ERROR_NOT_SUPPORTED))
}

/// Les variantes transactionnelles du registre : sans transaction KTM,
/// l'opération se fait tout de suite, comme la version ordinaire.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn reg_create_key_transacted_w(
    key: usize,
    sub: *const u16,
    reserved: u32,
    class: *const u16,
    options: u32,
    sam: u32,
    sa: usize,
    out: *mut usize,
    disposition: *mut u32,
    _tx: usize,
    _param: usize,
) -> i32 {
    api!(
        "advapi32",
        "RegCreateKeyExW",
        fn(usize, *const u16, u32, *const u16, u32, u32, usize, *mut usize, *mut u32) -> i32,
        key,
        sub,
        reserved,
        class,
        options,
        sam,
        sa,
        out,
        disposition
    )
    .unwrap_or(ERROR_NOT_SUPPORTED as i32)
}

unsafe extern "win64" fn reg_open_key_transacted_w(
    key: usize,
    sub: *const u16,
    options: u32,
    sam: u32,
    out: *mut usize,
    _tx: usize,
    _param: usize,
) -> i32 {
    api!(
        "advapi32",
        "RegOpenKeyExW",
        fn(usize, *const u16, u32, u32, *mut usize) -> i32,
        key,
        sub,
        options,
        sam,
        out
    )
    .unwrap_or(ERROR_NOT_SUPPORTED as i32)
}

unsafe extern "win64" fn reg_delete_key_transacted_w(
    key: usize,
    sub: *const u16,
    sam: u32,
    reserved: u32,
    _tx: usize,
    _param: usize,
) -> i32 {
    api!(
        "advapi32",
        "RegDeleteKeyExW",
        fn(usize, *const u16, u32, u32) -> i32,
        key,
        sub,
        sam,
        reserved
    )
    .unwrap_or(ERROR_NOT_SUPPORTED as i32)
}

/// RegGetKeySecurity : le descripteur que Winlux donne aux objets de
/// l'utilisateur (propriétaire, contrôle total), code LSTATUS.
unsafe extern "win64" fn reg_get_key_security(
    key: usize,
    info: u32,
    sd: *mut u8,
    len: *mut u32,
) -> i32 {
    if len.is_null() {
        return ERROR_INVALID_PARAMETER as i32;
    }
    if key == 0 {
        return ERROR_INVALID_HANDLE as i32;
    }
    let mut needed = 0u32;
    let ok = crate::dlls::security::GetKernelObjectSecurity(key, info, sd, *len, &mut needed);
    if ok != 0 {
        *len = needed;
        return 0;
    }
    *len = needed;
    ERROR_INSUFFICIENT_BUFFER as i32
}

/// RegOverridePredefKey : les clés prédéfinies du registre Winlux ne se
/// redirigent pas.
extern "win64" fn reg_override_predef_key(_key: usize, _new: usize) -> i32 {
    ERROR_NOT_SUPPORTED as i32
}

/// Extensions exécutables de la liste publique des stratégies SAFER.
const EXECUTABLE_TYPES: [&str; 31] = [
    "ade", "adp", "app", "bas", "bat", "chm", "cmd", "com", "cpl", "crt", "exe", "hlp", "hta",
    "inf", "ins", "isp", "lnk", "mdb", "mde", "msc", "msi", "msp", "mst", "ocx", "pcd", "pif",
    "reg", "scr", "shs", "url", "vb",
];

pub fn is_executable_type(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) => {
            EXECUTABLE_TYPES.iter().any(|e| e.eq_ignore_ascii_case(ext))
                || ext.eq_ignore_ascii_case("wsc")
        }
        None => false,
    }
}

unsafe extern "win64" fn saferi_is_executable_file_type(path: *const u16, _shell: u8) -> i32 {
    if path.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    i32::from(is_executable_type(&wstr(path)))
}

pub fn exports() -> Exports {
    crate::exports! {
        "LsaNtStatusToWinError" => fn lsa_nt_status_to_win_error,
        "LsaFreeMemory" => fn lsa_free_memory,
        "LsaEnumerateAccountRights" => fn lsa_no_policy,
        "LsaEnumerateAccountsWithUserRight" => fn lsa_no_policy,
        "LsaQueryInformationPolicy" => fn lsa_no_policy,
        "LsaSetInformationPolicy" => fn lsa_no_policy,
        "LsaRemoveAccountRights" => fn lsa_no_policy,
        "LsaRetrievePrivateData" => fn lsa_no_policy,
        "LsaStorePrivateData" => fn lsa_no_policy,
        "OpenEventLogW" => fn open_event_log_w,
        "OpenBackupEventLogW" => fn open_backup_event_log_w,
        "CloseEventLog" => fn close_event_log,
        "GetNumberOfEventLogRecords" => fn get_number_of_event_log_records,
        "GetOldestEventLogRecord" => fn get_oldest_event_log_record,
        "ReadEventLogW" => fn read_event_log_w,
        "ClearEventLogW" => fn clear_event_log_w,
        "BackupEventLogW" => fn backup_event_log_w,
        "NotifyChangeEventLog" => fn notify_change_event_log,
        "FileEncryptionStatusW" => fn file_encryption_status_w,
        "EncryptFileW" => fn encrypt_file_w,
        "DecryptFileW" => fn decrypt_file_w,
        "EncryptionDisable" => fn encryption_disable,
        "QueryUsersOnEncryptedFile" => fn not_encrypted,
        "QueryRecoveryAgentsOnEncryptedFile" => fn not_encrypted,
        "AddUsersToEncryptedFile" => fn not_encrypted,
        "RemoveUsersFromEncryptedFile" => fn not_encrypted,
        "DuplicateEncryptionInfoFile" => fn duplicate_encryption_info_file,
        "FreeEncryptionCertificateHashList" => fn free_encryption_certificate_hash_list,
        "OpenEncryptedFileRawW" => fn open_encrypted_file_raw_w,
        "ReadEncryptedFileRaw" => fn encrypted_raw_io,
        "WriteEncryptedFileRaw" => fn encrypted_raw_io,
        "CloseEncryptedFileRaw" => fn close_encrypted_file_raw,
        "StartTraceW" => fn start_trace_w,
        "ControlTraceW" => fn control_trace_w,
        "OpenTraceW" => fn open_trace_w,
        "ProcessTrace" => fn process_trace,
        "CloseTrace" => fn close_trace,
        // Contrat api-ms-win-eventing-controller (Teams, #1790).
        "StartTraceA" => fn start_trace_a,
        "ControlTraceA" => fn control_trace_a,
        "OpenTraceA" => fn open_trace_a,
        "StopTraceW" => fn trace_session_op,
        "StopTraceA" => fn trace_session_op,
        "FlushTraceW" => fn trace_session_op,
        "FlushTraceA" => fn trace_session_op,
        "QueryTraceW" => fn trace_session_op,
        "QueryTraceA" => fn trace_session_op,
        "UpdateTraceW" => fn trace_session_op,
        "UpdateTraceA" => fn trace_session_op,
        "EnableTrace" => fn enable_trace,
        "EnableTraceEx" => fn enable_trace,
        "EnableTraceEx2" => fn enable_trace,
        "QueryAllTracesW" => fn query_all_traces,
        "QueryAllTracesA" => fn query_all_traces,
        "EnumerateTraceGuidsEx" => fn enumerate_trace_guids_ex,
        "TraceSetInformation" => fn trace_set_information,
        "TraceQueryInformation" => fn trace_query_information,
        "CryptEnumProvidersW" => fn crypt_enum_providers_w,
        "CryptEnumProviderTypesW" => fn crypt_enum_provider_types_w,
        "CryptGetDefaultProviderW" => fn crypt_get_default_provider_w,
        "CryptSetProviderExW" => fn crypt_set_provider_ex_w,
        "InitiateSystemShutdownW" => fn initiate_system_shutdown_w,
        "InitiateSystemShutdownExW" => fn initiate_system_shutdown_ex_w,
        "AbortSystemShutdownW" => fn abort_system_shutdown_w,
        "GetCurrentHwProfileW" => fn get_current_hw_profile_w,
        "ImpersonateAnonymousToken" => fn impersonate_anonymous_token,
        "LogonUserW" => fn logon_user_w,
        "LookupAccountNameA" => fn lookup_account_name_a,
        "LookupAccountSidA" => fn lookup_account_sid_a,
        "SetFileSecurityA" => fn set_file_security_a,
        "ConvertStringSecurityDescriptorToSecurityDescriptorA" => fn convert_string_sd_to_sd_a,
        "RegCreateKeyTransactedW" => fn reg_create_key_transacted_w,
        "RegOpenKeyTransactedW" => fn reg_open_key_transacted_w,
        "RegDeleteKeyTransactedW" => fn reg_delete_key_transacted_w,
        "RegGetKeySecurity" => fn reg_get_key_security,
        "RegOverridePredefKey" => fn reg_override_predef_key,
        "SaferiIsExecutableFileType" => fn saferi_is_executable_file_type,
    }
}

pub fn secur32_exports() -> Exports {
    crate::exports! {
        "LsaConnectUntrusted" => fn lsa_connect_untrusted,
        "LsaDeregisterLogonProcess" => fn lsa_deregister_logon_process,
        "LsaRegisterLogonProcess" => fn lsa_register_logon_process,
        "LsaLookupAuthenticationPackage" => fn lsa_lookup_authentication_package,
        "LsaCallAuthenticationPackage" => fn lsa_call_authentication_package,
        "LsaFreeReturnBuffer" => fn lsa_free_return_buffer,
        "LsaEnumerateLogonSessions" => fn lsa_enumerate_logon_sessions,
        "LsaGetLogonSessionData" => fn lsa_get_logon_session_data,
        "LsaRegisterPolicyChangeNotification" => fn lsa_policy_change_notification,
        "LsaUnregisterPolicyChangeNotification" => fn lsa_policy_change_notification,
        "SetContextAttributesA" => fn set_context_attributes_a,
        "TranslateNameW" => fn translate_name_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_nt_convertis() {
        assert_eq!(nt_status_to_win_error(0), 0);
        assert_eq!(nt_status_to_win_error(0xC000_0022), 5);
        assert_eq!(nt_status_to_win_error(0xC000_0034), 2);
        assert_eq!(nt_status_to_win_error(0x1234_5678), 317);
    }

    #[test]
    fn types_executables() {
        assert!(is_executable_type("C:\\a\\setup.EXE"));
        assert!(is_executable_type("x.msi"));
        assert!(!is_executable_type("notes.txt"));
        assert!(!is_executable_type("sans_extension"));
    }

    #[test]
    fn guid_de_profil_materiel() {
        assert_eq!(
            hw_profile_guid("0123456789abcdef0123456789abcdef\n"),
            "{01234567-89ab-cdef-0123-456789abcdef}"
        );
    }

    #[test]
    fn donnees_de_session() {
        let sid = [1u8, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0];
        let b = logon_session_data(0x10042, "dam", "PC", &sid, 7);
        assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 136);
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 0x10042);
        // UserName : 3 caractères, chaîne placée après la structure.
        assert_eq!(u16::from_le_bytes([b[16], b[17]]), 6);
        let at = u64::from_le_bytes(b[24..32].try_into().unwrap()) as usize;
        assert_eq!(&b[at..at + 6], &[b'd', 0, b'a', 0, b'm', 0]);
        let s = u64::from_le_bytes(b[72..80].try_into().unwrap()) as usize;
        assert_eq!(&b[s..], &sid);
    }

    #[test]
    fn ace_ajoutees_retirees_et_droits_effectifs() {
        use crate::dlls::security as sec;
        unsafe {
            let mut acl = vec![0u8; 256];
            assert_eq!(sec::InitializeAcl(acl.as_mut_ptr(), 256, 2), 1);
            let user = sec::sid_bytes("S-1-5-21-1-2-3-1000").unwrap();
            let everyone = sec::sid_bytes("S-1-1-0").unwrap();
            assert_eq!(
                sec::AddAccessAllowedAce(acl.as_mut_ptr(), 2, 0x1F_01FF, user.as_ptr()),
                1
            );
            assert_eq!(
                sec::AddAccessDeniedAceEx(acl.as_mut_ptr(), 2, 0, 0x2, everyone.as_ptr()),
                1
            );
            // Le refus placé avant l'autorisation retire le droit d'écriture.
            let entries = sec::acl_entries(acl.as_ptr());
            let mut deny_first = entries.clone();
            deny_first.rotate_right(1);
            let who = vec![user.clone(), everyone.clone()];
            assert_eq!(sec::effective_rights(&entries, &who), 0x1F_01FF);
            assert_eq!(sec::effective_rights(&deny_first, &who), 0x1F_01FF & !0x2);
            // AddAce en tête, puis DeleteAce.
            let raw = &acl[8 + 8 + user.len()..8 + 8 + user.len() + 8 + everyone.len()].to_vec();
            assert_eq!(
                sec::AddAce(acl.as_mut_ptr(), 2, 0, raw.as_ptr(), raw.len() as u32),
                1
            );
            assert_eq!(sec::acl_entries(acl.as_ptr()).len(), 3);
            assert_eq!(sec::acl_entries(acl.as_ptr())[0].0, 1);
            assert_eq!(sec::DeleteAce(acl.as_mut_ptr(), 0), 1);
            assert_eq!(sec::acl_entries(acl.as_ptr()), entries);
        }
    }

    #[test]
    fn identifiant_marshale_relu() {
        use crate::dlls::credentials::{marshal, unmarshal};
        let s = marshal(2, b"damien");
        assert!(s.starts_with("@@C"));
        assert_eq!(unmarshal(&s), Some((2, b"damien".to_vec())));
        assert_eq!(unmarshal("pas marshale"), None);
    }
}
