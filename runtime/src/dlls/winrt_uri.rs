//! Classe Windows Runtime `Windows.Foundation.Uri` (#2645).
//!
//! Teams active cette classe par sa fabrique `IUriRuntimeClassFactory`.
//! Quand elle est inconnue, C++/WinRT lève `hresult_class_not_registered`,
//! et Teams meurt sans fenêtre (passe teams-z).
//!
//! Servi ici :
//! - la fabrique : `CreateUri` et `CreateWithRelativeUri` ;
//! - les statiques `IUriEscapeStatics` : `EscapeComponent` et
//!   `UnescapeComponent` ;
//! - l'objet, avec `IUriRuntimeClass`,
//!   `IUriRuntimeClassWithAbsoluteCanonicalUri` et `IStringable`.
//!
//! Limites, choix de Winlux là où la documentation se tait :
//! - `QueryParsed` rend E_NOTIMPL : la classe `WwwFormUrlDecoder` n'existe
//!   pas encore ;
//! - `DisplayIri` ne décode pas les caractères non ASCII : elle rend la même
//!   chose que `DisplayUri` ;
//! - `Port` rend 0 pour un schéma sans port par défaut connu ;
//! - `Domain` garde les deux derniers libellés de l'hôte, sans liste des
//!   suffixes publics.
//!
//! Sources :
//! - la documentation publique de Microsoft (`Windows.Foundation.Uri`) ;
//! - les RFC 3986 (découpage, résolution d'une référence relative,
//!   §5.2, et retrait des segments « . » et « .. ») ;
//! - pour les identifiants d'interface et l'ordre des méthodes,
//!   windows-rs 0.58, `Windows/Foundation` (licence MIT).

use std::sync::atomic::{AtomicU32, Ordering};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
/// IActivationFactory {00000035-0000-0000-C000-000000000046}.
const IID_ACTIVATION_FACTORY: Guid = g(0x0000_0035, 0x0000, 0x0000, 0xC000_0000_0000_0046);
const IID_URI: Guid = g(0x9E36_5E57, 0x48B2, 0x4160, 0x956F_C738_5120_BBFC);
const IID_URI_FACTORY: Guid = g(0x44A9_796F, 0x723E, 0x4FDF, 0xA218_033E_75B0_C084);
const IID_URI_CANONICAL: Guid = g(0x758D_9661, 0x221C, 0x480F, 0xA339_5065_6673_F46F);
const IID_URI_ESCAPE: Guid = g(0xC1D4_32BA, 0xC824, 0x4452, 0xA7FD_512B_C3BB_E9A1);
const IID_STRINGABLE: Guid = g(0x9636_9F54, 0x8EB6, 0x48F0, 0xABCE_C1B2_11E6_27C3);

const CLASS: &str = "Windows.Foundation.Uri";

// ============================================================
//  Découpage et recomposition (RFC 3986)
// ============================================================

#[derive(Clone, Debug, PartialEq, Default)]
struct Parts {
    raw: String,
    scheme: String,
    /// Some : l'URI a une autorité (« // »).
    authority: Option<Authority>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
struct Authority {
    user: String,
    password: Option<String>,
    host: String,
    port: Option<u16>,
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

/// Découpe une URI absolue. None si la chaîne n'a pas de schéma valide :
/// une URI relative est refusée.
fn parse(raw: &str) -> Option<Parts> {
    let s = raw.trim_matches(|c: char| c.is_ascii_whitespace());
    let colon = s.find(':')?;
    let scheme = &s[..colon];
    let mut chars = scheme.chars();
    if !chars.next()?.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return None;
    }
    let mut rest = &s[colon + 1..];
    let mut authority = None;
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        authority = Some(parse_authority(&after[..end])?);
        rest = &after[end..];
    }
    let (rest, fragment) = match rest.split_once('#') {
        Some((a, f)) => (a, Some(f.to_string())),
        None => (rest, None),
    };
    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p, Some(q.to_string())),
        None => (rest, None),
    };
    let mut path = path.to_string();
    if authority.is_some() {
        path = if path.is_empty() {
            "/".to_string()
        } else {
            remove_dot_segments(&path)
        };
    }
    Some(Parts {
        raw: raw.to_string(),
        scheme: scheme.to_ascii_lowercase(),
        authority,
        path,
        query,
        fragment,
    })
}

fn parse_authority(a: &str) -> Option<Authority> {
    let (userinfo, hostport) = match a.rfind('@') {
        Some(i) => (Some(&a[..i]), &a[i + 1..]),
        None => (None, a),
    };
    let (user, password) = match userinfo {
        Some(u) => match u.split_once(':') {
            Some((n, p)) => (n.to_string(), Some(p.to_string())),
            None => (u.to_string(), None),
        },
        None => (String::new(), None),
    };
    let (host, port) = if hostport.starts_with('[') {
        let close = hostport.find(']')?;
        let after = &hostport[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(p) => parse_port(p)?,
            None if after.is_empty() => None,
            None => return None,
        };
        (hostport[..=close].to_string(), port)
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), parse_port(p)?),
            None => (hostport.to_string(), None),
        }
    };
    Some(Authority {
        user,
        password,
        host: host.to_ascii_lowercase(),
        port,
    })
}

/// Port écrit dans l'URI : vide → aucun ; non numérique ou trop grand →
/// URI refusée.
fn parse_port(p: &str) -> Option<Option<u16>> {
    if p.is_empty() {
        return Some(None);
    }
    if !p.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    p.parse::<u16>().ok().map(Some)
}

/// RFC 3986, §5.2.4.
fn remove_dot_segments(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let segs: Vec<&str> = path.split('/').collect();
    for (i, seg) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        match *seg {
            "." => {
                if last {
                    out.push("");
                }
            }
            ".." => {
                if out.len() > 1 {
                    out.pop();
                }
                if last {
                    out.push("");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if path.starts_with('/') && !joined.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

impl Parts {
    fn compose(&self, with_userinfo: bool) -> String {
        let mut s = format!("{}:", self.scheme);
        if let Some(a) = &self.authority {
            s.push_str("//");
            if with_userinfo && (!a.user.is_empty() || a.password.is_some()) {
                s.push_str(&a.user);
                if let Some(p) = &a.password {
                    s.push(':');
                    s.push_str(p);
                }
                s.push('@');
            }
            s.push_str(&a.host);
            if let Some(p) = a.port.filter(|p| Some(*p) != default_port(&self.scheme)) {
                s.push_str(&format!(":{p}"));
            }
        }
        s.push_str(&self.path);
        if let Some(q) = &self.query {
            s.push('?');
            s.push_str(q);
        }
        if let Some(f) = &self.fragment {
            s.push('#');
            s.push_str(f);
        }
        s
    }
    fn absolute(&self) -> String {
        self.compose(true)
    }
    fn display(&self) -> String {
        self.compose(false)
    }
    fn host(&self) -> &str {
        self.authority.as_ref().map_or("", |a| a.host.as_str())
    }
    fn port(&self) -> i32 {
        self.authority
            .as_ref()
            .and_then(|a| a.port)
            .or_else(|| default_port(&self.scheme))
            .map_or(0, i32::from)
    }
    /// Deux derniers libellés de l'hôte ; rien pour une adresse IP.
    fn domain(&self) -> String {
        let h = self.host();
        if h.starts_with('[') || h.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return String::new();
        }
        let labels: Vec<&str> = h.split('.').filter(|l| !l.is_empty()).collect();
        if labels.len() <= 2 {
            return labels.join(".");
        }
        labels[labels.len() - 2..].join(".")
    }
    fn extension(&self) -> String {
        let last = self.path.rsplit('/').next().unwrap_or("");
        last.rfind('.')
            .map_or(String::new(), |i| last[i..].to_string())
    }
    /// RFC 3986, §5.2.2 : résolution d'une référence sur cette base.
    fn resolve(&self, reference: &str) -> Option<Parts> {
        if let Some(abs) = parse(reference) {
            return Some(abs);
        }
        let r = reference.trim_matches(|c: char| c.is_ascii_whitespace());
        let (r, fragment) = match r.split_once('#') {
            Some((a, f)) => (a, Some(f.to_string())),
            None => (r, None),
        };
        if let Some(after) = r.strip_prefix("//") {
            return parse(&format!("{}://{after}", self.scheme)).map(|mut p| {
                p.fragment = fragment;
                p
            });
        }
        let (rpath, rquery) = match r.split_once('?') {
            Some((p, q)) => (p, Some(q.to_string())),
            None => (r, None),
        };
        let mut out = self.clone();
        out.fragment = fragment;
        if rpath.is_empty() {
            if rquery.is_some() {
                out.query = rquery;
            }
        } else {
            out.query = rquery;
            out.path = if rpath.starts_with('/') {
                remove_dot_segments(rpath)
            } else {
                let merged = match self.path.rfind('/') {
                    Some(i) => format!("{}{rpath}", &self.path[..=i]),
                    None if self.authority.is_some() => format!("/{rpath}"),
                    None => rpath.to_string(),
                };
                remove_dot_segments(&merged)
            };
        }
        out.raw = out.absolute();
        Some(out)
    }
}

/// Encodage d'un composant : seuls les caractères non réservés de la
/// RFC 3986 restent tels quels ; le reste est écrit en UTF-8 « %XX ».
fn escape_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Décodage des « %XX » ; une séquence invalide reste telle quelle.
fn unescape_component(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ============================================================
//  Chaînes HSTRING
// ============================================================

unsafe fn hstr_in(h: usize) -> String {
    String::from_utf16_lossy(super::electron_win32::hstring_slice(h))
}
unsafe fn hstr_out(s: &str, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = s.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

// ============================================================
//  Objet Uri
// ============================================================

/// Trois interfaces, trois tables : `this` pointe sur le champ de la table
/// demandée ; `base` retrouve l'objet.
#[repr(C)]
struct UriObj {
    v_uri: &'static VTable<23>,
    v_canonical: &'static VTable<8>,
    v_string: &'static VTable<7>,
    refs: AtomicU32,
    parts: Parts,
}

const OFF_CANONICAL: usize = std::mem::size_of::<usize>();
const OFF_STRING: usize = 2 * std::mem::size_of::<usize>();

fn new_uri(parts: Parts) -> usize {
    Box::into_raw(Box::new(UriObj {
        v_uri: &URI_VTBL,
        v_canonical: &CANONICAL_VTBL,
        v_string: &STRING_VTBL,
        refs: AtomicU32::new(1),
        parts,
    })) as usize
}

unsafe fn obj<'a>(base: usize) -> &'a UriObj {
    &*(base as *const UriObj)
}

unsafe fn uri_qi_base(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let p = if [IID_URI, IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT].contains(&iid) {
        base
    } else if iid == IID_URI_CANONICAL {
        base + OFF_CANONICAL
    } else if iid == IID_STRINGABLE {
        base + OFF_STRING
    } else {
        *out = 0;
        return E_NOINTERFACE;
    };
    obj(base).refs.fetch_add(1, Ordering::Relaxed);
    *out = p;
    S_OK
}
unsafe fn uri_add_ref_base(base: usize) -> u32 {
    obj(base).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe fn uri_release_base(base: usize) -> u32 {
    let n = obj(base).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut UriObj));
    }
    n
}

macro_rules! unknown_thunks {
    ($qi:ident, $add:ident, $rel:ident, $off:expr) => {
        unsafe extern "win64" fn $qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
            uri_qi_base(this - $off, iid, out)
        }
        unsafe extern "win64" fn $add(this: usize) -> u32 {
            uri_add_ref_base(this - $off)
        }
        unsafe extern "win64" fn $rel(this: usize) -> u32 {
            uri_release_base(this - $off)
        }
    };
}
unknown_thunks!(uri_qi, uri_add_ref, uri_release, 0);
unknown_thunks!(can_qi, can_add_ref, can_release, OFF_CANONICAL);
unknown_thunks!(str_qi, str_add_ref, str_release, OFF_STRING);

unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn class_name(_this: usize, out: *mut usize) -> i32 {
    hstr_out(CLASS, out)
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}

macro_rules! string_getter {
    ($name:ident, $off:expr, |$p:ident| $body:expr) => {
        unsafe extern "win64" fn $name(this: usize, out: *mut usize) -> i32 {
            let $p = &obj(this - $off).parts;
            let s: String = $body;
            hstr_out(&s, out)
        }
    };
}
string_getter!(uri_absolute, 0, |p| p.absolute());
string_getter!(uri_display, 0, |p| p.display());
string_getter!(uri_domain, 0, |p| p.domain());
string_getter!(uri_extension, 0, |p| p.extension());
string_getter!(uri_fragment, 0, |p| p
    .fragment
    .as_ref()
    .map_or(String::new(), |f| format!("#{f}")));
string_getter!(uri_host, 0, |p| p.host().to_string());
string_getter!(uri_password, 0, |p| p
    .authority
    .as_ref()
    .and_then(|a| a.password.clone())
    .unwrap_or_default());
string_getter!(uri_path, 0, |p| p.path.clone());
string_getter!(uri_query, 0, |p| p
    .query
    .as_ref()
    .map_or(String::new(), |q| format!("?{q}")));
string_getter!(uri_raw, 0, |p| p.raw.clone());
string_getter!(uri_scheme, 0, |p| p.scheme.clone());
string_getter!(uri_user, 0, |p| p
    .authority
    .as_ref()
    .map_or(String::new(), |a| a.user.clone()));
string_getter!(can_absolute, OFF_CANONICAL, |p| p.absolute());
string_getter!(can_display_iri, OFF_CANONICAL, |p| p.display());
string_getter!(str_to_string, OFF_STRING, |p| p.absolute());

/// QueryParsed : la classe WwwFormUrlDecoder n'existe pas encore
/// (winrt_uri.rs, #2645).
unsafe extern "win64" fn uri_query_parsed(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    E_NOTIMPL
}
unsafe extern "win64" fn uri_port(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = obj(this).parts.port();
    S_OK
}
unsafe extern "win64" fn uri_suspicious(_this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    S_OK
}
/// Equals : compare AbsoluteUri, lue par la table de l'autre objet.
unsafe extern "win64" fn uri_equals(this: usize, other: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if other == 0 {
        return S_OK;
    }
    let vt = *(other as *const *const usize);
    let get: unsafe extern "win64" fn(usize, *mut usize) -> i32 = std::mem::transmute(*vt.add(6));
    let mut h = 0usize;
    let hr = get(other, &mut h);
    if hr != S_OK {
        return hr;
    }
    let theirs = hstr_in(h);
    super::electron_win32::WindowsDeleteString(h);
    *out = (theirs == obj(this).parts.absolute()) as u8;
    S_OK
}
unsafe extern "win64" fn uri_combine(this: usize, relative: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match obj(this).parts.resolve(&hstr_in(relative)) {
        Some(p) => {
            *out = new_uri(p);
            S_OK
        }
        None => {
            *out = 0;
            E_INVALIDARG
        }
    }
}

static URI_VTBL: VTable<23> = VTable([
    uri_qi as *const (),
    uri_add_ref as *const (),
    uri_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    uri_absolute as *const (),
    uri_display as *const (),
    uri_domain as *const (),
    uri_extension as *const (),
    uri_fragment as *const (),
    uri_host as *const (),
    uri_password as *const (),
    uri_path as *const (),
    uri_query as *const (),
    uri_query_parsed as *const (),
    uri_raw as *const (),
    uri_scheme as *const (),
    uri_user as *const (),
    uri_port as *const (),
    uri_suspicious as *const (),
    uri_equals as *const (),
    uri_combine as *const (),
]);
static CANONICAL_VTBL: VTable<8> = VTable([
    can_qi as *const (),
    can_add_ref as *const (),
    can_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    can_absolute as *const (),
    can_display_iri as *const (),
]);
static STRING_VTBL: VTable<7> = VTable([
    str_qi as *const (),
    str_add_ref as *const (),
    str_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    str_to_string as *const (),
]);

// ============================================================
//  Fabrique (statique)
// ============================================================

#[repr(C)]
struct Factory {
    v_activation: &'static VTable<7>,
    v_factory: &'static VTable<8>,
    v_escape: &'static VTable<8>,
}

static FACTORY: Factory = Factory {
    v_activation: &ACTIVATION_VTBL,
    v_factory: &FACTORY_VTBL,
    v_escape: &ESCAPE_VTBL,
};

fn factory_base() -> usize {
    std::ptr::addr_of!(FACTORY) as usize
}

unsafe extern "win64" fn fac_qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    let base = factory_base();
    *out = if [
        IID_ACTIVATION_FACTORY,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else if iid == IID_URI_FACTORY {
        base + std::mem::size_of::<usize>()
    } else if iid == IID_URI_ESCAPE {
        base + 2 * std::mem::size_of::<usize>()
    } else {
        *out = 0;
        return E_NOINTERFACE;
    };
    S_OK
}
/// Fabrique statique : jamais libérée.
extern "win64" fn fac_add_ref(_this: usize) -> u32 {
    2
}
extern "win64" fn fac_release(_this: usize) -> u32 {
    1
}
/// ActivateInstance : Uri n'a pas de constructeur sans argument.
unsafe extern "win64" fn fac_activate(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}
/// CreateUri : une URI relative ou vide est refusée (E_INVALIDARG).
unsafe extern "win64" fn fac_create(_this: usize, uri: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match parse(&hstr_in(uri)) {
        Some(p) => {
            *out = new_uri(p);
            S_OK
        }
        None => {
            *out = 0;
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn fac_create_relative(
    _this: usize,
    base: usize,
    relative: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    match parse(&hstr_in(base)).and_then(|b| b.resolve(&hstr_in(relative))) {
        Some(p) => {
            *out = new_uri(p);
            S_OK
        }
        None => {
            *out = 0;
            E_INVALIDARG
        }
    }
}
unsafe extern "win64" fn esc_unescape(_this: usize, s: usize, out: *mut usize) -> i32 {
    hstr_out(&unescape_component(&hstr_in(s)), out)
}
unsafe extern "win64" fn esc_escape(_this: usize, s: usize, out: *mut usize) -> i32 {
    hstr_out(&escape_component(&hstr_in(s)), out)
}

static ACTIVATION_VTBL: VTable<7> = VTable([
    fac_qi as *const (),
    fac_add_ref as *const (),
    fac_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    fac_activate as *const (),
]);
static FACTORY_VTBL: VTable<8> = VTable([
    fac_qi as *const (),
    fac_add_ref as *const (),
    fac_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    fac_create as *const (),
    fac_create_relative as *const (),
]);
static ESCAPE_VTBL: VTable<8> = VTable([
    fac_qi as *const (),
    fac_add_ref as *const (),
    fac_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    esc_unescape as *const (),
    esc_escape as *const (),
]);

/// `RoGetActivationFactory` pour Windows.Foundation.Uri, `None` pour une
/// autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(fac_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn slot<T: Copy>(obj: usize, n: usize) -> T {
        let p = *(*(obj as *const *const *const ())).add(n);
        std::mem::transmute_copy(&p)
    }
    fn h(s: &str) -> usize {
        let w: Vec<u16> = s.encode_utf16().collect();
        let mut out = 0usize;
        unsafe {
            super::super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, &mut out)
        };
        out
    }
    unsafe fn read(obj: usize, n: usize) -> String {
        let f: unsafe extern "win64" fn(usize, *mut usize) -> i32 = slot(obj, n);
        let mut out = 0usize;
        assert_eq!(f(obj, &mut out), S_OK, "emplacement {n}");
        let s = hstr_in(out);
        super::super::electron_win32::WindowsDeleteString(out);
        s
    }

    #[test]
    fn decoupage_rfc3986() {
        let p = parse("HTTP://User:Pw@WWW.Contoso.com:80/a/./b/../page.htm?x=1#top").unwrap();
        assert_eq!(p.scheme, "http");
        assert_eq!(p.host(), "www.contoso.com");
        assert_eq!(p.path, "/a/page.htm");
        assert_eq!(
            p.absolute(),
            "http://User:Pw@www.contoso.com/a/page.htm?x=1#top"
        );
        assert_eq!(p.display(), "http://www.contoso.com/a/page.htm?x=1#top");
        assert_eq!(p.domain(), "contoso.com");
        assert_eq!(p.extension(), ".htm");
        assert_eq!(p.port(), 80);
        assert_eq!(
            parse("https://h:8443").unwrap().absolute(),
            "https://h:8443/"
        );
        assert_eq!(parse("ms-teams:open").unwrap().path, "open");
        assert!(parse("/relative").is_none());
        assert!(parse("").is_none());
        assert!(parse("http://h:port/").is_none());
        // RFC 3986, §5.4.1 (exemples normaux).
        let base = parse("http://a/b/c/d;p?q").unwrap();
        for (r, want) in [
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g/"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("..", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../g", "http://a/g"),
            ("", "http://a/b/c/d;p?q"),
        ] {
            assert_eq!(base.resolve(r).unwrap().absolute(), want, "référence {r:?}");
        }
        assert_eq!(escape_component("a b/é"), "a%20b%2F%C3%A9");
        assert_eq!(unescape_component("a%20b%2F%C3%A9%zz%"), "a b/é%zz%");
    }

    /// #2645 : la fabrique répond, l'objet sert ses trois interfaces.
    #[test]
    fn classe_uri_2645() {
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_URI_FACTORY.as_ptr(), &mut f),
                Some(S_OK)
            );
            assert!(
                activation_factory("Windows.Other", IID_URI_FACTORY.as_ptr(), &mut f).is_none()
            );
            let create: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(f, 6);
            let mut u = 0usize;
            assert_eq!(create(f, h("relatif/x"), &mut u), E_INVALIDARG);
            assert_eq!(u, 0);
            assert_eq!(
                create(f, h("https://teams.microsoft.com/v2/?a=1#f"), &mut u),
                S_OK
            );
            assert_eq!(read(u, 6), "https://teams.microsoft.com/v2/?a=1#f");
            assert_eq!(read(u, 8), "microsoft.com");
            assert_eq!(read(u, 10), "#f");
            assert_eq!(read(u, 11), "teams.microsoft.com");
            assert_eq!(read(u, 13), "/v2/");
            assert_eq!(read(u, 14), "?a=1");
            assert_eq!(read(u, 17), "https");
            let port: unsafe extern "win64" fn(usize, *mut i32) -> i32 = slot(u, 19);
            let mut n = 0;
            assert_eq!(port(u, &mut n), S_OK);
            assert_eq!(n, 443);
            // CombineUri, puis Equals avec un objet créé autrement.
            let combine: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(u, 22);
            let mut c = 0usize;
            assert_eq!(combine(u, h("../x"), &mut c), S_OK);
            assert_eq!(read(c, 6), "https://teams.microsoft.com/x");
            let rel: unsafe extern "win64" fn(usize, usize, usize, *mut usize) -> i32 = slot(f, 7);
            let mut d = 0usize;
            assert_eq!(
                rel(f, h("https://teams.microsoft.com/v2/"), h("/x"), &mut d),
                S_OK
            );
            let equals: unsafe extern "win64" fn(usize, usize, *mut u8) -> i32 = slot(c, 21);
            let mut eq = 0u8;
            assert_eq!(equals(c, d, &mut eq), S_OK);
            assert_eq!(eq, 1);
            assert_eq!(equals(c, u, &mut eq), S_OK);
            assert_eq!(eq, 0);
            // Les deux autres interfaces, et leur identité.
            let mut can = 0usize;
            assert_eq!(uri_qi(u, IID_URI_CANONICAL.as_ptr(), &mut can), S_OK);
            assert_eq!(read(can, 6), "https://teams.microsoft.com/v2/?a=1#f");
            let mut st = 0usize;
            assert_eq!(can_qi(can, IID_STRINGABLE.as_ptr(), &mut st), S_OK);
            assert_eq!(read(st, 6), "https://teams.microsoft.com/v2/?a=1#f");
            let mut back = 0usize;
            assert_eq!(str_qi(st, IID_IUNKNOWN.as_ptr(), &mut back), S_OK);
            assert_eq!(back, u);
            let mut none = 1usize;
            assert_eq!(uri_qi(u, g(1, 2, 3, 4).as_ptr(), &mut none), E_NOINTERFACE);
            assert_eq!(none, 0);
            // Statiques d'échappement.
            let mut esc = 0usize;
            assert_eq!(fac_qi(f, IID_URI_ESCAPE.as_ptr(), &mut esc), S_OK);
            let escape: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 = slot(esc, 7);
            let mut e = 0usize;
            assert_eq!(escape(esc, h("a b"), &mut e), S_OK);
            assert_eq!(hstr_in(e), "a%20b");
            assert_eq!(str_release(st), 3);
            assert_eq!(can_release(can), 2);
            assert_eq!(uri_release(back), 1);
            assert_eq!(uri_release(u), 0);
            assert_eq!(uri_release(c), 0);
            assert_eq!(uri_release(d), 0);
        }
    }
}
