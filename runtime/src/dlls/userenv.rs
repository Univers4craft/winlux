//! Stubs pour userenv.dll — profils utilisateur et blocs d'environnement.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn GetUserProfileDirectoryW(
    _token: usize,
    buf: *mut u16,
    size: *mut u32,
) -> i32 {
    let home = "/home/user";
    let wide: Vec<u16> = home.encode_utf16().chain(std::iter::once(0)).collect();
    if size.is_null() {
        return 0;
    }
    if buf.is_null() || (*size as usize) < wide.len() {
        *size = wide.len() as u32;
        crate::process::set_last_error(122); // ERROR_INSUFFICIENT_BUFFER
        return 0;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
    *size = wide.len() as u32;
    1
}
pub unsafe extern "win64" fn CreateEnvironmentBlock(
    env: *mut usize,
    _token: usize,
    _inherit: i32,
) -> i32 {
    if !env.is_null() {
        *env = 0;
    }
    1
}
pub extern "win64" fn DestroyEnvironmentBlock(_env: usize) -> i32 {
    1
}
pub unsafe extern "win64" fn GetUserProfileDirectoryA(
    _token: usize,
    buf: *mut u8,
    size: *mut u32,
) -> i32 {
    let home = b"/home/user\0";
    if size.is_null() {
        return 0;
    }
    if buf.is_null() || (*size as usize) < home.len() {
        *size = home.len() as u32;
        crate::process::set_last_error(122);
        return 0;
    }
    std::ptr::copy_nonoverlapping(home.as_ptr(), buf, home.len());
    *size = home.len() as u32;
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetUserProfileDirectoryW" => fn GetUserProfileDirectoryW,
        "GetUserProfileDirectoryA" => fn GetUserProfileDirectoryA,
        "CreateEnvironmentBlock" => fn CreateEnvironmentBlock,
        "DestroyEnvironmentBlock" => fn DestroyEnvironmentBlock,
    }
}
