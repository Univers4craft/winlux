use std::ffi::c_void;
pub extern "win64" fn mf_startup(_version: u32, _flags: u32) -> i32 {
    0
}
pub extern "win64" fn mf_shutdown() {}
pub unsafe extern "win64" fn mf_create_attributes(out: *mut *mut c_void, _size: u32) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    0xC00D36B0u32 as i32
}
pub unsafe extern "win64" fn mf_shutdown_object(_obj: *mut c_void) -> i32 {
    0
}
pub fn exports() -> crate::dlls::Exports {
    crate::exports! {"MFStartup"=>fn mf_startup,"MFShutdown"=>fn mf_shutdown,"MFCreateAttributes"=>fn mf_create_attributes,"MFShutdownObject"=>fn mf_shutdown_object}
}
