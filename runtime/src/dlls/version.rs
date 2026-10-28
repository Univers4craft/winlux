pub unsafe extern "win64" fn get_file_version_info_size_a(
    _file: *const u8,
    handle: *mut u32,
) -> u32 {
    if !handle.is_null() {
        *handle = 0;
    }
    0
}
pub unsafe extern "win64" fn get_file_version_info_a(
    _file: *const u8,
    _handle: u32,
    _len: u32,
    _data: *mut u8,
) -> i32 {
    0
}
pub unsafe extern "win64" fn ver_query_value_a(
    _block: *const u8,
    _sub: *const u8,
    value: *mut *mut u8,
    len: *mut u32,
) -> i32 {
    if !value.is_null() {
        *value = std::ptr::null_mut();
    }
    if !len.is_null() {
        *len = 0;
    }
    0
}
pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "GetFileVersionInfoSizeA"=>fn get_file_version_info_size_a,"GetFileVersionInfoA"=>fn get_file_version_info_a,"VerQueryValueA"=>fn ver_query_value_a
    }
}
