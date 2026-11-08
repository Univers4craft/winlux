//! Serveurs COM hors processus (LocalServer32, #16), pour l'automatisation.
//!
//! Côté serveur, `CoRegisterClassObject(…, CLSCTX_LOCAL_SERVER, …)` ouvre un
//! canal Unix privé `<dossier>/winlux-com/{CLSID}.sock` (dossier 0700, pair
//! vérifié par SO_PEERCRED). Côté client, `CoCreateInstance` avec
//! CLSCTX_LOCAL_SERVER s'y connecte ; si personne n'écoute, il lance la
//! commande `LocalServer32` du registre suivie de `-Embedding`, comme le
//! décrit la documentation publique (« LocalServer32 », « Registering COM
//! Servers »), puis attend le canal.
//!
//! Le client reçoit un mandataire IDispatch : GetIDsOfNames et Invoke sont
//! transportés avec leurs VARIANT par valeur (entiers, réels, booléens,
//! dates, monnaie, BSTR, vide, nul). Les interfaces propres, les
//! références (VT_BYREF), les tableaux et les objets en argument ne
//! traversent pas : DISP_E_BADVARTYPE. Le serveur exécute les appels sur
//! un thread Windows par connexion.

use super::ole32::Guid;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_UNEXPECTED: i32 = 0x8000_ffffu32 as i32;
const CO_E_SERVER_EXEC_FAILURE: i32 = 0x8008_0005u32 as i32;
const RPC_E_DISCONNECTED: i32 = 0x8001_0108u32 as i32;
const REGDB_E_CLASSNOTREG: i32 = 0x8004_0154u32 as i32;
const DISP_E_BADVARTYPE: i32 = 0x8002_0008u32 as i32;
const DISP_E_EXCEPTION: i32 = 0x8002_0009u32 as i32;

pub const CLSCTX_LOCAL_SERVER: u32 = 0x4;

const VARIANT_SIZE: usize = 24;
const VT_BSTR: u16 = 8;

const OP_CREATE: u8 = 1;
const OP_GET_IDS: u8 = 2;
const OP_INVOKE: u8 = 3;

const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const IID_IDISPATCH: Guid = Guid {
    data1: 0x0002_0400,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};

unsafe fn vcall(obj: usize, slot: usize) -> usize {
    *(*(obj as *const *const usize)).add(slot)
}

fn clsid_text(g: &Guid) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

/// Dossier privé des canaux COM de l'utilisateur.
fn dir() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/winlux-{}", unsafe { libc::getuid() })));
    let d = base.join("winlux-com");
    std::fs::create_dir_all(&d).ok()?;
    let _ = std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700));
    let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    Some(d)
}

fn socket_path(clsid: &Guid) -> Option<PathBuf> {
    Some(dir()?.join(format!("{}.sock", clsid_text(clsid))))
}

fn same_user(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    r == 0 && cred.uid == unsafe { libc::getuid() }
}

// ── Messages : u32 longueur, puis contenu ──────────────────────────

#[derive(Default)]
struct Writer(Vec<u8>);
impl Writer {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.u32(v as u32)
    }
    fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn text(&mut self, units: &[u16]) -> &mut Self {
        self.u32(units.len() as u32);
        for u in units {
            self.u16(*u);
        }
        self
    }
}

struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(self.u32()? as i32)
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn text(&mut self) -> Option<Vec<u16>> {
        let n = self.u32()? as usize;
        let raw = self.take(n.checked_mul(2)?)?;
        Some(
            raw.chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect(),
        )
    }
}

/// Plafond d'un message : 16 Mio.
const MAX_MESSAGE: usize = 16 << 20;

fn send(stream: &mut UnixStream, body: &[u8]) -> std::io::Result<()> {
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(body)
}

fn receive(stream: &mut UnixStream) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_MESSAGE {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok(body)
}

/// Taille de la valeur d'un VARIANT transportable, None sinon.
fn value_size(vt: u16) -> Option<usize> {
    Some(match vt {
        0 | 1 => 0,                     // EMPTY, NULL
        16 | 17 => 1,                   // I1, UI1
        2 | 11 | 18 => 2,               // I2, BOOL, UI2
        3 | 4 | 10 | 19 | 22 | 23 => 4, // I4, R4, ERROR, UI4, INT, UINT
        5..=7 | 20 | 21 => 8,           // R8, CY, DATE, I8, UI8
        _ => return None,
    })
}

unsafe fn write_variant(w: &mut Writer, var: *const u8) -> Result<(), i32> {
    let vt = *(var as *const u16);
    w.u16(vt);
    if vt == VT_BSTR {
        let b = *(var.add(8) as *const *const u16);
        let len = if b.is_null() {
            0
        } else {
            super::oleaut32::sys_string_len(b) as usize
        };
        let units = if b.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(b, len)
        };
        w.text(units);
        return Ok(());
    }
    let size = value_size(vt).ok_or(DISP_E_BADVARTYPE)?;
    let mut raw = [0u8; 8];
    std::ptr::copy_nonoverlapping(var.add(8), raw.as_mut_ptr(), size);
    w.u64(u64::from_le_bytes(raw));
    Ok(())
}

/// Lit un VARIANT dans `out` (24 octets) ; un BSTR est alloué.
unsafe fn read_variant(r: &mut Reader, out: *mut u8) -> Option<()> {
    std::ptr::write_bytes(out, 0, VARIANT_SIZE);
    let vt = r.u16()?;
    if vt == VT_BSTR {
        let units = r.text()?;
        let b = super::oleaut32::sys_alloc_string_len(units.as_ptr(), units.len() as u32);
        *(out as *mut u16) = VT_BSTR;
        *(out.add(8) as *mut *mut u16) = b;
        return Some(());
    }
    let size = value_size(vt)?;
    let raw = r.u64()?.to_le_bytes();
    *(out as *mut u16) = vt;
    std::ptr::copy_nonoverlapping(raw.as_ptr(), out.add(8), size);
    Some(())
}

// ── Serveur ────────────────────────────────────────────────────────

struct Served {
    cookie: u32,
    path: PathBuf,
    stop: Arc<AtomicBool>,
}
static SERVED: Mutex<Vec<Served>> = Mutex::new(Vec::new());

/// Ouvre le canal de la fabrique `factory` (déjà AddRef par l'appelant
/// pour la durée de l'inscription).
pub fn serve(cookie: u32, clsid: &Guid, factory: usize) -> i32 {
    let Some(path) = socket_path(clsid) else {
        return E_UNEXPECTED;
    };
    // Un canal mort laissé par un serveur arrêté brutalement.
    if UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            crate::log_warn!(
                "com",
                "LocalServer {} : canal {} refusé ({e})",
                clsid_text(clsid),
                path.display()
            );
            return CO_E_SERVER_EXEC_FAILURE;
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    SERVED.lock().unwrap().push(Served {
        cookie,
        path: path.clone(),
        stop: Arc::clone(&stop),
    });
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let Ok(conn) = conn else { continue };
            if !same_user(&conn) {
                continue;
            }
            let job = Box::new((conn, factory, Arc::clone(&stop)));
            // Le code de l'objet est du code Windows : thread Windows.
            let h = unsafe {
                super::kernel32::create_thread(
                    0,
                    0,
                    connection_thread as *const () as usize,
                    Box::into_raw(job) as usize,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if h != 0 {
                super::kernel32::close_handle(h);
            }
        }
    });
    S_OK
}

/// CoRevokeClassObject : ferme le canal de cette inscription.
pub fn revoke(cookie: u32) {
    let mut served = SERVED.lock().unwrap();
    let Some(i) = served.iter().position(|s| s.cookie == cookie) else {
        return;
    };
    let s = served.remove(i);
    drop(served);
    s.stop.store(true, Ordering::SeqCst);
    let _ = std::fs::remove_file(&s.path);
    // Réveille accept() pour qu'il voie l'arrêt.
    let _ = UnixStream::connect(&s.path);
}

type Job = (UnixStream, usize, Arc<AtomicBool>);

extern "win64" fn connection_thread(param: usize) -> u32 {
    let job = unsafe { Box::from_raw(param as *mut Job) };
    let (mut stream, factory, stop) = *job;
    let mut objects: Vec<usize> = Vec::new();
    while let Ok(body) = receive(&mut stream) {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let reply = unsafe { handle(&body, factory, &mut objects) };
        if send(&mut stream, &reply.0).is_err() {
            break;
        }
    }
    // Le client s'est déconnecté : ses objets sont libérés.
    for obj in objects {
        unsafe {
            let release: unsafe extern "win64" fn(usize) -> u32 =
                std::mem::transmute(vcall(obj, 2));
            release(obj);
        }
    }
    0
}

unsafe fn handle(body: &[u8], factory: usize, objects: &mut Vec<usize>) -> Writer {
    let mut r = Reader(body);
    let mut w = Writer::default();
    match r.u8() {
        Some(OP_CREATE) => {
            let create: unsafe extern "win64" fn(usize, usize, *const Guid, *mut usize) -> i32 =
                std::mem::transmute(vcall(factory, 3));
            let mut obj = 0usize;
            let hr = create(factory, 0, &IID_IDISPATCH, &mut obj);
            if hr >= 0 && obj != 0 {
                objects.push(obj);
                w.i32(S_OK).u32(objects.len() as u32 - 1);
            } else {
                w.i32(if hr < 0 { hr } else { E_NOINTERFACE }).u32(0);
            }
        }
        Some(OP_GET_IDS) => {
            let Some(obj) = r.u32().and_then(|i| objects.get(i as usize).copied()) else {
                w.i32(RPC_E_DISCONNECTED);
                return w;
            };
            let n = r.u32().unwrap_or(0) as usize;
            let mut names: Vec<Vec<u16>> = Vec::new();
            for _ in 0..n.min(1024) {
                let Some(mut t) = r.text() else { break };
                t.push(0);
                names.push(t);
            }
            let ptrs: Vec<*const u16> = names.iter().map(|t| t.as_ptr()).collect();
            let mut ids = vec![-1i32; ptrs.len()];
            let get: unsafe extern "win64" fn(
                usize,
                *const Guid,
                *const *const u16,
                u32,
                u32,
                *mut i32,
            ) -> i32 = std::mem::transmute(vcall(obj, 5));
            let hr = get(
                obj,
                &Guid::default(),
                ptrs.as_ptr(),
                ptrs.len() as u32,
                0x400,
                ids.as_mut_ptr(),
            );
            w.i32(hr).u32(ids.len() as u32);
            for id in ids {
                w.i32(id);
            }
        }
        Some(OP_INVOKE) => invoke_on_server(&mut r, &mut w, objects),
        _ => {
            w.i32(E_UNEXPECTED);
        }
    }
    w
}

unsafe fn invoke_on_server(r: &mut Reader, w: &mut Writer, objects: &[usize]) {
    let parsed = (|| {
        let obj = objects.get(r.u32()? as usize).copied();
        let dispid = r.i32()?;
        let flags = r.u16()?;
        let lcid = r.u32()?;
        let n = r.u32()? as usize;
        if n > 4096 {
            return None;
        }
        let mut args = vec![[0u8; VARIANT_SIZE]; n];
        for a in args.iter_mut() {
            read_variant(r, a.as_mut_ptr())?;
        }
        let named_count = r.u32()? as usize;
        if named_count > n {
            return None;
        }
        let mut named = Vec::with_capacity(named_count);
        for _ in 0..named_count {
            named.push(r.i32()?);
        }
        let want_result = r.u8()? != 0;
        Some((obj, dispid, flags, lcid, args, named, want_result))
    })();
    let Some((obj, dispid, flags, lcid, mut args, named, want_result)) = parsed else {
        w.i32(E_INVALIDARG);
        return;
    };
    let Some(obj) = obj else {
        w.i32(RPC_E_DISCONNECTED);
        return;
    };
    // DISPPARAMS x64.
    let mut params = [0u8; 24];
    *(params.as_mut_ptr() as *mut usize) = if args.is_empty() {
        0
    } else {
        args.as_mut_ptr() as usize
    };
    *(params.as_mut_ptr().add(8) as *mut usize) = if named.is_empty() {
        0
    } else {
        named.as_ptr() as usize
    };
    *(params.as_mut_ptr().add(16) as *mut u32) = args.len() as u32;
    *(params.as_mut_ptr().add(20) as *mut u32) = named.len() as u32;
    let mut result = [0u8; VARIANT_SIZE];
    let mut excep = [0u8; 64];
    let mut arg_err = 0u32;
    type Invoke = unsafe extern "win64" fn(
        usize,
        i32,
        *const Guid,
        u32,
        u16,
        *const u8,
        *mut u8,
        *mut u8,
        *mut u32,
    ) -> i32;
    let invoke: Invoke = std::mem::transmute(vcall(obj, 6));
    let hr = invoke(
        obj,
        dispid,
        &Guid::default(),
        lcid,
        flags,
        params.as_ptr(),
        if want_result {
            result.as_mut_ptr()
        } else {
            std::ptr::null_mut()
        },
        excep.as_mut_ptr(),
        &mut arg_err,
    );
    for a in args.iter_mut() {
        super::oleaut32::variant_clear(a.as_mut_ptr().cast());
    }
    w.i32(hr);
    if write_variant(w, result.as_ptr()).is_err() {
        // Résultat non transportable : rendu vide.
        let mut empty = Writer::default();
        empty.u16(0).u64(0);
        w.0.truncate(w.0.len().saturating_sub(2));
        w.0.extend_from_slice(&empty.0);
    }
    super::oleaut32::variant_clear(result.as_mut_ptr().cast());
    // EXCEPINFO : bstrSource +8, bstrDescription +16, scode +56.
    let ex = excep.as_ptr();
    w.i32(*(ex.add(56) as *const i32)).u16(*(ex as *const u16));
    for off in [8usize, 16] {
        let b = *(ex.add(off) as *const *mut u16);
        if b.is_null() {
            w.text(&[]);
        } else {
            let len = super::oleaut32::sys_string_len(b) as usize;
            w.text(std::slice::from_raw_parts(b, len));
            super::oleaut32::sys_free_string(b);
        }
    }
    let help = *(ex.add(24) as *const *mut u16);
    if !help.is_null() {
        super::oleaut32::sys_free_string(help);
    }
    w.u32(arg_err);
}

// ── Client ─────────────────────────────────────────────────────────

/// Mandataire IDispatch d'un objet d'un autre processus.
#[repr(C)]
struct Proxy {
    vtbl: *const usize,
    refs: AtomicU32,
    stream: Mutex<UnixStream>,
    object: u32,
}

struct Vtbl<const N: usize>([*const (); N]);
unsafe impl<const N: usize> Sync for Vtbl<N> {}

static PROXY_VTBL: Vtbl<7> = Vtbl([
    px_query_interface as *const (),
    px_add_ref as *const (),
    px_release as *const (),
    px_get_type_info_count as *const (),
    px_get_type_info as *const (),
    px_get_ids_of_names as *const (),
    px_invoke as *const (),
]);

unsafe extern "win64" fn px_query_interface(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if iid.is_null() {
        return E_INVALIDARG;
    }
    if *iid == IID_IUNKNOWN || *iid == IID_IDISPATCH {
        px_add_ref(this);
        *out = this;
        return S_OK;
    }
    E_NOINTERFACE
}
unsafe extern "win64" fn px_add_ref(this: usize) -> u32 {
    (*(this as *const Proxy))
        .refs
        .fetch_add(1, Ordering::SeqCst)
        + 1
}
unsafe extern "win64" fn px_release(this: usize) -> u32 {
    let left = (*(this as *const Proxy))
        .refs
        .fetch_sub(1, Ordering::SeqCst)
        - 1;
    if left == 0 {
        // Fermer la connexion libère l'objet côté serveur.
        drop(Box::from_raw(this as *mut Proxy));
    }
    left
}
unsafe extern "win64" fn px_get_type_info_count(_this: usize, count: *mut u32) -> i32 {
    if count.is_null() {
        return E_INVALIDARG;
    }
    *count = 0;
    S_OK
}
unsafe extern "win64" fn px_get_type_info(
    _this: usize,
    _i: u32,
    _lcid: u32,
    out: *mut usize,
) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    E_NOTIMPL
}

unsafe fn round_trip(this: usize, body: &[u8]) -> Result<Vec<u8>, i32> {
    let proxy = &*(this as *const Proxy);
    let mut stream = proxy.stream.lock().unwrap();
    send(&mut stream, body).map_err(|_| RPC_E_DISCONNECTED)?;
    receive(&mut stream).map_err(|_| RPC_E_DISCONNECTED)
}

unsafe extern "win64" fn px_get_ids_of_names(
    this: usize,
    _riid: *const Guid,
    names: *const *const u16,
    count: u32,
    _lcid: u32,
    ids: *mut i32,
) -> i32 {
    if names.is_null() || ids.is_null() {
        return E_INVALIDARG;
    }
    let mut w = Writer::default();
    w.u8(OP_GET_IDS)
        .u32((*(this as *const Proxy)).object)
        .u32(count);
    for i in 0..count as usize {
        let p = *names.add(i);
        let len = if p.is_null() {
            0
        } else {
            (0..).take_while(|&k| *p.add(k) != 0).count()
        };
        w.text(if p.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(p, len)
        });
    }
    let reply = match round_trip(this, &w.0) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let mut r = Reader(&reply);
    let hr = r.i32().unwrap_or(E_UNEXPECTED);
    let n = r.u32().unwrap_or(0) as usize;
    for i in 0..n.min(count as usize) {
        *ids.add(i) = r.i32().unwrap_or(-1);
    }
    hr
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn px_invoke(
    this: usize,
    dispid: i32,
    _riid: *const Guid,
    lcid: u32,
    flags: u16,
    params: *const u8,
    result: *mut u8,
    excep: *mut u8,
    arg_err: *mut u32,
) -> i32 {
    if params.is_null() {
        return E_INVALIDARG;
    }
    let rgvarg = *(params as *const *const u8);
    let named = *(params.add(8) as *const *const i32);
    let c_args = *(params.add(16) as *const u32) as usize;
    let c_named = *(params.add(20) as *const u32) as usize;
    let mut w = Writer::default();
    w.u8(OP_INVOKE)
        .u32((*(this as *const Proxy)).object)
        .i32(dispid)
        .u16(flags)
        .u32(lcid)
        .u32(c_args as u32);
    for i in 0..c_args {
        if let Err(e) = write_variant(&mut w, rgvarg.add(i * VARIANT_SIZE)) {
            if !arg_err.is_null() {
                *arg_err = i as u32;
            }
            return e;
        }
    }
    w.u32(c_named as u32);
    for j in 0..c_named {
        w.i32(*named.add(j));
    }
    w.u8(!result.is_null() as u8);
    let reply = match round_trip(this, &w.0) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let mut r = Reader(&reply);
    let Some(hr) = r.i32() else {
        return E_UNEXPECTED;
    };
    if hr == E_INVALIDARG && r.0.is_empty() {
        return hr;
    }
    let mut value = [0u8; VARIANT_SIZE];
    if read_variant(&mut r, value.as_mut_ptr()).is_none() {
        return E_UNEXPECTED;
    }
    if !result.is_null() {
        super::oleaut32::variant_clear(result.cast());
        std::ptr::copy_nonoverlapping(value.as_ptr(), result, VARIANT_SIZE);
    } else {
        super::oleaut32::variant_clear(value.as_mut_ptr().cast());
    }
    let scode = r.i32().unwrap_or(0);
    let code = r.u16().unwrap_or(0);
    let source = r.text().unwrap_or_default();
    let description = r.text().unwrap_or_default();
    let err = r.u32().unwrap_or(0);
    if hr == DISP_E_EXCEPTION && !excep.is_null() {
        std::ptr::write_bytes(excep, 0, 64);
        *(excep as *mut u16) = code;
        *(excep.add(56) as *mut i32) = scode;
        for (off, text) in [(8usize, &source), (16, &description)] {
            if !text.is_empty() {
                *(excep.add(off) as *mut *mut u16) =
                    super::oleaut32::sys_alloc_string_len(text.as_ptr(), text.len() as u32);
            }
        }
    }
    if !arg_err.is_null() {
        *arg_err = err;
    }
    hr
}

/// Lance le serveur inscrit (LocalServer32 … -Embedding) et attend son canal.
fn launch(clsid: &Guid, path: &std::path::Path) -> Result<UnixStream, i32> {
    let line = super::com_registry::local_server(clsid).ok_or(REGDB_E_CLASSNOTREG)?;
    let mut argv = super::advapi32_services::parse_command_line(&line);
    if argv.is_empty() {
        return Err(REGDB_E_CLASSNOTREG);
    }
    let exe = argv.remove(0);
    let program = crate::paths::to_linux(&exe);
    let windows_exe = program
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("exe"));
    let mut command = if windows_exe {
        let runtime = std::env::current_exe().map_err(|_| CO_E_SERVER_EXEC_FAILURE)?;
        let mut c = std::process::Command::new(runtime);
        c.arg(&program);
        c
    } else {
        std::process::Command::new(&program)
    };
    command.args(&argv).arg("-Embedding");
    let mut child = command.spawn().map_err(|e| {
        crate::log_warn!(
            "com",
            "LocalServer32 {} : lancement impossible ({e})",
            program.display()
        );
        CO_E_SERVER_EXEC_FAILURE
    })?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Ok(s) = UnixStream::connect(path) {
            // Le serveur reste en vie : pas de zombie quand il se termine.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(s);
        }
        if matches!(child.try_wait(), Ok(Some(_))) || std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CO_E_SERVER_EXEC_FAILURE);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// CoCreateInstance(CLSCTX_LOCAL_SERVER) : `None` si la classe n'a ni
/// serveur en écoute ni LocalServer32.
pub unsafe fn create(clsid: &Guid, iid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    if crate::wow32::active() {
        return None;
    }
    let path = socket_path(clsid)?;
    let stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(_) => {
            super::com_registry::local_server(clsid)?;
            match launch(clsid, &path) {
                Ok(s) => s,
                Err(e) => return Some(e),
            }
        }
    };
    if *iid != IID_IUNKNOWN && *iid != IID_IDISPATCH {
        crate::log_warn!(
            "com",
            "LocalServer {} : seule IDispatch traverse les processus (E_NOINTERFACE)",
            clsid_text(clsid)
        );
        return Some(E_NOINTERFACE);
    }
    let proxy = Box::new(Proxy {
        vtbl: PROXY_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        stream: Mutex::new(stream),
        object: 0,
    });
    let raw = Box::into_raw(proxy);
    let mut w = Writer::default();
    w.u8(OP_CREATE);
    let reply = match round_trip(raw as usize, &w.0) {
        Ok(r) => r,
        Err(e) => {
            px_release(raw as usize);
            return Some(e);
        }
    };
    let mut r = Reader(&reply);
    let hr = r.i32().unwrap_or(E_UNEXPECTED);
    if hr < 0 {
        px_release(raw as usize);
        return Some(hr);
    }
    (*raw).object = r.u32().unwrap_or(0);
    *out = raw as *mut c_void;
    Some(S_OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_par_valeur_aller_retour() {
        unsafe {
            let mut v = [0u8; VARIANT_SIZE];
            *(v.as_mut_ptr() as *mut u16) = 5; // VT_R8
            *(v.as_mut_ptr().add(8) as *mut f64) = 2.5;
            let mut w = Writer::default();
            write_variant(&mut w, v.as_ptr()).unwrap();
            let text: Vec<u16> = "é!".encode_utf16().collect();
            let b = super::super::oleaut32::sys_alloc_string_len(text.as_ptr(), 2);
            let mut s = [0u8; VARIANT_SIZE];
            *(s.as_mut_ptr() as *mut u16) = VT_BSTR;
            *(s.as_mut_ptr().add(8) as *mut *mut u16) = b;
            write_variant(&mut w, s.as_ptr()).unwrap();
            super::super::oleaut32::variant_clear(s.as_mut_ptr().cast());
            let mut r = Reader(&w.0);
            let mut out = [0u8; VARIANT_SIZE];
            read_variant(&mut r, out.as_mut_ptr()).unwrap();
            assert_eq!(*(out.as_ptr() as *const u16), 5);
            assert_eq!(*(out.as_ptr().add(8) as *const f64), 2.5);
            read_variant(&mut r, out.as_mut_ptr()).unwrap();
            let back = *(out.as_ptr().add(8) as *const *const u16);
            assert_eq!(std::slice::from_raw_parts(back, 2), &text[..]);
            super::super::oleaut32::variant_clear(out.as_mut_ptr().cast());
            // Un objet (VT_DISPATCH) ne traverse pas.
            let mut d = [0u8; VARIANT_SIZE];
            *(d.as_mut_ptr() as *mut u16) = 9;
            assert_eq!(
                write_variant(&mut Writer::default(), d.as_ptr()),
                Err(DISP_E_BADVARTYPE)
            );
        }
    }

    #[test]
    fn message_trop_long_refuse() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        a.write_all(&(u32::MAX).to_le_bytes()).unwrap();
        assert!(receive(&mut b).is_err());
    }
}
