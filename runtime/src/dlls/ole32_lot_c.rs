//! ole32, lot C du relevé des fonctions manquantes (#1511) : marshaling
//! dans le processus, OleLoadFromStream et OleSaveToStream, espions
//! d'initialisation (IInitializeSpy), annulation d'appel, type
//! d'appartement, noms de flux de jeux de propriétés, et le reste de
//! l'API Windows Runtime d'ole32 (combase).
//!
//! Sources : documentation Microsoft Learn de chaque fonction et
//! interface (objidl.h, objbase.h, roapi.h, roerrorapi.h, combaseapi.h),
//! et [MS-OLEPS] § 2.25 pour le nom d'un flux de propriétés. Les noms
//! exportés viennent de la bibliothèque d'importation Ole32.Lib du SDK
//! Windows public (aucun ordinal : ole32 n'exporte que par nom).
//!
//! Winlux n'a ni DCOM ni appartements séparés par des proxys : un bloc
//! marshalé ne vaut que dans le processus qui l'a écrit. Ailleurs,
//! CoUnmarshalInterface répond CO_E_OBJNOTCONNECTED, sans planter.

use super::comobj::{guid, Guid};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const CO_E_NOTINITIALIZED: i32 = 0x8004_01F0u32 as i32;
const CO_E_OBJNOTCONNECTED: i32 = 0x8004_01FDu32 as i32;
const CO_E_CANCEL_DISABLED: i32 = 0x8001_0140u32 as i32;
const RPC_E_CALL_COMPLETE: i32 = 0x8001_0117u32 as i32;
const REGDB_E_CLASSNOTREG: i32 = 0x8004_0154u32 as i32;
const REGDB_E_WRITEREGDB: i32 = 0x8004_0151u32 as i32;
const STG_E_READFAULT: i32 = 0x8003_001Eu32 as i32;
const STG_E_MEDIUMFULL: i32 = 0x8003_0070u32 as i32;
const STG_E_INVALIDNAME: i32 = 0x8003_00FCu32 as i32;
const STG_E_INVALIDPOINTER: i32 = 0x8003_0009u32 as i32;

const fn ole_guid(d1: u32) -> Guid {
    guid(d1, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46])
}

pub(crate) const IID_IUNKNOWN: Guid = ole_guid(0);
const IID_IPERSIST_STREAM: Guid = ole_guid(0x0000_0109);
const IID_IINITIALIZE_SPY: Guid = ole_guid(0x0000_0034);
const IID_IAGILE_REFERENCE: Guid = guid(
    0xC03F_6A43,
    0x65A4,
    0x9818,
    [0x98, 0x7E, 0xE0, 0xB8, 0x10, 0xD2, 0xA6, 0xF2],
);
const GUID_NULL: Guid = [0; 16];

// ---------- Appels d'interface ----------

pub(crate) unsafe fn slot(obj: usize, n: usize) -> usize {
    *(*(obj as *const *const usize)).add(n)
}

pub(crate) unsafe fn add_ref(obj: usize) -> u32 {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 1));
    f(obj)
}

pub(crate) unsafe fn release(obj: usize) -> u32 {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 2));
    f(obj)
}

pub(crate) unsafe fn query(obj: usize, iid: *const Guid, out: *mut usize) -> i32 {
    let f: unsafe extern "win64" fn(usize, *const Guid, *mut usize) -> i32 =
        std::mem::transmute(slot(obj, 0));
    f(obj, iid, out)
}

pub(crate) unsafe fn read_guid(p: *const Guid) -> Option<Guid> {
    if p.is_null() {
        None
    } else {
        Some(std::ptr::read_unaligned(p))
    }
}

/// IStream::Write de tout le tampon.
pub(crate) unsafe fn stream_write(stm: usize, bytes: &[u8]) -> i32 {
    let f: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stm, 4));
    let mut done = 0u32;
    let hr = f(stm, bytes.as_ptr(), bytes.len() as u32, &mut done);
    if hr < 0 {
        hr
    } else if done as usize != bytes.len() {
        STG_E_MEDIUMFULL
    } else {
        S_OK
    }
}

/// IStream::Read d'exactement `buf.len()` octets.
pub(crate) unsafe fn stream_read_exact(stm: usize, buf: &mut [u8]) -> i32 {
    let f: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stm, 3));
    let mut got = 0u32;
    let hr = f(stm, buf.as_mut_ptr(), buf.len() as u32, &mut got);
    if hr < 0 {
        hr
    } else if got as usize != buf.len() {
        STG_E_READFAULT
    } else {
        S_OK
    }
}

// ---------- Marshaling dans le processus ----------

/// Bloc écrit dans le flux : « WLXM », drapeaux MSHLFLAGS, processus,
/// réservé, jeton, IID marshalé.
const MARSHAL_MAGIC: u32 = u32::from_le_bytes(*b"WLXM");
const MARSHAL_SIZE: usize = 40;
const MSHLFLAGS_NORMAL: u32 = 0;

/// Interface gardée (référence prise) par jeton ; le bloc porte les
/// drapeaux MSHLFLAGS.
struct Marshaled {
    itf: usize,
}

fn marshal_table() -> &'static Mutex<HashMap<u64, Marshaled>> {
    static T: OnceLock<Mutex<HashMap<u64, Marshaled>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn table_lock() -> std::sync::MutexGuard<'static, HashMap<u64, Marshaled>> {
    marshal_table().lock().unwrap_or_else(|e| e.into_inner())
}

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

pub unsafe extern "win64" fn co_get_marshal_size_max(
    size: *mut u32,
    _iid: *const Guid,
    punk: usize,
    _dest: u32,
    _dest_data: usize,
    _flags: u32,
) -> i32 {
    if size.is_null() {
        return E_POINTER;
    }
    *size = 0;
    if punk == 0 {
        return E_INVALIDARG;
    }
    *size = MARSHAL_SIZE as u32;
    S_OK
}

pub unsafe extern "win64" fn co_marshal_interface(
    stm: usize,
    iid: *const Guid,
    punk: usize,
    _dest: u32,
    _dest_data: usize,
    flags: u32,
) -> i32 {
    let Some(g) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    if stm == 0 || punk == 0 {
        return E_INVALIDARG;
    }
    let mut itf = 0usize;
    let hr = query(punk, iid, &mut itf);
    if hr < 0 || itf == 0 {
        return if hr < 0 { hr } else { E_NOINTERFACE };
    }
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    table_lock().insert(token, Marshaled { itf });
    let mut block = [0u8; MARSHAL_SIZE];
    block[0..4].copy_from_slice(&MARSHAL_MAGIC.to_le_bytes());
    block[4..8].copy_from_slice(&flags.to_le_bytes());
    block[8..12].copy_from_slice(&std::process::id().to_le_bytes());
    block[16..24].copy_from_slice(&token.to_le_bytes());
    block[24..40].copy_from_slice(&g);
    let hr = stream_write(stm, &block);
    if hr < 0 {
        if let Some(m) = table_lock().remove(&token) {
            release(m.itf);
        }
    }
    hr
}

/// Lit un bloc et rend (drapeaux, jeton, IID) s'il vient de ce processus.
unsafe fn read_block(stm: usize) -> Result<(u32, u64, Guid), i32> {
    if stm == 0 {
        return Err(E_INVALIDARG);
    }
    let mut block = [0u8; MARSHAL_SIZE];
    let hr = stream_read_exact(stm, &mut block);
    if hr < 0 {
        return Err(hr);
    }
    let word = |i: usize| u32::from_le_bytes([block[i], block[i + 1], block[i + 2], block[i + 3]]);
    if word(0) != MARSHAL_MAGIC {
        return Err(E_INVALIDARG);
    }
    if word(8) != std::process::id() {
        return Err(CO_E_OBJNOTCONNECTED);
    }
    let mut token = [0u8; 8];
    token.copy_from_slice(&block[16..24]);
    let mut g = [0u8; 16];
    g.copy_from_slice(&block[24..40]);
    Ok((word(4), u64::from_le_bytes(token), g))
}

pub unsafe extern "win64" fn co_unmarshal_interface(
    stm: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let (flags, token, marshaled_iid) = match read_block(stm) {
        Ok(b) => b,
        Err(hr) => return hr,
    };
    // IID_NULL : l'interface marshalée elle-même.
    let wanted = match read_guid(iid) {
        Some(g) if g != GUID_NULL => g,
        _ => marshaled_iid,
    };
    let held = {
        let mut t = table_lock();
        if flags == MSHLFLAGS_NORMAL {
            t.remove(&token).map(|m| (m.itf, true))
        } else {
            t.get(&token).map(|m| (m.itf, false))
        }
    };
    let Some((itf, owned)) = held else {
        return CO_E_OBJNOTCONNECTED;
    };
    let hr = query(itf, &wanted, out);
    if owned {
        release(itf);
    }
    hr
}

pub unsafe extern "win64" fn co_release_marshal_data(stm: usize) -> i32 {
    let (_, token, _) = match read_block(stm) {
        Ok(b) => b,
        Err(hr) => return hr,
    };
    match table_lock().remove(&token) {
        Some(m) => {
            release(m.itf);
            S_OK
        }
        None => CO_E_OBJNOTCONNECTED,
    }
}

// ---------- OleSaveToStream / OleLoadFromStream ----------

pub unsafe extern "win64" fn ole_save_to_stream(obj: usize, stm: usize) -> i32 {
    if stm == 0 {
        return STG_E_INVALIDPOINTER;
    }
    let write_clsid = |g: &Guid| {
        super::ole32_storage::write_class_stm(stm, g.as_ptr() as *const super::ole32::Guid)
    };
    if obj == 0 {
        // Objet nul : CLSID_NULL seul, comme documenté.
        return write_clsid(&GUID_NULL);
    }
    let get_class: unsafe extern "win64" fn(usize, *mut Guid) -> i32 =
        std::mem::transmute(slot(obj, 3));
    let mut clsid = GUID_NULL;
    let hr = get_class(obj, &mut clsid);
    if hr < 0 {
        return hr;
    }
    let hr = write_clsid(&clsid);
    if hr < 0 {
        return hr;
    }
    let save: unsafe extern "win64" fn(usize, usize, i32) -> i32 =
        std::mem::transmute(slot(obj, 6));
    save(obj, stm, 1)
}

pub unsafe extern "win64" fn ole_load_from_stream(
    stm: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if stm == 0 || iid.is_null() {
        return E_INVALIDARG;
    }
    let mut clsid = super::ole32::Guid::default();
    let hr = super::ole32_storage::read_class_stm(stm, &mut clsid);
    if hr < 0 {
        return hr;
    }
    if clsid == super::ole32::Guid::default() {
        return REGDB_E_CLASSNOTREG;
    }
    let mut ps: *mut c_void = std::ptr::null_mut();
    // CLSCTX_INPROC_SERVER | INPROC_HANDLER | LOCAL_SERVER.
    let hr = super::ole32::co_create_instance(
        &clsid,
        0,
        0x7,
        IID_IPERSIST_STREAM.as_ptr() as *const super::ole32::Guid,
        &mut ps,
    );
    if hr < 0 || ps.is_null() {
        return if hr < 0 { hr } else { E_NOINTERFACE };
    }
    let ps = ps as usize;
    let load: unsafe extern "win64" fn(usize, usize) -> i32 = std::mem::transmute(slot(ps, 5));
    let mut hr = load(ps, stm);
    if hr >= 0 {
        hr = query(ps, iid, out);
    }
    release(ps);
    hr
}

// ---------- Espions d'initialisation (IInitializeSpy) ----------

thread_local! {
    static SPIES: RefCell<Vec<(u64, usize)>> = const { RefCell::new(Vec::new()) };
    static CANCEL_ENABLED: Cell<u32> = const { Cell::new(0) };
    static CANCEL_OBJECT: Cell<usize> = const { Cell::new(0) };
    static RESTRICTED_ERROR: Cell<usize> = const { Cell::new(0) };
}

static NEXT_COOKIE: AtomicU64 = AtomicU64::new(1);

pub unsafe extern "win64" fn co_register_initialize_spy(spy: usize, cookie: *mut u64) -> i32 {
    if cookie.is_null() || spy == 0 {
        return E_INVALIDARG;
    }
    *cookie = 0;
    let mut itf = 0usize;
    if query(spy, &IID_IINITIALIZE_SPY, &mut itf) < 0 || itf == 0 {
        return E_NOINTERFACE;
    }
    let c = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed);
    SPIES.with(|s| s.borrow_mut().push((c, itf)));
    *cookie = c;
    S_OK
}

pub unsafe extern "win64" fn co_revoke_initialize_spy(cookie: u64) -> i32 {
    let found = SPIES.with(|s| {
        let mut s = s.borrow_mut();
        s.iter()
            .position(|(c, _)| *c == cookie)
            .map(|i| s.remove(i).1)
    });
    match found {
        Some(itf) => {
            release(itf);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

/// Copie de la liste, références prises : un espion peut se retirer
/// pendant son propre rappel.
unsafe fn spies() -> Vec<usize> {
    let list: Vec<usize> = SPIES.with(|s| s.borrow().iter().map(|(_, p)| *p).collect());
    for &p in &list {
        add_ref(p);
    }
    list
}

unsafe fn release_all(list: Vec<usize>) {
    for p in list {
        release(p);
    }
}

pub(crate) unsafe fn before_initialize(coinit: u32, refs: u32) {
    if SPIES.with(|s| s.borrow().is_empty()) {
        return;
    }
    let list = spies();
    for &s in list.iter().rev() {
        let f: unsafe extern "win64" fn(usize, u32, u32) -> i32 = std::mem::transmute(slot(s, 3));
        f(s, coinit, refs);
    }
    release_all(list);
}

pub(crate) unsafe fn after_initialize(hr: i32, coinit: u32, refs: u32) -> i32 {
    if SPIES.with(|s| s.borrow().is_empty()) {
        return hr;
    }
    let list = spies();
    let mut hr = hr;
    for &s in &list {
        let f: unsafe extern "win64" fn(usize, i32, u32, u32) -> i32 =
            std::mem::transmute(slot(s, 4));
        hr = f(s, hr, coinit, refs);
    }
    release_all(list);
    hr
}

pub(crate) unsafe fn before_uninitialize(refs: u32) {
    if SPIES.with(|s| s.borrow().is_empty()) {
        return;
    }
    let list = spies();
    for &s in list.iter().rev() {
        let f: unsafe extern "win64" fn(usize, u32) -> i32 = std::mem::transmute(slot(s, 5));
        f(s, refs);
    }
    release_all(list);
}

pub(crate) unsafe fn after_uninitialize(refs: u32) {
    if SPIES.with(|s| s.borrow().is_empty()) {
        return;
    }
    let list = spies();
    for &s in &list {
        let f: unsafe extern "win64" fn(usize, u32) -> i32 = std::mem::transmute(slot(s, 6));
        f(s, refs);
    }
    release_all(list);
}

// ---------- Annulation et contexte d'appel ----------

/// Aucun appel sortant n'est jamais en cours : il n'y a rien à annuler,
/// mais l'activation est comptée comme sous Windows.
pub extern "win64" fn co_enable_call_cancellation(reserved: usize) -> i32 {
    if reserved != 0 {
        return E_INVALIDARG;
    }
    CANCEL_ENABLED.with(|c| c.set(c.get().saturating_add(1)));
    S_OK
}

pub extern "win64" fn co_disable_call_cancellation(reserved: usize) -> i32 {
    if reserved != 0 {
        return E_INVALIDARG;
    }
    CANCEL_ENABLED.with(|c| match c.get() {
        0 => CO_E_CANCEL_DISABLED,
        n => {
            c.set(n - 1);
            S_OK
        }
    })
}

pub unsafe extern "win64" fn co_get_cancel_object(
    _thread: u32,
    _iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    RPC_E_CALL_COMPLETE
}

pub unsafe extern "win64" fn co_set_cancel_object(punk: usize) -> i32 {
    if punk != 0 {
        add_ref(punk);
    }
    let old = CANCEL_OBJECT.with(|c| c.replace(punk));
    if old != 0 {
        release(old);
    }
    S_OK
}

/// Aucun appel entrant n'est servi par un proxy : pas de contexte d'appel.
pub unsafe extern "win64" fn co_get_call_context(_iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    *out = 0;
    RPC_E_CALL_COMPLETE
}

// ---------- Appartement, processus, heure ----------

const APTTYPE_CURRENT: i32 = -1;
const APTTYPE_STA: i32 = 0;
const APTTYPE_MTA: i32 = 1;
const APTTYPEQUALIFIER_NONE: i32 = 0;
const APTTYPEQUALIFIER_IMPLICIT_MTA: i32 = 1;

pub unsafe extern "win64" fn co_get_apartment_type(kind: *mut i32, qualifier: *mut i32) -> i32 {
    if kind.is_null() || qualifier.is_null() {
        return E_INVALIDARG;
    }
    let (k, q, hr) = match super::ole32::current_apartment() {
        Some((2, _)) => (APTTYPE_STA, APTTYPEQUALIFIER_NONE, S_OK),
        Some(_) => (APTTYPE_MTA, APTTYPEQUALIFIER_NONE, S_OK),
        // Thread non initialisé alors que le MTA existe (CoIncrementMTAUsage).
        None if super::ole32::mta_usage() > 0 => (APTTYPE_MTA, APTTYPEQUALIFIER_IMPLICIT_MTA, S_OK),
        None => (APTTYPE_CURRENT, APTTYPEQUALIFIER_NONE, CO_E_NOTINITIALIZED),
    };
    *kind = k;
    *qualifier = q;
    hr
}

/// Identifiant d'appartement : le thread pour un STA, une valeur fixe
/// pour l'unique MTA du processus.
const MTA_IDENTIFIER: u64 = 0x4D54_4100;

fn apartment_identifier() -> Option<u64> {
    match super::ole32::current_apartment() {
        Some((2, _)) => Some(unsafe { libc::gettid() } as u64),
        Some(_) => Some(MTA_IDENTIFIER),
        None if super::ole32::mta_usage() > 0 => Some(MTA_IDENTIFIER),
        None => None,
    }
}

pub extern "win64" fn co_get_current_process() -> u32 {
    std::process::id()
}

/// FILETIME : centaines de nanosecondes depuis le 1er janvier 1601 (UTC).
pub unsafe extern "win64" fn co_file_time_now(out: *mut u64) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ticks = since.as_secs() * 10_000_000 + u64::from(since.subsec_nanos() / 100);
    std::ptr::write_unaligned(out, ticks + 116_444_736_000_000_000);
    S_OK
}

/// CoTreatAsClass : clé TreatAs de l'ancienne classe ; CLSID_NULL, ou la
/// classe elle-même, retire l'émulation.
pub unsafe extern "win64" fn co_treat_as_class(old: *const Guid, new: *const Guid) -> i32 {
    let Some(old) = read_guid(old) else {
        return E_INVALIDARG;
    };
    let new = read_guid(new).unwrap_or(GUID_NULL);
    let key = format!(
        "hkcr\\clsid\\{}\\treatas",
        super::comobj::guid_str(&old).to_lowercase()
    );
    let remove = new == GUID_NULL || new == old;
    let value = super::comobj::guid_str(&new);
    let done = crate::registry::write(|d| {
        if remove {
            d.delete_tree(&key);
        } else {
            d.ensure(&key);
            d.set(
                &key,
                "",
                crate::registry::REG_SZ,
                crate::registry::utf16z(&value),
            );
        }
        Ok(())
    });
    if done.is_ok() {
        S_OK
    } else {
        REGDB_E_WRITEREGDB
    }
}

// ---------- Noms de flux des jeux de propriétés ([MS-OLEPS] § 2.25) ----------

const FMTID_SUMMARY_INFORMATION: Guid = guid(
    0xF29F_85E0,
    0x4FF9,
    0x1068,
    [0xAB, 0x91, 0x08, 0x00, 0x2B, 0x27, 0xB3, 0xD9],
);
const FMTID_DOC_SUMMARY_INFORMATION: Guid = guid(
    0xD5CD_D502,
    0x2E9C,
    0x101B,
    [0x93, 0x97, 0x08, 0x00, 0x2B, 0x2C, 0xF9, 0xAE],
);
const FMTID_USER_DEFINED_PROPERTIES: Guid = guid(
    0xD5CD_D505,
    0x2E9C,
    0x101B,
    [0x93, 0x97, 0x08, 0x00, 0x2B, 0x2C, 0xF9, 0xAE],
);
const SUMMARY_NAME: &str = "\u{5}SummaryInformation";
const DOC_SUMMARY_NAME: &str = "\u{5}DocumentSummaryInformation";
const NAME_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz012345";
/// 128 bits par groupes de 5 : 26 caractères.
const NAME_CHARS: usize = 26;

pub(crate) fn fmtid_to_name(g: &Guid) -> String {
    if *g == FMTID_SUMMARY_INFORMATION {
        return SUMMARY_NAME.into();
    }
    if *g == FMTID_DOC_SUMMARY_INFORMATION || *g == FMTID_USER_DEFINED_PROPERTIES {
        return DOC_SUMMARY_NAME.into();
    }
    let value = u128::from_le_bytes(*g);
    let mut s = String::from("\u{5}");
    for i in 0..NAME_CHARS {
        s.push(NAME_ALPHABET[((value >> (5 * i)) & 31) as usize] as char);
    }
    s
}

pub(crate) fn name_to_fmtid(name: &str) -> Option<Guid> {
    let body = name.strip_prefix('\u{5}').unwrap_or(name);
    if body.eq_ignore_ascii_case(&SUMMARY_NAME[1..]) {
        return Some(FMTID_SUMMARY_INFORMATION);
    }
    if body.eq_ignore_ascii_case(&DOC_SUMMARY_NAME[1..]) {
        return Some(FMTID_DOC_SUMMARY_INFORMATION);
    }
    if body.len() != NAME_CHARS {
        return None;
    }
    let mut value = 0u128;
    for (i, c) in body.bytes().enumerate() {
        let c = c.to_ascii_lowercase();
        let v = NAME_ALPHABET.iter().position(|&a| a == c)? as u128;
        // Le dernier caractère ne porte que 3 bits.
        if i == NAME_CHARS - 1 && v > 7 {
            return None;
        }
        value |= v << (5 * i);
    }
    Some(value.to_le_bytes())
}

/// CCH_MAX_PROPSTG_NAME (31) plus le zéro final.
const PROPSTG_NAME_BUFFER: usize = 32;

pub unsafe extern "win64" fn fmt_id_to_prop_stg_name(fmtid: *const Guid, name: *mut u16) -> i32 {
    let Some(g) = read_guid(fmtid) else {
        return E_INVALIDARG;
    };
    if name.is_null() {
        return E_INVALIDARG;
    }
    let w: Vec<u16> = fmtid_to_name(&g).encode_utf16().collect();
    let n = w.len().min(PROPSTG_NAME_BUFFER - 1);
    std::ptr::copy_nonoverlapping(w.as_ptr(), name, n);
    *name.add(n) = 0;
    S_OK
}

pub unsafe extern "win64" fn prop_stg_name_to_fmt_id(name: *const u16, fmtid: *mut Guid) -> i32 {
    if name.is_null() || fmtid.is_null() {
        return E_INVALIDARG;
    }
    let text = super::oleaut32_typelib::wide(name);
    match name_to_fmtid(&text) {
        Some(g) => {
            std::ptr::write_unaligned(fmtid, g);
            S_OK
        }
        None => STG_E_INVALIDNAME,
    }
}

// ---------- Windows Runtime ----------

/// IAgileReference : garde l'interface demandée ; Resolve est un
/// QueryInterface, tous les threads du processus partageant les objets.
#[repr(C)]
struct AgileRef {
    vtbl: *const (),
    refs: AtomicU32,
    itf: usize,
}

static AGILE_VT: super::comobj::VTable<4> = super::comobj::VTable([
    agile_qi as *const (),
    agile_add_ref as *const (),
    agile_release as *const (),
    agile_resolve as *const (),
]);

unsafe extern "win64" fn agile_qi(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    match read_guid(iid) {
        Some(g) if g == IID_IUNKNOWN || g == IID_IAGILE_REFERENCE => {
            agile_add_ref(this);
            *out = this;
            S_OK
        }
        Some(_) => E_NOINTERFACE,
        None => E_INVALIDARG,
    }
}

unsafe extern "win64" fn agile_add_ref(this: usize) -> u32 {
    (*(this as *const AgileRef))
        .refs
        .fetch_add(1, Ordering::AcqRel)
        + 1
}

unsafe extern "win64" fn agile_release(this: usize) -> u32 {
    let left = (*(this as *const AgileRef))
        .refs
        .fetch_sub(1, Ordering::AcqRel)
        - 1;
    if left == 0 {
        let b = Box::from_raw(this as *mut AgileRef);
        release(b.itf);
    }
    left
}

unsafe extern "win64" fn agile_resolve(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if iid.is_null() {
        return E_INVALIDARG;
    }
    query((*(this as *const AgileRef)).itf, iid, out)
}

pub unsafe extern "win64" fn ro_get_agile_reference(
    options: u32,
    iid: *const Guid,
    punk: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    // AGILEREFERENCE_DEFAULT (0) ou AGILEREFERENCE_DELAYEDMARSHAL (1).
    if options > 1 || iid.is_null() || punk == 0 {
        return E_INVALIDARG;
    }
    let mut itf = 0usize;
    let hr = query(punk, iid, &mut itf);
    if hr < 0 || itf == 0 {
        return if hr < 0 { hr } else { E_NOINTERFACE };
    }
    let obj = Box::new(AgileRef {
        vtbl: &AGILE_VT as *const _ as *const (),
        refs: AtomicU32::new(1),
        itf,
    });
    *out = Box::into_raw(obj) as usize;
    S_OK
}

pub unsafe extern "win64" fn ro_get_apartment_identifier(out: *mut u64) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    match apartment_identifier() {
        Some(id) => {
            std::ptr::write_unaligned(out, id);
            S_OK
        }
        None => {
            std::ptr::write_unaligned(out, 0);
            CO_E_NOTINITIALIZED
        }
    }
}

/// L'IID d'un type paramétré se calcule depuis les métadonnées WinMD,
/// que Winlux ne lit pas : échec propre, sorties remises à zéro.
pub unsafe extern "win64" fn ro_get_parameterized_type_instance_iid(
    _count: u32,
    _names: usize,
    _locator: usize,
    iid: *mut Guid,
    extra: *mut usize,
) -> i32 {
    if !iid.is_null() {
        std::ptr::write_unaligned(iid, GUID_NULL);
    }
    if !extra.is_null() {
        *extra = 0;
    }
    E_NOTIMPL
}

/// Comme RoOriginateError : aucune information d'erreur n'est enregistrée
/// pour le débogueur (FALSE).
pub extern "win64" fn ro_originate_language_exception(_hr: i32, _msg: usize, _exc: usize) -> i32 {
    0
}

/// Fabriques d'activation inscrites par le programme lui-même
/// (RoRegisterActivationFactories), consultées par RoGetActivationFactory.
fn factories() -> &'static Mutex<Vec<(usize, String, usize)>> {
    static F: Mutex<Vec<(usize, String, usize)>> = Mutex::new(Vec::new());
    &F
}

pub unsafe extern "win64" fn ro_register_activation_factories(
    classes: *const usize,
    getters: *const usize,
    count: u32,
    cookie: *mut usize,
) -> i32 {
    if cookie.is_null() || (count > 0 && (classes.is_null() || getters.is_null())) {
        return E_INVALIDARG;
    }
    *cookie = 0;
    let c = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed) as usize;
    let mut f = factories().lock().unwrap_or_else(|e| e.into_inner());
    for i in 0..count as usize {
        let name = String::from_utf16_lossy(super::electron_win32::hstring_slice(*classes.add(i)));
        let getter = *getters.add(i);
        if getter == 0 {
            f.retain(|(k, _, _)| *k != c);
            return E_INVALIDARG;
        }
        f.push((c, name, getter));
    }
    *cookie = c;
    S_OK
}

pub extern "win64" fn ro_revoke_activation_factories(cookie: usize) {
    factories()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(k, _, _)| *k != cookie);
}

/// Fabrique inscrite par le programme pour cette classe :
/// PFNGETACTIVATIONFACTORY puis QueryInterface vers `iid`.
pub(crate) unsafe fn registered_activation_factory(
    class: usize,
    name: &str,
    iid: *const u8,
    out: *mut usize,
) -> Option<i32> {
    let getter = factories()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .rev()
        .find(|(_, n, _)| n == name)
        .map(|(_, _, g)| *g)?;
    if out.is_null() {
        return Some(E_POINTER);
    }
    *out = 0;
    let get: unsafe extern "win64" fn(usize, *mut usize) -> i32 = std::mem::transmute(getter);
    let mut factory = 0usize;
    let hr = get(class, &mut factory);
    if hr < 0 || factory == 0 {
        return Some(if hr < 0 { hr } else { E_NOINTERFACE });
    }
    let hr = if iid.is_null() {
        *out = factory;
        add_ref(factory);
        S_OK
    } else {
        query(factory, iid as *const Guid, out)
    };
    release(factory);
    Some(hr)
}

/// Rappels IApartmentShutdown : (jeton, thread, appartement, rappel).
fn shutdown_list() -> &'static Mutex<Vec<(usize, i32, u64, usize)>> {
    static L: Mutex<Vec<(usize, i32, u64, usize)>> = Mutex::new(Vec::new());
    &L
}

pub unsafe extern "win64" fn ro_register_for_apartment_shutdown(
    callback: usize,
    apartment: *mut u64,
    cookie: *mut usize,
) -> i32 {
    if callback == 0 || apartment.is_null() || cookie.is_null() {
        return E_INVALIDARG;
    }
    *cookie = 0;
    let Some(id) = apartment_identifier() else {
        std::ptr::write_unaligned(apartment, 0);
        return CO_E_NOTINITIALIZED;
    };
    add_ref(callback);
    let c = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed) as usize;
    shutdown_list()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((c, libc::gettid(), id, callback));
    std::ptr::write_unaligned(apartment, id);
    *cookie = c;
    S_OK
}

pub unsafe extern "win64" fn ro_unregister_for_apartment_shutdown(cookie: usize) -> i32 {
    let found = {
        let mut l = shutdown_list().lock().unwrap_or_else(|e| e.into_inner());
        l.iter()
            .position(|(c, ..)| *c == cookie)
            .map(|i| l.remove(i).3)
    };
    match found {
        Some(cb) => {
            release(cb);
            S_OK
        }
        None => E_INVALIDARG,
    }
}

/// Dernier CoUninitialize du thread : IApartmentShutdown::OnUninitialize
/// pour chaque rappel inscrit depuis ce thread.
pub(crate) unsafe fn apartment_shutdown() {
    let Some(id) = apartment_identifier() else {
        return;
    };
    let tid = libc::gettid();
    let due: Vec<usize> = {
        let mut l = shutdown_list().lock().unwrap_or_else(|e| e.into_inner());
        let mut due = Vec::new();
        l.retain(|&(_, t, a, cb)| {
            if t == tid && a == id {
                due.push(cb);
                false
            } else {
                true
            }
        });
        due
    };
    for cb in due {
        let f: unsafe extern "win64" fn(usize, u64) = std::mem::transmute(slot(cb, 3));
        f(cb, id);
        release(cb);
    }
}

/// SetRestrictedErrorInfo : l'erreur du thread, rendue (et cédée) par
/// GetRestrictedErrorInfo.
pub unsafe extern "win64" fn set_restricted_error_info(info: usize) -> i32 {
    if info != 0 {
        add_ref(info);
    }
    let old = RESTRICTED_ERROR.with(|c| c.replace(info));
    if old != 0 {
        release(old);
    }
    S_OK
}

pub(crate) fn take_restricted_error_info() -> usize {
    RESTRICTED_ERROR.with(|c| c.replace(0))
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CoGetMarshalSizeMax" => fn co_get_marshal_size_max,
        "CoMarshalInterface" => fn co_marshal_interface,
        "CoUnmarshalInterface" => fn co_unmarshal_interface,
        "CoReleaseMarshalData" => fn co_release_marshal_data,
        "OleSaveToStream" => fn ole_save_to_stream,
        "OleLoadFromStream" => fn ole_load_from_stream,
        "CoRegisterInitializeSpy" => fn co_register_initialize_spy,
        "CoRevokeInitializeSpy" => fn co_revoke_initialize_spy,
        "CoEnableCallCancellation" => fn co_enable_call_cancellation,
        "CoDisableCallCancellation" => fn co_disable_call_cancellation,
        "CoGetCancelObject" => fn co_get_cancel_object,
        "CoSetCancelObject" => fn co_set_cancel_object,
        "CoGetCallContext" => fn co_get_call_context,
        "CoGetApartmentType" => fn co_get_apartment_type,
        "CoGetCurrentProcess" => fn co_get_current_process,
        "CoFileTimeNow" => fn co_file_time_now,
        "CoTreatAsClass" => fn co_treat_as_class,
        "FmtIdToPropStgName" => fn fmt_id_to_prop_stg_name,
        "PropStgNameToFmtId" => fn prop_stg_name_to_fmt_id,
        "RoGetAgileReference" => fn ro_get_agile_reference,
        "RoGetApartmentIdentifier" => fn ro_get_apartment_identifier,
        "RoGetParameterizedTypeInstanceIID" => fn ro_get_parameterized_type_instance_iid,
        "RoOriginateLanguageException" => fn ro_originate_language_exception,
        "RoRegisterActivationFactories" => fn ro_register_activation_factories,
        "RoRevokeActivationFactories" => fn ro_revoke_activation_factories,
        "RoRegisterForApartmentShutdown" => fn ro_register_for_apartment_shutdown,
        "RoUnregisterForApartmentShutdown" => fn ro_unregister_for_apartment_shutdown,
        "SetRestrictedErrorInfo" => fn set_restricted_error_info,
    }
}

#[cfg(test)]
#[path = "ole32_lot_c/tests.rs"]
mod tests;
