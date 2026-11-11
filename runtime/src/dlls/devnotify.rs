//! Avis de branchement et de retrait d'appareils (#1523) :
//! `RegisterDeviceNotificationW/A` et `UnregisterDeviceNotification`, et
//! leur forme cfgmgr32 `CM_Register_Notification` /
//! `CM_Unregister_Notification` (rappel au lieu d'une fenêtre, #1585).
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
    target: Target,
    /// Classe d'interface suivie, `None` pour toutes.
    class: Option<[u8; 16]>,
}

#[derive(Clone, Copy)]
enum Target {
    /// WM_DEVICECHANGE postés à la fenêtre (A ou W).
    Window { hwnd: usize, wide: bool },
    /// PCM_NOTIFY_CALLBACK, appelable en win64 (passerelle pour un
    /// programme 32 bits), et son contexte.
    Callback { function: usize, context: usize },
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
    let targets: Vec<(usize, Target, Option<[u8; 16]>)> = crate::dlls::lock(&STATE)
        .list
        .iter()
        .map(|r| (r.handle, r.target, r.class))
        .collect();
    for (handle, target, class) in targets {
        let wanted = |c: &[u8; 16]| class.is_none_or(|k| &k == c);
        let (hwnd, wide) = match target {
            Target::Window { hwnd, wide } => (hwnd, wide),
            Target::Callback { function, context } => {
                for (c, path) in before.difference(after).filter(|(c, _)| wanted(c)) {
                    cm_callback(function, handle, context, CM_NOTIFY_ACTION_REMOVAL, c, path);
                }
                for (c, path) in after.difference(before).filter(|(c, _)| wanted(c)) {
                    cm_callback(function, handle, context, CM_NOTIFY_ACTION_ARRIVAL, c, path);
                }
                continue;
            }
        };
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
            target: Target::Window {
                hwnd: recipient,
                wide,
            },
            class,
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

// ---------- cfgmgr32 : CM_Register_Notification ----------

const CR_SUCCESS: u32 = 0;
const CR_INVALID_POINTER: u32 = 3;
const CR_INVALID_FLAG: u32 = 4;
const CR_INVALID_DATA: u32 = 31;
/// sizeof(CM_NOTIFY_FILTER) : en-tête de 16 octets et union de
/// MAX_DEVICE_ID_LEN (200) caractères larges.
const CM_NOTIFY_FILTER_SIZE: u32 = 416;
const CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES: u32 = 1;
const CM_NOTIFY_FILTER_FLAG_ALL_DEVICE_INSTANCES: u32 = 2;
const CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE: u32 = 0;
const CM_NOTIFY_FILTER_TYPE_MAX: u32 = 3;
const CM_NOTIFY_ACTION_ARRIVAL: u32 = 0;
const CM_NOTIFY_ACTION_REMOVAL: u32 = 1;

/// CM_NOTIFY_EVENT_DATA d'une interface : FilterType, Reserved, ClassGuid,
/// SymbolicLink (UTF-16), sur le tas du processus comme les tampons DBT.
fn cm_event_data(class: &[u8; 16], path: &str) -> (usize, u32) {
    let link: Vec<u8> = path
        .encode_utf16()
        .chain([0])
        .flat_map(|c| c.to_le_bytes())
        .collect();
    let size = 24 + link.len();
    let heap = crate::dlls::kernel32::process_heap();
    let p = unsafe { crate::dlls::kernel32::heap_alloc(heap, 8, size) } as *mut u8;
    if p.is_null() {
        return (0, 0);
    }
    unsafe {
        (p as *mut u32).write_unaligned(CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE);
        std::ptr::copy_nonoverlapping(class.as_ptr(), p.add(8), 16);
        std::ptr::copy_nonoverlapping(link.as_ptr(), p.add(24), link.len());
    }
    (p as usize, size as u32)
}

/// Appelle le rappel du programme (depuis le thread de relevé, comme le
/// thread système de Windows), puis libère les données de l'avis.
fn cm_callback(
    function: usize,
    handle: usize,
    context: usize,
    action: u32,
    class: &[u8; 16],
    path: &str,
) {
    let (data, size) = cm_event_data(class, path);
    if data == 0 {
        return;
    }
    let f: extern "win64" fn(usize, usize, u32, usize, u32) -> u32 =
        unsafe { std::mem::transmute(function) };
    f(handle, context, action, data, size);
    let heap = crate::dlls::kernel32::process_heap();
    unsafe { crate::dlls::kernel32::heap_free(heap, 0, data as *mut _) };
}

pub unsafe extern "win64" fn CM_Register_Notification(
    filter: *const u8,
    context: usize,
    callback: usize,
    out: *mut usize,
) -> u32 {
    if filter.is_null() || callback == 0 || out.is_null() {
        return CR_INVALID_POINTER;
    }
    *out = 0;
    let size = (filter as *const u32).read_unaligned();
    let flags = (filter.add(4) as *const u32).read_unaligned();
    let kind = (filter.add(8) as *const u32).read_unaligned();
    if size != CM_NOTIFY_FILTER_SIZE || kind >= CM_NOTIFY_FILTER_TYPE_MAX {
        return CR_INVALID_DATA;
    }
    if flags
        & !(CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES
            | CM_NOTIFY_FILTER_FLAG_ALL_DEVICE_INSTANCES)
        != 0
    {
        return CR_INVALID_FLAG;
    }
    // Rappel x86 d'un programme 32 bits : passerelle win64.
    let function = if crate::wow32::active() {
        crate::wow32::callback64(callback as u32)
    } else {
        callback
    };
    let handle = {
        let mut st = crate::dlls::lock(&STATE);
        st.next += 4;
        let handle = st.next;
        // Avis par handle de fichier ou par instance : inscrits, jamais émis.
        if kind == CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE {
            let class = if flags & CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES != 0 {
                None
            } else {
                let mut g = [0u8; 16];
                std::ptr::copy_nonoverlapping(filter.add(16), g.as_mut_ptr(), 16);
                Some(g)
            };
            st.list.push(Registration {
                handle,
                target: Target::Callback { function, context },
                class,
            });
        }
        handle
    };
    if kind == CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE {
        start_watch();
    }
    *out = handle;
    CR_SUCCESS
}

pub extern "win64" fn CM_Unregister_Notification(handle: usize) -> u32 {
    if handle == 0 {
        return CR_INVALID_POINTER;
    }
    let mut st = crate::dlls::lock(&STATE);
    st.list.retain(|r| r.handle != handle);
    CR_SUCCESS
}

pub fn cfgmgr32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CM_Register_Notification" => fn CM_Register_Notification,
        "CM_Unregister_Notification" => fn CM_Unregister_Notification,
    }
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

    static SEEN: Mutex<Vec<(usize, usize, u32, String)>> = Mutex::new(Vec::new());

    extern "win64" fn recorder(h: usize, ctx: usize, action: u32, data: usize, size: u32) -> u32 {
        let p = data as *const u8;
        let n = (size as usize - 24) / 2 - 1;
        let link: Vec<u16> = (0..n)
            .map(|i| unsafe { (p.add(24 + 2 * i) as *const u16).read_unaligned() })
            .collect();
        SEEN.lock()
            .unwrap()
            .push((h, ctx, action, String::from_utf16_lossy(&link)));
        0
    }

    #[test]
    fn inscription_cfgmgr32_et_rappel() {
        unsafe {
            let mut filter = [0u64; 52];
            let f = filter.as_mut_ptr() as *mut u8;
            (f as *mut u32).write(CM_NOTIFY_FILTER_SIZE);
            (f.add(4) as *mut u32).write(CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES);
            let mut h = 0usize;
            assert_eq!(
                CM_Register_Notification(f, 77, recorder as *const () as usize, &mut h),
                CR_SUCCESS
            );
            assert_ne!(h, 0);

            // Un appareil apparaît puis disparaît entre deux relevés.
            let class = crate::dlls::hid::HID_GUID;
            let empty = BTreeSet::new();
            let one: BTreeSet<_> = [(class, "\\\\?\\hid#essai".to_string())].into();
            notify(&empty, &one);
            notify(&one, &empty);
            let seen: Vec<_> = SEEN
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.0 == h)
                .cloned()
                .collect();
            assert_eq!(seen.len(), 2);
            assert_eq!((seen[0].1, seen[0].2), (77, CM_NOTIFY_ACTION_ARRIVAL));
            assert_eq!(seen[0].3, "\\\\?\\hid#essai");
            assert_eq!(seen[1].2, CM_NOTIFY_ACTION_REMOVAL);

            assert_eq!(CM_Unregister_Notification(h), CR_SUCCESS);
            notify(&empty, &one);
            assert_eq!(SEEN.lock().unwrap().iter().filter(|e| e.0 == h).count(), 2);

            // Taille de filtre fausse, pointeurs nuls.
            (f as *mut u32).write(16);
            assert_eq!(
                CM_Register_Notification(f, 0, recorder as *const () as usize, &mut h),
                CR_INVALID_DATA
            );
            assert_eq!(
                CM_Register_Notification(std::ptr::null(), 0, 1, &mut h),
                CR_INVALID_POINTER
            );
        }
    }
}
