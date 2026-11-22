//! Contrat api-ms-win-oobe-notification-l1-1-0 (kernel32) : l'accueil de
//! Windows (OOBE) est toujours terminé quand un programme tourne sous
//! Winlux, puisque le compte existe déjà (Refs #2902).
//!
//! Source : pages sdk-api nf-oobenotification-oobecomplete,
//! -registerwaituntiloobecompleted et -unregisterwaituntiloobecompleted
//! (DLL : Kernel32.dll) ; ERROR_INVALID_STATE : winerror.h de mingw-w64.

const ERROR_INVALID_STATE: u32 = 5023;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;

/// `OOBEComplete(BOOL*)` : TRUE, l'accueil est terminé.
pub unsafe extern "win64" fn oobe_complete(out: *mut i32) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    out.write_unaligned(1);
    1
}

/// `RegisterWaitUntilOOBECompleted(rappel, contexte, &handle)` : la page
/// dit que FALSE avec ERROR_INVALID_STATE signifie « OOBE déjà terminé,
/// inutile de s'inscrire » ; c'est toujours le cas ici.
pub unsafe extern "win64" fn register_wait_until_oobe_completed(
    _callback: usize,
    _context: usize,
    _wait: usize,
) -> i32 {
    crate::process::set_last_error(ERROR_INVALID_STATE);
    0
}

/// `UnregisterWaitUntilOOBECompleted(handle)` : aucune inscription n'est
/// jamais délivrée. Inférence : ERROR_INVALID_HANDLE, la page ne nommant
/// pas l'erreur.
pub extern "win64" fn unregister_wait_until_oobe_completed(_wait: usize) -> i32 {
    crate::process::set_last_error(ERROR_INVALID_HANDLE);
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "OOBEComplete" => fn oobe_complete,
        "RegisterWaitUntilOOBECompleted" => fn register_wait_until_oobe_completed,
        "UnregisterWaitUntilOOBECompleted" => fn unregister_wait_until_oobe_completed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accueil_toujours_termine_2902() {
        let mut v = 0i32;
        unsafe {
            assert_eq!(oobe_complete(&mut v), 1);
            assert_eq!(v, 1);
            assert_eq!(oobe_complete(std::ptr::null_mut()), 0);
            assert_eq!(register_wait_until_oobe_completed(0, 0, 0), 0);
        }
        assert_eq!(unregister_wait_until_oobe_completed(0), 0);
    }
}
