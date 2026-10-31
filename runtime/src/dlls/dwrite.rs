//! Stubs pour dwrite.dll — rendu de texte DirectWrite.
//! Les apps Electron utilisent DirectWrite pour le rendu des polices.
//! On renvoie E_NOTIMPL pour forcer le fallback FreeType/Fontconfig.
#![allow(non_snake_case)]

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

pub unsafe extern "win64" fn DWriteCreateFactory(
    _type: u32,
    _iid: *const u8,
    _factory: *mut usize,
) -> i32 {
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "DWriteCreateFactory" => fn DWriteCreateFactory,
    }
}
