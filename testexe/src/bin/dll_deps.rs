//! #691 : une DLL amenée seulement comme dépendance d'un LoadLibrary reçoit
//! son DllMain, avant la DLL qui l'importe. Audacity plantait au premier
//! lancement parce que les objets globaux de lib-cloud-audiocom.dll, chargée
//! par mod-cloud-audiocom.dll, n'étaient jamais construits.

#![no_std]
#![no_main]

use winlux_testexe::*;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        check!(
            GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
            "testdll.dll pas encore chargée"
        );
        let dll = LoadLibraryA(c"testdll2.dll".as_ptr());
        check!(!dll.is_null(), "LoadLibrary d'une DLL qui en importe une autre");
        let f = if dll.is_null() {
            core::ptr::null()
        } else {
            GetProcAddress(dll, c"winlux_dep_attach_kind".as_ptr())
        };
        check!(!f.is_null(), "GetProcAddress(winlux_dep_attach_kind)");
        if !f.is_null() {
            let kind: extern "system" fn() -> u32 = core::mem::transmute(f);
            check!(
                kind() == 2,
                "DllMain de la dépendance appelé avant celui de la DLL chargée"
            );
        }
        check!(
            !GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
            "la dépendance est dans la liste des modules"
        );
        unload_and_reload(dll);
    }
    finish()
}

type U32Fn = extern "system" fn() -> u32;

unsafe fn export(dll: Handle, name: &core::ffi::CStr) -> Option<U32Fn> {
    let f = GetProcAddress(dll, name.as_ptr());
    (!f.is_null()).then(|| core::mem::transmute::<_, U32Fn>(f))
}

/// #24 : FreeLibrary décharge vraiment la DLL (et ses dépendances qu'elle
/// seule tenait) ; la recharger repart d'une image neuve et rappelle DllMain.
unsafe fn unload_and_reload(dll2: Handle) {
    let direct = LoadLibraryA(c"testdll.dll".as_ptr());
    check!(!direct.is_null(), "LoadLibrary de la dépendance déjà chargée");
    check!(FreeLibrary(dll2) != 0, "FreeLibrary(testdll2.dll)");
    check!(
        GetModuleHandleA(c"testdll2.dll".as_ptr()).is_null(),
        "testdll2.dll retirée de la liste des modules"
    );
    check!(
        !GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
        "la dépendance reste chargée tant que le programme la tient"
    );
    let set_mark = GetProcAddress(direct, c"winlux_set_mark".as_ptr());
    check!(!set_mark.is_null(), "GetProcAddress(winlux_set_mark)");
    if set_mark.is_null() {
        return;
    }
    let set_mark: extern "system" fn(u32) = core::mem::transmute(set_mark);
    set_mark(7);
    check!(
        export(direct, c"winlux_mark").is_some_and(|f| f() == 7),
        "marque posée dans l'image chargée"
    );
    check!(FreeLibrary(direct) != 0, "FreeLibrary(testdll.dll)");
    check!(
        GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
        "dernier FreeLibrary : la DLL n'est plus chargée"
    );
    let again = LoadLibraryA(c"testdll.dll".as_ptr());
    check!(!again.is_null(), "LoadLibrary après déchargement");
    if again.is_null() {
        return;
    }
    check!(
        export(again, c"winlux_mark").is_some_and(|f| f() == 0),
        "image neuve après rechargement (marque remise à zéro)"
    );
    check!(
        export(again, c"winlux_attach_count").is_some_and(|f| f() == 1),
        "DllMain(DLL_PROCESS_ATTACH) rappelé sur l'image neuve"
    );
    check!(FreeLibrary(again) != 0, "FreeLibrary après rechargement");
    // Dépendance amenée seulement par testdll2.dll : partie avec elle.
    let dll2 = LoadLibraryA(c"testdll2.dll".as_ptr());
    check!(!dll2.is_null(), "LoadLibrary(testdll2.dll) une seconde fois");
    check!(
        export(dll2, c"winlux_dep_attach_kind").is_some_and(|f| f() == 2),
        "dépendance rechargée et initialisée avant la DLL"
    );
    check!(FreeLibrary(dll2) != 0, "FreeLibrary(testdll2.dll) une seconde fois");
    check!(
        GetModuleHandleA(c"testdll2.dll".as_ptr()).is_null()
            && GetModuleHandleA(c"testdll.dll".as_ptr()).is_null(),
        "la DLL et sa dépendance sont toutes deux déchargées"
    );
}
