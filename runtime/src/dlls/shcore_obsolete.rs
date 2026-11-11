//! api-ms-win-shcore-obsolete-l1-1-0 (#1604) : CommandLineToArgvW, SHStrDupA
//! et SHStrDupW, servis avec shcore. L'installeur d'iCUE importe
//! CommandLineToArgvW par ce jeu d'API.
//!
//! Sources : documentation Microsoft Learn de CommandLineToArgvW et de
//! SHStrDupA/W ; liste des noms de ce jeu d'API d'après la bibliothèque
//! d'importation publique OneCore_apiset.Lib du SDK Windows (noms seulement).
#![allow(non_snake_case)]

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;

pub unsafe extern "win64" fn CommandLineToArgvW(
    line: *const u16,
    count: *mut i32,
) -> *mut *mut u16 {
    super::shell32::command_line_to_argv_w(line, count)
}

/// Copie UTF-16 allouée par CoTaskMemAlloc, rendue dans `out`.
unsafe fn dup(text: &[u16], out: *mut usize) -> i32 {
    let p = super::ole32::co_task_mem_alloc((text.len() + 1) * 2) as *mut u16;
    if p.is_null() {
        crate::wow32::write_ptr(out as usize, 0);
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
    *p.add(text.len()) = 0;
    crate::wow32::write_ptr(out as usize, p as usize);
    S_OK
}

/// SHStrDupW : copie d'une chaîne large par CoTaskMemAlloc.
pub unsafe extern "win64" fn SHStrDupW(src: *const u16, out: *mut usize) -> i32 {
    if src.is_null() || out.is_null() {
        if !out.is_null() {
            crate::wow32::write_ptr(out as usize, 0);
        }
        return E_INVALIDARG;
    }
    let len = (0..).take_while(|&i| *src.add(i) != 0).count();
    dup(std::slice::from_raw_parts(src, len), out)
}

/// SHStrDupA : chaîne ANSI convertie en large, puis copiée.
pub unsafe extern "win64" fn SHStrDupA(src: *const u8, out: *mut usize) -> i32 {
    if src.is_null() || out.is_null() {
        if !out.is_null() {
            crate::wow32::write_ptr(out as usize, 0);
        }
        return E_INVALIDARG;
    }
    let wide: Vec<u16> = super::cstr(src).encode_utf16().collect();
    dup(&wide, out)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CommandLineToArgvW" => fn CommandLineToArgvW,
        "SHStrDupA" => fn SHStrDupA,
        "SHStrDupW" => fn SHStrDupW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copie_et_decoupage() {
        unsafe {
            let src: Vec<u16> = "Été\0".encode_utf16().collect();
            let mut p = 0usize;
            assert_eq!(SHStrDupW(src.as_ptr(), &mut p), S_OK);
            assert_eq!(crate::dlls::wstr(p as *const u16), "Été");
            crate::dlls::ole32::co_task_mem_free(p as *mut _);
            assert_eq!(SHStrDupA(c"abc".as_ptr().cast(), &mut p), S_OK);
            assert_eq!(crate::dlls::wstr(p as *const u16), "abc");
            crate::dlls::ole32::co_task_mem_free(p as *mut _);
            assert_eq!(SHStrDupW(std::ptr::null(), &mut p), E_INVALIDARG);
            assert_eq!(p, 0);
            let line: Vec<u16> = "setup.exe /quiet \"a b\"\0".encode_utf16().collect();
            let mut n = 0;
            let argv = CommandLineToArgvW(line.as_ptr(), &mut n);
            assert_eq!(n, 3);
            assert_eq!(crate::dlls::wstr(*argv.add(2)), "a b");
        }
    }

    #[test]
    fn jeux_api_de_l_installeur_icue() {
        assert_eq!(
            crate::dlls::normalize("api-ms-win-ntuser-sysparams-l1-1-0.dll"),
            "user32"
        );
        assert_eq!(
            crate::dlls::normalize("api-ms-win-shcore-obsolete-l1-1-0.dll"),
            "shcore"
        );
    }
}
