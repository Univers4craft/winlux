//! Bouton de la barre des tâches : CLSID_TaskbarList (ITaskbarList,
//! ITaskbarList2, ITaskbarList3, ITaskbarList4), #1434.
//!
//! Navigateurs, installeurs, 7-Zip ou Paint.NET l'utilisent pour montrer
//! une progression sur leur bouton, une icône superposée ou des boutons
//! de miniature. La barre de la session native ne sait pas encore les
//! afficher : les appels sont acceptés (S_OK), et la progression de chaque
//! fenêtre est retenue (`progress`) pour le jour où elle le saura. Sans
//! cette classe, CoCreateInstance rendait REGDB_E_CLASSNOTREG. Méthodes et
//! ordre : documentation Microsoft Learn d'ITaskbarList3/4 et shobjidl.h
//! de MinGW-w64 (domaine public).

use super::comobj::{guid, Guid, VTable};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;

pub const CLSID_TASKBAR_LIST: Guid = guid(
    0x56FD_F344,
    0xFD6D,
    0x11D0,
    [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
);
pub const IID_ITASKBAR_LIST: Guid = guid(
    0x56FD_F342,
    0xFD6D,
    0x11D0,
    [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
);
pub const IID_ITASKBAR_LIST2: Guid = guid(
    0x602D_4995,
    0xB13A,
    0x429B,
    [0xA6, 0x6E, 0x19, 0x35, 0xE4, 0x4F, 0x43, 0x17],
);
pub const IID_ITASKBAR_LIST3: Guid = guid(
    0xEA1A_FB91,
    0x9E28,
    0x4B86,
    [0x90, 0xE9, 0x9E, 0x9F, 0x8A, 0x5E, 0xEF, 0xAF],
);
pub const IID_ITASKBAR_LIST4: Guid = guid(
    0xC43D_C798,
    0x95D1,
    0x4BEA,
    [0x90, 0x30, 0xBB, 0x99, 0xE2, 0x98, 0x3A, 0x1A],
);
const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);

/// TBPF_* de SetProgressState.
pub const TBPF_NOPROGRESS: u32 = 0;
pub const TBPF_INDETERMINATE: u32 = 1;

/// Progression d'une fenêtre : état TBPF_* et valeur en millièmes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub state: u32,
    pub permille: u32,
}

fn table() -> std::sync::MutexGuard<'static, HashMap<usize, Progress>> {
    static TABLE: OnceLock<Mutex<HashMap<usize, Progress>>> = OnceLock::new();
    TABLE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Progression montrée par le programme sur le bouton de `hwnd`. Lue par les
/// tests ; la barre de la session native la lira quand elle saura l'afficher.
#[allow(dead_code)]
pub fn progress(hwnd: usize) -> Option<Progress> {
    table().get(&hwnd).copied()
}

/// Objet sans état propre : un seul exemplaire, compteur fixe.
#[repr(C)]
struct Obj {
    vtbl: *const (),
}
unsafe impl Sync for Obj {}

static LIST: Obj = Obj {
    vtbl: &VT as *const _ as *const (),
};

static VT: VTable<22> = VTable([
    list_qi as *const (),
    add_ref as *const (),
    release as *const (),
    ok as *const (),                 // HrInit
    ok as *const (),                 // AddTab
    ok as *const (),                 // DeleteTab
    ok as *const (),                 // ActivateTab
    ok as *const (),                 // SetActiveAlt
    ok as *const (),                 // MarkFullscreenWindow (ITaskbarList2)
    set_progress_value as *const (), // ITaskbarList3
    set_progress_state as *const (),
    ok as *const (), // RegisterTab
    ok as *const (), // UnregisterTab
    ok as *const (), // SetTabOrder
    ok as *const (), // SetTabActive
    ok as *const (), // ThumbBarAddButtons
    ok as *const (), // ThumbBarUpdateButtons
    ok as *const (), // ThumbBarSetImageList
    ok as *const (), // SetOverlayIcon
    ok as *const (), // SetThumbnailTooltip
    ok as *const (), // SetThumbnailClip
    ok as *const (), // SetTabProperties (ITaskbarList4)
]);

unsafe extern "win64" fn list_qi(_this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let iid = *iid;
    if [
        IID_IUNKNOWN,
        IID_ITASKBAR_LIST,
        IID_ITASKBAR_LIST2,
        IID_ITASKBAR_LIST3,
        IID_ITASKBAR_LIST4,
    ]
    .contains(&iid)
    {
        *out = &LIST as *const Obj as *mut c_void;
        return S_OK;
    }
    E_NOINTERFACE
}

extern "win64" fn add_ref(_this: usize) -> u32 {
    2
}

extern "win64" fn release(_this: usize) -> u32 {
    1
}

/// Méthodes acceptées sans effet visible. En x64, l'appelant nettoie la
/// pile : une seule fonction convient à toutes les signatures.
extern "win64" fn ok(_this: usize) -> i32 {
    S_OK
}

extern "win64" fn set_progress_value(_this: usize, hwnd: usize, done: u64, total: u64) -> i32 {
    if hwnd == 0 {
        return E_INVALIDARG;
    }
    let permille = if total == 0 {
        0
    } else {
        (done.min(total) as u128 * 1000 / total as u128) as u32
    };
    let mut table = table();
    let entry = table.entry(hwnd).or_default();
    entry.permille = permille;
    // Sous Windows, une valeur sans état passe en TBPF_NORMAL.
    if entry.state == TBPF_NOPROGRESS || entry.state == TBPF_INDETERMINATE {
        entry.state = 2;
    }
    S_OK
}

extern "win64" fn set_progress_state(_this: usize, hwnd: usize, state: u32) -> i32 {
    if hwnd == 0 {
        return E_INVALIDARG;
    }
    let mut table = table();
    if state == TBPF_NOPROGRESS {
        table.remove(&hwnd);
    } else {
        table.entry(hwnd).or_default().state = state;
    }
    S_OK
}

/// CoCreateInstance de CLSID_TaskbarList.
///
/// # Safety
/// `out` doit être valide en écriture.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut c_void,
) -> Option<i32> {
    // Même disposition (16 octets) que le Guid d'ole32.
    let clsid = &*(clsid as *const _ as *const Guid);
    if *clsid != CLSID_TASKBAR_LIST {
        return None;
    }
    let iid = iid as *const _ as *const Guid;
    Some(list_qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(x: &Guid) -> &super::super::ole32::Guid {
        unsafe { &*(x as *const Guid as *const super::super::ole32::Guid) }
    }

    #[test]
    fn taskbar_list_cree_et_progression_retenue() {
        unsafe {
            for iid in [
                &IID_ITASKBAR_LIST,
                &IID_ITASKBAR_LIST2,
                &IID_ITASKBAR_LIST3,
                &IID_ITASKBAR_LIST4,
            ] {
                let mut obj: *mut c_void = std::ptr::null_mut();
                assert_eq!(
                    create_builtin(g(&CLSID_TASKBAR_LIST), g(iid), &mut obj),
                    Some(S_OK)
                );
                assert!(!obj.is_null());
            }
            let mut obj: *mut c_void = std::ptr::null_mut();
            create_builtin(g(&CLSID_TASKBAR_LIST), g(&IID_ITASKBAR_LIST3), &mut obj);
            let vt = *(obj as *const *const usize);
            let hr_init: extern "win64" fn(*mut c_void) -> i32 = std::mem::transmute(*vt.add(3));
            assert_eq!(hr_init(obj), S_OK);
            let value: extern "win64" fn(*mut c_void, usize, u64, u64) -> i32 =
                std::mem::transmute(*vt.add(9));
            let state: extern "win64" fn(*mut c_void, usize, u32) -> i32 =
                std::mem::transmute(*vt.add(10));
            let hwnd = 0x1434_0001;
            assert_eq!(value(obj, hwnd, 3, 4), S_OK);
            assert_eq!(
                progress(hwnd),
                Some(Progress {
                    state: 2,
                    permille: 750
                })
            );
            assert_eq!(state(obj, hwnd, 4), S_OK); // TBPF_ERROR
            assert_eq!(progress(hwnd).map(|p| p.state), Some(4));
            assert_eq!(state(obj, hwnd, TBPF_NOPROGRESS), S_OK);
            assert_eq!(progress(hwnd), None);
            // Une autre classe n'est pas prise.
            let mut other: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                create_builtin(g(&IID_ITASKBAR_LIST), g(&IID_IUNKNOWN), &mut other),
                None
            );
        }
    }
}
