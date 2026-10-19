use std::ffi::c_void;

pub unsafe extern "win64" fn rtl_get_current_peb() -> usize { crate::process::peb() }
pub extern "win64" fn rtl_get_current_teb() -> usize { crate::process::teb() }
pub extern "win64" fn rtl_get_last_win32_error() -> u32 { crate::process::last_error() }
pub extern "win64" fn rtl_set_last_win32_error(code:u32) { crate::process::set_last_error(code); }
pub unsafe extern "win64" fn rtl_get_version(info:*mut u16)->i32 {
    if info.is_null() { return 0xC000_000Du32 as i32; }
    if crate::dlls::kernel32::get_version_ex_w(info)==0 { 0xC000_000Du32 as i32 } else { 0 }
}
pub extern "win64" fn rtl_nt_status_to_dos_error(status:i32)->u32 {
    match status as u32 {
        0 => 0, 0xC000_0005 => 998, 0xC000_0008 => 6, 0xC000_0022 => 5,
        0xC000_0034 => 2, 0xC000_0094 => 534, 0xC000_0135 => 126,
        _ => 317,
    }
}
pub unsafe extern "win64" fn rtl_allocate_heap(_heap:usize,flags:u32,size:usize)->*mut c_void { crate::dlls::kernel32::heap_alloc(0,flags,size) }
pub unsafe extern "win64" fn rtl_free_heap(_heap:usize,_flags:u32,p:*mut c_void)->u8 { crate::dlls::kernel32::heap_free(0,0,p) as u8 }
pub unsafe extern "win64" fn rtl_reallocate_heap(_heap:usize,flags:u32,p:*mut c_void,size:usize)->*mut c_void { crate::dlls::kernel32::heap_realloc(0,flags,p,size) }
pub unsafe extern "win64" fn rtl_init_unicode_string(dst:*mut u8,src:*const u16) {
    if dst.is_null(){return;}let (len,max,buf)=if src.is_null(){(0u16,0u16,std::ptr::null())}else{let n=crate::printf::wide_len(src).saturating_mul(2).min((u16::MAX-2)as usize)as u16;(n,n.saturating_add(2),src)};
    std::ptr::write_unaligned(dst as *mut u16,len);std::ptr::write_unaligned(dst.add(2)as *mut u16,max);std::ptr::write_unaligned(dst.add(8)as *mut *const u16,buf);
}
pub unsafe extern "win64" fn rtl_random_ex(seed:*mut u32)->u32 {
    if seed.is_null(){return 0;}let mut x=*seed;x^=x<<13;x^=x>>17;x^=x<<5;if x==0{x=0x9e37_79b9;}*seed=x;x&0x7fff_ffff
}

pub fn exports() -> crate::dlls::Exports { crate::exports! {
    "RtlGetCurrentPeb"=>fn rtl_get_current_peb,"RtlGetCurrentTeb"=>fn rtl_get_current_teb,
    "RtlGetLastWin32Error"=>fn rtl_get_last_win32_error,"RtlSetLastWin32Error"=>fn rtl_set_last_win32_error,
    "RtlGetVersion"=>fn rtl_get_version,"RtlNtStatusToDosError"=>fn rtl_nt_status_to_dos_error,
    "RtlAllocateHeap"=>fn rtl_allocate_heap,"RtlFreeHeap"=>fn rtl_free_heap,"RtlReAllocateHeap"=>fn rtl_reallocate_heap,
    "RtlInitUnicodeString"=>fn rtl_init_unicode_string,"RtlRandomEx"=>fn rtl_random_ex
} }
