//! DirectShow : messages d’erreur uniquement. Aucune fausse interface COM.

// ---------- AMGetErrorTextA/W ----------
pub unsafe extern "win64" fn am_get_error_text_a(hr: i32, buf: *mut u8, max_len: u32) -> u32 {
    let msg = format!("DirectShow error 0x{:08X}", hr as u32);
    crate::dlls::put_a(&msg, buf, max_len)
}

pub unsafe extern "win64" fn am_get_error_text_w(hr: i32, buf: *mut u16, max_len: u32) -> u32 {
    let msg = format!("DirectShow error 0x{:08X}", hr as u32);
    crate::dlls::put_w(&msg, buf, max_len)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "AMGetErrorTextA" => fn am_get_error_text_a,
        "AMGetErrorTextW" => fn am_get_error_text_w,
    }
}
