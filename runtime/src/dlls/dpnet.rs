//! dpnet.dll / dpnaddr.dll : DirectPlay 8 (#863, suite de #836).
//!
//! Écrit d'après la documentation publique de DirectPlay 8 (interfaces
//! IDirectPlay8Peer, IDirectPlay8Client, IDirectPlay8Server et
//! IDirectPlay8Address, messages DPN_MSGID_*). Comme `dplayx.rs` pour
//! DirectPlay 4, le transport est propre à Winlux et non le protocole filaire
//! de Microsoft : les parties sont recherchées en UDP (port de jeu de l'hôte
//! et port 6073, celui de DPNSVR) et le jeu passe en TCP par l'hôte, qui
//! relaie messages, joueurs et groupes. Deux PC Winlux jouent ensemble ; un
//! PC sous Windows ne voit pas ces parties.
//!
//! Les messages sont remis à la fonction du programme par un fil unique par
//! objet, dans l'ordre, comme le font les fils de travail de DirectPlay ;
//! les structures sont écrites dans la disposition de l'ABI du programme.

#![allow(clippy::too_many_arguments)]

use super::comobj::{guid, Guid};
use super::legacy_com::{get_ptr, ptr_size, put_ptr, rd32, vtable_sig, wr32};
use crate::dlls::lock;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{
    IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════
// Constantes publiques (dplay8.h, dpaddr.h)
// ═══════════════════════════════════════════════════════════════════════

const DPN_OK: i32 = 0;
const DPNSUCCESS_PENDING: i32 = 0x0015_800E;
const DPNSUCCESS_EQUAL: i32 = 0x0015_8005;
const DPNSUCCESS_NOTEQUAL: i32 = 0x0015_800A;
const fn dpnerr(code: u32) -> i32 {
    (0x8015_8000 | code) as i32
}
const DPNERR_GENERIC: i32 = 0x8000_4005u32 as i32;
const DPNERR_INVALIDPARAM: i32 = 0x8007_0057u32 as i32;
const DPNERR_INVALIDPOINTER: i32 = 0x8000_4003u32 as i32;
const DPNERR_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const DPNERR_UNSUPPORTED: i32 = 0x8000_4001u32 as i32;
const DPNERR_ALREADYCONNECTED: i32 = dpnerr(0x60);
const DPNERR_ALREADYINITIALIZED: i32 = dpnerr(0x80);
const DPNERR_BUFFERTOOSMALL: i32 = dpnerr(0x100);
const DPNERR_CANTCREATEGROUP: i32 = dpnerr(0x120);
const DPNERR_CONNECTIONLOST: i32 = dpnerr(0x160);
const DPNERR_DOESNOTEXIST: i32 = dpnerr(0x180);
const DPNERR_HOSTREJECTEDCONNECTION: i32 = dpnerr(0x260);
const DPNERR_HOSTTERMINATEDSESSION: i32 = dpnerr(0x270);
const DPNERR_INCOMPLETEADDRESS: i32 = dpnerr(0x280);
const DPNERR_INVALIDADDRESSFORMAT: i32 = dpnerr(0x290);
const DPNERR_INVALIDGROUP: i32 = dpnerr(0x350);
const DPNERR_INVALIDDEVICEADDRESS: i32 = dpnerr(0x320);
const DPNERR_INVALIDHOSTADDRESS: i32 = dpnerr(0x370);
const DPNERR_INVALIDHANDLE: i32 = dpnerr(0x360);
const DPNERR_INVALIDPASSWORD: i32 = dpnerr(0x3A0);
const DPNERR_INVALIDPLAYER: i32 = dpnerr(0x3B0);
const DPNERR_NOCONNECTION: i32 = dpnerr(0x5A0);
const DPNERR_NOTHOST: i32 = dpnerr(0x5B0);
const DPNERR_SESSIONFULL: i32 = dpnerr(0x630);
const DPNERR_TIMEDOUT: i32 = dpnerr(0x690);
const DPNERR_UNINITIALIZED: i32 = dpnerr(0x6B0);
const DPNERR_USERCANCEL: i32 = dpnerr(0x6C0);
const DPNERR_PLAYERNOTINGROUP: i32 = dpnerr(0x5E0);
const DPNERR_PLAYERALREADYINGROUP: i32 = dpnerr(0x5D0);

const MSG: u32 = 0xFFFF_0000;
const M_ADD_PLAYER_TO_GROUP: u32 = MSG | 0x01;
const M_APPLICATION_DESC: u32 = MSG | 0x02;
const M_ASYNC_OP_COMPLETE: u32 = MSG | 0x03;
const M_CLIENT_INFO: u32 = MSG | 0x04;
const M_CONNECT_COMPLETE: u32 = MSG | 0x05;
const M_CREATE_GROUP: u32 = MSG | 0x06;
const M_CREATE_PLAYER: u32 = MSG | 0x07;
const M_DESTROY_GROUP: u32 = MSG | 0x08;
const M_DESTROY_PLAYER: u32 = MSG | 0x09;
const M_ENUM_HOSTS_QUERY: u32 = MSG | 0x0A;
const M_ENUM_HOSTS_RESPONSE: u32 = MSG | 0x0B;
const M_GROUP_INFO: u32 = MSG | 0x0C;
const M_INDICATE_CONNECT: u32 = MSG | 0x0E;
const M_PEER_INFO: u32 = MSG | 0x10;
const M_RECEIVE: u32 = MSG | 0x11;
const M_REMOVE_PLAYER_FROM_GROUP: u32 = MSG | 0x12;
const M_RETURN_BUFFER: u32 = MSG | 0x13;
const M_SEND_COMPLETE: u32 = MSG | 0x14;
const M_SERVER_INFO: u32 = MSG | 0x15;
const M_TERMINATE_SESSION: u32 = MSG | 0x16;

const SYNC: u32 = 0x8000_0000;
const DPNSEND_NOCOMPLETE: u32 = 0x2;
const DPNSEND_NOLOOPBACK: u32 = 0x10;
const DPNINFO_NAME: u32 = 0x1;
const DPNINFO_DATA: u32 = 0x2;
const DPNPLAYER_LOCAL: u32 = 0x2;
const DPNPLAYER_HOST: u32 = 0x4;
const DPNENUM_PLAYERS: u32 = 0x1;
const DPNENUM_GROUPS: u32 = 0x10;
const DPNSESSION_REQUIREPASSWORD: u32 = 0x80;
const DPNCANCEL_ALL: u32 = 0x8000;

const DESTROY_NORMAL: u32 = 1;
const DESTROY_CONNECTIONLOST: u32 = 2;
const DESTROY_SESSIONTERMINATED: u32 = 3;
const DESTROY_HOSTDESTROYED: u32 = 4;

const DT_STRING: u32 = 1;
const DT_DWORD: u32 = 2;
const DT_GUID: u32 = 3;
const DT_BINARY: u32 = 4;
const DT_STRING_ANSI: u32 = 5;

pub const CLSID_DIRECTPLAY8CLIENT: Guid = guid(
    0x743f1dc6,
    0x5aba,
    0x429f,
    [0x8b, 0xdf, 0xc5, 0x4d, 0x03, 0x25, 0x3d, 0xc2],
);
pub const CLSID_DIRECTPLAY8SERVER: Guid = guid(
    0xda825e1b,
    0x6830,
    0x43d7,
    [0x83, 0x5d, 0x0b, 0x5a, 0xd8, 0x29, 0x56, 0xa2],
);
pub const CLSID_DIRECTPLAY8PEER: Guid = guid(
    0x286f484d,
    0x375e,
    0x4458,
    [0xa2, 0x72, 0xb1, 0x38, 0xe2, 0xf8, 0x0a, 0x6a],
);
pub const CLSID_DIRECTPLAY8ADDRESS: Guid = guid(
    0x934a9523,
    0xa3ca,
    0x4bc5,
    [0xad, 0xa0, 0xd6, 0xd9, 0x5d, 0x97, 0x94, 0x21],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDIRECTPLAY8CLIENT: Guid = guid(
    0x5102dacd,
    0x241b,
    0x11d3,
    [0xae, 0xa7, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11],
);
const IID_IDIRECTPLAY8SERVER: Guid = guid(
    0x5102dace,
    0x241b,
    0x11d3,
    [0xae, 0xa7, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11],
);
const IID_IDIRECTPLAY8PEER: Guid = guid(
    0x5102dacf,
    0x241b,
    0x11d3,
    [0xae, 0xa7, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11],
);
const IID_IDIRECTPLAY8ADDRESS: Guid = guid(
    0x83783300,
    0x4063,
    0x4c8a,
    [0x9d, 0xb3, 0x82, 0x83, 0x0a, 0x7f, 0xeb, 0x31],
);
const IID_IDIRECTPLAY8ADDRESSIP: Guid = guid(
    0xe5a0e990,
    0x2bad,
    0x430b,
    [0x87, 0xda, 0xa1, 0x42, 0xcf, 0x75, 0xde, 0x58],
);
pub const CLSID_DP8SP_TCPIP: Guid = guid(
    0xebfe7ba0,
    0x628d,
    0x11d2,
    [0xae, 0x0f, 0x00, 0x60, 0x97, 0xb0, 0x14, 0x11],
);
/// Carte réseau unique présentée par le fournisseur TCP/IP de Winlux.
const DEVICE_LAN: Guid = guid(
    0x57494e4c,
    0x5544,
    0x5038,
    [0x4c, 0x41, 0x4e, 0x00, 0x00, 0x00, 0x00, 0x01],
);

/// Port d'énumération (celui de DPNSVR), réglable pour les tests.
fn enum_port() -> u16 {
    std::env::var("WINLUX_DPNET_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6073)
}
const DEFAULT_GAME_PORT: u16 = 2302;

fn sp_name() -> &'static str {
    match super::dialogs::session_lang().as_str() {
        "fr" => "Connexion TCP/IP (réseau local et Internet)",
        "de" => "TCP/IP-Verbindung (lokales Netzwerk und Internet)",
        "es" => "Conexión TCP/IP (red local e Internet)",
        _ => "TCP/IP connection (local network and Internet)",
    }
}
fn device_name() -> &'static str {
    match super::dialogs::session_lang().as_str() {
        "fr" => "Réseau local",
        "de" => "Lokales Netzwerk",
        "es" => "Red local",
        _ => "Local network",
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Structures dans l'ABI du programme
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
enum F {
    U32,
    Ptr,
    Guid,
}

/// Décalages des champs et taille totale (alignement naturel de l'ABI).
fn layout(fields: &[F]) -> (Vec<usize>, usize) {
    let ps = ptr_size();
    let mut off: usize = 0;
    let mut out = Vec::new();
    let mut align: usize = 4;
    for f in fields {
        let (sz, al) = match f {
            F::U32 => (4, 4),
            F::Ptr => (ps, ps),
            F::Guid => (16, 4),
        };
        align = align.max(al);
        off = off.div_ceil(al) * al;
        out.push(off);
        off += sz;
    }
    (out, off.div_ceil(align) * align)
}

const APP_DESC: &[F] = &[
    F::U32,
    F::U32,
    F::Guid,
    F::Guid,
    F::U32,
    F::U32,
    F::Ptr,
    F::Ptr,
    F::Ptr,
    F::U32,
    F::Ptr,
    F::U32,
];
const PLAYER_INFO: &[F] = &[F::U32, F::U32, F::Ptr, F::Ptr, F::U32, F::U32];
const BUFFER_DESC: &[F] = &[F::U32, F::Ptr];
const SP_INFO: &[F] = &[F::U32, F::Guid, F::Ptr, F::Ptr, F::U32];

/// Message construit champ par champ, puis lu après l'appel.
struct Blob {
    b: Vec<u8>,
    off: Vec<usize>,
}
impl Blob {
    fn new(fields: &[F]) -> Blob {
        let (off, size) = layout(fields);
        let mut b = vec![0u8; size];
        b[..4].copy_from_slice(&(size as u32).to_le_bytes());
        Blob { b, off }
    }
    fn base(&self) -> usize {
        self.b.as_ptr() as usize
    }
    unsafe fn u32(&mut self, i: usize, v: u32) {
        wr32(self.base() + self.off[i], v);
    }
    unsafe fn ptr(&mut self, i: usize, v: usize) {
        put_ptr(self.base() + self.off[i], v);
    }
    unsafe fn get_u32(&self, i: usize) -> u32 {
        rd32(self.base() + self.off[i])
    }
    unsafe fn get_ptr(&self, i: usize) -> usize {
        get_ptr(self.base() + self.off[i])
    }
}

unsafe fn read_guid(p: usize) -> Guid {
    let mut g = [0u8; 16];
    if p != 0 {
        std::ptr::copy_nonoverlapping(p as *const u8, g.as_mut_ptr(), 16);
    }
    g
}
unsafe fn read_bytes(p: usize, n: usize) -> Vec<u8> {
    if p == 0 || n == 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(p as *const u8, n).to_vec()
}
unsafe fn read_wstr(p: usize) -> String {
    if p == 0 {
        String::new()
    } else {
        super::wstr(p as *const u16)
    }
}
fn wide(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    v.extend([0, 0]);
    v
}

#[derive(Clone, Debug, Default, PartialEq)]
struct AppDesc {
    flags: u32,
    instance: Guid,
    application: Guid,
    max_players: u32,
    current: u32,
    name: String,
    password: String,
    reserved: Vec<u8>,
    app_reserved: Vec<u8>,
}

unsafe fn read_app_desc(p: usize) -> Option<AppDesc> {
    if p == 0 {
        return None;
    }
    let (o, _) = layout(APP_DESC);
    let r = |i: usize| rd32(p + o[i]);
    let g = |i: usize| get_ptr(p + o[i]);
    Some(AppDesc {
        flags: r(1),
        instance: read_guid(p + o[2]),
        application: read_guid(p + o[3]),
        max_players: r(4),
        current: r(5),
        name: read_wstr(g(6)),
        password: read_wstr(g(7)),
        reserved: read_bytes(g(8), r(9) as usize),
        app_reserved: read_bytes(g(10), r(11) as usize),
    })
}

/// DPN_APPLICATION_DESC suivie de ses chaînes, rangée à l'adresse `at`.
fn app_desc_bytes(d: &AppDesc, at: usize) -> Vec<u8> {
    let (o, size) = layout(APP_DESC);
    let mut b = vec![0u8; size];
    let name = wide(&d.name);
    let pw = if d.password.is_empty() {
        Vec::new()
    } else {
        wide(&d.password)
    };
    let mut tail = Vec::new();
    let place = |data: &[u8], tail: &mut Vec<u8>| -> usize {
        if data.is_empty() {
            return 0;
        }
        while !tail.len().is_multiple_of(4) {
            tail.push(0);
        }
        let p = at + size + tail.len();
        tail.extend_from_slice(data);
        p
    };
    let pn = place(&name, &mut tail);
    let pp = place(&pw, &mut tail);
    let pr = place(&d.reserved, &mut tail);
    let pa = place(&d.app_reserved, &mut tail);
    let ps = ptr_size();
    let put_u32 =
        |b: &mut Vec<u8>, i: usize, v: u32| b[o[i]..o[i] + 4].copy_from_slice(&v.to_le_bytes());
    let put_ptr_b = |b: &mut Vec<u8>, i: usize, v: usize| {
        b[o[i]..o[i] + ps].copy_from_slice(&v.to_le_bytes()[..ps]);
    };
    put_u32(&mut b, 0, size as u32);
    put_u32(&mut b, 1, d.flags);
    b[o[2]..o[2] + 16].copy_from_slice(&d.instance);
    b[o[3]..o[3] + 16].copy_from_slice(&d.application);
    put_u32(&mut b, 4, d.max_players);
    put_u32(&mut b, 5, d.current);
    put_ptr_b(&mut b, 6, pn);
    put_ptr_b(&mut b, 7, pp);
    put_ptr_b(&mut b, 8, pr);
    put_u32(&mut b, 9, d.reserved.len() as u32);
    put_ptr_b(&mut b, 10, pa);
    put_u32(&mut b, 11, d.app_reserved.len() as u32);
    b.extend(tail);
    b
}

/// Rend `bytes` (construit pour l'adresse `out`) si la place suffit.
unsafe fn give(out: usize, size_at: usize, build: impl Fn(usize) -> Vec<u8>) -> i32 {
    if size_at == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let need = build(out).len();
    let have = rd32(size_at) as usize;
    wr32(size_at, need as u32);
    if out == 0 || have < need {
        return DPNERR_BUFFERTOOSMALL;
    }
    let b = build(out);
    std::ptr::copy_nonoverlapping(b.as_ptr(), out as *mut u8, b.len());
    DPN_OK
}

// ═══════════════════════════════════════════════════════════════════════
// IDirectPlay8Address
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Default, PartialEq)]
struct AddrData {
    comps: Vec<(String, u32, Vec<u8>)>,
    user: Vec<u8>,
}

impl AddrData {
    fn set(&mut self, key: &str, ty: u32, data: Vec<u8>) {
        let k = key.to_ascii_lowercase();
        match self.comps.iter_mut().find(|c| c.0.eq_ignore_ascii_case(&k)) {
            Some(c) => {
                c.1 = ty;
                c.2 = data;
            }
            None => self.comps.push((key.to_string(), ty, data)),
        }
    }
    fn get(&self, key: &str) -> Option<&(String, u32, Vec<u8>)> {
        self.comps.iter().find(|c| c.0.eq_ignore_ascii_case(key))
    }
    fn string(&self, key: &str) -> Option<String> {
        let (_, ty, d) = self.get(key)?;
        Some(match *ty {
            DT_STRING => {
                let w: Vec<u16> = d
                    .chunks(2)
                    .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                    .collect();
                String::from_utf16_lossy(&w)
                    .trim_end_matches('\0')
                    .to_string()
            }
            DT_STRING_ANSI => String::from_utf8_lossy(d)
                .trim_end_matches('\0')
                .to_string(),
            _ => return None,
        })
    }
    fn dword(&self, key: &str) -> Option<u32> {
        let (_, ty, d) = self.get(key)?;
        (*ty == DT_DWORD && d.len() >= 4).then(|| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
    }
    fn set_str(&mut self, key: &str, v: &str) {
        self.set(key, DT_STRING, wide(v));
    }
    fn set_dword(&mut self, key: &str, v: u32) {
        self.set(key, DT_DWORD, v.to_le_bytes().to_vec());
    }
    fn set_guid(&mut self, key: &str, g: &Guid) {
        self.set(key, DT_GUID, g.to_vec());
    }

    fn url(&self) -> String {
        let mut s = String::from("x-directplay:/");
        let mut first = true;
        for (k, ty, d) in &self.comps {
            if !first {
                s.push(';');
            }
            first = false;
            s.push_str(k);
            s.push('=');
            match *ty {
                DT_DWORD if d.len() >= 4 => {
                    s.push_str(&u32::from_le_bytes([d[0], d[1], d[2], d[3]]).to_string())
                }
                DT_GUID if d.len() >= 16 => {
                    let mut g = [0u8; 16];
                    g.copy_from_slice(&d[..16]);
                    s.push_str(&format!("%7B{}%7D", guid_text(&g)));
                }
                DT_STRING | DT_STRING_ANSI => {
                    let v = self.string(k).unwrap_or_default();
                    s.push_str(&escape(v.as_bytes()));
                }
                _ => s.push_str(&escape(d)),
            }
        }
        if !self.user.is_empty() {
            s.push('#');
            s.push_str(&escape(&self.user));
        }
        s
    }

    fn parse(url: &str) -> Option<AddrData> {
        let lower = url.to_ascii_lowercase();
        let rest = lower
            .strip_prefix("x-directplay:/")
            .map(|_| &url["x-directplay:/".len()..])?;
        let (body, user) = match rest.split_once('#') {
            Some((b, u)) => (b, unescape(u)),
            None => (rest, Vec::new()),
        };
        let mut a = AddrData {
            comps: Vec::new(),
            user,
        };
        for part in body.split(';').filter(|p| !p.is_empty()) {
            let (k, v) = part.split_once('=')?;
            let raw = unescape(v);
            let text = String::from_utf8_lossy(&raw).to_string();
            let key = k.to_ascii_lowercase();
            if text.starts_with('{') && text.ends_with('}') {
                a.set_guid(k, &parse_guid(&text[1..text.len() - 1])?);
            } else if matches!(
                key.as_str(),
                "port" | "baud" | "processor" | "program" | "slot"
            ) && text.parse::<u32>().is_ok()
            {
                a.set_dword(k, text.parse().ok()?);
            } else {
                a.set_str(k, &text);
            }
        }
        Some(a)
    }
}

fn guid_text(g: &Guid) -> String {
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        u32::from_le_bytes([g[0], g[1], g[2], g[3]]),
        u16::from_le_bytes([g[4], g[5]]),
        u16::from_le_bytes([g[6], g[7]]),
        g[8],
        g[9],
        g[10],
        g[11],
        g[12],
        g[13],
        g[14],
        g[15]
    )
}
fn parse_guid(s: &str) -> Option<Guid> {
    let h: String = s.chars().filter(|c| *c != '-').collect();
    if h.len() != 32 {
        return None;
    }
    let n = |a: usize, b: usize| u64::from_str_radix(&h[a..b], 16).ok();
    let mut d4 = [0u8; 8];
    for (i, x) in d4.iter_mut().enumerate() {
        *x = n(16 + 2 * i, 18 + 2 * i)? as u8;
    }
    Some(guid(
        n(0, 8)? as u32,
        n(8, 12)? as u16,
        n(12, 16)? as u16,
        d4,
    ))
}
fn escape(d: &[u8]) -> String {
    let mut s = String::new();
    for &c in d {
        if c.is_ascii_alphanumeric() || b"-_.!~*()".contains(&c) {
            s.push(c as char);
        } else {
            s.push_str(&format!("%{c:02X}"));
        }
    }
    s
}
fn unescape(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

#[repr(C)]
struct Addr {
    vtbl: usize,
    refs: AtomicU32,
    d: Mutex<AddrData>,
}

unsafe fn addr<'a>(this: usize) -> &'a Addr {
    &*(this as *const Addr)
}

fn new_addr(d: AddrData) -> usize {
    Box::into_raw(Box::new(Addr {
        vtbl: addr_vtable(),
        refs: AtomicU32::new(1),
        d: Mutex::new(d),
    })) as usize
}

/// Adresse TCP/IP « hostname=…;port=… ».
fn ip_addr(host: &str, port: u16) -> AddrData {
    let mut a = AddrData::default();
    a.set_guid("provider", &CLSID_DP8SP_TCPIP);
    a.set_str("hostname", host);
    a.set_dword("port", port as u32);
    a
}
fn device_addr(port: u16) -> AddrData {
    let mut a = AddrData::default();
    a.set_guid("provider", &CLSID_DP8SP_TCPIP);
    a.set_guid("device", &DEVICE_LAN);
    if port != 0 {
        a.set_dword("port", port as u32);
    }
    a
}

unsafe extern "win64" fn a_qi(this: usize, iid: *const Guid, out: usize) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    put_ptr(out, 0);
    let i = if iid.is_null() { [0; 16] } else { *iid };
    if i != IID_IUNKNOWN && i != IID_IDIRECTPLAY8ADDRESS {
        if i == IID_IDIRECTPLAY8ADDRESSIP {
            crate::log_warn!("dpnet", "IDirectPlay8AddressIP non pris en charge");
        }
        return DPNERR_NOINTERFACE;
    }
    a_addref(this);
    put_ptr(out, this);
    DPN_OK
}
unsafe extern "win64" fn a_addref(this: usize) -> u32 {
    addr(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn a_release(this: usize) -> u32 {
    let n = addr(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut Addr));
    }
    n
}
unsafe extern "win64" fn a_build_w(this: usize, url: *const u16) -> i32 {
    if url.is_null() {
        return DPNERR_INVALIDPOINTER;
    }
    match AddrData::parse(&super::wstr(url)) {
        Some(d) => {
            *lock(&addr(this).d) = d;
            DPN_OK
        }
        None => DPNERR_INVALIDADDRESSFORMAT,
    }
}
unsafe extern "win64" fn a_build_a(this: usize, url: *const u8) -> i32 {
    if url.is_null() {
        return DPNERR_INVALIDPOINTER;
    }
    match AddrData::parse(&super::cstr(url)) {
        Some(d) => {
            *lock(&addr(this).d) = d;
            DPN_OK
        }
        None => DPNERR_INVALIDADDRESSFORMAT,
    }
}
unsafe extern "win64" fn a_duplicate(this: usize, out: usize) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    put_ptr(out, new_addr(lock(&addr(this).d).clone()));
    DPN_OK
}
unsafe extern "win64" fn a_set_equal(this: usize, other: usize) -> i32 {
    if other == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let d = lock(&addr(other).d).clone();
    *lock(&addr(this).d) = d;
    DPN_OK
}
unsafe extern "win64" fn a_is_equal(this: usize, other: usize) -> i32 {
    if other == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let a = lock(&addr(this).d).url().to_ascii_lowercase();
    let b = lock(&addr(other).d).url().to_ascii_lowercase();
    if a == b {
        DPNSUCCESS_EQUAL
    } else {
        DPNSUCCESS_NOTEQUAL
    }
}
unsafe extern "win64" fn a_clear(this: usize) -> i32 {
    *lock(&addr(this).d) = AddrData::default();
    DPN_OK
}
unsafe extern "win64" fn a_get_url_w(this: usize, out: usize, count: usize) -> i32 {
    if count == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let u: Vec<u16> = lock(&addr(this).d)
        .url()
        .encode_utf16()
        .chain([0])
        .collect();
    let have = rd32(count) as usize;
    wr32(count, u.len() as u32);
    if out == 0 || have < u.len() {
        return DPNERR_BUFFERTOOSMALL;
    }
    std::ptr::copy_nonoverlapping(u.as_ptr(), out as *mut u16, u.len());
    DPN_OK
}
unsafe extern "win64" fn a_get_url_a(this: usize, out: usize, count: usize) -> i32 {
    if count == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let mut u = lock(&addr(this).d).url().into_bytes();
    u.push(0);
    let have = rd32(count) as usize;
    wr32(count, u.len() as u32);
    if out == 0 || have < u.len() {
        return DPNERR_BUFFERTOOSMALL;
    }
    std::ptr::copy_nonoverlapping(u.as_ptr(), out as *mut u8, u.len());
    DPN_OK
}
unsafe fn get_guid_comp(this: usize, key: &str, out: usize) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    match lock(&addr(this).d).get(key) {
        Some((_, DT_GUID, d)) if d.len() >= 16 => {
            std::ptr::copy_nonoverlapping(d.as_ptr(), out as *mut u8, 16);
            DPN_OK
        }
        _ => DPNERR_DOESNOTEXIST,
    }
}
unsafe extern "win64" fn a_get_sp(this: usize, out: usize) -> i32 {
    get_guid_comp(this, "provider", out)
}
unsafe extern "win64" fn a_get_user_data(this: usize, out: usize, size: usize) -> i32 {
    if size == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let u = lock(&addr(this).d).user.clone();
    let have = rd32(size) as usize;
    wr32(size, u.len() as u32);
    if u.is_empty() {
        return DPNERR_DOESNOTEXIST;
    }
    if out == 0 || have < u.len() {
        return DPNERR_BUFFERTOOSMALL;
    }
    std::ptr::copy_nonoverlapping(u.as_ptr(), out as *mut u8, u.len());
    DPN_OK
}
unsafe extern "win64" fn a_set_sp(this: usize, g: usize) -> i32 {
    if g == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    lock(&addr(this).d).set_guid("provider", &read_guid(g));
    DPN_OK
}
unsafe extern "win64" fn a_set_user_data(this: usize, data: usize, size: u32) -> i32 {
    lock(&addr(this).d).user = read_bytes(data, size as usize);
    DPN_OK
}
unsafe extern "win64" fn a_get_num_components(this: usize, out: usize) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    wr32(out, lock(&addr(this).d).comps.len() as u32);
    DPN_OK
}
unsafe fn give_comp(c: &(String, u32, Vec<u8>), buf: usize, size: usize, ty: usize) -> i32 {
    if size == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let have = rd32(size) as usize;
    wr32(size, c.2.len() as u32);
    if ty != 0 {
        wr32(ty, c.1);
    }
    if buf == 0 || have < c.2.len() {
        return DPNERR_BUFFERTOOSMALL;
    }
    std::ptr::copy_nonoverlapping(c.2.as_ptr(), buf as *mut u8, c.2.len());
    DPN_OK
}
unsafe extern "win64" fn a_get_component_by_name(
    this: usize,
    name: *const u16,
    buf: usize,
    size: usize,
    ty: usize,
) -> i32 {
    if name.is_null() {
        return DPNERR_INVALIDPOINTER;
    }
    let n = super::wstr(name);
    let d = lock(&addr(this).d);
    match d.get(&n) {
        Some(c) => give_comp(c, buf, size, ty),
        None => DPNERR_DOESNOTEXIST,
    }
}
unsafe extern "win64" fn a_get_component_by_index(
    this: usize,
    index: u32,
    name: usize,
    name_len: usize,
    buf: usize,
    size: usize,
    ty: usize,
) -> i32 {
    let d = lock(&addr(this).d);
    let Some(c) = d.comps.get(index as usize) else {
        return DPNERR_DOESNOTEXIST;
    };
    if name_len == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let w: Vec<u16> = c.0.encode_utf16().chain([0]).collect();
    let have = rd32(name_len) as usize;
    wr32(name_len, w.len() as u32);
    let have_buf = if size != 0 { rd32(size) as usize } else { 0 };
    if size != 0 {
        wr32(size, c.2.len() as u32);
    }
    if name == 0 || have < w.len() || buf == 0 || have_buf < c.2.len() {
        return DPNERR_BUFFERTOOSMALL;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), name as *mut u16, w.len());
    std::ptr::copy_nonoverlapping(c.2.as_ptr(), buf as *mut u8, c.2.len());
    if ty != 0 {
        wr32(ty, c.1);
    }
    DPN_OK
}
unsafe extern "win64" fn a_add_component(
    this: usize,
    name: *const u16,
    data: usize,
    size: u32,
    ty: u32,
) -> i32 {
    if name.is_null() || (data == 0 && size != 0) {
        return DPNERR_INVALIDPOINTER;
    }
    let n = super::wstr(name);
    let bytes = read_bytes(data, size as usize);
    let ok = match ty {
        DT_DWORD => bytes.len() == 4,
        DT_GUID => bytes.len() == 16,
        DT_STRING | DT_STRING_ANSI | DT_BINARY => true,
        _ => false,
    };
    if !ok {
        return DPNERR_INVALIDPARAM;
    }
    lock(&addr(this).d).set(&n, ty, bytes);
    DPN_OK
}
unsafe extern "win64" fn a_get_device(this: usize, out: usize) -> i32 {
    get_guid_comp(this, "device", out)
}
unsafe extern "win64" fn a_set_device(this: usize, g: usize) -> i32 {
    if g == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    lock(&addr(this).d).set_guid("device", &read_guid(g));
    DPN_OK
}
unsafe extern "win64" fn a_build_from_dp4(_this: usize, _p: usize, _n: u32) -> i32 {
    DPNERR_UNSUPPORTED
}

fn f(p: *const ()) -> usize {
    p as usize
}

fn addr_vtable() -> usize {
    vtable_sig(
        "IDirectPlay8Address",
        &[
            (f(a_qi as *const ()), "ii"),
            (f(a_addref as *const ()), ""),
            (f(a_release as *const ()), ""),
            (f(a_build_w as *const ()), "i"),
            (f(a_build_a as *const ()), "i"),
            (f(a_duplicate as *const ()), "i"),
            (f(a_set_equal as *const ()), "i"),
            (f(a_is_equal as *const ()), "i"),
            (f(a_clear as *const ()), ""),
            (f(a_get_url_w as *const ()), "ii"),
            (f(a_get_url_a as *const ()), "ii"),
            (f(a_get_sp as *const ()), "i"),
            (f(a_get_user_data as *const ()), "ii"),
            (f(a_set_sp as *const ()), "i"),
            (f(a_set_user_data as *const ()), "ii"),
            (f(a_get_num_components as *const ()), "i"),
            (f(a_get_component_by_name as *const ()), "iiii"),
            (f(a_get_component_by_index as *const ()), "iiiiii"),
            (f(a_add_component as *const ()), "iiii"),
            (f(a_get_device as *const ()), "i"),
            (f(a_set_device as *const ()), "i"),
            (f(a_build_from_dp4 as *const ()), "ii"),
        ],
    )
}

// ═══════════════════════════════════════════════════════════════════════
// Protocole Winlux
// ═══════════════════════════════════════════════════════════════════════

const Q_MAGIC: &[u8; 7] = b"WLXDP8Q";
const R_MAGIC: &[u8; 7] = b"WLXDP8R";

const J_JOIN: u8 = 1;
const J_WELCOME: u8 = 2;
const J_REFUSE: u8 = 3;
const J_ADD: u8 = 4;
const J_DEL: u8 = 5;
const J_SEND: u8 = 6;
const J_MSG: u8 = 7;
const J_INFO: u8 = 8;
const J_APPDESC: u8 = 9;
const J_GROUP: u8 = 10;
const J_GROUP_DEL: u8 = 11;
const J_MEMBER: u8 = 12;
const J_MEMBER_DEL: u8 = 13;
const J_TERMINATE: u8 = 14;
const R_GROUP: u8 = 20;
const R_GROUP_DEL: u8 = 21;
const R_MEMBER: u8 = 22;
const R_MEMBER_DEL: u8 = 23;
const R_INFO: u8 = 24;
const R_GROUP_INFO: u8 = 25;

#[derive(Default)]
struct Wr(Vec<u8>);
impl Wr {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
        self
    }
    fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }
    fn guid(&mut self, g: &Guid) -> &mut Self {
        self.0.extend_from_slice(g);
        self
    }
    fn desc(&mut self, d: &AppDesc) -> &mut Self {
        self.u32(d.flags)
            .guid(&d.instance)
            .guid(&d.application)
            .u32(d.max_players)
            .u32(d.current)
            .str(&d.name)
            .u32(!d.password.is_empty() as u32)
            .bytes(&d.reserved)
            .bytes(&d.app_reserved)
    }
}
struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}
impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Rd<'a> {
        Rd { b, i: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i + n)?;
        self.i += n;
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
        Some(String::from_utf8_lossy(&self.bytes()?).into_owned())
    }
    fn guid(&mut self) -> Option<Guid> {
        self.take(16)?.try_into().ok()
    }
    fn desc(&mut self) -> Option<AppDesc> {
        let mut d = AppDesc {
            flags: self.u32()?,
            instance: self.guid()?,
            application: self.guid()?,
            max_players: self.u32()?,
            current: self.u32()?,
            name: self.str()?,
            ..AppDesc::default()
        };
        if self.u32()? != 0 {
            d.password = "*".into();
        }
        d.reserved = self.bytes()?;
        d.app_reserved = self.bytes()?;
        Some(d)
    }
}

fn frame(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 5);
    v.extend(((body.len() + 1) as u32).to_le_bytes());
    v.push(kind);
    v.extend_from_slice(body);
    v
}
fn send_frame(s: &TcpStream, kind: u8, body: &[u8]) -> bool {
    let mut s = s;
    s.write_all(&frame(kind, body)).is_ok()
}
fn read_frame(s: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut n = [0u8; 4];
    s.read_exact(&mut n).ok()?;
    let n = u32::from_le_bytes(n) as usize;
    if n == 0 || n > 64 << 20 {
        return None;
    }
    let mut b = vec![0u8; n];
    s.read_exact(&mut b).ok()?;
    let kind = b[0];
    b.remove(0);
    Some((kind, b))
}

// ═══════════════════════════════════════════════════════════════════════
// Session
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    Peer,
    Client,
    Server,
}

#[derive(Clone, Debug, Default)]
struct Player {
    name: String,
    data: Vec<u8>,
    flags: u32,
    ctx: usize,
    group: bool,
    members: Vec<u32>,
    owner: u32,
    gone: bool,
    addr: Option<SocketAddr>,
}

enum Ev {
    CreatePlayer(u32),
    DestroyPlayer(u32, u32),
    Receive(u32, Vec<u8>),
    SendComplete(u32, usize, i32),
    ConnectComplete(u32, usize, i32, Vec<u8>, u32),
    AsyncDone(u32, usize, i32),
    Terminate(i32, Vec<u8>),
    Info(u32),
    AppDesc,
    CreateGroup(u32, u32, Option<usize>),
    DestroyGroup(u32, u32),
    Member(u32, u32, bool),
    GroupInfo(u32),
    EnumResponse(SocketAddr, u16, AppDesc, Vec<u8>, usize, u32),
    ReturnBuffer(usize, usize),
    Stop,
}

struct St {
    handler: usize,
    ctx: usize,
    init: bool,
    hosting: bool,
    connected: bool,
    local: u32,
    host_id: u32,
    desc: AppDesc,
    players: BTreeMap<u32, Player>,
    next_id: u32,
    conns: HashMap<u32, TcpStream>,
    up: Option<TcpStream>,
    port: u16,
    threads: Vec<JoinHandle<()>>,
    buffers: HashMap<u32, Vec<u8>>,
    next_handle: u32,
    info: (String, Vec<u8>),
    enums: HashMap<u32, Arc<AtomicBool>>,
    group_ctx: HashMap<u32, usize>,
    host_addr: Option<SocketAddr>,
    dispatcher: Option<JoinHandle<()>>,
    dispatcher_id: Option<std::thread::ThreadId>,
}

struct Core {
    role: Role,
    st: Mutex<St>,
    q: Mutex<VecDeque<Ev>>,
    cv: Condvar,
    call: Mutex<()>,
    stop: Arc<AtomicBool>,
}

impl Core {
    fn new(role: Role) -> Arc<Core> {
        Arc::new(Core {
            role,
            st: Mutex::new(St {
                handler: 0,
                ctx: 0,
                init: false,
                hosting: false,
                connected: false,
                local: 0,
                host_id: 0,
                desc: AppDesc::default(),
                players: BTreeMap::new(),
                next_id: 0,
                conns: HashMap::new(),
                up: None,
                port: 0,
                threads: Vec::new(),
                buffers: HashMap::new(),
                next_handle: 0x10,
                info: (String::new(), Vec::new()),
                enums: HashMap::new(),
                group_ctx: HashMap::new(),
                host_addr: None,
                dispatcher: None,
                dispatcher_id: None,
            }),
            q: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            call: Mutex::new(()),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    fn post(&self, e: Ev) {
        lock(&self.q).push_back(e);
        self.cv.notify_all();
    }

    fn handle(&self) -> u32 {
        let mut st = lock(&self.st);
        st.next_handle += 1;
        st.next_handle
    }

    /// Appel direct de la fonction du programme (sérialisé).
    unsafe fn call(&self, id: u32, msg: usize) -> i32 {
        let (h, ctx) = {
            let st = lock(&self.st);
            (st.handler, st.ctx)
        };
        if h == 0 {
            return DPN_OK;
        }
        let _g = lock(&self.call);
        let f: unsafe extern "win64" fn(usize, u32, usize) -> i32 = std::mem::transmute(h);
        f(ctx, id, msg)
    }

    /// Identifiant d'un nouveau joueur ou groupe (hôte).
    fn new_id(st: &mut St) -> u32 {
        st.next_id += 1;
        // Identifiants non séquentiels, comme ceux de DirectPlay 8.
        0x0010_0000 | (st.next_id << 4) | 3
    }

    /// Joueurs visibles par un client dans ce rôle.
    fn sees_others(&self) -> bool {
        self.role == Role::Peer
    }
}

fn info_msg_id(role: Role) -> u32 {
    match role {
        Role::Peer => M_PEER_INFO,
        Role::Client => M_SERVER_INFO,
        Role::Server => M_CLIENT_INFO,
    }
}

/// Fil de remise des messages au programme.
fn dispatcher(core: Arc<Core>) {
    loop {
        let ev = {
            let mut q = lock(&core.q);
            loop {
                if let Some(e) = q.pop_front() {
                    break e;
                }
                q = core.cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        if matches!(ev, Ev::Stop) {
            return;
        }
        unsafe { deliver(&core, ev) };
    }
}

unsafe fn deliver(core: &Arc<Core>, ev: Ev) {
    let ctx_of = |id: u32| lock(&core.st).players.get(&id).map(|p| p.ctx).unwrap_or(0);
    match ev {
        Ev::CreatePlayer(id) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr]);
            m.u32(1, id);
            m.ptr(2, ctx_of(id));
            core.call(M_CREATE_PLAYER, m.base());
            let ctx = m.get_ptr(2);
            if let Some(p) = lock(&core.st).players.get_mut(&id) {
                p.ctx = ctx;
            }
        }
        Ev::DestroyPlayer(id, reason) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::U32]);
            m.u32(1, id);
            m.ptr(2, ctx_of(id));
            m.u32(3, reason);
            core.call(M_DESTROY_PLAYER, m.base());
            lock(&core.st).players.remove(&id);
        }
        Ev::Receive(from, data) => {
            let h = core.handle();
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::Ptr, F::U32, F::U32, F::U32]);
            m.u32(1, from);
            m.ptr(2, ctx_of(from));
            m.ptr(3, data.as_ptr() as usize);
            m.u32(4, data.len() as u32);
            m.u32(5, h);
            let r = core.call(M_RECEIVE, m.base());
            if r == DPNSUCCESS_PENDING {
                lock(&core.st).buffers.insert(h, data);
            }
        }
        Ev::SendComplete(h, ctx, hr) => {
            let mut m = Blob::new(&[
                F::U32,
                F::U32,
                F::Ptr,
                F::U32,
                F::U32,
                F::U32,
                F::U32,
                F::U32,
                F::Ptr,
                F::U32,
            ]);
            m.u32(1, h);
            m.ptr(2, ctx);
            m.u32(3, hr as u32);
            core.call(M_SEND_COMPLETE, m.base());
        }
        Ev::ConnectComplete(h, ctx, hr, reply, local) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::U32, F::Ptr, F::U32, F::U32]);
            m.u32(1, h);
            m.ptr(2, ctx);
            m.u32(3, hr as u32);
            m.ptr(
                4,
                if reply.is_empty() {
                    0
                } else {
                    reply.as_ptr() as usize
                },
            );
            m.u32(5, reply.len() as u32);
            m.u32(6, local);
            core.call(M_CONNECT_COMPLETE, m.base());
        }
        Ev::AsyncDone(h, ctx, hr) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::U32]);
            m.u32(1, h);
            m.ptr(2, ctx);
            m.u32(3, hr as u32);
            core.call(M_ASYNC_OP_COMPLETE, m.base());
        }
        Ev::Terminate(hr, data) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::U32]);
            m.u32(1, hr as u32);
            m.ptr(
                2,
                if data.is_empty() {
                    0
                } else {
                    data.as_ptr() as usize
                },
            );
            m.u32(3, data.len() as u32);
            core.call(M_TERMINATE_SESSION, m.base());
        }
        Ev::Info(id) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr]);
            m.u32(1, id);
            m.ptr(2, ctx_of(id));
            core.call(info_msg_id(core.role), m.base());
        }
        Ev::AppDesc => {
            core.call(M_APPLICATION_DESC, 0);
        }
        Ev::CreateGroup(g, owner, initial) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::U32, F::Ptr, F::Ptr]);
            m.u32(1, g);
            m.u32(2, owner);
            m.ptr(3, initial.unwrap_or_else(|| ctx_of(g)));
            m.ptr(4, ctx_of(owner));
            core.call(M_CREATE_GROUP, m.base());
            let ctx = m.get_ptr(3);
            if let Some(p) = lock(&core.st).players.get_mut(&g) {
                p.ctx = ctx;
            }
        }
        Ev::DestroyGroup(g, reason) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::U32]);
            m.u32(1, g);
            m.ptr(2, ctx_of(g));
            m.u32(3, reason);
            core.call(M_DESTROY_GROUP, m.base());
            lock(&core.st).players.remove(&g);
        }
        Ev::Member(g, p, add) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::U32, F::Ptr]);
            m.u32(1, g);
            m.ptr(2, ctx_of(g));
            m.u32(3, p);
            m.ptr(4, ctx_of(p));
            core.call(
                if add {
                    M_ADD_PLAYER_TO_GROUP
                } else {
                    M_REMOVE_PLAYER_FROM_GROUP
                },
                m.base(),
            );
        }
        Ev::GroupInfo(g) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr]);
            m.u32(1, g);
            m.ptr(2, ctx_of(g));
            core.call(M_GROUP_INFO, m.base());
        }
        Ev::EnumResponse(from, port, desc, data, ctx, rtt) => {
            let sender = new_addr(ip_addr(&from.ip().to_string(), port));
            let device = new_addr(device_addr(0));
            let mut blob = vec![0u8; 0];
            blob.resize(
                4096 + desc.name.len() * 2 + desc.reserved.len() + desc.app_reserved.len(),
                0,
            );
            let at = blob.as_ptr() as usize;
            let b = app_desc_bytes(&desc, at);
            blob[..b.len()].copy_from_slice(&b);
            let mut m = Blob::new(&[
                F::U32,
                F::Ptr,
                F::Ptr,
                F::Ptr,
                F::Ptr,
                F::U32,
                F::Ptr,
                F::U32,
            ]);
            m.ptr(1, sender);
            m.ptr(2, device);
            m.ptr(3, at);
            m.ptr(
                4,
                if data.is_empty() {
                    0
                } else {
                    data.as_ptr() as usize
                },
            );
            m.u32(5, data.len() as u32);
            m.ptr(6, ctx);
            m.u32(7, rtt);
            core.call(M_ENUM_HOSTS_RESPONSE, m.base());
            a_release(sender);
            a_release(device);
        }
        Ev::ReturnBuffer(buf, ctx) => {
            let mut m = Blob::new(&[F::U32, F::U32, F::Ptr, F::Ptr]);
            m.ptr(2, buf);
            m.ptr(3, ctx);
            core.call(M_RETURN_BUFFER, m.base());
        }
        Ev::Stop => {}
    }
}

fn start_dispatcher(core: &Arc<Core>) {
    let mut st = lock(&core.st);
    if st.dispatcher.is_some() {
        return;
    }
    let c = core.clone();
    let t = std::thread::spawn(move || dispatcher(c));
    st.dispatcher_id = Some(t.thread().id());
    st.dispatcher = Some(t);
}

/// Message de l'hôte vers tous ses clients (sauf `except`).
fn broadcast(st: &St, kind: u8, body: &[u8], except: u32) {
    for (id, s) in &st.conns {
        if *id != except {
            send_frame(s, kind, body);
        }
    }
}

fn player_body(id: u32, p: &Player) -> Vec<u8> {
    let mut w = Wr::default();
    w.u32(id)
        .u32(p.flags & DPNPLAYER_HOST)
        .str(&p.name)
        .bytes(&p.data);
    w.0
}

fn read_player(r: &mut Rd) -> Option<(u32, Player)> {
    let id = r.u32()?;
    let flags = r.u32()?;
    Some((
        id,
        Player {
            name: r.str()?,
            data: r.bytes()?,
            flags,
            ..Player::default()
        },
    ))
}

/// Remise d'un message à un ensemble de joueurs (côté hôte ou local).
fn route(core: &Arc<Core>, from: u32, to: u32, flags: u32, data: &[u8]) -> i32 {
    let st = lock(&core.st);
    let targets: Vec<u32> = if to == 0 {
        st.players
            .iter()
            .filter(|(_, p)| !p.group && !p.gone)
            .map(|(i, _)| *i)
            .collect()
    } else {
        match st.players.get(&to) {
            Some(p) if p.group => p.members.clone(),
            Some(_) => vec![to],
            None => return DPNERR_INVALIDPLAYER,
        }
    };
    let mut local = false;
    for t in targets {
        if t == st.local {
            if !(flags & DPNSEND_NOLOOPBACK != 0 && from == st.local) {
                local = true;
            }
        } else if let Some(s) = st.conns.get(&t) {
            let mut w = Wr::default();
            w.u32(from).bytes(data);
            send_frame(s, J_MSG, &w.0);
        }
    }
    drop(st);
    if local {
        core.post(Ev::Receive(from, data.to_vec()));
    }
    DPN_OK
}

/// Réponse UDP aux recherches de parties.
fn responder(core: Arc<Core>, sock: UdpSocket) {
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let mut buf = vec![0u8; 2048];
    while !core.stop.load(Ordering::Acquire) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        let q = &buf[..n];
        if q.len() < 23 || &q[..7] != Q_MAGIC {
            continue;
        }
        let mut r = Rd::new(&q[7..]);
        let (Some(app), Some(user)) = (r.guid(), r.bytes()) else {
            continue;
        };
        let (desc, port) = {
            let st = lock(&core.st);
            if !st.hosting {
                continue;
            }
            let mut d = st.desc.clone();
            d.current = st.players.values().filter(|p| !p.group && !p.gone).count() as u32;
            (d, st.port)
        };
        if app != [0; 16] && app != desc.application {
            continue;
        }
        // Le programme peut refuser ou ajouter des données de réponse.
        let reply = unsafe {
            let sender = new_addr(ip_addr(&from.ip().to_string(), from.port()));
            let device = new_addr(device_addr(port));
            let mut m = Blob::new(&[
                F::U32,
                F::Ptr,
                F::Ptr,
                F::Ptr,
                F::U32,
                F::U32,
                F::Ptr,
                F::U32,
                F::Ptr,
            ]);
            m.ptr(1, sender);
            m.ptr(2, device);
            m.ptr(
                3,
                if user.is_empty() {
                    0
                } else {
                    user.as_ptr() as usize
                },
            );
            m.u32(4, user.len() as u32);
            m.u32(5, 1000);
            let hr = core.call(M_ENUM_HOSTS_QUERY, m.base());
            a_release(sender);
            a_release(device);
            if hr < 0 {
                None
            } else {
                let p = m.get_ptr(6);
                let len = m.get_u32(7) as usize;
                let data = read_bytes(p, len.min(1000));
                if p != 0 {
                    core.post(Ev::ReturnBuffer(p, m.get_ptr(8)));
                }
                Some(data)
            }
        };
        let Some(data) = reply else { continue };
        let mut w = Wr::default();
        w.0.extend_from_slice(R_MAGIC);
        w.u32(port as u32).desc(&desc).bytes(&data);
        let _ = sock.send_to(&w.0, from);
    }
}

fn accept_loop(core: Arc<Core>, l: TcpListener) {
    let _ = l.set_nonblocking(true);
    while !core.stop.load(Ordering::Acquire) {
        match l.accept() {
            Ok((s, a)) => {
                let _ = s.set_nonblocking(false);
                let _ = s.set_nodelay(true);
                let c = core.clone();
                let t = std::thread::spawn(move || host_conn(c, s, a));
                lock(&core.st).threads.push(t);
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// Un client chez l'hôte : arrivée, puis messages.
fn host_conn(core: Arc<Core>, mut s: TcpStream, from: SocketAddr) {
    let Some((J_JOIN, body)) = read_frame(&mut s) else {
        return;
    };
    let mut r = Rd::new(&body);
    let (Some(name), Some(data), Some(user), Some(pw)) = (r.str(), r.bytes(), r.bytes(), r.str())
    else {
        return;
    };
    let refuse = |s: &TcpStream, hr: i32, reply: &[u8]| {
        let mut w = Wr::default();
        w.u32(hr as u32).bytes(reply);
        send_frame(s, J_REFUSE, &w.0);
    };
    {
        let st = lock(&core.st);
        let d = &st.desc;
        let n = st.players.values().filter(|p| !p.group && !p.gone).count() as u32;
        if d.flags & DPNSESSION_REQUIREPASSWORD != 0 && pw != d.password {
            drop(st);
            refuse(&s, DPNERR_INVALIDPASSWORD, &[]);
            return;
        }
        if d.max_players != 0 && n >= d.max_players {
            drop(st);
            refuse(&s, DPNERR_SESSIONFULL, &[]);
            return;
        }
    }
    // DPN_MSGID_INDICATE_CONNECT : le programme accepte ou refuse.
    let (ok, reply, pctx) = unsafe {
        let pa = new_addr(ip_addr(&from.ip().to_string(), from.port()));
        let port = lock(&core.st).port;
        let da = new_addr(device_addr(port));
        let mut m = Blob::new(&[
            F::U32,
            F::Ptr,
            F::U32,
            F::Ptr,
            F::U32,
            F::Ptr,
            F::Ptr,
            F::Ptr,
            F::Ptr,
        ]);
        m.ptr(
            1,
            if user.is_empty() {
                0
            } else {
                user.as_ptr() as usize
            },
        );
        m.u32(2, user.len() as u32);
        m.ptr(7, pa);
        m.ptr(8, da);
        let hr = core.call(M_INDICATE_CONNECT, m.base());
        a_release(pa);
        a_release(da);
        let rp = m.get_ptr(3);
        let reply = read_bytes(rp, m.get_u32(4) as usize);
        if rp != 0 {
            core.post(Ev::ReturnBuffer(rp, m.get_ptr(5)));
        }
        (hr >= 0, reply, m.get_ptr(6))
    };
    if !ok {
        refuse(&s, DPNERR_HOSTREJECTEDCONNECTION, &reply);
        return;
    }
    let Ok(writer) = s.try_clone() else { return };
    let id = {
        let mut st = lock(&core.st);
        let id = Core::new_id(&mut st);
        let p = Player {
            name,
            data,
            ctx: pctx,
            addr: Some(from),
            ..Player::default()
        };
        // Les autres pairs apprennent l'arrivée.
        if core.sees_others() {
            broadcast(&st, J_ADD, &player_body(id, &p), 0);
        }
        st.players.insert(id, p);
        let mut w = Wr::default();
        w.u32(id).u32(st.local).desc(&st.desc).bytes(&reply);
        let list: Vec<(u32, Player)> = st
            .players
            .iter()
            .filter(|(i, p)| !p.gone && (core.sees_others() || **i == st.local || **i == id))
            .map(|(i, p)| (*i, p.clone()))
            .collect();
        w.u32(list.len() as u32);
        for (i, p) in &list {
            w.u8(p.group as u8);
            w.0.extend(player_body(*i, p));
            w.u32(p.owner);
            w.u32(p.members.len() as u32);
            for m in &p.members {
                w.u32(*m);
            }
        }
        send_frame(&writer, J_WELCOME, &w.0);
        st.conns.insert(id, writer);
        id
    };
    core.post(Ev::CreatePlayer(id));
    host_loop(&core, s, id);
    // Départ ou coupure.
    let mut st = lock(&core.st);
    st.conns.remove(&id);
    if let Some(p) = st.players.get_mut(&id) {
        if !p.gone {
            p.gone = true;
            if core.sees_others() {
                let mut w = Wr::default();
                w.u32(id).u32(DESTROY_CONNECTIONLOST);
                broadcast(&st, J_DEL, &w.0, id);
            }
            for g in st.players.values_mut().filter(|g| g.group) {
                g.members.retain(|m| *m != id);
            }
            drop(st);
            core.post(Ev::DestroyPlayer(id, DESTROY_CONNECTIONLOST));
        }
    }
}

fn host_loop(core: &Arc<Core>, mut s: TcpStream, id: u32) {
    while let Some((kind, body)) = read_frame(&mut s) {
        let mut r = Rd::new(&body);
        match kind {
            J_SEND => {
                let (Some(to), Some(flags), Some(data)) = (r.u32(), r.u32(), r.bytes()) else {
                    continue;
                };
                route(core, id, to, flags, &data);
            }
            R_INFO => {
                let (Some(name), Some(data)) = (r.str(), r.bytes()) else {
                    continue;
                };
                let mut st = lock(&core.st);
                if let Some(p) = st.players.get_mut(&id) {
                    p.name = name;
                    p.data = data;
                    let b = player_body(id, p);
                    if core.sees_others() {
                        broadcast(&st, J_INFO, &b, 0);
                    }
                }
                drop(st);
                core.post(Ev::Info(id));
            }
            R_GROUP => {
                let (Some(token), Some(name), Some(data)) = (r.u32(), r.str(), r.bytes()) else {
                    continue;
                };
                host_create_group(core, id, token, name, data);
            }
            R_GROUP_DEL => {
                if let Some(g) = r.u32() {
                    host_destroy_group(core, g);
                }
            }
            R_MEMBER | R_MEMBER_DEL => {
                if let (Some(g), Some(p)) = (r.u32(), r.u32()) {
                    host_member(core, g, p, kind == R_MEMBER);
                }
            }
            R_GROUP_INFO => {
                if let (Some(g), Some(name), Some(data)) = (r.u32(), r.str(), r.bytes()) {
                    host_group_info(core, g, name, data);
                }
            }
            _ => {}
        }
    }
}

fn host_create_group(core: &Arc<Core>, owner: u32, token: u32, name: String, data: Vec<u8>) -> u32 {
    let mut st = lock(&core.st);
    let g = Core::new_id(&mut st);
    let p = Player {
        name,
        data,
        group: true,
        owner,
        ..Player::default()
    };
    let mut w = Wr::default();
    w.u32(token).u32(owner).0.extend(player_body(g, &p));
    broadcast(&st, J_GROUP, &w.0, 0);
    st.players.insert(g, p);
    let initial = if owner == st.local {
        st.group_ctx.remove(&token)
    } else {
        None
    };
    drop(st);
    core.post(Ev::CreateGroup(g, owner, initial));
    g
}
fn host_destroy_group(core: &Arc<Core>, g: u32) -> i32 {
    let mut st = lock(&core.st);
    match st.players.get_mut(&g) {
        Some(p) if p.group && !p.gone => p.gone = true,
        _ => return DPNERR_INVALIDGROUP,
    }
    let mut w = Wr::default();
    w.u32(g);
    broadcast(&st, J_GROUP_DEL, &w.0, 0);
    drop(st);
    core.post(Ev::DestroyGroup(g, DESTROY_NORMAL));
    DPN_OK
}
fn host_member(core: &Arc<Core>, g: u32, p: u32, add: bool) -> i32 {
    let mut st = lock(&core.st);
    if !st.players.get(&p).is_some_and(|x| !x.group && !x.gone) {
        return DPNERR_INVALIDPLAYER;
    }
    let Some(grp) = st.players.get_mut(&g).filter(|x| x.group && !x.gone) else {
        return DPNERR_INVALIDGROUP;
    };
    let has = grp.members.contains(&p);
    if add == has {
        return if add {
            DPNERR_PLAYERALREADYINGROUP
        } else {
            DPNERR_PLAYERNOTINGROUP
        };
    }
    if add {
        grp.members.push(p);
    } else {
        grp.members.retain(|m| *m != p);
    }
    let mut w = Wr::default();
    w.u32(g).u32(p);
    broadcast(&st, if add { J_MEMBER } else { J_MEMBER_DEL }, &w.0, 0);
    drop(st);
    core.post(Ev::Member(g, p, add));
    DPN_OK
}
fn host_group_info(core: &Arc<Core>, g: u32, name: String, data: Vec<u8>) -> i32 {
    let mut st = lock(&core.st);
    let Some(grp) = st.players.get_mut(&g).filter(|x| x.group) else {
        return DPNERR_INVALIDGROUP;
    };
    grp.name = name;
    grp.data = data;
    let b = player_body(g, grp);
    broadcast(&st, J_INFO, &b, 0);
    drop(st);
    core.post(Ev::GroupInfo(g));
    DPN_OK
}

/// Lecture des messages de l'hôte chez un client.
fn client_loop(core: Arc<Core>, mut s: TcpStream) {
    let mut terminated = false;
    while let Some((kind, body)) = read_frame(&mut s) {
        let mut r = Rd::new(&body);
        match kind {
            J_MSG => {
                if let (Some(from), Some(data)) = (r.u32(), r.bytes()) {
                    core.post(Ev::Receive(from, data));
                }
            }
            J_ADD => {
                if let Some((id, p)) = read_player(&mut r) {
                    lock(&core.st).players.insert(id, p);
                    core.post(Ev::CreatePlayer(id));
                }
            }
            J_DEL => {
                if let (Some(id), Some(reason)) = (r.u32(), r.u32()) {
                    let mut st = lock(&core.st);
                    if let Some(p) = st.players.get_mut(&id) {
                        p.gone = true;
                    }
                    for g in st.players.values_mut().filter(|g| g.group) {
                        g.members.retain(|m| *m != id);
                    }
                    drop(st);
                    core.post(Ev::DestroyPlayer(id, reason));
                }
            }
            J_INFO => {
                if let Some((id, np)) = read_player(&mut r) {
                    let mut st = lock(&core.st);
                    let group = if let Some(p) = st.players.get_mut(&id) {
                        p.name = np.name;
                        p.data = np.data;
                        p.group
                    } else {
                        false
                    };
                    drop(st);
                    core.post(if group {
                        Ev::GroupInfo(id)
                    } else {
                        Ev::Info(id)
                    });
                }
            }
            J_APPDESC => {
                if let Some(d) = r.desc() {
                    lock(&core.st).desc = d;
                    core.post(Ev::AppDesc);
                }
            }
            J_GROUP => {
                let (Some(token), Some(owner)) = (r.u32(), r.u32()) else {
                    continue;
                };
                if let Some((g, mut p)) = read_player(&mut r) {
                    p.group = true;
                    p.owner = owner;
                    let mut st = lock(&core.st);
                    let initial = if owner == st.local {
                        st.group_ctx.remove(&token)
                    } else {
                        None
                    };
                    st.players.insert(g, p);
                    drop(st);
                    core.post(Ev::CreateGroup(g, owner, initial));
                }
            }
            J_GROUP_DEL => {
                if let Some(g) = r.u32() {
                    if let Some(p) = lock(&core.st).players.get_mut(&g) {
                        p.gone = true;
                    }
                    core.post(Ev::DestroyGroup(g, DESTROY_NORMAL));
                }
            }
            J_MEMBER | J_MEMBER_DEL => {
                if let (Some(g), Some(p)) = (r.u32(), r.u32()) {
                    let add = kind == J_MEMBER;
                    if let Some(grp) = lock(&core.st).players.get_mut(&g) {
                        if add {
                            grp.members.push(p);
                        } else {
                            grp.members.retain(|m| *m != p);
                        }
                    }
                    core.post(Ev::Member(g, p, add));
                }
            }
            J_TERMINATE => {
                let data = r.bytes().unwrap_or_default();
                terminated = true;
                core.post(Ev::Terminate(DPNERR_HOSTTERMINATEDSESSION, data));
                break;
            }
            _ => {}
        }
    }
    if core.stop.load(Ordering::Acquire) {
        return;
    }
    // Hôte perdu : fin de session.
    if !terminated {
        core.post(Ev::Terminate(DPNERR_CONNECTIONLOST, Vec::new()));
    }
    let reason = if terminated {
        DESTROY_SESSIONTERMINATED
    } else {
        DESTROY_CONNECTIONLOST
    };
    let mut st = lock(&core.st);
    st.connected = false;
    st.up = None;
    let ids: Vec<(u32, bool)> = st
        .players
        .iter_mut()
        .filter(|(_, p)| !p.gone)
        .map(|(i, p)| {
            p.gone = true;
            (*i, p.group)
        })
        .collect();
    drop(st);
    for (i, g) in ids {
        core.post(if g {
            Ev::DestroyGroup(i, reason)
        } else {
            Ev::DestroyPlayer(i, reason)
        });
    }
}

/// Recherche des parties en UDP ; rend (adresse, port TCP, description, données).
fn enum_once(
    targets: &[SocketAddr],
    app: &Guid,
    user: &[u8],
    wait: Duration,
    stop: &AtomicBool,
    mut each: impl FnMut(SocketAddr, u16, AppDesc, Vec<u8>, u32),
) {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else {
        return;
    };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(100)));
    let mut q = Q_MAGIC.to_vec();
    let mut w = Wr::default();
    w.guid(app).bytes(user);
    q.extend(w.0);
    let start = Instant::now();
    for t in targets {
        let _ = sock.send_to(&q, t);
    }
    let mut seen: Vec<(SocketAddr, Guid)> = Vec::new();
    let mut buf = vec![0u8; 4096];
    while start.elapsed() < wait && !stop.load(Ordering::Acquire) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        if n < 7 || &buf[..7] != R_MAGIC {
            continue;
        }
        let mut r = Rd::new(&buf[7..n]);
        let (Some(port), Some(desc), Some(data)) = (r.u32(), r.desc(), r.bytes()) else {
            continue;
        };
        // Une même partie répond sur plusieurs ports : une seule fois.
        if seen
            .iter()
            .any(|(a, g)| a.ip() == from.ip() && *g == desc.instance)
        {
            continue;
        }
        seen.push((from, desc.instance));
        each(
            from,
            port as u16,
            desc,
            data,
            start.elapsed().as_millis() as u32,
        );
    }
}

fn targets_for(host: &AddrData) -> Vec<SocketAddr> {
    let mut v = Vec::new();
    let mut ports = vec![enum_port()];
    if let Some(p) = host.dword("port") {
        ports.insert(0, p as u16);
    }
    match host.string("hostname").filter(|h| !h.is_empty()) {
        Some(h) => {
            for p in &ports {
                if let Ok(it) = (h.as_str(), *p).to_socket_addrs() {
                    v.extend(it.filter(|a| a.is_ipv4()));
                }
            }
        }
        None => {
            for p in &ports {
                v.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), *p));
                v.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), *p));
            }
        }
    }
    v
}

// ═══════════════════════════════════════════════════════════════════════
// Objets DirectPlay 8
// ═══════════════════════════════════════════════════════════════════════

#[repr(C)]
struct Obj {
    vtbl: usize,
    refs: AtomicU32,
    core: Arc<Core>,
}

unsafe fn core<'a>(this: usize) -> &'a Arc<Core> {
    &(*(this as *const Obj)).core
}

unsafe extern "win64" fn d_qi(this: usize, iid: *const Guid, out: usize) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    put_ptr(out, 0);
    let i = if iid.is_null() { [0; 16] } else { *iid };
    let want = match core(this).role {
        Role::Peer => IID_IDIRECTPLAY8PEER,
        Role::Client => IID_IDIRECTPLAY8CLIENT,
        Role::Server => IID_IDIRECTPLAY8SERVER,
    };
    if i != IID_IUNKNOWN && i != want {
        return DPNERR_NOINTERFACE;
    }
    d_addref(this);
    put_ptr(out, this);
    DPN_OK
}
unsafe extern "win64" fn d_addref(this: usize) -> u32 {
    (*(this as *const Obj)).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn d_release(this: usize) -> u32 {
    let n = (*(this as *const Obj)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        close(core(this), true);
        drop(Box::from_raw(this as *mut Obj));
    }
    n
}

unsafe extern "win64" fn d_initialize(this: usize, ctx: usize, handler: usize, _flags: u32) -> i32 {
    if handler == 0 {
        return DPNERR_INVALIDPARAM;
    }
    let c = core(this);
    {
        let mut st = lock(&c.st);
        if st.init {
            return DPNERR_ALREADYINITIALIZED;
        }
        st.init = true;
        st.ctx = ctx;
        st.handler = if super::legacy_com::x86() {
            crate::wow32::callback64(handler as u32)
        } else {
            handler
        };
    }
    start_dispatcher(c);
    DPN_OK
}

/// DPN_SERVICE_PROVIDER_INFO[] : le fournisseur TCP/IP, ou sa carte réseau.
unsafe extern "win64" fn d_enum_service_providers(
    _this: usize,
    sp: usize,
    _app: usize,
    buf: usize,
    size: usize,
    count: usize,
    _flags: u32,
) -> i32 {
    if size == 0 || count == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let (g, name) = if sp == 0 {
        (CLSID_DP8SP_TCPIP, sp_name())
    } else if read_guid(sp) == CLSID_DP8SP_TCPIP {
        (DEVICE_LAN, device_name())
    } else {
        wr32(count, 0);
        wr32(size, 0);
        return DPNERR_DOESNOTEXIST;
    };
    wr32(count, 1);
    give(buf, size, |at| {
        let (o, sz) = layout(SP_INFO);
        let mut b = vec![0u8; sz];
        b[o[1]..o[1] + 16].copy_from_slice(&g);
        let p = (at + sz).to_le_bytes();
        b[o[2]..o[2] + ptr_size()].copy_from_slice(&p[..ptr_size()]);
        b.extend(wide(name));
        b
    })
}

unsafe extern "win64" fn d_cancel(this: usize, h: u32, flags: u32) -> i32 {
    let c = core(this);
    let st = lock(&c.st);
    if h == 0 || flags & DPNCANCEL_ALL != 0 {
        for s in st.enums.values() {
            s.store(true, Ordering::Release);
        }
        return DPN_OK;
    }
    match st.enums.get(&h) {
        Some(s) => {
            s.store(true, Ordering::Release);
            DPN_OK
        }
        None => DPNERR_INVALIDHANDLE,
    }
}

/// EnumHosts (Peer et Client, mêmes arguments).
unsafe fn enum_hosts(
    this: usize,
    desc: usize,
    host: usize,
    _device: usize,
    user: usize,
    user_size: u32,
    count: u32,
    retry: u32,
    timeout: u32,
    ctx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    let c = core(this).clone();
    if !lock(&c.st).init {
        return DPNERR_UNINITIALIZED;
    }
    let app = read_app_desc(desc)
        .map(|d| d.application)
        .unwrap_or([0; 16]);
    let haddr = if host != 0 {
        lock(&addr(host).d).clone()
    } else {
        AddrData::default()
    };
    let targets = targets_for(&haddr);
    let user = read_bytes(user, user_size as usize);
    let count = if count == 0 { 5 } else { count };
    let retry = Duration::from_millis(if retry == 0 { 1500 } else { retry.min(60_000) } as u64);
    let tail = Duration::from_millis(if timeout == 0 {
        1500
    } else {
        timeout.min(60_000)
    } as u64);
    let stop = Arc::new(AtomicBool::new(false));
    let h = c.handle();
    lock(&c.st).enums.insert(h, stop.clone());
    let sync = flags & SYNC != 0;
    let c2 = c.clone();
    let run = move || {
        let mut i = 0u32;
        while (count == u32::MAX || i < count)
            && !stop.load(Ordering::Acquire)
            && !c2.stop.load(Ordering::Acquire)
        {
            let wait = if count != u32::MAX && i + 1 == count {
                tail.max(retry)
            } else {
                retry
            };
            enum_once(&targets, &app, &user, wait, &stop, |a, p, d, data, rtt| {
                c2.post(Ev::EnumResponse(a, p, d, data, ctx, rtt));
            });
            i += 1;
        }
        lock(&c2.st).enums.remove(&h);
        let hr = if stop.load(Ordering::Acquire) {
            DPNERR_USERCANCEL
        } else {
            DPN_OK
        };
        if !sync {
            c2.post(Ev::AsyncDone(h, ctx, hr));
        }
        hr
    };
    if sync {
        let hr = run();
        // Les réponses sont remises avant le retour.
        wait_queue(&c);
        return hr;
    }
    if handle != 0 {
        wr32(handle, h);
    }
    let t = std::thread::spawn(move || {
        run();
    });
    lock(&c.st).threads.push(t);
    DPNSUCCESS_PENDING
}

/// Attend que le fil de remise ait vidé sa file (hors de ce fil).
fn wait_queue(c: &Arc<Core>) {
    if lock(&c.st).dispatcher_id == Some(std::thread::current().id()) {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !lock(&c.q).is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    // Le message en cours de remise.
    drop(lock(&c.call));
}

unsafe fn read_buffers(p: usize, n: u32) -> Vec<u8> {
    let (o, sz) = layout(BUFFER_DESC);
    let mut v = Vec::new();
    for i in 0..n as usize {
        let at = p + i * sz;
        let len = rd32(at + o[0]) as usize;
        v.extend(read_bytes(get_ptr(at + o[1]), len));
    }
    v
}

fn bind_game_port(want: u16) -> Option<(TcpListener, u16)> {
    let first = if want == 0 { DEFAULT_GAME_PORT } else { want };
    for p in first..first.saturating_add(100) {
        if let Ok(l) = TcpListener::bind(("0.0.0.0", p)) {
            return Some((l, p));
        }
        if want != 0 {
            break;
        }
    }
    TcpListener::bind(("0.0.0.0", 0))
        .ok()
        .and_then(|l| l.local_addr().ok().map(|a| (l, a.port())))
}

unsafe fn host(
    this: usize,
    desc: usize,
    devices: usize,
    ndev: u32,
    pctx: usize,
    _flags: u32,
) -> i32 {
    let c = core(this).clone();
    let Some(mut d) = read_app_desc(desc) else {
        return DPNERR_INVALIDPOINTER;
    };
    {
        let st = lock(&c.st);
        if !st.init {
            return DPNERR_UNINITIALIZED;
        }
        if st.hosting || st.connected {
            return DPNERR_ALREADYCONNECTED;
        }
    }
    let mut want = 0u16;
    if devices != 0 && ndev > 0 {
        let a = get_ptr(devices);
        if a != 0 {
            want = lock(&addr(a).d).dword("port").unwrap_or(0) as u16;
        }
    }
    let Some((listener, port)) = bind_game_port(want) else {
        return DPNERR_INVALIDDEVICEADDRESS;
    };
    if d.instance == [0; 16] {
        let mut g = [0u8; 16];
        for (i, x) in g.iter_mut().enumerate() {
            *x = (std::process::id() as u8)
                .wrapping_mul(31)
                .wrapping_add(i as u8 * 7)
                ^ (port as u8);
        }
        g[0..4].copy_from_slice(
            &(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|t| t.subsec_nanos())
                .unwrap_or(0))
            .to_le_bytes(),
        );
        d.instance = g;
    }
    let local = {
        let mut st = lock(&c.st);
        st.desc = d;
        st.hosting = true;
        st.port = port;
        let id = Core::new_id(&mut st);
        st.local = id;
        st.host_id = id;
        let info = st.info.clone();
        st.players.insert(
            id,
            Player {
                name: info.0,
                data: info.1,
                flags: DPNPLAYER_LOCAL | DPNPLAYER_HOST,
                ctx: pctx,
                ..Player::default()
            },
        );
        id
    };
    c.stop.store(false, Ordering::Release);
    let mut threads = Vec::new();
    let c2 = c.clone();
    threads.push(std::thread::spawn(move || accept_loop(c2, listener)));
    for p in [port, enum_port()] {
        if let Ok(s) = UdpSocket::bind(("0.0.0.0", p)) {
            let c3 = c.clone();
            threads.push(std::thread::spawn(move || responder(c3, s)));
        }
    }
    lock(&c.st).threads.extend(threads);
    c.post(Ev::CreatePlayer(local));
    DPN_OK
}

unsafe fn connect(
    this: usize,
    desc: usize,
    host: usize,
    user: usize,
    user_size: u32,
    pctx: usize,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    let c = core(this).clone();
    {
        let st = lock(&c.st);
        if !st.init {
            return DPNERR_UNINITIALIZED;
        }
        if st.hosting || st.connected {
            return DPNERR_ALREADYCONNECTED;
        }
    }
    if host == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let ha = lock(&addr(host).d).clone();
    let Some(hostname) = ha.string("hostname").filter(|h| !h.is_empty()) else {
        return DPNERR_INCOMPLETEADDRESS;
    };
    let port = ha
        .dword("port")
        .map(|p| p as u16)
        .unwrap_or(DEFAULT_GAME_PORT);
    let password = read_app_desc(desc).map(|d| d.password).unwrap_or_default();
    let user = read_bytes(user, user_size as usize);
    let h = c.handle();
    let sync = flags & SYNC != 0;
    let c2 = c.clone();
    let run = move || -> i32 {
        let res = (|| -> Result<(TcpStream, Vec<u8>), (i32, Vec<u8>)> {
            let a = (hostname.as_str(), port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut i| i.find(|a| a.is_ipv4()))
                .ok_or((DPNERR_INVALIDHOSTADDRESS, Vec::new()))?;
            let s = TcpStream::connect_timeout(&a, Duration::from_secs(5))
                .map_err(|_| (DPNERR_NOCONNECTION, Vec::new()))?;
            let _ = s.set_nodelay(true);
            let info = lock(&c2.st).info.clone();
            let mut w = Wr::default();
            w.str(&info.0).bytes(&info.1).bytes(&user).str(&password);
            send_frame(&s, J_JOIN, &w.0);
            let mut rs = s
                .try_clone()
                .map_err(|_| (DPNERR_NOCONNECTION, Vec::new()))?;
            let _ = rs.set_read_timeout(Some(Duration::from_secs(10)));
            match read_frame(&mut rs) {
                Some((J_WELCOME, b)) => {
                    let _ = rs.set_read_timeout(None);
                    lock(&c2.st).host_addr = Some(a);
                    Ok((rs, b))
                }
                Some((J_REFUSE, b)) => {
                    let mut r = Rd::new(&b);
                    let hr = r
                        .u32()
                        .map(|x| x as i32)
                        .unwrap_or(DPNERR_HOSTREJECTEDCONNECTION);
                    Err((hr, r.bytes().unwrap_or_default()))
                }
                _ => Err((DPNERR_TIMEDOUT, Vec::new())),
            }
        })();
        match res {
            Ok((s, b)) => {
                let mut r = Rd::new(&b);
                let (Some(me), Some(hid), Some(d), Some(reply), Some(n)) =
                    (r.u32(), r.u32(), r.desc(), r.bytes(), r.u32())
                else {
                    c2.post(Ev::ConnectComplete(
                        h,
                        actx,
                        DPNERR_NOCONNECTION,
                        Vec::new(),
                        0,
                    ));
                    return DPNERR_NOCONNECTION;
                };
                let mut created = Vec::new();
                {
                    let mut st = lock(&c2.st);
                    st.desc = d;
                    st.local = me;
                    st.host_id = hid;
                    st.connected = true;
                    st.up = s.try_clone().ok();
                    for _ in 0..n {
                        let Some(group) = r.u8() else { break };
                        let Some((id, mut p)) = read_player(&mut r) else {
                            break;
                        };
                        p.owner = r.u32().unwrap_or(0);
                        let nm = r.u32().unwrap_or(0);
                        p.members = (0..nm).filter_map(|_| r.u32()).collect();
                        p.group = group != 0;
                        if id == me {
                            p.flags |= DPNPLAYER_LOCAL;
                            p.ctx = pctx;
                            p.name = st.info.0.clone();
                            p.data = st.info.1.clone();
                        }
                        if id == hid {
                            p.flags |= DPNPLAYER_HOST;
                            p.addr = Some(SocketAddr::new(
                                s.peer_addr()
                                    .map(|a| a.ip())
                                    .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                                port,
                            ));
                        }
                        created.push((id, p.group));
                        st.players.insert(id, p);
                    }
                }
                // Pair : joueur local d'abord, puis les autres ; client : rien.
                if c2.sees_others() {
                    created.sort_by_key(|(id, _)| (*id != me) as u8);
                    for (id, _) in created.iter().filter(|x| !x.1) {
                        c2.post(Ev::CreatePlayer(*id));
                    }
                    for (id, _) in created.iter().filter(|x| x.1) {
                        c2.post(Ev::CreateGroup(*id, 0, None));
                    }
                }
                c2.post(Ev::ConnectComplete(h, actx, DPN_OK, reply, me));
                let c3 = c2.clone();
                let t = std::thread::spawn(move || client_loop(c3, s));
                lock(&c2.st).threads.push(t);
                DPN_OK
            }
            Err((hr, reply)) => {
                c2.post(Ev::ConnectComplete(h, actx, hr, reply, 0));
                hr
            }
        }
    };
    c.stop.store(false, Ordering::Release);
    if sync {
        let hr = run();
        wait_queue(&c);
        return hr;
    }
    if handle != 0 {
        wr32(handle, h);
    }
    let t = std::thread::spawn(move || {
        run();
    });
    lock(&c.st).threads.push(t);
    DPNSUCCESS_PENDING
}

unsafe fn send_to(
    this: usize,
    to: u32,
    bufs: usize,
    nbufs: u32,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    let c = core(this).clone();
    let data = read_buffers(bufs, nbufs);
    let (hosting, connected, local, up) = {
        let st = lock(&c.st);
        (
            st.hosting,
            st.connected,
            st.local,
            st.up.as_ref().and_then(|s| s.try_clone().ok()),
        )
    };
    let hr = if hosting {
        route(&c, local, to, flags, &data)
    } else if connected {
        // Envoi vers soi traité ici, le reste par l'hôte.
        let to_self = to == local || (to == 0 && flags & DPNSEND_NOLOOPBACK == 0);
        if to_self {
            c.post(Ev::Receive(local, data.clone()));
        }
        if to != local {
            let mut w = Wr::default();
            w.u32(to).u32(flags | DPNSEND_NOLOOPBACK).bytes(&data);
            match up {
                Some(s) => {
                    if send_frame(&s, J_SEND, &w.0) {
                        DPN_OK
                    } else {
                        DPNERR_CONNECTIONLOST
                    }
                }
                None => DPNERR_NOCONNECTION,
            }
        } else {
            DPN_OK
        }
    } else {
        DPNERR_NOCONNECTION
    };
    if hr < 0 {
        return hr;
    }
    if flags & SYNC != 0 {
        return DPN_OK;
    }
    let h = c.handle();
    if handle != 0 {
        wr32(handle, h);
    }
    if flags & DPNSEND_NOCOMPLETE == 0 {
        c.post(Ev::SendComplete(h, actx, DPN_OK));
    }
    DPNSUCCESS_PENDING
}

/// Fin d'une opération asynchrone qui s'est faite tout de suite.
unsafe fn done_now(c: &Arc<Core>, hr: i32, actx: usize, handle: usize, flags: u32) -> i32 {
    if hr < 0 || flags & SYNC != 0 {
        return hr;
    }
    let h = c.handle();
    if handle != 0 {
        wr32(handle, h);
    }
    c.post(Ev::AsyncDone(h, actx, hr));
    DPNSUCCESS_PENDING
}

unsafe fn read_info(p: usize) -> Option<(u32, String, Vec<u8>)> {
    if p == 0 {
        return None;
    }
    let (o, _) = layout(PLAYER_INFO);
    let flags = rd32(p + o[1]);
    Some((
        flags,
        read_wstr(get_ptr(p + o[2])),
        read_bytes(get_ptr(p + o[3]), rd32(p + o[4]) as usize),
    ))
}

unsafe fn set_info(this: usize, info: usize, actx: usize, handle: usize, flags: u32) -> i32 {
    let c = core(this).clone();
    let Some((which, name, data)) = read_info(info) else {
        return DPNERR_INVALIDPOINTER;
    };
    let (hosting, connected, local, up) = {
        let mut st = lock(&c.st);
        if which & DPNINFO_NAME != 0 {
            st.info.0 = name;
        }
        if which & DPNINFO_DATA != 0 {
            st.info.1 = data;
        }
        let info = st.info.clone();
        let local = st.local;
        if let Some(p) = st.players.get_mut(&local) {
            p.name = info.0;
            p.data = info.1;
        }
        (
            st.hosting,
            st.connected,
            st.local,
            st.up.as_ref().and_then(|s| s.try_clone().ok()),
        )
    };
    if hosting {
        let st = lock(&c.st);
        if let Some(p) = st.players.get(&local) {
            let b = player_body(local, p);
            broadcast(&st, J_INFO, &b, 0);
        }
        drop(st);
        c.post(Ev::Info(local));
    } else if connected {
        let info = lock(&c.st).info.clone();
        let mut w = Wr::default();
        w.str(&info.0).bytes(&info.1);
        if let Some(s) = up {
            send_frame(&s, R_INFO, &w.0);
        }
    }
    done_now(&c, DPN_OK, actx, handle, flags)
}

unsafe fn get_info(this: usize, id: Option<u32>, out: usize, size: usize) -> i32 {
    let c = core(this);
    let st = lock(&c.st);
    let id = id.unwrap_or(st.host_id);
    let Some(p) = st.players.get(&id).filter(|p| !p.gone) else {
        return DPNERR_INVALIDPLAYER;
    };
    let p = p.clone();
    drop(st);
    give(out, size, |at| {
        let (o, sz) = layout(PLAYER_INFO);
        let mut b = vec![0u8; sz];
        let ps = ptr_size();
        let name = wide(&p.name);
        let name_at = at + sz;
        let data_at = name_at + name.len();
        b[o[0]..o[0] + 4].copy_from_slice(&(sz as u32).to_le_bytes());
        b[o[1]..o[1] + 4].copy_from_slice(&(DPNINFO_NAME | DPNINFO_DATA).to_le_bytes());
        b[o[2]..o[2] + ps].copy_from_slice(&name_at.to_le_bytes()[..ps]);
        let dp = if p.data.is_empty() { 0 } else { data_at };
        b[o[3]..o[3] + ps].copy_from_slice(&dp.to_le_bytes()[..ps]);
        b[o[4]..o[4] + 4].copy_from_slice(&(p.data.len() as u32).to_le_bytes());
        b[o[5]..o[5] + 4].copy_from_slice(&p.flags.to_le_bytes());
        b.extend(name);
        b.extend(&p.data);
        b
    })
}

unsafe fn get_app_desc(this: usize, out: usize, size: usize) -> i32 {
    let c = core(this);
    let st = lock(&c.st);
    if !st.hosting && !st.connected {
        return DPNERR_NOCONNECTION;
    }
    let mut d = st.desc.clone();
    d.current = st.players.values().filter(|p| !p.group && !p.gone).count() as u32;
    drop(st);
    give(out, size, |at| app_desc_bytes(&d, at))
}

unsafe fn set_app_desc(this: usize, desc: usize) -> i32 {
    let c = core(this).clone();
    let Some(d) = read_app_desc(desc) else {
        return DPNERR_INVALIDPOINTER;
    };
    let mut st = lock(&c.st);
    if !st.hosting {
        return DPNERR_NOTHOST;
    }
    let instance = st.desc.instance;
    st.desc = AppDesc { instance, ..d };
    let mut w = Wr::default();
    w.desc(&st.desc);
    broadcast(&st, J_APPDESC, &w.0, 0);
    drop(st);
    c.post(Ev::AppDesc);
    DPN_OK
}

unsafe fn group_request(c: &Arc<Core>, kind: u8, body: &[u8]) -> i32 {
    let st = lock(&c.st);
    match st.up.as_ref() {
        Some(s) => {
            send_frame(s, kind, body);
            DPN_OK
        }
        None => DPNERR_NOCONNECTION,
    }
}

unsafe fn create_group(
    this: usize,
    info: usize,
    gctx: usize,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    let c = core(this).clone();
    let (_, name, data) = read_info(info).unwrap_or_default();
    let (hosting, connected, local, token) = {
        let mut st = lock(&c.st);
        st.next_handle += 1;
        let t = st.next_handle;
        st.group_ctx.insert(t, gctx);
        (st.hosting, st.connected, st.local, t)
    };
    let hr = if hosting {
        host_create_group(&c, local, token, name, data);
        DPN_OK
    } else if connected {
        let mut w = Wr::default();
        w.u32(token).str(&name).bytes(&data);
        group_request(&c, R_GROUP, &w.0)
    } else {
        DPNERR_CANTCREATEGROUP
    };
    done_now(&c, hr, actx, handle, flags)
}

unsafe fn group_op(
    this: usize,
    kind: u8,
    g: u32,
    p: u32,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    let c = core(this).clone();
    let hosting = lock(&c.st).hosting;
    let hr = if hosting {
        match kind {
            R_GROUP_DEL => host_destroy_group(&c, g),
            R_MEMBER => host_member(&c, g, p, true),
            _ => host_member(&c, g, p, false),
        }
    } else {
        {
            let st = lock(&c.st);
            if !st.players.get(&g).is_some_and(|x| x.group && !x.gone) {
                return DPNERR_INVALIDGROUP;
            }
        }
        let mut w = Wr::default();
        w.u32(g);
        if kind != R_GROUP_DEL {
            w.u32(p);
        }
        group_request(&c, kind, &w.0)
    };
    done_now(&c, hr, actx, handle, flags)
}

unsafe fn set_group_info(
    this: usize,
    g: u32,
    info: usize,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    let c = core(this).clone();
    let Some((_, name, data)) = read_info(info) else {
        return DPNERR_INVALIDPOINTER;
    };
    let hosting = lock(&c.st).hosting;
    let hr = if hosting {
        host_group_info(&c, g, name, data)
    } else {
        let mut w = Wr::default();
        w.u32(g).str(&name).bytes(&data);
        group_request(&c, R_GROUP_INFO, &w.0)
    };
    done_now(&c, hr, actx, handle, flags)
}

unsafe fn give_ids(ids: Vec<u32>, out: usize, count: usize) -> i32 {
    if count == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let have = rd32(count) as usize;
    wr32(count, ids.len() as u32);
    if have < ids.len() || (out == 0 && !ids.is_empty()) {
        return DPNERR_BUFFERTOOSMALL;
    }
    for (i, id) in ids.iter().enumerate() {
        wr32(out + 4 * i, *id);
    }
    DPN_OK
}

unsafe fn enum_players(this: usize, out: usize, count: usize, flags: u32) -> i32 {
    let c = core(this);
    let st = lock(&c.st);
    let flags = if flags == 0 { DPNENUM_PLAYERS } else { flags };
    let ids: Vec<u32> = st
        .players
        .iter()
        .filter(|(_, p)| !p.gone)
        .filter(|(_, p)| {
            if p.group {
                flags & DPNENUM_GROUPS != 0
            } else {
                flags & DPNENUM_PLAYERS != 0
            }
        })
        .map(|(i, _)| *i)
        .collect();
    drop(st);
    give_ids(ids, out, count)
}

unsafe fn enum_members(this: usize, g: u32, out: usize, count: usize) -> i32 {
    let c = core(this);
    let st = lock(&c.st);
    let Some(grp) = st.players.get(&g).filter(|p| p.group && !p.gone) else {
        return DPNERR_INVALIDGROUP;
    };
    let ids = grp.members.clone();
    drop(st);
    give_ids(ids, out, count)
}

unsafe fn get_context(this: usize, id: u32, out: usize, group: bool) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let c = core(this);
    let st = lock(&c.st);
    match st.players.get(&id) {
        Some(p) if p.group == group => {
            put_ptr(out, p.ctx);
            DPN_OK
        }
        _ if group => DPNERR_INVALIDGROUP,
        _ => DPNERR_INVALIDPLAYER,
    }
}

unsafe fn get_address(this: usize, id: Option<u32>, out: usize) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let c = core(this);
    let st = lock(&c.st);
    let id = id.unwrap_or(st.host_id);
    let a = match st.players.get(&id) {
        Some(p) if p.flags & DPNPLAYER_LOCAL != 0 => ip_addr("127.0.0.1", st.port),
        Some(p) => match p.addr.or(st.host_addr) {
            Some(a) => ip_addr(&a.ip().to_string(), a.port()),
            None => return DPNERR_INVALIDPLAYER,
        },
        None => return DPNERR_INVALIDPLAYER,
    };
    put_ptr(out, new_addr(a));
    DPN_OK
}

unsafe fn local_host_addresses(this: usize, out: usize, count: usize) -> i32 {
    if count == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let c = core(this);
    let (hosting, port) = {
        let st = lock(&c.st);
        (st.hosting, st.port)
    };
    if !hosting {
        return DPNERR_NOTHOST;
    }
    let have = rd32(count);
    wr32(count, 1);
    if have < 1 || out == 0 {
        return DPNERR_BUFFERTOOSMALL;
    }
    // Adresse locale vue du réseau (route par défaut), sinon la boucle locale.
    let ip = UdpSocket::bind(("0.0.0.0", 0))
        .ok()
        .and_then(|s| {
            s.connect(("192.0.2.1", 9))
                .ok()
                .and_then(|_| s.local_addr().ok())
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".into());
    put_ptr(out, new_addr(ip_addr(&ip, port)));
    DPN_OK
}

/// Fin de session : messages DESTROY_*, arrêt des fils.
fn close(c: &Arc<Core>, wait: bool) {
    let (hosting, conns, up, ids, threads) = {
        let mut st = lock(&c.st);
        if !st.init {
            return;
        }
        let ids: Vec<(u32, bool)> = st
            .players
            .iter_mut()
            .filter(|(_, p)| !p.gone)
            .map(|(i, p)| {
                p.gone = true;
                (*i, p.group)
            })
            .collect();
        let conns: Vec<TcpStream> = st.conns.drain().map(|(_, s)| s).collect();
        let r = (
            st.hosting,
            conns,
            st.up.take(),
            ids,
            std::mem::take(&mut st.threads),
        );
        st.hosting = false;
        st.connected = false;
        for s in st.enums.values() {
            s.store(true, Ordering::Release);
        }
        r
    };
    c.stop.store(true, Ordering::Release);
    if hosting {
        let mut w = Wr::default();
        w.bytes(&[]);
        for s in &conns {
            send_frame(s, J_TERMINATE, &w.0);
        }
    }
    for s in conns.iter().chain(up.iter()) {
        let _ = s.shutdown(Shutdown::Both);
    }
    // Joueurs et groupes, puis les groupes après leurs membres.
    for (i, _) in ids.iter().filter(|x| !x.1) {
        c.post(Ev::DestroyPlayer(*i, DESTROY_NORMAL));
    }
    for (i, _) in ids.iter().filter(|x| x.1) {
        c.post(Ev::DestroyGroup(*i, DESTROY_NORMAL));
    }
    let on_dispatcher = lock(&c.st).dispatcher_id == Some(std::thread::current().id());
    if wait && !on_dispatcher {
        wait_queue(c);
        for t in threads {
            let _ = t.join();
        }
        c.post(Ev::Stop);
        let d = lock(&c.st).dispatcher.take();
        if let Some(d) = d {
            let _ = d.join();
        }
        let mut st = lock(&c.st);
        st.init = false;
        st.dispatcher_id = None;
        st.players.clear();
        st.buffers.clear();
    } else {
        let mut st = lock(&c.st);
        st.init = false;
        st.players.clear();
    }
}

unsafe fn destroy_player(this: usize, id: u32, data: usize, size: u32) -> i32 {
    let c = core(this).clone();
    let mut st = lock(&c.st);
    if !st.hosting {
        return DPNERR_NOTHOST;
    }
    if id == st.local {
        return DPNERR_INVALIDPLAYER;
    }
    let Some(s) = st.conns.remove(&id) else {
        return DPNERR_INVALIDPLAYER;
    };
    let mut w = Wr::default();
    w.bytes(&read_bytes(data, size as usize));
    send_frame(&s, J_TERMINATE, &w.0);
    let _ = s.shutdown(Shutdown::Both);
    if let Some(p) = st.players.get_mut(&id) {
        p.gone = true;
    }
    for g in st.players.values_mut().filter(|g| g.group) {
        g.members.retain(|m| *m != id);
    }
    if c.sees_others() {
        let mut w = Wr::default();
        w.u32(id).u32(DESTROY_HOSTDESTROYED);
        broadcast(&st, J_DEL, &w.0, id);
    }
    drop(st);
    c.post(Ev::DestroyPlayer(id, DESTROY_HOSTDESTROYED));
    DPN_OK
}

unsafe fn terminate(this: usize, data: usize, size: u32) -> i32 {
    let c = core(this).clone();
    let ids: Vec<u32> = {
        let st = lock(&c.st);
        if !st.hosting {
            return DPNERR_NOTHOST;
        }
        st.conns.keys().copied().collect()
    };
    for id in ids {
        destroy_player(this, id, data, size);
    }
    c.post(Ev::Terminate(
        DPNERR_HOSTTERMINATEDSESSION,
        read_bytes(data, size as usize),
    ));
    DPN_OK
}

unsafe fn return_buffer(this: usize, h: u32) -> i32 {
    match lock(&core(this).st).buffers.remove(&h) {
        Some(_) => DPN_OK,
        None => DPNERR_INVALIDHANDLE,
    }
}

unsafe fn get_caps(caps: usize) -> i32 {
    if caps == 0 || rd32(caps) < 20 {
        return DPNERR_INVALIDPARAM;
    }
    wr32(caps + 4, 0);
    wr32(caps + 8, 200);
    wr32(caps + 12, 14);
    wr32(caps + 16, 60_000);
    DPN_OK
}
unsafe fn get_sp_caps(sp: usize, caps: usize) -> i32 {
    if caps == 0 || rd32(caps) < 36 {
        return DPNERR_INVALIDPARAM;
    }
    if sp == 0 || read_guid(sp) != CLSID_DP8SP_TCPIP {
        return DPNERR_DOESNOTEXIST;
    }
    for (i, v) in [0x30u32, 4, 5, 1500, 1500, 1000, 3, 8192]
        .iter()
        .enumerate()
    {
        wr32(caps + 4 + 4 * i, *v);
    }
    DPN_OK
}
unsafe fn zero_sized(p: usize) -> i32 {
    if p == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let n = rd32(p) as usize;
    if n > 4 {
        std::ptr::write_bytes((p + 4) as *mut u8, 0, n - 4);
    }
    DPN_OK
}
unsafe fn send_queue_info(n: usize, b: usize) -> i32 {
    if n != 0 {
        wr32(n, 0);
    }
    if b != 0 {
        wr32(b, 0);
    }
    DPN_OK
}

// ── IDirectPlay8Peer ──

unsafe extern "win64" fn p_connect(
    this: usize,
    desc: usize,
    host: usize,
    _device: usize,
    _sec: usize,
    _cred: usize,
    user: usize,
    user_size: u32,
    pctx: usize,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    connect(this, desc, host, user, user_size, pctx, actx, handle, flags)
}
unsafe extern "win64" fn p_send_to(
    this: usize,
    to: u32,
    bufs: usize,
    n: u32,
    _timeout: u32,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    send_to(this, to, bufs, n, actx, handle, flags)
}
unsafe extern "win64" fn p_get_send_queue_info(
    _this: usize,
    _id: u32,
    n: usize,
    b: usize,
    _f: u32,
) -> i32 {
    send_queue_info(n, b)
}
unsafe extern "win64" fn p_host(
    this: usize,
    desc: usize,
    devs: usize,
    ndev: u32,
    _sec: usize,
    _cred: usize,
    pctx: usize,
    flags: u32,
) -> i32 {
    host(this, desc, devs, ndev, pctx, flags)
}
unsafe extern "win64" fn p_get_app_desc(this: usize, out: usize, size: usize, _f: u32) -> i32 {
    get_app_desc(this, out, size)
}
unsafe extern "win64" fn p_set_app_desc(this: usize, d: usize, _f: u32) -> i32 {
    set_app_desc(this, d)
}
unsafe extern "win64" fn p_create_group(
    this: usize,
    info: usize,
    gctx: usize,
    actx: usize,
    h: usize,
    f: u32,
) -> i32 {
    create_group(this, info, gctx, actx, h, f)
}
unsafe extern "win64" fn p_destroy_group(
    this: usize,
    g: u32,
    actx: usize,
    h: usize,
    f: u32,
) -> i32 {
    group_op(this, R_GROUP_DEL, g, 0, actx, h, f)
}
unsafe extern "win64" fn p_add_to_group(
    this: usize,
    g: u32,
    p: u32,
    actx: usize,
    h: usize,
    f: u32,
) -> i32 {
    group_op(this, R_MEMBER, g, p, actx, h, f)
}
unsafe extern "win64" fn p_remove_from_group(
    this: usize,
    g: u32,
    p: u32,
    actx: usize,
    h: usize,
    f: u32,
) -> i32 {
    group_op(this, R_MEMBER_DEL, g, p, actx, h, f)
}
unsafe extern "win64" fn p_set_group_info(
    this: usize,
    g: u32,
    info: usize,
    actx: usize,
    h: usize,
    f: u32,
) -> i32 {
    set_group_info(this, g, info, actx, h, f)
}
unsafe extern "win64" fn p_get_group_info(
    this: usize,
    g: u32,
    out: usize,
    size: usize,
    _f: u32,
) -> i32 {
    get_info(this, Some(g), out, size)
}
unsafe extern "win64" fn p_enum_players(this: usize, out: usize, count: usize, f: u32) -> i32 {
    enum_players(this, out, count, f)
}
unsafe extern "win64" fn p_enum_members(
    this: usize,
    g: u32,
    out: usize,
    count: usize,
    _f: u32,
) -> i32 {
    enum_members(this, g, out, count)
}
unsafe extern "win64" fn p_set_info(
    this: usize,
    info: usize,
    actx: usize,
    h: usize,
    f: u32,
) -> i32 {
    set_info(this, info, actx, h, f)
}
unsafe extern "win64" fn p_get_info(this: usize, id: u32, out: usize, size: usize, _f: u32) -> i32 {
    get_info(this, Some(id), out, size)
}
unsafe extern "win64" fn p_get_address(this: usize, id: u32, out: usize, _f: u32) -> i32 {
    get_address(this, Some(id), out)
}
unsafe extern "win64" fn p_local_host_addresses(
    this: usize,
    out: usize,
    count: usize,
    _f: u32,
) -> i32 {
    local_host_addresses(this, out, count)
}
unsafe extern "win64" fn p_close(this: usize, _f: u32) -> i32 {
    close(core(this), true);
    DPN_OK
}
unsafe extern "win64" fn p_enum_hosts(
    this: usize,
    desc: usize,
    host: usize,
    dev: usize,
    user: usize,
    size: u32,
    count: u32,
    retry: u32,
    timeout: u32,
    ctx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    enum_hosts(
        this, desc, host, dev, user, size, count, retry, timeout, ctx, handle, flags,
    )
}
unsafe extern "win64" fn p_destroy_peer(
    this: usize,
    id: u32,
    data: usize,
    size: u32,
    _f: u32,
) -> i32 {
    destroy_player(this, id, data, size)
}
unsafe extern "win64" fn p_return_buffer(this: usize, h: u32, _f: u32) -> i32 {
    return_buffer(this, h)
}
unsafe extern "win64" fn p_get_player_context(this: usize, id: u32, out: usize, _f: u32) -> i32 {
    get_context(this, id, out, false)
}
unsafe extern "win64" fn p_get_group_context(this: usize, id: u32, out: usize, _f: u32) -> i32 {
    get_context(this, id, out, true)
}
unsafe extern "win64" fn p_get_caps(_this: usize, caps: usize, _f: u32) -> i32 {
    get_caps(caps)
}
unsafe extern "win64" fn p_set_caps(_this: usize, _caps: usize, _f: u32) -> i32 {
    DPN_OK
}
unsafe extern "win64" fn p_set_sp_caps(_this: usize, _sp: usize, _caps: usize, _f: u32) -> i32 {
    DPN_OK
}
unsafe extern "win64" fn p_get_sp_caps(_this: usize, sp: usize, caps: usize, _f: u32) -> i32 {
    get_sp_caps(sp, caps)
}
unsafe extern "win64" fn p_get_connection_info(
    _this: usize,
    _id: u32,
    info: usize,
    _f: u32,
) -> i32 {
    zero_sized(info)
}
unsafe extern "win64" fn p_register_lobby(_this: usize, _h: u32, _app: usize, _f: u32) -> i32 {
    DPN_OK
}
unsafe extern "win64" fn p_terminate(this: usize, data: usize, size: u32, _f: u32) -> i32 {
    terminate(this, data, size)
}

// ── IDirectPlay8Client ──

unsafe extern "win64" fn c_connect(
    this: usize,
    desc: usize,
    host: usize,
    _device: usize,
    _sec: usize,
    _cred: usize,
    user: usize,
    user_size: u32,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    connect(this, desc, host, user, user_size, 0, actx, handle, flags)
}
unsafe extern "win64" fn c_send(
    this: usize,
    bufs: usize,
    n: u32,
    _timeout: u32,
    actx: usize,
    handle: usize,
    flags: u32,
) -> i32 {
    let to = lock(&core(this).st).host_id;
    send_to(this, to, bufs, n, actx, handle, flags | DPNSEND_NOLOOPBACK)
}
unsafe extern "win64" fn c_get_send_queue_info(_this: usize, n: usize, b: usize, _f: u32) -> i32 {
    send_queue_info(n, b)
}
unsafe extern "win64" fn c_get_server_info(this: usize, out: usize, size: usize, _f: u32) -> i32 {
    get_info(this, None, out, size)
}
unsafe extern "win64" fn c_get_server_address(this: usize, out: usize, _f: u32) -> i32 {
    get_address(this, None, out)
}
unsafe extern "win64" fn c_get_connection_info(_this: usize, info: usize, _f: u32) -> i32 {
    zero_sized(info)
}

// ── IDirectPlay8Server ──

unsafe extern "win64" fn s_get_client_address(this: usize, id: u32, out: usize, _f: u32) -> i32 {
    get_address(this, Some(id), out)
}

fn peer_vtable() -> usize {
    vtable_sig(
        "IDirectPlay8Peer",
        &[
            (f(d_qi as *const ()), "ii"),
            (f(d_addref as *const ()), ""),
            (f(d_release as *const ()), ""),
            (f(d_initialize as *const ()), "iii"),
            (f(d_enum_service_providers as *const ()), "iiiiii"),
            (f(d_cancel as *const ()), "ii"),
            (f(p_connect as *const ()), "iiiiiiiiiii"),
            (f(p_send_to as *const ()), "iiiiiii"),
            (f(p_get_send_queue_info as *const ()), "iiii"),
            (f(p_host as *const ()), "iiiiiii"),
            (f(p_get_app_desc as *const ()), "iii"),
            (f(p_set_app_desc as *const ()), "ii"),
            (f(p_create_group as *const ()), "iiiii"),
            (f(p_destroy_group as *const ()), "iiii"),
            (f(p_add_to_group as *const ()), "iiiii"),
            (f(p_remove_from_group as *const ()), "iiiii"),
            (f(p_set_group_info as *const ()), "iiiii"),
            (f(p_get_group_info as *const ()), "iiii"),
            (f(p_enum_players as *const ()), "iii"),
            (f(p_enum_members as *const ()), "iiii"),
            (f(p_set_info as *const ()), "iiii"),
            (f(p_get_info as *const ()), "iiii"),
            (f(p_get_address as *const ()), "iii"),
            (f(p_local_host_addresses as *const ()), "iii"),
            (f(p_close as *const ()), "i"),
            (f(p_enum_hosts as *const ()), "iiiiiiiiiii"),
            (f(p_destroy_peer as *const ()), "iiii"),
            (f(p_return_buffer as *const ()), "ii"),
            (f(p_get_player_context as *const ()), "iii"),
            (f(p_get_group_context as *const ()), "iii"),
            (f(p_get_caps as *const ()), "ii"),
            (f(p_set_caps as *const ()), "ii"),
            (f(p_set_sp_caps as *const ()), "iii"),
            (f(p_get_sp_caps as *const ()), "iii"),
            (f(p_get_connection_info as *const ()), "iii"),
            (f(p_register_lobby as *const ()), "iii"),
            (f(p_terminate as *const ()), "iii"),
        ],
    )
}

fn client_vtable() -> usize {
    vtable_sig(
        "IDirectPlay8Client",
        &[
            (f(d_qi as *const ()), "ii"),
            (f(d_addref as *const ()), ""),
            (f(d_release as *const ()), ""),
            (f(d_initialize as *const ()), "iii"),
            (f(d_enum_service_providers as *const ()), "iiiiii"),
            (f(p_enum_hosts as *const ()), "iiiiiiiiiii"),
            (f(d_cancel as *const ()), "ii"),
            (f(c_connect as *const ()), "iiiiiiiiii"),
            (f(c_send as *const ()), "iiiiii"),
            (f(c_get_send_queue_info as *const ()), "iii"),
            (f(p_get_app_desc as *const ()), "iii"),
            (f(p_set_info as *const ()), "iiii"),
            (f(c_get_server_info as *const ()), "iii"),
            (f(c_get_server_address as *const ()), "ii"),
            (f(p_close as *const ()), "i"),
            (f(p_return_buffer as *const ()), "ii"),
            (f(p_get_caps as *const ()), "ii"),
            (f(p_set_caps as *const ()), "ii"),
            (f(p_set_sp_caps as *const ()), "iii"),
            (f(p_get_sp_caps as *const ()), "iii"),
            (f(c_get_connection_info as *const ()), "ii"),
            (f(p_register_lobby as *const ()), "iii"),
        ],
    )
}

fn server_vtable() -> usize {
    vtable_sig(
        "IDirectPlay8Server",
        &[
            (f(d_qi as *const ()), "ii"),
            (f(d_addref as *const ()), ""),
            (f(d_release as *const ()), ""),
            (f(d_initialize as *const ()), "iii"),
            (f(d_enum_service_providers as *const ()), "iiiiii"),
            (f(d_cancel as *const ()), "ii"),
            (f(p_get_send_queue_info as *const ()), "iiii"),
            (f(p_get_app_desc as *const ()), "iii"),
            (f(p_set_info as *const ()), "iiii"),
            (f(p_get_info as *const ()), "iiii"),
            (f(s_get_client_address as *const ()), "iii"),
            (f(p_local_host_addresses as *const ()), "iii"),
            (f(p_set_app_desc as *const ()), "ii"),
            (f(p_host as *const ()), "iiiiiii"),
            (f(p_send_to as *const ()), "iiiiiii"),
            (f(p_create_group as *const ()), "iiiii"),
            (f(p_destroy_group as *const ()), "iiii"),
            (f(p_add_to_group as *const ()), "iiiii"),
            (f(p_remove_from_group as *const ()), "iiiii"),
            (f(p_set_group_info as *const ()), "iiiii"),
            (f(p_get_group_info as *const ()), "iiii"),
            (f(p_enum_players as *const ()), "iii"),
            (f(p_enum_members as *const ()), "iiii"),
            (f(p_close as *const ()), "i"),
            (f(p_destroy_peer as *const ()), "iiii"),
            (f(p_return_buffer as *const ()), "ii"),
            (f(p_get_player_context as *const ()), "iii"),
            (f(p_get_group_context as *const ()), "iii"),
            (f(p_get_caps as *const ()), "ii"),
            (f(p_set_caps as *const ()), "ii"),
            (f(p_set_sp_caps as *const ()), "iii"),
            (f(p_get_sp_caps as *const ()), "iii"),
            (f(p_get_connection_info as *const ()), "iii"),
            (f(p_register_lobby as *const ()), "iii"),
        ],
    )
}

fn new_obj(role: Role) -> usize {
    let vtbl = match role {
        Role::Peer => peer_vtable(),
        Role::Client => client_vtable(),
        Role::Server => server_vtable(),
    };
    Box::into_raw(Box::new(Obj {
        vtbl,
        refs: AtomicU32::new(1),
        core: Core::new(role),
    })) as usize
}

/// Création par classe (CoCreateInstance, DirectPlay8Create).
unsafe fn create(clsid: &Guid, iid: *const Guid, out: usize) -> Option<i32> {
    let obj = if *clsid == CLSID_DIRECTPLAY8PEER {
        new_obj(Role::Peer)
    } else if *clsid == CLSID_DIRECTPLAY8CLIENT {
        new_obj(Role::Client)
    } else if *clsid == CLSID_DIRECTPLAY8SERVER {
        new_obj(Role::Server)
    } else if *clsid == CLSID_DIRECTPLAY8ADDRESS {
        let a = new_addr(AddrData::default());
        let hr = a_qi(a, iid, out);
        a_release(a);
        return Some(hr);
    } else {
        return None;
    };
    let hr = d_qi(obj, iid, out);
    d_release(obj);
    Some(hr)
}

pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut std::ffi::c_void,
) -> Option<i32> {
    let clsid: &Guid = &*(clsid as *const super::ole32::Guid as *const Guid);
    let iid = iid as *const super::ole32::Guid as *const Guid;
    let mut p: usize = 0;
    let hr = create(clsid, iid, &mut p as *mut usize as usize)?;
    *out = p as *mut std::ffi::c_void;
    Some(hr)
}

/// DirectPlay8Create(pcIID, ppvInterface, pUnknown) (DirectX 9).
unsafe extern "win64" fn direct_play8_create(iid: *const Guid, out: usize, _outer: usize) -> i32 {
    if iid.is_null() || out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    let clsid = match *iid {
        IID_IDIRECTPLAY8PEER => CLSID_DIRECTPLAY8PEER,
        IID_IDIRECTPLAY8CLIENT => CLSID_DIRECTPLAY8CLIENT,
        IID_IDIRECTPLAY8SERVER => CLSID_DIRECTPLAY8SERVER,
        IID_IDIRECTPLAY8ADDRESS => CLSID_DIRECTPLAY8ADDRESS,
        _ => return DPNERR_NOINTERFACE,
    };
    create(&clsid, iid, out).unwrap_or(DPNERR_GENERIC)
}
/// DirectPlay8AddressCreate(pcIID, ppvInterface, pUnknown).
unsafe extern "win64" fn direct_play8_address_create(
    iid: *const Guid,
    out: usize,
    _outer: usize,
) -> i32 {
    if out == 0 {
        return DPNERR_INVALIDPOINTER;
    }
    create(&CLSID_DIRECTPLAY8ADDRESS, iid, out).unwrap_or(DPNERR_GENERIC)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DirectPlay8Create" => fn direct_play8_create,
        "DirectPlay8AddressCreate" => fn direct_play8_address_create,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_aller_retour() {
        let mut a = ip_addr("192.168.1.20", 2302);
        a.user = b"ab c".to_vec();
        let u = a.url();
        assert!(u.starts_with("x-directplay:/provider=%7BEBFE7BA0-628D-11D2-AE0F-006097B01411%7D;"));
        assert!(u.contains("hostname=192.168.1.20;port=2302"), "{u}");
        let b = AddrData::parse(&u).unwrap();
        assert_eq!(b.string("hostname").as_deref(), Some("192.168.1.20"));
        assert_eq!(b.dword("port"), Some(2302));
        assert_eq!(b.user, b"ab c");
        assert_eq!(b.get("provider").unwrap().2, CLSID_DP8SP_TCPIP.to_vec());
        assert!(AddrData::parse("http://x").is_none());
    }

    #[test]
    fn description_et_protocole() {
        let d = AppDesc {
            flags: 4,
            instance: [1; 16],
            application: [2; 16],
            max_players: 8,
            current: 1,
            name: "Partie de Damien".into(),
            password: String::new(),
            reserved: vec![],
            app_reserved: vec![9, 9],
        };
        let mut w = Wr::default();
        w.desc(&d);
        assert_eq!(Rd::new(&w.0).desc().unwrap(), d);
        let (o, size) = layout(APP_DESC);
        if ptr_size() == 8 {
            assert_eq!((o[6], o[10], size), (48, 80, 96));
        }
        let mut buf = vec![0u8; 256];
        let at = buf.as_mut_ptr() as usize;
        let b = app_desc_bytes(&d, at);
        buf[..b.len()].copy_from_slice(&b);
        let back = unsafe { read_app_desc(at) }.unwrap();
        assert_eq!(back, d);
    }
}
