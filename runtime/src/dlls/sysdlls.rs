//! Petites DLL système importées en direct par des logiciels du corpus de
//! mesure (#26) : sans elles, le module qui les importe ne se charge pas du
//! tout. Chacune répond comme un Windows où le service correspondant n'a
//! rien à offrir, sans prétendre avoir agi.
//!
//! - authz : contrôle d'accès de Qt6Core (`qt_ntfs_permission_lookup`),
//!   mêmes droits que `AccessCheck` (#1392) ;
//! - avicap32 : Video for Windows de ffmpeg et panda3d ; les pilotes sont les
//!   caméras V4L2 présentes, la capture elle-même reste à brancher (#1391) ;
//! - credui : analyse des noms et invite en console (win32cred, #1393) ;
//! - evr : échantillon vidéo du greffon multimédia de Qt ;
//! - ktmw32, sfc, loadperf, rasapi32, ntdsapi, query : modules de pywin32
//!   (#1393), pour des services absents d'un poste Winlux (transactions
//!   NTFS, fichiers protégés, compteurs à installer, accès à distance,
//!   annuaire Active Directory, filtres d'indexation).
//!
//! Écrit d'après la documentation publique de chaque fonction.

use std::sync::atomic::{AtomicUsize, Ordering};

const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NO_MORE_FILES: u32 = 18;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_CANNOT_FIND_PHONEBOOK_ENTRY: u32 = 623;
const ERROR_NOT_FOUND: u32 = 1168;
const ERROR_CANCELLED: u32 = 1223;
const ERROR_NO_SUCH_LOGON_SESSION: u32 = 1312;
const ERROR_INVALID_ACCOUNT_NAME: u32 = 1315;
const ERROR_NO_SUCH_DOMAIN: u32 = 1355;
const E_FAIL: i32 = 0x8000_4005_u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001_u32 as i32;
const E_POINTER: i32 = 0x8000_4003_u32 as i32;
const INVALID_HANDLE_VALUE: usize = usize::MAX;

fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}

/// Poignées opaques des contextes authz : multiples de 4, jamais nulles.
fn new_handle() -> usize {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    0x00AD_0000 + NEXT.fetch_add(1, Ordering::Relaxed) * 4
}

// --------------------------------------------------------------------------
// authz
// --------------------------------------------------------------------------

/// Correspondance générique des fichiers (FILE_GENERIC_READ, WRITE,
/// EXECUTE, FILE_ALL_ACCESS) : c'est l'usage de Qt.
const FILE_MAPPING: [u32; 4] = [0x0012_0089, 0x0012_0116, 0x0012_00A0, 0x001F_01FF];

unsafe fn put<T>(out: *mut T, v: T) -> bool {
    if out.is_null() {
        return false;
    }
    out.write_unaligned(v);
    true
}

pub unsafe extern "win64" fn authz_initialize_resource_manager(
    _flags: u32,
    _check: usize,
    _compute_groups: usize,
    _free_groups: usize,
    _name: *const u16,
    rm: *mut usize,
) -> i32 {
    if put(rm, new_handle()) {
        1
    } else {
        fail(ERROR_INVALID_PARAMETER)
    }
}

pub unsafe extern "win64" fn authz_initialize_context_from_sid(
    _flags: u32,
    sid: usize,
    rm: usize,
    _expiration: *const i64,
    _id: u64,
    _dynamic: usize,
    ctx: *mut usize,
) -> i32 {
    if sid == 0 || rm == 0 || !put(ctx, new_handle()) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    1
}

pub unsafe extern "win64" fn authz_initialize_context_from_token(
    _flags: u32,
    _token: usize,
    rm: usize,
    _expiration: *const i64,
    _id: u64,
    _dynamic: usize,
    ctx: *mut usize,
) -> i32 {
    if rm == 0 || !put(ctx, new_handle()) {
        return fail(ERROR_INVALID_PARAMETER);
    }
    1
}

/// AUTHZ_ACCESS_REQUEST : DesiredAccess en tête. AUTHZ_ACCESS_REPLY :
/// ResultListLength (u32), puis GrantedAccessMask*, SaclEvaluationResults*
/// et Error*, alignés sur 8 octets.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn authz_access_check(
    _flags: u32,
    ctx: usize,
    request: *const u32,
    _audit: usize,
    sd: usize,
    _extra_sd: usize,
    _extra_count: u32,
    reply: *mut u8,
    results: *mut usize,
) -> i32 {
    if ctx == 0 || request.is_null() || sd == 0 || reply.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let count = (reply as *const u32).read_unaligned() as usize;
    let granted = (reply.add(8) as *const *mut u32).read_unaligned();
    let errors = (reply.add(24) as *const *mut u32).read_unaligned();
    let mask = super::electron_win32::access_check_granted(request.read_unaligned(), FILE_MAPPING);
    for i in 0..count {
        put(granted.wrapping_add(i), mask);
        put(errors.wrapping_add(i), ERROR_SUCCESS);
    }
    put(results, 0);
    1
}

pub extern "win64" fn authz_free(_h: usize) -> i32 {
    1
}

// --------------------------------------------------------------------------
// avicap32
// --------------------------------------------------------------------------

/// Caméras V4L2, dans l'ordre de leur numéro : (nom, version du noyau).
fn v4l2_devices() -> Vec<String> {
    let mut v: Vec<(u32, String)> = std::fs::read_dir("/sys/class/video4linux")
        .map(|d| {
            d.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    let idx = n.strip_prefix("video")?.parse().ok()?;
                    let name = std::fs::read_to_string(e.path().join("name")).ok()?;
                    Some((idx, name.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.into_iter().map(|(_, n)| n).collect()
}

fn driver_description(index: u16) -> Option<(String, String)> {
    // Video for Windows ne connaît que dix pilotes (0 à 9).
    if index > 9 {
        return None;
    }
    let name = v4l2_devices().into_iter().nth(index as usize)?;
    Some((name, "V4L2".into()))
}

pub unsafe extern "win64" fn cap_get_driver_description_a(
    index: u16,
    name: *mut u8,
    cb_name: i32,
    ver: *mut u8,
    cb_ver: i32,
) -> i32 {
    let Some((n, v)) = driver_description(index) else {
        return 0;
    };
    super::put_text_a(&n, name, cb_name.max(0) as u32);
    super::put_text_a(&v, ver, cb_ver.max(0) as u32);
    1
}

pub unsafe extern "win64" fn cap_get_driver_description_w(
    index: u16,
    name: *mut u16,
    cb_name: i32,
    ver: *mut u16,
    cb_ver: i32,
) -> i32 {
    let Some((n, v)) = driver_description(index) else {
        return 0;
    };
    super::put_text_w(&n, name, (cb_name.max(0) / 2) as u32);
    super::put_text_w(&v, ver, (cb_ver.max(0) / 2) as u32);
    1
}

/// Fenêtre de capture : une fenêtre ordinaire. Les messages WM_CAP_* n'y
/// sont pas encore traités (WM_CAP_DRIVER_CONNECT rend FALSE), le programme
/// voit donc une caméra qu'il ne peut pas ouvrir (#1391).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn cap_create_capture_window_w(
    title: *const u16,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: usize,
    id: i32,
) -> usize {
    let class: Vec<u16> = "Static".encode_utf16().chain([0]).collect();
    super::user32::create_window_ex_w(
        0,
        class.as_ptr(),
        title,
        style,
        x,
        y,
        w,
        h,
        parent,
        id as usize,
        0,
        std::ptr::null_mut(),
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn cap_create_capture_window_a(
    title: *const u8,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: usize,
    id: i32,
) -> usize {
    let t: Vec<u16> = if title.is_null() {
        vec![0]
    } else {
        super::cstr(title).encode_utf16().chain([0]).collect()
    };
    cap_create_capture_window_w(t.as_ptr(), style, x, y, w, h, parent, id)
}

// --------------------------------------------------------------------------
// credui
// --------------------------------------------------------------------------

/// « DOMAINE\utilisateur » ou « utilisateur@domaine » → (utilisateur, domaine).
pub(crate) fn parse_user_name(s: &str) -> Option<(String, String)> {
    if let Some((d, u)) = s.split_once('\\') {
        if d.is_empty() || u.is_empty() || u.contains('\\') {
            return None;
        }
        return Some((u.into(), d.into()));
    }
    if let Some((u, d)) = s.rsplit_once('@') {
        if u.is_empty() || d.is_empty() {
            return None;
        }
        return Some((u.into(), d.into()));
    }
    if s.is_empty() {
        None
    } else {
        Some((s.into(), String::new()))
    }
}

unsafe fn copy_w(dst: *mut u16, max: u32, text: &str) -> bool {
    let w: Vec<u16> = text.encode_utf16().collect();
    if dst.is_null() || w.len() + 1 > max as usize {
        return false;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), dst, w.len());
    *dst.add(w.len()) = 0;
    true
}

pub unsafe extern "win64" fn cred_ui_parse_user_name_w(
    name: *const u16,
    user: *mut u16,
    user_max: u32,
    domain: *mut u16,
    domain_max: u32,
) -> u32 {
    if name.is_null() || user.is_null() || domain.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let Some((u, d)) = parse_user_name(&super::wstr(name)) else {
        return ERROR_INVALID_ACCOUNT_NAME;
    };
    if !copy_w(user, user_max, &u) || !copy_w(domain, domain_max, &d) {
        return ERROR_INSUFFICIENT_BUFFER;
    }
    ERROR_SUCCESS
}

/// Aucun identifiant n'est jamais mis en attente de confirmation.
pub extern "win64" fn cred_ui_confirm_credentials_w(_target: *const u16, _ok: i32) -> u32 {
    ERROR_NOT_FOUND
}

/// La boîte de saisie graphique n'existe pas : le gestionnaire
/// d'identifiants est indisponible, ce que la documentation prévoit.
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn cred_ui_prompt_for_credentials_w(
    _info: usize,
    _target: *const u16,
    _reserved: usize,
    _auth_error: u32,
    _user: *mut u16,
    _user_max: u32,
    _password: *mut u16,
    _password_max: u32,
    _save: *mut i32,
    _flags: u32,
) -> u32 {
    ERROR_NO_SUCH_LOGON_SESSION
}

fn read_line(echo: bool) -> Option<String> {
    use std::io::BufRead;
    // SAFETY: termios de l'entrée standard, restauré avant de rendre.
    let saved = unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if !echo && libc::tcgetattr(0, &mut t) == 0 {
            let mut quiet = t;
            quiet.c_lflag &= !libc::ECHO;
            libc::tcsetattr(0, libc::TCSANOW, &quiet);
            Some(t)
        } else {
            None
        }
    };
    let mut line = String::new();
    let n = std::io::stdin().lock().read_line(&mut line).ok();
    if let Some(t) = saved {
        // SAFETY: rétablit l'état lu plus haut.
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &t) };
        eprintln!();
    }
    (n? > 0).then(|| line.trim_end_matches(['\r', '\n']).to_string())
}

/// Invite en console, comme la version Windows : nom puis mot de passe sans
/// écho, sur l'entrée et la sortie d'erreur standard.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn cred_ui_cmd_line_prompt_for_credentials_w(
    target: *const u16,
    _reserved: usize,
    _auth_error: u32,
    user: *mut u16,
    user_max: u32,
    password: *mut u16,
    password_max: u32,
    save: *mut i32,
    _flags: u32,
) -> u32 {
    if user.is_null() || password.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    let target = if target.is_null() {
        String::new()
    } else {
        super::wstr(target)
    };
    let current = super::wstr(user);
    if current.is_empty() {
        eprint!("Nom d'utilisateur pour {target} : ");
        let Some(u) = read_line(true) else {
            return ERROR_CANCELLED;
        };
        if !copy_w(user, user_max, &u) {
            return ERROR_INSUFFICIENT_BUFFER;
        }
    }
    eprint!("Mot de passe pour {target} : ");
    let Some(p) = read_line(false) else {
        return ERROR_CANCELLED;
    };
    if !copy_w(password, password_max, &p) {
        return ERROR_INSUFFICIENT_BUFFER;
    }
    put(save, 0);
    ERROR_SUCCESS
}

/// Pas d'authentification unique d'un domaine sur un poste Winlux.
pub unsafe extern "win64" fn cred_ui_read_sso_cred_w(_realm: *const u16, user: *mut usize) -> u32 {
    put(user, 0);
    ERROR_NOT_FOUND
}

pub extern "win64" fn cred_ui_store_sso_cred_w(
    _realm: *const u16,
    _user: *const u16,
    _password: *const u16,
    _persist: i32,
) -> u32 {
    ERROR_NOT_SUPPORTED
}

// --------------------------------------------------------------------------
// evr
// --------------------------------------------------------------------------

/// Échantillon vide de mfplat. Une surface Direct3D n'est pas encore
/// enveloppée dans un tampon : on le dit plutôt que de la perdre.
pub unsafe extern "win64" fn mf_create_video_sample_from_surface(
    surface: usize,
    sample: *mut *mut std::ffi::c_void,
) -> i32 {
    if sample.is_null() {
        return E_POINTER;
    }
    if surface != 0 {
        crate::log_warn!(
            "evr",
            "MFCreateVideoSampleFromSurface : surface non prise en charge"
        );
        *sample = std::ptr::null_mut();
        return E_NOTIMPL;
    }
    super::mfplat::mf_create_sample(sample)
}

// --------------------------------------------------------------------------
// ktmw32 : transactions du gestionnaire KTM, sans isolation.
//
// Linux n'a pas de Transactional NTFS. Sous Windows, CreateTransaction ne
// dépend pas du volume et réussit toujours ; seules les opérations de
// fichiers transactionnelles peuvent échouer (volume FAT, réseau). Le
// programme d'installation de Steam x86 traitait l'échec de
// CreateTransaction comme une installation impossible : il écrivait
// `.ntfs_transaction_failed`, ne mettait aucun fichier extrait en place et
// la mise à jour recommençait sans fin (#1475).
//
// Une transaction est donc une vraie poignée (un événement, que
// CloseHandle ferme), et les fonctions *Transacted de kernel32 agissent
// tout de suite. CommitTransaction n'a rien à valider ; RollbackTransaction
// ne peut pas défaire ce qui est déjà fait, et le journal le dit.
// --------------------------------------------------------------------------

fn transactions() -> &'static std::sync::Mutex<std::collections::HashSet<usize>> {
    static T: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<usize>>> =
        std::sync::OnceLock::new();
    T.get_or_init(Default::default)
}

fn is_transaction(h: usize) -> bool {
    transactions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&h)
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn create_transaction(
    _sa: usize,
    _uow: usize,
    _options: u32,
    _isolation: u32,
    _isolation_flags: u32,
    _timeout: u32,
    _description: *const u16,
) -> usize {
    // Événement à réinitialisation manuelle, non signalé : une poignée que
    // CloseHandle et DuplicateHandle connaissent.
    let h = unsafe { super::kernel32::create_event_w(0, 1, 0, std::ptr::null()) };
    if h == 0 || h == INVALID_HANDLE_VALUE {
        return INVALID_HANDLE_VALUE;
    }
    transactions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(h);
    crate::trace!("CreateTransaction = {h:#x} (opérations immédiates, sans isolation)");
    crate::process::set_last_error(ERROR_SUCCESS);
    h
}

pub extern "win64" fn open_transaction(_access: u32, _guid: usize) -> usize {
    crate::process::set_last_error(ERROR_NOT_SUPPORTED);
    INVALID_HANDLE_VALUE
}

/// CommitTransaction(Async) : tout est déjà écrit.
pub extern "win64" fn commit_transaction(h: usize) -> i32 {
    if !is_transaction(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    crate::trace!("CommitTransaction({h:#x})");
    1
}

/// RollbackTransaction(Async) : les opérations faites restent faites.
pub extern "win64" fn rollback_transaction(h: usize) -> i32 {
    if !is_transaction(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    crate::log_warn!(
        "ktmw32",
        "RollbackTransaction({h:#x}) : les opérations déjà faites ne sont pas défaites"
    );
    1
}

/// GetTransactionId : un GUID propre à la poignée.
pub unsafe extern "win64" fn get_transaction_id(h: usize, guid: *mut u8) -> i32 {
    if !is_transaction(h) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if guid.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut g = [0u8; 16];
    g[..8].copy_from_slice(&(h as u64).to_le_bytes());
    g[8..12].copy_from_slice(&std::process::id().to_le_bytes());
    g[12..].copy_from_slice(b"KTMw");
    std::ptr::copy_nonoverlapping(g.as_ptr(), guid, 16);
    1
}

// --------------------------------------------------------------------------
// sfc : aucun fichier protégé par Windows File Protection.
// --------------------------------------------------------------------------

pub extern "win64" fn sfc_is_file_protected(_rpc: usize, _path: *const u16) -> i32 {
    fail(ERROR_FILE_NOT_FOUND)
}

pub extern "win64" fn sfc_get_next_protected_file(_rpc: usize, _data: usize) -> i32 {
    fail(ERROR_NO_MORE_FILES)
}

// --------------------------------------------------------------------------
// loadperf : l'installation de compteurs n'est pas prise en charge.
// --------------------------------------------------------------------------

pub extern "win64" fn load_perf_counter_text_strings(_cmd: usize, _quiet: i32) -> u32 {
    ERROR_NOT_SUPPORTED
}

// --------------------------------------------------------------------------
// rasapi32 : aucune connexion d'accès à distance ni annuaire téléphonique.
// --------------------------------------------------------------------------

/// RasEnumConnectionsW / RasEnumEntriesW : liste vide, réussie.
pub unsafe extern "win64" fn ras_enum_connections_w(
    _buf: usize,
    _cb: *mut u32,
    count: *mut u32,
) -> u32 {
    if count.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    *count = 0;
    ERROR_SUCCESS
}

pub unsafe extern "win64" fn ras_enum_entries_w(
    _reserved: usize,
    _phonebook: *const u16,
    _buf: usize,
    _cb: *mut u32,
    count: *mut u32,
) -> u32 {
    ras_enum_connections_w(0, std::ptr::null_mut(), count)
}

pub unsafe extern "win64" fn ras_get_error_string_w(code: u32, buf: *mut u16, cch: u32) -> u32 {
    let text = format!("Erreur d'accès à distance {code}");
    if copy_w(buf, cch, &text) {
        ERROR_SUCCESS
    } else {
        ERROR_INSUFFICIENT_BUFFER
    }
}

/// Fonctions qui visent une connexion (HRASCONN) : il n'y en a aucune.
pub extern "win64" fn ras_no_connection(_conn: usize, _out: usize) -> u32 {
    ERROR_INVALID_HANDLE
}

/// Fonctions qui visent une entrée de l'annuaire : il n'y en a aucune.
pub extern "win64" fn ras_no_entry(_a: usize, _b: usize, _c: usize, _d: usize) -> u32 {
    ERROR_CANNOT_FIND_PHONEBOOK_ENTRY
}

pub extern "win64" fn ras_not_supported(_hwnd: usize, _phonebook: *const u16) -> u32 {
    ERROR_NOT_SUPPORTED
}

pub extern "win64" fn ras_free(_a: usize) {}

// --------------------------------------------------------------------------
// ntdsapi : le poste n'appartient à aucun domaine Active Directory.
// --------------------------------------------------------------------------

pub unsafe extern "win64" fn ds_bind_w(
    _dc: *const u16,
    _domain: *const u16,
    ds: *mut usize,
) -> u32 {
    put(ds, 0);
    ERROR_NO_SUCH_DOMAIN
}

pub extern "win64" fn ds_unbind_w(_ds: *mut usize) -> u32 {
    ERROR_SUCCESS
}

/// Fonctions qui demandent une liaison DsBind : aucune ne peut exister.
pub extern "win64" fn ds_no_binding(_ds: usize) -> u32 {
    ERROR_INVALID_HANDLE
}

pub extern "win64" fn ds_get_spn_w() -> u32 {
    ERROR_NOT_SUPPORTED
}

pub extern "win64" fn ds_free(_a: usize) {}

// --------------------------------------------------------------------------
// query : aucun filtre d'indexation (IFilter) enregistré.
// --------------------------------------------------------------------------

pub unsafe extern "win64" fn load_ifilter(_a: usize, _b: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_FAIL
}

pub fn authz_exports() -> crate::dlls::Exports {
    crate::exports! {
        "AuthzInitializeResourceManager" => fn authz_initialize_resource_manager,
        "AuthzInitializeContextFromSid" => fn authz_initialize_context_from_sid,
        "AuthzInitializeContextFromToken" => fn authz_initialize_context_from_token,
        "AuthzAccessCheck" => fn authz_access_check,
        "AuthzFreeContext" => fn authz_free,
        "AuthzFreeResourceManager" => fn authz_free,
        "AuthzFreeHandle" => fn authz_free,
    }
}

pub fn avicap32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "capGetDriverDescriptionA" => fn cap_get_driver_description_a,
        "capGetDriverDescriptionW" => fn cap_get_driver_description_w,
        "capCreateCaptureWindowA" => fn cap_create_capture_window_a,
        "capCreateCaptureWindowW" => fn cap_create_capture_window_w,
    }
}

pub fn credui_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CredUIParseUserNameW" => fn cred_ui_parse_user_name_w,
        "CredUIConfirmCredentialsW" => fn cred_ui_confirm_credentials_w,
        "CredUIPromptForCredentialsW" => fn cred_ui_prompt_for_credentials_w,
        "CredUICmdLinePromptForCredentialsW" => fn cred_ui_cmd_line_prompt_for_credentials_w,
        "CredUIReadSSOCredW" => fn cred_ui_read_sso_cred_w,
        "CredUIStoreSSOCredW" => fn cred_ui_store_sso_cred_w,
    }
}

pub fn evr_exports() -> crate::dlls::Exports {
    crate::exports! {
        "MFCreateVideoSampleFromSurface" => fn mf_create_video_sample_from_surface,
    }
}

pub fn ktmw32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateTransaction" => fn create_transaction,
        "OpenTransaction" => fn open_transaction,
        "CommitTransaction" => fn commit_transaction,
        "CommitTransactionAsync" => fn commit_transaction,
        "RollbackTransaction" => fn rollback_transaction,
        "RollbackTransactionAsync" => fn rollback_transaction,
        "GetTransactionId" => fn get_transaction_id,
    }
}

pub fn sfc_exports() -> crate::dlls::Exports {
    crate::exports! {
        "SfcIsFileProtected" => fn sfc_is_file_protected,
        "SfcGetNextProtectedFile" => fn sfc_get_next_protected_file,
    }
}

pub fn loadperf_exports() -> crate::dlls::Exports {
    crate::exports! {
        "LoadPerfCounterTextStringsA" => fn load_perf_counter_text_strings,
        "LoadPerfCounterTextStringsW" => fn load_perf_counter_text_strings,
        "UnloadPerfCounterTextStringsA" => fn load_perf_counter_text_strings,
        "UnloadPerfCounterTextStringsW" => fn load_perf_counter_text_strings,
    }
}

pub fn rasapi32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "RasEnumConnectionsW" => fn ras_enum_connections_w,
        "RasEnumEntriesW" => fn ras_enum_entries_w,
        "RasGetErrorStringW" => fn ras_get_error_string_w,
        "RasGetConnectStatusW" => fn ras_no_connection,
        "RasHangUpW" => fn ras_no_connection,
        "RasDialW" => fn ras_no_entry,
        "RasGetEntryDialParamsW" => fn ras_no_entry,
        "RasSetEntryDialParamsW" => fn ras_no_entry,
        "RasEditPhonebookEntryW" => fn ras_no_entry,
        "RasGetEapUserIdentityW" => fn ras_no_entry,
        "RasCreatePhonebookEntryW" => fn ras_not_supported,
        "RasFreeEapUserIdentityW" => fn ras_free,
    }
}

pub fn ntdsapi_exports() -> crate::dlls::Exports {
    crate::exports! {
        "DsBindW" => fn ds_bind_w,
        "DsUnBindW" => fn ds_unbind_w,
        "DsCrackNamesW" => fn ds_no_binding,
        "DsListDomainsInSiteW" => fn ds_no_binding,
        "DsListInfoForServerW" => fn ds_no_binding,
        "DsListRolesW" => fn ds_no_binding,
        "DsListServersForDomainInSiteW" => fn ds_no_binding,
        "DsListServersInSiteW" => fn ds_no_binding,
        "DsListSitesW" => fn ds_no_binding,
        "DsWriteAccountSpnW" => fn ds_no_binding,
        "DsGetSpnW" => fn ds_get_spn_w,
        "DsFreeNameResultW" => fn ds_free,
        "DsFreeSpnArrayW" => fn ds_free,
    }
}

pub fn query_exports() -> crate::dlls::Exports {
    crate::exports! {
        "LoadIFilter" => fn load_ifilter,
        "BindIFilterFromStorage" => fn load_ifilter,
        "BindIFilterFromStream" => fn load_ifilter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_d_utilisateur_credui() {
        let p = |s: &str| parse_user_name(s);
        assert_eq!(
            p("WINLUX\\damien"),
            Some(("damien".into(), "WINLUX".into()))
        );
        assert_eq!(
            p("damien@exemple.fr"),
            Some(("damien".into(), "exemple.fr".into()))
        );
        assert_eq!(p("damien"), Some(("damien".into(), String::new())));
        assert_eq!(p("\\damien"), None);
        assert_eq!(p(""), None);
        let name: Vec<u16> = "DOM\\moi".encode_utf16().chain([0]).collect();
        let (mut u, mut d) = ([0u16; 8], [0u16; 2]);
        unsafe {
            assert_eq!(
                cred_ui_parse_user_name_w(name.as_ptr(), u.as_mut_ptr(), 8, d.as_mut_ptr(), 2),
                ERROR_INSUFFICIENT_BUFFER
            );
            let mut d = [0u16; 8];
            assert_eq!(
                cred_ui_parse_user_name_w(name.as_ptr(), u.as_mut_ptr(), 8, d.as_mut_ptr(), 8),
                ERROR_SUCCESS
            );
            assert_eq!(super::super::wstr(u.as_ptr()), "moi");
            assert_eq!(super::super::wstr(d.as_ptr()), "DOM");
        }
    }

    #[test]
    fn authz_accorde_comme_access_check() {
        unsafe {
            let (mut rm, mut ctx) = (0usize, 0usize);
            let sid = [1u8; 12];
            assert_eq!(
                authz_initialize_resource_manager(0, 0, 0, 0, std::ptr::null(), &mut rm),
                1
            );
            assert_eq!(
                authz_initialize_context_from_sid(
                    0,
                    sid.as_ptr() as usize,
                    rm,
                    std::ptr::null(),
                    0,
                    0,
                    &mut ctx
                ),
                1
            );
            assert!(rm != 0 && ctx != 0 && rm != ctx);
            // MAXIMUM_ALLOWED : FILE_ALL_ACCESS.
            let request = [0x0200_0000u32, 0, 0, 0, 0, 0, 0, 0];
            let (mut granted, mut error) = (0u32, 99u32);
            let mut reply = [0u8; 32];
            reply[0..4].copy_from_slice(&1u32.to_le_bytes());
            reply[8..16].copy_from_slice(&(&mut granted as *mut u32 as usize).to_le_bytes());
            reply[24..32].copy_from_slice(&(&mut error as *mut u32 as usize).to_le_bytes());
            let sd = [0u8; 20];
            assert_eq!(
                authz_access_check(
                    0,
                    ctx,
                    request.as_ptr(),
                    0,
                    sd.as_ptr() as usize,
                    0,
                    0,
                    reply.as_mut_ptr(),
                    std::ptr::null_mut()
                ),
                1
            );
            assert_eq!(granted & 0x001F_01FF, 0x001F_01FF);
            assert_eq!(error, 0);
        }
    }

    #[test]
    fn transaction_sans_isolation() {
        // #1475 : CreateTransaction réussit comme sous Windows ; la
        // poignée se valide, s'annule (sans rien défaire) et se ferme.
        let h = create_transaction(0, 0, 0, 0, 0, 0, std::ptr::null());
        assert!(h != 0 && h != INVALID_HANDLE_VALUE);
        let mut guid = [0u8; 16];
        assert_eq!(unsafe { get_transaction_id(h, guid.as_mut_ptr()) }, 1);
        assert_ne!(guid, [0u8; 16]);
        assert_eq!(commit_transaction(h), 1);
        assert_eq!(rollback_transaction(h), 1);
        assert_eq!(commit_transaction(0x1234), 0);
        assert_eq!(crate::dlls::kernel32::close_handle(h), 1);
    }

    #[test]
    fn dll_du_corpus_fournies() {
        // #26 : chaque DLL que le corpus importe en direct est servie.
        for (dll, f) in [
            ("authz", "AuthzAccessCheck"),
            ("avicap32", "capGetDriverDescriptionA"),
            ("credui", "CredUIParseUserNameW"),
            ("evr", "MFCreateVideoSampleFromSurface"),
            ("ktmw32", "CreateTransaction"),
            ("sfc", "SfcIsFileProtected"),
            ("loadperf", "LoadPerfCounterTextStringsA"),
            ("rasapi32", "RasEnumConnectionsW"),
            ("ntdsapi", "DsBindW"),
            ("query", "LoadIFilter"),
            ("imagehlp", "SymGetSymFromAddr64"),
            ("imagehlp", "CheckSumMappedFile"),
        ] {
            let n = crate::dlls::normalize(dll);
            assert!(crate::dlls::lookup(&n, f).is_some(), "{dll}!{f}");
        }
        unsafe {
            let mut n = 0u32;
            assert_eq!(ras_enum_connections_w(0, std::ptr::null_mut(), &mut n), 0);
            assert_eq!(n, 0);
        }
    }
}
