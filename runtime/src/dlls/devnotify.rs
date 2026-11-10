//! Avis de branchement et de retrait d'appareils (#1523) :
//! `RegisterDeviceNotificationW/A` et `UnregisterDeviceNotification`.
//!
//! Les logiciels de matériel, SDL et hidapi s'inscrivent pour une classe
//! d'interface (`DBT_DEVTYP_DEVICEINTERFACE`, par ex. l'interface HID) et
//! attendent `WM_DEVICECHANGE` : `DBT_DEVICEARRIVAL` / `DBT_DEVICEREMOVECOMPLETE`
//! avec le chemin de l'interface, puis `DBT_DEVNODES_CHANGED`. Winlux relève
//! l'arbre de `devnodes` (hidraw, ports série, appareils USB) toutes les
//! secondes tant qu'une inscription existe, et poste ces messages à la
//! fenêtre inscrite. Structures d'après dbt.h (en-têtes publics).
//!
//! Le tampon pointé par lParam est alloué sur le tas du processus (sous
//! 4 Go pour un programme 32 bits) et gardé pour les 64 derniers avis, le
//! temps que le programme lise le message.

#![allow(non_snake_case)]

use std::collections::{BTreeSet, VecDeque};
use std::sync::Mutex;

const WM_DEVICECHANGE: u32 = 0x0219;
const DBT_DEVNODES_CHANGED: usize = 0x0007;
const DBT_DEVICEARRIVAL: usize = 0x8000;
const DBT_DEVICEREMOVECOMPLETE: usize = 0x8004;
const DBT_DEVTYP_DEVICEINTERFACE: u32 = 5;
const DEVICE_NOTIFY_SERVICE_HANDLE: u32 = 1;
const DEVICE_NOTIFY_ALL_INTERFACE_CLASSES: u32 = 4;
const ERROR_INVALID_PARAMETER: u32 = 87;

struct Registration {
    handle: usize,
    hwnd: usize,
    /// Classe d'interface suivie, `None` pour toutes.
    class: Option<[u8; 16]>,
    wide: bool,
}

struct State {
    next: usize,
    list: Vec<Registration>,
    watching: bool,
    /// Tampons encore lisibles par le programme.
    buffers: VecDeque<usize>,
}

static STATE: Mutex<State> = Mutex::new(State {
    next: 0xD0_0000,
    list: Vec::new(),
    watching: false,
    buffers: VecDeque::new(),
});

fn snapshot() -> BTreeSet<([u8; 16], String)> {
    crate::dlls::devnodes::interface_paths()
        .into_iter()
        .collect()
}

/// DEV_BROADCAST_DEVICEINTERFACE_W ou _A sur le tas du processus.
fn broadcast(class: &[u8; 16], path: &str, wide: bool) -> usize {
    let name: Vec<u8> = if wide {
        path.encode_utf16()
            .chain([0])
            .flat_map(|c| c.to_le_bytes())
            .collect()
    } else {
        path.bytes().chain([0]).collect()
    };
    let size = 28 + name.len();
    let heap = crate::dlls::kernel32::process_heap();
    let p = unsafe { crate::dlls::kernel32::heap_alloc(heap, 8, size) } as *mut u8;
    if p.is_null() {
        return 0;
    }
    unsafe {
        (p as *mut u32).write_unaligned(size as u32);
        (p.add(4) as *mut u32).write_unaligned(DBT_DEVTYP_DEVICEINTERFACE);
        std::ptr::copy_nonoverlapping(class.as_ptr(), p.add(12), 16);
        std::ptr::copy_nonoverlapping(name.as_ptr(), p.add(28), name.len());
    }
    let mut st = crate::dlls::lock(&STATE);
    st.buffers.push_back(p as usize);
    while st.buffers.len() > 64 {
        if let Some(old) = st.buffers.pop_front() {
            unsafe { crate::dlls::kernel32::heap_free(heap, 0, old as *mut _) };
        }
    }
    p as usize
}

fn post(hwnd: usize, wparam: usize, lparam: usize) {
    let m = crate::dlls::gui::Msg {
        hwnd,
        message: WM_DEVICECHANGE,
        wparam,
        lparam: lparam as isize,
        time: 0,
        pt: crate::dlls::gui::Point::default(),
        l_private: 0,
    };
    crate::dlls::gui::post_to_window(hwnd, m);
}

/// Écarts entre deux relevés, envoyés à chaque inscription concernée.
fn notify(before: &BTreeSet<([u8; 16], String)>, after: &BTreeSet<([u8; 16], String)>) {
    let targets: Vec<(usize, Option<[u8; 16]>, bool)> = crate::dlls::lock(&STATE)
        .list
        .iter()
        .map(|r| (r.hwnd, r.class, r.wide))
        .collect();
    for (hwnd, class, wide) in targets {
        let wanted = |c: &[u8; 16]| class.is_none_or(|k| &k == c);
        let mut changed = false;
        for (c, path) in before.difference(after).filter(|(c, _)| wanted(c)) {
            post(hwnd, DBT_DEVICEREMOVECOMPLETE, broadcast(c, path, wide));
            changed = true;
        }
        for (c, path) in after.difference(before).filter(|(c, _)| wanted(c)) {
            post(hwnd, DBT_DEVICEARRIVAL, broadcast(c, path, wide));
            changed = true;
        }
        if changed {
            post(hwnd, DBT_DEVNODES_CHANGED, 0);
        }
    }
}

fn start_watch() {
    {
        let mut st = crate::dlls::lock(&STATE);
        if st.watching {
            return;
        }
        st.watching = true;
    }
    let mut seen = snapshot();
    let spawned = std::thread::Builder::new()
        .name("winlux-devnotify".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if crate::dlls::lock(&STATE).list.is_empty() {
                crate::dlls::lock(&STATE).watching = false;
                return;
            }
            let now = snapshot();
            if now != seen {
                notify(&seen, &now);
                seen = now;
            }
        });
    if spawned.is_err() {
        crate::dlls::lock(&STATE).watching = false;
    }
}

unsafe fn register(recipient: usize, filter: *const u8, flags: u32, wide: bool) -> usize {
    if filter.is_null() || recipient == 0 {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let kind = (filter.add(4) as *const u32).read_unaligned();
    let mut handle_out;
    {
        let mut st = crate::dlls::lock(&STATE);
        st.next += 4;
        handle_out = st.next;
        // Avis à un service, ou d'un autre type (handle de fichier, volume) :
        // l'inscription réussit, aucun avis n'est émis.
        if flags & DEVICE_NOTIFY_SERVICE_HANDLE != 0 || kind != DBT_DEVTYP_DEVICEINTERFACE {
            return handle_out;
        }
        let class = if flags & DEVICE_NOTIFY_ALL_INTERFACE_CLASSES != 0 {
            None
        } else {
            let mut g = [0u8; 16];
            std::ptr::copy_nonoverlapping(filter.add(12), g.as_mut_ptr(), 16);
            Some(g)
        };
        st.list.push(Registration {
            handle: handle_out,
            hwnd: recipient,
            class,
            wide,
        });
    }
    start_watch();
    crate::process::set_last_error(0);
    handle_out = handle_out.max(1);
    handle_out
}

pub unsafe extern "win64" fn RegisterDeviceNotificationW(
    recipient: usize,
    filter: *const u8,
    flags: u32,
) -> usize {
    register(recipient, filter, flags, true)
}
pub unsafe extern "win64" fn RegisterDeviceNotificationA(
    recipient: usize,
    filter: *const u8,
    flags: u32,
) -> usize {
    register(recipient, filter, flags, false)
}
pub extern "win64" fn UnregisterDeviceNotification(handle: usize) -> i32 {
    let mut st = crate::dlls::lock(&STATE);
    let before = st.list.len();
    st.list.retain(|r| r.handle != handle);
    // Une inscription sans avis (service, autre type) n'est pas dans la
    // liste : elle se désinscrit tout aussi bien.
    i32::from(before != st.list.len() || (handle & 3 == 0 && handle >= 0xD0_0000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tampon_d_avis_au_format_dbt() {
        let class = crate::dlls::hid::HID_GUID;
        let p = broadcast(&class, "\\\\?\\hid#x", true) as *const u8;
        assert!(!p.is_null());
        unsafe {
            assert_eq!((p as *const u32).read_unaligned(), 28 + 2 * 10);
            assert_eq!((p.add(4) as *const u32).read_unaligned(), 5);
            assert_eq!(std::slice::from_raw_parts(p.add(12), 16), &class);
            assert_eq!((p.add(28) as *const u16).read_unaligned(), u16::from(b'\\'));
        }
    }
}
