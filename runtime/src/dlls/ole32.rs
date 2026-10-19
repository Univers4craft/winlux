use std::ffi::c_void;

pub extern "win64" fn co_initialize_ex(_reserved: usize, _coinit: u32) -> i32 { 0 }
pub extern "win64" fn co_uninitialize() {}
pub unsafe extern "win64" fn co_task_mem_alloc(size: usize) -> *mut c_void { libc::malloc(size.max(1)) }
pub unsafe extern "win64" fn co_task_mem_free(p: *mut c_void) { libc::free(p) }
pub unsafe extern "win64" fn co_create_instance(_rclsid:*const c_void,_outer:usize,_ctx:u32,_riid:*const c_void,out:*mut *mut c_void)->i32{if !out.is_null(){*out=std::ptr::null_mut();} 0x80004002u32 as i32}
pub fn exports()->crate::dlls::Exports{crate::exports!{
 "CoInitializeEx"=>fn co_initialize_ex,"CoUninitialize"=>fn co_uninitialize,"CoTaskMemAlloc"=>fn co_task_mem_alloc,
 "CoTaskMemFree"=>fn co_task_mem_free,"CoCreateInstance"=>fn co_create_instance
}}
