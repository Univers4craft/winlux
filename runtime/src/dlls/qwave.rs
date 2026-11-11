//! qwave.dll : l'API qWAVE de qualité de service (qos2.h), importée par
//! discord_voice.node (#1855).
//!
//! Un handle QoS regroupe des flux ; chaque flux porte des sockets et une
//! valeur DSCP. Le marquage DSCP est posé sur le socket Linux (IP_TOS ou
//! IPV6_TCLASS) ; si le noyau le refuse, le flux reste valide, comme sous
//! Windows quand la stratégie réseau ignore le marquage. Les requêtes de
//! mesure du chemin (QOSQueryFlow fondamentaux, QOSNotifyFlow) ne sont pas
//! disponibles et le disent par ERROR_NOT_SUPPORTED.
//!
//! Source : documentation publique de Microsoft (qos2.h, « qWAVE »).

#![allow(non_snake_case)]

use crate::process::set_last_error;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NOT_FOUND: u32 = 1168;

/// QOS_SET_FLOW.
const QOS_SET_TRAFFIC_TYPE: u32 = 0;
const QOS_SET_OUTGOING_RATE: u32 = 1;
const QOS_SET_OUTGOING_DSCP_VALUE: u32 = 2;
/// QOS_QUERY_FLOW.
const QOS_QUERY_PACKET_PRIORITY: u32 = 1;

#[derive(Default)]
struct Flow {
    sockets: Vec<usize>,
    dscp: u8,
    traffic: u32,
}

#[derive(Default)]
struct Session {
    flows: HashMap<u32, Flow>,
    next: u32,
}

fn sessions() -> &'static Mutex<HashMap<usize, Session>> {
    static S: OnceLock<Mutex<HashMap<usize, Session>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn fail(code: u32) -> i32 {
    set_last_error(code);
    0
}

/// DSCP documenté de chaque QOS_TRAFFIC_TYPE.
fn dscp_of(traffic: u32) -> Option<u8> {
    Some(match traffic {
        0 => 0,      // QOSTrafficTypeBestEffort
        1 => 8,      // QOSTrafficTypeBackground
        2 | 3 => 40, // ExcellentEffort, AudioVideo
        4 | 5 => 56, // Voice, Control
        _ => return None,
    })
}

/// Marque le socket (descripteur Linux) avec `dscp`, sans échec visible.
fn mark(socket: usize, dscp: u8) {
    let fd = socket as libc::c_int;
    let tos: libc::c_int = libc::c_int::from(dscp) << 2;
    // SAFETY : options simples sur un descripteur du processus ; une erreur
    // (descripteur invalide, refus du noyau) est ignorée.
    unsafe {
        let mut addr: libc::sockaddr_storage = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        if libc::getsockname(
            fd,
            (&mut addr as *mut libc::sockaddr_storage).cast(),
            &mut len,
        ) != 0
        {
            return;
        }
        let (level, name) = if i32::from(addr.ss_family) == libc::AF_INET6 {
            (libc::IPPROTO_IPV6, libc::IPV6_TCLASS)
        } else {
            (libc::IPPROTO_IP, libc::IP_TOS)
        };
        libc::setsockopt(
            fd,
            level,
            name,
            (&tos as *const libc::c_int).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
    }
}

/// QOSCreateHandle : seule la version 1.0 existe.
pub unsafe extern "win64" fn QOSCreateHandle(version: *const u16, handle: *mut usize) -> i32 {
    if version.is_null() || handle.is_null() || *version != 1 || *version.add(1) != 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0x7105_0004);
    let h = NEXT.fetch_add(4, Ordering::Relaxed);
    lock(sessions()).insert(h, Session::default());
    *handle = h;
    1
}

pub extern "win64" fn QOSCloseHandle(handle: usize) -> i32 {
    match lock(sessions()).remove(&handle) {
        Some(_) => 1,
        None => fail(ERROR_INVALID_HANDLE),
    }
}

/// QOSAddSocketToFlow : `*flow` à 0 crée un flux, sinon y ajoute le socket.
pub unsafe extern "win64" fn QOSAddSocketToFlow(
    handle: usize,
    socket: usize,
    _destination: *const u8,
    traffic: u32,
    _flags: u32,
    flow: *mut u32,
) -> i32 {
    if flow.is_null() || socket == 0 || socket == usize::MAX {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let Some(dscp) = dscp_of(traffic) else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    let mut all = lock(sessions());
    let Some(s) = all.get_mut(&handle) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let id = if *flow == 0 {
        s.next += 1;
        s.flows.insert(
            s.next,
            Flow {
                sockets: vec![],
                dscp,
                traffic,
            },
        );
        s.next
    } else {
        *flow
    };
    let Some(f) = s.flows.get_mut(&id) else {
        return fail(ERROR_NOT_FOUND);
    };
    f.sockets.push(socket);
    mark(socket, f.dscp);
    *flow = id;
    1
}

pub extern "win64" fn QOSRemoveSocketFromFlow(
    handle: usize,
    socket: usize,
    flow: u32,
    _flags: u32,
) -> i32 {
    let mut all = lock(sessions());
    let Some(s) = all.get_mut(&handle) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let ids: Vec<u32> = if flow == 0 {
        s.flows.keys().copied().collect()
    } else {
        vec![flow]
    };
    let mut found = false;
    for id in ids {
        if let Some(f) = s.flows.get_mut(&id) {
            let before = f.sockets.len();
            f.sockets.retain(|&k| k != socket);
            if f.sockets.len() != before {
                found = true;
                mark(socket, 0);
            }
            if f.sockets.is_empty() {
                s.flows.remove(&id);
            }
        }
    }
    if found {
        1
    } else {
        fail(ERROR_NOT_FOUND)
    }
}

/// QOSSetFlow : type de trafic et DSCP sortant ; le débit sortant (mise en
/// forme du trafic) n'est pas disponible.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn QOSSetFlow(
    handle: usize,
    flow: u32,
    operation: u32,
    size: u32,
    buffer: *const u8,
    _flags: u32,
    _overlapped: usize,
) -> i32 {
    let mut all = lock(sessions());
    let Some(s) = all.get_mut(&handle) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let Some(f) = s.flows.get_mut(&flow) else {
        return fail(ERROR_NOT_FOUND);
    };
    if buffer.is_null() || size < 4 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let value = (buffer as *const u32).read_unaligned();
    match operation {
        QOS_SET_TRAFFIC_TYPE => {
            let Some(dscp) = dscp_of(value) else {
                return fail(ERROR_INVALID_PARAMETER);
            };
            f.traffic = value;
            f.dscp = dscp;
        }
        QOS_SET_OUTGOING_DSCP_VALUE => {
            if value > 63 {
                return fail(ERROR_INVALID_PARAMETER);
            }
            f.dscp = value as u8;
        }
        QOS_SET_OUTGOING_RATE => return fail(ERROR_NOT_SUPPORTED),
        _ => return fail(ERROR_INVALID_PARAMETER),
    }
    for &k in &f.sockets {
        mark(k, f.dscp);
    }
    1
}

/// QOSQueryFlow : la priorité de paquet (QOS_PACKET_PRIORITY, 16 octets :
/// DSCP conforme, DSCP non conforme, 802.1p conforme, non conforme) ; les
/// mesures du chemin ne sont pas disponibles.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn QOSQueryFlow(
    handle: usize,
    flow: u32,
    operation: u32,
    size: *mut u32,
    buffer: *mut u8,
    _flags: u32,
    _overlapped: usize,
) -> i32 {
    let all = lock(sessions());
    let Some(s) = all.get(&handle) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let Some(f) = s.flows.get(&flow) else {
        return fail(ERROR_NOT_FOUND);
    };
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if operation != QOS_QUERY_PACKET_PRIORITY {
        return fail(ERROR_NOT_SUPPORTED);
    }
    if buffer.is_null() || *size < 16 {
        *size = 16;
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    let p = buffer as *mut u32;
    p.write_unaligned(u32::from(f.dscp));
    p.add(1).write_unaligned(0);
    p.add(2).write_unaligned(0);
    p.add(3).write_unaligned(0);
    *size = 16;
    1
}

/// QOSNotifyFlow : pas de notification de congestion ni de débit.
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn QOSNotifyFlow(
    handle: usize,
    _flow: u32,
    _operation: u32,
    _size: usize,
    _buffer: usize,
    _flags: u32,
    _overlapped: usize,
) -> i32 {
    if !lock(sessions()).contains_key(&handle) {
        return fail(ERROR_INVALID_HANDLE);
    }
    fail(ERROR_NOT_SUPPORTED)
}

pub extern "win64" fn QOSCancel(handle: usize, _overlapped: usize) -> i32 {
    if !lock(sessions()).contains_key(&handle) {
        return fail(ERROR_INVALID_HANDLE);
    }
    1
}

/// QOSEnumerateFlows : tableau des identifiants de flux.
pub unsafe extern "win64" fn QOSEnumerateFlows(
    handle: usize,
    size: *mut u32,
    buffer: *mut u32,
) -> i32 {
    let all = lock(sessions());
    let Some(s) = all.get(&handle) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if size.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let mut ids: Vec<u32> = s.flows.keys().copied().collect();
    ids.sort_unstable();
    let needed = (ids.len() * 4) as u32;
    if buffer.is_null() || *size < needed {
        *size = needed;
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    std::ptr::copy_nonoverlapping(ids.as_ptr(), buffer, ids.len());
    *size = needed;
    1
}

/// Suivi de client (QOSStartTrackingClient/QOSStopTrackingClient) : accepté
/// sans mesure du chemin.
pub extern "win64" fn QOSTrackingClient(handle: usize, destination: usize, _flags: u32) -> i32 {
    if !lock(sessions()).contains_key(&handle) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if destination == 0 {
        return fail(ERROR_INVALID_PARAMETER);
    }
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "QOSCreateHandle" => fn QOSCreateHandle,
        "QOSCloseHandle" => fn QOSCloseHandle,
        "QOSAddSocketToFlow" => fn QOSAddSocketToFlow,
        "QOSRemoveSocketFromFlow" => fn QOSRemoveSocketFromFlow,
        "QOSSetFlow" => fn QOSSetFlow,
        "QOSQueryFlow" => fn QOSQueryFlow,
        "QOSNotifyFlow" => fn QOSNotifyFlow,
        "QOSCancel" => fn QOSCancel,
        "QOSEnumerateFlows" => fn QOSEnumerateFlows,
        "QOSStartTrackingClient" => fn QOSTrackingClient,
        "QOSStopTrackingClient" => fn QOSTrackingClient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flux_voix_marque_le_socket() {
        unsafe {
            let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
            assert!(fd >= 0);
            let mut h = 0usize;
            assert_eq!(QOSCreateHandle([2u16, 0].as_ptr(), &mut h), 0);
            assert_eq!(QOSCreateHandle([1u16, 0].as_ptr(), &mut h), 1);
            let mut flow = 0u32;
            assert_eq!(
                QOSAddSocketToFlow(h, fd as usize, std::ptr::null(), 4, 2, &mut flow),
                1
            );
            assert_ne!(flow, 0);
            let mut size = 0u32;
            let mut prio = [0u32; 4];
            assert_eq!(
                QOSQueryFlow(h, flow, 1, &mut size, std::ptr::null_mut(), 0, 0),
                0
            );
            assert_eq!(size, 16);
            assert_eq!(
                QOSQueryFlow(h, flow, 1, &mut size, prio.as_mut_ptr().cast(), 0, 0),
                1
            );
            assert_eq!(prio[0], 56);
            let dscp = 46u32;
            assert_eq!(
                QOSSetFlow(h, flow, 2, 4, (&dscp as *const u32).cast(), 0, 0),
                1
            );
            let mut ids = [0u32; 4];
            size = 16;
            assert_eq!(QOSEnumerateFlows(h, &mut size, ids.as_mut_ptr()), 1);
            assert_eq!((size, ids[0]), (4, flow));
            assert_eq!(QOSRemoveSocketFromFlow(h, fd as usize, 0, 0), 1);
            assert_eq!(QOSRemoveSocketFromFlow(h, fd as usize, 0, 0), 0);
            assert_eq!(QOSCloseHandle(h), 1);
            assert_eq!(QOSCloseHandle(h), 0);
            libc::close(fd);
        }
    }
}
