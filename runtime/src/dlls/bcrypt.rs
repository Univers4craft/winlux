use std::ffi::c_void;

pub unsafe extern "win64" fn bcrypt_gen_random(_alg: usize, buf: *mut u8, len: u32, _flags: u32) -> i32 {
    const STATUS_INVALID_PARAMETER:i32=0xC000_000Du32 as i32;
    const STATUS_NOT_SUPPORTED:i32=0xC000_00BBu32 as i32;
    if buf.is_null() || _flags&!2!=0 || (_flags&2!=0&&_alg!=0) { return STATUS_INVALID_PARAMETER; }
    if _flags&2==0&&_alg!=1{return STATUS_INVALID_PARAMETER;}
    let slice=std::slice::from_raw_parts_mut(buf,len as usize);
    if let Ok(mut f)=std::fs::File::open("/dev/urandom") { use std::io::Read; if f.read_exact(slice).is_ok(){return 0;} }
    STATUS_NOT_SUPPORTED
}
pub unsafe extern "win64" fn bcrypt_open_algorithm_provider(out:*mut usize,id:*const u16,_impl:*const u16,flags:u32)->i32{if out.is_null()||id.is_null()||flags!=0{return 0xC000000Du32 as i32;}if crate::dlls::wstr(id)!="RNG"{*out=0;return 0xC0000002u32 as i32;}*out=1;0}
pub extern "win64" fn bcrypt_close_algorithm_provider(h:usize,_flags:u32)->i32{if h==1{0}else{0xC0000008u32 as i32}}
pub unsafe extern "win64" fn bcrypt_destroy_key(_h:usize)->i32{0xC0000002u32 as i32}
pub unsafe extern "win64" fn bcrypt_stub(_p:*mut c_void)->i32{0xC0000002u32 as i32}
pub fn exports()->crate::dlls::Exports{crate::exports!{
 "BCryptGenRandom"=>fn bcrypt_gen_random,"BCryptOpenAlgorithmProvider"=>fn bcrypt_open_algorithm_provider,
 "BCryptCloseAlgorithmProvider"=>fn bcrypt_close_algorithm_provider,"BCryptDestroyKey"=>fn bcrypt_destroy_key,
 "BCryptEncrypt"=>fn bcrypt_stub,"BCryptDecrypt"=>fn bcrypt_stub
}}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_rng_lit_une_source_systeme_et_refuse_les_buffers_invalides(){
        let algorithm:[u16;4]=[b'R' as u16,b'N' as u16,b'G' as u16,0];let mut h=0usize;
        assert_eq!(unsafe{bcrypt_open_algorithm_provider(&mut h,algorithm.as_ptr(),std::ptr::null(),0)},0);
        let mut bytes=[0u8;32];assert_eq!(unsafe{bcrypt_gen_random(h,bytes.as_mut_ptr(),bytes.len()as u32,0)},0);
        assert_eq!(unsafe{bcrypt_gen_random(0,std::ptr::null_mut(),16,2)},0xC000000Du32 as i32);
        assert_eq!(bcrypt_close_algorithm_provider(h,0),0);
    }
}
