//! Export privé `WinluxQueryGpu` : le bureau lit le rapport GPU (texte
//! `clé=valeur` ASCII) sans dupliquer la détection dans chaque programme.

/// Copie le rapport dans `buf` (terminé par NUL si la place le permet) et
/// renvoie sa longueur complète, pour que l'appelant puisse agrandir son tampon.
pub unsafe extern "win64" fn winlux_query_gpu(buf: *mut u8, len: u32) -> u32 {
    let text = crate::gpu::report(&crate::gpu::probe(&crate::gpu::system_root()));
    let bytes = text.as_bytes();
    if !buf.is_null() && len > 0 {
        let n = bytes.len().min(len as usize - 1);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n);
        *buf.add(n) = 0;
    }
    bytes.len() as u32
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxQueryGpu" => fn winlux_query_gpu,
    }
}
