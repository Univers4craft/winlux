use std::ffi::c_void;

/// BCryptPrimitives uses a SIZE_T length and a BOOL result, unlike BCrypt's
/// ULONG length / NTSTATUS API. Never report success with uninitialized bytes.
pub unsafe extern "win64" fn process_prng(buffer: *mut u8, size: usize) -> i32 {
    if size == 0 {
        return 1;
    }
    if buffer.is_null() || size > isize::MAX as usize {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut offset = 0;
    while offset < size {
        let count = libc::getrandom(
            buffer.add(offset).cast(),
            (size - offset).min(256 * 1024),
            0,
        );
        if count > 0 {
            offset += count as usize;
            continue;
        }
        if count < 0 && *libc::__errno_location() == libc::EINTR {
            continue;
        }
        // Older Linux kernels may not have getrandom. /dev/urandom is also a
        // system CSPRNG; there is no predictable application RNG fallback.
        let remaining = std::slice::from_raw_parts_mut(buffer.add(offset), size - offset);
        if std::fs::File::open("/dev/urandom")
            .and_then(|mut file| {
                use std::io::Read;
                file.read_exact(remaining)
            })
            .is_ok()
        {
            return 1;
        }
        crate::process::set_last_error(31);
        return 0;
    }
    1
}

pub fn primitives_exports() -> crate::dlls::Exports {
    crate::exports! { "ProcessPrng" => fn process_prng }
}

pub unsafe extern "win64" fn bcrypt_gen_random(
    _alg: usize,
    buf: *mut u8,
    len: u32,
    _flags: u32,
) -> i32 {
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
    const STATUS_NOT_SUPPORTED: i32 = 0xC000_00BBu32 as i32;
    if buf.is_null() || _flags & !2 != 0 || (_flags & 2 != 0 && _alg != 0) {
        return STATUS_INVALID_PARAMETER;
    }
    if _flags & 2 == 0 && _alg != 1 {
        return STATUS_INVALID_PARAMETER;
    }
    let slice = std::slice::from_raw_parts_mut(buf, len as usize);
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        if f.read_exact(slice).is_ok() {
            return 0;
        }
    }
    STATUS_NOT_SUPPORTED
}
pub unsafe extern "win64" fn bcrypt_open_algorithm_provider(
    out: *mut usize,
    id: *const u16,
    _impl: *const u16,
    flags: u32,
) -> i32 {
    if out.is_null() || id.is_null() || flags != 0 {
        return 0xC000000Du32 as i32;
    }
    if crate::dlls::wstr(id) != "RNG" {
        *out = 0;
        return 0xC0000002u32 as i32;
    }
    *out = 1;
    0
}
pub extern "win64" fn bcrypt_close_algorithm_provider(h: usize, _flags: u32) -> i32 {
    if h == 1 {
        0
    } else {
        0xC0000008u32 as i32
    }
}
pub unsafe extern "win64" fn bcrypt_destroy_key(_h: usize) -> i32 {
    0xC0000002u32 as i32
}
pub unsafe extern "win64" fn bcrypt_stub(_p: *mut c_void) -> i32 {
    0xC0000002u32 as i32
}
pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
     "BCryptGenRandom"=>fn bcrypt_gen_random,"BCryptOpenAlgorithmProvider"=>fn bcrypt_open_algorithm_provider,
     "BCryptCloseAlgorithmProvider"=>fn bcrypt_close_algorithm_provider,"BCryptDestroyKey"=>fn bcrypt_destroy_key,
     "BCryptEncrypt"=>fn bcrypt_stub,"BCryptDecrypt"=>fn bcrypt_stub
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_prng_uses_size_t_and_rejects_invalid_buffers() {
        let _teb = crate::process::test_teb();
        let mut first = [0u8; 64];
        let mut second = [0u8; 64];
        unsafe {
            assert_eq!(process_prng(first.as_mut_ptr(), first.len()), 1);
            assert_eq!(process_prng(second.as_mut_ptr(), second.len()), 1);
            assert_eq!(process_prng(std::ptr::null_mut(), 0), 1);
            assert_eq!(process_prng(std::ptr::null_mut(), 1), 0);
            assert_eq!(crate::process::last_error(), 87);
            assert_eq!(process_prng(first.as_mut_ptr(), usize::MAX), 0);
            assert_eq!(crate::process::last_error(), 87);
        }
        assert_ne!(first, second);
    }

    #[test]
    fn provider_rng_lit_une_source_systeme_et_refuse_les_buffers_invalides() {
        let algorithm: [u16; 4] = [b'R' as u16, b'N' as u16, b'G' as u16, 0];
        let mut h = 0usize;
        assert_eq!(
            unsafe {
                bcrypt_open_algorithm_provider(&mut h, algorithm.as_ptr(), std::ptr::null(), 0)
            },
            0
        );
        let mut bytes = [0u8; 32];
        assert_eq!(
            unsafe { bcrypt_gen_random(h, bytes.as_mut_ptr(), bytes.len() as u32, 0) },
            0
        );
        assert_eq!(
            unsafe { bcrypt_gen_random(0, std::ptr::null_mut(), 16, 2) },
            0xC000000Du32 as i32
        );
        assert_eq!(bcrypt_close_algorithm_provider(h, 0), 0);
    }
}
