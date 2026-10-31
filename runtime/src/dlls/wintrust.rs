//! Stubs pour wintrust.dll — vérification de signatures Authenticode.
//! On fait confiance à tout pour ne pas bloquer le lancement des apps.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn WinVerifyTrust(
    _hwnd: usize, _action: usize, _data: usize,
) -> i32 { 0 }
pub unsafe extern "win64" fn CryptCATAdminAcquireContext(
    admin: *mut usize, _subsystem: usize, _flags: u32,
) -> i32 {
    if !admin.is_null() { *admin = 0; }
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinVerifyTrust" => fn WinVerifyTrust,
        "CryptCATAdminAcquireContext" => fn CryptCATAdminAcquireContext,
    }
}
