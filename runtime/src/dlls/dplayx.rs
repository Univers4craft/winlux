//! dplayx.dll / dplay.dll : DirectPlay 4 (#836, suite de #237).
//!
//! Le multijoueur en réseau local des jeux des années 1997–2003 :
//! DirectPlayCreate, DirectPlayEnumerate(A/W), DirectPlayLobbyCreate(A/W),
//! CLSID_DirectPlay et CLSID_DirectPlayLobby par CoCreateInstance.
//! IDirectPlay2/3/4 (et leurs variantes A) partagent un seul objet ; seul le
//! fournisseur TCP/IP est proposé.
//!
//! Transport : un protocole propre à Winlux, et non le protocole filaire de
//! DirectPlay. Les parties sont annoncées en UDP (port 47624, celui que les
//! pare-feux ouvrent déjà pour DirectPlay) et le jeu passe par une connexion
//! TCP vers l'hôte, qui relaie messages et créations de joueurs (topologie en
//! étoile). Deux PC Winlux jouent donc ensemble ; un PC sous Windows ne voit
//! pas ces parties.
//!
//! Comme DirectDraw et Direct3D 9, les tables de méthodes sont celles de
//! l'ABI du programme : un jeu 32 bits appelle directement ces objets, et les
//! structures (DPSESSIONDESC2, DPNAME, messages système) sont écrites dans la
//! disposition de son ABI.

#![allow(clippy::too_many_arguments)]

use super::comobj::{guid, Guid};
use super::legacy_com::{call_back, get_ptr, ptr_size, put_ptr, rd32, vtable_sig, wr32};
use crate::dlls::lock;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match crate::dlls::lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

// ═══════════════════════════════════════════════════════════════════════
// Constantes publiques (dplay.h, dplobby.h)
// ═══════════════════════════════════════════════════════════════════════

const DP_OK: i32 = 0;
const fn dperr(code: u32) -> i32 {
    (0x8877_0000u32 | code) as i32
}
const DPERR_ALREADYINITIALIZED: i32 = dperr(5);
const DPERR_BUFFERTOOSMALL: i32 = dperr(30);
const DPERR_CANTCREATEPLAYER: i32 = dperr(60);
const DPERR_CANTCREATESESSION: i32 = dperr(70);
const DPERR_INVALIDFLAGS: i32 = dperr(120);
const DPERR_INVALIDPLAYER: i32 = dperr(150);
const DPERR_INVALIDGROUP: i32 = dperr(155);
const DPERR_NOCONNECTION: i32 = dperr(170);
const DPERR_NOMESSAGES: i32 = dperr(190);
const DPERR_NOSESSIONS: i32 = dperr(220);
const DPERR_TIMEOUT: i32 = dperr(240);
const DPERR_UNAVAILABLE: i32 = dperr(250);
const DPERR_SESSIONLOST: i32 = dperr(310);
const DPERR_UNINITIALIZED: i32 = dperr(320);
const DPERR_NONEWPLAYERS: i32 = dperr(330);
const DPERR_INVALIDPASSWORD: i32 = dperr(340);
const DPERR_NOTLOBBIED: i32 = dperr(1070);
const DPERR_INVALIDPARAMS: i32 = 0x8007_0057u32 as i32;
const DPERR_UNSUPPORTED: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;

const DPOPEN_JOIN: u32 = 0x1;
const DPOPEN_CREATE: u32 = 0x2;
const DPENUMSESSIONS_AVAILABLE: u32 = 0x1;
const DPENUMSESSIONS_ASYNC: u32 = 0x10;
const DPENUMSESSIONS_STOPASYNC: u32 = 0x20;
const DPENUMSESSIONS_PASSWORDREQUIRED: u32 = 0x40;
const DPESC_TIMEDOUT: u32 = 0x1;
const DPRECEIVE_TOPLAYER: u32 = 0x2;
const DPRECEIVE_FROMPLAYER: u32 = 0x4;
const DPRECEIVE_PEEK: u32 = 0x8;
const DPID_SYSMSG: u32 = 0;
const DPID_ALLPLAYERS: u32 = 0;
const DPID_SERVERPLAYER: u32 = 1;
const DPPLAYERTYPE_GROUP: u32 = 0;
const DPPLAYERTYPE_PLAYER: u32 = 1;
const DPPLAYER_SERVERPLAYER: u32 = 0x100;
const DPPLAYER_LOCAL: u32 = 0x8;
const DPENUMPLAYERS_LOCAL: u32 = 0x8;
const DPENUMPLAYERS_REMOTE: u32 = 0x10;
const DPENUMPLAYERS_SESSION: u32 = 0x80;
const DPENUMGROUPS_SHORTCUT: u32 = 0x400;
const DPSESSION_NEWPLAYERSDISABLED: u32 = 0x1;
const DPSESSION_JOINDISABLED: u32 = 0x20;
const DPSESSION_PASSWORDREQUIRED: u32 = 0x400;
const DPCONNECTION_DIRECTPLAY: u32 = 0x1;
const DPCAPS_ISHOST: u32 = 0x2;

const DPSYS_CREATEPLAYERORGROUP: u32 = 0x0003;
const DPSYS_DESTROYPLAYERORGROUP: u32 = 0x0005;
const DPSYS_ADDPLAYERTOGROUP: u32 = 0x0007;
const DPSYS_DELETEPLAYERFROMGROUP: u32 = 0x0021;
const DPSYS_SESSIONLOST: u32 = 0x0031;

pub const CLSID_DIRECTPLAY: Guid = guid(
    0xd1eb6d20,
    0x8923,
    0x11d0,
    [0x9d, 0x97, 0x00, 0xa0, 0xc9, 0x0a, 0x43, 0xcb],
);
pub const CLSID_DIRECTPLAYLOBBY: Guid = guid(
    0x2fe8f810,
    0xb2a5,
    0x11d0,
    [0xa7, 0x87, 0x00, 0x00, 0xf8, 0x03, 0xab, 0xfc],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDIRECTPLAY: Guid = guid(
    0x5454e9a0,
    0xdb65,
    0x11ce,
    [0x92, 0x1c, 0x00, 0xaa, 0x00, 0x6c, 0x49, 0x72],
);
const IID_IDIRECTPLAY2: Guid = guid(
    0x2b74f7c0,
    0x9154,
    0x11cf,
    [0xa9, 0xcd, 0x00, 0xaa, 0x00, 0x68, 0x86, 0xe3],
);
const IID_IDIRECTPLAY2A: Guid = guid(
    0x9d460580,
    0xa822,
    0x11cf,
    [0x96, 0x0c, 0x00, 0x80, 0xc7, 0x53, 0x4e, 0x82],
);
const IID_IDIRECTPLAY3: Guid = guid(
    0x133efe40,
    0x32dc,
    0x11d0,
    [0x9c, 0xfb, 0x00, 0xa0, 0xc9, 0x0a, 0x43, 0xcb],
);
const IID_IDIRECTPLAY3A: Guid = guid(
    0x133efe41,
    0x32dc,
    0x11d0,
    [0x9c, 0xfb, 0x00, 0xa0, 0xc9, 0x0a, 0x43, 0xcb],
);
const IID_IDIRECTPLAY4: Guid = guid(
    0x0ab1c530,
    0x4745,
    0x11d1,
    [0xa7, 0xa1, 0x00, 0x00, 0xf8, 0x03, 0xab, 0xfc],
);
const IID_IDIRECTPLAY4A: Guid = guid(
    0x0ab1c531,
    0x4745,
    0x11d1,
    [0xa7, 0xa1, 0x00, 0x00, 0xf8, 0x03, 0xab, 0xfc],
);
const LOBBY_IIDS_W: [Guid; 3] = [
    guid(
        0xaf465c71,
        0x9588,
        0x11cf,
        [0xa0, 0x20, 0x00, 0xaa, 0x00, 0x61, 0x57, 0xac],
    ),
    guid(
        0x0194c220,
        0xa303,
        0x11d0,
        [0x9c, 0x4f, 0x00, 0xa0, 0xc9, 0x05, 0x42, 0x5e],
    ),
    guid(
        0x2db72490,
        0x652c,
        0x11d1,
        [0xa7, 0xa8, 0x00, 0x00, 0xf8, 0x03, 0xab, 0xfc],
    ),
];
const LOBBY_IIDS_A: [Guid; 3] = [
    guid(
        0x26c66a70,
        0xb367,
        0x11cf,
        [0xa0, 0x24, 0x00, 0xaa, 0x00, 0x61, 0x57, 0xac],
    ),
    guid(
        0x1bb4af80,
        0xa303,
        0x11d0,
        [0x9c, 0x4f, 0x00, 0xa0, 0xc9, 0x05, 0x42, 0x5e],
    ),
    guid(
        0x2db72491,
        0x652c,
        0x11d1,
        [0xa7, 0xa8, 0x00, 0x00, 0xf8, 0x03, 0xab, 0xfc],
    ),
];

pub const DPSPGUID_TCPIP: Guid = guid(
    0x36e95ee0,
    0x8577,
    0x11cf,
    [0x96, 0x0c, 0x00, 0x80, 0xc7, 0x53, 0x4e, 0x82],
);
const DPAID_TOTALSIZE: Guid = guid(
    0x1318f560,
    0x912c,
    0x11d0,
    [0x9d, 0xaa, 0x00, 0xa0, 0xc9, 0x0a, 0x43, 0xcb],
);
const DPAID_SERVICEPROVIDER: Guid = guid(
    0x07d916c0,
    0xe0af,
    0x11cf,
    [0x9c, 0x4e, 0x00, 0xa0, 0xc9, 0x05, 0x42, 0x5e],
);
const DPAID_INET: Guid = guid(
    0xc4a54da0,
    0xe0af,
    0x11cf,
    [0x9c, 0x4e, 0x00, 0xa0, 0xc9, 0x05, 0x42, 0x5e],
);
const DPAID_INETW: Guid = guid(
    0xe63232a0,
    0x9dbf,
    0x11d1,
    [0x9c, 0x4e, 0x00, 0xa0, 0xc9, 0x05, 0x42, 0x5e],
);
const DPAID_INETPORT: Guid = guid(
    0xe4524541,
    0x8ea5,
    0x11d1,
    [0x8a, 0x96, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11],
);

/// Port d'annonce des parties (celui de DirectPlay), réglable pour les tests.
fn default_port() -> u16 {
    std::env::var("WINLUX_DPLAY_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(47624)
}

fn sp_name() -> &'static str {
    match super::dialogs::session_lang().as_str() {
        "fr" => "Connexion TCP/IP (réseau local et Internet)",
        "de" => "TCP/IP-Verbindung (lokales Netzwerk und Internet)",
        "es" => "Conexión TCP/IP (red local e Internet)",
        _ => "TCP/IP connection (local network and Internet)",
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Codage du protocole Winlux
// ═══════════════════════════════════════════════════════════════════════

const MAGIC: &[u8; 6] = b"WLXDP1";

#[derive(Default)]
struct Wr(Vec<u8>);
impl Wr {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.u32(b.len() as u32);
        self.0.extend_from_slice(b);
        self
    }
    fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }
    fn guid(&mut self, g: &Guid) -> &mut Self {
        self.0.extend_from_slice(g);
        self
    }
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rd { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }
    fn str(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?).ok()
    }
    fn guid(&mut self) -> Option<Guid> {
        self.take(16)?.try_into().ok()
    }
}

// Trames TCP (u32 longueur, u8 type, charge).
const T_ADD: u8 = 1;
const T_DEL: u8 = 2;
const T_MEMBER_ADD: u8 = 3;
const T_MEMBER_DEL: u8 = 4;
const T_SESSION: u8 = 5;
const T_MSG: u8 = 6;
const T_CREATE: u8 = 10;
const T_DESTROY: u8 = 11;
const T_REQ_MEMBER_ADD: u8 = 12;
const T_REQ_MEMBER_DEL: u8 = 13;
const T_SEND: u8 = 14;
const T_JOIN: u8 = 15;
const T_WELCOME: u8 = 20;
const T_REFUSED: u8 = 21;

fn frame(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 5);
    v.extend_from_slice(&(body.len() as u32 + 1).to_le_bytes());
    v.push(kind);
    v.extend_from_slice(body);
    v
}

fn send_frame(s: &mut TcpStream, kind: u8, body: &[u8]) -> bool {
    s.write_all(&frame(kind, body)).is_ok()
}

fn read_frame(s: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut len = [0u8; 4];
    s.read_exact(&mut len).ok()?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > 16 << 20 {
        return None;
    }
    let mut b = vec![0u8; n];
    s.read_exact(&mut b).ok()?;
    let kind = b.remove(0);
    Some((kind, b))
}

// ═══════════════════════════════════════════════════════════════════════
// État d'une session
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Default, Debug, PartialEq)]
struct Session {
    flags: u32,
    instance: Guid,
    application: Guid,
    max_players: u32,
    current: u32,
    name: String,
    password: String,
    user: [u32; 4],
}

impl Session {
    fn encode(&self, w: &mut Wr, with_password: bool) {
        let mut flags = self.flags;
        if !self.password.is_empty() {
            flags |= DPSESSION_PASSWORDREQUIRED;
        }
        w.u32(flags)
            .guid(&self.instance)
            .guid(&self.application)
            .u32(self.max_players)
            .u32(self.current)
            .str(&self.name)
            .str(if with_password { &self.password } else { "" });
        for u in self.user {
            w.u32(u);
        }
    }
    fn decode(r: &mut Rd) -> Option<Session> {
        let mut s = Session {
            flags: r.u32()?,
            instance: r.guid()?,
            application: r.guid()?,
            max_players: r.u32()?,
            current: r.u32()?,
            name: r.str()?,
            password: r.str()?,
            user: [0; 4],
        };
        for u in s.user.iter_mut() {
            *u = r.u32()?;
        }
        Some(s)
    }
}

#[derive(Clone, Default, Debug)]
struct Entity {
    id: u32,
    group: bool,
    /// Connexion propriétaire : 0 = l'hôte.
    owner: u32,
    flags: u32,
    parent: u32,
    short: String,
    long: String,
    data: Vec<u8>,
    members: Vec<u32>,
    /// Événement Win32 du joueur local (CreatePlayer), 0 sinon.
    event: usize,
}

impl Entity {
    fn encode(&self, w: &mut Wr) {
        w.u32(self.id)
            .u8(self.group as u8)
            .u32(self.owner)
            .u32(self.flags)
            .u32(self.parent)
            .str(&self.short)
            .str(&self.long)
            .bytes(&self.data)
            .u32(self.members.len() as u32);
        for m in &self.members {
            w.u32(*m);
        }
    }
    fn decode(r: &mut Rd) -> Option<Entity> {
        let mut e = Entity {
            id: r.u32()?,
            group: r.u8()? != 0,
            owner: r.u32()?,
            flags: r.u32()?,
            parent: r.u32()?,
            short: r.str()?,
            long: r.str()?,
            data: r.bytes()?,
            ..Default::default()
        };
        let n = r.u32()?;
        for _ in 0..n.min(65536) {
            e.members.push(r.u32()?);
        }
        Some(e)
    }
}

#[derive(Clone, Debug)]
enum Sys {
    Create(Entity),
    Destroy(Entity),
    AddToGroup(u32, u32),
    DeleteFromGroup(u32, u32),
    SessionLost,
}

#[derive(Clone, Debug)]
enum Body {
    User(Vec<u8>),
    Sys(Sys),
}

#[derive(Clone, Debug)]
struct Msg {
    from: u32,
    to: u32,
    body: Body,
}

#[derive(Clone, Debug)]
struct Found {
    session: Session,
    addr: SocketAddr,
}

struct State {
    /// Fournisseur de service choisi (DirectPlayCreate ou InitializeConnection).
    ready: bool,
    /// Adresse donnée par le programme (DPAID_INet), pour chercher hors du
    /// réseau local.
    inet: Option<String>,
    port: u16,
    session: Option<Session>,
    host: bool,
    /// Identifiant de notre connexion chez l'hôte (0 = nous sommes l'hôte).
    me: u32,
    lost: bool,
    ents: BTreeMap<u32, Entity>,
    next_id: u32,
    queue: VecDeque<Msg>,
    found: Vec<Found>,
    /// Hôte : connexions des clients.
    clients: HashMap<u32, TcpStream>,
    next_conn: u32,
    /// Client : connexion vers l'hôte.
    upstream: Option<TcpStream>,
    nonce: u32,
    created: HashMap<u32, u32>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    enum_thread: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

struct Inner {
    st: Mutex<State>,
    cv: Condvar,
}

impl Inner {
    fn new() -> Arc<Inner> {
        Arc::new(Inner {
            st: Mutex::new(State {
                ready: false,
                inet: None,
                port: default_port(),
                session: None,
                host: false,
                me: 0,
                lost: false,
                ents: BTreeMap::new(),
                next_id: 0x100,
                queue: VecDeque::new(),
                found: Vec::new(),
                clients: HashMap::new(),
                next_conn: 1,
                upstream: None,
                nonce: 1,
                created: HashMap::new(),
                stop: Arc::new(AtomicBool::new(false)),
                threads: Vec::new(),
                enum_thread: None,
            }),
            cv: Condvar::new(),
        })
    }
}

impl State {
    fn local(&self, e: &Entity) -> bool {
        e.owner == self.me
    }
    fn local_players(&self) -> Vec<(u32, usize)> {
        self.ents
            .values()
            .filter(|e| !e.group && e.owner == self.me)
            .map(|e| (e.id, e.event))
            .collect()
    }
    fn player_count(&self) -> u32 {
        self.ents.values().filter(|e| !e.group).count() as u32
    }
    fn queue_to(&mut self, from: u32, to: u32, body: Body) {
        let event = self.ents.get(&to).map(|e| e.event).unwrap_or(0);
        self.queue.push_back(Msg { from, to, body });
        if event != 0 {
            unsafe {
                api!("kernel32", "SetEvent", fn(usize) -> i32, event);
            }
        }
    }
    /// Message système à chaque joueur local (sauf `except`).
    fn queue_sys(&mut self, sys: Sys, except: u32) {
        for (id, _) in self.local_players() {
            if id != except {
                self.queue_to(DPID_SYSMSG, id, Body::Sys(sys.clone()));
            }
        }
    }
    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    fn broadcast(&mut self, kind: u8, body: &[u8], except: u32) {
        let f = frame(kind, body);
        let mut dead = Vec::new();
        for (c, s) in self.clients.iter_mut() {
            if *c != except && s.write_all(&f).is_err() {
                dead.push(*c);
            }
        }
        for c in dead {
            if let Some(s) = self.clients.remove(&c) {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
    }
    fn send_to(&mut self, conn: u32, kind: u8, body: &[u8]) {
        if let Some(s) = self.clients.get_mut(&conn) {
            if !send_frame(s, kind, body) {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
    }
    fn upstream(&mut self, kind: u8, body: &[u8]) -> bool {
        match self.upstream.as_mut() {
            Some(s) => send_frame(s, kind, body),
            None => false,
        }
    }

    // ---- Changements répliqués (appliqués chez l'hôte et chez chaque client) ----

    fn apply_add(&mut self, e: Entity) {
        let remote = !self.local(&e);
        if let Some(parent) = self.ents.get_mut(&e.parent) {
            if parent.group && e.group {
                // Sous-groupe : rien d'autre à tenir.
            }
        }
        self.ents.insert(e.id, e.clone());
        if remote {
            self.queue_sys(Sys::Create(e), 0);
        }
    }
    fn apply_del(&mut self, id: u32) {
        if let Some(e) = self.ents.remove(&id) {
            for g in self.ents.values_mut() {
                g.members.retain(|m| *m != id);
            }
            let remote = !self.local(&e);
            if remote {
                self.queue_sys(Sys::Destroy(e), 0);
            }
            // Les messages en attente pour un joueur détruit sont perdus.
            self.queue.retain(|m| m.to != id);
        }
    }
    fn apply_member(&mut self, group: u32, player: u32, add: bool) {
        let Some(g) = self.ents.get_mut(&group) else {
            return;
        };
        let had = g.members.contains(&player);
        if add && !had {
            g.members.push(player);
            self.queue_sys(Sys::AddToGroup(group, player), 0);
        } else if !add && had {
            g.members.retain(|m| *m != player);
            self.queue_sys(Sys::DeleteFromGroup(group, player), 0);
        }
    }

    // ---- Hôte ----

    fn host_add(&mut self, mut e: Entity, nonce: u32) -> u32 {
        if e.id == 0 {
            e.id = self.alloc_id();
        }
        let id = e.id;
        let mut w = Wr::default();
        e.encode(&mut w);
        w.u32(nonce).u32(e.owner);
        self.broadcast(T_ADD, &w.0, u32::MAX);
        self.apply_add(e);
        self.refresh_count();
        id
    }
    fn host_del(&mut self, id: u32) {
        let mut w = Wr::default();
        w.u32(id);
        self.broadcast(T_DEL, &w.0, u32::MAX);
        self.apply_del(id);
        self.refresh_count();
    }
    fn host_member(&mut self, group: u32, player: u32, add: bool) {
        let mut w = Wr::default();
        w.u32(group).u32(player);
        self.broadcast(
            if add { T_MEMBER_ADD } else { T_MEMBER_DEL },
            &w.0,
            u32::MAX,
        );
        self.apply_member(group, player, add);
    }
    fn refresh_count(&mut self) {
        let n = self.player_count();
        if let Some(s) = self.session.as_mut() {
            s.current = n;
        }
    }
    /// Joueurs visés par un envoi (joueur, groupe ou tous), sans l'émetteur.
    fn targets(&self, from: u32, to: u32) -> Vec<u32> {
        if to == DPID_ALLPLAYERS {
            return self
                .ents
                .values()
                .filter(|e| !e.group && e.id != from)
                .map(|e| e.id)
                .collect();
        }
        match self.ents.get(&to) {
            Some(e) if e.group => e.members.iter().copied().filter(|m| *m != from).collect(),
            Some(e) => vec![e.id],
            None => Vec::new(),
        }
    }
    fn host_route(&mut self, from: u32, to: u32, data: &[u8]) {
        for t in self.targets(from, to) {
            let owner = match self.ents.get(&t) {
                Some(e) => e.owner,
                None => continue,
            };
            if owner == 0 {
                self.queue_to(from, t, Body::User(data.to_vec()));
            } else {
                let mut w = Wr::default();
                w.u32(from).u32(t).bytes(data);
                self.send_to(owner, T_MSG, &w.0);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Fils réseau
// ═══════════════════════════════════════════════════════════════════════

fn enum_request(app: &Guid) -> Vec<u8> {
    let mut v = MAGIC.to_vec();
    v.push(1);
    v.extend_from_slice(app);
    v
}

/// Annonce des parties : répond aux demandes UDP.
fn announce_loop(inner: Arc<Inner>, sock: UdpSocket, stop: Arc<AtomicBool>, tcp_port: u16) {
    let _ = sock.set_read_timeout(Some(Duration::from_millis(100)));
    let mut buf = [0u8; 512];
    while !stop.load(Ordering::Acquire) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        let b = &buf[..n];
        if n < 23 || &b[..6] != MAGIC || b[6] != 1 {
            continue;
        }
        let app: Guid = b[7..23].try_into().unwrap_or_default();
        let reply = {
            let st = lock(&inner.st);
            let Some(s) = st.session.as_ref() else {
                continue;
            };
            if app != [0; 16] && app != s.application {
                continue;
            }
            let mut w = Wr::default();
            w.0.extend_from_slice(MAGIC);
            w.u8(2);
            s.encode(&mut w, false);
            w.u32(tcp_port as u32);
            w.0
        };
        let _ = sock.send_to(&reply, from);
    }
}

/// Recherche des parties : demande en diffusion, sur la machine et vers
/// l'adresse donnée par le programme.
fn discover(port: u16, inet: Option<&str>, app: &Guid, wait: Duration) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return out;
    };
    let _ = sock.set_broadcast(true);
    let mut dests: Vec<SocketAddr> = vec![
        SocketAddr::from(([255, 255, 255, 255], port)),
        SocketAddr::from(([127, 0, 0, 1], port)),
    ];
    if let Some(h) = inet.filter(|h| !h.trim().is_empty()) {
        let h = h.trim();
        let spec = if h.contains(':') && !h.starts_with('[') && h.matches(':').count() == 1 {
            h.to_string()
        } else {
            format!("{h}:{port}")
        };
        if let Ok(it) = spec.to_socket_addrs() {
            dests.extend(it.filter(|a| a.is_ipv4()));
        }
    }
    let req = enum_request(app);
    let start = Instant::now();
    let mut last_send: Option<Instant> = None;
    let mut buf = [0u8; 2048];
    loop {
        let elapsed = start.elapsed();
        if elapsed >= wait {
            break;
        }
        if last_send.is_none_or(|t| t.elapsed() >= Duration::from_millis(300)) {
            for d in &dests {
                let _ = sock.send_to(&req, d);
            }
            last_send = Some(Instant::now());
        }
        let left = (wait - elapsed).min(Duration::from_millis(100));
        let _ = sock.set_read_timeout(Some(left.max(Duration::from_millis(1))));
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        let b = &buf[..n];
        if n < 7 || &b[..6] != MAGIC || b[6] != 2 {
            continue;
        }
        let mut r = Rd::new(&b[7..]);
        let (Some(session), Some(tcp)) = (Session::decode(&mut r), r.u32()) else {
            continue;
        };
        let addr = SocketAddr::new(from.ip(), tcp as u16);
        match out
            .iter_mut()
            .find(|f| f.session.instance == session.instance)
        {
            Some(f) => f.session = session,
            None => out.push(Found { session, addr }),
        }
    }
    out
}

/// Hôte : accepte les clients.
fn accept_loop(inner: Arc<Inner>, listener: TcpListener, stop: Arc<AtomicBool>) {
    let _ = listener.set_nonblocking(true);
    let mut readers = Vec::new();
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((s, _)) => {
                let _ = s.set_nonblocking(false);
                let _ = s.set_nodelay(true);
                let inner = inner.clone();
                let stop = stop.clone();
                readers.push(std::thread::spawn(move || host_reader(inner, s, stop)));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    for r in readers {
        let _ = r.join();
    }
}

/// Hôte : trames d'un client.
fn host_reader(inner: Arc<Inner>, mut s: TcpStream, stop: Arc<AtomicBool>) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let Some((T_JOIN, body)) = read_frame(&mut s) else {
        return;
    };
    let _ = s.set_read_timeout(None);
    let password = Rd::new(&body).str().unwrap_or_default();
    let conn = {
        let mut st = lock(&inner.st);
        let Some(sess) = st.session.clone() else {
            return;
        };
        let refuse = if sess.flags & (DPSESSION_NEWPLAYERSDISABLED | DPSESSION_JOINDISABLED) != 0
            || (sess.max_players != 0 && st.player_count() >= sess.max_players)
        {
            Some(DPERR_NONEWPLAYERS)
        } else if !sess.password.is_empty() && sess.password != password {
            Some(DPERR_INVALIDPASSWORD)
        } else {
            None
        };
        if let Some(hr) = refuse {
            let mut w = Wr::default();
            w.u32(hr as u32);
            send_frame(&mut s, T_REFUSED, &w.0);
            return;
        }
        let conn = st.next_conn;
        st.next_conn += 1;
        let mut w = Wr::default();
        w.u32(conn);
        sess.encode(&mut w, false);
        w.u32(st.ents.len() as u32);
        for e in st.ents.values() {
            e.encode(&mut w);
        }
        if !send_frame(&mut s, T_WELCOME, &w.0) {
            return;
        }
        match s.try_clone() {
            Ok(c) => {
                st.clients.insert(conn, c);
            }
            Err(_) => return,
        }
        conn
    };
    while let Some((kind, body)) = read_frame(&mut s) {
        let mut st = lock(&inner.st);
        let mut r = Rd::new(&body);
        match kind {
            T_CREATE => {
                if let (Some(mut e), Some(nonce)) = (Entity::decode(&mut r), r.u32()) {
                    e.id = 0;
                    e.owner = conn;
                    e.event = 0;
                    if e.flags & DPPLAYER_SERVERPLAYER != 0 {
                        e.flags &= !DPPLAYER_SERVERPLAYER;
                    }
                    st.host_add(e, nonce);
                }
            }
            T_DESTROY => {
                if let Some(id) = r.u32() {
                    if st.ents.get(&id).is_some_and(|e| e.owner == conn) {
                        st.host_del(id);
                    }
                }
            }
            T_REQ_MEMBER_ADD | T_REQ_MEMBER_DEL => {
                if let (Some(g), Some(p)) = (r.u32(), r.u32()) {
                    st.host_member(g, p, kind == T_REQ_MEMBER_ADD);
                }
            }
            T_SEND => {
                if let (Some(from), Some(to), Some(data)) = (r.u32(), r.u32(), r.bytes()) {
                    if st.ents.get(&from).is_some_and(|e| e.owner == conn) {
                        st.host_route(from, to, &data);
                    }
                }
            }
            _ => {}
        }
    }
    // Client parti : ses joueurs et groupes disparaissent.
    let mut st = lock(&inner.st);
    st.clients.remove(&conn);
    if !stop.load(Ordering::Acquire) {
        let gone: Vec<u32> = st
            .ents
            .values()
            .filter(|e| e.owner == conn)
            .map(|e| e.id)
            .collect();
        for id in gone {
            st.host_del(id);
        }
    }
}

/// Client : trames de l'hôte.
fn client_reader(inner: Arc<Inner>, mut s: TcpStream, stop: Arc<AtomicBool>) {
    while let Some((kind, body)) = read_frame(&mut s) {
        let mut st = lock(&inner.st);
        let mut r = Rd::new(&body);
        match kind {
            T_ADD => {
                if let Some(e) = Entity::decode(&mut r) {
                    let nonce = r.u32().unwrap_or(0);
                    let owner = r.u32().unwrap_or(e.owner);
                    let id = e.id;
                    st.apply_add(Entity { owner, ..e });
                    if owner == st.me && nonce != 0 {
                        st.created.insert(nonce, id);
                        inner.cv.notify_all();
                    }
                }
            }
            T_DEL => {
                if let Some(id) = r.u32() {
                    st.apply_del(id);
                }
            }
            T_MEMBER_ADD | T_MEMBER_DEL => {
                if let (Some(g), Some(p)) = (r.u32(), r.u32()) {
                    st.apply_member(g, p, kind == T_MEMBER_ADD);
                }
            }
            T_SESSION => {
                if let Some(sess) = Session::decode(&mut r) {
                    st.session = Some(sess);
                }
            }
            T_MSG => {
                if let (Some(from), Some(to), Some(data)) = (r.u32(), r.u32(), r.bytes()) {
                    if st.ents.get(&to).is_some_and(|e| e.owner == st.me) {
                        st.queue_to(from, to, Body::User(data));
                    }
                }
            }
            _ => {}
        }
    }
    if !stop.load(Ordering::Acquire) {
        let mut st = lock(&inner.st);
        st.lost = true;
        st.upstream = None;
        st.queue_sys(Sys::SessionLost, 0);
        inner.cv.notify_all();
    }
}

fn close_session(inner: &Arc<Inner>) {
    let (threads, stop) = {
        let mut st = lock(&inner.st);
        if st.session.is_none() {
            return;
        }
        st.stop.store(true, Ordering::Release);
        for (_, s) in st.clients.drain() {
            let _ = s.shutdown(Shutdown::Both);
        }
        if let Some(s) = st.upstream.take() {
            let _ = s.shutdown(Shutdown::Both);
        }
        st.session = None;
        st.host = false;
        st.lost = false;
        st.me = 0;
        st.ents.clear();
        st.queue.clear();
        st.created.clear();
        (std::mem::take(&mut st.threads), st.stop.clone())
    };
    let _ = stop;
    for t in threads {
        let _ = t.join();
    }
}

fn stop_async_enum(inner: &Arc<Inner>) {
    let t = lock(&inner.st).enum_thread.take();
    if let Some((stop, t)) = t {
        stop.store(true, Ordering::Release);
        let _ = t.join();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Objet DirectPlay
// ═══════════════════════════════════════════════════════════════════════

#[repr(C)]
struct Face {
    vtbl: usize,
    obj: *const Obj,
}

const FACE_W: usize = 0;
const FACE_A: usize = 1;
const FACE_V1: usize = 2;

#[repr(C)]
struct Obj {
    faces: [Face; 3],
    refs: AtomicU32,
    inner: Arc<Inner>,
}

unsafe fn obj<'a>(this: usize) -> &'a Obj {
    &*(*(this as *const Face)).obj
}
unsafe fn ansi(this: usize) -> bool {
    let o = obj(this);
    this != &o.faces[FACE_W] as *const Face as usize
}
unsafe fn inner(this: usize) -> Arc<Inner> {
    obj(this).inner.clone()
}

fn new_obj(ready: bool) -> usize {
    let inner = Inner::new();
    lock(&inner.st).ready = ready;
    let tables = [dp_vtable(false), dp_vtable(true), dp1_vtable()];
    let o = Box::into_raw(Box::new(Obj {
        faces: tables.map(|vtbl| Face {
            vtbl,
            obj: std::ptr::null(),
        }),
        refs: AtomicU32::new(1),
        inner,
    }));
    unsafe {
        for f in (*o).faces.iter_mut() {
            f.obj = o;
        }
        &(*o).faces[FACE_V1] as *const Face as usize
    }
}

unsafe extern "win64" fn dp_qi(this: usize, iid: *const Guid, out: usize) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    put_ptr(out, 0);
    if iid.is_null() {
        return DPERR_INVALIDPARAMS;
    }
    let o = obj(this);
    let iid = *iid;
    let face = if iid == IID_IUNKNOWN
        || iid == IID_IDIRECTPLAY2
        || iid == IID_IDIRECTPLAY3
        || iid == IID_IDIRECTPLAY4
    {
        FACE_W
    } else if iid == IID_IDIRECTPLAY2A || iid == IID_IDIRECTPLAY3A || iid == IID_IDIRECTPLAY4A {
        FACE_A
    } else if iid == IID_IDIRECTPLAY {
        FACE_V1
    } else {
        return E_NOINTERFACE;
    };
    o.refs.fetch_add(1, Ordering::Relaxed);
    put_ptr(out, &o.faces[face] as *const Face as usize);
    DP_OK
}
unsafe extern "win64" fn dp_addref(this: usize) -> u32 {
    obj(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn dp_release(this: usize) -> u32 {
    let o = obj(this);
    let n = o.refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        stop_async_enum(&o.inner);
        close_session(&o.inner);
        drop(Box::from_raw(o as *const Obj as *mut Obj));
    }
    n
}

// ---- Structures du programme (disposition de son ABI) ----

/// Chaîne A ou W lue dans la mémoire du programme.
unsafe fn read_str(p: usize, a: bool) -> String {
    if p == 0 {
        String::new()
    } else if a {
        super::cstr(p as *const u8)
    } else {
        super::wstr(p as *const u16)
    }
}
fn enc_str(s: &str, a: bool) -> Vec<u8> {
    if a {
        let mut v = super::encode_ansi(s);
        v.push(0);
        v
    } else {
        let mut v: Vec<u8> = s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        v.extend_from_slice(&[0, 0]);
        v
    }
}
fn align(v: usize, a: usize) -> usize {
    v.div_ceil(a) * a
}

/// DPNAME : (court, long).
unsafe fn read_name(p: usize, a: bool) -> (String, String) {
    if p == 0 {
        return Default::default();
    }
    let ps = ptr_size();
    (
        read_str(get_ptr(p + 8), a),
        read_str(get_ptr(p + 8 + ps), a),
    )
}

/// Tampon construit pour le programme : structure fixe, chaînes et données
/// à la suite ; `ptrs` = (position de la case, position visée).
struct Blob {
    b: Vec<u8>,
    ptrs: Vec<(usize, usize)>,
}
impl Blob {
    fn new(fixed: usize) -> Self {
        Blob {
            b: vec![0; fixed],
            ptrs: Vec::new(),
        }
    }
    fn u32_at(&mut self, at: usize, v: u32) {
        self.b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    /// Ajoute `data` (aligné) et fait pointer la case `at` dessus.
    fn tail(&mut self, at: usize, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let pos = align(self.b.len(), 4);
        self.b.resize(pos, 0);
        self.b.extend_from_slice(data);
        self.ptrs.push((at, pos));
    }
    fn name(&mut self, at: usize, short: &str, long: &str, a: bool) {
        let ps = ptr_size();
        self.u32_at(at, (8 + 2 * ps) as u32);
        if !short.is_empty() {
            self.tail(at + 8, &enc_str(short, a));
        }
        if !long.is_empty() {
            self.tail(at + 8 + ps, &enc_str(long, a));
        }
    }
    /// Octets finaux pour l'adresse `base` du programme.
    fn finish(mut self, base: usize) -> Vec<u8> {
        let ps = ptr_size();
        for (at, pos) in std::mem::take(&mut self.ptrs) {
            let v = (base + pos) as u64;
            self.b[at..at + ps].copy_from_slice(&v.to_le_bytes()[..ps]);
        }
        self.b
    }
}

fn session_layout() -> (usize, usize) {
    // (taille, position du nom) : 80 en x86, 88 en x64.
    let ps = ptr_size();
    (48 + 2 * ps + 24, 48)
}

unsafe fn read_session(p: usize, a: bool) -> Session {
    let ps = ptr_size();
    let mut s = Session {
        flags: rd32(p + 4),
        ..Default::default()
    };
    s.instance
        .copy_from_slice(std::slice::from_raw_parts((p + 8) as *const u8, 16));
    s.application
        .copy_from_slice(std::slice::from_raw_parts((p + 24) as *const u8, 16));
    s.max_players = rd32(p + 40);
    s.current = rd32(p + 44);
    s.name = read_str(get_ptr(p + 48), a);
    s.password = read_str(get_ptr(p + 48 + ps), a);
    let u = 48 + 2 * ps + 8;
    for (i, v) in s.user.iter_mut().enumerate() {
        *v = rd32(p + u + 4 * i);
    }
    s
}

fn session_blob(s: &Session, a: bool) -> Blob {
    let ps = ptr_size();
    let (size, name_at) = session_layout();
    let mut b = Blob::new(size);
    b.u32_at(0, size as u32);
    let mut flags = s.flags;
    if !s.password.is_empty() {
        flags |= DPSESSION_PASSWORDREQUIRED;
    }
    b.u32_at(4, flags);
    b.b[8..24].copy_from_slice(&s.instance);
    b.b[24..40].copy_from_slice(&s.application);
    b.u32_at(40, s.max_players);
    b.u32_at(44, s.current);
    b.tail(name_at, &enc_str(&s.name, a));
    let u = 48 + 2 * ps + 8;
    for (i, v) in s.user.iter().enumerate() {
        b.u32_at(u + 4 * i, *v);
    }
    b
}

fn name_blob(e: &Entity, a: bool) -> Blob {
    let ps = ptr_size();
    let mut b = Blob::new(8 + 2 * ps);
    b.name(0, &e.short, &e.long, a);
    b
}

fn sys_blob(sys: &Sys, a: bool) -> Blob {
    let ps = ptr_size();
    match sys {
        Sys::Create(e) => {
            let data_size = 16 + ps;
            let name = align(data_size + 4, ps);
            let parent = name + 8 + 2 * ps;
            let total = align(parent + 8, ps);
            let mut b = Blob::new(total);
            b.u32_at(0, DPSYS_CREATEPLAYERORGROUP);
            b.u32_at(
                4,
                if e.group {
                    DPPLAYERTYPE_GROUP
                } else {
                    DPPLAYERTYPE_PLAYER
                },
            );
            b.u32_at(8, e.id);
            b.u32_at(12, 0); // complété par l'appelant
            b.u32_at(data_size, e.data.len() as u32);
            b.u32_at(parent, e.parent);
            b.u32_at(parent + 4, e.flags);
            b.name(name, &e.short, &e.long, a);
            b.tail(16, &e.data);
            b
        }
        Sys::Destroy(e) => {
            let local = align(12, ps);
            let remote = align(local + ps + 4, ps);
            let name = align(remote + ps + 4, ps);
            let parent = name + 8 + 2 * ps;
            let total = align(parent + 8, ps);
            let mut b = Blob::new(total);
            b.u32_at(0, DPSYS_DESTROYPLAYERORGROUP);
            b.u32_at(
                4,
                if e.group {
                    DPPLAYERTYPE_GROUP
                } else {
                    DPPLAYERTYPE_PLAYER
                },
            );
            b.u32_at(8, e.id);
            b.u32_at(remote + ps, e.data.len() as u32);
            b.u32_at(parent, e.parent);
            b.u32_at(parent + 4, e.flags);
            b.name(name, &e.short, &e.long, a);
            b.tail(remote, &e.data);
            b
        }
        Sys::AddToGroup(g, p) | Sys::DeleteFromGroup(g, p) => {
            let mut b = Blob::new(12);
            b.u32_at(
                0,
                if matches!(sys, Sys::AddToGroup(..)) {
                    DPSYS_ADDPLAYERTOGROUP
                } else {
                    DPSYS_DELETEPLAYERFROMGROUP
                },
            );
            b.u32_at(4, *g);
            b.u32_at(8, *p);
            b
        }
        Sys::SessionLost => {
            let mut b = Blob::new(4);
            b.u32_at(0, DPSYS_SESSIONLOST);
            b
        }
    }
}

/// Copie `blob` dans (data, *size) : DPERR_BUFFERTOOSMALL si trop petit.
unsafe fn give(blob: Blob, data: usize, size: usize) -> i32 {
    if size == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let need = blob.b.len();
    let cap = rd32(size) as usize;
    wr32(size, need as u32);
    if data == 0 || cap < need {
        return DPERR_BUFFERTOOSMALL;
    }
    let bytes = blob.finish(data);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data as *mut u8, need);
    DP_OK
}

/// Appel d'un rappel du programme avec un tampon recopié sur le tas (sous
/// 4 Go pour un programme 32 bits).
unsafe fn with_blob<R>(blob: Option<Blob>, f: impl FnOnce(usize) -> R) -> R {
    match blob {
        Some(b) => {
            let mut heap = vec![0u8; b.b.len().max(1)];
            let base = heap.as_mut_ptr() as usize;
            let bytes = b.finish(base);
            heap[..bytes.len()].copy_from_slice(&bytes);
            f(base)
        }
        None => f(0),
    }
}

// ---- Méthodes IDirectPlay2/3/4 ----

unsafe extern "win64" fn dp_add_player_to_group(this: usize, group: u32, player: u32) -> i32 {
    member_change(this, group, player, true)
}
unsafe extern "win64" fn dp_delete_player_from_group(this: usize, group: u32, player: u32) -> i32 {
    member_change(this, group, player, false)
}
unsafe fn member_change(this: usize, group: u32, player: u32, add: bool) -> i32 {
    let inner = inner(this);
    let mut st = lock(&inner.st);
    if st.session.is_none() {
        return DPERR_NOCONNECTION;
    }
    if !st.ents.get(&group).is_some_and(|e| e.group) {
        return DPERR_INVALIDGROUP;
    }
    if !st.ents.get(&player).is_some_and(|e| !e.group) {
        return DPERR_INVALIDPLAYER;
    }
    if st.host {
        st.host_member(group, player, add);
    } else {
        let mut w = Wr::default();
        w.u32(group).u32(player);
        let kind = if add {
            T_REQ_MEMBER_ADD
        } else {
            T_REQ_MEMBER_DEL
        };
        if !st.upstream(kind, &w.0) {
            return DPERR_SESSIONLOST;
        }
        // Appliqué tout de suite ici : l'hôte rediffusera le même changement.
        let g = st.ents.get_mut(&group).expect("groupe vérifié");
        if add && !g.members.contains(&player) {
            g.members.push(player);
        } else if !add {
            g.members.retain(|m| *m != player);
        }
    }
    DP_OK
}

unsafe extern "win64" fn dp_close(this: usize) -> i32 {
    let inner = inner(this);
    if lock(&inner.st).session.is_none() {
        return DPERR_NOCONNECTION;
    }
    close_session(&inner);
    DP_OK
}

unsafe fn create_entity(
    this: usize,
    out: usize,
    name: usize,
    event: usize,
    data: usize,
    size: u32,
    flags: u32,
    group: bool,
    parent: u32,
) -> i32 {
    if out == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let a = ansi(this);
    let (short, long) = read_name(name, a);
    let data = if data != 0 && size > 0 {
        std::slice::from_raw_parts(data as *const u8, size as usize).to_vec()
    } else {
        Vec::new()
    };
    let inner = inner(this);
    let mut st = lock(&inner.st);
    if st.session.is_none() {
        return DPERR_NOCONNECTION;
    }
    if st.lost {
        return DPERR_SESSIONLOST;
    }
    let server = !group && flags & DPPLAYER_SERVERPLAYER != 0;
    if server && (!st.host || st.ents.contains_key(&DPID_SERVERPLAYER)) {
        return DPERR_CANTCREATEPLAYER;
    }
    let e = Entity {
        id: if server { DPID_SERVERPLAYER } else { 0 },
        group,
        owner: st.me,
        flags,
        parent,
        short,
        long,
        data,
        members: Vec::new(),
        event: if group { 0 } else { event },
    };
    let id = if st.host {
        let id = st.host_add(e, 0);
        if let Some(x) = st.ents.get_mut(&id) {
            x.event = if group { 0 } else { event };
        }
        id
    } else {
        let nonce = st.nonce;
        st.nonce += 1;
        let mut w = Wr::default();
        e.encode(&mut w);
        w.u32(nonce);
        if !st.upstream(T_CREATE, &w.0) {
            return DPERR_SESSIONLOST;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(id) = st.created.remove(&nonce) {
                break id;
            }
            let now = Instant::now();
            if now >= deadline || st.lost {
                return DPERR_TIMEOUT;
            }
            st = inner
                .cv
                .wait_timeout(st, deadline - now)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    };
    if !group {
        if let Some(x) = st.ents.get_mut(&id) {
            x.event = event;
        }
    }
    wr32(out, id);
    DP_OK
}

unsafe extern "win64" fn dp_create_group(
    this: usize,
    out: usize,
    name: usize,
    data: usize,
    size: u32,
    flags: u32,
) -> i32 {
    create_entity(this, out, name, 0, data, size, flags, true, 0)
}
unsafe extern "win64" fn dp_create_group_in_group(
    this: usize,
    parent: u32,
    out: usize,
    name: usize,
    data: usize,
    size: u32,
    flags: u32,
) -> i32 {
    create_entity(this, out, name, 0, data, size, flags, true, parent)
}
unsafe extern "win64" fn dp_create_player(
    this: usize,
    out: usize,
    name: usize,
    event: usize,
    data: usize,
    size: u32,
    flags: u32,
) -> i32 {
    create_entity(this, out, name, event, data, size, flags, false, 0)
}

unsafe fn destroy_entity(this: usize, id: u32, group: bool) -> i32 {
    let inner = inner(this);
    let mut st = lock(&inner.st);
    let ok = st
        .ents
        .get(&id)
        .is_some_and(|e| e.group == group && e.owner == st.me);
    if !ok {
        return if group {
            DPERR_INVALIDGROUP
        } else {
            DPERR_INVALIDPLAYER
        };
    }
    if st.host {
        st.host_del(id);
    } else {
        let mut w = Wr::default();
        w.u32(id);
        st.upstream(T_DESTROY, &w.0);
        st.apply_del(id);
    }
    DP_OK
}
unsafe extern "win64" fn dp_destroy_group(this: usize, id: u32) -> i32 {
    destroy_entity(this, id, true)
}
unsafe extern "win64" fn dp_destroy_player(this: usize, id: u32) -> i32 {
    destroy_entity(this, id, false)
}

/// Rappel EnumPlayersCallback2(DPID, type, DPNAME*, flags, ctx).
unsafe fn enum_entities(this: usize, list: Vec<Entity>, cb: usize, ctx: usize) -> i32 {
    if cb == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let a = ansi(this);
    let me = lock(&inner(this).st).me;
    for e in list {
        let kind = if e.group {
            DPPLAYERTYPE_GROUP
        } else {
            DPPLAYERTYPE_PLAYER
        };
        let mut flags = e.flags & DPPLAYER_SERVERPLAYER;
        if e.owner == me {
            flags |= DPPLAYER_LOCAL;
        }
        let go = with_blob(Some(name_blob(&e, a)), |name| {
            call_back(
                cb,
                &[e.id as usize, kind as usize, name, flags as usize, ctx],
            )
        });
        if go as u32 == 0 {
            break;
        }
    }
    DP_OK
}

fn filter_players(st: &State, flags: u32) -> impl Fn(&Entity) -> bool + '_ {
    move |e: &Entity| {
        let local = e.owner == st.me;
        !(flags & DPENUMPLAYERS_LOCAL != 0 && !local || flags & DPENUMPLAYERS_REMOTE != 0 && local)
    }
}

unsafe extern "win64" fn dp_enum_group_players(
    this: usize,
    group: u32,
    _instance: usize,
    cb: usize,
    ctx: usize,
    flags: u32,
) -> i32 {
    let list = {
        let inner = inner(this);
        let st = lock(&inner.st);
        let Some(g) = st.ents.get(&group).filter(|e| e.group) else {
            return DPERR_INVALIDGROUP;
        };
        let keep = filter_players(&st, flags);
        g.members
            .iter()
            .filter_map(|m| st.ents.get(m))
            .filter(|e| keep(e))
            .cloned()
            .collect()
    };
    enum_entities(this, list, cb, ctx)
}
unsafe fn list_kind(
    this: usize,
    group: bool,
    parent: Option<u32>,
    flags: u32,
) -> Result<Vec<Entity>, i32> {
    if flags & DPENUMPLAYERS_SESSION != 0 {
        // Joueurs d'une partie à laquelle on n'est pas connecté.
        return Err(DPERR_UNSUPPORTED);
    }
    let inner = inner(this);
    let st = lock(&inner.st);
    if st.session.is_none() {
        return Err(DPERR_NOCONNECTION);
    }
    let keep = filter_players(&st, flags);
    Ok(st
        .ents
        .values()
        .filter(|e| e.group == group && keep(e))
        .filter(|e| match parent {
            Some(p) => e.parent == p,
            None => !e.group || e.parent == 0 || flags & DPENUMGROUPS_SHORTCUT != 0,
        })
        .cloned()
        .collect())
}
unsafe extern "win64" fn dp_enum_groups(
    this: usize,
    _instance: usize,
    cb: usize,
    ctx: usize,
    flags: u32,
) -> i32 {
    match list_kind(this, true, None, flags) {
        Ok(l) => enum_entities(this, l, cb, ctx),
        Err(hr) => hr,
    }
}
unsafe extern "win64" fn dp_enum_groups_in_group(
    this: usize,
    group: u32,
    _instance: usize,
    cb: usize,
    ctx: usize,
    flags: u32,
) -> i32 {
    match list_kind(this, true, Some(group), flags) {
        Ok(l) => enum_entities(this, l, cb, ctx),
        Err(hr) => hr,
    }
}
unsafe extern "win64" fn dp_enum_players(
    this: usize,
    _instance: usize,
    cb: usize,
    ctx: usize,
    flags: u32,
) -> i32 {
    match list_kind(this, false, None, flags) {
        Ok(l) => enum_entities(this, l, cb, ctx),
        Err(hr) => hr,
    }
}

unsafe extern "win64" fn dp_enum_sessions(
    this: usize,
    desc: usize,
    timeout: u32,
    cb: usize,
    ctx: usize,
    flags: u32,
) -> i32 {
    let inner = inner(this);
    if flags & DPENUMSESSIONS_STOPASYNC != 0 {
        stop_async_enum(&inner);
        return DP_OK;
    }
    if desc == 0 || cb == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let a = ansi(this);
    let want = read_session(desc, a);
    let (ready, port, inet) = {
        let st = lock(&inner.st);
        (st.ready, st.port, st.inet.clone())
    };
    if !ready {
        return DPERR_UNINITIALIZED;
    }
    let wait = Duration::from_millis(if timeout == 0 { 1000 } else { timeout as u64 }.min(10_000));
    let found = if flags & DPENUMSESSIONS_ASYNC != 0 {
        let running = lock(&inner.st).enum_thread.is_some();
        if !running {
            let stop = Arc::new(AtomicBool::new(false));
            let (i2, s2, app) = (inner.clone(), stop.clone(), want.application);
            let inet2 = inet.clone();
            let t = std::thread::spawn(move || {
                while !s2.load(Ordering::Acquire) {
                    let f = discover(port, inet2.as_deref(), &app, Duration::from_millis(800));
                    lock(&i2.st).found = f;
                }
            });
            lock(&inner.st).enum_thread = Some((stop, t));
        }
        lock(&inner.st).found.clone()
    } else {
        let f = discover(port, inet.as_deref(), &want.application, wait);
        lock(&inner.st).found = f.clone();
        f
    };
    let mut any = false;
    for f in found {
        let s = &f.session;
        if want.application != [0; 16] && s.application != want.application {
            continue;
        }
        if flags & DPENUMSESSIONS_AVAILABLE != 0 {
            let closed = s.flags & (DPSESSION_NEWPLAYERSDISABLED | DPSESSION_JOINDISABLED) != 0
                || (s.max_players != 0 && s.current >= s.max_players);
            let locked = s.flags & DPSESSION_PASSWORDREQUIRED != 0
                && flags & DPENUMSESSIONS_PASSWORDREQUIRED == 0;
            if closed || locked {
                continue;
            }
        }
        any = true;
        let mut t = Box::new(timeout);
        let go = with_blob(Some(session_blob(s, a)), |p| {
            call_back(cb, &[p, &mut *t as *mut u32 as usize, 0, ctx])
        });
        if go as u32 == 0 {
            return DP_OK;
        }
    }
    let mut t = Box::new(timeout);
    call_back(
        cb,
        &[
            0,
            &mut *t as *mut u32 as usize,
            DPESC_TIMEDOUT as usize,
            ctx,
        ],
    );
    let _ = any;
    DP_OK
}

unsafe fn fill_caps(this: usize, caps: usize) -> i32 {
    if caps == 0 || rd32(caps) < 40 {
        return DPERR_INVALIDPARAMS;
    }
    let host = lock(&inner(this).st).host;
    let v = [
        40,
        if host { DPCAPS_ISHOST } else { 0 },
        65_000,
        0,
        0xFFFF,
        1_000_000,
        20,
        0xFFFF,
        0,
        500,
    ];
    for (i, x) in v.iter().enumerate() {
        wr32(caps + 4 * i, *x);
    }
    DP_OK
}
unsafe extern "win64" fn dp_get_caps(this: usize, caps: usize, _flags: u32) -> i32 {
    fill_caps(this, caps)
}
unsafe extern "win64" fn dp_get_player_caps(this: usize, id: u32, caps: usize, _f: u32) -> i32 {
    if !lock(&inner(this).st).ents.contains_key(&id) {
        return DPERR_INVALIDPLAYER;
    }
    fill_caps(this, caps)
}

unsafe fn get_data(this: usize, id: u32, data: usize, size: usize, group: bool) -> i32 {
    let e = lock(&inner(this).st).ents.get(&id).cloned();
    let Some(e) = e.filter(|e| e.group == group) else {
        return if group {
            DPERR_INVALIDGROUP
        } else {
            DPERR_INVALIDPLAYER
        };
    };
    let mut b = Blob::new(0);
    b.b = e.data;
    give(b, data, size)
}
unsafe fn get_name(this: usize, id: u32, data: usize, size: usize, group: bool) -> i32 {
    let e = lock(&inner(this).st).ents.get(&id).cloned();
    let Some(e) = e.filter(|e| e.group == group) else {
        return if group {
            DPERR_INVALIDGROUP
        } else {
            DPERR_INVALIDPLAYER
        };
    };
    give(name_blob(&e, ansi(this)), data, size)
}
unsafe extern "win64" fn dp_get_group_data(
    this: usize,
    id: u32,
    data: usize,
    size: usize,
    _f: u32,
) -> i32 {
    get_data(this, id, data, size, true)
}
unsafe extern "win64" fn dp_get_group_name(this: usize, id: u32, data: usize, size: usize) -> i32 {
    get_name(this, id, data, size, true)
}
unsafe extern "win64" fn dp_get_player_data(
    this: usize,
    id: u32,
    data: usize,
    size: usize,
    _f: u32,
) -> i32 {
    get_data(this, id, data, size, false)
}
unsafe extern "win64" fn dp_get_player_name(this: usize, id: u32, data: usize, size: usize) -> i32 {
    get_name(this, id, data, size, false)
}

unsafe fn set_field(this: usize, id: u32, group: bool, f: impl FnOnce(&mut Entity)) -> i32 {
    let inner = inner(this);
    let mut st = lock(&inner.st);
    match st.ents.get_mut(&id).filter(|e| e.group == group) {
        Some(e) => {
            f(e);
            DP_OK
        }
        None if group => DPERR_INVALIDGROUP,
        None => DPERR_INVALIDPLAYER,
    }
}
unsafe fn bytes_of(data: usize, size: u32) -> Vec<u8> {
    if data == 0 || size == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(data as *const u8, size as usize).to_vec()
    }
}
unsafe extern "win64" fn dp_set_group_data(
    this: usize,
    id: u32,
    data: usize,
    size: u32,
    _f: u32,
) -> i32 {
    let d = bytes_of(data, size);
    set_field(this, id, true, |e| e.data = d)
}
unsafe extern "win64" fn dp_set_player_data(
    this: usize,
    id: u32,
    data: usize,
    size: u32,
    _f: u32,
) -> i32 {
    let d = bytes_of(data, size);
    set_field(this, id, false, |e| e.data = d)
}
unsafe extern "win64" fn dp_set_group_name(this: usize, id: u32, name: usize, _f: u32) -> i32 {
    let (s, l) = read_name(name, ansi(this));
    set_field(this, id, true, |e| {
        e.short = s;
        e.long = l;
    })
}
unsafe extern "win64" fn dp_set_player_name(this: usize, id: u32, name: usize, _f: u32) -> i32 {
    let (s, l) = read_name(name, ansi(this));
    set_field(this, id, false, |e| {
        e.short = s;
        e.long = l;
    })
}

unsafe extern "win64" fn dp_get_message_count(this: usize, id: u32, out: usize) -> i32 {
    if out == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let inner = inner(this);
    let st = lock(&inner.st);
    let n = st.queue.iter().filter(|m| id == 0 || m.to == id).count();
    wr32(out, n as u32);
    DP_OK
}
unsafe extern "win64" fn dp_get_message_queue(
    this: usize,
    from: u32,
    to: u32,
    _flags: u32,
    n: usize,
    bytes: usize,
) -> i32 {
    let inner = inner(this);
    let st = lock(&inner.st);
    let list: Vec<&Msg> = st
        .queue
        .iter()
        .filter(|m| (from == 0 || m.from == from) && (to == 0 || m.to == to))
        .collect();
    if n != 0 {
        wr32(n, list.len() as u32);
    }
    if bytes != 0 {
        let total: usize = list
            .iter()
            .map(|m| match &m.body {
                Body::User(d) => d.len(),
                Body::Sys(_) => 0,
            })
            .sum();
        wr32(bytes, total as u32);
    }
    DP_OK
}

unsafe extern "win64" fn dp_get_session_desc(this: usize, data: usize, size: usize) -> i32 {
    let s = {
        let inner = inner(this);
        let st = lock(&inner.st);
        st.session.clone().map(|mut s| {
            s.current = st.player_count();
            s
        })
    };
    let Some(s) = s else {
        return DPERR_NOCONNECTION;
    };
    give(session_blob(&s, ansi(this)), data, size)
}
unsafe extern "win64" fn dp_set_session_desc(this: usize, desc: usize, _f: u32) -> i32 {
    if desc == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let inner = inner(this);
    let mut st = lock(&inner.st);
    if !st.host {
        return DPERR_UNAVAILABLE;
    }
    let Some(cur) = st.session.clone() else {
        return DPERR_NOCONNECTION;
    };
    let mut s = read_session(desc, ansi(this));
    s.instance = cur.instance;
    s.application = cur.application;
    s.current = cur.current;
    let mut w = Wr::default();
    s.encode(&mut w, false);
    st.broadcast(T_SESSION, &w.0, u32::MAX);
    st.session = Some(s);
    DP_OK
}

unsafe extern "win64" fn dp_initialize(_this: usize, _guid: usize) -> i32 {
    DPERR_ALREADYINITIALIZED
}

unsafe fn open(this: usize, desc: usize, flags: u32) -> i32 {
    if desc == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let a = ansi(this);
    let want = read_session(desc, a);
    let inner = inner(this);
    {
        let st = lock(&inner.st);
        if !st.ready {
            return DPERR_UNINITIALIZED;
        }
        if st.session.is_some() {
            return DPERR_ALREADYINITIALIZED;
        }
    }
    if flags & DPOPEN_CREATE != 0 {
        open_create(&inner, want)
    } else if flags & DPOPEN_JOIN != 0 {
        open_join(&inner, want)
    } else {
        DPERR_INVALIDFLAGS
    }
}

fn random_guid() -> Guid {
    let mut g = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut g);
    }
    if g == [0; 16] {
        let t = Instant::now().elapsed().as_nanos() ^ std::process::id() as u128;
        g = t.to_le_bytes();
    }
    g[7] = (g[7] & 0x0f) | 0x40;
    g[8] = (g[8] & 0x3f) | 0x80;
    g
}

fn open_create(inner: &Arc<Inner>, mut s: Session) -> i32 {
    let Ok(listener) = TcpListener::bind("0.0.0.0:0") else {
        return DPERR_CANTCREATESESSION;
    };
    let tcp_port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let port = lock(&inner.st).port;
    s.instance = random_guid();
    s.current = 0;
    let stop = Arc::new(AtomicBool::new(false));
    let mut threads = Vec::new();
    match UdpSocket::bind(("0.0.0.0", port)) {
        Ok(sock) => {
            let (i, st) = (inner.clone(), stop.clone());
            threads.push(std::thread::spawn(move || {
                announce_loop(i, sock, st, tcp_port)
            }));
        }
        Err(e) => crate::log_warn!(
            "dplayx",
            "port d'annonce {port} indisponible ({e}) : la partie ne sera trouvée que par son adresse"
        ),
    }
    {
        let (i, st) = (inner.clone(), stop.clone());
        threads.push(std::thread::spawn(move || accept_loop(i, listener, st)));
    }
    let mut st = lock(&inner.st);
    st.session = Some(s);
    st.host = true;
    st.me = 0;
    st.lost = false;
    st.stop = stop;
    st.threads = threads;
    DP_OK
}

fn open_join(inner: &Arc<Inner>, want: Session) -> i32 {
    let (mut found, port, inet) = {
        let st = lock(&inner.st);
        (st.found.clone(), st.port, st.inet.clone())
    };
    if !found.iter().any(|f| f.session.instance == want.instance) {
        found = discover(port, inet.as_deref(), &[0; 16], Duration::from_millis(1000));
    }
    let Some(f) = found
        .into_iter()
        .find(|f| f.session.instance == want.instance)
    else {
        return DPERR_NOSESSIONS;
    };
    let Ok(mut s) = TcpStream::connect_timeout(&f.addr, Duration::from_secs(5)) else {
        return DPERR_NOCONNECTION;
    };
    let _ = s.set_nodelay(true);
    let mut w = Wr::default();
    w.str(&want.password);
    if !send_frame(&mut s, T_JOIN, &w.0) {
        return DPERR_NOCONNECTION;
    }
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let reply = read_frame(&mut s);
    let _ = s.set_read_timeout(None);
    let body = match reply {
        Some((T_WELCOME, b)) => b,
        Some((T_REFUSED, b)) => {
            return Rd::new(&b)
                .u32()
                .map(|v| v as i32)
                .unwrap_or(DPERR_NOCONNECTION)
        }
        _ => return DPERR_TIMEOUT,
    };
    let mut r = Rd::new(&body);
    let (Some(me), Some(session), Some(n)) = (r.u32(), Session::decode(&mut r), r.u32()) else {
        return DPERR_NOCONNECTION;
    };
    let mut ents = BTreeMap::new();
    for _ in 0..n {
        match Entity::decode(&mut r) {
            Some(e) => {
                ents.insert(e.id, e);
            }
            None => return DPERR_NOCONNECTION,
        }
    }
    let Ok(rs) = s.try_clone() else {
        return DPERR_NOCONNECTION;
    };
    let stop = Arc::new(AtomicBool::new(false));
    let t = {
        let (i, st) = (inner.clone(), stop.clone());
        std::thread::spawn(move || client_reader(i, rs, st))
    };
    let mut st = lock(&inner.st);
    st.session = Some(session);
    st.host = false;
    st.me = me;
    st.lost = false;
    st.ents = ents;
    st.upstream = Some(s);
    st.stop = stop;
    st.threads = vec![t];
    DP_OK
}

unsafe extern "win64" fn dp_open(this: usize, desc: usize, flags: u32) -> i32 {
    open(this, desc, flags)
}
unsafe extern "win64" fn dp_secure_open(
    this: usize,
    desc: usize,
    flags: u32,
    _security: usize,
    _credentials: usize,
) -> i32 {
    open(this, desc, flags)
}

unsafe extern "win64" fn dp_receive(
    this: usize,
    from: usize,
    to: usize,
    flags: u32,
    data: usize,
    size: usize,
) -> i32 {
    if size == 0 || from == 0 || to == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let a = ansi(this);
    let inner = inner(this);
    let mut st = lock(&inner.st);
    let want_from = rd32(from);
    let want_to = rd32(to);
    let pos = st.queue.iter().position(|m| {
        (flags & DPRECEIVE_TOPLAYER == 0 || m.to == want_to)
            && (flags & DPRECEIVE_FROMPLAYER == 0 || m.from == want_from)
    });
    let Some(pos) = pos else {
        return DPERR_NOMESSAGES;
    };
    let m = st.queue[pos].clone();
    let blob = match &m.body {
        Body::User(d) => {
            let mut b = Blob::new(0);
            b.b = d.clone();
            b
        }
        Body::Sys(sys) => {
            let mut b = sys_blob(sys, a);
            if matches!(sys, Sys::Create(_)) {
                b.u32_at(12, st.player_count());
            }
            b
        }
    };
    let hr = give(blob, data, size);
    if hr != DP_OK {
        return hr;
    }
    wr32(from, m.from);
    wr32(to, m.to);
    if flags & DPRECEIVE_PEEK == 0 {
        st.queue.remove(pos);
    }
    DP_OK
}

unsafe fn send(this: usize, from: u32, to: u32, data: usize, size: u32) -> i32 {
    if data == 0 || size == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let bytes = bytes_of(data, size);
    let inner = inner(this);
    let mut st = lock(&inner.st);
    if st.session.is_none() {
        return DPERR_NOCONNECTION;
    }
    if st.lost {
        return DPERR_SESSIONLOST;
    }
    if !st
        .ents
        .get(&from)
        .is_some_and(|e| !e.group && e.owner == st.me)
    {
        return DPERR_INVALIDPLAYER;
    }
    if to != DPID_ALLPLAYERS && !st.ents.contains_key(&to) {
        return DPERR_INVALIDPLAYER;
    }
    if st.host {
        st.host_route(from, to, &bytes);
    } else {
        let mut w = Wr::default();
        w.u32(from).u32(to).bytes(&bytes);
        if !st.upstream(T_SEND, &w.0) {
            return DPERR_SESSIONLOST;
        }
    }
    DP_OK
}
unsafe extern "win64" fn dp_send(
    this: usize,
    from: u32,
    to: u32,
    _flags: u32,
    data: usize,
    size: u32,
) -> i32 {
    send(this, from, to, data, size)
}
unsafe extern "win64" fn dp_send_ex(
    this: usize,
    from: u32,
    to: u32,
    _flags: u32,
    data: usize,
    size: u32,
    _priority: u32,
    _timeout: u32,
    _context: usize,
    msg_id: usize,
) -> i32 {
    if msg_id != 0 {
        wr32(msg_id, 0);
    }
    send(this, from, to, data, size)
}

/// Éléments d'une adresse DirectPlay (DPADDRESS + données).
unsafe fn address_elements(p: usize, size: u32) -> Vec<(Guid, usize, u32)> {
    let mut v = Vec::new();
    let mut off = 0usize;
    while off + 20 <= size as usize {
        let mut g = [0u8; 16];
        g.copy_from_slice(std::slice::from_raw_parts((p + off) as *const u8, 16));
        let n = rd32(p + off + 16);
        if off + 20 + n as usize > size as usize {
            break;
        }
        v.push((g, p + off + 20, n));
        off += 20 + n as usize;
    }
    v
}

unsafe extern "win64" fn dp_enum_connections(
    this: usize,
    _app: usize,
    cb: usize,
    ctx: usize,
    flags: u32,
) -> i32 {
    if cb == 0 {
        return DPERR_INVALIDPARAMS;
    }
    if flags != 0 && flags & DPCONNECTION_DIRECTPLAY == 0 {
        return DP_OK;
    }
    let a = ansi(this);
    let conn = build_address(&[(DPAID_SERVICEPROVIDER, DPSPGUID_TCPIP.to_vec())]);
    let mut heap = Vec::with_capacity(16 + conn.len());
    heap.extend_from_slice(&DPSPGUID_TCPIP);
    heap.extend_from_slice(&conn);
    let base = heap.as_ptr() as usize;
    let e = Entity {
        short: sp_name().into(),
        long: sp_name().into(),
        ..Default::default()
    };
    with_blob(Some(name_blob(&e, a)), |name| {
        call_back(
            cb,
            &[
                base,
                base + 16,
                conn.len(),
                name,
                DPCONNECTION_DIRECTPLAY as usize,
                ctx,
            ],
        )
    });
    DP_OK
}

unsafe extern "win64" fn dp_initialize_connection(this: usize, conn: usize, _flags: u32) -> i32 {
    if conn == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let inner = inner(this);
    if lock(&inner.st).ready {
        return DPERR_ALREADYINITIALIZED;
    }
    // La taille totale est le premier élément ; sinon lecture prudente.
    let total = if address_elements(conn, 24)
        .first()
        .is_some_and(|e| e.0 == DPAID_TOTALSIZE && e.2 == 4)
    {
        rd32(conn + 20)
    } else {
        1024
    };
    let mut sp = None;
    let mut host = None;
    let mut port = None;
    for (g, p, n) in address_elements(conn, total) {
        if g == DPAID_SERVICEPROVIDER && n == 16 {
            let mut x = [0u8; 16];
            x.copy_from_slice(std::slice::from_raw_parts(p as *const u8, 16));
            sp = Some(x);
        } else if g == DPAID_INET && n > 0 {
            host = Some(super::cstr(p as *const u8));
        } else if g == DPAID_INETW && n > 1 {
            host = Some(super::wstr(p as *const u16));
        } else if g == DPAID_INETPORT && n >= 2 {
            port = Some((p as *const u16).read_unaligned());
        }
    }
    if sp != Some(DPSPGUID_TCPIP) {
        return DPERR_UNAVAILABLE;
    }
    let mut st = lock(&inner.st);
    st.ready = true;
    st.inet = host.map(|h| match port {
        Some(p) if p != 0 => format!("{h}:{p}"),
        _ => h,
    });
    DP_OK
}

unsafe extern "win64" fn dp_get_flags(this: usize, id: u32, out: usize) -> i32 {
    if out == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let inner = inner(this);
    let st = lock(&inner.st);
    let Some(e) = st.ents.get(&id) else {
        return DPERR_INVALIDPLAYER;
    };
    let mut f = e.flags & DPPLAYER_SERVERPLAYER;
    if e.owner == st.me {
        f |= DPPLAYER_LOCAL;
    }
    wr32(out, f);
    DP_OK
}
unsafe extern "win64" fn dp_get_group_parent(this: usize, id: u32, out: usize) -> i32 {
    if out == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let inner = inner(this);
    let st = lock(&inner.st);
    match st.ents.get(&id).filter(|e| e.group) {
        Some(e) => {
            wr32(out, e.parent);
            DP_OK
        }
        None => DPERR_INVALIDGROUP,
    }
}

unsafe extern "win64" fn dp_unsupported0(_this: usize) -> i32 {
    DPERR_UNSUPPORTED
}
unsafe extern "win64" fn dp_unsupported2(_this: usize, _a: usize, _b: usize) -> i32 {
    DPERR_UNSUPPORTED
}
unsafe extern "win64" fn dp_unsupported3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    DPERR_UNSUPPORTED
}
unsafe extern "win64" fn dp_unsupported4(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
) -> i32 {
    DPERR_UNSUPPORTED
}
unsafe extern "win64" fn dp_ok2(_this: usize, _a: usize, _b: usize) -> i32 {
    DP_OK
}
unsafe extern "win64" fn dp_ok3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    DP_OK
}
unsafe extern "win64" fn dp_not_lobbied4(
    _this: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
) -> i32 {
    DPERR_NOTLOBBIED
}

fn f(p: *const ()) -> usize {
    p as usize
}

fn dp_methods() -> Vec<(usize, &'static str)> {
    vec![
        (f(dp_qi as *const ()), "ii"),
        (f(dp_addref as *const ()), ""),
        (f(dp_release as *const ()), ""),
        // IDirectPlay2
        (f(dp_add_player_to_group as *const ()), "ii"),
        (f(dp_close as *const ()), ""),
        (f(dp_create_group as *const ()), "iiiii"),
        (f(dp_create_player as *const ()), "iiiiii"),
        (f(dp_delete_player_from_group as *const ()), "ii"),
        (f(dp_destroy_group as *const ()), "i"),
        (f(dp_destroy_player as *const ()), "i"),
        (f(dp_enum_group_players as *const ()), "iiiii"),
        (f(dp_enum_groups as *const ()), "iiii"),
        (f(dp_enum_players as *const ()), "iiii"),
        (f(dp_enum_sessions as *const ()), "iiiii"),
        (f(dp_get_caps as *const ()), "ii"),
        (f(dp_get_group_data as *const ()), "iiii"),
        (f(dp_get_group_name as *const ()), "iii"),
        (f(dp_get_message_count as *const ()), "ii"),
        (f(dp_unsupported3 as *const ()), "iii"), // GetPlayerAddress
        (f(dp_get_player_caps as *const ()), "iii"),
        (f(dp_get_player_data as *const ()), "iiii"),
        (f(dp_get_player_name as *const ()), "iii"),
        (f(dp_get_session_desc as *const ()), "ii"),
        (f(dp_initialize as *const ()), "i"),
        (f(dp_open as *const ()), "ii"),
        (f(dp_receive as *const ()), "iiiii"),
        (f(dp_send as *const ()), "iiiii"),
        (f(dp_set_group_data as *const ()), "iiii"),
        (f(dp_set_group_name as *const ()), "iii"),
        (f(dp_set_player_data as *const ()), "iiii"),
        (f(dp_set_player_name as *const ()), "iii"),
        (f(dp_set_session_desc as *const ()), "ii"),
        // IDirectPlay3
        (f(dp_unsupported2 as *const ()), "ii"), // AddGroupToGroup
        (f(dp_create_group_in_group as *const ()), "iiiiii"),
        (f(dp_unsupported2 as *const ()), "ii"), // DeleteGroupFromGroup
        (f(dp_enum_connections as *const ()), "iiii"),
        (f(dp_enum_groups_in_group as *const ()), "iiiii"),
        (f(dp_not_lobbied4 as *const ()), "iiii"), // GetGroupConnectionSettings
        (f(dp_initialize_connection as *const ()), "ii"),
        (f(dp_secure_open as *const ()), "iiii"),
        (f(dp_unsupported4 as *const ()), "iiii"), // SendChatMessage
        (f(dp_unsupported3 as *const ()), "iii"),  // SetGroupConnectionSettings
        (f(dp_unsupported2 as *const ()), "ii"),   // StartSession
        (f(dp_get_flags as *const ()), "ii"),      // GetGroupFlags
        (f(dp_get_group_parent as *const ()), "ii"),
        (f(dp_unsupported4 as *const ()), "iiii"), // GetPlayerAccount
        (f(dp_get_flags as *const ()), "ii"),      // GetPlayerFlags
        // IDirectPlay4
        (f(dp_unsupported2 as *const ()), "ii"), // GetGroupOwner
        (f(dp_unsupported2 as *const ()), "ii"), // SetGroupOwner
        (f(dp_send_ex as *const ()), "iiiiiiiii"),
        (f(dp_get_message_queue as *const ()), "iiiii"),
        (f(dp_ok2 as *const ()), "ii"),  // CancelMessage
        (f(dp_ok3 as *const ()), "iii"), // CancelPriority
    ]
}

fn dp_vtable(a: bool) -> usize {
    vtable_sig(
        if a { "IDirectPlay4A" } else { "IDirectPlay4" },
        &dp_methods(),
    )
}

/// IDirectPlay (DirectX 3) : seul QueryInterface sert, vers IDirectPlay2+.
fn dp1_vtable() -> usize {
    let mut m: Vec<(usize, &'static str)> = dp_methods()[..3].to_vec();
    let sigs = [
        "ii", "", "iiiii", "iiiii", "ii", "i", "i", "i", "iiiii", "iiii", "iiii", "iiiii", "ii",
        "ii", "iii", "iiii", "i", "ii", "iiiii", "i", "iiiii", "iii",
    ];
    for s in sigs {
        let p = match s.len() {
            0 | 1 => dp_unsupported0 as *const (),
            2 => dp_unsupported2 as *const (),
            3 => dp_unsupported3 as *const (),
            _ => dp_unsupported4 as *const (),
        };
        // Les méthodes à plus de 4 arguments rendent aussi DPERR_UNSUPPORTED :
        // la passerelle x86 libère la pile d'après la signature.
        m.push((f(p), s));
    }
    vtable_sig("IDirectPlay", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// IDirectPlayLobby : adresses et fournisseur TCP/IP
// ═══════════════════════════════════════════════════════════════════════

fn build_address(parts: &[(Guid, Vec<u8>)]) -> Vec<u8> {
    let total: usize = 24 + parts.iter().map(|(_, d)| 20 + d.len()).sum::<usize>();
    let mut v = Vec::with_capacity(total);
    v.extend_from_slice(&DPAID_TOTALSIZE);
    v.extend_from_slice(&4u32.to_le_bytes());
    v.extend_from_slice(&(total as u32).to_le_bytes());
    for (g, d) in parts {
        v.extend_from_slice(g);
        v.extend_from_slice(&(d.len() as u32).to_le_bytes());
        v.extend_from_slice(d);
    }
    v
}

#[repr(C)]
struct LFace {
    vtbl: usize,
    obj: *const Lobby,
}
#[repr(C)]
struct Lobby {
    faces: [LFace; 2],
    refs: AtomicU32,
}

unsafe fn lobby<'a>(this: usize) -> &'a Lobby {
    &*(*(this as *const LFace)).obj
}

fn new_lobby(a: bool) -> usize {
    let tables = [lobby_vtable(false), lobby_vtable(true)];
    let o = Box::into_raw(Box::new(Lobby {
        faces: tables.map(|vtbl| LFace {
            vtbl,
            obj: std::ptr::null(),
        }),
        refs: AtomicU32::new(1),
    }));
    unsafe {
        for f in (*o).faces.iter_mut() {
            f.obj = o;
        }
        &(*o).faces[a as usize] as *const LFace as usize
    }
}

unsafe extern "win64" fn dpl_qi(this: usize, iid: *const Guid, out: usize) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    put_ptr(out, 0);
    if iid.is_null() {
        return DPERR_INVALIDPARAMS;
    }
    let o = lobby(this);
    let iid = *iid;
    let face = if iid == IID_IUNKNOWN || LOBBY_IIDS_W.contains(&iid) {
        0
    } else if LOBBY_IIDS_A.contains(&iid) {
        1
    } else {
        return E_NOINTERFACE;
    };
    o.refs.fetch_add(1, Ordering::Relaxed);
    put_ptr(out, &o.faces[face] as *const LFace as usize);
    DP_OK
}
unsafe extern "win64" fn dpl_addref(this: usize) -> u32 {
    lobby(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn dpl_release(this: usize) -> u32 {
    let o = lobby(this);
    let n = o.refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(o as *const Lobby as *mut Lobby));
    }
    n
}

unsafe fn give_bytes(v: Vec<u8>, out: usize, size: usize) -> i32 {
    let mut b = Blob::new(0);
    b.b = v;
    give(b, out, size)
}

unsafe extern "win64" fn dpl_create_address(
    _this: usize,
    sp: *const Guid,
    kind: *const Guid,
    data: usize,
    size: u32,
    out: usize,
    out_size: usize,
) -> i32 {
    if sp.is_null() || kind.is_null() || (size > 0 && data == 0) {
        return DPERR_INVALIDPARAMS;
    }
    let addr = build_address(&[
        (DPAID_SERVICEPROVIDER, (*sp).to_vec()),
        (*kind, bytes_of(data, size)),
    ]);
    give_bytes(addr, out, out_size)
}

unsafe extern "win64" fn dpl_create_compound_address(
    _this: usize,
    elems: usize,
    count: u32,
    out: usize,
    out_size: usize,
) -> i32 {
    if elems == 0 || count == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let ps = ptr_size();
    let stride = align(20, ps) + ps;
    let mut parts = Vec::new();
    for i in 0..count as usize {
        let e = elems + i * stride;
        let mut g = [0u8; 16];
        g.copy_from_slice(std::slice::from_raw_parts(e as *const u8, 16));
        let n = rd32(e + 16);
        let p = get_ptr(e + align(20, ps));
        if g != DPAID_TOTALSIZE {
            parts.push((g, bytes_of(p, n)));
        }
    }
    give_bytes(build_address(&parts), out, out_size)
}

unsafe extern "win64" fn dpl_enum_address(
    _this: usize,
    cb: usize,
    addr: usize,
    size: u32,
    ctx: usize,
) -> i32 {
    if cb == 0 || addr == 0 {
        return DPERR_INVALIDPARAMS;
    }
    for (g, p, n) in address_elements(addr, size) {
        // Le GUID est dans le tampon du programme : déjà sous 4 Go.
        let _ = g;
        if call_back(cb, &[p - 20, n as usize, p, ctx]) as u32 == 0 {
            break;
        }
    }
    DP_OK
}

unsafe extern "win64" fn dpl_enum_address_types(
    _this: usize,
    cb: usize,
    sp: *const Guid,
    ctx: usize,
    _flags: u32,
) -> i32 {
    if cb == 0 || sp.is_null() {
        return DPERR_INVALIDPARAMS;
    }
    if *sp != DPSPGUID_TCPIP {
        return DPERR_UNAVAILABLE;
    }
    let types: Vec<Guid> = vec![DPAID_INET, DPAID_INETW, DPAID_INETPORT];
    for g in &types {
        if call_back(cb, &[g.as_ptr() as usize, ctx, 0]) as u32 == 0 {
            break;
        }
    }
    DP_OK
}

unsafe extern "win64" fn dpl_none3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    DP_OK
}
unsafe extern "win64" fn dpl_not_lobbied3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    DPERR_NOTLOBBIED
}
unsafe extern "win64" fn dpl_receive(
    _this: usize,
    _flags: u32,
    _app: u32,
    _msg_flags: usize,
    _data: usize,
    _size: usize,
) -> i32 {
    DPERR_NOMESSAGES
}
unsafe extern "win64" fn dpl_ok2(_this: usize, _a: usize, _b: usize) -> i32 {
    DP_OK
}
unsafe extern "win64" fn dpl_ok1(_this: usize, _a: usize) -> i32 {
    DP_OK
}

fn lobby_vtable(a: bool) -> usize {
    let m: Vec<(usize, &'static str)> = vec![
        (f(dpl_qi as *const ()), "ii"),
        (f(dpl_addref as *const ()), ""),
        (f(dpl_release as *const ()), ""),
        (f(dpl_not_lobbied3 as *const ()), "iii"), // Connect
        (f(dpl_create_address as *const ()), "iiiiii"),
        (f(dpl_enum_address as *const ()), "iiii"),
        (f(dpl_enum_address_types as *const ()), "iiii"),
        (f(dpl_none3 as *const ()), "iii"), // EnumLocalApplications
        (f(dpl_not_lobbied3 as *const ()), "iii"), // GetConnectionSettings
        (f(dpl_receive as *const ()), "iiiii"),
        (f(dp_unsupported4 as *const ()), "iiii"), // RunApplication
        (f(dp_not_lobbied4 as *const ()), "iiii"), // SendLobbyMessage
        (f(dpl_not_lobbied3 as *const ()), "iii"), // SetConnectionSettings
        (f(dpl_none3 as *const ()), "iii"),        // SetLobbyMessageEvent
        (f(dpl_create_compound_address as *const ()), "iiii"),
        (f(dp_not_lobbied4 as *const ()), "iiii"), // ConnectEx
        (f(dpl_ok2 as *const ()), "ii"),           // RegisterApplication
        (f(dpl_ok2 as *const ()), "ii"),           // UnregisterApplication
        (f(dpl_ok1 as *const ()), "i"),            // WaitForConnectionSettings
    ];
    vtable_sig(
        if a {
            "IDirectPlayLobby3A"
        } else {
            "IDirectPlayLobby3"
        },
        &m,
    )
}

// ═══════════════════════════════════════════════════════════════════════
// Exports et CoCreateInstance
// ═══════════════════════════════════════════════════════════════════════

pub unsafe extern "win64" fn direct_play_create(sp: usize, out: usize, outer: usize) -> i32 {
    if out == 0 {
        return DPERR_INVALIDPARAMS;
    }
    put_ptr(out, 0);
    if outer != 0 {
        return 0x8004_0110u32 as i32; // CLASS_E_NOAGGREGATION
    }
    let sp_guid: Guid = if sp == 0 {
        [0; 16]
    } else {
        *(sp as *const Guid)
    };
    if sp_guid != [0; 16] && sp_guid != DPSPGUID_TCPIP {
        return DPERR_UNAVAILABLE;
    }
    put_ptr(out, new_obj(sp_guid == DPSPGUID_TCPIP));
    DP_OK
}

unsafe fn enumerate(cb: usize, ctx: usize, a: bool) -> i32 {
    if cb == 0 {
        return DPERR_INVALIDPARAMS;
    }
    let mut heap = DPSPGUID_TCPIP.to_vec();
    heap.extend_from_slice(&enc_str(sp_name(), a));
    let base = heap.as_ptr() as usize;
    call_back(cb, &[base, base + 16, 6, 0, ctx]);
    DP_OK
}
pub unsafe extern "win64" fn direct_play_enumerate_a(cb: usize, ctx: usize) -> i32 {
    enumerate(cb, ctx, true)
}
pub unsafe extern "win64" fn direct_play_enumerate_w(cb: usize, ctx: usize) -> i32 {
    enumerate(cb, ctx, false)
}

unsafe fn lobby_create(out: usize, outer: usize, a: bool) -> i32 {
    if out == 0 {
        return DPERR_INVALIDPARAMS;
    }
    put_ptr(out, 0);
    if outer != 0 {
        return 0x8004_0110u32 as i32;
    }
    put_ptr(out, new_lobby(a));
    DP_OK
}
pub unsafe extern "win64" fn direct_play_lobby_create_a(
    _sp: usize,
    out: usize,
    outer: usize,
    _data: usize,
    _size: u32,
) -> i32 {
    lobby_create(out, outer, true)
}
pub unsafe extern "win64" fn direct_play_lobby_create_w(
    _sp: usize,
    out: usize,
    outer: usize,
    _data: usize,
    _size: u32,
) -> i32 {
    lobby_create(out, outer, false)
}

/// Classes DirectPlay intégrées (appelé par CoCreateInstance).
///
/// # Safety
/// `iid` et `out` sont valides.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut std::ffi::c_void,
) -> Option<i32> {
    let clsid: &Guid = &*(clsid as *const super::ole32::Guid as *const Guid);
    let iid = iid as *const super::ole32::Guid as *const Guid;
    let obj = if *clsid == CLSID_DIRECTPLAY {
        new_obj(false)
    } else if *clsid == CLSID_DIRECTPLAYLOBBY {
        new_lobby(false)
    } else {
        return None;
    };
    let mut p: usize = 0;
    let slot = &mut p as *mut usize as usize;
    let hr = if *clsid == CLSID_DIRECTPLAY {
        let hr = dp_qi(obj, iid, slot);
        dp_release(obj);
        hr
    } else {
        let hr = dpl_qi(obj, iid, slot);
        dpl_release(obj);
        hr
    };
    *out = p as *mut std::ffi::c_void;
    Some(hr)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DirectPlayCreate" => fn direct_play_create,
        "DirectPlayEnumerate" => fn direct_play_enumerate_a,
        "DirectPlayEnumerateA" => fn direct_play_enumerate_a,
        "DirectPlayEnumerateW" => fn direct_play_enumerate_w,
        "DirectPlayLobbyCreateA" => fn direct_play_lobby_create_a,
        "DirectPlayLobbyCreateW" => fn direct_play_lobby_create_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Qi = unsafe extern "win64" fn(usize, *const Guid, usize) -> i32;

    unsafe fn method(o: usize, i: usize) -> usize {
        crate::dlls::legacy_com::method(o, i)
    }

    fn session_desc(app: Guid, name: &[u16], max: u32) -> Vec<u8> {
        let (size, _) = session_layout();
        let mut d = vec![0u8; size];
        d[..4].copy_from_slice(&(size as u32).to_le_bytes());
        d[24..40].copy_from_slice(&app);
        d[40..44].copy_from_slice(&max.to_le_bytes());
        d[48..56].copy_from_slice(&(name.as_ptr() as u64).to_le_bytes());
        d
    }

    #[test]
    fn adresse_tcpip() {
        let a = build_address(&[
            (DPAID_SERVICEPROVIDER, DPSPGUID_TCPIP.to_vec()),
            (DPAID_INET, b"127.0.0.1\0".to_vec()),
        ]);
        assert_eq!(a.len(), 24 + 36 + 30);
        let e = unsafe { address_elements(a.as_ptr() as usize, a.len() as u32) };
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].0, DPAID_TOTALSIZE);
        assert_eq!(e[2].0, DPAID_INET);
        assert_eq!(e[2].2, 10);
    }

    #[test]
    fn partie_en_reseau_local() {
        unsafe {
            let mut v1 = 0usize;
            assert_eq!(
                direct_play_create(
                    DPSPGUID_TCPIP.as_ptr() as usize,
                    &mut v1 as *mut usize as usize,
                    0
                ),
                DP_OK
            );
            let qi: Qi = std::mem::transmute(method(v1, 0));
            let mut host = 0usize;
            assert_eq!(
                qi(v1, &IID_IDIRECTPLAY4, &mut host as *mut usize as usize),
                0
            );
            dp_release(v1);
            // Port propre au test.
            lock(&inner(host).st).port = 47_000 + (std::process::id() % 900) as u16;
            let port = lock(&inner(host).st).port;

            let app = random_guid();
            let name: Vec<u16> = "Partie\0".encode_utf16().collect();
            let d = session_desc(app, &name, 4);
            assert_eq!(dp_open(host, d.as_ptr() as usize, DPOPEN_CREATE), DP_OK);
            let mut hp = 0u32;
            let short: Vec<u16> = "Hôte\0".encode_utf16().collect();
            let mut dpname = [0u8; 24];
            dpname[..4].copy_from_slice(&24u32.to_le_bytes());
            dpname[8..16].copy_from_slice(&(short.as_ptr() as u64).to_le_bytes());
            assert_eq!(
                dp_create_player(
                    host,
                    &mut hp as *mut u32 as usize,
                    dpname.as_ptr() as usize,
                    0,
                    0,
                    0,
                    0
                ),
                DP_OK
            );

            // Client : InitializeConnection avec une adresse 127.0.0.1.
            let client = {
                let o = new_obj(false);
                let mut c = 0usize;
                dp_qi(o, &IID_IDIRECTPLAY4, &mut c as *mut usize as usize);
                dp_release(o);
                c
            };
            lock(&inner(client).st).port = port;
            let addr = build_address(&[
                (DPAID_SERVICEPROVIDER, DPSPGUID_TCPIP.to_vec()),
                (DPAID_INET, b"127.0.0.1\0".to_vec()),
            ]);
            assert_eq!(
                dp_initialize_connection(client, addr.as_ptr() as usize, 0),
                0
            );
            let found = discover(port, Some("127.0.0.1"), &app, Duration::from_millis(600));
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].session.name, "Partie");
            assert_eq!(found[0].session.current, 1);
            lock(&inner(client).st).found = found.clone();
            let mut j = session_desc(app, &name, 0);
            j[8..24].copy_from_slice(&found[0].session.instance);
            assert_eq!(dp_open(client, j.as_ptr() as usize, DPOPEN_JOIN), DP_OK);
            let mut cp = 0u32;
            assert_eq!(
                dp_create_player(client, &mut cp as *mut u32 as usize, 0, 0, 0, 0, 0),
                DP_OK
            );
            assert!(cp != 0 && cp != hp);
            let msg = b"bonjour";
            assert_eq!(dp_send(client, cp, hp, 0, msg.as_ptr() as usize, 7), DP_OK);

            // L'hôte reçoit la création du joueur puis le message.
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut got = Vec::new();
            while got.len() < 2 && Instant::now() < deadline {
                let (mut from, mut to, mut size) = (0u32, 0u32, 256u32);
                let mut buf = [0u8; 256];
                let hr = dp_receive(
                    host,
                    &mut from as *mut u32 as usize,
                    &mut to as *mut u32 as usize,
                    0,
                    buf.as_mut_ptr() as usize,
                    &mut size as *mut u32 as usize,
                );
                if hr == DPERR_NOMESSAGES {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                assert_eq!(hr, DP_OK);
                got.push((from, to, buf[..size as usize].to_vec()));
            }
            assert_eq!(got.len(), 2);
            assert_eq!(got[0].0, DPID_SYSMSG);
            assert_eq!(&got[0].2[..4], &DPSYS_CREATEPLAYERORGROUP.to_le_bytes());
            assert_eq!(&got[0].2[8..12], &cp.to_le_bytes());
            assert_eq!((got[1].0, got[1].1), (cp, hp));
            assert_eq!(got[1].2, msg);

            // Réponse de l'hôte vers le client.
            assert_eq!(
                dp_send(host, hp, cp, 0, b"salut".as_ptr() as usize, 5),
                DP_OK
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut ok = false;
            while Instant::now() < deadline {
                let (mut from, mut to, mut size) = (0u32, 0u32, 64u32);
                let mut buf = [0u8; 64];
                if dp_receive(
                    client,
                    &mut from as *mut u32 as usize,
                    &mut to as *mut u32 as usize,
                    0,
                    buf.as_mut_ptr() as usize,
                    &mut size as *mut u32 as usize,
                ) == DP_OK
                    && from == hp
                {
                    assert_eq!(&buf[..5], b"salut");
                    ok = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(ok);

            // Départ du client : l'hôte reçoit la destruction du joueur.
            assert_eq!(dp_close(client), DP_OK);
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut destroyed = false;
            while Instant::now() < deadline && !destroyed {
                let (mut from, mut to, mut size) = (0u32, 0u32, 256u32);
                let mut buf = [0u8; 256];
                if dp_receive(
                    host,
                    &mut from as *mut u32 as usize,
                    &mut to as *mut u32 as usize,
                    0,
                    buf.as_mut_ptr() as usize,
                    &mut size as *mut u32 as usize,
                ) == DP_OK
                {
                    destroyed = buf[..4] == DPSYS_DESTROYPLAYERORGROUP.to_le_bytes();
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            assert!(destroyed);
            dp_release(client);
            assert_eq!(dp_close(host), DP_OK);
            dp_release(host);
        }
    }
}
