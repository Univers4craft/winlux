use crate::thunks::VaList;
use crate::varargs_thunk;
use std::ffi::c_void;

unsafe fn do_printf(args:*const u64, out:*mut Vec<u8>) -> i32 { let mut v=VaList::new(args); let fmt=v.next_ptr::<u8>(); let b=crate::printf::format(std::slice::from_raw_parts(fmt,crate::printf::narrow_len(fmt)),&mut v,crate::printf::MSVCRT); (*out).extend_from_slice(&b); b.len() as i32 }
unsafe extern "win64" fn imp_printf(args:*const u64)->i32 { let mut b=Vec::new(); let n=do_printf(args,&mut b); let _=std::io::Write::write_all(&mut std::io::stdout(),&b); n }
unsafe extern "win64" fn imp_sprintf(args:*const u64)->i32 { let mut v=VaList::new(args); let dst=v.next_u64() as *mut u8; let fmt=v.next_ptr::<u8>(); let b=crate::printf::format(std::slice::from_raw_parts(fmt,crate::printf::narrow_len(fmt)),&mut v,crate::printf::MSVCRT); std::ptr::copy_nonoverlapping(b.as_ptr(),dst,b.len()); *dst.add(b.len())=0; b.len() as i32 }
varargs_thunk!(printf_thunk => imp_printf);
varargs_thunk!(sprintf_thunk => imp_sprintf);
pub unsafe extern "win64" fn puts(s:*const u8)->i32 { let b=std::slice::from_raw_parts(s,crate::printf::narrow_len(s)); let _=std::io::Write::write_all(&mut std::io::stdout(),b); println!(); (b.len()+1) as i32 }
pub unsafe extern "win64" fn malloc(n:usize)->*mut c_void { libc::malloc(n) }
pub unsafe extern "win64" fn free(p:*mut c_void){libc::free(p)}
pub unsafe extern "win64" fn strlen(p:*const u8)->usize{crate::printf::narrow_len(p)}
pub unsafe extern "win64" fn strcmp(a:*const u8,b:*const u8)->i32{std::ffi::CStr::from_ptr(a as *const i8).to_bytes().cmp(std::ffi::CStr::from_ptr(b as *const i8).to_bytes()) as i32}
pub unsafe extern "win64" fn memcpy(d:*mut c_void,s:*const c_void,n:usize)->*mut c_void{std::ptr::copy_nonoverlapping(s as *const u8,d as *mut u8,n);d}
pub unsafe extern "win64" fn memset(d:*mut c_void,c:i32,n:usize)->*mut c_void{std::ptr::write_bytes(d,c as u8,n);d}
pub unsafe extern "win64" fn memcmp(a:*const c_void,b:*const c_void,n:usize)->i32{std::slice::from_raw_parts(a as *const u8,n).cmp(std::slice::from_raw_parts(b as *const u8,n)) as i32}
pub unsafe extern "win64" fn memmove(d:*mut c_void,s:*const c_void,n:usize)->*mut c_void{std::ptr::copy(s as *const u8,d as *mut u8,n);d}
pub unsafe extern "win64" fn atoi(s:*const u8)->i32{crate::dlls::cstr(s).trim().parse().unwrap_or(0)}
pub fn exports()->crate::dlls::Exports{crate::exports!{"printf"=>fn printf_thunk,"sprintf"=>fn sprintf_thunk,"puts"=>fn puts,"malloc"=>fn malloc,"free"=>fn free,"strlen"=>fn strlen,"strcmp"=>fn strcmp,"memcpy"=>fn memcpy,"memset"=>fn memset,"memcmp"=>fn memcmp,"memmove"=>fn memmove,"atoi"=>fn atoi}}
pub fn ucrt_exports()->crate::dlls::Exports{crate::exports!{}}
