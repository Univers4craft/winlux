//! #2230 : un programme Windows ordinaire enregistre une tâche par
//! `ITaskService` (NewTask, put_XmlText, RegisterTaskDefinition), la relit
//! (GetTask) puis la supprime (DeleteTask), sans mot de passe ni élévation.
//! Lancé par un compte ordinaire, la tâche va dans son magasin
//! (`~/.local/share/winlux/tasks`, #1824) ; par root, dans celui du système.
//!
//! `taches_compte.exe /garder` laisse la tâche « \winlux-test-compte » en
//! place pour la vérifier ensuite (`winlux --tasks list`, dossier du compte).
//! Interfaces et ordre des méthodes : taskschd.h de MinGW-w64 et
//! documentation publique de l'API Planificateur de tâches 2.0.

#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr::null_mut;
use winlux_testexe::*;

#[link(name = "ole32", kind = "raw-dylib")]
extern "system" {
    fn CoCreateInstance(
        clsid: *const Guid,
        outer: *mut c_void,
        context: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
}

#[link(name = "oleaut32", kind = "raw-dylib")]
extern "system" {
    fn SysAllocString(s: *const u16) -> *mut u16;
    fn SysFreeString(s: *mut u16);
}

// {0F87369F-A4E5-4CFC-BD3E-73E6154572DD}
const CLSID_TASK_SCHEDULER: Guid = Guid {
    data1: 0x0F87_369F,
    data2: 0xA4E5,
    data3: 0x4CFC,
    data4: [0xBD, 0x3E, 0x73, 0xE6, 0x15, 0x45, 0x72, 0xDD],
};
// {2FABA4C7-4DA9-4013-9697-20CC3FD40F85}
const IID_ITASK_SERVICE: Guid = Guid {
    data1: 0x2FAB_A4C7,
    data2: 0x4DA9,
    data3: 0x4013,
    data4: [0x96, 0x97, 0x20, 0xCC, 0x3F, 0xD4, 0x0F, 0x85],
};

// Rangs dans les tables de méthodes (IUnknown 0-2, IDispatch 3-6).
const RELEASE: usize = 2;
const SERVICE_GET_FOLDER: usize = 7;
const SERVICE_NEW_TASK: usize = 9;
const SERVICE_CONNECT: usize = 10;
const FOLDER_GET_TASK: usize = 13;
const FOLDER_DELETE_TASK: usize = 15;
const FOLDER_REGISTER_DEFINITION: usize = 17;
const DEFINITION_PUT_XML: usize = 20;
const REGISTERED_GET_NAME: usize = 7;

const TASK_CREATE_OR_UPDATE: i32 = 6;
const TASK_LOGON_INTERACTIVE_TOKEN: i32 = 3;

const NAME: &[u8] = b"winlux-test-compte";
// Aucun déclencheur : la tâche ne part jamais d'elle-même.
const XML: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-16\"?>\
<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\
<RegistrationInfo><Description>Test Winlux du magasin par compte</Description></RegistrationInfo>\
<Settings><Enabled>true</Enabled></Settings>\
<Actions><Exec><Command>/bin/true</Command></Exec></Actions></Task>";

/// VARIANT vide (VT_EMPTY), passé par adresse comme le veut l'ABI x64.
type Variant = [u64; 3];

unsafe fn slot(obj: *mut c_void, index: usize) -> usize {
    *(*(obj as *const *const usize)).add(index)
}

unsafe fn release(obj: *mut c_void) {
    if !obj.is_null() {
        let f: extern "system" fn(*mut c_void) -> u32 = core::mem::transmute(slot(obj, RELEASE));
        f(obj);
    }
}

/// BSTR depuis de l'ASCII.
unsafe fn bstr(s: &[u8]) -> *mut u16 {
    let mut w = [0u16; 1024];
    for (i, &c) in s.iter().take(w.len() - 1).enumerate() {
        w[i] = c as u16;
    }
    SysAllocString(w.as_ptr())
}

unsafe fn eq_ascii(w: *const u16, s: &[u8]) -> bool {
    if w.is_null() {
        return false;
    }
    for (i, &c) in s.iter().enumerate() {
        if *w.add(i) != c as u16 {
            return false;
        }
    }
    *w.add(s.len()) == 0
}

/// Vrai si `/garder` figure sur la ligne de commande.
unsafe fn keep() -> bool {
    let line = GetCommandLineA() as *const u8;
    let mut i = 0;
    while *line.add(i) != 0 {
        let rest = core::slice::from_raw_parts(line.add(i), 7.min(strlen(line.add(i) as _)));
        if rest == b"/garder" {
            return true;
        }
        i += 1;
    }
    false
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        CoInitializeEx(null_mut(), 0);
        let mut svc: *mut c_void = null_mut();
        let hr = CoCreateInstance(
            &CLSID_TASK_SCHEDULER,
            null_mut(),
            1,
            &IID_ITASK_SERVICE,
            &mut svc,
        );
        check!(hr == 0 && !svc.is_null(), "CoCreateInstance(TaskScheduler)");
        if svc.is_null() {
            finish();
        }
        let empty: Variant = [0; 3];
        let connect: extern "system" fn(
            *mut c_void,
            *const Variant,
            *const Variant,
            *const Variant,
            *const Variant,
        ) -> i32 = core::mem::transmute(slot(svc, SERVICE_CONNECT));
        check!(
            connect(svc, &empty, &empty, &empty, &empty) == 0,
            "ITaskService::Connect"
        );

        let root = bstr(b"\\");
        let mut folder: *mut c_void = null_mut();
        let get_folder: extern "system" fn(*mut c_void, *mut u16, *mut *mut c_void) -> i32 =
            core::mem::transmute(slot(svc, SERVICE_GET_FOLDER));
        check!(
            get_folder(svc, root, &mut folder) == 0 && !folder.is_null(),
            "GetFolder(\\)"
        );

        let mut def: *mut c_void = null_mut();
        let new_task: extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32 =
            core::mem::transmute(slot(svc, SERVICE_NEW_TASK));
        check!(new_task(svc, 0, &mut def) == 0 && !def.is_null(), "NewTask");
        if folder.is_null() || def.is_null() {
            finish();
        }
        let xml = bstr(XML);
        let put_xml: extern "system" fn(*mut c_void, *mut u16) -> i32 =
            core::mem::transmute(slot(def, DEFINITION_PUT_XML));
        check!(put_xml(def, xml) == 0, "ITaskDefinition::put_XmlText");

        // Ni compte ni mot de passe : la tâche est celle de l'appelant.
        let name = bstr(NAME);
        let mut reg: *mut c_void = null_mut();
        let register: extern "system" fn(
            *mut c_void,
            *mut u16,
            *mut c_void,
            i32,
            *const Variant,
            *const Variant,
            i32,
            *const Variant,
            *mut *mut c_void,
        ) -> i32 = core::mem::transmute(slot(folder, FOLDER_REGISTER_DEFINITION));
        let hr = register(
            folder,
            name,
            def,
            TASK_CREATE_OR_UPDATE,
            &empty,
            &empty,
            TASK_LOGON_INTERACTIVE_TOKEN,
            &empty,
            &mut reg,
        );
        check!(
            hr == 0 && !reg.is_null(),
            "RegisterTaskDefinition sans mot de passe"
        );
        if hr != 0 {
            out(b"  HRESULT ");
            let mut d = [0u8; 8];
            for (i, b) in d.iter_mut().enumerate() {
                *b = b"0123456789abcdef"[((hr as u32) >> (28 - 4 * i)) as usize & 15];
            }
            out(&d);
            out(b"\n");
        }
        release(reg);

        // Relue par GetTask, sous son nom.
        let mut got: *mut c_void = null_mut();
        let get_task: extern "system" fn(*mut c_void, *mut u16, *mut *mut c_void) -> i32 =
            core::mem::transmute(slot(folder, FOLDER_GET_TASK));
        let found = get_task(folder, name, &mut got) == 0 && !got.is_null();
        let mut same = false;
        if found {
            let mut n: *mut u16 = null_mut();
            let get_name: extern "system" fn(*mut c_void, *mut *mut u16) -> i32 =
                core::mem::transmute(slot(got, REGISTERED_GET_NAME));
            same = get_name(got, &mut n) == 0 && eq_ascii(n, NAME);
            SysFreeString(n);
        }
        check!(found && same, "GetTask relit la tâche enregistrée");
        release(got);

        if keep() {
            out(b"  tache \\winlux-test-compte gardee\n");
        } else {
            let delete: extern "system" fn(*mut c_void, *mut u16, i32) -> i32 =
                core::mem::transmute(slot(folder, FOLDER_DELETE_TASK));
            check!(delete(folder, name, 0) == 0, "DeleteTask");
            let mut gone: *mut c_void = null_mut();
            check!(
                get_task(folder, name, &mut gone) != 0,
                "la tâche supprimée n'est plus trouvée"
            );
            release(gone);
        }
        SysFreeString(xml);
        SysFreeString(name);
        SysFreeString(root);
        release(def);
        release(folder);
        release(svc);
        finish();
    }
}
