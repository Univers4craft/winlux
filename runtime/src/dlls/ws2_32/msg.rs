//! WSARecvMsg et WSASendMsg, fonctions d'extension obtenues par
//! WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER). Qt Network les demande sur
//! chaque socket et appelle le pointeur sans le vérifier (iCUE, #1629).
//!
//! WSAMSG (ws2def.h) : `name`, `namelen`, `lpBuffers`, `dwBufferCount`,
//! `Control` (WSABUF), `dwFlags` ; décalages 0/8/16/24/32/48 en x64,
//! 0/4/8/12/16/24 en x86. Les données de contrôle Windows (WSACMSGHDR :
//! `SIZE_T cmsg_len; INT cmsg_level; INT cmsg_type;`, données alignées sur
//! un pointeur) sont traduites vers et depuis celles de Linux pour les
//! options que Qt pose : IP_PKTINFO, IPV6_PKTINFO et les limites de sauts.

use super::{addr_in, addr_out, async_arm, last_error, read_wsabuf, set_error, valid_socket};
use super::{FD_OOB, FD_READ, WSAEFAULT, WSAENOTSOCK};
use std::ffi::c_void;

const SOCKET_ERROR: i32 = -1;
const WIN_IPPROTO_IP: i32 = 0;
const WIN_IPPROTO_IPV6: i32 = 41;
/// IP_PKTINFO et IPV6_PKTINFO valent 19 sous Windows (ws2ipdef.h).
const WIN_PKTINFO: i32 = 19;
/// IP_HOPLIMIT et IPV6_HOPLIMIT valent 21 sous Windows.
const WIN_HOPLIMIT: i32 = 21;
/// Drapeaux de sortie de WSAMSG.dwFlags (winsock2.h).
const WIN_MSG_TRUNC: u32 = 0x0100;
const WIN_MSG_CTRUNC: u32 = 0x0200;

struct WsaMsg {
    name: usize,
    namelen_at: usize,
    bufs: usize,
    count: usize,
    control_len_at: usize,
    control_buf: usize,
    flags_at: usize,
}

unsafe fn layout(msg: usize) -> WsaMsg {
    let ps = crate::wow32::pointer_size();
    let rd = |at: usize| {
        if ps == 4 {
            std::ptr::read_unaligned(at as *const u32) as usize
        } else {
            std::ptr::read_unaligned(at as *const u64) as usize
        }
    };
    let rd32 = |at: usize| std::ptr::read_unaligned(at as *const u32) as usize;
    if ps == 4 {
        WsaMsg {
            name: rd(msg),
            namelen_at: msg + 4,
            bufs: rd(msg + 8),
            count: rd32(msg + 12),
            control_len_at: msg + 16,
            control_buf: rd(msg + 20),
            flags_at: msg + 24,
        }
    } else {
        WsaMsg {
            name: rd(msg),
            namelen_at: msg + 8,
            bufs: rd(msg + 16),
            count: rd32(msg + 24),
            control_len_at: msg + 32,
            control_buf: rd(msg + 40),
            flags_at: msg + 48,
        }
    }
}

unsafe fn iovecs(m: &WsaMsg) -> Vec<libc::iovec> {
    (0..m.count)
        .map(|i| {
            let (len, buf) = read_wsabuf(m.bufs, i);
            libc::iovec {
                iov_base: buf as *mut c_void,
                iov_len: len,
            }
        })
        .collect()
}

fn align(n: usize, a: usize) -> usize {
    n.div_ceil(a) * a
}

/// Message de contrôle Linux (niveau, type, données) → Windows ; `None`
/// pour ceux que Windows ne connaît pas sous cette forme.
fn to_windows(level: i32, ty: i32, data: &[u8]) -> Option<(i32, i32, Vec<u8>)> {
    match (level, ty) {
        // in_pktinfo { ifindex; spec_dst; addr } → IN_PKTINFO { addr; ifindex }.
        (libc::IPPROTO_IP, libc::IP_PKTINFO) if data.len() >= 12 => {
            let mut out = data[8..12].to_vec();
            out.extend_from_slice(&data[0..4]);
            Some((WIN_IPPROTO_IP, WIN_PKTINFO, out))
        }
        (libc::IPPROTO_IP, libc::IP_TTL) => Some((WIN_IPPROTO_IP, WIN_HOPLIMIT, data.to_vec())),
        // in6_pktinfo et IN6_PKTINFO ont la même forme.
        (libc::IPPROTO_IPV6, libc::IPV6_PKTINFO) => {
            Some((WIN_IPPROTO_IPV6, WIN_PKTINFO, data.to_vec()))
        }
        (libc::IPPROTO_IPV6, libc::IPV6_HOPLIMIT) => {
            Some((WIN_IPPROTO_IPV6, WIN_HOPLIMIT, data.to_vec()))
        }
        _ => None,
    }
}

/// Message de contrôle Windows → Linux, pour WSASendMsg.
fn to_linux(level: i32, ty: i32, data: &[u8]) -> Option<(i32, i32, Vec<u8>)> {
    match (level, ty) {
        (WIN_IPPROTO_IP, WIN_PKTINFO) if data.len() >= 8 => {
            let mut out = data[4..8].to_vec(); // ifindex
            out.extend_from_slice(&data[0..4]); // spec_dst : adresse source
            out.extend_from_slice(&[0; 4]);
            Some((libc::IPPROTO_IP, libc::IP_PKTINFO, out))
        }
        (WIN_IPPROTO_IP, WIN_HOPLIMIT) => Some((libc::IPPROTO_IP, libc::IP_TTL, data.to_vec())),
        (WIN_IPPROTO_IPV6, WIN_PKTINFO) => {
            Some((libc::IPPROTO_IPV6, libc::IPV6_PKTINFO, data.to_vec()))
        }
        (WIN_IPPROTO_IPV6, WIN_HOPLIMIT) => {
            Some((libc::IPPROTO_IPV6, libc::IPV6_HOPLIMIT, data.to_vec()))
        }
        _ => None,
    }
}

/// Parcourt une suite de messages de contrôle dont l'en-tête est
/// `{ size_t len; int level; int type; }` et les données alignées sur
/// `ps` octets.
fn walk(buf: &[u8], ps: usize) -> Vec<(i32, i32, &[u8])> {
    let hdr = ps + 8;
    let mut out = Vec::new();
    let mut at = 0;
    while at + hdr <= buf.len() {
        let len = if ps == 4 {
            u32::from_le_bytes(buf[at..at + 4].try_into().unwrap()) as usize
        } else {
            u64::from_le_bytes(buf[at..at + 8].try_into().unwrap()) as usize
        };
        if len < hdr || at + len > buf.len() {
            break;
        }
        let level = i32::from_le_bytes(buf[at + ps..at + ps + 4].try_into().unwrap());
        let ty = i32::from_le_bytes(buf[at + ps + 4..at + ps + 8].try_into().unwrap());
        out.push((level, ty, &buf[at + align(hdr, ps)..at + len]));
        at += align(len, ps);
    }
    out
}

/// Écrit des messages de contrôle avec un en-tête de `ps` octets de
/// longueur ; rend les octets écrits et vrai si tout a tenu.
fn build(items: &[(i32, i32, Vec<u8>)], ps: usize, room: usize) -> (Vec<u8>, bool) {
    let hdr = align(ps + 8, ps);
    let mut out = Vec::new();
    for (level, ty, data) in items {
        let len = hdr + data.len();
        if align(out.len(), ps) + len > room {
            return (out, false);
        }
        out.resize(align(out.len(), ps), 0);
        if ps == 4 {
            out.extend_from_slice(&(len as u32).to_le_bytes());
        } else {
            out.extend_from_slice(&(len as u64).to_le_bytes());
        }
        out.extend_from_slice(&level.to_le_bytes());
        out.extend_from_slice(&ty.to_le_bytes());
        out.resize(out.len() + hdr - (ps + 8), 0);
        out.extend_from_slice(data);
    }
    (out, true)
}

/// WSARecvMsg(s, lpMsg, lpdwNumberOfBytesRecvd, lpOverlapped,
/// lpCompletionRoutine). La réception est faite tout de suite ; Qt passe un
/// OVERLAPPED nul.
pub(super) unsafe extern "win64" fn wsa_recv_msg(
    s: usize,
    msg: usize,
    received: *mut u32,
    _overlapped: usize,
    _completion: usize,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    async_arm(s, FD_READ | FD_OOB);
    if msg == 0 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let m = layout(msg);
    let mut iov = iovecs(&m);
    let control_room = std::ptr::read_unaligned(m.control_len_at as *const u32) as usize;
    let mut linux_control = vec![0u8; if m.control_buf != 0 { 512 } else { 0 }];
    let mut h: libc::msghdr = std::mem::zeroed();
    let namelen = if m.name != 0 {
        std::ptr::read_unaligned(m.namelen_at as *const i32).max(0) as libc::socklen_t
    } else {
        0
    };
    h.msg_name = m.name as *mut c_void;
    h.msg_namelen = namelen;
    h.msg_iov = iov.as_mut_ptr();
    h.msg_iovlen = iov.len();
    if !linux_control.is_empty() {
        h.msg_control = linux_control.as_mut_ptr() as *mut c_void;
        h.msg_controllen = linux_control.len();
    }
    let in_flags = std::ptr::read_unaligned(m.flags_at as *const u32) as i32 & 0x3;
    let r = libc::recvmsg(s as i32, &mut h, in_flags);
    if r < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }
    if m.name != 0 {
        addr_out(m.name as *mut u8, h.msg_namelen as usize);
        std::ptr::write_unaligned(m.namelen_at as *mut i32, h.msg_namelen as i32);
    }
    let mut out_flags = 0u32;
    if h.msg_flags & libc::MSG_TRUNC != 0 {
        out_flags |= WIN_MSG_TRUNC;
    }
    let mut written = 0;
    if m.control_buf != 0 {
        let got = &linux_control[..h.msg_controllen.min(linux_control.len())];
        let items: Vec<_> = walk(got, 8)
            .into_iter()
            .filter_map(|(l, t, d)| to_windows(l, t, d))
            .collect();
        let (bytes, fits) = build(&items, crate::wow32::pointer_size(), control_room);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), m.control_buf as *mut u8, bytes.len());
        written = bytes.len();
        if !fits || h.msg_flags & libc::MSG_CTRUNC != 0 {
            out_flags |= WIN_MSG_CTRUNC;
        }
    }
    std::ptr::write_unaligned(m.control_len_at as *mut u32, written as u32);
    std::ptr::write_unaligned(m.flags_at as *mut u32, out_flags);
    if !received.is_null() {
        *received = r as u32;
    }
    0
}

/// WSASendMsg(s, lpMsg, dwFlags, lpNumberOfBytesSent, lpOverlapped,
/// lpCompletionRoutine), envoi immédiat.
pub(crate) unsafe extern "win64" fn wsa_send_msg(
    s: usize,
    msg: usize,
    flags: u32,
    sent: *mut u32,
    _overlapped: usize,
    _completion: usize,
) -> i32 {
    if !valid_socket(s) {
        set_error(WSAENOTSOCK);
        return SOCKET_ERROR;
    }
    if msg == 0 {
        set_error(WSAEFAULT);
        return SOCKET_ERROR;
    }
    let m = layout(msg);
    let mut iov = iovecs(&m);
    let mut h: libc::msghdr = std::mem::zeroed();
    let len = if m.name != 0 {
        std::ptr::read_unaligned(m.namelen_at as *const i32)
    } else {
        0
    };
    let mut to = addr_in(m.name as *const u8, len);
    if m.name != 0 {
        h.msg_name = &mut to as *mut _ as *mut c_void;
        h.msg_namelen = len.clamp(0, std::mem::size_of_val(&to) as i32) as libc::socklen_t;
    }
    h.msg_iov = iov.as_mut_ptr();
    h.msg_iovlen = iov.len();
    let mut control = Vec::new();
    let control_len = std::ptr::read_unaligned(m.control_len_at as *const u32) as usize;
    if m.control_buf != 0 && control_len > 0 {
        let raw = std::slice::from_raw_parts(m.control_buf as *const u8, control_len);
        let items: Vec<_> = walk(raw, crate::wow32::pointer_size())
            .into_iter()
            .filter_map(|(l, t, d)| to_linux(l, t, d))
            .collect();
        control = build(&items, 8, usize::MAX).0;
    }
    if !control.is_empty() {
        h.msg_control = control.as_mut_ptr() as *mut c_void;
        h.msg_controllen = control.len();
    }
    let r = libc::sendmsg(s as i32, &h, (flags as i32 & 0x7) | libc::MSG_NOSIGNAL);
    if r < 0 {
        set_error(last_error());
        return SOCKET_ERROR;
    }
    if !sent.is_null() {
        *sent = r as u32;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controle_pktinfo_aller_retour() {
        // in_pktinfo Linux : ifindex 3, spec_dst 10.0.0.2, addr 10.0.0.1.
        let mut d = 3i32.to_le_bytes().to_vec();
        d.extend_from_slice(&[10, 0, 0, 2, 10, 0, 0, 1]);
        let (l, t, w) = to_windows(libc::IPPROTO_IP, libc::IP_PKTINFO, &d).unwrap();
        assert_eq!((l, t), (0, 19));
        assert_eq!(w, [10, 0, 0, 1, 3, 0, 0, 0]);
        let (bytes, fits) = build(&[(l, t, w)], 8, 64);
        assert!(fits);
        assert_eq!(bytes.len(), 24);
        assert_eq!(&bytes[..8], &24u64.to_le_bytes());
        let back = walk(&bytes, 8);
        assert_eq!(back.len(), 1);
        let (l2, t2, d2) = to_linux(back[0].0, back[0].1, back[0].2).unwrap();
        assert_eq!((l2, t2), (libc::IPPROTO_IP, libc::IP_PKTINFO));
        assert_eq!(&d2[..4], &3i32.to_le_bytes());
        // x86 : en-tête de 12 octets, données alignées sur 4.
        let (b4, _) = build(&[(41, 21, 64i32.to_le_bytes().to_vec())], 4, 64);
        assert_eq!(b4.len(), 16);
        assert!(!build(&[(41, 21, vec![0; 4])], 8, 8).1);
    }

    #[test]
    fn udp_local_par_wsamsg() {
        unsafe {
            let a = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
            let b = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
            super::super::register_socket(a as usize);
            super::super::register_socket(b as usize);
            let mut addr: libc::sockaddr_in = std::mem::zeroed();
            addr.sin_family = libc::AF_INET as u16;
            addr.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
            assert_eq!(
                libc::bind(a, &addr as *const _ as *const libc::sockaddr, 16),
                0
            );
            let mut len = 16u32;
            libc::getsockname(a, &mut addr as *mut _ as *mut libc::sockaddr, &mut len);
            let one = 1i32;
            libc::setsockopt(
                a,
                libc::IPPROTO_IP,
                libc::IP_PKTINFO,
                &one as *const _ as *const c_void,
                4,
            );
            // WSAMSG x64 pour l'envoi.
            let mut payload = *b"bonjour";
            let wbuf = super::super::WsaBuf {
                len: 7,
                buf: payload.as_mut_ptr(),
            };
            let mut m = [0u8; 56];
            m[0..8].copy_from_slice(&(&addr as *const _ as u64).to_le_bytes());
            m[8..12].copy_from_slice(&16i32.to_le_bytes());
            m[16..24].copy_from_slice(&(&wbuf as *const _ as u64).to_le_bytes());
            m[24..28].copy_from_slice(&1u32.to_le_bytes());
            let mut sent = 0u32;
            assert_eq!(
                wsa_send_msg(b as usize, m.as_ptr() as usize, 0, &mut sent, 0, 0),
                0
            );
            assert_eq!(sent, 7);
            // Réception avec adresse et contrôle.
            let mut into = [0u8; 32];
            let rbuf = super::super::WsaBuf {
                len: 32,
                buf: into.as_mut_ptr(),
            };
            let mut from = [0u8; 28];
            let mut control = [0u8; 64];
            let mut r = [0u8; 56];
            r[0..8].copy_from_slice(&(from.as_mut_ptr() as u64).to_le_bytes());
            r[8..12].copy_from_slice(&28i32.to_le_bytes());
            r[16..24].copy_from_slice(&(&rbuf as *const _ as u64).to_le_bytes());
            r[24..28].copy_from_slice(&1u32.to_le_bytes());
            r[32..36].copy_from_slice(&64u32.to_le_bytes());
            r[40..48].copy_from_slice(&(control.as_mut_ptr() as u64).to_le_bytes());
            let mut got = 0u32;
            assert_eq!(
                wsa_recv_msg(a as usize, r.as_mut_ptr() as usize, &mut got, 0, 0),
                0
            );
            assert_eq!(&into[..got as usize], b"bonjour");
            assert_eq!(u16::from_le_bytes([from[0], from[1]]), 2);
            assert_eq!(u32::from_le_bytes(r[32..36].try_into().unwrap()), 24);
            assert_eq!(i32::from_le_bytes(control[12..16].try_into().unwrap()), 19);
            assert_eq!(&control[16..20], &[127, 0, 0, 1]);
            libc::close(a);
            libc::close(b);
        }
    }
}
