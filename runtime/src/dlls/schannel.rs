//! Schannel : le fournisseur TLS de SSPI (secur32.dll), sur l'OpenSSL de l'hôte.
//!
//! L'application garde la main sur le réseau, comme sous Windows : elle passe
//! les octets reçus à InitializeSecurityContext / DecryptMessage et envoie
//! ce que Schannel lui rend. Ici, ces octets transitent par deux « BIO »
//! mémoire d'OpenSSL. Le découpage en enregistrements TLS (5 octets d'en-tête
//! puis la longueur) est fait par Winlux pour rendre exactement les codes de
//! Windows : SEC_E_INCOMPLETE_MESSAGE, SECBUFFER_EXTRA, SECBUFFER_MISSING.
//!
//! Le certificat du serveur est vérifié automatiquement avec les autorités
//! du système, sauf si l'application demande la validation manuelle
//! (SCH_CRED_MANUAL_CRED_VALIDATION ou ISC_REQ_MANUAL_CRED_VALIDATION).

use super::hostssl::{self, Ssl, P};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

pub const SEC_E_OK: i32 = 0;
const SEC_I_CONTINUE_NEEDED: i32 = 0x0009_0312;
const SEC_I_CONTEXT_EXPIRED: i32 = 0x0009_0317;
const SEC_E_INSUFFICIENT_MEMORY: i32 = 0x8009_0300u32 as i32;
pub const SEC_E_INVALID_HANDLE: i32 = 0x8009_0301u32 as i32;
const SEC_E_UNSUPPORTED_FUNCTION: i32 = 0x8009_0302u32 as i32;
const SEC_E_INTERNAL_ERROR: i32 = 0x8009_0304u32 as i32;
const SEC_E_INVALID_TOKEN: i32 = 0x8009_0308u32 as i32;
const SEC_E_NO_CREDENTIALS: i32 = 0x8009_030Eu32 as i32;
const SEC_E_INCOMPLETE_MESSAGE: i32 = 0x8009_0318u32 as i32;
const SEC_E_BUFFER_TOO_SMALL: i32 = 0x8009_0321u32 as i32;
const SEC_E_WRONG_PRINCIPAL: i32 = 0x8009_0322u32 as i32;
const SEC_E_UNTRUSTED_ROOT: i32 = 0x8009_0325u32 as i32;
const SEC_E_ILLEGAL_MESSAGE: i32 = 0x8009_0326u32 as i32;
const SEC_E_CERT_UNKNOWN: i32 = 0x8009_0327u32 as i32;
const SEC_E_CERT_EXPIRED: i32 = 0x8009_0328u32 as i32;
const SEC_E_DECRYPT_FAILURE: i32 = 0x8009_0330u32 as i32;

const SECBUFFER_EMPTY: u32 = 0;
const SECBUFFER_DATA: u32 = 1;
const SECBUFFER_TOKEN: u32 = 2;
const SECBUFFER_MISSING: u32 = 4;
const SECBUFFER_EXTRA: u32 = 5;
const SECBUFFER_STREAM_TRAILER: u32 = 6;
const SECBUFFER_STREAM_HEADER: u32 = 7;
const SECBUFFER_APPLICATION_PROTOCOLS: u32 = 18;

const ISC_REQ_ALLOCATE_MEMORY: u32 = 0x100;
const ISC_REQ_MANUAL_CRED_VALIDATION: u32 = 0x0008_0000;
const SCH_CRED_NO_SERVERNAME_CHECK: u32 = 0x4;
const SCH_CRED_MANUAL_CRED_VALIDATION: u32 = 0x8;

/// Taille maximale d'un enregistrement TLS (texte clair) et marges annoncées.
const MAX_MESSAGE: u32 = 16384;
const HEADER: u32 = 5;
/// Assez pour GCM, ChaCha20, TLS 1.3 et les suites CBC avec SHA-384.
const TRAILER: u32 = 128;
/// Valeur de dwUpper des poignées Schannel.
const MAGIC: usize = 0x5743_484E; // « WCHN »

#[repr(C)]
pub struct SecHandle {
    pub lower: usize,
    pub upper: usize,
}

#[repr(C)]
pub struct SecBuffer {
    pub cb: u32,
    pub ty: u32,
    pub pv: *mut u8,
}

#[repr(C)]
pub struct SecBufferDesc {
    pub version: u32,
    pub count: u32,
    pub buffers: *mut SecBuffer,
}

struct Cred {
    flags: u32,
}

struct Ctx {
    ssl: usize,
    rbio: usize,
    wbio: usize,
    host: String,
    req: u32,
    cred_flags: u32,
    done: bool,
    shutdown: bool,
}

static NEXT: AtomicUsize = AtomicUsize::new(1);

fn creds() -> &'static Mutex<HashMap<usize, Cred>> {
    static C: OnceLock<Mutex<HashMap<usize, Cred>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ctxs() -> &'static Mutex<HashMap<usize, Ctx>> {
    static C: OnceLock<Mutex<HashMap<usize, Ctx>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Identifiant Schannel d'une poignée, s'il s'agit bien d'une des nôtres.
unsafe fn id_of(h: *const SecHandle) -> Option<usize> {
    (!h.is_null() && (*h).upper == MAGIC).then(|| (*h).lower)
}

pub unsafe fn is_cred(h: *const SecHandle) -> bool {
    id_of(h).is_some_and(|id| super::lock(creds()).contains_key(&id))
}

pub unsafe fn is_ctx(h: *const SecHandle) -> bool {
    id_of(h).is_some_and(|id| super::lock(ctxs()).contains_key(&id))
}

/// Noms acceptés pour le paquet (UNISP_NAME, SCHANNEL_NAME, « Default TLS SSP »).
pub fn is_package(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    matches!(
        n.as_str(),
        "schannel"
            | "microsoft unified security protocol provider"
            | "default tls ssp"
            | "microsoft tls 1.0"
    )
}

unsafe fn buffers<'a>(d: *mut SecBufferDesc) -> &'a mut [SecBuffer] {
    if d.is_null() || (*d).buffers.is_null() {
        return &mut [];
    }
    std::slice::from_raw_parts_mut((*d).buffers, (*d).count as usize)
}

fn kind(b: &SecBuffer) -> u32 {
    b.ty & 0x0FFF_FFFF
}

fn ctx_global(s: &Ssl) -> P {
    static CTX: OnceLock<usize> = OnceLock::new();
    *CTX.get_or_init(|| unsafe {
        let c = (s.SSL_CTX_new)((s.TLS_client_method)());
        if !c.is_null() {
            // La vérification est faite par Winlux après la poignée de main.
            (s.SSL_CTX_set_verify)(c, 0, std::ptr::null_mut());
        }
        c as usize
    }) as P
}

/// Tout ce qu'OpenSSL a écrit vers le réseau.
unsafe fn drain(s: &Ssl, bio: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = (s.BIO_read)(bio as P, buf.as_mut_ptr().cast(), buf.len() as i32);
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    out
}

unsafe fn pending(s: &Ssl, bio: usize) -> usize {
    // BIO_CTRL_PENDING
    (s.BIO_ctrl)(bio as P, 10, 0, std::ptr::null_mut()).max(0) as usize
}

/// Longueur des enregistrements TLS complets en tête de `data`.
fn complete_records(data: &[u8]) -> usize {
    let mut off = 0;
    while off + 5 <= data.len() {
        let len = 5 + u16::from_be_bytes([data[off + 3], data[off + 4]]) as usize;
        if off + len > data.len() {
            break;
        }
        off += len;
    }
    off
}

// ---------- Identifiants ----------

pub unsafe fn acquire_credentials(
    use_: u32,
    auth: *const u8,
    cred: *mut SecHandle,
    expiry: *mut i64,
) -> i32 {
    if cred.is_null() {
        return SEC_E_INTERNAL_ERROR;
    }
    // SECPKG_CRED_OUTBOUND = 2 : le rôle de serveur (certificat local) n'est pas fourni.
    if use_ & 2 == 0 {
        crate::log_warn!("schannel", "rôle serveur TLS demandé : non fourni");
        return SEC_E_NO_CREDENTIALS;
    }
    if hostssl::ssl().is_none() {
        return SEC_E_INTERNAL_ERROR;
    }
    // SCHANNEL_CRED (version 4) : dwFlags à +72 ; SCH_CREDENTIALS (version 5) : à +52.
    let flags = if auth.is_null() {
        0
    } else {
        match (auth as *const u32).read_unaligned() {
            4 => (auth.add(72) as *const u32).read_unaligned(),
            5 => (auth.add(52) as *const u32).read_unaligned(),
            _ => 0,
        }
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    super::lock(creds()).insert(id, Cred { flags });
    (*cred).lower = id;
    (*cred).upper = MAGIC;
    if !expiry.is_null() {
        *expiry = i64::MAX;
    }
    SEC_E_OK
}

pub unsafe fn free_credentials(cred: *const SecHandle) -> i32 {
    match id_of(cred).and_then(|id| super::lock(creds()).remove(&id)) {
        Some(_) => SEC_E_OK,
        None => SEC_E_INVALID_HANDLE,
    }
}

// ---------- Poignée de main ----------

/// Rend `bytes` dans le tampon jeton de `output` (alloué si demandé).
unsafe fn put_token(output: *mut SecBufferDesc, bytes: &[u8], req: u32) -> Result<bool, i32> {
    let bufs = buffers(output);
    let Some(t) = bufs.iter_mut().find(|b| kind(b) == SECBUFFER_TOKEN) else {
        return if bytes.is_empty() {
            Ok(false)
        } else {
            Err(SEC_E_BUFFER_TOO_SMALL)
        };
    };
    if req & ISC_REQ_ALLOCATE_MEMORY != 0 {
        if bytes.is_empty() {
            t.pv = std::ptr::null_mut();
            t.cb = 0;
            return Ok(false);
        }
        let p = libc::malloc(bytes.len()) as *mut u8;
        if p.is_null() {
            return Err(SEC_E_INSUFFICIENT_MEMORY);
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        t.pv = p;
        t.cb = bytes.len() as u32;
        return Ok(true);
    }
    if (t.cb as usize) < bytes.len() || (t.pv.is_null() && !bytes.is_empty()) {
        return Err(SEC_E_BUFFER_TOO_SMALL);
    }
    if !bytes.is_empty() {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), t.pv, bytes.len());
    }
    t.cb = bytes.len() as u32;
    Ok(false)
}

/// Chaîne présentée par le serveur (certificat final en tête), en DER.
unsafe fn peer_chain(s: &Ssl, ssl: P) -> Vec<Vec<u8>> {
    let sk = (s.SSL_get_peer_cert_chain)(ssl);
    let mut v = Vec::new();
    if !sk.is_null() {
        for i in 0..(s.OPENSSL_sk_num)(sk) {
            if let Some(d) = hostssl::der_of((s.OPENSSL_sk_value)(sk, i)) {
                v.push(d);
            }
        }
    }
    if v.is_empty() {
        let x = (s.SSL_get1_peer_certificate)(ssl);
        if !x.is_null() {
            v.extend(hostssl::der_of(x));
            (s.X509_free)(x);
        }
    }
    v
}

/// Validation automatique du certificat du serveur, comme Schannel.
unsafe fn validate(s: &Ssl, c: &Ctx) -> i32 {
    let chain = peer_chain(s, c.ssl as P);
    let Some(leaf) = chain.first() else {
        return SEC_E_CERT_UNKNOWN;
    };
    let Some(v) = hostssl::verify(leaf, &chain[1..], None) else {
        return SEC_E_INTERNAL_ERROR;
    };
    match v.error {
        0 => {}
        9 | 10 => return SEC_E_CERT_EXPIRED,
        23 => return SEC_E_CERT_UNKNOWN,
        _ => return SEC_E_UNTRUSTED_ROOT,
    }
    if c.cred_flags & SCH_CRED_NO_SERVERNAME_CHECK == 0
        && !c.host.is_empty()
        && !hostssl::check_host(leaf, &c.host)
    {
        return SEC_E_WRONG_PRINCIPAL;
    }
    SEC_E_OK
}

/// Liste ALPN (format filaire) d'un tampon SECBUFFER_APPLICATION_PROTOCOLS.
unsafe fn alpn_of(input: *mut SecBufferDesc) -> Option<Vec<u8>> {
    let b = buffers(input)
        .iter()
        .find(|b| kind(b) == SECBUFFER_APPLICATION_PROTOCOLS && !b.pv.is_null())?;
    let raw = std::slice::from_raw_parts(b.pv, b.cb as usize);
    // SEC_APPLICATION_PROTOCOLS { u32 taille ; listes { u32 extension ; u16 taille ; octets } }
    let total = u32::from_le_bytes(raw.get(..4)?.try_into().ok()?) as usize;
    let mut rest = raw.get(4..4 + total)?;
    while rest.len() >= 6 {
        let ext = u32::from_le_bytes(rest[..4].try_into().ok()?);
        let n = u16::from_le_bytes([rest[4], rest[5]]) as usize;
        let list = rest.get(6..6 + n)?;
        if ext == 2 {
            return Some(list.to_vec());
        }
        rest = &rest[6 + n..];
    }
    None
}

fn is_ip(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn initialize(
    cred: *const SecHandle,
    ctx: *const SecHandle,
    target: String,
    req: u32,
    input: *mut SecBufferDesc,
    new_ctx: *mut SecHandle,
    output: *mut SecBufferDesc,
    attrs: *mut u32,
) -> i32 {
    let Some(s) = hostssl::ssl() else {
        return SEC_E_INTERNAL_ERROR;
    };
    let mut map = super::lock(ctxs());
    let existing = id_of(ctx).filter(|id| map.contains_key(id));
    let id = match existing {
        Some(id) => id,
        None => {
            let Some(cflags) =
                id_of(cred).and_then(|id| super::lock(creds()).get(&id).map(|c| c.flags))
            else {
                return SEC_E_INVALID_HANDLE;
            };
            let ssl = (s.SSL_new)(ctx_global(s));
            if ssl.is_null() {
                return SEC_E_INTERNAL_ERROR;
            }
            let rbio = (s.BIO_new)((s.BIO_s_mem)());
            let wbio = (s.BIO_new)((s.BIO_s_mem)());
            (s.SSL_set_bio)(ssl, rbio, wbio);
            let host = target.trim().to_string();
            if !host.is_empty() && !is_ip(&host) {
                let c = std::ffi::CString::new(host.clone()).unwrap_or_default();
                // SSL_CTRL_SET_TLSEXT_HOSTNAME, TLSEXT_NAMETYPE_host_name
                (s.SSL_ctrl)(ssl, 55, 0, c.as_ptr() as P);
            }
            if let Some(alpn) = alpn_of(input) {
                (s.SSL_set_alpn_protos)(ssl, alpn.as_ptr(), alpn.len() as u32);
            }
            (s.SSL_set_connect_state)(ssl);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            map.insert(
                id,
                Ctx {
                    ssl: ssl as usize,
                    rbio: rbio as usize,
                    wbio: wbio as usize,
                    host,
                    req,
                    cred_flags: cflags,
                    done: false,
                    shutdown: false,
                },
            );
            id
        }
    };
    if !new_ctx.is_null() {
        (*new_ctx).lower = id;
        (*new_ctx).upper = MAGIC;
    }
    let c = map.get_mut(&id).unwrap();
    let ssl = c.ssl as P;
    if !attrs.is_null() {
        *attrs = req & 0x0001_FEFF;
    }

    // Fermeture demandée par ApplyControlToken : l'alerte close_notify.
    if c.shutdown {
        (s.SSL_shutdown)(ssl);
        let out = drain(s, c.wbio);
        return match put_token(output, &out, req) {
            Ok(a) => {
                if a && !attrs.is_null() {
                    *attrs |= ISC_REQ_ALLOCATE_MEMORY;
                }
                SEC_E_OK
            }
            Err(e) => e,
        };
    }
    if c.done {
        return SEC_E_OK;
    }

    // Octets reçus du serveur : seuls les enregistrements complets sont passés.
    let mut leftover = 0usize;
    if existing.is_some() {
        let inb = buffers(input);
        let Some(tok) = inb.iter().position(|b| kind(b) == SECBUFFER_TOKEN) else {
            return SEC_E_INVALID_TOKEN;
        };
        let data: &[u8] = if inb[tok].pv.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(inb[tok].pv, inb[tok].cb as usize)
        };
        let complete = complete_records(data);
        if complete == 0 {
            if let Some(m) = inb.get_mut(tok + 1) {
                let need = if data.len() >= 5 {
                    5 + u16::from_be_bytes([data[3], data[4]]) as usize - data.len()
                } else {
                    5 - data.len()
                };
                m.ty = SECBUFFER_MISSING;
                m.cb = need as u32;
            }
            return SEC_E_INCOMPLETE_MESSAGE;
        }
        (s.BIO_write)(c.rbio as P, data.as_ptr().cast(), complete as i32);
        leftover = data.len() - complete;
    }

    let r = (s.SSL_do_handshake)(ssl);
    let err = if r == 1 { 0 } else { (s.SSL_get_error)(ssl, r) };
    let out = drain(s, c.wbio);
    if r != 1 && err != 2 {
        // Erreur fatale : l'alerte éventuelle n'est pas transmise, comme Windows.
        crate::log_warn!(
            "schannel",
            "poignée de main TLS refusée ({}) avec {}",
            err,
            c.host
        );
        let _ = put_token(output, &[], req);
        return SEC_E_ILLEGAL_MESSAGE;
    }
    // Ce qu'OpenSSL n'a pas lu (données après la fin de la poignée de main)
    // et l'enregistrement incomplet en queue reviennent à l'appelant.
    let unread = pending(s, c.rbio);
    if unread > 0 {
        drain(s, c.rbio);
    }
    let extra = unread + leftover;
    if existing.is_some() {
        let inb = buffers(input);
        if let Some(tok) = inb.iter().position(|b| kind(b) == SECBUFFER_TOKEN) {
            if let Some(e) = inb.get_mut(tok + 1) {
                if extra > 0 {
                    e.ty = SECBUFFER_EXTRA;
                    e.cb = extra as u32;
                } else if kind(e) == SECBUFFER_EXTRA {
                    e.ty = SECBUFFER_EMPTY;
                    e.cb = 0;
                }
            }
        }
    }
    let allocated = match put_token(output, &out, req) {
        Ok(a) => a,
        Err(e) => return e,
    };
    if allocated && !attrs.is_null() {
        *attrs |= ISC_REQ_ALLOCATE_MEMORY;
    }
    if r != 1 {
        return SEC_I_CONTINUE_NEEDED;
    }
    c.done = true;
    let manual = c.req & ISC_REQ_MANUAL_CRED_VALIDATION != 0
        || req & ISC_REQ_MANUAL_CRED_VALIDATION != 0
        || c.cred_flags & SCH_CRED_MANUAL_CRED_VALIDATION != 0;
    if !manual {
        let v = validate(s, c);
        if v != SEC_E_OK {
            crate::log_warn!(
                "schannel",
                "certificat de {} refusé : {:#x}",
                c.host,
                v as u32
            );
            return v;
        }
    }
    SEC_E_OK
}

pub unsafe fn delete(ctx: *const SecHandle) -> i32 {
    let Some(c) = id_of(ctx).and_then(|id| super::lock(ctxs()).remove(&id)) else {
        return SEC_E_INVALID_HANDLE;
    };
    if let Some(s) = hostssl::ssl() {
        (s.SSL_free)(c.ssl as P);
    }
    SEC_E_OK
}

pub unsafe fn apply_control_token(ctx: *const SecHandle, input: *mut SecBufferDesc) -> i32 {
    let mut map = super::lock(ctxs());
    let Some(c) = id_of(ctx).and_then(|id| map.get_mut(&id)) else {
        return SEC_E_INVALID_HANDLE;
    };
    let token = buffers(input)
        .iter()
        .find(|b| kind(b) == SECBUFFER_TOKEN && b.cb >= 4 && !b.pv.is_null());
    // SCHANNEL_SHUTDOWN = 1.
    match token.map(|b| (b.pv as *const u32).read_unaligned()) {
        Some(1) => {
            c.shutdown = true;
            SEC_E_OK
        }
        _ => SEC_E_UNSUPPORTED_FUNCTION,
    }
}

// ---------- Chiffrement des données ----------

pub unsafe fn encrypt(ctx: *const SecHandle, msg: *mut SecBufferDesc) -> i32 {
    let Some(s) = hostssl::ssl() else {
        return SEC_E_INTERNAL_ERROR;
    };
    let map = super::lock(ctxs());
    let Some(c) = id_of(ctx).and_then(|id| map.get(&id)) else {
        return SEC_E_INVALID_HANDLE;
    };
    let bufs = buffers(msg);
    let find = |t: u32| bufs.iter().position(|b| kind(b) == t);
    let (Some(h), Some(d), Some(t)) = (
        find(SECBUFFER_STREAM_HEADER),
        find(SECBUFFER_DATA),
        find(SECBUFFER_STREAM_TRAILER),
    ) else {
        return SEC_E_INVALID_TOKEN;
    };
    let plain = bufs[d].cb as usize;
    if plain > 0 {
        let w = (s.SSL_write)(c.ssl as P, bufs[d].pv as *const _, plain as i32);
        if w <= 0 {
            return SEC_E_INTERNAL_ERROR;
        }
    }
    let rec = drain(s, c.wbio);
    // Découpage : en-tête, puis à la place du texte clair, le reste en fin.
    let hn = (bufs[h].cb as usize).min(HEADER as usize).min(rec.len());
    if rec.len() < hn + plain || rec.len() - hn - plain > bufs[t].cb as usize {
        return SEC_E_BUFFER_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(rec.as_ptr(), bufs[h].pv, hn);
    bufs[h].cb = hn as u32;
    if plain > 0 {
        std::ptr::copy_nonoverlapping(rec[hn..].as_ptr(), bufs[d].pv, plain);
    }
    let tn = rec.len() - hn - plain;
    if tn > 0 {
        std::ptr::copy_nonoverlapping(rec[hn + plain..].as_ptr(), bufs[t].pv, tn);
    }
    bufs[t].cb = tn as u32;
    SEC_E_OK
}

pub unsafe fn decrypt(ctx: *const SecHandle, msg: *mut SecBufferDesc) -> i32 {
    let Some(s) = hostssl::ssl() else {
        return SEC_E_INTERNAL_ERROR;
    };
    let map = super::lock(ctxs());
    let Some(c) = id_of(ctx).and_then(|id| map.get(&id)) else {
        return SEC_E_INVALID_HANDLE;
    };
    let bufs = buffers(msg);
    let Some(di) = bufs.iter().position(|b| kind(b) == SECBUFFER_DATA) else {
        return SEC_E_INVALID_TOKEN;
    };
    let base = bufs[di].pv;
    let total = bufs[di].cb as usize;
    let data: &[u8] = if base.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(base, total)
    };
    let rec = if data.len() >= 5 {
        5 + u16::from_be_bytes([data[3], data[4]]) as usize
    } else {
        usize::MAX
    };
    if rec > data.len() {
        let need = if rec == usize::MAX {
            5 - data.len()
        } else {
            rec - data.len()
        };
        if let Some(m) = bufs.iter_mut().find(|b| kind(b) == SECBUFFER_EMPTY) {
            m.ty = SECBUFFER_MISSING;
            m.cb = need as u32;
        }
        return SEC_E_INCOMPLETE_MESSAGE;
    }
    (s.BIO_write)(c.rbio as P, data.as_ptr().cast(), rec as i32);
    let mut plain = vec![0u8; MAX_MESSAGE as usize + 2048];
    let n = (s.SSL_read)(c.ssl as P, plain.as_mut_ptr().cast(), plain.len() as i32);
    let (status, len) = if n > 0 {
        (SEC_E_OK, n as usize)
    } else {
        match (s.SSL_get_error)(c.ssl as P, n) {
            // Message de service (ticket de session TLS 1.3…) : aucune donnée.
            2 | 3 => (SEC_E_OK, 0),
            // close_notify du serveur.
            6 => (SEC_I_CONTEXT_EXPIRED, 0),
            _ => return SEC_E_DECRYPT_FAILURE,
        }
    };
    // Une éventuelle réponse automatique (mise à jour de clé) n'est pas émise.
    drain(s, c.wbio);
    let len = len.min(rec - 5);
    // Texte clair déchiffré sur place, après l'en-tête, comme Schannel.
    std::ptr::copy_nonoverlapping(plain.as_ptr(), base.add(5), len);
    let extra = total - rec;
    let mut layout = vec![
        (SECBUFFER_STREAM_HEADER, base, 5u32),
        (SECBUFFER_DATA, base.add(5), len as u32),
        (
            SECBUFFER_STREAM_TRAILER,
            base.add(5 + len),
            (rec - 5 - len) as u32,
        ),
    ];
    if extra > 0 {
        layout.push((SECBUFFER_EXTRA, base.add(rec), extra as u32));
    }
    for (i, b) in bufs.iter_mut().enumerate() {
        match layout.get(i) {
            Some(&(ty, pv, cb)) => {
                b.ty = ty;
                b.pv = pv;
                b.cb = cb;
            }
            None => {
                b.ty = SECBUFFER_EMPTY;
                b.cb = 0;
            }
        }
    }
    status
}

// ---------- Attributs ----------

pub unsafe fn query_attributes(ctx: *const SecHandle, attr: u32, out: *mut u8) -> i32 {
    let Some(s) = hostssl::ssl() else {
        return SEC_E_INTERNAL_ERROR;
    };
    let map = super::lock(ctxs());
    let Some(c) = id_of(ctx).and_then(|id| map.get(&id)) else {
        return SEC_E_INVALID_HANDLE;
    };
    if out.is_null() {
        return SEC_E_INTERNAL_ERROR;
    }
    let ssl = c.ssl as P;
    let put = |vals: &[u32]| {
        for (i, v) in vals.iter().enumerate() {
            (out as *mut u32).add(i).write_unaligned(*v);
        }
    };
    match attr {
        // SECPKG_ATTR_SIZES
        0 => put(&[0x6000, 0, 0, TRAILER]),
        // SECPKG_ATTR_STREAM_SIZES : en-tête, fin, message, tampons, bloc.
        4 => put(&[HEADER, TRAILER, MAX_MESSAGE, 4, 16]),
        // SECPKG_ATTR_APPLICATION_PROTOCOL
        0x23 => {
            std::ptr::write_bytes(out, 0, 4 + 4 + 1 + 255);
            let mut p: *const u8 = std::ptr::null();
            let mut n = 0u32;
            (s.SSL_get0_alpn_selected)(ssl, &mut p, &mut n);
            if !p.is_null() && n > 0 {
                let n = n.min(255);
                // Success = 1, ALPN = 2.
                put(&[1, 2]);
                *out.add(8) = n as u8;
                std::ptr::copy_nonoverlapping(p, out.add(9), n as usize);
            } else {
                // SecApplicationProtocolNegotiationStatus_None
                put(&[0, 2]);
            }
        }
        // SECPKG_ATTR_REMOTE_CERT_CONTEXT : contexte dans un magasin qui
        // contient aussi les intermédiaires envoyés par le serveur.
        0x53 => {
            let chain = peer_chain(s, ssl);
            if chain.is_empty() {
                return SEC_E_NO_CREDENTIALS;
            }
            let store = super::crypt32::new_store();
            let mut leaf = 0;
            for (i, d) in chain.iter().enumerate() {
                if let Some(ctx) = super::crypt32::store_add_der(store, d) {
                    if i == 0 {
                        leaf = super::crypt32::CertDuplicateCertificateContext(ctx);
                    }
                }
            }
            // Le magasin reste ouvert : sous Windows il vit tant que le
            // contexte rendu le désigne (hCertStore), et libcurl y cherche
            // les intermédiaires pour CertGetCertificateChain.
            (out as *mut usize).write_unaligned(leaf);
        }
        // SECPKG_ATTR_CONNECTION_INFO
        0x5a => {
            let proto = match (s.SSL_version)(ssl) {
                0x0301 => 0x80,
                0x0302 => 0x200,
                0x0303 => 0x800,
                0x0304 => 0x2000,
                _ => 0,
            };
            let cipher = (s.SSL_get_current_cipher)(ssl);
            let bits = if cipher.is_null() {
                0
            } else {
                (s.SSL_CIPHER_get_bits)(cipher, std::ptr::null_mut()).max(0) as u32
            };
            // CALG_AES_128 / CALG_AES_256, CALG_SHA_256, CALG_ECDH_EPHEM.
            let alg = if bits >= 256 { 0x6610 } else { 0x660e };
            put(&[proto, alg, bits, 0x800c, 256, 0xae06, 256]);
        }
        _ => return SEC_E_UNSUPPORTED_FUNCTION,
    }
    SEC_E_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn desc(bufs: &mut [SecBuffer]) -> SecBufferDesc {
        SecBufferDesc {
            version: 0,
            count: bufs.len() as u32,
            buffers: bufs.as_mut_ptr(),
        }
    }

    fn sb(ty: u32, pv: *mut u8, cb: u32) -> SecBuffer {
        SecBuffer { cb, ty, pv }
    }

    /// Boucle de poignée de main type « curl » ; renvoie le contexte et le reliquat.
    unsafe fn handshake(
        sock: &mut TcpStream,
        manual: bool,
        host: &str,
    ) -> Result<(SecHandle, Vec<u8>), i32> {
        let mut auth = [0u8; 80];
        auth[..4].copy_from_slice(&4u32.to_le_bytes());
        if manual {
            auth[72..76].copy_from_slice(&SCH_CRED_MANUAL_CRED_VALIDATION.to_le_bytes());
        }
        let mut cred = SecHandle { lower: 0, upper: 0 };
        assert_eq!(
            acquire_credentials(2, auth.as_ptr(), &mut cred, std::ptr::null_mut()),
            SEC_E_OK
        );
        let mut ctx = SecHandle { lower: 0, upper: 0 };
        let mut inbuf: Vec<u8> = Vec::new();
        let mut first = true;
        let mut attrs = 0;
        loop {
            let mut out = [sb(SECBUFFER_TOKEN, std::ptr::null_mut(), 0)];
            let mut od = desc(&mut out);
            let mut inb = [
                sb(SECBUFFER_TOKEN, inbuf.as_mut_ptr(), inbuf.len() as u32),
                sb(SECBUFFER_EMPTY, std::ptr::null_mut(), 0),
            ];
            let mut id = desc(&mut inb);
            let r = initialize(
                &cred,
                if first { std::ptr::null() } else { &ctx },
                host.into(),
                ISC_REQ_ALLOCATE_MEMORY | 0x8000,
                if first { std::ptr::null_mut() } else { &mut id },
                &mut ctx,
                &mut od,
                &mut attrs,
            );
            first = false;
            if out[0].cb > 0 {
                sock.write_all(std::slice::from_raw_parts(out[0].pv, out[0].cb as usize))
                    .unwrap();
                libc::free(out[0].pv.cast());
            }
            let extra = if inb[1].ty == SECBUFFER_EXTRA {
                inb[1].cb as usize
            } else {
                0
            };
            match r {
                SEC_E_OK => {
                    let rest = inbuf[inbuf.len() - extra..].to_vec();
                    free_credentials(&cred);
                    return Ok((ctx, rest));
                }
                SEC_I_CONTINUE_NEEDED | SEC_E_INCOMPLETE_MESSAGE => {
                    if r == SEC_I_CONTINUE_NEEDED {
                        inbuf.drain(..inbuf.len() - extra);
                    }
                    if r == SEC_E_INCOMPLETE_MESSAGE || extra == 0 {
                        let mut b = [0u8; 16384];
                        let n = sock.read(&mut b).unwrap();
                        assert!(n > 0, "connexion fermée pendant la poignée de main");
                        inbuf.extend_from_slice(&b[..n]);
                    }
                }
                e => {
                    free_credentials(&cred);
                    delete(&ctx);
                    return Err(e);
                }
            }
        }
    }

    unsafe fn exchange(sock: &mut TcpStream, ctx: &SecHandle, mut pending: Vec<u8>) -> String {
        let mut sizes = [0u32; 5];
        assert_eq!(
            query_attributes(ctx, 4, sizes.as_mut_ptr().cast()),
            SEC_E_OK
        );
        let req = b"GET / HTTP/1.0\r\n\r\n";
        let mut buf = vec![0u8; (sizes[0] + sizes[1]) as usize + req.len()];
        buf[sizes[0] as usize..][..req.len()].copy_from_slice(req);
        let p = buf.as_mut_ptr();
        let mut b = [
            sb(SECBUFFER_STREAM_HEADER, p, sizes[0]),
            sb(SECBUFFER_DATA, p.add(sizes[0] as usize), req.len() as u32),
            sb(
                SECBUFFER_STREAM_TRAILER,
                p.add(sizes[0] as usize + req.len()),
                sizes[1],
            ),
            sb(SECBUFFER_EMPTY, std::ptr::null_mut(), 0),
        ];
        let mut d = desc(&mut b);
        assert_eq!(encrypt(ctx, &mut d), SEC_E_OK);
        let n = (b[0].cb + b[1].cb + b[2].cb) as usize;
        sock.write_all(&buf[..n]).unwrap();
        let mut text = Vec::new();
        loop {
            if pending.is_empty() {
                let mut r = [0u8; 16384];
                let n = sock.read(&mut r).unwrap_or(0);
                if n == 0 {
                    break;
                }
                pending.extend_from_slice(&r[..n]);
            }
            let mut b = [
                sb(SECBUFFER_DATA, pending.as_mut_ptr(), pending.len() as u32),
                sb(SECBUFFER_EMPTY, std::ptr::null_mut(), 0),
                sb(SECBUFFER_EMPTY, std::ptr::null_mut(), 0),
                sb(SECBUFFER_EMPTY, std::ptr::null_mut(), 0),
            ];
            let mut d = desc(&mut b);
            let r = decrypt(ctx, &mut d);
            if r == SEC_E_INCOMPLETE_MESSAGE {
                let mut r = [0u8; 16384];
                let n = sock.read(&mut r).unwrap_or(0);
                if n == 0 {
                    break;
                }
                pending.extend_from_slice(&r[..n]);
                continue;
            }
            assert!(
                r == SEC_E_OK || r == SEC_I_CONTEXT_EXPIRED,
                "DecryptMessage : {r:#x}"
            );
            text.extend_from_slice(std::slice::from_raw_parts(b[1].pv, b[1].cb as usize));
            let extra = b
                .iter()
                .find(|x| x.ty == SECBUFFER_EXTRA)
                .map(|x| x.cb as usize)
                .unwrap_or(0);
            pending = pending[pending.len() - extra..].to_vec();
            if r == SEC_I_CONTEXT_EXPIRED {
                break;
            }
        }
        String::from_utf8_lossy(&text).into_owned()
    }

    /// Serveur `openssl s_server -www` local avec un certificat auto-signé.
    fn server() -> Option<(std::process::Child, u16, std::path::PathBuf)> {
        hostssl::ssl()?;
        let dir = std::env::temp_dir().join(format!("winlux-schannel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        let key = dir.join("k.pem");
        let crt = dir.join("c.pem");
        let ok = std::process::Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-subj",
                "/CN=localhost",
                "-days",
                "1",
                "-addext",
                "subjectAltName=DNS:localhost",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&crt)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?
            .success();
        if !ok {
            return None;
        }
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .ok()?
            .local_addr()
            .ok()?
            .port();
        let child = std::process::Command::new("openssl")
            .args(["s_server", "-quiet", "-www", "-accept"])
            .arg(port.to_string())
            .arg("-cert")
            .arg(&crt)
            .arg("-key")
            .arg(&key)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Some((child, port, dir));
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        None
    }

    #[test]
    fn decoupage_des_enregistrements() {
        let mut v = vec![0x16, 3, 3, 0, 2, 1, 2];
        assert_eq!(complete_records(&v), 7);
        v.extend_from_slice(&[0x17, 3, 3, 0, 10, 1]);
        assert_eq!(complete_records(&v), 7);
        assert!(is_package("Microsoft Unified Security Protocol Provider"));
        assert!(!is_package("Kerberos"));
    }

    #[test]
    fn tls_contre_un_serveur_local() {
        let Some((mut child, port, dir)) = server() else {
            eprintln!("openssl absent : test TLS sauté");
            return;
        };
        unsafe {
            // Certificat auto-signé : refusé par la validation automatique…
            let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let r = handshake(&mut sock, false, "localhost").err();
            assert_eq!(r, Some(SEC_E_UNTRUSTED_ROOT));
            drop(sock);
            // …accepté en validation manuelle, puis échange HTTP chiffré.
            let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let (ctx, rest) = handshake(&mut sock, true, "localhost").expect("poignée de main");
            let mut info = [0u32; 7];
            assert_eq!(
                query_attributes(&ctx, 0x5a, info.as_mut_ptr().cast()),
                SEC_E_OK
            );
            assert!(
                info[0] == 0x800 || info[0] == 0x2000,
                "protocole {:#x}",
                info[0]
            );
            let mut cert = 0usize;
            assert_eq!(
                query_attributes(&ctx, 0x53, (&mut cert as *mut usize).cast()),
                SEC_E_OK
            );
            let mut name = [0u16; 64];
            let n = super::super::crypt32::CertGetNameStringW(cert, 4, 0, 0, name.as_mut_ptr(), 64);
            assert_eq!(
                String::from_utf16_lossy(&name[..n as usize - 1]),
                "localhost"
            );
            super::super::crypt32::CertFreeCertificateContext(cert);
            let page = exchange(&mut sock, &ctx, rest);
            assert!(page.starts_with("HTTP/1.0 200"), "{page}");
            delete(&ctx);
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(dir);
    }
}
